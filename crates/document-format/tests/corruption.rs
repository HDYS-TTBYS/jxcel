//! 破損の検出（タスク 8.4。要件 1.7, 4.2, 4.3, 4.4, 4.5, 5.3, 5.4, 7.4）。
//!
//! design「Integration Tests / 破損検出」の 1 項目であり、**`open`（実ファイル経路）**で
//! 6 種の破損シナリオが成立し、シナリオごとに対応するエラー変種が返ること、`Err` であり
//! モデル（[`OpenOutcome`](document_format::OpenOutcome)）が返らないこと（要件 5.4）、
//! 対象ファイルと作業ディレクトリが変更されないこと（要件 5.5）を固定する。
//!
//! # `tests/validate.rs` との役割分担（写しを作らないこと）
//!
//! `tests/validate.rs` は [`StructuralValidator`](document_format::parts::StructuralValidator)
//! **単体**の権威であり、規則ごとの報告文面・報告順・境界を目録（`PartInventory`）の
//! レベルで固定している。本ファイルはその写しではなく、**同じ違反が実ファイル経路の
//! `open` で観測できる**ことだけを足す。したがって:
//!
//! * 目録を直接組み立てない（`PartInventory` を import しない）。
//! * 規則の網羅（どの規則がどう報告されるか）を再検証しない。各シナリオは
//!   「変種が返る」「該当箇所が診断文脈に載る」ことを 1 回ずつ確かめるに留める。
//! * 標本・違反文書ビルダ・一時ディレクトリは `tests/common/mod.rs` の 1 箇所を使う
//!   （`tests/api.rs` と同じもの。第二の標本や比較規約を作らない）。
//!
//! # どのシナリオがどの層で検出されるか
//!
//! `open` は `std::fs::read` → `ContainerCodec::decode` → `parts::from_parts` の 1 経路を
//! 通る（`src/lib.rs` の `open`）。各シナリオの検出層は次のとおりである:
//!
//! | シナリオ | 検出層 | 変種 |
//! |----------|--------|------|
//! | ダイジェスト改竄 | `from_parts` の完全性照合（段 3） | `IntegrityMismatch { entry }` |
//! | 識別子の重複（`TypeDef` / `Row`） | `from_parts` の構造検証（段 5） | `DuplicateId { kind, id, occurrences }` |
//! | 宙吊りの型定義参照 | 同上 | `DanglingTypeRef { from, to }` |
//! | 宙吊りの添付参照 | 同上 | `DanglingAttachmentRef { from, id }` |
//! | スキーマ欠落 | 同上 | `MissingSchema { sheet }` |
//! | `document.json` 欠落 | `from_parts` のパート復号（段 4） | `MissingPart { name }` |
//! | `manifest.json` 欠落 | `ContainerCodec::decode` の集合構築（段 6。索引の解決） | `MissingPart { name }` |
//!
//! # 壊し方（ZIP の生バイトをパッチしないこと）
//!
//! 正しい文書を `to_parts` で**パート集合のレベル**へ落とし、対象エントリを書き換えて
//! `manifest.json` を組み直し、`ContainerCodec::encode` でコンテナへ戻す。ZIP の生バイトを
//! パッチすると、CRC の検証で `zip` が先に落ちてしまい、検証したい層のエラーにならない。
//! 索引の記録だけを古いまま残す（ダイジェスト改竄）か、索引も組み直す（構造違反）かを
//! シナリオごとに選ぶ。
//!
//! `manifest.json` 欠落だけは例外である: 公開 API は索引の無い集合を構築できない
//! （`DocumentParts::from_entries` が常に索引を要求し、形式バージョンを索引から読む）ため、
//! この 1 シナリオは `zip` クレートでアーカイブを直接組む。エントリ名は許可リストの形のまま、
//! 内容は正しいパートのバイト列を使い、**欠くのは索引だけ**にする。同じ組み立てで索引を
//! 残した入力が読めることを対照に取り、欠落だけが原因であることを示す。

mod common;

use std::fs;
use std::io::{Cursor, Write};

use document_format::container::writer::marker_bytes;
use document_format::container::ContainerCodec;
use document_format::parts::{to_parts, DocumentParts};
use document_format::{
    Document, DocumentError, DocumentFormatApi, EntryName, FormatVersion, IdKind, SchemaPart,
};
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use common::{
    api, document_with_dangling_type_ref, document_with_duplicate_type_def,
    document_with_unregistered_attachment, entries_of, rows, sample, snapshot,
    with_rebuilt_manifest, Scratch, SCHEMA_EMPTY, UNREGISTERED_ATTACHMENT_HEX,
};

/// 標本のスキーマが宣言・参照する型定義識別子（`common` の違反文書ビルダと同じ値）。
const TYPEDEF_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

/// エントリ集合をコンテナのバイト列へ符号化する。
fn encode(entries: Vec<(EntryName, Vec<u8>)>) -> Vec<u8> {
    let parts = DocumentParts::from_entries(entries).expect("標本の集合は妥当");
    ContainerCodec::encode(&parts).expect("符号化")
}

/// 索引（`manifest.json`）を実体から組み直したコンテナのバイト列を返す。
///
/// ボディのエントリを差し替えた後、索引の記録を実体に合わせる（ダイジェスト照合を通し、
/// 後段の検証だけを壊した入力を作る）。
fn rebuilt_with_manifest(version: FormatVersion, entries: Vec<(EntryName, Vec<u8>)>) -> Vec<u8> {
    encode(with_rebuilt_manifest(version, entries))
}

/// 標本のパート集合のエントリを、（エントリ名, バイト列）の列として返す。
fn entries_of_document(document: &Document) -> Vec<(EntryName, Vec<u8>)> {
    let parts = to_parts(document).expect("標本はパート集合へ取り出せる");
    entries_of(&parts)
}

/// 別々のシートに 1 行ずつ持つ文書（行識別子をシートをまたいで重複させる標本）。
///
/// 各シートは列 0 個・ルートスキーマ 1 つで、行は `add_row` が発行する一意な識別子を持つ
/// （モデル API では重複を作れないため、重複はエントリの本文を書き換えて作る）。
fn document_with_a_row_in_each_sheet() -> Document {
    let mut document = Document::new();
    for name in ["第一", "第二"] {
        let sheet = document.add_sheet(name);
        document
            .set_root_schema(sheet, SchemaPart::parse(SCHEMA_EMPTY).expect("標本は妥当"))
            .expect("標本のシートは実在する");
        document.add_row(sheet).expect("標本のシートは実在する");
    }
    document
}

/// `manifest.json` の有無だけを切り替えた ZIP を `zip` クレートで組む（モジュール docs 参照）。
///
/// `include_manifest` が `true` なら正しい索引を含み（対照。読み込める）、`false` なら索引を
/// 欠く（本シナリオ。`MissingPart { name: "manifest.json" }`）。
fn container_with_optional_manifest(document: &Document, include_manifest: bool) -> Vec<u8> {
    let parts = to_parts(document).expect("標本はパート集合へ取り出せる");
    let version = parts.format_version();
    let entries: Vec<(EntryName, Vec<u8>)> = entries_of(&parts)
        .into_iter()
        .filter(|(name, _)| include_manifest || *name != EntryName::Manifest)
        .collect();

    // エントリの並び順・圧縮方式は読み込み側の要求ではない（マーカーの位置も要求されない）。
    // ここでは無圧縮で書き、内容の正しさだけに注目する。
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(EntryName::Marker.to_string(), options)
        .expect("型マーカーのエントリを開始できる");
    writer.write_all(&marker_bytes(version)).expect("型マーカーを書ける");
    for (name, bytes) in entries {
        writer.start_file(name.to_string(), options).expect("エントリを開始できる");
        writer.write_all(&bytes).expect("エントリを書ける");
    }
    writer.finish().expect("ZIP を完成できる").into_inner()
}

/// 壊れた入力をファイルへ書き、`open` が期待する違反を `Err` で返すことを確かめる。
///
/// 3 つを同時に固定する（要件 5.4, 5.5）:
///
/// 1. **対応する変種**（`expected`。診断文脈に該当箇所が載ることまで含めて呼び出し元が判定する）
/// 2. **モデルが返らないこと**: `open` の戻り値は `Err` であり、`OpenOutcome` を受理しない
///    （`expect_err` が `Ok` を即座に落とす。戻り値型による構造的な保証を実測で固定する）
/// 3. **対象ファイルと作業ディレクトリが不変であること**: 呼び出しの前後で
///    [`snapshot`]（名前とバイト列）が完全に一致する。一時ファイルの残留も同時に捉える
fn assert_open_rejects(
    scratch: &Scratch,
    tag: &str,
    container: Vec<u8>,
    expected: impl FnOnce(&DocumentError) -> bool,
) {
    let path = scratch.file(&format!("{tag}.jxcel"));
    fs::write(&path, container).expect("壊れた入力を書き出せる");
    let before = snapshot(scratch.path());

    let error = api().open(&path).expect_err("破損入りのコンテナが読み込まれた");
    assert!(expected(&error), "{tag}: 期待した違反と違う: {error:?}");

    assert_eq!(before, snapshot(scratch.path()), "{tag}: 読み込みがファイルを変更した");
}

/// ダイジェスト改竄は、索引の記録と一致しないエントリ名つきで中止される（要件 5.3）。
///
/// 索引（`manifest.json`）は正しいまま、実体の行エントリだけを書き換える。`from_parts` の
/// 段 3（完全性照合）が、復号（段 4）より前に検出する。
#[test]
fn digest_tampering_is_reported_with_the_changed_entry() {
    let scratch = Scratch::new("digest");
    let parts = to_parts(&sample()).expect("標本はパート集合へ取り出せる");
    let mut entries = entries_of(&parts);
    let slot = entries
        .iter()
        .position(|(name, _)| matches!(name, EntryName::Rows { .. }))
        .expect("標本は行エントリを持つ");
    let entry_name = entries[slot].0.to_string();
    entries[slot].1.extend_from_slice(b" ");

    assert_open_rejects(&scratch, "digest", encode(entries), |error| {
        matches!(error, DocumentError::IntegrityMismatch { entry } if entry == &entry_name)
    });
}

/// 同一の `TypeDefId` を 2 回宣言するスキーマは、識別子と出現箇所つきで中止される（要件 4.3）。
///
/// `SchemaPart::parse` はペイロード内の重複宣言を排除しないため、モデルは構築できてしまう。
/// 検出は `from_parts` の段 5（構造検証）であり、ダイジェスト照合は通る。
#[test]
fn duplicate_type_def_ids_are_reported_with_their_occurrences() {
    let scratch = Scratch::new("duplicate_typedef");
    let container = encode(entries_of_document(&document_with_duplicate_type_def()));

    assert_open_rejects(&scratch, "duplicate_typedef", container, |error| match error {
        DocumentError::DuplicateId { kind: IdKind::TypeDef, id, occurrences } => {
            id == TYPEDEF_ID
                && occurrences.len() == 2
                && occurrences.iter().all(|occurrence| occurrence.starts_with("schemas/"))
        }
        _ => false,
    });
}

/// 同一の `RowId` を 2 つのシートの行が宣言する入力は、識別子と出現箇所つきで中止される
/// （要件 4.3）。
///
/// **同一エントリ内**の行識別子の重複はここでは作れない: `RowsCodec::decode` が 1 エントリの
/// 中で同じ `$id` を `InvalidContainer`（`duplicate row identifier`）として先に拒否するため、
/// 構造検証の段へ到達しない（この層の分担は `tests/rows_codec.rs` が固定する）。到達可能な
/// 経路は**シートをまたぐ**重複である（行エントリはシートごとに独立して復号されるため、
/// 各エントリ単体では妥当なまま識別子空間だけが衝突する）。ここでは 2 シートに 1 行ずつ
/// 持つ文書の 2 つ目のシートの行エントリの `$id` を 1 つ目へ書き換えて作る。
#[test]
fn duplicate_row_ids_are_reported_with_their_occurrences() {
    let scratch = Scratch::new("duplicate_row");
    let document = document_with_a_row_in_each_sheet();
    let sheets: Vec<String> =
        document.sheets().iter().map(|sheet| sheet.id().to_string()).collect();
    let ids: Vec<String> =
        rows(&document).iter().map(|sheet_rows| sheet_rows[0].0.clone()).collect();
    assert_eq!(2, ids.len(), "標本は 2 シートに 1 行ずつを持つ");
    assert_ne!(ids[0], ids[1], "標本の行識別子は元々異なる");

    let parts = to_parts(&document).expect("標本はパート集合へ取り出せる");
    let version = parts.format_version();
    let second_sheet = document.sheets()[1].id();
    let mut entries = entries_of(&parts);
    let slot = entries
        .iter()
        .position(|(name, _)| *name == EntryName::Rows { sheet: second_sheet })
        .expect("標本は対象シートの行エントリを持つ");
    let text = String::from_utf8(entries[slot].1.clone()).expect("行エントリは UTF-8");

    // 2 つ目のシートの行の `$id` を 1 つ目のシートの行の識別子へ置き換える。
    let tampered = text.replacen(
        &format!("\"$id\":\"{}\"", ids[1]),
        &format!("\"$id\":\"{}\"", ids[0]),
        1,
    );
    assert_ne!(text, tampered, "行識別子の書き換えが起きていない");
    entries[slot].1 = tampered.into_bytes();

    let expected_id = ids[0].clone();
    let expected_sheets = sheets;
    assert_open_rejects(
        &scratch,
        "duplicate_row",
        rebuilt_with_manifest(version, entries),
        move |error| match error {
            DocumentError::DuplicateId { kind: IdKind::Row, id, occurrences } => {
                *id == expected_id
                    && occurrences.len() == 2
                    && expected_sheets
                        .iter()
                        .all(|sheet| occurrences.iter().any(|entry| entry.contains(sheet)))
            }
            _ => false,
        },
    );
}

/// 実在しない型定義を参照するスキーマは、参照元と参照先つきで中止される（要件 1.7）。
///
/// `$ref` は生テキストであり、`SchemaPart` はその実在を見ない。検出は構造検証（段 5）。
#[test]
fn a_dangling_type_ref_is_reported_with_source_and_target() {
    let scratch = Scratch::new("dangling_type");
    let container = encode(entries_of_document(&document_with_dangling_type_ref()));

    assert_open_rejects(&scratch, "dangling_type", container, |error| {
        matches!(
            error,
            DocumentError::DanglingTypeRef { from, to }
                if to == TYPEDEF_ID && from.starts_with("schemas/")
        )
    });
}

/// レジストリに無い添付を参照するセルは、参照元と識別子つきで中止される（要件 7.4）。
///
/// 埋め込み型の `AttachmentId`（64 文字小文字 hex。要件 7.3）はそのまま値に載るため、
/// 参照先の実在は構造検証（段 5）が唯一の防衛線である。
#[test]
fn a_dangling_attachment_ref_is_reported_with_source_and_id() {
    let scratch = Scratch::new("dangling_attachment");
    let container = encode(entries_of_document(&document_with_unregistered_attachment()));

    assert_open_rejects(&scratch, "dangling_attachment", container, |error| {
        matches!(
            error,
            DocumentError::DanglingAttachmentRef { from, id }
                if id == UNREGISTERED_ATTACHMENT_HEX && from.starts_with("sheets/")
        )
    });
}

/// 行データのエントリはあるが対応するスキーマが無いシートは、シート識別子つきで中止される
/// （要件 4.4）。
///
/// `document.json` のシート宣言と行エントリは正しいまま、`schemas/<ulid>.json` の実体だけを
/// 取り除き、索引を組み直す（行エントリの不在と取り違えないこと）。
#[test]
fn a_sheet_without_its_schema_part_is_reported_with_the_sheet_id() {
    let scratch = Scratch::new("missing_schema");
    let document = sample();
    let sheet = document.sheets()[0].id();
    let parts = to_parts(&document).expect("標本はパート集合へ取り出せる");
    let version = parts.format_version();

    let entries: Vec<(EntryName, Vec<u8>)> = entries_of(&parts)
        .into_iter()
        .filter(|(name, _)| *name != EntryName::Schema { sheet })
        .collect();
    assert!(
        entries.iter().any(|(name, _)| *name == EntryName::Rows { sheet }),
        "行エントリは残っている（欠くのはスキーマだけ）"
    );

    assert_open_rejects(
        &scratch,
        "missing_schema",
        rebuilt_with_manifest(version, entries),
        move |error| {
            matches!(
                error,
                DocumentError::MissingSchema { sheet: reported } if *reported == sheet.to_string()
            )
        },
    );
}

/// `document.json` が索引にも実体にも無い入力は、不足しているエントリ名つきで中止される
/// （要件 4.5）。
///
/// 検出は `from_parts` の段 4（パート復号）であり、ダイジェスト照合（段 3）は通る
/// （索引に `document.json` が載っていないため）。
#[test]
fn a_missing_document_part_is_reported() {
    let scratch = Scratch::new("missing_document");
    let parts = to_parts(&sample()).expect("標本はパート集合へ取り出せる");
    let version = parts.format_version();
    let entries: Vec<(EntryName, Vec<u8>)> = entries_of(&parts)
        .into_iter()
        .filter(|(name, _)| *name != EntryName::Document)
        .collect();

    assert_open_rejects(
        &scratch,
        "missing_document",
        rebuilt_with_manifest(version, entries),
        |error| {
            matches!(
                error,
                DocumentError::MissingPart { name } if name == &EntryName::Document.to_string()
            )
        },
    );
}

/// `manifest.json`（唯一の権威ある索引）が無い入力は、不足しているエントリ名つきで中止される
/// （要件 4.5）。
///
/// 公開 API は索引の無い集合を構築できない（`DocumentParts::from_entries` が索引を要求する）
/// ため、`zip` クレートでアーカイブを直接組む（モジュール docs 参照）。同じ組み立てで索引を
/// **残した**入力を対照に取り、この組み立て自体は読み込めることを先に確かめる。これにより、
/// 失敗の原因が索引の欠落だけであることが示される。検出は `ContainerCodec::decode` の
/// 集合構築（段 6。索引の解決）である。
#[test]
fn a_missing_manifest_is_reported() {
    let scratch = Scratch::new("missing_manifest");

    // 対照: 索引を残した同じ組み立ての入力は読める。
    let control = scratch.file("control.jxcel");
    fs::write(&control, container_with_optional_manifest(&sample(), true)).expect("書き出し");
    api().open(&control).expect("索引を残した入力は読める（欠落だけが原因であることの確認）");

    assert_open_rejects(
        &scratch,
        "missing_manifest",
        container_with_optional_manifest(&sample(), false),
        |error| {
            matches!(
                error,
                DocumentError::MissingPart { name } if name == &EntryName::Manifest.to_string()
            )
        },
    );
}

//! 全経路の往復同一性（タスク 8.1。要件 2.2, 2.3, 4.1）。
//!
//! design「File Structure Plan」の `tests/roundtrip.rs`（`Model -> Parts -> Container ->
//! Parts -> Model の同一性`）と「Testing Strategy / Integration Tests」の「往復」項目を
//! 実装する。ZIP の知識はコンテナ層（[`ContainerCodec`]）にだけ現れ、それ以外は公開 API の
//! 経路だけを使う。検証する経路は 3 つ:
//!
//! 1. **フルサーキット**: モデル → `to_parts` → `ContainerCodec::encode` →
//!    `ContainerCodec::decode` → `from_parts` → モデルが完全一致する。
//! 2. **ファイル往復**: `save` → `open` が完全一致する（`OpenOutcome.document` を比較。現行版
//!    なので `migrated_from` は `None`、`beyond_supported_scale` は `false`）。
//! 3. **経路の一致**: ZIP を経由する経路（`encode` → `decode`）と経由しない経路
//!    （`to_parts` → `from_parts`）が、同一の入力モデルから出発して同じモデルを与える。
//!
//! 標本は複数用意する（[`samples`]）。1 つの標本だけで通すテストにしない: 最小（0 シート）・
//! 複数シート（0 行のシートを含む）・添付あり・入れ子/添付参照/脱出口が要る値を含む行・
//! 未参照添付・未知フィールド（トップレベルと要素内）を分散させる。標本の組み立てと比較は
//! `tests/common/mod.rs` が持つ。
//!
//! 同一性の判定は [`assert_same_document`] が唯一の口である。`Document` に `PartialEq` が
//! 無く、`CellValue` の等値だけでは payload のバイト列・保持フィールド・列順の差を見逃すため、
//! 比較は `DocumentView` の全項目（識別子・シート・行・スキーマ・添付・未参照添付・ wire 射影）
//! で行う。その比較が弱くないことは
//! `the_comparison_view_separates_every_observable_aspect` が 1 箇所ずつ変えたモデルで実測する。
//!
//! 一時ファイルはリポジトリ内の `tests/scratch_*` に作り、`Drop` で削除する
//! （`tests/common/mod.rs` の docs 参照）。

mod common;

use std::fs;

use document_format::container::ContainerCodec;
use document_format::parts::DocumentParts;
use document_format::{CellValue, Document, DocumentFormatApi, EntryName, RowId, SchemaPart};

use common::{
    api, assert_same_document, document_view, document_with_sheet, entries_of,
    entries_with_preserved_fields, fixture_path, sample, sample_minimal, sample_with_unknown_fields,
    sample_with_unreferenced_attachment, sample_with_values, with_rebuilt_manifest, Scratch,
    SCHEMA_EMPTY, SCHEMA_WITH_REF,
};

/// 全経路で同一性を確かめる標本の一覧。
///
/// 標本ごとに 1 つの性質を担わせ、どれか 1 つが落ちれば経路の欠陥を特定できるようにする。
fn samples() -> Vec<(&'static str, Document)> {
    vec![
        ("minimal", sample_minimal()),
        ("multi_sheet", sample()),
        ("values", sample_with_values()),
        ("unreferenced_attachment", sample_with_unreferenced_attachment()),
        ("unknown_fields", sample_with_unknown_fields("A")),
    ]
}

/// モデル → パート集合 → ZIP → パート集合 → モデルで完全に同一のモデルが戻る（要件 2.2, 2.3）。
///
/// 途中のパート集合（復号後）が元の集合と（名前, バイト列）で一致することも確かめる
/// （マーカーの除去と展開後バイト列の復元をここで固定する）。
#[test]
fn the_full_circuit_restores_every_sample_exactly() {
    for (name, before) in samples() {
        let parts = api().to_parts(&before).expect("標本はパート集合へ取り出せる");
        let bytes = ContainerCodec::encode(&parts).expect("符号化できる");
        let decoded = ContainerCodec::decode(&bytes).expect("復号できる");
        assert_eq!(entries_of(&parts), entries_of(&decoded), "{name}: 復号が元の集合と違う");

        let after = api().from_parts(&decoded).expect("モデルを復元できる");
        assert_same_document(&before, &after);
    }
}

/// ZIP を経由しない経路（`to_parts` → `from_parts`）も完全に同一のモデルを戻す（要件 2.2）。
#[test]
fn the_zip_free_path_restores_every_sample_exactly() {
    for (_name, before) in samples() {
        let parts = api().to_parts(&before).expect("標本はパート集合へ取り出せる");
        let after = api().from_parts(&parts).expect("モデルを復元できる");
        assert_same_document(&before, &after);
    }
}

/// ZIP を経由する経路と経由しない経路が同じモデルを与える（要件 2.2, 2.3）。
///
/// 同一の入力モデルから 2 つの経路を走らせ、両者が元のモデルとも一致することを確かめる。
#[test]
fn the_zip_path_and_the_zip_free_path_give_the_same_model() {
    for (_name, before) in samples() {
        let parts = api().to_parts(&before).expect("標本はパート集合へ取り出せる");
        let zip_free = api().from_parts(&parts).expect("ZIP 非経由で復元できる");

        let bytes = ContainerCodec::encode(&parts).expect("符号化できる");
        let decoded = ContainerCodec::decode(&bytes).expect("復号できる");
        let via_zip = api().from_parts(&decoded).expect("ZIP 経由で復元できる");

        assert_same_document(&zip_free, &via_zip);
        assert_same_document(&before, &via_zip);
        assert_same_document(&before, &zip_free);
    }
}

/// 保存したドキュメントを開き直すと元のモデルと一致する（要件 4.1）。
///
/// 現行版の保存なので移行は起きず（`migrated_from` は `None`）、規模も保証対象内である。
#[test]
fn saving_and_reopening_restores_every_sample_exactly() {
    let scratch = Scratch::new("roundtrip_save");
    for (name, before) in samples() {
        let path = scratch.file(&format!("{name}.jxcel"));
        api().save(&before, &path).expect("標本は保存できる");

        let outcome = api().open(&path).expect("保存したファイルは開ける");
        assert_same_document(&before, &outcome.document);
        assert_eq!(None, outcome.migrated_from, "{name}: 現行版なのに移行元が記録された");
        assert!(!outcome.beyond_supported_scale, "{name}: 保証対象外とされた");
    }
}

/// コミット済みゴールデンを開き、両経路のモデルが一致する（実ファイルの経路も通す）。
///
/// ZIP を経由しない経路（`open` したモデルの `to_parts` → `from_parts`）と ZIP を経由する
/// 経路（実ファイルのバイト列を `decode` → `from_parts`）を、`open` が返したモデルと
/// 突き合わせる。
#[test]
fn the_committed_golden_fixture_agrees_on_both_paths() {
    let bytes = fs::read(fixture_path()).expect("ゴールデンが読める");
    let outcome = api().open(&fixture_path()).expect("ゴールデンは開ける");

    let restored = api()
        .to_parts(&outcome.document)
        .expect("ゴールデンのモデルはパート集合へ取り出せる");
    let zip_free = api().from_parts(&restored).expect("ZIP 非経由で復元できる");

    let decoded = ContainerCodec::decode(&bytes).expect("復号できる");
    let via_zip = api().from_parts(&decoded).expect("ZIP 経由で復元できる");

    assert_same_document(&outcome.document, &zip_free);
    assert_same_document(&zip_free, &via_zip);
}

/// どの行からも参照されない添付が、どの経路でも落ちない（要件 7.6 の前提）。
///
/// 本格的な検証（未参照添付の一覧と取得）はタスク 8.8 が担うが、往復で消えないこと自体は
/// ここで押さえる。モデルの `parts` 射影（`document_view`）にも添付エントリが含まれる。
#[test]
fn an_unreferenced_attachment_is_not_dropped_on_any_path() {
    let before = sample_with_unreferenced_attachment();
    assert!(!before.unreferenced_attachments().is_empty(), "標本に未参照添付が無い");

    let parts = api().to_parts(&before).expect("標本はパート集合へ取り出せる");
    let zip_free = api().from_parts(&parts).expect("ZIP 非経由で復元できる");

    let bytes = ContainerCodec::encode(&parts).expect("符号化できる");
    let via_zip = api()
        .from_parts(&ContainerCodec::decode(&bytes).expect("復号できる"))
        .expect("ZIP 経由で復元できる");

    assert_eq!(
        before.unreferenced_attachments(),
        zip_free.unreferenced_attachments(),
        "ZIP 非経由で未参照添付が落ちた"
    );
    assert_eq!(
        before.unreferenced_attachments(),
        via_zip.unreferenced_attachments(),
        "ZIP 経由で未参照添付が落ちた"
    );
    assert_same_document(&before, &zip_free);
    assert_same_document(&before, &via_zip);
}

/// 比較（`document_view` / `assert_same_document`）が弱くないことを実測する。
///
/// 観測可能な項目を 1 つずつ変えた対照を作り、**その項目が差を捉えること**を確かめる。
/// これは「比較ヘルパから 1 項目の比較を抜くと、対応する対照を検出できなくなる」ことの
/// 恒久的な自己検査である（変異検査の実測はタスク完了報告に記録する）。
#[test]
fn the_comparison_view_separates_every_observable_aspect() {
    // ドキュメント識別子: 独立に作った 2 文書は識別子が違う。
    assert_ne!(
        document_view(&sample_minimal()).document_id,
        document_view(&sample_minimal()).document_id,
        "識別子の差を捉えていない"
    );

    // シート名: 同一の文書の名前だけを変える（他の項目は変えない）。
    let mut named = document_with_sheet(SCHEMA_WITH_REF, &["a", "b"], Vec::new());
    let sheet = named.sheets()[0].id();
    let before_name = document_view(&named).sheets;
    named.rename_sheet(sheet, "改名後").expect("改名できる");
    assert_ne!(before_name, document_view(&named).sheets, "シート名の差を捉えていない");

    // 列順: 同一の文書の列名の順序だけを入れ替える（列名の集合は同じ）。
    let before_columns = document_view(&named).sheets;
    named
        .set_sheet_columns(sheet, vec!["b".to_owned(), "a".to_owned()])
        .expect("列を差し替えられる");
    assert_ne!(before_columns, document_view(&named).sheets, "列順の差を捉えていない");

    // 行順: 同一の行集合を並べ替える（行の内容は変えない）。
    let mut ordered = document_with_sheet(
        SCHEMA_EMPTY,
        &["v"],
        vec![vec![CellValue::Int(1)], vec![CellValue::Int(2)]],
    );
    let sheet = ordered.sheets()[0].id();
    let ids: Vec<RowId> = ordered.sheets()[0].rows().iter().map(|row| row.id()).collect();
    let before_order = document_view(&ordered).rows;
    ordered.reorder_rows(sheet, &[ids[1], ids[0]]).expect("並べ替えできる");
    assert_ne!(before_order, document_view(&ordered).rows, "行順の差を捉えていない");

    // 行の値: 1 セルだけ変える。
    let mut valued = document_with_sheet(SCHEMA_EMPTY, &["v"], vec![vec![CellValue::Int(1)]]);
    let sheet = valued.sheets()[0].id();
    let row = valued.sheets()[0].rows()[0].id();
    let before_values = document_view(&valued).rows;
    valued.set_row_values(sheet, row, vec![CellValue::Int(2)]).expect("値は設定できる");
    assert_ne!(before_values, document_view(&valued).rows, "行の値の差を捉えていない");

    // 型定義ペイロード: 同じ型定義識別子のまま `definition` だけを変える。
    let mut typed = document_with_sheet(SCHEMA_WITH_REF, &[], Vec::new());
    let sheet = typed.sheets()[0].id();
    let before_schema = document_view(&typed).schemas;
    let other = SchemaPart::parse(&SCHEMA_WITH_REF.replace("\"string\"", "\"number\""))
        .expect("標本は妥当");
    typed.set_root_schema(sheet, other).expect("差し替えできる");
    assert_ne!(before_schema, document_view(&typed).schemas, "型定義ペイロードの差を捉えていない");

    // 添付バイト列: 別の内容を登録する（識別子もバイト列も変わる）。
    let mut attached = document_with_sheet(SCHEMA_EMPTY, &[], Vec::new());
    let before_attachments = document_view(&attached).attachments;
    attached.add_attachment(vec![1, 2, 3]);
    assert_ne!(before_attachments, document_view(&attached).attachments, "添付の差を捉えていない");

    // 未参照添付: 行から参照されない添付を足すと、未参照一覧だけが変わる。
    let mut extras = sample_with_unreferenced_attachment();
    let before_unreferenced = document_view(&extras).unreferenced_attachments;
    extras.add_attachment(vec![9, 9, 9]);
    assert_ne!(
        before_unreferenced,
        document_view(&extras).unreferenced_attachments,
        "未参照添付の差を捉えていない"
    );

    // 保持フィールド: 内容が同じで未知フィールドの値だけが違う 2 モデルを作る。
    // 片方は他方のエントリから未知フィールドの値だけを差し替える（識別子もシートも行も
    // スキーマも添付も同一であることを下で確かめる）。
    let entries_a = entries_with_preserved_fields("A");
    let version = DocumentParts::from_entries(entries_a.clone())
        .expect("標本の集合は妥当")
        .format_version();
    let entries_b: Vec<(EntryName, Vec<u8>)> = entries_a
        .iter()
        .map(|(name, bytes)| {
            let text = String::from_utf8(bytes.clone()).expect("エントリは UTF-8");
            (*name, text.replace("\"A\"", "\"B\"").into_bytes())
        })
        .collect();
    let entries_b = with_rebuilt_manifest(version, entries_b);
    let model_a = api()
        .from_parts(&DocumentParts::from_entries(entries_a).expect("標本の集合は妥当"))
        .expect("モデルを復元できる");
    let model_b = api()
        .from_parts(&DocumentParts::from_entries(entries_b).expect("標本の集合は妥当"))
        .expect("モデルを復元できる");

    let view_a = document_view(&model_a);
    let view_b = document_view(&model_b);
    assert_eq!(view_a.document_id, view_b.document_id, "対照が識別子まで変えている");
    assert_eq!(view_a.sheets, view_b.sheets, "対照がシートまで変えている");
    assert_eq!(view_a.rows, view_b.rows, "対照が行まで変えている");
    assert_eq!(view_a.schemas, view_b.schemas, "対照がスキーマまで変えている");
    assert_eq!(view_a.attachments, view_b.attachments, "対照が添付まで変えている");
    assert_ne!(view_a.parts, view_b.parts, "保持フィールドの差を捉えていない");

    // 差が実際に比較ヘルパを落とすことも確かめる（View の項目が
    // `assert_same_document` の比較へ結線されていることの観測）。
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_same_document(&model_a, &model_b);
    }));
    assert!(rejected.is_err(), "assert_same_document が保持フィールドの差を検出しなかった");
}

//! 論理エントリ集合の公開契約（タスク 7.3。要件 2.2, 2.3）。
//!
//! このファイルは **`document_format::container` を import しない**。それ自体が
//! 「モデル ⇄ パート集合の公開経路（[`DocumentFormatApi::to_parts`] / [`DocumentFormatApi::from_parts`]）が
//! ZIP を経由しない」ことの実証である（design「Public API Layer / DocumentFormatApi」の
//! Implementation Notes「`to_parts` / `from_parts` が `version-control` との唯一の接点で
//! ある」）。`tests/api.rs` は open / save の検証のために `ContainerCodec` を import する
//! ため、ZIP 非経由を import だけで示せる場所として本ファイルを分けている。ファイル I/O も
//! 一切行わず、一時ファイルを作らない。
//!
//! 標本と比較ヘルパは `tests/common/mod.rs` が持つ（`tests/roundtrip.rs` と共有するため）。
//! **あちらも `document_format::container` を import しない**ので、`mod common;` を宣言しても
//! 本ファイルの ZIP 非経由は import の水準で保たれる。
//!
//! 観測する契約は 4 つ:
//!
//! 1. **往復**（要件 2.2）: モデル → `to_parts` → `from_parts` で同一のモデルが戻る。
//! 2. **到達可能性**（要件 2.2, 2.3）: 得た `DocumentParts` からエントリ名と中身を列挙でき、
//!    エントリ集合が期待どおり（`document.json` / シートごとのスキーマと行 /
//!    `attachments/<hex64>.bin` / `manifest.json`）であり、各行エントリが**そのシートの行
//!    だけ**を含み、形式マーカー `jxcel` を含まない。`version-control` はこの経路だけで
//!    ドキュメントの中身に到達できる。
//! 3. **決定性**（要件 2.2）: 同じモデルから 2 回 `to_parts` した結果のエントリ名と中身が
//!    バイト単位で一致し、`iter()` はエントリ名の昇順で反復する。
//! 4. **書き出し側の検証**: 構造的不変条件に違反するモデルは `to_parts` が拒否する
//!    （読み込み経路が拒否する集合を `version-control` へ渡さない）。

mod common;

use document_format::parts::{DocumentParts, RowsCodec};
use document_format::{DocumentError, DocumentFormatApi, EntryName, IdKind};

use common::{
    api, attachments, document_with_dangling_type_ref, document_with_duplicate_type_def,
    document_with_unregistered_attachment, rows, sample, schemas, sheet_metadata,
};

/// モデル → `to_parts` → `from_parts` で同一のモデルが戻る（要件 2.2）。
///
/// 比較は `tests/api.rs` の往復テストと同じ流儀で、識別子・シート・行・スキーマ・添付を
/// 実比較する。このテストはコンテナ層の型を一切参照しない（ファイル冒頭の import が
/// ZIP 非経由の実証である）。
#[test]
fn to_parts_and_from_parts_round_trip_the_model_without_zip() {
    let before = sample();

    let parts = api().to_parts(&before).expect("標本はパート集合へ取り出せる");
    let after = api().from_parts(&parts).expect("パート集合は復元できる");

    assert_eq!(before.document_id(), after.document_id(), "識別子が変わった");
    assert_eq!(sheet_metadata(&before), sheet_metadata(&after), "シートが変わった");
    assert_eq!(rows(&before), rows(&after), "行が変わった");
    assert_eq!(schemas(&before), schemas(&after), "スキーマが変わった");
    assert_eq!(attachments(&before), attachments(&after), "添付が変わった");
}

/// エントリ名と中身を列挙でき、集合と各行エントリの中身が契約どおりである
/// （要件 2.2, 2.3。`version-control` がこの経路だけで中身へ到達できることの実測）。
#[test]
fn parts_expose_every_entry_and_the_rows_of_their_own_sheet() {
    let document = sample();
    let parts = api().to_parts(&document).expect("標本はパート集合へ取り出せる");

    // エントリ集合の期待値: メタデータ・シートごとのスキーマと行・添付・索引（要件 2.2, 2.3）。
    let mut expected = vec![EntryName::Manifest, EntryName::Document];
    for sheet in document.sheets() {
        expected.push(EntryName::Schema { sheet: sheet.id() });
        expected.push(EntryName::Rows { sheet: sheet.id() });
    }
    for attachment in document.attachments().iter() {
        expected.push(EntryName::Attachment { attachment: attachment.id() });
    }
    expected.sort();

    let names: Vec<EntryName> = parts.iter().map(|part| part.name).collect();
    assert_eq!(expected, names, "公開されたエントリ集合が期待どおりでない");
    assert!(
        !names.iter().any(|name| matches!(name, EntryName::Marker)),
        "論理エントリ集合に形式マーカー jxcel が混ざっている"
    );
    assert!(
        names.windows(2).all(|pair| pair[0] < pair[1]),
        "エントリの並びがエントリ名の昇順でない"
    );

    // 中身（バイト列）が読める。行以外のエントリは空であってはならない
    // （0 行のシートの行エントリだけが空になる）。
    for part in parts.iter() {
        if !matches!(part.name, EntryName::Rows { .. }) {
            assert!(!part.bytes.is_empty(), "空のエントリがある: {:?}", part.name);
        }
    }

    // 各行エントリが**そのシートの行だけ**を含む。
    let mut row_entries = 0usize;
    for part in parts.iter() {
        let EntryName::Rows { .. } = part.name else {
            continue;
        };
        row_entries += 1;
        let decoded = RowsCodec::decode(&part.name, &part.bytes).expect("行エントリは復号できる");
        let sheet = document
            .sheets()
            .iter()
            .find(|sheet| sheet.id() == decoded.sheet())
            .expect("行エントリのシートがモデルに実在する");
        assert_eq!(sheet.rows().len(), decoded.rows().len(), "行数がそのシートと違う");
        for (expected_row, actual_row) in sheet.rows().iter().zip(decoded.rows()) {
            assert_eq!(
                expected_row.id().to_string(),
                actual_row.id().to_string(),
                "行識別子がそのシートの行と違う"
            );
            assert_eq!(expected_row.values(), actual_row.values(), "セル値が違う");
        }
    }
    assert_eq!(document.sheets().len(), row_entries, "行エントリが全シート分ない");

    // 添付の中身がバイト単位で読める（不透明バイト列として運ばれる。要件 7.5）。
    for attachment in document.attachments().iter() {
        let name = EntryName::Attachment { attachment: attachment.id() };
        let part = parts.get(&name).expect("添付エントリが実在する");
        assert_eq!(attachment.bytes(), part.bytes.as_slice(), "添付のバイト列が変わった");
    }
}

/// 同じモデルから 2 回取り出した集合は、エントリ名も中身もバイト単位で一致する（要件 2.2, 3.1）。
///
/// `DocumentParts` は `PartialEq` を持たないため、`iter()` の（名前, バイト列）で比較する。
#[test]
fn to_parts_is_deterministic() {
    let document = sample();

    let first = api().to_parts(&document).expect("1 回目は成功する");
    let second = api().to_parts(&document).expect("2 回目は成功する");

    let capture = |parts: &DocumentParts| -> Vec<(EntryName, Vec<u8>)> {
        parts.iter().map(|part| (part.name, part.bytes.clone())).collect()
    };
    assert_eq!(capture(&first), capture(&second), "同じモデルから違う集合が出た");
}

/// 読み込み経路が拒否する文書は `to_parts` も拒否する（書き出し側の不変条件検証）。
///
/// `to_parts` が `validate_document` を通しているため、構築では強制されない違反
/// （宙吊り型参照・未登録添付参照・`TypeDefId` の重複宣言）はここで遮断され、
/// `version-control` は読み込み経路が拒否する集合を受け取らない。
#[test]
fn to_parts_rejects_documents_the_read_path_would_refuse() {
    let error = api()
        .to_parts(&document_with_dangling_type_ref())
        .expect_err("宙吊り型参照は取り出せない");
    assert!(
        matches!(error, DocumentError::DanglingTypeRef { .. }),
        "宙吊り型参照が DanglingTypeRef でない: {error:?}"
    );

    let error = api()
        .to_parts(&document_with_unregistered_attachment())
        .expect_err("未登録添付参照は取り出せない");
    assert!(
        matches!(error, DocumentError::DanglingAttachmentRef { .. }),
        "未登録添付参照が DanglingAttachmentRef でない: {error:?}"
    );

    let error = api()
        .to_parts(&document_with_duplicate_type_def())
        .expect_err("TypeDefId の重複宣言は取り出せない");
    assert!(
        matches!(&error, DocumentError::DuplicateId { kind: IdKind::TypeDef, .. }),
        "型定義の重複宣言が DuplicateId(TypeDef) でない: {error:?}"
    );
}

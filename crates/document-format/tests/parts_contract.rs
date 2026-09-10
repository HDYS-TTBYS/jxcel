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

use document_format::parts::{DocumentParts, RowsCodec};
use document_format::{
    AttachmentId, CellValue, Document, DocumentError, DocumentFormat, DocumentFormatApi, EntryName,
    IdKind, SchemaPart, SheetId,
};

/// 標本のルートスキーマ（型定義への参照を含む）。
///
/// 型定義識別子は `concat!` がリテラルを要求するため、各断片に直接埋め込む。
const SCHEMA_WITH_REF: &str = concat!(
    r#"{"root":{"$ref":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#""},"types":[{"id":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#"","definition":{"kind":"string"}}]}"#
);

/// 型定義を 1 つも持たないルートスキーマ（0 行のシートの初期値）。
const SCHEMA_EMPTY: &str = r#"{"root":null}"#;

/// 実在しない型定義を参照するルートスキーマ（`to_parts` が遮断すべき宙吊り参照）。
const SCHEMA_DANGLING_REF: &str = concat!(
    r#"{"root":{"$ref":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#""},"types":[]}"#
);

/// 同一の型定義識別子を 2 回宣言するルートスキーマ（`to_parts` が遮断すべき一意性違反）。
const SCHEMA_DUPLICATE_TYPE_DEF: &str = concat!(
    r#"{"root":null,"types":[{"id":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#"","definition":{"kind":"string"}},{"id":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#"","definition":{"kind":"number"}}]}"#
);

/// レジストリに登録されていない添付識別子（正準 64 文字小文字 hex）。
const UNREGISTERED_ATTACHMENT_HEX: &str =
    "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

/// 公開経路を起動する実装（design はトレイトのみを指定するため、無状態の具象型を使う）。
fn api() -> DocumentFormat {
    DocumentFormat::new()
}

/// 標本の文書: 行と添付参照を持つシートと、0 行のシート（`tests/api.rs` と同じ骨格）。
fn sample() -> Document {
    let mut document = Document::new();

    let stocked = document.add_sheet("在庫");
    document
        .set_sheet_columns(
            stocked,
            ["name", "count", "blob"].iter().map(|name| (*name).to_owned()).collect(),
        )
        .expect("標本のシートは実在する");
    document
        .set_root_schema(stocked, SchemaPart::parse(SCHEMA_WITH_REF).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    // 非 UTF-8 を含む添付（要件 7.5 の不透明性）。
    let attachment = document.add_attachment(vec![0xff, 0x00, 0x80, b'j', 0xfe]);
    let row = document.add_row(stocked).expect("標本のシートは実在する");
    document
        .set_row_values(
            stocked,
            row,
            vec![
                CellValue::Text("りんご 🍎".to_owned()),
                CellValue::Int(3),
                CellValue::Attachment(attachment),
            ],
        )
        .expect("標本の行は実在する");

    let empty = document.add_sheet("空のシート");
    document
        .set_sheet_columns(empty, vec!["a".to_owned(), "b".to_owned()])
        .expect("標本のシートは実在する");
    document
        .set_root_schema(empty, SchemaPart::parse(SCHEMA_EMPTY).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    document
}

/// シートの（識別子, 名前, 列名）を文書順に写す。
fn sheet_metadata(document: &Document) -> Vec<(SheetId, String, Vec<String>)> {
    document
        .sheets()
        .iter()
        .map(|sheet| (sheet.id(), sheet.name().to_owned(), sheet.columns().to_vec()))
        .collect()
}

/// 行の（識別子, セル値）をシート順・行順に写す。
fn rows(document: &Document) -> Vec<Vec<(String, Vec<CellValue>)>> {
    document
        .sheets()
        .iter()
        .map(|sheet| {
            sheet
                .rows()
                .iter()
                .map(|row| (row.id().to_string(), row.values().to_vec()))
                .collect()
        })
        .collect()
}

/// ルートスキーマの（ルートバイト列, 型定義識別子, 参照）をシート順に写す。
fn schemas(document: &Document) -> Vec<(Vec<u8>, Vec<String>, Vec<(String, String)>)> {
    document
        .sheets()
        .iter()
        .map(|sheet| {
            let schema = sheet.root_schema();
            let defs: Vec<String> = schema.type_def_ids().iter().map(|id| id.to_string()).collect();
            let refs: Vec<(String, String)> = schema
                .type_refs()
                .iter()
                .map(|reference| (reference.from().to_owned(), reference.to().to_owned()))
                .collect();
            (schema.root().as_bytes().to_vec(), defs, refs)
        })
        .collect()
}

/// 添付の（識別子, バイト列）を昇順に写す。
fn attachments(document: &Document) -> Vec<(String, Vec<u8>)> {
    document
        .attachments()
        .iter()
        .map(|attachment| (attachment.id().to_string(), attachment.bytes().to_vec()))
        .collect()
}

/// 1 シート・指定のルートスキーマ・指定の列と行を持つ文書を組み立てる。
fn document_with_sheet(schema: &str, columns: &[&str], rows: Vec<Vec<CellValue>>) -> Document {
    let mut document = Document::new();
    let sheet = document.add_sheet("標本");
    document
        .set_sheet_columns(sheet, columns.iter().map(|name| (*name).to_owned()).collect())
        .expect("標本のシートは実在する");
    document
        .set_root_schema(sheet, SchemaPart::parse(schema).expect("標本のスキーマは解析できる"))
        .expect("標本のシートは実在する");
    for values in rows {
        let row = document.add_row(sheet).expect("標本のシートは実在する");
        document.set_row_values(sheet, row, values).expect("標本の行は実在する");
    }
    document
}

/// 実在しない型定義を参照する 1 シートの文書。
fn document_with_dangling_type_ref() -> Document {
    document_with_sheet(SCHEMA_DANGLING_REF, &[], Vec::new())
}

/// 同一の型定義識別子を 2 回宣言する 1 シートの文書。
fn document_with_duplicate_type_def() -> Document {
    document_with_sheet(SCHEMA_DUPLICATE_TYPE_DEF, &[], Vec::new())
}

/// レジストリに登録されていない添付を参照する 1 シート 1 行の文書。
fn document_with_unregistered_attachment() -> Document {
    let attachment = AttachmentId::from_hex(UNREGISTERED_ATTACHMENT_HEX)
        .expect("標本の添付識別子は正準形");
    document_with_sheet(
        SCHEMA_EMPTY,
        &["blob"],
        vec![vec![CellValue::Attachment(attachment)]],
    )
}

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

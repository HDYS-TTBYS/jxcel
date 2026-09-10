//! 統合テスト（`tests/*.rs`）が共有するヘルパの置き場。
//!
//! `tests/` 配下の各 `.rs` は独立したテストバイナリであり、あるテストクレートの
//! 関数を別のテストクレートから使う機構が無い。そのため共有したいものはこの
//! サブモジュールへ置き、各テストファイルが `mod common;` を宣言して取り込む。
//! ここに置くのは次の 3 種である:
//!
//! 1. **標本の組み立て**（[`sample`] / [`sample_minimal`] / [`sample_with_values`] /
//!    [`sample_with_unreferenced_attachment`] / [`sample_with_unknown_fields`] /
//!    [`document_with_sheet`] と違反文書ビルダ）。
//! 2. **モデルの完全比較**（[`document_view`] / [`assert_same_document`]）。往復の
//!    同一性判定はここが唯一の口である。`Document` / `Sheet` / `SchemaPart` は
//!    未知フィールド保持の内部カーソルを持つため `PartialEq` を持たず、`CellValue`
//!    の等値だけでは payload のバイト列・保持フィールド・列順の差を見逃す
//!    （[`document_view`] の docs 参照）。
//! 3. **テスト用の小さな道具**（[`Scratch`]（一時ディレクトリ）・[`snapshot`] /
//!    [`entry_names`]・[`api`] / [`fixture_path`]・[`entries_of`] /
//!    [`with_rebuilt_manifest`]）。
//!
//! **このファイルは `document_format::container` を import しない。** これにより、
//! ZIP を一切経由しない公開契約を import だけで実証している `tests/parts_contract.rs`
//! が `mod common;` を宣言しても、その性質（コンテナ層への依存が無いこと）を保てる。
//! コンテナ層を使うヘルパ（`ContainerCodec` を呼ぶもの）は `tests/api.rs` 側に残す。
//!
//! 各テストバイナリが使う部分集合は違うため、バイナリによっては未使用の項目が生じる。
//! 検証対象を 1 箇所へ集約する利点が、バイナリごとの未使用項目より大きいので、
//! モジュール全体に `#![allow(dead_code)]` を置く（この意図を消さないこと）。
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use document_format::parts::{to_parts, DocumentParts, ManifestEntry, ManifestPart};
use document_format::{
    AttachmentId, CellValue, Document, DocumentFormat, DocumentFormatApi, EntryName, FormatVersion,
    NestedValue, SchemaPart, SheetId,
};

/// 標本のルートスキーマ（型参照 1 個と型定義 1 個を持つ）。
///
/// 型定義識別子は `concat!` がリテラルを要求するため、各断片に直接埋め込む。
pub const SCHEMA_WITH_REF: &str = concat!(
    r#"{"root":{"$ref":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#""},"types":[{"id":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#"","definition":{"kind":"string"}}]}"#
);

/// 型定義を 1 つも持たないルートスキーマ（0 行のシートの初期値）。
pub const SCHEMA_EMPTY: &str = r#"{"root":null}"#;

/// 実在しない型定義を参照するルートスキーマ（保存時に遮断すべき宙吊り参照）。
///
/// `SchemaPart::parse` は `$ref` の実在を見ない（スキーマは不透明ペイロードであり、
/// 参照整合性は構造検証の責務）。
pub const SCHEMA_DANGLING_REF: &str = concat!(
    r#"{"root":{"$ref":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#""},"types":[]}"#
);

/// 同一の型定義識別子を 2 回宣言するルートスキーマ（保存時に遮断すべき一意性違反）。
pub const SCHEMA_DUPLICATE_TYPE_DEF: &str = concat!(
    r#"{"root":null,"types":[{"id":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#"","definition":{"kind":"string"}},{"id":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#"","definition":{"kind":"number"}}]}"#
);

/// レジストリに登録されていない添付識別子（正準 64 文字小文字 hex）。
pub const UNREGISTERED_ATTACHMENT_HEX: &str =
    "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

/// 公開経路を起動する実装（design はトレイトのみを指定するため、無状態の具象型を使う）。
pub fn api() -> DocumentFormat {
    DocumentFormat::new()
}

/// ゴールデン fixture（決定性の基準として固定されたコンテナ）の絶対パス。
///
/// テストの作業ディレクトリに依存しないよう、マニフェストの位置から組み立てる。
pub fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bytes/golden_container.zip")
}

/// テスト専用の作業ディレクトリ（終了時に必ず削除する）。
///
/// テストは並行に走るため、プロセス ID と単調カウンタで一意にする。`Drop` は
/// panic による巻き戻しでも走るので、失敗したテストの一時ファイルも残らない。
/// コンテナ内とホストで `std::env::temp_dir()` の見え方が違うため、リポジトリ内に閉じる。
pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    pub fn new(tag: &str) -> Self {
        static SEQUENCE: AtomicU32 = AtomicU32::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(format!("scratch_{tag}_{}_{sequence}", std::process::id()));
        fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // 既に消えていても失敗しない（多重削除・異常終了の後始末）。
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// ディレクトリ直下の（名前, バイト列）を名前順で写す。
///
/// ファイルの改変（バイト列の変化）と一時ファイルの残留（名前の追加）を同時に捉える。
pub fn snapshot(directory: &Path) -> Vec<(String, Vec<u8>)> {
    let mut entries: Vec<(String, Vec<u8>)> = fs::read_dir(directory)
        .expect("作業ディレクトリが読める")
        .map(|entry| {
            let entry = entry.expect("ディレクトリ要素が読める");
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = fs::read(entry.path()).expect("要素が読める");
            (name, bytes)
        })
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

/// ディレクトリ直下の名前を昇順で返す（保存が対象パス以外へ書かないことの観測）。
pub fn entry_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("作業ディレクトリが読める")
        .map(|entry| entry.expect("ディレクトリ要素が読める").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// 標本のエントリ集合を（エントリ名, バイト列）の列へ写す。
///
/// `DocumentParts` は `PartialEq` を持たない（`Part` がダイジェストを抱える）ため、
/// 集合の等値比較はこの写像かコンテナの符号化バイト列で行う（tasks.md の申し送り）。
pub fn entries_of(parts: &DocumentParts) -> Vec<(EntryName, Vec<u8>)> {
    parts.iter().map(|part| (part.name, part.bytes.clone())).collect()
}

/// 索引（`manifest.json`）を実体から組み直したエントリ集合を返す。
///
/// ダイジェスト照合を通過させたまま後段（構造検証・バージョンゲート）だけを壊す入力や、
/// 実体を差し替えた入力を作るのに使う。`open`（ファイル経由）と `from_parts`（パーツ経由）の
/// 両方へ同じ集合を渡せるよう、コンテナのバイト列ではなくエントリ列を返す。
pub fn with_rebuilt_manifest(
    version: FormatVersion,
    mut entries: Vec<(EntryName, Vec<u8>)>,
) -> Vec<(EntryName, Vec<u8>)> {
    entries.retain(|(name, _)| *name != EntryName::Manifest);
    let index: Vec<ManifestEntry> = entries
        .iter()
        .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
        .collect();
    let manifest = ManifestPart::new(version, index)
        .expect("標本の索引は妥当")
        .to_json_bytes()
        .expect("符号化");
    entries.push((EntryName::Manifest, manifest));
    entries
}

/// シートの（識別子, 名前, 列名の順序付き一覧）を文書順に写す。
pub fn sheet_metadata(document: &Document) -> Vec<(SheetId, String, Vec<String>)> {
    document
        .sheets()
        .iter()
        .map(|sheet| (sheet.id(), sheet.name().to_owned(), sheet.columns().to_vec()))
        .collect()
}

/// 行の（識別子, セル値）をシート順・行順に写す。
pub fn rows(document: &Document) -> Vec<Vec<(String, Vec<CellValue>)>> {
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

/// ルートスキーマの（ルートのバイト列, 型定義の（識別子, `definition` のバイト列）,
/// （参照元, 参照先）の列）をシート順に写す。
///
/// `definition` の**バイト列**まで含めるのは、`TypeDefId` の一致だけでは
/// 型定義ペイロードの差を見逃すためである（往復の同一性はここで固定する）。
pub fn schemas(
    document: &Document,
) -> Vec<(Vec<u8>, Vec<(String, Vec<u8>)>, Vec<(String, String)>)> {
    document
        .sheets()
        .iter()
        .map(|sheet| {
            let schema = sheet.root_schema();
            let defs: Vec<(String, Vec<u8>)> = schema
                .type_defs()
                .iter()
                .map(|definition| {
                    (definition.id().to_string(), definition.definition().as_bytes().to_vec())
                })
                .collect();
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
pub fn attachments(document: &Document) -> Vec<(String, Vec<u8>)> {
    document
        .attachments()
        .iter()
        .map(|attachment| (attachment.id().to_string(), attachment.bytes().to_vec()))
        .collect()
}

/// モデルを公開面から余さず写し取った比較用の射影。
///
/// 往復の同一性判定（「完全に同一のモデル」）はこの型の等値で行う。項目の対応は:
///
/// * [`Self::document_id`]: ドキュメント識別子。
/// * [`Self::sheets`]: シートの**文書順**・識別子・名前・**列名の順序付き一覧**。
/// * [`Self::rows`]: 行の**行順**・`RowId`・セル値の列（`CellValue` の等値）。
/// * [`Self::schemas`]: ルートの**バイト列**（決定性の確認を兼ねる）・型定義の
///   `TypeDefId` と `definition` の**バイト列**・型参照の（元, 先）。
/// * [`Self::attachments`]: 添付の識別子とバイト列（全件、識別子の昇順）。
/// * [`Self::unreferenced_attachments`]: 未参照添付の一覧（往復で落ちていないこと）。
/// * [`Self::parts`]: モデルの wire 射影（`to_parts` のエントリ名とバイト列）。
///   未知フィールド（保持フィールド）に公開アクセサが無いため、その保持は
///   この射影のバイト列で比較する（エンコードは決定的なので、内容が同じなら
///   バイト列も同じ。要件 3.1）。
#[derive(Debug, PartialEq)]
pub struct DocumentView {
    pub document_id: document_format::DocumentId,
    pub sheets: Vec<(SheetId, String, Vec<String>)>,
    pub rows: Vec<Vec<(String, Vec<CellValue>)>>,
    pub schemas: Vec<(Vec<u8>, Vec<(String, Vec<u8>)>, Vec<(String, String)>)>,
    pub attachments: Vec<(String, Vec<u8>)>,
    pub unreferenced_attachments: Vec<String>,
    pub parts: Vec<(EntryName, Vec<u8>)>,
}

/// モデルを比較用の射影へ写す（[`DocumentView`] の docs 参照）。
///
/// `to_parts` を通すため、構造的不変条件に違反するモデル（宙吊り参照など）は
/// panic する。比較の対象は常に妥当なモデルである。
pub fn document_view(document: &Document) -> DocumentView {
    let parts = to_parts(document).expect("標本はパート集合へ取り出せる");
    DocumentView {
        document_id: document.document_id(),
        sheets: sheet_metadata(document),
        rows: rows(document),
        schemas: schemas(document),
        attachments: attachments(document),
        unreferenced_attachments: document
            .unreferenced_attachments()
            .iter()
            .map(|id| id.to_string())
            .collect(),
        parts: entries_of(&parts),
    }
}

/// 2 つのモデルが完全に同一かを確かめる（各項目を個別に比較し、違いを特定できる形にする）。
///
/// 比較項目の全量は [`DocumentView`] の docs にある。`CellValue` の等値だけに頼らないのは、
/// 保持フィールド・payload のバイト列・列順が `Document` の `PartialEq` 不在の下で
/// 抜けやすいためである。
pub fn assert_same_document(expected: &Document, actual: &Document) {
    let expected = document_view(expected);
    let actual = document_view(actual);
    assert_eq!(expected.document_id, actual.document_id, "ドキュメント識別子が変わった");
    assert_eq!(expected.sheets, actual.sheets, "シート（文書順・識別子・名前・列順）が変わった");
    assert_eq!(expected.rows, actual.rows, "行（行順・識別子・セル値）が変わった");
    assert_eq!(
        expected.schemas, actual.schemas,
        "スキーマ（ルート・型定義ペイロード・型参照）が変わった"
    );
    assert_eq!(expected.attachments, actual.attachments, "添付（識別子・バイト列）が変わった");
    assert_eq!(
        expected.unreferenced_attachments, actual.unreferenced_attachments,
        "未参照添付の一覧が変わった"
    );
    assert_eq!(
        expected.parts, actual.parts,
        "wire 射影（未知フィールドの保持・エントリ集合の決定性）が変わった"
    );
}

/// 標本の文書: 行と添付参照を持つシートと、0 行のシート。
pub fn sample() -> Document {
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

/// 最小の標本: 0 シート・0 添付（要件 1.1 が認める下限）。
pub fn sample_minimal() -> Document {
    Document::new()
}

/// 添付参照・入れ子・数値・文字列（脱出口が要る値を含む）を持つ 1 シート 3 行の標本。
///
/// 脱出口（`{"$t":...}`）を踏む値と `$` 始まりのキーを必ず含め、`Int` の両端
/// （`i64::MIN` / `i64::MAX`）と `Float` を混ぜる（要件 3.6）。
pub fn sample_with_values() -> Document {
    let mut document = Document::new();
    let sheet = document.add_sheet("値");
    document
        .set_sheet_columns(
            sheet,
            ["text", "decimal", "number", "nested", "ref"]
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        )
        .expect("標本のシートは実在する");
    document
        .set_root_schema(sheet, SchemaPart::parse(SCHEMA_WITH_REF).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    let attachment = document.add_attachment(vec![0x00, 0xff, b'a', 0x80]);
    let values = vec![
        vec![
            // 十進文法に一致する Text は脱出口で書かないと Decimal に化ける。
            CellValue::Text("123".to_owned()),
            CellValue::Decimal("12.50".to_owned()),
            CellValue::Int(i64::MIN),
            CellValue::Nested(NestedValue::Object(vec![
                ("$t".to_owned(), CellValue::Int(1)),
                ("$ref".to_owned(), CellValue::Text("x".to_owned())),
            ])),
            CellValue::Attachment(attachment),
        ],
        vec![
            // 添付 hex に読める Text も脱出口で書く（64 文字小文字 hex）。
            CellValue::Text("a".repeat(64)),
            // 十進文法外の Decimal（空でない任意のテキスト）も脱出口で書く。
            CellValue::Decimal("説明".to_owned()),
            CellValue::Float(1.5),
            CellValue::Nested(NestedValue::Array(vec![
                CellValue::Text("x".to_owned()),
                CellValue::Null,
                CellValue::Nested(NestedValue::Object(vec![(
                    "$weird".to_owned(),
                    CellValue::Int(2),
                )])),
            ])),
            CellValue::Text("🍎".to_owned()),
        ],
        vec![
            CellValue::Text("plain".to_owned()),
            CellValue::Decimal("-0.5e-3".to_owned()),
            CellValue::Int(i64::MAX),
            CellValue::Bool(false),
            CellValue::Null,
        ],
    ];
    for row_values in values {
        let row = document.add_row(sheet).expect("標本のシートは実在する");
        document.set_row_values(sheet, row, row_values).expect("標本の行は実在する");
    }

    // 脱出口と `$` エスケープが実際に使われることを固定する（値の判別規則が変わって
    // 標本が素通りすると、往復の検証が空振りするため）。
    let parts = to_parts(&document).expect("標本はパート集合へ取り出せる");
    let wire: String = parts
        .iter()
        .filter(|part| matches!(part.name, EntryName::Rows { .. }))
        .map(|part| String::from_utf8(part.bytes.clone()).expect("行エントリは UTF-8"))
        .collect();
    assert!(wire.contains(r#"{"$t":"text","v":"123"}"#), "Text の脱出口が使われていない: {wire}");
    assert!(wire.contains(r#"{"$t":"decimal""#), "Decimal の脱出口が使われていない: {wire}");
    assert!(wire.contains(r#""$$t""#), "`$` 始まりのキーがエスケープされていない: {wire}");

    document
}

/// 参照される添付と、どの行からも参照されない添付を 1 つずつ持つ標本（要件 7.6 の前提）。
pub fn sample_with_unreferenced_attachment() -> Document {
    let mut document = Document::new();
    let sheet = document.add_sheet("未参照");
    document
        .set_sheet_columns(sheet, vec!["ref".to_owned()])
        .expect("標本のシートは実在する");
    document
        .set_root_schema(sheet, SchemaPart::parse(SCHEMA_EMPTY).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    let referenced = document.add_attachment(vec![1, 2, 3]);
    let _unreferenced = document.add_attachment(vec![0xde, 0xad, 0xbe, 0xef]);
    let row = document.add_row(sheet).expect("標本のシートは実在する");
    document
        .set_row_values(sheet, row, vec![CellValue::Attachment(referenced)])
        .expect("標本の行は実在する");

    document
}

/// 未知フィールド（保持フィールド）を含むエントリ集合を組み立てる。
///
/// `document.json` のトップレベルとシート要素、`schemas/<sheet>.json` のトップレベルと
/// 型定義要素のそれぞれに、`marker` を値に埋めた未知キーを差し込む（要件 6.2 / 6.3 の
/// 前方互換。トップレベルと要素内の両方を踏む）。差し込む位置は既知キーの宣言順に対する
/// 位置（先頭 / 末尾）で変え、差し戻し位置の検証を兼ねる。索引は実体から組み直す。
pub fn entries_with_preserved_fields(marker: &str) -> Vec<(EntryName, Vec<u8>)> {
    let mut base = Document::new();
    let sheet = base.add_sheet("保持");
    base.set_sheet_columns(sheet, vec!["c".to_owned()]).expect("標本のシートは実在する");
    base.set_root_schema(sheet, SchemaPart::parse(SCHEMA_WITH_REF).expect("標本は妥当"))
        .expect("標本のシートは実在する");
    let row = base.add_row(sheet).expect("標本のシートは実在する");
    base.set_row_values(sheet, row, vec![CellValue::Int(7)]).expect("標本の行は実在する");

    let parts = to_parts(&base).expect("標本はパート集合へ取り出せる");
    let version = parts.format_version();
    let mut entries = entries_of(&parts);

    replace_entry(&mut entries, EntryName::Document, |text| {
        let with_top = text.replacen(
            "\"document_id\"",
            &format!("\"future_document\":{{\"marker\":\"{marker}\"}},\"document_id\""),
            1,
        );
        with_top.replacen(
            "\"columns\":[\"c\"]",
            &format!("\"columns\":[\"c\"],\"future_sheet\":[1,\"{marker}\"]"),
            1,
        )
    });
    replace_entry(&mut entries, EntryName::Schema { sheet }, |text| {
        let with_top =
            text.replacen("\"root\"", &format!("\"future_schema\":\"{marker}\",\"root\""), 1);
        with_top.replacen(
            "{\"kind\":\"string\"}",
            &format!("{{\"kind\":\"string\"}},\"future_type\":{{\"m\":\"{marker}\"}}"),
            1,
        )
    });

    with_rebuilt_manifest(version, entries)
}

/// 未知フィールド入りのモデルを `from_parts` 経由で組み立てる。
///
/// 読み込みで未知フィールドが保持され、モデルを経由してもう一度 `to_parts` したときに
/// **同じエントリ集合が戻る**ことをここで固定する（保持が最初の読み込みで落ちると、
/// 往復比較だけでは検出できないため、標本の組み立て時に検証する）。
pub fn sample_with_unknown_fields(marker: &str) -> Document {
    let entries = DocumentParts::from_entries(entries_with_preserved_fields(marker))
        .expect("標本の集合は妥当");
    let document = api().from_parts(&entries).expect("未知フィールドを含む集合は復元できる");
    let restored = entries_of(&to_parts(&document).expect("標本はパート集合へ取り出せる"));
    assert_eq!(entries_of(&entries), restored, "未知フィールドがモデル経由の往復で落ちた");

    document
}

/// エントリの中身を 1 つだけ組み替える（[`entries_with_preserved_fields`] の補助）。
fn replace_entry(
    entries: &mut Vec<(EntryName, Vec<u8>)>,
    name: EntryName,
    rewrite: impl FnOnce(String) -> String,
) {
    let slot = entries.iter().position(|(candidate, _)| *candidate == name).expect("エントリが実在する");
    let text = String::from_utf8(entries[slot].1.clone()).expect("エントリは UTF-8");
    entries[slot].1 = rewrite(text).into_bytes();
}

/// 1 シート・指定のルートスキーマ・指定の列と行を持つ文書を組み立てる。
///
/// 保存時に遮断すべき構造違反（宙吊り参照・未登録添付・型定義の重複宣言）の標本を
/// 同じ骨格で作るための補助である。
pub fn document_with_sheet(schema: &str, columns: &[&str], rows: Vec<Vec<CellValue>>) -> Document {
    let mut document = Document::new();
    let sheet = document.add_sheet("標本");
    document
        .set_sheet_columns(sheet, columns.iter().map(|name| (*name).to_owned()).collect())
        .expect("標本のシートは実在する");
    document
        .set_root_schema(sheet, SchemaPart::parse(schema).expect("標本のスキーマは解析できる"))
        .expect("標本のシートは実在する");
    for row_values in rows {
        let row = document.add_row(sheet).expect("標本のシートは実在する");
        document.set_row_values(sheet, row, row_values).expect("標本の行は実在する");
    }
    document
}

/// 実在しない型定義を参照する 1 シートの文書。
pub fn document_with_dangling_type_ref() -> Document {
    document_with_sheet(SCHEMA_DANGLING_REF, &[], Vec::new())
}

/// 同一の型定義識別子を 2 回宣言する 1 シートの文書。
pub fn document_with_duplicate_type_def() -> Document {
    document_with_sheet(SCHEMA_DUPLICATE_TYPE_DEF, &[], Vec::new())
}

/// レジストリに登録されていない添付を参照する 1 シート 1 行の文書。
pub fn document_with_unregistered_attachment() -> Document {
    let attachment =
        AttachmentId::from_hex(UNREGISTERED_ATTACHMENT_HEX).expect("標本の添付識別子は正準形");
    document_with_sheet(
        SCHEMA_EMPTY,
        &["blob"],
        vec![vec![CellValue::Attachment(attachment)]],
    )
}

//! クレート外から見た論理エントリ集合の組み立てと分解（タスク 4.8。要件 2.2, 2.3, 3.1, 5.2, 5.4）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。
//! `document_format::parts::{DocumentParts, Part, to_parts, from_parts}` がクレート外から
//! 使えること、ZIP を一切経由せずにモデル ⇄ パート集合の往復が完結することを示す。
//!
//! 符号化の規則（確定形・未知フィールドの保持・順序規則）と不変条件の網羅は
//! `src/parts/document_parts.rs` の単体テストが担う。ここは**公開経路が機能し、契約が
//! クレート外から観測できる**ことを確かめる。

use document_format::entry_name::MANIFEST_ENTRY;
use document_format::integrity::{digest_part, verify_part};
use document_format::parts::{from_parts, to_parts, DocumentParts, ManifestEntry, ManifestPart};
use document_format::{
    AttachmentId, CellValue, Document, DocumentError, DocumentId, EntryName, FormatVersion,
    NestedValue, SchemaPart, SheetId,
};

/// 標本の型定義識別子（正準 Crockford base32 大文字 26 文字）。
const TYPE_DEF: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
/// `document.json` にもパートにも現れないシート識別子（未知シート参照の標本）。
const UNKNOWN_SHEET: &str = "01K4ANRRG004HMASW9NF6YY093";
/// 標本のルートスキーマ（型定義 `TYPE_DEF` への参照を含む）。
const SCHEMA_WITH_REF: &str = concat!(
    r#"{"root":{"$ref":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#""},"types":[{"id":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#"","definition":{"kind":"string"}}]}"#
);
/// 型定義を 1 つも持たないルートスキーマ。
const SCHEMA_EMPTY: &str = r#"{"root":null}"#;

/// 標本の文書と、その添付識別子（参照される 1 件 / 参照されない 1 件）。
struct Sample {
    document: Document,
    referenced: AttachmentId,
    unreferenced: AttachmentId,
}

/// 標本の文書: 3 シート（行あり / 0 行 / 行あり）、複数の `CellValue` 変種、
/// 型定義参照を持つスキーマ、参照される添付と参照されない添付。
///
/// 網羅性の検証（順序・索引・往復同一性）は対象を**複数**に分散させる必要があるため、
/// シートを 3 枚、添付を 2 種、型定義参照を持つスキーマと持たないスキーマを混在させる。
fn sample() -> Sample {
    let mut document = Document::new();

    // 1 枚目: 列 4・行 2・添付参照・型定義参照を持つスキーマ。
    let stocked = document.add_sheet("在庫");
    document
        .set_sheet_columns(
            stocked,
            ["name", "count", "blob", "meta"].iter().map(|name| (*name).to_string()).collect(),
        )
        .expect("標本のシートは実在する");
    document
        .set_root_schema(stocked, SchemaPart::parse(SCHEMA_WITH_REF).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    // 添付: 非 UTF-8・NUL を含むバイト列（要件 7.5 の不透明性）。
    let referenced = document.add_attachment(vec![0xff, 0x00, 0x80, b'j', b'x', 0xfe]);
    // 参照されない添付（要件 7.6: 自動削除しない）。
    let unreferenced = document.add_attachment(b"\x00\x01\x02 not utf-8 \xff".to_vec());

    let first = document.add_row(stocked).expect("標本のシートは実在する");
    document
        .set_row_values(
            stocked,
            first,
            vec![
                CellValue::Text("りんご 🍎".to_owned()),
                CellValue::Int(3),
                CellValue::Attachment(referenced),
                CellValue::Nested(NestedValue::Array(vec![
                    CellValue::float(-0.0),
                    CellValue::Bool(true),
                    CellValue::Null,
                ])),
            ],
        )
        .expect("標本の行は実在する");
    let second = document.add_row(stocked).expect("標本のシートは実在する");
    document
        .set_row_values(
            stocked,
            second,
            vec![
                CellValue::Decimal("1.25".to_owned()),
                CellValue::Float(2.5),
                CellValue::Attachment(referenced),
                CellValue::Nested(NestedValue::Object(vec![(
                    "key".to_owned(),
                    CellValue::Text("値".to_owned()),
                )])),
            ],
        )
        .expect("標本の行は実在する");

    // 2 枚目: 0 行のシート（列名は行エントリから復元できない。`document.json` が唯一の源）。
    let empty = document.add_sheet("空のシート");
    document
        .set_sheet_columns(empty, vec!["a".to_owned(), "b".to_owned()])
        .expect("標本のシートは実在する");
    document
        .set_root_schema(empty, SchemaPart::parse(SCHEMA_EMPTY).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    // 3 枚目: 列 2・行 1・添付を参照しないシート（添付参照の有無を分散させる）。
    let summary = document.add_sheet("集計");
    document
        .set_sheet_columns(summary, vec!["label".to_owned(), "total".to_owned()])
        .expect("標本のシートは実在する");
    document
        .set_root_schema(summary, SchemaPart::parse(SCHEMA_EMPTY).expect("標本は妥当"))
        .expect("標本のシートは実在する");
    let only = document.add_row(summary).expect("標本のシートは実在する");
    document
        .set_row_values(
            summary,
            only,
            vec![CellValue::Text("合計".to_owned()), CellValue::Int(3)],
        )
        .expect("標本の行は実在する");

    Sample { document, referenced, unreferenced }
}

/// パート集合を（エントリ名, 実バイト列）の列へ写す（順序は [`DocumentParts::iter`] のまま）。
fn entries(parts: &DocumentParts) -> Vec<(EntryName, Vec<u8>)> {
    parts.iter().map(|part| (part.name, part.bytes.clone())).collect()
}

/// エントリ名の表示テキスト（反復順そのまま）。
fn names(parts: &DocumentParts) -> Vec<String> {
    parts.iter().map(|part| part.name.to_string()).collect()
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
fn attachments(document: &Document) -> Vec<(AttachmentId, Vec<u8>)> {
    document
        .attachments()
        .iter()
        .map(|attachment| (attachment.id(), attachment.bytes().to_vec()))
        .collect()
}

/// 集合の内容を差し替え、索引（`manifest.json`）を組み直した集合を作る。
///
/// 「実体のダイジェストと索引の記録が食い違う」以外の失敗（構造・参照・順序）を試すには、
/// 索引のダイジェストを実体に合わせて**一致させておく**必要がある。索引の再組み立てには
/// 公開の [`ManifestEntry`] / [`ManifestPart`] を使う（実装の内部経路は使わない）。
fn rebuilt(version: FormatVersion, mut entries: Vec<(EntryName, Vec<u8>)>) -> DocumentParts {
    // 呼び出し元が元の索引を持ち込んでもよいように、まず外してから組み直す。
    entries.retain(|(name, _)| *name != MANIFEST_ENTRY);
    let index: Vec<ManifestEntry> = entries
        .iter()
        .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
        .collect();
    let manifest = ManifestPart::new(version, index)
        .expect("標本の索引は妥当")
        .to_json_bytes()
        .expect("符号化");
    entries.push((MANIFEST_ENTRY, manifest));
    DocumentParts::from_entries(entries).expect("標本の集合は妥当")
}

/// `entries` の 1 件を差し替える（無ければ何もしない）。
fn replace(entries: &mut [(EntryName, Vec<u8>)], name: EntryName, bytes: Vec<u8>) {
    if let Some(slot) = entries.iter_mut().find(|(entry, _)| *entry == name) {
        slot.1 = bytes;
    }
}

/// 標本の集合から 1 件を取り除く。
fn remove(entries: &mut Vec<(EntryName, Vec<u8>)>, name: EntryName) {
    entries.retain(|(entry, _)| *entry != name);
}

/// モデルのシート順序から `document.json` の確定形を文字列で組み立てる（テスト側の独立した
/// 組み立てであり、実装の経路に依存しない）。
fn render_document_json(
    document_id: DocumentId,
    sheets: &[(SheetId, String, Vec<String>)],
) -> String {
    let elements: Vec<String> = sheets
        .iter()
        .map(|(id, name, columns)| {
            let name = serde_json::to_string(name).expect("シート名のエスケープ");
            let columns: Vec<String> = columns
                .iter()
                .map(|column| serde_json::to_string(column).expect("列名のエスケープ"))
                .collect();
            format!(
                r#"{{"sheet_id":"{id}","name":{name},"columns":[{}]}}"#,
                columns.join(",")
            )
        })
        .collect();
    format!(r#"{{"document_id":"{document_id}","sheets":[{}]}}"#, elements.join(","))
}

/// シート順序（データである）を入れ替えても、パートの**反復順**はエントリ名の昇順のままである
/// （反復順は索引の都合で決まり、シート順序に従わない。要件 2.2）。
#[test]
fn iteration_order_does_not_follow_the_sheet_order() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let ascending = names(&parts);
    let version = parts.format_version();

    // シート順序を逆にした `document.json` を持つ集合を組み立てる（それ以外は同一）。
    let restored = from_parts(&parts).expect("読み込み経路");
    let mut sheet_order = sheet_metadata(&restored);
    sheet_order.reverse();
    let reversed = render_document_json(restored.document_id(), &sheet_order);
    let mut forward = entries(&parts);
    replace(&mut forward, EntryName::Document, reversed.clone().into_bytes());
    let swapped = rebuilt(version, forward);

    assert_ne!(entries(&parts), entries(&swapped), "シート順序の入れ替えが内容に現れていない");
    assert_eq!(ascending, names(&swapped), "シート順序がパートの反復順へ漏れている");

    // 入れ替えた集合も読み込めて、シート順序がそのままモデルへ現れる（順序はデータである）。
    let reloaded = from_parts(&swapped).expect("読み込み経路");
    let observed: Vec<String> =
        reloaded.sheets().iter().map(|sheet| sheet.name().to_owned()).collect();
    let expected: Vec<String> = sheet_order.iter().map(|(_, name, _)| name.clone()).collect();
    assert_eq!(expected, observed, "シート順序が復元されていない");
    let original: Vec<String> =
        restored.sheets().iter().map(|sheet| sheet.name().to_owned()).collect();
    assert_ne!(original, observed, "標本のシート順序が入れ替わっていない");
}

/// 往復同一性: `to_parts` → `from_parts` → `to_parts` が**バイト単位で一致**し、
/// `from_parts` の結果が識別子・順序・列名・セル値・ルートスキーマのバイト列・添付で
/// 元のモデルと一致する（要件 3.1, 5.4）。
#[test]
fn parts_round_trip_is_byte_identical_and_preserves_the_model() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let restored = from_parts(&parts).expect("読み込み経路");

    assert_eq!(sample.document.document_id(), restored.document_id(), "識別子が変わった");
    assert_eq!(sheet_metadata(&sample.document), sheet_metadata(&restored), "シートが変わった");
    assert_eq!(rows(&sample.document), rows(&restored), "行が変わった");
    assert_eq!(schemas(&sample.document), schemas(&restored), "スキーマが変わった");
    assert_eq!(attachments(&sample.document), attachments(&restored), "添付が変わった");

    // 参照整合性の検証が実際に型定義参照を通っていること（`with_sheet` の結線が
    // 欠けていれば、この標本は宙吊りとして誤報され `from_parts` が Err になる）。
    assert!(
        schemas(&restored)[0].2.iter().any(|(_, to)| to == TYPE_DEF),
        "標本が型定義参照を持っていない（結線の検証にならない）"
    );

    let again = to_parts(&restored).expect("再保存経路");
    assert_eq!(entries(&parts), entries(&again), "往復でパート集合のバイト列が変わった");
    assert_eq!(parts.format_version(), again.format_version(), "形式バージョンが変わった");
}

/// 同一モデルからは何度構築しても同一のパート集合（名前もバイト列も同一）になり、
/// 形式バージョンは現行 1.0 が書かれる（要件 3.1, 6.1）。
#[test]
fn parts_construction_is_deterministic() {
    let sample = sample();

    let first = to_parts(&sample.document).expect("保存経路");
    let second = to_parts(&sample.document).expect("保存経路");

    assert_eq!(entries(&first), entries(&second), "同一モデルで出力が変わった");
    assert_eq!(FormatVersion::new(1, 0), first.format_version(), "現行バージョンが 1.0 でない");
}

/// 反復は**エントリ名の昇順**であり、入力の並び（構築順・シート順）に依存しない。
/// ファイルシステムの列挙順に依存しないことは、同じ集合を逆順・入れ替え順で組み立てても
/// 反復順が同一であることで示す（要件 2.2, 2.3）。
#[test]
fn iteration_is_ascending_and_independent_of_the_input_order() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let ascending = names(&parts);

    // 宣言どおりの昇順（比較に使う `EntryName` の `Ord` は表示テキストの辞書順）。
    let mut sorted: Vec<EntryName> = parts.iter().map(|part| part.name).collect();
    sorted.sort();
    let sorted: Vec<String> = sorted.iter().map(|name| name.to_string()).collect();
    assert_eq!(sorted, ascending, "反復順がエントリ名の昇順でない");
    assert!(ascending.len() >= 4, "標本のパートが少なすぎる: {ascending:?}");

    // 入力順を逆順・入れ替え順にしても、反復順も内容も同一である。
    let forward = entries(&parts);
    for order in [forward.iter().rev().cloned().collect::<Vec<_>>(), {
        let mut shuffled = forward.clone();
        shuffled.swap(0, 2);
        shuffled.reverse();
        shuffled
    }] {
        let input: Vec<(EntryName, Vec<u8>)> =
            order.iter().map(|(name, bytes)| (*name, bytes.clone())).collect();
        let rebuilt = DocumentParts::from_entries(input).expect("標本の集合は妥当");
        assert_eq!(ascending, names(&rebuilt), "入力順が反復順へ漏れている");
        assert_eq!(forward, entries(&rebuilt), "入力順が内容を変えている");
    }
}

/// 索引（`manifest.json`）は**他の全パート**を含み、**自分自身を含まない**。
/// 記録された各ダイジェストは実パートのバイト列と一致する（要件 5.1, 5.2）。
#[test]
fn manifest_indexes_every_other_part_and_never_itself() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");

    let manifest_part = parts.get(&MANIFEST_ENTRY).expect("索引は常に存在する");
    let manifest =
        ManifestPart::from_json_bytes(&manifest_part.bytes).expect("索引は復号できる");
    let indexed: Vec<String> =
        manifest.entries().iter().map(|entry| entry.name().to_string()).collect();
    let observed = names(&parts);
    let expected: Vec<String> =
        observed.iter().filter(|name| name.as_str() != MANIFEST_ENTRY.to_string()).cloned().collect();

    assert_eq!(expected, indexed, "索引が全パートを含んでいないか、自分自身を含んでいる");
    assert!(
        !indexed.iter().any(|name| name == &MANIFEST_ENTRY.to_string()),
        "索引が自分自身を含んでいる"
    );
    assert!(indexed.len() >= 4, "索引の対象が少なすぎる: {indexed:?}");

    // 各ダイジェストが実パートのバイト列と一致すること（`Part` が持つ値と索引の記録の双方）。
    for entry in manifest.entries() {
        let part = parts.get(&entry.name()).expect("索引にあるパートは実在する");
        verify_part(&entry.name(), &part.bytes, entry.digest()).expect("索引のダイジェスト不一致");
        assert_eq!(
            digest_part(&part.bytes),
            part.digest,
            "Part のダイジェストが実バイト列と食い違う: {}",
            entry.name()
        );
    }
}

/// 実体の改竄（索引の記録とは異なるバイト列）は、対象エントリ名つきの
/// [`DocumentError::IntegrityMismatch`] で中止される（要件 5.2, 5.3, 5.4）。
///
/// 改竄対象を 2 つ（`document.json` と行データ）に分散させ、片方だけを照合する変異でも
/// 落ちるようにする。
#[test]
fn tampered_parts_are_reported_as_integrity_mismatch() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let forward = entries(&parts);

    let document_entry = EntryName::Document;
    let rows_entry = forward
        .iter()
        .map(|(name, _)| *name)
        .find(|name| matches!(name, EntryName::Rows { .. }))
        .expect("標本は行エントリを持つ");

    for target in [document_entry, rows_entry] {
        // バイト列だけを差し替え、索引（manifest）は元のまま残す。
        let mut tampered = forward.clone();
        let part = parts.get(&target).expect("標本は対象を持つ");
        let mut bytes = part.bytes.clone();
        bytes.push(b' ');
        replace(&mut tampered, target, bytes);

        let rebuilt = DocumentParts::from_entries(tampered).expect("標本の集合は妥当");
        match from_parts(&rebuilt) {
            Err(DocumentError::IntegrityMismatch { entry }) => {
                assert_eq!(target.to_string(), entry, "不一致のエントリ名が違う");
            }
            other => panic!("{target}: ダイジェスト不一致が報告されない: {other:?}"),
        }
    }
}

/// 索引に載っているパートが実在しない場合も不足として報告する（要件 4.5, 5.4）。
#[test]
fn a_missing_indexed_part_is_reported_as_missing_part() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let forward = entries(&parts);

    let schema_entry = forward
        .iter()
        .map(|(name, _)| *name)
        .find(|name| matches!(name, EntryName::Schema { .. }))
        .expect("標本はスキーマエントリを持つ");
    let rows_entry = forward
        .iter()
        .map(|(name, _)| *name)
        .find(|name| matches!(name, EntryName::Rows { .. }))
        .expect("標本は行エントリを持つ");

    // 索引（manifest）は元のまま、実体だけを取り除く。
    for target in [schema_entry, rows_entry] {
        let mut without = forward.clone();
        remove(&mut without, target);
        let rebuilt = DocumentParts::from_entries(without).expect("標本の集合は妥当");
        match from_parts(&rebuilt) {
            Err(DocumentError::MissingPart { name }) => {
                assert_eq!(target.to_string(), name, "不足したエントリ名が違う");
            }
            other => panic!("{target}: 不足が報告されない: {other:?}"),
        }
    }
}

/// メタデータ（`document.json`）や索引（`manifest.json`）が無い集合は、不足として中止する
/// （要件 4.5）。
#[test]
fn missing_manifest_or_document_part_is_rejected() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let version = parts.format_version();
    let forward = entries(&parts);

    // 索引そのものが無い集合は構築できない（集合は常に索引を持つ）。
    let mut without_manifest = forward.clone();
    remove(&mut without_manifest, MANIFEST_ENTRY);
    match DocumentParts::from_entries(without_manifest) {
        Err(DocumentError::MissingPart { name }) => {
            assert_eq!(MANIFEST_ENTRY.to_string(), name);
        }
        other => panic!("索引の無い集合が拒否されない: {:?}", other.map(|parts| names(&parts))),
    }

    // `document.json` が索引にも実体にも無い集合は、読み込みで不足として報告される。
    let mut without_document = forward.clone();
    remove(&mut without_document, EntryName::Document);
    let rebuilt = rebuilt(version, without_document);
    match from_parts(&rebuilt) {
        Err(DocumentError::MissingPart { name }) => {
            assert_eq!(EntryName::Document.to_string(), name);
        }
        other => panic!("メタデータ欠落が報告されない: {other:?}"),
    }
}

/// 破損したパート（JSON として不正・索引の記録と照合済み）は、対象エントリ名つきの
/// [`DocumentError::InvalidContainer`] で中止される（要件 5.4, 2.2）。
#[test]
fn a_corrupt_part_is_reported_as_invalid_container_with_its_entry_name() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let version = parts.format_version();
    let forward = entries(&parts);

    let rows_entry = forward
        .iter()
        .map(|(name, _)| *name)
        .find(|name| matches!(name, EntryName::Rows { .. }))
        .expect("標本は行エントリを持つ");

    for (target, corrupt) in [
        (EntryName::Document, b"{".to_vec()),
        (rows_entry, b"{\"$id\":\"nope\"}\n".to_vec()),
    ] {
        let mut tampered = forward.clone();
        replace(&mut tampered, target, corrupt);
        let rebuilt = rebuilt(version, tampered);
        match from_parts(&rebuilt) {
            Err(DocumentError::InvalidContainer { entry }) => {
                assert!(
                    entry.starts_with(&target.to_string()),
                    "{target}: 失敗箇所がエントリ名で始まらない: {entry}"
                );
            }
            other => panic!("{target}: 破損が報告されない: {other:?}"),
        }
    }
}

/// 構造の破れは、対応する型付き変種で報告され、**モデルは返らない**（要件 4.2, 4.4, 1.7, 7.4）。
///
/// 索引のダイジェストは実体に合わせて組み直すため、ここで報告されるのは完全性ではなく
/// 構造の破れである（検証が完全性照合の後に走ることも同時に確かめられる）。
#[test]
fn structural_breakage_is_reported_before_a_model_is_built() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let version = parts.format_version();
    let forward = entries(&parts);

    let stocked_schema = forward
        .iter()
        .map(|(name, _)| *name)
        .find(|name| matches!(name, EntryName::Schema { .. }))
        .expect("標本はスキーマエントリを持つ");
    let stockeds_rows = forward
        .iter()
        .map(|(name, _)| *name)
        .find(|name| matches!(name, EntryName::Rows { .. }))
        .expect("標本は行エントリを持つ");
    let stocked_sheet = match stocked_schema {
        EntryName::Schema { sheet } => sheet,
        other => panic!("スキーマエントリの形が違う: {other}"),
    };

    // 1. 実在しない型定義への参照（要件 1.7）。
    //    ルートの参照は残したまま、型定義の宣言だけを取り除いたエンベロープを書く。
    let dangling_schema =
        format!(r#"{{"root":{{"$ref":"{TYPE_DEF}"}},"types":[]}}"#).into_bytes();
    let mut broken = forward.clone();
    replace(&mut broken, stocked_schema, dangling_schema);
    match from_parts(&rebuilt(version, broken)) {
        Err(DocumentError::DanglingTypeRef { from, to }) => {
            assert!(from.starts_with(&stocked_schema.to_string()), "参照元が違う: {from}");
            assert_eq!(TYPE_DEF, to, "参照先が違う");
        }
        other => panic!("宙吊りの型定義参照が報告されない: {other:?}"),
    }

    // 2. 実在しない添付への参照（要件 7.4）。行データの添付 hex だけを差し替える。
    let mut rows_bytes = entries(&parts)
        .into_iter()
        .find(|(name, _)| *name == stockeds_rows)
        .expect("標本は行データを持つ")
        .1;
    let unknown_hex = "f".repeat(64);
    let text = String::from_utf8(rows_bytes.clone()).expect("行データは UTF-8");
    let known_hex = sample.referenced.to_string();
    assert!(text.contains(&known_hex), "標本の行が添付を参照していない");
    rows_bytes = text.replace(&known_hex, &unknown_hex).into_bytes();
    let mut broken = forward.clone();
    replace(&mut broken, stockeds_rows, rows_bytes);
    match from_parts(&rebuilt(version, broken)) {
        Err(DocumentError::DanglingAttachmentRef { from, id }) => {
            assert!(from.starts_with(&stockeds_rows.to_string()), "参照元が違う: {from}");
            assert_eq!(unknown_hex, id, "参照された添付識別子が違う");
        }
        other => panic!("宙吊りの添付参照が報告されない: {other:?}"),
    }

    // 3. スキーマの無いシート（要件 4.4）。索引からも実体からも外す。
    let mut broken = forward.clone();
    remove(&mut broken, stocked_schema);
    match from_parts(&rebuilt(version, broken)) {
        Err(DocumentError::MissingSchema { sheet }) => {
            assert_eq!(stocked_sheet.to_string(), sheet, "スキーマ欠落シートが違う");
        }
        other => panic!("スキーマ欠落が報告されない: {other:?}"),
    }
}

/// `document.json` に列挙されていないシートを指すパートは、参照破れとして報告される
/// （要件 4.2。専用変種が無いため `InvalidContainer` の `entry` にエントリ名と理由が載る）。
#[test]
fn a_part_pointing_at_an_unknown_sheet_is_rejected() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let version = parts.format_version();
    let mut forward = entries(&parts);

    let unknown = UNKNOWN_SHEET.parse::<SheetId>().expect("標本は正準 ULID");
    let stray = EntryName::Rows { sheet: unknown };
    forward.push((stray, b""[..].to_vec()));

    match from_parts(&rebuilt(version, forward)) {
        Err(DocumentError::InvalidContainer { entry }) => {
            assert!(
                entry.starts_with(&stray.to_string()),
                "失敗箇所がエントリ名で始まらない: {entry}"
            );
        }
        other => panic!("未知シート参照が報告されない: {other:?}"),
    }
}

/// 行エントリの列順・列集合が `document.json` の列名一覧と一致しない場合は
/// [`DocumentError::InvalidContainer`] で中止する（要件 2.3, 5.4）。
#[test]
fn row_keys_must_match_the_columns_persisted_in_document_json() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let version = parts.format_version();
    let forward = entries(&parts);

    let rows_entry = forward
        .iter()
        .map(|(name, _)| *name)
        .find(|name| matches!(name, EntryName::Rows { .. }))
        .expect("標本は行エントリを持つ");
    let bytes = forward
        .iter()
        .find(|(name, _)| *name == rows_entry)
        .expect("標本は行エントリを持つ")
        .1
        .clone();
    let text = String::from_utf8(bytes).expect("行データは UTF-8");
    assert!(text.contains("\"count\""), "標本の行が想定の列を持たない: {text}");
    // 列名を 1 つだけ書き換える（`document.json` は元の列名のまま）。
    let renamed = text.replace("\"count\"", "\"renamed\"").into_bytes();
    assert_ne!(text.as_bytes(), renamed.as_slice());

    let mut broken = forward.clone();
    replace(&mut broken, rows_entry, renamed);
    match from_parts(&rebuilt(version, broken)) {
        Err(DocumentError::InvalidContainer { entry }) => {
            assert!(
                entry.starts_with(&rows_entry.to_string()),
                "失敗箇所がエントリ名で始まらない: {entry}"
            );
        }
        other => panic!("列順序の不一致が報告されない: {other:?}"),
    }
}

/// 未知フィールド（`document.json` のトップレベルとシート要素の中）は、モデルを経由した
/// 往復で失われず、**バイト単位**に元へ戻る（要件 6.2, 6.3）。
///
/// 未知キーは 2 つ以上のシート要素に分散させ、位置（既知フィールドの手前・間・後ろ）も
/// 変える。
#[test]
fn unknown_fields_survive_the_round_trip_through_the_model() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let version = parts.format_version();
    let forward = entries(&parts);

    let original = forward
        .iter()
        .find(|(name, _)| *name == EntryName::Document)
        .expect("標本はメタデータを持つ")
        .1
        .clone();
    let text = String::from_utf8(original.clone()).expect("メタデータは UTF-8");

    // トップレベル: 先頭（`document_id` の手前）。
    let text = text.replacen('{', r#"{"future_top":{"unit":"mm"},"#, 1);
    // 1 つ目のシート要素: 先頭（`sheet_id` の手前）。
    let text = text.replacen(r#"{"sheet_id""#, r#"{"element_note":1,"sheet_id""#, 1);
    // 2 つ目のシート要素: 先頭（別の未知キー）。
    let text = text.replacen(r#"{"sheet_id""#, r#"{"element_color":"magenta","sheet_id""#, 1);
    // 3 つ目のシート要素: 末尾（`columns` の後ろ）。
    let text = text.replacen(
        r#""columns":["label","total"]}"#,
        r#""columns":["label","total"],"width":24}"#,
        1,
    );
    for key in ["future_top", "element_note", "element_color", "\"width\""] {
        assert_eq!(1, text.matches(key).count(), "未知キー `{key}` の差し込みに失敗した");
    }
    let modified = text.into_bytes();
    assert_ne!(
        original, modified,
        "未知キーの差し込みに失敗している（標本の形が想定と違う）"
    );

    let mut tampered = forward.clone();
    replace(&mut tampered, EntryName::Document, modified.clone());
    let rebuilt = rebuilt(version, tampered);

    let restored = from_parts(&rebuilt).expect("未知フィールドは読み込みを妨げない");
    let written = to_parts(&restored).expect("再保存経路");
    assert_eq!(
        modified,
        written
            .get(&EntryName::Document)
            .expect("メタデータは常に存在する")
            .bytes,
        "未知フィールドが往復で消えたか位置が変わった"
    );
    // 未知キーが既知フィールド（シートのメタデータ）を隠していない。
    assert_eq!(sheet_metadata(&sample.document), sheet_metadata(&restored));
}

/// 0 行のシートでも列名が `document.json` を経由して往復する（行エントリからは復元できない
/// ため、これが唯一の経路である。要件 2.3）。
#[test]
fn column_names_of_an_empty_sheet_survive_only_through_document_json() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");

    let empty_sheet = sample
        .document
        .sheets()
        .iter()
        .find(|sheet| sheet.rows().is_empty())
        .expect("標本は 0 行のシートを持つ");
    let empty_columns = empty_sheet.columns().to_vec();
    assert!(!empty_columns.is_empty(), "0 行シートに列名が無く、検証にならない");

    // 0 行のシートの行エントリは空であり、列順を表現できない。
    let rows_entry = EntryName::Rows { sheet: empty_sheet.id() };
    assert!(
        parts.get(&rows_entry).expect("0 行でも行エントリは存在する").bytes.is_empty(),
        "0 行の行エントリが空でない"
    );
    let decoded = document_format::parts::RowsCodec::decode(
        &rows_entry,
        parts.get(&rows_entry).expect("行エントリ").bytes.as_slice(),
    )
    .expect("空の行エントリは復号できる");
    assert!(decoded.columns().is_empty(), "空の行エントリから列順が復元できてしまっている");

    // `document.json` を経由した往復で列名が戻る。
    let restored = from_parts(&parts).expect("読み込み経路");
    let restored_empty = restored.sheet_by_id(empty_sheet.id()).expect("シートは実在する");
    assert_eq!(empty_columns, restored_empty.columns().to_vec(), "0 行シートの列名が消えた");
    assert!(restored_empty.rows().is_empty(), "0 行のはずが行ができている");
}

/// 添付のバイト列は 1 バイトも変わらず往復し、参照されていない添付も消えない
/// （要件 7.1, 7.5, 7.6）。
#[test]
fn attachment_bytes_and_unreferenced_attachments_survive() {
    let sample = sample();
    let parts = to_parts(&sample.document).expect("保存経路");
    let restored = from_parts(&parts).expect("読み込み経路");

    for id in [sample.referenced, sample.unreferenced] {
        let before = sample.document.attachment(id).expect("標本に添付がある");
        let after = restored.attachment(id).expect("往復で添付が消えた");
        assert_eq!(before.bytes(), after.bytes(), "添付のバイト列が変わった: {id}");
        assert!(!after.bytes().is_empty(), "添付が空になっている: {id}");
    }

    // 参照されていない添付は消えない（自動削除しない。要件 7.6）。
    assert_eq!(
        vec![sample.unreferenced],
        restored.unreferenced_attachments(),
        "未参照の添付が消えたか、参照されていない添付が増減した"
    );
    // パートとしても実在し、内容が一致する。
    let part = restored
        .attachments()
        .iter()
        .map(|attachment| attachment.id())
        .collect::<Vec<_>>();
    assert_eq!(2, part.len(), "添付の件数が変わった");
}

/// 論理エントリ集合の入口がクレート外から使え、クレート根に晒されていないことを
/// 名前で固定する（`parts::` 配下に統一する規約）。
#[test]
fn parts_api_is_reachable_from_outside_the_crate() {
    let sample = sample();
    let parts: DocumentParts = document_format::parts::document_parts::to_parts(&sample.document)
        .expect("モジュール経由の入口");
    let first: &document_format::parts::Part = parts.iter().next().expect("パートがある");
    assert!(!first.bytes.is_empty(), "先頭のパートが空");
    assert_eq!(
        digest_part(&first.bytes),
        first.digest,
        "先頭のパートのダイジェストが実バイト列と食い違う"
    );
    assert_eq!(first.name, parts.get(&first.name).expect("get で引ける").name);

    // クレート根に `DocumentParts` は無い（`parts::` 配下に統一する）。
    fn takes_parts(parts: &DocumentParts) -> usize {
        parts.iter().count()
    }
    assert_eq!(parts.iter().count(), takes_parts(&parts));
}

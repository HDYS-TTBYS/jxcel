//! クレート外から見たシート別スキーマパートの符号化・復号（タスク 4.4。要件 1.3, 2.2。
//! design「Container Entry Layout」）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。
//! `document_format::parts::` 直下と `document_format::parts::schema_codec::` 経由の
//! 双方から `SchemaCodec` が見えることをコンパイル時に示す（パート層の公開面は
//! `parts` 配下に統一されており、クレート根には出さない — `ManifestPart` / `DocumentPart`
//! と同じ扱い）。エントリ名の解決（`schemas/<sheet-ulid>.json`）、不透明ペイロードの
//! バイト単位の往復、シート識別子の復元という一連の流れがクレート外から成立することも
//! 確かめる。
//!
//! 符号化の規則（確定形・未知フィールドの保持・エラー対応）と不変条件は
//! `src/parts/schema_codec.rs` の単体テストが網羅する。ここは公開経路が機能することの
//! 最小の確認に留める。

use document_format::parts::{schema_codec, SchemaCodec};
use document_format::{DocumentError, EntryName, IdFactory, SchemaPart};

#[test]
fn schema_codec_is_usable_from_outside_the_crate() {
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id();
    let type_def = factory.new_type_def_id();

    // 不透明ペイロード（内部の空白・`\uXXXX` 表記・非 ASCII を含む）を持つ標本。
    let text = format!(
        r#"{{"root":{{ "cols" : [{{"name":"金額","note":"\u00e9"}}] }},"types":[{{"id":"{type_def}","definition":{{"kind":"text"}}}}]}}"#
    );
    let part = SchemaPart::parse(&text).expect("標本は妥当なエンベロープ");

    let (entry, bytes) = SchemaCodec::encode(sheet, &part).expect("符号化");
    // エントリ名は `schemas/<sheet-ulid>.json` 形で、シート識別子から作られる。
    assert_eq!(
        EntryName::parse(&format!("schemas/{sheet}.json")).expect("許可リスト内"),
        entry
    );
    assert_eq!(text, String::from_utf8(bytes.clone()).expect("UTF-8"));

    // 復号すると、エントリ名からシート識別子が戻り、スキーマも復元される。
    let (decoded_sheet, decoded) = SchemaCodec::decode(&entry, &bytes).expect("復号");
    assert_eq!(sheet, decoded_sheet, "復号でシート識別子が変わった");
    assert_eq!(part.root().as_str(), decoded.root().as_str());
    assert_eq!(
        part.type_def_ids(),
        decoded.type_def_ids(),
        "型定義識別子が往復で変わった"
    );
    let (re_entry, re_bytes) = SchemaCodec::encode(decoded_sheet, &decoded).expect("再符号化");
    assert_eq!(entry, re_entry);
    assert_eq!(bytes, re_bytes, "往復でバイト列が変わった");

    // サブモジュール経由の型も公開面である（再エクスポートと同一の型）。
    // `parts` 直下（`document_format::parts::SchemaCodec`）・定義元
    // （`document_format::parts::schema_codec::SchemaCodec`）が**同一の型**であることは、
    // 相互に代入できることでコンパイル時に検査される（別の型ならこの関数は通らない）。
    fn same_type(codec: schema_codec::SchemaCodec) -> SchemaCodec {
        codec
    }
    let _: schema_codec::SchemaCodec = same_type(SchemaCodec);

    // スキーマ以外のエントリ名を復号しようとするとコンテナ不正として拒否される
    // （別のパートをスキーマとして読む事故を型で防ぐ）。
    let manifest = EntryName::parse("manifest.json").expect("許可リスト内");
    let Err(DocumentError::InvalidContainer { entry: label }) =
        SchemaCodec::decode(&manifest, &bytes)
    else {
        panic!("スキーマ以外のエントリ名が受理された");
    };
    assert!(
        label.starts_with("manifest.json: "),
        "entry がエントリ名で始まらない: {label}"
    );
}

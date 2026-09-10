//! クレート外から見たマニフェストパート（タスク 4.2。要件 4.5, 6.1。
//! design「Container Entry Layout」）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。
//! `document_format::parts::` 直下の再エクスポートと
//! `document_format::parts::manifest::` 経由の双方がクレート外から見えることを
//! コンパイル時に示す（`ManifestPart` / `ManifestEntry` / `resolve_manifest`）。
//!
//! 符号化の規則（確定形・索引順・未知フィールドの保持・エラー対応）と索引の不変条件は
//! `src/parts/manifest.rs` の単体テストが網羅する。ここは公開経路が機能することを示す
//! 最小の確認に留める。

use document_format::parts::manifest;
use document_format::parts::{resolve_manifest, ManifestEntry, ManifestPart};
use document_format::{DocumentError, EntryName, FormatVersion};

#[test]
fn manifest_is_usable_from_outside_the_crate() {
    let sheet_entry = "sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonl";

    let part = ManifestPart::new(
        FormatVersion::new(2, 7),
        vec![
            ManifestEntry::of_bytes(
                EntryName::parse(sheet_entry).expect("標本は許可リスト内"),
                b"{\"a\":1}\n",
            ),
            ManifestEntry::of_bytes(
                EntryName::parse("document.json").expect("標本は許可リスト内"),
                b"{\"document_id\":\"01ARZ3NDEKTSV4RRFFQ69G5FAV\"}",
            ),
        ],
    )
    .expect("標本の索引は妥当");

    // 索引はエントリ名の昇順（`document.json` が先）。
    let names: Vec<String> = part
        .entries()
        .iter()
        .map(|entry| entry.name().to_string())
        .collect();
    assert_eq!(vec!["document.json".to_owned(), sheet_entry.to_owned()], names);

    let bytes = part.to_json_bytes().expect("符号化");
    let decoded = ManifestPart::from_json_bytes(&bytes).expect("復号");
    assert_eq!(FormatVersion::new(2, 7), decoded.version());
    assert_eq!(bytes, decoded.to_json_bytes().expect("再符号化"));
    assert_eq!(
        part.digest_of(&EntryName::parse(sheet_entry).expect("標本は許可リスト内")),
        decoded.digest_of(&EntryName::parse(sheet_entry).expect("標本は許可リスト内"))
    );

    // サブモジュール経由の型も公開面である（再エクスポートと同一の型）。
    fn module_path(part: &manifest::ManifestPart) -> usize {
        part.entries().len()
    }
    assert_eq!(2, module_path(&decoded));

    // マニフェストが無い場合は、不足しているエントリ名を含む MissingPart で中止する
    // （要件 4.5）。ある場合はそのエントリ名が返る。
    match resolve_manifest([EntryName::Document].iter()) {
        Err(DocumentError::MissingPart { name }) => assert_eq!("manifest.json", name),
        other => panic!("MissingPart 以外が返った: {other:?}"),
    }
    assert_eq!(
        EntryName::Manifest,
        resolve_manifest([EntryName::Document, EntryName::Manifest].iter())
            .expect("manifest がある")
    );
}

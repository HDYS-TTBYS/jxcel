//! クレート外から見たドキュメントパート（タスク 4.3。要件 2.2, 3.6。
//! design「Container Entry Layout」）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。
//! `document_format::parts::` 直下の再エクスポートと
//! `document_format::parts::document_part::` 経由の双方がクレート外から見えることを
//! コンパイル時に示す（`DocumentPart` / `SheetMeta` / `DocumentId`）。ドキュメント識別子を
//! 発行し、保存し、復号する一連の流れがクレート外から成立することも確かめる。
//!
//! 符号化の規則（確定形・シート順序・未知フィールドの保持・エラー対応）と不変条件は
//! `src/parts/document_part.rs` の単体テストが網羅する。ここは公開経路が機能することの
//! 最小の確認に留める。

use document_format::parts::document_part;
use document_format::parts::{DocumentPart, SheetMeta};
use document_format::{DocumentError, DocumentId, IdFactory};

#[test]
fn document_part_is_usable_from_outside_the_crate() {
    let mut factory = IdFactory::new();
    let document_id = factory.new_document_id();
    // 空文字列を含むシート名もそのまま往復する（正規化しない）。
    let sheets = vec![
        SheetMeta::new(factory.new_sheet_id(), "売上".to_owned()),
        SheetMeta::new(factory.new_sheet_id(), String::new()),
    ];

    let part = DocumentPart::new(document_id, sheets).expect("標本は妥当");
    assert_eq!(document_id, part.document_id(), "ドキュメント識別子が変わった");
    assert_eq!("売上", part.sheets()[0].name());

    let bytes = part.to_json_bytes().expect("符号化");
    // 発行した識別子は正準テキスト形（26 文字 Crockford base32 大文字）で現れる。
    let text = String::from_utf8(bytes.clone()).expect("UTF-8");
    assert!(
        text.contains(&document_id.to_string()),
        "ドキュメント識別子が正準テキスト形で出力されていない: {text}"
    );

    let decoded = DocumentPart::from_json_bytes(&bytes).expect("復号");
    assert_eq!(bytes, decoded.to_json_bytes().expect("再符号化"));
    assert_eq!(document_id, decoded.document_id());
    let names: Vec<&str> = decoded.sheets().iter().map(SheetMeta::name).collect();
    assert_eq!(vec!["売上", ""], names, "シート順序かシート名が往復で変わった");

    // サブモジュール経由の型も公開面である（再エクスポートと同一の型）。
    fn module_path(part: &document_part::DocumentPart) -> usize {
        part.sheets().len()
    }
    // ドキュメント識別子の型もクレート外から名前で使える（クレート根の再エクスポート）。
    fn document_identifier(part: &document_part::DocumentPart) -> DocumentId {
        part.document_id()
    }
    assert_eq!(2, module_path(&decoded));
    assert_eq!(document_id, document_identifier(&decoded));

    // 同一ファイル内でシート識別子が重複する入力はコンテナ不正として拒否される。
    let sheet_id = factory.new_sheet_id();
    let duplicate = DocumentPart::new(
        part.document_id(),
        vec![
            SheetMeta::new(sheet_id, "重複".to_owned()),
            SheetMeta::new(sheet_id, "同じ識別子".to_owned()),
        ],
    );
    let Err(DocumentError::InvalidContainer { entry }) = duplicate else {
        panic!("重複したシート識別子が拒否されなかった");
    };
    assert!(
        entry.starts_with("document.json: "),
        "entry が document.json で始まらない: {entry}"
    );
}

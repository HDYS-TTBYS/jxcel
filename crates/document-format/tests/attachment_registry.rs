//! クレート外から見た添付レジストリと未参照集計(タスク 2.3。要件 7.1, 7.5, 7.6)。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。`document_format::`
//! 直下の再エクスポートと `document_format::model::` 経由の双方がクレート外から
//! 見えることをコンパイル時に示す(`Attachment` / `AttachmentRegistry` / `UnknownRow`)。
//!
//! 未参照添付の一覧(要件 7.6)は `Document::set_row_values` で行のセル値を設定し、
//! `Document::unreferenced_attachments` で観測する。参照は `CellValue::Attachment`
//! としてのみ現れ、実在検証(要件 7.4)は行わない。

use document_format::model::{Attachment, AttachmentRegistry, UnknownRow};
use document_format::{AttachmentId, CellValue, Document};

#[test]
fn attachment_registry_is_usable_from_outside_the_crate() {
    // 要件 7.1 / 7.5: 任意のバイト列(ここでは非 UTF-8)が 1 バイトも変わらずに往復する。
    let mut registry: AttachmentRegistry = AttachmentRegistry::new();
    let payload: Vec<u8> = vec![0xff, 0xfe, 0x00, 0x80];
    let id: AttachmentId = registry.add(payload.clone());
    let stored: &Attachment = registry.get(id).expect("登録済みの添付は取得できる");
    assert_eq!(payload, stored.bytes());
    assert!(registry.get(AttachmentId::from_bytes(b"never registered")).is_none());
}

#[test]
fn unreferenced_attachments_are_observable_through_the_public_api() {
    // 要件 7.6: 参照されている添付は未参照一覧に現れず、されていない添付は現れる。
    // さらに、一覧に出た添付も削除されず取得できる。
    let mut doc = Document::new();
    let sheet = doc.add_sheet("rows");
    let row = doc.add_row(sheet).unwrap();
    let referenced = doc.add_attachment(b"referenced".to_vec());
    let unreferenced = doc.add_attachment(vec![0x00, 0xff, 0x00]);

    doc.set_row_values(sheet, row, vec![CellValue::Attachment(referenced)]).unwrap();

    assert_eq!(vec![unreferenced], doc.unreferenced_attachments());
    assert!(doc.attachment(referenced).is_some(), "参照済みも保持され続ける");
    assert_eq!(
        vec![0x00, 0xff, 0x00],
        doc.attachment(unreferenced).unwrap().bytes(),
        "未参照でも削除されない"
    );

    // 2 枚目のシートからのみ参照される添付も参照済みとして扱われる(全シート走査)。
    let second = doc.add_sheet("second");
    let second_row = doc.add_row(second).unwrap();
    let only_from_second = doc.add_attachment(b"only from second".to_vec());
    doc.set_row_values(second, second_row, vec![CellValue::Attachment(only_from_second)])
        .unwrap();
    assert_eq!(
        vec![unreferenced],
        doc.unreferenced_attachments(),
        "2 枚目からの参照も集計される"
    );

    // 未知の行はローカルエラー型で報告される(panic しない)。
    let other_sheet = doc.add_sheet("other");
    let other_row = doc.add_row(other_sheet).unwrap();
    let outcome: Result<(), UnknownRow> =
        doc.set_row_values(sheet, other_row, vec![CellValue::Null]);
    match outcome {
        Err(UnknownRow { row: reported }) => assert_eq!(other_row, reported),
        Ok(()) => panic!("他シートの行を指定した設定は失敗しなければならない"),
    }
}

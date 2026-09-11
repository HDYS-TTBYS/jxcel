//! クレート外から見た添付レジストリと、添付の往復・参照整合性。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。`document_format::`
//! 直下の再エクスポートと `document_format::model::` 経由の双方がクレート外から
//! 見えることをコンパイル時に示す(`Attachment` / `AttachmentRegistry` / `UnknownRow`)。
//!
//! # タスク 2.3: レジストリと未参照集計(要件 7.1, 7.5, 7.6)
//!
//! 未参照添付の一覧(要件 7.6)は `Document::set_row_values` で行のセル値を設定し、
//! `Document::unreferenced_attachments` で観測する。参照は `CellValue::Attachment`
//! としてのみ現れ、実在検証(要件 7.4)は行わない。
//!
//! # タスク 8.8: 往復と参照整合性(要件 7.1, 7.3, 7.5, 7.6)
//!
//! 標本の網羅が要である: 空 / 1 バイト / NUL と高ビット(0x80–0xFF)を含むバイナリ /
//! JSON として解釈可能な並び / 非 ASCII の UTF-8 テキスト / 改行を含む並び / 1 MiB /
//! 同一内容の重複。これらの `save` → `open` の往復で**全件がバイト単位で一致**し、
//! 識別子が内容のハッシュと一致し続けることを確かめる。加えて、どの行からも参照
//! されない添付が削除されず一覧に現れること、行から添付への参照が ZIP 経由・非経由の
//! 両方で保たれること、添付エントリの名前が `attachments/<hex64>.bin` の規約に従い
//! `<hex64>` が内容のハッシュであることを確かめる。比較は `common::assert_same_document`
//! と**添付ごとのバイト比較の両方**で行う(前者だけでは、どの標本がどこで壊れたかを
//! 特定できない)。破損・改竄された添付エントリの拒否(要件 7.2 の逆)はタスク 8.4 が
//! 担うため、ここでは正常系に絞る(重複させない)。
//!
//! # 「再圧縮しない」の意味(要件 7.5。親の裁定)
//!
//! 要件 7.5 は「添付エントリの内容を解釈、変換、または再圧縮しない」である。本形式は
//! **コンテナ全体を 1 つの ZIP として書く**ため、添付エントリも他のパートと同じく
//! `Deflate` で格納される(型マーカー `jxcel` だけが `Stored`)。これは**コンテナ層の
//! 透過で可逆な圧縮**であり、本モジュールが添付の内容に対して行う解釈・変換ではない。
//! したがって検証するのは次の 3 点である: (1) `save` → `open` を経た添付のバイト列が
//! 元と 1 バイトも変わらない(展開後に元が得られる = 可逆)、(2) 識別子(内容ハッシュ)が
//! 内容と一致し続ける、(3) 画像や特定形式を特別扱いする経路が無く、任意のバイト列を
//! そのまま運べる。生の ZIP ヘッダの圧縮方式を観測するテストは `tests/container_writer.rs`
//! が担い、「マーカーだけ `Stored`・他は `Deflate`」という確定形の範囲に留める。
//!
//! 一時ファイルはリポジトリ内の `tests/scratch_*` に作り、`Drop` で削除する
//! (`tests/common/mod.rs` の docs 参照)。

mod common;

use std::collections::BTreeSet;

use document_format::container::ContainerCodec;
use document_format::model::{Attachment, AttachmentRegistry, UnknownRow};
use document_format::{
    AttachmentId, CellValue, Document, DocumentFormatApi, EntryName, NestedValue,
};

use common::{api, assert_same_document, entries_of, Scratch};

#[test]
fn attachment_registry_is_usable_from_outside_the_crate() {
    // 要件 7.1 / 7.5: 任意のバイト列(ここでは非 UTF-8)が 1 バイトも変わらずに往復する。
    let mut registry: AttachmentRegistry = AttachmentRegistry::new();
    let payload: Vec<u8> = vec![0xff, 0xfe, 0x00, 0x80];
    let id: AttachmentId = registry.add(payload.clone());
    let stored: &Attachment = registry.get(id).expect("登録済みの添付は取得できる");
    assert_eq!(payload, stored.bytes());
    assert!(registry
        .get(AttachmentId::from_bytes(b"never registered"))
        .is_none());
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

    doc.set_row_values(sheet, row, vec![CellValue::Attachment(referenced)])
        .unwrap();

    assert_eq!(vec![unreferenced], doc.unreferenced_attachments());
    assert!(
        doc.attachment(referenced).is_some(),
        "参照済みも保持され続ける"
    );
    assert_eq!(
        vec![0x00, 0xff, 0x00],
        doc.attachment(unreferenced).unwrap().bytes(),
        "未参照でも削除されない"
    );

    // 2 枚目のシートからのみ参照される添付も参照済みとして扱われる(全シート走査)。
    let second = doc.add_sheet("second");
    let second_row = doc.add_row(second).unwrap();
    let only_from_second = doc.add_attachment(b"only from second".to_vec());
    doc.set_row_values(
        second,
        second_row,
        vec![CellValue::Attachment(only_from_second)],
    )
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

/// 往復で観測する添付の標本(要件 7.1 / 7.5 の任意のバイト列)。
///
/// 標本は 1 つずつ性質を担う。どれか 1 つが落ちれば、どの種類のバイト列で往復が
/// 壊れたかを特定できる:
///
/// * `empty` / `one_byte` — 下限(0 バイトと 1 バイト)。
/// * `binary` — NUL と高ビット(0x80–0xFF)を含む非 UTF-8。
/// * `json_like` — JSON として解釈可能な並び(特別扱いが無いこと)。
/// * `utf8` — 非 ASCII の UTF-8 テキスト。
/// * `newlines` — 改行を含む並び(行区切りの解釈が無いこと)。
/// * `large` — 1 MiB(大きさで経路が分岐しないこと。実行時間の実測対象)。
/// * `duplicate_of_binary` — `binary` と同一内容(content-addressed の畳み込み)。
fn samples() -> Vec<(&'static str, Vec<u8>)> {
    let binary = vec![0x00, 0xff, 0x80, 0x7f, 0xfe, 0x00, 0xa5];
    let mut large = Vec::with_capacity(1024 * 1024);
    // 決定的なパターン(0..251 の循環)。1 MiB の標本は実行時間の実測対象である。
    large.extend((0..1024 * 1024).map(|i| (i % 251) as u8));
    vec![
        ("empty", Vec::new()),
        ("one_byte", vec![0x00]),
        ("binary", binary.clone()),
        ("json_like", br#"{"a":1}"#.to_vec()),
        ("utf8", "日本語テキスト 🍎 café".as_bytes().to_vec()),
        ("newlines", b"line1\nline2\r\nline3".to_vec()),
        ("large", large),
        ("duplicate_of_binary", binary),
    ]
}

/// 標本のうち**内容が異なる**ものだけを登録順で返す(畳み込みの期待値)。
fn distinct_payloads() -> Vec<Vec<u8>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (_, bytes) in samples() {
        if seen.insert(AttachmentId::from_bytes(&bytes)) {
            out.push(bytes);
        }
    }
    out
}

/// 添付のバイト列が `save` → `open` の往復で 1 バイトも変わらない(要件 7.1, 7.5)。
///
/// 標本を網羅し、`common::assert_same_document` によるモデル全体の同一性と、
/// 添付ごとのバイト比較・識別子の一致の**両方**で確かめる。同一内容の重複が
/// 1 件へ畳まれること(content-addressed。要件 7.2)も往復後まで保たれる。
#[test]
fn attachment_payloads_survive_the_file_roundtrip_byte_for_byte() {
    let scratch = Scratch::new("attachment_payloads");
    let mut before = Document::new();
    let sheet = before.add_sheet("添付");
    before
        .set_sheet_columns(sheet, vec!["ref".to_owned()])
        .expect("列を設定できる");
    let row = before.add_row(sheet).expect("行を追加できる");

    // 同一内容(binary と duplicate_of_binary)を 2 回登録する。content-addressed なので
    // 1 件に畳まれ、返る識別子は同一である(要件 7.2)。
    let mut first = None;
    for (_, bytes) in samples() {
        let id = before.add_attachment(bytes);
        first = first.or(Some(id));
    }
    let referenced = first.expect("標本は非空");
    before
        .set_row_values(sheet, row, vec![CellValue::Attachment(referenced)])
        .expect("参照を設定できる");

    // 登録の時点で 8 回の登録が 7 件の異なる内容へ畳まれている。
    let distinct = distinct_payloads();
    assert_eq!(
        distinct.len(),
        before.attachments().len(),
        "同一内容が別エントリになった"
    );

    let path = scratch.file("attachments.jxcel");
    api().save(&before, &path).expect("保存できる");
    let after = api().open(&path).expect("開ける").document;

    assert_eq!(
        distinct.len(),
        after.attachments().len(),
        "往復で添付の件数が変わった"
    );

    // 全標本がバイト単位で一致し、識別子が内容のハッシュと一致し続ける(要件 7.1, 7.2, 7.5)。
    for (name, bytes) in samples() {
        let id = AttachmentId::from_bytes(&bytes);
        let stored = after
            .attachment(id)
            .unwrap_or_else(|| panic!("{name}: 往復で添付が落ちた"));
        assert_eq!(
            bytes,
            stored.bytes(),
            "{name}: 往復でバイト列が 1 バイトでも変わった"
        );
        assert_eq!(id, stored.id(), "{name}: 添付の識別子が内容と一致しない");
        assert_eq!(
            id,
            AttachmentId::from_bytes(stored.bytes()),
            "{name}: 識別子が内容からずれた"
        );
    }

    // 添付以外の経路(行・参照の持ち方)を含めたモデル全体の同一性。
    assert_same_document(&before, &after);
}

/// 参照される添付と参照されない添付が、往復後も区別されて両方残る(要件 7.3, 7.6)。
///
/// 未参照の添付は削除されず `unreferenced_attachments` に現れ、参照済みの添付は
/// 一覧に現れない。どちらも `Document::attachment` でバイト列を取得できる。
/// ZIP 経由(`save` → `open`)と非経由(`to_parts` → `from_parts`)の両方で確かめる。
#[test]
fn referenced_and_unreferenced_attachments_stay_distinct_through_the_roundtrip() {
    let scratch = Scratch::new("attachment_unreferenced");
    let mut before = Document::new();
    let sheet = before.add_sheet("参照");
    before
        .set_sheet_columns(sheet, vec!["ref".to_owned()])
        .expect("列を設定できる");
    let row = before.add_row(sheet).expect("行を追加できる");

    let referenced = before.add_attachment(b"referenced".to_vec());
    let unreferenced = before.add_attachment(vec![0x00, 0xff, 0x00, 0x80]);
    before
        .set_row_values(sheet, row, vec![CellValue::Attachment(referenced)])
        .expect("参照を設定できる");

    // 前提: 片方だけが未参照である。
    assert_eq!(vec![unreferenced], before.unreferenced_attachments());
    assert!(
        !before.unreferenced_attachments().contains(&referenced),
        "参照済みが未参照一覧に現れた"
    );
    assert!(
        before.attachment(unreferenced).is_some(),
        "未参照でも保持される"
    );

    // ZIP 経由(save → open)。
    let path = scratch.file("mixed.jxcel");
    api().save(&before, &path).expect("保存できる");
    let via_file = api().open(&path).expect("開ける").document;
    // ZIP を経由しない経路(to_parts → from_parts)。
    let zip_free = api()
        .from_parts(&api().to_parts(&before).expect("取り出せる"))
        .expect("ZIP 非経由で復元できる");

    for (path, after) in [("file", &via_file), ("parts", &zip_free)] {
        assert_eq!(
            before.unreferenced_attachments(),
            after.unreferenced_attachments(),
            "{path}: 未参照添付の一覧が変わった"
        );
        assert!(
            !after.unreferenced_attachments().contains(&referenced),
            "{path}: 参照済みの添付が未参照一覧に現れた"
        );
        let stored = after
            .attachment(unreferenced)
            .unwrap_or_else(|| panic!("{path}: 未参照添付が削除された"));
        assert_eq!(
            vec![0x00, 0xff, 0x00, 0x80],
            stored.bytes(),
            "{path}: 未参照添付のバイト列が変わった"
        );
        assert_eq!(
            unreferenced,
            stored.id(),
            "{path}: 未参照添付の識別子が変わった"
        );
        assert!(
            after.attachment(referenced).is_some(),
            "{path}: 参照済みの添付が保持されていない"
        );
    }

    assert_same_document(&before, &via_file);
    assert_same_document(&before, &zip_free);
}

/// 行から添付への参照が、ZIP 経由・非経由の両方で同じ識別子のまま保たれる(要件 7.3)。
///
/// 直接セル(`CellValue::Attachment`)と入れ子(`NestedValue::Array` の要素)の両方で
/// 参照し、往復後のセル値が元と等しいこと、参照先のバイト列が `Document::attachment`
/// で取得できることを確かめる。
#[test]
fn attachment_references_are_preserved_on_both_paths() {
    let mut before = Document::new();
    let sheet = before.add_sheet("参照");
    before
        .set_sheet_columns(sheet, vec!["direct".to_owned(), "nested".to_owned()])
        .expect("列を設定できる");
    let first = before.add_attachment(vec![0x00, 0xff, 0x80, 0x01]);
    let second = before.add_attachment("二番目の添付 🍎".as_bytes().to_vec());

    let row = before.add_row(sheet).expect("行を追加できる");
    let nested = CellValue::Nested(NestedValue::Array(vec![
        CellValue::Text("前".to_owned()),
        CellValue::Attachment(second),
    ]));
    before
        .set_row_values(
            sheet,
            row,
            vec![CellValue::Attachment(first), nested.clone()],
        )
        .expect("参照を設定できる");

    let parts = api().to_parts(&before).expect("取り出せる");
    let zip_free = api().from_parts(&parts).expect("ZIP 非経由で復元できる");
    let via_zip = api()
        .from_parts(
            &ContainerCodec::decode(&ContainerCodec::encode(&parts).expect("符号化できる"))
                .expect("復号できる"),
        )
        .expect("ZIP 経由で復元できる");

    for (path, after) in [("zip_free", &zip_free), ("via_zip", &via_zip)] {
        assert_same_document(&before, after);
        let values = after.sheets()[0].rows()[0].values();
        assert_eq!(
            &CellValue::Attachment(first),
            &values[0],
            "{path}: 直接参照が別の識別子になった"
        );
        assert_eq!(
            &nested, &values[1],
            "{path}: 入れ子内の参照が保たれなかった"
        );
        assert_eq!(
            vec![0x00, 0xff, 0x80, 0x01].as_slice(),
            after.attachment(first).expect("参照先を取得できる").bytes(),
            "{path}: 参照先のバイト列が変わった"
        );
        assert_eq!(
            second,
            after.attachment(second).expect("参照先を取得できる").id(),
            "{path}: 参照先の識別子が変わった"
        );
    }
}

/// 添付エントリの名前が内容のハッシュから作られる(要件 7.2, 7.5)。
///
/// `to_parts` のエントリ名を観測し、`attachments/<hex64>.bin` の `<hex64>` が
/// **内容のハッシュ**の 16 進表現であること、同一内容が別エントリとして書かれない
/// ことを確かめる(改竄されたエントリの拒否はタスク 8.4 が担う)。
#[test]
fn attachment_entry_names_are_content_addressed() {
    let mut document = Document::new();
    let sheet = document.add_sheet("添付");
    document
        .set_sheet_columns(sheet, Vec::<String>::new())
        .expect("列を設定できる");
    for (_, bytes) in samples() {
        document.add_attachment(bytes);
    }

    let parts = api().to_parts(&document).expect("取り出せる");
    let mut observed = Vec::new();
    for (name, bytes) in entries_of(&parts) {
        if let EntryName::Attachment { attachment } = name {
            let id = AttachmentId::from_bytes(&bytes);
            assert_eq!(
                attachment, id,
                "エントリ名の識別子が内容のハッシュと一致しない"
            );
            assert_eq!(
                format!("attachments/{}.bin", attachment.to_hex()),
                name.to_string(),
                "エントリ名が attachments/<hex64>.bin の規約から外れた"
            );
            observed.push((attachment, bytes));
        }
    }

    let distinct = distinct_payloads();
    assert_eq!(
        distinct.len(),
        observed.len(),
        "同一内容が別エントリとして書かれた"
    );
    // 観測した識別子の集合が標本の相異なる内容のハッシュ集合と一致する。
    let mut observed_ids: Vec<AttachmentId> = observed.iter().map(|(id, _)| *id).collect();
    observed_ids.sort();
    let mut expected_ids: Vec<AttachmentId> = distinct
        .iter()
        .map(|bytes| AttachmentId::from_bytes(bytes))
        .collect();
    expected_ids.sort();
    assert_eq!(expected_ids, observed_ids);
    // 各エントリのバイト列は名前が指す内容そのものである。
    for (id, bytes) in &observed {
        assert_eq!(bytes, document.attachment(*id).expect("登録済み").bytes());
    }
}

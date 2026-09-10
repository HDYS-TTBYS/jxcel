//! クレート外から見た構造検証（タスク 4.6。要件 1.4, 4.3, 4.4。design「Components and
//! Interfaces」の StructuralValidator）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。
//! `document_format::parts::` 直下の再エクスポートと `document_format::parts::validate::`
//! 経由の双方がクレート外から見えることをコンパイル時に示す（`StructuralValidator` /
//! `PartInventory` / `IdDeclaration`）。検証の規則・入力の形・報告順の定義は
//! `src/parts/validate.rs` のモジュール docs にあり、ここはその**観測可能な振る舞い**を
//! 固定する。
//!
//! 標本は 4 種別（シート / 行 / 型定義 / 添付）に分散させ、複数の違反を同時に持つ目録も
//! 使う。1 種別・1 違反の標本だけでは「種別ごとの検査」や「すべての出現箇所の報告」の
//! 回帰を検出できないためである。

use document_format::parts::validate;
use document_format::parts::{IdDeclaration, PartInventory, StructuralValidator};
use document_format::{AttachmentId, DocumentError, IdFactory, IdKind};

/// 1 種別・1 識別子と、与えた出現箇所だけを持つ目録を組み立てる。
fn inventory_of(kind: IdKind, id: &str, locations: &[impl AsRef<str>]) -> PartInventory {
    let mut inventory = PartInventory::new();
    for location in locations {
        inventory.declare(IdDeclaration::new(kind, id, location.as_ref()));
    }
    inventory
}

/// `DuplicateId` から（種別, 識別子, 出現箇所）を取り出す。
fn duplicate(error: DocumentError) -> (IdKind, String, Vec<String>) {
    match error {
        DocumentError::DuplicateId { kind, id, occurrences } => (kind, id, occurrences),
        other => panic!("DuplicateId を期待したが {other:?} だった"),
    }
}

/// `MissingSchema` からシート識別子を取り出す。
fn missing_schema(error: DocumentError) -> String {
    match error {
        DocumentError::MissingSchema { sheet } => sheet,
        other => panic!("MissingSchema を期待したが {other:?} だった"),
    }
}

// ---------------------------------------------------------------------------
// 識別子の一意性（要件 4.3）
// ---------------------------------------------------------------------------

#[test]
fn a_duplicated_sheet_identifier_is_reported() {
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let inventory = inventory_of(
        IdKind::Sheet,
        &sheet,
        &["document.json sheets[0]", "document.json sheets[2]"],
    );

    let (kind, id, occurrences) = duplicate(
        StructuralValidator::validate(&inventory).expect_err("重複したシート識別子が通った"),
    );
    assert_eq!(IdKind::Sheet, kind, "種別が Sheet でない");
    assert_eq!(sheet, id, "重複した識別子が報告されていない");
    assert_eq!(
        vec!["document.json sheets[0]".to_owned(), "document.json sheets[2]".to_owned()],
        occurrences,
        "出現箇所が報告されていない",
    );
}

#[test]
fn every_occurrence_of_a_duplicated_row_identifier_is_reported() {
    let mut factory = IdFactory::new();
    let row = factory.new_row_id().to_string();
    let first = factory.new_sheet_id().to_string();
    let second = factory.new_sheet_id().to_string();
    let third = factory.new_sheet_id().to_string();
    // 出現箇所は目録に与えた順のまま報告される（ソートされない）。シートのテキスト順と
    // 逆順に並べ、行番号も降順にして「昇順へソートする」変異を検出できるようにする
    // （`first < second < third` は単調カウンタが保証する）。
    let inventory = inventory_of(
        IdKind::Row,
        &row,
        &[
            format!("sheets/{third}.jsonl line 9"),
            format!("sheets/{second}.jsonl line 3"),
            format!("sheets/{first}.jsonl line 12"),
        ],
    );

    let (kind, id, occurrences) = duplicate(
        StructuralValidator::validate(&inventory).expect_err("重複した行識別子が受け入れられた"),
    );
    assert_eq!(IdKind::Row, kind, "種別が Row でない");
    assert_eq!(row, id, "重複した識別子が報告されていない");
    assert_eq!(
        vec![
            format!("sheets/{third}.jsonl line 9"),
            format!("sheets/{second}.jsonl line 3"),
            format!("sheets/{first}.jsonl line 12"),
        ],
        occurrences,
        "3 箇所すべての出現箇所が目録の順で報告されていない",
    );
}

#[test]
fn a_row_identifier_duplicated_across_two_sheets_is_reported() {
    // 行識別子の一意性はシート単位ではなくドキュメント全体で課される（要件 1.4）。
    let mut factory = IdFactory::new();
    let row = factory.new_row_id().to_string();
    let first = factory.new_sheet_id().to_string();
    let second = factory.new_sheet_id().to_string();
    let inventory = inventory_of(
        IdKind::Row,
        &row,
        &[format!("sheets/{first}.jsonl line 1"), format!("sheets/{second}.jsonl line 1")],
    );

    let (kind, id, occurrences) = duplicate(StructuralValidator::validate(&inventory).expect_err(
        "別シート間の行識別子の重複が受け入れられた（シート単位に閉じた検査になっている）",
    ));
    assert_eq!(IdKind::Row, kind);
    assert_eq!(row, id);
    assert_eq!(2, occurrences.len(), "2 シート分の出現箇所が報告されていない");
}

#[test]
fn a_duplicated_type_definition_identifier_is_reported() {
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let type_def = factory.new_type_def_id().to_string();
    let inventory = inventory_of(
        IdKind::TypeDef,
        &type_def,
        &[
            format!("schemas/{sheet}.json types[0]"),
            format!("schemas/{sheet}.json types[3]"),
        ],
    );

    let (kind, id, occurrences) = duplicate(StructuralValidator::validate(&inventory).expect_err(
        "重複した型定義識別子が受け入れられた",
    ));
    assert_eq!(IdKind::TypeDef, kind, "種別が TypeDef でない");
    assert_eq!(type_def, id, "重複した識別子が報告されていない");
    assert_eq!(2, occurrences.len());
}

#[test]
fn a_duplicated_attachment_identifier_is_reported() {
    // 添付識別子は内容から決まる（content-addressed）ため、宣言箇所はエントリ名そのもので
    // ある。同じ添付を 2 度宣言した目録を拒否する。
    let attachment = AttachmentId::from_bytes(b"same bytes").to_string();
    let entry = format!("attachments/{attachment}.bin");
    let inventory =
        inventory_of(IdKind::Attachment, &attachment, &[entry.as_str(), entry.as_str()]);

    let (kind, id, occurrences) = duplicate(
        StructuralValidator::validate(&inventory).expect_err("重複した添付識別子が受け入れられた"),
    );
    assert_eq!(IdKind::Attachment, kind, "種別が Attachment でない");
    assert_eq!(attachment, id, "重複した識別子が報告されていない");
    assert_eq!(vec![entry.clone(), entry], occurrences);
}

#[test]
fn the_same_text_in_different_identifier_kinds_is_not_a_duplicate() {
    // 一意性は種別ごとである（要件 1.4 の 4 体系）。種別を無視して識別子テキストだけで
    // 比較すると、この目録が誤って重複として報告される。
    let text = IdFactory::new().new_row_id().to_string();
    let mut inventory = PartInventory::new();
    inventory.declare(IdDeclaration::new(IdKind::Sheet, &text, "document.json sheets[0]"));
    inventory.declare(IdDeclaration::new(IdKind::Row, &text, "sheets/row.jsonl line 1"));
    inventory.declare(IdDeclaration::new(IdKind::TypeDef, &text, "schemas/row.json types[0]"));
    inventory.declare(IdDeclaration::new(IdKind::Attachment, &text, "attachments/row.bin"));

    StructuralValidator::validate(&inventory).expect("種別が違えば識別子の一意性は破られていない");
}

#[test]
fn an_empty_inventory_passes() {
    StructuralValidator::validate(&PartInventory::new()).expect("空の目録は違反なし");
}

#[test]
fn a_complete_unique_inventory_passes() {
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let row = factory.new_row_id().to_string();
    let type_def = factory.new_type_def_id().to_string();
    let attachment = AttachmentId::from_bytes(b"payload").to_string();

    let mut inventory = PartInventory::new();
    inventory.declare(IdDeclaration::new(IdKind::Sheet, &sheet, "document.json sheets[0]"));
    inventory.declare(IdDeclaration::new(
        IdKind::Row,
        &row,
        format!("sheets/{sheet}.jsonl line 1"),
    ));
    inventory.declare(IdDeclaration::new(
        IdKind::TypeDef,
        &type_def,
        format!("schemas/{sheet}.json types[0]"),
    ));
    inventory.declare(IdDeclaration::new(
        IdKind::Attachment,
        &attachment,
        format!("attachments/{attachment}.bin"),
    ));
    inventory.require_schema(&sheet);
    inventory.provide_schema(&sheet);

    StructuralValidator::validate(&inventory).expect("識別子が一意でスキーマも揃った目録は通る");
}

#[test]
fn blank_identifier_and_location_texts_are_reported_without_panicking() {
    // 目録のテキストは本検証にとって不透明である（解釈しない）。空文字列でも panic せず
    // 同じ規則で報告する。
    let inventory = inventory_of(IdKind::Row, "", &["", ""]);

    let (kind, id, occurrences) =
        duplicate(StructuralValidator::validate(&inventory).expect_err("空テキストの重複"));
    assert_eq!(IdKind::Row, kind);
    assert_eq!(String::new(), id);
    assert_eq!(2, occurrences.len());
}

// ---------------------------------------------------------------------------
// 報告順と決定性
// ---------------------------------------------------------------------------

#[test]
fn validation_is_deterministic_and_reports_the_smallest_kind_then_text() {
    let mut factory = IdFactory::new();
    // シート識別子を最後に発行する（単調カウンタなので行・型定義のテキストより大きい）。
    // 種別優先なら Sheet、識別子テキストの昇順優先なら Row が報告される標本である。
    let row = factory.new_row_id().to_string();
    let type_def = factory.new_type_def_id().to_string();
    let sheet = factory.new_sheet_id().to_string();
    let attachment = AttachmentId::from_bytes(b"blob").to_string();
    // 宣言順は報告順と逆にしてある（報告順は種別 → 識別子テキストで決まる）。
    let build = || {
        let mut inventory = PartInventory::new();
        inventory.declare(IdDeclaration::new(
            IdKind::Attachment,
            &attachment,
            "attachments/blob.bin",
        ));
        inventory.declare(IdDeclaration::new(
            IdKind::Attachment,
            &attachment,
            "attachments/blob.bin",
        ));
        inventory.declare(IdDeclaration::new(
            IdKind::TypeDef,
            &type_def,
            "schemas/s.json types[0]",
        ));
        inventory.declare(IdDeclaration::new(
            IdKind::TypeDef,
            &type_def,
            "schemas/s.json types[1]",
        ));
        inventory.declare(IdDeclaration::new(IdKind::Row, &row, "sheets/s.jsonl line 1"));
        inventory.declare(IdDeclaration::new(IdKind::Row, &row, "sheets/s.jsonl line 2"));
        inventory.declare(IdDeclaration::new(IdKind::Sheet, &sheet, "document.json sheets[0]"));
        inventory.declare(IdDeclaration::new(IdKind::Sheet, &sheet, "document.json sheets[1]"));
        inventory
    };

    let inventory = build();
    let expected = (
        IdKind::Sheet,
        sheet.clone(),
        vec!["document.json sheets[0]".to_owned(), "document.json sheets[1]".to_owned()],
    );
    // 4 種すべてが重複している目録では、種別順の先頭（Sheet）が報告される。
    let first = duplicate(StructuralValidator::validate(&inventory).expect_err("4 種すべて重複"));
    assert_eq!(expected, first, "報告順が種別順でない");
    // 同じ目録を 2 度検証しても同じエラーが返る（ハッシュ反復順に依存しない）。
    let again = duplicate(StructuralValidator::validate(&inventory).expect_err("4 種すべて重複"));
    assert_eq!(first, again, "同じ目録で 2 回の検証結果が違う");
    // 同じ内容を組み立て直した目録（別の内部領域）でも同じエラーが返る。
    let rebuilt = duplicate(StructuralValidator::validate(&build()).expect_err("4 種すべて重複"));
    assert_eq!(first, rebuilt, "同じ内容の目録で検証結果が違う");
}

#[test]
fn within_a_kind_the_smallest_identifier_text_is_reported() {
    // 単調カウンタで発行するため `small < large` のテキスト順が保証される。
    let mut factory = IdFactory::new();
    let small = factory.new_row_id().to_string();
    let large = factory.new_row_id().to_string();
    let mut inventory = PartInventory::new();
    // 宣言順はテキスト順と逆にしてある。
    inventory.declare(IdDeclaration::new(IdKind::Row, &large, "sheets/a.jsonl line 1"));
    inventory.declare(IdDeclaration::new(IdKind::Row, &large, "sheets/a.jsonl line 2"));
    inventory.declare(IdDeclaration::new(IdKind::Row, &small, "sheets/b.jsonl line 1"));
    inventory.declare(IdDeclaration::new(IdKind::Row, &small, "sheets/b.jsonl line 2"));

    let (kind, id, _) = duplicate(
        StructuralValidator::validate(&inventory).expect_err("2 つの行識別子が重複している"),
    );
    assert_eq!(IdKind::Row, kind);
    assert_eq!(small, id, "種別内で識別子テキストの昇順になっていない");
}

#[test]
fn validation_does_not_change_the_inventory() {
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    // 重複とスキーマ欠落の両方を持つ目録でも、検証は目録を変更しない。
    let mut inventory = inventory_of(
        IdKind::Sheet,
        &sheet,
        &["document.json sheets[0]", "document.json sheets[1]"],
    );
    inventory.require_schema(&sheet);
    let before = inventory.clone();

    duplicate(StructuralValidator::validate(&inventory).expect_err("重複したシート識別子"));

    assert_eq!(before, inventory, "検証が目録を変更した");
}

// ---------------------------------------------------------------------------
// スキーマの存在（要件 4.4）
// ---------------------------------------------------------------------------

#[test]
fn a_sheet_without_a_schema_part_is_reported() {
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let row = factory.new_row_id().to_string();
    let mut inventory = PartInventory::new();
    inventory.declare(IdDeclaration::new(IdKind::Sheet, &sheet, "document.json sheets[0]"));
    inventory.declare(IdDeclaration::new(
        IdKind::Row,
        &row,
        format!("sheets/{sheet}.jsonl line 1"),
    ));
    inventory.require_schema(&sheet);

    assert_eq!(
        sheet,
        missing_schema(
            StructuralValidator::validate(&inventory).expect_err("スキーマ欠落が報告されていない")
        ),
        "欠落したシートの識別子が報告されていない",
    );
}

#[test]
fn a_sheet_without_any_row_declaration_still_requires_a_schema() {
    // 要件 1.2 の帰結（design 不変条件「各シートはちょうど 1 つのルートスキーマを持つ」）:
    // 行データを持たないシートも文書のシートである以上ルートスキーマを要求する。
    // 1 枚目は行データとスキーマを持ち、2 枚目は行データもスキーマも持たない。報告される
    // のは 2 枚目であり、走査が最初のシートで止まらないことも同時に示す。
    let mut factory = IdFactory::new();
    let provided = factory.new_sheet_id().to_string();
    let empty = factory.new_sheet_id().to_string();
    let row = factory.new_row_id().to_string();
    let mut inventory = PartInventory::new();
    inventory.declare(IdDeclaration::new(IdKind::Sheet, &provided, "document.json sheets[0]"));
    inventory.declare(IdDeclaration::new(IdKind::Sheet, &empty, "document.json sheets[1]"));
    inventory.declare(IdDeclaration::new(
        IdKind::Row,
        &row,
        format!("sheets/{provided}.jsonl line 1"),
    ));
    inventory.require_schema(&provided);
    inventory.require_schema(&empty);
    inventory.provide_schema(&provided);

    assert_eq!(
        empty,
        missing_schema(
            StructuralValidator::validate(&inventory)
                .expect_err("行データを持たないシートのスキーマ欠落が報告されていない")
        ),
    );
}

#[test]
fn the_first_missing_sheet_in_document_order_is_reported() {
    // シート順序は document.json の配列順（それ自体がデータ）であり、識別子テキストの
    // 昇順ではない。テキスト順なら 2 番目に列挙した smaller が報告される標本である。
    let mut factory = IdFactory::new();
    let smaller = factory.new_sheet_id().to_string();
    let larger = factory.new_sheet_id().to_string();
    let provided = factory.new_sheet_id().to_string();
    let mut inventory = PartInventory::new();
    inventory.require_schema(&larger);
    inventory.require_schema(&smaller);
    inventory.require_schema(&provided);
    inventory.provide_schema(&provided);

    assert_eq!(
        larger,
        missing_schema(
            StructuralValidator::validate(&inventory)
                .expect_err("2 枚のシートでスキーマ欠落している")
        ),
        "文書順で最初に欠落したシートが報告されていない",
    );
}

// ---------------------------------------------------------------------------
// クレート外からの到達性
// ---------------------------------------------------------------------------

#[test]
fn the_validator_is_reachable_through_the_parts_module() {
    // 再エクスポート（`parts::`）とサブモジュール（`parts::validate::`）が同じ型を指す
    // ことを、双方の経路で同じ目録を検証して示す。クレート根には出していない。
    fn via_submodule(inventory: &validate::PartInventory) -> Result<(), DocumentError> {
        validate::StructuralValidator::validate(inventory)
    }
    fn via_reexport(inventory: &PartInventory) -> Result<(), DocumentError> {
        StructuralValidator::validate(inventory)
    }

    let id = IdFactory::new().new_sheet_id().to_string();
    let inventory = inventory_of(
        IdKind::Sheet,
        &id,
        &["document.json sheets[0]", "document.json sheets[1]"],
    );

    for reported in [
        duplicate(via_submodule(&inventory).expect_err("サブモジュール経由で検証できない")),
        duplicate(via_reexport(&inventory).expect_err("再エクスポート経由で検証できない")),
    ] {
        assert_eq!((IdKind::Sheet, id.clone()), (reported.0, reported.1));
    }
}

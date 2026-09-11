//! クレート外から見た構造検証（タスク 4.6 / 4.7。要件 1.4, 1.7, 4.2, 4.3, 4.4, 7.4。
//! design「Components and Interfaces」の StructuralValidator）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。
//! `document_format::parts::` 直下の再エクスポートと `document_format::parts::validate::`
//! 経由の双方がクレート外から見えることをコンパイル時に示す（`StructuralValidator` /
//! `PartInventory` / `IdDeclaration` / `TypeRefDeclaration` / `AttachmentRefDeclaration` /
//! `SheetRefDeclaration`）、および参照元つき抽出が `document_format::model` 経由で届くこと
//! （`model::SchemaPart::type_refs` が `model::TypeRef` を返す）を示す。検証の規則・入力の形・
//! 報告順の定義は `src/parts/validate.rs` のモジュール docs にあり、ここはその**観測可能な
//! 振る舞い**を固定する。
//!
//! 標本は 4 種別（シート / 行 / 型定義 / 添付）と 3 種の参照（型定義 / 添付 / シート）に
//! 分散させ、複数の違反を同時に持つ目録も使う。1 種別・1 違反の標本だけでは「種別ごとの
//! 検査」や「すべての出現箇所の報告」の回帰を検出できないためである。

use document_format::model::{SchemaPart, TypeRef};
use document_format::parts::validate;
use document_format::parts::{
    AttachmentRefDeclaration, IdDeclaration, PartInventory, SheetRefDeclaration,
    StructuralValidator, TypeRefDeclaration,
};
use document_format::{AttachmentId, CellValue, DocumentError, IdFactory, IdKind, NestedValue};

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
        DocumentError::DuplicateId {
            kind,
            id,
            occurrences,
        } => (kind, id, occurrences),
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
        vec![
            "document.json sheets[0]".to_owned(),
            "document.json sheets[2]".to_owned()
        ],
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
        &[
            format!("sheets/{first}.jsonl line 1"),
            format!("sheets/{second}.jsonl line 1"),
        ],
    );

    let (kind, id, occurrences) = duplicate(StructuralValidator::validate(&inventory).expect_err(
        "別シート間の行識別子の重複が受け入れられた（シート単位に閉じた検査になっている）",
    ));
    assert_eq!(IdKind::Row, kind);
    assert_eq!(row, id);
    assert_eq!(
        2,
        occurrences.len(),
        "2 シート分の出現箇所が報告されていない"
    );
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

    let (kind, id, occurrences) = duplicate(
        StructuralValidator::validate(&inventory)
            .expect_err("重複した型定義識別子が受け入れられた"),
    );
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
    let inventory = inventory_of(
        IdKind::Attachment,
        &attachment,
        &[entry.as_str(), entry.as_str()],
    );

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
    inventory.declare(IdDeclaration::new(
        IdKind::Sheet,
        &text,
        "document.json sheets[0]",
    ));
    inventory.declare(IdDeclaration::new(
        IdKind::Row,
        &text,
        "sheets/row.jsonl line 1",
    ));
    inventory.declare(IdDeclaration::new(
        IdKind::TypeDef,
        &text,
        "schemas/row.json types[0]",
    ));
    inventory.declare(IdDeclaration::new(
        IdKind::Attachment,
        &text,
        "attachments/row.bin",
    ));

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
    inventory.declare(IdDeclaration::new(
        IdKind::Sheet,
        &sheet,
        "document.json sheets[0]",
    ));
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
        inventory.declare(IdDeclaration::new(
            IdKind::Row,
            &row,
            "sheets/s.jsonl line 1",
        ));
        inventory.declare(IdDeclaration::new(
            IdKind::Row,
            &row,
            "sheets/s.jsonl line 2",
        ));
        inventory.declare(IdDeclaration::new(
            IdKind::Sheet,
            &sheet,
            "document.json sheets[0]",
        ));
        inventory.declare(IdDeclaration::new(
            IdKind::Sheet,
            &sheet,
            "document.json sheets[1]",
        ));
        inventory
    };

    let inventory = build();
    let expected = (
        IdKind::Sheet,
        sheet.clone(),
        vec![
            "document.json sheets[0]".to_owned(),
            "document.json sheets[1]".to_owned(),
        ],
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
    inventory.declare(IdDeclaration::new(
        IdKind::Row,
        &large,
        "sheets/a.jsonl line 1",
    ));
    inventory.declare(IdDeclaration::new(
        IdKind::Row,
        &large,
        "sheets/a.jsonl line 2",
    ));
    inventory.declare(IdDeclaration::new(
        IdKind::Row,
        &small,
        "sheets/b.jsonl line 1",
    ));
    inventory.declare(IdDeclaration::new(
        IdKind::Row,
        &small,
        "sheets/b.jsonl line 2",
    ));

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
    inventory.declare(IdDeclaration::new(
        IdKind::Sheet,
        &sheet,
        "document.json sheets[0]",
    ));
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
    inventory.declare(IdDeclaration::new(
        IdKind::Sheet,
        &provided,
        "document.json sheets[0]",
    ));
    inventory.declare(IdDeclaration::new(
        IdKind::Sheet,
        &empty,
        "document.json sheets[1]",
    ));
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

// ---------------------------------------------------------------------------
// 参照の実在性（要件 1.7, 4.2, 7.4）
// ---------------------------------------------------------------------------

/// `DanglingTypeRef` から（参照元, 参照先）を取り出す。
fn dangling_type_ref(error: DocumentError) -> (String, String) {
    match error {
        DocumentError::DanglingTypeRef { from, to } => (from, to),
        other => panic!("DanglingTypeRef を期待したが {other:?} だった"),
    }
}

/// `DanglingAttachmentRef` から（参照元, 添付識別子）を取り出す。
fn dangling_attachment_ref(error: DocumentError) -> (String, String) {
    match error {
        DocumentError::DanglingAttachmentRef { from, id } => (from, id),
        other => panic!("DanglingAttachmentRef を期待したが {other:?} だった"),
    }
}

/// `InvalidContainer` から `entry` を取り出す。
fn invalid_container(error: DocumentError) -> String {
    match error {
        DocumentError::InvalidContainer { entry } => entry,
        other => panic!("InvalidContainer を期待したが {other:?} だった"),
    }
}

/// `document.json` の `index` 番目にシートを 1 枚宣言し、ルートスキーマと型定義を与える。
///
/// 型定義の宣言には**所属シート**を与える（同一シート規則の判定材料。要件 1.7）。
fn add_sheet(inventory: &mut PartInventory, sheet: &str, index: usize, type_defs: &[&str]) {
    inventory.declare(IdDeclaration::new(
        IdKind::Sheet,
        sheet,
        format!("document.json sheets[{index}]"),
    ));
    inventory.require_schema(sheet);
    inventory.provide_schema(sheet);
    for (position, type_def) in type_defs.iter().enumerate() {
        inventory.declare(
            IdDeclaration::new(
                IdKind::TypeDef,
                *type_def,
                format!("schemas/{sheet}.json types[{position}]"),
            )
            .with_sheet(sheet),
        );
    }
}

/// `Object → Array → Object` と 3 段深い位置に添付参照を並べたセル値を作る。
///
/// 参照のほかに `text` を `Text` として同じ深さに置く（[`CellValue::Text`] の内容は
/// 添付参照ではない。要件 7.3）。
fn deeply_nested_attachments(ids: &[AttachmentId], text: &str) -> CellValue {
    let mut entries: Vec<(String, CellValue)> = ids
        .iter()
        .enumerate()
        .map(|(position, id)| (format!("ref{position}"), CellValue::Attachment(*id)))
        .collect();
    entries.push(("note".to_owned(), CellValue::Text(text.to_owned())));
    CellValue::Nested(NestedValue::Object(vec![(
        "files".to_owned(),
        CellValue::Nested(NestedValue::Array(vec![CellValue::Nested(
            NestedValue::Object(entries),
        )])),
    )]))
}

#[test]
fn a_dangling_type_ref_from_the_root_schema_is_reported() {
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let defined = factory.new_type_def_id().to_string();
    let missing = factory.new_type_def_id().to_string();
    let mut inventory = PartInventory::new();
    add_sheet(&mut inventory, &sheet, 0, &[defined.as_str()]);
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json root"),
        &missing,
        &sheet,
    ));

    let (from, to) = dangling_type_ref(
        StructuralValidator::validate(&inventory).expect_err("実在しない型定義参照が通った"),
    );
    assert_eq!(
        format!("schemas/{sheet}.json root"),
        from,
        "参照元が報告されていない"
    );
    assert_eq!(missing, to, "参照先が報告されていない");
}

#[test]
fn a_dangling_type_ref_from_a_type_definition_is_reported() {
    // 参照元は 2 種ある（ルートスキーマ / 型定義の中）。2 つ目の型定義の中からの参照で、
    // 参照元がその型定義を指すことを固定する。
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let first = factory.new_type_def_id().to_string();
    let second = factory.new_type_def_id().to_string();
    let missing = factory.new_type_def_id().to_string();
    let mut inventory = PartInventory::new();
    add_sheet(
        &mut inventory,
        &sheet,
        0,
        &[first.as_str(), second.as_str()],
    );
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json type {second}"),
        &missing,
        &sheet,
    ));

    let (from, to) = dangling_type_ref(
        StructuralValidator::validate(&inventory).expect_err("型定義内の宙吊り参照が通った"),
    );
    assert_eq!(
        format!("schemas/{sheet}.json type {second}"),
        from,
        "参照元が報告されていない"
    );
    assert_eq!(missing, to, "参照先が報告されていない");
}

#[test]
fn a_reference_to_a_type_defined_in_another_sheet_is_dangling() {
    let mut factory = IdFactory::new();
    let first = factory.new_sheet_id().to_string();
    let second = factory.new_sheet_id().to_string();
    let local = factory.new_type_def_id().to_string();
    let elsewhere = factory.new_type_def_id().to_string();
    let mut inventory = PartInventory::new();
    add_sheet(&mut inventory, &first, 0, &[local.as_str()]);
    add_sheet(&mut inventory, &second, 1, &[elsewhere.as_str()]);

    // 同一シートに実在する参照は通る（規則が「文書のどこか」ではなく「同じシート」を
    // 見ていることを、通る側からも押さえる）。
    let mut same_sheet = inventory.clone();
    same_sheet.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{first}.json root"),
        &local,
        &first,
    ));
    StructuralValidator::validate(&same_sheet)
        .expect("同一シートに実在する参照が宙吊りと報告された");

    // 別シートにしか無い型定義を指す参照は宙吊り（要件 1.7 の同一シート規則）。
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{first}.json root"),
        &elsewhere,
        &first,
    ));
    let (from, to) = dangling_type_ref(
        StructuralValidator::validate(&inventory)
            .expect_err("別シートの型定義を指す参照が通った（同一シート規則が無い）"),
    );
    assert_eq!(format!("schemas/{first}.json root"), from);
    assert_eq!(
        elsewhere, to,
        "別シートにしか無い型定義が実在として扱われた"
    );
}

#[test]
fn a_dangling_attachment_ref_nested_in_a_row_is_reported() {
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let present = AttachmentId::from_bytes(b"present payload");
    let missing = AttachmentId::from_bytes(b"missing payload");
    let mut inventory = PartInventory::new();
    add_sheet(&mut inventory, &sheet, 0, &[]);
    inventory.declare(IdDeclaration::new(
        IdKind::Attachment,
        present.to_string(),
        format!("attachments/{present}.bin"),
    ));
    // 3 段深い位置の参照。同じセルに実在する添付と、その識別子の**テキスト**も置く
    // （テキストは参照ではない）。
    inventory.declare_attachment_refs(
        format!("sheets/{sheet}.jsonl line 7"),
        &deeply_nested_attachments(&[present, missing], &present.to_string()),
    );

    let (from, id) = dangling_attachment_ref(
        StructuralValidator::validate(&inventory)
            .expect_err("3 段深い位置の宙吊り添付参照が通った（再帰が浅い）"),
    );
    assert_eq!(
        format!("sheets/{sheet}.jsonl line 7"),
        from,
        "参照元が報告されていない"
    );
    assert_eq!(
        missing.to_string(),
        id,
        "実在しない添付識別子が報告されていない"
    );
}

#[test]
fn attachment_refs_across_rows_and_sheets_are_all_checked() {
    // 1 枚目の行は実在する添付を参照し、2 枚目の複数行が実在しない添付を参照する。
    // 走査が最初のシート・最初の行で止まらないこと（`take(1)` の検出）と、参照元が
    // 報告されることを示す。
    let mut factory = IdFactory::new();
    let first = factory.new_sheet_id().to_string();
    let second = factory.new_sheet_id().to_string();
    let present = AttachmentId::from_bytes(b"present payload");
    let third_line = AttachmentId::from_bytes(b"missing at line 3");
    let ninth_line = AttachmentId::from_bytes(b"missing at line 9");
    let build = |line3: bool, line9: bool| {
        let mut inventory = PartInventory::new();
        add_sheet(&mut inventory, &first, 0, &[]);
        add_sheet(&mut inventory, &second, 1, &[]);
        inventory.declare(IdDeclaration::new(
            IdKind::Attachment,
            present.to_string(),
            format!("attachments/{present}.bin"),
        ));
        inventory.declare_attachment_refs(
            format!("sheets/{first}.jsonl line 1"),
            &CellValue::Attachment(present),
        );
        if line3 {
            inventory.declare_attachment_refs(
                format!("sheets/{second}.jsonl line 3"),
                &CellValue::Attachment(third_line),
            );
        }
        if line9 {
            inventory.declare_attachment_refs(
                format!("sheets/{second}.jsonl line 9"),
                &CellValue::Attachment(ninth_line),
            );
        }
        inventory
    };

    let (from, id) = dangling_attachment_ref(
        StructuralValidator::validate(&build(true, true)).expect_err("2 枚目の宙吊り参照が通った"),
    );
    assert_eq!(
        format!("sheets/{second}.jsonl line 3"),
        from,
        "参照元が違う"
    );
    assert_eq!(third_line.to_string(), id, "実在しない添付識別子が違う");

    // 3 行目の宙吊りを取り除くと、次の行の宙吊りが報告される（走査が続いている）。
    let (from, id) = dangling_attachment_ref(
        StructuralValidator::validate(&build(false, true))
            .expect_err("3 行目より後の宙吊り参照が報告されていない"),
    );
    assert_eq!(
        format!("sheets/{second}.jsonl line 9"),
        from,
        "参照元が違う"
    );
    assert_eq!(ninth_line.to_string(), id, "実在しない添付識別子が違う");

    // 1 枚目の実在する参照だけなら通る（同じ標本が別の理由で緑になっていないこと）。
    StructuralValidator::validate(&build(false, false)).expect("参照がすべて実在する");
}

#[test]
fn an_unreferenced_attachment_is_not_a_reference_failure() {
    // 要件 7.6（未参照添付の一覧）は本検証の担当外: 実在するが未参照の添付を破れとして
    // 報告しない（削除もしない）。
    let mut inventory = PartInventory::new();
    let referenced = AttachmentId::from_bytes(b"referenced payload");
    let unreferenced = AttachmentId::from_bytes(b"unreferenced payload");
    for id in [referenced, unreferenced] {
        inventory.declare(IdDeclaration::new(
            IdKind::Attachment,
            id.to_string(),
            format!("attachments/{id}.bin"),
        ));
    }
    inventory.declare_attachment_refs("sheets/s.jsonl line 1", &CellValue::Attachment(referenced));

    StructuralValidator::validate(&inventory).expect("未参照の添付が参照破れとして報告された");
}

#[test]
fn a_part_referencing_an_unknown_sheet_is_reported_as_a_container_error() {
    // 要件 4.2 の「シート」参照: document.json に無いシートを指すパートは参照破れである。
    // design に専用変種が無いため、既存規約どおり InvalidContainer の entry にエントリ名と
    // 理由を載せる（新しい変種を足さない）。
    let mut factory = IdFactory::new();
    let known = factory.new_sheet_id().to_string();
    let unknown = factory.new_sheet_id().to_string();

    for entry in [
        format!("sheets/{unknown}.jsonl"),
        format!("schemas/{unknown}.json"),
    ] {
        let mut inventory = PartInventory::new();
        add_sheet(&mut inventory, &known, 0, &[]);
        inventory.declare_sheet_ref(SheetRefDeclaration::new(entry.clone(), &unknown));

        let reported = invalid_container(
            StructuralValidator::validate(&inventory).expect_err("未知シートを指すパートが通った"),
        );
        assert!(
            reported.contains(&entry),
            "エントリ名が報告されていない: {reported}"
        );
        assert!(
            reported.contains(&unknown),
            "理由にシート識別子が含まれていない: {reported}"
        );
    }

    // 既知シートを指すパートは通る。
    let mut known_ref = PartInventory::new();
    add_sheet(&mut known_ref, &known, 0, &[]);
    known_ref.declare_sheet_ref(SheetRefDeclaration::new(
        format!("schemas/{known}.json"),
        &known,
    ));
    StructuralValidator::validate(&known_ref)
        .expect("既知シートを指すパートが参照破れとして報告された");
}

#[test]
fn reference_validation_reports_the_first_break_in_the_documented_order() {
    // 報告順: 型定義参照 → 添付参照 → シート参照（各種別内は目録に加えた順）。
    // 目録へは逆順（シート参照 → 添付参照 → 型定義参照）に加え、加えた順ではなく種別順で
    // 報告されることを示す。
    let mut factory = IdFactory::new();
    let known = factory.new_sheet_id().to_string();
    let unknown = factory.new_sheet_id().to_string();
    let missing_type = factory.new_type_def_id().to_string();
    let missing_attachment = AttachmentId::from_bytes(b"missing attachment");
    let build = |type_ref: bool, attachment_ref: bool, sheet_ref: bool| {
        let mut inventory = PartInventory::new();
        add_sheet(&mut inventory, &known, 0, &[]);
        if sheet_ref {
            inventory.declare_sheet_ref(SheetRefDeclaration::new(
                format!("sheets/{unknown}.jsonl"),
                &unknown,
            ));
        }
        if attachment_ref {
            inventory.declare_attachment_refs(
                "sheets/s.jsonl line 1",
                &CellValue::Attachment(missing_attachment),
            );
        }
        if type_ref {
            inventory.declare_type_ref(TypeRefDeclaration::new(
                "schemas/s.json root",
                &missing_type,
                &known,
            ));
        }
        inventory
    };

    // 3 種すべてが破れている目録でも、種別順で先頭の型定義参照が報告される。
    let inventory = build(true, true, true);
    let expected = ("schemas/s.json root".to_owned(), missing_type.clone());
    let first = dangling_type_ref(
        StructuralValidator::validate(&inventory).expect_err("3 種すべてが破れている"),
    );
    assert_eq!(expected, first, "型定義参照より先の種別が報告された");
    // 同じ目録を 3 回検証しても同じエラーが返る（ハッシュ反復順に依存しない）。
    for _ in 1..3 {
        assert_eq!(
            first,
            dangling_type_ref(
                StructuralValidator::validate(&inventory).expect_err("3 種すべてが破れている"),
            ),
            "同じ目録で検証結果が変わった",
        );
    }

    // 型定義参照が実在すると、次は添付参照が報告される。
    assert_eq!(
        (
            "sheets/s.jsonl line 1".to_owned(),
            missing_attachment.to_string()
        ),
        dangling_attachment_ref(
            StructuralValidator::validate(&build(false, true, true))
                .expect_err("添付参照とシート参照が破れている")
        ),
    );

    // 添付参照も実在すると、最後にシート参照が InvalidContainer として報告される。
    let reported = invalid_container(
        StructuralValidator::validate(&build(false, false, true))
            .expect_err("シート参照だけが破れている"),
    );
    assert!(
        reported.contains(&format!("sheets/{unknown}.jsonl")),
        "{reported}"
    );

    // 3 種とも実在すれば通る（標本の破れはこの 3 つだけである）。
    StructuralValidator::validate(&build(false, false, false)).expect("参照がすべて実在する");
}

#[test]
fn within_a_reference_kind_the_order_added_to_the_inventory_is_reported() {
    // 2 つの宙吊り型定義参照を、識別子テキストの昇順と**逆順**に加える。ソートする実装なら
    // 小さい方が、目録に加えた順なら先に加えた方が報告される（`IdFactory` は単調カウンタなので
    // `smaller < larger` のテキスト順が保証される）。
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let smaller = factory.new_type_def_id().to_string();
    let larger = factory.new_type_def_id().to_string();
    let mut inventory = PartInventory::new();
    add_sheet(&mut inventory, &sheet, 0, &[]);
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json root"),
        &larger,
        &sheet,
    ));
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json type {smaller}"),
        &smaller,
        &sheet,
    ));

    let (from, to) = dangling_type_ref(
        StructuralValidator::validate(&inventory).expect_err("2 つの宙吊り型定義参照"),
    );
    assert_eq!(
        format!("schemas/{sheet}.json root"),
        from,
        "参照元が目録の順でない"
    );
    assert_eq!(larger, to, "種別内で目録順ではなくソート順に報告している");
}

#[test]
fn the_identifier_and_schema_checks_still_take_precedence_over_references() {
    // 4.6 の検査（一意性・スキーマ存在）は参照より先に走る。
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let missing_type = factory.new_type_def_id().to_string();
    let mut inventory = PartInventory::new();
    inventory.declare(IdDeclaration::new(
        IdKind::Sheet,
        &sheet,
        "document.json sheets[0]",
    ));
    inventory.declare(IdDeclaration::new(
        IdKind::Sheet,
        &sheet,
        "document.json sheets[1]",
    ));
    inventory.require_schema(&sheet);
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json root"),
        &missing_type,
        &sheet,
    ));

    let (kind, id, _) = duplicate(StructuralValidator::validate(&inventory).expect_err("重複"));
    assert_eq!(
        (IdKind::Sheet, sheet.clone()),
        (kind, id),
        "一意性の検査が参照より後になった"
    );

    // 重複を消すとスキーマ欠落が先に報告される（参照より先）。
    let mut no_schema = PartInventory::new();
    no_schema.declare(IdDeclaration::new(
        IdKind::Sheet,
        &sheet,
        "document.json sheets[0]",
    ));
    no_schema.require_schema(&sheet);
    no_schema.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json root"),
        &missing_type,
        &sheet,
    ));
    assert_eq!(
        sheet,
        missing_schema(
            StructuralValidator::validate(&no_schema).expect_err("スキーマ欠落が報告されていない")
        ),
        "スキーマ存在の検査が参照より後になった",
    );
}

#[test]
fn reference_validation_does_not_change_the_inventory() {
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let missing_type = factory.new_type_def_id().to_string();
    let missing_attachment = AttachmentId::from_bytes(b"missing attachment");
    let mut inventory = PartInventory::new();
    add_sheet(&mut inventory, &sheet, 0, &[]);
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json root"),
        &missing_type,
        &sheet,
    ));
    inventory.declare_attachment_refs(
        format!("sheets/{sheet}.jsonl line 1"),
        &deeply_nested_attachments(&[missing_attachment], "note"),
    );
    inventory.declare_sheet_ref(SheetRefDeclaration::new(
        "sheets/unlisted.jsonl",
        "01UNLISTED",
    ));
    let before = inventory.clone();

    dangling_type_ref(StructuralValidator::validate(&inventory).expect_err("宙吊りの参照"));

    assert_eq!(before, inventory, "検証が参照の目録を変更した");
}

#[test]
fn blank_reference_texts_are_reported_without_panicking() {
    // 参照のテキストは本検証にとって不透明である（解釈しない）。空テキストでも panic せず
    // 同じ規則で報告する。
    let mut type_refs = PartInventory::new();
    type_refs.declare_type_ref(TypeRefDeclaration::new("", "", ""));
    assert_eq!(
        (String::new(), String::new()),
        dangling_type_ref(StructuralValidator::validate(&type_refs).expect_err("空の型定義参照")),
    );

    let missing = AttachmentId::from_bytes(b"missing payload");
    let mut attachment_refs = PartInventory::new();
    attachment_refs.declare_attachment_ref(AttachmentRefDeclaration::new("", missing.to_string()));
    assert_eq!(
        (String::new(), missing.to_string()),
        dangling_attachment_ref(
            StructuralValidator::validate(&attachment_refs).expect_err("空の参照元の添付参照")
        ),
    );

    // セル値を再帰的にたどる収集経路でも、空テキストは同じ扱いである。
    let mut collected = PartInventory::new();
    collected.declare_attachment_refs(
        "",
        &CellValue::Nested(NestedValue::Array(vec![CellValue::Attachment(missing)])),
    );
    assert_eq!(
        (String::new(), missing.to_string()),
        dangling_attachment_ref(
            StructuralValidator::validate(&collected).expect_err("空の参照元の添付参照")
        ),
    );

    let mut sheet_refs = PartInventory::new();
    sheet_refs.declare_sheet_ref(SheetRefDeclaration::new("", ""));
    let reported =
        invalid_container(StructuralValidator::validate(&sheet_refs).expect_err("空のシート参照"));
    assert!(
        reported.contains("document.json"),
        "理由が報告されていない: {reported}"
    );
}

#[test]
fn schema_type_refs_carry_their_source_and_feed_the_validator() {
    // model 側のアクセサ（`SchemaPart::type_refs`。タスク 4.7 が追加）が参照元つきで参照を
    // 返すこと、および既存の `type_ref_targets` の意味（順序・重複保持）が変わっていないこと。
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let defined = factory.new_type_def_id().to_string();
    let missing = factory.new_type_def_id().to_string();
    let text = format!(
        concat!(
            r#"{{"root":{{"field":{{"$ref":"{defined}"}}}},"#,
            r#""types":[{{"id":"{defined}","definition":{{"next":{{"$ref":"{missing}"}}}}}}]}}"#,
        ),
        defined = defined,
        missing = missing,
    );
    let part = SchemaPart::parse(&text).expect("標本は妥当なエンベロープ");

    let references: &[TypeRef] = part.type_refs();
    let sourced: Vec<(String, String)> = references
        .iter()
        .map(|reference| (reference.from().to_owned(), reference.to().to_owned()))
        .collect();
    assert_eq!(
        vec![
            ("root".to_owned(), defined.clone()),
            (format!("type {defined}"), missing.clone()),
        ],
        sourced,
        "参照元つきの抽出が参照元・参照先を正しく返していない",
    );
    assert_eq!(
        vec![defined.clone(), missing.clone()],
        part.type_ref_targets().to_vec(),
        "既存の type_ref_targets の意味が変わった",
    );

    // 4.8 が行う結線（参照元にエントリ名を前置して目録へ流し込む）で、宙吊りの参照が
    // 参照元つきで報告される。
    let mut inventory = PartInventory::new();
    add_sheet(&mut inventory, &sheet, 0, &[defined.as_str()]);
    let entry = format!("schemas/{sheet}.json");
    for reference in part.type_refs() {
        inventory.declare_type_ref(TypeRefDeclaration::new(
            format!("{entry} {}", reference.from()),
            reference.to(),
            &sheet,
        ));
    }

    let (from, to) =
        dangling_type_ref(StructuralValidator::validate(&inventory).expect_err("宙吊りの参照"));
    assert_eq!(
        format!("{entry} type {defined}"),
        from,
        "参照元が結線されていない"
    );
    assert_eq!(missing, to, "参照先が結線されていない");
}

#[test]
fn schema_type_refs_keep_payload_order_labels_and_duplicates() {
    // 参照を持たないペイロードが挟まっても、参照元ラベルと順序・重複が保たれること
    // （参照元つき抽出の範囲管理の回帰）。ルートは参照 0 件、1 つ目の型定義も 0 件、
    // 2 つ目が 1 件、3 つ目が 2 件（うち 1 件は重複）である。
    let mut factory = IdFactory::new();
    let first = factory.new_type_def_id().to_string();
    let second = factory.new_type_def_id().to_string();
    let third = factory.new_type_def_id().to_string();
    let text = format!(
        concat!(
            r#"{{"root":{{"note":"参照なし"}},"#,
            r#""types":[{{"id":"{first}","definition":{{"a":1}}}},"#,
            r#"{{"id":"{second}","definition":{{"$ref":"T2"}}}},"#,
            r#"{{"id":"{third}","definition":[{{"$ref":"T3"}},{{"$ref":"T2"}}]}}]}}"#,
        ),
        first = first,
        second = second,
        third = third,
    );
    let part = SchemaPart::parse(&text).expect("標本は妥当なエンベロープ");

    let sourced: Vec<(String, String)> = part
        .type_refs()
        .iter()
        .map(|reference| (reference.from().to_owned(), reference.to().to_owned()))
        .collect();
    assert_eq!(
        vec![
            (format!("type {second}"), "T2".to_owned()),
            (format!("type {third}"), "T3".to_owned()),
            (format!("type {third}"), "T2".to_owned()),
        ],
        sourced,
        "参照元ラベル・順序・重複のいずれかが変わった",
    );
    let targets: Vec<&str> = part.type_ref_targets().iter().map(String::as_str).collect();
    assert_eq!(
        vec!["T2", "T3", "T2"],
        targets,
        "ターゲット列が参照元つき抽出とずれた",
    );
}

// ---------------------------------------------------------------------------
// 識別子の解決規約（大小文字。要件 1.7, 4.2）
// ---------------------------------------------------------------------------

#[test]
fn a_type_reference_in_lower_case_resolves_to_the_declaration() {
    // `$ref` は生テキストである。`ids` の識別子解決は大小文字を問わない（正準形は大文字）
    // ため、小文字表記で参照した妥当な文書を宙吊りと報告してはならない。
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let defined = factory.new_type_def_id().to_string();
    let mut inventory = PartInventory::new();
    add_sheet(&mut inventory, &sheet, 0, &[defined.as_str()]);
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json root"),
        defined.to_lowercase(),
        &sheet,
    ));

    StructuralValidator::validate(&inventory).expect("小文字表記の参照先が宙吊りと報告された");
}

#[test]
fn a_type_reference_and_its_declaration_both_in_lower_case_resolve() {
    // 宣言側も参照側も小文字表記の文書（`schema_codec` が受理し、再符号化で正準形へ揃う入力）
    // でも、同一の ULID を指していれば実在する。
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let defined = factory.new_type_def_id().to_string().to_lowercase();
    let mut inventory = PartInventory::new();
    add_sheet(&mut inventory, &sheet, 0, &[defined.as_str()]);
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json type {defined}"),
        &defined,
        &sheet,
    ));

    StructuralValidator::validate(&inventory).expect("小文字表記どうしの参照が宙吊りと報告された");
}

#[test]
fn an_unparsable_type_reference_is_dangling_with_the_raw_text() {
    // 正規化であってサニタイズではない: 識別子としてパースできないテキストは原文のまま
    // 「実在しない参照先」として報告する（エラーにはしない）。
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let defined = factory.new_type_def_id().to_string();
    let mut inventory = PartInventory::new();
    add_sheet(&mut inventory, &sheet, 0, &[defined.as_str()]);
    let raw = format!("not-{defined}");
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json root"),
        &raw,
        &sheet,
    ));

    let (from, to) = dangling_type_ref(
        StructuralValidator::validate(&inventory).expect_err("パースできない参照先が通った"),
    );
    assert_eq!(
        format!("schemas/{sheet}.json root"),
        from,
        "参照元が報告されていない"
    );
    assert_eq!(raw, to, "報告の参照先が原文でない");
}

#[test]
fn a_type_reference_to_another_identifier_in_lower_case_is_dangling_with_the_raw_text() {
    // 別の ULID（小文字表記）を指す参照は宙吊りであり、報告の `to` は原文を保つ。
    let mut factory = IdFactory::new();
    let sheet = factory.new_sheet_id().to_string();
    let defined = factory.new_type_def_id().to_string();
    let other = factory.new_type_def_id().to_string();
    let mut inventory = PartInventory::new();
    add_sheet(&mut inventory, &sheet, 0, &[defined.as_str()]);
    let raw = other.to_lowercase();
    inventory.declare_type_ref(TypeRefDeclaration::new(
        format!("schemas/{sheet}.json type {defined}"),
        &raw,
        &sheet,
    ));

    let (from, to) = dangling_type_ref(
        StructuralValidator::validate(&inventory).expect_err("別の識別子を指す参照が通った"),
    );
    assert_eq!(format!("schemas/{sheet}.json type {defined}"), from);
    assert_eq!(raw, to, "報告の参照先が原文でない");
}

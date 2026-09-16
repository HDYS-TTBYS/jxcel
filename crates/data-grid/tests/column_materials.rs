//! 列の**宣言**から導ける材料が列の構成に載る（データグリッドのタスク 10.3。data-grid 要件
//! 3.2, 3.7, 3.8, 5.4, 5.5, 10.1, 10.4）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のものである。
//!
//! 1. **選択肢**（要件 3.2）。`Enum` の列は宣言された選択肢を**宣言順に**持ち、選択肢を持たない
//!    列は持たない。
//! 2. **参照先のシート**（要件 3.8）。`Ref` の列は宣言が指すシートの識別子を持ち、参照しない
//!    列は持たない。
//! 3. **ユーザー定義型の識別子**（要件 10.1, 10.4）。`Custom` の列は**登録簿を引く鍵**を持ち、
//!    他の列は持たない。
//! 4. **値なしを許すか**（要件 3.7）。**宣言から写す** — 必須の列は偽、任意の列は真である。
//!    内側の位置は**そのフィールドの** `required` から写す（列の `required` ではない）。
//! 5. **入れ子の内側の宣言**（要件 5.5）。**展開していない列でも読める**こと、位置がセル直下
//!    からの絶対の位置であること、名と型の札を持つこと。
//! 6. **深さの上限**（要件 5.4）。内側の宣言は列として展開する深さの上限で切られ、どれだけ
//!    深い宣言でも材料は有限である。
//! 7. **材料が無いことは誤りではない**（要件 10.4）。材料を持たない列は空／`None` を運び、
//!    構成そのものは壊れない。
//! 8. **参照先の行の頁**（要件 3.8）。開始位置と件数で切り出し、総数を返し、続きがあるかを
//!    名乗る。**検索の文字で絞り込める**（空なら絞り込まない）。
//!
//! 前提は `CompiledSchema` の公開面から読み直す（期待値を手書きの写しにしない）。

use std::str::FromStr;
use std::sync::Arc;

use data_grid::{
    ColumnDeclaration, ColumnIndex, Expandability, LayoutColumn, LayoutMember, MAX_EXPANSION_DEPTH,
    NestedPathSegment, ReferencePage, ViewState, reference_page,
};
use document_format::{CellValue, Document, SheetId};
use schema_engine::compile::plan::ColumnValidator;
use schema_engine::{
    ColumnDecl, CompiledSchema, Constraints, CustomType, CustomTypeFailure, CustomTypeId,
    CustomVerdict, DeclaredKind, FieldDecl, Schema, TypeDecl, TypeKind, TypeRegistry,
    compile_declaration,
};

// ---------------------------------------------------------------------------
// 道具
// ---------------------------------------------------------------------------

/// 宣言の型を組む。
fn kind(kind: TypeKind, constraints: Constraints) -> TypeDecl {
    TypeDecl::Kind {
        kind: DeclaredKind::Known(kind),
        constraints,
    }
}

/// 任意の型（制約なし）。
fn plain(tag: TypeKind) -> TypeDecl {
    kind(tag, Constraints::default())
}

/// フィールド 1 本。
fn field(name: &str, ty: TypeDecl, required: bool) -> FieldDecl {
    FieldDecl {
        name: name.into(),
        ty,
        required,
        default: None,
        description: None,
    }
}

/// 入れ子のオブジェクトの型。
fn object(fields: Vec<FieldDecl>) -> TypeDecl {
    kind(
        TypeKind::Object,
        Constraints {
            fields,
            ..Constraints::default()
        },
    )
}

/// 列 1 本の宣言（必須かどうかだけを指定できる）。
fn column_decl(name: &str, ty: TypeDecl, required: bool) -> ColumnDecl {
    ColumnDecl {
        name: name.into(),
        ty,
        required,
        unique: false,
        default: None,
        description: None,
    }
}

/// 列 1 本だけの宣言をコンパイルする。
fn compile(schema: &Schema, registry: &TypeRegistry) -> CompiledSchema {
    compile_declaration(schema, &[], registry).expect("検査の宣言は妥当である")
}

/// 構成（折りたたみのまま）を導出する。
fn layout(schema: &CompiledSchema) -> Vec<LayoutColumn> {
    ViewState::new().layout(schema).columns().to_vec()
}

/// 検査用の拡張型（**判定は何もせず適合を返す**。材料の検査に判定は要らない）。
struct EchoType(CustomTypeId);

impl CustomType for EchoType {
    fn id(&self) -> &CustomTypeId {
        &self.0
    }

    fn validate(&self, _value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
        Ok(CustomVerdict::Accepted)
    }
}

// ---------------------------------------------------------------------------
// 要件 3.2 / 3.8 / 10.1 / 3.7: 葉の列の材料
// ---------------------------------------------------------------------------

/// 選択肢・参照先・ユーザー定義型の識別子・値なしを許すかは、**宣言から写る**。
#[test]
fn a_leaf_column_carries_the_material_of_its_declaration() {
    let reference_target = SheetId::from_str("01K4ANRRG004HMASW9NF6YY091").expect("識別子である");
    let mut registry = TypeRegistry::new();
    registry
        .register(Arc::new(EchoType(CustomTypeId::new("postal-code"))))
        .expect("検査の拡張型は登録できる");

    let schema = Schema {
        columns: vec![
            column_decl("品番", plain(TypeKind::Text), true),
            column_decl(
                "区分",
                kind(
                    TypeKind::Enum,
                    Constraints {
                        choices: vec!["赤".into(), "青".into(), "緑".into()],
                        ..Constraints::default()
                    },
                ),
                false,
            ),
            column_decl(
                "仕入先",
                kind(
                    TypeKind::Ref,
                    Constraints {
                        sheet: Some(reference_target),
                        ..Constraints::default()
                    },
                ),
                true,
            ),
            column_decl(
                "郵便番号",
                kind(
                    TypeKind::Custom,
                    Constraints {
                        custom_type: Some("postal-code".into()),
                        ..Constraints::default()
                    },
                ),
                false,
            ),
            column_decl("数量", plain(TypeKind::Int), false),
        ],
    };
    let compiled = compile(&schema, &registry);
    let columns = layout(&compiled);

    // 前提: 5 列の宣言がそのまま 5 件の構成になる（入れ子を展開していない）。
    assert_eq!(5, columns.len());
    assert_eq!(Some(TypeKind::Enum), columns[1].kind);
    assert_eq!(Some(TypeKind::Ref), columns[2].kind);
    assert_eq!(Some(TypeKind::Custom), columns[3].kind);

    // 1. 選択肢（要件 3.2）。宣言順であり、値と名は同じ 1 つの宣言から来る。
    assert_eq!(
        vec!["赤".to_owned(), "青".to_owned(), "緑".to_owned()],
        columns[1]
            .declaration
            .choices
            .iter()
            .map(|choice| choice.to_string())
            .collect::<Vec<_>>(),
    );
    // 材料を持たない列は空である（「材料が無い」と「選択肢が 0 件」を混同しない — 面は既定へ
    // 落ちる。要件 10.4）。
    for index in [0, 2, 3, 4] {
        assert!(
            columns[index].declaration.choices.is_empty(),
            "列 {index} は選択肢を持たない"
        );
    }

    // 2. 参照先のシート（要件 3.8）。**宣言が持つ識別子がそのまま載る**（人が読む名は文書が
    //    持つため、名への写しは文書を見られる側が行う）。
    assert_eq!(
        Some("01K4ANRRG004HMASW9NF6YY091"),
        columns[2].declaration.reference_sheet.as_deref(),
    );
    for index in [0, 1, 3, 4] {
        assert_eq!(None, columns[index].declaration.reference_sheet);
    }

    // 3. ユーザー定義型の識別子（要件 10.1、10.4）。登録簿を引く鍵そのものである。
    assert_eq!(
        Some("postal-code"),
        columns[3].declaration.custom_type_id.as_deref(),
    );
    for index in [0, 1, 2, 4] {
        assert_eq!(None, columns[index].declaration.custom_type_id);
    }

    // 4. 値なしを許すか（要件 3.7）。**必須の列は偽**（「値なしへ戻す」道を出さない）。
    assert!(!columns[0].declaration.nullable, "品番は必須である");
    assert!(!columns[2].declaration.nullable, "仕入先は必須である");
    assert!(columns[1].declaration.nullable, "区分は任意である");
    assert!(columns[3].declaration.nullable, "郵便番号は任意である");
    assert!(columns[4].declaration.nullable, "数量は任意である");

    // 材料を持たない列は内側の宣言も持たない（要件 10.4 の既定へ落ちる道を塞がない）。
    for column in &columns {
        assert!(
            column.declaration.members.is_empty(),
            "{} は入れ子でない",
            column.name
        );
    }
}

// ---------------------------------------------------------------------------
// 要件 5.5 / 5.4: 入れ子の内側の宣言
// ---------------------------------------------------------------------------

/// 入れ子の列は、**展開していない**状態でも内側の宣言を名と型で運ぶ（要件 5.5）。
#[test]
fn a_collapsed_nested_column_carries_its_inner_declaration() {
    let schema = Schema {
        columns: vec![
            column_decl("品番", plain(TypeKind::Text), true),
            column_decl(
                "届け先",
                object(vec![
                    field("郵便番号", plain(TypeKind::Text), true),
                    field(
                        "区分",
                        kind(
                            TypeKind::Enum,
                            Constraints {
                                choices: vec!["自宅".into(), "勤務先".into()],
                                ..Constraints::default()
                            },
                        ),
                        false,
                    ),
                    field(
                        "住所",
                        object(vec![field("市", plain(TypeKind::Text), false)]),
                        false,
                    ),
                ]),
                false,
            ),
        ],
    };
    let compiled = compile(&schema, &TypeRegistry::new());
    // 前提: 内側のフィールドの宣言はスキーマから読み直せる（期待値を手書きの写しにしない）。
    let Some(ColumnValidator::Object { fields }) = compiled.validator(ColumnIndex::new(1)) else {
        panic!("前提が崩れた: 届け先はオブジェクトである");
    };
    assert_eq!(3, fields.len());

    let columns = layout(&compiled);
    // **折りたたみのまま**である（構成は 2 件。内側は列として現れない）。
    assert_eq!(2, columns.len());
    let nested = &columns[1];
    assert!(nested.path.is_root(), "折りたたみでは内側の位置を持たない");
    assert_eq!(Some(TypeKind::Object), nested.kind);

    // 内側の宣言は位置・名・型の札を持つ（**絶対の位置**であり、セル直下からの段の並びである）。
    let declared = &nested.declaration.members;
    let paths: Vec<Vec<String>> = declared.iter().map(member_path).collect();
    assert_eq!(
        vec![
            vec!["郵便番号".to_owned()],
            vec!["区分".to_owned()],
            vec!["住所".to_owned()],
            vec!["住所".to_owned(), "市".to_owned()],
        ],
        paths,
        "内側の位置はセル直下からの絶対の位置であり、宣言順に並ぶ",
    );
    assert_eq!("届け先.郵便番号", declared[0].name.as_ref());
    assert_eq!(TypeKind::Text, declared[0].kind);
    assert_eq!(TypeKind::Enum, declared[1].kind);
    assert_eq!(TypeKind::Object, declared[2].kind);
    assert_eq!(TypeKind::Text, declared[3].kind);
    assert_eq!("届け先.住所.市", declared[3].name.as_ref());

    // 内側の位置の「値なしを許すか」は**そのフィールドの** `required` から写る（要件 3.7）。
    assert!(!declared[0].nullable, "郵便番号は必須である");
    assert!(declared[1].nullable, "区分は任意である");
    assert!(declared[2].nullable, "住所は任意である");

    // 内側の位置の選択肢も材料である（要件 3.2）— 葉の列と同じ形で載る。
    assert_eq!(
        vec!["自宅".to_owned(), "勤務先".to_owned()],
        declared[1]
            .choices
            .iter()
            .map(|choice| choice.to_string())
            .collect::<Vec<_>>(),
    );
    assert!(declared[0].choices.is_empty());
}

/// 内側の宣言は**列として展開する深さの上限**で切られる（要件 5.4、5.5）。
///
/// 深い宣言（12 段）でも材料は有限であり、位置の段数は上限を越えない。上限を外すと本検査は
/// 深い位置まで材料を並べて落ちる（材料の大きさが宣言の深さに従ってしまう）。
#[test]
fn the_inner_declaration_is_cut_at_the_expansion_depth() {
    /// `levels` 段の入れ子（各段は葉 1 本と次の段 1 本を持つ）。再帰ではなく書き下しである。
    fn spine(levels: usize) -> TypeDecl {
        assert!(levels >= 1, "段数は 1 以上である");
        let mut ty = object(vec![field("葉", plain(TypeKind::Text), false)]);
        for level in (1..levels).rev() {
            ty = object(vec![
                field(&format!("葉{level}"), plain(TypeKind::Text), false),
                field(&format!("次{level}"), ty, false),
            ]);
        }
        ty
    }

    const LEVELS: usize = 12;
    let schema = Schema {
        columns: vec![column_decl("連鎖", spine(LEVELS), false)],
    };
    let compiled = compile(&schema, &TypeRegistry::new());
    // 前提: 書き下した 12 段は使用不能ではない（再帰ではないので検証器が組める）。
    assert!(
        compiled.validator(ColumnIndex::new(0)).is_some(),
        "前提が崩れた: 連鎖は使用可能である"
    );

    let columns = layout(&compiled);
    let declared = &columns[0].declaration.members;
    let deepest = declared
        .iter()
        .map(|member| member.path.len())
        .max()
        .expect("内側の宣言がある");
    assert_eq!(
        usize::from(MAX_EXPANSION_DEPTH),
        deepest,
        "位置の段数は上限で止まる（{LEVELS} 段の宣言でも材料は上限までである）",
    );
    // 上限を越える位置が 1 つも無い（件数そのものを固定する — 「返ってきた」ことではない）。
    assert!(
        declared
            .iter()
            .all(|member| member.path.len() <= usize::from(MAX_EXPANSION_DEPTH)),
    );
    // 上限の段に達した位置は、それ以上降りない（その位置自身は材料に載る）。
    assert!(
        declared
            .iter()
            .any(|member| member.path.len() == usize::from(MAX_EXPANSION_DEPTH))
    );

    // 展開の深さの上限と内側の宣言の深さの上限が同じ規律であること（印と材料が食い違わない）。
    let capped = ViewState::new().layout(&compiled);
    assert_eq!(
        Expandability::Available,
        capped.columns()[0].expandability,
        "折りたたみでは降りられる（印は付かない）"
    );
}

/// 使用不能な列は材料も持たない（要件 11.7 の列が構成を壊さない。要件 10.4 の既定へ落ちる）。
#[test]
fn an_unusable_column_carries_no_material() {
    let schema = Schema {
        columns: vec![
            column_decl(
                "未来",
                TypeDecl::Kind {
                    kind: DeclaredKind::Unknown("hologram".into()),
                    constraints: Constraints::default(),
                },
                false,
            ),
            column_decl("数", plain(TypeKind::Int), false),
        ],
    };
    let compiled = compile(&schema, &TypeRegistry::new());
    assert!(
        compiled.validator(ColumnIndex::new(0)).is_none(),
        "前提が崩れた"
    );

    let columns = layout(&compiled);
    let unusable: &ColumnDeclaration = &columns[0].declaration;
    assert!(unusable.choices.is_empty());
    assert_eq!(None, unusable.reference_sheet);
    assert_eq!(None, unusable.custom_type_id);
    assert!(unusable.members.is_empty());
    assert!(unusable.nullable, "未知の型の列も任意である（宣言どおり）");
}

// ---------------------------------------------------------------------------
// 要件 3.8: 参照先の行の頁
// ---------------------------------------------------------------------------

/// 参照先のシートの行を、開始位置と件数で切り出す（要件 3.8）。
#[test]
fn a_page_cuts_the_referenced_rows_by_position_and_size() {
    let (document, sheet) = reference_document(10);
    let rows = document.sheets()[sheet].rows();
    assert_eq!(10, rows.len(), "前提が崩れた: 標本は 10 行である");

    let first = reference_page(&document.sheets()[sheet], "", 0, 4);
    assert_eq!(10, first.total, "総数は頁の外も数える");
    assert!(first.has_more, "後ろにまだ行がある");
    assert_eq!(4, first.rows.len());
    assert_eq!(
        vec!["S0", "S1", "S2", "S3"],
        first
            .rows
            .iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>(),
    );
    // 識別子は文書のものである（表示の名ではない。要件 8.6）。
    assert_eq!(rows[0].id().to_string(), first.rows[0].id.to_string(),);

    let last = reference_page(&document.sheets()[sheet], "", 8, 4);
    assert_eq!(10, last.total);
    assert_eq!(2, last.rows.len(), "残りだけが返る");
    assert!(!last.has_more, "末尾に達した");
    assert_eq!(
        vec!["S8", "S9"],
        last.rows
            .iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>(),
    );

    // 開始位置が総数を越えていても壊れない（空の頁であり、続きは無い）。
    let past = reference_page(&document.sheets()[sheet], "", 99, 4);
    assert!(past.rows.is_empty());
    assert_eq!(10, past.total);
    assert!(!past.has_more);
}

/// 検索の文字で行が絞られ、**総数も絞ったあとの数**になる（要件 3.8）。
#[test]
fn the_search_text_filters_the_rows_and_the_total() {
    let (document, sheet) = reference_document(10);
    let page = reference_page(&document.sheets()[sheet], "S1", 0, 10);
    // "S1" を含む表示の名は S1 だけである（S10 は標本に無い — 標本は 10 行である）。
    assert_eq!(1, page.total);
    assert_eq!(
        vec!["S1"],
        page.rows
            .iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>()
    );
    assert!(!page.has_more);

    // 大文字と小文字は畳まない（`view/mod.rs` の `FilterSpec::Contains` と同じ規則である）。
    let lower = reference_page(&document.sheets()[sheet], "s1", 0, 10);
    assert_eq!(0, lower.total);
    assert!(lower.rows.is_empty());

    // 空の検索は絞り込まない。
    let all = reference_page(&document.sheets()[sheet], "", 0, 1);
    assert_eq!(10, all.total);
}

/// 頁は**要求された件数を超えない**（呼び出し側が与えた件数のままである）。
#[test]
fn a_page_never_returns_more_rows_than_asked() {
    let (document, sheet) = reference_document(10_000);
    let page = reference_page(&document.sheets()[sheet], "", 0, 3);
    assert_eq!(3, page.rows.len());
    assert_eq!(10_000, page.total);
    assert!(page.has_more);

    // すべてを要求した場合との対比（上限は境界が強制する。本層は与えられた件数を使う）。
    let whole = reference_page(&document.sheets()[sheet], "", 0, 10_000);
    assert_eq!(10_000, whole.rows.len());

    let empty = ReferencePage::empty();
    assert!(empty.rows.is_empty());
    assert_eq!(0, empty.total);
    assert!(!empty.has_more);
}

// ---------------------------------------------------------------------------
// 標本
// ---------------------------------------------------------------------------

/// `rows` 行（`S0` … `S{rows-1}` の 1 列）の文書と、そのシートの添字を返す。
fn reference_document(rows: usize) -> (Document, usize) {
    let mut document = Document::new();
    let sheet = document.add_sheet("仕入先");
    document
        .set_sheet_columns(sheet, vec!["名".to_owned()])
        .expect("標本のシートは実在する");
    for index in 0..rows {
        let row = document.add_row(sheet).expect("標本のシートは実在する");
        document
            .set_row_values(sheet, row, vec![CellValue::Text(format!("S{index}"))])
            .expect("標本の行は実在する");
    }
    (document, 0)
}

/// 内側の位置を人が読める段の並びにする（検査の可読性のため）。
fn member_path(member: &LayoutMember) -> Vec<String> {
    member
        .path
        .segments()
        .iter()
        .map(|segment| match segment {
            NestedPathSegment::Field(name) => name.to_string(),
            NestedPathSegment::Index(index) => format!("[{index}]"),
        })
        .collect()
}

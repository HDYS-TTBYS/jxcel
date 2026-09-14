//! 入れ子の展開状態から列の構成を導出する（データグリッドのタスク 2.3。data-grid 要件 5.1,
//! 5.2, 5.3, 5.4, 5.6）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のものである。
//!
//! 1. **折りたたみはちょうど 1 列、展開は内側のフィールドを宣言順に並べる**（要件 5.1, 5.2）。
//!    折りたたんだ入れ子の列は構成の要素を**ちょうど 1 件**持ち、展開すると内側のフィールドが
//!    **宣言順**に現れ（親の列そのものは現れない）、折りたたむと元の 1 件へ戻る。入れ子でない
//!    列と、要素数の並び（配列）の列は、展開を指定しても 1 件のままである。
//! 2. **段数の上限**（要件 5.4）。上限 [`data_grid::MAX_EXPANSION_DEPTH`] 段まで展開でき、
//!    上限に達した入れ子はそれ以上降りず、**詳細表示へ委ねる印**
//!    （[`data_grid::Expandability::Capped`]）が観測できる。浅い段数を要求すればそこまでで
//!    止まり、印は付かない（上限に達していないため）。
//! 3. **発散しない**こと（タスク 2.3 が明示的に求める検査）。上限より深い入れ子を持つ宣言と、
//!    再帰する型定義の双方について、構成が**有限で上限に収まる**ことを、件数と構造の
//!    exact な突き合わせで示す（「返ってきた」ことではなく、件数そのものを固定する）。
//!    再帰する型定義は `schema-engine` が**その列だけ使用不能**にするため（`validator` が
//!    `None`）、そもそも展開の対象にならない — この前提も検査する。
//! 4. **同一の型の並びの要素数**（要件 5.6）。配列の列は宣言された要素数の能力
//!    （`minItems` / `maxItems`）を持ち、配列でない列は持たない。入れ子の内側の配列も同じで
//!    あり、**配列は列として展開されない**（要素数で示す）。
//! 5. **展開の状態は順序の再計算で失われない**（要件 5.3）。再計算の前後で `expansion` の
//!    1 件ずつの同一性（列・展開の有無・段数）が変わりなく、構成も変わらない。再計算は
//!    [`data_grid::ViewState::recompute_order`] が **`&self`** を取るため、展開状態への共有
//!    借用を持ったまま呼べる（消す経路が型の上に存在しない — 2.1 が `&Document` で
//!    「ドキュメントを書き換えない」ことを示したのと同じ形である）。
//! 6. **決定性**。同じ入力からの導出は常に同じ構成になり、`expansion` の並びの順が違っても
//!    結果は同じである。同じ列に複数の指定がある場合は**後ろの指定が前を上書きする**。
//! 7. **使用不能な列**（`CompiledSchema::validator` が `None`）は展開されず、panic しない。
//! 8. **フィールドを持たないオブジェクト**を展開しても列は消えない（1 件のまま残る）。
//!
//! # 前提を先に確かめる
//!
//! 標本（`tests/common/sample.rs`）を使う検査は、**標本がその検査の前提を満たしていることを
//! 最初に検査する**（tasks.md の Implementation Notes の規則）。列の宣言順・入れ子のフィールド
//! の名前・配列の要素数の宣言は、`CompiledSchema` の公開面から読み直した値と突き合わせ、
//! 期待値を手書きの写しにしない（写しを書くと、宣言が変わったときに検査が黙って古くなる）。
//! 表示名の区切り文字だけは 1 箇所で literal に固定する（人が読む形そのものであり、
//! スキーマから導けないためである）。
//!
//! # 決定性の前提
//!
//! 標本の行識別子は発行時刻を含む ULID であり、組み立てのたびに変わる。本ファイルは標本の
//! **識別子を期待値に書かない**（構成の検査は列の構成だけを見ており、行の値も識別子も読まない。

mod common;

use common::sample::{sample, Sample, SampleOptions, COVERAGE_COLUMNS, SAMPLE_COLUMNS};
use data_grid::{
    derive_layout, ColumnIndex, ColumnLayout, ElementCount, Expandability, ExpansionState,
    LayoutColumn, NestedPathSegment, RowOrder, SortKey, ViewSpec, ViewState, ViewSummary,
    MAX_EXPANSION_DEPTH,
};
use document_format::IdFactory;
use schema_engine::compile::plan::{ColumnValidator, FieldValidator};
use schema_engine::{
    compile_declaration, ColumnDecl, CompiledSchema, Constraints, DeclaredKind, FieldDecl, Schema,
    TypeDecl, TypeDefinition, TypeKind, TypeRegistry,
};

// ---------------------------------------------------------------------------
// 標本の列の位置（`tests/common/sample.rs` の宣言の並び）
// ---------------------------------------------------------------------------

/// 入れ子のオブジェクト（インライン宣言。フィールドを 5 つ持つ）。
const DESTINATION: usize = 10;
/// 入れ子の配列（要素はインラインのオブジェクト。要素数の範囲つき）。
const LINE_ITEMS: usize = 11;
/// 入れ子の配列（上限だけを宣言する）。
const REVISIONS: usize = 27;

fn column(index: usize) -> ColumnIndex {
    ColumnIndex::new(index)
}

/// 標本を組み立て、その計画とともに返す。
fn sample_schema(rows: usize, columns: usize) -> (Sample, CompiledSchema) {
    let sample = sample(&SampleOptions::new(rows, columns));
    let schema = sample.compiled();
    (sample, schema)
}

/// オブジェクトの型のフィールドを読み出す（前提が崩れていれば落ちる）。
fn object_fields(schema: &CompiledSchema, index: usize) -> Box<[FieldValidator]> {
    match schema.validator(column(index)) {
        Some(ColumnValidator::Object { fields }) => fields.clone(),
        _ => panic!("前提が崩れた: 列 {index} はオブジェクトでなければならない"),
    }
}

/// 配列の型の要素数の宣言を読み出す（前提が崩れていれば落ちる）。
fn array_counts(schema: &CompiledSchema, index: usize) -> (Option<usize>, Option<usize>) {
    match schema.validator(column(index)) {
        Some(ColumnValidator::Array {
            min_items,
            max_items,
            ..
        }) => (*min_items, *max_items),
        _ => panic!("前提が崩れた: 列 {index} は配列でなければならない"),
    }
}

/// 検証器の種別（`schema-engine` の型カタログの札）。`None` は使用不能な列である。
fn kind_of(validator: Option<&ColumnValidator>) -> Option<TypeKind> {
    validator.map(|validator| match validator {
        ColumnValidator::Int { .. } => TypeKind::Int,
        ColumnValidator::Float { .. } => TypeKind::Float,
        ColumnValidator::Decimal { .. } => TypeKind::Decimal,
        ColumnValidator::Text { .. } => TypeKind::Text,
        ColumnValidator::Bool => TypeKind::Bool,
        ColumnValidator::Date { .. } => TypeKind::Date,
        ColumnValidator::DateTime { .. } => TypeKind::DateTime,
        ColumnValidator::Enum { .. } => TypeKind::Enum,
        ColumnValidator::Ref { .. } => TypeKind::Ref,
        ColumnValidator::Attachment => TypeKind::Attachment,
        ColumnValidator::Object { .. } => TypeKind::Object,
        ColumnValidator::Array { .. } => TypeKind::Array,
        ColumnValidator::Any => TypeKind::Any,
        ColumnValidator::Custom { .. } => TypeKind::Custom,
    })
}

// ---------------------------------------------------------------------------
// 検査のための宣言を組む補助（`schema-engine` の公開面だけを使う）
// ---------------------------------------------------------------------------

/// 種別による型の宣言。
fn kind(kind: TypeKind, constraints: Constraints) -> TypeDecl {
    TypeDecl::Kind {
        kind: DeclaredKind::Known(kind),
        constraints,
    }
}

/// 整数の型。
fn int() -> TypeDecl {
    kind(TypeKind::Int, Constraints::default())
}

/// 未知の種別（使用不能な列を作るために使う）。
fn unknown_kind() -> TypeDecl {
    TypeDecl::Kind {
        kind: DeclaredKind::Unknown("geo-point".into()),
        constraints: Constraints::default(),
    }
}

/// フィールド 1 本の宣言（既定値・説明は使わない）。
fn field(name: &str, ty: TypeDecl, required: bool) -> FieldDecl {
    FieldDecl {
        name: name.into(),
        ty,
        required,
        default: None,
        description: None,
    }
}

/// 任意のフィールド 1 本。
fn optional(name: &str, ty: TypeDecl) -> FieldDecl {
    field(name, ty, false)
}

/// 名前つきフィールドの集合の型。
fn object(fields: Vec<FieldDecl>) -> TypeDecl {
    kind(
        TypeKind::Object,
        Constraints {
            fields,
            ..Constraints::default()
        },
    )
}

/// 同一の型の並びの型（要素数の宣言つき）。
fn array(items: TypeDecl, min: Option<usize>, max: Option<usize>) -> TypeDecl {
    kind(
        TypeKind::Array,
        Constraints {
            items: Some(Box::new(items)),
            min_items: min,
            max_items: max,
            ..Constraints::default()
        },
    )
}

/// 列 1 本の宣言（必須・一意・既定値・説明は使わない）。
fn column_decl(name: &str, ty: TypeDecl) -> ColumnDecl {
    ColumnDecl {
        name: name.into(),
        ty,
        required: false,
        unique: false,
        default: None,
        description: None,
    }
}

/// `levels` 段の入れ子を持つオブジェクト（各段は葉 1 本と、次の段 1 本を持つ）。
///
/// **再帰ではない** — 書き下した有限の入れ子であり、降下を止めるのは段数の上限だけである。
/// 段の数だけ宣言が伸びるため、上限が無ければ任意に深い宣言を作れる（＝上限が無ければ構成が
/// 発散しうる形である）。深さ `i` の位置には `葉i`（整数）と `次i`（オブジェクト）が並ぶ。
fn spine(levels: usize) -> TypeDecl {
    assert!(levels >= 1, "段数は 1 以上である");
    // 最深部は葉だけを持つオブジェクト。そこから手前へ 1 段ずつ包む。
    let mut ty = object(vec![optional("葉", int())]);
    for level in (1..levels).rev() {
        ty = object(vec![
            optional(&format!("葉{level}"), int()),
            optional(&format!("次{level}"), ty),
        ]);
    }
    ty
}

/// 列 1 本だけの宣言。
fn single(name: &str, ty: TypeDecl) -> Schema {
    Schema {
        columns: vec![column_decl(name, ty)],
    }
}

/// 型定義を使わない宣言をコンパイルする。
fn compile(schema: &Schema) -> CompiledSchema {
    compile_declaration(schema, &[], &TypeRegistry::new()).expect("検査の宣言は妥当である")
}

/// 型定義を使う宣言をコンパイルする（**使用不能な列もそのまま返す**）。
fn compile_with(schema: &Schema, definitions: &[TypeDefinition]) -> CompiledSchema {
    compile_declaration(schema, definitions, &TypeRegistry::new()).expect("検査の宣言は妥当である")
}

/// 1 本の列を段数 `depth` まで展開する表示状態。
fn expanded_state(index: usize, depth: u8) -> ViewState {
    let mut state = ViewState::new();
    state.set_expansion(ExpansionState::expanded_to(column(index), depth));
    state
}

/// 構成の表示名の並び（検査の可読性のため）。
fn names(layout: &ColumnLayout) -> Vec<&str> {
    layout
        .columns()
        .iter()
        .map(|entry| entry.name.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// 要件 5.1 / 5.2: 折りたたみは 1 列、展開は内側のフィールドを宣言順に
// ---------------------------------------------------------------------------

/// 折りたたんだ入れ子の列は、構成の要素を**ちょうど 1 件**持つ（要件 5.1, 5.2）。
#[test]
fn a_collapsed_nested_column_is_exactly_one_layout_column() {
    let (_sample, schema) = sample_schema(64, COVERAGE_COLUMNS);

    // 前提: 標本の列 10 はフィールドを持つオブジェクトである。
    let fields = object_fields(&schema, DESTINATION);
    assert!(!fields.is_empty(), "前提が崩れた: 届け先はフィールドを持つ");

    let layout = ViewState::new().layout(&schema);
    // 折りたたみでは列は 1 本につきちょうど 1 件（入れ子を指定しなければ全部折りたたみである）。
    assert_eq!(COVERAGE_COLUMNS, layout.len());
    let entry = &layout.columns()[DESTINATION];
    assert_eq!(column(DESTINATION), entry.column);
    assert!(entry.path.is_root(), "折りたたみでは内側の位置を持たない");
    assert_eq!("届け先", entry.name);
    assert_eq!(Some(TypeKind::Object), entry.kind);
    assert_eq!(None, entry.element_count, "オブジェクトは要素数を持たない");
    assert_eq!(Expandability::Available, entry.expandability);
    assert!(entry.is_expandable());

    // 入れ子でない列（品番）と、要素数の並びの列（明細）は、展開を指定しても 1 件のままである
    // （配列は列として展開されず、要素数で示す。要件 5.6）。
    for index in [0, LINE_ITEMS] {
        let expanded = expanded_state(index, MAX_EXPANSION_DEPTH).layout(&schema);
        assert_eq!(
            ViewState::new().layout(&schema),
            expanded,
            "列 {index} は展開の対象ではない"
        );
    }
}

/// 展開すると内側のフィールドが宣言順に現れ、折りたたむと元の 1 件へ戻る（要件 5.1, 5.2）。
#[test]
fn expanding_lists_the_inner_fields_in_declaration_order() {
    let (_sample, schema) = sample_schema(64, COVERAGE_COLUMNS);
    let fields = object_fields(&schema, DESTINATION);

    let collapsed = ViewState::new().layout(&schema);
    let mut state = expanded_state(DESTINATION, MAX_EXPANSION_DEPTH);
    let expanded = state.layout(&schema);

    // 展開した列は内側のフィールドに置き換わる（親の列そのものは現れない）。
    assert_eq!(COVERAGE_COLUMNS - 1 + fields.len(), expanded.len());
    let names = names(&expanded);
    let expected: Vec<String> = fields
        .iter()
        .map(|field| format!("届け先.{}", field.name()))
        .collect();
    assert_eq!(
        expected,
        names[DESTINATION..DESTINATION + fields.len()],
        "内側のフィールドは宣言順に並ぶ"
    );
    // 表示名の区切りは `.` である（人が読む形そのものであり、スキーマからは導けない。
    // 地域に依る書式は使わない）。
    assert_eq!("届け先.郵便番号", names[DESTINATION]);

    for (offset, field) in fields.iter().enumerate() {
        let entry = &expanded.columns()[DESTINATION + offset];
        assert_eq!(column(DESTINATION), entry.column, "内側の列も最上位の列を指す");
        assert_eq!(
            vec![NestedPathSegment::Field(field.name().into())],
            entry.path.segments().to_vec(),
            "内側の位置はフィールド名 1 段である"
        );
        assert_eq!(
            kind_of(Some(field.validator())),
            entry.kind,
            "葉の型は内側のフィールドの型そのものである（8.5・7.4 が入力手段を選ぶのに使う）"
        );
        assert_eq!(1, entry.path.len(), "段数は位置の段数で観測できる");
    }
    // 内側の位置はフィールド名だけで構成される（配列は展開されないため添字は現れない）。
    assert!(expanded
        .columns()
        .iter()
        .flat_map(|entry| entry.path.segments())
        .all(|segment| matches!(segment, NestedPathSegment::Field(_))));
    // 独立した錨として 1 件だけ literal でも固定する（郵便番号は書式つきの文字列である）。
    assert_eq!(Some(TypeKind::Text), expanded.columns()[DESTINATION].kind);

    // 折りたたむと元の 1 件へ戻る（折りたたみの指定は、指定が無いときと同じ構成になる）。
    state.set_expansion(ExpansionState::collapsed(column(DESTINATION)));
    assert_eq!(collapsed, state.layout(&schema));
}

// ---------------------------------------------------------------------------
// 要件 5.4: 段数の上限と、詳細表示へ委ねる印
// ---------------------------------------------------------------------------

/// 上限段まで展開でき、上限に達した入れ子は降りずに**詳細表示へ委ねる印**が付く（要件 5.4）。
#[test]
fn the_depth_limit_stops_the_descent_and_marks_the_capped_position() {
    // 上限より深い入れ子（12 段。再帰ではなく、書き下した有限の入れ子である）。
    let schema = compile(&single("根", spine(12)));
    // 正典の値そのものを固定する（上限の意味が変われば、以下の件数も変わる）。
    assert_eq!(3, MAX_EXPANSION_DEPTH);

    let layout = expanded_state(0, u8::MAX).layout(&schema);
    // 上限を超える段数を要求しても、降りるのは上限までである。展開したオブジェクトは内部の
    // フィールドに**置き換わる**（要件 5.1, 5.2）ため、途中のオブジェクトは列として現れない。
    assert_eq!(
        vec![
            "根.葉1",
            "根.次1.葉2",
            "根.次1.次2.葉3",
            "根.次1.次2.次3",
        ],
        names(&layout)
    );
    assert_eq!(4, layout.len());
    // 上限に達した位置だけが詳細表示へ委ねられる（内側の位置の段数が上限に一致する）。
    let capped = &layout.columns()[3];
    assert_eq!("根.次1.次2.次3", capped.name);
    assert_eq!(Expandability::Capped, capped.expandability);
    assert!(capped.requires_detail());
    assert!(!capped.is_expandable());
    assert_eq!(usize::from(MAX_EXPANSION_DEPTH), capped.path.len());
    assert_eq!(Some(TypeKind::Object), capped.kind, "内側を持つ位置である");
    assert_eq!(column(0), capped.column, "内側の位置も最上位の列を指す");
    // 葉には印が付かない（内側を持たない）。
    assert_eq!(Expandability::Leaf, layout.columns()[0].expandability);
    assert!(!layout.columns()[0].requires_detail());
    assert_eq!(Some(TypeKind::Int), layout.columns()[0].kind);

    // 浅い段数を要求すればそこまでで止まる。止まった位置は**まだ降りられる**ので印は付かず、
    // 段数を増やせば降りられる（「上限に達した」と「指定で止まった」の区別である）。
    let shallow = expanded_state(0, 1).layout(&schema);
    assert_eq!(vec!["根.葉1", "根.次1"], names(&shallow));
    assert_eq!(Expandability::Available, shallow.columns()[1].expandability);
    assert!(shallow.columns()[1].is_expandable());
    assert!(!shallow.columns()[1].requires_detail());
    assert_eq!(Some(TypeKind::Object), shallow.columns()[1].kind);
    let shallow = expanded_state(0, 2).layout(&schema);
    assert_eq!(
        vec!["根.葉1", "根.次1.葉2", "根.次1.次2"],
        names(&shallow)
    );
    assert_eq!(Expandability::Available, shallow.columns()[2].expandability);
    assert!(!shallow.columns()[2].requires_detail());

    // 要求した段数が上限以上でも、切り詰めた結果は同じである。
    assert_eq!(
        layout,
        expanded_state(0, MAX_EXPANSION_DEPTH).layout(&schema)
    );
}

// ---------------------------------------------------------------------------
// タスク 2.3 の明示的な検査: 構成が発散しない
// ---------------------------------------------------------------------------

/// 上限をはるかに超える入れ子でも、構成は有限で上限に収まる（件数を exact に固定する）。
#[test]
fn nesting_deeper_than_the_limit_yields_a_finite_layout() {
    // 上限（3 段）に対して 64 段の宣言。上限が無ければ 64 列が生まれる（この標本は**線形の
    // 背骨**であり、各段は 1 フィールドしか持たない）。上限があれば件数は段数だけで決まり、
    // 宣言の深さに依らない。
    let schema = compile(&single("根", spine(64)));
    let state = expanded_state(0, u8::MAX);
    let layout = state.layout(&schema);
    let names = names(&layout);
    assert_eq!(4, layout.len(), "件数は上限だけで決まる");
    // `names` は `layout` から作られるので件数は一致して当然である。**ここで見るのは
    // 名前の中身**（上限まで降りた 3 段のパスが `.` で連結されていること）であり、
    // 件数ではない（件数は上の `layout.len()` が固定する）。
    assert_eq!(
        vec![
            "根.葉1",
            "根.次1.葉2",
            "根.次1.次2.葉3",
            "根.次1.次2.次3"
        ],
        names
    );
    // 最深部は上限に一致し、それより深い位置は列として現れない。
    assert!(!names.iter().any(|name| name.contains("次4")));
    assert!(!names.iter().any(|name| name.ends_with("次1")));
    assert!(layout
        .columns()
        .last()
        .expect("構成は空でない")
        .requires_detail());

    // 導出を繰り返しても件数は増えない（前回の結果を引き継がない）。
    for _ in 0..3 {
        assert_eq!(4, state.layout(&schema).len());
    }
}

/// 再帰する型定義は**その列だけ使用不能**に落ちるため、展開の対象にならない（要件 5.4, 11.7）。
///
/// 宣言そのものは拒否されない（`required: false` や配列を経由する再帰は正当な宣言である）。
/// 展開できないのは、検証器が入れ子の検証器を**値として**内包するためである。
#[test]
fn recursive_type_definitions_are_unusable_and_never_diverge() {
    let id = IdFactory::new().new_type_def_id();
    let definitions = vec![TypeDefinition {
        id,
        // 値が有限の大きさで存在しえない循環ではない（子は任意であり、配列は空になりうる）。
        definition: object(vec![
            optional("子", TypeDecl::Ref(id)),
            optional("群", array(TypeDecl::Ref(id), None, None)),
        ]),
    }];
    let schema = Schema {
        columns: vec![
            column_decl("木", TypeDecl::Ref(id)),
            column_decl("数", int()),
        ],
    };
    let schema = compile_with(&schema, &definitions);

    // 前提: 展開できない定義を参照する列は使用不能に落ちる（スキーマは破棄されない）。
    assert!(schema.validator(column(0)).is_none());
    assert_eq!("木", schema.columns()[0]);
    // 残りの列は正しく引ける（列の添字も動かない）。
    assert_eq!(Some(TypeKind::Int), kind_of(schema.validator(column(1))));

    let layout = expanded_state(0, u8::MAX).layout(&schema);
    assert_eq!(2, layout.len());
    let entry = &layout.columns()[0];
    assert_eq!(column(0), entry.column);
    assert!(entry.path.is_root());
    assert_eq!("木", entry.name);
    assert_eq!(None, entry.kind);
    assert_eq!(None, entry.element_count);
    assert_eq!(Expandability::Leaf, entry.expandability);
    assert!(!entry.requires_detail());
    assert_eq!("数", layout.columns()[1].name);
}

// ---------------------------------------------------------------------------
// 要件 5.6: 同一の型の並びの要素数
// ---------------------------------------------------------------------------

/// 配列の列は宣言された要素数の能力を持ち、配列でない列は持たない（要件 5.6）。
#[test]
fn array_columns_carry_their_declared_element_count() {
    let (_sample, schema) = sample_schema(64, SAMPLE_COLUMNS);

    // 前提: 標本の列 11 は両端つきの配列、列 27 は上限だけの配列である。
    let (min, max) = array_counts(&schema, LINE_ITEMS);
    assert_eq!((Some(1), Some(8)), (min, max));
    let (min, max) = array_counts(&schema, REVISIONS);
    assert_eq!((None, Some(4)), (min, max));
    // 前提: 標本の列 0（品番）は配列でない（書式つきの文字列である）。
    assert_eq!(Some(TypeKind::Text), kind_of(schema.validator(column(0))));

    let layout = ViewState::new().layout(&schema);
    assert_eq!(
        Some(ElementCount {
            items: TypeKind::Object,
            min: Some(1),
            max: Some(8),
        }),
        layout.columns()[LINE_ITEMS].element_count
    );
    assert_eq!(
        Some(ElementCount {
            items: TypeKind::Object,
            min: None,
            max: Some(4),
        }),
        layout.columns()[REVISIONS].element_count
    );
    // 上下限が一致する宣言は要素数そのものである（5.6 の「要素数を導出できる」）。
    let exact = layout.columns()[LINE_ITEMS].element_count.expect("配列である");
    assert_eq!(None, exact.exact(), "1..=8 は 1 つに定まらない");
    assert_eq!(None, layout.columns()[0].element_count);
    assert_eq!(Some(TypeKind::Array), layout.columns()[LINE_ITEMS].kind);
}

/// 上下限が一致する配列の宣言は、要素数を 1 つに定める（要件 5.6）。
#[test]
fn an_array_with_equal_bounds_has_an_exact_element_count() {
    let schema = compile(&single(
        "固定",
        array(int(), Some(3), Some(3)),
    ));
    let layout = ViewState::new().layout(&schema);
    let count = layout.columns()[0].element_count.expect("配列である");
    assert_eq!(Some(3), count.exact());
    assert_eq!(TypeKind::Int, count.items);
    // 上下限が 0 の配列も要素数は 0 に定まる（空の並び）。
    let schema = compile(&single("空", array(int(), Some(0), Some(0))));
    let count = ViewState::new().layout(&schema).columns()[0]
        .element_count
        .expect("配列である");
    assert_eq!(Some(0), count.exact());
}

/// 入れ子の内側の配列も要素数の能力を持ち、**配列は列として展開されない**（要件 5.6）。
#[test]
fn a_nested_array_reports_its_element_count_instead_of_being_expanded() {
    let schema = compile(&single(
        "親",
        object(vec![
            optional("子", array(int(), Some(2), None)),
            optional("孫", array(int(), None, None)),
        ]),
    ));
    let layout = expanded_state(0, MAX_EXPANSION_DEPTH).layout(&schema);
    assert_eq!(vec!["親.子", "親.孫"], names(&layout));
    assert_eq!(
        Some(ElementCount {
            items: TypeKind::Int,
            min: Some(2),
            max: None,
        }),
        layout.columns()[0].element_count
    );
    // 要素数の宣言が 1 つも無くても、配列である限り要素数の能力はある（上下限が開いている）。
    assert_eq!(
        Some(ElementCount {
            items: TypeKind::Int,
            min: None,
            max: None,
        }),
        layout.columns()[1].element_count
    );
    assert_eq!(Some(TypeKind::Array), layout.columns()[1].kind);
    // 配列は上限に達していなくても展開されない（要素数で示す）。
    assert_eq!(Expandability::Leaf, layout.columns()[1].expandability);
    assert!(!layout.columns()[1].is_expandable());
}

/// 入れ子の内側のオブジェクトは、上限まで展開される（展開した位置は内側に置き換わる）。
#[test]
fn a_nested_object_is_expanded_up_to_the_limit() {
    // 親 ⊃ 子 ⊃ {孫(整数), 曾孫 ⊃ 玄孫(整数)}。
    let schema = compile(&single(
        "親",
        object(vec![optional(
            "子",
            object(vec![
                optional("孫", int()),
                optional("曾孫", object(vec![optional("玄孫", int())])),
            ]),
        )]),
    ));
    // 上限（3 段）まで降りる: 親 → 子 → 曾孫 → 玄孫（孫は葉である）。
    let layout = expanded_state(0, MAX_EXPANSION_DEPTH).layout(&schema);
    assert_eq!(vec!["親.子.孫", "親.子.曾孫.玄孫"], names(&layout));
    assert_eq!(2, layout.len());
    assert_eq!(Some(TypeKind::Int), layout.columns()[0].kind);
    assert_eq!(None, layout.columns()[0].element_count);
    assert_eq!(Expandability::Leaf, layout.columns()[0].expandability);
    assert_eq!(Some(TypeKind::Int), layout.columns()[1].kind);
    assert_eq!(3, layout.columns()[1].path.len());

    // 1 段だけ展開すると「親.子」だけで止まる（子はまだ降りられるので印は付かない）。
    let one = expanded_state(0, 1).layout(&schema);
    assert_eq!(vec!["親.子"], names(&one));
    assert_eq!(Some(TypeKind::Object), one.columns()[0].kind);
    assert_eq!(Expandability::Available, one.columns()[0].expandability);
    assert!(!one.columns()[0].requires_detail());

    // 2 段では内側の 2 位置が並ぶ（孫は葉、曾孫はまだ降りられる）。
    let two = expanded_state(0, 2).layout(&schema);
    assert_eq!(vec!["親.子.孫", "親.子.曾孫"], names(&two));
    assert_eq!(Expandability::Leaf, two.columns()[0].expandability);
    assert_eq!(Expandability::Available, two.columns()[1].expandability);
    assert_eq!(Some(TypeKind::Object), two.columns()[1].kind);
}

/// 上限に達した位置は**内側を持っている**（印が「内側が無い」と混ざらない。要件 5.4）。
#[test]
fn a_capped_position_is_not_a_leaf() {
    // 親 ⊃ 子 ⊃ 孫 ⊃ 曾孫 ⊃ 玄孫（曾孫は内側を持つオブジェクトである）。
    let schema = compile(&single(
        "親",
        object(vec![optional(
            "子",
            object(vec![optional(
                "孫",
                object(vec![optional("曾孫", object(vec![optional("玄孫", int())]))]),
            )]),
        )]),
    ));
    let layout = expanded_state(0, MAX_EXPANSION_DEPTH).layout(&schema);
    // 段数 3 の位置「親.子.孫.曾孫」は内側（玄孫）を持つが、上限に達しているため降りない。
    assert_eq!(vec!["親.子.孫.曾孫"], names(&layout));
    assert_eq!(1, layout.len());
    let capped = &layout.columns()[0];
    assert_eq!(Some(TypeKind::Object), capped.kind, "内側を持つ位置である");
    assert_eq!(usize::from(MAX_EXPANSION_DEPTH), capped.path.len());
    assert_eq!(Expandability::Capped, capped.expandability);
    assert!(capped.requires_detail());
    assert!(!capped.is_expandable());
    assert_ne!(Expandability::Leaf, capped.expandability);

    // 段数を 1 つ減らすと、その位置はまだ降りられる（上限に達したから止まっていることの観測）。
    let two = expanded_state(0, 2).layout(&schema);
    assert_eq!(vec!["親.子.孫"], names(&two));
    assert_eq!(Some(TypeKind::Object), two.columns()[0].kind);
    assert_eq!(Expandability::Available, two.columns()[0].expandability);
    assert!(two.columns()[0].is_expandable());
    assert!(!two.columns()[0].requires_detail());
    assert_eq!(2, two.columns()[0].path.len());
}

// ---------------------------------------------------------------------------
// 要件 5.3: 展開の状態は順序の再計算で失われない
// ---------------------------------------------------------------------------

/// 順序を再計算しても展開の状態は 1 件ずつそのままである（要件 5.3）。
#[test]
fn expansion_survives_recomputing_the_order() {
    let (sample, schema) = sample_schema(64, COVERAGE_COLUMNS);
    let mut state = ViewState::new();
    state.set_expansion(ExpansionState::expanded_to(column(DESTINATION), 2));
    state.set_expansion(ExpansionState::collapsed(column(LINE_ITEMS)));
    let before = state.expansion.clone();
    let layout_before = state.layout(&schema);
    assert_eq!(2, before.len());

    state.spec = ViewSpec {
        sort: vec![SortKey {
            column: column(0),
            descending: false,
        }],
        filters: Vec::new(),
    };
    let mut order = RowOrder::default();
    let summary: ViewSummary = state.recompute_order(&mut order, sample.document(), sample.sheet());
    // 再計算は実際に行われた（可視行が標本の行数に一致し、隠された行は無い）。
    assert_eq!(sample.rows(), summary.visible);
    assert_eq!(0, summary.hidden);
    assert_eq!(sample.rows(), order.len());

    // 展開の状態は 1 件ずつそのままである（列の同一性・展開の有無・段数のすべて）。
    assert_eq!(before, state.expansion);
    assert_eq!(layout_before, state.layout(&schema));

    // 別の指定での再計算でも失われない。
    state.spec = ViewSpec::default();
    let summary = state.recompute_order(&mut order, sample.document(), sample.sheet());
    assert_eq!(sample.rows(), summary.visible);
    assert_eq!(before, state.expansion);

    // 展開状態への共有借用を持ったまま再計算を呼べる（消す経路が型の上に無い）。
    let held = state
        .expansion_of(column(DESTINATION))
        .expect("届け先の展開の指定は残っている");
    state.recompute_order(&mut order, sample.document(), sample.sheet());
    assert!(held.expanded);
    assert_eq!(2, held.depth);
    assert_eq!(column(DESTINATION), held.column);
}

/// 折りたたみの指定も再計算で失われない（指定が無い状態へ戻らない）。
#[test]
fn a_collapsed_expansion_also_survives_recomputing_the_order() {
    let (sample, schema) = sample_schema(64, COVERAGE_COLUMNS);
    let mut state = ViewState::new();
    state.set_expansion(ExpansionState::collapsed(column(DESTINATION)));
    let before = state.layout(&schema);
    let mut order = RowOrder::default();
    state.recompute_order(&mut order, sample.document(), sample.sheet());
    assert_eq!(before, state.layout(&schema));
    assert!(!state
        .expansion_of(column(DESTINATION))
        .expect("届け先の指定は残っている")
        .expanded);
}

// ---------------------------------------------------------------------------
// 決定性
// ---------------------------------------------------------------------------

/// 同じ入力からの導出は常に同じ構成になり、`expansion` の並びの順に依らない。
#[test]
fn the_layout_is_deterministic_across_repeated_derivations() {
    let (_sample, schema) = sample_schema(64, SAMPLE_COLUMNS);
    let mut state = ViewState::new();
    state.set_expansion(ExpansionState::expanded_to(
        column(DESTINATION),
        MAX_EXPANSION_DEPTH,
    ));
    state.set_expansion(ExpansionState::expanded_to(column(LINE_ITEMS), 2));
    let first = state.layout(&schema);
    for _ in 0..3 {
        assert_eq!(first, state.layout(&schema));
    }
    assert!(first.len() > SAMPLE_COLUMNS, "展開が効いている");

    // 同じ指定を別の順で入れた状態も同じ構成になる（`expansion` は列の昇順へ正規化される）。
    let mut reversed = ViewState::new();
    reversed.set_expansion(ExpansionState::expanded_to(column(LINE_ITEMS), 2));
    reversed.set_expansion(ExpansionState::expanded_to(
        column(DESTINATION),
        MAX_EXPANSION_DEPTH,
    ));
    assert_eq!(state.expansion, reversed.expansion);
    assert_eq!(first, reversed.layout(&schema));

    // 手で組んだ `expansion` の並びを逆にしても結果は同じである（導出は並びに依らない）。
    let mut flipped = reversed.expansion.clone();
    flipped.reverse();
    assert_eq!(first, derive_layout(&schema, &flipped));

    // 同じ列に複数の指定がある場合は**後ろの指定が前を上書きする**。
    let mut overwritten = state.expansion.clone();
    overwritten.push(ExpansionState::collapsed(column(DESTINATION)));
    let expected = derive_layout(
        &schema,
        &[
            ExpansionState::expanded_to(column(LINE_ITEMS), 2),
            ExpansionState::collapsed(column(DESTINATION)),
        ],
    );
    let derived = derive_layout(&schema, &overwritten);
    assert_eq!(expected, derived);
    assert_ne!(first, derived, "上書きが効いていなければ同じ構成になる");
}

// ---------------------------------------------------------------------------
// 使用不能な列と、フィールドを持たないオブジェクト
// ---------------------------------------------------------------------------

/// 使用不能な列（`validator` が `None`）は展開されず、panic しない。
#[test]
fn an_unusable_column_is_not_expanded_and_does_not_panic() {
    let schema = compile(&Schema {
        columns: vec![
            column_decl("壊れた型", unknown_kind()),
            column_decl("数", int()),
        ],
    });
    // 前提: 未知の種別の列だけが使用不能であり、他の列は引ける。
    assert!(schema.validator(column(0)).is_none());
    assert_eq!(Some(TypeKind::Int), kind_of(schema.validator(column(1))));

    let layout = expanded_state(0, u8::MAX).layout(&schema);
    assert_eq!(2, layout.len());
    let entry = &layout.columns()[0];
    assert_eq!(column(0), entry.column);
    assert!(entry.path.is_root());
    assert_eq!("壊れた型", entry.name);
    assert_eq!(None, entry.kind);
    assert_eq!(None, entry.element_count);
    assert_eq!(Expandability::Leaf, entry.expandability);
    assert!(!entry.requires_detail());
    assert_eq!(Some(TypeKind::Int), layout.columns()[1].kind);
}

/// フィールドを持たないオブジェクトを展開しても、列は消えずに 1 件のまま残る。
#[test]
fn expanding_an_object_without_fields_keeps_the_column() {
    let schema = compile(&single("空", object(Vec::new())));
    let layout = expanded_state(0, MAX_EXPANSION_DEPTH).layout(&schema);
    assert_eq!(1, layout.len());
    let entry: &LayoutColumn = &layout.columns()[0];
    assert_eq!("空", entry.name);
    assert!(entry.path.is_root());
    assert_eq!(Some(TypeKind::Object), entry.kind);
    assert_eq!(Expandability::Leaf, entry.expandability);
    assert_eq!(ViewState::new().layout(&schema), layout);
}

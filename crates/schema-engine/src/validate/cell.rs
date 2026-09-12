//! 1 セルの判定と入れ子の再帰（design.md「コンポーネントとファイルの対応」の
//! `CellValidator`。tasks.md 5.1。要件 2.6, 3.4, 4.4, 5.3, 11.3, 11.5）。
//!
//! 値それ自体に閉じた性質（型・範囲・長さ・書式・桁・必須・入れ子）だけを判定する。
//! 行を跨ぐ性質（一意性と参照の実在）は本モジュールでは判定しない（design.md
//!「Validate Layer / SheetValidator」の線引き。要件 4.7 と 9.2 は [`super::unique`] /
//! [`super::refs`] が持つ）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは `compile` 層までと、同じ `validate` 層の
//! [`super::report`] を参照する。
//!
//! # 打ち切らない（要件 5.3）
//!
//! 1 つの行に複数の違反があっても最初の違反で止めない。列は宣言順に、入れ子は値の内側へ
//! 深さ優先で降り、違反のたびに [`Violation`] を蓄積する。**セル自身の違反があっても
//! 内側の走査は続ける** — 配列の要素数の逸脱と要素の中身の逸脱は別の違反であり、
//! 一方を報告しただけで他方を黙って落とさない。
//!
//! # 位置はフィールド名と添字の並びで運ぶ（要件 3.4）
//!
//! 入れ子へ降りるたびに [`ValuePath`] へ 1 段積み、戻るときに畳む。違反のときだけ現在の
//! 位置を複製する。訪問は深さ優先であり、同じ入力に対して常に同じ並びになる
//! （要件 5.5 の第 1 段の順序はこの訪問順である）。
//!
//! # 使用不能な列はその列の値だけを違反にする（要件 11.7）
//!
//! [`CompiledSchema::validator`] が `None` を返す列（未知の `kind`・未登録の拡張型・
//! 有限に展開できない再帰型）では、**その列のすべての値**を
//! `ViolationReason::UnusableColumn` に落とす。値なしも例外ではない — 適合の判定そのものが
//! 存在しないためである。他の列の判定はそのまま続き、スキーマは破棄されない
//! （design.md「解決できない型の扱い」）。
//!
//! # 一括経路との関係（tasks.md 4.2 / 5.4）
//!
//! 本モジュールが判定するのは**1 行の内側**である。拡張型の一括判定
//! （[`CustomType::validate_batch`](crate::registry::CustomType::validate_batch)）を列ごとに
//! 1 回呼ぶのは一括検証（タスク 5.4）の仕事であり、ここでは [`ColumnValidator::check`] が
//! 1 件ずつ実装へ委ねる。判定の失敗（`Err`）は
//! [`ColumnViolation::CustomFailed`] としてその値の違反に閉じ込め、**次の値の走査を
//! 続ける**（要件 11.5）。違反の理由は組込型と同じ [`ViolationReason`] の変種として
//! 報告される（要件 11.3）。
//!
//! 行の内側の入口は 2 つある。[`validate_row`] は計画の全列を宣言順に判定し、
//! [`validate_row_columns`] は**指定した列だけ**を判定する（要件 10.5 の再検証）。
//! 判定の本体は [`validate_column`] の 1 つだけであり、列を絞るかどうかで規則は変わらない。
//!
//! [`CustomType::validate_batch`]: crate::registry::CustomType::validate_batch

use document_format::{CellValue, NestedValue, RowId};

use crate::compile::plan::{ColumnValidator, ColumnVerdict, ColumnViolation};
use crate::compile::{ColumnIndex, CompiledSchema};
use crate::declaration::codec::kind_token;

use super::report::{Expected, ValuePath, Violation, ViolationReason, ViolationReport};

/// 1 行分の値を検証し、違反を [`ViolationReport`] へ蓄積する（tasks.md 5.1。
/// 要件 2.6, 3.4, 4.4, 5.3, 11.3, 11.5）。
///
/// `values` は [`CompiledSchema::columns`] と同じ並びの行の値である（design.md
///「Data Models / Domain Model」の不変条件 — `Row::values()` もこの添字で並ぶ）。計画より
/// 短い並びは残りを値なしとして扱い、計画の列数を超える値は計画の外として判定しない。
///
/// `row` は違反の属する行である。`None` を渡せるのは、まだ行に属さない値（書き込み経路の
/// 判定。タスク 6.2）を検証するときだけであり、シート全体の走査（タスク 5.4）は必ず
/// `Some` を渡す（要件 5.1）。
///
/// **列は宣言順に、入れ子は深さ優先で降りる。** 1 つの違反で止めない（要件 5.3）。
pub fn validate_row(
    schema: &CompiledSchema,
    row: Option<RowId>,
    values: &[CellValue],
    report: &mut ViolationReport,
) {
    for index in 0..schema.column_count() {
        validate_column(schema, row, values, ColumnIndex::new(index), report);
    }
}

/// 指定した列だけを 1 行分検証する（tasks.md 5.4 の列指定の再検証。要件 10.5）。
///
/// [`validate_row`] と同じ判定を、`columns` に挙げた列についてだけ行う。スキーマの一部が
/// 変わったときに全列を舐め直さないための経路であり、一括検証（[`super::validate_columns`]）
/// が使う。計画の外の添字は判定しない（列名が無く、違反の位置を組み立てられない）。
pub fn validate_row_columns(
    schema: &CompiledSchema,
    row: Option<RowId>,
    values: &[CellValue],
    columns: &[ColumnIndex],
    report: &mut ViolationReport,
) {
    for column in columns {
        validate_column(schema, row, values, *column, report);
    }
}

/// 1 列分の判定（[`validate_row`] と [`validate_row_columns`] が共有する本体）。
///
/// 使用不能な列（未知の `kind`・未登録の拡張型・有限に展開できない再帰型）は、値の種類を
/// 問わず**その列のすべての値**を [`ViolationReason::UnusableColumn`] に落とす（要件 11.7。
/// 適合の判定そのものが存在しないため、値なしも例外ではない）。
fn validate_column(
    schema: &CompiledSchema,
    row: Option<RowId>,
    values: &[CellValue],
    column: ColumnIndex,
    report: &mut ViolationReport,
) {
    let index = column.index();
    let Some(name) = schema.columns().get(index) else {
        return;
    };
    let mut cursor = Cursor {
        row,
        column,
        column_name: name.as_str(),
        path: ValuePath::root(),
    };
    let Some(validator) = schema.validator(column) else {
        let actual = values.get(index).cloned().unwrap_or(CellValue::Null);
        let kind = schema.unusable_kind(column).unwrap_or_default();
        report.push(cursor.violation(ViolationReason::UnusableColumn {
            expected: Expected::Usable,
            actual,
            kind: kind.into(),
        }));
        return;
    };

    let null = CellValue::Null;
    let value = values.get(index).unwrap_or(&null);
    inspect(
        &mut cursor,
        value,
        validator,
        schema.required(column),
        report,
    );
}

/// 1 つのセルの中を再帰する間ずっと変わらない位置（行・列の添字・列名）と、降りるたびに
/// 伸び縮みする入れ子の位置（[`ValuePath`]）。
///
/// 再帰の引数を位置ごとに運び直さないために束ねる。入れ子の内側の違反も列名は**セルの列名**
/// のままである（入れ子のフィールド名は [`ValuePath`] が運ぶ）。
struct Cursor<'a> {
    row: Option<RowId>,
    column: ColumnIndex,
    column_name: &'a str,
    path: ValuePath,
}

impl Cursor<'_> {
    /// 現在の位置で違反 1 件を組み立てる（要件 5.1）。
    fn violation(&self, reason: ViolationReason) -> Violation {
        Violation::new(
            self.row,
            self.column,
            self.column_name,
            self.path.clone(),
            reason,
        )
    }
}

/// 値 1 つを検証し、違反を蓄積してから入れ子の内側へ降りる（要件 2.7, 3.4, 4.4, 5.3）。
///
/// `required` は値なしの受理可否である。列では [`CompiledSchema::required`] が、入れ子の
/// フィールドでは [`FieldValidator::required`] が決める。配列の要素には存在の制約が無い
/// ため常に `false` である（存在の制約は列とフィールドだけが持つ。tasks.md 3.1）。
///
/// [`FieldValidator::required`]: crate::compile::plan::FieldValidator::required
fn inspect(
    cursor: &mut Cursor<'_>,
    value: &CellValue,
    validator: &ColumnValidator,
    required: bool,
    report: &mut ViolationReport,
) {
    // 値なしは型の側では適合である（`ColumnValidator::check` と同じ扱い）。受理されるかは
    // `required` が決める（design.md「組込型カタログと `CellValue` への写像」）。
    if matches!(value, CellValue::Null) {
        if required {
            report.push(cursor.violation(ViolationReason::MissingValue {
                expected: Expected::Present,
                actual: CellValue::Null,
            }));
        }
        return;
    }

    // セル自身の判定。組込型は列挙体の直接マッチであり、拡張型だけが実装へ委ねる。
    // 拡張型の拒否も失敗もこの 1 箇所で組込型と同じ違反の形へ落ちる（要件 11.3, 11.5）。
    if let ColumnVerdict::Violating(violation) = validator.check(value) {
        report.push(cursor.violation(reason(violation, value)));
    }

    // 入れ子の内側へ降りる。**セル自身の違反があっても続ける** — 配列の要素数の逸脱と
    // 要素の中身の逸脱は別の違反であり、一方を報告しただけで他方を落とさない（要件 5.3）。
    match (validator, value) {
        (ColumnValidator::Object { fields }, CellValue::Nested(NestedValue::Object(entries))) => {
            for field in fields {
                cursor.path.push_field(field.name());
                // 宣言されたフィールドが値に無いことは値なしと同じである（要件 4.4）。
                let null = CellValue::Null;
                let inner = entries
                    .iter()
                    .find(|(key, _)| key == field.name())
                    .map(|(_, value)| value)
                    .unwrap_or(&null);
                inspect(cursor, inner, field.validator(), field.required(), report);
                cursor.path.pop();
            }
        }
        (ColumnValidator::Array { items, .. }, CellValue::Nested(NestedValue::Array(elements))) => {
            for (index, element) in elements.iter().enumerate() {
                cursor.path.push_index(index);
                inspect(cursor, element, items, false, report);
                cursor.path.pop();
            }
        }
        _ => {}
    }
}

/// 検証器の違反（`compile` 層の語彙）を違反の理由（`validate` 層の語彙）へ写す
/// （要件 5.2, 11.3）。
///
/// 層の鎖は `compile → validate` の一方向であるため、`compile` 層は本層の型を参照できず、
/// 同じ文脈を自層の語彙（`plan::ColumnViolation`）で運ぶ。**写すのはこの 1 箇所だけ**で
/// あり、組込型も拡張型も同じ [`ViolationReason`] の変種へ落ちる（要件 11.3）。
fn reason(violation: ColumnViolation, actual: &CellValue) -> ViolationReason {
    let actual = actual.clone();
    match violation {
        ColumnViolation::TypeMismatch { kind } => ViolationReason::TypeMismatch {
            expected: Expected::Kind(kind_token(kind).into()),
            actual,
        },
        ColumnViolation::OutOfRange { min, max } => ViolationReason::OutOfRange {
            expected: Expected::Range { min, max },
            actual,
        },
        ColumnViolation::LengthOutOfRange { min, max } => ViolationReason::LengthOutOfRange {
            expected: Expected::Length { min, max },
            actual,
        },
        ColumnViolation::PatternMismatch { pattern } => ViolationReason::PatternMismatch {
            expected: Expected::Pattern(pattern),
            actual,
        },
        ColumnViolation::ChoiceNotAllowed { choices } => ViolationReason::ChoiceNotAllowed {
            expected: Expected::Choices(choices.into_vec()),
            actual,
        },
        ColumnViolation::PrecisionExceeded { precision, scale } => {
            ViolationReason::PrecisionExceeded {
                expected: Expected::Decimal { precision, scale },
                actual,
            }
        }
        ColumnViolation::CustomRejected { id, reason } => ViolationReason::CustomRejected {
            expected: Expected::AcceptedBy(id),
            actual,
            reason,
        },
        ColumnViolation::CustomFailed { id, reason } => ViolationReason::CustomFailed {
            expected: Expected::AcceptedBy(id),
            actual,
            reason,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile_declaration;
    use crate::declaration::{ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl};
    use crate::registry::{
        CustomType, CustomTypeFailure, CustomTypeId, CustomVerdict, TypeRegistry,
    };
    use crate::types::decimal::DecimalDigits;
    use crate::types::TypeKind;
    use crate::validate::report::{SheetReport, ValidationOptions, ValuePathSegment};
    use document_format::{AttachmentId, SheetId};
    use std::str::FromStr;
    use std::sync::Arc;

    /// 標本のシート識別子（ULID の正準テキスト形）。
    const SHEET: &str = "01K4ANRRG004HMASW9NF6YY091";
    /// 標本の行識別子（ULID の正準テキスト形）。
    const ROW: &str = "01K4ANRRG004HMASW9NF6YY101";
    /// 2 行目の標本の行識別子。
    const ROW_2: &str = "01K4ANRRG004HMASW9NF6YY102";
    /// 標本の拡張型の識別子。
    const POSTAL: &str = "postal-code";
    /// 失敗を返す標本の拡張型の識別子。
    const FLAKY: &str = "flaky";

    /// 標本のシート識別子。
    fn sheet_id() -> SheetId {
        SheetId::from_str(SHEET).expect("標本のシート識別子が解析できない")
    }

    /// 標本の行識別子。
    fn row_id(text: &str) -> RowId {
        RowId::from_str(text).expect("標本の行識別子が解析できない")
    }

    /// 標本の添付識別子（BLAKE3 の内容アドレス）。
    fn attachment_id() -> AttachmentId {
        AttachmentId::from_bytes(b"jxcel schema-engine cell test attachment")
    }

    /// 種別による指定の型を組み立てる。
    fn declared(kind: TypeKind, constraints: Constraints) -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(kind),
            constraints,
        }
    }

    /// 既定値も説明も持たない列を組み立てる。
    fn column(name: &str, ty: TypeDecl) -> ColumnDecl {
        ColumnDecl {
            name: name.into(),
            ty,
            required: false,
            unique: false,
            default: None,
            description: None,
        }
    }

    /// 値なしを許さない列を組み立てる。
    fn required_column(name: &str, ty: TypeDecl) -> ColumnDecl {
        ColumnDecl {
            required: true,
            ..column(name, ty)
        }
    }

    /// 入れ子のフィールドを組み立てる。
    fn field(name: &str, ty: TypeDecl, required: bool) -> FieldDecl {
        FieldDecl {
            name: name.into(),
            ty,
            required,
            default: None,
            description: None,
        }
    }

    /// 種別による指定のルートスキーマをコンパイルする。
    fn compiled_with(columns: Vec<ColumnDecl>, registry: &TypeRegistry) -> CompiledSchema {
        compile_declaration(&Schema { columns }, &[], registry)
            .expect("標本の宣言はコンパイルできる")
    }

    /// 拡張型の登録の無いスキーマをコンパイルする。
    fn compiled(columns: Vec<ColumnDecl>) -> CompiledSchema {
        compiled_with(columns, &TypeRegistry::new())
    }

    /// 1 行を検証して結果を確定する（標本の行識別子を使う）。
    fn validate(schema: &CompiledSchema, values: &[CellValue]) -> SheetReport {
        validate_at(schema, row_id(ROW), values)
    }

    /// 指定した行識別子で 1 行を検証して結果を確定する。
    fn validate_at(schema: &CompiledSchema, row: RowId, values: &[CellValue]) -> SheetReport {
        let mut report = ViolationReport::new(&ValidationOptions::unlimited());
        validate_row(schema, Some(row), values, &mut report);
        report.finish(sheet_id())
    }

    /// 値なしの上限に依らず 1 件ずつ検証する（無制限）。
    fn text(value: &str) -> CellValue {
        CellValue::Text(value.to_owned())
    }

    /// 入れ子のオブジェクトの値を組み立てる。
    fn object(entries: Vec<(&str, CellValue)>) -> CellValue {
        CellValue::Nested(NestedValue::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        ))
    }

    /// 入れ子の配列の値を組み立てる。
    fn array(items: Vec<CellValue>) -> CellValue {
        CellValue::Nested(NestedValue::Array(items))
    }

    /// `〒` で始まるテキストだけを受理する標本の拡張型（要件 11.3）。
    struct PostalCode {
        id: CustomTypeId,
    }

    impl CustomType for PostalCode {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            match value {
                CellValue::Text(text) if text.starts_with('〒') => Ok(CustomVerdict::Accepted),
                _ => Ok(CustomVerdict::rejected("〒 で始まらない")),
            }
        }
    }

    /// `fail` のときだけ失敗を返す標本の拡張型（要件 11.5）。
    struct Flaky {
        id: CustomTypeId,
    }

    impl CustomType for Flaky {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            match value {
                CellValue::Text(text) if text.as_str() == "fail" => {
                    Err(CustomTypeFailure::new("応答がない"))
                }
                _ => Ok(CustomVerdict::Accepted),
            }
        }
    }

    /// 標本の拡張型を 1 つ登録した辞書を組み立てる。
    fn registry_with(id: &str, implementation: impl CustomType + 'static) -> TypeRegistry {
        debug_assert_eq!(id, implementation.id().as_str());
        let mut registry = TypeRegistry::new();
        registry
            .register(Arc::new(implementation))
            .expect("標本の拡張型は登録できる");
        registry
    }

    /// 必須の列に値なしが与えられたとき、その行と列を指す違反として報告する（要件 4.4）。
    #[test]
    fn a_required_column_without_a_value_is_a_violation() {
        let schema = compiled(vec![
            required_column("数量", declared(TypeKind::Int, Constraints::default())),
            column("備考", declared(TypeKind::Any, Constraints::default())),
        ]);
        let report = validate(&schema, &[CellValue::Null, CellValue::Null]);

        assert_eq!(
            1,
            report.total_violations(),
            "必須の列の値なしだけが違反になり、任意の列の値なしは違反にならない"
        );
        let violation = &report.violations()[0];
        assert_eq!(Some(row_id(ROW)), violation.row());
        assert_eq!(0, violation.column().index());
        assert_eq!("数量", violation.column_name());
        assert!(violation.path().is_root(), "セル直下の違反の位置が空でない");
        assert_eq!(
            ViolationReason::MissingValue {
                expected: Expected::Present,
                actual: CellValue::Null,
            },
            *violation.reason()
        );
        assert_eq!(vec![row_id(ROW)], report.invalid_rows());
    }

    /// 型の不一致・範囲外・長さ超過・書式不一致・桁超過を、期待した内容と実際の値つきで
    /// 報告し、1 つの行の複数の違反を最初の違反で打ち切らない（要件 5.3）。
    #[test]
    fn every_violation_in_a_row_is_reported_with_expected_and_actual() {
        let schema = compiled(vec![
            column(
                "数量",
                declared(
                    TypeKind::Int,
                    Constraints {
                        min: Some(CellValue::Int(0)),
                        ..Constraints::default()
                    },
                ),
            ),
            column(
                "単価",
                declared(
                    TypeKind::Decimal,
                    Constraints {
                        digits: Some(DecimalDigits::new(4, 2).expect("4 >= 1 かつ 2 <= 4")),
                        ..Constraints::default()
                    },
                ),
            ),
            column(
                "品番",
                declared(
                    TypeKind::Text,
                    Constraints {
                        max_length: Some(3),
                        ..Constraints::default()
                    },
                ),
            ),
            column(
                "コード",
                declared(
                    TypeKind::Text,
                    Constraints {
                        pattern: Some("^[0-9]{3}$".into()),
                        ..Constraints::default()
                    },
                ),
            ),
            column("有効", declared(TypeKind::Bool, Constraints::default())),
        ]);
        let report = validate(
            &schema,
            &[
                CellValue::Int(-1),
                CellValue::Decimal("1.234".into()),
                text("abcd"),
                text("12a"),
                text("yes"),
            ],
        );

        assert_eq!(5, report.total_violations(), "最初の違反で打ち切られている");
        assert_eq!(
            ViolationReason::OutOfRange {
                expected: Expected::Range {
                    min: Some(CellValue::Int(0)),
                    max: None,
                },
                actual: CellValue::Int(-1),
            },
            *report.violations()[0].reason()
        );
        assert_eq!(
            ViolationReason::PrecisionExceeded {
                expected: Expected::Decimal {
                    precision: 4,
                    scale: 2,
                },
                actual: CellValue::Decimal("1.234".into()),
            },
            *report.violations()[1].reason()
        );
        assert_eq!(
            ViolationReason::LengthOutOfRange {
                expected: Expected::Length {
                    min: None,
                    max: Some(3),
                },
                actual: text("abcd"),
            },
            *report.violations()[2].reason()
        );
        assert_eq!(
            ViolationReason::PatternMismatch {
                expected: Expected::Pattern("^[0-9]{3}$".into()),
                actual: text("12a"),
            },
            *report.violations()[3].reason()
        );
        assert_eq!(
            ViolationReason::TypeMismatch {
                expected: Expected::Kind("bool".into()),
                actual: text("yes"),
            },
            *report.violations()[4].reason()
        );
        let columns: Vec<usize> = report
            .violations()
            .iter()
            .map(|violation| violation.column().index())
            .collect();
        assert_eq!(vec![0, 1, 2, 3, 4], columns, "違反が列の順に出ていない");
        assert!(report
            .violations()
            .iter()
            .all(|violation| violation.path().is_root()));
    }

    /// 入れ子の内側の違反を、フィールド名と添字の並びで特定できる位置に報告する
    /// （要件 3.4）。入れ子のフィールドの必須の欠落も違反になる（要件 4.4）。
    #[test]
    fn nested_violations_point_into_the_value_by_field_name_and_index() {
        let line = declared(
            TypeKind::Object,
            Constraints {
                fields: vec![field(
                    "品番",
                    declared(
                        TypeKind::Text,
                        Constraints {
                            max_length: Some(2),
                            ..Constraints::default()
                        },
                    ),
                    true,
                )],
                ..Constraints::default()
            },
        );
        let schema = compiled(vec![column(
            "明細",
            declared(
                TypeKind::Object,
                Constraints {
                    fields: vec![
                        field(
                            "数量",
                            declared(TypeKind::Int, Constraints::default()),
                            true,
                        ),
                        field(
                            "明細行",
                            declared(
                                TypeKind::Array,
                                Constraints {
                                    items: Some(Box::new(line)),
                                    ..Constraints::default()
                                },
                            ),
                            false,
                        ),
                    ],
                    ..Constraints::default()
                },
            ),
        )]);
        let report = validate(
            &schema,
            &[object(vec![
                ("数量", text("x")),
                (
                    "明細行",
                    array(vec![object(vec![("品番", text("abcd"))]), object(vec![])]),
                ),
            ])],
        );

        assert_eq!(
            3,
            report.total_violations(),
            "入れ子の内側の違反が打ち切られている"
        );
        let paths: Vec<Vec<ValuePathSegment>> = report
            .violations()
            .iter()
            .map(|violation| violation.path().segments().to_vec())
            .collect();
        assert_eq!(
            vec![
                vec![ValuePathSegment::Field("数量".into())],
                vec![
                    ValuePathSegment::Field("明細行".into()),
                    ValuePathSegment::Index(0),
                    ValuePathSegment::Field("品番".into()),
                ],
                vec![
                    ValuePathSegment::Field("明細行".into()),
                    ValuePathSegment::Index(1),
                    ValuePathSegment::Field("品番".into()),
                ],
            ],
            paths,
            "入れ子の内側の位置がフィールド名と添字の並びで特定できていない"
        );
        assert_eq!(
            ViolationReason::TypeMismatch {
                expected: Expected::Kind("int".into()),
                actual: text("x"),
            },
            *report.violations()[0].reason()
        );
        assert_eq!(
            ViolationReason::LengthOutOfRange {
                expected: Expected::Length {
                    min: None,
                    max: Some(2),
                },
                actual: text("abcd"),
            },
            *report.violations()[1].reason()
        );
        assert_eq!(
            ViolationReason::MissingValue {
                expected: Expected::Present,
                actual: CellValue::Null,
            },
            *report.violations()[2].reason()
        );
        assert!(
            report
                .violations()
                .iter()
                .all(|violation| violation.row() == Some(row_id(ROW))
                    && violation.column().index() == 0),
            "入れ子の違反が列そのものの位置を失っている"
        );
    }

    /// ANY の列は保持可能なすべての変種を適合として扱い、値の中身を解釈しない
    /// （要件 2.5, 2.6）。
    #[test]
    fn an_any_column_accepts_every_variant_without_interpretation() {
        let schema = compiled(vec![column(
            "自由",
            declared(TypeKind::Any, Constraints::default()),
        )]);
        let values = [
            CellValue::Null,
            CellValue::Bool(true),
            CellValue::Int(1),
            CellValue::Float(1.5),
            CellValue::Decimal("文法外の 10 進数".into()),
            text("任意のテキスト"),
            object(vec![("未知", text("x"))]),
            array(vec![text("x")]),
            CellValue::Attachment(attachment_id()),
        ];

        for value in values {
            let report = validate(&schema, std::slice::from_ref(&value));
            assert_eq!(
                0,
                report.total_violations(),
                "ANY の列が値を拒否している: {value:?}"
            );
        }
    }

    /// 使用不能な列はその列のすべての値を違反にするが、他の列の判定は正しく続く
    /// （要件 11.7）。
    #[test]
    fn an_unusable_column_reports_every_value_but_other_columns_still_validate() {
        let schema = compiled_with(
            vec![
                ColumnDecl {
                    name: "将来の型".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Unknown("geo-point".into()),
                        constraints: Constraints::default(),
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
                column(
                    "数量",
                    declared(
                        TypeKind::Int,
                        Constraints {
                            min: Some(CellValue::Int(0)),
                            ..Constraints::default()
                        },
                    ),
                ),
                column(
                    "郵便番号",
                    declared(
                        TypeKind::Custom,
                        Constraints {
                            custom_type: Some("未登録".into()),
                            ..Constraints::default()
                        },
                    ),
                ),
                column("備考", declared(TypeKind::Text, Constraints::default())),
            ],
            &TypeRegistry::new(),
        );

        let row_a = row_id(ROW);
        let row_b = row_id(ROW_2);
        let mut report = ViolationReport::new(&ValidationOptions::unlimited());
        validate_row(
            &schema,
            Some(row_a),
            &[text("a"), CellValue::Int(-1), text("123"), text("ok")],
            &mut report,
        );
        validate_row(
            &schema,
            Some(row_b),
            &[
                CellValue::Null,
                CellValue::Int(5),
                CellValue::Null,
                text("fine"),
            ],
            &mut report,
        );
        let report = report.finish(sheet_id());

        let found: Vec<(Option<RowId>, usize, &ViolationReason)> = report
            .violations()
            .iter()
            .map(|violation| {
                (
                    violation.row(),
                    violation.column().index(),
                    violation.reason(),
                )
            })
            .collect();
        assert_eq!(
            vec![
                (
                    Some(row_a),
                    0,
                    &ViolationReason::UnusableColumn {
                        expected: Expected::Usable,
                        actual: text("a"),
                        kind: "geo-point".into(),
                    },
                ),
                (
                    Some(row_a),
                    1,
                    &ViolationReason::OutOfRange {
                        expected: Expected::Range {
                            min: Some(CellValue::Int(0)),
                            max: None,
                        },
                        actual: CellValue::Int(-1),
                    },
                ),
                (
                    Some(row_a),
                    2,
                    &ViolationReason::UnusableColumn {
                        expected: Expected::Usable,
                        actual: text("123"),
                        kind: "未登録".into(),
                    },
                ),
                (
                    Some(row_b),
                    0,
                    &ViolationReason::UnusableColumn {
                        expected: Expected::Usable,
                        actual: CellValue::Null,
                        kind: "geo-point".into(),
                    },
                ),
                (
                    Some(row_b),
                    2,
                    &ViolationReason::UnusableColumn {
                        expected: Expected::Usable,
                        actual: CellValue::Null,
                        kind: "未登録".into(),
                    },
                ),
            ],
            found,
            "使用不能な列の値がすべて違反になり、他の列が正しく検証されていない"
        );
        assert_eq!(vec![row_a, row_b], report.invalid_rows());
    }

    /// 拡張型の違反が組込型と同一の形式で報告される（要件 11.3）。
    #[test]
    fn a_custom_type_violation_is_reported_in_the_same_form_as_a_built_in() {
        let registry = registry_with(
            POSTAL,
            PostalCode {
                id: CustomTypeId::new(POSTAL),
            },
        );
        let schema = compiled_with(
            vec![
                column(
                    "郵便番号",
                    declared(
                        TypeKind::Custom,
                        Constraints {
                            custom_type: Some(POSTAL.into()),
                            ..Constraints::default()
                        },
                    ),
                ),
                column("数量", declared(TypeKind::Int, Constraints::default())),
            ],
            &registry,
        );

        let report = validate(&schema, &[text("123-4567"), text("x")]);
        assert_eq!(2, report.total_violations());
        assert_eq!(
            ViolationReason::CustomRejected {
                expected: Expected::AcceptedBy(POSTAL.into()),
                actual: text("123-4567"),
                reason: "〒 で始まらない".into(),
            },
            *report.violations()[0].reason()
        );
        assert_eq!(
            ViolationReason::TypeMismatch {
                expected: Expected::Kind("int".into()),
                actual: text("x"),
            },
            *report.violations()[1].reason()
        );
        for violation in report.violations() {
            assert_eq!(Some(row_id(ROW)), violation.row());
            assert!(
                violation.path().is_root(),
                "組込型と同じ位置の形で報告されていない"
            );
        }
        assert_eq!("郵便番号", report.violations()[0].column_name());
        assert_eq!("数量", report.violations()[1].column_name());

        // 適合する値は違反にならない。
        let accepted = validate(&schema, &[text("〒100-0001"), CellValue::Int(1)]);
        assert_eq!(0, accepted.total_violations());
    }

    /// 拡張型の判定の失敗をその値の違反に閉じ込め、走査を次の値へ進める（要件 11.5）。
    #[test]
    fn a_custom_type_failure_is_confined_to_its_value_and_the_scan_continues() {
        let registry = registry_with(
            FLAKY,
            Flaky {
                id: CustomTypeId::new(FLAKY),
            },
        );
        let schema = compiled_with(
            vec![
                column(
                    "郵便番号",
                    declared(
                        TypeKind::Custom,
                        Constraints {
                            custom_type: Some(FLAKY.into()),
                            ..Constraints::default()
                        },
                    ),
                ),
                column(
                    "数量",
                    declared(
                        TypeKind::Int,
                        Constraints {
                            min: Some(CellValue::Int(0)),
                            ..Constraints::default()
                        },
                    ),
                ),
                column("備考", declared(TypeKind::Text, Constraints::default())),
            ],
            &registry,
        );

        let row_a = row_id(ROW);
        let row_b = row_id(ROW_2);
        let mut report = ViolationReport::new(&ValidationOptions::unlimited());
        validate_row(
            &schema,
            Some(row_a),
            &[text("fail"), CellValue::Int(-1), text("ok")],
            &mut report,
        );
        validate_row(
            &schema,
            Some(row_b),
            &[text("fine"), CellValue::Int(5), text("ok")],
            &mut report,
        );
        let report = report.finish(sheet_id());

        assert_eq!(
            2,
            report.total_violations(),
            "失敗した値の違反で走査が打ち切られている"
        );
        assert_eq!(
            ViolationReason::CustomFailed {
                expected: Expected::AcceptedBy(FLAKY.into()),
                actual: text("fail"),
                reason: "応答がない".into(),
            },
            *report.violations()[0].reason()
        );
        assert_eq!(
            ViolationReason::OutOfRange {
                expected: Expected::Range {
                    min: Some(CellValue::Int(0)),
                    max: None,
                },
                actual: CellValue::Int(-1),
            },
            *report.violations()[1].reason(),
            "失敗の次の列の判定が続いていない"
        );
        assert_eq!(vec![row_a], report.invalid_rows());
    }
}

//! 書き込み経路ごとの受け入れ方針（design.md「コンポーネントとファイルの対応」の
//! `WritePolicy`。tasks.md 6.2。要件 6.1, 6.2, 6.3, 6.4, 10.2）。
//!
//! 1 行分の書き込みを**強制してから検証し**、経路ごとの判定へ落とす唯一の場所である。
//! 強制と検証の順序は両経路で同じであり、違うのは違反があったときの扱いだけである
//! （design.md「System Flows / 書き込み経路の判定」）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本層は `compile` 層と、その右隣の [`crate::coerce`] /
//! [`crate::validate`] を参照する。本層を参照するのは `evolution` 層と `api` 層だけである。
//! **拒否の実行はしない** — 送信を止めるのは呼び出し元（`form-web-server`）の仕事である
//! （design.md「Write Layer / WritePolicy」）。
//!
//! # 経路の分類は本層が所有する（要件 6.3）
//!
//! 画面上の入力とマクロからの書き込みが**編集経路**（[`WriteOrigin::Edit`]）、フォームからの
//! 送信が**収集経路**（[`WriteOrigin::Collect`]）である。どの呼び出し元がどちらを渡すかは
//! 呼び出し元が決めるが、**その 2 つ以外の経路を認めない**ことが本層の所有物である
//! （design.md の `Revalidation Triggers` が「書き込み経路の分類」を下流の追随対象に
//! 挙げているのはこのためである）。
//!
//! # 編集経路は保持を妨げない（要件 6.1）
//!
//! 違反があっても値を捨てない。判定（[`EditVerdict::AcceptedWithViolations`]）が**強制後の
//! 値と違反の双方**を返すため、呼び出し元は打ち込まれた値をそのまま保持しつつ、いつでも
//! 違反を取り出せる。違反する値のための特別な格納先は作らない — セル値は任意の入力を表現し、
//! 上流の `document-format` が逐語で往復させる（design.md「Write Layer」の「違反する値を
//! どこに置くか」）。
//!
//! # 収集経路は受け入れられないと判定するだけである（要件 6.2）
//!
//! 違反があれば値を返さず [`CollectVerdict::Rejected`] を返す。**送信を止める処理は書かない**
//! （design.md「Out of Boundary」— 判定に基づく実行は配信側が行う）。
//!
//! # 違反の位置と理由は判定とともに返る（要件 6.4）
//!
//! 判定が運ぶ [`Violation`] は行の識別子・列の添字・列名・入れ子の内側の位置
//! （[`ValuePath`]）・理由（期待した内容と実際の値）を持つ。違反は**保持されず、常に宣言と
//! 値から導出される** — だからこそ「いつでも取得できる」経路がこの 1 本で足りる
//! （design.md「Write Layer」）。
//!
//! # 行を跨ぐ性質は本層では判定しない（design.md「Validate Layer / SheetValidator」）
//!
//! 一意性（要件 4.7）と参照の実在（要件 9.2）は 1 行だけを見ても決まらない。本層は
//! **値それ自体に閉じた性質**（型・範囲・長さ・書式・桁・必須・入れ子）だけを判定し、
//! 行を跨ぐ性質は一括経路（[`crate::validate::validate_sheet`] /
//! [`crate::validate::validate_columns`]）が持つ。この線引きが要件 10.2 の 16 ミリ秒を
//! 成立させる（書き込みのたびに 10 万行分の索引を持ち回らない）。
//!
//! # 経路ごとの判定は型で分かれる（tasks.md 6.2 の「型の上での保証」）
//!
//! [`EditVerdict`] は `Rejected` を、[`CollectVerdict`] は `AcceptedWithViolations` を
//! **変種として持たない**。したがって「編集経路が拒否を返す」「収集経路が違反つきの受理を
//! 返す」という状態は構成できず、design.md「Write Layer」の Postconditions が型の上で
//! 保証される。[`WriteVerdict`] はその 2 つの和であり、design.md の 3 変種
//! （`Accepted` / `AcceptedWithViolations` / `Rejected`）を経路ごとの型として保つ。
//!
//! # 判定の材料を二重に持たない
//!
//! 強制は [`crate::coerce::coerce`]（タスク 6.1 の規則表）、値の判定は
//! [`crate::validate::cell::validate_row`]（タスク 5.1）へ委ね、本層は**順序と経路ごとの
//! 分岐だけ**を持つ。違反の理由の写像（`compile` 層の語彙 → `validate` 層の語彙）は
//! `validate` 層の内側に 1 箇所だけある。

use document_format::CellValue;

use crate::coerce::{coerce, Coercion};
use crate::compile::{ColumnIndex, CompiledSchema};
use crate::validate::cell;
use crate::validate::report::{ValidationOptions, Violation, ViolationReport};

/// 書き込みの経路（design.md「Write Layer / WritePolicy」の Service Interface。要件 6.3）。
///
/// 画面上の入力とマクロからの書き込みが [`WriteOrigin::Edit`]、フォームからの送信が
/// [`WriteOrigin::Collect`] である。**経路の分類は本層が所有する** — 呼び出し元は経路を
/// 選ぶだけで、経路ごとの受け入れ方針（[`EditVerdict`] / [`CollectVerdict`]）を知らない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOrigin {
    /// 画面上の入力とマクロからの書き込み。
    Edit,
    /// フォームからの送信。
    Collect,
}

/// 編集経路の判定（design.md「Write Layer / WritePolicy」の `WriteVerdict` のうち
/// `Accepted` と `AcceptedWithViolations`。要件 6.1）。
///
/// **`Rejected` を持たない** — 編集経路は違反があっても保持を妨げないためである
/// （design.md の Postconditions）。
#[derive(Debug, Clone, PartialEq)]
pub enum EditVerdict {
    /// 全値が適合した。`values` には強制後の値が入る。
    Accepted {
        /// 強制後の値。長さは入力と同じである。
        values: Vec<CellValue>,
        /// 値ごとの強制の記録（要件 7.4）。
        coercions: Vec<Coercion>,
    },
    /// 違反はあるが、呼び出し元が保持できる値を返す（要件 6.1）。
    AcceptedWithViolations {
        /// 強制後の値。違反する値も**入力のまま**（または強制の結果のまま）残る。
        values: Vec<CellValue>,
        /// 値ごとの強制の記録（要件 7.4）。
        coercions: Vec<Coercion>,
        /// 違反の一覧（位置と理由。要件 6.4）。
        violations: Vec<Violation>,
    },
}

/// 収集経路の判定（design.md「Write Layer / WritePolicy」の `WriteVerdict` のうち
/// `Accepted` と `Rejected`。要件 6.2）。
///
/// **`AcceptedWithViolations` を持たない** — 収集経路は違反する値を受け入れられないものとして
/// 判定するためである（design.md の Postconditions）。
#[derive(Debug, Clone, PartialEq)]
pub enum CollectVerdict {
    /// 全値が適合した。`values` には強制後の値が入る。
    Accepted {
        /// 強制後の値。長さは入力と同じである。
        values: Vec<CellValue>,
        /// 値ごとの強制の記録（要件 7.4）。
        coercions: Vec<Coercion>,
    },
    /// 受け入れられない（要件 6.2）。値を返さない — 呼び出し元はこの判定に基づいて
    /// 配信を止める（止めるのは呼び出し元の仕事である）。
    Rejected {
        /// 違反の一覧（位置と理由。要件 6.4）。
        violations: Vec<Violation>,
    },
}

/// 経路ごとの判定（design.md「Write Layer / WritePolicy」の Service Interface）。
///
/// どちらの経路の判定かが型で分かれるため、`Edit` の判定は決して拒否を、`Collect` の判定は
/// 決して違反つきの受理を含まない（モジュール docs「経路ごとの判定は型で分かれる」）。
#[derive(Debug, Clone, PartialEq)]
pub enum WriteVerdict {
    /// 編集経路の判定（要件 6.1）。
    Edit(EditVerdict),
    /// 収集経路の判定（要件 6.2）。
    Collect(CollectVerdict),
}

/// 1 行分の書き込みを強制し、検証し、経路ごとの判定へ落とす
/// （design.md「System Flows / 書き込み経路の判定」。要件 6.1, 6.2, 6.3, 6.4, 10.2）。
///
/// 強制（[`coerce`]）と検証（[`cell::validate_row`]）の順序は両経路で同じであり、分岐する
/// のは違反があったときの扱いだけである。`values` は [`CompiledSchema::columns`] と同じ並びの
/// 1 行分の値である（design.md「Data Models / Domain Model」の不変条件）。計画より短い並びは
/// 残りを値なしとして扱い、計画の列数を超える値は計画の外として判定しない（
/// [`cell::validate_row`] と同じ規則）。
///
/// 文書を変更しない。**行を跨ぐ性質（一意性と参照の実在）は判定しない**（モジュール docs）。
pub fn validate_write(
    origin: WriteOrigin,
    schema: &CompiledSchema,
    values: Vec<CellValue>,
) -> WriteVerdict {
    let (values, coercions) = coerce_values(schema, values);
    let violations = value_violations(schema, &values);
    match origin {
        WriteOrigin::Edit => WriteVerdict::Edit(judge_edit(values, coercions, violations)),
        WriteOrigin::Collect => WriteVerdict::Collect(judge_collect(values, coercions, violations)),
    }
}

/// 値ごとに列の型へ強制する（design.md「System Flows / 書き込み経路の判定」の強制の段。
/// 要件 7.1〜7.6）。
///
/// `values` の添字がそのまま列の添字である（design.md「Data Models / Domain Model」の
/// 不変条件 — `Row::values()` もこの添字で並ぶ）。計画の外の添字（列数を超える値）は列の
/// 型が決まらないため変換せず、そのまま返す（[`coerce`] と同じ規則）。**返す値の長さは
/// 入力と同じである**（design.md「Write Layer / WritePolicy」の Invariants）。
fn coerce_values(
    schema: &CompiledSchema,
    values: Vec<CellValue>,
) -> (Vec<CellValue>, Vec<Coercion>) {
    let mut coerced = Vec::with_capacity(values.len());
    let mut coercions = Vec::with_capacity(values.len());
    for (index, value) in values.into_iter().enumerate() {
        let result = coerce(schema, ColumnIndex::new(index), value);
        coerced.push(result.value);
        coercions.push(result.coercion);
    }
    (coerced, coercions)
}

/// 強制後の値を検証し、違反だけを返す（design.md 同節の検証の段。要件 6.4）。
///
/// **行に属さない値**（`row: None`）として 1 行分を判定する — 書き込みの値はまだ行の
/// 識別子を持たない。判定そのものは [`cell::validate_row`]（タスク 5.1）へ委ねる。
/// 違反の理由の写像（`compile` 層の語彙 → `validate` 層の語彙）は `validate` 層の内側に
/// 1 箇所だけあり、本層は写し取らない。行を跨ぐ性質（一意性と参照の実在）は判定しない
/// （モジュール docs）。
fn value_violations(schema: &CompiledSchema, values: &[CellValue]) -> Vec<Violation> {
    let mut report = ViolationReport::new(&ValidationOptions::unlimited());
    cell::validate_row(schema, None, values, &mut report);
    report.into_violations()
}

/// 編集経路の判定（要件 6.1）。
///
/// 違反があっても値を捨てず、違反とともに返す（[`EditVerdict::AcceptedWithViolations`]）。
fn judge_edit(
    values: Vec<CellValue>,
    coercions: Vec<Coercion>,
    violations: Vec<Violation>,
) -> EditVerdict {
    if violations.is_empty() {
        EditVerdict::Accepted { values, coercions }
    } else {
        EditVerdict::AcceptedWithViolations {
            values,
            coercions,
            violations,
        }
    }
}

/// 収集経路の判定（要件 6.2）。
///
/// 違反があれば値ではなく拒否を返す（[`CollectVerdict::Rejected`]）。**送信を止める処理は
/// 行わない** — 判定に基づく実行は呼び出し元の仕事である（design.md「Out of Boundary」）。
fn judge_collect(
    values: Vec<CellValue>,
    coercions: Vec<Coercion>,
    violations: Vec<Violation>,
) -> CollectVerdict {
    if violations.is_empty() {
        CollectVerdict::Accepted { values, coercions }
    } else {
        CollectVerdict::Rejected { violations }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;
    use std::time::{Duration, Instant};

    use document_format::{NestedValue, SheetId};

    use crate::compile::compile_declaration;
    use crate::declaration::{ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl};
    use crate::registry::TypeRegistry;
    use crate::types::TypeKind;
    use crate::validate::report::{ValuePathSegment, ViolationReason};

    /// 標本のシート識別子（ULID の正準テキスト形）。
    const SHEET: &str = "01K4ANRRG004HMASW9NF6YY091";

    /// テキストの値を組み立てる。
    fn text(value: &str) -> CellValue {
        CellValue::Text(value.to_owned())
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

    /// 列の集合をコンパイルする。
    fn compiled(columns: Vec<ColumnDecl>) -> CompiledSchema {
        compiled_with(columns, &TypeRegistry::new())
    }

    /// 列の集合を辞書つきでコンパイルする。
    fn compiled_with(columns: Vec<ColumnDecl>, registry: &TypeRegistry) -> CompiledSchema {
        compile_declaration(&Schema { columns }, &[], registry)
            .expect("標本の宣言はコンパイルできる")
    }

    /// 標本のシート識別子。
    fn sheet_id() -> SheetId {
        SheetId::from_str(SHEET).expect("標本のシート識別子が解析できない")
    }

    /// 編集経路の判定から、受理された値と強制の記録を取り出す。
    fn edit_accepted(verdict: WriteVerdict) -> (Vec<CellValue>, Vec<Coercion>) {
        match verdict {
            WriteVerdict::Edit(EditVerdict::Accepted { values, coercions }) => (values, coercions),
            other => panic!("編集経路が受理の判定を返さなかった: {other:?}"),
        }
    }

    /// 編集経路の判定から、保持できる値・強制の記録・違反を取り出す。
    fn edit_violated(verdict: WriteVerdict) -> (Vec<CellValue>, Vec<Coercion>, Vec<Violation>) {
        match verdict {
            WriteVerdict::Edit(EditVerdict::AcceptedWithViolations {
                values,
                coercions,
                violations,
            }) => (values, coercions, violations),
            other => panic!("編集経路が違反つきの受理を返さなかった: {other:?}"),
        }
    }

    /// 収集経路の判定から、受理された値と強制の記録を取り出す。
    fn collect_accepted(verdict: WriteVerdict) -> (Vec<CellValue>, Vec<Coercion>) {
        match verdict {
            WriteVerdict::Collect(CollectVerdict::Accepted { values, coercions }) => {
                (values, coercions)
            }
            other => panic!("収集経路が受理の判定を返さなかった: {other:?}"),
        }
    }

    /// 収集経路の判定から違反を取り出す。
    fn collect_rejected(verdict: WriteVerdict) -> Vec<Violation> {
        match verdict {
            WriteVerdict::Collect(CollectVerdict::Rejected { violations }) => violations,
            other => panic!("収集経路が拒否の判定を返さなかった: {other:?}"),
        }
    }

    /// 編集経路の判定の全変種（`Rejected` を持たない。要件 6.1）。
    ///
    /// 網羅マッチであるため、`Rejected` を足すとここがコンパイルできなくなる —
    /// 型の上での保証をこの関数が体現する。
    fn edit_form(verdict: &EditVerdict) -> &'static str {
        match verdict {
            EditVerdict::Accepted { .. } => "accepted",
            EditVerdict::AcceptedWithViolations { .. } => "accepted with violations",
        }
    }

    /// 収集経路の判定の全変種（`AcceptedWithViolations` を持たない。要件 6.2）。
    fn collect_form(verdict: &CollectVerdict) -> &'static str {
        match verdict {
            CollectVerdict::Accepted { .. } => "accepted",
            CollectVerdict::Rejected { .. } => "rejected",
        }
    }

    // 経路の分類（要件 6.3）。

    /// 経路の分類が編集と収集の 2 つだけである（要件 6.3）。
    #[test]
    fn the_classification_has_exactly_the_edit_and_the_collect_paths() {
        assert_ne!(WriteOrigin::Edit, WriteOrigin::Collect);
        assert_eq!(WriteOrigin::Edit, WriteOrigin::Edit);
        assert_eq!(WriteOrigin::Collect, WriteOrigin::Collect);
    }

    // 適合した書き込み（要件 6.1, 6.2）。

    /// 適合した値はどちらの経路でも受理され、強制の記録も返る。
    #[test]
    fn both_paths_accept_a_conforming_write() {
        let plan = compiled(vec![column(
            "数量",
            declared(TypeKind::Int, Constraints::default()),
        )]);

        for origin in [WriteOrigin::Edit, WriteOrigin::Collect] {
            let verdict = validate_write(origin, &plan, vec![CellValue::Int(42)]);
            let (values, coercions) = match verdict {
                WriteVerdict::Edit(EditVerdict::Accepted { values, coercions })
                | WriteVerdict::Collect(CollectVerdict::Accepted { values, coercions }) => {
                    (values, coercions)
                }
                other => panic!("適合した書き込みが受理されなかった: {other:?}"),
            };
            assert_eq!(vec![CellValue::Int(42)], values);
            assert_eq!(vec![Coercion::Unchanged], coercions);
        }
    }

    // 編集経路（要件 6.1, 6.4）。

    /// 編集経路は違反を報告したうえで、呼び出し元が保持できる値を返す。
    #[test]
    fn the_edit_path_keeps_a_violating_value_and_reports_it() {
        let plan = compiled(vec![column(
            "数量",
            declared(TypeKind::Int, Constraints::default()),
        )]);

        let (values, coercions, violations) =
            edit_violated(validate_write(WriteOrigin::Edit, &plan, vec![text("abc")]));

        assert_eq!(
            vec![text("abc")],
            values,
            "編集経路が違反する値を捨てた（要件 6.1）"
        );
        assert_eq!(vec![Coercion::Unchanged], coercions);
        assert_eq!(1, violations.len(), "違反が報告されていない");

        let violation = &violations[0];
        assert_eq!(None, violation.row(), "書き込みの値はまだ行に属さない");
        assert_eq!(0, violation.column().index());
        assert_eq!("数量", violation.column_name());
        assert!(violation.path().is_root(), "セル直下の位置でない");
        assert_eq!(&text("abc"), violation.reason().actual());
        assert!(
            matches!(violation.reason(), ViolationReason::TypeMismatch { .. }),
            "整数の列のテキストが型の不一致にならない"
        );
    }

    /// 編集経路の判定は拒否の変種を持たない（型の上での保証。要件 6.1）。
    #[test]
    fn the_edit_path_never_rejects() {
        let plan = compiled(vec![column(
            "数量",
            declared(TypeKind::Int, Constraints::default()),
        )]);
        let verdict = validate_write(WriteOrigin::Edit, &plan, vec![text("abc")]);
        match &verdict {
            WriteVerdict::Edit(edit) => assert_eq!("accepted with violations", edit_form(edit)),
            other => panic!("編集経路が編集の判定を返さなかった: {other:?}"),
        }
    }

    // 収集経路（要件 6.2, 6.4）。

    /// 収集経路は違反する値を受け入れられないものとして判定する。
    #[test]
    fn the_collect_path_rejects_a_violating_value() {
        let plan = compiled(vec![column(
            "数量",
            declared(TypeKind::Int, Constraints::default()),
        )]);

        let violations = collect_rejected(validate_write(
            WriteOrigin::Collect,
            &plan,
            vec![text("abc")],
        ));

        assert_eq!(1, violations.len(), "拒否の理由が返っていない");
        assert_eq!(&text("abc"), violations[0].reason().actual());
        assert_eq!("数量", violations[0].column_name());
    }

    /// 収集経路の判定は違反つきの受理の変種を持たない（型の上での保証。要件 6.2）。
    #[test]
    fn the_collect_path_never_accepts_with_violations() {
        let plan = compiled(vec![column(
            "数量",
            declared(TypeKind::Int, Constraints::default()),
        )]);
        let verdict = validate_write(WriteOrigin::Collect, &plan, vec![text("abc")]);
        match &verdict {
            WriteVerdict::Collect(collect) => assert_eq!("rejected", collect_form(collect)),
            other => panic!("収集経路が収集の判定を返さなかった: {other:?}"),
        }
    }

    // 強制と検証の順序（要件 6.1, 6.2, 7.1）。

    /// 両経路で強制が検証より先に走る。
    #[test]
    fn coercion_runs_before_validation_on_both_paths() {
        let plan = compiled(vec![column(
            "数量",
            declared(TypeKind::Int, Constraints::default()),
        )]);

        let (values, coercions) =
            edit_accepted(validate_write(WriteOrigin::Edit, &plan, vec![text("7")]));
        assert_eq!(vec![CellValue::Int(7)], values);
        assert_eq!(
            vec![Coercion::Converted { from: text("7") }],
            coercions,
            "強制の記録が返っていない（要件 7.4）"
        );

        let (values, coercions) =
            collect_accepted(validate_write(WriteOrigin::Collect, &plan, vec![text("7")]));
        assert_eq!(vec![CellValue::Int(7)], values);
        assert_eq!(vec![Coercion::Converted { from: text("7") }], coercions);
    }

    // 行内の違反を打ち切らない（要件 5.3 を `validate` 層から受け継ぐ）。

    /// 1 行に複数の違反があっても最初の違反で打ち切らない。
    #[test]
    fn a_write_reports_every_violation_in_the_row() {
        let plan = compiled(vec![
            column("数量", declared(TypeKind::Int, Constraints::default())),
            column("単価", declared(TypeKind::Decimal, Constraints::default())),
        ]);

        let (_, _, violations) = edit_violated(validate_write(
            WriteOrigin::Edit,
            &plan,
            vec![text("a"), text("b")],
        ));

        assert_eq!(2, violations.len(), "行内の違反が打ち切られた");
        assert_eq!(0, violations[0].column().index());
        assert_eq!(1, violations[1].column().index());
    }

    // 入れ子の位置（要件 6.4, 3.4）。

    /// 入れ子の内側の違反も、フィールド名の位置つきで返る。
    #[test]
    fn a_nested_violation_carries_its_position() {
        let fields = vec![field(
            "色",
            declared(
                TypeKind::Enum,
                Constraints {
                    choices: vec!["赤".into(), "青".into()],
                    ..Constraints::default()
                },
            ),
            true,
        )];
        let plan = compiled(vec![column(
            "属性",
            declared(
                TypeKind::Object,
                Constraints {
                    fields,
                    ..Constraints::default()
                },
            ),
        )]);

        let value = CellValue::Nested(NestedValue::Object(
            vec![("色".to_owned(), text("緑"))].into_iter().collect(),
        ));
        let (_, _, violations) =
            edit_violated(validate_write(WriteOrigin::Edit, &plan, vec![value]));

        assert_eq!(1, violations.len());
        assert_eq!(
            vec![ValuePathSegment::Field("色".into())],
            violations[0].path().segments(),
            "入れ子の内側の位置が特定できない（要件 3.4）"
        );
        assert_eq!("属性", violations[0].column_name());
        assert!(matches!(
            violations[0].reason(),
            ViolationReason::ChoiceNotAllowed { .. }
        ));
    }

    // 行を跨ぐ性質は判定しない（design.md「Validate Layer / SheetValidator」）。

    /// 一意性と参照の実在はここでは判定しない（一括経路が持つ）。
    #[test]
    fn row_spanning_properties_are_left_to_the_batch_path() {
        let unique = ColumnDecl {
            unique: true,
            ..column("数量", declared(TypeKind::Int, Constraints::default()))
        };
        let reference = column(
            "仕入先",
            declared(
                TypeKind::Ref,
                Constraints {
                    sheet: Some(sheet_id()),
                    ..Constraints::default()
                },
            ),
        );
        let plan = compiled(vec![unique, reference]);

        // 一意な値と実在しない行を指す参照を書いても、1 行だけでは決まらない性質は
        // 判定しない（一括経路が持つ）。したがってこの書き込みは**適合として受理される**。
        let verdict = validate_write(
            WriteOrigin::Edit,
            &plan,
            vec![CellValue::Int(1), text("01K4ANRRG004HMASW9NF6YY101")],
        );
        let (values, violations) = match &verdict {
            WriteVerdict::Edit(EditVerdict::Accepted { values, .. }) => {
                (values.as_slice(), &[][..])
            }
            WriteVerdict::Edit(EditVerdict::AcceptedWithViolations {
                values, violations, ..
            }) => (values.as_slice(), violations.as_slice()),
            other => panic!("編集経路が編集の判定を返さなかった: {other:?}"),
        };

        assert!(
            violations.is_empty(),
            "1 行だけでは決まらない性質を書き込み経路が判定した: {violations:?}"
        );
        assert_eq!(
            vec![CellValue::Int(1), text("01K4ANRRG004HMASW9NF6YY101")],
            values
        );
    }

    // 使用不能な列（要件 11.7 の書き込み経路への波及）。

    /// 使用不能な列の値は、どちらの経路でも使用不能として報告される。
    #[test]
    fn an_unusable_column_is_reported_on_both_paths() {
        let plan = compiled(vec![column(
            "将来",
            TypeDecl::Kind {
                kind: DeclaredKind::Unknown("future-kind".into()),
                constraints: Constraints::default(),
            },
        )]);

        let (_, _, violations) =
            edit_violated(validate_write(WriteOrigin::Edit, &plan, vec![text("x")]));
        assert_eq!(1, violations.len());
        assert!(matches!(
            violations[0].reason(),
            ViolationReason::UnusableColumn { kind, .. } if kind.as_ref() == "future-kind"
        ));

        let violations =
            collect_rejected(validate_write(WriteOrigin::Collect, &plan, vec![text("x")]));
        assert_eq!(1, violations.len());
        assert!(matches!(
            violations[0].reason(),
            ViolationReason::UnusableColumn { .. }
        ));
    }

    // 経路ごとの判定が受理の可否で食い違わないこと。

    /// 両経路は適合の可否で一致し、食い違うのは判定の形だけである。
    #[test]
    fn the_two_paths_agree_on_whether_the_write_is_accepted() {
        let plan = compiled(vec![
            column("数量", declared(TypeKind::Int, Constraints::default())),
            ColumnDecl {
                required: true,
                ..column(
                    "名称",
                    declared(
                        TypeKind::Text,
                        Constraints {
                            max_length: Some(2),
                            ..Constraints::default()
                        },
                    ),
                )
            },
        ]);

        let cases = vec![
            vec![CellValue::Int(1), text("ab")],
            vec![text("x"), text("abc")],
            vec![CellValue::Null, text("")],
            vec![CellValue::Int(1), CellValue::Null],
            vec![text("007"), text("ab")],
        ];

        for values in cases {
            let edit = validate_write(WriteOrigin::Edit, &plan, values.clone());
            let collect = validate_write(WriteOrigin::Collect, &plan, values.clone());

            let (edit_values, edit_violations) = match &edit {
                WriteVerdict::Edit(EditVerdict::Accepted { values, .. }) => {
                    (values.as_slice(), &[][..])
                }
                WriteVerdict::Edit(EditVerdict::AcceptedWithViolations {
                    values,
                    violations,
                    ..
                }) => (values.as_slice(), violations.as_slice()),
                other => panic!("編集経路が編集の判定を返さなかった: {other:?}"),
            };
            let (collect_values, collect_violations) = match &collect {
                WriteVerdict::Collect(CollectVerdict::Accepted { values, .. }) => {
                    (values.as_slice(), &[][..])
                }
                WriteVerdict::Collect(CollectVerdict::Rejected { violations }) => {
                    (&[][..], violations.as_slice())
                }
                other => panic!("収集経路が収集の判定を返さなかった: {other:?}"),
            };

            assert_eq!(
                edit_violations.is_empty(),
                collect_violations.is_empty(),
                "経路によって適合の可否が食い違った: {values:?}"
            );
            assert_eq!(
                edit_violations, collect_violations,
                "経路によって違反が食い違った: {values:?}"
            );
            if edit_violations.is_empty() {
                assert_eq!(
                    edit_values, collect_values,
                    "受理された値が経路によって食い違った: {values:?}"
                );
            }
        }
    }

    // 1 セルの判定の所要時間（要件 10.2）。

    /// 1 セルの書き込みの判定が 16 ミリ秒以内に返る（テスト内の計測。CI のゲートにしない）。
    #[test]
    fn the_single_cell_verdict_returns_within_sixteen_milliseconds() {
        // 実運用に近い 30 列を組み立て、1 セル分の書き込みを判定する。
        let mut columns = Vec::with_capacity(30);
        for index in 0..30 {
            let name = format!("列{index}");
            let ty = match index % 3 {
                0 => declared(TypeKind::Int, Constraints::default()),
                1 => declared(
                    TypeKind::Text,
                    Constraints {
                        max_length: Some(32),
                        ..Constraints::default()
                    },
                ),
                _ => declared(
                    TypeKind::Decimal,
                    Constraints {
                        digits: Some(crate::types::decimal::DecimalDigits::new(12, 2).unwrap()),
                        ..Constraints::default()
                    },
                ),
            };
            columns.push(column(&name, ty));
        }
        let plan = compiled(columns);

        // 1 セルだけを打ち込む（残りは値なし）。
        let mut values = vec![CellValue::Null; 30];
        values[7] = text("こんにちは");

        let started = Instant::now();
        let verdict = validate_write(WriteOrigin::Edit, &plan, values);
        let elapsed = started.elapsed();

        assert!(
            matches!(verdict, WriteVerdict::Edit(EditVerdict::Accepted { .. })),
            "適合する値の書き込みが受理されなかった"
        );
        assert!(
            elapsed < Duration::from_millis(16),
            "1 セルの判定が 16 ミリ秒を超えた: {elapsed:?}"
        );
    }
}

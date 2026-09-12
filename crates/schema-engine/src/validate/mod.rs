//! 検証層（design.md「コンポーネントとファイルの対応」の `SheetValidator` /
//! `CellValidator` / `UniqueScan` / `ReferenceScan` / `ViolationReport`。tasks.md 群 5）。
//!
//! 10 万行を 1 回の呼び出しで検証し、違反を位置と理由つきで報告する。**一括経路であること**
//! が本層の契約である（要件 10.4。`structure.md`「性能はドメイン側で守る」: 行ごとに境界を
//! 越える API を公開しない）。走査は 2 段であり、第 1 段が行ごとの値の判定（[`cell`]）、
//! 第 2 段が行を跨ぐ性質の判定（[`unique`] と [`refs`]）である。最後に安定併合して順序を
//! 決める（design.md「System Flows」）。
//!
//! | モジュール | 責務 | 要件 |
//! |------------|------|------|
//! | [`cell`] | 1 セルの判定と入れ子の再帰 | 2.6, 3.4, 4.4, 5.3, 11.3, 11.5 |
//! | [`unique`] | 一意制約の 1 パス判定 | 4.6, 4.7 |
//! | [`refs`] | シート間参照の一括実在判定 | 9.2, 9.3, 9.4, 9.5, 9.6 |
//! | [`report`] | 違反の表現・順序・上限 | 5.1, 5.2, 5.3, 5.5, 5.6 |
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本層は `compile` 層までを参照でき、`write` 以降へは依存しない。
//! 本層の中で最も下流に置かれる葉は [`report`] であり、他の 3 つのサブモジュールと本
//! ファイルがその型を組み立てる。
//!
//! # 入口（tasks.md 5.4）
//!
//! - [`validate_sheet`]: シート全体の一括検証（要件 5.4, 5.7, 10.4）
//! - [`validate_columns`]: 指定した列だけの再検証（要件 10.5）
//!
//! どちらも 1 回の呼び出しで全行を舐める。呼び出し元が行ごとに本層を呼ぶ経路は用意しない
//! （要件 10.4）。
//!
//! # 2 段の併合（design.md「System Flows / 一括検証の 2 段構成」。要件 5.4, 5.5）
//!
//! 第 2 段（[`unique::scan`] と [`refs::scan`]）は全行を走査し終えてから違反を返す。
//! したがって両段の結果をそのまま連結すると、同じ行の違反が段を跨いで離れて並ぶ。本
//! ファイルは第 2 段の違反を行の識別子でまとめ、**第 1 段を行の並び順に流しながら 1 行
//! ずつ併合する**。並びは 行の並び順 → 列の添字 → 入れ子の位置 である。
//!
//! 行ごとに併合するのは記憶域のためでもある。第 1 段の違反を全行分ためてから並べ替えると、
//! 上限（[`ValidationOptions`]）が守るはずの記憶域が上限の外で膨らむ（design.md「検証結果の
//! 表現」の上限の存在理由）。1 行分だけを確保し、最終の報告は上限までしか保持しない。
//!
//! # 入れ子の位置の順序（[`compare_paths`]）
//!
//! 位置の段を先頭から比べ、フィールド名は文字列として、添字は数値として比べる。**宣言順
//! ではない** — 併合は計画の木を引かずに違反だけで順序を決める必要があり、値の形から
//! 復元できるのは名前と添字だけである。同じ位置の 2 件は第 1 段を先に置く（`sort_by` が
//! 安定であるため）。この順序は同じ入力に対して常に同じ結果を与える（要件 5.5）。

pub mod cell;
pub mod refs;
pub mod report;
pub mod unique;

use std::cmp::Ordering;
use std::collections::HashMap;

use document_format::{Document, Row, RowId, SheetId};

use crate::compile::plan::ColumnIndex;
use crate::compile::CompiledSchema;

use report::{
    SheetReport, ValidationOptions, ValuePath, ValuePathSegment, Violation, ViolationReport,
};

/// シート全体の一括検証（tasks.md 5.4。要件 5.4, 5.7, 10.4）。
///
/// `sheet` の全行を 1 回の呼び出しで検証し、上限までの違反・総件数・違反を持つ行の一覧を
/// 返す。`schema` は同じシートからコンパイルした計画であること（`Row::values()` の並びが
/// [`CompiledSchema::columns`] と一致する前提。design.md「Data Models / Domain Model」の
/// 不変条件）。
///
/// 文書を変更しない。何度呼んでも同じ結果になる（design.md「Validate Layer /
/// SheetValidator」の Batch 契約）。
pub fn validate_sheet(
    doc: &Document,
    sheet: SheetId,
    schema: &CompiledSchema,
    options: &ValidationOptions,
) -> SheetReport {
    validate(doc, sheet, schema, None, options)
}

/// 指定した列だけの再検証（tasks.md 5.4。要件 10.5）。
///
/// スキーマの一部が変わったときに全列を舐め直さないための経路である。指定できるのは列の
/// 添字であり、`columns` の**並びは結果に影響しない**（列添字の昇順へ正規化し、重複も
/// 畳む）。行を跨ぐ性質（一意性と参照の実在）も指定した列に閉じて判定する。
pub fn validate_columns(
    doc: &Document,
    sheet: SheetId,
    schema: &CompiledSchema,
    columns: &[ColumnIndex],
    options: &ValidationOptions,
) -> SheetReport {
    let mut selected = columns.to_vec();
    selected.sort_unstable();
    selected.dedup();
    validate(doc, sheet, schema, Some(&selected), options)
}

/// 2 段の走査と安定併合（[`validate_sheet`] と [`validate_columns`] が共有する本体）。
///
/// `selected` が `Some` のときはその列の違反だけを結果に載せる（列添字の昇順に正規化済みで
/// あること）。`None` は全列である。
fn validate(
    doc: &Document,
    sheet: SheetId,
    schema: &CompiledSchema,
    selected: Option<&[ColumnIndex]>,
    options: &ValidationOptions,
) -> SheetReport {
    let mut report = ViolationReport::new(options);
    let Some(target) = doc.sheet_by_id(sheet) else {
        // 文書に無いシート（削除されたシートを指した場合）は検証する行を持たない。
        return report.finish(sheet);
    };
    let rows = target.rows();

    // 第 2 段: 行を跨ぐ性質（一意性と参照の実在）。全行を走査し終えてから違反が返るため、
    // 行ごとに併合できるよう行の識別子でまとめておく。参照を 1 つも含まない計画では
    // どちらの走査も空を返す（呼び出し元は判定の要否を確かめずに呼んでよい）。
    let mut cross_by_row: HashMap<RowId, Vec<Violation>> = HashMap::new();
    let mut orphans: Vec<Violation> = Vec::new();
    let found = unique::scan(schema, rows.iter().map(|row| (row.id(), row.values())))
        .into_iter()
        .chain(refs::scan(
            doc,
            schema,
            rows.iter().map(|row| (row.id(), row.values())),
        ));
    for violation in found {
        if let Some(selected) = selected {
            if selected.binary_search(&violation.column()).is_err() {
                continue;
            }
        }
        match violation.row() {
            Some(row) => cross_by_row.entry(row).or_default().push(violation),
            // 行に属さない違反（列そのものの問題）は行の並びに置けない。末尾へ回す。
            None => orphans.push(violation),
        }
    }

    // 第 1 段: 行ごとの値の判定。行の並び順に流し、その行の第 2 段の違反を併合する。
    // 確保するのは 1 行分だけである（モジュール docs「2 段の併合」）。
    let mut scratch = ViolationReport::new(&ValidationOptions::unlimited());
    for row in rows {
        let mut violations = value_violations(schema, row, selected, &mut scratch);
        if let Some(mut extra) = cross_by_row.remove(&row.id()) {
            violations.append(&mut extra);
        }
        // 安定な並べ替えであり、同じ位置の 2 件は第 1 段が先に残る。
        violations.sort_by(compare_violations);
        for violation in violations {
            report.push(violation);
        }
    }

    // 行の一覧に置けなかった違反（列そのものの問題。design.md「検証結果の表現」の
    // `row: Option<RowId>`）。行の一覧に載らないため、行の並びの外へ決定的に並べて置く。
    orphans.sort_by(compare_violations);
    report.extend(orphans);

    // 第 2 段の走査は第 1 段と同じ行の一覧を見るため、行の識別子は必ず一致する。ここに
    // 残りがあれば、その違反は行の一覧に現れない行を指している（報告から落ちる）。
    debug_assert!(
        cross_by_row.is_empty(),
        "第 2 段の違反が行の一覧の外の行を指した"
    );

    report.finish(sheet)
}

/// 1 行分の第 1 段の違反を取り出す（`scratch` は行ごとに使い回す）。
///
/// 確保するのは 1 行分だけであり、`scratch` は行を跨いで空に戻る
/// （[`ViolationReport::take_violations`]）。
fn value_violations(
    schema: &CompiledSchema,
    row: &Row,
    selected: Option<&[ColumnIndex]>,
    scratch: &mut ViolationReport,
) -> Vec<Violation> {
    match selected {
        Some(columns) => {
            cell::validate_row_columns(schema, Some(row.id()), row.values(), columns, scratch)
        }
        None => cell::validate_row(schema, Some(row.id()), row.values(), scratch),
    }
    scratch.take_violations()
}

/// 同じ行の中での違反の順序（design.md「検証結果の表現」: 列の添字 → 入れ子の位置）。
///
/// 行は呼び出し側が行の並び順に処理するため、ここでは行を比べない。
fn compare_violations(left: &Violation, right: &Violation) -> Ordering {
    left.column()
        .cmp(&right.column())
        .then_with(|| compare_paths(left.path(), right.path()))
}

/// 入れ子の位置の順序（モジュール docs「入れ子の位置の順序」）。
///
/// 段を先頭から比べ、短い方が先である（親の位置が子の位置より先に来る）。
fn compare_paths(left: &ValuePath, right: &ValuePath) -> Ordering {
    let mut left = left.segments().iter();
    let mut right = right.segments().iter();
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(field), Some(other)) => match compare_segments(field, other) {
                Ordering::Equal => continue,
                order => return order,
            },
        }
    }
}

/// 位置の 1 段の順序。名前は文字列として、添字は数値として比べる。
///
/// 1 つの値はオブジェクトか配列のどちらかであるため、名前と添字が同じ段で競合することは
/// 無い。それでも全順序にするため、名前を先に置く。
fn compare_segments(left: &ValuePathSegment, right: &ValuePathSegment) -> Ordering {
    match (left, right) {
        (ValuePathSegment::Field(left), ValuePathSegment::Field(right)) => left.cmp(right),
        (ValuePathSegment::Index(left), ValuePathSegment::Index(right)) => left.cmp(right),
        (ValuePathSegment::Field(_), ValuePathSegment::Index(_)) => Ordering::Less,
        (ValuePathSegment::Index(_), ValuePathSegment::Field(_)) => Ordering::Greater,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::compile::compile_declaration;
    use crate::declaration::{ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl};
    use crate::registry::TypeRegistry;
    use crate::types::TypeKind;
    use crate::validate::report::ViolationReason;
    use document_format::{CellValue, Document, IdFactory, NestedValue, RowId, SheetId};

    /// 整数のセル値。
    fn int(value: i64) -> CellValue {
        CellValue::Int(value)
    }

    /// 文字列のセル値。
    fn text(value: &str) -> CellValue {
        CellValue::Text(value.to_owned())
    }

    /// 入れ子のオブジェクトのセル値（キー順をそのまま保つ）。
    fn object(entries: Vec<(&str, CellValue)>) -> CellValue {
        CellValue::Nested(NestedValue::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        ))
    }

    /// 種別と制約による型の指定。
    fn kind(kind: TypeKind, constraints: Constraints) -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(kind),
            constraints,
        }
    }

    /// 種別と制約による列を組み立てる。
    fn column(name: &str, kind_of: TypeKind, constraints: Constraints) -> ColumnDecl {
        ColumnDecl {
            name: name.into(),
            ty: kind(kind_of, constraints),
            required: false,
            unique: false,
            default: None,
            description: None,
        }
    }

    /// 一意制約を持つ列。
    fn unique_column(name: &str, kind_of: TypeKind) -> ColumnDecl {
        ColumnDecl {
            unique: true,
            ..column(name, kind_of, Constraints::default())
        }
    }

    /// 値なしを許さない列。
    fn required_column(name: &str, kind_of: TypeKind) -> ColumnDecl {
        ColumnDecl {
            required: true,
            ..column(name, kind_of, Constraints::default())
        }
    }

    /// 上限つきの整数の列。
    fn bounded_int_column(name: &str, max: i64) -> ColumnDecl {
        column(
            name,
            TypeKind::Int,
            Constraints {
                max: Some(CellValue::Int(max)),
                ..Constraints::default()
            },
        )
    }

    /// 参照先シートを指す型の指定。
    fn ref_type(sheet: SheetId) -> TypeDecl {
        kind(
            TypeKind::Ref,
            Constraints {
                sheet: Some(sheet),
                ..Constraints::default()
            },
        )
    }

    /// 参照先シートを指す列。
    fn ref_column(name: &str, sheet: SheetId) -> ColumnDecl {
        column(
            name,
            TypeKind::Ref,
            Constraints {
                sheet: Some(sheet),
                ..Constraints::default()
            },
        )
    }

    /// 入れ子のフィールド 1 本分の宣言。
    fn field(name: &str, ty: TypeDecl) -> FieldDecl {
        FieldDecl {
            name: name.into(),
            ty,
            required: false,
            default: None,
            description: None,
        }
    }

    /// 列名を宣言したシートを文書へ足す。
    fn add_sheet(doc: &mut Document, name: &str, columns: &[&str]) -> SheetId {
        let sheet = doc.add_sheet(name);
        let columns = columns.iter().map(|column| (*column).to_owned()).collect();
        doc.set_sheet_columns(sheet, columns)
            .expect("標本のシートが文書にある");
        sheet
    }

    /// シートへ行を足し、発行された識別子を返す。
    fn add_row(doc: &mut Document, sheet: SheetId, values: Vec<CellValue>) -> RowId {
        let row = doc.add_row(sheet).expect("標本のシートが文書にある");
        doc.set_row_values(sheet, row, values)
            .expect("標本の行がシートにある");
        row
    }

    /// 宣言から計画を組み立てる。
    fn compiled(columns: Vec<ColumnDecl>) -> CompiledSchema {
        compile_declaration(&Schema { columns }, &[], &TypeRegistry::new())
            .expect("標本の宣言はコンパイルできる")
    }

    /// 違反の並びを `(行, 列添字, 入れ子の位置)` の形で取り出す。
    fn positions(report: &SheetReport) -> Vec<(Option<RowId>, usize, Vec<ValuePathSegment>)> {
        report
            .violations()
            .iter()
            .map(|violation| {
                (
                    violation.row(),
                    violation.column().index(),
                    violation.path().segments().to_vec(),
                )
            })
            .collect()
    }

    /// フィールド名 1 段の位置。
    fn at_field(name: &str) -> Vec<ValuePathSegment> {
        vec![ValuePathSegment::Field(name.into())]
    }

    /// 2 段の標本（値の違反と行を跨ぐ違反が同じ行に同居する）。
    ///
    /// - `品番` は一意（同じ値を持つ 2 行がある）
    /// - `数量` は上限 10（2 行とも超える）
    /// - `仕入先` は参照（1 行目は実在、2 行目は実在しない）
    /// - `属性` は入れ子（内側に参照と上限つきの整数を持つ）
    fn two_stage_sample() -> (Document, SheetId, CompiledSchema, RowId, RowId) {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let sheet = add_sheet(&mut doc, "発注", &["品番", "数量", "仕入先", "属性"]);
        let supplier = add_row(&mut doc, target, vec![text("甲")]);
        let missing = IdFactory::new().new_row_id().to_string();

        let attributes = Constraints {
            fields: vec![
                field("a", ref_type(target)),
                field(
                    "b",
                    kind(
                        TypeKind::Int,
                        Constraints {
                            max: Some(CellValue::Int(10)),
                            ..Constraints::default()
                        },
                    ),
                ),
            ],
            ..Constraints::default()
        };
        let schema = compiled(vec![
            unique_column("品番", TypeKind::Text),
            bounded_int_column("数量", 10),
            ref_column("仕入先", target),
            column("属性", TypeKind::Object, attributes),
        ]);

        let first = add_row(
            &mut doc,
            sheet,
            vec![
                text("A"),
                int(99),
                text(&supplier.to_string()),
                object(vec![("a", text(&supplier.to_string())), ("b", int(1))]),
            ],
        );
        let second = add_row(
            &mut doc,
            sheet,
            vec![
                text("A"),
                int(99),
                text(&missing),
                object(vec![("a", text(&missing)), ("b", int(99))]),
            ],
        );
        (doc, sheet, schema, first, second)
    }

    /// 第 1 段と第 2 段の違反が、行の並び順 → 列の添字 → 入れ子の位置で併合される
    /// （tasks.md 5.4。要件 5.4, 5.5, 10.4）。
    #[test]
    fn the_two_stages_are_merged_by_row_then_column_then_nested_position() {
        let (doc, sheet, schema, first, second) = two_stage_sample();
        let report = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());

        assert_eq!(
            vec![
                (Some(first), 0, vec![]),
                (Some(first), 1, vec![]),
                (Some(second), 1, vec![]),
                (Some(second), 2, vec![]),
                (Some(second), 3, at_field("a")),
                (Some(second), 3, at_field("b")),
            ],
            positions(&report),
            "2 段の併合の並びが違う"
        );

        // 2 行目で初めて現れる重複の違反は、**最初に現れた行**（1 行目）に属する。
        assert!(
            matches!(
                report.violations()[0].reason(),
                ViolationReason::Duplicate { rows, .. } if rows.as_slice() == [first, second].as_slice()
            ),
            "重複の違反が重複するすべての行を運んでいない"
        );
        assert!(
            matches!(
                report.violations()[1].reason(),
                ViolationReason::OutOfRange { .. }
            ) && matches!(
                report.violations()[2].reason(),
                ViolationReason::OutOfRange { .. }
            ) && matches!(
                report.violations()[3].reason(),
                ViolationReason::BrokenReference { .. }
            ) && matches!(
                report.violations()[4].reason(),
                ViolationReason::BrokenReference { .. }
            ) && matches!(
                report.violations()[5].reason(),
                ViolationReason::OutOfRange { .. }
            ),
            "違反の理由が期待と違う"
        );

        // 違反を持つ行だけが行の並び順で載る（要件 5.4）。
        assert_eq!(vec![first, second], report.invalid_rows());
        assert_eq!(6, report.total_violations());
    }

    /// 同一の入力に対して違反集合と順序が常に一致する（tasks.md 5.4。要件 5.5）。
    #[test]
    fn the_same_input_always_yields_the_same_violations_in_the_same_order() {
        let (doc, sheet, schema, _, _) = two_stage_sample();
        let first = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
        for _ in 0..8 {
            let again = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
            assert_eq!(first, again, "同じ入力で違反集合か順序が変わった");
        }
    }

    /// 上限に達しても総件数を数え続け、上限がなければ全件を保持する（要件 5.6）。
    #[test]
    fn the_cap_limits_held_violations_but_not_the_count_or_the_invalid_rows() {
        let mut doc = Document::new();
        let sheet = add_sheet(&mut doc, "空", &["甲", "乙", "丙"]);
        let mut rows = Vec::new();
        for _ in 0..40 {
            rows.push(add_row(&mut doc, sheet, vec![CellValue::Null; 3]));
        }
        let schema = compiled(vec![
            required_column("甲", TypeKind::Text),
            required_column("乙", TypeKind::Int),
            required_column("丙", TypeKind::Bool),
        ]);

        let capped = validate_sheet(&doc, sheet, &schema, &ValidationOptions::capped(7));
        assert_eq!(
            7,
            capped.violations().len(),
            "上限までの違反を保持していない"
        );
        assert_eq!(120, capped.total_violations(), "総件数を数え落としている");
        assert_eq!(
            rows,
            capped.invalid_rows(),
            "違反を持つ行の一覧が欠けている"
        );
        assert!(capped.is_truncated(), "打ち切りが観測できない");

        let unlimited = validate_sheet(&doc, sheet, &schema, &ValidationOptions::unlimited());
        assert_eq!(
            120,
            unlimited.violations().len(),
            "上限なしで全件が残らない"
        );
        assert!(!unlimited.is_truncated(), "上限なしで打ち切りになっている");
    }

    /// 指定した列だけを再検証し、その列に閉じた第 1 段と第 2 段の違反を返す
    /// （tasks.md 5.4。要件 10.5）。
    #[test]
    fn revalidating_selected_columns_reports_only_those_columns() {
        let (doc, sheet, schema, first, second) = two_stage_sample();
        let options = ValidationOptions::default();

        let by_column = |columns: &[ColumnIndex]| {
            positions(&validate_columns(&doc, sheet, &schema, columns, &options))
        };

        // 要求の並びに依らず列添字の順で報告する。
        assert_eq!(
            vec![
                (Some(first), 0, vec![]),
                (Some(second), 3, at_field("a")),
                (Some(second), 3, at_field("b")),
            ],
            by_column(&[ColumnIndex::new(3), ColumnIndex::new(0)]),
            "指定した列の違反だけが列添字の順に出ていない"
        );

        // 第 2 段の違反（重複・壊れた参照）も指定した列に閉じる。
        assert_eq!(
            vec![(Some(first), 0, vec![])],
            by_column(&[ColumnIndex::new(0)]),
            "指定していない列の違反が混ざっている"
        );
        assert!(by_column(&[]).is_empty(), "空の指定で違反が出ている");
        assert!(
            by_column(&[ColumnIndex::new(99)]).is_empty(),
            "計画の外の列が判定された"
        );
    }

    /// 検証する行を持たないシートは、違反の無い結果になる（要件 5.4, 5.7）。
    #[test]
    fn a_sheet_without_rows_or_a_document_without_the_sheet_yields_no_violations() {
        let (doc, _sheet, schema, _, _) = two_stage_sample();
        let options = ValidationOptions::default();

        // 行を 1 つも持たないシート。
        let mut empty_document = Document::new();
        let empty = add_sheet(&mut empty_document, "空", &["品番"]);
        let report = validate_sheet(&empty_document, empty, &schema, &options);
        assert_eq!(
            0,
            report.total_violations(),
            "行の無いシートで違反が出ている"
        );
        assert!(
            report.invalid_rows().is_empty(),
            "行の無いシートで違反行が出ている"
        );

        // 文書に無いシート（削除されたシートを指した場合）。
        let missing = IdFactory::new().new_sheet_id();
        let report = validate_sheet(&doc, missing, &schema, &options);
        assert_eq!(
            0,
            report.total_violations(),
            "文書に無いシートで違反が出ている"
        );
        assert!(
            report.invalid_rows().is_empty(),
            "文書に無いシートで違反行が出ている"
        );
    }
}

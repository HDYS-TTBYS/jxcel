//! シート全体の一括検証の統合テスト（design.md「Testing Strategy / Integration Tests」の
//! `validation.rs`。tasks.md 5.4）。要件 5.4, 5.5, 5.7, 10.4, 10.5。
//!
//! 上流のドキュメントモデルに載せたスキーマ宣言から計画を組み立て、シートを 1 回の
//! 呼び出しで検証する経路を、実際の保存形式（ルートスキーマの不透明ペイロード）を通して
//! 確かめる。スキーマ層の単体の検証は `validate` 層の各モジュールが持ち、本ファイルは
//! **宣言から計画までの結線**と、2 段の併合の結果だけを見る。

use document_format::{CellValue, Document, IdFactory, RowId, SchemaPart, SheetId};
use schema_engine::compile::{compile, ColumnIndex};
use schema_engine::registry::TypeRegistry;
use schema_engine::validate::report::{ValidationOptions, ViolationReason};
use schema_engine::validate::{validate_columns, validate_sheet};

/// ルートスキーマの標本（不透明ペイロードとして上流へ渡す）。
///
/// `品番` は一意、`数量` は上限 10、`納品日` は日付である。列名は行オブジェクトのキーと
/// 同じ並びになる。
const ROOT: &str = concat!(
    r#"{"columns":["#,
    r#"{"name":"品番","type":{"kind":"text"},"unique":true},"#,
    r#"{"name":"数量","type":{"kind":"int","max":10}},"#,
    r#"{"name":"納品日","type":{"kind":"date"}}"#,
    r#"]}"#,
);

/// 整数のセル値。
fn int(value: i64) -> CellValue {
    CellValue::Int(value)
}

/// 文字列のセル値。
fn text(value: &str) -> CellValue {
    CellValue::Text(value.to_owned())
}

/// スキーマ宣言を載せたシートを持つ文書と、その計画を組み立てる。
fn sample() -> (
    Document,
    SheetId,
    schema_engine::compile::CompiledSchema,
    Vec<RowId>,
) {
    let mut doc = Document::new();
    let sheet = doc.add_sheet("発注");
    doc.set_sheet_columns(
        sheet,
        vec!["品番".to_owned(), "数量".to_owned(), "納品日".to_owned()],
    )
    .expect("標本のシートが文書にある");
    let envelope = format!(r#"{{"root":{ROOT},"types":[]}}"#);
    doc.set_root_schema(
        sheet,
        SchemaPart::parse(&envelope).expect("標本のエンベロープは妥当"),
    )
    .expect("標本のシートが文書にある");

    let mut rows = Vec::new();
    for values in [
        // 適合する行。
        vec![text("A"), int(5), text("2026-09-12")],
        // 重複（1 行目と同じ `品番`）と範囲外（上限 10 を超える）。
        vec![text("A"), int(99), text("2026-09-12")],
        // 日付として解釈できない値。
        vec![text("B"), int(1), text("2026/09/12")],
        // 値なし（`品番` は必須でなく、一意の比較からも外れる）。
        vec![CellValue::Null, int(1), CellValue::Null],
    ] {
        let row = doc.add_row(sheet).expect("標本のシートが文書にある");
        doc.set_row_values(sheet, row, values)
            .expect("標本の行がシートにある");
        rows.push(row);
    }

    let schema = compile(
        doc.sheet_by_id(sheet).expect("標本のシートが文書にある"),
        &TypeRegistry::new(),
    )
    .expect("標本の宣言はコンパイルできる");
    (doc, sheet, schema, rows)
}

/// 文書を開いたときに全行の検証結果が取得でき、違反を持つ行と持たない行が区別される
/// （要件 5.4, 5.7, 10.4）。
#[test]
fn opening_a_sheet_yields_every_rows_verdicts_in_one_batch_call() {
    let (doc, sheet, schema, rows) = sample();
    let report = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());

    assert_eq!(3, report.total_violations(), "違反の総件数が違う");
    assert_eq!(
        vec![rows[0], rows[1], rows[2]],
        report.invalid_rows(),
        "違反を持つ行の一覧が違う（行の並び順で重複しない）"
    );

    let shape: Vec<(usize, Option<RowId>)> = report
        .violations()
        .iter()
        .map(|violation| (violation.column().index(), violation.row()))
        .collect();
    assert_eq!(
        vec![
            // 重複の違反は最初に現れた行（1 行目）に属し、その行の列の順に並ぶ。
            (0, Some(rows[0])),
            (1, Some(rows[1])),
            (2, Some(rows[2])),
        ],
        shape,
        "2 段の併合の並びが違う"
    );
    assert!(
        matches!(
            report.violations()[0].reason(),
            ViolationReason::Duplicate { .. }
        ) && matches!(
            report.violations()[1].reason(),
            ViolationReason::OutOfRange { .. }
        ) && matches!(
            report.violations()[2].reason(),
            ViolationReason::TypeMismatch { .. }
        ),
        "違反の理由が期待と違う"
    );
}

/// 同一の入力に対して違反集合と順序が常に一致する（要件 5.5）。
#[test]
fn the_same_document_always_yields_the_same_report() {
    let (doc, sheet, schema, _) = sample();
    let first = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
    for _ in 0..4 {
        assert_eq!(
            first,
            validate_sheet(&doc, sheet, &schema, &ValidationOptions::default()),
            "同じ文書で違反集合か順序が変わった"
        );
    }
}

/// 影響を受ける列だけを再検証できる（要件 10.5）。
#[test]
fn revalidating_a_single_column_reports_only_that_column() {
    let (doc, sheet, schema, rows) = sample();
    let report = validate_columns(
        &doc,
        sheet,
        &schema,
        &[ColumnIndex::new(0)],
        &ValidationOptions::default(),
    );

    assert_eq!(
        1,
        report.total_violations(),
        "指定した列以外の違反が混ざっている"
    );
    assert_eq!(vec![rows[0]], report.invalid_rows(), "違反を持つ行が違う");
    assert!(
        matches!(
            report.violations()[0].reason(),
            ViolationReason::Duplicate { .. }
        ),
        "一意制約の違反が列指定の経路で出ていない"
    );

    let untouched = validate_columns(
        &doc,
        sheet,
        &schema,
        &[ColumnIndex::new(1)],
        &ValidationOptions::default(),
    );
    assert_eq!(
        1,
        untouched.total_violations(),
        "指定した列以外の違反が混ざっている"
    );
    assert_eq!(
        vec![rows[1]],
        untouched.invalid_rows(),
        "違反を持つ行が違う"
    );
}

/// 文書に無いシートを指しても、空の結果を返して止まらない（tasks.md 5.4。
/// 上流ではシートの削除が起こりうる）。
#[test]
fn a_missing_sheet_yields_an_empty_report() {
    let (doc, _, schema, _) = sample();
    let missing = IdFactory::new().new_sheet_id();
    let report = validate_sheet(&doc, missing, &schema, &ValidationOptions::default());

    assert_eq!(0, report.total_violations());
    assert!(report.invalid_rows().is_empty());
}

/// 公開面だけを使って、スキーマの宣言から全件検証までが通る（tasks.md 5.4 の一括経路。
/// 要件 10.4）。
#[test]
fn the_batch_entry_point_is_reachable_without_calling_per_row() {
    let (doc, sheet, schema, rows) = sample();
    // 行ごとの呼び出しは 1 度も無い。1 回の呼び出しで全行の判定が返る。
    let report = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
    assert_eq!(
        rows.len(),
        doc.sheet_by_id(sheet)
            .expect("標本のシートが文書にある")
            .rows()
            .len(),
        "標本の行数が変わっている"
    );
    assert!(
        report.total_violations() > 0,
        "一括経路で違反が 1 件も出ていない"
    );
}

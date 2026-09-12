//! 公開面だけを使った、スキーマの宣言から全件検証までの結線（design.md「Public API Layer /
//! SchemaEngineApi」。tasks.md 8.1。要件 1.2, 4.3, 5.7, 10.4）。
//!
//! 本ファイルは `schema_engine` の**根の再輸出だけ**を import する。`schema_engine::compile`
//! や `schema_engine::validate` のような下位モジュールの経路を 1 つも書かない — これが
//! 「他クレートが下位モジュールを直接触らずに済む」ことの実行可能な証明である。上流の
//! `document-format`（`Document` / `CellValue` / `SchemaPart`）は本クレートの内部ではなく
//! 別クレートであるため、ここから直接使ってよい。
//!
//! 通す経路は 8 つ（design.md の Service Interface の全メソッド）である: 計画の生成・列の
//! 並び順の供給・シート全体の検証・列指定の再検証・書き込みの判定・行の初期値・変更の
//! 集計・変更の適用。**検証を走らせる時機は本クレートが決めない** — ここでは呼び出し元
//! （テスト）が引き金を引いている。

use document_format::{CellValue, Document, RowId, SchemaPart, SheetId};
use schema_engine::{
    parse_schema, Coercion, CollectVerdict, ColumnIndex, EditVerdict, SchemaEngine,
    SchemaEngineApi, TypeRegistry, ValidationOptions, ViolationReason, WriteOrigin, WriteVerdict,
};

/// ルートスキーマの標本（不透明ペイロードとして上流へ渡す）。
///
/// `品番` は必須かつ一意で既定値を持ち、`数量` は範囲つきで既定値を持ち、`納品日` は既定値
/// を持たない。**列の並び順はこの配列の順そのもの**であり、行データのキー順になる（要件 1.1,
/// 1.2）。
const ROOT: &str = concat!(
    r#"{"columns":["#,
    r#"{"name":"品番","type":{"kind":"text","maxLength":8},"#,
    r#""required":true,"unique":true,"default":"A"},"#,
    r#"{"name":"数量","type":{"kind":"int","min":0,"max":10},"default":0},"#,
    r#"{"name":"納品日","type":{"kind":"date"}}"#,
    r#"]}"#,
);

/// 列を 1 本足した新しい宣言（要件 8.1 の「列の追加」）。
const ROOT_WITH_NOTE: &str = concat!(
    r#"{"columns":["#,
    r#"{"name":"品番","type":{"kind":"text","maxLength":8},"#,
    r#""required":true,"unique":true,"default":"A"},"#,
    r#"{"name":"数量","type":{"kind":"int","min":0,"max":10},"default":0},"#,
    r#"{"name":"納品日","type":{"kind":"date"}},"#,
    r#"{"name":"備考","type":{"kind":"text"},"default":"未記入"}"#,
    r#"]}"#,
);

/// 上流のエンベロープ（`{ "root": …, "types": [] }`）を組み立てる。
fn envelope(root: &str) -> SchemaPart {
    SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#))
        .expect("標本のエンベロープは妥当")
}

/// 整数のセル値。
fn int(value: i64) -> CellValue {
    CellValue::Int(value)
}

/// 文字列のセル値。
fn text(value: &str) -> CellValue {
    CellValue::Text(value.to_owned())
}

/// ルートスキーマを載せたシートを持つ文書（行はまだ無い）。
fn document(root: &str) -> (Document, SheetId) {
    let mut doc = Document::new();
    let sheet = doc.add_sheet("発注");
    doc.set_root_schema(sheet, envelope(root))
        .expect("標本のシートが文書にある");
    (doc, sheet)
}

/// 適合する行・範囲外の行・日付として解釈できない行を 1 行ずつ加える。
fn add_sample_rows(doc: &mut Document, sheet: SheetId) -> (RowId, RowId, RowId) {
    let rows = [
        vec![text("A"), int(0), CellValue::Null],
        // 数量が上限 10 を超える。
        vec![text("B"), int(99), text("2026-09-12")],
        // 納品日が日付として解釈できない（地域依存の書式は解釈しない。要件 7.3）。
        vec![text("C"), int(1), text("2026/09/12")],
    ];
    let mut ids = Vec::new();
    for values in rows {
        let row = doc.add_row(sheet).expect("標本のシートが文書にある");
        doc.set_row_values(sheet, row, values)
            .expect("標本の行がシートにある");
        ids.push(row);
    }
    (ids[0], ids[1], ids[2])
}

/// 公開面だけで、宣言から計画・列の供給・全件検証・列指定の再検証・書き込みの判定・行の
/// 初期値までが通る（要件 1.2, 4.3, 5.7, 10.4）。
#[test]
fn the_public_surface_carries_a_declaration_to_a_full_validation() {
    let engine = SchemaEngine::new();
    let registry = TypeRegistry::new();
    let (mut doc, sheet) = document(ROOT);

    // 計画の生成（`compile`）は上流の `Sheet`（＝不透明なルートスキーマの保持者）から行う。
    let schema = {
        let target = doc.sheet_by_id(sheet).expect("標本のシートが文書にある");
        engine
            .compile(target, &registry)
            .expect("標本の宣言はコンパイルできる")
    };
    assert_eq!(3, schema.column_count(), "列数が違う");

    // 列の並び順の供給（要件 1.2）。行データを永続化する呼び出し元はこの並びをそのまま
    // 行のキー順として使う（ここでは上流の `set_sheet_columns` がその受け手である）。
    assert_eq!(
        vec!["品番", "数量", "納品日"],
        engine.columns(&schema),
        "供給した列の並び順が宣言と違う"
    );
    doc.set_sheet_columns(sheet, engine.columns(&schema).to_vec())
        .expect("標本のシートが文書にある");

    // 行の初期値（要件 4.3）。既定値の無い列は値なしになる。行の追加そのものは上流の
    // 経路であり、本クレートは値を供給するだけである。
    assert_eq!(
        vec![text("A"), int(0), CellValue::Null],
        engine.default_row(&schema),
        "行の初期値が宣言と違う"
    );

    let (first, second, third) = add_sample_rows(&mut doc, sheet);

    // シート全体の一括検証（要件 5.4, 5.7, 10.4）。行ごとの呼び出しを要しない。
    let options = ValidationOptions::default();
    let report = engine.validate_sheet(&doc, sheet, &schema, &options);
    assert_eq!(2, report.total_violations(), "違反の総件数が違う");
    assert_eq!(
        vec![second, third],
        report.invalid_rows(),
        "違反を持つ行の一覧が違う（1 行目は違反を持たない）"
    );
    let shape: Vec<(usize, Option<RowId>)> = report
        .violations()
        .iter()
        .map(|violation| (violation.column().index(), violation.row()))
        .collect();
    assert_eq!(
        vec![(1, Some(second)), (2, Some(third))],
        shape,
        "違反の並びが行の並び順 → 列添字と違う"
    );
    assert!(
        matches!(
            report.violations()[0].reason(),
            ViolationReason::OutOfRange { .. }
        ) && matches!(
            report.violations()[1].reason(),
            ViolationReason::TypeMismatch { .. }
        ),
        "違反の理由が期待と違う"
    );
    assert_eq!(
        "数量",
        report.violations()[0].column_name(),
        "違反が列名を運んでいない"
    );
    assert!(
        !report.invalid_rows().contains(&first),
        "既定値で埋めた行が違反を持つと判定された"
    );

    // 列指定の再検証（要件 10.5）。日付の列だけを舐め直す。
    let only_date = engine.validate_columns(&doc, sheet, &schema, &[ColumnIndex::new(2)], &options);
    assert_eq!(1, only_date.total_violations(), "列指定の違反件数が違う");
    assert_eq!(
        vec![third],
        only_date.invalid_rows(),
        "列指定の違反行が違う"
    );
    assert_eq!(
        2,
        only_date.violations()[0].column().index(),
        "別の列を検証した"
    );

    // 書き込みの判定（要件 6.1, 6.2, 7.1）。強制も含めて経路ごとの判定が返る。
    let coerced = engine.validate_write(
        WriteOrigin::Edit,
        &schema,
        vec![text("D"), text("7"), text("2026-09-12")],
    );
    match coerced {
        WriteVerdict::Edit(EditVerdict::Accepted { values, coercions }) => {
            assert_eq!(
                vec![text("D"), int(7), text("2026-09-12")],
                values,
                "文字列が列の型へ強制されていない"
            );
            assert!(
                matches!(coercions[1], Coercion::Converted { .. }),
                "変換の記録が残っていない"
            );
        }
        other => panic!("適合する値の編集経路が受理されなかった: {other:?}"),
    }

    let invalid = vec![text("E"), int(99), text("2026-09-12")];
    assert!(
        matches!(
            engine.validate_write(WriteOrigin::Edit, &schema, invalid.clone()),
            WriteVerdict::Edit(EditVerdict::AcceptedWithViolations { .. })
        ),
        "編集経路が違反する値を保持できる形で返さなかった"
    );
    assert!(
        matches!(
            engine.validate_write(WriteOrigin::Collect, &schema, invalid.clone()),
            WriteVerdict::Collect(CollectVerdict::Rejected { .. })
        ),
        "収集経路が違反する値を受け入れた"
    );
    assert!(
        matches!(
            engine.validate_write(
                WriteOrigin::Collect,
                &schema,
                vec![text("F"), int(1), text("2026-09-12")]
            ),
            WriteVerdict::Collect(CollectVerdict::Accepted { .. })
        ),
        "収集経路が適合する値を拒否した"
    );

    // 検証は文書を変更しない（何度呼んでも同じ結果になる）。
    assert_eq!(
        report,
        engine.validate_sheet(&doc, sheet, &schema, &options),
        "同じ文書で結果が変わった"
    );
}

/// 公開面だけで、変更の集計と適用が通る（要件 8.2, 8.4, 8.5, 8.8）。
///
/// 適用の後は、呼び出し元が新しい宣言を自分で設置する（`apply_change` はルートスキーマを
/// 設置しない。tasks.md 7.2 の裁定）。
#[test]
fn the_public_surface_carries_a_change_plan_to_application() {
    let engine = SchemaEngine::new();
    let registry = TypeRegistry::new();
    let (mut doc, sheet) = document(ROOT);
    let schema = {
        let target = doc.sheet_by_id(sheet).expect("標本のシートが文書にある");
        engine
            .compile(target, &registry)
            .expect("標本の宣言はコンパイルできる")
    };
    doc.set_sheet_columns(sheet, engine.columns(&schema).to_vec())
        .expect("標本のシートが文書にある");
    add_sample_rows(&mut doc, sheet);

    let to = parse_schema(ROOT_WITH_NOTE).expect("新しい宣言は解析できる");
    let plan = engine
        .plan_change(&doc, sheet, &to, &registry)
        .expect("新しい宣言は計画できる");

    // 適用の前に、集計だけが得られる（要件 8.2, 8.4）。ドキュメントは変更されない。
    let applied = engine
        .apply_change(&mut doc, plan)
        .expect("計画は陳腐化していない");
    assert_eq!(3, applied.rows(), "適用した行数が違う");
    assert_eq!(
        vec!["品番", "数量", "納品日", "備考"],
        applied.columns(),
        "適用した列の並び順が違う（要件 1.2）"
    );
    assert_eq!(2, applied.impact().violating_rows, "違反になる行数が違う");
    assert_eq!(0, applied.impact().converted_rows, "変換される行数が違う");
    assert_eq!(0, applied.impact().losing_rows, "値が失われる行数が違う");

    // 呼び出し元が新しい宣言を設置し、計画を立て直して検証する。
    doc.set_root_schema(sheet, envelope(ROOT_WITH_NOTE))
        .expect("標本のシートが文書にある");
    let schema = {
        let target = doc.sheet_by_id(sheet).expect("標本のシートが文書にある");
        engine
            .compile(target, &registry)
            .expect("新しい宣言もコンパイルできる")
    };
    assert_eq!(4, schema.column_count(), "追加した列が計画に載っていない");
    let report = engine.validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
    assert_eq!(2, report.total_violations(), "適用後の違反件数が集計と違う");

    // 行は新しい列数と同じ個数の値を持つ（要件 8.8）。
    for row in doc
        .sheet_by_id(sheet)
        .expect("標本のシートが文書にある")
        .rows()
    {
        assert_eq!(4, row.values().len(), "適用後の行の値の個数が列数と違う");
        assert_eq!(
            text("未記入"),
            row.values()[3],
            "追加した列に既定値が入っていない"
        );
    }

    // 集計の後に文書が変われば、適用は陳腐化として拒否する（要件 8.5）。
    let (mut doc, sheet) = document(ROOT);
    let schema = {
        let target = doc.sheet_by_id(sheet).expect("標本のシートが文書にある");
        engine
            .compile(target, &registry)
            .expect("標本の宣言はコンパイルできる")
    };
    doc.set_sheet_columns(sheet, engine.columns(&schema).to_vec())
        .expect("標本のシートが文書にある");
    let to = parse_schema(ROOT_WITH_NOTE).expect("新しい宣言は解析できる");
    let plan = engine
        .plan_change(&doc, sheet, &to, &registry)
        .expect("新しい宣言は計画できる");
    let row = doc.add_row(sheet).expect("標本のシートが文書にある");
    doc.set_row_values(sheet, row, vec![text("G"), int(1), CellValue::Null])
        .expect("標本の行がシートにある");
    assert!(
        engine.apply_change(&mut doc, plan).is_err(),
        "集計の後に変わった文書へ古い計画が適用された"
    );
}

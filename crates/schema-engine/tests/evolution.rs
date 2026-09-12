//! スキーマ変更の影響集計の統合テスト（design.md「Testing Strategy / Integration Tests」の
//! `evolution.rs`。tasks.md 7.2）。要件 8.2, 8.3, 8.4。
//!
//! 上流のドキュメントモデルに載せた宣言から計画を組み立て、**適用した場合に何が起きるか**を
//! 集計する経路を確かめる。適用そのもの（タスク 7.3）はここでは行わず、集計が示した数と
//! 位置、および計画が計算しきった値だけを見る。集計がドキュメントを変更しないことは、
//! 部品集合のバイト列の前後比較で示す（要件 8.4）。

use document_format::parts::to_parts;
use document_format::{CellValue, Document, RowId, SchemaPart, SheetId};
use schema_engine::compile::compile;
use schema_engine::declaration::codec::parse_schema;
use schema_engine::error::SchemaError;
use schema_engine::evolution::impact::{plan_change, PlanDigest, ValueLoss};
use schema_engine::registry::TypeRegistry;
use schema_engine::validate::report::{ValidationOptions, ViolationReason};
use schema_engine::validate::validate_sheet;

/// 旧宣言。`旧名` は改名され、`数量` は型が変わり、`消える` は削除される。
///
/// 削除される列（`消える`）を `int` にし、追加される列をすべて `text` にしてある。改名の
/// 抽出は「型の同一性が等しい未使用の旧列」を対にするため（`diff` の規則。タスク 7.1）、
/// 削除を作るには追加される列と型が重ならないようにする必要がある。
const FROM: &str = concat!(
    r#"{"columns":["#,
    r#"{"name":"旧名","type":{"kind":"text"}},"#,
    r#"{"name":"消える","type":{"kind":"int"}},"#,
    r#"{"name":"数量","type":{"kind":"int"}}"#,
    r#"]}"#,
);

/// 新宣言。改名・型の変更（`int` → `float`）・制約の変更（`max`）・列の追加（既定値あり /
/// 既定値なし）・列の削除を 1 つずつ含む。
const TO: &str = concat!(
    r#"{"columns":["#,
    r#"{"name":"新名","type":{"kind":"text"}},"#,
    r#"{"name":"増えた","type":{"kind":"text"},"default":"なし"},"#,
    r#"{"name":"空欄","type":{"kind":"text"}},"#,
    r#"{"name":"数量","type":{"kind":"float","max":6.0}}"#,
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

/// 宣言を不透明ペイロードとして包む上流のエンベロープ。
fn envelope(root: &str) -> String {
    format!(r#"{{"root":{root},"types":[]}}"#)
}

/// 宣言・列名・行の値から文書を組み立て、対象シートと行識別子を返す。
fn build(root: &str, columns: &[&str], rows: &[Vec<CellValue>]) -> (Document, SheetId, Vec<RowId>) {
    let mut doc = Document::new();
    let sheet = doc.add_sheet("発注");
    doc.set_sheet_columns(
        sheet,
        columns.iter().map(|name| (*name).to_owned()).collect(),
    )
    .expect("標本のシートが文書にある");
    doc.set_root_schema(
        sheet,
        SchemaPart::parse(&envelope(root)).expect("標本のエンベロープは妥当"),
    )
    .expect("標本のシートが文書にある");

    let mut ids = Vec::with_capacity(rows.len());
    for values in rows {
        let row = doc.add_row(sheet).expect("標本のシートが文書にある");
        doc.set_row_values(sheet, row, values.clone())
            .expect("標本の行がシートにある");
        ids.push(row);
    }
    (doc, sheet, ids)
}

/// 標本の 3 行。2 行目の `数量` は新しい上限（6.0）を超え、3 行目の `消える` は値なしである。
fn sample_rows() -> Vec<Vec<CellValue>> {
    vec![
        vec![text("A"), int(7), int(5)],
        vec![text("B"), int(8), int(9)],
        vec![text("C"), CellValue::Null, int(1)],
    ]
}

/// 対象シートの標本を組み立てる。
fn sample() -> (Document, SheetId, Vec<RowId>) {
    build(FROM, &["旧名", "消える", "数量"], &sample_rows())
}

/// 文書の観測可能な状態を部品集合のバイト列として取り出す（要件 8.4 の前後比較）。
fn snapshot(doc: &Document) -> Vec<(String, Vec<u8>)> {
    to_parts(doc)
        .expect("標本は部品集合へ取り出せる")
        .iter()
        .map(|part| (part.name.to_string(), part.bytes.clone()))
        .collect()
}

/// 列の追加では既定値が入り、既定値が宣言されていなければ値なしが入る（要件 8.2, 8.3）。
///
/// 集計は「適用した場合に変換される行数・違反になる行数・値が失われる行数」と、値が
/// 失われる位置を返す（要件 8.2, 8.3）。
#[test]
fn the_impact_counts_conversions_violations_and_losses() {
    let (doc, sheet, ids) = sample();
    let to = parse_schema(TO).expect("新しい宣言は解析できる");
    let plan = plan_change(&doc, sheet, &to, &TypeRegistry::new()).expect("計画は組み立てられる");

    let impact = plan.impact();
    assert_eq!(
        3, impact.converted_rows,
        "`数量` の整数がすべて浮動小数へ変換されるため、変換される行数は全行である"
    );
    assert_eq!(
        1, impact.violating_rows,
        "2 行目の `数量` だけが新しい上限を超える（`float` の 9.0 > 6.0）"
    );
    assert_eq!(
        2, impact.losing_rows,
        "`消える` に値がある行は 2 行である（3 行目は値なしなので失う値が無い）"
    );
    assert_eq!(
        vec![
            ValueLoss {
                row: ids[0],
                column: "消える".into(),
            },
            ValueLoss {
                row: ids[1],
                column: "消える".into(),
            },
        ],
        impact.losses,
        "値が失われる位置は行の並び順 → 削除された列の並び順である（要件 8.3）"
    );

    assert_eq!(
        vec![
            "新名".to_owned(),
            "増えた".to_owned(),
            "空欄".to_owned(),
            "数量".to_owned()
        ],
        plan.columns(),
        "計画は新しい列名の配列を宣言の並び順で持つ"
    );

    let computed: Vec<(RowId, Vec<CellValue>)> = plan
        .rows()
        .map(|(row, values)| (row, values.to_vec()))
        .collect();
    assert_eq!(
        vec![
            (
                ids[0],
                vec![
                    text("A"),
                    text("なし"),
                    CellValue::Null,
                    CellValue::float(5.0)
                ]
            ),
            (
                ids[1],
                vec![
                    text("B"),
                    text("なし"),
                    CellValue::Null,
                    CellValue::float(9.0)
                ]
            ),
            (
                ids[2],
                vec![
                    text("C"),
                    text("なし"),
                    CellValue::Null,
                    CellValue::float(1.0)
                ]
            ),
        ],
        computed,
        "改名は値を運び、追加された列は既定値または値なしを受け取る（要件 8.7, 8.8）"
    );
}

/// 集計を実行してもドキュメントが変わらない（要件 8.4）。
#[test]
fn planning_does_not_touch_the_document() {
    let (doc, sheet, _) = sample();
    let before = snapshot(&doc);

    let to = parse_schema(TO).expect("新しい宣言は解析できる");
    let plan = plan_change(&doc, sheet, &to, &TypeRegistry::new()).expect("計画は組み立てられる");
    assert!(
        plan.impact().converted_rows > 0,
        "標本はドキュメントを観測するに足る影響を持つ"
    );

    assert_eq!(
        before,
        snapshot(&doc),
        "集計がドキュメントを変更した（要件 8.4 は適用が承認されるまで変更しないことを求める）"
    );
}

/// 行を跨ぐ性質（一意制約）の違反も集計に含まれ、適用した状態の全件検証と一致する。
///
/// 第 1 段（値ごとの判定）だけでは 1 件も違反にならない標本を使い、第 2 段の違反が
/// 集計に載ることを確かめる。あわせて、集計の `violating_rows` が実際に適用した状態の
/// 全件検証（タスク 5.4）と同じ行数を数えることを確かめる。
#[test]
fn cross_row_violations_are_counted_and_match_the_applied_state() {
    const FROM_TEXT: &str = concat!(
        r#"{"columns":["#,
        r#"{"name":"名前","type":{"kind":"text"}},"#,
        r#"{"name":"値","type":{"kind":"text"}}"#,
        r#"]}"#,
    );
    const TO_TEXT: &str = concat!(
        r#"{"columns":["#,
        r#"{"name":"名前","type":{"kind":"text"}},"#,
        r#"{"name":"値","type":{"kind":"text"},"unique":true}"#,
        r#"]}"#,
    );

    let rows = vec![
        vec![text("x"), text("1")],
        vec![text("y"), text("1")],
        vec![text("z"), text("2")],
    ];
    let (mut doc, sheet, _) = build(FROM_TEXT, &["名前", "値"], &rows);
    let to = parse_schema(TO_TEXT).expect("新しい宣言は解析できる");
    let plan = plan_change(&doc, sheet, &to, &TypeRegistry::new()).expect("計画は組み立てられる");

    let impact = plan.impact();
    assert_eq!(0, impact.converted_rows, "変換される値は無い");
    assert_eq!(
        1, impact.violating_rows,
        "重複は 1 件の違反として最初に現れた行に属するため、違反を持つ行は 1 行である"
    );
    assert_eq!(0, impact.losing_rows, "削除される列は無い");
    assert!(impact.losses.is_empty(), "失われる値は無い");

    // 計画が計算したとおりに書き戻し（タスク 7.3 の適用に相当）、全件検証と突き合わせる。
    let columns = plan.columns().to_vec();
    let computed: Vec<(RowId, Vec<CellValue>)> = plan
        .rows()
        .map(|(row, values)| (row, values.to_vec()))
        .collect();
    doc.set_sheet_columns(sheet, columns)
        .expect("標本のシートがある");
    for (row, values) in computed {
        doc.set_row_values(sheet, row, values)
            .expect("標本の行がある");
    }
    doc.set_root_schema(
        sheet,
        SchemaPart::parse(&envelope(TO_TEXT)).expect("エンベロープは妥当"),
    )
    .expect("標本のシートがある");

    let compiled = compile(
        doc.sheet_by_id(sheet).expect("標本のシートがある"),
        &TypeRegistry::new(),
    )
    .expect("新しい宣言はコンパイルできる");
    let report = validate_sheet(&doc, sheet, &compiled, &ValidationOptions::default());

    assert_eq!(
        impact.violating_rows,
        report.invalid_rows().len(),
        "集計の違反行数が適用後の全件検証と一致する"
    );
    assert_eq!(1, report.total_violations(), "違反は重複の 1 件である");
    assert!(
        report.violations().iter().all(|violation| matches!(
            violation.reason(),
            ViolationReason::Duplicate { rows, .. } if rows.len() == 2
        )),
        "違反の理由は重複であり、重複する 2 行を挙げる"
    );
}

/// 集計の後にドキュメントが変わったことをダイジェストが検出する。
///
/// ダイジェストは集計の時点のシートの**行識別子の並びと列名**を刻む（適用前に現在の
/// シートと照合するための値である。タスク 7.2）。
#[test]
fn a_changed_sheet_state_is_detected_by_the_digest() {
    let (mut doc, sheet, _) = sample();
    let to = parse_schema(TO).expect("新しい宣言は解析できる");
    let plan = plan_change(&doc, sheet, &to, &TypeRegistry::new()).expect("計画は組み立てられる");

    let current = doc.sheet_by_id(sheet).expect("標本のシートがある");
    assert_eq!(
        plan.digest(),
        PlanDigest::of(current),
        "変わっていないシートのダイジェストは計画のものと一致する"
    );
    assert!(
        !plan.is_stale(&doc),
        "変わっていないシートは陳腐化していない"
    );

    // 行が増える（行識別子の並びが変わる）。
    let row = doc.add_row(sheet).expect("標本のシートがある");
    doc.set_row_values(sheet, row, vec![text("Z"), int(1), text("x")])
        .expect("標本の行がある");
    assert!(plan.is_stale(&doc), "行が増えたシートは陳腐化している");

    // 列名が変わる。
    let (mut doc, sheet, _) = sample();
    let plan = plan_change(&doc, sheet, &to, &TypeRegistry::new()).expect("計画は組み立てられる");
    doc.set_sheet_columns(
        sheet,
        vec![
            "旧名".to_owned(),
            "消える".to_owned(),
            "数量".to_owned(),
            "増えた".to_owned(),
        ],
    )
    .expect("標本のシートがある");
    assert!(plan.is_stale(&doc), "列名が変わったシートは陳腐化している");
}

/// 解決できない宣言からは計画を組み立てない（集計の前提）。
///
/// 計画は解釈できた宣言の上でしか意味を持たない。実在しない型定義を指す宣言は
/// 宣言の誤りとして拒否され、影響の集計へ進まない。
#[test]
fn a_declaration_that_cannot_be_resolved_is_rejected() {
    const DANGLING: &str = concat!(
        r#"{"columns":["#,
        r#"{"name":"属性","type":{"$ref":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}"#,
        r#"]}"#,
    );

    let (doc, sheet, _) = sample();
    let to = parse_schema(DANGLING).expect("宣言の解析そのものは通る");
    let error = plan_change(&doc, sheet, &to, &TypeRegistry::new())
        .expect_err("実在しない型定義を指す宣言は拒否される");
    assert!(
        matches!(error, SchemaError::DanglingTypeRef { .. }),
        "宣言の誤りとして拒否される"
    );
}

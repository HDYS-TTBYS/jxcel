//! セルの編集の適用（データグリッドのタスク 3.1。data-grid 要件 3.3, 3.4, 3.5, 3.7, 11.4）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のものである。
//!
//! 1. **1 セルの編集の呼び出しの形**（要件 11.4）。判定（`validate_write`）は**1 回**、
//!    編集した行の値を渡して呼ばれ、再検証は**編集した列だけ**を指定して 1 回呼ばれ、
//!    **シート全件の検証は 1 回も呼ばれない**。速度ではなく**呼び出しの形**で固定する
//!    （`verification.md`「速度を証拠にしない。証拠は『呼び出しの形』で取る」）。
//! 2. **呼び出しの形が行数に依らない**（要件 11.4）。行数だけを変えた 2 つの標本で、
//!    縫い目へ届いた呼び出しの並びが同じであることを見る（時間は測らない）。
//! 3. **型に適合しない値が破棄されずドキュメントに残り、違反として報告される**（要件 3.5）。
//!    適用は成功し（`Ok`）、値はドキュメントから読み戻せる。
//! 4. **変換が起きたとき、変換前と変換後の双方が結果に載る**（要件 3.4）。変換後の期待値は
//!    表示文字列の唯一の源（[`display_text`]）から導く（手書きの写しを置かない）。
//! 5. **値なしへ戻せる**（要件 3.7）。空の文字列は値なしとして書かれ、値なしを許す列では
//!    違反にならず、必須の列では違反として報告される（**値なしも破棄されない**）。
//! 6. **同じ打たれた文字が列によって別の道を通る**（要件 3.3）。本層が判定の分岐を持たず、
//!    列の型を見て受理を決めていないことを、同じ入力に対する 2 つの結果で示す。
//! 7. **誤りの 3 経路と部分適用の不在**。未知の行・範囲外の列・使えないスキーマは
//!    [`GridError`] の判別可能な変種として返り、**1 つのセルも書かれず、判定も呼ばれない**。
//!    複数セルの命令の途中で不正が見つかった場合も同じである。
//! 8. **決定性**。同じドキュメントと同じ命令の 2 回の適用は同じ結果を返し、状態も変えない。
//! 9. **`affected` と `row_count`**。`affected` は影響を受けた行を命令に現れた順で重複なく
//!    並べ、`row_count` は適用後の行数である（`SetCells` は行を増減しない）。
//! 10. **空の命令**。セル 0 個の `SetCells` は成功し、何も変えず、**縫い目を 1 回も呼ばない**。
//! 11. **本番の既定の経路**（[`EditApply::new`]）が **`schema-engine` 自身**の判定を出して
//!     いること。既定の経路を、**素通しの縫い目では出せない観測**（変換の記録・変換の前後の
//!     相違・変換後の値・違反の総数）で固定する。加えて、既定の経路と
//!     [`SchemaEngineQuery`] を明示した経路が同じ結果になることも見る（両者の食い違いの
//!     検出であり、上の固定とは役割が違う）。
//! 12. **同じセルを 2 度書く命令**。後ろの文字が残り、判定も**実際に書かれる値**に対して
//!     1 回だけ行われる（判定した値と書いた値が食い違わない）。
//!
//! # 前提を先に確かめる
//!
//! 標本（`tests/common/sample.rs`）を使う検査は、**標本がその検査の前提を満たしていることを
//! 最初に検査する**（tasks.md の Implementation Notes の規則）。本ファイルは
//! [`assert_clean_premises`] で次を確かめてから依拠する。
//!
//! - 違反を 1 件も仕込んでいない（`injected_violations` / `unique_violations` が 0）こと。
//! - 列 0 が**唯一の一意制約つきの列**であり、重複が 1 件も無い（**零の同点**）こと。
//! - **文書の行順が行識別子の順と一致する**こと（標本は行識別子を発行順に並べる）。
//! - 全件検証の報告が 0 件であること（標本は適合する値だけで組み立てられている）。
//!
//! **標本の識別子は発行のたびに変わる**ため、生の識別子や生の値を実行を跨いで比較しない。
//! 比較するのは**構造**（呼び出しの形・列の添字・違反の位置・行数のような数）である
//! （tasks.md の Implementation Notes の規則）。
//!
//! # 数える縫い目
//!
//! 要件 11.4 は「全件検証を呼ばない」ことの表明であり、その証拠は**呼び出しの形**だけである。
//! 本ファイルは [`EditSchemaQuery`] を実装した [`CountingQuery`] を
//! [`EditApply::with_query`] へ差し込み、縫い目へ届いた呼び出しを**到着順の 1 本の記録**として
//! 観測する。記録が 1 本であるため、「判定が 1 回」「再検証の列がこれだけ」「全件検証は 0 回」
//! を**同じ並びの 1 つの表明**で書ける — 計数器を 3 つ並べると、片方を見落とした呼び出しが
//! どの計数器にも現れない余地が残る。
//!
//! 数える先は**本番の実装そのもの**（[`SchemaEngineQuery`]）である。非公開の模擬へ委譲すると、
//! 本番の経路が数えた対象と一致する保証が無くなる（`structure.md`
//! 「一括メソッドを置くだけでは足りない。本番の一括経路からそれが呼ばれていることを示すこと」）。

mod common;

use std::sync::{Arc, Mutex};

use common::sample::sample;
use common::sample::{SampleEditParts, SampleOptions};
use data_grid::{
    display_text, CellAddress, CoercionNotice, ColumnIndex, EditApply, EditCommand, EditSchemaQuery,
    GridError, SchemaEngineQuery,
};
use document_format::{CellValue, Document, RowId, SchemaPart, SheetId};
use schema_engine::{
    schema_to_text, validate_columns, validate_sheet, ColumnDecl, CompiledSchema, Constraints,
    DeclaredKind, EditVerdict, Schema, SchemaEngine, SchemaEngineApi, SheetReport, TypeDecl,
    TypeKind, TypeRegistry, ValidationOptions,
};

// ---------------------------------------------------------------------------
// 縫い目へ届いた呼び出しの記録
// ---------------------------------------------------------------------------

/// 縫い目へ届いた 1 回の呼び出し（到着順に並べる）。
///
/// 3 変種は縫い目の 3 つの操作に 1 対 1 で対応し、**呼ばれたものをそのまま記録する**
/// （記録から漏れる呼び出しが無いことが「全件検証は 0 回」の表明の前提である）。
#[derive(Debug, Clone, PartialEq)]
enum QueryCall {
    /// 書き込みの判定。渡った 1 行分の値を運ぶ。
    JudgeWrite(Vec<CellValue>),
    /// 列を限定した再検証。渡った列の集合を運ぶ。
    RevalidateColumns(Vec<ColumnIndex>),
    /// シート全件の検証。**編集経路はこれを呼んではならない**（要件 11.4）。
    ValidateSheet,
}

/// `schema-engine` の判定を**数える**縫い目。
///
/// 本番の縫い目（[`SchemaEngineQuery`]）へ**そのまま委譲**し、委譲の前に呼び出しを記録する。
/// したがって記録は「本番が実際に何を呼んだか」であり、判定の結果は本番と同一である。
struct CountingQuery {
    /// 委譲先（本番の縫い目）。
    inner: SchemaEngineQuery,
    /// 到着順の記録（判定の後に読むため共有する。縫い目の状態は引数にも返り値にも現れない
    /// ため、記録は内部可変性で持つ）。
    calls: Arc<Mutex<Vec<QueryCall>>>,
}

impl EditSchemaQuery for CountingQuery {
    fn judge_write(&self, schema: &CompiledSchema, values: Vec<CellValue>) -> EditVerdict {
        self.record(QueryCall::JudgeWrite(values.clone()));
        self.inner.judge_write(schema, values)
    }

    fn revalidate_columns(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        columns: &[ColumnIndex],
        options: &ValidationOptions,
    ) -> SheetReport {
        self.record(QueryCall::RevalidateColumns(columns.to_vec()));
        self.inner
            .revalidate_columns(doc, sheet, schema, columns, options)
    }

    fn validate_sheet(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        options: &ValidationOptions,
    ) -> SheetReport {
        self.record(QueryCall::ValidateSheet);
        self.inner.validate_sheet(doc, sheet, schema, options)
    }
}

impl CountingQuery {
    /// 呼び出しを 1 件記録する。
    fn record(&self, call: QueryCall) {
        self.calls.lock().expect("記録の錠は毒されない").push(call);
    }
}

/// 数える縫い目を差し込んだ適用の経路と、その記録を返す。
fn counting(sheet: SheetId, schema: CompiledSchema) -> (EditApply, Arc<Mutex<Vec<QueryCall>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let apply = EditApply::with_query(
        sheet,
        schema,
        Box::new(CountingQuery {
            inner: SchemaEngineQuery,
            calls: Arc::clone(&calls),
        }),
    );
    (apply, calls)
}

/// ここまでに縫い目へ届いた呼び出しの並び（到着順）。
fn recorded(calls: &Arc<Mutex<Vec<QueryCall>>>) -> Vec<QueryCall> {
    calls.lock().expect("記録の錠は毒されない").clone()
}

/// 呼び出しの**形**だけを取り出す（渡った値の中身は含めない）。
///
/// 行数の違う 2 つの標本の呼び出しの形を突き合わせるために使う。渡った値は標本ごとに中身が
/// 違う（生の値を実行を跨いで比較しない規則）が、**回数と渡った対象の数**は行数に依らない
/// 主張の対象である。
fn call_shape(calls: &[QueryCall]) -> Vec<String> {
    calls
        .iter()
        .map(|call| match call {
            QueryCall::JudgeWrite(values) => format!("judge_write(値{}件)", values.len()),
            QueryCall::RevalidateColumns(columns) => format!(
                "revalidate_columns(列{:?})",
                columns
                    .iter()
                    .map(|column| column.index())
                    .collect::<Vec<_>>()
            ),
            QueryCall::ValidateSheet => "validate_sheet".to_owned(),
        })
        .collect()
}

/// 全件検証が 1 回も呼ばれていないことを表明する（要件 11.4）。
fn assert_no_sheet_wide_validation(calls: &Arc<Mutex<Vec<QueryCall>>>) {
    assert!(
        !recorded(calls).contains(&QueryCall::ValidateSheet),
        "編集経路はシート全件の検証を呼ばない"
    );
}

// ---------------------------------------------------------------------------
// 標本と前提
// ---------------------------------------------------------------------------

/// 編集に要る部品へ分解した標本。
struct Fixture {
    /// 標本の部品（文書・シート・行識別子・列名）。
    parts: SampleEditParts,
    /// 開いた計画。
    plan: CompiledSchema,
}

impl Fixture {
    /// 違反を 1 件も仕込まない標本（呼び出しの形と値の保持の検査は、仕込んだ違反と混ざると
    /// 数えられないため）。
    fn clean(rows: usize, columns: usize) -> Self {
        let sample = sample(&SampleOptions::new(rows, columns).with_ratio(0.0));
        assert_clean_premises(&sample);
        // 宣言と計画は**分解の前**に取り出す（分解の後は標本が文書を手放す）。
        let plan = sample.compiled();
        Self {
            parts: sample.into_edit_parts(),
            plan,
        }
    }

    fn sheet(&self) -> SheetId {
        self.parts.sheet
    }

    fn document(&self) -> &Document {
        &self.parts.document
    }

    fn document_mut(&mut self) -> &mut Document {
        &mut self.parts.document
    }

    fn row(&self, index: usize) -> RowId {
        self.parts.row_ids[index]
    }

    fn row_count(&self) -> usize {
        self.parts.row_ids.len()
    }

    /// 標本の列名。
    fn columns(&self) -> &[String] {
        &self.parts.columns
    }

    /// 行 `row` の列 `column` の値をドキュメントから読み戻す。
    fn value_at(&self, row: RowId, column: usize) -> CellValue {
        value_at(self.document(), self.sheet(), row, column)
    }

    /// ドキュメントの行の値（適用の前後の比較用）。
    fn snapshot(&self) -> (Vec<RowId>, Vec<Vec<CellValue>>) {
        snapshot(self.document(), self.sheet())
    }

    /// 行 `row` の値を、列 `column` だけ `value` に置き換えた並び（判定へ渡る値の期待値）。
    fn row_with(&self, row: RowId, column: usize, value: CellValue) -> Vec<CellValue> {
        row_with(self.document(), self.sheet(), row, column, value)
    }
}

/// 標本を使う検査の前提を先に確かめる（モジュール docs「前提を先に確かめる」）。
fn assert_clean_premises(sample: &common::sample::Sample) {
    assert_eq!(0, sample.injected_violations(), "違反を仕込んでいない標本");
    assert_eq!(0, sample.unique_violations(), "一意制約の重複を混ぜていない標本");

    let plan = sample.compiled();
    // 列 0（品番）が標本で唯一の一意制約つきの列であり、同点が零であること。
    assert_eq!(
        vec![ColumnIndex::new(0)],
        plan.unique_columns().to_vec(),
        "標本の一意制約つきの列は列 0 だけ"
    );
    assert_eq!(
        sample.column_count(),
        plan.column_count(),
        "計画の列数は標本の列数"
    );

    // 文書の行順が行識別子の順と一致すること（標本は発行順に並べる）。
    assert_eq!(
        sample.row_ids().to_vec(),
        document_ids(sample.document(), sample.sheet()),
        "文書の行順は行識別子の順"
    );

    // 標本は適合する値だけで組み立てられている（全件検証の報告が 0 件）。
    let report = validate_sheet(
        sample.document(),
        sample.sheet(),
        &plan,
        &ValidationOptions::unlimited(),
    );
    assert_eq!(0, report.total_violations(), "標本は違反を 1 件も持たない");
}

/// ドキュメントの行識別子を文書の順に取り出す。
fn document_ids(document: &Document, sheet: SheetId) -> Vec<RowId> {
    document
        .sheet_by_id(sheet)
        .expect("シートは文書にある")
        .rows()
        .iter()
        .map(|row| row.id())
        .collect()
}

/// ドキュメントの行の値（適用の前後の比較用）。
fn snapshot(document: &Document, sheet: SheetId) -> (Vec<RowId>, Vec<Vec<CellValue>>) {
    let sheet_ref = document.sheet_by_id(sheet).expect("シートは文書にある");
    (
        sheet_ref.rows().iter().map(|row| row.id()).collect(),
        sheet_ref
            .rows()
            .iter()
            .map(|row| row.values().to_vec())
            .collect(),
    )
}

/// 行 `row` の列 `column` の値をドキュメントから読み戻す。
fn value_at(document: &Document, sheet: SheetId, row: RowId, column: usize) -> CellValue {
    document
        .sheet_by_id(sheet)
        .expect("シートは文書にある")
        .rows()
        .iter()
        .find(|found| found.id() == row)
        .expect("行はシートにある")
        .values()
        .get(column)
        .cloned()
        .unwrap_or(CellValue::Null)
}

/// 行の値を、指定した列だけ置き換えた並び（判定へ渡る値の期待値）。
fn row_with(
    document: &Document,
    sheet: SheetId,
    row: RowId,
    column: usize,
    value: CellValue,
) -> Vec<CellValue> {
    let mut values = document
        .sheet_by_id(sheet)
        .expect("シートは文書にある")
        .rows()
        .iter()
        .find(|found| found.id() == row)
        .expect("行はシートにある")
        .values()
        .to_vec();
    values[column] = value;
    values
}

/// 1 セルを書く命令。
fn set_one(row: RowId, column: usize, text: &str) -> EditCommand {
    EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(row, ColumnIndex::new(column)),
            text.to_owned(),
        )],
    }
}

// ---------------------------------------------------------------------------
// 1. 呼び出しの形（要件 11.4 の核）
// ---------------------------------------------------------------------------

/// 1 セルの編集は、判定を 1 回（その行の値を渡して）呼び、**編集した列だけ**を再検証し、
/// 全件検証は 1 回も呼ばない（要件 11.4）。
#[test]
fn a_single_cell_edit_judges_once_and_revalidates_only_the_edited_column() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let column = 1;
    // 判定へ渡る値は「その行の値を、その列だけ置き換えたもの」である（上流の
    // `validate_write` は 1 行分の値を受け取り、値の添字を列の添字として読む）。
    let judged = fixture.row_with(row, column, CellValue::Text("7".to_owned()));
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(fixture.document_mut(), set_one(row, column, "7"))
        .expect("適合する値の書き込みは成功する");

    // **記録の全体**が「判定 1 回 + 当該列だけの再検証 1 回」である（全件検証は現れない）。
    assert_eq!(
        vec![
            QueryCall::JudgeWrite(judged),
            QueryCall::RevalidateColumns(vec![ColumnIndex::new(column)]),
        ],
        recorded(&calls),
        "1 セルの編集の呼び出しは判定 1 回と当該列の再検証 1 回だけ"
    );
    assert_no_sheet_wide_validation(&calls);

    // 判定の結果は写され、行は影響を受けたものとして報告される。
    assert_eq!(vec![row], outcome.affected);
    assert_eq!(CellValue::Int(7), fixture.value_at(row, column));
    assert_eq!(0, outcome.violation_total, "適合する値は違反を生まない");
}

/// 1 セルの編集の呼び出しの形は**行数に依らない**（要件 11.4。時間ではなく形で示す）。
#[test]
fn the_call_shape_of_a_single_cell_edit_does_not_depend_on_the_row_count() {
    let mut shapes = Vec::new();
    let mut judged_value_counts = Vec::new();

    for rows in [64, 512] {
        let mut fixture = Fixture::clean(rows, 13);
        let sheet = fixture.sheet();
        let row = fixture.row(0);
        let (mut apply, calls) = counting(sheet, fixture.plan.clone());

        apply
            .apply(fixture.document_mut(), set_one(row, 1, "7"))
            .expect("適合する値の書き込みは成功する");

        let log = recorded(&calls);
        shapes.push(call_shape(&log));
        judged_value_counts.push(match &log[0] {
            QueryCall::JudgeWrite(values) => values.len(),
            other => panic!("最初の呼び出しが判定でない: {other:?}"),
        });
        assert_no_sheet_wide_validation(&calls);
    }

    assert_eq!(
        vec![
            vec![
                "judge_write(値13件)".to_owned(),
                "revalidate_columns(列[1])".to_owned(),
            ];
            2
        ],
        shapes,
        "行数だけを変えても、呼び出しの回数と対象の数は同じ"
    );
    assert_eq!(
        judged_value_counts,
        vec![13, 13],
        "判定へ渡るのは 1 行分の値であり、行数に比例しない"
    );
}

// ---------------------------------------------------------------------------
// 2. 適合しない値の保持（要件 3.5）
// ---------------------------------------------------------------------------

/// 列の型に適合しない値は破棄されずドキュメントに残り、違反として報告される（要件 3.5）。
#[test]
fn a_value_that_does_not_fit_is_kept_in_the_document_and_reported() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    // 列 4（検査済み）は `bool`。`yes` は規則表の変換（`true` / `false` の 2 綴りだけ）に
    // 無いため変換されず、適合しない値として残る。
    let column = 4;
    let judged = fixture.row_with(row, column, CellValue::Text("yes".to_owned()));
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(fixture.document_mut(), set_one(row, column, "yes"))
        .expect("編集経路は拒否しない（違反があっても成功する）");

    assert_eq!(vec![row], outcome.affected, "影響を受けた行");
    assert_eq!(1, outcome.violation_total, "違反の総数（再検証した列に閉じる）");
    assert!(outcome.coercions.is_empty(), "変換は起きていない");

    // **値は破棄されず、ドキュメントにそのまま残る**（要件 3.5）。
    assert_eq!(
        CellValue::Text("yes".to_owned()),
        fixture.value_at(row, column),
        "適合しない値もドキュメントに残る"
    );

    // 違反の位置と列名は再検証の報告から確かめる（本層は報告から総数だけを写す）。
    let report = validate_columns(
        fixture.document(),
        sheet,
        &fixture.plan,
        &[ColumnIndex::new(column)],
        &ValidationOptions::unlimited(),
    );
    assert_eq!(1, report.total_violations(), "当該列の違反は 1 件");
    let violation = &report.violations()[0];
    assert_eq!(Some(row), violation.row(), "違反の行");
    assert_eq!(ColumnIndex::new(column), violation.column(), "違反の列");
    assert_eq!(
        fixture.columns()[column],
        violation.column_name(),
        "違反の列名"
    );

    assert_eq!(
        vec![
            QueryCall::JudgeWrite(judged),
            QueryCall::RevalidateColumns(vec![ColumnIndex::new(column)]),
        ],
        recorded(&calls),
        "違反があっても呼び出しの形は変わらない"
    );
}

/// 必須の列を空にすると、値なしがドキュメントに残り、違反として報告される（要件 3.5, 3.7）。
#[test]
fn clearing_a_required_cell_keeps_the_absence_and_reports_it() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    // 列 1（数量）は必須である（値なしは違反になる）。
    let column = 1;
    assert!(
        fixture.plan.required(ColumnIndex::new(column)),
        "標本の列 1 は必須"
    );

    let mut apply = EditApply::new(sheet, fixture.plan.clone());
    let outcome = apply
        .apply(fixture.document_mut(), set_one(row, column, ""))
        .expect("空の文字列の書き込みも成功する");

    assert_eq!(1, outcome.violation_total, "必須の列の値なしは違反");
    assert_eq!(
        CellValue::Null,
        fixture.value_at(row, column),
        "値なしがドキュメントに残る"
    );
    assert!(outcome.coercions.is_empty(), "値なしは変換ではない");
}

// ---------------------------------------------------------------------------
// 3. 変換の記録（要件 3.4）
// ---------------------------------------------------------------------------

/// 変換が起きたとき、変換の前と後の双方が結果に載り、ドキュメントには変換後の値が入る
/// （要件 3.4）。
#[test]
fn a_coerced_value_is_reported_with_its_before_and_after() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    // 列 1（数量）は `int`。`42` は規則表の `Text` → `int` の行に一致し、変換される。
    let column = 1;
    let row_count = fixture.row_count();
    let (mut apply, _calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(fixture.document_mut(), set_one(row, column, "42"))
        .expect("変換を伴う書き込みも成功する");

    assert_eq!(vec![row], outcome.affected, "影響を受けた行");
    assert_eq!(row_count, outcome.row_count, "SetCells は行数を変えない");
    assert_eq!(0, outcome.violation_total, "変換後の値は適合する");
    assert_eq!(
        vec![CoercionNotice {
            cell: CellAddress::new(row, ColumnIndex::new(column)),
            before: "42".to_owned(),
            // 変換後の期待値は表示文字列の唯一の源（`display_text`）から導く。
            after: display_text(&CellValue::Int(42)).into_owned(),
        }],
        outcome.coercions,
        "変換の前と後の双方が載る"
    );
    assert_eq!(
        CellValue::Int(42),
        fixture.value_at(row, column),
        "ドキュメントには変換後の値が入る"
    );
}

// ---------------------------------------------------------------------------
// 4. 値なしへ戻す（要件 3.7）
// ---------------------------------------------------------------------------

/// 値なしを許す列では、値を消して値なしへ戻せる（要件 3.7）。
#[test]
fn clearing_an_optional_cell_writes_the_absence_of_a_value() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    // 列 3（予備率）は必須ではない（値なしが適合する）。
    let column = 3;
    assert!(
        !fixture.plan.required(ColumnIndex::new(column)),
        "標本の列 3 は必須ではない"
    );

    let mut apply = EditApply::new(sheet, fixture.plan.clone());

    // まず値を入れ、次に消す（消す前の状態が値なしでないことを先に確かめる）。
    apply
        .apply(fixture.document_mut(), set_one(row, column, "7.5"))
        .expect("適合する値の書き込みは成功する");
    assert_ne!(
        CellValue::Null,
        fixture.value_at(row, column),
        "消す前は値がある"
    );

    let outcome = apply
        .apply(fixture.document_mut(), set_one(row, column, ""))
        .expect("値を消す書き込みも成功する");

    assert_eq!(
        CellValue::Null,
        fixture.value_at(row, column),
        "空の文字列は値なしとして書かれる"
    );
    assert_eq!(
        0,
        outcome.violation_total,
        "値なしを許す列では違反にならない"
    );
    assert_eq!(vec![row], outcome.affected, "消した行も影響を受けた行");
}

// ---------------------------------------------------------------------------
// 5. 判定の分岐を持たない（要件 3.3）
// ---------------------------------------------------------------------------

/// 同じ打たれた文字が、列の型によって別の道を通る（本層は列の型を見て受理を決めていない）。
#[test]
fn the_same_text_takes_two_different_paths_because_the_columns_differ() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let quantity = 1; // `int`。`42` は変換される。
    let checked = 4; // `bool`。`42` は変換されず、違反として残る。
    let (mut apply, _calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::SetCells {
                cells: vec![
                    (
                        CellAddress::new(row, ColumnIndex::new(quantity)),
                        "42".to_owned(),
                    ),
                    (
                        CellAddress::new(row, ColumnIndex::new(checked)),
                        "42".to_owned(),
                    ),
                ],
            },
        )
        .expect("どちらの列への書き込みも成功する");

    // 同じ文字列が、`int` の列では変換され、`bool` の列では変換されずに残る。**この違いを
    // 決めているのは `schema-engine` の規則表だけである**（本層は列の型を見ない）。
    assert_eq!(
        CellValue::Int(42),
        fixture.value_at(row, quantity),
        "`int` の列では変換後の値"
    );
    assert_eq!(
        CellValue::Text("42".to_owned()),
        fixture.value_at(row, checked),
        "`bool` の列では打たれた文字のまま"
    );
    assert_eq!(1, outcome.violation_total, "違反は `bool` の列の 1 件だけ");
    assert_eq!(vec![row], outcome.affected, "同じ行を 2 セル書いても行は 1 つ");
    assert_eq!(1, outcome.coercions.len(), "変換の記録は起きた 1 件だけ");
    assert_eq!(
        quantity,
        outcome.coercions[0].cell.column().index(),
        "変換の記録は `int` の列を指す"
    );
}

// ---------------------------------------------------------------------------
// 6. 誤りの経路と部分適用の不在
// ---------------------------------------------------------------------------

/// 他シートの行を指す命令は [`GridError::UnknownRow`] を返し、**何も書かない**。
#[test]
fn an_unknown_row_stops_before_any_write() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    // 標本の参照先シートの行はデータシートに属さない（上流の `CellWriteError::UnknownRow` と
    // 同じ意味の「他シートの行」である）。
    let foreign = fixture
        .document()
        .sheet_by_id(fixture.parts.reference)
        .expect("参照先シートは文書にある")
        .rows()[0]
        .id();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before = fixture.snapshot();

    let error = apply
        .apply(fixture.document_mut(), set_one(foreign, 1, "7"))
        .expect_err("他シートの行への書き込みは失敗する");

    assert_eq!(GridError::UnknownRow { row: foreign }, error);
    assert_eq!(before, fixture.snapshot(), "1 つのセルも書かれない");
    assert_eq!(Vec::<QueryCall>::new(), recorded(&calls), "判定も呼ばれない");
}

/// 列の数の外を指す命令は [`GridError::ColumnOutOfRange`] を返し、**何も書かない**。
#[test]
fn an_out_of_range_column_stops_before_any_write() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let column = fixture.columns().len();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before = fixture.snapshot();

    let error = apply
        .apply(fixture.document_mut(), set_one(row, column, "7"))
        .expect_err("範囲外の列への書き込みは失敗する");

    assert_eq!(
        GridError::ColumnOutOfRange {
            column: ColumnIndex::new(column),
            count: fixture.columns().len(),
        },
        error
    );
    assert_eq!(before, fixture.snapshot(), "1 つのセルも書かれない");
    assert_eq!(Vec::<QueryCall>::new(), recorded(&calls), "判定も呼ばれない");
}

/// 複数のセルのうち 1 つでも不正なら、**どのセルも書かない**（部分適用が無い）。
#[test]
fn a_command_with_one_invalid_cell_writes_nothing() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let foreign = fixture
        .document()
        .sheet_by_id(fixture.parts.reference)
        .expect("参照先シートは文書にある")
        .rows()[0]
        .id();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before = fixture.snapshot();

    let error = apply
        .apply(
            fixture.document_mut(),
            EditCommand::SetCells {
                cells: vec![
                    (CellAddress::new(row, ColumnIndex::new(1)), "7".to_owned()),
                    (
                        CellAddress::new(foreign, ColumnIndex::new(3)),
                        "1.5".to_owned(),
                    ),
                ],
            },
        )
        .expect_err("1 つでも不正なら失敗する");

    assert_eq!(GridError::UnknownRow { row: foreign }, error);
    assert_eq!(
        before,
        fixture.snapshot(),
        "先のセルも書かれない（部分適用が無い）"
    );
    assert_eq!(Vec::<QueryCall>::new(), recorded(&calls), "判定も呼ばれない");
}

/// 列が 1 件も宣言されていないシートは編集できない（[`GridError::SchemaUnusable`]）。
#[test]
fn a_sheet_without_columns_cannot_be_edited() {
    let mut document = Document::new();
    let sheet = document.add_sheet("空のシート");
    document
        .set_sheet_columns(sheet, Vec::new())
        .expect("列を設定できない");
    document
        .set_root_schema(sheet, SchemaPart::empty())
        .expect("宣言を設置できない");
    let plan = SchemaEngine::new()
        .compile(
            document.sheet_by_id(sheet).expect("シートは文書にある"),
            &TypeRegistry::new(),
        )
        .expect("空の宣言はコンパイルできる");
    assert_eq!(0, plan.column_count(), "空の宣言は列 0 本の計画になる");

    let (mut apply, calls) = counting(sheet, plan);
    let before = snapshot(&document, sheet);
    // 行識別子は正準テキスト形から作る（この経路はシートの前提で止まるため、行の実在は問わない）。
    let row = "00000000000000000000000000"
        .parse::<RowId>()
        .expect("正準の行識別子が解析できない");
    let error = apply
        .apply(&mut document, set_one(row, 0, "7"))
        .expect_err("列 0 本のシートは編集できない");

    assert_eq!(GridError::SchemaUnusable { sheet }, error);
    assert_eq!(before, snapshot(&document, sheet), "何も変わらない");
    assert_eq!(Vec::<QueryCall>::new(), recorded(&calls), "判定も呼ばれない");
}

/// シートの列数と食い違う計画は使えない（列の添字がドキュメントの列名と対応しない）。
#[test]
fn a_plan_that_does_not_match_the_sheet_is_unusable() {
    // 1 列だけの別の文書から計画を作る（この計画は 3 列のシートには対応しない）。
    let mut other = Document::new();
    let other_sheet = other.add_sheet("別のシート");
    other
        .set_sheet_columns(other_sheet, vec!["数量".to_owned()])
        .expect("列を設定できない");
    other
        .set_root_schema(other_sheet, one_column_declaration())
        .expect("宣言を設置できない");
    let mismatched = SchemaEngine::new()
        .compile(
            other.sheet_by_id(other_sheet).expect("シートは文書にある"),
            &TypeRegistry::new(),
        )
        .expect("宣言はコンパイルできる");
    assert_eq!(1, mismatched.column_count(), "1 列の計画ができる");

    let mut document = Document::new();
    let sheet = document.add_sheet("3 列のシート");
    document
        .set_sheet_columns(sheet, vec!["A".to_owned(), "B".to_owned(), "C".to_owned()])
        .expect("列を設定できない");
    let row = document.add_row(sheet).expect("行を追加できない");
    document
        .set_row_values(sheet, row, vec![CellValue::Null; 3])
        .expect("値を設定できない");

    let mut apply = EditApply::new(sheet, mismatched);
    let before = snapshot(&document, sheet);
    let error = apply
        .apply(&mut document, set_one(row, 0, "1"))
        .expect_err("列数の食い違う計画は使えない");

    assert_eq!(GridError::SchemaUnusable { sheet }, error);
    assert_eq!(before, snapshot(&document, sheet), "何も変わらない");
}

/// 1 列（`数量`。`int`）だけの宣言を、上流が保持するエンベロープとして組み立てる。
fn one_column_declaration() -> SchemaPart {
    let schema = Schema {
        columns: vec![ColumnDecl {
            name: "数量".into(),
            ty: TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Int),
                constraints: Constraints::default(),
            },
            required: false,
            unique: false,
            default: None,
            description: None,
        }],
    };
    let root = schema_to_text(&schema).expect("宣言は正準出力できる");
    SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#)).expect("宣言は解析できる")
}

// ---------------------------------------------------------------------------
// 7. 決定性・affected・row_count・空の命令
// ---------------------------------------------------------------------------

/// 同じドキュメントと同じ命令の 2 回の適用は同じ結果を返し、状態も変えない。
#[test]
fn applying_the_same_command_twice_gives_the_same_outcome() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(1);
    let command = set_one(row, 1, "42");
    let mut apply = EditApply::new(sheet, fixture.plan.clone());

    let first = apply
        .apply(fixture.document_mut(), command.clone())
        .expect("1 回目の適用は成功する");
    let after_first = fixture.snapshot();
    let second = apply
        .apply(fixture.document_mut(), command)
        .expect("2 回目の適用も成功する");

    assert_eq!(first, second, "同じ命令の 2 回の適用は同じ結果");
    assert_eq!(after_first, fixture.snapshot(), "状態も変わらない");
    // 識別子は再発行されない（`SetCells` は行を増減しない）。
    assert_eq!(
        fixture.parts.row_ids,
        document_ids(fixture.document(), sheet),
        "行識別子と並びは変わらない"
    );
}

/// 同じセルを 2 度書く命令は、後ろのものを残す 1 つの編集として扱う。
///
/// 上流の一括経路（`Document::set_cells`）の last-wins と同じ規則であり、判定へ渡る値も
/// **実際に書かれる値**である（判定した値と書いた値が食い違わない）。
#[test]
fn a_repeated_cell_keeps_the_last_text_and_judges_only_that_value() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let column = 1;
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::SetCells {
                cells: vec![
                    (CellAddress::new(row, ColumnIndex::new(column)), "7".to_owned()),
                    (CellAddress::new(row, ColumnIndex::new(column)), "9".to_owned()),
                ],
            },
        )
        .expect("同じセルを 2 度書く命令も成功する");

    assert_eq!(vec![row], outcome.affected, "行は 1 回だけ報告される");
    assert_eq!(
        CellValue::Int(9),
        fixture.value_at(row, column),
        "後ろの文字が残る"
    );
    // 判定へ渡る値も後ろの文字（実際に書かれる値）である。
    assert_eq!(
        vec![
            QueryCall::JudgeWrite(fixture.row_with(row, column, CellValue::Text("9".to_owned()))),
            QueryCall::RevalidateColumns(vec![ColumnIndex::new(column)]),
        ],
        recorded(&calls),
        "判定は 1 回、実際に書かれる値に対して行われる"
    );
}

/// `affected` は影響を受けた行を命令に現れた順で重複なく並べ、`row_count` は適用後の行数である。
#[test]
fn affected_lists_each_row_once_and_row_count_is_the_count_after_the_edit() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let first = fixture.row(0);
    let second = fixture.row(1);
    let row_count = fixture.row_count();
    // 判定の入力は命令の**直前**の状態である（先に書いたセルの値を後の判定が見ない）。
    // 判定は**編集の対象になった行ごとに 1 回**であり、行の順は命令に現れた順である。
    let judged_second = fixture.row_with(second, 1, CellValue::Text("7".to_owned()));
    let mut judged_second_both = judged_second.clone();
    judged_second_both[3] = CellValue::Text("2.5".to_owned());
    let judged_first = fixture.row_with(first, 3, CellValue::Text("1.5".to_owned()));
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::SetCells {
                cells: vec![
                    (CellAddress::new(second, ColumnIndex::new(1)), "7".to_owned()),
                    (CellAddress::new(first, ColumnIndex::new(3)), "1.5".to_owned()),
                    (CellAddress::new(second, ColumnIndex::new(3)), "2.5".to_owned()),
                ],
            },
        )
        .expect("3 セルの書き込みは成功する");

    assert_eq!(
        vec![second, first],
        outcome.affected,
        "命令に現れた順、重複は最初の 1 回だけ"
    );
    assert_eq!(row_count, outcome.row_count, "SetCells は行数を変えない");
    assert_eq!(
        fixture.parts.row_ids,
        document_ids(fixture.document(), sheet),
        "行の並びも変わらない"
    );
    // 判定は**行ごとに 1 回**（2 行だから 2 回）であり、同じ行の 2 つのセルは同じ 1 回の
    // 判定に載る。再検証は編集した列の集合を 1 回で受ける。
    assert_eq!(
        vec![
            QueryCall::JudgeWrite(judged_second_both),
            QueryCall::JudgeWrite(judged_first),
            QueryCall::RevalidateColumns(vec![ColumnIndex::new(1), ColumnIndex::new(3)]),
        ],
        recorded(&calls),
        "判定は編集した行ごとに 1 回、再検証は編集した列の集合を 1 回"
    );
    assert_eq!(
        CellValue::float(2.5),
        fixture.value_at(second, 3),
        "同じ行の 2 つのセルがどちらも書かれる（`float` の列なので変換後の値）"
    );
}

/// セル 0 個の命令は成功し、何も変えず、縫い目を 1 回も呼ばない。
#[test]
fn an_empty_command_changes_nothing_and_calls_nothing() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row_count = fixture.row_count();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before = fixture.snapshot();

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::SetCells { cells: Vec::new() },
        )
        .expect("空の命令は成功する");

    assert_eq!(Vec::<RowId>::new(), outcome.affected, "影響を受けた行は無い");
    assert_eq!(Vec::<CoercionNotice>::new(), outcome.coercions, "変換も無い");
    assert_eq!(0, outcome.violation_total, "違反の総数も 0");
    assert_eq!(row_count, outcome.row_count, "行数は変わらない");
    assert_eq!(before, fixture.snapshot(), "何も変わらない");
    assert_eq!(
        Vec::<QueryCall>::new(),
        recorded(&calls),
        "縫い目を 1 回も呼ばない"
    );
    assert_no_sheet_wide_validation(&calls);
}

// ---------------------------------------------------------------------------
// 8. 本番の既定の縫い目
// ---------------------------------------------------------------------------

/// 本番の既定の経路（[`EditApply::new`]）が **`schema-engine` 自身**の判定を出していること。
///
/// この検査が固定するのは「既定の経路と、[`SchemaEngineQuery`] を明示した経路が同じである」
/// こと**ではない**（それは両方の腕が同じ実装を使うため、`new` が別の実装へ委ねていても
/// 通ってしまう）。固定するのは、**素通しの縫い目では出せない観測**である:
///
/// 1. 変換が起きたこと（[`CoercionNotice`] が 1 件載る）。素通しの実装は変換しないため、
///    記録が空になる。
/// 2. 変換の**前と後が異なる**こと（`"007"` → `Int(7)`）。素通しなら `before` と `after` が
///    同じ綴りになる。
/// 3. ドキュメントに入るのが**変換後の値**（[`CellValue::Int`]）であること。素通しなら
///    打たれた文字（[`CellValue::Text`]）のまま残る。
/// 4. 適合しない値が**違反として数えられる**こと（1 件）。素通しの実装が空の報告を返せば
///    0 件になる。
///
/// 入力は `crates/schema-engine/src/coerce/mod.rs` の規則表が変換を保証する組（`Text` →
/// `int`。`"007"` は先頭の 0 を保つ綴りであり、変換後の表示 `"7"` と異なる）と、変換されない
/// 組（`Text` → `bool` は `true` / `false` の 2 綴りだけ）を同時に使う。
#[test]
fn the_default_path_produces_the_engines_own_conversions_and_violations() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let quantity = 1; // `int`。`"007"` は変換される（先頭の 0 は情報ではない）。
    let checked = 4; // `bool`。`"yes"` は変換されず、違反として残る。
    // 既定の経路（本番の構築）だけを使う。
    let mut apply = EditApply::new(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::SetCells {
                cells: vec![
                    (
                        CellAddress::new(row, ColumnIndex::new(quantity)),
                        "007".to_owned(),
                    ),
                    (
                        CellAddress::new(row, ColumnIndex::new(checked)),
                        "yes".to_owned(),
                    ),
                ],
            },
        )
        .expect("適合しない値があっても成功する");

    // 1・2. 変換が起き、前後の表示が異なる（変換後の表示は `display_text` から導く）。
    assert_eq!(
        vec![CoercionNotice {
            cell: CellAddress::new(row, ColumnIndex::new(quantity)),
            before: "007".to_owned(),
            after: display_text(&CellValue::Int(7)).into_owned(),
        }],
        outcome.coercions,
        "既定の経路が `schema-engine` の変換の記録を写している"
    );
    assert_ne!(
        outcome.coercions[0].before, outcome.coercions[0].after,
        "変換の前後が同じ綴りなら、変換していない（素通し）"
    );
    // 3. ドキュメントに入るのは変換後の値である。
    assert_eq!(
        CellValue::Int(7),
        fixture.value_at(row, quantity),
        "変換後の値が書かれる"
    );
    // 4. 適合しない値は違反として数えられる。
    assert_eq!(
        1, outcome.violation_total,
        "変換されない値は違反として報告される"
    );
    assert_eq!(
        CellValue::Text("yes".to_owned()),
        fixture.value_at(row, checked),
        "適合しない値は破棄されない"
    );
}

/// 既定の経路（[`EditApply::new`]）と [`SchemaEngineQuery`] を明示した経路が**同じ結果**に
/// なることの確認（両者の食い違いを検出する。
/// [`the_default_path_produces_the_engines_own_conversions_and_violations`] が既定の経路の
/// 中身を固定し、本検査が 2 つの腕の一致を固定する — 役割が違う）。
#[test]
fn the_default_path_agrees_with_an_explicit_schema_engine_query() {
    let mut structures = Vec::new();

    for explicit in [false, true] {
        let mut fixture = Fixture::clean(64, 13);
        let sheet = fixture.sheet();
        let row = fixture.row(0);
        let mut apply = if explicit {
            EditApply::with_query(sheet, fixture.plan.clone(), Box::new(SchemaEngineQuery))
        } else {
            EditApply::new(sheet, fixture.plan.clone())
        };

        let outcome = apply
            .apply(fixture.document_mut(), set_one(row, 4, "yes"))
            .expect("適合しない値でも成功する");

        structures.push((
            outcome.affected.len(),
            outcome.violation_total,
            outcome.row_count,
            outcome
                .coercions
                .iter()
                .map(|notice| {
                    (
                        notice.cell.column().index(),
                        notice.before.clone(),
                        notice.after.clone(),
                    )
                })
                .collect::<Vec<_>>(),
            fixture.value_at(row, 4),
        ));
    }

    assert_eq!(
        structures[0], structures[1],
        "既定の経路と `SchemaEngineQuery` を明示した経路は同じ結果になる"
    );
    assert_eq!(1, structures[0].1, "適合しない値は違反として数えられる");
}

/// 同じセルを 2 度書き、**後ろが空の文字列**のときも後ろが残る（値なしへ戻る）。
///
/// 3.1 の last-wins の検査（`a_repeated_cell_keeps_the_last_text_and_judges_only_that_value`）は
/// 後ろが空でない文字列の場合だけを見ている。空の文字列は [`edited_value`] によって値なしへ
/// 写るため、**畳む経路が 2 度目の変換を落としていない**ことは別に固定する必要がある
/// （落としていれば、打たれた `""` が値なしにならず空文字のまま書かれる）。
#[test]
fn a_repeated_cell_whose_last_text_is_empty_writes_the_absence_of_a_value() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let column = 1;
    // 前提: この列は必須であり、値なしが違反として数えられる（値なしが書かれたことの観測）。
    assert!(
        fixture.plan.required(ColumnIndex::new(column)),
        "標本の列 1 は必須"
    );
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::SetCells {
                cells: vec![
                    (CellAddress::new(row, ColumnIndex::new(column)), "7".to_owned()),
                    (CellAddress::new(row, ColumnIndex::new(column)), String::new()),
                ],
            },
        )
        .expect("空の文字列で終わる命令も成功する");

    assert_eq!(
        CellValue::Null,
        fixture.value_at(row, column),
        "後ろの空の文字列が値なしとして書かれる"
    );
    assert_eq!(1, outcome.violation_total, "必須の列の値なしは違反");
    assert_eq!(vec![row], outcome.affected, "行は 1 回だけ報告される");
    // 判定へ渡る値も**実際に書かれる値**（値なし）である。
    assert_eq!(
        vec![
            QueryCall::JudgeWrite(fixture.row_with(row, column, CellValue::Null)),
            QueryCall::RevalidateColumns(vec![ColumnIndex::new(column)]),
        ],
        recorded(&calls),
        "判定は 1 回、実際に書かれる値（値なし）に対して行われる"
    );
}

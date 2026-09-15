//! 入れ子の値の編集の適用（データグリッドのタスク 3.3。data-grid 要件 5.5, 5.7）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のものである。
//!
//! 1. **構造表現として受け取り、構造を保ったまま書き戻す**（要件 5.5）。命令が運ぶのは
//!    `document-format` のセル値の JSON 表現であり、解釈は上流の `from_json_bytes` が行う。
//!    **本層は解析器を持たない** — 本ファイルも JSON の文字列を手で書かず、期待値は上流の
//!    `to_json_bytes` で組み立てる（手書きの写しを置かない）。
//! 2. **入れ子の内側の違反が、内側の位置とともに報告される**（提示は要件 4.5、編集経路は
//!    要件 5.5 / 5.7）。違反の位置は
//!    `schema-engine` の `Violation::path()`（[`ValuePath`](schema_engine::ValuePath)）であり、
//!    本クレートの [`NestedPath`] がその唯一の写しである。**配列の要素位置と、オブジェクトの
//!    フィールド名の双方**を、編集したフィールドそのものと突き合わせる。
//! 3. **構造表現として解釈できない入力は、処理を止める誤りである**（[`GridError::NestedDecode`]）。
//!    JSON として不正な入力・`i64` の範囲外の整数・非有限になる数値リテラルのいずれも止まり、
//!    **1 つのセルも書かず、縫い目も 1 回も呼ばない**。検査の順（セッションの前提 → 表現の
//!    解釈 → 宛先）も、**解釈できない入力と宛先の誤りが同時に成り立つ命令**で固定する。
//! 4. **解釈できた値が入れ子でない場合の取り決め**（本タスクが明示する決定）。入れ子でない値も
//!    **そのまま判定へ渡す** — 入れ子の列へ書けば型の不一致として報告され（値は破棄されない）、
//!    `int` の列へ書けば適合する。どちらを選ぶかを決めるのは `schema-engine` の規則表だけである
//!    （本層は列の型を見ない。3.1 と同じ規律）。
//! 5. **入れ子の値を編集して往復させても、編集していないフィールドが変化しない**（要件 5.5,
//!    5.7）。編集の前後の値を宣言されたフィールドごとに突き合わせる。加えて、**変更前の JSON**
//!    （4.1 の逆命令が保持するもの）で書き戻すと値が 1 つも違わずに戻ることを見る。
//! 6. **1 セルの編集の呼び出しの形**（要件 11.4）。入れ子の編集も **1 セルの編集**であり、
//!    判定（`validate_write`）はその行の値を 1 回だけ受け取り、再検証は**編集した列だけ**を
//!    指定して 1 回呼ばれ、**シート全件の検証は 1 回も呼ばれない**。速度ではなく**呼び出しの形**
//!    で固定する（`verification.md`「速度を証拠にしない。証拠は『呼び出しの形』で取る」）。
//! 7. **変換の記録**（要件 3.4）。構造表現から解釈した値も `schema-engine` の規則表を通るため、
//!    規則表が変換を定める組では変換の**前と後**の双方が結果に載る。
//! 8. **`affected` と `row_count`**。入れ子の編集は行を増減しないため、`row_count` は適用の
//!    前後で変わらず、`affected` は編集した行である。
//! 9. **誤りの経路と部分適用の不在**。未知の行・範囲外の列・使えないスキーマは [`GridError`] の
//!    判別可能な変種として返り、**1 つのセルも書かず、縫い目も 1 回も呼ばれない**。
//! 10. **決定性**。同じドキュメントと同じ命令の 2 回の適用は同じ結果を返し、状態も変えない。
//!
//! # 前提を先に確かめる
//!
//! 標本（`tests/common/sample.rs`）を使う検査は、**標本がその検査の前提を満たしていることを
//! 最初に検査する**（tasks.md の Implementation Notes の規則）。本ファイルは [`Fixture::clean`]
//! で次を確かめてから依拠する。
//!
//! - 違反を 1 件も仕込んでいない（`injected_violations` / `unique_violations` が 0）こと。
//! - 列 0 が**唯一の一意制約つきの列**であり、重複が 1 件も無い（**零の同点**）こと。
//! - **文書の行順が行識別子の順と一致する**こと（標本は行識別子を発行順に並べる）。
//! - 全件検証の報告が 0 件であること（標本は適合する値だけで組み立てられている）。
//!
//! 入れ子の列については、**宣言されたフィールドの名前を `CompiledSchema` から読み直し**
//! （写しを書かない）、標本の値がそのフィールドを持つことを検査してから依拠する。
//!
//! **標本の識別子は発行のたびに変わる**ため、生の識別子や生の値を実行を跨いで比較しない。
//! 比較するのは**構造**（呼び出しの形・列の添字・入れ子の位置・行数のような数）である
//! （tasks.md の Implementation Notes の規則）。
//!
//! # 数える縫い目
//!
//! 要件 11.4 の観測の縫い目（[`EditSchemaQuery`]）を本ファイルも使う。数える先は**本番の実装
//! そのもの**（[`SchemaEngineQuery`]）であり、記録は到着順の 1 本である — 「判定が 1 回」
//! 「再検証の列がこれだけ」「全件検証は 0 回」を同じ並びの 1 つの表明で書ける
//! （`structure.md`「本番の一括経路からそれが呼ばれていることを示すこと」）。

mod common;

use std::sync::{Arc, Mutex};

use common::sample::sample;
use common::sample::{SampleEditParts, SampleOptions};
use data_grid::{
    display_text, CellAddress, CoercionNotice, ColumnIndex, EditApply, EditCommand,
    EditSchemaQuery, GridError, NestedPath, NestedPathSegment, SchemaEngineQuery,
};
use document_format::{
    from_json_bytes, to_json_bytes, CellValue, Document, NestedValue, RowId, SchemaPart, SheetId,
};
use schema_engine::compile::plan::ColumnValidator;
use schema_engine::{
    validate_columns, CompiledSchema, EditVerdict, SchemaEngine, SchemaEngineApi, SheetReport,
    TypeRegistry, ValidationOptions,
};

// ---------------------------------------------------------------------------
// 標本の列の位置（`tests/common/sample.rs` の宣言の並び）
// ---------------------------------------------------------------------------

/// 入れ子のオブジェクト（インライン宣言。フィールドを 5 つ持つ）。
const DESTINATION: usize = 10;
/// 入れ子の配列（要素はインラインのオブジェクト。要素数の範囲つき）。
const LINE_ITEMS: usize = 11;
/// 数量の列（`int`）。入れ子でない値の判定を突き合わせる相手である。
const QUANTITY: usize = 1;

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
    /// 到着順の記録（適用の後に読むため共有する。縫い目の状態は引数にも返り値にも現れない
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

/// 全件検証が 1 回も呼ばれていないことを表明する（要件 11.4）。
fn assert_no_sheet_wide_validation(calls: &Arc<Mutex<Vec<QueryCall>>>) {
    assert!(
        !recorded(calls).contains(&QueryCall::ValidateSheet),
        "編集経路はシート全件の検証を呼ばない"
    );
}

/// 判定へ渡った値の並びを読む（1 回目の呼び出しが判定であることを前提にする）。
fn judged_values(calls: &Arc<Mutex<Vec<QueryCall>>>) -> Vec<CellValue> {
    let log = recorded(calls);
    match log.first() {
        Some(QueryCall::JudgeWrite(values)) => values.clone(),
        other => panic!("最初の呼び出しが判定でない: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 入れ子の値と構造表現の相互変換（上流の変換そのもの。本ファイルは JSON を手で書かない）
// ---------------------------------------------------------------------------

/// セル値の**構造表現**（`document-format` の JSON 表現そのもの）。
fn json_of(value: &CellValue) -> String {
    let bytes = to_json_bytes(value, "tests/edit_nested.rs").expect("標本の値は表現できる");
    String::from_utf8(bytes).expect("JSON 表現は UTF-8")
}

/// 構造表現をセル値へ戻す（上流の変換そのもの）。
fn value_of(json: &str) -> CellValue {
    from_json_bytes(json.as_bytes()).expect("組み立てた表現は解釈できる")
}

/// オブジェクトのフィールドを読む（値に無ければ検査の前提が崩れている）。
fn field_of(value: &CellValue, field: &str) -> CellValue {
    match value {
        CellValue::Nested(NestedValue::Object(entries)) => entries
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("オブジェクトにフィールド {field} が無い: {value:?}")),
        other => panic!("オブジェクトでない値のフィールドを読もうとした: {other:?}"),
    }
}

/// オブジェクトのフィールドを 1 つだけ置き換えた値。
fn with_field(value: &CellValue, field: &str, replacement: CellValue) -> CellValue {
    match value {
        CellValue::Nested(NestedValue::Object(entries)) => {
            let Some(position) = entries.iter().position(|(name, _)| name == field) else {
                panic!("オブジェクトにフィールド {field} が無い: {value:?}");
            };
            let mut entries = entries.clone();
            entries[position].1 = replacement;
            CellValue::Nested(NestedValue::Object(entries))
        }
        other => panic!("オブジェクトでない値のフィールドを置き換えようとした: {other:?}"),
    }
}

/// 配列の要素を 1 つだけ置き換えた値。
fn with_element(value: &CellValue, index: usize, replacement: CellValue) -> CellValue {
    match value {
        CellValue::Nested(NestedValue::Array(items)) => {
            let mut items = items.clone();
            items[index] = replacement;
            CellValue::Nested(NestedValue::Array(items))
        }
        other => panic!("配列でない値の要素を置き換えようとした: {other:?}"),
    }
}

/// 配列の要素を読む（範囲外なら検査の前提が崩れている）。
fn element_of(value: &CellValue, index: usize) -> CellValue {
    match value {
        CellValue::Nested(NestedValue::Array(items)) => items
            .get(index)
            .cloned()
            .unwrap_or_else(|| panic!("配列に要素 {index} が無い: {value:?}")),
        other => panic!("配列でない値の要素を読もうとした: {other:?}"),
    }
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
    /// 違反を 1 件も仕込まない標本（違反の位置の検査は、仕込んだ違反と混ざると数えられない）。
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

    /// 列 `column` の入れ子のフィールド名（宣言から読み直す。写しを書かない）。
    fn object_fields(&self, column: usize) -> Vec<String> {
        object_fields(&self.plan, column)
    }

    /// 列 `column` の違反を、**その列に限定した**報告から読む（上限を設けない）。
    ///
    /// 本層は [`data_grid::EditOutcome`] に違反の**総数**しか載せない（design.md の Service
    /// Interface）。位置（入れ子の内側の位置を含む）は違反の一覧を持つ報告から読む — これが
    /// 要件 5.5 の「入れ子のどの位置が違反しているか」を観測する経路である。
    fn column_report(&self, column: usize) -> SheetReport {
        validate_columns(
            self.document(),
            self.sheet(),
            &self.plan,
            &[ColumnIndex::new(column)],
            &ValidationOptions::unlimited(),
        )
    }
}

/// 標本を使う検査の前提を先に確かめる（モジュール docs「前提を先に確かめる」）。
fn assert_clean_premises(sample: &common::sample::Sample) {
    assert_eq!(0, sample.injected_violations(), "違反を仕込んでいない標本");
    assert_eq!(
        0,
        sample.unique_violations(),
        "一意制約の重複を混ぜていない標本"
    );

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
    let report = SchemaEngine::new().validate_sheet(
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

/// 列 `column` の入れ子のフィールド名を計画から読み出す（オブジェクト、または配列の要素の
/// オブジェクト）。
fn object_fields(schema: &CompiledSchema, column: usize) -> Vec<String> {
    let Some(validator) = schema.validator(ColumnIndex::new(column)) else {
        return Vec::new();
    };
    let fields = match validator {
        ColumnValidator::Object { fields } => fields,
        ColumnValidator::Array { items, .. } => match items.as_ref() {
            ColumnValidator::Object { fields } => fields,
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    fields.iter().map(|field| field.name().to_owned()).collect()
}

/// 適用の前後の差が、指定した 1 セルだけであることを表明する。
fn assert_only_cell_changed(
    before: &(Vec<RowId>, Vec<Vec<CellValue>>),
    after: &(Vec<RowId>, Vec<Vec<CellValue>>),
    row: RowId,
    column: usize,
) {
    assert_eq!(before.0, after.0, "行の並びと識別子は変わらない");
    assert_eq!(before.1.len(), after.1.len(), "行数は変わらない");
    for (position, (before_row, after_row)) in before.1.iter().zip(&after.1).enumerate() {
        assert_eq!(
            before_row.len(),
            after_row.len(),
            "行 {position} の値数が変わった"
        );
        for (index, (before_value, after_value)) in before_row.iter().zip(after_row).enumerate() {
            if before.0[position] == row && index == column {
                assert_ne!(
                    before_value, after_value,
                    "編集したセルの値が変わっていない（検査が空虚になる）"
                );
            } else {
                assert_eq!(
                    before_value, after_value,
                    "編集していないセルが変わった: 行 {position} 列 {index}"
                );
            }
        }
    }
}

/// 入れ子のセルを構造表現で書く命令。
fn set_nested(row: RowId, column: usize, json: &str) -> EditCommand {
    EditCommand::SetNested {
        cell: CellAddress::new(row, ColumnIndex::new(column)),
        json: json.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// 1. 構造を保ったまま書き戻す（要件 5.5）
// ---------------------------------------------------------------------------

/// 入れ子のオブジェクトの内側のフィールドを 1 つだけ編集する。
///
/// 編集したフィールドは変わり、**他のフィールドは 1 つも変わらない**。加えて、変更前の JSON
/// （4.1 の逆命令が保持するもの）で書き戻すと、値が 1 つも違わずに戻る（往復）。
#[test]
fn editing_one_inner_field_leaves_every_other_field_and_the_rest_of_the_document_unchanged() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let column = DESTINATION;

    // 前提: このセルは入れ子のオブジェクトであり、宣言されたフィールドを持つ。
    let fields = fixture.object_fields(column);
    assert_eq!(5, fields.len(), "届け先のフィールドの数が前提と違う");
    let before = fixture.value_at(row, column);
    assert!(
        matches!(before, CellValue::Nested(NestedValue::Object(_))),
        "届け先の値がオブジェクトでない: {before:?}"
    );
    // それぞれのフィールドを読めることを確かめてから依拠する（読めなければ panic する）。
    for field in &fields {
        field_of(&before, field);
    }
    let before_json = json_of(&before);
    assert_eq!(
        before,
        value_of(&before_json),
        "構造表現の往復が値と一致しない（前提が崩れている）"
    );

    // 編集する値は、**上流の変換で**構造表現を組み立てる（本ファイルは JSON を手で書かない）。
    let edited_field = fields[2].clone();
    let edited_text = CellValue::Text("架空市9丁目9番9号".to_owned());
    let edited = with_field(&before, &edited_field, edited_text.clone());
    assert_ne!(
        before, edited,
        "編集した値が元の値と同じ（検査が空虚になる）"
    );
    let edited_json = json_of(&edited);
    assert_ne!(
        before_json, edited_json,
        "編集した表現が元の表現と同じ（検査が空虚になる）"
    );

    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before_document = fixture.snapshot();

    let outcome = apply
        .apply(
            fixture.document_mut(),
            set_nested(row, column, &edited_json),
        )
        .expect("構造表現として解釈できる入力の書き込みは成功する");

    assert_eq!(vec![row], outcome.affected, "影響を受けた行");
    assert_eq!(
        fixture.row_count(),
        outcome.row_count,
        "入れ子の編集は行数を変えない"
    );
    assert_eq!(
        0, outcome.violation_total,
        "適合する入れ子の値は違反を生まない"
    );
    assert!(outcome.coercions.is_empty(), "変換は起きていない");

    // 1. 編集したフィールドは変わり、他のフィールドは 1 つも変わらない。
    let after = fixture.value_at(row, column);
    for field in &fields {
        let before_field = field_of(&before, field);
        let after_field = field_of(&after, field);
        if *field == edited_field {
            assert_eq!(edited_text, after_field, "編集したフィールドの値が違う");
            assert_ne!(
                before_field, after_field,
                "編集したフィールド {field} が変わっていない（検査が空虚になる）"
            );
        } else {
            assert_eq!(
                before_field, after_field,
                "編集していないフィールド {field} が変わった"
            );
        }
    }
    // 2. 構造表現としても編集後の期待値と一致する（構造が保たれている）。
    assert_eq!(
        edited_json,
        json_of(&after),
        "書き戻した値の構造表現が期待と違う"
    );
    // 3. ドキュメントの他のセルは 1 つも変わらない。
    assert_only_cell_changed(&before_document, &fixture.snapshot(), row, column);

    // 4. 往復: 変更前の JSON で書き戻すと、値が 1 つも違わずに戻る（要件 5.5, 5.7）。
    let restored = apply
        .apply(
            fixture.document_mut(),
            set_nested(row, column, &before_json),
        )
        .expect("変更前の表現の書き戻しも成功する");
    assert_eq!(vec![row], restored.affected, "書き戻しも同じ行を報告する");
    assert_eq!(
        0, restored.violation_total,
        "書き戻した値は元の値であり違反を生まない"
    );
    assert_eq!(
        before,
        fixture.value_at(row, column),
        "往復で値が元に戻らない（変換が可逆でない）"
    );
    assert_eq!(
        before_json,
        json_of(&fixture.value_at(row, column)),
        "往復後の構造表現が変更前と違う"
    );
    assert_no_sheet_wide_validation(&calls);
}

/// 入れ子の配列の要素の内側のフィールドを編集する（配列の要素位置が保たれる）。
#[test]
fn editing_an_element_of_an_array_keeps_the_other_elements() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let column = LINE_ITEMS;

    // 前提: 2 要素の配列であり、各要素が宣言されたフィールドを持つオブジェクトである。
    let fields = fixture.object_fields(column);
    assert_eq!(3, fields.len(), "明細の要素のフィールドの数が前提と違う");
    let before = fixture.value_at(row, column);
    let first_before = element_of(&before, 0);
    let second_before = element_of(&before, 1);
    for field in &fields {
        field_of(&first_before, field);
        field_of(&second_before, field);
    }

    let edited_first = with_field(
        &first_before,
        &fields[0],
        CellValue::Text("ITEM-9999".to_owned()),
    );
    let edited = with_element(&before, 0, edited_first.clone());
    assert_ne!(
        before, edited,
        "編集した値が元の値と同じ（検査が空虚になる）"
    );
    let (mut apply, _calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            set_nested(row, column, &json_of(&edited)),
        )
        .expect("配列の要素の編集も成功する");

    assert_eq!(vec![row], outcome.affected, "影響を受けた行");
    assert_eq!(0, outcome.violation_total, "適合する値は違反を生まない");
    let after = fixture.value_at(row, column);
    assert_eq!(
        edited_first,
        element_of(&after, 0),
        "編集した要素の値が違う"
    );
    assert_ne!(
        first_before,
        element_of(&after, 0),
        "編集した要素が変わっていない（検査が空虚になる）"
    );
    assert_eq!(
        second_before,
        element_of(&after, 1),
        "編集していない要素が変わった"
    );
}

// ---------------------------------------------------------------------------
// 2. 入れ子の内側の違反の位置（要件 5.5）
// ---------------------------------------------------------------------------

/// 配列の要素の内側の違反が、**要素の位置とフィールド名**を保って報告される（要件 5.5）。
#[test]
fn a_violation_inside_an_array_reports_the_element_position() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let column = LINE_ITEMS;

    // 前提: 宣言されたフィールドのうち 2 番目が数量である（最小値 1 の `int`）。
    let fields = fixture.object_fields(column);
    assert_eq!(
        "数量", fields[1],
        "明細の要素の 2 番目のフィールドが前提と違う"
    );
    let before = fixture.value_at(row, column);
    let first_before = element_of(&before, 0);
    // 編集する値は、**上流の変換で**組み立てる（要素 0 の数量を下限の外へ出す）。
    let edited_first = with_field(&first_before, &fields[1], CellValue::Int(0));
    let edited = with_element(&before, 0, edited_first);
    let edited_json = json_of(&edited);
    let parsed = value_of(&edited_json);
    assert_eq!(edited, parsed, "組み立てた表現の解釈が期待と違う");

    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            set_nested(row, column, &edited_json),
        )
        .expect("適合しない入れ子の値でも編集は成功する（編集経路は拒否しない）");

    // **違反の総数は報告から来る**（`EditOutcome` が運ぶのは総数だけである）。
    assert_eq!(
        1, outcome.violation_total,
        "内側の違反が 1 件報告されていない"
    );
    assert_eq!(vec![row], outcome.affected, "影響を受けた行");
    // 適合しない値も破棄されず、構造ごとドキュメントに残る（要件 3.5 と同じ規律）。
    assert_eq!(
        edited,
        fixture.value_at(row, column),
        "適合しない入れ子の値もドキュメントに残る"
    );
    // 判定へ渡った値も**入れ子の木そのもの**である（平坦化していない）。
    assert_eq!(
        edited,
        judged_values(&calls)[column],
        "判定へ渡った値が入れ子の木でない（平坦化している）"
    );

    // 内側の位置は、編集した列に限定した報告の違反が運ぶ（要件 5.5）。
    let report = fixture.column_report(column);
    assert_eq!(1, report.total_violations(), "当該列の違反は 1 件");
    assert!(!report.is_truncated(), "違反の保持が上限で切れている");
    let violation = &report.violations()[0];
    assert_eq!(Some(row), violation.row(), "違反の行");
    assert_eq!(ColumnIndex::new(column), violation.column(), "違反の列");
    assert_eq!(
        fixture.columns()[column],
        violation.column_name(),
        "違反の列名"
    );
    // 入れ子の内側の位置は上流の `ValuePath` であり、本クレートの `NestedPath` がその写しである。
    let path = NestedPath::from(violation.path());
    assert_eq!(
        vec![
            NestedPathSegment::Index(0),
            NestedPathSegment::Field(fields[1].clone().into()),
        ],
        path.segments().to_vec(),
        "違反の内側の位置が、編集した要素とフィールドを指していない"
    );
    assert!(!path.is_root(), "内側の違反の位置がセル直下になっている");
}

/// オブジェクトのフィールドの内側の違反が、**フィールド名**を保って報告される（要件 5.5）。
#[test]
fn a_violation_inside_an_object_reports_the_field_position() {
    let mut fixture = Fixture::clean(64, 13);
    let row = fixture.row(0);
    let column = DESTINATION;

    // 前提: 宣言されたフィールドのうち 1 番目が郵便番号である（書式つき）。
    let fields = fixture.object_fields(column);
    assert_eq!(
        "郵便番号", fields[0],
        "届け先の 1 番目のフィールドが前提と違う"
    );
    let before = fixture.value_at(row, column);
    // ハイフンの無い 7 桁（書式だけを外す）。
    let edited = with_field(&before, &fields[0], CellValue::Text("1000001".to_owned()));
    let (mut apply, _calls) = counting(fixture.sheet(), fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            set_nested(row, column, &json_of(&edited)),
        )
        .expect("適合しない入れ子の値でも編集は成功する");

    assert_eq!(
        1, outcome.violation_total,
        "内側の違反が 1 件報告されていない"
    );
    let report = fixture.column_report(column);
    let violation = &report.violations()[0];
    assert_eq!(Some(row), violation.row(), "違反の行");
    let path = NestedPath::from(violation.path());
    assert_eq!(
        vec![NestedPathSegment::Field(fields[0].clone().into())],
        path.segments().to_vec(),
        "違反の内側の位置が、編集したフィールドを指していない"
    );
    assert_eq!(
        edited,
        fixture.value_at(row, column),
        "適合しない入れ子の値もドキュメントに残る"
    );
}

// ---------------------------------------------------------------------------
// 3. 構造表現として解釈できない入力（処理を止める誤り）
// ---------------------------------------------------------------------------

/// 構造表現として解釈できない入力は [`GridError::NestedDecode`] を返し、**何も書かない**。
///
/// JSON として不正な入力・`i64` の範囲外の整数・非有限になる数値リテラルのいずれも、
/// 「構造表現として解釈できない入力」である（上流の `from_json_bytes` が同じ門で拒む）。
#[test]
fn input_that_is_not_a_structural_representation_stops_the_edit() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let column = DESTINATION;
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before = fixture.snapshot();
    let cell = CellAddress::new(row, ColumnIndex::new(column));

    // 前提: これらの入力が上流の解釈で拒まれること（本層の写しだけを検査しない）。
    let payloads = [
        "{",                   // JSON として不正
        "",                    // 空（JSON として不正）
        "1e999",               // 非有限になる数値リテラル
        "9223372036854775808", // `i64` の範囲外の整数リテラル
    ];
    for payload in payloads {
        assert!(
            from_json_bytes(payload.as_bytes()).is_err(),
            "前提が崩れている（上流が {payload} を拒まない）"
        );

        let error = apply
            .apply(fixture.document_mut(), set_nested(row, column, payload))
            .expect_err("解釈できない入力は失敗する");

        assert_eq!(
            GridError::NestedDecode { cell },
            error,
            "解釈できない入力の誤りが `NestedDecode` でない: {payload}"
        );
        assert_eq!(
            before,
            fixture.snapshot(),
            "解釈できない入力で 1 つのセルも書かれない: {payload}"
        );
        assert_eq!(
            Vec::<QueryCall>::new(),
            recorded(&calls),
            "解釈できない入力では縫い目を 1 回も呼ばない: {payload}"
        );
    }
}

/// 解釈できない入力と宛先の誤りが同時に成り立つ命令は、**解釈の誤り**として止まる。
///
/// `SetNested` の検査の順（セッションの前提 → 表現の解釈 → 宛先）は本層の決定である。
/// 入れ子の編集は表現の解釈が**ドキュメントを読まない**操作であるため、壊れた入力は行の
/// 走査を起こさずに止まる — この順序を、**未知の行と壊れた表現の双方を持つ命令**で固定する
/// （どちらの誤りも「1 つも書かず縫い目を呼ばない」を満たすため、返る変種を決めているのは
/// 順序だけである）。
#[test]
fn a_broken_representation_is_reported_before_the_destination_is_checked() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    // 標本の参照先シートの行はデータシートに属さない（未知の行）。
    let foreign = fixture
        .document()
        .sheet_by_id(fixture.parts.reference)
        .expect("参照先シートは文書にある")
        .rows()[0]
        .id();
    let outside = fixture.columns().len();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before = fixture.snapshot();

    // 1. 未知の行 + 壊れた表現 → 解釈の誤り（表現の側に**セルの位置を載せて**返る）。
    let error = apply
        .apply(
            fixture.document_mut(),
            set_nested(foreign, DESTINATION, "{"),
        )
        .expect_err("壊れた表現は失敗する");
    assert_eq!(
        GridError::NestedDecode {
            cell: CellAddress::new(foreign, ColumnIndex::new(DESTINATION)),
        },
        error,
        "解釈の誤りが宛先の誤りより先に返る"
    );

    // 2. 範囲外の列 + 壊れた表現 → 同じく解釈の誤り。
    let error = apply
        .apply(fixture.document_mut(), set_nested(row, outside, "{"))
        .expect_err("壊れた表現は失敗する");
    assert_eq!(
        GridError::NestedDecode {
            cell: CellAddress::new(row, ColumnIndex::new(outside)),
        },
        error,
        "解釈の誤りが列の範囲の誤りより先に返る"
    );

    // 3. 解釈できる表現なら、宛先の誤りがそのまま返る（順序が両方向で効いている）。
    let payload = json_of(&CellValue::Int(42));
    let error = apply
        .apply(
            fixture.document_mut(),
            set_nested(foreign, DESTINATION, &payload),
        )
        .expect_err("未知の行への書き込みは失敗する");
    assert_eq!(GridError::UnknownRow { row: foreign }, error);

    assert_eq!(before, fixture.snapshot(), "1 つのセルも書かれない");
    assert_eq!(
        Vec::<QueryCall>::new(),
        recorded(&calls),
        "判定も呼ばれない"
    );
}

// ---------------------------------------------------------------------------
// 4. 入れ子でない値の取り決め
// ---------------------------------------------------------------------------

/// 構造表現として解釈できても**入れ子でない**値を、本層は**そのまま判定へ渡す**。
///
/// 同じ入力（JSON の整数 `42`）を入れ子の列と `int` の列へ書く。どちらが適合するかを決めるのは
/// `schema-engine` の規則表だけである（本層は列の型を見ない）。入れ子の列では**誤りではなく**
/// 違反として報告され、値は破棄されずドキュメントに残る。
#[test]
fn json_that_is_not_nested_is_decided_by_the_type_system() {
    let mut fixture = Fixture::clean(64, 13);
    let row = fixture.row(0);
    let json = json_of(&CellValue::Int(42));
    assert_eq!(CellValue::Int(42), value_of(&json), "前提: 整数の表現");
    let (mut apply, _calls) = counting(fixture.sheet(), fixture.plan.clone());

    // 1. 入れ子の列: 適合しないので違反になるが、**中止しない**。
    let outcome = apply
        .apply(fixture.document_mut(), set_nested(row, DESTINATION, &json))
        .expect("入れ子でない値の書き込みも成功する（編集経路は拒否しない）");
    assert_eq!(1, outcome.violation_total, "入れ子の列では違反になる");
    assert!(outcome.coercions.is_empty(), "入れ子の列への変換は無い");
    assert_eq!(
        CellValue::Int(42),
        fixture.value_at(row, DESTINATION),
        "適合しない値もドキュメントに残る"
    );
    // 違反は**セルそのもの**についてであり、内側の位置を持たない。
    let cell_violation = fixture.column_report(DESTINATION);
    assert!(
        NestedPath::from(cell_violation.violations()[0].path()).is_root(),
        "セルそのものの違反が内側の位置を持っている"
    );

    // 2. `int` の列: 同じ入力が適合する（`Int` は `int` の型であり、変換も起きない）。
    let outcome = apply
        .apply(fixture.document_mut(), set_nested(row, QUANTITY, &json))
        .expect("適合する値の書き込みは成功する");
    assert_eq!(0, outcome.violation_total, "`int` の列では適合する");
    assert!(
        outcome.coercions.is_empty(),
        "`Int` から `int` への変換は無い"
    );
    assert_eq!(
        CellValue::Int(42),
        fixture.value_at(row, QUANTITY),
        "解釈した値がそのまま書かれる"
    );
}

// ---------------------------------------------------------------------------
// 5. 変換の記録（要件 3.4）
// ---------------------------------------------------------------------------

/// 構造表現から解釈した値も `schema-engine` の規則表を通り、変換の前後が結果に載る（要件 3.4）。
///
/// 使う組は規則表が変換を定める `Text` → `int` である。JSON の文字列 `"007"` は 10 進文法に
/// 一致するため `Decimal` に読まれる — `Text` を運ぶには構造表現の脱出口
/// （`document-format` が `Text` を書くときに使う形そのもの）を通す。本ファイルはその表現も
/// **上流の `to_json_bytes` で組み立て**、解釈が `Text` へ戻ることを前提として確かめる。
#[test]
fn a_coerced_value_is_reported_with_its_before_and_after() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let column = QUANTITY;
    let text = CellValue::Text("007".to_owned());
    let json = json_of(&text);
    // 前提: この表現は `Text` として解釈される（`Decimal` へ化けない）。
    assert_eq!(text, value_of(&json), "前提: 脱出口つきの `Text` の表現");
    assert!(
        fixture.plan.required(ColumnIndex::new(column)),
        "標本の列 1 は必須"
    );
    let row_count = fixture.row_count();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(fixture.document_mut(), set_nested(row, column, &json))
        .expect("変換を伴う書き込みも成功する");

    assert_eq!(vec![row], outcome.affected, "影響を受けた行");
    assert_eq!(row_count, outcome.row_count, "入れ子の編集は行数を変えない");
    assert_eq!(0, outcome.violation_total, "変換後の値は適合する");
    assert_eq!(
        vec![CoercionNotice {
            cell: CellAddress::new(row, ColumnIndex::new(column)),
            before: "007".to_owned(),
            // 変換後の期待値は表示文字列の唯一の源（`display_text`）から導く。
            after: display_text(&CellValue::Int(7)).into_owned(),
        }],
        outcome.coercions,
        "変換の前と後の双方が載り、前後が異なる"
    );
    assert_ne!(
        outcome.coercions[0].before, outcome.coercions[0].after,
        "変換の前後が同じ綴りなら変換していない"
    );
    assert_eq!(
        CellValue::Int(7),
        fixture.value_at(row, column),
        "ドキュメントには変換後の値が入る"
    );
    assert_eq!(
        vec![
            QueryCall::JudgeWrite(fixture.row_with(row, column, text)),
            QueryCall::RevalidateColumns(vec![ColumnIndex::new(column)]),
        ],
        recorded(&calls),
        "判定へ渡るのは解釈した値そのものであり、呼び出しの形は 1 セルの編集のまま"
    );
}

// ---------------------------------------------------------------------------
// 6. 1 セルの編集の呼び出しの形（要件 11.4）
// ---------------------------------------------------------------------------

/// 入れ子の編集も **1 セルの編集**である: 判定は 1 回、再検証は**編集した列だけ**、
/// シート全件の検証は 1 回も呼ばれない（要件 11.4）。
#[test]
fn a_nested_edit_is_a_single_cell_edit_for_the_call_shape() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let column = DESTINATION;
    let before = fixture.value_at(row, column);
    let fields = fixture.object_fields(column);
    let edited = with_field(
        &before,
        &fields[2],
        CellValue::Text("架空市7丁目7番7号".to_owned()),
    );
    // 判定へ渡る値は「その行の値を、その列だけ解釈した値へ置き換えたもの」である（上流の
    // `validate_write` は 1 行分の値を受け取り、値の添字を列の添字として読む）。
    let judged = fixture.row_with(row, column, edited.clone());
    let row_count = fixture.row_count();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            set_nested(row, column, &json_of(&edited)),
        )
        .expect("適合する値の書き込みは成功する");

    // **記録の全体**が「判定 1 回 + 当該列だけの再検証 1 回」である（全件検証は現れない）。
    assert_eq!(
        vec![
            QueryCall::JudgeWrite(judged),
            QueryCall::RevalidateColumns(vec![ColumnIndex::new(column)]),
        ],
        recorded(&calls),
        "入れ子の編集の呼び出しは判定 1 回と当該列の再検証 1 回だけ"
    );
    assert_no_sheet_wide_validation(&calls);
    assert_eq!(vec![row], outcome.affected, "影響を受けた行");
    assert_eq!(row_count, outcome.row_count, "行数は変わらない");
}

// ---------------------------------------------------------------------------
// 7. 誤りの経路と部分適用の不在
// ---------------------------------------------------------------------------

/// 他シートの行・範囲外の列を指す命令は、判別可能な誤りを返し、**何も書かない**。
#[test]
fn an_unknown_row_or_an_out_of_range_column_stops_before_any_write() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    // 標本の参照先シートの行はデータシートに属さない。
    let foreign = fixture
        .document()
        .sheet_by_id(fixture.parts.reference)
        .expect("参照先シートは文書にある")
        .rows()[0]
        .id();
    let outside = fixture.columns().len();
    let payload = json_of(&CellValue::Nested(NestedValue::Object(Vec::new())));
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before = fixture.snapshot();

    let error = apply
        .apply(
            fixture.document_mut(),
            set_nested(foreign, DESTINATION, &payload),
        )
        .expect_err("他シートの行への書き込みは失敗する");
    assert_eq!(GridError::UnknownRow { row: foreign }, error);
    assert_eq!(before, fixture.snapshot(), "1 つのセルも書かれない");
    assert_eq!(
        Vec::<QueryCall>::new(),
        recorded(&calls),
        "判定も呼ばれない"
    );

    let error = apply
        .apply(fixture.document_mut(), set_nested(row, outside, &payload))
        .expect_err("範囲外の列への書き込みは失敗する");
    assert_eq!(
        GridError::ColumnOutOfRange {
            column: ColumnIndex::new(outside),
            count: outside,
        },
        error
    );
    assert_eq!(before, fixture.snapshot(), "1 つのセルも書かれない");
    assert_eq!(
        Vec::<QueryCall>::new(),
        recorded(&calls),
        "判定も呼ばれない"
    );
}

/// 列が 1 件も宣言されていないシートは入れ子の編集もできない（[`GridError::SchemaUnusable`]）。
#[test]
fn a_nested_command_on_a_sheet_without_columns_is_unusable() {
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
    // 行識別子は正準テキスト形から作る（この経路はシートの前提で止まるため実在は問わない）。
    let row = "00000000000000000000000000"
        .parse::<RowId>()
        .expect("正準の行識別子が解析できない");
    let error = apply
        .apply(
            &mut document,
            set_nested(
                row,
                0,
                &json_of(&CellValue::Nested(NestedValue::Array(Vec::new()))),
            ),
        )
        .expect_err("列 0 本のシートは編集できない");

    assert_eq!(GridError::SchemaUnusable { sheet }, error);
    assert_eq!(before, snapshot(&document, sheet), "何も変わらない");
    assert_eq!(
        Vec::<QueryCall>::new(),
        recorded(&calls),
        "判定も呼ばれない"
    );
}

// ---------------------------------------------------------------------------
// 8. 決定性
// ---------------------------------------------------------------------------

/// 同じドキュメントと同じ命令の 2 回の適用は同じ結果を返し、状態も変えない。
#[test]
fn applying_the_same_nested_command_twice_gives_the_same_outcome() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(1);
    let column = DESTINATION;
    let before = fixture.value_at(row, column);
    let fields = fixture.object_fields(column);
    let edited = with_field(&before, &fields[3], CellValue::Text("新館".to_owned()));
    let command = set_nested(row, column, &json_of(&edited));
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
    // 識別子は再発行されない（入れ子の編集は行を増減しない）。
    assert_eq!(
        fixture.parts.row_ids,
        document_ids(fixture.document(), sheet),
        "行識別子と並びは変わらない"
    );
    // 変換の記録も違反の総数も同じである（判定が決定的である）。
    assert!(first.coercions.is_empty(), "変換は起きていない");
    assert_eq!(0, first.violation_total, "適合する値は違反を生まない");
}

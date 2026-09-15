//! 表形式テキストの解釈と、範囲への貼り付けの適用（データグリッドのタスク 3.4。
//! data-grid 要件 7.3, 7.4, 7.5, 7.7, 8.9）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のものである。
//!
//! 1. **表形式テキストの解釈**（要件 7.3, 7.4）。行の区切り（LF。`\r\n` も受ける）と列の区切り
//!    （TAB）でセルの矩形に分かれ、値の中に区切りが含まれる場合は囲み（`"`。`""` が
//!     escaped quote）で表される。囲みの中の改行は 1 つの値の一部である。
//! 2. **往復**（設計の Testing Strategy の `PasteCodec`）。行と列の区切りを含む値が
//!    テキスト → セル → テキスト で保たれる（要件 7.2 の複製の経路が同じ写しを使う）。
//! 3. **貼り付けの宛先は 2 つの成分で決まる**（要件 8.6, 8.9）。列は `anchor` の列から右へ、
//!    行は `rows`（**表示されている行の並び**）を `anchor` の行の位置から順に歩く。
//! 4. **絞り込みが効いている間は表示されている行にのみ反映する**（要件 8.9）。隠れている行は
//!    1 つも変わらず、可視の行は矩形の対応する行を受ける。錨が表示されていなければ
//!    1 つのセルも書かない。
//! 5. **貼り付けの範囲が既存の行数を超えるとき、不足する行を末尾へ足して完了する**（要件 7.4）。
//!    補充した行は宣言の既定値で作られ、矩形が覆わない列はその既定値のまま残る。
//! 6. **適合しない値があっても反映を中止せず、違反として保持したうえで件数を返す**（要件 7.5）。
//!    適合しない値は破棄されずドキュメントから読み戻せる。
//! 7. **判定の呼び出しの形**（要件 7.7, 11.5）。貼り付けは**行ごとの判定を 1 回も呼ばず**
//!    （`judge_write` を 0 回）、**貼り付けた列に限定した再検証を 1 回だけ**呼び、**シート全件の
//!    検証を 1 回も呼ばない**。証拠は速度ではなく**呼び出しの形**であり、**1 万行の貼り付けでも
//!    4 行の貼り付けと同じ形**であることを、同じ表明で固定する。
//! 8. **誤りの経路と部分適用の不在**。未知の行（錨の行・`rows` の中の行）・範囲外の列・
//!    矩形の列のはみ出し・使えないスキーマは [`GridError`] の判別可能な変種として返り、
//!    **1 つのセルも書かれず、縫い目も 1 回も呼ばれない**。
//! 9. **`affected` と `row_count`**。`affected` は値を書いた行（補充した行を含む）を書いた順に
//!    重複なく並べ、`row_count` は適用後の行数である。
//! 10. **決定性**。同じドキュメントと同じ命令の 2 回の適用は同じ結果を返し、状態も変えない
//!    （行を補充しない貼り付けについて）。
//! 11. **空の貼り付け**。行 0 件のテキストも、表示されている行が 1 つも無い場合も、成功し、
//!    何も変えず、**縫い目を 1 回も呼ばない**（3.1 の空の命令と同じ規則）。
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
//! **標本の識別子は発行のたびに変わる**ため、生の識別子や生の値を実行を跨いで比較しない
//! （tasks.md の Implementation Notes の規則）。比較するのは**構造**（呼び出しの形・列の
//! 添字・行数・値の変種と中身のような数と文字列）である。
//!
//! # 数える縫い目
//!
//! 要件 7.7 / 11.5 は「行ごとの判定で完了する」ことの表明であり、その証拠は**呼び出しの形**
//! だけである。本ファイルは [`EditSchemaQuery`] を実装した [`CountingQuery`] を
//! [`EditApply::with_query`] へ差し込み、縫い目へ届いた呼び出しを**到着順の 1 本の記録**として
//! 観測する（`tests/edit_apply.rs` と同じ形）。数える先は**本番の実装そのもの**
//! （[`SchemaEngineQuery`]）であり、非公開の模擬へ委譲しない。

mod common;

use std::sync::{Arc, Mutex};

use common::sample::{sample, SampleEditParts, SampleOptions};
use data_grid::{
    display_text, CellAddress, CoercionNotice, ColumnIndex, EditApply, EditCommand, EditSchemaQuery,
    FilterSpec, GridError, PasteCodec, RowOrder, RowOrdinal, RowSpan, SchemaEngineQuery, ViewSpec,
};
use document_format::{CellValue, Document, RowId, SheetId};
use schema_engine::{
    validate_sheet, CompiledSchema, EditVerdict, SheetReport, ValidationOptions,
};

// ---------------------------------------------------------------------------
// 縫い目へ届いた呼び出しの記録
// ---------------------------------------------------------------------------

/// 縫い目へ届いた 1 回の呼び出し（到着順に並べる）。
///
/// 3 変種は縫い目の 3 つの操作に 1 対 1 で対応し、**呼ばれたものをそのまま記録する**
/// （記録から漏れる呼び出しが無いことが「行ごとの判定も全件検証も 0 回」の表明の前提である）。
#[derive(Debug, Clone, PartialEq)]
enum QueryCall {
    /// 書き込みの判定（**貼り付けはこれを 1 回も呼ばない**）。
    JudgeWrite(Vec<CellValue>),
    /// 列を限定した再検証。渡った列の集合を運ぶ。
    RevalidateColumns(Vec<ColumnIndex>),
    /// シート全件の検証。**編集経路はこれを呼ばない**（要件 11.4）。
    ValidateSheet,
}

/// `schema-engine` の判定を**数える**縫い目。
///
/// 本番の縫い目（[`SchemaEngineQuery`]）へ**そのまま委譲**し、委譲の前に呼び出しを記録する。
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
    /// 違反を 1 件も仕込まない標本（貼り付けが生む違反を数えるには、仕込んだ違反と混ざると
    /// 数えられない）。
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

    /// 文書の位置 `index` の行の識別子（前提により、標本の文書順は行識別子の順である）。
    fn row(&self, index: usize) -> RowId {
        self.parts.row_ids[index]
    }

    fn row_count(&self) -> usize {
        self.parts.row_ids.len()
    }

    /// 文書の行の識別子（適用の後は適用前と変わりうるため、その都度読む）。
    fn ids(&self) -> Vec<RowId> {
        ids_of(self.document(), self.sheet())
    }

    /// 文書の行順の行の識別子（表示の指定を掛けない並び）。
    fn all_rows(&self) -> Vec<RowId> {
        self.ids()
    }

    /// 他シートの行（データシートに属さない行。誤りの経路の検査に使う）。
    fn foreign_row(&self) -> RowId {
        self.document()
            .sheet_by_id(self.parts.reference)
            .expect("参照先シートは文書にある")
            .rows()[0]
            .id()
    }

    /// 行 `row` の列 `column` の値をドキュメントから読み戻す。
    fn value_at(&self, row: RowId, column: usize) -> CellValue {
        value_at(self.document(), self.sheet(), row, column)
    }

    /// 列 `column` の値の並び（文書順）。
    fn column_values(&self, column: usize) -> Vec<CellValue> {
        self.ids()
            .into_iter()
            .map(|row| self.value_at(row, column))
            .collect()
    }

    /// 行 `row` の文書の位置。
    fn position_of(&self, row: RowId) -> usize {
        self.ids()
            .iter()
            .position(|found| *found == row)
            .expect("行はシートにある")
    }

    /// 適用の前後で変わらないことを見るための写し（行の並びと全行の値）。
    fn snapshot(&self) -> (Vec<RowId>, Vec<Vec<CellValue>>) {
        snapshot(self.document(), self.sheet())
    }

    /// シートの違反の総数（全件検証。貼り付けの `violation_total` の**対照**に使う）。
    fn sheet_violations(&self) -> usize {
        validate_sheet(
            self.document(),
            self.sheet(),
            &self.plan,
            &ValidationOptions::unlimited(),
        )
        .total_violations()
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
        ids_of(sample.document(), sample.sheet()),
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
fn ids_of(document: &Document, sheet: SheetId) -> Vec<RowId> {
    document
        .sheet_by_id(sheet)
        .expect("シートは文書にある")
        .rows()
        .iter()
        .map(|row| row.id())
        .collect()
}

/// ドキュメント全体の行識別子と値（適用の前後の比較用）。
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

/// 貼り付けの命令を組み立てる（錨・表示されている行の並び・テキスト）。
fn paste(anchor: CellAddress, rows: Vec<RowId>, text: &str) -> EditCommand {
    EditCommand::PasteRange {
        anchor,
        rows,
        text: text.to_owned(),
    }
}

/// 文字列の矩形をセル値の矩形へ写す（`PasteCodec::write` はセル値を受け取る）。
fn value_rows(rows: &[Vec<String>]) -> Vec<Vec<CellValue>> {
    rows.iter()
        .map(|row| row.iter().cloned().map(CellValue::Text).collect())
        .collect()
}

/// 文字列の並びで書いた矩形（検査の読みやすさのためだけの補助）。
fn rectangle(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|row| row.iter().map(|text| (*text).to_owned()).collect())
        .collect()
}

/// 可視行の並びを導出して返す（表示の指定を掛けた並び。要件 8.4）。
fn visible_rows(fixture: &Fixture, spec: &ViewSpec) -> Vec<RowId> {
    let mut order = RowOrder::default();
    order.recompute(fixture.document(), fixture.sheet(), spec);
    let visible = order
        .span(RowSpan::new(RowOrdinal::new(0), order.len()))
        .to_vec();
    assert_eq!(
        order.len(),
        visible.len(),
        "可視行の並びは `span` で全部読める"
    );
    visible
}

// ---------------------------------------------------------------------------
// 1. 表形式テキストの解釈（要件 7.3, 7.4）
// ---------------------------------------------------------------------------

/// 行の区切りと列の区切りを持つテキストが、セルの矩形として解釈される（要件 7.4）。
#[test]
fn the_codec_reads_rows_and_columns_apart() {
    assert_eq!(
        rectangle(&[&["a", "b"], &["c", "d"]]),
        PasteCodec::parse("a\tb\nc\td"),
        "TAB が列、LF が行"
    );
    assert_eq!(
        rectangle(&[&["a"], &["b"]]),
        PasteCodec::parse("a\r\nb"),
        "`\\r\\n` も行の区切りとして受ける"
    );
    assert_eq!(
        rectangle(&[&["a\rb"]]),
        PasteCodec::parse("a\rb"),
        "LF を伴わない単独の `\\r` は値の文字である"
    );
    assert_eq!(
        rectangle(&[&["", ""]]),
        PasteCodec::parse("\t"),
        "空の値も 1 つの列として数える（TAB は 2 つの空の値）"
    );
    // 末尾の行の区切りは**行を作らない**（貼り付けの最後に付いた改行が余分な行を書かない
    // ようにするため）。連続する区切りは空の行として残る。
    assert_eq!(rectangle(&[&["a"]]), PasteCodec::parse("a\n"));
    assert_eq!(rectangle(&[&["a"], &[""]]), PasteCodec::parse("a\n\n"));
    assert!(
        PasteCodec::parse("").is_empty(),
        "空のテキストは行 0 件（貼り付けるものが無い）"
    );
    assert_eq!(
        rectangle(&[&["x"]]),
        PasteCodec::parse("x"),
        "1 行 1 列も矩形である"
    );
}

/// 値の中に区切りが含まれる場合の囲みが解釈される（要件 7.4）。
#[test]
fn quoting_covers_separators_newlines_and_escaped_quotes() {
    assert_eq!(
        rectangle(&[&["a\tb", "c"]]),
        PasteCodec::parse("\"a\tb\"\tc"),
        "囲みの中の TAB は区切りにならない"
    );
    assert_eq!(
        rectangle(&[&["行1\n行2"]]),
        PasteCodec::parse("\"行1\n行2\""),
        "囲みの中の改行は 1 つの値の一部であり、行を作らない"
    );
    assert_eq!(
        rectangle(&[&["say \"hi\""]]),
        PasteCodec::parse("\"say \"\"hi\"\"\""),
        "`\"\"` は escaped quote"
    );
    assert_eq!(
        rectangle(&[&[""] ]),
        PasteCodec::parse("\"\""),
        "空の囲みは空の値"
    );
    assert_eq!(
        rectangle(&[&["a\r\nb"]]),
        PasteCodec::parse("\"a\r\nb\""),
        "囲みの中の `\\r\\n` も値の一部"
    );
    // 解釈は全域である（解析の誤りを返す変種が `GridError` に無い）。閉じない囲みは
    // 残り全部を 1 つの値とし、囲みの後に続く文字は値へ足す。
    assert_eq!(
        rectangle(&[&["ab\tcd"]]),
        PasteCodec::parse("\"ab\tcd"),
        "閉じない囲みは残り全部"
    );
    assert_eq!(
        rectangle(&[&["abcd"]]),
        PasteCodec::parse("\"ab\"cd"),
        "囲みの後の文字は値へ足す（囲みの外の `\"` は値の文字のまま）"
    );
    assert_eq!(
        rectangle(&[&["a\"b"]]),
        PasteCodec::parse("a\"b"),
        "囲みの外の `\"` は値の文字である（区切りにも囲みにもならない）"
    );
}

/// 行と列の区切りを含む値が、テキスト → セル → テキスト で往復する
/// （設計の Testing Strategy の `PasteCodec`。要件 7.2, 7.3）。
#[test]
fn the_codec_round_trips_values_that_hold_the_separators() {
    let expected = rectangle(&[
        &["plain", "a\tb"],
        &["line1\nline2", "say \"hi\""],
        &["", "trailing\t"],
        &["a\rb", "\"quoted\""],
    ]);

    let text = PasteCodec::write(&value_rows(&expected));

    assert_eq!(expected, PasteCodec::parse(&text), "区切りを含む値が往復する");
    // 正規化は 1 度で止まる（読み直したものを書き直しても同じテキストになる）。
    assert_eq!(
        text,
        PasteCodec::write(&value_rows(&PasteCodec::parse(&text))),
        "テキスト → セル → テキスト の往復が同じ綴りへ戻る"
    );
    // 値なしと空のテキストはテキストの上で区別できない（3.1 の写しと同じ限界）。
    assert_eq!(
        "7\t1.50\t",
        PasteCodec::write(&[vec![
            CellValue::Int(7),
            CellValue::Decimal("1.50".to_owned()),
            CellValue::Null,
        ]])
        .as_str(),
        "表示文字列が唯一の源（10 進数は逐語のまま、値なしは空）"
    );
    assert_eq!(
        rectangle(&[&["7", "1.50", ""]]),
        PasteCodec::parse(&PasteCodec::write(&[vec![
            CellValue::Int(7),
            CellValue::Decimal("1.50".to_owned()),
            CellValue::Null,
        ]])),
        "書いたものが読み直せる"
    );
    assert_eq!(
        "",
        PasteCodec::write(&[]).as_str(),
        "空の矩形は空のテキスト（空のテキストは行 0 件へ戻る）"
    );
}

// ---------------------------------------------------------------------------
// 2. 矩形の貼り付け（要件 7.3, 8.6）
// ---------------------------------------------------------------------------

/// 矩形が錨の行と列から書かれ、**列は右へ、行は下へ**進む（要件 7.3, 8.6）。
///
/// 打ち込まれた文字は上流の規則表で列の型へ強制される（列 1 は `int`、列 2 は `decimal`）ため、
/// 貼り付けの意味論は 1 セルの編集（3.1 の `SetCells`）と同じである。
#[test]
fn a_paste_writes_a_rectangle_of_cells_from_the_anchor() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let first = fixture.row(0);
    let second = fixture.row(1);
    let row_count = fixture.row_count();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            paste(
                CellAddress::new(first, ColumnIndex::new(1)),
                vec![first, second],
                "7\t9\n11\t13",
            ),
        )
        .expect("適合する値の貼り付けは成功する");

    assert_eq!(
        CellValue::Int(7),
        fixture.value_at(first, 1),
        "1 行目の 1 列目（`int` へ強制される）"
    );
    assert_eq!(
        CellValue::Decimal("9".to_owned()),
        fixture.value_at(first, 2),
        "1 行目の 2 列目（`decimal`。綴りは逐語）"
    );
    assert_eq!(CellValue::Int(11), fixture.value_at(second, 1), "2 行目");
    assert_eq!(
        CellValue::Decimal("13".to_owned()),
        fixture.value_at(second, 2),
        "2 行目の 2 列目"
    );
    assert_eq!(vec![first, second], outcome.affected, "書いた順に並ぶ");
    assert_eq!(row_count, outcome.row_count, "既存の行数に収まる貼り付けは行を増やさない");
    assert_eq!(0, outcome.violation_total, "適合する値は違反を生まない");
    assert_eq!(
        vec![
            CoercionNotice {
                cell: CellAddress::new(first, ColumnIndex::new(1)),
                before: "7".to_owned(),
                after: display_text(&CellValue::Int(7)).into_owned(),
            },
            CoercionNotice {
                cell: CellAddress::new(first, ColumnIndex::new(2)),
                before: "9".to_owned(),
                after: display_text(&CellValue::Decimal("9".to_owned())).into_owned(),
            },
            CoercionNotice {
                cell: CellAddress::new(second, ColumnIndex::new(1)),
                before: "11".to_owned(),
                after: display_text(&CellValue::Int(11)).into_owned(),
            },
            CoercionNotice {
                cell: CellAddress::new(second, ColumnIndex::new(2)),
                before: "13".to_owned(),
                after: display_text(&CellValue::Decimal("13".to_owned())).into_owned(),
            },
        ],
        outcome.coercions,
        "打ち込まれた文字が変換されたことが 1 セルずつ載る"
    );
    // 呼び出しの形: 貼り付けた列の再検証 1 回だけ（判定も全件検証も現れない）。
    assert_eq!(
        vec![QueryCall::RevalidateColumns(vec![
            ColumnIndex::new(1),
            ColumnIndex::new(2)
        ])],
        recorded(&calls),
        "貼り付けは貼り付けた列に限定した再検証を 1 回だけ呼ぶ"
    );
}

/// 貼り付けは `rows`（表示されている行の並び）の**錨の行の位置**から歩く。
///
/// 錨より前に並ぶ行は、`rows` に現れていても書かれない（表示の並びの中で錨が起点である）。
#[test]
fn the_paste_starts_at_the_anchor_within_the_displayed_rows() {
    let mut fixture = Fixture::clean(8, 13);
    let sheet = fixture.sheet();
    let (before, middle, after) = (fixture.row(0), fixture.row(1), fixture.row(2));
    let untouched = fixture.value_at(before, 1);
    let (mut apply, _calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            paste(
                CellAddress::new(middle, ColumnIndex::new(1)),
                vec![before, middle, after],
                "7\n9",
            ),
        )
        .expect("貼り付けは成功する");

    assert_eq!(
        vec![middle, after],
        outcome.affected,
        "錨の行とその後ろの行だけが書かれる"
    );
    assert_eq!(untouched, fixture.value_at(before, 1), "錨より前の行は書かれない");
    assert_eq!(CellValue::Int(7), fixture.value_at(middle, 1));
    assert_eq!(CellValue::Int(9), fixture.value_at(after, 1));
}

/// 貼り付けの前に値を読んでおけば、隠れた行が変わっていないことを値で確かめられる
/// （要件 8.9）。可視の行は矩形の**対応する行**を受ける。
#[test]
fn a_filtered_paste_touches_only_the_displayed_rows() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    // 列 4（検査済み）は真偽であり、`true` の行だけを見せる絞り込みを掛ける。
    let visible = visible_rows(
        &fixture,
        &ViewSpec {
            sort: Vec::new(),
            filters: vec![FilterSpec::Equals {
                column: ColumnIndex::new(4),
                text: "true".to_owned(),
            }],
        },
    );
    // 前提: 絞り込みが効いており、**書く範圍の中に隠れた行が挟まっている**。これが無ければ
    // 「隠れた行を飛ばした」ことと「文書順に書いた」ことを区別できない（文書順に書く実装でも
    // 緑になる）。最後の 1 行は矩形から外して、書かれない行の扱いも同時に見る。
    assert!(visible.len() >= 4, "前提: 可視行が複数ある");
    assert!(
        fixture.row_count() > visible.len(),
        "前提: 絞り込みが行を隠している"
    );
    let lines = visible.len() - 1;
    let last_written = visible[lines - 1];
    assert!(
        fixture.position_of(last_written) - fixture.position_of(visible[0]) + 1 > lines,
        "前提: 書く範囲の中に隠れた行が挟まっている"
    );

    let before = fixture.column_values(1);
    let text = (1..=lines)
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let written: Vec<RowId> = visible[..lines].to_vec();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            paste(CellAddress::new(visible[0], ColumnIndex::new(1)), visible.clone(), &text),
        )
        .expect("絞り込み中の貼り付けも成功する");

    // 可視の行は矩形の対応する行を受ける（飛ばした行の数だけ矩形が進む）。
    for (index, row) in written.iter().enumerate() {
        assert_eq!(
            CellValue::Int(index as i64 + 1),
            fixture.value_at(*row, 1),
            "可視の {index} 番目は矩形の {index} 行目を受ける"
        );
    }
    // 書かれなかった行（隠れている行と、矩形から外れた可視の行）は 1 つも変わらない。
    let ids = fixture.ids();
    for (position, row) in ids.iter().enumerate() {
        if written.contains(row) {
            continue;
        }
        assert_eq!(
            before[position],
            fixture.value_at(*row, 1),
            "表示されていない行（と矩形の外の行）は変わらない"
        );
    }
    assert_eq!(written, outcome.affected, "書いた行だけが影響を受けた行");
    assert_eq!(
        fixture.row_count(),
        outcome.row_count,
        "行数に収まる貼り付けは行を増やさない"
    );
    assert_eq!(
        vec![QueryCall::RevalidateColumns(vec![ColumnIndex::new(1)])],
        recorded(&calls)
    );
}

/// 錨の行が表示されている行の並びに現れない場合、**1 つのセルも書かない**（要件 8.9）。
///
/// 錨は表示の選択から写すため（要件 8.6）、表示されていない行を錨にすることは画面からは
/// 起こらない。それでも書かない側に倒すのは、「表示されていない行へ値が届く」経路を
/// 1 つも残さないためである。
#[test]
fn a_paste_anchored_on_a_row_that_is_not_displayed_writes_nothing() {
    let mut fixture = Fixture::clean(16, 13);
    let sheet = fixture.sheet();
    let visible = visible_rows(
        &fixture,
        &ViewSpec {
            sort: Vec::new(),
            filters: vec![FilterSpec::Equals {
                column: ColumnIndex::new(4),
                text: "true".to_owned(),
            }],
        },
    );
    assert!(
        fixture.row_count() > visible.len(),
        "前提: 絞り込みが行を隠している"
    );
    let hidden = fixture
        .all_rows()
        .into_iter()
        .find(|row| !visible.contains(row))
        .expect("前提: 隠れた行がある");
    let before = fixture.snapshot();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            // 隠れた行を錨にし、可視の行は `rows` に並べる（錨が現れない）。
            paste(
                CellAddress::new(hidden, ColumnIndex::new(1)),
                visible,
                "7\n9\n11",
            ),
        )
        .expect("錨が表示されていない貼り付けも成功する");

    assert_eq!(Vec::<RowId>::new(), outcome.affected, "1 行も書かれない");
    assert_eq!(0, outcome.violation_total, "何も変わらないので違反も増えない");
    assert_eq!(before, fixture.snapshot(), "ドキュメントは 1 セルも変わらない");
    assert_eq!(
        Vec::<QueryCall>::new(),
        recorded(&calls),
        "縫い目も呼ばれない"
    );
}

// ---------------------------------------------------------------------------
// 3. 不足する行の補充（要件 7.4）
// ---------------------------------------------------------------------------

/// 貼り付けの範囲が既存の行数を超えるとき、不足する行を末尾へ足して完了する（要件 7.4）。
///
/// 補充した行は `InsertRows` と同じく**宣言の既定値**で作られ、矩形が覆わない列はその既定値の
/// まま残る（要件 6.1 と同じ規律）。
#[test]
fn a_paste_beyond_the_last_row_appends_the_missing_rows() {
    let mut fixture = Fixture::clean(8, 15);
    let sheet = fixture.sheet();
    let last = fixture.row(7);
    let rows = vec![last];
    let row_count = fixture.row_count();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            paste(
                CellAddress::new(last, ColumnIndex::new(1)),
                rows.clone(),
                "7\n9\n11",
            ),
        )
        .expect("不足する行を足して貼り付けは完了する");

    assert_eq!(row_count + 2, outcome.row_count, "足りない 2 行が増える");
    assert_eq!(3, outcome.affected.len(), "書いた 3 行が影響を受けた行");
    // 4.1 の逆命令が保持する「補充された行の `RowId`」は `affected` から取り出せる
    // （`rows` に現れないものが補充された行である）。
    let appended: Vec<RowId> = outcome
        .affected
        .iter()
        .copied()
        .filter(|row| !rows.contains(row))
        .collect();
    assert_eq!(2, appended.len(), "補充された行の識別子が 2 つ取り出せる");
    let ids = fixture.ids();
    assert_eq!(
        appended,
        ids[row_count..].to_vec(),
        "補充は文書の末尾へ足す（既存の行は 1 つも動かない）"
    );
    // 矩形の行が順に届く。
    assert_eq!(CellValue::Int(7), fixture.value_at(last, 1));
    assert_eq!(CellValue::Int(9), fixture.value_at(appended[0], 1));
    assert_eq!(CellValue::Int(11), fixture.value_at(appended[1], 1));
    // 矩形が覆わない列は宣言の既定値（列 13 は `単位`、列 14 は `通貨`。列 3 は既定値なし）。
    assert_eq!(
        CellValue::Text("個".to_owned()),
        fixture.value_at(appended[0], 13)
    );
    assert_eq!(
        CellValue::Text("JPY".to_owned()),
        fixture.value_at(appended[1], 14)
    );
    assert_eq!(CellValue::Null, fixture.value_at(appended[0], 3));
    // 判定の形: 行を補充した貼り付けは**全列**の再検証 1 回だけである（行の集合が変わるため、
    // 貼り付けた列の外の違反も変わりうる。3.2 の行の操作と同じ理由）。
    assert_eq!(
        vec![QueryCall::RevalidateColumns(
            (0..fixture.plan.column_count())
                .map(ColumnIndex::new)
                .collect()
        )],
        recorded(&calls)
    );
    // `violation_total` は**再検証した列に閉じた**総数である。補充した行の覆わない列は違反を
    // 持ちうる（列 0 の品番は必須かつ一意であり、補充した行の値は既定値＝値なし）。
    assert!(
        fixture.sheet_violations() > 0,
        "補充した行の覆わない列は違反を持つ"
    );
    assert_eq!(
        fixture.sheet_violations(),
        outcome.violation_total,
        "全列を再検証したため、適用後のシートの違反の総数と一致する"
    );
}

// ---------------------------------------------------------------------------
// 4. 部分的に適合しない値（要件 7.5）
// ---------------------------------------------------------------------------

/// 適合しない値があっても反映を中止せず、違反として保持したうえで件数を返す（要件 7.5）。
#[test]
fn a_paste_with_values_that_do_not_fit_completes_and_reports_them() {
    let mut fixture = Fixture::clean(8, 13);
    let sheet = fixture.sheet();
    let rows: Vec<RowId> = (0..4).map(|index| fixture.row(index)).collect();
    // 列 1 は範囲つきの `int` である。`abc` と `9999999999999999999999`（範囲外の綴り）は
    // 適合せず、`7` と `9` は適合する。
    let text = "7\nabc\n9\n9999999999999999999999";
    assert_eq!(0, fixture.sheet_violations(), "前提: 貼り付けの前は違反が無い");
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            paste(
                CellAddress::new(rows[0], ColumnIndex::new(1)),
                rows.clone(),
                text,
            ),
        )
        .expect("適合しない値があっても貼り付けは完了する");

    assert_eq!(2, outcome.violation_total, "適合しない 2 件が違反として数えられる");
    assert_eq!(
        CellValue::Int(7),
        fixture.value_at(rows[0], 1),
        "適合する値は反映される"
    );
    assert_eq!(
        CellValue::Text("abc".to_owned()),
        fixture.value_at(rows[1], 1),
        "適合しない値も破棄されず残る"
    );
    assert_eq!(CellValue::Int(9), fixture.value_at(rows[2], 1));
    assert_eq!(
        CellValue::Text("9999999999999999999999".to_owned()),
        fixture.value_at(rows[3], 1),
        "解釈できない綴りも書かれたとおり残る"
    );
    assert_eq!(rows, outcome.affected, "影響を受けた行は 4 行である");
    assert_eq!(
        vec![QueryCall::RevalidateColumns(vec![ColumnIndex::new(1)])],
        recorded(&calls)
    );
    // 違反の集合は再検証した列（貼り付けた列 1 本）の総数である。貼り付けは他の列の値を
    // 1 つも変えず、貼り付けの前は違反が 0 件だったため、シート全体の総数と一致する。
    assert_eq!(
        fixture.sheet_violations(),
        outcome.violation_total,
        "貼り付けた列に閉じた総数が、適用後のシートの総数と一致する"
    );
}

/// 囲みで表した改行を含む値も、1 つのセルへ 1 つの値として届く（要件 7.3, 7.4）。
#[test]
fn a_quoted_value_with_a_newline_lands_in_one_cell() {
    let mut fixture = Fixture::clean(4, 20);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    // 列 19（備考）は最大長 200 の文字列である。
    let (mut apply, _calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            paste(
                CellAddress::new(row, ColumnIndex::new(19)),
                vec![row],
                "\"1 行目\n2 行目\"",
            ),
        )
        .expect("囲みを含む貼り付けも成功する");

    assert_eq!(
        CellValue::Text("1 行目\n2 行目".to_owned()),
        fixture.value_at(row, 19),
        "囲みの中の改行は値の一部であり、行を作らない"
    );
    assert_eq!(vec![row], outcome.affected, "行は 1 つだけ書かれる");
    assert_eq!(0, outcome.violation_total);
}

/// 矩形の行が列数に満たない場合、**覆わない列は触らない**（値なしで埋めない）。
#[test]
fn a_short_rectangle_row_leaves_the_other_columns_alone() {
    let mut fixture = Fixture::clean(4, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let untouched = fixture.value_at(row, 5);
    let (mut apply, _calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            paste(CellAddress::new(row, ColumnIndex::new(1)), vec![row], "7"),
        )
        .expect("1 列だけの貼り付けも成功する");

    assert_eq!(CellValue::Int(7), fixture.value_at(row, 1));
    assert_eq!(
        untouched,
        fixture.value_at(row, 5),
        "矩形が覆わない列は元の値のまま（値なしで埋めない）"
    );
    assert_eq!(0, outcome.violation_total);
}

// ---------------------------------------------------------------------------
// 5. 判定の形（要件 7.7, 11.5 の核）
// ---------------------------------------------------------------------------

/// **1 万行の貼り付け**が、行ごとの判定を 1 回も呼ばずに完了する（要件 7.7, 11.5）。
///
/// 証拠は速度ではなく**呼び出しの形**である。4 行の貼り付けと 1 万行の貼り付けで縫い目へ届く
/// 呼び出しの並びが**同じ**であることを 1 つの表明で固定する（形が行数に依らないことの証拠）。
#[test]
fn a_ten_thousand_row_paste_judges_once_and_never_per_row() {
    let small = Fixture::clean(4, 13);
    let large = Fixture::clean(10_000, 13);

    /// 文書の全行を持つ列へ、行数と同じ行数の矩形を貼り付ける。
    fn paste_all_rows(fixture: &mut Fixture) -> (Vec<QueryCall>, usize, usize) {
        let sheet = fixture.sheet();
        let rows = fixture.all_rows();
        let count = rows.len();
        let text = vec!["1"; count].join("\n");
        let (mut apply, calls) = counting(sheet, fixture.plan.clone());

        let outcome = apply
            .apply(
                fixture.document_mut(),
                paste(
                    CellAddress::new(rows[0], ColumnIndex::new(1)),
                    rows.clone(),
                    &text,
                ),
            )
            .expect("全行への貼り付けは成功する");

        assert_eq!(count, outcome.affected.len(), "全行が影響を受けた行");
        assert_eq!(0, outcome.violation_total, "`1` は列 1 の範囲に適合する");
        assert_eq!(
            CellValue::Int(1),
            fixture.value_at(*rows.last().expect("行がある"), 1),
            "最後の行まで届く"
        );
        (recorded(&calls), outcome.row_count, count)
    }

    let small_shape = paste_all_rows(&mut {
        let mut fixture = small;
        fixture
    });
    assert_eq!(
        vec![QueryCall::RevalidateColumns(vec![ColumnIndex::new(1)])],
        small_shape.0,
        "4 行の貼り付けの呼び出しの形"
    );

    let large_shape = paste_all_rows(&mut {
        let mut fixture = large;
        fixture
    });
    assert_eq!(
        small_shape.0, large_shape.0,
        "1 万行でも呼び出しの形は同じ（行ごとの判定が現れない）"
    );
    assert_eq!(4, small_shape.1, "行を補充しない（全行を覆う矩形）");
    assert_eq!(10_000, large_shape.1, "行を補充しない");
    assert_eq!(10_000, large_shape.2, "要件が名指しする規模そのもの（1 万行）");
}

// ---------------------------------------------------------------------------
// 6. 誤りの経路と部分適用の不在
// ---------------------------------------------------------------------------

/// 貼り付けの誤りは判別可能な変種として返り、**1 つのセルも書かれず、縫い目も 1 回も
/// 呼ばれない**。
#[test]
fn a_broken_paste_writes_nothing_and_calls_no_judgement() {
    let mut fixture = Fixture::clean(16, 13);
    let sheet = fixture.sheet();
    let first = fixture.row(0);
    let foreign = fixture.foreign_row();
    let before = fixture.snapshot();

    let cases: Vec<(EditCommand, GridError)> = vec![
        // 錨の行が文書に無い（他シートの行である）。
        (
            paste(CellAddress::new(foreign, ColumnIndex::new(1)), vec![first], "7"),
            GridError::UnknownRow { row: foreign },
        ),
        // `rows` の中に文書に無い行がある。
        (
            paste(
                CellAddress::new(first, ColumnIndex::new(1)),
                vec![first, foreign],
                "7\n9",
            ),
            GridError::UnknownRow { row: foreign },
        ),
        // 錨の列が範囲外である。
        (
            paste(CellAddress::new(first, ColumnIndex::new(13)), vec![first], "7"),
            GridError::ColumnOutOfRange {
                column: ColumnIndex::new(13),
                count: 13,
            },
        ),
        // 矩形が列をはみ出す（最初の範囲外の列は列数そのもの）。
        (
            paste(
                CellAddress::new(first, ColumnIndex::new(11)),
                vec![first],
                "a\tb\tc",
            ),
            GridError::ColumnOutOfRange {
                column: ColumnIndex::new(13),
                count: 13,
            },
        ),
    ];

    for (command, expected) in cases {
        let (mut apply, calls) = counting(sheet, fixture.plan.clone());
        let error = apply
            .apply(fixture.document_mut(), command)
            .expect_err("壊れた貼り付けは止まる");

        assert_eq!(expected, error, "判別可能な変種として返る");
        assert_eq!(before, fixture.snapshot(), "1 つのセルも書かれない");
        assert_eq!(
            Vec::<QueryCall>::new(),
            recorded(&calls),
            "縫い目も 1 回も呼ばれない"
        );
    }
}

/// 列 0 本のシートでは貼り付けもできない（セッションの前提は命令の中身に依らない）。
#[test]
fn a_paste_needs_a_sheet_with_columns() {
    use document_format::SchemaPart;
    use schema_engine::{SchemaEngine, SchemaEngineApi, TypeRegistry};

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

    let row = document.add_row(sheet).expect("行を追加できない");
    let (mut apply, calls) = counting(sheet, plan);
    let before = snapshot(&document, sheet);
    let error = apply
        .apply(
            &mut document,
            paste(CellAddress::new(row, ColumnIndex::new(0)), vec![row], "7"),
        )
        .expect_err("列 0 本のシートでは貼り付けもできない");

    assert_eq!(GridError::SchemaUnusable { sheet }, error);
    assert_eq!(before, snapshot(&document, sheet), "何も変わらない");
    assert_eq!(
        Vec::<QueryCall>::new(),
        recorded(&calls),
        "縫い目も呼ばれない"
    );
}

// ---------------------------------------------------------------------------
// 7. 空の貼り付けと決定性
// ---------------------------------------------------------------------------

/// 行 0 件のテキストと、表示されている行が 1 つも無い場合は、成功し、何も変えず、
/// **縫い目を 1 回も呼ばない**（3.1 の空の命令と同じ規則）。
#[test]
fn an_empty_paste_changes_nothing_and_calls_nothing() {
    let mut fixture = Fixture::clean(8, 13);
    let sheet = fixture.sheet();
    let row = fixture.row(0);
    let row_count = fixture.row_count();
    let before = fixture.snapshot();

    for command in [
        // 空のテキストは行 0 件である。
        paste(CellAddress::new(row, ColumnIndex::new(1)), vec![row], ""),
        // 区切りだけのテキストも 1 つの値（空の文字列）を持つ 1 行である — ここでは
        // 貼り付ける列が無い場合として列 0 本を渡せないため、`rows` が空の場合で確かめる。
        paste(CellAddress::new(row, ColumnIndex::new(1)), Vec::new(), "7\n9"),
    ] {
        let (mut apply, calls) = counting(sheet, fixture.plan.clone());
        let outcome = apply
            .apply(fixture.document_mut(), command)
            .expect("空の貼り付けも成功する");

        assert_eq!(
            Vec::<RowId>::new(),
            outcome.affected,
            "影響を受けた行は無い"
        );
        assert_eq!(0, outcome.violation_total);
        assert_eq!(row_count, outcome.row_count);
        assert_eq!(before, fixture.snapshot(), "ドキュメントは変わらない");
        assert_eq!(
            Vec::<QueryCall>::new(),
            recorded(&calls),
            "縫い目も呼ばれない"
        );
    }
}

/// 同じドキュメントと同じ命令の 2 回の適用は同じ結果を返し、状態も変えない（行を補充しない
/// 貼り付けについて）。
#[test]
fn pasting_the_same_rectangle_twice_gives_the_same_outcome() {
    let mut fixture = Fixture::clean(8, 13);
    let sheet = fixture.sheet();
    let rows = vec![fixture.row(1), fixture.row(2)];
    let command = paste(
        CellAddress::new(rows[0], ColumnIndex::new(1)),
        rows,
        "7\n9",
    );
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
}

/// **行ごとに幅が違う矩形**（いわゆる ragged）でも、覆わない列は触らず、違反の件数は
/// **実際に書いたセル**から数える。
///
/// 上の `a_short_rectangle_row_leaves_the_other_columns_alone` は 1×1 の矩形だけを見るため、
/// **短い行の詰め物**（覆っていない列を値なしで埋める実装）と、**幅を最初の行の長さで決める**
/// 実装の双方を素通りさせる（レビューの変異試験が実測）。ここでは幅の違う 2 行を貼り、
/// (1) 短い行が覆わない列は元の値のままであること、(2) 幅は**最も広い行**で決まり、
/// 広い行の違反が件数に現れることを固定する。
#[test]
fn a_ragged_rectangle_leaves_uncovered_columns_alone_and_counts_every_written_cell() {
    let mut fixture = Fixture::clean(4, 13);
    let sheet = fixture.sheet();
    let first = fixture.row(0);
    let second = fixture.row(1);

    // 前提: 覆わない列の元の値は値なしではない（「元のまま」と「値なしで埋めた」を区別できる）。
    let untouched_first = fixture.value_at(first, 3);
    let untouched_second = fixture.value_at(second, 3);
    assert_ne!(
        CellValue::Null,
        untouched_first,
        "前提が崩れた: 列 3 の元の値が値なしであり、埋めた場合と区別できない"
    );
    assert_ne!(
        CellValue::Null,
        untouched_second,
        "前提が崩れた: 列 3 の元の値が値なしであり、埋めた場合と区別できない"
    );

    // 1 行目は 1 列だけ（`7`）、2 行目は 3 列（うち列 3 が適合しない `"zzz"`）。
    // 幅は**最も広い行**（3）で決まるため、2 行目の列 3 まで書かれ、その違反が数えられる。
    let (mut apply, _calls) = counting(sheet, fixture.plan.clone());
    let outcome = apply
        .apply(
            fixture.document_mut(),
            paste(
                CellAddress::new(first, ColumnIndex::new(1)),
                vec![first, second],
                "7\n7\t7\tzzz",
            ),
        )
        .expect("幅の違う矩形の貼り付けも成功する");

    // (1) 短い行が覆わない列は元の値のままである（値なしで埋めていない）。
    assert_eq!(
        untouched_first,
        fixture.value_at(first, 3),
        "短い行が覆わない列が書き換えられている（値なしで埋めている）"
    );
    // (2) 幅は最も広い行で決まり、広い行の 3 列目まで書かれる。
    assert_eq!(CellValue::Int(7), fixture.value_at(first, 1));
    assert_eq!(CellValue::Int(7), fixture.value_at(second, 1));
    // 列 2 は 10 進数の列であり、打たれた `7` はそのまま 10 進数として保持される
    // （型解釈は `schema-engine` の領分である）。
    assert_eq!(
        CellValue::Decimal("7".to_owned()),
        fixture.value_at(second, 2),
        "列 2（10 進数）へ書かれる"
    );
    assert_eq!(
        CellValue::Text("zzz".to_owned()),
        fixture.value_at(second, 3),
        "最も広い行の覆う列まで書かれる"
    );
    // (3) 違反の件数は**実際に書いたセル**から数える（広い行の適合しない値が落ちない）。
    assert_eq!(
        1, outcome.violation_total,
        "最も広い行の適合しない値が違反として数えられていない"
    );
    assert_eq!(2, outcome.affected.len(), "書いた行は 2 つ");
}

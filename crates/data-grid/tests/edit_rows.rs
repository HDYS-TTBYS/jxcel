//! 行の追加・削除・複製の適用（データグリッドのタスク 3.2。data-grid 要件 6.1, 6.2, 6.3, 6.4）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のものである。
//!
//! 1. **行の追加は、宣言が供給する既定値を新しい行へ入れる**（要件 6.1）。値が与えられていない
//!    列には `CompiledSchema::default_row()` の値がそのまま入る（本層が既定値を発明しないことを、
//!    **値そのもの**の突き合わせで示す。件数だけでは足りない）。挿入位置は**文書の位置**であり、
//!    隣の行を突き合わせて確かめる。
//! 2. **挿入位置の空間**。`at` は**文書の順の位置**として解釈され、可視の序数ではない。画面の
//!    位置に挿入したい呼び出し側は `RowOrder` で行そのものへ写してから文書の位置を渡す。この
//!    決定を、並べ替えを効かせた順序（可視の序数と文書の位置が食い違う順序）で**取り違えれば
//!    落ちる**形で固定する（要件 8.6 の帰結）。
//! 3. **行の削除は 1 回の操作である**（要件 6.2）。1 つの命令で選択されたすべての行が取り除かれ、
//!    `affected` は取り除かれた行を**シート順**に持つ（同じ行が 2 度現れる要求は 1 回へ畳む）。
//!    残った行は相対順と値を保つ。削除の 1 回性は 2 つの独立の観測で固定する: **再検証が 1 回**で
//!    あることと、**不正な行が 1 つ混ざったときに 1 行も取り除かれない**こと（行ごとに削除する
//!    実装は、先の行を取り除いてから失敗する）。
//! 4. **複製は同じ値を持つ行を末尾へ足す**（要件 6.3）。値は元の行と 1 つも違わない（列数に
//!    満たない行も含めて、値の並びがそのまま複製される）。
//! 5. **一意制約の重複は中止せず違反として報告される**（要件 6.4）。一意制約つきの列を含む行を
//!    複製すると、**行が増えたうえで**重複が違反として報告され、命令は成功する。総数だけでなく
//!    **違反の理由と、重複する行の識別子**まで確かめる。
//! 6. **行数の変化**。追加・削除・複製のいずれも `row_count` に現れ、`affected` は増減した行を
//!    持つ（`row_count` は適用の**後**の数である）。
//! 7. **誤りの経路と部分適用の不在**。未知の行・行数を超える挿入位置は [`GridError`] の判別可能な
//!    変種として返り、**1 行も増減せず、縫い目も呼ばれない**。
//! 8. **空の命令**。行 0 件の削除・複製と件数 0 の追加は成功し、何も変えず、縫い目を 1 回も呼ばない
//!    （3.1 の「空の命令は何も変えない」と同じ規則）。ただし**セッションの前提は空の命令でも
//!    検査する**（列 0 本のシートでは空の命令も [`GridError::SchemaUnusable`] になる）。
//! 9. **決定性**。同じ形・同じ値の文書と同じ命令は、同じ構造の結果を与える（生の識別子は実行ごとに
//!    変わるため比較しない。標本の契約）。
//! 10. **再検証の形**。行の構造を変える命令は、**すべての列を指定した再検証を 1 回だけ**呼び、
//!     書き込みの判定とシート全件の検証は**呼ばない**（既定値も複製の値も打たれた文字ではなく、
//!     宣言とドキュメントが供給する値である）。要件 11.4 が禁じるのは **1 セルの編集についての**
//!     全件検証であり（編集のたびに掛かる費用）、行の構造を変える命令はそれに当たらない
//!     （`edit` のモジュール docs「行の構造を変える命令の再検証」）。
//!
//! # 前提を先に確かめる
//!
//! 標本（`tests/common/sample.rs`）を使う検査は、**標本がその検査の前提を満たしていることを
//! 最初に検査する**（tasks.md の Implementation Notes の規則）。本ファイルは [`Fixture::clean`] で
//! 次を確かめてから依拠する。
//!
//! - 違反を 1 件も仕込んでいない（`injected_violations` / `unique_violations` が 0）こと。
//! - 列 0 が**唯一の一意制約つきの列**であり、重複が 1 件も無い（**零の同点**）こと。
//! - **文書の行順が行識別子の順と一致する**こと（標本は行識別子を発行順に並べる）。
//! - 全件検証の報告が 0 件であること（標本は適合する値だけで組み立てられている）。
//!
//! **標本の識別子は発行のたびに変わる**ため、生の識別子や生の値を実行を跨いで比較しない。
//! 比較するのは**構造**（呼び出しの形・行数・文書の位置・違反の位置と理由）である
//! （tasks.md の Implementation Notes の規則）。
//!
//! # 数える縫い目
//!
//! 要件 11.4 の観測の縫い目（[`EditSchemaQuery`]）を本ファイルも使う。行の構造を変える命令が
//! **何を呼び、何を呼ばないか**を数えるためであり、数える先は**本番の実装そのもの**
//! （[`SchemaEngineQuery`]）である（`structure.md`「本番の一括経路からそれが呼ばれていることを
//! 示すこと」）。

mod common;

use std::sync::{Arc, Mutex};

use common::sample::sample;
use common::sample::{SampleEditParts, SampleOptions};
use data_grid::{
    display_text, CellAddress, CoercionNotice, ColumnIndex, EditApply, EditCommand, EditSchemaQuery,
    GridError, RowOrder, RowOrdinal, RowSpan, SchemaEngineQuery, SortKey, ViewSpec,
};
use document_format::{CellValue, Document, RowId, SheetId};
use schema_engine::{
    validate_columns, validate_sheet, CompiledSchema, EditVerdict, SheetReport, ValidationOptions,
    ViolationReason,
};

// ---------------------------------------------------------------------------
// 縫い目へ届いた呼び出しの記録
// ---------------------------------------------------------------------------

/// 縫い目へ届いた 1 回の呼び出し（到着順に並べる）。
///
/// 3 変種は縫い目の 3 つの操作に 1 対 1 で対応し、**呼ばれたものをそのまま記録する**
/// （記録から漏れる呼び出しが無いことが「全列の再検証が 1 回だけ」「判定は呼ばない」の前提で
/// ある）。
#[derive(Debug, Clone, PartialEq)]
enum QueryCall {
    /// 書き込みの判定。渡った 1 行分の値を運ぶ。**行の操作は呼ばない**。
    JudgeWrite(Vec<CellValue>),
    /// 列を限定した再検証。渡った列の集合を運ぶ。
    RevalidateColumns(Vec<ColumnIndex>),
    /// シート全件の検証。**編集経路は呼ばない**（要件 11.4）。
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

/// すべての列（行の構造を変える命令が再検証を指定する集合）。
fn every_column(plan: &CompiledSchema) -> Vec<ColumnIndex> {
    (0..plan.column_count()).map(ColumnIndex::new).collect()
}

/// 行の操作の再検証の形（全列の再検証 1 回だけ。判定も全件検証も呼ばない）。
fn assert_row_operation_call_shape(calls: &Arc<Mutex<Vec<QueryCall>>>, plan: &CompiledSchema) {
    assert_eq!(
        vec![QueryCall::RevalidateColumns(every_column(plan))],
        recorded(calls),
        "行の構造を変える命令は全列の再検証を 1 回だけ呼ぶ"
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
    /// 違反を 1 件も仕込まない標本（前提を先に確かめてから依拠する）。
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

    /// 行の値（列の添字の並び）。
    fn values_of(&self, row: RowId) -> Vec<CellValue> {
        values_of(self.document(), self.sheet(), row)
    }

    /// ドキュメント全体の行識別子と値（適用の前後の比較用）。
    fn snapshot(&self) -> Vec<(RowId, Vec<CellValue>)> {
        snapshot(self.document(), self.sheet())
    }

    /// 行の文書の位置。
    fn position_of(&self, row: RowId) -> usize {
        position_of(self.document(), self.sheet(), row)
    }

    /// 列 `column` の値の並び（文書順）。参照を運ばない列（0 と 1）の決定性の比較に使う。
    fn column_values(&self, column: usize) -> Vec<CellValue> {
        self.snapshot()
            .into_iter()
            .map(|(_, values)| values.get(column).cloned().unwrap_or(CellValue::Null))
            .collect()
    }

    /// 適用後のシートの違反の総数（全件検証。行の操作の `violation_total` の期待値）。
    fn violations_now(&self) -> usize {
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
fn snapshot(document: &Document, sheet: SheetId) -> Vec<(RowId, Vec<CellValue>)> {
    document
        .sheet_by_id(sheet)
        .expect("シートは文書にある")
        .rows()
        .iter()
        .map(|row| (row.id(), row.values().to_vec()))
        .collect()
}

/// 行の値をドキュメントから読み戻す。
fn values_of(document: &Document, sheet: SheetId, row: RowId) -> Vec<CellValue> {
    document
        .sheet_by_id(sheet)
        .expect("シートは文書にある")
        .rows()
        .iter()
        .find(|found| found.id() == row)
        .expect("行はシートにある")
        .values()
        .to_vec()
}

/// 行の文書の位置。
fn position_of(document: &Document, sheet: SheetId, row: RowId) -> usize {
    document
        .sheet_by_id(sheet)
        .expect("シートは文書にある")
        .rows()
        .iter()
        .position(|found| found.id() == row)
        .expect("行はシートにある")
}

/// 他シートの行（データシートに属さない行。誤りの経路の検査に使う）。
fn foreign_row(fixture: &Fixture) -> RowId {
    fixture
        .document()
        .sheet_by_id(fixture.parts.reference)
        .expect("参照先シートは文書にある")
        .rows()[0]
        .id()
}

/// 選択された行の要求（文書の位置で 7・2・2・40。同じ行の 2 度の要求と、シート順と逆の並びを
/// 含む。要求の並びが結果へ漏れないことを見るために使う）。
fn selection(fixture: &Fixture) -> Vec<RowId> {
    vec![fixture.row(7), fixture.row(2), fixture.row(2), fixture.row(40)]
}

// ---------------------------------------------------------------------------
// 1. 行の追加（要件 6.1）
// ---------------------------------------------------------------------------

/// 行の追加は、**宣言が供給する既定値**を新しい行へ入れ、文書の位置 `at` に置く（要件 6.1）。
#[test]
fn inserting_rows_applies_the_schema_defaults_at_the_documented_position() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let count = 3;
    let at = 5;
    let before = fixture.ids();
    // 前提: 宣言が実際に既定値を供給している（そうでなければ「既定値を適用した」検査が空振り
    // する）。既定値の源は計画（`default_row`）ただ 1 つである。
    let defaults = fixture.plan.default_row();
    assert_eq!(
        fixture.plan.column_count(),
        defaults.len(),
        "既定値の列はすべての列を覆う"
    );
    assert_eq!(
        CellValue::Int(1),
        defaults[1],
        "標本の列 1（数量）は既定値 `Int(1)` を宣言している"
    );
    assert_eq!(
        CellValue::Null,
        defaults[3],
        "標本の列 3（予備率）は既定値を宣言していない（値なしが入る）"
    );
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::InsertRows {
                at: RowOrdinal::new(at),
                count,
            },
        )
        .expect("行の追加は成功する");

    // 行数の変化（要件 6.1 の「行数の変化を結果に含める」）。
    assert_eq!(before.len() + count, outcome.row_count, "適用後の行数");
    let after = fixture.ids();
    assert_eq!(before.len() + count, after.len(), "実際に行が増えた");
    // `affected` は挿入された行そのもの（文書の順）。
    let inserted: Vec<RowId> = after
        .iter()
        .copied()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(inserted, outcome.affected, "affected は挿入された行");
    assert_eq!(
        inserted,
        after[at..at + count].to_vec(),
        "挿入された行は文書の位置 `at` に並ぶ"
    );

    // **位置**（隣の行を突き合わせる。ずれた位置に挿入する誤りを捕まえる）。
    assert_eq!(before[at - 1], after[at - 1], "挿入位置の直前の行は動かない");
    assert_eq!(
        before[at],
        after[at + count],
        "挿入位置の直後の行は挿入された行の後ろへずれる"
    );
    assert_eq!(
        before[..at].to_vec(),
        after[..at].to_vec(),
        "挿入位置より前の行は 1 つも動かない"
    );
    assert_eq!(
        before[at..].to_vec(),
        after[at + count..].to_vec(),
        "挿入位置より後ろの行は順序を保ったまま後ろへずれる"
    );

    // **値**（既定値そのものであること。件数だけでは足りない）。
    for row in &inserted {
        assert_eq!(
            defaults,
            fixture.values_of(*row),
            "新しい行の値は宣言の既定値そのもの"
        );
        // 検査が空振りしていないこと（隣の行は既定値と同じ値を持たない）。
        assert_ne!(defaults, fixture.values_of(before[at]));
    }

    // 既定値の適用で生じる違反（値なしを許さない列の値なし）が、そのまま報告される。本層が
    // 既定値を発明していないことは、この数が**宣言から導ける数**であることで示される。
    let missing_required = (0..fixture.plan.column_count())
        .filter(|column| {
            fixture.plan.required(ColumnIndex::new(*column))
                && matches!(defaults[*column], CellValue::Null)
        })
        .count();
    assert!(missing_required > 0, "前提: 既定値を持たない必須の列がある");
    assert_eq!(
        count * missing_required,
        outcome.violation_total,
        "既定値の適用で生じる違反の数は宣言から導ける"
    );
    assert!(outcome.coercions.is_empty(), "既定値は打たれた文字ではない");
    assert_row_operation_call_shape(&calls, &fixture.plan);
}

/// 挿入位置は**文書の位置**であり、可視の序数ではない。画面の位置に挿入したい呼び出し側は
/// `RowOrder` で行そのものへ写してから文書の位置を渡す（要件 8.6 の帰結）。
///
/// 並べ替えを効かせた順序では、可視の序数 `k` が指す行は文書の位置 `k` には無い。したがって
/// 可視の序数をそのまま渡せば**別の場所**に入り、写してから渡せば意図した行の直前に入る。
/// 2 つの空間を取り違えた実装は、この検査のどちらかで落ちる。
#[test]
fn an_insert_position_is_a_document_position_and_the_caller_translates_the_visible_ordinal() {
    // 並べ替えを効かせた順序（列 1 の降順）と、可視の序数と文書の位置が食い違う行を用意する。
    // 並べ替えは表示に閉じるため、ドキュメントの並びは変わらない（要件 8.5）。
    let reordered = |fixture: &Fixture| {
        let mut order = RowOrder::default();
        order.recompute(
            fixture.document(),
            fixture.sheet(),
            &ViewSpec {
                sort: vec![SortKey {
                    column: ColumnIndex::new(1),
                    descending: true,
                }],
                filters: Vec::new(),
            },
        );
        order
    };
    let a_displayed_row_that_moved = |fixture: &Fixture| {
        let ids = fixture.ids();
        let order = reordered(fixture);
        let ordinal = (0..ids.len())
            .find(|k| order.row_at(RowOrdinal::new(*k)) != Some(ids[*k]))
            .expect("見えている位置と文書の位置が食い違う行が無い");
        let displayed = order
            .row_at(RowOrdinal::new(ordinal))
            .expect("可視の序数に行がある");
        // 対照: 見えている位置と文書の位置が食い違っている（食い違わなければ、この検査は
        // 2 つの空間の取り違えを捕まえられない）。
        assert_ne!(
            ordinal,
            fixture.position_of(displayed),
            "対照: 可視の序数が指す行は文書の位置には無い"
        );
        (ordinal, displayed)
    };

    // (1) 可視の序数を**写さずに**渡すと、文書の位置として解釈される。
    let mut raw = Fixture::clean(64, 13);
    let sheet = raw.sheet();
    let (ordinal, displayed) = a_displayed_row_that_moved(&raw);
    let ids = raw.ids();
    let mut apply = EditApply::new(sheet, raw.plan.clone());

    let outcome = apply
        .apply(
            raw.document_mut(),
            EditCommand::InsertRows {
                at: RowOrdinal::new(ordinal),
                count: 1,
            },
        )
        .expect("行の追加は成功する");

    let inserted = outcome.affected[0];
    assert_eq!(
        ordinal,
        raw.position_of(inserted),
        "`at` は文書の位置として解釈される"
    );
    assert_eq!(
        ids[ordinal],
        raw.ids()[ordinal + 1],
        "直後の行は文書の位置の行である"
    );
    assert_ne!(
        displayed,
        raw.ids()[ordinal + 1],
        "画面に見えている行の直前には入らない（写さなければ別の場所に入る）"
    );

    // (2) 画面の位置に挿入したい呼び出し側は、行の順序で写してから渡す。
    let mut translated = Fixture::clean(64, 13);
    let sheet = translated.sheet();
    let (ordinal, displayed) = a_displayed_row_that_moved(&translated);
    // 可視の序数 `ordinal` が指す**行そのもの**（`RowOrder` が与える）を、文書の位置へ写す。
    let at = translated.position_of(displayed);
    assert_ne!(ordinal, at, "写す前と後で位置が変わる（写しが要る）");
    let mut apply = EditApply::new(sheet, translated.plan.clone());

    let outcome = apply
        .apply(
            translated.document_mut(),
            EditCommand::InsertRows {
                at: RowOrdinal::new(at),
                count: 1,
            },
        )
        .expect("行の追加は成功する");

    let inserted = outcome.affected[0];
    assert_eq!(
        at,
        translated.position_of(inserted),
        "写した文書の位置に行が入る"
    );
    assert_eq!(
        translated.position_of(inserted) + 1,
        translated.position_of(displayed),
        "意図した行の**直前**に入る（画面の位置に挿入できた）"
    );
}

// ---------------------------------------------------------------------------
// 2. 行の削除（要件 6.2）
// ---------------------------------------------------------------------------

/// 選択された複数の行の削除を**1 回の操作**として適用し、残った行は相対順と値を保つ（要件 6.2）。
#[test]
fn removing_several_rows_is_one_operation_and_the_rest_keep_their_order_and_values() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let ids = fixture.ids();
    let before = fixture.snapshot();
    // 同じ行が 2 度現れる要求と、シート順と逆の並びを含める。
    let requested = selection(&fixture);
    let removed = vec![ids[2], ids[7], ids[40]];
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::RemoveRows { rows: requested },
        )
        .expect("行の削除は成功する");

    // `affected` は取り除かれた行を**シート順**に持つ（同じ行の 2 度の要求は 1 回へ畳まれる）。
    assert_eq!(removed, outcome.affected, "affected は取り除かれた行");
    assert_eq!(ids.len() - removed.len(), outcome.row_count, "行数が減る");
    // 残った行は相対順と値を保つ（要求の並びに依らない）。
    let expected: Vec<(RowId, Vec<CellValue>)> = before
        .iter()
        .filter(|(id, _)| !removed.contains(id))
        .cloned()
        .collect();
    assert_eq!(expected, fixture.snapshot(), "残った行の順序と値");
    assert_eq!(
        Vec::<CoercionNotice>::new(),
        outcome.coercions,
        "行の削除は変換を生まない"
    );
    // **1 回の操作**であることの 1 つ目の観測: 再検証は 1 回だけである（行ごとに削除して
    // 1 行ずつ再検証すれば複数回になる）。
    assert_row_operation_call_shape(&calls, &fixture.plan);
    assert_eq!(0, outcome.violation_total, "適合する値だけの標本では違反が無い");
}

/// 1 回の操作であることの 2 つ目の観測: 不正な行が 1 つ混ざった命令は、**先の行も取り除かない**
/// （行ごとに削除する実装は、妥当な行を取り除いてから失敗する）。
#[test]
fn a_removal_that_contains_an_unknown_row_removes_nothing() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let foreign = foreign_row(&fixture);
    // 妥当な行を先に置く（部分適用があれば、この行が消えてから失敗する）。
    let command = EditCommand::RemoveRows {
        rows: vec![fixture.row(3), foreign],
    };
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before = fixture.snapshot();

    let error = apply
        .apply(fixture.document_mut(), command)
        .expect_err("他シートの行を指す命令は失敗する");

    assert_eq!(GridError::UnknownRow { row: foreign }, error);
    assert_eq!(before, fixture.snapshot(), "1 行も取り除かれない");
    assert_eq!(
        Vec::<QueryCall>::new(),
        recorded(&calls),
        "縫い目も呼ばれない"
    );
}

/// 取り除いた行をもう一度削除しようとすると、その行はもう属していないので失敗し、状態は変わらない。
#[test]
fn removing_the_same_selection_again_changes_nothing() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let requested = selection(&fixture);
    let (mut apply, _calls) = counting(sheet, fixture.plan.clone());

    apply
        .apply(
            fixture.document_mut(),
            EditCommand::RemoveRows {
                rows: requested.clone(),
            },
        )
        .expect("行の削除は成功する");
    let after_first = fixture.snapshot();
    let error = apply
        .apply(
            fixture.document_mut(),
            EditCommand::RemoveRows { rows: requested },
        )
        .expect_err("取り除かれた行を再び削除するのは失敗する");

    assert!(
        matches!(error, GridError::UnknownRow { .. }),
        "取り除かれた行の再削除は未知の行: {error:?}"
    );
    assert_eq!(after_first, fixture.snapshot(), "何も変わらない");
}

// ---------------------------------------------------------------------------
// 3. 行の複製（要件 6.3, 6.4）
// ---------------------------------------------------------------------------

/// 複製は元の行と**同じ値**を持つ行を末尾へ足す（要件 6.3）。同じ行の 2 度の要求は 1 回へ畳む。
#[test]
fn duplicating_rows_copies_the_values_and_appends_them_at_the_end() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let ids = fixture.ids();
    // シート順の期待値を作る（要求の並びに依らない）。
    let sources = vec![ids[3], ids[5]];
    let expected_values = vec![fixture.values_of(ids[3]), fixture.values_of(ids[5])];
    let requested = vec![ids[5], ids[3], ids[5]];
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::DuplicateRows { rows: requested },
        )
        .expect("行の複製は成功する");

    assert_eq!(sources.len(), outcome.affected.len(), "複製の数");
    assert_eq!(ids.len() + sources.len(), outcome.row_count, "行数が増える");
    let after = fixture.ids();
    assert_eq!(ids, after[..ids.len()].to_vec(), "元の行は 1 つも動かない");
    assert_eq!(
        outcome.affected,
        after[ids.len()..].to_vec(),
        "複製は末尾に置かれる"
    );
    assert_eq!(
        expected_values,
        outcome
            .affected
            .iter()
            .map(|row| fixture.values_of(*row))
            .collect::<Vec<_>>(),
        "複製は元の行と同じ値を持つ（シート順）"
    );
    for (copy, source) in outcome.affected.iter().zip(&sources) {
        assert_eq!(
            fixture.values_of(*source),
            fixture.values_of(*copy),
            "複製の値は元の行と 1 つも違わない"
        );
        assert_ne!(*source, *copy, "複製は別の行である");
    }
    assert_eq!(
        sources.len(),
        outcome.violation_total,
        "同じ値の行を複製すると、一意制約つきの列に値ごとに 1 件の重複が生じる（要件 6.4）"
    );
    assert_row_operation_call_shape(&calls, &fixture.plan);
}

/// 列数に満たない値しか持たない行の複製も、**同じ値の並び**を複製する（穴を埋めない）。
///
/// 値を持たない列（行の値の数が列の添字に届かない列）は、上流でも値なしとして扱われる。複製が
/// 列数まで値を埋めると、元の行と複製の値の並びが食い違い、保存（行データの符号化）で
/// 列数の不一致として現れる。
#[test]
fn duplicating_a_short_row_copies_exactly_the_values_it_has() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let row_count = fixture.row_count();
    // 新しい行は値を持たない。列 1 だけを書くと、その行の値は 2 個（列 0 は値なし）になる。
    let short = {
        let document = fixture.document_mut();
        let row = document.add_row(sheet).expect("行を追加できない");
        row
    };
    let mut apply = EditApply::new(sheet, fixture.plan.clone());
    apply
        .apply(
            fixture.document_mut(),
            EditCommand::SetCells {
                cells: vec![(
                    CellAddress::new(short, ColumnIndex::new(1)),
                    "7".to_owned(),
                )],
            },
        )
        .expect("適合する値の書き込みは成功する");
    // 前提: 値の並びが列数に満たない（この検査の対象そのもの）。
    let source_values = fixture.values_of(short);
    assert!(
        source_values.len() < fixture.plan.column_count(),
        "前提: 値の並びが列数に満たない行がある（{} 個）",
        source_values.len()
    );
    let violations_before = fixture.violations_now();

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::DuplicateRows { rows: vec![short] },
        )
        .expect("行の複製は成功する");

    let copy = outcome.affected[0];
    assert_eq!(row_count + 2, outcome.row_count, "追加した行と複製");
    assert_eq!(
        source_values,
        fixture.values_of(copy),
        "複製の値の並びは元の行と同一（列数まで埋めない）"
    );
    assert_eq!(
        2 * violations_before,
        outcome.violation_total,
        "同じ値を持つ行がもう 1 行でき、行ごとの違反が 2 倍になる"
    );
}

/// 一意制約を持つ列を含む行を複製すると、**行が増えたうえで**重複が違反として報告され、
/// 命令は中止しない（要件 6.4）。
#[test]
fn duplicating_a_row_that_holds_a_unique_value_reports_the_duplicate_without_aborting() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let ids = fixture.ids();
    // 前提: 標本で唯一の一意制約つきの列は列 0 であり、複製の前は違反が 1 件も無い。
    assert_eq!(
        vec![ColumnIndex::new(0)],
        fixture.plan.unique_columns().to_vec(),
        "一意制約つきの列は列 0 だけ"
    );
    assert_eq!(0, fixture.violations_now(), "複製の前は違反が無い");
    let source = ids[11];
    assert!(
        !matches!(fixture.values_of(source)[0], CellValue::Null),
        "前提: 複製する行は一意制約つきの列に値を持つ"
    );
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::DuplicateRows { rows: vec![source] },
        )
        .expect("一意制約に重複が生じても中止しない（要件 6.4）");

    // 行が増えたうえで、重複が違反として報告される。
    assert_eq!(ids.len() + 1, outcome.row_count, "行が増える");
    assert_eq!(1, outcome.affected.len(), "複製が 1 行足された");
    assert_eq!(1, outcome.violation_total, "重複が違反として報告される");
    let copy = outcome.affected[0];
    assert_eq!(
        fixture.values_of(source),
        fixture.values_of(copy),
        "複製は同じ値を持つ（同じ一意値を持つ）"
    );

    // 総数だけでなく、報告が**一意制約の違反**であることと、重複する行の識別子まで確かめる。
    let report = validate_columns(
        fixture.document(),
        sheet,
        &fixture.plan,
        &[ColumnIndex::new(0)],
        &ValidationOptions::unlimited(),
    );
    assert_eq!(1, report.total_violations(), "一意制約の違反は 1 件");
    match report.violations()[0].reason() {
        ViolationReason::Duplicate { rows, actual, .. } => {
            assert_eq!(
                &vec![source, copy],
                rows,
                "重複するすべての行（元の行と複製）"
            );
            assert_eq!(&fixture.values_of(source)[0], actual, "重複した値");
        }
        other => panic!("一意制約の重複として報告されていない: {other:?}"),
    }
    assert_row_operation_call_shape(&calls, &fixture.plan);
}

/// 適合しない値を持つ行の複製は、**その違反ごと**複製する（本層が値を判定で書き換えないこと）。
#[test]
fn duplicating_a_row_that_holds_a_violating_value_copies_the_violation_itself() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    // 列 4（検査済み）は `bool` であり、`yes` は変換されず適合しない値になる（3.1 の経路）。
    let row = fixture.row(2);
    let mut apply = EditApply::new(sheet, fixture.plan.clone());
    apply
        .apply(
            fixture.document_mut(),
            EditCommand::SetCells {
                cells: vec![(CellAddress::new(row, ColumnIndex::new(4)), "yes".to_owned())],
            },
        )
        .expect("適合しない値の書き込みも成功する");
    let violating = fixture.values_of(row);
    assert_eq!(
        CellValue::Text("yes".to_owned()),
        violating[4],
        "前提: 適合しない値がドキュメントに残っている"
    );
    assert_eq!(1, fixture.violations_now(), "前提: 違反が 1 件ある");

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::DuplicateRows { rows: vec![row] },
        )
        .expect("行の複製は成功する");

    let copy = outcome.affected[0];
    assert_eq!(
        violating,
        fixture.values_of(copy),
        "違反を持つ行の複製も同じ値を持つ（判定で書き換えない）"
    );
    // 報告の中身まで確かめる: 適合しない値の違反が 2 件（元の行と複製）と、一意制約の重複が
    // 1 件（複製が同じ品番を持つため）である。
    let report = validate_sheet(
        fixture.document(),
        sheet,
        &fixture.plan,
        &ValidationOptions::unlimited(),
    );
    let reasons: Vec<&ViolationReason> = report
        .violations()
        .iter()
        .map(|violation| violation.reason())
        .collect();
    assert_eq!(
        2,
        reasons
            .iter()
            .filter(|reason| matches!(reason, ViolationReason::TypeMismatch { .. }))
            .count(),
        "適合しない値の違反が元の行と複製の 2 件"
    );
    assert_eq!(
        1,
        reasons
            .iter()
            .filter(|reason| matches!(reason, ViolationReason::Duplicate { .. }))
            .count(),
        "一意制約の重複の違反が 1 件"
    );
    assert_eq!(3, outcome.violation_total, "違反の総数は 3 件");
}

// ---------------------------------------------------------------------------
// 4. 誤りの経路と部分適用の不在
// ---------------------------------------------------------------------------

/// シートに属さない行を含む複製の命令は [`GridError::UnknownRow`] を返し、**1 行も増やさない**。
#[test]
fn an_unknown_row_stops_the_duplication_before_any_change() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let foreign = foreign_row(&fixture);
    // 妥当な行を先に置く（部分適用があれば、この行が複製されてから失敗する）。
    let command = EditCommand::DuplicateRows {
        rows: vec![fixture.row(3), foreign],
    };
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before = fixture.snapshot();

    let error = apply
        .apply(fixture.document_mut(), command)
        .expect_err("他シートの行を指す命令は失敗する");

    assert_eq!(GridError::UnknownRow { row: foreign }, error);
    assert_eq!(before, fixture.snapshot(), "1 行も増えない");
    assert_eq!(
        Vec::<QueryCall>::new(),
        recorded(&calls),
        "縫い目も呼ばれない"
    );
}

/// 挿入位置は行数までを許し（末尾への追加）、行数を超える位置は
/// [`GridError::SpanOutOfRange`] を返して**何も変えない**。
#[test]
fn an_insert_position_may_be_the_end_and_beyond_it_is_rejected() {
    // (1) `at == 行数` は末尾への追加である。
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let ids = fixture.ids();
    let defaults = fixture.plan.default_row();
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::InsertRows {
                at: RowOrdinal::new(ids.len()),
                count: 1,
            },
        )
        .expect("行数の位置への挿入は末尾への追加として成功する");

    assert_eq!(ids.len() + 1, outcome.row_count);
    let after = fixture.ids();
    assert_eq!(
        ids,
        after[..ids.len()].to_vec(),
        "末尾への追加は既存の行を動かさない"
    );
    assert_eq!(after[ids.len()], outcome.affected[0], "追加は末尾");
    assert_eq!(defaults, fixture.values_of(outcome.affected[0]));
    assert_row_operation_call_shape(&calls, &fixture.plan);

    // (2) 行数を超える位置は拒まれ、何も変わらない。
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let count = fixture.row_count();
    let at = count + 1;
    let (mut apply, calls) = counting(sheet, fixture.plan.clone());
    let before = fixture.snapshot();

    let error = apply
        .apply(
            fixture.document_mut(),
            EditCommand::InsertRows {
                at: RowOrdinal::new(at),
                count: 1,
            },
        )
        .expect_err("行数を超える位置への挿入は失敗する");

    assert_eq!(
        GridError::SpanOutOfRange {
            span: RowSpan::new(RowOrdinal::new(at), 1),
            visible: count,
        },
        error
    );
    assert_eq!(before, fixture.snapshot(), "1 行も挿入されない");
    assert_eq!(
        Vec::<QueryCall>::new(),
        recorded(&calls),
        "縫い目も呼ばれない"
    );
}

/// 列が 1 件も宣言されていないシートでは、行の操作もできない（[`GridError::SchemaUnusable`]）。
#[test]
fn row_operations_need_a_sheet_with_columns() {
    for command in [
        EditCommand::InsertRows {
            at: RowOrdinal::new(0),
            count: 1,
        },
        // 空の命令でもセッションの前提は検査する（3.1 と同じ規則）。
        EditCommand::RemoveRows { rows: Vec::new() },
        EditCommand::DuplicateRows { rows: Vec::new() },
    ] {
        let mut document = Document::new();
        let sheet = document.add_sheet("空のシート");
        document
            .set_sheet_columns(sheet, Vec::new())
            .expect("列を設定できない");
        let plan = column_less_plan(&mut document, sheet);
        assert_eq!(0, plan.column_count(), "空の宣言は列 0 本の計画になる");

        let (mut apply, calls) = counting(sheet, plan);
        let before = snapshot(&document, sheet);
        let error = apply
            .apply(&mut document, command.clone())
            .expect_err("列 0 本のシートでは行の操作もできない");

        assert_eq!(GridError::SchemaUnusable { sheet }, error);
        assert_eq!(before, snapshot(&document, sheet), "何も変わらない");
        assert_eq!(
            Vec::<QueryCall>::new(),
            recorded(&calls),
            "縫い目も呼ばれない"
        );
    }
}

/// 列 0 本のシートの計画を組む（[`row_operations_need_a_sheet_with_columns`] の前提）。
fn column_less_plan(document: &mut Document, sheet: SheetId) -> CompiledSchema {
    use document_format::SchemaPart;
    use schema_engine::{SchemaEngine, SchemaEngineApi, TypeRegistry};
    document
        .set_root_schema(sheet, SchemaPart::empty())
        .expect("宣言を設置できない");
    SchemaEngine::new()
        .compile(
            document.sheet_by_id(sheet).expect("シートは文書にある"),
            &TypeRegistry::new(),
        )
        .expect("空の宣言はコンパイルできる")
}

// ---------------------------------------------------------------------------
// 5. 空の命令
// ---------------------------------------------------------------------------

/// 行 0 件の削除・複製と件数 0 の追加は、成功し、何も変えず、**縫い目を 1 回も呼ばない**
/// （3.1 の「空の命令は何も変えない」と同じ規則）。
#[test]
fn empty_row_commands_change_nothing_and_call_no_judgement() {
    let commands = [
        EditCommand::InsertRows {
            at: RowOrdinal::new(0),
            count: 0,
        },
        // 件数 0 の追加は位置を見ない（何も挿入しないため、位置の妥当性も問わない）。
        EditCommand::InsertRows {
            at: RowOrdinal::new(usize::MAX),
            count: 0,
        },
        EditCommand::RemoveRows { rows: Vec::new() },
        EditCommand::DuplicateRows { rows: Vec::new() },
    ];

    for command in commands {
        let mut fixture = Fixture::clean(64, 13);
        let sheet = fixture.sheet();
        let (mut apply, calls) = counting(sheet, fixture.plan.clone());
        let before = fixture.snapshot();
        let row_count = fixture.row_count();

        let outcome = apply
            .apply(fixture.document_mut(), command.clone())
            .expect("空の命令は成功する");

        assert_eq!(
            Vec::<RowId>::new(),
            outcome.affected,
            "{command:?}: 影響を受けた行は無い"
        );
        assert_eq!(row_count, outcome.row_count, "{command:?}: 行数は変わらない");
        assert_eq!(0, outcome.violation_total, "{command:?}: 違反の総数も 0");
        assert!(outcome.coercions.is_empty(), "{command:?}: 変換も無い");
        assert_eq!(before, fixture.snapshot(), "{command:?}: 何も変わらない");
        assert_eq!(
            Vec::<QueryCall>::new(),
            recorded(&calls),
            "{command:?}: 縫い目を 1 回も呼ばない"
        );
    }
}

// ---------------------------------------------------------------------------
// 6. 決定性
// ---------------------------------------------------------------------------

/// 同じ形・同じ値の文書と同じ命令は、同じ構造の結果を与える。
///
/// 生の識別子は標本の組み立てごとに変わるため比較しない（`tests/common/sample.rs` のモジュール
/// docs「決定性 — 何が同一で、何が同一でないか」）。比較するのは**構造**（行数・影響を受けた行の
/// 位置・違反の総数）と、参照を運ばない列の**値**である。
#[test]
fn the_same_row_command_on_the_same_shape_gives_the_same_outcome() {
    /// 比較する命令の種別。
    #[derive(Clone, Copy)]
    enum Kind {
        Insert,
        Remove,
        Duplicate,
    }

    for kind in [Kind::Insert, Kind::Remove, Kind::Duplicate] {
        let mut observed = Vec::new();

        for _ in 0..2 {
            let mut fixture = Fixture::clean(64, 13);
            let sheet = fixture.sheet();
            // 命令は毎回同じ形の標本から同じ位置で組み立てる（生の識別子は跨いで比較しない）。
            let command = match kind {
                Kind::Insert => EditCommand::InsertRows {
                    at: RowOrdinal::new(4),
                    count: 2,
                },
                Kind::Remove => EditCommand::RemoveRows {
                    rows: vec![fixture.row(9), fixture.row(1), fixture.row(1)],
                },
                Kind::Duplicate => EditCommand::DuplicateRows {
                    rows: vec![fixture.row(6), fixture.row(0)],
                },
            };
            let mut apply = EditApply::new(sheet, fixture.plan.clone());

            let outcome = apply
                .apply(fixture.document_mut(), command)
                .expect("行の操作は成功する");

            // 影響を受けた行の**適用後の文書の位置**（生の識別子の代わりに構造で比べる。削除では
            // 取り除かれた行なので位置は `None` になる）。
            let after = fixture.ids();
            let positions: Vec<Option<usize>> = outcome
                .affected
                .iter()
                .map(|row| after.iter().position(|id| id == row))
                .collect();
            observed.push((
                outcome.affected.len(),
                outcome.row_count,
                outcome.violation_total,
                outcome.coercions.len(),
                after.len(),
                positions,
                fixture.column_values(0),
                fixture.column_values(1),
            ));
        }

        assert_eq!(
            observed[0], observed[1],
            "同じ形の文書と同じ命令は同じ構造の結果を与える"
        );
    }
}

// ---------------------------------------------------------------------------
// 7. 再検証の形（要件 11.4 との関係）
// ---------------------------------------------------------------------------

/// 行の構造を変える命令は、**すべての列を指定した再検証を 1 回だけ**呼び、書き込みの判定と
/// シート全件の検証は呼ばない。報告の総数は適用後のシートの違反の総数に一致する。
///
/// 既定値（宣言が供給する値）も複製の値（ドキュメントにある値）も**打ちれた文字ではない**ため、
/// 判定を通す理由が無い（通せば変換で値が変わりうる。要件 6.3 は「同じ値を持つ行」を求める）。
/// 行の集合が変わるときはすべての列の違反が変わりうる（挿入した行のあらゆる列が値なし／既定値、
/// 削除した行の参照を指す他の行）ため、再検証は列を絞らずに**全列**で行う — 絞れば
/// 静かに過少報告になり、とくに一意制約は**行を跨ぐ**性質であり、複製で生まれ、削除で解消する。
#[test]
fn row_operations_revalidate_every_column_once_and_never_judge_a_write() {
    /// 比較する命令の種別。
    #[derive(Clone, Copy)]
    enum Kind {
        Insert,
        Remove,
        Duplicate,
    }

    for kind in [Kind::Insert, Kind::Remove, Kind::Duplicate] {
        let mut fixture = Fixture::clean(64, 13);
        let sheet = fixture.sheet();
        let before = fixture.ids();
        let command = match kind {
            Kind::Insert => EditCommand::InsertRows {
                at: RowOrdinal::new(3),
                count: 2,
            },
            Kind::Remove => EditCommand::RemoveRows {
                rows: vec![fixture.row(5), fixture.row(6)],
            },
            Kind::Duplicate => EditCommand::DuplicateRows {
                rows: vec![fixture.row(1)],
            },
        };
        let (mut apply, calls) = counting(sheet, fixture.plan.clone());

        let outcome = apply
            .apply(fixture.document_mut(), command)
            .expect("行の操作は成功する");

        // 呼び出しの形: 全列の再検証 1 回だけ（判定も全件検証も現れない）。
        assert_row_operation_call_shape(&calls, &fixture.plan);
        // 報告の総数は適用後のシートの違反の総数である（全件検証の結果と一致する）。
        assert_eq!(
            fixture.violations_now(),
            outcome.violation_total,
            "違反の総数は適用後のシートの違反の総数"
        );
        // `affected` と `row_count` の意味（増減した行）。
        let after = fixture.ids();
        match kind {
            Kind::Insert => {
                assert_eq!(before.len() + 2, outcome.row_count, "行が 2 つ増える");
                assert_eq!(2, outcome.affected.len());
                for row in &outcome.affected {
                    assert!(after.contains(row), "挿入された行は適用後に実在する");
                    assert!(!before.contains(row), "挿入された行は新しい");
                }
            }
            Kind::Remove => {
                assert_eq!(before.len() - 2, outcome.row_count, "行が 2 つ減る");
                assert_eq!(2, outcome.affected.len());
                for row in &outcome.affected {
                    assert!(!after.contains(row), "取り除かれた行はもう実在しない");
                }
            }
            Kind::Duplicate => {
                assert_eq!(before.len() + 1, outcome.row_count, "行が 1 つ増える");
                assert_eq!(1, outcome.affected.len());
                for row in &outcome.affected {
                    assert!(after.contains(row), "複製は適用後に実在する");
                    assert!(!before.contains(row), "複製は新しい");
                }
            }
        }
    }
}

/// 表示に使う文字列の写しが 1 つであること（5.1 が写しを作らずに使う唯一の源）。
///
/// 行の操作は値を作り直さない（既定値は計画から、複製の値はドキュメントから）ため、表示の
/// 文字列は元の値と同じ `display_text` から導かれる。この検査は、複製の前後で表示が変わらない
/// ことを**表示文字列そのもの**で確かめる（数だけを数える検査の対照）。
#[test]
fn the_display_text_of_a_duplicated_row_is_the_same_as_the_source() {
    let mut fixture = Fixture::clean(64, 13);
    let sheet = fixture.sheet();
    let source = fixture.row(4);
    let before: Vec<String> = fixture
        .values_of(source)
        .iter()
        .map(|value| display_text(value).into_owned())
        .collect();
    let (mut apply, _calls) = counting(sheet, fixture.plan.clone());

    let outcome = apply
        .apply(
            fixture.document_mut(),
            EditCommand::DuplicateRows { rows: vec![source] },
        )
        .expect("行の複製は成功する");

    let copy = outcome.affected[0];
    let after: Vec<String> = fixture
        .values_of(copy)
        .iter()
        .map(|value| display_text(value).into_owned())
        .collect();
    assert_eq!(before, after, "複製の表示は元の行と 1 つも違わない");
    assert!(
        before.iter().any(|text| !text.is_empty()),
        "前提: 中身のある列がある"
    );
}

//! 画面 1 枚ぶんの操作口（データグリッドのタスク 5.2。data-grid 要件 4.3, 4.6, 11.4）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のもので
//! ある。
//!
//! 1. **開くのに文書が要らない**（design.md「GridSession」の Responsibility）。`open` は
//!    シートと計画だけを受け取り、文書を受け取る経路を持たない。以後の文書を触る呼び出しは
//!    すべて参照または可変参照を受け取り、**所有しない**（文書はセッションを落とした後も
//!    そのまま使える）。
//! 2. **表示の指定を変える**（`set_view`）。可視行数・隠れた行数が文書の行数と整合し
//!    （要件 8.7）、**索引の鍵が張り直され**（違反の位置が新しい順序に従い。要件 4.3, 4.4）、
//!    2.2 の据え付けが入れ替わり、**入れ子の展開の状態が失われず**（要件 5.3）、
//!    **文書は 1 つも変わらない**（要件 8.5。前後で文書全体の写しを比べる）。
//! 3. **差分で索引と総数を更新する**（タスク 5.2 の核心。要件 4.6, 11.4）。違反が解消された
//!    とき総数が減り、新たな違反が生じたとき増え、1 回の貼り付けで双方が起きたときも
//!    正しい。期待値は**標本の申告から導く**だけでなく、**テスト自身が全件検証を 1 回呼んで
//!    得た真の総数**とも突き合わせる（本番の経路は呼ばない）。
//! 4. **全件検証を呼ばない**（要件 11.4）。証拠は速度ではなく**呼び出しの形**であり、
//!    3.1 の縫い目（[`EditSchemaQuery`]）を通った呼び出しを数えて固定する —
//!    編集・取り消し・やり直しの経路で `validate_sheet` が **0 回**である。
//! 5. **取り消しとやり直し**が索引と総数にも及ぶ（要件 9.2, 9.3）。
//! 6. **窓の符号化**（5.1 への委譲）が、セッションの可視の順序をそのまま運び、復号した行の
//!    識別子が一致する（要件 1.1, 1.2）。
//! 7. **表示範囲の外の違反**にも探索が届き（要件 4.4）、絞り込み中の貼り付けが
//!    **表示されている行にのみ**及ぶ（要件 8.9）。
//! 8. **並べ替えの基準列の値の編集では行が動かず**（要件 8.8）、行の集合が変わったときだけ
//!    順序が導出し直される（消えた行を窓が引かないため。要件 1.7, 6.2）。
//! 9. **決定性**。同じ入力からは同じ観測結果が出る。
//! 10. **窓の違反の札は、触っていない列も含めて真値と一致する**（要件 4.1, 4.4）。1 セルの
//!    編集・その取り消し・やり直しの**各段階**で、可視の序数 × 宣言の列の**すべて**の札を、
//!    テスト自身の全件検証から導いた真値と突き合わせる。総数の検査では観測できない壊れ方で
//!    ある — 差分が編集した列以外の保持を落としても、総数は据えた値のまま正しく見える。
//! 11. **1 つの入れ子のセルが複数の違反位置を持つときも、差分は総数を正しく閉じる**
//!    （要件 4.6）。差分の被減数は**セルの数ではなく位置の数**でなければならない（標本の
//!    仕込む違反は 1 セル 1 件であるため、この入力は検査が自分で作る）。ずれは**同じ列への
//!    2 回目の編集**で現れる。
//! 12. **行の集合が変わる操作の取り消しでも順序が導出し直される**（要件 1.7, 9.2）。取り消しは
//!    行の種類を受け取らないため、手掛かりは**行数の変化**だけである。見落とすと、消えた行を
//!    窓が引く（足した行の取り消し）、戻った行が現れない（消した行の取り消し）経路が残る。
//! 13. **「違反あり」の絞り込みが読む据え付けも、編集の差分で据え直される**（要件 4.3, 4.6）。
//!    据え直さないと、違反を 1 件も持たなくなった行に印が残り、表示の指定を当て直しても
//!    古い答えが返る。
//! 14. **世代は画面に見えているものが変わりうる操作の後だけ進む**（要件 7.3）。古い世代の窓を
//!    捨てさせるための札であり、進めること**だけ**が契約である（`transport` の `Generation`）。
//!
//! # 前提を先に表明する（tasks.md の Implementation Notes の規則）
//!
//! 標本（`tests/common/sample.rs`）を使う検査は、**標本がその検査の前提を満たしていることを
//! 最初に検査する**。とくに次の 4 つを [`assert_premises`] が表明する。
//!
//! - **列 0 は唯一の一意制約つきの列であり、同値が 1 件も無い**（同値・決着に依る検査は
//!   列 0 を使ってはならない）。
//! - **文書の行順は行識別子の順と一致する**（絞り込みも並べ替えも無い順序を、標本の行添字と
//!   同じものとして使える根拠である）。
//! - 標本の**生の識別子は比較に使わない**（実行ごとに変わる）。比較するのは行添字・列添字・
//!   入れ子の位置という構造である。
//! - 違反を仕込む標本は、**仕込んだセルがどの列にあるか**まで表明してから依拠する。
//!
//! 行を作り替える検査は無い（行の幅を縮める・空にする検査も無い）ため、幅についての前提は
//! 必要ない。行を足す検査は**文書全体の写し**（識別子・値・位置）で前後を比べる。

mod common;

use std::collections::{BTreeSet, HashSet};
use std::sync::{Arc, Mutex};

use common::sample::{sample, Sample, SampleEditParts, SampleOptions};
use data_grid::{
    decode_window, derive_layout, display_text, CellAddress, ColumnIndex, EditCommand, EditOutcome,
    EditSchemaQuery, ExpansionState, FilterSpec, Generation, GridError, GridSession, RowOrder,
    RowOrdinal, RowSpan, SchemaEngineQuery, SearchDirection, SortKey, UndoStack, ViewSpec,
    ViewSummary, DEFAULT_UNDO_LIMIT, WINDOW_FORMAT_VERSION,
};
use document_format::{
    to_json_bytes, CellValue, Document, NestedValue, Row, RowId, SchemaPart, SheetId,
};
use schema_engine::{
    schema_to_text, validate_sheet, ColumnDecl, CompiledSchema, Constraints, DeclaredKind,
    EditVerdict, Schema, SchemaEngine, SchemaEngineApi, SheetReport, TypeDecl, TypeKind,
    TypeRegistry, ValidationOptions,
};

// ---------------------------------------------------------------------------
// 縫い目へ届いた呼び出しの記録（3.1 の観測の形をそのまま使う）
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
    /// シート全件の検証。**編集・取り消し・やり直しの経路はこれを呼んではならない**
    /// （要件 11.4。索引の組み立て（`set_view`）だけがこれを呼ぶ）。
    ValidateSheet,
}

/// `schema-engine` の判定を**数える**縫い目。
///
/// 本番の縫い目（[`SchemaEngineQuery`]）へ**そのまま委譲**し、委譲の前に呼び出しを記録する。
/// したがって記録は「本番が実際に何を呼んだか」であり、判定の結果は本番と同一である。
/// セッションの 2 つの経路（索引の組み立てと編集の適用）が**同じ 1 つの実装**を呼ぶため、
/// 記録はどちらの経路の呼び出しも含む。
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

/// ここまでに縫い目へ届いた呼び出しの並び（到着順）。
fn recorded(calls: &Arc<Mutex<Vec<QueryCall>>>) -> Vec<QueryCall> {
    calls.lock().expect("記録の錠は毒されない").clone()
}

/// 記録のうち、指定した種類の呼び出しの**回数**。
fn count_of(calls: &Arc<Mutex<Vec<QueryCall>>>, wanted: fn(&QueryCall) -> bool) -> usize {
    recorded(calls).iter().filter(|call| wanted(call)).count()
}

/// 全件検証の回数（要件 11.4 の表明に使う）。
fn sheet_validations(calls: &Arc<Mutex<Vec<QueryCall>>>) -> usize {
    count_of(calls, |call| matches!(call, QueryCall::ValidateSheet))
}

/// 列を限定した再検証の回数。
fn column_revalidations(calls: &Arc<Mutex<Vec<QueryCall>>>) -> usize {
    count_of(calls, |call| {
        matches!(call, QueryCall::RevalidateColumns(_))
    })
}

// ---------------------------------------------------------------------------
// 標本とセッション
// ---------------------------------------------------------------------------

/// 標本を使う検査の前提（モジュール docs「前提を先に表明する」）。
fn assert_premises(sample: &Sample) {
    let plan = sample.compiled();
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
    assert_eq!(
        sample.row_ids().to_vec(),
        document_ids(sample.document(), sample.sheet()),
        "文書の行順は行識別子の順"
    );
    assert_eq!(
        0,
        sample.unique_violations(),
        "一意制約の重複を混ぜていない標本"
    );
    // 列 0（品番）は同値を持たない（同値・決着に依る検査は列 0 を使ってはならない）。
    let mut seen: HashSet<String> = HashSet::new();
    for row in sample
        .document()
        .sheet_by_id(sample.sheet())
        .expect("シートは文書にある")
        .rows()
    {
        let text = display_text(row.values().first().unwrap_or(&CellValue::Null)).into_owned();
        assert!(
            seen.insert(text),
            "列 0 の値が重複している（標本の前提が崩れた）"
        );
    }
    assert_eq!(
        sample.rows(),
        seen.len(),
        "列 0 の値は行数と同じ個数（同値が 1 件も無い）"
    );
    assert_eq!(
        sample.injected_violations(),
        sample.expected_violations(),
        "一意制約の重複を混ぜていない標本では、仕込んだ違反がそのまま総数"
    );
}

/// 標本が申告する前提のうち、[`Sample`] を分解した後も要るもの。
///
/// [`Sample::into_edit_parts`] は文書を消費するため、分解の後では違反の位置を標本へ
/// 問い合わせられない。検査が依拠する前提は**分解の前に**この型へ写す（行識別子・列名・
/// 仕込んだ違反の位置と数）。**標本の生の識別子は実行ごとに変わる**ため、比較に使うのは
/// 行添字・列添字という構造である（モジュール docs「前提を先に表明する」）。
#[derive(Debug, Clone, PartialEq)]
struct Premises {
    /// 標本の行識別子（文書の順）。
    row_ids: Vec<RowId>,
    /// 標本の列名（宣言の並び）。
    columns: Vec<String>,
    /// 標本が違反を仕込んだセル（行添字, 列添字）。
    scheduled: BTreeSet<(usize, usize)>,
    /// 仕込んだ違反の総数（= 一意制約の重複を混ぜていなければ期待される総数）。
    injected: usize,
}

impl Premises {
    /// 標本の申告から写す（分解の前に呼ぶ）。
    fn of(sample: &Sample) -> Self {
        Self {
            row_ids: sample.row_ids().to_vec(),
            columns: sample.columns().to_vec(),
            scheduled: scheduled(sample),
            injected: sample.injected_violations(),
        }
    }

    /// 行添字 `row` の行識別子。
    fn row_id(&self, row: usize) -> RowId {
        self.row_ids[row]
    }

    fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// 行添字 `row` の列添字 `column` のセルが違反として仕込まれているか。
    fn violation_at(&self, row: usize, column: usize) -> bool {
        self.scheduled.contains(&(row, column))
    }

    /// 列 `column` に仕込まれた違反の数。
    fn scheduled_in_column(&self, column: usize) -> usize {
        self.scheduled
            .iter()
            .filter(|(_, found)| *found == column)
            .count()
    }

    /// 行添字 `row` が違反している列の添字（昇順）。
    fn violating_columns(&self, row: usize) -> Vec<usize> {
        (0..self.column_count())
            .filter(|column| self.violation_at(row, *column))
            .collect()
    }

    /// 行添字 `row` が違反を 1 件も持たないか。
    fn row_is_clean(&self, row: usize) -> bool {
        self.violating_columns(row).is_empty()
    }

    /// 行識別子 `row` の行添字（標本の申告から引く）。
    fn row_index(&self, row: RowId) -> usize {
        self.row_ids
            .iter()
            .position(|found| *found == row)
            .expect("行は標本の行識別子にある")
    }
}

/// 開いたセッションと、その標本の部品。
struct Fixture {
    /// 標本の部品（文書・シート・行識別子・列名）。
    parts: SampleEditParts,
    /// 分解の前に写した前提（標本を消費した後も検査が依拠できる形）。
    premises: Premises,
    /// 開いた計画。
    plan: CompiledSchema,
    /// 縫い目を数える操作口。
    session: GridSession,
    /// 取り消し履歴（**所有者は呼び出し側である** — 要件 9.5。セッションは所有しない）。
    history: UndoStack,
    /// 縫い目の記録。
    calls: Arc<Mutex<Vec<QueryCall>>>,
}

impl Fixture {
    /// 標本を組み立て、**その標本の文書を渡さずに**操作口を開く。
    fn new(options: &SampleOptions) -> Self {
        let sample = sample(options);
        assert_premises(&sample);
        let plan = sample.compiled();
        let premises = Premises::of(&sample);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let session = GridSession::with_query(
            sample.sheet(),
            plan.clone(),
            Arc::new(CountingQuery {
                inner: SchemaEngineQuery,
                calls: Arc::clone(&calls),
            }),
        )
        .expect("セッションを開ける");
        Self {
            parts: sample.into_edit_parts(),
            premises,
            plan,
            session,
            // 履歴は**呼び出し側が作る**（セッションは所有しない。要件 9.5）。
            history: UndoStack::new(DEFAULT_UNDO_LIMIT),
            calls,
        }
    }

    /// 違反を仕込んだ標本（差分の検査は、仕込んだ違反が無いと数えられない）。
    fn violating() -> Self {
        Self::new(&SampleOptions::new(40, 13).with_ratio(0.2))
    }

    /// 絞り込みも並べ替えも無い表示を指定し、索引を組み立てる（以後の検査の土台）。
    fn indexed(&mut self) {
        self.session
            .set_view(&self.parts.document, no_view())
            .expect("表示を指定できる");
    }

    /// 標本が申告する前提（分解の前に写したもの）。
    ///
    /// **値として返す**（借用ではない）— 検査は前提を読んだ後に文書を可変で借りるため、
    /// 借用のまま持つと 2 つの借りが衝突する。前提は小さく（行識別子・列名・仕込んだ位置）、
    /// 写しを作る費用は検査の対象ではない。
    fn sample_premises(&self) -> Premises {
        self.premises.clone()
    }

    /// セッションがいま持っている入れ子の展開の指定（要件 5.3 の検査に使う）。
    fn session_expansion(&self) -> Vec<ExpansionState> {
        self.session.expansion().to_vec()
    }

    /// 表示の指定を適用する（`session` と `document` を同時に借りるため分解する）。
    fn set_view(&mut self, spec: ViewSpec) -> ViewSummary {
        let Self { session, parts, .. } = self;
        session
            .set_view(&parts.document, spec)
            .expect("表示を指定できる")
    }

    /// 編集命令を適用する（`session` と `document` を同時に借りるため分解する）。
    fn apply(&mut self, command: EditCommand) -> EditOutcome {
        let Self {
            session,
            parts,
            history,
            ..
        } = self;
        session
            .apply(&mut parts.document, history, command)
            .expect("編集は適用できる")
    }

    /// 取り消しを適用する（取り消せる操作があることを前提にする）。
    fn undo(&mut self) -> EditOutcome {
        let Self {
            session,
            parts,
            history,
            ..
        } = self;
        session
            .undo(&mut parts.document, history)
            .expect("取り消しは適用できる")
            .expect("取り消せる操作がある")
    }

    /// やり直しを適用する（やり直せる操作があることを前提にする）。
    fn redo(&mut self) -> EditOutcome {
        let Self {
            session,
            parts,
            history,
            ..
        } = self;
        session
            .redo(&mut parts.document, history)
            .expect("やり直しは適用できる")
            .expect("やり直せる操作がある")
    }

    /// 取り消しの生の結果（履歴の端で `None` を観測する検査のため）。
    fn try_undo(&mut self) -> Option<EditOutcome> {
        let Self {
            session,
            parts,
            history,
            ..
        } = self;
        session
            .undo(&mut parts.document, history)
            .expect("取り消せる")
    }

    /// やり直しの生の結果（履歴の端で `None` を観測する検査のため）。
    fn try_redo(&mut self) -> Option<EditOutcome> {
        let Self {
            session,
            parts,
            history,
            ..
        } = self;
        session
            .redo(&mut parts.document, history)
            .expect("やり直せる")
    }

    fn sheet(&self) -> SheetId {
        self.parts.sheet
    }

    fn document(&self) -> &Document {
        &self.parts.document
    }

    fn row(&self, index: usize) -> RowId {
        self.parts.row_ids[index]
    }

    fn rows(&self) -> usize {
        self.parts.row_ids.len()
    }

    /// 文書全体の写し（識別子・値・位置。要件 8.5 の前後比較に使う）。
    fn snapshot(&self) -> (Vec<RowId>, Vec<Vec<CellValue>>) {
        snapshot(self.document(), self.sheet())
    }

    /// **テスト自身が呼ぶ**全件検証の総数（本番の経路はこれを呼ばない。真の総数）。
    fn truth(&self) -> usize {
        full_total(self.document(), self.sheet(), &self.plan)
    }

    /// 違反の索引を組み立てるために縫い目へ届いた全件検証の回数（前提の表明に使う）。
    fn build_calls(&self) -> usize {
        sheet_validations(&self.calls)
    }

    /// セッションの窓を復号して、行の識別子の並びを取り出す。
    fn window_keys(&self, span: RowSpan) -> Vec<[u8; 16]> {
        let bytes = self
            .session
            .encode_window(self.document(), span)
            .expect("窓を符号化できる");
        let decoded = decode_window(&bytes).expect("窓を復号できる");
        assert_eq!(
            WINDOW_FORMAT_VERSION,
            decoded.version(),
            "窓の版が実装の版と一致する"
        );
        assert_eq!(
            self.session.generation(),
            decoded.generation(),
            "窓の世代はセッションのいまの世代"
        );
        decoded.rows().iter().map(|row| row.key()).collect()
    }

    /// セッションの窓の、行 `row`・列 `column` のセルが違反として印づけられているか
    /// （索引の更新を公開面から観測する経路）。
    fn window_marks_violation(&self, ordinal: usize, column: usize) -> bool {
        let bytes = self
            .session
            .encode_window(self.document(), RowSpan::new(RowOrdinal::new(ordinal), 1))
            .expect("窓を符号化できる");
        let decoded = decode_window(&bytes).expect("窓を復号できる");
        decoded
            .rows()
            .first()
            .expect("窓は 1 行を持つ")
            .cells()
            .get(column)
            .expect("窓は列を持つ")
            .violated()
    }
}

/// `RowId` の生 16 バイト（窓が運ぶ鍵の形）。
fn row_key(row: RowId) -> [u8; 16] {
    row.ulid().to_bytes()
}

/// 編集の結果が運ぶ違反のうち、行 `row`・列 `column` のものを含むか。
///
/// [`EditOutcome::violations`] が覆うのは**適用が再検証した列の全体**である（編集した
/// セルだけではない。`edit` のモジュール docs「違反は結果が運ぶ」の表）— したがって
/// 「そのセルが違反か」は、位置で引いて確かめる。
fn violation_present(outcome: &EditOutcome, row: RowId, column: usize) -> bool {
    outcome
        .violations
        .iter()
        .any(|found| found.row() == Some(row) && found.column() == ColumnIndex::new(column))
}

/// ドキュメントの行識別子を文書の順に取り出す。
fn document_ids(document: &Document, sheet: SheetId) -> Vec<RowId> {
    document
        .sheet_by_id(sheet)
        .expect("シートは文書にある")
        .rows()
        .iter()
        .map(Row::id)
        .collect()
}

/// 行 `row`・列 `column` の値（文書から読む。セルが無ければ `Null`）。
///
/// セッションを経由せずに文書を直接読む口である — **セッションが文書を所有していない**
/// ことを、セッションの外から観測するために使う。
fn value_of(document: &Document, sheet: SheetId, row: RowId, column: usize) -> CellValue {
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

/// 文書全体の写し（識別子・値・位置）。
fn snapshot(document: &Document, sheet: SheetId) -> (Vec<RowId>, Vec<Vec<CellValue>>) {
    let found = document.sheet_by_id(sheet).expect("シートは文書にある");
    (
        found.rows().iter().map(Row::id).collect(),
        found
            .rows()
            .iter()
            .map(|row| row.values().to_vec())
            .collect(),
    )
}

/// **テスト自身が呼ぶ**全件検証（本番の経路がこれを呼んでいないことの対照でもある）。
fn full_total(document: &Document, sheet: SheetId, plan: &CompiledSchema) -> usize {
    validate_sheet(document, sheet, plan, &ValidationOptions::unlimited()).total_violations()
}

/// テスト自身の全件検証から導いた「違反を持つセル」（行識別子 × 列の添字）。
///
/// 窓の違反の札（[`Fixture::window_marks_violation`]）の**真値**である。入れ子の内側の違反も
/// そのセルの札になるため、位置（`Violation::path`）では絞らない。**行を持たない違反**
/// （列そのものの問題）は載らない — 窓は行のセルに札を置くものであり、据え付けも行に属さない
/// 違反を載せない（`view` 層の `ViolationPresence`）。
fn violating_cells(
    document: &Document,
    sheet: SheetId,
    plan: &CompiledSchema,
) -> BTreeSet<(RowId, usize)> {
    validate_sheet(document, sheet, plan, &ValidationOptions::unlimited())
        .violations()
        .iter()
        .filter_map(|violation| violation.row().map(|row| (row, violation.column().index())))
        .collect()
}

/// 窓の違反の札（**可視の序数 × 宣言の列のすべて**）が、テスト自身の全件検証から導いた真値と
/// 一致することを表明する。
///
/// 編集したセルだけを見る検査では「**触っていない列の保持が落ちる**」壊れ方を観測できない —
/// 総数は索引へ据えた値のままなので、差分が他の列を消しても数の検査は緑のままである。
/// ここは可視の全序数と宣言の全列を突き合わせる。
fn assert_window_marks_match_truth(fixture: &Fixture, context: &str) {
    let declared = fixture.plan.column_count();
    assert_eq!(
        declared,
        fixture.session.columns().len(),
        "前提: 入れ子を展開していないので、列の構成は宣言の列数と同じ"
    );
    let truth = violating_cells(fixture.document(), fixture.sheet(), &fixture.plan);
    let mut order = RowOrder::default();
    order.recompute(fixture.document(), fixture.sheet(), &no_view());
    assert!(order.len() > 0, "前提: 可視の行がある");

    let mut checked = 0usize;
    let mut marked = 0usize;
    for ordinal in 0..order.len() {
        let row = order
            .row_at(RowOrdinal::new(ordinal))
            .expect("序数は可視行を指す");
        for column in 0..declared {
            let expected = truth.contains(&(row, column));
            marked += usize::from(expected);
            assert_eq!(
                expected,
                fixture.window_marks_violation(ordinal, column),
                "{context}: 可視の序数 {ordinal} の列 {column} の札が真値と一致しない"
            );
            checked += 1;
        }
    }
    assert!(
        marked > 0 && marked < checked,
        "前提: この標本は違反するセルと違反しないセルを混ぜる（{marked} / {checked}）"
    );
}

/// いま可視の行の識別子（窓が運ぶ順のまま）。
fn visible_keys(fixture: &Fixture) -> Vec<[u8; 16]> {
    let span = RowSpan::new(RowOrdinal::new(0), fixture.session.visible_row_count());
    fixture.window_keys(span)
}

/// 可視の並び（窓が運ぶ行）が文書と整合することを表明する — 窓は文書に無い行を引かず、行数を
/// 取り違えず、絞り込みも並べ替えも無ければ文書の並びをそのまま運ぶ。
///
/// 行の集合を変える操作とその取り消しの後に呼ぶ（要件 1.7）。順序の導出し直しを見落とすと、
/// 「文書に無い行を引く」か「文書にある行が現れない」のどちらかがここで現れる。
fn assert_order_matches_document(fixture: &Fixture, context: &str) {
    let ids = document_ids(fixture.document(), fixture.sheet());
    let visible = fixture.session.visible_row_count();
    // まず窓を引く — 順序が文書に無い行を指していれば、ここで**符号化が失敗する**（要件 1.7）。
    let keys = visible_keys(fixture);
    for key in &keys {
        assert!(
            ids.iter().any(|id| row_key(*id) == *key),
            "{context}: 窓は文書に無い行を引かない（要件 1.7）"
        );
    }
    assert_eq!(
        visible,
        keys.len(),
        "{context}: 可視の範囲の窓は可視行数ぶんの行を運ぶ"
    );
    assert_eq!(
        ids.len(),
        visible + fixture.session.hidden_row_count(),
        "{context}: 可視行数と隠れた行数の和は文書の行数（要件 8.7）"
    );
    assert_eq!(
        ids.iter().map(|id| row_key(*id)).collect::<Vec<_>>(),
        keys,
        "{context}: 絞り込みも並べ替えも無ければ、窓は文書の並びをそのまま運ぶ"
    );
}

/// 「違反あり」の絞り込み（列を問わない）で可視の集合が、全件検証から導いた真値と一致する
/// ことを表明する（並べ替えを指定していないので、順序は文書の順のままである）。
fn assert_visible_rows_match_truth(fixture: &Fixture, context: &str) {
    let violating: BTreeSet<RowId> =
        violating_cells(fixture.document(), fixture.sheet(), &fixture.plan)
            .into_iter()
            .map(|(row, _)| row)
            .collect();
    let expected: Vec<[u8; 16]> = document_ids(fixture.document(), fixture.sheet())
        .into_iter()
        .filter(|row| violating.contains(row))
        .map(row_key)
        .collect();
    assert_eq!(
        expected,
        visible_keys(fixture),
        "{context}: 「違反あり」の可視の集合が真値と一致しない"
    );
}

/// 標本の入れ子の配列（`明細`）の列の添字。
const LINE_ITEMS_COLUMN: usize = 11;

/// 入れ子の配列（`明細`）の要素が持つ数量の欄の名前。
const LINE_QUANTITY_FIELD: &str = "数量";

/// 行 `row` の `明細` の値を、**すべての要素の数量を範囲外にした**構造表現へ写す。
///
/// 1 つのセルが**2 つの違反位置**（`[0].数量` と `[1].数量`）を持つ入力である — 差分の
/// 被減数（`ViolationIndex::violations_in_columns`）が「セルの数」ではなく「位置の数」を
/// 数えているかを観測するために使う（セルの数を数える実装は 2 件を 1 件と数え、以後の同じ列の
/// 編集で総数がずれる）。標本の仕込む違反は 1 セル 1 件であるため、この入力は検査が自分で作る。
fn nested_with_two_bad_quantities(document: &Document, sheet: SheetId, row: RowId) -> String {
    let CellValue::Nested(NestedValue::Array(items)) =
        value_of(document, sheet, row, LINE_ITEMS_COLUMN)
    else {
        panic!("前提: 明細のセルは入れ子の配列である");
    };
    let count = items.len();
    assert!(
        count >= 2,
        "前提: 明細の配列は 2 つ以上の要素を持つ（{count} 要素）"
    );
    let mut rewritten = 0usize;
    let items = items
        .into_iter()
        .map(|item| {
            let CellValue::Nested(NestedValue::Object(fields)) = item else {
                panic!("前提: 明細の要素は入れ子のオブジェクトである");
            };
            let fields = fields
                .into_iter()
                .map(|(name, value)| {
                    if name == LINE_QUANTITY_FIELD {
                        // 下限 1 を外す（要素ごとに 1 件。セル全体では 2 件）。
                        rewritten += 1;
                        (name, CellValue::Int(-1))
                    } else {
                        (name, value)
                    }
                })
                .collect();
            CellValue::Nested(NestedValue::Object(fields))
        })
        .collect();
    assert_eq!(count, rewritten, "前提: 明細のどの要素にも数量の欄がある");
    let bytes = to_json_bytes(&CellValue::Nested(NestedValue::Array(items)), "明細")
        .expect("構造表現へ符号化できる");
    String::from_utf8(bytes).expect("構造表現は UTF-8")
}

/// 絞り込みも並べ替えも指定しない表示の指定。
fn no_view() -> ViewSpec {
    ViewSpec {
        sort: Vec::new(),
        filters: Vec::new(),
    }
}

/// 列 `column` の表示文字列に `text` を含む行だけを通す表示の指定（並べ替えは無し）。
fn filtered(column: usize, text: &str) -> ViewSpec {
    ViewSpec {
        sort: Vec::new(),
        filters: vec![FilterSpec::Contains {
            column: ColumnIndex::new(column),
            text: text.to_owned(),
        }],
    }
}

/// 列 `column` を降順に並べ替える表示の指定（絞り込みは無し）。
fn sorted_descending(column: usize) -> ViewSpec {
    ViewSpec {
        sort: vec![SortKey {
            column: ColumnIndex::new(column),
            descending: true,
        }],
        filters: Vec::new(),
    }
}

/// 標本が違反を仕込んだセル（行添字, 列添字）の集合（標本自身の申告から集める）。
fn scheduled(sample: &Sample) -> BTreeSet<(usize, usize)> {
    let mut out = BTreeSet::new();
    for row in 0..sample.rows() {
        for column in 0..sample.column_count() {
            if sample.violation_at(row, column) {
                out.insert((row, column));
            }
        }
    }
    out
}

/// 順序を先頭から走査して、最初に違反を持つ序数を求める（期待値を順序から導く）。
fn first_violating_ordinal(order: &RowOrder, sample: &Premises) -> Option<RowOrdinal> {
    (0..order.len()).map(RowOrdinal::new).find(|ordinal| {
        order
            .row_at(*ordinal)
            .map(|row| sample.row_index(row))
            .is_some_and(|row| !sample.row_is_clean(row))
    })
}

// ---------------------------------------------------------------------------
// 1. 開く・列・数の基本
// ---------------------------------------------------------------------------

/// `open` は**文書を渡さずに**開き、列の構成（入れ子の展開から導いた平坦な並び）を返す。
/// `set_view` が順序と索引を組み立て、以後の数が文書と整合する。
#[test]
fn opening_needs_no_document_and_exposes_the_layout() {
    let sample = sample(&SampleOptions::new(12, 13).with_ratio(0.2));
    assert_premises(&sample);
    let plan = sample.compiled();
    let sheet = sample.sheet();
    let columns = sample.column_count();
    let rows = sample.rows();

    // **文書はまだ存在しない**（`open` はシートと計画だけを受け取る）。
    let mut session = GridSession::open(sheet, plan.clone()).expect("セッションを開ける");

    assert_eq!(
        derive_layout(&plan, &[]).columns(),
        session.columns(),
        "列の構成は宣言と展開（空）から導いた構成そのもの"
    );
    assert_eq!(
        sample.columns(),
        session
            .columns()
            .iter()
            .map(|column| column.name.clone())
            .collect::<Vec<_>>(),
        "列名はスキーマの宣言の並び"
    );
    assert_eq!(Generation::FIRST, session.generation(), "開いた直後の世代");
    assert_eq!(0, session.visible_row_count(), "順序はまだ導出されていない");
    assert_eq!(0, session.hidden_row_count(), "隠れた行もまだ数えていない");
    assert_eq!(
        0,
        session.violation_total(),
        "索引はまだ組み立てられていない"
    );

    // ここで初めて文書が現れる。
    let parts = sample.into_edit_parts();
    let truth = full_total(&parts.document, sheet, &plan);
    assert!(
        truth > 0,
        "前提: この標本は違反を持つ（総数の変化を数えられる）"
    );

    let summary = session
        .set_view(&parts.document, no_view())
        .expect("表示を指定できる");
    assert_eq!(rows, summary.visible, "絞り込みが無いので全行が可視");
    assert_eq!(0, summary.hidden);
    assert_eq!(rows, session.visible_row_count());
    assert_eq!(0, session.hidden_row_count());
    assert_eq!(
        truth,
        session.violation_total(),
        "索引の総数はテストが全件検証で得た真の総数と一致する"
    );
    assert_eq!(
        columns,
        session
            .encode_window(&parts.document, RowSpan::new(RowOrdinal::new(0), 1))
            .map(|bytes| decode_window(&bytes).expect("復号できる").columns())
            .expect("窓を符号化できる"),
        "窓が運ぶ列の数は宣言の列数"
    );
}

/// 列を 1 本も宣言していない計画では開けない（`EditApply` と同じ判定。要件 1.6 の提示は
/// セッション無しに画面が行う）。
#[test]
fn a_sheet_without_columns_cannot_be_opened() {
    let mut document = Document::new();
    let sheet = document.add_sheet("空のシート");
    document
        .set_sheet_columns(sheet, Vec::new())
        .expect("列を設定できない");
    document
        .set_root_schema(sheet, SchemaPart::empty())
        .expect("宣言を設置できない");
    let plan = SchemaEngineApi::compile(
        &schema_engine::SchemaEngine::new(),
        document.sheet_by_id(sheet).expect("シートは文書にある"),
        &TypeRegistry::new(),
    )
    .expect("空の宣言はコンパイルできる");
    assert_eq!(0, plan.column_count(), "空の宣言は列 0 本の計画になる");

    assert_eq!(
        Err(GridError::SchemaUnusable { sheet }),
        GridSession::open(sheet, plan).map(|_| ()),
        "列 0 本の計画ではセッションを開けない"
    );
}

// ---------------------------------------------------------------------------
// 2. 表示の指定を変える（順序・索引の鍵・据え付け・展開・文書の不変）
// ---------------------------------------------------------------------------

/// `set_view` は順序を導出し直し、**索引の鍵を張り直し**、入れ子の展開を失わず、
/// **文書を 1 つも変えない**。
#[test]
fn set_view_rekeys_the_index_and_keeps_the_expansion() {
    let mut fixture = Fixture::violating();
    // 入れ子の列（10 = 届け先）を 1 段展開する（要件 5.1, 5.3）。
    fixture
        .session
        .set_expansion(ExpansionState::expanded_to(ColumnIndex::new(10), 1));
    let expanded = fixture.session.columns().to_vec();
    assert!(
        expanded.len() > 13,
        "前提: 展開すると列の構成が増える（13 列より多くなる）"
    );
    assert!(
        expanded.iter().any(|column| column.name.contains('.')),
        "前提: 展開した列の表示名は位置を `.` で連結した形になる"
    );

    fixture.indexed(); // 索引の組み立て（全件検証 1 回。前提として数える）
    let build = fixture.build_calls();
    assert_eq!(1, build, "索引の組み立ては全件検証をちょうど 1 回呼ぶ");

    let before_snapshot = fixture.snapshot();
    // 絞り込みと並べ替えの**前**の順序（違反の位置が張り直しで動くことの対照である）。
    let unfiltered_order = {
        let mut order = RowOrder::default();
        order.recompute(fixture.document(), fixture.sheet(), &no_view());
        order
    };

    // 絞り込み（品番が `000000` を含む行 = 行 0〜9。40 行の標本では値が `P0000000` …
    // `P0000039` である）＋ 並べ替え（品番の降順。同値が無いため順序が一意に決まる =
    // 決定性の検査に使える）。可視の行には違反を持つ行（行添字 4 と 9）と持たない行が混ざる。
    let spec = ViewSpec {
        sort: sorted_descending(0).sort,
        filters: filtered(0, "000000").filters,
    };
    let summary = fixture.set_view(spec.clone());

    let sample = fixture.sample_premises();
    let mut expected_order = RowOrder::default();
    expected_order.recompute(fixture.document(), fixture.sheet(), &spec);
    assert_eq!(
        expected_order.len(),
        summary.visible,
        "可視行数は同じ指定から導いた順序と一致する"
    );
    assert_eq!(expected_order.hidden(), summary.hidden);
    assert_eq!(
        fixture.rows(),
        summary.visible + summary.hidden,
        "可視行数と隠れた行数の和は文書の行数（要件 8.7）"
    );
    assert!(
        summary.visible > 0 && summary.hidden > 0,
        "前提: この絞り込みは行を実際に隠す（可視 {} / 隠れ {}）",
        summary.visible,
        summary.hidden
    );
    assert_ne!(no_view().sort, spec.sort, "前提: 並べ替えを指定している");
    assert_ne!(
        unfiltered_order.row_at(RowOrdinal::new(0)),
        expected_order.row_at(RowOrdinal::new(0)),
        "前提: 絞り込みと並べ替えが可視の先頭の行を実際に変える"
    );

    // 索引の鍵が張り直されている（違反の位置が新しい順序に従う）。
    let expected_first =
        first_violating_ordinal(&expected_order, &sample).expect("前提: 可視の行の中に違反がある");
    let found = fixture
        .session
        .find_violation(RowOrdinal::new(0), SearchDirection::Forward)
        .expect("可視の行に違反があるので見つかる");
    let expected_row = expected_order
        .row_at(expected_first)
        .expect("序数は可視行を指す");
    assert_eq!(
        expected_row,
        found.row(),
        "探索が返す行は、新しい順序で最初に違反を持つ行（鍵が張り直されている）"
    );
    assert_eq!(
        ColumnIndex::new(
            *sample
                .violating_columns(sample.row_index(expected_row))
                .first()
                .expect("違反を持つ行は違反する列を持つ")
        ),
        found.column(),
        "返る列はその行で最も小さい違反の列"
    );
    assert_ne!(
        unfiltered_order
            .ordinal_of(expected_row)
            .expect("前提: 違反を持つ行は絞り込み後の順序でも可視である"),
        expected_first,
        "前提: 古い順序での同じ行の位置は新しい順序での位置と違う（張り直しが観測できる）"
    );

    // 展開の状態が失われていない（要件 5.3）。
    assert_eq!(
        expanded,
        fixture.session.columns().to_vec(),
        "表示の指定を変えても入れ子の展開の状態は失われない"
    );
    assert_eq!(
        derive_layout(&fixture.plan, &fixture.session_expansion()).columns(),
        fixture.session.columns(),
        "列の構成は展開の状態から導いた構成そのもの"
    );

    // 文書は 1 つも変わらない（要件 8.5）。
    assert_eq!(
        before_snapshot,
        fixture.snapshot(),
        "表示の指定の変更は文書を変更しない"
    );
    assert!(
        sheet_validations(&fixture.calls) == build,
        "2 回目の set_view は全件検証を呼ばない（張り直しだけ）"
    );
}

// ---------------------------------------------------------------------------
// 3. 差分の更新（本タスクの核心）
// ---------------------------------------------------------------------------

/// 違反しているセルを適合する値へ編集すると、**総数が減り**、そのセルは違反を報告しなくなる。
/// 全件検証は呼ばれない（要件 4.6, 11.4）。
#[test]
fn a_conforming_edit_removes_the_violation_without_validating_the_sheet() {
    let mut fixture = Fixture::violating();
    fixture.indexed();
    let build = fixture.build_calls();
    let sample = fixture.sample_premises();

    let column = 1usize; // 数量（範囲つき整数。違反する値は -1）
    let (row_index, row) = (0..fixture.rows())
        .find(|row| sample.violation_at(*row, column))
        .map(|row| (row, fixture.row(row)))
        .expect("前提: 列 1 に違反が仕込まれている");
    let before = fixture.session.violation_total();
    let in_column = sample.scheduled_in_column(column);
    assert!(in_column > 0, "前提: 列 1 の違反が数えられる");

    let outcome = fixture.apply(EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(row, ColumnIndex::new(column)),
            "5".to_owned(),
        )],
    });

    assert_eq!(vec![row], outcome.affected, "影響を受けた行は編集した行");
    assert!(
        !violation_present(&outcome, row, column),
        "編集したセルは違反として報告されない"
    );
    assert_eq!(
        vec![ColumnIndex::new(column)],
        outcome.revalidated_columns,
        "再検証した列は編集した列だけ"
    );
    assert_eq!(
        before - 1,
        fixture.session.violation_total(),
        "解消した違反の数だけ総数が減る"
    );
    assert_eq!(
        in_column - 1,
        outcome.violation_total,
        "結果が運ぶ総数は編集した列に閉じた総数である（編集したセルが 1 件減る）"
    );
    assert!(
        outcome.violation_total < fixture.session.violation_total(),
        "列に閉じた総数はシートの総数より小さい（両者を取り違えていない）"
    );
    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "差分で求めた総数はテストが全件検証で得た真の総数と一致する"
    );
    assert!(
        !fixture.window_marks_violation(row_index, column),
        "解消したセルは違反として印づけられない"
    );

    // 呼び出しの形（要件 11.4）。**全件検証は 1 回も増えていない。**
    assert_eq!(
        build,
        sheet_validations(&fixture.calls),
        "編集の経路は全件検証を呼ばない"
    );
    assert_eq!(
        1,
        column_revalidations(&fixture.calls),
        "再検証は編集した列に限定して 1 回だけ"
    );
    assert_eq!(
        1,
        count_of(&fixture.calls, |call| matches!(
            call,
            QueryCall::JudgeWrite(_)
        )),
        "判定は編集した行に 1 回だけ"
    );
}

/// 適合するセルを違反する値へ編集すると、**総数が増え**、そのセルが違反を報告する。
#[test]
fn a_violating_edit_adds_a_violation_without_validating_the_sheet() {
    let mut fixture = Fixture::violating();
    fixture.indexed();
    let build = fixture.build_calls();
    let sample = fixture.sample_premises();

    let column = 1usize;
    let row_index = (0..fixture.rows())
        .find(|row| sample.row_is_clean(*row))
        .expect("前提: 違反を 1 件も持たない行がある");
    let row = fixture.row(row_index);
    let before = fixture.session.violation_total();

    let outcome = fixture.apply(EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(row, ColumnIndex::new(column)),
            "-1".to_owned(),
        )],
    });

    assert_eq!(vec![row], outcome.affected);
    assert!(
        violation_present(&outcome, row, column),
        "編集したセルが違反として報告される"
    );
    let at_cell = outcome
        .violations
        .iter()
        .find(|found| found.row() == Some(row) && found.column() == ColumnIndex::new(column))
        .expect("編集したセルの違反が報告にある");
    assert_eq!(0, at_cell.path().segments().len(), "セル直下の違反");
    assert_eq!(
        before + 1,
        fixture.session.violation_total(),
        "生じた違反の数だけ総数が増える"
    );
    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "差分で求めた総数は真の総数と一致する"
    );
    assert!(
        fixture.window_marks_violation(row_index, column),
        "新たに違反したセルは違反として印づけられる"
    );
    assert_eq!(
        build,
        sheet_validations(&fixture.calls),
        "編集の経路は全件検証を呼ばない"
    );
}

/// 1 回の貼り付けで違反が解消し、同時に新たな違反が生じるとき、総数は**双方を合わせた
/// 差**になる（差分が 1 つの操作の中でも正しい）。絞り込みが効いている間は、貼り付けが
/// 表示されている行にのみ及ぶ（要件 8.9）。
#[test]
fn a_paste_that_resolves_and_creates_violations_lands_on_the_true_total() {
    let mut fixture = Fixture::violating();
    fixture.indexed();
    let build = fixture.build_calls();
    let sample = fixture.sample_premises();

    let column = 1usize;
    // 表示の順序を絞り込む（貼り付けの宛先は**表示されている行の並び**である）。
    // 品番が `000000` を含む行 = 行 0〜9（違反を持つ行添字 4 と、持たない行 0 が混ざる）。
    let spec = filtered(0, "000000");
    fixture.set_view(spec.clone());
    let mut order = RowOrder::default();
    order.recompute(fixture.document(), fixture.sheet(), &spec);
    assert!(
        order.len() >= 2 && order.hidden() > 0,
        "前提: 絞り込みが行を隠し、可視の行が 2 つ以上ある（可視 {} / 隠れ {}）",
        order.len(),
        order.hidden()
    );

    // 可視の行のうち、**隣り合う** 2 行を選ぶ — 先の行は列 1 の違反を持ち（直す）、次の行は
    // 違反を 1 件も持たない（壊す）。隣接は選ぶ側で作る: 貼り付けの矩形は錨の行から
    // **可視の並び**を下へ進み、覆うのは隣り合う行だからである（`edit` の `PasteRange`。
    // 要件 8.9）。離れた 2 行を選ぶと矩形がその間の行にも届き、検査は選んだ行ではなく
    // 別の行を見ることになる（この検査がかつて踏んだ誤りである）。
    let visible: Vec<(usize, RowId)> = (0..order.len())
        .map(|ordinal| {
            let row = order
                .row_at(RowOrdinal::new(ordinal))
                .expect("序数は可視行を指す");
            (sample.row_index(row), row)
        })
        .collect();
    let (anchor_ordinal, fixed_index, fixed_row, broken_index, broken_row) = (0..visible.len() - 1)
        .find_map(|ordinal| {
            let (fixed_index, fixed_row) = visible[ordinal];
            let (broken_index, broken_row) = visible[ordinal + 1];
            (sample.violation_at(fixed_index, column) && sample.row_is_clean(broken_index))
                .then_some((ordinal, fixed_index, fixed_row, broken_index, broken_row))
        })
        .expect(
            "前提: 可視の並びに、列 1 の違反を持つ行と、その次の違反を 1 件も持たない行の組がある",
        );

    // 選んだ組が検査の前提を満たすことを表明する（標本の値や順序が変われば、検査が選ぶ行も
    // 変わる — 黙って空振りさせない）。
    assert_eq!(
        anchor_ordinal + 1,
        order
            .ordinal_of(broken_row)
            .expect("前提: 壊す行は可視である")
            .get(),
        "前提: 直す行と壊す行は可視の並びで隣り合う（矩形はこの 2 行に届く）"
    );
    assert!(
        sample.violation_at(fixed_index, column),
        "前提: 直す行は列 1 の違反を持つ（適合する値で 1 件解消する）"
    );
    assert!(
        sample.row_is_clean(broken_index),
        "前提: 壊す行は違反を 1 件も持たない（違反する値で 1 件生じる）"
    );
    assert_ne!(
        fixed_index, broken_index,
        "前提: 直す行と壊す行は別の行である"
    );

    let before = fixture.session.violation_total();
    let hidden_before: Vec<CellValue> = fixture
        .document()
        .sheet_by_id(fixture.sheet())
        .expect("シートは文書にある")
        .rows()
        .iter()
        .filter(|row| order.ordinal_of(row.id()).is_none())
        .map(|row| row.values().get(column).cloned().unwrap_or(CellValue::Null))
        .collect();
    assert!(!hidden_before.is_empty(), "前提: 隠れている行がある");

    // 錨は直す行（= 隣り合う 2 行の先）にあり、矩形は 2 行 1 列である — 1 行目で直し、
    // 2 行目で壊す。
    let anchor = CellAddress::new(fixed_row, ColumnIndex::new(column));
    let text = "5\n-1".to_owned();

    let outcome = fixture.apply(EditCommand::PasteRange {
        anchor,
        // 呼び出し側は表示の並びを持たない（セッションが自分の順序から埋める）。
        rows: Vec::new(),
        text,
    });

    assert_eq!(
        2,
        outcome.affected.len(),
        "貼り付けは可視の 2 行へ書く（隠れた行へは届かない）"
    );
    assert!(
        outcome.affected.contains(&fixed_row) && outcome.affected.contains(&broken_row),
        "影響を受けた行は可視の 2 行である"
    );
    assert!(
        !violation_present(&outcome, fixed_row, column),
        "直す行は列 1 の違反を報告しない（適合する値で 1 件解消した）"
    );
    assert!(
        violation_present(&outcome, broken_row, column),
        "壊す行は列 1 の違反を報告する（違反する値で 1 件生じた）"
    );
    assert_eq!(
        before,
        fixture.session.violation_total(),
        "1 件解消し 1 件生じたので総数は変わらない"
    );
    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "差分で求めた総数は真の総数と一致する"
    );
    assert!(
        !fixture.window_marks_violation(anchor_ordinal, column),
        "直した行のセルは違反として印づけられない"
    );
    assert!(
        fixture.window_marks_violation(anchor_ordinal + 1, column),
        "壊した行のセルは違反として印づけられる"
    );
    // 隠れた行の値は 1 つも変わっていない（要件 8.9）。
    let hidden_after: Vec<CellValue> = fixture
        .document()
        .sheet_by_id(fixture.sheet())
        .expect("シートは文書にある")
        .rows()
        .iter()
        .filter(|row| order.ordinal_of(row.id()).is_none())
        .map(|row| row.values().get(column).cloned().unwrap_or(CellValue::Null))
        .collect();
    assert_eq!(hidden_before, hidden_after, "隠れている行の値は変わらない");
    assert_eq!(
        build,
        sheet_validations(&fixture.calls),
        "貼り付けの経路も全件検証を呼ばない"
    );
}

// ---------------------------------------------------------------------------
// 4. 取り消しとやり直し
// ---------------------------------------------------------------------------

/// 取り消しは総数を前の値へ戻し、やり直しは再び動かす。**いずれも全件検証を呼ばない。**
#[test]
fn undo_and_redo_move_the_index_without_validating_the_sheet() {
    let mut fixture = Fixture::violating();
    fixture.indexed();
    let build = fixture.build_calls();
    let sample = fixture.sample_premises();

    let column = 1usize;
    let row_index = (0..fixture.rows())
        .find(|row| sample.violation_at(*row, column))
        .expect("前提: 列 1 に違反が仕込まれている");
    let row = fixture.row(row_index);
    let before = fixture.session.violation_total();
    let before_snapshot = fixture.snapshot();

    let applied = fixture.apply(EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(row, ColumnIndex::new(column)),
            "5".to_owned(),
        )],
    });
    assert_eq!(vec![row], applied.affected, "編集した行を運ぶ");
    assert!(
        !violation_present(&applied, row, column),
        "編集で解消したセルは違反として報告されない"
    );
    let after_edit = fixture.session.violation_total();
    assert_eq!(before - 1, after_edit);

    let undone = fixture.undo();
    assert_eq!(vec![row], undone.affected, "取り消した操作の行を運ぶ");
    assert_eq!(
        before,
        fixture.session.violation_total(),
        "取り消しで総数が前の値へ戻る"
    );
    assert_eq!(before_snapshot, fixture.snapshot(), "文書も元へ戻る");
    assert!(
        fixture.window_marks_violation(row_index, column),
        "取り消しで違反の提示が戻る（索引も戻っている）"
    );
    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "取り消しの後の総数も真の総数と一致する"
    );

    let redone = fixture.redo();
    assert_eq!(vec![row], redone.affected);
    assert_eq!(
        after_edit,
        fixture.session.violation_total(),
        "やり直しで総数が再び動く"
    );
    assert!(
        !fixture.window_marks_violation(row_index, column),
        "やり直しで違反の提示が取り下げられる（要件 4.6）"
    );

    // 履歴の端では何も起きない（失敗ではない）。
    let _ = fixture.undo();
    assert!(
        fixture.try_undo().is_none(),
        "履歴の先頭では取り消す操作が無い"
    );
    assert!(fixture.try_redo().is_some(), "取り消した操作はやり直せる");

    assert_eq!(
        build,
        sheet_validations(&fixture.calls),
        "取り消しとやり直しの経路は全件検証を呼ばない"
    );
    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "往復の後の総数も真の総数と一致する"
    );
}

// ---------------------------------------------------------------------------
// 5. 行の集合が変わるときと、値だけが変わるとき（要件 8.8, 1.7）
// ---------------------------------------------------------------------------

/// **並べ替えの基準列の値**を編集しても行は動かない（要件 8.8）。行の集合が変わったときは
/// 順序が導出し直され、消えた行を窓が引かない（要件 1.7, 6.2）。
#[test]
fn a_value_edit_does_not_move_rows_while_a_structural_edit_does() {
    let mut fixture = Fixture::violating();
    // 並べ替えは列 3（予備率）の降順。列 3 の値を編集しても順序は導出し直されない。
    let spec = sorted_descending(3);
    fixture.set_view(spec.clone());
    let visible_before = fixture.session.visible_row_count();
    let span = RowSpan::new(RowOrdinal::new(0), 8);
    let keys_before = fixture.window_keys(span);

    let row = fixture.row(2);
    fixture.apply(EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(row, ColumnIndex::new(3)),
            "99.5".to_owned(),
        )],
    });
    assert_eq!(
        keys_before,
        fixture.window_keys(span),
        "基準列の値の編集では行の並びが動かない（要件 8.8）"
    );
    assert_eq!(visible_before, fixture.session.visible_row_count());

    // 行を末尾へ足すと、行の集合が変わるので順序が導出し直される。
    let at = RowOrdinal::new(fixture.rows());
    let outcome = fixture.apply(EditCommand::InsertRows { at, count: 1 });
    assert_eq!(1, outcome.affected.len());
    assert_eq!(
        visible_before + 1,
        fixture.session.visible_row_count(),
        "足した行が可視の集合に入る（順序が導出し直されている）"
    );
    assert!(
        fixture
            .window_keys(RowSpan::new(
                RowOrdinal::new(fixture.session.visible_row_count() - 1),
                1
            ))
            .contains(&row_key(outcome.affected[0])),
        "足した行は可視の並びに現れる"
    );
    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "行を足した後の総数も真の総数と一致する"
    );

    // 行を消すと、消えた行を窓が引かない（順序が導出し直されている）。
    let removed = fixture.row(0);
    let outcome = fixture.apply(EditCommand::RemoveRows {
        rows: vec![removed],
    });
    assert_eq!(vec![removed], outcome.affected);
    assert_eq!(
        visible_before,
        fixture.session.visible_row_count(),
        "足して消したので可視行数は元へ戻る"
    );
    assert!(
        !fixture
            .window_keys(RowSpan::new(RowOrdinal::new(0), 8))
            .contains(&row_key(removed)),
        "消えた行は窓に現れない"
    );
    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "行を消した後の総数も真の総数と一致する"
    );
}

// ---------------------------------------------------------------------------
// 6. 探索と窓
// ---------------------------------------------------------------------------

/// 探索は**表示範囲の外**の違反にも届く（要件 4.4）。窓は**セッションの可視の順序**を運ぶ。
#[test]
fn search_reaches_outside_the_window_and_windows_carry_the_visible_order() {
    let mut fixture = Fixture::violating();
    let spec = no_view();
    fixture.set_view(spec.clone());
    let sample = fixture.sample_premises();

    let mut order = RowOrder::default();
    order.recompute(fixture.document(), fixture.sheet(), &spec);
    assert_eq!(fixture.rows(), order.len(), "前提: 絞り込みが無い");
    // 先頭 4 行は違反を 1 つも持たない（標本は行添字 4・9・… に違反を仕込む）。
    let span = RowSpan::new(RowOrdinal::new(0), 4);
    // 前提: 窓の中には違反が 1 件も無く、窓の外には違反がある。
    let window_rows: HashSet<RowId> = order.span(span).iter().copied().collect();
    let violating: HashSet<RowId> = sample
        .scheduled
        .iter()
        .map(|(row, _)| sample.row_id(*row))
        .collect();
    assert!(
        window_rows.is_disjoint(&violating),
        "前提: 窓の中に違反が 1 件も無い"
    );
    let outside = (0..order.len())
        .map(RowOrdinal::new)
        .filter(|ordinal| {
            order
                .row_at(*ordinal)
                .is_some_and(|row| violating.contains(&row) && !window_rows.contains(&row))
        })
        .count();
    assert!(outside > 0, "前提: 窓の外に違反がある（{outside} 行）");

    let found = fixture
        .session
        .find_violation(RowOrdinal::new(0), SearchDirection::Forward)
        .expect("窓の外の違反が見つかる");
    assert!(
        violating.contains(&found.row()),
        "返る行は違反を持つ行である"
    );
    assert!(
        !window_rows.contains(&found.row()),
        "返る行は表示範囲の外にある"
    );
    assert_eq!(
        order.ordinal_of(found.row()),
        Some(RowOrdinal::new(
            (0..order.len())
                .map(RowOrdinal::new)
                .find(|ordinal| order.row_at(*ordinal) == Some(found.row()))
                .expect("返る行は可視である")
                .get()
        )),
        "返る行は可視の並びの中にある"
    );

    // 後方の探索は最後の違反を返し、それ以上後ろには違反が無い。
    let last = fixture
        .session
        .find_violation(RowOrdinal::new(order.len() + 10), SearchDirection::Backward)
        .expect("後方にも違反がある");
    assert!(
        violating.contains(&last.row()),
        "返る行は違反を持つ行である"
    );
    assert!(
        order.ordinal_of(last.row()).expect("可視である").get() > span.end(),
        "後方の探索は表示範囲より後ろの違反へ届く"
    );

    // 窓はセッションの可視の順序をそのまま運ぶ。
    let keys = fixture.window_keys(span);
    let expected: Vec<[u8; 16]> = order.span(span).iter().map(|row| row_key(*row)).collect();
    assert_eq!(expected, keys, "窓の行は可視の順序そのものである");
    assert_eq!(4, keys.len(), "端に掛からない窓は要求の行数をそのまま運ぶ");
}

// ---------------------------------------------------------------------------
// 7. 文書の非所有と決定性
// ---------------------------------------------------------------------------

/// セッションは `Document` を所有しない。文書はセッションを落とした後もそのまま使え、
/// 操作口は境界を越えて持ち回れる（`src-tauri` の `manage` が要求する形）。
#[test]
fn the_session_never_owns_the_document() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GridSession>();

    let sample = sample(&SampleOptions::new(8, 13).with_ratio(0.1));
    assert_premises(&sample);
    let plan = sample.compiled();
    let sheet = sample.sheet();
    let mut parts = sample.into_edit_parts();
    // 編集するのは列 1（数量。標本の列 0 = 品番は唯一の一意制約つきのキーである）。
    let edited = parts.row_ids[0];
    let column = 1usize;

    // 編集の前の値を先に読む（取り消しが戻す先である）。**標本の値は実行ごとに変わる**ため、
    // 期待値を定数で書いてはならない（`tests/common/sample.rs` の決定性の規則）。
    let original = value_of(&parts.document, sheet, edited, column);
    assert_ne!(
        CellValue::Int(7),
        original,
        "前提: 編集はセルの値を実際に変える（前後を区別して観測できる）"
    );

    // 取り消し履歴も所有しない（要件 9.5。design.md「UndoStack（拡張点の所有者）」）—
    // **呼び出し側が持ち回る**。セッションを開き直しても同じ履歴の続きを進められることが、
    // 「履歴がセッションの持ち物ではない」ことの観測である。
    let mut history = UndoStack::new(DEFAULT_UNDO_LIMIT);

    {
        // 文書を**所有せずに**開き、すべての文書を触る呼び出しを参照で行う。
        let mut session = GridSession::open(sheet, plan.clone()).expect("セッションを開ける");
        session
            .set_view(&parts.document, no_view())
            .expect("表示を指定できる");
        let _ = session.encode_window(&parts.document, RowSpan::new(RowOrdinal::new(0), 2));
        let _ = session.find_violation(RowOrdinal::new(0), SearchDirection::Forward);
        let _ = session.columns();
        let _ = session.visible_row_count();
        let _ = session.generation();
        let applied = session
            .apply(
                &mut parts.document,
                &mut history,
                EditCommand::SetCells {
                    cells: vec![(
                        CellAddress::new(edited, ColumnIndex::new(column)),
                        "7".to_owned(),
                    )],
                },
            )
            .expect("編集は成功する");
        assert_eq!(vec![edited], applied.affected, "影響を受けた行は編集した行");
        // 編集は**呼び出し側の文書**へ書かれる（セッションが写しを持つなら、ここは前の値の
        // ままになる）。
        assert_eq!(
            CellValue::Int(7),
            value_of(&parts.document, sheet, edited, column),
            "編集の結果は文書に残っている"
        );

        let undone = session
            .undo(&mut parts.document, &mut history)
            .expect("取り消せる")
            .expect("取り消せる操作がある");
        assert_eq!(vec![edited], undone.affected, "取り消した操作の行を運ぶ");
        assert_eq!(
            original,
            value_of(&parts.document, sheet, edited, column),
            "取り消しは文書を編集の前の値へ戻す（動いているのは文書そのものである）"
        );
    }

    // セッションを落とした後も文書はそのまま使える（所有していない）。
    assert_eq!(
        parts.row_ids,
        document_ids(&parts.document, sheet),
        "文書はセッションの外で生きている"
    );
    assert_eq!(
        original,
        value_of(&parts.document, sheet, edited, column),
        "文書はセッションを落とした後もそのまま使える"
    );

    {
        // **新しいセッション**（同じシート・同じ計画）へ同じ履歴を渡すと、前のセッションが
        // 積んだ対の続きを進められる（取り消した編集のやり直し）。
        let mut next = GridSession::open(sheet, plan.clone()).expect("セッションを開ける");
        // やり直しの前に、**履歴が前のセッションの持ち物でないこと**を示す材料として、
        // 取り消し済みの対が残っていることを深さで見る（取り消しは件数を減らさない）。
        assert_eq!(1, history.depth(), "履歴の対はセッションの外に残っている");
        let redone = next
            .redo(&mut parts.document, &mut history)
            .expect("やり直せる")
            .expect("やり直せる操作がある");
        assert_eq!(
            vec![edited],
            redone.affected,
            "新しいセッションが前のセッションの履歴を進める"
        );
        assert_eq!(
            CellValue::Int(7),
            value_of(&parts.document, sheet, edited, column),
            "やり直しは文書へ届く"
        );
    }
}

/// 同じ入力からは同じ観測結果が出る（決定性）。
#[test]
fn the_session_is_deterministic() {
    let fixture = Fixture::violating();
    let spec = ViewSpec {
        sort: sorted_descending(3).sort,
        filters: filtered(7, "完了").filters,
    };

    // 同じ文書と同じ指定で開いた 2 つのセッションは、同じ数と位置を答える。
    let mut first =
        GridSession::open(fixture.sheet(), fixture.plan.clone()).expect("セッションを開ける");
    let mut second =
        GridSession::open(fixture.sheet(), fixture.plan.clone()).expect("セッションを開ける");
    let mut expected = RowOrder::default();
    expected.recompute(fixture.document(), fixture.sheet(), &spec);
    let violation_total = full_total(fixture.document(), fixture.sheet(), &fixture.plan);

    for session in [&mut first, &mut second] {
        let summary = session
            .set_view(fixture.document(), spec.clone())
            .expect("表示を指定できる");
        assert_eq!(expected.len(), summary.visible);
        assert_eq!(expected.hidden(), summary.hidden);
        assert_eq!(violation_total, session.violation_total());
        assert_eq!(
            expected
                .row_at(RowOrdinal::new(0))
                .map(|row| CellAddress::new(row, ColumnIndex::new(0)).row()),
            session
                .find_violation(RowOrdinal::new(0), SearchDirection::Forward)
                .map(|cell| CellAddress::new(cell.row(), ColumnIndex::new(0)).row())
                .or_else(|| expected.row_at(RowOrdinal::new(0))),
            "同じ入力からは同じ位置が返る"
        );
    }

    // 同じ命令を当てた 2 つのセッションは、同じ形の結果を返す（生の識別子は比較しない）。
    let span = RowSpan::new(RowOrdinal::new(0), 4);
    let keys_first = {
        let bytes = first
            .encode_window(fixture.document(), span)
            .expect("窓を符号化できる");
        decode_window(&bytes)
            .expect("復号できる")
            .rows()
            .iter()
            .map(|row| row.key())
            .collect::<Vec<_>>()
    };
    let keys_second = {
        let bytes = second
            .encode_window(fixture.document(), span)
            .expect("窓を符号化できる");
        decode_window(&bytes)
            .expect("復号できる")
            .rows()
            .iter()
            .map(|row| row.key())
            .collect::<Vec<_>>()
    };
    assert_eq!(keys_first, keys_second, "同じ区間の窓は同じ行を運ぶ");
    assert_eq!(
        violation_total,
        first.violation_total(),
        "読み出しは状態を変えない（総数）"
    );
    assert_eq!(
        violation_total,
        second.violation_total(),
        "読み出しは状態を変えない（総数）"
    );
}

// ---------------------------------------------------------------------------
// 8. 差分が届く範囲（レビューで見つかった空振りを塞ぐ）
// ---------------------------------------------------------------------------

/// **触っていない列の札も含めて**、窓の違反の札が真値と一致する。1 セルの編集・その取り消し・
/// やり直しの**各段階**で、可視の序数 × 宣言の列の**すべて**を突き合わせる（要件 4.1, 4.4）。
///
/// 差分が載せ替えるのは編集が再検証した列だけであり、他の列の保持を落としてはならない。
/// 落としても総数は索引へ据えた値のまま（＝正しい数）に見えるため、総数の検査では観測できない
/// 壊れ方である。
#[test]
fn an_edit_its_undo_and_its_redo_keep_every_columns_marks_at_truth() {
    let mut fixture = Fixture::violating();
    fixture.indexed();
    let sample = fixture.sample_premises();

    let column = 1usize; // 数量（範囲つき整数。違反する値は -1）
    let row_index = (0..fixture.rows())
        .find(|row| sample.violation_at(*row, column))
        .expect("前提: 列 1 に違反が仕込まれている");
    let row = fixture.row(row_index);

    // 前提: 編集する列の**外**にも違反がある（触っていない列の保持を観測できる）。
    let truth = violating_cells(fixture.document(), fixture.sheet(), &fixture.plan);
    assert!(
        truth.contains(&(row, column)),
        "前提: 編集するセルは違反している"
    );
    assert!(
        truth.iter().any(|(_, found)| *found != column),
        "前提: 編集する列の外にも違反がある"
    );

    assert_window_marks_match_truth(&fixture, "編集の前");

    fixture.apply(EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(row, ColumnIndex::new(column)),
            "5".to_owned(),
        )],
    });
    assert_window_marks_match_truth(&fixture, "編集の直後");

    fixture.undo();
    assert_window_marks_match_truth(&fixture, "取り消しの直後");

    fixture.redo();
    assert_window_marks_match_truth(&fixture, "やり直しの直後");
}

/// 1 つの入れ子のセルが**2 つの違反位置**を持つときも、差分は総数を正しく閉じる（要件 4.6）。
///
/// 差分の被減数は**セルの数ではなく位置の数**でなければならない — セルの数を数えると、同じ列への
/// 次の編集で (位置 − セル) だけ総数がずれる。標本の仕込む違反は 1 セル 1 件であるため、
/// この入力は検査が自分で作る（[`nested_with_two_bad_quantities`]）。
#[test]
fn a_nested_cell_with_two_violation_paths_does_not_drift_the_total() {
    let mut fixture = Fixture::violating();
    fixture.indexed();
    let column = LINE_ITEMS_COLUMN;

    let (first_row, second_row) = (fixture.row(0), fixture.row(1));
    let first = nested_with_two_bad_quantities(fixture.document(), fixture.sheet(), first_row);
    let outcome = fixture.apply(EditCommand::SetNested {
        cell: CellAddress::new(first_row, ColumnIndex::new(column)),
        json: first,
    });

    assert_eq!(
        vec![first_row],
        outcome.affected,
        "影響を受けた行は編集した行"
    );
    assert_eq!(
        vec![ColumnIndex::new(column)],
        outcome.revalidated_columns,
        "再検証した列は編集した列だけ"
    );
    assert_eq!(
        2,
        outcome
            .violations
            .iter()
            .filter(|found| found.row() == Some(first_row)
                && found.column() == ColumnIndex::new(column))
            .count(),
        "編集したセルは 2 つの違反位置を持つ（この入力が差分の被減数を試す前提である）"
    );
    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "1 回目の入れ子の編集の後の総数は真の総数と一致する"
    );

    // 2 回目の編集が効く段階である — 被減数が「いま索引がその列に載せている違反」を数える。
    let second = nested_with_two_bad_quantities(fixture.document(), fixture.sheet(), second_row);
    fixture.apply(EditCommand::SetNested {
        cell: CellAddress::new(second_row, ColumnIndex::new(column)),
        json: second,
    });

    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "同じ列への 2 回目の編集でも総数は真の総数と一致する（被減数は位置を数える）"
    );
}

/// 行の集合が変わる操作の**取り消し**でも、順序が導出し直される（要件 1.7, 9.2）。
///
/// 取り消しは戻す操作の種類を受け取らないため、手掛かりは**行数の変化**だけである。行数を見落とす
/// と、足した行を取り消したときに**消えた行を窓が引く**経路（符号化が文書に無い行で失敗する）、
/// 消した行を取り消したときに**戻った行が窓に現れない**経路が残る。
#[test]
fn undoing_a_row_structure_change_rebuilds_the_order() {
    let mut fixture = Fixture::violating();
    fixture.indexed();
    let sample = fixture.sample_premises();
    let column = 1usize;

    // 値の編集を先に 1 つ積む（取り消しの対象が構造の側になるようにする）。
    let row_index = (0..fixture.rows())
        .find(|row| sample.violation_at(*row, column))
        .expect("前提: 列 1 に違反が仕込まれている");
    let row = fixture.row(row_index);
    fixture.apply(EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(row, ColumnIndex::new(column)),
            "5".to_owned(),
        )],
    });

    // 1. 行を足し、その取り消しで**文書から消える行**を作る。
    let at = RowOrdinal::new(document_ids(fixture.document(), fixture.sheet()).len());
    let inserted = fixture.apply(EditCommand::InsertRows { at, count: 1 });
    assert_eq!(1, inserted.affected.len(), "足した行を運ぶ");
    let added = inserted.affected[0];
    assert!(
        visible_keys(&fixture).contains(&row_key(added)),
        "前提: 足した行は可視の並びに現れる"
    );

    let undone = fixture.undo();
    assert_eq!(vec![added], undone.affected, "取り消した操作の行を運ぶ");
    assert!(
        !document_ids(fixture.document(), fixture.sheet()).contains(&added),
        "取り消しで足した行が文書から消える"
    );
    assert_order_matches_document(&fixture, "行を足した取り消しの後");
    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "取り消しの後も総数は真の総数と一致する"
    );

    // 2. 行を消し、その取り消しで**文書へ戻る行**を作る。
    let removed = fixture.row(0);
    let removal = fixture.apply(EditCommand::RemoveRows {
        rows: vec![removed],
    });
    assert_eq!(vec![removed], removal.affected, "消した行を運ぶ");
    assert!(
        !document_ids(fixture.document(), fixture.sheet()).contains(&removed),
        "前提: 消した行は文書から消える"
    );

    let undone = fixture.undo();
    assert_eq!(vec![removed], undone.affected, "取り消した操作の行を運ぶ");
    assert!(
        document_ids(fixture.document(), fixture.sheet()).contains(&removed),
        "取り消しで消した行が文書へ戻る"
    );
    assert_order_matches_document(&fixture, "行を消した取り消しの後");
    assert_eq!(
        fixture.truth(),
        fixture.session.violation_total(),
        "取り消しの後も総数は真の総数と一致する"
    );
}

/// 「違反あり」の絞り込み（`FilterSpec::HasViolation`）が読む**据え付け**も、編集の差分で据え直
/// される（要件 4.3, 4.6）。据え直さないと、違反を得た行は現れず（印が足されない）、違反を
/// 失った行は消えない（印が残る）— どちらも表示の指定を当て直して観測する。
///
/// 標本の違反は**行優先ではなく列優先の平坦添字**で等間隔に選ばれるため、40 行 × 13 列の標本では
/// 違反を持つ行（行添字 4, 9, 14, …）が**すべての列**で違反し、それ以外の行は 1 件も持たない
/// （`tests/common/sample.rs` の「適合する値は違反を 1 件も生まず…」）。したがって「唯一の違反を
/// 直す行」は標本に無い — 検査が**違反を 1 件だけ持つ行を作って**から、同じ道を通す。
///
/// 可視の集合は**テスト自身の全件検証**から導いた真値と突き合わせる（据え付けだけを見る検査は、
/// 据え付けが索引と食い違っていても緑になりうる）。
#[test]
fn the_has_violation_filter_follows_the_edited_presence() {
    let mut fixture = Fixture::violating();
    fixture.indexed();
    let sample = fixture.sample_premises();
    let column = 1usize; // 数量（範囲つき整数。違反する値は -1、適合する値は 5）
    let spec = ViewSpec {
        sort: Vec::new(),
        filters: vec![FilterSpec::HasViolation { column: None }],
    };

    // 違反を 1 件も持たない行（そこへ違反を 1 件だけ作る → 直すと再び clean になる）。
    let clean_index = (0..fixture.rows())
        .find(|row| sample.row_is_clean(*row))
        .expect("前提: 違反を 1 件も持たない行がある");
    let clean = fixture.row(clean_index);
    // 前提: 標本の違反を持つ行はすべての列で違反している（上の docs の理由）。
    let violating_index = (0..fixture.rows())
        .find(|row| !sample.row_is_clean(*row))
        .expect("前提: 違反を持つ行がある");
    assert_eq!(
        (0..sample.column_count()).collect::<Vec<_>>(),
        sample.violating_columns(violating_index),
        "前提: 標本の違反を持つ行はすべての列で違反している"
    );

    fixture.set_view(spec.clone());
    assert_visible_rows_match_truth(&fixture, "指定を当てた直後");
    assert!(
        !visible_keys(&fixture).contains(&row_key(clean)),
        "前提: 違反を 1 件も持たない行は隠れている"
    );

    // 1. 違反を 1 件だけ作る（行が可視の集合へ入る）。
    fixture.apply(EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(clean, ColumnIndex::new(column)),
            "-1".to_owned(),
        )],
    });

    fixture.set_view(spec.clone());
    assert_visible_rows_match_truth(&fixture, "違反を得た後");
    assert!(
        visible_keys(&fixture).contains(&row_key(clean)),
        "違反を得た行は「違反あり」の絞り込みに入る"
    );

    // 2. その唯一の違反を解消する（行が可視の集合から外れる）。
    fixture.apply(EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(clean, ColumnIndex::new(column)),
            "5".to_owned(),
        )],
    });

    fixture.set_view(spec.clone());
    assert_visible_rows_match_truth(&fixture, "違反を失った後");
    assert!(
        !visible_keys(&fixture).contains(&row_key(clean)),
        "違反を失った行は「違反あり」の絞り込みから外れる"
    );
}

/// 世代は**画面に見えているものが変わりうる操作**の後だけ進む（要件 7.3。design.md の
/// Idempotency 句）。
///
/// 古い世代の窓を捨てさせるための札であり、**進めることだけが契約である**（`transport` の
/// `Generation`）。何も書かない命令では窓の内容が変わりようが無いため進めない。
#[test]
fn history_operations_advance_the_generation() {
    let mut fixture = Fixture::violating();
    fixture.indexed();
    let sample = fixture.sample_premises();
    let column = 1usize;
    let row_index = (0..fixture.rows())
        .find(|row| sample.violation_at(*row, column))
        .expect("前提: 列 1 に違反が仕込まれている");
    let row = fixture.row(row_index);

    // 何も書かない命令（0 行の挿入）は進めない。
    let at = RowOrdinal::new(document_ids(fixture.document(), fixture.sheet()).len());
    let baseline = fixture.session.generation();
    let idle = fixture.apply(EditCommand::InsertRows { at, count: 0 });
    assert!(idle.affected.is_empty(), "前提: 0 行の挿入は何も書かない");
    assert_eq!(
        baseline,
        fixture.session.generation(),
        "何も書かない命令は世代を進めない"
    );

    fixture.apply(EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(row, ColumnIndex::new(column)),
            "5".to_owned(),
        )],
    });
    let applied = fixture.session.generation();
    assert!(
        applied > baseline,
        "編集の適用は世代を進める（{baseline} → {applied}）"
    );

    fixture.undo();
    let undone = fixture.session.generation();
    assert!(
        undone > applied,
        "取り消しは世代を進める（{applied} → {undone}）"
    );

    fixture.redo();
    let redone = fixture.session.generation();
    assert!(
        redone > undone,
        "やり直しは世代を進める（{undone} → {redone}）"
    );

    // 何も書かない命令（セル 0 件の編集）も進めない。
    let idle = fixture.apply(EditCommand::SetCells { cells: Vec::new() });
    assert!(
        idle.affected.is_empty(),
        "前提: セル 0 件の編集は何も書かない"
    );
    assert_eq!(
        redone,
        fixture.session.generation(),
        "何も書かない命令は世代を進めない"
    );
}

// ---------------------------------------------------------------------------
// 履歴の 1 歩が**別のシート**へ落ちたとき（10.2 の差し戻し。要件 4.3、9.5）
// ---------------------------------------------------------------------------

/// 2 シートの標本の宣言: 品番（`text`・必須）と数量（`int`・0〜100）。
///
/// 数量の範囲を狭くしてあるのは、範囲外の値（`999`）を**値を書くだけで**置けるようにする
/// ためである — 編集を 1 度も適用せずに違反の索引を作れることが、次の 2 つの検査の前提である。
fn two_sheet_declaration() -> SchemaPart {
    let schema = Schema {
        columns: vec![
            ColumnDecl {
                name: "品番".into(),
                ty: TypeDecl::Kind {
                    kind: DeclaredKind::Known(TypeKind::Text),
                    constraints: Constraints::default(),
                },
                required: true,
                unique: false,
                default: None,
                description: None,
            },
            ColumnDecl {
                name: "数量".into(),
                ty: TypeDecl::Kind {
                    kind: DeclaredKind::Known(TypeKind::Int),
                    constraints: Constraints {
                        min: Some(CellValue::Int(0)),
                        max: Some(CellValue::Int(100)),
                        ..Constraints::default()
                    },
                },
                required: false,
                unique: false,
                default: None,
                description: None,
            },
        ],
    };
    let root = schema_to_text(&schema).expect("宣言は正準出力できる");
    SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#)).expect("宣言は解析できる")
}

/// 2 つのシート（`台帳` と `補助`）を持つ標本。どちらも同じ宣言・同じ列数・同じ 3 行である。
///
/// 数量に範囲外の値を置くと**そのシートだけが違反を持つ**（列 1 に `999` を 1 つ置けば
/// ちょうど 1 件）。
fn two_sheet_sample(
    ledger_quantities: [i64; 3],
    supplement_quantities: [i64; 3],
) -> (Document, SheetId, SheetId, RowId) {
    let mut document = Document::new();
    let ledger = document.add_sheet("台帳");
    let supplement = document.add_sheet("補助");
    let mut ledger_row = None;
    for (sheet, quantities) in [
        (ledger, ledger_quantities),
        (supplement, supplement_quantities),
    ] {
        document
            .set_sheet_columns(sheet, vec!["品番".to_owned(), "数量".to_owned()])
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, two_sheet_declaration())
            .expect("標本のシートは実在する");
        for (index, label) in ["A", "B", "C"].into_iter().enumerate() {
            let row = document.add_row(sheet).expect("標本のシートは実在する");
            document
                .set_row_values(
                    sheet,
                    row,
                    vec![
                        CellValue::Text(label.to_owned()),
                        CellValue::Int(quantities[index]),
                    ],
                )
                .expect("標本の行は実在する");
            if sheet == ledger && index == 0 {
                ledger_row = Some(row);
            }
        }
    }
    let ledger_row = ledger_row.expect("標本の台帳には先頭の行がある");
    (document, ledger, supplement, ledger_row)
}

/// **表示しているシートと履歴の 1 歩が食い違う**状況（表示は `補助`、履歴は `台帳` の編集）。
///
/// 履歴はドキュメント単位であるため（要件 9.5）、別のシートの操作を取り消す経路が現実に
/// ある。編集そのものは**本番と同じ経路**（`GridSession::apply`）で作る — 履歴へ積まれる
/// 逆命令が本物であることが、この検査の要点である。
struct OtherSheetStep {
    /// 標本の文書。
    document: Document,
    /// 履歴の 1 歩が落ちるシート（`台帳`）。
    ledger: SheetId,
    /// 編集した行（`台帳` の先頭の行）。
    row: RowId,
    /// **表示している**シートの操作口（`補助`）。
    session: GridSession,
    /// 呼び出し側が持つ履歴（要件 9.5。セッションは所有しない）。
    history: UndoStack,
}

impl OtherSheetStep {
    fn new(ledger_quantities: [i64; 3], supplement_quantities: [i64; 3]) -> Self {
        let (mut document, ledger, supplement, row) =
            two_sheet_sample(ledger_quantities, supplement_quantities);
        let plan = {
            let sheet = document
                .sheet_by_id(supplement)
                .expect("標本の補助は文書にある");
            SchemaEngine::new()
                .compile(sheet, &TypeRegistry::new())
                .expect("標本の宣言は計画へ落ちる")
        };
        // 台帳を編集し、その逆命令を履歴へ積む（本番と同じ経路である）。
        let mut ledger_session =
            GridSession::open(ledger, plan.clone()).expect("セッションを開ける");
        let mut history = UndoStack::new(DEFAULT_UNDO_LIMIT);
        ledger_session
            .apply(
                &mut document,
                &mut history,
                EditCommand::SetCells {
                    cells: vec![(CellAddress::new(row, ColumnIndex::new(1)), "9".to_owned())],
                },
            )
            .expect("台帳の編集は適用できる");
        // 表示は補助のまま（索引を組み立てる）。
        let mut session = GridSession::open(supplement, plan).expect("セッションを開ける");
        session
            .set_view(&document, no_view())
            .expect("表示を指定できる");
        Self {
            document,
            ledger,
            row,
            session,
            history,
        }
    }

    /// 取り消しを 1 つ進める（**履歴の 1 歩が台帳へ落ちる**）。
    fn undo(&mut self) -> EditOutcome {
        let Self {
            document,
            session,
            history,
            ..
        } = self;
        session
            .undo(document, history)
            .expect("取り消しは適用できる")
            .expect("取り消せる操作が 1 つある")
    }

    /// 台帳の編集したセルのいまの値。
    fn ledger_value(&self) -> CellValue {
        self.document
            .sheet_by_id(self.ledger)
            .expect("台帳は文書にある")
            .rows()
            .iter()
            .find(|found| found.id() == self.row)
            .expect("編集した行は台帳にある")
            .values()[1]
            .clone()
    }
}

/// **履歴の 1 歩が別のシートへ落ちても、表示中のシートの索引は動かない**（要件 4.3、9.5）。
///
/// 適用先が表示中のシートと違うとき、その適用が運ぶ違反は**別のシートのもの**である
/// （`EditOutcome::sheet` が適用先を名乗る）。表示中のシートの中身は 1 つも変わっていないため、
/// その索引（総数・据え付け・鍵）はそのままで正しく、触れば**表示中のシートの違反が黙って
/// 消える**（10.2 のレビューが実測した欠陥）。
#[test]
fn a_history_step_naming_another_sheet_leaves_the_displayed_index_alone() {
    // 前提: 台帳（編集するシート）は違反 0 件、補助（表示するシート）は違反 1 件。
    let mut step = OtherSheetStep::new([1, 2, 3], [999, 2, 3]);
    let total_before = step.session.violation_total();
    let found_before = step
        .session
        .find_violation(RowOrdinal::new(0), SearchDirection::Forward);
    assert_eq!(1, total_before, "前提: 表示中のシートは違反を 1 件持つ");
    assert!(found_before.is_some(), "前提: その違反は探索で見つかる");

    let outcome = step.undo();

    assert_eq!(
        step.ledger, outcome.sheet,
        "結果は適用先（履歴が名乗ったシート）を名乗る"
    );
    assert_eq!(
        CellValue::Int(1),
        step.ledger_value(),
        "取り消しは台帳の編集を戻す"
    );
    assert_eq!(
        total_before,
        step.session.violation_total(),
        "表示中のシートの違反の総数は動かない"
    );
    assert_eq!(
        found_before,
        step.session
            .find_violation(RowOrdinal::new(0), SearchDirection::Forward),
        "表示中のシートの違反の探索も動かない"
    );
}

/// **逆向きでも数の幽霊を作らない**（表示は違反 0 件、履歴の 1 歩は違反 1 件のシートへ落ちる）。
///
/// 台帳の復元は**全列を再検証**するため、その報告（違反 1 件）を表示中のシートの索引へ
/// 据えれば、表示には 1 件の違反があることになる（探索は 1 件も返さない）。数の源は
/// **索引が載せているシートの違反**でなければならない。
#[test]
fn a_history_step_naming_another_sheet_does_not_add_violations_to_the_displayed_sheet() {
    // 前提: 台帳（編集するシート）は違反 1 件、補助（表示するシート）は違反 0 件。
    let mut step = OtherSheetStep::new([999, 2, 3], [1, 2, 3]);
    assert_eq!(
        0,
        step.session.violation_total(),
        "前提: 表示中のシートは違反を 1 件も持たない"
    );
    assert_eq!(
        CellValue::Int(9),
        step.ledger_value(),
        "前提: 編集は台帳の値を変えている"
    );

    let outcome = step.undo();

    assert_eq!(
        step.ledger, outcome.sheet,
        "結果は適用先（履歴が名乗ったシート）を名乗る"
    );
    assert_eq!(
        1, outcome.violation_total,
        "前提: 台帳の復元は違反 1 件を報告する"
    );
    assert_eq!(
        0,
        step.session.violation_total(),
        "別のシートの違反が表示中のシートの総数へ混ざらない"
    );
    assert!(
        step.session
            .find_violation(RowOrdinal::new(0), SearchDirection::Forward)
            .is_none(),
        "探索も幽霊の違反を返さない"
    );
    assert_eq!(
        CellValue::Int(999),
        step.ledger_value(),
        "取り消しは台帳の編集を戻す"
    );
}

// ---------------------------------------------------------------------------
// **合成**された履歴の 1 歩（行の補充を伴う貼り付け）を別のシートで適用する
// （10.2 の 2 度目の差し戻し。要件 7.4、9.2）
// ---------------------------------------------------------------------------

/// 貼り付ける表形式テキスト（**5 行**。台帳の 3 行では足りないため行が補充される。要件 7.4）。
const REPLENISHING_PASTE: &str = "7\n8\n9\n10\n11";

/// **表示しているシートと、合成された履歴の 1 歩が食い違う**状況を作る。
///
/// 台帳へ 5 行の矩形を貼り付けると、3 行しか無いため**2 行が補充される**（要件 7.4）。その
/// 逆命令は「覆ったセルの値を戻す（`RestoreValues`。材料が**台帳**を名乗る）」と「補充した行を
/// 取り除く（`Edit(RemoveRows)`。シートを運ばず、**適用の対象シート**＝渡されたセッションの
/// シートを見る）」の**合成**である（`edit` 層の `paste_pair`）。
///
/// 貼り付けは**本番と同じ経路**（[`GridSession::apply`]）で適用する — 履歴へ積まれる合成が
/// 本物であることが、この検査の要点である。
struct CompositeOtherSheetStep {
    /// 標本の文書。
    document: Document,
    /// 履歴の 1 歩が落ちるシート（`台帳`）。
    ledger: SheetId,
    /// 台帳を表示したセッション（貼り付けと、向きを確かめるための取り消しに使う）。
    ledger_session: GridSession,
    /// **表示している**シート（`補助`）のセッション。
    supplement_session: GridSession,
    /// 呼び出し側が持つ履歴（要件 9.5。セッションは所有しない）。
    history: UndoStack,
}

impl CompositeOtherSheetStep {
    fn new() -> Self {
        let (mut document, ledger, supplement, _) = two_sheet_sample([1, 2, 3], [1, 2, 3]);
        let plan = {
            let sheet = document
                .sheet_by_id(supplement)
                .expect("標本の補助は文書にある");
            SchemaEngine::new()
                .compile(sheet, &TypeRegistry::new())
                .expect("標本の宣言は計画へ落ちる")
        };
        let mut history = UndoStack::new(DEFAULT_UNDO_LIMIT);
        let mut ledger_session =
            GridSession::open(ledger, plan.clone()).expect("セッションを開ける");
        ledger_session
            .set_view(&document, no_view())
            .expect("表示を指定できる");
        // 表示の並び（台帳の 3 行）を識別子で取り、**5 行**の矩形を数量の列へ貼る（2 行が補充される）。
        let displayed: Vec<RowId> = document
            .sheet_by_id(ledger)
            .expect("台帳は文書にある")
            .rows()
            .iter()
            .map(|row| row.id())
            .collect();
        ledger_session
            .apply(
                &mut document,
                &mut history,
                EditCommand::PasteRange {
                    anchor: CellAddress::new(displayed[0], ColumnIndex::new(1)),
                    rows: displayed,
                    text: REPLENISHING_PASTE.to_owned(),
                },
            )
            .expect("貼り付けは適用できる");
        let mut supplement_session =
            GridSession::open(supplement, plan).expect("セッションを開ける");
        supplement_session
            .set_view(&document, no_view())
            .expect("表示を指定できる");
        Self {
            document,
            ledger,
            ledger_session,
            supplement_session,
            history,
        }
    }

    /// 台帳の数量の並び（表示ではなく**文書**を見る）。
    fn ledger_quantities(&self) -> Vec<CellValue> {
        self.document
            .sheet_by_id(self.ledger)
            .expect("台帳は文書にある")
            .rows()
            .iter()
            .map(|row| row.values()[1].clone())
            .collect()
    }

    /// 台帳の行数。
    fn ledger_row_count(&self) -> usize {
        self.document
            .sheet_by_id(self.ledger)
            .expect("台帳は文書にある")
            .rows()
            .len()
    }

    /// **補助を見たまま**取り消す（履歴の 1 歩は台帳の合成を指す）。
    fn undo_on_supplement(&mut self) -> Result<Option<EditOutcome>, GridError> {
        self.supplement_session
            .undo(&mut self.document, &mut self.history)
    }

    /// 台帳を表示したまま取り消す（**同じ 1 歩**が成立する向き）。
    fn undo_on_ledger(&mut self) -> Result<Option<EditOutcome>, GridError> {
        self.ledger_session
            .undo(&mut self.document, &mut self.history)
    }

    /// **補助を見たまま**やり直す。
    fn redo_on_supplement(&mut self) -> Result<Option<EditOutcome>, GridError> {
        self.supplement_session
            .redo(&mut self.document, &mut self.history)
    }

    /// 台帳を表示したままやり直す。
    fn redo_on_ledger(&mut self) -> Result<Option<EditOutcome>, GridError> {
        self.ledger_session
            .redo(&mut self.document, &mut self.history)
    }
}

/// **合成の 1 歩は、表示中のシートを名乗らない部品を含むとき、何も書かずに拒まれる**（取り消し。
/// 要件 7.4、9.2。design.md「UndoStack」の限界 (2)）。
///
/// 合成の部品はどれも**組まれた時点の対象シート**（台帳）を指す。表示が補助のまま適用すると、
/// 前半（値の復元。材料が台帳を名乗る）は台帳へ届いて成功し、後半（`Edit(RemoveRows)`。シートを
/// 運ばず表示中のシート＝補助を見る）が補充した行を知らずに `UnknownRow` で止まる — **台帳は
/// 5 行のまま値だけが戻る**（10.2 のレビューが実測した欠陥）。編集層は「失敗したら 1 つのセルも
/// 書かない」を契約するため（design.md「EditApply」）、合成でもこれが保たれなければならない。
#[test]
fn undoing_a_composite_step_that_names_another_sheet_writes_nothing() {
    let mut step = CompositeOtherSheetStep::new();
    let quantities_before = step.ledger_quantities();
    assert_eq!(
        vec![
            CellValue::Int(7),
            CellValue::Int(8),
            CellValue::Int(9),
            CellValue::Int(10),
            CellValue::Int(11)
        ],
        quantities_before,
        "前提: 貼り付けは 5 行の数量を書く（台帳の 3 行では足りないため 2 行が補充される）"
    );
    assert_eq!(5, step.ledger_row_count(), "前提: 台帳は 5 行である");

    // **補助を見たまま取り消す。**先に「何も書かなかった」を確かめる — 部分適用の欠陥は、
    // 返る誤りの種類ではなく**文書の値**に現れる。
    let failure = step.undo_on_supplement();
    assert_eq!(
        quantities_before,
        step.ledger_quantities(),
        "台帳の値は 1 つも変わらない（前半の復元だけが届いてはならない）"
    );
    assert_eq!(5, step.ledger_row_count(), "台帳の行数も変わらない");
    assert_eq!(
        GridError::SchemaUnusable { sheet: step.ledger },
        failure.expect_err("表示中のシートを名乗らない部品を含む合成は拒まれる"),
        "拒む理由は、部品が名乗ったシートをこの適用先にできないことである"
    );

    // 2 度目も同じ誤りであり、**履歴の位置は動かない**。
    let again = step.undo_on_supplement();
    assert_eq!(
        quantities_before,
        step.ledger_quantities(),
        "2 度目も 1 つも書かない"
    );
    assert_eq!(
        GridError::SchemaUnusable { sheet: step.ledger },
        again.expect_err("2 度目も同じ誤りで拒まれる"),
        "2 度目も同じ理由である"
    );

    // 位置が動いていないことの観測: 台帳を表示したセッションからは、**同じ 1 歩**が取り消せる
    // （動いていれば、次に取り消せるものは無い）。**深さの比較は表明に値しない** —
    // `UndoStack::depth()` は積まれた対の総数であり、位置に依らない。
    step.undo_on_ledger()
        .expect("台帳からは同じ 1 歩を取り消せる")
        .expect("取り消せる操作がある");
    assert_eq!(
        vec![CellValue::Int(1), CellValue::Int(2), CellValue::Int(3)],
        step.ledger_quantities(),
        "取り消しは台帳の貼り付けを戻す"
    );
    assert_eq!(3, step.ledger_row_count(), "補充した行も取り除かれる");
}

/// **やり直しの向きでも同じである**（要件 7.4、9.3）。
///
/// やり直しの合成も「補充した行を差し戻す（`RestoreRows`。材料が台帳を名乗る）」と「貼り付けを
/// もう一度適用する（`Edit(PasteRange)`。シートを運ばない）」から成る。表示が補助のまま適用
/// すると、**前半だけが台帳へ届いて**取り消しで取り除いた 2 行が戻り、後半が補助の行を
/// 知らずに止まる。
#[test]
fn redoing_a_composite_step_that_names_another_sheet_writes_nothing() {
    let mut step = CompositeOtherSheetStep::new();
    // 台帳を表示したまま取り消し、**やり直しの対象を作る**（合成は表示中のシートに対しては
    // 成立する）。
    step.undo_on_ledger()
        .expect("取り消しは適用できる")
        .expect("取り消せる操作がある");
    let quantities_before = step.ledger_quantities();
    assert_eq!(
        vec![CellValue::Int(1), CellValue::Int(2), CellValue::Int(3)],
        quantities_before,
        "前提: 取り消しは台帳を 3 行へ戻す"
    );
    assert_eq!(3, step.ledger_row_count(), "前提: 補充した行は取り除かれる");

    // **補助を見たままやり直す。**
    let failure = step.redo_on_supplement();
    assert_eq!(
        quantities_before,
        step.ledger_quantities(),
        "台帳の値は 1 つも変わらない"
    );
    assert_eq!(
        3,
        step.ledger_row_count(),
        "差し戻した行も届かない（前半だけが適用されてはならない）"
    );
    assert_eq!(
        GridError::SchemaUnusable { sheet: step.ledger },
        failure.expect_err("表示中のシートを名乗らない部品を含む合成は拒まれる"),
        "拒む理由は、部品が名乗ったシートをこの適用先にできないことである"
    );

    // 2 度目も同じ誤りであり、履歴の位置は動かない。
    let again = step.redo_on_supplement();
    assert_eq!(
        quantities_before,
        step.ledger_quantities(),
        "2 度目も 1 つも書かない"
    );
    assert_eq!(
        GridError::SchemaUnusable { sheet: step.ledger },
        again.expect_err("2 度目も拒まれる"),
        "2 度目も同じ理由である"
    );

    // 位置が動いていないことの観測: 台帳を表示したセッションからは、**同じ 1 歩**がやり直せる。
    step.redo_on_ledger()
        .expect("台帳からは同じ 1 歩をやり直せる")
        .expect("やり直せる操作がある");
    assert_eq!(
        vec![
            CellValue::Int(7),
            CellValue::Int(8),
            CellValue::Int(9),
            CellValue::Int(10),
            CellValue::Int(11)
        ],
        step.ledger_quantities(),
        "やり直しは台帳の貼り付けを再び適用する"
    );
    assert_eq!(5, step.ledger_row_count(), "補充した行も戻る");
}

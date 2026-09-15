//! 取り消し履歴（命令と逆命令の対）の検査（データグリッドのタスク 4.1。data-grid 要件 6.6, 7.6,
//! 9.1, 9.5）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のものである。
//!
//! 1. **逆命令は適用時に生成される**。適用の後に作ることはできないため、材料（変更前の値・
//!    取り除かれた行の値と位置）は適用の**前**に読まれ、対に載る（要件 9.1）。
//! 2. **逆命令の適用が適用前の状態を復元する**。セルの編集・行の追加・削除・複製・貼り付けの
//!    5 つそれぞれについて、**識別子・値・位置**の 3 つを突き合わせる（要件 9.2）。とくに
//!    行の削除の逆命令（復元用の内部命令）は、取り除かれた行を**同じ `RowId`・同じ値・同じ
//!    位置**へ戻す（要件 6.6 の本体）。
//! 3. **セルの編集の逆命令は表示文字列を経由しない**。添付の列と入れ子の列では、表示文字列へ
//!    写して書き戻すと値の変種が変わる（添付は hex のテキストになり、入れ子は要約になる）ため、
//!    「変更前の表示文字列」を保持する逆命令は元の状態を復元できない。この 2 列でも復元が
//!    成立することを突き合わせる（材料は**値そのもの**である）。
//! 4. **登録口は `push` 1 つ**である（要件 9.1 の拡張点）。数式の再計算とマクロの実行の区分
//!    （`Recalculation` / `MacroRun`）を先に用意し、**本タスクが生成しない区分の対も同じ 1 つの
//!    登録口から積める**ことを示す（後続スペックが履歴の内部構造に触れずに乗れる）。
//! 5. **履歴はドキュメント単位であり、シートごとではない**（要件 9.5）。1 つの履歴が 2 つの
//!    シートの操作を同時に保持し、取り消しが全体で 1 つの後入れ先出しであることを示す。
//! 6. **1 回の貼り付けは 1 つの操作である**（要件 7.6）。貼り付けの逆命令は複数の部分から
//!    成るが、履歴には 1 つの対として積まれ、1 回の適用で元の状態へ戻る。
//! 7. **決定性**。同じ文書と同じ命令の並びは同じ**形**の履歴を与える（識別子は実行ごとに
//!    変わるため比較しない。標本の契約）。
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
//! **標本の識別子は発行のたびに変わる**ため、生の識別子や生の値を実行を跨いで比較しない。
//! 比較するのは**構造**（区分・種類・規模・位置）と、**同一の実行の中での**識別子・値である。

mod common;

use common::sample::{sample, Sample, SampleEditParts, SampleOptions};
use data_grid::{
    CellAddress, ColumnIndex, EditApply, EditCommand, FilterSpec, HistoryCommand, HistoryPair,
    RowOrder, RowOrdinal, SortKey, UndoEntry, UndoLabel, UndoStack, ViewSpec,
};
use document_format::{CellValue, Document, RowId, SchemaPart, SheetId};
use schema_engine::{
    validate_sheet, CompiledSchema, SchemaEngine, SchemaEngineApi, TypeRegistry, ValidationOptions,
};

/// 標本の添付の列（表示文字列へ写すと hex のテキストになり、値の変種が変わる列）。
const ATTACHMENT_COLUMN: usize = 9;

/// 標本の入れ子（オブジェクト）の列（表示文字列へ写すと要約になる列）。
const NESTED_COLUMN: usize = 10;

/// 標本の整数の列（表示文字列を経由しても型が変わらない列）。
const INT_COLUMN: usize = 1;

/// 標本の真偽の列（表示文字列を経由しても型が変わらない列）。
const BOOL_COLUMN: usize = 4;

// ---------------------------------------------------------------------------
// 標本と前提
// ---------------------------------------------------------------------------

/// 編集に要る部品へ分解した標本。
struct Fixture {
    /// 標本の部品（文書・シート・行識別子・列名）。
    parts: SampleEditParts,
    /// 標本の宣言（2 つ目のシートを同じ宣言で作るために取っておく）。
    schema_part: SchemaPart,
    /// 開いた計画。
    plan: CompiledSchema,
}

impl Fixture {
    /// 違反を 1 件も仕込まない標本（前提を先に確かめてから依拠する）。
    fn clean(rows: usize, columns: usize) -> Self {
        let sample = sample(&SampleOptions::new(rows, columns).with_ratio(0.0));
        assert_clean_premises(&sample);
        // 宣言と計画は**分解の前**に取り出す（分解の後は標本が文書を手放す）。
        let schema_part = sample.schema_part().clone();
        let plan = sample.compiled();
        Self {
            parts: sample.into_edit_parts(),
            schema_part,
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

    /// 文章の位置 `index` の行の識別子（前提により、標本の文書順は行識別子の順である）。
    fn row(&self, index: usize) -> RowId {
        self.parts.row_ids[index]
    }

    fn row_count(&self) -> usize {
        self.parts.row_ids.len()
    }

    /// 適用の経路（本番の縫い目）。
    fn apply(&self) -> EditApply {
        EditApply::new(self.sheet(), self.plan.clone())
    }

    /// ドキュメント全体の行識別子と値（適用の前後の比較用。**識別子を含む**）。
    fn snapshot(&self) -> Vec<(RowId, Vec<CellValue>)> {
        snapshot(self.document(), self.sheet())
    }

    /// 指定した行の値の並び。
    fn values_of(&self, row: RowId) -> Vec<CellValue> {
        values_of(self.document(), self.sheet(), row)
    }

    /// 指定したセルの値。
    fn value_at(&self, row: RowId, column: usize) -> CellValue {
        values_of(self.document(), self.sheet(), row)
            .get(column)
            .cloned()
            .unwrap_or(CellValue::Null)
    }

    /// 適用の後のシートの違反の総数（全件検証）。
    fn violations_now(&self) -> usize {
        validate_sheet(
            self.document(),
            self.sheet(),
            &self.plan,
            &ValidationOptions::unlimited(),
        )
        .total_violations()
    }

    /// 2 つ目のシートを同じ宣言で足し、4 行を標本の先頭 4 行と同じ値で埋める。
    ///
    /// 履歴が**シートごとではない**ことを示す検査の前提である（要件 9.5）。同じ宣言を使うのは
    /// 2 つ目の計画を手書きの宣言から作らないためである（宣言の写しを作らない）。
    fn add_second_sheet(&mut self, name: &str) -> (SheetId, CompiledSchema) {
        let second = self.parts.document.add_sheet(name);
        self.parts
            .document
            .set_sheet_columns(second, self.parts.columns.clone())
            .expect("足したシートは文書にある");
        self.parts
            .document
            .set_root_schema(second, self.schema_part.clone())
            .expect("足したシートは文書にある");
        for index in 0..4 {
            let values = values_of(self.document(), self.sheet(), self.row(index));
            let row = self
                .document_mut()
                .add_row(second)
                .expect("足したシートは文書にある");
            self.document_mut()
                .set_row_values(second, row, values)
                .expect("足した行はシートにある");
        }
        let plan = SchemaEngine::new()
            .compile(
                self.document().sheet_by_id(second).expect("シートは文書にある"),
                &TypeRegistry::new(),
            )
            .expect("同じ宣言はもう一度コンパイルできる");
        (second, plan)
    }
}

/// 標本を使う検査の前提を先に確かめる（モジュール docs「前提を先に確かめる」）。
fn assert_clean_premises(sample: &Sample) {
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
        sample.columns().len(),
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

/// 列数に満たない値しか持たない行を 1 つ足し、その識別子を返す（短い行の検査の前提）。
///
/// 新しい行は値を持たない。列 1 だけを打つと、その行の値の並びは 2 個になる（列 0 は値なし）。
/// 「値の並びが列数に満たない行は埋めない」ことは `tests/edit_rows.rs` の
/// `duplicating_a_short_row_copies_exactly_the_values_it_has` が固定する規律であり、
/// 本ファイルは**その規律の下で往復が成立すること**を検査する。
fn add_short_row(apply: &mut EditApply, fixture: &mut Fixture, text: &str) -> RowId {
    let sheet = fixture.sheet();
    let row = fixture
        .document_mut()
        .add_row(sheet)
        .expect("行を追加できない");
    apply
        .apply(
            fixture.document_mut(),
            set_cell(row, INT_COLUMN, text),
        )
        .expect("適合する値の書き込みは成功する");
    assert!(
        fixture.values_of(row).len() < fixture.plan.column_count(),
        "前提: 値の並びが列数に満たない行（{} < {}）",
        fixture.values_of(row).len(),
        fixture.plan.column_count()
    );
    row
}

/// 値の並びを 1 つも持たない行を 1 つ足し、その識別子を返す（幅 0 の検査の前提）。
///
/// [`Document::add_row`] は**値を持たない行**を作る。値の並びを 1 つも書かなければ幅は 0 の
/// ままである（`Row` の幅は「最後に書いた列 + 1」ではなく「値の並びの長さ」である）。
fn add_zero_width_row(fixture: &mut Fixture) -> RowId {
    let sheet = fixture.sheet();
    let row = fixture
        .document_mut()
        .add_row(sheet)
        .expect("行を追加できない");
    assert!(
        fixture.values_of(row).is_empty(),
        "前提: 値の並びを 1 つも持たない行（幅 0）"
    );
    row
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

/// 指定した行の値を読み出す。
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

/// 打たれた文字を 1 つのセルへ書く編集。
fn set_cell(row: RowId, column: usize, text: &str) -> EditCommand {
    EditCommand::SetCells {
        cells: vec![(
            CellAddress::new(row, ColumnIndex::new(column)),
            text.to_owned(),
        )],
    }
}

/// 適用の結果（状態を変えない適用では対が無い）から、履歴へ積む 1 件を作る。
fn entry(label: UndoLabel, pair: Option<HistoryPair>) -> UndoEntry {
    let pair = pair.expect("状態を変える適用は対を持つ");
    UndoEntry {
        label,
        inverse: pair.inverse,
        redo: pair.redo,
    }
}

// ---------------------------------------------------------------------------
// セルの編集（要件 9.1, 9.2）
// ---------------------------------------------------------------------------

/// セルの編集の逆命令が、適用前の状態を**表示文字列を経由せず**に戻すこと。
///
/// 4 つの列で固定する: 整数と真偽の列（表示文字列を経由しても値の型が変わらない列）、入れ子の
/// 列（表示文字列は要素数の要約になる）、添付の列（表示文字列は hex のテキストになる）。後ろの
/// 2 列で復元が成立することが、「逆命令が**値そのもの**を保持している」ことの証拠である
/// （表示文字列を保持する逆命令は、変種が変わった値を書き戻す）。
#[test]
fn a_cell_edit_is_undone_to_the_values_read_at_apply_time() {
    for column in [INT_COLUMN, BOOL_COLUMN, NESTED_COLUMN, ATTACHMENT_COLUMN] {
        let mut fixture = Fixture::clean(16, 13);
        let mut apply = fixture.apply();
        let row = fixture.row(2);
        let before = fixture.snapshot();
        let original = fixture.value_at(row, column);
        assert_ne!(
            CellValue::Text("うっかり".to_owned()),
            original,
            "前提: 書く前の値は打たれる文字と違う"
        );

        let (outcome, pair) = apply
            .apply_with_inverse(fixture.document_mut(), set_cell(row, column, "うっかり"))
            .expect("編集は適用できる");
        assert_eq!(vec![row], outcome.affected, "編集した行が報告される");
        let pair = pair.expect("状態を変える適用は対を持つ");

        // 逆命令は**適用の前**に読んだ値をそのまま保持する（変更した行の、変更前の値の並び）。
        let (sheet, restored) = match &pair.inverse {
            HistoryCommand::RestoreValues { sheet, rows } => (*sheet, rows.clone()),
            other => panic!("セルの編集の逆命令は値の復元である: {other:?}"),
        };
        assert_eq!(fixture.sheet(), sheet, "逆命令は対象シートを名乗る");
        assert_eq!(1, restored.len(), "触れた行は 1 つ");
        assert_eq!(row, restored[0].id, "触れた行の識別子");
        assert_eq!(
            before
                .iter()
                .position(|(id, _)| *id == row)
                .expect("行は適用前の文書にある"),
            restored[0].position,
            "位置は適用前の文書の位置"
        );
        assert_eq!(
            &before[restored[0].position].1,
            &restored[0].values,
            "値は適用前の行の値の並びそのもの（列 {column}）"
        );

        // 逆命令の適用が適用前の状態を復元する（識別子・値・位置）。
        apply
            .apply_history(fixture.document_mut(), &pair.inverse)
            .expect("逆命令は適用できる");
        assert_eq!(before, fixture.snapshot(), "適用前の状態へ戻る（列 {column}）");
        assert_eq!(0, fixture.violations_now(), "違反も元へ戻る（列 {column}）");

        // やり直しの命令も同じ編集である（適用の時点で組になったもの）。
        let HistoryCommand::Edit(EditCommand::SetCells { cells }) = &pair.redo else {
            panic!("セルの編集のやり直しは同じ命令である: {:?}", pair.redo);
        };
        assert_eq!(
            &vec![(
                CellAddress::new(row, ColumnIndex::new(column)),
                "うっかり".to_owned()
            )],
            cells,
            "やり直しは適用した命令そのものである"
        );
    }
}

/// 逆命令が**適用時に**読んだ値を持つこと — 後から同じセルを編集しても、先の対は先の適用の
/// 前の値へ戻す（要件 9.1「適用後には作れないため」）。
#[test]
fn an_inverse_keeps_the_value_it_read_and_is_not_affected_by_later_edits() {
    let mut fixture = Fixture::clean(8, 4);
    let mut apply = fixture.apply();
    let row = fixture.row(1);
    let original = fixture.value_at(row, INT_COLUMN);
    let before = fixture.snapshot();

    let (_, first) = apply
        .apply_with_inverse(fixture.document_mut(), set_cell(row, INT_COLUMN, "1"))
        .expect("1 回目の編集は適用できる");
    let first = first.expect("1 回目の適用は状態を変える");
    assert_eq!(CellValue::Int(1), fixture.value_at(row, INT_COLUMN));

    // 2 回目の編集は同じセルを別の値にする（1 回目の対の材料を上書きしない）。
    let (_, second) = apply
        .apply_with_inverse(fixture.document_mut(), set_cell(row, INT_COLUMN, "2"))
        .expect("2 回目の編集は適用できる");
    assert!(second.is_some(), "2 回目の適用も状態を変える");

    // 1 回目の逆命令は、2 回目の編集の結果（1）ではなく、1 回目の適用の前の値へ戻す。
    apply
        .apply_history(fixture.document_mut(), &first.inverse)
        .expect("1 回目の逆命令は適用できる");
    assert_eq!(
        original,
        fixture.value_at(row, INT_COLUMN),
        "逆命令は自分が適用時に読んだ値へ戻す"
    );
    assert_eq!(before, fixture.snapshot(), "適用前の状態へ戻る");
}

/// 空の命令は何も変えず、**対も作らない**（状態を変えない操作を積むと、取り消しが何も戻さない
/// 操作になる。`edit` のモジュール docs「履歴（4.x）との境目」）。
#[test]
fn an_empty_command_changes_nothing_and_leaves_no_pair() {
    let mut fixture = Fixture::clean(4, 4);
    let mut apply = fixture.apply();
    let before = fixture.snapshot();

    let empties = vec![
        EditCommand::SetCells { cells: Vec::new() },
        EditCommand::InsertRows {
            at: RowOrdinal::new(0),
            count: 0,
        },
        EditCommand::RemoveRows { rows: Vec::new() },
        EditCommand::DuplicateRows { rows: Vec::new() },
        EditCommand::PasteRange {
            anchor: CellAddress::new(fixture.row(0), ColumnIndex::new(0)),
            rows: fixture.parts.row_ids.clone(),
            text: String::new(),
        },
    ];
    for command in empties {
        let (_, pair) = apply
            .apply_with_inverse(fixture.document_mut(), command.clone())
            .expect("空の命令も成功する");
        assert_eq!(None, pair, "空の命令は対を持たない: {command:?}");
    }
    assert_eq!(before, fixture.snapshot(), "空の命令は何も変えない");
}

// ---------------------------------------------------------------------------
// 行の追加・削除・複製（要件 6.6, 9.1, 9.2）
// ---------------------------------------------------------------------------

/// 行の追加の逆命令が、追加された行を**取り除いて**適用前の状態へ戻すこと（要件 6.6）。
#[test]
fn an_inserted_row_is_undone_by_removing_the_added_rows() {
    let mut fixture = Fixture::clean(8, 4);
    let mut apply = fixture.apply();
    let before = fixture.snapshot();
    let at = 3;

    let (outcome, pair) = apply
        .apply_with_inverse(
            fixture.document_mut(),
            EditCommand::InsertRows {
                at: RowOrdinal::new(at),
                count: 2,
            },
        )
        .expect("行の追加は適用できる");
    assert_eq!(10, outcome.row_count, "適用後の行数");
    let pair = pair.expect("状態を変える適用は対を持つ");
    let HistoryCommand::Edit(EditCommand::RemoveRows { rows }) = &pair.inverse else {
        panic!("行の追加の逆命令は追加された行の削除である: {:?}", pair.inverse);
    };
    assert_eq!(
        &outcome.affected, rows,
        "逆命令は追加された行（`affected`）をそのまま保持する"
    );
    // 挿入された行は**発行されたばかり**であり、適用前のどの行でもない（挿入位置にあった行は
    // 1 つ後ろへずれるだけである）。
    assert!(
        before.iter().all(|(id, _)| !rows.contains(id)),
        "逆命令は適用前の行ではなく、新しく発行された行を保持する"
    );
    assert_eq!(2, outcome.affected.len(), "挿入した 2 行が報告される");

    // やり直しは**発行済みの行をそのまま差し戻す**（追加し直すと識別子が変わり、上の逆命令が
    // 指す行が消える）。
    let HistoryCommand::RestoreRows {
        sheet,
        rows: restored,
    } = &pair.redo
    else {
        panic!("行の追加のやり直しは差し戻しである: {:?}", pair.redo);
    };
    assert_eq!(&fixture.sheet(), sheet, "やり直しは対象シートを名乗る");
    assert_eq!(
        vec![(at, outcome.affected[0]), (at + 1, outcome.affected[1])],
        restored
            .iter()
            .map(|row| (row.position, row.id))
            .collect::<Vec<_>>(),
        "差し戻す位置は適用で挿入された位置である"
    );

    apply
        .apply_history(fixture.document_mut(), &pair.inverse)
        .expect("逆命令は適用できる");
    assert_eq!(
        before,
        fixture.snapshot(),
        "識別子・値・位置が適用前へ戻る（既存の行は 1 つも動かない）"
    );
}

/// 行の削除の逆命令が、取り除いた行の**値・識別子・位置**を保持し、適用前の状態へ戻すこと
/// （要件 6.6 / 9.2 の本体）。隣り合わない 2 行を 1 回の削除で取り除き、位置まで戻ることを示す。
#[test]
fn a_removed_row_is_restored_with_the_same_identifier_values_and_position() {
    let mut fixture = Fixture::clean(8, 4);
    let mut apply = fixture.apply();
    let before = fixture.snapshot();
    let removed_ids = vec![fixture.row(2), fixture.row(5)];

    let (outcome, pair) = apply
        .apply_with_inverse(
            fixture.document_mut(),
            EditCommand::RemoveRows {
                rows: removed_ids.clone(),
            },
        )
        .expect("行の削除は適用できる");
    assert_eq!(6, outcome.row_count, "2 行減る");
    let pair = pair.expect("状態を変える適用は対を持つ");
    let HistoryCommand::RestoreRows { sheet, rows } = &pair.inverse else {
        panic!("行の削除の逆命令は差し戻しである: {:?}", pair.inverse);
    };
    assert_eq!(&fixture.sheet(), sheet, "逆命令は対象シートを名乗る");
    assert_eq!(2, rows.len());
    // 材料は**適用の前**に読まれている（適用後には値も位置も存在しない）。
    for restored in rows {
        let (position, original) = before
            .iter()
            .enumerate()
            .find(|(_, (id, _))| *id == restored.id)
            .expect("取り除いた行は適用前の文書にある");
        assert_eq!(position, restored.position, "位置は適用前の文書の位置");
        assert_eq!(&original.1, &restored.values, "値は適用前のそのまま");
    }
    assert_eq!(
        vec![2usize, 5],
        rows.iter().map(|row| row.position).collect::<Vec<_>>(),
        "位置は適用前の文書の位置（昇順）"
    );
    assert_eq!(
        removed_ids,
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        "識別子は取り除かれた行そのもの（シート順）"
    );
    assert_eq!(
        removed_ids, outcome.affected,
        "取り除かれた行の報告と一致する"
    );

    apply
        .apply_history(fixture.document_mut(), &pair.inverse)
        .expect("逆命令は適用できる");
    assert_eq!(
        before,
        fixture.snapshot(),
        "識別子・値・位置が適用前へ戻る（識別子も同じ）"
    );

    // やり直しは同じ識別子をもう一度取り除く（発行済みの行を指しているため、そのまま使える）。
    apply
        .apply_history(fixture.document_mut(), &pair.redo)
        .expect("やり直しは適用できる");
    assert_eq!(6, ids_of(fixture.document(), fixture.sheet()).len());
}

/// 行の複製の逆命令が、複製された行を取り除いて適用前の状態へ戻すこと（要件 6.6）。
#[test]
fn a_duplicated_row_is_undone_by_removing_the_copies() {
    let mut fixture = Fixture::clean(8, 4);
    let mut apply = fixture.apply();
    let before = fixture.snapshot();
    let sources = vec![fixture.row(1), fixture.row(4)];

    let (outcome, pair) = apply
        .apply_with_inverse(
            fixture.document_mut(),
            EditCommand::DuplicateRows { rows: sources },
        )
        .expect("行の複製は適用できる");
    assert_eq!(10, outcome.row_count, "2 行増える");
    let pair = pair.expect("状態を変える適用は対を持つ");
    let HistoryCommand::Edit(EditCommand::RemoveRows { rows }) = &pair.inverse else {
        panic!("行の複製の逆命令は複製の削除である: {:?}", pair.inverse);
    };
    assert_eq!(&outcome.affected, rows, "逆命令は複製の識別子を保持する");

    // やり直しは、複製の値（元の行と同じ値）をそのまま差し戻す。
    let HistoryCommand::RestoreRows {
        rows: restored, ..
    } = &pair.redo
    else {
        panic!("行の複製のやり直しは差し戻しである: {:?}", pair.redo);
    };
    for (copy, source_index) in restored.iter().zip([1usize, 4]) {
        assert_eq!(
            values_of(fixture.document(), fixture.sheet(), fixture.row(source_index)),
            copy.values,
            "複製の値は元の行の値と同じ"
        );
    }

    apply
        .apply_history(fixture.document_mut(), &pair.inverse)
        .expect("逆命令は適用できる");
    assert_eq!(before, fixture.snapshot(), "適用前の状態へ戻る");
}

/// 呼び出しの形が変わっても、削除の対の材料は常に同じ（要求の並びに依らない）。
///
/// 削除は同じ行集合の要求を 1 回に畳み、**シート順**で報告する（3.2 の規律）。対の材料も同じ
/// 規律であることを、要求の並びを逆にした 2 回の削除で突き合わせる（決定性）。
#[test]
fn the_restore_material_does_not_depend_on_the_order_of_the_request() {
    let mut fixture = Fixture::clean(8, 4);
    let mut apply = fixture.apply();
    let request = vec![fixture.row(5), fixture.row(2), fixture.row(5)];

    let (_, pair) = apply
        .apply_with_inverse(
            fixture.document_mut(),
            EditCommand::RemoveRows {
                rows: request.clone(),
            },
        )
        .expect("行の削除は適用できる");
    let pair = pair.expect("状態を変える適用は対を持つ");
    let HistoryCommand::RestoreRows { rows, .. } = &pair.inverse else {
        panic!("行の削除の逆命令は差し戻しである");
    };
    assert_eq!(
        vec![2usize, 5],
        rows.iter().map(|row| row.position).collect::<Vec<_>>(),
        "重複した要求は 1 回へ畳まれ、位置はシート順になる"
    );
}

// ---------------------------------------------------------------------------
// 貼り付け（要件 7.6）
// ---------------------------------------------------------------------------

/// 1 回の貼り付けは**1 つの操作**として取り消される（要件 7.6）。行を補充した貼り付けの逆命令は
/// 値の復元と行の削除の 2 つの部分から成るが、履歴には 1 つの対として積まれ、**1 回の適用**で
/// 適用前の状態（識別子・値・位置）へ戻る。
#[test]
fn one_paste_is_one_undo_operation() {
    let mut fixture = Fixture::clean(8, 4);
    let mut apply = fixture.apply();
    let stack_row = fixture.row_count() - 2;
    let text = "あ\tい\nう\tえ\nお\tか\nき\tく";
    let command = EditCommand::PasteRange {
        anchor: CellAddress::new(fixture.row(stack_row), ColumnIndex::new(1)),
        rows: fixture.parts.row_ids.clone(),
        text: text.to_owned(),
    };
    let before = fixture.snapshot();

    let (outcome, pair) = apply
        .apply_with_inverse(fixture.document_mut(), command.clone())
        .expect("貼り付けは適用できる");
    assert_eq!(
        10, outcome.row_count,
        "矩形の行が表示の並びを超えるので 2 行補充される"
    );
    let pair = pair.expect("状態を変える適用は対を持つ");
    let HistoryCommand::Composite(parts) = &pair.inverse else {
        panic!("行を補充した貼り付けの逆命令は合成である: {:?}", pair.inverse);
    };
    assert_eq!(2, parts.len(), "値の復元と、補充した行の削除");
    assert!(
        matches!(parts[0], HistoryCommand::RestoreValues { .. }),
        "先に**貼り付けが覆ったセル**の値（変更前）を戻す"
    );
    assert!(
        matches!(
            parts[1],
            HistoryCommand::Edit(EditCommand::RemoveRows { .. })
        ),
        "そのあとで補充した行を取り除く"
    );

    // 履歴には 1 つの対として積まれる（1 回の貼り付け = 1 つの操作）。
    let mut stack = UndoStack::new(16);
    stack.push(entry(UndoLabel::of_edit(&command), Some(pair.clone())));
    assert_eq!(1, stack.depth(), "貼り付けは 1 つの操作として積まれる");

    let inverse = stack.undo().expect("取り消しの対象がある").clone();
    assert_eq!(
        pair.inverse, inverse,
        "取り消しは積んだ貼り付けの逆命令そのものである"
    );
    apply
        .apply_history(fixture.document_mut(), &inverse)
        .expect("貼り付けの逆命令は適用できる");
    assert_eq!(
        before,
        fixture.snapshot(),
        "識別子・値・位置が適用前へ戻る（補充した行も消える）"
    );
    assert_eq!(8, ids_of(fixture.document(), fixture.sheet()).len());

    // やり直しも 1 つの対である（補充した行を差し戻してから、覆ったセルの値を書き戻す）。
    let redone = stack.redo().expect("やり直しの対象がある").clone();
    apply
        .apply_history(fixture.document_mut(), &redone)
        .expect("やり直しは適用できる");
    assert_eq!(
        snapshot(fixture.document(), fixture.sheet()).len(),
        10,
        "やり直しで補充した行が戻る"
    );

    // 行を補充しない貼り付けの逆命令は、値の復元**だけ**である。
    let mut fixture = Fixture::clean(8, 4);
    let mut apply = fixture.apply();
    let command = EditCommand::PasteRange {
        anchor: CellAddress::new(fixture.row(1), ColumnIndex::new(1)),
        rows: fixture.parts.row_ids.clone(),
        text: "x\ty\nz\tw".to_owned(),
    };
    let before = fixture.snapshot();
    let (_, pair) = apply
        .apply_with_inverse(fixture.document_mut(), command)
        .expect("貼り付けは適用できる");
    let pair = pair.expect("状態を変える適用は対を持つ");
    assert!(
        matches!(pair.inverse, HistoryCommand::RestoreValues { .. }),
        "補充が無ければ逆命令は値の復元だけである: {:?}",
        pair.inverse
    );
    apply
        .apply_history(fixture.document_mut(), &pair.inverse)
        .expect("逆命令は適用できる");
    assert_eq!(before, fixture.snapshot(), "適用前の状態へ戻る");
}

// ---------------------------------------------------------------------------
// 表示の並びが文書の並びと食い違うときの往復（要件 8.6, 8.9, 9.2）
// ---------------------------------------------------------------------------

/// **表示の並びが文書の並びと食い違う**ときの貼り付けの往復が、適用前の状態をそのまま戻す
/// こと（要件 8.6, 8.9 と 9.2 の交わり）。
///
/// 貼り付けの宛先は「錨（物理の行）」と「表示されている行の並び」の 2 つの成分で決まる
/// （`edit` のモジュール docs「誰が表示の座標を物理の座標へ写すか」）。絞り込みや並べ替えが
/// 効いていると**表示の並びの位置は文書の位置と一致しない**ため、逆命令の材料を文書の位置で
/// 引くと**別の行**を指してしまう — 取り消しが、実際に書いた行ではなく他の行の値を戻し、
/// 書いた行は変わったまま残る。
///
/// 本検査は前提を先に確かめる（表示の並びが文書の並びと食い違うこと、絞り込みで実際に
/// 行が隠れていること）うえで、適用・取り消しの前後で**文書全体**（識別子・値・位置）を
/// 突き合わせる。
#[test]
fn a_paste_round_trip_under_a_filter_restores_the_rows_it_actually_wrote() {
    let mut fixture = Fixture::clean(32, 13);
    let mut apply = fixture.apply();
    let document_ids = ids_of(fixture.document(), fixture.sheet());
    // 真偽の列で絞り込む（標本の列 4 は真偽であり、両方の値が現れる）。
    let view = ViewSpec {
        sort: Vec::new(),
        filters: vec![FilterSpec::Equals {
            column: ColumnIndex::new(BOOL_COLUMN),
            text: "true".to_owned(),
        }],
    };
    let mut order = RowOrder::default();
    let summary = order.recompute(fixture.document(), fixture.sheet(), &view);
    let displayed: Vec<RowId> = (0..summary.visible)
        .map(|ordinal| order.row_at(RowOrdinal::new(ordinal)).expect("可視行"))
        .collect();

    // 前提: 絞り込みで行が隠れており、**表示の並びが文書の並びと食い違う**。
    assert!(summary.hidden > 0, "前提: 絞り込みで行が隠れる");
    assert!(displayed.len() > 2, "前提: 貼り付けの行が残る");
    assert_ne!(
        document_ids, displayed,
        "前提: 表示の並びが文書の並びと食い違う（本検査の対象そのもの）"
    );

    // 表示の並びの**先頭**に貼る（文書の先頭ではない — 錨は表示の 1 行目である）。
    let anchor = displayed[0];
    let text = displayed
        .iter()
        .take(3)
        .map(|_| "z\tw")
        .collect::<Vec<_>>()
        .join("\n");
    let before = fixture.snapshot();
    let (_, pair) = apply
        .apply_with_inverse(
            fixture.document_mut(),
            EditCommand::PasteRange {
                anchor: CellAddress::new(anchor, ColumnIndex::new(INT_COLUMN)),
                rows: displayed.clone(),
                text,
            },
        )
        .expect("貼り付けは適用できる");
    let pair = pair.expect("状態を変える適用は対を持つ");
    assert_ne!(before, fixture.snapshot(), "前提: 貼り付けが文書を変えた");

    // **適用前の状態へ戻ること**が本検査の本体である（表示と文書が食い違うと、材料を文書の
    // 位置で引く実装は別の行を戻し、実際に書いた行は変わったまま残る）。
    apply
        .apply_history(fixture.document_mut(), &pair.inverse)
        .expect("逆命令は適用できる");
    assert_eq!(
        before,
        fixture.snapshot(),
        "識別子・値・位置のすべてが適用前へ戻る（表示と文書が食い違っていても）"
    );

    // 材料が**書いた行**（表示の並びの先頭 3 行）を指すことも突き合わせる（上の表明の
    // 診断である — 食い違うと、どの行を指すべきだったかがここに出る）。
    let written: Vec<RowId> = displayed.iter().copied().take(3).collect();
    let HistoryCommand::RestoreValues { rows, .. } = &pair.inverse else {
        panic!("行を補充しない貼り付けの逆命令は値の復元である: {:?}", pair.inverse);
    };
    assert_eq!(
        written,
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        "材料は実際に書いた行（表示の並びの先頭）を指す"
    );
    assert_eq!(
        written
            .iter()
            .map(|row| document_ids.iter().position(|id| id == row).expect("文書にある"))
            .collect::<Vec<_>>(),
        rows.iter().map(|row| row.position).collect::<Vec<_>>(),
        "材料の位置は**文書の位置**である（表示の位置ではない）"
    );
}

/// 並べ替えの下でも同じことが成り立つこと（表示の並びが文書の並びと食い違うもう 1 つの形）。
///
/// 絞り込みと違い、**行は 1 つも隠れない**。それでも表示の並びは文書の並びと食い違うため、
/// 材料を文書の位置で引く実装は同じ誤りを犯す。
#[test]
fn a_paste_round_trip_under_a_sort_restores_the_rows_it_actually_wrote() {
    let mut fixture = Fixture::clean(32, 13);
    let mut apply = fixture.apply();
    let document_ids = ids_of(fixture.document(), fixture.sheet());
    // 数量（列 1）の降順で並べ替える。標本の数量は行ごとに異なる。
    let view = ViewSpec {
        sort: vec![SortKey {
            column: ColumnIndex::new(INT_COLUMN),
            descending: true,
        }],
        filters: Vec::new(),
    };
    let mut order = RowOrder::default();
    let summary = order.recompute(fixture.document(), fixture.sheet(), &view);
    let displayed: Vec<RowId> = (0..summary.visible)
        .map(|ordinal| order.row_at(RowOrdinal::new(ordinal)).expect("可視行"))
        .collect();

    // 前提: 行は隠れていないが、並びが文書の並びと食い違う。
    assert_eq!(0, summary.hidden, "前提: 並べ替えは行を隠さない");
    assert_eq!(document_ids.len(), displayed.len(), "前提: 全行が表示される");
    assert_ne!(
        document_ids, displayed,
        "前提: 並べ替えで表示の並びが文書の並びと食い違う"
    );

    let anchor = displayed[0];
    let before = fixture.snapshot();
    let (_, pair) = apply
        .apply_with_inverse(
            fixture.document_mut(),
            EditCommand::PasteRange {
                anchor: CellAddress::new(anchor, ColumnIndex::new(INT_COLUMN)),
                rows: displayed.clone(),
                text: "q\tr\ns\tt".to_owned(),
            },
        )
        .expect("貼り付けは適用できる");
    let pair = pair.expect("状態を変える適用は対を持つ");

    apply
        .apply_history(fixture.document_mut(), &pair.inverse)
        .expect("逆命令は適用できる");
    assert_eq!(before, fixture.snapshot(), "適用前の状態へ戻る（並べ替えの下でも）");
}

// ---------------------------------------------------------------------------
// 行の幅の往復（要件 9.2）
// ---------------------------------------------------------------------------

/// 値の並びが列数に満たない行の削除の取り消しが、**同じ幅**で戻すこと（要件 9.2）。
///
/// 行の値の並びが列数に満たないことは正当であり（`tests/edit_rows.rs` の
/// `duplicating_a_short_row_copies_exactly_the_values_it_has`）、埋めてもいけない。復元の
/// 材料は行データの wire 形式（全行が同じキー列を要求する）を通るため、復号された行は
/// 列数の幅になる — そのまま差し戻すと、短い行が列数の幅で戻ってしまう。
#[test]
fn a_removed_short_row_is_restored_with_its_original_width() {
    let mut fixture = Fixture::clean(8, 13);
    let mut apply = fixture.apply();
    let short = add_short_row(&mut apply, &mut fixture, "7");
    let before = fixture.snapshot();

    let (_, pair) = apply
        .apply_with_inverse(
            fixture.document_mut(),
            EditCommand::RemoveRows { rows: vec![short] },
        )
        .expect("行の削除は適用できる");
    let pair = pair.expect("状態を変える適用は対を持つ");

    apply
        .apply_history(fixture.document_mut(), &pair.inverse)
        .expect("逆命令は適用できる");
    assert_eq!(
        before,
        fixture.snapshot(),
        "識別子・値・位置（**値の個数も**）が適用前へ戻る"
    );
    assert_eq!(
        2,
        fixture.values_of(short).len(),
        "短い行の幅は元のまま（列数まで埋まらない）"
    );
}

/// 短い行の複製の往復（取り消し → やり直し）が、複製の幅を元の行の幅のまま保つこと
/// （要件 6.3 と 9.2 の交わり）。
///
/// 複製のやり直しは**発行済みの行の差し戻し**である（複製し直すと識別子が変わるため）ため、
/// 差し戻しの経路が幅を戻さないと、やり直した複製だけが列数の幅になって元の行と食い違う。
#[test]
fn a_duplicated_short_row_keeps_its_width_through_undo_and_redo() {
    let mut fixture = Fixture::clean(8, 13);
    let mut apply = fixture.apply();
    let short = add_short_row(&mut apply, &mut fixture, "9");
    let source_width = fixture.values_of(short).len();
    let before = fixture.snapshot();

    let (outcome, pair) = apply
        .apply_with_inverse(
            fixture.document_mut(),
            EditCommand::DuplicateRows { rows: vec![short] },
        )
        .expect("行の複製は適用できる");
    let copy = outcome.affected[0];
    let copied = fixture.snapshot();
    assert_eq!(
        source_width,
        fixture.values_of(copy).len(),
        "前提: 複製の幅は元の行と同じ（埋めない）"
    );

    // 取り消し → やり直し（やり直しは発行済みの行の差し戻しである）。
    apply
        .apply_history(fixture.document_mut(), &pair.as_ref().expect("対がある").inverse)
        .expect("逆命令は適用できる");
    assert_eq!(before, fixture.snapshot(), "適用前の状態へ戻る");
    apply
        .apply_history(fixture.document_mut(), &pair.expect("対がある").redo)
        .expect("やり直しは適用できる");
    assert_eq!(
        copied,
        fixture.snapshot(),
        "やり直しの後の状態は適用の後の状態と同一（**複製の幅も**）"
    );
    assert_eq!(
        source_width,
        fixture.values_of(copy).len(),
        "やり直した複製の幅は元の行の幅のまま"
    );
}

/// 幅 0 の行の削除の取り消しが、**幅 0 のまま**戻すこと（要件 9.2）。
///
/// 値の並びを 1 つも持たない行は正当である（[`Document::add_row`] が作り、値を書かなければ
/// そのまま）。差し戻しの経路は行データの wire 形式（列数ぶんのキーを書く）を通るため、
/// 復号された行の幅は**つねに列数**である — 材料の値の並びを書かなければ、幅 0 の行は
/// 列数の幅で戻ってしまう。
#[test]
fn a_removed_zero_width_row_is_restored_with_zero_width() {
    let mut fixture = Fixture::clean(8, 13);
    let mut apply = fixture.apply();
    let zero = add_zero_width_row(&mut fixture);
    let row_count = fixture.row_count();
    let before = fixture.snapshot();

    let (outcome, pair) = apply
        .apply_with_inverse(fixture.document_mut(), EditCommand::RemoveRows { rows: vec![zero] })
        .expect("行の削除は適用できる");
    assert_eq!(row_count, outcome.row_count, "前提: 1 行減った");

    apply
        .apply_history(fixture.document_mut(), &pair.expect("対がある").inverse)
        .expect("逆命令は適用できる");
    assert_eq!(before, fixture.snapshot(), "適用前の状態へ戻る（幅 0 の行を含む）");
    assert!(
        fixture.values_of(zero).is_empty(),
        "前提つきの表明: 幅 0 の行は幅 0 のまま戻る（列数まで埋まらない。実際の幅 {}）",
        fixture.values_of(zero).len()
    );
}

/// 短い行を**広げた**セルの編集の取り消しが、**元の幅**へ戻すこと（要件 9.2）。
///
/// セル単位の書き込みは行を伸ばすことしかできない（`Row::set_cell` は
/// `resize(column + 1, Null)` であり、決して縮めない）。したがって逆命令がセル単位で書くと、
/// 編集で広がったぶんの幅が残る — 材料は適用前の値の並びそのものだから、行の幅も材料の
/// 幅でなければ復元にならない。
#[test]
fn an_undo_of_an_edit_that_widened_a_short_row_restores_its_original_width() {
    let mut fixture = Fixture::clean(8, 13);
    let mut apply = fixture.apply();
    // 列 1 だけを書いた短い行（幅 2）を作る。
    let short = add_short_row(&mut apply, &mut fixture, "3");
    let width_before = fixture.values_of(short).len();
    let before = fixture.snapshot();

    // 錨から遠い列（列 5）へ書くと、その行の幅は 6 へ**広がる**。
    let (_, pair) = apply
        .apply_with_inverse(
            fixture.document_mut(),
            set_cell(short, 5, "ひろげる"),
        )
        .expect("編集は適用できる");
    let pair = pair.expect("状態を変える適用は対を持つ");
    // 前提: 幅が実際に広がった（本検査の対象そのもの）。
    assert!(
        fixture.values_of(short).len() > width_before,
        "前提: 編集で行の幅が広がった（{width_before} → {}）",
        fixture.values_of(short).len()
    );

    apply
        .apply_history(fixture.document_mut(), &pair.inverse)
        .expect("逆命令は適用できる");
    assert_eq!(before, fixture.snapshot(), "適用前の状態へ戻る");
    assert_eq!(
        width_before,
        fixture.values_of(short).len(),
        "行の幅も適用前へ戻る（広がったぶんが残らない）"
    );
}

/// 短い行を**広げた**貼り付けの取り消しも、**元の幅**へ戻すこと（要件 9.2）。
///
/// 貼り付けは行の幅を広げうる（矩形の列が行の値数より先まで届く場合）。逆命令は
/// [`EditApply::restore_values`] の経路を共有するため、セル単位で書けば同じく幅が残る。
#[test]
fn an_undo_of_a_paste_that_widened_a_short_row_restores_its_original_width() {
    let mut fixture = Fixture::clean(8, 13);
    let mut apply = fixture.apply();
    let short = add_short_row(&mut apply, &mut fixture, "4");
    let width_before = fixture.values_of(short).len();
    let before = fixture.snapshot();
    let displayed = ids_of(fixture.document(), fixture.sheet());

    // 短い行の**幅の外**に届く矩形を貼る（列 3 から 2 列 → 幅 5 まで広がる）。
    let (_, pair) = apply
        .apply_with_inverse(
            fixture.document_mut(),
            EditCommand::PasteRange {
                anchor: CellAddress::new(short, ColumnIndex::new(3)),
                rows: displayed,
                text: "A\tB".to_owned(),
            },
        )
        .expect("貼り付けは適用できる");
    let pair = pair.expect("状態を変える適用は対を持つ");
    assert!(
        fixture.values_of(short).len() > width_before,
        "前提: 貼り付けで行の幅が広がった（{width_before} → {}）",
        fixture.values_of(short).len()
    );

    apply
        .apply_history(fixture.document_mut(), &pair.inverse)
        .expect("逆命令は適用できる");
    assert_eq!(before, fixture.snapshot(), "適用前の状態へ戻る");
    assert_eq!(
        width_before,
        fixture.values_of(short).len(),
        "行の幅も適用前へ戻る（広がったぶんが残らない）"
    );
}

// ---------------------------------------------------------------------------
// 区分（`UndoLabel`）と登録口（要件 9.1）
// ---------------------------------------------------------------------------

/// 5 つの編集命令がそれぞれの区分へ写り、後続スペックのための 2 つの区分が**用意されている**
/// こと（要件 9.1 / 9.7）。
#[test]
fn each_command_maps_to_its_label() {
    let fixture = Fixture::clean(4, 4);
    let row = fixture.row(0);
    let commands = vec![
        (set_cell(row, INT_COLUMN, "3"), UndoLabel::CellEdit),
        (
            EditCommand::SetNested {
                cell: CellAddress::new(row, ColumnIndex::new(NESTED_COLUMN)),
                json: "{\"a\":1}".to_owned(),
            },
            UndoLabel::CellEdit,
        ),
        (
            EditCommand::InsertRows {
                at: RowOrdinal::new(2),
                count: 1,
            },
            UndoLabel::RowInsert,
        ),
        (
            EditCommand::RemoveRows { rows: vec![row] },
            UndoLabel::RowRemove,
        ),
        (
            EditCommand::DuplicateRows { rows: vec![row] },
            UndoLabel::RowDuplicate,
        ),
        (
            EditCommand::PasteRange {
                anchor: CellAddress::new(row, ColumnIndex::new(0)),
                rows: vec![row],
                text: "1".to_owned(),
            },
            UndoLabel::Paste,
        ),
    ];
    for (command, expected) in commands {
        assert_eq!(
            expected,
            UndoLabel::of_edit(&command),
            "命令の区分: {command:?}"
        );
    }

    // 後続スペックのための 2 つの区分は本タスクでは生成されないが、**登録できる**。
    let _ = UndoLabel::Recalculation;
    let _ = UndoLabel::MacroRun;
}

/// 登録口は `push` 1 つであり、**本タスクが生成しない区分の対もそこから積める**（要件 9.1 の
/// 拡張点。後続スペックが履歴の内部構造に触れずに乗れる）。
///
/// # 他に登録の口が無いこと
///
/// `UndoStack` の状態は非公開であり、公開している変更の口は `push`（と、対を戻す `undo` /
/// `redo`）だけである。**他の入口が無いことは型の上で示されている**（非公開のフィールドは
/// クレートの外から触れない）ため、本検査が示すのは「同じ 1 つの口から、別々の生成元の対が
/// 同じ履歴へ積まれ、`depth` がその件数を数える」ことである。
#[test]
fn every_entry_goes_through_one_push() {
    let mut fixture = Fixture::clean(4, 4);
    let row = fixture.row(0);
    let mut stack = UndoStack::new(16);

    // 生成元 1: 編集の適用が返した対。
    let mut apply = fixture.apply();
    let (_, edit_pair) = apply
        .apply_with_inverse(fixture.document_mut(), set_cell(row, INT_COLUMN, "7"))
        .expect("編集は適用できる");
    let edit_entry = entry(UndoLabel::CellEdit, edit_pair);
    stack.push(edit_entry.clone());

    // 生成元 2: 後続スペック（数式の再計算）の区分の対。材料は本検査が組み立てる。
    let foreign = UndoEntry {
        label: UndoLabel::Recalculation,
        inverse: HistoryCommand::RestoreValues {
            sheet: fixture.sheet(),
            rows: Vec::new(),
        },
        redo: HistoryCommand::Composite(Vec::new()),
    };
    stack.push(foreign.clone());
    // 生成元 3: 同じく拡張点から積めること（マクロの実行）。
    let macro_entry = UndoEntry {
        label: UndoLabel::MacroRun,
        ..foreign.clone()
    };
    stack.push(macro_entry.clone());

    assert_eq!(3, stack.depth(), "積んだ件数をそのまま数える");
    // 3 件は同じ 1 つの並びにあり、生成元の順に並ぶ（別々の生成元の対が同じ口から入る）。
    assert_eq!(
        vec![
            UndoLabel::CellEdit,
            UndoLabel::Recalculation,
            UndoLabel::MacroRun
        ],
        stack
            .entries()
            .iter()
            .map(|entry| entry.label)
            .collect::<Vec<_>>(),
        "同じ 1 つの口から、3 つの生成元の対が 1 つの並びへ入る"
    );

    // 3 件は同じ 1 つの後入れ先出しの並びにあり、順に戻る。
    assert_eq!(
        Some(&macro_entry.inverse),
        stack.undo(),
        "最後に積んだ対の逆命令が戻る"
    );
    assert_eq!(
        Some(&foreign.inverse),
        stack.undo(),
        "中央に積んだ対の逆命令が戻る"
    );
    assert_eq!(
        Some(&edit_entry.inverse),
        stack.undo(),
        "最初に積んだ対の逆命令が戻る"
    );
    assert_eq!(None, stack.undo(), "並びの先には何も無い");
    assert_eq!(3, stack.depth(), "取り消しても積んだ件数は変わらない");
}

// ---------------------------------------------------------------------------
// 文書単位（要件 9.5）と決定性
// ---------------------------------------------------------------------------

/// 履歴は**ドキュメント単位**であり、シートごとではない（要件 9.5）。1 つの履歴が 2 つの
/// シートの操作を同時に保持し、取り消しが全体で 1 つの後入れ先出しであることを示す。
///
/// 後続スペック（マクロの実行）は複数のシートを跨ぐ 1 つの操作を積むため、履歴がシートで
/// 分かれているとその操作を 1 つの対として表せない。
#[test]
fn the_history_holds_operations_of_several_sheets_as_one_document() {
    let mut fixture = Fixture::clean(8, 4);
    let (second, second_plan) = fixture.add_second_sheet("別のシート");
    let mut stack = UndoStack::new(16);

    let first_before = fixture.snapshot();
    let second_before = snapshot(fixture.document(), second);

    // 1 つ目と 2 つ目のシートの編集を、それぞれの適用の経路で行い、**同じ履歴**へ積む。
    let mut first_apply = fixture.apply();
    let first_row = fixture.row(1);
    let (_, first_pair) = first_apply
        .apply_with_inverse(
            fixture.document_mut(),
            set_cell(first_row, INT_COLUMN, "11"),
        )
        .expect("1 つ目のシートの編集は適用できる");
    stack.push(entry(UndoLabel::CellEdit, first_pair));

    let mut second_apply = EditApply::new(second, second_plan);
    let second_row = fixture
        .document()
        .sheet_by_id(second)
        .expect("シートは文書にある")
        .rows()[2]
        .id();
    let (_, second_pair) = second_apply
        .apply_with_inverse(
            fixture.document_mut(),
            set_cell(second_row, INT_COLUMN, "22"),
        )
        .expect("2 つ目のシートの編集は適用できる");
    stack.push(entry(UndoLabel::CellEdit, second_pair));

    assert_eq!(2, stack.depth(), "同じ履歴が 2 つのシートの操作を持つ");
    assert_ne!(
        first_before,
        fixture.snapshot(),
        "1 つ目のシートは変わっている"
    );
    assert_ne!(
        second_before,
        snapshot(fixture.document(), second),
        "2 つ目のシートも変わっている"
    );

    // 取り消しは**全体で 1 つの後入れ先出し**である（シートごとの履歴に分かれていない）。
    let second_inverse = stack.undo().expect("2 件目がある").clone();
    second_apply
        .apply_history(fixture.document_mut(), &second_inverse)
        .expect("2 つ目のシートの逆命令は適用できる");
    assert_eq!(
        second_before,
        snapshot(fixture.document(), second),
        "2 つ目のシートが元へ戻る"
    );
    assert_ne!(
        first_before,
        fixture.snapshot(),
        "1 つ目のシートはまだ戻っていない（後入れ先出し）"
    );

    let first_inverse = stack.undo().expect("1 件目がある").clone();
    first_apply
        .apply_history(fixture.document_mut(), &first_inverse)
        .expect("1 つ目のシートの逆命令は適用できる");
    assert_eq!(first_before, fixture.snapshot(), "1 つ目のシートも元へ戻る");
    assert_eq!(2, stack.depth(), "積んだ件数は変わらない（位置だけが動く）");
}

/// 取り消しの後に積むと、**やり直しの対象は破棄される**（design.md の Invariants）。
///
/// これは積んだ対が指す位置を一意にするために要る（そうしないと、取り消し済みの対が
/// もう一度取り消されて、同じ操作が 2 回適用される）。上限による追い出しと、取り消し・
/// やり直しの**結果**（影響を受けた行の報告）は 4.2 が担う。
#[test]
fn a_push_after_undo_discards_the_redo_tail() {
    let fixture = Fixture::clean(4, 4);
    let pair = HistoryPair {
        inverse: HistoryCommand::Composite(Vec::new()),
        redo: HistoryCommand::Composite(Vec::new()),
    };
    let mut stack = UndoStack::new(8);
    stack.push(UndoEntry {
        label: UndoLabel::CellEdit,
        inverse: pair.inverse.clone(),
        redo: pair.redo.clone(),
    });
    assert_eq!(1, stack.depth());

    stack.undo().expect("取り消せる");
    // 取り消した後のやり直しは、積んだ対のやり直しの命令を返す。
    let redone = stack.redo().expect("やり直せる").clone();
    assert_eq!(pair.redo, redone);
    assert_eq!(None, stack.redo(), "やり直しの先には何も無い");

    // 取り消した状態で新しい操作を積むと、やり直しの対象（取り消した対）が破棄される。
    stack.undo().expect("もう一度取り消せる");
    stack.push(UndoEntry {
        label: UndoLabel::RowRemove,
        inverse: pair.inverse.clone(),
        redo: pair.redo.clone(),
    });
    assert_eq!(
        1,
        stack.depth(),
        "取り消した操作は破棄され、新しい操作だけが残る"
    );
    assert_eq!(None, stack.redo(), "破棄された対はやり直せない");
    let _ = fixture;
}

/// 同じ文書と同じ命令の並びは、同じ**形**の履歴を与える（識別子は実行ごとに変わるため、
/// 比較するのは対の種類・区分・規模・位置である）。
#[test]
fn the_shape_of_the_history_is_deterministic() {
    /// 標本を 1 つ作り、同じ並びの命令を適用して、対の**形**を文字列で返す。
    fn shape() -> Vec<(UndoLabel, String, String)> {
        let mut fixture = Fixture::clean(8, 4);
        let mut apply = fixture.apply();
        // 行の構造を変える命令が続くため、**表示の並びはその時点の文書から取る**
        // （貼り付けの `rows` は適用の直前に存在する行でなければならない）。
        let first = vec![
            set_cell(fixture.row(1), INT_COLUMN, "5"),
            EditCommand::InsertRows {
                at: RowOrdinal::new(2),
                count: 2,
            },
            EditCommand::DuplicateRows {
                rows: vec![fixture.row(0)],
            },
            EditCommand::RemoveRows {
                rows: vec![fixture.row(3), fixture.row(6)],
            },
        ];
        let mut stack = UndoStack::new(16);
        for command in first {
            let label = UndoLabel::of_edit(&command);
            let (_, pair) = apply
                .apply_with_inverse(fixture.document_mut(), command)
                .expect("適用は成功する");
            stack.push(entry(label, pair));
        }
        let displayed = ids_of(fixture.document(), fixture.sheet());
        let command = EditCommand::PasteRange {
            anchor: CellAddress::new(fixture.row(1), ColumnIndex::new(1)),
            rows: displayed,
            text: "p\tq\nr\ts".to_owned(),
        };
        let label = UndoLabel::of_edit(&command);
        let (_, pair) = apply
            .apply_with_inverse(fixture.document_mut(), command)
            .expect("適用は成功する");
        stack.push(entry(label, pair));
        // 対の**形**: 種別の並びと、材料の規模（行の件数・位置）だけを写す（値も識別子も写さない）。
        stack
            .entries()
            .iter()
            .map(|entry| {
                (
                    entry.label,
                    describe(&entry.inverse),
                    describe(&entry.redo),
                )
            })
            .collect()
    }

    /// 復元命令の**形**（種別と、位置と件数）。
    fn describe(command: &HistoryCommand) -> String {
        match command {
            HistoryCommand::Edit(EditCommand::SetCells { cells }) => format!("set:{}", cells.len()),
            HistoryCommand::Edit(EditCommand::RemoveRows { rows }) => format!("remove:{}", rows.len()),
            // 貼り付けのやり直しは、覆う行の件数と矩形の大きさだけを写す（**識別子は写さない**。
            // 実行を跨ぐと変わるため、形の比較に含められない）。
            HistoryCommand::Edit(EditCommand::PasteRange { rows, text, .. }) => {
                format!("paste:{}x{}", rows.len(), text.lines().count())
            }
            HistoryCommand::Edit(EditCommand::InsertRows { at, count }) => {
                format!("insert:{}:{}", at.get(), count)
            }
            HistoryCommand::Edit(EditCommand::SetNested { .. }) => "nested".to_owned(),
            HistoryCommand::Edit(EditCommand::DuplicateRows { rows }) => {
                format!("duplicate:{}", rows.len())
            }
            HistoryCommand::RestoreValues { rows, .. } => format!(
                "values:{}",
                rows.iter()
                    .map(|row| format!("{}", row.position))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            HistoryCommand::RestoreRows { rows, .. } => format!(
                "rows:{}",
                rows.iter()
                    .map(|row| format!("{}x{}", row.position, row.values.len()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            HistoryCommand::Composite(parts) => format!(
                "composite:[{}]",
                parts.iter().map(describe).collect::<Vec<_>>().join(" ")
            ),
        }
    }

    assert_eq!(shape(), shape(), "同じ命令の並びは同じ形の履歴を与える");
}

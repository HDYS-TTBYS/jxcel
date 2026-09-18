//! 変更集合の適用と 1 回の取り消し（tasks.md 4.2。要件 5.1, 5.2, 5.3, 5.4, 6.3, 7.1, 7.2,
//! 7.3, 7.4）。
//!
//! # 層の位置（この層が文書へ触ってよい唯一の理由）
//!
//! エンジン（`crates/macro-runtime`）はマクロの書き込みを**未適用の変更集合**
//! （[`ChangeSet`]）として実行の間だけ持ち、**文書へは触らない**（design.md 決定 2 / 3。
//! `host/changes.rs` の module docs）。それを文書へ適用するのはアダプタである本モジュール
//! であり、**`document-session` の `edit` の閉包 1 回**の中で、実行が成功で終わったときだけ
//! 適用する（design.md「System Flows → 実行の流れ」の差分適用の規則）。
//!
//! **失敗・打ち切りでは何も適用しない**（要件 6.3 / 7.3）は、呼び出し側（4.3 の `macro_run`）
//! が `RunOutcome::Ran` のときだけ本モジュールを呼ぶことで満たされる。本モジュールは
//! 「成功した実行の変更集合」だけを受け取り、その適用に失敗した場合は**1 つも書かずに**
//! 理由を返す（要件 5.4）。
//!
//! # この層が守る 4 つの規律
//!
//! 1. **適用は 1 回の `edit` の閉包の中で行う。** 1 つの変更が生んだ命令の並びは、閉包の
//!    内側で**1 つの履歴の対**へまとめてから積む（要件 7.1）。命令ごとに積めば取り消しが
//!    1 命令ずつになり、「実行まるごとを 1 回で戻せる」が満たされない
//! 2. **適用の前にシートを照合し、1 つでも食い違えば何も書かずに拒む。** これは `data-grid`
//!    の `HistoryCommand::Composite` が既に持つ規律（`EditApply::apply_parts` →
//!    `ensure_parts_share_target`）と同じであり、理由も同じである: 部分を順に適用すると
//!    **前半だけが文書へ届く**（巻き戻す口が無い）
//! 3. **履歴へは `UndoLabel::MacroRun` の 1 対だけを積む。** 逆命令は**適用した命令の並び
//!    から**組む（逆順に並べる。理由は [`assemble`] の docs）
//! 4. **順序は `host/changes.rs` の規則に従う**（追加 → 値 → 削除）。そこに無い規則を発明
//!    しない — 同じセルへの複数の書き込みは既に「最後の書き込み」へ解決済みであり
//!    （`ChangeSet::cell_writes`）、同じ行の削除の繰り返しは既に 1 回へ畳んである
//!    （`ChangeSet::removed_rows`）
//!
//! # 変更集合を複製しない（要件 11.3）
//!
//! 変更集合は実行 1 回の縫い目（[`HostPort`]）の内側のロックの下にあり、**外へは出せない**
//! （`macro_runtime::host` の module docs「設計から動かした 2 点」）。したがって本モジュールは
//! [`HostPort::with_changes`] を**読むためだけ**に呼び、その借用（`&ChangeSet`）のまま写像を
//! 走査する。**10 万セルの写像を丸ごと複製する経路は無い**（複製されるのは、文書へ渡す値と
//! 履歴の材料という、**そもそも文書と履歴が所有しなければならない**分だけである）。
//!
//! # 設計から動かした点（理由つき）
//!
//! 1. **署名。** design.md「MacroHost / macro_apply」の Service Interface は
//!    `apply_macro_changes(window: &WindowLabel, changes: &ChangeSet)` と書くが、`ChangeSet`
//!    は縫い目の内側にあり**アダプタの関数へ渡せない**（上の理由）。したがって
//!    [`apply_macro_changes`] は縫い目（`&dyn HostPort`）と、文書・履歴・適用先のシートを
//!    受け取る形にした。設計の意図（1 回の `edit` で全部適用し、`MacroRun` の 1 対を積む）は
//!    変えていない
//! 2. **値の書き込みに `EditCommand::SetCells` を通さない。** `EditCommand::SetCells` が運ぶ
//!    のは**打たれた文字**（`String`）であり、マクロの値は**型付きの値**
//!    （`document_format::CellValue`）である。文字列へ写して判定へ通すと値の変種が変わり
//!    （整数の列へ `Text` を書く等）、要件 5.2 の「適合しない値を破棄せず保持する」が
//!    「適合する値へ変換されてしまう」に化ける。したがって値は**上流の一括のセル書き込み**
//!    （[`Document::set_cells`]。画面の編集が判定の後に呼ぶのと同じ口であり、同じセルへ
//!    複数回書いたときの「後ろが残る」規則と、行の幅を越えて書いたときの値なしの埋め方を
//!    実装している唯一の場所である）で書き、**履歴の命令**として
//!    `HistoryCommand::RestoreValues` を持つ
//! 3. **適用先は 1 シートに限る。** 履歴はドキュメント単位であり（`data-grid` 要件 9.5）、
//!    その 1 歩は `EditApply`（1 つのシートの計画を固定して持つ）が適用する。`Composite` の
//!    照合は適用先のシートとの一致を要求するため（`ensure_parts_share_target`）、複数シートに
//!    跨る変更集合は**取り消せない**。取り消せない 1 歩を積むより、適用の前に拒むほうが
//!    回復可能である（規律 2 と同じ判断）。変更集合が名乗るシートが 1 つでも適用先と
//!    食い違えば [`MacroApplyError::SheetMismatch`] を返し、**1 つも書かない**
//! 4. **再検証を本経路では呼ばない。** 構造を変える命令（追加・削除・複製）は `data-grid` の
//!    `EditCommand` を通るため再検証が伴うが、値の書き込みは上の理由で `Document::set_cells`
//!    を通る。**違反の索引を持つのはセッション（`data_grid::GridSession`）であり、本モジュール
//!    ではない**（本モジュールが返すのは件数の要約だけである。design.md の Service Interface）。
//!    実行の後で違反を提示し直すのは呼び出し側（4.3）が表示を作り直す経路の仕事である
//!
//! # 版と履歴（検査を `edit` の前に置く理由）
//!
//! [`apply_macro_changes`] は**読みのロックの下で先に全部検査し**、適用の可否を決めてから
//! `edit` を呼ぶ。`document-session` の `edit` は**閉包が失敗を返しても未保存の印を立て、
//! 版を 1 進める**（`change.rs` の module docs「閉包が失敗を返しても印を立てる」: 閉包が文書を
//! 変えたかを判定できないため保守側に倒す規律）。したがって「適用の失敗ではドキュメントが
//! 変わらない（版も履歴も動かない）」を満たすには、**拒否を `edit` の外で決める**必要がある。
//! 適用の閉包の内側でも同じ検査をやり直す（下の [`apply_all`]）— 読みのロックの下の判定は
//! 適用時点の保証ではないためである（実行中も利用者は表を編集できる。要件 2.2）。

use std::collections::HashMap;
use std::fmt;

use app_shell::ipc::WindowLabel;
use data_grid::{
    EditApply, EditCommand, GridError, HistoryCommand, RestoredRow, RowAnchor, RowOrder, RowTarget,
    UndoEntry, UndoLabel, UndoStack, ViewSpec,
};
use document_format::{CellValue, Document, Row, RowId, SheetId};
use document_session::{DocumentSessions, DocumentSessionsApi, SessionError};
use macro_runtime::host::HostPort;
use macro_runtime::{ChangeSet, ChangeSummary, StagedChange};
use schema_engine::{ColumnIndex, CompiledSchema};

/// 変更集合を適用できなかった理由。
///
/// **表示の文言ではなく、判別可能な理由**である（`data-grid` の [`GridError`] /
/// `document-format` の `CellWriteError` と同じ規律。提示の文言を組み立てるのは境界の側で
/// ある）。要件 5.4 の「理由を提示する」は、この型を提示の層へ運ぶことで満たされる。
#[derive(Debug)]
pub enum MacroApplyError {
    /// 適用先のシートが文書に無い、または列が 1 本も宣言されていない／計画の列数と食い違う。
    SheetUnusable {
        /// 適用先として名指されたシート。
        sheet: SheetId,
    },
    /// 変更集合が名乗るシートが適用先と食い違う（**何も書かない**。module docs の判断 3）。
    SheetMismatch {
        /// 適用先（この 1 回の `edit` が書ける唯一のシート）。
        expected: SheetId,
        /// 変更集合が名乗ったシート。
        found: SheetId,
    },
    /// 存在しない行を指した（要件 5.4。**何も書かない**）。
    UnknownRow {
        /// 対象のシート。
        sheet: SheetId,
        /// 存在しなかった行。
        row: RowId,
    },
    /// 存在しない列を指した（要件 5.4。**何も書かない**）。
    UnknownColumn {
        /// 対象のシート。
        sheet: SheetId,
        /// 範囲外だった列。
        column: ColumnIndex,
        /// そのシートが持つ列の数。
        columns: usize,
    },
    /// 追加する行の値が列の数より広い（要件 5.4。**何も書かない**）。
    RowTooWide {
        /// 対象のシート。
        sheet: SheetId,
        /// 渡された行の値の個数。
        values: usize,
        /// そのシートが持つ列の数。
        columns: usize,
    },
    /// 適用の直前の検査を通った後に、上流（`data-grid` / `document-format`）が拒んだ。
    ///
    /// ここへ届くのは、**検査と適用の間**に実行中の利用者の編集で前提が変わった場合だけである
    /// （要件 2.2 により実行中も編集できる）。理由は上流の文言をそのまま運ぶ。
    Rejected {
        /// 対象のシート。
        sheet: SheetId,
        /// 上流が返した理由。
        reason: String,
    },
    /// 写像の前提が崩れた（到達しない見込み。`data-grid` の結果が本モジュールの読みと食い違う）。
    Inconsistent {
        /// 食い違った内容。
        reason: String,
    },
    /// 変更集合を読めなかった（[`HostPort::with_changes`] が 1 度も読ませなかった）。
    Unread,
    /// セッションの側で適用できなかった（文書を保持していない等）。
    Session(SessionError),
}

// `Display` は**持ち物を機械的に写すだけ**である（`GridError` /
// `CellWriteError` と同じ規律）。提示の文言は呼び出し元が組み立てる。
impl fmt::Display for MacroApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SheetUnusable { sheet } => write!(f, "schema of sheet {sheet} is unusable"),
            Self::SheetMismatch { expected, found } => write!(
                f,
                "the change names sheet {found}, but this apply writes sheet {expected}"
            ),
            Self::UnknownRow { sheet, row } => write!(f, "no row {row} in sheet {sheet}"),
            Self::UnknownColumn {
                sheet,
                column,
                columns,
            } => write!(
                f,
                "column {} is out of range: sheet {sheet} has {columns} columns",
                column.index()
            ),
            Self::RowTooWide {
                sheet,
                values,
                columns,
            } => write!(
                f,
                "a row of {values} values is wider than the {columns} columns of sheet {sheet}"
            ),
            Self::Rejected { sheet, reason } => {
                write!(f, "sheet {sheet} rejected the change: {reason}")
            }
            Self::Inconsistent { reason } => {
                write!(f, "the mapping disagrees with the change set: {reason}")
            }
            Self::Unread => write!(f, "the change set was not readable"),
            Self::Session(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for MacroApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Session(error) => Some(error),
            _ => None,
        }
    }
}

/// 実行 1 回の変更を、そのウィンドウの文書へ**1 回の `edit`** で適用し、履歴へ 1 対だけ積む。
///
/// `sheet` は**適用先のシート**（ウィンドウが表示しているシート）、`schema` はそのシートから
/// 落とした計画（`SheetEntry` が持っているものをそのまま渡す）、`port` は実行 1 回の縫い目
/// （変更集合はその内側にある）、`history` はウィンドウの保持が所有する履歴である
/// （要件 9.5。借りて 1 件積むだけである）。
///
/// 戻るのは**実行が加えた変更の件数**（要件 5.5。design.md の `ChangeSummary`）。変更が
/// 1 件も無ければ `edit` を呼ばない（版も履歴も動かさない）。
///
/// 拒否はすべて**文書へ 1 つも触れる前に**決まる（module docs「版と履歴」）。適用の途中で
/// 上流が拒んだ場合（[`MacroApplyError::Rejected`]）は、上流の各命令が「失敗したら 1 つも
/// 書かない」契約を持つためそこまでに適用した部分だけが残る — 検査を 2 度行うのはこの幅を
/// 狭めるためである。
pub fn apply_macro_changes(
    documents: &DocumentSessions,
    window: &WindowLabel,
    sheet: SheetId,
    schema: &CompiledSchema,
    port: &dyn HostPort,
    history: &mut UndoStack,
) -> Result<ChangeSummary, MacroApplyError> {
    // 1. 読みのロックの下で検査し、適用の可否を決める（何も変えない）。
    let summary = documents
        .read(window, &mut |document| {
            let mut verdict = None;
            port.with_changes(&mut |changes| {
                verdict = Some(inspect(document, sheet, schema, changes));
            });
            verdict.unwrap_or(Err(MacroApplyError::Unread))
        })
        .map_err(MacroApplyError::Session)??;

    // 2. 何も変えない実行は `edit` を呼ばない（版も履歴も動かさない）。
    if summary.is_empty() {
        return Ok(summary);
    }

    // 3. **閉包 1 回**で全部適用する。変更集合はここでも複製せずに読む。
    let edited = documents
        .edit(window, &mut |document| {
            let mut result = None;
            port.with_changes(&mut |changes| {
                result = Some(apply_all(document, history, sheet, schema, changes));
            });
            result.unwrap_or(Err(MacroApplyError::Unread))
        })
        .map_err(MacroApplyError::Session)?;
    edited.value
}

/// 変更集合を検査し、適用できるなら件数の要約を返す（**読みだけ**。何も書かない）。
///
/// 検査は 1 パスである（`application_order` の並びに従う）。見るのは 4 つ:
///
/// 1. 適用先のシートが使えるか（文書にあり、列を持ち、計画の列数と一致する）
/// 2. 変更集合が名乗るシートが**すべて**適用先と一致するか（規律 2）
/// 3. 指された行がすべて存在するか（要件 5.4）
/// 4. 指された列と、追加する行の値の幅が列の中に収まるか（要件 5.4）
///
/// 行の索引は 1 度だけ作る（行数に対する 1 パス。10 万行でも行ごとに走査しない）。
fn inspect(
    document: &Document,
    sheet: SheetId,
    schema: &CompiledSchema,
    changes: &ChangeSet,
) -> Result<ChangeSummary, MacroApplyError> {
    let columns = schema.column_count();
    let target = document
        .sheet_by_id(sheet)
        .ok_or(MacroApplyError::SheetUnusable { sheet })?;
    // 列 0 本の計画は書き込む先を持たない（`data-grid` の `EditApply::usable_columns` と
    // 同じ前提）。列数が食い違う計画は「同じシートから落とした」という事前条件が破れている。
    if columns == 0 || target.columns().len() != columns {
        return Err(MacroApplyError::SheetUnusable { sheet });
    }
    let index: HashMap<RowId, usize> = target
        .rows()
        .iter()
        .enumerate()
        .map(|(at, row)| (row.id(), at))
        .collect();

    for staged in changes.application_order() {
        match staged {
            StagedChange::Insert(insert) => {
                require_sheet(sheet, insert.sheet)?;
                for values in &insert.values {
                    if values.len() > columns {
                        return Err(MacroApplyError::RowTooWide {
                            sheet,
                            values: values.len(),
                            columns,
                        });
                    }
                }
            }
            StagedChange::Duplicate(duplicate) => {
                require_sheet(sheet, duplicate.sheet)?;
                for row in &duplicate.rows {
                    require_row(&index, sheet, *row)?;
                }
            }
            StagedChange::SetCells {
                sheet: named,
                row,
                column,
                ..
            } => {
                require_sheet(sheet, named)?;
                require_row(&index, sheet, row)?;
                if column.index() >= columns {
                    return Err(MacroApplyError::UnknownColumn {
                        sheet,
                        column,
                        columns,
                    });
                }
            }
            StagedChange::Remove(remove) => {
                require_sheet(sheet, remove.sheet)?;
                for row in &remove.rows {
                    require_row(&index, sheet, *row)?;
                }
            }
        }
    }

    // 件数は変更集合の数える口からそのまま写す（`host/changes.rs` の規則: 挿入・複製・削除は
    // 呼び出し 1 回が 1 件、セルは解決済みのセルごとに 1 件。削除は行の集合の大きさである）。
    Ok(ChangeSummary {
        set_cells: changes.cell_count(),
        inserted_rows: changes.inserted_row_count(),
        removed_rows: changes.removed_row_count(),
        duplicated_rows: changes.duplicated_row_count(),
    })
}

/// 適用の閉包の本体（`edit` の内側で呼ばれる）。
///
/// **まず検査をやり直す**（読みのロックの下の判定は適用時点の保証ではない。要件 2.2）。
/// そのあと `host/changes.rs` の順序（追加 → 値 → 削除）のまま 1 種類ずつ適用し、適用した
/// 命令の並びから履歴の 1 対を組んで積む。
fn apply_all(
    document: &mut Document,
    history: &mut UndoStack,
    sheet: SheetId,
    schema: &CompiledSchema,
    changes: &ChangeSet,
) -> Result<ChangeSummary, MacroApplyError> {
    let summary = inspect(document, sheet, schema, changes)?;
    if summary.is_empty() {
        return Ok(summary);
    }

    // 適用の経路。計画は呼び出し側が持っているものを複製して渡す（`data-grid` の型が所有権を
    // 要求する。計画は列の宣言 1 つ分であり、変更の規模に比例しない）。
    let mut apply = EditApply::new(sheet, schema.clone());
    // **可視の序数は 1 つも使わない**（対象は識別子、挿入は文書の末尾）ため、順序は適用の
    // 直前の文書の並びを 1 度だけ導出して渡す（本経路は順序を読まない）。
    let mut order = RowOrder::default();
    order.recompute(document, sheet, &ViewSpec::default());

    let mut steps: Vec<Step> = Vec::new();

    // 1. 追加（呼び出しの順に文書の末尾へ足す）。
    for insert in changes.inserts() {
        steps.push(insert_rows(document, &mut apply, &order, sheet, insert)?);
    }
    // 2. 複製（呼び出しの順。同じ行の繰り返しは繰り返しの数だけ増える）。
    for duplicate in changes.duplicates() {
        steps.push(duplicate_rows(
            document,
            &mut apply,
            &order,
            sheet,
            &duplicate.rows,
        )?);
    }
    // 3. 値（同じセルは既に最後の書き込みへ解決されており、並びは (シート, 行, 列) の昇順）。
    if changes.cell_count() > 0 {
        steps.push(write_cells(document, sheet, changes)?);
    }
    // 4. 削除（**畳んだ行の集合**を 1 回で。呼び出しをまたぐ同じ行の繰り返しは 1 回の削除に
    //    畳まれる — `host/changes.rs` の規則である。呼び出しごとに送ると 2 度目が既に無い行を
    //    指す）。
    let removals: Vec<RowId> = changes
        .removed_rows()
        .iter()
        .filter(|(named, _)| *named == sheet)
        .map(|(_, row)| *row)
        .collect();
    if !removals.is_empty() {
        steps.push(remove_rows(document, &mut apply, &order, sheet, removals)?);
    }

    // 逆命令は**適用と逆順**、やり直しは**適用と同じ順**に並べる（[`assemble`]）。
    let (inverse, redo) = assemble(steps);
    if inverse.is_empty() {
        // 状態を変えなかった（`data-grid` はそのような適用に対を持たせない）。履歴へは何も
        // 積まない（`UndoStack` の登録口の規律。要件 9.1）。
        return Ok(summary);
    }
    history.push(UndoEntry {
        label: UndoLabel::MacroRun,
        inverse: collapse(inverse),
        redo: collapse(redo),
    });
    Ok(summary)
}

/// 変更 1 つが適用した命令の並びから組んだ、**逆命令とやり直しの命令**。
///
/// どちらも適用の**時点**でしか作れない（適用の後には変更前の値も、取り除かれた行も、その
/// 位置も存在しない。`data-grid` の `edit` 層の module docs と同じ理由）。
struct Step {
    /// 逆命令（適用した順に並ぶ。全体は [`assemble`] が逆順にする）。
    inverse: Vec<HistoryCommand>,
    /// やり直しの命令（適用した順）。
    redo: Vec<HistoryCommand>,
}

/// 適用した命令の並びから、履歴へ積む 1 対を組む。
///
/// **逆命令は適用と逆順に並べる。** `c1 → c2` と適用した状態を戻すには `c2` の逆命令を先に
/// 適用する必要がある（先に `c1` の逆命令を適用すると、`c2` が書いた行・値が既に無い前提で
/// 適用される）。やり直しは同じ理由で適用と同じ順である。変更の中の並び（同じ変更が複数の
/// 命令を生む場合。複製のラウンド）も同じ規則で扱う。
fn assemble(steps: Vec<Step>) -> (Vec<HistoryCommand>, Vec<HistoryCommand>) {
    let mut inverse: Vec<HistoryCommand> = Vec::new();
    let mut redo: Vec<HistoryCommand> = Vec::new();
    for step in steps {
        inverse.extend(step.inverse);
        redo.extend(step.redo);
    }
    inverse.reverse();
    (inverse, redo)
}

/// 複数の命令を 1 つの履歴の命令へまとめる（1 つなら包まない）。
fn collapse(mut commands: Vec<HistoryCommand>) -> HistoryCommand {
    if commands.len() == 1 {
        commands.pop().expect("1 件ある")
    } else {
        HistoryCommand::Composite(commands)
    }
}

/// 変更集合が名乗るシートを適用先と照合する（規律 2）。
fn require_sheet(expected: SheetId, found: SheetId) -> Result<(), MacroApplyError> {
    if expected == found {
        Ok(())
    } else {
        Err(MacroApplyError::SheetMismatch { expected, found })
    }
}

/// 行がそのシートにあることを確かめる（要件 5.4）。
fn require_row(
    index: &HashMap<RowId, usize>,
    sheet: SheetId,
    row: RowId,
) -> Result<(), MacroApplyError> {
    if index.contains_key(&row) {
        Ok(())
    } else {
        Err(MacroApplyError::UnknownRow { sheet, row })
    }
}

/// 行の識別子から文書の位置（0 起点）への索引を作る（行数に対する 1 パス）。
fn positions(
    document: &Document,
    sheet: SheetId,
) -> Result<HashMap<RowId, usize>, MacroApplyError> {
    let found = document
        .sheet_by_id(sheet)
        .ok_or(MacroApplyError::SheetUnusable { sheet })?;
    Ok(found
        .rows()
        .iter()
        .enumerate()
        .map(|(at, row)| (row.id(), at))
        .collect())
}

/// そのシートの行の並びを引く（適用の前に検査済みである）。
fn rows_of(document: &Document, sheet: SheetId) -> Result<&[Row], MacroApplyError> {
    Ok(document
        .sheet_by_id(sheet)
        .ok_or(MacroApplyError::SheetUnusable { sheet })?
        .rows())
}

/// 上流の拒否を、適用先のシートつきの理由へ写す。
fn rejected(sheet: SheetId) -> impl Fn(GridError) -> MacroApplyError {
    move |error| MacroApplyError::Rejected {
        sheet,
        reason: error.to_string(),
    }
}

/// 行を追加する（`Change::InsertRows` の写し）。
///
/// 順方向は `data-grid` の `InsertRows`（文書の末尾へ `count` 行。値は宣言の既定値であり、
/// 行の識別子は文書が発行する）で行い、そのあと**マクロが渡した値を重ねる**
/// （[`Document::set_cells`]。上流の一括のセル書き込みであり、行の値の並びを丸ごと置換する
/// `RestoreValues` と違って残りの列の既定値を消さない）。
///
/// 逆命令は**足した行を取り除く**ことであり、やり直しは**その行を同じ識別子・同じ位置・
/// 同じ値で差し戻す**ことである（`RestoreRows`）。やり直しに `InsertRows` を使えないのは、
/// 差し戻す行の識別子がやり直しの時点で変わってしまい、その行へ書く値の宛先が失われるためで
/// ある（マクロの値は識別子で指す）。
fn insert_rows(
    document: &mut Document,
    apply: &mut EditApply,
    order: &RowOrder,
    sheet: SheetId,
    insert: &macro_runtime::RowInsert,
) -> Result<Step, MacroApplyError> {
    let count = insert.values.len();
    apply
        .apply_with_inverse(
            document,
            EditCommand::InsertRows {
                at: RowAnchor::End,
                count,
            },
            order,
        )
        .map_err(rejected(sheet))?;

    // 足した行は文書の末尾にある（`RowAnchor::End` は挿入前の行数を位置とする）。文書そのものを
    // 真とする（`EditOutcome::affected` の契約に依らず、行の識別子も値もここから読む）。
    let (start, cells): (usize, Vec<(RowId, usize, CellValue)>) = {
        let rows = rows_of(document, sheet)?;
        let start = rows
            .len()
            .checked_sub(count)
            .ok_or_else(|| MacroApplyError::Inconsistent {
                reason: format!("InsertRows が {count} 行を足していない"),
            })?;
        let mut cells = Vec::new();
        // マクロが渡した値の並びの順が、足した行の順である。
        for (row, values) in rows[start..].iter().zip(insert.values.iter()) {
            for (column, value) in values.iter().enumerate() {
                cells.push((row.id(), column, value.clone()));
            }
        }
        (start, cells)
    };
    document
        .set_cells(sheet, &cells)
        .map_err(|error| MacroApplyError::Rejected {
            sheet,
            reason: error.to_string(),
        })?;

    // 書いたあとの行がそのままやり直しの材料である（識別子・位置・値。位置は行の集合を
    // 変えないため `set_cells` の前後で同じ）。
    let rows = rows_of(document, sheet)?;
    let material: Vec<RestoredRow> = rows[start..]
        .iter()
        .enumerate()
        .map(|(offset, row)| RestoredRow {
            id: row.id(),
            position: start + offset,
            values: row.values().to_vec(),
        })
        .collect();
    let ids: Vec<RowId> = material.iter().map(|row| row.id).collect();
    Ok(Step {
        inverse: vec![HistoryCommand::Edit(EditCommand::RemoveRows {
            target: RowTarget::Ids(ids),
        })],
        redo: vec![HistoryCommand::RestoreRows {
            sheet,
            rows: material,
        }],
    })
}

/// 行を複製する（`Change::DuplicateRows` の写し）。
///
/// `data-grid` の `DuplicateRows` は**1 回の命令の中の同じ行を 1 回へ畳む**。マクロの
/// `duplicateRows` は同じ行を 2 度渡せば 2 行増える（`host/changes.rs` の `RowDuplicate` の
/// 規則であり、本モジュールが発明した規則ではない）ため、**出現回数の最大値**ぶんのラウンドへ
/// 分けて送る（各ラウンドは「まだ残っている行」を 1 つずつ含む）。識別子がすべて異なる普通の
/// 場合、ラウンドは 1 つである。
///
/// 逆命令とやり直しは `data-grid` が組んだ対をそのまま使う（逆命令は増えた行を取り除き、
/// やり直しは元の行をもう一度複製する。増えた行の識別子はやり直しの時点で新しく発行される
/// ため、対を自前で組むと識別子が失われる）。
fn duplicate_rows(
    document: &mut Document,
    apply: &mut EditApply,
    order: &RowOrder,
    sheet: SheetId,
    rows: &[RowId],
) -> Result<Step, MacroApplyError> {
    let mut inverse = Vec::new();
    let mut redo = Vec::new();
    for round in rounds_of(rows) {
        let (_, pair) = apply
            .apply_with_inverse(
                document,
                EditCommand::DuplicateRows {
                    target: RowTarget::Ids(round),
                },
                order,
            )
            .map_err(rejected(sheet))?;
        // 状態を変えなかった適用は対を持たない（`data-grid` の規律。ここでは空のラウンドが
        // それに当たる）。
        if let Some(pair) = pair {
            inverse.push(pair.inverse);
            redo.push(pair.redo);
        }
    }
    Ok(Step { inverse, redo })
}

/// 複製の対象を、出現回数のぶんのラウンドへ分ける（[`duplicate_rows`] の docs）。
///
/// 初出の順を保つ（同じ集合の要求が引数の並びに依らず同じ結果になる、`data-grid` の既存の
/// 規律に揃える）。
fn rounds_of(rows: &[RowId]) -> Vec<Vec<RowId>> {
    let mut counts: Vec<(RowId, usize)> = Vec::new();
    for row in rows {
        match counts.iter_mut().find(|(seen, _)| seen == row) {
            Some((_, count)) => *count += 1,
            None => counts.push((*row, 1)),
        }
    }
    let rounds = counts.iter().map(|(_, count)| *count).max().unwrap_or(0);
    (0..rounds)
        .map(|round| {
            counts
                .iter()
                .filter(|(_, count)| *count > round)
                .map(|(row, _)| *row)
                .collect()
        })
        .collect()
}

/// セルの値を書く（`Change::SetCells` の写し）。
///
/// 順方向は [`Document::set_cells`] を **1 回**呼ぶ（module docs の判断 2）。同じセルの最後の
/// 書き込みへの解決と、行の幅を越えて書いたときの値なしの埋め方は上流の 1 か所だけが持つ規則
/// であり、本モジュールはそれを写し直さない。
///
/// 逆命令（適用の前の値）とやり直し（適用の後の値）は、**どちらも文書から読む**。値の並びを
/// 組み立て直さないのは、行の幅の規則を二重に実装しないためである。読むのは**書く行だけ**
/// であり、行数には比例しない。
fn write_cells(
    document: &mut Document,
    sheet: SheetId,
    changes: &ChangeSet,
) -> Result<Step, MacroApplyError> {
    let index = positions(document, sheet)?;
    let mut before: Vec<RestoredRow> = Vec::new();
    let mut cells: Vec<(RowId, usize, CellValue)> = Vec::with_capacity(changes.cell_count());
    {
        let rows = rows_of(document, sheet)?;
        for ((named, row, column), value) in changes.cell_writes() {
            // `inspect` がすべての書き込みのシートを照合済みである（ここで食い違えば写像の
            // 前提が崩れている）。
            require_sheet(sheet, *named)?;
            // 行ごとに材料を 1 つ作る（同じ行の複数の書き込みは 1 つの材料に載る。
            // `cell_writes` の並びは (シート, 行, 列) の昇順であるため、行は連続して現れる）。
            if before.last().map(|material| material.id) != Some(*row) {
                let at = *index
                    .get(row)
                    .ok_or(MacroApplyError::UnknownRow { sheet, row: *row })?;
                before.push(RestoredRow {
                    id: *row,
                    position: at,
                    values: rows[at].values().to_vec(),
                });
            }
            cells.push((*row, column.index(), value.clone()));
        }
    }
    document
        .set_cells(sheet, &cells)
        .map_err(|error| MacroApplyError::Rejected {
            sheet,
            reason: error.to_string(),
        })?;

    // 適用の後の値（やり直しの材料）。同じ行を同じ並びで読む。
    let after: Vec<RestoredRow> = {
        let rows = rows_of(document, sheet)?;
        before
            .iter()
            .map(|material| {
                let at = *index.get(&material.id).ok_or(MacroApplyError::UnknownRow {
                    sheet,
                    row: material.id,
                })?;
                Ok(RestoredRow {
                    id: material.id,
                    position: material.position,
                    values: rows[at].values().to_vec(),
                })
            })
            .collect::<Result<Vec<_>, MacroApplyError>>()?
    };
    Ok(Step {
        inverse: vec![HistoryCommand::RestoreValues {
            sheet,
            rows: before,
        }],
        redo: vec![HistoryCommand::RestoreValues { sheet, rows: after }],
    })
}

/// 行を取り除く（`Change::RemoveRows` の写し）。
///
/// 取り除くのは**畳んだ行の集合**である（`ChangeSet::removed_rows`。呼び出しをまたぐ同じ行の
/// 繰り返しは 1 回の削除に畳まれる）。逆命令とやり直しは `data-grid` が組んだ対をそのまま使う
/// （逆命令は取り除いた行を同じ識別子・同じ位置・同じ値で差し戻す）。
fn remove_rows(
    document: &mut Document,
    apply: &mut EditApply,
    order: &RowOrder,
    sheet: SheetId,
    rows: Vec<RowId>,
) -> Result<Step, MacroApplyError> {
    let (_, pair) = apply
        .apply_with_inverse(
            document,
            EditCommand::RemoveRows {
                target: RowTarget::Ids(rows),
            },
            order,
        )
        .map_err(rejected(sheet))?;
    match pair {
        Some(pair) => Ok(Step {
            inverse: vec![pair.inverse],
            redo: vec![pair.redo],
        }),
        // 非空の削除は必ず状態を変える（`affected` が空にならない）ため対を持つ。持たなければ
        // 写像の前提が崩れている。
        None => Err(MacroApplyError::Inconsistent {
            reason: "RemoveRows が対を返さなかった".to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use document_format::{IdFactory, SchemaPart};
    use document_session::{DocumentSessionsApi, SessionState};
    use macro_runtime::host::HostError;
    use macro_runtime::{CellWrite, Change, ChangeSet};
    use macro_runtime::{ColumnTypeInfo, RowPage, RowSpan, SheetInfo};

    use super::*;

    use data_grid::{EditApply, UndoRedo};
    use schema_engine::{SchemaEngine, SchemaEngineApi, TypeRegistry};

    /// 標本のウィンドウ。
    fn window() -> WindowLabel {
        WindowLabel::new("doc-1")
    }

    /// 列の宣言（種別は text。値は文字列のまま往復する）。
    fn declaration(columns: usize) -> String {
        let entries: Vec<String> = (0..columns)
            .map(|column| format!(r#"{{"name":"c{column}","type":{{"kind":"text"}}}}"#))
            .collect();
        format!(
            r#"{{"root":{{"columns":[{}]}},"types":[]}}"#,
            entries.join(",")
        )
    }

    /// 列の名前。
    fn names(columns: usize) -> Vec<String> {
        (0..columns).map(|column| format!("c{column}")).collect()
    }

    /// 行数 × 列数の標本の文書を用意し、そのシートと計画を返す。
    ///
    /// 行の値は `r{行}c{列}` である（比較のためだけの値であり、列の型に適合する）。
    fn open(sessions: &DocumentSessions, columns: usize, rows: usize) -> (SheetId, CompiledSchema) {
        let window = window();
        sessions.create(&window).expect("新規の文書を用意できる");
        let sheet = sessions
            .read(&window, &mut |document| document.sheets()[0].id())
            .expect("文書を読める");
        sessions
            .edit(&window, &mut |document| {
                document
                    .set_sheet_columns(sheet, names(columns))
                    .expect("列を宣言できる");
                document
                    .set_root_schema(
                        sheet,
                        SchemaPart::parse(&declaration(columns)).expect("宣言は妥当"),
                    )
                    .expect("宣言を置ける");
                let ids: Vec<RowId> = (0..rows)
                    .map(|_| document.add_row(sheet).expect("行を足せる"))
                    .collect();
                // 値は 1 回の一括で書く（行ごとに書くと上流の行の索引が毎回走る）。
                let cells: Vec<(RowId, usize, CellValue)> = ids
                    .iter()
                    .flat_map(|id| {
                        (0..columns).map(move |column| {
                            (*id, column, CellValue::Text(format!("r{id}c{column}")))
                        })
                    })
                    .collect();
                document.set_cells(sheet, &cells).expect("値を書ける");
            })
            .expect("適用できる");
        let schema = sessions
            .read(&window, &mut |document| {
                SchemaEngine::new()
                    .compile(
                        document.sheet_by_id(sheet).expect("シートがある"),
                        &TypeRegistry::new(),
                    )
                    .expect("計画を組める")
            })
            .expect("文書を読める");
        (sheet, schema)
    }

    /// そのシートの行の識別子（文書の順）。
    fn row_ids(sessions: &DocumentSessions, sheet: SheetId) -> Vec<RowId> {
        sessions
            .read(&window(), &mut |document| {
                document
                    .sheet_by_id(sheet)
                    .expect("シートがある")
                    .rows()
                    .iter()
                    .map(Row::id)
                    .collect()
            })
            .expect("文書を読める")
    }

    /// 文書の内容（シートごとの行の識別子と値）。`Document` は `PartialEq` を持たないため、
    /// 比較はアクセサで写して行う。
    fn snapshot(sessions: &DocumentSessions) -> Vec<(SheetId, Vec<(RowId, Vec<CellValue>)>)> {
        sessions
            .read(&window(), &mut |document| {
                document
                    .sheets()
                    .iter()
                    .map(|sheet| {
                        let rows = sheet
                            .rows()
                            .iter()
                            .map(|row| (row.id(), row.values().to_vec()))
                            .collect();
                        (sheet.id(), rows)
                    })
                    .collect()
            })
            .expect("文書を読める")
    }

    /// 文書の内容（シートごとの行の値だけ）。`snapshot` と違い**行の識別子を含まない** —
    /// やり直しは複製の識別子を新しく発行するため（`data-grid` の `DuplicateRows` の
    /// やり直し）、識別子まで一致するのは取り消しの側だけである（要件 7.2 が求めるのは
    /// 「同じ行と値をもう一度適用する」ことである）。
    fn contents(
        snapshot: &[(SheetId, Vec<(RowId, Vec<CellValue>)>)],
    ) -> Vec<(SheetId, Vec<Vec<CellValue>>)> {
        snapshot
            .iter()
            .map(|(sheet, rows)| {
                (
                    *sheet,
                    rows.iter().map(|(_, values)| values.clone()).collect(),
                )
            })
            .collect()
    }

    /// そのウィンドウの版（変更の適用と差し替えで 1 進む。`SessionState::Open`）。
    fn revision(sessions: &DocumentSessions) -> u64 {
        match sessions.state(&window()) {
            SessionState::Open { revision, .. } => revision,
            other => panic!("文書を保持していない: {other:?}"),
        }
    }

    /// 標本の並び（文書の順）。
    fn order_of(sessions: &DocumentSessions, sheet: SheetId) -> RowOrder {
        let mut order = RowOrder::default();
        sessions
            .read(&window(), &mut |document| {
                order.recompute(document, sheet, &ViewSpec::default());
            })
            .expect("文書を読める");
        order
    }

    /// 取り消し（`GridSession::undo` と同じ道。履歴と適用の経路を束ねた口を通す）。
    fn undo(
        sessions: &DocumentSessions,
        history: &mut UndoStack,
        sheet: SheetId,
        schema: &CompiledSchema,
    ) {
        let order = order_of(sessions, sheet);
        let mut apply = EditApply::new(sheet, schema.clone());
        let mut undo_redo = UndoRedo::new(history, &mut apply);
        sessions
            .edit(&window(), &mut |document| undo_redo.undo(document, &order))
            .expect("取り消しの閉包は走る")
            .value
            .expect("取り消せる")
            .expect("取り消す操作がある");
    }

    /// やり直し（`GridSession::redo` と同じ道）。
    fn redo(
        sessions: &DocumentSessions,
        history: &mut UndoStack,
        sheet: SheetId,
        schema: &CompiledSchema,
    ) {
        let order = order_of(sessions, sheet);
        let mut apply = EditApply::new(sheet, schema.clone());
        let mut undo_redo = UndoRedo::new(history, &mut apply);
        sessions
            .edit(&window(), &mut |document| undo_redo.redo(document, &order))
            .expect("やり直しの閉包は走る")
            .value
            .expect("やり直せる")
            .expect("やり直す操作がある");
    }

    /// 実行 1 回ぶんの縫い目（**適用がこれをどう使うかを観測する**ための二重）。
    ///
    /// `with_changes` だけが適用で使われる口であり、他の口は適用が呼べば落ちるようにしてある
    /// （呼べば「読みや能力の口を適用が使っている」ことが分かる）。
    struct FakeHost {
        changes: ChangeSet,
    }

    impl FakeHost {
        fn new() -> Self {
            Self {
                changes: ChangeSet::new(),
            }
        }

        /// マクロの書き込みを 1 件集める（本番では `HostPort::stage` へ届くもの）。
        fn collect(&mut self, change: Change) {
            self.changes.stage(change).expect("変更を集められる");
        }
    }

    impl HostPort for FakeHost {
        fn sheets(&self) -> Result<Vec<SheetInfo>, HostError> {
            panic!("変更の適用はシートの一覧を読まない")
        }

        fn columns(&self, _sheet: SheetId) -> Result<Vec<ColumnTypeInfo>, HostError> {
            panic!("変更の適用は列の宣言を読まない")
        }

        fn read_rows(&self, _sheet: SheetId, _span: RowSpan) -> Result<RowPage, HostError> {
            panic!("変更の適用は行を読まない")
        }

        fn stage(&self, _change: Change) -> Result<(), HostError> {
            panic!("変更を集めるのはテストの側である")
        }

        fn with_changes(&self, read: &mut dyn FnMut(&ChangeSet)) {
            read(&self.changes);
        }

        fn file_read(&self, _path: &str) -> Result<String, HostError> {
            panic!("変更の適用はファイルを読まない")
        }

        fn file_write(&self, _path: &str, _text: &str) -> Result<(), HostError> {
            panic!("変更の適用はファイルへ書かない")
        }

        fn net_fetch(&self, _url: &str) -> Result<String, HostError> {
            panic!("変更の適用はネットワークを使わない")
        }
    }

    /// セルへの書き込み 1 件（列は 0 起点）。
    fn write(row: RowId, column: usize, text: &str) -> CellWrite {
        CellWrite::new(
            row,
            ColumnIndex::new(column),
            CellValue::Text(text.to_owned()),
        )
    }

    /// **マクロの変更が取り消し 1 回で全部戻り、やり直し 1 回で全部戻る**（要件 7.1, 7.2）。
    ///
    /// 4 種類の変更（値・追加・削除・複製）を 1 つの変更集合へ入れ、履歴が **1 対**しか
    /// 増えないこと（1 命令ずつ積まれていないこと）と、取り消し・やり直しが**文書まるごと**を
    /// 往復させることを固定する。
    #[test]
    fn a_macro_run_is_undone_and_redone_in_one_step() {
        let sessions = DocumentSessions::new();
        let (sheet, schema) = open(&sessions, 2, 4);
        let rows = row_ids(&sessions, sheet);

        let mut host = FakeHost::new();
        // 値（2 行 × 2 列）・追加（2 行）・複製（同じ行を 2 度 = 2 行増える）・削除（1 行）。
        host.collect(Change::SetCells {
            sheet,
            writes: vec![
                write(rows[0], 0, "書き換え0"),
                write(rows[0], 1, "書き換え1"),
                write(rows[2], 0, "書き換え2"),
                write(rows[2], 1, "書き換え3"),
            ],
        });
        host.collect(Change::InsertRows {
            sheet,
            values: vec![
                vec![CellValue::Text("追加0".to_owned())],
                vec![CellValue::Text("追加1".to_owned())],
            ],
        });
        host.collect(Change::DuplicateRows {
            sheet,
            rows: vec![rows[1], rows[1]],
        });
        host.collect(Change::RemoveRows {
            sheet,
            rows: vec![rows[3]],
        });

        let mut history = UndoStack::new(100);
        let before = snapshot(&sessions);
        let summary =
            apply_macro_changes(&sessions, &window(), sheet, &schema, &host, &mut history)
                .expect("適用できる");
        assert_eq!(
            ChangeSummary {
                set_cells: 4,
                inserted_rows: 2,
                removed_rows: 1,
                duplicated_rows: 2,
            },
            summary,
            "件数が変更集合と食い違う"
        );
        let after = snapshot(&sessions);
        assert_ne!(before, after, "変更が文書へ届いていない");
        assert!(
            !row_ids(&sessions, sheet).contains(&rows[3]),
            "削除が届いていない"
        );

        // **1 対だけ**（1 命令ずつ積まれていない）。
        assert_eq!(1, history.depth(), "履歴へ積まれた対が 1 つでない");
        assert_eq!(UndoLabel::MacroRun, history.entries()[0].label);

        // 取り消し 1 回で全部戻る。
        undo(&sessions, &mut history, sheet, &schema);
        assert_eq!(
            before,
            snapshot(&sessions),
            "取り消しで実行前へ戻っていない"
        );

        // やり直し 1 回で全部戻る。**識別子は複製のぶんだけ変わる**（やり直しの複製は
        // 新しい識別子を発行する。`data-grid` の規律）ため、ここは行と値を比べる。
        redo(&sessions, &mut history, sheet, &schema);
        assert_eq!(
            contents(&after),
            contents(&snapshot(&sessions)),
            "やり直しで実行後へ戻っていない"
        );
    }

    /// **適用の失敗ではドキュメントが変わらない**（要件 5.4, 7.3）。
    ///
    /// 別のシートを名乗る変更集合は、適用の**前**に拒まれ、文書・版・履歴のどれも動かない
    /// （`document-session` の `edit` は閉包が失敗しても版を進めるため、拒否が `edit` の外で
    /// 決まることがこの性質の本体である）。
    #[test]
    fn a_change_naming_another_sheet_is_refused_without_writing() {
        let sessions = DocumentSessions::new();
        let (sheet, schema) = open(&sessions, 2, 2);
        // 2 枚目のシートを、列と行を持つ形で用意する（拒否は適用先のシートだけを見るが、
        // 宛先として妥当な行を指す変更集合を作るため）。
        let other = sessions
            .edit(&window(), &mut |document| document.add_sheet("別のシート"))
            .expect("文書を編集できる")
            .value;
        let other_row = sessions
            .edit(&window(), &mut |document| {
                document
                    .set_sheet_columns(other, names(1))
                    .expect("列を宣言できる");
                document.add_row(other).expect("行を足せる")
            })
            .expect("文書を編集できる")
            .value;

        let mut host = FakeHost::new();
        host.collect(Change::SetCells {
            sheet: other,
            writes: vec![write(other_row, 0, "別のシートへの書き込み")],
        });

        let mut history = UndoStack::new(100);
        let before = snapshot(&sessions);
        let version_before = revision(&sessions);
        let error = apply_macro_changes(&sessions, &window(), sheet, &schema, &host, &mut history)
            .expect_err("別のシートを名乗る変更を拒む");
        assert!(
            matches!(error, MacroApplyError::SheetMismatch { expected, found } if expected == sheet && found == other),
            "拒否の理由がシートの食い違いでない"
        );
        assert_eq!(before, snapshot(&sessions), "拒否したのに文書が変わった");
        assert_eq!(
            version_before,
            revision(&sessions),
            "拒否したのに版が動いた"
        );
        assert_eq!(0, history.depth(), "拒否したのに履歴が動いた");
    }

    /// 存在しない行を指した変更集合は、適用の前に拒まれ、**先に集まっている妥当な変更も
    /// 1 つも書かれない**（要件 5.4 の「変更を適用せずに理由を提示する」＝部分適用の不在）。
    #[test]
    fn a_change_naming_a_missing_row_is_refused_without_writing() {
        let sessions = DocumentSessions::new();
        let (sheet, schema) = open(&sessions, 2, 2);
        let row = row_ids(&sessions, sheet)[0];
        // 文書が発行していない識別子（別の発行者が作った 1 つ）。
        let missing = IdFactory::default().new_row_id();

        let mut host = FakeHost::new();
        // 先に**適用できるはずの**変更を集めてから、拒まれるべき変更を集める（部分適用が
        // 起きるならここで先の 1 件が文書へ届く）。
        host.collect(Change::SetCells {
            sheet,
            writes: vec![write(row, 0, "先に集まっている書き込み")],
        });
        host.collect(Change::SetCells {
            sheet,
            writes: vec![write(missing, 0, "無い行")],
        });

        let mut history = UndoStack::new(100);
        let before = snapshot(&sessions);
        let version_before = revision(&sessions);
        let error = apply_macro_changes(&sessions, &window(), sheet, &schema, &host, &mut history)
            .expect_err("無い行を拒む");
        assert!(
            matches!(error, MacroApplyError::UnknownRow { row, .. } if row == missing),
            "拒否の理由が行の不在でない"
        );
        assert_eq!(before, snapshot(&sessions), "拒否したのに文書が変わった");
        assert_eq!(
            version_before,
            revision(&sessions),
            "拒否したのに版が動いた"
        );
        assert_eq!(0, history.depth(), "拒否したのに履歴が動いた");
    }

    /// 存在しない列を指した変更集合は、適用の前に拒まれ、何も書かない（要件 5.4）。
    #[test]
    fn a_change_naming_a_missing_column_is_refused_without_writing() {
        let sessions = DocumentSessions::new();
        let (sheet, schema) = open(&sessions, 2, 2);
        let row = row_ids(&sessions, sheet)[0];

        let mut host = FakeHost::new();
        host.collect(Change::SetCells {
            sheet,
            writes: vec![write(row, 2, "無い列")],
        });

        let mut history = UndoStack::new(100);
        let before = snapshot(&sessions);
        let error = apply_macro_changes(&sessions, &window(), sheet, &schema, &host, &mut history)
            .expect_err("無い列を拒む");
        assert!(
            matches!(
                error,
                MacroApplyError::UnknownColumn { column, columns, .. }
                    if column == ColumnIndex::new(2) && columns == 2
            ),
            "拒否の理由が列の不在でない"
        );
        assert_eq!(before, snapshot(&sessions), "拒否したのに文書が変わった");
        assert_eq!(0, history.depth(), "拒否したのに履歴が動いた");
    }

    /// 変更が 1 件も無い実行は、版も履歴も動かさない（要件 2.6 の裏面）。
    #[test]
    fn an_empty_change_set_touches_nothing() {
        let sessions = DocumentSessions::new();
        let (sheet, schema) = open(&sessions, 2, 2);
        let host = FakeHost::new();
        let mut history = UndoStack::new(100);
        let before = snapshot(&sessions);
        let version_before = revision(&sessions);

        let summary =
            apply_macro_changes(&sessions, &window(), sheet, &schema, &host, &mut history)
                .expect("空の変更集合は適用できる");
        assert!(summary.is_empty(), "空の実行が件数を返した");
        assert_eq!(before, snapshot(&sessions), "空の実行で文書が変わった");
        assert_eq!(version_before, revision(&sessions), "空の実行で版が動いた");
        assert_eq!(0, history.depth(), "空の実行で履歴が動いた");
    }

    /// **10 万セルの変更を、写像を複製せずに 1 回で適用できる**（要件 11.3, 7.1）。
    ///
    /// 変更集合は縫い目（[`HostPort::with_changes`]）の**借用のまま**読まれる（写像を複製する
    /// 口を持たない。写像を丸ごと複製する経路が無いことは、この道が `&ChangeSet` しか受け取らない
    /// ことに現れている）。ここでは 1 行 1 列ずつ書き換える 10 万件で、その道が規模に対して通る
    /// ことと、**取り消しが 1 歩**（10 万行ぶんの材料を 1 つの命令が覆う）であることを固定する。
    ///
    /// **取り消しの適用そのものはここで走らせない**（実測: 10 万行の値の復元は 35.2 秒）。
    /// `data-grid` の `RestoreValues` は行ごとに `Document::set_row_values` を呼び、その口が
    /// 行を線形に探す（`Sheet::set_row_values`）ため、費用が**行数 × 書いた行数**になる
    /// （上流の実装であり、本モジュールの写像ではない — 本モジュールの適用は同じ条件で
    /// 259.9 ミリ秒である）。往復そのものは [`a_macro_run_is_undone_and_redone_in_one_step`]
    /// が小さい標本で固定する。
    #[test]
    fn a_hundred_thousand_cells_are_applied_in_one_step() {
        const ROWS: usize = 100_000;
        let sessions = DocumentSessions::new();
        let (sheet, schema) = open(&sessions, 1, ROWS);
        let rows = row_ids(&sessions, sheet);
        assert_eq!(ROWS, rows.len());

        let mut host = FakeHost::new();
        host.collect(Change::SetCells {
            sheet,
            writes: rows.iter().map(|row| write(*row, 0, "一括")).collect(),
        });

        let mut history = UndoStack::new(100);
        let summary =
            apply_macro_changes(&sessions, &window(), sheet, &schema, &host, &mut history)
                .expect("10 万セルを適用できる");
        assert_eq!(ROWS, summary.set_cells);
        assert_eq!(1, history.depth(), "履歴が 1 対でない");
        // 取り消しの 1 歩が**10 万行ぶんの材料**を持つ（適用と同じ 1 回で全部を覆う）。
        match &history.entries()[0].inverse {
            HistoryCommand::RestoreValues {
                sheet: named,
                rows: material,
            } => {
                assert_eq!(sheet, *named, "取り消しの材料が別のシートを名乗る");
                assert_eq!(ROWS, material.len(), "取り消しの材料が全部を覆っていない");
            }
            other => panic!("取り消しが値の復元の 1 歩でない: {other:?}"),
        }
        // 書けたこと（行数・先頭・末尾を確かめる）。
        let written = sessions
            .read(&window(), &mut |document| {
                let rows = document.sheet_by_id(sheet).expect("シートがある").rows();
                (
                    rows.len(),
                    rows[0].values()[0].clone(),
                    rows[ROWS - 1].values()[0].clone(),
                )
            })
            .expect("文書を読める");
        assert_eq!(ROWS, written.0, "行の数が変わった");
        assert_eq!(CellValue::Text("一括".to_owned()), written.1);
        assert_eq!(CellValue::Text("一括".to_owned()), written.2);
    }
}

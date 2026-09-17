//! 未適用の変更の集合（tasks.md 2.3。要件 5.3, 5.4, 5.5。design.md 決定 2 / 3）。
//!
//! マクロの書き込み（セルの値・行の追加・削除・複製）を**適用せずに**ここへ集める。
//! 集めたものを文書へ適用するのはアダプタ（`src-tauri` の `macro_apply`。タスク 4.2）で
//! あり、本層は文書を知らない（design.md 決定 2 / 3）。このクレートが `data-grid` /
//! `document-session` を依存に持たない理由がそれである（`Cargo.toml` の依存方針 2）。
//!
//! # 集約の規則（この 5 つがこの層の意味の全体）
//!
//! 1. **同じセルへの複数の書き込みは後ろが残る** — 適用は 1 回であり、途中の値はどこにも
//!    届かない。したがって [`ChangeSet`] はセルの書き込みを
//!    `BTreeMap<(SheetId, RowId, ColumnIndex), CellValue>` へ**解決しながら**畳む
//!    （design.md「Data Models / Domain Model」の不変条件）。
//! 2. **適用の順序は 追加 → 値 → 削除** — [`ChangeSet::application_order`] がその順に
//!    1 件ずつ返す（design.md「State Management」）。追加の中は「挿入 → 複製」の順だが、
//!    挿入した行は**適用まで識別子を持たない**ため複製が挿入した行を指すことは起こり得ず、
//!    この 2 つの相対順は結果を変えない（決定的であることだけが要る）。
//! 3. **1 回の呼び出しは 1 件**（[`ChangeSet::change_count`]）— 10 万行の一括挿入は
//!    1 件の [`RowInsert`] であり、10 万件の変更ではない。要素は**移す**だけで複製しない
//!    （`Vec` をそのまま受け取る。要件 11.3 の一括処理）。
//! 4. **拒むときは何も残さない** — [`ChangeSet::stage`] は呼び出しの全体を先に検査し、
//!    1 つでも通せなければ [`ChangeError`] を返して**その呼び出しの分を 1 つも残さない**。
//!    半端に残ると、マクロが例外を見たのに文書が変わる状態が生まれる（適用は 1 回なので、
//!    残った分は実行が成功したときにまとめて適用される）。
//! 5. **空の並びは記録しない** — 何もしない呼び出し（`setCells(sheet, [])`）は
//!    [`ChangeSet::change_count`] も件数も増やさない（`is_empty` と件数が食い違わない）。
//!
//! # 存在しない行を拒むのは誰か（要件 5.4 の分界）
//!
//! 要件 5.4 の「存在しない行または列を指したら、変更を適用せず理由を提示する」のうち、
//! **この層が判定できる分**と**できない分**を分けて書く（黙って取りこぼさないため）。
//!
//! - **判定できる**: この実行が既に削除した行。削除は適用の最後に行われるが、その行は
//!   適用時に**存在しない**ため、書き込みと複製は通せない（[`ChangeError::RemovedRow`]）。
//!   同じ行を 2 度削除することは誤りではなく、1 回の削除に畳まれる
//!   （上流 `document-format` の `RowRemovalError` の規約と同じ。要件 5.3 の削除の意味は
//!   「その行が無くなること」であって「削除命令の回数」ではない）。
//! - **判定できない**: 文書に元から無い行と、範囲外の列。エンジンは文書を知らない
//!   （design.md 決定 2）ため、これらはホストの縫い目（`HostPort::stage` の実装。タスク
//!   4.1）が文書を見て拒む。その拒否は `FailureKind::HostRejected` として、本層の拒否と
//!   同じ経路でマクロへ返る（タスク 3.2 が `host.setCells` などの op で写す）。
//!
//! **適用の直前にも**アダプタがシートを照合する（タスク 4.2）。要件 2.2 により実行中も
//! 利用者は表を編集できるため、集約した時点の判断は適用時点の保証にならない。
//!
//! # 失敗・打ち切りとの関係
//!
//! 実行が `Failed` / `Aborted` で終わったとき、集めた変更は**適用されない**
//! （要件 6.3 / 7.3）。[`ChangeSet`] は実行の間だけ保持され、実行が終われば捨てられる
//! （design.md「Data Models / Domain Model」の `ChangeSet` は実行 1 回のトランザクション
//! 境界である）。
//!
//! # 件数の型をここに置かない理由（層の鎖）
//!
//! 件数（種別ごと）の**型**はタスク 1.3 が `engine/outcome.rs` の `ChangeSummary` に
//! 置いている。本層は `engine` を参照しない（層の鎖は
//! `error / source → surface → host → engine → …` であり、左の層だけを参照する）。
//! したがってここは**数える口**（[`ChangeSet::cell_count`] たち）だけを持ち、実行の結果
//! （`RunOutcome::Ran` の変更の件数）への詰め替えはエンジン層（タスク 3.2）が行う。
//!
//! # マクロ向けの `CellWrite`
//!
//! 宣言表（タスク 2.1）は `setCells(sheet, writes: CellWrite[])` を公開しており、その
//! `CellWrite` は（行の識別子, 列の添字, 値）の 3 つ組である。Rust 側の形は名前付きの
//! [`CellWrite`] である。design.md「Service Interface」は `Change` の腕を
//! `writes: Vec<(RowId, ColumnIndex, CellValue)>` と書くが、**同じ 3 つ組を 2 通りに
//! 書かない**ため、また境界の型の名（`CellWrite` / `RowPage` / `SheetInfo` …）が宣言表で
//! 名前を持つのと同じく**名前を持たせる**ため、構造体 1 つに寄せている（上流
//! `Document::set_cells` が受ける 3 つ組と同じ並びである）。

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use document_format::{CellValue, RowId, SheetId};
use schema_engine::ColumnIndex;

/// 1 セルへの書き込み（マクロ側の `host.setCells(sheet, writes)` の要素）。
///
/// 列の添字は **0 起点**で、[`ColumnIndex`] が指す位置（`Sheet::columns` の並びに対する
/// 位置）である。行は文書の識別子であり、マクロは**読みで受け取った識別子**をそのまま返す
/// （タスク 2.4 の範囲の読みが `RowId` を渡す）。
///
/// JS 側の型名 `CellWrite`（要件 4.6 / 10.1）を持つのはこの型であり、タスク 3.3 の生成器が
/// 宣言表と本型から `.d.ts` へ出す。
#[derive(Debug, Clone, PartialEq)]
pub struct CellWrite {
    /// 書き込む行（文書の識別子）。
    pub row: RowId,
    /// 書き込む列（0 起点）。
    pub column: ColumnIndex,
    /// 書き込む値。
    pub value: CellValue,
}

impl CellWrite {
    /// 1 セルへの書き込みを組み立てる。
    pub fn new(row: RowId, column: ColumnIndex, value: CellValue) -> Self {
        Self { row, column, value }
    }
}

/// 未適用の変更 1 件（design.md「Service Interface」の `Change`。要件 5.1, 5.3）。
///
/// 4 つの腕が要件 5.3 の 4 操作そのものである。**適用はしない** — [`ChangeSet::stage`] が
/// 集約し、アダプタが 1 回の編集として適用する（design.md 決定 3）。
///
/// 呼び出し 1 回が [`ChangeSet`] の 1 件になる（規則 3）。したがって 10 万行の追加は
/// `values` に 10 万行を入れた 1 件であり、行ごとに呼ぶ必要は無い（要件 11.3）。
#[derive(Debug, PartialEq)]
pub enum Change {
    /// セルの値を書き換える（要件 5.1）。同じセルへ複数回書いてよく、**後ろの値が残る**
    /// （規則 1。適用は 1 回なので途中の値は届かない）。
    SetCells {
        /// 対象のシート。
        sheet: SheetId,
        /// 書き換えるセル。同じセルが複数回現れてよい（最後のものが残る）。
        writes: Vec<CellWrite>,
    },
    /// 行を追加する（要件 5.3）。`values` の要素 1 つが 1 行である。
    ///
    /// 追加した行は**適用まで識別子を持たない**（design.md「Responsibilities &
    /// Constraints」）ため、この腕だけは行の実在を要さない。
    InsertRows {
        /// 対象のシート。
        sheet: SheetId,
        /// 追加する行の値。要素 1 つが 1 行である。
        values: Vec<Vec<CellValue>>,
    },
    /// 行を取り除く（要件 5.3）。同じ行が複数回現れてよい（1 回の削除に畳まれる）。
    RemoveRows {
        /// 対象のシート。
        sheet: SheetId,
        /// 取り除く行。**適用時に存在する行**でなければならない（要件 5.4）。
        rows: Vec<RowId>,
    },
    /// 行を複製する（要件 5.3）。`rows` の行が同じ内容で増える（識別子は新しく発行される。
    /// 誰が発行するかはアダプタの仕事であり、本層は関知しない）。
    DuplicateRows {
        /// 対象のシート。
        sheet: SheetId,
        /// 複製する行。**適用時に存在する行**でなければならない（要件 5.4）。
        rows: Vec<RowId>,
    },
}

/// 集約した行の挿入 1 件（**呼び出し 1 回が 1 件**）。
///
/// 10 万行の一括はこの 1 件に入る（規則 3）。値は複製せずに移す。
#[derive(Debug, PartialEq)]
pub struct RowInsert {
    /// 対象のシート。
    pub sheet: SheetId,
    /// 追加する行の値。要素 1 つが 1 行である。
    pub values: Vec<Vec<CellValue>>,
}

/// 集約した行の削除 1 件（**呼び出し 1 回が 1 件**）。
#[derive(Debug, PartialEq)]
pub struct RowRemove {
    /// 対象のシート。
    pub sheet: SheetId,
    /// 取り除く行（呼び出しに渡された並びのまま。同じ行が複数回現れうる）。
    /// 重複を畳んだ**行の集合**は [`ChangeSet::removed_rows`] が持つ。
    pub rows: Vec<RowId>,
}

/// 集約した行の複製 1 件（**呼び出し 1 回が 1 件**）。
///
/// 同じ行を 2 度入れてよい（2 行増える。複製は行を減らさないため、削除と違って畳まれない）。
#[derive(Debug, PartialEq)]
pub struct RowDuplicate {
    /// 対象のシート。
    pub sheet: SheetId,
    /// 複製する行。**同じ行の繰り返しは繰り返しの数だけ増える**。
    pub rows: Vec<RowId>,
}

/// 変更を集約できなかった理由（**表示の文言は持たない**。組み立てるのは提示の層である）。
///
/// `document-format` の `CellWriteError` / `RowRemovalError` と同じ規律であり、判別可能な
/// 変種が文脈（どのシート・どの行）だけを持つ。拒否を `FailureKind::HostRejected` へ写すのは
/// ホスト API の op（タスク 3.2）であり、そのときに API の名前が付く。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeError {
    /// この実行が既に削除した行を指した（要件 5.4）。
    ///
    /// 削除は適用の最後に行われるが、その行は**適用時に存在しない**ため、書き込みと複製は
    /// 通せない。同じ行の削除を 2 度受け取ることはこの理由に当たらない（1 回に畳まれる）。
    RemovedRow {
        /// 対象のシート。
        sheet: SheetId,
        /// 削除済みの行（指された識別子）。
        row: RowId,
    },
}

impl fmt::Display for ChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RemovedRow { sheet, row } => {
                write!(f, "row {row} of sheet {sheet} has been removed in this run")
            }
        }
    }
}

impl std::error::Error for ChangeError {}

/// 適用の順序で並べた 1 件（**追加 → 値 → 削除**。design.md「State Management」）。
///
/// [`ChangeSet::application_order`] だけがこれを返す。適用する側は腕ごとに命令へ写せばよく、
/// **順序を自分で組み立て直す必要が無い**（順序が型と手順の側に固定されている）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StagedChange<'a> {
    /// 行の挿入（追加）。
    Insert(&'a RowInsert),
    /// 行の複製（追加）。
    Duplicate(&'a RowDuplicate),
    /// セルの書き換え（値）。**同じセルの最後の書き込み**だけが現れる（規則 1）。
    SetCells {
        /// 対象のシート。
        sheet: SheetId,
        /// 対象の行。
        row: RowId,
        /// 対象の列（0 起点）。
        column: ColumnIndex,
        /// 書き込む値（値への参照。10 万セルの写像でコピーを作らない）。
        value: &'a CellValue,
    },
    /// 行の削除（削除）。
    Remove(&'a RowRemove),
}

/// 未適用の変更の集合（design.md「Data Models」の `ChangeSet`。実行 1 回のトランザクション
/// 境界であり、適用されると消える）。
///
/// 状態は 5 つに分かれる（規則 1〜3）:
///
/// - `sets`: セルの書き込みの**解決済みの写像**（同じセルは最後の値。キーはシート・行・列）
/// - `inserts` / `duplicates` / `removes`: 呼び出し 1 回 = 1 件の追加・複製・削除
/// - `removed`: この実行が削除する**行の集合**（重複を畳んだもの。削除の検査が読む）
///
/// 文書と `data-grid` の状態は持たない（エンジンは文書を知らない。design.md 決定 2）。
#[derive(Debug, Default)]
pub struct ChangeSet {
    /// セルの書き込み（同じセルは最後の値に解決済み）。
    sets: BTreeMap<(SheetId, RowId, ColumnIndex), CellValue>,
    /// 行の挿入（呼び出し 1 回 = 1 件）。
    inserts: Vec<RowInsert>,
    /// 行の削除（呼び出し 1 回 = 1 件。並びは呼び出しのまま）。
    removes: Vec<RowRemove>,
    /// 行の複製（呼び出し 1 回 = 1 件）。
    duplicates: Vec<RowDuplicate>,
    /// この実行が削除する行の集合（シートと行の対。重複を畳んだもの）。
    removed: BTreeSet<(SheetId, RowId)>,
}

impl ChangeSet {
    /// 空の集合を作る（実行の開始時に 1 つ作られる）。
    pub const fn new() -> Self {
        Self {
            sets: BTreeMap::new(),
            inserts: Vec::new(),
            removes: Vec::new(),
            duplicates: Vec::new(),
            removed: BTreeSet::new(),
        }
    }

    /// 1 件の変更を集める（**適用はしない**。design.md「Postconditions」）。
    ///
    /// 呼び出しの全体を先に検査し、1 つでも通せなければ [`ChangeError`] を返して**この
    /// 呼び出しの分を 1 つも残さない**（規則 4）。空の並びは記録しない（規則 5）。
    ///
    /// 拒否の条件は「この実行が既に削除した行を指したこと」だけである（要件 5.4 の分界は
    /// モジュール docs）。文書に無い行と範囲外の列は、ホストの縫い目（タスク 4.1）が
    /// 文書を見て拒む。
    pub fn stage(&mut self, change: Change) -> Result<(), ChangeError> {
        match change {
            Change::SetCells { sheet, writes } => {
                if writes.is_empty() {
                    return Ok(());
                }
                // 検査を先に済ませる（移動で写像を書き換える前に）。
                if let Some(write) = writes
                    .iter()
                    .find(|write| self.is_row_removed(sheet, write.row))
                {
                    return Err(ChangeError::RemovedRow {
                        sheet,
                        row: write.row,
                    });
                }
                for write in writes {
                    // 同じセルは上書きされる（規則 1。後ろの値だけが残る）。
                    self.sets
                        .insert((sheet, write.row, write.column), write.value);
                }
            }
            Change::InsertRows { sheet, values } => {
                if values.is_empty() {
                    return Ok(());
                }
                // 追加した行は適用まで識別子を持たないため、行の実在を要さない唯一の変更である。
                self.inserts.push(RowInsert { sheet, values });
            }
            Change::DuplicateRows { sheet, rows } => {
                if rows.is_empty() {
                    return Ok(());
                }
                if let Some(row) = rows.iter().find(|row| self.is_row_removed(sheet, **row)) {
                    return Err(ChangeError::RemovedRow { sheet, row: *row });
                }
                self.duplicates.push(RowDuplicate { sheet, rows });
            }
            Change::RemoveRows { sheet, rows } => {
                if rows.is_empty() {
                    return Ok(());
                }
                // 削除は**行の集合**へ畳む（同じ行の繰り返しは 1 回に畳まれる。上流
                // `Document::remove_rows` と同じ規約。識別子の重複は誤りではない）。
                // 拒否が無いのはこの腕だけである: 削除は行を消す側であり、消えた行を
                // 消すことは 1 回の削除と同じ意味を持つ（規則 1 の削除版）。
                self.removed.extend(rows.iter().map(|row| (sheet, *row)));
                self.removes.push(RowRemove { sheet, rows });
            }
        }
        Ok(())
    }

    /// 変更が 1 件も集まっていないか（実行の結果の「変更の有無」。要件 2.6）。
    ///
    /// 空の並びは記録しないため（規則 5）、これは `cell_count` たちがすべて 0 であることと
    /// 一致する。
    pub fn is_empty(&self) -> bool {
        self.change_count() == 0
    }

    /// 集約した**変更の件数**（適用の単位の数）。
    ///
    /// 挿入・複製・削除は**呼び出し 1 回が 1 件**である（10 万行の一括挿入も 1 件。要件
    /// 11.3）。セルの書き換えは**セルごとに 1 件**である（同じセルへ複数回書いても、解決後は
    /// 1 件しか残らない）。
    ///
    /// 「何件を変更したか」の提示（要件 5.5）が読むのはこれではなく件数の口
    /// （`cell_count` たち）である。
    pub fn change_count(&self) -> usize {
        self.inserts.len() + self.duplicates.len() + self.sets.len() + self.removes.len()
    }

    /// **適用の順序**（追加 → 値 → 削除）で 1 件ずつ返す（規則 2。design.md「State
    /// Management」）。
    ///
    /// 適用する側（アダプタの `macro_apply`。タスク 4.2）はこの並びのまま命令へ写す。
    /// セルの書き換えの並びは写像の反復順（添字の昇順）であり、**同じセルは解決済みで
    /// 1 件しか無い**ため、この順序は結果を変えない（決定的であることだけが要る）。
    ///
    /// 削除と複製の中の並びは**呼び出しに渡された順**である（呼び出しをまたぐ相対順も
    /// 呼び出し順である）。
    pub fn application_order(&self) -> impl Iterator<Item = StagedChange<'_>> {
        let inserts = self.inserts.iter().map(StagedChange::Insert);
        let duplicates = self.duplicates.iter().map(StagedChange::Duplicate);
        let sets = self
            .sets
            .iter()
            .map(|((sheet, row, column), value)| StagedChange::SetCells {
                sheet: *sheet,
                row: *row,
                column: *column,
                value,
            });
        let removes = self.removes.iter().map(StagedChange::Remove);
        inserts.chain(duplicates).chain(sets).chain(removes)
    }

    /// 書き換えるセルの写像（**最後の書き込みに解決済み**）。
    ///
    /// 読みの重ね合わせ（タスク 2.4 の `host/overlay.rs`）がこれを読む: マクロが書いた
    /// セルを読み直すと、書いた値が返る（要件 5.4 の裏面）。行の範囲を引くときは、
    /// キーが `(シート, 行, 列)` の順であることを使って
    /// `range((sheet, from, ColumnIndex::new(0))..=(sheet, to, ColumnIndex::new(usize::MAX)))`
    /// で走査できる。
    pub fn cell_writes(&self) -> &BTreeMap<(SheetId, RowId, ColumnIndex), CellValue> {
        &self.sets
    }

    /// 1 つのセルに書き込まれる値（書き込みが無ければ `None`）。
    ///
    /// [`Self::cell_writes`] と同じ写像を引く（1 セルの問い合わせのために写像を走査する
    /// 必要を無くすだけであり、規則は増えない）。
    pub fn cell_value(
        &self,
        sheet: SheetId,
        row: RowId,
        column: ColumnIndex,
    ) -> Option<&CellValue> {
        self.sets.get(&(sheet, row, column))
    }

    /// 集約した挿入（呼び出し順）。
    pub fn inserts(&self) -> &[RowInsert] {
        &self.inserts
    }

    /// 集約した削除（呼び出し順。並びは呼び出しのままで、重複を畳んでいない）。
    pub fn removes(&self) -> &[RowRemove] {
        &self.removes
    }

    /// 集約した複製（呼び出し順）。
    pub fn duplicates(&self) -> &[RowDuplicate] {
        &self.duplicates
    }

    /// この実行が削除する**行の集合**（重複を畳んだもの。シートと行の対）。
    ///
    /// 削除の検査（要件 5.4 の「存在しない行」）と、読みの重ね合わせ（削除された行は
    /// 読めない。タスク 2.4）が読む。シートごとの範囲は
    /// `range((sheet, from)..=(sheet, to))` で引ける。
    pub fn removed_rows(&self) -> &BTreeSet<(SheetId, RowId)> {
        &self.removed
    }

    /// その行がこの実行で削除されるか。
    pub fn is_row_removed(&self, sheet: SheetId, row: RowId) -> bool {
        self.removed.contains(&(sheet, row))
    }

    /// 書き換えるセルの件数（**同じセルへ 2 回書いても 1 件**。後ろの値だけが文書へ届く）。
    pub fn cell_count(&self) -> usize {
        self.sets.len()
    }

    /// 追加する行数の合計（要件 5.5。一括の 10 万行は 10 万である）。
    pub fn inserted_row_count(&self) -> usize {
        self.inserts.iter().map(|insert| insert.values.len()).sum()
    }

    /// 削除する行数（**行の集合の大きさ**。同じ行の繰り返しは 1 回に畳まれる）。
    pub fn removed_row_count(&self) -> usize {
        self.removed.len()
    }

    /// 複製する行数の合計（同じ行の繰り返しは繰り返しの数だけ数える。複製は行を減らさない）。
    pub fn duplicated_row_count(&self) -> usize {
        self.duplicates
            .iter()
            .map(|duplicate| duplicate.rows.len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_format::IdFactory;

    /// テスト用の識別子を発行する（本物と同じ規則: 連続発行は厳密昇順）。
    fn identifiers() -> IdFactory {
        IdFactory::default()
    }

    /// 整数のセル値（テストの値は整数で足りる）。
    fn cell(value: i64) -> CellValue {
        CellValue::Int(value)
    }

    /// 1 セルへの書き込み（列は 0 起点）。
    fn write(row: RowId, column: usize, value: i64) -> CellWrite {
        CellWrite::new(row, ColumnIndex::new(column), cell(value))
    }

    /// 件数の口を 1 つの 4 つ組にする（エンジン層が `ChangeSummary` を組むのと同じ 4 つ）。
    fn counts(changes: &ChangeSet) -> (usize, usize, usize, usize) {
        (
            changes.cell_count(),
            changes.inserted_row_count(),
            changes.removed_row_count(),
            changes.duplicated_row_count(),
        )
    }

    /// **同じセルへの複数の書き込みは後ろが残る**（規則 1）。途中の値はどこにも残らない。
    #[test]
    fn the_last_write_to_a_cell_wins() {
        let mut ids = identifiers();
        let sheet = ids.new_sheet_id();
        let row = ids.new_row_id();

        let mut changes = ChangeSet::new();
        changes
            .stage(Change::SetCells {
                sheet,
                writes: vec![write(row, 0, 1), write(row, 1, 3), write(row, 0, 2)],
            })
            .expect("書き込みを拒んだ");

        assert_eq!(
            Some(&cell(2)),
            changes.cell_value(sheet, row, ColumnIndex::new(0)),
            "同じセルの前の値が残った"
        );
        assert_eq!(
            Some(&cell(2)),
            changes
                .cell_writes()
                .get(&(sheet, row, ColumnIndex::new(0))),
            "解決済みの写像に最後の値が無い"
        );
        // セルごとに 1 件であり、同じセルへ 2 回書いても 2 件にはならない。
        assert_eq!(2, changes.cell_count());
        assert_eq!(2, changes.change_count());
    }

    /// **適用の順序は 追加 → 値 → 削除**（規則 2）。呼び出しの順ではない。
    #[test]
    fn the_application_order_is_inserts_then_values_then_removes() {
        let mut ids = identifiers();
        let sheet = ids.new_sheet_id();
        let kept = ids.new_row_id();
        let doomed = ids.new_row_id();

        let mut changes = ChangeSet::new();
        // わざと呼び出しを逆順にする（順序が規則で決まることを見る）。
        changes
            .stage(Change::RemoveRows {
                sheet,
                rows: vec![doomed],
            })
            .expect("削除を拒んだ");
        changes
            .stage(Change::SetCells {
                sheet,
                writes: vec![write(kept, 1, 7)],
            })
            .expect("書き込みを拒んだ");
        changes
            .stage(Change::DuplicateRows {
                sheet,
                rows: vec![kept],
            })
            .expect("複製を拒んだ");
        changes
            .stage(Change::InsertRows {
                sheet,
                values: vec![vec![cell(1)]],
            })
            .expect("追加を拒んだ");

        let mut kinds = Vec::new();
        for change in changes.application_order() {
            match change {
                StagedChange::Insert(insert) => {
                    assert_eq!(sheet, insert.sheet);
                    assert_eq!(1, insert.values.len());
                    kinds.push("insert");
                }
                StagedChange::Duplicate(duplicate) => {
                    assert_eq!(sheet, duplicate.sheet);
                    assert_eq!(vec![kept], duplicate.rows);
                    kinds.push("duplicate");
                }
                StagedChange::SetCells {
                    sheet: target,
                    row,
                    column,
                    value,
                } => {
                    assert_eq!((sheet, kept, ColumnIndex::new(1)), (target, row, column));
                    assert_eq!(&cell(7), value);
                    kinds.push("set");
                }
                StagedChange::Remove(remove) => {
                    assert_eq!(sheet, remove.sheet);
                    assert_eq!(vec![doomed], remove.rows);
                    kinds.push("remove");
                }
            }
        }
        assert_eq!(vec!["insert", "duplicate", "set", "remove"], kinds);
    }

    /// **10 万行の一括は 1 つの変更である**（規則 3）。要素は複製せずに移す。
    #[test]
    fn a_batch_of_a_hundred_thousand_rows_is_one_change() {
        let mut ids = identifiers();
        let sheet = ids.new_sheet_id();

        let values: Vec<Vec<CellValue>> = (0..100_000)
            .map(|index| {
                vec![
                    CellValue::Text(format!("行 {index}")),
                    cell(index),
                    CellValue::Bool(true),
                ]
            })
            .collect();
        // 移すだけなら文字列の確保は同じ位置のままである（複製すれば別の位置になる）。
        let CellValue::Text(first) = &values[0][0] else {
            panic!("テストの値が文字列でない");
        };
        let moved_from = first.as_ptr();

        let mut changes = ChangeSet::new();
        changes
            .stage(Change::InsertRows { sheet, values })
            .expect("一括の追加を拒んだ");

        assert_eq!(1, changes.change_count(), "一括が複数の変更に割れた");
        assert_eq!(1, changes.inserts().len());
        assert_eq!(100_000, changes.inserted_row_count());
        assert_eq!(
            0,
            changes.removed_row_count() + changes.duplicated_row_count()
        );
        let CellValue::Text(moved) = &changes.inserts()[0].values[0][0] else {
            panic!("集約した値が文字列でない");
        };
        assert_eq!(moved_from, moved.as_ptr(), "一括の値が複製された");
    }

    /// 削除した行は**適用時に存在しない**ため、書き込みと複製は理由つきで拒む（要件 5.4）。
    /// 拒んだ呼び出しは 1 つも残らない（規則 4）。
    #[test]
    fn a_removed_row_cannot_be_written_to_or_duplicated() {
        let mut ids = identifiers();
        let sheet = ids.new_sheet_id();
        let kept = ids.new_row_id();
        let doomed = ids.new_row_id();

        let mut changes = ChangeSet::new();
        changes
            .stage(Change::RemoveRows {
                sheet,
                rows: vec![doomed],
            })
            .expect("削除を拒んだ");

        let removed = ChangeError::RemovedRow { sheet, row: doomed };
        // 並びの一部だけが不正な場合も、その呼び出しの分は 1 つも残らない。
        assert_eq!(
            Err(removed.clone()),
            changes.stage(Change::SetCells {
                sheet,
                writes: vec![write(kept, 0, 1), write(doomed, 1, 2)],
            })
        );
        assert_eq!(
            None,
            changes.cell_value(sheet, kept, ColumnIndex::new(0)),
            "拒んだ呼び出しの先頭の書き込みが残った"
        );
        assert_eq!(
            Err(removed.clone()),
            changes.stage(Change::DuplicateRows {
                sheet,
                rows: vec![doomed],
            })
        );
        assert_eq!((0, 0, 1, 0), counts(&changes));
        assert_eq!(1, changes.change_count());

        // 追加は識別子を持たない行を作るため、削除の後でも通る（実在を要さない唯一の変更）。
        assert!(changes
            .stage(Change::InsertRows {
                sheet,
                values: vec![vec![cell(1)]],
            })
            .is_ok());
    }

    /// 同じ行を 2 度削除するのは**1 回の削除**である（上流 `document-format` の
    /// `RowRemovalError` と同じ規約。畳まれる）。
    #[test]
    fn removing_the_same_row_twice_collapses_to_one_removal() {
        let mut ids = identifiers();
        let sheet = ids.new_sheet_id();
        let row = ids.new_row_id();

        let mut changes = ChangeSet::new();
        changes
            .stage(Change::RemoveRows {
                sheet,
                rows: vec![row, row],
            })
            .expect("同じ並びの中の繰り返しを拒んだ");
        changes
            .stage(Change::RemoveRows {
                sheet,
                rows: vec![row],
            })
            .expect("2 度目の削除を拒んだ");

        assert_eq!(1, changes.removed_row_count(), "同じ行を 2 回数えた");
        assert_eq!(1, changes.removed_rows().len());
        assert!(changes.is_row_removed(sheet, row));
        // 呼び出しは 2 件のままである（件数は呼び出し数であって行数ではない）。
        assert_eq!(2, changes.change_count());
    }

    /// 空の並びは記録しない（規則 5）。`is_empty` と件数が食い違わない。
    #[test]
    fn empty_batches_record_nothing() {
        let mut ids = identifiers();
        let sheet = ids.new_sheet_id();

        let mut changes = ChangeSet::new();
        for change in [
            Change::SetCells {
                sheet,
                writes: Vec::new(),
            },
            Change::InsertRows {
                sheet,
                values: Vec::new(),
            },
            Change::RemoveRows {
                sheet,
                rows: Vec::new(),
            },
            Change::DuplicateRows {
                sheet,
                rows: Vec::new(),
            },
        ] {
            changes.stage(change).expect("空の並びを拒んだ");
        }

        assert!(changes.is_empty());
        assert_eq!(0, changes.change_count());
        assert_eq!((0, 0, 0, 0), counts(&changes));
    }

    /// 件数は種別ごとに数える（要件 5.5）。**同じセルは 1 件**である。
    #[test]
    fn counts_are_kept_per_kind() {
        let mut ids = identifiers();
        let sheet = ids.new_sheet_id();
        let first = ids.new_row_id();
        let second = ids.new_row_id();

        let mut changes = ChangeSet::new();
        changes
            .stage(Change::InsertRows {
                sheet,
                values: vec![vec![cell(1)], vec![cell(2)]],
            })
            .expect("追加を拒んだ");
        changes
            .stage(Change::SetCells {
                sheet,
                writes: vec![write(first, 0, 1), write(first, 0, 2), write(second, 1, 3)],
            })
            .expect("書き込みを拒んだ");
        changes
            .stage(Change::DuplicateRows {
                sheet,
                rows: vec![first],
            })
            .expect("複製を拒んだ");
        changes
            .stage(Change::RemoveRows {
                sheet,
                rows: vec![first, second],
            })
            .expect("削除を拒んだ");

        assert_eq!((2, 2, 2, 1), counts(&changes));
        assert!(!changes.is_empty());
    }

    /// この集合が削除していない行は受け付ける — 文書にあるかどうかは**ここでは判定できない**
    /// （エンジンは文書を知らない。design.md 決定 2）。文書に無い行の拒否はホストの縫い目
    /// （タスク 4.1）が行い、適用の直前の照合（タスク 4.2）がそれを裏付ける。
    #[test]
    fn rows_this_change_set_never_touched_are_accepted_here() {
        let mut ids = identifiers();
        let sheet = ids.new_sheet_id();
        let stranger = ids.new_row_id();

        let mut changes = ChangeSet::new();
        assert!(changes
            .stage(Change::SetCells {
                sheet,
                writes: vec![write(stranger, 0, 1)],
            })
            .is_ok());
        assert_eq!(1, changes.cell_count());
    }
}

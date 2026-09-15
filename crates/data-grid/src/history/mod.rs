//! 取り消し履歴: 命令と逆命令の対を積む [`UndoStack`]（data-grid のタスク 4.1。要件 6.6, 7.6,
//! 9.1, 9.2, 9.5, 9.6, 9.7）。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本層は左の `types` / `edit` を
//! 参照する**（design.md「内部の依存の向き」。層の鎖の文言を各層の冒頭に置く規約は
//! `structure.md`「ドメインクレートの内部構造」）。履歴の**材料の型**
//! （[`HistoryCommand`] / [`HistoryPair`](crate::edit::HistoryPair) / 復元の材料の型
//! [`RestoredRow`](crate::edit::RestoredRow)）は `edit` 層にある — 適用の経路
//! （`edit` 層の `EditApply`）がその型を名指すためであり、本層に置くと鎖が閉じない
//! （逆向きの参照になる）。
//! 本層が持つのは**積む側**（履歴そのものとその区分）である。
//!
//! # 逆命令は適用時にしか作れない（本タスクの主題）
//!
//! 1 つの編集を適用すると、変更前の値も、取り除かれた行も、その位置も消える。したがって
//! 逆命令は**適用の前に読んだ材料**から組み立てるしかなく（要件 9.1）、その組み立ては
//! `edit` 層の `EditApply::apply_with_inverse` が適用と同じ本体の中で行う（tasks.md 4.1）。
//! 本層は**その対を積むだけ**である:
//!
//! ```text
//! let (outcome, pair) = apply.apply_with_inverse(&mut document, command)?;
//! if let Some(pair) = pair {
//!     stack.push(UndoEntry { label: UndoLabel::of_edit(&command), inverse: pair.inverse, redo: pair.redo });
//! }
//! ```
//!
//! **状態を変えなかった適用は対を持たない**（`pair` は `None`）。積まないのは、取り消しが
//! 何も戻さない操作を履歴へ入れないためである（`edit` 層のモジュール docs「空の命令」）。
//!
//! # 履歴はドキュメント単位であり、シートごとではない（要件 9.5）
//!
//! 1 つの履歴が**1 つのドキュメント**の操作を持つ。シートごとに分けない。理由は
//! `macro-runtime` のマクロの実行が**複数のシートを跨ぐ 1 つの操作**になりうることであり、
//! 履歴がシートで分かれているとその操作を 1 つの対として表せない。
//!
//! したがって本層は**シート識別子を持たない**（履歴の入れ物にシートの欄が無い）。材料が
//! シートを名乗るのは、取り消しを**どのシートへ適用するか**を材料自身が知るためである
//! （`edit` 層の `EditApply::apply_history` の docs「復元の材料が名乗るシート」）。
//!
//! **所有者は誰か。** 履歴はドキュメントを保持する側が持つ — design.md の
//! `GridSession`（タスク 5.2）が `{ view, order, history, schema }` を持つ。本層は
//! 履歴の**入れ物**だけを定め、ドキュメントを持たない（適用は `edit` 層の仕事である）。
//!
//! # 登録口は `push` ただ 1 つ（要件 9.1 の拡張点）
//!
//! 本タスクが積むのは `edit` 層の 5 つの命令（セルの編集・行の追加・削除・複製・貼り付け）
//! である。**それ以外に履歴を増やす口を本層は持たない** — 状態はすべて非公開であり、公開して
//! いる変更の口は [`UndoStack::push`] だけである。後続のスペック（`formula-engine` の数式の
//! 再計算、`macro-runtime` のマクロの実行）は**同じ 1 つの `push`** から自分の対を積む。
//!
//! そのために [`UndoLabel`] は 2 つの区分を**先に**持つ（本タスクは生成しない）:
//! [`UndoLabel::Recalculation`] と [`UndoLabel::MacroRun`]。区分が先にあることで、後続の
//! スペックは**履歴の内部構造に触れずに**乗れる（design.md「UndoStack（拡張点の所有者）」の
//! Implementation Notes）。区分は「その操作が何であったか」を表す札であり、取り消しの
//! **意味論には関与しない**（取り消しが適用するのは逆命令そのものである）。
//!
//! # 取り消しとやり直しは位置を動かすだけ（4.2 への申し送り）
//!
//! [`UndoStack::undo`] / [`UndoStack::redo`] は**対を返すだけ**であり、ドキュメントへは
//! 何もしない（適用するのは操作口の仕事である）。これにより本層は `Document` を持たず、
//! 取り消しの意味論（何が戻るか）は `edit` 層の 1 か所に留まる。
//!
//! 本タスクが実装するのは次の 2 つである:
//!
//! - **位置を動かすこと**。`undo` は 1 つ前の対の逆命令を返し、`redo` は 1 つ後の対の
//!   やり直しの命令を返す。並びの端では `None` を返す（失敗ではない）。
//! - **積んだ対の件数**（[`UndoStack::depth`]）。
//!
//! タスク 4.2 が担うのは、**上限による追い出し**（[`UndoStack::new`] が受け取る `limit`。
//! 本タスクは値を持つだけであり、[`UndoStack::limit`] で読める）と、取り消し・やり直しの
//! **結果**（影響を受けた行の報告）である。本タスクは 4.2 を妨げない形にしてある —
//! 追い出しは「古い側を捨てる」1 か所の追加であり、位置の規律（下の不変条件）は
//! そのまま使える。
//!
//! # 不変条件
//!
//! - **`push` は `cursor` 以降のやり直しの対象を破棄する**（要件 9.4）。取り消した後に新しい
//!   操作を積むと、取り消した対はもうやり直せない。これが無いと、積んだ対が指す位置が
//!   一意にならず、取り消し済みの対がもう一度取り消されて同じ操作が 2 回適用されうる。
//! - **`depth` は積んだ件数であり、位置に依らない**。取り消しは件数を減らさない
//!   （「いま何番目か」と「何件積んだか」は別である）。
//! - **同じ文書と同じ命令の並びは同じ形の履歴を与える**（決定性）。対の種類・区分・規模は
//!   命令の並びで決まる（材料に含まれる識別子は発行のたびに変わるため、形の比較には
//!   含めない。標本の契約は `tests/common/sample.rs` のモジュール docs にある）。

// 材料の型は `edit` 層にある（本層はそれを使う。層の鎖は左向きの一方向である）。
use crate::edit::{EditCommand, HistoryCommand};

/// 取り消し履歴が積む 1 件（命令と逆命令の対。design.md「UndoStack」の Service Interface）。
///
/// 対の**材料**は `edit` 層の [`HistoryPair`](crate::edit::HistoryPair) が適用の時に作る。本型はそれへ**区分**
/// （[`UndoLabel`]）を足したものである — 区分は提示（何の操作だったか）のためのものであり、
/// 取り消しの意味論は材料だけが決める。
///
/// 3 つの欄はすべて公開している。境界の適応層（6.x）が区分を提示へ写すためと、後続の
/// スペックが `..` 記法で自分の対を組み立てるためである（新しい登録口を増やさない）。
#[derive(Debug, Clone, PartialEq)]
pub struct UndoEntry {
    /// この操作の区分（提示用）。
    pub label: UndoLabel,
    /// 適用前の状態へ戻す命令。
    pub inverse: HistoryCommand,
    /// 適用後の状態へ進める命令。
    pub redo: HistoryCommand,
}

/// 1 件の操作の区分（design.md「UndoStack」の Service Interface。要件 9.1, 9.7）。
///
/// 5 つの編集命令に対応する 5 つと、**後続のスペックのための 2 つ**から成る。後ろの 2 つは
/// 本タスクでは生成されない — 先に場所を空けてある（拡張点は所有者が形を決める、
/// `structure.md`）。「場所を空ける」とは、後続のスペックが**本層の内部構造に触れずに**
/// 自分の対を積めるということである（[`UndoStack::push`] が唯一の登録口）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoLabel {
    /// セルの編集（`SetCells` / `SetNested`）。
    CellEdit,
    /// 行の追加（`InsertRows`）。
    RowInsert,
    /// 行の削除（`RemoveRows`）。
    RowRemove,
    /// 行の複製（`DuplicateRows`）。
    RowDuplicate,
    /// 貼り付け（`PasteRange`。**1 回の貼り付けは 1 つの操作である**。要件 7.6）。
    Paste,
    /// 数式の再計算。**本タスクは生成しない**（`formula-engine` が同じ `push` から積む）。
    Recalculation,
    /// マクロの実行。**本タスクは生成しない**（`macro-runtime` が同じ `push` から積む）。
    MacroRun,
}

impl UndoLabel {
    /// 編集命令を区分へ写す（本タスクが積む 5 つ）。
    ///
    /// `SetCells` と `SetNested` はどちらも**セルの編集**である（値を書く形が違うだけで、
    /// 利用者から見た操作は同じである）。後続の 2 区分はここへは現れない — 編集命令では
    /// ないためである（`Recalculation` / `MacroRun` は `edit` 層の命令を持たない）。
    pub fn of_edit(command: &EditCommand) -> Self {
        match command {
            EditCommand::SetCells { .. } | EditCommand::SetNested { .. } => Self::CellEdit,
            EditCommand::InsertRows { .. } => Self::RowInsert,
            EditCommand::RemoveRows { .. } => Self::RowRemove,
            EditCommand::DuplicateRows { .. } => Self::RowDuplicate,
            EditCommand::PasteRange { .. } => Self::Paste,
        }
    }
}

/// 命令と逆命令の対を積む取り消し履歴（design.md「UndoStack」の Service Interface。要件 6.6,
/// 7.6, 9.1, 9.4, 9.5, 9.6, 9.7）。
///
/// **ドキュメント単位**であり、シートごとではない（要件 9.5。モジュール docs）。したがって
/// 本型はシート識別子を持たず、`Document` も持たない（適用は `edit` 層の仕事である）。
/// 保持するのは積んだ対と、**いまどこに居るか**（`cursor`）と、上限（`limit`）だけである。
///
/// 並びは [`Vec`] を使う（design.md の字面は `VecDeque` だが、本層は
/// [`UndoStack::entries`] で**積んだ対の並びを丸ごと読める**必要がある — 履歴の形を検査が
/// 突き合わせるためである。`Vec` なら借用の切片として返せる）。取り消し・やり直しは
/// **末尾側だけ**を触るため、両端の取り出しを要さない。
///
/// # 上限（`limit`）は値として持つだけである
///
/// 追い出しの**振る舞い**はタスク 4.2 が定める。本タスクは受け取った値を保持し、
/// [`UndoStack::limit`] で読めるようにする（0 を「無制限」とするか「積めない」とするかは
/// 4.2 の裁定である。本タスクはどちらにも倒れない）。
#[derive(Debug, Clone)]
pub struct UndoStack {
    /// 積んだ対（古い側が先頭）。
    entries: Vec<UndoEntry>,
    /// **次のやり直しの位置**。`cursor` 件が取り消し済みであり、`cursor..` がやり直しの対象で
    /// ある。`cursor == entries.len()` は「すべて適用済み」を意味する。
    cursor: usize,
    /// 積める件数の上限（4.2 が使う。本タスクは保持するだけ）。
    limit: usize,
}

impl UndoStack {
    /// 空の履歴を作る（`limit` は 4.2 が使う上限。本タスクは保持するだけ）。
    pub fn new(limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            limit,
        }
    }

    /// 対を 1 つ積む（**唯一の登録口**。要件 9.1 の拡張点）。
    ///
    /// 積むと**やり直しの対象は破棄される**（要件 9.4。モジュール docs「不変条件」）—
    /// 取り消した後に新しい操作を積んだとき、取り消した対がもう一度やり直されると、同じ
    /// 操作が 2 回適用される経路ができる。破棄するのは「いま居る位置より後ろ」だけであり、
    /// 取り消していない対（前にあるもの）は 1 つも失われない。
    ///
    /// 後続のスペック（`formula-engine` / `macro-runtime`）も**この 1 つの口**から積む。
    /// 区分（[`UndoLabel`]）が先に用意してあるため、履歴の内部構造に触れる必要は無い。
    pub fn push(&mut self, entry: UndoEntry) {
        // やり直しの対象（`cursor` 以降）を捨ててから積む（`cursor` の位置が「いま居る位置」
        // であるため、捨てる操作は `cursor` へ切り詰めることに等しい）。
        self.entries.truncate(self.cursor);
        self.entries.push(entry);
        self.cursor = self.entries.len();
    }

    /// 1 つ前の対の**逆命令**を返し、位置を 1 つ戻す（要件 9.2, 9.3）。
    ///
    /// 戻る対が無ければ `None` を返し、位置は動かさない（失敗ではない — 並びの端である）。
    /// **ドキュメントへは何もしない**（適用は操作口の仕事である。モジュール docs）。
    pub fn undo(&mut self) -> Option<&HistoryCommand> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        Some(&self.entries[self.cursor].inverse)
    }

    /// 1 つ後の対の**やり直しの命令**を返し、位置を 1 つ進める（要件 9.2, 9.3）。
    ///
    /// 進む対が無ければ `None` を返し、位置は動かさない。
    pub fn redo(&mut self) -> Option<&HistoryCommand> {
        if self.cursor >= self.entries.len() {
            return None;
        }
        let entry = &self.entries[self.cursor];
        self.cursor += 1;
        Some(&entry.redo)
    }

    /// 積んだ対の件数（要件 9.6 の「深さ」。**位置に依らない**）。
    ///
    /// 取り消し・やり直しは位置を動かすだけであり、この数を変えない（モジュール docs
    /// 「不変条件」）。
    pub fn depth(&self) -> usize {
        self.entries.len()
    }

    /// 積んだ対の並び（古い側が先頭）。
    ///
    /// 検査（`tests/undo_stack.rs`）が履歴の**形**を突き合わせるための読み口である
    /// （区分と材料の規模・位置のみを写し、識別子は写さない）。
    pub fn entries(&self) -> &[UndoEntry] {
        &self.entries
    }

    /// 積める件数の上限（[`UndoStack::new`] が受け取った値。追い出しは 4.2 が実装する）。
    pub fn limit(&self) -> usize {
        self.limit
    }
}

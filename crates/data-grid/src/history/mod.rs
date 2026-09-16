//! 取り消し履歴: 命令と逆命令の対を積む [`UndoStack`]（data-grid のタスク 4.1 と 4.2。要件 6.6,
//! 7.6, 9.1, 9.2, 9.3, 9.4, 9.5, 9.6, 9.7）。
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
//! **所有者は誰か。** 履歴はドキュメントを保持する側が持つ — **適応層のウィンドウの保持**
//! である（`src-tauri/src/commands/grid.rs` の `SheetEntry`。要件 9.5 は履歴が
//! **ドキュメント単位**であることを求め、操作口 [`GridSession`](crate::GridSession) は
//! シートごとに作り直されるため、履歴はその持ち物にできない。10.2 が所有者をここへ移した）。
//! [`GridSession`](crate::GridSession) は履歴を所有せず、文書を触る 3 つの経路
//! （`apply` / `undo` / `redo`）が `&mut UndoStack` を受け取る。本層は履歴の**入れ物**だけを
//! 定め、ドキュメントを持たない（適用は `edit` 層の仕事である）。
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
//! # 取り消しとやり直しをドキュメントへ適用する（タスク 4.2）
//!
//! [`UndoStack::undo`] / [`UndoStack::redo`] は**位置を動かして命令を返すだけ**であり、
//! ドキュメントへは何もしない。適用するのは [`UndoRedo`] である — 履歴と適用の経路
//! （`edit` 層の `EditApply`）を**借用の対**として束ね、位置の移動と適用を 1 つの口で
//! 行う。これにより取り消しの意味論（何が戻るか）は `edit` 層の 1 か所に留まったままで
//! あり、履歴は `Document` を所有しない。
//!
//! 取り消し・やり直しの**結果は [`EditOutcome`]** であり、**影響を受けた行**
//! （`affected`）を運ぶ（要件 9.2, 9.3 の「結果」）。何も取り消せないときは `Ok(None)` を
//! 返す（失敗ではない — 並びの端である）。
//!
//! **適用が失敗したときは位置を動かさない。** [`UndoRedo`] は位置を動かす**前に**命令を読み
//! （`UndoStack` の内部の読み口である）、適用が成功してはじめて位置を進める。古い対（対象の
//! 行がもう無い等）で失敗したとき、位置が動かずドキュメントも変わらない状態を残さないためで
//! ある。これは `UndoRedo::undo` と `UndoRedo::redo` の**両方**について検査で固定してある
//! （片方だけでは、失敗したときに次の操作を飛び越して適用する実装を見逃す。
//! `tests/undo_stack.rs` の `a_failed_undo_leaves_the_position_unchanged` と
//! `a_failed_redo_leaves_the_position_unchanged`）。
//!
//! **取り消しとやり直しは履歴へ積まれない。** 積むと「取り消しの取り消し」になり、操作が
//! 2 回適用される経路ができる。位置を動かすだけであり、[`UndoStack::depth`] は前後で
//! 変わらない（[`UndoStack::push`] が唯一の登録口である）。
//!
//! # 上限（`limit`）は古い側から捨てる（要件 9.6）
//!
//! [`UndoStack::push`] は積んだ結果が上限を超えるとき、**古い側から**捨てて上限に収める。
//! 上限 **0 は「1 件も保持しない」** — 追い出しの一般の規則がそのまま働く（1 件積めば直ちに
//! 上限を超えるので、その 1 件が捨てられる。専用の分岐は無い）。「無制限」に 0 を負わせない
//! のは、10 万行を扱う道具で**有界でない記憶の伸びる経路**が既定で開く意味になるためであり、
//! 無制限が要るなら上限を持たない型（`Option<usize>` の `None`）が正しい表現である。
//!
//! # 不変条件
//!
//! - **`push` は `cursor` 以降のやり直しの対象を破棄する**（要件 9.4）。取り消した後に新しい
//!   操作を積むと、取り消した対はもうやり直せない。これが無いと、積んだ対が指す位置が
//!   一意にならず、取り消し済みの対がもう一度取り消されて同じ操作が 2 回適用されうる。
//!   この破棄は**追い出しより先**である（捨てるのは「いま居る位置より後ろ」であって、
//!   上限を超えた古い側ではない）。
//! - **`cursor` は「次に積む位置」と「取り消し済みの件数」を同時に表す**。`entries[..cursor]`
//!   が適用済み（取り消しの対象）であり、`entries[cursor..]` がやり直しの対象である。
//!   **追い出しは古い側を捨てるので、位置も同じだけ手前へ寄せる** — 寄せなければ取り消しが
//!   捨てた対の位置を指し、**保持している対を飛ばす**（上限 2 で 3 件積んだ後、1 回目の
//!   取り消しが 3 件目、2 回目が 2 件目の逆命令を返す、という帰結がこの不変条件である）。
//! - **`depth` は積んだ件数であり、位置に依らない**。取り消し・やり直しは件数を減らさない
//!   （「いま何番目か」と「何件積んだか」は別である）。**件数が変わるのは `push` と、その
//!   中の追い出しだけである**。
//! - **同じ文書と同じ命令の並びは同じ形の履歴を与える**（決定性）。対の種類・区分・規模は
//!   命令の並びで決まる（材料に含まれる識別子は発行のたびに変わるため、形の比較には
//!   含めない。標本の契約は `tests/common/sample.rs` のモジュール docs にある）。

// 材料の型は `edit` 層にある（本層はそれを使う。層の鎖は左向きの一方向である）。
use crate::edit::{EditApply, EditCommand, EditOutcome, HistoryCommand};
use crate::error::GridError;
use crate::view::RowOrder;

use document_format::Document;

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
/// # 上限（`limit`）は古い側から捨てる（タスク 4.2。要件 9.6）
///
/// [`UndoStack::push`] が、積んだ結果が上限を超えるときに**古い側から**捨てる。上限 **0 は
/// 「1 件も保持しない」** である（追い出しの一般の規則がそのまま働く — 専用の分岐は無い。
/// モジュール docs「上限は古い側から捨てる」）。
#[derive(Debug, Clone)]
pub struct UndoStack {
    /// 積んだ対（古い側が先頭）。
    entries: Vec<UndoEntry>,
    /// **次のやり直しの位置**。`cursor` 件が取り消し済みであり、`cursor..` がやり直しの対象で
    /// ある。`cursor == entries.len()` は「すべて適用済み」を意味する。
    cursor: usize,
    /// 積める件数の上限（0 は「1 件も保持しない」）。
    limit: usize,
}

impl UndoStack {
    /// 空の履歴を作る（`limit` は保持する対の件数の上限。**0 は「1 件も保持しない」**）。
    pub fn new(limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            limit,
        }
    }

    /// 対を 1 つ積む（**唯一の登録口**。要件 9.1 の拡張点。要件 9.4, 9.6）。
    ///
    /// 積むと**やり直しの対象は破棄される**（要件 9.4。モジュール docs「不変条件」）—
    /// 取り消した後に新しい操作を積んだとき、取り消した対がもう一度やり直されると、同じ
    /// 操作が 2 回適用される経路ができる。破棄するのは「いま居る位置より後ろ」だけであり、
    /// 取り消していない対（前にあるもの）は 1 つも失われない。
    ///
    /// そのうえで、**積んだ件数が上限を超えていれば古い側から捨てる**（要件 9.6）。上限 0 では
    /// いま積んだ 1 件が直ちに捨てられ、履歴は空のままである（専用の分岐を置かない）。
    /// **位置（`cursor`）も捨てた件数だけ手前へ寄せる** — 寄せなければ取り消しが捨てた対の
    /// 位置を指し、保持している対を飛ばす（モジュール docs「不変条件」）。
    ///
    /// 後続のスペック（`formula-engine` / `macro-runtime`）も**この 1 つの口**から積む。
    /// 区分（[`UndoLabel`]）が先に用意してあるため、履歴の内部構造に触れる必要は無い。
    pub fn push(&mut self, entry: UndoEntry) {
        // やり直しの対象（`cursor` 以降）を捨ててから積む（`cursor` の位置が「いま居る位置」
        // であるため、捨てる操作は `cursor` へ切り詰めることに等しい）。**追い出しより先**で
        // ある — 捨てるのは「いま居る位置より後ろ」であって、上限を超えた古い側ではない。
        self.entries.truncate(self.cursor);
        self.entries.push(entry);
        self.cursor = self.entries.len();
        // 上限を超えた分を古い側から捨てる（`limit == 0` なら、いま積んだ 1 件がここで消える）。
        let excess = self.entries.len().saturating_sub(self.limit);
        if excess > 0 {
            self.entries.drain(..excess);
            // 位置は「捨てた件数」だけ手前へ寄せる（`drain` の後も `cursor` は
            // 「次に積む位置」＝ `entries.len()` である — 積んだ直後だからである）。
            self.cursor = self.entries.len();
        }
    }

    /// 1 つ前の対の**逆命令**を返し、位置を 1 つ戻す（要件 9.2, 9.3）。
    ///
    /// 戻る対が無ければ `None` を返し、位置は動かさない（失敗ではない — 並びの端である）。
    /// **ドキュメントへは何もしない**（適用は [`UndoRedo`] の仕事である）。
    ///
    /// **件数（[`UndoStack::depth`]）を変えない** — 位置を動かすだけである（要件 9.2 の
    /// 「取り消しは履歴へ積まれない」。モジュール docs「不変条件」）。
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

    /// 次に取り消す対の**逆命令**を、**位置を動かさずに**読む（タスク 4.2）。
    ///
    /// [`UndoRedo`] が使う。適用はドキュメントを変える操作であり、**失敗しうる**
    /// （古い対は対象の行をもう持たない）。先に読んで適用し、成功してから
    /// [`UndoStack::undo`] で位置を進めることで、失敗したときに位置が動かない。
    ///
    /// 位置を動かさないので、`peek_undo` のあと `undo` を呼べば同じ命令が返る。
    /// **公開しない** — 公開の面を広げないためである（登録口は `push` ただ 1 つであり、
    /// 本メソッドは読み口にすぎない。モジュール docs「登録口は `push` ただ 1 つ」）。
    pub(crate) fn peek_undo(&self) -> Option<&HistoryCommand> {
        if self.cursor == 0 {
            return None;
        }
        Some(&self.entries[self.cursor - 1].inverse)
    }

    /// 次にやり直す対の**命令**を、**位置を動かさずに**読む（タスク 4.2）。
    ///
    /// [`UndoStack::peek_undo`] と同じ理由で在る（[`UndoRedo`] が使い、公開しない）。
    pub(crate) fn peek_redo(&self) -> Option<&HistoryCommand> {
        self.entries.get(self.cursor).map(|entry| &entry.redo)
    }

    /// 積んだ対の件数（要件 9.6 の「深さ」。**位置に依らない**）。
    ///
    /// 取り消し・やり直しは位置を動かすだけであり、この数を変えない（モジュール docs
    /// 「不変条件」）。**変わるのは [`UndoStack::push`]（と上限による追い出し）だけである**。
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

    /// 積める件数の上限（[`UndoStack::new`] が受け取った値。**0 は「1 件も保持しない」**）。
    pub fn limit(&self) -> usize {
        self.limit
    }
}

/// 履歴と適用の経路を束ね、取り消しとやり直しを**ドキュメントへ適用する**口
/// （design.md「UndoStack」の Service Interface。要件 9.2, 9.3）。
///
/// # 層の鎖を保つ形
///
/// `history` 層は `edit` 層を参照してよい（`error / types → view → edit → history →
/// transport → api`）。したがって本型は**`history` 層に置ける** — 適用の経路
/// （[`EditApply`]）は左の層の型であり、本層がそれを使うのは鎖の向きのままである。
/// 逆向き（`edit` が `history` を名指す）は起きない。
///
/// 束ねるのは**借用**である（`&mut UndoStack` と `&mut EditApply`）。所有しないので、
/// 履歴の所有者は自分の欄をそのまま渡せる — 所有者は**適応層のウィンドウの保持**である
/// （`src-tauri/src/commands/grid.rs` の `SheetEntry`。[`GridSession`](crate::GridSession) は
/// 履歴を所有せず、`apply` / `undo` / `redo` が `&mut UndoStack` を受け取る。10.2）。
/// `UndoStack` が `Document` を持たずに済む性質（モジュール docs）も保たれる — ドキュメントは
/// 呼び出しごとに受け取る。
///
/// 呼び出しの形:
///
/// ```text
/// let mut undo_redo = UndoRedo::new(history, &mut session.apply);  // history は保持の欄
/// if let Some(outcome) = undo_redo.undo(&mut document)? { /* 影響を受けた行は outcome.affected */ }
/// ```
///
/// # 取り消しとやり直しは履歴へ積まれない
///
/// 位置を動かすだけで、`push` を呼ばない（呼べば「取り消しの取り消し」になり、操作が 2 回
/// 適用される経路ができる）。したがって [`UndoStack::depth`] は前後で変わらない。
pub struct UndoRedo<'a> {
    /// 位置を動かす対象（履歴そのものは本型を跨いで生き続ける）。
    stack: &'a mut UndoStack,
    /// 命令をドキュメントへ適用する経路（`edit` 層）。
    apply: &'a mut EditApply,
}

impl<'a> UndoRedo<'a> {
    /// 履歴と適用の経路を束ねる。
    pub fn new(stack: &'a mut UndoStack, apply: &'a mut EditApply) -> Self {
        Self { stack, apply }
    }

    /// 直前の操作の**前**の状態へ戻し、**影響を受けた行**を結果として返す（要件 9.2）。
    ///
    /// 戻る操作が無ければ `Ok(None)` を返す（失敗ではない — 履歴の先頭である）。上限 0 の
    /// 履歴はつねに `Ok(None)` を返す（1 件も保持しない。モジュール docs）。
    ///
    /// 適用が失敗したときは**位置を動かさない** — 先に読んで（`UndoStack` の内部の読み口）
    /// 適用し、成功してから位置を進める。位置だけが動いてドキュメントが変わらない状態を
    /// 残さないためである。
    ///
    /// 結果（[`EditOutcome`]）の `affected` は**その操作が触れた行**である（セルの編集では
    /// 編集したセルの行、行の削除では取り除かれた行、貼り付けでは書いた行）。適用は
    /// `edit` 層の [`EditApply::apply_history`] が行い、本型は解釈を足さない。
    pub fn undo(
        &mut self,
        doc: &mut Document,
        order: &RowOrder,
    ) -> Result<Option<EditOutcome>, GridError> {
        // 位置を動かす**前に**読む（借りはこの呼び出しの間だけであり、適用の結果は命令を
        // 借りない — 適用が終われば借りは切れ、そのあとで位置を動かせる）。
        let Some(command) = self.stack.peek_undo() else {
            return Ok(None);
        };
        let outcome = self.apply.apply_history(doc, command, order)?;
        // 適用が成功したので位置を進める（`peek_undo` が読んだ対と同じものである）。
        self.stack.undo();
        Ok(Some(outcome))
    }

    /// 取り消した操作を**再び適用**し、**影響を受けた行**を結果として返す（要件 9.3）。
    ///
    /// やり直す操作が無ければ `Ok(None)` を返す。取り消しの後に新しい操作が積まれていれば、
    /// やり直しの対象は破棄されているため `None` である（要件 9.4。
    /// [`UndoStack::push`]）。
    ///
    /// 適用が失敗したときは**位置を動かさない**（[`UndoRedo::undo`] と同じ規律）。
    pub fn redo(
        &mut self,
        doc: &mut Document,
        order: &RowOrder,
    ) -> Result<Option<EditOutcome>, GridError> {
        let Some(command) = self.stack.peek_redo() else {
            return Ok(None);
        };
        let outcome = self.apply.apply_history(doc, command, order)?;
        self.stack.redo();
        Ok(Some(outcome))
    }
}

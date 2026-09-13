//! 1 つのセッションの状態機械（tasks.md 2.1。design.md「Slot」「Error Handling」
//! 「System Flows → 起動時に指定されたドキュメントの解決」。要件 1.2, 1.8, 2.1, 2.3, 2.5,
//! 4.4, 4.6, 7.1, 7.2, 7.3）。
//!
//! # 層の鎖（design.md「Architecture Pattern & Boundary Map」）
//!
//! 本モジュールは `error / state → session → change → table → api` の第 2 層である。
//! 参照するのは**左の層**（[`crate::error`] / [`crate::state`]）と上流 `document-format`
//! だけであり、`change` / `table` / `api` を知らない。
//!
//! # 状態機械
//!
//! 1 つの [`Slot`] が 1 つのウィンドウのドキュメントの寿命を持つ。内部状態は 3 つである:
//!
//! ```text
//! Unresolved ──(resolve: 生成要求の位置を 1 度だけ読む)──> Resolved { origin, document }
//! Unresolved ──(読み込みが失敗する)────────────────────> Unavailable { reason }
//! Resolved   ──(attach: 利用者が選んだ位置を読む)──────> Resolved { origin: File(…), … }
//! Unavailable──(attach: 利用者が選び直す)──────────────> Resolved { origin: File(…), … }
//! Resolved   ──(create: 未保存でなければ)──────────────> Resolved { origin: New, … }
//! Resolved   ──(edit: タスク 2.4 の変更の適用)──────────> Resolved（版 +1 / 未保存）
//! ```
//!
//! [`Inner::Unavailable`] は「読み込みに失敗したことを覚えている」状態である（design.md
//! 「Slot」の `Inner`）。覚えないと、同じ位置への読み込みを**問い合わせのたびに**試みることに
//! なる（起動時に指定された位置の読み込みは `document_state` の問い合わせが引き金になるため、
//! 壊れたファイルがあるたびに画面の再描画が秒単位の読み込みを繰り返す）。状態の写しの
//! [`SessionState::Unavailable`] と 1 対 1 に対応する。
//!
//! # 読み込みの 2 つの入口（要件 1.2, 1.3, 2.1, 2.2）
//!
//! - [`Slot::resolve`]（**起動時に指定された位置**）: **冪等**である。`Resolved` に対する
//!   2 度目のアクセスは何もしない（読み込みを起こさない）。読み込みを試みるのは `Unresolved`
//!   からだけで、`Unavailable` からは試みない（覚えた失敗を問い合わせのたびに繰り返さない）
//! - [`Slot::attach`]（**利用者が選んだ位置**）: 利用者の明示の操作であるため、`Resolved` /
//!   `Unavailable` のどちらからでも読み直して出所を更新する。未保存なら拒否し（要件 2.2）、
//!   読み込みに失敗しても保持している文書を変えない（要件 2.1）
//!
//! どちらも**読み込みに失敗しても保持している内容を変えない**。何も保持していなかった場合は
//! [`Inner::Unavailable`] として理由を覚える。
//!
//! # 未保存と版（要件 4.1, 4.4, 4.6）
//!
//! - **未保存**の印は「内容が変わりうる操作を受けた」ことの記録である。読み込み（resolve /
//!   attach）と新規作成の完了で落ち（要件 4.4）、保存の成功（タスク 2.2）と [`Slot::discard`]
//!   でも落ちる
//! - **版**は**文書が入れ替わるか変更が適用されたときに 1 進む**（読み込みの完了・新規作成・
//!   変更の適用。design.md「Slot」の不変条件）。下流は版の変化で「自分が見たあとに変わった」
//!   ことを知る。内容が入れ替わったのに据え置かれると、下流の窓が古い内容を表示し続ける
//! - **判定と更新は同じ臨界区間で行う**: 未保存の判定（差し替えを拒むか）と、文書の差し替え・
//!   印・版の更新を、文書のロックを**保持したまま**行う。でなければ「未保存でない」と判定した
//!   直後に別の経路が変更を適用し、その変更が差し替えで黙って失われる
//!
//! # セッションを作る経路（要件 1.2, 7.1）
//!
//! 本モジュールが与える入口は 3 つである: [`Slot::resolve`]（起動時に指定された位置。`Some` の
//! ときだけ読み込む）・[`Slot::attach`]（利用者が選んだ位置の引き渡し）・[`Slot::create`]
//! （新規作成）。読み取り・可変の借用・破棄の印は**未解決のままなら失敗を返す**
//! （[`SessionError::NoDocument`]）。生成要求の位置を読めるのは呼び出し元（適応層）だけであり、
//! この一意性が破棄の購読を伴わないセッションが生まれる経路を塞ぐ（design.md
//! 「DocumentSessions」の Preconditions）。
//!
//! # ロックの規律（要件 2.5, 3.4）
//!
//! 待つ操作と待たない操作を分ける:
//!
//! - **待つ**: [`Slot::read`] / [`Slot::with_document_mut`] / [`Slot::state`] /
//!   [`Slot::discard`] は文書のロックを待って取る。適用中に到着した読み取りには適用済みの
//!   最新を返す（要件 3.4）
//! - **待たない**: [`Slot::resolve`] / [`Slot::attach`] / [`Slot::create`] は `Mutex::try_lock`
//!   を使い、別の操作が進行中なら [`SessionError::Busy`] で**即座に**失敗する（要件 2.3）
//! - **ロックを取らない**: [`Slot::may_close`] は未保存の原子値だけを読む。適用や読み込みが
//!   進行中でも待たない（要件 2.5）
//!
//! 未保存（`AtomicBool`）と変更の版（`AtomicU64`）を文書のロックの外に置くのは、
//! `may_close` がロックを待たずに答えられるようにするためである（design.md「Slot」）。
//!
//! # 再入禁止
//!
//! [`Slot::read`] / [`Slot::with_document_mut`] の閉包の内側から**同じセッションを呼び返しては
//! ならない**。`Mutex` は再入できないため、文書のロックを取る操作（[`Slot::read`] /
//! [`Slot::with_document_mut`] / [`Slot::state`] / [`Slot::discard`] と、タスク 2.2 以降の
//! 保存）を呼ぶとデッドロックする。例外は [`Slot::may_close`] であり、文書のロックを取らない
//! ため閉包の内側からでも安全である。
//!
//! # 書き込むが、記録しない
//!
//! [`Slot::with_document_mut`] は文書を可変で貸すが、**未保存の印と版を変えない**。
//! 適用の記録（版を 1 進め、未保存の印を立てる）は変更の適用口（タスク 2.4 の `change`）の
//! 責務である。
//!
//! そのため `session` は、ロックを**保持したまま** `Inner` を貸す [`Slot::lock_for_change`] と、
//! ロック保持中の記録 [`Slot::record_edit`] を与える。`with_document_mut` は Guard を返さない
//! （関数を抜けると解放する）ため、記録を閉包と同じ臨界区間に入れられない。判定（文書を保持して
//! いるか）・閉包の実行・記録を分けないために、Guard を呼び出し元へ貸す口が要る。
//!
//! # 保存の 2 経路（要件 5.1, 5.4, 5.5, 5.7, 5.8）
//!
//! [`Slot::save`]（出所の位置へ）と [`Slot::save_to`]（選ばれた位置へ書き出し、以後の出所に
//! する）の 2 つである。どちらも**文書のロックの下で 1 パス**として書き出す: 書き出しの間は
//! ロックを保持するため、進行中に到着した適用は書き出しの完了を待ち、**書き出した内容と
//! 保持している内容が食い違わない**（要件 5.5）。書き出す内容と形式は形式の側
//! （[`DocumentFormatApi::save`]）に委ね、本クレートはファイルを組み立てない（要件 5.7）。
//!
//! - 成功したら**ロックを保持したまま**未保存の印を落とす（要件 5.1）。失敗のときは印を保つ
//!   （要件 5.4）
//! - **保存は内容を変えないため版は進めない**（モジュール docs「未保存と版」）
//! - 出所が [`Origin::New`] のときは**書き出さずに** [`SaveReport::NeedsLocation`] を返す
//!   （保存先の選択は適応層の仕事である。要件 5.2）
//! - 書き出しの失敗は [`SaveReport::Failed`] が形式の側の理由をそのまま運ぶ
//!   （[`SessionError`] には書式の誤りを足さない。design.md「Error Handling」の分担）

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};

use document_format::{Document, DocumentFormat, DocumentFormatApi, Sheet};

use crate::error::{SaveReport, SessionError};
use crate::state::{CloseAnswer, Edited, Origin, SessionState, SheetSummary};

/// 新規作成するドキュメントのシート名。
///
/// 要件 7.1 は「行も列も無いシートを 1 つ持つ」ことだけを定め、名前を定めない。名前は
/// 境界（`document_state`）を通って画面に出るため、利用者に見える日本語の既定名を置く。
const NEW_SHEET_NAME: &str = "シート1";

/// 1 つのセッションの状態機械（design.md「Slot」）。
///
/// 出所と保持する [`Document`] を 1 対で持ち、読み取りと変更を同じロックの下で貸す。
/// 未保存と変更の版は**ロックの外**の原子値である（[`Slot::may_close`] が待たずに答えるため。
/// モジュール docs「ロックの規律」）。
///
/// 本クレートの公開面（タスク 2.5）・引き渡しの配線（タスク 3.2）が使うまでの seam である。
#[allow(dead_code)] // 上記の seam（`src-tauri/src/ports.rs` と同じ扱い）。
pub(crate) struct Slot {
    /// 出所と文書（解決済みのときだけ文書を持つ）。読み取り・変更・状態はこのロックを取る。
    inner: Mutex<Inner>,
    /// 未保存の変更があるか。**文書のロックの外**（待たずに読む。要件 2.5）。
    /// 更新は文書のロックを保持したまま行う（モジュール docs「未保存と版」）。
    unsaved: AtomicBool,
    /// 変更の版。**文書が入れ替わるか変更が適用されたときに 1 進む**（読み込みの完了・新規作成・
    /// 変更の適用。タスク 2.4 が適用の分を記録する）。**文書のロックの外**。
    revision: AtomicU64,
}

/// セッションの内部状態。
///
/// `Resolved` だけが文書を持ち、読み取り・変更・保存・破棄の印を受け付ける。
#[allow(dead_code)] // `Slot` と同じ seam（公開面 = 2.5 / 引き渡しの配線 = 3.2）。
pub(crate) enum Inner {
    /// まだ解決していない（生成要求の位置を読むのは呼び出し元の責任である）。
    Unresolved,

    /// 解決済み。出所と文書を 1 対で持つ。
    Resolved {
        /// 出所（新規 / 読み込んだ位置）。
        origin: Origin,
        /// 保持する文書。1 セッションにつき 1 実体である（複製しない）。
        document: Box<Document>,
    },

    /// 読み込みに失敗したことを覚えている（理由つき。design.md「Slot」の `Inner`）。
    /// **覚えないと、同じ位置への読み込みを問い合わせのたびに試みることになる**（起動時の
    /// 読み込みは画面の問い合わせが引き金である）。利用者が別の位置を選び直したときは
    /// [`Slot::attach`] がここから読み直す。
    Unavailable {
        /// 形式の側の誤り [`document_format::DocumentError`] の技術的診断
        /// （`Display` の結果。表示用の文言ではない）。
        reason: String,
    },
}

#[allow(dead_code)] // `Slot` と同じ seam（公開面 = 2.5 / 引き渡しの配線 = 3.2）。
impl Slot {
    /// 未解決のセッションを作る。文書はまだ無く、未保存でもない。
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::Unresolved),
            unsaved: AtomicBool::new(false),
            revision: AtomicU64::new(0),
        }
    }

    /// 与えられた位置からドキュメントを**1 度だけ**読み込んで保持する（design.md
    /// 「起動時に指定されたドキュメントの解決」）。
    ///
    /// `requested` は起動時に指定された位置である（無ければ `None`）。`None` のときは
    /// 読み込む対象が無いため何もしない。**冪等**であり、既に解決済みなら何もしない。
    ///
    /// 戻り値は読み込みの結果である: `Ok` のとき保持している（または何もしなかった）。
    /// 成功した読み込みは出所と文書を差し替え、**未保存を落として版を 1 進める**
    /// （モジュール docs「未保存と版」）。失敗したときは [`SessionError::Read`] を返し、
    /// **未解決だった場合は理由を状態として覚える**（[`SessionState::Unavailable`]）。
    /// 既に文書を保持していればそれを保つ。
    ///
    /// 別の操作が進行中なら待たずに [`SessionError::Busy`] を返す（要件 2.3。モジュール docs
    /// 「ロックの規律」）。
    pub(crate) fn resolve(&self, requested: Option<&Path>) -> Result<(), SessionError> {
        // 生成要求の位置が無い窓には読み込む対象が無い（resolve が扱うのは起動時に指定された
        // 位置だけである。利用者が選んだ位置は `attach` が扱う）。
        let Some(path) = requested else {
            return Ok(());
        };

        // 読み込みは**待たない**操作である: 進行中の読み込み・新規作成には Busy で答える。
        let mut inner = self.try_lock()?;
        match &*inner {
            // 冪等: 既に解決済みなら読み込みを起こさない（2 度目のアクセスで読み込みが起きない）。
            Inner::Resolved { .. } => return Ok(()),
            // 覚えた失敗は繰り返さない（問い合わせのたびに同じ位置を読み直さない）。
            // 利用者が選び直したときは `attach` が読み直す。
            Inner::Unavailable { .. } => return Ok(()),
            Inner::Unresolved => {}
        }

        match DocumentFormat::new().open(path) {
            Ok(outcome) => {
                // 判定と更新は同じ臨界区間である（ロックを保持したまま差し替える）。
                self.record_replacement(
                    &mut inner,
                    Origin::File(path.to_path_buf()),
                    outcome.document,
                );
                Ok(())
            }
            Err(source) => {
                // 読み込みに失敗した。未解決だったため保持していた文書は無く（失敗が既存の
                // 内容を上書きする経路は無い）、理由を状態として覚える（要件 2.1）。
                *inner = Inner::Unavailable {
                    reason: source.to_string(),
                };
                Err(SessionError::Read { source })
            }
        }
    }

    /// **利用者が選んだ位置**を読み込んで保持する（ファイルの引き渡し。design.md
    /// 「DocumentSessions」の `attach`）。
    ///
    /// 起動時の解決（[`Slot::resolve`]）と違い、**`Resolved` / `Unavailable` のどちらからでも
    /// 読み直して出所を更新する**（利用者の明示の操作であるため、覚えている失敗を繰り返さない）。
    /// 何も保持していないセッション（`Unresolved`）からは、この経路がセッションを作る。
    ///
    /// 未保存の変更があるときは [`SessionError::UnsavedChanges`] を返して**内容を変えない**
    /// （要件 2.2）。読み込みに失敗したときは [`SessionError::Read`] を返し、**保持している
    /// 文書を変えない**（何も保持していないときは理由を覚える。要件 2.1）。成功した差し替えは
    /// 未保存を落とし、版を 1 進める（モジュール docs「未保存と版」）。
    ///
    /// 別の操作が進行中なら待たずに [`SessionError::Busy`] を返す（要件 2.3）。
    pub(crate) fn attach(&self, location: &Path) -> Result<(), SessionError> {
        // 読み込みと同じく**待たない**操作である。
        let mut inner = self.try_lock()?;

        // 未保存の判定と差し替えを**同じ臨界区間で**行う。ロックの外で判定すると、判定の直後に
        // 別の経路が変更を適用し、その変更が差し替えで黙って失われる（要件 2.2）。
        if self.unsaved.load(Ordering::SeqCst) {
            return Err(SessionError::UnsavedChanges);
        }

        match DocumentFormat::new().open(location) {
            Ok(outcome) => {
                self.record_replacement(
                    &mut inner,
                    Origin::File(location.to_path_buf()),
                    outcome.document,
                );
                Ok(())
            }
            Err(source) => {
                // 保持している文書を変えない（`Resolved` のまま）。何も保持していないときだけ
                // 失敗を覚える（`Unresolved` と、前の失敗を覚えている `Unavailable`）。
                if !matches!(&*inner, Inner::Resolved { .. }) {
                    *inner = Inner::Unavailable {
                        reason: source.to_string(),
                    };
                }
                Err(SessionError::Read { source })
            }
        }
    }

    /// 保持している文書を読む。**未解決なら [`SessionError::NoDocument`]**。
    ///
    /// 文書のロックを取って閉包に参照を貸す。適用中に到着した読み取りは適用の完了を待ち、
    /// 適用済みの最新を返す（要件 3.4）。閉包の内側からセッションを呼び返してはならない
    /// （モジュール docs「再入禁止」）。
    pub(crate) fn read<R>(&self, f: &mut dyn FnMut(&Document) -> R) -> Result<R, SessionError> {
        let inner = self.lock();
        match &*inner {
            Inner::Resolved { document, .. } => Ok(f(document)),
            Inner::Unresolved | Inner::Unavailable { .. } => Err(SessionError::NoDocument),
        }
    }

    /// 保持している文書を可変で借りる（**記録しない**借用の口である）。
    ///
    /// **未保存の印と版はここでは変えない**（モジュール docs「書き込むが、記録しない」）。
    /// 記録まで含む変更の適用は `change::edit` が [`Slot::lock_for_change`] /
    /// [`Slot::record_edit`] で行う。未解決なら [`SessionError::NoDocument`]。閉包の内側から
    /// セッションを呼び返してはならない（モジュール docs「再入禁止」）。
    ///
    /// **production の経路は本関数を使わない**（現状はテストが「ロックを保持したまま、記録せずに
    /// 書き換える」操作として使う）。残すか `#[cfg(test)]` へ閉じるかは公開面（タスク 2.5）が
    /// 決める。
    pub(crate) fn with_document_mut<R>(
        &self,
        f: &mut dyn FnMut(&mut Document) -> R,
    ) -> Result<R, SessionError> {
        let mut inner = self.lock();
        match &mut *inner {
            Inner::Resolved { document, .. } => Ok(f(document)),
            Inner::Unresolved | Inner::Unavailable { .. } => Err(SessionError::NoDocument),
        }
    }

    /// 変更の適用のために文書のロックを**保持したまま**内部状態を貸す（タスク 2.4 の `change` が
    /// 使う。crate 可視）。
    ///
    /// 返した Guard を保持している間、同じセッションの他の操作（読み取り・状態の取得・保存・
    /// 差し替え）はこのロックを待つ。したがって呼び出し元は、**Guard を持ったまま**閉包を実行し、
    /// 同じ Guard を [`Slot::record_edit`] へ渡して記録すること。これにより判定（文書を保持して
    /// いるか）・閉包の実行・記録が 1 つの臨界区間に入る（design.md「Slot」の不変条件
    /// 「未保存と版の更新は文書のロックを保持したまま行う」）。
    ///
    /// この入口が要るのは、[`Slot::with_document_mut`] が**記録しない**契約だからである
    /// （モジュール docs「書き込むが、記録しない」）。記録まで含む口を `session` に置くと
    /// 「変更の適用」の意味論（変更の語彙を持たないこと・閉包の失敗でも記録する保守側への
    /// 倒し方）が状態機械へ混ざるため、状態機械は**貸す口**だけを与え、意味論は `change` が持つ。
    pub(crate) fn lock_for_change(&self) -> MutexGuard<'_, Inner> {
        self.lock()
    }

    /// 変更の適用を記録する（**その時点で**文書のロックを保持していることが前提。タスク 2.4 の
    /// `change` が使う。crate 可視）。
    ///
    /// 未保存の印を立て、**版を 1 進める**（モジュール docs「未保存と版」。1 回の閉包が 1 回の
    /// 適用である）。閉包が失敗を返していても、閉包が文書を変えたかを判定できないため保守側に
    /// 倒して記録する（design.md「ChangeApply」）。
    ///
    /// `_lock` が要求するのは「**呼び出しの時点で**いずれかの Guard を保持している」ことだけで
    /// あり、**同じ臨界区間の内側で起きること自体は型では強制されない**（`&MutexGuard` は
    /// `slot.record_edit(&slot.lock_for_change(), value)` のような取り直しも通す）。記録が閉包と
    /// 同じ 1 つの Guard の生存範囲に入ることは [`crate::change::edit`] のコードの形状が担保する。
    pub(crate) fn record_edit<R>(&self, _lock: &MutexGuard<'_, Inner>, value: R) -> Edited<R> {
        self.unsaved.store(true, Ordering::SeqCst);
        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        Edited {
            value,
            revision,
            unsaved: true,
        }
    }

    /// 行も列も無いシートを 1 つ持つドキュメントを用意し、出所を「新規」にする（要件 7.1,
    /// 7.2, 7.4）。
    ///
    /// **未保存でない状態から始め、版を 1 進める**（文書が入れ替わるため。モジュール docs
    /// 「未保存と版」）。未保存の変更があるときは拒否する（[`SessionError::UnsavedChanges`]。
    /// 要件 7.3）。別の操作が進行中なら待たずに [`SessionError::Busy`] を返す（モジュール docs
    /// 「ロックの規律」）。
    pub(crate) fn create(&self) -> Result<(), SessionError> {
        // 読み込みと同じく**待たない**操作である。
        let mut inner = self.try_lock()?;

        // 未保存の判定と差し替えを**同じ臨界区間で**行う。ロックの外で判定すると、判定の直後に
        // 別の経路が変更を適用し、その変更が差し替えで黙って失われる（要件 7.3）。
        if self.unsaved.load(Ordering::SeqCst) {
            return Err(SessionError::UnsavedChanges);
        }

        // 行も列も無いシートを 1 つ持つ文書（要件 7.1）。出所を持たず（要件 7.2）、
        // 未保存の変更も無い状態から始める。
        let mut document = Document::new();
        document.add_sheet(NEW_SHEET_NAME);
        self.record_replacement(&mut inner, Origin::New, document);
        Ok(())
    }

    /// 未保存の印を落とす（**保存しない**。利用者の明示の指示による。要件 6.5）。
    ///
    /// 未解決なら [`SessionError::NoDocument`]。文書のロックを取るため、適用の完了を待つ。
    /// 印を落とすのは**同じ臨界区間の内側**である（判定と更新を分けない）。文書は入れ替わらない
    /// ため版は進めない。
    pub(crate) fn discard(&self) -> Result<(), SessionError> {
        // 文書のロックの下で判定し、そのまま印を落とす（進行中の適用が完了してから落ちる）。
        let inner = self.lock();
        if !matches!(&*inner, Inner::Resolved { .. }) {
            return Err(SessionError::NoDocument);
        }
        self.unsaved.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// 出所の位置へ書き出す（要件 5.1, 5.4, 5.5, 5.7）。
    ///
    /// **文書のロックの下で 1 パスとして行う**: 書き出しの間ロックを保持するため、進行中に
    /// 到着した適用は書き出しの完了を待ち、書き出した内容と保持している内容が食い違わない
    /// （要件 5.5）。書き出す内容と形式は形式の側（[`DocumentFormatApi::save`]）に委ね、
    /// 本クレートはファイルを組み立てない（要件 5.7）。
    ///
    /// 未解決なら [`SessionError::NoDocument`]（要件 1.8。セッションを作るのは適応層の 3 つの
    /// 入口だけである）。出所が [`Origin::New`] のときは**書き出さずに**
    /// [`SaveReport::NeedsLocation`] を返す（保存先の選択は適応層の仕事である。要件 5.2）。
    /// 書き出しに成功したら**ロックを保持したまま**未保存の印を落とし（要件 5.1）、失敗の
    /// ときは印を保つ（要件 5.4）。**保存は内容を変えないため版は進めない**（モジュール docs
    /// 「未保存と版」）。
    pub(crate) fn save(&self) -> Result<SaveReport, SessionError> {
        let mut inner = self.lock();
        let location = match &*inner {
            Inner::Unresolved | Inner::Unavailable { .. } => return Err(SessionError::NoDocument),
            // 出所が無い: 書き出す位置が決まらないため、選択を要することを結果として返す
            // （誤りではない。design.md「Error Strategy」の第 3 分類）。
            Inner::Resolved {
                origin: Origin::New, ..
            } => return Ok(SaveReport::NeedsLocation),
            // 出所がある: その位置へ書き出す（出所は既にこの位置であるため差し替えない）。
            Inner::Resolved {
                origin: Origin::File(location),
                ..
            } => location.clone(),
        };
        self.write_under_lock(&mut inner, &location, false)
    }

    /// 選ばれた位置へ書き出し、成功したら**以後の出所をその位置にする**（要件 5.2, 5.8）。
    ///
    /// 出所を差し替えるため、以後の [`Slot::save`] は同じ位置へ同じ内容を書き出す（要件 5.8）。
    /// 書き出しの規律（ロックの下の 1 パス・成功で未保存を落とす・版を進めない）は
    /// [`Slot::save`] と同じであり、失敗したときは出所も未保存も変えない（要件 5.4）。
    /// 未解決なら [`SessionError::NoDocument`]。
    pub(crate) fn save_to(&self, location: &Path) -> Result<SaveReport, SessionError> {
        let mut inner = self.lock();
        self.write_under_lock(&mut inner, location, true)
    }

    /// 保持している文書を `location` へ書き出し、記録を更新する（**呼び出し元が文書のロックを
    /// 保持していること**が前提）。
    ///
    /// `adopt` が真なら出所をその位置へ差し替える（[`Slot::save_to`]）。成功なら**同じ臨界
    /// 区間の内側で**未保存の印を落とし、失敗なら印と出所を保つ。**版はどちらの経路でも進め
    /// ない**（保存は内容を変えない）。未解決なら [`SessionError::NoDocument`]。
    fn write_under_lock(
        &self,
        inner: &mut Inner,
        location: &Path,
        adopt: bool,
    ) -> Result<SaveReport, SessionError> {
        let Inner::Resolved { origin, document } = inner else {
            return Err(SessionError::NoDocument);
        };

        match DocumentFormat::new().save(document, location) {
            Ok(()) => {
                if adopt {
                    *origin = Origin::File(location.to_path_buf());
                }
                // 成功: ロックを保持したまま未保存を落とす（判定と更新を分けない）。
                self.unsaved.store(false, Ordering::SeqCst);
                Ok(SaveReport::Saved {
                    location: location.to_path_buf(),
                })
            }
            // 失敗: 印を保ち、出所も変えない。理由は形式の側のまま運ぶ。
            Err(source) => Ok(SaveReport::Failed { source }),
        }
    }

    /// 状態の写しを返す（要件 1.6, 1.7, 4.3）。
    ///
    /// **名前はファイル名だけ**であり、位置そのものを境界へ出さない（design.md「Boundary
    /// Commitments」）。文書のロックを取るため、適用の完了を待つ。
    pub(crate) fn state(&self) -> SessionState {
        let inner = self.lock();
        match &*inner {
            Inner::Unresolved => SessionState::Absent,
            Inner::Unavailable { reason } => SessionState::Unavailable {
                reason: reason.clone(),
            },
            Inner::Resolved { origin, document } => SessionState::Open {
                name: document_name(origin),
                origin: origin.clone(),
                unsaved: self.unsaved.load(Ordering::SeqCst),
                sheets: document.sheets().iter().map(sheet_summary).collect(),
            },
        }
    }

    /// 閉じてよいかを返す（要件 4.6, 2.5）。
    ///
    /// **未保存の原子値だけを読み、文書のロックを取らない**: 読み込みや適用が進行中でも
    /// 待たない。
    pub(crate) fn may_close(&self) -> CloseAnswer {
        if self.unsaved.load(Ordering::SeqCst) {
            CloseAnswer::Deny
        } else {
            CloseAnswer::Allow
        }
    }

    /// 文書の差し替えを記録する（**呼び出し元が文書のロックを保持していること**が前提）。
    ///
    /// 出所と文書を差し替え、未保存を落とし、版を 1 進める。design.md「Slot」の不変条件
    /// 「未保存と版の更新は文書のロックを保持したまま行う」「文書が入れ替わる操作（読み込みの
    /// 完了・新規作成）でも版は 1 進む」を 1 箇所に集め、読み込みの完了（[`Slot::resolve`] /
    /// [`Slot::attach`]）と新規作成（[`Slot::create`]）が共有する。
    fn record_replacement(&self, inner: &mut Inner, origin: Origin, document: Document) {
        *inner = Inner::Resolved {
            origin,
            document: Box::new(document),
        };
        self.unsaved.store(false, Ordering::SeqCst);
        self.revision.fetch_add(1, Ordering::SeqCst);
    }

    /// 文書のロックを待って取る（読み取り・可変の借用・状態・破棄の印）。
    ///
    /// 毒された場合は中身をそのまま使う（`document-format` と `app-shell` の
    /// `unwrap_or_else(PoisonError::into_inner)` と同じ規律。panic した閉包の後でも、
    /// 保持している文書は構造的には壊れていない）。
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 文書のロックを**待たずに**取る（読み込み・引き渡し・新規作成）。
    ///
    /// 別の操作が保持していれば [`SessionError::Busy`] を返す（要件 2.3）。
    fn try_lock(&self) -> Result<MutexGuard<'_, Inner>, SessionError> {
        match self.inner.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::Poisoned(poisoned)) => Ok(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => Err(SessionError::Busy),
        }
    }
}

/// 保持している文書の名前（**ファイル名だけ**。design.md「Boundary Commitments」の
/// 「位置を境界へ出さない。名前はファイル名のみとする」）。
///
/// 出所を持たない新規の文書は空文字である（design.md「型（コア）」）。
fn document_name(origin: &Origin) -> String {
    match origin {
        Origin::New => String::new(),
        Origin::File(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

/// シート 1 枚の要約（識別子は上流の型のまま。文字列化は境界型の担当である）。
fn sheet_summary(sheet: &Sheet) -> SheetSummary {
    SheetSummary {
        id: sheet.id(),
        name: sheet.name().to_owned(),
        columns: sheet.columns().len(),
        rows: sheet.rows().len(),
    }
}

#[cfg(test)]
mod tests {
    // テストとベンチが共有する標本の生成器（タスク 1.4）。`tests/` 配下のモジュールは `src/` の
    // モジュールから参照できないため、`lib.rs` が `#[cfg(test)]` で取り込んでいる
    // （`crate::common`。取り込みの理由は `lib.rs` の宣言を参照）。
    use crate::common;

    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use document_format::Document;

    use super::*;

    /// 標本を一時ディレクトリへ書き出し、その位置を返す（`Scratch` は呼び出し元が保つ）。
    fn write_sample(scratch: &common::Scratch, name: &str, rows: usize, columns: usize) -> PathBuf {
        let path = scratch.file(name);
        let sample = common::sample(&common::SampleSpec::new(rows, columns));
        common::api()
            .save(sample.document(), &path)
            .expect("標本を保存できる");
        path
    }

    /// 標本を 1 度読み込んだ解決済みのセッションを返す（一時ディレクトリは読み込み後に消える）。
    fn resolved_slot(rows: usize, columns: usize) -> Slot {
        let scratch = common::Scratch::new("session");
        let path = write_sample(&scratch, "sample.jxcel", rows, columns);
        let slot = Slot::new();
        slot.resolve(Some(&path)).expect("標本を読み込める");
        slot
    }

    /// 文書のロックを保持したまま待つスレッドを起こし、`body` を実行してから解放する。
    ///
    /// `body` がロックを待つ実装なら本関数はそこで止まる。つまり「待たない」ことは、
    /// **完了すること**と返り値の正しさで観測される（この補助は時間の閾値を使わない。
    /// 待ったまま止まらないことを**落として**示す必要がある `may_close` のテストは、
    /// デッドロック検出の待ちを伴う自前の形で観測する）。
    fn hold_lock_while<T>(slot: &Arc<Slot>, body: impl FnOnce() -> T) -> T {
        let (held_tx, held_rx): (Sender<()>, Receiver<()>) = mpsc::channel();
        let (release_tx, release_rx): (Sender<()>, Receiver<()>) = mpsc::channel();
        let holder = {
            let slot = Arc::clone(slot);
            thread::spawn(move || {
                slot.with_document_mut(&mut |_document| {
                    held_tx.send(()).expect("観測側が待っている");
                    release_rx.recv().expect("解放の指示を待つ");
                })
                .expect("文書を可変で借りられる");
            })
        };

        held_rx.recv().expect("ロックが保持された");
        let value = body();
        release_tx.send(()).expect("解放を指示できる");
        holder.join().expect("保持側のスレッドが終わる");
        value
    }

    #[test]
    fn unresolved_slot_rejects_read_edit_and_discard() {
        let slot = Slot::new();
        assert_eq!(SessionState::Absent, slot.state(), "未解決の状態の写し");

        let mut read = |_document: &Document| ();
        assert!(
            matches!(slot.read(&mut read), Err(SessionError::NoDocument)),
            "未解決のセッションの読み取りが失敗しない"
        );
        let mut edit = |_document: &mut Document| ();
        assert!(
            matches!(slot.with_document_mut(&mut edit), Err(SessionError::NoDocument)),
            "未解決のセッションの可変の借用が失敗しない"
        );
        assert!(
            matches!(slot.discard(), Err(SessionError::NoDocument)),
            "未解決のセッションの破棄の印が失敗しない"
        );

        assert_eq!(SessionState::Absent, slot.state(), "拒否が状態を変えた");
        assert!(!slot.unsaved.load(Ordering::SeqCst), "未保存の印が変わった");
        assert_eq!(0, slot.revision.load(Ordering::SeqCst), "版が変わった");
    }

    #[test]
    fn resolve_reads_the_requested_location_once() {
        let scratch = common::Scratch::new("session-resolve-once");
        let path = write_sample(&scratch, "once.jxcel", 4, 3);

        let slot = Slot::new();
        slot.resolve(Some(&path)).expect("標本を読み込める");
        let loaded = slot.state();
        let SessionState::Open {
            ref name,
            ref sheets,
            unsaved,
            ..
        } = loaded
        else {
            panic!("読み込みの後に保持していない: {loaded:?}");
        };
        assert_eq!("once.jxcel", name, "名前はファイル名だけである");
        assert!(!unsaved, "読み込みの直後は未保存でない");
        assert_eq!(1, sheets.len());
        assert_eq!(4, sheets[0].rows);
        assert_eq!(3, sheets[0].columns);
        assert_eq!(
            1,
            slot.revision.load(Ordering::SeqCst),
            "読み込みの完了で版がちょうど 1 進んでいない"
        );

        // 位置を消してから 2 度目の解決を行っても、保持し続ける（読み込みは 1 度だけ）。
        std::fs::remove_file(&path).expect("標本を消せる");
        slot.resolve(Some(&path))
            .expect("2 度目の解決は読み込みを起こさない");
        assert_eq!(loaded, slot.state(), "2 度目の解決が状態を変えた");
        assert_eq!(
            1,
            slot.revision.load(Ordering::SeqCst),
            "読み込みを起こさなかったのに版が進んだ"
        );

        let rows = slot
            .read(&mut |document| document.sheets()[0].rows().len())
            .expect("保持している文書を読める");
        assert_eq!(4, rows, "保持していた文書が失われた");
    }

    #[test]
    fn resolve_failure_remembers_the_reason_and_keeps_the_held_document() {
        let scratch = common::Scratch::new("session-read-failure");
        let missing = scratch.file("missing.jxcel");

        // (1) 未解決だった場合: 読み込みの失敗が状態として残る。
        let failing = Slot::new();
        let error = failing
            .resolve(Some(&missing))
            .expect_err("存在しない位置は読み込めない");
        assert!(
            matches!(error, SessionError::Read { .. }),
            "読み込みの失敗が形式の側の理由を運んでいない"
        );
        let SessionState::Unavailable { reason } = failing.state() else {
            panic!("読み込みに失敗した理由が状態に残っていない");
        };
        assert!(!reason.is_empty(), "理由が空である");

        // 同じ位置への問い合わせのたびに読み込みを試みない（覚えないと同じ位置への読み込みを
        // 問い合わせのたびに試みることになる）。位置に本物の文書を置いても状態は変わらない。
        write_sample(&scratch, "missing.jxcel", 2, 2);
        failing
            .resolve(Some(&missing))
            .expect("2 度目の解決は読み込みを試みない");
        assert!(
            matches!(failing.state(), SessionState::Unavailable { .. }),
            "覚えた失敗が消えた"
        );

        // 文書を持たないため、読み取りは失敗し、新規作成は通る。
        let mut read = |_document: &Document| ();
        assert!(matches!(
            failing.read(&mut read),
            Err(SessionError::NoDocument)
        ));
        failing.create().expect("読み込めなかった窓でも新規作成できる");
        assert!(matches!(failing.state(), SessionState::Open { .. }));

        // (2) 既に文書を保持している場合: 失敗しても保持内容を変えない。
        let held = resolved_slot(3, 2);
        let before = held.state();
        held.resolve(Some(&missing))
            .expect("解決済みのセッションは読み込みを起こさない");
        assert_eq!(before, held.state(), "読み込みの失敗が保持内容を変えた");
        let rows = held
            .read(&mut |document| document.sheets()[0].rows().len())
            .expect("保持している文書を読める");
        assert_eq!(3, rows, "保持していた文書が失われた");
    }

    #[test]
    fn attach_replaces_the_held_document_and_records_the_change() {
        let scratch = common::Scratch::new("session-attach");
        let chosen = write_sample(&scratch, "chosen.jxcel", 5, 4);

        let slot = resolved_slot(2, 2);
        assert_eq!(1, slot.revision.load(Ordering::SeqCst));
        let before = slot.state();

        slot.attach(&chosen)
            .expect("利用者が選んだ位置を読み込める");

        let SessionState::Open {
            name,
            origin,
            unsaved,
            sheets,
        } = slot.state()
        else {
            panic!("引き渡しの後に保持していない");
        };
        assert_eq!("chosen.jxcel", name, "出所の位置が更新されていない");
        assert_eq!(
            Origin::File(chosen.clone()),
            origin,
            "出所が選ばれた位置でない"
        );
        assert!(!unsaved, "引き渡しの直後は未保存でない");
        assert_eq!(1, sheets.len());
        assert_eq!(5, sheets[0].rows, "新しい文書の行数でない");
        assert_eq!(4, sheets[0].columns, "新しい文書の列数でない");
        assert_eq!(
            2,
            slot.revision.load(Ordering::SeqCst),
            "差し替えで版がちょうど 1 進んでいない"
        );
        assert_eq!(CloseAnswer::Allow, slot.may_close());
        assert_ne!(before, slot.state(), "前の保持内容のままである");
    }

    #[test]
    fn attach_is_refused_while_unsaved_and_keeps_the_document() {
        let scratch = common::Scratch::new("session-attach-unsaved");
        let chosen = write_sample(&scratch, "chosen.jxcel", 5, 4);

        let slot = resolved_slot(2, 2);
        slot.unsaved.store(true, Ordering::SeqCst);
        let before = slot.state();
        let revision = slot.revision.load(Ordering::SeqCst);

        assert!(
            matches!(slot.attach(&chosen), Err(SessionError::UnsavedChanges)),
            "未保存のまま引き渡しを受け付けた"
        );
        assert_eq!(before, slot.state(), "拒否が保持内容を変えた");
        assert_eq!(
            revision,
            slot.revision.load(Ordering::SeqCst),
            "拒否が版を進めた"
        );
        assert!(
            slot.unsaved.load(Ordering::SeqCst),
            "拒否が未保存の印を落とした"
        );
    }

    #[test]
    fn attach_failure_keeps_the_held_document() {
        let scratch = common::Scratch::new("session-attach-failure");
        let missing = scratch.file("missing.jxcel");

        let slot = resolved_slot(3, 2);
        let before = slot.state();
        let revision = slot.revision.load(Ordering::SeqCst);

        let error = slot
            .attach(&missing)
            .expect_err("存在しない位置は読み込めない");
        assert!(
            matches!(error, SessionError::Read { .. }),
            "失敗が形式の側の理由を運んでいない"
        );
        assert_eq!(before, slot.state(), "失敗が保持内容を変えた");
        assert_eq!(
            revision,
            slot.revision.load(Ordering::SeqCst),
            "失敗が版を進めた"
        );
        let rows = slot
            .read(&mut |document| document.sheets()[0].rows().len())
            .expect("保持している文書を読める");
        assert_eq!(3, rows, "保持していた文書が失われた");
    }

    #[test]
    fn attach_loads_a_chosen_location_from_nothing_and_from_a_remembered_failure() {
        let scratch = common::Scratch::new("session-attach-recovery");
        let missing = scratch.file("missing.jxcel");
        let chosen = write_sample(&scratch, "chosen.jxcel", 2, 3);

        // 何も保持していないセッション（未解決）から、引き渡しがセッションを作る。
        let fresh = Slot::new();
        fresh.attach(&chosen).expect("未解決からでも読み込める");
        assert!(matches!(fresh.state(), SessionState::Open { .. }));
        assert_eq!(1, fresh.revision.load(Ordering::SeqCst));

        // 覚えている失敗（読み込めなかった状態）から、利用者が選び直せば読み直す。
        let failing = Slot::new();
        failing
            .attach(&missing)
            .expect_err("存在しない位置は読み込めない");
        assert!(matches!(
            failing.state(),
            SessionState::Unavailable { .. }
        ));
        assert_eq!(0, failing.revision.load(Ordering::SeqCst));
        failing
            .attach(&chosen)
            .expect("選び直した位置で復帰できる");

        let SessionState::Open { name, unsaved, .. } = failing.state() else {
            panic!("復帰の後に保持していない");
        };
        assert_eq!("chosen.jxcel", name);
        assert!(!unsaved);
        assert_eq!(
            1,
            failing.revision.load(Ordering::SeqCst),
            "復帰で版がちょうど 1 進んでいない"
        );
    }

    #[test]
    fn may_close_allows_a_clean_session_and_denies_an_unsaved_one() {
        let slot = resolved_slot(2, 2);
        assert_eq!(CloseAnswer::Allow, slot.may_close(), "未保存でない");

        // 印を直接立てる（未保存の記録は適用口の責務であり、ここでは状態だけを作る）。
        slot.unsaved.store(true, Ordering::SeqCst);
        assert_eq!(CloseAnswer::Deny, slot.may_close(), "未保存である");
        assert!(
            matches!(slot.state(), SessionState::Open { unsaved: true, .. }),
            "状態の写しが未保存を運んでいない"
        );

        // 破棄の印は未保存を落とすが、**文書は入れ替わらないため版を進めない**（design.md
        // 「Data Models → Domain Model」）。
        let revision = slot.revision.load(Ordering::SeqCst);
        slot.discard().expect("破棄の印は文書があるとき通る");
        assert!(!slot.unsaved.load(Ordering::SeqCst), "破棄の印が落ちていない");
        assert_eq!(
            revision,
            slot.revision.load(Ordering::SeqCst),
            "破棄の印が版を進めた"
        );
        assert_eq!(CloseAnswer::Allow, slot.may_close(), "破棄の後");
    }

    #[test]
    fn may_close_does_not_wait_for_the_document_lock() {
        /// デッドロック検出のための待ち時間である（**性能の閾値ではない**）。`verification.md`
        /// が禁じているのは「所要時間を速度の証拠として使う閾値」であり、ここで見ているのは
        /// 「返ってくるかどうか」だけである。時間の閾値はこのテストの外に置かない。
        const DEADLOCK_WAIT: Duration = Duration::from_secs(10);

        let slot = Arc::new(resolved_slot(2, 2));
        slot.unsaved.store(true, Ordering::SeqCst);

        // 文書のロックを保持したまま待つスレッドを起こす。解放を待つ側も有限にしておく
        // （時間切れならこのスレッドは降りる。永久にブロックさせない）。観測側の待ちより
        // **長く**待つので、実装がロックを取る場合は観測側が先に時間切れになり、失敗の理由が
        // 「may_close が待っている」に定まる。
        let (held_tx, held_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let holder = {
            let slot = Arc::clone(&slot);
            thread::spawn(move || {
                slot.with_document_mut(&mut |_document| {
                    held_tx.send(()).expect("観測側が待っている");
                    release_rx
                        .recv_timeout(DEADLOCK_WAIT * 2)
                        .expect("待ち時間内に解放される");
                })
                .expect("文書を可変で借りられる");
            })
        };
        held_rx
            .recv_timeout(DEADLOCK_WAIT)
            .expect("待ち時間内にロックが保持される");

        // `may_close` は**別のスレッドで**呼ぶ。実装が文書のロックを取るようになればこの
        // 呼び出しは返らず、下の待ちが時間切れになってテストが**落ちる**（ハングしない）。
        let (answer_tx, answer_rx) = mpsc::channel::<CloseAnswer>();
        let asker = {
            let slot = Arc::clone(&slot);
            thread::spawn(move || {
                answer_tx
                    .send(slot.may_close())
                    .expect("観測側が待っている");
            })
        };

        let answer = answer_rx
            .recv_timeout(DEADLOCK_WAIT)
            .expect("may_close が文書のロックを待っている（デッドロック検出の時間切れ）");
        assert_eq!(CloseAnswer::Deny, answer, "未保存のまま閉じてよいと答えた");

        release_tx.send(()).expect("解放を指示できる");
        asker.join().expect("観測側のスレッドが終わる");
        holder.join().expect("保持側のスレッドが終わる");
    }

    #[test]
    fn resolve_attach_and_create_are_refused_while_the_lock_is_held() {
        let slot = Arc::new(resolved_slot(2, 2));
        let scratch = common::Scratch::new("session-busy");
        let other = scratch.file("other.jxcel");

        // 待たない操作（読み込み・引き渡し・新規作成）は、進行中の操作を待たずに Busy で
        // 失敗する。
        let (loaded, attached, created) = hold_lock_while(&slot, || {
            (
                slot.resolve(Some(&other)),
                slot.attach(&other),
                slot.create(),
            )
        });
        assert!(
            matches!(loaded, Err(SessionError::Busy)),
            "進行中の読み込みを待った"
        );
        assert!(
            matches!(attached, Err(SessionError::Busy)),
            "進行中の引き渡しを待った"
        );
        assert!(
            matches!(created, Err(SessionError::Busy)),
            "進行中の新規作成を待った"
        );

        assert!(
            matches!(slot.state(), SessionState::Open { .. }),
            "進行中の操作の間に状態が変わった"
        );
    }

    #[test]
    fn mutable_borrow_does_not_record_an_edit() {
        let slot = resolved_slot(2, 2);
        let before = slot.state();
        let revision = slot.revision.load(Ordering::SeqCst);

        let rows = slot
            .with_document_mut(&mut |document| document.sheets()[0].rows().len())
            .expect("文書を可変で借りられる");
        assert_eq!(2, rows);
        assert_eq!(before, slot.state(), "可変の借用が状態を変えた");
        assert!(
            !slot.unsaved.load(Ordering::SeqCst),
            "可変の借用が未保存の印を立てた（記録は 2.4 の責務である）"
        );
        assert_eq!(
            revision,
            slot.revision.load(Ordering::SeqCst),
            "可変の借用が版を進めた（記録は 2.4 の責務である）"
        );
    }

    #[test]
    fn create_prepares_one_empty_sheet_without_unsaved_changes() {
        let slot = Slot::new();
        slot.create().expect("ドキュメントを持たない窓で新規作成できる");

        let SessionState::Open {
            name,
            origin,
            unsaved,
            sheets,
        } = slot.state()
        else {
            panic!("新規作成でドキュメントを保持していない");
        };
        assert_eq!("", name, "新規の名前はファイル名を持たない");
        assert_eq!(Origin::New, origin, "出所が新規でない");
        assert!(!unsaved, "新規作成は未保存でない状態から始まる");
        assert_eq!(1, sheets.len(), "シートが 1 つでない");
        assert_eq!(0, sheets[0].rows, "行がある");
        assert_eq!(0, sheets[0].columns, "列がある");
        assert!(!sheets[0].name.is_empty(), "シート名が空である");
        assert_eq!(CloseAnswer::Allow, slot.may_close());
        assert_eq!(
            1,
            slot.revision.load(Ordering::SeqCst),
            "新規作成で版がちょうど 1 進んでいない"
        );

        // 以後、読み取りと変更を受け付ける（要件 7.4）。
        let rows = slot
            .read(&mut |document| document.sheets()[0].rows().len())
            .expect("新規作成の後は読める");
        assert_eq!(0, rows);

        // 未保存でなければ、保持している文書の差し替えも通る。
        let replaced = resolved_slot(1, 1);
        assert_eq!(1, replaced.revision.load(Ordering::SeqCst));
        replaced.create().expect("未保存でなければ差し替えられる");
        let SessionState::Open { origin, sheets, .. } = replaced.state() else {
            panic!("差し替えの後に保持していない");
        };
        assert_eq!(Origin::New, origin);
        assert_eq!(1, sheets.len());
        assert_eq!(0, sheets[0].rows);
        assert!(!replaced.unsaved.load(Ordering::SeqCst));
        assert_eq!(
            2,
            replaced.revision.load(Ordering::SeqCst),
            "差し替えで版がちょうど 1 進んでいない"
        );
    }

    #[test]
    fn create_is_refused_while_unsaved_and_keeps_the_document() {
        let slot = resolved_slot(3, 2);
        slot.unsaved.store(true, Ordering::SeqCst);
        let before = slot.state();
        let revision = slot.revision.load(Ordering::SeqCst);

        assert!(
            matches!(slot.create(), Err(SessionError::UnsavedChanges)),
            "未保存のまま新規作成を受け付けた"
        );
        assert_eq!(before, slot.state(), "拒否が保持内容を変えた");
        assert_eq!(
            revision,
            slot.revision.load(Ordering::SeqCst),
            "拒否が版を進めた"
        );
    }

    #[test]
    fn save_without_an_origin_asks_for_a_location_and_writes_nothing() {
        let scratch = common::Scratch::new("session-save-new");
        let slot = Slot::new();
        slot.create().expect("新規作成できる");
        // 未保存を立てる（変更の適用の代わり。記録は適用口の責務である）。出所が無いため保存は
        // 何も書き出さない: 印が保たれることで「何も起きていない」ことを確かめる。
        slot.unsaved.store(true, Ordering::SeqCst);

        let report = slot.save().expect("出所の無い保存は結果として返る");
        assert!(
            matches!(report, SaveReport::NeedsLocation),
            "出所の無い保存が保存先の選択を求めない: {report:?}"
        );
        assert!(
            slot.unsaved.load(Ordering::SeqCst),
            "保存先の選択を求めただけで未保存の印が落ちた（何も書き出していない）"
        );
        assert_eq!(
            CloseAnswer::Deny,
            slot.may_close(),
            "何も書き出していないのに閉じてよいと答えた"
        );
        assert!(
            std::fs::read_dir(scratch.path())
                .expect("一時ディレクトリを読める")
                .next()
                .is_none(),
            "保存先の選択を求めたのに何か書き出した"
        );
        assert!(
            matches!(slot.state(), SessionState::Open { unsaved: true, .. }),
            "書き出していないのに状態が変わった"
        );
    }

    #[test]
    fn save_writes_to_the_origin_and_clears_the_unsaved_mark() {
        let scratch = common::Scratch::new("session-save-origin");
        let origin = write_sample(&scratch, "sample.jxcel", 4, 3);

        let slot = Slot::new();
        slot.resolve(Some(&origin)).expect("標本を読み込める");
        // 変更の適用（タスク 2.4）の代わりに印を直接立てる（未保存の記録は適用口の責務である）。
        slot.unsaved.store(true, Ordering::SeqCst);
        let revision = slot.revision.load(Ordering::SeqCst);

        let report = slot.save().expect("出所へ保存できる");
        assert!(
            matches!(&report, SaveReport::Saved { location } if location == &origin),
            "保存が結果に出所の位置を運ばない: {report:?}"
        );
        assert!(
            !slot.unsaved.load(Ordering::SeqCst),
            "成功しても未保存の印が落ちていない"
        );
        assert_eq!(CloseAnswer::Allow, slot.may_close(), "成功の後に閉じられない");
        assert_eq!(
            revision,
            slot.revision.load(Ordering::SeqCst),
            "保存が版を進めた（保存は内容を変えない）"
        );

        let SessionState::Open { origin: held, .. } = slot.state() else {
            panic!("保存の後に保持していない");
        };
        assert_eq!(Origin::File(origin), held, "出所の位置が変わった");
    }

    #[test]
    fn save_writes_the_same_bytes_as_a_direct_format_write() {
        let scratch = common::Scratch::new("session-save-bytes");
        let origin = write_sample(&scratch, "sample.jxcel", 6, 4);
        let direct = scratch.file("direct.jxcel");

        let slot = Slot::new();
        slot.resolve(Some(&origin)).expect("標本を読み込める");
        slot.unsaved.store(true, Ordering::SeqCst);
        slot.save().expect("出所へ保存できる");

        // 同じ文書（保持している実体）を形式の側へ**直接**書き出す（要件 5.7 の比較の相手）。
        slot.read(&mut |document| common::api().save(document, &direct))
            .expect("保持している文書を読める")
            .expect("形式の側へ直接書き出せる");

        let through_session = std::fs::read(&origin).expect("セッションが保存したファイルを読める");
        let through_format =
            std::fs::read(&direct).expect("形式の側へ直接書き出したファイルを読める");
        assert!(!through_format.is_empty(), "比較したバイト列が空である");
        assert_eq!(
            through_format, through_session,
            "セッションの経路のバイト列が形式の側へ直接書き出したバイト列と一致しない"
        );
    }

    #[test]
    fn save_to_adopts_the_location_for_following_saves() {
        let scratch = common::Scratch::new("session-save-to");
        let first = write_sample(&scratch, "first.jxcel", 5, 3);
        let chosen = scratch.file("chosen.jxcel");

        let slot = Slot::new();
        slot.resolve(Some(&first)).expect("標本を読み込める");
        // 未保存を立ててから保存先を指定する: 成功したら印が落ちること（`adopt` の腕でも落ちる
        // こと）を、この経路でも固定する。
        slot.unsaved.store(true, Ordering::SeqCst);
        let revision = slot.revision.load(Ordering::SeqCst);

        let report = slot.save_to(&chosen).expect("選ばれた位置へ保存できる");
        assert!(
            matches!(&report, SaveReport::Saved { location } if location == &chosen),
            "保存が選ばれた位置を運ばない: {report:?}"
        );
        assert!(
            !slot.unsaved.load(Ordering::SeqCst),
            "save_to の成功で未保存の印が落ちていない"
        );
        assert_eq!(
            CloseAnswer::Allow,
            slot.may_close(),
            "save_to の成功の後に閉じられない"
        );

        let SessionState::Open { name, origin, .. } = slot.state() else {
            panic!("保存の後に保持していない");
        };
        assert_eq!(
            Origin::File(chosen.clone()),
            origin,
            "出所が選ばれた位置でない"
        );
        assert_eq!("chosen.jxcel", name, "名前が新しい出所のファイル名でない");
        assert_eq!(
            revision,
            slot.revision.load(Ordering::SeqCst),
            "保存が版を進めた"
        );

        // 以後の保存は同じ位置へ書き出す（要件 5.8）。位置の内容を壊してから保存し、書き戻された
        // ことをバイト列で確かめる。
        std::fs::write(&chosen, b"broken").expect("選ばれた位置の内容を壊せる");
        let again = slot.save().expect("出所へ保存できる");
        assert!(
            matches!(&again, SaveReport::Saved { location } if location == &chosen),
            "2 度目の保存が選ばれた位置へ書き出さない: {again:?}"
        );

        let direct = scratch.file("direct.jxcel");
        slot.read(&mut |document| common::api().save(document, &direct))
            .expect("保持している文書を読める")
            .expect("形式の側へ直接書き出せる");
        assert_eq!(
            std::fs::read(&direct).expect("形式の側へ直接書き出したファイルを読める"),
            std::fs::read(&chosen).expect("2 度目に書き出したファイルを読める"),
            "2 度目の保存が同じ内容を書き出していない"
        );
    }

    #[test]
    fn failed_save_keeps_the_unsaved_mark_and_the_origin() {
        let scratch = common::Scratch::new("session-save-failure");
        let origin = write_sample(&scratch, "sample.jxcel", 3, 2);
        let missing = scratch.file("missing").join("deep").join("out.jxcel");

        let slot = Slot::new();
        slot.resolve(Some(&origin)).expect("標本を読み込める");
        slot.unsaved.store(true, Ordering::SeqCst);
        let revision = slot.revision.load(Ordering::SeqCst);
        let before = slot.state();

        let report = slot.save_to(&missing).expect("書き出しの失敗は結果として返る");
        assert!(
            matches!(report, SaveReport::Failed { .. }),
            "書き出せない位置への保存が失敗を報告しない: {report:?}"
        );
        assert!(
            slot.unsaved.load(Ordering::SeqCst),
            "失敗が未保存の印を落とした"
        );
        assert_eq!(CloseAnswer::Deny, slot.may_close(), "失敗の後に閉じてよいと答えた");
        assert_eq!(before, slot.state(), "失敗が状態（出所）を変えた");
        assert_eq!(
            revision,
            slot.revision.load(Ordering::SeqCst),
            "失敗が版を進めた"
        );
        assert!(!missing.exists(), "失敗したのにファイルが現れた");

        // 出所の位置そのものが書けなくなった場合（ディレクトリごと消えた）も同じである。
        let gone = common::Scratch::new("session-save-origin-gone");
        let vanished = write_sample(&gone, "sample.jxcel", 2, 2);
        let lost = Slot::new();
        lost.resolve(Some(&vanished)).expect("標本を読み込める");
        lost.unsaved.store(true, Ordering::SeqCst);
        std::fs::remove_dir_all(gone.path()).expect("一時ディレクトリごと消せる");

        let report = lost.save().expect("書き出しの失敗は結果として返る");
        assert!(
            matches!(report, SaveReport::Failed { .. }),
            "出所が消えたときの保存が失敗を報告しない: {report:?}"
        );
        assert!(
            lost.unsaved.load(Ordering::SeqCst),
            "出所が消えたときの失敗が未保存の印を落とした"
        );
    }

    #[test]
    fn unresolved_slot_rejects_both_save_routes() {
        let scratch = common::Scratch::new("session-save-absent");
        let location = scratch.file("nowhere.jxcel");
        let slot = Slot::new();

        assert!(
            matches!(slot.save(), Err(SessionError::NoDocument)),
            "未解決のセッションの保存が失敗しない"
        );
        assert!(
            matches!(slot.save_to(&location), Err(SessionError::NoDocument)),
            "未解決のセッションの保存先指定が失敗しない"
        );
        assert_eq!(SessionState::Absent, slot.state(), "拒否が状態を変えた");
        assert!(!location.exists(), "拒否したのにファイルが現れた");
    }

    #[test]
    fn unavailable_slot_rejects_both_save_routes() {
        let scratch = common::Scratch::new("session-save-unavailable");
        let missing = scratch.file("missing.jxcel");
        let location = scratch.file("nowhere.jxcel");

        let slot = Slot::new();
        slot.resolve(Some(&missing))
            .expect_err("存在しない位置は読み込めない");
        assert!(
            matches!(slot.state(), SessionState::Unavailable { .. }),
            "読み込みの失敗を覚えていない"
        );

        assert!(
            matches!(slot.save(), Err(SessionError::NoDocument)),
            "読み込めなかったセッションの保存が失敗しない"
        );
        assert!(
            matches!(slot.save_to(&location), Err(SessionError::NoDocument)),
            "読み込めなかったセッションの保存先指定が失敗しない"
        );
        assert!(!location.exists(), "拒否したのにファイルが現れた");
    }

    #[test]
    fn save_waits_for_an_apply_in_progress_and_writes_after_it() {
        /// デッドロック検出のための待ち時間である（**性能の閾値ではない**）。ここで見ているのは
        /// 「保存が文書のロックを待つか」であり、所要時間を速度の証拠に使うものではない。
        /// 時間の閾値はこのテストの外に置かない。
        const DEADLOCK_WAIT: Duration = Duration::from_secs(10);

        let scratch = common::Scratch::new("session-save-waits");
        let origin = write_sample(&scratch, "sample.jxcel", 4, 3);

        let slot = Arc::new(Slot::new());
        slot.resolve(Some(&origin)).expect("標本を読み込める");
        slot.unsaved.store(true, Ordering::SeqCst);

        // 保持側: 文書のロックを握り、解放の指示を待つ（時間切れなら降りる。永久にブロック
        // させない）。
        let (held_tx, held_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let holder = {
            let slot = Arc::clone(&slot);
            thread::spawn(move || {
                slot.with_document_mut(&mut |_document| {
                    held_tx.send(()).expect("観測側が待っている");
                    release_rx
                        .recv_timeout(DEADLOCK_WAIT * 2)
                        .expect("待ち時間内に解放される");
                })
                .expect("文書を可変で借りられる");
            })
        };
        held_rx
            .recv_timeout(DEADLOCK_WAIT)
            .expect("待ち時間内にロックが保持される");

        // 保持している間に書き出しが起きないことを観測できるよう、位置の内容を壊しておく。
        std::fs::write(&origin, b"not-a-document").expect("位置の内容を壊せる");

        // 保存は**別のスレッドで**走らせる。ロックを待たずに書き出す実装なら、解放の前に結果が
        // 届き、下の待ちが成功してテストが落ちる。
        let (saved_tx, saved_rx) = mpsc::channel::<Result<SaveReport, SessionError>>();
        let saver = {
            let slot = Arc::clone(&slot);
            thread::spawn(move || {
                saved_tx.send(slot.save()).expect("観測側が待っている");
            })
        };

        assert!(
            saved_rx.recv_timeout(DEADLOCK_WAIT).is_err(),
            "保存が文書のロックを待たずに書き出した（デッドロック検出の時間切れの前に返った）"
        );
        assert_eq!(
            b"not-a-document".to_vec(),
            std::fs::read(&origin).expect("位置を読める"),
            "文書のロックを保持している間に書き出した"
        );

        release_tx.send(()).expect("解放を指示できる");
        holder.join().expect("保持側のスレッドが終わる");

        let outcome = saved_rx
            .recv_timeout(DEADLOCK_WAIT)
            .expect("解放後に保存が完了する");
        assert!(
            matches!(outcome, Ok(SaveReport::Saved { .. })),
            "解放後の保存が成功しない: {outcome:?}"
        );
        assert_ne!(
            b"not-a-document".to_vec(),
            std::fs::read(&origin).expect("位置を読める"),
            "解放後も書き出していない"
        );
        assert!(
            !slot.unsaved.load(Ordering::SeqCst),
            "保存の成功で未保存の印が落ちていない"
        );
        saver.join().expect("保存側のスレッドが終わる");
    }

    #[test]
    fn save_does_not_advance_the_revision() {
        let scratch = common::Scratch::new("session-save-revision");
        let origin = write_sample(&scratch, "sample.jxcel", 4, 2);
        let chosen = scratch.file("chosen.jxcel");

        let slot = Slot::new();
        slot.resolve(Some(&origin)).expect("標本を読み込める");
        slot.unsaved.store(true, Ordering::SeqCst);
        let revision = slot.revision.load(Ordering::SeqCst);

        slot.save().expect("出所へ保存できる");
        assert_eq!(
            revision,
            slot.revision.load(Ordering::SeqCst),
            "出所への保存が版を進めた"
        );

        slot.unsaved.store(true, Ordering::SeqCst);
        slot.save_to(&chosen).expect("選ばれた位置へ保存できる");
        assert_eq!(
            revision,
            slot.revision.load(Ordering::SeqCst),
            "保存先を指定した保存が版を進めた"
        );
    }
}

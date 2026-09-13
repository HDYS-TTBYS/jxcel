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

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};

use document_format::{Document, DocumentFormat, DocumentFormatApi, Sheet};

use crate::error::SessionError;
use crate::state::{CloseAnswer, Origin, SessionState, SheetSummary};

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
/// 本クレートの表（タスク 2.3）・変更の適用口（タスク 2.4）・引き渡しの配線（タスク 3.2）が
/// 使うまでの seam である。
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
#[allow(dead_code)] // `Slot` と同じ seam（表 = 2.3 / 変更の適用 = 2.4 / 引き渡しの配線 = 3.2）。
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

#[allow(dead_code)] // `Slot` と同じ seam（表 = 2.3 / 変更の適用 = 2.4 / 引き渡しの配線 = 3.2）。
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

    /// 保持している文書を可変で借りる（タスク 2.4 の変更の適用口が使う）。
    ///
    /// **未保存の印と版はここでは変えない**（記録はタスク 2.4 の責務。モジュール docs
    /// 「書き込むが、記録しない」）。未解決なら [`SessionError::NoDocument`]。閉包の内側から
    /// セッションを呼び返してはならない（モジュール docs「再入禁止」）。
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
}

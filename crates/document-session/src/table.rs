//! ウィンドウ → セッションの表（tasks.md 2.3。design.md「DocumentSessions（公開面）」
//! 「Data Models → Logical Data Model」「File Structure Plan」。要件 1.1, 1.5, 3.6）。
//!
//! # 層の鎖（design.md「Architecture Pattern & Boundary Map」）
//!
//! 本モジュールは `error / state → session → change → table → api` の第 4 層である。
//! 参照するのは**左の層**（[`crate::error`] / [`crate::session`]）と、ウィンドウの識別子
//! [`WindowLabel`] の単一の定義を持つ上流 `app-shell` だけであり、`change` / `api` を知らない。
//!
//! # 表が保証すること（要件 1.1, 1.5）
//!
//! 1 つの [`WindowLabel`] につき [`Slot`] は**高々 1 つ**である（表の値は `Arc<Slot>`）。
//! [`Sessions::slot`] を何度呼んでも**同じ実体**が返り（`Arc::ptr_eq`）、別のウィンドウには
//! 別の実体が対応する。**同じ窓に 2 つの実体ができる競合は、挿入を書きロックの下で
//! 二重に確認して塞ぐ**（読みロックで見つからなかった 2 つのスレッドが同時に挿入路へ
//! 入っても、実体は 1 つしか残らない）。ウィンドウが破棄されたら [`Sessions::forget`] が
//! その窓のセッションだけを手放す（**他の窓の文書と未保存の状態を変えない**）。
//!
//! # ロックの規律（要件 3.6）
//!
//! 表のロックは、**表そのものへの参照（既存の確認）と、挿入・除去の間だけ**取る。
//! `Slot` の処理（読み込み・読み取り・変更の適用・保存）の間は**保持しない**: 保持すると、
//! 10 万行の変更の適用が終わるまで他のウィンドウが表のロックで待たされる（要件 3.6。
//! 「あるウィンドウの変更の適用が進行している間、他のウィンドウへの読み取り・変更・
//! 問い合わせを待たせない」）。したがって各メソッドは、`Slot` を触る前に Guard を落とし、
//! 貸すのは `Arc<Slot>` の複製だけである（セッションの操作は表のロックを必要としない）。
//! 除去では、取り出した `Arc` をロックの外で落とす（最後の参照だった場合の文書の解放
//! ——10 万行——を表のロックの下で走らせないため）。
//!
//! # 毒されたロック
//!
//! `RwLock` が毒されていても中身をそのまま使う（[`PoisonError::into_inner`]。[`crate::session`]
//! の `lock` / `try_lock` と `document-format` / `app-shell` の同じ規律）。panic した閉包の
//! 後でも、表（識別子 → セッションの対応）は構造的には壊れていない。
//!
//! # 表は操作の口を持たない
//!
//! 本モジュールが与えるのは**表の仕組み**（同じ窓には同じセッション・参照・挿入・除去）だけ
//! である。解決・読み取り・変更の適用・状態・保存を表のメソッドとして並べるのは公開面
//! （タスク 2.5 の `api`）の仕事であり、本モジュールは [`Slot`] を貸すだけに留める。
//! 表が経路ごとのメソッドを持つと、ロックの保持範囲がメソッドごとに散り、上の規律が
//! 守られているかを 1 箇所で読めなくなる。

use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use app_shell::ipc::WindowLabel;

use crate::session::Slot;

/// 表の中身（ウィンドウの識別子 → セッション）。
///
/// `HashMap` は反復順を持たないが、本モジュールは反復しない（識別子による参照・挿入・
/// 除去だけを行う）ため、決定性は問題にならない。
type Entries = HashMap<WindowLabel, Arc<Slot>>;

/// ウィンドウ → セッションの表（design.md「DocumentSessions（公開面）」の
/// 「`Sessions` は `Send + Sync`」と、構成要素 `SessionTable` の実体）。
///
/// 名前を `Sessions` としたのは、公開面（タスク 2.5）が組み立てる**操作口**の
/// `DocumentSessions` と、その下で動く**表そのもの**を混同しないためである（design.md の
/// 公開面は「セッションの表と操作口」の両方を `DocumentSessions` と呼ぶが、本クレートでは
/// 表（本型）と操作口（2.5）を別の型に分ける。決定はタスク 2.3 の報告に記す）。
///
/// 本型は crate 可視であり、根の再輸出には現れない（下流が使うのは公開面の名前だけである）。
pub(crate) struct Sessions {
    /// 識別子 → セッション。ロックの範囲はモジュール docs「ロックの規律」を参照。
    windows: RwLock<Entries>,
}

impl Sessions {
    /// 空の表を作る（どのウィンドウもドキュメントを持たない）。
    pub(crate) fn new() -> Self {
        Self {
            windows: RwLock::new(HashMap::new()),
        }
    }

    /// そのウィンドウのセッションを返す。**無ければ作って挿入する。**
    ///
    /// 同じウィンドウに対して何度呼んでも**同じ実体**を返す（要件 1.1 の「ウィンドウ 1 つに
    /// つきドキュメントを高々 1 つ」を、表の段で「セッション 1 つ」として保証する）。
    /// 既存の確認は読みロックで行い、挿入は書きロックの下で**もう一度**確認する: 読みロックで
    /// 見つからなかった複数のスレッドが同時に挿入路へ入っても、先に入った実体が残り、
    /// 後から入った側はそれを返す（同じ窓に 2 つの実体ができる競合を塞ぐ）。
    ///
    /// 表のロックは**この関数の中で閉じる**。返すのは `Arc<Slot>` の複製であり、Guard は
    /// 呼び出し元へ漏れない（モジュール docs「ロックの規律」）。
    pub(crate) fn slot(&self, window: &WindowLabel) -> Arc<Slot> {
        // 参照の経路: 読みロックで既存を探し、見つかれば**表のロックを離してから**貸す。
        if let Some(existing) = self.existing(window) {
            return existing;
        }

        // 挿入の経路: 書きロックの下で**もう一度**確認してから入れる。読みロックで見つから
        // なかった複数のスレッドが同時にここへ入っても、先に入った実体が残り、後から入った
        // 側は `or_insert_with` が返す既存の実体を受け取る（同じ窓に 2 つの実体ができる競合を
        // 塞ぐ）。
        let mut windows = self.windows_write();
        Arc::clone(
            windows
                .entry(window.clone())
                .or_insert_with(|| Arc::new(Slot::new())),
        )
    }

    /// そのウィンドウのセッションを**作らずに**参照する（無ければ `None`）。
    ///
    /// セッションを作るのは適応層の 3 つの入口（起動時に指定された位置の解決・利用者が
    /// 選んだ位置の引き渡し・新規作成）だけである（design.md「DocumentSessions」の
    /// Preconditions）。参照はこのどれでもないため、ここで表へセッションを挿入してはならない
    /// （挿入すると、破棄の購読を伴わないセッションが生まれる経路が開く）。表のロックは
    /// この関数の中で閉じる。
    pub(crate) fn existing(&self, window: &WindowLabel) -> Option<Arc<Slot>> {
        self.windows_read().get(window).map(Arc::clone)
    }

    /// そのウィンドウのセッションを表から除く（ウィンドウの破棄。要件 1.5）。
    ///
    /// **他のウィンドウのセッションには触れない**（表から 1 つ取り除くだけであり、他の値は
    /// そのまま残る）。除いたあとの同じウィンドウへの [`Sessions::slot`] は**新しい**
    /// （未解決の）セッションを作る。
    ///
    /// 取り出した `Arc` はロックを離してから落とす（最後の参照であれば、10 万行の文書の解放が
    /// 表のロックの下で走って他のウィンドウを待たせるため。モジュール docs「ロックの規律」）。
    pub(crate) fn forget(&self, window: &WindowLabel) {
        let mut windows = self.windows_write();
        let removed = windows.remove(window);
        drop(windows);
        drop(removed);
    }

    /// 表を読むロックを取る（毒されていても中身を使う。モジュール docs「毒されたロック」）。
    fn windows_read(&self) -> RwLockReadGuard<'_, Entries> {
        self.windows.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// 表を書くロックを取る（毒されていても中身を使う）。
    fn windows_write(&self) -> RwLockWriteGuard<'_, Entries> {
        self.windows.write().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    // テストとベンチが共有する標本の生成器（タスク 1.4）。`lib.rs` が `#[cfg(test)]` で
    // 取り込んでいる（`crate::common`。取り込みの理由は `lib.rs` の宣言を参照）。
    use crate::common;

    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use document_format::{CellValue, DocumentFormatApi};

    use crate::error::SessionError;
    use crate::state::{CloseAnswer, SessionState};

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

    /// 表のウィンドウを 1 つ与え、標本を 1 度読み込んだセッションを返す。
    fn resolved(sessions: &Sessions, window: &WindowLabel, path: &Path) -> Arc<Slot> {
        let slot = sessions.slot(window);
        slot.resolve(Some(path)).expect("標本を読み込める");
        slot
    }

    /// 標本の先頭のセル（他の窓の内容が変わっていないことの観測に使う）。
    fn first_cell(slot: &Slot) -> CellValue {
        slot.read(&mut |document| document.sheets()[0].rows()[0].values()[0].clone())
            .expect("文書を読める")
    }

    /// 先頭の行の値を 1 つ書き換える（変更の適用の代わり。記録はタスク 2.4 の責務である）。
    fn replace_first_cell(slot: &Slot, value: CellValue) {
        slot.with_document_mut(&mut |document| {
            let sheet = document.sheets()[0].id();
            let row = document.sheets()[0].rows()[0].id();
            document
                .set_cells(sheet, &[(row, 0, value.clone())])
                .expect("標本のセルを書き換えられる");
        })
        .expect("文書を可変で借りられる");
    }

    /// デッドロック検出のための待ち時間である（**性能の閾値ではない**。`verification.md` が
    /// 禁じているのは所要時間を速度の証拠に使う閾値であり、ここで見ているのは「待たずに
    /// 完了するか」だけである）。時間の閾値はこの 1 箇所の外に置かない（2 つのテストが
    /// 共有する）。
    const DEADLOCK_WAIT: Duration = Duration::from_secs(10);

    /// 一方のウィンドウの文書のロックを保持したまま `body` を実行し、その戻り値を返す。
    ///
    /// `body` は**別のスレッドで**走らせ、[`DEADLOCK_WAIT`] の内に完了することを観測する。
    /// 「他のウィンドウを待たせない」ことは、**完了すること**と返り値の正しさで観測される。
    /// ロックを共有する退行（セッションがロックを分けない・表が同じ実体を返す）を入れると
    /// `body` はそこで待つため、本関数は**ハングではなく失敗メッセージ付きで落ちる**。
    fn hold_lock_while<T: Send + 'static>(
        slot: &Arc<Slot>,
        body: impl FnOnce() -> T + Send + 'static,
    ) -> T {
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
        held_rx
            .recv_timeout(DEADLOCK_WAIT)
            .expect("一方の文書のロックが保持されなかった");

        // 観測する側も別のスレッドで走らせる（待ったままハングさせない）。
        let (done_tx, done_rx): (Sender<T>, Receiver<T>) = mpsc::channel();
        let worker = thread::spawn(move || {
            done_tx.send(body()).expect("観測側が待っている");
        });

        match done_rx.recv_timeout(DEADLOCK_WAIT) {
            Ok(value) => {
                release_tx.send(()).expect("解放を指示できる");
                holder.join().expect("保持側のスレッドが終わる");
                worker.join().expect("観測する側のスレッドが終わる");
                value
            }
            Err(_) => {
                // ロックを待っている（= セッションごとにロックが分かれていない）。
                // 保持側を解放してから、失敗として落とす（ハングさせない）。
                drop(release_tx);
                holder.join().ok();
                panic!("他のウィンドウの操作が待たされた（セッションごとにロックが分かれていない）");
            }
        }
    }

    #[test]
    fn slot_returns_the_same_session_for_the_same_window() {
        let sessions = Sessions::new();
        let window = WindowLabel::new("same");
        let other = WindowLabel::new("other");

        let first = sessions.slot(&window);
        let second = sessions.slot(&window);
        assert!(
            Arc::ptr_eq(&first, &second),
            "同じウィンドウに 2 回目のアクセスで別のセッションができた"
        );
        assert!(
            Arc::ptr_eq(
                &first,
                &sessions.existing(&window).expect("表にある")
            ),
            "参照が挿入した実体と別のものを返した"
        );

        assert!(sessions.existing(&other).is_none(), "作っていない窓が表にある");
        let other_slot = sessions.slot(&other);
        assert!(
            !Arc::ptr_eq(&first, &other_slot),
            "別のウィンドウが同じセッションを共有した"
        );
        assert!(
            Arc::ptr_eq(&other_slot, &sessions.slot(&other)),
            "別のウィンドウの 2 回目のアクセスで実体が変わった"
        );
        assert_eq!(2, sessions.windows_read().len(), "表の大きさが合わない");
    }

    #[test]
    fn concurrent_requests_for_the_same_window_yield_one_session() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        // 全スレッドを `slot()` の手前で**スピンで**揃えてから走らせる。これにより
        // **二重確認の無い挿入**（読みロックで見つからず、書きロックの下で確認せずに
        // 無条件で入れる形）が、全員が空の表に対して既存の確認を行ってから挿入路へ入る形で
        // 現れ、取り逃さず検出できる。
        //
        // `Barrier` では取り逃した（起床が futex 経由のため、最初に起きたスレッドが挿入を
        // 終えてから最後のスレッドが確認へ入ることがあり、8 スレッドで 10 回中 2 回、
        // 退行が通った）。ここで見ているのは「同時に入る形」であり、時間の閾値は使わない。
        const THREADS: usize = 8;
        let arrived = Arc::new(AtomicUsize::new(0));
        let go = Arc::new(AtomicBool::new(false));
        let sessions = Arc::new(Sessions::new());
        let window = Arc::new(WindowLabel::new("racing"));
        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                let sessions = Arc::clone(&sessions);
                let window = Arc::clone(&window);
                let arrived = Arc::clone(&arrived);
                let go = Arc::clone(&go);
                thread::spawn(move || {
                    arrived.fetch_add(1, Ordering::SeqCst);
                    while !go.load(Ordering::Acquire) {
                        std::hint::spin_loop();
                    }
                    sessions.slot(&window)
                })
            })
            .collect();
        while arrived.load(Ordering::Acquire) != THREADS {
            std::hint::spin_loop();
        }
        go.store(true, Ordering::Release);

        let slots: Vec<Arc<Slot>> = handles
            .into_iter()
            .map(|handle| handle.join().expect("求めた側のスレッドが終わる"))
            .collect();

        let first = &slots[0];
        assert!(
            slots.iter().all(|slot| Arc::ptr_eq(first, slot)),
            "同じウィンドウを同時に求めた複数のスレッドに別の実体が返った"
        );
        assert_eq!(1, sessions.windows_read().len(), "同じ窓の実体が複数残った");
    }

    #[test]
    fn a_loaded_document_leaves_another_window_unresolved() {
        let scratch = common::Scratch::new("table-separate");
        let path = write_sample(&scratch, "sample.jxcel", 3, 2);
        let sessions = Sessions::new();
        let loaded_label = WindowLabel::new("loaded");
        let blank_label = WindowLabel::new("blank");
        let loaded = sessions.slot(&loaded_label);
        let blank = sessions.slot(&blank_label);

        loaded.resolve(Some(&path)).expect("標本を読み込める");

        assert!(
            matches!(loaded.state(), SessionState::Open { .. }),
            "読み込んだ窓がドキュメントを保持していない: {:?}",
            loaded.state()
        );
        assert_eq!(
            SessionState::Absent,
            blank.state(),
            "触っていない窓がドキュメントを保持している"
        );
        assert!(
            matches!(blank.read(&mut |_document| ()), Err(SessionError::NoDocument)),
            "触っていない窓の文書が読めた"
        );
    }

    #[test]
    fn operations_in_one_window_leave_a_window_without_a_document_absent() {
        let scratch = common::Scratch::new("table-opened-and-blank");
        let path = write_sample(&scratch, "sample.jxcel", 4, 3);
        let sessions = Sessions::new();
        let opened_label = WindowLabel::new("opened");
        let blank_label = WindowLabel::new("blank");
        let opened = resolved(&sessions, &opened_label, &path);
        let blank = sessions.slot(&blank_label);

        // 触っていない窓は表に居るがドキュメントを持たない（`Absent` は「まだ解決していない」
        // であり、版を進める経路——読み込み・引き渡し・新規作成——が 1 度も走っていない）。
        assert_eq!(SessionState::Absent, blank.state());
        assert_eq!(CloseAnswer::Allow, blank.may_close(), "未保存になった");

        // 一方で変更を適用し、保存し、破棄の印を落とす。
        replace_first_cell(&opened, CellValue::Text("changed".to_owned()));
        assert!(
            matches!(opened.save(), Ok(crate::error::SaveReport::Saved { .. })),
            "出所へ保存できない"
        );
        opened.discard().expect("文書があるので破棄の印は通る");

        // 触っていない窓は同じ実体のまま、状態・未保存・読み取りの答えが変わらない。
        // （版は `SessionState` が運ばず `Slot` の私有であるため表の側からは読めない。
        // 版を進める経路は必ず文書を据えるので、実体・状態・内容が不変であることが
        // 版が動いていないことを捉える。）
        assert!(
            Arc::ptr_eq(
                &blank,
                &sessions.existing(&blank_label).expect("表にある")
            ),
            "触っていない窓のセッションが差し替わった"
        );
        assert_eq!(
            SessionState::Absent,
            blank.state(),
            "触っていない窓の状態が変わった"
        );
        assert_eq!(
            CloseAnswer::Allow,
            blank.may_close(),
            "触っていない窓が未保存になった"
        );
        assert!(
            matches!(blank.read(&mut |_document| ()), Err(SessionError::NoDocument)),
            "触っていない窓の文書が読めた"
        );
    }

    #[test]
    fn changes_saves_and_discards_in_one_window_do_not_touch_another_window() {
        let scratch = common::Scratch::new("table-two-documents");
        let first_path = write_sample(&scratch, "first.jxcel", 4, 3);
        let second_path = write_sample(&scratch, "second.jxcel", 5, 2);
        let sessions = Sessions::new();
        let first_label = WindowLabel::new("first");
        let second_label = WindowLabel::new("second");
        let first = resolved(&sessions, &first_label, &first_path);
        let second = resolved(&sessions, &second_label, &second_path);

        let second_state = second.state();
        let second_cell = first_cell(&second);

        let changed = CellValue::Text("changed".to_owned());
        replace_first_cell(&first, changed.clone());
        assert_eq!(
            changed,
            first_cell(&first),
            "変更が適用されていない（比較の前提が崩れている）"
        );
        assert!(
            matches!(first.save(), Ok(crate::error::SaveReport::Saved { .. })),
            "出所へ保存できない"
        );
        first.discard().expect("文書があるので破棄の印は通る");

        // 他方は同じ実体のまま、状態（未保存の有無を含む）も内容も変わらない。
        assert!(
            Arc::ptr_eq(
                &second,
                &sessions.existing(&second_label).expect("表にある")
            ),
            "他方のセッションが差し替わった"
        );
        assert_eq!(second_state, second.state(), "他方の状態が変わった");
        assert_eq!(
            second_cell,
            first_cell(&second),
            "他方の内容が変わった"
        );
        assert_eq!(
            CloseAnswer::Allow,
            second.may_close(),
            "他方が未保存になった"
        );
    }

    #[test]
    fn forget_removes_only_that_window_and_the_next_access_is_a_fresh_session() {
        let scratch = common::Scratch::new("table-forget");
        let forgotten_path = write_sample(&scratch, "forgotten.jxcel", 3, 2);
        let kept_path = write_sample(&scratch, "kept.jxcel", 4, 2);
        let sessions = Sessions::new();
        let forgotten_label = WindowLabel::new("forgotten");
        let kept_label = WindowLabel::new("kept");
        let forgotten = resolved(&sessions, &forgotten_label, &forgotten_path);
        let kept = resolved(&sessions, &kept_label, &kept_path);

        let kept_state = kept.state();
        let kept_cell = first_cell(&kept);

        // 表に無い窓を忘れても何も起きない。
        sessions.forget(&WindowLabel::new("ghost"));
        assert!(sessions.existing(&kept_label).is_some(), "居ない窓の除去で表が壊れた");

        sessions.forget(&forgotten_label);
        assert!(
            sessions.existing(&forgotten_label).is_none(),
            "忘れた窓が表に残っている"
        );

        // 忘れたあとのアクセスは**新しい**（未解決の）セッションになる。
        let fresh = sessions.slot(&forgotten_label);
        assert!(
            !Arc::ptr_eq(&forgotten, &fresh),
            "忘れた窓に古い実体が返った"
        );
        assert_eq!(
            SessionState::Absent,
            fresh.state(),
            "新しいセッションがドキュメントを保持している"
        );
        fresh
            .resolve(Some(&forgotten_path))
            .expect("忘れた窓は読み直せる（新しいセッションとして）");
        assert!(matches!(fresh.state(), SessionState::Open { .. }));

        // 他のウィンドウのセッション・文書・未保存は変わらない。
        assert!(
            Arc::ptr_eq(&kept, &sessions.existing(&kept_label).expect("表にある")),
            "他の窓のセッションが差し替わった"
        );
        assert_eq!(kept_state, kept.state(), "他の窓の状態が変わった");
        assert_eq!(kept_cell, first_cell(&kept), "他の窓の内容が変わった");
        assert_eq!(CloseAnswer::Allow, kept.may_close(), "他の窓が未保存になった");
    }

    #[test]
    fn a_held_document_lock_does_not_block_another_window() {
        let scratch = common::Scratch::new("table-lock-isolation");
        let first_path = write_sample(&scratch, "first.jxcel", 3, 3);
        let second_path = write_sample(&scratch, "second.jxcel", 4, 2);
        // 観測する側を別のスレッドで走らせるため `Arc` で持つ（`body` を `'static` にする）。
        let sessions = Arc::new(Sessions::new());
        let first_label = WindowLabel::new("first");
        let second_label = WindowLabel::new("second");
        let first = resolved(&sessions, &first_label, &first_path);
        let second = resolved(&sessions, &second_label, &second_path);

        // 一方の文書のロックを保持したまま、他方のセッションの操作（参照・読み取り・状態・
        // 変更の適用・保存）が完了する（要件 3.6。ロックはセッションごとに分かれている）。
        // ロックを共有する退行を入れると、この本体は待たされ、[`DEADLOCK_WAIT`] の内に
        // 完了しないため、ハングせずに失敗として落ちる。
        let observed_sessions = Arc::clone(&sessions);
        let observed_second = Arc::clone(&second);
        hold_lock_while(&first, move || {
            assert!(
                Arc::ptr_eq(
                    &observed_second,
                    &observed_sessions
                        .existing(&second_label)
                        .expect("表にある")
                ),
                "参照が他方の実体を返さない"
            );
            assert_eq!(
                4,
                observed_second
                    .read(&mut |document| document.sheets()[0].rows().len())
                    .expect("読み取れる"),
            );
            assert!(
                matches!(observed_second.state(), SessionState::Open { .. }),
                "状態が読めない: {:?}",
                observed_second.state()
            );
            replace_first_cell(&observed_second, CellValue::Text("changed".to_owned()));
            assert!(
                matches!(
                    observed_second.save(),
                    Ok(crate::error::SaveReport::Saved { .. })
                ),
                "保存できない"
            );
        });
    }

    #[test]
    fn a_session_operation_does_not_need_the_table_lock() {
        let scratch = common::Scratch::new("table-lock-not-needed");
        let path = write_sample(&scratch, "sample.jxcel", 3, 2);
        let sessions = Sessions::new();
        let window = WindowLabel::new("window");
        let slot = resolved(&sessions, &window, &path);

        // 表の書きロックを**保持したまま**、セッションの操作が完了することを観測する。
        // 表が貸すのは `Arc<Slot>` であり、セッションの操作は表へ戻らない（表のロックを
        // スロットの処理の間ずっと保持する実装なら、ここで待って完了しない）。
        let table = sessions.windows_write();
        let (done_tx, done_rx): (Sender<()>, Receiver<()>) = mpsc::channel();
        let worker = {
            let slot = Arc::clone(&slot);
            thread::spawn(move || {
                assert_eq!(
                    3,
                    slot.read(&mut |document| document.sheets()[0].rows().len())
                        .expect("読み取れる"),
                );
                assert!(matches!(slot.state(), SessionState::Open { .. }), "状態が読めない");
                replace_first_cell(&slot, CellValue::Text("changed".to_owned()));
                assert!(
                    matches!(slot.save(), Ok(crate::error::SaveReport::Saved { .. })),
                    "保存できない"
                );
                done_tx.send(()).expect("観測側が待っている");
            })
        };

        let completed = done_rx.recv_timeout(DEADLOCK_WAIT);
        drop(table);
        worker.join().expect("操作する側のスレッドが終わる");
        completed.expect("表のロックを保持している間にセッションの操作が完了しなかった");
    }

    /// 適応層が表とセッションをスレッドを跨いで保持できる（design.md「Implementation Notes」）。
    #[test]
    fn sessions_and_slots_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Sessions>();
        assert_send_sync::<Slot>();
    }
}

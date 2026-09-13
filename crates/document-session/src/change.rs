//! 変更の適用の唯一の口（tasks.md 2.4。design.md「ChangeApply」「System Flows → 変更の適用
//! （下流からの一括を含む）」。要件 3.1, 3.2, 3.3, 3.4, 4.1, 4.2）。
//!
//! # 層の鎖（design.md「Architecture Pattern & Boundary Map」）
//!
//! 本モジュールは `error / state → session → change → table → api` の第 3 層である。
//! **production の参照**は**左の層**（[`crate::error`] / [`crate::state`] / [`crate::session`]）
//! と上流 `document-format` だけであり、`table` / `api` を知らない。テストだけが、2 つの窓の
//! 独立性を観測するために `table` の `Sessions` を使う（production の依存ではない）。
//!
//! # 変更の語彙を持たない（design.md「ChangeApply」）
//!
//! 本モジュールが定めるのは**閉包を貸す口**だけである。何をどう変えるかを表す型は置かず、
//! 変更の正否も判定しない（値がスキーマに適合するかの判断は `schema-engine` の所有である）。
//! `document-format` の [`Document::set_cells`] などを呼ぶのは**閉包の側**であり、本モジュールは
//! 閉包が返した値をそのまま [`Edited::value`] として運ぶ（要件 3.2）。
//!
//! # 判定・閉包・記録を 1 つの臨界区間に入れる（要件 3.1, 3.3, 4.1）
//!
//! [`edit`] は文書のロックを**保持したまま**閉包へ可変参照を渡し、**同じ臨界区間の内側で**
//! 未保存の印を立て、版を 1 進める（design.md「Slot」の不変条件「未保存と版の更新は文書の
//! ロックを保持したまま行う」）。判定（文書を保持しているか）・閉包の実行・記録を別々の
//! 臨界区間に分けると、「未保存でない」と判定した直後に差し替えが入り、適用した変更が
//! 黙って失われる（2.3 のレビューで見つかった TOCTOU をここへ作り込まない）。
//! 借用は呼び出しの外へ出ない（閉包へ貸すのはロックの内側だけであり、`&mut Document` は
//! [`edit`] の戻り値に現れない）。
//!
//! # 閉包が失敗を返しても印を立てる（design.md「ChangeApply」）
//!
//! 閉包の戻り値は型 `R` のままであり、本モジュールは**中身を判定しない**。したがって
//! `R = Result<_, _>` の閉包が `Err` を返した場合も、**未保存の印と版の更新は行われる**
//! （閉包が文書を変更したかどうかを本モジュールは判定できず、変更されていた場合に印が無いと
//! 差し替えで変更が黙って失われるため、**保守側に倒す**）。失敗は [`Edited::value`] が運び、
//! 呼び出し元が受け取る（部分的な適用を成功として報告しない。要件 3.3）。
//!
//! # 再入禁止
//!
//! **閉包の内側から同じセッションを呼び返してはならない。** [`edit`] は閉包の実行中ずっと
//! 文書のロックを保持するため、閉包の内側から `read` / `state` / `save` / `save_to` /
//! `discard` などのロックを待つ操作を呼ぶと、`Mutex` は再入できないためデッドロックする
//! （design.md「Risks & Mitigations」）。閉包は渡された `&mut Document` の内側だけで完結する。
//!
//! 適用中の読み取り（要件 3.4）
//!
//! 読み取り（`Slot::read`）は同じロックを共有する。適用中に到着した読み取りは適用の完了を
//! 待ち、**適用済みの最新**を見る。本モジュールはその共有を新たに設けず、`session` の
//! ロックの規律（モジュール docs「ロックの規律」）に従う。

use document_format::Document;

use crate::error::SessionError;
use crate::session::{Inner, Slot};
use crate::state::Edited;

/// 保持している文書に変更を適用する（tasks.md 2.4。要件 3.1〜3.4, 4.1, 4.2）。
///
/// 文書のロックを**保持したまま**閉包へ可変参照を渡し、**同じ臨界区間の内側で**未保存の印を
/// 立て、版を 1 進める（モジュール docs「判定・閉包・記録を 1 つの臨界区間に入れる」）。
/// 閉包の戻り値はそのまま [`Edited::value`] として返る。借用は呼び出しの外へ出ない。
///
/// **判定・閉包・記録を 1 つの Guard の生存範囲に入れることは本関数の契約である**: 途中で
/// Guard を解放して取り直す形（記録を別の臨界区間へ移す形）へ変えてはならない（`Slot` の
/// `record_edit` は「その時点でロックを保持している」ことしか型で要求しないため、この契約は
/// ここで守る）。
///
/// 閉包が失敗を返しても（`R` が `Err` でも）印は立ち、版は進む（モジュール docs「閉包が失敗を
/// 返しても印を立てる」）。
///
/// 未解決（生成要求をまだ読んでいない）または保持していない（読み込みに失敗したまま）の
/// セッションでは [`SessionError::NoDocument`] を返し、**閉包を呼ばない**。
///
/// **閉包の内側から同じセッションを呼び返してはならない**（モジュール docs「再入禁止」）。
pub(crate) fn edit<R>(
    slot: &Slot,
    f: &mut dyn FnMut(&mut Document) -> R,
) -> Result<Edited<R>, SessionError> {
    // 臨界区間の開始: この Guard が生きている間、同じセッションの読み取り・状態の取得・保存・
    // 差し替えはロックを待つ。判定（文書を保持しているか）もこの内側で行う。
    let mut inner = slot.lock_for_change();
    let Inner::Resolved { document, .. } = &mut *inner else {
        // 保持していない: 閉包は呼ばず、差し替えも記録も行わない。
        return Err(SessionError::NoDocument);
    };
    // 閉包の実行（ロックを保持したまま可変参照を貸す。借用はこの式で終わる）。
    let value = f(document);
    // 記録も同じ Guard を保持したまま行う（`_lock` が同じ臨界区間であることの証である）。
    Ok(slot.record_edit(&inner, value))
}

#[cfg(test)]
mod tests {
    // テストとベンチが共有する標本の生成器（タスク 1.4）。`lib.rs` が `#[cfg(test)]` で
    // 取り込んでいる（`crate::common`。取り込みの理由は `lib.rs` の宣言を参照）。
    use crate::common;

    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use app_shell::ipc::WindowLabel;
    use document_format::{CellValue, Document, DocumentFormatApi};

    use crate::error::SessionError;
    use crate::session::Slot;
    use crate::state::{CloseAnswer, SessionState};
    use crate::table::Sessions;

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

    /// 標本を 1 度読み込んだ（解決済みの）セッションを返す。
    fn resolved_slot(path: &Path) -> Slot {
        let slot = Slot::new();
        slot.resolve(Some(path)).expect("標本を読み込める");
        slot
    }

    /// 先頭のセルを読む（他の窓の内容が変わっていないことの観測にも使う）。
    fn first_cell(slot: &Slot) -> CellValue {
        slot.read(&mut |document| document.sheets()[0].rows()[0].values()[0].clone())
            .expect("文書を読める")
    }

    /// 先頭のセルを書き換える（**変更の語彙は要求側の所有である**ためテストの側に置く。
    /// design.md「ChangeApply」）。
    fn write_first_cell(document: &mut Document, value: &CellValue) {
        let sheet = document.sheets()[0].id();
        let row = document.sheets()[0].rows()[0].id();
        document
            .set_cells(sheet, &[(row, 0, value.clone())])
            .expect("標本のセルを書き換えられる");
    }

    #[test]
    fn edit_applies_the_closure_and_returns_its_value() {
        let scratch = common::Scratch::new("change-apply");
        let path = write_sample(&scratch, "sample.jxcel", 4, 3);
        let slot = resolved_slot(&path);

        let before = first_cell(&slot);
        let replacement = CellValue::Text("changed".to_owned());
        let edited = edit(&slot, &mut |document| {
            write_first_cell(document, &replacement);
            // 閉包の戻り値はそのまま `Edited::value` として返る（要件 3.2）。
            document.sheets()[0].rows()[0].values()[0].clone()
        })
        .expect("適用できる");

        assert_eq!(
            replacement, edited.value,
            "閉包の戻り値がそのまま返っていない"
        );
        assert_ne!(before, edited.value, "変更が値に現れていない（比較の前提）");
        assert_eq!(
            replacement,
            first_cell(&slot),
            "変更が保持している文書に反映されていない"
        );
    }

    #[test]
    fn edit_advances_the_revision_by_one_and_marks_unsaved() {
        let scratch = common::Scratch::new("change-record");
        let path = write_sample(&scratch, "sample.jxcel", 3, 2);
        let slot = resolved_slot(&path);

        // 前提: 読み込みの完了で版はちょうど 1 進み（タスク 2.1 が固定）、未保存は落ちている。
        assert!(
            matches!(slot.state(), SessionState::Open { unsaved: false, .. }),
            "読み込みの直後が未保存である"
        );
        assert_eq!(
            CloseAnswer::Allow,
            slot.may_close(),
            "読み込みの直後に閉じられない"
        );

        let replacement = CellValue::Text("changed".to_owned());
        let first = edit(&slot, &mut |document| {
            write_first_cell(document, &replacement)
        })
        .expect("適用できる");
        assert_eq!(
            2, first.revision,
            "適用で版がちょうど 1 進んでいない（読み込みの完了の 1 からの差分）"
        );
        assert!(first.unsaved, "適用で未保存の印が立っていない");
        assert_eq!(
            CloseAnswer::Deny,
            slot.may_close(),
            "適用の後に閉じてよいと答えた"
        );
        assert!(
            matches!(slot.state(), SessionState::Open { unsaved: true, .. }),
            "適用で未保存の印が立っていない"
        );

        let second = edit(&slot, &mut |_document| ()).expect("適用できる");
        assert_eq!(
            3, second.revision,
            "2 回目の適用で版がちょうど 1 進んでいない"
        );
        assert!(second.unsaved);
    }

    #[test]
    fn edit_marks_unsaved_even_when_the_closure_reports_failure() {
        let scratch = common::Scratch::new("change-failure");
        let path = write_sample(&scratch, "sample.jxcel", 3, 2);
        let slot = resolved_slot(&path);
        let before = first_cell(&slot);

        // 範囲外の列を指定した一括の書き換えは `CellWriteError` を返す（`set_cells` は 1 つでも
        // 不正ならどのセルも変えない）。閉包は失敗を返すが、本モジュールは印を立てて版を進める
        // （保守側に倒す。design.md「ChangeApply」）。
        let edited = edit(&slot, &mut |document| {
            let sheet = document.sheets()[0].id();
            let row = document.sheets()[0].rows()[0].id();
            document.set_cells(sheet, &[(row, usize::MAX, CellValue::Null)])
        })
        .expect("適用の口は閉包の成否を失敗として扱わない");

        assert!(edited.value.is_err(), "閉包の失敗がそのまま返っていない");
        assert!(edited.unsaved, "閉包の失敗で未保存の印が立っていない");
        assert_eq!(
            2, edited.revision,
            "閉包の失敗で版がちょうど 1 進んでいない"
        );
        assert_eq!(before, first_cell(&slot), "失敗した閉包が文書を変えた");
    }

    #[test]
    fn reads_and_state_do_not_change_the_revision_or_the_unsaved_mark() {
        let scratch = common::Scratch::new("change-read-only");
        let path = write_sample(&scratch, "sample.jxcel", 3, 2);
        let slot = resolved_slot(&path);

        // 読み取りだけでは未保存にならない（要件 4.2）。
        for _ in 0..3 {
            let rows = slot
                .read(&mut |document| document.sheets()[0].rows().len())
                .expect("読み取れる");
            assert_eq!(3, rows);
            assert!(
                matches!(slot.state(), SessionState::Open { unsaved: false, .. }),
                "読み取りが未保存の印を立てた"
            );
            assert_eq!(
                CloseAnswer::Allow,
                slot.may_close(),
                "読み取りが閉じてよくした"
            );
        }

        // 版を運ぶのは適用の戻り値だけである（状態の写しは版を持たない）。適用の直後に同じ
        // 読み取りを行い、**前後の戻り値の差がちょうど 1** であることを観測する: 読み取りや
        // 状態の取得が版を進める実装なら、差は 2 以上になる。
        let first = edit(&slot, &mut |_document| ()).expect("適用できる");
        for _ in 0..3 {
            let _ = slot.read(&mut |document| document.sheets()[0].rows().len());
            let _ = slot.state();
            let _ = slot.may_close();
        }
        let second = edit(&slot, &mut |_document| ()).expect("適用できる");

        assert_eq!(
            first.revision + 1,
            second.revision,
            "読み取りと状態の取得が版を進めた（適用の前後の差が 1 でない）"
        );
        assert!(second.unsaved, "適用の後に未保存でなくなった");
        assert!(
            matches!(slot.state(), SessionState::Open { unsaved: true, .. }),
            "適用の後に未保存でなくなった"
        );
    }

    #[test]
    fn a_slot_without_a_document_does_not_call_the_closure_and_returns_no_document() {
        // 未解決のセッション（生成要求が無い窓）。
        let unresolved = Slot::new();
        // 読み込みに失敗したままのセッション（理由を覚えている）。
        let scratch = common::Scratch::new("change-no-document");
        let missing = scratch.file("missing.jxcel");
        let unavailable = Slot::new();
        assert!(
            unavailable.resolve(Some(&missing)).is_err(),
            "存在しない位置の読み込みが成功した"
        );
        assert!(matches!(
            unavailable.state(),
            SessionState::Unavailable { .. }
        ));

        for (slot, what) in [(&unresolved, "未解決"), (&unavailable, "保持なし")] {
            // 閉包が呼ばれたら観測できるようにする（呼ばれれば印が立つ）。
            let called = AtomicBool::new(false);
            let outcome = edit(slot, &mut |_document| {
                called.store(true, Ordering::SeqCst);
            });

            assert!(
                matches!(outcome, Err(SessionError::NoDocument)),
                "{what}のセッションが NoDocument を返さない"
            );
            assert!(!called.load(Ordering::SeqCst), "{what}: 閉包が呼ばれた");
        }
    }

    #[test]
    fn a_read_waits_for_an_apply_in_progress_and_sees_the_applied_value() {
        /// デッドロック検出のための待ち時間である（**性能の閾値ではない**。`verification.md` が
        /// 禁じているのは所要時間を速度の証拠に使う閾値であり、ここで見ているのは「適用中に
        /// 到着した読み取りが適用の完了を待つか」だけである）。時間の閾値はこのテストの外に
        /// 置かない。
        const DEADLOCK_WAIT: Duration = Duration::from_secs(10);

        let scratch = common::Scratch::new("change-read-waits");
        let path = write_sample(&scratch, "sample.jxcel", 3, 2);
        let slot = Arc::new(Slot::new());
        slot.resolve(Some(&path)).expect("標本を読み込める");

        let replacement = CellValue::Text("applied".to_owned());
        // 適用側: 閉包の内側でセルを書き換え、**ロックを保持したまま**解放を待つ。
        let (applying_tx, applying_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let applier = {
            let slot = Arc::clone(&slot);
            let replacement = replacement.clone();
            thread::spawn(move || {
                edit(&slot, &mut |document| {
                    write_first_cell(document, &replacement);
                    applying_tx.send(()).expect("観測側が待っている");
                    release_rx
                        .recv_timeout(DEADLOCK_WAIT * 2)
                        .expect("待ち時間内に解放される");
                    replacement.clone()
                })
                .expect("適用できる")
            })
        };
        applying_rx
            .recv_timeout(DEADLOCK_WAIT)
            .expect("待ち時間内に適用が始まる");

        // 適用中に到着した読み取りを別のスレッドで走らせる。同じロックを共有する実装なら、
        // 解放までは返らない（待たずに返る実装なら、下の待ちが成功してテストが落ちる）。
        let (read_tx, read_rx): (Sender<CellValue>, Receiver<CellValue>) = mpsc::channel();
        let reader = {
            let slot = Arc::clone(&slot);
            thread::spawn(move || {
                let value = slot
                    .read(&mut |document| document.sheets()[0].rows()[0].values()[0].clone())
                    .expect("読み取れる");
                read_tx.send(value).expect("観測側が待っている");
            })
        };
        let early = read_rx.recv_timeout(DEADLOCK_WAIT);

        release_tx.send(()).expect("解放を指示できる");
        let edited = applier.join().expect("適用側のスレッドが終わる");
        assert!(
            early.is_err(),
            "読み取りが適用の完了を待たずに返った（デッドロック検出の時間切れの前に返った）"
        );
        assert_eq!(
            replacement, edited.value,
            "適用の戻り値が閉包の戻り値でない"
        );

        let seen = read_rx
            .recv_timeout(DEADLOCK_WAIT)
            .expect("解放後に読み取りが完了する");
        assert_eq!(
            replacement, seen,
            "解放後の読み取りが適用済みの最新を見ていない"
        );
        reader.join().expect("読み取る側のスレッドが終わる");
    }

    #[test]
    fn an_edit_in_one_window_leaves_the_other_windows_unsaved_mark_alone() {
        let scratch = common::Scratch::new("change-two-windows");
        let first_path = write_sample(&scratch, "first.jxcel", 3, 2);
        let second_path = write_sample(&scratch, "second.jxcel", 4, 3);
        let sessions = Sessions::new();
        let first_label = WindowLabel::new("first");
        let second_label = WindowLabel::new("second");
        let first = sessions.slot(&first_label);
        let second = sessions.slot(&second_label);
        first.resolve(Some(&first_path)).expect("標本を読み込める");
        second
            .resolve(Some(&second_path))
            .expect("標本を読み込める");

        let second_state = second.state();
        let second_cell = first_cell(&second);

        let replacement = CellValue::Text("changed".to_owned());
        let edited = edit(&first, &mut |document| {
            write_first_cell(document, &replacement);
        })
        .expect("適用できる");

        // 適用した窓には印が立つ（比較の前提。2.3 では印を立てる経路が無く、この観測が
        // 空振りだった。tasks.md 2.4 の申し送り）。
        assert!(edited.unsaved, "適用した窓に未保存の印が立っていない");
        assert_eq!(CloseAnswer::Deny, first.may_close());
        assert_eq!(
            replacement,
            first_cell(&first),
            "適用が保持している文書に届いていない"
        );

        // 他方の窓は未保存にならず、状態も内容も変わらない（要件 1.1, 3.6）。
        assert_eq!(second_state, second.state(), "他方の窓の状態が変わった");
        assert!(
            matches!(second.state(), SessionState::Open { unsaved: false, .. }),
            "他方の窓が未保存になった"
        );
        assert_eq!(second_cell, first_cell(&second), "他方の窓の内容が変わった");
        assert_eq!(
            CloseAnswer::Allow,
            second.may_close(),
            "他方の窓が閉じてよくなくなった"
        );
    }
}

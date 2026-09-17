//! 時間とメモリの上限の適用・打ち切り・種類の記録・復帰（tasks.md 1.5。要件 6.1–6.4）。
//!
//! 2 つの上限は**同じ 1 つの終了経路に合流する**（`research.md`「実行の打ち切りとメモリ上限」）。
//! V8 が持つ打ち切りの機構は `terminate_execution` 1 つであり、時間はタイマースレッドから、
//! メモリは near-heap-limit callback の中から、その同じ口を叩く。**どちらで止めたか**は
//! 1 つの `AtomicU8` に記録し、`RunOutcome::Aborted` の `limit` に載る（種別を outcome に
//! 載せるのはタスク 1.3 の型である）。
//!
//! # 実測に基づく形（`deno_core` 0.412 の `runtime/tests/misc.rs`）
//!
//! | 上限 | 形 | 根拠 |
//! |------|----|------|
//! | 時間 | `thread_safe_handle()` の clone をタイマースレッドへ渡し、期限で `terminate_execution()` | `terminate_execution`（357–386 行） |
//! | メモリ | `create_params` の `heap_limits(0, max)` ＋ `add_near_heap_limit_callback` の中で `terminate_execution()` | `test_heap_limits`（566–590 行） |
//!
//! どちらの打ち切りも、`execute_script` からは `"Uncaught Error: execution terminated"` の
//! 例外として戻る（`tech.md`「Known Risks」1 と同じ形である）。**実測（本クレートのテスト）**:
//! 時間の上限（150 ms、`for (;;) {}`）は 159 ms で、メモリの上限（8 MiB、文字列の連結）は
//! 18 ms で打ち切られ、どちらも `MacroFailure` は `Error: execution terminated`・フレーム
//! 無しになる（V8 の打ち切りは位置を持たない）。**どちらの上限かは `RunOutcome::Aborted` の
//! `limit` が運ぶ**ので、提示（4.4）は理由の文言ではなくこの種類を使う。
//!
//! **callback を付け忘れると V8 がプロセスを abort させる**（`v8::CreateParams::heap_limits`
//! の doc: 「callback が上限を上げなければ V8 は `FatalProcessOutOfMemory` で落ちる」）。
//! したがってメモリの上限を使うときは必ず callback を付け、**callback は上限を上げてから
//! terminate へ合流する** — 上げないと、打ち切りが届く前に V8 がプロセスを落とす。
//!
//! # 打ち切りの後始末と復帰
//!
//! - 実行の**前**に `cancel_terminate_execution()` を呼ぶ（design.md「MacroActor / isolate /
//!   limits」の要求）。isolate は実行ごとに作るので持ち越しは構造的に起きないが、手順としても
//!   保証する
//! - 実行の**後**、isolate を落とす前に `cancel_terminate_execution()` を呼ぶ（deno_core の
//!   `terminate_execution` のテストが「打ち切り → cancel → 同じ isolate を再び使える」を
//!   固定している。打ち切りの状態のまま片付けない）
//! - **actor の状態（実行中の印）を戻すのは actor の仕事**である（1.4）。本モジュールは
//!   isolate とタイマーだけを畳み、同じ actor が次の実行を受けられる状態にする
//!
//! # 上限の端の値
//!
//! - 時間が 0 のときは、タイマーが待たずに期限へ達する（その実行が実際に打ち切られるかは、
//!   実行が始まる前か後かという競合で決まる。上限が 0 である以上、成功は保証しない）。
//! - 時間が 1 日を超えるときは**タイマーを 1 日で切る**（1 日を超える実行は 1 日で
//!   打ち切られる）。`recv_timeout` は `Instant + Duration` が表現できない大きさで panic
//!   するためであり、既定は 30 秒で、それに近い値しか来ないという見込みのうえの頭打ちである
//!   （設定がどんな値を許すかはタスク 4.3 の範囲である）。
//! - メモリが 0 のときは V8 の既定の上限になる（`heap_limits(0, 0)` は V8 の既定を意味し、
//!   本層は 0 を「上限なし」ではなく「V8 の既定」として扱う。設定の検証は 4.3 の仕事である）。
//!   callback はその場合でも張る — V8 自身の既定の上限に当たったときに、プロセスの abort
//!   ではなく**打ち切り**として戻すためである。
//! - メモリの上限が**小さすぎる**場合（isolate の起動そのものが上限へ達する大きさ）は、
//!   callback を張る前に V8 が `FatalProcessOutOfMemory` を出しうる（この経路だけは
//!   プロセスの abort になる。deno_core の `test_heap_limits` は 5 MiB で通り、本クレートの
//!   実測は 8 MiB で通る）。**設定に下限を課すのはタスク 4.3 の仕事**であり、本層は値を
//!   加工しない（黙って下限へ丸めると、利用者が設定した上限と実際の上限が食い違う）。

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use deno_core::v8;
use deno_core::JsRuntime;

use crate::engine::outcome::{LimitKind, Limits};

/// 時間の上限のタイマーが待つ最大の長さ（[module docs](self) の「上限の端の値」）。
const MAX_TIMER_WAIT: Duration = Duration::from_secs(24 * 60 * 60);

/// 打ち切りの記録（`AtomicU8` の値）。0 は「打ち切っていない」。
///
/// **種類を記録するのは 1 箇所**であり、時間のタイマースレッドとメモリの callback が同じ
/// 値へ書く。`LimitKind` を直接 `AtomicU8` に載せず、この 3 値へ写すのは、`LimitKind` の
/// 表現（列挙の並び）を V8 側の記録の形式に結び付けないためである。
const NOT_ABORTED: u8 = 0;
const TIME_LIMIT: u8 = 1;
const MEMORY_LIMIT: u8 = 2;

/// 記録から種類へ戻す。
fn decode(recorded: u8) -> Option<LimitKind> {
    match recorded {
        TIME_LIMIT => Some(LimitKind::Time),
        MEMORY_LIMIT => Some(LimitKind::Memory),
        _ => None,
    }
}

/// メモリの上限から isolate の `create_params` を組む（要件 6.2）。
///
/// **初期の大きさは V8 の既定に任せ、最大だけを上限にする**（`heap_limits(0, max)`。
/// deno_core の `test_heap_limits` と同じ形）。`max` に近づくと V8 は GC を繰り返したうえで
/// near-heap-limit callback を呼ぶ（`v8::CreateParams::heap_limits` の doc）。
pub(crate) fn create_params(limits: Limits) -> v8::CreateParams {
    let max = usize::try_from(limits.memory_bytes).unwrap_or(usize::MAX);
    v8::Isolate::create_params().heap_limits(0, max)
}

/// 打ち切りの印を isolate から下ろす（`cancel_terminate_execution`）。
///
/// 実行の**前**と**後**の両方で呼ぶ（[module docs](self) の「打ち切りの後始末と復帰」）。
/// 戻り値（V8 が「下ろした」と答えたか）は使わない — 打ち切っていない isolate に対しては
/// 何もしないのが正しい振る舞いであり、判定に使える情報ではないためである。
pub(crate) fn cancel_terminate(js: &mut JsRuntime) {
    let _ = js.v8_isolate().cancel_terminate_execution();
}

/// 実行 1 回ぶんの打ち切りの見張り（時間のタイマーとメモリの callback）。
///
/// isolate ができた直後に [`AbortWatch::arm`] で張り、実行が終わったら
/// [`AbortWatch::disarm`] で畳む。**見張りは実行ごとに 1 つ**であり、前の実行のタイマーが
/// 次の実行へ撃ち込むことはない（畳むときに join する）。
pub(crate) struct AbortWatch {
    /// 打ち切りの種類の記録（時間のタイマーとメモリの callback が書く）。
    record: Arc<AtomicU8>,
    /// 時間のタイマー（畳むと `None` になる）。
    timer: Option<Timer>,
}

impl AbortWatch {
    /// isolate に上限の見張りを張る（要件 6.1, 6.2）。
    ///
    /// メモリの callback は**常に**張る（[module docs](self) の「上限の端の値」）。時間の
    /// タイマースレッドを起こせなかったときだけ `Err` を返す — 時間の上限が**効かないまま
    /// 走る**より、走らせずに失敗として返すほうが正直である。
    pub(crate) fn arm(js: &mut JsRuntime, limits: Limits) -> io::Result<Self> {
        let record = Arc::new(AtomicU8::new(NOT_ABORTED));
        // メモリの上限: near-heap-limit callback の中で terminate へ合流する。
        // **上限を上げてから terminate する**（上げないと V8 がプロセスを abort させる）。
        let handle = js.v8_isolate().thread_safe_handle();
        let memory_record = Arc::clone(&record);
        let memory_handle = handle.clone();
        js.add_near_heap_limit_callback(move |current_limit, _initial_limit| {
            memory_record.store(MEMORY_LIMIT, Ordering::SeqCst);
            memory_handle.terminate_execution();
            current_limit.saturating_mul(2)
        });
        // 時間の上限: タイマースレッドが期限で terminate する。
        let timer = Timer::arm(handle, limits.time, Arc::clone(&record))?;
        Ok(Self {
            record,
            timer: Some(timer),
        })
    }

    /// 実行が終わったら畳む（時間のタイマーを止めて join し、打ち切りの種類を返す）。
    ///
    /// 種類が `Some` でも、**実行が戻り値を返していれば打ち切りではない**（期限と実行の
    /// 終わりが競合した場合）。どちらだったかの判断は actor が行う（1.4 の写像）。
    pub(crate) fn disarm(mut self) -> Option<LimitKind> {
        if let Some(timer) = self.timer.take() {
            timer.stop();
        }
        decode(self.record.load(Ordering::SeqCst))
    }
}

/// 時間の上限を刻むタイマー（実行ごとに 1 本）。
///
/// **実行が先に終われば期限まで待たない** — 畳むときに実行の終わりを伝えて目を覚まさせる。
/// これが無いと、上限 30 秒の既定で 1 回の実行がいつも 30 秒かかることになる。
struct Timer {
    /// 実行の終わりを伝える口（落とすとタイマースレッドが目を覚ます）。
    done: Option<mpsc::Sender<()>>,
    /// 実行が終わった印（終わった実行をタイマーが撃たないためのもの）。畳む側が立てる。
    finished: Arc<AtomicBool>,
    /// タイマースレッド（畳むときに join する）。
    thread: Option<thread::JoinHandle<()>>,
}

impl Timer {
    /// タイマースレッドを起こす。
    fn arm(handle: v8::IsolateHandle, time: Duration, record: Arc<AtomicU8>) -> io::Result<Self> {
        let (done, wake) = mpsc::channel::<()>();
        let finished = Arc::new(AtomicBool::new(false));
        let timer_finished = Arc::clone(&finished);
        let thread = thread::Builder::new()
            .name("macro-time-limit".to_owned())
            .spawn(move || {
                if let Err(RecvTimeoutError::Timeout) = wake.recv_timeout(time.min(MAX_TIMER_WAIT))
                {
                    // 期限に達した。**実行が終わった印が立っていれば何もしない**
                    // （畳む側が印を立ててから join するので、終わった実行は撃たれない）。
                    if !timer_finished.load(Ordering::SeqCst) {
                        record.store(TIME_LIMIT, Ordering::SeqCst);
                        handle.terminate_execution();
                    }
                }
            })?;
        Ok(Self {
            done: Some(done),
            finished,
            thread: Some(thread),
        })
    }

    /// 実行の終わりを伝え、タイマースレッドを join する。
    fn stop(mut self) {
        // 印を立ててから口を落とす（タイマーが「終わった実行」を撃たないようにする）。
        self.finished.store(true, Ordering::SeqCst);
        self.done.take();
        if let Some(thread) = self.thread.take() {
            // タイマースレッドの終わり方（正常 / panic）は実行の結果を変えない。
            let _ = thread.join();
        }
    }
}

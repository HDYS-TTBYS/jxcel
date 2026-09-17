//! 実行基盤の actor（専用スレッド・直列化・実行ごとの isolate・上限。tasks.md 1.4 / 1.5。
//! 要件 2.1, 2.2, 6.1–6.4）。
//!
//! V8 の isolate は `Send` ではなく、**current-thread のランタイムを要求する**。したがって
//! isolate は**専用の OS スレッドが所有**し、多スレッド側からは **mpsc + oneshot** 越しに
//! 要求を送る（`tech.md`「Known Risks」1 の 2026-09-17 のスパイクで確定した形。動く実例が
//! `target/spike-macro-runtime/` にある）。**isolate はこのスレッドを離れない**。
//!
//! # この層が持つもの（tasks.md 1.4 / 1.5）
//!
//! - 専用スレッドが current-thread の tokio ランタイムを 1 つ回し、その上で
//!   **実行ごとに `JsRuntime` を作って所有スレッドの上で落とす**（design.md 決定 1）。
//!   isolate の生成は `run_once` の**1 箇所**だけであり、そこが別スレッドへ移ることはない
//! - 要求は mpsc + oneshot で**直列化**する。**実行中の 2 つ目の要求は
//!   [`ActorError::Busy`] で断る**（並行実行しない。design.md「State Management」の
//!   「1 実行ずつ」であり、要件 2.2 の「表の操作を止めない」の裏返しである）
//! - 評価は `execute_script` → `run_event_loop(Default::default())` →
//!   **Promise の状態を読む**。**`resolve` を使わない**（スパイクの実測。`resolve` は
//!   イベントループが回り切った後でも返らなかった）
//! - 戻り値と 3 値（[`RunOutcome::Ran`] / [`RunOutcome::Failed`] /
//!   [`RunOutcome::Aborted`]）への写像を 1.3 の型で返す
//! - **時間とメモリの上限で打ち切り**、どちらの上限だったかを [`RunOutcome::Aborted`] の
//!   `limit` に載せる（タスク 1.5。適用・種類の記録・復帰は [`super::limits`] が持つ）。
//!   実行の前後に `cancel_terminate_execution()` を呼び、打ち切りの後も**同じ actor で次の
//!   実行ができる**
//! - 後始末（[`MacroActor::shutdown`]）。実行ごとに isolate を作り直すため、**連続する
//!   2 回の実行の間でグローバルは共有されない**（下の「観測」）
//!
//! # まだ持たないもの（担当タスク。ここに嘘のフォールバックは置かない）
//!
//! - **ソースの変換**（タスク 3.1）: いまは種別に関わらずソースをそのまま `execute_script`
//!   へ渡す。型注釈の除去とソースマップは 3.1 がモジュールの解決の段で行う。したがって
//!   **TypeScript のマクロは、変換が入るまで実行できない**（型注釈が構文誤りになる）
//! - **op の登録と `console` の取り込み**（タスク 3.2）: `RuntimeOptions` は既定（拡張なし）
//!   である。ホスト API・コンソール・ソースマップの組み立ては 3.2 が `engine/isolate.rs`
//!   に置き、`run_once` がそれを呼ぶ。`RunOutcome::Ran` の `output` が空なのはそのためである。
//!   op の panic を捕捉して次の実行へ復帰するのも、op が入る 3.2 の範囲である（いま panic
//!   すればスレッドが畳まれ、以後の要求は [`ActorError::Stopped`] として断られる）
//! - **変更の集約**（タスク 2.3）: いまは常に 0 件（`ChangeSummary::default()`）
//!
//! # 呼び出しの形（同期である理由と、その代償）
//!
//! [`MacroActor::run`] は**同期**である（design.md「Service Interface」の
//! `fn run(&self, request: RunRequest) -> Result<RunOutcome, MacroError>` と同じ形）。
//! 内部では tokio の `blocking_send` / `blocking_recv` を使うため、**非同期の実行文脈の
//! 中から直接呼んではならない** — tokio はその場で panic する。アダプタは `spawn_blocking`
//! の上で呼ぶ（design.md「Implementation Notes」の「`run` を非同期の腕で呼ぶ」）。
//!
//! # 観測（tasks.md 1.4 / 1.5 の受け入れ。テストとして固定してある）
//!
//! | 観測 | テスト |
//! |------|--------|
//! | 1 回の評価が戻り値を返す（Promise は状態から読む） | `tests::一回の評価が戻り値を返す` |
//! | 例外が実行の失敗として理由と位置を運ぶ | `tests::投げられた例外は実行の失敗として理由と位置を運ぶ` |
//! | 実行中の 2 つ目の要求が「実行中である」として断られる | `tests::実行中の二つ目の要求は実行中として断られる` |
//! | 連続 2 回の実行でグローバルが共有されない | `tests::連続する二回の実行でグローバルは共有されない` |
//! | 停止した actor がスレッドを畳み、以後の要求を断る | `tests::停止したactorはスレッドを畳み以降の要求を断る` |
//! | 終わらない繰り返しが**時間の上限**で打ち切られ、次の実行が成功する | `tests::時間の上限で打ち切られ次の実行が成功する` |
//! | 大量確保が**メモリの上限**で打ち切られ、次の実行が成功する | `tests::メモリの上限で打ち切られ次の実行が成功する` |
//! | メモリの上限が 0 のときは V8 の既定で走る | `tests::メモリの上限が零のときはv8の既定で走る` |

use std::fmt;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use deno_core::error::{CoreError, CoreErrorKind, JsError};
use deno_core::{v8, JsRuntime, RuntimeOptions};
use tokio::sync::{mpsc, oneshot};

use crate::engine::limits;
use crate::engine::outcome::{
    ChangeSummary, FailureKind, Frame, MacroFailure, RunOutcome, RunRequest,
};
use crate::source::record::MacroName;

/// 要求を並べるチャネルの容量。
///
/// **1 である**。同時に走る実行は 1 つであり（design.md「State Management」）、実行中の
/// 要求は並べずに断る（[`MacroActor::run`]）。容量を増やしても待ちが長くなるだけで、
/// 「実行中である」を伝える意味は変わらない。
const REQUEST_CAPACITY: usize = 1;

/// actor への要求（**専用スレッドが 1 つずつ処理する**）。
enum Request {
    /// 1 回の実行。結果は oneshot で返る（`RunOutcome` は失敗も成功も運ぶ）。
    Run {
        /// 実行の要求（記録・上限・対象ウィンドウ）。
        request: RunRequest,
        /// 結果の返し口。
        reply: oneshot::Sender<RunOutcome>,
    },
    /// 後始末。ここまでに並んだ要求を処理し終えてから畳む。
    Stop {
        /// 畳む直前の合図。
        reply: oneshot::Sender<()>,
    },
}

/// V8 isolate を所有する専用スレッドへの口（design.md「MacroActor / isolate / limits」）。
///
/// **多スレッド側から見えるのはこれだけ**である。1 つの actor が同時に走らせる実行は 1 つで
/// あり、isolate はこの口の向こうのスレッドを離れない。
pub struct MacroActor {
    /// 専用スレッドへの要求の口。
    requests: mpsc::Sender<Request>,
    /// 実行中か（design.md「State Management」の `running`）。
    ///
    /// 呼び出し側が要求を送る**前に**立て、専用スレッドが実行を終えたら下ろす。
    /// 「実行中の 2 つ目の要求」をチャネルへ並べずに断るための印である。
    running: Arc<AtomicBool>,
    /// 専用スレッドの持ち手。後始末（[`MacroActor::shutdown`]）で畳む。
    thread: Mutex<Option<thread::JoinHandle<()>>>,
}

impl MacroActor {
    /// 専用の OS スレッドを 1 つ起こし、その上で要求を処理する actor を作る。
    ///
    /// スレッドの名前は `macro-runtime` である（診断のために他と区別する）。isolate は
    /// **この呼び出しでは作らない** — 生成は最初の実行まで遅れる（スパイクの実測で生成は
    /// 5.5 ms・常駐 +24 MB であり、マクロを 1 度も実行しない起動から費用を外せる）。
    ///
    /// 失敗するのはスレッドを起こせないとき（資源の枯渇）だけである。
    pub fn spawn() -> io::Result<Self> {
        let (requests, inbox) = mpsc::channel(REQUEST_CAPACITY);
        let running = Arc::new(AtomicBool::new(false));
        let serve_running = Arc::clone(&running);
        let thread = thread::Builder::new()
            .name("macro-runtime".to_owned())
            .spawn(move || serve(inbox, serve_running))?;
        Ok(Self {
            requests,
            running,
            thread: Mutex::new(Some(thread)),
        })
    }

    /// 実行の要求を 1 つ渡し、終わるまで待つ。
    ///
    /// **実行中に届いた 2 つ目の要求は [`ActorError::Busy`] で断る**（並行実行しない。
    /// 要件 2.2 の裏返しであり、UI を止めないための規則である）。要求そのものが届かなかった
    /// とき（停止済み、または実行の途中でスレッドが畳まれた）は [`ActorError::Stopped`] を返す。
    ///
    /// **同期である**（モジュール docs「呼び出しの形」）。非同期の実行文脈の中から直接呼ぶと
    /// tokio が panic するため、アダプタは `spawn_blocking` の上で呼ぶ。
    pub fn run(&self, request: RunRequest) -> Result<RunOutcome, ActorError> {
        // 実行中の印を**送る前に**立てる。ここで負けた要求は並べずに断る。
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(ActorError::Busy);
        }
        let (reply, answer) = oneshot::channel();
        if self
            .requests
            .blocking_send(Request::Run { request, reply })
            .is_err()
        {
            // 専用スレッドが居ない（停止済み・または実行の途中で畳まれた）
            self.running.store(false, Ordering::SeqCst);
            return Err(ActorError::Stopped);
        }
        match answer.blocking_recv() {
            Ok(outcome) => Ok(outcome),
            Err(_) => {
                // 要求は届いたが答えが来なかった（スレッドが畳まれた）
                self.running.store(false, Ordering::SeqCst);
                Err(ActorError::Stopped)
            }
        }
    }

    /// 実行中か（design.md「State Management」の `running`）。
    ///
    /// 実行の面（タスク 4.4 の「実行中」の提示）と結合検査が読む。要求を送った直後から
    /// 実行が終わるまで真であり、**送った要求が専用スレッドへ渡ったかは示さない**。
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 後始末: ここまでに並んだ要求を処理し終えてから専用スレッドを畳む。
    ///
    /// 専用スレッドの上で isolate が落ち、スレッドが終わるまで待つ（isolate を所有スレッドの
    /// 外で落とさない）。2 回目以降の呼び出しは何もしない（すでに畳まれている）。
    /// 実行そのものは**打ち切らない** — 打ち切りはタスク 1.5 である。
    ///
    /// 戻り値は専用スレッドの終わり方である（`Err` は実行の途中でスレッドが panic した
    /// ことを表す。V8 の失敗は panic ではなく [`RunOutcome::Failed`] として返る）。
    ///
    /// 呼び出しの形は [`MacroActor::run`] と同じく同期であり、非同期の実行文脈の中から
    /// 直接呼んではならない。
    ///
    /// なお、この口を呼ばずに actor を落としても専用スレッドは畳まれる（要求の口が閉じると
    /// ループが終わる）。`shutdown` は**終わりを待つ**点だけが違う。
    pub fn shutdown(&self) -> thread::Result<()> {
        let (reply, answer) = oneshot::channel();
        // すでに畳まれていれば送れない（`Err`）。その場合も下の join が終わり方を返す。
        if self.requests.blocking_send(Request::Stop { reply }).is_ok() {
            // 実行中の要求があれば、それが終わってから合図が来る。
            let _ = answer.blocking_recv();
        }
        let handle = self
            .thread
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        match handle {
            Some(handle) => handle.join(),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for MacroActor {
    /// 内部のスレッドの持ち手は表示しない（口と状態だけを見せる）。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroActor")
            .field("is_running", &self.is_running())
            .finish_non_exhaustive()
    }
}

/// 要求を断る理由（design.md「Error Handling」の「業務の誤り」）。
///
/// 設計の `MacroError::Busy` はこの型を api 層（タスク 3.2 以降）が写したものである。
/// ここに置くのは、実行の入口を持たない actor の層が**自分が断った理由**を持てるようにする
/// ためである（提示の文言は持たない。境界の規約は 4.4 の面が組み立てる）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActorError {
    /// 実行中である（要件 2.2 の裏返し）。並行実行しない。
    Busy,
    /// 専用スレッドが居ない（停止済み、または実行の途中で畳まれた）。
    Stopped,
}

impl ActorError {
    /// 診断の記録に使う安定トークン（ロケール依存なし）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Stopped => "stopped",
        }
    }
}

impl fmt::Display for ActorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::error::Error for ActorError {}

/// 専用スレッドが畳まれるときに「実行中」の印を下ろす。
///
/// 実行の途中でスレッドが巻き戻っても（op の panic 等）、印を立てたままにしない。印が
/// 残ると、以後の要求が「実行中」として断られ続け、**本当の理由（スレッドが居ない）を
/// 隠してしまう**。印を下ろしておけば、以後の要求は `Stopped` として断られる。
struct ClearRunningOnDrop(Arc<AtomicBool>);

impl Drop for ClearRunningOnDrop {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// 専用スレッドの本体。current-thread のランタイムを 1 つ回し、要求を 1 つずつ処理する。
fn serve(mut inbox: mpsc::Receiver<Request>, running: Arc<AtomicBool>) {
    let _clear_on_drop = ClearRunningOnDrop(Arc::clone(&running));
    // **current-thread のランタイムだけを使う**。`LocalSet` は挟まない — スパイクの実測で、
    // 挟んだ形では deferred op の完了が Promise へ届かなかった（`tech.md`「Known Risks」1）。
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        // ランタイムを作れないときは要求に答えずにスレッドを畳む。要求の口が閉じるので、
        // 以後の要求は `Stopped` として断られる（**成功を装わない**）。
        Err(_) => return,
    };
    runtime.block_on(async move {
        while let Some(request) = inbox.recv().await {
            match request {
                Request::Run { request, reply } => {
                    let outcome = run_once(&request).await;
                    // 実行を終えたら印を下ろす。次の要求はここから受け付ける
                    // （答えを返す前に下ろすので、呼び出し側が戻った時点で次を送れる）。
                    running.store(false, Ordering::SeqCst);
                    let _ = reply.send(outcome);
                }
                Request::Stop { reply } => {
                    let _ = reply.send(());
                    break;
                }
            }
        }
    });
}

/// 1 回の実行。**isolate の生成はこの 1 箇所だけ**である（design.md「Implementation Notes」の
/// 「生成は 1 箇所（`isolate.rs`）に閉じる」）。
///
/// 作って、走らせて、**同じスレッドの上で落とす**。所要は isolate の生成から破棄までを含む
/// （利用者から見た 1 回の実行の所要。要件 11.3）。
async fn run_once(request: &RunRequest) -> RunOutcome {
    let started = Instant::now();
    // メモリの上限は isolate を作るときに V8 へ渡す（タスク 1.5。`limits`）。
    // `JsRuntime::new` ではなく `try_new` を使う（isolate を作れないときに panic しない）。
    let options = RuntimeOptions {
        create_params: Some(limits::create_params(request.limits)),
        ..Default::default()
    };
    let mut js = match JsRuntime::try_new(options) {
        Ok(js) => js,
        Err(error) => {
            return RunOutcome::Failed {
                failure: MacroFailure::new(
                    FailureKind::Execution,
                    format!("実行基盤を作れない: {error}"),
                    Vec::new(),
                ),
            }
        }
    };
    // 実行の前に打ち切りの印を下ろす（前の実行の打ち切りを持ち越さない。タスク 1.5）。
    limits::cancel_terminate(&mut js);
    // 時間のタイマーとメモリの callback を張る。**どちらの打ち切りも同じ終了経路**を通る。
    let watch = match limits::AbortWatch::arm(&mut js, request.limits) {
        Ok(watch) => watch,
        Err(error) => {
            return RunOutcome::Failed {
                failure: MacroFailure::new(
                    FailureKind::Execution,
                    format!("時間の上限のタイマーを起こせない: {error}"),
                    Vec::new(),
                ),
            }
        }
    };
    let evaluated = evaluate(&mut js, request).await;
    // 実行の終わりをタイマーへ伝えて畳む（期限まで待たない）。打ち切りの種類をここで読む。
    let limit = watch.disarm();
    // **打ち切りの印を下ろしてから** isolate を落とす（打ち切りの状態のまま片付けない）。
    limits::cancel_terminate(&mut js);
    // **isolate は所有スレッドの上で落とす**（V8 の片付けが境界を越えない）。
    drop(js);
    let elapsed_ms = elapsed_ms(started);
    match (limit, evaluated) {
        // 打ち切りが実行を止めた（時間 / メモリ。要件 6.1, 6.2）。理由とフレームは V8 が
        // 打ち切りの例外として返したものをそのまま運ぶ。**実測**: どちらの上限でも
        // `Error: execution terminated` であり、フレームは無い（V8 の打ち切りは位置を
        // 持たない）。種類は `limit` が運ぶので、提示（4.4）はそれを使う。
        (Some(limit), Err(failure)) => RunOutcome::Aborted {
            limit,
            elapsed_ms,
            failure,
        },
        // 上限に間に合わなかった（実行は戻り値を返した）。**成功を上書きしない** —
        // 上限と実行の終わりが競合しただけで、打ち切られたものは無い。
        (_, Ok(value)) => RunOutcome::Ran {
            value,
            // `console` の取り込みはタスク 3.2、変更の集約はタスク 2.3 である。
            output: Vec::new(),
            changes: ChangeSummary::default(),
            elapsed_ms,
        },
        (None, Err(failure)) => RunOutcome::Failed { failure },
    }
}

/// ソースを 1 回評価し、戻り値の提示用の表現を返す。
///
/// 正しい形は「**実行 → イベントループを回し切る → Promise の状態を読む**」である。
/// `resolve` は使わない（スパイクの実測で、イベントループが回り切った後でも返らなかった）。
async fn evaluate(js: &mut JsRuntime, request: &RunRequest) -> Result<String, MacroFailure> {
    let name = request.record.name.clone();
    // スクリプトの名前はマクロの名前である（V8 のスタックフレームがどのマクロの位置かを示す）。
    let script = js
        .execute_script(name.as_str().to_owned(), request.record.source.clone())
        .map_err(|error| failure_of_js_error(&name, &error))?;
    js.run_event_loop(Default::default())
        .await
        .map_err(|error| failure_of_core_error(&name, error))?;
    deno_core::scope!(scope, js);
    let value = v8::Local::<v8::Value>::new(scope, &script);
    match value.try_cast::<v8::Promise>() {
        Ok(promise) => match promise.state() {
            v8::PromiseState::Fulfilled => Ok(presentation(scope, promise.result(scope))),
            // 状態が拒否である場合。**実測では、未処理の拒否は `run_event_loop` が
            // `CoreErrorKind::Js` として返す**（その経路も [`failure_of_core_error`] が
            // 理由とフレームを取る）。それでも状態を 3 つとも読む形は残す —
            // `Fulfilled` / `Pending` と同じ形で尽くすためである。
            v8::PromiseState::Rejected => Err(MacroFailure::new(
                FailureKind::Execution,
                format!(
                    "約束が拒否された: {}",
                    presentation(scope, promise.result(scope))
                ),
                Vec::new(),
            )),
            // イベントループが空になったのに未解決である（op が完了を届けていない等の異常。
            // 成功を装わず失敗として返す）。
            v8::PromiseState::Pending => Err(MacroFailure::new(
                FailureKind::Execution,
                "イベントループが空になったのに約束が未解決である".to_owned(),
                Vec::new(),
            )),
        },
        // Promise でない値はそのまま戻り値である（同期のスクリプトの完結値）。
        // `Local` は `Copy` なので、`try_cast` の後でも元の値を読める。
        Err(_) => Ok(presentation(scope, value)),
    }
}

/// 戻り値を提示用の表現へ写す（要件 2.3）。
///
/// **オブジェクトは JSON** である（`JSON.stringify` と同じ。`to_string` の `[object Object]`
/// では利用者に何も伝わらない）。JSON にできない値（`undefined`・関数・Symbol・BigInt・循環）
/// は JavaScript の文字列表現へ落とす — 提示の面が「何が返ったか」を失わないためである。
fn presentation(scope: &v8::PinScope<'_, '_>, value: v8::Local<'_, v8::Value>) -> String {
    if value.is_undefined() {
        return "undefined".to_owned();
    }
    match v8::json::stringify(scope, value) {
        Some(json) => json.to_rust_string_lossy(scope),
        None => value.to_rust_string_lossy(scope),
    }
}

/// 実行の例外を失敗へ写す（要件 9.1, 9.3）。
///
/// 理由は例外の種別とメッセージ（`TypeError: …`）であり、フレームは V8 のスタックから
/// **内側（投げた位置）から外側へ**の順で取る。**TypeScript の原位置**へ写すのは変換
/// （タスク 3.1）の仕事であり、ここで取れるのは実行したソースの位置である。
fn failure_of_js_error(macro_name: &MacroName, error: &JsError) -> MacroFailure {
    MacroFailure::new(
        FailureKind::Execution,
        message_of(error),
        frames_of(macro_name, error),
    )
}

/// イベントループの誤りを失敗へ写す。
///
/// JS の例外（未処理の拒否など）は [`CoreErrorKind::Js`] として返るので、**例外と同じ形で
/// 理由とフレームを取り出す**（V8 のスタック本文を理由へ混ぜない。提示の組み立ては 4.4 の
/// 面の仕事である）。
fn failure_of_core_error(macro_name: &MacroName, error: CoreError) -> MacroFailure {
    match *error.0 {
        CoreErrorKind::Js(js_error) => failure_of_js_error(macro_name, &js_error),
        other => MacroFailure::new(FailureKind::Execution, other.to_string(), Vec::new()),
    }
}

/// 例外の理由。JavaScript の `Error` は名前と本文を分けて持つため、**両方を残す**
/// （`TypeError: undefined は関数ではありません`。名前を落とすと提示から情報が消える）。
fn message_of(error: &JsError) -> String {
    match (&error.name, &error.message) {
        (Some(name), Some(message)) if !name.is_empty() && !message.is_empty() => {
            format!("{name}: {message}")
        }
        (_, Some(message)) if !message.is_empty() => message.clone(),
        // `Error` でない値が投げられた場合（`throw '…'` 等）は V8 の文言を使う。
        _ => error
            .exception_message
            .trim_start_matches("Uncaught ")
            .to_owned(),
    }
}

/// V8 のスタックからフレームを取る（内側から外側へ）。
///
/// 位置を持たないフレーム（V8 の内部や `eval` の外側）は落とす — 1 起点の行・列を指せない
/// ものを「位置がある」ように見せないためである。
fn frames_of(macro_name: &MacroName, error: &JsError) -> Vec<Frame> {
    error
        .frames
        .iter()
        .filter_map(|frame| {
            let line = u32::try_from(frame.line_number?).ok()?;
            let column = u32::try_from(frame.column_number.unwrap_or(1)).ok()?;
            if line == 0 {
                return None;
            }
            Some(Frame::at(
                macro_name.clone(),
                frame.function_name.clone(),
                line,
                column,
            ))
        })
        .collect()
}

/// 所要をミリ秒へ丸める（要件 11.3 の記録はミリ秒）。
fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::outcome::{LimitKind, Limits};
    use crate::source::record::{MacroKind, MacroRecord};
    use crate::WindowLabel;
    use std::time::Duration;

    /// JavaScript のマクロ 1 件の実行の要求（上限は既定）。
    fn request(name: &str, source: &str) -> RunRequest {
        with_limits(name, source, Limits::default())
    }

    /// 上限を指定した実行の要求（打ち切りの検査は**既定の 30 秒を待たない**ために上限を
    /// 短くする。既定値そのものは 1.3 の型のテストが固定している）。
    fn with_limits(name: &str, source: &str, limits: Limits) -> RunRequest {
        RunRequest::new(
            MacroRecord::new(MacroName::from(name), MacroKind::JavaScript, source),
            limits,
            WindowLabel::from("main"),
        )
    }

    /// `Ran` から戻り値の提示用の表現を取り出す。
    fn ran_value(outcome: &RunOutcome) -> &str {
        match outcome {
            RunOutcome::Ran { value, .. } => value,
            other => panic!("最後まで走り切るはずである: {other:?}"),
        }
    }

    /// 実行が始まるまで待つ（1 つ目の要求が専用スレッドへ渡る前に 2 つ目を送ると、
    /// 「実行中」ではなく「並んだ」ことになってしまう）。
    fn wait_until_running(actor: &MacroActor) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !actor.is_running() {
            assert!(Instant::now() < deadline, "1 つ目の実行が始まらない");
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// 1 回の評価が戻り値を返す。オブジェクトは JSON であり、Promise は
    /// **イベントループを回した後の状態**から読む（要件 2.3。tasks.md 1.4 の観測①）。
    #[test]
    fn 一回の評価が戻り値を返す() {
        let actor = MacroActor::spawn().expect("actor を起こせる");

        let number = actor.run(request("計算", "1 + 1")).expect("実行できる");
        assert_eq!(ran_value(&number), "2");

        let object = actor
            .run(request("集計", "({ 合計: 3, 内訳: [1, 2] })"))
            .expect("実行できる");
        assert_eq!(ran_value(&object), r#"{"合計":3,"内訳":[1,2]}"#);

        // `resolve` を使わず、Promise の状態を読む（`tech.md`「Known Risks」1 の確定形）。
        let promised = actor
            .run(request("非同期", "(async () => '非同期の値')()"))
            .expect("実行できる");
        assert_eq!(ran_value(&promised), r#""非同期の値""#);

        actor.shutdown().expect("スレッドを畳める");
    }

    /// 投げられた例外は実行の失敗として、理由と投げた位置を運ぶ（要件 9.1, 9.3）。
    #[test]
    fn 投げられた例外は実行の失敗として理由と位置を運ぶ() {
        let actor = MacroActor::spawn().expect("actor を起こせる");
        // 投げる位置を 3 行目に置く（**行が本当に読まれているか**を見るため）。
        let source = "const 前置き = 1;\nconst 二番目 = 2;\nthrow new Error('3 行目で投げた');";
        let outcome = actor
            .run(request("わざと失敗", source))
            .expect("actor は要求を断らない");

        match outcome {
            RunOutcome::Failed { failure } => {
                assert_eq!(failure.kind.as_str(), "execution");
                assert!(
                    failure.message.contains("3 行目で投げた"),
                    "理由に例外のメッセージが入る: {}",
                    failure.message
                );
                let innermost = failure.innermost().expect("投げた位置のフレームがある");
                assert_eq!(innermost.macro_name.as_str(), "わざと失敗");
                assert_eq!(innermost.line, 3, "投げた行を指す: {innermost:?}");
                assert!(innermost.column >= 1, "列は 1 起点である: {innermost:?}");
            }
            other => panic!("失敗として返るはずである: {other:?}"),
        }

        // 失敗しても actor は生き続ける（次の実行ができる）。
        let next = actor
            .run(request("次", "'生きている'"))
            .expect("実行できる");
        assert_eq!(ran_value(&next), r#""生きている""#);

        actor.shutdown().expect("スレッドを畳める");
    }

    /// 実行中の 2 つ目の要求は「実行中である」として断られる（並行実行しない。要件 2.2 の
    /// 裏返し。tasks.md 1.4 の観測②）。
    #[test]
    fn 実行中の二つ目の要求は実行中として断られる() {
        let actor = MacroActor::spawn().expect("actor を起こせる");
        // 1 つ目は 300 ms 回る（`Date.now` は V8 の組込であり、ホスト API を要らない）。
        const SLOW: &str =
            "const 終わり = Date.now() + 300; while (Date.now() < 終わり) {} '終わった'";
        thread::scope(|scope| {
            let first = scope.spawn(|| actor.run(request("長い実行", SLOW)));
            wait_until_running(&actor);

            match actor.run(request("割り込み", "'割り込んだ'")) {
                Err(ActorError::Busy) => {}
                other => panic!("実行中として断られるはずである: {other:?}"),
            }

            let first = first.join().expect("1 つ目は走り切る");
            assert_eq!(ran_value(&first.expect("実行できる")), r#""終わった""#);
        });

        // 断られた要求はどこにも積まれていない（順番待ちの実行が後から走らない）。
        let after = actor.run(request("後", "'後から'")).expect("実行できる");
        assert_eq!(ran_value(&after), r#""後から""#);

        actor.shutdown().expect("スレッドを畳める");
    }

    /// 連続する 2 回の実行の間でグローバルは共有されない（実行ごとに isolate を作る。
    /// design.md 決定 1。tasks.md 1.4 の観測③）。
    #[test]
    fn 連続する二回の実行でグローバルは共有されない() {
        let actor = MacroActor::spawn().expect("actor を起こせる");

        // 1 回目: グローバルに印を付けて読み返す（同じ isolate の中では見える）。
        let marked = actor
            .run(request(
                "印を付ける",
                "globalThis.印 = 'あり'; globalThis.印",
            ))
            .expect("実行できる");
        assert_eq!(ran_value(&marked), r#""あり""#);

        // 2 回目: 別の isolate なので印は見えない（**グローバルが持ち越されない**）。
        let read = actor
            .run(request("印を読む", "globalThis.印 === undefined"))
            .expect("実行できる");
        assert_eq!(ran_value(&read), "true");

        actor.shutdown().expect("スレッドを畳める");
    }

    /// 解決しない約束は失敗として返る。イベントループが空になったのに未解決である、という
    /// 異常であり、**成功を装わない**（`resolve` を使わず状態を読む形の裏側である）。
    #[test]
    fn 未解決の約束は失敗として返る() {
        let actor = MacroActor::spawn().expect("actor を起こせる");
        let outcome = actor
            .run(request("解決しない", "new Promise(() => {})"))
            .expect("actor は要求を断らない");

        match outcome {
            RunOutcome::Failed { failure } => assert_eq!(failure.kind.as_str(), "execution"),
            other => panic!("失敗として返るはずである: {other:?}"),
        }

        actor.shutdown().expect("スレッドを畳める");
    }

    /// 拒否された約束も失敗として返り、理由に拒否の内容が入る（要件 9.1）。
    #[test]
    fn 拒否された約束は失敗として返る() {
        let actor = MacroActor::spawn().expect("actor を起こせる");
        let outcome = actor
            .run(request(
                "拒否する",
                "(async () => { throw new Error('拒否した'); })()",
            ))
            .expect("actor は要求を断らない");

        match outcome {
            RunOutcome::Failed { failure } => {
                assert_eq!(failure.kind.as_str(), "execution");
                assert!(
                    failure.message.contains("拒否した"),
                    "理由に拒否の内容が入る: {}",
                    failure.message
                );
                // 未処理の拒否も**位置を持つ**（イベントループが例外として返す経路でも
                // フレームを落とさない）。
                let innermost = failure.innermost().expect("投げた位置のフレームがある");
                assert_eq!(innermost.line, 1, "投げた行を指す: {innermost:?}");
            }
            other => panic!("失敗として返るはずである: {other:?}"),
        }

        actor.shutdown().expect("スレッドを畳める");
    }

    /// 時間の上限で打ち切られ、**その後に同じ actor で次の実行が成功する**（要件 6.1, 6.4。
    /// tasks.md 1.5 の観測）。
    #[test]
    fn 時間の上限で打ち切られ次の実行が成功する() {
        let actor = MacroActor::spawn().expect("actor を起こせる");
        // 終わらない繰り返しを短い上限（150 ms）で走らせる。
        let outcome = actor
            .run(with_limits(
                "終わらない繰り返し",
                "for (;;) {}",
                Limits::new(Duration::from_millis(150), Limits::DEFAULT_MEMORY_BYTES),
            ))
            .expect("actor は要求を断らない");

        match outcome {
            RunOutcome::Aborted {
                limit, elapsed_ms, ..
            } => {
                assert_eq!(limit, LimitKind::Time, "時間の上限として種類が載る");
                assert!(
                    elapsed_ms >= 100,
                    "期限まで走ってから打ち切られる（即座の失敗ではない）: {elapsed_ms} ms"
                );
            }
            other => panic!("時間の上限で打ち切られるはずである: {other:?}"),
        }

        // 復帰: 打ち切りの後始末（isolate の破棄と印の復帰）が済んでおり、**前の実行の
        // タイマーが次の実行を撃たない**（撃てば、これも打ち切りとして返る）。
        let again = actor
            .run(request("生き返る", "'生き返った'"))
            .expect("実行できる");
        match &again {
            RunOutcome::Ran {
                value, elapsed_ms, ..
            } => {
                assert_eq!(value, r#""生き返った""#);
                // 実行が先に終わったらタイマーを待たない（既定 30 秒の上限で待たされない）。
                assert!(
                    *elapsed_ms < 5_000,
                    "打ち切りの後もタイマーを待たない: {elapsed_ms} ms"
                );
            }
            other => panic!("次の実行は成功するはずである: {other:?}"),
        }

        actor.shutdown().expect("スレッドを畳める");
    }

    /// メモリの上限で打ち切られ、**その後に同じ actor で次の実行が成功する**（要件 6.2, 6.4）。
    #[test]
    fn メモリの上限で打ち切られ次の実行が成功する() {
        let actor = MacroActor::spawn().expect("actor を起こせる");
        // 大量に確保する（`deno_core` の `test_heap_limits` と同じ形のソース）。時間の上限は
        // 余裕を持たせる — メモリが先に来ることを見るためであり、既定の 30 秒は待たない。
        let outcome = actor
            .run(with_limits(
                "大量に確保する",
                r#"let s = ""; while (true) { s += "Hello"; }"#,
                Limits::new(Duration::from_secs(10), 8 * 1024 * 1024),
            ))
            .expect("actor は要求を断らない");

        match outcome {
            RunOutcome::Aborted { limit, .. } => {
                assert_eq!(limit, LimitKind::Memory, "メモリの上限として種類が載る");
            }
            other => panic!("メモリの上限で打ち切られるはずである: {other:?}"),
        }

        // 復帰: メモリを大量に使った実行の後でも、次の実行が通る。
        let again = actor
            .run(request("生き返る", "'生き返った'"))
            .expect("実行できる");
        assert_eq!(ran_value(&again), r#""生き返った""#);

        actor.shutdown().expect("スレッドを畳める");
    }

    /// メモリの上限が 0 のときは V8 の既定として扱う（`heap_limits(0, 0)`）。**0 を硬い
    /// 上限として渡すと isolate が即座に落ちる**ため、そこを踏まないことを固定する。
    /// 0 を受け付けるかどうかは設定の検証（タスク 4.3）の仕事である。
    #[test]
    fn メモリの上限が零のときはv8の既定で走る() {
        let actor = MacroActor::spawn().expect("actor を起こせる");
        let outcome = actor
            .run(with_limits(
                "既定のヒープ",
                "'走った'",
                Limits::new(Duration::from_secs(5), 0),
            ))
            .expect("実行できる");
        assert_eq!(ran_value(&outcome), r#""走った""#);

        actor.shutdown().expect("スレッドを畳める");
    }

    /// 停止した actor は専用スレッドを畳み、以後の要求を「止まっている」として断る
    /// （tasks.md 1.4「後始末」。成功を装わない）。
    #[test]
    fn 停止したactorはスレッドを畳み以降の要求を断る() {
        let actor = MacroActor::spawn().expect("actor を起こせる");
        let first = actor.run(request("最初", "'走った'")).expect("実行できる");
        assert_eq!(ran_value(&first), r#""走った""#);

        actor.shutdown().expect("スレッドを畳める");
        assert!(!actor.is_running());

        // ここから先は誰も要求を処理しない。要求は**届かない**（`Busy` ではない）。
        match actor.run(request("停止後", "'走らない'")) {
            Err(ActorError::Stopped) => {}
            other => panic!("止まっているとして断られるはずである: {other:?}"),
        }
        // 2 回目の後始末は何もしない（すでに畳まれている）。
        actor.shutdown().expect("畳んだ後の後始末も通る");
    }
}

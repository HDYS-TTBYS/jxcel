//! 検証専用: **実行基盤と上限の実測**（tasks.md 5.4。要件 6.1, 6.2, 11.3）。
//!
//! 5.4 は「実行基盤の実測（isolate の生成・常駐）」「打ち切りの実測（時間の精度・**メモリ**）」
//! を `research.md` へ記録する。5.1 / 5.2 の実起動の観測は**時間の上限**までしか見ていない
//! （標本の `標本の打ち切り` は終わらない繰り返しであり、メモリの上限には当たらない）。
//! **メモリの打ち切りを製品の既定（512 MB）で観測する口が本ファイルである。**
//!
//! # なぜ例なのか（製品のコードを変えない）
//!
//! 実行基盤は**製品の公開面だけ**で測れる（[`MacroRuntime`] の起動・実行と、`/proc` の
//! ピーク値）。計測のために `src/` へ計器を足す必要は無い — 足せば「計器を入れた製品」を
//! 測ることになる。本ファイルは**出荷物の一部ではない**（`examples/` はアプリのバイナリにも
//! 配布物にも入らない。`Cargo.toml` の `make-macro-document` と同じ規律）。
//!
//! # 何を測るか（1 実行 = 1 つの問い）
//!
//! | 副命令 | 問い | 材料 |
//! |---|---|---|
//! | `isolate` | 実行基盤の起動と**実行 1 回ごとの isolate の生成**の費用、および常駐 | 実行の所要と `/proc/self/status` の `VmRSS` / `VmHWM` |
//! | `time` | **時間の上限の精度**（既定 30 秒に対して実際に何 ms で止まるか） | [`RunOutcome::Aborted`] の `elapsed_ms` と実行中の `VmRSS` の山 |
//! | `memory` | **メモリの上限**（既定 512 MB）で打ち切られること | 同じ（`limit` が [`LimitKind::Memory`] であること） |
//!
//! 打ち切りの 2 つはどちらも**製品の既定**（[`Limits::default`] = 30 秒 / 512 MB）で走らせる
//! （5.4 が記録するのは製品の値であり、テスト用に縮めた上限の値ではない。縮めた値の実測は
//! `engine/limits.rs` の module doc と `engine/actor.rs` の結合検査にある）。
//!
//! `VmRSS` / `VmHWM` は Linux の `/proc/self/status` から読む。**他の OS では読めない**ので
//! `計測できない` と出す（macOS / Windows の実測は別の機械が要る。5.4 の記録にもそう書く）。
//!
//! # 使い方
//!
//! ```text
//! cargo build -p macro-runtime --example measure-runtime --release
//! ./target/release/examples/measure-runtime isolate
//! ./target/release/examples/measure-runtime time
//! ./target/release/examples/measure-runtime memory
//! ```
//!
//! **release で走らせる**（`tech.md`「Known Risks」1 と予算の計測が release であり、
//! デバッグビルドの値は製品の値ではない）。
//!
//! 出力は 1 行 1 計測であり、`計測:` で始まる（記録へ写すときの読み口である）。値は
//! **そのプロセスの実測**であり、同じ機械・同じビルドで再現できる範囲で読む。
//!
//! # 終了コード
//!
//! `0` = 期待どおりの結果が観測できた／`2` = 引数が使えない（副命令の欠落・未知）／
//! `1` = 期待と違う結果（打ち切りが来ない・種類が違う・実行そのものが断られた）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use document_format::SheetId;
use macro_runtime::host::overlay::{ColumnTypeInfo, RowPage, RowSpan, SheetInfo};
use macro_runtime::host::{HostError, HostPort};
use macro_runtime::{
    ChangeSet, LimitKind, Limits, MacroKind, MacroName, MacroRecord, MacroRuntime, MacroRuntimeApi,
    RunOutcome, RunRequest, WindowLabel,
};

/// 実行基盤の起動と実行 1 回の費用を測るマクロ（仕事をしない）。
const TRIVIAL_SOURCE: &str = "export default \"計測\";\n";

/// 時間の上限を測るマクロ（終わらない繰り返し。標本の `標本の打ち切り` と同じ形である）。
const SPIN_SOURCE: &str =
    "let total = 0;\nwhile (true) {\n  total += 1;\n}\nexport default total;\n";

/// メモリの上限を測るマクロ（**確保し続ける**）。
///
/// 数を保持し続ける形にする（`chunks` が生かし続けるので GC でも戻らない）。1 回の確保は
/// 1 万要素の倍精度配列（約 80 KB）であり、**文字列ではない** — 文字列の連結は V8 の
/// 最大文字列長（約 512 MB）に先に当たりうるため、上限の種類を取り違える危険がある。
const ALLOCATE_SOURCE: &str =
    "const chunks = [];\nwhile (true) {\n  chunks.push(new Array(10000).fill(1.5));\n}\n";

/// ホストの縫い目（**呼ばれたら理由つきで拒む**）。
///
/// 本ファイルのマクロはホスト API を 1 つも呼ばない（測るのは実行基盤と上限だけである）。
/// それでも実装が要るのは、op の本体が panic してはならないためである — panic は
/// `extern "C"` の境界を越えてプロセスを abort させる（`tech.md`「Known Risks」1 の実測）。
/// **panic ではなく `Err` を返す。**
struct NoHost {
    changes: Mutex<ChangeSet>,
}

impl NoHost {
    fn new() -> Self {
        Self {
            changes: Mutex::new(ChangeSet::new()),
        }
    }

    fn absent(what: &str) -> HostError {
        HostError::new(format!("本計測のマクロは {what} を呼ばない"))
    }
}

impl HostPort for NoHost {
    fn sheets(&self) -> Result<Vec<SheetInfo>, HostError> {
        Err(Self::absent("sheets"))
    }

    fn columns(&self, _sheet: SheetId) -> Result<Vec<ColumnTypeInfo>, HostError> {
        Err(Self::absent("columns"))
    }

    fn read_rows(&self, _sheet: SheetId, _span: RowSpan) -> Result<RowPage, HostError> {
        Err(Self::absent("readRange"))
    }

    fn stage(&self, _change: macro_runtime::Change) -> Result<(), HostError> {
        Err(Self::absent("setCells"))
    }

    fn with_changes(&self, read: &mut dyn FnMut(&ChangeSet)) {
        read(&self.changes.lock().unwrap_or_else(PoisonError::into_inner));
    }

    fn file_read(&self, _path: &str) -> Result<String, HostError> {
        Err(Self::absent("fileRead"))
    }

    fn file_write(&self, _path: &str, _text: &str) -> Result<(), HostError> {
        Err(Self::absent("fileWrite"))
    }

    fn net_fetch(&self, _url: &str) -> Result<String, HostError> {
        Err(Self::absent("netFetch"))
    }
}

/// `/proc/self/status` の 1 つの値（kB）。**Linux 以外では読めない**（`None`）。
fn status_kb(key: &str) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/self/status").ok()?;
        text.lines().find_map(|line| {
            let rest = line.strip_prefix(key)?.strip_prefix(':')?;
            rest.split_whitespace().next()?.parse().ok()
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = key;
        None
    }
}

/// 実行の間、`VmRSS` を刻んで**山**を返す見張り（実行は呼び出し側のスレッドを塞ぐため、
/// 別のスレッドで読む）。
struct RssWatch {
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<u64>,
}

impl RssWatch {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let mut peak = 0;
            while !flag.load(Ordering::Relaxed) {
                if let Some(rss) = status_kb("VmRSS") {
                    peak = peak.max(rss);
                }
                thread::sleep(Duration::from_millis(5));
            }
            peak
        });
        Self { stop, handle }
    }

    /// 見張りを止め、実行中に見た `VmRSS` の山（kB）を返す。
    fn stop(self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.join().unwrap_or(0)
    }
}

/// kB を人の読める形にする（記録は kB の生値と MB の概数を持つ）。
fn kb(value: u64) -> String {
    format!("{value} kB（約 {} MB）", value / 1024)
}

/// 実行を 1 回行い、結果と所要を返す。
fn run(runtime: &MacroRuntime, name: &str, source: &str, limits: Limits) -> (RunOutcome, Duration) {
    let record = MacroRecord::new(MacroName::new(name), MacroKind::TypeScript, source);
    let request = RunRequest::new(record, limits, WindowLabel::from("measure"));
    let started = Instant::now();
    let outcome = runtime
        .run(request, Arc::new(NoHost::new()))
        .expect("実行の要求は受け取られる（直列に呼ぶ）");
    (outcome, started.elapsed())
}

/// 結果を 1 行に写す。
fn describe(outcome: &RunOutcome) -> String {
    match outcome {
        RunOutcome::Ran { value, changes, .. } => {
            format!("ran / 戻り値 = {value} / 変更 = {} 件", changes.total())
        }
        RunOutcome::Failed { failure } => {
            format!("failed / {:?} / {}", failure.kind, failure.message)
        }
        RunOutcome::Aborted {
            limit,
            elapsed_ms,
            failure,
        } => format!(
            "aborted({}) / 打ち切りまでの所要 = {elapsed_ms} ms / {}",
            limit.as_str(),
            failure.message
        ),
    }
}

/// 実行の結果と、実行の前後の `VmRSS` / `VmHWM`、実行中の `VmRSS` の山を 1 行ずつ出す。
fn report(label: &str, outcome: &RunOutcome, elapsed: Duration, peak_rss: u64) {
    println!(
        "計測: {label} = {} ms / {}",
        elapsed.as_millis(),
        describe(outcome)
    );
    if let Some(peak) = (peak_rss > 0).then_some(peak_rss) {
        println!("計測: {label} の実行中の VmRSS の山 = {}", kb(peak));
    }
}

/// 実行基盤の起動（`MacroRuntime::new`）と実行 1 回ごとの isolate の生成を測る。
///
/// **isolate は実行ごとに作られ、所有スレッドの上で落ちる**（design.md 決定 1）ので、
/// 実行 1 回の所要は毎回「生成 + 変換 + 実行 + 後始末」を含む。起動（`MacroRuntime::new`）は
/// isolate を作らない（遅延）ことを、両者の差として観測する。
fn measure_isolate(code: &mut u8) {
    let rss_before = status_kb("VmRSS");
    let hwm_before = status_kb("VmHWM");

    let started = Instant::now();
    let runtime = MacroRuntime::new().expect("実行基盤（actor）を起こせる");
    let spawn = started.elapsed();
    println!(
        "計測: 実行基盤の起動（isolate は作らない） = {} ms",
        spawn.as_millis()
    );

    for attempt in 1..=3 {
        let (outcome, elapsed) = run(
            &runtime,
            "計測（仕事をしない）",
            TRIVIAL_SOURCE,
            Limits::default(),
        );
        let peak = 0;
        report(
            &format!("実行 {attempt} 回目（isolate の生成を含む）"),
            &outcome,
            elapsed,
            peak,
        );
        if !matches!(outcome, RunOutcome::Ran { .. }) {
            println!("計測: 期待と違う結果（{attempt} 回目）");
            *code = 1;
        }
    }

    if let Some(rss) = status_kb("VmRSS") {
        println!(
            "計測: 実行の前後の VmRSS = {} → {}（実行ごとに isolate は落ちるが、確保したページはプロセスに残る）",
            kb(rss_before.unwrap_or(0)),
            kb(rss)
        );
    } else {
        println!("計測: VmRSS / VmHWM は計測できない（Linux 以外）");
    }
    if let (Some(before), Some(after)) = (hwm_before, status_kb("VmHWM")) {
        println!(
            "計測: VmHWM（実行基盤が触ったピーク） = {} → {}（増分 {}）",
            kb(before),
            kb(after),
            kb(after.saturating_sub(before))
        );
    }

    let started = Instant::now();
    runtime.shutdown().expect("専用スレッドを畳める");
    println!(
        "計測: 実行基盤の後始末 = {} ms",
        started.elapsed().as_millis()
    );
}

/// 時間の上限（製品の既定 = 30 秒）の精度を測る。
fn measure_time(code: &mut u8) {
    let limits = Limits::default();
    println!("計測: 時間の上限 = {} ms（既定）", limits.time.as_millis());
    let runtime = MacroRuntime::new().expect("実行基盤（actor）を起こせる");
    let watch = RssWatch::start();
    let (outcome, elapsed) = run(&runtime, "計測（終わらない繰り返し）", SPIN_SOURCE, limits);
    let peak = watch.stop();
    report("打ち切り（時間）", &outcome, elapsed, peak);

    match &outcome {
        RunOutcome::Aborted {
            limit, elapsed_ms, ..
        } => {
            if *limit != LimitKind::Time {
                println!("計測: 種類が時間ではない（{}）", limit.as_str());
                *code = 1;
            }
            let declared = limits.time.as_millis() as i64;
            println!(
                "計測: 上限と実測の差 = {} ms（上限 {} ms / 実測 {} ms）",
                *elapsed_ms as i64 - declared,
                declared,
                elapsed_ms
            );
        }
        _ => {
            println!("計測: 時間の上限で打ち切られなかった");
            *code = 1;
        }
    }

    // 打ち切りの後に同じ実行基盤で次の実行ができる（要件 6.4 の復帰）。
    let (again, elapsed) = run(
        &runtime,
        "計測（打ち切りの後）",
        TRIVIAL_SOURCE,
        Limits::default(),
    );
    report("打ち切りの後の実行", &again, elapsed, 0);
    if !matches!(again, RunOutcome::Ran { .. }) {
        println!("計測: 打ち切りの後の実行が成功しない");
        *code = 1;
    }
    runtime.shutdown().expect("専用スレッドを畳める");
}

/// メモリの上限（製品の既定 = 512 MB）で打ち切られることを測る。
fn measure_memory(code: &mut u8) {
    let limits = Limits::default();
    println!(
        "計測: メモリの上限 = {} B（既定 = {} MB）",
        limits.memory_bytes,
        limits.memory_bytes / (1024 * 1024)
    );
    let hwm_before = status_kb("VmHWM");
    let runtime = MacroRuntime::new().expect("実行基盤（actor）を起こせる");
    let watch = RssWatch::start();
    let (outcome, elapsed) = run(&runtime, "計測（確保し続ける）", ALLOCATE_SOURCE, limits);
    let peak = watch.stop();
    report("打ち切り（メモリ）", &outcome, elapsed, peak);
    if let (Some(before), Some(after)) = (hwm_before, status_kb("VmHWM")) {
        println!(
            "計測: VmHWM（確保したピーク） = {} → {}（増分 {}）",
            kb(before),
            kb(after),
            kb(after.saturating_sub(before))
        );
    }

    match &outcome {
        RunOutcome::Aborted { limit, .. } if *limit == LimitKind::Memory => {}
        RunOutcome::Aborted { limit, .. } => {
            println!("計測: 種類がメモリではない（{}）", limit.as_str());
            *code = 1;
        }
        _ => {
            println!("計測: メモリの上限で打ち切られなかった");
            *code = 1;
        }
    }

    // 打ち切りの後に同じ実行基盤で次の実行ができる（要件 6.4 の復帰）。
    let (again, elapsed) = run(
        &runtime,
        "計測（打ち切りの後）",
        TRIVIAL_SOURCE,
        Limits::default(),
    );
    report("打ち切りの後の実行", &again, elapsed, 0);
    if !matches!(again, RunOutcome::Ran { .. }) {
        println!("計測: 打ち切りの後の実行が成功しない");
        *code = 1;
    }
    runtime.shutdown().expect("専用スレッドを畳める");
}

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(what) = args.next() else {
        eprintln!("使い方: measure-runtime <isolate|time|memory>");
        return std::process::ExitCode::from(2_u8);
    };
    if args.next().is_some() {
        eprintln!("使い方: measure-runtime <isolate|time|memory>");
        return std::process::ExitCode::from(2_u8);
    }

    println!(
        "計測: 実行環境 = {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!(
        "計測: ビルド = {}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    let mut code = 0_u8;
    match what.as_str() {
        "isolate" => measure_isolate(&mut code),
        "time" => measure_time(&mut code),
        "memory" => measure_memory(&mut code),
        other => {
            eprintln!("使い方: measure-runtime <isolate|time|memory>（未知: {other}）");
            return std::process::ExitCode::from(2_u8);
        }
    }
    std::process::ExitCode::from(code)
}

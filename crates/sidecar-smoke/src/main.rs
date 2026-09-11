//! 補助プロセス機構の検証専用ヘルパー（tasks.md 1.6）。
//!
//! # 目的
//!
//! 本実行ファイルは、配布物に同梱する補助プロセスの**機構を実プロセスで検証するためだけ**に
//! 存在する。役割は次の 2 つに限られる。
//!
//! 1. コマンドラインで受け取った親プロセスの識別子を監視し、親が消えたら自ら終了する。
//! 2. 標準入力の各行に、標準出力で 1 行ずつ応答する。
//!
//! さらに、終了保証（tasks.md 3.3）を検証するために限った 3 つの検証専用オプションを持つ:
//!
//! - `--spawn-grandchild` — 自身と同じ実行ファイルを `--idle` で 1 つ起動する。子が孫を
//!   持つ状況を作るためだけに存在する。
//! - `--ignore-term` — 穏やかな終了信号を無視する。猶予段では終わらず強制段でのみ終わる
//!   ことを検証するためだけに存在する。
//! - `--idle` — 親監視も応答もしない待機プロセス。`--spawn-grandchild` が作る孫が使う。
//!   孫が親監視を持つと、子が終了した時点で孫が自ら終了してしまい、プロセスグループ /
//!   Job Object による終了保証そのものを検証できなくなるため、意図的に監視させない。
//!
//! これらは配布しない検証専用の相手役である（design.md「Out of Boundary」、research.md
//! 決定 9: 実物の言語サーバを登録するのは下流の `macro-editor-lsp` である）。**上記の役割を
//! 超える実用的な機能を足してはならない。** プロトコル・JSON・IPC・引数解析フレームワーク・
//! 記録機構・設定読み取りは意図的に持たない。ここに機能が増えた時点で、検証したい機構
//! （tasks.md 3.x）と検証対象が混ざり、何を確かめているのか分からなくなる。
//!
//! # 本プロセスを消費するタスク
//!
//! - 1.7 — ターゲットトリプル接尾辞付きの名前で 3 OS の配布物へ配置する（要件 5.1）。
//! - 3.2 / 3.3 / 3.4 / 3.5 — 起動・共有・終了保証・出力取得・親監視による自己終了。
//! - 10.2 — 配布物から取り出して起動できること、同梱前とバイト一致することを CI で検証する。
//! - 10.7 — 通常終了・強制終了・孫ありの 3 条件で残留プロセスが 0 であることを検証する。
//!
//! 実行ファイル名の語幹は `sidecar-smoke` でなければならない。これは
//! `crates/app-shell` の `SidecarKind::Smoke::as_str()` が返す値と一致し、1.7 の配置名と
//! 3.5 の孤児掃除（PID と実行ファイル名の両方で照合する）が共有する契約である。
//!
//! # コマンドライン
//!
//! ```text
//! sidecar-smoke --parent-pid <PID> [--spawn-grandchild] [--ignore-term]
//! sidecar-smoke --idle [--ignore-term]
//! ```
//!
//! `<PID>` は監視する親プロセスの識別子で、1 以上の整数に限る。`0` は Unix では
//! プロセスグループを指してしまい「消えた」ことを検出できないため受け付けない。
//! `--idle` は `--parent-pid` と併用しない。引数の欠落・未知の引数・数値でない値・範囲外の値・
//! 不正な組み合わせは、標準エラー出力へ使い方を出して終了コード 2 で即座に終了する（ハングしない）。
//!
//! # 標準出力の形式
//!
//! 起動時に 1 行、以後は標準入力の 1 行につき 1 行を返す。**いずれも改行まで含めて都度
//! flush する**（行単位の購読側が順序どおりに受け取れるようにするため。tasks.md 3.4 が
//! 行の順序に依存する）。`--spawn-grandchild` のときは孫の起動後に `grandchild pid=<識別子>`
//! の行も出す。
//!
//! ```text
//! sidecar-smoke ready pid=<自身の識別子> parent_pid=<監視対象の識別子>
//! sidecar-smoke grandchild pid=<孫の識別子>
//! echo: <入力行の内容>
//! ```
//!
//! `--idle` では `sidecar-smoke ready pid=<自身の識別子> mode=idle` の 1 行だけを出す。
//!
//! 標準入力が閉じても自発的には終了しない。終了は親監視（または監督側からの終了操作）の
//! 責務であり、補助プロセスは監督側が生かしておく限り生存する。
//!
//! # 終了
//!
//! 親が消えたことを検出したとき、終了コード 0 で自ら終了する。状態の保存や後始末は持たない
//! （プロセスグループ / Job Object の後始末は監督側 = tasks.md 3.3 が所有する）。

use std::io::{BufRead, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

/// 親の生存を確認する間隔。最悪検出遅延はこの間隔 + スケジューリング遅延である。
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// 引数が不正なときの終了コード。
const EXIT_USAGE: i32 = 2;

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Some(options) => options,
        None => {
            print_usage();
            std::process::exit(EXIT_USAGE);
        }
    };

    // 終了保証（tasks.md 3.3）の検証専用。穏やかな終了信号を握りつぶす。
    if options.ignore_term {
        ignore_termination_signal();
    }

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match options.mode {
        Mode::Idle => {
            let _ = writeln!(
                out,
                "sidecar-smoke ready pid={} mode=idle",
                std::process::id()
            );
            let _ = out.flush();
            drop(out);
            park_forever();
        }
        Mode::Supervised {
            parent_pid,
            spawn_grandchild,
        } => {
            if spawn_grandchild {
                let grandchild = spawn_grandchild_process();
                let _ = writeln!(out, "sidecar-smoke grandchild pid={grandchild}");
            }
            // 起動行。監視対象と自身の識別子を 1 行で示し、行単位の購読側に起動を伝える。
            let _ = writeln!(
                out,
                "sidecar-smoke ready pid={} parent_pid={parent_pid}",
                std::process::id()
            );
            let _ = out.flush();
            drop(out);

            // 監視は専用スレッドで行う。本体は標準入力の応答に専念でき、どちらの経路からでも
            // プロセスを終了できる。
            std::thread::spawn(move || watch_parent(parent_pid));

            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) => {
                        // 1 行ごとに応答し、改行まで flush する。書き込みに失敗したら（購読側が
                        // 閉じた等）応答を諦め、親監視に終了を委ねる。
                        if writeln!(out, "echo: {line}").is_err() || out.flush().is_err() {
                            break;
                        }
                    }
                    // 読み取りの失敗も入力の終端として扱う。
                    Err(_) => break,
                }
            }
            drop(out);

            // 標準入力が尽きても生存し続ける。終了は親監視が決める。
            park_forever();
        }
    }
}

/// コマンドラインの解釈結果。
struct Options {
    mode: Mode,
    /// 穏やかな終了信号を無視するか（検証専用）。
    ignore_term: bool,
}

/// 動作の種類。
enum Mode {
    /// `--parent-pid` を監視し、標準入力へ応答する通常の補助プロセス。
    Supervised {
        parent_pid: u32,
        /// `--spawn-grandchild` が指定されたか（検証専用）。
        spawn_grandchild: bool,
    },
    /// `--idle`。親監視も応答もしない待機プロセス（`--spawn-grandchild` が作る孫用）。
    Idle,
}

impl Options {
    /// 引数を解釈する。不正なら `None`。
    fn parse<I>(mut args: I) -> Option<Options>
    where
        I: Iterator<Item = String>,
    {
        let mut parent_pid: Option<u32> = None;
        let mut idle = false;
        let mut spawn_grandchild = false;
        let mut ignore_term = false;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--parent-pid" => {
                    // 重複は受け付けない。
                    if parent_pid.is_some() {
                        return None;
                    }
                    parent_pid = Some(parse_pid(&args.next()?)?);
                }
                "--spawn-grandchild" => spawn_grandchild = true,
                "--ignore-term" => ignore_term = true,
                "--idle" => idle = true,
                // 余分な引数は黙って無視しない。
                _ => return None,
            }
        }

        let mode = if idle {
            // `--idle` は監視対象も孫も持たない。併用は不正である。
            if parent_pid.is_some() || spawn_grandchild {
                return None;
            }
            Mode::Idle
        } else {
            Mode::Supervised {
                parent_pid: parent_pid?,
                spawn_grandchild,
            }
        };
        Some(Options { mode, ignore_term })
    }
}

/// `<PID>` を解釈する。0 は Unix でプロセスグループを指し存在確認が常に成功するため拒否し、
/// 符号付き 32 ビットを超える値も pid として不正である。
fn parse_pid(raw: &str) -> Option<u32> {
    let pid: u32 = raw.parse().ok()?;
    if pid == 0 || pid > i32::MAX as u32 {
        return None;
    }
    Some(pid)
}

/// 検証専用: 自身と同じ実行ファイルを `--idle` で起動し、その識別子を返す。
///
/// 孫は親監視を持たない（`--idle`）。親監視を持たせると、子が終了した時点で孫が自ら終了して
/// しまい、プロセスグループ / Job Object による終了保証そのものを検証できなくなる。
/// 起動した子プロセスハンドルは待たずに破棄する（孫は監督側が終了させる）。
fn spawn_grandchild_process() -> u32 {
    let executable = std::env::current_exe().expect("自身の実行ファイルパスを取得できる");
    let child = Command::new(executable)
        .arg("--idle")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("孫プロセスを起動できる");
    child.id()
}

/// 穏やかな終了信号を無視する（検証専用）。
#[cfg(unix)]
fn ignore_termination_signal() {
    // SIGTERM を無視する。猶予段（SIGTERM）では終了せず、強制段（SIGKILL）でのみ終了する
    // ことを検証するための振る舞いである（tasks.md 3.3）。
    unsafe {
        libc::signal(libc::SIGTERM, libc::SIG_IGN);
    }
}

/// 穏やかな終了信号を無視する（検証専用）。
#[cfg(windows)]
fn ignore_termination_signal() {
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

    /// コンソール制御イベントを握りつぶすハンドラ。`TRUE`（= 1）は「処理した」を意味する。
    unsafe extern "system" fn ignore(_event: u32) -> windows_sys::core::BOOL {
        1
    }

    // CTRL_BREAK_EVENT を無視する。ジョブの強制終了（TerminateJobObject / KILL_ON_JOB_CLOSE）は
    // カーネルが行うため防げない（tasks.md 3.3 の猶予→強制の検証）。
    // SAFETY: ハンドラは静的な関数であり、プロセス終了まで有効である。
    unsafe {
        SetConsoleCtrlHandler(Some(ignore), 1);
    }
}

fn park_forever() -> ! {
    loop {
        std::thread::park();
    }
}

fn print_usage() {
    eprintln!("使い方: sidecar-smoke --parent-pid <PID> [--spawn-grandchild] [--ignore-term]");
    eprintln!("        sidecar-smoke --idle [--ignore-term]");
    eprintln!("  <PID> は監視する親プロセスの識別子（1 以上の整数）");
    eprintln!("  --spawn-grandchild / --ignore-term / --idle は終了保証の検証専用である");
}

/// 親プロセスを監視し、消えたら終了コード 0 で自ら終了する。
///
/// Unix では `kill(pid, 0)` だけでは不十分である。親が死んでも回収されるまでゾンビとして
/// 残り、`kill(pid, 0)` は成功し続ける。そこで、監視対象が実の親である場合は `getppid()` の
/// 変化（reparent）も合わせて見る。親が終了した時点で子は再親付けされるため、`getppid()` は
/// ゾンビの回収を待たずに監視対象と異なる値になる（Linux で実測で確認済み。macOS にも
/// reparent の意味論は同一で、`/proc` を必要としない）。
#[cfg(unix)]
fn watch_parent(parent_pid: u32) {
    let watched = parent_pid as libc::pid_t;
    // 起動時点の自身の親。監視対象と一致するなら、それは実の親である。
    let initial_ppid = unsafe { libc::getppid() };
    let direct_parent = initial_ppid == watched;

    loop {
        if !parent_alive(watched, direct_parent) {
            // 親が消えた。後始末は監督側が持つため、そのまま終了する。
            std::process::exit(0);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// 監視対象が生存しているかを返す（Unix）。
#[cfg(unix)]
fn parent_alive(watched: libc::pid_t, direct_parent: bool) -> bool {
    // 1. 存在確認。`kill(pid, 0)` はシグナルを送らずに存在と権限だけを確認する。
    if unsafe { libc::kill(watched, 0) } == -1 {
        match std::io::Error::last_os_error().raw_os_error() {
            // そのようなプロセスは存在しない。
            Some(libc::ESRCH) => return false,
            // 存在するがシグナルを送る権限がない。生存扱いとする。
            Some(libc::EPERM) => {}
            // その他の失敗は判定不能。安全側（生存）に倒す。
            _ => {}
        }
    }

    // 2. reparent の検出。監視対象が実の親である場合に限り、`getppid()` が監視対象から
    //    離れた時点で親は終了している（ゾンビの回収は待たない）。
    if direct_parent && unsafe { libc::getppid() } != watched {
        return false;
    }

    true
}

/// 親プロセスを監視し、消えたら終了コード 0 で自ら終了する。
///
/// Windows ではプロセスハンドルを待つ。`OpenProcess` で `SYNCHRONIZE` 権を持つハンドルを
/// 開き、`WaitForSingleObject` をタイムアウト 0 で繰り返し呼ぶ。ハンドルがシグナル状態に
/// なった時点で親は終了している。開いたハンドルは終了前に `CloseHandle` で解放する。
#[cfg(windows)]
fn watch_parent(parent_pid: u32) {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    // 親が既に存在しない（または開けない）なら、起動直後に自己終了する。
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, parent_pid) };
    if handle.is_null() {
        std::process::exit(0);
    }

    loop {
        match unsafe { WaitForSingleObject(handle, 0) } {
            // 親が終了した。ハンドルを明示的に解放してから終了する。
            WAIT_OBJECT_0 => {
                unsafe { CloseHandle(handle) };
                std::process::exit(0);
            }
            // まだ生存している。待機を続ける。
            WAIT_TIMEOUT => {}
            // 待機が失敗した場合は判定不能であり、孤児を残すより終了する側に倒す。
            _ => {
                unsafe { CloseHandle(handle) };
                std::process::exit(0);
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

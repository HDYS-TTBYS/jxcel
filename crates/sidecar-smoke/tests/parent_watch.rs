//! 親プロセスの監視と標準出力への応答の検証（tasks.md 1.6 の完了状態）。
//!
//! 実プロセスを起動して検証する:
//!
//! - テストプロセスがダミー親を起動し、その識別子を監視させた補助プロセスを起動する。
//! - ダミー親を強制終了し、**回収まで行って**（ゾンビを残さない）補助プロセスが自ら
//!   終了するまでに要した時間を測る。
//! - 既に死んでいる親を渡した場合に、ハングせず即座に自己終了することも確かめる。
//! - 生存している親（テストプロセス自身）を渡し、標準入力の各行に標準出力で応答する
//!   ことを確かめる。
//!
//! シェルに依存しない。ダミー親はテスト実行ファイル自身を libtest の
//! `--ignored --exact dummy_parent` で再実行して作るため、3 OS で同じ経路が動く。

use std::io::{BufRead, BufReader, Write};
use std::ops::{Deref, DerefMut};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

/// 親の消滅から補助プロセスが自己終了するまでの上限（完了状態の「一定時間内」）。
const SELF_EXIT_DEADLINE: Duration = Duration::from_secs(5);
/// 標準出力の 1 行、およびプロセス終了を待つ上限。壊れた実装がハングせず失敗するようにする。
const WAIT_DEADLINE: Duration = Duration::from_secs(5);
/// ダミー親が自発的に生き続ける時間。テストが kill し損ねても一定時間で消える。
const DUMMY_LIFETIME_SECS: u64 = 60;

/// 補助プロセスの実行ファイル。ビルド済みの実物をそのまま起動する。
fn sidecar_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sidecar-smoke")
}

/// 失敗経路でも子プロセスを残さないための後始末。明示的に kill 済みなら何もしない。
struct Guard(Child);

impl Guard {
    fn new(child: Child) -> Self {
        Guard(child)
    }
}

impl Deref for Guard {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}

impl DerefMut for Guard {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(None) = self.0.try_wait() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// 標準出力を行単位で読み、タイムアウト付きで受け取れるようにする。
fn line_reader(stdout: std::process::ChildStdout) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if tx
                        .send(line.trim_end_matches(['\r', '\n']).to_string())
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });
    rx
}

fn recv_line(rx: &Receiver<String>, what: &str) -> String {
    rx.recv_timeout(WAIT_DEADLINE)
        .unwrap_or_else(|_| panic!("{what} を {WAIT_DEADLINE:?} 以内に受け取れなかった"))
}

/// 期限付きでプロセスの終了を待つ。期限内に終了すれば終了状態を返す。
fn wait_with_deadline(child: &mut Child, deadline: Duration) -> Option<ExitStatus> {
    let until = Instant::now() + deadline;
    loop {
        match child.try_wait().expect("try_wait に失敗した") {
            Some(status) => return Some(status),
            None => {
                if Instant::now() >= until {
                    return None;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

/// テスト実行ファイル自身をダミー親として起動する。`dummy_parent` だけを走らせて眠らせる。
fn spawn_dummy_parent() -> Guard {
    let exe = std::env::current_exe().expect("テスト実行ファイルの解決に失敗した");
    Guard::new(
        Command::new(exe)
            .args(["--ignored", "--exact", "dummy_parent", "--nocapture"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("ダミー親の起動に失敗した"),
    )
}

fn spawn_sidecar(parent_pid: u32) -> Guard {
    Guard::new(
        Command::new(sidecar_bin())
            .arg("--parent-pid")
            .arg(parent_pid.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("補助プロセスの起動に失敗した"),
    )
}

/// ダミー親としてのみ実行するテスト。通常のテスト実行では `--ignored` により除外される。
#[test]
#[ignore = "テスト用のダミー親プロセスとしてのみ実行する"]
fn dummy_parent() {
    thread::sleep(Duration::from_secs(DUMMY_LIFETIME_SECS));
}

/// 完了状態: 親プロセスを強制終了させると、補助プロセスが一定時間内に自ら終了する。
#[test]
fn self_exits_when_parent_is_killed() {
    let mut dummy = spawn_dummy_parent();
    let dummy_pid = dummy.id();

    let mut sidecar = spawn_sidecar(dummy_pid);
    let rx = line_reader(sidecar.stdout.take().expect("stdout パイプが無い"));

    // 起動行に監視対象の識別子が現れることを確かめる（起動を待つ役割も兼ねる）。
    let ready = recv_line(&rx, "起動行");
    assert!(ready.contains("ready"), "起動行の形式が違う: {ready}");
    assert!(
        ready.contains(&format!("parent_pid={dummy_pid}")),
        "監視対象の識別子が起動行に無い: {ready}"
    );

    // 親を強制終了し、回収する（ゾンビのままにしない。これが本テストの肝）。
    dummy.kill().expect("ダミー親の強制終了に失敗した");
    dummy.wait().expect("ダミー親の回収に失敗した");

    // ここから補助プロセスが自力で終了するまでの時間を測る。
    let killed_at = Instant::now();
    let status = wait_with_deadline(&mut sidecar, SELF_EXIT_DEADLINE);
    let elapsed = killed_at.elapsed();

    match status {
        Some(status) => {
            println!("親の強制終了から {elapsed:?} で補助プロセスが自己終了した（{status}）");
            assert_eq!(
                status.code(),
                Some(0),
                "自己終了の終了コードは 0 であるべき: {status}"
            );
        }
        None => {
            let _ = sidecar.kill();
            let _ = sidecar.wait();
            panic!("親の強制終了から {SELF_EXIT_DEADLINE:?} 以内に補助プロセスが自己終了しなかった");
        }
    }
}

/// 既に死んでいる親を渡した場合、ハングせず即座に自己終了する。
#[test]
fn exits_promptly_when_parent_is_already_gone() {
    // ダミーを起動して即座に殺し、回収する。以降その識別子のプロセスは存在しない。
    let mut dummy = spawn_dummy_parent();
    let dead_pid = dummy.id();
    dummy.kill().expect("ダミー親の強制終了に失敗した");
    dummy.wait().expect("ダミー親の回収に失敗した");

    let mut sidecar = spawn_sidecar(dead_pid);
    let started = Instant::now();
    let status = wait_with_deadline(&mut sidecar, SELF_EXIT_DEADLINE);
    let elapsed = started.elapsed();

    match status {
        Some(status) => {
            println!("死んだ親を渡してから {elapsed:?} で自己終了した（{status}）");
            assert_eq!(status.code(), Some(0), "自己終了の終了コードは 0 であるべき: {status}");
        }
        None => {
            let _ = sidecar.kill();
            let _ = sidecar.wait();
            panic!("死んだ親を渡しても {SELF_EXIT_DEADLINE:?} 以内に自己終了しなかった");
        }
    }
}

/// 生存している親（テストプロセス自身）を渡し、標準入力の各行に標準出力で応答する。
#[test]
fn echoes_each_stdin_line_on_stdout_in_order() {
    let mut sidecar = spawn_sidecar(std::process::id());
    let own_pid = sidecar.id();
    let rx = line_reader(sidecar.stdout.take().expect("stdout パイプが無い"));

    let ready = recv_line(&rx, "起動行");
    assert!(
        ready.contains(&format!("pid={own_pid}")),
        "自身の識別子が起動行に無い: {ready}"
    );

    let mut stdin = sidecar.stdin.take().expect("stdin パイプが無い");
    writeln!(stdin, "first").expect("stdin への書き込みに失敗した");
    stdin.flush().expect("stdin の flush に失敗した");
    writeln!(stdin, "second").expect("stdin への書き込みに失敗した");
    stdin.flush().expect("stdin の flush に失敗した");

    let first = recv_line(&rx, "1 行目の応答");
    let second = recv_line(&rx, "2 行目の応答");
    println!("応答: {first:?} / {second:?}");
    assert!(
        first.contains("first"),
        "1 行目の応答が入力に対応しない: {first}"
    );
    assert!(
        second.contains("second"),
        "2 行目の応答が入力に対応しない: {second}"
    );
}

/// 引数の欠落・不正は、ハングせず速やかに非 0 で終了する。
#[test]
fn rejects_missing_and_invalid_parent_pid() {
    let cases: [&[&str]; 4] = [
        &[],
        &["--parent-pid"],
        &["--parent-pid", "abc"],
        &["--parent-pid", "0"],
    ];
    for args in cases {
        let mut child = Guard::new(
            Command::new(sidecar_bin())
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .expect("補助プロセスの起動に失敗した"),
        );
        let status = wait_with_deadline(&mut child, WAIT_DEADLINE);
        match status {
            Some(status) => assert!(
                !status.success(),
                "不正な引数 {args:?} が非 0 で終了しなかった: {status}"
            ),
            None => panic!("不正な引数 {args:?} でハングした"),
        }
    }
}

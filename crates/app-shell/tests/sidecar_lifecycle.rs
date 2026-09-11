//! 補助プロセスの起動・共有・再起動、起動失敗の区別、終了保証、出力の取得と予期せぬ終了の通知、
//! 残留プロセスの掃除と親監視の結線（要件 5.4〜5.9、tasks.md 3.2〜3.5）。
//!
//! **完了状態**（同じ種類への要求を並行して 10 回出しても起動したプロセスが 1 つであること）を、
//! 実プロセスで固定する。実際に起動するのは配置規約（tasks.md 1.7）が置く原本
//! `sidecars/sidecar-smoke-<ターゲットトリプル>` であり、3.1 と同じく、未配置のクローンでは
//! 理由を出してスキップする。CI は `Test` の前に `Stage sidecar original` を走らせるため、
//! CI では常に実経路が通る（tasks.md 3.1 の申し送り）。
//!
//! 起動失敗の区別は 4 つすべてを観測する: 実行ファイル不在（`NotFound`）・実行権限なし
//! （`NotExecutable`）・整合性不一致（`IntegrityMismatch`）・起動失敗（`Spawn`）。最後の 1 つは
//! 「実在し、実行権限もあり、整合性検査だけを通せない」ファイルを本物の起動経路へ渡す必要が
//! あるため、整合性検査を差し替える seam（[`Supervisor::with_verifier`]）を使う。本番の
//! [`Supervisor::new`] は実検査（[`app_shell::sidecar::integrity::verify`]）をそのまま使う。
//!
//! 残留プロセスの掃除（tasks.md 3.5）は、監督を経由せずに直接起動した補助プロセスを
//! **前回の実行が残したもの**と見立てて検証する。掃除は識別子だけでなく**実行ファイル名**でも
//! 照合するため、別名へ複製したプロセスを終了させないこと（PID 再利用の誤終了の防止）を
//! 同じ節で固定する。親監視の引数は監督が注入する（`sidecar_spec` は与えない）。
//!
//! プロセス数は OS から数える。Linux は `/proc`（コンテナ内に `pgrep` / `ps` が無いことを実測）、
//! macOS は `pgrep`、Windows は `tasklist` を使う。cargo は同一バイナリ内のテストを並行に
//! 走らせるため、このファイルのテストは直列化する。子プロセスは [`SidecarGuard`] の `Drop` で
//! 必ず強制終了して回収し、成功・失敗のどちらの経路でも孤児を残さない。直接起動した検証用の
//! プロセスも、各テストが最後に回収する（ゾンビを残さない）。

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use app_shell::sidecar::integrity::{IntegrityError, BUILD_TARGET_TRIPLE, EXPECTED_DIGESTS};
use app_shell::sidecar::supervisor::{
    SidecarEvent, SidecarHandle, SidecarSpec, SidecarStream, SidecarSupervisor, SpawnError,
    Supervisor,
};
use app_shell::sidecar::SidecarKind;

// ---------------------------------------------------------------------------
// 直列化
// ---------------------------------------------------------------------------

/// このファイルのテストを直列化する。プロセス一覧を OS から数えるため、同時に複数のテストが
/// 補助プロセスを起動すると数え合わせが壊れる。テストが panic しても毒を回復して先へ進む
/// （臨界区間に壊れる不変条件は無い）。
static LIFECYCLE_SERIAL: Mutex<()> = Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    LIFECYCLE_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// 配置済み原本・一時ディレクトリ
// ---------------------------------------------------------------------------

/// リポジトリルートを `CARGO_MANIFEST_DIR` から解決する。カレントディレクトリに依存しない。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/app-shell はリポジトリ直下の crates/ 配下にある")
        .to_path_buf()
}

/// このビルドのターゲットトリプル向けの、配置規約（tasks.md 1.7）が決める原本のパス。
fn staged_original_path() -> PathBuf {
    let exe = if BUILD_TARGET_TRIPLE.contains("windows") {
        ".exe"
    } else {
        ""
    };
    repo_root().join("sidecars").join(format!(
        "{}-{BUILD_TARGET_TRIPLE}{exe}",
        SidecarKind::Smoke.as_str()
    ))
}

/// 実プロセスを起動できる原本が使えるときだけ `Some(パス)` を返す。`build.rs` がこのトリプル向けに
/// ダイジェストを発行していない（原本が未配置の）クローンでは `None`。
fn staged_original() -> Option<PathBuf> {
    let registered = EXPECTED_DIGESTS
        .iter()
        .any(|(kind, _)| *kind == SidecarKind::Smoke);
    let path = staged_original_path();
    (registered && path.is_file()).then_some(path)
}

/// 原本が使えないときに理由を出してスキップする（3.1 と同じ方針）。
fn skip_staged(test: &str) {
    eprintln!(
        "{test} をスキップします: 原本 {} が未配置、またはこのトリプルのダイジェストが未登録です。\
         `bash scripts/stage-sidecars.sh` を実行して再ビルドすると実経路が走ります。",
        staged_original_path().display()
    );
}

/// テスト中だけ使う一時ディレクトリ。`Drop` でベストエフォート削除する。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が UNIX エポックより前")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "jxcel-sidecar-lifecycle-{tag}-{}-{nanos}-{seq}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("一時ディレクトリを作成できる");
        TempDir { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, bytes).expect("一時ファイルへ書き込める");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 実行ビットを立てる（Unix のみ）。Windows に実行ビットは存在しない。
#[cfg(unix)]
fn make_executable(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .expect("メタデータを読める")
        .permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).expect("権限を設定できる");
}

/// 起動する補助プロセスの仕様。
///
/// **親監視の引数（`--parent-pid`）はここでは与えない。** 監督が自分の識別子を注入する
/// （要件 5.6、tasks.md 3.5 の親監視の結線）。この関数が引数を足すと、注入の経路が試されない
/// まま「テストだけが親監視を渡している」状態になり、本番の結線が壊れても気づけない。
fn sidecar_spec(executable: &Path) -> SidecarSpec {
    SidecarSpec {
        kind: SidecarKind::Smoke,
        executable: executable.to_path_buf(),
        args: Vec::new(),
    }
}

/// さらに孫プロセスを 1 つ起動する補助プロセスの仕様。`--spawn-grandchild` は検証専用の
/// オプションであり、`crates/sidecar-smoke` の実用的な機能ではない（tasks.md 3.3 の完了状態を
/// 実プロセスで確かめるためだけに存在する）。
fn grandchild_spec(executable: &Path) -> SidecarSpec {
    let mut spec = sidecar_spec(executable);
    spec.args.push("--spawn-grandchild".to_string());
    spec
}

// ---------------------------------------------------------------------------
// OS からのプロセス数の取得
// ---------------------------------------------------------------------------

/// 実行中の `sidecar-smoke` プロセスの数を OS から数える。
///
/// Linux は `/proc` を直接読む — 検証用コンテナには `pgrep` / `ps` が入っていない（実測）。
/// ゾンビは `cmdline` が空なので数えない（= 「実行中」だけを数える）。
#[cfg(target_os = "linux")]
fn count_running_sidecars() -> usize {
    let stem = SidecarKind::Smoke.as_str();
    let self_pid = std::process::id();
    let mut count = 0usize;
    let entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.parse::<u32>().is_err() {
            continue;
        }
        let dir = entry.path();
        // 直接の子だけを数える。無関係な同名プロセスを拾わない。
        let status = match fs::read_to_string(dir.join("status")) {
            Ok(status) => status,
            Err(_) => continue,
        };
        let parent = status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))
            .and_then(|value| value.trim().parse::<u32>().ok());
        if parent != Some(self_pid) {
            continue;
        }
        let cmdline = match fs::read(dir.join("cmdline")) {
            Ok(cmdline) => cmdline,
            Err(_) => continue,
        };
        if !cmdline.is_empty() && String::from_utf8_lossy(&cmdline).contains(stem) {
            count += 1;
        }
    }
    count
}

/// macOS には `/proc` が無いため `pgrep` を使う。`-P` で直接の子に限り、`-f` でコマンドライン全体
/// （配置名に付くターゲットトリプル接尾辞を含む）に一致させる。
#[cfg(target_os = "macos")]
fn count_running_sidecars() -> usize {
    let output = Command::new("pgrep")
        .arg("-P")
        .arg(std::process::id().to_string())
        .arg("-f")
        .arg(SidecarKind::Smoke.as_str())
        .output()
        .expect("pgrep を実行できる");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

/// Windows には `pgrep` が無いため `tasklist` を使い、イメージ名が語幹で始まる行を数える。
/// 親子の絞り込みは `tasklist` ではできないが、このファイルのテストは直列化されており、
/// 実行中に他へ補助プロセスを起動しないため同一性の検査として足りる。
#[cfg(windows)]
fn count_running_sidecars() -> usize {
    let output = Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output()
        .expect("tasklist を実行できる");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            line.trim_start_matches('"')
                .starts_with(SidecarKind::Smoke.as_str())
        })
        .count()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn count_running_sidecars() -> usize {
    panic!("この OS では補助プロセスの数を数える経路を持たない（対象は Linux / macOS / Windows）")
}

/// 起動したハンドルが **すべて OS から見える** ようになるまで待ってから、実行中の補助プロセスの
/// 数を返す。
///
/// `Command::spawn` が返った直後でも、子の `exec` が完了するまで `/proc/<pid>/cmdline` は空に
/// なりうる（この環境で実測。`spawn` の戻り直後に数えると 0 に見える）。ハンドルの PID 数
/// （= 起動したはずのプロセス数）に OS のカウントが到達するまで待つことで、数え漏れによる
/// 偽陰性を避ける。**「到達した瞬間」では打ち切らない** — 一回間を置いて再サンプルし、同数で
/// 安定したときだけ確定する。これにより、共有 PID を返しつつ裏で追加のプロセスを起動する
/// ような壊れ方でも、遅れて `exec` した分を数え落とさない。期限を超えたら、その時点の
/// カウントを返して判定に委ねる（呼び出し側の `assert_eq!` が落ちる）。
fn count_settled_sidecars(handles: &[SidecarHandle], context: &str) -> usize {
    let expected_processes = handles
        .iter()
        .map(SidecarHandle::pid)
        .collect::<HashSet<_>>()
        .len();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let count = count_running_sidecars();
        if count >= expected_processes {
            // 安定確認。ここで待つのは、遅れて exec するプロセスを数え落とさないためである。
            thread::sleep(Duration::from_millis(25));
            let settled = count_running_sidecars();
            if settled == count {
                eprintln!(
                    "{context}: OS 上の sidecar-smoke プロセス数 = {settled}（ハンドルの PID 数 {expected_processes}）"
                );
                return settled;
            }
        }
        if Instant::now() >= deadline {
            let count = count_running_sidecars();
            eprintln!(
                "{context}: OS 上の sidecar-smoke プロセス数 = {count}（期限切れ、ハンドルの PID 数 {expected_processes}）"
            );
            return count;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// OS 上で実行中の `sidecar-smoke` の総数を返す（親子関係を問わない）。
///
/// 孫ありの終了保証（tasks.md 3.3 の完了状態）を確かめるには、直接の子だけでなく孫の消滅まで
/// 見る必要がある。**親子関係で絞ってはならない**: 直接の子を終了した時点で孫は再親付けされ、
/// 「テストの子孫」ではなくなるため、ツリーをたどる方式では生存している孫を数え落とす。
/// このファイルのテストは直列化されており、実行中に `sidecar-smoke` を起動するのはテスト自身
/// だけなので、名前で数えても同一性は保たれる。
#[cfg(target_os = "linux")]
fn count_live_sidecars() -> usize {
    let stem = SidecarKind::Smoke.as_str();
    let mut count = 0usize;
    let entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.parse::<u32>().is_err() {
            continue;
        }
        let cmdline = match fs::read(entry.path().join("cmdline")) {
            Ok(cmdline) => cmdline,
            Err(_) => continue,
        };
        // ゾンビは `cmdline` が空なので数えない（= 「実行中」だけを数える）。
        if !cmdline.is_empty() && String::from_utf8_lossy(&cmdline).contains(stem) {
            count += 1;
        }
    }
    count
}

/// macOS には `/proc` が無いため `pgrep -f` でコマンドライン全体に一致させる。`-P` は付けない —
/// 孫は直接の子ではないため、親を限定すると数え落とす。テストは直列化されているので、
/// 同時に補助プロセスを起動する他のテストは無い。
#[cfg(target_os = "macos")]
fn count_live_sidecars() -> usize {
    let output = Command::new("pgrep")
        .arg("-f")
        .arg(SidecarKind::Smoke.as_str())
        .output()
        .expect("pgrep を実行できる");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

/// Windows の `tasklist` はイメージ名で全プロセスを列挙する。親子で絞らない点が、ここで
/// 必要とする数え方そのものである。
#[cfg(windows)]
fn count_live_sidecars() -> usize {
    count_running_sidecars()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn count_live_sidecars() -> usize {
    panic!("この OS では補助プロセスの数を数える経路を持たない（対象は Linux / macOS / Windows）")
}

/// 実行中の `sidecar-smoke` の数が `expected` に安定するまで待ってから返す。孫の起動（0→2）と
/// 終了（2→0）のどちらにも使う。`expected` に到達した瞬間では打ち切らず、一回間を置いて
/// 再サンプルし、同数で安定したときだけ確定する。
fn wait_for_sidecar_count(expected: usize, timeout: Duration, context: &str) -> usize {
    let deadline = Instant::now() + timeout;
    loop {
        let count = count_live_sidecars();
        if count == expected {
            thread::sleep(Duration::from_millis(25));
            let settled = count_live_sidecars();
            if settled == expected {
                eprintln!("{context}: OS 上の実行中 sidecar-smoke 数 = {settled}");
                return settled;
            }
        }
        if Instant::now() >= deadline {
            let count = count_live_sidecars();
            eprintln!(
                "{context}: OS 上の実行中 sidecar-smoke 数 = {count}（期限切れ、期待 {expected}）"
            );
            return count;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

// ---------------------------------------------------------------------------
// 子プロセスの後始末
// ---------------------------------------------------------------------------

/// テストが起動した子プロセスを必ず強制終了して回収するガード。
///
/// `Drop` で走るため、アサーションが失敗して panic した経路でも孤児を残さない。3.3 の
/// 猶予付き終了（プロセスグループ / Job Object）ではなく、テストの後始末に限った直接の
/// 強制終了である。
struct SidecarGuard {
    handle: SidecarHandle,
}

impl SidecarGuard {
    fn new(handle: SidecarHandle) -> Self {
        SidecarGuard { handle }
    }
}

impl Drop for SidecarGuard {
    fn drop(&mut self) {
        let _ = self.handle.kill();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match self.handle.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                // 判定不能。これ以上待たずに戻る（プロセスは既に存在しない）。
                Err(_) => return,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 完了状態: 並行 10 要求でも起動は 1 回
// ---------------------------------------------------------------------------

/// 同じ種類への `ensure` を 10 個のスレッドから同時に呼んでも、起動したプロセスは 1 つである
/// （tasks.md 3.2 の完了状態、要件 5.5）。
///
/// 2 つのことを主張する: (a) 返ったすべてのハンドルが同じ PID を報告する、(b) OS から数えた
/// 実行中の補助プロセス数が 1 である。バリアで全スレッドを同時に走らせて競合を最大化する。
#[test]
fn concurrent_ensure_starts_exactly_one_process() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("concurrent_ensure_starts_exactly_one_process");
        return;
    };

    let supervisor = Supervisor::new();
    let spec = sidecar_spec(&executable);

    const CALLERS: usize = 10;
    let barrier = Arc::new(Barrier::new(CALLERS));
    let mut threads = Vec::with_capacity(CALLERS);
    for _ in 0..CALLERS {
        let supervisor = supervisor.clone();
        let spec = spec.clone();
        let barrier = Arc::clone(&barrier);
        threads.push(thread::spawn(move || {
            barrier.wait();
            supervisor.ensure(&spec)
        }));
    }

    let handles: Vec<SidecarHandle> = threads
        .into_iter()
        .map(|thread| {
            thread
                .join()
                .expect("呼び出しスレッドが panic した")
                .expect("並行要求の ensure が失敗した")
        })
        .collect();
    // 壊れた実装（起動が複数回走る）でも孤児を残さないよう、返ったハンドルを全部ガードする。
    let _guards: Vec<SidecarGuard> = handles.iter().cloned().map(SidecarGuard::new).collect();

    let first_pid = handles[0].pid();
    for handle in &handles {
        assert_eq!(
            handle.pid(),
            first_pid,
            "同じ種類への並行要求が別のプロセスに解決した"
        );
    }

    let count = count_settled_sidecars(&handles, "並行 10 要求後");
    assert_eq!(
        count, 1,
        "同じ種類への起動は 1 回に収まるはずである（OS 上のプロセス数 {count}）"
    );
}

// ---------------------------------------------------------------------------
// 共有: 別の呼び出し元でも同じプロセス
// ---------------------------------------------------------------------------

/// 2 つの「ウィンドウ」に相当する監督ハンドルから要求しても、同じプロセスが返る（要件 5.5）。
#[test]
fn different_callers_share_the_same_process() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("different_callers_share_the_same_process");
        return;
    };

    let supervisor = Supervisor::new();
    let spec = sidecar_spec(&executable);

    // 監督はアプリ全体で 1 実体。clone は同じ実体を指す（ウィンドウごとの複製ではない）。
    let window_a = supervisor.clone();
    let window_b = supervisor.clone();

    let from_a = window_a.ensure(&spec).expect("ウィンドウ A からの起動");
    let _guard_a = SidecarGuard::new(from_a.clone());
    let from_b = window_b.ensure(&spec).expect("ウィンドウ B からの起動");
    let _guard_b = SidecarGuard::new(from_b.clone());

    assert_eq!(
        from_a.pid(),
        from_b.pid(),
        "ウィンドウ間で同じ補助プロセスが共有されていない"
    );
    let via_get = supervisor
        .get(SidecarKind::Smoke)
        .expect("起動中の種類は get が返す");
    assert_eq!(via_get.pid(), from_a.pid(), "get が別のプロセスを報告した");

    let count = count_settled_sidecars(&[from_a.clone()], "共有確認時");
    assert_eq!(
        count, 1,
        "種類ごとに高々 1 つのはずである（OS 上のプロセス数 {count}）"
    );
}

// ---------------------------------------------------------------------------
// 再起動: 予期せぬ終了の次の要求で改めて起動
// ---------------------------------------------------------------------------

/// 予期せず終了した子を `get` は生存として報告せず、登録から外す（要件 5.8 の前半）。
///
/// このテストは `get` の振る舞いを固定する。`get` が死んだ登録を取り除くため、この経路では
/// [`Supervisor::ensure`] 側の「登録はあるが子が死んでいる」腕は通らない — そちらは
/// [`ensure_itself_respawns_a_dead_registered_child`] が、`get` を呼ばずに直接固定する。
#[test]
fn ensure_respawns_after_unexpected_exit() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("ensure_respawns_after_unexpected_exit");
        return;
    };

    let supervisor = Supervisor::new();
    let spec = sidecar_spec(&executable);

    let first = supervisor.ensure(&spec).expect("最初の起動");
    let _first_guard = SidecarGuard::new(first.clone());
    let first_pid = first.pid();

    // 外部から強制終了する（予期せぬ終了の再現）。
    first.kill().expect("子を強制終了できる");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut cleared = false;
    while Instant::now() < deadline {
        if supervisor.get(SidecarKind::Smoke).is_none() {
            cleared = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        cleared,
        "終了した子を get が生存として報告し続けている（ゾンビを回収していない可能性がある）"
    );

    let second = supervisor.ensure(&spec).expect("改めて起動できる");
    let _second_guard = SidecarGuard::new(second.clone());
    assert_ne!(
        second.pid(),
        first_pid,
        "終了後の ensure が新しいプロセスを起動していない"
    );

    // 最初の子は既に回収済みなので、生存しているのは 2 つ目だけである。
    let count = count_settled_sidecars(&[second.clone()], "再起動後");
    assert_eq!(
        count, 1,
        "再起動後も種類ごとに高々 1 つである（OS 上のプロセス数 {count}）"
    );
}

/// `ensure` 自身が、登録に残った死んだ子を検出して起動し直す（要件 5.8 の後半、tasks.md 3.2
/// 「予期せず終了していた場合、次に必要になった時点で改めて起動を試みる」）。
///
/// 死の検出は **`get` を呼ばずに** 子の `try_wait` で行う。`get` を先に呼ぶと `get` が登録から
/// 死んだ子を取り除いてしまい、`ensure` の「登録はあるが死んでいる」腕（`registry.remove` の
/// 後に再起動する分岐）を通らない。このテストが無いと、その腕を「死んだハンドルをそのまま返す」
/// よう壊してもスイート全体が緑のままになる（レビューで実証された）。
#[test]
fn ensure_itself_respawns_a_dead_registered_child() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("ensure_itself_respawns_a_dead_registered_child");
        return;
    };

    let supervisor = Supervisor::new();
    let spec = sidecar_spec(&executable);

    let first = supervisor.ensure(&spec).expect("最初の起動");
    let _first_guard = SidecarGuard::new(first.clone());
    let first_pid = first.pid();

    // 強制終了し、**get を呼ばずに** 終了を待つ（登録には死んだ子が残ったままになる）。
    first.kill().expect("子を強制終了できる");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match first.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => panic!("強制終了した子が期限内に終了しない"),
            Err(error) => panic!("子の終了状態を取得できない: {error}"),
        }
    }

    // 登録に死んだ子が残った状態で ensure を呼ぶ。改めて起動すること（新しい PID）を固定する。
    let second = supervisor.ensure(&spec).expect("改めて起動できる");
    let _second_guard = SidecarGuard::new(second.clone());
    assert_ne!(
        second.pid(),
        first_pid,
        "ensure が死んだ登録をそのまま返した（起動し直していない）"
    );

    let count = count_settled_sidecars(&[second.clone()], "ensure による再起動後");
    assert_eq!(
        count, 1,
        "ensure の再起動後も種類ごとに高々 1 つである（OS 上のプロセス数 {count}）"
    );
}

// ---------------------------------------------------------------------------
// 起動失敗の区別（要件 5.4）
// ---------------------------------------------------------------------------

/// 実行ファイルが存在しないときは `NotFound`（他の 3 つと区別できる）。
#[test]
fn missing_executable_is_reported_as_not_found() {
    let _serial = serialize();
    let dir = TempDir::new("not-found");
    let supervisor = Supervisor::new();
    let spec = sidecar_spec(&dir.path().join("sidecar-smoke-does-not-exist"));

    let error = supervisor
        .ensure(&spec)
        .expect_err("存在しない実行ファイルで成功してはならない");
    assert!(
        matches!(error, SpawnError::NotFound { .. }),
        "NotFound を期待したが {error:?} だった"
    );
    assert_eq!(count_running_sidecars(), 0, "起動していないはずである");
}

/// 実行権限がないファイルは Unix では `NotExecutable`。Windows に実行ビットは存在しないため、
/// 存在するファイルは前段を通過し、整合性検査の失敗（`IntegrityMismatch`）として返る
/// （実行可否は最終的に `CreateProcess` が決める）。
#[test]
fn non_executable_file_is_reported_distinctly() {
    let _serial = serialize();
    let dir = TempDir::new("not-executable");
    let path = dir.write("sidecar-smoke", b"#!/bin/sh\nexit 0\n");
    #[cfg(unix)]
    make_executable(&path, 0o644);

    let supervisor = Supervisor::new();
    let spec = sidecar_spec(&path);
    let error = supervisor
        .ensure(&spec)
        .expect_err("実行できないファイルで成功してはならない");

    #[cfg(unix)]
    assert!(
        matches!(error, SpawnError::NotExecutable { .. }),
        "NotExecutable を期待したが {error:?} だった"
    );
    #[cfg(windows)]
    assert!(
        matches!(error, SpawnError::IntegrityMismatch { .. }),
        "Windows では現行の検査では IntegrityMismatch になるはずである: {error:?}"
    );
    assert_eq!(count_running_sidecars(), 0, "起動していないはずである");
}

/// 原本を 1 バイト改変した複製は、起動の前に `IntegrityMismatch` として検出される（要件 5.3）。
/// 原本が未配置のクローンではスキップする（CI は配置済みのため実経路が走る）。
#[test]
fn modified_original_is_reported_as_integrity_mismatch() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("modified_original_is_reported_as_integrity_mismatch");
        return;
    };

    let original = fs::read(&executable).expect("ステージング済み原本を読み取れる");
    let mut modified = original.clone();
    modified[0] ^= 0x01;

    let dir = TempDir::new("integrity-mismatch");
    let path = dir.write("sidecar-smoke", &modified);
    #[cfg(unix)]
    make_executable(&path, 0o755);

    let supervisor = Supervisor::new();
    let spec = sidecar_spec(&path);
    let error = supervisor
        .ensure(&spec)
        .expect_err("改変された実行ファイルで成功してはならない");

    match error {
        SpawnError::IntegrityMismatch { source, .. } => {
            assert!(
                matches!(source, IntegrityError::Mismatch { .. }),
                "改変の原因は Mismatch のはずである: {source:?}"
            );
        }
        other => panic!("IntegrityMismatch を期待したが {other:?} だった"),
    }
    assert_eq!(count_running_sidecars(), 0, "起動していないはずである");
}

/// `IntegrityError::Unregistered`（期待ダイジェストが埋め込まれていない）も黙って通さず、
/// 起動失敗の 1 つとして報告される（要件 5.3、tasks.md 1.7 / 3.1 の申し送り）。原本の有無に
/// 依存せず決定的に観測できるよう、整合性検査だけを差し替えてこの結果を返す。
#[test]
fn unregistered_digest_is_reported_through_the_spawn_error() {
    let _serial = serialize();
    let dir = TempDir::new("unregistered");
    let path = dir.write("sidecar-smoke", b"#!/bin/sh\nexit 0\n");
    #[cfg(unix)]
    make_executable(&path, 0o755);

    let supervisor = Supervisor::with_verifier(Arc::new(
        |kind: SidecarKind, path: &Path| -> Result<(), IntegrityError> {
            Err(IntegrityError::Unregistered {
                kind,
                path: path.to_path_buf(),
            })
        },
    ));
    let spec = sidecar_spec(&path);
    let error = supervisor
        .ensure(&spec)
        .expect_err("期待ダイジェストが無いのに成功してはならない");

    let text = error.to_string();
    assert!(
        text.contains("整合性検査に失敗した"),
        "報告に起動中止の理由が含まれない: {text}"
    );
    assert!(
        text.contains("登録されていない"),
        "報告に「期待値が無い」事実が含まれない（沈黙して通している）: {text}"
    );
    match error {
        SpawnError::IntegrityMismatch { source, .. } => {
            assert!(
                matches!(source, IntegrityError::Unregistered { .. }),
                "Unregistered を保持していない: {source:?}"
            );
        }
        other => panic!("IntegrityMismatch を期待したが {other:?} だった"),
    }
    assert_eq!(count_running_sidecars(), 0, "起動していないはずである");
}

/// 起動そのものの失敗は `Spawn`（他の 3 つと区別できる）。
///
/// 実在し、実行権限もあり、`exec` が失敗するファイルを渡す。整合性検査は本物だとこの一時ファイルを
/// 通せない（登録が無い / 内容が違う）ため、検査だけを差し替える seam を使う。本番の
/// [`Supervisor::new`] は差し替えない。
#[test]
fn failed_spawn_is_reported_distinctly() {
    let _serial = serialize();
    let dir = TempDir::new("spawn-failure");

    // Unix: 存在しないインタプリタを指す shebang。`execve` が ENOENT を返し、
    // ENOEXEC のときだけ起きる `/bin/sh` へのフォールバックは起きない。
    #[cfg(unix)]
    let path = {
        let path = dir.write(
            "sidecar-smoke-bad",
            b"#!/nonexistent/jxcel-sidecar-spawn-test-interpreter\n",
        );
        make_executable(&path, 0o755);
        path
    };
    // Windows: 有効な PE でない内容の実行ファイル。`CreateProcess` が拒否する。
    #[cfg(windows)]
    let path = dir.write("sidecar-smoke-bad.exe", b"\x00 not a valid PE image \x00");

    let supervisor = Supervisor::with_verifier(Arc::new(|_, _| Ok(())));
    let spec = sidecar_spec(&path);
    let error = supervisor
        .ensure(&spec)
        .expect_err("起動できないファイルで成功してはならない");

    assert!(
        matches!(error, SpawnError::Spawn { .. }),
        "Spawn を期待したが {error:?} だった"
    );
    assert_eq!(count_running_sidecars(), 0, "起動していないはずである");
}

// ---------------------------------------------------------------------------
// 終了保証: 孫を含めて残さない（要件 5.6、tasks.md 3.3 の完了状態）
// ---------------------------------------------------------------------------

/// 子がさらに孫を起動している状態で `shutdown_all` を呼んでも、**孫を含めて**残らない
/// （tasks.md 3.3 の完了状態）。API の戻り値ではなく OS から見たプロセス数で確認する。
///
/// Unix はプロセスグループ宛の終了、Windows は Job Object の終了が孫に届くことを、
/// 実プロセスで固定する。
#[test]
fn shutdown_all_reaps_child_and_grandchild() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("shutdown_all_reaps_child_and_grandchild");
        return;
    };

    let supervisor = Supervisor::new().with_grace(Duration::from_millis(200));
    let handle = supervisor
        .ensure(&grandchild_spec(&executable))
        .expect("子と孫を起動できる");
    let _guard = SidecarGuard::new(handle.clone());

    // 子（1）と孫（1）の両方が OS から見えるまで待つ。ここで 2 に到達しないなら、そもそも
    // 孫が起動していないので、以降の主張は意味を持たない。
    let before = wait_for_sidecar_count(2, Duration::from_secs(10), "終了処理の前");
    assert_eq!(
        before, 2,
        "子と孫が起動しているはずである（OS 上の子孫数 {before}）"
    );

    supervisor.shutdown_all().expect("子も孫も残さず終了できる");

    // API を信じず、OS から見て子孫が消えたことを確かめる。
    let after = wait_for_sidecar_count(0, Duration::from_secs(10), "終了処理の後");
    assert_eq!(
        after, 0,
        "終了処理の後に子または孫が残っている（OS 上の子孫数 {after}）"
    );
    assert!(
        supervisor.get(SidecarKind::Smoke).is_none(),
        "終了処理の後に登録簿が空になっていない"
    );
}

/// 直接の子だけを終了しても孫は残る — したがってグループ / ジョブ宛の終了が**必要**である。
///
/// これは終了保証の負荷証明を恒久的な回帰テストにしたものである。[`SidecarHandle::kill`] は
/// 直接の子にしか届かないため、孫は生存し続ける。グループ宛の `shutdown_all` が孫を終わらせる。
#[test]
fn direct_kill_leaves_the_grandchild_until_group_termination() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("direct_kill_leaves_the_grandchild_until_group_termination");
        return;
    };

    let supervisor = Supervisor::new().with_grace(Duration::from_millis(200));
    let handle = supervisor
        .ensure(&grandchild_spec(&executable))
        .expect("子と孫を起動できる");
    let _guard = SidecarGuard::new(handle.clone());

    let before = wait_for_sidecar_count(2, Duration::from_secs(10), "直接終了の前");
    assert_eq!(
        before, 2,
        "子と孫が起動しているはずである（OS 上の子孫数 {before}）"
    );

    // 直接の子だけを強制終了する（グループ / ジョブ宛ではない）。
    handle.kill().expect("直接の子を強制終了できる");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match handle.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => panic!("強制終了した直接の子が期限内に終了しない"),
            Err(error) => panic!("直接の子の終了状態を取得できない: {error}"),
        }
    }

    let after_direct =
        wait_for_sidecar_count(1, Duration::from_secs(5), "直接の子だけを終了した後");
    assert_eq!(
        after_direct, 1,
        "孫が直接の子の終了に巻き込まれてはならない（グループ / ジョブ宛の終了が必要である証拠。OS 上の子孫数 {after_direct}）"
    );

    // グループ / ジョブ宛の終了は、直接の子が既に死んでいても孫まで届く。
    supervisor.shutdown_all().expect("登録簿から孫を終了できる");
    let after_group = wait_for_sidecar_count(0, Duration::from_secs(10), "グループ宛の終了の後");
    assert_eq!(
        after_group, 0,
        "グループ / ジョブ宛の終了でも孫が残った（OS 上の子孫数 {after_group}）"
    );
}

/// 猶予段が先に走り、猶予信号を無視する子は強制段で終了する（tasks.md 3.3 の猶予→強制の段階）。
///
/// 補助プロセスは `--ignore-term`（検証専用）で猶予信号を無視する。`shutdown_all` は猶予時間を
/// 待ってから強制段へ移るため、経過時間は猶予時間以上になる。実装が猶予段を飛ばして即座に
/// 強制していたら経過時間が猶予を下回ってこのテストが落ち、猶予段だけ送って終わらせていれば
/// 子が生存したままになって落ちる。
#[test]
fn shutdown_all_graceful_stage_precedes_forced_kill() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("shutdown_all_graceful_stage_precedes_forced_kill");
        return;
    };

    let grace = Duration::from_millis(300);
    let supervisor = Supervisor::new().with_grace(grace);

    let mut spec = sidecar_spec(&executable);
    spec.args.push("--ignore-term".to_string());
    let handle = supervisor.ensure(&spec).expect("起動できる");
    let _guard = SidecarGuard::new(handle.clone());

    let started_count = count_settled_sidecars(&[handle.clone()], "猶予段の検証の前");
    assert_eq!(
        started_count, 1,
        "猶予信号を無視する子が起動しているはずである（OS 上のプロセス数 {started_count}）"
    );

    let started = Instant::now();
    supervisor
        .shutdown_all()
        .expect("猶予信号を無視する子も強制段で終了できる");
    let elapsed = started.elapsed();

    assert!(
        elapsed >= grace,
        "猶予段を経ずに強制終了した（経過 {elapsed:?} < 猶予 {grace:?}）"
    );
    let after = wait_for_sidecar_count(0, Duration::from_secs(10), "強制段の後");
    assert_eq!(
        after, 0,
        "猶予信号を無視した子が強制段で終了していない（OS 上のプロセス数 {after}）"
    );
    match handle.try_wait() {
        Ok(Some(_)) => {}
        other => panic!("子が回収されていない: {other:?}"),
    }
}

/// `shutdown_all` は冪等であり、登録簿を空に保つ（tasks.md 3.3「2 回呼んでも安全」）。
///
/// 2 回目は登録簿が空であるため、猶予時間を待たずに即座に成功する。起動が 1 度も無い監督でも
/// 成功する。
#[test]
fn shutdown_all_is_idempotent_and_clears_the_registry() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("shutdown_all_is_idempotent_and_clears_the_registry");
        return;
    };

    let supervisor = Supervisor::new().with_grace(Duration::from_millis(100));
    let handle = supervisor
        .ensure(&sidecar_spec(&executable))
        .expect("起動できる");
    let _guard = SidecarGuard::new(handle.clone());

    supervisor.shutdown_all().expect("1 回目の終了処理");
    assert!(
        supervisor.get(SidecarKind::Smoke).is_none(),
        "1 回目の終了処理の後に登録簿が空になっていない"
    );

    let started = Instant::now();
    supervisor.shutdown_all().expect("2 回目の終了処理");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "2 回目の終了処理が待たされた（冪等でない）"
    );

    // 起動が一度も無い監督でも成功する。
    let empty = Supervisor::new().with_grace(Duration::from_millis(100));
    empty.shutdown_all().expect("起動が無くても成功する");

    assert_eq!(count_running_sidecars(), 0, "起動していないはずである");
}

/// 子が既に終了しているときに `shutdown_all` を呼んでもハングしない（tasks.md 3.3
/// 「既に終了した子でハングしない」）。
///
/// 5 秒の猶予を与えておく。実装が猶予を無条件に消費するならこのテストが落ちる。
#[test]
fn shutdown_all_returns_promptly_when_the_child_is_already_dead() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("shutdown_all_returns_promptly_when_the_child_is_already_dead");
        return;
    };

    let grace = Duration::from_secs(5);
    let supervisor = Supervisor::new().with_grace(grace);
    let handle = supervisor
        .ensure(&sidecar_spec(&executable))
        .expect("起動できる");
    let _guard = SidecarGuard::new(handle.clone());

    handle.kill().expect("子を強制終了できる");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match handle.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => panic!("強制終了した子が期限内に終了しない"),
            Err(error) => panic!("子の終了状態を取得できない: {error}"),
        }
    }

    let started = Instant::now();
    supervisor
        .shutdown_all()
        .expect("既に終了した子でも成功する");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "既に終了した子に猶予 {grace:?} を消費した（ハングしない要件に反する）"
    );
    assert!(
        supervisor.get(SidecarKind::Smoke).is_none(),
        "終了処理の後に登録簿が空になっていない"
    );
}

// ---------------------------------------------------------------------------
// 出力の取得と予期せぬ終了の通知（要件 5.7、5.9、tasks.md 3.4）
// ---------------------------------------------------------------------------

/// 出来事を期限付きで 1 つ受け取る。期限切れは panic する（無限に待たない）。
fn recv_event(receiver: &Receiver<SidecarEvent>, timeout: Duration, context: &str) -> SidecarEvent {
    receiver
        .recv_timeout(timeout)
        .unwrap_or_else(|error| panic!("{context}: 出来事を期限内に受け取れない: {error}"))
}

/// 失敗報告に載せる、受け取った出来事の末尾の並び。件数が多くても報告が肥大しないよう
/// 直近の数件だけを保持する。
fn push_tail(tail: &mut Vec<String>, entry: String) {
    const TAIL: usize = 8;
    if tail.len() == TAIL {
        tail.remove(0);
    }
    tail.push(entry);
}

/// 予期せぬ終了の事実が購読側へ届き、監督側が巻き込まれて終了しない（要件 5.7、tasks.md 3.4 の
/// 完了状態）。
///
/// 外部から子の識別子へ `SIGKILL` を送る（[`SidecarHandle::kill`] は子の識別子への `SIGKILL`
/// であり、監督側の終了処理を経由しない）。その後、(a) 当該種類の `Exited` が届くこと、
/// (b) 監督が生存し続け、`get` が終了した子を生存として報告せず、`ensure` が新しい子を起動
/// できること、(c) 再起動後も出力の購読が機能すること（読み取り機構が子の死で壊れていない）
/// を確かめる。(c) が無いと、読み取りスレッドが子の終了で panic して黙って止まっていても
/// 気づけない。
#[test]
fn unexpected_exit_is_notified_and_the_supervisor_survives() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("unexpected_exit_is_notified_and_the_supervisor_survives");
        return;
    };

    let supervisor = Supervisor::new();
    let events = supervisor.subscribe();
    let spec = sidecar_spec(&executable);

    let handle = supervisor.ensure(&spec).expect("起動できる");
    let first_pid = handle.pid();
    let _guard = SidecarGuard::new(handle.clone());

    // 起動行が届く（読み取りが張られていることの確認を兼ねる）。
    let started = recv_event(&events, Duration::from_secs(10), "起動行");
    assert!(
        matches!(
            &started,
            SidecarEvent::Output { kind: SidecarKind::Smoke, stream: SidecarStream::Stdout, line }
                if line.starts_with("sidecar-smoke ready")
        ),
        "標準出力の起動行が届いていない: {started:?}"
    );

    // 外部からの強制終了。監督側の終了処理は通さない。
    handle.kill().expect("子を強制終了できる");

    // (a) 予期せぬ終了の通知が届く。監督が巻き込まれて終了していればここへ到達しない。
    let status = loop {
        match recv_event(&events, Duration::from_secs(10), "終了通知") {
            SidecarEvent::Exited { kind, status } => {
                assert_eq!(kind, SidecarKind::Smoke, "終了通知の種類が違う");
                break status;
            }
            SidecarEvent::Output { .. } => {}
        }
    };
    assert!(
        !status.is_deliberate(),
        "外部からの強制終了を意図的な終了として報告した: {status:?}"
    );

    // (b) 監督は生存し、終了した子を生存として報告しない。
    let deadline = Instant::now() + Duration::from_secs(5);
    while supervisor.get(SidecarKind::Smoke).is_some() {
        assert!(
            Instant::now() < deadline,
            "終了した子を get が生存として報告し続けている"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let second = supervisor
        .ensure(&spec)
        .expect("強制終了の後も改めて起動できる");
    let _second_guard = SidecarGuard::new(second.clone());
    assert_ne!(
        second.pid(),
        first_pid,
        "終了後の ensure が新しいプロセスを起動していない"
    );

    // (c) 再起動後も出力の購読が機能する。
    let mut stdin = second.take_stdin().expect("標準入力を取得できる");
    writeln!(stdin, "after-restart").expect("標準入力へ書ける");
    stdin.flush().expect("flush できる");
    let echoed = loop {
        match recv_event(&events, Duration::from_secs(10), "再起動後の出力") {
            SidecarEvent::Output {
                stream: SidecarStream::Stdout,
                line,
                ..
            } if line == "echo: after-restart" => break line,
            _ => {}
        }
    };
    assert_eq!(echoed, "echo: after-restart");
}

/// 出力を行単位で取得し、終了通知が**その子の最後の出力の後**に届く（要件 5.9、tasks.md 3.4
/// 「終了の通知は、そのプロセスの最後の出力の後に届く」）。
///
/// 読み取り側の上限（64 KiB）を大きく超える 1 行を子へ送る。子はその応答（同じ長さの 1 行）を
/// 書き切る最中に標準出力のパイプの前で止まるため、**強制終了の時点で未読の行がパイプに残る**。
/// 読み取り側を join してから通知を出す実装では、残った断片が届いた後に通知が出る。join を
/// 怠る実装では通知が先に出て、`Exited` の後に断片が現れる。応答の断片が届き始めたことを
/// 観測してから強制終了するので、固定の待ちに頼らない。
#[test]
fn every_output_line_is_delivered_before_the_exit_event() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("every_output_line_is_delivered_before_the_exit_event");
        return;
    };

    let supervisor = Supervisor::new();
    let events = supervisor.subscribe();

    // 読み続けない購読を多数登録する（配布は購読者ごとに複製を作るので読み取り側が遅くなる）。
    // これにより子は標準出力へ書き切れずにパイプの前で止まり、**強制終了の時点で未読の断片が
    // 残る状態**を確実に作れる。読み続けない購読がいても、他の購読への配布と順序は影響を
    // 受けないことの確認も兼ねる（送信路は無限容量で、`send` はブロックしない）。
    const SLOW_SUBSCRIBERS: usize = 32;
    let _slow: Vec<Receiver<SidecarEvent>> = (0..SLOW_SUBSCRIBERS)
        .map(|_| supervisor.subscribe())
        .collect();

    let handle = supervisor
        .ensure(&sidecar_spec(&executable))
        .expect("起動できる");
    let _guard = SidecarGuard::new(handle.clone());

    // 起動行を受け取る（読み取りが張られていることを確認する）。
    let ready = recv_event(&events, Duration::from_secs(10), "起動行");
    assert!(
        matches!(
            &ready,
            SidecarEvent::Output {
                stream: SidecarStream::Stdout,
                ..
            }
        ),
        "起動行が標準出力として届いていない: {ready:?}"
    );

    // 上限を大きく超える 1 行を送る。子が応答を書き切る最中に強制終了するので、パイプには
    // 読み取り側が未消費の断片が残る。
    const HUGE: usize = 8 * 1024 * 1024;
    let mut stdin = handle.take_stdin().expect("標準入力を取得できる");
    let block = vec![b'x'; 64 * 1024];
    for _ in 0..(HUGE / block.len()) {
        stdin.write_all(&block).expect("標準入力へ書ける");
    }
    stdin.write_all(b"\n").expect("行末を書ける");
    stdin.flush().expect("flush できる");

    // 応答の断片が届き始めてから強制終了する（子がまだ書き切っていないことを観測してから
    // 終了させる）。
    for index in 0..4 {
        let event = recv_event(&events, Duration::from_secs(20), "応答の断片");
        assert!(
            matches!(
                &event,
                SidecarEvent::Output {
                    stream: SidecarStream::Stdout,
                    ..
                }
            ),
            "{index} 番目の応答の断片が標準出力として届いていない: {event:?}"
        );
    }
    handle.kill().expect("子を強制終了できる");

    let mut outputs_before_exit = 0usize;
    let mut tail: Vec<String> = Vec::new();
    let status = loop {
        match recv_event(&events, Duration::from_secs(20), "終了通知まで") {
            SidecarEvent::Output { kind, stream, line } => {
                outputs_before_exit += 1;
                push_tail(
                    &mut tail,
                    format!("Output({kind}/{stream:?}) {} バイト", line.len()),
                );
            }
            SidecarEvent::Exited { status, .. } => {
                push_tail(&mut tail, format!("Exited({status:?})"));
                break status;
            }
        }
    };

    // 通知の後に届く出力を拾う（join を怠る実装ではここで現れる）。
    let mut outputs_after_exit = 0usize;
    let drain_deadline = Instant::now() + Duration::from_millis(250);
    loop {
        match events.recv_timeout(Duration::from_millis(50)) {
            Ok(SidecarEvent::Output { line, .. }) => {
                outputs_after_exit += 1;
                push_tail(
                    &mut tail,
                    format!("Output(after exit) {} バイト", line.len()),
                );
            }
            Ok(SidecarEvent::Exited { .. }) => {}
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) if Instant::now() >= drain_deadline => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }

    // 主張は順序だけである。**終了通知の後に出力が届いてはならない。** 読み取り側を join
    // しない実装では、強制終了の時点でパイプに残っていた断片がここで現れる。
    assert_eq!(
        outputs_after_exit, 0,
        "終了通知の後に出力が届いた（順序が守られていない）。通知の前 {outputs_before_exit} 件、\
         後 {outputs_after_exit} 件。末尾の並び: {tail:?}"
    );
    eprintln!(
        "順序: 出力 {outputs_before_exit} 件 → Exited({status:?})。通知後の出力 {outputs_after_exit} 件。\
         末尾の並び: {tail:?}"
    );
}

/// 内容 `length` バイト（`x` の連続）+ 終端からなる入力を作る。
#[cfg(unix)]
fn x_line(length: usize, terminator: &[u8]) -> Vec<u8> {
    let mut line = vec![b'x'; length];
    line.extend_from_slice(terminator);
    line
}

/// 実パイプ越しに、読み取りの上限の前後で CRLF が分かれても `\r` が漏れず・落ちず、余分な
/// 空の出力も出ないことを確かめる（レビューで実測された回帰の固定）。
///
/// `sidecar-smoke` の応答は LF 終端なので、入力をそのまま流す `/bin/cat` を相手役に使う
/// （整合性検査は差し替える。ここで確かめるのは機構であり配布物の検証ではない）。送る内容は
/// 上限の前後の `\r\n`、**内容の `\r` が行末の `\r\n` と連続する場合**、CRLF のみの空行である。
#[cfg(unix)]
#[test]
fn crlf_at_the_pipe_cap_is_trimmed_end_to_end() {
    let _serial = serialize();

    let cat = Path::new("/bin/cat");
    if !cat.is_file() {
        eprintln!(
            "crlf_at_the_pipe_cap_is_trimmed_end_to_end をスキップします: {} が無い",
            cat.display()
        );
        return;
    }

    let supervisor = Supervisor::with_verifier(Arc::new(|_, _| Ok(())));
    let events = supervisor.subscribe();
    let mut spec = sidecar_spec(cat);
    spec.args.clear();
    let handle = supervisor.ensure(&spec).expect("起動できる");
    let _guard = SidecarGuard::new(handle.clone());
    let mut stdin = handle.take_stdin().expect("標準入力を取得できる");

    // 読み取りの上限（`MAX_LINE_BYTES` = 64 KiB。supervisor の私有定数なので値を直接書く）。
    let cap = 64 * 1024;
    // 内容の `\r` と行末の `\r\n` が連続する入力（`\r\r\n` の 1 バイト目は内容である）。
    let mut content_cr = vec![b'x'; cap - 1];
    content_cr.push(b'\r');
    let inputs: Vec<Vec<u8>> = vec![
        x_line(cap - 1, b"\r\n"),
        x_line(cap, b"\r\n"),
        x_line(cap + 1, b"\r\n"),
        x_line(cap - 1, b"\r\r\n"),
        b"\r\n".to_vec(),
    ];
    for input in &inputs {
        stdin.write_all(input).expect("標準入力へ書ける");
    }
    stdin.flush().expect("flush できる");
    // 標準入力を閉じて cat を終了させる（読み取りは EOF で終わる）。
    drop(stdin);

    let mut observed: Vec<Vec<u8>> = Vec::new();
    loop {
        match recv_event(&events, Duration::from_secs(20), "断片と終了通知") {
            SidecarEvent::Output { line, .. } => observed.push(line.into_bytes()),
            SidecarEvent::Exited { .. } => break,
        }
    }

    // 上限未満・ちょうど・超過（上限 + 1 バイト）・内容 `\r` を含む上限ちょうど・空行。
    let lengths: Vec<usize> = observed.iter().map(Vec::len).collect();
    assert_eq!(
        lengths,
        vec![cap - 1, cap, cap, 1, cap, 0],
        "CRLF の境界で断片の分かれ方が違う（観測 {lengths:?}、観測内容 {observed:?}）"
    );
    // 内容の `\r` は残り、行末の `\r` は落ちる。空行は空の断片として届く（正当である）。
    assert_eq!(
        observed[4], content_cr,
        "内容の `\\r` が行末の `\\r` と一緒に落ちている"
    );
    for (index, fragment) in observed.iter().enumerate() {
        if index != 4 {
            assert!(
                !fragment.contains(&b'\r'),
                "{index} 番目の断片に行末の `\\r` が漏れている（{} バイト）",
                fragment.len()
            );
        }
    }
}

/// 標準出力と標準エラーを、どちらの流れかが分かる形で取得する（要件 5.9）。
///
/// 正常な子の起動行は標準出力へ、引数が不正なときの使い方は標準エラーへ出る（1.6 の仕様）。
/// どちらの経路も検証用の補助プロセスを変更せずに到達できる。
#[test]
fn stdout_and_stderr_are_captured_with_distinct_streams() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("stdout_and_stderr_are_captured_with_distinct_streams");
        return;
    };

    let supervisor = Supervisor::new();
    let events = supervisor.subscribe();

    // 1. 正常な子: 起動行は標準出力。
    let handle = supervisor
        .ensure(&sidecar_spec(&executable))
        .expect("起動できる");
    let _guard = SidecarGuard::new(handle.clone());
    let ready = recv_event(&events, Duration::from_secs(10), "標準出力の起動行");
    match &ready {
        SidecarEvent::Output {
            stream: SidecarStream::Stdout,
            line,
            ..
        } => assert!(
            line.starts_with("sidecar-smoke ready"),
            "起動行の内容が違う: {line:?}"
        ),
        other => panic!("起動行が標準出力として届いていない: {other:?}"),
    }

    handle.kill().expect("子を強制終了できる");
    loop {
        if let SidecarEvent::Exited { .. } =
            recv_event(&events, Duration::from_secs(10), "終了通知")
        {
            break;
        }
    }

    // 2. 引数が不正な子: 使い方は標準エラーへ出て、終了コード 2 で終わる（1.6 の仕様）。
    let mut invalid = sidecar_spec(&executable);
    invalid.args = vec!["--not-a-real-flag".to_string()];
    let failing = supervisor.ensure(&invalid).expect("起動自体は成功する");
    let _failing_guard = SidecarGuard::new(failing.clone());

    let mut saw_stderr = false;
    let status = loop {
        match recv_event(&events, Duration::from_secs(10), "使い方と終了通知") {
            SidecarEvent::Output {
                stream: SidecarStream::Stderr,
                line,
                ..
            } => {
                assert!(
                    !line.trim().is_empty(),
                    "標準エラーの空行は取得対象にならない"
                );
                if line.contains("使い方") {
                    saw_stderr = true;
                }
            }
            SidecarEvent::Output {
                stream: SidecarStream::Stdout,
                ..
            } => {}
            SidecarEvent::Exited { status, .. } => break status,
        }
    };
    assert!(
        saw_stderr,
        "引数が不正なときの使い方が標準エラーとして取得できていない"
    );
    assert_eq!(
        status.status().and_then(|status| status.code()),
        Some(2),
        "使い方の終了コードは 2 である: {status:?}"
    );
}

/// 2 つの独立した購読のそれぞれに出来事が届く（tasks.md 3.4「複数の購読側」）。
#[test]
fn two_independent_subscribers_both_receive_the_events() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("two_independent_subscribers_both_receive_the_events");
        return;
    };

    let supervisor = Supervisor::new();
    let first = supervisor.subscribe();
    let second = supervisor.subscribe();
    let handle = supervisor
        .ensure(&sidecar_spec(&executable))
        .expect("起動できる");
    let _guard = SidecarGuard::new(handle.clone());
    let mut stdin = handle.take_stdin().expect("標準入力を取得できる");

    writeln!(stdin, "hello").expect("標準入力へ書ける");
    stdin.flush().expect("flush できる");

    // 両方の購読が同じ出来事を受け取るまで待ってから強制終了する（応答が届く前に終了すると
    // 何を主張しているのか分からなくなる）。
    for (name, events) in [("1 人目", &first), ("2 人目", &second)] {
        let mut saw_ready = false;
        let mut saw_echo = false;
        while !(saw_ready && saw_echo) {
            match recv_event(events, Duration::from_secs(10), name) {
                SidecarEvent::Output {
                    stream: SidecarStream::Stdout,
                    line,
                    ..
                } => {
                    if line.starts_with("sidecar-smoke ready") {
                        saw_ready = true;
                    }
                    if line == "echo: hello" {
                        saw_echo = true;
                    }
                }
                _ => {}
            }
        }
    }

    handle.kill().expect("子を強制終了できる");

    for (name, events) in [("1 人目", &first), ("2 人目", &second)] {
        let status = loop {
            match recv_event(events, Duration::from_secs(10), name) {
                SidecarEvent::Exited { status, .. } => break status,
                SidecarEvent::Output { .. } => {}
            }
        };
        assert!(
            !status.is_deliberate(),
            "{name} が強制終了を意図的な終了として受け取った: {status:?}"
        );
    }
}

/// `shutdown_all`（要件 5.6）による意図的な終了は、予期せぬ終了（要件 5.7）と区別できる形で
/// 通知される。
///
/// 決定: `shutdown_all` も `Exited` を出す。区別は `status` の値（`SidecarExit`）が担うため、
/// アプリケーション自身の終了処理が「予期せぬクラッシュ」として解釈されることはない。逆に
/// 終了の由来を通知しないと、利用側は「通知が無い = 生存している」と解釈せざるを得ず、
/// 子が消えたことを知る手段が無くなる。
#[test]
fn deliberate_shutdown_is_distinguishable_from_an_unexpected_exit() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("deliberate_shutdown_is_distinguishable_from_an_unexpected_exit");
        return;
    };

    let supervisor = Supervisor::new().with_grace(Duration::from_millis(200));
    let events = supervisor.subscribe();
    let handle = supervisor
        .ensure(&sidecar_spec(&executable))
        .expect("起動できる");
    let _guard = SidecarGuard::new(handle.clone());

    supervisor.shutdown_all().expect("終了処理が成功する");

    let status = loop {
        match recv_event(&events, Duration::from_secs(10), "終了通知") {
            SidecarEvent::Exited { status, .. } => break status,
            SidecarEvent::Output { .. } => {}
        }
    };
    assert!(
        status.is_deliberate(),
        "shutdown_all による予期せぬ終了として報告された: {status:?}"
    );
    assert!(
        status.status().is_some(),
        "終了状態が取得できていない: {status:?}"
    );
}

// ---------------------------------------------------------------------------
// 残留プロセスの掃除と親監視の結線（要件 5.6、tasks.md 3.5）
// ---------------------------------------------------------------------------

/// 監督を経由せずに補助プロセスを直接起動する。前回の実行が残したプロセスを模す。
///
/// `--idle` は親監視も応答も持たず永遠に待つ（`crates/sidecar-smoke`）。前回の実行が強制終了
/// された場合に残るのは、この形のプロセス（監視を持たない孫、または監視対象が未回収のゾンビで
/// 監視が効かなかった子）である — 1.6 の申し送りのとおり親監視だけでは塞げない経路が実在し、
/// **3.5 の掃除が唯一の backstop** になる。監督の登録簿には載らないため、掃除の照合に掛かる
/// かどうかをこのテストが直接決められる。
fn spawn_leftover(executable: &Path, tag: &str) -> Child {
    Command::new(executable)
        .arg("--idle")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("残存プロセス（{tag}）を起動できる: {error}"))
}

/// テストが監督を経由せずに直接起動したプロセスを、必ず強制終了して回収するガード。
///
/// [`SidecarGuard`] は監督のハンドルを持つ子にしか使えない。直接起動した検証用のプロセス
/// （残存プロセス・別名の複製）はハンドルを持たないため、このガードが `Drop` で回収する。
/// アサーションが失敗して panic した経路でも孤児を残さない（次のテストの数え合わせを壊さない）。
struct LeftoverGuard {
    child: Child,
}

impl LeftoverGuard {
    fn new(child: Child) -> Self {
        LeftoverGuard { child }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// 明示的に回収する（`Drop` より前に終了状態を観測したい場合）。
    fn reap(mut self) -> std::process::ExitStatus {
        self.child.wait().expect("残存プロセスを回収できる")
    }
}

impl Drop for LeftoverGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) if Instant::now() >= deadline => return,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .expect("実行ファイルにファイル名がある")
        .to_string()
}

/// その識別子の実行ファイル名を OS から取得する。`None` は生存していないか取得できない場合。
#[cfg(target_os = "linux")]
fn process_executable_name(pid: u32) -> Option<String> {
    let path = fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

#[cfg(target_os = "macos")]
fn process_executable_name(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .expect("ps を実行できる");
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let name = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

#[cfg(windows)]
fn process_executable_name(pid: u32) -> Option<String> {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .expect("tasklist を実行できる");
    let text = String::from_utf8_lossy(&output.stdout);
    let name = text.lines().next()?.trim_start_matches('"');
    let name = name.split('"').next()?.trim();
    (!name.is_empty() && !name.starts_with("INFO:")).then(|| name.to_string())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn process_executable_name(_pid: u32) -> Option<String> {
    panic!("この OS ではプロセスの実行ファイル名を取得する経路を持たない")
}

/// OS から見て、その識別子のプロセスがまだ実行中か。**ゾンビは実行中と見なさない。**
///
/// ゾンビを生存と扱うと、終了させたのに生存と観測して猶予時間を無駄に消費する。残留の掃除は
/// 「実行中のプロセス」を対象にするので、ここでも同じ意味に揃える。
#[cfg(target_os = "linux")]
fn process_is_alive(pid: u32) -> bool {
    let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    // `pid (comm) state ...` — comm は空白や括弧を含みうるので、最後の `)` の次を見る。
    let Some((_, rest)) = stat.rsplit_once(')') else {
        return false;
    };
    !matches!(
        rest.trim_start().chars().next(),
        None | Some('Z') | Some('X')
    )
}

#[cfg(target_os = "macos")]
fn process_is_alive(pid: u32) -> bool {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat="])
        .output()
        .expect("ps を実行できる");
    if !output.status.success() {
        return false;
    }
    match String::from_utf8_lossy(&output.stdout)
        .trim()
        .chars()
        .next()
    {
        Some(state) => !matches!(state, 'Z' | 'X'),
        None => false,
    }
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .expect("tasklist を実行できる");
    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn process_is_alive(_pid: u32) -> bool {
    panic!("この OS ではプロセスの生存を確認する経路を持たない")
}

/// その識別子のプロセスの実行ファイル名が期待どおりになるまで待つ（`exec` 完了の観測）。
fn wait_for_executable_name(pid: u32, name: &str, timeout: Duration, context: &str) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if process_executable_name(pid).as_deref() == Some(name) {
            return true;
        }
        if Instant::now() >= deadline {
            eprintln!("{context}: 実行ファイル名が {name} にならない（pid {pid}）");
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// その識別子のプロセスが消えるまで待つ。掃除の効果は非同期なので、固定の待ちではなく観測で待つ。
fn wait_for_process_gone(pid: u32, timeout: Duration, context: &str) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !process_is_alive(pid) {
            return true;
        }
        if Instant::now() >= deadline {
            eprintln!("{context}: pid {pid} が消えない");
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// 起動行（`sidecar-smoke ready pid=<自身> parent_pid=<監視対象>`）から監視対象を取り出す。
fn parse_parent_pid(line: &str) -> Option<u32> {
    line.split_whitespace()
        .find_map(|token| token.strip_prefix("parent_pid="))
        .and_then(|value| value.parse().ok())
}

/// 起動時に、前回の実行が残した補助プロセスを掃除する（tasks.md 3.5 の完了状態、要件 5.6）。
///
/// 監督を経由せずに起動したプロセスを「前回の実行（強制終了されて終了処理が走らなかった）が
/// 残したもの」と見立てる。現在の監督の登録簿には無いので、掃除が見つけて終了させなければ
/// ならない。**API の戻り値だけでなく OS から見た不在**まで確かめる（[`process_is_alive`] と
/// 実行中の補助プロセス数）。
#[test]
fn sweep_orphans_terminates_a_leftover_sidecar() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("sweep_orphans_terminates_a_leftover_sidecar");
        return;
    };

    let leftover = LeftoverGuard::new(spawn_leftover(&executable, "leftover"));
    let pid = leftover.pid();
    assert!(
        wait_for_executable_name(
            pid,
            &file_name_of(&executable),
            Duration::from_secs(10),
            "残存プロセスの起動"
        ),
        "残存プロセスが起動しない"
    );
    assert_eq!(
        wait_for_sidecar_count(1, Duration::from_secs(10), "残存プロセスの起動"),
        1,
        "残存プロセスが OS から見えていない"
    );

    let supervisor = Supervisor::new();
    let swept = supervisor.sweep_orphans();

    assert!(
        swept >= 1,
        "残存プロセスを 1 つも掃除できていない（swept={swept}）"
    );
    assert!(
        wait_for_process_gone(pid, Duration::from_secs(10), "掃除の後"),
        "掃除の後も残存プロセスが実行中である"
    );
    assert_eq!(
        wait_for_sidecar_count(0, Duration::from_secs(10), "掃除の後"),
        0,
        "OS 上に残存プロセスが残っている"
    );

    let status = leftover.reap();
    eprintln!("掃除された残存プロセスの終了状態: {status:?}");
}

/// 実行ファイル名が一致しないプロセスは、識別子が対象でも終了させない（design.md
/// 「PID と実行ファイル名の両方で照合する」）。
///
/// 識別子の再利用を模す: 前回の補助プロセスの識別子が、無関係な実行ファイルのプロセスへ
/// 再利用された状況を作る。名前で照合する実装はこれを終了させない。**名前の照合を外す
/// （識別子だけで照合する）とこのテストが落ちることは、実装時に観測して記録してある**
/// （swept=1 になり、別名のプロセスが終了させられた）。
#[test]
fn sweep_orphans_does_not_kill_a_process_with_a_different_executable_name() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("sweep_orphans_does_not_kill_a_process_with_a_different_executable_name");
        return;
    };

    let dir = TempDir::new("different-name");
    #[cfg(unix)]
    let copy = dir.path().join("unrelated-sidecar-process");
    #[cfg(windows)]
    let copy = dir.path().join("unrelated-sidecar-process.exe");
    fs::copy(&executable, &copy).expect("別名へ複製できる");
    #[cfg(unix)]
    make_executable(&copy, 0o755);

    let impostor = LeftoverGuard::new(
        Command::new(&copy)
            .arg("--idle")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("別名のプロセスを起動できる"),
    );
    let pid = impostor.pid();
    assert!(
        wait_for_executable_name(
            pid,
            &file_name_of(&copy),
            Duration::from_secs(10),
            "別名プロセスの起動"
        ),
        "別名のプロセスが起動しない"
    );

    let supervisor = Supervisor::new();
    let swept = supervisor.sweep_orphans();

    assert!(
        process_is_alive(pid),
        "実行ファイル名が一致しないプロセスを終了させた（swept={swept}、pid {pid}）"
    );
}

/// 掃除は、現在の監督が追跡している子を対象にしない（要件 5.6、tasks.md 3.5）。
#[test]
fn sweep_orphans_does_not_disturb_tracked_children() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("sweep_orphans_does_not_disturb_tracked_children");
        return;
    };

    let supervisor = Supervisor::new();
    let handle = supervisor
        .ensure(&sidecar_spec(&executable))
        .expect("起動できる");
    let _guard = SidecarGuard::new(handle.clone());
    let pid = handle.pid();

    let swept = supervisor.sweep_orphans();

    assert!(
        handle.try_wait().expect("生存判定できる").is_none(),
        "追跡している子を掃除が終了させた（swept={swept}、pid {pid}）"
    );
    assert!(
        supervisor.get(SidecarKind::Smoke).is_some(),
        "追跡している子が登録から消えた"
    );
}

/// 監督が自分の識別子を子へ注入し、親監視（tasks.md 1.6 の `--parent-pid`）を起動する
/// （tasks.md 3.5「異常終了で終了処理が走らない経路に備える」）。
///
/// 呼び出し側が既に `--parent-pid` を渡していても重複させず、監督の識別子で置き換える。
/// 主張は 3 つ: (a) 起動行が報告する監視対象が監督の識別子である、(b) OS から見た
/// コマンドラインにフラグが 1 つだけである、(c) 呼び出し側の値（ここでは 1）が残っていない。
/// **注入を外すとこのテストが落ちることは、実装時に観測して記録してある**（起動行が
/// `parent_pid=1` を報告し、`Some(1)` と監督の識別子が一致しない）。
#[test]
fn supervisor_injects_its_own_pid_into_the_parent_watch() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("supervisor_injects_its_own_pid_into_the_parent_watch");
        return;
    };

    let supervisor = Supervisor::new();
    let events = supervisor.subscribe();
    // 呼び出し側が別の値を渡している。監督はこれを重複させず置き換えなければならない
    // （重複すれば sidecar-smoke は使い方を出して終了コード 2 で終わり、起動行は出ない）。
    let spec = SidecarSpec {
        kind: SidecarKind::Smoke,
        executable: executable.clone(),
        args: vec!["--parent-pid".to_string(), "1".to_string()],
    };
    let handle = supervisor.ensure(&spec).expect("起動できる");
    let _guard = SidecarGuard::new(handle.clone());

    let supervisor_pid = std::process::id();
    let mut reported: Option<u32> = None;
    let mut ready = false;
    while !ready {
        match recv_event(&events, Duration::from_secs(10), "起動行") {
            SidecarEvent::Output { line, .. } if line.starts_with("sidecar-smoke ready") => {
                reported = parse_parent_pid(&line);
                ready = true;
            }
            _ => {}
        }
    }
    assert_eq!(
        reported,
        Some(supervisor_pid),
        "起動行が報告する監視対象が監督の識別子でない（呼び出し側の値が残っている可能性）"
    );

    #[cfg(target_os = "linux")]
    {
        let pid = handle.pid();
        let cmdline = fs::read(format!("/proc/{pid}/cmdline")).expect("子のコマンドラインを読める");
        let args: Vec<String> = cmdline
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect();
        let flags = args
            .iter()
            .filter(|arg| arg.as_str() == "--parent-pid")
            .count();
        assert_eq!(flags, 1, "親監視のフラグが重複している: {args:?}");
        let value = args
            .iter()
            .position(|arg| arg == "--parent-pid")
            .and_then(|index| args.get(index + 1));
        let expected = supervisor_pid.to_string();
        assert_eq!(
            value.map(String::as_str),
            Some(expected.as_str()),
            "コマンドラインの監視対象が監督の識別子でない: {args:?}"
        );
    }
}

/// 掃除は冪等である。残存が無ければ 2 回目は 0 を返し、失敗しない。
#[test]
fn sweep_orphans_is_idempotent() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("sweep_orphans_is_idempotent");
        return;
    };

    let leftover = LeftoverGuard::new(spawn_leftover(&executable, "idempotence"));
    let pid = leftover.pid();
    assert!(
        wait_for_executable_name(
            pid,
            &file_name_of(&executable),
            Duration::from_secs(10),
            "残存プロセスの起動"
        ),
        "残存プロセスが起動しない"
    );

    let supervisor = Supervisor::new();
    assert!(
        supervisor.sweep_orphans() >= 1,
        "1 回目の掃除が残存プロセスを終了させていない"
    );
    assert!(
        wait_for_process_gone(pid, Duration::from_secs(10), "1 回目の掃除の後"),
        "残存プロセスが残っている"
    );
    assert_eq!(supervisor.sweep_orphans(), 0, "2 回目の掃除が 0 を返さない");
}

/// 掃除は、解決した実行ファイルがこのアプリの使うパスであることも要求する（design.md の
/// 照合規則を、名前だけでなくパスまで強めたもの。tasks.md 3.5 の指定）。
///
/// 名前が一致しても、期待するパスの集合に無ければ終了させない。正しいパスを与えれば終了する。
#[test]
fn sweep_orphans_requires_the_expected_executable_path() {
    let _serial = serialize();
    let Some(executable) = staged_original() else {
        skip_staged("sweep_orphans_requires_the_expected_executable_path");
        return;
    };

    let leftover = LeftoverGuard::new(spawn_leftover(&executable, "expected-path"));
    let pid = leftover.pid();
    assert!(
        wait_for_executable_name(
            pid,
            &file_name_of(&executable),
            Duration::from_secs(10),
            "残存プロセスの起動"
        ),
        "残存プロセスが起動しない"
    );

    // 同じ語幹を持つが別の場所のパスを期待値にする。名前は一致するがパスが一致しない。
    let elsewhere = TempDir::new("expected-path-elsewhere");
    let wrong = elsewhere.path().join(file_name_of(&executable));
    fs::write(&wrong, b"").expect("期待値のファイルを作れる");
    let supervisor = Supervisor::new().with_expected_executables([wrong]);
    assert_eq!(
        supervisor.sweep_orphans(),
        0,
        "期待するパスに無いのに掃除した"
    );
    assert!(process_is_alive(pid), "期待するパスに無いのに終了させた");

    let supervisor = Supervisor::new().with_expected_executables([executable.clone()]);
    assert!(
        supervisor.sweep_orphans() >= 1,
        "期待するパスなのに掃除できていない"
    );
    assert!(
        wait_for_process_gone(pid, Duration::from_secs(10), "正しい期待値での掃除の後"),
        "残存プロセスが残っている"
    );
}

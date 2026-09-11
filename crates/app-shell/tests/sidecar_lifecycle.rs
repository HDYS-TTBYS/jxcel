//! 補助プロセスの起動・共有・再起動と、起動失敗の区別（要件 5.4、5.5、5.8、tasks.md 3.2）。
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
//! プロセス数は OS から数える。Linux は `/proc`（コンテナ内に `pgrep` / `ps` が無いことを実測）、
//! macOS は `pgrep`、Windows は `tasklist` を使う。cargo は同一バイナリ内のテストを並行に
//! 走らせるため、このファイルのテストは直列化する。子プロセスは [`SidecarGuard`] の `Drop` で
//! 必ず強制終了して回収し、成功・失敗のどちらの経路でも孤児を残さない。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(any(target_os = "macos", windows))]
use std::process::Command;

use app_shell::sidecar::integrity::{IntegrityError, BUILD_TARGET_TRIPLE, EXPECTED_DIGESTS};
use app_shell::sidecar::supervisor::{
    SidecarHandle, SidecarSpec, SidecarSupervisor, SpawnError, Supervisor,
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
    repo_root()
        .join("sidecars")
        .join(format!(
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
    let mut permissions = fs::metadata(path).expect("メタデータを読める").permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).expect("権限を設定できる");
}

/// 起動する補助プロセスの仕様。親監視（tasks.md 1.6）が生存できるよう、監視対象はテスト自身の
/// 識別子にする。テストが終われば親が消え、補助プロセスは自ら終了する経路も持つ。
fn sidecar_spec(executable: &Path) -> SidecarSpec {
    SidecarSpec {
        kind: SidecarKind::Smoke,
        executable: executable.to_path_buf(),
        args: vec!["--parent-pid".to_string(), std::process::id().to_string()],
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
        .filter(|line| line.trim_start_matches('"').starts_with(SidecarKind::Smoke.as_str()))
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
            eprintln!("{context}: OS 上の実行中 sidecar-smoke 数 = {count}（期限切れ、期待 {expected}）");
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

    supervisor
        .shutdown_all()
        .expect("子も孫も残さず終了できる");

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
    assert_eq!(before, 2, "子と孫が起動しているはずである（OS 上の子孫数 {before}）");

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

    let after_direct = wait_for_sidecar_count(1, Duration::from_secs(5), "直接の子だけを終了した後");
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
    let handle = supervisor.ensure(&sidecar_spec(&executable)).expect("起動できる");
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
    let handle = supervisor.ensure(&sidecar_spec(&executable)).expect("起動できる");
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

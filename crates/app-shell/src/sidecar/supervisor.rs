//! 補助プロセスの起動・共有・再起動と、起動失敗の区別、プラットフォーム別の終了保証
//! （要件 5.4、5.5、5.6、5.8）。
//!
//! 種類ごとに高々 1 つのプロセスを保持し、要求が重なっても起動を 1 回に収める。予期せず終了して
//! いた場合は次に必要になった時点で改めて起動を試みる。起動失敗は原因を区別できる列挙として
//! 返す。終了は [`Supervisor::shutdown_all`] が担い、猶予を与えてから強制へ移る段階を経て、
//! 孫プロセスまで残さない（要件 5.6）。残留の掃除は 3.5、出力の行単位の取得と終了の通知は
//! 3.4 が所有する。
//!
//! # 不変条件と、それを成立させている箇所
//!
//! 「種類ごとに高々 1 つ」（要件 5.5）は、[`Supervisor::ensure`] が登録簿のロックを
//! **起動の決定から実行まで保持する**ことで成立している。ロックを外してから spawn すると、
//! 同じ種類への並行要求がそれぞれ「未起動」を観測して複数回起動する。この不変条件は並行 10
//! 要求のテスト（`tests/sidecar_lifecycle.rs` の完了状態）が OS 上のプロセス数まで数えて
//! 検証している。
//!
//! # 整合性検査の順序
//!
//! [`Supervisor::ensure`] は起動の**前**に整合性検査（[`super::integrity::verify`]）を行う。
//! したがって要件 5.3 の順序は呼び出し側の規約ではなく、この 1 箇所の構造で保証される。
//! 検査に失敗した場合、プロセスは起動しない。
//!
//! # 後続タスクへの申し送り
//!
//! - **3.5**: [`SidecarSupervisor`] は `ensure` / `get` / `shutdown_all` を宣言している。
//!   `sweep_orphans`（残留の掃除）は 3.5 がこの trait に追加する — ここに空のスタブを置くと
//!   「実装済み」と誤認されるため置かない。終了保証は `group_unix` / `job_windows` が持つ。
//! - **3.4**: 標準出力・標準エラーは [`Stdio::piped`] で作ってあり、[`SidecarHandle`] の
//!   [`take_stdout`](SidecarHandle::take_stdout) / [`take_stderr`](SidecarHandle::take_stderr) から
//!   取り出せる。本タスクでは読まない。3.4 が読み取りを張るまでに子がパイプの容量
//!   （OS 依存、Linux で 64 KiB）を超えて書くと子がブロックしうるが、検証用の
//!   `sidecar-smoke` は起動行 1 行だけを書いて後は標準入力を待つため、この前提は満たされる。

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use super::integrity::{self, IntegrityError};
use super::SidecarKind;

/// プラットフォーム別の終了保証。Unix はプロセスグループ、Windows は Job Object。
#[cfg(unix)]
use super::group_unix as platform;
#[cfg(windows)]
use super::job_windows as platform;

/// 猶予段で強制段へ移るまでに待つ既定時間。
///
/// 穏やかな終了通知を受けた補助プロセスが、書きかけの出力を流し、資源を閉じて自ら終了するには
/// 十分な長さでありながら、アプリ終了を目に見えて遅らせない上限である。通常は補助プロセスが
/// もっと早く終了するため、この上限まで待つのは「猶予信号を無視した」場合だけである。テストは
/// [`Supervisor::with_grace`] で短い値を与える。
pub const DEFAULT_GRACE: Duration = Duration::from_secs(3);

/// 終了段の待ち合わせで状態を確かめる間隔。
const TERMINATION_POLL: Duration = Duration::from_millis(10);

/// 強制段の後に直接の子を回収する上限。これを超えても待ち続けない（無限に待たない）。
const REAP_TIMEOUT: Duration = Duration::from_secs(5);

/// 起動する補助プロセスの仕様（design.md「SidecarSupervisor」の `SidecarSpec`）。
///
/// `kind` が共有の単位である。同じ `kind` への [`SidecarSupervisor::ensure`] は、`executable` や
/// `args` が異なっていても、起動中のプロセスがあればそれを返す（要件 5.5）。
#[derive(Debug, Clone)]
pub struct SidecarSpec {
    /// 監督する種類。登録簿の鍵であり、[`super::SidecarKind::as_str`] が実行ファイル名とも対応する。
    pub kind: SidecarKind,
    /// 起動する実行ファイルの絶対パス（プラットフォーム別の解決はアダプタ層 8.1 が行う）。
    pub executable: PathBuf,
    /// 実行ファイルへ渡す引数。
    pub args: Vec<String>,
}

/// 補助プロセスの起動に失敗した理由。原因は互いに区別できる（要件 5.4）。
#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    /// 実行ファイルが存在しない（パスが指すものが無い）。
    #[error("実行ファイルが見つからない: {path}")]
    NotFound { path: PathBuf },

    /// 実行ファイルは存在するが、実行できる形ではない（実行ビットが無い、ディレクトリなど）。
    #[error("実行権限がない: {path}")]
    NotExecutable { path: PathBuf },

    /// 整合性検査に失敗した（同梱時の内容と一致しない、読み取れない、期待値が未登録）。
    ///
    /// 原因は [`IntegrityError`] がさらに区別する。**`Unregistered`（期待ダイジェストが
    /// 埋め込まれていない）もここを通り、報告文にその事実が現れる** — 沈黙して通すことはない
    /// （要件 5.3、tasks.md 1.7 / 3.1 の申し送り）。design.md の列挙は `path` だけを持つが、
    /// 5.3 が求める「期待値と実測値」を報告から落とさないため、原因を `source` として併せて運ぶ。
    #[error("整合性検査に失敗した: {path}: {source}")]
    IntegrityMismatch {
        path: PathBuf,
        #[source]
        source: IntegrityError,
    },

    /// 実行ファイルの起動そのものに失敗した（`exec` / `CreateProcess` の失敗）。
    #[error("プロセスの起動に失敗した: {message}")]
    Spawn { message: String },
}

/// 補助プロセスの終了に失敗した理由（design.md「SidecarSupervisor」の `ShutdownError`）。
///
/// 終了処理は冪等であり、既に終了した子・存在しないグループへの信号は成功として扱う。したがって
/// ここへ到達するのは、強制段の信号そのものが失敗した場合（権限など）に限られる。登録簿は
/// 失敗の有無にかかわらず空にするため、呼び出しを繰り返しても状態は一貫する。
#[derive(Debug, thiserror::Error)]
#[error("補助プロセス {kind} の終了に失敗した: {message}")]
pub struct ShutdownError {
    /// 終了できなかった補助プロセスの種類。
    pub kind: SidecarKind,
    /// 失敗の内容（`Display` 済みの理由）。
    pub message: String,
}

/// 整合性検査の差し替え口。
pub type IntegrityVerifier =
    dyn Fn(SidecarKind, &Path) -> Result<(), IntegrityError> + Send + Sync;

/// 起動した補助プロセスへの共有ハンドル。
///
/// 同じ種類への要求はすべて同じ実体を参照する（要件 5.5）。`Arc` で共有するため clone は安い。
/// 子は `Mutex` の内側にある — [`try_wait`](Self::try_wait) / [`kill`](Self::kill) と、
/// 標準出力・標準エラーの取り出しは `&mut Child` を要求するためである。
#[derive(Debug, Clone)]
pub struct SidecarHandle {
    kind: SidecarKind,
    child: Arc<Mutex<Child>>,
    /// プラットフォーム別の終了保証の対象（Unix はプロセスグループ、Windows は Job Object）。
    /// `Arc` で共有し、最後の参照が消えた時点で Windows のジョブハンドルが閉じる。
    platform: Arc<platform::Group>,
}

impl SidecarHandle {
    fn new(kind: SidecarKind, child: Child, platform: platform::Group) -> Self {
        SidecarHandle {
            kind,
            child: Arc::new(Mutex::new(child)),
            platform: Arc::new(platform),
        }
    }

    /// このハンドルが監督している種類。
    pub fn kind(&self) -> SidecarKind {
        self.kind
    }

    /// 起動したプロセスの識別子。回収後も起動時の値であり、他のプロセスへは再利用されうる。
    pub fn pid(&self) -> u32 {
        self.lock().id()
    }

    /// 子の終了状態を取得する。`Ok(None)` はまだ生存、`Ok(Some(_))` は終了（このとき子は回収
    /// される）。監視は `try_wait` によるものであり、待ち合わせ（busy wait）はしない。
    pub fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        self.lock().try_wait()
    }

    /// 直接の子だけを強制終了する。
    ///
    /// 補助プロセスがさらに孫を起動している場合、これだけでは孫に届かない。本番の終了経路は
    /// [`SidecarSupervisor::shutdown_all`]（プロセスグループ / Job Object 宛）であり、こちらは
    /// テストの後始末など、直接の子だけを対象にした低水準の操作である。
    pub fn kill(&self) -> io::Result<()> {
        self.lock().kill()
    }

    /// 標準出力の読み取り口を取り出す（1 度だけ）。行単位の取得はタスク 3.4 が張る。
    pub fn take_stdout(&self) -> Option<ChildStdout> {
        self.lock().stdout.take()
    }

    /// 標準エラーの読み取り口を取り出す（1 度だけ）。行単位の取得はタスク 3.4 が張る。
    pub fn take_stderr(&self) -> Option<ChildStderr> {
        self.lock().stderr.take()
    }

    /// 子がまだ動いているか。
    ///
    /// 判定不能（`try_wait` の失敗）は「生存」に倒す。種類ごとに高々 1 つという不変条件
    /// （要件 5.5）は、死んだ子を取り違えて二重起動するより、保守的に保つ方が安全である。
    fn is_live(&self) -> bool {
        matches!(self.lock().try_wait(), Ok(None))
    }

    fn lock(&self) -> MutexGuard<'_, Child> {
        self.child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// 補助プロセスの監督（design.md「SidecarSupervisor」の Service Interface）。
///
/// `sweep_orphans`（残留の掃除）はタスク 3.5 がこの trait に追加する。ここに空のスタブを置くと
/// 「実装済み」と誤認されるため宣言しない。
pub trait SidecarSupervisor {
    /// 起動済みなら既存のハンドルを返し、未起動なら起動する。予期せず終了していた場合はこの
    /// 呼び出しで改めて起動を試みる（要件 5.8）。
    fn ensure(&self, spec: &SidecarSpec) -> Result<SidecarHandle, SpawnError>;

    /// 起動しているものだけを返す。起動はしない。終了済みは `None` を返す。
    fn get(&self, kind: SidecarKind) -> Option<SidecarHandle>;

    /// 起動したすべての補助プロセスを終了させる（要件 5.6）。
    ///
    /// 種類ごとに、まずグループ / ジョブ宛の穏やかな終了を送り、[`DEFAULT_GRACE`]（テストでは
    /// [`Supervisor::with_grace`] が与える値）だけ待ってから強制終了へ移る。**直接の子だけでなく
    /// 孫プロセスまで対象にする**（Unix は `killpg`、Windows は `TerminateJobObject`）。
    ///
    /// 冪等である: 登録簿はこの呼び出しで空になり、既に終了した子・存在しないグループへの信号は
    /// 成功として扱う。したがって 2 回目は何も待たずに成功し、既に死んだ子でハングしない。
    fn shutdown_all(&self) -> Result<(), ShutdownError>;
}

/// アプリ全体で 1 実体の監督。
///
/// 起動中のプロセスの登録簿を `Arc` で共有するため、clone は同じ登録簿を指す。ウィンドウごとに
/// 複製を作っても、同じ種類への要求は同じプロセスに解決する（要件 5.5）。GUI を起動せずに
/// 検証できるよう、Tauri に依存しない。
#[derive(Clone)]
pub struct Supervisor {
    /// design.md「Data Models」の `SidecarTable`（揮発）。種類から実行中のプロセスへの写像で、
    /// 不変条件は「1 つの種類につき実行中のプロセスは高々 1 つ」（要件 5.5）。
    /// [`SidecarSupervisor::ensure`] がロックを保持してこの不変条件を守る。
    registry: Arc<Mutex<HashMap<SidecarKind, SidecarHandle>>>,
    verify: Arc<IntegrityVerifier>,
    /// 猶予段で強制段へ移るまでに待つ時間。本番は [`DEFAULT_GRACE`]、テストは
    /// [`Supervisor::with_grace`] が短い値を与える。
    grace: Duration,
}

impl Supervisor {
    /// 本番の監督。整合性検査は [`integrity::verify`]（ビルド時に埋め込んだダイジェストとの照合）。
    pub fn new() -> Self {
        let verify: Arc<IntegrityVerifier> = Arc::new(integrity::verify);
        Supervisor {
            registry: Arc::new(Mutex::new(HashMap::new())),
            verify,
            grace: DEFAULT_GRACE,
        }
    }

    /// 整合性検査を差し替えた監督を作る（テスト seam）。
    ///
    /// 本番の [`Supervisor::new`] は実検査をそのまま使う。この入口が必要なのは、実検査だけでは
    /// 決定的に再現できない起動失敗の結果を確かめるためである:
    /// - `Spawn` — 実在し実行権限もあるが `exec` に失敗するファイルは、実検査を通せない
    /// - `Unregistered` — 期待ダイジェストが埋め込まれていないビルドを再現せずに観測する
    /// - `NotExecutable` / `NotFound` は前段（[`preflight`]）が返すため差し替えを要さない
    ///
    /// **検査を無効化した監督を本番のコードが構築してはならない。**
    pub fn with_verifier(verify: Arc<IntegrityVerifier>) -> Self {
        Supervisor {
            registry: Arc::new(Mutex::new(HashMap::new())),
            verify,
            grace: DEFAULT_GRACE,
        }
    }

    /// 猶予時間を差し替える（テスト seam）。
    ///
    /// テストは短い値（数百ミリ秒）を与えて、猶予段と強制段の段階を現実的な時間で観測する。
    /// 本番は [`DEFAULT_GRACE`] のまま使う。
    pub fn with_grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    fn registry(&self) -> MutexGuard<'_, HashMap<SidecarKind, SidecarHandle>> {
        self.registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for Supervisor {
    fn default() -> Self {
        Supervisor::new()
    }
}

impl SidecarSupervisor for Supervisor {
    fn ensure(&self, spec: &SidecarSpec) -> Result<SidecarHandle, SpawnError> {
        // 登録簿のロックを起動の決定と実行の全体にわたって保持する。この保持が
        // 「要求が重なっても起動は 1 回」（要件 5.5、tasks.md 3.2 の完了状態）を成立させている。
        // ロックを外してから spawn すると、同じ種類への並行要求がそれぞれ「未起動」を観測して
        // 複数回起動する。並行 10 要求のテストが OS 上のプロセス数まで数えてこの保持を検証する。
        let mut registry = self.registry();

        let existing = registry.get(&spec.kind).cloned();
        match existing {
            Some(handle) if handle.is_live() => return Ok(handle),
            // 予期せず終了していた。登録を外し、この要求で改めて起動する（要件 5.8）。
            Some(_) => {
                registry.remove(&spec.kind);
            }
            None => {}
        }

        // 整合性検査は spawn の内側で起動の前に行われる（要件 5.3）。
        let handle = spawn(spec, &*self.verify)?;
        registry.insert(spec.kind, handle.clone());
        Ok(handle)
    }

    fn get(&self, kind: SidecarKind) -> Option<SidecarHandle> {
        let mut registry = self.registry();
        let handle = registry.get(&kind).cloned()?;
        if handle.is_live() {
            Some(handle)
        } else {
            // 終了済みは「起動しているもの」ではない。登録から外しておく（次回の ensure が
            // 改めて起動する。要件 5.8）。
            registry.remove(&kind);
            None
        }
    }

    fn shutdown_all(&self) -> Result<(), ShutdownError> {
        // 登録簿のロックを終了処理の全体にわたって保持する。終了中に `ensure` が新しい子を
        // 登録して「終了したはずの補助プロセス」が増えることを防ぎ、戻った時点で登録簿が空で
        // あることを保証する。ロックの取得順は `ensure` と同じ（登録簿 → 子）なので循環しない。
        let mut registry = self.registry();
        let handles: Vec<SidecarHandle> = registry.values().cloned().collect();

        let mut first_error: Option<ShutdownError> = None;
        for handle in &handles {
            if let Err(error) = terminate(handle, self.grace) {
                first_error.get_or_insert(error);
            }
        }

        // 強制段が失敗しても登録簿は空にする。これにより 2 回目の呼び出しは何もせず成功し、
        // 状態が一貫する（タスク 3.3 の冪等性）。
        registry.clear();

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// 1 つの補助プロセスを、猶予段 → 強制段の順にグループ / ジョブ宛で終了させる。
///
/// 猶予段ではグループ宛に穏やかな終了を送り、直接の子が終了してグループ / ジョブが空になるか、
/// 猶予時間が尽きるまで待つ。尽きたら強制段でグループ / ジョブ全体を強制終了し、直接の子を
/// 回収する（ゾンビを残さない）。既に終了している子に対しては、待たずに成功する。
fn terminate(handle: &SidecarHandle, grace: Duration) -> Result<(), ShutdownError> {
    let group = handle.platform.as_ref();

    // 猶予段。既にグループが空ならこの信号は ESRCH になり、成功として扱われる。
    // 穏やかな終了通知が届かないプラットフォーム（コンソールを持たない Windows など）でも
    // 失敗を無視して強制段へ進む。
    let _ = group.signal_graceful();

    let deadline = Instant::now() + grace;
    loop {
        let child_exited = matches!(handle.try_wait(), Ok(Some(_)));
        if child_exited && !group.exists() {
            // 直接の子も孫も残っていない。待つ必要は無い。
            return Ok(());
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(TERMINATION_POLL);
    }

    // 強制段。猶予内に消えなかった（孫が猶予信号を無視した等）ため、グループ / ジョブ全体を
    // 強制終了する。
    group.signal_force().map_err(|error| ShutdownError {
        kind: handle.kind,
        message: error.to_string(),
    })?;

    // 直接の子を回収してゾンビを残さない。期限付きで、届かなければ待ち続けない。
    let reap_deadline = Instant::now() + REAP_TIMEOUT;
    loop {
        match handle.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) if Instant::now() >= reap_deadline => break,
            Ok(None) => thread::sleep(TERMINATION_POLL),
        }
    }
    Ok(())
}

/// 実プロセスを起動する。
///
/// **整合性検査は起動の前**に行い、失敗したら spawn しない（要件 5.3）。これにより
/// 「検査を通ってから起動する」順序が、呼び出し側の規約ではなくこの 1 箇所の構造で保証される。
/// 標準出力・標準エラーはパイプで作り、行単位の取得（タスク 3.4）へ引き渡す。標準入力は
/// 与えない（監督側からの入力を持たない）。
fn spawn(spec: &SidecarSpec, verify: &IntegrityVerifier) -> Result<SidecarHandle, SpawnError> {
    preflight(&spec.executable)?;
    verify(spec.kind, &spec.executable).map_err(|source| SpawnError::IntegrityMismatch {
        path: spec.executable.clone(),
        source,
    })?;

    let mut command = Command::new(&spec.executable);
    command
        .args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // 終了保証の対象になるよう、プラットフォーム別の起動設定を与える（Unix は新しい
    // プロセスグループのリーダーにする `setpgid`、Windows は新しいプロセスグループを作る
    // 作成フラグ）。これにより、この後に子が起動する孫も同じグループ / ジョブに入る。
    platform::configure(&mut command);

    let mut child = command.spawn().map_err(|error| SpawnError::Spawn {
        message: error.to_string(),
    })?;

    // 起動した子を終了保証の対象へ割り当てる（Unix は PID = PGID、Windows は Job Object）。
    // 割り当てに失敗した子は終了保証の外にあるため、起動を成功として返さず、子を始末して報告する。
    let group = match platform::Group::attach(&child) {
        Ok(group) => group,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SpawnError::Spawn {
                message: format!("終了保証の対象へ割り当てられない: {error}"),
            });
        }
    };

    Ok(SidecarHandle::new(spec.kind, child, group))
}

/// 起動対象として妥当かを、整合性検査より前に確かめる。
///
/// `NotFound`（存在しない）と `NotExecutable`（実行できる形ではない）を、`IntegrityMismatch` /
/// `Spawn` と区別できる形で返す（要件 5.4）。
fn preflight(path: &Path) -> Result<(), SpawnError> {
    if !path.exists() {
        return Err(SpawnError::NotFound {
            path: path.to_path_buf(),
        });
    }
    if !path.is_file() || !is_executable(path) {
        return Err(SpawnError::NotExecutable {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

/// 実行権限を持つかを返す。
///
/// Unix は実行ビット（所有者・グループ・その他のいずれか）を見る。Windows に実行ビットは
/// 存在しないため、存在するファイルはここを通過し、実行可能かどうかは `CreateProcess` が
/// 決める（拒否されれば `SpawnError::Spawn` になる）。
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

//! 補助プロセスの起動・共有・再起動・終了保証と、出力の取得および予期せぬ終了の通知
//! （要件 5.4〜5.9）。
//!
//! 種類ごとに高々 1 つのプロセスを保持し、要求が重なっても起動を 1 回に収める。予期せず終了して
//! いた場合は次に必要になった時点で改めて起動を試みる。起動失敗は原因を区別できる列挙として
//! 返す。終了は [`Supervisor::shutdown_all`] が担い、猶予を与えてから強制へ移る段階を経て、
//! 孫プロセスまで残さない（要件 5.6）。標準出力と標準エラーは行単位で読み、購読側へ
//! [`SidecarEvent::Output`] として配る（要件 5.9）。子が終了した事実は
//! [`SidecarEvent::Exited`] として配る（要件 5.7）。残留の掃除は 3.5 が所有する
//! （design.md「Event Contract」）。
//!
//! # 不変条件と、それを成立させている箇所
//!
//! 「種類ごとに高々 1 つ」（要件 5.5）は、[`Supervisor::ensure`] が登録簿のロックを
//! **起動の決定から実行まで保持する**ことで成立している。ロックを外してから spawn すると、
//! 同じ種類への並行要求がそれぞれ「未起動」を観測して複数回起動する。この不変条件は並行 10
//! 要求のテスト（`tests/sidecar_lifecycle.rs` の完了状態）が OS 上のプロセス数まで数えて
//! 検証している。
//!
//! 「Exited はその子の最後の出力の後に届く」は、監視スレッドが
//! **読み取りスレッドを join してから通知する**ことで成立している（[`spawn_monitor`]）。
//! パイプに未読の行が残っていても、読み取りが終わってから通知が出る。タイミングの偶然に
//! 依存しない。
//!
//! **帰結（既知の境界）**: 子が孫プロセスを起動し、その孫が標準出力のパイプを保持したまま
//! 生き続ける場合、読み取りは EOF に到達しないため `Exited` は孫が終了するまで届かない。
//! これは「最後の出力の後に通知する」を構造で守るための帰結である。孫の残留そのものは
//! 終了保証（要件 5.6、[`SidecarSupervisor::shutdown_all`]）と残留の掃除（3.5）が扱う。
//!
//! # ロックの取得順
//!
//! ①登録簿（[`Supervisor::registry`]） → ②子（[`SidecarInner::child`]） →
//! ③終了状態（[`SidecarInner::exit`]） → ④購読者表（[`Subscribers`]）。
//!
//! 取得する側は常にこの順で取り、下位から上位へは取らない。①を保持したまま起動・終了を
//! 行うのは意図的であり（要件 5.5 の「起動は 1 回」）、②③の保持は短い。`kill` だけが
//! ②を保持したまま③を確かめ、既に終了を記録した子へは信号を送らない（回収から記録までの
//! 窓の扱いは [`SidecarHandle::kill`] の doc を参照）。③を保持したまま②を取る経路は無いので
//! 循環しない。**配布（④）は登録簿のロックを保持せずに行う** — 読み取りスレッドは①②③を
//! 取らず、監視スレッドは③だけを配布の前に取るため、購読者が配布を待って監督へ再入しても
//! 循環しない。購読者ごとの送信路は無限容量であり、`send` はブロックしない。
//!
//! # 行の単位と長さの上限、8.1 への契約
//!
//! 1 行は改行（LF / CRLF）までを 1 つの [`SidecarEvent::Output`] とする。**行の区切り方と
//! CRLF の扱いは `BufRead::lines` と完全に同じ**である — `\r` の直後が `\n` のとき、その 1 つ
//! だけを落とす（`\r\r\n` の 1 バイト目の内容 `\r` は落とさない）。これは `{x, CR, LF}` から
//! 作れる長さ 7 までの全文字列・上限 1〜8 の網羅的な参照等価テスト
//! （`supervisor::tests::fragments_reconstruct_to_bufread_lines_for_every_string`）で固定して
//! いる。**1 行が [`MAX_LINE_BYTES`] を超えた場合は、その長さごとに区切って複数の出来事として
//! 通知する**点だけが `lines` と異なる（断片の連結で元の 1 行に戻る）。上限を設けないと、改行を
//! 書かない子（壊れた / 悪意ある出力）が読み取り側のメモリを無制限に消費する。
//!
//! **8.1 への契約（注意）**: 断片は、それだけでは**完成した 1 行なのか長い行の先頭部分なのかを
//! 見分けられない**（上限ちょうどの行と、上限で切られた断片は同じバイト列になりうる）。読み取りの
//! 内部（[`read_lines`]）の契約は「**終端された断片 = 行末**、終端されていない断片 = 行の続き
//! （連結する）」であり、行の復元規則はそこに定義されている。しかし design が定める
//! [`SidecarEvent`] のフィールド集合（`kind` / `stream` / `line`）にはその区別を載せる場所が
//! 無い。したがって 8.1 は `Output` を**流れの断片**として扱い、そのまま記録に流すこと。断片を
//! 連結して元の 1 行を復元することは現契約では**保証できない** — 必要なら 8.1（または 3.x）が
//! 「行末まで届いたか」を持つフィールドを設計へ加算する判断を行うこと。順序そのもの（同じ流れの
//! 断片が順に届くこと、`Exited` が最後の断片の後に届くこと）は保証される。
//!
//! # 再起動をまたぐ出来事の帰属（8.1 への契約）
//!
//! 監視スレッドは終了を**記録してから**読み取りスレッドを join する（生存判定を早く更新し、
//! 要件 5.8 の再起動を成立させるため。`spawn_monitor`）。したがって `ensure` が新しい子を
//! 起動した後も前の子の読み取りが配布を続けていることがあり、**前の子の `Exited` より後に、
//! 新しい子の `Output` が届きうる**。出来事は種類（`kind`）しか持たないため、購読側は「いま
//! 届いた出来事が現在の実体のものか」を区別できない。8.1 はイベントを**そのまま時系列で**
//! 記録すること（実体の同一性を前提にしないこと）。
//!
//! # 意図的な終了の扱い（決定）
//!
//! [`Supervisor::shutdown_all`] による終了も [`SidecarEvent::Exited`] を出す。終了の由来は
//! [`SidecarExit`] の値（`Deliberate` / `Unexpected`）が担う。**由来を通知しないと、利用側は
//! 「通知が無い = 生存している」と解釈せざるを得ず、子が消えたことを知る手段が無くなる。**
//! 由来を値で分けることで、**アプリケーション自身の終了処理が「予期せぬクラッシュ」として
//! 報告されることはない**（要件 5.7 が対象とするのは予期せぬ場合である）。判定は
//! 「信号を送る時点で、監視スレッドが終了を記録していたか」で行う（[`terminate`]）。
//!
//! **分類の実際の窓（正直に記載する）**: 判定は実際の死の瞬間ではなく、**監視スレッドが記録した
//! 終了状態**を基準にする。したがって、子が自力で死んだ直後に `shutdown_all` が走り、監視が
//! まだ終了を記録していなければ、その子は `Deliberate` として報告される。安全側（意図的な終了を
//! クラッシュと誤報しない）に倒れており、逆向き — 意図的な終了が `Unexpected` になる — は
//! 起こらない（フラグは信号より先に立つ）。
//!
//! # 後続タスクへの申し送り
//!
//! - **3.5**: [`SidecarSupervisor`] は `ensure` / `get` / `shutdown_all` / `subscribe` を
//!   宣言している。`sweep_orphans`（残留の掃除）は 3.5 がこの trait に追加する — ここに
//!   空のスタブを置くと「実装済み」と誤認されるため置かない。終了保証は `group_unix` /
//!   `job_windows` が持つ。
//! - **8.1**: 出来事を診断の記録先へ流すのは `SidecarHost` の責務である。監督は publish
//!   するだけで、記録の購読は [`SidecarSupervisor::subscribe`] の受信側が行う。

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::integrity::{self, IntegrityError};
use super::SidecarKind;

/// プラットフォーム別の終了保証と終了の待機。Unix はプロセスグループ、Windows は Job Object。
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

/// 1 行として通知する最大の長さ。超えた行はこの長さごとに分割して通知する（モジュール冒頭の
/// 「行の単位と長さの上限」を参照）。改行を書かない子による無制限のメモリ消費を防ぐ。
const MAX_LINE_BYTES: usize = 64 * 1024;

/// 読み取りのバッファ長。1 行の上限と同じ大きさにして、長い行でも syscall の回数を抑える。
const READER_BUFFER_BYTES: usize = MAX_LINE_BYTES;

/// 補助プロセスの出力がどちらの流れから来たか（design.md「Event Contract」の `stream`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SidecarStream {
    /// 標準出力。
    Stdout,
    /// 標準エラー出力。
    Stderr,
}

/// 補助プロセスの終了の由来と、OS が報告した終了状態（design.md「Event Contract」の `status`）。
///
/// design.md は `Exited { kind, status }` とだけ定めており `status` の型を指定していない。
/// `ExitStatus` をそのまま入れると意図的な終了（要件 5.6）と予期せぬ終了（要件 5.7）を区別
/// できず、購読側が両者を取り違える。そこで `status` の型をこの列挙とし、由来を値で分ける
/// （フィールドは増やしていない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarExit {
    /// 子が自ら終了したか、外部から強制終了された（要件 5.7 の予期せぬ終了）。
    ///
    /// `status` は OS が報告した終了状態。取得できなかった場合だけ `None` になる。
    Unexpected {
        /// OS が報告した終了状態。
        status: Option<ExitStatus>,
    },
    /// 監督側の [`SidecarSupervisor::shutdown_all`] が意図的に終了させた（要件 5.6）。
    Deliberate {
        /// OS が報告した終了状態。
        status: Option<ExitStatus>,
    },
}

impl SidecarExit {
    /// OS が報告した終了状態（取得できなかった場合は `None`）。
    pub fn status(self) -> Option<ExitStatus> {
        match self {
            SidecarExit::Unexpected { status } | SidecarExit::Deliberate { status } => status,
        }
    }

    /// 意図的な終了（[`SidecarSupervisor::shutdown_all`]）か。
    ///
    /// **予期せぬ終了（要件 5.7）だけを扱いたい購読側はこれで選別する。** アプリケーション
    /// 自身の終了処理がクラッシュとして解釈されることはない。
    pub fn is_deliberate(self) -> bool {
        matches!(self, SidecarExit::Deliberate { .. })
    }
}

/// 補助プロセスから購読側へ届く出来事（design.md「Event Contract」）。
///
/// 同一プロセスの出力は行単位で順序を保ち、[`SidecarEvent::Exited`] はその子の最後の出力の後に
/// 届く。購読側は `Exited` を受けてもアプリケーション本体を終了させてはならない（要件 5.7）。
#[derive(Debug, Clone)]
pub enum SidecarEvent {
    /// 行単位の出力（要件 5.9）。`stream` が標準出力と標準エラーを区別する。
    ///
    /// **`line` は断片である。** 完成した 1 行か、[`MAX_LINE_BYTES`] で切られた長い行の一部かを
    /// このイベントだけでは区別できない（モジュール冒頭「8.1 への契約」を参照）。改行と CRLF の
    /// `\r` は含まない。読み続けない購読者がいても、他の購読と読み取りスレッドは止まらない。
    Output {
        /// 出力した補助プロセスの種類。
        kind: SidecarKind,
        /// どちらの流れから来たか。
        stream: SidecarStream,
        /// 改行（LF / CRLF の `\r`）を含まない断片。
        line: String,
    },
    /// 補助プロセスの終了（要件 5.7）。その子の最後の出力より後に届く。
    ///
    /// 同じ種類の子が再起動された場合、**この出来事が前の実体のものである保証は購読側には無い**
    /// （モジュール冒頭「再起動をまたぐ出来事の帰属」を参照）。
    Exited {
        /// 終了した補助プロセスの種類。
        kind: SidecarKind,
        /// 終了の由来と終了状態。
        status: SidecarExit,
    },
}

/// 出来事を購読側へ配る（design.md「SettingsStore」の `subscribe` と同じ形の入口）。
///
/// 購読者ごとに独立した送信路を持つ。1 つの購読者が受信を止めても他の購読者と読み取りスレッドは
/// 止まらない（送信路は無限容量であり `send` はブロックしない）。代償として、受信を止めた
/// 購読者の待ち行列はその分だけ伸びる — **取りこぼしはしないが、遅い購読者はメモリを消費する**。
/// 診断の記録先（8.1）のように常時読み続ける購読者を想定しており、取りこぼしよりこちらを選ぶ。
#[derive(Debug, Default)]
struct Subscribers {
    /// 購読者ごとの送信路。送信に失敗した（受信側を破棄した）ものは配布のたびに外す。
    senders: Mutex<Vec<Sender<SidecarEvent>>>,
}

impl Subscribers {
    /// 新しい購読を作る。戻り値はこの呼び出し以降の出来事を受け取る。
    fn subscribe(&self) -> Receiver<SidecarEvent> {
        let (sender, receiver) = mpsc::channel();
        self.lock().push(sender);
        receiver
    }

    /// すべての購読者へ同じ出来事を配る。
    ///
    /// 購読者表のロックを保持したまま送る。`Sender::send`（無限容量）はブロックしないため、
    /// 遅い購読者がいても読み取りスレッドは止まらない。ロックが配布の全順序を作るので、
    /// **あるスレッドが送り終えた出力より後に別のスレッドが終了通知を送る**という順序が、
    /// 購読者ごとに保証される（多重生産の送信路の順序保証だけに頼らない）。
    fn publish(&self, event: SidecarEvent) {
        let mut senders = self.lock();
        senders.retain(|sender| sender.send(event.clone()).is_ok());
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Sender<SidecarEvent>>> {
        self.senders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

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

/// 子の終了状態。監視スレッドが記録し、監督の生存判定はこれだけを見る。
///
/// プラットフォーム別の待機（`platform::Waiter`）が子を回収するため、`Child::try_wait` を
/// 生存判定には使えない。記録は待機が返った直後に行うので、判定は子の死とほぼ同時に切り替わる。
#[derive(Debug, Clone, Copy)]
enum ExitState {
    /// まだ終了を観測していない。
    Alive,
    /// 終了を観測した。`None` は終了状態そのものを取得できなかった場合。
    Exited(Option<ExitStatus>),
}

/// 起動した補助プロセスへの共有ハンドル。
///
/// 同じ種類への要求はすべて同じ実体を参照する（要件 5.5）。`Arc` で共有するため clone は安い。
#[derive(Debug, Clone)]
pub struct SidecarHandle {
    inner: Arc<SidecarInner>,
}

/// ハンドルが共有する実体。読み取りスレッドと監視スレッドも `Arc` でこれを参照する。
#[derive(Debug)]
struct SidecarInner {
    kind: SidecarKind,
    /// 起動時に確定する子の識別子。回収後も変わらない。
    pid: u32,
    /// 起動した子。`kill` と標準入力の取り出しに使う。終了の待機は
    /// `platform::Waiter` が別に行う（このロックを保持したままブロックしない）。
    child: Mutex<Child>,
    /// プラットフォーム別の終了保証の対象（Unix はプロセスグループ、Windows は Job Object）。
    /// `Arc` で共有し、最後の参照が消えた時点で Windows のジョブハンドルが閉じる。
    platform: Arc<platform::Group>,
    /// 監視スレッドが記録する終了状態。
    exit: Mutex<ExitState>,
    /// この子を意図的に終了させたか。`terminate` が信号を送る前に立てる。
    deliberate: AtomicBool,
    /// 出来事の配布先。監督の実体と共有する。
    subscribers: Arc<Subscribers>,
}

impl SidecarInner {
    fn lock(&self) -> MutexGuard<'_, Child> {
        self.child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn exit(&self) -> MutexGuard<'_, ExitState> {
        self.exit
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 監視スレッドが終了を記録する。
    fn mark_exited(&self, status: Option<ExitStatus>) {
        *self.exit() = ExitState::Exited(status);
    }
}

impl SidecarHandle {
    /// このハンドルが監督している種類。
    pub fn kind(&self) -> SidecarKind {
        self.inner.kind
    }

    /// 起動したプロセスの識別子。回収後も起動時の値であり、他のプロセスへは再利用されうる。
    pub fn pid(&self) -> u32 {
        self.inner.pid
    }

    /// 子の終了状態を取得する。`Ok(None)` はまだ生存、`Ok(Some(_))` は終了。待ち合わせはしない。
    ///
    /// 終了していても状態を取得できなかった場合だけ `Err` を返す（監視スレッドが待機に失敗した
    /// 場合であり、通常は起こらない）。
    pub fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        match &*self.inner.exit() {
            ExitState::Alive => Ok(None),
            ExitState::Exited(Some(status)) => Ok(Some(*status)),
            ExitState::Exited(None) => Err(io::Error::other("子の終了状態を取得できなかった")),
        }
    }

    /// 直接の子だけを強制終了する。
    ///
    /// 補助プロセスがさらに孫を起動している場合、これだけでは孫に届かない。本番の終了経路は
    /// [`SidecarSupervisor::shutdown_all`]（プロセスグループ / Job Object 宛）であり、こちらは
    /// テストの後始末など、直接の子だけを対象にした低水準の操作である。
    ///
    /// **保証の範囲（正確に）**: 監視が `ExitState::Exited` を記録した後は信号を送らない。これは
    /// 「終了の待機（`platform::Waiter`）が生の `waitpid` / `WaitForSingleObject` で子を回収する
    /// ため、std の `Child::kill` が持つ『回収済みなら失敗する』防御が当てにできず、回収済みの
    /// 識別子が再利用されうる」ことへの防御である。**ただし記録は回収の直後（同じ監視スレッドで
    /// 待機が返った後）に行われるため、回収から記録までの間に呼ばれた場合は `Alive` に見え、
    /// 再利用された識別子へ信号を送りうる。** この窓は待機が返ってから mutex を取って書くまでの
    /// 数命令であり、その間に PID 空間（Linux の既定で約 3 万）を使い切って同一識別子が再利用
    /// されることは現実的でないため、許容している。記録後の（実際に起こる）経路は塞いである。
    pub fn kill(&self) -> io::Result<()> {
        let mut child = self.inner.lock();
        if matches!(&*self.inner.exit(), ExitState::Exited(_)) {
            return Ok(());
        }
        child.kill()
    }

    /// 標準入力の書き込み口を取り出す（1 度だけ）。
    ///
    /// 補助プロセスへ入力を与える唯一の経路である。取り出した値を破棄すると子の標準入力が
    /// 閉じる（`sidecar-smoke` は入力が尽きても自発的には終了しない）。
    pub fn take_stdin(&self) -> Option<ChildStdin> {
        self.inner.lock().stdin.take()
    }

    /// 子がまだ動いているか。
    ///
    /// 判定は監視スレッドが記録した終了状態だけで行う。`Child::try_wait` は使えない
    /// （プラットフォーム別の待機が子を回収するため）。
    fn is_live(&self) -> bool {
        matches!(&*self.inner.exit(), ExitState::Alive)
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

    /// 新しい購読を作る。戻り値はこの呼び出し以降に配られた出来事を受け取る
    /// （要件 5.7、5.9。design.md「Event Contract」の購読入口）。
    ///
    /// 購読は複数作れる。それぞれ独立しており、片方が受信を止めても他方と読み取りスレッドは
    /// 止まらない。既に配られた出来事は再送しない（購読は過去を遡らない）。
    fn subscribe(&self) -> Receiver<SidecarEvent>;
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
    /// 出来事の配布先。起動した子の読み取りスレッドと監視スレッドが共有する。
    subscribers: Arc<Subscribers>,
}

impl Supervisor {
    /// 本番の監督。整合性検査は [`integrity::verify`]（ビルド時に埋め込んだダイジェストとの照合）。
    pub fn new() -> Self {
        let verify: Arc<IntegrityVerifier> = Arc::new(integrity::verify);
        Supervisor {
            registry: Arc::new(Mutex::new(HashMap::new())),
            verify,
            grace: DEFAULT_GRACE,
            subscribers: Arc::new(Subscribers::default()),
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
            subscribers: Arc::new(Subscribers::default()),
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
        //
        // 読み取りスレッドはこのロックを取らないため、子の出力待ちで起動が止まることはない
        // （モジュール冒頭の「ロックの取得順」を参照）。
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
        let handle = spawn(spec, &*self.verify, &self.subscribers)?;
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
        // あることを保証する。ロックの取得順は `ensure` と同じ（登録簿 → 子 → 終了状態）なので
        // 循環しない。監視スレッドは登録簿を取らないため、終了通知の配布で待たされることもない。
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

    fn subscribe(&self) -> Receiver<SidecarEvent> {
        self.subscribers.subscribe()
    }
}

/// 1 つの補助プロセスを、猶予段 → 強制段の順にグループ / ジョブ宛で終了させる。
///
/// 猶予段ではグループ宛に穏やかな終了を送り、直接の子が終了してグループ / ジョブが空になるか、
/// 猶予時間が尽きるまで待つ。尽きたら強制段でグループ / ジョブ全体を強制終了し、直接の子を
/// 回収する（ゾンビを残さない）。既に終了している子に対しては、待たずに成功する。
fn terminate(handle: &SidecarHandle, grace: Duration) -> Result<(), ShutdownError> {
    let group = handle.inner.platform.as_ref();

    // **終了の由来を、信号を送る前に決める。** この時点で生存していた子は意図的な終了として
    // 報告され、既に終了していた子は予期せぬ終了のまま報告される（`SidecarExit`）。
    if handle.is_live() {
        handle.inner.deliberate.store(true, Ordering::SeqCst);
    }

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
        kind: handle.kind(),
        message: error.to_string(),
    })?;

    // 直接の子の終了を、期限付きで待つ。届かなければ待ち続けない。
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

/// 実プロセスを起動し、読み取りと監視のスレッドを張る。
///
/// **整合性検査は起動の前**に行い、失敗したら spawn しない（要件 5.3）。これにより
/// 「検査を通ってから起動する」順序が、呼び出し側の規約ではなくこの 1 箇所の構造で保証される。
/// 標準出力・標準エラーはパイプで作り、**必ずこの場で取り出して読み取りスレッドへ引き渡す** —
/// 取り出さずに放置すると、子がパイプの容量（Linux で 64 KiB）を超えて書いた時点で子が
/// ブロックする。標準入力もパイプとし、[`SidecarHandle::take_stdin`] から利用側が取り出す。
fn spawn(
    spec: &SidecarSpec,
    verify: &IntegrityVerifier,
    subscribers: &Arc<Subscribers>,
) -> Result<SidecarHandle, SpawnError> {
    preflight(&spec.executable)?;
    verify(spec.kind, &spec.executable).map_err(|source| SpawnError::IntegrityMismatch {
        path: spec.executable.clone(),
        source,
    })?;

    let mut command = Command::new(&spec.executable);
    command
        .args(&spec.args)
        .stdin(Stdio::piped())
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

    // 終了を待つ経路を先に作る。ここで失敗したら、読み取りを張る前に子を始末する
    // （パイプを開いたまま放置しない）。
    let waiter = match platform::Waiter::new(&child) {
        Ok(waiter) => waiter,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SpawnError::Spawn {
                message: format!("終了を待つ経路を作れない: {error}"),
            });
        }
    };

    let pid = child.id();
    let inner = Arc::new(SidecarInner {
        kind: spec.kind,
        pid,
        child: Mutex::new(child),
        platform: Arc::new(group),
        exit: Mutex::new(ExitState::Alive),
        deliberate: AtomicBool::new(false),
        subscribers: Arc::clone(subscribers),
    });

    // パイプはここで取り出す。取り出した分だけが読み取りスレッドに渡り、取り残しは無い。
    let (stdout, stderr) = {
        let mut child = inner.lock();
        (child.stdout.take(), child.stderr.take())
    };

    let mut readers: Vec<JoinHandle<()>> = Vec::with_capacity(2);
    if let Some(stdout) = stdout {
        readers.push(spawn_reader(
            Arc::clone(&inner),
            SidecarStream::Stdout,
            stdout,
        ));
    }
    if let Some(stderr) = stderr {
        readers.push(spawn_reader(
            Arc::clone(&inner),
            SidecarStream::Stderr,
            stderr,
        ));
    }
    spawn_monitor(Arc::clone(&inner), waiter, readers);

    Ok(SidecarHandle { inner })
}

/// パイプ 1 本を行単位で読み、購読側へ配るスレッドを張る。
///
/// このスレッドは **panic しない**。読み取りの失敗・行の変換の失敗はすべて読み取りの終端として
/// 扱う（要件 5.7: アプリケーション本体を巻き込まない）。子の死でパイプが閉じれば EOF で終わる。
fn spawn_reader<R>(inner: Arc<SidecarInner>, stream: SidecarStream, pipe: R) -> JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::with_capacity(READER_BUFFER_BYTES, pipe);
        // 読み取りの失敗は読み取りの終端として扱う（ここで panic するとアプリケーション本体を
        // 巻き込む。要件 5.7）。
        let _ = read_lines(&mut reader, MAX_LINE_BYTES, &mut |fragment, _terminated| {
            // 断片か 1 行かは `SidecarEvent` のフィールドでは区別しない（design のフィールド集合を
            // 変えない）。8.1 へ渡す契約は `SidecarEvent::Output` の doc を参照。
            let text = String::from_utf8_lossy(fragment).into_owned();
            inner.subscribers.publish(SidecarEvent::Output {
                kind: inner.kind,
                stream,
                line: text,
            });
        });
    })
}

/// 子の終了を待ち、**読み取りが終わってから**終了通知を配るスレッドを張る。
///
/// 待機はブロックする（ポーリングしない）。終了の観測後、読み取りスレッドを join してから
/// [`SidecarEvent::Exited`] を配る。この join が「通知はその子の最後の出力の後に届く」を
/// タイミングに依存せず保証する。
fn spawn_monitor(inner: Arc<SidecarInner>, waiter: platform::Waiter, readers: Vec<JoinHandle<()>>) {
    thread::spawn(move || {
        // 子が終了するまで眠る。待機中に CPU を使わない。
        let status = waiter.wait().ok();

        // 終了した事実を先に記録する。読み取りの完了を待たないので、未読の出力が残っていても
        // `get` / `ensure` / `terminate` は生存判定を更新できる（要件 5.8）。
        inner.mark_exited(status);

        // **読み取りスレッドを join してから通知する。** パイプに未読の行が残っていても、
        // それがすべて配られてから終了通知が出る。読み取りスレッドは panic しない作りだが、
        // join の失敗も握りつぶして通知そのものは必ず出す（要件 5.7 の通知は、読み取りの
        // 失敗より優先される）。
        for reader in readers {
            let _ = reader.join();
        }

        let status = if inner.deliberate.load(Ordering::SeqCst) {
            SidecarExit::Deliberate { status }
        } else {
            SidecarExit::Unexpected { status }
        };
        inner.subscribers.publish(SidecarEvent::Exited {
            kind: inner.kind,
            status,
        });
    });
}

/// 内容バイトを現在の断片へ加える。断片が上限に達していれば、先に出してから加える。
///
/// 上限に達した時点では出さず、**次の内容バイトが来た時点で**出す。行がちょうど上限で終わる
/// 場合、その断片は終端された 1 断片として出したいからである（先に出すと、行末が空の断片として
/// 二重に届く）。
fn push_content(
    fragment: &mut Vec<u8>,
    byte: u8,
    cap: usize,
    emit: &mut impl FnMut(&[u8], bool),
) {
    if fragment.len() >= cap {
        emit(fragment, false);
        fragment.clear();
    }
    fragment.push(byte);
}

/// 行の断片を順に `emit(fragment, terminated)` へ渡す。行の区切り方の唯一の実装であり、
/// **`BufRead::lines` と完全に同じ行を返す**（上限で断片に分かれる点だけが異なる）。
///
/// - `fragment` は行の内容の断片（改行も CRLF の `\r` も含まない）。`terminated` はその断片で
///   行が終わったか（改行まで届いたか）を表す。
/// - `cap` を超える行は `cap` ごとの断片に分ける。上限で切られた断片は `terminated == false`
///   であり、利用側は終端された断片までを連結して元の 1 行を得る。上限ちょうどの長さの行は
///   1 つの終端された断片になる。
/// - CRLF は「`\r` の直後が `\n` のとき、その 1 つだけを落とす」— `lines` と同じ規則である。
///   このため `\r` は**内容か行末かが確定するまで退避**し（[`read_lines`] の `held_cr`）、
///   断片へは内容と確定した `\r` だけを入れる。したがって「断片の末尾の `\r` を落とす」という
///   二重に効きうる処理は存在しない。`\r\r\n` の内容 `\r` も、上限の境界をまたぐ `\r\n` も、
///   裸の `\r` も同じ規則で扱われる。
/// - 改行で終わらない最終内容は、そのまま（末尾の `\r` も含めて）`terminated == false` で届く。
///
/// 上限は、改行を書かない子が読み取り側のメモリを無制限に消費するのを防ぐためにある。
/// 上限ちょうどの断片は、次のバイトが内容と確定するまで配送されない（**ちょうど上限まで書いて
/// 止まった子では、その断片の配送が次の 1 バイトか EOF まで遅れる**。取りこぼしはしない）。
fn read_lines<R: BufRead>(
    reader: &mut R,
    cap: usize,
    emit: &mut impl FnMut(&[u8], bool),
) -> io::Result<()> {
    let mut fragment: Vec<u8> = Vec::new();
    // 読んだが「内容の `\r`」か「CRLF の `\r`」かが決まっていないバイト（最大 1 バイト）。
    // 次が `\n` なら行末の `\r` であり落とす。それ以外（内容・もう 1 つの `\r`・EOF）なら
    // 内容なので、そのときに断片へ入れる。
    let mut held_cr = false;

    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            // EOF。退避した `\r` は内容である。
            if held_cr {
                push_content(&mut fragment, b'\r', cap, emit);
            }
            if !fragment.is_empty() {
                emit(&fragment, false);
            }
            return Ok(());
        }

        // 退避した `\r` を、次のバイトで確定させる。`\n` なら行末の `\r`（落とす）、
        // それ以外なら内容（`\r` の直後に内容が続く場合と、`\r` がもう 1 つ続く場合を含む）。
        if held_cr {
            held_cr = false;
            if available[0] == b'\n' {
                reader.consume(1);
                emit(&fragment, true);
                fragment.clear();
                continue;
            }
            push_content(&mut fragment, b'\r', cap, emit);
        }

        // `\r` / `\n` の手前までを一気に取り込む（速い経路）。上限を超える分は取り込まない。
        let run = available
            .iter()
            .position(|byte| *byte == b'\r' || *byte == b'\n')
            .unwrap_or(available.len());
        let take = run.min(cap.saturating_sub(fragment.len()));
        // `available` の借用はここで終える（この後の `consume` と両立させる）。
        let special = if run < available.len() {
            Some(available[run])
        } else {
            None
        };
        if take > 0 {
            fragment.extend_from_slice(&available[..take]);
            reader.consume(take);
        }
        if take < run {
            // 上限に達し、内容がまだ続いている。断片を出して続きを読む。
            emit(&fragment, false);
            fragment.clear();
            continue;
        }
        let Some(byte) = special else {
            // このバッファに `\r` / `\n` は無かった。続きを読む。
            continue;
        };
        reader.consume(1);
        match byte {
            b'\n' => {
                // 行末。断片には行末の `\r` を入れていないので、ここで落とすものは無い。
                emit(&fragment, true);
                fragment.clear();
            }
            b'\r' => {
                // 内容か行末かは次のバイトで決まる。確定するまで退避する。
                held_cr = true;
            }
            _ => unreachable!("fill_buf の走査で `\\r` と `\\n` だけを拾っている"),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// 与えた大きさずつしか返さない `Read`。`BufReader` の内部バッファの境界が `\r\n` を
    /// またぐ状況を意図的に作るために使う（実パイプでも書き込みの分割次第で起こる）。
    struct ChunkedReader {
        data: Vec<u8>,
        position: usize,
        chunk: usize,
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let remaining = self.data.len() - self.position;
            let take = remaining.min(self.chunk).min(buf.len());
            buf[..take].copy_from_slice(&self.data[self.position..self.position + take]);
            self.position += take;
            Ok(take)
        }
    }

    /// `bytes` を上限 `cap` で読み、`(断片, 行末まで届いたか)` の列を返す。`chunk` は読み取りが
    /// 一度に受け取る最大バイト数（`usize::MAX` はバッファ任せ、1 は 1 バイトずつ）。
    fn fragments(bytes: &[u8], cap: usize, chunk: usize) -> Vec<(Vec<u8>, bool)> {
        let source = ChunkedReader {
            data: bytes.to_vec(),
            position: 0,
            chunk: chunk.max(1),
        };
        let mut reader = BufReader::with_capacity(cap, source);
        let mut collected = Vec::new();
        read_lines(&mut reader, cap, &mut |fragment, terminated| {
            collected.push((fragment.to_vec(), terminated));
        })
        .expect("メモリ上の読み取りは失敗しない");
        collected
    }

    /// 内容 `length` バイト（`x` の連続）+ 終端のストリームを読む。
    fn stream(length: usize, terminator: &[u8], cap: usize, chunk: usize) -> Vec<(Vec<u8>, bool)> {
        let mut bytes = vec![b'x'; length];
        bytes.extend_from_slice(terminator);
        fragments(&bytes, cap, chunk)
    }

    /// 断片列を契約どおりに読む（終端された断片までを連結して 1 行にする）。行末まで届かなかった
    /// 最終断片も 1 行として数える。
    fn lines(fragments: &[(Vec<u8>, bool)]) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current = Vec::new();
        for (fragment, terminated) in fragments {
            current.extend_from_slice(fragment);
            if *terminated {
                lines.push(String::from_utf8_lossy(&current).into_owned());
                current.clear();
            }
        }
        if !current.is_empty() {
            lines.push(String::from_utf8_lossy(&current).into_owned());
        }
        lines
    }

    /// 断片の長さと終端だけを取り出す（期待値を読みやすくする）。
    fn shape(fragments: &[(Vec<u8>, bool)]) -> Vec<(usize, bool)> {
        fragments
            .iter()
            .map(|(bytes, terminated)| (bytes.len(), *terminated))
            .collect()
    }

    /// 改行で行に分かれ、CRLF の `\r` は落ちる。空行は 1 行として残り、行の途中の `\r` は内容。
    #[test]
    fn lines_are_split_on_newlines_and_empty_lines_are_kept() {
        assert_eq!(lines(&fragments(b"a\r\nbb\n\n", 8, usize::MAX)), ["a", "bb", ""]);
        // 行の途中の `\r` は内容である（CRLF ではない）。
        assert_eq!(lines(&fragments(b"a\rb\n", 8, usize::MAX)), ["a\rb"]);
        // 改行で終わらない最終行も届く。
        assert_eq!(lines(&fragments(b"a\nbc", 8, usize::MAX)), ["a", "bc"]);
        // 空のストリームでは何も届かない。
        assert!(fragments(b"", 4, usize::MAX).is_empty());
    }

    /// LF 終端: 上限ちょうど・未満・超過で断片の分かれ方が正しい。
    #[test]
    fn lf_lines_at_and_around_the_cap_are_split_exactly() {
        for cap in [4usize, MAX_LINE_BYTES] {
            assert_eq!(shape(&stream(cap, b"\n", cap, usize::MAX)), [(cap, true)], "cap={cap}");
            assert_eq!(
                shape(&stream(cap - 1, b"\n", cap, usize::MAX)),
                [(cap - 1, true)],
                "cap={cap}"
            );
            assert_eq!(
                shape(&stream(cap + 1, b"\n", cap, usize::MAX)),
                [(cap, false), (1, true)],
                "cap={cap}"
            );
            assert_eq!(lines(&stream(cap + 1, b"\n", cap, usize::MAX)), ["x".repeat(cap + 1)]);
        }
    }

    /// CRLF 終端: 上限ちょうど・未満・超過で `\r` が漏れず、余分な空断片も出ない。
    ///
    /// 上限ちょうどは「1 断片・終端」、上限未満で `\r` が上限の位置に来る場合は「上限-1 バイトの
    /// 1 断片・終端」、上限超過は「上限 + 1 バイト」に分かれる。
    #[test]
    fn crlf_lines_at_and_around_the_cap_are_trimmed_exactly() {
        for cap in [4usize, MAX_LINE_BYTES] {
            assert_eq!(
                shape(&stream(cap, b"\r\n", cap, usize::MAX)),
                [(cap, true)],
                "上限ちょうど + CRLF は 1 断片である（余分な空断片を出さない）: cap={cap}"
            );
            assert_eq!(
                shape(&stream(cap - 1, b"\r\n", cap, usize::MAX)),
                [(cap - 1, true)],
                "上限の位置に来た `\\r` は行末として落とす: cap={cap}"
            );
            assert_eq!(
                shape(&stream(cap + 1, b"\r\n", cap, usize::MAX)),
                [(cap, false), (1, true)],
                "cap={cap}"
            );
            assert_eq!(lines(&stream(cap, b"\r\n", cap, usize::MAX)), ["x".repeat(cap)]);
            assert_eq!(
                lines(&stream(cap + 1, b"\r\n", cap, usize::MAX)),
                ["x".repeat(cap + 1)]
            );
        }
    }

    /// 読み取りが細かく分割されても（`\r\n` が読み取りの境界をまたいでも）結果は同じである。
    /// 1 バイトずつ（最悪）、パイプ相当、実ツール相当の中間、まとめて、のすべてで
    /// 上限ちょうど・未満・超過が一致する。
    ///
    /// **これは分割への不変性だけを主張する。** 正しい行そのもの（`BufRead::lines` との一致）は
    /// [`fragments_reconstruct_to_bufread_lines_for_every_string`] が網羅的に固定する
    /// （この 2 つは別々の失敗クラスを捕まえる。分割の実装を壊すとこちらが、行の解釈を壊すと
    /// 参照等価テストが落ちる）。
    #[test]
    fn crlf_split_across_reads_behaves_like_a_single_chunk() {
        for cap in [4usize, MAX_LINE_BYTES] {
            for terminator in [b"\n".as_slice(), b"\r\n".as_slice()] {
                for length in [cap - 1, cap, cap + 1] {
                    let single = stream(length, terminator, cap, usize::MAX);
                    for chunk in [1usize, 3, 8 * 1024, usize::MAX] {
                        let split = stream(length, terminator, cap, chunk);
                        assert_eq!(
                            shape(&split),
                            shape(&single),
                            "読み取りの分割で断片が変わる: cap={cap} len={length} term={terminator:?} chunk={chunk}"
                        );
                        assert_eq!(
                            lines(&split),
                            lines(&single),
                            "読み取りの分割で内容が変わる: cap={cap} len={length} term={terminator:?} chunk={chunk}"
                        );
                    }
                }
            }
        }
    }

    /// 空行（LF のみ・CRLF のみ）は、空の終端された断片 1 つになる。
    #[test]
    fn a_lone_newline_is_an_empty_line() {
        assert_eq!(shape(&fragments(b"\n", 4, usize::MAX)), [(0, true)]);
        assert_eq!(shape(&fragments(b"\r\n", 4, usize::MAX)), [(0, true)]);
        assert_eq!(lines(&fragments(b"\r\n", 4, usize::MAX)), [""]);
    }

    /// `\n` を伴わない末尾の `\r` は内容として残る（`BufRead::lines` と同じ）。
    #[test]
    fn a_trailing_bare_cr_without_a_newline_is_content() {
        assert_eq!(lines(&fragments(b"abc\r", 8, usize::MAX)), ["abc\r"]);
        // 上限の境界に来た場合も落とさない（次の断片の内容になる）。
        for cap in [4usize, MAX_LINE_BYTES] {
            let fragments = stream(cap, b"\r", cap, 1);
            assert_eq!(
                shape(&fragments),
                [(cap, false), (1, false)],
                "cap={cap}"
            );
            assert_eq!(lines(&fragments), [format!("{}\r", "x".repeat(cap))], "cap={cap}");
        }
    }

    /// **参照等価の網羅検査**: `{x, \r, \n}` から作れる長さ 7 までのすべての文字列について、
    /// 断片列（終端フラグ込み）の復元結果が `std::io::BufRead::lines` の出力と**完全に一致**
    /// することを確かめる。上限 1〜8 と読み取りの分割（まとめて / 1 バイトずつ）の全組み合わせを
    /// 走査するので、`\r\r\n`、上限の前後での `\r\n`、裸の `\r`、改行の無い最終断片をすべて含む。
    ///
    /// 個別の事例ではなく**このクラス全体**を固定するための入口である。件数と不一致数を出力する
    /// （不一致 0 が合格条件）。
    #[test]
    fn fragments_reconstruct_to_bufread_lines_for_every_string() {
        const ALPHABET: [u8; 3] = [b'x', b'\r', b'\n'];
        const MAX_SYMBOLS: usize = 7;

        let mut cases = 0usize;
        let mut mismatches = 0usize;
        let mut preview: Vec<(Vec<u8>, usize, usize, Vec<String>, Vec<String>)> = Vec::new();
        for cap in 1..=8usize {
            for length in 0..=MAX_SYMBOLS {
                let total = ALPHABET.len().pow(length as u32);
                for value in 0..total {
                    let mut bytes = vec![b'x'; length];
                    let mut rest = value;
                    for slot in bytes.iter_mut() {
                        *slot = ALPHABET[rest % ALPHABET.len()];
                        rest /= ALPHABET.len();
                    }
                    let reference: Vec<String> =
                        std::io::BufReader::new(Cursor::new(bytes.clone()))
                            .lines()
                            .map(|line| line.expect("テスト用の入力は UTF-8 である"))
                            .collect();
                    for chunk in [usize::MAX, 1] {
                        cases += 1;
                        let got = lines(&fragments(&bytes, cap, chunk));
                        if got != reference {
                            mismatches += 1;
                            if preview.len() < 5 {
                                preview.push((bytes.clone(), cap, chunk, got, reference.clone()));
                            }
                        }
                    }
                }
            }
        }

        assert!(cases > 10_000, "走査した件数が少なすぎる: {cases}");
        assert_eq!(
            mismatches, 0,
            "BufRead::lines と一致しない入力が {mismatches} 件ある（{cases} 件中）。\
             最初の不一致（入力, cap, chunk, 実際, 参照）: {preview:?}"
        );
        eprintln!(
            "参照等価: {cases} 件（cap 1..=8、長さ {MAX_SYMBOLS} までの `{{x, \\r, \\n}}`）で不一致 0"
        );
    }
}

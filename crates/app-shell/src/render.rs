//! 初回描画の監視とラスタライザの判定（要件 10.1、10.2。タスク 8.2）。
//!
//! 所有: `RenderWatchdog`（design.md「Components and Interfaces → Adapter Layer」）の
//! **判定と記録の中核**。アダプタ層（`src-tauri/src/watchdog.rs`）がこの中核へ実時計・診断の
//! 記録先・設定の印を注入し、期限を見張るスレッドと、不成立のときの利用者への提示を担う。
//!
//! # なぜ中核がこのクレートにあるのか
//!
//! **描画の判定は GUI を起動せずに検証できなければならない**（tasks.md 8.2 の完了状態は
//! 「通知コマンドを呼ぶと描画成立が記録され、呼ばずに期限を超過させると不成立が記録される
//! ことが、実画面なしでテストできる」である）。時計（[`Clock`]）と記録先
//! （[`RenderRecorder`]）と印の置き場（[`FallbackMark`]）をすべて注入できるようにしてある
//! ので、テストは**実時間を待たず**、決定的に期限超過を再現できる。**判定の関数は
//! コマンドが呼ぶものと同じ 1 本である** — 検証専用の並行実装を持たない。
//!
//! # 三値の判定とその写像（要件 10.1、10.2）
//!
//! [`RenderVerdict`] は 3 つの値を取る。確定する経路は 2 つだけで、互いに排他である:
//!
//! | 経路 | 判定 | 根拠 |
//! |---|---|---|
//! | 通知が期限内に届き、ラスタライザが既知のソフトウェア実装に一致した | `SoftwareRaster` | フロントエンドが**描画フレームの中から**通知し、その申告したラスタライザがソフトウェア実装である |
//! | 通知が期限内に届き、一致しなかった（またはラスタライザを取得できなかった） | `Painted` | 描画フレームの中から通知が届いたこと自体が描画成立の証拠である。ラスタライザの文字列は補助的な情報にすぎない |
//! | 期限までに通知が届かなかった | `NoPaint` | **描画を検出する API は基盤側に存在しない**（design.md「RenderWatchdog」）。通知が届かないことが唯一の判別手段である |
//!
//! **タイムアウトとソフトウェアラスタライザは別の値である** — 前者は描画が成立しなかった
//! ことを意味し、後者は描画が成立したが低速な経路であることを意味する。したがって
//! [`RenderVerdict::NoPaint`] だけが 8.3 の印を立てる（design.md「RenderWatchdog」の
//! 「`NoPaint` を検出したら設定に印を残し」）。
//!
//! ラスタライザを取得できなかったときを `Painted` に倒すのは意図である。
//! `WEBGL_debug_renderer_info` は環境によって無効化されうる（プライバシー保護のための
//! 無効化や、WebGL 文脈自体が作れない環境）。取得できないことと描画が成立していないことは
//! 別であり、**取得できないことを不成立として扱うと、正常な環境で印を立ててしまう**。
//!
//! # 期限の値と起動予算との関係（要件 1.3、10.3）
//!
//! [`FIRST_PAINT_DEADLINE`] は**ウィンドウの生成から**測って 3 秒である。起動予算は
//! 「起動から操作可能なウィンドウが表示されるまで 2 秒」（要件 1.3、tasks.md 10.3）であり、
//! **ウィンドウの生成は起動の途中に起きる**ので、予算を使い切る環境でも通知は期限より十分
//! 前に届く（予算いっぱいの環境で少なくとも 1 秒の余裕がある）。期限を予算よりずっと大きく
//! 取る（例: 30 秒）と、**無内容の画面がその時間だけ残る**ことになり、要件 10.2 の
//! 「無内容の画面を提示したまま留まらず」を満たせない。逆に予算より短く取ると、遅い環境で
//! 正常な描画を不成立と誤判定して印を立ててしまう。
//!
//! **誤判定の代償は 1 回の起動に限られる。** 印は次の起動で 8.3 が代替経路を適用した後、
//! **通常の描画経路が成立したこと**（[`RenderWatchdog`] が `Painted` を確定した時点）で
//! 下ろされる。したがって誤って印を立てても、その次の起動は代替経路で始まり、描画が成立
//! すればその次の起動からは適用されない。
//!
//! # 8.3 が読む印のインタフェースと、その状態機械（要件 10.3）
//!
//! - **鍵**: [`SettingsKey::RenderFallback`]（ファイル上の名前は `render.fallback`）。真偽値。
//! - **意味**: `true` なら「直近に確定的に観測した描画の試行が `NoPaint` であった」。
//! - **書く側**: [`RenderWatchdog`] だけである。**8.3 は印を書かない**（書き手が 2 つに
//!   なると、描画の成立を観測していない段で印を消しうる）。
//! - **読む側**: タスク 8.3 が起動時（`tauri::Builder` を組み立てる前）に
//!   [`render_fallback_pending`] で読む。
//!
//! ## 判定 → 印の写像（**8.3 の申し送りで確定した規則**）
//!
//! | 確定した判定 | 印 | 根拠 |
//! |---|---|---|
//! | [`RenderVerdict::NoPaint`] | `true` を書く | 描画が成立しなかった。次回は代替経路を試す |
//! | [`RenderVerdict::Painted`] | `false` を書く | **通常の描画経路が成立した。**印を下ろす |
//! | [`RenderVerdict::SoftwareRaster`] | **動かさない** | 下の「振動の回避」を参照 |
//!
//! 同値なら書かない。**未設定のまま `false` を書くこともない**（正常な環境の設定ファイルに
//! 無意味な鍵を作らない）。
//!
//! ## 振動の回避（タスク 8.3 が確定させた規則。**旧: `SoftwareRaster` で `false` を書いた**）
//!
//! `SoftwareRaster` は「描画は成立したが、低速な経路（ソフトウェア実装）である」ことを
//! 意味する。**この判定で印を下ろすと恒久的な振動になる**: 代替経路を適用した起動が
//! ソフトウェア経路で描画に成功する → 印が下りる → 次の起動は代替経路を適用しない →
//! 描画が再び不成立になる → 印が立つ → 次の起動は代替経路を適用する、を繰り返す。
//! すなわち**成立の原因が代替経路である場合に、それを 1 回おきに捨ててしまう**。
//!
//! したがって写像を **`Painted` だけが印を下ろす**ように定めた。これは design.md
//! 「RenderWatchdog」の「`Painted` が観測できたら印を消す」に一致する。`Painted` は
//! **通常の描画経路（ハードウェア加速を含む経路）が成立した**ことを意味するので、それを
//! 観測できたときにだけ「もう代替経路は要らない」と結論できる。
//!
//! ### 残る仮定（代替経路の選択と表裏である）
//!
//! `Painted` を確定した起動が**代替経路を適用していた**場合、その成功が代替経路のおかげで
//! ある可能性は排除できない。規則はそれでも印を下ろす（design.md「RenderWatchdog」の
//! 「`Painted` が観測できたら印を消す」）。
//!
//! **この残余は 1 回の遅れでは済まない。**採用中の回避策の下で `Painted`（通常の描画経路）が
//! 観測される場合 — このホストで実測した 1 例では `WEBKIT_DISABLE_DMABUF_RENDERER=1` の下でも
//! ラスタライザは通常の実装のままだった — は、**適用 → `Painted` で印が下りる → 次の起動は
//! 適用せず `NoPaint` → 印が立つ → また適用**、という**起動ごとの交互の振動**になる。
//! `SoftwareRaster` を下ろさない規則が防ぐのは、**回避策の下でソフトウェア描画に落ちる**型の
//! 振動だけである。
//!
//! これは設計が印を 1 つの真偽値に固定し、消去を `Painted` の観測に結びつけた帰結である
//! （要件 10.3 の「成立が観測された次の起動では適用されない」をそのまま満たす）。振動を
//! 消すには「代替経路を適用した起動での成功」と「適用しない起動での成功」を区別する状態が
//! 要り、設計の変更になる。**現状は設計どおりに実装し、残余は design.md の
//! 「Revalidation Triggers」に記録して後続スペックの判断に委ねる。**
//!
//! # 記録の内容（要件 8.1、8.4、10.2）
//!
//! [`RenderRecorder`] が受け取るのはウィンドウのラベル・判定・ラスタライザの文字列・
//! **実際に描画されていた画面の識別子**・経過ミリ秒である。**ラスタライザの文字列も画面の
//! 識別子も環境の情報であって利用者の内容ではない**ので、4.4 の秘匿
//! （[`crate::diagnostics::Redacted`]）の対象ではない。ドキュメントの内容がこの経路に載る
//! ことはない（セル値もスキーマもここへ入ってくる経路が無い）。**画面の識別子を載せるのは、
//! 3 OS の描画確認（tasks.md 10.4）が「初回描画が成立したこと」と「どの画面が描画されたか」を
//! 同じ 1 つの記録から判定できるようにするためである**（要求した識別子ではなく、通知の時点で
//! 領域が表示していた識別子である）。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::ipc::{RenderVerdict, WindowLabel};
use crate::settings::{FileSettingsStore, SettingsKey, SettingsStore};

// ---------------------------------------------------------------------------
// 期限（要件 10.1、10.2、10.3）
// ---------------------------------------------------------------------------

/// 初回描画の通知を待つ期限。**ウィンドウの生成から**測る。
///
/// 3 秒。起動予算（要件 1.3: 起動から操作可能なウィンドウまで 2 秒）より 1 秒だけ大きい。
/// 値の根拠と、予算との関係、誤判定の代償はモジュール doc を参照。**期限をこれより大きく
/// 広げる変更は、無内容の画面が残る時間をそのまま伸ばす**（要件 10.2）。
pub const FIRST_PAINT_DEADLINE: Duration = Duration::from_secs(3);

/// 起動予算（要件 1.3）。[`FIRST_PAINT_DEADLINE`] との関係をテストで固定するために置く。
///
/// tasks.md 10.3 が計測する区間は「起動から操作可能なウィンドウが表示されるまで」であり、
/// 本モジュールの期限は「ウィンドウの生成から」である。ウィンドウの生成は起動の途中に起きる
/// ため、**予算を使い切る環境でも通知は期限より前に届く**。
pub const STARTUP_BUDGET: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// 注入する部品（実時間に依存しないための入口）
// ---------------------------------------------------------------------------

/// 単調に増加するミリ秒時刻の読み取り。**実時間に依存しない検証のための入口である。**
pub trait Clock: Send + Sync {
    /// 任意の起点からの経過ミリ秒。単調であることだけを要求する。
    fn now_millis(&self) -> u64;
}

/// 実時間の時計（`Instant` の起点からの経過）。
///
/// アダプタ層はこれを使い、テストは手で進められる時計を使う（[`RenderWatchdog::new`] の
/// 引数が唯一の差し替え点である）。
pub struct MonotonicClock {
    origin: Instant,
}

impl MonotonicClock {
    /// この瞬間を起点として作る。
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    fn now_millis(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }
}

/// 判定を診断へ記録する先（要件 8.1、10.2）。
///
/// 中核は記録の**形式と保存先を知らない**（Tauri にも記録機構にも依存しない）。実装は
/// アダプタ層にあり、対象名 `jxcel::render` で記録する（8.1 の `jxcel::sidecar` と同じ規約）。
pub trait RenderRecorder: Send + Sync {
    /// 確定した判定を記録する。
    fn record(&self, record: &VerdictRecord);

    /// 8.3 の印を書けなかったことを記録する。**判定そのものは失わない**（診断には残る）。
    fn mark_failed(&self, label: &WindowLabel, detail: &str);
}

/// 8.3 が読む印の置き場（[`SettingsKey::RenderFallback`]）。
///
/// `true` は「直近に確定した判定が `NoPaint` であった」を意味する。読み書きの入口だけを
/// 定義し、保存先の実装は [`SettingsFallbackMark`] が持つ。
pub trait FallbackMark: Send + Sync {
    /// 現在の値。未設定なら `None`。
    fn current(&self) -> Option<bool>;

    /// 値を書く。失敗は人が読める 1 行で返す（**パニックしない**）。
    fn set(&self, pending: bool) -> Result<(), String>;
}

/// 設定ストアの [`SettingsKey::RenderFallback`] を印として使う実装。
pub struct SettingsFallbackMark {
    settings: Arc<FileSettingsStore>,
}

impl SettingsFallbackMark {
    /// 共有の設定実体を印の置き場として使う（要件 7.3 の同一実体）。
    pub fn new(settings: Arc<FileSettingsStore>) -> Self {
        Self { settings }
    }
}

impl FallbackMark for SettingsFallbackMark {
    fn current(&self) -> Option<bool> {
        self.settings.get::<bool>(&SettingsKey::RenderFallback)
    }

    fn set(&self, pending: bool) -> Result<(), String> {
        self.settings
            .set(&SettingsKey::RenderFallback, &pending)
            .map_err(|error| error.to_string())
    }
}

/// タスク 8.3 が起動時に読む印（要件 10.3）。`true` なら代替経路を適用する。
///
/// 設定ストアから直接読むだけの入口である。**8.3 はこの関数だけを使い、印を書かない**
/// （書き手は [`RenderWatchdog`] の 1 つに限る。モジュール doc「8.3 が読む印の
/// インタフェース」）。
pub fn render_fallback_pending(settings: &FileSettingsStore) -> bool {
    settings
        .get::<bool>(&SettingsKey::RenderFallback)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// 判定の材料（要件 10.2）
// ---------------------------------------------------------------------------

/// ソフトウェアラスタライザを示す既知の目印（小文字で比較する）。
///
/// ラスタライザの文字列は環境ごとに異なり、**網羅的な一覧は存在しない**。したがってここは
/// 「ソフトウェア実装だと分かっている綴り」だけを持ち、一致しなければハードウェア加速
/// （または判別不能）として扱う。判定の結論に効くのは「一致したかどうか」だけであり、
/// 一致しなければ [`RenderVerdict::Painted`] に倒れる（描画フレームから通知が届いている
/// こと自体が描画成立の証拠である。モジュール doc を参照）。
///
/// **新しい環境でソフトウェア経路が観測されたら、この一覧を伸ばす。** 伸ばし忘れても
/// 誤って `Painted` と記録されるだけで、描画そのものの判定（`NoPaint`）は変わらない。
pub const SOFTWARE_RENDERER_MARKERS: &[&str] = &[
    // Mesa のソフトウェア実装（Linux で最も一般的である）。
    "llvmpipe",
    "softpipe",
    "lavapipe",
    "mesa offscreen",
    "osmesa",
    // Chromium / WebKit が GPU 無しで使う実装（Windows の ANGLE 経由を含む）。
    "swiftshader",
    // Windows のソフトウェア実装（D3D11 WARP を含む）。
    "microsoft basic render driver",
    // macOS のソフトウェア実装。
    "apple software renderer",
    // 一般的な綴り（上記に当てはまらない実装の受け皿）。
    "software rasterizer",
    "software renderer",
];

/// 与えられたラスタライザの文字列が、既知のソフトウェア実装に一致するか。
///
/// 比較は大文字小文字を畳んで行う（`ANGLE (Google, Vulkan … SwiftShader …)` のような
/// 包み込みの綴りも部分一致で拾う）。空・空白のみは `false` である（判別不能を
/// ソフトウェアと断定しない）。
pub fn is_software_rasterizer(renderer: &str) -> bool {
    let folded = renderer.trim().to_ascii_lowercase();
    if folded.is_empty() {
        return false;
    }
    SOFTWARE_RENDERER_MARKERS
        .iter()
        .any(|marker| folded.contains(marker))
}

/// 期限内に届いた通知から判定を決める（要件 10.1、10.2）。**純粋関数。**
///
/// ラスタライザが既知のソフトウェア実装に一致すれば [`RenderVerdict::SoftwareRaster`]、
/// それ以外（一致しない・取得できなかった）は [`RenderVerdict::Painted`] である。
/// [`RenderVerdict::NoPaint`] はこの関数からは決して返らない — それは通知が届かなかった
/// ことだけを根拠とする値であり、[`RenderWatchdog::expire_due`] だけが確定する。
pub fn verdict_for_report(renderer: Option<&str>) -> RenderVerdict {
    match renderer {
        Some(value) if is_software_rasterizer(value) => RenderVerdict::SoftwareRaster,
        _ => RenderVerdict::Painted,
    }
}

/// 確定した判定 1 件。記録と提示の両方の材料になる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictRecord {
    /// 判定の対象のウィンドウ（6.1 のラベル規約）。
    pub label: WindowLabel,
    /// 確定した判定。
    pub verdict: RenderVerdict,
    /// 通知が運んだラスタライザの文字列（期限超過では `None`）。
    pub renderer: Option<String>,
    /// 通知が運んだ**実際に描画されていた画面の識別子**（期限超過と未報告では `None`）。
    ///
    /// 3 OS の描画確認（tasks.md 10.4）が「どの画面が描画されたか」を記録から読むために
    /// 載せる。**要求した識別子ではない** — アダプタが記録する行に出し、CI の段は
    /// 「描画は成立したが画面が違う」を成立として扱わない（8.2 の完了状態と 10.4 の
    /// 完了状態を同じ 1 つの記録で判定できるようにする）。
    pub screen: Option<String>,
    /// 監視の開始から判定までの経過ミリ秒。
    pub elapsed_millis: u64,
}

/// 通知の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyOutcome {
    /// 初回の通知で判定が確定した（記録した）。
    Decided(RenderVerdict),
    /// 既に確定していた通知。**記録も印も動かさず、確定済みの判定をそのまま返す。**
    ///
    /// この腕へ来るのは次の 2 つである:
    ///
    /// - **2 回目以降の通知**（React の StrictMode や再描画で届く）。**判定は最初の描画の
    ///   観測であって、後からの上書きは履歴を歪める**ため、初回の判定を変えない。
    /// - **期限超過のあとに届いた通知**（[`RenderWatchdog::expire_due`] が
    ///   [`RenderVerdict::NoPaint`] を確定した後）。この場合も**判定は `NoPaint` のまま
    ///   変えない**（期限の時点で描画が成立していなかった事実は動かない）が、画面は遅れて
    ///   使える状態になっているので、呼び出し側（アダプタの `watchdog::render_heartbeat`）が
    ///   **不成立の提示を取り下げる**。**この腕に来ることが、その取り下げの唯一の合図で
    ///   ある。**
    ///
    /// [`RenderWatchdog::expire_due`] は確定した項目を削除せず判定を書き込むだけなので、
    /// **期限超過の後の通知は [`NotifyOutcome::Unwatched`] ではなくこの腕になる。**
    AlreadyDecided(RenderVerdict),
    /// 監視していないウィンドウからの通知。**期限超過の後ではない。**
    ///
    /// 期限超過の後は監視の項目が判定つきで残るため [`NotifyOutcome::AlreadyDecided`] に
    /// なる。ここへ来るのは「そもそも監視が無い」場合だけである:
    ///
    /// - 破棄されて監視の取り消しが済んでいる（[`RenderWatchdog::forget`]。
    ///   `window::on_window_event` の `Destroyed` が呼ぶ。**取り消しは期限の前後に依らない**）。
    /// - 監視を始めていない（生成に失敗した、または生成の唯一の場所を通っていない）。
    Unwatched,
}

// ---------------------------------------------------------------------------
// 監視（要件 10.1、10.2）
// ---------------------------------------------------------------------------

/// 監視 1 件の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Watch {
    /// 監視を開始した時刻（ミリ秒）。
    started_millis: u64,
    /// 期限の時刻（ミリ秒）。
    deadline_millis: u64,
    /// 確定した判定（未確定なら `None`）。
    verdict: Option<RenderVerdict>,
}

/// ウィンドウごとの初回描画の監視。**判定と記録の唯一の実体である。**
///
/// アプリ全体で 1 実体だけ置き（`Manager::manage`）、ウィンドウのラベルを鍵にする
/// （**6.1 のラベルが身元であり、別の識別子を発明しない**）。生成の唯一の場所
/// （`src-tauri/src/window/mod.rs` の `build_window`）が [`start`](Self::start) を呼び、
/// 通知コマンドが [`notify`](Self::notify) を、期限を見張るスレッドが
/// [`expire_due`](Self::expire_due) を呼ぶ。
///
/// ロックは毒されても回復する（イベントループの中から呼ばれるため、ここで panic すると
/// 他のウィンドウを巻き込む。6.1 のレジストリと同じ方針）。
pub struct RenderWatchdog {
    clock: Arc<dyn Clock>,
    recorder: Arc<dyn RenderRecorder>,
    mark: Arc<dyn FallbackMark>,
    deadline: Duration,
    inner: Mutex<BTreeMap<String, Watch>>,
}

impl RenderWatchdog {
    /// 中核を組み立てる。**3 つの注入が検証の差し替え点である。**
    pub fn new(
        clock: Arc<dyn Clock>,
        recorder: Arc<dyn RenderRecorder>,
        mark: Arc<dyn FallbackMark>,
        deadline: Duration,
    ) -> Self {
        Self {
            clock,
            recorder,
            mark,
            deadline,
            inner: Mutex::new(BTreeMap::new()),
        }
    }

    /// 待つ期限（アダプタが記録に出すために読む）。
    pub fn deadline(&self) -> Duration {
        self.deadline
    }

    /// ロックを取る。毒されていても panic しない。
    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Watch>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// ウィンドウの初回描画の監視を開始する（要件 10.1、10.2）。
    ///
    /// **ウィンドウの生成が成功した直後に、生成の唯一の場所から呼ぶ。**同じラベルで
    /// 2 回呼ばれた場合は期限を測り直す（ラベルは一意であり、実際には起きない）。
    pub fn start(&self, label: &WindowLabel) {
        let now = self.clock.now_millis();
        let mut watches = self.lock();
        watches.insert(
            label.as_str().to_owned(),
            Watch {
                started_millis: now,
                deadline_millis: now.saturating_add(self.deadline.as_millis() as u64),
                verdict: None,
            },
        );
    }

    /// 描画フレームの中から届いた通知を処理する（要件 10.1、10.2）。
    ///
    /// **期限超過の後と、監視していないウィンドウは別の結果になる。**結果は 3 つである
    /// （[`NotifyOutcome`] の各腕を参照）:
    ///
    /// 1. **期限内（未確定）** — 判定を確定して記録し、[`NotifyOutcome::Decided`] を返す。
    /// 2. **確定済み** — 記録も印も動かさず、確定済みの判定を
    ///    [`NotifyOutcome::AlreadyDecided`] で返す。**期限超過の後に確定した `NoPaint` も
    ///    ここに含まれる**（[`RenderWatchdog::expire_due`] は項目を削除しない）。
    /// 3. **監視の項目が無い** — [`NotifyOutcome::Unwatched`] を返す。**期限超過の後はここへ
    ///    来ない**（項目が判定つきで残るため 2 になる）。来るのは監視を始めていないか、
    ///    破棄されて取り消し済みの場合だけである。
    ///
    /// **2 で [`RenderVerdict::NoPaint`] が返ったことは「画面が遅れて使える状態になった」を
    /// 意味する**ので、呼び出し側（アダプタの `watchdog::render_heartbeat`）は不成立の提示を
    /// 取り下げる。判定・記録・印を決めるのはこの中核であり、呼び出し側はその意味に従う。
    ///
    /// `screen` は通知を送ったフロントエンドが**実際に描画していた画面の識別子**である
    /// （[`VerdictRecord::screen`]）。判定には影響しないが、記録に載せて 3 OS の描画確認
    /// （tasks.md 10.4）が「どの画面が描画されたか」を読めるようにする。
    pub fn notify(
        &self,
        label: &WindowLabel,
        renderer: Option<&str>,
        screen: Option<&str>,
    ) -> NotifyOutcome {
        let now = self.clock.now_millis();
        let decided = {
            let mut watches = self.lock();
            let Some(watch) = watches.get_mut(label.as_str()) else {
                return NotifyOutcome::Unwatched;
            };
            if let Some(verdict) = watch.verdict {
                return NotifyOutcome::AlreadyDecided(verdict);
            }
            let verdict = verdict_for_report(renderer);
            watch.verdict = Some(verdict);
            VerdictRecord {
                label: label.clone(),
                verdict,
                renderer: renderer.map(str::to_owned),
                screen: screen.map(str::to_owned),
                elapsed_millis: now.saturating_sub(watch.started_millis),
            }
        };
        // **ロックを解放してから記録と印の書き込みを行う**（印はファイルへ書くため、監視表を
        // 保持したまま行うと他のウィンドウの通知を待たせる）。
        self.record_and_mark(&decided);
        NotifyOutcome::Decided(decided.verdict)
    }

    /// 期限を過ぎた監視を確定する（要件 10.2）。**通知が届かなかったことが唯一の根拠である。**
    ///
    /// 確定した監視ごとに [`RenderVerdict::NoPaint`] を記録し、8.3 の印を立てる。戻り値は
    /// 確定した記録の一覧であり、**呼び出し側（アダプタの期限の監視スレッド）が利用者への
    /// 提示に使う**（記録そのものは `recorder` が行う）。
    ///
    /// 期限を見張るスレッドから定期的に呼ばれる。**時計は注入されたものを使うので、テストは
    /// 実時間を待たずに同じ経路を通せる。**
    pub fn expire_due(&self) -> Vec<VerdictRecord> {
        let now = self.clock.now_millis();
        let expired: Vec<VerdictRecord> = {
            let mut watches = self.lock();
            let mut records = Vec::new();
            for (label, watch) in watches.iter_mut() {
                if watch.verdict.is_some() || now < watch.deadline_millis {
                    continue;
                }
                watch.verdict = Some(RenderVerdict::NoPaint);
                records.push(VerdictRecord {
                    label: WindowLabel::new(label.clone()),
                    verdict: RenderVerdict::NoPaint,
                    renderer: None,
                    // 通知が届いていないので、描画された画面の報告も無い。
                    screen: None,
                    elapsed_millis: now.saturating_sub(watch.started_millis),
                });
            }
            records
        };
        for record in &expired {
            self.record_and_mark(record);
        }
        expired
    }

    /// 監視を取り除く（ウィンドウの破棄の通知から呼ぶ）。
    ///
    /// **不成立として記録しない。** 破棄されたウィンドウは画面に残っていないので、
    /// 要件 10.2 の「無内容の画面のまま留まらせない」の対象ではなく、記録すると利用者が
    /// 閉じただけのウィンドウを失敗として数えてしまう。
    ///
    /// 戻り値は取り除けたかどうか（項目が無ければ `false`）。**判定が確定済みの監視も項目が
    /// 残っているため `true` になる**（[`RenderWatchdog::expire_due`] は項目を削除せず判定を
    /// 書き込むだけである）。`false` になるのは、監視を始めていないウィンドウか、既に
    /// 取り消し済みのウィンドウだけである。
    pub fn forget(&self, label: &WindowLabel) -> bool {
        self.lock().remove(label.as_str()).is_some()
    }

    /// 未確定の監視の数（**テストのため**。production の経路は期限の判定に使わない）。
    pub fn pending_count(&self) -> usize {
        self.lock()
            .values()
            .filter(|watch| watch.verdict.is_none())
            .count()
    }

    /// 判定を記録し、8.3 の印を更新する（要件 10.2、10.3）。
    fn record_and_mark(&self, record: &VerdictRecord) {
        self.recorder.record(record);
        if let Err(detail) = self.update_mark(record.verdict) {
            self.recorder.mark_failed(&record.label, &detail);
        }
    }

    /// 印を判定に合わせる（要件 10.3）。**既に同じ値なら書かない**（起動ごとの無駄な書き込みを
    /// 避け、未設定のまま `false` を書くこともしない）。失敗は人が読める 1 行で返す。
    ///
    /// 写像は [`RenderVerdict::NoPaint`] → `true`、[`RenderVerdict::Painted`] → `false`、
    /// [`RenderVerdict::SoftwareRaster`] → **動かさない**である。`SoftwareRaster` で下ろすと
    /// 代替経路が成立の原因である場合に 1 回おきにそれを捨てる恒久的な振動になる — 理由は
    /// モジュール doc「振動の回避」にある。
    fn update_mark(&self, verdict: RenderVerdict) -> Result<(), String> {
        let pending = match verdict {
            RenderVerdict::NoPaint => true,
            RenderVerdict::Painted => false,
            // 描画は成立している（低速なだけ）ので、印を**動かさない**。
            RenderVerdict::SoftwareRaster => return Ok(()),
        };
        let current = self.mark.current();
        if current == Some(pending) || (current.is_none() && !pending) {
            return Ok(());
        }
        self.mark.set(pending)
    }
}

// ---------------------------------------------------------------------------
// テスト（タスク 8.2 の完了状態: 実画面なしで判定と記録を固定する）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// 手で進める時計。**実時間を待たずに期限超過を再現するための唯一の道具である。**
    struct ManualClock {
        now: AtomicU64,
    }

    impl ManualClock {
        fn new() -> Self {
            Self {
                now: AtomicU64::new(0),
            }
        }

        fn advance(&self, millis: u64) {
            self.now.fetch_add(millis, Ordering::SeqCst);
        }
    }

    impl Clock for ManualClock {
        fn now_millis(&self) -> u64 {
            self.now.load(Ordering::SeqCst)
        }
    }

    /// 記録された判定を集める記録先（診断の代わり）。
    #[derive(Default)]
    struct CollectingRecorder {
        records: Mutex<Vec<VerdictRecord>>,
        mark_failures: Mutex<Vec<(String, String)>>,
    }

    impl CollectingRecorder {
        fn verdicts(&self) -> Vec<RenderVerdict> {
            self.records
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .map(|record| record.verdict)
                .collect()
        }

        fn records(&self) -> Vec<VerdictRecord> {
            self.records
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl RenderRecorder for CollectingRecorder {
        fn record(&self, record: &VerdictRecord) {
            self.records
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(record.clone());
        }

        fn mark_failed(&self, label: &WindowLabel, detail: &str) {
            self.mark_failures
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((label.as_str().to_owned(), detail.to_owned()));
        }
    }

    /// 印の置き場の代わり（設定ファイルを触らない）。書き込みの履歴も残す。
    #[derive(Default)]
    struct MemoryMark {
        value: Mutex<Option<bool>>,
        writes: AtomicU64,
    }

    impl FallbackMark for MemoryMark {
        fn current(&self) -> Option<bool> {
            *self.value.lock().unwrap_or_else(PoisonError::into_inner)
        }

        fn set(&self, pending: bool) -> Result<(), String> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            *self.value.lock().unwrap_or_else(PoisonError::into_inner) = Some(pending);
            Ok(())
        }
    }

    /// 失敗する印（書けなかったことを記録できることの検査に使う）。
    struct FailingMark;

    impl FallbackMark for FailingMark {
        fn current(&self) -> Option<bool> {
            None
        }

        fn set(&self, _pending: bool) -> Result<(), String> {
            Err("印を書けない".to_owned())
        }
    }

    /// 監視一式を組み立てる（実時間を使わない）。
    fn watch(
        deadline_millis: u64,
    ) -> (
        Arc<ManualClock>,
        Arc<CollectingRecorder>,
        Arc<MemoryMark>,
        RenderWatchdog,
    ) {
        let clock = Arc::new(ManualClock::new());
        let recorder = Arc::new(CollectingRecorder::default());
        let mark = Arc::new(MemoryMark::default());
        let watchdog = RenderWatchdog::new(
            clock.clone(),
            recorder.clone(),
            mark.clone(),
            Duration::from_millis(deadline_millis),
        );
        (clock, recorder, mark, watchdog)
    }

    fn label(name: &str) -> WindowLabel {
        WindowLabel::new(name)
    }

    // ------------------------------------------------------------------
    // 完了状態 (a): 通知 ⇒ 描画成立（Painted）が記録される
    // ------------------------------------------------------------------

    #[test]
    fn a_notification_records_the_painted_verdict() {
        let (_clock, recorder, mark, watchdog) = watch(3_000);
        watchdog.start(&label("empty-1"));
        assert_eq!(watchdog.pending_count(), 1);

        let outcome = watchdog.notify(&label("empty-1"), Some("ANGLE (NVIDIA GeForce RTX)"), None);
        assert_eq!(outcome, NotifyOutcome::Decided(RenderVerdict::Painted));
        assert_eq!(recorder.verdicts(), vec![RenderVerdict::Painted]);
        assert_eq!(watchdog.pending_count(), 0);
        // 描画が成立したので印は書かない（未設定のまま false を書かない）。
        assert_eq!(mark.current(), None);
        assert_eq!(mark.writes.load(Ordering::SeqCst), 0);
    }

    /// 記録には判定を識別できる材料（ラベル・ラスタライザ・**描画された画面**・経過）が載る
    /// （要件 10.2、tasks.md 10.4）。
    #[test]
    fn the_record_carries_what_identifies_the_verdict() {
        let (clock, recorder, _mark, watchdog) = watch(3_000);
        watchdog.start(&label("doc-7"));
        clock.advance(120);

        watchdog.notify(
            &label("doc-7"),
            Some("Mesa Intel(R) UHD Graphics 620"),
            Some("smoke-table"),
        );

        let records = recorder.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].label.as_str(), "doc-7");
        assert_eq!(records[0].elapsed_millis, 120);
        assert_eq!(
            records[0].renderer.as_deref(),
            Some("Mesa Intel(R) UHD Graphics 620")
        );
        // **要求した識別子ではなく、通知が報告した識別子がそのまま載る。**
        assert_eq!(records[0].screen.as_deref(), Some("smoke-table"));
    }

    /// 画面の報告が無い通知（領域を読めなかった・古いフロントエンド）と、通知そのものが
    /// 無い期限超過は、どちらも `screen` が `None` になる。**どちらも「どの画面が描画されたか」
    /// の証明にはならない**（10.4 の段はその場合に落ちる）。
    #[test]
    fn a_report_without_a_screen_leaves_the_screen_unknown() {
        let (clock, recorder, _mark, watchdog) = watch(10);
        watchdog.start(&label("empty-1"));
        watchdog.notify(&label("empty-1"), Some("Apple GPU"), None);
        // 期限超過の記録（通知が届いていない）。
        watchdog.start(&label("empty-2"));
        clock.advance(20);
        watchdog.expire_due();

        assert_eq!(
            recorder
                .records()
                .iter()
                .map(|record| record.screen.clone())
                .collect::<Vec<_>>(),
            vec![None, None]
        );
    }

    /// ラスタライザを取得できなくても、通知が届いた以上は描画成立である。
    #[test]
    fn a_notification_without_a_renderer_string_is_still_painted() {
        let (_clock, recorder, _mark, watchdog) = watch(3_000);
        watchdog.start(&label("empty-2"));

        assert_eq!(
            watchdog.notify(&label("empty-2"), None, None),
            NotifyOutcome::Decided(RenderVerdict::Painted)
        );
        assert_eq!(recorder.verdicts(), vec![RenderVerdict::Painted]);
    }

    // ------------------------------------------------------------------
    // 完了状態 (b): 通知せずに期限を超過 ⇒ 描画不成立（NoPaint）が記録される
    // ------------------------------------------------------------------

    #[test]
    fn a_missed_deadline_records_the_no_paint_verdict() {
        let (clock, recorder, mark, watchdog) = watch(3_000);
        watchdog.start(&label("empty-3"));

        // 期限の直前では何も確定しない（実時間は待たない）。
        clock.advance(2_999);
        assert!(watchdog.expire_due().is_empty());
        assert!(recorder.verdicts().is_empty());

        // 期限に達すると不成立が記録され、8.3 の印が立つ。
        clock.advance(1);
        let expired = watchdog.expire_due();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].label.as_str(), "empty-3");
        assert_eq!(expired[0].verdict, RenderVerdict::NoPaint);
        assert_eq!(expired[0].renderer, None);
        assert_eq!(expired[0].elapsed_millis, 3_000);
        assert_eq!(recorder.verdicts(), vec![RenderVerdict::NoPaint]);
        assert_eq!(mark.current(), Some(true));

        // 2 回目は何も確定しない（記録が重複しない）。
        assert!(watchdog.expire_due().is_empty());
        assert_eq!(recorder.verdicts(), vec![RenderVerdict::NoPaint]);
    }

    /// 期限超過そのものが「識別できる情報」であり、経過ミリ秒が記録に載る。
    #[test]
    fn the_timeout_record_carries_the_elapsed_time() {
        let (clock, recorder, _mark, watchdog) = watch(50);
        watchdog.start(&label("doc-1"));
        clock.advance(75);

        let expired = watchdog.expire_due();

        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].elapsed_millis, 75);
        assert_eq!(recorder.records()[0].elapsed_millis, 75);
    }

    /// 通知の後に期限が来ても、記録は通知の判定のままである（後から上書きしない）。
    #[test]
    fn a_notified_window_is_not_overwritten_by_later_expiry() {
        let (clock, _recorder, _mark, watchdog) = watch(100);
        watchdog.start(&label("empty-4"));
        watchdog.notify(&label("empty-4"), None, None);

        clock.advance(1_000);
        assert!(watchdog.expire_due().is_empty());
    }

    // ------------------------------------------------------------------
    // 完了状態 (c): ソフトウェアラスタライザの入力 ⇒ 第 3 の値
    // ------------------------------------------------------------------

    #[test]
    fn a_software_rasterizer_records_the_third_verdict() {
        let (_clock, recorder, mark, watchdog) = watch(3_000);
        watchdog.start(&label("empty-5"));

        let outcome = watchdog.notify(
            &label("empty-5"),
            Some("llvmpipe (LLVM 17.0.6, 256 bits)"),
            None,
        );

        assert_eq!(
            outcome,
            NotifyOutcome::Decided(RenderVerdict::SoftwareRaster)
        );
        assert_eq!(recorder.verdicts(), vec![RenderVerdict::SoftwareRaster]);
        // 描画は成立している（低速なだけ）ので、8.3 の印は立てない。
        assert_eq!(mark.current(), None);
    }

    /// ソフトウェア判定の目印（代表例）。大文字小文字と包み込みの綴りを畳む。
    #[test]
    fn the_software_markers_cover_the_known_implementations() {
        for renderer in [
            "llvmpipe (LLVM 17.0.6, 256 bits)",
            "LLVMPIPE",
            "softpipe",
            "ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device (Subzero) 5.0.0))",
            "Mesa OffScreen",
            "Microsoft Basic Render Driver",
            "Apple Software Renderer",
            "Mesa/X.org, Software Rasterizer",
        ] {
            assert!(
                is_software_rasterizer(renderer),
                "ソフトウェア実装として判定されるべき: {renderer}"
            );
        }
        for renderer in [
            "ANGLE (NVIDIA, NVIDIA GeForce RTX 4090 Direct3D11 vs_5_0 ps_5_0, D3D11)",
            "Mesa Intel(R) UHD Graphics 620 (KBL GT2)",
            "Apple M3 Pro",
            "",
            "   ",
        ] {
            assert!(
                !is_software_rasterizer(renderer),
                "ソフトウェア実装として判定されてはならない: {renderer}"
            );
        }
    }

    /// 三値が互いに区別できること（同じ入力から同じ値が出る）。
    #[test]
    fn the_three_verdicts_are_distinct() {
        assert_ne!(RenderVerdict::Painted, RenderVerdict::SoftwareRaster);
        assert_ne!(RenderVerdict::Painted, RenderVerdict::NoPaint);
        assert_ne!(RenderVerdict::SoftwareRaster, RenderVerdict::NoPaint);
        assert_eq!(verdict_for_report(None), RenderVerdict::Painted);
        assert_eq!(verdict_for_report(Some("")), RenderVerdict::Painted);
        assert_eq!(
            verdict_for_report(Some("llvmpipe")),
            RenderVerdict::SoftwareRaster
        );
    }

    // ------------------------------------------------------------------
    // 通知の重複・対象外・破棄
    // ------------------------------------------------------------------

    #[test]
    fn a_second_notification_keeps_the_first_verdict_and_records_nothing() {
        let (_clock, recorder, _mark, watchdog) = watch(3_000);
        watchdog.start(&label("empty-6"));
        watchdog.notify(&label("empty-6"), Some("llvmpipe"), None);

        let outcome = watchdog.notify(&label("empty-6"), None, None);

        assert_eq!(
            outcome,
            NotifyOutcome::AlreadyDecided(RenderVerdict::SoftwareRaster)
        );
        assert_eq!(recorder.verdicts(), vec![RenderVerdict::SoftwareRaster]);
    }

    #[test]
    fn a_notification_for_an_unknown_window_is_unwatched() {
        let (_clock, recorder, _mark, watchdog) = watch(3_000);

        assert_eq!(
            watchdog.notify(&label("empty-9"), None, None),
            NotifyOutcome::Unwatched
        );
        assert!(recorder.verdicts().is_empty());
    }

    /// 期限超過のあとの通知は `Unwatched` ではなく `AlreadyDecided(NoPaint)` になる。
    ///
    /// **確定を変えず、記録も増やさない**（`expire_due` は項目を削除しないため、後の通知は
    /// 監視表に見つかる）。アダプタ（`watchdog::render_heartbeat`）はこの結果を「画面が
    /// 遅れて使える状態になった」と解釈して**不成立の提示だけを取り下げる** — その対応付けは
    /// アダプタ側の `heartbeat_action` のテストが固定する。
    #[test]
    fn a_notification_after_the_deadline_keeps_the_no_paint_verdict() {
        let (clock, recorder, _mark, watchdog) = watch(10);
        watchdog.start(&label("empty-7"));
        clock.advance(20);
        watchdog.expire_due();

        let outcome = watchdog.notify(&label("empty-7"), Some("ANGLE (NVIDIA GeForce RTX)"), None);

        assert_eq!(
            outcome,
            NotifyOutcome::AlreadyDecided(RenderVerdict::NoPaint)
        );
        assert_eq!(recorder.verdicts(), vec![RenderVerdict::NoPaint]);
    }

    /// 確定済みの監視も破棄で取り除ける（`expire_due` は項目を削除しない）。
    #[test]
    fn a_decided_watch_is_still_forgettable() {
        let (clock, _recorder, _mark, watchdog) = watch(10);
        watchdog.start(&label("empty-10"));
        clock.advance(20);
        watchdog.expire_due();

        // 判定が確定していても項目は残っているので、破棄で取り除ける（`true`）。
        assert!(watchdog.forget(&label("empty-10")));
        // 2 度目は項目が無い（`false`）。
        assert!(!watchdog.forget(&label("empty-10")));
    }

    /// 期限より前に破棄されたウィンドウは不成立として記録しない。
    #[test]
    fn forgetting_a_window_before_the_deadline_records_nothing() {
        let (clock, recorder, mark, watchdog) = watch(10);
        watchdog.start(&label("empty-8"));

        assert!(watchdog.forget(&label("empty-8")));
        clock.advance(20);

        assert!(watchdog.expire_due().is_empty());
        assert!(recorder.verdicts().is_empty());
        assert_eq!(mark.current(), None);
        assert!(!watchdog.forget(&label("empty-8")));
    }

    // ------------------------------------------------------------------
    // 8.3 の印（要件 10.3 のインタフェース）
    // ------------------------------------------------------------------

    /// 判定が変われば印も変わり、同じ値なら書かない。
    #[test]
    fn the_mark_follows_the_latest_verdict_and_avoids_redundant_writes() {
        let (clock, _recorder, mark, watchdog) = watch(10);
        watchdog.start(&label("empty-1"));
        clock.advance(20);
        watchdog.expire_due();
        assert_eq!(mark.current(), Some(true));
        assert_eq!(mark.writes.load(Ordering::SeqCst), 1);

        // 次のウィンドウで描画が成立すれば印は下りる。
        watchdog.start(&label("empty-2"));
        watchdog.notify(&label("empty-2"), None, None);
        assert_eq!(mark.current(), Some(false));
        assert_eq!(mark.writes.load(Ordering::SeqCst), 2);

        // もう一枚描画が成立しても、同じ値なので書かない。
        watchdog.start(&label("empty-3"));
        watchdog.notify(&label("empty-3"), None, None);
        assert_eq!(mark.writes.load(Ordering::SeqCst), 2);
    }

    /// ソフトウェアラスタライザは印を動かさない（**振動の回避**。要件 10.3）。
    ///
    /// 代替経路を適用した起動がソフトウェア経路で描画に成功した場合、ここで印を下ろすと
    /// 次の起動が代替経路を捨て、描画が再び不成立になる — 1 回おきの恒久的な振動になる。
    #[test]
    fn a_software_rasterizer_keeps_the_mark_it_found() {
        let (clock, _recorder, mark, watchdog) = watch(10);
        watchdog.start(&label("empty-1"));
        clock.advance(20);
        watchdog.expire_due();
        assert_eq!(mark.current(), Some(true));
        assert_eq!(mark.writes.load(Ordering::SeqCst), 1);

        // 代替経路を適用した起動で描画が成立しても、ソフトウェア経路なら印は下ろさない。
        watchdog.start(&label("empty-2"));
        watchdog.notify(&label("empty-2"), Some("llvmpipe"), None);
        assert_eq!(mark.current(), Some(true));
        assert_eq!(
            mark.writes.load(Ordering::SeqCst),
            1,
            "書き込んではならない"
        );

        // 通常の描画経路が成立したときにだけ印が下りる。
        watchdog.start(&label("empty-3"));
        watchdog.notify(&label("empty-3"), Some("Apple GPU"), None);
        assert_eq!(mark.current(), Some(false));
        assert_eq!(mark.writes.load(Ordering::SeqCst), 2);
    }

    /// 印を書けなくても判定は失われず、その事実が記録される。
    #[test]
    fn a_failed_mark_write_is_recorded_without_losing_the_verdict() {
        let clock = Arc::new(ManualClock::new());
        let recorder = Arc::new(CollectingRecorder::default());
        let watchdog = RenderWatchdog::new(
            clock.clone(),
            recorder.clone(),
            Arc::new(FailingMark),
            Duration::from_millis(10),
        );
        watchdog.start(&label("empty-1"));
        clock.advance(20);

        let expired = watchdog.expire_due();

        assert_eq!(expired.len(), 1);
        assert_eq!(recorder.verdicts(), vec![RenderVerdict::NoPaint]);
        let failures = recorder
            .mark_failures
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, "empty-1");
    }

    /// 実機の設定ストアを印として使えること（8.3 が読む鍵に本当に書かれる）。
    #[test]
    fn the_real_settings_store_holds_the_mark_8_3_reads() {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "jxcel-render-mark-{}-{sequence}",
            std::process::id()
        ));
        let (store, _report) = crate::settings::open(&directory).expect("設定ストアを開ける");
        assert!(!render_fallback_pending(&store));

        let clock = Arc::new(ManualClock::new());
        let recorder = Arc::new(CollectingRecorder::default());
        let watchdog = RenderWatchdog::new(
            clock.clone(),
            recorder,
            Arc::new(SettingsFallbackMark::new(Arc::clone(&store))),
            Duration::from_millis(10),
        );
        watchdog.start(&label("empty-1"));
        clock.advance(20);
        watchdog.expire_due();

        assert!(render_fallback_pending(&store));

        // 描画が成立すれば下りる。
        watchdog.start(&label("empty-2"));
        watchdog.notify(&label("empty-2"), None, None);
        assert!(!render_fallback_pending(&store));

        let _ = std::fs::remove_dir_all(&directory);
    }

    // ------------------------------------------------------------------
    // 期限と起動予算の関係（要件 1.3、10.3）
    // ------------------------------------------------------------------

    /// 期限は起動予算より大きい（予算内の描画を不成立と誤判定しない）。
    #[test]
    fn the_deadline_leaves_room_above_the_startup_budget() {
        assert!(
            FIRST_PAINT_DEADLINE > STARTUP_BUDGET,
            "期限 {FIRST_PAINT_DEADLINE:?} は起動予算 {STARTUP_BUDGET:?} より大きくなければならない"
        );
    }

    /// 期限は予算から 1 秒以内の余裕に留める（大きく広げると無内容の画面が残る）。
    #[test]
    fn the_deadline_stays_close_to_the_startup_budget() {
        assert!(
            FIRST_PAINT_DEADLINE - STARTUP_BUDGET <= Duration::from_secs(1),
            "期限 {FIRST_PAINT_DEADLINE:?} が起動予算 {STARTUP_BUDGET:?} から離れすぎている"
        );
    }
}

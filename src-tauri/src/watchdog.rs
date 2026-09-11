//! 初回描画の監視とラスタライザの判定（タスク 8.2。要件 10.1、10.2）。
//!
//! 所有: `RenderWatchdog`（design.md「Components and Interfaces → Adapter Layer」）の
//! **アダプタ側**。判定と記録の中核は Tauri 非依存の [`app_shell::render`] にあり、本モジュールは
//! そこへ実時計・診断の記録先・設定の印を注入し、次の 3 つを担う:
//!
//! 1. **期限の監視スレッド**（[`start_deadline_watch`]）。一定周期で中核の
//!    [`RenderWatchdog::expire_due`] を呼び、期限を過ぎた監視を確定させる。**ウィンドウごとに
//!    スレッドを立てない**（ウィンドウの数だけ資源を消費するため）。1 本のスレッドが全ウィンドウを
//!    見る。中核が時計を注入で受けるので、テストはこのスレッドも実時間も必要としない。
//! 2. **通知コマンド** [`render_heartbeat`]。フロントエンドが**描画フレームの中から**呼ぶ
//!    （送信側は `src/shell/renderHeartbeat.ts`。9.7 のスモーク画面も同じ送信側を使う）。
//!    呼び出し元ウィンドウは Tauri が注入する [`WebviewWindow`] から取り、**payload からは
//!    受け取らない**（偽装できない。要件 4.6、tasks.md 7.1）。
//! 3. **無内容の画面のまま留まらせない提示**（[`present_missing_paint`]）。期限を超過した
//!    ウィンドウの**ネイティブの題名**を変え、DOM へも注意書きを差し込む。
//!
//! # 期限（要件 10.1、10.2、10.3）
//!
//! 期限は中核の [`app_shell::render::FIRST_PAINT_DEADLINE`]（**ウィンドウの生成から 3 秒**）
//! である。起動予算（要件 1.3: 起動から操作可能なウィンドウまで 2 秒）との関係と、誤判定の
//! 代償は中核のモジュール doc に書いてある。**ウィンドウ生成の唯一の場所**
//! （`window::build_window`）が [`start_watch`] を呼ぶので、起動時・二重起動の引き継ぎ・
//! Dock クリックのどの経路で開かれたウィンドウにも監視が付く。破棄は
//! `window::on_window_event` が [`forget_watch`] で取り消す（**破棄されたウィンドウを不成立と
//! して記録しない** — 画面に残っていないため要件 10.2 の対象ではない）。
//!
//! # 三値の判定（要件 10.1、10.2）
//!
//! 判定は [`app_shell::render`] が行い、境界の型は [`RenderVerdict`] である。**タイムアウト
//! （[`RenderVerdict::NoPaint`]）とソフトウェアラスタライザ（[`RenderVerdict::SoftwareRaster`]）
//! は別の値である** — 前者は描画の不成立、後者は描画の成立（低速な経路）である。写像の根拠は
//! 中核のモジュール doc にある。**ソフトウェアラスタライザの判定だけがこのクレートの外へ出ない
//! 理由**: 判定は環境に依存しない純粋な関数であり（文字列の照合）、GUI を起動せずに検証できる
//! 必要があるため、Tauri 非依存の中核に置いた。**8.3 の印を立てるのは `NoPaint` だけである。**
//!
//! # 記録（要件 8.1、8.4、10.2）
//!
//! 記録の対象名は `jxcel::render` である（8.1 の `jxcel::sidecar` と同じ規約）。記録先は 5.2 が
//! 登録した記録機構であり、本モジュールは行を出すだけである。不成立は **`error`** で、
//! ラベル・期限・経過ミリ秒・診断情報の保存先を含む（要件 10.2 の「識別できる情報」）。
//! ラスタライザの文字列は環境の情報であってドキュメントの内容ではないので、4.4 の秘匿の対象
//! ではない（中核のモジュール doc）。
//!
//! # 8.3 が読む印（要件 10.3）
//!
//! 設定 [`SettingsKey::RenderFallback`]（`render.fallback`、真偽値）である。**書くのは中核の
//! [`RenderWatchdog`] だけ** — `NoPaint` を確定したときに `true`、`Painted` /
//! `SoftwareRaster` を確定したときに `false`（同じ値なら書かない）。8.3 は起動時
//! （`tauri::Builder` を組み立てる前）に [`app_shell::render::render_fallback_pending`] で読み、
//! **印を書かない**。詳しくは中核のモジュール doc「8.3 が読む印のインタフェース」。
//!
//! # 提示がアプリ自身の資産に依存しない理由（要件 10.2）
//!
//! **検出している失敗は「資産が読み込まれず無内容の画面になる」ことである。** したがって
//! 提示をアプリのバンドル（`dist/` の JS や CSS）に依存させてはならない。本モジュールは
//! 2 つを併用する:
//!
//! - **ネイティブの題名**（[`missing_paint_title`]）。OS が描くので WebView の描画が壊れて
//!   いても見える。ウィンドウマネージャのツリー（`xwininfo`）からも観測できる。
//! - **DOM への注意書き**（[`notice_script`]）。`document.createElement` とインラインの
//!   スタイルだけで組み立て、**アプリの CSS も JS バンドルも使わない**。資産の読み込みに
//!   失敗していても、WebView のページ文書さえあれば表示できる。
//!
//! どちらも失敗を記録するだけで例外を外へ出さず、待機もしない（`set_title` / `eval` は
//! イベントループへ処理を渡して戻る）。利用者が観測するものは、**題名の変わったウィンドウ**と
//! （描画基盤が生きていれば）赤い面の注意書き、そして記録に残った 1 行の `error` である。
//! アプリは終了しない（要件 5.4 の精神。他のウィンドウの動作も中断させない）。

use std::sync::Arc;
use std::time::Duration;

use app_shell::ipc::{
    IpcError, IpcResult, RenderHeartbeatRequest, RenderHeartbeatResponse, RenderVerdict,
    WindowContext, WindowLabel,
};
use app_shell::render::{
    MonotonicClock, NotifyOutcome, RenderRecorder, RenderWatchdog, SettingsFallbackMark,
    VerdictRecord, FIRST_PAINT_DEADLINE,
};
use app_shell::settings::{FileSettingsStore, SettingsKey};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_log::log;

/// 診断の対象名。8.1 が補助プロセスの出力に使う `jxcel::sidecar` と同じ規約である。
pub const LOG_TARGET: &str = "jxcel::render";

/// 期限を過ぎた監視を検出する周期。
///
/// 200 ms ごとに中核の [`RenderWatchdog::expire_due`] を呼ぶ。**期限そのものはウィンドウの
/// 生成時刻から測る**ので、この周期は検出の遅れの上限（最悪 200 ms）でしかない。1 本の
/// スレッドが全ウィンドウを見るため、ウィンドウが増えても本数は増えない。
const DEADLINE_POLL_INTERVAL: Duration = Duration::from_millis(200);

// ---------------------------------------------------------------------------
// 起動時の組み立てとウィンドウへの結線（要件 10.1、10.2）
// ---------------------------------------------------------------------------

/// アプリ全体で 1 実体の監視を組み立てる（起動時に 1 回。`lifecycle::run` が `Builder::build`
/// の前に管理状態として置く）。
///
/// **`Builder::build` より前に登録する。**ウィンドウの生成は構築後に起きるので、生成の
/// 唯一の場所（[`start_watch`]）が管理状態を見つけられない瞬間を作らない。時計・記録先・印の
/// どれも `AppHandle` を要さない（提示は期限の監視スレッドが行う）ため、構築の前に組める。
pub fn render_watch(settings: &Arc<FileSettingsStore>) -> RenderWatchdog {
    RenderWatchdog::new(
        Arc::new(MonotonicClock::new()),
        Arc::new(DiagnosticsRecorder),
        Arc::new(SettingsFallbackMark::new(Arc::clone(settings))),
        FIRST_PAINT_DEADLINE,
    )
}

/// 期限を見張るスレッドを開始する（**構築の後に 1 回だけ**呼ぶ）。
///
/// 中核の [`RenderWatchdog::expire_due`] を周期ごとに呼び、確定した不成立を
/// [`present_missing_paint`] で提示する。**記録そのものは中核が行う**（`expire_due` の戻り値は
/// 提示のための材料である）。
///
/// スレッドはプロセスの寿命と同じだけ生きる。終了処理を持たないのは、アプリの終了とともに
/// プロセスが終わり、このスレッドが残る経路が無いためである。**パニックしない** —
/// 呼ぶのは毒を回復する中核の関数と、失敗を記録するだけの提示である。
pub fn start_deadline_watch(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(DEADLINE_POLL_INTERVAL);
        let expired = app.state::<RenderWatchdog>().expire_due();
        for record in expired {
            if record.verdict == RenderVerdict::NoPaint {
                present_missing_paint(&app, &record);
            }
        }
    });
}

/// ウィンドウの初回描画の監視を開始する（要件 10.1、10.2）。
///
/// **ウィンドウの生成が成功した直後に、生成の唯一の場所（`window::build_window`）から呼ぶ。**
/// 生成に失敗したウィンドウには監視を付けない（失敗は 6.1 が既に報告しており、画面は存在
/// しないため要件 10.2 の「無内容の画面」に当たらない）。
pub fn start_watch<R: Runtime>(app: &AppHandle<R>, label: &WindowLabel) {
    let watch = app.state::<RenderWatchdog>();
    watch.start(label);
    log::debug!(
        target: LOG_TARGET,
        "初回描画の監視を開始した: label={} 期限={} ms",
        label.as_str(),
        watch.deadline().as_millis(),
    );
}

/// ウィンドウの監視を取り消す（**破棄の通知**から呼ぶ）。
///
/// 期限より前に閉じられたウィンドウを不成立として記録しないための入口である。取り消しの
/// 成否は記録に残さない（監視を始めていないウィンドウでは `false` になる。**判定が確定済みの
/// 監視でも項目は残っているため `true` になる** — 中核の
/// [`RenderWatchdog::expire_due`] は項目を削除せず判定を書き込むだけである）。
pub fn forget_watch<R: Runtime>(app: &AppHandle<R>, label: &WindowLabel) {
    app.state::<RenderWatchdog>().forget(label);
}

// ---------------------------------------------------------------------------
// 通知コマンド（要件 10.1、10.2、4.6）
// ---------------------------------------------------------------------------

/// 描画フレームの中から届いた通知を処理する（要件 10.1、10.2）。
///
/// フロントエンド（`src/shell/renderHeartbeat.ts`）が**マウント後の描画フレームの中から**
/// 呼ぶ。手順は 3 つである:
///
/// 1. 呼び出し元ウィンドウを Tauri の注入から得て、境界の文脈 [`WindowContext`] へ写す
///    （要件 4.6）。**payload からウィンドウを受け取らない**（偽装できない）。
/// 2. 中核の [`RenderWatchdog::notify`] に判定させる（**時計・記録先・印は中核が持つ**）。
/// 3. 確定した判定を封筒で返す（**通知の結果 3 つをそのまま写す**。下の節）。
///
/// # 通知の結果 3 つ（**期限超過の後と、監視していないウィンドウは別の腕である**）
///
/// 中核の [`app_shell::render::NotifyOutcome`] の意味論をそのまま写す。1 と 2 は封筒の
/// 成功腕（`Ok { verdict }`）で返る — **腕を分けているのは、フロントエンドが判定を読める
/// ようにするためと、アダプタが取る行動を決めるためである**:
///
/// 1. **初回（期限内・未確定）** — 判定を確定して記録し、`Ok` で返す。
/// 2. **確定済み**（[`NotifyOutcome::AlreadyDecided`]）— 記録も印も動かさず、確定済みの
///    判定を `Ok` で返す。この腕は 2 つの場合を覆う:
///    - **2 回目以降の通知**（React の StrictMode・再描画）。最初に確定した判定をそのまま
///      返し、何も変えない。
///    - **期限超過のあとに届いた通知**（判定は [`RenderVerdict::NoPaint`]）。判定は
///      `NoPaint` のまま変えない（期限の時点で描画が成立していなかった事実は動かない）が、
///      画面は遅れて使える状態になっているので、**不成立の提示だけを取り下げる**
///      （[`withdraw_missing_paint`]）。8.3 の印は `NoPaint` のままである。
/// 3. **監視の項目が無い**（[`NotifyOutcome::Unwatched`]）— 封筒の失敗腕
///    （[`IpcError::Window`]）で返し、理由にラベルを含める。**この腕に来るのは、監視を
///    始めていないか、破棄されて取り消し済みのウィンドウだけである**（期限超過の後は監視の
///    項目が判定つきで残るため 2 になる）。**成功として握り潰さない**のは、フロントエンドが
///    「通知が届いたのに記録されていない」状態を検出できるようにするためである（記録の
///    取りこぼしは要件 10.2 の判別手段そのものを壊す）。
///
/// **どの腕でどの行動を取るかは [`heartbeat_action`] が決める**（GUI を取らない純粋な部分。
/// `WebviewWindow` を要するコマンド本体は単体化できないため、契約はそこで固定する）。
///
/// # 検証専用の抑止（既定のビルドには存在しない）
///
/// `verification-triggers` feature が有効なビルドでは、環境変数
/// [`SUPPRESS_HEARTBEAT_ENV`] でこの通知を抑止できる。**期限超過の経路（要件 10.2 の核心）を
/// 実際の画面で実測するための唯一の入口である** — フロントエンドを書き換えずに「通知が届か
/// ない」状態を作れる。既定のビルドには環境変数の読み取り自体が無い（tasks.md 5.4 の
/// 申し送りを受け、7.4 と同じく非既定の feature で括る）。
#[tauri::command]
pub fn render_heartbeat(
    watch: State<'_, RenderWatchdog>,
    window: WebviewWindow,
    request: RenderHeartbeatRequest,
) -> IpcResult<RenderHeartbeatResponse, IpcError> {
    let label = WindowLabel::new(window.label().to_owned());
    #[cfg(feature = "verification-triggers")]
    if heartbeat_is_suppressed() {
        log::info!(
            target: LOG_TARGET,
            "[検証] 描画の通知を抑止した（期限超過の経路を実測する）: label={}",
            label.as_str(),
        );
        return IpcResult::Err {
            error: IpcError::Window {
                message: format!("検証用に {SUPPRESS_HEARTBEAT_ENV} で通知を抑止している"),
            },
        };
    }

    let outcome = watch.notify(&label, request.renderer.as_deref());
    let (verdict, withdraw) = match heartbeat_action(outcome) {
        HeartbeatAction::Reply { verdict, withdraw } => (verdict, withdraw),
        HeartbeatAction::Unwatched => {
            log::warn!(
                target: LOG_TARGET,
                "監視していないウィンドウから描画の通知が届いた: label={}",
                label.as_str(),
            );
            return IpcResult::Err {
                error: IpcError::Window {
                    message: format!(
                        "このウィンドウ（{}）の初回描画は監視されていない（監視の対象外、または期限より前に破棄された）",
                        label.as_str(),
                    ),
                },
            };
        }
    };

    // 期限超過のあとに届いた通知は「描画が期限より遅れて成立した」ことを意味する。判定は
    // [`RenderVerdict::NoPaint`] のまま変えない（期限の時点で描画は成立していなかった
    // という事実は動かない）が、**提示を残すと復帰した画面を覆い続ける**。したがって
    // 提示だけを取り下げる（8.3 の印は `NoPaint` のままである — 起動が期限を超えた事実は
    // 残す）。
    if withdraw {
        log::warn!(
            target: LOG_TARGET,
            "期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label={}",
            label.as_str(),
        );
        withdraw_missing_paint(&window);
    }

    IpcResult::Ok {
        data: RenderHeartbeatResponse {
            context: WindowContext { window: label },
            verdict,
        },
    }
}

/// 中核の通知の結果から、アダプタが取る行動を決める。**GUI を取らない純粋な部分である。**
///
/// コマンド本体（[`render_heartbeat`]）は [`WebviewWindow`] を取るため単体化できない
/// （tasks.md 7.6 のテスト基盤の制約）。したがって**「期限超過のあとの通知では判定を変えず
/// に提示だけを取り下げる」という契約はここで固定する** — 本体はこの関数の結論に従うだけで
/// ある。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeartbeatAction {
    /// 封筒の成功腕（`Ok { verdict }`）で返す。`withdraw` が真なら不成立の提示を取り下げる。
    Reply {
        /// 返す判定（確定済みなら最初に確定した値）。
        verdict: RenderVerdict,
        /// 不成立の提示を取り下げるか（**期限超過のあとの通知だけが真**）。
        withdraw: bool,
    },
    /// 封筒の失敗腕（[`IpcError::Window`]）で返す（監視の項目が無いウィンドウ）。
    Unwatched,
}

/// [`HeartbeatAction`] への写像。**中核の [`NotifyOutcome`] の 3 つの腕をそのまま写す。**
fn heartbeat_action(outcome: NotifyOutcome) -> HeartbeatAction {
    match outcome {
        NotifyOutcome::Decided(verdict) => HeartbeatAction::Reply {
            verdict,
            withdraw: false,
        },
        // **期限超過のあとに確定した `NoPaint` だけが取り下げを伴う。**それ以外の確定済みの
        // 判定（`Painted` / `SoftwareRaster`）は 2 回目以降の通知であり、提示は動かさない。
        NotifyOutcome::AlreadyDecided(verdict) => HeartbeatAction::Reply {
            verdict,
            withdraw: verdict == RenderVerdict::NoPaint,
        },
        NotifyOutcome::Unwatched => HeartbeatAction::Unwatched,
    }
}

// ---------------------------------------------------------------------------
// 診断への記録（要件 8.1、8.4、10.2）
// ---------------------------------------------------------------------------

/// 中核の記録先を記録機構（5.2 が登録したもの）へつなぐ実装。
///
/// **中核は記録の形式を知らない。**ここが唯一の対応付けであり、対象名 `jxcel::render` を
/// 付けて行を出す。レベルは判定の意味に合わせる:
///
/// - [`RenderVerdict::Painted`] → `info`（正常）
/// - [`RenderVerdict::SoftwareRaster`] → `warn`（描画は成立したが低速な経路である）
/// - [`RenderVerdict::NoPaint`] → `error`（**描画が成立しなかった**。要件 10.2 の失敗）
pub struct DiagnosticsRecorder;

impl RenderRecorder for DiagnosticsRecorder {
    fn record(&self, record: &VerdictRecord) {
        match record.verdict {
            RenderVerdict::Painted => log::info!(
                target: LOG_TARGET,
                "初回描画が成立した: label={} 経過={} ms ラスタライザ={}",
                record.label.as_str(),
                record.elapsed_millis,
                renderer_text(record.renderer.as_deref()),
            ),
            RenderVerdict::SoftwareRaster => log::warn!(
                target: LOG_TARGET,
                "初回描画は成立したがソフトウェアラスタライザ経由である: label={} 経過={} ms ラスタライザ={}",
                record.label.as_str(),
                record.elapsed_millis,
                renderer_text(record.renderer.as_deref()),
            ),
            RenderVerdict::NoPaint => log::error!(
                target: LOG_TARGET,
                "初回描画が成立しなかった: label={} 期限={} ms 経過={} ms 診断情報の保存先={}",
                record.label.as_str(),
                FIRST_PAINT_DEADLINE.as_millis(),
                record.elapsed_millis,
                log_location(),
            ),
        }
    }

    fn mark_failed(&self, label: &WindowLabel, detail: &str) {
        log::warn!(
            target: LOG_TARGET,
            "描画不成立の印（{}）を書けなかった: label={} 詳細={}",
            SettingsKey::RenderFallback.as_str(),
            label.as_str(),
            detail,
        );
    }
}

/// 記録に出すラスタライザの表現（未取得を空文字で表さない）。
fn renderer_text(renderer: Option<&str>) -> &str {
    renderer.unwrap_or("(取得できなかった)")
}

/// 診断情報の保存先（4.4 の方針が解決する場所）。解決できなければその事実を返す。
fn log_location() -> String {
    match app_shell::diagnostics::log_dir() {
        Ok(directory) => directory.display().to_string(),
        Err(error) => format!("(解決できない: {error})"),
    }
}

// ---------------------------------------------------------------------------
// 無内容の画面のまま留まらせない提示（要件 10.2）
// ---------------------------------------------------------------------------

/// 期限を超過したウィンドウに、描画が成立しなかったことを提示する。
///
/// **ネイティブの題名**と**DOM の注意書き**の両方を出す。理由と、資産に依存しない設計は
/// モジュール doc「提示がアプリ自身の資産に依存しない理由」にある。どちらの失敗も記録する
/// だけで、例外を外へ出さず、待機もしない（**アプリを終了させない**。要件 5.4 の精神）。
///
/// ウィンドウが既に無い場合は何もしない（破棄の通知との競合であり、異常ではない）。
fn present_missing_paint<R: Runtime>(app: &AppHandle<R>, record: &VerdictRecord) {
    let Some(window) = app.get_webview_window(record.label.as_str()) else {
        log::warn!(
            target: LOG_TARGET,
            "描画不成立を提示できなかった（ウィンドウが既に無い）: label={}",
            record.label.as_str(),
        );
        return;
    };

    let title = missing_paint_title(record);
    if let Err(error) = window.set_title(&title) {
        log::warn!(
            target: LOG_TARGET,
            "描画不成立の題名を設定できなかった: label={} {error}",
            record.label.as_str(),
        );
    }

    let script = notice_script(&title, &missing_paint_notice(record));
    if let Err(error) = window.eval(&script) {
        log::warn!(
            target: LOG_TARGET,
            "描画不成立の注意書きを差し込めなかった: label={} {error}",
            record.label.as_str(),
        );
    }
}

/// 描画不成立の提示を取り下げる（**期限超過のあとに通知が届いたとき**）。
///
/// 判定そのものは [`RenderVerdict::NoPaint`] のままだが、**復帰した画面を赤い面で覆い続け
/// ない**。題名を元へ戻し、差し込んだ注意書きを取り除く。どちらの失敗も記録するだけで、
/// 例外を外へ出さない。
///
/// この経路が要るのは、期限（3 秒）を超えてから描画が成立する場合があるためである — 遅い
/// 環境では期限が早すぎただけで、画面は使える状態になる。提示を残すと、その画面が利用者から
/// 見えなくなる（3 OS の描画確認＝10.4 も、覆われた画面を「描けていない」と判定しかねない）。
fn withdraw_missing_paint<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    if let Err(error) = window.set_title(crate::window::WINDOW_TITLE) {
        log::warn!(
            target: LOG_TARGET,
            "描画不成立の題名を元へ戻せなかった: label={} {error}",
            window.label(),
        );
    }
    if let Err(error) = window.eval(withdraw_script()) {
        log::warn!(
            target: LOG_TARGET,
            "描画不成立の注意書きを取り除けなかった: label={} {error}",
            window.label(),
        );
    }
}

/// 描画不成立のときのウィンドウの題名（1 行）。
///
/// **利用者が最初に見る情報である。**何が起きたか・どのウィンドウか・診断情報がどこにあるかを
/// 含める（要件 10.2 の「識別できる情報」）。
fn missing_paint_title(record: &VerdictRecord) -> String {
    format!(
        "jxcel — 描画が成立しませんでした（{} ミリ秒待機 / {}） / 診断情報: {}",
        record.elapsed_millis,
        record.label.as_str(),
        log_location(),
    )
}

/// DOM へ差し込む注意書き（複数行）。題名と同じ情報を、改行を保った形で運ぶ。
///
/// **この時点で真であることだけを述べる。** 代替経路の適用はタスク 8.3 が実装するため、
/// 「次回の起動では描画の代替経路を試みます」とは書かない（未実装の機能を利用者へ約束すると、
/// 提示している情報そのものが誤りになる。8.2 の時点で真なのは「不成立を記録した」ことと
/// 「アプリは動作を続ける」ことである）。**8.3 が適用を実装したら、その事実を述べる行を
/// 足してよい**（そのときは真になる）。
fn missing_paint_notice(record: &VerdictRecord) -> String {
    format!(
        "jxcel: 初回描画が成立しませんでした。\n\
         {} ミリ秒待っても、描画フレームの中から通知が届きませんでした。\n\
         対象のウィンドウ: {}\n\
         診断情報の保存先: {}\n\
         この事実は診断情報に記録し、アプリはこのまま動作を続けます。",
        record.elapsed_millis,
        record.label.as_str(),
        log_location(),
    )
}

/// 差し込む注意書きの要素の `id`。
///
/// **差し込みと取り下げの両方がこの定数を使う**（同じ綴りを 2 箇所に持たない）。要素を引く
/// 鍵であると同時に、同じウィンドウへ 2 度差し込まれても増やさないための目印である。
const NOTICE_ELEMENT_ID: &str = "jxcel-render-warning";

/// 注意書きを差し込む JavaScript。**アプリの資産に依存しない。**
///
/// `document.createElement` とインラインのスタイルだけで要素を作る（アプリのバンドルも CSS も
/// 参照しない）。`try` / `catch` で包み、**表示できない環境でもアプリを止めない**。同じ
/// ウィンドウへ 2 度差し込まれても要素を増やさない（`id` で引く）。
///
/// # スタイルは CSSOM で当てる（`setAttribute("style", …)` を使わない）
///
/// 本アプリの通信内容保護方針は `style-src 'self'` であり、**`style` 属性の設定は遮断される**
/// （インラインのスタイル属性は `'unsafe-inline'` か nonce を要求する）。CSSOM
/// （`element.style.…`）への代入はこの制限の対象外である — React の `style={{…}}` が
/// そのまま効いているのと同じ理由である（5.3 の review で実測）。したがって注意書きの見た目は
/// `element.style.…` で組み立てる。**`'unsafe-inline'` を CSP へ足して直してはならない**
/// （5.3 が明示的に外した）。
fn notice_script(title: &str, notice: &str) -> String {
    let title = js_string_literal(title);
    let notice = js_string_literal(notice);
    let mut script = String::new();
    script.push_str("(function () {\n");
    script.push_str("  try {\n");
    script.push_str("    document.title = ");
    script.push_str(&title);
    script.push_str(";\n");
    script.push_str("    var notice = ");
    script.push_str(&notice);
    script.push_str(";\n");
    script.push_str("    var element = document.getElementById(");
    script.push_str(&js_string_literal(NOTICE_ELEMENT_ID));
    script.push_str(");\n");
    script.push_str("    if (!element) {\n");
    script.push_str("      element = document.createElement(\"pre\");\n");
    script.push_str("      element.id = ");
    script.push_str(&js_string_literal(NOTICE_ELEMENT_ID));
    script.push_str(";\n");
    script.push_str("      element.style.position = \"fixed\";\n");
    script.push_str("      element.style.top = \"0\";\n");
    script.push_str("      element.style.left = \"0\";\n");
    script.push_str("      element.style.right = \"0\";\n");
    script.push_str("      element.style.bottom = \"0\";\n");
    script.push_str("      element.style.zIndex = \"2147483647\";\n");
    script.push_str("      element.style.margin = \"0\";\n");
    script.push_str("      element.style.padding = \"2rem\";\n");
    script.push_str("      element.style.overflow = \"auto\";\n");
    script.push_str("      element.style.backgroundColor = \"#b00020\";\n");
    script.push_str("      element.style.color = \"#ffffff\";\n");
    script.push_str("      element.style.font = \"14px/1.6 monospace\";\n");
    script.push_str("      element.style.whiteSpace = \"pre-wrap\";\n");
    script.push_str("      document.body.appendChild(element);\n");
    script.push_str("    }\n");
    script.push_str("    element.textContent = notice;\n");
    script.push_str("  } catch (error) {\n");
    script
        .push_str("    // 表示できない環境でもアプリを止めない（ネイティブの題名は設定済み）。\n");
    script.push_str("  }\n");
    script.push_str("})();\n");
    script
}

/// 差し込んだ注意書きを取り除く JavaScript。
///
/// [`notice_script`] と同じく `try` / `catch` で包み、**アプリの資産を参照しない**。要素は
/// [`NOTICE_ELEMENT_ID`] で引き、無ければ何もしない（期限超過のあとに初めて通知が届いた場合
/// でも安全である）。
fn withdraw_script() -> String {
    let mut script = String::new();
    script.push_str("(function () {\n");
    script.push_str("  try {\n");
    script.push_str("    document.title = ");
    script.push_str(&js_string_literal(crate::window::WINDOW_TITLE));
    script.push_str(";\n");
    script.push_str("    var element = document.getElementById(");
    script.push_str(&js_string_literal(NOTICE_ELEMENT_ID));
    script.push_str(");\n");
    script.push_str("    if (element && element.parentNode) {\n");
    script.push_str("      element.parentNode.removeChild(element);\n");
    script.push_str("    }\n");
    script.push_str("  } catch (error) {\n");
    script.push_str("    // 取り除けない環境でもアプリを止めない。\n");
    script.push_str("  }\n");
    script.push_str("})();\n");
    script
}

/// Rust の文字列を JavaScript の文字列リテラルへ写す。
///
/// ユーザーの環境によってはパスに引用符やバックスラッシュが含まれうるので、**文字列をそのまま
/// 連結しない**。制御文字と、JavaScript が行区切りとして扱う `U+2028` / `U+2029` も逃がす
/// （後者は ES2019 より前の環境で文字列リテラルを壊す）。依存を増やさないため、`serde_json`
/// は使わずここで綴る。
fn js_string_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// 検証専用の抑止（既定のビルドには存在しない）
// ---------------------------------------------------------------------------

/// 検証専用: 描画の通知を抑止して期限超過の経路を実測する環境変数の名前。
///
/// 値が空でも `"0"` でもなければ抑止する。**既定のビルドにはこの定数も読み取りも存在しない**
/// （`verification-triggers` feature。7.4 が確立した片付けの規約）。
#[cfg(feature = "verification-triggers")]
const SUPPRESS_HEARTBEAT_ENV: &str = "JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT";

/// [`SUPPRESS_HEARTBEAT_ENV`] が抑止を指示しているか。
#[cfg(feature = "verification-triggers")]
fn heartbeat_is_suppressed() -> bool {
    match std::env::var(SUPPRESS_HEARTBEAT_ENV) {
        Ok(value) => !value.is_empty() && value != "0",
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// テスト（GUI を必要としない部分だけを固定する。tasks.md の検証手順を参照）
//
// 判定と記録の経路そのものは Tauri 非依存の中核（crates/app-shell/src/render.rs）が持ち、
// 実画面なしの `cargo test -p app-shell` が固定する。ここはアダプタに固有の純粋な部分
// （提示の文言と、JavaScript へ埋め込む文字列の逃がし方）を固定する。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn record(renderer: Option<&str>) -> VerdictRecord {
        VerdictRecord {
            label: WindowLabel::new("empty-1"),
            verdict: RenderVerdict::NoPaint,
            renderer: renderer.map(str::to_owned),
            elapsed_millis: 3_000,
        }
    }

    /// 題名は、識別できる情報（経過・対象・診断の保存先）を含む（要件 10.2）。
    #[test]
    fn the_title_identifies_the_window_and_the_diagnostics() {
        let title = missing_paint_title(&record(None));

        assert!(title.contains("描画が成立しませんでした"), "{title}");
        assert!(title.contains("3000 ミリ秒"), "{title}");
        assert!(title.contains("empty-1"), "{title}");
        assert!(title.contains("診断情報"), "{title}");
        // 改行を含めない（ネイティブの題名は 1 行である）。
        assert!(!title.contains('\n'), "{title}");
    }

    /// 注意書きは、**この時点で真であることだけ**を述べる。
    ///
    /// 代替経路の適用はタスク 8.3 が実装する。実装前に「次回の起動では描画の代替経路を
    /// 試みます」と書くと、提示している情報が利用者に対して誤りになる（レビュー指摘 2）。
    /// **8.3 が適用を実装したら、この否定を外して約束の行を足してよい**（そのときは真になる）。
    #[test]
    fn the_notice_states_only_what_is_true_now() {
        let notice = missing_paint_notice(&record(None));

        assert!(notice.contains("初回描画が成立しませんでした"), "{notice}");
        assert!(notice.contains("empty-1"), "{notice}");
        assert!(notice.contains("記録"), "{notice}");
        assert!(notice.contains("動作を続け"), "{notice}");
        assert!(
            !notice.contains("代替経路"),
            "未実装の機能（8.3 の代替経路）を利用者へ約束している: {notice}"
        );
        assert!(notice.contains('\n'), "{notice}");
    }

    /// 埋め込む文字列は JavaScript のリテラルとして逃がされる（引用符・改行・バックスラッシュ）。
    #[test]
    fn embedded_strings_are_escaped_for_javascript() {
        let script = notice_script("題名 \"引用符\" と \\ と \n 改行", "注意 \"書き\"");

        assert!(script.contains("\\\"引用符\\\""), "{script}");
        assert!(script.contains("\\\\"), "{script}");
        assert!(script.contains("\\n"), "{script}");
        // 生の改行がリテラルの中へ漏れていない（文そのものの改行は別である）。
        assert!(
            !script.contains("題名 \"引用符\""),
            "逃がさない文字列がそのまま入っている: {script}"
        );
    }

    /// 差し込みはアプリの資産を参照せず、自分で要素を作る（要件 10.2 の核心）。
    ///
    /// **スタイルは CSSOM で当てる。** `style` 属性の設定は通信内容保護方針（`style-src
    /// 'self'`、5.3）が遮断するため、`setAttribute("style", …)` を使うと注意書きが
    /// 位置も色も失ったただの文字列になる（実測）。`element.style.…` への代入は遮断されない。
    #[test]
    fn the_notice_script_depends_on_no_application_asset() {
        let script = notice_script("title", "notice");

        assert!(script.contains("document.createElement"), "{script}");
        assert!(script.contains("document.getElementById"), "{script}");
        assert!(script.contains("element.textContent"), "{script}");
        assert!(script.contains("catch"), "{script}");
        // スタイルは CSSOM 経由で当てる（CSP が `style` 属性を遮断するため）。
        assert!(script.contains("element.style.position"), "{script}");
        assert!(script.contains("element.style.backgroundColor"), "{script}");
        assert!(
            !script.contains("setAttribute(\"style\""),
            "CSP が遮断するスタイル属性の設定を使っている: {script}"
        );
        for forbidden in ["import ", "/assets/", ".css", "React", "import.meta"] {
            assert!(
                !script.contains(forbidden),
                "アプリの資産への参照がある ({forbidden}): {script}"
            );
        }
    }

    /// 取り下げは同じ `id` の要素を引いて取り除き、題名を元へ戻す。
    ///
    /// **差し込みと取り下げが同じ定数を使うこと**を、両方のスクリプトに同じ `id` が現れることで
    /// 固定する（片方だけ綴りを変えると、復帰した画面が覆われたままになる）。
    #[test]
    fn the_withdrawal_removes_the_notice_and_restores_the_title() {
        let withdraw = withdraw_script();
        let notice = notice_script("title", "notice");

        assert!(withdraw.contains("removeChild"), "{withdraw}");
        assert!(
            withdraw.contains(&js_string_literal(NOTICE_ELEMENT_ID)),
            "{withdraw}"
        );
        assert!(
            notice.contains(&js_string_literal(NOTICE_ELEMENT_ID)),
            "{notice}"
        );
        assert!(
            withdraw.contains(&js_string_literal(crate::window::WINDOW_TITLE)),
            "{withdraw}"
        );
        assert!(withdraw.contains("catch"), "{withdraw}");
        // アプリの資産を参照しない（取り下げも同じ制約の下にある）。
        for forbidden in ["import ", "/assets/", ".css", "React", "import.meta"] {
            assert!(
                !withdraw.contains(forbidden),
                "アプリの資産への参照がある ({forbidden}): {withdraw}"
            );
        }
    }

    /// 初回の通知は `Ok` で返し、提示は動かさない。
    #[test]
    fn a_first_notification_replies_with_the_verdict_and_keeps_the_notice() {
        assert_eq!(
            heartbeat_action(NotifyOutcome::Decided(RenderVerdict::Painted)),
            HeartbeatAction::Reply {
                verdict: RenderVerdict::Painted,
                withdraw: false,
            }
        );
    }

    /// **期限超過のあとの通知は、判定（`NoPaint`）をそのまま返しつつ提示だけを取り下げる。**
    ///
    /// これが `already_decided_no_paint` → 取り下げの唯一の対応付けである（コマンド本体は
    /// `WebviewWindow` を取るため単体化できず、契約はこの純粋な関数で固定する）。
    #[test]
    fn a_late_notification_replies_with_the_no_paint_verdict_and_withdraws_the_notice() {
        assert_eq!(
            heartbeat_action(NotifyOutcome::AlreadyDecided(RenderVerdict::NoPaint)),
            HeartbeatAction::Reply {
                verdict: RenderVerdict::NoPaint,
                withdraw: true,
            }
        );
    }

    /// 確定済みの通知の繰り返しは、最初の判定をそのまま返し、提示も記録も動かさない。
    #[test]
    fn a_repeated_notification_replies_with_the_first_verdict_and_keeps_the_notice() {
        for verdict in [RenderVerdict::Painted, RenderVerdict::SoftwareRaster] {
            assert_eq!(
                heartbeat_action(NotifyOutcome::AlreadyDecided(verdict)),
                HeartbeatAction::Reply {
                    verdict,
                    withdraw: false,
                }
            );
        }
    }

    /// 監視の項目が無いウィンドウだけが封筒の失敗腕になる。
    #[test]
    fn only_an_unwatched_notification_takes_the_error_arm() {
        assert_eq!(
            heartbeat_action(NotifyOutcome::Unwatched),
            HeartbeatAction::Unwatched
        );
    }

    /// ラスタライザの表現（取得できなかったことを空文字で表さない）。
    #[test]
    fn the_renderer_text_marks_a_missing_value() {
        assert_eq!(renderer_text(None), "(取得できなかった)");
        assert_eq!(renderer_text(Some("llvmpipe")), "llvmpipe");
    }
}

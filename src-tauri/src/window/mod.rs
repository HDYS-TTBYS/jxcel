//! ウィンドウの生成とレジストリ — ラベル規約 `doc-<連番>` / `empty-<連番>` と、ウィンドウの
//! 識別子から状態への写像を保持する。ウィンドウ単位の状態管理は基盤側に存在しない。
//!
//! 所有: `WindowManager`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 2.1, 2.2, 2.3, 2.5, 2.7, 2.10, 4.6。
//!
//! タスク 6.1 が置いた実体は次の 4 つである:
//!
//! 1. **生成は非同期で行う**（[`open`]）。`WebviewWindowBuilder::build()` を同期のコマンドや
//!    イベントハンドラの中で呼ぶと Windows でデッドロックする（design.md「WindowManager」、
//!    research.md「ウィンドウ管理と複数ウィンドウ」）。Tauri の `build()` はメインスレッドへ
//!    メッセージを送って応答を待つため、呼び出し元がメインスレッド（イベントループの
//!    コールバック）だと永久に待つ。**したがって `build()` を呼ぶのは
//!    [`tauri::async_runtime::spawn`] の中の 1 箇所（[`build_window`]）だけにしてある。**
//!    [`open`] はラベルを確保して登録し、生成を非同期ランタイムへ渡して即座に戻る。
//!    同期文脈（単一インスタンスの引き継ぎコールバック、`RunEvent::Reopen`、`RunEvent::Ready`）
//!    からは [`open`] だけを呼ぶこと。
//! 2. **レジストリ**（[`WindowRegistry`]）。`Manager::manage<T>()` は型ごとに 1 インスタンスの
//!    アプリ全体状態であり、ウィンドウ単位の状態管理機構は基盤に存在しない。ラベルをキーに
//!    した写像を自前で持ち、生成時に登録し、破棄（[`WindowEvent::Destroyed`]）で取り除く。
//!    **登録は生成の前に行うので、写像の値は段階（[`WindowPhase`]）を持つ** — ネイティブの
//!    ウィンドウがまだ無い「生成中」は正常な中間状態であり、**残骸として取り除いてはならない**
//!    （取り除くと生きているウィンドウが写像から落ち、冗長な 2 枚目ができる）。
//! 3. **ラベル規約**。ドキュメントを関連付けるウィンドウは `doc-<連番>`、関連付けない
//!    ウィンドウは `empty-<連番>`。連番は種別ごとに 1 から単調増加する。**位置とサイズの記憶は
//!    この規約に依存させない**（要件 2.7。単一のキーへ束ねるのは 6.3 の [`geometry`] の責務で
//!    あり、ここでは規約をキーに使わないことだけを保つ）。
//! 4. **関連付けの意味論**。1 つのウィンドウは高々 1 つのドキュメントに関連付く（要件 2.1）。
//!    別のドキュメントを開く要求は**既存のウィンドウを閉じずに**新しいウィンドウを開く
//!    （要件 2.3）。ドキュメントを指定しない起動は関連付けの無いウィンドウを開く（要件 2.2）。
//!    すべてのウィンドウは 1 つのアプリケーションプロセスの中にある（要件 2.5。生成はすべて
//!    このモジュールの `WebviewWindowBuilder` 経由であり、別プロセスを起こす経路は無い）。
//!
//! 生成の失敗は他のウィンドウの動作を中断させない（要件 2.10）。[`open`] の非同期タスクは
//! 失敗を記録して登録を取り消して戻るだけで、パニックも `?` の伝播もイベントループへ届かない
//! （タスクのパニックは非同期ランタイムが隔離し、返った `JoinHandle` を捨てているため外へ
//! 出ない）。**提示は 2 段である** — 記録機構の `log::error!` の 1 行（標準出力と診断の記録）と、
//! 診断の保存先へ残す 1 件の記録
//! （`crate::lifecycle::persist_window_failure`。失敗したラベル・対象のドキュメント・理由を
//! 名指しし、他のウィンドウに触れていないことを述べる）。強制的に失敗させる検証用の入口は
//! `force_creation_failure` にあり、
//! **`verification-triggers` feature でのみコンパイルされる**（既定のビルドには検証専用の
//! 経路が入らない。tasks.md 5.4 の申し送り）。
//!
//! 子モジュール [`close`]（終了拒否の仲介 / タスク 7.6）は、フロントエンドの購読から呼ばれる
//! `can_close_window` コマンドを持ち、委譲点（[`crate::ports::DocumentHostPort`]）の判定を
//! 境界へ写す。**Rust 側は拒否を足さない** — 基盤が「JS リスナの存在」だけで自動的に拒否し、
//! 可否の往復はフロントエンドが載せる（[`close`] のモジュール doc を参照）。
//! [`geometry`]（位置とサイズの記憶 / 要件 2.7 / タスク 6.3）は生成の初期値（[`build_window`]）
//! と破棄の通知（[`on_window_event`]）の 2 箇所に結線されている。
//!
//! **初回描画の監視（要件 10.1、10.2 / タスク 8.2）も同じ 2 箇所に結線されている。**生成が
//! 成功した時点（[`build_window`]。**ウィンドウ生成の唯一の場所**であるため、起動時・引き継ぎ・
//! Dock クリックのどの経路でも通る）で [`crate::watchdog::start_watch`] が期限付きの監視を
//! 始め、破棄の通知で [`crate::watchdog::forget_watch`] が取り消す（**期限より前に閉じられた
//! ウィンドウを不成立として記録しない**）。実体は Tauri 非依存の中核
//! （`app_shell::render`）にあり、ここは呼ぶだけである。
//!
//! 子モジュール [`association`]（タスク 9.6）は、**このレジストリに記録された関連付けを
//! 境界へ写す** `window_document_state` コマンドを持つ。ドキュメントを関連付けていない
//! ウィンドウにだけ操作の導線を提示するための判定材料であり、**判定は接頭辞ではなく写像の
//! 記録から取る**（`attach` は記録を書き換えない。同モジュールの doc を参照）。

pub(crate) mod association;
pub(crate) mod close;
mod geometry;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use app_shell::ipc::WindowLabel;
use tauri::{
    AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window,
    WindowEvent,
};
use tauri_plugin_log::log;

/// 生成するウィンドウの表示名。`tauri.conf.json` の `productName` と一致させる。
///
/// **クレート内に公開する**のは、描画不成立の提示を取り下げるとき（`crate::watchdog`）に
/// 元の題名へ戻す必要があるためである（同じ綴りを 2 箇所に持たない）。
pub(crate) const WINDOW_TITLE: &str = "jxcel";

/// 新しいウィンドウの既定の大きさ（論理ピクセル）。
///
/// `tauri.conf.json` の `app.windows` が宣言していた大きさを引き継ぐ（同ファイルの宣言は
/// 生成経路を 6.1 が引き取ったため空にしてある）。**位置とサイズの記憶と復元（要件 2.7）は
/// 6.3 が [`geometry`] で重ねる** — ここは宣言されていた既定値の維持だけを担う。
const DEFAULT_WINDOW_SIZE: (f64, f64) = (1200.0, 800.0);

// ---------------------------------------------------------------------------
// 生成要求と公開の生成経路（要件 2.2, 2.3, 2.10）
// ---------------------------------------------------------------------------

/// ウィンドウの生成要求。**ドキュメントの有無が種別を決める**（要件 2.1〜2.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowRequest {
    /// ドキュメントを関連付けないウィンドウ（要件 2.2）。
    Empty,
    /// ドキュメントを関連付ける新しいウィンドウ（要件 2.3）。
    Document(PathBuf),
}

impl WindowRequest {
    /// 要求が運ぶドキュメント（無ければ `None`）。ラベル規約の種別はこれで決まる。
    fn document(&self) -> Option<PathBuf> {
        match self {
            Self::Empty => None,
            Self::Document(path) => Some(path.clone()),
        }
    }
}

/// 要求に応じたウィンドウを**非同期で**開く。**同期文脈から呼んでよい唯一の生成経路である。**
///
/// 行うことは 2 つだけである:
///
/// 1. レジストリからラベルを確保して登録する（この時点で「どのウィンドウがどのドキュメントに
///    関連付くか」が確定する）。
/// 2. 生成を [`tauri::async_runtime::spawn`] へ渡す（[`spawn_creation`]）。
///
/// この関数はブロックしないので、同期のコマンドやイベントハンドラから呼んでも Windows の
/// デッドロックを起こさない。**生成の失敗は記録に残し、既に開いている他のウィンドウには
/// 何もしない**（要件 2.10）。
pub fn open<R: Runtime>(app: &AppHandle<R>, request: WindowRequest) {
    let state = app.state::<WindowRegistry>().allocate(request.document());
    log::debug!(
        "ウィンドウの生成を予約した: label={} ドキュメント={}",
        state.label().as_str(),
        describe_document(state.document()),
    );
    spawn_creation(app.clone(), state);
}

/// 確保済みのラベルでウィンドウを 1 枚構築する非同期タスク。
///
/// **登録は先に済んでいる。**生成できた場合はそのままレジストリに残し（破棄の通知が取り除く）、
/// 失敗した場合は登録を取り消す。こうしないと、生成直後にウィンドウが閉じられたときの破棄通知が
/// 登録を見つけられず、登録が漏れる。
fn spawn_creation<R: Runtime>(app: AppHandle<R>, state: WindowState) {
    tauri::async_runtime::spawn(async move {
        let label = state.label().as_str().to_owned();
        // 生成に使うラベル。**検証専用の引き金（`verification-triggers` feature）が有効なとき
        // だけ**、確保したラベルの代わりに意図的に衝突するラベルへ差し替わる。既定のビルドでは
        // 確保したラベルがそのまま返る（分岐ごと消える）。
        let target = creation_target(&label);
        match build_window(&app, &target) {
            Ok(window) => {
                focus(&window);
                let registry = app.state::<WindowRegistry>();
                if registry.mark_ready(&label) {
                    log::info!(
                        "ウィンドウを開いた: label={label} ドキュメント={} / レジストリ: {}",
                        describe_document(state.document()),
                        registry.describe(),
                    );
                    // **ウィンドウの集合が変わった**ので、メニューの有効・無効を計算し直す
                    // （要件 3.5。タスク 7.5）。アプリ全体のメニューしか持てない環境では、
                    // 新しいウィンドウが対象になると状態が変わる。
                    crate::menu::refresh(&app);
                } else {
                    // 生成の完了前に破棄の通知が届いていた（登録は既に無い）。取り消し済みなので
                    // 登録をやり直さない。
                    log::warn!("ウィンドウは生成できたが登録が既に無い: label={label}");
                }
            }
            Err(error) => {
                // 存在しないウィンドウの登録を残さない。**他のウィンドウには触れない**（要件 2.10）。
                app.state::<WindowRegistry>().remove(&label);
                log::error!(
                    "ウィンドウを生成できなかった: label={label} ドキュメント={} / {error}",
                    describe_document(state.document()),
                );
                // **失敗の提示**（要件 2.10）。記録機構の 1 行だけでは利用者に届く保証が無いため、
                // 診断の保存先へ 1 件の記録として残す（`lifecycle::persist_window_failure` の doc
                // を参照）。**他のウィンドウには触れない** — ここで行うのは登録の取り消しと記録だけ
                // であり、既に開いているウィンドウは動作を続ける。
                crate::lifecycle::persist_window_failure(
                    &label,
                    &describe_document(state.document()),
                    &error.to_string(),
                );
            }
        }
    });
}

/// ウィンドウを 1 枚構築する。**`WebviewWindowBuilder::build()` を呼ぶ唯一の場所である。**
///
/// ここは必ず [`tauri::async_runtime::spawn`] のタスク（イベントループのスレッドではない）から
/// 呼ばれる。同期の文脈から直接呼ぶと Windows でデッドロックする（モジュール doc を参照）。
///
/// **直近に閉じられたウィンドウの位置とサイズを初期値に使う**（要件 2.7、タスク 6.3）。
/// 復元の判断は [`geometry::initial_geometry`] が行う（保存値が無い・壊れている・画面外の
/// 位置はどれも生成側の既定値に落ちる）。生成が済んだら、その形状を最初の観測として
/// [`geometry::remember_created`] に渡す（移動・拡大縮小の通知が届かないうちに閉じられても、
/// 少なくとも生成時の形状を次のウィンドウへ引き継げるようにする）。
fn build_window<R: Runtime>(app: &AppHandle<R>, label: &str) -> tauri::Result<WebviewWindow<R>> {
    let initial = geometry::initial_geometry(app);
    let (width, height) = initial.size().unwrap_or(DEFAULT_WINDOW_SIZE);
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::default())
        .title(WINDOW_TITLE)
        .inner_size(width, height);
    // 検証専用: 初期画面の選択（要件 10.4 / タスク 9.7）と一括転送の駆動（要件 4.5 / タスク
    // 10.8）。**既定のビルドではこの分岐ごと消える**ので、配布物の初期画面は常に 9.6 の
    // 空ウィンドウの画面であり、一括転送も起きない（環境変数の読み取りは
    // [`initial_screen_script`] / [`bulk_rows_script`] の中にしか無く、どちらも
    // `verification-triggers` の下にある）。指定が無ければ何もしない
    // （**無条件に初期化スクリプトを足さない**）。
    #[cfg(feature = "verification-triggers")]
    if let Some(script) = verification_init_script() {
        builder = builder.initialization_script(script);
    }
    // 位置は復元できたときだけ指定する（指定しなければウィンドウマネージャの既定の配置に
    // 任せる）。
    if let Some((x, y)) = initial.position() {
        builder = builder.position(x, y);
    }
    let window = builder.build()?;
    geometry::remember_created(&window);
    // 初回描画の監視を開始する（要件 10.1、10.2。タスク 8.2）。**ここがウィンドウ生成の
    // 唯一の場所**なので、起動時・二重起動の引き継ぎ・Dock クリックのどの経路で開かれた
    // ウィンドウにも監視が付く。期限はこの瞬間から測る（`app_shell::render::FIRST_PAINT_DEADLINE`）。
    // 破棄は [`on_window_event`] が `forget` で取り消す（期限より前に閉じられたウィンドウを
    // 不成立として記録しない）。
    crate::watchdog::start_watch(app, &WindowLabel::new(label.to_owned()));
    // メニューを付ける（要件 3.1, 3.6。タスク 7.4）。**ここがウィンドウ生成の唯一の場所**なので、
    // 後から作られるウィンドウ（起動時・二重起動の引き継ぎ・Dock クリック）にもメニューが付く。
    // ウィンドウ単位のメニューを持てないプラットフォーム（macOS）では何もしない — アプリ全体の
    // メニューは `menu::install` が設定済みである（[`crate::menu::attach_to_window`] の doc）。
    crate::menu::attach_to_window(&window);
    Ok(window)
}

/// ウィンドウを復元して前面に出す。失敗は記録に残すだけで、呼び出し元の処理を妨げない。
pub fn focus<R: Runtime>(window: &WebviewWindow<R>) {
    if let Err(error) = window.unminimize() {
        log::warn!("ウィンドウを復元できない: {error}");
    }
    if let Err(error) = window.set_focus() {
        log::warn!("ウィンドウを前面に出せない: {error}");
    }
}

/// ウィンドウのイベントを処理する（`Builder::on_window_event` に結線する）。
///
/// 行うことは 3 つである:
///
/// 1. **位置とサイズの記憶**（要件 2.7、タスク 6.3）。[`geometry::observe`] が移動・
///    拡大縮小を観測し、**破棄の通知で設定ストアへ保存する**（終了イベントを待たない）。
/// 2. **メニューの有効・無効の更新**（要件 3.5、タスク 7.5）。**フォーカスが移るたび**に
///    [`crate::menu::refresh`] を呼ぶ（アプリ全体のメニューしか持てない環境では、メニューが
///    ウィンドウごとの状態を持てないため、対象ウィンドウを解決し直す必要がある）。
/// 3. **破棄の通知をレジストリに反映する**。登録は生成の前に済んでいるため、**破棄の通知は
///    必ず登録を見つける**（登録が漏れない）。破棄はウィンドウが閉じられた後に届くので、
///    ここで取り除いた登録はもう使われない。**ウィンドウの集合が変わった**ので、ここでも
///    メニューの有効・無効を計算し直す（対象ウィンドウが消えると対象が無くなる）。
///
/// **`WindowEvent::CloseRequested` で `prevent_close()` を呼ばない。** Tauri は
/// `tauri://close-requested` の JS リスナが登録されているだけで自動的に拒否する
/// （`tauri` 2.11.5 の `src/manager/window.rs`）。可否の往復はフロントエンドが
/// [`close`] の `can_close_window` で載せるので、ここに 2 つ目の拒否を重ねると
/// その経路まで塞いでしまう（[`close`] のモジュール doc）。
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    geometry::observe(window, event);
    // フォーカスの出入りの両方で更新する（アプリ全体のメニューでは、フォーカスを失ったときに
    // 対象が無くなることが状態に現れる）。
    if matches!(event, WindowEvent::Focused(_)) {
        crate::menu::refresh(window.app_handle());
    }
    if !matches!(event, WindowEvent::Destroyed) {
        return;
    }
    let label = window.label().to_owned();
    // 初回描画の監視を取り消す（要件 10.2。タスク 8.2）。**破棄されたウィンドウを不成立と
    // して記録しない** — 画面に残っていないので「無内容の画面のまま留まらせる」ことに
    // 当たらず、記録すると利用者が閉じただけのウィンドウを失敗として数えてしまう。
    crate::watchdog::forget_watch(window.app_handle(), &WindowLabel::new(label.clone()));
    if let Some(state) = window.state::<WindowRegistry>().remove(&label) {
        log::info!(
            "ウィンドウを閉じた: label={label} ドキュメント={}",
            describe_document(state.document()),
        );
    }
    crate::menu::refresh(window.app_handle());
}

/// **検証専用**: 生成の失敗経路（要件 2.10）を実測する。
///
/// 行うことは「生きているウィンドウのラベルを [`FORCED_FAILURE_LABEL`] に置いてから、通常の
/// 生成経路（[`open`] → [`spawn_creation`]）をそのまま通す」ことだけである。`spawn_creation` は
/// 確保したラベルの代わりにそのラベルで構築を試み、Tauri の `WindowManager::prepare_window` が
/// 重複として拒否する。**その結果、通常の失敗側の分岐（確保済みの登録の取り消しと、エラーの
/// 報告）が実行される** — 検証専用の分岐を別に持たないので、失敗の扱いそのものを実測できる。
///
/// 衝突相手が実在することが前提である（まだ作られていないラベルでは重複にならない）。対象の
/// ウィンドウが現れるまでブロッキング用の実行器で待つ。**メインスレッドも同期のコールバックも
/// 塞がない**（待つのは検証用のブロッキングタスクの中だけである）。呼ぶのは検証専用の引き金
/// （`JXCEL_VERIFICATION_EXIT_AFTER_MS` の `fail-window`）だけであり、通常の起動では通らない。
/// # 既定のビルドには存在しない
///
/// **この関数は `verification-triggers` feature でのみコンパイルされる**（tasks.md 5.4 の
/// 申し送り — 7.4 が「終了」メニュー項目を配線したことに伴い、検証専用の引き金を配布物から
/// 外す）。したがって [`spawn_creation`] のラベルの差し替え（[`creation_target`]）も既定では
/// 確保したラベルをそのまま返すだけになる。
#[cfg(feature = "verification-triggers")]
pub fn force_creation_failure<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 衝突相手になる「生きている」ウィンドウが現れるまで待つ（起動直後は生成中だから）。
        let mut target = None;
        for _ in 0..200 {
            let registry = app.state::<WindowRegistry>();
            if let Some(label) = registry.first_ready_label() {
                if app.get_webview_window(label.as_str()).is_some() {
                    target = Some(label.as_str().to_owned());
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let Some(target) = target else {
            log::warn!("[検証] 生成失敗の実測には生きているウィンドウが 1 枚要る");
            return;
        };
        *FORCED_FAILURE_LABEL
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(target.clone());
        log::info!("[検証] ウィンドウ生成の失敗を起こす（衝突相手: label={target}）");
        open(&app, WindowRequest::Empty);
    });
}

/// 検証専用: [`spawn_creation`] に、確保したラベルの代わりに使わせる「意図的に衝突するラベル」。
///
/// 通常は `None` であり、そのとき [`spawn_creation`] は確保したラベルで構築する。`Some` の間だけ
/// 検証用の失敗経路に入る。**設定するのは [`force_creation_failure`] だけで、`spawn_creation` が
/// 取り出して直ちに `None` に戻す**（一度きり）。
#[cfg(feature = "verification-triggers")]
static FORCED_FAILURE_LABEL: Mutex<Option<String>> = Mutex::new(None);

/// [`FORCED_FAILURE_LABEL`] を取り出す（取り出したら `None` に戻す）。通常は `None`。
#[cfg(feature = "verification-triggers")]
fn forced_failure_label() -> Option<String> {
    FORCED_FAILURE_LABEL
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

/// 生成に使うラベル。**既定のビルドでは確保したラベルをそのまま返す。**
///
/// `verification-triggers` feature が有効なときだけ、検証専用の引き金
/// （[`force_creation_failure`]）が置いた「意図的に衝突するラベル」を優先する。この差し替えが
/// あることで、通常の生成経路（[`spawn_creation`]）の**失敗側の分岐をそのまま**通して実測
/// できる（検証専用の失敗経路を別に持たない）。
fn creation_target(label: &str) -> String {
    #[cfg(feature = "verification-triggers")]
    let target = forced_failure_label().unwrap_or_else(|| label.to_owned());
    #[cfg(not(feature = "verification-triggers"))]
    let target = label.to_owned();
    target
}

// ---------------------------------------------------------------------------
// 検証専用: 初期画面の選択（要件 10.4 / タスク 9.7）
// ---------------------------------------------------------------------------

/// 検証専用: ウィンドウが最初に表示する画面（フロントエンドの画面識別子）を指定する環境変数。
///
/// **`verification-triggers` feature の下にのみ存在する**（既定のビルドには環境変数の読み取り
/// 自体が入らない。`src-tauri/Cargo.toml` の feature の説明を参照）。
///
/// 3 OS の描画確認（10.4）が、実用画面ではない 2 つの最小画面（`smoke-table` / `smoke-editor`。
/// `src/features/smoke/`）を直接開くための経路である。**Webview はプロセスの環境変数を読めない**
/// ので、値は [`initial_screen_script`] がウィンドウの初期化スクリプトとしてグローバルへ書き、
/// フロントエンド（`src/shell/verificationScreen.ts`）が起動時に読む。**コマンドは増やさない**
/// （通信境界の集合は閉じており、検証のために開けない）。既定のビルドにはこの経路が 1 つも
/// 入らないので、配布物の初期画面は常に 9.6 の空ウィンドウの画面である。
#[cfg(feature = "verification-triggers")]
const VERIFY_INITIAL_SCREEN_ENV: &str = "JXCEL_VERIFICATION_INITIAL_SCREEN";

/// 初期化スクリプトが書くグローバルの名前。**`src/shell/verificationScreen.ts` の
/// `VERIFICATION_INITIAL_SCREEN_GLOBAL` と同じ綴りでなければならない** — 既定のビルドには
/// どちらか一方しか存在しないため、共有できる定数を持てない検証専用の対の契約である。
#[cfg(feature = "verification-triggers")]
const VERIFY_INITIAL_SCREEN_GLOBAL: &str = "__JXCEL_VERIFICATION_INITIAL_SCREEN__";

/// 検証専用: 環境変数の値が初期化スクリプトへ埋め込んでよい形か。
///
/// **値は JavaScript のソースへ文字列として埋め込まれる**ため、任意の文字を許すと式を混ぜられる。
/// 画面の識別子に現れる文字（ASCII の英数字と `-` `_` `.`）だけを許す。フロントエンドも登録済みの
/// 識別子と一致しなければ既定の初期画面へ落ちるので、**外からの入力は二重に止まる**。
#[cfg(feature = "verification-triggers")]
fn is_embeddable_screen_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// 検証専用: フロントエンドの初期画面を選ぶ初期化スクリプト。
///
/// 指定が無い・空・使えない形の値のときは `None` を返し、**ウィンドウは通常どおり既定の初期画面
/// で開く**。初期化スクリプトは「グローバルが作られた後・文書が解析される前・文書の他の
/// スクリプトより前」に走るので、フロントエンドのバンドルが読む時点で値は必ず載っている
/// （`WebviewWindowBuilder::initialization_script` の doc）。
#[cfg(feature = "verification-triggers")]
fn initial_screen_script() -> Option<String> {
    let requested = std::env::var(VERIFY_INITIAL_SCREEN_ENV).ok()?;
    let requested = requested.trim();
    if !is_embeddable_screen_id(requested) {
        log::warn!(
            "{} の値を初期画面の指定に使えない（無視する）: {requested:?}",
            VERIFY_INITIAL_SCREEN_ENV,
        );
        return None;
    }
    // 3 OS の描画確認（10.4）が**どの画面を要求して起動したか**を、ウィンドウの出現と初回描画の
    // 成立（8.2 の成立行）と**同じ記録から**読めるようにする。3 OS はいずれもこの行を同じ綴りで
    // 出すため、CI の段はプラットフォームごとの別機構を足さずに起動の識別を検査できる。
    //
    // **この行は要求であって、描画の証明ではない。**実際に描画された画面はフロントエンドが
    // 登録済みの識別子と照合して決め（`src/shell/verificationScreen.ts`。未知の識別子なら既定の
    // 初期画面へ落ちる）、通知（`RenderHeartbeatRequest::screen`）として戻り、8.2 の成立行の
    // `画面=` に出る。10.4 の段が要求するのは**そちら**である — この行だけを証明にすると、
    // 登録簿に無い識別子でも段が緑になる（レビューで再現された反証）。
    log::info!("検証用の初期画面を指定した: screen={requested}");
    Some(format!(
        "window.{} = \"{}\";",
        VERIFY_INITIAL_SCREEN_GLOBAL, requested
    ))
}

// ---------------------------------------------------------------------------
// 検証専用: 大きなペイロードの一括転送の駆動（要件 4.5 / タスク 10.8）
// ---------------------------------------------------------------------------

/// 検証専用: 転送する行数の一覧を指定する環境変数（例 `100,100000`）。
///
/// **`verification-triggers` feature の下にのみ存在する**（既定のビルドには環境変数の読み取り
/// 自体が入らない）。10.8 は「10 万行規模のデータが **1 回の呼び出し**で受け渡せること」と
/// 「**呼び出し回数が行数に比例しないこと**」を実アプリで観測する。後者を示すには少なくとも
/// 2 つの大きさが要るので、値は**正の整数の一覧**（2〜4 件）とする。
#[cfg(feature = "verification-triggers")]
const VERIFY_BULK_ROWS_ENV: &str = "JXCEL_VERIFICATION_BULK_ROWS";

/// 初期化スクリプトが書く行数の一覧のグローバルの名前。**`src/shell/verificationBulk.ts` の
/// `VERIFICATION_BULK_ROWS_GLOBAL` と同じ綴りでなければならない**（既定のビルドにはどちらか
/// 一方しか存在しない検証専用の対の契約）。
#[cfg(feature = "verification-triggers")]
const VERIFY_BULK_ROWS_GLOBAL: &str = "__JXCEL_VERIFICATION_BULK_ROWS__";

/// 検証専用: 受け付ける行数の下限の件数。**1 件では「行数に比例しない」ことを示せない**ので、
/// 1 件だけの指定は受け付けない（フロントエンド側も同じ規則で弾く）。
#[cfg(feature = "verification-triggers")]
const MIN_VERIFY_BULK_SIZES: usize = 2;

/// 検証専用: 受け付ける行数の上限の件数（初期化スクリプトと記録を小さく保つ）。
#[cfg(feature = "verification-triggers")]
const MAX_VERIFY_BULK_SIZES: usize = 4;

/// 検証専用: 1 件あたりの行数の上限。1 行あたり [`VERIFY_BULK_LINE_BYTES`] バイトなので
/// 1,000,000 行 = 47,000,000 B であり、**7.2 の上限（64 MiB = 67,108,864 B）の内側に収まる**
/// （上限を超える入力は 7.2 の契約で空の応答になるため、検証が経路の上限に当たらない値にする）。
#[cfg(feature = "verification-triggers")]
const MAX_VERIFY_BULK_ROWS: u64 = 1_000_000;

/// 検証専用: 1 行あたりのバイト数。**`src/shell/verificationBulk.ts` の `BULK_LINE_BYTES` と
/// `scripts/check-bulk-transfer.sh` の `bulk_line_bytes` と同じ値でなければならない**
/// （検査器が期待バイト数を `行数 × これ` で計算する）。7.2 の実測（10 万行 = 4,700,000 B）と
/// 同じ 47 B である。
#[cfg(feature = "verification-triggers")]
const VERIFY_BULK_LINE_BYTES: u64 = 47;

/// 検証専用: 環境変数の値を行数の一覧に解釈する。**純粋関数**であり、受け付ける形をテストで
/// 固定する。**初期化スクリプトのソースへそのまま埋め込む**ため、数字と区切り以外は受け付けない
/// （引用符・括弧・式を混ぜられない）。
///
/// 受け付ける形: 空でない正の整数を `,` で 2〜4 件。前後の空白は許す。0・負・非数・範囲外・
/// 件数外はすべて `None`。
#[cfg(feature = "verification-triggers")]
fn parse_bulk_rows(value: &str) -> Option<Vec<u64>> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let mut rows = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let count: u64 = part.parse().ok()?;
        if count == 0 || count > MAX_VERIFY_BULK_ROWS {
            return None;
        }
        rows.push(count);
    }
    if !(MIN_VERIFY_BULK_SIZES..=MAX_VERIFY_BULK_SIZES).contains(&rows.len()) {
        return None;
    }
    Some(rows)
}

/// 検証専用: フロントエンドへ行数の一覧を渡す初期化スクリプト（タスク 10.8）。
///
/// 指定が無い・解釈できない・範囲外のときは `None` を返し、**転送は 1 件も起きない**
/// （検査器は結果の行が現れないので非 0 で落ちる — 黙って小さい転送へ縮退しない）。
/// **どの一覧を要求したかを記録に 1 行残す**（検査器が起動の識別として読む。期待バイト数も
/// ここに出すので、記録だけで要求と期待が突き合わせられる）。
#[cfg(feature = "verification-triggers")]
fn bulk_rows_script() -> Option<String> {
    let requested = std::env::var(VERIFY_BULK_ROWS_ENV).ok()?;
    let rows = match parse_bulk_rows(&requested) {
        Some(rows) => rows,
        None => {
            log::warn!(
                "{} の値を行数の一覧に使えない（無視する）: {requested:?}",
                VERIFY_BULK_ROWS_ENV,
            );
            return None;
        }
    };
    // 区切りの作り方はフロントエンド（`src/shell/verificationBulk.ts` の `join(",")`）と
    // 検査器（`scripts/check-bulk-transfer.sh` の `IFS=,` による分割）の両方で同じ形になる
    // ように固定する。
    let list = rows
        .iter()
        .map(|count| count.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let expected = rows
        .iter()
        .map(|count| (count * VERIFY_BULK_LINE_BYTES).to_string())
        .collect::<Vec<_>>()
        .join(",");
    log::info!(
        "検証用の一括転送を要求した: 行数={list} / 1 行あたり={} B / 期待バイト数={expected}",
        VERIFY_BULK_LINE_BYTES,
    );
    Some(format!("window.{} = [{list}];", VERIFY_BULK_ROWS_GLOBAL))
}

// ---------------------------------------------------------------------------
// 検証専用: グリッドの観測（9.2）の起動の条件
// ---------------------------------------------------------------------------

/// 検証専用: グリッドの観測を要求する環境変数（`1` のときだけ有効）。
///
/// **`verification-triggers` feature の下にのみ存在する**（既定のビルドには環境変数の読み取り
/// 自体が入らない）。9.2 の観測の画面（`src/features/grid/gridObservation.tsx`）は、この指定が
/// 無ければ**登録簿に載るだけで一度も初期画面に選ばれない**（`../shell/Layout.tsx` の
/// `__JXCEL_VERIFICATION__` の分岐と `src/shell/verificationScreen.ts` の対）。
#[cfg(feature = "verification-triggers")]
const VERIFY_GRID_OBSERVATION_ENV: &str = "JXCEL_VERIFICATION_GRID_OBSERVATION";

/// 観測を要求したことを載せるグローバルの名前。**`src/features/grid/gridObservation.tsx` の
/// `VERIFICATION_GRID_OBSERVATION_GLOBAL` と同じ綴りでなければならない**（既定のビルドには
/// どちらか一方しか存在しない検証専用の対の契約）。
#[cfg(feature = "verification-triggers")]
const VERIFY_GRID_OBSERVATION_GLOBAL: &str = "__JXCEL_VERIFICATION_GRID_OBSERVATION__";

/// 検証専用: **グリッドの面が塗られない条件**を要求する環境変数（`1` のときだけ有効）。
///
/// 9.2 は「描画を成立させない条件で起動したとき、無内容の領域のまま留まらず、識別できる情報が
/// 提示され、`paint_failed` の記録が残ること」（要件 12.2 / 12.3）を実起動で観測する。**この
/// 条件を作る引き金は 9.3 の時点で存在しなかった**（`scripts/check-no-paint.sh` の抑止は器の
/// 初回描画のものであり、表の面の不成立ではない）ので、9.2 がここへ足す。
///
/// **実装の差し替えは行わない**（製品の分岐を増やさない）。値は観測の画面へ渡り、画面が
/// **「描かない移植口」を選ぶ**（`src/features/grid/renderer/nonPaintingPort.tsx`）— 製品の
/// 画面は通常どおり移植口を使い、その移植口が何も塗らないので、**描画成立の検査（7.6）と
/// 告知（9.3）の経路そのもの**が不成立を観測する。
#[cfg(feature = "verification-triggers")]
const VERIFY_GRID_PAINT_FAILURE_ENV: &str = "JXCEL_VERIFICATION_GRID_PAINT_FAILURE";

/// 塗られない条件を要求したことを載せるグローバルの名前（上の対の契約）。
#[cfg(feature = "verification-triggers")]
const VERIFY_GRID_PAINT_FAILURE_GLOBAL: &str = "__JXCEL_VERIFICATION_GRID_PAINT_FAILURE__";

/// 検証専用: **貼り付けの往復の筋書き**（10.8。要件 7.2）を要求する環境変数（`1` で有効）。
///
/// 貼り付けの往復は**ネイティブのメニューの活性化**が要る（`data-grid.paste` には
/// アクセラレータを付けない — 付けると DOM の打鍵の貼り付けが基盤に取られる）。活性化できるのは
/// アクセシビリティの木を持つ Linux の段だけであり（macOS の WKWebView / Windows の WebView2 には
/// AT-SPI が無い）、**活性化しない段では観測の画面が待ちに入らないように**この引き金で分ける。
#[cfg(feature = "verification-triggers")]
const VERIFY_GRID_PASTE_ENV: &str = "JXCEL_VERIFICATION_GRID_PASTE";

/// 貼り付けの筋書きを要求したことを載せるグローバルの名前（上の対の契約）。
#[cfg(feature = "verification-triggers")]
const VERIFY_GRID_PASTE_GLOBAL: &str = "__JXCEL_VERIFICATION_GRID_PASTE__";

/// 検証専用: 貼り付けの筋書きを要求する初期化スクリプト（9.2。10.8）。
#[cfg(feature = "verification-triggers")]
fn grid_paste_script() -> Option<String> {
    let requested = std::env::var(VERIFY_GRID_PASTE_ENV).ok()?;
    if !is_verify_switch_on(&requested) {
        log::warn!(
            "{} の値を貼り付けの筋書きの指定に使えない（無視する）: {requested:?}",
            VERIFY_GRID_PASTE_ENV,
        );
        return None;
    }
    log::info!("検証用の貼り付けの筋書きを要求した: {VERIFY_GRID_PASTE_ENV}=1");
    Some(format!("window.{} = true;", VERIFY_GRID_PASTE_GLOBAL))
}

/// 検証専用: 環境変数の値が「有効」の綴りか（`1` だけを有効とする）。
///
/// **`is_embeddable_screen_id` と同じ理由で厳しくする** — 値は初期化スクリプトのソースへ
/// 埋め込まれるので、`true` 以外の任意の文字列を受け付けると式を混ぜられる。ここで扱う値は
/// 真偽だけなので、受理するのは `1` ただ 1 つである（`0` は「指定しない」と同じ扱い）。
#[cfg(feature = "verification-triggers")]
fn is_verify_switch_on(value: &str) -> bool {
    value.trim() == "1"
}

// ---------------------------------------------------------------------------
// 検証専用: マクロの実行の観測（macro-runtime スペックの 5.1）の引き金
// ---------------------------------------------------------------------------

/// 検証専用: 起動時に実行するマクロの名前を指定する環境変数。**`,` 区切りの並び**である。
///
/// **`verification-triggers` feature の下にのみ存在する**（既定のビルドには環境変数の読み取り
/// 自体が入らない）。`macro-runtime` スペックの 5.1 が、**「一覧 → 選択 → 実行」を起動時に
/// 仕込む**ために置き、5.2 が**並び**へ広げた（下の「なぜ並びなのか」）。
///
/// # なぜ画面の押下ではないのか（4.4 の申し送り）
///
/// 「選ぶ → 実行する」の押下は **AT-SPI では再現できない** — この機械の WebKitGTK は DOM を
/// アクセシビリティの木へ露出しない（4.4 の実測）。したがって 5.1 の引き金は**画面の押下では
/// なく、起動時の仕込み**であり、値は「どのマクロを実行するか」だけを運ぶ。実行の要求そのものは
/// 製品の経路（`macro_run` のコマンドとフロントエンドの面）を通る — 検証専用の実行経路を
/// 別に持たない（`window/close.rs` の拒否と `session/verification.rs` と同じ規律）。
///
/// # なぜ並びなのか（5.2 の要求）
///
/// 要件 6.4 は「**打ち切りの後も画面を操作できる状態に戻す**」ことを求める。これは
/// **打ち切りが起きたのと同じ起動の中で、続けて別のマクロが走ること**でしか観測できない
/// （別の起動で確かめると、確かめているのは「次の起動ができること」である）。したがって 5.2 は
/// 1 回の仕込みで**順に実行する並び**を要求する — `JXCEL_VERIFICATION_MACRO_RUN=標本の打ち切り,標本の記入`
/// は、打ち切りの後に同じウィンドウで `標本の記入` を走らせ、**観測の行を 2 行**残す。
///
/// **`,` は既存の一族と同じ区切りである**（`JXCEL_VERIFICATION_SESSION=open,edit,<行数>,save`、
/// `JXCEL_VERIFICATION_BULK_ROWS=100,100000`）。したがって**名前そのものに `,` を含むマクロは
/// この引き金では指せない**（`split_macro_names` が要素として読む）。
///
/// # 併せて要る指定（**この引き金だけでは実行まで進まない**）
///
/// 1. **起動の引数に標本の文書を渡す**（`crates/macro-runtime/examples/make-macro-document.rs`
///    が書き出す `.jxcel`。標本のマクロの名前は `標本の記入` / `標本の往復` / `標本の拒否` /
///    `標本の失敗` / `標本の打ち切り`）。
/// 2. `JXCEL_VERIFICATION_INITIAL_SCREEN=grid` — マクロの変更を適用するには、そのウィンドウで
///    **グリッドがシートを開いている**ことが要る（`crates/macro-runtime` の design.md
///    「System Flows → 実行の流れ」と `src-tauri/src/commands/macro.rs` の表）。初期画面が
///    グリッドでなければ、実行は「適用先が決まらない」経路の失敗として記録に残る
///    （**黙って成功に見えることは無い**）。
///
/// # 側の分担
///
/// 値は [`macro_run_script`] がウィンドウの初期化スクリプトとしてグローバルへ書き、
/// フロントエンド（`src/shell/verificationMacroRun.ts`。`src/main.tsx` が
/// `__JXCEL_VERIFICATION__` の下でだけ動的 import する）が読んで、製品の面の保持
/// （`MACRO_SURFACE_STORE`）を通して**並びの順に**「一覧 → 選択 → 実行」を駆動する。
/// **コマンドも権限も増やしていない**（使うのは製品の `macro_list` / `macro_run` だけである）。
#[cfg(feature = "verification-triggers")]
const VERIFY_MACRO_RUN_ENV: &str = "JXCEL_VERIFICATION_MACRO_RUN";

/// 起動時に実行するマクロの名前の**並び**を載せるグローバルの名前。
/// **`src/shell/verificationMacroRun.ts` の `VERIFICATION_MACRO_RUN_GLOBAL` と同じ綴りでなければ
/// ならない**（既定のビルドにはどちらか一方しか存在しない検証専用の対の契約）。
///
/// 載る値は**文字列の配列**である（5.1 は文字列 1 つを載せていた。5.2 が並びへ広げた —
/// フロントエンドは配列だけを受け付け、文字列は受け付けない。**両方を受け付けると、
/// どちらの形でも動くぶんだけ「仕込みが読まれていない」ことに気付きにくくなる**）。
#[cfg(feature = "verification-triggers")]
const VERIFY_MACRO_RUN_GLOBAL: &str = "__JXCEL_VERIFICATION_MACRO_RUN__";

/// 検証専用: 実行するマクロの名前の上限。**初期化スクリプトと記録を無駄に大きくしない**ためと、
/// 仕込みの誤り（長すぎる並び）を黙って通さないために置く（`is_embeddable_screen_id` の 64 と
/// 同じ趣旨。標本の並びは 2 件である）。
#[cfg(feature = "verification-triggers")]
const MAX_VERIFY_MACRO_NAMES: usize = 8;

/// 検証専用: [`VERIFY_MACRO_RUN_ENV`] の値を**名前の並び**へ解釈する。**純粋関数**であり、
/// 文法をテストで固定する（`tests::the_macro_run_trigger_is_a_comma_separated_list`）。
///
/// 解釈できない値は `None`（呼び出し元は何も仕込まない）:
///
/// - 要素が 1 つも無い（空文字）
/// - 空の要素がある（`,標本の記入` / `標本の記入,` / `標本の記入,,標本の失敗`）— 空の名前は
///   一覧に必ず無いので、受け付けると「一覧に無い名前」の観測が 1 行増えるだけである
/// - 要素が [`MAX_VERIFY_MACRO_NAMES`] を超える
///
/// 要素の前後の空白は無視する（既存の引き金の一族と同じ扱い）。
#[cfg(feature = "verification-triggers")]
fn split_macro_names(value: &str) -> Option<Vec<&str>> {
    let names: Vec<&str> = value.split(',').map(str::trim).collect();
    if names.is_empty() || names.len() > MAX_VERIFY_MACRO_NAMES {
        return None;
    }
    if names.iter().any(|name| name.is_empty()) {
        return None;
    }
    Some(names)
}

/// 検証専用: マクロの名前を初期化スクリプトの**文字列リテラル**へ埋め込める形にする。
///
/// **`is_embeddable_screen_id` の白名簿は使えない** — 標本のマクロの名前は日本語
/// （`標本の記入`）であり、ASCII の英数字だけを許すと**仕込みたい名前が 1 つも通らない**。
/// 代わりに、**リテラルの外へ出る文字を拒み、リテラルの内側で意味を持つ 2 文字を
/// エスケープする**:
///
/// - 制御文字（改行・タブ・NUL など）と行区切り（U+2028 / U+2029）は**名前として不正**として
///   拒む（`None`）。マクロの名前は 1 行の名前であり、これらを含む名前が文書にあるなら、
///   その名前を実行しない方がよい（エスケープして実行するより、拒んで記録に残す方がよい）。
/// - `\` と `"` はエスケープする（名前として正当でありうる）。
///
/// 返るのは**引用符を含むリテラルそのもの**である（呼び出し元はそのまま配列の要素へ置く）。
#[cfg(feature = "verification-triggers")]
fn embeddable_macro_name(value: &str) -> Option<String> {
    // 名前の長さの上限。標本の名前は数文字であり、1000 文字を超える名前は初期化スクリプトと
    // 記録を無駄に大きくするだけである（`is_embeddable_screen_id` の 64 と同じ趣旨）。
    const MAX_MACRO_NAME_BYTES: usize = 256;
    if value.is_empty() || value.len() > MAX_MACRO_NAME_BYTES {
        return None;
    }
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    for character in value.chars() {
        match character {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '\u{2028}' | '\u{2029}' => return None,
            other if other.is_control() => return None,
            other => literal.push(other),
        }
    }
    literal.push('"');
    Some(literal)
}

/// 検証専用: 起動時のマクロの実行を要求する初期化スクリプト（5.1 / 5.2）。
///
/// 指定が無い・解釈できない（空の要素・多すぎる並び）・名前として使えない文字を含むときは
/// `None` を返し、**仕込みは 1 つも起きない**（検査器は実行の記録が現れないので非 0 で落ちる —
/// 黙って別のマクロを実行しない）。**どの名前を要求したかを記録に 1 行残す**（3 OS の段が
/// 起動の識別として読む。10.4 / 10.8 と同じ規律）。
#[cfg(feature = "verification-triggers")]
fn macro_run_script() -> Option<String> {
    let requested = std::env::var(VERIFY_MACRO_RUN_ENV).ok()?;
    let Some(names) = split_macro_names(&requested) else {
        log::warn!(
            "{} の値を実行するマクロの名前に使えない（無視する）: {requested:?}",
            VERIFY_MACRO_RUN_ENV,
        );
        return None;
    };
    let mut literals = Vec::with_capacity(names.len());
    for name in &names {
        let Some(literal) = embeddable_macro_name(name) else {
            log::warn!(
                "{} の値のうち実行するマクロの名前に使えない要素がある（無視する）: {name:?}",
                VERIFY_MACRO_RUN_ENV,
            );
            return None;
        };
        literals.push(literal);
    }
    log::info!(
        "検証用のマクロの実行を要求した: 名前={}",
        names.join(", "),
    );
    Some(format!(
        "window.{} = [{}];",
        VERIFY_MACRO_RUN_GLOBAL,
        literals.join(", ")
    ))
}


/// 検証専用: グリッドの観測を要求する初期化スクリプト（9.2）。
///
/// 指定が無い・`1` でないときは `None` を返し、**観測の画面は起動しない**（登録簿に載るだけで
/// 初期画面にはならない）。**どの引き金を読んだかを記録に 1 行残す** — 3 OS の段は
/// ウィンドウの出現と初回描画の成立と同じ記録から起動を識別する（10.4 / 10.8 と同じ規律）。
#[cfg(feature = "verification-triggers")]
fn grid_observation_script() -> Option<String> {
    let requested = std::env::var(VERIFY_GRID_OBSERVATION_ENV).ok()?;
    if !is_verify_switch_on(&requested) {
        log::warn!(
            "{} の値を観測の指定に使えない（無視する）: {requested:?}",
            VERIFY_GRID_OBSERVATION_ENV,
        );
        return None;
    }
    log::info!("検証用のグリッドの観測を要求した: {VERIFY_GRID_OBSERVATION_ENV}=1");
    Some(format!("window.{} = true;", VERIFY_GRID_OBSERVATION_GLOBAL))
}

/// 検証専用: グリッドの面が**塗られない条件**を要求する初期化スクリプト（9.2。要件 12.2）。
///
/// 指定が無い・`1` でないときは `None` を返し、**通常の移植口（Glide）が使われる**。観測の
/// 画面はこのグローバルが真のときだけ「描かない移植口」を選ぶ。
#[cfg(feature = "verification-triggers")]
fn grid_paint_failure_script() -> Option<String> {
    let requested = std::env::var(VERIFY_GRID_PAINT_FAILURE_ENV).ok()?;
    if !is_verify_switch_on(&requested) {
        log::warn!(
            "{} の値を描画の不成立の指定に使えない（無視する）: {requested:?}",
            VERIFY_GRID_PAINT_FAILURE_ENV,
        );
        return None;
    }
    log::info!("検証用の描画の不成立を要求した: {VERIFY_GRID_PAINT_FAILURE_ENV}=1");
    Some(format!(
        "window.{} = true;",
        VERIFY_GRID_PAINT_FAILURE_GLOBAL
    ))
}

/// 検証専用: 初期化スクリプトを 1 本にまとめる。
///
/// `WebviewWindowBuilder::initialization_script` は複数回呼べるが、**呼び出しを 1 箇所に保つ**
/// ためここで連結する（初期画面の指定・一括転送の指定・グリッドの観測の指定・マクロの実行の
/// 指定は独立であり、どれか 1 つだけでも有効でなければならない）。どれも指定が無ければ
/// `None`（**無条件に初期化スクリプトを足さない**）。
#[cfg(feature = "verification-triggers")]
fn verification_init_script() -> Option<String> {
    let parts: Vec<String> = [
        initial_screen_script(),
        bulk_rows_script(),
        grid_observation_script(),
        grid_paint_failure_script(),
        grid_paste_script(),
        macro_run_script(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.concat())
    }
}

// ---------------------------------------------------------------------------
// レジストリ（要件 2.1, 2.2, 2.3, 2.5）
// ---------------------------------------------------------------------------

/// ウィンドウの種別。ラベル規約の接頭辞と 1 対 1 で対応する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowKind {
    /// ドキュメントを関連付けないウィンドウ。
    Empty,
    /// ドキュメントを関連付けるウィンドウ。
    Document,
}

impl WindowKind {
    /// ラベル規約の接頭辞。
    fn prefix(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Document => "doc",
        }
    }
}

/// ウィンドウの生成段階。
///
/// **登録はネイティブのウィンドウの生成より先に済む**（生成中に閉じられても登録が漏れないための
/// 順序である）。したがって「登録がある」ことと「ネイティブのウィンドウが存在する」ことは別で
/// あり、この区別が無いと生成中の登録を残骸と誤認して取り除いたり、生成中のウィンドウが要求を
/// 満たすのに 2 枚目を作ったりする。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowPhase {
    /// ラベルを確保して生成を予約したが、ネイティブのウィンドウはまだ無い。
    Creating,
    /// 生成が完了し、ネイティブのウィンドウが存在する。
    Ready,
}

/// レジストリが保持するウィンドウ 1 枚の状態。
///
/// **ラベルはこのウィンドウの識別子であり、規約に従って単調に割り当てられる。**関連付けられる
/// ドキュメントは高々 1 つである（要件 2.1）ため、写像ではなく [`Option`] で持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowState {
    /// 割り当てられたラベル（`doc-<連番>` または `empty-<連番>`）。
    label: WindowLabel,
    /// 関連付けられたドキュメントの位置。無ければ関連付けは無い。
    document: Option<PathBuf>,
    /// 生成の段階。
    phase: WindowPhase,
}

impl WindowState {
    /// 割り当てられたラベル。
    pub fn label(&self) -> &WindowLabel {
        &self.label
    }

    /// 関連付けられたドキュメントの位置（無ければ `None`）。
    pub fn document(&self) -> Option<&Path> {
        self.document.as_deref()
    }
}

/// ラベル規約に従って連番を払い出す。**純粋な状態機械であり、ウィンドウにもロックにも依存
/// しない**ので単体テストで固定できる。
#[derive(Debug, PartialEq, Eq)]
struct LabelAllocator {
    /// 次に払い出す `empty-<連番>` の連番。
    next_empty: u64,
    /// 次に払い出す `doc-<連番>` の連番。
    next_document: u64,
}

impl Default for LabelAllocator {
    fn default() -> Self {
        // 連番は 1 から始まる（規約の表記 `doc-<連番>` に合わせる）。
        Self {
            next_empty: 1,
            next_document: 1,
        }
    }
}

impl LabelAllocator {
    /// 種別に応じた次のラベルを払い出す。**連番は種別ごとに単調増加し、独立している。**
    fn allocate(&mut self, kind: WindowKind) -> String {
        let next = match kind {
            WindowKind::Empty => &mut self.next_empty,
            WindowKind::Document => &mut self.next_document,
        };
        let ordinal = (*next).max(1);
        *next = ordinal + 1;
        format!("{}-{ordinal}", kind.prefix())
    }
}

/// レジストリの内部状態（ロックで保護する）。
#[derive(Default)]
struct RegistryInner {
    /// ラベルからウィンドウ状態への写像。順序はラベルの辞書順で決定的である。
    windows: BTreeMap<String, WindowState>,
    /// ラベルの払い出し。
    allocator: LabelAllocator,
}

/// ウィンドウの識別子から状態への写像。**アプリ全体で 1 つだけ持つ**（`Manager::manage`）。
///
/// ウィンドウ単位の状態管理機構は Tauri に存在しないため自前で持つ（research.md
/// 「ウィンドウ管理と複数ウィンドウ」）。生成（[`open`]）が登録し、破棄
/// （[`on_window_event`]）が取り除くので、**登録は漏れない**。
pub struct WindowRegistry {
    inner: Mutex<RegistryInner>,
}

impl Default for WindowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowRegistry {
    /// 空のレジストリを作る。
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RegistryInner::default()),
        }
    }

    /// ロックを取る。**毒されていても panic しない** — ウィンドウの破棄通知はイベントループの
    /// 中で走るため、ここで panic すると他のウィンドウを巻き込む。中身は panic で壊れるような
    /// 不変条件を持たないので、そのまま使う。
    fn lock(&self) -> MutexGuard<'_, RegistryInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 新しいウィンドウのラベルを確保し、**生成中の段階で**登録する。
    ///
    /// **生成の前に呼ぶ。**生成の後に登録すると、生成直後にウィンドウが閉じられたときの破棄
    /// 通知が登録を見つけられず、登録が漏れる。生成が完了したら [`mark_ready`](Self::mark_ready)
    /// が段階を進め、失敗した場合は呼び出し元（[`spawn_creation`]）が
    /// [`remove`](Self::remove) で取り消す。
    fn allocate(&self, document: Option<PathBuf>) -> WindowState {
        let kind = if document.is_some() {
            WindowKind::Document
        } else {
            WindowKind::Empty
        };
        let mut inner = self.lock();
        let label = inner.allocator.allocate(kind);
        let state = WindowState {
            label: WindowLabel::new(label.clone()),
            document,
            phase: WindowPhase::Creating,
        };
        inner.windows.insert(label, state.clone());
        state
    }

    /// 生成の完了を記録する。登録が無ければ `false`（生成の完了前に破棄された）。
    ///
    /// **`Ready` になっても登録は取り除かない。**取り除くのは破棄の通知
    /// （[`on_window_event`]）と生成の失敗（[`spawn_creation`]）だけである。
    fn mark_ready(&self, label: &str) -> bool {
        match self.lock().windows.get_mut(label) {
            Some(state) => {
                state.phase = WindowPhase::Ready;
                true
            }
            None => false,
        }
    }

    /// 登録を取り除く。取り除けたらその状態を返す（未登録なら `None` で何も変えない）。
    pub fn remove(&self, label: &str) -> Option<WindowState> {
        self.lock().windows.remove(label)
    }

    /// 登録されているウィンドウが 1 枚も無いか。**生成中のものも数える**（生成が完了すれば
    /// ウィンドウになるため、二重に開かない判断にはこちらを使う）。
    pub fn is_empty(&self) -> bool {
        self.lock().windows.is_empty()
    }

    /// 生成中のウィンドウが 1 枚でもあるか。
    ///
    /// **あるなら、ドキュメント無しの要求は二重に開いてはならない** — 生成が完了したウィンドウが
    /// その要求を満たし、生成タスクが完了時に自ら前面に出す（[`spawn_creation`] の `focus`）。
    pub fn is_creating(&self) -> bool {
        self.lock()
            .windows
            .values()
            .any(|state| state.phase == WindowPhase::Creating)
    }

    /// 生成が完了した（ネイティブのウィンドウが存在する）1 枚を決定的に選ぶ（ラベルの辞書順で
    /// 先頭）。無ければ `None`。**生成中のものは返さない。**
    pub fn first_ready_label(&self) -> Option<WindowLabel> {
        self.lock()
            .windows
            .values()
            .find(|state| state.phase == WindowPhase::Ready)
            .map(|state| state.label.clone())
    }

    /// ラベルに関連付けられたドキュメントを引く（未登録・関連付け無しはどちらも `None`）。
    ///
    /// メニュー項目の有効・無効は**対象ウィンドウの状態**で決まる（要件 3.5。タスク 7.5）ので、
    /// メニュー側（[`crate::menu`]）がここから対象ウィンドウの関連付けを読む。**写像を二重に
    /// 持たない**ための入口である。
    pub fn document_of(&self, label: &str) -> Option<PathBuf> {
        self.lock()
            .windows
            .get(label)
            .and_then(|state| state.document.clone())
    }

    /// 記録に出すための 1 行（ラベル・関連付け・生成中の印の一覧）。
    fn describe(&self) -> String {
        let inner = self.lock();
        if inner.windows.is_empty() {
            return "(ウィンドウなし)".to_owned();
        }
        inner
            .windows
            .values()
            .map(|state| {
                let phase = match state.phase {
                    WindowPhase::Creating => "[生成中]",
                    WindowPhase::Ready => "",
                };
                format!(
                    "{}={}{phase}",
                    state.label.as_str(),
                    describe_document(state.document.as_deref()),
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// 記録に出すドキュメントの表現。関連付けが無いことを空文字ではなく明示する。
fn describe_document(document: Option<&Path>) -> String {
    match document {
        Some(path) => path.display().to_string(),
        None => "(ドキュメントなし)".to_owned(),
    }
}

// ---------------------------------------------------------------------------
// テスト（タスク 6.1）
//
// GUI を必要としない純粋な部分だけを固定する。生成そのもの（`WebviewWindowBuilder::build`、
// 破棄通知の結線、失敗の隔離）は `AppHandle` とイベントループを必要とするため、ホスト側の
// GUI 実行でしか検証できない（`tasks.md` の検証手順を参照）。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{LabelAllocator, WindowKind, WindowPhase, WindowRegistry, WindowRequest};
    use std::path::{Path, PathBuf};

    /// 検証専用の引き金（5.1 / 5.2）の文法。**`,` 区切りの並び**だけを受け付け、
    /// 空の要素・上限を超える並びは解釈できない（`macro_run_script` は何も仕込まない）。
    ///
    /// **破ると落ちる 2 つの性質**: 標本の 2 件の並び（打ち切りの後に続けて実行する形。
    /// 要件 6.4 の観測がこれに依存する）が通ること、空の要素が 1 つも通らないこと。
    #[cfg(feature = "verification-triggers")]
    #[test]
    fn the_macro_run_trigger_is_a_comma_separated_list() {
        use super::{split_macro_names, MAX_VERIFY_MACRO_NAMES};

        assert_eq!(
            split_macro_names("標本の打ち切り,標本の記入"),
            Some(vec!["標本の打ち切り", "標本の記入"]),
            "5.2 の標本の並び（2 件）が通らなければ、打ち切りの後の実行を観測できない",
        );
        assert_eq!(
            split_macro_names("標本の記入"),
            Some(vec!["標本の記入"]),
            "1 件の並びは 5.1 と同じ振る舞いである",
        );
        // 要素の前後の空白は無視する（既存の引き金の一族と同じ扱い）。
        assert_eq!(
            split_macro_names(" 標本の記入 , 標本の往復 "),
            Some(vec!["標本の記入", "標本の往復"]),
        );

        // 解釈できない値。**空の要素を 1 つも通さない**（通すと「一覧に無い名前」の観測が
        // 1 行増えるだけで、仕込みの誤りが見えなくなる）。
        assert_eq!(split_macro_names(""), None);
        assert_eq!(split_macro_names(","), None);
        assert_eq!(split_macro_names("標本の記入,"), None);
        assert_eq!(split_macro_names(",標本の記入"), None);
        assert_eq!(split_macro_names("標本の記入,,標本の失敗"), None);
        let too_many = vec!["標本の記入"; MAX_VERIFY_MACRO_NAMES + 1].join(",");
        assert_eq!(split_macro_names(&too_many), None);
    }

    #[test]
    fn the_label_convention_is_kind_prefixed_and_monotonic() {
        let mut allocator = LabelAllocator::default();
        assert_eq!(allocator.allocate(WindowKind::Empty), "empty-1");
        assert_eq!(allocator.allocate(WindowKind::Empty), "empty-2");
        assert_eq!(allocator.allocate(WindowKind::Document), "doc-1");
        assert_eq!(allocator.allocate(WindowKind::Document), "doc-2");
        // **種別ごとに独立した連番である。**ドキュメントのウィンドウを開いても、次に開く
        // ドキュメント無しのウィンドウの連番は進まない。
        assert_eq!(allocator.allocate(WindowKind::Empty), "empty-3");
    }

    #[test]
    fn a_request_carries_a_document_only_when_one_is_asked_for() {
        assert_eq!(WindowRequest::Empty.document(), None);
        assert_eq!(
            WindowRequest::Document(PathBuf::from("/tmp/one.csv")).document(),
            Some(PathBuf::from("/tmp/one.csv")),
        );
    }

    #[test]
    fn opening_a_document_registers_the_association_and_an_empty_window_has_none() {
        let registry = WindowRegistry::new();
        assert!(registry.is_empty());

        let empty = registry.allocate(None);
        assert_eq!(empty.label().as_str(), "empty-1");
        assert_eq!(empty.document(), None, "関連付けの無いウィンドウ");
        // 登録は生成より先に済む（生成中の段階で写像に入る）。
        assert_eq!(empty.phase, WindowPhase::Creating);

        let first = PathBuf::from("/tmp/one.csv");
        let document = registry.allocate(Some(first.clone()));
        assert_eq!(document.label().as_str(), "doc-1");
        assert_eq!(document.document(), Some(first.as_path()));
        assert!(!registry.is_empty());

        // **1 つのウィンドウは高々 1 つのドキュメントに関連付く**（要件 2.1）。別のドキュメントは
        // 既存のウィンドウを書き換えず、**別のウィンドウとして**現れる（要件 2.3）。
        let second = registry.allocate(Some(PathBuf::from("/tmp/two.csv")));
        assert_eq!(second.label().as_str(), "doc-2");
        assert_eq!(document.document(), Some(first.as_path()));
        assert_ne!(document.label(), second.label());
    }

    #[test]
    fn a_registered_window_is_creating_until_the_build_completes() {
        // 登録は生成より先に済む。**その間の登録を「残骸」と誤認してはならない**
        // （レビューで再現した競合の回帰防止。生成中に要求が届くと、生きているウィンドウが写像
        // から落ちて冗長な 2 枚目ができる、という壊れ方をする）。
        let registry = WindowRegistry::new();
        let state = registry.allocate(None);
        assert_eq!(state.phase, WindowPhase::Creating);
        assert_eq!(
            registry.first_ready_label(),
            None,
            "生成中のものは提示の対象にしない"
        );
        assert!(registry.is_creating());
        assert!(
            !registry.is_empty(),
            "生成中の登録も数える（二重に開かない判断に使う）"
        );

        // 生成が完了すると提示の対象になるが、**登録は取り除かれない**。
        assert!(registry.mark_ready(state.label().as_str()));
        assert!(!registry.is_creating());
        assert!(!registry.is_empty());
        assert_eq!(
            registry
                .first_ready_label()
                .map(|label| label.as_str().to_owned()),
            Some("empty-1".to_owned()),
        );

        // 生成の完了前に破棄された場合（破棄通知が先に届いた）は何も変えない。
        assert!(!registry.mark_ready("empty-9"));
    }

    #[test]
    fn removing_one_window_leaves_every_other_window_intact() {
        // 生成に失敗したウィンドウの登録を取り消しても、他のウィンドウは影響を受けない
        // （要件 2.10 の隔離をレジストリの層で固定する）。
        let registry = WindowRegistry::new();
        let empty = registry.allocate(None);
        let first = registry.allocate(Some(PathBuf::from("/tmp/one.csv")));
        let second = registry.allocate(Some(PathBuf::from("/tmp/two.csv")));
        // 生成が完了した状態にする（生成中の印は `describe` に出る）。
        for state in [&empty, &first, &second] {
            assert!(registry.mark_ready(state.label().as_str()));
        }

        let removed = registry.remove(first.label().as_str()).expect("登録済み");
        assert_eq!(removed.document(), first.document());

        assert!(
            registry.remove("doc-9").is_none(),
            "未登録のラベルを取り除いても何も変えない"
        );
        assert!(!registry.is_empty(), "他のウィンドウは残る");

        let described = registry.describe();
        assert!(
            described.contains("empty-1=(ドキュメントなし)"),
            "{described}"
        );
        assert!(described.contains("doc-2=/tmp/two.csv"), "{described}");
        assert!(
            !described.contains("doc-1="),
            "取り消した登録を残さない: {described}"
        );
        assert_eq!(second.document(), Some(Path::new("/tmp/two.csv")));
    }

    #[test]
    fn the_registry_description_marks_a_window_that_is_still_being_created() {
        // 生成中の登録が写像にあることは、記録からも見分けられる（障害解析のため）。
        let registry = WindowRegistry::new();
        let state = registry.allocate(Some(PathBuf::from("/tmp/one.csv")));
        assert!(registry.describe().contains("doc-1=/tmp/one.csv[生成中]"));
        registry.mark_ready(state.label().as_str());
        assert!(registry.describe().contains("doc-1=/tmp/one.csv"));
        assert!(!registry.describe().contains("[生成中]"));
    }

    #[test]
    fn first_ready_label_is_deterministic_and_ignores_entries_still_being_created() {
        let registry = WindowRegistry::new();
        assert_eq!(registry.first_ready_label(), None);

        let document = registry.allocate(Some(PathBuf::from("/tmp/one.csv")));
        let empty = registry.allocate(None);
        assert_eq!(
            registry.first_ready_label(),
            None,
            "どちらも生成中なので提示の対象は無い"
        );

        // 生成が完了した `doc-1` を選ぶ（`empty-1` は生成中のまま）。
        assert!(registry.mark_ready(document.label().as_str()));
        assert!(registry.is_creating(), "empty-1 は生成中のままである");
        assert_eq!(
            registry
                .first_ready_label()
                .map(|label| label.as_str().to_owned()),
            Some("doc-1".to_owned()),
        );

        assert!(registry.mark_ready(empty.label().as_str()));
        assert!(!registry.is_creating());
        // 辞書順で先頭を選ぶ（`doc-1` < `empty-1`）。選び方が実行ごとに変わらない。
        assert_eq!(
            registry
                .first_ready_label()
                .map(|label| label.as_str().to_owned()),
            Some("doc-1".to_owned()),
        );
    }

    /// 検証専用の初期画面の指定（要件 10.4 / タスク 9.7）は**初期化スクリプトのソースへ
    /// 埋め込まれる**ので、埋め込んでよい形だけを受け付ける。**引用符や改行を通すと任意の式を
    /// 混ぜられる**ため、受け付ける文字を「画面の識別子に現れるもの」に固定する。
    #[cfg(feature = "verification-triggers")]
    #[test]
    fn only_a_plain_screen_id_can_be_embedded_in_the_initialization_script() {
        use super::is_embeddable_screen_id;

        // 実際に使う識別子と、その形（ASCII の英数字と `-` `_` `.`）。
        assert!(is_embeddable_screen_id("smoke-table"));
        assert!(is_embeddable_screen_id("smoke-editor"));
        assert!(is_embeddable_screen_id("shell.initial"));
        assert!(is_embeddable_screen_id("empty-window"));
        assert!(is_embeddable_screen_id("a_b-1.2"));

        // 空・長すぎる・空白・引用符・バックスラッシュ・改行・文の終わり・括弧・非 ASCII は
        // **すべて拒否する**（ソースへ埋め込む位置に置ける文字ではない）。
        assert!(!is_embeddable_screen_id(""));
        assert!(!is_embeddable_screen_id(" smoke-table"));
        assert!(!is_embeddable_screen_id(&"a".repeat(65)));
        assert!(!is_embeddable_screen_id("smoke\";alert(1);//"));
        assert!(!is_embeddable_screen_id("smoke\\table"));
        assert!(!is_embeddable_screen_id("smoke\ntable"));
        assert!(!is_embeddable_screen_id("smoke;table"));
        assert!(!is_embeddable_screen_id("(smoke)"));
        assert!(!is_embeddable_screen_id("表"));
    }

    /// 検証専用のマクロの名前（5.1）も**初期化スクリプトのソースへ埋め込まれる**が、画面の
    /// 識別子とは違い**日本語でありうる**（標本のマクロは `標本の記入` である）。したがって
    /// 白名簿ではなく、**リテラルの外へ出る文字を拒み、リテラルの中で意味を持つ 2 文字を
    /// エスケープする**。ここが壊れると、任意の式を混ぜられる（または日本語の名前が実行できなく
    /// なる）ので、両側を固定する。
    #[cfg(feature = "verification-triggers")]
    #[test]
    fn a_macro_name_is_embedded_as_a_quoted_and_escaped_literal() {
        use super::embeddable_macro_name;

        // 標本の名前（CJK と空白。そのまま引用符で囲む）。
        assert_eq!(
            embeddable_macro_name("標本の記入").as_deref(),
            Some("\"標本の記入\"")
        );
        // リテラルの内側で意味を持つ 2 文字はエスケープする（名前としては正当である）。
        assert_eq!(
            embeddable_macro_name("say \"hi\"\\").as_deref(),
            Some("\"say \\\"hi\\\"\\\\\"")
        );
        // **リテラルの外へ出る文字は拒む**（改行・タブ・NUL・行区切り）。エスケープして実行
        // するより、拒んで記録に残す方がよい（名前は 1 行の名前である）。
        assert_eq!(embeddable_macro_name(""), None);
        assert_eq!(embeddable_macro_name("改行\nあり"), None);
        assert_eq!(embeddable_macro_name("タブ\tあり"), None);
        assert_eq!(embeddable_macro_name("nul\0あり"), None);
        assert_eq!(embeddable_macro_name("行区切り\u{2028}あり"), None);
        assert_eq!(embeddable_macro_name("段落区切り\u{2029}あり"), None);
        // 長さの上限は**バイト数**である（`is_embeddable_screen_id` と同じ趣旨）。
        assert!(embeddable_macro_name(&"a".repeat(256)).is_some());
        assert_eq!(embeddable_macro_name(&"a".repeat(257)), None);
        // 標本の名前は日本語であり 1 文字 3 バイトである（`標本の記入` は 15 バイトで内側）。
        assert!(embeddable_macro_name(&"あ".repeat(85)).is_some());
        assert_eq!(embeddable_macro_name(&"あ".repeat(86)), None);
    }

    /// 検証専用の行数の一覧（要件 4.5 / タスク 10.8）は**初期化スクリプトのソースへそのまま
    /// 埋め込まれる**ので、数字と区切り以外を受け付けない。**1 件だけの指定は受け付けない** —
    /// 1 件では「呼び出し回数が行数に比例しない」ことを示せないためである（null を返すと転送が
    /// 1 件も起きず、検査器が非 0 で落ちる）。
    #[cfg(feature = "verification-triggers")]
    #[test]
    fn only_a_plain_row_list_can_be_embedded_in_the_initialization_script() {
        use super::{parse_bulk_rows, MAX_VERIFY_BULK_ROWS};

        // 実際に使う値（10 万行規模と、比較用の小さい値）と、周囲の空白・上限の値。
        assert_eq!(parse_bulk_rows("100,100000"), Some(vec![100, 100_000]));
        assert_eq!(parse_bulk_rows(" 100 , 100000 "), Some(vec![100, 100_000]));
        assert_eq!(
            parse_bulk_rows("1,2,3,4"),
            Some(vec![1, 2, 3, 4]),
            "件数の上限までは受け付ける"
        );
        assert_eq!(
            parse_bulk_rows(&format!("{MAX_VERIFY_BULK_ROWS},{MAX_VERIFY_BULK_ROWS}")),
            Some(vec![MAX_VERIFY_BULK_ROWS, MAX_VERIFY_BULK_ROWS]),
            "行数の上限そのものは受け付ける"
        );
        assert_eq!(
            parse_bulk_rows(&format!("{},100", MAX_VERIFY_BULK_ROWS + 1)),
            None,
            "行数の上限を超える値は受け付けない"
        );

        // 1 件・0 件・空・空要素・非数・0・負・上限超え・件数超え・式を混ぜた値は**すべて拒否する**。
        assert_eq!(parse_bulk_rows("100"), None);
        assert_eq!(parse_bulk_rows(""), None);
        assert_eq!(parse_bulk_rows("   "), None);
        assert_eq!(parse_bulk_rows("100,"), None);
        assert_eq!(parse_bulk_rows(",100000"), None);
        assert_eq!(parse_bulk_rows("100,100000,"), None);
        assert_eq!(parse_bulk_rows("100,abc"), None);
        assert_eq!(parse_bulk_rows("100,1_000"), None);
        assert_eq!(parse_bulk_rows("100,0"), None);
        assert_eq!(parse_bulk_rows("100,-1"), None);
        assert_eq!(parse_bulk_rows("100,100000,1,2,3"), None);
        assert_eq!(parse_bulk_rows("100,1000001"), None);
        assert_eq!(parse_bulk_rows("100,100000];alert(1);//"), None);
        assert_eq!(parse_bulk_rows("100,1e5"), None);
    }

    // **feature の下の検査である** — 引き金の綴りの受理は `verification-triggers` の下にしか
    // 無い（既定のビルドには環境変数の読み取り自体が入らない）。同じ module の
    // `only_a_plain_row_list_can_be_embedded_in_the_initialization_script` と同じ形にする。
    #[cfg(feature = "verification-triggers")]
    #[test]
    fn only_the_exact_switch_value_turns_a_verification_trigger_on() {
        use super::is_verify_switch_on;
        // 9.2 の 2 つの引き金（グリッドの観測と、面が塗られない条件）は**真偽だけ**を運ぶ。
        // 値は初期化スクリプトのソースへ埋め込まれるので、受理する綴りは `1` ただ 1 つであり、
        // 式を混ぜられる余地を残さない（`is_embeddable_screen_id` と同じ理由である）。
        assert!(is_verify_switch_on("1"));
        assert!(is_verify_switch_on(" 1 "), "前後の空白は許す");
        for rejected in ["", "0", "true", "1+1", "1;alert(1)", "01", "１"] {
            assert!(
                !is_verify_switch_on(rejected),
                "受理してはならない値: {rejected:?}"
            );
        }
    }
}

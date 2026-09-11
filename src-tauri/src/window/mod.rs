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
//! 出ない）。強制的に失敗させる検証用の入口は `force_creation_failure` にあり、
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
}

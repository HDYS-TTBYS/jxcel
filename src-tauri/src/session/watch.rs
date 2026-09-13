//! ウィンドウの破棄の購読 — 破棄されたウィンドウのセッションを手放す（design.md
//! 「WindowDestroyWatch」）。要件: 1.5。
//!
//! # このモジュールが持つもの
//!
//! セッションを作る**適応層の 3 つの入口**（生成要求からの解決 = [`WindowDestroyWatch::resolve`]、
//! 利用者が選んだ位置の引き渡し = [`WindowDestroyWatch::attach`]、新規作成 =
//! [`WindowDestroyWatch::create`]）と、その 3 つが共有する**破棄の購読**である。手順は design.md
//! の逐語どおり「**ラベルでウィンドウを引き、購読を登録してから、表へセッションを挿入する**」。
//! 登録を先に行うので、挿入だけが済んで購読が無い状態は作られない。逆にウィンドウを引けない
//! ときは**何も作らない** — 既に破棄されたウィンドウのドキュメントを保持し続けない。
//!
//! セッションを作る経路は本型の 3 つの入口に**閉じる**。コマンド（タスク 3.4）だけでなく、
//! 宿主の連鎖（タスク 3.2 の [`super::host::SessionDocumentHost`]）も、利用者が選んだファイルの
//! 引き渡しを [`attach`](Self::attach) へ委ねる — 宿主が表を直接触ると、**購読を伴わない
//! セッション**が生まれて要件 1.5 が破れる（破棄しても文書が表に残る）。
//!
//! # なぜ「ウィンドウ 1 つにつき 1 回」を適応層が持つのか
//!
//! コアの `Slot`（`crates/document-session`）は購読の存在を知らない（Tauri 非依存であり、
//! そもそも知る必要が無い）。したがって「同じウィンドウに購読を二重に登録しない」ことは
//! **適応層側の登録済みラベルの集合**（[`registered`](WindowDestroyWatch::registered)）が保証する。
//! `resolve` は冪等であり（`DocumentSessionsApi::resolve` の契約）、読み込みに失敗したあとの
//! 再試行でも登録が増えてはならない。集合からは**破棄の通知と掃除の経路が項目を取り除く**ので、
//! 集合の中身は「今も生きている購読」と一致する。
//!
//! # なぜ掃除の経路が要るのか
//!
//! `WebviewWindow::on_window_event` は**戻り値を持たない**（tauri 2.11.5 の
//! `webview_window.rs:1524`）。したがって登録の失敗は検出できない。「取得と登録の間に
//! ウィンドウが破棄された」場合、その破棄の通知は永久に失われ、表にドキュメントが残る。
//! そこで**ウィンドウの不在を見つけたら先に表から取り除く掃除の経路**
//! （[`forget_unresolvable`](WindowDestroyWatch::forget_unresolvable)。design.md は
//! `document_state` などの入口で呼ぶことを想定している）を併せて持つ。3 つの入口も、
//! ウィンドウを引けなかったとき（または登録に失敗したとき）は同じ経路で先に落としてから
//! 失敗を返す — design.md の Risks「購読を登録したウィンドウが取得と登録の間に破棄された場合、
//! その通知は失われる。上の掃除の経路が受け皿である」の実体である。
//!
//! # app-shell の破棄の通知には触れない
//!
//! `crate::window::on_window_event` はメニューの更新とレジストリの掃除を担っており、本モジュールは
//! **その隣に自分の購読を足すだけ**である（app-shell 側の変更を要しない）。Tauri は同じウィンドウに
//! 複数の購読を許すので、どちらか一方の通知が他方を打ち消すことはない。
//!
//! # テストの縫い目（[`WindowDestroyEvents`]）
//!
//! 破棄の通知は**実行時**（イベントループとネイティブウィンドウ）にしか現れない。単体テストで
//! 使える `tauri::test` の mock runtime は購読の登録を捨てる（`MockWindowDispatcher::on_window_event`
//! は id を返すだけで、渡された閉包を保持しない。tauri 2.11.5 の `test/mock_runtime.rs:711`）。
//! したがってウィンドウの側を [`WindowDestroyEvents`] という**1 つの縫い目**に閉じ、テストは
//! 二重を駆動して破棄を起こす（`window/geometry.rs` の `GeometryRead`、`watchdog.rs` の
//! `RenderRecorder` と同じ形）。本番の実装（[`TauriWindowEvents`]）は「ラベルでウィンドウを引き、
//! `Destroyed` を選んで渡された処理へ渡す」だけの薄い写像であり、**その実物の振る舞いは実画面の
//! 観測が担う**（`verification.md`「主張は実際に動かして観測した結果で裏付ける」）。

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use app_shell::ipc::WindowLabel;
use document_session::{DocumentSessions, DocumentSessionsApi, SessionError};
use tauri::{AppHandle, Manager, Runtime, WindowEvent};
use tauri_plugin_log::log;

/// 破棄の通知を受け取る処理（ラベルを引数に取る）。
pub type DestroyHandler = Arc<dyn Fn(&WindowLabel) + Send + Sync>;

/// 破棄の購読を登録できるウィンドウの側（本番は Tauri、テストは二重）。
///
/// この縫い目が表すのは 2 つだけである:
///
/// 1. そのラベルのウィンドウの破棄を購読する（引けなければ登録せず `false` を返す）
/// 2. そのラベルのウィンドウが今も引けるか（掃除の経路が使う）
///
/// **セッションの表にも通知の中身にも触れない** — 通知で何をするかは呼び出し元
/// （[`WindowDestroyWatch`]）が決める。
pub trait WindowDestroyEvents: Send + Sync {
    /// このラベルのウィンドウの破棄を購読する。登録できたら `true`。
    ///
    /// **ウィンドウが引けないときは `false` を返し、何も登録しない**（既に破棄された
    /// ウィンドウの通知は永久に来ないので、登録したと嘘をつかない）。
    fn subscribe_destroyed(&self, label: &WindowLabel, on_destroyed: DestroyHandler) -> bool;

    /// このラベルのウィンドウが今も引けるか。
    fn has_window(&self, label: &WindowLabel) -> bool;
}

/// Tauri のウィンドウを購読する実装（本番）。
///
/// `AppHandle::get_webview_window` でラベルからウィンドウを引き、
/// [`WebviewWindow::on_window_event`](tauri::WebviewWindow::on_window_event) に
/// `WindowEvent::Destroyed` だけを選ぶ閉包を渡す。**`CloseRequested` には何もしない** —
/// 閉じてよいかの可否は `crate::window::close` の往復が担っており、破棄は「閉じた後」の
/// 事実である（拒否されたウィンドウは破棄されないので、この購読は正しい時点でだけ働く）。
pub struct TauriWindowEvents<R: Runtime> {
    /// ラベルからウィンドウを引く唯一の入口（`Manager` の実装が本番のウィンドウ表である）。
    app: AppHandle<R>,
}

impl<R: Runtime> TauriWindowEvents<R> {
    /// アプリのハンドルから作る。ハンドルは `Clone` であり、アプリの寿命と一致する。
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> WindowDestroyEvents for TauriWindowEvents<R> {
    fn subscribe_destroyed(&self, label: &WindowLabel, on_destroyed: DestroyHandler) -> bool {
        let Some(window) = self.app.get_webview_window(label.as_str()) else {
            return false;
        };
        // 通知では**表から取り除く操作（`forget`）を呼ぶ**（design.md「ウィンドウの破棄」の
        // 系列図）。ラベルは購読の単位ごとに 1 つで足りる（ウィンドウは 1 つにつき購読 1 つ）。
        let label = label.clone();
        window.on_window_event(move |event| {
            if matches!(event, WindowEvent::Destroyed) {
                on_destroyed(&label);
            }
        });
        true
    }

    fn has_window(&self, label: &WindowLabel) -> bool {
        self.app.get_webview_window(label.as_str()).is_some()
    }
}

/// ウィンドウの破棄の購読と、セッションを作る適応層の 3 つの入口（design.md
/// 「WindowDestroyWatch」）。
///
/// 表（[`DocumentSessions`]）とウィンドウの側（[`WindowDestroyEvents`]）を 1 対で持ち、
/// **セッションを作る唯一の適応層の入口**を与える。セッションを作らない読み取り（状態の写し・
/// 保存・破棄の印・閉じてよいか）は [`sessions`](Self::sessions) が返す表をそのまま使う。
///
/// `install`（`session/mod.rs`）が作った 1 実体をアプリ全体の管理状態として置き、コマンド
/// （3.4）と宿主の連鎖（3.2）が**同じ実体**を取る — 別々に作ると登録済みの集合が 2 つに割れ、
/// 「ウィンドウ 1 つにつき購読 1 つ」が保証できなくなる。
pub struct WindowDestroyWatch {
    /// ウィンドウの側（本番は [`TauriWindowEvents`]、テストは二重）。
    events: Arc<dyn WindowDestroyEvents>,
    /// セッションの表（コアの公開面）。`install` が管理状態へ置くのと**同じ実体**である。
    sessions: Arc<DocumentSessions>,
    /// 登録済みのラベルの集合（「ウィンドウ 1 つにつき購読 1 つ」の実体）。
    ///
    /// 破棄の通知の閉包も同じ集合を掴む（通知の側で項目を取り除く）ため、`Arc` で共有する。
    registered: Arc<Mutex<HashSet<String>>>,
}

impl WindowDestroyWatch {
    /// ウィンドウの側と表を結びつける。
    pub fn new(events: Arc<dyn WindowDestroyEvents>, sessions: Arc<DocumentSessions>) -> Self {
        Self {
            events,
            sessions,
            registered: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// セッションを作らない読み取り（状態の写し・保存・破棄の印・閉じてよいか）が使う表。
    ///
    /// **セッションを作る口は本型の 3 つの入口だけである。** この表を直接触って
    /// `DocumentSessionsApi::resolve` / `attach` / `create` を呼んではならない — 購読を
    /// 伴わないセッションが生まれ、破棄しても文書が表に残る（要件 1.5）。
    pub fn sessions(&self) -> &Arc<DocumentSessions> {
        &self.sessions
    }

    /// 生成要求（あれば）からセッションを確定させる（`document_state` の入口。要件 1.2）。
    // 3.4 の `document_state` がこの入口を呼ぶまでの seam（`ports.rs` の契約と同じ扱い）。
    // 表を直接触らせないために本型に置いており、削除すると 3.4 が購読を伴わない経路を作る。
    #[allow(dead_code)]
    pub fn resolve(
        &self,
        window: &WindowLabel,
        requested: Option<&Path>,
    ) -> Result<(), SessionError> {
        if !self.register(window) {
            return Err(SessionError::NoDocument);
        }
        self.sessions.resolve(window, requested)
    }

    /// 利用者が選んだ位置をそのウィンドウへ引き渡す（要件 1.3）。
    ///
    /// 宿主の連鎖（[`super::host::SessionDocumentHost`]）もこの入口を使う — 表を直接触ると
    /// 購読が登録されない。
    pub fn attach(&self, window: &WindowLabel, location: &Path) -> Result<(), SessionError> {
        if !self.register(window) {
            return Err(SessionError::NoDocument);
        }
        self.sessions.attach(window, location)
    }

    /// 行も列も無いシートを 1 つ持つドキュメントを用意する（要件 7.1）。
    // 3.4 の `document_new` がこの入口を呼ぶまでの seam（`resolve` と同じ扱い）。
    #[allow(dead_code)]
    pub fn create(&self, window: &WindowLabel) -> Result<(), SessionError> {
        if !self.register(window) {
            return Err(SessionError::NoDocument);
        }
        self.sessions.create(window)
    }

    /// 掃除の経路: ラベルのウィンドウが引けないなら、そのセッションを先に手放す。
    ///
    /// 戻り値は「手放したか」。ウィンドウが引けるときは**何もしない**（`false`）— 生存の確認と
    /// 登録を分けると、まだ生きているウィンドウのセッションを落としかねない。
    ///
    /// 適応層の入口（3.4 の `document_state` など）が、ラベルのウィンドウを引けなかったときに
    /// 呼ぶ。破棄の通知が失われていても、次の入口で必ず落ちる（design.md の Risks）。
    // 3.4 のコマンド（状態の問い合わせなど、セッションを作らない入口）がこの経路を呼ぶまでの
    // seam。`ports.rs` の `#[allow(dead_code)]` 付きの契約と同じ扱いである — **本番の入口が
    // 現れるまでの未使用**であり、削除すると 3.4 が掃除の経路を持てなくなる。
    #[allow(dead_code)]
    pub fn forget_unresolvable(&self, window: &WindowLabel) -> bool {
        if self.events.has_window(window) {
            return false;
        }
        log::info!(
            "ウィンドウが引けないためセッションを手放した: label={}",
            window.as_str()
        );
        self.forget(window);
        true
    }

    /// そのウィンドウの購読の登録と、セッションの保持を取り除く（通知と掃除の共有部分）。
    fn forget(&self, window: &WindowLabel) {
        release(&self.registered, &self.sessions, window);
    }

    /// **ラベルでウィンドウを引き、購読を登録する**（表への挿入は呼び出し元の入口が行う）。
    ///
    /// 戻り値は「登録が生きているか」。`false` のときは**表からも取り除いてある**ので、
    /// 呼び出し元はセッションを作らずに失敗を返す（何も作らない）。
    ///
    /// 登録は登録済みの集合で 1 回に抑える（`resolve` の反復と、失敗後の再試行で増えない）。
    fn register(&self, window: &WindowLabel) -> bool {
        let already = self
            .registered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(window.as_str());
        if already {
            // 登録済みでも、ウィンドウが消えていれば通知が失われている（掃除して何も作らない）。
            if self.events.has_window(window) {
                return true;
            }
            log::debug!(
                "登録済みのウィンドウが引けないためセッションを手放す: label={}",
                window.as_str()
            );
            self.forget(window);
            return false;
        }

        // **登録を先に行う。** 閉包は集合と表を掴み、通知では `forget` と同じ後始末をする
        // （`registered` を保持したまま登録しない — 通知が同期で届いても再入しない）。
        let registered = Arc::clone(&self.registered);
        let sessions = Arc::clone(&self.sessions);
        let handler: DestroyHandler = Arc::new(move |window: &WindowLabel| {
            release(&registered, &sessions, window);
            log::info!(
                "ウィンドウの破棄でセッションを手放した: label={}",
                window.as_str()
            );
        });
        if !self.events.subscribe_destroyed(window, handler) {
            // 取得と登録の間に破棄された。**先に表から取り除いてから**失敗を返す。
            log::debug!(
                "破棄の購読を登録できないためセッションを手放す: label={}",
                window.as_str()
            );
            self.forget(window);
            return false;
        }
        self.registered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(window.as_str().to_owned());
        log::info!("破棄の購読を登録した: label={}", window.as_str());
        true
    }
}

/// そのウィンドウの登録とセッションを取り除く（**破棄の通知と掃除の経路が共有する唯一の実体**）。
///
/// 順序は「登録 → 表」であり、表の取り除き（`forget`）はそのウィンドウのセッションだけを
/// 手放す（他のウィンドウの文書と未保存の状態を変えない。要件 1.5）。**記録はここでは出さない**
/// — 呼び出し元が「破棄」か「掃除」かで文言を分ける（同じ関数を使う 2 つの経路の区別を
/// 記録から読み取れるようにする）。
fn release(
    registered: &Mutex<HashSet<String>>,
    sessions: &DocumentSessions,
    window: &WindowLabel,
) {
    registered
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(window.as_str());
    sessions.forget(window);
}

// ---------------------------------------------------------------------------
// テスト（タスク 3.3）
// ---------------------------------------------------------------------------

/// テスト専用のウィンドウの側（**本番の実装ではない**）。
///
/// 宿主の連鎖（`session/host.rs`）のテストは、セッションを作る入口を通すために
/// [`WindowDestroyEvents`] の実装を要する。破棄の観測そのものは本モジュールの `tests` が
/// 専用の二重で行うので、ここは「どのラベルも引ける」という最小の答えだけを与える。
#[cfg(test)]
pub(crate) mod testing {
    use super::{DestroyHandler, WindowDestroyEvents};
    use app_shell::ipc::WindowLabel;

    /// 常に「引ける」と答えるウィンドウの側（登録も常に成功する）。
    ///
    /// **破棄を起こす口を持たない** — 破棄の観測は本モジュールの `tests` の二重が担う。
    /// 宿主のテストが確かめるのは連鎖の順序（セッション → 内側）であり、破棄ではない。
    #[derive(Debug, Default)]
    pub(crate) struct AlwaysPresent;

    impl WindowDestroyEvents for AlwaysPresent {
        fn subscribe_destroyed(&self, _label: &WindowLabel, _on_destroyed: DestroyHandler) -> bool {
            true
        }

        fn has_window(&self, _label: &WindowLabel) -> bool {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use document_format::{CellValue, Document, DocumentFormat, DocumentFormatApi, SchemaPart};
    use document_session::SessionState;

    use super::*;

    /// テストが駆動する二重のウィンドウの側。
    ///
    /// 破棄は [`destroy`](Self::destroy) が**登録済みの処理を呼ぶ**ことで起こす（Tauri の
    /// イベントループの代わり）。`subscribe_fails` は「取得と登録の間の破棄」を再現する。
    #[derive(Default)]
    struct FakeWindows {
        /// 今も引けるウィンドウのラベル。
        existing: Mutex<HashSet<String>>,
        /// 登録された破棄の処理（ラベルごと）。
        handlers: Mutex<HashMap<String, DestroyHandler>>,
        /// 登録の**呼び出し履歴**（ラベルごとの回数）。重複登録の観測に使う。
        subscriptions: Mutex<HashMap<String, usize>>,
        /// `true` なら [`WindowDestroyEvents::subscribe_destroyed`] が登録に失敗する
        /// （取得と登録の間にウィンドウが破棄された状況）。
        subscribe_fails: bool,
    }

    impl FakeWindows {
        /// 与えられたラベルのウィンドウが引ける状態で作る。
        fn with(labels: &[&str]) -> Arc<Self> {
            let existing = labels.iter().map(|label| (*label).to_owned()).collect();
            Arc::new(Self {
                existing: Mutex::new(existing),
                ..Self::default()
            })
        }

        /// 取得と登録の間に破棄が起きる状態で作る。
        fn with_failing_subscription(labels: &[&str]) -> Arc<Self> {
            let existing = labels.iter().map(|label| (*label).to_owned()).collect();
            Arc::new(Self {
                existing: Mutex::new(existing),
                subscribe_fails: true,
                ..Self::default()
            })
        }

        /// ウィンドウを破棄する（引ける集合から外し、登録済みの処理を呼ぶ）。
        fn destroy(&self, label: &str) {
            self.existing
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(label);
            let handler = self
                .handlers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(label);
            if let Some(handler) = handler {
                handler(&WindowLabel::new(label));
            }
        }

        /// ウィンドウを**通知なしで**引けなくする（取得と登録の間に破棄された状況）。
        fn forget_silently(&self, label: &str) {
            self.existing
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(label);
        }

        /// そのラベルに購読が登録された回数。
        fn subscription_count(&self, label: &str) -> usize {
            self.subscriptions
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(label)
                .copied()
                .unwrap_or(0)
        }
    }

    impl WindowDestroyEvents for FakeWindows {
        fn subscribe_destroyed(&self, label: &WindowLabel, on_destroyed: DestroyHandler) -> bool {
            if self.subscribe_fails {
                return false;
            }
            if !self
                .existing
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains(label.as_str())
            {
                return false;
            }
            *self
                .subscriptions
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(label.as_str().to_owned())
                .or_insert(0) += 1;
            self.handlers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(label.as_str().to_owned(), on_destroyed);
            true
        }

        fn has_window(&self, label: &WindowLabel) -> bool {
            self.existing
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains(label.as_str())
        }
    }

    /// 二重と表から購読を作る（テストの標準の組み立て）。
    fn watch_with(fake: Arc<FakeWindows>) -> (WindowDestroyWatch, Arc<DocumentSessions>) {
        let sessions = Arc::new(DocumentSessions::new());
        (
            WindowDestroyWatch::new(fake, Arc::clone(&sessions)),
            sessions,
        )
    }

    /// 登録済みのラベルの集合（ソート済み）。
    fn registered(watch: &WindowDestroyWatch) -> Vec<String> {
        let mut labels: Vec<String> = watch
            .registered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect();
        labels.sort();
        labels
    }

    /// 一時ディレクトリを作る（プロセスごとに一意。`document-session` の `Scratch` と同じ規律）。
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("時計は 1970 以降である")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "jxcel-session-watch-{tag}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
            Self { path }
        }

        fn file(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// 1 シート 1 行の**本物の文書**を書く（dev-dependency の `document-format` を使う）。
    fn write_document(path: &Path, sheet_name: &str, value: &str) {
        let mut document = Document::new();
        let sheet = document.add_sheet(sheet_name);
        document
            .set_sheet_columns(sheet, vec!["note".to_owned()])
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, SchemaPart::empty())
            .expect("標本のシートは実在する");
        let row = document.add_row(sheet).expect("標本のシートは実在する");
        document
            .set_row_values(sheet, row, vec![CellValue::Text(value.to_owned())])
            .expect("標本の行は実在する");
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
    }

    /// 3 つの入口はいずれも**購読を登録してから**表へ挿入する（要件 1.2、1.3、7.1）。
    #[test]
    fn every_creating_entry_registers_the_subscription() {
        let scratch = Scratch::new("entries");
        let path = scratch.file("opened.jxcel");
        write_document(&path, "開いた", "入口");

        let fake = FakeWindows::with(&["doc-1", "empty-1", "empty-2"]);
        let (watch, sessions) = watch_with(Arc::clone(&fake));

        watch
            .resolve(&WindowLabel::new("doc-1"), Some(&path))
            .expect("生成要求から解決できる");
        watch
            .attach(&WindowLabel::new("empty-1"), &path)
            .expect("引き渡しを受け取れる");
        watch
            .create(&WindowLabel::new("empty-2"))
            .expect("新規作成はセッションを用意する");

        for label in ["doc-1", "empty-1", "empty-2"] {
            assert_eq!(1, fake.subscription_count(label), "{label} の購読が無い");
            assert!(
                matches!(
                    sessions.state(&WindowLabel::new(label)),
                    SessionState::Open { .. }
                ),
                "{label} のセッションが用意されていない"
            );
        }
        assert_eq!(
            vec!["doc-1".to_owned(), "empty-1".to_owned(), "empty-2".to_owned()],
            registered(&watch)
        );
    }

    /// **(a) ウィンドウが破棄されたら、そのラベルの状態が「保持していない」へ戻る**（要件 1.5）。
    #[test]
    fn a_destroyed_window_returns_to_absent() {
        let scratch = Scratch::new("destroyed");
        let path = scratch.file("held.jxcel");
        write_document(&path, "保持", "破棄される");

        let fake = FakeWindows::with(&["doc-1"]);
        let (watch, sessions) = watch_with(Arc::clone(&fake));
        let label = WindowLabel::new("doc-1");
        watch.attach(&label, &path).expect("引き渡しを受け取れる");
        assert!(matches!(sessions.state(&label), SessionState::Open { .. }));

        fake.destroy("doc-1");

        assert_eq!(
            SessionState::Absent,
            sessions.state(&label),
            "破棄されたウィンドウの状態が「保持していない」へ戻っていない"
        );
        assert!(registered(&watch).is_empty(), "登録済みのラベルが残っている");
        // 文書そのものも表から落ちている（読み取りが「保持していない」で失敗する）。
        assert!(
            matches!(
                sessions.read(&label, &mut |_| ()),
                Err(SessionError::NoDocument)
            ),
            "破棄された窓の文書がまだ読める"
        );
    }

    /// **(b) 破棄は他のウィンドウの文書と未保存の状態を変えない**（要件 1.5）。
    #[test]
    fn a_destroy_leaves_the_other_windows_untouched() {
        let scratch = Scratch::new("destroyed-others");
        let first = scratch.file("first.jxcel");
        let second = scratch.file("second.jxcel");
        write_document(&first, "最初", "破棄される");
        write_document(&second, "二番目", "残る");

        let fake = FakeWindows::with(&["doc-1", "doc-2"]);
        let (watch, sessions) = watch_with(Arc::clone(&fake));
        let one = WindowLabel::new("doc-1");
        let two = WindowLabel::new("doc-2");
        watch.attach(&one, &first).expect("引き渡しを受け取れる");
        watch.attach(&two, &second).expect("引き渡しを受け取れる");
        sessions.edit(&two, &mut |_| ()).expect("変更を適用できる");
        let before = sessions.state(&two);
        assert!(
            matches!(before, SessionState::Open { unsaved: true, .. }),
            "2 つ目の窓が未保存になっていない: {before:?}"
        );

        fake.destroy("doc-1");

        assert_eq!(SessionState::Absent, sessions.state(&one));
        assert_eq!(
            before,
            sessions.state(&two),
            "破棄が他のウィンドウの文書と未保存の状態を変えた"
        );
        // 残った窓の文書は中身まで読める（表から落ちていない）。
        let note = sessions
            .read(&two, &mut |document| {
                match &document.sheets()[0].rows()[0].values()[0] {
                    CellValue::Text(text) => text.clone(),
                    other => panic!("標本の値がテキストでない: {other:?}"),
                }
            })
            .expect("残った窓の文書を読める");
        assert_eq!("残る", note);
        assert_eq!(vec!["doc-2".to_owned()], registered(&watch));
    }

    /// **(c) ウィンドウが引けないときは、入口が先に表から取り除いてから失敗を返す**。
    ///
    /// 取得と登録の間に破棄された場合の受け皿である（`on_window_event` は戻り値を持たず、
    /// 登録の失敗を検出できない）。**何も作らない**ことも併せて見る。
    #[test]
    fn a_creating_entry_cleans_up_a_window_that_cannot_be_resolved() {
        let scratch = Scratch::new("entry-cleanup");
        let path = scratch.file("held.jxcel");
        write_document(&path, "保持", "取り残される");

        let fake = FakeWindows::with(&["doc-1"]);
        let (watch, sessions) = watch_with(Arc::clone(&fake));
        let label = WindowLabel::new("doc-1");
        watch.attach(&label, &path).expect("引き渡しを受け取れる");
        assert!(matches!(sessions.state(&label), SessionState::Open { .. }));

        // 取得と登録の間に破棄された（通知は届かない）。
        let subscriptions_before = fake.subscription_count("doc-1");
        fake.forget_silently("doc-1");

        // どの入口も、セッションを作らずに先に掃除する。
        assert!(
            matches!(watch.create(&label), Err(SessionError::NoDocument)),
            "ウィンドウが無いのに新規作成が成功した"
        );
        assert_eq!(
            SessionState::Absent,
            sessions.state(&label),
            "掃除の経路が表から取り除いていない"
        );
        assert!(registered(&watch).is_empty(), "登録済みのラベルが残っている");

        // 解決と引き渡しの入口でも同じである（セッションを作らない）。
        assert!(
            matches!(
                watch.resolve(&label, Some(&path)),
                Err(SessionError::NoDocument)
            ),
            "ウィンドウが無いのに解決が成功した"
        );
        assert!(
            matches!(watch.attach(&label, &path), Err(SessionError::NoDocument)),
            "ウィンドウが無いのに引き渡しが成功した"
        );
        assert_eq!(SessionState::Absent, sessions.state(&label));
        assert_eq!(
            subscriptions_before,
            fake.subscription_count("doc-1"),
            "いない窓を購読した"
        );
    }

    /// **(c) 掃除の経路そのもの**: 引けないウィンドウのセッションを先に手放す。
    #[test]
    fn the_cleanup_path_releases_a_session_whose_window_is_gone() {
        let scratch = Scratch::new("cleanup-path");
        let path = scratch.file("held.jxcel");
        write_document(&path, "保持", "取り残される");

        let fake = FakeWindows::with(&["doc-1", "doc-2"]);
        let (watch, sessions) = watch_with(Arc::clone(&fake));
        let one = WindowLabel::new("doc-1");
        let two = WindowLabel::new("doc-2");
        watch.attach(&one, &path).expect("引き渡しを受け取れる");
        watch.attach(&two, &path).expect("引き渡しを受け取れる");
        sessions.edit(&two, &mut |_| ()).expect("変更を適用できる");
        let other = sessions.state(&two);

        fake.forget_silently("doc-1");

        assert!(
            watch.forget_unresolvable(&one),
            "引けないウィンドウのセッションが手放されていない"
        );
        assert_eq!(SessionState::Absent, sessions.state(&one));
        assert_eq!(vec!["doc-2".to_owned()], registered(&watch));
        assert_eq!(other, sessions.state(&two), "他の窓を変えた");

        // **引けるウィンドウでは何もしない**（掃除の経路が空回りしていないことの対照）。
        assert!(
            !watch.forget_unresolvable(&two),
            "引けるウィンドウのセッションを手放した"
        );
        assert_eq!(other, sessions.state(&two));
        assert_eq!(vec!["doc-2".to_owned()], registered(&watch));
    }

    /// **(d) 同じウィンドウに購読は 1 つだけ**（`resolve` の反復と、失敗のあとの再試行）。
    #[test]
    fn repeated_resolves_register_the_subscription_once() {
        let scratch = Scratch::new("one-subscription");
        let path = scratch.file("opened.jxcel");
        let missing = scratch.file("missing.jxcel");
        write_document(&path, "開いた", "冪等");

        let fake = FakeWindows::with(&["doc-1", "doc-2"]);
        let (watch, _sessions) = watch_with(Arc::clone(&fake));
        let label = WindowLabel::new("doc-1");

        // 冪等な解決の反復（2 回目は読み込みを起こさない）。
        watch.resolve(&label, Some(&path)).expect("1 度目");
        watch.resolve(&label, Some(&path)).expect("2 度目");
        assert_eq!(1, fake.subscription_count("doc-1"), "購読が二重に登録された");

        // 読み込みに失敗したあとの再試行でも増えない。**コアは覚えた失敗を繰り返さない**ので
        // 2 度目は `Ok` であり（`Slot::resolve` の冪等）、購読も二重にならない。
        let retry = WindowLabel::new("doc-2");
        assert!(
            matches!(
                watch.resolve(&retry, Some(&missing)),
                Err(SessionError::Read { .. })
            ),
            "1 度目の解決が失敗として報告されない"
        );
        assert!(
            watch.resolve(&retry, Some(&missing)).is_ok(),
            "再試行が読み直した"
        );
        assert_eq!(1, fake.subscription_count("doc-2"), "再試行で購読が増えた");
        assert_eq!(
            vec!["doc-1".to_owned(), "doc-2".to_owned()],
            registered(&watch)
        );
    }

    /// **登録を先に行う**: 登録に失敗したら表へ挿入しない（「取得と登録の間の破棄」）。
    #[test]
    fn a_failed_registration_does_not_insert_a_session() {
        let scratch = Scratch::new("registration-failure");
        let path = scratch.file("opened.jxcel");
        write_document(&path, "開いた", "登録できない");

        let fake = FakeWindows::with_failing_subscription(&["doc-1"]);
        let (watch, sessions) = watch_with(Arc::clone(&fake));
        let label = WindowLabel::new("doc-1");

        assert!(
            matches!(
                watch.resolve(&label, Some(&path)),
                Err(SessionError::NoDocument)
            ),
            "購読を登録できないのに解決が成功した"
        );
        assert_eq!(
            SessionState::Absent,
            sessions.state(&label),
            "購読の無いセッションが挿入された"
        );
        assert!(registered(&watch).is_empty());
    }
}

//! グリッドのコマンド面 — 5 つの封筒つきコマンドと、**ドメイン型 ⇄ 境界用の型の変換を行う
//! 唯一の場所**（タスク 6.2。design.md「GridCommands」の API Contract、要件 3.3、4.4、
//! 8.3、8.4、9.2、9.3）。
//!
//! # 5 つの経路
//!
//! | コマンド | 経路 | 何を答えるか |
//! |---|---|---|
//! | [`grid_open_sheet`] | 文書から対象シートを引き、スキーマを計画へ落として [`GridSession::open`] | 列の構成とシートの行数（要件 1.1、1.5、1.6） |
//! | [`grid_set_view`] | [`GridSession::set_view`]（＋展開の適用） | 可視行数・隠された行数・違反の総数（要件 8.3、8.4、8.7） |
//! | [`grid_apply_edit`] | [`GridSession::apply`] | 影響範囲・型強制・違反・行数（要件 3.3、3.5） |
//! | [`grid_history`] | [`GridSession::undo`] / [`GridSession::redo`] | 同じ要約（要件 9.2、9.3） |
//! | [`grid_find_violation`] | [`GridSession::find_violation`] | 次の違反の位置と理由（要件 4.2、4.4、4.5） |
//!
//! **呼び出し元ウィンドウは基盤が注入する [`WebviewWindow`] から取る**（ペイロードで
//! 受け取らない ＝ 偽装できない。要件 4.6、`ipc-contract.md`）。したがって 5 つとも要求の型に
//! ウィンドウは現れない — 要求が運ぶのは操作の対象（シートの識別子・表示の指定・編集命令・
//! 進める向き・探索の起点）だけである。
//!
//! # 失敗の載せ方（design.md「Error Handling」の表）
//!
//! **利用者の入力の結果は封筒の成功腕に載る。** 型に合わない値を保持して違反として返すこと
//! （要件 3.5）、「これ以上違反が無い」こと（要件 4.4）、「進める履歴が無い」こと
//! （要件 9.2、9.3）は、いずれも**正常な結果**であり、[`IpcError`] の腕には載せない。
//!
//! **封筒の失敗腕へ落ちるのは、操作の誤りと経路そのものの失敗だけである**:
//!
//! - 操作の誤り — [`GridError`]（範囲外のセル・解釈できない入れ子の表現・未知の行）。
//!   `design.md` の同表の「操作の誤り」の行がこれを定める（画面は再要求する）
//! - 経路そのものの失敗 — そのウィンドウにドキュメントが無い（[`SessionError::NoDocument`]）、
//!   要求されたシートが文書に無い、宣言が壊れていて計画へ落とせない、グリッドがまだ
//!   開かれていない、行の識別子が解釈できない。どれも「コマンドの経路が成立しない」であり、
//!   ドメインの判定結果ではない。
//!
//! 理由の文言を組み立てるのは適応層の仕事である（`session/commands.rs` と同じ規律。
//! `GridError` / `SessionError` / `ViolationReason` はいずれも表示用の文言を持たない）。
//!
//! # 唯一の変換の場所
//!
//! 境界の型（`app_shell::ipc::grid`）とドメインの型（`data-grid` / `schema-engine`）を写すのは
//! **本モジュールだけ**である。とくに次の 2 つは本モジュールの責任である:
//!
//! 1. **違反の総数をシート全体へ閉じる。** [`GridEditOutcome::violation_total`] は
//!    **再検証した列に閉じた**総数である（6.1 の同型の doc）。シート全体の総数を保つのは
//!    [`GridSession`] であり（違反の差分を載せる唯一の所有者）、本モジュールは
//!    [`GridSession::violation_total`] を読んで境界へ載せる（要件 4.3、11.4。
//!    **そのために検証を呼び直すことは無い**）
//! 2. **型の種別の札の対応。** [`TypeKindTag`] は `app-shell` にあり（他のドメインクレートに
//!    依存できない）、`schema-engine` の [`TypeKind`] は本クレートでしか見られない。
//!    したがって**対応を検査できる唯一の場所が本モジュール**であり、[`type_kind_tag`] が
//!    全変種を網羅する `match`（ワイルドカード無し ＝ 総関数）と、その像が
//!    [`TypeKindTag::ALL`] と綴り・件数・並びの 3 点で一致することを検査するテストを持つ。
//!
//! # ウィンドウごとの `GridSession`（design.md「GridCommands」の Integration）
//!
//! **ウィンドウごとに 1 つ保持し、ウィンドウが閉じたら破棄する。** 保持するのは
//! [`GridSessions`] であり、表（ラベル → 保持）と破棄の購読を 1 対で持つ。表のロックは
//! **参照と挿入・除去のためだけ**に取り、処理の間は保持しない（10 万行の適用が他の
//! ウィンドウを待たせない。`document-session` の表と同じ規律）— ウィンドウごとの実体は
//! [`Arc`] で持ち、そこに 1 つずつロックを置く。
//!
//! **破棄の購読は [`WindowDestroyEvents`] の縫い目を通す**（セッションの層が確立した形を
//! そのまま使う。テストは二重（`session/watch.rs` の `testing::AlwaysPresent`）を駆動する）。購読は登録済みの
//! ラベルの集合で 1 回に抑え、破棄の通知と、登録できなかったときの掃除が同じ後始末を通る。
//!
//! # 実行モデル（5 つとも主スレッドの外で走らせる）
//!
//! **5 つとも `#[tauri::command(async)]` である。** これは関数を非同期にするのではなく、
//! **同期の本体を Tauri のブロッキング用のスレッドプールで走らせる**印である（Tauri の
//! 既定では同期コマンドは IPC の処理の中＝主スレッドで走る）。
//!
//! 理由は費用である。`grid_set_view` は最初の 1 回に**シート全体の検証**を行って違反の索引を
//! 組み立て（10 万行 × 30 列で約 255 ミリ秒。design.md「Performance & Scalability」の実測）、
//! `grid_apply_edit` と `grid_history` は 1 万行の貼り付けとその取り消しを運びうる（要件 11.5 の
//! 予算は 3 秒）。主スレッドをその間占めると、**どの操作でも待たされない**という要件 11 の
//! 目的そのものが壊れる（描画も入力も止まる）。
//!
//! **`State` を引数に取らない**のはこのためである（借用はスレッドを跨げない）。管理状態は
//! 本体の中で `app.state::<...>()` から取る — 引数の型は `AppHandle` / `WebviewWindow` /
//! 要求だけで、いずれも所有権ごと渡せる。
//!
//! **応答の形は変わらない。** 封筒（[`IpcResult`]）を返すことは、経路がどこで走るかに
//! 依らない（要件 4.4）。
//!
//! # スキーマはどこから来るか（要件 1.1、1.2）
//!
//! [`GridSession::open`] は `SheetId` と [`CompiledSchema`] を要する（design.md「GridSession」の
//! Service Interface）。文書を保持しているのは `document-session` であり、シートとその
//! ルートスキーマは `document-format` の文書が持つ。したがって本モジュールは
//! **[`DocumentSessionsApi::read`] の閉包の内側で**シートを引き、`schema-engine` の
//! [`SchemaEngineApi::compile`] で計画へ落とす — **その文書のそのシートから**落とすことが
//! 計画の事前条件である（別のシートの計画を渡すと列の添字が意味を失う）。
//!
//! 計画を保持し続けるのは、違反の理由（要件 4.2）を組み立てるときに列をもう一度判定する
//! ためである。**セッションは計画を外へ出さない**（`GridSession` の欄はすべて私有）ため、
//! 本モジュールが同じ計画を持つ。
//!
//! # このクレートが `document-format` を通常依存に持たないこと（`session/verification.rs` と同じ規律）
//!
//! `src-tauri` は `document-format` を**通常依存に持たない**（テストだけが dev-dependency と
//! して使う。`src-tauri/Cargo.toml` の依存方針）。したがって本モジュールは
//! **`document-format` の名前を 1 つも書かない** — 文書のシート・行数・識別子は
//! `document-session` の公開面（`read` / `edit` の閉包が与える `&Document`）を通して触り、
//! 型は文脈から推論させる。シートの識別子も、文書の側の文字列表現と要求の文字列を突き合わせる
//! ことで選ぶ（識別子の型を名指しする必要が無い）。
//!
//! # テストの形
//!
//! Tauri の実体（`WebviewWindow` / `AppHandle`）を要するのはコマンド関数の 5 つだけであり、
//! 中身は**すべて本体の関数**（[`answer_open`] / [`answer_set_view`] / [`answer_apply_edit`] /
//! [`answer_history`] / [`answer_find_violation`]）へ切り出してある。テストはそれらを直接
//! 駆動する — GUI を起こさず、**本物の文書**（`document-format` は dev-dependency）と、
//! 破棄の購読の二重（`session/watch.rs` の `testing::AlwaysPresent`）だけで足りる。
//! 5 つのコマンド関数そのものの形（注入の 2 引数と要求、封筒の戻り値）は、関数の型を
//! 書いたテストがコンパイル時に固定する。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use app_shell::ipc::{
    command_names, ColumnDescriptor, ColumnElementCount, ColumnExpandability, GridCellAddress,
    GridCoercionNotice, GridEditCommand, GridEditOutcome, GridEditRequest, GridEditResponse,
    GridExpansionState, GridFilterSpec, GridHistoryDirection, GridHistoryRequest, GridOpenRequest,
    GridOpenResponse, GridPathSegment, GridSearchDirection, GridSheetSummary, GridViewRequest,
    GridViewResponse, GridViewSpec, GridViolation, GridViolationLocation, GridViolationRequest,
    GridViolationResponse, IpcError, IpcResult, TypeKindTag, WindowContext, WindowLabel,
};
use data_grid::{
    display_text, CellAddress, CoercionNotice, ColumnIndex, EditCommand, EditOutcome, ElementCount,
    Expandability, ExpansionState, FilterSpec, GridError, GridSession, LayoutColumn,
    NestedPathSegment, RowOrdinal, SearchDirection, SortKey, ViewSpec,
};
use document_session::{DocumentSessions, DocumentSessionsApi, SessionError};
use schema_engine::{
    CompiledSchema, Expected, SchemaEngine, SchemaEngineApi, TypeKind, TypeRegistry,
    ValidationOptions, ValuePathSegment, Violation, ViolationReason,
};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_log::log;

use crate::session::watch::{TauriWindowEvents, WindowDestroyEvents};

// ---------------------------------------------------------------------------
// ウィンドウごとの保持（design.md「GridCommands」）
// ---------------------------------------------------------------------------

/// 1 つのウィンドウが表示しているシートのセッション（design.md「GridCommands」の
/// 「ウィンドウごとに 1 つ保持する」）。
///
/// セッションのほかに**計画**（[`CompiledSchema`]）と、**表示中のシートの識別子**
/// （要求と突き合わせるための文字列）を持つ。計画を持つのは違反の理由（要件 4.2）を
/// 組み立てるときに列をもう一度判定するためであり、識別子を持つのは理由を組み立てる
/// 時点で「どのシートか」を要求からではなく保持から取るためである（要求は開いたときの
/// ものであり、以後のコマンドは持ち回らない）。
struct SheetEntry {
    /// 表示しているシートのセッション（表示状態・履歴・違反の索引を所有する）。
    session: GridSession,
    /// 開いた時点の計画。**同じシートから**落としたものである
    /// （[`SchemaEngineApi::compile`] の事前条件）。
    schema: CompiledSchema,
    /// 表示しているシートの識別子（文書の側の文字列表現と突き合わせる）。
    sheet: String,
}

/// ウィンドウごとの [`SheetEntry`] の表と、破棄の購読（design.md「GridCommands」）。
///
/// **表のロックは参照と挿入・除去のためだけに取る。** ウィンドウごとの実体は
/// [`Arc<Mutex<SheetEntry>>`] であり、処理（`apply` / `set_view` / 探索）はそちらのロックの
/// 下で行う — 10 万行の適用が**他のウィンドウのコマンドを待たせない**ようにするためである
/// （`document-session` の表と同じ規律。要件 1.4）。
///
/// # 破棄の購読（design.md「GridCommands」の Integration）
///
/// 破棄の通知は**実行時**（イベントループとネイティブウィンドウ）にしか現れないため、
/// ウィンドウの側を [`WindowDestroyEvents`] の縫い目に閉じる（`session/watch.rs` が確立した
/// 形をそのまま使う）。購読は**セッションを置くときに 1 回だけ**登録し、登録できなければ
/// 何も置かない（登録の済んでいない保持を作らない）。通知の側ではそのウィンドウの保持だけを
/// 落とす（他のウィンドウの表示状態と履歴を変えない）。
pub struct GridSessions {
    /// ウィンドウの側（本番は [`TauriWindowEvents`]、テストは二重）。
    events: Arc<dyn WindowDestroyEvents>,
    /// ラベル → 保持。**反復しない**（観測に出る順序を持たない）ため [`HashMap`] で足りる。
    /// 破棄の通知の閉包も同じ表を掴む（通知の側で項目を取り除く）ため、[`Arc`] で共有する。
    entries: Arc<Mutex<HashMap<WindowLabel, Arc<Mutex<SheetEntry>>>>>,
    /// 購読を登録したラベル（「ウィンドウ 1 つにつき購読 1 つ」の実体）。破棄の通知の閉包も
    /// 同じ集合を掴む（通知の側で項目を取り除く）ため、[`Arc`] で共有する。
    subscribed: Arc<Mutex<HashSet<String>>>,
}

impl GridSessions {
    /// ウィンドウの側と空の表を結びつける。
    pub fn new(events: Arc<dyn WindowDestroyEvents>) -> Self {
        Self {
            events,
            entries: Arc::new(Mutex::new(HashMap::new())),
            subscribed: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// そのウィンドウの保持を**表のロックを離してから**使えるように取り出す（無ければ `None`）。
    ///
    /// 返るのはウィンドウごとの実体（[`Arc`]）であり、処理はその内側のロックの下で行う。
    fn entry(&self, label: &WindowLabel) -> Option<Arc<Mutex<SheetEntry>>> {
        lock(&self.entries).get(label).cloned()
    }

    /// セッションを置き（同じウィンドウの前の保持は置き換える）、破棄の購読を登録する。
    ///
    /// 戻り値は「置けたか」。`false` のときは**何も置いていない**（ウィンドウを引けない、
    /// または取得と登録の間に破棄された）ので、呼び出し元はセッションを作らずに失敗を返す。
    fn store(&self, label: &WindowLabel, entry: SheetEntry) -> bool {
        if !self.subscribe(label) {
            return false;
        }
        lock(&self.entries).insert(label.clone(), Arc::new(Mutex::new(entry)));
        true
    }

    /// そのウィンドウの購読の登録を 1 回だけ行う（`session/watch.rs` の `register` と同じ形）。
    ///
    /// **登録を先に行い、保持はそのあとに置く。** こうしないと、挿入だけが済んで購読の無い
    /// 保持が生まれ、ウィンドウが閉じても落ちない（design.md「GridCommands」の
    /// Integration。セッションの層が破棄の購読で同じ順序を守っている）。ウィンドウを
    /// 引けないときは購読を登録せずに `false` を返し、**呼び出し元が置かない**。
    fn subscribe(&self, label: &WindowLabel) -> bool {
        if lock(&self.subscribed).contains(label.as_str()) {
            return true;
        }

        // 閉包は集合と表を掴み、通知では `forget` と同じ後始末をする。
        let subscribed = Arc::clone(&self.subscribed);
        let entries = Arc::clone(&self.entries);
        let handler: crate::session::watch::DestroyHandler =
            Arc::new(move |window: &WindowLabel| {
                release(&subscribed, &entries, window);
                log::info!(
                    "ウィンドウの破棄でグリッドの保持を手放した: label={}",
                    window.as_str()
                );
            });
        if !self.events.subscribe_destroyed(label, handler) {
            // 取得と登録の間に破棄された。**先に後始末をしてから**失敗を返す。
            log::debug!(
                "破棄の購読を登録できないためグリッドの保持を手放す: label={}",
                label.as_str()
            );
            self.forget(label);
            return false;
        }
        lock(&self.subscribed).insert(label.as_str().to_owned());
        log::info!("グリッドの破棄の購読を登録した: label={}", label.as_str());
        true
    }

    /// そのウィンドウの購読の登録と保持を取り除く（通知と掃除の共有部分）。
    fn forget(&self, label: &WindowLabel) {
        release(&self.subscribed, &self.entries, label);
    }
}

/// そのウィンドウの登録と保持を取り除く（**破棄の通知と掃除の経路が共有する唯一の実体**）。
fn release(
    subscribed: &Mutex<HashSet<String>>,
    entries: &Mutex<HashMap<WindowLabel, Arc<Mutex<SheetEntry>>>>,
    window: &WindowLabel,
) {
    lock(subscribed).remove(window.as_str());
    lock(entries).remove(window);
}

/// ロックを取る（毒された場合も中身を使う。パニックの伝播より、表を読めることを優先する）。
///
/// `session/watch.rs` と同じ扱いである — ロックの内側でパニックする経路を持たないため、
/// 毒は実質的に起きないが、起きたときに他のウィンドウの操作まで巻き添えにしない。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// 管理状態（アプリ全体で 1 実体）
// ---------------------------------------------------------------------------

/// 管理状態の生成を直列化する（下の [`grid_state`] を参照）。
static GRID_STATE_CREATION: Mutex<()> = Mutex::new(());

/// アプリ全体で 1 実体の表を取る。**初回に作る**（`lifecycle::run` は本タスクの境界の外に
/// あり、起動時に `manage` する行を足せない）。
///
/// 作るのは 1 回だけである（[`GRID_STATE_CREATION`] が確認と生成を直列化する。
/// `Manager::manage` は既に同じ型があれば上書きせず `false` を返すので、二重に作っても
/// **最初の 1 つが正**である）。ドキュメントの側（`session::install`）が作る
/// `Arc<WindowDestroyWatch>` とは別の実体である — あちらは文書の保持を、こちらは表示状態と
/// 履歴を所有し、寿命も破棄の購読も別である。
///
/// **検証専用の分岐は無い。** 表は常にこの 1 経路で作られる。
fn grid_state(app: &AppHandle) -> State<'_, GridSessions> {
    if app.try_state::<GridSessions>().is_none() {
        let _guard = lock(&GRID_STATE_CREATION);
        if app.try_state::<GridSessions>().is_none() {
            let events: Arc<dyn WindowDestroyEvents> =
                Arc::new(TauriWindowEvents::new(app.clone()));
            let _ = app.manage(GridSessions::new(events));
            log::info!("グリッドの表を管理状態として置いた");
        }
    }
    app.state::<GridSessions>()
}

/// 文書を保持している表（セッションの層が管理状態へ置いたもの）を取り出す。
///
/// **セッションと同じ実体**を使う（別に作ると「開いているドキュメント」の真実が 2 つに割れる。
/// `session/mod.rs` の module doc）。
fn documents_of(app: &AppHandle) -> Arc<DocumentSessions> {
    Arc::clone(
        app.state::<Arc<crate::session::watch::WindowDestroyWatch>>()
            .sessions(),
    )
}

/// Tauri が注入した呼び出し元ウィンドウを、境界の文脈（要件 4.6）へ写す。
///
/// **境界の型を新設しない。** `app_shell::ipc::WindowContext` / `WindowLabel` をそのまま使う
/// （3 つ目の識別子を作らない。`session/commands.rs` と同じ形）。
fn caller_context(window: &WebviewWindow) -> WindowContext {
    WindowContext {
        window: WindowLabel::new(window.label()),
    }
}

// ---------------------------------------------------------------------------
// 失敗の写像（文言を組み立てるのは適応層の仕事である）
// ---------------------------------------------------------------------------

/// 経路そのものが成立しないことを伝える失敗（封筒の失敗腕）。
fn path_failure(command: &str, label: &WindowLabel, reason: &str) -> IpcError {
    IpcError::Document {
        message: format!(
            "{command}: ウィンドウ {} の経路が成立しない: {reason}",
            label.as_str()
        ),
    }
}

/// グリッドがまだ開かれていないことを伝える失敗（要件 1.1 の順序）。
///
/// `grid_open_sheet` を通らずに表示の指定・編集・履歴・探索を呼んだ場合である。**経路の
/// 失敗**であり、ドメインの判定結果ではない（画面は開いてから呼び直す）。
fn not_open(command: &str, label: &WindowLabel) -> IpcError {
    path_failure(command, label, "グリッドがまだ開かれていない")
}

/// ドメインの誤りを封筒の失敗腕へ写す（design.md「Error Handling」の「操作の誤り」）。
///
/// **値の不適合はここへ来ない** — 違反は値を保持したまま成功腕の要約に載る（要件 3.5）。
/// ここへ来るのは範囲外のセル・未知の行・解釈できない入れ子の表現であり、画面は要求を
/// 直して呼び直す（`design.md` の同表）。
fn grid_failure(command: &str, label: &WindowLabel, error: &GridError) -> IpcError {
    path_failure(command, label, &format!("グリッドを進められない: {error}"))
}

/// セッションの誤りを封筒の失敗腕へ写す。
///
/// **保持していない**（[`SessionError::NoDocument`]）は利用者が画面を開いた順序の問題であり、
/// 読み込みの失敗は文書そのものの問題である。どちらも「このコマンドの経路が成立しない」
/// であり、判定の結果ではない（`session/commands.rs` の `document_state` が同じ失敗を
/// **状態として**答えるのと対照的である — あちらは状態の問い合わせであり、こちらは操作である）。
fn session_failure(command: &str, label: &WindowLabel, error: &SessionError) -> IpcError {
    let reason = match error {
        SessionError::NoDocument => "ドキュメントを保持していない".to_owned(),
        SessionError::Busy => "別の操作が進行中である".to_owned(),
        SessionError::UnsavedChanges => "未保存の変更が解決されていない".to_owned(),
        SessionError::Read { source } => format!("ドキュメントを読めない: {source}"),
    };
    path_failure(command, label, &reason)
}

/// 件数を境界の [`u32`] へ写す（64 ビット整数を境界へ出さない規約。`ipc/document.rs`）。
///
/// 上限を超える値は**飽和させる** — `as` による切り捨ては、嘘の小さい数を利用者へ見せる
/// （32 ビットを超えるシートは現実には作れないが、写像が黙って壊れる形にはしない。
/// `session/commands.rs` の `count_to_u32` と同じ判断である）。
fn count_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

// ---------------------------------------------------------------------------
// 境界からドメインへの変換（要求の側）
// ---------------------------------------------------------------------------

/// 表示の指定をドメインの [`ViewSpec`] へ写す（要件 8.3、8.4）。
fn view_spec(view: &GridViewSpec) -> ViewSpec {
    ViewSpec {
        sort: view
            .sort
            .iter()
            .map(|key| SortKey {
                column: column_index(key.column),
                descending: key.descending,
            })
            .collect(),
        filters: view.filters.iter().map(filter_spec).collect(),
    }
}

/// 並べ替えの基準列 1 本を写す。
fn filter_spec(filter: &GridFilterSpec) -> FilterSpec {
    match filter {
        GridFilterSpec::Equals { column, text } => FilterSpec::Equals {
            column: column_index(*column),
            text: text.clone(),
        },
        GridFilterSpec::Contains { column, text } => FilterSpec::Contains {
            column: column_index(*column),
            text: text.clone(),
        },
        GridFilterSpec::IsEmpty { column } => FilterSpec::IsEmpty {
            column: column_index(*column),
        },
        GridFilterSpec::IsNotEmpty { column } => FilterSpec::IsNotEmpty {
            column: column_index(*column),
        },
        // **列を問わない指定と、列を指定した要求を区別する**（6.1 の同型の doc）。
        GridFilterSpec::HasViolation { column } => FilterSpec::HasViolation {
            column: column.map(column_index),
        },
    }
}

/// 展開の状態 1 列ぶんを写す（要件 5.1〜5.4）。
fn expansion_state(state: &GridExpansionState) -> ExpansionState {
    ExpansionState {
        column: column_index(state.column),
        expanded: state.expanded,
        depth: state.depth,
    }
}

/// 境界の列の添字をドメインの [`ColumnIndex`] へ写す。
///
/// 範囲の検査はここでしない — 範囲外の列はドメインの判定（`ColumnOutOfRange`）が答える
/// （境界で 2 つ目の規則を作らない）。
fn column_index(column: u32) -> ColumnIndex {
    ColumnIndex::new(column as usize)
}

/// 行の識別子の文字列を、行の識別子の型へ解釈する。失敗は**理由の文字列**である
/// （封筒への写像は呼び出し元が 1 箇所で行う。`path_failure` の doc）。
///
/// **型を名指ししない**（`src-tauri` は `document-format` を通常依存に持たない。module doc
/// 「このクレートが `document-format` を通常依存に持たないこと」）。解釈できない文字列は
/// 経路の失敗である（ドメインの判定に到達しない）。
fn parse_row<T>(text: &str) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    text.parse()
        .map_err(|error| format!("編集命令の行の識別子 {text} を解釈できない: {error}"))
}

/// 境界のセルの位置をドメインの [`CellAddress`] へ写す（要件 3.3、8.6）。
fn cell_address(cell: &GridCellAddress) -> Result<CellAddress, String> {
    Ok(CellAddress::new(
        parse_row(&cell.row)?,
        column_index(cell.column),
    ))
}

/// 行の識別子の並びを写す（行を取り除く・複製する命令と、貼り付けの表示の並び）。
fn row_ids<T>(rows: &[String]) -> Result<Vec<T>, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    rows.iter().map(|row| parse_row(row)).collect()
}

/// 編集命令をドメインの [`EditCommand`] へ写す（要件 3.3、5.7、6.1、6.3、7.3、8.9）。
///
/// 6 つの命令を過不足なく写す（ワイルドカードを使わない — `data-grid` に命令が増えれば
/// ここがコンパイルエラーになり、境界の形を追随させ忘れない）。**値を型付きで運ばない**
/// 規約は 6.1 の型が既に守っているため、ここは文字列と位置をそのまま渡す。
///
/// 失敗は**理由の文字列**である（封筒への写像は [`answer_apply_edit`] が 1 箇所で行う）。
fn edit_command(command: &GridEditCommand) -> Result<EditCommand, String> {
    Ok(match command {
        GridEditCommand::SetCells { cells } => {
            let mut converted = Vec::with_capacity(cells.len());
            for cell in cells {
                converted.push((cell_address(&cell.cell)?, cell.text.clone()));
            }
            EditCommand::SetCells { cells: converted }
        }
        GridEditCommand::SetNested { cell, json } => EditCommand::SetNested {
            cell: cell_address(cell)?,
            json: json.clone(),
        },
        GridEditCommand::InsertRows { at, count } => EditCommand::InsertRows {
            at: RowOrdinal::new(*at as usize),
            count: *count as usize,
        },
        GridEditCommand::RemoveRows { rows } => EditCommand::RemoveRows {
            rows: row_ids(rows)?,
        },
        GridEditCommand::DuplicateRows { rows } => EditCommand::DuplicateRows {
            rows: row_ids(rows)?,
        },
        GridEditCommand::PasteRange { anchor, rows, text } => EditCommand::PasteRange {
            anchor: cell_address(anchor)?,
            rows: row_ids(rows)?,
            text: text.clone(),
        },
    })
}

// ---------------------------------------------------------------------------
// ドメインから境界への変換（応答の側）
// ---------------------------------------------------------------------------

/// 型の種別を境界の札へ写す（**総関数**。要件 3.1、3.2、10.1〜10.4）。
///
/// ワイルドカードを書かないため、[`TypeKind`] に変種が増えればここがコンパイルエラーになり、
/// 境界の札を追随させ忘れない。**綴りは変種名そのまま**であり（[`TypeKindTag`] の doc が
/// 定める）、`match` の腕が写すのは名前だけである — 綴りが食い違えば
/// 同名のテスト（`type_kind_tag_covers_every_type_kind`）が落ちる。
fn type_kind_tag(kind: TypeKind) -> TypeKindTag {
    match kind {
        TypeKind::Int => TypeKindTag::Int,
        TypeKind::Float => TypeKindTag::Float,
        TypeKind::Decimal => TypeKindTag::Decimal,
        TypeKind::Text => TypeKindTag::Text,
        TypeKind::Bool => TypeKindTag::Bool,
        TypeKind::Date => TypeKindTag::Date,
        TypeKind::DateTime => TypeKindTag::DateTime,
        TypeKind::Enum => TypeKindTag::Enum,
        TypeKind::Ref => TypeKindTag::Ref,
        TypeKind::Attachment => TypeKindTag::Attachment,
        TypeKind::Object => TypeKindTag::Object,
        TypeKind::Array => TypeKindTag::Array,
        TypeKind::Any => TypeKindTag::Any,
        TypeKind::Custom => TypeKindTag::Custom,
    }
}

/// 展開の可否を写す（3 状態を潰さない。要件 5.4）。
fn expandability_to_boundary(expandability: Expandability) -> ColumnExpandability {
    match expandability {
        Expandability::Available => ColumnExpandability::Available,
        Expandability::Capped => ColumnExpandability::Capped,
        Expandability::Leaf => ColumnExpandability::Leaf,
    }
}

/// 要素数の能力を写す（要件 5.6）。**`None` は開いた端点**であり 0 とは違う（6.1 の doc）。
fn element_count_to_boundary(count: &ElementCount) -> ColumnElementCount {
    ColumnElementCount {
        items: type_kind_tag(count.items),
        min: count.min.map(count_to_u32),
        max: count.max.map(count_to_u32),
    }
}

/// 入れ子の内側の位置の 1 段を、境界の段へ写す（`view` 層の段から。要件 4.5、5.5）。
fn nested_segment(segment: &NestedPathSegment) -> GridPathSegment {
    match segment {
        NestedPathSegment::Field(name) => GridPathSegment::Field {
            name: name.to_string(),
        },
        NestedPathSegment::Index(position) => GridPathSegment::Index {
            position: count_to_u32(*position),
        },
    }
}

/// 違反の内側の位置の 1 段を写す（`schema-engine` の段から）。[`nested_segment`] と同じ形へ
/// 落とすが、**別の型から**写す（`data-grid` の段と検証の段は寿命も変更の理由も違う。
/// `types` の module doc）。
fn value_path_segment(segment: &ValuePathSegment) -> GridPathSegment {
    match segment {
        ValuePathSegment::Field(name) => GridPathSegment::Field {
            name: name.to_string(),
        },
        ValuePathSegment::Index(position) => GridPathSegment::Index {
            position: count_to_u32(*position),
        },
    }
}

/// 構成の 1 列を写す（要件 1.1、1.2、3.1、5.1、5.4、5.6）。
fn column_to_boundary(column: &LayoutColumn) -> ColumnDescriptor {
    ColumnDescriptor {
        column: count_to_u32(column.column.index()),
        path: column.path.segments().iter().map(nested_segment).collect(),
        name: column.name.clone(),
        kind: column.kind.map(type_kind_tag),
        element_count: column.element_count.as_ref().map(element_count_to_boundary),
        expandability: expandability_to_boundary(column.expandability),
    }
}

/// シートの要約を組み立てる（要件 1.1、1.5、1.6）。
///
/// **列の構成と行数を同じ型に載せる**（2 つの空の状態を列の数で区別する。6.1 の
/// `GridSheetSummary` の doc）。行数は**シートの行数**であり、可視行数ではない
/// （絞り込みの結果は [`GridViewResponse`] が運ぶ）。
fn sheet_summary(session: &GridSession, rows: usize) -> GridSheetSummary {
    GridSheetSummary {
        columns: session.columns().iter().map(column_to_boundary).collect(),
        row_count: count_to_u32(rows),
    }
}

/// セルの位置を写す（境界では行は文字列、列は [`u32`]）。
fn cell_to_boundary(cell: CellAddress) -> GridCellAddress {
    GridCellAddress {
        row: cell.row().to_string(),
        column: count_to_u32(cell.column().index()),
    }
}

/// 型強制の記録を写す（要件 3.4）。**前後の表示文字列はドメインが既に持っている**
/// （本モジュールは組み立て直さない）。
fn coercion_to_boundary(notice: &CoercionNotice) -> GridCoercionNotice {
    GridCoercionNotice {
        cell: cell_to_boundary(notice.cell),
        before: notice.before.clone(),
        after: notice.after.clone(),
    }
}

/// 違反の位置を写す（要件 4.2、4.5、6.4、7.5）。
///
/// 行は**文字列**（64 ビット整数を境界へ出さない規約）、列は [`u32`]、内側の位置は段の並びで
/// ある。行を持たない違反（列そのものの問題）は `None` として写る。
fn violation_location(violation: &Violation) -> GridViolationLocation {
    GridViolationLocation {
        row: violation.row().map(|row| row.to_string()),
        column: count_to_u32(violation.column().index()),
        path: violation
            .path()
            .segments()
            .iter()
            .map(value_path_segment)
            .collect(),
    }
}

/// 編集の要約を写す（要件 3.4、4.3、4.6、6.2、6.4、7.5）。
///
/// **違反の総数はシート全体の数を載せる**（要件 4.3）。[`EditOutcome::violation_total`] は
/// 再検証した列に閉じた総数であり（6.1 の同型の欄の doc）、シート全体へ閉じるのは本モジュール
/// の仕事である — 差分で最新に保たれている数を [`GridSession::violation_total`] から読む。
/// **そのために検証を呼び直すことは無い**（要件 11.4）。
fn outcome_to_boundary(session: &GridSession, outcome: &EditOutcome) -> GridEditOutcome {
    GridEditOutcome {
        affected: outcome.affected.iter().map(|row| row.to_string()).collect(),
        coercions: outcome.coercions.iter().map(coercion_to_boundary).collect(),
        violation_total: count_to_u32(session.violation_total()),
        violations: outcome.violations.iter().map(violation_location).collect(),
        revalidated_columns: outcome
            .revalidated_columns
            .iter()
            .map(|column| count_to_u32(column.index()))
            .collect(),
        row_count: count_to_u32(outcome.row_count),
    }
}

/// 期待した内容を、利用者へ伝える語へ組み立てる（要件 4.2）。
///
/// ドメインの [`Expected`] は表示用の文言を持たない（`schema-engine` の doc）ため、
/// 語を組み立てるのは適応層の仕事である。**変種ごとに書き分ける** — 利用者が取るべき行動が
/// 型・範囲・書式・選択肢で異なるためである（1 つの文言に畳まない）。
///
/// 値の表示文字列は `data-grid` の `display_text` から取る — **表示の規則の写しを作らない**
/// （画面が窓に載せる文字列と同じものである。要件 4.2 の文言が画面と食い違わない）。
/// 値の型（`document-format` の `CellValue`）は**名指ししない**（本クレートは通常依存に
/// 持たないため、閉包と引数の型は文脈から推論させる）。
fn describe_expected(expected: &Expected) -> String {
    match expected {
        Expected::Kind(kind) => format!("{kind} 型"),
        Expected::Range { min, max } => match (min, max) {
            (Some(min), Some(max)) => {
                format!("{}〜{} の範囲", display_text(min), display_text(max))
            }
            (Some(min), None) => format!("{} 以上", display_text(min)),
            (None, Some(max)) => format!("{} 以下", display_text(max)),
            (None, None) => "範囲の指定".to_owned(),
        },
        Expected::Length { min, max } => match (min, max) {
            (Some(min), Some(max)) => format!("長さ {min}〜{max} 文字"),
            (Some(min), None) => format!("長さ {min} 文字以上"),
            (None, Some(max)) => format!("長さ {max} 文字以下"),
            (None, None) => "長さの指定".to_owned(),
        },
        Expected::Pattern(pattern) => format!("書式 {pattern} に一致する文字列"),
        Expected::Decimal { precision, scale } => {
            format!("有効桁数 {precision}・小数点以下 {scale} 桁の 10 進数")
        }
        Expected::Choices(choices) => format!("選択肢 {}", choices.join(" / ")),
        Expected::Present => "値なしを許さないこと".to_owned(),
        Expected::Unique => "一意であること".to_owned(),
        Expected::RowsOf(sheet) => format!("シート {sheet} に実在する行"),
        Expected::Usable => "使用可能な列であること".to_owned(),
        Expected::AcceptedBy(identifier) => format!("拡張型 {identifier} が受理すること"),
    }
}

/// 違反の理由を、利用者へ伝える文言へ組み立てる（要件 4.2）。
///
/// **変種ごとに書き分ける**（12 変種。`Expected` の語は [`describe_expected`] に委ねる）。
/// 値は表示文字列として埋め込む（画面と同じ見え方にする。`display_text` を呼ぶ理由は
/// [`describe_expected`] の doc を参照）。
fn describe_reason(reason: &ViolationReason) -> String {
    match reason {
        ViolationReason::TypeMismatch { expected, actual } => format!(
            "{}の値を期待したが「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::OutOfRange { expected, actual } => format!(
            "{}の外の値「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::LengthOutOfRange { expected, actual } => format!(
            "{}に合わない長さの値「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::PatternMismatch { expected, actual } => format!(
            "{}に一致しない値「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::ChoiceNotAllowed { expected, actual } => format!(
            "{}に無い値「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::PrecisionExceeded { expected, actual } => format!(
            "{}を超える値「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        // 値なしそのものが違反である（`actual` はつねに値なしなので、文言へ埋め込まない）。
        ViolationReason::MissingValue { .. } => {
            "値なしを許さない列またはフィールドに値が無い".to_owned()
        }
        ViolationReason::Duplicate {
            expected,
            actual,
            rows,
        } => format!(
            "{}に反する値「{}」が {} 行にある",
            describe_expected(expected),
            display_text(actual),
            rows.len()
        ),
        ViolationReason::BrokenReference {
            expected, actual, ..
        } => format!(
            "{}が実在しない（参照先の値は「{}」）",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::UnusableColumn { kind, .. } => {
            format!("列の型 {kind} を解釈できないため、この列の値は使えない")
        }
        ViolationReason::CustomRejected { reason, .. } => {
            format!("拡張型が値を拒否した: {reason}")
        }
        ViolationReason::CustomFailed { reason, .. } => {
            format!("拡張型の判定が失敗した: {reason}")
        }
    }
}

// ---------------------------------------------------------------------------
// 本体（Tauri に依らない。テストはここを直接駆動する）
// ---------------------------------------------------------------------------

/// シートを開く本体（[`grid_open_sheet`] の中身。要件 1.1、1.5、1.6）。
///
/// 手順は 3 つである:
///
/// 1. **文書を読む**（[`DocumentSessionsApi::read`]）。保持していなければ経路の失敗である
/// 2. 閉包の内側で**シートを識別子で引き**、ルートスキーマを計画へ落とし（
///    [`SchemaEngineApi::compile`]）、[`GridSession::open`] でセッションを作る
/// 3. 表へ置く（破棄の購読が登録できなければ**置かずに**失敗を返す）
///
/// **同じウィンドウで 2 度呼ぶと前の保持を置き換える**（別のシートへ切り替える経路である）。
/// 置き換えでも購読は増えない（登録済みの集合が 1 回に抑える）。
pub(crate) fn answer_open(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridOpenRequest,
) -> IpcResult<GridOpenResponse, IpcError> {
    let command = command_names::GRID_OPEN_SHEET;
    let opened = documents.read(label, &mut |document| {
        let Some(sheet) = document
            .sheets()
            .iter()
            .find(|sheet| sheet.id().to_string() == request.sheet)
        else {
            return Err(format!("シート {} が文書に無い", request.sheet));
        };
        let schema = SchemaEngine::new()
            .compile(sheet, &TypeRegistry::new())
            .map_err(|error| format!("シート {} の宣言を解釈できない: {error}", request.sheet))?;
        let session = GridSession::open(sheet.id(), schema.clone())
            .map_err(|error| format!("シート {} のグリッドを開けない: {error}", request.sheet))?;
        let summary = sheet_summary(&session, sheet.rows().len());
        Ok((
            SheetEntry {
                session,
                schema,
                sheet: request.sheet.clone(),
            },
            summary,
        ))
    });

    match opened {
        Ok(Ok((entry, sheet))) => {
            if !grids.store(label, entry) {
                return IpcResult::Err {
                    error: path_failure(command, label, "破棄の購読を登録できない"),
                };
            }
            IpcResult::Ok {
                data: GridOpenResponse {
                    context: WindowContext {
                        window: label.clone(),
                    },
                    sheet,
                },
            }
        }
        Ok(Err(reason)) => IpcResult::Err {
            error: path_failure(command, label, &reason),
        },
        Err(error) => IpcResult::Err {
            error: session_failure(command, label, &error),
        },
    }
}

/// 表示の指定を変える本体（[`grid_set_view`] の中身。要件 8.3、8.4、8.7）。
///
/// **要求は完全な記述である。** 前の指定のうち今回の要求に現れない展開は折りたたみへ戻す —
/// 戻さないと、要求から消えた列が展開されたまま残り、「空の指定は展開無し」という 6.1 の
/// 規約が破れる（前の指定が黙って生き続ける）。**文書は読むだけである**（要件 8.5）。
pub(crate) fn answer_set_view(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridViewRequest,
) -> IpcResult<GridViewResponse, IpcError> {
    let command = command_names::GRID_SET_VIEW;
    let context = WindowContext {
        window: label.clone(),
    };
    let Some(entry) = grids.entry(label) else {
        return IpcResult::Err {
            error: not_open(command, label),
        };
    };
    let mut entry = lock(&entry);

    // 1. 要求に現れない展開を折りたたみへ戻す（要求は完全な記述である）。
    let requested: HashSet<u32> = request
        .view
        .expansion
        .iter()
        .map(|state| state.column)
        .collect();
    let previous: Vec<ExpansionState> = entry.session.expansion().to_vec();
    for state in previous {
        if !requested.contains(&count_to_u32(state.column.index())) {
            entry
                .session
                .set_expansion(ExpansionState::collapsed(state.column));
        }
    }

    // 2. 順序と索引（`set_view` は可視行の並びと違反の索引を導出する）。
    let spec = view_spec(&request.view);
    let view = documents.read(label, &mut |document| {
        entry.session.set_view(document, spec.clone())
    });
    let summary = match view {
        Ok(Ok(summary)) => summary,
        Ok(Err(error)) => {
            return IpcResult::Err {
                error: grid_failure(command, label, &error),
            }
        }
        Err(error) => {
            return IpcResult::Err {
                error: session_failure(command, label, &error),
            }
        }
    };

    // 3. 展開の適用（列の構成が変わる。`set_view` のあとに行う）。
    for state in &request.view.expansion {
        entry.session.set_expansion(expansion_state(state));
    }

    IpcResult::Ok {
        data: GridViewResponse {
            context,
            visible_rows: count_to_u32(summary.visible),
            hidden_rows: count_to_u32(summary.hidden),
            violation_total: count_to_u32(entry.session.violation_total()),
        },
    }
}

/// 編集を適用する本体（[`grid_apply_edit`] の中身。要件 3.3、3.4、3.5、4.3、4.6）。
///
/// **命令の変換は閉包の外で済ませる。** 変換できない要求（解釈できない行の識別子）で
/// 文書へ触れると、何も変えていないのに未保存の印が立つ — [`DocumentSessionsApi::edit`] は
/// 閉包が失敗を返しても印を立てる（閉包が文書を変えたかを判定できないため保守側に倒す）から
/// である。したがって境界からドメインへの写像は先に済ませ、失敗はそこで返す。
///
/// 適用そのものは `document-session` の**可変の貸出口**を通す（文書を変える唯一の経路。
/// 未保存の印と版も同じ臨界区間の内側で記録される）。
pub(crate) fn answer_apply_edit(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridEditRequest,
) -> IpcResult<GridEditResponse, IpcError> {
    let command = command_names::GRID_APPLY_EDIT;
    let context = WindowContext {
        window: label.clone(),
    };
    let Some(entry) = grids.entry(label) else {
        return IpcResult::Err {
            error: not_open(command, label),
        };
    };
    let mut entry = lock(&entry);

    let mut converted = match edit_command(&request.command) {
        Ok(converted) => Some(converted),
        Err(reason) => {
            return IpcResult::Err {
                error: path_failure(command, label, &reason),
            }
        }
    };
    let applied = documents.edit(label, &mut |document| {
        // `FnMut` の閉包は持ち物を move できないため、命令は 1 度だけ取り出す。
        // **`edit` の閉包は高々 1 回しか呼ばれない**（`DocumentSessionsApi::edit` の契約）。
        let command = converted.take().expect("適用の閉包は 1 回だけ呼ばれる");
        entry.session.apply(document, command)
    });

    match applied {
        Ok(edited) => match edited.value {
            Ok(outcome) => IpcResult::Ok {
                data: GridEditResponse {
                    context,
                    outcome: Some(outcome_to_boundary(&entry.session, &outcome)),
                },
            },
            Err(error) => IpcResult::Err {
                error: grid_failure(command, label, &error),
            },
        },
        Err(error) => IpcResult::Err {
            error: session_failure(command, label, &error),
        },
    }
}

/// 履歴を進める本体（[`grid_history`] の中身。要件 9.2、9.3）。
///
/// **進める向きは要求が言う。** 取り消しとやり直しは同じ経路（履歴と適用を束ねた口）を通り、
/// どちらも [`EditOutcome`] を返すため、応答の形は編集の適用と同じである。
///
/// **進める履歴が無いことは失敗ではない** — `outcome` を `None` にして成功腕で答える
/// （要件 9.2、9.3。画面は「取り消せる操作が無い」ことを失敗として扱わない）。
pub(crate) fn answer_history(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridHistoryRequest,
) -> IpcResult<GridEditResponse, IpcError> {
    let command = command_names::GRID_HISTORY;
    let context = WindowContext {
        window: label.clone(),
    };
    let Some(entry) = grids.entry(label) else {
        return IpcResult::Err {
            error: not_open(command, label),
        };
    };
    let mut entry = lock(&entry);

    let direction = request.direction;
    let advanced = documents.edit(label, &mut |document| match direction {
        GridHistoryDirection::Undo => entry.session.undo(document),
        GridHistoryDirection::Redo => entry.session.redo(document),
    });

    match advanced {
        Ok(edited) => match edited.value {
            Ok(Some(outcome)) => IpcResult::Ok {
                data: GridEditResponse {
                    context,
                    outcome: Some(outcome_to_boundary(&entry.session, &outcome)),
                },
            },
            Ok(None) => IpcResult::Ok {
                data: GridEditResponse {
                    context,
                    outcome: None,
                },
            },
            Err(error) => IpcResult::Err {
                error: grid_failure(command, label, &error),
            },
        },
        Err(error) => IpcResult::Err {
            error: session_failure(command, label, &error),
        },
    }
}

/// 次の違反を探す本体（[`grid_find_violation`] の中身。要件 4.2、4.4、4.5）。
///
/// 手順は 3 つである:
///
/// 1. **[`GridSession::find_violation`] が位置を答える**（可視行の序数から最も近い違反セルへ。
///    索引を読むだけなので、表示範囲の外にある違反にも到達する。要件 4.4）
/// 2. **理由を組み立てる**（要件 4.2）。理由を持つのは判定だけであり、セッションは
///    違反そのものを外へ出さない。そこで**見つかった列をもう一度だけ判定**し
///    （[`SchemaEngineApi::validate_columns`]。全件検証ではない）、その列の報告から
///    見つかったセルの違反を選んで文言へ写す
/// 3. **見つからなければ `None` を返す** — 「これ以上違反が無い」は正常な結果である
///
/// 手順 2 の判定は**列に閉じた 1 回**である。理由は利用者が明示的に指示したときにだけ要る
/// （要件 4.2）ため、走査の経路（窓の符号化）でこの費用を払うことはない。
pub(crate) fn answer_find_violation(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridViolationRequest,
) -> IpcResult<GridViolationResponse, IpcError> {
    let command = command_names::GRID_FIND_VIOLATION;
    let context = WindowContext {
        window: label.clone(),
    };
    let Some(entry) = grids.entry(label) else {
        return IpcResult::Err {
            error: not_open(command, label),
        };
    };
    let entry = lock(&entry);

    let from = RowOrdinal::new(request.from as usize);
    let direction = match request.direction {
        GridSearchDirection::Forward => SearchDirection::Forward,
        GridSearchDirection::Backward => SearchDirection::Backward,
    };
    let Some(found) = entry.session.find_violation(from, direction) else {
        return IpcResult::Ok {
            data: GridViolationResponse {
                context,
                violation: None,
            },
        };
    };

    let column = found.column();
    let row = found.row().to_string();
    // 理由は判定だけが持つ。**見つかった列に閉じた 1 回の判定**で写す（全件検証ではない）。
    let composed = documents.read(label, &mut |document| {
        let sheet = document
            .sheets()
            .iter()
            .find(|sheet| sheet.id().to_string() == entry.sheet)?;
        let report = SchemaEngine::new().validate_columns(
            document,
            sheet.id(),
            &entry.schema,
            &[column],
            &ValidationOptions::default(),
        );
        report
            .violations()
            .iter()
            .find(|violation| {
                violation.column() == column
                    && violation.row().map(|row| row.to_string()) == Some(row.clone())
            })
            .map(|violation| GridViolation {
                location: violation_location(violation),
                reason: describe_reason(violation.reason()),
            })
    });

    let violation = match composed {
        Ok(violation) => violation,
        Err(error) => {
            return IpcResult::Err {
                error: session_failure(command, label, &error),
            }
        }
    };
    IpcResult::Ok {
        data: GridViolationResponse { context, violation },
    }
}

// ---------------------------------------------------------------------------
// コマンド面（5 つ）
// ---------------------------------------------------------------------------

/// 呼び出し元ウィンドウに表示するシートを開く（要件 1.1、1.5、1.6、4.6）。
///
/// 呼び出し元は注入された [`WebviewWindow`] から取る（要件 4.6）。**`grid_open_sheet` が
/// 通らなければ他の 4 つは失敗する** — 表示状態・履歴・違反の索引はこの経路が作る。
///
/// 開いたセッションは**ウィンドウごとに 1 つ**保持され、ウィンドウが閉じたら破棄される
/// （[`GridSessions`]）。
#[tauri::command(async)]
pub fn grid_open_sheet(
    app: AppHandle,
    window: WebviewWindow,
    request: GridOpenRequest,
) -> IpcResult<GridOpenResponse, IpcError> {
    let command = command_names::GRID_OPEN_SHEET;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_open(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / シート = {} / 列 = {} / 行 = {}",
            context.window.as_str(),
            request.sheet,
            data.sheet.columns.len(),
            data.sheet.row_count,
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / シート = {} / 失敗 = {error}",
            context.window.as_str(),
            request.sheet,
        ),
    }
    result
}

/// 呼び出し元ウィンドウのグリッドの表示の指定を変える（要件 8.3、8.4、8.5、8.7）。
///
/// **ドキュメントは変わらない**（要件 8.5）— 文書は読むだけで、表示の指定はセッションが持つ。
#[tauri::command(async)]
pub fn grid_set_view(
    app: AppHandle,
    window: WebviewWindow,
    request: GridViewRequest,
) -> IpcResult<GridViewResponse, IpcError> {
    let command = command_names::GRID_SET_VIEW;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_set_view(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / 可視 = {} / 隠れ = {} / 違反 = {}",
            context.window.as_str(),
            data.visible_rows,
            data.hidden_rows,
            data.violation_total,
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

/// 呼び出し元ウィンドウのグリッドへ編集命令を 1 つ適用する（要件 3.3、3.4、3.5、4.6）。
///
/// 判定は `schema-engine` が行い（本モジュールは分岐を持たない）、**適合しない値も破棄せず
/// 違反として返す**（要件 3.5）。`WriteOrigin::Edit` は決して拒否しないため
/// （`schema-engine` 要件 6.1）、「編集が失敗して値が戻る」経路は存在しない。
#[tauri::command(async)]
pub fn grid_apply_edit(
    app: AppHandle,
    window: WebviewWindow,
    request: GridEditRequest,
) -> IpcResult<GridEditResponse, IpcError> {
    let command = command_names::GRID_APPLY_EDIT;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_apply_edit(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / 影響 = {} 行 / 違反 = {}",
            context.window.as_str(),
            data.outcome
                .as_ref()
                .map_or(0, |outcome| outcome.affected.len()),
            data.outcome
                .as_ref()
                .map_or(0, |outcome| outcome.violation_total),
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

/// 呼び出し元ウィンドウのグリッドの履歴を進める（要件 9.2、9.3、4.6）。
///
/// **進める履歴が無ければ成功腕で `outcome: None` を返す**（失敗ではない）。
#[tauri::command(async)]
pub fn grid_history(
    app: AppHandle,
    window: WebviewWindow,
    request: GridHistoryRequest,
) -> IpcResult<GridEditResponse, IpcError> {
    let command = command_names::GRID_HISTORY;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_history(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / 向き = {:?} / 変わった = {}",
            context.window.as_str(),
            request.direction,
            data.outcome.is_some(),
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

/// 呼び出し元ウィンドウのグリッドで、指定した位置から次の違反を探す（要件 4.2、4.4、4.5、4.6）。
///
/// **表示範囲の外にある違反にも到達する**（要件 4.4。索引が可視行の序数を鍵に持つ）。
/// 見つからなければ「これ以上無い」を正常な結果として返す。
#[tauri::command(async)]
pub fn grid_find_violation(
    app: AppHandle,
    window: WebviewWindow,
    request: GridViolationRequest,
) -> IpcResult<GridViolationResponse, IpcError> {
    let command = command_names::GRID_FIND_VIOLATION;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_find_violation(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / 起点 = {} / 向き = {:?} / 見つかった = {}",
            context.window.as_str(),
            request.from,
            request.direction,
            data.violation.is_some(),
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

// ---------------------------------------------------------------------------
// テスト（タスク 6.2）
//
// Tauri の実体（`WebviewWindow` / `AppHandle`）を要するのはコマンド関数の 5 つだけであり、
// 中身は**すべて本体の関数**へ切り出してある。したがってテストは本体を直接駆動する —
// GUI を起こさず、**本物の文書**（`document-format` は dev-dependency）と、破棄の購読の
// 二重だけで足りる。5 つのコマンド関数そのものの形は、関数の型を書いた 1 つのテストが
// コンパイル時に固定する。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use app_shell::ipc::{
        GridCellEdit, GridEditRequest, GridHistoryRequest, GridOpenRequest, GridSearchDirection,
        GridSortKey, GridViewRequest, GridViolationRequest,
    };
    use document_format::{CellValue, Document, DocumentFormat, DocumentFormatApi, SchemaPart};
    use document_session::{DocumentSessions, DocumentSessionsApi, SessionState};
    use schema_engine::{
        schema_to_text, ColumnDecl, Constraints, DeclaredKind, Schema, TypeDecl, TypeKind,
    };

    use super::*;
    use crate::session::watch::testing::AlwaysPresent;
    use crate::session::watch::DestroyHandler;

    /// 一時ディレクトリ（`session/commands.rs` のテストと同じ規律。プロセスごとに一意）。
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
                "jxcel-grid-commands-{tag}-{}-{unique}",
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

    /// 標本の宣言: 品番（`text`・一意・必須）と数量・単価（`int`・0〜100）。
    ///
    /// 数量と単価の範囲を狭くしてあるのは、境界の外の値（`999`）を編集で書くだけで違反を
    /// 列ごとに作れるようにするためである（要件 4.4 の探索と、**違反の総数を列に閉じた数から
    /// シート全体の数へ閉じる**ことを実物で駆動する）。
    fn declaration() -> SchemaPart {
        let schema = Schema {
            columns: vec![
                ColumnDecl {
                    name: "品番".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Text),
                        constraints: Constraints::default(),
                    },
                    required: true,
                    unique: true,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "数量".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Int),
                        constraints: Constraints {
                            min: Some(CellValue::Int(0)),
                            max: Some(CellValue::Int(100)),
                            ..Constraints::default()
                        },
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "単価".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Int),
                        constraints: Constraints {
                            min: Some(CellValue::Int(0)),
                            max: Some(CellValue::Int(100)),
                            ..Constraints::default()
                        },
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
            ],
        };
        let root = schema_to_text(&schema).expect("宣言は正準出力できる");
        SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#)).expect("宣言は解析できる")
    }

    /// 3 行（品番 = `A` / `B` / `C`、数量 = `1` / `2` / `3`、単価 = `10`〜`30`）の
    /// **本物の文書**を書く。
    fn write_document(path: &Path) {
        let mut document = Document::new();
        let sheet = document.add_sheet("台帳");
        document
            .set_sheet_columns(
                sheet,
                vec!["品番".to_owned(), "数量".to_owned(), "単価".to_owned()],
            )
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, declaration())
            .expect("標本のシートは実在する");
        for (index, label) in ["A", "B", "C"].into_iter().enumerate() {
            let row = document.add_row(sheet).expect("標本のシートは実在する");
            document
                .set_row_values(
                    sheet,
                    row,
                    vec![
                        CellValue::Text(label.to_owned()),
                        CellValue::Int(index as i64 + 1),
                        CellValue::Int(index as i64 * 10 + 10),
                    ],
                )
                .expect("標本の行は実在する");
        }
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
    }

    /// 保存された標本の行の識別子を、文書の順に返す（順序に依拠しない観測の材料）。
    fn stored_rows(path: &Path) -> Vec<String> {
        let opened = DocumentFormat::new().open(path).expect("標本を読める");
        opened
            .document
            .sheets()
            .first()
            .expect("標本にはシートが 1 つある")
            .rows()
            .iter()
            .map(|row| row.id().to_string())
            .collect()
    }

    /// グリッドの表（破棄の購読は常に成功し、どのラベルも引ける二重）。
    fn grids() -> GridSessions {
        GridSessions::new(Arc::new(AlwaysPresent))
    }

    /// 文書を 1 つ保持した表（文書の側）と、そのウィンドウのラベル。
    fn documents(path: &Path) -> (Arc<DocumentSessions>, WindowLabel) {
        let sessions = Arc::new(DocumentSessions::new());
        let label = WindowLabel::new("doc-1");
        sessions
            .resolve(&label, Some(path))
            .expect("標本を読み込める");
        (sessions, label)
    }

    /// 標本を開いた状態（表 + 文書 + ラベル）を作る。**開く経路そのものを通す。**
    fn opened(tag: &str) -> (Scratch, Arc<DocumentSessions>, GridSessions, WindowLabel) {
        let scratch = Scratch::new(tag);
        let path = scratch.file("台帳.jxcel");
        write_document(&path);
        let (sessions, label) = documents(&path);
        let grids = grids();
        let opened = answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet_id(&path),
            },
        );
        assert!(matches!(opened, IpcResult::Ok { .. }), "標本は開ける");
        (scratch, sessions, grids, label)
    }

    /// 標本のシートの識別子（文書の側の文字列表現。境界を通るのはこの文字列である）。
    fn sheet_id(path: &Path) -> String {
        let opened = DocumentFormat::new().open(path).expect("標本を読める");
        opened
            .document
            .sheets()
            .first()
            .expect("標本にはシートが 1 つある")
            .id()
            .to_string()
    }

    /// 成功の腕からデータを取り出す（期待が外れたときに何を返したかを示す）。
    fn data<T>(result: IpcResult<T, IpcError>) -> T {
        match result {
            IpcResult::Ok { data } => data,
            IpcResult::Err { error } => panic!("成功を期待したが失敗した: {error:?}"),
        }
    }

    /// 失敗の腕から原因を取り出す。
    fn error<T>(result: IpcResult<T, IpcError>) -> IpcError {
        match result {
            IpcResult::Ok { .. } => panic!("失敗を期待したが成功した"),
            IpcResult::Err { error } => error,
        }
    }

    /// 1 セルへ打たれた文字を書く命令を組み立てる。
    fn set_one(row: &str, column: u32, text: &str) -> GridEditCommand {
        GridEditCommand::SetCells {
            cells: vec![GridCellEdit {
                cell: GridCellAddress {
                    row: row.to_owned(),
                    column,
                },
                text: text.to_owned(),
            }],
        }
    }

    /// 空の表示の指定（絞り込み無し・並べ替え無し・展開無し）。
    fn empty_view() -> GridViewSpec {
        GridViewSpec::default()
    }

    // -----------------------------------------------------------------------
    // 5 つのコマンド関数の形（コンパイル時の表明）
    // -----------------------------------------------------------------------

    /// **5 つのコマンド関数は、基盤が注入する 2 引数と要求 1 つを取り、封筒を返す。**
    ///
    /// これは実行時テストではなく**コンパイル時の表明**である — 5 つを実体
    /// （`WebviewWindow` / `AppHandle`）つきで呼ぶには本物のウィンドウ基盤が要り、単体テスト
    /// では起こせない（`tauri` のモック基盤は `MockRuntime` のアプリしか作れず、コマンドの
    /// 引数は `Wry` に固定されている）。ここで関数の型を書くことで、**呼び出し元ウィンドウを
    /// 引数で受け取る形（要件 4.6）と封筒の型（要件 4.4）が変わればコンパイルが壊れる**。
    /// 中身の呼び出し可能性は下の各テストが本体を通して示す。
    #[test]
    fn the_command_wrappers_have_the_injected_window_shape() {
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridOpenRequest,
        ) -> IpcResult<GridOpenResponse, IpcError> = grid_open_sheet;
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridViewRequest,
        ) -> IpcResult<GridViewResponse, IpcError> = grid_set_view;
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridEditRequest,
        ) -> IpcResult<GridEditResponse, IpcError> = grid_apply_edit;
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridHistoryRequest,
        ) -> IpcResult<GridEditResponse, IpcError> = grid_history;
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridViolationRequest,
        ) -> IpcResult<GridViolationResponse, IpcError> = grid_find_violation;
    }

    // -----------------------------------------------------------------------
    // シートを開く（要件 1.1、1.5、1.6）
    // -----------------------------------------------------------------------

    /// **開くと、列の構成とシートの行数が返る**（要件 1.1、1.5）。
    #[test]
    fn opening_a_sheet_answers_the_columns_and_the_row_count() {
        let (_scratch, sessions, grids, label) = opened("open");
        let path = _scratch.file("台帳.jxcel");
        let opened = data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet_id(&path),
            },
        ));

        assert_eq!(label, opened.context.window, "呼び出し元の文脈が返る");
        let names: Vec<&str> = opened
            .sheet
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect();
        assert_eq!(vec!["品番", "数量", "単価"], names, "宣言の順に列が並ぶ");
        assert_eq!(3, opened.sheet.row_count, "シートの行数が返る");
        assert!(!opened.sheet.has_no_columns(), "列は宣言されている");
        assert!(
            !opened.sheet.has_columns_but_no_rows(),
            "行も 3 件ある（要件 1.5 の空の状態ではない）"
        );
        assert_eq!(
            Some(TypeKindTag::Text),
            opened.sheet.columns[0].kind,
            "葉の型の札が境界へ出る"
        );
    }

    /// **文書に無いシートと、ドキュメントを保持していないウィンドウは、経路の失敗になる。**
    ///
    /// どちらも封筒の失敗腕である（ドメインの判定結果ではない。`design.md`「Error Handling」）。
    #[test]
    fn an_unknown_sheet_and_a_window_without_a_document_fail_at_the_path() {
        let scratch = Scratch::new("open-failure");
        let path = scratch.file("台帳.jxcel");
        write_document(&path);
        let (sessions, label) = documents(&path);
        let grids = grids();

        let unknown = error(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: "存在しないシート".to_owned(),
            },
        ));
        match unknown {
            IpcError::Document { message } => {
                assert!(
                    message.contains("存在しないシート"),
                    "原因が名指しされる: {message}"
                );
            }
            other => panic!("経路の失敗を期待した: {other:?}"),
        }

        let absent = error(answer_open(
            &sessions,
            &grids,
            &WindowLabel::new("empty-1"),
            &GridOpenRequest {
                sheet: sheet_id(&path),
            },
        ));
        match absent {
            IpcError::Document { message } => {
                assert!(
                    message.contains("保持していない"),
                    "保持していないことが伝わる: {message}"
                );
            }
            other => panic!("経路の失敗を期待した: {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // 表示の指定を変える（要件 8.3、8.4、8.7）
    // -----------------------------------------------------------------------

    /// **絞り込みを適用すると、可視行数と隠された行数が返る**（要件 8.4、8.7）。
    #[test]
    fn setting_the_view_answers_the_visible_and_hidden_rows() {
        let (_scratch, sessions, grids, label) = opened("view");
        let request = GridViewRequest {
            view: GridViewSpec {
                sort: vec![GridSortKey {
                    column: 1,
                    descending: true,
                }],
                filters: vec![GridFilterSpec::Equals {
                    column: 0,
                    text: "B".to_owned(),
                }],
                expansion: Vec::new(),
            },
        };

        let view = data(answer_set_view(&sessions, &grids, &label, &request));
        assert_eq!(1, view.visible_rows, "品番 B の 1 行だけが可視である");
        assert_eq!(2, view.hidden_rows, "残る 2 行は隠れている");
        assert_eq!(0, view.violation_total, "違反は 1 件も無い");
        assert_eq!(label, view.context.window);

        // **要求は完全な記述である。** 空の指定へ戻すと、前の絞り込みは効かない。
        let cleared = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        assert_eq!(3, cleared.visible_rows, "絞り込みが外れる");
        assert_eq!(0, cleared.hidden_rows);
    }

    /// **要求に現れない展開は折りたたみへ戻る**（要求は完全な記述である。6.1 の規約）。
    #[test]
    fn a_view_request_replaces_the_expansion_states() {
        let (_scratch, sessions, grids, label) = opened("expansion");
        let expanded = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest {
                view: GridViewSpec {
                    expansion: vec![GridExpansionState {
                        column: 0,
                        expanded: true,
                        depth: 1,
                    }],
                    ..GridViewSpec::default()
                },
            },
        ));
        assert_eq!(3, expanded.visible_rows, "展開は行を隠さない");

        let entry = grids.entry(&label).expect("開いた保持がある");
        let states = lock(&entry).session.expansion().to_vec();
        assert!(
            states.iter().any(|state| state.expanded),
            "展開が適用されている（前提の表明）"
        );

        // 展開を 1 つも含まない指定へ戻す。
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        let entry = grids.entry(&label).expect("開いた保持がある");
        let states = lock(&entry).session.expansion().to_vec();
        assert!(
            states.iter().all(|state| !state.expanded),
            "要求に現れない展開は折りたたみへ戻る"
        );
    }

    /// **開いていないウィンドウへの操作は経路の失敗になる**（要求の順序の契約）。
    #[test]
    fn a_command_before_opening_fails_at_the_path() {
        let scratch = Scratch::new("not-open");
        let path = scratch.file("台帳.jxcel");
        write_document(&path);
        let (sessions, label) = documents(&path);
        let grids = grids();

        let failure = error(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        match failure {
            IpcError::Document { message } => {
                assert!(
                    message.contains("開かれていない"),
                    "原因が伝わる: {message}"
                );
            }
            other => panic!("経路の失敗を期待した: {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // 編集の適用（要件 3.3、3.4、3.5、4.3）
    // -----------------------------------------------------------------------

    /// **編集を適用すると、影響範囲と違反の要約が返る**（要件 3.3、3.4）。
    ///
    /// 違反の総数は**シート全体の数**である（要件 4.3。再検証した列に閉じた数ではない）—
    /// 境界へ閉じるのは本モジュールの仕事である。**2 つの列に違反を置いて区別する**:
    /// 2 度目の応答が「編集した列に閉じた数（1）」ではなく「シート全体の数（2）」を返すこと
    /// を確かめる（`EditOutcome::violation_total` をそのまま載せる実装はここで落ちる）。
    #[test]
    fn applying_an_edit_answers_the_verdict_and_the_sheet_wide_violation_total() {
        let (_scratch, sessions, grids, label) = opened("apply");
        let path = _scratch.file("台帳.jxcel");
        let rows = stored_rows(&path);
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));

        let applied = data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&rows[0], 1, "999"),
            },
        ));
        let outcome = applied.outcome.expect("適用は必ず要約を返す");
        assert_eq!(vec![rows[0].clone()], outcome.affected, "影響した行が返る");
        assert_eq!(3, outcome.row_count, "行数は変わらない");
        assert_eq!(1, outcome.violation_total, "範囲外の 1 件が総数になる");
        assert_eq!(1, outcome.violations.len(), "変わった違反が載る");
        assert_eq!(1, outcome.violations[0].column, "違反した列は数量である");
        assert_eq!(
            Some(rows[0].clone()),
            outcome.violations[0].row,
            "違反した行は編集した行である"
        );
        assert!(
            outcome.violations[0].path.is_empty(),
            "セル直下の違反である"
        );
        assert_eq!(vec![1], outcome.revalidated_columns, "再検証した列が載る");

        // 別の列にも違反を 1 件作る。応答の総数は**シート全体**（2 件）であり、
        // 編集した列に閉じた数（1 件）ではない（要件 4.3）。
        let second = data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&rows[1], 2, "999"),
            },
        ));
        let second = second.outcome.expect("適用は必ず要約を返す");
        assert_eq!(2, second.violation_total, "シート全体の違反の総数である");
        assert_eq!(
            1,
            second.violations.len(),
            "変わったのは編集した列の 1 件だけである"
        );
        assert_eq!(2, second.violations[0].column, "違反した列は単価である");
    }

    /// **解釈できない行の識別子は経路の失敗であり、文書へ触れない**（未保存の印も立てない）。
    #[test]
    fn a_malformed_row_identifier_does_not_touch_the_document() {
        let (_scratch, sessions, grids, label) = opened("malformed");
        let failure = error(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one("これは識別子ではない", 1, "1"),
            },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));
        assert!(
            matches!(
                sessions.state(&label),
                SessionState::Open { unsaved: false, .. }
            ),
            "変換できない要求で未保存の印を立てない"
        );
    }

    // -----------------------------------------------------------------------
    // 履歴（要件 9.2、9.3）
    // -----------------------------------------------------------------------

    /// **取り消しとやり直しが要約を返し、進める履歴が無いことは失敗ではない。**
    #[test]
    fn history_undoes_and_redoes_and_reports_an_empty_history_as_success() {
        let (_scratch, sessions, grids, label) = opened("history");
        let path = _scratch.file("台帳.jxcel");
        let rows = stored_rows(&path);
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));

        // 1. 何もしていないグリッドの取り消しは「進める履歴が無い」である（要件 9.2）。
        let empty = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ));
        assert_eq!(
            None, empty.outcome,
            "進める履歴が無いことは成功腕の `None` である"
        );

        // 2. 編集 → 取り消し（要件 9.2）。
        data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&rows[1], 0, "Z"),
            },
        ));
        let undone = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ));
        let undone = undone.outcome.expect("取り消しは要約を返す");
        assert_eq!(vec![rows[1].clone()], undone.affected, "取り消した行が返る");

        // 3. やり直し（要件 9.3）。
        let redone = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Redo,
            },
        ));
        let redone = redone.outcome.expect("やり直しは要約を返す");
        assert_eq!(vec![rows[1].clone()], redone.affected, "やり直した行が返る");
    }

    // -----------------------------------------------------------------------
    // 次の違反を探す（要件 4.2、4.4、4.5）
    // -----------------------------------------------------------------------

    /// **表示範囲の外にある違反にも到達し、位置と理由が返る**（要件 4.2、4.4、4.5）。
    #[test]
    fn finding_the_next_violation_answers_the_position_and_the_reason() {
        let (_scratch, sessions, grids, label) = opened("violation");
        let path = _scratch.file("台帳.jxcel");
        let rows = stored_rows(&path);
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));

        // 前提: 最初の探索は何も見つけない（違反を 1 件も作っていない）。
        let none = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 0,
                direction: GridSearchDirection::Forward,
            },
        ));
        assert!(none.violation.is_none(), "違反が無ければ見つからない");

        // 前提: 違反を持つ行を作る（範囲外の値）。
        data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&rows[2], 1, "-1"),
            },
        ));

        let found = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 0,
                direction: GridSearchDirection::Forward,
            },
        ));
        let violation = found.violation.expect("違反が見つかる");
        assert_eq!(
            Some(rows[2].clone()),
            violation.location.row,
            "違反した行の識別子が返る（文字列である）"
        );
        assert_eq!(1, violation.location.column, "違反した列は数量である");
        assert!(violation.location.path.is_empty(), "セル直下の違反である");
        assert!(
            violation.reason.contains("の外の値"),
            "範囲外であることが理由として伝わる: {}",
            violation.reason
        );

        // 通り過ぎた位置からは見つからない（索引が可視行の序数を鍵に持つため）。
        let past = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 3,
                direction: GridSearchDirection::Forward,
            },
        ));
        assert!(past.violation.is_none(), "末尾より後ろには違反が無い");

        // 逆向きにも到達する。
        let backward = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 2,
                direction: GridSearchDirection::Backward,
            },
        ));
        assert!(
            backward.violation.is_some(),
            "後ろ向きでも同じ違反へ到達する"
        );
    }

    // -----------------------------------------------------------------------
    // 破棄（design.md「GridCommands」: ウィンドウが閉じたら破棄する）
    // -----------------------------------------------------------------------

    /// テスト専用のウィンドウの側: **破棄の通知を外から起こせる**二重。
    ///
    /// `AlwaysPresent` は破棄を起こせない（購読を捨てないだけ）ため、破棄で保持が落ちる
    /// ことを観測するにはこちらを使う。
    #[derive(Clone, Default)]
    struct Destructible {
        handlers: Arc<Mutex<HashMap<String, DestroyHandler>>>,
        /// 購読の登録に成功するか（偽は「取得と登録の間に破棄された」を表す）。
        subscribes: bool,
    }

    impl Destructible {
        fn new() -> Self {
            Self {
                handlers: Arc::new(Mutex::new(HashMap::new())),
                subscribes: true,
            }
        }

        /// 名指ししたウィンドウの破棄を起こす。
        fn destroy(&self, label: &WindowLabel) {
            let handler = lock(&self.handlers).remove(label.as_str());
            if let Some(handler) = handler {
                handler(label);
            }
        }

        /// 登録された購読の数。
        fn subscriptions(&self) -> usize {
            lock(&self.handlers).len()
        }
    }

    impl WindowDestroyEvents for Destructible {
        fn subscribe_destroyed(&self, label: &WindowLabel, on_destroyed: DestroyHandler) -> bool {
            if !self.subscribes {
                return false;
            }
            lock(&self.handlers).insert(label.as_str().to_owned(), on_destroyed);
            true
        }

        fn has_window(&self, _label: &WindowLabel) -> bool {
            true
        }
    }

    /// **ウィンドウが閉じると、そのウィンドウの保持は落ちる**（design.md「GridCommands」）。
    ///
    /// 他のウィンドウの保持は変わらない（1 つのウィンドウの破棄が他へ及ばない）。
    #[test]
    fn a_destroyed_window_releases_only_its_own_session() {
        let scratch = Scratch::new("destroy");
        let path = scratch.file("台帳.jxcel");
        write_document(&path);
        let sheet = sheet_id(&path);
        // 2 枚のウィンドウが同じ文書を表示している（`doc-2` も同じ標本を読む）。
        let sessions = Arc::new(DocumentSessions::new());
        let label = WindowLabel::new("doc-1");
        let other = WindowLabel::new("doc-2");
        for target in [&label, &other] {
            sessions
                .resolve(target, Some(&path))
                .expect("標本を読み込める");
        }
        let events = Destructible::new();
        let grids = GridSessions::new(Arc::new(events.clone()));

        for target in [&label, &other] {
            assert!(
                matches!(
                    answer_open(
                        &sessions,
                        &grids,
                        target,
                        &GridOpenRequest {
                            sheet: sheet.clone()
                        }
                    ),
                    IpcResult::Ok { .. }
                ),
                "標本は開ける"
            );
        }
        assert_eq!(
            2,
            events.subscriptions(),
            "ウィンドウごとに 1 回だけ購読する"
        );

        events.destroy(&label);
        assert!(
            grids.entry(&label).is_none(),
            "破棄されたウィンドウの保持は落ちる"
        );
        assert!(
            grids.entry(&other).is_some(),
            "他のウィンドウの保持は変わらない"
        );
        assert_eq!(1, events.subscriptions(), "購読も取り除かれる");

        // 落ちたあとの操作は経路の失敗である（開き直しが要る）。
        let failure = error(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));
    }

    /// **購読を登録できないときは保持を置かない**（取得と登録の間に破棄された場合）。
    #[test]
    fn a_window_that_vanishes_before_subscribing_is_not_kept() {
        let scratch = Scratch::new("vanished");
        let path = scratch.file("台帳.jxcel");
        write_document(&path);
        let (sessions, label) = documents(&path);
        let grids = GridSessions::new(Arc::new(Destructible {
            subscribes: false,
            ..Destructible::new()
        }));

        let failure = error(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet_id(&path),
            },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));
        assert!(grids.entry(&label).is_none(), "購読の無い保持を残さない");
    }

    // -----------------------------------------------------------------------
    // 型の種別の札（tasks.md 6.1 の申し送り）
    // -----------------------------------------------------------------------

    /// **`TypeKind` の対応は総関数であり、その像は `TypeKindTag::ALL` と綴り・件数・並びが一致する。**
    ///
    /// この検査がここにしか置けない理由は 2 つある。`app-shell` は他のドメインクレートに
    /// 依存できないため `TypeKind` を見られず、`schema-engine` は境界の型
    /// （`ts-rs` の derive を持つ型）を持てない（置けるのは `crates/app-shell/src/ipc/` の
    /// 下だけである）。**両方を見られる唯一のクレートが `src-tauri` である。**
    ///
    /// # 何を捕まえるか
    ///
    /// - **対応は総関数である** — [`type_kind_tag`] はワイルドカードの無い `match` であり、
    ///   [`TypeKind`] に変種が増えれば**コンパイルが壊れる**（片側だけの追加をその場で止める）。
    ///   したがって「写像が種別を取りこぼす」ことは起こり得ない
    /// - **件数** — [`TypeKindTag::ALL`] に札が増えれば、写像の像（`TypeKind::ALL` の像）の
    ///   件数と食い違い、下の一致で落ちる（境界にだけ種別が増えた場合）
    /// - **綴り** — 変種の名前が食い違えば（改名・綴り間違い）落ちる
    /// - **並び** — 6.1 の doc が定める「並びは `TypeKind::ALL` と同じ」を固定する
    #[test]
    fn type_kind_tag_covers_every_type_kind() {
        assert_eq!(
            TypeKind::ALL.len(),
            TypeKindTag::ALL.len(),
            "種別の数が食い違っている（片方だけに種別が増えていないか）"
        );

        let image: Vec<TypeKindTag> = TypeKind::ALL.into_iter().map(type_kind_tag).collect();
        assert_eq!(
            TypeKindTag::ALL.to_vec(),
            image,
            "並びと綴りが一致しない（写像と境界の札を突き合わせること）"
        );

        // 綴りを明示的に突き合わせる（`Vec` の一致だけでは、両側が同時に同じ綴りへ
        // 間違えられた場合に気づけない）。14 種すべてを 1 つずつ名指しする。
        for (kind, tag) in TypeKind::ALL.into_iter().zip(TypeKindTag::ALL) {
            assert_eq!(format!("{kind:?}"), format!("{tag:?}"), "綴りが食い違う");
        }

        // 像の要素数（同じ札へ 2 つの種別が写っていれば、この数が減る）。
        let mut unique = image;
        unique.sort();
        unique.dedup();
        assert_eq!(
            TypeKindTag::ALL.len(),
            unique.len(),
            "2 つの種別が同じ札へ写っている"
        );
    }
}

//! 親ウィンドウを指定したネイティブファイル選択 — OS 標準のファイル選択手段を提示し、
//! 選ばれた位置をドキュメント所有者へ渡す。
//!
//! 所有: `DialogGate`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 2.4。
//!
//! 本モジュールが担うのは 3 つである。
//!
//! 1. **選択手段の提示**（[`pick_document_file`] と [`install`] の処理）。操作対象の
//!    ウィンドウを**必ず親として指定する**。
//! 2. **選ばれた位置の引き渡し**。位置は [`DocumentHostPort`] を**実行時に引いて**
//!    `DocumentHost::attach` へ渡す（tasks.md 6.2 の申し送りをここで履行する）。
//! 3. **メニューからの引き金**。7.4 の登録口（[`MenuRegistry::register`]）を通して
//!    「開く」項目を登録し、活性化の対象ウィンドウ（7.5 の振り向け）から提示する。
//!
//! # 本機能はパスを読まない
//!
//! **これは要件 2.4 と design.md「DialogGate」の明文の制約である。** したがって本モジュールは
//! 選ばれた位置に対して次のいずれもしない — 存在確認（`exists`）、種別や大きさの取得
//! （`metadata`）、内容の読み取り（`fs::read`）、正規化（`canonicalize`）、拡張子や形式の検査、
//! 複製、そして**本番の記録への出力**。位置は `PathBuf` のまま委譲先へ渡すだけで、解釈は所有者
//! （下流スペック）の責務である。既定の委譲先（[`crate::ports::DefaultDocumentHost`]）も
//! パスに触れずに成功を返す。
//!
//! **検証ビルドだけは例外である。** `verification-triggers` feature の下でのみ入る検証用の
//! 委譲先（[`crate::ports::VerificationDocumentHost`]）が、引き渡された `(ウィンドウ, 位置)` を
//! 記録に残す。これは「位置がどのウィンドウの委譲先へ届いたか」を実測するための唯一の手段で
//! あり、配布物（既定ビルド）にはその実装自体が存在しない。
//!
//! # 親ウィンドウは必ず指定する
//!
//! **親を指定しないダイアログは、複数ウィンドウのアプリで誤ったウィンドウに乗る。** 親は
//! どちらの経路でも**操作対象のウィンドウそのもの**から取り、決してフロントエンドの payload
//! からは取らない（偽装できない。tasks.md 7.1）。
//!
//! - **コマンド経路**（[`pick_document_file`]）: Tauri が注入する `WebviewWindow` をそのまま
//!   親にする。
//! - **メニュー経路**（[`install`]）: 7.5 が解決した**活性化の対象ウィンドウ**のラベルから
//!   `AppHandle::get_webview_window` でウィンドウを引く。対象は「そのメニューを所有する
//!   ウィンドウ」（Windows / Linux）または「活性化の時点でフォーカスされているウィンドウ」
//!   （macOS）であり、利用者が指定する値ではない。
//!
//! **親が既に失われているときは、親無しで提示しない。** 親を指定できないなら要件 2.4 の
//! 前提が満たせないので、提示せずに失敗（境界では [`IpcError::Window`]）として報告する。
//! ウィンドウのラベルが見つからない場合（メニュー経路）と、GTK のウィンドウが既に破棄されて
//! いる場合（Linux）の 2 か所で判定する。
//!
//! # 実行モデル（ブロックする選択をイベントループのスレッドで走らせない）
//!
//! **コマンドは `async fn` として宣言する。** Tauri の非同期コマンドは非同期ランタイムの
//! タスクとして走り（`tauri-macros` の `ExecutionContext::Async`。`#[tauri::command]` の既定）、
//! **イベントループのスレッドでは実行されない**。そのうえで選択手段の待ち合わせは
//! `tauri::async_runtime::spawn_blocking` に載せるので、ランタイムのワーカーも塞がない。
//!
//! **メニューの処理はイベントループのスレッドで走る。** したがってそこで選択を提示してはならず、
//! 同じ `spawn_blocking` へ渡す（[`install`]）。
//!
//! **Linux では GTK をメインスレッドでしか触れない。** `WebviewWindow::gtk_window()` も
//! メインスレッド限定である。したがって提示そのものは `AppHandle::run_on_main_thread` で
//! メインスレッドへ依頼し、**その完了は依頼した側のスレッドで待つ**（メインスレッドは
//! 入れ子のループに入らず、`show()` して即座に戻る）。つまり「ブロックするのは
//! `spawn_blocking` のスレッドだけ」であり、イベントループは一度も止まらない。
//!
//! # 依存の選択（タスク 1.3 から送られた判断）
//!
//! 1.3 は `tauri-plugin-dialog` を宣言せず、この判断を 7.7 へ送った。固定した版で両案を実測した
//! 結果は次のとおりである。
//!
//! - **(a) `tauri-plugin-dialog` 2.7.3**: `tauri-plugin-fs` 2.5.2 を**非 optional の通常依存**に
//!   持つ（プラグインの `Cargo.toml` で確認）。宣言した時点で `cargo tree -p jxcel` に
//!   `tauri-plugin-fs` が現れ、要件 4.7 の**第一の制御**（fs プラグインを依存に入れない）が
//!   破れる。加えて、このプラグインの Linux 実装は `rfd` 0.16 の GTK3 経路に委譲し、その
//!   経路は親ウィンドウを無視する（後述）。**両方の理由で採らない。**
//! - **(b) `rfd` 直接（GTK3 経路）**: `rfd` は `FileDialog::set_parent` を受け取るが、Linux の
//!   `gtk3` 実装は内部で `gtk_file_chooser_native_new(…, ptr::null_mut(), …)` を呼び、**親を
//!   渡さない**（0.16.0 と 0.17.2 の両方で確認）。つまり API は親を受け取っても Linux では
//!   捨てられる。**要件 2.4 の「親ウィンドウを必ず指定する」を満たせない。**
//! - **(b') `rfd` 直接（XDG ポータル経路）**: この経路は `parent_window` をポータルへ渡すので
//!   親は効くが、ダイアログを描くのは**ポータルの別プロセス**であり、この環境では Wayland の
//!   サーフェスになる。プロセスの外に出るため提示先の帰属を外から観測できず、動作も
//!   デスクトップポータルの有無に依存する。**採らない。**
//! - **採用**: Linux は **`gtk` 0.18（Tauri が既に解決している版と同じ。`Cargo.lock` に
//!   新しいパッケージは増えない）** の `FileChooserNative` を自分で提示し、**親を明示的に
//!   指定する**。GTK の選択器はこの環境の OS 標準の選択器そのものである。Windows と macOS は
//!   `rfd` 0.17 を使う — これらのバックエンド（`IFileDialog::SetParent` と `NSOpenPanel` の
//!   シート）は `set_parent` を実際に使うため、親の指定が効く。
//!
//! この選択で `tauri-plugin-fs` は依存ツリーに入らず、`tauri-plugin-dialog` も入らない。
//! `Cargo.lock` に増える新しいパッケージは `rfd`（macOS のみ・Windows は 0.61 系が既に解決
//! 済み）だけであり、Linux の依存には `gtk` を直接参照する以外の追加が無い。

use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::sync::mpsc::Sender;

use app_shell::ipc::command_names;
use app_shell::ipc::{
    DocumentPickOutcome, IpcError, IpcResult, PickDocumentFileResponse, WindowContext, WindowLabel,
};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_log::log;

use crate::menu::{MenuItemSpec, MenuPath, MenuRegistry, MenuSelection};
use crate::ports::DocumentHostPort;

/// 選択手段の題名。
const TITLE: &str = "ドキュメントを開く";

/// 承認の操作の表示名（GTK の選択器でのみ使う）。
#[cfg(target_os = "linux")]
const ACCEPT_LABEL: &str = "開く";

/// 取り消しの操作の表示名（GTK の選択器でのみ使う）。
#[cfg(target_os = "linux")]
const CANCEL_LABEL: &str = "キャンセル";

/// 登録元の識別子（7.4 の組み込み項目と同じ名前空間を使う）。
const OWNER: &str = "app-shell";

/// メニュー項目の識別子。**アプリ全体で一意でなければならない**（7.4 の
/// `MenuRegistrationError::ItemIdConflict` が検査する）。
const OPEN_ITEM_ID: &str = "app-shell.open-document";

/// メニュー項目の表示名。
const OPEN_LABEL: &str = "開く…";

/// 「開く」のショートカット（非 macOS。プラットフォーム解決済みの綴り。4.6 の契約）。
#[cfg(not(target_os = "macos"))]
const OPEN_ACCELERATOR_SPELLING: &str = "Ctrl+O";

/// 「開く」のショートカット（macOS。メニュー上は `⌘O` と描かれる）。
#[cfg(target_os = "macos")]
const OPEN_ACCELERATOR_SPELLING: &str = "Cmd+O";

// ---------------------------------------------------------------------------
// 選択と引き渡しの結果（境界へ写す前の形）
// ---------------------------------------------------------------------------

/// 選択手段そのものの結果。
///
/// **取り消しと「提示できなかった」を区別する。** 取り消しは正常な結果であり、提示できなかった
/// ことは失敗である（親が失われた等）。混ぜると、利用者が取り消しただけのときに誤りを報告する
/// ことになる。
#[derive(Debug, Clone, PartialEq, Eq)]
enum PickResult {
    /// 利用者が取り消した。
    Cancelled,
    /// 位置が選ばれた。
    Picked(PathBuf),
    /// 選択手段を提示できなかった（理由を運ぶ）。
    ///
    /// **Linux でのみ生じる。** GTK のウィンドウはメインスレッドでしか触れないため、提示は
    /// メインスレッドへ依頼する。その依頼に失敗した場合と、親ウィンドウが既に失われている場合が
    /// ここに来る。**Windows / macOS の `rfd` は「提示できなかった」と「取り消し」を区別して
    /// 返さない**（どちらも「選択なし」である）ため、この腕は既定のビルドのこの 2 つでは
    /// 構築されない。
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Unavailable(String),
}

/// 選択と引き渡しの最終結果。
#[derive(Debug, Clone, PartialEq, Eq)]
enum HandOff {
    /// 利用者が取り消した。**引き渡しは起きていない。**
    Cancelled,
    /// 委譲先が受け入れた。
    Attached,
    /// 委譲先が受け取らなかった（理由を運ぶ）。
    Rejected { reason: String },
    /// 選択手段を提示できなかった（理由を運ぶ）。
    Unavailable { message: String },
}

/// 最終結果を境界の形へ写す。**純粋関数。**
///
/// **`Cancelled` と `Rejected` は封筒の成功側に載せる。** どちらもコマンドが正常に答えた結果で
/// あり、失敗側（[`IpcError`]）に載せると「通信が失敗した」ことと区別できなくなる
/// （7.6 の終了拒否が `Deny` を成功側に置いたのと同じ判断）。**提示できなかったことだけが
/// 失敗である**（[`IpcError::Window`]）。
fn to_boundary(hand_off: HandOff) -> Result<DocumentPickOutcome, IpcError> {
    match hand_off {
        HandOff::Cancelled => Ok(DocumentPickOutcome::Cancelled),
        HandOff::Attached => Ok(DocumentPickOutcome::Attached),
        HandOff::Rejected { reason } => Ok(DocumentPickOutcome::Rejected { reason }),
        HandOff::Unavailable { message } => Err(IpcError::Window { message }),
    }
}

/// 記録に出す 1 行。**位置そのものは書かない**（本番の記録にパスを残さない。module doc
/// 「本機能はパスを読まない」）。
fn describe(hand_off: &HandOff) -> String {
    match hand_off {
        HandOff::Cancelled => "利用者が選択を取り消した".to_owned(),
        HandOff::Attached => "選ばれた位置を委譲先へ引き渡し、受け入れられた".to_owned(),
        HandOff::Rejected { reason } => format!("委譲先が引き渡しを受け取らなかった（{reason}）"),
        HandOff::Unavailable { message } => format!("ファイル選択を提示できなかった（{message}）"),
    }
}

// ---------------------------------------------------------------------------
// 提示と引き渡し（唯一の実装）
// ---------------------------------------------------------------------------

/// ファイル選択を提示し、選ばれた位置を委譲先へ引き渡す。**ブロックする。**
///
/// **イベントループのスレッドから呼んではならない。** 呼び出し元は 2 つあり、どちらも
/// `spawn_blocking` のスレッドで呼ぶ（[`pick_document_file`] と [`install`]）。
///
/// 提示そのものは [`pick`] が行う（プラットフォーム差はそこだけにある）。
///
/// **結果はここで 1 回だけ記録する。** メニュー経路は戻り値を捨てる（利用者へ返す先が無い）ので、
/// 記録が無いと「提示できなかった」ことがどこにも現れない。
fn pick_and_hand_off(app: &AppHandle, window: &WebviewWindow) -> HandOff {
    let command = command_names::PICK_DOCUMENT_FILE;
    let target = window.label();
    log::info!("{command}: 対象ウィンドウ = {target} にファイル選択を提示する");
    let hand_off = match pick(window) {
        PickResult::Cancelled => HandOff::Cancelled,
        PickResult::Unavailable(message) => HandOff::Unavailable { message },
        PickResult::Picked(path) => hand_off(app, window, &path),
    };
    log::info!(
        "{command}: 対象ウィンドウ = {target} / 結果 = {}",
        describe(&hand_off)
    );
    hand_off
}

/// 選ばれた位置を委譲先へ引き渡す。**本機能はパスの中身に触れない。**
///
/// 委譲点は**実行時に**管理状態から引く（tasks.md 6.2 の申し送り。7.6 と同じ経路）。呼び出し元
/// ウィンドウのラベルは、注入された `WebviewWindow`（または 7.5 が解決した対象）から取る。
///
/// **拒否（[`crate::ports::AttachError`]）はここで握りつぶさない。** 呼び出し元が結果を
/// 知る必要がある（利用者へ伝えるかどうかを決めるのは呼び出し元である）ので、
/// [`HandOff::Rejected`] として理由ごと運ぶ。
fn hand_off(app: &AppHandle, window: &WebviewWindow, path: &Path) -> HandOff {
    let command = command_names::PICK_DOCUMENT_FILE;
    let label = WindowLabel::new(window.label());
    let port = app.state::<DocumentHostPort>();
    match port.attach(&label, path) {
        Ok(()) => {
            log::info!(
                "{command}: 委譲先が引き渡しを受け入れた（ウィンドウ = {}）",
                label.as_str()
            );
            HandOff::Attached
        }
        Err(error) => {
            log::warn!(
                "{command}: 委譲先が引き渡しを受け取らなかった（ウィンドウ = {}）: {error}",
                label.as_str()
            );
            HandOff::Rejected {
                reason: error.to_string(),
            }
        }
    }
}

/// 選択手段を提示し、その完了を待つ。**ブロックする。**
///
/// 引数の `window` が**親**である（module doc「親ウィンドウは必ず指定する」）。
#[cfg(target_os = "linux")]
fn pick(window: &WebviewWindow) -> PickResult {
    use std::sync::mpsc;

    // GTK はメインスレッドでしか触れない（`WebviewWindow::gtk_window` も同じ）。したがって
    // **提示はメインスレッドへ依頼し、完了はこのスレッドで待つ** — メインスレッドは入れ子の
    // ループに入らない。
    let (sender, receiver) = mpsc::channel();
    let target = window.clone();
    if let Err(error) = window.app_handle().run_on_main_thread(move || {
        show_chooser(&target, sender);
    }) {
        return PickResult::Unavailable(format!(
            "メインスレッドへファイル選択の提示を依頼できなかった: {error}"
        ));
    }
    // 送信側が先に消えた場合（メインスレッドが終了した等）は失敗する。**無限に待たない。**
    receiver.recv().unwrap_or_else(|_| {
        PickResult::Unavailable("ファイル選択の応答が得られなかった".to_owned())
    })
}

/// GTK のネイティブ選択器を**メインスレッドで**提示する（応答は待たずに戻る）。
///
/// **親を必ず指定する。** 親は Tauri がこのウィンドウのために保持している `GtkWindow` であり、
/// フロントエンドの payload からは取れない。親が既に失われている場合（GTK のウィンドウを
/// 取得できない、または既に破棄されて実体を失っている）は**親無しで提示せず**、失敗として
/// 報告する — 親無しのダイアログは誤ったウィンドウに乗りうるので、提示しない方が正しい。
#[cfg(target_os = "linux")]
fn show_chooser(window: &WebviewWindow, sender: Sender<PickResult>) {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk::prelude::*;

    let parent = match window.gtk_window() {
        // `is_realized` は「ネイティブのウィンドウがまだ存在する」ことを表す（破棄で失われる）。
        Ok(parent) if parent.is_realized() => parent,
        Ok(_) => {
            let _ = sender.send(PickResult::Unavailable(
                "親ウィンドウは既に破棄されている（親無しでは提示しない）".to_owned(),
            ));
            return;
        }
        Err(error) => {
            let _ = sender.send(PickResult::Unavailable(format!(
                "親ウィンドウを取得できなかった: {error}"
            )));
            return;
        }
    };

    // `FileChooserNative` は GTK の標準の選択器である（ポータルが使える環境では GTK 自身が
    // ポータルへ委譲し、そのときも親の指定が引き継がれる）。第 2 引数が親である。
    let chooser = gtk::FileChooserNative::new(
        Some(TITLE),
        Some(&parent),
        gtk::FileChooserAction::Open,
        Some(ACCEPT_LABEL),
        Some(CANCEL_LABEL),
    );
    chooser.set_modal(true);

    // **選択器への強い参照を、応答が来るまで保持する。** `g_object` の参照をここで落とすと
    // 表示中のネイティブ選択器が破棄され、GTK の内部状態を壊す（実測: 表示直後にプロセスが
    // 落ちた）。スロットは応答を受け取った時点で空にし、選択器は応答処理の中で破棄する
    // （表示の失敗で応答が来ない場合も、親の破棄が応答として届く）。
    let slot: Rc<RefCell<Option<gtk::FileChooserNative>>> =
        Rc::new(RefCell::new(Some(chooser.clone())));

    // 親が破棄されるとこの選択器も破棄され、応答が返らないことがある。**待ち続けるスレッドを
    // 残さない**よう、取り消しとして完了させる（2 度目以降の送信は無視される）。
    {
        let sender = sender.clone();
        parent.connect_destroy(move |_| {
            let _ = sender.send(PickResult::Cancelled);
        });
    }

    {
        let slot = Rc::clone(&slot);
        chooser.connect_response(move |_chooser, response| {
            let Some(chooser) = slot.borrow_mut().take() else {
                return;
            };
            let result = if response == gtk::ResponseType::Accept {
                chooser
                    .filename()
                    .map_or(PickResult::Cancelled, PickResult::Picked)
            } else {
                PickResult::Cancelled
            };
            let _ = sender.send(result);
            chooser.destroy();
            // ここで選択器の最後の参照が落ち、同じ（メイン）スレッドで破棄が完了する。
        });
    }
    chooser.show();
}

/// 選択手段を提示し、その完了を待つ（Windows / macOS）。**ブロックする。**
///
/// 引数の `window` が**親**である。`rfd` はこの 2 つのプラットフォームで `set_parent` を実際に
/// 使う（Windows は `IFileDialog::SetParent`、macOS は親ウィンドウへ載るシート）。
#[cfg(not(target_os = "linux"))]
fn pick(window: &WebviewWindow) -> PickResult {
    let chosen = rfd::FileDialog::new()
        .set_title(TITLE)
        .set_parent(window)
        .pick_file();
    match chosen {
        Some(path) => PickResult::Picked(path),
        None => PickResult::Cancelled,
    }
}

// ---------------------------------------------------------------------------
// コマンド面（9.6 の画面からの経路。メニューと同じ 1 本の実装を通る）
// ---------------------------------------------------------------------------

/// 親ウィンドウを指定したファイル選択を提示し、選ばれた位置を委譲先へ引き渡す（要件 2.4）。
///
/// **呼び出し元ウィンドウは Tauri が注入する `WebviewWindow` から取る**（フロントエンドの
/// payload からではない = 偽装できない。要件 4.6、tasks.md 7.1）。それがそのまま**親**になる。
///
/// 待ち合わせは `spawn_blocking` に載せる（module doc「実行モデル」）。応答は封筒であり、
/// **取り消しと委譲先の拒否は成功側**、提示できなかったことだけが失敗側である（[`to_boundary`]）。
///
/// **このコマンドはメニュー経路と同じ [`pick_and_hand_off`] を通る。** 選択手段の実装を
/// 2 つ持たない（9.6 の画面もこの経路を使う）。
#[tauri::command]
pub async fn pick_document_file(
    window: WebviewWindow,
) -> IpcResult<PickDocumentFileResponse, IpcError> {
    let context = WindowContext {
        window: WindowLabel::new(window.label()),
    };
    let app = window.app_handle().clone();
    let target = window.clone();
    let joined =
        tauri::async_runtime::spawn_blocking(move || pick_and_hand_off(&app, &target)).await;
    let hand_off = match joined {
        Ok(hand_off) => hand_off,
        Err(error) => {
            return IpcResult::Err {
                error: IpcError::Window {
                    message: format!("ファイル選択の処理が異常終了した: {error}"),
                },
            };
        }
    };
    match to_boundary(hand_off) {
        Ok(outcome) => IpcResult::Ok {
            data: PickDocumentFileResponse { context, outcome },
        },
        Err(error) => IpcResult::Err { error },
    }
}

// ---------------------------------------------------------------------------
// メニューからの引き金（7.4 の登録口を通す）
// ---------------------------------------------------------------------------

/// 「開く」項目を置く部分メニュー。**7.4 が決めたトップレベルの並び**（`ファイル`）に従う。
///
/// 位置の名前をここで書き写さず、`menu` モジュールの定数を参照する（並びと名前の食い違いを
/// 作らない）。
fn document_menu_path() -> MenuPath {
    MenuPath::new([crate::menu::FILE_MENU_LABEL]).expect("位置は空でない")
}

/// 「開く」項目の登録内容を組み立てる（登録口へ渡す値の組み立てだけを切り出す）。
///
/// `handler` を差し替えられる形にしてあるのは、**GUI 無しで登録の受理と内容を検査できる**
/// ようにするためである（[`MenuRegistry::enroll`] は画面を要しない）。
fn open_document_spec(handler: impl Fn(&MenuSelection) + Send + Sync + 'static) -> MenuItemSpec {
    MenuItemSpec::new(
        OWNER,
        OPEN_ITEM_ID,
        document_menu_path(),
        OPEN_LABEL,
        handler,
    )
    .with_accelerator(OPEN_ACCELERATOR_SPELLING)
}

/// 起動時に 1 回だけ「開く」項目を登録する。`lifecycle::run` がメニューの構築
/// （`menu::install`）の後に呼ぶ。
///
/// 登録は 7.4 の登録口（[`MenuRegistry::register`]）を通す。ショートカットの綴りは
/// **プラットフォーム解決済み**（非 macOS `Ctrl+O` / macOS `Cmd+O`）で、競合は登録時に
/// 4.6 の検査へかかり、登録元（このモジュール）へ報告される。
///
/// 選択されたときの処理は 2 段である:
///
/// 1. **活性化の対象ウィンドウ**（7.5 の振り向け）をラベルから解決する。対象が無いときは
///    **親を指定できないので提示しない**（誤ったウィンドウに乗せるより、何もしない方が正しい）。
/// 2. 選択を [`spawn_blocking`](tauri::async_runtime::spawn_blocking) へ渡す。**メニューの
///    処理はイベントループのスレッドなので、ここでブロックしてはならない。**
pub fn install(app: &AppHandle) {
    let registry = app.state::<MenuRegistry>();
    let handling_app = app.clone();
    let spec = open_document_spec(move |selection: &MenuSelection| {
        let command = command_names::PICK_DOCUMENT_FILE;
        let Some(label) = selection.window().cloned() else {
            log::warn!("{command}: 対象ウィンドウが無いためファイル選択を提示しない");
            return;
        };
        let Some(window) = handling_app.get_webview_window(label.as_str()) else {
            log::warn!(
                "{command}: 対象ウィンドウ {} は既に無いためファイル選択を提示しない",
                label.as_str()
            );
            return;
        };
        let app = handling_app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _ = pick_and_hand_off(&app, &window);
        });
    });
    if let Err(error) = registry.register(app, spec) {
        log::error!("「開く」メニュー項目を登録できなかった: {error}");
    }
}

#[cfg(test)]
mod tests {
    use app_shell::accelerator::{Accelerator, MenuItemId};
    use app_shell::ipc::{DocumentPickOutcome, IpcError};

    use super::{
        document_menu_path, open_document_spec, to_boundary, HandOff, OPEN_ACCELERATOR_SPELLING,
        OPEN_ITEM_ID, OPEN_LABEL, OWNER,
    };
    use crate::menu::{MenuNode, MenuPath, MenuRegistry};

    /// 取り消しと委譲先の拒否は**封筒の成功側**に載る（通信の失敗と区別できる）。
    #[test]
    fn cancellation_and_rejection_are_successful_answers() {
        assert_eq!(
            to_boundary(HandOff::Cancelled),
            Ok(DocumentPickOutcome::Cancelled)
        );
        assert_eq!(
            to_boundary(HandOff::Attached),
            Ok(DocumentPickOutcome::Attached)
        );
        assert_eq!(
            to_boundary(HandOff::Rejected {
                reason: "受け取れない".to_owned(),
            }),
            Ok(DocumentPickOutcome::Rejected {
                reason: "受け取れない".to_owned(),
            })
        );
    }

    /// 提示できなかったことだけが失敗である（理由を保ったまま境界へ渡る）。
    #[test]
    fn an_unavailable_picker_is_a_window_failure() {
        assert_eq!(
            to_boundary(HandOff::Unavailable {
                message: "親ウィンドウを取得できなかった".to_owned(),
            }),
            Err(IpcError::Window {
                message: "親ウィンドウを取得できなかった".to_owned(),
            })
        );
    }

    /// 記録の 1 行が結果ごとに区別できる（GUI 実行での観測はこの行で行う）。
    #[test]
    fn the_log_line_distinguishes_every_outcome() {
        let lines = [
            HandOff::Cancelled,
            HandOff::Attached,
            HandOff::Rejected {
                reason: "理由".to_owned(),
            },
            HandOff::Unavailable {
                message: "理由".to_owned(),
            },
        ];
        let mut seen = std::collections::BTreeSet::new();
        for hand_off in &lines {
            let line = super::describe(hand_off);
            assert!(!line.is_empty());
            assert!(seen.insert(line.clone()), "記録の行が重複している: {line}");
        }
    }

    /// 「開く」項目は登録口を通り、**7.4 の組み込みの「終了」と同じ部分メニュー**に、
    /// 競合しないショートカット付きで並ぶ。
    ///
    /// 起動時に実際に登録される 2 件（終了と開く）を同じ登録簿へ入れて検査するので、
    /// **ショートカットの競合が無いこともここで固定される**（4.6 / 要件 3.4）。
    #[test]
    fn the_open_item_is_registered_beside_quit_with_a_conflict_free_shortcut() {
        let registry = MenuRegistry::new();
        let noop = |_: &super::MenuSelection| {};
        registry
            .enroll(open_document_spec(noop))
            .expect("「開く」は未登録の組み合わせを使う");
        // 組み込みの「終了」（7.4 / 7.5）と同じ登録元・同じ部分メニューへ入れて競合を見る。
        let quit = crate::menu::MenuItemSpec::new(
            OWNER,
            "app-shell.quit",
            MenuPath::new(["ファイル"]).expect("空でない"),
            "終了",
            noop,
        )
        .with_accelerator(if cfg!(target_os = "macos") {
            "Cmd+Q"
        } else {
            "Ctrl+Q"
        });
        registry.enroll(quit).expect("終了との競合が無い");

        let path = document_menu_path();
        assert_eq!(
            path.segments(),
            &["ファイル".to_owned()],
            "「開く」は 7.4 のファイルメニューに置く"
        );

        let model = registry.model();
        let file_submenu = model
            .top()
            .iter()
            .find(|submenu| submenu.label() == "ファイル")
            .expect("ファイルの部分メニューがある");
        let items: Vec<_> = file_submenu
            .children()
            .iter()
            .filter_map(|child| match child {
                MenuNode::Item(item) => Some(item),
                MenuNode::Submenu(_) => None,
            })
            .collect();
        assert_eq!(items.len(), 2, "終了と開くの 2 件である");
        let open = items
            .iter()
            .find(|item| item.item() == &MenuItemId::new(OPEN_ITEM_ID))
            .expect("「開く」がファイルメニューにある");
        assert_eq!(open.label(), OPEN_LABEL);
        assert_eq!(
            open.accelerator(),
            Some(&Accelerator::parse(OPEN_ACCELERATOR_SPELLING).expect("解決済みの綴り"))
        );
        // 登録元は組み込みの「終了」と同じ名前空間である（項目の識別子はアプリ全体で一意）。
        assert_eq!(open.owner().as_str(), OWNER);
    }

    /// 存在しないパスを渡しても**本機能は何も読まない**（既定の委譲点の契約をここからも固定する）。
    ///
    /// 引き渡しは [`crate::ports::DocumentHostPort`] の既定実装（常に成功し、パスに触れない）へ
    /// 届くので、存在しないパスでも成功し、そのあとパスは作られていない。**本機能が
    /// `exists` や `metadata` を呼べば、この経路で失敗か副作用が出る。**
    #[test]
    fn the_hand_off_does_not_touch_the_path() {
        use crate::ports::DocumentHostPort;
        use app_shell::ipc::WindowLabel;

        let port = DocumentHostPort::default();
        let missing = std::env::temp_dir().join(format!(
            "jxcel-dialog-test-missing-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        assert!(!missing.exists(), "前提: 存在しないパス");
        assert_eq!(port.attach(&WindowLabel::new("empty-1"), &missing), Ok(()));
        assert!(
            !missing.exists(),
            "本機能がパスを作った: {}",
            missing.display()
        );
    }
}

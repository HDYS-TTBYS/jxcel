//! 親ウィンドウを指定したネイティブファイル選択 — OS 標準のファイル選択手段を提示し、
//! 選ばれた位置をドキュメント所有者へ渡す。
//!
//! 所有: `DialogGate`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 2.4。
//!
//! 本モジュールが担うのは 4 つである。
//!
//! 1. **選択手段の提示**（[`pick_document_file`] と [`install`] の処理）。操作対象の
//!    ウィンドウを**必ず親として指定する**。
//! 2. **選ばれた位置の引き渡し**。位置は [`DocumentHostPort`] を**実行時に引いて**
//!    `DocumentHost::attach` へ渡す（tasks.md 6.2 の申し送りをここで履行する）。
//! 3. **メニューからの引き金**。7.4 の登録口（[`MenuRegistry::register`]）を通して
//!    「開く」項目を登録し、活性化の対象ウィンドウ（7.5 の振り向け）から提示する。
//! 4. **引き渡しの成立の通知**（[`hand_off`]）。引き渡しはセッションの状態を変えるので、
//!    成立したときに対象ウィンドウへ `document_session_changed` を 1 回送る。**2 つの経路
//!    （メニューとコマンド）が同じ [`hand_off`] を通るため、送る場所は 1 つで足りる。**
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
//! # 保存先の選択（要件 5.2、5.3）
//!
//! [`pick_save_location`] は「保存」の判断のうち**利用者に保存先を尋ねる**部分だけを持つ。何を
//! いつ書き出すかを決めるのはコア（`document-session`）と 3.4 の `document_save` であり、
//! 本関数は尋ねて位置を返すだけである。
//!
//! **3 つの答えを区別する。** `Chosen`（選ばれた）/ `Cancelled`（取り消した）/
//! `Unavailable`（提示できなかった）。**取り消しは正常な結果であり、失敗ではない** — 保存の
//! 指示は「何も書き出さず、未保存のまま保つ」という正しい答えへ進む（要件 5.3）。提示できなかった
//! ことだけが失敗である（3.4 が利用者へ理由を伝えるかどうかを決める）。
//!
//! **位置はコマンドの応答に含めない。** 本関数は選ばれた位置を Rust の呼び出し元へ返すだけであり、
//! 応答の型（境界の `DocumentSaveResponse`）は `status` と `outcome` だけを運ぶ。3.4 の
//! `document_save` は**位置を応答へ写してはならない** — 境界の約束（design.md
//! 「Boundary Commitments」の「位置を境界へ出さない」）を破る唯一の経路になる。
//!
//! **`tauri-plugin-dialog` はここでも使わない。** 依存の判断は module doc「依存の選択」
//! のとおりであり、保存の動作でも同じ 3 つの理由（fs プラグインの非 optional 依存・Linux で
//! 親が無視されること・ポータルの別プロセス）がそのまま効く。
//!
//! # 保存先の選択の呼び出し元は `document_save` である
//!
//! 保存先の提示の生産側の呼び出し元は [`document_save`](crate::session::commands::document_save)
//! であり、出所を持たない文書の保存で [`pick_save_location`] を呼び、`Chosen` ならコアの
//! `save_to` へ渡す。**呼び出しは `spawn_blocking` のスレッドの内側で完結し、選ばれた位置が
//! 応答へ写ることはない**（位置は境界を越えない）。3.5 がこの節へ付けていた
//! `#[allow(dead_code)]` の seam は 3.4 の配線で解消した（`ports.rs` / `session/watch.rs` の
//! seam と同じ扱い）。
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

/// 保存先の選択の題名。
const SAVE_TITLE: &str = "名前を付けて保存";

/// 保存先の選択の承認の操作の表示名（GTK の選択器でのみ使う）。
#[cfg(target_os = "linux")]
const SAVE_ACCEPT_LABEL: &str = "保存";

/// 保存先の選択の取り消しの操作の表示名（GTK の選択器でのみ使う）。
#[cfg(target_os = "linux")]
const SAVE_CANCEL_LABEL: &str = "キャンセル";

/// 出所を持たないドキュメントに与える既定の提案名（design.md「DialogGate」の
/// 「提案名は『無題』または既存のファイル名」）。
///
/// **拡張子まで含めた完全なファイル名である。** 利用者がそのまま承認すれば、この名前の
/// ファイルが作られる。形式の側（`document-format`）は拡張子を要求しない（`open` は中身の
/// 型マーカーで判定する）が、**OS と利用者には拡張子が見える**ため、既定にも付ける。
/// 名前の本体はタスク 3.4 の `document_save` が与える `suggested_name` であり、本定数は
/// **出所を持たないドキュメントに対する適応層の既定**である。
pub const DEFAULT_SAVE_NAME: &str = "無題.jxcel";

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
// 保存先の選択（要件 5.2、5.3。呼び出し元は 3.4 の `document_save`）
// ---------------------------------------------------------------------------

/// 選択手段の答えを保存先の答えへ写す。**純粋関数**（GUI 無しで検査できる）。
///
/// **これが「取り消しを誤りにしない」写像そのものである。** [`PickResult::Cancelled`] だけが
/// [`SaveLocation::Cancelled`] になり、[`PickResult::Unavailable`] は
/// [`SaveLocation::Unavailable`] のまま運ばれる。この 2 つを混ぜると、利用者が取り消しただけの
/// ときに失敗が報告される（要件 5.3 が禁じること）。**3 つの腕の対応はこの 1 か所だけに書く** —
/// プラットフォームごとの `pick_save` が別々に写すと、片方だけが取り消しを誤りへ倒しうる。
///
/// **両方のプラットフォームがこの関数を通る**（Linux は `gtk` の答えを、Windows / macOS は
/// `rfd` の答えを写す）。**Linux 以外では [`PickResult::Unavailable`] を構築しない** — `rfd` の
/// `save_file` は「取り消し」と「提示できなかった」を区別せず、どちらも `None` を返すためである
/// （その判断は [`pick_save`](pick_save) の Windows / macOS 版の doc にある）。したがって
/// **[`PickResult::Unavailable`] をこの写像へ渡すのは Linux だけである**が、写像そのものは
/// プラットフォームに依らない（`cfg` を付けない）— 「取り消しは取り消しのまま」という規則は
/// どの OS でも同じであり、片方のビルドでしか検査できない規則を作らないためである。
fn save_location_from_pick(result: PickResult) -> SaveLocation {
    match result {
        PickResult::Cancelled => SaveLocation::Cancelled,
        PickResult::Picked(path) => SaveLocation::Chosen(path),
        PickResult::Unavailable(message) => SaveLocation::Unavailable(message),
    }
}

/// 保存先の選択の結果（design.md「DialogGate（保存先の選択）」）。
///
/// **`Cancelled` は誤りではない。** 利用者が取り消したという正常な結果であり、保存の指示は
/// 「何も書き出さず、未保存のまま保つ」へ進む（要件 5.3）。設計の対応表（design.md「Error
/// Handling」）では `SaveReport::Cancelled` → `DocumentSaveOutcome::Cancelled` → 封筒の成功腕
/// であり、**この 3 段のどれにも誤りは現れない**。取り消しを `Unavailable` に混ぜると、利用者が
/// 取り消しただけのときに失敗が報告される（既存の [`PickResult`] と同じ判断）。
///
/// **`Unavailable` だけが失敗である。** 提示できなかった理由を運び、3.4 の `document_save` が
/// 利用者へ伝えるかどうかを決める。
///
/// **位置は外へ出さない。** `Chosen` が運ぶ `PathBuf` は本関数の Rust の呼び出し元だけが読み、
/// コマンドの応答（境界の `DocumentSaveResponse`）へは写さない（design.md
/// 「Boundary Commitments」）。書き出しの相手は同じ関数の内側で完結する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveLocation {
    /// 保存先が選ばれた。**位置は Rust の側にだけ留まる。**
    Chosen(PathBuf),
    /// 利用者が取り消した。**何も書き出さず、未保存を保つ**（要件 5.3）。
    ///
    /// 提示できなかったこと（[`SaveLocation::Unavailable`]）と混ぜてはならない。
    Cancelled,
    /// 選択手段を提示できなかった（理由を運ぶ）。**これだけが失敗である。**
    ///
    /// **Linux でのみ生じる。** GTK のウィンドウはメインスレッドでしか触れないため、提示は
    /// メインスレッドへ依頼する。その依頼に失敗した場合と、親ウィンドウが既に失われている場合が
    /// ここに来る。**Windows / macOS の `rfd` は「提示できなかった」と「取り消し」を区別して
    /// 返さない**（`save_file` はどちらも `None` である）ため、この腕は既定のビルドのこの 2 つ
    /// では構築されない — [`PickResult::Unavailable`] と同じ扱いである。
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Unavailable(String),
}

/// 記録に出す 1 行。**位置そのものは書かない**（本番の記録にパスを残さない。module doc
/// 「本機能はパスを読まない」の規律は保存先でも同じ）。
///
/// **取り消しと提示できなかったことを書き分ける。** 3.4 の実画面の観測はこの行で行う
/// （画面からの保存が取り消されたとき、この行が出て何も書き出されないことを確かめる）。
fn describe_save_location(location: &SaveLocation) -> String {
    match location {
        SaveLocation::Chosen(_) => "保存先が選ばれた".to_owned(),
        SaveLocation::Cancelled => "利用者が保存先の選択を取り消した".to_owned(),
        SaveLocation::Unavailable(message) => {
            format!("保存先の選択を提示できなかった（{message}）")
        }
    }
}

/// 提案名を組み立てる。**純粋関数**（GUI 無しで検査できる）。
///
/// 規則は design.md「DialogGate」の 1 行そのものである: **出所から既存のファイル名が得られれば
/// それ**、無ければ既定の `無題`。呼び出し元（`document_save`）はコアの状態
/// （`app-shell` の `DocumentSummary::name`。出所を持たない新規の文書は空文字）から名前を取って
/// ここへ渡す。
///
/// **`None` と空文字を同じ扱いにする**のは、境界の写像が「出所を持たない新規の文書は空文字」と
/// 定めているためである。両者は呼び出し元にとって同じ「使える名前が無い」状態であり、区別して
/// も提示の結果は変わらない（空文字をそのまま GTK や `rfd` へ渡すと、提案名が空欄になる）。
///
/// 出所を持つ文書の保存はコアの `save` が位置を知っているため保存先を尋ねず、本関数を呼ぶのは
/// **出所を持たない文書の保存**だけである。
pub fn suggested_save_name(origin_file_name: Option<&str>) -> String {
    match origin_file_name {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => DEFAULT_SAVE_NAME.to_owned(),
    }
}

/// 保存先の選択を提示し、選ばれた位置を返す。**ブロックする。**
///
/// 引数の `window` が**親**である（module doc「親ウィンドウは必ず指定する」）。`suggested_name`
/// は選択器の入力欄の初期値であり、出所を持たない文書のための提案名である
/// （[`suggested_save_name`]）。
///
/// **イベントループのスレッドから呼んではならない。** 呼び出し元は
/// [`document_save`](crate::session::commands::document_save) であり、`spawn_blocking` の
/// スレッドで呼ぶ（module doc「実行モデル」）。提示そのものは [`pick_save`] が行う
/// （プラットフォーム差はそこだけにある）。
///
/// **`Cancelled` は正常な結果であり、`Unavailable` だけが失敗である**（[`SaveLocation`]）。
/// 取り消しのときに書き出しが起きないこと（要件 5.3）は、呼び出し元が `Cancelled` で
/// **コアの `save_to` を呼ばない**ことで成立する。本関数は何も書き出さない。
///
/// **本関数は位置を応答へ写す経路を持たない。** 返すのは Rust の列挙であり、選ばれた位置を
/// 読むのは呼び出し元（`save_to` へ渡す）だけである。応答の型（境界の
/// `DocumentSaveResponse`）は `status` と `outcome` だけを運ぶ。
pub fn pick_save_location(window: &WebviewWindow, suggested_name: &str) -> SaveLocation {
    let location = pick_save(window, suggested_name);
    log::info!(
        "保存先の選択: 対象ウィンドウ = {} / 結果 = {}",
        window.label(),
        describe_save_location(&location)
    );
    location
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
///
/// # 引き渡しが成立したら状態変化を通知する（ここが唯一の場所である）
///
/// **この関数は、利用者が選んだドキュメントを所有者へ引き渡す唯一の場所である。** メニューの
/// 「開く…」（[`install`]）もコマンドの `pick_document_file` も、同じ [`pick_and_hand_off`] を
/// 通ってここへ来る。引き渡しは**セッションの状態を変える**（未解決のウィンドウが文書を
/// 保持し始める）ので、design.md「セッション状態の通知」が挙げる変化の 1 つ（**読み込み**）に
/// 当たる。したがって成立したときに [`crate::session::commands::emit_session_changed`] を
/// **1 回だけ**送る。
///
/// **送らないと、メニュー経由の「開く…」が状態を黙って変える。** 画面は通知を購読して
/// 問い合わせ直す設計であり（`EmptyWindowScreen` の doc「状態が変わったら問い合わせ直す」）、
/// コマンド経路の応答もこの通知も無いメニュー経路では、開いたあとも画面が
/// 「ドキュメントを保持していません」を出し続ける（実測: 2026-09-13 の実画面。メニューの
/// 「開く…」→ `委譲先が引き渡しを受け入れた` → **`document_state` の問い合わせが 1 回も
/// 起きない** → 画面は更新されない）。
///
/// **emit の規則をここへ写さない。** 送るのは成功の腕だけであり（拒否・提示できなかったことは
/// 何も変えない）、送信そのものは [`crate::session::commands::emit_session_changed`] が持つ
/// 1 箇所の実装をそのまま呼ぶ。この関数は「どの操作が送るか」を判断せず、**成立したという
/// 事実**だけを渡す。
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
            // 引き渡しはセッションの状態を変えた（未解決のウィンドウが文書を保持し始める）。
            // 画面が古い状態を持ち続けないよう、対象ウィンドウへ 1 回通知する。
            crate::session::commands::emit_session_changed(window);
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

/// GTK のネイティブ選択器に与える設定（「開く」と「保存」の差のすべて）。
///
/// **この値を組み立てる部分は純粋であり、GUI 無しで検査できる**（[`show_chooser`] は値で GTK を
/// 呼ぶだけである）。差は題名・動作・承認と取り消しの表示名・入力欄の初期値の 5 つであり、
/// **「保存」で `FileChooserAction::Save` と提案名を使うこと**が要件 5.2 の Linux 側の実体である。
///
/// [`ChooserPlan::open`] と [`ChooserPlan::save`] の 2 つの入口は、`TITLE` / `ACCEPT_LABEL` /
/// `CANCEL_LABEL`（開く）と `SAVE_*`（保存）を混同しないためにある — 題名だけ保存で動作が開く、
/// という食い違いを型の側で作れなくする。
///
/// **提案名を借用ではなく所有で持つ。** 提示は `AppHandle::run_on_main_thread` へ**クロージャを
/// 移して**依頼するため、その中身は `'static` でなければならない（借用のままだとコンパイルが
/// 通らない）。保存の 1 回につき 1 つの文字列を作るだけで、待ち時間は選択器の表示が占める。
#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct ChooserPlan {
    /// 選択器の題名。
    title: &'static str,
    /// 選択器の動作（開く / 保存）。
    action: gtk::FileChooserAction,
    /// 承認の操作の表示名。
    accept: &'static str,
    /// 取り消しの操作の表示名。
    cancel: &'static str,
    /// 入力欄へ入れる初期値（`set_current_name` へ渡す値）。
    ///
    /// **保存のときだけ `Some`** である。開く動作で名前を入れると「開く対象のファイル名を
    /// 入力欄に入れる」ことになり、選択の意味が変わる。
    current_name: Option<String>,
}

#[cfg(target_os = "linux")]
impl ChooserPlan {
    /// 既存のファイルを開く設定（要件 2.4）。
    fn open() -> Self {
        Self {
            title: TITLE,
            action: gtk::FileChooserAction::Open,
            accept: ACCEPT_LABEL,
            cancel: CANCEL_LABEL,
            current_name: None,
        }
    }

    /// 保存先を選ぶ設定（要件 5.2）。**提案名を入力欄の初期値に持つ。**
    fn save(suggested_name: String) -> Self {
        Self {
            title: SAVE_TITLE,
            action: gtk::FileChooserAction::Save,
            accept: SAVE_ACCEPT_LABEL,
            cancel: SAVE_CANCEL_LABEL,
            current_name: Some(suggested_name),
        }
    }
}

/// 選択手段を提示し、その完了を待つ。**ブロックする。**
///
/// 引数の `window` が**親**である（module doc「親ウィンドウは必ず指定する」）。
#[cfg(target_os = "linux")]
fn pick(window: &WebviewWindow) -> PickResult {
    pick_with_plan(window, ChooserPlan::open())
}

/// 保存先を選び、選ばれた位置を待つ（要件 5.2）。**ブロックする。**
///
/// 呼び出し元は 3.4 の `document_save` であり、`spawn_blocking` のスレッドで呼ぶ。
#[cfg(target_os = "linux")]
fn pick_save(window: &WebviewWindow, suggested_name: &str) -> SaveLocation {
    // 提案名を 1 つだけ所有の文字列にする（メインスレッドへ移すクロージャが `'static` を要する
    // ため。`ChooserPlan` の doc を参照）。
    let plan = ChooserPlan::save(suggested_name.to_owned());
    // 写像はプラットフォーム共通の純粋関数 1 か所だけである（[`save_location_from_pick`]）。
    save_location_from_pick(pick_with_plan(window, plan))
}

/// メインスレッドへ依頼して選択器を提示し、応答を待つ。**ブロックする。**
///
/// `plan` が「開く」と「保存」の差である（GTK の側の差は [`show_chooser`] が解釈する）。
#[cfg(target_os = "linux")]
fn pick_with_plan(window: &WebviewWindow, plan: ChooserPlan) -> PickResult {
    use std::sync::mpsc;

    // GTK はメインスレッドでしか触れない（`WebviewWindow::gtk_window` も同じ）。したがって
    // **提示はメインスレッドへ依頼し、完了はこのスレッドで待つ** — メインスレッドは入れ子の
    // ループに入らない。
    let (sender, receiver) = mpsc::channel();
    let target = window.clone();
    if let Err(error) = window.app_handle().run_on_main_thread(move || {
        show_chooser(&target, plan, sender);
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
///
/// `plan` が「開く」と「保存」の差のすべてである（題名・動作・表示名・入力欄の初期値）。
/// **選択器の組み立てと応答の処理は 1 本に保つ** — 2 つの動作で別々に書くと、参照の保持・親の
/// 破棄の扱い・`filename()` の解釈という壊れやすい部分が二重になる。
#[cfg(target_os = "linux")]
fn show_chooser(window: &WebviewWindow, plan: ChooserPlan, sender: Sender<PickResult>) {
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
        Some(plan.title),
        Some(&parent),
        plan.action,
        Some(plan.accept),
        Some(plan.cancel),
    );
    chooser.set_modal(true);

    // **保存のときだけ提案名を入力欄の初期値にする。** `set_current_name` は「まだ存在しない
    // ファイルの名前」を入力欄へ入れる GTK の口であり、保存の動作でだけ意味を持つ（開く動作で
    // 呼ぶと、開く対象のファイル名を入力欄へ入れることになり、選択の意味が変わる）。
    if let Some(name) = plan.current_name.as_deref() {
        chooser.set_current_name(name);
    }

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

/// 保存先を選び、選ばれた位置を待つ（Windows / macOS。要件 5.2）。**ブロックする。**
///
/// 引数の `window` が**親**である（[`pick`] と同じ理由）。
/// `set_file_name` が入力欄の初期値に提案名を入れる（Linux の `set_current_name` に対応する）。
///
/// **`None` は取り消しとして扱う。** `rfd` の `save_file` は「利用者が取り消した」と
/// 「提示できなかった」を区別して返さない（どちらも `None` である）。**この 2 つを区別できない
/// こと自体が、この 2 つのプラットフォームの契約である** — 取り消しを `Unavailable` に倒すと
/// 利用者が取り消しただけのときに失敗が報告され（要件 5.3 が禁じる）、逆に倒すと提示できなかった
/// ことが黙る。**要件 5.3（取り消しは正常な結果）が満たされる側を選ぶ。**
/// [`PickResult::Unavailable`] が同じ理由で既定のビルドでは構築されないのと同じ判断である。
///
/// 写像はこの腕でも[`save_location_from_pick`] を通す（取り消しを誤りにしない規則を 2 か所に
/// 書かない）。`rfd` の `None` が [`PickResult::Cancelled`] に落ちるのはそのためである。
#[cfg(not(target_os = "linux"))]
fn pick_save(window: &WebviewWindow, suggested_name: &str) -> SaveLocation {
    let chosen = rfd::FileDialog::new()
        .set_title(SAVE_TITLE)
        .set_parent(window)
        .set_file_name(suggested_name)
        .save_file();
    let picked = match chosen {
        Some(path) => PickResult::Picked(path),
        None => PickResult::Cancelled,
    };
    save_location_from_pick(picked)
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
        describe_save_location, document_menu_path, open_document_spec, save_location_from_pick,
        suggested_save_name, to_boundary, HandOff, PickResult, SaveLocation, DEFAULT_SAVE_NAME,
        OPEN_ACCELERATOR_SPELLING, OPEN_ITEM_ID, OPEN_LABEL, OWNER,
    };
    use crate::menu::{MenuNode, MenuPath, MenuRegistry};

    /// 一時ディレクトリ（`session/host.rs` のテストと同じ形。保存先が無い文書の保存で
    /// **何も書き出されない**ことをディレクトリの空さで見るために使う）。
    struct Scratch {
        path: std::path::PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("時計は 1970 以降である")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "jxcel-dialog-{tag}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
            Self { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

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

    // -----------------------------------------------------------------------
    // 保存先の選択（要件 5.2、5.3。呼び出し元は 3.4 の `document_save`）
    // -----------------------------------------------------------------------

    /// **取り消しは正常な結果であり、失敗ではない**（要件 5.3）。
    ///
    /// 3 つの答えは互いに区別でき（等値比較で混ざらない）、`Cancelled` は `Unavailable` とも
    /// `Chosen` とも等しくない。**取り消しが誤りとして表現される経路が無いこと**をここで固定する
    /// — `DocumentSaveOutcome::Cancelled`（封筒の成功腕）へ写すのは 3.4 であり、その写像が
    /// 取り消しを失敗の腕へ落とすなら、利用者が取り消しただけのときに失敗が報告される。
    #[test]
    fn cancellation_is_a_normal_answer_distinct_from_unavailability() {
        let chosen = SaveLocation::Chosen(std::path::PathBuf::from("/tmp/無題.jxcel"));
        let cancelled = SaveLocation::Cancelled;
        let unavailable = SaveLocation::Unavailable("親ウィンドウを取得できなかった".to_owned());

        // 3 つの答えが互いに異なる（`SaveLocation` の等値が判別子として使える）。
        assert_ne!(cancelled, chosen);
        assert_ne!(cancelled, unavailable);
        assert_ne!(chosen, unavailable);
        assert_eq!(cancelled, SaveLocation::Cancelled, "取り消しは取り消しである");

        // 取り消しの記録の行は「提示できなかった」の行と異なる（実画面の観測が取り違えない）。
        assert_ne!(
            describe_save_location(&cancelled),
            describe_save_location(&unavailable),
            "取り消しと提示できなかったことが記録で同じ行になる"
        );
        assert_ne!(
            describe_save_location(&cancelled),
            describe_save_location(&chosen)
        );

        // **取り消しの行は失敗を意味する語を持たない**（利用者が取り消しただけである）。
        let line = describe_save_location(&cancelled);
        assert!(
            line.contains("取り消"),
            "取り消しの行が取り消しと読めない: {line}"
        );
        assert!(
            !line.contains("できなかった"),
            "取り消しが「できなかった」と記録される: {line}"
        );
    }

    /// **提案名が選択器へ渡る値になる。** [`super::ChooserPlan`] を組み立て、その
    /// `current_name` と `action` を読む。
    ///
    /// **GUI は開かない。** 選択器を組み立てる部分（[`super::ChooserPlan`]）は値であり、
    /// それを読むだけで「呼び出し元が与えた提案名が入力欄の初期値になる」ことが確かめられる。
    ///
    /// **本テストが拘束するのは `ChooserPlan` までである。** `set_current_name` /
    /// `FileChooserNative::new` の**呼び出し地点そのものは拘束しない** — それらは実機の
    /// ウィンドウを要する `show_chooser` の中にあり、このテストはそれを呼ばない。したがって
    /// 「保存の設定を組み立てたのに開く動作で提示する」という食い違いは、`ChooserPlan` を
    /// 経由しない限り本テストでは捕まらない（`show_chooser` が `plan.action` と
    /// `plan.current_name` をそのまま使うことは、`show_chooser` の 1 本の実装とレビューで
    /// 抑える）。**実際に OS の選択器が描く文字は実画面の観測**（design.md「E2E / 3 OS の
    /// 観測」）**で確かめるほかない。**
    #[cfg(target_os = "linux")]
    #[test]
    fn the_suggested_name_reaches_the_save_chooser() {
        let plan = super::ChooserPlan::save("棚卸し.jxcel".to_owned());
        assert_eq!(
            plan.current_name.as_deref(),
            Some("棚卸し.jxcel"),
            "呼び出し元の提案名が入力欄の初期値に入らない"
        );
        assert_eq!(
            plan.action,
            gtk::FileChooserAction::Save,
            "保存の設定が保存の動作でない（開く動作では提案名を入れても意味が無い）"
        );

        // **開く動作は提案名を持たない**（開く対象のファイル名を入力欄へ入れる意味は無い）。
        let open = super::ChooserPlan::open();
        assert_eq!(open.current_name, None);
        assert_eq!(open.action, gtk::FileChooserAction::Open);

        // 題名・表示名も動作ごとに取り違えない。
        assert_ne!(plan.title, open.title);
        assert_ne!(plan.accept, open.accept);
    }

    /// 出所を持たない文書の既定の提案名が定義され、境界の写像（新規は空文字）と同じ意味を
    /// 持つ（design.md「DialogGate」の「提案名は『無題』または既存のファイル名」）。
    ///
    /// **空文字をそのまま選択器へ渡さない**ことが本関数の役目である（空の入力欄は利用者に
    /// 何も示さない）。出所がある文書は保存先を尋ねない（コアの `save` が位置を知っている）ので、
    /// 本関数が使われるのは出所を持たない場合だけである（3.4）。
    #[test]
    fn the_default_suggested_name_stands_for_a_document_without_an_origin() {
        assert_eq!(
            suggested_save_name(None),
            DEFAULT_SAVE_NAME.to_owned(),
            "出所が無い文書に既定の提案名を与えない"
        );
        assert_eq!(
            suggested_save_name(Some("")),
            DEFAULT_SAVE_NAME.to_owned(),
            "空文字（境界の写像の「新規」）に既定の提案名を与えない"
        );
        assert_eq!(
            suggested_save_name(Some("既存.jxcel")),
            "既存.jxcel".to_owned(),
            "出所がある文書のファイル名をそのまま使わない"
        );
        // 既定の名前は空でなく、拡張子まで含む完全なファイル名である。
        assert!(!DEFAULT_SAVE_NAME.is_empty());
        assert!(
            DEFAULT_SAVE_NAME.ends_with(".jxcel"),
            "既定の提案名が形式の拡張子で終わらない: {DEFAULT_SAVE_NAME}"
        );
        assert!(
            !DEFAULT_SAVE_NAME.starts_with('.'),
            "既定の提案名が隠しファイルになる: {DEFAULT_SAVE_NAME}"
        );
    }

    /// 選択手段の答えから保存先の答えへの写像が、**取り消しを誤りへ倒さない**こと
    /// （要件 5.3）。これが 3.4 の `document_save` の判断の土台である。
    ///
    /// 3 つの腕を**それぞれ別の値で**検査する:
    ///
    /// - `Picked(p)` → `Chosen(p)`。**位置が保たれる**（`p` を書き換えると落ちる）。
    /// - `Cancelled` → `Cancelled`。**`Chosen` でも `Unavailable` でもない**
    ///   （取り消しを位置や失敗へ倒す変異で落ちる）。
    /// - `Unavailable(m)` → `Unavailable(m)`。**理由が保たれる。**
    ///
    /// **`assert_ne!` を並べるのは、取り消しの腕が「たまたま通る」ことを許さないためである。**
    /// `Chosen` を返す変異は `assert_eq!(cancelled, SaveLocation::Cancelled)` だけでも落ちるが、
    /// 3 つの答えが互いに異なることを明示しておくと、後で腕が増えたときに「どれか 1 つ」で
    /// 通るテストにならない。
    #[test]
    fn the_pick_mapping_keeps_cancellation_cancelled() {
        let chosen_path = std::path::PathBuf::from("/tmp/選ばれた.jxcel");

        let picked = save_location_from_pick(PickResult::Picked(chosen_path.clone()));
        let cancelled = save_location_from_pick(PickResult::Cancelled);
        let unavailable =
            save_location_from_pick(PickResult::Unavailable("親ウィンドウを取得できなかった".to_owned()));

        // 位置は保たれる（別の位置へ写す変異で落ちる）。
        assert_eq!(picked, SaveLocation::Chosen(chosen_path));
        // 取り消しは取り消しのままである（位置や失敗へ倒す変異で落ちる）。
        assert_eq!(cancelled, SaveLocation::Cancelled);
        // 提示できなかったことは理由ごと運ばれる。
        assert_eq!(
            unavailable,
            SaveLocation::Unavailable("親ウィンドウを取得できなかった".to_owned())
        );
        // 3 つの答えは互いに異なる（写像が腕を混ぜていない）。
        assert_ne!(picked, cancelled);
        assert_ne!(cancelled, unavailable);
        assert_ne!(picked, unavailable);
        // **取り消しが正常な結果であることは、`SaveLocation` の外側でも保たれる** —
        // 3.4 が写す先（コア）は取り消しを誤り型に持たない。
    }

    /// **取り消しのときに書き出しが起きず、未保存が保たれる**（要件 5.3）ことを、
    /// 提示の答えの写像・コアの保存の経路・実ファイルの 3 つを**つないで**確かめる。
    ///
    /// 3.4 の `document_save` は「出所が無い → [`super::pick_save_location`] → `Chosen` なら
    /// コアの `save_to`、それ以外は何もしない」という形になる（design.md「保存（出所がある場合と
    /// 無い場合）」の流れ図そのもの）。**その形を、選択器の答え（[`PickResult::Cancelled`]）から
    /// 始めてここで実物で走らせる。**
    ///
    /// **`Cancelled` を直書きしない。** 答えは**選択手段の答え**として与え、[`save_location_from_pick`]
    /// で保存先へ写してから分岐させる。これにより次の 3 つの変異がいずれも本テストを落とす:
    ///
    /// 1. 写像が `Cancelled` を `Chosen` へ倒す → `save_to` が呼ばれ、`scratch` の中にファイルが
    ///    でき、`save` の応答も `Saved` になる。
    /// 2. 分岐が `Chosen` 以外でも書き出す → `read_dir` がファイルを見つける。
    /// 3. 書き出しが未保存を落とす → `save_to` が成功して（`Chosen` のときだけ通る腕で）印が
    ///    落ちるので、**この変異は「書き出しが起きた」こととして 1 と同じ経路で観測される**。
    ///
    /// **書き出し先は `scratch` の中に置く。** 写像が位置を `Chosen` へ通してしまえば、その位置は
    /// 一時ディレクトリの中であり、`read_dir` が空でなくなる（外の `/tmp` を指すと観測できない）。
    ///
    /// **本テストが証明しないこと**: 選択器そのものを実画面で取り消す操作は、選択器が GTK の
    /// メインループの中でしか走らないためここでは起こせない（`session/commands.rs` の
    /// `answer_save` のテストが、提示を差し替えて「取り消しなら書き出さない」を本物のコアで
    /// 固定する）。本テストが証明するのは「取り消しの答えを受けた適応層が何もしない」ことである。
    #[test]
    fn a_cancelled_save_location_writes_nothing_and_keeps_the_document_unsaved() {
        use app_shell::ipc::WindowLabel;
        use document_session::{CloseAnswer, DocumentSessionsApi, SaveReport, SessionState};

        let scratch = Scratch::new("dialog-save-cancelled");
        let window = WindowLabel::new("empty-1");
        let sessions = document_session::DocumentSessions::new();

        // 出所を持たない文書（要件 7.1、7.2）。保存先を持たないため、保存は選択を要する。
        sessions.create(&window).expect("新規作成できる");
        // 変更を 1 回適用して未保存を立てる（適用の口は本機能ではない。未保存と版の記録は
        // コアの `edit` が行い、その契約は `document-session` のテストが固定している）。
        let edited = sessions
            .edit(&window, &mut |document| {
                document.add_sheet("棚卸し");
            })
            .expect("変更を適用できる");
        assert!(edited.unsaved, "適用のあと未保存にならない");
        assert!(edited.revision >= 1, "適用の版が進んでいない");

        // **選択器が取り消しを返した**ところから始める。3.4 が受け取る形と同じ経路を通す。
        let location = save_location_from_pick(PickResult::Cancelled);
        // 書き出し先は `scratch` の中に置く（`Chosen` へ倒す変異がここで観測できるようにする）。
        let destination = scratch.path().join("無題.jxcel");
        let wrote = match location {
            SaveLocation::Chosen(_path) => {
                // `Chosen` が来たときだけ書き出す（3.4 の判断）。**行き先は `scratch` の中**で
                // あり、外の `PathBuf` をそのまま使うと観測がディレクトリの外へ逃げる。
                sessions
                    .save_to(&window, &destination)
                    .expect("書き出しは失敗を結果として返す");
                true
            }
            SaveLocation::Cancelled | SaveLocation::Unavailable(_) => false,
        };
        assert!(!wrote, "取り消しが書き出しへ進んだ");

        // **何も書き出されていない。** 写像が取り消しを `Chosen` へ倒すか、分岐が取り消しでも
        // 書き出せば、このディレクトリにファイルが現れる（`read_dir` が空でなくなる）。
        assert!(
            std::fs::read_dir(scratch.path())
                .expect("一時ディレクトリを読める")
                .next()
                .is_none(),
            "取り消しなのに書き出した: {:?}",
            std::fs::read_dir(scratch.path())
                .expect("一時ディレクトリを読める")
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Vec<_>>()
        );
        assert!(!destination.exists(), "書き出し先が作られた");
        // 未保存が保たれ、閉じてよくないままである（要件 5.3、4.5、4.6）。
        assert!(
            matches!(
                sessions.state(&window),
                SessionState::Open { unsaved: true, .. }
            ),
            "取り消しが未保存を落とした"
        );
        assert_eq!(
            CloseAnswer::Deny,
            sessions.may_close(&window),
            "取り消しのあとに閉じてよいと答えた"
        );
        // 出所も変わっていない（次の保存も同じく選択を要する。要件 5.3 の「未保存のまま」）。
        assert!(
            matches!(sessions.save(&window), Ok(SaveReport::NeedsLocation)),
            "取り消しのあとの保存が出所を持った"
        );
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

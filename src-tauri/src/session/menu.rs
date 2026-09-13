//! 「新規」と「保存」をメニューへ登録する — 画面を経由しない 2 つ目の入口。
//!
//! 所有: `SessionMenu`（design.md「Components and Interfaces → adapter → SessionMenu」）。
//! 要件: 5.1（保存の指示）、7.1（新規作成の指示）。タスク 3.6。
//!
//! 本モジュールが置くのは 4 つである。
//!
//! 1. **登録内容**（[`new_item_spec`] / [`save_item_spec`]）— 登録元 `document-session`、項目の
//!    識別子 `document-session.new` / `document-session.save`、位置 `ファイル`、表示名
//!    「新規」「保存」、**プラットフォーム解決済みのショートカット**（非 macOS `Ctrl+N` /
//!    `Ctrl+S`、macOS `Cmd+N` / `Cmd+S`）。登録は 7.4 の登録口
//!    （[`MenuRegistry::register`]）を通すので、**ショートカットの競合は登録時に登録元へ報告される**
//!    （片方を黙って捨てる経路は無い。要件 3.4）
//! 2. **活性化の対象ウィンドウの解決**（[`target_window`]）— `dialog.rs` の「開く」と同じ形。
//!    対象は [`MenuSelection::window`]（7.5 の振り向け）が与えるラベルであり、利用者が指定する
//!    値ではない
//! 3. **実行モデル** — 活性化は [`spawn_blocking`](tauri::async_runtime::spawn_blocking) へ渡す。
//!    **メニューの処理はイベントループのスレッドで走るため、そこで待ってはならない**
//!    （保存先の提示は利用者が答えるまで、保存は 10 万行で秒単位かかる。`dialog.rs`
//!    「実行モデル」と同じ判断）
//! 4. **状態変化の通知** — 状態が実際に変わったときだけ、`commands::emit_session_changed` が
//!    対象ウィンドウへ 1 回送る
//!
//! # 経路の選択（画面からの操作とメニューからの操作で結果を一致させる）
//!
//! 本モジュールの活性化は、**対応するコマンドの本体をそのまま呼ぶ** — 2 つ目の実装を持たない。
//!
//! | 項目 | 呼ぶ本体 | 画面からの対応 |
//! |---|---|---|
//! | 「新規」 | [`commands::answer_new`] | `document_new` |
//! | 「保存」 | [`commands::answer_save`]（提示は [`dialog::pick_save_location`]） | `document_save` |
//!
//! **これがタスク 3.6 の受入（「項目の選択が対応するコマンドと同じ経路を通る」）の実体である。**
//! メニュー面はフロントエンドを経由せず Rust 側で完結するため（design.md「セッション状態の通知」）、
//! 同じ処理をここへ写すと判定（`should_notify`）と保存先の提示の順序が 2 か所に分かれ、**画面から
//! の保存とメニューからの保存で結果が食い違いうる**。写す代わりに本体を共有するので、
//! 次はどちらの入口でも同じである:
//!
//! - **未保存があるときの新規作成の拒否**（要件 7.3。`Refused { reason }` が返り、何も変わらない）
//! - **出所を持たない文書の保存で保存先を尋ねること**（要件 5.2。`answer_save` の `pick` に
//!   [`dialog::pick_save_location`] を渡す — コマンド面の `document_save` と同じ）
//! - **取り消しは正常な結果であり、何も書き出さないこと**（要件 5.3。`Cancelled`）
//! - **`should_notify` が唯一の通知の規則であること**（変わらない操作でイベントが飛ばない）
//!
//! メニュー面が自分で持つのは「結果を返す先が無い」ことの扱いだけである — 境界の封筒
//! （`IpcResult` / `*Response`）は組まず、**結果と状態を記録に 1 行残す**（下の「記録」）。
//!
//! # 通知は 1 回だけ・状態が変わったときだけ
//!
//! [`activate_new`] / [`activate_save`] は本体が返した「送るべきか」を見て、真のときだけ
//! `notify` を 1 回呼ぶ。順序と回数をこの 1 箇所に置くことで、**メニュー面でもイベントが
//! 二重に飛ぶ経路・飛ばない経路が生まれない**。`notify` を差し替え可能にしてあるのは、
//! **GUI 無しで通知の回数を数えられるようにするため**である（回数は実画面では数えにくい。
//! `verification.md` の「速度ではなく呼び出しの形を数える」と同じ観点）。
//!
//! # 有効・無効の述語は与えない（判断と理由）
//!
//! [`MenuItemSpec::with_enablement`] は**与えない**。理由は 2 つある。
//!
//! 1. **述語が受け取るのは生成要求の関連付けであり、セッションの状態ではない。**
//!    [`crate::menu::MenuTarget::document`] は `WindowRegistry` の「そのウィンドウがドキュメントの位置を
//!    指定して作られたか」であり、`attach` でも新規作成でも更新されない（design.md
//!    「既存の関連付けとの関係」）。これを「保存できるか」の判定に使うと、**新規作成した直後の
//!    文書の保存を無効化する**（関連付けが無いため）
//! 2. **再計算の機会がフォーカス移動とウィンドウの集合の変化に限られる**（[`crate::menu::refresh`]）。
//!    セッションの状態は「新規」の活性化や保存の成功でも変わるので、**有効のまま古びる**
//!    （新規で文書ができたのに「保存」が無効のまま残る等）。誤って無効化された項目は
//!    利用者から見て「壊れたメニュー」であり、**コマンド経路が既に安全な答えを返す**
//!    （保持していないウィンドウの保存は `Failed { reason }`、新規作成は `Refused { reason }`）
//!    以上、無効化で得るものが無い
//!
//! したがって **7.5 の `with_enablement` の seam（`#[allow(dead_code)]`）はそのまま**にする
//! （あれは 9.5 の診断の項目と検証専用の項目のためのものであり、本モジュールは使わない）。
//!
//! # 記録（実画面の観測の根拠）
//!
//! 活性化のあとに 1 行残す:
//!
//! ```text
//! document_new: メニュー項目の選択で実行した: ウィンドウ = … / 結果 = 用意した / 状態 = 保持している
//! ```
//!
//! **語はコマンド面と同じものを使う**（[`commands::describe_new`] / [`commands::describe_save`] /
//! [`commands::describe_status`]）。同じ操作がどちらの入口からでも同じ語で記録されるので、
//! 実画面の観測（メニューの活性化 → 記録の行）と、コマンド経路の行を突き合わせられる。
//! 選択そのものの行は 7.5 の `dispatch` が残す（`メニュー項目が選択された: 登録元=… 項目=…`）。
//!
//! # 起動の結線
//!
//! [`install`] は `session::install` の**最後**に 1 回呼ぶ（`lifecycle::run` は `menu::install` →
//! `dialog::install` → `session::install` の順に呼ぶ）。メニューの登録口（`MenuRegistry`）は
//! `menu::install` が管理状態へ置いた後に使えるので、`dialog::install` と同じ起動段に並ぶ。
//! **ウィンドウはまだ 1 枚も無い** — ウィンドウ単位のメニュー（Windows / Linux）への配置は
//! 生成時（`crate::menu::attach_to_window`）が受け取る。

use std::sync::Arc;

use app_shell::ipc::{command_names, DocumentNewOutcome, DocumentSaveOutcome, WindowLabel};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_log::log;

use crate::dialog::{self, SaveLocation};
use crate::menu::{MenuItemSpec, MenuPath, MenuRegistry, MenuSelection};
use crate::session::commands;
use crate::session::watch::WindowDestroyWatch;

/// 登録元の識別子（`AcceleratorOwner`）。**本スペックの項目はすべてこの名前空間を使う。**
///
/// 7.4 の組み込み項目（`app-shell`）と分けるのは、**項目の識別子の重複を登録元ごとに
/// 閉じる**ためである（`MenuRegistrationError::ItemIdConflict` は別の登録元の重複を拒否する）。
const OWNER: &str = "document-session";

/// 「新規」の項目の識別子。**アプリ全体で一意でなければならない**
/// （`app-shell.open-document` は 3.5/7.7 が使っている）。
const NEW_ITEM_ID: &str = "document-session.new";

/// 「保存」の項目の識別子。
const SAVE_ITEM_ID: &str = "document-session.save";

/// 「新規」の表示名（design.md「SessionMenu」）。
const NEW_LABEL: &str = "新規";

/// 「保存」の表示名。
const SAVE_LABEL: &str = "保存";

/// 「新規」のショートカット（非 macOS）。**プラットフォーム解決済みの綴りである** —
/// `CmdOrCtrl+N` は 4.6 の構文契約が受理せず、登録時に
/// `MenuRegistrationError::AcceleratorSyntax` になる（`crate::menu` の module doc）。
#[cfg(not(target_os = "macos"))]
const NEW_ACCELERATOR_SPELLING: &str = "Ctrl+N";

/// 「新規」のショートカット（macOS。メニュー上は `⌘N` と描かれる）。
#[cfg(target_os = "macos")]
const NEW_ACCELERATOR_SPELLING: &str = "Cmd+N";

/// 「保存」のショートカット（非 macOS）。
#[cfg(not(target_os = "macos"))]
const SAVE_ACCELERATOR_SPELLING: &str = "Ctrl+S";

/// 「保存」のショートカット（macOS。メニュー上は `⌘S` と描かれる）。
#[cfg(target_os = "macos")]
const SAVE_ACCELERATOR_SPELLING: &str = "Cmd+S";

/// 2 つの項目を置く部分メニュー。**7.4 が決めたトップレベルの並び**（`ファイル`）に従う。
///
/// 位置の名前をここで書き写さず、`menu` モジュールの定数を参照する（並びと名前の食い違いを
/// 作らない。「開く」と「終了」も同じ場所に並ぶ）。
fn document_menu_path() -> MenuPath {
    MenuPath::new([crate::menu::FILE_MENU_LABEL]).expect("位置は空でない")
}

// ---------------------------------------------------------------------------
// 登録内容（登録口へ渡す値の組み立てだけを切り出す）
// ---------------------------------------------------------------------------

/// 「新規」の登録内容を組み立てる。
///
/// `handler` を差し替えられる形にしてあるのは、**GUI 無しで登録の受理と内容を検査できる**
/// ようにするためである（[`MenuRegistry::enroll`] は画面を要しない。「開く」と同じ形）。
fn new_item_spec(handler: impl Fn(&MenuSelection) + Send + Sync + 'static) -> MenuItemSpec {
    MenuItemSpec::new(
        OWNER,
        NEW_ITEM_ID,
        document_menu_path(),
        NEW_LABEL,
        handler,
    )
    .with_accelerator(NEW_ACCELERATOR_SPELLING)
}

/// 「保存」の登録内容を組み立てる（[`new_item_spec`] と同旨）。
fn save_item_spec(handler: impl Fn(&MenuSelection) + Send + Sync + 'static) -> MenuItemSpec {
    MenuItemSpec::new(
        OWNER,
        SAVE_ITEM_ID,
        document_menu_path(),
        SAVE_LABEL,
        handler,
    )
    .with_accelerator(SAVE_ACCELERATOR_SPELLING)
}

// ---------------------------------------------------------------------------
// 活性化の対象（7.5 の振り向けをウィンドウへ解決する）
// ---------------------------------------------------------------------------

/// 活性化の対象ウィンドウを解決する。**`dialog.rs` の「開く」と同じ手順である。**
///
/// 対象のラベルは [`MenuSelection::window`]（7.5 が活性化の時点で決める）が与える。**解決でき
/// ないときは何もしない** — 対象が無い（どのウィンドウもフォーカスされていない）か、選択と処理の
/// 間にウィンドウが破棄された場合である。**別のウィンドウへ勝手に振り向けない**: 触っていない
/// ウィンドウの文書を差し替えたり保存したりする方が、何もしないより悪い。
///
/// 2 つの項目で同じ手順を使うので 1 つにまとめてある（「開く」は項目が 1 つなので
/// `dialog.rs` の中に直接書かれている）。
fn target_window(
    app: &AppHandle,
    selection: &MenuSelection,
    command: &str,
) -> Option<WebviewWindow> {
    let Some(label) = selection.window().cloned() else {
        log::warn!("{command}: 対象ウィンドウが無いためメニュー項目の処理を行わない");
        return None;
    };
    let Some(window) = app.get_webview_window(label.as_str()) else {
        log::warn!(
            "{command}: 対象ウィンドウ {} は既に無いためメニュー項目の処理を行わない",
            label.as_str()
        );
        return None;
    };
    Some(window)
}

// ---------------------------------------------------------------------------
// 活性化の本体（コマンド面と同じ本体を呼ぶ。GUI 無しで駆動できる）
// ---------------------------------------------------------------------------

/// 「新規」の活性化の本体。**`document_new` と同じ本体（[`commands::answer_new`]）を呼ぶ。**
///
/// 戻り値は境界の結果であり、メニュー面はこれを記録に出すだけで捨てる（応答を返す先が無い）。
///
/// **`notify` は状態が実際に変わったときだけ 1 回呼ばれる。** 判定（`should_notify`）は本体の
/// 内側の 1 箇所にあり、ここは「真なら 1 回送る」という順序だけを持つ — 未保存がある文書への
/// 活性化（`Refused`）や、何も変わらない操作でイベントが飛ばないことは、この形から出る。
fn activate_new<F>(watch: &WindowDestroyWatch, label: &WindowLabel, notify: F) -> DocumentNewOutcome
where
    F: FnOnce(),
{
    let (outcome, changed) = commands::answer_new(watch, label);
    if changed {
        notify();
    }
    outcome
}

/// 「保存」の活性化の本体。**`document_save` と同じ本体（[`commands::answer_save`]）を呼ぶ。**
///
/// `pick` は保存先の選択の縫い目である（コマンド面が
/// [`dialog::pick_save_location`] を渡すのと同じ値を、メニュー面も
/// 渡す）。**出所を持たない文書でのみ呼ばれる** — 出所があれば `answer_save` が出所へ書き出す
/// （要件 5.1）。取り消し（要件 5.3）と提示できなかったことは、コマンド面と同じ 3 つの腕へ写る。
///
/// **`notify` は状態が実際に変わったときだけ 1 回呼ばれる**（[`activate_new`] と同じ規則。
/// 保存の成功だけが状態を変える — 未保存でない文書の保存は何も変えないので送らない）。
fn activate_save<F, P>(
    watch: &WindowDestroyWatch,
    label: &WindowLabel,
    pick: P,
    notify: F,
) -> DocumentSaveOutcome
where
    F: FnOnce(),
    P: FnOnce(&str) -> SaveLocation,
{
    let (outcome, changed) = commands::answer_save(watch, label, pick);
    if changed {
        notify();
    }
    outcome
}

// ---------------------------------------------------------------------------
// 起動時の登録
// ---------------------------------------------------------------------------

/// 起動時に 1 回だけ 2 つの項目を 7.4 の登録口へ登録する（`session::install` の最後が呼ぶ）。
///
/// **登録は [`MenuRegistry::register`] を通す。** したがって次はどちらも登録元（本モジュール）へ
/// 返り、**片方を黙って捨てる経路は無い**:
///
/// - ショートカットの競合（要件 3.4）— 既存の登録（「開く」`Ctrl+O`、「終了」`Ctrl+Q`、
///   診断の 3 項目 `Ctrl+Shift+*`）と衝突すれば `MenuRegistrationError::Accelerator`
/// - 項目の識別子の重複（ほかの登録元が `document-session.new` / `.save` を使っていれば
///   `MenuRegistrationError::ItemIdConflict`）
///
/// **登録に失敗しても起動は続ける**（メニュー項目が引けないことより、アプリが立ち上がらない
/// ことの方が悪い。診断の導線と同じ判断）。
///
/// 選択されたときの処理は 3 段である:
///
/// 1. [`target_window`] で活性化の対象を解決する（無ければ何もしない）
/// 2. 活性化を [`spawn_blocking`](tauri::async_runtime::spawn_blocking) へ渡す（**メニューの
///    処理はイベントループのスレッドなので、そこで待ってはならない**）
/// 3. 本体（[`activate_new`] / [`activate_save`]）を実行し、結果と状態を記録に残す。状態が
///    変わったときは本体の通知が `commands::emit_session_changed` を 1 回呼ぶ
///
/// `AppHandle` を登録時に捕捉するのは、通知の処理が「登録元が自分で用意した処理」への一方的な
/// 呼び出しだからである（`crate::menu::MenuHandler` の doc。「開く」と同じ）。
pub fn install(app: &AppHandle) {
    let registry = app.state::<MenuRegistry>();

    let handling_app = app.clone();
    let new_spec = new_item_spec(move |selection: &MenuSelection| {
        let command = command_names::DOCUMENT_NEW;
        let Some(target) = target_window(&handling_app, selection, command) else {
            return;
        };
        let watch = Arc::clone(&handling_app.state::<Arc<WindowDestroyWatch>>());
        tauri::async_runtime::spawn_blocking(move || {
            let label = WindowLabel::new(target.label());
            let outcome = activate_new(&watch, &label, || commands::emit_session_changed(&target));
            let status = commands::boundary_status(&watch, &label);
            log::info!(
                "{command}: メニュー項目の選択で実行した: ウィンドウ = {} / 結果 = {} / 状態 = {}",
                label.as_str(),
                commands::describe_new(&outcome),
                commands::describe_status(&status),
            );
        });
    });
    if let Err(error) = registry.register(app, new_spec) {
        log::error!("「新規」のメニュー項目を登録できなかった: {error}");
    }

    let handling_app = app.clone();
    let save_spec = save_item_spec(move |selection: &MenuSelection| {
        let command = command_names::DOCUMENT_SAVE;
        let Some(target) = target_window(&handling_app, selection, command) else {
            return;
        };
        let watch = Arc::clone(&handling_app.state::<Arc<WindowDestroyWatch>>());
        tauri::async_runtime::spawn_blocking(move || {
            let label = WindowLabel::new(target.label());
            // 提示と保存は**このスレッドの内側で完結する**（コマンド面と同じ。位置は応答へも
            // 記録へも出ない）。
            let outcome = activate_save(
                &watch,
                &label,
                |suggested| dialog::pick_save_location(&target, suggested),
                || commands::emit_session_changed(&target),
            );
            let status = commands::boundary_status(&watch, &label);
            log::info!(
                "{command}: メニュー項目の選択で実行した: ウィンドウ = {} / 結果 = {} / 状態 = {}",
                label.as_str(),
                commands::describe_save(&outcome),
                commands::describe_status(&status),
            );
        });
    });
    if let Err(error) = registry.register(app, save_spec) {
        log::error!("「保存」のメニュー項目を登録できなかった: {error}");
    }

    log::info!(
        "ドキュメントのメニュー項目を登録した（{} の「新規」「保存」）",
        crate::menu::FILE_MENU_LABEL,
    );
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use app_shell::accelerator::{Accelerator, MenuItemId};
    use app_shell::ipc::{
        DocumentNewOutcome, DocumentSaveOutcome, DocumentSessionStatus, WindowLabel,
    };
    use document_format::{Document, DocumentFormat, DocumentFormatApi, SchemaPart};
    use document_session::{DocumentSessions, DocumentSessionsApi};

    use super::{
        activate_new, activate_save, document_menu_path, new_item_spec, save_item_spec,
        NEW_ACCELERATOR_SPELLING, NEW_ITEM_ID, NEW_LABEL, OWNER, SAVE_ACCELERATOR_SPELLING,
        SAVE_ITEM_ID, SAVE_LABEL,
    };
    use crate::dialog::{self, SaveLocation};
    use crate::menu::{MenuNode, MenuPath, MenuRegistry};
    use crate::session::commands;
    use crate::session::watch::testing::AlwaysPresent;
    use crate::session::watch::WindowDestroyWatch;

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
                "jxcel-session-menu-{tag}-{}-{unique}",
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
    fn write_document(path: &std::path::Path, sheet_name: &str, value: &str) {
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
            .set_row_values(sheet, row, vec![document_format::CellValue::Text(value.to_owned())])
            .expect("標本の行は実在する");
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
    }

    /// 二重のウィンドウの側から入口を作る（テストの標準の組み立て）。
    fn watch() -> WindowDestroyWatch {
        WindowDestroyWatch::new(Arc::new(AlwaysPresent), Arc::new(DocumentSessions::new()))
    }

    /// 未保存の変更を 1 つ適用する（**コアの適用の口**を通す。印を立てる唯一の経路）。
    fn mark_unsaved(watch: &WindowDestroyWatch, label: &WindowLabel) {
        let edited = watch
            .sessions()
            .edit(label, &mut |_document| ())
            .expect("保持しているウィンドウへ適用できる");
        assert!(edited.unsaved, "適用のあとは未保存である");
    }

    /// 状態を**経路の比較に使える形へ射影する**。
    ///
    /// 境界の写しを `Debug` のまま比べてはならない — **シートの識別子は ULID であり、
    /// セッション表が違えば必ず異なる**（生成ごとに一意である）。比べるのは利用者に見える要素
    /// （名前・出所の種類・未保存・シートの名前と件数）だけである。**位置もここには現れない** —
    /// 境界の写像が落としているからであり、そのことも同時に固定される。
    fn comparable(watch: &WindowDestroyWatch, label: &WindowLabel) -> String {
        match commands::boundary_status(watch, label) {
            DocumentSessionStatus::Absent => "保持していない".to_owned(),
            DocumentSessionStatus::Unavailable { reason } => format!("読み込めなかった: {reason}"),
            DocumentSessionStatus::Open(summary) => {
                let sheets: Vec<String> = summary
                    .sheets
                    .iter()
                    .map(|sheet| format!("{}({} 列 {} 行)", sheet.name, sheet.columns, sheet.rows))
                    .collect();
                format!(
                    "保持: 名前={} 出所={:?} 未保存={} シート=[{}]",
                    summary.name,
                    summary.origin,
                    summary.unsaved,
                    sheets.join(", "),
                )
            }
        }
    }

    /// 部分メニュー `ファイル` の項目（表示名 → 項目のノード）を取り出す。
    fn file_menu_items(registry: &MenuRegistry) -> Vec<crate::menu::MenuItemNode> {
        let model = registry.model();
        let file_submenu = model
            .top()
            .iter()
            .find(|submenu| submenu.label() == crate::menu::FILE_MENU_LABEL)
            .expect("ファイルの部分メニューがある");
        file_submenu
            .children()
            .iter()
            .filter_map(|child| match child {
                MenuNode::Item(item) => Some(item.clone()),
                MenuNode::Submenu(_) => None,
            })
            .collect()
    }

    /// **2 つの項目が登録口を通り、ファイルメニューへ競合しないショートカット付きで並ぶ**
    /// （要件 5.1、7.1）。
    ///
    /// 起動時に実際に並ぶ 4 件（`終了` / `開く…` / `新規` / `保存`）を同じ登録簿へ入れるので、
    /// **4 者の間でショートカットが競合しないこと**もここで固定される。「終了」と「開く…」を
    /// 同じ識別子・表示名・綴りで登録するのは、`menu.rs` / `dialog.rs` の定数が私有であり、
    /// 本テストが固定したいのが**起動時の組み合わせそのもの**だからである（`dialog.rs` の
    /// テストが「終了」を同じ形で組み立てているのと同じ判断）。
    #[test]
    fn the_two_items_are_registered_beside_open_and_quit_with_platform_resolved_shortcuts() {
        let registry = MenuRegistry::new();
        let noop = |_: &super::MenuSelection| {};
        registry
            .enroll(new_item_spec(noop))
            .expect("「新規」は未登録の組み合わせを使う");
        registry
            .enroll(save_item_spec(noop))
            .expect("「保存」は未登録の組み合わせを使う");

        // 起動時に先に登録される 2 件（7.4 の「終了」と 7.7 の「開く」）。
        let quit_path = MenuPath::new([crate::menu::FILE_MENU_LABEL]).expect("空でない");
        registry
            .enroll(
                crate::menu::MenuItemSpec::new("app-shell", "app-shell.quit", quit_path.clone(), "終了", noop)
                    .with_accelerator(if cfg!(target_os = "macos") {
                        "Cmd+Q"
                    } else {
                        "Ctrl+Q"
                    }),
            )
            .expect("終了との競合が無い");
        registry
            .enroll(
                crate::menu::MenuItemSpec::new(
                    "app-shell",
                    "app-shell.open-document",
                    quit_path,
                    "開く…",
                    noop,
                )
                .with_accelerator(if cfg!(target_os = "macos") {
                    "Cmd+O"
                } else {
                    "Ctrl+O"
                }),
            )
            .expect("開くとの競合が無い");

        assert_eq!(
            document_menu_path().segments(),
            &[crate::menu::FILE_MENU_LABEL.to_owned()],
            "2 つの項目は 7.4 のファイルメニューに置く"
        );

        let items = file_menu_items(&registry);
        assert_eq!(items.len(), 4, "終了・開く・新規・保存の 4 件である");

        for (item_id, label, spelling) in [
            (NEW_ITEM_ID, NEW_LABEL, NEW_ACCELERATOR_SPELLING),
            (SAVE_ITEM_ID, SAVE_LABEL, SAVE_ACCELERATOR_SPELLING),
        ] {
            let item = items
                .iter()
                .find(|item| item.item() == &MenuItemId::new(item_id))
                .unwrap_or_else(|| panic!("{item_id} がファイルメニューにない"));
            assert_eq!(item.label(), label);
            assert_eq!(
                item.accelerator(),
                Some(&Accelerator::parse(spelling).expect("解決済みの綴り")),
                "{label} のショートカットがプラットフォーム解決済みの綴りではない"
            );
            assert_eq!(item.owner().as_str(), OWNER, "登録元が本スペックの名前空間でない");
        }

        // **解決済みの綴りである**: 正準形が `ctrl+…` / `super+…` へ畳まれている
        // （`CmdOrCtrl` は 4.6 が受理しない）。
        let new_item = items
            .iter()
            .find(|item| item.item() == &MenuItemId::new(NEW_ITEM_ID))
            .expect("「新規」がある");
        let expected_key = if cfg!(target_os = "macos") { "super+" } else { "ctrl+" };
        assert!(
            new_item
                .accelerator()
                .expect("ショートカットがある")
                .as_str()
                .starts_with(expected_key),
            "「新規」のショートカットが実行中のプラットフォームの綴りでない: {:?}",
            new_item.accelerator()
        );
    }

    /// **ショートカットの競合は登録時に報告され、黙って捨てられない**（要件 3.4、task 3.6）。
    ///
    /// 別の登録元が同じ組み合わせを要求したときに、**どちらとどちらが衝突したか**が返り、
    /// **どちらの項目も失われない**（先の登録は残り、後の登録は現れない）。
    #[test]
    fn a_shortcut_conflict_is_reported_to_the_registrant() {
        let registry = MenuRegistry::new();
        let noop = |_: &super::MenuSelection| {};
        registry
            .enroll(save_item_spec(noop))
            .expect("「保存」は未登録の組み合わせを使う");

        // 別の登録元が同じ組み合わせ（別綴りでも正準形が同じ）を要求する。
        let conflict = registry
            .enroll(
                crate::menu::MenuItemSpec::new(
                    "macro-runtime",
                    "macro-runtime.save-macro",
                    MenuPath::new([crate::menu::FILE_MENU_LABEL]).expect("空でない"),
                    "マクロを保存",
                    noop,
                )
                .with_accelerator("control+KeyS"),
            )
            .expect_err("競合として返る");

        match &conflict {
            crate::menu::MenuRegistrationError::Accelerator(conflict) => {
                assert_eq!(conflict.existing.owner.as_str(), OWNER);
                assert_eq!(conflict.existing.item.as_str(), SAVE_ITEM_ID);
                assert_eq!(conflict.incoming.owner.as_str(), "macro-runtime");
                assert_eq!(conflict.incoming.item.as_str(), "macro-runtime.save-macro");
            }
            other => panic!("競合が期待される: {other}"),
        }

        // **拒否された項目はメニューに現れない**（現れてショートカットだけ失うことはない）。
        let labels: Vec<String> = file_menu_items(&registry)
            .iter()
            .map(|item| item.label().to_owned())
            .collect();
        assert_eq!(labels, [SAVE_LABEL]);
    }

    /// **プラットフォーム依存の綴り（`CmdOrCtrl`）は登録時に拒否される**（要件 3.4 / 4.6）。
    ///
    /// 基盤は解釈できない綴りをエラーにせず**その項目だけショートカットを失う**ので、
    /// ここで弾かれることが「メニューにショートカットが黙って消えない」ことの根拠である。
    #[test]
    fn an_unresolved_spelling_is_rejected_at_registration() {
        let registry = MenuRegistry::new();
        let noop = |_: &super::MenuSelection| {};
        let error = registry
            .enroll(
                crate::menu::MenuItemSpec::new(
                    "macro-runtime",
                    "macro-runtime.save",
                    MenuPath::new([crate::menu::FILE_MENU_LABEL]).expect("空でない"),
                    "保存",
                    noop,
                )
                .with_accelerator("CmdOrCtrl+S"),
            )
            .expect_err("受理されない");
        assert!(
            matches!(
                error,
                crate::menu::MenuRegistrationError::AcceleratorSyntax { .. }
            ),
            "構文の誤りとして返る: {error}"
        );
    }

    /// **「新規」の活性化はコマンドと同じ本体を通り、同じ結果と同じ通知を生む**（task 3.6 の受入）。
    ///
    /// 同じ開始状態の 2 つのセッション表を用意し、片方をメニュー項目の活性化の本体で、もう片方を
    /// 対応するコマンドの本体で駆動して、**結果・状態・通知の回数が一致する**ことを見る。
    /// `Refused`（未保存がある文書）でも `Created` でも一致することを確かめるので、
    /// **メニュー面が通知の規則（`should_notify`）を飛ばす実装に置き換われば本テストが落ちる**。
    #[test]
    fn the_new_item_takes_the_same_path_as_the_command() {
        let scratch = Scratch::new("new-parity");
        let path = scratch.file("台帳.jxcel");
        write_document(&path, "台帳", "一度だけ");
        let label = WindowLabel::new("doc-1");

        // 開始状態 1: 未保存の変更がある（新規作成は拒否される）。
        let menu_side = watch();
        let command_side = watch();
        for side in [&menu_side, &command_side] {
            side.resolve(&label, Some(&path)).expect("読み込める");
            mark_unsaved(side, &label);
        }

        let mut menu_notifications = 0u32;
        let menu_outcome = activate_new(&menu_side, &label, || menu_notifications += 1);
        let (command_outcome, command_changed) = commands::answer_new(&command_side, &label);

        assert!(
            matches!(menu_outcome, DocumentNewOutcome::Refused { .. }),
            "未保存がある文書への活性化は拒否される（要件 7.3）: {menu_outcome:?}"
        );
        assert_eq!(
            menu_outcome, command_outcome,
            "メニュー面とコマンド面の結果が食い違う"
        );
        assert_eq!(
            menu_notifications,
            u32::from(command_changed),
            "メニュー面の通知の回数がコマンド面と食い違う"
        );
        assert_eq!(0, menu_notifications, "拒否は何も変えないので通知しない");
        assert_eq!(
            comparable(&menu_side, &label),
            comparable(&command_side, &label),
            "拒否のあとの状態が食い違う"
        );

        // 開始状態 2: 未保存の変更が無い（新規作成が通り、状態が変わる）。
        let menu_side = watch();
        let command_side = watch();
        for side in [&menu_side, &command_side] {
            side.create(&label).expect("文書を用意できる");
        }

        let mut menu_notifications = 0u32;
        let menu_outcome = activate_new(&menu_side, &label, || menu_notifications += 1);
        let (command_outcome, command_changed) = commands::answer_new(&command_side, &label);

        assert_eq!(DocumentNewOutcome::Created, menu_outcome);
        assert_eq!(
            menu_outcome, command_outcome,
            "メニュー面とコマンド面の結果が食い違う"
        );
        assert!(command_changed, "新規作成は状態を変える");
        assert_eq!(
            1, menu_notifications,
            "状態が変わったのに通知がちょうど 1 回でない"
        );
        assert_eq!(
            comparable(&menu_side, &label),
            comparable(&command_side, &label),
            "新規作成のあとの状態が食い違う"
        );
        let summary = comparable(&menu_side, &label);
        assert!(
            summary.contains("出所=New") && summary.contains("未保存=false"),
            "用意した文書が出所を持たず未保存でない状態になっていない: {summary}"
        );
    }

    /// **「保存」の活性化はコマンドと同じ本体を通る**（task 3.6 の受入）。
    ///
    /// 出所を持たない文書の保存について、**提示へ渡る提案名・結果・状態・通知の回数**が、
    /// メニュー面とコマンド面で一致することを見る。とくに:
    ///
    /// - 取り消し（要件 5.3）: どちらも `Cancelled` で、**書き出しは起きず、通知もしない**
    /// - 選ばれた位置への保存（要件 5.2、5.8）: どちらも `Saved`、ファイル名が同じで、通知は 1 回
    ///
    /// 提案名を観測するのは `NeedsLocation` の腕を通ったことの証拠であり、
    /// [`dialog::DEFAULT_SAVE_NAME`] 以外が渡れば落ちる。
    ///
    /// **書き出したバイト列の比較は、開始の内容が同一のときにだけ行う**（別のテスト）。新規作成の
    /// 文書は**シートの識別子が ULID であり生成ごとに異なる**ので、2 つのセッションが作った文書の
    /// バイト列は比較できない（比べられるのは利用者に見える要素だけである）。
    #[test]
    fn the_save_item_takes_the_same_path_as_the_command() {
        let menu_scratch = Scratch::new("save-parity-menu");
        let command_scratch = Scratch::new("save-parity-command");
        let label = WindowLabel::new("empty-1");

        let menu_side = watch();
        let command_side = watch();
        for side in [&menu_side, &command_side] {
            side.create(&label).expect("文書を用意できる");
            mark_unsaved(side, &label);
        }

        // 取り消し: 何も書き出さず、未保存を保つ（要件 5.3）。
        let mut menu_suggested = Vec::new();
        let mut menu_notifications = 0u32;
        let menu_outcome = activate_save(
            &menu_side,
            &label,
            |suggested| {
                menu_suggested.push(suggested.to_owned());
                SaveLocation::Cancelled
            },
            || menu_notifications += 1,
        );
        let mut command_suggested = Vec::new();
        let (command_outcome, command_changed) =
            commands::answer_save(&command_side, &label, |suggested| {
                command_suggested.push(suggested.to_owned());
                SaveLocation::Cancelled
            });

        assert_eq!(DocumentSaveOutcome::Cancelled, menu_outcome);
        assert_eq!(menu_outcome, command_outcome, "取り消しの結果が食い違う");
        assert_eq!(
            vec![dialog::DEFAULT_SAVE_NAME.to_owned()],
            menu_suggested,
            "出所を持たない文書には既定の提案名を渡す（要件 5.2）"
        );
        assert_eq!(menu_suggested, command_suggested, "提案名が食い違う");
        assert_eq!(u32::from(command_changed), menu_notifications);
        assert_eq!(0, menu_notifications, "取り消しは通知しない");
        assert_eq!(
            comparable(&menu_side, &label),
            comparable(&command_side, &label),
            "取り消しのあとの状態が食い違う"
        );
        assert!(
            menu_scratch.file("無題.jxcel").exists() == false,
            "取り消しで書き出しが起きた"
        );

        // 選ばれた位置への保存: 出所が確定し、未保存が落ち、通知が 1 回（要件 5.2、5.8）。
        let menu_path = menu_scratch.file("保存.jxcel");
        let command_path = command_scratch.file("保存.jxcel");
        let mut menu_notifications = 0u32;
        let menu_outcome = activate_save(
            &menu_side,
            &label,
            |_suggested| SaveLocation::Chosen(menu_path.clone()),
            || menu_notifications += 1,
        );
        let (command_outcome, command_changed) =
            commands::answer_save(&command_side, &label, |_suggested| {
                SaveLocation::Chosen(command_path.clone())
            });

        assert_eq!(DocumentSaveOutcome::Saved, menu_outcome);
        assert_eq!(menu_outcome, command_outcome, "保存の結果が食い違う");
        assert!(command_changed, "保存の成功は状態を変える");
        assert_eq!(
            1, menu_notifications,
            "状態が変わったのに通知がちょうど 1 回でない"
        );
        assert_eq!(
            comparable(&menu_side, &label),
            comparable(&command_side, &label),
            "保存のあとの状態（ファイル名と未保存）が食い違う"
        );
        let summary = comparable(&menu_side, &label);
        assert!(
            summary.contains("保存.jxcel") && summary.contains("未保存=false"),
            "保存のあとに未保存が落ち、出所が確定していない: {summary}"
        );
        assert!(menu_path.exists() && command_path.exists(), "書き出されていない");

        // **同じ文書の 2 度目の保存は同じバイト列である**（要件 5.7 の決定性。出所が確定した
        // あとの保存は提示を経ない）。メニュー面が最初に書いた内容と、コマンド面が同じセッション
        // で書いた内容を比べる。
        let first = std::fs::read(&menu_path).expect("メニュー面が書き出した");
        let (again, _) = commands::answer_save(&menu_side, &label, |_suggested| {
            panic!("出所が確定しているので保存先を尋ねてはならない")
        });
        assert_eq!(DocumentSaveOutcome::Saved, again);
        assert_eq!(
            first,
            std::fs::read(&menu_path).expect("2 度目を書き出した"),
            "同じ文書の再保存でバイト列が変わった（決定性が壊れている）"
        );
    }

    /// **開始の内容が同一なら、どちらの入口でも同じバイト列が書き出される**（task 3.6 の受入。
    /// 要件 5.7 の決定性の契約を 2 つの入口で見る）。
    ///
    /// 2 つのセッションが**同じファイルから**読み込むので、文書の内容（シートの識別子を含む）は
    /// 一致する。どちらも未保存の印を立ててから出所へ保存するので、**同じ位置へ同じ内容が
    /// 2 回書かれる** — バイト列が一致しなければ、片方の入口だけが別の内容を書いている
    /// （形式の側の呼び方を変える、別の文書を保存する等）。**メニュー面の保存が文書を書き換えて
    /// いないこと**も同時に固定される。
    #[test]
    fn both_entry_points_write_the_same_bytes_for_the_same_content() {
        let scratch = Scratch::new("byte-parity");
        let shared = scratch.file("共有.jxcel");
        write_document(&shared, "共有", "同じ内容");
        let label = WindowLabel::new("doc-1");

        let menu_side = watch();
        let command_side = watch();
        for side in [&menu_side, &command_side] {
            side.resolve(&label, Some(&shared)).expect("読み込める");
            mark_unsaved(side, &label);
        }

        // メニュー面の保存（出所があるので提示を経ない。要件 5.1）。
        let mut menu_notifications = 0u32;
        let menu_outcome = activate_save(
            &menu_side,
            &label,
            |_suggested| panic!("出所があるので保存先を尋ねてはならない"),
            || menu_notifications += 1,
        );
        assert_eq!(DocumentSaveOutcome::Saved, menu_outcome);
        assert_eq!(1, menu_notifications, "保存の成功は通知する");
        let after_menu = std::fs::read(&shared).expect("メニュー面が書き出した");

        // コマンド面の保存（同じ位置へ同じ内容が書かれる）。
        let (command_outcome, command_changed) = commands::answer_save(&command_side, &label, |_s| {
            panic!("出所があるので保存先を尋ねてはならない")
        });
        assert_eq!(DocumentSaveOutcome::Saved, command_outcome);
        assert_eq!(menu_outcome, command_outcome, "結果が食い違う");
        assert!(command_changed, "保存の成功は状態を変える");
        let after_command = std::fs::read(&shared).expect("コマンド面が書き出した");

        assert_eq!(
            after_menu, after_command,
            "同じ内容でも入口によって書き出したバイト列が違う（決定性が入口に依存している）"
        );
        // **保存のあとは未保存が落ちる**（どちらの入口でも同じ）。
        assert_eq!(
            comparable(&menu_side, &label),
            comparable(&command_side, &label),
        );
    }

    /// **未保存でない文書の保存は通知しない**（`should_notify` が唯一の規則であること）。
    ///
    /// 「保存が成功したら必ず送る」という決め打ちの実装に置き換われば、本テストが落ちる
    /// （状態は変わっていないのにイベントが飛ぶ）。メニュー面とコマンド面の双方で確かめる。
    #[test]
    fn saving_an_unchanged_document_notifies_nobody() {
        let scratch = Scratch::new("save-unchanged");
        let path = scratch.file("変更なし.jxcel");
        write_document(&path, "変更なし", "そのまま");
        let label = WindowLabel::new("doc-1");

        let menu_side = watch();
        let command_side = watch();
        for side in [&menu_side, &command_side] {
            side.resolve(&label, Some(&path)).expect("読み込める");
        }

        let mut menu_notifications = 0u32;
        let menu_outcome = activate_save(&menu_side, &label, |_suggested| {
            panic!("出所があるので保存先を尋ねてはならない")
        }, || menu_notifications += 1);
        let (command_outcome, command_changed) = commands::answer_save(
            &command_side,
            &label,
            |_suggested| panic!("出所があるので保存先を尋ねてはならない"),
        );

        assert_eq!(DocumentSaveOutcome::Saved, menu_outcome, "出所へ書き出す");
        assert_eq!(menu_outcome, command_outcome);
        assert!(!command_changed, "未保存でない文書の保存は状態を変えない");
        assert_eq!(0, menu_notifications, "変わっていないのに通知した");
        assert_eq!(
            comparable(&menu_side, &label),
            comparable(&command_side, &label)
        );
        let summary = comparable(&menu_side, &label);
        assert!(
            summary.contains("未保存=false") && summary.contains("出所=File"),
            "保存のあと未保存のままか出所が失われている: {summary}"
        );
    }

    /// 保持していないウィンドウへの活性化は**失敗として答える**（設計の表「保持していない窓への
    /// 操作 → 状態 `Absent`（保存は `Failed`）」）。
    ///
    /// メニュー面もコマンド面と同じ答えを返し、提案を求めない（保存先を尋ねる腕へ入らない）。
    #[test]
    fn activating_on_a_window_without_a_document_answers_the_same_failure() {
        let label = WindowLabel::new("empty-1");
        let menu_side = watch();
        let command_side = watch();

        let menu_outcome = activate_save(
            &menu_side,
            &label,
            |_suggested| panic!("保持していないので保存先を尋ねてはならない"),
            || panic!("保持していないウィンドウへの操作は通知しない"),
        );
        let (command_outcome, command_changed) = commands::answer_save(
            &command_side,
            &label,
            |_suggested| panic!("保持していないので保存先を尋ねてはならない"),
        );

        assert!(
            matches!(menu_outcome, DocumentSaveOutcome::Failed { .. }),
            "保持していないウィンドウへの保存は失敗として答える: {menu_outcome:?}"
        );
        assert_eq!(menu_outcome, command_outcome);
        assert!(!command_changed);
        assert_eq!(
            comparable(&menu_side, &label),
            comparable(&command_side, &label)
        );
    }

    /// 記録に出す語が、コマンド面と同じ関数から来ていること（**写像を 2 つ持たない**）。
    ///
    /// 実画面の観測はこの行を突き合わせるので、語が分かれれば観測が成立しなくなる。
    #[test]
    fn the_record_words_come_from_the_same_mapping_as_the_commands() {
        assert_eq!("用意した", commands::describe_new(&DocumentNewOutcome::Created));
        assert_eq!(
            "保存した",
            commands::describe_save(&DocumentSaveOutcome::Saved)
        );
        let label = WindowLabel::new("empty-1");
        let watch = watch();
        assert_eq!(
            "保持していない",
            commands::describe_status(&commands::boundary_status(&watch, &label))
        );
        watch.create(&label).expect("文書を用意できる");
        assert_eq!(
            "保持している",
            commands::describe_status(&commands::boundary_status(&watch, &label))
        );
        // 位置は境界の写像で落ちる（ここが崩れると記録にパスが混ざる）。
        let summary = comparable(&watch, &label);
        assert!(
            !summary.contains(std::env::temp_dir().to_string_lossy().as_ref()),
            "境界の写しに位置が混ざっている: {summary}"
        );
        assert!(summary.contains("出所=New"));
    }
}

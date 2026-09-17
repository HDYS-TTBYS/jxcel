//! 診断の利用者向け導線（タスク 9.5。要件 8.1、8.6、8.7）。
//!
//! 所有: `CommandSurface` の診断部分と、`MenuSurface` の登録口へ差し込む 3 つの項目
//! （design.md「Components and Interfaces → Adapter Layer」）。**実体（記録の保存場所の解決・
//! 保持方針・書き出し・詳細度の列挙）は Tauri 非依存の中核 [`app_shell::diagnostics`] にあり、
//! 本モジュールは利用者へ届けるための薄いアダプタである。**
//!
//! # このタスクが作る 3 つの導線
//!
//! | 導線 | メニュー項目 | コマンド | 画面での提示 |
//! |---|---|---|---|
//! | 記録の保存場所の確認（要件 8.1） | `診断 > 記録の保存場所を表示` | [`diagnostics_log_location`] | 解決したディレクトリを文字列として表示する |
//! | 記録の書き出し（要件 8.6） | `診断 > 診断情報を書き出す…` | [`diagnostics_export`] | 書き出したファイルの位置と、記録の有無を表示する |
//! | 記録の詳細度の変更（要件 8.7） | `診断 > 記録の詳細度…` | [`diagnostics_verbosity_get`] / [`diagnostics_verbosity_set`] | 現在値と選択肢（`Off`〜`Trace`）を表示する |
//!
//! 3 つとも **7.4 の登録口（[`MenuRegistry::register`]）** へ登録したメニュー項目から到達でき、
//! 選択されると活性化の対象ウィンドウ（7.5 の振り向け）へ
//! [`DIAGNOSTICS_REQUESTED_EVENT`] を送る。フロントエンドの診断画面
//! （`src/features/diagnostics/`）がその通知を受けて遷移し、該当の区画を提示する。
//!
//! **導線をコマンド名で通す理由（tasks.md 4.5 の申し送り）**: 4.5 は「5.2 が
//! [`app_shell::diagnostics::DiagnosticsLevel`] を `log::LevelFilter` へ 1 対 1 で写像し、
//! **9.5 が既存の `diagnostics_*` コマンド名で利用者に出す**」と定めている。したがって本モジュール
//! は中核の関数を直接呼ぶのではなく、**4 つの既存コマンド名
//! （`crates/app-shell/src/ipc/command_names.rs`）をハンドラとして結線する**。フロントエンドは
//! 生成された名前定数（`src/ipc/bindings.ts`）だけを参照してこれを呼ぶ（tasks.md 2.2 / 2.4）。
//!
//! # 保存場所の提示にネイティブのメッセージ表示を使わない理由
//!
//! 利用者へ「どこに記録があるか」を伝える手段として、本タスクは**画面（9.1 の領域の中）**を
//! 選び、OS 標準のメッセージ表示は使わない。理由は 3 つある:
//!
//! 1. **7.7 が作ったのはファイル選択器だけである**（`src-tauri/src/dialog.rs`）。メッセージ
//!    表示を足すには GTK / NSAlert / Win32 の 3 系統を新たに書くことになり、本タスクの境界
//!    （診断の導線）を越える。1.3 の依存方針（`tauri-plugin-dialog` を採らない）も同じ結論を
//!    支持する — 採用すれば `tauri-plugin-fs` が依存木へ入る。
//! 2. **3 OS で同じ見え方になる。** 画面はアプリのウィンドウの中に描かれるので、GTK / WKWebView /
//!    WebView2 のどれでも同じ内容が同じ場所に出る（10.4 の描画確認と同じ面である）。
//! 3. **場所を開かない。** ファイルマネージャの起動やフォルダを開く操作は、**アプリから
//!    プロセスを起動しない**という要件 4.7 の境界に触れる。提示するのは文字列だけであり、
//!    場所の選択もコピーも利用者の操作（テキスト選択）に委ねる。
//!
//! 位置は `data-testid="jxcel-diagnostics-location-value"` の要素に**選択可能な文字列**として
//! 出るので、利用者はそのまま読めるし、必要なら自分でコピーできる。
//!
//! # 書き出しの宛先（要件 8.6）
//!
//! **宛先をフロントエンドから受け取らない。** 受け取れば「フロントエンドが任意のパスへ書ける」
//! 経路になり、要件 4.7 の境界（フロントエンドに任意のファイル読み書きを提供しない）を破る。
//! したがって宛先は Rust 側で決める:
//!
//! 1. OS の**ダウンロード領域**（[`tauri::path::PathResolver::download_dir`]）が実在すればそこ。
//! 2. 無ければ**ホーム領域**（[`tauri::path::PathResolver::home_dir`]）。
//! 3. どちらも実在しなければ失敗として報告する（親ディレクトリを作らないのは 4.5 の宛先契約）。
//!
//! ファイル名は `jxcel-diagnostics-<UNIX 秒>.log` であり、**書き出しのたびに新しい名前**になる。
//! 同じ秒に 2 回要求すると 4.5 の契約どおり既存のファイルを置き換える（成功は成功である）。
//! 書き出しそのものは `spawn_blocking` で走らせる — 記録の合計は最大 48 MB
//! （[`app_shell::diagnostics::MAX_RETAINED_LOG_BYTES`]）であり、イベントループを止めない。
//!
//! # 詳細度の変更がその場で効く理由（要件 8.7。tasks.md 5.2 の 1 対 1 対応）
//!
//! 5.2 は起動時に [`app_shell::diagnostics::DiagnosticsLevel`] を `log::LevelFilter` へ
//! 1 対 1 で写像する（[`crate::lifecycle::to_level_filter`]）。実行中の変更も**同じ写像を
//! 通す**ので、起動時の適用と実行中の適用が食い違う余地が無い。
//!
//! **効く先は全体の上限（`log::set_max_level`）1 つである。** 採用する記録機構
//! （`tauri-plugin-log` 2.9.1）は `fern` のディスパッチを持ち、その水準は `Builder::level` が
//! 構築時に固定する（`src/lib.rs` の `Builder::level` → `Dispatch::level`。`acquire_logger` が
//! `dispatch.into_log()` の上限を `attach_logger` へ渡し、`attach_logger` が
//! `log::set_boxed_logger` と `log::set_max_level` を並べて呼ぶ）。`log::set_max_level` は
//! **全体の上限を下げることしかできない**ので、ディスパッチの水準を設定値にすると詳細度を
//! **下げる**変更だけが効き、**上げる**変更は効かない（round-1 のレビューで実測され棄却された）。
//!
//! したがって 5.2 はディスパッチを**アプリが要求しうる最大**（`crate::lifecycle` の
//! `dispatch_ceiling`）に固定し、ここは**全体の上限だけを書き換える**（下の
//! [`diagnostics_verbosity_set`]）。これにより上げる方向も下げる方向も
//! その場で効く（記録機構側のフィルタは最大なので、全体の上限が唯一の実効フィルタである）。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use app_shell::diagnostics::{self, DiagnosticsLevel as CoreLevel};
use app_shell::ipc::{
    DiagnosticsExportRecords, DiagnosticsExportResponse, DiagnosticsLevel,
    DiagnosticsLogLocationResponse, DiagnosticsRequestedEvent, DiagnosticsSection,
    DiagnosticsVerbosityResponse, DiagnosticsVerbositySetRequest, IpcError, IpcResult,
    ObservationItem, ObservationItemOutcome, ObservationUndo, RenderHealthRecordRequest,
    RenderHealthRecordResponse, RenderHealthReport, RenderPaintFailure, WindowContext, WindowLabel,
    DIAGNOSTICS_REQUESTED_EVENT,
};
use app_shell::settings::FileSettingsStore;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_log::log;

use crate::lifecycle::to_level_filter;
use crate::menu::{MenuItemSpec, MenuPath, MenuRegistry, MenuSelection};

/// 登録元の識別子（7.4 の組み込み項目と同じ名前空間を使う）。
const OWNER: &str = "app-shell";

/// 3 つの導線を置く部分メニューの名前。
///
/// **トップレベルの部分メニュー**である（7.4 の `MenuPath` は非空を要求するので単独項目には
/// できない）。並びは [`crate::menu`] の既知の並び（ファイル / 編集 / 表示 / ヘルプ）の後ろに
/// 辞書順で置かれる（macOS ではアプリケーションメニューが常に先頭）。
const DIAGNOSTICS_MENU_LABEL: &str = "診断";

/// 記録の保存場所を表示する項目の識別子。**アプリ全体で一意でなければならない。**
const LOG_LOCATION_ITEM_ID: &str = "app-shell.diagnostics-log-location";

/// 診断情報を書き出す項目の識別子。
const EXPORT_ITEM_ID: &str = "app-shell.diagnostics-export";

/// 記録の詳細度を変える項目の識別子。
const VERBOSITY_ITEM_ID: &str = "app-shell.diagnostics-verbosity";

/// 記録の保存場所を表示する項目の表示名。
const LOG_LOCATION_LABEL: &str = "記録の保存場所を表示";

/// 診断情報を書き出す項目の表示名。
const EXPORT_LABEL: &str = "診断情報を書き出す…";

/// 記録の詳細度を変える項目の表示名。
const VERBOSITY_LABEL: &str = "記録の詳細度…";

/// 保存場所の項目のショートカット（非 macOS。プラットフォーム解決済みの綴り。4.6 の契約）。
#[cfg(not(target_os = "macos"))]
const LOG_LOCATION_ACCELERATOR: &str = "Ctrl+Shift+L";

/// 保存場所の項目のショートカット（macOS。メニュー上は `⌘⇧L` と描かれる）。
#[cfg(target_os = "macos")]
const LOG_LOCATION_ACCELERATOR: &str = "Cmd+Shift+L";

/// 書き出しの項目のショートカット（非 macOS）。
#[cfg(not(target_os = "macos"))]
const EXPORT_ACCELERATOR: &str = "Ctrl+Shift+E";

/// 書き出しの項目のショートカット（macOS）。
#[cfg(target_os = "macos")]
const EXPORT_ACCELERATOR: &str = "Cmd+Shift+E";

/// 詳細度の項目のショートカット（非 macOS）。
#[cfg(not(target_os = "macos"))]
const VERBOSITY_ACCELERATOR: &str = "Ctrl+Shift+V";

/// 詳細度の項目のショートカット（macOS）。
#[cfg(target_os = "macos")]
const VERBOSITY_ACCELERATOR: &str = "Cmd+Shift+V";

/// 書き出すファイルの名前の接頭辞。
const EXPORT_FILE_PREFIX: &str = "jxcel-diagnostics-";

/// 診断の導線がメニューから要求されたことを記録に残すときの説明（導線ごとの 1 語）。
fn section_name(section: DiagnosticsSection) -> &'static str {
    match section {
        DiagnosticsSection::Location => "記録の保存場所",
        DiagnosticsSection::Export => "診断情報の書き出し",
        DiagnosticsSection::Verbosity => "記録の詳細度",
    }
}

/// Tauri が注入した呼び出し元ウィンドウを、境界の文脈（要件 4.6）へ写す。
///
/// **境界の型を新設しない。** `app_shell::ipc::WindowContext` / `WindowLabel` をそのまま使う
/// （2.1 の申し送り。3 つ目の識別子を作らない）。
fn caller_context(window: &WebviewWindow) -> WindowContext {
    WindowContext {
        window: WindowLabel::new(window.label()),
    }
}

// ---------------------------------------------------------------------------
// 記録の保存場所（要件 8.1）
// ---------------------------------------------------------------------------

/// 記録の保存場所を返す（要件 8.1。タスク 9.5 の導線）。
///
/// 応答は共通の封筒 [`IpcResult`] であり、**呼び出し元ウィンドウの文脈を必ず含む**
/// （要件 4.6。呼び出し元は Tauri が注入する [`WebviewWindow`] から取るので偽装できない）。
/// 解決は 4.4 の [`app_shell::diagnostics::log_dir`] をそのまま使う（**同じ解決を 2 つ持たない**）
/// ので、5.2 が記録機構へ渡した保存先とこの導線が示す保存先は常に一致する。
///
/// 環境変数が無い等で解決できない場合は封筒の失敗腕
/// （[`IpcError::Diagnostics`]）になり、**`Err` を返して `invoke` に拒否させない**（7.1 と同じ方針）。
/// 提示するのは文字列だけであり、場所を開いたり走査したりしない（要件 4.7）。
#[tauri::command]
pub fn diagnostics_log_location(
    window: WebviewWindow,
) -> IpcResult<DiagnosticsLogLocationResponse, IpcError> {
    let command = app_shell::ipc::command_names::DIAGNOSTICS_LOG_LOCATION;
    let context = caller_context(&window);
    match diagnostics::log_dir() {
        Ok(directory) => {
            // 記録に残すのは提示した場所だけである（ドキュメントの内容はこの経路に存在しない）。
            log::info!(
                "{command}: ウィンドウ = {} に記録の保存場所を提示した: {}",
                context.window.as_str(),
                directory.display(),
            );
            IpcResult::Ok {
                data: DiagnosticsLogLocationResponse {
                    context,
                    directory: directory.to_string_lossy().into_owned(),
                },
            }
        }
        Err(error) => {
            log::error!(
                "{command}: 記録の保存場所を解決できなかった（ウィンドウ = {}）: {error}",
                context.window.as_str(),
            );
            IpcResult::Err {
                error: IpcError::Diagnostics {
                    message: error.to_string(),
                },
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 記録の書き出し（要件 8.6）
// ---------------------------------------------------------------------------

/// 書き出すファイルの名前を決める（**純粋関数**。時刻を引数で受ける）。
///
/// `jxcel-diagnostics-<UNIX 秒>.log` である。**書き出しのたびに新しい名前**になるので、利用者は
/// 直前の書き出しと見分けられる。同じ秒に 2 回要求した場合は 4.5 の契約どおり既存のファイルを
/// 置き換える（原子的な置き換えなので、途中の状態が残ることはない）。
fn export_file_name(now: SystemTime) -> String {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    format!("{EXPORT_FILE_PREFIX}{seconds}.log")
}

/// 書き出しに含めた記録の有無を境界の形へ写す（**純粋関数**。4.5 の `files_merged`）。
///
/// 0 件は失敗ではなく「記録が無かった」という結果であり、その場合も 1 つのファイルができて
/// いる（4.5 の契約）。境界へ数値を出さない規則に従い、件数ではなく閉じた列挙で運ぶ。
fn records_of(files_merged: usize) -> DiagnosticsExportRecords {
    if files_merged == 0 {
        DiagnosticsExportRecords::Empty
    } else {
        DiagnosticsExportRecords::Merged
    }
}

/// 書き出しの宛先を決める（要件 8.6。module doc「書き出しの宛先」）。
///
/// **実在するディレクトリだけを選ぶ**（親ディレクトリを作らないのは 4.5 の宛先契約）。
/// ダウンロード領域 → ホーム領域の順に試し、どちらも無ければ失敗として報告する。
/// **フロントエンドから受け取ったパスは使わない**（要件 4.7）。
fn export_destination(app: &AppHandle) -> Result<PathBuf, IpcError> {
    let resolver = app.path();
    let directory = [resolver.download_dir(), resolver.home_dir()]
        .into_iter()
        .filter_map(Result::ok)
        .find(|candidate| candidate.is_dir())
        .ok_or_else(|| IpcError::Diagnostics {
            message: "書き出し先のディレクトリを決められなかった（ダウンロード領域もホーム領域も実在しない）"
                .to_owned(),
        })?;
    Ok(directory.join(export_file_name(SystemTime::now())))
}

/// 記録を 1 つのファイルへ書き出す（要件 8.6。タスク 9.5 の導線）。
///
/// 実体は 4.5 の [`app_shell::diagnostics::export`] であり、**この経路が書き出しの唯一の
/// 実装である**（2 つ目を作らない）。書き出しは記録ディレクトリの `*.log` を更新時刻の昇順で
/// 連結し、宛先へ原子的に置き換える（4.5 の契約）。
///
/// `async` にしてあるのは、`spawn_blocking` した処理を待つためである（同期コマンドは
/// イベントループのスレッドで走るので、最大 48 MB の読み書きで画面を止めない。7.7 と同じ理由）。
/// 記録が 1 つも無い（ディレクトリが無い・`*.log` が無い）場合も**成功**であり、宛先には
/// 見出しだけのファイルが 1 つできる（応答の [`DiagnosticsExportRecords`] が `Empty` になる）。
#[tauri::command]
pub async fn diagnostics_export(
    app: AppHandle,
    window: WebviewWindow,
) -> IpcResult<DiagnosticsExportResponse, IpcError> {
    let command = app_shell::ipc::command_names::DIAGNOSTICS_EXPORT;
    let context = caller_context(&window);
    let destination = match export_destination(&app) {
        Ok(destination) => destination,
        Err(error) => {
            log::error!(
                "{command}: 書き出し先を決められなかった（ウィンドウ = {}）",
                context.window.as_str(),
            );
            return IpcResult::Err { error };
        }
    };

    // 実体（4.5）はローカルのファイル操作だけを行う。イベントループを止めないよう
    // ブロッキング用のスレッドへ渡し、結果を待つ。
    let target = destination.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || diagnostics::export(&target)).await;

    let report = match outcome {
        Ok(Ok(report)) => report,
        Ok(Err(error)) => {
            log::error!(
                "{command}: 書き出しに失敗した（ウィンドウ = {} / 宛先 = {}）: {error}",
                context.window.as_str(),
                destination.display(),
            );
            return IpcResult::Err {
                error: IpcError::Diagnostics {
                    message: format!("書き出しに失敗した（{}）: {error}", destination.display()),
                },
            };
        }
        Err(join_error) => {
            log::error!(
                "{command}: 書き出しの処理が異常終了した（ウィンドウ = {}）: {join_error}",
                context.window.as_str(),
            );
            return IpcResult::Err {
                error: IpcError::Diagnostics {
                    message: format!("書き出しの処理が異常終了した: {join_error}"),
                },
            };
        }
    };

    let records = records_of(report.files_merged);
    log::info!(
        "{command}: ウィンドウ = {} の要求で診断情報を 1 つのファイルへ書き出した: {}（記録 {} 件 / {} バイト）",
        context.window.as_str(),
        report.destination.display(),
        report.files_merged,
        report.bytes_written,
    );
    IpcResult::Ok {
        data: DiagnosticsExportResponse {
            context,
            destination: report.destination.to_string_lossy().into_owned(),
            records,
        },
    }
}

// ---------------------------------------------------------------------------
// 記録の詳細度（要件 8.7）
// ---------------------------------------------------------------------------

/// 現在の詳細度と、選べる値の全体を並べた応答を組み立てる（**純粋関数**）。
///
/// `levels` は [`DiagnosticsLevel::ALL`]（`Off` から `Trace` へ昇順）である。**画面は選択肢の
/// 集合と順序を自前で持たない**ので、中核の閉じた列挙と食い違う余地が無い（4.5 の契約）。
fn verbosity_response(context: WindowContext, level: CoreLevel) -> DiagnosticsVerbosityResponse {
    DiagnosticsVerbosityResponse {
        context,
        level: level.into(),
        levels: DiagnosticsLevel::ALL.to_vec(),
    }
}

/// 記録の詳細度を読み取る（要件 8.7。タスク 9.5 の導線）。
///
/// 4.5 の [`CoreLevel::from_store`] をそのまま使う。**壊れた保存値は既定（`Info`）へ落ちるが
/// 消去しない**という 4.5 の意味論をここで作り直さない（設定ストアの読み取りは値を書き戻さない）。
/// したがって「解釈できない値」は失敗ではなく、既定を提示することになる。
#[tauri::command]
pub fn diagnostics_verbosity_get(
    settings: State<'_, Arc<FileSettingsStore>>,
    window: WebviewWindow,
) -> IpcResult<DiagnosticsVerbosityResponse, IpcError> {
    let command = app_shell::ipc::command_names::DIAGNOSTICS_VERBOSITY_GET;
    let context = caller_context(&window);
    let level = CoreLevel::from_store(&**settings);
    log::info!(
        "{command}: ウィンドウ = {} に現在の詳細度を提示した: {level:?}",
        context.window.as_str(),
    );
    IpcResult::Ok {
        data: verbosity_response(context, level),
    }
}

/// 記録の詳細度を変更する（要件 8.7。タスク 9.5 の導線）。
///
/// 1. **設定へ保存する**（4.5 の [`CoreLevel::write_to`]。設定ストア経由なので、他の
///    ウィンドウへは既存の設定変更の通知（`settings_changed`）が届く。要件 7.4）。
/// 2. **実行中の記録機構へ適用する**（`log::set_max_level`）。写像は 5.2 が起動時に使う
///    [`to_level_filter`] と同じものである（**1 対 1 の対応を 2 つ持たない**。tasks.md 5.2）。
///    記録機構のディスパッチは 5.2 が最大（`crate::lifecycle` の `dispatch_ceiling`）に固定して
///    いるので、**全体の上限が唯一の実効フィルタ**であり、ここで上げる方向へ書き換えても
///    記録機構が先に落とすことはない（module doc「詳細度の変更がその場で効く理由」）。
///
/// 保存に失敗した場合は封筒の失敗腕（[`IpcError::Settings`]）になり、設定は変更前のままである
/// （`set` の契約）。**その場合は記録の水準も変えない**（保存できなかった変更をその場だけ
/// 適用すると、次回起動との食い違いになる）。
#[tauri::command]
pub fn diagnostics_verbosity_set(
    settings: State<'_, Arc<FileSettingsStore>>,
    window: WebviewWindow,
    request: DiagnosticsVerbositySetRequest,
) -> IpcResult<DiagnosticsVerbosityResponse, IpcError> {
    let command = app_shell::ipc::command_names::DIAGNOSTICS_VERBOSITY_SET;
    let context = caller_context(&window);
    let level: CoreLevel = request.level.into();
    if let Err(error) = level.write_to(&**settings) {
        log::error!(
            "{command}: 詳細度を保存できなかった（ウィンドウ = {}）: {error}",
            context.window.as_str(),
        );
        return IpcResult::Err {
            error: IpcError::Settings {
                message: error.to_string(),
            },
        };
    }
    // 5.2 と同じ写像で、以後の記録の水準を変える。`Off` を選ぶとここより後の記録は
    // すべて落ちる（記録中のファイルは作られたまま残る）。
    log::set_max_level(to_level_filter(level));
    // 変更の事実を残す。**この行は新しい水準で絞られる**ので、`Off` / `Error` / `Warn` を
    // 選ぶと残らない（「変更した」ことの証拠は設定ファイル側にある）。仕様どおりの挙動である。
    log::info!(
        "{command}: ウィンドウ = {} の要求で詳細度を変更した: {level:?}",
        context.window.as_str(),
    );
    IpcResult::Ok {
        data: verbosity_response(context, level),
    }
}

// ---------------------------------------------------------------------------
// 描画の健全性の記録（要件 12.2、12.3。タスク 9.3）
// ---------------------------------------------------------------------------

/// 描画の健全性を診断の記録へ 1 件残す（要件 12.2、12.3。タスク 9.3）。
///
/// **これが 12.3 の「診断情報に記録する」の実体である。**グリッドの画面は描画の劣化を
/// **自分では記録できない** — フロントエンドから記録機構へ書く経路が無いためであり
/// （`tauri-plugin-log` の宛先は `Stdout` と `Folder` だけで、`Webview` ターゲットは 5.2 が
/// 意図的に有効にしていない。`research.md` の実測）、画面はこの 1 本のコマンドを通してだけ
/// 記録を残せる。
///
/// **記録の 1 行は本層が組み立てる。**要求が運ぶのは札と数値だけであり（[`RenderHealthReport`]）、
/// 任意の文字列は境界を越えられない — 記録の注入面を広げないためである。ミリ秒への戻し
/// （マイクロ秒の整数 ÷ 1000）もここで行う。
///
/// **1 行は「測った事実」だけを述べ、結論（要件を満たしたか）を名乗らない。**観測の腕
/// （[`RenderHealthReport::Observation`]。tasks.md 9.2）も同じであり、**要件の合否を判定するのは
/// 検査器の側である**（3 OS の検査器が同じ形で読めるのはここだけである — macOS / Windows には
/// AT-SPI が無い）。記録の有無を決める
/// 閾値は画面側にあり（要件値 + 計測の刻みの許容。`src/features/grid/renderHealth.ts`）、
/// **要件 11.1 の合否を判定するのは 9.2 の実画面の観測である** — ここで「予算を満たしていない」
/// と書くと、記録が 1.6 の否定した読み方を事実として運ぶ。走査の腕の 1 行が運ぶのは
/// **フレーム時間の中央値と、要件 11.1 の予算の数**である（対象を「走査」とも名乗らない —
/// 最初の標本は起動直後の読み込みを測りうる）。
///
/// **記録の水準は事実で変える**: 描画の不成立は `warn`、走査の劣化は `info` である。
/// どちらも「失敗した」わけではない事実の記録であり、詳細度の設定（要件 8.7）が
/// `Off` / `Error` のときは残らない — 記録の水準は利用者が選ぶ（5.2 の契約）。
///
/// 呼び出し元ウィンドウは基盤が注入する引数から取るため、フロントエンドはウィンドウを
/// 偽装できない（要件 4.6）。**記録は必ず行われ**（失敗は封筒の失敗腕）、応答は呼び出し元の
/// 文脈だけを返す（画面の提示はこの往復の結果に依存しない）。
#[tauri::command]
pub fn diagnostics_record_render(
    window: WebviewWindow,
    request: RenderHealthRecordRequest,
) -> IpcResult<RenderHealthRecordResponse, IpcError> {
    let command = app_shell::ipc::command_names::DIAGNOSTICS_RECORD_RENDER;
    let context = caller_context(&window);
    let label = context.window.as_str();
    match request.report {
        RenderHealthReport::PaintFailed { failure, colors } => {
            log::warn!(
                "{command}: 表の描画が成立しなかった（ウィンドウ = {label}, 理由 = {}, 色数 = {}）",
                paint_failure_name(failure),
                match colors {
                    Some(count) => count.to_string(),
                    None => "読めず".to_owned(),
                },
            );
        }
        RenderHealthReport::Observation {
            first_screen_ms,
            scan_median_us,
            reached_row,
            row_count,
            edit_ms,
            undo,
            paint_failed,
            colors,
            items,
        } => {
            // **検証専用の 1 行である**（tasks.md 9.2）。3 OS の検査器が同じ形で読める場所は
            // 診断の記録だけである（macOS / Windows には AT-SPI が無い）。**要件の合否は書かない**
            // — 記録が運ぶのは実測であり、判定は検査器が要件値で行う（`ScanBelowBudget` と同じ規律）。
            log::info!(
                "{command}: グリッドの観測: 最初の画面ms={} 走査中央値us={} 到達行={} 行数={} \
編集ms={} 取消={} 描画={} 色数={}",
                measurement(first_screen_ms),
                measurement(scan_median_us),
                measurement(reached_row),
                measurement(row_count),
                measurement(edit_ms),
                match undo {
                    ObservationUndo::Ok => "ok".to_owned(),
                    ObservationUndo::Ng => "ng".to_owned(),
                    ObservationUndo::NotObserved => "未観測".to_owned(),
                },
                if paint_failed { "不成立" } else { "成立" },
                match colors {
                    Some(count) => count.to_string(),
                    None => "読めず".to_owned(),
                },
            );
            // **筋書きの項目は 1 行ずつ残す**（9.2 の段は「どの項目が成立しなかったか」を読む）。
            // 1 つの行へまとめない — 記録の読み手が grep で項目を引けるようにする。
            for result in items {
                log::info!(
                    "{command}: グリッドの観測の項目: {}={}",
                    observation_item_name(result.item),
                    match result.outcome {
                        ObservationItemOutcome::Ok => "ok",
                        ObservationItemOutcome::Ng => "ng",
                    },
                );
            }
        }
        RenderHealthReport::ScanBelowBudget {
            median_us,
            budget_us,
        } => {
            // **測った事実をそのまま運ぶ。**「予算を満たしていない」という結論は書かない —
            // 記録の閾値は画面側が要件値へ計測の刻みの許容を足して決めており
            // （`src/features/grid/renderHealth.ts` の `FRAME_BUDGET_TOLERANCE_MS`。1.6 の
            // 実画面の実測では健全な走査の中央値が 17.00 ms）、**ここで結論を名乗ると記録が
            // 1.6 の否定した読み方を事実として運ぶ**。要件 11.1 の合否は 9.2 の実画面の観測が
            // 要件値で判定する。
            //
            // **「走査の」とも名乗らない。**標本を始める引き金は可視区間の知らせであり、移植口は
            // 取り付けの直後にもそれを 1 回報せる（最初の標本は起動直後の読み込みを測りうる）。
            // 測っている対象は**フレーム時間の中央値**である。
            log::info!(
                "{command}: フレーム時間の中央値 {:.2} ms が予算 {:.2} ms を超えた（ウィンドウ = {label}）",
                f64::from(median_us) / 1000.0,
                f64::from(budget_us) / 1000.0,
            );
        }
    }
    IpcResult::Ok {
        data: RenderHealthRecordResponse { context },
    }
}

/// 筋書きの項目の識別子を、記録の 1 行へ載せる短い名前にする（**閉じた列挙の綴り**）。
///
/// 綴りは画面側（`src/features/grid/gridObservation.tsx`）と検査器（`scripts/`）が同じものを見る。
/// **どちらか片方だけを改名してもコンパイルは通る**ので、`app-shell` の
/// `bindings_declare_the_render_health_surface` が生成物の綴りを名指しで固定している。
fn observation_item_name(item: ObservationItem) -> &'static str {
    match item {
        ObservationItem::NestedExpansion => "nested_expansion",
        ObservationItem::InsertRow => "insert_row",
        ObservationItem::SortThenDelete => "sort_then_delete",
        ObservationItem::ViolationReason => "violation_reason",
        ObservationItem::ReferenceRows => "reference_rows",
        ObservationItem::PasteThroughMenu => "paste_through_menu",
        ObservationItem::SheetSwitchUndo => "sheet_switch_undo",
        ObservationItem::ReplaceDocument => "replace_document",
    }
}

/// 観測の 1 項目を記録の 1 行へ載せる形にする（**数値か「未観測」**。推測で埋めない）。
fn measurement(value: Option<u32>) -> String {
    match value {
        Some(number) => number.to_string(),
        None => "未観測".to_owned(),
    }
}

/// 成立しなかった理由の種別を、記録の 1 行へ載せる短い名前にする。
///
/// **利用者へ見せる文言ではない**（12.2 の提示は画面が組み立てる）。診断の記録を読む側
/// （9.2 の台本と、記録を読む人）が事実を機械的に見分けられるようにするための札である。
fn paint_failure_name(failure: RenderPaintFailure) -> &'static str {
    match failure {
        RenderPaintFailure::NoCanvas => "面が無い",
        RenderPaintFailure::Unpaintable => "面に塗って読み戻せない",
        RenderPaintFailure::Blank => "面が一様である",
    }
}

// ---------------------------------------------------------------------------
// メニューからの引き金（7.4 の登録口を通す）
// ---------------------------------------------------------------------------

/// 3 つの導線を置く部分メニュー。**7.4 が組み立てる木の 1 つの頂点になる。**
fn diagnostics_menu_path() -> MenuPath {
    MenuPath::new([DIAGNOSTICS_MENU_LABEL]).expect("位置は空でない")
}

/// 1 つの導線のメニュー項目を組み立てる（登録口へ渡す値の組み立てだけを切り出す）。
///
/// `handler` を差し替えられる形にしてあるのは、**GUI 無しで登録の受理と内容を検査できる**
/// ようにするためである（[`MenuRegistry::enroll`] は画面を要しない）。
fn diagnostics_item_spec(
    section: DiagnosticsSection,
    label: &'static str,
    accelerator: &'static str,
    handler: impl Fn(&MenuSelection) + Send + Sync + 'static,
) -> MenuItemSpec {
    let item = match section {
        DiagnosticsSection::Location => LOG_LOCATION_ITEM_ID,
        DiagnosticsSection::Export => EXPORT_ITEM_ID,
        DiagnosticsSection::Verbosity => VERBOSITY_ITEM_ID,
    };
    MenuItemSpec::new(OWNER, item, diagnostics_menu_path(), label, handler)
        .with_accelerator(accelerator)
}

/// 3 つの項目（導線・表示名・ショートカット）の一覧。**起動時の登録と、登録内容を検査する
/// テストが同じ定義を使う。**
fn diagnostics_items() -> [(&'static str, &'static str, &'static str, DiagnosticsSection); 3] {
    [
        (
            LOG_LOCATION_ITEM_ID,
            LOG_LOCATION_LABEL,
            LOG_LOCATION_ACCELERATOR,
            DiagnosticsSection::Location,
        ),
        (
            EXPORT_ITEM_ID,
            EXPORT_LABEL,
            EXPORT_ACCELERATOR,
            DiagnosticsSection::Export,
        ),
        (
            VERBOSITY_ITEM_ID,
            VERBOSITY_LABEL,
            VERBOSITY_ACCELERATOR,
            DiagnosticsSection::Verbosity,
        ),
    ]
}

/// 選択された導線を、**活性化の対象ウィンドウ**（7.5 の振り向け）へ通知する。
///
/// メニューの処理はイベントループのスレッドで走るため、ここでブロックしてはならない。
/// 送るのは 1 つのイベント（[`DIAGNOSTICS_REQUESTED_EVENT`]）だけで、送り先は**選ばれた時点で
/// 対象になっているウィンドウ**である（要件 3.5: ショートカットの操作は操作対象のウィンドウに
/// のみ適用する）。対象が無い場合（どのウィンドウもフォーカスされていない）は何もしない —
/// 送り先が無いのに全ウィンドウへ配ると、触っていないウィンドウの画面が勝手に変わる。
fn request_section(app: &AppHandle, selection: &MenuSelection, section: DiagnosticsSection) {
    let Some(label) = selection.window().cloned() else {
        log::warn!(
            "診断の導線（{}）: 対象ウィンドウが無いため画面を開かない",
            section_name(section),
        );
        return;
    };
    let payload = DiagnosticsRequestedEvent { section };
    match app.emit_to(label.as_str(), DIAGNOSTICS_REQUESTED_EVENT, payload) {
        Ok(()) => log::info!(
            "診断の導線の要求を送った: ウィンドウ = {} / 導線 = {}",
            label.as_str(),
            section_name(section),
        ),
        Err(error) => log::error!(
            "診断の導線の要求を送れなかった（ウィンドウ = {} / 導線 = {}）: {error}",
            label.as_str(),
            section_name(section),
        ),
    }
}

/// 起動時に 1 回だけ 3 つの導線を 7.4 の登録口へ登録する。`lifecycle::run` がメニューの構築
/// （[`crate::menu::install`]）の後に呼ぶ。
///
/// 登録は [`MenuRegistry::register`] を通すので、**ショートカットの競合（要件 3.4）と項目の
/// 識別子の重複（7.4 の `ItemIdConflict`）は登録時に検出され、登録元（このモジュール）へ報告
/// される** — 片方を黙って無効化する経路は無い。登録に失敗しても起動は続ける（診断の導線が
/// 引けないことより、アプリが立ち上がらないことの方が悪い）。
pub fn install(app: &AppHandle) {
    let registry = app.state::<MenuRegistry>();
    for (item, label, accelerator, section) in diagnostics_items() {
        let handling_app = app.clone();
        let spec = diagnostics_item_spec(section, label, accelerator, move |selection| {
            request_section(&handling_app, selection, section);
        });
        if let Err(error) = registry.register(app, spec) {
            log::error!("診断のメニュー項目を登録できなかった（{item}）: {error}");
        }
    }
    log::info!(
        "診断の導線をメニューへ登録した（{} の 3 項目）",
        DIAGNOSTICS_MENU_LABEL,
    );
}

#[cfg(test)]
mod tests {
    use app_shell::accelerator::{Accelerator, MenuItemId};
    use app_shell::ipc::command_names;

    use super::{
        diagnostics_item_spec, diagnostics_items, diagnostics_menu_path, export_file_name,
        records_of, verbosity_response, DiagnosticsExportRecords, DiagnosticsLevel,
        DiagnosticsSection, EXPORT_ITEM_ID, LOG_LOCATION_ITEM_ID, OWNER, VERBOSITY_ITEM_ID,
    };
    use crate::menu::{MenuNode, MenuPath, MenuRegistry};

    /// 書き出すファイルの名前が**決定的**で、拡張子が `.log` であることを固定する。
    /// 同じ秒に 2 回要求すると同じ名前になる（4.5 の置き換え契約）ことも含めて固定する。
    #[test]
    fn export_file_name_is_deterministic_and_named() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        assert_eq!(export_file_name(at), "jxcel-diagnostics-1700000000.log");
        assert_eq!(export_file_name(at), export_file_name(at));
        // 秒が違えば名前も違う（直前の書き出しと見分けられる）。
        assert_ne!(
            export_file_name(at),
            export_file_name(at + std::time::Duration::from_secs(1))
        );
    }

    /// 記録が 0 件でも**成功**であり、そのことが境界では閉じた列挙として現れる（要件 8.6）。
    #[test]
    fn no_records_is_reported_as_an_empty_but_successful_export() {
        assert_eq!(records_of(0), DiagnosticsExportRecords::Empty);
        assert_eq!(records_of(1), DiagnosticsExportRecords::Merged);
        assert_eq!(records_of(6), DiagnosticsExportRecords::Merged);
    }

    /// 詳細度の応答が**現在値と、選べる値の全体を昇順で**運ぶ（要件 8.7）。
    #[test]
    fn verbosity_response_lists_every_level_in_order() {
        let context = app_shell::ipc::WindowContext {
            window: app_shell::ipc::WindowLabel::new("empty-1"),
        };
        let response = verbosity_response(
            context.clone(),
            app_shell::diagnostics::DiagnosticsLevel::Warn,
        );
        assert_eq!(response.context, context);
        assert_eq!(response.level, DiagnosticsLevel::Warn);
        assert_eq!(
            response.levels,
            vec![
                DiagnosticsLevel::Off,
                DiagnosticsLevel::Error,
                DiagnosticsLevel::Warn,
                DiagnosticsLevel::Info,
                DiagnosticsLevel::Debug,
                DiagnosticsLevel::Trace,
            ]
        );
    }

    /// 3 つの導線が登録口を通り、**同じ部分メニューへ競合しないショートカット付きで並ぶ**。
    ///
    /// 起動時に実際に登録される 3 件を同じ登録簿へ入れるので、**3 者間の競合が無いことも
    /// ここで固定される**（4.6 / 要件 3.4）。あわせて、組み込みの「終了」（7.4）と
    /// 「開く」（7.7）と同じ登録元の名前空間を使っていることも検査する。
    #[test]
    fn the_three_entry_points_are_registered_in_their_own_submenu() {
        let registry = MenuRegistry::new();
        let noop = |_: &super::MenuSelection| {};
        for (item, label, accelerator, section) in diagnostics_items() {
            registry
                .enroll(diagnostics_item_spec(section, label, accelerator, noop))
                .expect("同じ部分メニューへ競合なく並ぶ");
            assert_eq!(item, expected_item_id(section));
        }

        let path = diagnostics_menu_path();
        assert_eq!(
            path.segments(),
            &["診断".to_owned()],
            "3 つの導線は診断の部分メニューに置く"
        );

        let model = registry.model();
        let submenu = model
            .top()
            .iter()
            .find(|submenu| submenu.label() == "診断")
            .expect("診断の部分メニューがある");
        let items: Vec<_> = submenu
            .children()
            .iter()
            .filter_map(|child| match child {
                MenuNode::Item(item) => Some(item),
                MenuNode::Submenu(_) => None,
            })
            .collect();
        assert_eq!(items.len(), 3, "保存場所・書き出し・詳細度の 3 件である");

        for (item, label, accelerator, _) in diagnostics_items() {
            let node = items
                .iter()
                .find(|node| node.item() == &MenuItemId::new(item))
                .unwrap_or_else(|| panic!("{item} が診断の部分メニューにある"));
            assert_eq!(node.label(), label);
            assert_eq!(
                node.accelerator(),
                Some(&Accelerator::parse(accelerator).expect("プラットフォーム解決済みの綴り")),
                "{item}"
            );
            assert_eq!(node.owner().as_str(), OWNER);
        }
    }

    /// 導線と項目の識別子の対応（テストの期待値を 1 箇所に持つ）。
    fn expected_item_id(section: DiagnosticsSection) -> &'static str {
        match section {
            DiagnosticsSection::Location => LOG_LOCATION_ITEM_ID,
            DiagnosticsSection::Export => EXPORT_ITEM_ID,
            DiagnosticsSection::Verbosity => VERBOSITY_ITEM_ID,
        }
    }

    /// 3 つの導線が**既存の 4 つのコマンド名**（4.5 の申し送り）に対応していることを固定する。
    /// 名前は単一の源（`command_names`）の定数である。
    #[test]
    fn the_entry_points_use_the_existing_diagnostics_command_names() {
        assert_eq!(
            command_names::DIAGNOSTICS_LOG_LOCATION,
            "diagnostics_log_location"
        );
        assert_eq!(command_names::DIAGNOSTICS_EXPORT, "diagnostics_export");
        assert_eq!(
            command_names::DIAGNOSTICS_VERBOSITY_GET,
            "diagnostics_verbosity_get"
        );
        assert_eq!(
            command_names::DIAGNOSTICS_VERBOSITY_SET,
            "diagnostics_verbosity_set"
        );
    }

    /// 詳細度の閉じた列挙が**中核と同じ順序**で並ぶ（`Off` < `Error` < `Warn` < `Info` < `Debug` <
    /// `Trace`。4.5 の契約）。画面に出る選択肢の並びはこの定義だけに由来する。
    #[test]
    fn the_closed_verbosity_enum_keeps_the_core_order() {
        let mut previous = app_shell::diagnostics::DiagnosticsLevel::Off;
        for (index, level) in DiagnosticsLevel::ALL.into_iter().enumerate() {
            let core: app_shell::diagnostics::DiagnosticsLevel = level.into();
            if index > 0 {
                assert!(previous < core, "{previous:?} の次に {core:?} が来ている");
            }
            previous = core;
        }
        assert_eq!(
            previous,
            app_shell::diagnostics::DiagnosticsLevel::Trace,
            "最後は最も細かい詳細度である"
        );
        // `MenuPath` は空を許さない（トップレベルに単独項目を置けない）。
        assert!(MenuPath::new(Vec::<String>::new()).is_err());
    }
}

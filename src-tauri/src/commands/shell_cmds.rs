//! app-shell 自身のコマンド — ドメインクレートを呼ばず、シェルの機構（設定・診断・
//! ウィンドウ・描画の通知）だけを扱うコマンドを置く。
//!
//! 所有: app-shell 自身のコマンド面（design.md「File Structure Plan」の `commands/shell_cmds.rs`）。
//! 要件: 4.1, 4.4, 4.6, 7.1, 7.3, 7.4。
//!
//! タスク 7.1 が置いた実体は次の 3 つである:
//!
//! 1. **設定の読み書き**（[`settings_get`] / [`settings_set`]）。どちらも共通の封筒
//!    [`IpcResult`] を返し、例外に頼らない（要件 4.4）。カタログに無い鍵は封筒の誤り側の腕に
//!    なり、原因は [`IpcError::Settings`] が運ぶ（`SettingsKey::from_name` が唯一の入口であり、
//!    任意の鍵は入らない。要件 7.7）。両コマンドは**呼び出し元ウィンドウを引数として受け取り**、
//!    境界の文脈 [`WindowContext`] へ写して応答に載せる（要件 4.6）。
//! 2. **設定変更の通知の配線**（[`start_settings_notifications`]）。起動時に共有実体
//!    （`Arc<FileSettingsStore>`。要件 7.3 の同一実体）へ**1 回だけ** `subscribe()` し、返った
//!    受信側を専用スレッドが所有して `recv()` ループを回し、変更のたびに**全ウィンドウ**へ
//!    Tauri イベントを emit する（要件 7.4。tasks.md 4.3 から 7.1 への申し送り）。
//! 3. その他のコマンド（診断の導線はタスク 9.5、ウィンドウの終了拒否はタスク 7.6）は
//!    **この段では置かない**。各タスクが自分の関数を足し、[`crate::commands`] の根へ列挙する。
//!
//! # 記録に何を書くか（要件 8.4 の秘匿の規律）
//!
//! コマンドは呼び出し元ウィンドウのラベルと**鍵の名前だけ**を記録に書く。**値は書かない。**
//! 鍵は `SettingsKey` の閉じたカタログの名前であり、設定に載りうるのはシェルの設定だけである
//! （要件 7.7）。ドキュメントの内容を指す名前はカタログに存在しないため、この経路から
//! 記録へドキュメントの内容が混入しえない。値まで記録に流す経路は作らない
//! （`app_shell::diagnostics::Redacted` はドキュメントの内容を扱う経路の規律であり、ここは
//! そもそも値を記録しない）。
//!
//! # 通知が全ウィンドウへ届く理由と、値の形
//!
//! 設定は全ウィンドウで共有される（要件 7.3）ため、通知は 1 つのウィンドウへではなく
//! `AppHandle::emit` で**全ウィンドウ**へ配る。運ぶのは境界の型 `SettingsChangedEvent`
//! （`crates/app-shell/src/ipc/mod.rs` が定義。`settings` モジュールに `ts-rs` を付けない
//! 規約を守るため、コアの `SettingsChanged` をそのまま境界へ出さず、ここで写す）である。

use std::sync::Arc;

use app_shell::ipc::command_names;
use app_shell::ipc::{
    IpcError, IpcResult, SettingsChangedEvent, SettingsGetRequest, SettingsResponse,
    SettingsSetRequest, SettingsValue, WindowContext, WindowLabel, SETTINGS_CHANGED_EVENT,
};
use app_shell::settings::{FileSettingsStore, SettingsKey, SettingsStore};
use tauri::{AppHandle, Emitter, Runtime, State, WebviewWindow};
use tauri_plugin_log::log;

// ---------------------------------------------------------------------------
// 設定の読み書き（要件 4.1, 4.4, 4.6, 7.1）
// ---------------------------------------------------------------------------

/// 名前付きの設定値を読み取る（要件 7.1、7.3）。
///
/// 応答は共通の封筒 [`IpcResult`] である。カタログに無い鍵は [`IpcError::Settings`] を載せた
/// 誤り側の腕になり、**`Err` を返して `invoke` に拒否させることはしない** — 拒否にすると
/// フロントエンド側のラッパが「フロントエンド局所の失敗」として扱うことになり、ドメインの
/// 失敗と区別できなくなる（要件 4.4、tasks.md 2.4）。
///
/// `window` は Tauri が**呼び出し元のウィンドウ**を注入する引数である。これにより呼び出し先は
/// 呼び出し元を識別でき（要件 4.6）、その識別子は応答の [`WindowContext`] に載って
/// フロントエンドからも観測できる。
#[tauri::command]
pub fn settings_get(
    settings: State<'_, Arc<FileSettingsStore>>,
    window: WebviewWindow,
    request: SettingsGetRequest,
) -> IpcResult<SettingsResponse, IpcError> {
    let command = command_names::SETTINGS_GET;
    let context = caller_context(&window);
    log::info!("{command}: 呼び出し元ウィンドウ = {}", context.window.as_str());

    let key = match catalog_key(command, &request.key) {
        Ok(key) => key,
        Err(error) => return IpcResult::Err { error },
    };
    // 記録に値は書かない（モジュール doc「記録に何を書くか」）。
    let value = settings.get::<SettingsValue>(&key);
    IpcResult::Ok {
        data: SettingsResponse {
            context,
            key: key.as_str().to_owned(),
            value,
        },
    }
}

/// 名前付きの設定値を書き込む（要件 7.1、7.4）。
///
/// 書き込みが成功したときだけ応答の成功側の腕を返す。値の符号化やファイルへの書き込みに
/// 失敗した場合（`SettingsError`）は [`IpcError::Settings`] を載せた誤り側の腕になる。
/// **メモリとディスクは失敗時に変更前のままである**（`FileSettingsStore::set` の契約）。
///
/// 同一値の書き込みは保存と成功応答を行うが、変更通知は配られない（4.3 の意味論。値が
/// 変わっていないので「他のウィンドウへ反映する」対象が無い）。
#[tauri::command]
pub fn settings_set(
    settings: State<'_, Arc<FileSettingsStore>>,
    window: WebviewWindow,
    request: SettingsSetRequest,
) -> IpcResult<SettingsResponse, IpcError> {
    let command = command_names::SETTINGS_SET;
    let context = caller_context(&window);
    log::info!("{command}: 呼び出し元ウィンドウ = {}", context.window.as_str());

    let key = match catalog_key(command, &request.key) {
        Ok(key) => key,
        Err(error) => return IpcResult::Err { error },
    };
    // 記録に値は書かない（モジュール doc「記録に何を書くか」）。
    let value = request.value;
    if let Err(error) = settings.set::<SettingsValue>(&key, &value) {
        log::warn!("{command}: 設定を書き込めない（キー: {}）: {error}", key.as_str());
        return IpcResult::Err {
            error: IpcError::Settings { message: error.to_string() },
        };
    }
    IpcResult::Ok {
        data: SettingsResponse {
            context,
            key: key.as_str().to_owned(),
            value: Some(value),
        },
    }
}

/// Tauri が注入した呼び出し元ウィンドウを、境界の文脈（要件 4.6）へ写す。
///
/// **境界の型を新設しない。** `app_shell::ipc::WindowContext` / `WindowLabel` をそのまま使う
/// （tasks.md 2.1 / 6.2 の申し送り。3 つ目の識別子を作らない）。
fn caller_context(window: &WebviewWindow) -> WindowContext {
    WindowContext { window: WindowLabel::new(window.label()) }
}

/// 要求された鍵を閉じたカタログ（[`SettingsKey`]）へ解決する（要件 7.7）。
///
/// カタログに無い名前は [`IpcError::Settings`] になる。メッセージは**要求された名前を含む**
/// （原因を識別できる情報を返す。要件 4.4）が、記録に書くのは名前だけであり、値は書かない。
fn catalog_key(command: &str, requested: &str) -> Result<SettingsKey, IpcError> {
    SettingsKey::from_name(requested).ok_or_else(|| {
        log::warn!("{command}: カタログに無い設定キー: {requested}");
        IpcError::Settings { message: format!("カタログに無い設定キー: {requested}") }
    })
}

// ---------------------------------------------------------------------------
// 設定変更の通知（要件 7.4。4.3 の申し送り）
// ---------------------------------------------------------------------------

/// 設定変更の通知をフロントエンドへ届ける配線を起動する（要件 7.4）。
///
/// **起動時に 1 回だけ呼ぶ**（`lifecycle::run` が構築の後に呼ぶ）。共有実体へ `subscribe()` は
/// 1 回だけ行い、返った受信側（[`std::sync::mpsc::Receiver`] は `Sync` ではない）を専用
/// スレッドが所有して `recv()` ループを回す。変更のたびに境界の型
/// [`SettingsChangedEvent`] を**全ウィンドウ**へ emit する。
///
/// - **通知は耐久化の後にしか来ない**（4.3）。したがってこのループが受け取った時点で、新しい
///   値はメモリとディスクの両方にある。失敗した書き込みと同一値の書き込みは通知しないので、
///   ここへは「実際に変わった」ときだけ届く。
/// - **全ウィンドウへ配る。** 設定は全ウィンドウで共有される値だからである（要件 7.3、7.4）。
///   `AppHandle::emit` は開いているすべてのウィンドウへ配る。
/// - 送信に失敗しても（ウィンドウが 1 枚も無い等）ループは止めない。**起動を妨げない。**
pub fn start_settings_notifications<R: Runtime>(
    app: &AppHandle<R>,
    settings: &Arc<FileSettingsStore>,
) {
    let receiver = settings.subscribe();
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("settings-changed".to_owned())
        .spawn(move || {
            for changed in receiver.iter() {
                // 記録に値は書かない（モジュール doc「記録に何を書くか」）。
                let event = SettingsChangedEvent {
                    key: changed.key.as_str().to_owned(),
                    value: SettingsValue::new(changed.value),
                };
                log::info!("設定変更を全ウィンドウへ通知する（キー: {}）", event.key);
                if let Err(error) = app.emit(SETTINGS_CHANGED_EVENT, event) {
                    log::warn!("設定変更の通知を配れなかった: {error}");
                }
            }
        });
    if let Err(error) = spawned {
        log::warn!("設定変更の通知スレッドを起動できなかった: {error}");
    }
}

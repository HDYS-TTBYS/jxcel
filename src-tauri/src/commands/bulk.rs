//! 大きなペイロードの経路 — `tauri::ipc::Response` の生バイトで応答し、JSON を通さない。
//!
//! 所有: 一括転送のコマンド（design.md「File Structure Plan」の `commands/bulk.rs`）。
//! 要件: 4.5（10 万行規模を 1 回の呼び出しで）、4.6（呼び出し元ウィンドウの識別）、
//! 8.4（記録に内容を書かない）。
//!
//! # 経路の性質（research.md「IPC の転送形式と大きなペイロード」の実測に基づく）
//!
//! - **応答**の内容型が JSON でもテキストでもない場合、フロントエンドには `ArrayBuffer` と
//!   して届く。`tauri::ipc::Response::new(Vec<u8>)` は `application/octet-stream` として
//!   配られるため、**両側とも JSON を触らない**。
//! - **引数**側で生バイトを送る場合、バッファが**引数全体**でなければならない。オブジェクトへ
//!   入れ子にすると Tauri が `Uint8Array` を `Array.from()` で数値の配列へ変換し、JSON
//!   （`InvokeBody::Json`）として送る。受け手は生バイトとして扱えなくなる。
//! - `Channel` は一括転送に向かない（閾値を超えるメッセージは追加の往復を生む）。本経路では
//!   使わない。
//!
//! # 封筒の規則からの例外（タスク 7.1 との関係。消費者がどう知るか）
//!
//! タスク 7.1 は面の規則として「すべてのコマンドは `IpcResult` を返し、例外に頼らない」を
//! 置いた（design.md「CommandSurface」、要件 4.4）。[`bulk_echo`] はその**例外**である。
//! 理由は要件 4.5 が「JSON を経由しない」ことを要求しているためである:
//!
//! - 封筒 `IpcResult` は `serde` により JSON へ直列化される。100k 行のペイロードを封筒へ
//!   載せると応答は JSON になり、受け手は `JSON.parse` を通すことになる。これは要件 4.5 が
//!   避けよと言っている経路そのものである（design.md「Performance & Scalability」）。
//! - したがって**成功と失敗を封筒の腕で区別できない**。成功は「生バイトの応答が返る」ことで
//!   あり、失敗は `invoke` の拒否（フロントエンドの `IpcClientError` では `kind: "Frontend"`）
//!   として現れる。**封筒の `IpcError` をこの経路で返す余地は無い。** ドメインの失敗を
//!   封筒で返す必要がある呼び出しは、封筒を返す通常のコマンド（例: `settings_get`）を使うこと。
//! - **消費者は入口の違いとして例外を知る。** `src/ipc/client.ts` の `invokeRaw` は
//!   `ArrayBuffer` を返す別の入口であり、封筒を通る `invokeCommand` とは関数が分かれている。
//!   `invokeCommand` を `bulk_echo` に使うと `application/octet-stream` のバイト列を封筒として
//!   解釈することになり、`status` の分岐が型でも実行でも成立しない。
//!
//! 要件 4.6 は**呼び出し先が満たす**: [`bulk_echo`] は Tauri が注入する [`WebviewWindow`] を
//! 受け取り、そのラベルを記録に残す（フロントエンドの申告ではないので偽装できない）。生バイト
//! の応答は `WindowContext` を運べないため、文脈は応答には載らない（7.1 の封筒経路との差）。
//! 呼び出し元まで文脈を返す必要がある消費者は、封筒を返すコマンドを使うこと。
//!
//! # バッチ意味論（行ごとに境界を越えない）
//!
//! この経路に**行指向の API は無い**。[`bulk_echo`] はバッチ全体を 1 引数として受け取り、
//! 1 応答で返す。行ごとに呼ぶコマンドは本モジュールにもコマンド面にも存在しない
//! （`COMMAND_NAMES` に現れる一括転送の名前は [`command_names::BULK_ECHO`] の 1 つだけである）。
//!
//! # 記録に何を書くか
//!
//! 書くのは**呼び出し元ウィンドウのラベルと受信バイト数だけ**である。ペイロードの内容
//! （セル値やスキーマ）は決して記録へ流さない（要件 8.4）。

use app_shell::ipc::command_names;
use tauri::WebviewWindow;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri_plugin_log::log;

/// 一括転送で受け付ける最大バイト数（64 MiB）。
///
/// 要件 4.5 の規模を余裕をもって収める大きさである。1 行 8 バイトの二進バッチなら 100k 行で
/// 約 0.8 MB、1 行 32 バイトの行テキストなら約 3.2 MB にしかならない。上限は「際限のない入力を
/// 受け取らない」ために置く: [`bulk_echo`] は受け取ったバイトをそのまま返すため、応答を作る
/// 時点で入力と同量の複製をもう 1 つ持つ。上限を超える入力は例外ではなく**空の応答**と警告に
/// なる（呼び出し側は `ArrayBuffer` の長さ 0 で気づく）。
pub const MAX_BULK_BYTES: usize = 64 * 1024 * 1024;

/// バッチ全体を生バイトで受け取り、そのまま生バイトで返す（要件 4.5）。
///
/// # 引数の契約
///
/// **バッファは引数全体でなければならない。** フロントエンドは
/// `invoke("bulk_echo", buffer)` の形で呼ぶ（`src/ipc/client.ts` の `invokeRaw` がこれを行う）。
/// `invoke("bulk_echo", { payload: buffer })` のように入れ子にすると、Tauri は
/// `Uint8Array` を `Array.from()` で数値の配列へ変換して JSON として送るため、ここへは
/// [`InvokeBody::Json`] が届く。その場合は**例外を投げず**空の応答と警告を返す
/// （生バイトでないことを呼び出し側が長さ 0 で観測できる）。
///
/// # 応答の契約
///
/// 受け取ったバイトをそのまま返す。したがって呼び出し側は送ったバイトと**同一の列**を
/// `ArrayBuffer` として受け取る（成功の判定はバイト列の一致で行う）。封筒を返さない理由は
/// モジュール doc「封筒の規則からの例外」を参照。
///
/// # 大きさ
///
/// 上限は [`MAX_BULK_BYTES`]。超える入力は複製を作らずに空の応答と警告になる。
///
/// # 呼び出し元
///
/// `window` は Tauri が注入する呼び出し元ウィンドウである（要件 4.6）。ラベルは記録に残す。
#[tauri::command]
pub fn bulk_echo(window: WebviewWindow, request: Request<'_>) -> Response {
    let command = command_names::BULK_ECHO;
    // 呼び出し元の識別（要件 4.6）。Tauri が注入する値であり、フロントエンドの申告ではない。
    let caller = window.label();
    match request.body() {
        InvokeBody::Raw(bytes) => {
            if bytes.len() > MAX_BULK_BYTES {
                // 記録に内容は書かない（要件 8.4）。書くのは呼び出し元と長さだけ。
                log::warn!(
                    "{command}: ペイロードが上限を超える（呼び出し元ウィンドウ = {caller}, \
                     受信バイト数 = {}, 上限 = {}）— 内容は返さない",
                    bytes.len(),
                    MAX_BULK_BYTES
                );
                return Response::new(Vec::new());
            }
            log::info!(
                "{command}: 呼び出し元ウィンドウ = {caller}, 受信バイト数 = {}",
                bytes.len()
            );
            // 要求の本体は借用で渡るため、応答を作るには 1 回の複製が要る（memcpy 1 回）。
            Response::new(bytes.clone())
        }
        InvokeBody::Json(_) => {
            // 入れ子の罠（モジュール doc「経路の性質」）。生バイトではないので返せるものが無い。
            log::warn!(
                "{command}: 引数が生バイトでない（呼び出し元ウィンドウ = {caller}）— \
                 バッファは引数全体でなければならない。入れ子にすると数値の配列へ変換される"
            );
            Response::new(Vec::new())
        }
    }
}

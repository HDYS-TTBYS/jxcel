// このファイルは生成物である。**手で編集しない。**
// 型の宣言は crates/app-shell/src/ipc/ の定義から ts-rs が、コマンド名の定数は
// command_names.rs の COMMAND_NAMES が生成する。直すのは生成元である。
//
// 再生成（リポジトリルートで実行する）: cargo run -p app-shell --bin generate-bindings
// 本ファイルは追跡対象である。ドリフト検査（タスク 2.3）がバイト比較する。

/**
 * 境界を越えるコマンド名の一覧。`crates/app-shell/src/ipc/command_names.rs` の
 * `COMMAND_NAMES` と同一の内容・同一の順序である。`src-tauri` のハンドラ登録と
 * 本生成物が同じ配列を参照し、名前のドリフトを構造的に塞ぐ。
 */
export const COMMAND_NAMES = [
  "render_heartbeat",
  "can_close_window",
  "settings_get",
  "settings_set",
  "pick_document_file",
  "bulk_echo",
  "diagnostics_log_location",
  "diagnostics_export",
  "diagnostics_verbosity_get",
  "diagnostics_verbosity_set",
] as const;

// ---------------------------------------------------------------------------
// 境界を越える型（crates/app-shell/src/ipc/ の定義から ts-rs が生成）

/**
 * 失敗の原因を区別できる列挙（要件 4.4）。文字列だけのエラーにしない。
 *
 * `kind` を判別子とし、原因ごとの詳細を `detail` に持つ判別可能な合併型として TypeScript へ
 * 落ちる。利用側は `kind` で網羅的に分岐でき、原因ごとに異なる扱いを型で強制できる
 * （tasks.md 2.4）。
 */
export type IpcError = { "kind": "Settings", "detail": { message: string, } } | { "kind": "Sidecar", "detail": { message: string, } } | { "kind": "Window", "detail": { message: string, } };
/**
 * 境界を越えるすべてのコマンドが返す封筒（要件 4.2、4.4）。
 *
 * `status` を判別子とし、成功（`ok`）と失敗（`error`）を型で区別する判別可能な合併型として
 * TypeScript へ落ちる。利用側は `status` で網羅的に分岐できる（tasks.md 2.4 がこの性質の上に
 * 薄い呼び出しラッパを載せる）。本型を含め、境界の生成物に `any` を混入させない
 * （research.md 決定 1 が `tauri-specta` を却下した理由のひとつ）。
 */
export type IpcResult<T, E> = { "status": "ok", data: T, } | { "status": "error", error: E, };
/**
 * コマンド呼び出しの文脈（要件 4.2、4.6）。呼び出し元ウィンドウを呼び出し先が識別できる
 * ようにする。
 */
export type WindowContext = { 
/**
 * 呼び出し元ウィンドウのラベル。
 */
window: WindowLabel, };
// コマンド応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指ししない
// ため、境界が名指しできる具体形を明示的に置く。
export type WindowContextResult = IpcResult<WindowContext, IpcError>;
/**
 * 境界を越えるウィンドウの識別子（要件 4.2、4.6）。
 *
 * 境界を越える識別子は 64 ビット整数をそのまま公開せず、文字列表現とする。JavaScript の
 * `number` は IEEE 754 の倍精度であり、`i64` / `u64` の全域を正確に表せないためである。
 * TypeScript 側の型も `string` に固定する（design.md「IpcContract」の不変条件）。
 */
export type WindowLabel = string;

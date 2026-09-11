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

/**
 * 境界を越えるイベント名の一覧。`crates/app-shell/src/ipc/mod.rs` の定義と同一で、
 * フロントエンドはこの定数だけを参照する（文字列リテラルを書かない）。
 */
export const SETTINGS_CHANGED_EVENT = "settings_changed";

// ---------------------------------------------------------------------------
// 境界を越える型（crates/app-shell/src/ipc/ の定義から ts-rs が生成）

/**
 * ウィンドウを閉じてよいかの問い合わせの応答（タスク 7.6。要件 2.6、4.6）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む。**呼び出し元は Tauri が注入する
 * `WebviewWindow` から得るので、**フロントエンドがウィンドウの識別子を payload で申告する
 * 経路は存在しない**（偽装できない。要件 4.6、tasks.md 7.1）。`verdict` が委譲先の判定で
 * あり、`Allow` のときだけフロントエンドがウィンドウを破棄する。
 */
export type CanCloseWindowResponse = { 
/**
 * 呼び出し元ウィンドウの文脈（要件 4.6）。
 */
context: WindowContext, 
/**
 * ドキュメント所有者の判定（要件 2.6）。
 */
verdict: WindowCloseVerdict, };
// 終了可否の問い合わせの応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を
// 名指ししないため、境界が名指しできる具体形を明示的に置く。
export type CanCloseWindowResult = IpcResult<CanCloseWindowResponse, IpcError>;
/**
 * ファイル選択の結果（タスク 7.7。要件 2.4）。
 *
 * 選択手段（`src-tauri/src/dialog.rs` の DialogGate）が得た結果と、その位置をドキュメント
 * 所有者へ引き渡した結果を、**1 つの判別可能な合併型**にまとめてフロントエンドへ返す。
 * `outcome` を判別子とするため、利用側は網羅的に分岐できる。
 *
 * **`Cancelled` と `Rejected` は失敗ではない。**「利用者が取り消した」ことも「所有者が
 * 受け取らなかった」ことも、コマンドが正常に答えた結果である。したがって封筒
 * （[`IpcResult`]）の `status: "error"` の腕には載せない — 載せると「通信が失敗した」ことと
 * 区別できなくなる（tasks.md 7.6 が終了拒否で同じ判断をしている）。`Rejected` は利用者へ
 * 伝えるための材料（`reason`）を運び、**見せ方を決めるのは呼び出し元である**。
 *
 * **選択された位置そのものは境界を越えない。** 位置は `DocumentHost::attach` へ引き渡す
 * だけであり（要件 2.4）、アプリケーションシェルもフロントエンドもその中身に触れない。
 * したがってパスを表す型はここに現れない。
 */
export type DocumentPickOutcome = { "outcome": "Cancelled" } | { "outcome": "Attached" } | { "outcome": "Rejected", 
/**
 * 拒否の理由。
 */
reason: string, };
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
 * ファイル選択の応答（タスク 7.7。要件 2.4、4.6）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む。**呼び出し元は Tauri が注入する `WebviewWindow`
 * から得るので、**フロントエンドがウィンドウの識別子を payload で申告する経路は存在しない**
 * （偽装できない。要件 4.6、tasks.md 7.1）。`outcome` が選択と引き渡しの結果である。
 *
 * **要求の型は無い。** この機能に必要な入力は操作対象のウィンドウだけであり、それは基盤が
 * 注入する（[`CanCloseWindowResponse`] と同じ形）。
 */
export type PickDocumentFileResponse = { 
/**
 * 呼び出し元ウィンドウの文脈（要件 4.6）。
 */
context: WindowContext, 
/**
 * 選択と引き渡しの結果（要件 2.4）。
 */
outcome: DocumentPickOutcome, };
// ファイル選択の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指し
// しないため、境界が名指しできる具体形を明示的に置く。
export type PickDocumentFileResult = IpcResult<PickDocumentFileResponse, IpcError>;
/**
 * 初回描画の通知の要求（タスク 8.2。要件 10.1、10.2）。
 *
 * 運ぶのは**ラスタライザの文字列だけ**である（フロントエンドが
 * `WEBGL_debug_renderer_info` の `UNMASKED_RENDERER_WEBGL` から得た値。research.md 決定 7）。
 * 取得できない環境では `null` であり、その場合も通知が届いたこと自体は描画成立の証拠に
 * なる（中核の写像を参照）。
 *
 * **ウィンドウは運ばない。** 呼び出し元は Tauri が注入する `WebviewWindow` から取るため、
 * フロントエンドがウィンドウを偽装する経路は存在しない（要件 4.6、tasks.md 7.1）。
 */
export type RenderHeartbeatRequest = { 
/**
 * ラスタライザの文字列。取得できなければ `null`。
 */
renderer: string | null, };
/**
 * 初回描画の通知の応答（タスク 8.2。要件 10.1、10.2、4.6）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。`verdict` はこの通知で確定した
 * 判定であり、2 回目以降の通知では最初に確定した値がそのまま返る（上書きしない）。
 */
export type RenderHeartbeatResponse = { 
/**
 * 呼び出し元ウィンドウの文脈（要件 4.6）。
 */
context: WindowContext, 
/**
 * 確定した判定（要件 10.1、10.2）。
 */
verdict: RenderVerdict, };
// 初回描画の通知の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を
// 名指ししないため、境界が名指しできる具体形を明示的に置く。
export type RenderHeartbeatResult = IpcResult<RenderHeartbeatResponse, IpcError>;
/**
 * 初回描画の判定（タスク 8.2。要件 10.1、10.2）。**三値である。**
 *
 * 判定の実体は Tauri 非依存の中核（`crates/app-shell/src/render.rs`）にあり、本型はその
 * 結果を境界へ出すための形である（`ts-rs` の derive を付けてよい唯一の場所が本モジュールで
 * あるという不変条件に従う）。写像の根拠は `render.rs` のモジュール doc にある。要約:
 *
 * - `Painted`（[`RenderVerdict::Painted`]）: 描画フレームの中から通知が届き、ラスタライザが
 *   ハードウェア加速（または判別不能）だった。**描画が成立した。**
 * - `SoftwareRaster`（[`RenderVerdict::SoftwareRaster`]）: 通知が届いたが、ラスタライザが
 *   既知のソフトウェア実装だった。**描画は成立している**（低速な経路である）。
 * - `NoPaint`（[`RenderVerdict::NoPaint`]）: 期限までに通知が届かなかった。**描画が成立して
 *   いない。** したがってこの値だけが、次回起動で代替経路を適用するための印を立てる（要件 10.3）。
 *
 * **タイムアウト（`NoPaint`）とソフトウェアラスタライザ（`SoftwareRaster`）は別の値で
 * ある。**前者は描画の不成立、後者は描画の成立であり、混同すると要件 10.3 の代替経路を
 * 正常な環境へ適用してしまう。
 */
export type RenderVerdict = "Painted" | "SoftwareRaster" | "NoPaint";
/**
 * 設定変更の通知（タスク 7.1。要件 7.4）。
 *
 * 設定ストアの通知（`crate::settings::SettingsChanged`）を境界の形へ写したものである。運ぶのは
 * **カタログにある閉じた鍵の名前と、その新しい値だけ**である。したがってドキュメントの内容
 * （セル値・スキーマ）を指す鍵はカタログに存在せず、この経路には載りえない（要件 7.7、8.4）。
 * 記録側へ値を渡す経路はこの型ではなく [`crate::diagnostics::Redacted`] を通る（タスク 7.1 は
 * 記録に**鍵だけ**を書き、値は決して書かない）。
 */
export type SettingsChangedEvent = { 
/**
 * 変更された鍵（カタログ名）。
 */
key: string, 
/**
 * 変更後の値。
 */
value: SettingsValue, };
/**
 * 設定値の読み取り要求（タスク 7.1。要件 7.1）。
 *
 * 鍵は**名前の文字列**で運ぶ。`SettingsKey` は閉じた列挙であり、文字列から鍵を作る入口は
 * `SettingsKey::from_name` だけである（要件 7.7）。したがってカタログに無い名前は
 * 呼び出し先でエラー封筒の腕になり、**値の型は鍵ごとに固定しない**（値の型の閉性は
 * 7.7 の要求ではなく、要求は鍵空間の閉性である。tasks.md 4.2）。
 */
export type SettingsGetRequest = { 
/**
 * 読み取る設定の鍵（カタログ名。例 `appearance.theme`）。
 */
key: string, };
/**
 * 設定コマンドの応答（タスク 7.1。要件 4.6、7.1）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む。**呼び出し先は呼び出し元を識別でき（要件 4.6、
 * 2.1 の [`WindowContext`] を再定義せずそのまま使う）、フロントエンドも自分がどのウィンドウから
 * 呼んだかを応答から観測できる。`key` は正規化後のカタログ名、`value` は書き込み後（読み取りは
 * 現在）の値で、鍵が存在しなければ `None` である。
 */
export type SettingsResponse = { 
/**
 * 呼び出し元ウィンドウの文脈（要件 4.6）。
 */
context: WindowContext, 
/**
 * 対象の鍵（カタログ名）。
 */
key: string, 
/**
 * 現在（書き込みコマンドでは書き込み後）の値。存在しない鍵は `None`。
 */
value: SettingsValue | null, };
// 設定コマンドの応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指し
// しないため、境界が名指しできる具体形を明示的に置く。
export type SettingsResult = IpcResult<SettingsResponse, IpcError>;
/**
 * 設定値の書き込み要求（タスク 7.1。要件 7.1、7.4）。
 */
export type SettingsSetRequest = { 
/**
 * 書き込む設定の鍵（カタログ名）。
 */
key: string, 
/**
 * 書き込む値。ドキュメントの内容を指す鍵はカタログに存在しない（要件 7.7）。
 */
value: SettingsValue, };
/**
 * 境界を越える設定値（タスク 7.1。要件 7.1、7.4）。
 *
 * 設定ストアは鍵から生の JSON への写像を持つ（[`crate::settings`]）。その値を境界で型付け
 * するには、カタログの 5 鍵それぞれに固有の型を並べた列挙を持つか、JSON の形をそのまま写す
 * かのどちらかである。ここは後者を取り、**TypeScript では `unknown`** として出す
 * （`#[ts(type = "unknown")]`）。
 *
 * **`serde_json::Value` をそのまま境界へ出さない理由**は 2 つある。ts-rs の
 * `serde-json-impl` feature を有効にすると生成物が `any` になり、「生成物に `any` を混ぜない」
 * という不変条件（tasks.md 2.1、research.md 決定 1）を破る。また `i64` / `u64` を露出させる
 * 経路を型で塞いでおく必要がある（識別子は文字列とする規則）。`unknown` は受け手に絞り込みを
 * 強制するため、値の形を呼び出し側が仮定しない。
 *
 * 列挙の新定型は serde でも内側の値そのものとして直列化される（[`WindowLabel`] と同じ）。
 */
export type SettingsValue = unknown;
/**
 * ウィンドウを閉じてよいかの判定（タスク 7.6。要件 2.6）。
 *
 * ドキュメント所有者への委譲点（`src-tauri/src/ports.rs` の `CloseVerdict`）の判定を、
 * そのまま境界の形へ写したものである。`verdict` を判別子とする判別可能な合併型として
 * TypeScript へ落ちるため、フロントエンドは `verdict` で網羅的に分岐できる。
 *
 * **`Deny` は失敗ではない。**「委譲先が閉じてはならないと答えた」という正常な応答であり、
 * 封筒（[`IpcResult`]）の `status: "error"` の腕には載せない。`reason` は利用者へ伝えるための
 * 材料であり、**見せ方を決めるのは呼び出し元（フロントエンド）である**
 * （tasks.md 6.2 / 7.6。ここで文言を確定しない）。
 */
export type WindowCloseVerdict = { "verdict": "Allow" } | { "verdict": "Deny", 
/**
 * 拒否の理由。利用者へ提示するための材料であり、そのまま見せる文言とは限らない。
 */
reason: string, };
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

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
  "window_document_state",
] as const;

/**
 * 境界を越えるイベント名の一覧。`crates/app-shell/src/ipc/mod.rs` の定義と同一で、
 * フロントエンドはこの定数だけを参照する（文字列リテラルを書かない）。
 */
export const SETTINGS_CHANGED_EVENT = "settings_changed";
export const DIAGNOSTICS_REQUESTED_EVENT = "diagnostics_requested";

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
 * 書き出しに含めた記録の有無（タスク 9.5。要件 8.6）。
 *
 * 4.5 の [`crate::diagnostics::ExportReport::files_merged`] は件数を数値で持つが、**境界へ
 * 数値を出さない**（`crates/app-shell` の不変条件: 境界を越える値は文字列か、数値を含まない
 * 閉じた列挙である）。利用者にとって必要な区別は「記録を連結した」か「記録が 1 つも無かった」か
 * だけなので、件数ではなく**閉じた列挙**で運ぶ。
 */
export type DiagnosticsExportRecords = "merged" | "empty";
/**
 * 記録の書き出しの応答（タスク 9.5。要件 8.6、4.6）。
 *
 * **書き出しは 1 つのファイルにまとまる**（4.5 の [`crate::diagnostics::export`] の契約）。
 * [`DiagnosticsExportRecords::Empty`] でも成功であり、その場合も `destination` に 1 つの
 * ファイルができている（記録が無かったことを利用者へ伝えるための材料）。
 */
export type DiagnosticsExportResponse = { 
/**
 * 呼び出し元ウィンドウの文脈（要件 4.6）。
 */
context: WindowContext, 
/**
 * 書き出したファイルの位置（表示用の文字列）。
 */
destination: string, 
/**
 * 連結した記録の有無（`Empty` でも書き出しは成功している）。
 */
records: DiagnosticsExportRecords, };
// 記録の書き出しの応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を
// 名指ししないため、境界が名指しできる具体形を明示的に置く。
export type DiagnosticsExportResult = IpcResult<DiagnosticsExportResponse, IpcError>;
/**
 * 記録の詳細度（タスク 9.5。要件 8.7）。**閉じた列挙である。**
 *
 * 実体は Tauri 非依存の中核 [`crate::diagnostics::DiagnosticsLevel`] であり、この型は
 * **境界の形**である（`ts-rs` の derive を付けてよい唯一の場所が本モジュールであるという
 * 不変条件に従う。`RenderVerdict` と同じ扱い）。したがって境界の列挙と中核の列挙の間に
 * 対応付けが必要であり、それは [`From`] の 2 方向（網羅的な `match`）が担う — **どちらかの
 * 列挙に値を足すと、もう一方への写像がコンパイルエラーになる**（片側だけの追加を許さない）。
 *
 * 詳細度の昇順は [`Ord`] が表す（`Off` < `Error` < `Warn` < `Info` < `Debug` < `Trace`）。
 * 中核の列挙と同じ順序であり、[`DiagnosticsLevel::ALL`] がその閉じた集合を昇順で並べる。
 * 利用者へは [`DiagnosticsVerbosityResponse::levels`] としてこの順序で渡すので、**画面は
 * 並び順を自前で持たない**（tasks.md 4.5 の詳細度の契約）。
 *
 * 直列化は中核と同じ小文字表現（`"off"` … `"trace"`）であり、設定ファイルに載る値と
 * 境界を越える値の綴りが一致する（4.5 の `#[serde(rename_all = "lowercase")]`）。
 */
export type DiagnosticsLevel = "off" | "error" | "warn" | "info" | "debug" | "trace";
/**
 * 記録の保存場所の応答（タスク 9.5。要件 8.1、4.6）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。`directory` は各 OS の規約で解決した
 * 記録ディレクトリであり（4.4 の [`crate::diagnostics::log_dir`]）、**利用者に見せるための
 * 文字列**である（境界では識別子も位置も文字列で運ぶ。表示できないバイト列は置換される）。
 * この経路は保存場所を提示するだけで、場所を開いたり走査したりしない（要件 4.7）。
 */
export type DiagnosticsLogLocationResponse = { 
/**
 * 呼び出し元ウィンドウの文脈（要件 4.6）。
 */
context: WindowContext, 
/**
 * 記録の保存場所（表示用の文字列）。
 */
directory: string, };
// 記録の保存場所の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を
// 名指ししないため、境界が名指しできる具体形を明示的に置く。
export type DiagnosticsLogLocationResult = IpcResult<DiagnosticsLogLocationResponse, IpcError>;
/**
 * メニューの活性化を画面へ引き渡す通知（タスク 9.5）。
 *
 * メニューの処理はイベントループのスレッドで走り、対象ウィンドウのフロントエンドへ届ける
 * 必要がある。そこで 7.4 の登録口が受けた選択を、この 1 つのイベントとして**活性化の対象
 * ウィンドウへ**送る（7.5 の振り向けの結果を使う。要件 3.5）。画面はこれを購読し、遷移と
 * 区画の選択を行う。
 */
export type DiagnosticsRequestedEvent = { 
/**
 * 利用者が選んだ導線。
 */
section: DiagnosticsSection, };
/**
 * 診断の導線のうち、利用者がメニューから選んだもの（タスク 9.5。要件 8.1、8.6、8.7）。
 *
 * メニューの項目は 3 つの導線に 1 つずつ対応するので、活性化は**どれが選ばれたか**を運ぶ。
 * 画面はこの値で該当の区画を示す（利用者にとっては「選んだ項目の場所が開く」ことになる）。
 */
export type DiagnosticsSection = "location" | "export" | "verbosity";
/**
 * 記録の詳細度の応答（タスク 9.5。要件 8.7、4.6）。
 *
 * 読み取りと変更の**両方**がこの形を返す。`level` が現在の値（変更では変更後の値）であり、
 * `levels` が選べる値の全体を**詳細度の昇順**で並べたものである（[`DiagnosticsLevel::ALL`]）。
 * 画面はこの 2 つだけを見て「現在値の表示」と「選択肢の列挙」を行えるので、**選べる値の集合と
 * 順序を画面側に写さない**（写すと中核の列挙と食い違う余地ができる）。
 */
export type DiagnosticsVerbosityResponse = { 
/**
 * 呼び出し元ウィンドウの文脈（要件 4.6）。
 */
context: WindowContext, 
/**
 * 現在の詳細度（変更コマンドでは変更後の値）。
 */
level: DiagnosticsLevel, 
/**
 * 選べる詳細度の全体（`Off` から `Trace` へ昇順）。
 */
levels: Array<DiagnosticsLevel>, };
// 記録の詳細度の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を
// 名指ししないため、境界が名指しできる具体形を明示的に置く。
export type DiagnosticsVerbosityResult = IpcResult<DiagnosticsVerbosityResponse, IpcError>;
/**
 * 記録の詳細度の変更要求（タスク 9.5。要件 8.7）。
 *
 * 詳細度は**閉じた列挙 [`DiagnosticsLevel`] の値だけ**であり、任意の文字列は載らない。
 * 列挙に無い値は `serde` の復元に失敗するため、コマンドの引数として境界を越えられない
 * （その拒否はフロントエンド側のラッパが通信境界の失敗として扱う。tasks.md 2.4）。
 */
export type DiagnosticsVerbositySetRequest = { 
/**
 * 設定する詳細度。
 */
level: DiagnosticsLevel, };
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
export type IpcError = { "kind": "Settings", "detail": { message: string, } } | { "kind": "Sidecar", "detail": { message: string, } } | { "kind": "Window", "detail": { message: string, } } | { "kind": "Diagnostics", "detail": { message: string, } };
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
 * 運ぶのは**ラスタライザの文字列**と**実際に描画されていた画面の識別子**である。
 *
 * - `renderer` はフロントエンドが `WEBGL_debug_renderer_info` の
 *   `UNMASKED_RENDERER_WEBGL` から得た値である（research.md 決定 7）。取得できない環境では
 *   `null` であり、その場合も通知が届いたこと自体は描画成立の証拠になる（中核の写像を参照）。
 * - `screen` は通知を送る時点で**シェルの領域が実際に表示していた画面**の識別子である
 *   （`src/shell/Layout.tsx` が領域の要素に書く `data-shell-screen`。tasks.md 9.1 の契約）。
 *   3 OS の描画確認（tasks.md 10.4）が「**どの画面が描画されたか**」をこの 1 つの記録から
 *   読めるようにするために載せる（`src/shell/renderHeartbeat.ts` のモジュール doc を参照）。
 *   **要求した識別子ではなく、描画された識別子であること**が要点である — 起動時に要求した
 *   画面が未登録なら、シェルは既定の初期画面へ落ちるため、両者は一致しない（9.7 の契約）。
 *   領域を読めなかったときは `null`（「報告なし」として扱われ、描画の証明にはならない）。
 *
 * **ウィンドウは運ばない。** 呼び出し元は Tauri が注入する `WebviewWindow` から取るため、
 * フロントエンドがウィンドウを偽装する経路は存在しない（要件 4.6、tasks.md 7.1）。
 */
export type RenderHeartbeatRequest = { 
/**
 * ラスタライザの文字列。取得できなければ `null`。
 */
renderer: string | null, 
/**
 * 通知の時点で**実際に描画されていた画面の識別子**。読めなければ `null`。
 */
screen: string | null, };
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
 * ウィンドウとドキュメントの関連付けの状態（タスク 9.6。要件 2.1、2.2）。**閉じた列挙である。**
 *
 * 要件 2.2 が操作の導線を提示する対象は「ドキュメントを関連付けていないウィンドウ」であり、
 * その判定はウィンドウの生成時に確定した関連付け（レジストリの写像）から取る。**ラベルの
 * 接頭辞（`empty-` / `doc-`）からは判定しない** — 接頭辞は割り当て順の規約であって関連付けの
 * 事実ではなく、`attach` は記録された関連付けを書き換えないため両者が食い違いうる
 * （9.6 の画面のモジュール doc を参照）。
 *
 * **パスは運ばない。** 問いは関連付けの有無だけであり、どのドキュメントかは所有者
 * （下流スペック）が持つ（[`DocumentPickOutcome`] と同じ方針）。
 */
export type WindowDocumentState = "unassociated" | "associated";
/**
 * 関連付けの問い合わせの応答（タスク 9.6。要件 2.2、4.6）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む。**呼び出し元は Tauri が注入する `WebviewWindow`
 * から得るので、**フロントエンドがウィンドウの識別子を payload で申告する経路は存在しない**
 * （偽装できない。要件 4.6、tasks.md 7.1）。**要求の型は無い**（操作対象のウィンドウだけが
 * 入力であり、それは基盤が注入する。[`PickDocumentFileResponse`] と同じ形）。
 */
export type WindowDocumentStateResponse = { 
/**
 * 呼び出し元ウィンドウの文脈（要件 4.6）。
 */
context: WindowContext, 
/**
 * 関連付けの状態（要件 2.1、2.2）。
 */
state: WindowDocumentState, };
// 関連付けの問い合わせの応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を
// 名指ししないため、境界が名指しできる具体形を明示的に置く。
export type WindowDocumentStateResult = IpcResult<WindowDocumentStateResponse, IpcError>;
/**
 * 境界を越えるウィンドウの識別子（要件 4.2、4.6）。
 *
 * 境界を越える識別子は 64 ビット整数をそのまま公開せず、文字列表現とする。JavaScript の
 * `number` は IEEE 754 の倍精度であり、`i64` / `u64` の全域を正確に表せないためである。
 * TypeScript 側の型も `string` に固定する（design.md「IpcContract」の不変条件）。
 */
export type WindowLabel = string;

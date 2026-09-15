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
  "document_state",
  "document_save",
  "document_new",
  "document_discard",
  "grid_open_sheet",
  "grid_set_view",
  "grid_apply_edit",
  "grid_history",
  "grid_find_violation",
  "grid_rows_window",
] as const;

/**
 * 境界を越えるイベント名の一覧。`crates/app-shell/src/ipc/mod.rs` の定義と同一で、
 * フロントエンドはこの定数だけを参照する（文字列リテラルを書かない）。
 */
export const SETTINGS_CHANGED_EVENT = "settings_changed";
export const DIAGNOSTICS_REQUESTED_EVENT = "diagnostics_requested";
export const DOCUMENT_SESSION_CHANGED_EVENT = "document_session_changed";

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
 * 構成の 1 列: 窓が運ぶ列そのものであり、どの値が載るかを指す（タスク 6.1。要件 1.1、1.2、
 * 3.1、5.1、5.4、5.6）。
 *
 * `view` 層の `LayoutColumn` を写したものであり、6.1 の消費側（描画側と入力手段の登録簿）が
 * 必要とするものを全部運ぶ — 列の添字・内側の位置・表示名・葉の型の札・要素数の能力・
 * 展開の可否である。`design.md`「GridSession」の Implementation Notes が「6.1 はここから
 * 境界型へ写す」と定めているのはこの型である。
 *
 * **列の同一性は（[`ColumnDescriptor::column`], [`ColumnDescriptor::path`]）の対であり、
 * 名前ではない** — 表示名は人が読むためのものであり、送る先を決めるのは位置である
 * （ドメインの `LayoutColumn` の docs）。折りたたんだ列は位置が空であり、展開された列は
 * 位置が 1 段以上である。
 *
 * `kind` が `None` であるのは、その列が**使用できない**（宣言が壊れている）場合である。
 * このとき入力手段の登録簿は既定の入力へ落ちる（10.4）。
 */
export type ColumnDescriptor = { 
/**
 * 最上位の列の添字（0 起点。内側の位置も同じ最上位の列を指す）。
 */
column: number, 
/**
 * 内側の位置（空 = セル直下）。
 */
path: Array<GridPathSegment>, 
/**
 * 表示名（位置に沿ったフィールド名を `.` で連結したもの）。
 */
name: string, 
/**
 * 葉の型の札（3.2 と 7.4 が入力手段を選ぶのに使う）。使用不能な列は `None`。
 */
kind: TypeKindTag | null, 
/**
 * 同一の型の並びの要素数の能力（要件 5.6）。配列でなければ `None`。
 */
element_count: ColumnElementCount | null, 
/**
 * 展開の可否と、上限に達したことの印（要件 5.4）。
 */
expandability: ColumnExpandability, };
/**
 * 同一の型の並び（配列）の要素数の能力（タスク 6.1。要件 5.6）。
 *
 * `view` 層の `ElementCount` を写したもので、要素の型の札と、**列に宣言された**要素数の
 * 上下限を持つ。**`None` は開いた端点**であり、宣言が無いことと上下限が 0 であることは違う
 * （ドメインの `ElementCount` の docs。`0..=0` は空の並びであり、宣言の無い並びではない）。
 */
export type ColumnElementCount = { 
/**
 * 要素の型の札（配列の `items` の種別）。
 */
items: TypeKindTag, 
/**
 * 要素数の下限（`minItems`）。未宣言は `None`（開いた下限）。
 */
min: number | null, 
/**
 * 要素数の上限（`maxItems`）。未宣言は `None`（開いた上限）。
 */
max: number | null, };
/**
 * 列を展開できるか、詳細の表示へ委ねるかの札（タスク 6.1。要件 5.1、5.2、5.4）。
 *
 * `view` 層の `Expandability` を写した**閉じた種類の列挙**である（小文字へ落とすのは
 * [`super::DocumentOrigin`] と同じ扱いである）。**3 値であることが要点であり、2 つの真偽へ
 * 潰さない** — 「内側を持たない」と「段数の上限に達した」は別の事実であり、潰すと展開の指定が
 * 上限の手前で止まっている列にも「詳細の表示へ」が出る（ドメインの `Expandability` の docs）。
 *
 * 消費側が要る 2 つの事実（展開できるか・詳細の表示へ委ねるか）は
 * [`ColumnDescriptor::is_expandable`] と [`ColumnDescriptor::requires_detail`] がこの札から
 * 導く — ドメインの `LayoutColumn` の同名のメソッドと同じ判断であり、写しを二重に持たない。
 */
export type ColumnExpandability = "available" | "capped" | "leaf";
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
 * 破棄の印の応答（タスク 3.1。要件 6.5）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。破棄の印は失敗しうる操作ではない
 * （未保存を落とすだけである）ため、結果の型を持たず、結果の状態だけを運ぶ。
 */
export type DocumentDiscardResponse = { 
/**
 * 呼び出し元ウィンドウの文脈。
 */
context: WindowContext, 
/**
 * 破棄の印のあとのセッションの状態。
 */
status: DocumentSessionStatus, };
// 破棄の印の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指し
// しないため、境界が名指しできる具体形を明示的に置く。
export type DocumentDiscardResult = IpcResult<DocumentDiscardResponse, IpcError>;
/**
 * 新規作成の指示の結果（タスク 3.1。要件 7.1、7.3）。
 *
 * [`DocumentSaveOutcome`] と同じく、正常な結果を封筒の成功腕に載せる。**`Refused` は失敗では
 * ない** — 未保存の変更があるため作成できなかったという、ドメインの側の正しい答えであり、
 * 利用者へ伝えるための理由（`reason`）を運ぶ（要件 7.3）。
 */
export type DocumentNewOutcome = { "outcome": "Created" } | { "outcome": "Refused", 
/**
 * 拒否の理由。
 */
reason: string, };
/**
 * 新規作成の応答（タスク 3.1。要件 4.6、7.1）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。[`DocumentSaveResponse`] と同じ形で、
 * `status` は作成のあとの状態、`outcome` はこの指示の結果である。
 */
export type DocumentNewResponse = { 
/**
 * 呼び出し元ウィンドウの文脈。
 */
context: WindowContext, 
/**
 * 作成のあとのセッションの状態。
 */
status: DocumentSessionStatus, 
/**
 * この新規作成の指示の結果。
 */
outcome: DocumentNewOutcome, };
// 新規作成の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指し
// しないため、境界が名指しできる具体形を明示的に置く。
export type DocumentNewResult = IpcResult<DocumentNewResponse, IpcError>;
/**
 * ドキュメントの出所（タスク 3.1。要件 1.6、7.1）。
 *
 * **位置は運ばない。** 出所がファイルであることは判別できるが、そのファイルがどこにあるかは
 * 境界を越えない — 位置を知るのはドメインの側（`document-session`）だけでよい。新規の
 * ドキュメントは保存先を持たないため、保存の指示で保存先の選択を要する（要件 5.2）。
 *
 * 綴りは小文字（`"file"` / `"new"`）であり、既存の閉じた列挙
 * （[`super::WindowDocumentState`]）と同じ `#[serde(rename_all = "lowercase")]` に従う。
 */
export type DocumentOrigin = "file" | "new";
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
 * 保存の指示の結果（タスク 3.1。要件 5.1、5.3、5.4）。
 *
 * **`Cancelled` は失敗ではない。** 利用者が保存先の選択を取り消したという正常な結果であり、
 * 封筒の `status: "error"` の腕には載せない（`design.md`「Error Handling」。
 * [`super::DocumentPickOutcome::Cancelled`] と同じ判断）。`Failed` は書き出せなかったことを
 * 意味し、利用者へ伝えるための理由（`reason`）を運ぶ。**失敗と取り消しでは未保存が保たれる。**
 */
export type DocumentSaveOutcome = { "outcome": "Saved" } | { "outcome": "Cancelled" } | { "outcome": "Failed", 
/**
 * 書き出せなかった理由。
 */
reason: string, };
/**
 * 保存の応答（タスク 3.1。要件 4.6、5.1）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。`status` は保存のあとのセッションの
 * 状態であり、`outcome` はこの指示の結果である。**未保存が落ちたかどうかは `status` の
 * [`DocumentSummary::unsaved`] を読めば分かる**ので、結果の型に重複して持たない。
 */
export type DocumentSaveResponse = { 
/**
 * 呼び出し元ウィンドウの文脈。
 */
context: WindowContext, 
/**
 * 保存のあとのセッションの状態。
 */
status: DocumentSessionStatus, 
/**
 * この保存の指示の結果。
 */
outcome: DocumentSaveOutcome, };
// 保存の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指ししない
// ため、境界が名指しできる具体形を明示的に置く。
export type DocumentSaveResult = IpcResult<DocumentSaveResponse, IpcError>;
/**
 * あるウィンドウのドキュメントのセッションの状態（タスク 3.1。要件 1.6、1.7、2.1）。
 *
 * `state` を判別子とする判別可能な合併型であり、フロントエンドは `switch` で網羅的に分岐
 * できる（`assertNever` が新しい変種をコンパイルエラーにする。`src/ipc/client.ts`）。
 * タグの語は「どの状態か」を表す `state` であり、封筒の `status`（成功か失敗か）とは別の
 * 判別子である。
 *
 * **`Unavailable` は封筒の失敗ではない。** ドキュメントを読み込めなかったという**ドメインの
 * 結果**であり、コマンドは正常に答えた。したがって [`super::IpcError`] の腕には載せず、
 * 成功の腕（[`DocumentStateResponse`]）がこの状態として運ぶ（`design.md`「Error Handling」）。
 * 理由の文言は適応層が組み立てたものをそのまま運び、見せ方を決めるのは呼び出し元である。
 *
 * **判別子の語は `state` であり、値は変種名のまま（`"Absent"` / `"Open"` / `"Unavailable"`）
 * である。** 本列挙には `#[serde(rename_all = "lowercase")]` を付けない — 同じく判別可能な
 * 合併型である [`super::WindowCloseVerdict`] / [`super::DocumentPickOutcome`] と揃える
 * （小文字へ落とすのは [`DocumentOrigin`] のような閉じた「種類」の列挙だけである）。
 * 生成物（`src/ipc/bindings.ts`）は
 * `{ "state": "Absent" } | { "state": "Open" } & DocumentSummary | …` となり、
 * 分岐を書く側は `case "Absent":` のように**変種名のまま**照合しなければ絞り込みが効かない。
 */
export type DocumentSessionStatus = { "state": "Absent" } | { "state": "Open" } & DocumentSummary | { "state": "Unavailable", 
/**
 * 読み込めなかった理由。
 */
reason: string, };
/**
 * 保持しているドキュメントのシートの要約（タスク 3.1。要件 1.7）。
 *
 * 識別子は**文字列**、件数（列数・行数）は [`u32`] で運ぶ。どちらも境界の規約
 * （64 ビット整数を出さない・他のドメインクレートの型を参照しない）に従った結果である。
 * `document-format` の `SheetId` / `usize` をそのまま出さない — 境界の型はドメインの型に
 * 依存せず、写すのは適応層の仕事である。
 */
export type DocumentSheet = { 
/**
 * シートの識別子（文字列表現）。
 */
id: string, 
/**
 * 利用者に見せるシートの名前。
 */
name: string, 
/**
 * 列数。
 */
columns: number, 
/**
 * 行数。
 */
rows: number, };
/**
 * セッションの状態の問い合わせの応答（タスク 3.1。要件 1.6、1.7）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。要求の型は無い — 必要な入力は操作の
 * 対象ウィンドウだけで、それは基盤が注入する（[`super::CanCloseWindowResponse`] と同じ形）。
 */
export type DocumentStateResponse = { 
/**
 * 呼び出し元ウィンドウの文脈。
 */
context: WindowContext, 
/**
 * そのウィンドウのセッションの状態。
 */
status: DocumentSessionStatus, };
// セッションの状態の問い合わせの応答の具体形。ジェネリックな `IpcResult` の宣言は
// ペイロード型を名指ししないため、境界が名指しできる具体形を明示的に置く。
export type DocumentStateResult = IpcResult<DocumentStateResponse, IpcError>;
/**
 * 保持しているドキュメントの要約（タスク 3.1。要件 1.6、1.7）。
 *
 * 要件 1.6 が求める「開いているドキュメントの名前」と「未保存の変更があるかどうか」、および
 * 要件 1.7 が求める「シートの一覧」を 1 つの形にまとめる。**名前はファイル名のみ**であり
 * （位置は境界を越えない）、新規のドキュメントは空文字で運ぶ。
 */
export type DocumentSummary = { 
/**
 * ファイル名のみ。新規のドキュメントは空文字。
 */
name: string, 
/**
 * 出所。
 */
origin: DocumentOrigin, 
/**
 * 未保存の変更があるかどうか（要件 4.3）。
 */
unsaved: boolean, 
/**
 * 保持しているシートの一覧（要件 1.7）。
 */
sheets: Array<DocumentSheet>, };
/**
 * 物理のセルの位置（タスク 6.1。要件 3.3、8.6、8.9）。
 *
 * `types` 層の `CellAddress` を写したもので、**行の識別子（文字列）と列の添字（`u32`）**を
 * 持つ。**可視行の序数ではない** — 絞り込みや並べ替えの下では表示の位置と一致せず、取り違えると
 * 別の行を編集する（要件 8.6）。表示の座標からこの位置への写像を持つのは、順序を持つ側
 * （`src-tauri` の適応層と画面）である。
 */
export type GridCellAddress = { 
/**
 * 行の識別子（文字列表現。64 ビット整数を境界へ出さない）。
 */
row: string, 
/**
 * 列の添字（0 起点）。
 */
column: number, };
/**
 * 1 つのセルへ書く、打たれた文字（タスク 6.1。要件 3.3、3.5、7.3）。
 *
 * `edit` 層の `EditCommand::SetCells` は `Vec<(CellAddress, String)>` を持つが、境界では
 * **名前のある欄に分ける** — 位置と文字の 2 つ組は、生成物（`src/ipc/bindings.ts`）で
 * `[GridCellAddress, string]` という無名の並びになり、読み手に意味を伝えないためである。
 *
 * `text` は**打たれた文字そのもの**であり、型の解釈は `schema-engine` が行う（要件 3.3）。
 * 適合しない値も破棄せずに保持し、違反として報告する（要件 3.5）。
 */
export type GridCellEdit = { 
/**
 * 書くセル。
 */
cell: GridCellAddress, 
/**
 * そこへ打たれた文字。
 */
text: string, };
/**
 * 型強制によって値が変換されたことの記録（タスク 6.1。要件 3.4）。
 *
 * `edit` 層の `CoercionNotice` を写したもので、変換の**前と後**の双方を表示文字列として
 * 持つ。「変換が起きたこと」と「変換前の値」を人が確認できる形にするためである（要件 3.4）。
 * 表示文字列の写しはドメインが 1 つだけ持ち、本型はその写しを運ぶ。
 */
export type GridCoercionNotice = { 
/**
 * 変換が起きたセルの位置。
 */
cell: GridCellAddress, 
/**
 * 変換**前**の値の表示文字列（打たれた文字そのもの）。
 */
before: string, 
/**
 * 変換**後**の値の表示文字列（ドキュメントへ書かれた値）。
 */
after: string, };
/**
 * 編集命令（タスク 6.1。要件 3.3、5.7、6.1、6.2、6.3、7.3、7.4、8.9）。
 *
 * `edit` 層の `EditCommand` を写したもので、**6 つの命令を過不足なく持つ**。命令の意味は
 * ドメインの同名の変種と同じであり、本型は解釈を持たない（適用するのは `data-grid`）。
 *
 * **値を型付きで運ばない。** 値を運ぶ 3 つの命令は、いずれも文字列を運ぶ —
 * `SetCells` は打たれた文字、`SetNested` はセル値の**構造表現（JSON）**、`PasteRange` は
 * **表形式テキスト**である（要件 7.2。`document-format` の `CellValue` を写した型は境界に
 * 無い）。**行の構造を変える 3 つの命令は値を運ばない** — 挿入する行の値は宣言が供給し、
 * 複製する行の値はドメインが写す（要件 6.1、6.3）。
 *
 * 挿入の位置（`InsertRows::at`）は**文書の行順に対する位置**であり、可視の序数ではない
 * （画面の位置に挿入したい呼び出し側は、順序を持つ側で行そのものへ写してからその行の位置を
 * 渡す）。貼り付けは起点と**表示されている行の並び**の 2 つで宛先が決まる（要件 8.9）ため、
 * `PasteRange::rows` を持つ。
 */
export type GridEditCommand = { "command": "SetCells", 
/**
 * 書くセルと、そこへ打たれた文字。同じセルが複数回現れた場合は**後ろのものが残る**
 * （ドメインの `Document::set_cells` の契約）。
 */
cells: Array<GridCellEdit>, } | { "command": "SetNested", 
/**
 * 書くセル（物理の位置）。
 */
cell: GridCellAddress, 
/**
 * そのセルへ書く値の構造表現。
 */
json: string, } | { "command": "InsertRows", 
/**
 * 挿入する文書の位置（適用前の行順に対する添字。行数までの値が妥当）。
 */
at: number, 
/**
 * 挿入する行数。`0` は何も変えない。
 */
count: number, } | { "command": "RemoveRows", 
/**
 * 取り除く行の識別子。空なら何も変えない。
 */
rows: Array<string>, } | { "command": "DuplicateRows", 
/**
 * 複製する元の行の識別子。空なら何も変えない。
 */
rows: Array<string>, } | { "command": "PasteRange", 
/**
 * 貼り付けの起点（**物理の行**と列。要件 8.6）。
 */
anchor: GridCellAddress, 
/**
 * **表示されている行の並び**（順序を持つ側が導出したもの。要件 8.9）。
 *
 * 貼り付けは錨の行がこの並びに現れる位置から歩くため、**隠れている行には 1 セルも
 * 書かれない**。空なら何も書かない。
 */
rows: Array<string>, 
/**
 * 貼り付ける表形式テキスト（行の区切りと列の区切りを持つ。要件 7.2）。
 */
text: string, };
/**
 * 編集を適用した結果の要約（タスク 6.1。要件 3.4、4.3、4.6、6.2、6.4、7.5）。
 *
 * `edit` 層の `EditOutcome` を写したものであり、**判定が返したものと、画面が直ちに要るもの**
 * だけを運ぶ。運ぶ欄は次のとおりである。
 *
 * - `affected` — 影響を受けた行（重複を畳み、命令に現れた順）。画面はこの行の窓を捨てる
 *   （要件 1.7）ために使う。
 * - `coercions` — 型強制の記録（要件 3.4）。変換が起きなければ空である。
 * - `violation_total` — **シート全体**の違反の総数（`u32`。`usize` を境界へ出さない）。
 *   適応層がドメインの `GridSession::violation_total()`（差分的に最新へ保たれる）から写す。
 *   **ドメインの `EditOutcome::violation_total`（再検証した列に閉じる）とは別物である** —
 *   適応層が写し替えるのはそのためである（要件 4.3）。
 * - `violations` — 適用のあとに再検証した列が持つ違反（重複なし、報告の順）。
 *   **範囲は `revalidated_columns` と同じであり、`violation_total` とは別である**（総数は
 *   シート全体、この一覧は再検証した列に閉じる）。画面はこれで**変わった違反だけ**を
 *   受け取れる（要件 4.6）。
 * - `revalidated_columns` — 適用のあとに再検証した列の添字（昇順・重複なし）。空の命令では
 *   空であり、そのとき違反も 0 件である。
 * - `row_count` — 適用の**後**のシートの行数。行を足す・取り除く命令がこれを変える
 *   （要件 6.2 が提示する行数の変化）。
 *
 * 本型は解釈を持たない — 適用したのは `data-grid` であり、判定をしたのは `schema-engine`
 * である。境界はそれらの結果を写すだけである。
 */
export type GridEditOutcome = { 
/**
 * 影響を受けた行の識別子（重複を畳み、命令に現れた順）。
 */
affected: Array<string>, 
/**
 * 型強制によって値が変換されたセル。変換が起きなければ空である。
 */
coercions: Array<GridCoercionNotice>, 
/**
 * **シート全体**の違反の総数（要件 4.3）。
 *
 * **再検証した列に閉じない。**適応層が `GridSession::violation_total()`（差分的に最新へ
 * 保たれるシート全体の数）から写す。**再検証した列に閉じるのは [`Self::violations`] の
 * 一覧のほうである** — こちらは適用のあとに列の検証をやり直した結果であり、総数ではない。
 * この欄の**文言**は、総数を列に閉じるとしていた頃のドメインの `EditOutcome` の doc を
 * 引き写したものである（この境界型自体は 6.1 が書いており、その版の doc は
 * `再検証した列に閉じた総数` と述べていた）。同種の記述は 8.3 の作業中に画面の module の
 * **注記**にも現れていたため、どちらも正した（8.3 のレビューが指摘）。
 */
violation_total: number, 
/**
 * 適用のあとに再検証した列が持つ違反の一覧（重複なし、報告の順）。
 */
violations: Array<GridViolationLocation>, 
/**
 * 適用のあとに再検証した列の添字（昇順・重複なし）。
 */
revalidated_columns: Array<number>, 
/**
 * 適用の後のシートの行数。
 */
row_count: number, };
/**
 * 編集を適用する要求（タスク 6.2。要件 3.3、5.7、6.1、7.3）。
 *
 * 運ぶのは編集命令 1 つである（[`GridEditCommand`] の 6 つの命令）。**値を型付きで運ばない**
 * 規約は 6.1 の型が既に守っている。ウィンドウは要求の型に現れない（要件 4.6）。
 */
export type GridEditRequest = { 
/**
 * 適用する編集命令。
 */
command: GridEditCommand, };
/**
 * 編集の結果の要約を運ぶ応答（タスク 6.2。要件 3.4、4.6、9.2、9.3）。
 *
 * **適用（[`GridEditRequest`]）と履歴（[`GridHistoryRequest`]）が同じ形を返す。**
 * 履歴を進めることも「1 つの命令がドキュメントへ適用された」ことであり、画面が要るもの
 * （影響範囲・変換・違反・行数）は同じだからである（`design.md`「GridCommands」の API
 * Contract が `grid_apply_edit` と `grid_history` の応答を同じ型と定めている）。
 *
 * **`outcome` が `None` であるのは「進める履歴が無かった」場合だけである**（要件 9.2、9.3）。
 * 取り消し・やり直しの対象が空のときに何も変えずに答える正常な結果であり、封筒の失敗腕には
 * 載せない — 「直前の操作が無い」ことは利用者の操作が失敗したことではない。適用
 * （[`GridEditRequest`]）ではつねに `Some` である。
 */
export type GridEditResponse = { 
/**
 * 呼び出し元ウィンドウの文脈。
 */
context: WindowContext, 
/**
 * 適用された操作の要約。`None` は「進める履歴が無く、何も変わらなかった」（要件 9.2、9.3）。
 */
outcome: GridEditOutcome | null, };
// 編集の結果の応答の具体形（適用と履歴で同じ形）。ジェネリックな `IpcResult` の宣言は
// ペイロード型を名指ししないため、境界が名指しできる具体形を明示的に置く。
export type GridEditResult = IpcResult<GridEditResponse, IpcError>;
/**
 * 入れ子の展開の状態 1 列ぶん（タスク 6.1。要件 5.1、5.2、5.3、5.4）。
 *
 * `view` 層の `ExpansionState` を写したものである。展開は**表示状態の一部**であり
 * （要件 5.3）、走査や並べ替えでは失われない。`depth` は表示する段数であり、上限
 * （`design.md`「表示状態」の `MAX_EXPANSION_DEPTH`）に達した列は
 * [`ColumnExpandability::Capped`] として現れる（要件 5.4）。
 *
 * 段数は `u8` である — ドメインと同じ幅にしておき、上限を越える指定が境界で復元に失敗する
 * ようにする（`u32` に広げると、通ってから拒否する経路ができる）。
 */
export type GridExpansionState = { 
/**
 * 対象の列の添字（0 起点）。
 */
column: number, 
/**
 * 展開しているか（偽は折りたたみの指定である。要件 5.2）。
 */
expanded: boolean, 
/**
 * 表示する入れ子の段数（要件 5.4）。
 */
depth: number, };
/**
 * 絞り込みの条件 1 本（タスク 6.1。要件 8.4）。
 *
 * `view` 層の `FilterSpec` を写したもので、**設計が固定する 5 条件**（一致・部分一致・値なし・
 * 値あり・違反あり）を過不足なく持つ。複数与えられた場合は**積**として働く
 * （[`GridViewSpec::filters`]）。
 *
 * **`HasViolation` は画面が要求できる。** 「違反あり」で絞り込む導線（要件 8.4）はこの条件
 * だけで成立し、列を問わない指定（`column: None`）と列を指定した要求を**別の要求として
 * 区別する**。
 *
 * `Equals` / `Contains` が比較するのは**値ではなく表示文字列**である（ドメインの
 * `FilterSpec` の docs）。境界は値を型付きで運ばないため、比較の対象は文字列で足りる。
 */
export type GridFilterSpec = { "filter": "Equals", 
/**
 * 対象の列の添字（0 起点）。
 */
column: number, 
/**
 * 一致させる表示文字列（そのまま比較する）。
 */
text: string, } | { "filter": "Contains", 
/**
 * 対象の列の添字（0 起点）。
 */
column: number, 
/**
 * 含まれることを求める表示文字列（空文字は全行に一致する）。
 */
text: string, } | { "filter": "IsEmpty", 
/**
 * 対象の列の添字（0 起点）。
 */
column: number, } | { "filter": "IsNotEmpty", 
/**
 * 対象の列の添字（0 起点）。
 */
column: number, } | { "filter": "HasViolation", 
/**
 * 対象の列。`None` は**列を問わない**（その行に違反が 1 つでもあれば選ぶ）。
 */
column: number | null, };
/**
 * 履歴を進める向き（タスク 6.2。要件 9.2、9.3）。**閉じた列挙である。**
 *
 * 「取り消し」と「やり直し」は利用者の別々の指示であり、1 つの真偽へ潰さない
 * （潰すと生成物のフロントエンドで意味が読めない）。
 */
export type GridHistoryDirection = "undo" | "redo";
/**
 * 履歴を進める要求（タスク 6.2。要件 9.2、9.3）。
 *
 * **どちらへ進めるかを要求が言う。** 取り消しとやり直しは同じ経路（履歴と適用を束ねた口）
 * を通るが、進める向きは要求が決める。ウィンドウは要求の型に現れない（要件 4.6）。
 */
export type GridHistoryRequest = { 
/**
 * 進める向き。
 */
direction: GridHistoryDirection, };
/**
 * 表示するシートを開く要求（タスク 6.2。要件 1.1、1.5、1.6）。
 *
 * **シートは識別子の文字列で選ぶ。** 1 つのドキュメントは複数のシートを持ちうるが
 * （要件 1.7 の [`super::DocumentSheet`]）、表示する対象を選ぶ手段は本機能の外にあり
 * （`design.md`「Out of Boundary」）、境界を越える識別子は文字列である（64 ビット整数を
 * 出さない規約）。呼び出し側は [`super::DocumentStateResponse`] が運ぶシートの一覧の
 * `id` をそのまま渡す。
 */
export type GridOpenRequest = { 
/**
 * 表示するシートの識別子（[`super::DocumentSheet::id`] の文字列そのもの）。
 */
sheet: string, };
/**
 * シートを開いた応答（タスク 6.2。要件 1.1、1.5、1.6、4.6）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。`sheet` は窓が運ぶ列の構成と
 * シートの行数であり、**2 つの空の状態（列が 1 本も無い・列はあるが行が無い）を形の上で
 * 区別する**（[`GridSheetSummary`] の doc を参照）。
 *
 * **表示の指定はここに無い。** 絞り込み・並べ替え・展開は [`GridViewResponse`] が運ぶ。
 */
export type GridOpenResponse = { 
/**
 * 呼び出し元ウィンドウの文脈。
 */
context: WindowContext, 
/**
 * 開いたシートの要約（列の構成と行数）。
 */
sheet: GridSheetSummary, };
// シートを開いた応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を
// 名指ししないため、境界が名指しできる具体形を明示的に置く。
export type GridOpenResult = IpcResult<GridOpenResponse, IpcError>;
/**
 * 入れ子の内側の位置の 1 段（タスク 6.1。要件 4.5、5.5）。
 *
 * `types` 層の `NestedPathSegment` を写したもので、**フィールド名と配列の位置を区別する**。
 * 区別を潰すと、要件 4.5 が求める「入れ子のどの位置が違反しているか」を表示するときに
 * `a.b` と `a[1]` を書き分けられない。
 *
 * 段の並び（[`ColumnDescriptor::path`] / [`GridViolationLocation::path`]）は、**空なら
 * セル直下**を指す（ドメインの `NestedPath` と同じ規約）。配列の位置は `u32` である
 * （`usize` を境界へ出さない）。
 */
export type GridPathSegment = { "segment": "Field", 
/**
 * フィールド名。
 */
name: string, } | { "segment": "Index", 
/**
 * 要素の位置。
 */
position: number, };
/**
 * 違反を探す向き（タスク 6.2。要件 4.4）。**閉じた列挙である。**
 *
 * **可視行の序数が増える向き**が [`GridSearchDirection::Forward`] である（`data-grid` の
 * `SearchDirection` と同じ意味）。名前を `prev` / `next` にしないのは、向きが可視の順序に
 * 対して定義されており、画面の「前へ」が文書の順序と一致しないためである。
 */
export type GridSearchDirection = "forward" | "backward";
/**
 * シートの要約: 窓が運ぶ列の構成と、シートの行数（タスク 6.1。要件 1.1、1.5、1.6）。
 *
 * **これは封筒ではない。** `status` も呼び出し元ウィンドウの文脈も持たない、応答の内側の荷で
 * ある（`design.md`「GridCommands」の `grid_open_sheet` の応答は 6.2 が組み立て、その荷として
 * 本型を使う）。
 *
 * # 2 つの空の状態（要件 1.5、1.6）
 *
 * **列の数が 2 つを区別する。** 行数は両者を区別しない（列が 1 本も宣言されていないシートも、
 * 列はあるが行が 1 件も無いシートも、行数は 0 件でありうる）ため、**列の並びと行数を同じ型に
 * 載せる**ことが要件である。
 *
 * | 状態 | 形 | 画面の振る舞い |
 * |---|---|---|
 * | 列が 1 本も宣言されていない（要件 1.6） | `columns` が空 | 表を描かず、スキーマが定義されていないことを示す |
 * | 列はあるが行が 1 件も無い（要件 1.5） | `columns` が非空かつ `row_count == 0` | 列の構成を提示したうえで、行が無いことを示す |
 * | 通常 | `columns` が非空かつ `row_count > 0` | 表を描く |
 *
 * 2 つの状態の判定は [`GridSheetSummary::has_no_columns`] /
 * [`GridSheetSummary::has_columns_but_no_rows`] が行う。**画面が自前で書かない**のは、
 * 「列の数で区別する」という規則を 1 箇所に閉じるためである。
 *
 * # 行数は「シートの行数」であり、可視行数ではない
 *
 * 絞り込みで可視の行が 0 件になった状態（要件 8.7）を要件 1.5 と混同してはならない —
 * 前者は**行が在って隠れている**のであり、後者は**行が無い**。可視行数と隠された行数は表示の
 * 指定の結果であり、6.2 の `GridViewResponse` が別に運ぶ（本型は表示の指定を知らない）。
 */
export type GridSheetSummary = { 
/**
 * 窓が運ぶ列の構成（左から右への表示順。入れ子の展開を含む）。
 */
columns: Array<ColumnDescriptor>, 
/**
 * シートの行数（絞り込みの結果ではない。要件 1.5）。
 */
row_count: number, };
/**
 * 並べ替えの基準列 1 本（タスク 6.1。要件 8.3）。
 *
 * `view` 層の `SortKey` を写したものである。列の添字は `Row::values()` に対する位置であり、
 * シートの列名の並びと同じ添字である。`descending` は**その基準列の比較だけ**を反転する
 * （同値の行の決着は反転しない。ドメインの `SortKey` の docs）。
 */
export type GridSortKey = { 
/**
 * 基準となる列の添字（0 起点。要件 8.3）。
 */
column: number, 
/**
 * この基準列を降順で並べるか。
 */
descending: boolean, };
/**
 * 表示の指定を変える要求（タスク 6.2。要件 8.3、8.4）。
 *
 * **指定は完全な記述である。** 空の [`GridViewSpec`] は「絞り込み無し・並べ替え無し・
 * 展開無し」を意味する（6.1 の doc と同じ規約）ので、前の指定のうちここに現れないものは
 * 適用されない。ウィンドウは要求の型に現れない（呼び出し元は基盤が注入する。
 * 要件 4.6）。
 */
export type GridViewRequest = { 
/**
 * 適用する表示の指定。
 */
view: GridViewSpec, };
/**
 * 表示の指定を変えた応答（タスク 6.2。要件 8.5、8.7、4.3、4.6）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。可視行数と隠された行数を運ぶのは
 * 要件 8.7（絞り込みで表示されていない行の数を提示する）である。**要件 1.5 の「行が無い」
 * とは別である** — あちらは行そのものが無い状態であり、[`GridOpenResponse::sheet`] の
 * `row_count` が表す。ここが運ぶのは**行が在って隠れている**数である。
 *
 * `violation_total` は**シート全体の違反の総数**である（要件 4.3。絞り込みに依らない）。
 */
export type GridViewResponse = { 
/**
 * 呼び出し元ウィンドウの文脈。
 */
context: WindowContext, 
/**
 * 表示の指定を適用したあとの可視行数（要件 8.7）。
 */
visible_rows: number, 
/**
 * 絞り込みによって表示されていない行数（要件 8.7）。
 */
hidden_rows: number, 
/**
 * 表示中のシートに存在する違反の総数（要件 4.3）。
 */
violation_total: number, };
// 表示の指定を変えた応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を
// 名指ししないため、境界が名指しできる具体形を明示的に置く。
export type GridViewResult = IpcResult<GridViewResponse, IpcError>;
/**
 * 表示の指定: 行の並びと列の構成をどう導出するか（タスク 6.1。要件 5.3、8.3、8.4）。
 *
 * **並べ替え・絞り込み・展開を 1 つの形にまとめる。** `view` 層では表示状態が
 * `ViewSpec`（並べ替えと絞り込み）と `ExpansionState` の並び（展開）に割れているが、
 * 境界では 1 つにする — 3 つとも**窓が運ぶ行と列を変える**ものであり（`design.md`
 * 「表示状態」の割り方の根拠）、要求の口は `grid_set_view` 1 つだからである。
 * 列幅と表示上の列順は**ここに無い** — あれらは窓の中身を変えず、境界を越える理由が無い
 * （画面側の `DisplayState`。要件 8.1、8.2）。
 *
 * 空の指定は「絞り込み無し・並べ替え無し・展開無し」であり、文書の行順と宣言の列が
 * そのまま現れる（ドメインの `ViewSpec` / `RowOrder::recompute` の規約）。
 */
export type GridViewSpec = { 
/**
 * 並べ替えの基準列。先頭が第一の基準であり、同値のときだけ次の基準が効く（要件 8.3）。
 */
sort: Array<GridSortKey>, 
/**
 * 絞り込みの条件。**すべてに一致する行だけが可視になる（積）**（要件 8.4）。
 */
filters: Array<GridFilterSpec>, 
/**
 * 入れ子の展開の状態（要件 5.1〜5.4）。列ごとに 1 件である。
 */
expansion: Array<GridExpansionState>, };
/**
 * 見つかった違反（タスク 6.2。要件 4.2、4.5）。
 *
 * 位置（[`GridViolationLocation`]。入れ子の内側の位置を含む）と、**利用者へ伝えるための
 * 理由の文言**を対で運ぶ。理由はドメインの側の語（`schema-engine` の `ViolationReason`）を
 * そのまま出さず、適応層が組み立てた文言を載せる（理由の写像を持たない境界の型に
 * 当たる。`design.md`「GridCommands」の Implementation Notes）。
 */
export type GridViolation = { 
/**
 * 違反の位置（要件 4.5 の入れ子の内側の位置を含む）。
 */
location: GridViolationLocation, 
/**
 * 違反の理由を利用者へ伝える文言（要件 4.2）。
 */
reason: string, };
/**
 * 違反の位置（タスク 6.1。要件 4.2、4.5、6.4、7.5）。
 *
 * `schema-engine` の `Violation` と `data-grid` の `CellViolations` / `NestedPath` が持つ
 * **位置だけ**を写したものである — 行の識別子（文字列）・列の添字（`u32`）・入れ子の内側の
 * 位置である。**理由（`ViolationReason`）は本型に無い** — 違反の理由を提示する経路
 * （要件 4.2）は 6.2 が [`GridViolation`] として定め、文言を組み立てるのは適応層である。
 *
 * **行を持たない違反がある。** ドメインの `Violation::row` は `Option<RowId>` であり、
 * 列そのものの問題は行を持たない。したがって `row` は `Option<String>` である
 * （セルに属する違反はつねに行を持つ）。
 *
 * 内側の位置が空であることは**セル直下**の違反を意味する（要件 4.5 の位置の表現。
 * ドメインの `NestedPath` と同じ規約）。
 */
export type GridViolationLocation = { 
/**
 * 違反の属する行の識別子。`None` は列そのものの問題である。
 */
row: string | null, 
/**
 * 違反している列の添字（0 起点）。
 */
column: number, 
/**
 * 違反している内側の位置（空 = セル直下。要件 4.5）。
 */
path: Array<GridPathSegment>, };
/**
 * 次の違反を探す要求（タスク 6.2。要件 4.4）。
 *
 * **起点は可視行の序数である**（行そのものではない）。可視の序数から行への写像を持つのは
 * 順序を持つ側（`data-grid` の `RowOrder`）であり、写しは要求を通さない。ウィンドウは要求の
 * 型に現れない（要件 4.6）。
 */
export type GridViolationRequest = { 
/**
 * 探索の起点（**可視行の 0 起点の序数**）。
 */
from: number, 
/**
 * 探索の向き。
 */
direction: GridSearchDirection, };
/**
 * 次の違反を探した結果（タスク 6.2。要件 4.2、4.4、4.5、4.6）。
 *
 * **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。`violation` が `None` であるのは
 * 「その向きにこれ以上違反が無い」場合であり、**正常な結果である**（封筒の失敗腕には
 * 載せない）。画面は「見つからなかった」を日付の変更ではなく、これ以上無いこととして扱う。
 *
 * 位置と理由の両方を 1 つの型（[`GridViolation`]）にまとめるのは、**見つかった違反にだけ
 * 両方が存在する**ためである（`location` と `reason` を別々の [`Option`] にすると、
 * 「位置はあるが理由が無い」という状態が型の上で表現できてしまう）。
 */
export type GridViolationResponse = { 
/**
 * 呼び出し元ウィンドウの文脈。
 */
context: WindowContext, 
/**
 * 見つかった違反（位置と理由）。見つからなければ `None`。
 */
violation: GridViolation | null, };
// 次の違反を探した結果の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を
// 名指ししないため、境界が名指しできる具体形を明示的に置く。
export type GridViolationResult = IpcResult<GridViolationResponse, IpcError>;
/**
 * 失敗の原因を区別できる列挙（要件 4.4）。文字列だけのエラーにしない。
 *
 * `kind` を判別子とし、原因ごとの詳細を `detail` に持つ判別可能な合併型として TypeScript へ
 * 落ちる。利用側は `kind` で網羅的に分岐でき、原因ごとに異なる扱いを型で強制できる
 * （tasks.md 2.4）。
 */
export type IpcError = { "kind": "Settings", "detail": { message: string, } } | { "kind": "Sidecar", "detail": { message: string, } } | { "kind": "Window", "detail": { message: string, } } | { "kind": "Diagnostics", "detail": { message: string, } } | { "kind": "Document", "detail": { message: string, } };
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
 * セルの型の種別を表す札（タスク 6.1。要件 3.1、3.2、3.8、10.1〜10.4）。
 *
 * **境界を越える唯一の型の札であり、フロントエンドはこれを取り込む。** 描画側
 * （`design.md`「RendererPort」の `RenderCell.variant`）と入力手段の登録簿（同
 * 「EditorRegistry」の `CellEditorRegistration.kind`）は、双方ともこの生成された札を参照し、
 * **独自に札を定義しない** — 写しを 2 つ持つと、片方だけが増えたときに気づけない
 * （`design.md`「EditorRegistry」の Risks）。
 *
 * 綴りは設計の合併型（同節の `TypeKindTag`）そのままであり、**小文字へ落とさない**。
 * `schema-engine` の `TypeKind` の変種名と 1 対 1 に対応していることが、`src-tauri`
 * （唯一 `schema-engine` と `app-shell` の双方を見られるクレート）で対応を検査できる前提で
 * あるため、`#[serde(rename_all = ...)]` を付けない。
 *
 * [`TypeKindTag::ALL`] が閉じた集合の唯一の源であり、並びは `TypeKind::ALL` と同じ順である。
 * **その一致の検査は `src-tauri` に置く** — 本クレートは他のドメインクレートに依存しては
 * ならない（`crates/app-shell/Cargo.toml` の依存方針）ため、ここから `TypeKind` を参照して
 * 数え合わせることはできない。6.2 / 6.3 の適応層が `TypeKindTag::ALL` と `TypeKind::ALL` を
 * 突き合わせ、**片方だけに変種が増えたときに落ちる検査**を置く。
 */
export type TypeKindTag = "Int" | "Float" | "Decimal" | "Text" | "Bool" | "Date" | "DateTime" | "Enum" | "Ref" | "Attachment" | "Object" | "Array" | "Any" | "Custom";
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

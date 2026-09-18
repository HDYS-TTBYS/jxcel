//! フロントエンドとドメインコアをつなぐ単一の通信境界（要件 4.1、4.2）。
//!
//! 本モジュールは、境界を越えるすべての入力と出力の型を**ひとつの定義**から導けるようにする
//! 場所である。フロントエンド側とドメイン側はこの定義だけを参照し、呼び出し口を各機能が
//! 生やさない。`ts-rs` の derive を付けてよい唯一の場所でもあり、他のモジュールで境界の型に
//! derive してはならない（design.md「IpcContract」の不変条件）。`i64` / `u64` は境界へ直接
//! 出さず、識別子は文字列表現とする。
//!
//! 本モジュールは境界の型そのものと、エラー封筒を再輸出する。実体は tasks.md が追加する:
//! - 2.1: 境界を越える型とエラー封筒（本ファイルと `error`）
//! - 2.2: コマンド名の単一配列と TypeScript 生成（`command_names`）
//! - 7.1: 設定コマンドの入力と応答（[`SettingsGetRequest`] / [`SettingsSetRequest`] /
//!   [`SettingsResponse`] / [`SettingsValue`]）と、設定変更のイベント（[`SETTINGS_CHANGED_EVENT`] /
//!   [`SettingsChangedEvent`]）
//! - 7.6: 終了可否の問い合わせの応答（[`CanCloseWindowResponse`] / [`WindowCloseVerdict`]）。
//!   **要求の型は無い** — 呼び出し元ウィンドウは Tauri が注入する `WebviewWindow` から得るため、
//!   フロントエンドがウィンドウを申告する payload を持たない（偽装できない）
//! - 7.7: ファイル選択の結果と応答（[`DocumentPickOutcome`] / [`PickDocumentFileResponse`]）。
//!   **選択された位置は境界を越えない** — 委譲先（`DocumentHost::attach`）へ引き渡すだけで、
//!   本機能もフロントエンドもその中身に触れないためである（要件 2.4）
//! - 8.2: 初回描画の通知の要求と応答、および三値の判定（[`RenderHeartbeatRequest`] /
//!   [`RenderHeartbeatResponse`] / [`RenderVerdict`]）。**要求はウィンドウを運ばない** —
//!   呼び出し元は Tauri が注入する `WebviewWindow` から取る（偽装できない。要件 4.6）
//! - 9.5: 診断の導線の応答（[`DiagnosticsLogLocationResponse`] /
//!   [`DiagnosticsExportResponse`] / [`DiagnosticsVerbosityResponse`] /
//!   [`DiagnosticsVerbositySetRequest`]）と、詳細度の閉じた列挙（[`DiagnosticsLevel`]）、
//!   およびメニューの活性化を画面へ引き渡すイベント（[`DIAGNOSTICS_REQUESTED_EVENT`] /
//!   [`DiagnosticsRequestedEvent`] / [`DiagnosticsSection`]）。**実体は 4.4 / 4.5 にあり、
//!   ここは境界の形だけを持つ**（`crates/app-shell/src/diagnostics.rs`）
//! - 3.1: ドキュメントのセッションの境界（[`DocumentSummary`] / [`DocumentSheet`] /
//!   [`DocumentOrigin`] / [`DocumentSessionStatus`] と、状態・保存・新規作成・破棄の
//!   4 つの応答型、および保存と新規作成の結果の列挙 [`DocumentSaveOutcome`] /
//!   [`DocumentNewOutcome`] の 2 つ）と、状態変化のイベント
//!   （[`DOCUMENT_SESSION_CHANGED_EVENT`]）。**実体は `document-session` にあり、ここは
//!   境界の形だけを持つ**（`crate::ipc::document`）。位置は境界を越えない — 名前は
//!   ファイル名のみ、件数は `u32` である
//! - 6.1: グリッドの境界（列の情報 [`ColumnDescriptor`] / [`ColumnElementCount`] /
//!   [`ColumnExpandability`] / [`GridPathSegment`]、シートの要約 [`GridSheetSummary`]、
//!   表示の指定 [`GridViewSpec`] / [`GridSortKey`] / [`GridFilterSpec`] /
//!   [`GridExpansionState`]、編集命令 [`GridEditCommand`] / [`GridCellEdit`] /
//!   [`GridCellAddress`] と**行の指し方** [`GridRowTarget`] / [`GridRowAnchor`]（10.4）、
//!   判定の要約 [`GridEditOutcome`] / [`GridCoercionNotice`]、
//!   違反の位置 [`GridViolationLocation`]、および型の種別の札 [`TypeKindTag`]）。
//!   **実体は `data-grid` にあり、ここは境界の形だけを持つ**（`crate::ipc::grid`）
//! - 6.2: グリッドの 5 つのコマンドの封筒（要求 [`GridOpenRequest`] / [`GridViewRequest`] /
//!   [`GridEditRequest`] / [`GridHistoryRequest`] / [`GridViolationRequest`] と、応答
//!   [`GridOpenResponse`] / [`GridViewResponse`] / [`GridEditResponse`] /
//!   [`GridViolationResponse`]、および向きの閉じた列挙 [`GridHistoryDirection`] /
//!   [`GridSearchDirection`] / [`GridViolation`]）。**荷は 6.1 の型をそのまま使い、写すのは
//!   `src-tauri` の適応層である**（`crate::ipc::grid`）。要求はどれもウィンドウを運ばない —
//!   呼び出し元は基盤が注入する引数から取る（偽装できない。要件 4.6）
//! - 6.3: 生バイト経路（`grid_rows_window`）の引数の型は**ここに無い** — 封筒を運べない
//!   経路のものであり、境界の型（`ts-rs` の derive を持つ型）として表せない
//!   （`design.md`「WindowCodec」）
//! - 7.8: グリッドの複製のメニューの活性化を画面へ引き渡すイベント
//!   （[`GRID_COPY_REQUESTED_EVENT`]）。**ペイロード型を持たない** — 複製は引数を取らず、
//!   対象は「そのとき移植口が持っている選択」であり、運ぶ値が 1 つも無いためである
//!   （9.5 の診断の導線が `DiagnosticsRequestedEvent` でどの導線かを運ぶのと対照的である）
//! - 8.9: グリッドの履歴（取り消し・やり直し）のメニューの活性化を画面へ引き渡すイベント
//!   （[`GRID_HISTORY_REQUESTED_EVENT`] / [`GridHistoryRequestedEvent`]）。**荷は向きだけ**
//!   であり、2 つの項目（取り消し・やり直し）を 1 つのイベントで運ぶ（9.5 の診断の導線と
//!   同じ形。8.7 の複製は引数を取らないので荷を持たない）
//! - 10.8: 貼り付けのメニューの活性化を画面へ引き渡すイベント
//!   （[`GRID_PASTE_REQUESTED_EVENT`] / [`GridPasteRequestedEvent`]）。**荷はクリップボードから
//!   読んだ文字**であり、読めなかったときはこのイベントを送らない（8.7 の複製が荷を持たない
//!   のと対照的である — 貼り付けは器だけが読める値を運ぶ必要がある）
//! - 9.3: 描画の健全性の記録（要求 [`RenderHealthRecordRequest`] / 応答
//!   [`RenderHealthRecordResponse`] と、報告の合併型 [`RenderHealthReport`] / 理由の種別
//!   [`RenderPaintFailure`]）。**運ぶのは閉じた札と数値だけであり、任意の文字列を記録へ流す口は
//!   作らない** — 記録の 1 行を組み立てるのは適応層である（10.8 がクリップボードの文字を記録へ
//!   出さなかったのと同じ規律）。要求はウィンドウを運ばない（要件 4.6）
//! - 4.3（`macro-runtime`）: マクロの面（要求 [`MacroStoreRequest`] / [`MacroDeleteRequest`] /
//!   [`MacroRunRequest`] と、応答 [`MacroListResponse`] / [`MacroStoreResponse`] /
//!   [`MacroDeleteResponse`] / [`MacroRunResponse`]、一覧の 1 件 [`MacroSummary`]、実行の 3 値
//!   [`MacroRunOutcome`] と失敗 [`MacroFailureReport`] / [`MacroFrame`]、および種別・能力・
//!   出力・打ち切りの閉じた札）。**実体は `macro-runtime` にあり、ここは境界の形だけを持つ。**
//!   要求はどれもウィンドウを運ばず（要件 4.6）、**上限も運ばない**（設定から適応層が解決する。
//!   要件 6.5）。一覧は**解釈できなかったマクロとその理由**を運ぶ（要件 1.4）

use serde::{Deserialize, Serialize};

pub mod command_names;
pub mod document;
pub mod error;
pub mod grid;

pub use command_names::COMMAND_NAMES;
pub use document::{
    DocumentDiscardResponse, DocumentNewOutcome, DocumentNewResponse, DocumentOrigin,
    DocumentSaveOutcome, DocumentSaveResponse, DocumentSessionStatus, DocumentSheet,
    DocumentStateResponse, DocumentSummary, DOCUMENT_SESSION_CHANGED_EVENT,
};
pub use error::IpcError;
pub use grid::{
    ColumnChoice, ColumnDescriptor, ColumnElementCount, ColumnExpandability,
    ColumnMemberDescriptor, GridCellAddress, GridCellEdit, GridCoercionNotice, GridEditCommand,
    GridEditOutcome, GridEditRequest, GridEditResponse, GridExpansionState, GridFilterSpec,
    GridHistoryDirection, GridHistoryRequest, GridOpenRequest, GridOpenResponse, GridPathSegment,
    GridReferenceRequest, GridReferenceResponse, GridReferenceRow, GridRowAnchor, GridRowTarget,
    GridSearchDirection, GridSheetSummary, GridSortKey, GridViewRequest, GridViewResponse,
    GridViewSpec, GridViolation, GridViolationLocation, GridViolationRequest,
    GridViolationResponse, TypeKindTag, GRID_REFERENCE_PAGE_LIMIT,
};

/// 境界を越えるすべてのコマンドが返す封筒（要件 4.2、4.4）。
///
/// `status` を判別子とし、成功（`ok`）と失敗（`error`）を型で区別する判別可能な合併型として
/// TypeScript へ落ちる。利用側は `status` で網羅的に分岐できる（tasks.md 2.4 がこの性質の上に
/// 薄い呼び出しラッパを載せる）。本型を含め、境界の生成物に `any` を混入させない
/// （research.md 決定 1 が `tauri-specta` を却下した理由のひとつ）。
//
// `serde(tag = "status")` の内部タグ付け。生成される TypeScript は
// `{ "status": "ok", data: T, } | { "status": "error", error: E, }` である。失敗の原因は
// [`IpcError`] が運ぶ（要件 4.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "status")]
pub enum IpcResult<T, E> {
    /// 成功。ドメインの結果を `data` に載せる。
    #[serde(rename = "ok")]
    Ok { data: T },
    /// 失敗。原因を `error` に載せる。
    #[serde(rename = "error")]
    Err { error: E },
}

/// 境界を越えるウィンドウの識別子（要件 4.2、4.6）。
///
/// 境界を越える識別子は 64 ビット整数をそのまま公開せず、文字列表現とする。JavaScript の
/// `number` は IEEE 754 の倍精度であり、`i64` / `u64` の全域を正確に表せないためである。
/// TypeScript 側の型も `string` に固定する（design.md「IpcContract」の不変条件）。
//
// 丸めの具体例: `u64::MAX = 18446744073709551615` は JavaScript の数値では
// `18446744073709552000` になる。識別子を 64 ビット整数のまま境界へ出すと、型検査も実行時も
// 静かに値を取り違える。
//
// `ts(type = "string")` は、内側の型が将来 64 ビット整数へ変わっても生成物が `bigint` /
// `number` へ落ちないようにするための固定である。1 フィールドの新定型は serde でも内側の値
// そのものとして直列化されるため `serde(transparent)` は付けない（付けると ts-rs が解釈
// できず警告を出すうえ、挙動は変わらない）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[ts(type = "string")]
pub struct WindowLabel(String);

impl WindowLabel {
    /// ラベル文字列から識別子を作る。
    pub fn new(label: impl Into<String>) -> Self {
        Self(label.into())
    }

    /// 元のラベル文字列を返す。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// コマンド呼び出しの文脈（要件 4.2、4.6）。呼び出し元ウィンドウを呼び出し先が識別できる
/// ようにする。
//
// 呼び出し元の識別子は文字列の [`WindowLabel`] で運ぶため、64 ビット整数は境界に現れない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct WindowContext {
    /// 呼び出し元ウィンドウのラベル。
    pub window: WindowLabel,
}

/// 境界を越える設定値（タスク 7.1。要件 7.1、7.4）。
///
/// 設定ストアは鍵から生の JSON への写像を持つ（[`crate::settings`]）。その値を境界で型付け
/// するには、カタログの 5 鍵それぞれに固有の型を並べた列挙を持つか、JSON の形をそのまま写す
/// かのどちらかである。ここは後者を取り、**TypeScript では `unknown`** として出す
/// （`#[ts(type = "unknown")]`）。
///
/// **`serde_json::Value` をそのまま境界へ出さない理由**は 2 つある。ts-rs の
/// `serde-json-impl` feature を有効にすると生成物が `any` になり、「生成物に `any` を混ぜない」
/// という不変条件（tasks.md 2.1、research.md 決定 1）を破る。また `i64` / `u64` を露出させる
/// 経路を型で塞いでおく必要がある（識別子は文字列とする規則）。`unknown` は受け手に絞り込みを
/// 強制するため、値の形を呼び出し側が仮定しない。
///
/// 列挙の新定型は serde でも内側の値そのものとして直列化される（[`WindowLabel`] と同じ）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(type = "unknown")]
pub struct SettingsValue(serde_json::Value);

impl SettingsValue {
    /// 生の JSON から境界の値を作る。
    pub fn new(value: serde_json::Value) -> Self {
        Self(value)
    }

    /// 生の JSON を借用する。
    pub fn as_json(&self) -> &serde_json::Value {
        &self.0
    }
}

/// 設定値の読み取り要求（タスク 7.1。要件 7.1）。
///
/// 鍵は**名前の文字列**で運ぶ。`SettingsKey` は閉じた列挙であり、文字列から鍵を作る入口は
/// `SettingsKey::from_name` だけである（要件 7.7）。したがってカタログに無い名前は
/// 呼び出し先でエラー封筒の腕になり、**値の型は鍵ごとに固定しない**（値の型の閉性は
/// 7.7 の要求ではなく、要求は鍵空間の閉性である。tasks.md 4.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct SettingsGetRequest {
    /// 読み取る設定の鍵（カタログ名。例 `appearance.theme`）。
    pub key: String,
}

/// 設定値の書き込み要求（タスク 7.1。要件 7.1、7.4）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct SettingsSetRequest {
    /// 書き込む設定の鍵（カタログ名）。
    pub key: String,
    /// 書き込む値。ドキュメントの内容を指す鍵はカタログに存在しない（要件 7.7）。
    pub value: SettingsValue,
}

/// 設定コマンドの応答（タスク 7.1。要件 4.6、7.1）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む。**呼び出し先は呼び出し元を識別でき（要件 4.6、
/// 2.1 の [`WindowContext`] を再定義せずそのまま使う）、フロントエンドも自分がどのウィンドウから
/// 呼んだかを応答から観測できる。`key` は正規化後のカタログ名、`value` は書き込み後（読み取りは
/// 現在）の値で、鍵が存在しなければ `None` である。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct SettingsResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// 対象の鍵（カタログ名）。
    pub key: String,
    /// 現在（書き込みコマンドでは書き込み後）の値。存在しない鍵は `None`。
    pub value: Option<SettingsValue>,
}

/// 設定変更を通知する Tauri イベントの名前（タスク 7.1。要件 7.4）。
///
/// `invoke` の宛先を持たないためコマンド名の配列（[`COMMAND_NAMES`]）には現れない。代わりに
/// **生成物（`src/ipc/bindings.ts`）へ定数として出す**ことで、フロントエンドが文字列
/// リテラルを綴り間違える経路を塞ぐ（タスク 2.3 のドリフト検査がこの定数もバイト比較する）。
pub const SETTINGS_CHANGED_EVENT: &str = "settings_changed";

/// 設定変更の通知（タスク 7.1。要件 7.4）。
///
/// 設定ストアの通知（`crate::settings::SettingsChanged`）を境界の形へ写したものである。運ぶのは
/// **カタログにある閉じた鍵の名前と、その新しい値だけ**である。したがってドキュメントの内容
/// （セル値・スキーマ）を指す鍵はカタログに存在せず、この経路には載りえない（要件 7.7、8.4）。
/// 記録側へ値を渡す経路はこの型ではなく [`crate::diagnostics::Redacted`] を通る（タスク 7.1 は
/// 記録に**鍵だけ**を書き、値は決して書かない）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct SettingsChangedEvent {
    /// 変更された鍵（カタログ名）。
    pub key: String,
    /// 変更後の値。
    pub value: SettingsValue,
}

/// ウィンドウを閉じてよいかの判定（タスク 7.6。要件 2.6）。
///
/// ドキュメント所有者への委譲点（`src-tauri/src/ports.rs` の `CloseVerdict`）の判定を、
/// そのまま境界の形へ写したものである。`verdict` を判別子とする判別可能な合併型として
/// TypeScript へ落ちるため、フロントエンドは `verdict` で網羅的に分岐できる。
///
/// **`Deny` は失敗ではない。**「委譲先が閉じてはならないと答えた」という正常な応答であり、
/// 封筒（[`IpcResult`]）の `status: "error"` の腕には載せない。`reason` は利用者へ伝えるための
/// 材料であり、**見せ方を決めるのは呼び出し元（フロントエンド）である**
/// （tasks.md 6.2 / 7.6。ここで文言を確定しない）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "verdict")]
pub enum WindowCloseVerdict {
    /// 閉じてよい。
    Allow,
    /// 閉じてはならない。`reason` は拒否の理由（空でもよい）。
    Deny {
        /// 拒否の理由。利用者へ提示するための材料であり、そのまま見せる文言とは限らない。
        reason: String,
    },
}

/// ウィンドウを閉じてよいかの問い合わせの応答（タスク 7.6。要件 2.6、4.6）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む。**呼び出し元は Tauri が注入する
/// `WebviewWindow` から得るので、**フロントエンドがウィンドウの識別子を payload で申告する
/// 経路は存在しない**（偽装できない。要件 4.6、tasks.md 7.1）。`verdict` が委譲先の判定で
/// あり、`Allow` のときだけフロントエンドがウィンドウを破棄する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct CanCloseWindowResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// ドキュメント所有者の判定（要件 2.6）。
    pub verdict: WindowCloseVerdict,
}

/// 初回描画の判定（タスク 8.2。要件 10.1、10.2）。**三値である。**
///
/// 判定の実体は Tauri 非依存の中核（`crates/app-shell/src/render.rs`）にあり、本型はその
/// 結果を境界へ出すための形である（`ts-rs` の derive を付けてよい唯一の場所が本モジュールで
/// あるという不変条件に従う）。写像の根拠は `render.rs` のモジュール doc にある。要約:
///
/// - `Painted`（[`RenderVerdict::Painted`]）: 描画フレームの中から通知が届き、ラスタライザが
///   ハードウェア加速（または判別不能）だった。**描画が成立した。**
/// - `SoftwareRaster`（[`RenderVerdict::SoftwareRaster`]）: 通知が届いたが、ラスタライザが
///   既知のソフトウェア実装だった。**描画は成立している**（低速な経路である）。
/// - `NoPaint`（[`RenderVerdict::NoPaint`]）: 期限までに通知が届かなかった。**描画が成立して
///   いない。** したがってこの値だけが、次回起動で代替経路を適用するための印を立てる（要件 10.3）。
///
/// **タイムアウト（`NoPaint`）とソフトウェアラスタライザ（`SoftwareRaster`）は別の値で
/// ある。**前者は描画の不成立、後者は描画の成立であり、混同すると要件 10.3 の代替経路を
/// 正常な環境へ適用してしまう。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub enum RenderVerdict {
    /// 描画が成立した。
    Painted,
    /// 描画は成立したが、ソフトウェアラスタライザ経由だった。
    SoftwareRaster,
    /// 期限までに通知が届かず、**描画が成立しなかった**。
    NoPaint,
}

/// 初回描画の通知の要求（タスク 8.2。要件 10.1、10.2）。
///
/// 運ぶのは**ラスタライザの文字列**と**実際に描画されていた画面の識別子**である。
///
/// - `renderer` はフロントエンドが `WEBGL_debug_renderer_info` の
///   `UNMASKED_RENDERER_WEBGL` から得た値である（research.md 決定 7）。取得できない環境では
///   `null` であり、その場合も通知が届いたこと自体は描画成立の証拠になる（中核の写像を参照）。
/// - `screen` は通知を送る時点で**シェルの領域が実際に表示していた画面**の識別子である
///   （`src/shell/Layout.tsx` が領域の要素に書く `data-shell-screen`。tasks.md 9.1 の契約）。
///   3 OS の描画確認（tasks.md 10.4）が「**どの画面が描画されたか**」をこの 1 つの記録から
///   読めるようにするために載せる（`src/shell/renderHeartbeat.ts` のモジュール doc を参照）。
///   **要求した識別子ではなく、描画された識別子であること**が要点である — 起動時に要求した
///   画面が未登録なら、シェルは既定の初期画面へ落ちるため、両者は一致しない（9.7 の契約）。
///   領域を読めなかったときは `null`（「報告なし」として扱われ、描画の証明にはならない）。
///
/// **ウィンドウは運ばない。** 呼び出し元は Tauri が注入する `WebviewWindow` から取るため、
/// フロントエンドがウィンドウを偽装する経路は存在しない（要件 4.6、tasks.md 7.1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct RenderHeartbeatRequest {
    /// ラスタライザの文字列。取得できなければ `null`。
    pub renderer: Option<String>,
    /// 通知の時点で**実際に描画されていた画面の識別子**。読めなければ `null`。
    pub screen: Option<String>,
}

/// 初回描画の通知の応答（タスク 8.2。要件 10.1、10.2、4.6）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。`verdict` はこの通知で確定した
/// 判定であり、2 回目以降の通知では最初に確定した値がそのまま返る（上書きしない）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct RenderHeartbeatResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// 確定した判定（要件 10.1、10.2）。
    pub verdict: RenderVerdict,
}

/// ファイル選択の結果（タスク 7.7。要件 2.4）。
///
/// 選択手段（`src-tauri/src/dialog.rs` の DialogGate）が得た結果と、その位置をドキュメント
/// 所有者へ引き渡した結果を、**1 つの判別可能な合併型**にまとめてフロントエンドへ返す。
/// `outcome` を判別子とするため、利用側は網羅的に分岐できる。
///
/// **`Cancelled` と `Rejected` は失敗ではない。**「利用者が取り消した」ことも「所有者が
/// 受け取らなかった」ことも、コマンドが正常に答えた結果である。したがって封筒
/// （[`IpcResult`]）の `status: "error"` の腕には載せない — 載せると「通信が失敗した」ことと
/// 区別できなくなる（tasks.md 7.6 が終了拒否で同じ判断をしている）。`Rejected` は利用者へ
/// 伝えるための材料（`reason`）を運び、**見せ方を決めるのは呼び出し元である**。
///
/// **選択された位置そのものは境界を越えない。** 位置は `DocumentHost::attach` へ引き渡す
/// だけであり（要件 2.4）、アプリケーションシェルもフロントエンドもその中身に触れない。
/// したがってパスを表す型はここに現れない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "outcome")]
pub enum DocumentPickOutcome {
    /// 利用者が選択を取り消した。**正常な結果であり、引き渡しは起きていない。**
    Cancelled,
    /// 選択された位置を委譲先へ引き渡し、**委譲先が受け入れた**。
    Attached,
    /// 選択された位置を委譲先へ引き渡したが、**委譲先が受け取らなかった**。
    ///
    /// `reason` は拒否の理由（空でもよい）。利用者へ提示するための材料であり、そのまま見せる
    /// 文言とは限らない。
    Rejected {
        /// 拒否の理由。
        reason: String,
    },
}

/// ファイル選択の応答（タスク 7.7。要件 2.4、4.6）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む。**呼び出し元は Tauri が注入する `WebviewWindow`
/// から得るので、**フロントエンドがウィンドウの識別子を payload で申告する経路は存在しない**
/// （偽装できない。要件 4.6、tasks.md 7.1）。`outcome` が選択と引き渡しの結果である。
///
/// **要求の型は無い。** この機能に必要な入力は操作対象のウィンドウだけであり、それは基盤が
/// 注入する（[`CanCloseWindowResponse`] と同じ形）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct PickDocumentFileResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// 選択と引き渡しの結果（要件 2.4）。
    pub outcome: DocumentPickOutcome,
}

/// ウィンドウとドキュメントの関連付けの状態（タスク 9.6。要件 2.1、2.2）。**閉じた列挙である。**
///
/// 要件 2.2 が操作の導線を提示する対象は「ドキュメントを関連付けていないウィンドウ」であり、
/// その判定はウィンドウの生成時に確定した関連付け（レジストリの写像）から取る。**ラベルの
/// 接頭辞（`empty-` / `doc-`）からは判定しない** — 接頭辞は割り当て順の規約であって関連付けの
/// 事実ではなく、`attach` は記録された関連付けを書き換えないため両者が食い違いうる
/// （9.6 の画面のモジュール doc を参照）。
///
/// **パスは運ばない。** 問いは関連付けの有無だけであり、どのドキュメントかは所有者
/// （下流スペック）が持つ（[`DocumentPickOutcome`] と同じ方針）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum WindowDocumentState {
    /// 関連付けが無い。**この状態のウィンドウが新規作成と既存ファイルを開く操作を提示する**
    /// （要件 2.2）。
    Unassociated,
    /// ドキュメントが関連付けられている。
    Associated,
}

/// 関連付けの問い合わせの応答（タスク 9.6。要件 2.2、4.6）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む。**呼び出し元は Tauri が注入する `WebviewWindow`
/// から得るので、**フロントエンドがウィンドウの識別子を payload で申告する経路は存在しない**
/// （偽装できない。要件 4.6、tasks.md 7.1）。**要求の型は無い**（操作対象のウィンドウだけが
/// 入力であり、それは基盤が注入する。[`PickDocumentFileResponse`] と同じ形）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct WindowDocumentStateResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// 関連付けの状態（要件 2.1、2.2）。
    pub state: WindowDocumentState,
}

// ---------------------------------------------------------------------------
// 診断の導線（タスク 9.5。要件 8.1、8.6、8.7）
// ---------------------------------------------------------------------------

/// 記録の詳細度（タスク 9.5。要件 8.7）。**閉じた列挙である。**
///
/// 実体は Tauri 非依存の中核 [`crate::diagnostics::DiagnosticsLevel`] であり、この型は
/// **境界の形**である（`ts-rs` の derive を付けてよい唯一の場所が本モジュールであるという
/// 不変条件に従う。`RenderVerdict` と同じ扱い）。したがって境界の列挙と中核の列挙の間に
/// 対応付けが必要であり、それは [`From`] の 2 方向（網羅的な `match`）が担う — **どちらかの
/// 列挙に値を足すと、もう一方への写像がコンパイルエラーになる**（片側だけの追加を許さない）。
///
/// 詳細度の昇順は [`Ord`] が表す（`Off` < `Error` < `Warn` < `Info` < `Debug` < `Trace`）。
/// 中核の列挙と同じ順序であり、[`DiagnosticsLevel::ALL`] がその閉じた集合を昇順で並べる。
/// 利用者へは [`DiagnosticsVerbosityResponse::levels`] としてこの順序で渡すので、**画面は
/// 並び順を自前で持たない**（tasks.md 4.5 の詳細度の契約）。
///
/// 直列化は中核と同じ小文字表現（`"off"` … `"trace"`）であり、設定ファイルに載る値と
/// 境界を越える値の綴りが一致する（4.5 の `#[serde(rename_all = "lowercase")]`）。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticsLevel {
    /// 記録しない。
    Off,
    /// 失敗だけを記録する。
    Error,
    /// 失敗と警告を記録する。
    Warn,
    /// 失敗・警告・通常の出来事を記録する（既定）。
    Info,
    /// 開発時の詳細を記録する。
    Debug,
    /// 最も細かい記録。
    Trace,
}

impl DiagnosticsLevel {
    /// 閉じた列挙の全値を**詳細度の昇順**で並べたもの（[`Ord`] の順序と同じ）。
    ///
    /// 画面へ渡す [`DiagnosticsVerbosityResponse::levels`] の源であり、**「どの値があり、
    /// どの順に並ぶか」の唯一の定義**である。値を足すときはここへも足す（足し忘れは
    /// [`DiagnosticsLevel::ALL`] を走査するテストが捕まえる）。
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::Error,
        Self::Warn,
        Self::Info,
        Self::Debug,
        Self::Trace,
    ];
}

impl From<crate::diagnostics::DiagnosticsLevel> for DiagnosticsLevel {
    /// 中核の詳細度を境界の形へ写す（**1 対 1**。tasks.md 4.5 の契約）。
    fn from(level: crate::diagnostics::DiagnosticsLevel) -> Self {
        use crate::diagnostics::DiagnosticsLevel as Core;
        match level {
            Core::Off => Self::Off,
            Core::Error => Self::Error,
            Core::Warn => Self::Warn,
            Core::Info => Self::Info,
            Core::Debug => Self::Debug,
            Core::Trace => Self::Trace,
        }
    }
}

impl From<DiagnosticsLevel> for crate::diagnostics::DiagnosticsLevel {
    /// 境界の詳細度を中核の形へ戻す（**1 対 1**）。要求（
    /// [`DiagnosticsVerbositySetRequest`]）はこの向きを通る。
    fn from(level: DiagnosticsLevel) -> Self {
        match level {
            DiagnosticsLevel::Off => Self::Off,
            DiagnosticsLevel::Error => Self::Error,
            DiagnosticsLevel::Warn => Self::Warn,
            DiagnosticsLevel::Info => Self::Info,
            DiagnosticsLevel::Debug => Self::Debug,
            DiagnosticsLevel::Trace => Self::Trace,
        }
    }
}

/// 記録の保存場所の応答（タスク 9.5。要件 8.1、4.6）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。`directory` は各 OS の規約で解決した
/// 記録ディレクトリであり（4.4 の [`crate::diagnostics::log_dir`]）、**利用者に見せるための
/// 文字列**である（境界では識別子も位置も文字列で運ぶ。表示できないバイト列は置換される）。
/// この経路は保存場所を提示するだけで、場所を開いたり走査したりしない（要件 4.7）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DiagnosticsLogLocationResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// 記録の保存場所（表示用の文字列）。
    pub directory: String,
}

/// 書き出しに含めた記録の有無（タスク 9.5。要件 8.6）。
///
/// 4.5 の [`crate::diagnostics::ExportReport::files_merged`] は件数を数値で持つが、**境界へ
/// 数値を出さない**（`crates/app-shell` の不変条件: 境界を越える値は文字列か、数値を含まない
/// 閉じた列挙である）。利用者にとって必要な区別は「記録を連結した」か「記録が 1 つも無かった」か
/// だけなので、件数ではなく**閉じた列挙**で運ぶ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticsExportRecords {
    /// 記録が 1 つ以上あり、そのすべてを連結した。
    Merged,
    /// 記録が 1 つも無かった。**それでも書き出しは成功しており、宛先に 1 つのファイルがある**
    /// （見出しと「記録は見つからなかった」の行だけ）。
    Empty,
}

/// 記録の書き出しの応答（タスク 9.5。要件 8.6、4.6）。
///
/// **書き出しは 1 つのファイルにまとまる**（4.5 の [`crate::diagnostics::export`] の契約）。
/// [`DiagnosticsExportRecords::Empty`] でも成功であり、その場合も `destination` に 1 つの
/// ファイルができている（記録が無かったことを利用者へ伝えるための材料）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DiagnosticsExportResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// 書き出したファイルの位置（表示用の文字列）。
    pub destination: String,
    /// 連結した記録の有無（`Empty` でも書き出しは成功している）。
    pub records: DiagnosticsExportRecords,
}

/// 記録の詳細度の応答（タスク 9.5。要件 8.7、4.6）。
///
/// 読み取りと変更の**両方**がこの形を返す。`level` が現在の値（変更では変更後の値）であり、
/// `levels` が選べる値の全体を**詳細度の昇順**で並べたものである（[`DiagnosticsLevel::ALL`]）。
/// 画面はこの 2 つだけを見て「現在値の表示」と「選択肢の列挙」を行えるので、**選べる値の集合と
/// 順序を画面側に写さない**（写すと中核の列挙と食い違う余地ができる）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DiagnosticsVerbosityResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// 現在の詳細度（変更コマンドでは変更後の値）。
    pub level: DiagnosticsLevel,
    /// 選べる詳細度の全体（`Off` から `Trace` へ昇順）。
    pub levels: Vec<DiagnosticsLevel>,
}

/// 記録の詳細度の変更要求（タスク 9.5。要件 8.7）。
///
/// 詳細度は**閉じた列挙 [`DiagnosticsLevel`] の値だけ**であり、任意の文字列は載らない。
/// 列挙に無い値は `serde` の復元に失敗するため、コマンドの引数として境界を越えられない
/// （その拒否はフロントエンド側のラッパが通信境界の失敗として扱う。tasks.md 2.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DiagnosticsVerbositySetRequest {
    /// 設定する詳細度。
    pub level: DiagnosticsLevel,
}

/// 診断の導線がメニューから要求されたことを伝える Tauri イベントの名前（タスク 9.5）。
///
/// `invoke` の宛先を持たないためコマンド名の配列（[`COMMAND_NAMES`]）には現れない。設定変更の
/// 通知（[`SETTINGS_CHANGED_EVENT`]）と同じく、**生成物（`src/ipc/bindings.ts`）へ定数として
/// 出す**ことで、フロントエンドが文字列リテラルを綴り間違える経路を塞ぐ（タスク 2.3 の
/// ドリフト検査がこの定数もバイト比較する）。
pub const DIAGNOSTICS_REQUESTED_EVENT: &str = "diagnostics_requested";

/// 診断の導線のうち、利用者がメニューから選んだもの（タスク 9.5。要件 8.1、8.6、8.7）。
///
/// メニューの項目は 3 つの導線に 1 つずつ対応するので、活性化は**どれが選ばれたか**を運ぶ。
/// 画面はこの値で該当の区画を示す（利用者にとっては「選んだ項目の場所が開く」ことになる）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticsSection {
    /// 記録の保存場所の確認（要件 8.1）。
    Location,
    /// 記録の書き出し（要件 8.6）。
    Export,
    /// 記録の詳細度の変更（要件 8.7）。
    Verbosity,
}

/// メニューの活性化を画面へ引き渡す通知（タスク 9.5）。
///
/// メニューの処理はイベントループのスレッドで走り、対象ウィンドウのフロントエンドへ届ける
/// 必要がある。そこで 7.4 の登録口が受けた選択を、この 1 つのイベントとして**活性化の対象
/// ウィンドウへ**送る（7.5 の振り向けの結果を使う。要件 3.5）。画面はこれを購読し、遷移と
/// 区画の選択を行う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DiagnosticsRequestedEvent {
    /// 利用者が選んだ導線。
    pub section: DiagnosticsSection,
}

/// グリッドの範囲の複製がメニューから要求されたことを伝える Tauri イベントの名前
/// （タスク 8.7。要件 7.8）。
///
/// `invoke` の宛先を持たないためコマンド名の配列（[`COMMAND_NAMES`]）には現れない。設定変更の
/// 通知（[`SETTINGS_CHANGED_EVENT`]）と同じく、**生成物（`src/ipc/bindings.ts`）へ定数として
/// 出す**ことで、フロントエンドが文字列リテラルを綴り間違える経路を塞ぐ（タスク 2.3 の
/// ドリフト検査がこの定数もバイト比較する）。
///
/// **ペイロード型を持たない。**複製は引数を取らない — 対象は「そのとき移植口が持っている
/// 選択」であり、メニューの項目が選んだものを運ぶ必要が無い（9.5 の診断の導線は 3 つの導線の
/// どれが選ばれたかを運ぶので [`DiagnosticsRequestedEvent`] を持つ）。境界を越える値が 1 つも
/// 無いので、型を足すことは「空の構造体を 1 つ越えさせる」ことになる。
///
/// 送り先は**活性化の対象ウィンドウ**（7.5 の振り向け）1 つだけである。グリッドの画面を
/// 出していないウィンドウには購読者が居ないので、そこで選んでも何も起きない（画面が
/// 自分で判断する。器は関知しない）。
pub const GRID_COPY_REQUESTED_EVENT: &str = "grid_copy_requested";

/// グリッドの履歴（取り消し・やり直し）がメニューから要求されたことを伝える Tauri イベントの
/// 名前（タスク 8.9。要件 9.9）。
///
/// `invoke` の宛先を持たないためコマンド名の配列（[`COMMAND_NAMES`]）には現れない。設定変更の
/// 通知（[`SETTINGS_CHANGED_EVENT`]）と同じく、**生成物（`src/ipc/bindings.ts`）へ定数として
/// 出す**ことで、フロントエンドが文字列リテラルを綴り間違える経路を塞ぐ（タスク 2.3 の
/// ドリフト検査がこの定数もバイト比較する）。
///
/// **項目は 2 つ（取り消し・やり直し）であり、イベントは 1 つである。**どちらの項目が選ばれたか
/// は荷（[`GridHistoryRequestedEvent`]）が運ぶ — 9.5 の診断の導線が 3 つの導線を 1 つのイベントで
/// 運ぶのと同じ形である。**画面の側の入口も 1 つになる**（2 つのイベントに分けると、片方だけを
/// 購読した画面が作れてしまう）。
///
/// 送り先は**活性化の対象ウィンドウ**（7.5 の振り向け）1 つだけである。グリッドの画面を
/// 出していないウィンドウには購読者が居ないので、そこで選んでも何も起きない（画面が
/// 自分で判断する。器は関知しない）。
pub const GRID_HISTORY_REQUESTED_EVENT: &str = "grid_history_requested";

/// グリッドへの貼り付けがメニューから要求されたことを伝える Tauri イベントの名前
/// （タスク 10.8。要件 7.8）。
///
/// `invoke` の宛先を持たないためコマンド名の配列（[`COMMAND_NAMES`]）には現れない。設定変更の
/// 通知（[`SETTINGS_CHANGED_EVENT`]）と同じく、**生成物（`src/ipc/bindings.ts`）へ定数として
/// 出す**ことで、フロントエンドが文字列リテラルを綴り間違える経路を塞ぐ（タスク 2.3 の
/// ドリフト検査がこの定数もバイト比較する）。
///
/// **複製（[`GRID_COPY_REQUESTED_EVENT`]）と違い、荷を持つ。**複製は引数を取らない（対象は
/// 「そのとき移植口が持っている選択」である）が、貼り付けは**クリップボードの文字**を要する —
/// 器（Rust 側）だけが読めるので、読んだ文字をこのイベントで画面へ渡す（[`GridPasteRequestedEvent`]）。
///
/// 送り先は**活性化の対象ウィンドウ**（7.5 の振り向け）1 つだけである。グリッドの画面を
/// 出していないウィンドウには購読者が居ないので、そこで選んでも何も起きない（画面が
/// 自分で判断する。器は関知しない）。
pub const GRID_PASTE_REQUESTED_EVENT: &str = "grid_paste_requested";

/// メニューの活性化を画面へ引き渡す通知（タスク 10.8。要件 7.8）。
///
/// メニューの処理はイベントループのスレッドで走り、対象ウィンドウのフロントエンドへ届ける
/// 必要がある。そこで 7.4 の登録口が受けた選択を、この 1 つのイベントとして**活性化の対象
/// ウィンドウへ**送る（7.5 の振り向けの結果を使う。要件 3.5）。
///
/// **運ぶのはクリップボードから読んだ文字だけである。**貼り付けの解釈（何行何列か、どの列の
/// 型に掛けるか）はドメインの `PasteCodec::parse` と適用層の仕事であり、ここは**文字列を
/// 1 バイトも変えずに運ぶ**（画面も解釈しない。`design.md` の「貼り付けのテキストは
/// 1 バイトも変えない」）。
///
/// **読めなかったときはこのイベントそのものを送らない** — 空の荷を送れば、画面は空の矩形を
/// 貼り付けて「貼り付けられた」と見える何かを出す（打鍵の経路と同じ扱いであり、新しい失敗の
/// 提示を作らない）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridPasteRequestedEvent {
    /// クリップボードから読んだ文字。**解釈も正規化もしない**（そのまま `onPaste` へ渡る）。
    pub text: String,
}

/// メニューの活性化を画面へ引き渡す通知（タスク 8.9。要件 9.9）。
///
/// メニューの処理はイベントループのスレッドで走り、対象ウィンドウのフロントエンドへ届ける
/// 必要がある。そこで 7.4 の登録口が受けた選択を、この 1 つのイベントとして**活性化の対象
/// ウィンドウへ**送る（7.5 の振り向けの結果を使う。要件 3.5）。
///
/// **運ぶのは向きだけである。**`direction` は 6.2 の閉じた列挙（[`GridHistoryDirection`]）を
/// そのまま使い、**画面は綴りを書かない**（生成物の型で分岐する）。取り消しとやり直しの
/// 2 項目しか無いので、運ぶ値はこの 1 つで足りる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridHistoryRequestedEvent {
    /// 利用者が選んだ向き（取り消し・やり直し）。
    pub direction: GridHistoryDirection,
}

/// 描画の健全性の報告（タスク 9.3。要件 12.2、12.3）。**札と数値だけを運ぶ閉じた合併型である。**
///
/// 3 つの腕は**互いに素**であり、それぞれが要る数値だけを持つ。同じ事実を平たい欄の集まりで
/// 表すと「走査の劣化なのに理由の種別が載っている」ような状態が型の上で作れてしまうため、
/// ここでは**作れない形**にしてある（6.1 の [`GridEditCommand`] と同じ規律）。
///
/// 3 つ目の腕（[`RenderHealthReport::Observation`]）だけは**検証専用**である — 書くのは検証用の
/// 観測画面（tasks.md 9.2。既定のビルドでは登録が定数畳み込みで消える）だけであり、要件の合否を
/// 判定するのは検査器の側である（記録は「測った事実」だけを運ぶ）。
///
/// **任意の文字列は運べない。**記録へ流せるのはこの 3 つの事実と数値だけであり、記録の 1 行を
/// 組み立てるのは器の側である — 記録の注入面を広げないためである（10.8 がクリップボードから
/// 読んだ文字を記録へ出さず文字数だけを写したのと同じ規律）。
///
/// **時間はマイクロ秒の整数で運ぶ。**要件 11.1 の予算は 16.67 ms であり、境界は文字列と
/// 32 ビット以下の整数だけで構成する（`ipc-contract.md`）ため、浮動小数をそのまま出せない。
/// したがって予算も [`RenderHealthReport::ScanBelowBudget::budget_us`]（16670）として送り、
/// **記録の 1 行を組み立てる側がミリ秒へ戻す**（丸めの経路を境界へ持ち込まない）。
///
/// 直列化の札は小文字の綴りであり、画面側の記録の口
/// （`src/features/grid/renderHealth.ts` の `RenderHealthReport.fact`）と同じ綴りである。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "fact", rename_all = "snake_case")]
pub enum RenderHealthReport {
    /// 表の描画が成立しなかった（要件 12.2）。
    PaintFailed {
        /// 成立しなかった理由の種別。
        failure: RenderPaintFailure,
        /// 数えた色数（面が読めなかったときは `null`）。
        colors: Option<u32>,
    },
    /// 走査の滑らかさが予算を満たさなくなった（要件 12.3）。
    ///
    /// **記録の有無を決める閾値は画面側にあり、要件値に計測の刻みの許容を足したものである**
    /// （`src/features/grid/renderHealth.ts` の `FRAME_BUDGET_TOLERANCE_MS`）。したがって
    /// **要件値をわずかに超えただけの中央値は記録されない**ことがある — 1.6 の実画面の実測では
    /// 健全な走査が 17.00 ms（要件値 16.67 ms。時計の刻みが 1 ms）であり、要件値で記録を決めると
    /// 健全な走査が劣化として載る。ここが運ぶ `budget_us` は**要件値そのもの**であり、
    /// 要件 11.1 の合否は 9.2 の実画面の観測が要件値で判定する。
    ScanBelowBudget {
        /// 測定したフレーム時間の中央値（**マイクロ秒**）。
        median_us: u32,
        /// 要件 11.1 の予算（**マイクロ秒**。16670）。
        budget_us: u32,
    },
    /// **観測の項目が待ちに入った**（tasks.md 9.2）。**検証専用である。**
    ///
    /// 貼り付けの往復（10.8）は**段が外からメニューの項目を活性化する**ことでしか成立しない。
    /// 段が「今が活性化の機会である」と知る道は**記録しか無い** — アクセシビリティの木を
    /// 深く歩くと WebKit のアクセシビリティが答えなくなり（実測: 565 節の直後に 14 節へ落ち、
    /// 300 秒以上戻らなかった）、後から置かれる印は誰にも読めない。ウィンドウの題名も
    /// 反映されない（実測: `document.title` を変えても `frame` の名前は `jxcel` のまま）。
    /// **記録は 3 OS で同じものを読める唯一の場所**である（5.2 / 5.3 と同じ理由）。
    ItemWaiting {
        /// 待ちに入った項目。
        item: ObservationItem,
    },
    /// **グリッドの観測**（tasks.md 9.2。要件 11.1、11.2、11.3、12.1、12.2）。**検証専用である。**
    ///
    /// 9.2 の受け入れは「10 万行のシートを開き、末尾へ移動し、セルを編集し、取り消して戻すまでを
    /// **実際に起動して観測する**」ことと、**3 つの OS のそれぞれで観測が成功すること**である。
    /// 観測するのは検証用の初期画面（`src/features/grid/gridObservation.tsx`。製品の
    /// [`GridScreen`] そのものを描く）であり、**その実測を 3 OS の検査器が同じ形で読める場所へ
    /// 残す**必要がある。Linux には AT-SPI（アクセシビリティの木）があるが、macOS の WKWebView と
    /// Windows の WebView2 には無く（それぞれ別の API であり、CI のランナーでは権限も無い）、
    /// **3 OS で同じものを読める唯一の場所が診断の記録である**（5.2 / 5.3 の段と同じ理由。
    /// `.kiro/steering/verification.md`「ログと記録を一次証拠にする」）。
    ///
    /// **観測できなかった項目は `None` である**（推測で埋めない。9.2 の段は「観測が成功し、
    /// 失敗したときに何が起きたかが記録から分かる」ことを求める）。
    Observation {
        /// 表示の指示（画面がドキュメントを持つことを知った瞬間）から、表が現れて面が描かれる
        /// までの経過（**ミリ秒**。要件 11.2 の予算は 1000）。
        first_screen_ms: Option<u32>,
        /// 末端までの走査のフレーム時間の中央値（**マイクロ秒**。要件 11.1 の予算は 16670）。
        scan_median_us: Option<u32>,
        /// 走査が到達した表示の序数（0 起点）。
        reached_row: Option<u32>,
        /// 表示の行数。
        row_count: Option<u32>,
        /// セルの編集の確定から反映までの経過（**ミリ秒**。要件 11.3 の予算は 100）。
        edit_ms: Option<u32>,
        /// 編集の取り消しの成否。
        undo: ObservationUndo,
        /// 描画が成立しなかったか（要件 12.2 の不成立条件の起動での観測）。
        paint_failed: bool,
        /// 面（canvas）から読めた色数（**読めなかったときは `None`**。`Some(0)` は「読めたが
        /// 一様である」ではなく、読めない状態を写した値である — 観測の画面は `None` を読めずに
        /// 使う）。
        colors: Option<u32>,
        /// **9.2 の筋書きの項目ごとの結果**（[`ObservationItemResult`]）。**観測しなかった項目は
        /// 載せない**（`Skipped` のような値を作らない — 「走らせなかった」と「走らせて駄目だった」を
        /// 記録の読み手が混同しないためである。段は自分が要求する項目を引数で名乗る）。
        items: Vec<ObservationItemResult>,
        /// **面（canvas）の国勢調査**（[`ObservationSurface`]）。`colors` が 0 のとき、
        /// **面が小さいのか、塗られていないのか、読めないのか**を 3 OS で切り分けるための実測で
        /// ある（CI の実測: macOS と Windows のランナーは `色数=0` を記録し、Linux は 2 以上を
        /// 記録した。切り分けの材料が無いと、原因の特定に CI の往復が要る）。
        surface: ObservationSurface,
    },
}

/// 9.2 の筋書きの項目（[`RenderHealthReport::Observation`]）。**閉じた列挙である。**
///
/// tasks.md 9.2 は「群 10 が閉じた経路を筋書きに含める」ことを求める（`10.1` 入れ子の展開のあとの
/// 走査、`10.5` 行の追加と取り消し・やり直し、`10.4` 並べ替えた表示での位置指定の追加と範囲の削除、
/// `10.6` 違反の理由、`10.3` 参照の一覧、`10.8` 貼り付けの往復、`10.2` シートを切り替えても
/// 取り消しが効く、`10.7` 文書の差し替えへの追随）。**単体テストが観測しない結線であり、実起動で
/// しか確かめられない。**項目の識別子を閉じた列挙にするのは、記録から読む段が綴りで分岐できる
/// ようにするためである（任意の文字列を運ばない規律は [`RenderHealthReport`] と同じ）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ObservationItem {
    /// 入れ子の列を展開したあとに走査する（10.1）。
    NestedExpansion,
    /// 行を追加して取り消し・やり直しをし、現在位置が対象の行へ移る（10.5）。
    InsertRow,
    /// 並べ替えた表示で位置を指定して行を追加し、範囲を選んで削除する（10.4）。
    SortThenDelete,
    /// 違反しているセルの理由を読む（10.6）。
    ViolationReason,
    /// 参照の列の面が行を一覧する（10.3）。
    ReferenceRows,
    /// 貼り付けがメニューの項目を経由して戻る（10.8。**ネイティブのメニューの活性化が要るため、
    /// 活性化できる段だけが要求する**）。
    PasteThroughMenu,
    /// シートを切り替えても取り消しが効く（10.2）。
    SheetSwitchUndo,
    /// 文書を差し替えたとき、表が古い行を残さずに追随する（10.7）。
    ReplaceDocument,
}

/// 筋書きの項目の結果（[`RenderHealthReport::Observation`]）。
///
/// **2 値である。**「観測しなかった」は結果ではなく**項目が載らないこと**で表す（項目の列挙に
/// 「未観測」を混ぜると、段が要求した項目の欠落を記録の読み手が見分けられなくなる）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ObservationItemOutcome {
    /// 成立した。
    Ok,
    /// 成立しなかった。
    Ng,
}

/// 筋書きの項目が**どこで止まったか**（[`ObservationItemResult`]）。**閉じた列挙である。**
///
/// **失敗の理由を人が読める形でしか残さないと、CI の 3 OS で診断できない** — 人が読む行は
/// 画面の `aria-label` にあり、それを読めるのは Linux のアクセシビリティの木だけである
/// （実測: Windows の段で「項目が ng」までしか分からず、原因の切り分けに 1 往復を要した）。
/// 記録へ**札**として載せ、3 OS の検査器が同じ形で読めるようにする。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ObservationItemReason {
    /// 押す入口（ボタン・項目）が無い。
    EntryMissing,
    /// 行数を読めなかった。
    RowCountUnreadable,
    /// 行数が期待どおりに変わらなかった。
    RowCountUnchanged,
    /// 行数が元に戻らなかった。
    RowCountNotRestored,
    /// 現在位置を読めなかった。
    PositionUnreadable,
    /// 現在位置が対象の行へ移らなかった。
    PositionNotMoved,
    /// 文書の状態を読めなかった（境界の往復が失敗した。**保持されていないこととは別である** —
    /// 遅い環境では往復が期限に間に合わないことがあり、原因の切り分けが変わる）。
    DocumentStateUnreadable,
    /// 文書が保持されていない（状態が `Open` でない）。
    DocumentNotOpen,
    /// シートが無い。
    SheetMissing,
    /// 参照の列が無い。
    ReferenceColumnMissing,
    /// 別のシートを開けなかった。
    SheetOpenFailed,
    /// 元のシートへ戻れなかった。
    SheetNotRestored,
    /// 並べ替えが表示へ反映されなかった。
    SortNotReflected,
    /// 範囲を選べなかった。
    SelectionEmpty,
    /// 違反の理由が読めなかった。
    ViolationReasonMissing,
    /// 同じ行の別の違反セルで理由が読めなかった。
    ViolationReasonNotRepeated,
    /// 面の色数が 2 に満たなかった（一様である）。
    SurfaceUniform,
    /// 面か窓の到着の数を読めなかった。
    SurfaceUnreadable,
    /// 窓の到着が増えなかった。
    ArrivalsUnchanged,
    /// 貼り付けが表へ届かなかった。
    PasteNotDelivered,
    /// 表が無い。
    TableMissing,
    /// 新しい文書を作れなかった。
    DocumentNewFailed,
    /// 表が新しい文書へ追随しなかった。
    DocumentNotFollowed,
    /// 未保存の変更を破棄できなかった。
    DiscardFailed,
    /// 参照の面が行を一覧しなかった。
    ReferenceNotListed,
    /// 項目が例外で止まった。
    Exception,
}

/// 筋書きの 1 項目の結果（[`RenderHealthReport::Observation`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct ObservationItemResult {
    /// どの項目か。
    pub item: ObservationItem,
    /// その結果。
    pub outcome: ObservationItemOutcome,
    /// 成立しなかったときの**止まった場所**（成立したときは `None`）。
    pub reason: Option<ObservationItemReason>,
}

/// 面（canvas）の国勢調査（[`RenderHealthReport::Observation`]）。**検証専用である。**
///
/// **要件の合否には使わない。** 12.2 の判定は「面に内容が描かれているか」であり、その実測が
/// [`RenderHealthReport::Observation::colors`] である。本型が運ぶのは、その数値が
/// **なぜその値になったか**を段の側で切り分けるための材料である:
///
/// - `container_width` / `container_height` が 0 なら、**表の器が寸法を持っていない**
/// - `pixel_ratio_milli` が 0 なら、**移植口が面へ大きさを与えられない**（画素比を掛けて実寸を
///   決める実装では、比が 0 だと面が 0 画素のまま残る）
/// - `canvas_count` が 0 なら、**面そのものが無い**（移植口の破綻）
/// - `first_width` / `first_height` が 0 なら、**製品の検査が読む面が大きさを持っていない**
///   （最も大きい面が 0 でなければ、**読む面を間違えている** — `largest_*` と比べる）
/// - `first_colors` / `largest_colors` が `Some(0)` なら**読めなかった**、`Some(1)` なら
///   **一様**、2 以上なら**内容が描かれている**
///
/// **画素比は 1000 倍の整数で運ぶ**（境界は 32 ビット以下の整数だけで構成する。
/// `ipc-contract.md`。比は 1.25 / 2 のような小数を取り得るため、マイクロ秒と同じ規律で整数へ
/// 持ち上げる — 丸めの経路を境界へ持ち込まない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct ObservationSurface {
    /// 表の器（`jxcel-grid-table` の親）の見た目の幅（**CSS 画素**）。
    pub container_width: u32,
    /// 表の器の見た目の高さ（**CSS 画素**）。
    pub container_height: u32,
    /// 器等の画素比（**1000 倍**。`window.devicePixelRatio` が 2 なら 2000）。
    pub pixel_ratio_milli: u32,
    /// 表の器の内側にある面（canvas）の数。
    pub canvas_count: u32,
    /// **先頭**の面（`querySelector("canvas")` が返す面 = 製品の検査が読む面）の実寸の幅（**画素**）。
    pub first_width: u32,
    /// 先頭の面の実寸の高さ（**画素**）。
    pub first_height: u32,
    /// 先頭の面から読めた色数（読めなかったときは `None`）。
    pub first_colors: Option<u32>,
    /// **最も大きい**面の実寸の幅（**画素**）。
    pub largest_width: u32,
    /// 最も大きい面の実寸の高さ（**画素**）。
    pub largest_height: u32,
    /// 最も大きい面から読めた色数（読めなかったときは `None`）。
    pub largest_colors: Option<u32>,
}

/// 編集の取り消しの成否（[`RenderHealthReport::Observation`]。要件 9.2 の往復）。
///
/// **3 値である。**「観測していない」は「成功した」でも「失敗した」でもない — 塗られない条件の
/// 起動では走査と編集を観測しない（12.2 の陽性の観測がその起動の目的である）。`bool` に潰すと、
/// 観測していないことを失敗として検査器へ見せることになる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ObservationUndo {
    /// 取り消しが成立した。
    Ok,
    /// 取り消しが成立しなかった。
    Ng,
    /// 観測していない。
    NotObserved,
}

/// 表の描画が成立しなかった理由の種別（タスク 9.3。要件 12.2）。**閉じた列挙である。**
///
/// 3 つは排他である: 面が無い（`NoCanvas`）／面はあるが塗って読み戻せない（`Unpaintable`）／
/// 面は塗れるが何も描かれていない（`Blank`）。**12.2 の「識別できる情報」の芯はこの札であり**、
/// 利用者へ見せる文言（「表の描画が成立しませんでした: …」）は画面が組み立てる。
///
/// 3 つ目が要るのは、塗って読み戻す検査（7.6 の `probePaint`）だけでは**「DOM はあるが何も
/// 塗られない」症状を捕まえられない**ためである — あの検査は自分が塗った画素を読む。7.6 が
/// 「面の内容を数える」口（`countDistinctColors`）を併せて公開した理由であり、1.6 の実測でも
/// 塗られた面は 52〜59 色、一様な面は 1 色だった。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum RenderPaintFailure {
    /// 表の面（canvas）が見つからない（移植口がまだ描き始めていない）。
    NoCanvas,
    /// 面に塗って読み戻せない（2D の文脈が取れない・読み戻しが失敗する・既知の色と違う・
    /// 塗ったのに alpha が 0 である）。
    Unpaintable,
    /// 面が一様である（何も描かれていない）。`colors` が数えた色数（1）である。
    Blank,
}

/// 描画の健全性の記録の要求（タスク 9.3。要件 12.2、12.3）。
///
/// 運ぶのは [`RenderHealthReport`] 1 つである。ウィンドウは要求の型に現れない — 呼び出し元は
/// 基盤が注入する `WebviewWindow` から取るため、フロントエンドが偽装する経路は存在しない
/// （要件 4.6、`ipc-contract.md`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct RenderHealthRecordRequest {
    /// 記録する事実。
    pub report: RenderHealthReport,
}

/// 描画の健全性の記録の応答（タスク 9.3。要件 12.3、4.6）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。記録は必ず行われる（記録できない場合は
/// 封筒の失敗腕になる）ので、結果を表す欄を別に持たない — 画面の提示（12.2 の告知）はこの
/// 往復の結果に依存しない（記録が失敗しても、描画が成立しなかったことは利用者に見えている）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct RenderHealthRecordResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
}

// ---------------------------------------------------------------------------
// マクロの面（タスク 4.3。マクロ実行の要件 1.3, 1.4, 1.6, 1.7, 2.1–2.6, 6.5, 8.2, 9.1–9.3）
// ---------------------------------------------------------------------------
//
// design.md「Data Contracts & Integration」の表が定める 4 つのコマンド（`macro_list` /
// `macro_store` / `macro_delete` / `macro_run`）の要求と応答を置く。**文字列と 32 ビット以下の
// 整数と真偽だけで構成し、他のドメインクレートの型を参照しない**（境界の型の規約。
// `crate::ipc::grid` のモジュール doc と同じ）。写すのは `src-tauri/src/commands/macro.rs` の
// 適応層である。
//
// 要求の型はどれも**ウィンドウを運ばない** — 呼び出し元は基盤が注入する `WebviewWindow` から
// 取る（偽装できない。要件 4.6、`ipc-contract.md`）。**上限（時間とメモリ）も運ばない** —
// 設定から適応層が解決する（要件 6.5。design.md「Data Contracts & Integration」）。

/// マクロの種別の札（要件 1.3, 3.1, 3.3）。
///
/// 綴りは**文書の中の形と同じ小文字**（`document-format` の `macros.json` の `kind` は
/// `typescript` / `javascript` の 2 つだけである。`crates/document-format/src/parts/macros_part.rs`）
/// であり、`macro_runtime::MacroKind::as_str`（診断の記録が使う安定トークン）とも同じである。
/// 3 つの綴りを揃えるのは、記録と文書と境界を突き合わせる検査を 1 つの語で書けるようにするため。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum MacroKindTag {
    /// TypeScript。実行の前に型注釈を落とす（要件 3.1）。
    TypeScript,
    /// JavaScript。そのまま実行する（要件 3.3）。
    JavaScript,
}

/// 宣言できる能力の札（要件 8.2, 8.4）。
///
/// **閉じた集合である**（`macro_runtime::Capability` の 3 値）。綴りは宣言に書く正準の綴り
/// そのままであり（`file.read` / `file.write` / `net`）、**そのまま利用者へ見せられる**
/// （要件 8.2 の「宣言している能力を提示する」は文言の組み立てを要さない）。
/// ファイルの読み込み・書き込み・ネットワークは**それぞれ別の能力**である（要件 8.4。
/// まとめて 1 つにしない — 読むだけのマクロに書く権利を与えない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS)]
pub enum MacroCapabilityTag {
    /// ファイルの読み込み。
    #[serde(rename = "file.read")]
    FileRead,
    /// ファイルの書き込み。
    #[serde(rename = "file.write")]
    FileWrite,
    /// ネットワークの利用。
    #[serde(rename = "net")]
    Net,
}

impl MacroCapabilityTag {
    /// 札の全体（**閉じた集合の唯一の源**）。並びは綴りの辞書順であり、
    /// `macro_runtime::Capability` の集合の提示順（`CapabilitySet` の順序）と同じである。
    ///
    /// 綴りと順序が一致することは `src-tauri` のテストが写像（`to_boundary`）を通して固定する
    /// （本クレートは `macro-runtime` に依存できないため、ここから数え合わせることはできない）。
    pub const ALL: [MacroCapabilityTag; 3] = [
        MacroCapabilityTag::FileRead,
        MacroCapabilityTag::FileWrite,
        MacroCapabilityTag::Net,
    ];
}

/// 失敗の種別の札（要件 2.4, 3.4, 8.3, 9.1, 9.2）。
///
/// `macro_runtime::FailureKind` の 4 層（ソースの解釈 / 変換 / 実行 / ホスト API の拒否）を
/// そのまま写す。**どの層で失敗したかによって提示と対処が変わる**ため、文字列を混ぜずに
/// 判別できる形で運ぶ。ホスト API の拒否だけが**拒んだ API の名前**を運ぶ（要件 9.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MacroFailureTag {
    /// ソースの解釈（種別とソースの不一致・能力宣言の誤り）。**実行そのものを行わない**。
    Source,
    /// 変換（TypeScript / JavaScript の構文誤り。要件 3.4）。
    Transpile,
    /// 実行（マクロが投げた例外。要件 9.1）。
    Execution,
    /// ホスト API の拒否（能力の宣言漏れ・存在しない行や列・読み取り専用の要求。要件 8.3, 9.2）。
    HostRejected {
        /// 拒んだ API の名前（宣言表の名前。例 `host.readRows`）。
        api: String,
    },
}

/// 失敗に至る呼び出しの 1 段（要件 9.1, 9.3）。
///
/// 行と列は **1 起点**であり、**TypeScript（保存されたソース）の原位置**である（要件 9.1 の
/// 「マクロのソースの行と列」。写しを通すのはエンジンである）。`function` は無名の位置では
/// **空文字**である（`Option` を境界へ出さない — 生成物の型が `string | null` になり、
/// 分岐を書く側に不要な場合分けが増える）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroFrame {
    /// どのマクロの位置か（実行した 1 件の名前。失敗が複数のモジュールに跨ることはない）。
    pub macro_name: String,
    /// 関数名（無名の位置では空文字）。
    pub function: String,
    /// 行（1 起点。保存されたソースの位置）。
    pub line: u32,
    /// 列（1 起点。保存されたソースの位置）。
    pub column: u32,
}

/// 失敗（要件 2.4, 9.1, 9.2, 9.3）。
///
/// **理由の文言は運ぶが、見せ方を決めない**（`structure.md` の「表示の文言を持たない」規約は
/// ドメインの型についてのものであり、境界は利用者に見せる材料を運ぶ）。`frames` は**内側
/// （投げた位置）から外側へ**並ぶ（要件 9.3 の呼び出しの並び）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroFailureReport {
    /// どの層で失敗したか（要件 9.2 の「失敗した API の名前」もここが運ぶ）。
    pub kind: MacroFailureTag,
    /// 失敗の理由（例外のメッセージ、拒否の理由、構文の診断）。
    pub reason: String,
    /// 失敗に至る呼び出しの並び（内側から外側へ。空でありうる）。
    pub frames: Vec<MacroFrame>,
}

/// `console` の出力の種別（要件 2.3）。
///
/// どの呼び出しだったかで提示の重みが変わる（`error` は警告として見せる等）ため、種別を持つ。
/// `macro_runtime::OutputLevel` の 5 値をそのまま写す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum MacroOutputLevel {
    /// `console.log`。
    Log,
    /// `console.info`。
    Info,
    /// `console.warn`。
    Warn,
    /// `console.error`。
    Error,
    /// `console.debug`。
    Debug,
}

/// マクロが出した出力の 1 行（要件 2.3）。
///
/// 並びが**順序を保つ**（マクロが出した順に読める）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroOutputLine {
    /// 呼び出しの種別。
    pub level: MacroOutputLevel,
    /// 出力の本文。
    pub text: String,
}

/// 変更の件数（種別ごと。要件 2.5, 2.6, 5.5）。
///
/// **件数は `u32` で運ぶ**（境界は 64 ビット整数を出さない。`ipc-contract.md`）。実行 1 回の
/// 変更が 40 億件を超えることは無い（超える場合は飽和させる — 件数は「何件変わったか」の
/// 提示であり、飽和しても「非常に多い」という意味は保たれる）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroChangeCounts {
    /// セルへ書き込んだ件数。
    pub set_cells: u32,
    /// 追加した行数。
    pub inserted_rows: u32,
    /// 削除した行数。
    pub removed_rows: u32,
    /// 複製した行数。
    pub duplicated_rows: u32,
}

impl MacroChangeCounts {
    /// 変更が 1 件も無いか（要件 2.6 の「変更の有無」）。
    pub const fn is_empty(&self) -> bool {
        self.set_cells == 0
            && self.inserted_rows == 0
            && self.removed_rows == 0
            && self.duplicated_rows == 0
    }

    /// 変更の合計（要件 5.5 の「書き込みの合計」）。
    pub const fn total(&self) -> u32 {
        self.set_cells + self.inserted_rows + self.removed_rows + self.duplicated_rows
    }
}

/// 打ち切りの種類（要件 6.1, 6.2）。
///
/// **どちらの上限に当たったか**が提示で変わるため、種別として運ぶ。綴りは
/// `macro_runtime::LimitKind::as_str`（診断の記録の語）と同じである。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum MacroAbortKind {
    /// 時間の上限（既定 30 秒。要件 6.1）。
    Time,
    /// メモリの上限（既定 512 MB。要件 6.2）。
    Memory,
}

/// 実行がどう終わったか（**3 値**。要件 2.3, 2.4, 6.1, 6.2）。
///
/// **打ち切りは失敗の一種ではなく別の値**である（要件 6.1 / 6.2 の提示が失敗と異なるため）。
/// `outcome` を判別子とする判別可能な合併型であり、フロントエンドは `switch` で網羅的に
/// 分岐できる（`src/ipc/client.ts` の `assertNever` が新しい変種をコンパイルエラーにする）。
///
/// **`Failed` / `Aborted` のとき、ドキュメントは変わっていない**（要件 6.3, 7.3）。変更の件数を
/// `Ran` だけが運ぶのは、その事実を型で表すためである（`macro_runtime::RunOutcome` と同じ）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "outcome")]
pub enum MacroRunOutcome {
    /// 最後まで走り切った（要件 2.3, 2.5）。
    Ran {
        /// 戻り値の提示用の表現（**オブジェクトは JSON**）。
        value: String,
        /// `console` の出力（順序を保つ）。
        output: Vec<MacroOutputLine>,
        /// 変更の件数（種別ごと。要件 2.5）。
        changes: MacroChangeCounts,
        /// 実行の所要（ミリ秒。要件 11.3）。
        elapsed_ms: u32,
    },
    /// 失敗して終わった（要件 2.4, 9.1）。
    Failed {
        /// 理由・種別・フレーム。
        failure: MacroFailureReport,
    },
    /// 上限で打ち切られた（要件 6.1, 6.2）。
    Aborted {
        /// どちらの上限に当たったか。
        limit: MacroAbortKind,
        /// 打ち切りまでの所要（ミリ秒）。
        elapsed_ms: u32,
        /// 打ち切りの理由とフレーム。
        failure: MacroFailureReport,
    },
}

/// 一覧に載る 1 件（要件 1.3, 1.4, 8.2）。
///
/// **解釈できなかったマクロも 1 件として載る**（要件 1.4）。そのとき `failure` が理由を持ち、
/// `capabilities` は空である。解釈できたマクロは `failure` が `None` であり、`capabilities` は
/// **宣言が 1 つも無ければ空**である（「解釈できたが何も宣言していない」と「解釈できなかった」は
/// `failure` の有無で区別できる）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroSummary {
    /// マクロの名前（要件 1.3）。
    pub name: String,
    /// マクロの種別（要件 1.3）。
    pub kind: MacroKindTag,
    /// 宣言されている能力（要件 8.2）。解釈できなかったときは空。
    pub capabilities: Vec<MacroCapabilityTag>,
    /// 解釈できなかった理由（要件 1.4）。解釈できたときは `None`。
    pub failure: Option<MacroFailureReport>,
}

/// マクロの一覧の応答（要件 1.3, 1.4）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。要求の型は無い — 必要な入力は
/// 対象ウィンドウだけで、それは基盤が注入する（[`CanCloseWindowResponse`] と同じ形）。
/// 並びは**保存された順**である（一覧の提示順。`design.md`「Logical Data Model」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroListResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// そのウィンドウのドキュメントが持つマクロ（保存順）。
    pub macros: Vec<MacroSummary>,
}

/// マクロの保存の要求（要件 1.1, 1.5, 1.6）。
///
/// `source` は**保存されたままのテキスト**である（整形しない。要件 1.5）。種別は閉じた札で
/// 運ぶため、`macros.json` の `kind` の綴り（`typescript` / `javascript`）以外は境界で弾かれる。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroStoreRequest {
    /// マクロの名前（ドキュメントの中で一意。同じ名前は置き換えである。要件 1.6）。
    pub name: String,
    /// マクロの種別。
    pub kind: MacroKindTag,
    /// 保存するソース（整形しない）。
    pub source: String,
}

/// マクロの保存の応答（要件 1.1, 1.6）。
///
/// `stored` は**この保存で書いた 1 件の要約**であり、`macros` は**保存後の一覧**である
/// （要件 1.3 の提示を、面が 2 度目の往復なしに差し替えられるようにする。保存が置き換えで
/// あったか追加であったかは `macros` の並びで分かる）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroStoreResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// この保存で書いた 1 件の要約。
    pub stored: MacroSummary,
    /// 保存後の一覧（保存順）。
    pub macros: Vec<MacroSummary>,
}

/// マクロの削除の要求（要件 1.7）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroDeleteRequest {
    /// 削除するマクロの名前。
    pub name: String,
}

/// マクロの削除の応答（要件 1.7）。
///
/// `removed` は**取り除いたか**である。無い名前を指定した場合は**失敗ではなく `false`** で
/// あり、一覧は削除前のまま返る（要件 1.7 は削除の結果についてだけ定めており、存在しない名前を
/// 誤りとする要求を持たない。押しても何も起きないことを面が区別できるようにする）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroDeleteResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// 取り除いたかどうか。
    pub removed: bool,
    /// 削除後の一覧（保存順）。
    pub macros: Vec<MacroSummary>,
}

/// マクロの実行の要求（要件 2.1）。
///
/// 運ぶのは名前だけである。**ソースも上限も運ばない** — ソースはドキュメントの中の記録であり
/// （要件 1.1）、上限は設定から適応層が解決する（要件 6.5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroRunRequest {
    /// 実行するマクロの名前（そのウィンドウのドキュメントに保存されているもの）。
    pub name: String,
}

/// マクロの実行の応答（要件 2.3, 2.4, 2.5, 6.1, 6.2）。
///
/// **失敗と打ち切りも成功の腕で運ぶ**（封筒の失敗腕は経路の失敗だけである — 実行できなかった
/// ことと、実行したが失敗したことは別である。`design.md`「Error Handling」の表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MacroRunResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// 実行の結果（3 値）。
    pub outcome: MacroRunOutcome,
}

/// マクロの実行がメニューから要求されたことを伝える Tauri イベントの名前（要件 2.1）。
///
/// `invoke` の宛先を持たないためコマンド名の配列（[`COMMAND_NAMES`]）には現れない。他のイベント
/// と同じく、**生成物（`src/ipc/bindings.ts`）へ定数として出す**ことで、フロントエンドが
/// 文字列リテラルを綴り間違える経路を塞ぐ。
///
/// **ペイロード型を持たない。** メニューの項目は引数を取らず（対象は「そのときのウィンドウの
/// ドキュメント」であり、選ばれたマクロは面が一覧から選ばせる。要件 2.1）、境界を越える値が
/// 1 つも無い（[`GRID_COPY_REQUESTED_EVENT`] と同じ形）。送り先は**活性化の対象ウィンドウ**
/// 1 つだけである。
///
/// **実行できるマクロが 1 つも無いときに項目を無効化しない**（要件 2.7 は面が担う）。
/// 有効・無効の述語はフォーカスが移るたびに評価されるが、その時点でドキュメントを読むと
/// **イベントループのスレッドが文書のロックを待つ**（実行中のマクロが 10 万行を書いていれば
/// 秒単位で待つ）— 要件 2.2 の「表の操作を止めない」に反する。面は一覧を既に持っているので、
/// 導線を出すかどうかは面が決める（`src-tauri/src/session/menu.rs` が同じ理由で述語を与えて
/// いない）。
pub const MACRO_RUN_REQUESTED_EVENT: &str = "macro_run_requested";

/// TypeScript の生成物を再生成する、唯一の文書化されたコマンド（タスク 2.2）。
///
/// 生成物のヘッダにもこの文字列を埋め込むため、定数として一箇所に持つ。実行ファイルは
/// `crates/app-shell/src/bin/generate-bindings.rs` であり、**リポジトリルートで**実行する。
pub const REGENERATE_BINDINGS_COMMAND: &str = "cargo run -p app-shell --bin generate-bindings";

/// 生成物のヘッダ。生成物であることと再生成の手段を明示する（tasks.md 2.2）。
fn generated_header() -> String {
    format!(
        "// このファイルは生成物である。**手で編集しない。**\n\
         // 型の宣言は crates/app-shell/src/ipc/ の定義から ts-rs が、コマンド名の定数は\n\
         // command_names.rs の COMMAND_NAMES が生成する。直すのは生成元である。\n\
         //\n\
         // 再生成（リポジトリルートで実行する）: {REGENERATE_BINDINGS_COMMAND}\n\
         // 本ファイルは追跡対象である。ドリフト検査（タスク 2.3）がバイト比較する。\n"
    )
}

/// コマンド名の定数を生成する。単一の源（[`command_names::COMMAND_NAMES`]）をそのまま写し、
/// 順序も変えない。
fn command_names_constant() -> String {
    let mut out = String::new();
    out.push('\n');
    out.push_str("/**\n");
    out.push_str(
        " * 境界を越えるコマンド名の一覧。`crates/app-shell/src/ipc/command_names.rs` の\n",
    );
    out.push_str(
        " * `COMMAND_NAMES` と同一の内容・同一の順序である。`src-tauri` のハンドラ登録と\n",
    );
    out.push_str(" * 本生成物が同じ配列を参照し、名前のドリフトを構造的に塞ぐ。\n");
    out.push_str(" */\n");
    out.push_str("export const COMMAND_NAMES = [\n");
    for name in command_names::COMMAND_NAMES {
        out.push_str("  \"");
        out.push_str(name);
        out.push_str("\",\n");
    }
    out.push_str("] as const;\n");
    out
}

/// イベント名の定数を生成する（タスク 7.1 / 9.5 / 3.1 / 8.7 / 8.9 / 10.8。要件 7.4、8.1、8.6、
/// 8.7、1.6、7.8、9.9）。
///
/// 設定変更の通知（[`SETTINGS_CHANGED_EVENT`]）・診断の導線の要求
/// （[`DIAGNOSTICS_REQUESTED_EVENT`]）・ドキュメントの状態変化
/// （[`DOCUMENT_SESSION_CHANGED_EVENT`]）・グリッドの複製の要求
/// （[`GRID_COPY_REQUESTED_EVENT`]）・グリッドの履歴の要求
/// （[`GRID_HISTORY_REQUESTED_EVENT`]）・グリッドの貼り付けの要求
/// （[`GRID_PASTE_REQUESTED_EVENT`]）は `invoke` の宛先を持たないため
/// [`command_names::COMMAND_NAMES`] には現れないが、**フロントエンドが文字列リテラルを
/// 綴り間違えない**ように、名前を生成物へ定数として出す。生成物はタスク 2.3 のドリフト検査が
/// バイト比較するので、名前の変更は生成のやり直しを強制する。
///
/// 並びは**この配列の順序**であり、イベント名を足す側はここへ 1 行足す（名前の一覧を
/// 生成物側で持たない）。
fn event_names_constant() -> String {
    let mut out = String::new();
    out.push('\n');
    out.push_str("/**\n");
    out.push_str(
        " * 境界を越えるイベント名の一覧。`crates/app-shell/src/ipc/mod.rs` の定義と同一で、\n",
    );
    out.push_str(" * フロントエンドはこの定数だけを参照する（文字列リテラルを書かない）。\n");
    out.push_str(" */\n");
    for (constant, event) in [
        ("SETTINGS_CHANGED_EVENT", SETTINGS_CHANGED_EVENT),
        ("DIAGNOSTICS_REQUESTED_EVENT", DIAGNOSTICS_REQUESTED_EVENT),
        (
            "DOCUMENT_SESSION_CHANGED_EVENT",
            DOCUMENT_SESSION_CHANGED_EVENT,
        ),
        ("GRID_COPY_REQUESTED_EVENT", GRID_COPY_REQUESTED_EVENT),
        ("GRID_HISTORY_REQUESTED_EVENT", GRID_HISTORY_REQUESTED_EVENT),
        ("GRID_PASTE_REQUESTED_EVENT", GRID_PASTE_REQUESTED_EVENT),
        ("MACRO_RUN_REQUESTED_EVENT", MACRO_RUN_REQUESTED_EVENT),
    ] {
        out.push_str(&format!("export const {constant} = \"{event}\";\n"));
    }
    out
}

/// 型 `T` の TypeScript 宣言を、JSDoc と `export` を付けて組み立て、整列用の型名と対で返す。
///
/// `TS::decl` はジェネリックな型でも `type IpcResult<T, E> = ...` の形を返すため、
/// ジェネリックな宣言もそのまま単一ファイルへ置ける。
fn declared<T: ts_rs::TS + 'static>(cfg: &ts_rs::Config) -> (String, String) {
    let mut text = String::new();
    if let Some(docs) = <T as ts_rs::TS>::docs() {
        text.push_str(&docs);
    }
    text.push_str("export ");
    text.push_str(&<T as ts_rs::TS>::decl(cfg));
    text.push('\n');
    (<T as ts_rs::TS>::ident(cfg), text)
}

/// 封筒の具体形。`IpcResult<T, E>` の宣言はジェネリックな形（`IpcResult<T, E>`）で出るため、
/// ペイロード型を名指しする具体形を明示的に生成する（2.1 の申し送り、tasks.md 2.2）。
///
/// ペイロード型を増やしたタスクは、同じ要領で具体形を足す（タスク 7.1 が
/// [`concrete_settings_result`] を足した）。
fn concrete_window_context_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "WindowContextResult";
    let mut text = String::from(
        "// コマンド応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指ししない\n\
         // ため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<WindowContext, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 終了可否の応答の具体形（タスク 7.6）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`CanCloseWindowResponse`] である。
fn concrete_can_close_window_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "CanCloseWindowResult";
    let mut text = String::from(
        "// 終了可否の問い合わせの応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<CanCloseWindowResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// ファイル選択の応答の具体形（タスク 7.7）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`PickDocumentFileResponse`] である。
fn concrete_pick_document_file_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "PickDocumentFileResult";
    let mut text = String::from(
        "// ファイル選択の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指し\n\
         // しないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<PickDocumentFileResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 関連付けの問い合わせの応答の具体形（タスク 9.6）。 [`concrete_window_context_result`] と
/// 同じ理由で置く。ペイロード型は [`WindowDocumentStateResponse`] である。
fn concrete_window_document_state_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "WindowDocumentStateResult";
    let mut text = String::from(
        "// 関連付けの問い合わせの応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<WindowDocumentStateResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 初回描画の通知の応答の具体形（タスク 8.2）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`RenderHeartbeatResponse`] である。
fn concrete_render_heartbeat_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "RenderHeartbeatResult";
    let mut text = String::from(
        "// 初回描画の通知の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<RenderHeartbeatResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 設定コマンドの応答の具体形（タスク 7.1）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`SettingsResponse`] である。
fn concrete_settings_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "SettingsResult";
    let mut text = String::from(
        "// 設定コマンドの応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指し\n\
         // しないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<SettingsResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 記録の保存場所の応答の具体形（タスク 9.5）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`DiagnosticsLogLocationResponse`] である。
fn concrete_diagnostics_log_location_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "DiagnosticsLogLocationResult";
    let mut text = String::from(
        "// 記録の保存場所の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<DiagnosticsLogLocationResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 記録の書き出しの応答の具体形（タスク 9.5）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`DiagnosticsExportResponse`] である。
fn concrete_diagnostics_export_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "DiagnosticsExportResult";
    let mut text = String::from(
        "// 記録の書き出しの応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<DiagnosticsExportResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 記録の詳細度の応答の具体形（タスク 9.5）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`DiagnosticsVerbosityResponse`] である（読み取りと変更で同じ形）。
fn concrete_diagnostics_verbosity_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "DiagnosticsVerbosityResult";
    let mut text = String::from(
        "// 記録の詳細度の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<DiagnosticsVerbosityResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// セッションの状態の問い合わせの応答の具体形（タスク 3.1）。 [`concrete_window_context_result`]
/// と同じ理由で置く。ペイロード型は [`DocumentStateResponse`] である。
fn concrete_document_state_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "DocumentStateResult";
    let mut text = String::from(
        "// セッションの状態の問い合わせの応答の具体形。ジェネリックな `IpcResult` の宣言は\n\
         // ペイロード型を名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<DocumentStateResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 保存の応答の具体形（タスク 3.1）。 [`concrete_window_context_result`] と同じ理由で置く。
/// ペイロード型は [`DocumentSaveResponse`] である。
fn concrete_document_save_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "DocumentSaveResult";
    let mut text = String::from(
        "// 保存の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指ししない\n\
         // ため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<DocumentSaveResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 新規作成の応答の具体形（タスク 3.1）。 [`concrete_window_context_result`] と同じ理由で置く。
/// ペイロード型は [`DocumentNewResponse`] である。
fn concrete_document_new_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "DocumentNewResult";
    let mut text = String::from(
        "// 新規作成の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指し\n\
         // しないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<DocumentNewResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 破棄の印の応答の具体形（タスク 3.1）。 [`concrete_window_context_result`] と同じ理由で置く。
/// ペイロード型は [`DocumentDiscardResponse`] である。
fn concrete_document_discard_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "DocumentDiscardResult";
    let mut text = String::from(
        "// 破棄の印の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指し\n\
         // しないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<DocumentDiscardResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// シートを開いた応答の具体形（タスク 6.2）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`GridOpenResponse`] である。
fn concrete_grid_open_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "GridOpenResult";
    let mut text = String::from(
        "// シートを開いた応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<GridOpenResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 表示の指定を変えた応答の具体形（タスク 6.2）。 [`concrete_window_context_result`] と同じ
/// 理由で置く。ペイロード型は [`GridViewResponse`] である。
fn concrete_grid_view_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "GridViewResult";
    let mut text = String::from(
        "// 表示の指定を変えた応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<GridViewResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 編集の結果の応答の具体形（タスク 6.2）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`GridEditResponse`] であり、**適用と履歴の 2 つのコマンドが同じ形を
/// 返す**（`design.md`「GridCommands」の API Contract）ため、具体形も 1 つで足りる。
fn concrete_grid_edit_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "GridEditResult";
    let mut text = String::from(
        "// 編集の結果の応答の具体形（適用と履歴で同じ形）。ジェネリックな `IpcResult` の宣言は\n\
         // ペイロード型を名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<GridEditResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 次の違反を探した結果の具体形（タスク 6.2）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`GridViolationResponse`] である。
fn concrete_grid_violation_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "GridViolationResult";
    let mut text = String::from(
        "// 次の違反を探した結果の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<GridViolationResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 描画の健全性の記録の応答の具体形（タスク 9.3）。 [`concrete_window_context_result`] と
/// 同じ理由で置く。ペイロード型は [`RenderHealthRecordResponse`] である。
fn concrete_render_health_record_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "RenderHealthRecordResult";
    let mut text = String::from(
        "// 描画の健全性の記録の応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<RenderHealthRecordResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 参照先の行を読んだ結果の具体形（タスク 10.3）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`GridReferenceResponse`] である。
fn concrete_grid_reference_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "GridReferenceResult";
    let mut text = String::from(
        "// 参照先の行を読んだ結果の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を\n\
         // 名指ししないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<GridReferenceResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// マクロの 4 つのコマンドの封筒の具体形（タスク 4.3）。
///
/// [`concrete_window_context_result`] と同じ理由で置く（ジェネリックな `IpcResult` の宣言は
/// ペイロード型を名指ししないため、境界が名指しできる具体形を明示的に置く）。4 本を 1 つの
/// 関数にまとめるのは、写しが 4 つ並ぶ同じ形の繰り返しであり、**その形が 4 本とも同じである
/// こと**が読み取れるようにするためである。
fn concrete_macro_results(cfg: &ts_rs::Config) -> [(String, String); 4] {
    [
        (
            "MacroListResult".to_owned(),
            concrete_result::<MacroListResponse>(cfg, "MacroListResult"),
        ),
        (
            "MacroStoreResult".to_owned(),
            concrete_result::<MacroStoreResponse>(cfg, "MacroStoreResult"),
        ),
        (
            "MacroDeleteResult".to_owned(),
            concrete_result::<MacroDeleteResponse>(cfg, "MacroDeleteResult"),
        ),
        (
            "MacroRunResult".to_owned(),
            concrete_result::<MacroRunResponse>(cfg, "MacroRunResult"),
        ),
    ]
}

/// ペイロード型 `T` のコマンドの封筒の具体形を組み立てる（上の 4 本が共有する実体）。
fn concrete_result<T: ts_rs::TS>(cfg: &ts_rs::Config, name: &str) -> String {
    format!(
        "// マクロのコマンド（{name}）の封筒の具体形。ジェネリックな `IpcResult` の宣言は\n\
         // ペイロード型を名指ししないため、境界が名指しできる具体形を明示的に置く。\n\
         export type {name} = {};\n",
        <IpcResult<T, IpcError> as ts_rs::TS>::name(cfg)
    )
}

/// 境界を越える型とコマンド名から、追跡対象の TypeScript（`src/ipc/bindings.ts`）を生成する
/// （tasks.md 2.2、design.md「IpcContract」の Service Interface）。
///
/// 同一の入力から常に同一のバイト列を返す。時刻・絶対パス・ホスト名・走査順に依存する内容を
/// 含めず、宣言は TypeScript の型名で整列するため、型の追加や削除でも順序が揺れない。
///
/// `Result` を返すのは design.md の署名に合わせるためである。現在の生成経路は
/// `TS::decl` / `TS::ident` / `TS::name` だけで完結して失敗しないが、`export_to_string` を
/// 要する型が境界へ加わったときに失敗を伝えられる形を保つ。
pub fn render_bindings() -> Result<String, ts_rs::ExportError> {
    let cfg = ts_rs::Config::default();

    let mut declarations = vec![
        declared::<WindowLabel>(&cfg),
        declared::<WindowContext>(&cfg),
        declared::<SettingsValue>(&cfg),
        declared::<SettingsGetRequest>(&cfg),
        declared::<SettingsSetRequest>(&cfg),
        declared::<SettingsResponse>(&cfg),
        declared::<SettingsChangedEvent>(&cfg),
        declared::<WindowCloseVerdict>(&cfg),
        declared::<CanCloseWindowResponse>(&cfg),
        declared::<DocumentPickOutcome>(&cfg),
        declared::<PickDocumentFileResponse>(&cfg),
        declared::<WindowDocumentState>(&cfg),
        declared::<WindowDocumentStateResponse>(&cfg),
        declared::<RenderVerdict>(&cfg),
        declared::<RenderHeartbeatRequest>(&cfg),
        declared::<RenderHeartbeatResponse>(&cfg),
        declared::<DiagnosticsLevel>(&cfg),
        declared::<DiagnosticsSection>(&cfg),
        declared::<DiagnosticsRequestedEvent>(&cfg),
        declared::<DiagnosticsLogLocationResponse>(&cfg),
        declared::<DiagnosticsExportRecords>(&cfg),
        declared::<DiagnosticsExportResponse>(&cfg),
        declared::<DiagnosticsVerbosityResponse>(&cfg),
        declared::<DiagnosticsVerbositySetRequest>(&cfg),
        declared::<DocumentOrigin>(&cfg),
        declared::<DocumentSheet>(&cfg),
        declared::<DocumentSummary>(&cfg),
        declared::<DocumentSessionStatus>(&cfg),
        declared::<DocumentStateResponse>(&cfg),
        declared::<DocumentSaveOutcome>(&cfg),
        declared::<DocumentSaveResponse>(&cfg),
        declared::<DocumentNewOutcome>(&cfg),
        declared::<DocumentNewResponse>(&cfg),
        declared::<DocumentDiscardResponse>(&cfg),
        declared::<TypeKindTag>(&cfg),
        declared::<ColumnExpandability>(&cfg),
        declared::<ColumnElementCount>(&cfg),
        declared::<ColumnChoice>(&cfg),
        declared::<ColumnMemberDescriptor>(&cfg),
        declared::<ColumnDescriptor>(&cfg),
        declared::<GridPathSegment>(&cfg),
        declared::<GridSheetSummary>(&cfg),
        declared::<GridSortKey>(&cfg),
        declared::<GridFilterSpec>(&cfg),
        declared::<GridExpansionState>(&cfg),
        declared::<GridViewSpec>(&cfg),
        declared::<GridCellAddress>(&cfg),
        declared::<GridCellEdit>(&cfg),
        declared::<GridRowTarget>(&cfg),
        declared::<GridRowAnchor>(&cfg),
        declared::<GridEditCommand>(&cfg),
        declared::<GridCoercionNotice>(&cfg),
        declared::<GridViolationLocation>(&cfg),
        declared::<GridEditOutcome>(&cfg),
        declared::<GridOpenRequest>(&cfg),
        declared::<GridOpenResponse>(&cfg),
        declared::<GridViewRequest>(&cfg),
        declared::<GridViewResponse>(&cfg),
        declared::<GridEditRequest>(&cfg),
        declared::<GridEditResponse>(&cfg),
        declared::<GridHistoryDirection>(&cfg),
        declared::<GridHistoryRequest>(&cfg),
        declared::<GridHistoryRequestedEvent>(&cfg),
        declared::<GridPasteRequestedEvent>(&cfg),
        declared::<GridSearchDirection>(&cfg),
        declared::<GridViolationRequest>(&cfg),
        declared::<GridViolation>(&cfg),
        declared::<GridViolationResponse>(&cfg),
        declared::<GridReferenceRequest>(&cfg),
        declared::<GridReferenceRow>(&cfg),
        declared::<GridReferenceResponse>(&cfg),
        declared::<RenderPaintFailure>(&cfg),
        declared::<ObservationUndo>(&cfg),
        declared::<ObservationItem>(&cfg),
        declared::<ObservationItemOutcome>(&cfg),
        declared::<ObservationItemResult>(&cfg),
        declared::<ObservationSurface>(&cfg),
        declared::<ObservationItemReason>(&cfg),
        declared::<RenderHealthReport>(&cfg),
        declared::<RenderHealthRecordRequest>(&cfg),
        declared::<RenderHealthRecordResponse>(&cfg),
        declared::<MacroKindTag>(&cfg),
        declared::<MacroCapabilityTag>(&cfg),
        declared::<MacroFailureTag>(&cfg),
        declared::<MacroFrame>(&cfg),
        declared::<MacroFailureReport>(&cfg),
        declared::<MacroOutputLevel>(&cfg),
        declared::<MacroOutputLine>(&cfg),
        declared::<MacroChangeCounts>(&cfg),
        declared::<MacroAbortKind>(&cfg),
        declared::<MacroRunOutcome>(&cfg),
        declared::<MacroSummary>(&cfg),
        declared::<MacroListResponse>(&cfg),
        declared::<MacroStoreRequest>(&cfg),
        declared::<MacroStoreResponse>(&cfg),
        declared::<MacroDeleteRequest>(&cfg),
        declared::<MacroDeleteResponse>(&cfg),
        declared::<MacroRunRequest>(&cfg),
        declared::<MacroRunResponse>(&cfg),
        declared::<IpcError>(&cfg),
        declared::<IpcResult<WindowContext, IpcError>>(&cfg),
        concrete_window_context_result(&cfg),
        concrete_settings_result(&cfg),
        concrete_can_close_window_result(&cfg),
        concrete_pick_document_file_result(&cfg),
        concrete_window_document_state_result(&cfg),
        concrete_render_heartbeat_result(&cfg),
        concrete_diagnostics_log_location_result(&cfg),
        concrete_diagnostics_export_result(&cfg),
        concrete_diagnostics_verbosity_result(&cfg),
        concrete_document_state_result(&cfg),
        concrete_document_save_result(&cfg),
        concrete_document_new_result(&cfg),
        concrete_document_discard_result(&cfg),
        concrete_grid_open_result(&cfg),
        concrete_grid_view_result(&cfg),
        concrete_grid_edit_result(&cfg),
        concrete_grid_violation_result(&cfg),
        concrete_grid_reference_result(&cfg),
        concrete_render_health_record_result(&cfg),
    ];
    declarations.extend(concrete_macro_results(&cfg));
    declarations.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = generated_header();
    out.push_str(&command_names_constant());
    out.push_str(&event_names_constant());
    out.push_str(
        "\n// ---------------------------------------------------------------------------\n\
         // 境界を越える型（crates/app-shell/src/ipc/ の定義から ts-rs が生成）\n\n",
    );
    for (_, declaration) in declarations {
        out.push_str(&declaration);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::error::IpcError;
    use super::*;
    use ts_rs::TS;

    /// 境界を越える型の TypeScript 宣言を文字列として得る。`export_to_string` はファイルを
    /// 書かないため、テストから副作用がない。
    fn generated<T: TS + 'static>() -> String {
        T::export_to_string(&ts_rs::Config::default())
            .expect("境界を越える型は TypeScript へ生成できなければならない")
    }

    /// 生成物からコメントを取り除く。ts-rs は Rust の `///` を JSDoc（`/** … */`）として
    /// 出力するため、説明文がそのまま型宣言の隣に並ぶ。
    ///
    /// **違反を判定してよいのは型の位置だけである。** 要件 4.2 / 4.4 が問題にしているのは
    /// 型の表現であり、research.md 決定 1 が `tauri-specta` を却下した理由（`e as any` /
    /// `payload: any` / `Event<any>`）も、フロントエンドの `@typescript-eslint/no-explicit-any`
    /// が見るのも型の位置である。説明文に `any` や `number` が現れても逸反ではないので、
    /// 走査の前にコメントを落とす。
    ///
    /// 文字列リテラルの中の `//` をコメントと誤認しないよう、引用符の状態を追う。
    fn strip_comments(ts: &str) -> String {
        let mut out = String::with_capacity(ts.len());
        let mut chars = ts.chars().peekable();
        let mut quote: Option<char> = None;
        while let Some(c) = chars.next() {
            if let Some(q) = quote {
                out.push(c);
                if c == '\\' {
                    if let Some(escaped) = chars.next() {
                        out.push(escaped);
                    }
                } else if c == q {
                    quote = None;
                }
                continue;
            }
            match c {
                '"' | '\'' | '`' => {
                    quote = Some(c);
                    out.push(c);
                }
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    let mut prev = '\0';
                    for c in chars.by_ref() {
                        if prev == '*' && c == '/' {
                            break;
                        }
                        prev = c;
                    }
                    // 前後の識別子が連結して見えないよう空白で置き換える。
                    out.push(' ');
                }
                '/' if chars.peek() == Some(&'/') => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                    out.push(' ');
                }
                _ => out.push(c),
            }
        }
        out
    }

    /// 型の位置に現れた `token` を単語境界で探す。識別子の一部（`NumberOfRows` の `number`、
    /// `anyCount` の `any`）は型ではないので拾わない。JSDoc の中身は [`strip_comments`] で
    /// 落としてから走査する。
    fn assert_no_type_token(ts: &str, token: &str) {
        let code = strip_comments(ts);
        let mut from = 0;
        while let Some(rel) = code[from..].find(token) {
            let at = from + rel;
            let before = code[..at].chars().next_back();
            let after = code[at + token.len()..].chars().next();
            let is_ident =
                |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$');
            assert!(
                is_ident(before) || is_ident(after),
                "生成物の型の位置に {token} が現れた:\n{code}"
            );
            from = at + token.len();
        }
    }

    /// 生成物の型の位置に `any` が現れないことを検査する。
    fn assert_no_any(ts: &str) {
        assert_no_type_token(ts, "any");
    }

    /// 生成物の型の位置に 64 ビット整数由来の数値型が現れないことを検査する。
    fn assert_no_numeric_type(ts: &str) {
        assert_no_type_token(ts, "number");
        assert_no_type_token(ts, "bigint");
    }

    /// 数値が 1 つでも現れたら失敗する。境界の型が 64 ビット整数を露出していれば、
    /// JSON では `Number` として現れる。
    fn assert_no_json_number(value: &serde_json::Value, at: &str) {
        match value {
            serde_json::Value::Number(n) => panic!("{at} に数値が露出している: {n}"),
            serde_json::Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    assert_no_json_number(item, &format!("{at}[{i}]"));
                }
            }
            serde_json::Value::Object(fields) => {
                for (name, item) in fields {
                    assert_no_json_number(item, &format!("{at}.{name}"));
                }
            }
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::String(_) => {
            }
        }
    }

    #[test]
    fn generated_bindings_contain_no_any() {
        assert_no_any(&generated::<IpcResult<WindowContext, IpcError>>());
        assert_no_any(&generated::<IpcError>());
        assert_no_any(&generated::<WindowContext>());
        assert_no_any(&generated::<WindowLabel>());
        assert_no_numeric_type(&generated::<IpcResult<WindowContext, IpcError>>());
        assert_no_numeric_type(&generated::<IpcError>());
        assert_no_numeric_type(&generated::<WindowContext>());
        assert_no_numeric_type(&generated::<WindowLabel>());
    }

    /// 検出器そのものが本物の `any` を落とすことを固定する。これが無いと、判定が常に通っても
    /// 誰も気づけない。
    #[test]
    #[should_panic(expected = "any")]
    fn any_detector_rejects_a_real_any() {
        assert_no_any("export type Value = any;");
    }

    /// 数値型についても同じ対称性を固定する。
    #[test]
    #[should_panic(expected = "number")]
    fn numeric_detector_rejects_a_real_number() {
        assert_no_numeric_type("export type Id = number;");
    }

    #[test]
    #[should_panic(expected = "bigint")]
    fn numeric_detector_rejects_a_real_bigint() {
        assert_no_numeric_type("export type Id = bigint;");
    }

    /// `any` を部分文字列として含む識別子を誤検出しないことを固定する。
    #[test]
    fn detectors_ignore_identifiers_containing_the_tokens() {
        assert_no_any(
            "export type Company = { many: string, anything: string, anyCount: string, }",
        );
        assert_no_numeric_type(
            "export type RowCount = { numberOfRows: string, bigintValue: string, }",
        );
    }

    /// 説明文に現れただけの語は違反ではないことを固定する。ts-rs は `///` を JSDoc として
    /// 出力するため、これが無いと「文書を書くとテストが落ちる」状態に戻る。
    #[test]
    fn detectors_ignore_tokens_that_appear_only_in_comments() {
        let ts = concat!(
            "// This value must not be any arbitrary JavaScript number.\n",
            "/**\n",
            " * TypeScript の number へは落ちない。bigint でも、any でもない。\n",
            " */\n",
            "export type WindowLabel = string;\n",
        );
        assert_no_any(ts);
        assert_no_numeric_type(ts);
    }

    /// コメントを落とした後も型の位置は残ることを固定する（除去が過剰でないことの確認）。
    #[test]
    #[should_panic(expected = "number")]
    fn comment_stripping_keeps_type_positions() {
        assert_no_numeric_type("// number\nexport type Id = number;");
    }

    #[test]
    fn envelope_narrows_on_status() {
        let ts = generated::<IpcResult<WindowContext, IpcError>>();
        assert!(ts.contains("\"status\": \"ok\""), "{ts}");
        assert!(ts.contains("\"status\": \"error\""), "{ts}");
    }

    #[test]
    fn error_distinguishes_its_causes() {
        let ts = generated::<IpcError>();
        for kind in ["Settings", "Sidecar", "Window"] {
            assert!(ts.contains(&format!("\"kind\": \"{kind}\"")), "{ts}");
        }
    }

    #[test]
    fn identifier_is_a_string_that_keeps_the_exact_value() {
        let ts = generated::<WindowLabel>();
        assert!(ts.contains("type WindowLabel = string;"), "{ts}");
        // 型の位置に数値型が無いこと。説明文（JSDoc）の中身は対象外である。
        assert_no_numeric_type(&ts);
        assert_no_any(&ts);
        let ts = generated::<WindowContext>();
        assert!(ts.contains("window: WindowLabel"), "{ts}");
        // f64 では正確に表せない値。文字列表現ならそのまま往復する。
        let raw = "18446744073709551615";
        let value = serde_json::to_value(WindowLabel::new(raw)).unwrap();
        assert_eq!(value, serde_json::Value::String(raw.into()));
        let back: WindowLabel = serde_json::from_value(value).unwrap();
        assert_eq!(back.as_str(), raw);
    }

    #[test]
    fn boundary_types_expose_no_number() {
        let envelope: IpcResult<WindowContext, IpcError> = IpcResult::Ok {
            data: WindowContext {
                window: WindowLabel::new("doc-0"),
            },
        };
        assert_no_json_number(&serde_json::to_value(&envelope).unwrap(), "envelope");

        let err: IpcResult<WindowContext, IpcError> = IpcResult::Err {
            error: IpcError::Sidecar {
                message: "起動できない".into(),
            },
        };
        assert_no_json_number(&serde_json::to_value(&err).unwrap(), "error");
    }

    #[test]
    fn error_payload_is_not_a_bare_string() {
        let causes = [
            IpcError::Settings {
                message: "設定を読めない".into(),
            },
            IpcError::Sidecar {
                message: "起動できない".into(),
            },
            IpcError::Window {
                message: "生成できない".into(),
            },
        ];
        let mut kinds = std::collections::BTreeSet::new();
        for cause in &causes {
            let value = serde_json::to_value(cause).unwrap();
            let object = value
                .as_object()
                .expect("エラーは原因の情報を持つ対象である");
            assert!(kinds.insert(object["kind"].as_str().unwrap().to_owned()));
            // 詳細は `detail` の下の対象であり、裸の文字列ではない。
            let detail = object["detail"].as_object().expect("詳細は対象である");
            assert!(detail["message"].is_string(), "{value}");
        }
        assert_eq!(kinds.len(), causes.len());
    }

    #[test]
    fn envelope_round_trips_both_arms() {
        let ok: IpcResult<WindowContext, IpcError> = IpcResult::Ok {
            data: WindowContext {
                window: WindowLabel::new("doc-0"),
            },
        };
        let value = serde_json::to_value(&ok).unwrap();
        assert_eq!(value["status"], serde_json::Value::String("ok".into()));
        assert!(value.get("data").is_some());
        assert_eq!(
            serde_json::from_value::<IpcResult<WindowContext, IpcError>>(value).unwrap(),
            ok
        );

        let err: IpcResult<WindowContext, IpcError> = IpcResult::Err {
            error: IpcError::Window {
                message: "生成できない".into(),
            },
        };
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(value["status"], serde_json::Value::String("error".into()));
        assert!(value.get("error").is_some());
        assert_eq!(
            serde_json::from_value::<IpcResult<WindowContext, IpcError>>(value).unwrap(),
            err
        );
    }

    // ------------------------------------------------------------------
    // 2.2: コマンド名の単一配列と TypeScript 生成
    // ------------------------------------------------------------------

    /// 生成物の `COMMAND_NAMES` の配列リテラルから、要素の文字列を順に取り出す。
    ///
    /// 全文一致ではなく配列の中の文字列リテラルだけを取り出すため、整形（インデントや改行）が
    /// 変わっても「同一の内容・同一の順序」という性質だけを検査できる。名前は snake_case で
    /// 引用符を含まないため、素朴な走査で足りる。
    fn emitted_command_names(ts: &str) -> Vec<String> {
        let start = ts
            .find("export const COMMAND_NAMES = [")
            .expect("生成物にコマンド名の定数が必要である");
        let rest = &ts[start..];
        let open = rest.find('[').expect("コマンド名の配列が開かれていない");
        let close = rest.find(']').expect("コマンド名の配列が閉じられていない");
        assert!(open < close, "配列の開始が終了より後にある");
        let mut names = Vec::new();
        let mut chars = rest[open + 1..close].chars();
        while let Some(c) = chars.next() {
            if c != '"' {
                continue;
            }
            let mut name = String::new();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                name.push(c);
            }
            names.push(name);
        }
        names
    }

    #[test]
    fn bindings_are_byte_deterministic() {
        let first = render_bindings().expect("境界の型から TypeScript を生成できなければならない");
        let second = render_bindings().expect("境界の型から TypeScript を生成できなければならない");
        assert!(!first.is_empty(), "生成物が空である");
        assert_eq!(
            first, second,
            "同一の入力から常に同一のバイト列が出なければならない"
        );
    }

    #[test]
    fn bindings_mirror_command_names_in_order() {
        let ts = render_bindings().unwrap();
        let emitted = emitted_command_names(&ts);
        let expected = COMMAND_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            emitted, expected,
            "生成物のコマンド名は COMMAND_NAMES と同一の内容・同一の順序でなければならない"
        );
    }

    #[test]
    fn command_names_are_unique_and_well_formed() {
        assert!(!COMMAND_NAMES.is_empty(), "コマンド名の配列を空にしない");
        let mut seen = std::collections::BTreeSet::new();
        for name in COMMAND_NAMES {
            assert!(!name.is_empty(), "空のコマンド名を置かない");
            assert!(
                name.chars().next().is_some_and(|c| c.is_ascii_lowercase()),
                "コマンド名は英小文字で始める: {name}"
            );
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "コマンド名は snake_case とする: {name}"
            );
            assert!(seen.insert(*name), "コマンド名が重複している: {name}");
        }
    }

    /// 生成物に型の位置の `any` が無いこと、および **64 ビット整数の数値型が現れない**こと。
    /// 生成物の全体（境界の型の合算）をここで検査する。
    ///
    /// **`number` は禁止しない。** ドキュメントのセッションの境界は、シートの件数を `u32` で
    /// 運ぶ（design.md「Data Contracts & Integration」）。`u32` は IEEE 754 の倍精度で正確に
    /// 表せるので、JavaScript の `number` へ落ちても値を取り違えない。禁止しているのは
    /// `i64` / `u64` / `i128` / `u128` であり、ts-rs はこれらを `bigint` へ落とす
    /// （`WindowLabel` が識別子を文字列で運ぶのと同じ理由。2.1 の不変条件）。
    /// 数値を含まない型については、従来どおり `assert_no_numeric_type` を個別に当てる。
    #[test]
    fn bindings_contain_no_any_and_no_64_bit_numbers() {
        let ts = render_bindings().unwrap();
        assert_no_any(&ts);
        assert_no_type_token(&ts, "bigint");
    }

    #[test]
    fn bindings_declare_every_boundary_type() {
        let ts = render_bindings().unwrap();
        for declaration in [
            "export type WindowLabel = string;",
            "export type IpcError =",
            "export type IpcResult<T, E> =",
            "export type WindowContext =",
            "export type SettingsValue = unknown;",
            "export type SettingsGetRequest =",
            "export type SettingsSetRequest =",
            "export type SettingsResponse =",
            "export type SettingsChangedEvent =",
        ] {
            assert!(
                ts.contains(declaration),
                "生成物に `{declaration}` が無い:\n{ts}"
            );
        }
    }

    /// 封筒の宣言はジェネリックな形で出るため、ペイロード型を名指しする具体形を別途出力して
    /// いることを固定する（2.1 の申し送り）。
    #[test]
    fn bindings_name_the_payload_types_of_the_envelope() {
        let ts = render_bindings().unwrap();
        assert!(
            ts.contains("export type WindowContextResult = IpcResult<WindowContext, IpcError>;"),
            "封筒の具体形がペイロード型を名指ししていない:\n{ts}"
        );
        assert!(
            ts.contains("export type SettingsResult = IpcResult<SettingsResponse, IpcError>;"),
            "設定コマンドの封筒の具体形がペイロード型を名指ししていない:\n{ts}"
        );
    }

    /// 設定の境界の型がタスク 7.1 の契約どおりであることを固定する。
    #[test]
    fn settings_boundary_types_round_trip_the_value() {
        let request = SettingsSetRequest {
            key: "appearance.theme".to_owned(),
            value: SettingsValue::new(serde_json::Value::String("dark".to_owned())),
        };
        let encoded = serde_json::to_value(&request).unwrap();
        // 新定型は内側の値そのものとして直列化される（`{"key":...,"value":"dark"}`）。
        assert_eq!(
            encoded["value"],
            serde_json::Value::String("dark".to_owned())
        );
        let back: SettingsSetRequest = serde_json::from_value(encoded).unwrap();
        assert_eq!(back, request);

        let response = SettingsResponse {
            context: WindowContext {
                window: WindowLabel::new("empty-1"),
            },
            key: "appearance.theme".to_owned(),
            value: Some(SettingsValue::new(serde_json::Value::Bool(true))),
        };
        let encoded = serde_json::to_value(&response).unwrap();
        // 応答は呼び出し元ウィンドウの識別子を文字列で運ぶ（要件 4.6）。
        assert_eq!(
            encoded["context"]["window"],
            serde_json::Value::String("empty-1".into())
        );
        assert_eq!(encoded["value"], serde_json::Value::Bool(true));

        let changed = SettingsChangedEvent {
            key: "window.geometry".to_owned(),
            value: SettingsValue::new(
                serde_json::json!({ "x": 1, "y": 2, "width": 3, "height": 4 }),
            ),
        };
        let encoded = serde_json::to_value(&changed).unwrap();
        assert_eq!(
            encoded["key"],
            serde_json::Value::String("window.geometry".into())
        );
        assert_eq!(encoded["value"]["width"], serde_json::json!(3));
    }

    /// マクロの面の境界の型が契約どおりであることを固定する（タスク 4.3。要件 1.4, 2.3, 9.1）。
    ///
    /// 見るのは 3 点である: **解釈できなかった理由が一覧に載る**こと（要件 1.4）、**3 値が
    /// 判別子で区別できる**こと（要件 2.3, 2.4, 6.1, 6.2）、**フレームが位置を運ぶ**こと
    /// （要件 9.1, 9.3）。閉じた札の綴り（種別・能力・打ち切り）も同時に固定する — 綴りは
    /// 文書の中の形（`macros.json` の `kind`）と診断の記録の語に一致していなければならない。
    #[test]
    fn macro_boundary_types_carry_the_reason_and_the_three_outcomes() {
        // 種別と能力の綴り（文書の中の形と同じ）。
        assert_eq!(
            serde_json::to_value(MacroKindTag::TypeScript).unwrap(),
            serde_json::json!("typescript")
        );
        assert_eq!(
            serde_json::to_value(MacroKindTag::JavaScript).unwrap(),
            serde_json::json!("javascript")
        );
        assert_eq!(
            MacroCapabilityTag::ALL
                .iter()
                .map(|tag| serde_json::to_value(tag).unwrap())
                .collect::<Vec<_>>(),
            vec![
                serde_json::json!("file.read"),
                serde_json::json!("file.write"),
                serde_json::json!("net"),
            ],
            "能力の綴りが宣言の綴りと違う"
        );

        // 一覧: 解釈できなかった 1 件が理由つきで残る（要件 1.4）。
        let response = MacroListResponse {
            context: WindowContext {
                window: WindowLabel::new("doc-1"),
            },
            macros: vec![
                MacroSummary {
                    name: "棚卸し".to_owned(),
                    kind: MacroKindTag::TypeScript,
                    capabilities: vec![MacroCapabilityTag::FileRead, MacroCapabilityTag::Net],
                    failure: None,
                },
                MacroSummary {
                    name: "書きかけ".to_owned(),
                    kind: MacroKindTag::JavaScript,
                    capabilities: Vec::new(),
                    failure: Some(MacroFailureReport {
                        kind: MacroFailureTag::Transpile,
                        reason: "expected expression".to_owned(),
                        frames: vec![MacroFrame {
                            macro_name: "書きかけ".to_owned(),
                            function: String::new(),
                            line: 3,
                            column: 7,
                        }],
                    }),
                },
            ],
        };
        let encoded = serde_json::to_value(&response).unwrap();
        assert_eq!(
            encoded["context"]["window"],
            serde_json::Value::String("doc-1".into())
        );
        assert_eq!(
            encoded["macros"][1]["failure"]["kind"]["kind"],
            serde_json::json!("transpile")
        );
        assert_eq!(encoded["macros"][1]["failure"]["frames"][0]["line"], 3);
        assert_eq!(
            encoded["macros"][1]["capabilities"],
            serde_json::json!([]),
            "解釈できなかったマクロに能力が付いている"
        );
        // ホスト API の拒否は**拒んだ API の名前**を運ぶ（要件 9.2）。
        assert_eq!(
            serde_json::to_value(MacroFailureTag::HostRejected {
                api: "host.readRows".to_owned()
            })
            .unwrap(),
            serde_json::json!({ "kind": "host_rejected", "api": "host.readRows" })
        );
        let back: MacroListResponse = serde_json::from_value(encoded).unwrap();
        assert_eq!(back, response);

        // 実行: 3 値が `outcome` で区別でき、打ち切りの種類が読める（要件 2.3, 6.1, 6.2）。
        let ran = MacroRunOutcome::Ran {
            value: "42".to_owned(),
            output: vec![MacroOutputLine {
                level: MacroOutputLevel::Log,
                text: "はじめます".to_owned(),
            }],
            changes: MacroChangeCounts {
                set_cells: 3,
                inserted_rows: 0,
                removed_rows: 1,
                duplicated_rows: 0,
            },
            elapsed_ms: 12,
        };
        let encoded = serde_json::to_value(&ran).unwrap();
        assert_eq!(encoded["outcome"], serde_json::json!("Ran"));
        assert_eq!(encoded["output"][0]["level"], serde_json::json!("log"));
        assert_eq!(
            encoded["changes"],
            serde_json::json!({
                "set_cells": 3,
                "inserted_rows": 0,
                "removed_rows": 1,
                "duplicated_rows": 0,
            }),
            "件数の形が変わった（種別ごとの数である。要件 5.5）"
        );
        let counts = MacroChangeCounts {
            set_cells: 3,
            inserted_rows: 0,
            removed_rows: 1,
            duplicated_rows: 0,
        };
        assert_eq!(counts.total(), 4, "変更の合計が件数と食い違う");
        assert!(!counts.is_empty());
        assert!(MacroChangeCounts {
            set_cells: 0,
            inserted_rows: 0,
            removed_rows: 0,
            duplicated_rows: 0,
        }
        .is_empty());

        let aborted = MacroRunOutcome::Aborted {
            limit: MacroAbortKind::Memory,
            elapsed_ms: 900,
            failure: MacroFailureReport {
                kind: MacroFailureTag::Execution,
                reason: "execution terminated".to_owned(),
                frames: Vec::new(),
            },
        };
        let encoded = serde_json::to_value(&aborted).unwrap();
        assert_eq!(encoded["outcome"], serde_json::json!("Aborted"));
        assert_eq!(encoded["limit"], serde_json::json!("memory"));
        let back: MacroRunOutcome = serde_json::from_value(encoded).unwrap();
        assert_eq!(back, aborted);

        let failed = MacroRunOutcome::Failed {
            failure: MacroFailureReport {
                kind: MacroFailureTag::Execution,
                reason: "TypeError: undefined は関数ではありません".to_owned(),
                frames: vec![MacroFrame {
                    macro_name: "集計".to_owned(),
                    function: "合計".to_owned(),
                    line: 12,
                    column: 5,
                }],
            },
        };
        let encoded = serde_json::to_value(&failed).unwrap();
        assert_eq!(encoded["outcome"], serde_json::json!("Failed"));
        assert_ne!(
            serde_json::to_value(&failed).unwrap()["outcome"],
            serde_json::to_value(&ran).unwrap()["outcome"],
            "失敗と成功が同じ値になっている"
        );

        // 要求は名前だけを運ぶ（ソースも上限も運ばない。要件 6.5）。
        let run = MacroRunRequest {
            name: "集計".to_owned(),
        };
        let request = serde_json::to_value(&run).unwrap();
        assert_eq!(
            request
                .as_object()
                .map(|object| object.keys().cloned().collect::<Vec<_>>()),
            Some(vec!["name".to_owned()]),
            "実行の要求が名前以外を運んでいる: {request}"
        );

        // 保存はソースを**そのまま**運び、種別は閉じた札である（要件 1.5, 3.1）。
        let source = "// @grant net\n\nconst xs = await host.readRange(0, 3);\n";
        let store = MacroStoreRequest {
            name: "棚卸し".to_owned(),
            kind: MacroKindTag::TypeScript,
            source: source.to_owned(),
        };
        let encoded = serde_json::to_value(&store).unwrap();
        assert_eq!(
            encoded["source"],
            serde_json::Value::String(source.to_owned()),
            "保存の要求でソースが変わった"
        );
        assert_eq!(
            serde_json::from_value::<MacroStoreRequest>(encoded).unwrap(),
            store
        );
        // 種別の札に無い綴りは復元できない（境界で弾かれる）。
        assert!(serde_json::from_value::<MacroStoreRequest>(serde_json::json!({
            "name": "x",
            "kind": "typescript5",
            "source": ""
        }))
        .is_err());
    }

    /// イベント名の定数が生成物に出ることを固定する（フロントエンドが綴りを間違えないため）。
    #[test]
    fn bindings_expose_the_event_name() {
        let ts = render_bindings().unwrap();
        assert!(
            ts.contains(&format!(
                "export const SETTINGS_CHANGED_EVENT = \"{SETTINGS_CHANGED_EVENT}\";"
            )),
            "生成物にイベント名の定数が無い:\n{ts}"
        );
    }

    /// 設定変更の通知は閉じた鍵の名前だけを運ぶ（要件 7.7、8.4）。
    #[test]
    fn settings_change_event_carries_only_the_catalog_key_and_value() {
        let changed = SettingsChangedEvent {
            key: "diagnostics.level".to_owned(),
            value: SettingsValue::new(serde_json::Value::String("debug".to_owned())),
        };
        let encoded = serde_json::to_value(&changed).unwrap();
        let object = encoded.as_object().expect("通知は対象でなければならない");
        assert_eq!(
            object
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            ["key", "value"]
                .into_iter()
                .map(str::to_owned)
                .collect::<std::collections::BTreeSet<_>>(),
            "通知が鍵と値以外を運んでいる: {encoded}"
        );
    }

    #[test]
    fn bindings_header_names_the_regeneration_command() {
        let ts = render_bindings().unwrap();
        assert!(
            ts.contains(REGENERATE_BINDINGS_COMMAND),
            "生成物のヘッダに再生成コマンドが無い:\n{ts}"
        );
        assert!(
            ts.contains("手で編集しない"),
            "生成物であることの注意がヘッダに無い:\n{ts}"
        );
    }

    // ------------------------------------------------------------------
    // 9.5: 診断の導線（詳細度の閉じた列挙・書き出しの応答・メニューの通知）
    // ------------------------------------------------------------------

    /// 境界の詳細度は**閉じた列挙**であり、`ALL` が全値を詳細度の昇順で並べること、および
    /// 中核の列挙との写像が 1 対 1（往復して同じ値）であることを固定する。
    ///
    /// 中核へ値を足すと [`From`] の網羅的な `match` がコンパイルエラーになるので、ここが
    /// 検査するのは「境界側の並びが中核の [`Ord`] と一致すること」である — 順序が食い違うと、
    /// 画面に出る選択肢の並びが詳細度の順でなくなる。
    #[test]
    fn diagnostics_levels_are_closed_and_ordered() {
        use crate::diagnostics::DiagnosticsLevel as Core;

        assert_eq!(DiagnosticsLevel::ALL.len(), 6, "閉じた列挙の全値を並べる");

        let mut previous: Option<Core> = None;
        for level in DiagnosticsLevel::ALL {
            let core: Core = level.into();
            if let Some(previous) = previous {
                assert!(
                    previous < core,
                    "{previous:?} の次に {core:?} が来ている（昇順でない）"
                );
            }
            // 往復して同じ値へ戻ること（写像が 1 対 1 であることの実行時の証拠）。
            assert_eq!(DiagnosticsLevel::from(core), level);
            previous = Some(core);
        }
        assert_eq!(previous, Some(Core::Trace), "最後は最も細かい詳細度である");

        // 中核の既定（4.5: `Info`）が境界でも同じ位置にあること。
        let default: DiagnosticsLevel = Core::default().into();
        assert_eq!(default, DiagnosticsLevel::Info);
    }

    /// 詳細度の綴りが中核と同じ小文字表現で境界を越えること、および生成物の型がその閉じた
    /// 集合を文字列の合併型として出すことを固定する（要件 8.7）。
    #[test]
    fn diagnostics_levels_are_lowercase_strings_on_both_sides() {
        let encoded = DiagnosticsLevel::ALL
            .iter()
            .map(|level| serde_json::to_value(level).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            encoded,
            ["off", "error", "warn", "info", "debug", "trace"]
                .into_iter()
                .map(|name| serde_json::Value::String(name.to_owned()))
                .collect::<Vec<_>>(),
            "境界の詳細度は小文字の文字列で運ばれる"
        );

        let ts = generated::<DiagnosticsLevel>();
        for name in ["off", "error", "warn", "info", "debug", "trace"] {
            assert!(ts.contains(&format!("\"{name}\"")), "{ts}");
        }
        assert_no_any(&ts);
        assert_no_numeric_type(&ts);
    }

    /// 詳細度の応答が**現在値と、選べる値の全体を昇順で**運ぶことを固定する。画面はこの 2 つ
    /// だけを見て選択肢を組める（選べる値の集合と順序を画面側に写さない）。
    #[test]
    fn verbosity_response_lists_the_whole_enum_in_order() {
        let response = DiagnosticsVerbosityResponse {
            context: WindowContext {
                window: WindowLabel::new("empty-1"),
            },
            level: DiagnosticsLevel::Debug,
            levels: DiagnosticsLevel::ALL.to_vec(),
        };
        let encoded = serde_json::to_value(&response).unwrap();
        assert_eq!(encoded["context"]["window"], "empty-1");
        assert_eq!(encoded["level"], "debug");
        assert_eq!(
            encoded["levels"],
            serde_json::json!(["off", "error", "warn", "info", "debug", "trace"])
        );

        // 変更の要求は列挙の値だけを受け付ける（任意の文字列は載らない）。
        let request = DiagnosticsVerbositySetRequest {
            level: DiagnosticsLevel::Trace,
        };
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({ "level": "trace" })
        );
        let back: DiagnosticsVerbositySetRequest =
            serde_json::from_value(serde_json::json!({ "level": "trace" })).unwrap();
        assert_eq!(back, request);
        assert!(
            serde_json::from_value::<DiagnosticsVerbositySetRequest>(
                serde_json::json!({ "level": "verbose" })
            )
            .is_err(),
            "列挙に無い詳細度は境界を越えられない"
        );
    }

    /// 書き出しの応答が**単一ファイルの位置と、記録の有無**を運ぶことを固定する。
    /// `Empty` でも成功であり、その場合も宛先は 1 つである（4.5 の契約）。**境界に数値は出さない**
    /// ので、件数は閉じた列挙で運ぶ。
    #[test]
    fn export_response_reports_one_destination_and_whether_records_existed() {
        let response = DiagnosticsExportResponse {
            context: WindowContext {
                window: WindowLabel::new("doc-1"),
            },
            destination: "/home/user/ダウンロード/jxcel-diagnostics-1.log".to_owned(),
            records: DiagnosticsExportRecords::Empty,
        };
        let encoded = serde_json::to_value(&response).unwrap();
        assert_eq!(encoded["context"]["window"], "doc-1");
        assert_eq!(
            encoded["destination"],
            "/home/user/ダウンロード/jxcel-diagnostics-1.log"
        );
        assert_eq!(encoded["records"], "empty");
        assert_eq!(
            serde_json::to_value(DiagnosticsExportRecords::Merged).unwrap(),
            "merged"
        );
        assert_no_json_number(&encoded, "export");

        let ts = generated::<DiagnosticsExportResponse>();
        assert_no_numeric_type(&ts);
        assert_no_any(&ts);

        assert!(serde_json::to_value(&DiagnosticsLogLocationResponse {
            context: WindowContext {
                window: WindowLabel::new("doc-1"),
            },
            directory: "/home/user/.local/share/com.jxcel.app/logs".to_owned(),
        })
        .unwrap()["directory"]
            .is_string());
    }

    /// メニューの活性化の通知が**選ばれた導線だけ**を運ぶことを固定する（要件 3.5 の
    /// 振り向けで対象ウィンドウへ送るため、本文は種別だけである）。
    #[test]
    fn diagnostics_request_carries_only_the_section() {
        let event = DiagnosticsRequestedEvent {
            section: DiagnosticsSection::Export,
        };
        let encoded = serde_json::to_value(event).unwrap();
        let object = encoded.as_object().expect("通知は対象でなければならない");
        assert_eq!(
            object
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            ["section"].into_iter().map(str::to_owned).collect()
        );
        assert_eq!(encoded["section"], "export");
        assert_eq!(
            serde_json::to_value(DiagnosticsSection::Location).unwrap(),
            "location"
        );
        assert_eq!(
            serde_json::to_value(DiagnosticsSection::Verbosity).unwrap(),
            "verbosity"
        );
    }

    /// 生成物が 9.5 の境界（型・封筒の具体形・イベント名）をすべて宣言していることを固定する。
    #[test]
    fn bindings_declare_the_diagnostics_surface() {
        let ts = render_bindings().unwrap();
        for declaration in [
            "export type DiagnosticsLevel =",
            "export type DiagnosticsSection =",
            "export type DiagnosticsRequestedEvent =",
            "export type DiagnosticsLogLocationResponse =",
            "export type DiagnosticsExportResponse =",
            "export type DiagnosticsVerbosityResponse =",
            "export type DiagnosticsVerbositySetRequest =",
            "export type DiagnosticsLogLocationResult = IpcResult<DiagnosticsLogLocationResponse, IpcError>;",
            "export type DiagnosticsExportResult = IpcResult<DiagnosticsExportResponse, IpcError>;",
            "export type DiagnosticsVerbosityResult = IpcResult<DiagnosticsVerbosityResponse, IpcError>;",
            "export const DIAGNOSTICS_REQUESTED_EVENT = \"diagnostics_requested\";",
        ] {
            assert!(
                ts.contains(declaration),
                "生成物に `{declaration}` が無い:\n{ts}"
            );
        }
        assert!(
            ts.contains("\"kind\": \"Diagnostics\""),
            "封筒の失敗の原因に診断の導線の種別が無い:\n{ts}"
        );
    }

    /// 生成物が 3.1 の境界（型・封筒の具体形・イベント名）をすべて宣言していることを固定する。
    ///
    /// **ドリフト検査だけでは足りない。** あれは「生成物と生成器の出力が一致すること」を見る
    /// のであって、宣言そのものを消して再生成すれば一致したまま通ってしまう。ここで面の存在を
    /// 名指しで固定する（[`bindings_declare_the_diagnostics_surface`] と同じ形）。
    ///
    /// **状態の判別子の値も固定する。** [`DocumentSessionStatus`] は
    /// `serde(rename_all = "lowercase")` を付けないため、生成物の腕は `"Absent"` / `"Open"` /
    /// `"Unavailable"` のままである。小文字だと思って `case "absent":` と書く実装が
    /// 絞り込みの効かない分岐を作らないよう、正しい値をここで固定する。
    #[test]
    fn bindings_declare_the_document_surface() {
        let ts = render_bindings().unwrap();
        for declaration in [
            "export type DocumentOrigin = \"file\" | \"new\";",
            "export type DocumentSheet = {",
            "export type DocumentSummary = {",
            "export type DocumentSessionStatus =",
            "{ \"state\": \"Absent\" }",
            "{ \"state\": \"Open\" }",
            "{ \"state\": \"Unavailable\"",
            "export type DocumentStateResponse = {",
            "export type DocumentSaveResponse = {",
            "export type DocumentNewResponse = {",
            "export type DocumentDiscardResponse = {",
            "export type DocumentSaveOutcome =",
            "export type DocumentNewOutcome =",
            "export type DocumentStateResult = IpcResult<DocumentStateResponse, IpcError>;",
            "export type DocumentSaveResult = IpcResult<DocumentSaveResponse, IpcError>;",
            "export type DocumentNewResult = IpcResult<DocumentNewResponse, IpcError>;",
            "export type DocumentDiscardResult = IpcResult<DocumentDiscardResponse, IpcError>;",
            "export const DOCUMENT_SESSION_CHANGED_EVENT = \"document_session_changed\";",
        ] {
            assert!(
                ts.contains(declaration),
                "生成物に `{declaration}` が無い:\n{ts}"
            );
        }
        assert!(
            ts.contains("\"kind\": \"Document\""),
            "封筒の失敗の原因にドキュメントの種別が無い:\n{ts}"
        );
    }

    /// 生成物が 7.8 の複製の要求のイベント名を宣言していることを固定する（タスク 8.7）。
    ///
    /// **ドリフト検査だけでは足りない**（[`bindings_declare_the_document_surface`] と同じ理由）。
    /// 画面はこの定数だけを参照する — 文字列リテラルを綴り間違えると、メニューの活性化が
    /// **無言で届かなくなる**（購読側の名前が一致しないため。エラーにもならない）。
    #[test]
    fn bindings_declare_the_grid_copy_event() {
        let ts = render_bindings().unwrap();
        let declaration = "export const GRID_COPY_REQUESTED_EVENT = \"grid_copy_requested\";";
        assert!(
            ts.contains(declaration),
            "生成物に `{declaration}` が無い:\n{ts}"
        );
    }

    /// 生成物が 8.9 の履歴の要求（イベント名と荷の型）を宣言していることを固定する
    /// （タスク 8.9。要件 9.9）。
    ///
    /// **ドリフト検査だけでは足りない**（[`bindings_declare_the_document_surface`] と同じ理由）。
    /// 画面はこの定数だけを参照する — 文字列リテラルを綴り間違えると、メニューの活性化が
    /// **無言で届かなくなる**（購読側の名前が一致しないため。エラーにもならない）。荷の型も
    /// 名指しで固定する: 向きの腕の綴り（`"undo"` / `"redo"`）は生成物の閉じた列挙であり、
    /// 画面はそれを読んで分岐する（綴りが変われば画面の分岐が黙って落ちる）。
    #[test]
    fn bindings_declare_the_grid_history_event() {
        let ts = render_bindings().unwrap();
        for declaration in [
            "export const GRID_HISTORY_REQUESTED_EVENT = \"grid_history_requested\";",
            "export type GridHistoryRequestedEvent = {",
            "export type GridHistoryDirection = \"undo\" | \"redo\";",
        ] {
            assert!(
                ts.contains(declaration),
                "生成物に `{declaration}` が無い:\n{ts}"
            );
        }
        // 荷の欄の名前も固定する（**画面が読むのはこの欄である**）。
        assert!(
            ts.contains("direction: GridHistoryDirection"),
            "荷の欄の名前が違う（画面は `direction` を読む）:\n{ts}"
        );
    }

    /// 生成物が 10.8 の貼り付けの要求（イベント名と荷の型）を宣言していることを固定する
    /// （タスク 10.8。要件 7.8）。
    ///
    /// **ドリフト検査だけでは足りない**（[`bindings_declare_the_document_surface`] と同じ理由）。
    /// 画面はこの定数だけを参照する — 文字列リテラルを綴り間違えると、メニューの活性化が
    /// **無言で届かなくなる**（購読側の名前が一致しないため。エラーにもならない）。荷の欄の
    /// 名前も固定する: **画面が読むのは `text`** であり、名前が変われば画面は荷を捨てる
    /// （`clipboardRequests.ts` の `parsePasteText`）。
    #[test]
    fn bindings_declare_the_grid_paste_event() {
        let ts = render_bindings().unwrap();
        for declaration in [
            "export const GRID_PASTE_REQUESTED_EVENT = \"grid_paste_requested\";",
            "export type GridPasteRequestedEvent = {",
        ] {
            assert!(
                ts.contains(declaration),
                "生成物に `{declaration}` が無い:\n{ts}"
            );
        }
        assert!(
            ts.contains("text: string"),
            "荷の欄の名前が違う（画面は `text` を読む）:\n{ts}"
        );
        // 荷は**文字 1 つだけ**である（余分な欄を足せば、画面が読む欄が 2 つに分かれる）。
        let declared = generated::<GridPasteRequestedEvent>();
        assert_no_any(&declared);
        assert_no_type_token(&declared, "bigint");
    }

    /// 診断の失敗は**設定の失敗と区別できる**（要件 4.4）。種別が違えば `kind` も違う。
    #[test]
    fn diagnostics_failure_has_its_own_kind() {
        let cause = IpcError::Diagnostics {
            message: "保存先を解決できない".into(),
        };
        let value = serde_json::to_value(&cause).unwrap();
        assert_eq!(value["kind"], "Diagnostics");
        assert!(value["detail"]["message"].is_string());
    }

    // ------------------------------------------------------------------
    // 6.1: グリッドの境界の型
    // （列の情報・表示の指定・編集命令・判定の要約・違反の位置・型の種別の札）
    // ------------------------------------------------------------------

    /// 設計が固定する札の綴り（`design.md`「EditorRegistry」の `TypeKindTag` の合併型）。
    /// 7.4 の入力手段の登録簿と 7.1 の描画側が**この綴りで**生成物を取り込む。
    const DESIGN_TYPE_KIND_SPELLINGS: [&str; 14] = [
        "Int",
        "Float",
        "Decimal",
        "Text",
        "Bool",
        "Date",
        "DateTime",
        "Enum",
        "Ref",
        "Attachment",
        "Object",
        "Array",
        "Any",
        "Custom",
    ];

    /// `export type <name> = "A" | "B";` の形の宣言から、合併の要素を順に取り出す。
    ///
    /// 宣言の本体（`=` から `;` まで）にある文字列リテラルだけを拾うので、JSDoc や整形の
    /// 違いに依らない。
    fn union_members(ts: &str, name: &str) -> Vec<String> {
        let head = format!("export type {name} =");
        let start = ts
            .find(&head)
            .unwrap_or_else(|| panic!("生成物に {name} の宣言が無い:\n{ts}"));
        let rest = &ts[start + head.len()..];
        let end = rest
            .find(';')
            .unwrap_or_else(|| panic!("{name} の宣言が `;` で閉じられていない:\n{ts}"));
        let mut members = Vec::new();
        let mut chars = rest[..end].chars();
        while let Some(c) = chars.next() {
            if c != '"' {
                continue;
            }
            let mut member = String::new();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                member.push(c);
            }
            members.push(member);
        }
        members
    }

    /// 境界の値に現れた数値が**すべて 32 ビット以下の整数**であることを検査する。
    ///
    /// グリッドの境界は列の添字・行数・件数を運ぶため数値を持つ（`u32` であり、JavaScript の
    /// `number` はこれを正確に表せる）。禁止しているのは 64 ビット整数であり、それは JSON でも
    /// 同じ `number` として現れるため、値の側で上限を見る。
    fn assert_json_numbers_fit_u32(value: &serde_json::Value, at: &str) {
        match value {
            serde_json::Value::Number(n) => {
                let exact = n
                    .as_u64()
                    .unwrap_or_else(|| panic!("{at} の数値が 0 以上の整数でない: {n}"));
                assert!(
                    exact <= u64::from(u32::MAX),
                    "{at} の数値が 32 ビットを越えている: {n}"
                );
            }
            serde_json::Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    assert_json_numbers_fit_u32(item, &format!("{at}[{i}]"));
                }
            }
            serde_json::Value::Object(fields) => {
                for (name, item) in fields {
                    assert_json_numbers_fit_u32(item, &format!("{at}.{name}"));
                }
            }
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::String(_) => {
            }
        }
    }

    /// 札が**設計の 14 種と過不足なく一致**し、生成物の合併型も同じ 14 個を同じ綴りで並べる
    /// ことを固定する（要件 3.2。`TypeKindTag` は入力手段を選ぶ唯一の札である）。
    #[test]
    fn type_kind_tag_matches_the_design_union() {
        assert_eq!(
            TypeKindTag::ALL.len(),
            DESIGN_TYPE_KIND_SPELLINGS.len(),
            "札の数は設計の 14 種である"
        );

        let spellings = TypeKindTag::ALL
            .iter()
            .map(|tag| {
                serde_json::to_value(tag)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            spellings, DESIGN_TYPE_KIND_SPELLINGS,
            "札の綴りと並びは設計の合併型と一致しなければならない"
        );

        let ts = generated::<TypeKindTag>();
        assert_eq!(
            union_members(&ts, "TypeKindTag"),
            DESIGN_TYPE_KIND_SPELLINGS,
            "生成物の札の合併型が設計と食い違っている:\n{ts}"
        );
        assert_no_any(&ts);
        assert_no_type_token(&ts, "bigint");
    }

    /// グリッドの境界の型が、型の位置に `any` も 64 ビット整数の数値型も出さないことを
    /// 固定する（`i64` / `u64` は ts-rs が `bigint` へ落とす）。
    #[test]
    fn grid_boundary_types_expose_no_any_and_no_64_bit_number_type() {
        let declarations = [
            generated::<ColumnDescriptor>(),
            generated::<ColumnElementCount>(),
            generated::<ColumnExpandability>(),
            generated::<ColumnChoice>(),
            generated::<ColumnMemberDescriptor>(),
            generated::<GridPathSegment>(),
            generated::<GridSheetSummary>(),
            generated::<GridSortKey>(),
            generated::<GridFilterSpec>(),
            generated::<GridExpansionState>(),
            generated::<GridViewSpec>(),
            generated::<GridCellAddress>(),
            generated::<GridCellEdit>(),
            generated::<GridEditCommand>(),
            generated::<GridCoercionNotice>(),
            generated::<GridViolationLocation>(),
            generated::<GridEditOutcome>(),
            generated::<GridOpenRequest>(),
            generated::<GridOpenResponse>(),
            generated::<GridViewRequest>(),
            generated::<GridViewResponse>(),
            generated::<GridEditRequest>(),
            generated::<GridEditResponse>(),
            generated::<GridHistoryDirection>(),
            generated::<GridHistoryRequest>(),
            generated::<GridSearchDirection>(),
            generated::<GridViolationRequest>(),
            generated::<GridViolation>(),
            generated::<GridViolationResponse>(),
            generated::<GridReferenceRequest>(),
            generated::<GridReferenceRow>(),
            generated::<GridReferenceResponse>(),
        ];
        for ts in declarations {
            assert_no_any(&ts);
            assert_no_type_token(&ts, "bigint");
        }
    }

    /// 列の情報が `view` 層の `LayoutColumn` の持つものを全部運ぶことを固定する
    /// （列の添字・内側の位置・表示名・葉の型の札・要素数の能力・展開の可否、そして
    /// **宣言から導ける材料** — 値なしを許すか・選択肢・参照先のシート・ユーザー定義型の
    /// 識別子・入れ子の内側の宣言。タスク 10.3）。
    ///
    /// 内側の位置は**セル直下（空）から入れ子の段まで**を表現でき、`Field` と `Index` を
    /// 区別する（要件 4.5 が入れ子のどの位置かを特定できる形を求めるため）。
    #[test]
    fn column_descriptor_mirrors_the_layout_column() {
        let nested = ColumnDescriptor {
            column: 3,
            path: vec![
                GridPathSegment::Field {
                    name: "b".to_owned(),
                },
                GridPathSegment::Index { position: 2 },
            ],
            name: "a.b[2]".to_owned(),
            kind: Some(TypeKindTag::Array),
            element_count: Some(ColumnElementCount {
                items: TypeKindTag::Text,
                min: Some(1),
                max: None,
            }),
            expandability: ColumnExpandability::Available,
            nullable: false,
            choices: Vec::new(),
            reference_sheet: None,
            custom_type_id: None,
            members: Vec::new(),
        };
        let encoded = serde_json::to_value(&nested).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "column": 3,
                "path": [
                    { "segment": "Field", "name": "b" },
                    { "segment": "Index", "position": 2 },
                ],
                "name": "a.b[2]",
                "kind": "Array",
                "element_count": { "items": "Text", "min": 1, "max": null },
                "expandability": "available",
                "nullable": false,
                "choices": [],
                "reference_sheet": null,
                "custom_type_id": null,
                "members": [],
            })
        );
        assert_json_numbers_fit_u32(&encoded, "column");
        let back: ColumnDescriptor = serde_json::from_value(encoded).unwrap();
        assert_eq!(back, nested);

        // セル直下（空の位置）・使用不能な列・配列でない列も表現できる。
        let flat = ColumnDescriptor {
            column: 0,
            path: Vec::new(),
            name: "amount".to_owned(),
            kind: None,
            element_count: None,
            expandability: ColumnExpandability::Leaf,
            nullable: true,
            choices: Vec::new(),
            reference_sheet: None,
            custom_type_id: None,
            members: Vec::new(),
        };
        let encoded = serde_json::to_value(&flat).unwrap();
        assert_eq!(encoded["path"], serde_json::json!([]));
        assert_eq!(encoded["kind"], serde_json::Value::Null);
        assert_eq!(encoded["element_count"], serde_json::Value::Null);
        assert_eq!(encoded["expandability"], "leaf");
        // **材料が無い列は空／`null` を運ぶ**（面はそのとき既定へ落ちる。要件 10.4）。
        assert_eq!(encoded["nullable"], serde_json::json!(true));
        assert_eq!(encoded["choices"], serde_json::json!([]));
        assert_eq!(encoded["reference_sheet"], serde_json::Value::Null);
        assert_eq!(encoded["custom_type_id"], serde_json::Value::Null);
        assert_eq!(encoded["members"], serde_json::json!([]));
        assert_eq!(
            serde_json::from_value::<ColumnDescriptor>(encoded).unwrap(),
            flat
        );

        // 展開の可否の 2 つの事実は `expandability` から導かれる（ドメインの
        // `LayoutColumn::is_expandable` / `requires_detail` と同じ判断である）。
        assert!(nested.is_expandable());
        assert!(!nested.requires_detail());
        let capped = ColumnDescriptor {
            expandability: ColumnExpandability::Capped,
            ..nested.clone()
        };
        assert!(!capped.is_expandable());
        assert!(capped.requires_detail());
        assert!(!flat.is_expandable());
        assert!(!flat.requires_detail());

        // 要素数の上下限は**開いた端点**を `null` で運ぶ（宣言が無いことと 0 は違う）。
        assert_eq!(
            serde_json::to_value(ColumnElementCount {
                items: TypeKindTag::Int,
                min: None,
                max: Some(8),
            })
            .unwrap(),
            serde_json::json!({ "items": "Int", "min": null, "max": 8 })
        );
    }

    /// 列の**宣言から導ける材料**が境界を越え、往復すること（タスク 10.3。要件 3.2、3.7、
    /// 3.8、5.5、10.1、10.4）。
    ///
    /// 固定するのは 4 点である: ①選択肢は値と名の対で運ばれる ②参照先のシートは名（文字列）
    /// ③ユーザー定義型の識別子は文字列 ④入れ子の内側の宣言は位置・名・型の札・値なしを許すか
    /// を持ち、**展開していない列でも空でない**。
    #[test]
    fn column_descriptor_carries_the_declaration_material() {
        let column = ColumnDescriptor {
            column: 1,
            path: Vec::new(),
            name: "届け先".to_owned(),
            kind: Some(TypeKindTag::Object),
            element_count: None,
            expandability: ColumnExpandability::Available,
            nullable: false,
            choices: vec![ColumnChoice {
                value: "赤".to_owned(),
                label: "赤".to_owned(),
            }],
            reference_sheet: Some("仕入先".to_owned()),
            custom_type_id: Some("postal-code".to_owned()),
            members: vec![
                ColumnMemberDescriptor {
                    path: vec![GridPathSegment::Field {
                        name: "郵便番号".to_owned(),
                    }],
                    name: "届け先.郵便番号".to_owned(),
                    kind: TypeKindTag::Text,
                    nullable: false,
                    choices: Vec::new(),
                    custom_type_id: Some("postal-code".to_owned()),
                },
                ColumnMemberDescriptor {
                    path: vec![
                        GridPathSegment::Field {
                            name: "住所".to_owned(),
                        },
                        GridPathSegment::Field {
                            name: "市".to_owned(),
                        },
                    ],
                    name: "届け先.住所.市".to_owned(),
                    kind: TypeKindTag::Enum,
                    nullable: true,
                    choices: vec![ColumnChoice {
                        value: "自宅".to_owned(),
                        label: "自宅".to_owned(),
                    }],
                    custom_type_id: None,
                },
            ],
        };
        let encoded = serde_json::to_value(&column).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "column": 1,
                "path": [],
                "name": "届け先",
                "kind": "Object",
                "element_count": null,
                "expandability": "available",
                // **値なしを許すかは宣言から写した真偽である**（定数にしない。要件 3.7）。
                "nullable": false,
                "choices": [{ "value": "赤", "label": "赤" }],
                // **参照先は名（文字列）で運ぶ**（識別子の写しは適応層が行う）。
                "reference_sheet": "仕入先",
                "custom_type_id": "postal-code",
                "members": [
                    {
                        "path": [{ "segment": "Field", "name": "郵便番号" }],
                        "name": "届け先.郵便番号",
                        "kind": "Text",
                        "nullable": false,
                        "choices": [],
                        "custom_type_id": "postal-code",
                    },
                    {
                        "path": [
                            { "segment": "Field", "name": "住所" },
                            { "segment": "Field", "name": "市" },
                        ],
                        "name": "届け先.住所.市",
                        "kind": "Enum",
                        "nullable": true,
                        "choices": [{ "value": "自宅", "label": "自宅" }],
                        "custom_type_id": null,
                    },
                ],
            })
        );
        // 数の欄は 32 ビットに収まる（64 ビット整数を境界へ出さない規約）。
        assert_json_numbers_fit_u32(&encoded, "column");
        assert_json_numbers_fit_u32(&encoded["members"], "members");
        assert_eq!(
            serde_json::from_value::<ColumnDescriptor>(encoded).unwrap(),
            column
        );
    }

    /// 参照先の行を読む経路の型が、頁と総数を 32 ビット以下で運び、往復すること
    /// （タスク 10.3。要件 3.8）。
    ///
    /// **要求は件数を運び、応答はその頁と総数を運ぶ。**上限
    /// （[`GRID_REFERENCE_PAGE_LIMIT`]）は型ではなく**境界の切り詰め**である（要求が上限を
    /// 超えてもこの型は成立する — 切り詰めるのは適応層である）。
    #[test]
    fn the_reference_rows_types_carry_a_page_and_a_total() {
        let request = GridReferenceRequest {
            column: 2,
            search: "仕入".to_owned(),
            start: 0,
            count: GRID_REFERENCE_PAGE_LIMIT + 1000,
        };
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "column": 2,
                "search": "仕入",
                "start": 0,
                "count": GRID_REFERENCE_PAGE_LIMIT + 1000,
            })
        );
        assert_json_numbers_fit_u32(&encoded, "request");
        assert_eq!(
            serde_json::from_value::<GridReferenceRequest>(encoded).unwrap(),
            request
        );

        let response = GridReferenceResponse {
            context: WindowContext {
                window: super::WindowLabel::new("main"),
            },
            rows: vec![GridReferenceRow {
                id: "01K4ANRRG004HMASW9NF6YY091".to_owned(),
                label: "仕入先A 東京".to_owned(),
            }],
            total: 10_000,
            has_more: true,
        };
        let encoded = serde_json::to_value(&response).unwrap();
        assert_eq!(encoded["total"], serde_json::json!(10_000));
        assert_eq!(encoded["has_more"], serde_json::json!(true));
        assert_eq!(
            encoded["rows"][0],
            serde_json::json!({ "id": "01K4ANRRG004HMASW9NF6YY091", "label": "仕入先A 東京" })
        );
        assert_json_numbers_fit_u32(&encoded, "response");
        assert_eq!(
            serde_json::from_value::<GridReferenceResponse>(encoded).unwrap(),
            response
        );
    }

    /// 絞り込みが**要件 8.4 の 5 条件**をすべて表現でき、「違反あり」を含むことを固定する
    /// （8.4 と、違反の絞り込みを画面が要求できること）。
    #[test]
    fn filter_spec_covers_the_five_conditions_including_has_violation() {
        let cases = [
            (
                GridFilterSpec::Equals {
                    column: 1,
                    text: "東京".to_owned(),
                },
                serde_json::json!({ "filter": "Equals", "column": 1, "text": "東京" }),
            ),
            (
                GridFilterSpec::Contains {
                    column: 1,
                    text: "東".to_owned(),
                },
                serde_json::json!({ "filter": "Contains", "column": 1, "text": "東" }),
            ),
            (
                GridFilterSpec::IsEmpty { column: 2 },
                serde_json::json!({ "filter": "IsEmpty", "column": 2 }),
            ),
            (
                GridFilterSpec::IsNotEmpty { column: 2 },
                serde_json::json!({ "filter": "IsNotEmpty", "column": 2 }),
            ),
            (
                GridFilterSpec::HasViolation { column: None },
                serde_json::json!({ "filter": "HasViolation", "column": null }),
            ),
            (
                GridFilterSpec::HasViolation { column: Some(4) },
                serde_json::json!({ "filter": "HasViolation", "column": 4 }),
            ),
        ];
        let mut tags = std::collections::BTreeSet::new();
        for (spec, expected) in cases {
            let encoded = serde_json::to_value(&spec).unwrap();
            assert_eq!(encoded, expected);
            assert_json_numbers_fit_u32(&encoded, "filter");
            tags.insert(encoded["filter"].as_str().unwrap().to_owned());
            assert_eq!(
                serde_json::from_value::<GridFilterSpec>(encoded).unwrap(),
                spec
            );
        }
        assert_eq!(
            tags,
            [
                "Equals",
                "Contains",
                "IsEmpty",
                "IsNotEmpty",
                "HasViolation"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );

        // 列を問わない「違反あり」と、列を指定した「違反あり」は別の要求である。
        assert_ne!(
            GridFilterSpec::HasViolation { column: None },
            GridFilterSpec::HasViolation { column: Some(0) }
        );
    }

    /// 表示の指定が**並べ替え・絞り込み・展開**を 1 つの形で運ぶことを固定する
    /// （要件 8.3、8.4、5.3。展開を運ぶ口は `grid_set_view` だけである）。
    #[test]
    fn view_spec_carries_sort_filter_and_expansion() {
        let spec = GridViewSpec {
            sort: vec![
                GridSortKey {
                    column: 0,
                    descending: true,
                },
                GridSortKey {
                    column: 2,
                    descending: false,
                },
            ],
            filters: vec![GridFilterSpec::HasViolation { column: None }],
            expansion: vec![
                GridExpansionState {
                    column: 1,
                    expanded: true,
                    depth: 2,
                },
                GridExpansionState {
                    column: 3,
                    expanded: false,
                    depth: 0,
                },
            ],
        };
        let encoded = serde_json::to_value(&spec).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "sort": [
                    { "column": 0, "descending": true },
                    { "column": 2, "descending": false },
                ],
                "filters": [{ "filter": "HasViolation", "column": null }],
                "expansion": [
                    { "column": 1, "expanded": true, "depth": 2 },
                    { "column": 3, "expanded": false, "depth": 0 },
                ],
            })
        );
        assert_json_numbers_fit_u32(&encoded, "view");
        assert_eq!(
            serde_json::from_value::<GridViewSpec>(encoded).unwrap(),
            spec
        );

        // 空の指定は「絞り込み無し・並べ替え無し・展開無し」である（すべての行と列）。
        let empty = GridViewSpec::default();
        assert_eq!(
            serde_json::to_value(&empty).unwrap(),
            serde_json::json!({ "sort": [], "filters": [], "expansion": [] })
        );
    }

    /// 編集命令が `edit` 層の 6 つの命令を過不足なく写すことを固定する。
    ///
    /// 値は**打たれた文字**として運び（`String`）、構造表現と表形式テキストも文字列である
    /// — 境界に `document-format` のセル値は現れない。
    #[test]
    fn edit_command_mirrors_the_domain_commands() {
        let anchor = GridCellAddress {
            row: "01J8Z0".to_owned(),
            column: 2,
        };
        let cases = [
            (
                GridEditCommand::SetCells {
                    cells: vec![
                        GridCellEdit {
                            cell: anchor.clone(),
                            text: "1,5".to_owned(),
                        },
                        GridCellEdit {
                            cell: GridCellAddress {
                                row: "01J8Z1".to_owned(),
                                column: 0,
                            },
                            text: String::new(),
                        },
                    ],
                },
                serde_json::json!({
                    "command": "SetCells",
                    "cells": [
                        { "cell": { "row": "01J8Z0", "column": 2 }, "text": "1,5" },
                        { "cell": { "row": "01J8Z1", "column": 0 }, "text": "" },
                    ],
                }),
            ),
            (
                GridEditCommand::SetNested {
                    cell: anchor.clone(),
                    json: "{\"b\":[1,2]}".to_owned(),
                },
                serde_json::json!({
                    "command": "SetNested",
                    "cell": { "row": "01J8Z0", "column": 2 },
                    "json": "{\"b\":[1,2]}",
                }),
            ),
            (
                // **10.4 が足した 2 つの空間**（挿入の位置は文書の位置か可視の序数である）。
                GridEditCommand::InsertRows {
                    at: GridRowAnchor::Document { at: 3 },
                    count: 2,
                },
                serde_json::json!({
                    "command": "InsertRows",
                    "at": { "anchor": "Document", "at": 3 },
                    "count": 2,
                }),
            ),
            (
                GridEditCommand::RemoveRows {
                    target: GridRowTarget::Ids {
                        rows: vec!["01J8Z0".to_owned(), "01J8Z1".to_owned()],
                    },
                },
                serde_json::json!({
                    "command": "RemoveRows",
                    "target": { "target": "Ids", "rows": ["01J8Z0", "01J8Z1"] },
                }),
            ),
            (
                GridEditCommand::DuplicateRows {
                    target: GridRowTarget::Ordinals { from: 0, count: 1 },
                },
                serde_json::json!({
                    "command": "DuplicateRows",
                    "target": { "target": "Ordinals", "from": 0, "count": 1 },
                }),
            ),
            (
                GridEditCommand::PasteRange {
                    anchor: anchor.clone(),
                    rows: vec!["01J8Z0".to_owned(), "01J8Z1".to_owned()],
                    text: "a\tb\nc\td".to_owned(),
                },
                serde_json::json!({
                    "command": "PasteRange",
                    "anchor": { "row": "01J8Z0", "column": 2 },
                    "rows": ["01J8Z0", "01J8Z1"],
                    "text": "a\tb\nc\td",
                }),
            ),
        ];
        let mut tags = std::collections::BTreeSet::new();
        for (command, expected) in cases {
            let encoded = serde_json::to_value(&command).unwrap();
            assert_eq!(encoded, expected);
            assert_json_numbers_fit_u32(&encoded, "command");
            tags.insert(encoded["command"].as_str().unwrap().to_owned());
            assert_eq!(
                serde_json::from_value::<GridEditCommand>(encoded).unwrap(),
                command
            );
        }
        assert_eq!(
            tags,
            [
                "SetCells",
                "SetNested",
                "InsertRows",
                "RemoveRows",
                "DuplicateRows",
                "PasteRange"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );

        // 貼り付けは**表示されている行の並び**を運ぶ（要件 8.9）。空の並びも表現できる。
        let empty = GridEditCommand::PasteRange {
            anchor,
            rows: Vec::new(),
            text: String::new(),
        };
        assert_eq!(
            serde_json::to_value(&empty).unwrap()["rows"],
            serde_json::json!([])
        );
    }

    /// 判定の要約が `edit` 層の `EditOutcome` の持つものを全部運ぶことを固定する
    /// （影響を受けた行・変換の記録・違反の総数・違反そのもの・再検証した列・行数）。
    ///
    /// 件数は `u32` で運び、`usize` を境界へ出さない。
    #[test]
    fn edit_outcome_summarises_the_verdict() {
        let outcome = GridEditOutcome {
            affected: vec!["01J8Z0".to_owned()],
            // **写せなかった行は入らない**（`affected` の 1 件に対し、こちらは 1 件である —
            // 対応は部分列であり、添字が一致することを契約にしない。10.5）。
            affected_ordinals: vec![7],
            coercions: vec![GridCoercionNotice {
                cell: GridCellAddress {
                    row: "01J8Z0".to_owned(),
                    column: 1,
                },
                before: "1,5".to_owned(),
                after: "1.5".to_owned(),
            }],
            violation_total: 3,
            violations: vec![GridViolationLocation {
                row: Some("01J8Z0".to_owned()),
                column: 1,
                path: vec![GridPathSegment::Field {
                    name: "amount".to_owned(),
                }],
            }],
            revalidated_columns: vec![1, 4],
            row_count: 99,
        };
        let encoded = serde_json::to_value(&outcome).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "affected": ["01J8Z0"],
                "affected_ordinals": [7],
                "coercions": [{
                    "cell": { "row": "01J8Z0", "column": 1 },
                    "before": "1,5",
                    "after": "1.5",
                }],
                "violation_total": 3,
                "violations": [{
                    "row": "01J8Z0",
                    "column": 1,
                    "path": [{ "segment": "Field", "name": "amount" }],
                }],
                "revalidated_columns": [1, 4],
                "row_count": 99,
            })
        );
        assert_json_numbers_fit_u32(&encoded, "outcome");
        assert_eq!(
            serde_json::from_value::<GridEditOutcome>(encoded).unwrap(),
            outcome
        );

        // 空の命令の結果（何も書かず、違反も無い）も表現できる。
        let empty = GridEditOutcome {
            affected: Vec::new(),
            affected_ordinals: Vec::new(),
            coercions: Vec::new(),
            violation_total: 0,
            violations: Vec::new(),
            revalidated_columns: Vec::new(),
            row_count: 0,
        };
        assert_eq!(
            serde_json::to_value(&empty).unwrap(),
            serde_json::json!({
                "affected": [],
                "affected_ordinals": [],
                "coercions": [],
                "violation_total": 0,
                "violations": [],
                "revalidated_columns": [],
                "row_count": 0,
            })
        );
    }

    /// 違反の位置が**行の識別子・列の添字・入れ子の内側の位置**を運ぶことを固定する
    /// （要件 4.5）。
    ///
    /// 行を持たない違反（列そのものの問題）も表現できる — ドメインの `Violation::row` は
    /// `Option<RowId>` である。
    #[test]
    fn violation_location_carries_row_column_and_inner_path() {
        let cell = GridViolationLocation {
            row: Some("01J8Z0".to_owned()),
            column: 2,
            path: vec![
                GridPathSegment::Field {
                    name: "b".to_owned(),
                },
                GridPathSegment::Index { position: 1 },
                GridPathSegment::Field {
                    name: "c".to_owned(),
                },
            ],
        };
        let encoded = serde_json::to_value(&cell).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "row": "01J8Z0",
                "column": 2,
                "path": [
                    { "segment": "Field", "name": "b" },
                    { "segment": "Index", "position": 1 },
                    { "segment": "Field", "name": "c" },
                ],
            })
        );
        assert_json_numbers_fit_u32(&encoded, "violation");
        assert_eq!(
            serde_json::from_value::<GridViolationLocation>(encoded).unwrap(),
            cell
        );

        // セル直下の違反は空の位置であり、行を持たない違反は `null` で運ぶ。
        let column_level = GridViolationLocation {
            row: None,
            column: 0,
            path: Vec::new(),
        };
        assert_eq!(
            serde_json::to_value(&column_level).unwrap(),
            serde_json::json!({ "row": null, "column": 0, "path": [] })
        );
    }

    /// 要件 1.5 と 1.6 の 2 つの空の状態が、シートの要約の**形の上で区別できる**ことを
    /// 固定する。区別するのは列の数であり、行数だけでは足りない。
    ///
    /// - 列が 1 本も無い（要件 1.6）: 表を描かず、スキーマが定義されていないことを示す
    /// - 列はあるが行が 1 件も無い（要件 1.5）: 列の構成を提示したうえで行が無いことを示す
    #[test]
    fn sheet_summary_distinguishes_the_two_empty_states() {
        let column = ColumnDescriptor {
            column: 0,
            path: Vec::new(),
            name: "amount".to_owned(),
            kind: Some(TypeKindTag::Int),
            element_count: None,
            expandability: ColumnExpandability::Leaf,
            nullable: true,
            choices: Vec::new(),
            reference_sheet: None,
            custom_type_id: None,
            members: Vec::new(),
        };

        let no_columns = GridSheetSummary {
            columns: Vec::new(),
            row_count: 7,
        };
        assert!(no_columns.has_no_columns());
        assert!(!no_columns.has_columns_but_no_rows());

        let no_rows = GridSheetSummary {
            columns: vec![column.clone()],
            row_count: 0,
        };
        assert!(!no_rows.has_no_columns());
        assert!(no_rows.has_columns_but_no_rows());

        let populated = GridSheetSummary {
            columns: vec![column],
            row_count: 7,
        };
        assert!(!populated.has_no_columns());
        assert!(!populated.has_columns_but_no_rows());

        // 形の上で区別できる（列の数と行数の対が 3 つの状態を分ける）。
        assert_eq!(
            serde_json::to_value(&no_columns).unwrap(),
            serde_json::json!({ "columns": [], "row_count": 7 })
        );
        assert_eq!(
            serde_json::to_value(&no_rows).unwrap()["row_count"],
            serde_json::json!(0)
        );
        assert_ne!(
            serde_json::to_value(&no_columns).unwrap(),
            serde_json::to_value(&no_rows).unwrap()
        );
        assert_json_numbers_fit_u32(&serde_json::to_value(&populated).unwrap(), "sheet");
    }

    /// 生成物が 6.1 の境界（列の情報から違反の位置までの型と、札の合併型）をすべて
    /// 宣言していることを固定する。
    ///
    /// **ドリフト検査だけでは足りない。** あれは「生成物と生成器の出力が一致すること」を
    /// 見るのであって、宣言そのものを消して再生成すれば一致したまま通ってしまう。
    /// [`bindings_declare_the_document_surface`] と同じ形で、面の存在を名指しで固定する。
    #[test]
    fn bindings_declare_the_grid_surface() {
        let ts = render_bindings().unwrap();
        for declaration in [
            "export type TypeKindTag = \"Int\" | \"Float\" | \"Decimal\" | \"Text\" | \"Bool\" | \"Date\" | \"DateTime\" | \"Enum\" | \"Ref\" | \"Attachment\" | \"Object\" | \"Array\" | \"Any\" | \"Custom\";",
            "export type ColumnExpandability = \"available\" | \"capped\" | \"leaf\";",
            "export type GridPathSegment =",
            "export type ColumnElementCount = {",
            "export type ColumnDescriptor = {",
            "export type GridSheetSummary = {",
            "export type GridSortKey = {",
            "export type GridFilterSpec =",
            "export type GridExpansionState = {",
            "export type GridViewSpec = {",
            "export type GridCellAddress = {",
            "export type GridCellEdit = {",
            "export type GridEditCommand =",
            "export type GridCoercionNotice = {",
            "export type GridViolationLocation = {",
            "export type GridEditOutcome = {",
        ] {
            assert!(
                ts.contains(declaration),
                "生成物に `{declaration}` が無い:\n{ts}"
            );
        }
        for tag in [
            "{ \"command\": \"SetCells\"",
            "{ \"command\": \"SetNested\"",
            "{ \"command\": \"InsertRows\"",
            "{ \"command\": \"RemoveRows\"",
            "{ \"command\": \"DuplicateRows\"",
            "{ \"command\": \"PasteRange\"",
            "{ \"filter\": \"Equals\"",
            "{ \"filter\": \"Contains\"",
            "{ \"filter\": \"IsEmpty\"",
            "{ \"filter\": \"IsNotEmpty\"",
            "{ \"filter\": \"HasViolation\"",
            "{ \"segment\": \"Field\"",
            "{ \"segment\": \"Index\"",
        ] {
            assert!(ts.contains(tag), "生成物に `{tag}` が無い:\n{ts}");
        }
    }

    /// 生成物が 9.3 の境界（描画の健全性の報告と、その要求・応答）を宣言していることを固定する。
    ///
    /// **ドリフト検査だけでは足りない**（宣言そのものを消して再生成すれば一致したまま通る）。
    /// [`bindings_declare_the_diagnostics_surface`] と同じ形で、面の存在と**札の綴り**を
    /// 名指しで固定する。
    #[test]
    fn bindings_declare_the_render_health_surface() {
        let ts = render_bindings().unwrap();
        for declaration in [
            "export type RenderPaintFailure = \"no_canvas\" | \"unpaintable\" | \"blank\";",
            "export type ObservationUndo = \"ok\" | \"ng\" | \"not_observed\";",
            "export type ObservationItem =",
            "\"nested_expansion\" |",
            "\"paste_through_menu\" |",
            "export type ObservationItemOutcome = \"ok\" | \"ng\";",
            "export type ObservationItemResult = {",
            "export type ObservationItemReason =",
            "export type RenderHealthReport =",
            "{ \"fact\": \"paint_failed\"",
            "{ \"fact\": \"scan_below_budget\"",
            "{ \"fact\": \"observation\"",
            "export type RenderHealthRecordRequest = {",
            "export type RenderHealthRecordResponse = {",
            "export type RenderHealthRecordResult = IpcResult<RenderHealthRecordResponse, IpcError>;",
        ] {
            assert!(
                ts.contains(declaration),
                "生成物に `{declaration}` が無い:\n{ts}"
            );
        }
    }

    /// 描画の健全性の報告が、**画面が書く札の綴り**で直列化される（タスク 9.3。要件 12.2、12.3）。
    ///
    /// 境界を越える値はこの綴りであり、画面（`src/features/grid/renderHealth.ts`）は同じ綴りを
    /// 自分の型に持つ。**どちらか片方だけを改名しても、他のどの検査も落ちない** — 落ちるのは
    /// 実行時の記録だけであり、記録を読む 9.2 の台本が「知らない札」を見ることになる。ここで
    /// 事実と理由の種別の両方の綴りを固定する。
    #[test]
    fn render_health_report_serializes_with_the_spellings_the_screen_writes() {
        let paint = serde_json::to_value(RenderHealthReport::PaintFailed {
            failure: RenderPaintFailure::Blank,
            colors: Some(1),
        })
        .unwrap();
        assert_eq!(paint["fact"], "paint_failed");
        assert_eq!(paint["failure"], "blank");
        assert_eq!(paint["colors"], 1);

        let scan = serde_json::to_value(RenderHealthReport::ScanBelowBudget {
            median_us: 20_000,
            budget_us: 16_670,
        })
        .unwrap();
        assert_eq!(scan["fact"], "scan_below_budget");
        assert_eq!(scan["median_us"], 20_000);
        assert_eq!(scan["budget_us"], 16_670);
        // **走査の腕に理由の種別は載らない**（2 つの腕は互いに素である）。
        assert!(scan.get("failure").is_none());

        for (failure, spelling) in [
            (RenderPaintFailure::NoCanvas, "no_canvas"),
            (RenderPaintFailure::Unpaintable, "unpaintable"),
            (RenderPaintFailure::Blank, "blank"),
        ] {
            assert_eq!(serde_json::to_value(failure).unwrap(), spelling);
        }

        // 復元も同じ綴りで通る（画面が送った要求が境界で解釈できる）。
        let request: RenderHealthRecordRequest = serde_json::from_value(serde_json::json!({
            "report": { "fact": "scan_below_budget", "median_us": 17_000, "budget_us": 16_670 }
        }))
        .expect("画面が送る形の要求を解釈できる");
        assert_eq!(
            request.report,
            RenderHealthReport::ScanBelowBudget {
                median_us: 17_000,
                budget_us: 16_670,
            }
        );
    }
}

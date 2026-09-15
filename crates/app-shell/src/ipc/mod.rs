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
//!   [`GridCellAddress`]、判定の要約 [`GridEditOutcome`] / [`GridCoercionNotice`]、
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
    ColumnDescriptor, ColumnElementCount, ColumnExpandability, GridCellAddress, GridCellEdit,
    GridCoercionNotice, GridEditCommand, GridEditOutcome, GridEditRequest, GridEditResponse,
    GridExpansionState, GridFilterSpec, GridHistoryDirection, GridHistoryRequest, GridOpenRequest,
    GridOpenResponse, GridPathSegment, GridSearchDirection, GridSheetSummary, GridSortKey,
    GridViewRequest, GridViewResponse, GridViewSpec, GridViolation, GridViolationLocation,
    GridViolationRequest, GridViolationResponse, TypeKindTag,
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

/// イベント名の定数を生成する（タスク 7.1 / 9.5 / 3.1。要件 7.4、8.1、8.6、8.7、1.6）。
///
/// 設定変更の通知（[`SETTINGS_CHANGED_EVENT`]）・診断の導線の要求
/// （[`DIAGNOSTICS_REQUESTED_EVENT`]）・ドキュメントの状態変化
/// （[`DOCUMENT_SESSION_CHANGED_EVENT`]）は `invoke` の宛先を持たないため
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
        declared::<ColumnDescriptor>(&cfg),
        declared::<GridPathSegment>(&cfg),
        declared::<GridSheetSummary>(&cfg),
        declared::<GridSortKey>(&cfg),
        declared::<GridFilterSpec>(&cfg),
        declared::<GridExpansionState>(&cfg),
        declared::<GridViewSpec>(&cfg),
        declared::<GridCellAddress>(&cfg),
        declared::<GridCellEdit>(&cfg),
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
        declared::<GridSearchDirection>(&cfg),
        declared::<GridViolationRequest>(&cfg),
        declared::<GridViolation>(&cfg),
        declared::<GridViolationResponse>(&cfg),
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
    ];
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
        ];
        for ts in declarations {
            assert_no_any(&ts);
            assert_no_type_token(&ts, "bigint");
        }
    }

    /// 列の情報が `view` 層の `LayoutColumn` の持つものを全部運ぶことを固定する
    /// （列の添字・内側の位置・表示名・葉の型の札・要素数の能力・展開の可否）。
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
        };
        let encoded = serde_json::to_value(&flat).unwrap();
        assert_eq!(encoded["path"], serde_json::json!([]));
        assert_eq!(encoded["kind"], serde_json::Value::Null);
        assert_eq!(encoded["element_count"], serde_json::Value::Null);
        assert_eq!(encoded["expandability"], "leaf");
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
                GridEditCommand::InsertRows { at: 3, count: 2 },
                serde_json::json!({ "command": "InsertRows", "at": 3, "count": 2 }),
            ),
            (
                GridEditCommand::RemoveRows {
                    rows: vec!["01J8Z0".to_owned(), "01J8Z1".to_owned()],
                },
                serde_json::json!({ "command": "RemoveRows", "rows": ["01J8Z0", "01J8Z1"] }),
            ),
            (
                GridEditCommand::DuplicateRows {
                    rows: vec!["01J8Z0".to_owned()],
                },
                serde_json::json!({ "command": "DuplicateRows", "rows": ["01J8Z0"] }),
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
}

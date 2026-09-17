//! ドキュメントのセッションが境界を越える型（タスク 3.1。要件 1.6、1.7、7.1）。
//!
//! `design.md`「Data Contracts & Integration」が定める境界の型を、そのまま置く場所である。
//! **文字列と 32 ビット以下の整数と真偽だけで構成し、他のドメインクレートの型を参照しない。**
//! 境界の型は `crates/app-shell/src/ipc/` の下にだけ置き、`ts-rs` の derive を付けてよいのは
//! 本モジュールを含む `crate::ipc` の内側だけである（`design.md`「IpcContract」の不変条件）。
//!
//! とくに守る規約は 2 つある。
//!
//! 1. **位置（`PathBuf`）を境界へ出さない。** ドキュメントの名前はファイル名のみとし、新規の
//!    ドキュメントは空文字で運ぶ。アプリケーションシェルもフロントエンドも、ファイルシステム
//!    上の位置そのものに触れない（要件 2.4 の [`super::DocumentPickOutcome`] と同じ方針）。
//! 2. **`i64` / `u64` を境界へ出さない。** シートの件数は [`u32`] で運ぶ。JavaScript の
//!    `number` は IEEE 754 の倍精度であり、32 ビット以下の整数は正確に表せるが、64 ビット
//!    整数はそうではない（[`super::WindowLabel`] の doc を参照）。
//!
//! **ドメインの失敗は封筒の成功腕に載せる。** 読み込めなかった理由は
//! [`DocumentSessionStatus::Unavailable`]、保存と新規作成の結果は [`DocumentSaveOutcome`] /
//! [`DocumentNewOutcome`] が運び、経路レベルの失敗だけが [`super::IpcError::Document`] へ落ちる
//! （`design.md`「Error Handling」の表）。

use serde::{Deserialize, Serialize};

use super::WindowContext;

/// ドキュメントの出所（タスク 3.1。要件 1.6、7.1）。
///
/// **位置は運ばない。** 出所がファイルであることは判別できるが、そのファイルがどこにあるかは
/// 境界を越えない — 位置を知るのはドメインの側（`document-session`）だけでよい。新規の
/// ドキュメントは保存先を持たないため、保存の指示で保存先の選択を要する（要件 5.2）。
///
/// 綴りは小文字（`"file"` / `"new"`）であり、既存の閉じた列挙
/// （[`super::WindowDocumentState`]）と同じ `#[serde(rename_all = "lowercase")]` に従う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum DocumentOrigin {
    /// ファイルから読み込んだドキュメント。
    File,
    /// まだ保存先を持たない、新しく作られたドキュメント。
    New,
}

/// 保持しているドキュメントのシートの要約（タスク 3.1。要件 1.7）。
///
/// 識別子は**文字列**、件数（列数・行数）は [`u32`] で運ぶ。どちらも境界の規約
/// （64 ビット整数を出さない・他のドメインクレートの型を参照しない）に従った結果である。
/// `document-format` の `SheetId` / `usize` をそのまま出さない — 境界の型はドメインの型に
/// 依存せず、写すのは適応層の仕事である。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentSheet {
    /// シートの識別子（文字列表現）。
    pub id: String,
    /// 利用者に見せるシートの名前。
    pub name: String,
    /// 列数。
    pub columns: u32,
    /// 行数。
    pub rows: u32,
}

/// 保持しているドキュメントの要約（タスク 3.1。要件 1.6、1.7）。
///
/// 要件 1.6 が求める「開いているドキュメントの名前」と「未保存の変更があるかどうか」、および
/// 要件 1.7 が求める「シートの一覧」を 1 つの形にまとめる。**名前はファイル名のみ**であり
/// （位置は境界を越えない）、新規のドキュメントは空文字で運ぶ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentSummary {
    /// ファイル名のみ。新規のドキュメントは空文字。
    pub name: String,
    /// 出所。
    pub origin: DocumentOrigin,
    /// 未保存の変更があるかどうか（要件 4.3）。
    pub unsaved: bool,
    /// **変更の版**（適用と差し替えのたびに 1 進む。**飽和して `u32::MAX` で止まる**）。
    ///
    /// 下流（`data-grid`）が「同じシートのまま内容だけが外の経路で変わった」ことを検出する
    /// 材料である（`data-grid` の要件 1.7 の残りを閉じるための口。`data-grid/design.md` の
    /// 「申し送り（境界に足りないもの）」がこれを求めていた）。境界は 64 ビット整数を運ばない
    /// （`ipc-contract.md`）ため `u32` で写す — **飽和しても等価性の判定は壊れない**
    /// （値が変わるたびに増えるという性質だけを使う）。
    pub revision: u32,
    /// 保持しているシートの一覧（要件 1.7）。
    pub sheets: Vec<DocumentSheet>,
}

/// あるウィンドウのドキュメントのセッションの状態（タスク 3.1。要件 1.6、1.7、2.1）。
///
/// `state` を判別子とする判別可能な合併型であり、フロントエンドは `switch` で網羅的に分岐
/// できる（`assertNever` が新しい変種をコンパイルエラーにする。`src/ipc/client.ts`）。
/// タグの語は「どの状態か」を表す `state` であり、封筒の `status`（成功か失敗か）とは別の
/// 判別子である。
///
/// **`Unavailable` は封筒の失敗ではない。** ドキュメントを読み込めなかったという**ドメインの
/// 結果**であり、コマンドは正常に答えた。したがって [`super::IpcError`] の腕には載せず、
/// 成功の腕（[`DocumentStateResponse`]）がこの状態として運ぶ（`design.md`「Error Handling」）。
/// 理由の文言は適応層が組み立てたものをそのまま運び、見せ方を決めるのは呼び出し元である。
///
/// **判別子の語は `state` であり、値は変種名のまま（`"Absent"` / `"Open"` / `"Unavailable"`）
/// である。** 本列挙には `#[serde(rename_all = "lowercase")]` を付けない — 同じく判別可能な
/// 合併型である [`super::WindowCloseVerdict`] / [`super::DocumentPickOutcome`] と揃える
/// （小文字へ落とすのは [`DocumentOrigin`] のような閉じた「種類」の列挙だけである）。
/// 生成物（`src/ipc/bindings.ts`）は
/// `{ "state": "Absent" } | { "state": "Open" } & DocumentSummary | …` となり、
/// 分岐を書く側は `case "Absent":` のように**変種名のまま**照合しなければ絞り込みが効かない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "state")]
pub enum DocumentSessionStatus {
    /// ドキュメントを持たない。保存などの操作はドメインの側で失敗として報告される
    /// （要件 1.8。境界では「保持していない」という状態として答える）。
    Absent,
    /// ドキュメントを保持している。
    Open(DocumentSummary),
    /// 読み込めなかった。`reason` は利用者へ伝えるための材料である（要件 2.1）。
    Unavailable {
        /// 読み込めなかった理由。
        reason: String,
    },
}

/// セッションの状態の問い合わせの応答（タスク 3.1。要件 1.6、1.7）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。要求の型は無い — 必要な入力は操作の
/// 対象ウィンドウだけで、それは基盤が注入する（[`super::CanCloseWindowResponse`] と同じ形）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentStateResponse {
    /// 呼び出し元ウィンドウの文脈。
    pub context: WindowContext,
    /// そのウィンドウのセッションの状態。
    pub status: DocumentSessionStatus,
}

/// 保存の指示の結果（タスク 3.1。要件 5.1、5.3、5.4）。
///
/// **`Cancelled` は失敗ではない。** 利用者が保存先の選択を取り消したという正常な結果であり、
/// 封筒の `status: "error"` の腕には載せない（`design.md`「Error Handling」。
/// [`super::DocumentPickOutcome::Cancelled`] と同じ判断）。`Failed` は書き出せなかったことを
/// 意味し、利用者へ伝えるための理由（`reason`）を運ぶ。**失敗と取り消しでは未保存が保たれる。**
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "outcome")]
pub enum DocumentSaveOutcome {
    /// 保存先へ書き出すことができ、以後の出所が確定した（要件 5.1、5.2）。
    Saved,
    /// 利用者が保存先の選択を取り消した。**何も書き出していない**（要件 5.3）。
    Cancelled,
    /// 書き出せなかった。未保存は保たれる（要件 5.4）。
    Failed {
        /// 書き出せなかった理由。
        reason: String,
    },
}

/// 新規作成の指示の結果（タスク 3.1。要件 7.1、7.3）。
///
/// [`DocumentSaveOutcome`] と同じく、正常な結果を封筒の成功腕に載せる。**`Refused` は失敗では
/// ない** — 未保存の変更があるため作成できなかったという、ドメインの側の正しい答えであり、
/// 利用者へ伝えるための理由（`reason`）を運ぶ（要件 7.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "outcome")]
pub enum DocumentNewOutcome {
    /// 空のドキュメントを用意した（要件 7.1、7.2）。
    Created,
    /// 未保存の変更があるため作成を拒否した（要件 7.3）。
    Refused {
        /// 拒否の理由。
        reason: String,
    },
}

/// 保存の応答（タスク 3.1。要件 4.6、5.1）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。`status` は保存のあとのセッションの
/// 状態であり、`outcome` はこの指示の結果である。**未保存が落ちたかどうかは `status` の
/// [`DocumentSummary::unsaved`] を読めば分かる**ので、結果の型に重複して持たない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentSaveResponse {
    /// 呼び出し元ウィンドウの文脈。
    pub context: WindowContext,
    /// 保存のあとのセッションの状態。
    pub status: DocumentSessionStatus,
    /// この保存の指示の結果。
    pub outcome: DocumentSaveOutcome,
}

/// 新規作成の応答（タスク 3.1。要件 4.6、7.1）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。[`DocumentSaveResponse`] と同じ形で、
/// `status` は作成のあとの状態、`outcome` はこの指示の結果である。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentNewResponse {
    /// 呼び出し元ウィンドウの文脈。
    pub context: WindowContext,
    /// 作成のあとのセッションの状態。
    pub status: DocumentSessionStatus,
    /// この新規作成の指示の結果。
    pub outcome: DocumentNewOutcome,
}

/// 破棄の印の応答（タスク 3.1。要件 6.5）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。破棄の印は失敗しうる操作ではない
/// （未保存を落とすだけである）ため、結果の型を持たず、結果の状態だけを運ぶ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentDiscardResponse {
    /// 呼び出し元ウィンドウの文脈。
    pub context: WindowContext,
    /// 破棄の印のあとのセッションの状態。
    pub status: DocumentSessionStatus,
}

/// ドキュメントのセッションの状態が変わったことを伝える Tauri イベントの名前（タスク 3.1。
/// 要件 1.6、1.7、7.1）。
///
/// `invoke` の宛先を持たないためコマンド名の配列（[`super::COMMAND_NAMES`]）には現れない。
/// 設定変更の通知（[`super::SETTINGS_CHANGED_EVENT`]）と同じく、**生成物
/// （`src/ipc/bindings.ts`）へ定数として出す**ことで、フロントエンドが文字列リテラルを
/// 綴り間違える経路を塞ぐ（タスク 2.3 のドリフト検査がこの定数もバイト比較する）。
///
/// **イベントは状態を運ばない。** 送るのは「変わった」ことだけで、購読側は状態の唯一の源で
/// ある [`DocumentStateResponse`] を問い合わせ直す（`design.md`「セッション状態の通知」）。
/// 粒度は設定変更と同じ 1 種であり、種別ごとのイベントは作らない。
pub const DOCUMENT_SESSION_CHANGED_EVENT: &str = "document_session_changed";

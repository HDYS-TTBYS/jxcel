//! セッションの 4 コマンド — 状態・保存・新規作成・破棄の印を封筒つきで境界へ出す
//! （タスク 3.4。design.md「DocumentCommands（4 コマンド）」。要件 1.6、1.7、2.4、5.1、5.3、
//! 7.1、7.4）。
//!
//! # 4 つの経路
//!
//! | コマンド | 経路 | 状態を変える操作 |
//! |---|---|---|
//! | [`document_state`] | `WindowRegistry::document_of` → [`WindowDestroyWatch::resolve`] | 起動時に指定された位置の**遅延読み込み**（未解決のウィンドウだけ） |
//! | [`document_save`] | コアの `save` →（出所が無ければ）`crate::dialog::pick_save_location` → `save_to` | **保存の成功**（未保存が落ち、出所が確定する） |
//! | [`document_new`] | [`WindowDestroyWatch::create`] | **新規作成**（文書が入れ替わり版が 1 進む） |
//! | [`document_discard`] | コアの `discard` | **破棄の印**（未保存が落ちる） |
//!
//! 呼び出し元ウィンドウは**基盤が注入する [`WebviewWindow`] から取る**（ペイロードで受け取らない
//! ＝偽装できない。要件 4.6、`ipc-contract.md`）。したがって 4 つとも**要求の型を持たない** —
//! 入力は操作の対象ウィンドウだけである。
//!
//! # 失敗の載せ方（design.md「Error Handling」の表）
//!
//! **ドメインの失敗は封筒の成功腕に載せる。** 読み込めなかった理由は
//! [`DocumentSessionStatus::Unavailable`]、書き出せなかった理由と新規作成の拒否は
//! [`DocumentSaveOutcome`] / [`DocumentNewOutcome`] が運ぶ。利用者が取り消したことは**誤りでは
//! ない**（[`DocumentSaveOutcome::Cancelled`]。要件 5.3）。**経路レベルの失敗だけ**が
//! [`IpcError::Document`] へ落ちる — 本モジュールでは、`spawn_blocking` のタスクが異常終了した
//! 場合（保存の処理が panic した）だけである。
//!
//! # 理由の文言はここで組み立てる
//!
//! `document-session` の誤りと状態は**表示用の文言を持たない**（技術的診断だけを持つ）。利用者へ
//! 伝える理由を組み立てるのは適応層の仕事であり、本モジュールがその唯一の場所である
//! （[`failure_reason`] / [`unavailable_reason`] / [`write_failure_reason`]）。**形式の側の
//! 誤りの `Display` を素材に含める** — 利用者が「なぜ読めなかったか」を知る唯一の材料である。
//!
//! # イベントを送る操作（design.md「セッション状態の通知」）
//!
//! **状態を変えた操作のあとに、対象ウィンドウへ
//! [`DOCUMENT_SESSION_CHANGED_EVENT`] を 1 回送る。** 送るのは次の 4 つである:
//!
//! 1. **遅延読み込み** — [`document_state`] が未解決のウィンドウを解決して文書を読んだとき
//! 2. **新規作成** — [`document_new`] が `Created` を返したとき
//! 3. **保存の成功** — [`document_save`] が `Saved` を返したとき
//! 4. **破棄の印** — [`document_discard`] が未保存を落としたとき
//!
//! **読み取りは送らない。** 解決済みのウィンドウへの [`document_state`]（および同じ生成要求での
//! 2 度目の問い合わせ）、`Cancelled` / `Failed` / `Refused` の結果、保持していないウィンドウへの
//! 操作は、状態を変えないのでイベントを送らない。「変わった」は**状態の写しの比較**で決める —
//! 時間や回数の推測を使わない（`verification.md`「呼び出しの形を数える」。テストは各本体が返す
//! 「送るべきか」の真偽を数える）。
//!
//! **`Absent` へ落ちる変化だけは送らない。** 送り先のウィンドウが既に無いので、`emit` は必ず
//! 失敗する（その変化は掃除の経路が起こす。`answer_state` の doc）。
//!
//! **イベントは状態を運ばない**（payload は `()`）。購読側は状態の唯一の源である
//! [`document_state`] を問い合わせ直す。粒度は `settings_changed` と同じ 1 種であり、種別ごとの
//! イベントは作らない。**ウィンドウの破棄では送らない** — 送り先が既に無い（破棄は
//! [`WindowDestroyWatch`] が表の後始末として受ける）。
//!
//! # 本体は 2 つの入口から使われる（コマンド面とメニュー面）
//!
//! タスク 3.6 のメニュー項目（[`super::menu`]）は `#[tauri::command]` を通らない — フロント
//! エンドを経由せず Rust 側で完結するためである（design.md「セッション状態の通知」）。したがって
//! 同じ処理をメニューへ写すと、**画面からの操作とメニューからの操作で結果が食い違いうる**
//! （task 3.6 の受入はこの一致を求めている）。そこで次を `pub(crate)` にして、メニュー面が
//! **同じ本体を直接呼ぶ**:
//!
//! - [`answer_new`] / [`answer_save`]（操作の本体。判定・適用・`should_notify` を含む）
//! - [`emit_session_changed`]（状態変化の通知。**送る規則は 1 箇所に保つ**）
//! - [`describe_new`] / [`describe_save`]（記録の語。メニュー面の記録行も同じ語を使う）
//!
//! **公開面を広げるための可視性ではない。** 唯一の本体へ 2 つ目の入口を通すためのものであり、
//! メニュー面は境界の写像（封筒・文脈）を必要としない（応答を返す先が無い）。
//!
//! # 実行モデル（保存だけが非同期である理由）
//!
//! [`document_save`] は `async fn` とし、**保存と保存先の提示を `spawn_blocking` に載せる** —
//! 提示は利用者が答えるまでブロックし、保存は 10 万行 × 30 列で秒単位かかる。どちらもイベント
//! ループとランタイムのワーカーを塞いではならない（`crate::dialog` の「実行モデル」と同じ形）。
//!
//! **残る 3 つは同期コマンドである**（design.md の逐語）。[`document_state`] は遅延読み込みを
//! 引き受けるため読み込みの秒単位の待ちを含みうるが、これは設計の選択である — 起動時に指定された
//! ドキュメントの読み込みは**この問い合わせが唯一の引き金**であり（design.md
//! 「DocumentStateView」）、読み込みを別スレッドへ逃がすと「最初の問い合わせで名前とシートを
//! 返す」という応答の意味が変わる。
//!
//! # 位置は応答へ写さない
//!
//! [`document_save`] は保存先の提示を**内側で完結させる**。選ばれた位置は Rust の側にだけ留まり、
//! 応答（[`DocumentSaveResponse`]）は状態と結果だけを運ぶ（design.md「Boundary Commitments」の
//! 「位置を境界へ出さない」）。本モジュールは [`Path`] を受け取ることも返すこともない。
//!
//! # テストの形
//!
//! Tauri の実体（`WebviewWindow` / `AppHandle`）を要するのはコマンド関数の 4 つだけであり、
//! 中身は**すべて本体の関数**（[`answer_state`] / [`answer_save`] / [`answer_new`] /
//! [`answer_discard`]）へ切り出してある。テストはそれらを直接駆動する — GUI を起こさず、
//! `WindowDestroyWatch` の二重（`session/watch.rs` の `testing::AlwaysPresent`）と
//! **本物の文書**（`document-format` は dev-dependency）だけで足りる。

use std::path::Path;
use std::sync::Arc;

use app_shell::ipc::{
    command_names, DocumentDiscardResponse, DocumentNewOutcome, DocumentNewResponse,
    DocumentOrigin, DocumentSaveOutcome, DocumentSaveResponse, DocumentSessionStatus,
    DocumentSheet, DocumentStateResponse, DocumentSummary, IpcError, IpcResult, WindowContext,
    WindowLabel, DOCUMENT_SESSION_CHANGED_EVENT,
};
use document_session::{
    DocumentSessionsApi, Origin, SaveReport, SessionError, SessionState, SheetSummary,
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_plugin_log::log;

use crate::dialog::{self, SaveLocation};
use crate::session::watch::WindowDestroyWatch;
use crate::window::WindowRegistry;

/// Tauri が注入した呼び出し元ウィンドウを、境界の文脈（要件 4.6）へ写す。
///
/// **境界の型を新設しない。** `app_shell::ipc::WindowContext` / `WindowLabel` をそのまま使う
/// （3 つ目の識別子を作らない）。
fn caller_context(window: &WebviewWindow) -> WindowContext {
    WindowContext {
        window: WindowLabel::new(window.label()),
    }
}

/// 状態が変わったことを**対象ウィンドウへ 1 回**送る（design.md「セッション状態の通知」）。
///
/// 送るのは「変わった」ことだけで、状態そのものは運ばない（payload は `()`。状態の唯一の源は
/// [`document_state`]）。イベント名は**生成物の定数**を参照し、文字列リテラルを書かない
/// （tasks.md 2.2 の拡張規則。フロントエンドも同じ定数を参照する）。
///
/// 送信に失敗しても操作の結果は変えない（ウィンドウが既に無い等）。**記録に残すだけ**である。
/// **送る前に 1 行残す** — 実画面の観測と 3 OS の段は、この行で「どの操作が通知を起こしたか」を
/// 数える（`verification.md`「呼び出しの形を数える」）。
///
/// **送るかどうかを決めるのは呼び出し元である**（この関数は送るだけ）。判定は
/// [`should_notify`] の 1 箇所にあり、コマンド面もメニュー面（[`super::menu`]）もその結果だけを
/// ここへ渡す — イベントが 2 回飛ぶ経路・飛ばない経路を作らないためである（タスク 3.6）。
pub(crate) fn emit_session_changed(window: &WebviewWindow) {
    log::info!(
        "セッションの状態変化を通知した: ウィンドウ = {} / イベント = {DOCUMENT_SESSION_CHANGED_EVENT}",
        window.label(),
    );
    if let Err(error) = window.emit(DOCUMENT_SESSION_CHANGED_EVENT, ()) {
        log::warn!(
            "セッションの状態変化を送れなかった（ウィンドウ = {}）: {error}",
            window.label(),
        );
    }
}

// ---------------------------------------------------------------------------
// コアの値から境界の値への写像（純粋関数。位置は運ばない）
// ---------------------------------------------------------------------------

/// 出所を境界の形へ写す。**位置は捨てる。**
///
/// [`DocumentOrigin`] が運ぶのは「ファイルから読んだ / 新規」という**種類**だけであり、その
/// ファイルがどこにあるかは境界を越えない（要件 2.4 と同じ方針。`ipc/document.rs` の doc）。
fn origin_to_boundary(origin: Origin) -> DocumentOrigin {
    match origin {
        Origin::New => DocumentOrigin::New,
        // 位置（`PathBuf`）は**ここで落ちる**。境界は種類だけを受け取る。
        Origin::File(_) => DocumentOrigin::File,
    }
}

/// 件数を境界の [`u32`] へ写す（64 ビット整数を境界へ出さない規約。`ipc/document.rs`）。
///
/// 上限を超える値は**飽和させる** — `as` による切り捨ては、嘘の小さい数を利用者へ見せる
/// （32 ビットを超えるシートは現実には作れないが、写像が黙って壊れる形にはしない）。
fn count_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// シートの要約 1 枚を境界の形へ写す。**識別子は文字列、件数は [`u32`]。**
///
/// `document-format` の `SheetId` / `usize` をそのまま出さない — 境界の型はドメインの型に
/// 依存せず、写すのは適応層の仕事である（`ipc/document.rs` の `DocumentSheet` の doc）。
fn sheet_to_boundary(sheet: SheetSummary) -> DocumentSheet {
    DocumentSheet {
        id: sheet.id.to_string(),
        name: sheet.name,
        columns: count_to_u32(sheet.columns),
        rows: count_to_u32(sheet.rows),
    }
}

/// コアの状態の写しを境界の状態へ写す。**純粋関数**（GUI 無しで検査できる）。
///
/// 3 つの腕をそのまま 3 つの腕へ写す（ワイルドカードを使わない — コアに腕が増えればここが
/// コンパイルエラーになり、境界の形を追随させ忘れない）。`Unavailable` の理由だけは
/// **覚えている技術的診断を文言へ包み直す**（[`unavailable_reason`]）— 初回の失敗
/// （[`failure_status`]）と同じ綴りになるので、同じ失敗への 2 度目の問い合わせでも文言が変わらない。
fn to_boundary(state: SessionState, label: &WindowLabel) -> DocumentSessionStatus {
    match state {
        SessionState::Absent => DocumentSessionStatus::Absent,
        SessionState::Open {
            name,
            origin,
            unsaved,
            sheets,
        } => DocumentSessionStatus::Open(DocumentSummary {
            name,
            origin: origin_to_boundary(origin),
            unsaved,
            sheets: sheets.into_iter().map(sheet_to_boundary).collect(),
        }),
        SessionState::Unavailable { reason } => DocumentSessionStatus::Unavailable {
            reason: unavailable_reason(label, &reason),
        },
    }
}

/// 読み込めなかったことを伝える文言（要件 2.1）。
///
/// 素材は形式の側の誤りの `Display`（技術的診断）であり、**そのままでは利用者へ出さない** —
/// どのウィンドウの何が起きたのかを添えて初めて理由として読める（文言を組み立てるのは適応層の
/// 仕事である）。
fn unavailable_reason(label: &WindowLabel, detail: &str) -> String {
    format!(
        "{} のドキュメントを読み込めなかった: {detail}",
        label.as_str()
    )
}

/// 書き出せなかったことを伝える文言（要件 5.4）。
fn write_failure_reason(label: &WindowLabel, detail: &str) -> String {
    format!(
        "{} のドキュメントを書き出せなかった: {detail}",
        label.as_str()
    )
}

/// 状態の変化の前後を比べて「通知を送るべきか」を決める（**唯一の規則**）。
///
/// 4 つの操作（遅延読み込み・新規作成・保存・破棄の印）のどれも、送るかどうかはこの 1 つの
/// 規則で決まる — 操作ごとに「変えたはず」と決め打つと、**変えていない操作**（既に未保存でない
/// 文書への破棄の印、未保存でない文書の保存）でも通知が飛ぶ。観測点を状態そのものに置くことで、
/// 「読み取りは送らない」「状態を変えた操作だけが送る」が同じ式から出る。
///
/// **`Absent` へ落ちる変化だけは送らない。** それは消えたウィンドウの掃除
/// （[`WindowDestroyWatch::forget_unresolvable`] と、購読を登録できないウィンドウの後始末）で
/// あり、**送り先のウィンドウが既に無い** — 送っても `emit` が失敗するだけである。
fn should_notify(before: &SessionState, after: &SessionState) -> bool {
    before != after && !matches!(after, SessionState::Absent)
}

/// セッションの誤りを利用者へ伝える文言へ写す（要件 1.8、2.1、2.2、7.3）。
///
/// **変種ごとに書き分ける。** 4 つの誤りは利用者が取るべき行動が異なる（読み直す / 待つ /
/// 保存する / 何もしない）ので、1 つの文言に畳まない。
fn failure_reason(label: &WindowLabel, error: &SessionError) -> String {
    match error {
        SessionError::Read { source } => unavailable_reason(label, &source.to_string()),
        SessionError::Busy => format!("{} のドキュメントで別の操作が進行中である", label.as_str()),
        SessionError::UnsavedChanges => {
            format!("{} のドキュメントに未保存の変更がある", label.as_str())
        }
        SessionError::NoDocument => format!("{} はドキュメントを保持していない", label.as_str()),
    }
}

/// 読み込みの失敗に対する**状態の答え**（要件 1.8、2.1）。
///
/// セッションを用意できなかった場合（[`SessionError::NoDocument`]）は「保持していない」が
/// 正しい答えである — ウィンドウが引けず購読を登録できなかった場合であり、表からも落として
/// ある（design.md の表「保持していない窓への操作 → 状態 `Absent`」）。それ以外の失敗は
/// **読み込めなかった**であり、理由つきで答える（封筒の成功腕。`IpcError` へは載せない）。
fn failure_status(label: &WindowLabel, error: &SessionError) -> DocumentSessionStatus {
    match error {
        SessionError::NoDocument => DocumentSessionStatus::Absent,
        other => DocumentSessionStatus::Unavailable {
            reason: failure_reason(label, other),
        },
    }
}

/// 新規作成を拒否した理由（要件 7.3）。
///
/// [`failure_reason`] をそのまま使わないのは、**拒否の理由が「なぜ作れないか」でなければ
/// ならない**ためである — 保持していないウィンドウへの `create` は成功するので、ここへ
/// `NoDocument` が来るのはウィンドウを引けなかった場合だけであり、「ドキュメントを保持して
/// いない」では原因を取り違えさせる。
fn refusal_reason(label: &WindowLabel, error: &SessionError) -> String {
    match error {
        SessionError::UnsavedChanges => format!(
            "{} のドキュメントに未保存の変更があるため、新しいドキュメントを用意できない",
            label.as_str()
        ),
        SessionError::Busy => format!("{} のドキュメントで別の操作が進行中である", label.as_str()),
        // セッションを用意できなかったのは、購読を登録できないウィンドウだった場合だけである
        // （`watch.create` はウィンドウを引けないとき何も作らない）。
        SessionError::NoDocument => format!("{} のウィンドウを特定できなかった", label.as_str()),
        SessionError::Read { source } => unavailable_reason(label, &source.to_string()),
    }
}

// ---------------------------------------------------------------------------
// 本体（Tauri に依らない。テストはここを直接駆動する）
// ---------------------------------------------------------------------------

/// 状態の問い合わせの本体（[`document_state`] の中身。要件 1.6、1.7、2.1）。
///
/// 戻り値は（境界の状態, **通知を送るべきか**）である。2 つ目が真のときだけ、呼び出し元が
/// イベントを 1 回送る。
///
/// 手順は 3 つである:
///
/// 1. `before` に今の写しを取る（**セッションを作らない**読み取り。[`SessionState::Absent`] が
///    未解決のウィンドウの答えである）
/// 2. [`WindowDestroyWatch::resolve`] へ生成要求の位置を渡す。**冪等**なので、解決済み・覚えた
///    失敗のどちらでも読み込みは起きない（要件 1.2 の「2 度目のアクセスで読み込みが起きない」）
/// 3. `after` の写しを境界へ写し、`before` と比較して「送るべきか」を決める
///
/// **比較で決める理由**: 呼び出しの形（何回読んだか）を外から数える手段が無いため、状態そのものの
/// 変化を観測点にする。これにより「読み取りは送らない・状態を変えた操作だけが送る」が
/// 1 つの規則（[`SessionState`] の等価性）から導かれる。
///
/// **`Absent` へ落ちる変化だけは送らない。** それは消えたウィンドウの掃除
/// （[`WindowDestroyWatch::forget_unresolvable`] と、購読を登録できないウィンドウの後始末）で
/// あり、**送り先のウィンドウが既に無い** — 送っても `emit` が失敗するだけである。
///
/// **検証専用の引き金（`session::verification`）もこの本体を呼ぶ**（メニュー面は呼ばない —
/// あちらが共有するのは [`answer_new`] / [`answer_save`] である）。引き金がこれを呼ぶのは、
/// **読み込みの経路そのもの**（解決 + 状態の写し + 通知の要否）を写さずに通すためである —
/// 写すと「起動時の位置は最初のアクセスで 1 度だけ読む」という要件 1.2 の契約が 2 箇所に分かれる
/// （`session/verification.rs` の module doc）。
pub(crate) fn answer_state(
    watch: &WindowDestroyWatch,
    label: &WindowLabel,
    requested: Option<&Path>,
) -> (DocumentSessionStatus, bool) {
    let before = watch.sessions().state(label);
    let outcome = watch.resolve(label, requested);
    let after = watch.sessions().state(label);
    let notify = should_notify(&before, &after);

    let status = match outcome {
        Ok(()) => to_boundary(after, label),
        // 読み込みに失敗した。**ドメインの結果であり封筒の成功腕に載る** — 覚えた失敗
        // （`after` の `Unavailable`）を写すのではなく、ここで手元にある誤りから文言を組み立てる
        // （同じ綴りになる。`to_boundary` の doc）。
        Err(error) => failure_status(label, &error),
    };
    (status, notify)
}

/// 現在の状態を境界の形で答える（状態を変えた操作のあとの応答が使う）。
///
/// **セッションを作らない**読み取りである（[`DocumentSessionsApi::state`] の契約）。
/// **メニュー面（[`super::menu`]）も記録の 1 行のためにこれを呼ぶ** — 状態の写しを 2 つ持たない。
pub(crate) fn boundary_status(
    watch: &WindowDestroyWatch,
    label: &WindowLabel,
) -> DocumentSessionStatus {
    to_boundary(watch.sessions().state(label), label)
}

/// 新規作成の本体（[`document_new`] の中身。要件 7.1、7.3）。
///
/// 戻り値は（境界の結果, **通知を送るべきか**）。**作成が通れば文書が入れ替わり版が 1 進む**ので
/// 状態は必ず変わる。拒否（未保存がある・別の操作が進行中・ウィンドウが引けない）は何も変えない
/// ので、[`should_notify`] が偽を返す（決め打ちしない）。
///
/// **メニュー面（[`super::menu`]）もこの本体を呼ぶ** — 応答を返す先が無いので、結果の境界の型を
/// 捨てるだけである（task 3.6 の受入:「画面からの操作とメニューからの操作で結果が一致する」）。
pub(crate) fn answer_new(
    watch: &WindowDestroyWatch,
    label: &WindowLabel,
) -> (DocumentNewOutcome, bool) {
    let before = watch.sessions().state(label);
    let outcome = watch.create(label);
    let after = watch.sessions().state(label);
    let notify = should_notify(&before, &after);
    match outcome {
        Ok(()) => (DocumentNewOutcome::Created, notify),
        // **拒否は失敗ではない** — 未保存の変更があるというドメインの正しい答えであり、
        // 理由つきで成功腕に載る（要件 7.3）。
        Err(error) => (
            DocumentNewOutcome::Refused {
                reason: refusal_reason(label, &error),
            },
            false,
        ),
    }
}

/// 破棄の印の本体（[`document_discard`] の中身。要件 6.5）。
///
/// 戻り値は（**通知を送るべきか**, 失敗の記録）。未保存の印を落とす操作なので通れば状態が変わる
/// が、**既に未保存でない文書への指示は何も変えない** — そこは [`should_notify`] が偽を返す
/// （「成功したから真」と決め打つと、変えていないのにイベントが飛ぶ）。
/// 保持していないウィンドウへの指示は失敗であり、何も変えない（設計の表）。
fn answer_discard(watch: &WindowDestroyWatch, label: &WindowLabel) -> (bool, Option<SessionError>) {
    let before = watch.sessions().state(label);
    let outcome = watch.sessions().discard(label);
    let after = watch.sessions().state(label);
    match outcome {
        Ok(()) => (should_notify(&before, &after), None),
        Err(error) => (false, Some(error)),
    }
}

/// コアの保存の結果を境界の結果へ写す。**保存先を尋ねる必要が無い 3 つの腕**だけを扱う。
///
/// `None` は「保存先の選択が要る」（[`SaveReport::NeedsLocation`]）。ワイルドカードを使わないのは、
/// コアに腕が増えたときに写像の追随漏れをコンパイルエラーにするためである。**通知の要否は
/// ここでは決めない** — 保存の前後の状態の比較（[`should_notify`]）が唯一の規則である。
fn save_outcome(label: &WindowLabel, report: &SaveReport) -> Option<DocumentSaveOutcome> {
    match report {
        // 成功: 未保存が落ち、出所が確定する（要件 5.1）。
        SaveReport::Saved { .. } => Some(DocumentSaveOutcome::Saved),
        // 失敗: 未保存は保たれる（要件 5.4）。
        SaveReport::Failed { source } => Some(DocumentSaveOutcome::Failed {
            reason: write_failure_reason(label, &source.to_string()),
        }),
        // 取り消しは**正常な結果**である（要件 5.3）。何も書き出しておらず、状態も変わらない。
        SaveReport::Cancelled => Some(DocumentSaveOutcome::Cancelled),
        SaveReport::NeedsLocation => None,
    }
}

/// 保存先の提示に渡す提案名を決める（design.md「DialogGate」の 1 行）。
///
/// **出所から既存のファイル名が得られればそれ**、無ければ既定の `無題`。出所を持たない
/// ドキュメント（保存先を要する状態）の名前は境界の写像と同じく空文字であるため、
/// `None` と空文字を同じ扱いにする [`dialog::suggested_save_name`] にそのまま渡す。
fn suggested_name(watch: &WindowDestroyWatch, label: &WindowLabel) -> String {
    let state = watch.sessions().state(label);
    let origin_file_name = match &state {
        SessionState::Open {
            name,
            origin: Origin::File(_),
            ..
        } => Some(name.as_str()),
        _ => None,
    };
    dialog::suggested_save_name(origin_file_name)
}

/// 保存の本体（[`document_save`] の中身。要件 5.1〜5.4、5.8）。
///
/// 戻り値は（境界の結果, 通知を送るべきか）。**保存先の提示だけを縫い目にする** — 提示は GUI を
/// 要するので、テストは「何を提案したか」「取り消したか」だけを差し替えて残りの経路
/// （`save` → `save_to` → 印の更新）を本物のコアで通す。
///
/// 手順は 3 つである:
///
/// 1. 出所があるかを先に確かめる（**セッションを作らない**読み取り）。出所が無ければ**提示**を
///    行い、選ばれた位置へ `save_to` を呼ぶ（要件 5.2）。**取り消しのときは `save_to` を呼ばない**
///    — これが要件 5.3「何も書き出さず、未保存のまま保つ」の実体である
/// 2. 出所があればコアの `save` を呼ぶ（要件 5.1）
/// 3. 結果を境界の結果へ写し、**保存の前後の状態を比べて**通知の要否を決める
///    （[`should_notify`]）— 未保存でない文書への保存は状態を変えないので通知しない
///
/// `save_to` のあとに再び `NeedsLocation` が返っても**提示を繰り返さない**（提示のループを
/// 作らない）。位置を指定した書き出しで保存先を要することはコアの契約上ありえないが、
/// 万一その腕が来たら失敗として報告する。
///
/// **メニュー面（[`super::menu`]）もこの本体を呼ぶ。** 出所を持たない文書では
/// `pick`（メニュー面は `crate::dialog::pick_save_location` を渡す）が保存先を尋ねる — メニュー
/// からの保存でも要件 5.2 が同じように働く（task 3.6 の受入）。
pub(crate) fn answer_save<P>(
    watch: &WindowDestroyWatch,
    label: &WindowLabel,
    pick: P,
) -> (DocumentSaveOutcome, bool)
where
    P: FnOnce(&str) -> SaveLocation,
{
    let sessions = watch.sessions();
    let before = sessions.state(label);
    // 出所が無い（新規の）文書だけが保存先の提示を要する。**保持していないウィンドウと、既に
    // 出所を持つ文書はここを通らない** — 前者はコアの `save` が失敗を返し、後者は出所へ書き出す。
    let needs_location = matches!(
        before,
        SessionState::Open {
            origin: Origin::New,
            ..
        }
    );

    let report = if needs_location {
        // 提示は**この呼び出しの内側で完結する**（位置は応答へ写さない）。
        let suggested = suggested_name(watch, label);
        let location = pick(&suggested);
        match location {
            // 取り消し: 何も書き出さず、未保存のまま保つ（要件 5.3）。
            SaveLocation::Cancelled => {
                return (DocumentSaveOutcome::Cancelled, false);
            }
            // 提示できなかった: 保存は行われていない（未保存は保たれる）。理由をそのまま運ぶ。
            SaveLocation::Unavailable(message) => {
                return (DocumentSaveOutcome::Failed { reason: message }, false);
            }
            SaveLocation::Chosen(location) => sessions.save_to(label, &location),
        }
    } else {
        sessions.save(label)
    };

    let report = match report {
        Ok(report) => report,
        // 保持していないウィンドウへの保存は**失敗**として答える（設計の表）。
        Err(error) => {
            return (
                DocumentSaveOutcome::Failed {
                    reason: failure_reason(label, &error),
                },
                false,
            )
        }
    };

    let outcome = match save_outcome(label, &report) {
        Some(outcome) => outcome,
        // `NeedsLocation` が位置を指定した書き出しのあとに来た（コアの契約上ありえない）。
        // **提示を繰り返さず**失敗として報告する。
        None => DocumentSaveOutcome::Failed {
            reason: format!("{} の保存先を決められなかった", label.as_str()),
        },
    };
    let after = sessions.state(label);
    (outcome, should_notify(&before, &after))
}

// ---------------------------------------------------------------------------
// 記録に出す 1 行（状態の名前だけ。ファイル名も位置も書かない）
// ---------------------------------------------------------------------------

/// 状態の問い合わせの記録に出す 1 語。**名前も位置も書かない**（どの状態かを追えれば足りる）。
///
/// **メニュー面（[`super::menu`]）の記録行もこの語を使う**（同じ操作が同じ語で記録される。
/// タスク 3.6）。
pub(crate) fn describe_status(status: &DocumentSessionStatus) -> &'static str {
    match status {
        DocumentSessionStatus::Absent => "保持していない",
        DocumentSessionStatus::Open(_) => "保持している",
        DocumentSessionStatus::Unavailable { .. } => "読み込めなかった",
    }
}

/// 保存の結果の記録に出す 1 語。**位置は書かない**（境界へ出さないのと同じ理由）。
///
/// **メニュー面（[`super::menu`]）の記録行もこの語を使う** — 同じ操作がどちらの入口からでも同じ語で
/// 記録される（タスク 3.6 の受入の観測点）。
pub(crate) fn describe_save(outcome: &DocumentSaveOutcome) -> &'static str {
    match outcome {
        DocumentSaveOutcome::Saved => "保存した",
        DocumentSaveOutcome::Cancelled => "取り消された",
        DocumentSaveOutcome::Failed { .. } => "書き出せなかった",
    }
}

/// 新規作成の結果の記録に出す 1 語。**メニュー面（[`super::menu`]）の記録行もこの語を使う**
/// （同じ操作が同じ語で記録される。タスク 3.6）。
pub(crate) fn describe_new(outcome: &DocumentNewOutcome) -> &'static str {
    match outcome {
        DocumentNewOutcome::Created => "用意した",
        DocumentNewOutcome::Refused { .. } => "拒否した",
    }
}

// ---------------------------------------------------------------------------
// コマンド面（4 つ）
// ---------------------------------------------------------------------------

/// セッションの状態を返す（要件 1.6、1.7、2.1）。**起動時に指定されたドキュメントの読み込みは
/// この問い合わせが引き金になる**（遅延解決）。
///
/// 呼び出し元は注入された [`WebviewWindow`] から取る（要件 4.6）。手順は 4 つである:
///
/// 1. **掃除の経路**: [`WindowDestroyWatch::forget_unresolvable`] を呼ぶ。取得と購読の登録の
///    間に破棄されたウィンドウのセッションは、次の入口でここが必ず落とす（design.md の Risks。
///    引けるウィンドウには何もしない）
/// 2. **生成要求の位置**を [`WindowRegistry::document_of`] から読む。位置を読めるのは呼び出し元
///    だけであり、これが「読み込みを起こしてよい唯一の場所」である（要件 1.2）
/// 3. [`answer_state`] が解決と状態の写しを行う
/// 4. 状態が変わった（＝読み込みが起きた）ときだけ、**対象ウィンドウへイベントを 1 回**送る
///
/// **失敗の腕は無い**（読み込めなかったことは状態として答える）。応答の形を揃えるために封筒を
/// 通す（要件 4.4）。
#[tauri::command]
pub fn document_state(
    app: AppHandle,
    window: WebviewWindow,
) -> IpcResult<DocumentStateResponse, IpcError> {
    let command = command_names::DOCUMENT_STATE;
    let context = caller_context(&window);
    let label = context.window.clone();
    let watch = app.state::<Arc<WindowDestroyWatch>>();

    // 1. 掃除の経路（破棄の通知が失われていても、次の入口で必ず落ちる）。
    watch.forget_unresolvable(&label);

    // 2. 生成要求の位置。無ければ `None`（読み込む対象が無い）。
    let requested = app.state::<WindowRegistry>().document_of(label.as_str());

    // 3. 解決と写像。
    let (status, changed) = answer_state(&watch, &label, requested.as_deref());
    if changed {
        // 4. 遅延読み込みは**状態を変えた操作**である（フロントエンドはこれを購読して
        //    問い合わせ直す）。
        emit_session_changed(&window);
    }

    log::info!(
        "{command}: 呼び出し元ウィンドウ = {} / 状態 = {}",
        label.as_str(),
        describe_status(&status),
    );
    IpcResult::Ok {
        data: DocumentStateResponse { context, status },
    }
}

/// 出所へ（無ければ選ばれた位置へ）保存する（要件 5.1〜5.4、5.8）。
///
/// **`async fn` であり、保存と保存先の提示を `spawn_blocking` に載せる** — 提示は利用者が答える
/// までブロックし、保存は秒単位かかりうる。どちらもイベントループとランタイムのワーカーを
/// 塞いでならない（module doc「実行モデル」）。
///
/// **位置は応答へ写さない。** 提示は [`answer_save`] の内側で完結し、選ばれた位置は
/// `save_to` へ渡るだけで終わる。応答は状態と結果だけを運ぶ（design.md「Boundary
/// Commitments」）。
///
/// 経路レベルの失敗は `spawn_blocking` のタスクが異常終了した場合だけである（保存の処理が
/// panic した）。それ以外の失敗は**ドメインの結果**として成功腕に載る。
#[tauri::command]
pub async fn document_save(
    app: AppHandle,
    window: WebviewWindow,
) -> IpcResult<DocumentSaveResponse, IpcError> {
    let command = command_names::DOCUMENT_SAVE;
    let context = caller_context(&window);
    let label = context.window.clone();
    let watch = Arc::clone(&app.state::<Arc<WindowDestroyWatch>>());
    let saving = Arc::clone(&watch);
    let target = window.clone();

    let joined = tauri::async_runtime::spawn_blocking(move || {
        // 保存と提示は**このスレッドの内側で完結する**（位置は応答へ出ない）。
        answer_save(&saving, &label, |suggested| {
            dialog::pick_save_location(&target, suggested)
        })
    })
    .await;

    let (outcome, changed) = match joined {
        Ok(result) => result,
        Err(error) => {
            // **経路レベルの失敗**（保存の処理が異常終了した）。ドメインの失敗ではないので、
            // 封筒の失敗腕へ載せる唯一の経路である。
            log::error!(
                "{command}: ウィンドウ = {} の保存の処理が異常終了した: {error}",
                context.window.as_str(),
            );
            return IpcResult::Err {
                error: IpcError::Document {
                    message: format!("保存の処理が異常終了した: {error}"),
                },
            };
        }
    };

    if changed {
        emit_session_changed(&window);
    }
    let status = boundary_status(&watch, &context.window);
    log::info!(
        "{command}: 呼び出し元ウィンドウ = {} / 結果 = {} / 状態 = {}",
        context.window.as_str(),
        describe_save(&outcome),
        describe_status(&status),
    );
    IpcResult::Ok {
        data: DocumentSaveResponse {
            context,
            status,
            outcome,
        },
    }
}

/// 行も列も無いシートを 1 つ持つドキュメントを用意する（要件 7.1〜7.4）。
///
/// セッションを作る経路は [`WindowDestroyWatch::create`] だけである（表を直接触ると破棄の購読を
/// 伴わないセッションが生まれ、要件 1.5 が破れる）。**未保存の変更があれば拒否**し、その理由を
/// 運ぶ（要件 7.3。拒否は失敗ではない）。
///
/// 応答の `status` は作成の**あとの**状態である。作成が通れば名前は空文字（出所を持たない）で、
/// シートが 1 つ見える（要件 1.7）。
#[tauri::command]
pub fn document_new(
    app: AppHandle,
    window: WebviewWindow,
) -> IpcResult<DocumentNewResponse, IpcError> {
    let command = command_names::DOCUMENT_NEW;
    let context = caller_context(&window);
    let label = context.window.clone();
    let watch = app.state::<Arc<WindowDestroyWatch>>();

    let (outcome, changed) = answer_new(&watch, &label);
    if changed {
        emit_session_changed(&window);
    }
    let status = boundary_status(&watch, &label);
    log::info!(
        "{command}: 呼び出し元ウィンドウ = {} / 結果 = {} / 状態 = {}",
        label.as_str(),
        describe_new(&outcome),
        describe_status(&status),
    );
    IpcResult::Ok {
        data: DocumentNewResponse {
            context,
            status,
            outcome,
        },
    }
}

/// 未保存の印を落とす（**保存しない**。要件 6.5）。
///
/// 利用者が提示で「変更を破棄して閉じる」を選んだときだけ呼ばれる — 以後そのウィンドウは
/// 閉じてよいと答え、閉じる経路（`crate::window::close`）が破棄する。
///
/// 結果の型を持たない（失敗しうる操作ではない。設計の表）。保持していないウィンドウへの指示は
/// 何もせず、状態は「保持していない」のまま答える（**失敗は記録に残すだけ**である — 呼び出し元へ
/// 返す先が無い。応答の形は閉じた 3 つの腕を持つ）。
#[tauri::command]
pub fn document_discard(
    app: AppHandle,
    window: WebviewWindow,
) -> IpcResult<DocumentDiscardResponse, IpcError> {
    let command = command_names::DOCUMENT_DISCARD;
    let context = caller_context(&window);
    let label = context.window.clone();
    let watch = app.state::<Arc<WindowDestroyWatch>>();

    let (notify, failure) = answer_discard(&watch, &label);
    if let Some(error) = &failure {
        log::warn!(
            "{command}: ウィンドウ = {} へ破棄の印を付けられなかった: {}",
            label.as_str(),
            failure_reason(&label, error),
        );
    }
    if notify {
        emit_session_changed(&window);
    }
    let status = boundary_status(&watch, &label);
    log::info!(
        "{command}: 呼び出し元ウィンドウ = {} / 結果 = {} / 状態 = {}",
        label.as_str(),
        if notify {
            "破棄の印を付けた"
        } else {
            "変わらなかった"
        },
        describe_status(&status),
    );
    IpcResult::Ok {
        data: DocumentDiscardResponse { context, status },
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use app_shell::ipc::{DocumentOrigin, DocumentSessionStatus, DocumentSheet};
    use document_format::{CellValue, Document, DocumentFormat, DocumentFormatApi, SchemaPart};
    use document_session::{DocumentSessions, DocumentSessionsApi, SessionState};

    use super::{
        answer_discard, answer_new, answer_save, answer_state, boundary_status, count_to_u32,
        describe_new, describe_save, describe_status, origin_to_boundary,
    };
    use crate::dialog::SaveLocation;
    use crate::session::watch::testing::AlwaysPresent;
    use crate::session::watch::WindowDestroyWatch;
    use app_shell::ipc::WindowLabel;

    /// 一時ディレクトリ（`session/watch.rs` のテストと同じ規律。プロセスごとに一意）。
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("時計は 1970 以降である")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "jxcel-session-commands-{tag}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
            Self { path }
        }

        fn file(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }

        /// ディレクトリの中の項目（保存が起きていないことの観測に使う）。
        fn entries(&self) -> Vec<PathBuf> {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(&self.path)
                .expect("一時ディレクトリを読める")
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .collect();
            entries.sort();
            entries
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// 1 シート 1 行の**本物の文書**を書く（dev-dependency の `document-format` を使う）。
    fn write_document(path: &Path, sheet_name: &str, value: &str) {
        let mut document = Document::new();
        let sheet = document.add_sheet(sheet_name);
        document
            .set_sheet_columns(sheet, vec!["note".to_owned()])
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, SchemaPart::empty())
            .expect("標本のシートは実在する");
        let row = document.add_row(sheet).expect("標本のシートは実在する");
        document
            .set_row_values(sheet, row, vec![CellValue::Text(value.to_owned())])
            .expect("標本の行は実在する");
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
    }

    /// 二重のウィンドウの側から入口を作る（テストの標準の組み立て）。
    fn watch() -> WindowDestroyWatch {
        WindowDestroyWatch::new(Arc::new(AlwaysPresent), Arc::new(DocumentSessions::new()))
    }

    /// 未保存の変更を 1 つ適用する（**コアの適用の口**を通す。印を立てる唯一の経路）。
    fn mark_unsaved(watch: &WindowDestroyWatch, label: &WindowLabel) {
        let edited = watch
            .sessions()
            .edit(label, &mut |_document| ())
            .expect("保持しているウィンドウへ適用できる");
        assert!(edited.unsaved, "適用のあとは未保存である");
    }

    /// 保持している状態から名前と出所を取り出す（テストの読み取り補助）。
    fn summary(watch: &WindowDestroyWatch, label: &WindowLabel) -> (String, DocumentOrigin) {
        match boundary_status(watch, label) {
            DocumentSessionStatus::Open(summary) => (summary.name, summary.origin),
            other => panic!("保持していると期待した: {other:?}"),
        }
    }

    /// **保持していないウィンドウへの問い合わせは「保持していない」を返す**（要件 1.8 の問い側）。
    ///
    /// 読み取りなので**状態は変わらない**（イベントを送る側の真偽が偽である。要件 1.6）。
    #[test]
    fn a_fresh_window_answers_absent() {
        let watch = watch();
        let label = WindowLabel::new("empty-1");

        let (status, changed) = answer_state(&watch, &label, None);
        assert_eq!(DocumentSessionStatus::Absent, status);
        assert!(!changed, "読み取りは状態を変えない");

        // 生成要求の位置があっても、まだ読んでいないウィンドウを問い合わせるだけでは
        // 「保持していない」である（読み込みはこの入口が起こすが、位置を持つのはコマンド側
        // である。ここでは位置を渡していない）。
        let (again, changed) = answer_state(&watch, &label, None);
        assert_eq!(DocumentSessionStatus::Absent, again);
        assert!(!changed);
    }

    /// **起動時に指定された位置の読み込みは 1 度だけ起きる**（要件 1.2）。
    ///
    /// 呼び出しの形で観測する — 1 度目のあとにファイルを消す。2 度目に読み込みが起きるなら
    /// 失敗（`Unavailable`）になるはずであり、`Open` のままなら読み込みは起きていない。
    #[test]
    fn the_creation_request_is_read_once_and_never_twice() {
        let scratch = Scratch::new("resolve-once");
        let path = scratch.file("台帳.jxcel");
        write_document(&path, "台帳", "一度だけ");

        let watch = watch();
        let label = WindowLabel::new("doc-1");

        let (status, changed) = answer_state(&watch, &label, Some(&path));
        let DocumentSessionStatus::Open(summary) = status else {
            panic!("読み込めるはずである: {status:?}");
        };
        // **名前はファイル名のみ**（位置は境界を越えない）。
        assert_eq!("台帳.jxcel", summary.name);
        assert!(!summary.name.contains('/'), "位置が名前へ混ざっている");
        assert_eq!(DocumentOrigin::File, summary.origin);
        assert!(
            !summary.unsaved,
            "読み込みの完了で未保存は落ちる（要件 4.4）"
        );
        assert_eq!(
            vec![DocumentSheet {
                id: summary.sheets[0].id.clone(),
                name: "台帳".to_owned(),
                columns: 1,
                rows: 1,
            }],
            summary.sheets,
            "シートの一覧は識別子（文字列）と件数（32 ビット以下）で運ぶ"
        );
        assert_eq!(26, summary.sheets[0].id.len(), "識別子は ULID のテキスト形");
        assert!(changed, "読み込みは状態を変えた操作である");

        // 読み込みを起こせないようにする（ファイルを消す）。それでも `Open` のままなら、
        // 2 度目の問い合わせは読み込みを起こしていない。
        std::fs::remove_file(&path).expect("標本を消せる");
        let (again, changed) = answer_state(&watch, &label, Some(&path));
        assert!(
            matches!(again, DocumentSessionStatus::Open(_)),
            "2 度目のアクセスで読み込みが起きた: {again:?}"
        );
        assert!(!changed, "同じ状態への問い合わせはイベントを送らない");
    }

    /// **読み込めなかった理由は適応層が組み立て、2 度目も同じ文言になる**（要件 2.1）。
    ///
    /// 覚えた失敗は繰り返さない（問い合わせのたびに秒単位の読み込みを起こさない）ので、
    /// 2 度目は誤りではなく状態から文言を組み立てる — 両者が一致することをここで固定する。
    #[test]
    fn a_failed_load_answers_with_an_assembled_reason() {
        let scratch = Scratch::new("read-failure");
        let missing = scratch.file("存在しない.jxcel");

        let watch = watch();
        let label = WindowLabel::new("doc-1");

        let (status, changed) = answer_state(&watch, &label, Some(&missing));
        let DocumentSessionStatus::Unavailable { reason } = &status else {
            panic!("読み込めないはずである: {status:?}");
        };
        assert!(
            reason.contains("doc-1"),
            "どのウィンドウかが理由に無い: {reason}"
        );
        assert!(!reason.is_empty());
        assert!(changed, "覚えた失敗も状態の変化である");

        // 2 度目は読み込みを試みない（覚えている）。文言は初回と一致する。
        let (again, changed) = answer_state(&watch, &label, Some(&missing));
        assert_eq!(status, again, "同じ失敗の文言が揺れている");
        assert!(!changed, "覚えている失敗の問い合わせは状態を変えない");
    }

    /// **未保存のままの新規作成は拒否され、理由を運ぶ**（要件 7.3）。
    #[test]
    fn create_is_refused_while_unsaved_and_carries_a_reason() {
        let scratch = Scratch::new("refuse");
        let path = scratch.file("作業中.jxcel");
        write_document(&path, "作業中", "未保存");

        let watch = watch();
        let label = WindowLabel::new("doc-1");
        answer_state(&watch, &label, Some(&path));
        mark_unsaved(&watch, &label);

        let (outcome, changed) = answer_new(&watch, &label);
        let app_shell::ipc::DocumentNewOutcome::Refused { reason } = &outcome else {
            panic!("未保存なら拒否されるはずである: {outcome:?}");
        };
        assert!(reason.contains("doc-1"), "理由にウィンドウが無い: {reason}");
        assert!(
            reason.contains("未保存"),
            "理由が拒否の内容を伝えていない: {reason}"
        );
        assert!(!changed, "拒否は何も変えない");

        // 破棄の印を付けると通る（以後そのウィンドウは閉じてよい。要件 6.5）。
        let (notify, failure) = answer_discard(&watch, &label);
        assert!(notify, "破棄の印は状態を変える");
        assert!(
            failure.is_none(),
            "保持しているウィンドウへの破棄は失敗しない"
        );
        let (outcome, changed) = answer_new(&watch, &label);
        assert_eq!(app_shell::ipc::DocumentNewOutcome::Created, outcome);
        assert!(changed, "新規作成は状態を変える");
        let status = boundary_status(&watch, &label);
        let DocumentSessionStatus::Open(summary) = &status else {
            panic!("作成のあとは保持しているはずである: {status:?}");
        };
        assert_eq!("", summary.name, "出所を持たない文書の名前は空文字");
        assert_eq!(DocumentOrigin::New, summary.origin);
        assert!(
            !summary.unsaved,
            "作成は未保存でない状態から始まる（要件 7.2）"
        );
    }

    /// **出所を持たない文書の保存は保存先を尋ねる**（要件 5.2）。
    ///
    /// 提示へ渡る提案名を観測する（`NeedsLocation` を通ったことの証拠）。**取り消しでは何も
    /// 書き出さず、未保存を保つ**（要件 5.3）— ディレクトリが空であることと、状態の名前が
    /// 空文字（出所が確定していない）であることの両方で見る。
    #[test]
    fn saving_without_an_origin_asks_for_a_location_and_a_cancel_changes_nothing() {
        let scratch = Scratch::new("cancel");
        let watch = watch();
        let label = WindowLabel::new("empty-1");
        answer_new(&watch, &label);
        mark_unsaved(&watch, &label);

        let mut suggested = Vec::new();
        let (outcome, changed) = answer_save(&watch, &label, |name| {
            suggested.push(name.to_owned());
            SaveLocation::Cancelled
        });

        assert_eq!(
            vec![crate::dialog::DEFAULT_SAVE_NAME.to_owned()],
            suggested,
            "出所を持たない文書には既定の提案名を渡す"
        );
        assert_eq!(app_shell::ipc::DocumentSaveOutcome::Cancelled, outcome);
        assert!(!changed, "取り消しは状態を変えない");
        assert!(
            scratch.entries().is_empty(),
            "取り消しで書き出しが起きた: {:?}",
            scratch.entries()
        );
        let (name, origin) = summary(&watch, &label);
        assert_eq!("", name, "取り消しで出所が確定してはならない");
        assert_eq!(DocumentOrigin::New, origin);
        let DocumentSessionStatus::Open(summary) = boundary_status(&watch, &label) else {
            panic!("保持したままであるはずである");
        };
        assert!(summary.unsaved, "取り消しでは未保存を保つ（要件 5.3）");
    }

    /// **選ばれた位置へ書き出し、以後の出所になる**（要件 5.2、5.8）。
    #[test]
    fn saving_without_an_origin_writes_to_the_chosen_location_and_keeps_it_as_the_origin() {
        let scratch = Scratch::new("chosen");
        let watch = watch();
        let label = WindowLabel::new("empty-1");
        answer_new(&watch, &label);
        mark_unsaved(&watch, &label);

        let chosen = scratch.file("保存した.jxcel");
        let (outcome, changed) =
            answer_save(&watch, &label, |_name| SaveLocation::Chosen(chosen.clone()));

        assert_eq!(app_shell::ipc::DocumentSaveOutcome::Saved, outcome);
        assert!(
            changed,
            "保存の成功は状態を変える（未保存が落ち、出所が確定する）"
        );
        assert_eq!(
            vec![chosen.clone()],
            scratch.entries(),
            "1 つのファイルができる"
        );
        let (name, origin) = summary(&watch, &label);
        assert_eq!("保存した.jxcel", name, "以後の出所がファイル名として見える");
        assert_eq!(DocumentOrigin::File, origin);
        let DocumentSessionStatus::Open(summary) = boundary_status(&watch, &label) else {
            panic!("保持しているはずである");
        };
        assert!(!summary.unsaved, "保存の成功で未保存が落ちる（要件 5.5）");
    }

    /// 保持していないウィンドウへの保存は**失敗として理由つきで答える**（設計の表）。
    #[test]
    fn saving_a_window_without_a_document_fails_with_a_reason() {
        let watch = watch();
        let label = WindowLabel::new("empty-1");

        let (outcome, changed) = answer_save(&watch, &label, |_name| {
            panic!("保持していないので保存先を尋ねてはならない")
        });
        let app_shell::ipc::DocumentSaveOutcome::Failed { reason } = outcome else {
            panic!("失敗として答えなければならない");
        };
        assert!(
            reason.contains("empty-1"),
            "理由にウィンドウが無い: {reason}"
        );
        assert!(!changed);
    }

    /// **書き出しの失敗は理由つきで答える**（要件 5.4 の後半）。
    ///
    /// `save_outcome` の `SaveReport::Failed` の腕（write_failure_reason）はこれ以外に
    /// 到達するテストが無い — **コアが書き出しの失敗を返す実経路**（保存先が書けない）を通す。
    /// 未保存が保たれ、通知も送らない（失敗は状態を変えない）。
    #[test]
    fn a_failed_write_answers_with_a_reason_and_keeps_the_unsaved_mark() {
        let scratch = Scratch::new("write-failure");
        let watch = watch();
        let label = WindowLabel::new("empty-1");
        answer_new(&watch, &label);
        mark_unsaved(&watch, &label);

        // **書けない宛先**を選ばせる（既存のファイルを親に持つパスは、親がディレクトリでないため
        // 書き出しに失敗する）。`save_to` は失敗を結果として返す（panic しない）。
        let blocker = scratch.file("ふさぐ");
        std::fs::write(&blocker, b"not a directory").expect("標本のファイルを書ける");
        let unopenable = blocker.join("書けない.jxcel");

        let (outcome, changed) = answer_save(&watch, &label, |_name| {
            SaveLocation::Chosen(unopenable.clone())
        });

        let app_shell::ipc::DocumentSaveOutcome::Failed { reason } = outcome else {
            panic!("書き出せない場合は失敗として答えなければならない");
        };
        assert!(
            reason.contains("empty-1") && reason.contains("書き出せなかった"),
            "失敗の理由が用途を伝えていない: {reason}"
        );
        assert!(!changed, "失敗は状態を変えない（未保存は保たれる）");
        let DocumentSessionStatus::Open(summary) = boundary_status(&watch, &label) else {
            panic!("保持しているはずである");
        };
        assert!(
            summary.unsaved,
            "書き出しの失敗で未保存が落ちた（要件 5.4）"
        );
        assert!(
            !std::path::Path::new(&unopenable).exists(),
            "書けない宛先にファイルができた"
        );
    }

    /// **イベントを送る操作の数がちょうど 4 である**（design.md「セッション状態の通知」）。
    ///
    /// 読み取りと失敗・拒否・取り消しは状態を変えないので送らない。**数えるのは時間ではなく
    /// 状態変化の回数**である（`verification.md` の規律）。
    #[test]
    fn only_state_changing_operations_change_the_state() {
        let scratch = Scratch::new("emissions");
        let path = scratch.file("流れ.jxcel");
        write_document(&path, "流れ", "最初");
        let watch = watch();
        let label = WindowLabel::new("doc-1");

        // 観測した「状態が変わった」の列。コマンドはこれが真のときだけイベントを送る。
        let mut emissions: Vec<&'static str> = Vec::new();
        let mut observe = |changed: bool, what: &'static str| {
            if changed {
                emissions.push(what);
            }
        };

        // 読み取り（保持していない）
        let (_, changed) = answer_state(&watch, &label, None);
        observe(changed, "空の問い合わせ");
        // 遅延読み込み（1 つ目: 状態を変える）
        let (_, changed) = answer_state(&watch, &label, Some(&path));
        observe(changed, "読み込み");
        // 読み取り（2 つ目: 変わらない）
        let (_, changed) = answer_state(&watch, &label, Some(&path));
        observe(changed, "読み取り");
        // 拒否（未保存があるので新規作成できない）
        mark_unsaved(&watch, &label);
        let (_, changed) = answer_new(&watch, &label);
        observe(changed, "拒否");
        // 破棄の印（状態を変える）
        let (notify, failure) = answer_discard(&watch, &label);
        assert!(
            failure.is_none(),
            "保持しているウィンドウへの破棄は失敗しない"
        );
        observe(notify, "破棄の印");
        // 新規作成（状態を変える）
        let (_, changed) = answer_new(&watch, &label);
        observe(changed, "新規作成");
        mark_unsaved(&watch, &label);
        // 保存の取り消し（変わらない）
        let (_, changed) = answer_save(&watch, &label, |_name| SaveLocation::Cancelled);
        observe(changed, "取り消し");
        // 保存の成功（状態を変える）
        let chosen = scratch.file("流れ-保存.jxcel");
        let (_, changed) = answer_save(&watch, &label, |_name| SaveLocation::Chosen(chosen));
        observe(changed, "保存");
        // 保存のあとの読み取り（変わらない）
        let (_, changed) = answer_state(&watch, &label, None);
        observe(changed, "保存後の読み取り");

        assert_eq!(
            vec!["読み込み", "破棄の印", "新規作成", "保存"],
            emissions,
            "状態を変えた操作だけがイベントを送る"
        );
    }

    /// 状態・結果・記録の語が**互いに区別できる**（写像が閉じた列挙を網羅している）。
    ///
    /// `match` なので、境界の列挙に値が増えればこれらの関数がコンパイルエラーになる。
    #[test]
    fn every_boundary_value_has_its_own_record_word() {
        let statuses = [
            DocumentSessionStatus::Absent,
            DocumentSessionStatus::Open(document_summary()),
            DocumentSessionStatus::Unavailable {
                reason: "理由".to_owned(),
            },
        ];
        let mut seen = std::collections::BTreeSet::new();
        for status in &statuses {
            let word = describe_status(status);
            assert!(!word.is_empty());
            assert!(seen.insert(word), "記録の語が重複している: {word}");
        }

        let saves = [
            app_shell::ipc::DocumentSaveOutcome::Saved,
            app_shell::ipc::DocumentSaveOutcome::Cancelled,
            app_shell::ipc::DocumentSaveOutcome::Failed {
                reason: "理由".to_owned(),
            },
        ];
        let mut seen = std::collections::BTreeSet::new();
        for outcome in &saves {
            let word = describe_save(outcome);
            assert!(!word.is_empty());
            assert!(seen.insert(word), "記録の語が重複している: {word}");
        }

        let news = [
            app_shell::ipc::DocumentNewOutcome::Created,
            app_shell::ipc::DocumentNewOutcome::Refused {
                reason: "理由".to_owned(),
            },
        ];
        let mut seen = std::collections::BTreeSet::new();
        for outcome in &news {
            let word = describe_new(outcome);
            assert!(!word.is_empty());
            assert!(seen.insert(word), "記録の語が重複している: {word}");
        }
    }

    /// 純粋な写像の境界値: **出所の位置は落ち、件数は飽和する**（64 ビットを境界へ出さない）。
    #[test]
    fn the_mapping_drops_the_location_and_caps_the_counts() {
        assert_eq!(
            DocumentOrigin::File,
            origin_to_boundary(document_session::Origin::File(PathBuf::from(
                "/秘密/場所.jxcel"
            )))
        );
        assert_eq!(
            DocumentOrigin::New,
            origin_to_boundary(document_session::Origin::New)
        );
        assert_eq!(0, count_to_u32(0));
        assert_eq!(100_000, count_to_u32(100_000));
        assert_eq!(u32::MAX, count_to_u32(usize::MAX), "切り捨てず飽和させる");
    }

    /// **掃除の経路は状態を変えるが、通知は送らない**（送り先のウィンドウが既に無い）。
    ///
    /// 購読の登録に失敗する二重（`watch.rs` の `FakeWindows` に相当する最小のもの）を使い、
    /// 「引けないウィンドウのセッションが落ちる」ことを状態の変化として観測したうえで、
    /// `answer_state` の戻り値が偽であることを固定する（`Absent` への変化は通知しない）。
    #[test]
    fn releasing_a_gone_window_changes_the_state_without_a_notification() {
        struct GoneWindows;

        impl crate::session::watch::WindowDestroyEvents for GoneWindows {
            fn subscribe_destroyed(
                &self,
                _label: &WindowLabel,
                _on_destroyed: crate::session::watch::DestroyHandler,
            ) -> bool {
                false
            }

            fn has_window(&self, _label: &WindowLabel) -> bool {
                false
            }
        }

        let watch =
            WindowDestroyWatch::new(Arc::new(GoneWindows), Arc::new(DocumentSessions::new()));
        let label = WindowLabel::new("doc-消えた");

        let (status, notify) = answer_state(&watch, &label, None);
        assert_eq!(DocumentSessionStatus::Absent, status);
        assert!(!notify, "送り先の無い変化でイベントを送った");
        assert!(watch.forget_unresolvable(&label), "掃除の経路が落とす");
    }

    /// **`Open` から `Absent` へ落ちる変化は通知しない**（`should_notify` の `Absent` 節）。
    ///
    /// **このテストはその節を守っている唯一のテストである。** 節は「送り先のウィンドウが既に
    /// 無い変化では `emit` が必ず失敗する」ために在る（`should_notify` の doc）。節を落とすと
    /// **このテストだけが落ちる** — 隣の `releasing_a_gone_window_changes_the_state_without_a_notification`
    /// はセッションを持たないラベルを扱うため `before == after == Absent` となり、
    /// `before != after` の側で短絡して節まで到達しない（節を守っていることにならない）。
    ///
    /// 到達させるには**生きたウィンドウで本物の文書を読み込んでから**ウィンドウを引けなくする —
    /// 次の `answer_state` の `resolve` が `register` の中で「登録済みだがウィンドウが無い」を見て
    /// 表からセッションを落とすので、変化は実経路で `Open → Absent` になる。
    #[test]
    fn a_window_that_disappears_after_loading_changes_to_absent_without_a_notification() {
        use std::sync::atomic::{AtomicBool, Ordering};

        /// `has_window` を外から倒せる二重（倒すと `register` が `forget` を通る）。
        struct FlippableWindows {
            present: AtomicBool,
        }

        impl crate::session::watch::WindowDestroyEvents for FlippableWindows {
            fn subscribe_destroyed(
                &self,
                _label: &WindowLabel,
                _on_destroyed: crate::session::watch::DestroyHandler,
            ) -> bool {
                self.present.load(Ordering::SeqCst)
            }

            fn has_window(&self, _label: &WindowLabel) -> bool {
                self.present.load(Ordering::SeqCst)
            }
        }

        let scratch = Scratch::new("flip-to-absent");
        let path = scratch.file("消える窓.jxcel");
        write_document(&path, "消える窓", "読み込んでから消える");

        let windows = Arc::new(FlippableWindows {
            present: AtomicBool::new(true),
        });
        let watch = WindowDestroyWatch::new(windows.clone(), Arc::new(DocumentSessions::new()));
        let label = WindowLabel::new("doc-消える");

        // 1. 生きたウィンドウで本物の文書を読み込む（状態は `Open`）。
        let (status, notify) = answer_state(&watch, &label, Some(&path));
        assert!(
            matches!(status, DocumentSessionStatus::Open(_)),
            "読み込めていない: {status:?}"
        );
        assert!(notify, "読み込みは状態を変える操作である");

        // 2. ウィンドウを引けなくする（破棄の通知は失われた状況）。
        windows.present.store(false, Ordering::SeqCst);

        // 3. 変化は実経路で `Open → Absent` になる。**通知は送らない**（送り先が無い）。
        let (status, notify) = answer_state(&watch, &label, None);
        assert_eq!(
            DocumentSessionStatus::Absent,
            status,
            "引けないウィンドウのセッションが落ちていない"
        );
        assert!(
            !notify,
            "送り先の無い `Open → Absent` の変化でイベントを送った"
        );
    }

    /// **変えていない操作は通知しない**（`should_notify` が唯一の規則であることの固定）。
    ///
    /// 破棄の印は未保存でない文書には何もせず、出所を持つ文書の保存は未保存でないとき何も
    /// 変えない。「成功したから送る」と決め打つ実装は、この 2 つで偽のイベントを出す。
    #[test]
    fn operations_that_change_nothing_do_not_ask_for_a_notification() {
        let scratch = Scratch::new("no-change");
        let path = scratch.file("そのまま.jxcel");
        write_document(&path, "そのまま", "変更なし");

        let watch = watch();
        let label = WindowLabel::new("doc-1");
        answer_state(&watch, &label, Some(&path));

        // 読み込んだ直後は未保存でない。破棄の印は何も変えない。
        let (notify, failure) = answer_discard(&watch, &label);
        assert!(failure.is_none());
        assert!(!notify, "未保存でない文書への破棄の印がイベントを要求した");

        // 出所を持つ文書の保存は書き出すが、未保存でないので状態は変わらない。
        let (outcome, notify) = answer_save(&watch, &label, |_name| {
            panic!("出所を持つ文書の保存は保存先を尋ねない")
        });
        assert_eq!(app_shell::ipc::DocumentSaveOutcome::Saved, outcome);
        assert!(!notify, "未保存でない文書の保存がイベントを要求した");

        // 同じ生成要求での 2 度目の問い合わせも変わらない（読み込みは 1 度だけ）。
        let (_, notify) = answer_state(&watch, &label, Some(&path));
        assert!(!notify, "2 度目の問い合わせがイベントを要求した");
    }

    /// 保持している文書の要約（記録の語の検査が使う最小の値）。
    fn document_summary() -> app_shell::ipc::DocumentSummary {
        app_shell::ipc::DocumentSummary {
            name: "名前.jxcel".to_owned(),
            origin: DocumentOrigin::File,
            unsaved: false,
            sheets: Vec::new(),
        }
    }

    /// コアの状態の写しがそのまま境界へ写る（`SessionState::Open` の 1 経路）。
    #[test]
    fn the_summary_follows_the_core_state() {
        let watch = watch();
        let label = WindowLabel::new("empty-1");
        answer_new(&watch, &label);

        let state = watch.sessions().state(&label);
        assert!(matches!(state, SessionState::Open { .. }));
        let status = super::to_boundary(state, &label);
        let DocumentSessionStatus::Open(summary) = status else {
            panic!("写像が状態を落とした");
        };
        assert_eq!("", summary.name);
        assert_eq!(DocumentOrigin::New, summary.origin);
        assert_eq!(
            1,
            summary.sheets.len(),
            "新規はシートを 1 つ持つ（要件 7.1）"
        );
        assert_eq!("シート1", summary.sheets[0].name);
        assert_eq!(0, summary.sheets[0].rows);
        assert_eq!(0, summary.sheets[0].columns);
    }
}

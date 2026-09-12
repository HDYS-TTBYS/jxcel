//! ウィンドウとドキュメントの関連付けの問い合わせ — ドキュメントを関連付けていない
//! ウィンドウにだけ操作の導線を提示するための判定材料（タスク 9.6。要件 2.1、2.2）。
//!
//! 所有: `WindowManager` のレジストリ（design.md「Components and Interfaces → Adapter Layer」の
//! `WindowRegistry`）。**本モジュールは判定を写すだけで、関連付けを変更する経路を持たない。**
//!
//! # 関連付けの真実はレジストリにあり、ラベルの接頭辞にはない
//!
//! ウィンドウのラベルは `doc-<連番>` / `empty-<連番>` という**割り当て順の規約**である
//! （[`crate::window`] のモジュール doc）。関連付けの真実は生成要求
//! （[`WindowRequest::Document`](crate::window::WindowRequest) / `Empty`）が決め、生成の**前**に
//! レジストリへ記録される（[`WindowRegistry::document_of`] がそれを引く）。したがって:
//!
//! - **`empty-` で始まるから関連付けが無い、とは言えない。** 7.7 のファイル選択は選ばれた位置を
//!   実行時に [`DocumentHost::attach`](crate::ports::DocumentHost::attach) へ引き渡すが、
//!   **`attach` は記録された関連付けを書き換えない**（6.2 のポート契約は `may_close` と
//!   `attach` の 2 つだけで、「関連付けを書き換える」操作を持たない）。下流スペックの所有者が
//!   位置を受け取ってそのウィンドウのドキュメントを読み込んだとしても、レジストリの記録は
//!   生成時のままである。**接頭辞を真実として扱うと、記録と食い違った判断を外へ出す。**
//! - **`doc-` で始まるから関連付けがある、とも言えない。** `doc-<連番>` の関連付けが
//!   `Some(path)` であることは生成時の記録であって、所有者が引き渡しを拒否した場合
//!   （7.7 の `Rejected`）に記録が変わるわけではない。逆に、名前を規約から外れた形で作る
//!   経路が将来増えれば、接頭辞は何も保証しない。
//! - **単一の源を二重に持たない。** 7.5 は既に、メニュー項目の有効・無効をこのレジストリの
//!   `document_of` から決めている。接頭辞の解析を 2 つ目の判定として足すと、同じ問いに対する
//!   答えが 2 箇所に生まれる。
//!
//! したがってこの判定は、**レジストリの写像に記録された関連付けだけ**を読む。ウィンドウ側の
//! 実装（[`WindowRegistry::document_of`]）は「未登録」と「関連付け無し」をどちらも `None` で
//! 返すが、生きているウィンドウは必ず生成の**前**に登録される（生成中に閉じられても登録が
//! 漏れないための順序であり、[`crate::window`] のモジュール doc を参照）ので、コマンドを
//! 呼べる時点でそのラベルは登録済みである。したがって `None` は「関連付けが無い」を意味する。
//!
//! # 境界の形
//!
//! 応答は封筒（[`IpcResult`]）であり、呼び出し元ウィンドウの文脈を必ず含む（要件 4.6）。
//! **呼び出し元は Tauri が注入する [`WebviewWindow`] から取る**ので、フロントエンドが
//! ウィンドウの識別子を payload で申告する経路は存在しない（偽装できない。tasks.md 7.1）。
//! **要求の型は無い**（入力は操作対象のウィンドウだけである。[`can_close_window`] と同じ形）。
//!
//! **パスは境界を越えない。** 答えるのは関連付けの有無だけであり、どのドキュメントかは所有者
//! （下流スペック）が持つ（7.7 の `DocumentPickOutcome` と同じ方針）。
//!
//! [`can_close_window`]: crate::window::close::can_close_window
//! [`WebviewWindow`]: tauri::WebviewWindow
//! [`IpcResult`]: app_shell::ipc::IpcResult

use std::path::Path;

use app_shell::ipc::{
    command_names, IpcError, IpcResult, WindowContext, WindowDocumentState,
    WindowDocumentStateResponse, WindowLabel,
};
use tauri::{Manager, WebviewWindow};
use tauri_plugin_log::log;

use crate::window::WindowRegistry;

/// 呼び出し元ウィンドウにドキュメントが関連付けられているかを返す（タスク 9.6。要件 2.1、2.2）。
///
/// 9.6 の画面（`src/features/empty/EmptyWindowScreen.tsx`）だけが呼ぶ。手順は 3 つである:
///
/// 1. 呼び出し元ウィンドウを Tauri の注入から得て、境界の文脈 [`WindowContext`] へ写す
///    （要件 4.6）。**payload からウィンドウを受け取らない。**
/// 2. レジストリ（実行時の管理状態）からそのラベルの関連付けを引く
///    （[`WindowRegistry::document_of`]。要件 2.1）。
/// 3. 有無を境界の形 [`WindowDocumentState`] へ写して封筒で返す。
///
/// # 失敗しない
///
/// レジストリは「どのウィンドウに対しても答える」契約であり、このコマンドに失敗の腕は無い。
/// それでも封筒の誤り型を [`IpcError`] にしてあるのは、境界を越えるコマンドがすべて同じ封筒の
/// 形を取るためである（要件 4.4、tasks.md 7.1）。
#[tauri::command]
pub fn window_document_state(
    window: WebviewWindow,
) -> IpcResult<WindowDocumentStateResponse, IpcError> {
    let context = WindowContext {
        window: WindowLabel::new(window.label()),
    };

    // **レジストリの記録だけを読む。** ラベルの接頭辞は判定に使わない（モジュール doc）。
    let document = window
        .app_handle()
        .state::<WindowRegistry>()
        .document_of(context.window.as_str());
    let state = to_boundary(document.as_deref());

    log::info!(
        "{}: 呼び出し元ウィンドウ = {} / 関連付け = {}",
        command_names::WINDOW_DOCUMENT_STATE,
        context.window.as_str(),
        describe(state),
    );

    IpcResult::Ok {
        data: WindowDocumentStateResponse { context, state },
    }
}

/// レジストリが記録した関連付けを境界の形（[`WindowDocumentState`]）へ写す。**純粋関数。**
///
/// 写像を 1 箇所に固定するのは、記録の運び方を変えたときに境界の形（生成物
/// `src/ipc/bindings.ts`）との対応が崩れないようにするためである（7.6 の `to_boundary` と同じ
/// 理由）。**パスそのものは使わない** — 有無だけが問いである（要件 2.2）。
fn to_boundary(document: Option<&Path>) -> WindowDocumentState {
    if document.is_some() {
        WindowDocumentState::Associated
    } else {
        WindowDocumentState::Unassociated
    }
}

/// 記録に出す関連付けの 1 行。**パスは書かない**（境界の形と同じ理由。関連付けの有無だけを
/// 事後に追えるようにする）。
fn describe(state: WindowDocumentState) -> &'static str {
    match state {
        WindowDocumentState::Unassociated => "ドキュメントなし",
        WindowDocumentState::Associated => "ドキュメントあり",
    }
}

#[cfg(test)]
mod tests {
    use super::{describe, to_boundary};
    use app_shell::ipc::WindowDocumentState;
    use std::path::Path;

    /// レジストリが記録した `None`（関連付け無し）だけが `Unassociated` になり、
    /// **パスが 1 つでもあれば `Associated`** であることを固定する（要件 2.2 の判定材料）。
    /// パスの中身は見ない（存在しないパスでも関連付けの事実は変わらない）。
    #[test]
    fn the_boundary_state_follows_the_recorded_association() {
        assert_eq!(
            to_boundary(None),
            WindowDocumentState::Unassociated,
            "関連付けが無いウィンドウだけが操作の導線を提示する"
        );
        assert_eq!(
            to_boundary(Some(Path::new("/tmp/jxcel-9-6-listening.csv"))),
            WindowDocumentState::Associated,
        );
        // 存在しないパスでも「関連付けがある」ことは記録の事実である（本コマンドはパスを
        // 読まない・存在確認しない）。
        assert_eq!(
            to_boundary(Some(Path::new("/tmp/jxcel-9-6-does-not-exist.csv"))),
            WindowDocumentState::Associated,
        );
    }

    /// 2 つの状態がどちらも記録用の語を持つ（写像が閉じた列挙を網羅している）。
    /// `match` なので、境界の列挙に値が増えればこの関数がコンパイルエラーになる。
    #[test]
    fn every_state_has_a_record_description() {
        assert_eq!(
            describe(WindowDocumentState::Unassociated),
            "ドキュメントなし"
        );
        assert_eq!(
            describe(WindowDocumentState::Associated),
            "ドキュメントあり"
        );
    }
}

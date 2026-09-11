//! ウィンドウの終了拒否の仲介 — ドキュメントを所有する機能へ終了の可否を問い合わせ、
//! 拒否された場合はウィンドウを閉じない。
//!
//! 所有: `WindowCloseGate`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 2.6。
//!
//! # なぜ「待ってから拒否する」ことができないのか
//!
//! 基盤（Tauri ランタイム）は `CloseRequestApi::prevent_close()` の結果を**非ブロッキングに
//! 読む**（`try_recv()`。research.md「ウィンドウを閉じる操作の拒否」）。したがって終了要求の
//! コールバックの中で「委譲先の判定が返るまで待つ」ことはできない。判定が非同期に決まる以上、
//! **拒否は先に確定させ、可否の往復を後から載せる**しかない。
//!
//! # 二段構えの仕組み（Rust 側は拒否を足さない）
//!
//! 1. **基盤の自動拒否**。Tauri は `tauri://close-requested` に対する **JS リスナが登録されて
//!    いることだけを検出して** `prevent_close()` を呼ぶ（`tauri` 2.11.5 の
//!    `src/manager/window.rs`: `WindowEvent::CloseRequested { api } => { if
//!    window.has_js_listener(WINDOW_CLOSE_REQUESTED_EVENT) { api.prevent_close(); } … }`。
//!    このリポジトリの `.cargo` に固定した実物を確認済み）。つまり**フロントエンドの購読が
//!    存在するだけで、ウィンドウは必ず一度拒否される**。
//! 2. **非同期の往復**。拒否された後、フロントエンドが [`can_close_window`] を呼び、委譲点
//!    [`DocumentHostPort`](crate::ports::DocumentHostPort) の判定を得る。`Allow` のときだけ
//!    `destroy()` でウィンドウを破棄し、`Deny` のときは何もしない（ウィンドウは閉じない）。
//!
//! **本モジュールは Rust 側に 2 つ目の拒否を足さない。** `Builder::on_window_event`
//! （`crate::window::on_window_event`）は `WindowEvent::CloseRequested` を見ても何もせず、
//! 形状の観測（要件 2.7、タスク 6.3）だけを行う。ここで `api.prevent_close()` を重ねると、
//! 基盤が既に行っている拒否と二重になり、`destroy()` の経路まで塞ぐ危険がある。
//!
//! # 閉じるのは `destroy()` だけ（`close()` は使わない）
//!
//! 拒否が解除された後に閉じるときは、**終了要求を再発火しない方の操作**を使う。
//! `Window::close()` は `WindowEvent::CloseRequested` を再発火するため、上の 1 に再突入して
//! 再び拒否される（research.md。無限ループの入口になる）。`destroy()` は `CloseRequested` を
//! 発火せずにウィンドウを破棄するので、**閉じる操作はこれだけである**。
//!
//! 破棄は `WindowEvent::Destroyed` を発火し、そこで位置とサイズが保存される（要件 2.7、
//! タスク 6.3）。**拒否されただけのウィンドウは保存されない** — 観測は `CloseRequested` でも
//! 行うが、書き込みは `Destroyed` のときだけだからである。
//!
//! # コマンド面
//!
//! [`can_close_window`] は命令的には読み取り専用である。呼び出し元ウィンドウは Tauri が注入する
//! [`WebviewWindow`] から取り、**フロントエンドの payload からは受け取らない**（偽装できない。
//! 要件 4.6、tasks.md 7.1）。判定は [`DocumentHostPort`] を**実行時に**引く
//! （tasks.md 6.2 の申し送り: 7.6 はこの経路を実際に通すこと）。ポートの契約どおり、
//! **ハンドラはブロックも待機もしない** — 現在の判定をその場で返すだけである。
//!
//! 応答は封筒（[`IpcResult`]）である。**`Deny` は封筒の失敗腕ではない** — 委譲先が
//! 「閉じてはならない」と答えた正常な応答であり、失敗（`kind: "Window"`）と混同してはならない。
//! そのため拒否の理由は `verdict` の内側に載せ、フロントエンドが利用者へ提示できるようにする。

use app_shell::ipc::{
    CanCloseWindowResponse, IpcError, IpcResult, WindowCloseVerdict, WindowContext, WindowLabel,
};
use tauri::{Manager, WebviewWindow};
use tauri_plugin_log::log;

use crate::ports::{CloseVerdict, DocumentHostPort};

/// ウィンドウを閉じてよいかをドキュメント所有者へ問い合わせる（要件 2.6）。
///
/// フロントエンドの終了要求の購読（`src/shell/closeVeto.ts`）だけが呼ぶ。手順は 3 つである:
///
/// 1. 呼び出し元ウィンドウを Tauri の注入から得て、境界の文脈 [`WindowContext`] へ写す
///    （要件 4.6）。**payload からウィンドウを受け取らない。**
/// 2. [`DocumentHostPort`] を実行時の管理状態から引き、そのウィンドウの判定を読む
///    （要件 2.6）。**待たない** — ポートは現在の判定を即座に返す契約である。
/// 3. 判定を境界の型 [`WindowCloseVerdict`] へ写して封筒で返す。
///
/// # 失敗しない
///
/// 委譲点は「どのウィンドウに対しても答える」契約であり（`src-tauri/src/ports.rs`）、
/// このコマンドに失敗の腕は無い。それでも封筒の誤り型を [`IpcError`] にしてあるのは、
/// 境界を越えるコマンドがすべて同じ封筒の形を取るためである（要件 4.4、tasks.md 7.1）。
#[tauri::command]
pub fn can_close_window(window: WebviewWindow) -> IpcResult<CanCloseWindowResponse, IpcError> {
    let context = WindowContext {
        window: WindowLabel::new(window.label()),
    };

    // **委譲点は実行時に引く。**`install`（下流スペックの差し替え）は `Builder::build` より前に
    // 走るので、ここで取れるのは常に最新の宿主である（`ports.rs` のモジュール doc を参照）。
    let verdict = window
        .app_handle()
        .state::<DocumentHostPort>()
        .may_close(&context.window);

    log::info!(
        "{}: 呼び出し元ウィンドウ = {} / 判定 = {}",
        app_shell::ipc::command_names::CAN_CLOSE_WINDOW,
        context.window.as_str(),
        describe_verdict(&verdict),
    );

    IpcResult::Ok {
        data: CanCloseWindowResponse {
            context,
            verdict: to_boundary(verdict),
        },
    }
}

/// 委譲点の判定（[`CloseVerdict`]）を境界の形（[`WindowCloseVerdict`]）へ写す。**純粋関数。**
///
/// 写像を 1 箇所に固定するのは、拒否の理由の運び方を変えたときに境界の形（生成物
/// `src/ipc/bindings.ts`）との対応が崩れないようにするためである。
fn to_boundary(verdict: CloseVerdict) -> WindowCloseVerdict {
    if verdict.is_allow() {
        return WindowCloseVerdict::Allow;
    }
    WindowCloseVerdict::Deny {
        reason: verdict.reason().unwrap_or_default().to_owned(),
    }
}

/// 記録に出す判定の 1 行。**理由の文言も残す**（拒否が起きたことを事後に追えるようにする）。
fn describe_verdict(verdict: &CloseVerdict) -> String {
    match verdict.reason() {
        None => "許可".to_owned(),
        Some(reason) => format!("拒否（{reason}）"),
    }
}

#[cfg(test)]
mod tests {
    use super::{describe_verdict, to_boundary};
    use crate::ports::CloseVerdict;
    use app_shell::ipc::WindowCloseVerdict;

    /// 許可の判定は境界でも許可になる。
    #[test]
    fn allow_maps_to_the_allow_variant() {
        assert_eq!(to_boundary(CloseVerdict::Allow), WindowCloseVerdict::Allow);
    }

    /// 拒否の判定は**理由を保ったまま**境界の拒否になる。理由を落とすと、利用者へ何も
    /// 伝えられないままウィンドウが閉じなくなる（要件 2.6 の「伝えられるだけの情報」）。
    #[test]
    fn deny_maps_to_the_deny_variant_with_its_reason() {
        assert_eq!(
            to_boundary(CloseVerdict::Deny {
                reason: "未保存の変更がある".to_owned(),
            }),
            WindowCloseVerdict::Deny {
                reason: "未保存の変更がある".to_owned(),
            }
        );
    }

    /// 理由が空でも拒否は拒否として運ぶ（理由の有無で可否が変わってはならない）。
    #[test]
    fn deny_without_a_reason_is_still_a_deny() {
        assert_eq!(
            to_boundary(CloseVerdict::Deny {
                reason: String::new(),
            }),
            WindowCloseVerdict::Deny {
                reason: String::new(),
            }
        );
    }

    /// 記録用の 1 行に可否と理由の両方が現れる（拒否の追跡が記録だけでできる）。
    #[test]
    fn the_log_line_distinguishes_allow_from_deny() {
        assert_eq!(describe_verdict(&CloseVerdict::Allow), "許可");
        assert_eq!(
            describe_verdict(&CloseVerdict::Deny {
                reason: "理由".to_owned(),
            }),
            "拒否（理由）"
        );
    }
}

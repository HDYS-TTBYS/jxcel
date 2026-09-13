//! ドキュメント所有者（`document-session`）の適応層 — セッションの設置と、宿主の連鎖。
//!
//! 所有: 本モジュールは `session/` の入口である（design.md「File Structure Plan」の
//! `src-tauri/src/session/mod.rs`。タスク 3.2）。要件: 1.3、2.2、4.6、6.1。
//!
//! 本モジュールが置くのは [`install`] の 1 つだけである。**3.2 の非目標**（コマンドの登録 =
//! 3.3/3.4、保存先の選択 = 3.5、メニュー = 3.6、破棄の購読 = 3.3）はここに含めない。
//!
//! # 起動の結線
//!
//! `lifecycle::run` が `dialog::install(handle)` の**直後**に `session::install(handle)` を
//! 1 行だけ呼ぶ（design.md「SessionDocumentHost」の Integration）。順序が意味を持つのは、
//! 次の 2 つをどちらも満たすためである:
//!
//! 1. **メニューの登録口が構築済みであること** — [`install`] は `MenuRegistry` を要さないが、
//!    `dialog::install` と同じ起動段（`Builder::build` の中で `app.run` より前）に置くことで、
//!    以後のタスク（3.6 のメニュー項目）が同じ並びへ足せる
//! 2. **宿主のポートが既に管理状態にあること** — [`install`] は
//!    `app.state::<DocumentHostPort>()` を引く。ポートは構築の前（`lifecycle::run` の
//!    `Builder::manage`）に置かれている
//!
//! # なぜ連鎖なのか（検証用の宿主を保存する）
//!
//! `DocumentHostPort` は 1 つの宿主しか持てない（`Manager::manage` は型ごとに 1 回しか置けず、
//! 後から入れ替える口も無い。`ports.rs` の「差し替え（下流スペックの接続点）」）。したがって
//! セッションの宿主を入れるとき、**現在の宿主を [`DocumentHostPort::host`] で取り出して
//! から**内側に置く。そうしなければ設置前にポートへ入っていた宿主が捨てられる。
//!
//! 捨ててはならない相手は `verification-triggers` feature の**検証用の宿主**
//! （`ports.rs` の `VerificationDocumentHost`）である。この宿主は 2 つの完了状態の実測根拠で
//! あり、どちらも設置で失われてはならない:
//!
//! - **拒否の実測**（タスク 7.6）: 名指しされたラベルのウィンドウの終了を拒否する。連鎖の
//!   内側に居続けるからこそ、セッションが許可を返したときにその拒否がそのまま生きる
//!   （`host.rs` の `may_close`）
//! - **引き渡しの記録**（タスク 7.7。要件 2.4）: `attach` が受け取った（ウィンドウ, 位置）を
//!   記録する。連鎖はセッションが受け取ったあとに内側へも渡すので、記録が残り続ける
//!
//! これが連鎖の唯一の存在理由である（design.md「SessionDocumentHost」の Risks も同旨:
//! 内側の宿主が 3 つ目のメソッドを得たら写像の更新が要る）。
//!
//! # セッションの実体は管理状態にも置く
//!
//! [`install`] は `Arc<DocumentSessions>` を 1 つ作り、**宿主に渡すのと同じ実体を
//! `app.manage` でも置く**。後続のタスク（3.4 の 4 コマンド、3.3 の破棄の購読）が
//! `app.state::<Arc<DocumentSessions>>()` から**同じ表**を取れるようにするためである。
//! 別々の実体を作ると、コマンドが宿主とは別の表を見て「開いているドキュメント」の真実が
//! 2 つに割れる。

pub mod host;

use std::sync::Arc;

use document_session::DocumentSessions;
use tauri::{AppHandle, Manager};

use crate::ports::DocumentHostPort;
use crate::session::host::SessionDocumentHost;

/// ドキュメント所有者を設置する（design.md「SessionDocumentHost」の Integration。タスク 3.2）。
///
/// 行うことは 3 つである:
///
/// 1. セッションの表を 1 実体作り（`Arc<DocumentSessions>`）、**管理状態としても置く**
///    （`app.manage`。3.4 のコマンドと 3.3 の購読が同じ実体を取れるようにする）
/// 2. `app.state::<DocumentHostPort>()` から**現在の宿主を [`DocumentHostPort::host`] で
///    取り出す**（設置前の宿主を内側に保つ。モジュール doc「なぜ連鎖なのか」）
/// 3. セッション → 内側の連鎖（[`SessionDocumentHost`]）を
///    [`DocumentHostPort::install`] でポートへ入れる
///
/// **この関数は `app.run` より前に呼ぶ**（`lifecycle::run` の `Builder::build` の後、
/// `dialog::install` の直後）。走り出した後でも安全に呼べるが、その時点の判定と引き渡しが
/// 新しい宿主へ切り替わる（`DocumentHostPort::install` の契約）。
///
/// **パニックしない。**`Manager::manage` は既に同じ型が置かれていれば `false` を返すだけであり
/// （上書きしない）、`app.state` は `lifecycle::run` が構築の前に置いたポートを見つける。
/// 二重に呼ばれた場合は、2 つ目のセッション（空の表）を管理状態へ置こうとして `manage` が
/// `false` を返す — その場合も**既に置かれた 1 つ目の実体が正**であり、連鎖は 1 つ目を内側へ
/// 積む（空の 2 つ目がコマンドから見えることは無い）。
pub fn install(app: &AppHandle) {
    // 1. セッションの実体を 1 つ作り、宿主と管理状態の双方で同じものを共有する。
    let sessions = Arc::new(DocumentSessions::new());
    // 3.4 のコマンドと 3.3 の購読が `app.state::<Arc<DocumentSessions>>()` で取れるようにする。
    //    既に置かれている場合（二重の `install`）は `false` が返るだけで、上書きはしない。
    let _ = app.manage(Arc::clone(&sessions));

    // 2. 設置前の宿主を内側に保つ（検証用の宿主の拒否と記録を捨てない）。
    let port = app.state::<DocumentHostPort>();
    let inner = port.host();

    // 3. 連鎖をポートへ入れる。以後の判定と引き渡しはセッションを先に見る。
    port.install(Arc::new(SessionDocumentHost::new(sessions, inner)));
}

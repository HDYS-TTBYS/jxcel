//! ドキュメント所有者（`document-session`）の適応層 — セッションの設置と、宿主の連鎖。
//!
//! 所有: 本モジュールは `session/` の入口である（design.md「File Structure Plan」の
//! `src-tauri/src/session/mod.rs`。タスク 3.2、破棄の購読はタスク 3.3）。要件: 1.3、1.5、2.2、
//! 4.6、6.1。
//!
//! 本モジュールが置く起動の結線は [`install`] の 1 つだけである。4 コマンドは [`commands`] に
//! あり、メニューの 2 項目（「新規」「保存」）は [`menu`] にあり、保存先の選択は `crate::dialog`
//! が担う（3.5）。
//!
//! # 起動の結線
//!
//! `lifecycle::run` が `dialog::install(handle)` の**直後**に `session::install(handle)` を
//! 1 行だけ呼ぶ（design.md「SessionDocumentHost」の Integration）。順序が意味を持つのは、
//! 次の 2 つをどちらも満たすためである:
//!
//! 1. **メニューの登録口が構築済みであること** — [`install`] は [`menu::install`] で
//!    `MenuRegistry` を引く（`lifecycle::run` の `menu::install` が管理状態へ置いた後）。
//!    `dialog::install` と同じ起動段（`Builder::build` の中で `app.run` より前）に並ぶので、
//!    **ウィンドウが 1 枚も無い時点で登録だけが済む**（配置は生成時に受け取る）
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
//! # セッションの実体は管理状態にも置く（破棄の購読ごと）
//!
//! [`install`] は `Arc<DocumentSessions>`（表）と [`WindowDestroyWatch`]（セッションを作る
//! 3 つの入口 + 破棄の購読）を 1 つずつ作り、**宿主に渡すのと同じ実体を `app.manage` でも
//! 置く**。後続の [`commands`]（4 コマンド）が `app.state::<Arc<WindowDestroyWatch>>()` から
//! **同じ入口**を取れるようにするためである。別々の実体を作ると、コマンドが宿主とは別の表を
//! 見て「開いているドキュメント」の真実が 2 つに割れ、**登録済みラベルの集合も 2 つに割れて
//! 購読が二重に登録される**（`watch.rs` の「ウィンドウ 1 つにつき 1 回」）。

pub mod commands;
pub mod host;
pub mod menu;
pub mod watch;

use std::sync::Arc;

use document_session::DocumentSessions;
use tauri::{AppHandle, Manager};

use crate::ports::DocumentHostPort;
use crate::session::host::SessionDocumentHost;
use crate::session::watch::{TauriWindowEvents, WindowDestroyWatch};

/// ドキュメント所有者を設置する（design.md「SessionDocumentHost」の Integration。タスク 3.2、
/// 破棄の購読は 3.3）。
///
/// 行うことは 3 つである:
///
/// 1. セッションの表（`Arc<DocumentSessions>`）と、セッションを作る 3 つの入口 + 破棄の購読
///    （`Arc<WindowDestroyWatch>`）を 1 実体ずつ作り、**管理状態としても置く**
///    （`app.manage`。3.4 の [`commands`] が同じ入口を取れるようにする）
/// 2. `app.state::<DocumentHostPort>()` から**現在の宿主を [`DocumentHostPort::host`] で
///    取り出す**（設置前の宿主を内側に保つ。モジュール doc「なぜ連鎖なのか」）
/// 3. セッション → 内側の連鎖（[`SessionDocumentHost`]）を
///    [`DocumentHostPort::install`] でポートへ入れる
/// 4. [`menu::install`] で「新規」と「保存」を登録口へ登録する（タスク 3.6。要件 5.1、7.1）。
///    **1 の後**でなければならない — 活性化の処理が管理状態の同じ入口
///    （`app.state::<Arc<WindowDestroyWatch>>()`）を引くためである
///
/// **この関数は `app.run` より前に呼ぶ**（`lifecycle::run` の `Builder::build` の後、
/// `dialog::install` の直後）。走り出した後でも安全に呼べるが、その時点の判定と引き渡しが
/// 新しい宿主へ切り替わる（`DocumentHostPort::install` の契約）。
///
/// **パニックしない。**`Manager::manage` は既に同じ型が置かれていれば `false` を返すだけであり
/// （上書きしない）、`app.state` は `lifecycle::run` が構築の前に置いたポートを見つける。
/// 二重に呼ばれた場合は、2 つ目の入口（空の表と空の登録済み集合）を管理状態へ置こうとして
/// `manage` が `false` を返す — その場合も**既に置かれた 1 つ目の実体が正**であり、連鎖は
/// 1 つ目を内側へ積む（空の 2 つ目がコマンドから見えることは無い）。
pub fn install(app: &AppHandle) {
    // 1. セッションの実体を 1 つ作り、入口と管理状態の双方で同じものを共有する。
    let sessions = Arc::new(DocumentSessions::new());
    // 破棄の購読は**適応層側のウィンドウの側**（`TauriWindowEvents`）を通す。購読と
    // セッションの生成を 1 つの入口に閉じることで、「購読を伴わないセッション」が
    // コマンドからも宿主からも作れなくなる（`watch.rs`）。
    let watch = Arc::new(WindowDestroyWatch::new(
        Arc::new(TauriWindowEvents::new(app.clone())),
        Arc::clone(&sessions),
    ));
    // 3.4 のコマンド（`commands`）が `app.state::<Arc<WindowDestroyWatch>>()` で同じ入口を
    //    取れるようにする。既に置かれている場合（二重の `install`）は `false` が返るだけで、
    //    上書きはしない。
    let _ = app.manage(Arc::clone(&watch));

    // 2. 設置前の宿主を内側に保つ（検証用の宿主の拒否と記録を捨てない）。
    let port = app.state::<DocumentHostPort>();
    let inner = port.host();

    // 3. 連鎖をポートへ入れる。以後の判定と引き渡しはセッションを先に見る。
    port.install(Arc::new(SessionDocumentHost::new(watch, inner)));

    // 4. 「新規」と「保存」をメニューへ登録する（タスク 3.6。要件 5.1、7.1）。**登録口が
    //    構築済みであることが前提**であり、`lifecycle::run` の `menu::install` → `dialog::install`
    //    → ここ、という並びがそれを満たす。セッションの表を管理状態へ置いた後なので、活性化の
    //    処理（`spawn_blocking` の中で `app.state::<Arc<WindowDestroyWatch>>()` を引く）が
    //    同じ入口を取れる（`session/menu.rs` の module doc）。
    menu::install(app);
}

//! jxcel デスクトップアプリケーションのエントリポイント（Tauri v2 アダプタ）。
//!
//! 所有: プロセスの入口。`tauri::Builder` を組み立てて実行するだけであり、業務ロジックを
//! 持たない（structure.md「Tauri アプリケーション」）。起動されるウィンドウは
//! `tauri.conf.json` の `app.windows` が宣言する 1 枚（ラベル `main`）である。
//!
//! 要件: 1.1（Windows / macOS / Linux それぞれで単一のファイルとして起動できる配布物）,
//! 1.2（追加のランタイム・ライブラリ・フレームワーク無しで起動）。本ファイルは
//! タスク 1.3 が置いた足場であり、ウィンドウを 1 枚開くのに必要な範囲を超える振る舞いを
//! 持たない。次のタスクがここへ書き込む:
//!
//! - タスク 5.1: 起動順序の固定（描画の代替経路を適用する場所の確保 → 単一インスタンスの
//!   登録 → 診断の初期化 → 残留プロセスの掃除 → 構築 → 実行）と、起動を継続できない前提の
//!   報告。回避策の環境変数は GTK / WebKit のコードが動く前、`Builder` の前で設定する
//!   （決定 7）。
//! - タスク 5.2: 記録機構の登録と保持方針の適用（要件 8.1、8.5）。
//! - タスク 5.3: 通信内容保護方針の設定（要件 1.6、8.3）。
//! - タスク 6.1: `app.windows` が宣言するウィンドウのラベルは、ウィンドウのレジストリと
//!   ラベル規約（`doc-<連番>` / `empty-<連番>`）を所有するこのタスクが引き取る。

// Windows のコンソールウィンドウを抑止する（release ビルドのみ）。配布物が単一の
// 実行ファイルとして「ウィンドウが 1 枚開く」ために必要であり、テンプレート既定の結線である。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dialog;
mod lifecycle;
mod menu;
mod ports;
mod sidecar_host;
mod watchdog;
mod window;

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("jxcel の起動に失敗しました（Tauri ランタイムを初期化できません）");
}

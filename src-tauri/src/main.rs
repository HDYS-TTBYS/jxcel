//! jxcel デスクトップアプリケーションのエントリポイント（Tauri v2 アダプタ）。
//!
//! 所有: プロセスの入口。起動の順序・単一インスタンス化・起動を継続できない前提の報告は
//! [`lifecycle`]（`AppLifecycle`）が所有し、ここはその結果を提示して終了コードに写すだけである。
//! 業務ロジックを持たない（structure.md「Tauri アプリケーション」）。
//!
//! 要件: 1.1（Windows / macOS / Linux それぞれで単一のファイルとして起動できる配布物）,
//! 1.2（追加のランタイム・ライブラリ・フレームワーク無しで起動）, 1.4（前提不成立時に
//! 満たされなかった前提を名指しして無言で終了しない）。
//!
//! 起動されるウィンドウは `tauri.conf.json` の `app.windows` が宣言する 1 枚（ラベル `main`）
//! に加え、二重起動を引き継いだときに [`lifecycle`] が提示するものである。ラベル規約と
//! ウィンドウのレジストリはタスク 6.1 が引き取る。

// Windows のコンソールウィンドウを抑止する（release ビルドのみ）。配布物が単一の
// 実行ファイルとして「ウィンドウが 1 枚開く」ために必要であり、テンプレート既定の結線である。
// 副作用として、release の Windows では stderr が見えなくなるため、前提不成立の提示が
// 端末に限られる（[`lifecycle::report_startup_failure`] に限界を記した）。
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
    // 起動の順序と単一インスタンス化は `lifecycle` が所有する。前提が満たされなかった場合は、
    // その事実を提示して非 0 で終了する（要件 1.4。**無言で終了しない**）。パニックに頼らないのは、
    // パニックの既定の出力が配布物（Windows の GUI サブシステム）では見えにくく、満たされなかった
    // 前提を名指ししないためである。
    if let Err(error) = lifecycle::run() {
        lifecycle::report_startup_failure(&error);
        std::process::exit(1);
    }
}

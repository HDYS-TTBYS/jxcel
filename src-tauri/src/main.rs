//! jxcel デスクトップアプリケーションのエントリポイント（Tauri v2 アダプタ）。
//!
//! 起動順序は design.md「File Structure Plan」が固定する: 環境変数（描画の代替経路）の
//! 条件付き適用 → single-instance の登録 → `tauri::Builder` → `run`。このうち
//! 環境変数の適用は GTK / WebKit のコードが動く前でなければ意味を持たない（決定 7）。
//!
//! `tauri::generate_context!()` はコンパイル時に `tauri.conf.json` を要求するため、
//! この結線は設定ファイルを追加するタスク 1.3 が行う。本タスク（1.1）ではワークスペースの
//! members を解決させ `cargo build --workspace --all-targets` を成立させるための
//! 最小の `main` を置く。タスク 1.3 がこの本体を Builder の起動に置き換える。

fn main() {}

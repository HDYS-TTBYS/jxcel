//! 親ウィンドウを指定したネイティブファイル選択 — OS 標準のファイル選択手段を提示し、
//! 選ばれた位置をドキュメント所有者へ渡す。
//!
//! 所有: `DialogGate`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 2.4。
//!
//! 本ファイルはタスク 1.3 が置いた空のモジュール骨組みである。実体を埋めるのはタスク 7.7。
//! 実装上の制約（design.md より）: **親ウィンドウの指定が必須である。**複数ウィンドウの
//! アプリで親を指定しないダイアログは誤ったウィンドウに乗る。選択されたパスは
//! `DocumentHost::attach`（[`crate::ports`]）へ渡すだけで、**本機能はパスを読まない**。
//!
//! ネイティブファイル選択の実体はタスク 7.7 が決める。`tauri-plugin-dialog` は 2.0.0〜2.7.3 の
//! すべての版が `tauri-plugin-fs` を非オプションの通常依存に持つため、タスク 1.3 では宣言して
//! いない（`src-tauri/Cargo.toml` の依存方針 3 を参照）。7.7 は (a) `tauri-plugin-dialog` を
//! 宣言して推移依存の `tauri-plugin-fs` を受け入れるか、(b) プラグインが内部で使う `rfd` を
//! 直接宣言して `AsyncFileDialog::set_parent` で親を指定するかを選ぶ。

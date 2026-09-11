//! jxcel アプリケーションシェルの中核（Tauri 非依存のドメインクレート）。
//!
//! 本クレートは、Tauri のランタイムから独立に検証したい機構を所有する
//! （design.md「Architecture Pattern & Boundary Map」の Core 層）:
//! 補助プロセスの監督・同梱物の整合性検査・設定の原子的永続化・
//! ショートカットの競合検査・診断方針。
//!
//! 依存の向きは `app_shell`（コア） → `jxcel`（Tauri アダプタ） → `src/`（フロントエンド）
//! の一方向であり、逆流を許容しない。Tauri の型とランタイムを知るのは `src-tauri/` の
//! アダプタだけである（structure.md「エンジンと UI の分離」）。
//!
//! 本クレートの各モジュールの骨組みはタスク 1.2 が置いた。通信境界（[`ipc`]）・設定
//! （[`settings`]）・ショートカット検査（[`accelerator`]）・診断方針（[`diagnostics`]）の
//! 実体は tasks.md の 2.x / 4.x が、補助プロセスの監督（[`sidecar`]）の実体は 3.x が埋める。
//! 複数のモジュールが参照する共有の列挙（補助プロセスの種類）は [`sidecar::SidecarKind`] に
//! 定義してある。
//!
//! [`render`] は初回描画の監視と三値の判定を所有する（tasks.md 8.2、要件 10.1、10.2）。
//! **GUI を起動せずに検証できるよう、時計と記録先を注入できる形にしてある** — 判定と記録の
//! 経路は実画面なしの `cargo test` で固定される。

pub mod accelerator;
pub mod diagnostics;
pub mod ipc;
pub mod render;
pub mod settings;
pub mod sidecar;

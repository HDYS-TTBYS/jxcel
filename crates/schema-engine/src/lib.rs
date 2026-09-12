//! jxcel スキーマエンジン（Tauri 非依存のドメインクレート）。
//!
//! シートのスキーマ宣言に意味を与える唯一の場所である（design.md「Overview」）。列が
//! どの型を持ち、どの値が適合し、適合しない値をどう扱い、スキーマを変えたとき既存データが
//! どうなるかを決める。画面・永続化・履歴・計算は所有しない（requirements.md「Boundary
//! Context」の Out of scope）。
//!
//! # 依存の向き（design.md「Architecture Integration」「Allowed Dependencies」）
//!
//! `document-format → schema-engine → (data-grid / schema-editor / macro-runtime /
//! export-templates / form-builder)` の一方向である。本クレートは上流に
//! `document-format` だけを持ち、`tauri` には推移的にも依存しない（CI の
//! `bash scripts/check-core-deps.sh schema-engine` が `cargo tree` 全体を走査して
//! 機械検査する）。`custom-types` は本クレートに依存する側であり、本クレートは
//! `custom-types` を知らない（structure.md「拡張点は所有者と実装者を分ける」）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write
//! → evolution → api`。各層の `mod.rs` の冒頭にこの鎖を書き、左の層だけを参照する
//! （`document-format` の `Ids / Value / EntryName → Model → Json → Parts → Container
//! → Api` と同じ規約）。
//!
//! # 現状
//!
//! 本ファイルはタスク 1.1 が置いた足場である。公開面（`SchemaEngineApi`。design.md
//!「コンポーネントとファイルの対応」）と各層のモジュールは、tasks.md の 1.2 以降が
//! この順に足す。タスク 1.2 が層の鎖の最左 [`error`] を置き、タスク 1.3 が検証層
//! [`validate`] の 4 つのサブモジュールと違反の表現を置いた。

pub mod error;
pub mod validate;

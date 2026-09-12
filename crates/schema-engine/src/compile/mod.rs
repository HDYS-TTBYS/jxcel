//! コンパイル層（design.md「コンポーネントとファイルの対応」の `SchemaCompiler`。
//! tasks.md 群 4 が実装する）。
//!
//! 宣言を一度だけ解決し、以後の実行から宣言の走査を排除する（design.md
//! 「Compile Layer / SchemaCompiler」）。本層が生む計画（列添字で引ける検証器の配列）が、
//! 10 万行の走査の内側で引かれる唯一のものである。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本層は `registry` までを参照でき、`coerce` / `validate` 以降へは
//! 依存しない。本層を参照するのは `{ coerce, validate }` 以降である。
//!
//! # 現状（tasks.md 群 4）
//!
//! タスク 4.2 が [`plan`]（列 1 本分の検証器。`ColumnValidator`）を置いた。型定義参照の
//! 解決と循環の検出（`resolve.rs`）はタスク 4.3 が、`SchemaCompiler` 本体
//! （`CompiledSchema` と列の並び順の供給。要件 1.1, 1.2, 1.8, 11.7）はタスク 4.4 が
//! 本モジュールに足す。

pub mod plan;

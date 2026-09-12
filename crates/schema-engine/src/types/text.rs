//! 文字列の長さと書式の検査（design.md「コンポーネントとファイルの対応」の
//! `TextConstraints`。tasks.md 2.4 が実装する。要件 4.5）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは [`super`]（`TypeCatalog`）の下に置かれ、文字列の
//! 最小長・最大長と書式（パターン）の検査だけを所有する。
//!
//! # 実行時間をパターンに依存させない（design.md「Compile Layer / SchemaCompiler」）
//!
//! 書式は `regex` 1.13 を使う。**有限オートマトンで線形時間**であり、後方参照と先読みは
//! 非対応で、使うとコンパイル時に落ちる（ReDoS が原理的に起きない）。利用者が書いた
//! パターンには生文字列長・コンパイル後の大きさ・入れ子の深さの上限を課し、超えるものは
//! 宣言の誤り（[`SchemaError::PatternLimitExceeded`](crate::error::SchemaError)）として
//! 拒否する。`regex` は**コンパイル時に一度だけ**構築し、走査中に構築しない。
//!
//! # 現状
//!
//! 本ファイルは tasks.md 2.1（境界 `TypeCatalog`）が親モジュールの宣言のために置いた骨格で
//! ある。実装はタスク 2.4 が入れる。

//! 1 セルの判定と入れ子の再帰（design.md「コンポーネントとファイルの対応」の
//! `CellValidator`。tasks.md 5.1 が実装する）。
//!
//! 値それ自体に閉じた性質（型・範囲・長さ・書式・桁・必須・入れ子）だけを判定する。
//! 行を跨ぐ性質（一意性と参照の実在）は本モジュールでは判定しない（design.md
//!「Validate Layer / SheetValidator」の線引き）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは `compile` 層までと、同じ `validate` 層の
//! [`super::report`] を参照する。
//!
//! # 現状
//!
//! tasks.md 1.3 が親モジュールの宣言のために置いた空の骨格である。実装はタスク 5.1 が
//! 入れる。

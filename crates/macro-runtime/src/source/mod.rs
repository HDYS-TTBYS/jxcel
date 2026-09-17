//! マクロの記録の層（design.md「File Structure Plan」の `source/`。要件 1.1–1.7, 3.2, 3.3）。
//!
//! マクロを**名前・種別（TypeScript / JavaScript）・ソース**の値として扱い、ソース先頭の
//! **能力宣言**を解釈する層である。上流のパート（`document-format` の `macros.json`。形だけを
//! 持つ。design.md 決定 4）との往復もここが担う。
//!
//! # 層の鎖（design.md「File Structure Plan」）
//!
//! `error / source → surface → host → engine → types → api`。本層は鎖の 2 番目であり、
//! 上流（`document-format` の値と識別子の型）と `std` だけを参照する。**下流の層
//! （`surface` / `host` / `engine`）を参照しない。**
//!
//! # いまあるもの（tasks.md 1.3）
//!
//! [`record`] の型（`MacroRecord` / `MacroKind` / `MacroName`）だけである。実行の要求
//! （`engine::outcome::RunRequest`）がマクロの記録を運ぶため、その**形**を 1 箇所に置いた。
//! 名前の一意性と置き換えの規則・能力宣言の解析・上流のパートとの往復は**タスク 1.6** が
//! 同じモジュールへ足す。

pub mod record;

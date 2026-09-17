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
//! # いまあるもの（tasks.md 1.3 と 1.6）
//!
//! - [`record`]: マクロの記録（`MacroRecord` / `MacroKind` / `MacroName`）と、**名前で一意な
//!   並び**の操作（`find` / `upsert` / `remove`。要件 1.6, 1.7）
//! - [`capability`]: ソース先頭の**能力宣言**の解析（`Capability` / `CapabilitySet` /
//!   `parse`。要件 8.1–8.5。design 決定 6）
//!
//! 上流のパート（`document-format` の `macros.json`）との往復は、クレートの外の
//! 結合テスト（`tests/macro_part_roundtrip.rs`）が実ファイルで固定している。

pub mod capability;
pub mod record;

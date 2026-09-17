//! ホスト API の層（design.md「File Structure Plan」の `surface/`。要件 4.1, 4.6, 8.1,
//! 8.3, 8.4, 10.1）。
//!
//! マクロへ公開する API を **1 つの宣言表**に並べ、その表だけを源にして **op の登録・
//! 能力の門・型定義の生成**が行われる（design.md 決定 5。用途ごとに一覧を持つと必ず
//! ずれる）。所有者はこの層だけであり、`host/` は実装（`HostPort`）、`engine/` は登録、
//! `types.rs` は型定義を、それぞれ表から受け取る。
//!
//! # 層の鎖（design.md「File Structure Plan」）
//!
//! `error / source → surface → host → engine → types → api`。本層は鎖の 3 番目であり、
//! **`source`（能力の集合。タスク 1.6）と `std` だけを参照する**。下流の層
//! （`host` / `engine` / `types` / `api`）を参照しない — 表は「何を公開するか」を持ち、
//! 「どう実装するか」を知らない（実装はアダプタが差し込む。design.md 決定 2）。
//!
//! # いまあるもの（tasks.md 2.1）
//!
//! - [`declaration`]: 宣言表（名前・引数・戻り値の型・**必要とする能力**）と、宣言と実装
//!   （登録された op）の一致の検査
//! - [`gate`]: 能力の門（マクロの宣言に無い能力を要する API を、**呼び出しの前に**拒む）
//!
//! op の実体（`HostPort` の結線）はタスク 3.2 / 4.1、型定義の生成はタスク 3.3 が担う。

pub mod declaration;
pub mod gate;

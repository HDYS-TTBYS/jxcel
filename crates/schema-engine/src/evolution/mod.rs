//! スキーマ変更の層（design.md「コンポーネントとファイルの対応」の `SchemaEvolution`。
//! tasks.md 群 7。要件 8.1〜8.8）。
//!
//! 変更の代償を**適用前に**見せ、適用は必ず提示どおりにする（design.md「Evolution Layer /
//! SchemaEvolution」の Intent）。本層は `document-format` の `migration`（形式バージョンの
//! 移行）とは**別物**である。名前の衝突を避けるため `evolution` と呼ぶ
//! （design.md「New components rationale」・research.md）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本層は `write` 層までを参照でき、`api`（公開面）だけが本層を参照する。
//!
//! # 責務の分割（tasks.md 群 7。design.md「File Structure Plan」）
//!
//! | モジュール | 責務 | 要件 | tasks.md |
//! |------------|------|------|----------|
//! | [`diff`] | 旧宣言と新宣言から変更を抽出する | 8.1, 8.7 | 7.1 |
//! | [`impact`] | 適用した場合の影響を集計する。ドキュメントを変更しない | 8.2, 8.3, 8.4 | 7.2 |
//!
//! 計画の適用（`apply`）はタスク 7.3 が同じ層に足す。本ファイルは層の入口（design.md
//! 「`evolution/mod.rs` # スキーマ変更の入口（計画 → 適用）」）であり、適用が揃った時点で
//! `apply_change` を再輸出する。
//!
//! # 改名は削除と追加の組ではない（design.md「Evolution Layer / SchemaEvolution」。要件 8.1, 8.7）
//!
//! 扱う変更は列の追加・削除・改名・型の変更・制約の変更である（要件 8.1）。**改名は
//! 削除と追加の組として扱わない** — 改名として抽出できた列は、既存の値を新しい名前の
//! もとに運ぶ対象になる（要件 8.7）。抽出の規則そのものは [`diff`] が定める。
//!
//! # 原子性の作り方（design.md「Evolution Layer / SchemaEvolution」。要件 8.6）
//!
//! 可謬な処理を**すべて計画側に寄せる**。計画が新しい列名の配列と全行分の新しい値を
//! 計算しきり、適用は計算済みの値を書き戻すだけにする。この分担はタスク 7.2 / 7.3 が実装する。

pub mod diff;
pub mod impact;

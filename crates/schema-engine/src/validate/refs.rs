//! シート間参照の一括実在判定（design.md「コンポーネントとファイルの対応」の
//! `ReferenceScan`。tasks.md 5.3 が実装する）。
//!
//! 参照先シートの行識別子の集合を 1 回だけ作り、行ごとの個別の問い合わせを行わない
//! （要件 9.6）。参照先の行が実在しないときは参照元の行と列および参照先の識別子を含む
//! 違反として報告し（要件 9.3）、参照されている行の削除には介入しない（要件 9.4）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは `compile` 層まで、`document-format` のモデル、同じ
//! `validate` 層の [`super::report`] を参照する。
//!
//! # 現状
//!
//! tasks.md 1.3 が親モジュールの宣言のために置いた空の骨格である。実装はタスク 5.3 が
//! 入れる。

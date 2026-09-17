//! 実行の層（design.md「File Structure Plan」の `engine/`。要件 2.1–2.4, 3.1–3.5, 6.1–6.4, 9.1–9.3）。
//!
//! 実行の実体を持つ層である。専用 OS スレッドが current-thread の tokio ランタイムを 1 つ
//! 回し、その上で**実行ごとに `JsRuntime` を作って所有スレッドの上で落とす**（design.md
//! 決定 1）。`resolve` を使わず、`#[op2]` を `async fn` に付ける（`tech.md`「Known Risks」1
//! の実測が確定形）。
//!
//! # 層の鎖（design.md「File Structure Plan」）
//!
//! `error / source → surface → host → engine → types → api`。本層は `source` / `surface` /
//! `host` までを参照でき、`types` / `api` へは依存しない。
//!
//! # いまあるもの（tasks.md 1.3）
//!
//! | モジュール | 責務 | 担当 |
//! |------------|------|------|
//! | [`outcome`] | 実行の要求・結果（3 値）・失敗（理由・種別・フレーム）・上限 | 1.3 |
//! | `actor` | 専用スレッド・直列化・実行ごとの isolate | 1.4 |
//! | `isolate` | `JsRuntime` の生成・op の登録・console・ソースマップ | 1.4 / 3.2 |
//! | `transpile` | TypeScript → JavaScript とソースマップ | 3.1 |
//! | `limits` | 上限の適用・打ち切り・種類の記録・復帰 | 1.5 |

pub mod outcome;

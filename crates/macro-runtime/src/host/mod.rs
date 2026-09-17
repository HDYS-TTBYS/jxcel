//! ホストの縫い目の層（design.md「File Structure Plan」の `host/`。要件 4.1–4.6, 5.1–5.4）。
//!
//! マクロがドキュメントへ触る経路の**エンジン側の半分**を持つ層である。エンジンは文書を
//! 知らない — 読みは縫い目（`HostPort`）越しに行い、書きは未適用の変更集合として持ち、
//! 文書へ適用するのはアダプタ（`src-tauri`）である（design.md 決定 2 / 3）。この層が持つのは
//! **値の写像**（[`value`]）・**未適用の変更集合**（[`changes`]）・**読みの重ね合わせ**
//! （`overlay`）と、その縫い目 `HostPort` である。
//!
//! # 層の鎖（design.md「File Structure Plan」）
//!
//! `error / source → surface → host → engine → types → api`。本層は鎖の 4 番目であり、
//! `source` / `surface` と上流の 2 クレート（`document-format` / `schema-engine`）を参照する。
//! `engine` は本層を参照する側であり、本層は `engine` を知らない（値の写像が V8 に依存しない
//! ことは、実装ではなく依存の向きで保たれている）。
//!
//! # いまあるもの（tasks.md 2.2 / 2.3）
//!
//! | モジュール | 責務 | 担当 |
//! |---|---|---|
//! | [`value`] | schema / document の値 ⇄ JavaScript の値の写像と対応表 | 2.2 |
//! | [`changes`] | 未適用の変更集合（セルの書き込み・行の追加・削除・複製）と件数・適用の順序 | 2.3（本タスク） |
//! | `overlay`（未着手） | 自分の書き込みを読む重ね合わせと範囲の読み | 2.4 |
//! | `HostPort`（未着手） | ホストの実装の縫い目（アダプタが実装する） | 3.2 / 4.1 |

pub mod changes;
pub mod overlay;
pub mod value;

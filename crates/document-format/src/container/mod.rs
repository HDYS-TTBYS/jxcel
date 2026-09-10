//! コンテナ層（design「Container Layer」。要件 5.6 ほか）。
//!
//! 本層は「バイト列のファイル」と「論理エントリ集合」の間を担う。依存方向
//! `Ids / Value / EntryName → Model → Json → Parts → Container → Api` で言えば
//! [`crate::parts`] の右隣であり、左方向（[`crate::entry_name`] / [`crate::error`] /
//! [`crate::parts`]）にのみ依存する。
//!
//! 現在あるのは [`atomic_save`] だけである（タスク 5.1）。これは保存の最終段にあたる
//! **ファイルの原子的置換**であり、コンテナの内容には触れない（`&[u8]` を受けて置き換える
//! だけ）。ZIP の符号化（タスク 5.2）と復号・許可リストの適用（5.3）は未実装であり、
//! 本層はまだ `zip` crate に依存しない。
//!
//! | モジュール | 責務 |
//! |------------|------|
//! | [`atomic_save`] | バイト列を対象パスへ原子的に置き換える（要件 5.6） |

pub mod atomic_save;

pub use atomic_save::AtomicWriter;

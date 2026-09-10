//! コンテナ層（design「Container Layer」。要件 5.6 ほか）。
//!
//! 本層は「バイト列のファイル」と「論理エントリ集合」の間を担う。依存方向
//! `Ids / Value / EntryName → Model → Json → Parts → Container → Api` で言えば
//! [`crate::parts`] の右隣であり、左方向（[`crate::entry_name`] / [`crate::error`] /
//! [`crate::parts`]）にのみ依存する。
//!
//! 現在あるのは**決定的な ZIP の書き出し**（[`writer`]。タスク 5.2）と、保存の最終段に
//! あたる**ファイルの原子的置換**（[`atomic_save`]。タスク 5.1）である。ZIP の読み込みと
//! 許可リストの適用（タスク 5.3）は未実装であり、復号は [`ContainerCodec`] へ
//! associated function として足す（型を分けない）。
//!
//! **ZIP に関する知識は本層に閉じる**（design「ContainerCodec / Implementation Notes」）。
//! `zip` crate を参照してよいのは `container/` 配下だけで、他の層は [`crate::parts`] が
//! 定める「展開後のバイト列」だけを扱う。一方 [`atomic_save`] はコンテナの内容には
//! 触れない（`&[u8]` を受けて置き換えるだけ）ので、ZIP も圧縮も参照しない。
//!
//! | モジュール | 責務 |
//! |------------|------|
//! | [`writer`] | パート集合を決定的な ZIP のバイト列へ符号化する（要件 2.1, 3.1, 3.2, 3.6） |
//! | [`atomic_save`] | バイト列を対象パスへ原子的に置き換える（要件 5.6） |

pub mod atomic_save;
pub mod writer;

pub use atomic_save::AtomicWriter;
pub use writer::ContainerCodec;

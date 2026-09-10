//! コンテナ層（design「Container Layer」。要件 5.6 ほか）。
//!
//! 本層は「バイト列のファイル」と「論理エントリ集合」の間を担う。依存方向
//! `Ids / Value / EntryName → Model → Json → Parts → Container → Api` で言えば
//! [`crate::parts`] の右隣であり、左方向（[`crate::entry_name`] / [`crate::error`] /
//! [`crate::parts`]）にのみ依存する。
//!
//! 本層が担うのは **決定的な ZIP の書き出し**（[`writer`]。タスク 5.2）、**ZIP の読み込みと
//! 不正コンテナの拒否**（[`reader`] と [`layout`]。タスク 5.3）、そして保存の最終段にあたる
//! **ファイルの原子的置換**（[`atomic_save`]。タスク 5.1）である。符号化と復号は同じ型
//! [`ContainerCodec`] の associated function であり、型を分けない
//! （[`ContainerCodec::encode`] / [`ContainerCodec::decode`]）。
//!
//! **ZIP に関する知識は本層に閉じる**（design「ContainerCodec / Implementation Notes」）。
//! `zip` crate を参照してよいのは `container/` 配下だけであり、とくに [`layout`] は
//! `zip` を一切参照しない純粋な判定である（許可リストと重複検出は名前の列だけで決まる。
//! 単体テストが `zip` 無しで書けることがこの分割の狙いである）。他の層は [`crate::parts`]
//! が定める「展開後のバイト列」だけを扱う。一方 [`atomic_save`] はコンテナの内容には
//! 触れない（`&[u8]` を受けて置き換えるだけ）ので、ZIP も圧縮も参照しない。
//!
//! | モジュール | 責務 |
//! |------------|------|
//! | [`writer`] | パート集合を決定的な ZIP のバイト列へ符号化する（要件 2.1, 3.1, 3.2, 3.6） |
//! | [`reader`] | ZIP を検証しながら読み、許可リスト照合・重複検出・サイズ照合を経てパート集合へ復号する（要件 2.5, 2.6）。型マーカー `jxcel` を集合から外す |
//! | [`layout`] | 復号時の許可リスト適用と同一パスの重複検出。`zip` に依存しない純粋な判定（要件 2.5, 2.6） |
//! | [`atomic_save`] | バイト列を対象パスへ原子的に置き換える（要件 5.6） |

pub mod atomic_save;
pub mod layout;
pub mod reader;
pub mod writer;

pub use atomic_save::AtomicWriter;
pub use writer::ContainerCodec;

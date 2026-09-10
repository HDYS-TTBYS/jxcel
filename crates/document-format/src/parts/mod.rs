//! パート層（design「Parts Layer」。要件 2.2, 2.3）。
//!
//! ドキュメントはコンテナの中で**論理エントリ（パート）**の集合として現れる。
//! 本層はそのパート集合と、各パートの符号化・復号を担う。ZIP の知識は一切持たず、
//! 圧縮にも触れない（コンテナ層 = タスク 5.x の責務）。
//!
//! 現在あるのは **manifest パート**（`manifest.json`）と **document パート**
//! （`document.json`）、**シート別スキーマパート**（`schemas/<sheet-ulid>.json`）、
//! **シート別行データパート**（`sheets/<sheet-ulid>.jsonl`）、復号済みパート群に
//! 対する**構造検証**（[`validate`]）、そして**論理エントリ集合そのもの**
//! （[`document_parts`]。タスク 4.8）である:
//!
//! | モジュール | エントリ | 責務 |
//! |------------|----------|------|
//! | [`manifest`] | `manifest.json` | 形式バージョン、パート索引、パートごとのダイジェスト（唯一の権威ある索引） |
//! | [`document_part`] | `document.json` | ドキュメント識別子、シート順序、シートのメタデータ（列名を含む） |
//! | [`schema_codec`] | `schemas/<sheet-ulid>.json` | シートごとのスキーマ（ルートスキーマ + ネスト型定義）の符号化・復号 |
//! | [`rows_codec`] | `sheets/<sheet-ulid>.jsonl` | シートごとの行データ（1 行 1 オブジェクトの NDJSON）の符号化・復号 |
//! | [`validate`] | — | 復号済みパート群の目録に対する構造検証（識別子の一意性、スキーマの存在、参照の実在性） |
//! | [`document_parts`] | 上記の全形 + `attachments/<hex64>.bin` | 論理エントリ集合（[`DocumentParts`] / [`Part`]）と、モデル ⇄ 集合の双方向変換（[`to_parts`] / [`from_parts`]） |
//!
//! 論理エントリ集合は**エントリ名の昇順で決定的に反復**し（[`DocumentParts::iter`]）、
//! ZIP の知識も圧縮も持たない。`manifest.json` が他の全パートを索引し、全シートが
//! schema エントリと rows エントリを持つ（0 行のシートも空の行エントリを持つ）。
//!
//! # パートごとの順序規則
//!
//! 配列を持つパートの順序規則はパートごとに異なる。`manifest.json` の索引はエントリ名の
//! 昇順へ整列する（引き当ての都合で並びが決まる索引だから。[`manifest`]）が、
//! `document.json` のシート順序は**それ自体がデータ**であり（要件 1.1 のモデル順）、
//! 与えられた順序をそのまま保持して並べ替えない（[`document_part`]）。**逆の規則**で
//! あるため、どちらの規則かを迷わないよう両モジュールの docs に理由を書いてある。
//! `schemas/<sheet-ulid>.json` の型定義の並びも同じ側であり、エンベロープ内の出現順を
//! そのまま書く（[`schema_codec`]）。`sheets/<sheet-ulid>.jsonl` も同じ側であり、
//! **行順は与えられた順序のまま**（シートの行順序そのものがデータ。ULID 昇順にも辞書順にも
//! ソートしない）で、列順は呼び出し元が与えた順序のままである（[`rows_codec`]）。
//!
//! # 依存方向
//!
//! `Ids / Value / EntryName → Model → Json → Parts → Container → Api` の一方向
//! （design「Architecture Integration」）。本層は [`crate::json`] /
//! [`crate::integrity`] / [`crate::entry_name`] / [`crate::error`] に依存し、
//! パートの対象がモデルの型である場合は [`crate::model`] にも依存する
//! （[`schema_codec`] は [`crate::model::SchemaPart`] を符号化する。この向きは上の
//! 鎖の `Model → … → Parts` と同じである）。`container` には依存しない。とくに
//! **ZIP を知らない**ことは本層の契約であり、パートのバイト列は圧縮前（展開後）の
//! 内容として扱う（design「IntegrityVerifier」: ダイジェストは各エントリの内容から
//! 算出する）。

pub mod document_part;
pub mod document_parts;
pub mod manifest;
pub mod rows_codec;
pub mod schema_codec;
pub mod validate;

pub use document_part::{DocumentPart, SheetMeta};
pub use document_parts::{from_parts, to_parts, validate_document, DocumentParts, Part};
pub use manifest::{resolve_manifest, ManifestEntry, ManifestPart};
pub use rows_codec::{RowsCodec, RowsEncodeError, SheetRows};
pub use schema_codec::SchemaCodec;
pub use validate::{
    AttachmentRefDeclaration, IdDeclaration, PartInventory, SheetRefDeclaration,
    StructuralValidator, TypeRefDeclaration,
};

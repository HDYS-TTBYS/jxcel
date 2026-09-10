//! パート層（design「Parts Layer」。要件 2.2, 2.3）。
//!
//! ドキュメントはコンテナの中で**論理エントリ（パート）**の集合として現れる。
//! 本層はそのパート集合と、各パートの符号化・復号を担う。ZIP の知識は一切持たず、
//! 圧縮にも触れない（コンテナ層 = タスク 5.x の責務）。
//!
//! 現在あるのは **manifest パート**（`manifest.json`）と **document パート**
//! （`document.json`）である:
//!
//! | モジュール | エントリ | 責務 |
//! |------------|----------|------|
//! | [`manifest`] | `manifest.json` | 形式バージョン、パート索引、パートごとのダイジェスト（唯一の権威ある索引） |
//! | [`document_part`] | `document.json` | ドキュメント識別子、シート順序、シートのメタデータ（安定メタデータ） |
//!
//! 論理エントリ集合そのものの型 `DocumentParts`（エントリ名 → バイト列 + ダイジェストの
//! 決定的な集合）と、他のパート（`schemas/*.json` / `sheets/*.jsonl` /
//! `attachments/*.bin`）の符号化は後続タスク（4.4〜4.8）で本層に加わる。
//!
//! # パートごとの順序規則
//!
//! 配列を持つパートの順序規則はパートごとに異なる。`manifest.json` の索引はエントリ名の
//! 昇順へ整列する（引き当ての都合で並びが決まる索引だから。[`manifest`]）が、
//! `document.json` のシート順序は**それ自体がデータ**であり（要件 1.1 のモデル順）、
//! 与えられた順序をそのまま保持して並べ替えない（[`document_part`]）。**逆の規則**で
//! あるため、どちらの規則かを迷わないよう両モジュールの docs に理由を書いてある。
//!
//! # 依存方向
//!
//! `Ids / Value / EntryName → Model → Json → Parts → Container → Api` の一方向
//! （design「Architecture Integration」）。本層は [`crate::json`] /
//! [`crate::integrity`] / [`crate::entry_name`] / [`crate::error`] に依存し、
//! `container` / `model` に依存しない。とくに **ZIP を知らない**ことは本層の契約であり、
//! パートのバイト列は圧縮前（展開後）の内容として扱う（design「IntegrityVerifier」:
//! ダイジェストは各エントリの内容から算出する）。

pub mod document_part;
pub mod manifest;

pub use document_part::{DocumentPart, SheetMeta};
pub use manifest::{resolve_manifest, ManifestEntry, ManifestPart};

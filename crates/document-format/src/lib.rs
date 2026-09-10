//! jxcel ドキュメント形式の実装。
//! ZIP コンテナに格納された JSON テキストとメモリ上のドキュメントモデルを
//! 双方向に変換する。同一内容は常に同一バイト列になる（決定的出力）。

pub mod container;
pub mod entry_name;
pub mod error;
pub mod ids;
pub mod integrity;
pub mod json;
pub mod migration;
pub mod model;
pub mod parts;
pub mod value;

pub use entry_name::EntryName;
pub use error::{DocumentError, IdKind};
pub use ids::{
    AttachmentId, Blake3Digest, DocumentId, IdFactory, IdParseError, RowId, SheetId, TypeDefId,
};
pub use migration::{CURRENT_FORMAT_VERSION, FormatVersion, MigrationChain, VersionVerdict};
pub use model::{
    Attachment, AttachmentRegistry, Document, RawJson, ReorderError, Row, SchemaPart, Sheet,
    TypeDef, UnknownRow, UnknownSheet,
};
pub use value::{CellValue, NestedValue, from_json_bytes, to_json_bytes};


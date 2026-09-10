//! jxcel ドキュメント形式の実装。
//! ZIP コンテナに格納された JSON テキストとメモリ上のドキュメントモデルを
//! 双方向に変換する。同一内容は常に同一バイト列になる（決定的出力）。

pub mod entry_name;
pub mod error;
pub mod ids;
pub mod model;
pub mod value;

pub use entry_name::EntryName;
pub use error::{DocumentError, FormatVersion, IdKind};
pub use ids::{
    AttachmentId, Blake3Digest, IdFactory, IdParseError, RowId, SheetId, TypeDefId,
};
pub use model::{Document, ReorderError, Row, Sheet, UnknownSheet};
pub use value::{CellValue, NestedValue, from_json_bytes, to_json_bytes};

#[cfg(test)]
mod scaffold_tests {
    /// スキャフォールドの証明: deflate 経路（flate2 / miniz_oxide バックエンド）が
    /// 結線され、同一内容の 2 回の書き出しがバイト単位で一致する。
    /// 本格的な決定性保証（全パラメータ固定・OS 間一致）はタスク 5.2 / 8.2 で保証する。
    #[test]
    fn zip_deflate_roundtrip_is_byte_deterministic() {
        use std::io::{Read, Write};

        fn write_zip() -> Vec<u8> {
            let options = zip::write::FileOptions::DEFAULT
                .compression_method(zip::CompressionMethod::Deflated);
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            w.start_file("probe.txt", options).unwrap();
            w.write_all(b"jxcel").unwrap();
            w.finish().unwrap().into_inner()
        }

        let bytes = write_zip();
        assert_eq!(bytes, write_zip(), "同一内容の 2 回書き出しがバイト一致しない");

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(1, archive.len());
        let mut text = String::new();
        archive.by_name("probe.txt").unwrap().read_to_string(&mut text).unwrap();
        assert_eq!("jxcel", text);
    }
}

//! 決定的な ZIP 書き出し（タスク 5.2。要件 2.1, 3.1, 3.2, 3.6。design「Container Layer /
//! ContainerCodec」）。
//!
//! 本モジュールの責務は、論理エントリ集合（[`DocumentParts`]）を**標準的な ZIP
//! アーカイブのバイト列**へ変換することだけである（要件 2.1）。逆方向（ZIP の読み込みと
//! 許可リストの適用）は同じ型 [`ContainerCodec`] の [`ContainerCodec::decode`] が担う
//! （実装は [`crate::container::reader`]）。
//!
//! # 確定形
//!
//! `ContainerCodec::encode(parts: &DocumentParts) -> Result<Vec<u8>, DocumentError>`。
//! 出力は次の 2 つを満たす:
//!
//! - **標準的な ZIP として読める**（要件 2.1）。全エントリが `zip::ZipArchive` で開け、
//!   エントリ名がパート名と一致し、展開後のバイト列がパートのバイト列とバイト単位で
//!   一致する。
//! - **参照透明**（要件 3.1, 3.2）。同一のパート集合に対して常に同一のバイト列を返し、
//!   保存時刻・実行環境・内部処理順序に依存する値を含まない（要件 3.6）。
//!
//! # 決定性のために固定する ZIP パラメータ（design Responsibilities）
//!
//! crate（`zip` 8.6）の既定値に依存するパラメータを 1 つも残さない。すべての値を定数と
//! して明示し、[`FileOptions::DEFAULT`] から組み立てる（`FileOptions::default()` は
//! **使わない**。前者は `const` であり、`time` feature の有無で「更新日時 = 現在時刻」へ
//! 分岐する余地を持ち込まない）。
//!
//! | パラメータ | 本実装の固定値 | 固定しない場合の危険 |
//! |------------|----------------|----------------------|
//! | 更新日時 | `FIXED_MODIFIED_TIME`（1980-01-01 00:00:00 = ZIP のエポック） | 保存時刻が混ざる（要件 3.6。`zip` の `time` feature が有効だと既定は現在時刻） |
//! | unix permissions | `FIXED_UNIX_PERMISSIONS`（0644） | crate の既定値に依存する（`zip` は `None` のとき 0644 を補う） |
//! | ホスト OS バイト | `FIXED_HOST_SYSTEM`（`Unix` = 3） | `zip` は未指定時に `cfg!(windows)` で DOS / Unix を切り替える（要件 3.2） |
//! | エントリ書き込み順 | 型マーカーが先頭、続いて [`DocumentParts::iter`] の昇順 | 入力順・列挙順が漏れる（要件 2.2, 3.6） |
//! | データディスクリプタ | 使わない（`Cursor<Vec<u8>>` は seek 可能で、サイズも既知） | ローカルヘッダのサイズが 0 になり、読み手が書き出し順に依存する |
//! | 圧縮方式 | 型マーカーが `Stored`、他が `Deflate`（レベル 6） | crate の既定（deflate バックエンド・`flate2` の既定レベル）に依存する |
//!
//! ## 型マーカー
//!
//! 先頭エントリ `jxcel` は**無圧縮**（`Stored`）で置く（design「Container Entry
//! Layout」）。固定オフセット（ローカルヘッダの直後）で型を判定でき、伸長器を持たない
//! 相手でも読める。内容は `jxcel\n<major>.<minor>\n`（[`marker_bytes`]）であり、保存時刻
//! のような揮発値を含まない。
//!
//! **バージョンは `manifest.json` が記録している値そのもの**（[`DocumentParts::format_version`]）
//! を使い、本モジュールは第二のリテラルを持たない。design は `manifest.json` を
//! 「唯一の権威ある索引」と定めており（マーカーは固定オフセットでの**早期判定のための
//! 写し**にすぎない**）、正式なバージョン判定は索引を読んで行う
//! （[`crate::migration::MigrationChain`]。本モジュールは版を判定しない）。
//! 写しと索引が食い違っても**権威は常に索引**である。両者が一致することは単体テスト
//! `the_type_marker_carries_the_manifest_format_version` が固定し、形式バージョンを
//! 上げたときに片方だけ更新される事故を落とす。
//!
//! `jxcel` は [`DocumentParts`] の一部ではない（design の裁定。task 4.8 はマーカーを
//! 含む集合を拒否する）。したがって**コンテナ層が自分で先頭に置き**、`manifest.json` の
//! 索引には加えない（索引は [`DocumentParts`] が持つ内容のままであり、本モジュールは
//! 索引を書き換えない）。
//!
//! # 決定性の根拠
//!
//! - パートの並びは [`DocumentParts::iter`] が定める（本モジュールは並べ替えを
//!   再実装しない。構築時に正準化済みである）。
//! - `zip::ZipWriter` はエントリを挿入順に保持する（`IndexMap`）ため、書いた順序が
//!   そのままローカルヘッダと中央ディレクトリの順序になる。
//! - 圧縮は `flate2`（バックエンドは `miniz_oxide` 固定。`Cargo.toml` の依存方針 3）を
//!   単一スレッドで使う。バイト列は入力だけで決まる。
//! - 書き出し先は `Cursor<Vec<u8>>`（オンメモリ）であり、ファイルシステムの状態を
//!   一切観測しない。
//!
//! **決定性はゴールデンファイルのバイト比較テストで守る**（design Implementation
//! Notes）。`flate2` / `miniz_oxide` のバージョン更新は決定性を壊す変更として扱い、
//! テストが落ちた場合は形式のマイナーバージョンを上げるか固定を継続するかを判断する
//! （`tests/container_writer.rs`）。
//!
//! # 失敗経路
//!
//! 書き出しの失敗はすべて [`DocumentError::Io`] で報告し、`retried` は常に `false` で
//! ある（`true` は保存経路の rename 再試行枯渇だけを指す。design エラー表）。部分的に
//! 書かれたバイト列は返さない（`Vec<u8>` は `finish` の後でだけ取り出す）。

use std::io::{Cursor, Write};

use zip::result::ZipError;
use zip::write::FileOptions;
use zip::{CompressionMethod, DateTime, System, ZipWriter};

use crate::entry_name::EntryName;
use crate::error::DocumentError;
use crate::migration::FormatVersion;
use crate::parts::DocumentParts;

/// 型マーカーの骨格（先頭側）: 形式名 `jxcel` と LF。
const MARKER_PREFIX: &[u8] = b"jxcel\n";

/// 型マーカーの骨格（末尾側）: LF。
const MARKER_SUFFIX: &[u8] = b"\n";

/// 型マーカーエントリ `jxcel` の内容を組み立てる（**確定形**）。
///
/// `jxcel\n<major>.<minor>\n` の ASCII バイト列である。バージョンは
/// **`manifest.json`（唯一の権威ある索引）が記録している値そのもの**
/// （[`DocumentParts::format_version`]）を使い、本モジュールは**第二のリテラルを
/// 持たない**（[`FormatVersion`] の `Display` = `major.minor` が正準テキスト形）。
/// 保存時刻・実行環境・乱数に由来する値は含まない（要件 3.6）。
pub fn marker_bytes(version: FormatVersion) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(MARKER_PREFIX.len() + MARKER_SUFFIX.len() + 8);
    bytes.extend_from_slice(MARKER_PREFIX);
    write!(bytes, "{version}").expect("Vec<u8> への書き込みは失敗しない");
    bytes.extend_from_slice(MARKER_SUFFIX);
    bytes
}

/// 固定する更新日時: 1980-01-01 00:00:00（ZIP のエポック。`DateTime::DEFAULT`）。
///
/// `zip` の既定値は `time` feature が有効なとき**現在時刻**になるため、既定に任せない。
const FIXED_MODIFIED_TIME: DateTime = DateTime::DEFAULT;

/// 固定する unix permissions: 通常ファイル 0644。
///
/// `zip` は `None` のままでも `DEFAULT_FILE_PERMISSION` を補うが、それは crate の既定値
/// であり本実装の固定値ではない。外部属性の上位 16 bit にこの値（と `S_IFREG`）が載る。
const FIXED_UNIX_PERMISSIONS: u32 = 0o644;

/// 固定するホスト OS バイト（`version made by` の上位バイト）: Unix (3)。
///
/// `zip` は未指定なら `cfg!(windows)` で DOS / Unix を選ぶ。要件 3.2（異なる OS 上でも
/// 同一バイト列）を守るため、ビルドプラットフォームを反映させず Unix に固定する。
const FIXED_HOST_SYSTEM: System = System::Unix;

/// 固定する圧縮レベル: 6（`flate2::Compression::default()` と同じ値）。
///
/// 既定値に任せるとバックエンドの既定が変わったときにバイト列が変わる。本実装は値を
/// 明示し、変更をゴールデンテストの差分として現す。
const FIXED_COMPRESSION_LEVEL: i64 = 6;

/// 型マーカーエントリ（`Stored`）の書き出しオプション（固定パラメータ）。
///
/// `Stored` は圧縮レベルを受け付けない（`zip` は `Some` を渡すと
/// `UnsupportedArchive` を返す）ため、レベルは指定しない。
const MARKER_OPTIONS: FileOptions<'static, ()> = FileOptions::DEFAULT
    .compression_method(CompressionMethod::Stored)
    .last_modified_time(FIXED_MODIFIED_TIME)
    .unix_permissions(FIXED_UNIX_PERMISSIONS)
    .system(FIXED_HOST_SYSTEM);

/// 型マーカー以外のエントリ（`Deflate`）の書き出しオプション（固定パラメータ）。
const PART_OPTIONS: FileOptions<'static, ()> = FileOptions::DEFAULT
    .compression_method(CompressionMethod::Deflated)
    .compression_level(Some(FIXED_COMPRESSION_LEVEL))
    .last_modified_time(FIXED_MODIFIED_TIME)
    .unix_permissions(FIXED_UNIX_PERMISSIONS)
    .system(FIXED_HOST_SYSTEM);

/// 論理エントリ集合と ZIP コンテナの相互変換（design「Container Layer / ContainerCodec」）。
///
/// 状態を持たない（対象は呼び出しごとに引数で渡す）ため、値ではなく名前空間としての型で
/// ある。型名 `ContainerCodec` は design が指定する名前である。
///
/// 本型が提供するのは**符号化**（[`ContainerCodec::encode`]）と**復号**
/// （[`ContainerCodec::decode`]）の 2 つである（design の Service Interface と同じ
/// associated function。型は分けない）。**許可リストの適用とマーカーの除去は復号側の
/// 責務**であり、符号化側と 1 対 1 に対応する（符号化はマーカーを先頭に書き、復号は
/// マーカーを集合から外す）。
pub struct ContainerCodec;

impl ContainerCodec {
    /// パート集合を決定的な ZIP のバイト列へ符号化する。確定形:
    /// `ContainerCodec::encode(parts: &DocumentParts) -> Result<Vec<u8>, DocumentError>`。
    ///
    /// 書き出し順は**型マーカー `jxcel`（`Stored`）が先頭**、続いて
    /// [`DocumentParts::iter`] の順（エントリ名の昇順）に各パートを `Deflate` で書く。
    /// マーカーの内容は [`marker_bytes`] が `manifest.json` の形式バージョンから組み立てる。
    /// パートのバイト列は解釈も再圧縮もせずそのまま運び、`manifest.json` の内容にも
    /// 手を触れない（索引の権威は [`DocumentParts`] 側にある）。
    ///
    /// 出力は `Cursor<Vec<u8>>` に対する書き出しであり、データディスクリプタを使わない
    /// （全件オンメモリでサイズは既知。design Responsibilities）。
    ///
    /// # 失敗
    ///
    /// `zip` の書き出しに失敗した場合は [`DocumentError::Io`]（`retried` は `false`）を
    /// 返す。部分的に書かれたバイト列は返さない。
    pub fn encode(parts: &DocumentParts) -> Result<Vec<u8>, DocumentError> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));

        let marker = marker_bytes(parts.format_version());
        write_entry(&mut writer, EntryName::Marker, &marker, MARKER_OPTIONS)?;
        for part in parts.iter() {
            write_entry(&mut writer, part.name, &part.bytes, PART_OPTIONS)?;
        }

        writer.finish().map(Cursor::into_inner).map_err(zip_error)
    }
}

/// 1 エントリを書く（名前 → 内容）。エントリ名は [`EntryName`] の表示テキストを使う
/// （コンテナ内の相対パスそのもの。許可リストの正準形）。
fn write_entry(
    writer: &mut ZipWriter<Cursor<Vec<u8>>>,
    name: EntryName,
    bytes: &[u8],
    options: FileOptions<'static, ()>,
) -> Result<(), DocumentError> {
    writer.start_file(name, options).map_err(zip_error)?;
    writer.write_all(bytes).map_err(io_error)
}

/// `zip` のエラーを [`DocumentError::Io`] へ写す（`retried` は常に `false`）。
///
/// `ZipError::Io` は元の I/O エラーをそのまま保つ（原因の連鎖を切らない）。
/// それ以外の変種は `zip` 自身が `From<ZipError> for std::io::Error` で I/O エラーへ
/// 写す経路を持っており、種別（`Unsupported` / `InvalidData` など）と説明文が保たれる。
fn zip_error(error: ZipError) -> DocumentError {
    let source = match error {
        ZipError::Io(source) => source,
        other => std::io::Error::from(other),
    };
    io_error(source)
}

/// I/O エラーを [`DocumentError::Io`] へ写す（`retried` は常に `false`。
/// 再試行の枯渇を表す `true` は保存経路 `AtomicWriter` だけが立てる）。
fn io_error(source: std::io::Error) -> DocumentError {
    DocumentError::Io { source, retried: false }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use zip::ZipArchive;

    use super::*;
    use crate::migration::FormatVersion;
    use crate::ids::{AttachmentId, SheetId};
    use crate::parts::{DocumentParts, ManifestEntry, ManifestPart};

    /// 標本のシート識別子（固定の正準形。発行器を使わない = 時刻に依存しない）。
    fn sample_sheet() -> SheetId {
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().expect("標本の識別子は正準形")
    }

    /// 標本のパート集合: 与えられた本体パートに、そこから算出した索引を足して組み立てる。
    fn sample_parts(entries: Vec<(EntryName, Vec<u8>)>) -> DocumentParts {
        let mut entries = entries;
        let index: Vec<ManifestEntry> = entries
            .iter()
            .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
            .collect();
        let manifest =
            ManifestPart::new(FormatVersion::new(1, 0), index).expect("標本の索引は妥当");
        entries.push((EntryName::Manifest, manifest.to_json_bytes().expect("符号化")));
        DocumentParts::from_entries(entries).expect("標本は妥当")
    }

    /// 大きい添付（Deflate の経路を自明にしない分量。内容は固定の反復で決まる）。
    fn large_attachment() -> Vec<u8> {
        const PATTERN: &[u8] = b"jxcel document-format: deterministic deflate. ";
        PATTERN.iter().copied().cycle().take(1 << 20).collect()
    }

    /// 決定性の標本（空に近い集合・単一パート・大きい添付）。横断的な性質は対象を
    /// 複数に分散させて示す。
    fn samples() -> Vec<(&'static str, DocumentParts)> {
        let attachment = large_attachment();
        vec![
            ("索引のみ", sample_parts(Vec::new())),
            (
                "単一パート",
                sample_parts(vec![(EntryName::Document, b"{}".to_vec())]),
            ),
            (
                "大きい添付",
                sample_parts(vec![(
                    EntryName::Attachment {
                        attachment: AttachmentId::from_bytes(&attachment),
                    },
                    attachment,
                )]),
            ),
            (
                "複数パート",
                sample_parts(vec![
                    (EntryName::Document, b"{\"probe\":1}".to_vec()),
                    (
                        EntryName::Schema { sheet: sample_sheet() },
                        b"{\"root\":null}".to_vec(),
                    ),
                    (EntryName::Rows { sheet: sample_sheet() }, Vec::new()),
                ]),
            ),
        ]
    }

    /// 同一のパート集合に対して常に同一のバイト列を返す（要件 3.1, 3.6）。
    #[test]
    fn encode_is_reproducible_for_every_sample() {
        for (label, parts) in samples() {
            let first = ContainerCodec::encode(&parts).expect("符号化");
            let second = ContainerCodec::encode(&parts).expect("符号化");
            assert_eq!(
                first, second,
                "{label}: 同一のパート集合に対する 2 回の符号化がバイト一致しない"
            );
        }
    }

    /// 標準的な ZIP として読め、型マーカーが `Stored` かつ先頭、他が `Deflate` である
    /// （要件 2.1）。エントリ名・展開後のバイト列はパートと一致する。
    #[test]
    fn encode_is_readable_as_a_standard_zip_archive() {
        for (label, parts) in samples() {
            let bytes = ContainerCodec::encode(&parts).expect("符号化");
            let mut archive =
                ZipArchive::new(Cursor::new(&bytes)).expect("標準的な ZIP として開けない");

            let part_count = parts.iter().count();
            assert_eq!(part_count + 1, archive.len(), "{label}: エントリ数が違う");

            {
                let mut marker = archive.by_index(0).expect("先頭エントリ");
                assert_eq!("jxcel", marker.name(), "{label}: 先頭が型マーカーでない");
                assert_eq!(
                    CompressionMethod::Stored,
                    marker.compression(),
                    "{label}: 型マーカーが無圧縮でない"
                );
                assert_eq!(
                    Some(FIXED_MODIFIED_TIME),
                    marker.last_modified(),
                    "{label}: 型マーカーの更新日時が固定値でない"
                );
                assert_eq!(
                    Some(0o100_644),
                    marker.unix_mode(),
                    "{label}: 型マーカーの permissions が固定値でない"
                );
                let mut contents = Vec::new();
                marker.read_to_end(&mut contents).expect("展開");
                assert_eq!(
                    marker_bytes(parts.format_version()),
                    contents,
                    "{label}: 型マーカーの内容が違う"
                );
            }

            for (index, part) in parts.iter().enumerate() {
                let mut entry = archive.by_index(index + 1).expect("エントリ");
                assert_eq!(
                    part.name.to_string(),
                    entry.name(),
                    "{label}: エントリ名がパート名と違う"
                );
                assert_eq!(
                    CompressionMethod::Deflated,
                    entry.compression(),
                    "{label}: パートが Deflate でない"
                );
                assert_eq!(
                    Some(FIXED_MODIFIED_TIME),
                    entry.last_modified(),
                    "{label}: パートの更新日時が固定値でない"
                );
                assert_eq!(
                    Some(0o100_644),
                    entry.unix_mode(),
                    "{label}: パートの permissions が固定値でない"
                );
                let mut contents = Vec::new();
                entry.read_to_end(&mut contents).expect("展開");
                assert_eq!(part.bytes, contents, "{label}: 展開後のバイト列がパートと違う");
            }
        }
    }

    /// 型マーカーが運ぶバージョンは、`manifest.json`（唯一の権威ある索引）が記録して
    /// いる形式バージョンと一致する。
    ///
    /// マーカーは固定オフセットでの早期判定のための**写し**であり、権威は索引である。
    /// 両者を突き合わせておくことで、形式バージョンを上げたときに片方だけ更新される
    /// 事故（マーカーは 1.1・索引は 1.0 という矛盾したファイル）がテストで落ちる。
    #[test]
    fn the_type_marker_carries_the_manifest_format_version() {
        let parts = sample_parts(vec![
            (EntryName::Document, b"{}".to_vec()),
            (EntryName::Rows { sheet: sample_sheet() }, Vec::new()),
        ]);
        let bytes = ContainerCodec::encode(&parts).expect("符号化");
        let mut archive = ZipArchive::new(Cursor::new(&bytes)).expect("標準的な ZIP として開ける");

        let mut marker = Vec::new();
        archive
            .by_name("jxcel")
            .expect("型マーカーがある")
            .read_to_end(&mut marker)
            .expect("展開");
        let mut manifest = Vec::new();
        archive
            .by_name("manifest.json")
            .expect("索引がある")
            .read_to_end(&mut manifest)
            .expect("展開");

        let recorded = ManifestPart::from_json_bytes(&manifest).expect("索引の復号").version();
        assert_eq!(
            parts.format_version(),
            recorded,
            "索引が記録したバージョンがパート集合の記録と違う"
        );

        let text = String::from_utf8(marker).expect("型マーカーは ASCII");
        let embedded = text
            .strip_prefix("jxcel\n")
            .and_then(|rest| rest.strip_suffix("\n"))
            .expect("型マーカーが `jxcel\\n<major>.<minor>\\n` 形でない");
        let (major, minor) = embedded.split_once('.').expect("バージョンが `major.minor` 形でない");
        let embedded = FormatVersion::new(
            major.parse().expect("major が数値でない"),
            minor.parse().expect("minor が数値でない"),
        );
        assert_eq!(
            recorded, embedded,
            "型マーカーが運ぶバージョンが索引の記録と一致しない（片方だけ更新されている）"
        );
    }

    /// 型マーカーは `manifest.json` の索引に載らず、コンテナ層が索引を書き換えない
    /// （索引の内容は [`DocumentParts`] が持っていたものそのもの）。
    #[test]
    fn encode_does_not_index_the_type_marker() {
        let parts = sample_parts(vec![
            (EntryName::Document, b"{\"probe\":1}".to_vec()),
            (EntryName::Rows { sheet: sample_sheet() }, Vec::new()),
        ]);
        let bytes = ContainerCodec::encode(&parts).expect("符号化");
        let mut archive = ZipArchive::new(Cursor::new(&bytes)).expect("標準的な ZIP として開ける");

        let mut manifest_bytes = Vec::new();
        archive
            .by_name("manifest.json")
            .expect("索引がある")
            .read_to_end(&mut manifest_bytes)
            .expect("展開");

        let held = parts.get(&EntryName::Manifest).expect("集合が索引を持つ");
        assert_eq!(held.bytes, manifest_bytes, "コンテナ層が索引を書き換えている");

        let decoded = ManifestPart::from_json_bytes(&manifest_bytes).expect("索引の復号");
        assert!(
            decoded.entries().iter().all(|entry| entry.name() != EntryName::Marker),
            "型マーカーが索引に載っている（`jxcel` は論理エントリ集合の一部ではない）"
        );
        // 索引は自分自身を載せない（`parts` 層の不変条件）ため、集合から `manifest.json` を
        // 除いた名前の列と一致する。
        let indexed: Vec<EntryName> = decoded.entries().iter().map(ManifestEntry::name).collect();
        let expected: Vec<EntryName> = parts
            .iter()
            .map(|part| part.name)
            .filter(|name| *name != EntryName::Manifest)
            .collect();
        assert_eq!(expected, indexed, "索引の内容がパート集合と違う");
    }

    /// 書き出しの失敗は再試行枯渇でない I/O エラーとして報告される（`retried` は
    /// 保存経路の rename リトライ枯渇だけが `true`）。
    #[test]
    fn zip_write_failures_are_reported_as_unretried_io_errors() {
        let DocumentError::Io { source, retried } = zip_error(ZipError::UnsupportedArchive("probe"))
        else {
            panic!("ZipError が DocumentError::Io へ写されていない");
        };
        assert!(!retried, "書き出しの失敗が再試行枯渇として報告された");
        assert_eq!(std::io::ErrorKind::Unsupported, source.kind());
        assert!(
            source.to_string().contains("probe"),
            "失敗の説明が失われている: {source}"
        );

        // `ZipError::Io` は元の I/O エラーをそのまま保つ（原因の連鎖を切らない）。
        let original = std::io::Error::new(std::io::ErrorKind::NotADirectory, "probe io");
        let DocumentError::Io { source, retried } = zip_error(ZipError::Io(original)) else {
            panic!("ZipError::Io が DocumentError::Io へ写されていない");
        };
        assert!(!retried);
        assert_eq!(std::io::ErrorKind::NotADirectory, source.kind());
    }
}

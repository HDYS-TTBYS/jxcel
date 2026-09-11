//! クレート外から見た決定的な ZIP 書き出し（タスク 5.2。要件 2.1, 3.1, 3.2, 3.6。
//! design「Container Layer / ContainerCodec」）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。
//! `document_format::container::ContainerCodec::encode` がクレート外から使えること、
//! 固定のパート集合に対する期待バイト列（`tests/fixtures/bytes/`）と**バイト単位で
//! 一致する**こと、そして決定性に関わる ZIP のパラメータが**生のバイト列の上で**
//! 固定されていることを示す。
//!
//! ZIP を「読む」側の観測（`zip::ZipArchive` による展開と、圧縮方式・更新日時・
//! エントリ内容の確認）は、本ファイルでは **`zip` を直接 import せず**に行う
//! （生のバイト列の解析のみ）。読み手としての `zip` クレートの観測は
//! `src/container/writer.rs` の単体テストが担う。ここは公開経路・バイト列・生のヘッダに限る。
//!
//! 補足（タスク 8.4 で判明）: `zip` は本クレートの依存であり、**統合テストからも名前で
//! 参照できる**（`tests/corruption.rs` が `zip::write::ZipWriter` を実際に使っている）。
//! 本ファイルが直接 import しないのは、**公開経路と生バイト列だけで決定的符号化を
//! 観測する**という方針によるもので、参照できないからではない。
//!
//! # 期待バイト列の生成手順（自己参照にしない）
//!
//! `tests/fixtures/bytes/golden_container.zip` は**本クレートの書き出し実装で生成**した
//! バイト列をそのまま固定したものである（手書きでも外部ツールでもない）。生成は
//! 固定のパート集合 [`fixed_parts`] を `ContainerCodec::encode` へ通し、その戻り値を
//! そのまま書き出す一時的なテストで行った（生成コードは残さない。テストは実装の出力から
//! 期待値を組み立てない。ファイルのバイト列が期待値の唯一の源である）。
//!
//! 標本そのものは**本物の文書**である: 各パートを本クレートの公開 API（`DocumentPart` /
//! `SchemaCodec` / `RowsCodec` / `ManifestPart`）で組み立て、[`from_parts`] が通ることを
//! 確かめてから符号化する。したがってゴールデンは「コンテナ層が壊れた」ときだけでなく
//! 「パート層の確定形が変わった」ときにも落ちる。

use std::fs;
use std::time::Duration;

use document_format::container::writer::marker_bytes;
use document_format::container::ContainerCodec;
use document_format::parts::{
    from_parts, DocumentPart, DocumentParts, ManifestEntry, ManifestPart, RowsCodec, SchemaCodec,
    SheetMeta,
};
use document_format::{AttachmentId, DocumentId, EntryName, FormatVersion, SchemaPart, SheetId};

mod common;

use common::{central_headers, fixture_path, local_headers};

/// 標本ドキュメントの識別子（正準 Crockford base32 大文字 26 文字）。
const DOCUMENT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
/// 標本シートの識別子（同上。ドキュメント識別子とは別の値）。
const SHEET_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FB0";
/// 標本の行識別子の接頭辞（`{index:04}` を足して 26 文字にする）。
const ROW_ID_PREFIX: &str = "01ARZ3NDEKTSV4RRFFQ69G";
/// 標本の行数（Deflate が自明にならない程度の分量を持たせる）。
const ROW_COUNT: u32 = 40;

/// 標本シートの列名（`document.json` の列順が権威であり、行エントリのキー順と一致させる）。
fn columns() -> Vec<String> {
    ["name", "count", "blob"]
        .iter()
        .map(|name| (*name).to_owned())
        .collect()
}

/// 標本の添付（内容は固定の式で作る。乱数も時刻も使わない）。
fn attachment_bytes() -> Vec<u8> {
    (0..512u32)
        .map(|index| (index.wrapping_mul(37) % 251) as u8)
        .collect()
}

/// 標本の行データ（1 行 1 オブジェクトの NDJSON。キー順は `$id` → 列順）。
///
/// 偶数行が添付を参照し、奇数行は `null` を置く（列集合を全行で揃えるため）。
fn rows_bytes(attachment: AttachmentId) -> Vec<u8> {
    let hex = attachment.to_hex();
    let mut out = Vec::new();
    for index in 0..ROW_COUNT {
        let id = format!("{ROW_ID_PREFIX}{index:04}");
        let blob = if index % 2 == 0 {
            format!("\"{hex}\"")
        } else {
            "null".to_owned()
        };
        out.extend_from_slice(
            format!(
                "{{\"$id\":\"{id}\",\"name\":\"標本の行 {index}\",\"count\":{index},\"blob\":{blob}}}\n"
            )
            .as_bytes(),
        );
    }
    out
}

/// 標本のスキーマ（ルートスキーマのみ。ネスト型定義は持たない）。
const SCHEMA: &str = r#"{"root":{"kind":"object"},"types":[]}"#;

/// 標本のパート集合を索引（`manifest.json`）を除く本体だけ組み立てる。
///
/// 索引は与えられた全パートから [`ManifestEntry::of_bytes`] で算出する（ダイジェストの
/// 自己申告をしない）。
fn fixed_entries() -> Vec<(EntryName, Vec<u8>)> {
    let document_id: DocumentId = DOCUMENT_ID.parse().expect("標本の識別子は正準形");
    let sheet: SheetId = SHEET_ID.parse().expect("標本の識別子は正準形");
    let columns = columns();

    let document_part = DocumentPart::new(
        document_id,
        vec![SheetMeta::new(sheet, "標本シート".to_owned()).with_columns(columns.clone())],
    )
    .expect("標本は妥当");
    let document_bytes = document_part.to_json_bytes().expect("符号化");

    let (schema_entry, schema_bytes) = SchemaCodec::encode(
        sheet,
        &SchemaPart::parse(SCHEMA).expect("標本は妥当なスキーマ"),
    )
    .expect("標本のスキーマは符号化できる");

    let rows_entry = EntryName::Rows { sheet };
    let attachment = attachment_bytes();
    let attachment_id = AttachmentId::from_bytes(&attachment);
    let rows = rows_bytes(attachment_id);

    // 標本の行テキストが本クレートの正準形であることを確かめる（4.5 の往復契約。
    // 正準形でないテキストを固定すると、ゴールデンが実装の出力ではなく
    // 手書きの揺れを固定してしまう）。
    let decoded = RowsCodec::decode(&rows_entry, &rows).expect("標本の行は妥当");
    let (canonical_entry, canonical_rows) =
        RowsCodec::encode(sheet, decoded.columns(), decoded.rows()).expect("再符号化");
    assert_eq!(rows_entry, canonical_entry, "標本の行エントリ名が変わった");
    assert_eq!(rows, canonical_rows, "標本の行テキストが正準形でない");

    vec![
        (EntryName::Document, document_bytes),
        (schema_entry, schema_bytes),
        (rows_entry, rows),
        (
            EntryName::Attachment {
                attachment: attachment_id,
            },
            attachment,
        ),
    ]
}

/// 与えられた本体パートから、正しい索引を足したパート集合を組み立てる。
fn parts_with_manifest(entries: Vec<(EntryName, Vec<u8>)>) -> DocumentParts {
    let mut entries = entries;
    let index: Vec<ManifestEntry> = entries
        .iter()
        .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
        .collect();
    let manifest = ManifestPart::new(FormatVersion::new(1, 0), index).expect("標本の索引は妥当");
    entries.push((
        EntryName::Manifest,
        manifest.to_json_bytes().expect("符号化"),
    ));
    let parts = DocumentParts::from_entries(entries).expect("標本は妥当");
    // 標本が本物の文書であること（コンテナ層が透明に運べる対象であること）を確かめる。
    from_parts(&parts).expect("標本は妥当な文書");
    parts
}

/// 標本のパート集合（固定の内容だけから決まる。入力順は [`EntryName`] 昇順）。
fn fixed_parts() -> DocumentParts {
    parts_with_manifest(fixed_entries())
}

/// 同一のパート集合を 2 回符号化したバイト列が一致する（要件 3.1）。
#[test]
fn encode_is_reproducible_for_the_same_part_set() {
    let parts = fixed_parts();
    let first = ContainerCodec::encode(&parts).expect("符号化");
    let second = ContainerCodec::encode(&parts).expect("符号化");
    assert_eq!(
        first, second,
        "同一のパート集合に対する 2 回の符号化がバイト一致しない（要件 3.1）"
    );
    assert!(!first.is_empty(), "符号化の結果が空である");
}

/// 入力の構築順を変えても符号化の結果が変わらない（要件 2.2, 3.1, 3.6）。
///
/// エントリ名昇順と一致しない入力順を 2 通り以上与える（逆順・回転）。実装が
/// `DocumentParts::iter()` の順序を再実装していたり、`HashMap` の反復順に依存して
/// いたりすれば、ここで落ちる。
#[test]
fn encode_ignores_the_construction_order_of_the_part_set() {
    let expected = ContainerCodec::encode(&fixed_parts()).expect("符号化");

    let mut reversed = fixed_entries();
    reversed.reverse();
    let mut rotated = fixed_entries();
    rotated.rotate_left(2);

    // 与える入力順が実際に昇順と異なり、互いにも異なることを確かめる（標本の取り違え防止）。
    let names = |entries: &[(EntryName, Vec<u8>)]| -> Vec<EntryName> {
        entries.iter().map(|(name, _)| *name).collect()
    };
    let ascending = names(&fixed_entries());
    assert_ne!(ascending, names(&reversed), "逆順の標本が昇順と同じである");
    assert_ne!(ascending, names(&rotated), "回転の標本が昇順と同じである");
    assert_ne!(
        names(&reversed),
        names(&rotated),
        "2 つの入力順が同じである"
    );

    for (label, entries) in [("逆順", reversed), ("回転", rotated)] {
        let parts = parts_with_manifest(entries);
        assert_eq!(
            expected,
            ContainerCodec::encode(&parts).expect("符号化"),
            "{label}で組み立てたパート集合の符号化結果が、昇順で組み立てたものと違う"
        );
    }
}

/// 期待バイト列（`tests/fixtures/bytes/`）と**バイト単位で一致する**（要件 3.1, 3.2）。
#[test]
fn encode_matches_the_committed_golden_bytes() {
    let path = fixture_path();
    let expected = fs::read(&path)
        .unwrap_or_else(|error| panic!("期待バイト列 {} が読めない: {error}", path.display()));
    let actual = ContainerCodec::encode(&fixed_parts()).expect("符号化");

    if expected != actual {
        let mismatch = expected
            .iter()
            .zip(&actual)
            .position(|(left, right)| left != right);
        panic!(
            "期待バイト列と符号化結果がバイト単位で一致しない（要件 3.1, 3.2）。\n\
             期待 {} バイト / 実際 {} バイト / 最初に相違した位置 {mismatch:?}\n\
             このテストは圧縮バックエンド（`flate2` / `miniz_oxide`）の更新で決定性が\
             壊れたことを検出するためにある。相違した場合は、形式のマイナーバージョンを\
             上げるか固定を継続するかを判断すること（design「ContainerCodec / \
             Implementation Notes」）。\
             再生成は `fixed_parts()` を `ContainerCodec::encode` へ通した戻り値を\
             このパスへそのまま書き出す一時的なテストで行う（生成コードは残さない）。",
            expected.len(),
            actual.len(),
        );
    }
    assert!(!actual.is_empty(), "符号化の結果が空である");
}

/// 決定性に関わる ZIP のパラメータが、生のバイト列の上で固定されている（要件 3.6）。
///
/// 観測するのは次の 6 点である（いずれも crate の既定値ではなく本実装の固定値）:
/// エントリ書き込み順（型マーカーが先頭 + `DocumentParts::iter()` の昇順）、更新日時、
/// データディスクリプタの不在、圧縮方式（型マーカーが `Stored`・他が `Deflate`）、
/// version made by のホスト OS バイト、unix permissions。
#[test]
fn encode_fixes_every_determinism_relevant_zip_parameter() {
    let bytes = ContainerCodec::encode(&fixed_parts()).expect("符号化");
    let local = local_headers(&bytes);
    let central = central_headers(&bytes);

    // エントリ書き込み順: 型マーカーが先頭、続いて `DocumentParts::iter()` の昇順。
    let expected_names: Vec<String> = std::iter::once("jxcel".to_owned())
        .chain(fixed_parts().iter().map(|part| part.name.to_string()))
        .collect();
    let local_names: Vec<String> = local.iter().map(|header| header.name.clone()).collect();
    let central_names: Vec<String> = central.iter().map(|header| header.name.clone()).collect();
    assert_eq!(
        expected_names, local_names,
        "エントリの書き込み順が固定されていない"
    );
    assert_eq!(
        local_names, central_names,
        "中央ディレクトリの順序がローカルヘッダの書き込み順と違う"
    );
    assert_eq!(
        "jxcel", local_names[0],
        "型マーカーが先頭エントリでない（固定オフセットでの型判定ができない）"
    );

    for header in &local {
        assert_eq!(
            (0x0000u16, 0x0021u16),
            (header.modified_time, header.modified_date),
            "更新日時が 1980-01-01 00:00:00 に固定されていない: {}",
            header.name
        );
        assert_eq!(
            0,
            header.flags & 0x0008,
            "データディスクリプタが使われている（サイズは既知であり使ってはならない）: {}",
            header.name
        );
    }
    for header in &central {
        assert_eq!(
            (0x0000u16, 0x0021u16),
            (header.modified_time, header.modified_date),
            "中央ディレクトリの更新日時が固定されていない: {}",
            header.name
        );
        assert_eq!(
            0,
            header.flags & 0x0008,
            "中央ディレクトリがデータディスクリプタの使用を宣言している: {}",
            header.name
        );
        assert_eq!(
            3,
            (header.version_made_by >> 8) as u8,
            "version made by のホスト OS バイトが Unix 固定でない（ビルド環境を反映している）: {}",
            header.name
        );
        assert_eq!(
            0o100644,
            header.external_attributes >> 16,
            "unix permissions が固定されていない: {}",
            header.name
        );
    }

    // 圧縮方式: 型マーカーは無圧縮（`Stored` = 0）、他のエントリは `Deflate` = 8。
    assert_eq!(0, local[0].compression_method, "型マーカーが無圧縮でない");
    assert_eq!(0, central[0].compression_method, "型マーカーが無圧縮でない");
    for header in local.iter().skip(1) {
        assert_eq!(
            8, header.compression_method,
            "Deflate でない: {}",
            header.name
        );
    }
    for header in central.iter().skip(1) {
        assert_eq!(
            8, header.compression_method,
            "Deflate でない: {}",
            header.name
        );
    }

    // 型マーカーの内容は確定形（`jxcel\n<major>.<minor>\n`。`Stored` なので展開せずに
    // 読める）。バージョンの値は索引の記録と一致する（単体テストが突き合わせる）。
    let marker = &local[0];
    let end = marker.data_start + marker.data_len;
    assert_eq!(
        marker_bytes(fixed_parts().format_version()),
        &bytes[marker.data_start..end],
        "型マーカーの内容が確定形と違う"
    );
}

/// 現在時刻を進めても符号化の結果が変わらない（要件 3.6: 保存時刻に依存しない）。
#[test]
fn encode_contains_no_value_derived_from_the_current_time() {
    let parts = fixed_parts();
    let first = ContainerCodec::encode(&parts).expect("符号化");
    // DOS 時刻の分解能は 2 秒である。境界を必ずまたぐ長さだけ待ってから符号化し直す。
    std::thread::sleep(Duration::from_millis(2_100));
    let second = ContainerCodec::encode(&parts).expect("符号化");
    assert_eq!(
        first, second,
        "時刻を進めた後の符号化がバイト一致しない（保存時刻に依存している。要件 3.6）"
    );
}

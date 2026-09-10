//! クレート外から見た形式バージョンのゲート（タスク 6.1。要件 6.1, 6.5。
//! design「MigrationChain」/「読み込みフロー」）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。要件 6.1（保存経路が
//! 書く `manifest.json` に現行バージョンが記録され、集合がそれを報告すること）と、要件 6.5
//! （現行より新しい major の集合を、要求バージョンを含む `UnsupportedVersion` として拒否し、
//! 部分的なモデルを返さないこと）を、クレート外の観測点から固定する。
//!
//! ゲートの判定そのもの（3 分岐の意味）と、`from_parts` の内部順序（ゲートがダイジェスト
//! 照合より先であること）は `src/parts/document_parts.rs` と `src/migration/mod.rs` の
//! 単体テストが担う。ここは**公開経路とコンテナ経路の端から端まで**を確かめる。
//!
//! # タスク 6.2 への申し送り
//!
//! 現行より**古い** major は design 上「移行チェーンの適用が必要」だが、適用先の実チェーンは
//! タスク 6.2 が実装する。本タスクの読み込み経路は移行先が無い版を開けないため、
//! [`an_older_major_is_rejected_until_the_migration_chain_lands`] が固定しているとおり
//! 同じ `UnsupportedVersion` として中止する。6.2 はこの分岐を実チェーン適用へ置き換え、
//! このテストの期待も更新する。

use document_format::container::ContainerCodec;
use document_format::entry_name::MANIFEST_ENTRY;
use document_format::parts::{from_parts, to_parts, DocumentParts, ManifestEntry, ManifestPart};
use document_format::{Document, DocumentError, EntryName, FormatVersion};

/// 本実装の現行バージョン（design「Container Entry Layout」の 1.0）。
const CURRENT: FormatVersion = FormatVersion::new(1, 0);

/// 標本の文書。ゲートの判定は内容に依存しないため 0 シートの最小の文書で足りる。
fn sample() -> Document {
    Document::new()
}

/// 標本の集合を、指定バージョンを記録した索引で組み直す（`adjust` で索引を壊せるようにする）。
///
/// 索引の再組み立てには公開の [`ManifestEntry`] / [`ManifestPart`] を使う
/// （実装の内部経路は使わない）。内容は本物の保存経路が出力したものと同じである。
fn parts_with_index(
    version: FormatVersion,
    adjust: impl FnOnce(&mut Vec<ManifestEntry>),
) -> DocumentParts {
    let parts = to_parts(&sample()).expect("保存経路");
    let mut entries: Vec<(EntryName, Vec<u8>)> =
        parts.iter().map(|part| (part.name, part.bytes.clone())).collect();
    // 呼び出し元が持つ索引は使わず、実体に合わせて組み直す（`adjust` の壊し方が効くように）。
    entries.retain(|(name, _)| *name != MANIFEST_ENTRY);
    let mut index: Vec<ManifestEntry> = entries
        .iter()
        .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
        .collect();
    adjust(&mut index);
    let manifest = ManifestPart::new(version, index)
        .expect("標本の索引は妥当")
        .to_json_bytes()
        .expect("符号化");
    entries.push((MANIFEST_ENTRY, manifest));
    DocumentParts::from_entries(entries).expect("標本の集合は妥当")
}

/// 指定バージョンを記録した標本の集合（索引は実体と一致している）。
fn parts_at(version: FormatVersion) -> DocumentParts {
    parts_with_index(version, |_| {})
}

/// ゲートの公開面はクレート外から見える（タスク 7.x が読み込み経路の結線に使う）:
/// [`document_format::migration`] とクレート根の再輸出が同じ項目を指す。
#[test]
fn the_versioning_surface_is_reachable_from_outside_the_crate() {
    use document_format::migration::{MigrationChain as Chain, VersionVerdict as Verdict};

    assert_eq!(
        CURRENT,
        document_format::CURRENT_FORMAT_VERSION,
        "クレート根の現行版が違う"
    );
    assert_eq!(CURRENT, document_format::migration::CURRENT_FORMAT_VERSION);
    assert!(Chain::admit(CURRENT).is_ok(), "現行版がゲートを通らない");
    assert_eq!(Verdict::Openable, Chain::gate(CURRENT));
    assert_eq!(
        Verdict::Unsupported { found: FormatVersion::new(2, 7), supported: CURRENT },
        Chain::gate(FormatVersion::new(2, 7))
    );
    assert_eq!(
        Verdict::NeedsMigration { from: FormatVersion::new(0, 9) },
        Chain::gate(FormatVersion::new(0, 9))
    );
}

/// 要件 6.1: 保存経路が書く `manifest.json` に現行バージョンが記録され、集合がそれを報告する。
#[test]
fn the_manifest_records_the_current_format_version() {
    let parts = to_parts(&sample()).expect("保存経路");
    assert_eq!(CURRENT, parts.format_version(), "集合が報告する現行バージョンが違う");

    // 記録されているのは索引**そのもの**である（集合の内部値ではなくバイト列から確かめる）。
    let manifest = parts.get(&MANIFEST_ENTRY).expect("集合は索引を持つ");
    let decoded = ManifestPart::from_json_bytes(&manifest.bytes).expect("索引は復号できる");
    assert_eq!(CURRENT, decoded.version(), "manifest.json に現行バージョンが記録されていない");
}

/// 要件 6.5: 現行より新しい major の集合は、**要求バージョンを含む** `UnsupportedVersion`
/// として拒否され、部分的なモデルを返さない（要件 5.4）。
#[test]
fn a_newer_major_is_rejected_with_the_requested_version() {
    for recorded in [FormatVersion::new(2, 7), FormatVersion::new(2, 0), FormatVersion::new(9, 9)] {
        match from_parts(&parts_at(recorded)) {
            Err(DocumentError::UnsupportedVersion { found, supported }) => {
                assert_eq!(recorded, found, "要求バージョンが報告されていない");
                assert_eq!(CURRENT, supported, "対応バージョンが報告されていない");
                let text =
                    DocumentError::UnsupportedVersion { found, supported }.to_string();
                assert!(
                    text.contains(&recorded.to_string()),
                    "Display に要求バージョンが現れない: {text}"
                );
                assert!(
                    text.contains(&CURRENT.to_string()),
                    "Display に対応バージョンが現れない: {text}"
                );
            }
            Ok(document) => panic!(
                "{recorded} が拒否されず、部分的なモデルが返った（{} シート）",
                document.sheets().len()
            ),
            Err(other) => panic!("{recorded} の変種が違う: {other}"),
        }
    }
}

/// 同一 major は minor が現行より新しくても受理される（design「MigrationChain」: minor の増加は
/// 省略可能フィールドの追加のみに限られ、未知フィールドは破棄せず保持して書き戻すため）。
#[test]
fn a_same_major_is_accepted_even_with_a_newer_minor() {
    for recorded in [CURRENT, FormatVersion::new(1, 1), FormatVersion::new(1, 99)] {
        let restored = from_parts(&parts_at(recorded))
            .unwrap_or_else(|error| panic!("同一 major の {recorded} が拒否された: {error}"));
        assert!(restored.sheets().is_empty(), "{recorded} のモデルが標本と違う");
    }
}

/// 現行より古い major は「移行チェーンの適用が必要」である。**適用先の実チェーンはタスク 6.2**
/// が実装するため、本タスクでは移行先が無い版として同じ中止（`UnsupportedVersion`）で報告する
/// （6.2 がこの期待を実チェーン適用へ置き換える）。
#[test]
fn an_older_major_is_rejected_until_the_migration_chain_lands() {
    for recorded in [FormatVersion::new(0, 9), FormatVersion::new(0, 0)] {
        match from_parts(&parts_at(recorded)) {
            Err(DocumentError::UnsupportedVersion { found, supported }) => {
                assert_eq!(recorded, found, "移行前の版が報告されていない");
                assert_eq!(CURRENT, supported, "対応バージョンが報告されていない");
            }
            Ok(document) => panic!(
                "{recorded} の移行先がまだ無いのにモデルが返った（{} シート）",
                document.sheets().len()
            ),
            Err(other) => panic!("{recorded} の変種が違う: {other}"),
        }
    }
}

/// ゲートはダイジェスト照合（design 読み込みフローの「形式バージョン」→「ダイジェスト照合」）
/// より先に走る: 索引が実体の無いパートを載せていても、バージョンが新しければ
/// `UnsupportedVersion` が返る（`MissingPart` や `IntegrityMismatch` ではない）。
#[test]
fn the_version_gate_runs_before_the_integrity_check() {
    let absent =
        EntryName::parse("sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonl").expect("標本は許可リスト内");
    let parts = parts_with_index(FormatVersion::new(2, 7), |index| {
        index.push(ManifestEntry::of_bytes(absent, b"no such part in the set"));
    });

    match from_parts(&parts) {
        Err(DocumentError::UnsupportedVersion { found, .. }) => {
            assert_eq!(FormatVersion::new(2, 7), found);
        }
        Ok(document) => panic!("拒否されずモデルが返った（{} シート）", document.sheets().len()),
        Err(other) => panic!("ゲートが完全性照合より後にある: {other}"),
    }
}

/// コンテナ経路の端から端まで: 新しい major の集合は符号化・復号までは成功し（ゲートは復号の
/// 責務ではない）、読み込み（[`from_parts`]）で拒否される。マーカーは索引の記録値から導出
/// されるため、復号後の集合も 2.7 を報告する。
#[test]
fn a_newer_major_survives_the_container_until_the_read_gate() {
    let encoded = ContainerCodec::encode(&parts_at(FormatVersion::new(2, 7))).expect("符号化");
    let decoded =
        ContainerCodec::decode(&encoded).expect("復号は成功する（ゲートは復号の責務ではない）");
    assert_eq!(FormatVersion::new(2, 7), decoded.format_version(), "復号で記録値が失われた");

    match from_parts(&decoded) {
        Err(DocumentError::UnsupportedVersion { found, supported }) => {
            assert_eq!(FormatVersion::new(2, 7), found, "要求バージョンが報告されていない");
            assert_eq!(CURRENT, supported, "対応バージョンが報告されていない");
        }
        Ok(document) => panic!("拒否されずモデルが返った（{} シート）", document.sheets().len()),
        Err(other) => panic!("コンテナ経路の変種が違う: {other}"),
    }
}

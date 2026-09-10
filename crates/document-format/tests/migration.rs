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
//! # タスク 6.2 で置き換えたこと
//!
//! 現行より**古い** major は design 上「移行チェーンの適用が必要」であり、タスク 6.1 は
//! 移行先が無い版として `UnsupportedVersion` で中止していた。**タスク 6.2 はその分岐を実
//! チェーン適用へ置き換えた**: 読み込み経路は記録値から現行 major まで段を 1 段ずつ適用し、
//! 実表が空である初版では「移行を試みたが移行先が無い」として**同じ `UnsupportedVersion`** で
//! 中止する（[`an_older_major_without_a_migration_target_is_rejected`]）。
//!
//! 多段の適用そのものはクレート内部の合成表（`migration::steps::synthetic`）を使う単体テストが
//! 担う（統合テストは公開面だけを使うため、ステップ表を注入できない）。ここは公開経路と
//! コンテナ経路の端から端までを確かめる。
//!
//! # タスク 8.7 で足したこと（ゴールデン fixture。要件 6.2, 6.3, 6.4）
//!
//! 移行チェーンの中間ステップが未保守のまま腐る失敗形態は先行事例（nbformat）で実際に起きて
//! おり、**過去版ごとのゴールデン fixture が唯一の防御**である（design「Migration Strategy /
//! 検証チェックポイント」）。本ファイルは次を固定する:
//!
//! - [`the_current_version_golden_fixture_round_trips_byte_for_byte`]: 現行版の fixture
//!   （`tests/fixtures/golden/v1/anchored.jxcel`）が現行として読め、往復で**同一のコンテナ**に
//!   戻ること（期待値は fixture のバイト列そのもの）。
//! - [`every_version_in_the_migration_chain_has_a_golden_fixture`]: `STEPS` の各段の `from` と
//!   `CURRENT_FORMAT_VERSION` から**必要な fixture の集合を導出**し、その全てに実在する fixture
//!   が記録バージョンまで一致して置かれていること。新しい版を足して fixture を忘れると落ちる。
//!
//! **変換が発生した場合に初回保存で退避が残ること**（要件 6.4）は、ディスク上の古い版 fixture を
//! 入力にする必要があるため、`src/lib.rs` のクレート内部テスト
//! `save_after_migrating_a_disk_fixture_keeps_the_pre_conversion_file` が担う（`from_parts_with`
//! と合成ステップ表がクレート内部のため。統合テストは公開面だけを使う）。

use std::fs;
use std::path::{Path, PathBuf};

use document_format::container::ContainerCodec;
use document_format::entry_name::MANIFEST_ENTRY;
use document_format::migration::steps::STEPS;
use document_format::parts::{from_parts, to_parts, DocumentParts, ManifestEntry, ManifestPart};
use document_format::{
    Document, DocumentError, DocumentFormatApi, EntryName, FormatVersion, CURRENT_FORMAT_VERSION,
};

mod common;

use common::api;

/// 本実装の現行バージョン。
///
/// クレートの**単一定義**から導出する（テスト内に `1.0` を直書きしない。「現行が 1.0 である」
/// ことの固定は `src/migration/mod.rs` の `the_current_version_is_one_zero` が担う）。
const CURRENT: FormatVersion = CURRENT_FORMAT_VERSION;

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
    // 現行版は移行されない（集合を複製しない）。
    assert!(
        Chain::apply(&parts_at(CURRENT)).expect("現行版は中止しない").is_none(),
        "現行版が移行された"
    );
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

/// 現行より古い major は、実表（v1 のみ・空）に移行先が無いため `UnsupportedVersion` で中止する
/// （要件 6.2 の「移行したうえで構築する」は、段が現行 major へ届く場合の応答である）。
///
/// タスク 6.1 は「ゲートで即拒否」していた。**タスク 6.2 は移行チェーンを試みる経路に置き換え、
/// 移行先が無い場合の観測結果は同じ**である（`found` は記録値 = 移行前の版、`supported` は現行）。
#[test]
fn an_older_major_without_a_migration_target_is_rejected() {
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

/// 過去バージョンごとのゴールデン fixture の置き場が存在する（design「File Structure Plan」の
/// `tests/fixtures/golden/vN/`）。
///
/// # 配置・命名・生成・更新の規約
///
/// - **配置**: 形式バージョンごとに `tests/fixtures/golden/v<major>/` を置く（初版は `v1/`）。
///   ディレクトリが空になる場合は `.gitkeep` を置いて追跡される形にする（`v1/` は
///   `anchored.jxcel` を持つため `.gitkeep` を置かない）。
/// - **命名**: その版の代表的なドキュメントを `<名前>.jxcel` として置く（複数可）。名前は内容が
///   分かる英小文字の語（例 `minimal.jxcel` / `two_sheets.jxcel`）。
/// - **生成**: 期待値は**その版を書いた実装の出力**をそのまま固定する（手書きでも外部ツールでも
///   ない。`tests/fixtures/bytes/golden_container.zip` と同じ方針）。生成コードは残さない。
///   `v1/anchored.jxcel` は `tests/container_writer.rs` の `fixed_parts`（識別子まで固定した
///   1 シート 40 行 + 添付の標本）を `ContainerCodec::encode` へ通した出力であり、
///   `tests/fixtures/bytes/golden_container.zip` と同一のバイト列である（git のブロブは
///   内容アドレスで共有されるため、バイト列を二重に保持しない）。
/// - **更新**: 形式バージョンを上げたときは**新バージョンの fixture を追加**し、過去バージョンの
///   fixture は「移行チェーンが現行版へ運べること」の入力として保つ（比較は
///   [`the_current_version_golden_fixture_round_trips_byte_for_byte`] と
///   [`every_version_in_the_migration_chain_has_a_golden_fixture`] が担う。タスク 8.7）。
///   移行チェーンの中間ステップが未保守のまま腐る失敗形態は先行事例（nbformat）で
///   実際に起きており、fixture が唯一の防御である（design「Migration Strategy /
///   検証チェックポイント」）。
///
/// **初版の注記（実在する過去版は無い）**: 現時点でコミットされているのは `v1/`（現行版）
/// だけである。古い版を移行の入力にするテストは、コミット済みの現行版 fixture の
/// **記録バージョンだけを差し替えて合成**する（`src/lib.rs` の
/// `save_after_migrating_a_disk_fixture_keeps_the_pre_conversion_file`）。**v0 という形式は
/// 歴史上存在せず**、合成の入力は「過去版が実在した」ことを意味しない。
#[test]
fn the_golden_fixture_directory_for_the_current_version_exists() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/golden/v{}", CURRENT.major));
    assert!(
        directory.is_dir(),
        "過去バージョンのゴールデン fixture の置き場が無い: {}",
        directory.display()
    );
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

/// 形式バージョン `version` のゴールデン fixture の置き場（`tests/fixtures/golden/v<major>/`）。
///
/// ディレクトリ名が表すのは **major** だけである（minor の fixture も同じディレクトリへ置き、
/// 記録バージョンで区別する。`the_golden_fixture_directory_for_the_current_version_exists`
/// の doc「配置・命名」）。
fn golden_directory(version: FormatVersion) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/golden/v{}", version.major))
}

/// ディレクトリ直下の `.jxcel` fixture を名前順で返す（列挙順に依存しない）。
///
/// `.gitkeep` 等の fixture でないファイルは除く。
fn golden_fixtures(directory: &Path) -> Vec<PathBuf> {
    let mut fixtures: Vec<PathBuf> = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{} が読めない: {error}", directory.display()))
        .map(|entry| entry.expect("ディレクトリ要素が読める").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jxcel"))
        .collect();
    fixtures.sort();
    fixtures
}

/// 現行版のゴールデン fixture の絶対パス（`golden/v<現行 major>/` のうち、**記録バージョンが
/// [`CURRENT`] と一致する**コンテナ）。
///
/// 選択は「ディレクトリにある唯一の `.jxcel`」ではなく**記録バージョンの一致**で行う。規約
/// （[`the_golden_fixture_directory_for_the_current_version_exists`] の doc）は「同じ major の
/// 複数 minor は同じディレクトリへ置き、**記録バージョンで区別する**」と定めているため、
/// 例えば minor の fixture（記録 1.1）を同じ `v1/` へ足しても本ヘルパは壊れない。
fn current_golden_fixture() -> PathBuf {
    let directory = golden_directory(CURRENT);
    let mut matches: Vec<PathBuf> = Vec::new();
    for path in golden_fixtures(&directory) {
        let bytes = fs::read(&path)
            .unwrap_or_else(|error| panic!("{} が読めない: {error}", path.display()));
        let recorded = ContainerCodec::decode(&bytes)
            .unwrap_or_else(|error| {
                panic!("{} がコンテナとして復号できない: {error}", path.display())
            })
            .format_version();
        if recorded == CURRENT {
            matches.push(path);
        }
    }
    assert_eq!(
        1,
        matches.len(),
        "{} に記録バージョン {CURRENT} の fixture が 1 つでない: {matches:?}",
        directory.display()
    );
    matches.into_iter().next().expect("1 つある")
}

/// 現行版（v1）のゴールデン fixture は現行として読め、往復で**同一のコンテナ**に戻る
/// （要件 4.1, 6.2, 3.1）。
///
/// 期待値は **fixture のバイト列そのもの**である（実装の出力から期待値を組み立てる自己参照を
/// しない。`tests/container_writer.rs` の `encode_matches_the_committed_golden_bytes` と同じ
/// 方針）。確かめるのは 4 点:
///
/// 1. `open` が成功すること（現行版として読める）。
/// 2. 変換が発生しないこと（`OpenOutcome::migrated_from` が `None`）。
/// 3. `to_parts` → 符号化が fixture のバイト列と同一であること（往復決定性）。
/// 4. 公開の `from_parts` / `to_parts` 経由でも同じ文書が得られること（ZIP を経由しない経路）。
#[test]
fn the_current_version_golden_fixture_round_trips_byte_for_byte() {
    let path = current_golden_fixture();
    let expected =
        fs::read(&path).unwrap_or_else(|error| panic!("{} が読めない: {error}", path.display()));

    let outcome = api().open(&path).expect("現行版の fixture は現行として読める");
    assert_eq!(None, outcome.migrated_from, "現行版の fixture で移行が起きた");

    let parts = api().to_parts(&outcome.document).expect("読み込んだ文書は保存できる");
    assert_eq!(CURRENT, parts.format_version(), "fixture が現行版として読まれていない");
    let encoded = ContainerCodec::encode(&parts).expect("符号化");
    assert_eq!(
        expected,
        encoded,
        "現行版の fixture の往復がバイト単位で一致しない（要件 3.1, 6.2）"
    );

    // ファイルを経由しない公開契約（`version-control` との唯一の接点）でも同じ文書になる。
    let restored = api().from_parts(&parts).expect("集合からモデルへ戻せる");
    let reencoded = ContainerCodec::encode(&api().to_parts(&restored).expect("保存経路"))
        .expect("符号化");
    assert_eq!(expected, reencoded, "from_parts / to_parts 経由の往復が一致しない");
}

/// 移行チェーンと現行バージョンが要求する全ての版に、ゴールデン fixture が実在する
/// （design「Migration Strategy / 検証チェックポイント」。要件 6.2, 6.3）。
///
/// # 何を導出しているか
///
/// `STEPS` の各段の `from`（＝移行の**入力**になる版）と `CURRENT_FORMAT_VERSION` が、
/// ゴールデン fixture を持つべき版の集合そのものである。実装の表 [`STEPS`] を読み取り、
/// 表と fixture の食い違いをテストの構造で検出する（新しい版を足して fixture を忘れた瞬間に
/// ここが落ちる）。
///
/// # 新しい形式バージョンを足すときの手順
///
/// 1. `migration::steps::STEPS` に隣接版の変換（`from` → `to`）を足し、
///    `CURRENT_FORMAT_VERSION` を新しい版へ上げる。
/// 2. **その版を書いた実装の出力**を `tests/fixtures/golden/v<新しい major>/<名前>.jxcel`
///    として置く（手書きでも外部ツールでもない。生成コードは残さない。配置・命名の規約は
///    [`the_golden_fixture_directory_for_the_current_version_exists`] の doc）。同じ major の
///    複数 minor は同じディレクトリに置き、**記録バージョンで区別する**（ディレクトリ名が
///    表すのは major だけである）。
/// 3. 過去版の fixture は**移行の入力として残す**（消さない）。過去版を消すと、移行チェーンの
///    中間ステップが未保守のまま腐る失敗形態を検出できなくなる（先行事例 nbformat）。
///
/// 1 だけを行って 2 を忘れると本テストが「fixture が無い」で落ちる。
///
/// # 検査の内容
///
/// 要求された版 `version` について、(a) `golden/v<major>/` が存在し、(b) `.jxcel` fixture を
/// 少なくとも 1 つ持ち、(c) そこにある fixture は全てコンテナとして復号できて記録 major が
/// ディレクトリ名と一致し、(d) `version` と**記録バージョンまで一致する** fixture が 1 つ以上
/// あること（`from` と記録値は minor も含めて完全一致で引かれるため）を確かめる。
#[test]
fn every_version_in_the_migration_chain_has_a_golden_fixture() {
    let mut versions: Vec<FormatVersion> = STEPS.iter().map(|step| step.from()).collect();
    versions.push(CURRENT_FORMAT_VERSION);
    versions.sort();
    versions.dedup();
    assert!(!versions.is_empty(), "現行版が fixture の要求集合へ入っていない");

    for version in versions {
        let directory = golden_directory(version);
        assert!(
            directory.is_dir(),
            "要求された版 {version} の置き場が無い: {}",
            directory.display()
        );

        let fixtures = golden_fixtures(&directory);
        assert!(
            !fixtures.is_empty(),
            "要求された版 {version} の fixture が {} に無い（新しい版を足したら、\
             その版を書いた実装の出力を置くこと）",
            directory.display()
        );

        let mut exact = false;
        for path in &fixtures {
            let bytes =
                fs::read(path).unwrap_or_else(|error| panic!("{} が読めない: {error}", path.display()));
            let decoded = ContainerCodec::decode(&bytes).unwrap_or_else(|error| {
                panic!("{} がコンテナとして復号できない: {error}", path.display())
            });
            let recorded = decoded.format_version();
            assert_eq!(
                version.major,
                recorded.major,
                "{} の記録 major がディレクトリ名 v{} と違う: {recorded}",
                path.display(),
                version.major
            );
            exact |= recorded == version;
        }
        assert!(
            exact,
            "golden/v{}/ に記録バージョン {version} の fixture が無い（過去版は移行の入力として\
             保持すること）",
            version.major
        );
    }
}

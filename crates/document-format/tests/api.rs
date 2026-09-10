//! クレート外から見た公開 API の読み込み経路（タスク 7.1。要件 4.1, 5.2, 5.4, 5.5, 8.5）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。design
//! 「Public API Layer / DocumentFormatApi」の `open` が実ファイルを読み、ZIP 復号
//! （タスク 5.3）・パート層の 1 経路（タスク 4.8 / 6.1 / 6.2）を通して
//! [`OpenOutcome`] を返すことを確かめる。
//!
//! 検証する横断的な性質は次の 4 つである（レビュー教訓に従い対象を分散させる）:
//!
//! 1. **往復**: モデル → パート集合 → ZIP → ファイル → `open` で同一のモデルが戻る。
//! 2. **非書き込み**（要件 5.5）: `open` の前後で対象ファイルのバイト列と
//!    作業ディレクトリの内容（名前とバイト列）が完全に一致する。一時ファイルの残留も
//!    同時に検出できる形にする。
//! 3. **部分モデルを返さない**（要件 5.4）: 後段（ダイジェスト照合・構造検証）で
//!    失敗する入力を `open` が `Err` として観測する。
//! 4. **規模の通知**（要件 8.4, 8.5）: 保証対象の境界（100,000 / 100,001 行）で
//!    `beyond_supported_scale` が切り替わり、超過しても `Ok` である。
//!
//! 一時ファイルはリポジトリ内のテスト専用ディレクトリ（`tests/api_tmp_*`）に作り、
//! 各テストの終了時に [`Scratch`] の `Drop` が削除する（コンテナ内とホストで
//! `std::env::temp_dir()` の見え方が違うため、リポジトリ内に閉じる）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use document_format::container::ContainerCodec;
use document_format::parts::{
    to_parts, DocumentParts, ManifestEntry, ManifestPart, SchemaCodec,
};
use document_format::{
    CellValue, Document, DocumentError, DocumentFormat, DocumentFormatApi, EntryName,
    FormatVersion, SchemaPart, SheetId, SUPPORTED_ROW_LIMIT,
};

/// 標本のルートスキーマ（型定義への参照を含む）。
///
/// 型定義識別子は `concat!` がリテラルを要求するため、各断片に直接埋め込む。
const SCHEMA_WITH_REF: &str = concat!(
    r#"{"root":{"$ref":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#""},"types":[{"id":""#,
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    r#"","definition":{"kind":"string"}}]}"#
);

/// 型定義を 1 つも持たないルートスキーマ（0 行のシートの初期値）。
const SCHEMA_EMPTY: &str = r#"{"root":null}"#;

/// 読み込み経路を起動する実装（design はトレイトのみを指定するため、無状態の具象型を使う）。
fn api() -> DocumentFormat {
    DocumentFormat::new()
}

/// ゴールデン fixture（決定性の基準として固定されたコンテナ）の絶対パス。
///
/// テストの作業ディレクトリに依存しないよう、マニフェストの位置から組み立てる。
fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bytes/golden_container.zip")
}

/// テスト専用の作業ディレクトリ（終了時に必ず削除する）。
///
/// テストは並行に走るため、プロセス ID と単調カウンタで一意にする。`Drop` は
/// panic による巻き戻しでも走るので、失敗したテストの一時ファイルも残らない。
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Self {
        static SEQUENCE: AtomicU32 = AtomicU32::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(format!("api_tmp_{tag}_{}_{sequence}", std::process::id()));
        fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // 既に消えていても失敗しない（多重削除・異常終了の後始末）。
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// ディレクトリ直下の（名前, バイト列）を名前順で写す。
///
/// ファイルの改変（バイト列の変化）と一時ファイルの残留（名前の追加）を同時に捉える。
fn snapshot(directory: &Path) -> Vec<(String, Vec<u8>)> {
    let mut entries: Vec<(String, Vec<u8>)> = fs::read_dir(directory)
        .expect("作業ディレクトリが読める")
        .map(|entry| {
            let entry = entry.expect("ディレクトリ要素が読める");
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = fs::read(entry.path()).expect("要素が読める");
            (name, bytes)
        })
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

/// 標本の文書をパート集合へ符号化し、ファイルとして書き出す。
///
/// `save`（タスク 7.2）は未実装であるため、テストが公開経路
/// （`to_parts` → `ContainerCodec::encode`）でファイルを用意する。
fn write_document(scratch: &Scratch, name: &str, document: &Document) -> PathBuf {
    let parts = to_parts(document).expect("保存経路");
    let bytes = ContainerCodec::encode(&parts).expect("符号化");
    let path = scratch.file(name);
    fs::write(&path, bytes).expect("書き出し");
    path
}

/// 与えられた（エントリ名, バイト列）からコンテナのバイト列を組み立てる。
///
/// 索引は呼び出し元が与えた集合に**そのまま含まれるものを尊重**する（差し替えの
/// 有無でダイジェスト照合の成否を切り替えられるようにする）。
fn encode_entries(entries: Vec<(EntryName, Vec<u8>)>) -> Vec<u8> {
    let parts = DocumentParts::from_entries(entries).expect("標本の集合は妥当");
    ContainerCodec::encode(&parts).expect("符号化")
}

/// 標本のエントリ集合を（エントリ名, バイト列）の列へ写す。
fn entries_of(parts: &DocumentParts) -> Vec<(EntryName, Vec<u8>)> {
    parts.iter().map(|part| (part.name, part.bytes.clone())).collect()
}

/// 索引（`manifest.json`）を実体から組み直したエントリ集合を返す。
///
/// ダイジェスト照合を通過させたまま後段（構造検証）だけを壊す入力を作るのに使う。
fn rebuilt_with_manifest(
    version: FormatVersion,
    mut entries: Vec<(EntryName, Vec<u8>)>,
) -> Vec<u8> {
    entries.retain(|(name, _)| *name != EntryName::Manifest);
    let index: Vec<ManifestEntry> = entries
        .iter()
        .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
        .collect();
    let manifest = ManifestPart::new(version, index)
        .expect("標本の索引は妥当")
        .to_json_bytes()
        .expect("符号化");
    entries.push((EntryName::Manifest, manifest));
    encode_entries(entries)
}

/// シートの（識別子, 名前, 列名）を文書順に写す。
fn sheet_metadata(document: &Document) -> Vec<(SheetId, String, Vec<String>)> {
    document
        .sheets()
        .iter()
        .map(|sheet| (sheet.id(), sheet.name().to_owned(), sheet.columns().to_vec()))
        .collect()
}

/// 行の（識別子, セル値）をシート順・行順に写す。
fn rows(document: &Document) -> Vec<Vec<(String, Vec<CellValue>)>> {
    document
        .sheets()
        .iter()
        .map(|sheet| {
            sheet
                .rows()
                .iter()
                .map(|row| (row.id().to_string(), row.values().to_vec()))
                .collect()
        })
        .collect()
}

/// ルートスキーマの（ルートバイト列, 型定義識別子, 参照）をシート順に写す。
fn schemas(document: &Document) -> Vec<(Vec<u8>, Vec<String>, Vec<(String, String)>)> {
    document
        .sheets()
        .iter()
        .map(|sheet| {
            let schema = sheet.root_schema();
            let defs: Vec<String> = schema.type_def_ids().iter().map(|id| id.to_string()).collect();
            let refs: Vec<(String, String)> = schema
                .type_refs()
                .iter()
                .map(|reference| (reference.from().to_owned(), reference.to().to_owned()))
                .collect();
            (schema.root().as_bytes().to_vec(), defs, refs)
        })
        .collect()
}

/// 添付の（識別子, バイト列）を昇順に写す。
fn attachments(document: &Document) -> Vec<(String, Vec<u8>)> {
    document
        .attachments()
        .iter()
        .map(|attachment| (attachment.id().to_string(), attachment.bytes().to_vec()))
        .collect()
}

/// 標本の文書: 行と添付参照を持つシートと、0 行のシート。
fn sample() -> Document {
    let mut document = Document::new();

    let stocked = document.add_sheet("在庫");
    document
        .set_sheet_columns(
            stocked,
            ["name", "count", "blob"].iter().map(|name| (*name).to_owned()).collect(),
        )
        .expect("標本のシートは実在する");
    document
        .set_root_schema(stocked, SchemaPart::parse(SCHEMA_WITH_REF).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    // 非 UTF-8 を含む添付（要件 7.5 の不透明性）。
    let attachment = document.add_attachment(vec![0xff, 0x00, 0x80, b'j', 0xfe]);
    let row = document.add_row(stocked).expect("標本のシートは実在する");
    document
        .set_row_values(
            stocked,
            row,
            vec![
                CellValue::Text("りんご 🍎".to_owned()),
                CellValue::Int(3),
                CellValue::Attachment(attachment),
            ],
        )
        .expect("標本の行は実在する");

    let empty = document.add_sheet("空のシート");
    document
        .set_sheet_columns(empty, vec!["a".to_owned(), "b".to_owned()])
        .expect("標本のシートは実在する");
    document
        .set_root_schema(empty, SchemaPart::parse(SCHEMA_EMPTY).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    document
}

/// 1 シート `count` 行（列 0 個）の文書を作る。
///
/// 大量の行は `add_row`（O(1) の一括経路）で積む。`set_row_values` は行ごとに線形探索
/// するため O(n²) であり、10 万行では分単位になる（タスク 4.8 の性能の罠）。
fn document_with_rows(count: usize) -> Document {
    let mut document = Document::new();
    let sheet = document.add_sheet("大量");
    for _ in 0..count {
        document.add_row(sheet).expect("標本のシートは実在する");
    }
    document
}

/// モデル → ファイル → `open` で同一のモデルが戻る（要件 4.1, 5.2）。
#[test]
fn open_round_trips_the_model_through_a_real_file() {
    let scratch = Scratch::new("roundtrip");
    let before = sample();
    let path = write_document(&scratch, "sample.jxcel", &before);

    let outcome = api().open(&path).expect("標本のファイルは開ける");

    assert_eq!(before.document_id(), outcome.document.document_id(), "識別子が変わった");
    assert_eq!(sheet_metadata(&before), sheet_metadata(&outcome.document), "シートが変わった");
    assert_eq!(rows(&before), rows(&outcome.document), "行が変わった");
    assert_eq!(schemas(&before), schemas(&outcome.document), "スキーマが変わった");
    assert_eq!(attachments(&before), attachments(&outcome.document), "添付が変わった");
    assert_eq!(None, outcome.migrated_from, "現行版なのに移行元が記録された");
    assert!(!outcome.beyond_supported_scale, "小さい文書が保証対象外とされた");
}

/// 決定性ゴールデン fixture を読み込み経路の入力にできる（要件 4.1）。
#[test]
fn open_reads_the_committed_golden_fixture() {
    let outcome = api().open(&fixture_path()).expect("ゴールデンは開ける");

    assert_eq!(1, outcome.document.sheets().len(), "ゴールデンのシート数が違う");
    let sheet = &outcome.document.sheets()[0];
    assert_eq!("標本シート", sheet.name(), "ゴールデンのシート名が違う");
    assert_eq!(3, sheet.columns().len(), "ゴールデンの列数が違う");
    assert_eq!(40, sheet.rows().len(), "ゴールデンの行数が違う");
    assert_eq!(1, attachments(&outcome.document).len(), "ゴールデンの添付数が違う");
    assert_eq!(None, outcome.migrated_from, "現行版なのに移行元が記録された");
    assert!(!outcome.beyond_supported_scale, "ゴールデンが保証対象外とされた");
}

/// 存在しない / 壊れた / jxcel でないファイルは `Err` になり、ファイルは変更されない。
///
/// 「jxcel でないファイル」は ZIP ですらないバイト列で代表させる（許可リストに載らない
/// 名前を持つ ZIP を作るには、本クレートの公開面の外に ZIP 書き出しが必要になる。
/// 許可リスト照合そのものの観測は `container/` の単体テストが担う）。
#[test]
fn open_rejects_absent_corrupt_and_foreign_files_without_changing_them() {
    let scratch = Scratch::new("errors");

    let corrupt = scratch.file("corrupt.jxcel");
    fs::write(&corrupt, b"this is not a zip container at all").expect("書き出し");

    let truncated = scratch.file("truncated.jxcel");
    let golden = fs::read(fixture_path()).expect("ゴールデンが読める");
    fs::write(&truncated, &golden[..golden.len() / 2]).expect("書き出し");

    let absent = scratch.file("absent.jxcel");

    // 判定順の観測（不在は Io、壊れた ZIP は InvalidContainer）。
    let missing = api().open(&absent).expect_err("存在しないファイルは開けない");
    assert!(
        matches!(missing, DocumentError::Io { retried: false, .. }),
        "不在が Io(retried=false) でない: {missing:?}"
    );
    for path in [&corrupt, &truncated] {
        let error = api().open(path).expect_err("壊れたファイルは開けない");
        assert!(
            matches!(error, DocumentError::InvalidContainer { .. }),
            "{} が InvalidContainer でない: {error:?}",
            path.display()
        );
    }

    // 失敗した読み込みはファイルを変更しない（要件 5.5。ディレクトリ全体で見る）。
    let expected: Vec<(String, Vec<u8>)> = vec![
        ("corrupt.jxcel".to_owned(), fs::read(&corrupt).expect("読める")),
        ("truncated.jxcel".to_owned(), fs::read(&truncated).expect("読める")),
    ];
    assert_eq!(expected, snapshot(scratch.path()), "失敗した読み込みがファイルを変更した");
    assert!(!absent.exists(), "存在しないパスが読み込みで作られた");
}

/// 読み込み経路はいかなる書き込みも行わない（要件 5.5）。
///
/// 成功・失敗・不在の 3 通りを連続して走らせ、対象ファイルのバイト列と作業ディレクトリの
/// 内容（名前とバイト列）が呼び出しの前後で完全に一致することを実測する。作業ディレクトリ
/// には一時ファイルを置き忘れない設計であることを、名前の一覧でも確かめる。
#[test]
fn open_never_writes_anything_to_the_filesystem() {
    let scratch = Scratch::new("readonly");
    let valid = write_document(&scratch, "valid.jxcel", &sample());
    let corrupt = scratch.file("corrupt.jxcel");
    fs::write(&corrupt, b"not a container").expect("書き出し");
    let absent = scratch.file("absent.jxcel");

    let before = snapshot(scratch.path());
    let valid_before = fs::read(&valid).expect("読める");
    let corrupt_before = fs::read(&corrupt).expect("読める");

    api().open(&valid).expect("妥当なファイルは開ける");
    api().open(&corrupt).expect_err("壊れたファイルは開けない");
    api().open(&absent).expect_err("存在しないファイルは開けない");

    assert_eq!(valid_before, fs::read(&valid).expect("読める"), "対象ファイルが変更された");
    assert_eq!(
        corrupt_before,
        fs::read(&corrupt).expect("読める"),
        "壊れた対象ファイルが変更された"
    );
    assert_eq!(before, snapshot(scratch.path()), "読み込みが一時ファイルを残した");
}

/// 後段で失敗する入力は `Err` になり、部分的なモデルとして外に出ない（要件 5.4）。
///
/// ダイジェスト照合（読む前段の検証）と構造検証（モデル構築の直前）のそれぞれで
/// 失敗する入力を作り、`open` の戻り値として区別できることを示す。
#[test]
fn open_reports_late_stage_failures_without_returning_a_partial_model() {
    let scratch = Scratch::new("late");

    // (a) ダイジェスト照合の失敗: 実体だけを差し替え、索引の記録を古いまま残す。
    let parts = ContainerCodec::decode(&fs::read(fixture_path()).expect("読める"))
        .expect("ゴールデンは復号できる");
    let mut tampered = entries_of(&parts);
    let row_entry = tampered
        .iter()
        .position(|(name, _)| matches!(name, EntryName::Rows { .. }))
        .expect("ゴールデンは行エントリを持つ");
    tampered[row_entry].1.extend_from_slice(b" ");
    let digest_case = scratch.file("digest.jxcel");
    fs::write(&digest_case, encode_entries(tampered)).expect("書き出し");

    let error = api().open(&digest_case).expect_err("ダイジェスト不一致は失敗する");
    assert!(
        matches!(error, DocumentError::IntegrityMismatch { .. }),
        "ダイジェスト不一致が IntegrityMismatch でない: {error:?}"
    );

    // (b) 構造検証の失敗: ダイジェストは合わせたまま、実在しない型定義を参照させる。
    let sheet = parts
        .iter()
        .find_map(|part| match part.name {
            EntryName::Schema { sheet } => Some(sheet),
            _ => None,
        })
        .expect("ゴールデンはスキーマエントリを持つ");
    let dangling = SchemaPart::parse(concat!(
        r#"{"root":{"$ref":""#,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        r#""},"types":[]}"#
    ))
    .expect("標本のスキーマは妥当");
    let (schema_entry, schema_bytes) =
        SchemaCodec::encode(sheet, &dangling).expect("符号化");
    let mut structural = entries_of(&parts);
    let slot = structural
        .iter()
        .position(|(name, _)| *name == schema_entry)
        .expect("スキーマエントリが実在する");
    structural[slot].1 = schema_bytes;
    let structural_case = scratch.file("structural.jxcel");
    fs::write(&structural_case, rebuilt_with_manifest(parts.format_version(), structural))
        .expect("書き出し");

    let error = api().open(&structural_case).expect_err("構造の違反は失敗する");
    assert!(
        matches!(error, DocumentError::DanglingTypeRef { .. }),
        "宙吊り参照が DanglingTypeRef でない: {error:?}"
    );

    // 失敗した 2 つのファイルはどちらも変更されていない（要件 5.5）。
    let expected: Vec<(String, Vec<u8>)> = vec![
        ("digest.jxcel".to_owned(), fs::read(&digest_case).expect("読める")),
        ("structural.jxcel".to_owned(), fs::read(&structural_case).expect("読める")),
    ];
    assert_eq!(expected, snapshot(scratch.path()), "失敗した読み込みがファイルを変更した");
}

/// 記録された形式バージョンのゲートが読み込み経路に結線されている（要件 6.2, 6.5）。
///
/// 現行版はそのまま読め、現行より新しい major は中止される。現行より古い major は
/// 移行チェーンが現行へ運ぶ対象であるが、**初版のチェーンは空**であるため移行先が無く
/// 中止される（`open` からは「古い版が拒否される」ことだけが観測できる。移行が成功した
/// 場合の `migrated_from = Some(..)` の観測は、段を持つ移行チェーンが入るまで不能である）。
#[test]
fn open_gates_the_recorded_format_version() {
    let scratch = Scratch::new("version");
    let parts = ContainerCodec::decode(&fs::read(fixture_path()).expect("読める"))
        .expect("ゴールデンは復号できる");
    let current = parts.format_version();

    // 現行版: そのまま読め、移行元は記録されない。
    let current_path = scratch.file("current.jxcel");
    fs::write(&current_path, rebuilt_with_manifest(current, entries_of(&parts))).expect("書き出し");
    let outcome = api().open(&current_path).expect("現行版は読める");
    assert_eq!(None, outcome.migrated_from, "現行版なのに移行元が記録された");

    // 現行より新しい major: 中止（要件 6.5）。
    let future = FormatVersion::new(current.major + 1, 0);
    let future_path = scratch.file("future.jxcel");
    fs::write(&future_path, rebuilt_with_manifest(future, entries_of(&parts))).expect("書き出し");
    let error = api().open(&future_path).expect_err("未来の major は開けない");
    assert!(
        matches!(error, DocumentError::UnsupportedVersion { found, supported }
            if found == future && supported == current),
        "未来版の拒否が UnsupportedVersion でない: {error:?}"
    );

    // 現行より古い major: 空のチェーンには移行先が無いため中止（要件 6.2, 6.3）。
    let past = FormatVersion::new(0, 9);
    let past_path = scratch.file("past.jxcel");
    fs::write(&past_path, rebuilt_with_manifest(past, entries_of(&parts))).expect("書き出し");
    let error = api().open(&past_path).expect_err("移行先の無い古い版は開けない");
    assert!(
        matches!(error, DocumentError::UnsupportedVersion { found, supported }
            if found == past && supported == current),
        "古い版の拒否が UnsupportedVersion でない: {error:?}"
    );
}

/// 保証対象は 1 ドキュメントあたり 100,000 行である（要件 8.4）。
///
/// 境界をリテラルで扱うテスト（下記）とは別に、公開定数そのものを 1 本で固定する。
#[test]
fn supported_row_limit_is_one_hundred_thousand() {
    assert_eq!(100_000, SUPPORTED_ROW_LIMIT, "保証対象の行数が要件 8.4 と違う");
}

/// 10 万行を超えるドキュメントを拒否せず、保証対象外として通知する（要件 8.4, 8.5）。
///
/// 境界は 100,000 行（対象内）と 100,001 行（超過）でリテラルの実データとして作る。
/// 行は O(1) の `add_row` で積み、`Row` の値は空・列は 0 個にして、行数の観測だけを
/// 最小の費用で行う（100 万セル級の実測はタスク 8.9 のベンチが担う）。
#[test]
fn open_flags_documents_beyond_the_supported_row_limit_without_rejecting_them() {
    let scratch = Scratch::new("scale");

    let at_limit = write_document(&scratch, "at_limit.jxcel", &document_with_rows(100_000));
    let outcome = api().open(&at_limit).expect("100,000 行は開ける");
    assert_eq!(
        100_000,
        outcome.document.sheets()[0].rows().len(),
        "行数が往復で変わった"
    );
    assert!(!outcome.beyond_supported_scale, "100,000 行が保証対象外とされた");

    let over_limit = write_document(&scratch, "over_limit.jxcel", &document_with_rows(100_001));
    let outcome = api().open(&over_limit).expect("超過しても拒否しない（要件 8.5）");
    assert_eq!(
        100_001,
        outcome.document.sheets()[0].rows().len(),
        "行数が往復で変わった"
    );
    assert!(outcome.beyond_supported_scale, "100,001 行が保証対象内とされた");
}

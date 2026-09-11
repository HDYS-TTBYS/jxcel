//! 設定ストアの原子性・永続化・共有・直列化（要件 7.1、7.2、7.3。tasks.md 4.1）。
//!
//! 固定するのは次の 6 つである:
//!
//! 1. **完了状態（tasks.md 4.1）**: 書き込みの途中でプロセスを強制終了しても、対象ファイルには
//!    直前の完全な内容か新しい完全な内容のいずれかが残る（部分的な内容は残らない）
//! 2. `set` した値が `get` で戻り、ディスクから**開き直した**実体からも戻る（要件 7.1）
//! 3. 成功した書き込みは一時ファイルを残さず、対象は完全に置き換わる（要件 7.1）
//! 4. 同じディレクトリの `open` は同一の実体を返し、片方の書き込みがもう片方に見える（要件 7.3）
//! 5. 複数スレッドからの並行 `set` が失われない（書き込みの直列化）
//! 6. OS 標準のアプリケーションデータ領域の解決が各 OS の規約に従い、環境変数が無ければ
//!    Err を返す（パニックしない）。識別子は `src-tauri/tauri.conf.json` と一致する（要件 7.2）
//!
//! # 強制終了による原子性の検証
//!
//! 原子性は**実プロセスを落として**確かめる。手法は
//! `crates/document-format/tests/atomic_save.rs` と同じである:
//!
//! 1. 親テスト（本ファイルの通常テスト）が、対象を完全な内容（状態 A）で用意する
//! 2. 親は [`crash_writer`]（`#[ignore]`。通常の `cargo test` では走らない）を
//!    `current_exe()` + `--ignored --exact` で起動する。ワーカーは公開 API の `set` を
//!    呼び続け、状態 A と状態 B を交互に書く（どちらもバイト長が同じ完全な JSON）
//! 3. 親は「一時ファイルが現れた」または「対象の長さが完全な長さと違う」のどちらかを観測した
//!    瞬間に `Child::kill()` で落とす（Unix は `SIGKILL`、Windows は `TerminateProcess`）。
//!    タイムアウトした場合も落とし、観測結果を記録する
//! 4. 落とした直後に、対象が完全な JSON として読め、状態 A か状態 B のどちらかに**完全に**
//!    一致することを確かめる。部分的な内容・長さの不一致・キーの欠落は失敗である
//!
//! 正確な瞬間は狙えないため複数回試行し、「一時ファイルを観測して落とした試行が 1 回以上ある」
//! ことも要求する。これは、強制終了が実際に書き込みの窓へ入ったことの積極的な証拠である
//! （truncate-then-write の実装では一時ファイルが決して現れないため、この要求だけで落ちる。
//! 実測は下のテストの docs を参照）。
//!
//! ワーカーは [`WriterGuard`] の `Drop` が必ず強制終了して回収する。アサーションが失敗して
//! panic した経路でも孤児を残さない。待ちはすべて期限付きで、固定の sleep に正しさを依存しない。
//! 一時ディレクトリは [`Scratch`]（`Drop` で削除）に閉じるため、開発者の実の `HOME` には
//! 依存しない。

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use app_shell::settings::atomic::TEMP_FILE_PREFIX;
use app_shell::settings::{
    app_data_base_dir, app_data_base_dir_with, app_data_dir, open, OpenReport, SettingsError,
    SettingsKey, SettingsStore, APP_IDENTIFIER, SETTINGS_FILE_NAME,
};

// ---------------------------------------------------------------------------
// テストが使うキー
// ---------------------------------------------------------------------------

const TEST_KEY: SettingsKey = SettingsKey::new("test.value");
const COUNT_KEY: SettingsKey = SettingsKey::new("test.count");
const OBJECT_KEY: SettingsKey = SettingsKey::new("test.object");
const MISSING_KEY: SettingsKey = SettingsKey::new("test.missing");
const BLOCKER_KEY: SettingsKey = SettingsKey::new("test.blocker");

/// 並行書き込みテストでスレッドごとに固定するキー。文字列リテラルから作る（`SettingsKey` は
/// 安定した文字列を要求するため、実行時に組み立てた名前は使わない）。
const CONCURRENT_THREADS: usize = 8;
const CONCURRENT_WRITES_PER_THREAD: usize = 64;
const CONCURRENT_KEYS: [SettingsKey; CONCURRENT_THREADS] = [
    SettingsKey::new("concurrent.0"),
    SettingsKey::new("concurrent.1"),
    SettingsKey::new("concurrent.2"),
    SettingsKey::new("concurrent.3"),
    SettingsKey::new("concurrent.4"),
    SettingsKey::new("concurrent.5"),
    SettingsKey::new("concurrent.6"),
    SettingsKey::new("concurrent.7"),
];

// ---------------------------------------------------------------------------
// 一時ディレクトリと小さな道具
// ---------------------------------------------------------------------------

/// テスト中だけ使うディレクトリ。`Drop` で削除する（失敗経路でも残さない）。
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Self {
        static SEQUENCE: AtomicU32 = AtomicU32::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "jxcel-settings-{tag}-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn target(&self) -> PathBuf {
        self.path.join(SETTINGS_FILE_NAME)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // 既に消えていても失敗しない（多重削除・異常終了の後始末）。
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// ディレクトリ直下のエントリ名を昇順で返す。
fn entry_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("一時ディレクトリを読める")
        .map(|entry| entry.expect("エントリを読める").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// 一時ファイル（[`TEMP_FILE_PREFIX`] で始まる名前）だけを昇順で返す。
fn temp_file_names(directory: &Path) -> Vec<String> {
    entry_names(directory)
        .into_iter()
        .filter(|name| name.starts_with(TEMP_FILE_PREFIX))
        .collect()
}

/// 設定ファイル以外がディレクトリに残っていないことを確かめる。
fn assert_only_settings_file(directory: &Path) {
    assert_eq!(
        entry_names(directory),
        vec![SETTINGS_FILE_NAME.to_owned()],
        "一時ファイルが残っている"
    );
}

// ---------------------------------------------------------------------------
// 要件 7.1: 往復と開き直し
// ---------------------------------------------------------------------------

/// `set` した値が同じ実体の `get` で戻り、**ディスクから開き直した**実体でも戻る（要件 7.1）。
///
/// 型付きの境界（文字列・整数・入れ物）を 1 度ずつ通す。`get` は存在しないキーと型の合わない
/// 値に対して `None` を返す（design の `get` は失敗を返す経路を持たない）。
#[test]
fn set_then_get_round_trips_across_a_fresh_open() {
    let scratch = Scratch::new("round-trip");
    let (store, report) = open(scratch.path()).expect("設定ストアを開ける");
    assert_eq!(report, OpenReport::default(), "4.1 の open は復旧の事実を返さない");

    store.set(&TEST_KEY, &"文字列").expect("書ける");
    store.set(&COUNT_KEY, &7u32).expect("書ける");
    store
        .set(&OBJECT_KEY, &serde_json::json!({"a": [1, 2, 3]}))
        .expect("書ける");

    assert_eq!(store.get::<String>(&TEST_KEY).as_deref(), Some("文字列"));
    assert_eq!(store.get::<u32>(&COUNT_KEY), Some(7));
    assert_eq!(
        store.get::<serde_json::Value>(&OBJECT_KEY),
        Some(serde_json::json!({"a": [1, 2, 3]}))
    );
    assert_eq!(store.get::<u32>(&MISSING_KEY), None, "存在しないキーは None");
    assert_eq!(store.get::<u32>(&TEST_KEY), None, "型の合わない値は None");

    // 実体を手放してから開き直す。共有の登録簿が空になるため、必ずディスクから読む。
    drop(store);
    let (fresh, _) = open(scratch.path()).expect("開き直せる");
    assert_eq!(fresh.get::<String>(&TEST_KEY).as_deref(), Some("文字列"));
    assert_eq!(fresh.get::<u32>(&COUNT_KEY), Some(7));
    assert_eq!(
        fresh.get::<serde_json::Value>(&OBJECT_KEY),
        Some(serde_json::json!({"a": [1, 2, 3]}))
    );
}

/// 保存先ディレクトリが無ければ作り、設定ファイルはそこへ置かれる（要件 7.2 の前提）。
#[test]
fn open_creates_a_missing_directory() {
    let scratch = Scratch::new("missing-dir");
    let nested = scratch.path().join("a").join("b");

    let (store, _) = open(&nested).expect("無いディレクトリを作って開ける");
    store.set(&TEST_KEY, &"value").expect("書ける");

    assert!(nested.join(SETTINGS_FILE_NAME).is_file(), "設定ファイルが作られていない");
}

/// ディレクトリを用意できないときはパニックせず、原因を区別できるエラーを返す。
#[test]
fn open_reports_an_unusable_directory() {
    let scratch = Scratch::new("unusable-dir");
    // ファイルの下はディレクトリにできない。
    fs::write(scratch.path().join("blocker"), b"not a directory").expect("塞げる");

    let error = open(&scratch.path().join("blocker").join("sub"))
        .err()
        .expect("開けないはず");
    assert!(
        matches!(error, SettingsError::DirectoryUnavailable { .. }),
        "原因を区別できるエラーでない: {error:?}"
    );
}

// ---------------------------------------------------------------------------
// 要件 7.1: 一時ファイルを残さない置き換え
// ---------------------------------------------------------------------------

/// 成功した書き込みは対象を新しい内容へ完全に置き換え、一時ファイルを残さない。
///
/// 2 回続けて書いても一時ファイルは残らず、対象は最後の内容だけを含む。実装が
/// truncate-then-write であってもこのテストは通るため、原子性は下の強制終了テストが担う。
#[test]
fn successful_write_leaves_no_temporary_file() {
    let scratch = Scratch::new("replace");
    let (store, _) = open(scratch.path()).expect("設定ストアを開ける");

    store.set(&TEST_KEY, &"first").expect("書ける");
    assert_only_settings_file(scratch.path());

    store.set(&TEST_KEY, &"second").expect("書ける");
    assert_only_settings_file(scratch.path());

    let bytes = fs::read(scratch.target()).expect("対象を読める");
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).expect("完全な JSON である");
    assert_eq!(parsed[TEST_KEY.as_str()], serde_json::json!("second"));
    assert_eq!(parsed.as_object().expect("オブジェクトである").len(), 1);
}

/// 書き込みに失敗したときはエラーを返し、一時ファイルを残さず、メモリ側も元へ戻す。
///
/// 対象パスをディレクトリで塞ぐと置換（`rename`）が失敗する。このとき `Err` が返り、
/// `get` は書き込めなかった新しい値を返してはならない（ファイルとメモリが食い違わない）。
#[test]
fn failed_write_removes_the_temporary_file_and_rolls_back() {
    let scratch = Scratch::new("failed-write");
    let (store, _) = open(scratch.path()).expect("設定ストアを開ける");

    // メモリには値を持たせない。対象パスをディレクトリで塞いでから書く。
    fs::create_dir(scratch.target()).expect("対象パスを塞げる");

    let error = store.set(&BLOCKER_KEY, &"value").err().expect("置換は失敗するはず");
    assert!(
        matches!(error, SettingsError::WriteFailed { .. }),
        "書き込みの失敗として返らない: {error:?}"
    );
    assert!(temp_file_names(scratch.path()).is_empty(), "失敗した書き込みが一時ファイルを残した");
    assert_eq!(
        store.get::<String>(&BLOCKER_KEY),
        None,
        "失敗した書き込みがメモリに残った"
    );
}

// ---------------------------------------------------------------------------
// 要件 7.3: 同一ディレクトリの共有実体
// ---------------------------------------------------------------------------

/// 同じディレクトリに対する 2 回の `open` は同一の実体を返し、書き込みが互いに見える
/// （要件 7.3。全ウィンドウが 1 つの値を参照する）。
///
/// `./` を挟んだ別綴りのパスでも同じ実体になることまで確かめる（正規化したパスで共有する）。
#[test]
fn open_shares_one_instance_per_directory() {
    let scratch = Scratch::new("sharing");
    let (first, _) = open(scratch.path()).expect("1 回目を開ける");
    let (second, _) = open(scratch.path()).expect("2 回目を開ける");
    assert!(Arc::ptr_eq(&first, &second), "同じディレクトリで別の実体が返った");

    let dotted = scratch.path().join(".");
    let (third, _) = open(&dotted).expect("別綴りで開ける");
    assert!(Arc::ptr_eq(&first, &third), "同じディレクトリの別綴りで別の実体が返った");

    first.set(&TEST_KEY, &"from-first").expect("書ける");
    assert_eq!(
        second.get::<String>(&TEST_KEY).as_deref(),
        Some("from-first"),
        "片方の書き込みがもう片方に見えない"
    );

    second.set(&TEST_KEY, &"from-second").expect("書ける");
    assert_eq!(first.get::<String>(&TEST_KEY).as_deref(), Some("from-second"));
}

// ---------------------------------------------------------------------------
// 書き込みの直列化: 失われない
// ---------------------------------------------------------------------------

/// 複数スレッドがそれぞれ別のキーへ同時に書いても、すべての書き込みがディスクに残る
/// （書き込みの直列化。tasks.md 4.1「書き込みを直列化する」）。
///
/// 各スレッドは自分のキーだけを繰り返し書き、最後の値が残ることを確かめる。直列化が無いと、
/// あるスレッドの書き込みが、そのスレッドのキーを含まない古い写しで上書きされて失われる。
///
/// **実測した負荷証明**: `set` の読み → 変更 → 書きの全体を直列化しない変異（各書き込みを
/// ディスクの独立した写しから作る）にすると、このテストは落ちる — 実測ではキー
/// `concurrent.0` がファイルから丸ごと失われた。なお「いったん写しを取ってからロックを外して
/// 書く」だけの変異では、キーごとの待ちを入れない限り上書きの逆転が起きにくく、このテストは
/// 緑のままだった（負荷の与え方に依存する）。
#[test]
fn concurrent_writes_do_not_lose_updates() {
    let scratch = Scratch::new("concurrent");
    let (store, _) = open(scratch.path()).expect("設定ストアを開ける");
    let barrier = Arc::new(Barrier::new(CONCURRENT_THREADS));

    let mut handles = Vec::new();
    for thread_index in 0..CONCURRENT_THREADS {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        let key = CONCURRENT_KEYS[thread_index];
        handles.push(thread::spawn(move || {
            barrier.wait();
            for write_index in 0..CONCURRENT_WRITES_PER_THREAD {
                let value = format!("{thread_index}:{write_index}");
                store.set(&key, &value).expect("並行しても書き込める");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("スレッドが完走する");
    }

    // 同じ実体から見える。
    for thread_index in 0..CONCURRENT_THREADS {
        let expected = format!("{thread_index}:{}", CONCURRENT_WRITES_PER_THREAD - 1);
        assert_eq!(
            store.get::<String>(&CONCURRENT_KEYS[thread_index]).as_deref(),
            Some(expected.as_str()),
            "スレッド {thread_index} の書き込みが見えない"
        );
    }

    // ディスクから開き直しても見える（書き込みは `set` の戻りまでに永続化されている）。
    drop(store);
    let (fresh, _) = open(scratch.path()).expect("開き直せる");
    for thread_index in 0..CONCURRENT_THREADS {
        let expected = format!("{thread_index}:{}", CONCURRENT_WRITES_PER_THREAD - 1);
        assert_eq!(
            fresh.get::<String>(&CONCURRENT_KEYS[thread_index]).as_deref(),
            Some(expected.as_str()),
            "スレッド {thread_index} の書き込みがディスクに残っていない（失われた更新）"
        );
    }
}

// ---------------------------------------------------------------------------
// 要件 7.2: OS 標準のアプリケーションデータ領域
// ---------------------------------------------------------------------------

/// アプリケーション識別子は `src-tauri/tauri.conf.json` の `identifier` と一致する。
///
/// 一致は実行時の正しさそのものである: アダプタが設定の保存先を別に解決すれば、同じ
/// アプリケーションが 2 つのディレクトリを読み書きする。定数の綴りを固定する。
#[test]
fn app_identifier_matches_the_tauri_identifier() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest.ancestors().nth(2).expect("リポジトリルートを解決できる");
    let conf = fs::read_to_string(repo_root.join("src-tauri").join("tauri.conf.json"))
        .expect("tauri.conf.json を読める");
    let parsed: serde_json::Value = serde_json::from_str(&conf).expect("JSON として読める");
    assert_eq!(
        parsed["identifier"].as_str(),
        Some(APP_IDENTIFIER),
        "識別子が tauri.conf.json と食い違っている（別のディレクトリを読むバグになる）"
    );
}

/// 解決した保存先は OS 標準の領域の下に識別子を足したものである（要件 7.2）。
#[test]
fn app_data_dir_appends_the_identifier_to_the_platform_base() {
    let base = app_data_base_dir().expect("このホストには OS 標準の環境変数がある");
    assert_eq!(
        app_data_dir().expect("解決できる"),
        base.join(APP_IDENTIFIER),
        "識別子が付いていない"
    );
    assert!(
        app_data_dir().expect("解決できる").ends_with(APP_IDENTIFIER),
        "保存先が識別子で終わらない"
    );
}

/// 環境変数が無い（または空の）ときは、パニックせず Err を返す（要件 7.2）。
///
/// 3 OS のいずれでも、すべての環境変数が未設定なら解決できない。空の値は未設定と同じに扱う。
#[test]
fn app_data_base_reports_a_missing_environment() {
    let missing = app_data_base_dir_with(&|_name: &str| None);
    assert!(missing.is_err(), "環境変数が無いのに解決できた: {missing:?}");

    let empty = app_data_base_dir_with(&|_name: &str| Some(OsString::new()));
    assert!(empty.is_err(), "空の値を設定済みとして扱った: {empty:?}");
}

/// Linux は `$XDG_DATA_HOME`、無ければ `$HOME/.local/share` を使う。
#[cfg(target_os = "linux")]
#[test]
fn linux_base_prefers_xdg_data_home_then_home() {
    let xdg = PathBuf::from("/xdg/data");
    let home = PathBuf::from("/home/user");

    let with_xdg = {
        let xdg = xdg.clone();
        let home = home.clone();
        app_data_base_dir_with(&move |name: &str| match name {
            "XDG_DATA_HOME" => Some(OsString::from(xdg.clone())),
            "HOME" => Some(OsString::from(home.clone())),
            _ => None,
        })
        .expect("XDG_DATA_HOME がある")
    };
    assert_eq!(with_xdg, xdg, "XDG_DATA_HOME が優先されていない");

    let without_xdg = {
        let home = home.clone();
        app_data_base_dir_with(&move |name: &str| match name {
            "HOME" => Some(OsString::from(home.clone())),
            _ => None,
        })
        .expect("HOME がある")
    };
    assert_eq!(
        without_xdg,
        home.join(".local").join("share"),
        "HOME からの既定の場所と違う"
    );

    let empty_xdg = {
        let home = home.clone();
        app_data_base_dir_with(&move |name: &str| match name {
            "XDG_DATA_HOME" => Some(OsString::new()),
            "HOME" => Some(OsString::from(home.clone())),
            _ => None,
        })
        .expect("空の XDG は未設定として HOME を使う")
    };
    assert_eq!(empty_xdg, home.join(".local").join("share"), "空の XDG を設定済みとして扱った");
}

/// macOS は `$HOME/Library/Application Support` を使う。
#[cfg(target_os = "macos")]
#[test]
fn macos_base_uses_home_library_application_support() {
    let resolved = app_data_base_dir_with(&|name: &str| match name {
        "HOME" => Some(OsString::from("/Users/user")),
        _ => None,
    })
    .expect("HOME がある");
    assert_eq!(
        resolved,
        PathBuf::from("/Users/user").join("Library").join("Application Support")
    );
}

/// Windows はローミングの `%APPDATA%` を使う。
#[cfg(windows)]
#[test]
fn windows_base_uses_roaming_appdata() {
    let resolved = app_data_base_dir_with(&|name: &str| match name {
        "APPDATA" => Some(OsString::from(r"C:\Users\user\AppData\Roaming")),
        _ => None,
    })
    .expect("APPDATA がある");
    assert_eq!(resolved, PathBuf::from(r"C:\Users\user\AppData\Roaming"));
}

// ---------------------------------------------------------------------------
// 完了状態: 書き込みの途中で落としても完全な内容が残る
// ---------------------------------------------------------------------------

/// 落とされる側のワーカーへ渡す環境変数。値の**厳密一致**で子モードを判定する
/// （外部環境に同名の変数があっても親の検証が空振りしないようにする）。
const CRASH_MODE_ENV: &str = "JXCEL_SETTINGS_CRASH_MODE";
const CRASH_DIR_ENV: &str = "JXCEL_SETTINGS_CRASH_DIR";
const CRASH_MODE: &str = "settings-crash-writer";

/// ワーカーが交互に書く値の大きさ。`write_all` → `sync_all` の窓を親が観測できる程度に
/// 大きく取る（小さすぎると窓がポーリング間隔より短くなる）。
const CRASH_VALUE_BYTES: usize = 4 * 1024 * 1024;

/// 試行回数。1 回でも「書き込みの窓で落ちた」観測が得られればよい。
const CRASH_ATTEMPTS: usize = 6;

/// ワーカーが「開き終えて書き込みを始める」ことを知らせるファイル名。
const READY_FILE_NAME: &str = "writer-ready";

/// ワーカーの起動（`open` と準備完了の書き出し）を待つ上限。
const CRASH_READY_TIMEOUT: Duration = Duration::from_secs(60);

/// 書き込みの窓の観測を待つ上限。超えたら「観測できなかった」として落とす。
const CRASH_SIGHTING_TIMEOUT: Duration = Duration::from_secs(10);

/// ワーカーのキー。親子が同じ定数から作る。
const CRASH_KEY: SettingsKey = SettingsKey::new("crash.payload");

/// 長さ `bytes` の、`marker` だけからなる値（状態 A と状態 B で長さを揃える）。
fn crash_state(marker: char, bytes: usize) -> String {
    std::iter::repeat(marker).take(bytes).collect()
}

/// 状態 `marker` だけを持つ設定ファイルのバイト列（親子が同じ関数から作る）。
fn crash_document(marker: char) -> Vec<u8> {
    let mut values = serde_json::Map::new();
    values.insert(
        CRASH_KEY.as_str().to_owned(),
        serde_json::Value::String(crash_state(marker, CRASH_VALUE_BYTES)),
    );
    serde_json::to_vec(&values).expect("直列化できる")
}

/// 落とされる側のワーカー。
///
/// **通常の `cargo test` では実行されない**（`#[ignore]`）。親テストだけが
/// `current_exe()` を `--ignored --exact crash_writer` で起動する。公開 API の `set` を
/// 呼び続け、状態 A と状態 B を交互に書く。親が落とさなければ永遠に書き続ける。
#[ignore = "親テストが current_exe 経由で強制終了する専用のワーカー"]
#[test]
fn crash_writer() {
    if std::env::var(CRASH_MODE_ENV).as_deref() != Ok(CRASH_MODE) {
        // `--ignored` の一括実行など、親以外からの起動では何もしない。
        return;
    }
    let directory = PathBuf::from(
        std::env::var(CRASH_DIR_ENV).unwrap_or_else(|error| panic!("{CRASH_DIR_ENV} が無い: {error}")),
    );
    assert!(directory.is_absolute(), "作業ディレクトリは絶対パスでなければならない");

    let (store, _) = open(&directory).expect("設定ストアを開ける");
    // 親に「書き込みを始められる」ことを知らせる。親はここから書き込みの窓を観測する。
    fs::write(directory.join(READY_FILE_NAME), b"").expect("準備完了を書ける");

    let mut marker = 'A';
    loop {
        store
            .set(&CRASH_KEY, &crash_state(marker, CRASH_VALUE_BYTES))
            .expect("書き込める");
        marker = if marker == 'A' { 'B' } else { 'A' };
    }
}

/// ワーカーを必ず強制終了して回収するガード。`Drop` で走るため、panic した経路でも
/// 孤児を残さない。
struct WriterGuard {
    child: Option<Child>,
}

impl WriterGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("ワーカーは生きている")
    }

    fn kill_and_wait(&mut self) -> ExitStatus {
        let mut child = self.child.take().expect("ワーカーは生きている");
        let _ = child.kill();
        child.wait().expect("ワーカーを回収できる")
    }
}

impl Drop for WriterGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// 1 回の試行で観測したこと。
#[derive(Debug)]
struct Attempt {
    /// 強制終了の直前に一時ファイルを観測したか（書き込みの窓に入っていたことの証拠）。
    temp_seen: bool,
    /// 落とす判断の根拠（`temp-file` / `target-size` / `timeout`）。
    reason: &'static str,
    /// 落とした時点の対象の長さ。読めなければ `None`。
    target_len: Option<u64>,
    /// 親が落とす前にワーカーが自力で終了していたか（実装の不具合の兆候）。
    child_exited_early: bool,
    /// 落とした直後の対象がどちらの完全な状態だったか（`A` = 直前の内容、`B` = 新しい内容）。
    observed_state: Option<char>,
}

/// 対象の内容が状態 A か状態 B のどちらかに**完全に**一致することを確かめ、どちらだったかを返す。
fn assert_complete_state(target: &Path, context: &str) -> char {
    let bytes = fs::read(target).unwrap_or_else(|error| panic!("{context}: 対象を読めない: {error}"));
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!("{context}: 対象が完全な JSON でない（{} バイト）: {error}", bytes.len())
    });
    let value = parsed
        .get(CRASH_KEY.as_str())
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("{context}: キーが無い"));
    assert_eq!(value.len(), CRASH_VALUE_BYTES, "{context}: 内容の長さが違う");
    let first = value.chars().next().expect("空でない");
    assert!(first == 'A' || first == 'B', "{context}: 未知の内容");
    assert!(value.chars().all(|character| character == first), "{context}: 内容が混ざっている");
    assert_eq!(
        bytes.len(),
        crash_document('A').len(),
        "{context}: ファイル長が完全な内容と違う"
    );
    first
}

/// ワーカーが準備完了を書くまで待つ（期限付き）。
fn wait_for_ready(ready: &Path) {
    let deadline = Instant::now() + CRASH_READY_TIMEOUT;
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "ワーカーが準備完了を書かなかった: {}",
            ready.display()
        );
        thread::sleep(Duration::from_millis(1));
    }
}

/// 書き込みの窓を観測してワーカーを落とし、観測結果を返す。
///
/// 「一時ファイルが現れた」（正しい実装）か「対象の長さが完全な長さと違う」（truncate-then-write
/// の実装）のどちらかを観測した瞬間に落とす。どちらも観測できないまま期限を超えた場合も落とし、
/// `reason` に `timeout` を残す（呼び出し側が「窓に入った試行」の有無を判定する）。
fn observe_and_kill(
    writer: &mut WriterGuard,
    directory: &Path,
    target: &Path,
    complete_len: u64,
) -> Attempt {
    let deadline = Instant::now() + CRASH_SIGHTING_TIMEOUT;
    let mut temp_seen = false;
    let mut reason = "timeout";
    let mut child_exited_early = false;

    loop {
        if writer.child_mut().try_wait().expect("子の状態を観測できる").is_some() {
            child_exited_early = true;
            reason = "child-exited";
            break;
        }
        if !temp_file_names(directory).is_empty() {
            temp_seen = true;
            reason = "temp-file";
            break;
        }
        let length_changed = fs::metadata(target).map(|meta| meta.len() != complete_len).unwrap_or(true);
        if length_changed {
            reason = "target-size";
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        // 100 µs 程度で譲る。書き込みの窓（4 MiB の書き込み + `sync_all`）より十分短い。
        thread::sleep(Duration::from_micros(100));
    }

    let status = writer.kill_and_wait();
    assert!(!child_exited_early, "ワーカーが親に落とされる前に終了した（根拠: {reason}）");
    assert!(!status.success(), "ワーカーを落とせていない: {status:?}");

    Attempt {
        temp_seen,
        reason,
        target_len: fs::metadata(target).ok().map(|meta| meta.len()),
        child_exited_early,
        // 落とした直後の内容は呼び出し側が [`assert_complete_state`] で確かめて埋める。
        observed_state: None,
    }
}

/// `current_exe()` のワーカーを起動する（`--ignored --exact crash_writer`）。
fn spawn_crash_writer(directory: &Path) -> Child {
    let executable = std::env::current_exe().expect("テストバイナリのパスが取れる");
    Command::new(executable)
        .args(["--exact", "--ignored", "--test-threads=1", "crash_writer"])
        .env(CRASH_MODE_ENV, CRASH_MODE)
        .env(CRASH_DIR_ENV, directory)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ワーカーを起動できる")
}

/// 試行の準備: 一時ファイルと準備完了を消し、対象を状態 A（完全な内容）へ戻す。
fn reset_attempt(scratch: &Scratch) {
    for name in temp_file_names(scratch.path()) {
        let _ = fs::remove_file(scratch.path().join(name));
    }
    let _ = fs::remove_file(scratch.path().join(READY_FILE_NAME));
    fs::write(scratch.target(), crash_document('A')).expect("初期状態を書ける");
}

/// **完了状態（tasks.md 4.1）**: 書き込みの途中でプロセスを落としても、対象には直前の完全な
/// 内容（状態 A）か新しい完全な内容（状態 B）のいずれかが残る。
///
/// 実プロセスを `Child::kill()` で落として確かめる。各試行で「落とした直後の対象が完全な
/// JSON であり、状態 A か状態 B に完全に一致する」ことを要求し、加えて「一時ファイルを
/// 観測して落とした試行が 1 回以上ある」ことを要求する。
///
/// **負荷証明**: 書き込みを truncate-then-write に変えると、一時ファイルが現れないため
/// 「観測した試行が無い」で必ず落ち、加えて部分的な対象が観測されれば
/// [`assert_complete_state`] が落ちる。実装時に実測して記録してある。
#[test]
fn crash_during_write_leaves_a_complete_file() {
    let scratch = Scratch::new("crash");
    let complete_len = crash_document('A').len() as u64;
    let mut attempts = Vec::with_capacity(CRASH_ATTEMPTS);

    for attempt_index in 0..CRASH_ATTEMPTS {
        reset_attempt(&scratch);
        let mut writer = WriterGuard::new(spawn_crash_writer(scratch.path()));
        wait_for_ready(&scratch.path().join(READY_FILE_NAME));
        let mut observation =
            observe_and_kill(&mut writer, scratch.path(), &scratch.target(), complete_len);
        observation.observed_state = Some(assert_complete_state(
            &scratch.target(),
            &format!("試行 {attempt_index}（根拠 {}）", observation.reason),
        ));
        attempts.push(observation);
    }

    let temp_seen = attempts.iter().filter(|attempt| attempt.temp_seen).count();
    // 観測の内訳は失敗メッセージに載せる（本番の実装で落ちたときに、どの位相で落ちたかを
    // 診断できるようにする。この整形が各フィールドの唯一の読み手である）。
    let summary: Vec<String> = attempts
        .iter()
        .map(|attempt| {
            format!(
                "{} (一時ファイル={}, 対象長={:?}, 事前終了={}, 残った内容={:?})",
                attempt.reason,
                attempt.temp_seen,
                attempt.target_len,
                attempt.child_exited_early,
                attempt.observed_state
            )
        })
        .collect();
    assert!(
        temp_seen > 0,
        "書き込みの窓で落ちた試行が 1 回も無い（原子性を確かめられていない）: {summary:?}"
    );
    // 「直前の完全な内容」が実際に観測されること。一時ファイルを観測した直後に落としている
    // 以上、置換はまだ成立しておらず、対象は状態 A のままであるはずである。これが無いと、
    // 置換が先に成立してから落としている（= 書き込みの窓に入っていない）可能性を排除できない。
    assert!(
        attempts.iter().any(|attempt| attempt.observed_state == Some('A')),
        "直前の完全な内容が残る場合を 1 度も観測できていない: {summary:?}"
    );
    // 落とされる直前までワーカーが書き続けていたこと（早期終了は実装の不具合）。
    assert!(
        attempts.iter().all(|attempt| !attempt.child_exited_early),
        "ワーカーが親に落とされる前に終了した試行がある: {summary:?}"
    );
}

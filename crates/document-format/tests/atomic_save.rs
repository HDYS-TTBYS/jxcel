#![cfg(unix)]

//! 原子的保存の中断耐性（タスク 8.6。要件 5.6。design「AtomicWriter / Validation」）。
//!
//! 要件 5.6 は「保存処理が完了前に中断されたとき、保存対象のファイルを保存前の内容の
//! まま残す」ことを求める。`AtomicWriter::commit` はこのために、対象と**同一ディレクトリ**
//! の一時ファイル（prefix `.jxcel-tmp-`）へ `write_all` → `sync_all` した後、`rename` で
//! 置換する（`src/container/atomic_save.rs`）。置換の前に落ちれば、対象は保存前のまま
//! 残り、**残るのは一時ファイルだけ**である（クラッシュでは `Drop` が走らないため）。
//!
//! `commit` には注入点が無く、`src/` も変更できないため、この保証は**子プロセスを実際に
//! `SIGKILL` して観測する**:
//!
//! 1. 親（本ファイルの通常テスト）は、対象を保存前の内容（ゴールデン fixture の
//!    バイト列）で用意し、inode / mtime / 長さ / バイト列を記録する。
//! 2. 子は `std::env::current_exe()` でこのテストバイナリを再実行し、`#[ignore]` の
//!    ワーカー [`crash_worker`] を `--ignored --exact` で走らせる。ワーカーは環境変数で
//!    対象・レイアウト・期待サイズの出力先を受け取り、**公開 API（`save`）** で保存する。
//!    期待サイズ（保存が書くバイト数）は保存の前にワーカー自身が `to_parts` /
//!    `ContainerCodec::encode` で算出して書き出す。親はこれを、一時ファイルが
//!    **書き切られた位相**で落ちたことを実測するために使う。
//! 3. 親は一時ファイル `.jxcel-tmp-` の出現と大きさをポーリングし、方針（出現直後 /
//!    期待サイズ到達後 / 出現から一定時間後 / 「一時ファイルが残っているのに対象が
//!    変わった」不変条件違反）に従って `Child::kill()`（Unix では `SIGKILL`）する。
//!    正確な瞬間は狙えないため、複数の方針で複数回試行する。
//! 4. 各試行の後に、対象が保存前のバイト列のままで `open` できること、一時ファイルの
//!    残骸が存在すること（＝落ちたのが「一時ファイルを書いた後」であることの積極的な
//!    証拠）、inode / mtime / 長さが不変であること（＝置換が起きていないこと）を実測する。
//!
//! 落とす前提のワーカーは `#[ignore]` により通常の `cargo test` では実行されない
//! （親だけが `--ignored --exact` で明示的に起動する）。
//!
//! 対照として、落とさなければ置換が完了する（対象が新しい内容になる）ことも
//! [`an_uninterrupted_save_completes_the_replacement`] で固定する。これが無いと
//! 「何も書かない実装」でも中断耐性テストが通ってしまう。
//!
//! 試行ディレクトリは [`Scratch`]（リポジトリ内・`Drop` で削除）に閉じるため、並列実行
//! される他のテストと干渉しない。子プロセスの環境変数は**値の厳密一致**で分岐する
//! （過去のレビュー教訓）。

mod common;

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use document_format::container::ContainerCodec;
use document_format::{CellValue, Document, DocumentFormatApi, SchemaPart};

use common::{api, fixture_path, Scratch, SCHEMA_EMPTY};

/// ワーカーが使うレイアウトの識別子。親と子が環境変数の値の**厳密一致**で共有する。
const LARGE_ATTACHMENT_LAYOUT: &str = "large-attachment";

/// ワーカーへ渡す環境変数。
const TARGET_ENV: &str = "JXCEL_CRASH_TARGET";
const LAYOUT_ENV: &str = "JXCEL_CRASH_LAYOUT";
const SIZE_FILE_ENV: &str = "JXCEL_CRASH_SIZE_FILE";

/// 一時ファイルの接頭辞（`src/container/atomic_save.rs` の `TEMP_FILE_PREFIX` と一致）。
const TEMP_FILE_PREFIX: &str = ".jxcel-tmp-";

/// ワーカーが「保存前に書く期待サイズ」のファイル名。
const SIZE_FILE_NAME: &str = "expected-size.txt";

/// 添付の大きさ。`write_all` → `sync_all` の窓を観測できる程度に大きく、debug の
/// deflate が遅くなりすぎない範囲に取る（実測した実行時間は status report を参照）。
const ATTACHMENT_BYTES: usize = 4 * 1024 * 1024;

/// 一時ファイルの出現を待つ上限（これを超えたら落として試行を打ち切る）。
const SIGHTING_TIMEOUT: Duration = Duration::from_secs(120);

/// 一時ファイルを観測してから方針を満たすのを待つ上限。
const WINDOW_TIMEOUT: Duration = Duration::from_secs(10);

/// 「書き切った後に落ちた」観測を得るための最大試行回数（この間に得られなければ失敗）。
const MAX_ATTEMPTS: usize = 6;

/// 実際に `SIGKILL` したことの確認に使うシグナル番号（`Child::kill` の Unix 実装）。
const SIGKILL: i32 = 9;

// --- 落とされる側のワーカー -------------------------------------------------------------

/// 落とされる側のワーカー。
///
/// **通常の `cargo test` では実行されない**（`#[ignore]`）。親テストだけが
/// `current_exe()` を `--ignored --exact crash_worker` で起動する。環境変数で対象・
/// レイアウト・期待サイズの出力先を受け取り、公開 API [`DocumentFormatApi::save`] で
/// 保存する。親が落とさなければ保存は完了して通常終了する（対照テストがこれを観測する）。
#[ignore = "親テストが current_exe 経由で SIGKILL する専用のワーカー"]
#[test]
fn crash_worker() {
    let target = absolute_env(TARGET_ENV);
    let size_file = absolute_env(SIZE_FILE_ENV);
    match std::env::var(LAYOUT_ENV).as_deref() {
        Ok(LARGE_ATTACHMENT_LAYOUT) => {}
        Ok(other) => panic!("未知のレイアウト: {other}"),
        Err(err) => panic!("{LAYOUT_ENV} が無い: {err}"),
    }

    let document = document_with_large_attachment();

    // 親が「一時ファイルが書き切られた位相」を実測できるよう、保存の前に、保存が書く
    // バイト数を公開 API 経由（`save` の内部と同じ経路）で算出して書き出す。
    let parts = api().to_parts(&document).expect("標本は妥当");
    let encoded = ContainerCodec::encode(&parts).expect("符号化できる");
    fs::write(&size_file, encoded.len().to_string()).expect("期待サイズを書ける");

    // ここで親が落とす。落とされなければ保存は完了し、通常終了する。
    api().save(&document, &target).expect("中断されなければ保存できる");
}

/// 環境変数を絶対パスとして読む（相対パスは親の意図に反するため弾く）。
fn absolute_env(name: &str) -> PathBuf {
    let value = std::env::var(name).unwrap_or_else(|err| panic!("{name} が無い: {err}"));
    let path = PathBuf::from(&value);
    assert!(path.is_absolute(), "{name} は絶対パスでなければならない: {value}");
    path
}

/// ワーカーが保存する文書: 大きな添付を 1 つ参照する 1 シート 1 行。
///
/// エンコード後のバイト列が大きく、`write_all` → `sync_all` の窓が観測できる程度に
/// 広がる。添付は DEFLATE で縮まない擬似乱数にし、圧縮後のサイズを保つ。
fn document_with_large_attachment() -> Document {
    let mut document = Document::new();
    let sheet = document.add_sheet("大きい添付");
    document
        .set_sheet_columns(sheet, vec!["blob".to_owned()])
        .expect("標本のシートは実在する");
    document
        .set_root_schema(sheet, SchemaPart::parse(SCHEMA_EMPTY).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    let attachment = document.add_attachment(pseudo_random_bytes(ATTACHMENT_BYTES));
    let row = document.add_row(sheet).expect("標本のシートは実在する");
    document
        .set_row_values(sheet, row, vec![CellValue::Attachment(attachment)])
        .expect("標本の行は実在する");

    document
}

/// 外部 crate を増やさず、DEFLATE でほとんど縮まないバイト列を作る（xorshift64）。
fn pseudo_random_bytes(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

// --- 親（通常テスト） -------------------------------------------------------------------

/// 一時ファイルを観測した後にどう落とすか。
#[derive(Debug, Clone, Copy)]
enum KillPolicy {
    /// 一時ファイルを観測した瞬間に落とす。
    OnSighting,
    /// 一時ファイルが期待サイズへ達するのを待ってから落とす（書き切った後・置換の前）。
    AfterFullWrite,
    /// 一時ファイルを観測してから指定時間だけ待って落とす。
    After(Duration),
    /// 「一時ファイルが残っているのに対象が変わった」不変条件違反を観測した瞬間に落とす
    /// （違反しなければ保存を完了させる）。`rename` を経ない in-place 書き換えを殺す。
    OnViolation,
}

impl KillPolicy {
    /// 試行の表示に使う短い名前。
    fn label(self) -> String {
        match self {
            KillPolicy::OnSighting => "出現直後".to_owned(),
            KillPolicy::AfterFullWrite => "書き切り後".to_owned(),
            KillPolicy::After(delay) => format!("出現 + {}ms", delay.as_millis()),
            KillPolicy::OnViolation => "不変条件違反".to_owned(),
        }
    }
}

/// 落とす直前に観測した一時ファイルの状態。
#[derive(Debug, Clone, Copy)]
struct Observation {
    /// 落とす直前に観測した一時ファイルの大きさ。
    temp_bytes: Option<u64>,
    /// 落とす直前に期待サイズへ達していたか（＝書き切った後）。
    full_write: bool,
}

/// 試行 1 回の観測結果。
#[derive(Debug)]
struct Attempt {
    policy: String,
    status: ExitStatus,
    /// 落ちた後に対象が保存前のバイト列のままで、一時ファイルの残骸がある（＝望む位相）。
    interrupted: bool,
    /// 落とすまでに置換が成立した（一時ファイルが無く、対象が新しい内容）。
    replaced: bool,
    /// 落とす直前に一時ファイルが期待サイズへ達していた。
    full_write_observed: bool,
    temp_bytes_before_kill: Option<u64>,
    expected_bytes: u64,
    /// 落ちた後も対象が `open` で読める（壊れた/切り詰められたファイルでない）。
    target_openable: bool,
    ino_unchanged: bool,
    mtime_unchanged: bool,
    len_unchanged: bool,
    /// 上記のどれにも当てはまらない異常（実装の欠陥または試行の設定ミス）。
    anomaly: Option<String>,
}

/// 中断が起きなければ置換が完了する（対照。要件 5.6 の肯定側）。
#[test]
fn an_uninterrupted_save_completes_the_replacement() {
    let scratch = Scratch::new("atomic_control");
    let target = scratch.file("target.jxcel");
    let seed = fs::read(fixture_path()).expect("ゴールデンが読める");
    fs::write(&target, &seed).expect("保存前の内容を置ける");

    let size_file = scratch.file(SIZE_FILE_NAME);
    let mut child = spawn_worker(&target, &size_file);
    let status = child.wait().expect("ワーカーを回収できる");
    assert!(status.success(), "中断しなければ保存は成功するはず: {status:?}");

    let after = fs::read(&target).expect("対象が読める");
    assert_ne!(seed, after, "対象が置換されていない（何も書かない実装でも通ってしまう）");
    api().open(&target).expect("置換後の内容は開ける");
    assert!(temporary_files(scratch.path()).is_empty(), "完了した保存が一時ファイルを残した");
}

/// 要件 5.6 の中核: 置換前にプロセスを落とすと、対象は保存前の内容のまま残る。
///
/// 複数の方針で試行し、各試行について「対象が保存前のまま `open` できる」「一時ファイルの
/// 残骸がある」「inode / mtime / 長さが不変」を実測する。少なくとも 1 回は**書き切った後**
/// （期待サイズ到達後）に落ちたことを実測で示す。
#[test]
fn an_interrupted_save_leaves_the_previous_content_in_place() {
    let scratch = Scratch::new("atomic_crash");
    let target = scratch.file("target.jxcel");
    let seed = fs::read(fixture_path()).expect("ゴールデンが読める");

    let policies = [
        KillPolicy::OnSighting,
        KillPolicy::AfterFullWrite,
        KillPolicy::After(Duration::from_millis(2)),
        KillPolicy::After(Duration::from_millis(20)),
        KillPolicy::OnViolation,
    ];

    let mut attempts: Vec<Attempt> = Vec::new();
    for policy in policies {
        attempts.push(attempt(&scratch, &target, &seed, policy));
    }

    // 「書き切った後に落ちた」観測が取れなければ、方針を絞って追加試行する（窓は確率的
    // なので、置換が先に成立した試行は「落とせなかった」として数え直す）。
    let mut full_write_interrupted = count_full_write_interrupted(&attempts);
    while full_write_interrupted == 0 && attempts.len() < MAX_ATTEMPTS {
        let extra = attempt(&scratch, &target, &seed, KillPolicy::AfterFullWrite);
        if extra.interrupted && extra.full_write_observed {
            full_write_interrupted += 1;
        }
        attempts.push(extra);
    }

    // 観測した位相の分布を残す（`cargo test -- --nocapture` で読める）。
    for attempt in &attempts {
        eprintln!(
            "試行[{}]: 状態={:?} 中断={} 置換={} 書切り={} 一時={:?}/期待={} 例外={:?}",
            attempt.policy,
            attempt.status,
            attempt.interrupted,
            attempt.replaced,
            attempt.full_write_observed,
            attempt.temp_bytes_before_kill,
            attempt.expected_bytes,
            attempt.anomaly,
        );
    }

    for attempt in &attempts {
        assert!(attempt.anomaly.is_none(), "試行が異常: {attempt:?}");
        if attempt.interrupted {
            assert!(
                attempt.ino_unchanged && attempt.mtime_unchanged && attempt.len_unchanged,
                "置換が起きている（inode/mtime/長さが変化）: {attempt:?}"
            );
            assert!(attempt.target_openable, "保存前の内容が open できない: {attempt:?}");
        }
        assert!(
            attempt.interrupted || attempt.replaced,
            "落ちた後に対象でも置換でもない状態は起こらない: {attempt:?}"
        );
    }

    let interrupted = attempts.iter().filter(|attempt| attempt.interrupted).count();
    assert!(interrupted >= 1, "中断された試行が 1 回も無い: {attempts:#?}");
    assert!(
        count_full_write_interrupted(&attempts) >= 1,
        "「書き切った後に落ちた」試行が 1 回も無い: {attempts:#?}"
    );
    assert!(
        attempts.iter().any(|attempt| killed_by_sigkill(&attempt.status)),
        "SIGKILL で落ちた試行が 1 回も無い: {attempts:#?}"
    );

    // 残骸を片付ける（`Scratch` の `Drop` でも消えるが、テスト内で消えたことを明示する）。
    clean_scratch(scratch.path());
    assert!(temporary_files(scratch.path()).is_empty(), "一時ファイルの残骸が残った");
}

/// 試行を 1 回行う。対象を `seed` へ戻し、子を起動し、方針に従って落として観測する。
fn attempt(scratch: &Scratch, target: &Path, seed: &[u8], policy: KillPolicy) -> Attempt {
    clean_scratch(scratch.path());
    fs::write(target, seed).expect("対象を保存前の内容へ戻せる");
    let before = fs::metadata(target).expect("メタデータが読める");

    let size_file = scratch.file(SIZE_FILE_NAME);
    let mut child = spawn_worker(target, &size_file);
    let expected_bytes = wait_for_expected_size(&mut child, &size_file);
    let observation =
        observe_and_kill(&mut child, scratch.path(), target, seed, expected_bytes, policy);
    let status = child.wait().expect("ワーカーを回収できる");

    let temp_present = !temporary_files(scratch.path()).is_empty();
    let after = fs::metadata(target).expect("メタデータが読める");
    let after_bytes = fs::read(target).expect("対象が読める");
    let target_untouched = after_bytes == seed;
    let target_openable = api().open(target).is_ok();

    let interrupted = temp_present && !status.success() && target_untouched;
    let replaced = !temp_present && !target_untouched;
    let anomaly = if temp_present && !target_untouched {
        Some("一時ファイルが残っているのに対象が変わっている".to_owned())
    } else if !interrupted && !replaced {
        Some(format!("落ちた後に対象が保存前のままでも置換でもない（status={status:?}）"))
    } else {
        None
    };

    Attempt {
        policy: policy.label(),
        status,
        interrupted,
        replaced,
        full_write_observed: observation.full_write,
        temp_bytes_before_kill: observation.temp_bytes,
        expected_bytes,
        target_openable,
        ino_unchanged: before.ino() == after.ino(),
        mtime_unchanged: before.modified().ok() == after.modified().ok(),
        len_unchanged: before.len() == after.len(),
        anomaly,
    }
}

/// ワーカーが保存前に書く「期待サイズ」を待つ（子の終了で中断）。
fn wait_for_expected_size(child: &mut Child, size_file: &Path) -> u64 {
    let deadline = Instant::now() + SIGHTING_TIMEOUT;
    loop {
        if let Ok(text) = fs::read_to_string(size_file) {
            if let Ok(value) = text.trim().parse::<u64>() {
                assert!(value > 0, "期待サイズが 0 である");
                return value;
            }
        }
        if let Some(status) = child.try_wait().expect("子を観測できる") {
            panic!("ワーカーが期待サイズを書く前に終了した: {status:?}");
        }
        assert!(Instant::now() < deadline, "ワーカーが期待サイズを書かない");
        thread::sleep(Duration::from_micros(200));
    }
}

/// 一時ファイルの出現を待ち、方針に従って子を落とす。落とす直前の観測を返す。
///
/// [`KillPolicy::OnViolation`] は「一時ファイルが残っているのに対象が保存前の内容から
/// 変わった」不変条件違反を検出した瞬間に落とす（正しい実装では違反は起きないため、
/// 保存を完了させて置換として観測する）。`rename` を経ない in-place 書き換えを殺す。
fn observe_and_kill(
    child: &mut Child,
    directory: &Path,
    target: &Path,
    seed: &[u8],
    expected_bytes: u64,
    policy: KillPolicy,
) -> Observation {
    let sighting_deadline = Instant::now() + SIGHTING_TIMEOUT;
    let mut sighted_at: Option<Instant> = None;
    let mut last_bytes: Option<u64> = None;
    loop {
        if let Some(bytes) = temporary_file_bytes(directory) {
            let first_seen = *sighted_at.get_or_insert_with(Instant::now);
            last_bytes = Some(bytes);
            let full_write = bytes >= expected_bytes;
            let should_kill = match policy {
                KillPolicy::OnSighting => true,
                KillPolicy::AfterFullWrite => full_write,
                KillPolicy::After(delay) => first_seen.elapsed() >= delay,
                KillPolicy::OnViolation => target_changed(target, seed),
            };
            if should_kill {
                let _ = child.kill();
                return Observation { temp_bytes: last_bytes, full_write };
            }
            if first_seen.elapsed() >= WINDOW_TIMEOUT {
                let _ = child.kill();
                return Observation { temp_bytes: last_bytes, full_write };
            }
            // 一時ファイルが現れた後は窓が狭いので、譲りながら密にポーリングする。
            thread::yield_now();
        } else {
            if child.try_wait().expect("子を観測できる").is_some() {
                // 置換が成立した（一時ファイルが消えた）か、保存が失敗して終了した。
                let full_write = last_bytes.map(|bytes| bytes >= expected_bytes).unwrap_or(false);
                return Observation { temp_bytes: last_bytes, full_write };
            }
            if Instant::now() >= sighting_deadline {
                let _ = child.kill();
                return Observation { temp_bytes: None, full_write: false };
            }
            // 一時ファイルが現れる前はエンコードに時間がかかるため、粗く待つ。
            thread::sleep(Duration::from_micros(100));
        }
    }
}

/// 対象が保存前の内容から変わったか（読めない場合は保存の途中とみなす）。
fn target_changed(target: &Path, seed: &[u8]) -> bool {
    match fs::read(target) {
        Ok(bytes) => bytes != seed,
        Err(_) => true,
    }
}

/// `current_exe()` のワーカーを起動する（`--ignored --exact crash_worker`）。
fn spawn_worker(target: &Path, size_file: &Path) -> Child {
    let executable = std::env::current_exe().expect("テストバイナリのパスが取れる");
    Command::new(executable)
        .args(["--exact", "--ignored", "--test-threads=1", "crash_worker"])
        .env(TARGET_ENV, target)
        .env(LAYOUT_ENV, LARGE_ATTACHMENT_LAYOUT)
        .env(SIZE_FILE_ENV, size_file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ワーカーを起動できる")
}

/// 作業ディレクトリ直下の一時ファイル名（`.jxcel-tmp-` で始まるもの）を昇順で返す。
fn temporary_files(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("作業ディレクトリが読める")
        .map(|entry| entry.expect("要素が読める").file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(TEMP_FILE_PREFIX))
        .collect();
    names.sort();
    names
}

/// 一時ファイルが現れていればその大きさを返す。
fn temporary_file_bytes(directory: &Path) -> Option<u64> {
    for entry in fs::read_dir(directory).expect("作業ディレクトリが読める") {
        let entry = entry.expect("要素が読める");
        if entry.file_name().to_string_lossy().starts_with(TEMP_FILE_PREFIX) {
            return entry.metadata().ok().map(|metadata| metadata.len());
        }
    }
    None
}

/// 一時ファイルの残骸と期待サイズのファイルを消す（試行間の干渉を防ぐ）。
fn clean_scratch(directory: &Path) {
    for name in temporary_files(directory) {
        let _ = fs::remove_file(directory.join(name));
    }
    let _ = fs::remove_file(directory.join(SIZE_FILE_NAME));
}

/// `SIGKILL` で落ちたことの確認（`Child::kill` の Unix 実装）。
fn killed_by_sigkill(status: &ExitStatus) -> bool {
    status.signal() == Some(SIGKILL)
}

/// 「書き切った後に落ちた」試行の数。
fn count_full_write_interrupted(attempts: &[Attempt]) -> usize {
    attempts.iter().filter(|attempt| attempt.interrupted && attempt.full_write_observed).count()
}

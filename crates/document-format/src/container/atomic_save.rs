//! ファイルの原子的な置換（タスク 5.1。要件 5.6。design「Container Layer / AtomicWriter」）。
//!
//! 本モジュールの責務は「**バイト列を、既存ファイルがあるパスへ原子的に置き換える**」
//! ことだけである。ZIP の知識も、ドキュメント内容の解釈も持たない（`&[u8]` を受け取って
//! 書くだけ）。コンテナの符号化はタスク 5.2、復号は 5.3 の担当である。
//!
//! # 手順（design「System Flows / 保存フロー」）
//!
//! 1. 対象と**同一ディレクトリ**に一時ファイルを作る（同一ファイルシステム上の `rename`
//!    でなければ置換は原子的にならない）
//! 2. バイト列を書き切り、`sync_all` で内容を永続化する
//! 3. `rename` で対象を置換する（Windows は共有違反に対してバックオフ付きで再試行する）
//! 4. Unix では親ディレクトリを fsync し、エントリの置換それ自体を永続化する
//!
//! 2 の完了前、3 の完了前にプロセスが落ちた場合、対象パスには保存前の内容がそのまま残る
//! （要件 5.6）。落ちた時点で残るのは一時ファイルであり、対象パスではない。
//! `Err` は常に「対象が置換されていない」ことを意味し、置換成立後の親ディレクトリ同期の
//! 失敗は握り潰す（詳細と理由は [`AtomicWriter::commit`] の docs にある）。
//!
//! # `NamedTempFile::persist()` を使わない理由
//!
//! design「AtomicWriter / Risks」のとおり、`persist()` は全プラットフォームでの原子性を
//! 保証しない（Windows の `MoveFileExW` は、対象をハンドルで保持している他プロセスがあると
//! 共有違反で失敗する）。本モジュールは [`std::fs::rename`] を直接呼び、Windows では
//! 共有違反系のエラーコードに対してバックオフ付きで再試行する。`NamedTempFile` には
//! 一時ファイルの作成と、失敗時に残さないための `Drop` の削除だけを任せる。
//!
//! # 実装の分割
//!
//! [`AtomicWriter::commit`] は 2 つの非公開関数 `stage`（一時ファイルへ書いて同期する）と
//! `install`（置換と親ディレクトリの fsync）に分かれている。この分割は「置換の前に落ちた
//! 場合に対象が無傷である」ことを実測するためでもある: テストは `stage` の直後・`install`
//! の前にプロセスを `abort` させ、対象が保存前の内容のままであることを検証する
//! （design「AtomicWriter / Validation」）。

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

use tempfile::{Builder, NamedTempFile};

use crate::error::DocumentError;

/// 一時ファイル名の接頭辞。対象ファイル名からは作らない（`tempfile` が付ける
/// ランダムな 6 文字と合わせて、対象ファイル名と衝突しない）。
const TEMP_FILE_PREFIX: &str = ".jxcel-tmp-";

/// `rename` の最大試行回数（初回を含む）。
const RENAME_ATTEMPTS: u32 = 10;

/// 再試行 1 回目の待機時間。2 回目以降は倍々に延ばす（1, 2, 4, …, 200 ms）。
const RENAME_FIRST_BACKOFF: Duration = Duration::from_millis(1);

/// 待機時間の上限（倍々にした結果がこれを超えたらこの値で頭打ちにする）。
const RENAME_MAX_BACKOFF: Duration = Duration::from_millis(200);

/// Windows で再試行する OS エラーコード。
///
/// - 32 `ERROR_SHARING_VIOLATION`: 他プロセスがハンドルを保持している
/// - 33 `ERROR_LOCK_VIOLATION`: 同上（バイト範囲ロック）
/// - 5 `ERROR_ACCESS_DENIED`: `FILE_SHARE_DELETE` 無しで開かれた対象の置換で返る
///
/// アンチウイルス・検索インデクサ・自アプリの別インスタンスが短時間ハンドルを保持する
/// ことがあるため、待ってからやり直せば通る（design「AtomicWriter / Responsibilities」）。
///
/// ただし 5 は曖昧である: **置換先が既存のディレクトリのときも 5 が返る**（Unix の
/// `EISDIR` に当たる恒久的な失敗）。コードだけでは両者を区別できないため、
/// `rename_with_retries` が置換先の種別を見てディレクトリなら再試行しない。
#[cfg(windows)]
const RETRYABLE_RENAME_CODES: &[i32] = &[5, 32, 33];

/// Unix で再試行する OS エラーコードは無い。`rename` は置換対象を開いているプロセスが
/// あっても成功するため、「共有違反」という状態が存在しない。Windows のコード値
/// （5 / 32 / 33）は Unix では別の `errno` を指すので流用してはならない。
#[cfg(not(windows))]
const RETRYABLE_RENAME_CODES: &[i32] = &[];

/// 保存の最終段: バイト列を対象パスへ原子的に置き換える（要件 5.6）。
///
/// 状態を持たない（対象は呼び出しごとに [`AtomicWriter::commit`] へ渡す）ため、値ではなく
/// 名前空間としての型である。型名 `AtomicWriter` は design「Container Layer / AtomicWriter」
/// が指定する名前である。
pub struct AtomicWriter;

impl AtomicWriter {
    /// `bytes` を `target` へ原子的に置き換える。確定形:
    /// `AtomicWriter::commit(target: &Path, bytes: &[u8]) -> Result<(), DocumentError>`。
    ///
    /// 手順は design「System Flows / 保存フロー」のとおり: 対象と同一ディレクトリの
    /// 一時ファイルへ `bytes` を書き切る → `sync_all` → `rename` → 親ディレクトリの fsync
    /// （Unix）。`rename` が共有違反で失敗する Windows ではバックオフ付きで再試行する
    /// （モジュール docs と「再試行」節）。
    ///
    /// # 保証
    ///
    /// - **置換の前に失敗した場合、`target` は保存前の内容のままである**（要件 5.6）。
    ///   一時ファイルの作成・書き込み・`sync_all`・`rename` のどこで失敗しても、また
    ///   置換の前にプロセスが落ちても、対象パスは変更されない。
    /// - 失敗したときに一時ファイルを残さない（[`tempfile::NamedTempFile`] の `Drop` が
    ///   消す）。
    /// - `target` が存在しなければ新規作成する。親ディレクトリが存在しなければ `Err` で
    ///   あり、作成はしない。
    /// - `bytes` は借用のまま書く（内容のコピーを作らない）。
    /// - 置換は**ディレクトリエントリの差し替え**である（対象の中身を書き換えるのでは
    ///   ない）。対象を開いているプロセスと既存のハードリンクは保存前の内容を読み続ける
    ///   （`rename` の意味論）。
    ///
    /// # 失敗したときの対象ファイル
    ///
    /// **`Err` は常に「`target` は保存前の内容のままである」ことを意味する**（要件 5.6、
    /// design エラー表の `Io { source, retried }` の応答「既存ファイルは無変更」）。
    /// 一時ファイルの作成・書き込み・`sync_all`・`rename` のどこで失敗しても、また置換の
    /// 前にプロセスが落ちても、対象パスは変更されない。
    ///
    /// したがって、**`rename` が成立した後は `Err` を返さない**: 置換の後に続く親
    /// ディレクトリの同期（Unix）は試みるが、その失敗は `Ok(())` として握り潰す
    /// （対象は既に置換済みであり、そこで `Err` を返すと呼び出し元が依存する
    /// 「`Err` ⇒ 対象は無変更」が破れるため）。この握り潰しで落ちうるのは**置換の
    /// 永続化だけ**である: 置換の直後に電源断すれば置換が失われて保存前の内容に戻る
    /// 可能性はあるが、**部分的に書かれたファイルにはならない**（完全に書き切って
    /// `sync_all` した一時ファイルを置換しているため）。
    ///
    /// # 再試行（Windows）
    ///
    /// 共有違反系のエラーコード（`RETRYABLE_RENAME_CODES`）に対して最大
    /// `RENAME_ATTEMPTS` 回（初回を含む）試行する。待機は `RENAME_FIRST_BACKOFF`
    /// （1 ms）から倍々に延ばし、`RENAME_MAX_BACKOFF`（200 ms）で頭打ちにする。
    /// Unix では再試行しない。再試行予算を使い切った場合にだけ [`DocumentError::Io`] の
    /// `retried` が `true` になる（再試行の途中で再試行不能なエラーに変わった場合は
    /// `false`）。
    ///
    /// # 注意
    ///
    /// - 対象のファイルモードは保存前の値に引き継がれない（一時ファイルのモードが残る）。
    ///   design が AtomicWriter に与える責務は内容の原子的な置換であり、権限の保持は
    ///   要件に無い。
    /// - `target` がシンボリックリンクなら、リンク先ではなくリンクそのものが置換される
    ///   （`rename` の意味論）。
    ///
    /// # 例
    ///
    /// ```
    /// use std::fs;
    /// use document_format::container::AtomicWriter;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// let path = dir.path().join("document.jxcel");
    /// fs::write(&path, b"before").unwrap();
    ///
    /// AtomicWriter::commit(&path, b"after").unwrap();
    ///
    /// assert_eq!(b"after", fs::read(&path).unwrap().as_slice());
    /// ```
    pub fn commit(target: &Path, bytes: &[u8]) -> Result<(), DocumentError> {
        commit_with_sync(target, bytes, sync_parent_dir)
    }
}

/// [`AtomicWriter::commit`] の本体。置換の後に走る同期段だけを差し替えられるようにして
/// ある（本番は [`sync_parent_dir`] を渡す。テストはそこへ失敗を注入して、置換成立後に
/// `Err` を返さないことを実測する）。
fn commit_with_sync(
    target: &Path,
    bytes: &[u8],
    sync_dir: impl FnOnce(&Path) -> Result<(), DocumentError>,
) -> Result<(), DocumentError> {
    let staged = stage(target, bytes)?;
    install(staged, target, sync_dir)
}

/// `commit` の第 1 段: 対象と同一ディレクトリに一時ファイルを作り、`bytes` を書き切って
/// `sync_all` する。
///
/// 戻り値の一時ファイルを `install` へ渡すまで、対象パスは一切変更されない。`Drop` に
/// 任せているので、この段のどこかで失敗しても一時ファイルは残らない。
fn stage(target: &Path, bytes: &[u8]) -> Result<NamedTempFile, DocumentError> {
    let dir = parent_dir(target)?;
    let mut staged = Builder::new()
        .prefix(TEMP_FILE_PREFIX)
        .tempfile_in(dir)
        .map_err(|err| io_error(err, false))?;
    // `NamedTempFile` の `Write` は `File` への素通しでバッファリングしないため、
    // フラッシュは要らない（`write_all` のあとそのまま `sync_all` できる）。
    staged.write_all(bytes).map_err(|err| io_error(err, false))?;
    staged.as_file().sync_all().map_err(|err| io_error(err, false))?;
    Ok(staged)
}

/// `commit` の第 2 段: 一時ファイルを対象パスへ `rename` し、親ディレクトリを同期する。
///
/// 失敗すると一時ファイルは `Drop` が削除する（対象パスは変更されない）。**`rename` が
/// 成立した後に `Err` を返す経路を持たない**（[`AtomicWriter::commit`] の「失敗したときの
/// 対象ファイル」節: `Err` は「対象が置換されていない」ことを意味しなければならない）。
fn install(
    staged: NamedTempFile,
    target: &Path,
    sync_dir: impl FnOnce(&Path) -> Result<(), DocumentError>,
) -> Result<(), DocumentError> {
    // 親ディレクトリは置換の**前**に確定させる。置換が成立した後に `?` で抜ける経路を
    // 1 つも残さないため、`?` を使ってよいのはこの行までである。
    let dir = parent_dir(target)?;
    rename_with_retries(staged.path(), target)?;
    // ここから下、この関数は必ず `Ok(())` を返す。
    // 置換は成立したので、一時ファイル名は消えている。`Drop` に削除を試みさせない
    // （対象ディレクトリに同名の別ファイルが現れていた場合の削除を避ける）。
    let _ = staged.keep();
    // 親ディレクトリの同期は試みるが、失敗は握り潰す（上記 docs）。
    let _ = sync_dir(dir);
    Ok(())
}

/// 対象パスの親ディレクトリを返す。
///
/// `"document.jxcel"` のようにカレントディレクトリ直下を指す名前では親が空パスになる。
/// 空パスはディレクトリとして開けない（親の fsync が失敗する）ため、カレントディレクトリ
/// `"."` へ正規化する。親を持たないパス（ルート）は対象にできない。
fn parent_dir(target: &Path) -> Result<&Path, DocumentError> {
    match target.parent() {
        Some(dir) if dir.as_os_str().is_empty() => Ok(Path::new(".")),
        Some(dir) => Ok(dir),
        None => Err(io_error(
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "対象パスに親ディレクトリが無い（ルートは置換できない）",
            ),
            false,
        )),
    }
}

/// `rename` を [`RENAME_ATTEMPTS`] 回まで試す。再試行するのは共有違反系のエラーコード
/// （[`RETRYABLE_RENAME_CODES`]）だけで、Unix では常に 1 回で終わる。置換先が既存の
/// ディレクトリなら、コードが再試行対象でも再試行しない（Windows はこの場合も
/// `ERROR_ACCESS_DENIED` を返す。[`RETRYABLE_RENAME_CODES`] の docs）。
///
/// 予算を使い切った場合にだけ `retried` を `true` にして返す（[`DocumentError::Io`] の
/// docs にある「保存経路のみが `true` にする」の唯一の実装箇所）。
fn rename_with_retries(staged: &Path, target: &Path) -> Result<(), DocumentError> {
    let mut attempt: u32 = 1;
    loop {
        match fs::rename(staged, target) {
            Ok(()) => return Ok(()),
            Err(err) => {
                if !is_retryable_rename_error(&err) || is_directory(target) {
                    // 待っても直らない失敗（対象がディレクトリ、親が消えた、等）。
                    return Err(io_error(err, false));
                }
                if attempt >= RENAME_ATTEMPTS {
                    // 予算を使い切った。`retried = true` になる唯一の経路である。
                    return Err(io_error(err, true));
                }
                thread::sleep(retry_backoff(attempt));
                attempt += 1;
            }
        }
    }
}

/// 再試行 `attempt` 回目の待機時間（`attempt` は 1 始まり）。
///
/// [`RENAME_FIRST_BACKOFF`] から倍々に延ばし、[`RENAME_MAX_BACKOFF`] で頭打ちにする。
fn retry_backoff(attempt: u32) -> Duration {
    // 上限（2^31 倍）で止めてから飽和させる: `1u32 << 32` の桁溢れを避ける。
    let doublings = attempt.saturating_sub(1).min(31);
    RENAME_FIRST_BACKOFF.saturating_mul(1u32 << doublings).min(RENAME_MAX_BACKOFF)
}

/// `rename` の失敗を再試行すべきか（[`RETRYABLE_RENAME_CODES`] の判定）。
fn is_retryable_rename_error(err: &io::Error) -> bool {
    match err.raw_os_error() {
        Some(code) => RETRYABLE_RENAME_CODES.contains(&code),
        None => false,
    }
}

/// `path` が既存のディレクトリか。シンボリックリンクは辿らない（`rename` はリンクそのもの
/// を置換するため、判定もリンク自身の種別で行う）。種別を取れない場合は偽とし、判定を
/// エラーコードに委ねる。
fn is_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

/// 親ディレクトリのエントリを永続化する（Unix）。
#[cfg(unix)]
fn sync_parent_dir(dir: &Path) -> Result<(), DocumentError> {
    // ディレクトリは読み取り専用で開ける（fsync に書き込み権限は要らない）。
    let handle = fs::File::open(dir).map_err(|err| io_error(err, false))?;
    handle.sync_all().map_err(|err| io_error(err, false))
}

/// Windows にはディレクトリの fsync が無い（置換の永続化は `rename` 自身が担う）。
#[cfg(not(unix))]
fn sync_parent_dir(_dir: &Path) -> Result<(), DocumentError> {
    Ok(())
}

/// I/O エラーを [`DocumentError::Io`] へ包む。`retried` は「`rename` の再試行予算を
/// 使い切った」場合にだけ `true`（渡すのは `rename_with_retries` の 1 箇所）。
fn io_error(source: io::Error, retried: bool) -> DocumentError {
    DocumentError::Io { source, retried }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::{Command, ExitStatus};

    /// 子プロセスへ `--exact` で渡す、このテスト自身のハーネス上の完全名。
    const CRASH_TEST: &str =
        "container::atomic_save::tests::interruption_before_replacement_keeps_the_original_file";

    /// 子プロセスを子モードにする値。**存在ではなく値の厳密一致**で判定する
    /// （外部環境に同名の変数がある場合に子モードへ入り、親の検証が空振りするのを防ぐ）。
    const CHILD_ACTION: &str = "stage-then-abort";
    const CHILD_ACTION_ENV: &str = "JXCEL_ATOMIC_SAVE_CHILD_ACTION";
    const CHILD_TARGET_ENV: &str = "JXCEL_ATOMIC_SAVE_CHILD_TARGET";

    /// 保存前の内容。新内容より長くしておく（切り詰め漏れを検出するため）。
    const ORIGINAL: &[u8] = b"{\"before\":true}\n";

    /// 保存後の内容。親子で同じ関数から作る（子へバイト列を渡さない）。
    fn replacement_bytes() -> Vec<u8> {
        (0..64 * 1024).map(|i| (i % 251) as u8).collect()
    }

    /// ディレクトリのエントリ名を整列して返す（一時ファイルの残留を数える）。
    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("ディレクトリを列挙できない")
            .map(|entry| {
                entry
                    .expect("エントリを読めない")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    /// 保存前の内容を書いた対象ファイル（`document.jxcel`）を作る。
    fn target_with_original(dir: &Path) -> PathBuf {
        let target = dir.join("document.jxcel");
        fs::write(&target, ORIGINAL).expect("保存前の内容を書けない");
        target
    }

    fn read(path: &Path) -> Vec<u8> {
        fs::read(path).expect("読めない")
    }

    /// 置換が成立する前の失敗であること（`Err` かつ `retried` が偽）を確かめる。
    /// Unix には再試行対象の OS エラーコードが無い。Windows で呼ぶのは待っても直らない
    /// 失敗（対象がディレクトリ、親が無い）だけなので、どちらでも `retried` は偽になる
    /// はずである。
    fn assert_failed_before_replacement(err: DocumentError, context: &str) {
        match err {
            DocumentError::Io { retried, .. } => {
                assert!(!retried, "再試行していない失敗で retried が真になった: {context}");
            }
            other => panic!("Io 以外が返った（{context}）: {other:?}"),
        }
    }

    /// 要件 5.6（通常経路）: 既存ファイルの内容が新しいバイト列で**バイト単位で**完全に
    /// 置き換わる。0 バイト・非 UTF-8・大きな列（旧内容より長い）の 3 標本を回す。
    #[test]
    fn commit_replaces_existing_content_byte_for_byte() {
        let large: Vec<u8> = (0..256 * 1024).map(|i| (i % 251) as u8).collect();
        let cases: Vec<&[u8]> = vec![b"", &[0xff, 0x00, 0x80, 0xfe, 0x0a], &large];

        for (index, replacement) in cases.iter().enumerate() {
            let dir = tempfile::tempdir().expect("一時ディレクトリを作れない");
            let target = target_with_original(dir.path());

            AtomicWriter::commit(&target, replacement).expect("置換が失敗した");

            let observed = read(&target);
            assert_eq!(*replacement, observed.as_slice(), "バイト単位で一致しない: 標本 {index}");
            assert_ne!(ORIGINAL, observed.as_slice(), "旧内容のままである: 標本 {index}");
            assert_eq!(
                vec!["document.jxcel".to_string()],
                entries(dir.path()),
                "一時ファイルが残った: 標本 {index}"
            );
        }
    }

    /// 対象が存在しない場合は新規作成する。
    #[test]
    fn commit_creates_a_missing_target() {
        let dir = tempfile::tempdir().expect("一時ディレクトリを作れない");
        let target = dir.path().join("new.jxcel");
        assert!(!target.exists(), "前提: 対象が存在しない");

        AtomicWriter::commit(&target, &replacement_bytes()).expect("新規作成が失敗した");

        assert_eq!(replacement_bytes(), read(&target));
        assert_eq!(vec!["new.jxcel".to_string()], entries(dir.path()));
    }

    /// 同じ内容での 2 回の置換は同じ結果になり、エントリも増えない（冪等）。
    #[test]
    fn repeated_commit_is_idempotent() {
        let dir = tempfile::tempdir().expect("一時ディレクトリを作れない");
        let target = target_with_original(dir.path());
        let replacement = replacement_bytes();

        AtomicWriter::commit(&target, &replacement).expect("1 回目が失敗した");
        let first = read(&target);
        AtomicWriter::commit(&target, &replacement).expect("2 回目が失敗した");
        let second = read(&target);

        assert_eq!(first, second, "同じ内容の 2 回の置換で結果が変わる");
        assert_eq!(replacement, second);
        assert_eq!(vec!["document.jxcel".to_string()], entries(dir.path()));
    }

    /// 置換は**ディレクトリエントリの差し替え**であり、対象の中身を書き換えるのではない。
    /// 同じ inode を指す別名（ハードリンク）は保存前の内容を読み続ける。対象を開いたまま
    /// 書き換える実装（一時ファイルを使わない実装）はここで落ちる。
    #[test]
    fn commit_swaps_the_directory_entry_instead_of_rewriting_the_inode() {
        let dir = tempfile::tempdir().expect("一時ディレクトリを作れない");
        let target = target_with_original(dir.path());
        let alias = dir.path().join("alias.jxcel");
        fs::hard_link(&target, &alias).expect("ハードリンクを作れない");
        let replacement = replacement_bytes();

        AtomicWriter::commit(&target, &replacement).expect("置換が失敗した");

        assert_eq!(replacement, read(&target), "対象の内容が置換されていない");
        assert_eq!(
            ORIGINAL,
            read(&alias).as_slice(),
            "同じ inode を指す別名が書き換えられた（エントリの差し替えになっていない）"
        );
        assert_eq!(
            vec!["alias.jxcel".to_string(), "document.jxcel".to_string()],
            entries(dir.path()),
            "一時ファイルが残った"
        );
    }

    /// 置換が成立した後は `Err` を返さない（不変条件: `Err` ⇒ 対象は保存前のまま）。
    /// 置換の後に走る親ディレクトリの同期が失敗しても `Ok(())` であり、対象は**新しい
    /// 内容**に置換されている。同期段には失敗を注入する（本番経路も同じ
    /// `commit_with_sync` → `install` を通る）。
    ///
    /// この握り潰しで落ちうるのは置換の永続化だけである（直後の電源断で置換が失われうる）
    /// ため、対象が部分的に書かれた状態にならないこと（= 完全な新内容であること）も
    /// 併せて確かめる。
    #[test]
    fn failure_of_the_post_replacement_sync_still_succeeds() {
        let dir = tempfile::tempdir().expect("一時ディレクトリを作れない");
        let target = target_with_original(dir.path());
        let replacement = replacement_bytes();
        let mut synced: Option<PathBuf> = None;

        let outcome = commit_with_sync(&target, &replacement, |sync_target| {
            synced = Some(sync_target.to_path_buf());
            Err(DocumentError::Io {
                source: io::Error::other("注入した親ディレクトリ同期の失敗"),
                retried: false,
            })
        });

        match outcome {
            Ok(()) => {}
            Err(err) => panic!("置換成立後の同期失敗が Err になった: {err:?}"),
        }
        assert_eq!(
            Some(dir.path().to_path_buf()),
            synced,
            "同期段が対象の親ディレクトリで呼ばれていない"
        );
        assert_eq!(replacement, read(&target), "置換が成立していない（部分的な内容が疑われる）");
        assert_eq!(
            vec!["document.jxcel".to_string()],
            entries(dir.path()),
            "一時ファイルが残った"
        );
    }

    /// 要件 5.6（失敗経路 1）: 対象がディレクトリのときは置換が失敗し（Unix では
    /// `EISDIR`、Windows では `ERROR_ACCESS_DENIED`）、そのディレクトリと、中に在る既存
    /// ファイル、兄弟ファイルは 1 バイトも変わらない。失敗しても一時ファイルは残らない。
    /// Windows では再試行対象のコードが返るが、恒久的な失敗なので再試行しない
    /// （`retried` が偽）。
    #[test]
    fn failure_on_a_directory_target_keeps_it_untouched() {
        let dir = tempfile::tempdir().expect("一時ディレクトリを作れない");
        let sibling = dir.path().join("sibling.txt");
        fs::write(&sibling, b"sibling").expect("兄弟ファイルを書けない");
        let occupied = dir.path().join("occupied");
        fs::create_dir(&occupied).expect("ディレクトリを作れない");
        fs::write(occupied.join("inner.txt"), b"inner").expect("中身を書けない");

        let err = AtomicWriter::commit(&occupied, b"replacement")
            .expect_err("ディレクトリへの置換が成功してしまった");

        assert_failed_before_replacement(err, "対象がディレクトリ");
        assert!(occupied.is_dir(), "対象のディレクトリが消えた");
        assert_eq!(
            b"inner",
            read(&occupied.join("inner.txt")).as_slice(),
            "対象ディレクトリの中のファイルが変わった"
        );
        assert_eq!(b"sibling", read(&sibling).as_slice(), "兄弟ファイルが変わった");
        assert_eq!(
            vec!["occupied".to_string(), "sibling.txt".to_string()],
            entries(dir.path()),
            "失敗時に一時ファイルが残った"
        );
    }

    /// 要件 5.6（失敗経路 2）: 親ディレクトリが存在しない場合は `Err` であり、
    /// ディレクトリを作らない。既存のファイルも一切変わらない。
    #[test]
    fn missing_parent_directory_is_an_error_and_creates_nothing() {
        let dir = tempfile::tempdir().expect("一時ディレクトリを作れない");
        let sibling = dir.path().join("sibling.txt");
        fs::write(&sibling, b"sibling").expect("兄弟ファイルを書けない");
        let missing = dir.path().join("missing");

        let err = AtomicWriter::commit(&missing.join("document.jxcel"), &replacement_bytes())
            .expect_err("親ディレクトリが無いのに成功した");

        assert_failed_before_replacement(err, "親ディレクトリが無い");
        assert!(!missing.exists(), "親ディレクトリが作られた");
        assert_eq!(b"sibling", read(&sibling).as_slice(), "既存のファイルが変わった");
        assert_eq!(vec!["sibling.txt".to_string()], entries(dir.path()));
    }

    /// 要件 5.6（失敗経路 3）: 対象ファイルは実在し、その親ディレクトリも書き込み可能で
    /// ある。しかし**一時ファイル名を足すとパス長の上限（`PATH_MAX`）を超える**ため、
    /// 置換の第 1 段が失敗する。対象は 1 バイトも変わらず、一時ファイルも残らない。
    ///
    /// この標本は、テストプロセスが特権を持つ場合でも成立する第 1 段の失敗である
    /// （読み取り専用ディレクトリは root には効かない）。対象パスへ直接書く実装はここで
    /// 落ちる: 対象パスは上限に収まっているので、書き込みは成功してしまう。
    #[cfg(target_os = "linux")]
    #[test]
    fn staging_failure_keeps_an_existing_target_untouched() {
        /// Linux の `PATH_MAX` は終端 NUL を含めて 4096。
        const PATH_LIMIT: usize = 4096;
        /// 対象ファイル名（1 文字。上限までの余白を一時ファイル側に寄せるため短くする）。
        const TARGET_NAME: &str = "t";

        let dir = tempfile::tempdir().expect("一時ディレクトリを作れない");
        let parent = deep_directory(dir.path(), PATH_LIMIT - 11);
        let parent_len = parent.as_os_str().len();
        assert!(
            parent_len + 1 + TARGET_NAME.len() < PATH_LIMIT,
            "標本の前提が壊れた: 対象パスが上限を超えている（{parent_len} バイト）"
        );
        assert!(
            parent_len + 1 + TEMP_FILE_PREFIX.len() + 8 >= PATH_LIMIT,
            "標本の前提が壊れた: 一時ファイルパスが上限に収まってしまう（{parent_len} バイト）"
        );

        let target = parent.join(TARGET_NAME);
        fs::write(&target, ORIGINAL).expect("保存前の内容を書けない");

        let err = AtomicWriter::commit(&target, b"replacement")
            .expect_err("一時ファイルを作れないパスで成功してしまった");

        assert_failed_before_replacement(err, "一時ファイルを作れない（パス長の上限）");
        assert_eq!(ORIGINAL, read(&target).as_slice(), "失敗したのに既存の対象が変わった");
        assert_eq!(
            vec![TARGET_NAME.to_string()],
            entries(&parent),
            "失敗時に一時ファイルが残った"
        );
    }

    /// 長さ `len` バイトのディレクトリパスを作って返す（各成分はファイル名長の上限
    /// 255 未満に収める）。
    #[cfg(target_os = "linux")]
    fn deep_directory(root: &Path, len: usize) -> PathBuf {
        let mut path = root.to_path_buf();
        loop {
            let remaining = len.saturating_sub(path.as_os_str().len());
            if remaining <= 1 {
                break;
            }
            path.push("a".repeat((remaining - 1).min(254)));
        }
        fs::create_dir_all(&path).expect("深いディレクトリを作れない");
        path
    }

    /// 対象にできないパス（親を持たない = ルート）は、書き込みを試みずに `Err` になる。
    #[test]
    fn a_path_without_a_parent_is_rejected() {
        let err = AtomicWriter::commit(Path::new("/"), b"replacement")
            .expect_err("ルートへの置換が成功してしまった");

        assert_failed_before_replacement(err, "親を持たないパス");
    }

    /// カレントディレクトリ直下を指す名前（親が空パス）は、一時ファイルの置き場を
    /// カレントディレクトリとして扱う。空パスのままディレクトリを開こうとすると
    /// 置換が成立した後の親 fsync が失敗し、`Err` を返しながら対象は新内容になっている
    /// という最悪の組み合わせになる。
    #[test]
    fn a_name_without_a_directory_is_staged_in_the_current_directory() {
        assert_eq!(
            Path::new("."),
            parent_dir(Path::new("document.jxcel")).expect("親ディレクトリを取れない")
        );
        assert_eq!(
            Path::new("/tmp"),
            parent_dir(Path::new("/tmp/document.jxcel")).expect("親ディレクトリを取れない")
        );
        assert!(parent_dir(Path::new("/")).is_err(), "親を持たないパスが受理された");
    }

    /// 要件 5.6 の中核（design「AtomicWriter / Validation」）: 一時ファイルを書き終えた後・
    /// 置換の前にプロセスが落ちても、対象ファイルは保存前の内容のままである。
    ///
    /// 子プロセスはこのテストバイナリ自身を再実行し、本番の第 1 段（`stage`）だけを実行して
    /// から `abort` する。親は (1) 子がシグナルで落ちたこと、(2) 対象が保存前の内容のまま
    /// であること、(3) 書き終えた一時ファイルが**対象と同一ディレクトリ**に残っていること
    /// （＝落ちた時点が置換の直前だったこと）を確かめる。
    #[test]
    fn interruption_before_replacement_keeps_the_original_file() {
        maybe_run_as_crash_child();

        let dir = tempfile::tempdir().expect("一時ディレクトリを作れない");
        // 子の作業ディレクトリ。`abort` がコアダンプを書く環境でも、リポジトリや対象
        // ディレクトリを汚さないようにする。
        let scratch = tempfile::tempdir().expect("一時ディレクトリを作れない");
        let target = target_with_original(dir.path());

        let child = Command::new(std::env::current_exe().expect("テストバイナリのパスを得られない"))
            .args(["--exact", CRASH_TEST, "--nocapture"])
            .current_dir(scratch.path())
            .env(CHILD_ACTION_ENV, CHILD_ACTION)
            .env(CHILD_TARGET_ENV, &target)
            .output()
            .expect("子プロセスを起動できない");

        assert!(
            died_abnormally(child.status),
            "子が異常終了していない（子モードに入っていない可能性がある）: status={:?}, \
             stdout={}, stderr={}",
            child.status,
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );

        // (1) 対象は保存前の内容のまま。
        assert_eq!(
            ORIGINAL,
            read(&target).as_slice(),
            "置換の前に落ちたのに既存の対象が変わった"
        );

        // (2) 書き終えた一時ファイルが対象と同一ディレクトリに残っている。
        let names = entries(dir.path());
        assert_eq!(2, names.len(), "対象ディレクトリのエントリが想定と違う: {names:?}");
        let staged_name = names
            .iter()
            .find(|name| name.as_str() != "document.jxcel")
            .expect("一時ファイルが残っていない");
        assert!(
            staged_name.starts_with(TEMP_FILE_PREFIX),
            "残ったファイルが一時ファイルの命名規約に合わない: {staged_name}"
        );
        assert_eq!(
            replacement_bytes(),
            read(&dir.path().join(staged_name)),
            "置換の前の段で書き切れていない（一時ファイルの内容が保存後と違う）"
        );
    }

    /// 子プロセスの振る舞い。環境変数の値が [`CHILD_ACTION`] と厳密一致するときだけ子モードに
    /// 入り、本番の第 1 段（`stage`）を実行してから `abort` する（戻らない）。
    fn maybe_run_as_crash_child() {
        if std::env::var(CHILD_ACTION_ENV).as_deref() != Ok(CHILD_ACTION) {
            return;
        }
        let target =
            PathBuf::from(std::env::var(CHILD_TARGET_ENV).expect("子モードには対象パスが必要"));
        let _staged = stage(&target, &replacement_bytes()).expect("第 1 段が失敗した");
        // 置換（`install`）へは進まず、プロセスごと落ちる。`Drop` による一時ファイルの
        // 削除も走らないので、書き終えた一時ファイルは残る。
        std::process::abort();
    }

    /// 異常終了（`abort`）したか。テストハーネス自身の失敗（終了コード 101）と区別する
    /// ため、Unix ではシグナル死かどうかで判定する。
    #[cfg(unix)]
    fn died_abnormally(status: ExitStatus) -> bool {
        use std::os::unix::process::ExitStatusExt;
        status.signal().is_some()
    }

    #[cfg(not(unix))]
    fn died_abnormally(status: ExitStatus) -> bool {
        !status.success()
    }

    /// Windows の再試行は待機を挟み、その待機は倍々に延びて上限で頭打ちになる
    /// （即時再試行・一定間隔・上限の無い伸びを落とす）。
    #[test]
    fn rename_retry_backoff_grows_and_is_capped() {
        assert_eq!(RENAME_FIRST_BACKOFF, retry_backoff(1), "1 回目の待機が初期待機でない");
        assert!(retry_backoff(2) > retry_backoff(1), "2 回目で待機が延びない");
        assert!(retry_backoff(3) > retry_backoff(2), "3 回目で待機が延びない");

        for attempt in 1..RENAME_ATTEMPTS {
            let current = retry_backoff(attempt);
            assert!(!current.is_zero(), "待機が 0 になった: attempt={attempt}");
            assert!(current <= RENAME_MAX_BACKOFF, "上限を超えた: attempt={attempt}");
            assert!(
                current <= retry_backoff(attempt + 1),
                "待機が縮んだ: attempt={attempt}"
            );
        }
        assert_eq!(RENAME_MAX_BACKOFF, retry_backoff(RENAME_ATTEMPTS), "上限で頭打ちにならない");
        assert_eq!(RENAME_MAX_BACKOFF, retry_backoff(u32::MAX), "大きな試行回で飽和しない");
    }

    /// この環境（Unix）では、どの OS エラーコードも再試行の対象にならない。Windows の
    /// コード値を Unix の `errno` と取り違えて再試行する実装はここで落ちる。
    #[cfg(not(windows))]
    #[test]
    fn no_os_error_is_retryable_on_unix() {
        for code in [2, 5, 13, 32, 33] {
            assert!(
                !is_retryable_rename_error(&io::Error::from_raw_os_error(code)),
                "errno {code} を再試行対象にしている"
            );
        }
        assert!(
            !is_retryable_rename_error(&io::Error::other("OS コードを持たない失敗")),
            "OS コードを持たない失敗を再試行対象にしている"
        );
    }

    /// Windows では共有違反・ロック違反・アクセス拒否（他プロセスのハンドル保持で返る）を
    /// 再試行の対象にする。
    #[cfg(windows)]
    #[test]
    fn sharing_violations_are_retryable_on_windows() {
        for code in [5, 32, 33] {
            assert!(
                is_retryable_rename_error(&io::Error::from_raw_os_error(code)),
                "共有違反系のコード {code} を再試行していない"
            );
        }
        assert!(
            !is_retryable_rename_error(&io::Error::from_raw_os_error(2)),
            "存在しないパスを再試行対象にしている"
        );
    }

    /// 再試行を打ち切る置換先の判定: 既存のディレクトリだけが真で、ファイルと存在しない
    /// パスは偽（判定をエラーコードに委ねる）。Windows でコード 5 を返す 2 つの状況
    /// （共有違反とディレクトリ）を分ける根拠であり、Unix でも同じ判定になることを確かめる。
    #[test]
    fn only_an_existing_directory_stops_the_retries() {
        let dir = tempfile::tempdir().expect("一時ディレクトリを作れない");
        let file = target_with_original(dir.path());

        assert!(is_directory(dir.path()), "ディレクトリを判定できない");
        assert!(!is_directory(&file), "ファイルをディレクトリとした");
        assert!(
            !is_directory(&dir.path().join("missing")),
            "存在しないパスをディレクトリとした"
        );
    }
}

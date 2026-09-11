//! 設定ファイルの原子的な置き換え（要件 7.1。design.md「SettingsStore」の不変条件）。
//!
//! 手順は固定である:
//!
//! 1. 対象と**同一ディレクトリ**に一時ファイルを作る（同一ファイルシステム上の `rename` で
//!    なければ置換は原子的にならない）
//! 2. バイト列を書き切り、`flush` してから `sync_all` で内容を永続化する
//! 3. `rename` で対象を置き換える（Windows の `MoveFileExW` は既存の対象を置き換える）
//! 4. Unix では親ディレクトリを fsync して、置換というエントリの変更それ自体を永続化する
//!
//! truncate-then-write は使わない。`tauri-plugin-store` の `Store::save()` が `fs::write`
//! （切り詰めてから書く）であり、クラッシュで切り詰められた JSON が残ることは採用を見送った
//! 理由の 1 つである（research.md 決定 6）。ここでは対象を直接開かない。
//!
//! # 保証
//!
//! - [`replace`] が `Err` を返したとき、対象は**変更されていない**（直前の完全な内容の
//!   ままであり、新しく作られもしない）。一時ファイルは `Drop` が削除する。
//! - [`replace`] が `Ok` を返したとき、対象は `bytes` の完全な内容である。
//! - 上記の間のどこかでプロセスが落ちた場合、対象には直前の完全な内容か新しい完全な内容の
//!   いずれかが残る（一時ファイルが残ることはあるが、対象は部分的な内容にならない）。
//! - 一時ファイル名は識別子とプロセス内連番から作り、`create_new`（`O_EXCL`）で作るため、
//!   同時に走る複数の書き手（スレッド・プロセス）の間で衝突しない。
//!
//! `rename` が成立した後に `Err` を返す経路は無い。置換の後に続く親ディレクトリの同期
//! （Unix）の失敗は `Ok` として握り潰す — 対象は既に置換済みであり、そこで `Err` を返すと
//! 「`Err` ⇒ 対象は無変更」という呼び出し元が依存する不変条件が破れるためである
//! （失われうるのは置換の永続化だけであり、部分的に書かれたファイルは決して残らない）。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 一時ファイル名の接頭辞。
///
/// 対象ファイル名からは作らない（対象名に依存しないので、名前の衝突を対象と共有しない）。
/// この接頭辞はテストが「書き込みの途中で残る一時ファイル」を識別するためにも使う
/// （`crates/app-shell/tests/settings_store.rs`）。
pub const TEMP_FILE_PREFIX: &str = ".jxcel-settings-atomic-";

/// プロセス内で一時ファイル名を一意にするための連番。
///
/// プロセス識別子と組にすることで、同じプロセスの複数スレッドが同時に書いても衝突しない。
/// 前回の実行が落ちて残った同名の一時ファイルがある場合に備え、[`TEMP_CREATE_ATTEMPTS`] 回まで
/// 連番を進めて作り直す。
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// 一時ファイルの作成を試す回数。
const TEMP_CREATE_ATTEMPTS: u32 = 16;

/// `bytes` を `target` へ原子的に置き換える（要件 7.1）。
///
/// # Errors
///
/// 一時ファイルを作れない・書けない・同期できない、または `rename` できない場合に `io::Error`
/// を返す。その場合 `target` は変更されていない。
pub fn replace(target: &Path, bytes: &[u8]) -> io::Result<()> {
    replace_with(target, |file| file.write_all(bytes))
}

/// [`replace`] の内容を呼び出し側が供給する形（要件 7.1）。
///
/// 診断の書き出し（tasks.md 4.5、`diagnostics::export`）が、連結する記録の合計をメモリに
/// 載せずに同じ保証 — 一時ファイル → `sync_all` → `rename` — を使うための入口である。
/// `write` には**対象と同一ディレクトリに作った一時ファイル**が渡され、`write` が戻った後で
/// `flush` と `sync_all` を行ってから `rename` する。
///
/// # Errors
///
/// 一時ファイルを作れない、`write` が失敗した、同期できない、または `rename` できない場合に
/// `io::Error` を返す。その場合 `target` は変更されていない（`write` が書いた途中の内容は
/// 一時ファイルとともに破棄される）。
pub fn replace_with<F>(target: &Path, write: F) -> io::Result<()>
where
    F: FnOnce(&mut File) -> io::Result<()>,
{
    let directory = parent_dir(target)?;
    let (temp_path, mut file) = create_temp(directory)?;
    // 以降の失敗経路（書き込み・同期・置換のいずれか）で一時ファイルを残さない。
    let mut guard = TempGuard {
        path: temp_path.clone(),
        armed: true,
    };

    write(&mut file)?;
    // `File` に対する `flush` は実質的に何もしないが、「書き切ってから同期する」という
    // 意図をコード上で明示しておく（永続化を担うのは `sync_all` である）。
    file.flush()?;
    file.sync_all()?;
    drop(file);

    fs::rename(&temp_path, target)?;
    guard.armed = false;

    // 置換の後に続く同期。ここで失敗しても `Err` を返さない（上の docs「保証」を参照）。
    let _ = sync_dir(directory);
    Ok(())
}

/// 失敗したとき（および `Drop` が走る限り）に一時ファイルを削除するガード。
struct TempGuard {
    path: PathBuf,
    armed: bool,
}

impl Drop for TempGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// 対象の親ディレクトリを返す。カレントディレクトリ直下の名前（親が空）は `.` へ正規化する。
fn parent_dir(target: &Path) -> io::Result<&Path> {
    match target.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Ok(Path::new(".")),
        Some(parent) => Ok(parent),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "設定ファイルのパスに親ディレクトリが無い",
        )),
    }
}

/// 対象と同一のディレクトリに、衝突しない一時ファイルを作る。
fn create_temp(directory: &Path) -> io::Result<(PathBuf, File)> {
    let process = std::process::id();
    for _ in 0..TEMP_CREATE_ATTEMPTS {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!("{TEMP_FILE_PREFIX}{process}-{sequence}"));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            // 前回の実行が残した同名のファイル。連番を進めてやり直す。
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "一時ファイル名の衝突が解消しない",
    ))
}

/// 親ディレクトリのエントリを永続化する（Unix）。
///
/// ディレクトリは読み取り専用で開ける（fsync に書き込み権限は要らない）。
#[cfg(unix)]
fn sync_dir(directory: &Path) -> io::Result<()> {
    File::open(directory)?.sync_all()
}

/// Windows にはディレクトリの fsync が無い（置換の永続化は `rename` 自身が担う）。
#[cfg(not(unix))]
fn sync_dir(_directory: &Path) -> io::Result<()> {
    Ok(())
}

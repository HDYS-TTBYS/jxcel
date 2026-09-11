//! 診断の保持方針・秘匿規則（要件 8.1、8.3、8.4、8.5）。
//!
//! **方針値だけを持ち、記録機構そのものは持たない。** 記録機構（`tauri-plugin-log`）の登録と
//! 保持方針の適用はアダプタ層（`AppLifecycle`、tasks.md 5.2）の仕事であり、本モジュールは
//! Tauri 非依存を保つ（design.md「DiagnosticsPolicy」）。
//!
//! - **保存先（要件 8.1）**: [`log_dir`] が各 OS の規約で記録の置き場所を返す。規約は採用する
//!   機構の既定ディレクトリと同じであり（research.md「ログと設定の永続化」）、アダプタが登録
//!   する場所と方針が食い違わない。Linux はアプリケーションデータ領域と一致するため 4.1 の
//!   [`settings::app_data_base_dir_with`] を再利用し、macOS / Windows はログ領域が
//!   アプリケーションデータ領域と別なので明示的に解決する（[`macos_log_dir_with`] /
//!   [`windows_log_dir_with`]）。
//! - **保持（要件 8.5）**: 合計の上限 50 MB（= 50,000,000 B。通常の十進の読みで確実に
//!   50 MB 以下）を [`MAX_TOTAL_LOG_BYTES`] としてここで定義する。**適用はアダプタの仕事で
//!   ある** — 5.2 が [`MAX_LOG_FILE_BYTES`] を機構の `max_file_size` へ、
//!   [`KEEP_SOME_ARCHIVED_FILES`] を `RotationStrategy::KeepSome` へ渡し、既定（40 KB /
//!   `KeepOne`。要件の上限と桁が違う。research.md 決定 6）を上書きする。**`KeepSome(n)` は
//!   記録中の現行ファイルを含まない**ため保持総数は n + 1 であり、[`MAX_RETAINED_LOG_BYTES`]
//!   はその前提で合計上限以下に収まる。
//! - **秘匿（要件 8.4）**: ドキュメントのセル値とスキーマの内容は [`Redacted`] を通してしか
//!   記録経路へ渡せない。[`recorded_value`] が記録に載せる唯一の入口であり、内側の値は保持も
//!   公開もされない。`Display` / `Debug` / `Serialize` を実装しないことは doc test の
//!   `compile_fail` が機械検査する。
//! - **外部送信なし（要件 8.3）**: 本モジュールはネットワーク I/O を行わず、ローカルパスの
//!   解決だけを担う。宛先（URL 等）を受け取る公開関数は無く、`crates/app-shell` の依存集合に
//!   HTTP クライアントは無い（Cargo.toml と `cargo tree -p app-shell`）。書き出し（tasks.md
//!   4.5）もローカルの出力先パスを受け取る形にする。
//! - **書き出し（要件 8.6）**: [`export`] が記録ディレクトリの記録を **1 つのファイル**へ
//!   まとめる。[`export`] を参照。
//! - **詳細度（要件 8.7）**: [`DiagnosticsLevel`] が記録の詳細度であり、設定の鍵
//!   [`SettingsKey::DiagnosticsLevel`] の下に保存する。機構への適用（詳細度の対応付け）は
//!   アダプタ層（5.2）の仕事である。

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::settings::atomic;
use crate::settings::{self, SettingsError, SettingsKey, SettingsStore, APP_IDENTIFIER};

// ---------------------------------------------------------------------------
// 保持方針の値（要件 8.5）
// ---------------------------------------------------------------------------

/// 識別子の直下に置く記録ディレクトリの名前。
///
/// Linux / Windows は `{識別子}/logs`、macOS は `~/Library/Logs/{識別子}` である
/// （research.md「ログと設定の永続化」）。この接尾辞が付くのは前者だけである。
pub const LOG_DIR_NAME: &str = "logs";

/// 記録に載る固定の表現。内側の値は決して現れない（要件 8.4、[`recorded_value`]）。
pub const REDACTION_PLACEHOLDER: &str = "[REDACTED]";

/// 保持する記録の合計サイズの上限（要件 8.5）。
///
/// 50 MB = 50,000,000 B（十進）。要件 8.5 の「50 MB 以下」を通常の十進の読みで確実に満たす
/// ため、二進（MiB）ではなく十進で定義する。**方針値であり、機構への適用はアダプタ層が行う**
/// （tasks.md 5.2）。保持総数がこの値を超えないことは [`MAX_RETAINED_LOG_BYTES`] と
/// コンパイル時検査が保証する。
pub const MAX_TOTAL_LOG_BYTES: u64 = 50_000_000;

/// 1 ファイルあたりの上限。5.2 が機構の `max_file_size` へ渡す。
///
/// 8 MB = 8,000,000 B（十進）。採用する機構の既定 `40_000`（40 KB）の 200 倍であり、既定の
/// ままだと要件 8.5 の上限に対して記録がほとんど残らない（research.md 決定 6）。
pub const MAX_LOG_FILE_BYTES: u64 = 8_000_000;

/// 保持する**アーカイブ済み**ファイル数。5.2 が `RotationStrategy::KeepSome` へ渡す値である。
///
/// **`KeepSome(n)` が保持するのは n 個のアーカイブであり、記録中の現行ファイルは含まない。**
/// プラグインは現行ファイルをリネームする直前に `remove_old_files(keep_count - 1)` を呼び、
/// 起動時に `remove_old_files(keep_count)` を呼ぶ（`tauri-plugin-log` 2.9.1）。したがって
/// 保持される総ファイル数は [`RETAINED_LOG_FILES`] = n + 1 である。
pub const KEEP_SOME_ARCHIVED_FILES: u32 = 5;

/// ローテーション後に保持されうる総ファイル数（アーカイブ 5 + 記録中の現行 1）。
pub const RETAINED_LOG_FILES: u64 = KEEP_SOME_ARCHIVED_FILES as u64 + 1;

/// 適用後に保持されうる合計の上限（[`MAX_LOG_FILE_BYTES`] × [`RETAINED_LOG_FILES`]）。
///
/// 5 + 1 = 6 ファイル × 8 MB = 48,000,000 B であり、合計上限 50,000,000 B に対して 2,000,000 B
/// の余裕を持つ（サイズ判定が書き込み単位で行われるため現行ファイルが上限をわずかに超えうる
/// 分を吸収する）。方針値どうしの整合は下のコンパイル時検査が保証する。
pub const MAX_RETAINED_LOG_BYTES: u64 = MAX_LOG_FILE_BYTES * RETAINED_LOG_FILES;

// 保持総数（アーカイブ + 現行）× 1 ファイル上限が合計上限を超えたら、このクレートは
// コンパイルできない（片方だけを変える事故を型ではなくビルドで止める）。
const _: () = assert!(MAX_RETAINED_LOG_BYTES <= MAX_TOTAL_LOG_BYTES);

// ---------------------------------------------------------------------------
// 保存先の解決（要件 8.1）
// ---------------------------------------------------------------------------

/// 実機の OS 規約で記録の保存先を返す（要件 8.1）。
///
/// 返すのは場所だけであり、**ディレクトリの作成は行わない** — 作成は記録機構の登録（5.2）の
/// 仕事である。
///
/// # Errors
///
/// 必要な環境変数が無い（または空の）とき [`DiagnosticsError::LogDirUnavailable`] を返す。
/// **パニックしない。**
pub fn log_dir() -> Result<PathBuf, DiagnosticsError> {
    log_dir_with(&|name: &str| std::env::var_os(name))
}

/// [`log_dir`] の環境変数の読み取りを差し替えられる形。
///
/// 4.1 の [`settings::app_data_base_dir_with`] と同じ試験用の入口である。値が与えられていない
/// 場合と空の場合をどちらも「未設定」として扱う。
///
/// Linux の記録領域はアプリケーションデータ領域の下の `{識別子}/logs` であり、4.1 の解決を
/// そのまま再利用する（規約が一致する唯一の OS である）。macOS / Windows のログ領域は
/// アプリケーションデータ領域と別であるため、それぞれ [`macos_log_dir_with`] /
/// [`windows_log_dir_with`] が明示的に解決する。
pub fn log_dir_with(lookup: &dyn Fn(&str) -> Option<OsString>) -> Result<PathBuf, DiagnosticsError> {
    #[cfg(target_os = "linux")]
    {
        return linux_log_dir(lookup);
    }

    #[cfg(target_os = "macos")]
    {
        return macos_log_dir_with(lookup);
    }

    #[cfg(windows)]
    {
        return windows_log_dir_with(lookup);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = lookup;
        Err(DiagnosticsError::LogDirUnavailable {
            reason: "この OS の記録の保存先の規約を持たない".to_owned(),
        })
    }
}

/// Linux の規約: `$XDG_DATA_HOME/{識別子}/logs`、無ければ `$HOME/.local/share/{識別子}/logs`。
///
/// アプリケーションデータ領域と一致するため、4.1 の [`settings::app_data_base_dir_with`] を
/// 再利用する（識別子の足し方も 4.1 の設定ディレクトリと同じ形である）。
#[cfg(target_os = "linux")]
fn linux_log_dir(lookup: &dyn Fn(&str) -> Option<OsString>) -> Result<PathBuf, DiagnosticsError> {
    let base = settings::app_data_base_dir_with(lookup)
        .map_err(|source| DiagnosticsError::LogDirUnavailable { reason: source.to_string() })?;
    Ok(base.join(APP_IDENTIFIER).join(LOG_DIR_NAME))
}

/// macOS の規約: `$HOME/Library/Logs/{識別子}`（要件 8.1）。
///
/// **アプリケーションデータ領域（`$HOME/Library/Application Support`）とは別である。** 記録は
/// ログ領域に置かれ、機構の既定もそこを指す（research.md「ログと設定の永続化」）。この関数は
/// ホスト OS に依存しないので、どの OS のテストからもこの規約を検査できる。
///
/// # Errors
///
/// `HOME` が無い（または空の）とき [`DiagnosticsError::LogDirUnavailable`] を返す。
pub fn macos_log_dir_with(
    lookup: &dyn Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, DiagnosticsError> {
    let home = settings::non_empty_env(lookup, "HOME").ok_or_else(|| {
        DiagnosticsError::LogDirUnavailable { reason: "HOME が設定されていない".to_owned() }
    })?;
    Ok(home.join("Library").join("Logs").join(APP_IDENTIFIER))
}

/// Windows の規約: `%LOCALAPPDATA%/{識別子}/logs`（要件 8.1）。
///
/// **設定のアプリケーションデータ領域（ローミングの `%APPDATA%`）とは別である。** 記録は
/// ローカルのアプリデータに置かれ、機構の既定もそこを指す — ローミングプロファイルへ記録を
/// 同期させない（research.md「ログと設定の永続化」）。この関数はホスト OS に依存しないので、
/// どの OS のテストからもこの規約を検査できる。
///
/// # Errors
///
/// `LOCALAPPDATA` が無い（または空の）とき [`DiagnosticsError::LogDirUnavailable`] を返す。
pub fn windows_log_dir_with(
    lookup: &dyn Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, DiagnosticsError> {
    let local_app_data = settings::non_empty_env(lookup, "LOCALAPPDATA").ok_or_else(|| {
        DiagnosticsError::LogDirUnavailable {
            reason: "LOCALAPPDATA が設定されていない".to_owned(),
        }
    })?;
    Ok(local_app_data.join(APP_IDENTIFIER).join(LOG_DIR_NAME))
}

// ---------------------------------------------------------------------------
// 詳細度（要件 8.7）
// ---------------------------------------------------------------------------

/// 記録の詳細度（要件 8.7）。値は [`SettingsKey::DiagnosticsLevel`] の下に保存する。
///
/// 詳細度の昇順は [`Ord`] が表す（`Off` < `Error` < `Warn` < `Info` < `Debug` < `Trace`）。
/// 設定ファイルには `serde` の小文字表現（`"off"` … `"trace"`）で載る。
///
/// **既定は [`DiagnosticsLevel::Info`] である。** 未設定の利用者にとって妥当な量であり、
/// 採用する記録機構の既定（`log::LevelFilter::Trace`。research.md 決定 6）より静かである。
/// 5.2 は起動時に [`DiagnosticsLevel::from_store`] を読み、この列挙を機構の
/// `log::LevelFilter` へ**そのまま 1 対 1 で**対応付けて適用する（`Off` → `Off`、`Error` →
/// `Error`、…、`Trace` → `Trace`）。本モジュールは `log` クレートに依存しないため、対応付けは
/// アダプタ層が持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticsLevel {
    /// 記録しない（`log::LevelFilter::Off`）。
    Off,
    /// 失敗だけを記録する。
    Error,
    /// 失敗と警告を記録する。
    Warn,
    /// 失敗・警告・通常の出来事を記録する（既定）。
    Info,
    /// 開発時の詳細を記録する。
    Debug,
    /// 最も細かい記録。
    Trace,
}

impl Default for DiagnosticsLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl DiagnosticsLevel {
    /// 設定から詳細度を読む（要件 8.7）。
    ///
    /// 鍵が無い場合と、保存されている値を [`DiagnosticsLevel`] として解釈できない場合
    /// （別の版が書いた未知の名前、数値など）はどちらも[既定値](DiagnosticsLevel::default)を
    /// 返す。**解釈できないことは失敗ではない** — 4.2 の復旧（既定値で起動して事実を報告する）
    /// と同じ精神であり、呼び出し側はパニックも `Err` も扱わなくてよい。この関数は保存された
    /// 値を書き戻さない（解釈できない値もそのまま残る）。
    pub fn from_store(store: &impl SettingsStore) -> Self {
        store.get::<DiagnosticsLevel>(&SettingsKey::DiagnosticsLevel).unwrap_or_default()
    }

    /// 詳細度を設定へ書く（要件 8.7）。
    ///
    /// 戻った時点で値はメモリとディスクの両方にあり、値が実際に変わる場合は
    /// [`SettingsStore::subscribe`] の購読者へ通知される（[`SettingsStore::set`] の契約）。
    ///
    /// # Errors
    ///
    /// 保存に失敗した場合（[`SettingsError`]）。この場合、設定は変更前のままである。
    pub fn write_to(self, store: &impl SettingsStore) -> Result<(), SettingsError> {
        store.set(&SettingsKey::DiagnosticsLevel, &self)
    }
}

// ---------------------------------------------------------------------------
// 書き出し（要件 8.6）
// ---------------------------------------------------------------------------

/// 記録ファイルとして扱う拡張子。
///
/// 記録機構（`tauri-plugin-log`）は記録中の `{名前}.log` とローテーション済みの
/// `{名前}_{日時}.log` を書く（research.md「ログと設定の永続化」）。書き出しはこの拡張子の
/// 通常ファイルだけを連結する。
pub const LOG_FILE_EXTENSION: &str = "log";

/// 書き出しの見出しの先頭行。
pub const EXPORT_HEADER_PREFIX: &str = "# jxcel 診断情報の書き出し";

/// 見出しに載せる対象ディレクトリの行の接頭辞。
pub const EXPORT_HEADER_DIRECTORY_PREFIX: &str = "# 対象: ";

/// 見出しに載せる連結したファイル数の行の接頭辞。
pub const EXPORT_HEADER_COUNT_PREFIX: &str = "# ファイル数: ";

/// 見出しに載せる、連結した記録が 1 つも無いという行。
pub const EXPORT_EMPTY_NOTICE: &str = "# 記録は見つからなかった";

/// 各記録の内容の前に置く区切り行の書式。`{name}` がファイル名に置き換わる。
pub const EXPORT_FILE_MARKER_FORMAT: &str = "===== {name} =====";

/// 書き出しが一度に読む固定長チャンクの大きさ（バイト）。
///
/// 記録はこの単位で読んで書くため、書き出し中に保持するメモリは連結する記録の合計に依存しない
/// （[`merge_records`]）。保持しうる最大の記録集合（[`MAX_RETAINED_LOG_BYTES`] = 48 MB）より
/// 2 桁以上小さい。
pub const EXPORT_CHUNK_BYTES: usize = 64 * 1024;

/// 書き出しの結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    /// 記録を探したディレクトリ（[`log_dir`] の解決結果）。
    pub source_dir: PathBuf,
    /// 書き出したファイル（呼び出し側が渡した宛先）。
    pub destination: PathBuf,
    /// 連結した記録ファイルの数。0 の場合も 1 つのファイルは作られる。
    pub files_merged: usize,
    /// 宛先へ書いた合計バイト数（見出しと区切り行を含む）。
    pub bytes_written: u64,
}

/// 記録を 1 つのファイルへまとめて書き出す（要件 8.6）。
///
/// 各 OS の規約で解決した記録ディレクトリ（[`log_dir`]）の直下にある `*.log` を、**更新時刻の
/// 昇順**（同時刻はファイル名の昇順）に連結し、`destination` へ書く。宛先は**ローカルのパス
/// だけ**であり、URL や送信先エンドポイントを受け取る入口は無い（要件 8.3）。
///
/// # 形式
///
/// - 先頭に見出しを 3 行置く: [`EXPORT_HEADER_PREFIX`]、[`EXPORT_HEADER_DIRECTORY_PREFIX`] に
///   続けて対象ディレクトリ、[`EXPORT_HEADER_COUNT_PREFIX`] に続けて連結したファイル数。
///   記録が 1 つも無ければ [`EXPORT_EMPTY_NOTICE`] の行を足す
/// - 記録ごとに [`EXPORT_FILE_MARKER_FORMAT`] の区切り行（ファイル名入り）を置き、その後に
///   内容をバイト単位でそのまま写す
/// - 内容が空、または改行で終わらない場合は区切りの改行を 1 つ足す（次の区切り行が必ず行頭に
///   来るようにする）。それ以外は内容を変換しない
///
/// 記録に載りうるのは 4.4 の記録経路が書いたものだけであり、ドキュメントのセル値とスキーマは
/// [`Redacted`] を通してしか経路へ入らない（要件 8.4）。書き出しは記録の内容を変換しないので、
/// セル値が現れることはない。
///
/// # 空・欠落した記録ディレクトリ
///
/// 記録ディレクトリが無い場合と `*.log` が 1 つも無い場合は、どちらも**成功として 1 つの
/// ファイルを作る**（見出しと [`EXPORT_EMPTY_NOTICE`] だけ）。要件 8.6 は「記録をひとつの
/// ファイルにまとめて出力する」ことを求めるので、渡すものが無いことを理由に失敗させない。
/// 呼び出し側は [`ExportReport::files_merged`] が 0 であることで「記録が無かった」と区別できる。
///
/// # 宛先
///
/// 親ディレクトリは存在していなければならない（作らない）。既存のファイルは**置き換える**
/// （利用者が保存先を選ぶ経路では既存ファイルが選ばれうる）。置き換えは
/// [`atomic::replace_with`] の一時ファイル → `rename` で行うため、**失敗した場合に宛先が
/// 切り詰められたり部分的になったりすることはない**（直前の完全な内容のままである）。宛先が
/// 記録ディレクトリの直下にある場合、その既存の宛先自身は入力から除く（自己取り込みの防止）。
///
/// # Errors
///
/// 記録ディレクトリを解決できない（[`DiagnosticsError::LogDirUnavailable`]）、記録を読めない
/// （[`DiagnosticsError::RecordsUnreadable`]）、宛先へ書けない
/// （[`DiagnosticsError::ExportFailed`]）。いずれの場合も宛先は変更されない。
pub fn export(destination: &Path) -> Result<ExportReport, DiagnosticsError> {
    export_with(&|name: &str| std::env::var_os(name), destination)
}

/// [`export`] の環境変数の読み取りを差し替えられる形（4.4 の [`log_dir_with`] と同じ入口）。
///
/// テストが開発者の実の `HOME` や実の記録ディレクトリに依存せずに書き出しを検査するための
/// 入口である。
pub fn export_with(
    lookup: &dyn Fn(&str) -> Option<OsString>,
    destination: &Path,
) -> Result<ExportReport, DiagnosticsError> {
    let directory = log_dir_with(lookup)?;
    export_records(&directory, destination)
}

/// 解決済みの記録ディレクトリから宛先へ書き出す。
fn export_records(directory: &Path, destination: &Path) -> Result<ExportReport, DiagnosticsError> {
    let files = record_files(directory, destination)?;
    let mut bytes_written = 0u64;
    let mut source_failure: Option<DiagnosticsError> = None;

    let outcome = atomic::replace_with(destination, |file| {
        merge_records(directory, &files, file, &mut source_failure, &mut bytes_written)
    });

    match outcome {
        Ok(()) => Ok(ExportReport {
            source_dir: directory.to_path_buf(),
            destination: destination.to_path_buf(),
            files_merged: files.len(),
            bytes_written,
        }),
        // 記録の読み取りで失敗した場合は、その原因（パス付き）を優先して返す。
        Err(source) => match source_failure {
            Some(failure) => Err(failure),
            None => Err(DiagnosticsError::ExportFailed {
                path: destination.to_path_buf(),
                source,
            }),
        },
    }
}

/// 記録ディレクトリ直下の記録ファイルを、更新時刻の昇順（同時刻はファイル名の昇順）で返す。
///
/// 対象は `.log` 拡張子を持つ通常ファイルだけである（サブディレクトリは再帰しない。拡張子違い
/// のファイルは取り込まない）。ディレクトリが無い場合は空を返す（[`export`] の「空・欠落した
/// 記録ディレクトリ」を参照）。宛先自身が記録ディレクトリの直下にある場合は入力から除く。
fn record_files(directory: &Path, destination: &Path) -> Result<Vec<PathBuf>, DiagnosticsError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        // 記録機構がまだ登録されていなければディレクトリは無い。空として扱う。
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(DiagnosticsError::RecordsUnreadable { path: directory.to_path_buf(), source });
        }
    };

    let mut records: Vec<(PathBuf, SystemTime)> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| DiagnosticsError::RecordsUnreadable {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.as_path() == destination {
            continue;
        }
        let file_type = entry.file_type().map_err(|source| DiagnosticsError::RecordsUnreadable {
            path: path.clone(),
            source,
        })?;
        if !file_type.is_file() {
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some(LOG_FILE_EXTENSION) {
            continue;
        }
        // 更新時刻を読めないファイルシステムでは、名前順にだけ意味を持たせる。
        let modified = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        records.push((path, modified));
    }

    records.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| left.0.file_name().cmp(&right.0.file_name()))
    });
    Ok(records.into_iter().map(|(path, _)| path).collect())
}

/// 記録ファイルの内容を `writer` へ連結する。
///
/// 記録は [`EXPORT_CHUNK_BYTES`] 単位で読んで書くため、保持するメモリは**連結する記録の合計に
/// 依存しない**（集合全体を読み込まない）。記録を読めなかった場合は `source_failure` に
/// 原因（パス入り）を残して `io::Error` を返し、[`atomic::replace_with`] に一時ファイルを
/// 破棄させる。
fn merge_records<W: Write>(
    source_dir: &Path,
    files: &[PathBuf],
    writer: &mut W,
    source_failure: &mut Option<DiagnosticsError>,
    bytes_written: &mut u64,
) -> io::Result<()> {
    let mut header = String::new();
    header.push_str(EXPORT_HEADER_PREFIX);
    header.push('\n');
    header.push_str(EXPORT_HEADER_DIRECTORY_PREFIX);
    header.push_str(&source_dir.display().to_string());
    header.push('\n');
    header.push_str(EXPORT_HEADER_COUNT_PREFIX);
    header.push_str(&files.len().to_string());
    header.push('\n');
    if files.is_empty() {
        header.push_str(EXPORT_EMPTY_NOTICE);
        header.push('\n');
    }
    write_counted(writer, header.as_bytes(), bytes_written)?;

    for path in files {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let marker = format!("{}\n", EXPORT_FILE_MARKER_FORMAT.replace("{name}", &name));
        write_counted(writer, marker.as_bytes(), bytes_written)?;

        let mut file = File::open(path).map_err(|source| {
            *source_failure =
                Some(DiagnosticsError::RecordsUnreadable { path: path.clone(), source });
            interrupted()
        })?;

        let mut buffer = [0u8; EXPORT_CHUNK_BYTES];
        let mut last_byte: Option<u8> = None;
        loop {
            let read = file.read(&mut buffer).map_err(|source| {
                *source_failure =
                    Some(DiagnosticsError::RecordsUnreadable { path: path.clone(), source });
                interrupted()
            })?;
            if read == 0 {
                break;
            }
            write_counted(writer, &buffer[..read], bytes_written)?;
            last_byte = Some(buffer[read - 1]);
        }
        // 空、または改行で終わらない記録には区切りの改行を足す（区切り行を行頭に保つ）。
        if last_byte != Some(b'\n') {
            write_counted(writer, b"\n", bytes_written)?;
        }
    }
    Ok(())
}

/// `bytes` を書いて合計へ加える。
fn write_counted<W: Write>(writer: &mut W, bytes: &[u8], total: &mut u64) -> io::Result<()> {
    writer.write_all(bytes)?;
    *total += bytes.len() as u64;
    Ok(())
}

/// 読み取りの失敗で中断することを [`atomic::replace_with`] へ伝えるための `io::Error`。
///
/// 原因の分類は `source_failure` が運ぶので、ここでは一時ファイルを破棄させられればよい。
fn interrupted() -> io::Error {
    io::Error::new(io::ErrorKind::Other, "記録の読み取りを中断した")
}

// ---------------------------------------------------------------------------
// 秘匿（要件 8.4）
// ---------------------------------------------------------------------------

/// 記録に載せてはならない値を示す型（要件 8.4）。
///
/// ドキュメントのセル値とスキーマの内容は、この型を通してしか記録経路へ渡せない。
/// [`recorded_value`] が記録へ値を渡す唯一の入口であり、[`Redacted`] 以外の引数を受け付けない。
///
/// 内側の値は**保持もしない**（[`Redacted::new`] が受け取った時点で破棄する）。記録経路が
/// 参照できるのは「秘匿された値が 1 つあった」という型の事実だけである。加えて `Display` /
/// `Debug` / `Serialize` を実装しないので、次の呼び出しはいずれもコンパイルできない:
///
/// ```compile_fail
/// use app_shell::diagnostics::Redacted;
///
/// let value = Redacted::new("セルの値");
/// let _ = format!("{value}"); // Display が無い
/// ```
///
/// ```compile_fail
/// use app_shell::diagnostics::Redacted;
///
/// let value = Redacted::new("セルの値");
/// let _ = format!("{value:?}"); // Debug が無い
/// ```
///
/// ```compile_fail
/// use app_shell::diagnostics::Redacted;
///
/// let value = Redacted::new("セルの値");
/// let _ = serde_json::to_string(&value); // Serialize が無い
/// ```
pub struct Redacted<T>(PhantomData<T>);

impl<T> Redacted<T> {
    /// 秘匿対象の値を受け取る。**値は保持せずその場で破棄する。**
    ///
    /// 保持しないことが漏洩を構造的に不可能にする — 読む入口を作らないのではなく、読む対象を
    /// 残さない。記録に載る表現は [`recorded_value`] が返す [`REDACTION_PLACEHOLDER`] だけである。
    pub fn new(_value: T) -> Self {
        Self(PhantomData)
    }
}

/// 秘匿された値を記録に載せる唯一の入口（要件 8.4）。
///
/// 生の `T` ではなく [`Redacted`] を要求するため、ドキュメントのセル値やスキーマの内容を
/// そのまま渡す呼び出しは型検査で落ちる（下の `compile_fail` の doc test が機械検査する）。
///
/// ```
/// use app_shell::diagnostics::{recorded_value, Redacted, REDACTION_PLACEHOLDER};
///
/// let rendered = recorded_value(&Redacted::new("セルの値"));
/// assert_eq!(rendered, REDACTION_PLACEHOLDER);
/// ```
///
/// 生の値を渡すことはできない:
///
/// ```compile_fail
/// use app_shell::diagnostics::recorded_value;
///
/// // 誤り: 生のセル値は受け付けられない。
/// let _ = recorded_value(&"セルの値");
/// ```
pub const fn recorded_value<T>(_value: &Redacted<T>) -> &'static str {
    REDACTION_PLACEHOLDER
}

// ---------------------------------------------------------------------------
// エラー
// ---------------------------------------------------------------------------

/// 診断の方針を適用できない失敗。
#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    /// 記録の保存先を決められない（必要な環境変数が無い等）。**パニックしない。**
    #[error("記録の保存先を解決できない: {reason}")]
    LogDirUnavailable { reason: String },

    /// 記録ファイル（または記録ディレクトリの一覧）を読めない。**書き出しは中止され、宛先は
    /// 変更されない。**
    #[error("記録を読めない: {path}: {source}")]
    RecordsUnreadable { path: PathBuf, source: io::Error },

    /// 書き出し先へ書けない。**宛先は変更されない**（一時ファイル → `rename` のため）。
    #[error("診断情報を書き出せない: {path}: {source}")]
    ExportFailed { path: PathBuf, source: io::Error },
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs::{self, File};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc::Receiver;
    use std::time::{Duration, SystemTime};

    use crate::settings::atomic::TEMP_FILE_PREFIX;
    use crate::settings::{open, SettingsChanged, SETTINGS_FILE_NAME};

    use super::*;

    /// 記録に現れてはならない目印。秘匿の検査だけに使う。
    const SENTINEL: &str = "cell-value-4f8a2c1d9b7e";

    /// 環境変数の写像から [`log_dir_with`] へ渡す参照関数を作る。
    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    // -----------------------------------------------------------------------
    // 秘匿（要件 8.4）
    // -----------------------------------------------------------------------

    #[test]
    fn redacted_value_renders_only_the_placeholder() {
        let rendered = recorded_value(&Redacted::new(SENTINEL));
        assert_eq!(rendered, REDACTION_PLACEHOLDER);
        assert!(!rendered.contains(SENTINEL), "秘匿された値が記録の表現に現れた: {rendered}");
    }

    #[test]
    fn redaction_placeholder_does_not_contain_the_sentinel() {
        assert!(!REDACTION_PLACEHOLDER.contains(SENTINEL));
    }

    // -----------------------------------------------------------------------
    // 保存先（要件 8.1）
    // -----------------------------------------------------------------------

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_convention_prefers_xdg_data_home() {
        let dir = log_dir_with(&lookup(&[("XDG_DATA_HOME", "/xdg/data"), ("HOME", "/home/user")]))
            .expect("XDG_DATA_HOME があるので解決できる");
        assert_eq!(dir, Path::new("/xdg/data").join(APP_IDENTIFIER).join(LOG_DIR_NAME));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_convention_falls_back_to_home_local_share() {
        let dir =
            log_dir_with(&lookup(&[("HOME", "/home/user")])).expect("HOME があるので解決できる");
        let expected = Path::new("/home/user")
            .join(".local")
            .join("share")
            .join(APP_IDENTIFIER)
            .join(LOG_DIR_NAME);
        assert_eq!(dir, expected);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_convention_treats_an_empty_variable_as_unset() {
        let dir = log_dir_with(&lookup(&[("XDG_DATA_HOME", ""), ("HOME", "/home/user")]))
            .expect("空の XDG_DATA_HOME は未設定として HOME へ落ちる");
        let expected = Path::new("/home/user")
            .join(".local")
            .join("share")
            .join(APP_IDENTIFIER)
            .join(LOG_DIR_NAME);
        assert_eq!(dir, expected);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_convention_without_environment_is_an_error_not_a_panic() {
        let error = log_dir_with(&lookup(&[])).expect_err("環境変数が無ければ失敗する");
        assert!(matches!(error, DiagnosticsError::LogDirUnavailable { .. }));
    }

    #[test]
    fn macos_convention_uses_home_library_logs() {
        let dir = macos_log_dir_with(&lookup(&[("HOME", "/Users/tester")]))
            .expect("HOME があるので解決できる");
        let expected = Path::new("/Users/tester").join("Library").join("Logs").join(APP_IDENTIFIER);
        assert_eq!(dir, expected);
        // 設定のデータ領域（`Library/Application Support`）とは別の場所である。
        assert!(!dir.to_string_lossy().contains("Application Support"));
    }

    #[test]
    fn macos_convention_without_home_is_an_error() {
        assert!(macos_log_dir_with(&lookup(&[])).is_err());
    }

    #[test]
    fn windows_convention_uses_local_app_data() {
        let local = r"C:\Users\tester\AppData\Local";
        let dir =
            windows_log_dir_with(&lookup(&[("LOCALAPPDATA", local)])).expect("LOCALAPPDATA がある");
        assert_eq!(dir, PathBuf::from(local).join(APP_IDENTIFIER).join(LOG_DIR_NAME));
        // ローミングの `APPDATA`（設定のデータ領域）は読まない。
        let roaming = r"C:\Users\tester\AppData\Roaming";
        assert!(windows_log_dir_with(&lookup(&[("APPDATA", roaming)])).is_err());
    }

    #[test]
    fn current_log_dir_agrees_with_the_injected_resolution_and_never_panics() {
        let from_environment = log_dir_with(&|name: &str| std::env::var_os(name));
        match (log_dir(), from_environment) {
            (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
            // 環境変数を持たない環境でも panic しないこと自体が検査対象である。
            (Err(_), Err(_)) => {}
            (actual, expected) => {
                panic!("log_dir と log_dir_with が食い違う: {actual:?} / {expected:?}")
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_convention_is_the_current_one() {
        let resolved = log_dir_with(&lookup(&[("HOME", "/Users/tester")]))
            .expect("HOME があるので解決できる");
        let expected = Path::new("/Users/tester").join("Library").join("Logs").join(APP_IDENTIFIER);
        assert_eq!(resolved, expected);
    }

    #[cfg(windows)]
    #[test]
    fn windows_convention_is_the_current_one() {
        let local = r"C:\Users\tester\AppData\Local";
        let resolved =
            log_dir_with(&lookup(&[("LOCALAPPDATA", local)])).expect("LOCALAPPDATA がある");
        assert_eq!(resolved, PathBuf::from(local).join(APP_IDENTIFIER).join(LOG_DIR_NAME));
    }

    // -----------------------------------------------------------------------
    // 保持方針（要件 8.5）
    // -----------------------------------------------------------------------

    #[test]
    fn retention_cap_is_50_megabytes() {
        assert_eq!(MAX_TOTAL_LOG_BYTES, 50_000_000);
        assert!(MAX_TOTAL_LOG_BYTES <= 50_000_000, "十進の 50 MB を超えてはならない");
    }

    #[test]
    fn retention_values_are_the_documented_ones() {
        assert_eq!(MAX_LOG_FILE_BYTES, 8_000_000);
        assert_eq!(KEEP_SOME_ARCHIVED_FILES, 5);
        assert_eq!(RETAINED_LOG_FILES, u64::from(KEEP_SOME_ARCHIVED_FILES) + 1);
        assert_eq!(MAX_RETAINED_LOG_BYTES, 48_000_000);
        assert!(MAX_RETAINED_LOG_BYTES <= MAX_TOTAL_LOG_BYTES);
    }

    #[test]
    fn retention_model_counts_the_active_file_plus_the_archives() {
        // `KeepSome(n)` の n はアーカイブ数であり、記録中の現行ファイルは含まれない。
        assert_eq!(RETAINED_LOG_FILES, 6);
        assert_eq!(MAX_RETAINED_LOG_BYTES, MAX_LOG_FILE_BYTES * 6);
        assert_eq!(MAX_TOTAL_LOG_BYTES - MAX_RETAINED_LOG_BYTES, 2_000_000);
    }

    #[test]
    fn retention_values_dominate_the_plugin_defaults_by_two_orders_of_magnitude() {
        // research.md「ログと設定の永続化」: `tauri-plugin-log` の既定は 40 KB / `KeepOne`。
        const PLUGIN_DEFAULT_MAX_FILE_BYTES: u64 = 40_000;
        // `KeepOne` はアーカイブ 1 + 現行 1 の 2 ファイルを保持する。
        const PLUGIN_DEFAULT_RETAINED_FILES: u64 = 2;
        assert!(MAX_LOG_FILE_BYTES >= PLUGIN_DEFAULT_MAX_FILE_BYTES * 100);
        assert!(u64::from(KEEP_SOME_ARCHIVED_FILES) >= PLUGIN_DEFAULT_RETAINED_FILES);
        assert!(
            MAX_RETAINED_LOG_BYTES
                >= PLUGIN_DEFAULT_MAX_FILE_BYTES * PLUGIN_DEFAULT_RETAINED_FILES * 100
        );
    }

    // -----------------------------------------------------------------------
    // テスト用の一時ディレクトリと道具
    // -----------------------------------------------------------------------

    /// テスト中だけ使うディレクトリ。`Drop` で削除する（失敗経路でも残さない）。
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            static SEQUENCE: AtomicU32 = AtomicU32::new(0);
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "jxcel-diagnostics-{tag}-{}-{sequence}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// 記録ディレクトリの解決が読む環境変数だけを `base` に向ける（読む名前は OS で違う）。
    ///
    /// `export_with` / `log_dir_with` はどちらも環境変数の読み取りを差し替えられるので、
    /// テストは開発者の実の `HOME` や実の記録ディレクトリに依存しない。
    fn env_from_base(base: &Path) -> impl Fn(&str) -> Option<OsString> + '_ {
        move |name: &str| match name {
            "XDG_DATA_HOME" | "HOME" | "LOCALAPPDATA" => Some(base.as_os_str().to_os_string()),
            _ => None,
        }
    }

    /// 記録ファイルを 1 つ作る。更新時刻を固定するので、並び順を名前ではなく時刻で検査できる。
    fn write_record(directory: &Path, name: &str, content: &str, modified_secs: u64) -> PathBuf {
        fs::create_dir_all(directory).expect("記録ディレクトリを作れる");
        let path = directory.join(name);
        let mut file = File::create(&path).expect("記録を作れる");
        file.write_all(content.as_bytes()).expect("記録を書ける");
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(modified_secs))
            .expect("更新時刻を設定できる");
        path
    }

    /// ディレクトリ直下のエントリ名を昇順で返す。
    fn entry_names(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .expect("ディレクトリを読める")
            .map(|entry| entry.expect("エントリを読める").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// 一時ファイル（[`TEMP_FILE_PREFIX`] で始まる名前）がディレクトリに残っていない。
    fn assert_no_temporary_files(directory: &Path) {
        let leftovers: Vec<String> =
            entry_names(directory).into_iter().filter(|name| name.starts_with(TEMP_FILE_PREFIX)).collect();
        assert!(leftovers.is_empty(), "一時ファイルが残っている: {leftovers:?}");
    }

    /// 記録の区切り行を、実装と同じ書式で組み立てる。
    fn marker(name: &str) -> String {
        EXPORT_FILE_MARKER_FORMAT.replace("{name}", name)
    }

    // -----------------------------------------------------------------------
    // 書き出し（要件 8.6）— 完了状態
    // -----------------------------------------------------------------------

    #[test]
    fn export_merges_every_record_into_one_file_in_updated_order() {
        let scratch = Scratch::new("export-order");
        let lookup = env_from_base(scratch.path());
        let log_dir = log_dir_with(&lookup).expect("記録ディレクトリを解決できる");
        // 名前順は a, b, c だが更新時刻の順は c, a, b である。名前順に並べる実装はここで落ちる。
        write_record(&log_dir, "c.log", "記録C: 最初\n", 1_000);
        write_record(&log_dir, "a.log", "記録A: 二番目\n", 2_000);
        write_record(&log_dir, "b.log", "記録B: 三番目\n", 3_000);
        // 記録でないもの（拡張子違い・ディレクトリ）は取り込まない。
        fs::write(log_dir.join("memo.txt"), "記録ではない").expect("書ける");
        fs::create_dir(log_dir.join("nested.log")).expect("ディレクトリを作れる");
        fs::write(log_dir.join("nested.log").join("inner.log"), "入れ子").expect("書ける");

        let output_dir = scratch.path().join("out");
        fs::create_dir(&output_dir).expect("出力先を作れる");
        let destination = output_dir.join("diagnostics.txt");

        let report = export_with(&lookup, &destination).expect("書き出せる");

        assert_eq!(report.source_dir, log_dir);
        assert_eq!(report.destination, destination);
        assert_eq!(report.files_merged, 3, "記録 3 件だけを取り込む");
        assert_eq!(
            entry_names(&output_dir),
            vec!["diagnostics.txt".to_owned()],
            "出力は 1 つだけであり、一時ファイルも残らない"
        );
        assert_eq!(
            report.bytes_written,
            fs::metadata(&destination).expect("読める").len(),
            "報告したバイト数が実ファイルと一致する"
        );

        let merged = fs::read_to_string(&destination).expect("読める");
        assert!(merged.starts_with(EXPORT_HEADER_PREFIX), "見出しが無い: {merged}");
        assert!(merged.contains("# ファイル数: 3"), "件数が無い: {merged}");
        assert!(merged.contains(&marker("c.log")), "c.log の区切りが無い: {merged}");
        assert!(merged.contains(&marker("a.log")), "a.log の区切りが無い: {merged}");
        assert!(merged.contains(&marker("b.log")), "b.log の区切りが無い: {merged}");
        let first = merged.find("記録C: 最初").expect("C が無い");
        let second = merged.find("記録A: 二番目").expect("A が無い");
        let third = merged.find("記録B: 三番目").expect("B が無い");
        assert!(first < second && second < third, "更新時刻の昇順になっていない: {merged}");
        assert!(!merged.contains("記録ではない"), "記録でないファイルを取り込んだ: {merged}");
        assert!(!merged.contains("入れ子"), "入れ子のファイルを取り込んだ: {merged}");
        assert_no_temporary_files(&output_dir);
    }

    #[test]
    fn export_from_an_empty_directory_still_writes_one_file() {
        let scratch = Scratch::new("export-empty");
        let lookup = env_from_base(scratch.path());
        let log_dir = log_dir_with(&lookup).expect("解決できる");
        fs::create_dir_all(&log_dir).expect("空の記録ディレクトリを作れる");
        let output_dir = scratch.path().join("out");
        fs::create_dir(&output_dir).expect("出力先を作れる");
        let destination = output_dir.join("empty.txt");

        let report = export_with(&lookup, &destination).expect("記録が無くても成功する");

        assert_eq!(report.files_merged, 0);
        assert_eq!(entry_names(&output_dir), vec!["empty.txt".to_owned()]);
        let merged = fs::read_to_string(&destination).expect("1 つのファイルは作られる");
        assert!(merged.starts_with(EXPORT_HEADER_PREFIX));
        assert!(merged.contains(EXPORT_EMPTY_NOTICE), "記録が無いことが分からない: {merged}");
    }

    #[test]
    fn export_from_a_missing_directory_still_writes_one_file() {
        let scratch = Scratch::new("export-missing");
        let lookup = env_from_base(scratch.path());
        // 記録ディレクトリは作らない（記録機構がまだ登録されていない状態に相当する）。
        let destination = scratch.path().join("missing.txt");

        let report = export_with(&lookup, &destination).expect("記録が無くても成功する");

        assert_eq!(report.files_merged, 0);
        assert!(destination.is_file(), "1 つのファイルは作られる");
        let merged = fs::read_to_string(&destination).expect("読める");
        assert!(merged.contains(EXPORT_EMPTY_NOTICE));
        assert_no_temporary_files(scratch.path());
    }

    #[test]
    fn export_replaces_an_existing_destination() {
        let scratch = Scratch::new("export-existing");
        let lookup = env_from_base(scratch.path());
        let log_dir = log_dir_with(&lookup).expect("解決できる");
        write_record(&log_dir, "a.log", "新しい記録\n", 1_000);
        let destination = scratch.path().join("existing.txt");
        fs::write(&destination, "古い内容").expect("古い宛先を用意できる");

        let report = export_with(&lookup, &destination).expect("上書きできる");

        assert_eq!(report.files_merged, 1);
        let merged = fs::read_to_string(&destination).expect("読める");
        assert!(merged.contains("新しい記録"));
        assert!(!merged.contains("古い内容"), "古い内容が残っている: {merged}");
        assert_no_temporary_files(scratch.path());
    }

    // -----------------------------------------------------------------------
    // 書き出し（要件 8.6）— 失敗と頑健性
    // -----------------------------------------------------------------------

    #[test]
    fn export_to_an_unusable_destination_fails_without_creating_anything() {
        let scratch = Scratch::new("export-unusable");
        let lookup = env_from_base(scratch.path());
        let log_dir = log_dir_with(&lookup).expect("解決できる");
        write_record(&log_dir, "a.log", "記録\n", 1_000);
        // 宛先の親を通常ファイルにする（ディレクトリではない）。
        let blocker = scratch.path().join("blocker");
        fs::write(&blocker, "ファイル").expect("塞げる");
        let destination = blocker.join("diagnostics.txt");

        let error = export_with(&lookup, &destination).expect_err("書けないはず");

        assert!(matches!(error, DiagnosticsError::ExportFailed { .. }), "原因: {error:?}");
        assert!(blocker.is_file(), "宛先の親が壊された");
        assert_eq!(fs::read_to_string(&blocker).expect("読める"), "ファイル");
    }

    #[test]
    fn export_failure_leaves_the_destination_untouched() {
        let scratch = Scratch::new("export-untouched");
        let lookup = env_from_base(scratch.path());
        let log_dir = log_dir_with(&lookup).expect("解決できる");
        write_record(&log_dir, "a.log", "記録\n", 1_000);
        // 宛先を非空のディレクトリにする。`rename` は失敗し、既存の中身は変わらない。
        let destination = scratch.path().join("report");
        fs::create_dir(&destination).expect("作れる");
        fs::write(destination.join("kept.txt"), "残す").expect("書ける");

        let error = export_with(&lookup, &destination).expect_err("置き換えられない");

        assert!(matches!(error, DiagnosticsError::ExportFailed { .. }), "原因: {error:?}");
        assert_eq!(entry_names(&destination), vec!["kept.txt".to_owned()], "宛先が変わった");
        assert_eq!(fs::read_to_string(destination.join("kept.txt")).expect("読める"), "残す");
        assert_no_temporary_files(scratch.path());
    }

    #[cfg(unix)]
    #[test]
    fn export_into_an_unwritable_directory_fails_and_leaves_the_destination() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = Scratch::new("export-readonly");
        let lookup = env_from_base(scratch.path());
        let log_dir = log_dir_with(&lookup).expect("解決できる");
        write_record(&log_dir, "a.log", "記録\n", 1_000);
        let output_dir = scratch.path().join("out");
        fs::create_dir(&output_dir).expect("作れる");
        let destination = output_dir.join("diagnostics.txt");
        fs::write(&destination, "以前の内容").expect("書ける");
        fs::set_permissions(&output_dir, fs::Permissions::from_mode(0o555))
            .expect("読み取り専用にできる");

        // 権限を無視する環境（root 等）では、この検査は成立しない。書けるなら検証せず戻る。
        if fs::write(output_dir.join("probe"), b"x").is_ok() {
            let _ = fs::remove_file(output_dir.join("probe"));
            fs::set_permissions(&output_dir, fs::Permissions::from_mode(0o755))
                .expect("権限を戻せる");
            return;
        }

        let error = export_with(&lookup, &destination).expect_err("書けないはず");

        fs::set_permissions(&output_dir, fs::Permissions::from_mode(0o755))
            .expect("権限を戻せる");
        assert!(matches!(error, DiagnosticsError::ExportFailed { .. }), "原因: {error:?}");
        assert_eq!(
            fs::read_to_string(&destination).expect("読める"),
            "以前の内容",
            "失敗した書き出しが宛先を切り詰めた"
        );
        assert_eq!(entry_names(&output_dir), vec!["diagnostics.txt".to_owned()]);
    }

    // -----------------------------------------------------------------------
    // 書き出し（要件 8.6）— 作業領域の上限
    // -----------------------------------------------------------------------

    /// 書き出しの作業領域が連結する記録の合計に依存しないことを、書き込みの粒度で観測する。
    ///
    /// 実装が集合全体（数十 MB になりうる）を `Vec` に読んでから書くなら、`merge_records` は
    /// チャンク上限を超える 1 回の `write` を行う。上限を超える書き込みを拒否する書き手に
    /// 通すことで、その実装はここで失敗する — したがってこの検査は「集合全体をメモリに
    /// 載せない」ことの load-bearing な証拠である。
    #[test]
    fn export_copies_records_in_bounded_chunks() {
        /// チャンク上限を超える 1 回の書き込みを拒否する書き手。
        struct ChunkCapWriter {
            cap: usize,
            written: Vec<u8>,
            largest_write: usize,
        }

        impl Write for ChunkCapWriter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if bytes.len() > self.cap {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("1 回の書き込みが上限を超えた: {} > {}", bytes.len(), self.cap),
                    ));
                }
                self.largest_write = self.largest_write.max(bytes.len());
                self.written.extend_from_slice(bytes);
                Ok(bytes.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let scratch = Scratch::new("export-stream");
        let log_dir = scratch.path().join("logs");
        let big = "x".repeat(EXPORT_CHUNK_BYTES * 2 + 11);
        let medium = "y".repeat(EXPORT_CHUNK_BYTES + 3);
        write_record(&log_dir, "a.log", &big, 1_000);
        write_record(&log_dir, "b.log", &medium, 2_000);
        let destination = scratch.path().join("dest.txt");
        let files = record_files(&log_dir, &destination).expect("列挙できる");
        assert_eq!(files.len(), 2);

        let mut writer =
            ChunkCapWriter { cap: EXPORT_CHUNK_BYTES, written: Vec::new(), largest_write: 0 };
        let mut failure = None;
        let mut bytes_written = 0u64;
        merge_records(&log_dir, &files, &mut writer, &mut failure, &mut bytes_written)
            .expect("上限内の書き込みだけで連結できる");

        assert!(failure.is_none());
        assert!(writer.largest_write <= EXPORT_CHUNK_BYTES, "チャンク上限を超えた");
        assert!(writer.largest_write > 0);
        assert_eq!(writer.written.len() as u64, bytes_written, "報告したバイト数が違う");
        let rendered = String::from_utf8(writer.written).expect("UTF-8");
        assert!(rendered.contains(&big), "大きい記録が欠けた");
        assert!(rendered.contains(&medium), "中くらいの記録が欠けた");
        // 作業領域は保持しうる最大の集合（48 MB）より 2 桁以上小さい。
        assert!((EXPORT_CHUNK_BYTES as u64) * 100 < MAX_RETAINED_LOG_BYTES);
    }

    // -----------------------------------------------------------------------
    // 詳細度（要件 8.7）
    // -----------------------------------------------------------------------

    fn recv_event(receiver: &Receiver<SettingsChanged>) -> SettingsChanged {
        receiver.recv_timeout(Duration::from_secs(5)).expect("通知が届く")
    }

    #[test]
    fn level_set_uses_the_documented_names_and_order() {
        assert_eq!(DiagnosticsLevel::default(), DiagnosticsLevel::Info);
        let levels = [
            DiagnosticsLevel::Off,
            DiagnosticsLevel::Error,
            DiagnosticsLevel::Warn,
            DiagnosticsLevel::Info,
            DiagnosticsLevel::Debug,
            DiagnosticsLevel::Trace,
        ];
        for (level, name) in levels
            .iter()
            .zip(["off", "error", "warn", "info", "debug", "trace"])
        {
            assert_eq!(serde_json::to_value(level).expect("直列化できる"), serde_json::json!(name));
        }
        for pair in levels.windows(2) {
            assert!(
                pair[0] < pair[1],
                "詳細度の昇順が崩れている: {:?} / {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn an_unset_level_reads_the_default() {
        let scratch = Scratch::new("level-default");
        let (store, _) = open(scratch.path()).expect("設定ストアを開ける");
        assert_eq!(DiagnosticsLevel::from_store(&*store), DiagnosticsLevel::Info);
    }

    #[test]
    fn a_stored_level_survives_a_fresh_open() {
        let scratch = Scratch::new("level-persist");
        let (store, _) = open(scratch.path()).expect("設定ストアを開ける");
        DiagnosticsLevel::Debug.write_to(&*store).expect("書ける");
        assert_eq!(DiagnosticsLevel::from_store(&*store), DiagnosticsLevel::Debug);

        // 実体を手放してから開き直す（4.1 の往復テストと同じ方法。必ずディスクから読む）。
        drop(store);
        let (fresh, _) = open(scratch.path()).expect("開き直せる");
        assert_eq!(DiagnosticsLevel::from_store(&*fresh), DiagnosticsLevel::Debug);

        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(scratch.path().join(SETTINGS_FILE_NAME)).expect("読める"))
                .expect("JSON として読める");
        assert_eq!(raw[SettingsKey::DiagnosticsLevel.as_str()], serde_json::json!("debug"));
    }

    #[test]
    fn a_garbage_stored_level_reads_the_default_without_being_erased() {
        let scratch = Scratch::new("level-garbage");
        let (store, _) = open(scratch.path()).expect("設定ストアを開ける");

        store.set(&SettingsKey::DiagnosticsLevel, &"とても詳しく").expect("書ける");
        assert_eq!(DiagnosticsLevel::from_store(&*store), DiagnosticsLevel::Info, "未知の名前");

        store.set(&SettingsKey::DiagnosticsLevel, &7).expect("書ける");
        assert_eq!(DiagnosticsLevel::from_store(&*store), DiagnosticsLevel::Info, "別の型");

        // 解釈できない値でも保存されたものを消さない（4.2 の復旧と同じ精神）。
        assert_eq!(
            store.get::<serde_json::Value>(&SettingsKey::DiagnosticsLevel),
            Some(serde_json::json!(7))
        );
    }

    #[test]
    fn a_level_change_is_observable_through_subscribe() {
        let scratch = Scratch::new("level-subscribe");
        let (store, _) = open(scratch.path()).expect("設定ストアを開ける");
        let receiver = store.subscribe();

        DiagnosticsLevel::Trace.write_to(&*store).expect("書ける");

        let event = recv_event(&receiver);
        assert_eq!(event.key, SettingsKey::DiagnosticsLevel);
        assert_eq!(event.value, serde_json::json!("trace"));
        assert_eq!(DiagnosticsLevel::from_store(&*store), DiagnosticsLevel::Trace);
    }
}

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

use std::ffi::OsString;
use std::marker::PhantomData;
use std::path::PathBuf;

use crate::settings::{self, APP_IDENTIFIER};

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
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

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
}

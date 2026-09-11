//! 設定の永続化（要件 7.1〜7.3。design.md「Core Layer / SettingsStore」）。
//!
//! 設定ストアは自前で持つ（`tauri-plugin-store` は採らない。research.md 決定 6）。理由は、
//! プラグインの保存が truncate-then-write で原子的でないこと、未知キーの保持（要件 7.6）と
//! 破損時の既定値起動（要件 7.5）の意味論を持たないことである。
//!
//! 本モジュール（tasks.md 4.1）が持つのは次の 5 つである:
//!
//! - 原子的な置き換え。[`atomic`] が同一ディレクトリの一時ファイル → `sync_all` → `rename`
//!   の順で行う（truncate-then-write はしない）
//! - 保存先の解決。各 OS が定める標準のアプリケーションデータ領域の下に識別子を足す
//!   （要件 7.2。[`app_data_base_dir`] / [`app_data_dir`]）
//! - キー空間の型 [`SettingsKey`]。安定した文字列へ落ちる
//! - 同一ディレクトリにつき 1 つの共有実体を返す [`open`]（要件 7.3）
//! - [`SettingsStore::get`] / [`SettingsStore::set`] と、書き込みの直列化
//!
//! ファイルは `SettingsKey` をキーとする 1 つの JSON オブジェクトである（design.md
//! 「Logical Data Model」の Settings 構造）。値は [`serde_json::Value`] として読み書きするため、
//! このモジュールが解釈しないキーもそのまま往復する。
//!
//! # 後続タスクが拡張するもの（このモジュールでは宣言しない）
//!
//! - **4.2**: 未知キーの保持の保証（要件 7.6）と、読み取りに失敗したときの既定値起動・復旧の
//!   報告（要件 7.5）。[`open`] の第 2 要素 [`OpenReport`] がその受け口であり、4.1 は常に空の
//!   値を返す。読み取りに失敗した既存ファイルは、4.1 では [`SettingsError::ParseFailed`] として
//!   返す（既定値で起動するかどうかは 4.2 が決める）
//! - **4.3**: 変更通知の購読（要件 7.4）。`subscribe` はここに置かない
//! - **`schema_version`**: design.md「Logical Data Model」は設定ファイルの版を持つ。版の解釈は
//!   4.2（未知の版は既定値で起動する）が担う。4.1 は版を特別扱いしない — ファイルがキーから
//!   値への JSON オブジェクトである限り、4.2 が版のキーを足しても形式は変わらない
//!
//! # design.md からの意図的な差異
//!
//! design.md の `open` は戻り値を `Arc<dyn SettingsStore>` と書くが、[`SettingsStore`] の
//! [`get`](SettingsStore::get) / [`set`](SettingsStore::set) は型引数を持つ汎用メソッドであり、
//! そのような trait は object-safe ではない（`dyn` にできない。E0038）。境界で型付きの値を
//! 得ることを優先し、[`open`] は実体の `Arc` を返す。object-safe な層が必要になった時点で、
//! そのとき raw な非汎用メソッドを足せばよい。

pub mod atomic;

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, RwLock, Weak};

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};

/// アプリケーションの識別子。**`src-tauri/tauri.conf.json` の `identifier` と一致しなければ
/// ならない**（`crates/app-shell/tests/settings_store.rs` が照合する）。
///
/// これが食い違うのはバグである: アダプタがログや診断の保存先をこの定数で解決し、Tauri が
/// バンドル識別子として別の値を使えば、同じアプリケーションが 2 つのアプリケーションデータ
/// 領域を読み書きすることになる。
pub const APP_IDENTIFIER: &str = "com.jxcel.app";

/// 設定ファイルの名前。保存先ディレクトリ（[`app_data_dir`]）の直下に置く。
pub const SETTINGS_FILE_NAME: &str = "settings.json";

// ---------------------------------------------------------------------------
// キー空間
// ---------------------------------------------------------------------------

/// 設定のキー。
///
/// **安定した文字列**（`window.geometry` のようなドット区切りの名前）でファイルに載る。
/// 境界では裸の文字列ではなくこの型を通す（design.md「SettingsStore / Service Interface」）。
/// 名前を変えると保存済みの値が読めなくなるため、公開後は変更しない。
///
/// 具体のキーの一覧（design.md「Logical Data Model」の表）は、それを必要とする後続タスクが
/// 定数として足す。4.1 はキーの綴りを決める型と、その文字列化・順序だけを持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SettingsKey(&'static str);

impl SettingsKey {
    /// 安定した名前からキーを作る。
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// ファイルとエラー表示に載る安定した文字列。
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for SettingsKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

// ---------------------------------------------------------------------------
// 保存先の解決（要件 7.2）
// ---------------------------------------------------------------------------

/// OS が定める標準のアプリケーションデータ領域を返す（識別子を付けない基底）。
///
/// これが 4.1 の保存先だけでなく、4.4 の診断情報の保存先にも使う規約の単一の実装である
/// （`tauri-plugin-log` の既定値も同じ規約で、Linux は `$XDG_DATA_HOME/{id}`、macOS は
/// `~/Library/Application Support/{id}`、Windows はローミングの `%APPDATA%/{id}`）。
///
/// - Linux: `$XDG_DATA_HOME`、無ければ `$HOME/.local/share`
/// - macOS: `$HOME/Library/Application Support`
/// - Windows: `%APPDATA%`（ローミング）
///
/// 環境変数が無い（または空の）ときはパニックせず [`SettingsError::AppDataDirUnavailable`] を
/// 返す。
pub fn app_data_base_dir() -> Result<PathBuf, SettingsError> {
    app_data_base_dir_with(&|name: &str| std::env::var_os(name))
}

/// [`app_data_base_dir`] の環境変数の読み取りを差し替えられる形。
///
/// テストが開発者の実の環境に依存せずに各 OS の規約を確かめるための入口である。値が与えられて
/// いない場合と空の場合をどちらも「未設定」として扱う。
pub fn app_data_base_dir_with(lookup: &dyn Fn(&str) -> Option<OsString>) -> Result<PathBuf, SettingsError> {
    #[cfg(target_os = "linux")]
    {
        if let Some(xdg_data_home) = non_empty_env(lookup, "XDG_DATA_HOME") {
            return Ok(xdg_data_home);
        }
        return match non_empty_env(lookup, "HOME") {
            Some(home) => Ok(home.join(".local").join("share")),
            None => Err(SettingsError::AppDataDirUnavailable {
                reason: "XDG_DATA_HOME も HOME も設定されていない".to_owned(),
            }),
        };
    }

    #[cfg(target_os = "macos")]
    {
        return match non_empty_env(lookup, "HOME") {
            Some(home) => Ok(home.join("Library").join("Application Support")),
            None => Err(SettingsError::AppDataDirUnavailable {
                reason: "HOME が設定されていない".to_owned(),
            }),
        };
    }

    #[cfg(windows)]
    {
        return match non_empty_env(lookup, "APPDATA") {
            Some(app_data) => Ok(app_data),
            None => Err(SettingsError::AppDataDirUnavailable {
                reason: "APPDATA が設定されていない".to_owned(),
            }),
        };
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = lookup;
        Err(SettingsError::AppDataDirUnavailable {
            reason: "この OS のアプリケーションデータ領域の規約を持たない".to_owned(),
        })
    }
}

/// 設定の保存先。OS 標準の領域の下に [`APP_IDENTIFIER`] を足したもの（要件 7.2）。
pub fn app_data_dir() -> Result<PathBuf, SettingsError> {
    Ok(app_data_base_dir()?.join(APP_IDENTIFIER))
}

/// 環境変数を非空のパスとして読む（空の値は未設定として扱う）。
fn non_empty_env(lookup: &dyn Fn(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    let value = lookup(name)?;
    if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value))
    }
}

// ---------------------------------------------------------------------------
// エラー
// ---------------------------------------------------------------------------

/// 設定ストアの失敗。原因を区別できる。
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// OS 標準のアプリケーションデータ領域を決められない（環境変数が無い等）。
    #[error("アプリケーションデータ領域を解決できない: {reason}")]
    AppDataDirUnavailable { reason: String },

    /// 保存先ディレクトリを用意できない。
    #[error("設定ディレクトリを用意できない: {path}: {source}")]
    DirectoryUnavailable { path: PathBuf, source: io::Error },

    /// 既存の設定ファイルを読み取れない。
    #[error("設定ファイルを読み取れない: {path}: {source}")]
    ReadFailed { path: PathBuf, source: io::Error },

    /// 既存の設定ファイルを解釈できない（破損、またはオブジェクトでない）。
    ///
    /// 破損したファイルから既定値で起動するかどうか（要件 7.5）は 4.2 が決める。
    #[error("設定ファイルを解釈できない: {path}: {source}")]
    ParseFailed { path: PathBuf, source: serde_json::Error },

    /// 設定値を JSON へ変換できない。
    #[error("設定値を JSON に変換できない: {key}: {source}")]
    EncodeFailed { key: SettingsKey, source: serde_json::Error },

    /// 書き込みに失敗した。**対象ファイルは直前の完全な内容のままである**（[`atomic`] の保証）。
    #[error("設定ファイルを書き込めない: {path}: {source}")]
    WriteFailed { path: PathBuf, source: io::Error },
}

// ---------------------------------------------------------------------------
// 境界
// ---------------------------------------------------------------------------

/// 設定ストアのサービスインタフェース（design.md「SettingsStore / Service Interface」）。
///
/// `Send + Sync` であり、1 つの実体を全ウィンドウで共有する（要件 7.3）。変更通知の購読
/// （要件 7.4）は 4.3 が足すまでここに置かない。
pub trait SettingsStore: Send + Sync {
    /// 保存されている値を型付きで返す。存在しないキーと、保存されている値が `T` として
    /// 解釈できない場合はどちらも `None`（design の `get` は失敗を返す経路を持たない）。
    fn get<T: DeserializeOwned>(&self, key: &SettingsKey) -> Option<T>;

    /// 値を保存してから返る。戻った時点で値はメモリとディスクの両方にある。
    ///
    /// 失敗した場合、メモリとディスクのどちらも変更前のままである。
    fn set<T: Serialize>(&self, key: &SettingsKey, value: &T) -> Result<(), SettingsError>;
}

/// `open` が返す第 2 要素。design.md の `Option<RecoveredFrom>` に対応する受け口である。
///
/// **4.1 の時点で復旧経路は存在しない**ため、この型は空であり、[`open`] は常に空の値を返す。
/// 破損したファイルからの復旧（要件 7.5）を実装する 4.2 がここへ事実を載せる。呼び出し側は
/// 今のうちから受け取っておけるので、4.2 は [`open`] の形を変えずに済む。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenReport {}

// ---------------------------------------------------------------------------
// 実体
// ---------------------------------------------------------------------------

/// メモリ上の状態。書き込みロックがこの全体を覆う。
struct State {
    /// 設定ファイルのパス（正規化したディレクトリ + [`SETTINGS_FILE_NAME`]）。
    file: PathBuf,
    /// ファイルの中身。キーは [`SettingsKey::as_str`]、値は生の JSON。
    ///
    /// 生の [`Value`] を保つため、このモジュールが解釈しないキーも失われない（保証そのものは
    /// 4.2 が定める）。
    values: Map<String, Value>,
}

/// ファイルに永続化する設定ストア。1 ディレクトリにつき 1 つの実体を [`open`] が共有する。
pub struct FileSettingsStore {
    /// 書き込みロックは「値を差し替えてからファイルへ書き終えるまで」の全体を覆う。
    ///
    /// これが並行する `set` の直列化そのものである（失われた更新を防ぐ）。読み取りも同じ
    /// ロックを共有するため、書き込みの間は待つ — 設定の書き込みは小さく頻度も低いので、
    /// 読み取りを待たせないための追加の機構は持たない。その代わり、`set` が戻った時点で
    /// メモリとディスクが必ず一致する。
    state: RwLock<State>,
}

impl FileSettingsStore {
    /// ディレクトリから読み込む。ファイルが無ければ空から始める。
    fn load(directory: &Path) -> Result<Self, SettingsError> {
        let file = directory.join(SETTINGS_FILE_NAME);
        let values = match fs::read(&file) {
            Ok(bytes) => serde_json::from_slice::<Map<String, Value>>(&bytes)
                .map_err(|source| SettingsError::ParseFailed { path: file.clone(), source })?,
            Err(source) if source.kind() == io::ErrorKind::NotFound => Map::new(),
            Err(source) => return Err(SettingsError::ReadFailed { path: file.clone(), source }),
        };
        Ok(Self { state: RwLock::new(State { file, values }) })
    }

    /// 読み取りロックを取る。毒されていても中の状態は壊れていない（書き込みロックが守る不変条件は
    /// 「メモリとディスクが一致する」であり、panic の途中で崩れない）ので、回復して先へ進む。
    fn read(&self) -> std::sync::RwLockReadGuard<'_, State> {
        self.state.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// 書き込みロックを取る（[`Self::read`] と同じ理由で毒を回復する）。
    fn write(&self) -> std::sync::RwLockWriteGuard<'_, State> {
        self.state.write().unwrap_or_else(PoisonError::into_inner)
    }
}

impl SettingsStore for FileSettingsStore {
    fn get<T: DeserializeOwned>(&self, key: &SettingsKey) -> Option<T> {
        let state = self.read();
        let value = state.values.get(key.as_str())?;
        // 借用したまま解釈する（値の複製を作らない）。`T` は `DeserializeOwned` なので、
        // 借用したデシリアライザから所有の値を作れる。
        T::deserialize(value).ok()
    }

    fn set<T: Serialize>(&self, key: &SettingsKey, value: &T) -> Result<(), SettingsError> {
        let encoded = serde_json::to_value(value)
            .map_err(|source| SettingsError::EncodeFailed { key: *key, source })?;

        // ここから先、書き込みロックをファイルへの書き込みの完了まで保持する。これが並行する
        // `set` を直列化し、あるスレッドの写しが別のスレッドの書き込みを上書きすることを防ぐ。
        let mut state = self.write();
        let previous = state.values.insert(key.as_str().to_owned(), encoded);
        let bytes = match serde_json::to_vec(&state.values) {
            Ok(bytes) => bytes,
            Err(source) => {
                restore(&mut state.values, key, previous);
                return Err(SettingsError::EncodeFailed { key: *key, source });
            }
        };
        match atomic::replace(&state.file, &bytes) {
            Ok(()) => Ok(()),
            Err(source) => {
                // 失敗した書き込みをメモリに残さない（ファイルとメモリを食い違わせない）。
                restore(&mut state.values, key, previous);
                Err(SettingsError::WriteFailed { path: state.file.clone(), source })
            }
        }
    }
}

/// 失敗した書き込みを取り消して、キーを書き込み前の値へ戻す。
fn restore(values: &mut Map<String, Value>, key: &SettingsKey, previous: Option<Value>) {
    match previous {
        Some(previous) => {
            values.insert(key.as_str().to_owned(), previous);
        }
        None => {
            values.remove(key.as_str());
        }
    }
}

// ---------------------------------------------------------------------------
// 共有実体の登録簿
// ---------------------------------------------------------------------------

/// 正規化したディレクトリから実体への写像。
///
/// [`Weak`] を保つので、誰も使っていない実体は登録簿が解放を妨げない（次の `open` が
/// ディスクから読み直す）。`Arc` を保持すると、テストが実体を手放しても同じものが返り、
/// 「開き直す」検証が成立しない。
static REGISTRY: LazyLock<Mutex<HashMap<PathBuf, Weak<FileSettingsStore>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 設定ストアを開く（design.md「SettingsStore / Service Interface」の `open`）。
///
/// 同じディレクトリに対する `open` は**同じ実体**を返す（要件 7.3）。ディレクトリが無ければ
/// 作り、正規化したパスで登録簿を引く。既存の設定ファイルはここで読み込む。
///
/// # Errors
///
/// ディレクトリを用意できない場合（[`SettingsError::DirectoryUnavailable`]）、既存のファイルを
/// 読み取れない場合（[`SettingsError::ReadFailed`]）、JSON オブジェクトとして解釈できない場合
/// （[`SettingsError::ParseFailed`]）に返す。破損したファイルから既定値で起動するかどうか
/// （要件 7.5）は 4.2 が決める。
///
/// # 戻り値
///
/// 第 2 要素は 4.2 が復旧の事実を載せる受け口であり、4.1 は常に空の [`OpenReport`] を返す。
pub fn open(directory: &Path) -> Result<(Arc<FileSettingsStore>, OpenReport), SettingsError> {
    fs::create_dir_all(directory).map_err(|source| SettingsError::DirectoryUnavailable {
        path: directory.to_path_buf(),
        source,
    })?;
    // 綴り違い（`./` を挟む等）でも同じ実体へ寄せるため、正規化したパスをキーにする。
    let canonical = fs::canonicalize(directory).map_err(|source| SettingsError::DirectoryUnavailable {
        path: directory.to_path_buf(),
        source,
    })?;

    let mut entries = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = entries.get(&canonical).and_then(Weak::upgrade) {
        return Ok((existing, OpenReport::default()));
    }

    // 登録簿のロックを保持したまま読み込む。同じディレクトリへの並行 `open` が二重に
    // 読み込んで別の実体を作ることを防ぐ。
    let store = Arc::new(FileSettingsStore::load(&canonical)?);
    entries.insert(canonical, Arc::downgrade(&store));
    Ok((store, OpenReport::default()))
}

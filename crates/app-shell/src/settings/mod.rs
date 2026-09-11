//! 設定の永続化（要件 7.1〜7.7。design.md「Core Layer / SettingsStore」）。
//!
//! 設定ストアは自前で持つ（`tauri-plugin-store` は採らない。research.md 決定 6）。理由は、
//! プラグインの保存が truncate-then-write で原子的でないこと、未知キーの保持（要件 7.6）と
//! 破損時の既定値起動（要件 7.5）の意味論を持たないことである。
//!
//! 本モジュール（tasks.md 4.1、4.2、4.3）が持つのは次のものである:
//!
//! - 原子的な置き換え。[`atomic`] が同一ディレクトリの一時ファイル → `sync_all` → `rename`
//!   の順で行う（truncate-then-write はしない）
//! - 保存先の解決。各 OS が定める標準のアプリケーションデータ領域の下に識別子を足す
//!   （要件 7.2。[`app_data_base_dir`] / [`app_data_dir`]）
//! - 閉じたキー空間 [`SettingsKey`] と、カタログのメタ [`SUPPORTED_SCHEMA_VERSION`]（要件 7.7）
//! - 同一ディレクトリにつき 1 つの共有実体を返す [`open`]（要件 7.3）
//! - [`SettingsStore::get`] / [`SettingsStore::set`] と、書き込みの直列化（要件 7.1）
//! - 未知キーの保持（要件 7.6）と、読み取りに失敗したときの既定値起動・報告（要件 7.5）
//! - 値の変更を購読者へ配る [`SettingsStore::subscribe`]（要件 7.4）
//!
//! ファイルはキーから値への 1 つの JSON オブジェクトである（design.md「Logical Data Model」
//! の Settings 構造）。値は [`serde_json::Value`] として読み書きする。
//!
//! # キー空間（要件 7.7）
//!
//! 保存できるのは [`SettingsKey`] が列挙するシェルの設定だけである。`SettingsKey` は**閉じた
//! 列挙**であり、文字列から鍵を作る公開の入口は [`SettingsKey::from_name`] だけである。これは
//! カタログにある名前しか受け付けないため、任意の名前 — とりわけドキュメントの内容（セル値・
//! 行・スキーマ）を指す名前 — を設定の鍵として持ち込む API は存在しない。[`SettingsStore::set`]
//! は `T: Serialize` の総称であり（design.md「Service Interface」の署名）、型の上では任意の値を
//! シェルの鍵に載せられてしまう。この一点だけは型では閉じられないため、鍵空間の閉性と合わせて
//! レビューで支える（「シェルの設定以外を保存しない」という規則そのものである）。
//!
//! # 未知キーの保持（要件 7.6）
//!
//! このモジュールが [`SettingsKey`] として解釈しないキー（別の版が書いた項目など）は、値の形を
//! 変えずに保持し、書き戻す。`document-format` が確立した「理解できないものを壊さない」原則の
//! 継承である。仕組みは単純で、[`SettingsStore::set`] は書き込みのたびに**保持している写像の
//! 全体**を直列化する。`set` が差し替えるのは要求された鍵の値だけで、他の鍵を落とす経路は無い。
//!
//! # 読み取り失敗時の既定値起動（要件 7.5）
//!
//! 設定ファイルの内容を読み取れない場合、[`open`] は失敗を返さず、空の設定（= 既定値）で起動し、
//! [`OpenReport::recovered_from`] に事実を載せる。**起動を中止しない。ファイルを削除しない**
//! （利用者が内容を確認できるようにする）。復旧として扱うのは次の 3 つである:
//!
//! - 内容を読み取れない（権限がない、設定パスがディレクトリである等）— [`RecoveryCause::Unreadable`]
//! - JSON オブジェクトとして解釈できない（0 バイト、不正な JSON、オブジェクトでない JSON）—
//!   [`RecoveryCause::Malformed`]
//! - `schema_version` が現行版と一致しない — [`RecoveryCause::UnsupportedSchemaVersion`]
//!
//! [`open`] が `Err` を返すのは、**保存先ディレクトリを用意できない場合だけ**である
//! （[`SettingsError::DirectoryUnavailable`]）。これは「設定の内容を読めなかった」ではなく
//! 「そもそも置き場所が無い」であり、起動の前提が成立していない（design.md「Error Handling」の
//! 「起動時の前提不成立」）。
//!
//! # schema_version（design.md「Logical Data Model」）
//!
//! [`SettingsKey::SchemaVersion`] の値が現行版 [`SUPPORTED_SCHEMA_VERSION`] と一致しないファイルは、
//! 別の版のものとして**解釈しない**。既定値で起動し、[`RecoveryCause::UnsupportedSchemaVersion`]
//! を報告する。検出は「`schema_version` が存在し、その値が現行版と等しくない」ことであり、整数と
//! して読めない値も同じ経路に落ちる（報告の `found` が `None` になる）。版を持たないファイルは
//! 現行版として扱う（4.1 以前が書いたファイルと、版を書かない現行の書き手のため）。
//!
//! 現行の `set` は `schema_version` を自動では書かない。版を書くのはファイルを所有する書き手
//! （アダプタ層）の責務であり、本モジュールは版を読んで解釈だけをする。カタログに
//! [`SettingsKey::SchemaVersion`] があるため、書き手は `set` で版を保存できる。
//!
//! # 変更の通知（要件 7.4）
//!
//! [`SettingsStore::subscribe`] は購読者ごとに独立した [`std::sync::mpsc`] の受信側を返す。
//! 実体は全ウィンドウで共有される（要件 7.3）ため、購読者の登録も同じ実体内で直列化する。
//! 配布は内部の購読者登録簿が行い、3.4 のサイドカー出来事と同じ「購読者ごとの無限容量 `mpsc` を
//! `Mutex<Vec<Sender>>` に登録する」方式である。送信に失敗した（受信側を破棄した）購読者は
//! 配布のたびに表から外すので、**購読を捨てた利用者は他の購読者への配布も `set` の成功も
//! 妨げない**。無限容量である代償として、受信を止めた購読者の待ち行列はその分だけ伸びる —
//! **取りこぼしはしないが、遅い購読者はメモリを消費する**（3.4 と同じ選択。設定の変更は
//! 小さく頻度も低いため、取りこぼしより到着を優先する）。
//!
//! ## いつ配るか
//!
//! 通知は、値の差し替えと [`atomic::replace`] によるファイルへの書き込みの**両方が成功した後**に
//! 配る。[`SettingsStore::set`] が失敗を返す場合（エンコード失敗・書き込み失敗）は通知しない。
//! 書き込みの前に配ると、通知を受けた購読者が `get` で読む値がまだ古い、という窓ができる。
//! 要件 7.4 は「他のウィンドウにも変更後の値を反映する」こと、すなわち通知を受けた側が
//! **変更後の値**を観測できることを求めるので、耐久化の後に配る。
//!
//! ## 何を通知するか
//!
//! [`SettingsChanged`] が変更された鍵と変更後の値を運ぶ。値は [`serde_json::Value`] であり、
//! 購読者は `get` を呼ばずに通知だけで新しい値を読める（`get` を呼ぶ場合も、この時点で
//! 新しい値がメモリとディスクの両方にある）。
//!
//! **同一値の `set` は通知しない**。要件 7.4 の引き金は「設定が**変更された**とき」であり、
//! 同じ値を書いても観測できる状態は変わらないためである。書き込み自体は従来どおり行う
//! （4.1 の挙動を変えない）。これは「書き込み」ではなく「変更」の意味論である。
//!
//! **購読前の変更は再生しない**。戻り値は呼び出し以降の通知だけを運ぶ。購読時点の値は
//! [`SettingsStore::get`] で読む（再生の有無を呼び出し側が仮定しなくて済む）。
//!
//! ## ロック順（デッドロックしないこと）
//!
//! `set` は通知の順序札（`publish_order`）を、**書き込みロックを保持したまま**取る。
//! こうすると通知を出す順序が書き込みの順序と一致する（順序札を取れる順序が
//! 書き込みの順序そのものである）。その後に書き込みロックを解放し、購読者表のロックを取って
//! 配る。したがってロックの入れ子は常に `state` → `publish_order` → `subscribers` の順で、
//! 循環しない。**書き込みロックを解放してから配る**ので、配布中に購読者が `get` / `set` で
//! 再入しても、その購読者は自分のスレッドで `state` を取るだけであり、`set` は既に `state` を
//! 手放している。また [`Sender::send`]（無限容量）はブロックもせず、受信側の処理を同期実行
//! することもない。よって購読者がストアを再入してもデッドロックしない。
//!
//! # design.md からの意図的な差異
//!
//! - design.md の `open` は戻り値を `Arc<dyn SettingsStore>` と書くが、[`SettingsStore`] の
//!   [`get`](SettingsStore::get) / [`set`](SettingsStore::set) は型引数を持つ汎用メソッドであり、
//!   そのような trait は object-safe ではない（`dyn` にできない。E0038）。境界で型付きの値を
//!   得ることを優先し、[`open`] は実体の `Arc` を返す。object-safe な層が必要になった時点で、
//!   そのとき raw な非汎用メソッドを足せばよい
//! - design.md の `open` の第 2 要素は `Option<RecoveredFrom>` だが、本モジュールは [`OpenReport`]
//!   を返す。復旧の事実は [`OpenReport::recovered_from`] で取り出せる。4.1 が確立した戻り値の形を
//!   変えないための受け口である

pub mod atomic;

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
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

/// 現行の設定ファイルの版（[`SettingsKey::SchemaVersion`]）。
///
/// これと一致しない版を持つファイルは解釈せず、既定値で起動して復旧の事実を報告する
/// （design.md「Logical Data Model」の `schema_version` の規則）。
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// キー空間（要件 7.7）
// ---------------------------------------------------------------------------

/// 設定のキー。**閉じた列挙**であり、これが保存できる鍵の全体である（design.md
/// 「Logical Data Model」の Settings 構造。要件 7.7）。
///
/// ファイルに載るのは [`as_str`](SettingsKey::as_str) が返す安定した文字列（`window.geometry`
/// のようなドット区切りの名前）である。名前を変えると保存済みの値が読めなくなるため、公開後は
/// 変更しない。[`SettingsKey::SchemaVersion`] だけがメタ情報であり、他はシェルの設定値である。
///
/// **閉じていることの意味**: 文字列から鍵を作る公開の入口は [`from_name`](SettingsKey::from_name)
/// だけで、これはカタログにある名前しか受け付けない。任意の名前 — とりわけドキュメントの内容を
/// 指す名前 — を設定の鍵として持ち込む API は存在しない（要件 7.7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SettingsKey {
    /// 設定ファイルの版（メタ）。値は整数。
    SchemaVersion,
    /// 直近に閉じられたウィンドウの位置とサイズ（要件 2.7）。値はオブジェクト。
    WindowGeometry,
    /// 外観。`system` / `light` / `dark`（要件 9.3、9.4）。値は文字列。
    AppearanceTheme,
    /// 記録の詳細度（要件 8.7）。値は詳細度を表す文字列。
    DiagnosticsLevel,
    /// 前回の起動で描画が成立しなかった印（要件 10.3）。値は真偽。
    RenderFallback,
}

impl SettingsKey {
    /// カタログの全体（閉じたキー空間）。並びはファイルに載る名前の昇順である。
    pub const ALL: [SettingsKey; 5] = [
        SettingsKey::AppearanceTheme,
        SettingsKey::DiagnosticsLevel,
        SettingsKey::RenderFallback,
        SettingsKey::SchemaVersion,
        SettingsKey::WindowGeometry,
    ];

    /// ファイルとエラー表示に載る安定した文字列。
    pub const fn as_str(self) -> &'static str {
        match self {
            SettingsKey::SchemaVersion => "schema_version",
            SettingsKey::WindowGeometry => "window.geometry",
            SettingsKey::AppearanceTheme => "appearance.theme",
            SettingsKey::DiagnosticsLevel => "diagnostics.level",
            SettingsKey::RenderFallback => "render.fallback",
        }
    }

    /// ファイルに載る名前から鍵を引く。
    ///
    /// **カタログにある名前だけを受け付け**、それ以外は `None` を返す。文字列から鍵を作る唯一の
    /// 入口であり、この閉性がキー空間の閉性そのものである（要件 7.7）。
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|key| key.as_str() == name)
    }
}

impl fmt::Display for SettingsKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
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
pub fn app_data_base_dir_with(
    lookup: &dyn Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, SettingsError> {
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
///
/// 診断の保存先（tasks.md 4.4）も macOS / Windows ではアプリケーションデータ領域とは別の
/// 環境変数（`HOME` / `LOCALAPPDATA`）を読むため、この読み取り規則だけを `pub(crate)` で
/// 共有する。4.1 のデータ領域の解決そのものは Linux の診断保存先だけが再利用する。
pub(crate) fn non_empty_env(
    lookup: &dyn Fn(&str) -> Option<OsString>,
    name: &str,
) -> Option<PathBuf> {
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
///
/// **設定の内容を読めなかったことは失敗ではない**（要件 7.5）。それは既定値で起動して
/// [`OpenReport`] で報告する事実であり、`Err` は起動の前提が崩れた場合に限る。
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// OS 標準のアプリケーションデータ領域を決められない（環境変数が無い等）。
    #[error("アプリケーションデータ領域を解決できない: {reason}")]
    AppDataDirUnavailable { reason: String },

    /// 保存先ディレクトリを用意できない。**起動の前提が成立していない**（design.md
    /// 「Error Handling」の「起動時の前提不成立」）。
    #[error("設定ディレクトリを用意できない: {path}: {source}")]
    DirectoryUnavailable { path: PathBuf, source: io::Error },

    /// 設定値を JSON へ変換できない。
    #[error("設定値を JSON に変換できない: {key}: {source}")]
    EncodeFailed {
        key: SettingsKey,
        source: serde_json::Error,
    },

    /// 書き込みに失敗した。**対象ファイルは直前の完全な内容のままである**（[`atomic`] の保証）。
    #[error("設定ファイルを書き込めない: {path}: {source}")]
    WriteFailed { path: PathBuf, source: io::Error },
}

// ---------------------------------------------------------------------------
// 境界
// ---------------------------------------------------------------------------

/// 設定ストアのサービスインタフェース（design.md「SettingsStore / Service Interface」）。
///
/// `Send + Sync` であり、1 つの実体を全ウィンドウで共有する（要件 7.3）。
pub trait SettingsStore: Send + Sync {
    /// 保存されている値を型付きで返す。存在しないキーと、保存されている値が `T` として
    /// 解釈できない場合はどちらも `None`（design の `get` は失敗を返す経路を持たない）。
    fn get<T: DeserializeOwned>(&self, key: &SettingsKey) -> Option<T>;

    /// 値を保存してから返る。戻った時点で値はメモリとディスクの両方にある。
    ///
    /// 鍵は [`SettingsKey`]（閉じたカタログ）に限られる。**このモジュールが解釈しない鍵は
    /// 書き込みで落ちない** — 保持している写像の全体を直列化するためである（要件 7.6）。
    ///
    /// 失敗した場合、メモリとディスクのどちらも変更前のままである。
    fn set<T: Serialize>(&self, key: &SettingsKey, value: &T) -> Result<(), SettingsError>;

    /// 変更通知を購読する（design.md「SettingsStore / Service Interface」、要件 7.4）。
    ///
    /// 戻り値は**この呼び出し以降**の通知だけを運ぶ（購読前の変更は再生しない。購読時点の値は
    /// [`get`](SettingsStore::get) で読む）。複数の購読は互いに独立であり、1 つの受信側を
    /// 破棄しても他の購読者への配布と `set` の成功は妨げられない。
    fn subscribe(&self) -> Receiver<SettingsChanged>;
}

// ---------------------------------------------------------------------------
// 変更通知（要件 7.4）
// ---------------------------------------------------------------------------

/// 設定が変更されたという通知（design.md「Requirements Traceability」の `SettingsChanged`）。
///
/// [`SettingsStore::subscribe`] の受信側が受け取る。変更された鍵と、変更後の値を運ぶ。値は
/// 保存された生の JSON（`set` に渡された値の符号化）であり、購読者は [`SettingsStore::get`] を
/// 呼ばずに通知だけで新しい値を読める。型付きで読みたい場合は `get::<T>(&event.key)` を使う
/// （通知の時点で新しい値がメモリとディスクの両方にある。module doc「変更の通知」を参照）。
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsChanged {
    /// 変更された鍵。ワイルドカードではなく、変更されたその鍵である。
    pub key: SettingsKey,
    /// 変更後の値（保存された生の JSON）。
    pub value: Value,
}

/// 購読者へ通知を配る登録簿（3.4 のサイドカーの出来事と同じ方式）。
///
/// 購読者ごとに独立した送信路を持つ。1 つの購読者が受信を止めても他の購読者と `set` は止まらない
/// （送信路は無限容量であり [`Sender::send`] はブロックしない）。代償として、受信を止めた購読者の
/// 待ち行列はその分だけ伸びる — **取りこぼしはしないが、遅い購読者はメモリを消費する**。
#[derive(Debug, Default)]
struct Subscribers {
    /// 購読者ごとの送信路。送信に失敗した（受信側を破棄した）ものは配布のたびに外す。
    senders: Mutex<Vec<Sender<SettingsChanged>>>,
}

impl Subscribers {
    /// 新しい購読を作る。戻り値はこの呼び出し以降の通知を受け取る。
    fn subscribe(&self) -> Receiver<SettingsChanged> {
        let (sender, receiver) = mpsc::channel();
        self.lock().push(sender);
        receiver
    }

    /// すべての購読者へ同じ通知を配る。
    ///
    /// 購読者表のロックを保持したまま送る。`Sender::send`（無限容量）はブロックしないため、
    /// 遅い購読者がいても `set` は止まらない。送信に失敗した送信路はここで外れる。
    fn publish(&self, event: SettingsChanged) {
        let mut senders = self.lock();
        senders.retain(|sender| sender.send(event.clone()).is_ok());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Sender<SettingsChanged>>> {
        self.senders.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// ---------------------------------------------------------------------------
// 復旧の報告（要件 7.5）
// ---------------------------------------------------------------------------

/// 既定値で起動するに至った原因。design.md の `RecoveredFrom` が運ぶ事実である。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryCause {
    /// 設定ファイルの内容を読み取れなかった（権限がない、設定パスがディレクトリである等）。
    /// **ファイルは削除も置換もしない**。
    Unreadable,
    /// 内容が JSON オブジェクトとして解釈できない（0 バイト、不正な JSON、オブジェクトでない
    /// JSON）。**ファイルは削除も置換もしない**。
    Malformed,
    /// `schema_version` が現行版 [`SUPPORTED_SCHEMA_VERSION`] と一致しない。
    ///
    /// `found` は検出した値。整数として読めない値の場合は `None`（いずれも「その版は知らない」
    /// という同じ扱いになる）。
    UnsupportedSchemaVersion { found: Option<i64> },
}

impl fmt::Display for RecoveryCause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecoveryCause::Unreadable => formatter.write_str("設定ファイルを読み取れなかった"),
            RecoveryCause::Malformed => {
                formatter.write_str("設定ファイルを JSON オブジェクトとして解釈できなかった")
            }
            RecoveryCause::UnsupportedSchemaVersion { found: Some(found) } => {
                write!(
                    formatter,
                    "設定ファイルの schema_version が未知（検出値 {found}）"
                )
            }
            RecoveryCause::UnsupportedSchemaVersion { found: None } => {
                formatter.write_str("設定ファイルの schema_version が整数として読めない")
            }
        }
    }
}

/// 読み取れなかった設定ファイルと、その原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredFrom {
    /// 対象の設定ファイルのパス。**削除していない**（利用者が内容を確認できる）。
    pub path: PathBuf,
    /// 既定値で起動した原因。
    pub cause: RecoveryCause,
}

impl fmt::Display for RecoveredFrom {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.cause, self.path.display())
    }
}

/// [`open`] が返す第 2 要素。design.md の `Option<RecoveredFrom>` に対応する受け口である。
///
/// 設定ファイルを読めた場合は [`OpenReport::default`]（復旧なし）である。既定値で起動した場合は
/// [`OpenReport::recovered_from`] が事実を返す。呼び出し側（アダプタ層）はこれを診断情報へ
/// 記録する（要件 7.5）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenReport {
    recovered_from: Option<RecoveredFrom>,
}

impl OpenReport {
    /// 既定値で起動した事実。設定ファイルを読めた場合は `None`。
    pub fn recovered_from(&self) -> Option<&RecoveredFrom> {
        self.recovered_from.as_ref()
    }
}

// ---------------------------------------------------------------------------
// 実体
// ---------------------------------------------------------------------------

/// メモリ上の状態。書き込みロックがこの全体を覆う。
struct State {
    /// 設定ファイルのパス（正規化したディレクトリ + [`SETTINGS_FILE_NAME`]）。
    file: PathBuf,
    /// ファイルの中身。キーは [`SettingsKey::as_str`]、値は生の JSON。
    ///
    /// このモジュールが解釈しないキーもこの写像にそのまま載るため、書き戻しで失われない
    /// （要件 7.6）。
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
    /// 通知の順序札。書き込みロックを保持したまま取ることで、通知を出す順序を書き込みの順序に
    /// 一致させる（module doc「ロック順」）。配布そのものはこの札を保持して行う。
    publish_order: Mutex<()>,
    /// 変更通知の購読者（要件 7.4）。
    subscribers: Subscribers,
    /// 読み込み時に既定値で起動した事実。実体が生きている間は同じ事実を返す
    /// （復旧はこの実体の読み込みについての事実であり、途中の書き込みで消える性質のものではない）。
    recovered_from: Option<RecoveredFrom>,
}

impl FileSettingsStore {
    /// ディレクトリから読み込む。**内容の失敗では `Err` を返さない**（要件 7.5）。
    ///
    /// ファイルが無ければ空から始める。内容を読み取れない・解釈できない・版が未知の場合は、
    /// 空の写像（= 既定値）から始め、事実を第 2 要素に載せる。対象のファイルには触れない。
    fn load(directory: &Path) -> (State, Option<RecoveredFrom>) {
        let file = directory.join(SETTINGS_FILE_NAME);
        let bytes = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return (
                    State {
                        file,
                        values: Map::new(),
                    },
                    None,
                );
            }
            Err(_) => {
                return (
                    State {
                        file: file.clone(),
                        values: Map::new(),
                    },
                    Some(RecoveredFrom {
                        path: file,
                        cause: RecoveryCause::Unreadable,
                    }),
                );
            }
        };

        match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Object(values)) => match unsupported_schema_version(&values) {
                None => (State { file, values }, None),
                Some(found) => (
                    State {
                        file: file.clone(),
                        values: Map::new(),
                    },
                    Some(RecoveredFrom {
                        path: file,
                        cause: RecoveryCause::UnsupportedSchemaVersion { found },
                    }),
                ),
            },
            Ok(_) | Err(_) => (
                State {
                    file: file.clone(),
                    values: Map::new(),
                },
                Some(RecoveredFrom {
                    path: file,
                    cause: RecoveryCause::Malformed,
                }),
            ),
        }
    }

    /// この実体の読み込みについての報告。
    fn open_report(&self) -> OpenReport {
        OpenReport {
            recovered_from: self.recovered_from.clone(),
        }
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

/// `schema_version` が現行版と一致しなければ、その値を返す。版が無い場合と現行版の場合は `None`。
///
/// 整数として読めない値も `Some(None)` になる（未知の版として扱う）。
fn unsupported_schema_version(values: &Map<String, Value>) -> Option<Option<i64>> {
    match values.get(SettingsKey::SchemaVersion.as_str()) {
        None => None,
        Some(value) if value.as_i64() == Some(i64::from(SUPPORTED_SCHEMA_VERSION)) => None,
        Some(value) => Some(value.as_i64()),
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
        // 値が実際に変わる場合だけ通知を作る（同一値の `set` は「変更」ではない。module doc
        // 「何を通知するか」）。複製はこの経路だけに閉じる。
        let changed = state.values.get(key.as_str()) != Some(&encoded);
        let notification = if changed { Some(encoded.clone()) } else { None };
        let previous = state.values.insert(key.as_str().to_owned(), encoded);
        // **写像の全体**を直列化する。これが未知キーの保持そのものである（要件 7.6）:
        // カタログに無い鍵も値の形を変えずにそのまま書き戻る。ここで鍵を絞る経路を作っては
        // ならない。
        let bytes = match serde_json::to_vec(&state.values) {
            Ok(bytes) => bytes,
            Err(source) => {
                restore(&mut state.values, key, previous);
                return Err(SettingsError::EncodeFailed { key: *key, source });
            }
        };
        match atomic::replace(&state.file, &bytes) {
            Ok(()) => {}
            Err(source) => {
                // 失敗した書き込みをメモリに残さない（ファイルとメモリを食い違わせない）。
                restore(&mut state.values, key, previous);
                return Err(SettingsError::WriteFailed {
                    path: state.file.clone(),
                    source,
                });
            }
        }

        // ここから通知である。**書き込みロックを保持したまま**順序札を取る — 札を取れる順序が
        // 書き込みの順序そのものなので、通知の到着順が書き込みの順序と一致する。札を取った後で
        // 書き込みロックを解放し、購読者表のロックを取って配る。したがってロックの入れ子は常に
        // state → publish_order → subscribers の順である（module doc「ロック順」）。
        let order = if notification.is_some() {
            Some(
                self.publish_order
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            )
        } else {
            None
        };
        drop(state);
        if let Some(value) = notification {
            // 耐久化が成功した後に配る。購読者が `get` で読む値は既に新しい（要件 7.4）。
            self.subscribers
                .publish(SettingsChanged { key: *key, value });
        }
        drop(order);
        Ok(())
    }

    fn subscribe(&self) -> Receiver<SettingsChanged> {
        self.subscribers.subscribe()
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
/// 作り、正規化したパスで登録簿を引く。既存の設定ファイルはここで読み込む。既存の実体を返す
/// 場合も、その実体の読み込みについての [`OpenReport`] を返す（復旧の事実は実体ごとに一定）。
///
/// # Errors
///
/// 保存先ディレクトリを用意できない場合（[`SettingsError::DirectoryUnavailable`]）だけを返す。
/// 設定の内容を読み取れない場合は失敗ではなく、既定値で起動して [`OpenReport`] に事実を載せる
/// （要件 7.5）。
pub fn open(directory: &Path) -> Result<(Arc<FileSettingsStore>, OpenReport), SettingsError> {
    fs::create_dir_all(directory).map_err(|source| SettingsError::DirectoryUnavailable {
        path: directory.to_path_buf(),
        source,
    })?;
    // 綴り違い（`./` を挟む等）でも同じ実体へ寄せるため、正規化したパスをキーにする。
    // **正規化したパスは登録簿のキーにだけ使う。** 読み込み・書き込み・復旧の報告には
    // 使わない: macOS では `/var` が `/private/var` に、Windows では `C:\…` が `\\?\C:\…`
    // （8.3 形式の短い名前も展開される）になり、呼び出し側の知らない綴りがユーザー向けの
    // 復旧の報告に出てしまう（CI の macOS / Windows で実際に食い違った）。
    let canonical =
        fs::canonicalize(directory).map_err(|source| SettingsError::DirectoryUnavailable {
            path: directory.to_path_buf(),
            source,
        })?;
    // 読み込みと書き込みの対象は、呼び出し側の綴りを絶対パスにしたものである（シンボリック
    // リンクを解決せず、`\\?\` 形式にもしない）。絶対パスにするのは、相対パスで開いた後に
    // カレントディレクトリが変わっても書き込み先が動かないようにするため。
    let absolute =
        std::path::absolute(directory).map_err(|source| SettingsError::DirectoryUnavailable {
            path: directory.to_path_buf(),
            source,
        })?;

    let mut entries = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = entries.get(&canonical).and_then(Weak::upgrade) {
        let report = existing.open_report();
        return Ok((existing, report));
    }

    // 登録簿のロックを保持したまま読み込む。同じディレクトリへの並行 `open` が二重に
    // 読み込んで別の実体を作ることを防ぐ。
    let (state, recovered_from) = FileSettingsStore::load(&absolute);
    let store = Arc::new(FileSettingsStore {
        state: RwLock::new(state),
        publish_order: Mutex::new(()),
        subscribers: Subscribers::default(),
        recovered_from,
    });
    let report = store.open_report();
    entries.insert(canonical, Arc::downgrade(&store));
    Ok((store, report))
}

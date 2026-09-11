//! ドキュメント所有者への委譲点 — ウィンドウを閉じてよいかの問い合わせと、選択された
//! ファイルの引き渡しを受け取る契約（要件 2.1、2.6）。
//!
//! 所有: `DocumentHostPort`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 2.1、2.6。design.md の契約の形:
//!
//! ```text
//! trait DocumentHost {
//!     fn may_close(&self, window: &WindowId) -> CloseVerdict;
//!     fn attach(&self, window: &WindowId, path: &Path) -> Result<(), AttachError>;
//! }
//! ```
//!
//! # このポートの存在理由
//!
//! 要件 2.1 / 2.6 は「ドキュメントを所有する機能」への委譲を要求するが、**その機能は本スペック
//! の時点で存在しない**。そこで本スペックが契約だけを定義し、**常に許可し、引き渡しを受けても
//! 何もしない既定実装**（[`DefaultDocumentHost`]）を同梱する。ウィンドウとドキュメントを結ぶ
//! 下流スペック（および将来のドキュメント所有者）が差し替える。**このポートの所有権は本スペック
//! （app-shell）にあり、契約の変更は design.md「Revalidation Triggers」に挙げられた統合の
//! 再確認を依存スペックへ強いる。**
//!
//! # 決定事項（design.md からの逸脱ではなく、その空白の充填）
//!
//! - **ウィンドウの識別子は [`WindowLabel`] を使う。** design.md の模式図は
//!   `WindowId` と書くが、その名前の具体型はリポジトリに存在しない。ウィンドウの識別子は既に
//!   [`WindowLabel`] として通信境界（要件 4.6 の `WindowContext`）とウィンドウのレジストリ
//!   （タスク 6.1）の両方で**単一の定義**になっており、ここで 3 つ目の識別子を新設すると
//!   両者の間に写像が必要になる。`WebviewWindow` が持つ Tauri 側のラベル文字列は
//!   `WindowLabel::new` でこの型へ写す。
//! - **判定は [`CloseVerdict`]**（許可 / 拒否）。`Result` にしないのは、**どのウィンドウに対しても
//!   答えることが契約**であり、「答えられない」という状態を持たないためである。拒否は利用者へ
//!   伝えるための理由（[`CloseVerdict::Deny`]）を運ぶ。**利用者に見せる文言を決めるのは
//!   終了拒否の仲介（タスク 7.6）であり**、ここは材料だけを運ぶ。
//! - **引き渡しの失敗は [`AttachError`]**。運ぶのは説明文字列だけである。**アプリケーション
//!   シェルは渡されたパスを読まない・開かない・複製しない**（design.md「DialogGate」: 本機能は
//!   パスを読まない）。ポートが保証するのは「パスを所有者へ引き渡そうとした結果」だけであり、
//!   パスの中身の解釈は所有者の責務である。既定実装の `attach` は何もせず成功を返す。
//! - **メソッドは同期である**（design.md の形のまま）。呼び出しはブロックしてはならない —
//!   基盤（Tauri ランタイム）は `CloseRequestApi::prevent_close()` を**非ブロッキングに読む**ため、
//!   「待ってから拒否する」ことはできない（research.md「ウィンドウを閉じる操作の拒否」）。
//!   非同期の往復は**フロントエンド側の `onCloseRequested` リスナ経路（タスク 7.6）に載せる**。
//!   したがって [`DocumentHost::may_close`] は**現在の状態を即座に返す**契約であり、この実装に
//!   ブロックする待ち合わせを入れてはならない。
//!
//! # 差し替え（下流スペックの接続点）
//!
//! [`DocumentHostPort`] はアプリ全体で 1 実体の管理状態であり、**中身の `Arc<dyn DocumentHost>`
//! を差し替えられる**。`Manager::manage<T>()` は型ごとに 1 回しか置けず後から置換できないため、
//! 「ポート（不変の置き場）」と「宿主（差し替え可能な実装）」を分けている。下流スペックは次の
//! どちらかで自前の実装を入れる:
//!
//! - 構築時に `lifecycle::run` の `.manage(DocumentHostPort::default())` を
//!   `.manage(DocumentHostPort::new(Arc::new(自分の実装)))` へ置き換える、または
//! - 自分の `setup` フック（`Builder::build` の中で走り、`app.run` より前）で
//!   `app.state::<DocumentHostPort>().install(Arc::new(自分の実装))` を呼ぶ。
//!
//! 消費側（タスク 7.6 の終了拒否の仲介、タスク 7.7 のネイティブファイル選択）は
//! `app.state::<DocumentHostPort>()` を取り、[`DocumentHostPort::may_close`] /
//! [`DocumentHostPort::attach`] を呼ぶ。**判定と引き渡しは必ずこのポートを経由する**
//! （実装を直接掴まない）。

use std::fmt;
use std::path::Path;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

// 検証専用の委譲先（`verification-triggers` の下でのみ存在する）が使う。
#[cfg(feature = "verification-triggers")]
use std::path::PathBuf;
#[cfg(feature = "verification-triggers")]
use std::sync::Mutex;

use app_shell::ipc::WindowLabel;

/// ウィンドウを閉じてよいかの判定（要件 2.6）。
///
/// **すべてのウィンドウに対して答える。**「答えられない」状態は持たない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseVerdict {
    /// 閉じてよい。
    Allow,
    /// 閉じてはならない。
    ///
    /// **既定のビルドでは構築されない** — 拒否を返すのは差し替えられた宿主（下流スペックの
    /// ドキュメント所有者、および検証ビルドの委譲先）だけである。それでも封筒の写像
    /// （[`crate::window::close`] の `to_boundary`）がこの腕を読むので、契約として必要である。
    #[allow(dead_code)]
    Deny {
        /// 拒否の理由。**利用者へそのまま見せる文言ではなく、呼び出し元が伝えるための材料**で
        /// ある（見せ方を決めるのはタスク 7.6）。空でもよい。
        reason: String,
    },
}

impl CloseVerdict {
    /// 許可かどうか。
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }

    /// 拒否の理由（許可なら `None`）。
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Allow => None,
            Self::Deny { reason } => Some(reason),
        }
    }
}

/// 選択されたファイルの引き渡しが失敗したときの原因。
///
/// **アプリケーションシェル自身の入出力の失敗ではない**（本機能はパスを読まない）。所有者が
/// 受け取れなかった理由を運ぶ。運ぶのは説明文字列だけに留め、分類は所有者の必要になった時点で
/// 足す（本スペックの時点で所有者が存在しないため、推測で種類を増やさない）。
#[allow(dead_code)]
// 差し替わる実装（7.6/7.7 と下流スペック）が構築・報告するまでの seam。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachError {
    message: String,
}

#[allow(dead_code)] // 差し替わる実装が構築・報告するまでの seam。
impl AttachError {
    /// 説明文字列から原因を作る。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// 説明文字列。
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AttachError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AttachError {}

/// ドキュメント所有者への委譲点（design.md「DocumentHostPort」）。
///
/// **メソッドは同期である。**[`may_close`](DocumentHost::may_close) はブロックしてはならない —
/// 基盤は閉じてよいかの判定を非ブロッキングに読むため、待ってから拒否することはできない
/// （ファイル冒頭の「メソッドは同期である」を参照）。非同期の往復はタスク 7.6 のフロントエンド
/// 側リスナ経路に載る。
///
/// 実装は [`Send`] + [`Sync`] でなければならない（アプリ全体の管理状態として共有されるため）。
#[allow(dead_code)] // 委譲点の呼び出し元（7.6/7.7）がこの契約を呼ぶまでの seam。
pub trait DocumentHost: Send + Sync {
    /// このウィンドウを閉じてよいか。**現在の状態を即座に返す**（ブロックしない）。
    fn may_close(&self, window: &WindowLabel) -> CloseVerdict;

    /// 選択されたファイルを引き渡す。**パスを読む・開く・複製するのは所有者の仕事であり、
    /// アプリケーションシェルは中身に触れない。**
    fn attach(&self, window: &WindowLabel, path: &Path) -> Result<(), AttachError>;
}

/// 同梱する既定実装（**下流スペックが差し替える**）。
///
/// - [`may_close`](DocumentHost::may_close) は**常に許可**を返す。
/// - [`attach`](DocumentHost::attach) は**何もしない**（パスを読まず・開かず・複製せず・記録せず、
///   `Ok(())` を返す）。
///
/// **これは未実装の経路ではなく、意図した既定の振る舞いである。** 本スペックの時点でドキュメントを
/// 所有する機能が存在せず、存在しない所有者へ閉じるのを拒否させたり、パスを受け取ったふりを
/// させたりすると、ウィンドウが閉じられなくなるか、所有者のいない引き渡しが成功したように見える。
/// 委譲先が入るまでは「許可し、何もしない」が正しい。
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultDocumentHost;

impl DocumentHost for DefaultDocumentHost {
    fn may_close(&self, _window: &WindowLabel) -> CloseVerdict {
        CloseVerdict::Allow
    }

    fn attach(&self, _window: &WindowLabel, _path: &Path) -> Result<(), AttachError> {
        // 意図的に何もしない（パスに触れない）。未実装ではない。
        Ok(())
    }
}

/// 委譲点のハンドル。**アプリ全体で 1 実体を管理状態として置く**（`Manager::manage`）。
///
/// 中身の `Arc<dyn DocumentHost>` は [`install`](Self::install) で差し替えられる。
/// `Manager::manage<T>()` は同じ型を 2 回置けず、置いた値を後から入れ替える口も持たないため、
/// 「不変の置き場（本型）」と「差し替え可能な宿主（[`DocumentHost`] の実装）」を分けている。
/// 差し替えの接続点はファイル冒頭の「差し替え（下流スペックの接続点）」を参照。
pub struct DocumentHostPort {
    /// 現在の宿主。読み手はこの [`Arc`] を複製してからロックを離すので、宿主の呼び出し中に
    /// ロックを保持しない（宿主が [`install`](Self::install) を呼び返してもデッドロックしない）。
    host: RwLock<Arc<dyn DocumentHost>>,
}

impl Default for DocumentHostPort {
    /// 既定の宿主（[`DefaultDocumentHost`]）を入れたポートを作る。**起動時に置くのはこれである。**
    fn default() -> Self {
        Self::new(Arc::new(DefaultDocumentHost))
    }
}

#[allow(dead_code)] // 委譲点の呼び出し元（7.6/7.7）と差し替え（下流スペック）が使うまでの seam。
impl DocumentHostPort {
    /// 任意の宿主を入れたポートを作る。
    pub fn new(host: Arc<dyn DocumentHost>) -> Self {
        Self {
            host: RwLock::new(host),
        }
    }

    /// 現在の宿主を差し替える。
    ///
    /// **アプリが走り出す前に呼ぶのが基本である**（`setup` フックなど）。走り出した後でも
    /// 安全に呼べるが、その時点の判定と引き渡しが新しい宿主へ切り替わる。
    pub fn install(&self, host: Arc<dyn DocumentHost>) {
        *self.write() = host;
    }

    /// 現在の宿主をトレイトオブジェクトとして取り出す（呼び出しごとの複製）。
    ///
    /// **判定そのものは [`may_close`](Self::may_close) / [`attach`](Self::attach) を使うこと。**
    /// ここで取り出した宿主は、取り出した時点のものである。
    pub fn host(&self) -> Arc<dyn DocumentHost> {
        Arc::clone(&self.read())
    }

    /// 委譲先へ「このウィンドウを閉じてよいか」を問い合わせる。**ブロックしない。**
    pub fn may_close(&self, window: &WindowLabel) -> CloseVerdict {
        self.host().may_close(window)
    }

    /// 委譲先へ選択されたファイルを引き渡す。**アプリケーションシェルはパスの中身に触れない。**
    pub fn attach(&self, window: &WindowLabel, path: &Path) -> Result<(), AttachError> {
        self.host().attach(window, path)
    }

    /// ロックを取る。**毒されていても panic しない** — 宿主の判定はイベントループの中から
    /// 呼ばれうるため、ここで panic するとウィンドウを巻き込む（タスク 6.1 の
    /// [`crate::window`] のレジストリと同じ判断）。中身は panic で壊れる不変条件を持たない。
    fn read(&self) -> RwLockReadGuard<'_, Arc<dyn DocumentHost>> {
        self.host.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// 書きロックを取る。毒されていても panic しない（[`read`](Self::read) と同じ理由）。
    fn write(&self) -> RwLockWriteGuard<'_, Arc<dyn DocumentHost>> {
        self.host.write().unwrap_or_else(PoisonError::into_inner)
    }
}

// ---------------------------------------------------------------------------
// 検証専用の委譲先（`verification-triggers` feature の下にのみ存在する）
// ---------------------------------------------------------------------------

/// 検証専用: 拒否するウィンドウのラベルを与える環境変数の名前。
///
/// **この定数は `verification-triggers` feature の下にのみ存在する。** 既定のビルド（配布物）
/// には名前自体が無いので、環境変数を読むコードを既定の経路へ繋ぐことは**コンパイルできない**。
/// 実行時の担保は `strings -a` による不在の確認である（tasks.md 5.4 の申し送りを 7.4 が
/// feature 化し、7.6 がその規約に従う）。
///
/// 値は**ラベルそのもの**（例 `empty-1`）である。名前が指すラベルのウィンドウだけを拒否し、
/// 他のラベル・未設定・空文字のときは許可する。**許可と拒否の両方の腕を同じバイナリで実測
/// できる**ようにするための形である。
#[cfg(feature = "verification-triggers")]
pub const VERIFY_DENY_CLOSE_ENV: &str = "JXCEL_VERIFICATION_DENY_CLOSE";

/// 検証専用: 名指しされたラベルのウィンドウの終了を拒否し、**引き渡された位置を記録する**
/// 委譲先。
///
/// **本番の実装ではない。** 次の 2 つの完了状態を実測するために検証ビルドにだけ同梱する。
///
/// 1. 「委譲先が拒否を返すとウィンドウが閉じず、許可を返すと閉じる」（タスク 7.6）。
///    既定実装（[`DefaultDocumentHost`]）は常に許可するので、拒否を返す実装が要る。
///    **拒否の対象は環境変数で外から選べる**（[`VERIFY_DENY_CLOSE_ENV`]）。
/// 2. 「選ばれた位置が委譲先へ届いた」（タスク 7.7。要件 2.4）。[`attach`](DocumentHost::attach)
///    が受け取った**（ウィンドウ, 位置）の組**を記録に 1 行残す。**既定実装は沈黙する**ので、
///    この記録が「引き渡しが実際に起きた」ことの唯一の実測根拠になる。
///
/// 判定と引き渡しのたびに記録へ 1 行残す。**この行が「ハードコードされた判断ではなく委譲点を
/// 通った往復である」ことの実測根拠になる**（`strings -a` の不在確認と対で使う）。
#[cfg(feature = "verification-triggers")]
#[derive(Debug)]
pub struct VerificationDocumentHost {
    /// このラベルのウィンドウだけを拒否する。`None` なら常に許可する（引き渡しの記録だけを
    /// 行う）。
    deny_label: Option<String>,
    /// `attach` が受け取った（ウィンドウ, 位置）。**検証の実測とテストが読む。**
    attachments: Mutex<Vec<(String, PathBuf)>>,
}

#[cfg(feature = "verification-triggers")]
impl VerificationDocumentHost {
    /// 拒否するラベル（無ければ `None`）を指定して作る。
    pub fn new(deny_label: Option<String>) -> Self {
        Self {
            deny_label,
            attachments: Mutex::new(Vec::new()),
        }
    }

    /// これまでに引き渡された（ウィンドウ, 位置）の一覧（記録の複製）。
    ///
    /// **検証の実測（テスト）が読む seam。** 実画面での観測は記録行
    /// （`[検証] 引き渡しを受けた: …`）で行うため、バイナリではこの入口は使われない。
    #[allow(dead_code)] // 検証ビルドのテストだけが読む。
    pub fn attachments(&self) -> Vec<(String, PathBuf)> {
        self.attachments
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[cfg(feature = "verification-triggers")]
impl DocumentHost for VerificationDocumentHost {
    fn may_close(&self, window: &WindowLabel) -> CloseVerdict {
        if self.deny_label.as_deref() == Some(window.as_str()) {
            tauri_plugin_log::log::info!("検証用の委譲先: {} の終了を拒否した", window.as_str());
            return CloseVerdict::Deny {
                reason: format!(
                    "検証用の委譲先が {} の終了を拒否した（{}）",
                    window.as_str(),
                    VERIFY_DENY_CLOSE_ENV
                ),
            };
        }
        tauri_plugin_log::log::info!(
            "検証用の委譲先: {} の終了を許可した（拒否の対象は {}）",
            window.as_str(),
            self.deny_label.as_deref().unwrap_or("(なし)"),
        );
        CloseVerdict::Allow
    }

    fn attach(&self, window: &WindowLabel, path: &Path) -> Result<(), AttachError> {
        // **本番の実装ではない。パスに触れず、受け取った組を記録するだけである**（記録は
        // 検証ビルドにしか存在しない）。
        self.attachments
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((window.as_str().to_owned(), path.to_path_buf()));
        tauri_plugin_log::log::info!(
            "[検証] 引き渡しを受けた: ウィンドウ = {} 位置 = {}",
            window.as_str(),
            path.display(),
        );
        Ok(())
    }
}

/// 検証専用: 記録を行う委譲先を入れたポートを返す（`verification-triggers` の下でのみ
/// コンパイルされる）。
///
/// **拒否は環境変数 [`VERIFY_DENY_CLOSE_ENV`] がラベルを名指ししたときだけ**起きる。未設定・
/// 空なら常に許可し、**引き渡しの記録だけを行う**（タスク 7.7 の実測はこの形で行う）。
#[cfg(feature = "verification-triggers")]
pub fn verification_port() -> DocumentHostPort {
    let deny_label = std::env::var(VERIFY_DENY_CLOSE_ENV)
        .ok()
        .filter(|label| !label.is_empty());
    DocumentHostPort::new(Arc::new(VerificationDocumentHost::new(deny_label)))
}

/// 既定のビルドの委譲点が [`DefaultDocumentHost`] だけで組まれることをコンパイル時に表明する。
///
/// 既定のビルドでは検証専用の型・定数（[`VerificationDocumentHost`] /
/// [`VERIFY_DENY_CLOSE_ENV`]）が**どれも定義されない**。したがって既定の経路
/// （`crate::lifecycle::document_host_port`）がそれらを名指すコードは未解決の名前になり、
/// コンパイルできない — これが「配布物に検証専用の拒否を入れない」ことの主要な担保である。
/// この `const` はそのうえで、既定の経路が [`DocumentHostPort::default`]（＝常に許可する
/// [`DefaultDocumentHost`]）を指すことを型で名指しする（拒否の不在そのものは、既定ビルドで
/// のみコンパイルされる `the_default_build_cannot_deny` と `strings -a` が実測する）。
#[cfg(not(feature = "verification-triggers"))]
const _: fn() -> DocumentHostPort = DocumentHostPort::default;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use app_shell::ipc::WindowLabel;

    use super::{AttachError, CloseVerdict, DefaultDocumentHost, DocumentHost, DocumentHostPort};

    /// 差し替え後の実装が委譲を受けたことを記録するテスト用の宿主。
    /// **本番の実装ではない** — 判定と引き渡しがポートを通って届くことだけを見る。
    ///
    /// ロックの毒は [`PoisonError::into_inner`] で捨てる（本番の [`DocumentHostPort`] と同じ方針）。
    #[derive(Default)]
    struct RecordingHost {
        /// このラベルのウィンドウだけを拒否する（`None` なら常に許可）。
        deny: Option<(String, String)>,
        /// `attach` が受け取った（ウィンドウ, パス）。
        attached: Mutex<Vec<(String, PathBuf)>>,
    }

    impl RecordingHost {
        fn denying(window: &str, reason: &str) -> Self {
            Self {
                deny: Some((window.to_owned(), reason.to_owned())),
                ..Self::default()
            }
        }

        fn attached(&self) -> Vec<(String, PathBuf)> {
            self.attached
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    impl DocumentHost for RecordingHost {
        fn may_close(&self, window: &WindowLabel) -> CloseVerdict {
            match &self.deny {
                Some((target, reason)) if target == window.as_str() => CloseVerdict::Deny {
                    reason: reason.clone(),
                },
                _ => CloseVerdict::Allow,
            }
        }

        fn attach(&self, window: &WindowLabel, path: &Path) -> Result<(), AttachError> {
            self.attached
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((window.as_str().to_owned(), path.to_path_buf()));
            Ok(())
        }
    }

    fn label(raw: &str) -> WindowLabel {
        WindowLabel::new(raw)
    }

    /// 既定実装は任意のウィンドウに許可を返す。
    #[test]
    fn default_host_allows_every_window() {
        let host = DefaultDocumentHost;
        for raw in ["doc-1", "empty-1", "handover-3", ""] {
            assert_eq!(
                host.may_close(&label(raw)),
                CloseVerdict::Allow,
                "window={raw}"
            );
        }
    }

    /// 既定実装の引き渡しは成功するが、パスには一切触れない。
    /// **存在しないパスを渡しても成功する** = 読み取りを試みていない（試みれば NotFound になる）。
    #[test]
    fn default_host_attach_succeeds_without_touching_the_path() {
        let host = DefaultDocumentHost;
        let missing = missing_path();
        assert!(
            !missing.exists(),
            "前提: 存在しないパスを作れなかった: {}",
            missing.display()
        );
        let result = host.attach(&label("doc-1"), &missing);
        assert_eq!(result, Ok(()));
        assert!(
            !missing.exists(),
            "既定実装がパスを作った: {}",
            missing.display()
        );
    }

    /// 差し替えた実装の判定が**ポート経由で**得られる（拒否は理由を運ぶ）。
    #[test]
    fn installed_host_verdict_is_observed_through_the_port() {
        let port = DocumentHostPort::new(Arc::new(RecordingHost::denying(
            "doc-1",
            "未保存の変更がある",
        )));
        assert_eq!(
            port.may_close(&label("doc-1")),
            CloseVerdict::Deny {
                reason: "未保存の変更がある".to_owned()
            }
        );
        // 他のウィンドウは許可される（判定はウィンドウ単位である）。
        assert_eq!(port.may_close(&label("doc-2")), CloseVerdict::Allow);
        // 拒否が運ぶ理由は呼び出し元が読める。
        match port.may_close(&label("doc-1")) {
            CloseVerdict::Deny { reason } => assert!(!reason.is_empty()),
            other => panic!("拒否が届かなかった: {other:?}"),
        }
    }

    /// 引き渡しの委譲が差し替えた実装へ届く（ウィンドウとパスの両方がそのまま渡る）。
    #[test]
    fn attach_is_delegated_to_the_installed_host() {
        let host = Arc::new(RecordingHost::default());
        let port = DocumentHostPort::new(host.clone());
        let path = PathBuf::from("/tmp/jxcel-ports-test-doc.json");
        assert_eq!(port.attach(&label("doc-7"), &path), Ok(()));
        assert_eq!(host.attached(), vec![("doc-7".to_owned(), path.clone())]);
    }

    /// 既定のポートは引き渡しを受けても何もしない（パスを作らない）。
    #[test]
    fn default_port_records_nothing_on_attach() {
        let port = DocumentHostPort::default();
        let missing = missing_path();
        assert_eq!(port.attach(&label("empty-1"), &missing), Ok(()));
        assert!(
            !missing.exists(),
            "既定実装がパスを作った: {}",
            missing.display()
        );
    }

    /// ポートは管理状態として置ける（`manage` の要求する境界を満たす）うえ、
    /// 現在の実装を**トレイトオブジェクトとして**取り出せる。
    #[test]
    fn port_is_managed_and_reachable_as_a_trait_object() {
        fn assert_managed_state<T: Send + Sync + 'static>() {}
        assert_managed_state::<DocumentHostPort>();

        let port = DocumentHostPort::default();
        let host: Arc<dyn DocumentHost> = port.host();
        assert_eq!(host.may_close(&label("doc-1")), CloseVerdict::Allow);
    }

    /// 差し替えはポート 1 箇所で完結し、取り出したトレイトオブジェクトにも反映される。
    #[test]
    fn install_replaces_the_host_behind_the_port() {
        let port = DocumentHostPort::default();
        assert_eq!(port.may_close(&label("doc-1")), CloseVerdict::Allow);
        port.install(Arc::new(RecordingHost::denying("doc-1", "拒否")));
        assert_eq!(
            port.may_close(&label("doc-1")),
            CloseVerdict::Deny {
                reason: "拒否".to_owned()
            }
        );
        let host: Arc<dyn DocumentHost> = port.host();
        assert!(matches!(
            host.may_close(&label("doc-1")),
            CloseVerdict::Deny { .. }
        ));
    }

    /// リポジトリ内に作らない存在しないパス。プロセスごとに一意にして干渉を避ける。
    fn missing_path() -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "jxcel-ports-missing-{}-{nanos}",
            std::process::id()
        ))
    }
}

/// 既定のビルド（配布物）では**拒否の腕が 1 つも入らない**ことを固定する。
///
/// 検証専用の委譲先（[`VerificationDocumentHost`]）は `verification-triggers` の下にのみ
/// 定義されるため、このテストは既定ビルドでのみコンパイルされる。既定の委譲点が常に許可を
/// 返すことを実測し、`strings -a` による「検証専用の名前がバイナリに無い」ことの確認と対にする。
#[cfg(not(feature = "verification-triggers"))]
#[test]
fn the_default_build_cannot_deny() {
    let port = DocumentHostPort::default();
    assert_eq!(
        port.may_close(&WindowLabel::new("empty-1")),
        CloseVerdict::Allow
    );
}

/// 検証専用の委譲先の単体テスト（`verification-triggers` の下でのみコンパイルされる）。
///
/// **拒否は名指しされたラベルだけ**であることと、**引き渡された（ウィンドウ, 位置）が
/// 記録される**ことを固定する。前者が崩れると許可の腕の実測
/// （`JXCEL_VERIFICATION_DENY_CLOSE` に一致しないラベルを指定して閉じる）が成立せず、後者が
/// 崩れると「選ばれた位置が委譲先へ届いた」ことの実測根拠が消える（タスク 7.7。要件 2.4）。
#[cfg(all(test, feature = "verification-triggers"))]
mod verification_tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use app_shell::ipc::WindowLabel;

    use super::{CloseVerdict, DocumentHostPort, VerificationDocumentHost};

    fn port_with(deny_label: Option<&str>) -> DocumentHostPort {
        DocumentHostPort::new(Arc::new(VerificationDocumentHost::new(
            deny_label.map(str::to_owned),
        )))
    }

    #[test]
    fn the_verification_host_denies_only_the_named_label() {
        let port = port_with(Some("empty-1"));
        assert!(matches!(
            port.may_close(&WindowLabel::new("empty-1")),
            CloseVerdict::Deny { .. }
        ));
        assert_eq!(
            port.may_close(&WindowLabel::new("empty-2")),
            CloseVerdict::Allow
        );
    }

    #[test]
    fn an_unnamed_host_allows_every_window() {
        // 終了拒否の対象が名指しされていなくても、引き渡しの記録のためにこの委譲先は入る
        // （タスク 7.7）。そのときは 7.6 の拒否の腕が 1 つも生じない。
        let port = port_with(None);
        for label in ["empty-1", "doc-1"] {
            assert_eq!(
                port.may_close(&WindowLabel::new(label)),
                CloseVerdict::Allow
            );
        }
    }

    #[test]
    fn the_denial_carries_the_named_label_and_the_environment_variable() {
        let port = port_with(Some("doc-1"));
        let verdict = port.may_close(&WindowLabel::new("doc-1"));
        let reason = verdict.reason().expect("拒否には理由が付く");
        assert!(reason.contains("doc-1"), "理由にラベルが無い: {reason}");
        assert!(
            reason.contains(crate::ports::VERIFY_DENY_CLOSE_ENV),
            "理由に環境変数の名前が無い: {reason}"
        );
    }

    /// **引き渡しが（ウィンドウ, 位置）の組として記録される。** 7.7 の完了状態
    /// 「選ばれた位置が正しいウィンドウの委譲先へ届いた」はこの記録と記録行で実測する。
    #[test]
    fn the_verification_host_records_each_attachment_with_its_window() {
        let host = Arc::new(VerificationDocumentHost::new(None));
        let port = DocumentHostPort::new(host.clone());
        let first = PathBuf::from("/tmp/jxcel-verification-one.csv");
        let second = PathBuf::from("/tmp/jxcel-verification-two.csv");
        assert_eq!(port.attach(&WindowLabel::new("doc-1"), &first), Ok(()));
        assert_eq!(port.attach(&WindowLabel::new("empty-2"), &second), Ok(()));
        assert_eq!(
            host.attachments(),
            vec![("doc-1".to_owned(), first), ("empty-2".to_owned(), second),],
            "ウィンドウと位置の組が受け取った順に残る"
        );
    }

    /// 記録はパスに触れない（記録するだけで、存在確認も読み取りもしない）。
    #[test]
    fn recording_an_attachment_does_not_read_the_path() {
        let host = Arc::new(VerificationDocumentHost::new(None));
        let port = DocumentHostPort::new(host.clone());
        let missing =
            std::env::temp_dir().join(format!("jxcel-verification-missing-{}", std::process::id()));
        let _ = std::fs::remove_file(&missing);
        assert!(!missing.exists(), "前提: 存在しないパス");
        assert_eq!(port.attach(&WindowLabel::new("doc-1"), &missing), Ok(()));
        assert_eq!(host.attachments().len(), 1);
        assert!(!missing.exists(), "記録がパスを作った");
    }
}

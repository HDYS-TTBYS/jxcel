//! メニューの登録口とプラットフォーム差の吸収 — 個別機能がメニュー項目を登録し、選択時に
//! 登録元へ通知を受け取る。macOS はアプリ全体で 1 つのメニューしか持てないためトップレベルは
//! すべて部分メニューとし、Windows / Linux はウィンドウ単位のメニューを使う。
//!
//! 所有: `MenuSurface`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 3.1, 3.2, 3.3, 3.5, 3.6。
//!
//! タスク 7.4 が置いた実体は次の 5 つである（タスク 1.3 の骨組みをここで埋めた）。
//!
//! 1. **登録口**（[`MenuRegistry`]）。個別機能は [`MenuItemSpec`] を
//!    [`MenuRegistry::register`] へ渡して自分の項目を登録する。**登録元の識別は
//!    [`AcceleratorOwner`]**（4.6 と同じ型・同じ意味であり、ショートカットの登録元識別子と
//!    一致する）、**項目の識別は [`MenuItemId`]** である。**両者の組が登録の同一性**であり、
//!    同じ組の再登録は同じ項目の更新として扱われる（メニューは再構築されるため）。
//!    選択の通知は登録時に渡した処理（[`MenuHandler`]）へ [`MenuSelection`] を渡すことで届く。
//!    `MenuSelection` は登録元・項目・（分かれば）発生元のウィンドウを運ぶので、登録元は
//!    「自分のどの項目がどのウィンドウで選ばれたか」を知って処理を振り分けられる。
//! 2. **選択の通知経路は 1 本だけ**（[`MenuRegistry::dispatch`]）。実際のメニューイベントを
//!    受ける [`on_menu_event`] もテストもこの 1 本を通る。テスト専用の並行した実装は持たない
//!    — したがってテストが固定するのは**本番と同じ関数**の振る舞いである。
//! 3. **プラットフォーム差は 1 箇所**（[`PLACEMENT`] と [`apply`]）。macOS は
//!    `AppHandle::set_menu` でアプリ全体のメニューを 1 つだけ設定し、Windows / Linux は
//!    生成された各ウィンドウへ `Window::set_menu` で付ける。**トップレベルはすべて部分メニュー**
//!    という規則は [`MenuModel::top`] の型が構造的に保証し、空の位置は [`MenuPath::new`] が
//!    拒否するので、基盤に無視される単独のトップレベル項目は作れない（[`MenuPathError`]）。
//! 4. **登録時のショートカット検査**（4.6 との接続。要件 3.4）。[`MenuItemSpec::with_accelerator`]
//!    で受け取った綴りは登録時に [`AcceleratorRegistry`] へ通し、**競合は登録元へ返す**
//!    （[`MenuRegistrationError::Accelerator`]。片方を黙って捨てない）。綴りは
//!    **プラットフォーム解決済み**でなければならない（非 macOS は `Ctrl`、macOS は
//!    `Cmd` / `Super`。`CmdOrCtrl` は 4.6 が受理しないため、ここで明示的にエラーになる）。
//! 5. **組み込みの「終了」項目**。[`install`] が起動時に 1 回だけ、同じ登録口を通して登録する。
//!    選択時の処理は 5.4 の唯一の終了入口 [`crate::lifecycle::request_exit`] を呼ぶ
//!    （常駐の拒否を解除してから終了するので、どのプラットフォームでも確実にプロセスが終わる）。
//!
//! # 下流スペックの接続点（共有の継ぎ目）
//!
//! 各 UI スペック（9.5 の診断の導線、9.6 のドキュメント操作、以降のスペック）は、自分の
//! `setup` フックなどから
//! `app.state::<MenuRegistry>().register(app, MenuItemSpec::new(...))` を呼ぶ。**本モジュールは
//! 個々の項目を持たない** — 持つのは登録口・モデルの組み立て・配置先の吸収だけである。
//! 組み込み項目も「終了」の 1 つだけで、それも同じ登録口を通る。
//!
//! # 7.5 への申し送り
//!
//! - **キーの振り向け**（要件 3.5）と、**フォーカス移動のたびの有効・無効の更新**は 7.5 が
//!   所有する。本モジュールは [`MenuModel`] を公開しており、7.5 はそこから項目を引いて状態を
//!   変えたうえで [`apply`] を呼び直す。
//! - **ショートカットの表示**（`Ctrl+Shift+S` のようなプラットフォームの表記）も 7.5 が担う。
//!   本モジュールは 4.6 の正準形（`ctrl+shift+KeyS`）をそのまま基盤へ渡す — 基盤はそれを
//!   受理し、プラットフォームの表記で提示する。
//! - 組み込みの「終了」項目のショートカットは 7.5 が与える（5.4 の申し送り）。
//!
//! # 選択の通知に発生元が付かない理由
//!
//! Tauri 2.11.5 のメニューイベント（`MenuEvent`）は**項目の識別子しか運ばない**。ウィンドウ単位の
//! メニューでもアプリ全体のメニューでも同じであり、基盤から「どのウィンドウのメニューが押された
//! か」は得られない（research.md「メニューとキーボードショートカット」。ウィンドウ単位のイベント
//! リスナも全イベントに対して呼ばれる）。したがって [`on_menu_event`] は**現在フォーカスされて
//! いるウィンドウ**を手掛かりとして渡す。これは 7.5 が refine する暫定であり、判定そのものは
//! 登録元に委ねる（`None` もありうる）。

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use app_shell::accelerator::{
    Accelerator, AcceleratorConflict, AcceleratorOwner, AcceleratorParseError, AcceleratorRegistry,
    MenuItemId,
};
use app_shell::ipc::WindowLabel;
use tauri::menu::{
    IsMenuItem, Menu, MenuBuilder, MenuEvent, MenuItem, MenuItemBuilder, Submenu, SubmenuBuilder,
};
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};
use tauri_plugin_log::log;

// ---------------------------------------------------------------------------
// 配置先（プラットフォーム差が存在する唯一の場所）
// ---------------------------------------------------------------------------

/// メニューの配置先。**プラットフォーム差はこの型と [`apply`] の 1 箇所だけにある。**
///
/// どちらを選ぶかは [`PLACEMENT`] が実行中のプラットフォームから決める。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPlacement {
    /// **アプリ全体で 1 つのメニューしか持てない**（macOS）。ウィンドウ単位のメニュー設定は
    /// 基盤が非対応である（`Window::set_menu` の分岐が macOS では切られている）。
    /// トップレベルはすべて部分メニューでなければならず、**先頭の部分メニューは
    /// アプリケーションメニューへ畳み込まれる**。
    ApplicationWide,
    /// **ウィンドウ単位のメニューを持てる**（Windows / Linux）。生成された各ウィンドウに
    /// 同じ構成のメニューを付ける。後から作られるウィンドウは生成時に受け取る
    /// （[`attach_to_window`]）。
    PerWindow,
}

/// 実行中のプラットフォームの配置先。`cfg!` はコンパイル時に評価されるため、実行時の分岐は
/// 残らない（macOS 以外は常に [`MenuPlacement::PerWindow`]）。
pub const PLACEMENT: MenuPlacement = if cfg!(target_os = "macos") {
    MenuPlacement::ApplicationWide
} else {
    MenuPlacement::PerWindow
};

/// アプリケーションメニューの部分メニュー名（macOS）。`tauri.conf.json` の `productName` と
/// 一致させる。
///
/// **macOS では先頭の部分メニューがアプリケーションメニューへ畳み込まれる**ため、この名前が
/// そのまま見出しとして残るとは限らない（畳み込まれた後は OS がアプリ名を出す）。それでも
/// 名前を持つのは、モデル・記録・テストで部分メニューを特定できるようにするためである。
const APPLICATION_MENU_LABEL: &str = "jxcel";

/// ファイルメニューの部分メニュー名（macOS 以外）。
#[cfg_attr(target_os = "macos", allow(dead_code))] // macOS はアプリケーションメニューを使う。
const FILE_MENU_LABEL: &str = "ファイル";

/// トップレベルの部分メニューの慣習的な並び（要件 3.6）。
///
/// ここに無い名前の部分メニューは、この並びの後ろに**辞書順**で置かれる。並びを固定するのは、
/// 登録の順序（実体は登録元の識別子の辞書順）が部分メニューの見え方に漏れないようにするため
/// である。**macOS ではアプリケーションメニューが常に先頭**になる（[`top_level_sort_key`]）。
const TOP_LEVEL_ORDER: &[&str] = &["ファイル", "編集", "表示", "ヘルプ"];

/// 組み込みの終了項目の表示名。
const QUIT_LABEL: &str = "終了";

/// 組み込みの登録元の識別子（[`AcceleratorOwner`]）。
const BUILTIN_OWNER: &str = "app-shell";

/// 組み込みの終了項目の識別子（[`MenuItemId`]）。
///
/// **項目の識別子はアプリ全体で一意でなければならない** — 基盤のメニューイベントは識別子しか
/// 運ばないため、同じ識別子が 2 つあると選択を登録元へ振り分けられない。登録口がこれを検査する
/// （[`MenuRegistrationError::ItemIdConflict`]）。
const QUIT_ITEM_ID: &str = "app-shell.quit";

// ---------------------------------------------------------------------------
// 位置（部分メニューの並び）
// ---------------------------------------------------------------------------

/// メニュー項目の位置が不正である原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuPathError {
    /// 位置が空である。**トップレベルに単独の項目は置けない。**
    ///
    /// macOS はトップレベルをすべて部分メニューとして扱い、**部分メニューでない項目を無言で
    /// 無視する**（research.md）。空の位置を許すと「置いたつもりで現れない項目」ができるため、
    /// 登録の入口で拒否する（要件 3.1・3.2 の「指定された位置に表示」を構造的に守る）。
    BareTopLevelItem,
    /// 部分メニューの名前が空（または空白だけ）である。
    EmptySegment,
}

impl fmt::Display for MenuPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BareTopLevelItem => f.write_str(
                "メニュー項目の位置が空である。トップレベルには部分メニューしか置けず、\
                 単独の項目は macOS では無視される",
            ),
            Self::EmptySegment => f.write_str("部分メニューの名前が空である"),
        }
    }
}

impl std::error::Error for MenuPathError {}

/// メニュー項目の位置。**最初の要素がトップレベルの部分メニュー**であり、以降の要素は入れ子の
/// 部分メニューである。
///
/// **非空であることが型の前提条件であり、[`MenuPath::new`] だけが作る入口である。** これにより
/// 「トップレベルに単独の項目を置く」ことはできない（[`MenuPathError::BareTopLevelItem`]）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MenuPath {
    /// 部分メニューの名前（先頭がトップレベル）。
    segments: Vec<String>,
}

impl MenuPath {
    /// 部分メニューの並びから位置を作る。
    ///
    /// # Errors
    ///
    /// 並びが空のとき [`MenuPathError::BareTopLevelItem`]、名前が空のとき
    /// [`MenuPathError::EmptySegment`] を返す。
    pub fn new<I, S>(segments: I) -> Result<Self, MenuPathError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let segments: Vec<String> = segments.into_iter().map(Into::into).collect();
        if segments.is_empty() {
            return Err(MenuPathError::BareTopLevelItem);
        }
        if segments.iter().any(|segment| segment.trim().is_empty()) {
            return Err(MenuPathError::EmptySegment);
        }
        Ok(Self { segments })
    }

    /// 部分メニューの並び（先頭がトップレベル）。
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// トップレベルの部分メニューの名前。
    #[allow(dead_code)] // macOS の慣習の検査とテストが読む seam。
    pub fn top_level(&self) -> &str {
        // `new` が非空を保証するため、添字 0 は常に存在する。
        &self.segments[0]
    }
}

// ---------------------------------------------------------------------------
// 組み立て済みのモデル（純粋・GUI 不要）
// ---------------------------------------------------------------------------

/// メニュー木の 1 項目。登録の識別（登録元 + 項目）と表示に必要な値を持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItemNode {
    /// 登録元。
    owner: AcceleratorOwner,
    /// 項目の識別子。
    item: MenuItemId,
    /// 表示名。
    label: String,
    /// 割り当てられた組み合わせ（正準形。無ければ `None`）。
    accelerator: Option<Accelerator>,
}

impl MenuItemNode {
    /// 登録元。
    #[allow(dead_code)] // 7.5 の振り向けとテストが読む seam。
    pub fn owner(&self) -> &AcceleratorOwner {
        &self.owner
    }

    /// 項目の識別子。
    pub fn item(&self) -> &MenuItemId {
        &self.item
    }

    /// 表示名。
    pub fn label(&self) -> &str {
        &self.label
    }

    /// 割り当てられた組み合わせ（正準形）。
    pub fn accelerator(&self) -> Option<&Accelerator> {
        self.accelerator.as_ref()
    }
}

/// 部分メニュー。**トップレベルはこの型しか持てない**（[`MenuModel::top`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmenuNode {
    /// 部分メニューの名前。
    label: String,
    /// 子（項目と入れ子の部分メニュー）。項目が先、入れ子の部分メニューが後である。
    children: Vec<MenuNode>,
}

impl SubmenuNode {
    /// 部分メニューの名前。
    pub fn label(&self) -> &str {
        &self.label
    }

    /// 子（項目と入れ子の部分メニュー）。
    pub fn children(&self) -> &[MenuNode] {
        &self.children
    }
}

/// 部分メニューの子。**部分メニューか項目のどちらか**であり、トップレベルには項目が現れない
/// （[`MenuModel::top`] は [`SubmenuNode`] しか持たない）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuNode {
    /// 入れ子の部分メニュー。
    Submenu(SubmenuNode),
    /// 選択できる項目。
    Item(MenuItemNode),
}

/// 登録から組み立てたメニューのモデル。**GUI を起動せずに組み立て・検査できる。**
///
/// [`MenuModel::top`] が [`SubmenuNode`] の並びであることが、**「トップレベルはすべて部分
/// メニュー」という macOS の規則を型で保証する**。単独のトップレベル項目はこの型で表現できない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuModel {
    /// トップレベルの部分メニュー（慣習的な並び。macOS はアプリケーションメニューが先頭）。
    top: Vec<SubmenuNode>,
}

impl MenuModel {
    /// トップレベルの部分メニュー。
    pub fn top(&self) -> &[SubmenuNode] {
        &self.top
    }

    /// 木全体の項目を深さ優先で列挙する（登録の並び順）。
    #[allow(dead_code)] // 7.5 の有効・無効の更新とテストが読む seam。
    pub fn items(&self) -> Vec<&MenuItemNode> {
        fn collect<'a>(submenu: &'a SubmenuNode, out: &mut Vec<&'a MenuItemNode>) {
            for child in submenu.children() {
                match child {
                    MenuNode::Item(item) => out.push(item),
                    MenuNode::Submenu(nested) => collect(nested, out),
                }
            }
        }
        let mut out = Vec::new();
        for submenu in &self.top {
            collect(submenu, &mut out);
        }
        out
    }
}

/// 組み立ての途中段階（トップレベルの部分メニューと、その入れ子）。
#[derive(Debug, Default)]
struct PendingSubmenu {
    /// 入れ子の部分メニュー（名前順）。
    children: BTreeMap<String, PendingSubmenu>,
    /// この部分メニューに直接属する項目（登録の識別順）。
    items: Vec<MenuItemNode>,
}

impl PendingSubmenu {
    /// 部分メニューへ確定する。**項目が先、入れ子の部分メニューが後**である。
    fn assemble(self, label: String) -> SubmenuNode {
        let mut children: Vec<MenuNode> = self.items.into_iter().map(MenuNode::Item).collect();
        children.extend(
            self.children
                .into_iter()
                .map(|(child_label, pending)| MenuNode::Submenu(pending.assemble(child_label))),
        );
        SubmenuNode { label, children }
    }
}

/// 入れ子の位置まで降りて、その部分メニューの入れ物を返す。
fn slot<'a>(root: &'a mut PendingSubmenu, segments: &[String]) -> &'a mut PendingSubmenu {
    match segments.split_first() {
        Some((head, rest)) => slot(root.children.entry(head.clone()).or_default(), rest),
        None => root,
    }
}

/// トップレベルの部分メニューの並び順。
///
/// **macOS ではアプリケーションメニュー（[`APPLICATION_MENU_LABEL`]）を必ず先頭**にする
/// （先頭の部分メニューがアプリケーションメニューへ畳み込まれるため、ここが動くと畳み込み先が
/// 変わる）。そのうえで [`TOP_LEVEL_ORDER`] の既知の並びを先に、未知のものを名前順に置く。
fn top_level_sort_key(label: &str) -> (u8, u8, String) {
    let application_menu = if cfg!(target_os = "macos") && label == APPLICATION_MENU_LABEL {
        0
    } else {
        1
    };
    let known = TOP_LEVEL_ORDER
        .iter()
        .position(|known| *known == label)
        .map(|index| (index + 1) as u8)
        .unwrap_or(u8::MAX);
    (application_menu, known, label.to_owned())
}

// ---------------------------------------------------------------------------
// 登録（登録元の識別と選択の通知）
// ---------------------------------------------------------------------------

/// 選択されたメニュー項目。**登録元が処理を振り分けるのに足りる識別を運ぶ。**
#[allow(dead_code)] // 選択を受けた登録元（とテスト）が読む seam。7.5 の振り向けも使う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuSelection {
    /// 登録元（[`MenuItemSpec::new`] へ渡した識別子）。
    owner: AcceleratorOwner,
    /// 選択された項目の識別子。
    item: MenuItemId,
    /// 発生元の手掛かり（分かれば）。**基盤のイベントはウィンドウを運ばない**ため、現在
    /// フォーカスされているウィンドウである（module doc「選択の通知に発生元が付かない理由」）。
    window: Option<WindowLabel>,
}

#[allow(dead_code)] // 選択を受けた登録元（とテスト）が読む seam。7.5 の振り向けも使う。
impl MenuSelection {
    /// 登録元。
    pub fn owner(&self) -> &AcceleratorOwner {
        &self.owner
    }

    /// 選択された項目の識別子。
    pub fn item(&self) -> &MenuItemId {
        &self.item
    }

    /// 発生元の手掛かり（分からなければ `None`）。
    pub fn window(&self) -> Option<&WindowLabel> {
        self.window.as_ref()
    }
}

/// 選択を登録元へ通知する処理。登録時に [`MenuItemSpec::new`] へ渡す。
///
/// `AppHandle` を引数に取らないのは、**通知が「登録元が自分で用意した処理」への一方的な呼び出し
/// であり、基盤の状態を必要としない**ためである。基盤を操作する必要がある登録元（組み込みの
/// 終了など）は、登録時に `AppHandle` を複製して自分で捕捉する（[`install`] の `quit` 項目）。
/// これにより通知経路は GUI 無しでテストできる。
pub type MenuHandler = Arc<dyn Fn(&MenuSelection) + Send + Sync + 'static>;

/// 登録された項目（正規化済み）。
struct RegisteredItem {
    /// 位置。
    path: MenuPath,
    /// 表示名。
    label: String,
    /// 正規化済みの組み合わせ（無ければ `None`）。
    accelerator: Option<Accelerator>,
    /// 選択を受け取る処理。
    handler: MenuHandler,
}

/// 登録するメニュー項目。**登録元（[`AcceleratorOwner`]）と項目（[`MenuItemId`]）の組が
/// 登録の同一性**である。
pub struct MenuItemSpec {
    /// 登録元。
    owner: AcceleratorOwner,
    /// 項目の識別子。**アプリ全体で一意でなければならない。**
    item: MenuItemId,
    /// 位置（非空。先頭がトップレベルの部分メニュー）。
    path: MenuPath,
    /// 表示名。
    label: String,
    /// 割り当てる組み合わせの綴り（**プラットフォーム解決済み**。登録時に解釈する）。
    accelerator: Option<String>,
    /// 選択を受け取る処理。
    handler: MenuHandler,
}

impl MenuItemSpec {
    /// 項目を作る。
    ///
    /// `owner` は登録元の識別子（4.6 のショートカットの登録元と同じ名前空間を使うこと）、
    /// `item` は項目の識別子（**アプリ全体で一意**）、`path` は位置（[`MenuPath`]）、
    /// `label` は表示名、`handler` は選択時に呼ばれる処理である。
    pub fn new(
        owner: impl Into<AcceleratorOwner>,
        item: impl Into<MenuItemId>,
        path: MenuPath,
        label: impl Into<String>,
        handler: impl Fn(&MenuSelection) + Send + Sync + 'static,
    ) -> Self {
        Self {
            owner: owner.into(),
            item: item.into(),
            path,
            label: label.into(),
            accelerator: None,
            handler: Arc::new(handler),
        }
    }

    /// ショートカットを割り当てる。**綴りはプラットフォーム解決済みでなければならない**
    /// （4.6 の構文契約。非 macOS は `Ctrl`、macOS は `Cmd` / `Super`）。
    ///
    /// 解釈できない綴り（`CmdOrCtrl` など）と、ほかの登録と競合する組み合わせは、登録時に
    /// [`MenuRegistrationError`] として登録元へ返る。
    #[allow(dead_code)] // 7.5 とテストが使う seam（組み込みの項目はまだショートカットを持たない）。
    pub fn with_accelerator(mut self, spelling: impl Into<String>) -> Self {
        self.accelerator = Some(spelling.into());
        self
    }
}

/// 登録が受け付けられなかった原因。**いずれも登録元へ返り、無言で捨てられる経路は無い。**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuRegistrationError {
    /// **項目の識別子がほかの登録元に使われている。**識別子は基盤のイベントが運ぶ唯一の手掛かり
    /// なので、重複すると選択を振り分けられない。登録元ごとに一意な名前空間を使うこと。
    ItemIdConflict {
        /// 重複した項目の識別子。
        item: MenuItemId,
    },
    /// ショートカットの綴りを解釈できない（4.6 の構文契約）。**この項目は登録されない。**
    ///
    /// 基盤は綴りの誤りをエラーにせずその項目だけショートカットを失うため、ここで弾く。
    AcceleratorSyntax {
        /// 対象の項目。
        item: MenuItemId,
        /// 解釈できなかった原因。
        source: AcceleratorParseError,
    },
    /// ショートカットがほかの登録と競合した（要件 3.4）。**競合した両方の登録を運ぶ。**
    /// この項目は登録されず、既存の登録も変わらない。
    Accelerator(AcceleratorConflict),
    /// メニューを基盤へ配置できなかった。**登録そのものは成立している**（登録簿には入り、
    /// 次の再構築で再び配置を試みる）。
    Placement {
        /// 基盤が返した説明。
        message: String,
    },
}

impl fmt::Display for MenuRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ItemIdConflict { item } => write!(
                f,
                "メニュー項目の識別子 \"{item}\" はほかの登録元が既に使っている",
            ),
            Self::AcceleratorSyntax { item, source } => {
                write!(f, "メニュー項目 \"{item}\" のショートカットを解釈できない: {source}")
            }
            Self::Accelerator(conflict) => write!(f, "{conflict}"),
            Self::Placement { message } => {
                write!(f, "メニューを配置できなかった: {message}")
            }
        }
    }
}

impl std::error::Error for MenuRegistrationError {}

impl From<AcceleratorConflict> for MenuRegistrationError {
    fn from(conflict: AcceleratorConflict) -> Self {
        Self::Accelerator(conflict)
    }
}

/// メニューの登録口。**アプリ全体で 1 実体を管理状態として置く**（`Manager::manage`）。
///
/// 個別機能は [`register`](Self::register) で自分の項目を登録し、選択は
/// [`dispatch`](Self::dispatch)（本番では [`on_menu_event`] が呼ぶ）で登録時に渡した処理へ届く。
/// 登録簿は登録元の識別子と項目の識別子の組をキーにした [`BTreeMap`] なので、**列挙と並びが
/// 決定的**である（メニューを再構築しても描画順が変わらない）。
pub struct MenuRegistry {
    /// 登録簿（ロックで保護する）。
    inner: Mutex<MenuRegistryInner>,
}

/// 登録簿の内部状態。
#[derive(Default)]
struct MenuRegistryInner {
    /// 登録の同一性（登録元 + 項目）→ 登録。列挙の順序を与える。
    items: BTreeMap<(AcceleratorOwner, MenuItemId), RegisteredItem>,
    /// 組み合わせの一意性検査（4.6）。**このポートは同じ実体を共有する**ので、7.5 が同じ登録口を
    /// 通る限り競合はここで検出される。
    accelerators: AcceleratorRegistry,
}

impl Default for MenuRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuRegistry {
    /// 空の登録簿を作る。
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(MenuRegistryInner::default()),
        }
    }

    /// ロックを取る。**毒されていても panic しない** — 選択の通知はイベントループの中で走るため、
    /// ここで panic するとアプリ全体を巻き込む（`ports.rs` / `window` のレジストリと同じ判断）。
    /// 中身は panic で壊れる不変条件を持たない。
    fn lock(&self) -> MutexGuard<'_, MenuRegistryInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// **登録の入口（プラットフォーム非依存）。** 登録簿へ加えるだけで、メニューの組み立てと
    /// 配置は行わない。[`register`](Self::register) がこれを呼んでから配置する。
    ///
    /// テストはここを使う — メニューの配置には画面（イベントループとウィンドウ）が要るが、
    /// **登録簿の内容と選択の通知（[`dispatch`](Self::dispatch)）は GUI 無しで実測できる。**
    ///
    /// 同じ登録（同じ登録元 + 同じ項目）の再登録は、表示名・位置・組み合わせ・処理を更新して
    /// 成功する（メニューは再構築されるため）。**ショートカットの競合と項目識別子の重複は
    /// 状態を変えずに拒否される。**
    ///
    /// # Errors
    ///
    /// [`MenuRegistrationError::ItemIdConflict`]（項目識別子の重複）、
    /// [`MenuRegistrationError::AcceleratorSyntax`]（解釈できない綴り）、
    /// [`MenuRegistrationError::Accelerator`]（競合。要件 3.4）。
    pub fn enroll(&self, spec: MenuItemSpec) -> Result<(), MenuRegistrationError> {
        let MenuItemSpec {
            owner,
            item,
            path,
            label,
            accelerator,
            handler,
        } = spec;

        // 綴りの解釈はロックの外で行う（登録簿に触れない）。`CmdOrCtrl` は 4.6 が明示的に拒否する。
        let chord = match accelerator {
            Some(spelling) => Some(Accelerator::parse(&spelling).map_err(|source| {
                MenuRegistrationError::AcceleratorSyntax {
                    item: item.clone(),
                    source,
                }
            })?),
            None => None,
        };

        let mut inner = self.lock();

        // 項目の識別子はアプリ全体で一意でなければならない（ほかの登録元のものは拒否する）。
        if let Some(((other, _), _)) = inner.items.iter().find(|((_, id), _)| id == &item) {
            if other != &owner {
                return Err(MenuRegistrationError::ItemIdConflict { item });
            }
        }

        // 組み合わせの一意性検査（4.6）。競合したら**登録簿を変えずに**返す。
        // 組み合わせを外した再登録は古い組み合わせを明示的に解放する（使われない組み合わせを
        // 残すと、あとから別の登録元が同じ組み合わせを使えなくなる）。
        match &chord {
            Some(chord) => inner
                .accelerators
                .insert(owner.clone(), item.clone(), chord.clone())?,
            None => {
                let _released = inner.accelerators.remove(&owner, &item);
            }
        }

        inner.items.insert(
            (owner, item),
            RegisteredItem {
                path,
                label,
                accelerator: chord,
                handler,
            },
        );
        Ok(())
    }

    /// **登録の入口（登録してから配置する）。** 個別機能はこれを呼ぶ。
    ///
    /// [`enroll`](Self::enroll) のあと、現在の登録からメニューを組み立て直して配置する
    /// （macOS はアプリ全体、Windows / Linux は既存の全ウィンドウ）。
    ///
    /// # Errors
    ///
    /// 登録が拒否されたとき（[`enroll`](Self::enroll) のエラー）、または配置に失敗したとき
    /// （[`MenuRegistrationError::Placement`]）。**配置に失敗しても登録は残る** — 次の登録や
    /// 再構築で再び配置を試みる（同じ登録の再登録は冪等なので、呼び直しても安全である）。
    pub fn register(
        &self,
        app: &AppHandle,
        spec: MenuItemSpec,
    ) -> Result<(), MenuRegistrationError> {
        self.enroll(spec)?;
        let model = self.model();
        match apply(app, &model) {
            Ok(()) => Ok(()),
            Err(error) => {
                log::error!("メニューを配置できなかった: {error}");
                Err(MenuRegistrationError::Placement {
                    message: error.to_string(),
                })
            }
        }
    }

    /// 現在の登録からメニューのモデルを組み立てる。**GUI を起動せずに検査できる。**
    ///
    /// トップレベルの部分メニューの並びは [`top_level_sort_key`] が決める。同じ部分メニューの
    /// 下では、項目が登録の識別順（登録元 → 項目）で先、入れ子の部分メニューが名前順で後である。
    pub fn model(&self) -> MenuModel {
        let inner = self.lock();
        let mut roots: BTreeMap<String, PendingSubmenu> = BTreeMap::new();
        for ((owner, item), registered) in &inner.items {
            let segments = registered.path.segments();
            let root = roots.entry(segments[0].clone()).or_default();
            slot(root, &segments[1..])
                .items
                .push(MenuItemNode {
                    owner: owner.clone(),
                    item: item.clone(),
                    label: registered.label.clone(),
                    accelerator: registered.accelerator.clone(),
                });
        }
        let mut top: Vec<SubmenuNode> = roots
            .into_iter()
            .map(|(label, pending)| pending.assemble(label))
            .collect();
        top.sort_by_key(|node| top_level_sort_key(node.label()));
        MenuModel { top }
    }

    /// **選択の通知の唯一の経路（活性化の seam）。** 項目の識別子から登録を引き、登録元が
    /// 渡した処理へ [`MenuSelection`] を渡す。本番では [`on_menu_event`] がこれを呼ぶ。
    ///
    /// 戻り値は「通知先が見つかって呼んだ」かどうかである。未登録の識別子（基盤が古いメニューを
    /// 保持している、など）は記録に残して `false` を返す — **panic しない**（イベントループの
    /// 中で走るため）。
    ///
    /// **処理はロックの外で呼ぶ。** 処理が [`register`](Self::register) を呼び返しても
    /// デッドロックしない。
    pub fn dispatch(&self, item: &MenuItemId, window: Option<WindowLabel>) -> bool {
        let target = {
            let inner = self.lock();
            inner
                .items
                .iter()
                .find(|((_, id), _)| id == item)
                .map(|((owner, id), registered)| {
                    (owner.clone(), id.clone(), Arc::clone(&registered.handler))
                })
        };
        let Some((owner, item, handler)) = target else {
            log::warn!("未登録のメニュー項目が選択された: {item}");
            return false;
        };
        // 選択の事実を記録に残す（**どの登録元へ通知したか**が運用時に追える。GUI 実行での
        // 検証もこの行で観測できる）。
        log::info!(
            "メニュー項目が選択された: 登録元={owner} 項目={item} ウィンドウ={}",
            window
                .as_ref()
                .map(WindowLabel::as_str)
                .unwrap_or("(不明)"),
        );
        handler(&MenuSelection {
            owner,
            item,
            window,
        });
        true
    }
}

// ---------------------------------------------------------------------------
// 起動時の構築と配置（プラットフォーム差の吸収）
// ---------------------------------------------------------------------------

/// 起動時に 1 回だけメニューを構築して配置する。`lifecycle::run` が構築の後・`app.run` の前に
/// 呼ぶ。
///
/// ここが登録するのは**組み込みの「終了」項目だけ**であり、それも個別機能と同じ登録口
/// （[`MenuRegistry::register`]）を通す。選択時の処理は 5.4 の唯一の終了入口
/// [`crate::lifecycle::request_exit`] を呼ぶ — 常駐の拒否を解除してから `app.exit(0)` するので、
/// **どのプラットフォームでも確実にプロセスが終わる**（要件 2.9 の「明示的な終了操作を必ず
/// 用意する」）。ショートカットの割り当てと表示は 7.5 が与える。
///
/// この時点ではウィンドウが 1 枚も無い（起動時のウィンドウは `RunEvent::Ready` が作る）ため、
/// Windows / Linux の配置はこの呼び出しでは空振りする。起動時のウィンドウは生成時に
/// [`attach_to_window`] を通って受け取る。
pub fn install(app: &AppHandle) {
    let registry = app.state::<MenuRegistry>();
    let quit = {
        // 基盤を操作する必要があるので、登録時に `AppHandle` を捕捉する（[`MenuHandler`] が
        // 引数に取らない理由を参照）。
        let app = app.clone();
        MenuItemSpec::new(
            BUILTIN_OWNER,
            QUIT_ITEM_ID,
            builtin_quit_path(),
            QUIT_LABEL,
            move |_selection| crate::lifecycle::request_exit(&app),
        )
    };
    if let Err(error) = registry.register(app, quit) {
        log::error!("組み込みの終了メニュー項目を登録できなかった: {error}");
    }
}

/// 組み込みの終了項目を置く部分メニュー。**プラットフォームの慣習に合わせる。**
///
/// - macOS: 先頭の部分メニューはアプリケーションメニューへ畳み込まれるので、そこに置く
///   （終了はアプリケーションメニューにあるのが慣習である）。
/// - Windows / Linux: ファイルメニューに置く。
#[cfg(target_os = "macos")]
fn builtin_quit_path() -> MenuPath {
    MenuPath::new([APPLICATION_MENU_LABEL]).expect("組み込みの位置は空でない")
}

/// 組み込みの終了項目を置く部分メニュー（macOS 以外。ファイルメニュー）。
#[cfg(not(target_os = "macos"))]
fn builtin_quit_path() -> MenuPath {
    MenuPath::new([FILE_MENU_LABEL]).expect("組み込みの位置は空でない")
}

/// **生成されたウィンドウへメニューを付ける。** ウィンドウ生成の唯一の場所
/// （`crate::window::build_window`）から呼ぶ。
///
/// ウィンドウ単位のメニューを持てるプラットフォーム（Windows / Linux）では、生成のたびにこれを
/// 通るので、**後から作られるウィンドウ（起動時・二重起動の引き継ぎ・Dock クリック）にもメニュー
/// が付く**。メニューを 1 つしか持てないプラットフォーム（macOS）では何もしない — アプリ全体の
/// メニューは [`install`] が設定済みであり、ウィンドウ単位の設定は基盤が非対応である。
///
/// 失敗は記録に残して**生成を妨げない**（メニューの欠落でウィンドウが開かなくなる方が悪い）。
pub fn attach_to_window<R: Runtime>(window: &WebviewWindow<R>) {
    if PLACEMENT != MenuPlacement::PerWindow {
        return;
    }
    let app = window.app_handle();
    let model = app.state::<MenuRegistry>().model();
    let result = build_native_menu(app, &model).and_then(|menu| window.set_menu(menu).map(|_| ()));
    match result {
        Ok(()) => log::info!(
            "ウィンドウへメニューを付けた: label={} トップレベル={} 項目={}",
            window.label(),
            model.top().len(),
            model.items().len(),
        ),
        Err(error) => log::error!(
            "ウィンドウへメニューを付けられなかった: label={} / {error}",
            window.label(),
        ),
    }
}

/// **メニュー選択の実際の入口。**`Builder::on_menu_event` へ結線する（1 本だけ）。
///
/// 判断は [`MenuRegistry::dispatch`] に委ねる。ここが行うのは「基盤のイベントを登録の識別子へ
/// 写す」ことと「発生元の手掛かりを集める」ことだけである（後者の限界は module doc を参照）。
/// ウィンドウ単位のメニューでもアプリ全体のメニューでも、基盤は選択をこのハンドラへ届ける
/// （tauri 2.11.5 のグローバルなイベントリスナはすべてのメニューイベントに対して呼ばれる）。
pub fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let item = MenuItemId::new(event.id().0.as_str());
    // 基盤のメニューイベントはウィンドウを運ばないので、現在フォーカスされているウィンドウを
    // 手掛かりにする（module doc「選択の通知に発生元が付かない理由」）。
    let window = app
        .webview_windows()
        .values()
        .find(|window| window.is_focused().unwrap_or(false))
        .map(|window| WindowLabel::new(window.label()));
    app.state::<MenuRegistry>().dispatch(&item, window);
}

/// モデルを基盤のメニューへ組み立てる。**トップレベルはすべて部分メニューである**
/// （[`MenuModel::top`] の型が保証する）。
fn build_native_menu<R: Runtime>(app: &AppHandle<R>, model: &MenuModel) -> tauri::Result<Menu<R>> {
    let mut submenus: Vec<Submenu<R>> = Vec::with_capacity(model.top().len());
    for submenu in model.top() {
        submenus.push(build_native_submenu(app, submenu)?);
    }
    let items: Vec<&dyn IsMenuItem<R>> = submenus
        .iter()
        .map(|submenu| submenu as &dyn IsMenuItem<R>)
        .collect();
    MenuBuilder::new(app).items(&items).build()
}

/// 部分メニュー 1 つを組み立てる（入れ子も再帰で扱う）。
fn build_native_submenu<R: Runtime>(
    app: &AppHandle<R>,
    node: &SubmenuNode,
) -> tauri::Result<Submenu<R>> {
    let mut children: Vec<Box<dyn IsMenuItem<R>>> = Vec::with_capacity(node.children().len());
    for child in node.children() {
        match child {
            MenuNode::Item(item) => children.push(Box::new(build_native_item(app, item)?)),
            MenuNode::Submenu(nested) => {
                children.push(Box::new(build_native_submenu(app, nested)?));
            }
        }
    }
    let items: Vec<&dyn IsMenuItem<R>> = children.iter().map(|child| &**child).collect();
    SubmenuBuilder::new(app, node.label()).items(&items).build()
}

/// 項目 1 つを組み立てる。**割り当てられた組み合わせは 4.6 の正準形をそのまま渡す** —
/// 基盤はそれを受理し、プラットフォームの表記で提示する。
fn build_native_item<R: Runtime>(
    app: &AppHandle<R>,
    item: &MenuItemNode,
) -> tauri::Result<MenuItem<R>> {
    let mut builder = MenuItemBuilder::new(item.label()).id(item.item().as_str());
    if let Some(chord) = item.accelerator() {
        builder = builder.accelerator(chord.as_str());
    }
    builder.build(app)
}

/// モデルを**プラットフォームの配置先へ配置する。ここが唯一の分岐である。**
///
/// - [`MenuPlacement::ApplicationWide`]（macOS）: `AppHandle::set_menu` でアプリ全体のメニューを
///   1 つだけ設定する。**先頭の部分メニューはアプリケーションメニューへ畳み込まれる**
///   （research.md「メニューとキーボードショートカット」）ので、並びは
///   [`top_level_sort_key`] が固定している。
/// - [`MenuPlacement::PerWindow`]（Windows / Linux）: 現在の各ウィンドウへ `Window::set_menu`
///   で付ける。**ここで見えているのは既存のウィンドウだけ**であり、後から作られるウィンドウは
///   生成時に [`attach_to_window`] が受ける。
fn apply<R: Runtime>(app: &AppHandle<R>, model: &MenuModel) -> tauri::Result<()> {
    let menu = build_native_menu(app, model)?;
    match PLACEMENT {
        MenuPlacement::ApplicationWide => {
            let _previous = app.set_menu(menu)?;
        }
        MenuPlacement::PerWindow => {
            for window in app.webview_windows().values() {
                let _previous = window.set_menu(menu.clone())?;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// テスト（タスク 7.4）
//
// GUI を必要としない部分（登録の受理・拒否、モデルの組み立て、選択の通知）を固定する。
// **ネットワークもイベントループも要らない** — 通知は登録元が渡した処理を直接呼ぶためである。
// 実際のメニューの描画（GTK のメニューバー、macOS のアプリケーションメニュー）と、
// 基盤のイベントから [`on_menu_event`] が呼ばれることは、ホスト側の GUI 実行でしか検証できない。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;

    /// 選択を記録するだけの登録元。**本番の経路をそのまま通す**（テスト専用の通知経路を持たない）。
    #[derive(Clone, Default)]
    struct Recorder {
        seen: Arc<StdMutex<Vec<MenuSelection>>>,
    }

    impl Recorder {
        /// [`MenuItemSpec::new`] へ渡す処理。
        fn handler(&self) -> impl Fn(&MenuSelection) + Send + Sync + 'static {
            let seen = Arc::clone(&self.seen);
            move |selection: &MenuSelection| {
                seen.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(selection.clone());
            }
        }

        /// これまでに届いた選択。
        fn seen(&self) -> Vec<MenuSelection> {
            self.seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    /// 位置を作る（テストの前提が崩れたらそこで落ちる）。
    fn path(segments: &[&str]) -> MenuPath {
        MenuPath::new(segments.iter().copied()).expect("テストの位置は空でない")
    }

    #[test]
    fn a_registered_item_appears_at_the_requested_position() {
        let registry = MenuRegistry::new();
        let recorder = Recorder::default();

        registry
            .enroll(
                MenuItemSpec::new("document", "save", path(&["ファイル"]), "保存", recorder.handler())
                    .with_accelerator("Ctrl+S"),
            )
            .expect("登録できる");
        registry
            .enroll(MenuItemSpec::new(
                "document",
                "save-as",
                path(&["ファイル"]),
                "名前を付けて保存",
                recorder.handler(),
            ))
            .expect("登録できる");
        registry
            .enroll(MenuItemSpec::new(
                "diagnostics",
                "show-log",
                path(&["ヘルプ", "診断"]),
                "記録の場所",
                recorder.handler(),
            ))
            .expect("登録できる");

        let model = registry.model();
        // 並びは慣習的な順（`ファイル` → `ヘルプ`）であり、登録順にも識別子の辞書順にも依存しない。
        let top: Vec<&str> = model.top().iter().map(SubmenuNode::label).collect();
        assert_eq!(top, ["ファイル", "ヘルプ"]);

        let items = model.items();
        let labels: Vec<&str> = items.iter().map(|item| item.label()).collect();
        assert_eq!(labels, ["保存", "名前を付けて保存", "記録の場所"]);
        // **ショートカットは 4.6 の正準形で保持される**（表示形ではない。表示は 7.5）。
        assert_eq!(
            items[0].accelerator().map(Accelerator::as_str),
            Some("ctrl+KeyS")
        );
        assert_eq!(items[1].accelerator(), None);

        // 入れ子の部分メニューはトップレベルではなく `ヘルプ` の下にある。
        let help = &model.top()[1];
        assert_eq!(help.children().len(), 1);
        match &help.children()[0] {
            MenuNode::Submenu(nested) => {
                assert_eq!(nested.label(), "診断");
                match &nested.children()[0] {
                    MenuNode::Item(item) => assert_eq!(item.item().as_str(), "show-log"),
                    MenuNode::Submenu(_) => panic!("項目が期待される位置に部分メニューがある"),
                }
            }
            MenuNode::Item(_) => panic!("部分メニューが期待される位置に項目がある"),
        }
    }

    #[test]
    fn selecting_a_registered_item_notifies_the_registrant() {
        let registry = MenuRegistry::new();
        let recorder = Recorder::default();
        registry
            .enroll(MenuItemSpec::new(
                "document",
                "save",
                path(&["ファイル"]),
                "保存",
                recorder.handler(),
            ))
            .expect("登録できる");

        // 本番の `on_menu_event` が呼ぶのと同じ経路（`dispatch`）を通す。
        assert!(registry.dispatch(&MenuItemId::new("save"), Some(WindowLabel::new("doc-1"))));

        let seen = recorder.seen();
        assert_eq!(seen.len(), 1);
        // **登録元が処理を振り分けられるだけの識別が届く**（登録元・項目・発生元）。
        assert_eq!(seen[0].owner().as_str(), "document");
        assert_eq!(seen[0].item().as_str(), "save");
        assert_eq!(
            seen[0].window().map(WindowLabel::as_str),
            Some("doc-1"),
            "発生元の手掛かりが届く（無い場合は None）"
        );
    }

    #[test]
    fn selecting_an_unregistered_item_notifies_nobody() {
        let registry = MenuRegistry::new();
        let recorder = Recorder::default();
        registry
            .enroll(MenuItemSpec::new(
                "document",
                "save",
                path(&["ファイル"]),
                "保存",
                recorder.handler(),
            ))
            .expect("登録できる");

        assert!(!registry.dispatch(&MenuItemId::new("存在しない"), None));
        assert!(recorder.seen().is_empty(), "未登録の項目では誰にも通知しない");
    }

    #[test]
    fn a_duplicate_shortcut_is_reported_to_the_registrant_at_registration() {
        let registry = MenuRegistry::new();
        let recorder = Recorder::default();
        registry
            .enroll(
                MenuItemSpec::new("document", "save", path(&["ファイル"]), "保存", recorder.handler())
                    .with_accelerator("Ctrl+S"),
            )
            .expect("先の登録は成功する");

        // **別の登録元が同じ組み合わせを要求すると、登録の時点で競合が返る**（要件 3.4）。
        // 同じ組み合わせの別綴りでも同じ正準形へ畳まれるので競合する。
        let conflict = registry
            .enroll(
                MenuItemSpec::new(
                    "macro",
                    "save-macro",
                    path(&["ファイル"]),
                    "マクロを保存",
                    recorder.handler(),
                )
                .with_accelerator("control+KeyS"),
            )
            .expect_err("競合として返る");

        match &conflict {
            MenuRegistrationError::Accelerator(conflict) => {
                // どちらとどちらが衝突したかが分かる（片方を黙って捨てない）。
                assert_eq!(conflict.existing.owner.as_str(), "document");
                assert_eq!(conflict.existing.item.as_str(), "save");
                assert_eq!(conflict.incoming.owner.as_str(), "macro");
                assert_eq!(conflict.incoming.item.as_str(), "save-macro");
                assert_eq!(conflict.chord.as_str(), "ctrl+KeyS");
            }
            other => panic!("競合が期待される: {other}"),
        }

        // **拒否された項目はメニューに現れない**（現れてショートカットだけ失うことはない）。
        let model = registry.model();
        let labels: Vec<&str> = model.items().iter().map(|item| item.label()).collect();
        assert_eq!(labels, ["保存"]);
    }

    #[test]
    fn re_registering_the_same_item_with_the_same_shortcut_is_idempotent() {
        let registry = MenuRegistry::new();
        let recorder = Recorder::default();
        let spec = || {
            MenuItemSpec::new("document", "save", path(&["ファイル"]), "保存", recorder.handler())
                .with_accelerator("Ctrl+S")
        };
        registry.enroll(spec()).expect("1 回目は成功する");
        // メニューは再構築されるので、同じ登録が何度も来る（4.6 の冪等性）。
        registry.enroll(spec()).expect("同じ登録の再登録も成功する");
        registry.enroll(spec()).expect("3 回目も成功する");

        let model = registry.model();
        assert_eq!(model.items().len(), 1, "同じ項目が増えない");
        assert_eq!(registry.lock().accelerators.len(), 1, "組み合わせも 1 つだけ");
    }

    #[test]
    fn a_re_registration_with_a_new_shortcut_releases_the_old_one() {
        let registry = MenuRegistry::new();
        let recorder = Recorder::default();
        registry
            .enroll(
                MenuItemSpec::new("document", "save", path(&["ファイル"]), "保存", recorder.handler())
                    .with_accelerator("Ctrl+S"),
            )
            .expect("登録できる");
        registry
            .enroll(
                MenuItemSpec::new("document", "save", path(&["ファイル"]), "保存", recorder.handler())
                    .with_accelerator("Ctrl+O"),
            )
            .expect("同じ項目の組み合わせの更新として成功する");

        // 解放された組み合わせは別の登録元が使える（使われない組み合わせを残さない）。
        registry
            .enroll(
                MenuItemSpec::new("macro", "open", path(&["ファイル"]), "開く", recorder.handler())
                    .with_accelerator("Ctrl+S"),
            )
            .expect("解放済みの組み合わせは競合しない");
        assert_eq!(registry.lock().accelerators.len(), 2);
    }

    #[test]
    fn a_shortcut_that_is_not_platform_resolved_is_rejected() {
        let registry = MenuRegistry::new();
        let recorder = Recorder::default();
        // `CmdOrCtrl` はプラットフォーム依存なので 4.6 が受理しない。**この項目は登録されない。**
        let error = registry
            .enroll(
                MenuItemSpec::new("document", "save", path(&["ファイル"]), "保存", recorder.handler())
                    .with_accelerator("CmdOrCtrl+S"),
            )
            .expect_err("解釈できない綴りは登録時に拒否する");
        match error {
            MenuRegistrationError::AcceleratorSyntax { item, source } => {
                assert_eq!(item.as_str(), "save");
                // **プラットフォーム依存の擬似修飾は明示的に拒否される**（4.6 の構文契約）。
                assert!(
                    matches!(
                        source,
                        AcceleratorParseError::PlatformDependentModifier { .. }
                    ),
                    "プラットフォーム依存の修飾キーとして拒否される: {source:?}"
                );
            }
            other => panic!("綴りの誤りが期待される: {other}"),
        }
        assert!(registry.model().items().is_empty(), "登録されていない");
    }

    #[test]
    fn a_bare_top_level_item_is_rejected() {
        // **トップレベルに単独の項目は置けない**（macOS は部分メニュー以外を無言で無視する）。
        // 空の位置は登録の入口（型の構築）で拒否される。
        assert_eq!(
            MenuPath::new(Vec::<String>::new()),
            Err(MenuPathError::BareTopLevelItem)
        );
        assert_eq!(MenuPath::new([""]), Err(MenuPathError::EmptySegment));
        assert_eq!(MenuPath::new(["ファイル", " "]), Err(MenuPathError::EmptySegment));
    }

    #[test]
    fn every_top_level_entry_of_the_model_is_a_submenu() {
        let registry = MenuRegistry::new();
        let recorder = Recorder::default();
        for (owner, item, segments) in [
            ("document", "save", vec!["ファイル"]),
            ("edit", "undo", vec!["編集"]),
            ("help", "about", vec!["ヘルプ"]),
            ("unknown", "custom", vec!["独自"]),
        ] {
            registry
                .enroll(MenuItemSpec::new(
                    owner,
                    item,
                    path(&segments),
                    item,
                    recorder.handler(),
                ))
                .expect("登録できる");
        }

        let model = registry.model();
        assert!(!model.top().is_empty());
        // 型（`Vec<SubmenuNode>`）がこれを保証するが、期待する並びも固定しておく
        // （macOS では先頭の部分メニューがアプリケーションメニューへ畳み込まれるため、
        // 並びは畳み込み先を決める。既知の並びが先、未知のものが後）。
        let top: Vec<&str> = model.top().iter().map(SubmenuNode::label).collect();
        assert_eq!(top, ["ファイル", "編集", "ヘルプ", "独自"]);
        // 項目はすべて部分メニューの下にあり、トップレベルには現れない。
        for item in model.items() {
            assert!(model.top().iter().any(|submenu| {
                submenu.children().iter().any(|child| matches!(
                    child,
                    MenuNode::Item(node) if node.item() == item.item()
                ))
            }));
        }
    }

    #[test]
    fn a_duplicate_item_identifier_from_another_owner_is_rejected() {
        let registry = MenuRegistry::new();
        let recorder = Recorder::default();
        registry
            .enroll(MenuItemSpec::new(
                "document",
                "save",
                path(&["ファイル"]),
                "保存",
                recorder.handler(),
            ))
            .expect("登録できる");

        // 基盤のイベントは項目の識別子しか運ばない。**別の登録元が同じ識別子を取ると選択を
        // 振り分けられないので拒否する。**
        let error = registry
            .enroll(MenuItemSpec::new(
                "macro",
                "save",
                path(&["ファイル"]),
                "マクロを保存",
                recorder.handler(),
            ))
            .expect_err("識別子の重複は拒否する");
        assert_eq!(
            error,
            MenuRegistrationError::ItemIdConflict {
                item: MenuItemId::new("save")
            }
        );

        // 同じ登録元の同じ項目（再登録）は重複ではない。
        registry
            .enroll(MenuItemSpec::new(
                "document",
                "save",
                path(&["ファイル"]),
                "保存",
                recorder.handler(),
            ))
            .expect("同じ登録の再登録は成功する");
    }

    #[test]
    fn the_builtin_quit_item_is_placed_in_the_platform_conventional_submenu() {
        let quit = builtin_quit_path();
        #[cfg(target_os = "macos")]
        assert_eq!(quit.top_level(), APPLICATION_MENU_LABEL);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(quit.top_level(), FILE_MENU_LABEL);
        assert!(quit.segments().len() >= 1);
    }
}

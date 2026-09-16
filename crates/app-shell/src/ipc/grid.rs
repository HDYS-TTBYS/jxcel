//! グリッドの境界の型（タスク 6.1。要件 1.1、1.5、1.6、3.2）。
//!
//! `design.md`「GridCommands」の Risks が定める境界の型を、そのまま置く場所である。
//! **文字列と 32 ビット以下の整数と真偽だけで構成し、他のドメインクレートの型を参照しない。**
//! 境界の型は `crates/app-shell/src/ipc/` の下にだけ置き、`ts-rs` の derive を付けてよいのは
//! 本モジュールを含む `crate::ipc` の内側だけである（`design.md`「IpcContract」の不変条件）。
//! ドメインの型を境界へそのまま出せない理由は `design.md`「Data Contracts & Integration」の
//! 表にある — `Row` は `Serialize` を持たず、`Violation` も持たず、`CellValue` は 64 ビット
//! 整数（`Int`）と識別子を出す。したがって**写しをここに定義し、写すのは `src-tauri` の
//! 適応層（6.2 / 6.3）の仕事とする**。
//!
//! とくに守る規約は 5 つある。
//!
//! 1. **`i64` / `u64` を境界へ出さない。** 列の添字・行数・件数・要素数は [`u32`] で運ぶ。
//!    JavaScript の `number` は IEEE 754 の倍精度であり、32 ビット以下の整数は正確に表せるが、
//!    64 ビット整数はそうではない（[`super::WindowLabel`] の doc を参照）。行の識別子は
//!    文字列で運ぶ。
//! 2. **値は打たれた文字として運ぶ。** セルの値を型付きの値として境界へ出さない。
//!    6 つの編集命令のうち値を運ぶのは `SetCells`（打たれた文字）・`SetNested`（構造表現の
//!    JSON）・`PasteRange`（表形式テキスト）だけで、いずれも文字列である。窓の二進形式も
//!    「数値としての値を一切含まない」（`design.md`「窓の二進形式」）ため、境界に値の列挙は
//!    要らない。**`document-format` の `CellValue` を写した型はここに置かない。**
//! 3. **型の種別の札は 1 つだけである。** [`TypeKindTag`] が唯一の札であり、描画側と入力手段の
//!    登録簿は双方ともこの生成された札を取り込む（7.1 / 7.4）。
//! 4. **列の情報は `view` 層の `LayoutColumn` を過不足なく写す。** 内側の位置は平坦な段の並び
//!    として運び、フィールド名と配列の位置を区別する（要件 4.5）。
//! 5. **空の 2 つの状態を形の上で区別できるようにする。** 列が 1 本も無いこと（要件 1.6）と、
//!    列はあるが行が無いこと（要件 1.5）は別であり、区別するのは**列の数**である
//!    （[`GridSheetSummary`]）。
//!
//! タスク 6.2 が**封筒つきの 5 つのコマンドの要求と応答**を足した（[`GridOpenRequest`] /
//! [`GridOpenResponse`] / [`GridViewRequest`] / [`GridViewResponse`] / [`GridEditRequest`] /
//! [`GridEditResponse`] / [`GridHistoryRequest`] / [`GridHistoryDirection`] /
//! [`GridViolationRequest`] / [`GridViolationResponse`] / [`GridSearchDirection`]）。
//! これらは**荷として本モジュールの型を使うだけ**であり、載せるのは呼び出し元ウィンドウの
//! 文脈（[`super::WindowContext`]）と、上の表の荷である。要求の型はどれも
//! **ウィンドウを運ばない** — 呼び出し元は基盤が注入する `WebviewWindow` から取り、ペイロード
//! で受け取らない（偽装できない。要件 4.6、`ipc-contract.md`）。
//!
//! **本モジュールが持たないもの**: 生バイト経路（`grid_rows_window`）の引数の型である。
//! あれは二進の窓を要求するための型であり、封筒を運べない経路のものである（`design.md`
//! 「WindowCodec」。タスク 6.3）。

use serde::{Deserialize, Serialize};

use super::WindowContext;

/// セルの型の種別を表す札（タスク 6.1。要件 3.1、3.2、3.8、10.1〜10.4）。
///
/// **境界を越える唯一の型の札であり、フロントエンドはこれを取り込む。** 描画側
/// （`design.md`「RendererPort」の `RenderCell.variant`）と入力手段の登録簿（同
/// 「EditorRegistry」の `CellEditorRegistration.kind`）は、双方ともこの生成された札を参照し、
/// **独自に札を定義しない** — 写しを 2 つ持つと、片方だけが増えたときに気づけない
/// （`design.md`「EditorRegistry」の Risks）。
///
/// 綴りは設計の合併型（同節の `TypeKindTag`）そのままであり、**小文字へ落とさない**。
/// `schema-engine` の `TypeKind` の変種名と 1 対 1 に対応していることが、`src-tauri`
/// （唯一 `schema-engine` と `app-shell` の双方を見られるクレート）で対応を検査できる前提で
/// あるため、`#[serde(rename_all = ...)]` を付けない。
///
/// [`TypeKindTag::ALL`] が閉じた集合の唯一の源であり、並びは `TypeKind::ALL` と同じ順である。
/// **その一致の検査は `src-tauri` に置く** — 本クレートは他のドメインクレートに依存しては
/// ならない（`crates/app-shell/Cargo.toml` の依存方針）ため、ここから `TypeKind` を参照して
/// 数え合わせることはできない。6.2 / 6.3 の適応層が `TypeKindTag::ALL` と `TypeKind::ALL` を
/// 突き合わせ、**片方だけに変種が増えたときに落ちる検査**を置く。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ts_rs::TS,
)]
pub enum TypeKindTag {
    /// 64 ビット整数。
    Int,
    /// 倍精度小数。
    Float,
    /// 10 進数。
    Decimal,
    /// 文字列。
    Text,
    /// 真偽。
    Bool,
    /// 日付。
    Date,
    /// 日時。
    DateTime,
    /// 列挙（選択肢を持つ型。要件 3.2）。
    Enum,
    /// シート間参照（要件 3.8）。
    Ref,
    /// 添付参照。
    Attachment,
    /// 名前つきフィールドの集合。
    Object,
    /// 同一型の並び。
    Array,
    /// 任意の値。
    Any,
    /// 拡張型（変種の範囲は実装が決める。10.6 の登録簿が扱う）。
    Custom,
}

impl TypeKindTag {
    /// 設計表に挙がるすべての種別。**閉じた集合の唯一の源である。**
    ///
    /// 並びは `schema-engine` の `TypeKind::ALL` と同じ（14 種）。すべての種別がちょうど 1 回
    /// 現れることと、生成物の合併型が同じ 14 個を同じ綴りで並べることは
    /// `crate::ipc` の検査（`tests::type_kind_tag_matches_the_design_union`）が固定する
    /// （取りこぼしを実行時に検出するため）。
    pub const ALL: [TypeKindTag; 14] = [
        TypeKindTag::Int,
        TypeKindTag::Float,
        TypeKindTag::Decimal,
        TypeKindTag::Text,
        TypeKindTag::Bool,
        TypeKindTag::Date,
        TypeKindTag::DateTime,
        TypeKindTag::Enum,
        TypeKindTag::Ref,
        TypeKindTag::Attachment,
        TypeKindTag::Object,
        TypeKindTag::Array,
        TypeKindTag::Any,
        TypeKindTag::Custom,
    ];
}

/// 入れ子の内側の位置の 1 段（タスク 6.1。要件 4.5、5.5）。
///
/// `types` 層の `NestedPathSegment` を写したもので、**フィールド名と配列の位置を区別する**。
/// 区別を潰すと、要件 4.5 が求める「入れ子のどの位置が違反しているか」を表示するときに
/// `a.b` と `a[1]` を書き分けられない。
///
/// 段の並び（[`ColumnDescriptor::path`] / [`GridViolationLocation::path`]）は、**空なら
/// セル直下**を指す（ドメインの `NestedPath` と同じ規約）。配列の位置は `u32` である
/// （`usize` を境界へ出さない）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "segment")]
pub enum GridPathSegment {
    /// オブジェクトのフィールド名。
    Field {
        /// フィールド名。
        name: String,
    },
    /// 配列の 0 起点の要素位置。
    Index {
        /// 要素の位置。
        position: u32,
    },
}

/// 列を展開できるか、詳細の表示へ委ねるかの札（タスク 6.1。要件 5.1、5.2、5.4）。
///
/// `view` 層の `Expandability` を写した**閉じた種類の列挙**である（小文字へ落とすのは
/// [`super::DocumentOrigin`] と同じ扱いである）。**3 値であることが要点であり、2 つの真偽へ
/// 潰さない** — 「内側を持たない」と「段数の上限に達した」は別の事実であり、潰すと展開の指定が
/// 上限の手前で止まっている列にも「詳細の表示へ」が出る（ドメインの `Expandability` の docs）。
///
/// 消費側が要る 2 つの事実（展開できるか・詳細の表示へ委ねるか）は
/// [`ColumnDescriptor::is_expandable`] と [`ColumnDescriptor::requires_detail`] がこの札から
/// 導く — ドメインの `LayoutColumn` の同名のメソッドと同じ判断であり、写しを二重に持たない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum ColumnExpandability {
    /// 内側を持ち、段数の上限に達していない（展開できる）。
    Available,
    /// 内側を持つが、段数の上限に達している（**詳細の表示へ委ねる**。要件 5.4）。
    Capped,
    /// 内側を持たない（展開の対象ではない）。
    Leaf,
}

/// 同一の型の並び（配列）の要素数の能力（タスク 6.1。要件 5.6）。
///
/// `view` 層の `ElementCount` を写したもので、要素の型の札と、**列に宣言された**要素数の
/// 上下限を持つ。**`None` は開いた端点**であり、宣言が無いことと上下限が 0 であることは違う
/// （ドメインの `ElementCount` の docs。`0..=0` は空の並びであり、宣言の無い並びではない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct ColumnElementCount {
    /// 要素の型の札（配列の `items` の種別）。
    pub items: TypeKindTag,
    /// 要素数の下限（`minItems`）。未宣言は `None`（開いた下限）。
    pub min: Option<u32>,
    /// 要素数の上限（`maxItems`）。未宣言は `None`（開いた上限）。
    pub max: Option<u32>,
}

/// 構成の 1 列: 窓が運ぶ列そのものであり、どの値が載るかを指す（タスク 6.1。要件 1.1、1.2、
/// 3.1、5.1、5.4、5.6）。
///
/// `view` 層の `LayoutColumn` を写したものであり、6.1 の消費側（描画側と入力手段の登録簿）が
/// 必要とするものを全部運ぶ — 列の添字・内側の位置・表示名・葉の型の札・要素数の能力・
/// 展開の可否である。`design.md`「GridSession」の Implementation Notes が「6.1 はここから
/// 境界型へ写す」と定めているのはこの型である。
///
/// **列の同一性は（[`ColumnDescriptor::column`], [`ColumnDescriptor::path`]）の対であり、
/// 名前ではない** — 表示名は人が読むためのものであり、送る先を決めるのは位置である
/// （ドメインの `LayoutColumn` の docs）。折りたたんだ列は位置が空であり、展開された列は
/// 位置が 1 段以上である。
///
/// `kind` が `None` であるのは、その列が**使用できない**（宣言が壊れている）場合である。
/// このとき入力手段の登録簿は既定の入力へ落ちる（10.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct ColumnDescriptor {
    /// 最上位の列の添字（0 起点。内側の位置も同じ最上位の列を指す）。
    pub column: u32,
    /// 内側の位置（空 = セル直下）。
    pub path: Vec<GridPathSegment>,
    /// 表示名（位置に沿ったフィールド名を `.` で連結したもの）。
    pub name: String,
    /// 葉の型の札（3.2 と 7.4 が入力手段を選ぶのに使う）。使用不能な列は `None`。
    pub kind: Option<TypeKindTag>,
    /// 同一の型の並びの要素数の能力（要件 5.6）。配列でなければ `None`。
    pub element_count: Option<ColumnElementCount>,
    /// 展開の可否と、上限に達したことの印（要件 5.4）。
    pub expandability: ColumnExpandability,
}

impl ColumnDescriptor {
    /// この列を展開できるか（内側を持ち、段数の上限に達していない。要件 5.1）。
    ///
    /// ドメインの `LayoutColumn::is_expandable` と同じ判断である。
    #[inline]
    #[must_use]
    pub const fn is_expandable(&self) -> bool {
        matches!(self.expandability, ColumnExpandability::Available)
    }

    /// この列が**詳細の表示へ委ねられている**か（要件 5.4 の印）。
    ///
    /// 画面はこの印を見て「詳細の表示へ誘導する」を出す。上限に達していない入れ子は段数を
    /// 増やせば降りられるため、ここでは真にならない（ドメインの
    /// `LayoutColumn::requires_detail` と同じ判断）。
    #[inline]
    #[must_use]
    pub const fn requires_detail(&self) -> bool {
        matches!(self.expandability, ColumnExpandability::Capped)
    }
}

/// シートの要約: 窓が運ぶ列の構成と、シートの行数（タスク 6.1。要件 1.1、1.5、1.6）。
///
/// **これは封筒ではない。** `status` も呼び出し元ウィンドウの文脈も持たない、応答の内側の荷で
/// ある（`design.md`「GridCommands」の `grid_open_sheet` の応答は 6.2 が組み立て、その荷として
/// 本型を使う）。
///
/// # 2 つの空の状態（要件 1.5、1.6）
///
/// **列の数が 2 つを区別する。** 行数は両者を区別しない（列が 1 本も宣言されていないシートも、
/// 列はあるが行が 1 件も無いシートも、行数は 0 件でありうる）ため、**列の並びと行数を同じ型に
/// 載せる**ことが要件である。
///
/// | 状態 | 形 | 画面の振る舞い |
/// |---|---|---|
/// | 列が 1 本も宣言されていない（要件 1.6） | `columns` が空 | 表を描かず、スキーマが定義されていないことを示す |
/// | 列はあるが行が 1 件も無い（要件 1.5） | `columns` が非空かつ `row_count == 0` | 列の構成を提示したうえで、行が無いことを示す |
/// | 通常 | `columns` が非空かつ `row_count > 0` | 表を描く |
///
/// 2 つの状態の判定は [`GridSheetSummary::has_no_columns`] /
/// [`GridSheetSummary::has_columns_but_no_rows`] が行う。**画面が自前で書かない**のは、
/// 「列の数で区別する」という規則を 1 箇所に閉じるためである。
///
/// # 行数は「シートの行数」であり、可視行数ではない
///
/// 絞り込みで可視の行が 0 件になった状態（要件 8.7）を要件 1.5 と混同してはならない —
/// 前者は**行が在って隠れている**のであり、後者は**行が無い**。可視行数と隠された行数は表示の
/// 指定の結果であり、6.2 の `GridViewResponse` が別に運ぶ（本型は表示の指定を知らない）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridSheetSummary {
    /// 窓が運ぶ列の構成（左から右への表示順。入れ子の展開を含む）。
    pub columns: Vec<ColumnDescriptor>,
    /// シートの行数（絞り込みの結果ではない。要件 1.5）。
    pub row_count: u32,
}

impl GridSheetSummary {
    /// 列が 1 本も宣言されていないか（要件 1.6）。真なら表を描かず、スキーマが定義されて
    /// いないことを示す。
    ///
    /// **行数の 0 と混同しない。** 行が無いだけなら列の構成は提示できる（要件 1.5）。
    #[inline]
    #[must_use]
    pub fn has_no_columns(&self) -> bool {
        self.columns.is_empty()
    }

    /// 列は宣言されているが、行が 1 件も無いか（要件 1.5）。列の構成は提示できる。
    ///
    /// [`GridSheetSummary::has_no_columns`] が真のときは偽である（列が無ければ表を描かない）。
    #[inline]
    #[must_use]
    pub fn has_columns_but_no_rows(&self) -> bool {
        !self.columns.is_empty() && self.row_count == 0
    }
}

/// 並べ替えの基準列 1 本（タスク 6.1。要件 8.3）。
///
/// `view` 層の `SortKey` を写したものである。列の添字は `Row::values()` に対する位置であり、
/// シートの列名の並びと同じ添字である。`descending` は**その基準列の比較だけ**を反転する
/// （同値の行の決着は反転しない。ドメインの `SortKey` の docs）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridSortKey {
    /// 基準となる列の添字（0 起点。要件 8.3）。
    pub column: u32,
    /// この基準列を降順で並べるか。
    pub descending: bool,
}

/// 絞り込みの条件 1 本（タスク 6.1。要件 8.4）。
///
/// `view` 層の `FilterSpec` を写したもので、**設計が固定する 5 条件**（一致・部分一致・値なし・
/// 値あり・違反あり）を過不足なく持つ。複数与えられた場合は**積**として働く
/// （[`GridViewSpec::filters`]）。
///
/// **`HasViolation` は画面が要求できる。** 「違反あり」で絞り込む導線（要件 8.4）はこの条件
/// だけで成立し、列を問わない指定（`column: None`）と列を指定した要求を**別の要求として
/// 区別する**。
///
/// `Equals` / `Contains` が比較するのは**値ではなく表示文字列**である（ドメインの
/// `FilterSpec` の docs）。境界は値を型付きで運ばないため、比較の対象は文字列で足りる。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "filter")]
pub enum GridFilterSpec {
    /// 列の表示文字列が `text` と完全に一致する行を選ぶ。
    Equals {
        /// 対象の列の添字（0 起点）。
        column: u32,
        /// 一致させる表示文字列（そのまま比較する）。
        text: String,
    },
    /// 列の表示文字列が `text` を含む行を選ぶ。
    Contains {
        /// 対象の列の添字（0 起点）。
        column: u32,
        /// 含まれることを求める表示文字列（空文字は全行に一致する）。
        text: String,
    },
    /// 列の表示文字列が空である行を選ぶ（値なし）。
    IsEmpty {
        /// 対象の列の添字（0 起点）。
        column: u32,
    },
    /// 列の表示文字列が空でない行を選ぶ（値あり）。
    IsNotEmpty {
        /// 対象の列の添字（0 起点）。
        column: u32,
    },
    /// 違反を持つ行を選ぶ。
    HasViolation {
        /// 対象の列。`None` は**列を問わない**（その行に違反が 1 つでもあれば選ぶ）。
        column: Option<u32>,
    },
}

/// 入れ子の展開の状態 1 列ぶん（タスク 6.1。要件 5.1、5.2、5.3、5.4）。
///
/// `view` 層の `ExpansionState` を写したものである。展開は**表示状態の一部**であり
/// （要件 5.3）、走査や並べ替えでは失われない。`depth` は表示する段数であり、上限
/// （`design.md`「表示状態」の `MAX_EXPANSION_DEPTH`）に達した列は
/// [`ColumnExpandability::Capped`] として現れる（要件 5.4）。
///
/// 段数は `u8` である — ドメインと同じ幅にしておき、上限を越える指定が境界で復元に失敗する
/// ようにする（`u32` に広げると、通ってから拒否する経路ができる）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridExpansionState {
    /// 対象の列の添字（0 起点）。
    pub column: u32,
    /// 展開しているか（偽は折りたたみの指定である。要件 5.2）。
    pub expanded: bool,
    /// 表示する入れ子の段数（要件 5.4）。
    pub depth: u8,
}

/// 表示の指定: 行の並びと列の構成をどう導出するか（タスク 6.1。要件 5.3、8.3、8.4）。
///
/// **並べ替え・絞り込み・展開を 1 つの形にまとめる。** `view` 層では表示状態が
/// `ViewSpec`（並べ替えと絞り込み）と `ExpansionState` の並び（展開）に割れているが、
/// 境界では 1 つにする — 3 つとも**窓が運ぶ行と列を変える**ものであり（`design.md`
/// 「表示状態」の割り方の根拠）、要求の口は `grid_set_view` 1 つだからである。
/// 列幅と表示上の列順は**ここに無い** — あれらは窓の中身を変えず、境界を越える理由が無い
/// （画面側の `DisplayState`。要件 8.1、8.2）。
///
/// 空の指定は「絞り込み無し・並べ替え無し・展開無し」であり、文書の行順と宣言の列が
/// そのまま現れる（ドメインの `ViewSpec` / `RowOrder::recompute` の規約）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridViewSpec {
    /// 並べ替えの基準列。先頭が第一の基準であり、同値のときだけ次の基準が効く（要件 8.3）。
    pub sort: Vec<GridSortKey>,
    /// 絞り込みの条件。**すべてに一致する行だけが可視になる（積）**（要件 8.4）。
    pub filters: Vec<GridFilterSpec>,
    /// 入れ子の展開の状態（要件 5.1〜5.4）。列ごとに 1 件である。
    pub expansion: Vec<GridExpansionState>,
}

/// 物理のセルの位置（タスク 6.1。要件 3.3、8.6、8.9）。
///
/// `types` 層の `CellAddress` を写したもので、**行の識別子（文字列）と列の添字（`u32`）**を
/// 持つ。**可視行の序数ではない** — 絞り込みや並べ替えの下では表示の位置と一致せず、取り違えると
/// 別の行を編集する（要件 8.6）。表示の座標からこの位置への写像を持つのは、順序を持つ側
/// （`src-tauri` の適応層と画面）である。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridCellAddress {
    /// 行の識別子（文字列表現。64 ビット整数を境界へ出さない）。
    pub row: String,
    /// 列の添字（0 起点）。
    pub column: u32,
}

/// 1 つのセルへ書く、打たれた文字（タスク 6.1。要件 3.3、3.5、7.3）。
///
/// `edit` 層の `EditCommand::SetCells` は `Vec<(CellAddress, String)>` を持つが、境界では
/// **名前のある欄に分ける** — 位置と文字の 2 つ組は、生成物（`src/ipc/bindings.ts`）で
/// `[GridCellAddress, string]` という無名の並びになり、読み手に意味を伝えないためである。
///
/// `text` は**打たれた文字そのもの**であり、型の解釈は `schema-engine` が行う（要件 3.3）。
/// 適合しない値も破棄せずに保持し、違反として報告する（要件 3.5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridCellEdit {
    /// 書くセル。
    pub cell: GridCellAddress,
    /// そこへ打たれた文字。
    pub text: String,
}

/// 編集命令（タスク 6.1。要件 3.3、5.7、6.1、6.2、6.3、7.3、7.4、8.9）。
///
/// `edit` 層の `EditCommand` を写したもので、**6 つの命令を過不足なく持つ**。命令の意味は
/// ドメインの同名の変種と同じであり、本型は解釈を持たない（適用するのは `data-grid`）。
///
/// **値を型付きで運ばない。** 値を運ぶ 3 つの命令は、いずれも文字列を運ぶ —
/// `SetCells` は打たれた文字、`SetNested` はセル値の**構造表現（JSON）**、`PasteRange` は
/// **表形式テキスト**である（要件 7.2。`document-format` の `CellValue` を写した型は境界に
/// 無い）。**行の構造を変える 3 つの命令は値を運ばない** — 挿入する行の値は宣言が供給し、
/// 複製する行の値はドメインが写す（要件 6.1、6.3）。
///
/// 挿入の位置（`InsertRows::at`）は**文書の行順に対する位置**であり、可視の序数ではない
/// （画面の位置に挿入したい呼び出し側は、順序を持つ側で行そのものへ写してからその行の位置を
/// 渡す）。貼り付けは起点と**表示されている行の並び**の 2 つで宛先が決まる（要件 8.9）ため、
/// `PasteRange::rows` を持つ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "command")]
pub enum GridEditCommand {
    /// 指定したセルへ打たれた文字を書く。
    SetCells {
        /// 書くセルと、そこへ打たれた文字。同じセルが複数回現れた場合は**後ろのものが残る**
        /// （ドメインの `Document::set_cells` の契約）。
        cells: Vec<GridCellEdit>,
    },
    /// 入れ子のセルへ、構造を保った表現（JSON）を書く（要件 5.5、5.7）。
    SetNested {
        /// 書くセル（物理の位置）。
        cell: GridCellAddress,
        /// そのセルへ書く値の構造表現。
        json: String,
    },
    /// 指定した**文書の位置**へ、宣言の既定値を持つ行を `count` 行足す（要件 6.1）。
    InsertRows {
        /// 挿入する文書の位置（適用前の行順に対する添字。行数までの値が妥当）。
        at: u32,
        /// 挿入する行数。`0` は何も変えない。
        count: u32,
    },
    /// 選択された複数の行を 1 回の操作として取り除く（要件 6.2）。
    RemoveRows {
        /// 取り除く行の識別子。空なら何も変えない。
        rows: Vec<String>,
    },
    /// 選択された行と同じ値を持つ行を末尾へ足す（要件 6.3、6.4）。
    DuplicateRows {
        /// 複製する元の行の識別子。空なら何も変えない。
        rows: Vec<String>,
    },
    /// 表形式テキストを、錨のセルから始まる矩形として貼り付ける（要件 7.3、7.4、7.5、8.9）。
    PasteRange {
        /// 貼り付けの起点（**物理の行**と列。要件 8.6）。
        anchor: GridCellAddress,
        /// **表示されている行の並び**（順序を持つ側が導出したもの。要件 8.9）。
        ///
        /// 貼り付けは錨の行がこの並びに現れる位置から歩くため、**隠れている行には 1 セルも
        /// 書かれない**。空なら何も書かない。
        rows: Vec<String>,
        /// 貼り付ける表形式テキスト（行の区切りと列の区切りを持つ。要件 7.2）。
        text: String,
    },
}

/// 型強制によって値が変換されたことの記録（タスク 6.1。要件 3.4）。
///
/// `edit` 層の `CoercionNotice` を写したもので、変換の**前と後**の双方を表示文字列として
/// 持つ。「変換が起きたこと」と「変換前の値」を人が確認できる形にするためである（要件 3.4）。
/// 表示文字列の写しはドメインが 1 つだけ持ち、本型はその写しを運ぶ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridCoercionNotice {
    /// 変換が起きたセルの位置。
    pub cell: GridCellAddress,
    /// 変換**前**の値の表示文字列（打たれた文字そのもの）。
    pub before: String,
    /// 変換**後**の値の表示文字列（ドキュメントへ書かれた値）。
    pub after: String,
}

/// 違反の位置（タスク 6.1。要件 4.2、4.5、6.4、7.5）。
///
/// `schema-engine` の `Violation` と `data-grid` の `CellViolations` / `NestedPath` が持つ
/// **位置だけ**を写したものである — 行の識別子（文字列）・列の添字（`u32`）・入れ子の内側の
/// 位置である。**理由（`ViolationReason`）は本型に無い** — 違反の理由を提示する経路
/// （要件 4.2）は 6.2 が [`GridViolation`] として定め、文言を組み立てるのは適応層である。
///
/// **行を持たない違反がある。** ドメインの `Violation::row` は `Option<RowId>` であり、
/// 列そのものの問題は行を持たない。したがって `row` は `Option<String>` である
/// （セルに属する違反はつねに行を持つ）。
///
/// 内側の位置が空であることは**セル直下**の違反を意味する（要件 4.5 の位置の表現。
/// ドメインの `NestedPath` と同じ規約）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridViolationLocation {
    /// 違反の属する行の識別子。`None` は列そのものの問題である。
    pub row: Option<String>,
    /// 違反している列の添字（0 起点）。
    pub column: u32,
    /// 違反している内側の位置（空 = セル直下。要件 4.5）。
    pub path: Vec<GridPathSegment>,
}

/// 編集を適用した結果の要約（タスク 6.1。要件 3.4、4.3、4.6、6.2、6.4、7.5）。
///
/// `edit` 層の `EditOutcome` を写したものであり、**判定が返したものと、画面が直ちに要るもの**
/// だけを運ぶ。運ぶ欄は次のとおりである。
///
/// - `affected` — 影響を受けた行（重複を畳み、命令に現れた順）。画面はこの行の窓を捨てる
///   （要件 1.7）ために使う。
/// - `coercions` — 型強制の記録（要件 3.4）。変換が起きなければ空である。
/// - `violation_total` — **シート全体**の違反の総数（`u32`。`usize` を境界へ出さない）。
///   適応層がドメインの `GridSession::violation_total()`（差分的に最新へ保たれる）から写す。
///   **ドメインの `EditOutcome::violation_total`（再検証した列に閉じる）とは別物である** —
///   適応層が写し替えるのはそのためである（要件 4.3）。
/// - `violations` — 適用のあとに再検証した列が持つ違反（重複なし、報告の順）。
///   **範囲は `revalidated_columns` と同じであり、`violation_total` とは別である**（総数は
///   シート全体、この一覧は再検証した列に閉じる）。画面はこれで**変わった違反だけ**を
///   受け取れる（要件 4.6）。
/// - `revalidated_columns` — 適用のあとに再検証した列の添字（昇順・重複なし）。空の命令では
///   空であり、そのとき違反も 0 件である。
/// - `row_count` — 適用の**後**のシートの行数。行を足す・取り除く命令がこれを変える
///   （要件 6.2 が提示する行数の変化）。
///
/// 本型は解釈を持たない — 適用したのは `data-grid` であり、判定をしたのは `schema-engine`
/// である。境界はそれらの結果を写すだけである。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridEditOutcome {
    /// 影響を受けた行の識別子（重複を畳み、命令に現れた順）。
    pub affected: Vec<String>,
    /// 型強制によって値が変換されたセル。変換が起きなければ空である。
    pub coercions: Vec<GridCoercionNotice>,
    /// **シート全体**の違反の総数（要件 4.3）。
    ///
    /// **再検証した列に閉じない。**適応層が `GridSession::violation_total()`（差分的に最新へ
    /// 保たれるシート全体の数）から写す。**再検証した列に閉じるのは [`Self::violations`] の
    /// 一覧のほうである** — こちらは適用のあとに列の検証をやり直した結果であり、総数ではない。
    /// この欄の**文言**は、総数を列に閉じるとしていた頃のドメインの `EditOutcome` の doc を
    /// 引き写したものである（この境界型自体は 6.1 が書いており、その版の doc は
    /// `再検証した列に閉じた総数` と述べていた）。同種の記述は 8.3 の作業中に画面の module の
    /// **注記**にも現れていたため、どちらも正した（8.3 のレビューが指摘）。
    pub violation_total: u32,
    /// 適用のあとに再検証した列が持つ違反の一覧（重複なし、報告の順）。
    pub violations: Vec<GridViolationLocation>,
    /// 適用のあとに再検証した列の添字（昇順・重複なし）。
    pub revalidated_columns: Vec<u32>,
    /// 適用の後のシートの行数。
    pub row_count: u32,
}

// ---------------------------------------------------------------------------
// 封筒（タスク 6.2。要件 3.3、4.4、8.3、8.4、9.2、9.3）
//
// 5 つのコマンドの要求と応答である。**荷は上の型をそのまま使う** — ここが載せるのは
// 呼び出し元ウィンドウの文脈と、上の表の結果だけである。要求の型はどれもウィンドウを
// 運ばない（呼び出し元は基盤が注入する `WebviewWindow` から取る。要件 4.6）。
//
// **ドメインの失敗は封筒の成功腕に載る。** 見つからなかった違反（[`GridViolationResponse`]）
// と、進める履歴が無かったこと（[`GridEditResponse`]）は**正常な結果**であり、
// `design.md`「Error Handling」の表の「利用者の入力」の行である。封筒の失敗腕へ落ちるのは
// 操作の誤り（`data-grid` の `GridError`。範囲外のセル・解釈できない入れ子の表現）と、
// 経路そのものが成立しない場合（そのウィンドウにドキュメントが無い・シートが引けない・
// グリッドがまだ開かれていない）だけである。
// ---------------------------------------------------------------------------

/// 表示するシートを開く要求（タスク 6.2。要件 1.1、1.5、1.6）。
///
/// **シートは識別子の文字列で選ぶ。** 1 つのドキュメントは複数のシートを持ちうるが
/// （要件 1.7 の [`super::DocumentSheet`]）、表示する対象を選ぶ手段は本機能の外にあり
/// （`design.md`「Out of Boundary」）、境界を越える識別子は文字列である（64 ビット整数を
/// 出さない規約）。呼び出し側は [`super::DocumentStateResponse`] が運ぶシートの一覧の
/// `id` をそのまま渡す。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridOpenRequest {
    /// 表示するシートの識別子（[`super::DocumentSheet::id`] の文字列そのもの）。
    pub sheet: String,
}

/// シートを開いた応答（タスク 6.2。要件 1.1、1.5、1.6、4.6）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。`sheet` は窓が運ぶ列の構成と
/// シートの行数であり、**2 つの空の状態（列が 1 本も無い・列はあるが行が無い）を形の上で
/// 区別する**（[`GridSheetSummary`] の doc を参照）。
///
/// **表示の指定はここに無い。** 絞り込み・並べ替え・展開は [`GridViewResponse`] が運ぶ。
///
/// **世代（`generation`）は「この応答を組み立てた時点の世代」である**（タスク 10.1）。源は
/// `GridSession::generation()` ただ 1 つであり、画面はこれを**採用するだけ**である（数え直さない）。
/// 境界の規約（文字列と 32 ビット以下の整数）に従い、**10 進の文字列**で運ぶ — u64 を数値として
/// 出すと、生成物のフロントエンド（TS の数は 2^53 まで）で上位のバイトが消える
/// （`document-format` の窓の世代と同じ規約）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridOpenResponse {
    /// 呼び出し元ウィンドウの文脈。
    pub context: WindowContext,
    /// **この応答を組み立てた時点の世代**（10 進の文字列。タスク 10.1）。
    ///
    /// 開いた直後は [`Generation::FIRST`] の 10 進表現（`"0"`）である。以後の窓の要求は
    /// **つねにこの値**（または [`GridViewResponse`] / [`GridEditResponse`] が運ぶより新しい値）
    /// を名乗らなければならない — 一致しない世代には `transport` が空の窓を返す。
    pub generation: String,
    /// 開いたシートの要約（列の構成と行数）。
    pub sheet: GridSheetSummary,
}

/// 表示の指定を変える要求（タスク 6.2。要件 8.3、8.4）。
///
/// **指定は完全な記述である。** 空の [`GridViewSpec`] は「絞り込み無し・並べ替え無し・
/// 展開無し」を意味する（6.1 の doc と同じ規約）ので、前の指定のうちここに現れないものは
/// 適用されない。ウィンドウは要求の型に現れない（呼び出し元は基盤が注入する。
/// 要件 4.6）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridViewRequest {
    /// 適用する表示の指定。
    pub view: GridViewSpec,
}

/// 表示の指定を変えた応答（タスク 6.2。要件 8.5、8.7、4.3、4.6）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。可視行数と隠された行数を運ぶのは
/// 要件 8.7（絞り込みで表示されていない行の数を提示する）である。**要件 1.5 の「行が無い」
/// とは別である** — あちらは行そのものが無い状態であり、[`GridOpenResponse::sheet`] の
/// `row_count` が表す。ここが運ぶのは**行が在って隠れている**数である。
///
/// `violation_total` は**シート全体の違反の総数**である（要件 4.3。絞り込みに依らない）。
///
/// # `columns` は**導出後**の列の構成である（要件 5.1、5.2、5.4）
///
/// **構成を導出するのは本コマンドそのもの**（`GridSession::set_view` と展開の適用）であるため、
/// その結果は同じコマンドの応答に載る — 別のコマンドの応答に載せると、**どの指定に対する構成
/// なのかが要求と応答の対応から読めなくなる**。とくに `grid_open_sheet` の応答では足りない:
/// 開く時点で展開の指定はまだ存在せず（画面は開いたあとに指定を送る）、開き直しても展開後の
/// 構成は得られない。
///
/// 並びは [`ColumnDescriptor`] の並びであり（[`GridSheetSummary::columns`] と同じ規約）、
/// **左から右への表示順**である。入れ子を展開した列の内側の位置は、**親と同じ最上位の列を指す
/// 別の記述**として並ぶ（要件 5.1）ので、この並びの位置（＝表示の位置）と
/// [`ColumnDescriptor::column`]（＝文書の列）は一致しない — **表示の位置から文書の列への写像を
/// 組むのは、この並びを受け取った側の仕事である**（要件 8.6。折りたたむと内側の記述は消え、
/// 元の 1 本だけが残る — 要件 5.2）。
///
/// **`GridSheetSummary` をそのまま載せない**のは、あれが持つ `row_count` が**シートの行数**で
/// あり表示の指定では変わらないためである（行の側の数は `visible_rows` / `hidden_rows` が運ぶ）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridViewResponse {
    /// 呼び出し元ウィンドウの文脈。
    pub context: WindowContext,
    /// **この応答を組み立てた時点の世代**（10 進の文字列。タスク 10.1）。
    ///
    /// **画面はこれを採用し、数え直さない。** 本コマンドは 1 つの呼び出しの内側で世代を
    /// **複数回**進める — 要求に現れない展開の折りたたみ（適応層の手順 1）と、要求された展開の
    /// 適用（同 3）がそれぞれ 1 回ずつ進め、順序の導出（同 2。`set_view`）も進める。したがって
    /// 「成功ごとに +1」という数え方ではこの値に追いつけない。ずれると、以後の窓の要求が空の窓を
    /// 受け取り（`WindowCodec::is_stale`）、取り直したセルは**永久に読み込み中**のままになる
    /// （design.md「世代を進めるのは境界である」）。
    pub generation: String,
    /// 表示の指定を適用したあとの可視行数（要件 8.7）。
    pub visible_rows: u32,
    /// 絞り込みによって表示されていない行数（要件 8.7）。
    pub hidden_rows: u32,
    /// 表示中のシートに存在する違反の総数（要件 4.3）。
    pub violation_total: u32,
    /// **導出後**の列の構成（左から右への表示順。入れ子の展開を含む。要件 5.1、5.2、5.4）。
    ///
    /// [`GridSheetSummary::columns`] と同じ形・同じ意味である。指定を適用したあとの
    /// `GridSession::columns()` を写したものであり、**画面はこれで描く列を差し替える**
    /// （据え置くと、展開を指定しても描かれる列が変わらない）。
    pub columns: Vec<ColumnDescriptor>,
}

/// 編集を適用する要求（タスク 6.2。要件 3.3、5.7、6.1、7.3）。
///
/// 運ぶのは編集命令 1 つである（[`GridEditCommand`] の 6 つの命令）。**値を型付きで運ばない**
/// 規約は 6.1 の型が既に守っている。ウィンドウは要求の型に現れない（要件 4.6）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridEditRequest {
    /// 適用する編集命令。
    pub command: GridEditCommand,
}

/// 編集の結果の要約を運ぶ応答（タスク 6.2。要件 3.4、4.6、9.2、9.3）。
///
/// **適用（[`GridEditRequest`]）と履歴（[`GridHistoryRequest`]）が同じ形を返す。**
/// 履歴を進めることも「1 つの命令がドキュメントへ適用された」ことであり、画面が要るもの
/// （影響範囲・変換・違反・行数）は同じだからである（`design.md`「GridCommands」の API
/// Contract が `grid_apply_edit` と `grid_history` の応答を同じ型と定めている）。
///
/// **`outcome` が `None` であるのは「進める履歴が無かった」場合だけである**（要件 9.2、9.3）。
/// 取り消し・やり直しの対象が空のときに何も変えずに答える正常な結果であり、封筒の失敗腕には
/// 載せない — 「直前の操作が無い」ことは利用者の操作が失敗したことではない。適用
/// （[`GridEditRequest`]）ではつねに `Some` である。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridEditResponse {
    /// 呼び出し元ウィンドウの文脈。
    pub context: WindowContext,
    /// **この応答を組み立てた時点の世代**（10 進の文字列。タスク 10.1）。
    ///
    /// 適用（[`GridEditRequest`]）と履歴（[`GridHistoryRequest`]）のどちらも、**影響を受けた行が
    /// あるときだけ**世代を進める（`GridSession::apply` / `undo` / `redo`）。`outcome` が `None`
    /// である腕は何も適用していないので、この値も据え置きである。画面はこれを採用するだけであり、
    /// `outcome.affected` の空・非空から進み方を推し量る規則を持たない（規則が 2 つあると、
    /// 片方だけが正しいまま残る）。
    pub generation: String,
    /// 適用された操作の要約。`None` は「進める履歴が無く、何も変わらなかった」（要件 9.2、9.3）。
    pub outcome: Option<GridEditOutcome>,
}

/// 履歴を進める向き（タスク 6.2。要件 9.2、9.3）。**閉じた列挙である。**
///
/// 「取り消し」と「やり直し」は利用者の別々の指示であり、1 つの真偽へ潰さない
/// （潰すと生成物のフロントエンドで意味が読めない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum GridHistoryDirection {
    /// 直前の操作が行われる前の状態を復元する（要件 9.2）。
    Undo,
    /// 取り消した操作を再び適用する（要件 9.3）。
    Redo,
}

/// 履歴を進める要求（タスク 6.2。要件 9.2、9.3）。
///
/// **どちらへ進めるかを要求が言う。** 取り消しとやり直しは同じ経路（履歴と適用を束ねた口）
/// を通るが、進める向きは要求が決める。ウィンドウは要求の型に現れない（要件 4.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridHistoryRequest {
    /// 進める向き。
    pub direction: GridHistoryDirection,
}

/// 違反を探す向き（タスク 6.2。要件 4.4）。**閉じた列挙である。**
///
/// **可視行の序数が増える向き**が [`GridSearchDirection::Forward`] である（`data-grid` の
/// `SearchDirection` と同じ意味）。名前を `prev` / `next` にしないのは、向きが可視の順序に
/// 対して定義されており、画面の「前へ」が文書の順序と一致しないためである。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum GridSearchDirection {
    /// 可視行の序数が増える向き（指定した位置より後ろの行へ）。
    Forward,
    /// 可視行の序数が減る向き（指定した位置より前の行へ）。
    Backward,
}

/// 次の違反を探す要求（タスク 6.2。要件 4.4）。
///
/// **起点は可視行の序数である**（行そのものではない）。可視の序数から行への写像を持つのは
/// 順序を持つ側（`data-grid` の `RowOrder`）であり、写しは要求を通さない。ウィンドウは要求の
/// 型に現れない（要件 4.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridViolationRequest {
    /// 探索の起点（**可視行の 0 起点の序数**）。
    pub from: u32,
    /// 探索の向き。
    pub direction: GridSearchDirection,
}

/// 見つかった違反（タスク 6.2。要件 4.2、4.5）。
///
/// 位置（[`GridViolationLocation`]。入れ子の内側の位置を含む）と、**利用者へ伝えるための
/// 理由の文言**を対で運ぶ。理由はドメインの側の語（`schema-engine` の `ViolationReason`）を
/// そのまま出さず、適応層が組み立てた文言を載せる（理由の写像を持たない境界の型に
/// 当たる。`design.md`「GridCommands」の Implementation Notes）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridViolation {
    /// 違反の位置（要件 4.5 の入れ子の内側の位置を含む）。
    pub location: GridViolationLocation,
    /// 違反の理由を利用者へ伝える文言（要件 4.2）。
    pub reason: String,
}

/// 次の違反を探した結果（タスク 6.2。要件 4.2、4.4、4.5、4.6）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む**（要件 4.6）。`violation` が `None` であるのは
/// 「その向きにこれ以上違反が無い」場合であり、**正常な結果である**（封筒の失敗腕には
/// 載せない）。画面は「見つからなかった」を日付の変更ではなく、これ以上無いこととして扱う。
///
/// 位置と理由の両方を 1 つの型（[`GridViolation`]）にまとめるのは、**見つかった違反にだけ
/// 両方が存在する**ためである（`location` と `reason` を別々の [`Option`] にすると、
/// 「位置はあるが理由が無い」という状態が型の上で表現できてしまう）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct GridViolationResponse {
    /// 呼び出し元ウィンドウの文脈。
    pub context: WindowContext,
    /// 見つかった違反（位置と理由）。見つからなければ `None`。
    pub violation: Option<GridViolation>,
}

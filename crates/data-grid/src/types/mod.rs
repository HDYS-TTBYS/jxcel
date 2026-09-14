//! 座標の型: セルの位置、可視行の序数、行の区間、セルの範囲、入れ子の内側の位置。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本層は鎖の最も左であり、
//! 本クレートの他のどの層にも依存しない**（design.md「内部の依存の向き」。層の鎖の文言を
//! 各層の冒頭に置く規約は `structure.md`「ドメインクレートの内部構造」）。
//!
//! # 2 つの空間を混ぜない
//!
//! 座標は**2 つの空間**に分かれる。この分離が本モジュールの主題である。
//!
//! | 空間 | 型 | 何を指すか |
//! |---|---|---|
//! | **表示**（可視行の序数） | [`RowOrdinal`] / [`RowSpan`] / [`CellPosition`] / [`CellRange`] | 絞り込みと並べ替えを適用した**あとの**並びの何番目か |
//! | **文書**（物理の位置） | [`CellAddress`] | [`RowId`] が指す行そのもの。列は [`ColumnIndex`] |
//!
//! 並べ替えと絞り込みは**表示に閉じる**（要件 8.5。行の並びを書き換えると「並べ替え 1 回で
//! 全行が変更されたように見える」）。したがって表示の序数は文書の位置ではない。両者を
//! 取り違えると、絞り込みが効いている間に**別の行を編集する**（要件 8.6）。
//!
//! **表示の範囲を物理の行へ写す経路は本モジュールに無い。**その写像は `view` 層の行の順序
//! （`RowOrder`。群 2 のタスク 2.1）だけが持ち、編集経路がそこを通る（要件 8.6, 8.9）。
//! ここに [`CellRange`] から `Vec<RowId>` を作る補助を置くと、その補助を呼ぶだけで
//! 画面の位置を対象の行と取り違える経路が生まれる。**置かないのは意図である。**
//!
//! # 上流の型を定義し直さない
//!
//! [`ColumnIndex`] は**上流 `schema-engine` の型の再輸出**である（定義は
//! `schema_engine::compile::plan` にあり、そこが列添字で引ける配列を持つ層だからである）。
//! `SortKey { column }`・`FilterSpec`・`CompiledSchema::validator(column)`・
//! `validate_columns` はいずれも同じ型を使う。本クレートが独自の列添字を定義すると、
//! 見た目が同じ 2 つの型が生まれ、`SortKey` と検証器の間で列が静かにずれる。
//! **列の添字は 1 つでなければならない。**
//!
//! 行の識別子 [`RowId`] とシートの識別子 `SheetId` も同じ理由で定義し直さない（文書モデルの
//! 所有であり、`document-format` のものをそのまま使う）。**本クレートが定義するのは
//! 表示空間の型だけである。**

use core::fmt;

use document_format::RowId;
use schema_engine::{ValuePath, ValuePathSegment};

// 列の添字は上流 `schema-engine` の型そのもの。定義し直さない理由はモジュール冒頭を参照。
pub use schema_engine::ColumnIndex;

/// 可視行の序数: 並べ替えと絞り込みを適用した**あとの**並びで、行が何番目か。
///
/// **これは物理の位置ではない。**同じ序数が、絞り込みの指定ひとつで別の行を指す。
/// たとえば 5 行のうち 2 行目と 4 行目が隠れているとき、序数 `1` は可視の 2 行目
/// （物理の 3 行目）を指す。序数を物理の行の位置として扱うと、絞り込みが効いている間に
/// **別の行を編集する**（要件 8.6）。物理の行を指す型は [`CellAddress`] の行成分
/// （[`RowId`]）だけである。
///
/// # なぜ新型なのか
///
/// [`RowId`]・[`ColumnIndex`]・素の `usize` のいずれとも**別の型**であり、
/// `From<usize>` / `Into<usize>` を持たない。素の数との行き来には [`RowOrdinal::new`] と
/// [`RowOrdinal::get`] の明示が要る。`usize` との相互変換を許すと、行数・列数・序数が
/// すべて素の数として混ざり、**取り違えが型検査を通過する**（`document-format` の
/// 識別子が新型であるのと同じ規律。design.md「Architecture Integration」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowOrdinal(usize);

impl RowOrdinal {
    /// 可視行の 0 起点の序数を包む。
    ///
    /// 引数が可視行数の範囲に入っているかは検査しない（序数に絶対の上界は無く、
    /// 上界は行の順序と表示状態が持つ。範囲の検査は [`RowSpan`] の要求を扱う側が行う）。
    #[inline]
    pub const fn new(ordinal: usize) -> Self {
        Self(ordinal)
    }

    /// 可視行の 0 起点の序数。
    #[inline]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl fmt::Display for RowOrdinal {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// 可視行の区間: 連続する可視行の並び（窓の要求と、影響を受けた行の通知に使う）。
///
/// **半開区間である**: [`RowSpan::start`] を含み、[`RowSpan::end`] を含まない
/// （`start..end`。要素数は `count` と一致する）。半開にする理由は、可視 1 行目が `0` で
/// あって区間の長さが `0` のときも矛盾なく表せ、**隣り合う区間が重ならない**ためである
/// （`[5, 8)` の次は `[8, 11)`。閉区間だと境界の行をどちらが持つかを決める必要が生じる）。
/// [`RowSpan::contains`] の境界はこの規約に従う。
///
/// 区間の座標は**可視行の序数**であって物理の行の位置ではない（[`RowOrdinal`] 参照）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowSpan {
    start: RowOrdinal,
    count: usize,
}

impl RowSpan {
    /// 開始の序数と行数から区間を作る。
    ///
    /// 前提: `start + count` が `usize` を溢れないこと（可視行数は 10 万行の規模であり、
    /// 溢れる入力は本クレートの経路からは作られない）。
    #[inline]
    pub const fn new(start: RowOrdinal, count: usize) -> Self {
        Self { start, count }
    }

    /// 区間の開始（この序数を含む）。
    #[inline]
    pub const fn start(self) -> RowOrdinal {
        self.start
    }

    /// 区間の行数。
    #[inline]
    pub const fn count(self) -> usize {
        self.count
    }

    /// 区間の行数（[`RowSpan::count`] と同じ。`slice::len` と同じ語で読めるようにする）。
    #[inline]
    pub const fn len(self) -> usize {
        self.count
    }

    /// 区間が 1 行も含まないか。
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.count == 0
    }

    /// 区間の終端（**この序数を含まない**。`start + count`）。
    #[inline]
    #[must_use]
    pub const fn end(self) -> usize {
        self.start.0 + self.count
    }

    /// 可視行の序数がこの区間に入るか（開始を含み、終端を含まない）。
    #[inline]
    #[must_use]
    pub const fn contains(self, ordinal: RowOrdinal) -> bool {
        self.start.0 <= ordinal.0 && ordinal.0 < self.end()
    }
}

/// 表示空間のセルの位置: 可視行の序数と列の添字の対。
///
/// **選択と現在位置は画面に見えている位置で指定される**ため、こちらは序数を持つ
/// （要件 2.1, 2.3 の選択、要件 9.8 の現在位置の移動）。編集の経路は選択された表示の範囲を
/// 行の順序（`view` 層の `RowOrder`）で物理の行へ写してから [`CellAddress`] を組み立てる。
/// したがって編集命令が運ぶ宛先（`EditCommand::SetCells`・`PasteRange::anchor`）は
/// [`CellAddress`] であり、この型ではない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellPosition {
    row: RowOrdinal,
    column: ColumnIndex,
}

impl CellPosition {
    /// 可視行の序数と列の添字から位置を作る。
    #[inline]
    pub const fn new(row: RowOrdinal, column: ColumnIndex) -> Self {
        Self { row, column }
    }

    /// 可視行の序数。
    #[inline]
    pub const fn row(self) -> RowOrdinal {
        self.row
    }

    /// 列の添字。
    #[inline]
    pub const fn column(self) -> ColumnIndex {
        self.column
    }
}

/// セルの位置（**物理の同一性**）: 行の識別子と列の添字の対。
///
/// 編集は**行そのもの**へ届かなければならない（要件 8.6）。画面上の位置へ届くと、
/// 並べ替えや絞り込みが効いている間に別の行を書き換える。したがって行の成分は
/// [`RowOrdinal`] ではなく [`RowId`] である（要件 8.6, 8.9）。
///
/// 編集命令が運ぶ宛先がこの型である: `EditCommand::SetCells` / `EditCommand::SetNested` の
/// 対象と `PasteRange::anchor`。**貼り付けの起点も、表示の選択から行の順序（`view` 層の
/// `RowOrder`）を通して写した行そのものである**（要件 8.9: 絞り込みが効いている間の
/// 貼り付けは表示されている行にのみ及ぶ）。
///
/// 列の成分に序数が無いのは、並べ替えと絞り込みが**行だけを並べ替え、列の集合と並び順は
/// スキーマが供給する**ためである（列の表示順の変更は窓の中身を変えない。design.md
/// 「表示状態（ドキュメントに保存されない）」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellAddress {
    row: RowId,
    column: ColumnIndex,
}

impl CellAddress {
    /// 行の識別子と列の添字からセルの位置を作る。
    #[inline]
    pub const fn new(row: RowId, column: ColumnIndex) -> Self {
        Self { row, column }
    }

    /// 行の識別子（物理の同一性はここだけで決まる）。
    #[inline]
    pub const fn row(self) -> RowId {
        self.row
    }

    /// 列の添字。
    #[inline]
    pub const fn column(self) -> ColumnIndex {
        self.column
    }
}

/// 選択された矩形の範囲。**両端を含む**（`start` と `end` の両方のセルが範囲に入る）。
///
/// 要件 2.5 が求める行数・列数・セル数（[`CellRange::row_count`] /
/// [`CellRange::column_count`] / [`CellRange::cell_count`]）と、要件 2.6 が求める
/// 複製・貼り付け・削除・取り消しの対象は**画面の選択**である。したがって角は表示空間の
/// [`CellPosition`] であり、数は**序数だけ**で決まる。
///
/// **これは文書の宛先ではない。**物理の行へ写すには、いま表示されている行の順序
/// （`view` 層の `RowOrder`。群 2）を通さなければならない。本モジュールにその写像を置かない
/// 理由はモジュール冒頭を参照（要件 8.6, 8.9）。
///
/// # 正規化
///
/// [`CellRange::new`] は渡された 2 つの角を**軸ごとに独立して**正規化し、`start <= end` を
/// 両軸で満たすようにする。右下から左上へ引いた選択も、行だけ逆・列だけ逆の選択も、
/// 同じ矩形を表す（表す範囲が同じなら同じ値になる。要件 2.5 の数の一致の前提である）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellRange {
    start: CellPosition,
    end: CellPosition,
}

impl CellRange {
    /// 2 つの角から矩形を作る。角の与えられた順序は問わない（軸ごとに正規化する）。
    #[inline]
    #[must_use]
    pub const fn new(a: CellPosition, b: CellPosition) -> Self {
        // 軸ごとに独立して昇順へ揃える（行だけ逆・列だけ逆の選択も同じ矩形になる）。
        let (top, bottom) = if a.row.get() <= b.row.get() {
            (a.row, b.row)
        } else {
            (b.row, a.row)
        };
        let (left, right) = if a.column.index() <= b.column.index() {
            (a.column, b.column)
        } else {
            (b.column, a.column)
        };
        Self {
            start: CellPosition::new(top, left),
            end: CellPosition::new(bottom, right),
        }
    }

    /// 正規化された左上の角（両端に含まれる）。
    #[inline]
    pub const fn start(self) -> CellPosition {
        self.start
    }

    /// 正規化された右下の角（両端に含まれる）。
    #[inline]
    pub const fn end(self) -> CellPosition {
        self.end
    }

    /// 範囲に含まれる行数（両端を含む）。
    #[inline]
    #[must_use]
    pub const fn row_count(self) -> usize {
        self.end.row.0 - self.start.row.0 + 1
    }

    /// 範囲に含まれる列数（両端を含む）。
    #[inline]
    #[must_use]
    pub const fn column_count(self) -> usize {
        self.end.column.index() - self.start.column.index() + 1
    }

    /// 範囲に含まれるセル数（行数 × 列数。要件 2.5 が画面に示す数）。
    #[inline]
    #[must_use]
    pub const fn cell_count(self) -> usize {
        self.row_count() * self.column_count()
    }
}

/// 入れ子の内側の位置の 1 段（上流 `schema-engine` の [`ValuePathSegment`] を写したもの）。
///
/// 上流の型をそのまま使わないのは、上流の経路が**検証の副産物**であり、本クレートが
/// 所有する表示の都合（提示の組み立て、窓の符号化）とは寿命も変更の理由も異なるためである。
/// 写す経路は [`NestedPath`] の変換 1 つであり、生の JSON から組み立て直す経路は持たない。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NestedPathSegment {
    /// オブジェクトのフィールド名。
    Field(Box<str>),
    /// 配列の 0 起点の要素位置。
    Index(usize),
}

impl From<&ValuePathSegment> for NestedPathSegment {
    #[inline]
    fn from(segment: &ValuePathSegment) -> Self {
        match segment {
            ValuePathSegment::Field(name) => Self::Field(name.clone()),
            ValuePathSegment::Index(index) => Self::Index(*index),
        }
    }
}

/// 入れ子の値の内側の位置。**空ならセル直下**を指す。
///
/// 要件 4.5 は「入れ子のどの位置が違反しているかを特定できる形」で提示することを求める。
/// その位置の表現は上流 `schema-engine` の [`ValuePath`] が既に持っているため、本クレートは
/// それを写すだけであり、**独自の解析器も生の JSON からの再導出も持たない**
/// （`From<&ValuePath>` が唯一の入口である）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct NestedPath(Vec<NestedPathSegment>);

impl NestedPath {
    /// セル直下を指す空の位置（[`NestedPath::default`] と同じ）。
    #[inline]
    pub const fn root() -> Self {
        Self(Vec::new())
    }

    /// 位置の並び。
    #[inline]
    pub fn segments(&self) -> &[NestedPathSegment] {
        &self.0
    }

    /// 位置の段数。
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 位置が空であるか（[`NestedPath::is_root`] と同じ）。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 位置が空であり、セル直下を指すか。
    #[inline]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&ValuePath> for NestedPath {
    /// 上流の違反報告が持つ経路を写す。
    ///
    /// 段数と順序をそのまま保つ（`Field` / `Index` の区別も保つ）。上流の空の経路
    /// （セル直下）は [`NestedPath::root`] へ写る。
    #[inline]
    fn from(path: &ValuePath) -> Self {
        Self(path.segments().iter().map(NestedPathSegment::from).collect())
    }
}

/// 違反を探す向き（`find_violation(from: RowOrdinal, direction: SearchDirection)`）。
///
/// この型が本層（`types`）に属するのは、`design.md`「Components and Interfaces /
/// GridSession」が `find_violation` の引数としてこの型を固定しているためである
/// （`pub fn find_violation(&self, from: RowOrdinal, direction: SearchDirection) ->
/// Option<CellAddress>`）。実装は群 2 のタスク 2.4（要件 4.4）であり、本クレートは
/// その横断する型を先に定義しておく。
///
/// **向きの意味が観測できるのは実装が入ってからである。**現時点でこの 2 変種の意味を
/// 固定する検査は無い（`tests/coordinates.rs` の該当テストを参照）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchDirection {
    /// 前方: 可視行の序数が**増える**向き（指定した位置より後ろの行へ）。
    Forward,
    /// 後方: 可視行の序数が**減る**向き（指定した位置より前の行へ）。
    Backward,
}

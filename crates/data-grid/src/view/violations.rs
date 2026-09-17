//! 可視行の序数に対する違反の索引: [`ViolationIndex`]（要件 4.1, 4.3, 4.4, 4.5）。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本モジュールは `view` 層に
//! 属し、左の `types` と、同じ層の [`RowOrder`] / [`ViolationPresence`] だけを参照する**
//! （design.md「内部の依存の向き」・structure.md「ドメインクレートの内部構造」）。上流には
//! `schema-engine` の検証結果（[`SheetReport`] / [`Violation`]）を求めるだけで、
//! **判定は呼ばない** — 違反を決めるのは `schema-engine` であり、本モジュールは返った結果を
//! 索引へ写すだけである（design.md の Component 表が `ViolationIndex` の依存として
//! `RowOrder` と `schema-engine` を挙げているとおりである）。
//!
//! **依存の向きは `ViolationIndex → RowOrder` の一方向である。**逆に [`RowOrder`] が本索引を
//! 参照すると循環になるため、[`RowOrder`] が違反について知るのは据え付け
//! （[`ViolationPresence`]。`view/mod.rs` にある**値の入れ物**であり、判定も索引も持たない）
//! だけである（`view/mod.rs` のモジュール docs「違反ありの絞り込みは据え付けられた情報だけを
//! 見る」）。
//!
//! # 何を鍵とするか（タスク 2.4 の主題）
//!
//! 鍵は**可視行の序数**（[`RowOrdinal`]）である。design.md の File Structure Plan が本
//! モジュールに与えた役割そのものであり（「可視行の序数に対する違反の索引」）、要件 4.4 の
//! 「表示範囲の外にある違反であっても、その位置へ現在位置を移動する」は**画面の座標**で
//! 答えることを求める（利用者が次に動かすのは表示位置であり、文書の行ではない）。
//!
//! したがって上流の違反が運ぶ行（[`Violation::row`] が返す [`RowId`]。**物理の同一性**）は、
//! [`RowOrder`] を通して序数へ写してから索引に載せる。**表示空間と文書空間を混ぜない**
//! （`types` のモジュール docs「2 つの空間を混ぜない」。序数を物理の行の位置として扱うと、
//! 絞り込みが効いている間に別の行を指す。要件 8.6）。
//!
//! 序数は順序の導出（[`RowOrder::recompute`]）で変わる。したがって本索引は
//! **順序に依らない保持**（[`RowId`] を鍵とする違反）と、**そこから張る序数の写像**を
//! 分けて持つ:
//!
//! | 欄 | 鍵 | 何を持つか | 誰が書き換えるか |
//! |---|---|---|---|
//! | `rows` | [`RowId`] | 行ごとの違反（可視かどうかを問わない） | [`ViolationIndex::build`]（差分更新は 5.2） |
//! | `column_level` | — | 行に属さない違反（列そのものの問題） | [`ViolationIndex::build`]（同上） |
//! | `ordinals` | [`RowOrdinal`] | 可視行の序数 → [`RowId`] の写像 | [`ViolationIndex::rekey`] |
//! | `presence` | [`RowId`] | 2.2 への据え付け（行ごとの違反の列） | [`ViolationIndex::build`]（同上） |
//!
//! 順序や絞り込みが変わって**張り直す**のは `ordinals` だけである（タスク 2.4 の
//! 「順序や絞り込みが変わったとき、索引の鍵を張り直す」）。`rows` は順序に依らないため、
//! 並べ替えや絞り込みのたびに組み直す理由が無い（[`ViolationIndex::rekey`]）。
//!
//! # 索引の形
//!
//! 可視行の序数 1 つにつき [`RowViolations`] が 1 つあり、その行の中で違反している列の
//! 昇順に [`CellViolations`] が並ぶ。1 つのセルの違反は**内側の位置の並び**
//! （`Vec<NestedPath>`）として持つ（要件 4.5。後述「入れ子の内側の位置」）。
//!
//! ```text
//! ViolationIndex
//! ├── ordinals: RowOrdinal -> RowId
//! ├── rows:     RowId -> RowViolations { row, columns: [CellViolations { column, paths }, ..] }
//! │                                                           （列の昇順）
//! └── column_level: [ColumnViolations { column, paths }, ..]  （報告の順）
//! ```
//!
//! [`ViolationIndex::find`] は**行の中で最も小さい列の添字**のセルを返す。1 行に複数の列が
//! 違反しているとき、移動先は 1 つでなければならず、列の昇順で最初のものが決定的である
//! （画面を左から右へ読む順序と同じ）。内側の位置はセルの位置に影響しない — 要件 4.4 が
//! 求めるのは**セルの位置**であり、内側のどの位置かは要件 4.5 が別に示す（5.1 の窓の札と
//! 8.5 の詳細表示が読む）。
//!
//! # 違反の総数（要件 4.3）と、隠れた行・列そのものの問題
//!
//! [`ViolationIndex::violation_total`] は**シートに存在する違反の総数**
//! （[`SheetReport::total_violations`]）である。要件 4.3 の文面は「**表示中のシート**に
//! 存在する違反の総数を提示する」であり、表示中の**行**ではない — 絞り込みで隠れている行の
//! 違反も、行に属さない列そのものの問題も、同じシートの違反である。
//!
//! 絞り込みで隠れた行の数は要件 8.7 が別に求め、[`RowOrder::hidden`] が返す。両者を混ぜると
//! 「絞り込んだら壊れている箇所の数が減った」ように見え、8.7 の数（隠れた行数）と 4.3 の数
//! （違反の総数）のどちらを読んでいるのかが画面から分からなくなる。**総数は絞り込みに依らず、
//! 隠れた行数は違反に依らない** — この 2 つは直交する。
//!
//! 行に属する違反（[`Violation::row`] が `Some`）は、**可視かどうかに関わらず [`RowId`] を
//! 鍵として保持する**（`rows`）。したがって 2.2 への据え付け（[`ViolationIndex::presence`]）は
//! 隠れた行の違反も載せる — 据え付けは「その行の違反」を答えるものであり、いま可視の集合を
//! 答えるものではない（絞り込みは据え付けを**読む**側である）。
//!
//! **序数の写像には可視の行だけが載る。**隠れた行には序数が存在しないためであり、探索が
//! その違反を返さないのは「届かない」ではなく**正しい答え**である（返すべき位置が無い）。
//! これは [`ViolationIndex::find`] の `Option` の意味そのものである。
//!
//! 行に属さない違反（[`Violation::row`] が `None`。**列そのものの問題**）は、序数も行も
//! 持たないため探索でも据え付けでも届かない。総数には数え、**別に保持する**
//! （[`ViolationIndex::column_violations`]）。捨てないのは、これも**要件 4.3**「表示中の
//! シートに存在する違反の総数」の一部だからである（[`ViolationIndex::violation_total`] が
//! 返す数であり、**画面 8.4**（違反の提示と走査）がその数をそのまま出す。列の添字と内側の
//! 位置は分かるが、行が無いので移動先にはならない）。シート全体の検証はこの形の違反を
//! 報告しない — 現れるのは行に属さない値を判定する経路（書き込みの判定。群 3）だけであり、
//! そのときも本型は総数と保持の両方で扱える。
//!
//! ## 上限で切られた報告
//!
//! `schema-engine` の検証は違反の**保持**を上限で切ることがある（[`ValidationOptions`]。
//! 10 万行規模の報告から記憶域を守るためであり、要件 5.6）。切られても**総数は切られない**
//! （[`SheetReport::total_violations`]）。本索引は総数をそのまま保持し、載るのは渡された保持の
//! 分だけである。[`ViolationIndex::is_complete`] がその差を観測できる（「保持が切られた」ことと
//! 「違反が無い」ことを混同しない）。
//!
//! 上限の下で**どの違反が残るかは報告が決める**（本モジュールは取捨を行わない）。切られた側の
//! 違反は序数の写像にも据え付けにも載らない — 報告が与えていない情報を本モジュールが作ることは
//! できない。
//!
//! # 探索（要件 4.4）
//!
//! [`ViolationIndex::find`] は、指定された序数から**最も近い違反セル**を向きに沿って返す。
//!
//! | 向き | 規則 |
//! |---|---|
//! | [`SearchDirection::Forward`] | `from` **を含む**、`from` 以上の序数のうち最小の違反 |
//! | [`SearchDirection::Backward`] | `from` **を含む**、`from` 以下の序数のうち最大の違反 |
//!
//! **`from` を含めるのは両方向である。**含めない規則にすると、「違反へ移動」を続けて押した
//! ときに、いま居る違反の次へ進むのか同じ位置に留まるのかが呼び出し側の状態に依ることになる。
//! 含める規則では `find(from, direction)` が**現在位置から見た最も近い違反**を常に答え、
//! 続けて押す側が `from` を「いま居る違反の次の序数」へ進めれば前進する。本モジュールは
//! 呼び出しの履歴を持たない純粋な問い合わせであり、要件 4.4 の「次の違反への移動」は 8.4 が
//! この規則の上に組み立てる。
//!
//! 問い合わせは序数の写像に対する**区間の問い合わせ**（[`BTreeMap`] の `range`）であり、
//! 費用は索引に載った行の数の対数である。**描画の窓（[`RowSpan`]）を読まない**ため、要件
//! 4.4 の「表示範囲の外にある違反」も同じ費用で見つかる（`tests/violation_index.rs` の
//! `search_reaches_a_violation_outside_the_displayed_range` が、窓の中に違反が 1 件も無い
//! ことを表明したうえで窓の外の違反への到達を見る）。
//!
//! `from` が可視行数の外（可視行が 0 のときを含む）でも `None` を返すだけで失敗しない —
//! `Forward` で可視行数以上なら `None`、`Backward` で可視行数以上なら最後の違反である
//! （区間の問い合わせの帰結であり、境界の場合分けを書かない）。
//!
//! # 入れ子の内側の位置（要件 4.5）
//!
//! 入れ子の違反は、**内側の位置を保ったまま**索引に載る。位置は上流の [`ValuePath`] の写しで
//! ある [`NestedPath`]（`types` 層）であり、**写しは 1 つだけ**である — `NestedPath` の
//! `From<&ValuePath>` が唯一の入口であり、本モジュールは経路の解析器も生の表現からの再導出も
//! 持たない（`types` のモジュール docs「上流の型を定義し直さない」）。
//!
//! 1 つのセルに複数の内側の位置の違反があれば、**そのすべてを報告の順に保つ**
//! （[`CellViolations::paths`]）。畳まないのは、索引に載る件数と報告の件数が食い違わない
//! ためである（同じ位置の 2 件を 1 件に畳むと、[`ViolationIndex::violation_total`] と
//! [`ViolationIndex::indexed_violations`] が合わなくなる）。
//!
//! # 鍵の張り直し（タスク 2.4 の「順序や絞り込みが変わったとき」）
//!
//! [`ViolationIndex::rekey`] が序数の写像を**丸ごと張り直す**。呼ぶのは
//! [`RowOrder::recompute`] の**後**である（並べ替えの基準列の変更・絞り込みの変更・行の
//! 増減のいずれも、順序を導出し直した時点で序数と行の対応が変わる）。
//!
//! **読み出しのたびに黙って張り直す形にはしない。**探索（[`ViolationIndex::find`]）と
//! 据え付け（[`ViolationIndex::presence`]）が `&self` である限り、読み出しは状態を変えない
//! ことが型の上に現れる（2.1 が [`RowOrder::recompute`] を `&Document` にして「ドキュメントを
//! 書き換えない」ことを示したのと同じ形である）。張り直しの費用を払う時点が呼び出し側に
//! 見えることも重要である — 黙って張り直す形にすると、10 万行の走査が探索 1 回の内側で
//! 起きうる。
//!
//! ## 張り直しの費用（10 万行で違反が少ない場合）
//!
//! 張り直しは**可視行の並びを 1 回走査**し、その行が `rows` にあるかを引く。費用は
//! `O(可視行数 × log 違反行数)` であり、**違反行が 1 つも無ければ走査そのものを行わない**
//! （`O(1)`。載せる違反が無い索引は序数を持たない）。
//!
//! 違反行ごとに [`RowOrder::ordinal_of`] を呼ぶ形（`O(可視行数 × 違反行数)`）は採らない —
//! 10 万行で違反が 3,000 件ある標本では 3 億回の比較になり、費用の桁が変わる。可視行の並びは
//! **序数の昇順に並んだ [`RowId`] の [`Vec`]** であり、序数から行は `O(1)` で引けるが、行
//! から序数は引けない（[`RowOrder`] は逆写像の索引を持たない。design.md の Service
//! Interface）。実測できる唯一の安い道は 1 回の走査である。
//!
//! この走査が読むのは [`RowId`] だけで、**セルの値も表示文字列も宣言も読まない**。
//! 要件 11.4 の根拠が禁じている「全件検証」とは費用の階級が違う（全件検証は 10 万行 × 30 列の
//! すべての値を宣言に照らす。本走査は [`RowId`] の比較だけである）。それでも走査を避けたい
//! 経路（1 セルの編集の直後の差分更新。design.md の `ViolationIndex` の Intent「編集で差分
//! 更新する」）は 5.2 が**変わった行だけ**を扱う道である — 本モジュールは、その差分が触る先と
//! して [`RowId`] を鍵とする保持を組み立て、張り直しを順序の変化だけに閉じている。
//!
//! # 据え付け（2.2 への引き渡し）— 層の鎖を閉じる向き
//!
//! [`ViolationIndex::presence`] が [`ViolationPresence`] を作り、
//! [`ViolationIndex::install`] がそれを [`RowOrder`] へ据え付ける。向きは
//! **索引 → 据え付け → [`RowOrder`]** の一方向であり、[`RowOrder`] は本索引を知らない
//! （`view/mod.rs` のモジュール docs の循環を閉じないための形）。したがって `view` 層の中で
//! 依存が閉じ、層の鎖 `error / types → view → …` は崩れない。
//!
//! 据え付けは [`FilterSpec::HasViolation`] が読む**行ごとの列の集合**であり、本索引の
//! [`RowId`] を鍵とする保持から、**可視かどうかに依らず**作る（前述）。順序と絞り込みが
//! 変わっても据え付けは変わらない（鍵は [`RowId`] であり、序数ではない）— だから
//! [`ViolationIndex::rekey`] は据え付けに触れない。
//!
//! # 編集の差分更新（タスク 5.2）
//!
//! design.md は `ViolationIndex` の役割を「編集で差分更新する」とする。差分の**入口**は
//! 編集の適用が返す違反（[`EditOutcome::violations`]）であり、本モジュールはそれを
//! 索引へ載せ替える口を 1 つ持つ（[`ViolationIndex::apply_report_delta`]）。
//!
//! 差分が触るのは `rows`（行を鍵とする保持。順序に依らない）と、そこから作る `presence` と
//! `total` / `indexed` であり、`ordinals`（序数 → 行の写像）は**触らない** — 値だけの編集は
//! 行の集合も順序も変えないためである（要件 8.8）。順序が変わったときは呼び出し側が
//! [`ViolationIndex::rekey`] を呼ぶ（`set_view` と、行の集合を変える編集の後。5.2）。
//!
//! **全件検証を呼び直す経路は持たない**（要件 11.4）。差分の材料は編集の適用が既に得ている
//! 報告であり、本モジュールはその写しを載せ替えるだけである。
//!
//! [`EditOutcome::violations`]: crate::edit::EditOutcome::violations
//!
//! # 決定性
//!
//! 同じ報告と同じ順序からは常に同じ索引が出る。内部の写像はすべて [`BTreeMap`] であり、
//! **反復の順が値から決まる**（`document-format` が `NestedValue::Object` で `HashMap` を
//! 禁じているのと同じ理由。順序に依る実装を後から足したときに決定性が壊れないようにする。
//! `view/mod.rs` の [`ViolationPresence`] と同じ規律である）。報告の順（行の並び → 列の
//! 添字 → 入れ子の位置）は上流が決めており、本モジュールは並びを保つ。
//!
//! # 本モジュールが持たないもの
//!
//! - **違反の理由**。[`ViolationReason`] は運ばない — 本索引は**位置の索引**である
//!   （design.md の Requirements Traceability は 4.1・4.2・4.6 の理由の提示を
//!   `WindowCodec` / `GridScreen` / `ViolationBar` に割り当て、`ViolationIndex` には
//!   `violation_total` と `find_violation` を割り当てている）。理由は判定の結果から直接に
//!   組まれ、本索引はそこを通らない
//! - **判定**。違反を決めるのは `schema-engine` であり、本モジュールは検証を呼ばない
//!   （呼ぶのは検証結果を渡す側である）
//! - **編集の差分更新**（5.2）と、**窓への符号化**（5.1。design.md の `WindowCodec`）
//! - 誤り型。組み立てと張り直しは失敗しない（索引は常にその時点の入力から決まる）
//!
//! [`BTreeMap`]: std::collections::BTreeMap
//! [`FilterSpec::HasViolation`]: super::FilterSpec::HasViolation
//! [`RowSpan`]: crate::types::RowSpan
//! [`ValuePath`]: schema_engine::ValuePath
//! [`ViolationReason`]: schema_engine::ViolationReason
//! [`ValidationOptions`]: schema_engine::ValidationOptions
//! [`Violation::row`]: schema_engine::Violation::row

use std::collections::BTreeMap;

use document_format::RowId;
use schema_engine::{SheetReport, Violation};

use crate::types::{CellAddress, ColumnIndex, NestedPath, RowOrdinal, SearchDirection};

use super::{RowOrder, ViolationPresence};

/// 1 つのセルの違反: 列の添字と、違反している内側の位置の並び（要件 4.5）。
///
/// **1 つのセルに複数の違反がありうる** — 入れ子の値の複数の位置が同時に違反することも、
/// 同じ位置が複数回報告されることもある。畳まない理由は索引に載る件数を報告の件数と一致
/// させるためである（モジュール docs「入れ子の内側の位置」）。
///
/// 欄を私的にして読み出しをメソッドにしてあるのは、[`RowViolations::columns`] が**列の昇順**
/// であることを型の上で保証するためである（[`ViolationIndex::find`] が「最も小さい列」を
/// 返すことが、この並びに依っている）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellViolations {
    /// 違反している列の添字（0 起点。`Row::values()` に対する位置）。
    column: ColumnIndex,
    /// 違反している内側の位置（報告の順。セル直下は空の位置）。
    paths: Vec<NestedPath>,
}

impl CellViolations {
    /// 違反している列の添字。
    #[inline]
    pub fn column(&self) -> ColumnIndex {
        self.column
    }

    /// 違反している内側の位置（報告の順。セル直下の違反は空の位置として現れる）。
    ///
    /// 空になることはない（違反が 1 件も無いセルは [`RowViolations`] に現れない）。
    #[inline]
    pub fn paths(&self) -> &[NestedPath] {
        &self.paths
    }
}

/// 1 つの行の違反: 行の識別子と、違反しているセルの並び。
///
/// セルは**列の添字の昇順**である（[`ViolationIndex::find`] が行の中で最も小さい列を返す
/// ことがこの並びに依る）。並びは [`ViolationIndex::build`] が列の添字を鍵とする写像を経て
/// 組み立てるため、入力の順に依らない。
///
/// 本型は**可視かどうかに依らない**（序数ではなく行を鍵として持つ）。並べ替えと絞り込みは
/// 序数の写像だけを変えるので、本型はその影響を受けない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowViolations {
    /// 違反を持つ行そのもの（物理の同一性）。
    row: RowId,
    /// 違反しているセル（列の昇順。空になることはない）。
    columns: Vec<CellViolations>,
}

impl RowViolations {
    /// 違反を持つ行（物理の同一性）。
    #[inline]
    pub fn row(&self) -> RowId {
        self.row
    }

    /// 違反しているセル（列の添字の昇順）。
    #[inline]
    pub fn columns(&self) -> &[CellViolations] {
        &self.columns
    }

    /// 指定した列のセルの違反（その列に違反が無ければ `None`）。
    #[inline]
    pub fn cell(&self, column: ColumnIndex) -> Option<&CellViolations> {
        self.columns
            .binary_search_by_key(&column, CellViolations::column)
            .ok()
            .map(|found| &self.columns[found])
    }

    /// この行の違反の件数（内側の位置の数を数える）。
    #[inline]
    pub fn len(&self) -> usize {
        self.columns.iter().map(|cell| cell.paths.len()).sum()
    }

    /// この行が違反を 1 件も持たないか（本型は違反のある行にしか作られないため常に `false`）。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// 別に確定したセルの並びを併合する（**列の昇順を保つ**。[`ViolationIndex::build`] の
    /// 下請けであり、同じ行が報告に 2 度現れたときに使う）。
    ///
    /// `incoming` は列の昇順であることを前提とする（[`flush_row`] が列を鍵とする写像の
    /// `into_values` から作るため、常に成り立つ）。同じ列が既にあれば、そのセルの内側の位置を
    /// **後ろへ足す**（報告の順を保つ）。
    ///
    /// 新しい並びを組み立てるのではなく既存の並びへ挿入するのは、同じ行が 2 度現れるのが
    /// 例外的な入力だからである（通常の経路は [`RowViolations::columns`] が空の状態で
    /// 1 度だけ確定する。`flush_row`）。
    fn merge(&mut self, incoming: Vec<CellViolations>) {
        for cell in incoming {
            match self
                .columns
                .binary_search_by_key(&cell.column, CellViolations::column)
            {
                Ok(found) => self.columns[found].paths.extend(cell.paths),
                Err(at) => self.columns.insert(at, cell),
            }
        }
    }
}

/// 行に属さない違反（**列そのものの問題**。上流の [`Violation::row`][row] が `None`）。
///
/// 列の添字と、違反している内側の位置を持つ。**行が無いため序数の写像にも据え付けにも
/// 載らない**（探索の移動先にもならない）。総数には数える（モジュール docs「違反の総数」）。
///
/// シート全体の検証はこの形の違反を報告しない — 現れるのは行に属さない値を判定する経路
/// （書き込みの判定）だけである。それでも本型を持つのは、[`ViolationIndex::build`] が報告の
/// すべての違反を扱い、**報告の総数と索引の件数が食い違わない**ためである。
///
/// [row]: schema_engine::Violation::row
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnViolations {
    /// 問題のある列の添字（0 起点。`Row::values()` に対する位置）。
    column: ColumnIndex,
    /// 違反している内側の位置（報告の順。空になることはない）。
    paths: Vec<NestedPath>,
}

impl ColumnViolations {
    /// 問題のある列の添字。
    #[inline]
    pub fn column(&self) -> ColumnIndex {
        self.column
    }

    /// 違反している内側の位置（報告の順。空になることはない）。
    #[inline]
    pub fn paths(&self) -> &[NestedPath] {
        &self.paths
    }
}

/// 可視行の序数に対する違反の索引（design.md の Component 表の `ViolationIndex`。
/// 要件 4.1, 4.3, 4.4, 4.5）。
///
/// 検証の結果（[`SheetReport`]）と、いまの表示の順序（[`RowOrder`]）から組み立てる
/// （[`ViolationIndex::build`]）。順序や絞り込みが変わったら
/// [`ViolationIndex::rekey`] で鍵を張り直す。
///
/// # 何を答えるか
///
/// | 問い | メソッド | 要件 |
/// |---|---|---|
/// | シートの違反の総数 | [`ViolationIndex::violation_total`] | 4.3 |
/// | 指定した位置から最も近い違反セル | [`ViolationIndex::find`] | 4.4 |
/// | **行と列を指定したセルの違反**（内側の位置を含む） | [`ViolationIndex::cell_at`] | 4.2, 4.5 |
/// | 序数ごとの違反（内側の位置を含む） | [`ViolationIndex::row_violations`] | 4.1, 4.5 |
/// | 行に属さない違反（列そのものの問題） | [`ViolationIndex::column_violations`] | 4.3 |
/// | 2.2 への据え付け | [`ViolationIndex::presence`] / [`ViolationIndex::install`] | 4.3, 8.4 |
///
/// 型の全体像と設計の判断はモジュール docs が正典である。とくに**鍵が可視行の序数である
/// こと**、**総数は絞り込みに依らないこと**、**探索が描画の窓を読まないこと**の 3 点が
/// 本型の核心である。
///
/// # 決定性
///
/// 内部はすべて [`BTreeMap`] / [`Vec`] であり、`HashMap` を持たない（モジュール docs
/// 「決定性」）。同じ報告と同じ順序からは常に同じ値になり、読み出しは状態を変えない。
///
/// [`BTreeMap`]: std::collections::BTreeMap
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViolationIndex {
    /// 行を鍵とする違反（可視かどうかを問わない。報告の写し）。
    rows: BTreeMap<RowId, RowViolations>,
    /// 行に属さない違反（列そのものの問題。報告の順）。
    column_level: Vec<ColumnViolations>,
    /// 可視行の序数 → 行の写像（[`ViolationIndex::rekey`] が張り直す唯一の欄）。
    ordinals: BTreeMap<RowOrdinal, RowId>,
    /// 2.2 への据え付け（行ごとの違反の列。可視かどうかに依らない）。
    presence: ViolationPresence,
    /// 報告が数えた違反の総数（保持が切られても切られない。要件 4.3）。
    total: usize,
    /// この索引に実際に載っている違反の件数（総数と食い違うのは保持が切られたときだけ）。
    indexed: usize,
}

impl ViolationIndex {
    /// 検証結果と、いまの表示の順序から索引を組み立てる。
    ///
    /// `report` の違反を、行の識別子を鍵とする保持（行に属する違反）と、行に属さない保持
    /// （列そのものの問題）に振り分け、続いて [`ViolationIndex::rekey`] で序数の写像を張る。
    ///
    /// **`order` は [`RowOrder::recompute`] を終えたばかりのもの**を渡す（序数と行の対応が
    /// その時点の表示であること）。組み立てた後に並べ替えや絞り込みを行う場合は、順序を
    /// 導出し直した後で [`ViolationIndex::rekey`] を呼ぶ。
    ///
    /// 費用は `O(違反の件数 × log) + O(可視行数 × log 違反行数)` である（後者は載せる違反が
    /// 1 件も無ければ走査しない）。モジュール docs「張り直しの費用」。
    #[must_use]
    pub fn build(report: &SheetReport, order: &RowOrder) -> Self {
        let mut index = Self::default();
        index.total = report.total_violations();

        // 行を跨いで使い回す、列の添字 → セルの一時の表（列の昇順に積むために通す）。
        let mut cells: BTreeMap<ColumnIndex, CellViolations> = BTreeMap::new();
        let mut current: Option<RowId> = None;

        for violation in report.violations() {
            let path = NestedPath::from(violation.path());
            match violation.row() {
                Some(row) => {
                    // 行が変わったら、直前の行の欄を確定する。報告は**行の並び順**に違反を運ぶ
                    // ことを求められている（`ViolationReport::push` の docs「行は行の並び順で
                    // 押し込むこと」）ため、直前の行と比べれば通常は足りる。**同じ行が再び
                    // 現れる報告にも耐える** — そのときは `flush_row` が既存の欄へ**併合**する
                    // （上書きにすると、先に確定した列が静かに消える。`flush_row` の docs）。
                    if current != Some(row) {
                        if let Some(previous) = current.replace(row) {
                            flush_row(&mut index, previous, &mut cells);
                        }
                    }
                    let column = violation.column();
                    cells
                        .entry(column)
                        .or_insert_with(|| CellViolations {
                            column,
                            paths: Vec::new(),
                        })
                        .paths
                        .push(path);
                }
                // 行に属さない違反（列そのものの問題）。行ごとの表には入れられないため、
                // 列ごとに畳んで報告の順に保つ。
                None => {
                    index.indexed += 1;
                    push_rowless(&mut index.column_level, violation.column(), path);
                }
            }
        }
        if let Some(previous) = current {
            flush_row(&mut index, previous, &mut cells);
        }

        index.rekey(order);
        index
    }

    /// 編集の適用が返した違反で、行を鍵とする保持を**載せ替える**（タスク 5.2。要件 4.6,
    /// 11.4）。
    ///
    /// 差分の材料は [`EditOutcome::violations`](crate::edit::EditOutcome::violations) と
    /// [`EditOutcome::violation_total`](crate::edit::EditOutcome::violation_total) である。
    /// **どちらも編集の適用が既に得ている報告そのものであり、本メソッドは検証を呼ばない**
    /// （要件 11.4 が禁じる全件検証の再実行を、差分更新の側からも呼ばない）。
    ///
    /// # 何を置き換え、何を置き換えないか
    ///
    /// `violations` が覆うのは**適用が再検証した列に属する違反**だけである
    /// （`EditOutcome::violations` の docs の表）。したがって本メソッドは `columns`
    /// （適用が再検証した列）に属する違反を**その列について丸ごと置き換える** — 消えた違反は
    /// 消え、生じた違反は載る。他の列の保持は**そのまま残す**（本メソッドは覆っていない列の
    /// 違反について何も主張しない）。
    ///
    /// 行に属する違反（[`Violation::row`] が `Some`）は `rows` を、行に属さない違反
    /// （`None`）は `column_level` を置き換える。**行そのもの**（どの行が存在するか）は
    /// 呼び出し側が決める — 行の集合が変わったときは呼び出し側が順序を導出し直してから本
    /// メソッドを呼ぶ。本メソッドは与えられた違反の**位置**だけを扱い、最後に `order` で
    /// 序数の写像を張り直す（据え付けと序数が**呼び出しの後の状態**でそろう）。
    ///
    /// # `total` は呼び出し側が渡す
    ///
    /// 本メソッドは `total` を計算しない。**シート全体の総数は、覆った列に閉じた報告からは
    /// 導けない**ためである（1 セルの編集は 1 列しか再検証しない。覆っていない列の違反は
    /// 報告に現れない）。総数をどう閉じるかは 5.2 の `GridSession` が決める
    /// （`crate::api` のモジュール docs「違反の総数をどう閉じるか」）。本メソッドは
    /// 渡された `total` をそのまま総数として据える。
    ///
    /// # `indexed` と [`ViolationIndex::is_complete`]
    ///
    /// 載せ替えたあとの `indexed` は、`rows` と `column_level` に実際に載っている違反の数で
    /// 数え直す。したがって `is_complete()` の意味（「載せた件数が総数に一致するか」）は
    /// 変わらない — 5.2 が真のシート総数を据えれば、上限で切られていない索引では真になる。
    ///
    /// # 据え付け（2.2 への引き渡し）
    ///
    /// `presence` は**行を鍵とする保持から作り直す**（据え付けは「その行の違反」の写しで
    /// あり、差分の後に古い写しを残すと [`FilterSpec::HasViolation`][filter] が編集前の状態を答える）。
    /// 据え付けの**全体**を作り直すのは、その行の違反が空になった場合に列の印を消す必要が
    /// あるためである（[`ViolationPresence`] は印を消す口を持たない — 丸ごと入れ替える形が
    /// 2.2 の契約である。`view/mod.rs` の `set_violation_presence` の docs）。
    ///
    /// [filter]: super::FilterSpec::HasViolation
    pub fn apply_report_delta(
        &mut self,
        order: &RowOrder,
        columns: &[ColumnIndex],
        violations: &[Violation],
        total: usize,
    ) {
        // 1. 覆った列の違反を、行を鍵とする保持と行に属さない保持の双方から取り除く。
        //    取り除いた行（違反を持たなくなった行）は、序数の写像を直す対象である。
        let mut changed: Vec<RowId> = self.remove_columns(columns);

        // 2. 渡された違反を載せる。行に属さない違反は `column_level` へ、属する違反は
        //    一時の表を経て**列の昇順**の欄へ確定する（`build` と同じ規則 — 並びの作り方を
        //    2 つ持たない）。
        let mut cells: BTreeMap<ColumnIndex, CellViolations> = BTreeMap::new();
        let mut current: Option<RowId> = None;
        for violation in violations {
            let path = NestedPath::from(violation.path());
            match violation.row() {
                Some(row) => {
                    if current != Some(row) {
                        if let Some(previous) = current.replace(row) {
                            flush_row(self, previous, &mut cells);
                        }
                    }
                    let column = violation.column();
                    cells
                        .entry(column)
                        .or_insert_with(|| CellViolations {
                            column,
                            paths: Vec::new(),
                        })
                        .paths
                        .push(path);
                }
                None => {
                    push_rowless(&mut self.column_level, violation.column(), path);
                }
            }
        }
        if let Some(previous) = current {
            flush_row(self, previous, &mut cells);
        }
        changed.extend(violations.iter().filter_map(Violation::row));

        // 3. 総数と、載っている件数と、据え付けを据え直し、**変わった行だけ**の序数を直す。
        self.total = total;
        self.indexed = self.count_indexed();
        self.rebuild_presence();
        self.relink(&changed, order);
    }

    /// **変わった行だけ**の序数の写像を直す（[`ViolationIndex::apply_report_delta`] の
    /// 下請け。タスク 5.2「差分で更新する」の実体）。
    ///
    /// 値だけの編集は行の集合も順序も変えないため、序数と行の対応が変わるのは**違反を持つ
    /// ようになった行**と**違反を持たなくなった行**だけである（要件 8.8 が並べ替えの基準列の
    /// 値の編集で行を動かさないと定めているとおりである）。したがって
    /// [`ViolationIndex::rekey`]（可視行の**全部**を走査する）を呼ばず、`changed` の行だけを
    /// 直す。
    ///
    /// `changed` の並びは重複を許す（呼び出し側が 2 つの源を継ぐため）。同じ行を 2 度直しても
    /// 結果は同じである（この経路は冪等である）。
    ///
    /// [`RowOrder::ordinal_of`] は可視行の走査であり、`ordinals` からの除去は索引が載せる
    /// 違反行（**違反を持つ行だけ**）の走査である — どちらの走査も `RowId` しか読まない
    /// （モジュール docs「張り直しの費用」。全件検証とは費用の階級が違う）。
    fn relink(&mut self, changed: &[RowId], order: &RowOrder) {
        if changed.is_empty() {
            return;
        }
        for row in changed {
            match order.ordinal_of(*row) {
                // 可視であり、かつ違反を持つ: 序数を載せる（既にあれば同じ値で上書きされる）。
                Some(ordinal) if self.rows.contains_key(row) => {
                    self.ordinals.insert(ordinal, *row);
                }
                // 違反を持たないか、可視でない（隠れた行には序数が存在しない）。
                _ => {
                    self.ordinals.retain(|_, found| found != row);
                }
            }
        }
    }

    /// `columns` に属する違反を、この索引が**いま保持している数**（タスク 5.2 の差分の被減数）。
    ///
    /// 行に属する違反（`rows`）と行に属さない違反（`column_level`）の**双方**を数える —
    /// [`ViolationIndex::violation_total`] が数える集合と同じ集合であり、`columns` を
    /// 絞り込んだだけである。したがって被減数は `violation_total` を超えない。
    ///
    /// 数の意味は「**いま索引が載せている**その列の違反」である。詳細な報告から組み立てた
    /// 索引では（`is_complete()` が真のとき）それが「シートのその列の違反」に一致する — 5.2 の
    /// 差分の正しさはこの一致に依る（`crate::api` のモジュール docs「違反の総数をどう閉じるか」）。
    #[must_use]
    pub fn violations_in_columns(&self, columns: &[ColumnIndex]) -> usize {
        if columns.is_empty() {
            return 0;
        }
        let rows: usize = self
            .rows
            .values()
            .map(|entry| {
                entry
                    .columns
                    .iter()
                    .filter(|cell| columns.contains(&cell.column))
                    .map(|cell| cell.paths.len())
                    .sum::<usize>()
            })
            .sum();
        let level: usize = self
            .column_level
            .iter()
            .filter(|level| columns.contains(&level.column))
            .map(|level| level.paths.len())
            .sum();
        rows + level
    }

    /// `columns` に属する違反を、行を鍵とする保持と行に属さない保持から取り除き、
    /// **触れた行**（違反を持たなくなった行を含む）を返す
    /// （[`ViolationIndex::apply_report_delta`] の下請け）。
    ///
    /// 空になった欄は**取り除く**（違反が 1 件も無い行は `rows` に現れない — この不変条件は
    /// [`ViolationIndex::build`] が守っており、差分の後も保つ。`row_violations` と
    /// `find` の「違反を持つ行」の意味がこれに依っている）。
    ///
    /// 返る行の並びは、この呼び出しで**欄が変わった**（違反を失った列があった）行である。
    /// 重複はしない。行の順は `rows` の鍵の順（`RowId` の順）である。
    fn remove_columns(&mut self, columns: &[ColumnIndex]) -> Vec<RowId> {
        if columns.is_empty() {
            return Vec::new();
        }
        let mut changed = Vec::new();
        let mut emptied: Vec<RowId> = Vec::new();
        for (row, entry) in self.rows.iter_mut() {
            let before = entry.columns.len();
            entry.columns.retain(|cell| !columns.contains(&cell.column));
            if entry.columns.len() != before {
                changed.push(*row);
            }
            if entry.columns.is_empty() {
                emptied.push(*row);
            }
        }
        for row in emptied {
            self.rows.remove(&row);
        }
        // 行に属さない保持は**覆った列の欄だけ**を取り除く（覆っていない列の違反はそのまま
        // 残す。丸ごと空にすると、たとえば 1 セルの編集で他の列の列レベルの違反が消える）。
        self.column_level
            .retain(|level| !columns.contains(&level.column));
        changed
    }

    /// 保持に載っている違反の数を数える（`rows` と `column_level` の和）。
    fn count_indexed(&self) -> usize {
        let rows: usize = self
            .rows
            .values()
            .map(|entry| {
                entry
                    .columns
                    .iter()
                    .map(|cell| cell.paths.len())
                    .sum::<usize>()
            })
            .sum();
        let columns: usize = self
            .column_level
            .iter()
            .map(|level| level.paths.len())
            .sum();
        rows + columns
    }

    /// 行を鍵とする保持から据え付けを作り直す（[`ViolationIndex::apply_report_delta`] の
    /// 下請け。規則は `flush_row` と同じである）。
    fn rebuild_presence(&mut self) {
        let mut presence = ViolationPresence::new();
        for (row, entry) in &self.rows {
            for cell in &entry.columns {
                presence.mark_column(*row, cell.column);
            }
        }
        self.presence = presence;
    }

    /// 序数の写像を、いまの表示の順序へ張り直す（タスク 2.4「順序や絞り込みが変わったとき」）。
    ///
    /// [`RowOrder::recompute`] の**後に**呼ぶ。並べ替えの基準列の変更・絞り込みの変更・
    /// 行の増減はいずれも序数と行の対応を変えるため、順序を導出し直した時点で張り直す。
    ///
    /// **行の識別子を鍵とする保持は張り直さない** — 編集で値が変わっていなければ違反も
    /// 変わらず、順序は「どの行が何番目に見えるか」だけを変える（同じ報告から同じ索引が出る
    /// ことは、並べ替えの前後で [`ViolationIndex::violation_total`] が変わらないことに現れる）。
    ///
    /// 費用は `O(可視行数 × log 違反行数)` であり、**載せる違反が 1 つも無ければ可視行を
    /// 走査しない**（モジュール docs「張り直しの費用」）。
    pub fn rekey(&mut self, order: &RowOrder) {
        self.ordinals.clear();
        if self.rows.is_empty() {
            // 載せる違反が 1 件も無い。10 万行の走査を強いない（モジュール docs「張り直しの
            // 費用」）。
            return;
        }
        for pointer in 0..order.len() {
            let ordinal = RowOrdinal::new(pointer);
            let Some(row) = order.row_at(ordinal) else {
                continue;
            };
            // 違反を持たない行は序数を持たない（写像に載るのは違反のある行だけである）。
            if self.rows.contains_key(&row) {
                self.ordinals.insert(ordinal, row);
            }
        }
    }

    /// 指定した位置から、その向きで最も近い違反セル（要件 4.4）。
    ///
    /// `from` は**両方向で含まれる**（`from` 自身が違反していればそのセルを返す。規則は
    /// モジュール docs「探索」）。向きに違反が無ければ `None`。
    ///
    /// 返るのは**セルの位置（物理の同一性。行と列の対）**である（design.md の
    /// `find_violation` の signature。`types` の [`CellAddress`]）。表示の位置（序数）では
    /// ないため、返った位置をそのまま編集の宛先に使うことも、行の順序を通して表示の位置へ
    /// 写すこともできる（要件 8.6）。
    ///
    /// **描画の窓を読まない。**序数の写像に対する区間の問い合わせであり、表示範囲の外の違反も
    /// 見つかる（要件 4.4）。`from` が可視行数の外でも失敗しない（可視行数以上から後方へ
    /// 探すと最後の違反が返る。モジュール docs「探索」）。
    #[must_use]
    pub fn find(&self, from: RowOrdinal, direction: SearchDirection) -> Option<CellAddress> {
        let row = match direction {
            // 前方: `from` 以上で最小の違反。
            SearchDirection::Forward => self.ordinals.range(from..).next().map(|(_, row)| *row),
            // 後方: `from` 以下で最大の違反。
            SearchDirection::Backward => self
                .ordinals
                .range(..=from)
                .next_back()
                .map(|(_, row)| *row),
        }?;
        let entry = self.rows.get(&row)?;
        // 行の中で最も小さい列の添字を返す（欄は列の昇順である）。
        let cell = entry.columns.first()?;
        Some(CellAddress::new(row, cell.column))
    }

    /// シートに存在する違反の総数（要件 4.3）。
    ///
    /// **絞り込みに依らない。**隠れた行の違反も、行に属さない列そのものの問題も数える
    /// （モジュール docs「違反の総数」）。信用できないのは、報告が保持を切った場合の**内訳**
    /// だけである — 総数は切られない（[`ViolationIndex::is_complete`]）。
    #[inline]
    pub fn violation_total(&self) -> usize {
        self.total
    }

    /// この索引が報告から実際に載せた違反の件数（[`ViolationIndex::is_complete`] の右辺）。
    ///
    /// 診断と、保持が切られた報告の内訳のための数であり、4.3 の提示に使うのは
    /// [`ViolationIndex::violation_total`] である。
    #[inline]
    pub fn indexed_violations(&self) -> usize {
        self.indexed
    }

    /// 報告の保持が上限で切られていないか（載せた件数が総数に一致するか）。
    ///
    /// `false` は「違反が無い」ではなく**「報告が保持を切った」**である — 切られた側の違反は
    /// [`ViolationIndex::find`] にも [`ViolationIndex::presence`] にも載らない
    /// （モジュール docs「上限で切られた報告」）。呼び出し側はこの差を見て、必要なら上限を
    /// 上げた検証を組み立てる（本モジュールは検証を呼ばない）。
    #[inline]
    pub fn is_complete(&self) -> bool {
        self.indexed == self.total
    }

    /// 可視の序数 `ordinal` の行の違反（その序数が存在しない、または違反を持たなければ
    /// `None`）。
    ///
    /// [`RowOrder::row_at`] と同じ意味で序数を引き、その行の違反を返す。4.1 の提示（違反して
    /// いるセルを区別する）と 4.5 の提示（内側の位置）がこの経路を読む。**窓の外の序数**を
    /// 渡しても、可視行数の外でなければ答えが返る（窓は符号化の都合であり、索引は窓を持たない）。
    #[must_use]
    pub fn row_violations(&self, ordinal: RowOrdinal) -> Option<&RowViolations> {
        let row = self.ordinals.get(&ordinal)?;
        self.rows.get(row)
    }

    /// **行と列を指定して引く**（要件 4.2、4.5）。可視の序数 `ordinal` の行の、列 `column` の
    /// セルの違反（そのセルが違反していなければ `None`）。
    ///
    /// # なぜ「行の最小の列」では足りないのか
    ///
    /// [`ViolationIndex::find`] は**行の中で最も小さい列**を返す（1 行に複数の列が違反して
    /// いるとき、移動先を 1 つに定める規則である）。それは要件 4.4 の「次の違反への移動」には
    /// 足りるが、要件 4.2 の「**指定したセル**の違反の理由」には足りない — 利用者が指したのが
    /// 同じ行の**右**のセルであっても、返るのは左のセルの理由になり、指したセルと関係の無い
    /// 理由を、指したセルの理由として見せることになる。
    ///
    /// したがって本口は列を**指定**として受け取り、行の最小の列へは落とさない。行そのものが
    /// 違反を持たない、または指定の列が違反していなければ `None` である（**別の列の違反を
    /// 名乗らない**）。
    ///
    /// # 返るのは入れ子の内側の位置を保った値である（要件 4.5）
    ///
    /// [`CellViolations`] は**そのセルの違反の内側の位置の並び**をそのまま持つ。理由を組み立てる
    /// 側はこの値から、報告の中の違反をセルへ絞り込める（[`RowViolations::cell`] と同じ形で
    /// あり、本口はそれを**序数から**引く）。
    ///
    /// 序数の意味は [`ViolationIndex::row_violations`] と同じである（可視の序数。可視行数の外
    /// なら `None`）。
    #[must_use]
    pub fn cell_at(&self, ordinal: RowOrdinal, column: ColumnIndex) -> Option<&CellViolations> {
        self.row_violations(ordinal)?.cell(column)
    }

    /// 行に属さない違反（列そのものの問題。報告の順。要件 4.3）。
    ///
    /// 序数も行も持たないため、探索の移動先にも据え付けにもならない。総数には数えている
    /// （モジュール docs「違反の総数」）。
    #[inline]
    pub fn column_violations(&self) -> &[ColumnViolations] {
        &self.column_level
    }

    /// 2.2 への据え付け（[`FilterSpec::HasViolation`][filter] が読む行ごとの違反の列）。
    ///
    /// **可視かどうかに依らない** — 据え付けは「その行の違反」を答えるのであり、いま可視の
    /// 集合を答えるものではない（絞り込みが据え付けを読む側である。モジュール docs
    /// 「据え付け」）。行に属さない違反（列そのものの問題）は行を持たないため載らない。
    ///
    /// 据え付けは **索引 → 据え付け → [`RowOrder`]** の向きで渡す
    /// （[`ViolationIndex::install`]）。[`RowOrder`] が本索引を知る形にすると、層の鎖の中で
    /// 依存が循環する。
    ///
    /// [filter]: super::FilterSpec::HasViolation
    #[inline]
    pub fn presence(&self) -> &ViolationPresence {
        &self.presence
    }

    /// 索引の据え付けを `order` に据え付ける（[`RowOrder::set_violation_presence`] の薄い入口）。
    ///
    /// 順序と絞り込みが変わっても据え付けは変わらない（鍵は [`RowId`] であり、序数ではない）
    /// ため、[`ViolationIndex::rekey`] の後だけでなく、**据え付けを入れ替えるとき**
    /// （セッションの組み立てと編集の直後。5.2）にも呼ぶ。`order` が持っている順序は触らない
    /// （据え付けは順序の状態であり、[`RowOrder::recompute`] はこれを消さない）。
    #[inline]
    pub fn install(&self, order: &mut RowOrder) {
        order.set_violation_presence(self.presence.clone());
    }
}

/// 一時の表に溜めた 1 行分のセルを、列の昇順の並びとして `index` へ確定する
/// （[`ViolationIndex::build`] の下請け）。
///
/// 列を鍵とする [`BTreeMap`] の `into_values` が**列の昇順**を生むため、報告の並びに依らず
/// [`RowViolations::columns`] の順序が定まる。
///
/// # 同じ行が 2 度現れる場合（併合）
///
/// 報告は行の並び順に違反を運ぶことが求められている（`ViolationReport::push` の docs）が、
/// その前提を本モジュールは**仮定しない**。同じ [`RowId`] の欄が既にあれば、新しく確定した
/// セルを**列の昇順を保ったまま併合する**（上書きしない）。
///
/// 上書きにすると、報告が同じ行を非連続に 2 度運んだときに**先に確定した列が消える** —
/// 総数（`total`）も載せた件数（`indexed`）も食い違わないため、型の上では取りこぼしを
/// 検出できない静かな消失になる。本索引は 5.2 の差分更新が載せる報告も受けるため、
/// 前提に依らず併合する（`tests/violation_index.rs` の
/// `a_row_appearing_twice_in_the_report_keeps_all_of_its_violations`）。
///
/// [`BTreeMap`]: std::collections::BTreeMap
fn flush_row(
    index: &mut ViolationIndex,
    row: RowId,
    cells: &mut BTreeMap<ColumnIndex, CellViolations>,
) {
    let columns: Vec<CellViolations> = std::mem::take(cells).into_values().collect();
    index.indexed += columns.iter().map(|cell| cell.paths.len()).sum::<usize>();
    // 同じ行の違反は 1 つの欄へ畳む（`rows` の鍵が行であり、報告の並びに依らない）。
    let entry = index.rows.entry(row).or_insert_with(|| RowViolations {
        row,
        columns: Vec::new(),
    });
    // 空なら置き換えてよい（`Vec` の確保を 1 回で済ませる）。既にあれば**併合**する。
    if entry.columns.is_empty() {
        entry.columns = columns;
    } else {
        entry.merge(columns);
    }
    // 据え付けは行ごとの列の集合であり、行を鍵とする保持から作る（可視かどうかに依らない）。
    // 併合した欄の全列を改めて印づける（既存の列への `insert` は同じ値の重複であり、
    // `ViolationPresence` の列の集合は `BTreeSet` であるため、二重に印づけても変わらない）。
    for cell in &entry.columns {
        index.presence.mark_column(row, cell.column);
    }
}

/// 行に属さない違反を、列ごとに畳んで報告の順に積む（[`ViolationIndex::build`] の下請け）。
///
/// 報告は列の添字の昇順に並べて違反を運ぶため、末尾の欄を先に見れば畳める。並びは**報告の
/// 順**であり、本モジュールが並べ替えない。
fn push_rowless(level: &mut Vec<ColumnViolations>, column: ColumnIndex, path: NestedPath) {
    match level.last_mut() {
        Some(last) if last.column == column => last.paths.push(path),
        _ => level.push(ColumnViolations {
            column,
            paths: vec![path],
        }),
    }
}

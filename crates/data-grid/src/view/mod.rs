//! 並べ替えの順序の導出: 可視行の並び [`RowOrder`] と、その指定 [`ViewSpec`]。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本層は左の `types` だけを
//! 参照する**（design.md「内部の依存の向き」）。層の鎖の文言を各層の冒頭に置く規約は
//! `structure.md`「ドメインクレートの内部構造」。
//!
//! 本モジュールが上流に求めるのは `document-format` の [`Document`] / [`Row`] / [`RowId`] と、
//! `schema-engine` の [`ColumnIndex`]・10 進数の正準形（`schema_engine::types::decimal`）だけ
//! である。**判定は呼ばない**（値が型に適合するかを決めるのは `schema-engine` であり、
//! 本モジュールは値の大小だけを決める）。
//!
//! # 何を所有するか
//!
//! [`RowOrder`] は **`Vec<RowId>`（可視行の順）と、隠された行数だけを持つ**
//! （design.md「RowOrder」の Responsibilities & Constraints）。座標の型（[`RowOrdinal`] /
//! [`RowSpan`]）は鎖の最も左の `types` 層にあり、本モジュールはそれを使う側である。
//! [`RowOrder`] / [`ViewSpec`] / [`SortKey`] / [`ViewSummary`] は**本層の状態と指定**であり、
//! design.md の File Structure Plan が `view/mod.rs` に置いている（`types` 層は「どの層にも
//! 依存しない横断する型」の置き場であり、行の順序という状態をそこへ移す理由は無い。
//! `SortKey` が列の添字として `types` の再輸出である [`ColumnIndex`] を使うのは、
//! 列の添字を 2 つに増やさないためである。`types` のモジュール docs 参照）。
//!
//! # 並べ替えは表示に閉じる（要件 8.5）
//!
//! 本製品の中心価値は「いつ・誰が・どのセルを変えたか」が追えることであり、並べ替えが
//! 保存される順序を書き換えると、並べ替え 1 回で全行が変更されたように見える（要件 8 の
//! 方針の根拠）。したがって [`RowOrder::recompute`] は [`Document`] を**共有参照でしか
//! 受け取らない**。この 1 点が「ドキュメントの行の並びを書き換える経路を持たない」ことを
//! 型の上で示す実体である（要件 8.5。`&mut Document` を取る形にすれば書き換える経路が
//! 生まれ、呼び出し側は共有借用を持ったまま呼べなくなる。`tests/sort_order.rs` の
//! `the_order_derivation_only_borrows_the_document` がこの形を検査する）。
//!
//! ドキュメントに触れない帰結として、**保存された値の文字列も変えない**。とくに
//! 10 進数は逐語で往復する契約であり（`document-format` の `value.rs`）、比較のための
//! 正準形は**比較のためだけに作り**、`CellValue::Decimal` の中身へ書き戻さない。
//!
//! # 比較は値の変種ごとの順序で行う（要件 8.3）
//!
//! 表示文字列（`CellValue` を人が見る形へ写した文字列）で比較すると、`9` と `10` が
//! `"10" < "9"` になり、真偽が `"false" < "true"`、値なしが `""` として先頭に来る。
//! したがって本モジュールは [`CellValue`] の**変種ごとに**順序を定め、変種をまたぐときは
//! 順位で決める。これが本タスクの核心である。
//!
//! ## 変種の順位（正典）
//!
//! ```text
//! Null < Bool < Int < Float < Decimal < Text < Nested < Attachment
//! ```
//!
//! この並びが**正典**である（design.md「Implementation Notes」は「セルの表示文字列ではなく
//! 値の変種ごとの順序で行う」ことだけを定めており、順位そのものは本タスクが決める）。
//! [`CellValue`] は `PartialEq` だけを持ち `Ord` を持たないため、この順位と以下の規則が
//! 順序の唯一の源である。決め方は「値なし → 真偽 → 数値 → 文字列 → 構造 → 添付」であり、
//! 並べ替えの第一の読み方（小さい値・偽・空が先、構造は後）に合わせてある。
//!
//! | 変種 | 同じ変種の中の順序 |
//! |---|---|
//! | `Null` | すべて同値（値なしに大小は無い） |
//! | `Bool` | `false < true` |
//! | `Int` | 数値として（`i64` の `Ord`） |
//! | `Float` | 数値として（下記「浮動小数」） |
//! | `Decimal` | **数値として**（下記「10 進数」） |
//! | `Text` | UTF-8 のバイト列の辞書式順序（`str` の `Ord` そのもの。地域の並び替え規則は使わない） |
//! | `Nested` | オブジェクト < 配列。オブジェクトはキー→値の順の辞書式、配列は要素の辞書式（下記） |
//! | `Attachment` | 内容アドレスの識別子（BLAKE3 ダイジェスト）のバイト列順 |
//!
//! **浮動小数**: まず `-0.0` を `0.0` へ畳み（`CellValue::float` の正規化と `CellValue` の
//! `PartialEq` が `-0.0 == 0.0` であることに合わせる。畳まないと、等しいと見なされる 2 つの
//! 値が同値にならず決着が効かない）、`f64::total_cmp` で比べる。非数（`NaN`）は比較の相手が
//! 何であっても順序が定まる位置に来る（正の非数はすべての数より後、負の非数はすべての数より
//! 前）。`NaN` を書けるのは復号の門を迂回した値だけであり（`to_json_bytes` は非有限を
//! 拒否する）、**比較器は全順序でなければならない**（`sort_by` は全順序でない比較器に対して
//! 並びを保証せず、実装によっては失敗する）。
//!
//! **入れ子**: オブジェクトを配列より前に置く。オブジェクトは**キー順をそのまま保つ**
//! `Vec<(String, CellValue)>` であり（`HashMap` は反復順が実行ごとに変わるため
//! `document-format` が禁じている）、先頭から突き合わせてキー、次に値、を比べる。どちらかが
//! 他方の前置なら短い側が先（`Vec` の辞書式の規約と同じ）。配列も同じ規約で要素を比べる。
//!
//! **添付**: 添付の識別子は内容から決まる（content-addressed）ため、同じ内容は常に同じ
//! 位置に並ぶ。順序は 32 バイトのダイジェストのバイト列順であり、これは正準の小文字 hex
//! （固定幅）の辞書順と一致する。
//!
//! # 10 進数は数値として比較する
//!
//! `CellValue::Decimal` の中身は**文字列であり、文法の外の中身も持ちうる**。逐語で往復する
//! 契約（`document-format`。文法に一致しない中身は脱出口 `{"$t":"decimal",...}` で書かれる）
//! のため、文法を検査せずに保持される。
//!
//! 桁数の比較のために**新しい依存は足さない**。10 進数のライブラリ（`rust_decimal` 等）は
//! 出力時に正規化を行うため、逐語の契約を壊す（`schema-engine` の `types/decimal.rs` の
//! モジュール docs「なぜ 10 進数のライブラリを入れないか」）。比較のための正準形は上流に
//! 既にある — `schema_engine::types::decimal::canonicalize` が、文法に一致する文字列を
//! 値そのもの（符号・先頭と末尾の 0 を除いた数字列・10 の指数）へ畳む。本モジュールはそれを
//! **呼ぶだけ**であり、10 進数の文法も桁勘定も書き直さない（文法が 2 つに分かれると、
//! 上流が保存した `Decimal` を本クレートが別の値として並べる状態になる）。
//!
//! 規則は次のとおりである。
//!
//! - 双方が正準形を作れるなら、**正準形の `Ord`** で比べる。正準形の順序は値の順序に一致し、
//!   `-1000 < -2 < -1.5 < 0 < 0.0001 < 0.5 < 1 < 1.5 < 2 < 10 < 100 < 1e3` のようになる
//!   （`"9"` と `"10"` は `9 < 10` であり、文字列の辞書順とは逆である）。
//!   指数表記・末尾の 0・先頭の 0・明示の正符号は同じ値に畳まれるため、`"1.5"` と `"1.50"` と
//!   `"1e1"`/`"10"` は**同値**であり、決着（後述）で `RowId` の順に並ぶ。極端な指数でも
//!   表現が衝突しない（正準形は指数を展開しない）。
//! - **文法に一致しない中身**（決定的な規則が必要であり、値としての大小が存在しない）:
//!   文法に一致する値**より後ろ**に置き、文法外どうしは**バイト列の辞書順**で比べる。
//!   文法外の値は上流でも違反として報告される値であり（`DecimalDigits::accepts` が
//!   `Violating` を返す）、数として読めないものを数の列に混ぜない。規則は全順序であり、
//!   同じ入力からは常に同じ順序が出る（順序を「不定」にしない理由は、要件 8.3 が同一の
//!   入力から同一の順序を求めるためである）。
//!
//! # 変種をまたぐ数値の比較はしない
//!
//! `Int` / `Float` / `Decimal` は**別の変種**であり、順位が違う。したがって `Int(5)` と
//! `Float(5.0)` と `Decimal("5")` は**同値ではなく**、`Int(5) < Float(5.0) < Decimal("5")`
//! の順に並ぶ（数値として同値なら同じ位置に来る、という扱いはしない）。
//!
//! これは「値が型に適合するかの判断を本クレートが持たない」ことの帰結である。列の型が
//! 決まっていれば現れない値の組み合わせ（`document-format` の `CellValue` は変種の閉じた
//! 集合であり、1 つの列に複数の変種が並びうる）でも順序が定まることを優先し、変種の同一性を
//! 値の等値より先に見る。**安定性への帰結**: 数値として等しい値でも変種が違えば同値では
//! ないため、その 2 行の前後は決着（`RowId` の順）には委ねられず、順位が決める。
//!
//! # 決着と決定性（design.md の Invariants）
//!
//! design.md は「同一の `Document` と `ViewSpec` からは常に同一の順序が出る（並べ替えは
//! 安定であり、同値の行は `RowId` の順で並ぶ）」を不変条件とする。本モジュールは
//! **比較器そのものを全順序にする** — 基準列がすべて同値なら、最後の鍵として `RowId` の
//! 昇順を比較する。したがって並べ替えの結果は一意であり、**`sort_by` の安定性には依存しない**
//! （安定でない並べ替えでも同じ結果になる。安定であることは結果の性質として従う）。
//! `RowId` は ULID であり、その `Ord` は値の数値順 = 正準テキスト形の辞書順である
//! （`document-format` の `ids.rs`）。行の識別子は行ごとに一意であるため、決着は必ず付く。
//!
//! 実装は `sort_by`（安定な並べ替え）を使う。`sort_unstable_by` でも結果は同じ（比較器が
//! 全順序だから）であり、**どちらでも正しいが `sort_by` を選ぶ**: 決着の規則を将来
//! 取り違えても（たとえば `RowId` の比較を落としてしまっても）安定性が最後の防波堤として
//! 残るためである。つまり「正しさは全順序の比較器が担い、安定性は保険である」。設計が
//! 「安定な並べ替え」を明示している以上、素直に安定な側を使う。
//!
//! **降順はその基準列の比較だけを反転し、決着は反転しない。**反転させると昇順と降順で
//! 同値の行の並びが変わり、「決着は `RowId` の順」という不変条件が指定に依ってしまう。
//! 反転は各基準列の比較の直後に行い、`RowId` の比較はその外側で最後に 1 度だけ行う。
//!
//! 再計算は [`RowOrder::recompute`] の呼び出しごとに**入力だけから**順序を組み立てる
//! （前回の順序を引き継がない）。したがって「別の指定で上書きした後に戻しても同じ順序」に
//! なり、呼び出しの履歴に依らない。
//!
//! # 基準列が 0 本のときは文書の行順を保つ
//!
//! 決着（`RowId` の昇順）は**基準列があって、そのすべてが同値のとき**にだけ働く。基準列が
//! 0 本のときは比較そのものを行わないため、**文書の行順（`Sheet::rows()` の並び）がそのまま
//! 可視の順**になる。この区別は要件 6.1（位置を指定した行の挿入）と要件 8.6（行の増減が
//! 提示へ直ちに反映される）が要求する — 挿入された行の `RowId` は必ず最も新しい（ULID は
//! 発行時刻を先頭に持つ）ため、基準列が無いときに `RowId` の順へ並べ替えると、挿入した行が
//! **指定した位置ではなく末尾に現れる**。文書の行順は「行が実際に並んでいる順」であり、
//! 基準列が無いときの可視の順はまさにそれである。`Document::reorder_rows`（要件 1.5）が
//! 保存される順序を変えた後も、この規則は一貫して文書の並びを写す。
//!
//! # 値を持たない行
//!
//! 行の値の数が基準列の添字に届かない場合（列の追加前に作られた行、長さの短い行）は
//! **値なし（`CellValue::Null`）として比較する**。列を追加した直後の行が並べ替えで
//! 落ちたり、比較が失敗したりしないためである（`Row::values()` の外を読まない）。
//!
//! # 文書に無いシート
//!
//! `SheetId` が文書に無い場合は**行が 1 件も無いシート**として扱い、可視行 0 の順序を返す
//! （`Document::sheet_by_id` は `Option` を返し、`recompute` の signature は `Result` を
//! 持たない。design.md の Service Interface）。シートの妥当性を先に確かめるのは呼び出し側
//! （`GridSession::set_view`。要件 8.3）の責務である。
//!
//! # 本タスクが持たないもの
//!
//! - **絞り込み**（要件 8.4, 8.7）と、隠された行数の決定。本タスクでは可視行がシートの
//!   全行であるため、隠された行数は 0 になる（`ViewSpec` も `sort` だけを持つ最小の形で
//!   あり、`FilterSpec` と `filters` は絞り込みを実装する 2.2 が足す。意味論の無い
//!   `filters` の欄だけを先に置かない — 空のまま置くと「絞り込みが 0 件である」ことと
//!   「絞り込みを実装していない」ことが区別できなくなる）
//! - **基準列の値の編集で行が動かないこと**（要件 8.8）。順序の再計算はこの入口の呼び出し
//!   でだけ起き、編集の経路（群 3）はここを呼ばない
//! - **可視行の序数に対する違反の索引**（要件 4.1, 4.3, 4.4, 4.5）。2.4 が `view/violations.rs`
//!   に置く（本モジュールは違反を知らない）
//! - **入れ子の展開から導かれる列の構成**（要件 5.1 等）。2.3 が `ViewState` として足す
//! - 誤り型。順序の導出は失敗しない（`GridError` を返す経路を持たない）

use core::cmp::Ordering;

use document_format::{CellValue, Document, NestedValue, Row, RowId, SheetId};
use schema_engine::types::decimal;

use crate::types::{ColumnIndex, RowOrdinal, RowSpan};

/// 並べ替えの基準列 1 本: 列の添字と、降順かどうか。
///
/// 列の添字は `schema-engine` の [`ColumnIndex`] そのものである（本クレートが独自の列添字を
/// 定義すると、見た目が同じ 2 つの型が生まれて列が静かにずれる。`types` のモジュール docs）。
/// 添字は `Row::values()` に対する位置であり、シートの列名の並び（`Sheet::columns`）と
/// `CompiledSchema::columns` の並びは同じものである。
///
/// `descending` は**その基準列の比較だけ**を反転する。同値の行の決着（`RowId` の昇順）は
/// 反転しない（モジュール docs「決着と決定性」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SortKey {
    /// 基準となる列の添字（0 起点。`Row::values()` に対する位置）。
    pub column: ColumnIndex,
    /// この基準列を降順で並べるか。
    pub descending: bool,
}

/// 表示の指定: 行の並びをどう導出するか。
///
/// **本タスクの形は最小である** — 並べ替えの基準列の並びだけを持ち、絞り込みは持たない。
/// design.md の Service Interface は `filters: Vec<FilterSpec>` を持つ形を固定しているが、
/// `FilterSpec` の意味論（一致・部分一致・値なし・値あり・違反あり、要件 8.4）は絞り込みを
/// 実装する 2.2 の主題であり、**意味論の無い欄を先に置かない**（モジュール docs
/// 「本タスクが持たないもの」）。2.2 が `FilterSpec` とともに `filters` を足す。
///
/// 基準列が 0 本のときは比較を行わないため、**文書の行順**（`Sheet::rows()` の並び）が
/// そのまま可視の順になる（`RowId` の順ではない。`Document::reorder_rows` の後では両者は
/// 食い違う）。絞り込みだけを指定する 2.2 の形もこの指定から始まる。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewSpec {
    /// 並べ替えの基準列。先頭が第一の基準であり、同値のときだけ次の基準が効く。
    pub sort: Vec<SortKey>,
}

/// 表示の指定を適用した結果の要約: 可視行数と、隠された行数。
///
/// 「隠された行数」を提示するのは要件 8.7 である（絞り込みによって表示されていない行の数）。
/// 本タスクには絞り込みが無いため、可視行はシートの全行であり、隠された行数は常に 0 になる
/// — **0 を偽って置くのではなく、可視行とシートの行数の差として導出する**
/// （[`RowOrder::recompute`]）。絞り込みを実装する 2.2 が可視の集合を狭めた時点で、この差が
/// そのまま隠された行数になる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewSummary {
    /// 可視行の数（[`RowOrder::len`] と同じ値）。
    pub visible: usize,
    /// 隠された行の数（本タスクでは常に 0。上記）。
    pub hidden: usize,
}

/// 可視行の順序: 並べ替え（と、2.2 で入る絞り込み）を適用した**あとの**行の並び。
///
/// **`Vec<RowId>`（可視行の順）と、隠された行数だけを持つ**（design.md「RowOrder」）。
/// 物理の行の位置や値は持たない（値は [`Document`] が所有し、順序はその写像だけを持つ）。
///
/// 本型が [`Document`] を変更しないことは、[`RowOrder::recompute`] が `&Document` しか
/// 受け取らないことで型の上に現れている（要件 8.5。モジュール docs「並べ替えは表示に
/// 閉じる」）。この型のメソッドに `&mut Document` を取るものは無く、順序を物理の行へ写す
/// 読み出し（[`RowOrder::row_at`]）だけを持つ。
///
/// # 表示空間と文書空間
///
/// [`RowOrdinal`] は**可視行の序数**（この順序の何番目か）であり、[`RowId`] は行そのもので
/// ある。両者を取り違えると、絞り込みや並べ替えが効いている間に**別の行を編集する**
/// （要件 8.6）。写像は本型の [`RowOrder::row_at`] と [`RowOrder::ordinal_of`] の 1 対だけを
/// 通り、逆写像（可視の範囲から `Vec<RowId>` を作る補助）は `types` 層にもここにも置かない
/// （`types` のモジュール docs「2 つの空間を混ぜない」）。
#[derive(Debug, Clone, Default)]
pub struct RowOrder {
    /// 可視行の順（先頭が可視の 1 行目）。
    rows: Vec<RowId>,
    /// 隠された行の数（本タスクでは常に 0。絞り込みを実装する 2.2 が変える）。
    hidden: usize,
}

impl RowOrder {
    /// `doc` の `sheet` の行を `spec` の基準列で並べ替え、可視行の順を組み立てる。
    ///
    /// **`doc` は共有参照である。**これが要件 8.5（「ドキュメントの行の並びを書き換える経路を
    /// 持たない」ことを可変参照を受け取らない形で示す）の実体であり、本メソッドが `Document` に
    /// できることは読み出しだけである。呼び出し側は呼び出しの間も呼び出しの後も
    /// ドキュメントを共有借用したままでよく、`&mut Document` を取る形なら成立しない。
    ///
    /// 行の並びは**入力だけから**決まる（前回の順序を引き継がない）。基準列がすべて同値の
    /// 行は `RowId` の昇順で並ぶ（比較器そのものが全順序であり、`sort_by` の安定性には
    /// 依存しない。モジュール docs「決着と決定性」）。値を持たない行は値なしとして比較し、
    /// `SheetId` が文書に無い場合は可視行 0 の順序になる。
    ///
    /// 可視行数と隠された行数は [`ViewSummary`] として返り、隠された行数は
    /// シートの行数と可視行数の差である（本タスクでは絞り込みが無いため 0。2.2 が
    /// 可視の集合を狭めた時点でこの差が隠された行数になる）。
    pub fn recompute(&mut self, doc: &Document, sheet: SheetId, spec: &ViewSpec) -> ViewSummary {
        let rows: &[Row] = match doc.sheet_by_id(sheet) {
            Some(found) => found.rows(),
            // 文書に無いシートは行が 1 件も無いものとして扱う（モジュール docs）。
            None => &[],
        };

        self.rows.clear();
        if spec.sort.is_empty() {
            // 基準列が 0 本のときは**比較そのものを行わない**（比較が無いので行の同値・非同値も
            // 定まらず、決着も起こらない）。したがって文書の行順がそのまま可視の順になる。
            // これは要件 6.1（位置を指定した挿入）と 8.6（行の増減が直ちに提示へ反映される）が
            // 要求する振る舞いでもある: 挿入された行の `RowId` は必ず最も新しい（ULID）ため、
            // `RowId` の順に並べると指定された位置ではなく末尾に現れてしまう。
            self.rows.extend(rows.iter().map(Row::id));
        } else {
            // 行そのものを借りたまま並べ替える（比較のたびに行を引き直さない）。
            let mut visible: Vec<&Row> = rows.iter().collect();
            visible.sort_by(|left, right| compare_rows(left, right, &spec.sort));
            self.rows.extend(visible.into_iter().map(Row::id));
        }
        self.hidden = rows.len() - self.rows.len();

        ViewSummary {
            visible: self.rows.len(),
            hidden: self.hidden,
        }
    }

    /// 可視の `ordinal` 番目の行。可視行数の外なら `None`。
    ///
    /// 表示空間から文書空間への唯一の写像の 1 つである（要件 8.6, 8.9。編集の経路は
    /// 選択された表示の位置をこのメソッドで行そのものへ写してから宛先を組み立てる）。
    #[inline]
    pub fn row_at(&self, ordinal: RowOrdinal) -> Option<RowId> {
        self.rows.get(ordinal.get()).copied()
    }

    /// 行 `row` が可視の何番目か。可視でなければ `None`。
    ///
    /// [`RowOrder::row_at`] の逆写像である（往復は `row_at(ordinal_of(row)) == row`、
    /// `ordinal_of(row_at(ordinal)) == ordinal`）。可視行の並びを先頭から走査する
    /// （design.md は序数の索引を要求しておらず、行数は 10 万行の規模である）。
    #[inline]
    pub fn ordinal_of(&self, row: RowId) -> Option<RowOrdinal> {
        self.rows
            .iter()
            .position(|candidate| *candidate == row)
            .map(RowOrdinal::new)
    }

    /// `span` が指す可視行の並び。半開区間として切り出す。
    ///
    /// 区間の座標は**可視行の序数**である（[`RowSpan`]）。可視行数の外へ出る要求は
    /// **切り落とす**（`start` が可視行数を超えるなら空、`start + count` が行数を超えるなら
    /// 末尾まで）。窓の要求は表示範囲の端で必ず短くなるため、切り落としは呼び出し側の
    /// 事前検査ではなく本メソッドの規約である（窓の符号化 5.1 と窓の記憶 7.3 が依存する）。
    #[inline]
    pub fn span(&self, span: RowSpan) -> &[RowId] {
        let start = span.start().get().min(self.rows.len());
        let end = start.saturating_add(span.count()).min(self.rows.len());
        &self.rows[start..end]
    }

    /// 可視行の数（[`ViewSummary::visible`] と同じ値）。
    #[inline]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// 可視行が 1 件も無いか。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// 隠された行の数（[`ViewSummary::hidden`] と同じ値。本タスクでは常に 0）。
    #[inline]
    pub fn hidden(&self) -> usize {
        self.hidden
    }
}

/// 2 つの行を `keys` の順に比べ、すべて同値なら `RowId` の昇順で決着する。
///
/// **この比較器は全順序である**（決着が必ず付く）。降順の反転は各基準列の比較の直後に行い、
/// 決着は反転しない（モジュール docs「決着と決定性」）。
///
/// 呼び出し元（[`RowOrder::recompute`]）は `keys` が 1 本以上のときだけ本関数を使う。基準列が
/// 0 本のときは比較そのものを行わない（文書の行順を保つ。モジュール docs「基準列が 0 本の
/// ときは文書の行順を保つ」）。
fn compare_rows(left: &Row, right: &Row, keys: &[SortKey]) -> Ordering {
    for key in keys {
        let ordering = compare_values(value_of(left, key.column), value_of(right, key.column));
        let ordering = match key.descending {
            true => ordering.reverse(),
            false => ordering,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    // 決着: 同値の行は `RowId` の順（design.md の Invariants）。
    left.id().cmp(&right.id())
}

/// 行の `column` 番目の値。値の数が `column` に届かない行は値なしとして扱う。
///
/// 番人の値なし 1 つを共有する（行ごと・比較ごとに複製しない。10 万行 × 複数の基準列の
/// 比較では、この 1 つを返すかどうかが比較のたびの確保に効く）。
fn value_of(row: &Row, column: ColumnIndex) -> &CellValue {
    /// 値を持たない行に返す値なし（すべての行・すべての比較で共有する）。
    static ABSENT: CellValue = CellValue::Null;
    row.values().get(column.index()).unwrap_or(&ABSENT)
}

/// 2 つのセル値を、変種ごとの順序で比べる（変種をまたぐときは順位で決める）。
///
/// 同じ変種の対はその変種の規則（モジュール docs「変種の順位（正典）」の表）で比べ、変種が
/// 違う対は順位（[`VariantRank`]）で比べる。**変種をまたいで数値として比較しない**
/// （`Int(5)` と `Float(5.0)` は順位で決まる。モジュール docs「変種をまたぐ数値の比較は
/// しない」）。
///
/// 新しい変種が `CellValue` に足された場合、[`variant_rank`] の `match` が網羅でなくなるため
/// **コンパイルが止まる**（比較の規則を書かずに素通りすることはない）。
fn compare_values(left: &CellValue, right: &CellValue) -> Ordering {
    match (left, right) {
        (CellValue::Null, CellValue::Null) => Ordering::Equal,
        (CellValue::Bool(left), CellValue::Bool(right)) => left.cmp(right),
        (CellValue::Int(left), CellValue::Int(right)) => left.cmp(right),
        (CellValue::Float(left), CellValue::Float(right)) => compare_floats(*left, *right),
        (CellValue::Decimal(left), CellValue::Decimal(right)) => compare_decimals(left, right),
        (CellValue::Text(left), CellValue::Text(right)) => left.cmp(right),
        (CellValue::Nested(left), CellValue::Nested(right)) => compare_nested(left, right),
        (CellValue::Attachment(left), CellValue::Attachment(right)) => left.cmp(right),
        // 変種が違う: 順位で決める（モジュール docs「変種の順位（正典）」）。
        _ => variant_rank(left).cmp(&variant_rank(right)),
    }
}

/// 変種の順位（モジュール docs「変種の順位（正典）」が唯一の源）。
///
/// **宣言の順がそのまま順位である**（導出した `Ord` が宣言順を写す）。数を直に書かないのは、
/// 順位の表とコードが食い違わないようにするためである（表に行を足してここを直し忘れる、と
/// いう食い違いが起こりえない）。新しい変種が `CellValue` に足されれば、[`variant_rank`] の
/// `match` が網羅でなくなるためコンパイルが止まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum VariantRank {
    /// 値なし（最も小さい。値なしに大小は無い）。
    Null,
    /// 真偽（`false < true`）。
    Bool,
    /// 64 ビット整数。
    Int,
    /// 浮動小数。
    Float,
    /// 10 進数（値として比較する）。
    Decimal,
    /// テキスト。
    Text,
    /// 入れ子（オブジェクト / 配列）。
    Nested,
    /// 添付参照（最も大きい）。
    Attachment,
}

/// 値の変種の順位。
const fn variant_rank(value: &CellValue) -> VariantRank {
    match value {
        CellValue::Null => VariantRank::Null,
        CellValue::Bool(_) => VariantRank::Bool,
        CellValue::Int(_) => VariantRank::Int,
        CellValue::Float(_) => VariantRank::Float,
        CellValue::Decimal(_) => VariantRank::Decimal,
        CellValue::Text(_) => VariantRank::Text,
        CellValue::Nested(_) => VariantRank::Nested,
        CellValue::Attachment(_) => VariantRank::Attachment,
    }
}

/// 浮動小数を数値として比べる（`-0.0` を `0.0` へ畳んでから `f64::total_cmp`）。
///
/// 畳む理由と非数の扱いはモジュール docs「変種の順位（正典）」の「浮動小数」を参照。
fn compare_floats(left: f64, right: f64) -> Ordering {
    /// `-0.0` を `0.0` へ畳む（`PartialEq` が `-0.0 == 0.0` であることに合わせる）。
    fn fold_zero(value: f64) -> f64 {
        if value == 0.0 {
            0.0
        } else {
            value
        }
    }
    fold_zero(left).total_cmp(&fold_zero(right))
}

/// 10 進数を値として比べる（モジュール docs「10 進数は数値として比較する」）。
///
/// 文法に一致する中身は上流の正準形（`schema_engine::types::decimal::canonicalize`）へ畳んで
/// から比べる（**文法も桁勘定も本モジュールで書き直さない**）。文法に一致しない中身は
/// 文法に一致する値の後ろに置き、文法外どうしはバイト列の辞書順で比べる。
fn compare_decimals(left: &str, right: &str) -> Ordering {
    match (decimal::canonicalize(left), decimal::canonicalize(right)) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.cmp(right),
    }
}

/// 入れ子の値を構造で比べる（オブジェクト < 配列。規則はモジュール docs）。
fn compare_nested(left: &NestedValue, right: &NestedValue) -> Ordering {
    match (left, right) {
        (NestedValue::Object(left), NestedValue::Object(right)) => compare_entries(left, right),
        (NestedValue::Array(left), NestedValue::Array(right)) => compare_items(left, right),
        (NestedValue::Object(_), NestedValue::Array(_)) => Ordering::Less,
        (NestedValue::Array(_), NestedValue::Object(_)) => Ordering::Greater,
    }
}

/// オブジェクトのエントリ列を、キー→値の順の辞書式で比べる（前置は短い側が先）。
fn compare_entries(left: &[(String, CellValue)], right: &[(String, CellValue)]) -> Ordering {
    let common = left.len().min(right.len());
    for index in 0..common {
        let (left_key, left_value) = &left[index];
        let (right_key, right_value) = &right[index];
        let ordering = left_key
            .cmp(right_key)
            .then_with(|| compare_values(left_value, right_value));
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

/// 配列の要素列を辞書式で比べる（前置は短い側が先）。
fn compare_items(left: &[CellValue], right: &[CellValue]) -> Ordering {
    let common = left.len().min(right.len());
    for index in 0..common {
        let ordering = compare_values(&left[index], &right[index]);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

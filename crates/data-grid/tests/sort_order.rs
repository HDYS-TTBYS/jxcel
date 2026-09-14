//! 並べ替えの順序の導出（データグリッドのタスク 2.1。data-grid 要件 8.3, 8.5, 8.8）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次の 6 つである。
//!
//! 1. **比較は値の変種ごとの順序で行う**（要件 8.3）。変種をまたぐ順位の正典は
//!    `Null < Bool < Int < Float < Decimal < Text < Nested < Attachment` であり、
//!    隣り合う順位の対と、8 変種を混ぜた全順序の双方で確かめる。10 進数は**表示文字列では
//!    なく値として**比較する（`"9"` と `"10"` は文字列では `"10"` が先だが値では `9` が先）。
//!    同じ 2 つの文字列をテキストの列に置くと順序が入れ替わることを併せて示す
//!    （これが「表示文字列で比較していない」ことの観測である）。浮動小数は `-0.0` と `0.0` を
//!    同値として扱い（`CellValue` の `PartialEq` と同じ）、入れ子は**長さではなくキーを先に**
//!    見る（長さを先に見る実装と結果が食い違う対を置く）。
//! 2. **同値の行は `RowId` の順に並ぶ**（design.md `RowOrder` の Invariants）。
//!    同値になる値だけを持つ行を 4 件作り、**文書の行順を `RowId` の順と食い違わせて**から
//!    導出する。文書の順のまま出るなら（= 決着が無ければ）この検査が落ちる。
//! 3. **降順はその列の比較だけを反転する**。同値の決着は反転しない（`RowId` の昇順のまま）。
//! 4. **同一のドキュメントと同一の指定から常に同一の順序が出る**（design.md の Invariants）。
//!    手で組んだ文書と、タスク 1.4 の標本（同値の行が大量に出る列を含む）の双方で、
//!    反復した導出と、別の指定で上書きした後の再導出が一致することを見る。
//! 5. **ドキュメントを書き換える経路を持たない**（要件 8.5）。`recompute` は `&Document` を
//!    取る。このテストは**ドキュメントの共有借用を生かしたまま**呼び、呼び出しの前後で
//!    行の並びが同一であることを見る。`&mut Document` を取る形なら共有借用と衝突して
//!    コンパイルが通らない（型の上の証明はこの形でしか書けない）。
//! 6. **可視行の序数と行識別子の往復**（`row_at` / `ordinal_of` / `span`）と、その境界
//!    （空の順序、先頭・最終の序数、可視行数の外へ出る区間、文書に無いシート）。
//!
//! # 前提を先に確かめる
//!
//! 標本を使う検査は、**標本がその検査の前提を満たしていることを最初に検査する**。標本の
//! 列 0（品番）は一意制約つきであり**同値の行を 1 行も生まない**（実測: 行 64/256/512 の
//! いずれでも群はすべて 1 行）ため、同値の決着をそこで見ようとすると検査が空になる。
//! また標本の文書の行順は `RowId` の順と一致するため、そのままでは「`RowId` の順に並べた」
//! 結果と「文書の行順を保った」結果が区別できない（決着が無くても通る）。したがって
//! 同値の群を持つ列を選び、群の大きさを検査し、**行順を `RowId` の順と食い違わせてから**
//! 導出する。
//!
//! # 決定性の前提
//!
//! 標本（`tests/common/sample.rs`）の行識別子は発行時刻を含む ULID であり、組み立ての
//! たびに変わる。本ファイルは標本の**識別子を期待値に書かない**（同じ標本の 2 回の導出を
//! 突き合わせ、可視行の並びが行識別子の順列であることだけを見る）。手で組んだ文書は 1 回の
//! 組み立ての中で識別子を読み出すため、この制約に触れない。

mod common;

use common::sample::{sample, SampleOptions};
use data_grid::{ColumnIndex, RowOrder, RowOrdinal, RowSpan, SortKey, ViewSpec, ViewSummary};
use document_format::{
    AttachmentId, CellValue, Document, IdFactory, NestedValue, RowId, SheetId,
};

/// 表示の指定を短く書く（基準列と、降順かどうかの対の並び）。絞り込みは指定しない
/// （絞り込みの検査は `tests/filter_order.rs` が持つ）。
fn spec(keys: &[(usize, bool)]) -> ViewSpec {
    ViewSpec {
        sort: keys
            .iter()
            .map(|&(column, descending)| SortKey {
                column: ColumnIndex::new(column),
                descending,
            })
            .collect(),
        filters: Vec::new(),
    }
}

/// 昇順 1 本の指定。
fn ascending(column: usize) -> ViewSpec {
    spec(&[(column, false)])
}

/// 指定した値の行を持つ文書を組み立てる（列数は `columns`、行は与えられた順に並ぶ）。
///
/// 行の識別子は組み立ての中で発行されるため、この関数の戻り値を使う検査は 1 回の組み立ての
/// 中で完結する（標本の識別子を期待値に書かない規律と同じ理由）。
fn build(rows: Vec<Vec<CellValue>>, columns: usize) -> (Document, SheetId, Vec<RowId>) {
    let mut document = Document::new();
    let sheet = document.add_sheet("表");
    let names: Vec<String> = (0..columns).map(|index| format!("列{index}")).collect();
    document
        .set_sheet_columns(sheet, names)
        .expect("列名を設定できない");
    let ids = rows
        .into_iter()
        .map(|values| {
            let id = document.add_row(sheet).expect("行を追加できない");
            document
                .set_row_values(sheet, id, values)
                .expect("値を設定できない");
            id
        })
        .collect();
    (document, sheet, ids)
}

/// 可視行の並びを導出して返す。
fn visible(document: &Document, sheet: SheetId, spec: &ViewSpec) -> Vec<RowId> {
    let mut order = RowOrder::default();
    order.recompute(document, sheet, spec);
    visible_of(&order)
}

/// 順序が持っている可視行の並びを、公開面（`len` と `span`）だけを通して読み出す。
fn visible_of(order: &RowOrder) -> Vec<RowId> {
    order
        .span(RowSpan::new(RowOrdinal::new(0), order.len()))
        .to_vec()
}

/// 文書の行順の行識別子（並べ替えの前後で変わらないことを見るために使う）。
fn document_rows(document: &Document, sheet: SheetId) -> Vec<RowId> {
    document
        .sheet_by_id(sheet)
        .expect("標本のシートは文書にある")
        .rows()
        .iter()
        .map(|row| row.id())
        .collect()
}

/// 行の列の値を読む（保存された値が書き換わっていないことを見るために使う）。
fn stored_value(document: &Document, sheet: SheetId, row: RowId, column: usize) -> CellValue {
    document
        .sheet_by_id(sheet)
        .expect("標本のシートは文書にある")
        .rows()
        .iter()
        .find(|candidate| candidate.id() == row)
        .expect("行は文書にある")
        .values()
        .get(column)
        .cloned()
        .expect("列の添字は行の値の数の内側にある")
}

fn int(value: i64) -> CellValue {
    CellValue::Int(value)
}

fn float(value: f64) -> CellValue {
    CellValue::Float(value)
}

fn decimal(text: &str) -> CellValue {
    CellValue::Decimal(text.to_owned())
}

fn text(value: &str) -> CellValue {
    CellValue::Text(value.to_owned())
}

fn object(entries: Vec<(&str, CellValue)>) -> CellValue {
    CellValue::Nested(NestedValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    ))
}

fn array(items: Vec<CellValue>) -> CellValue {
    CellValue::Nested(NestedValue::Array(items))
}

/// 変種の順位の正典。**この並びが正典であり、この並びのとおりに並ぶことを検査する。**
///
/// `Null < Bool < Int < Float < Decimal < Text < Nested < Attachment`
/// （`crates/data-grid/src/view/mod.rs` のモジュール docs が唯一の源）。
fn one_of_each_variant() -> Vec<CellValue> {
    vec![
        CellValue::Null,
        CellValue::Bool(true),
        int(0),
        float(0.0),
        decimal("0"),
        text(""),
        object(Vec::new()),
        CellValue::Attachment(AttachmentId::from_bytes(b"sort-order")),
    ]
}

#[test]
fn the_variant_rank_is_the_documented_total_order() {
    let ranks = one_of_each_variant();

    // 隣り合う順位の対を、逆順に入れて確かめる（入力の順ではなく比較で並ぶこと）。
    for pair in ranks.windows(2) {
        let (document, sheet, ids) = build(vec![vec![pair[1].clone()], vec![pair[0].clone()]], 1);
        assert_eq!(
            vec![ids[1], ids[0]],
            visible(&document, sheet, &ascending(0)),
            "{:?} は {:?} より前に並ぶ（変種の順位）",
            pair[0],
            pair[1],
        );
    }

    // 8 変種を混ぜた全順序。入力は `ranks` の逆順であり、期待は `ranks` の順である。
    let rows: Vec<Vec<CellValue>> = ranks.iter().rev().cloned().map(|value| vec![value]).collect();
    let (document, sheet, ids) = build(rows, 1);
    let expected: Vec<RowId> = ranks
        .iter()
        .map(|value| {
            let position = ranks
                .iter()
                .rev()
                .position(|candidate| candidate == value)
                .expect("ranks の要素は逆順の入力に現れる");
            ids[position]
        })
        .collect();
    assert_eq!(expected, visible(&document, sheet, &ascending(0)));
}

#[test]
fn boolean_values_order_false_before_true() {
    let (document, sheet, ids) = build(vec![vec![CellValue::Bool(true)], vec![CellValue::Bool(false)]], 1);
    assert_eq!(vec![ids[1], ids[0]], visible(&document, sheet, &ascending(0)));
}

#[test]
fn integers_and_floats_compare_numerically() {
    let (document, sheet, ids) = build(vec![vec![int(10)], vec![int(-1)], vec![int(2)]], 1);
    assert_eq!(
        vec![ids[1], ids[2], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "整数は数値として比較する"
    );

    let (document, sheet, ids) = build(vec![vec![float(10.5)], vec![float(-0.5)], vec![float(2.5)]], 1);
    assert_eq!(
        vec![ids[1], ids[2], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "浮動小数は数値として比較する"
    );
}

#[test]
fn negative_zero_and_zero_are_equal_so_the_tie_break_decides() {
    // `CellValue::PartialEq` は `-0.0 == 0.0` である。比較がこの等値と食い違うと、等しいはずの
    // 2 行が同値にならず**決着が効かない**（一方が他方より小さいと判定され、順序が値ではなく
    // ビット列で決まる）。ここでは符号ビットが実際に保存を通過することを確かめたうえで、
    // 両者が同値であること（= 決着で `RowId` の順になること）を固定する。
    let (mut document, sheet, ids) = build(vec![vec![float(0.0)], vec![float(-0.0)]], 1);

    // 前提: 保存された値は符号ビットを保つ（`-0.0` が `0.0` へ潰れていない）。
    match stored_value(&document, sheet, ids[1], 0) {
        CellValue::Float(value) => assert!(
            value.is_sign_negative(),
            "前提が崩れた: -0.0 の符号ビットが保存で失われた"
        ),
        other => panic!("浮動小数である: {other:?}"),
    }

    // 同値なので決着（`RowId` の昇順）で並ぶ。文書順を逆にして、決着が無ければ落ちるようにする。
    document
        .reorder_rows(sheet, &[ids[1], ids[0]])
        .expect("行順を置き換えられない");
    assert_eq!(
        vec![ids[0], ids[1]],
        visible(&document, sheet, &ascending(0)),
        "-0.0 と 0.0 は同値であり、決着は RowId の順である"
    );
    // 降順でも決着は反転しない。
    assert_eq!(
        vec![ids[0], ids[1]],
        visible(&document, sheet, &spec(&[(0, true)])),
        "降順でも -0.0 と 0.0 の決着は RowId の順である"
    );
}

#[test]
fn text_compares_by_its_bytes_not_by_a_locale_order() {
    let (document, sheet, ids) = build(vec![vec![text("b")], vec![text("a")], vec![text("A")]], 1);
    assert_eq!(
        vec![ids[2], ids[1], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "テキストは UTF-8 のバイト列の辞書順（`A` < `a` < `b`）"
    );
}

#[test]
fn decimals_compare_as_numbers_not_as_display_strings() {
    // 値として比較するなら 9 < 10。表示文字列として比較するなら "10" < "9" になる。
    let (document, sheet, ids) = build(vec![vec![decimal("10")], vec![decimal("9")]], 1);
    assert_eq!(
        vec![ids[1], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "10 進数は数値として比較する（表示文字列ではない）"
    );

    // 同じ 2 つの文字列をテキストの列に置くと順序が入れ替わる。**これが「表示文字列ではなく
    // 値の変種ごとの順序で比較している」ことの観測である。**
    let (document, sheet, ids) = build(vec![vec![text("10")], vec![text("9")]], 1);
    assert_eq!(
        vec![ids[0], ids[1]],
        visible(&document, sheet, &ascending(0)),
        "テキストの列では 10 < 9 である"
    );

    // 桁区切りや指数表記ではなく値の大小で並ぶ。
    let (document, sheet, ids) = build(
        vec![vec![decimal("99.999")], vec![decimal("100")], vec![decimal("1e3")]],
        1,
    );
    assert_eq!(vec![ids[0], ids[1], ids[2]], visible(&document, sheet, &ascending(0)));
}

#[test]
fn decimal_spellings_of_one_value_are_equal_and_decided_by_row_id() {
    // "1.50" と "1.5" は同じ値である（正準形が 1 つ）。テキストの順序なら "1.5" が先になるが、
    // 同値であるため決着は `RowId` の昇順（先に作られた行が先）になる。
    let (document, sheet, ids) = build(vec![vec![decimal("1.50")], vec![decimal("1.5")]], 1);
    assert_eq!(
        vec![ids[0], ids[1]],
        visible(&document, sheet, &ascending(0)),
        "同じ値の書き方は同値として扱い、決着は RowId の順である"
    );

    // 保存された文字列は書き換えない（正準形は比較のためだけに作る）。
    assert_eq!(
        decimal("1.50"),
        stored_value(&document, sheet, ids[0], 0),
        "保存された 10 進数の文字列が変わった"
    );

    // 指数表記も同じ値である。
    let (document, sheet, ids) = build(vec![vec![decimal("1e1")], vec![decimal("10")]], 1);
    assert_eq!(vec![ids[0], ids[1]], visible(&document, sheet, &ascending(0)));
}

#[test]
fn decimals_outside_the_grammar_sort_after_the_numbers_by_their_bytes() {
    // 文法に一致しない値（上流の脱出口で書かれた値）は文法に一致する値の後ろに置き、
    // その中ではバイト列の辞書順で並べる（`1` = 0x31 < `a` = 0x61）。
    let (document, sheet, ids) = build(
        vec![
            vec![decimal("10")],
            vec![decimal("abc")],
            vec![decimal("1.2.3")],
            vec![decimal("9")],
            vec![decimal("-3")],
        ],
        1,
    );
    assert_eq!(
        vec![ids[4], ids[3], ids[0], ids[2], ids[1]],
        visible(&document, sheet, &ascending(0)),
    );
}

#[test]
fn integers_floats_and_decimals_keep_their_variant_rank() {
    // 変種が違えば数値として等しくても**同値ではない**。順位が先に効くため
    // `Int(5) < Float(5.0) < Decimal("5")` の順に並ぶ（同値の決着は起こらない）。
    let (document, sheet, ids) = build(vec![vec![decimal("5")], vec![float(5.0)], vec![int(5)]], 1);
    assert_eq!(
        vec![ids[2], ids[1], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "Int < Float < Decimal（変種をまたいで数値として比較しない）"
    );
}

#[test]
fn nested_values_compare_by_their_structure() {
    // オブジェクトが配列より前。
    let (document, sheet, ids) = build(vec![vec![array(vec![int(1)])], vec![object(Vec::new())]], 1);
    assert_eq!(vec![ids[1], ids[0]], visible(&document, sheet, &ascending(0)));

    // オブジェクトはキー、次に値の順。
    let (document, sheet, ids) = build(
        vec![
            vec![object(vec![("b", int(1))])],
            vec![object(vec![("a", int(9))])],
        ],
        1,
    );
    assert_eq!(
        vec![ids[1], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "キーのバイト列の順で決まる"
    );

    let (document, sheet, ids) = build(
        vec![
            vec![object(vec![("a", int(2))])],
            vec![object(vec![("a", int(1))])],
        ],
        1,
    );
    assert_eq!(
        vec![ids[1], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "キーが同じなら値で決まる"
    );

    // 片方が他方の前置なら短い側が先（`Vec` の `Ord` と同じ辞書式の規約）。
    let (document, sheet, ids) = build(
        vec![
            vec![object(vec![("a", int(1)), ("b", int(2))])],
            vec![object(vec![("a", int(1))])],
        ],
        1,
    );
    assert_eq!(vec![ids[1], ids[0]], visible(&document, sheet, &ascending(0)));

    // **長さが違い、かつ先頭のキーも違う**対。ここが「長さを先に見る」実装と
    // 「キー→値の順に見る」実装が食い違う点である。
    // `{"b":1}` と `{"a":1,"b":2}` は長さが違う（1 対 2）が、先頭のキーは `"b"` 対 `"a"` であり、
    // キーを先に見る規約では `"a"` を持つ長い側が先になる（長さを先に見ると短い側が先になり、
    // 結果が逆になる）。キー列を持たない行（入れ子の値そのもの）を相手にする場合も同じである。
    let (document, sheet, ids) = build(
        vec![
            vec![object(vec![("b", int(1))])],
            vec![object(vec![("a", int(1)), ("b", int(2))])],
        ],
        1,
    );
    assert_eq!(
        vec![ids[1], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "キーを先に見る（長さでは決まらない）"
    );

    // 配列も要素の順に突き合わせ、前置は短い側が先。
    let (document, sheet, ids) = build(
        vec![vec![array(vec![int(2)])], vec![array(vec![int(1), int(9)])]],
        1,
    );
    assert_eq!(vec![ids[1], ids[0]], visible(&document, sheet, &ascending(0)));
}

#[test]
fn attachments_compare_by_their_digest_bytes() {
    // 添付は内容アドレスの識別子であり、その `Ord`（ダイジェストのバイト列順）で並ぶ。
    // 期待値は識別子自身の順序から作る（固定幅の小文字 hex はバイト列の順序と一致する）。
    let digests: Vec<AttachmentId> = (0..8u8).map(|byte| AttachmentId::from_bytes(&[byte])).collect();
    let rows: Vec<Vec<CellValue>> = digests
        .iter()
        .rev()
        .map(|id| vec![CellValue::Attachment(*id)])
        .collect();
    let (document, sheet, ids) = build(rows, 1);
    let mut pairs: Vec<(AttachmentId, RowId)> =
        digests.iter().copied().zip(ids.iter().rev().copied()).collect();
    pairs.sort_by_key(|(id, _)| *id);
    let expected: Vec<RowId> = pairs.into_iter().map(|(_, row)| row).collect();
    assert_eq!(expected, visible(&document, sheet, &ascending(0)));
}

#[test]
fn equal_key_values_are_ordered_by_row_id() {
    let (mut document, sheet, ids) = build(vec![vec![text("same")]; 4], 1);
    // 文書の行順を `RowId` の順と食い違わせる。決着（`RowId` の昇順）が無ければ、
    // 可視の並びは文書の行順のままになり、この検査が落ちる。
    document
        .reorder_rows(sheet, &[ids[2], ids[0], ids[3], ids[1]])
        .expect("行順を置き換えられない");
    assert_eq!(
        vec![ids[0], ids[1], ids[2], ids[3]],
        visible(&document, sheet, &ascending(0)),
        "同値の行は RowId の順である"
    );
}

#[test]
fn descending_reverses_only_that_key_and_keeps_the_row_id_deciding() {
    let (document, sheet, ids) = build(vec![vec![int(1)], vec![int(3)], vec![int(2)]], 1);
    assert_eq!(
        vec![ids[0], ids[2], ids[1]],
        visible(&document, sheet, &ascending(0))
    );
    assert_eq!(
        vec![ids[1], ids[2], ids[0]],
        visible(&document, sheet, &spec(&[(0, true)])),
        "降順はその列の比較だけを反転する"
    );

    // 同値の行の決着は降順でも `RowId` の昇順のまま（反転しない）。
    let (mut document, sheet, ids) = build(vec![vec![text("same")]; 4], 1);
    document
        .reorder_rows(sheet, &[ids[2], ids[0], ids[3], ids[1]])
        .expect("行順を置き換えられない");
    assert_eq!(
        vec![ids[0], ids[1], ids[2], ids[3]],
        visible(&document, sheet, &spec(&[(0, true)])),
        "降順でも同値の行は RowId の昇順である"
    );

    // 複数の基準列のうち、降順にするのは指定された列だけである。
    let (document, sheet, ids) = build(
        vec![
            vec![text("a"), int(1)],
            vec![text("a"), int(2)],
            vec![text("b"), int(1)],
        ],
        2,
    );
    assert_eq!(
        vec![ids[1], ids[0], ids[2]],
        visible(&document, sheet, &spec(&[(0, false), (1, true)])),
        "第一の基準は昇順、第二の基準は降順"
    );
}

#[test]
fn multiple_keys_are_applied_from_the_first() {
    let (document, sheet, ids) = build(
        vec![
            vec![text("b"), int(1)],
            vec![text("a"), int(2)],
            vec![text("a"), int(1)],
        ],
        2,
    );
    assert_eq!(
        vec![ids[2], ids[1], ids[0]],
        visible(&document, sheet, &spec(&[(0, false), (1, false)])),
        "第一の基準が先に効く"
    );
    assert_eq!(
        vec![ids[2], ids[0], ids[1]],
        visible(&document, sheet, &spec(&[(1, false), (0, false)])),
        "基準の順を入れ替えると結果が変わる（第二の基準は第一が同値のときだけ効く）"
    );
}

#[test]
fn an_empty_sort_keeps_the_document_row_order() {
    // 基準列が 0 本のときは比較を行わないため、文書の行順がそのまま可視の順になる
    // （`RowId` の順ではない。`Document::reorder_rows` の後では両者は食い違う）。
    let (mut document, sheet, ids) = build(vec![vec![text("b")], vec![text("a")], vec![text("c")]], 1);
    document
        .reorder_rows(sheet, &[ids[2], ids[1], ids[0]])
        .expect("行順を置き換えられない");
    assert_eq!(
        vec![ids[2], ids[1], ids[0]],
        visible(&document, sheet, &ViewSpec::default()),
        "基準列が無いときは文書の行順を保つ"
    );
    assert_eq!(
        vec![ids[1], ids[0], ids[2]],
        visible(&document, sheet, &ascending(0)),
        "基準列があれば値の順に並ぶ"
    );
}

#[test]
fn missing_values_are_treated_as_the_absence_of_a_value() {
    // 値の数が列の添字に届かない行は値なしとして比較する（最も小さい側に並ぶ）。
    let (document, sheet, ids) = build(vec![vec![int(1)], Vec::new()], 1);
    assert_eq!(
        vec![ids[1], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "値を持たない行は値なしとして最も小さい側に並ぶ"
    );

    // 明示の値なしと同じ鍵になる（同値なので決着は `RowId` の順）。
    let (document, sheet, ids) = build(vec![vec![CellValue::Null], Vec::new()], 1);
    assert_eq!(
        vec![ids[0], ids[1]],
        visible(&document, sheet, &ascending(0)),
        "値なしと欠落は同じ鍵であり、決着は RowId の順になる"
    );

    // **欠落を別の番兵へ取り違えないことを固定する。** 上の 2 行だけでは足りない —
    // `Null` が最も小さいため、欠落を `Bool(false)` や `Int(0)` に置き換えても並びは
    // 変わらない（どちらでも「値なしの側が先」になる）。番兵を区別できるのは、
    // **番兵の候補そのものと突き合わせたとき**だけである:
    // 欠落の鍵が `Null` なら `Null < Bool(false)` なので**欠落の行が先**、
    // 欠落の鍵が `Bool(false)` なら同値になって決着（`RowId` の順）に落ち、
    // 先に作った行（`Bool(false)` の行）が先に来て**結果が入れ替わる**。
    // 相手の行を先に作り、文書順も `RowId` の順のままにするので、決着に落ちれば必ず落ちる。
    let (document, sheet, ids) = build(vec![vec![CellValue::Bool(false)], Vec::new()], 1);
    assert_eq!(
        vec![ids[1], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "欠落は値なしとして偽より前に並ぶ（偽を番兵にしていない）"
    );

    let (document, sheet, ids) = build(vec![vec![int(0)], Vec::new()], 1);
    assert_eq!(
        vec![ids[1], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "欠落は値なしとして整数 0 より前に並ぶ（0 を番兵にしていない）"
    );

    let (document, sheet, ids) = build(vec![vec![text("")], Vec::new()], 1);
    assert_eq!(
        vec![ids[1], ids[0]],
        visible(&document, sheet, &ascending(0)),
        "欠落は値なしとして空文字より前に並ぶ（空文字を番兵にしていない）"
    );

    // 列の添字が行の値の数を超える場合も同じ。
    let (document, sheet, ids) = build(vec![vec![int(1), int(2)], vec![int(3)]], 2);
    assert_eq!(vec![ids[1], ids[0]], visible(&document, sheet, &ascending(1)));
}

#[test]
fn repeated_derivations_from_the_same_input_are_identical() {
    let (document, sheet, _) = build(
        vec![
            vec![text("a"), int(1)],
            vec![text("a"), int(1)],
            vec![text("b"), int(0)],
            vec![text("a"), int(2)],
        ],
        2,
    );
    let two_keys = spec(&[(0, false), (1, true)]);

    let first = visible(&document, sheet, &two_keys);
    let second = visible(&document, sheet, &two_keys);
    assert_eq!(first, second, "同一の入力から同一の順序が出ない");

    // 別の指定で上書きした後に戻しても同じ（順序は呼び出しの履歴に依らない）。
    let mut order = RowOrder::default();
    order.recompute(&document, sheet, &ViewSpec::default());
    order.recompute(&document, sheet, &two_keys);
    assert_eq!(first, visible_of(&order));

    // 標本でも確かめる（列 4 は真偽であり、同値の行が大量に出る）。
    let sample = sample(&SampleOptions::new(512, 5));
    let sample_spec = spec(&[(1, false), (4, false)]);
    let mut left = RowOrder::default();
    let mut right = RowOrder::default();
    let left_summary = left.recompute(sample.document(), sample.sheet(), &sample_spec);
    let right_summary = right.recompute(sample.document(), sample.sheet(), &sample_spec);
    assert_eq!(left_summary, right_summary, "要約が一致しない");
    assert_eq!(visible_of(&left), visible_of(&right), "順序が一致しない");

    // 絞り込みを指定していないため、可視行はシートの行数と一致し、隠れた行は 0 である。
    assert_eq!(sample.rows(), left_summary.visible);
    assert_eq!(0, left_summary.hidden);
    assert_eq!(sample.rows(), left.len());
    assert_eq!(0, left.hidden());

    // 可視の並びは同じ行の順列である（同じ行が 2 度現れず、行が落ちない）。
    let mut sorted = visible_of(&left);
    sorted.sort();
    let mut expected: Vec<RowId> = sample.row_ids().to_vec();
    expected.sort();
    assert_eq!(expected, sorted);
}

#[test]
fn the_order_derivation_only_borrows_the_document() {
    let (document, sheet, ids) = build(vec![vec![int(3)], vec![int(1)], vec![int(2)]], 1);
    let before = document_rows(&document, sheet);

    // ドキュメントの**共有借用を生かしたまま** `recompute` を呼ぶ。`&mut Document` を取る形なら
    // ここで借用が衝突し、このテストはコンパイルに失敗する（要件 8.5 の型の上の証明）。
    let sheet_ref = document.sheet_by_id(sheet).expect("標本のシートは文書にある");
    let mut order = RowOrder::default();
    let summary = recompute_order(&mut order, &document, sheet, &ascending(0));

    // 呼び出しの後も、同じ共有借用でドキュメントを読める。
    assert_eq!(summary.visible, sheet_ref.rows().len());
    let after: Vec<RowId> = sheet_ref.rows().iter().map(|row| row.id()).collect();
    assert_eq!(before, after, "ドキュメントの行の並びが書き換わった");
    assert_eq!(
        before,
        document_rows(&document, sheet),
        "ドキュメントの行の並びが呼び出しの前後で変わった"
    );
    // 可視の並びは値の順であり、行そのものの順ではない（書き換える経路が無いことの観測）。
    assert_eq!(vec![ids[1], ids[2], ids[0]], visible_of(&order));
}

/// ドキュメントを共有参照で受け取り、順序だけを可変参照で受け取る。
///
/// この形が要件 8.5（「可変参照を受け取らない形で型の上で示す」）の実体である:
/// 順序は可変参照で受け取り、**ドキュメントは共有参照でしか受け取らない**。
fn recompute_order(
    order: &mut RowOrder,
    document: &Document,
    sheet: SheetId,
    spec: &ViewSpec,
) -> ViewSummary {
    order.recompute(document, sheet, spec)
}

#[test]
fn row_at_ordinal_of_and_span_round_trip_and_respect_their_bounds() {
    let (document, sheet, ids) = build(vec![vec![int(3)], vec![int(1)], vec![int(2)]], 1);
    let mut order = RowOrder::default();
    let summary = order.recompute(&document, sheet, &ascending(0));
    assert_eq!(ViewSummary { visible: 3, hidden: 0 }, summary);
    assert_eq!(3, order.len());
    assert!(!order.is_empty());

    // 可視の並びは値の順（1, 2, 3）であり、行識別子で引ける。
    let expected = vec![ids[1], ids[2], ids[0]];
    for (position, row) in expected.iter().enumerate() {
        let ordinal = RowOrdinal::new(position);
        assert_eq!(Some(*row), order.row_at(ordinal), "序数 {position}");
        assert_eq!(Some(ordinal), order.ordinal_of(*row), "行 {row}");
    }

    // 端の外は `None`。
    assert_eq!(None, order.row_at(RowOrdinal::new(3)));
    assert_eq!(None, order.row_at(RowOrdinal::new(usize::MAX)));

    // 区間は半開であり、可視行の並びの交わりを返す。
    assert_eq!(expected.clone(), visible_of(&order));
    assert_eq!(
        vec![ids[1], ids[2]],
        order.span(RowSpan::new(RowOrdinal::new(0), 2)).to_vec()
    );
    assert_eq!(
        vec![ids[2]],
        order.span(RowSpan::new(RowOrdinal::new(1), 1)).to_vec()
    );
    assert_eq!(
        Vec::<RowId>::new(),
        order.span(RowSpan::new(RowOrdinal::new(1), 0)).to_vec(),
        "行数 0 の区間は空である"
    );
    // 可視行数の外へ出る要求は切り落とす（末尾の窓が短くなる）。
    assert_eq!(
        vec![ids[0]],
        order.span(RowSpan::new(RowOrdinal::new(2), 5)).to_vec()
    );
    assert!(order.span(RowSpan::new(RowOrdinal::new(3), 5)).is_empty());
    assert!(order.span(RowSpan::new(RowOrdinal::new(99), 1)).is_empty());
    assert!(order.span(RowSpan::new(RowOrdinal::new(0), 0)).is_empty());
}

#[test]
fn an_empty_order_has_no_rows_and_no_lookups() {
    let (document, sheet, ids) = build(vec![vec![int(1)]], 1);

    // 一度も導出していない順序。
    let empty = RowOrder::default();
    assert_eq!(0, empty.len());
    assert!(empty.is_empty());
    assert_eq!(0, empty.hidden());
    assert_eq!(None, empty.row_at(RowOrdinal::new(0)));
    assert_eq!(None, empty.ordinal_of(ids[0]));
    assert!(empty.span(RowSpan::new(RowOrdinal::new(0), 3)).is_empty());

    // 行を持たないシートを導出した順序。
    let (bare, bare_sheet, _) = build(Vec::new(), 1);
    let mut order = RowOrder::default();
    let summary = order.recompute(&bare, bare_sheet, &ascending(0));
    assert_eq!(ViewSummary { visible: 0, hidden: 0 }, summary);
    assert_eq!(0, order.len());
    assert!(order.is_empty());
    assert_eq!(None, order.row_at(RowOrdinal::new(0)));
    assert!(order.span(RowSpan::new(RowOrdinal::new(0), 1)).is_empty());

    // 可視でない行（他のシート・他の文書の行）は引けない。
    let mut order = RowOrder::default();
    order.recompute(&document, sheet, &ascending(0));
    let mut factory = IdFactory::new();
    let unknown_sheet = factory.new_sheet_id();
    let mut other = Document::new();
    let other_sheet = other.add_sheet("別表");
    let other_row = other.add_row(other_sheet).expect("行を追加できない");
    assert_eq!(None, order.ordinal_of(other_row));
    let summary = order.recompute(&other, unknown_sheet, &ascending(0));
    assert_eq!(
        ViewSummary { visible: 0, hidden: 0 },
        summary,
        "文書に無いシートは行が 1 件も無いものとして扱う"
    );
    assert!(order.is_empty());
}

#[test]
fn a_view_spec_without_keys_is_a_valid_derivation() {
    // 表示の指定は並べ替えの基準列を持たない形も取れる（絞り込みだけを指定する 2.2 の前提）。
    let (document, sheet, ids) = build(vec![vec![int(2)], vec![int(1)]], 1);
    let spec = ViewSpec {
        sort: Vec::new(),
        filters: Vec::new(),
    };
    assert_eq!(ids, visible(&document, sheet, &spec), "文書の行順のまま");
    assert!(spec.sort.is_empty());
    // 絞り込みを指定していないため隠れた行は無い。
    let mut order = RowOrder::default();
    let summary = order.recompute(&document, sheet, &spec);
    assert_eq!(0, summary.hidden);
    assert_eq!(0, order.hidden());
    assert_eq!(
        ViewSummary { visible: 2, hidden: 0 },
        summary,
        "行数と可視行数の差が隠れた行数である"
    );
}

/// 標本の値を、**同値の行を多数含む列**で並べ替える。
///
/// # 前提を先に確かめる
///
/// 「標本は同値の行を含む」ことを**思い込まない**。標本の列 0（品番）は一意制約つきの列であり、
/// 同値の行を 1 行も生まない（実測: 行 64/256/512 のいずれでも列 0 の群はすべて 1 行）。
/// 同値の決着をそこで見ようとすると検査が空になる。したがって同値の群を持つ列を選び、
/// **その前提が成り立っていることを最初に検査する**。
///
/// # 決着が効いていることをどう観測するか
///
/// 標本の文書の行順は `RowId` の順と一致する（行は先頭から順に作られるため。実測で確認）。
/// そのままでは「`RowId` の順に並べた」結果と「文書の行順を保った」結果が同じになり、
/// 決着が無くても検査が通る。そこで標本の**値を写した自分の文書**を組み、行順を
/// `RowId` の順と食い違わせて（逆順に）から導出する。決着が無ければ可視の並びは逆順のままに
/// なり、この検査が落ちる。
///
/// # 列の選び方
///
/// 違反の割合を 0 にして（[`SampleOptions::with_ratio`]）全セルを適合させる。こうすると列の
/// 値が 1 つの変種に揃うため、期待値を**変種の規則を書き写さずに**書ける（列 4 `検査済み` は
/// 真偽であり、期待は「偽の群（`RowId` 昇順）の後に真の群（`RowId` 昇順）」だけである）。
#[test]
fn the_shared_sample_sorts_by_the_values_of_a_column() {
    // 違反を混ぜない（列 4 が真偽だけになる）。
    let sample = sample(&SampleOptions::new(512, 5).with_ratio(0.0));
    let column = 4usize;
    let sheet = sample
        .document()
        .sheet_by_id(sample.sheet())
        .expect("標本のシートは文書にある");

    // 標本の値を写して自分の文書を組む（`Sample` は文書を可変で貸さないため、行順を
    // `RowId` の順と食い違わせるには自分で組む必要がある）。
    let values: Vec<Vec<CellValue>> = sheet.rows().iter().map(|row| row.values().to_vec()).collect();
    let (mut document, sheet, ids) = build(values, sample.column_count());

    // 前提: 列 4 は真偽であり、偽と真の双方が複数行ずつ現れる（同値の群が実在する）。
    let mut falses: Vec<RowId> = Vec::new();
    let mut trues: Vec<RowId> = Vec::new();
    for (id, row) in ids.iter().zip(document.sheet_by_id(sheet).expect("シート").rows()) {
        match row.values().get(column) {
            Some(CellValue::Bool(false)) => falses.push(*id),
            Some(CellValue::Bool(true)) => trues.push(*id),
            other => panic!("違反を混ぜていないため検査済みは真偽である: {other:?}"),
        }
    }
    assert!(
        falses.len() >= 2 && trues.len() >= 2,
        "前提が崩れた: 列 {column} の同値の群が小さい（偽 {} 行・真 {} 行）— 決着を検査できない",
        falses.len(),
        trues.len()
    );

    // 行順を `RowId` の順と食い違わせる（逆順にする）。これが無いと「決着が効いている」ことと
    // 「文書の行順を保っている」ことが区別できない。
    let mut reversed = ids.clone();
    reversed.reverse();
    document
        .reorder_rows(sheet, &reversed)
        .expect("行順を置き換えられない");

    let mut order = RowOrder::default();
    order.recompute(&document, sheet, &ascending(column));
    let visible = visible_of(&order);
    assert_eq!(ids.len(), visible.len());

    // 期待: 偽の群の後に真の群であり、各群の中は `RowId` の昇順である。群を明示的に
    // 並べ替えて期待を作る（`build` が発行する識別子の順に依存しないため）。
    falses.sort();
    trues.sort();
    let expected: Vec<RowId> = falses.into_iter().chain(trues).collect();
    assert_eq!(
        expected, visible,
        "同値の群の中が RowId の順でない（文書の行順を写しただけになっている）"
    );
    // 期待は文書の行順（逆順にしたもの）とは異なる。この不一致があるから、決着が無ければ
    // この検査が落ちる（前提の `falses.len() >= 2 && trues.len() >= 2` がそれを保証する）。
    assert_ne!(reversed, expected, "文書の行順と決着の結果が一致してしまった");

    // 決着は文書の行順ではなく `RowId` にだけ依る。行順をもう一度別の並び（今度は
    // 先頭と末尾を入れ替えた巡回）にしても、同じ順序が出る。
    let mut rotated = reversed.clone();
    rotated.rotate_left(1);
    document
        .reorder_rows(sheet, &rotated)
        .expect("行順を置き換えられない");
    order.recompute(&document, sheet, &ascending(column));
    assert_eq!(
        expected,
        visible_of(&order),
        "文書の行順を変えても同じ順序が出ない（決着が RowId に依っていない）"
    );

    // 絞り込みを指定していないため隠れた行は無い。
    assert_eq!(0, order.hidden());
}

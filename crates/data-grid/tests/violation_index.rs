//! 可視行の序数に対する違反の索引（データグリッドのタスク 2.4。data-grid 要件 4.1, 4.3, 4.4,
//! 4.5）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のものである。
//!
//! 1. **違反の総数**（要件 4.3）。索引はシートの検証結果が報告した違反の総数を保持する。
//!    絞り込みで隠れた行の違反も、行に属さない列そのものの問題も数に入る（総数の意味は
//!    `view/violations.rs` のモジュール docs「違反の総数」が正典である）。
//! 2. **表示範囲の外にある違反が探索で見つかる**（要件 4.4）。描画の窓（[`RowSpan`]）の
//!    外にある違反行へ [`ViolationIndex::find`] が到達する。窓の中身を走査するのではなく、
//!    索引そのものを走査していることを、窓の行に違反が 1 件も無いことの表明と組にして見る。
//! 3. **前方・後方の意味**（[`SearchDirection`]）。`find` は**指定した位置を含む**側から
//!    最も近い違反セルを返し、その向きに違反が無ければ `None` を返す。可視の最初と最後の
//!    序数、違反の真上、可視行数の外をそれぞれ固定する。
//! 4. **入れ子の内側の位置**（要件 4.5）。入れ子の違反は内側の位置（[`NestedPath`]）を
//!    保ったまま索引に載る。1 つのセルに複数の内側の位置があれば、そのすべてが載る。
//! 5. **順序や絞り込みが変わったときの鍵の張り直し**（タスク 2.4）。再計算の前後で序数が
//!    指す行が入れ替わることを、**前後の双方**で観測する（片側だけでは「鍵が動いた」ことの
//!    証拠にならない）。
//! 6. **可視でない行の違反**（絞り込みで隠れた行）。総数には数え、序数の索引には載せない
//!    （その行に序数が存在しないため）。据え付け（[`ViolationPresence`]）には載る
//!    — 絞り込みの判定は「その行の違反」を読むのであり、現在の可視の集合を読むのではない。
//! 7. **列そのものの問題**（`Violation::row` が `None`）。総数には数え、序数も行も無いため
//!    探索でも据え付けでも届かない。別に保持する（[`ViolationIndex::column_violations`]）。
//! 8. **据え付けの端から端**（2.2 と 2.4 の継ぎ目）。索引が作った据え付けを `RowOrder` へ
//!    据え付けると、[`FilterSpec::HasViolation`] がちょうど期待どおりの行を選ぶ。
//! 9. **決定性**。同じ報告と同じ順序からは常に同じ索引が出る。標本の組み立てが違っても
//!    （識別子が違っても）**構造**は同じである。
//! 10. **保持が上限で切られた報告**。総数は切られず、索引に載るのは保持された分だけである
//!     （[`ViolationIndex::is_complete`] がその差を観測できる）。
//!
//! # 前提を先に確かめる
//!
//! 標本（`tests/common/sample.rs`）を使う検査は、**標本がその検査の前提を満たしていることを
//! 最初に検査する**（tasks.md の Implementation Notes の規則）。本ファイルは
//! 「どの行のどの列が違反か」を**標本自身の申告**（`violation_at`）と全件検証の報告の双方から
//! 確かめてから依拠し、期待値を手書きの写しにしない。違反を 1 件も含まない行・列についても
//! 同じである（「違反が無い」という前提の上に `None` の検査を載せている）。
//!
//! **標本の識別子は発行のたびに変わる**ため、期待値に生の識別子を書かない。標本が公開する
//! 行の並び（[`Sample::row_ids`]）と突き合わせ、比較するのは**構造**（序数が指す行の位置・
//! 列の添字・入れ子の位置）である（tasks.md の Implementation Notes の規則）。
//!
//! # 絞り込みの指定と、列 0 の性質
//!
//! 標本の列 0 は一意制約つきのキー（品番）であり、適合する値は `P` で始まり、違反する値は
//! `!` で始まる。したがって「列 0 が `P` を含む」という絞り込みは、**列 0 の違反を仕込んだ行
//! だけを隠す**（本ファイルはこの性質を、隠れた行の集合が標本の申告する違反行の集合と一致する
//! ことで確かめてから使う）。

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::sample::{sample, Sample, SampleOptions};
use data_grid::{
    CellAddress, ColumnIndex, FilterSpec, NestedPathSegment, RowOrder, RowOrdinal, RowSpan,
    SearchDirection, SortKey, ViewSpec, ViolationIndex,
};
use document_format::{CellValue, RowId};
use schema_engine::compile::plan::ColumnValidator;
use schema_engine::validate::report::ViolationReport;
use schema_engine::{
    Expected, SchemaEngine, SchemaEngineApi, SheetReport, ValidationOptions, ValuePath,
    ValuePathSegment, Violation, ViolationReason,
};

// ---------------------------------------------------------------------------
// 標本と報告を組み立てる補助
// ---------------------------------------------------------------------------

/// 標本を組み立て、その全件検証の報告とともに返す。
///
/// 検証は上限を設けずに走らせる（保持が切れた報告は
/// [`a_capped_report_keeps_the_total_but_not_the_index`] が別に組み立てる）。
fn validated(rows: usize, columns: usize, ratio: f64) -> (Sample, SheetReport) {
    let sample = sample(&SampleOptions::new(rows, columns).with_ratio(ratio));
    let report = SchemaEngine::new().validate_sheet(
        sample.document(),
        sample.sheet(),
        &sample.compiled(),
        &ValidationOptions::default(),
    );
    (sample, report)
}

/// 標本が違反を仕込んだセル（行添字, 列添字）の集合。標本自身の申告から集める。
fn scheduled(sample: &Sample) -> BTreeSet<(usize, usize)> {
    let mut out = BTreeSet::new();
    for row in 0..sample.rows() {
        for column in 0..sample.column_count() {
            if sample.violation_at(row, column) {
                out.insert((row, column));
            }
        }
    }
    out
}

/// 違反を持つ行の添字（昇順）。
fn violating_row_indices(sample: &Sample) -> Vec<usize> {
    let mut rows: Vec<usize> = scheduled(sample).into_iter().map(|(row, _)| row).collect();
    rows.dedup();
    rows
}

/// 違反を持つ行の識別子（昇順）。
fn violating_row_ids(sample: &Sample) -> BTreeSet<RowId> {
    violating_row_indices(sample)
        .into_iter()
        .map(|row| sample.row_ids()[row])
        .collect()
}

/// ある行添字の行識別子。
fn row_id(sample: &Sample, row: usize) -> RowId {
    sample.row_ids()[row]
}

/// 標本の行添字と列添字のセルの位置（物理の同一性）。
fn address(sample: &Sample, row: usize, column: usize) -> CellAddress {
    CellAddress::new(row_id(sample, row), ColumnIndex::new(column))
}

/// 標本の違反セルを（行添字, 列添字）の昇順に並べる（最初の違反の期待値を導く）。
fn first_scheduled(sample: &Sample) -> (usize, usize) {
    let cell = scheduled(sample)
        .into_iter()
        .next()
        .expect("標本は違反を 1 件以上仕込んでいる");
    cell
}

/// 絞り込みも並べ替えも指定しない表示の指定。
fn no_view() -> ViewSpec {
    ViewSpec {
        sort: Vec::new(),
        filters: Vec::new(),
    }
}

/// 列 1 本の並べ替えの指定。
fn sorted(column: usize, descending: bool) -> ViewSpec {
    ViewSpec {
        sort: vec![SortKey {
            column: ColumnIndex::new(column),
            descending,
        }],
        filters: Vec::new(),
    }
}

/// 列 0 の表示文字列に `text` を含む行だけを通す絞り込み。
fn contains(column: usize, text: &str) -> ViewSpec {
    ViewSpec {
        sort: Vec::new(),
        filters: vec![FilterSpec::Contains {
            column: ColumnIndex::new(column),
            text: text.to_owned(),
        }],
    }
}

/// 可視の順序を先頭から走査し、違反を持つ最初の序数を求める（期待値を順序そのものから導く）。
fn first_violating_ordinal(order: &RowOrder, violating: &BTreeSet<RowId>) -> Option<RowOrdinal> {
    (0..order.len())
        .map(RowOrdinal::new)
        .find(|ordinal| order.row_at(*ordinal).is_some_and(|row| violating.contains(&row)))
}

/// 可視の順序を末尾から走査し、違反を持つ最後の序数を求める。
fn last_violating_ordinal(order: &RowOrder, violating: &BTreeSet<RowId>) -> Option<RowOrdinal> {
    (0..order.len())
        .rev()
        .map(RowOrdinal::new)
        .find(|ordinal| order.row_at(*ordinal).is_some_and(|row| violating.contains(&row)))
}

/// 索引に載っている違反の件数（可視行の序数と、行に属さない列の問題をすべて数える）。
fn indexed_violations(index: &ViolationIndex, order: &RowOrder) -> usize {
    let mut count = 0;
    for pointer in 0..order.len() {
        if let Some(row) = index.row_violations(RowOrdinal::new(pointer)) {
            count += row.columns().iter().map(|cell| cell.paths().len()).sum::<usize>();
        }
    }
    count + index
        .column_violations()
        .iter()
        .map(|column| column.paths().len())
        .sum::<usize>()
}

/// 組み立てた索引と、可視の順序（絞り込みも並べ替えも無し）を組にして返す。
///
/// 絞り込みも並べ替えも無い順序は**文書の行順**であり、標本の行の並び（[`Sample::row_ids`]）
/// と一致する。この一致を前提として確かめてから、行添字を序数として使う検査へ進む。
fn indexed(sample: &Sample, report: &SheetReport) -> (RowOrder, ViolationIndex) {
    let mut order = RowOrder::default();
    order.recompute(sample.document(), sample.sheet(), &no_view());
    let index = ViolationIndex::build(report, &order);
    (order, index)
}

/// 絞り込みも並べ替えも無い順序が、標本の行の並びそのものであることを確かめる。
fn assert_order_is_the_document_order(sample: &Sample, order: &RowOrder) {
    assert_eq!(
        sample.rows(),
        order.len(),
        "絞り込みも並べ替えも無い可視行数が標本の行数と違う"
    );
    for row in 0..sample.rows() {
        assert_eq!(
            Some(row_id(sample, row)),
            order.row_at(RowOrdinal::new(row)),
            "可視の序数 {row} が標本の行 {row} を指していない"
        );
    }
}

// ---------------------------------------------------------------------------
// 1. 総数（要件 4.3）
// ---------------------------------------------------------------------------

/// 索引は、報告が数えた違反の総数をそのまま保持する。
///
/// 併せて、索引に載る違反の件数が報告の保持した違反と一致すること（取りこぼしも重複も無い）
/// と、報告の違反が標本の申告する位置と一致することを確かめる（位置の期待値を標本から導く）。
#[test]
fn the_index_totals_every_violation_the_report_found() {
    let (sample, report) = validated(2_000, 1, 0.001);
    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);

    // 前提: 標本は違反を 2 件仕込んでおり、全件検証がそれを報告している。
    let cell = first_scheduled(&sample);
    assert_eq!(
        2,
        scheduled(&sample).len(),
        "標本の違反の件数が 2 でない（前提が崩れている）"
    );
    assert_eq!(
        scheduled(&sample).len(),
        report.total_violations(),
        "報告の総件数が仕込んだ件数と違う"
    );
    assert!(!report.is_truncated(), "報告の保持が上限で切れている");

    // 総数は報告の総件数である（標本の申告とも一致する）。
    assert_eq!(2, index.violation_total(), "索引の総数が報告と違う");
    assert_eq!(
        report.total_violations(),
        index.violation_total(),
        "索引の総数が報告の違反の総件数と違う"
    );
    // 索引は保持された違反を取りこぼさない（報告の保持が無制限なら件数は総数に一致する）。
    assert!(index.is_complete(), "取りこぼしがある");
    assert_eq!(
        report.violations().len(),
        indexed_violations(&index, &order),
        "索引に載った違反の件数が報告の保持した件数と違う"
    );
    // シート全体の検証は行に属さない違反を報告しない（前提。列そのものの問題は
    // 「列そのものの問題」の検査が手で組み立てて確かめる）。
    assert!(
        report.violations().iter().all(|it| it.row().is_some()),
        "シート全体の検証が行に属さない違反を報告している"
    );
    assert!(index.column_violations().is_empty(), "列の問題が載っている");

    // 違反の位置は標本の申告したセルである。
    let found = index
        .find(RowOrdinal::new(0), SearchDirection::Forward)
        .expect("違反が見つからない");
    assert_eq!(address(&sample, cell.0, cell.1), found, "最初の違反が違う");
}

// ---------------------------------------------------------------------------
// 2. 表示範囲の外にある違反（要件 4.4）
// ---------------------------------------------------------------------------

/// 描画の窓の外にある違反へ探索が到達する。索引は窓ではなく索引を走査する。
///
/// 標本は 2,000 行のうち 2 行（行 999 と行 1999）に違反を持つ。窓を先頭 100 行に切ると、
/// 違反は窓の外にある。窓の中身（[`RowOrder::span`] が返す行）に違反が 1 件も無いことを
/// 確かめてから、`find` が窓の外の違反へ到達することを見る。
#[test]
fn search_reaches_a_violation_outside_the_displayed_range() {
    let (sample, report) = validated(2_000, 1, 0.001);
    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);

    // 前提: 違反は 1 列目（品番）の 2 行にあり、どちらも先頭 100 行の外にある。
    let cells = scheduled(&sample);
    assert_eq!(2, cells.len(), "違反の件数が 2 でない（前提が崩れている）");
    assert!(
        cells.iter().all(|(row, column)| *column == 0 && *row >= 100),
        "違反が先頭 100 行の中にある（窓の外という前提が崩れている）: {cells:?}"
    );
    let window = RowSpan::new(RowOrdinal::new(0), 100);
    assert!(
        !window.contains(RowOrdinal::new(999)),
        "序数 999 が窓の中に入っている"
    );
    for row in order.span(window) {
        assert!(
            !violating_row_ids(&sample).contains(row),
            "窓の中の行が違反を持っている（前提が崩れている）"
        );
    }

    // 窓の中から前方へ探すと、窓の外の最初の違反に到達する。
    let first = index
        .find(RowOrdinal::new(0), SearchDirection::Forward)
        .expect("窓の外の違反に到達できない");
    assert_eq!(address(&sample, 999, 0), first, "前方の到達点が違う");
    // 最後の可視行から後方へ探しても、窓の外の違反に到達する。
    let last = index
        .find(RowOrdinal::new(1_999), SearchDirection::Backward)
        .expect("後方の違反に到達できない");
    assert_eq!(address(&sample, 1_999, 0), last, "後方の到達点が違う");
    // 窓の内側の序数からでも、前方の最も近い違反（窓の外）へ到達する。
    let from_window = index
        .find(RowOrdinal::new(99), SearchDirection::Forward)
        .expect("窓の内側から違反に到達できない");
    assert_eq!(address(&sample, 999, 0), from_window, "窓の内側からの到達点が違う");
}

// ---------------------------------------------------------------------------
// 3. 前方・後方と、指定した位置の扱い
// ---------------------------------------------------------------------------

/// `find` は**指定した位置を含む**側から最も近い違反セルを返し、その向きに違反が無ければ
/// `None` を返す。
///
/// 標本は 100 行 × 2 列で、違反は行 9・19・…・99 の 2 列に仕込んである（10 行、20 件）。
/// 期待値は標本の申告から導き、手書きの定数にしない。
#[test]
fn forward_and_backward_include_the_starting_ordinal() {
    let (sample, report) = validated(100, 2, 0.1);
    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);

    let rows = violating_row_indices(&sample);
    assert_eq!(10, rows.len(), "違反を持つ行の数が 10 でない");
    assert_eq!(20, report.total_violations(), "違反の件数が 20 でない");
    let first = rows[0];
    let second = rows[1];
    let last = rows[rows.len() - 1];
    assert!(first > 0 && second > first, "違反行の並びが前提と違う");

    // 違反が無い行（先頭）から前方へ探すと、最初の違反行に到達する。
    assert_eq!(
        Some(address(&sample, first, 0)),
        index.find(RowOrdinal::new(0), SearchDirection::Forward),
        "先頭から前方の到達点が違う"
    );
    // 指定した位置が違反なら、その位置そのものを返す（含む）。
    assert_eq!(
        Some(address(&sample, first, 0)),
        index.find(RowOrdinal::new(first), SearchDirection::Forward),
        "違反の真上から前方の到達点が違う"
    );
    // その次の序数からは 2 番目の違反行へ進む（進み続けられる）。
    assert_eq!(
        Some(address(&sample, second, 0)),
        index.find(RowOrdinal::new(first + 1), SearchDirection::Forward),
        "違反の次から前方の到達点が違う"
    );
    // 最後の違反行の上でも、その位置そのものを返す（含む）。
    assert_eq!(
        Some(address(&sample, last, 0)),
        index.find(RowOrdinal::new(last), SearchDirection::Forward),
        "最後の違反の真上から前方の到達点が違う"
    );
    // 可視行数の外（= 100）から前方には違反が無い。
    assert_eq!(
        None,
        index.find(RowOrdinal::new(100), SearchDirection::Forward),
        "可視行数の外から前方で違反が返った"
    );

    // 後方も同じく指定した位置を含む。
    assert_eq!(
        Some(address(&sample, last, 0)),
        index.find(RowOrdinal::new(last), SearchDirection::Backward),
        "最後の違反の真上から後方の到達点が違う"
    );
    assert_eq!(
        Some(address(&sample, first, 0)),
        index.find(RowOrdinal::new(first), SearchDirection::Backward),
        "最初の違反の真上から後方の到達点が違う"
    );
    // 最初の違反より前には違反が無い。
    assert_eq!(
        None,
        index.find(RowOrdinal::new(0), SearchDirection::Backward),
        "先頭から後方で違反が返った"
    );
    assert_eq!(
        None,
        index.find(RowOrdinal::new(first - 1), SearchDirection::Backward),
        "最初の違反の手前から後方で違反が返った"
    );
    // 可視行数の外から後方へ探すと、最も後ろの違反に到達する（可視行の外は違反を持たない）。
    assert_eq!(
        Some(address(&sample, last, 0)),
        index.find(RowOrdinal::new(200), SearchDirection::Backward),
        "可視行数の外から後方の到達点が違う"
    );

    // 同じ行の複数の列が違反しているときは、**最も小さい列の添字**のセルを返す。
    // （行 `last` は 2 列とも違反である。前提を標本の申告で確かめる。）
    assert!(
        scheduled(&sample).contains(&(last, 0)) && scheduled(&sample).contains(&(last, 1)),
        "行 {last} が 2 列とも違反という前提が崩れている"
    );
    assert_eq!(
        ColumnIndex::new(0),
        index
            .find(RowOrdinal::new(last), SearchDirection::Forward)
            .expect("違反が見つからない")
            .column(),
        "同じ行で最も小さい列の添字を返していない"
    );
}

// ---------------------------------------------------------------------------
// 4. 入れ子の内側の位置（要件 4.5）
// ---------------------------------------------------------------------------

/// 入れ子の列の違反は、内側の位置を保ったまま索引に載る。
///
/// 標本の列 10（届け先）の違反は内側の 1 つのフィールドにあり、列 11（明細）の違反は配列の
/// 要素 0 の内側のフィールドにある。期待する位置は**宣言から読み直した**フィールド名で
/// 組み立てる（手書きの写しにしない）。
#[test]
fn a_nested_violation_keeps_its_inner_path() {
    // 標本の列の位置（`tests/common/sample.rs` の宣言の並び）。
    const DESTINATION: usize = 10;
    const LINE_ITEMS: usize = 11;
    const MACHINE_READABLE: usize = 12;

    let (sample, report) = validated(97, 13, 0.05);
    let schema = sample.compiled();
    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);

    // 前提: 3 つの列の名前と、内側のフィールドの宣言順。
    let destination_fields = object_fields(&schema, DESTINATION);
    let line_fields = object_fields(&schema, LINE_ITEMS);
    assert!(
        destination_fields.len() >= 1 && line_fields.len() >= 2,
        "入れ子のフィールドの宣言が前提と違う"
    );
    assert!(line_fields.len() == 3, "明細の要素のフィールドが 3 つでない");

    // 届け先: 内側の最初のフィールド（郵便番号）だけが違反である。
    let row = violating_row_for(&sample, DESTINATION);
    let cell = cell_at(&index, row, DESTINATION);
    assert_eq!(
        vec![NestedPathSegment::Field(
            destination_fields[0].clone().into()
        )],
        cell.paths()[0].segments().to_vec(),
        "届け先の違反の内側の位置が違う"
    );
    assert!(!cell.paths()[0].is_root(), "内側の位置が根になっている");

    // 明細: 要素 0 の 2 番目のフィールド（数量）だけが違反である。
    let row = violating_row_for(&sample, LINE_ITEMS);
    let cell = cell_at(&index, row, LINE_ITEMS);
    assert_eq!(
        vec![
            NestedPathSegment::Index(0),
            NestedPathSegment::Field(line_fields[1].clone().into()),
        ],
        cell.paths()[0].segments().to_vec(),
        "明細の違反の内側の位置が違う"
    );

    // 機械可読値: 値なしの違反であり、位置はセル直下（根）である。
    let row = violating_row_for(&sample, MACHINE_READABLE);
    let cell = cell_at(&index, row, MACHINE_READABLE);
    assert!(
        cell.paths()[0].is_root(),
        "セル直下の違反が内側の位置を持っている"
    );
}

/// 1 つのセルに複数の内側の位置があるとき、そのすべてが索引に載る。
///
/// 標本は 1 セルにつき違反を 1 件しか仕込まないため、この場合分けは**報告を手で組み立てて**
/// 確かめる（報告の組み立ては `ViolationReport` の公開面だけで行う）。
#[test]
fn a_cell_carries_all_of_its_inner_paths() {
    let sample = sample(&SampleOptions::new(4, 2).with_ratio(0.0));
    let row = row_id(&sample, 1);
    let column = ColumnIndex::new(1);
    let mut report = ViolationReport::new(&ValidationOptions::unlimited());
    report.push(violation_at_path(
        row,
        column,
        vec![ValuePathSegment::Field("内側A".into())],
    ));
    report.push(violation_at_path(
        row,
        column,
        vec![ValuePathSegment::Field("内側B".into())],
    ));
    report.push(violation_at_path(
        row,
        column,
        vec![ValuePathSegment::Index(0)],
    ));
    let report = report.finish(sample.sheet());

    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);
    assert_eq!(3, index.violation_total(), "総数が 3 でない");

    let cell = cell_at(&index, 1, 1);
    let paths: Vec<Vec<NestedPathSegment>> = cell
        .paths()
        .iter()
        .map(|path| path.segments().to_vec())
        .collect();
    assert_eq!(
        vec![
            vec![NestedPathSegment::Field("内側A".into())],
            vec![NestedPathSegment::Field("内側B".into())],
            vec![NestedPathSegment::Index(0)],
        ],
        paths,
        "1 つのセルの複数の内側の位置が保たれていない"
    );
    // 同じセルの違反は 1 つの行の 1 つの列にまとまる（列の見出しが重複しない）。
    let found = index.find(RowOrdinal::new(1), SearchDirection::Forward);
    assert_eq!(Some(CellAddress::new(row, column)), found, "探索の到達点が違う");
}

/// 同じ行の違反が**報告の中で飛び飛びに**現れても、その行の違反は 1 つも落ちない。
///
/// [`ViolationReport::push`] は行の並び順に押し込むことを呼び出し側へ求めているが、本索引の
/// 組み立て（[`ViolationIndex::build`]）は**公開面**であり、5.2 の差分更新が載せる報告は
/// その前提を満たすとは限らない。行が 2 度現れる報告を通しても、索引の件数・総数・到達可能性・
/// 据え付けのいずれも欠けてはならない（行を鍵とする保持は上書きではなく**併合**である）。
///
/// 報告の並びは (行1, 列0) → (行2, 列0) → (行1, 列1) とし、**行 1 が非連続に 2 度現れる**。
#[test]
fn a_row_appearing_twice_in_the_report_keeps_all_of_its_violations() {
    let sample = sample(&SampleOptions::new(4, 2).with_ratio(0.0));
    let first = row_id(&sample, 1);
    let second = row_id(&sample, 2);
    let mut report = ViolationReport::new(&ValidationOptions::unlimited());
    // 行 1 の列 0（内側の位置つき）。
    report.push(violation_at_path(
        first,
        ColumnIndex::new(0),
        vec![ValuePathSegment::Field("一度目".into())],
    ));
    // 行 2 の列 0（行 1 を挟んで、行 1 がまた現れる）。
    report.push(violation_at_path(
        second,
        ColumnIndex::new(0),
        Vec::new(),
    ));
    // 行 1 の列 1。
    report.push(violation_at_path(
        first,
        ColumnIndex::new(1),
        Vec::new(),
    ));
    let report = report.finish(sample.sheet());
    assert_eq!(3, report.total_violations(), "報告の総数が 3 でない");

    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);

    // 件数は総数にも、実際に載った数にも一致する（上書きで落ちていない）。
    assert_eq!(3, index.violation_total(), "総数が違う");
    assert_eq!(
        report.total_violations(),
        index.indexed_violations(),
        "載せた件数が報告の総数と違う（飛び飛びの行で取りこぼしている）"
    );
    assert!(index.is_complete(), "取りこぼしがある");
    assert_eq!(
        report.violations().len(),
        indexed_violations(&index, &order),
        "索引に載った違反の件数が報告の保持した件数と違う"
    );

    // 行 1 は列 0 と列 1 の双方を持ち、どちらも到達できる。
    let entry = index
        .row_violations(RowOrdinal::new(1))
        .expect("行 1 の違反が無い");
    assert_eq!(first, entry.row(), "行が違う");
    assert_eq!(
        vec![ColumnIndex::new(0), ColumnIndex::new(1)],
        entry
            .columns()
            .iter()
            .map(|cell| cell.column())
            .collect::<Vec<_>>(),
        "行 1 の列が昇順に揃っていない"
    );
    assert_eq!(2, entry.len(), "行 1 の違反の件数が 2 でない");
    assert_eq!(
        vec![NestedPathSegment::Field("一度目".into())],
        entry.columns()[0].paths()[0].segments().to_vec(),
        "先に押し込んだ列 0 の内側の位置が落ちている"
    );
    // 探索は行 1 の最も小さい列を返す（列 0 が生きている）。
    assert_eq!(
        Some(CellAddress::new(first, ColumnIndex::new(0))),
        index.find(RowOrdinal::new(1), SearchDirection::Forward),
        "行 1 の最も小さい列に到達できない"
    );
    assert_eq!(
        Some(CellAddress::new(second, ColumnIndex::new(0))),
        index.find(RowOrdinal::new(2), SearchDirection::Forward),
        "行 2 の違反に到達できない"
    );

    // 据え付けも行 1 の 2 列の双方を載せる（飛び飛びの併合で列の印が落ちない）。
    let presence = index.presence();
    assert_eq!(2, presence.len(), "据え付けられた行の数が違う");
    assert!(
        presence.has_column(first, ColumnIndex::new(0)),
        "行 1 の列 0 の印が落ちている"
    );
    assert!(
        presence.has_column(first, ColumnIndex::new(1)),
        "行 1 の列 1 の印が落ちている"
    );
    assert!(presence.has_any(first), "行 1 の印が落ちている");
    assert!(
        presence.has_column(second, ColumnIndex::new(0)),
        "行 2 の印が落ちている"
    );
}

/// 同じ行の**同じ列**が報告に 2 度現れたとき、そのセルの内側の位置は併合されて 1 つのセルに
/// まとまる（列の見出しが重複せず、総数も載せた件数も一致する）。
///
/// 報告の並びは (行1, 列0, 「一度目」) → (行2, 列1) → (行1, 列0, 「二度目」) とし、
/// **行 1 の列 0 が非連続に 2 度現れる**。併合の「同じ列が既にある」側である。
#[test]
fn a_repeated_cell_merges_its_inner_paths_into_one_column() {
    let sample = sample(&SampleOptions::new(4, 2).with_ratio(0.0));
    let first = row_id(&sample, 1);
    let second = row_id(&sample, 2);
    let mut report = ViolationReport::new(&ValidationOptions::unlimited());
    report.push(violation_at_path(
        first,
        ColumnIndex::new(0),
        vec![ValuePathSegment::Field("一度目".into())],
    ));
    report.push(violation_at_path(
        second,
        ColumnIndex::new(1),
        Vec::new(),
    ));
    report.push(violation_at_path(
        first,
        ColumnIndex::new(0),
        vec![ValuePathSegment::Field("二度目".into())],
    ));
    let report = report.finish(sample.sheet());
    assert_eq!(3, report.total_violations(), "報告の総数が 3 でない");

    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);

    // 載せた件数は報告の総数に一致し、セルの見出しは 1 つにまとまる。
    assert_eq!(report.total_violations(), index.indexed_violations(), "件数が違う");
    assert!(index.is_complete(), "取りこぼしがある");
    let entry = index
        .row_violations(RowOrdinal::new(1))
        .expect("行 1 の違反が無い");
    assert_eq!(1, entry.columns().len(), "同じ列が 2 つに分かれている");
    assert_eq!(2, entry.len(), "同じ列の違反の件数が 2 でない");
    assert_eq!(
        vec![
            vec![NestedPathSegment::Field("一度目".into())],
            vec![NestedPathSegment::Field("二度目".into())],
        ],
        entry.columns()[0]
            .paths()
            .iter()
            .map(|path| path.segments().to_vec())
            .collect::<Vec<_>>(),
        "同じ列の 2 つの内側の位置が報告の順に保たれていない"
    );
    assert_eq!(
        report.violations().len(),
        indexed_violations(&index, &order),
        "索引に載った違反の件数が報告の保持した件数と違う"
    );
}

// ---------------------------------------------------------------------------
// 5. 鍵の張り直し（タスク 2.4）

/// 並べ替えや絞り込みで可視の順序が変わったら、索引の鍵を張り直す。序数が指す行が入れ替わる。
///
/// 並べ替えの前後で**最初・最後の違反の位置**を観測し、同じ索引が違う行を返すことを見る
/// （片側だけでは、鍵が動いたのか標本の違反が動いたのかを区別できない）。
#[test]
fn rekeying_moves_the_ordinal_keys_after_recompute() {
    let (sample, report) = validated(100, 2, 0.1);
    let mut order = RowOrder::default();
    order.recompute(sample.document(), sample.sheet(), &no_view());
    assert_order_is_the_document_order(&sample, &order);
    let mut index = ViolationIndex::build(&report, &order);

    let violating = violating_row_ids(&sample);
    let before_first = first_violating_ordinal(&order, &violating).expect("違反が無い");
    let before_last = last_violating_ordinal(&order, &violating).expect("違反が無い");
    let before_first_row = order.row_at(before_first).expect("序数が引けない");
    let before_last_row = order.row_at(before_last).expect("序数が引けない");
    assert_eq!(
        Some(address(&sample, before_first.get(), 0)),
        index.find(RowOrdinal::new(0), SearchDirection::Forward),
        "張り直す前の前方の到達点が違う"
    );

    // 列 0（品番）の降順で並べ替える。適合する値は `P`、違反する値は `!` で始まるため、
    // 違反行は末尾へ寄る。並べ替えが実際に順序を変えたことを先に確かめる。
    order.recompute(sample.document(), sample.sheet(), &sorted(0, true));
    assert_ne!(
        Some(before_first_row),
        order.row_at(RowOrdinal::new(0)),
        "並べ替えが可視の順序を変えていない（前提が崩れている）"
    );
    index.rekey(&order);

    let after_first = first_violating_ordinal(&order, &violating).expect("違反が無い");
    let after_last = last_violating_ordinal(&order, &violating).expect("違反が無い");
    assert_ne!(
        before_first, after_first,
        "最初の違反の序数が動いていない（鍵を張り直しても動かない標本である）"
    );
    let after_first_row = order.row_at(after_first).expect("序数が引けない");
    let after_last_row = order.row_at(after_last).expect("序数が引けない");
    assert_ne!(
        before_first_row, after_first_row,
        "最初の違反の行が動いていない（前提が崩れている）"
    );
    assert_eq!(
        Some(CellAddress::new(after_first_row, ColumnIndex::new(0))),
        index.find(RowOrdinal::new(0), SearchDirection::Forward),
        "張り直した後の前方の到達点が違う"
    );
    assert_eq!(
        Some(CellAddress::new(after_last_row, ColumnIndex::new(0))),
        index.find(RowOrdinal::new(order.len() - 1), SearchDirection::Backward),
        "張り直した後の後方の到達点が違う"
    );
    // 張り直す前の最後の違反の行（文書順で最後の違反行）は、張り直した後は先頭側へ動く。
    assert_ne!(
        before_last_row, after_last_row,
        "最後の違反の行が動いていない（前提が崩れている）"
    );
    assert_eq!(
        Some(CellAddress::new(after_first_row, ColumnIndex::new(0))),
        index.find(after_first, SearchDirection::Backward),
        "張り直した後の、違反の真上からの後方の到達点が違う"
    );
}

/// 絞り込みで違反行が隠れた順序へ張り直すと、探索は違反に到達しない。絞り込みを外して
/// 張り直すと、再び到達する（索引は報告を組み直さずに鍵だけを張り直せる）。
#[test]
fn rekeying_after_a_filter_change_removes_and_restores_reachability() {
    let (sample, report) = validated(100, 2, 0.1);
    let violating = violating_row_ids(&sample);
    let mut order = RowOrder::default();
    order.recompute(sample.document(), sample.sheet(), &no_view());
    let mut index = ViolationIndex::build(&report, &order);
    assert_eq!(
        Some(address(&sample, violating_row_indices(&sample)[0], 0)),
        index.find(RowOrdinal::new(0), SearchDirection::Forward),
        "絞り込む前の到達点が違う"
    );

    // 列 0 が `P` を含む行だけを通す = 列 0 の違反を仕込んだ行がちょうど隠れる。
    order.recompute(sample.document(), sample.sheet(), &contains(0, "P"));
    index.rekey(&order);
    let hidden: BTreeSet<RowId> = order
        .row_at(RowOrdinal::new(0))
        .map(|_| ())
        .into_iter()
        .flat_map(|()| (0..order.len()).map(RowOrdinal::new))
        .filter_map(|ordinal| order.row_at(ordinal))
        .collect();
    let expect_hidden: BTreeSet<RowId> = sample
        .row_ids()
        .iter()
        .copied()
        .filter(|row| !hidden.contains(row))
        .collect();
    assert_eq!(
        violating, expect_hidden,
        "隠れた行が違反行の集合と一致しない（前提が崩れている）"
    );
    assert_eq!(None, first_violating_ordinal(&order, &violating), "違反行が可視である");
    assert_eq!(
        None,
        index.find(RowOrdinal::new(0), SearchDirection::Forward),
        "隠れた違反に前方から到達した"
    );
    assert_eq!(
        None,
        index.find(RowOrdinal::new(order.len() - 1), SearchDirection::Backward),
        "隠れた違反に後方から到達した"
    );

    // 絞り込みを外して張り直すと、同じ報告のまま再び到達できる。
    order.recompute(sample.document(), sample.sheet(), &no_view());
    index.rekey(&order);
    assert_eq!(
        Some(address(&sample, violating_row_indices(&sample)[0], 0)),
        index.find(RowOrdinal::new(0), SearchDirection::Forward),
        "絞り込みを外した後の到達点が違う"
    );
    assert_eq!(
        20,
        scheduled(&sample).len(),
        "違反の件数が 20 でない（前提が崩れている）"
    );
    assert_eq!(
        report.total_violations(),
        index.violation_total(),
        "絞り込みの前後で総数が変わっている"
    );
}

// ---------------------------------------------------------------------------
// 6. 可視でない行の違反
// ---------------------------------------------------------------------------

/// 絞り込みで隠れた行の違反は、総数に数え、据え付けに載せ、序数の索引には載せない。
///
/// 序数が存在しない以上、探索で到達できないのは「届かない」ではなく**正しい答え**である
/// （到達点として返す位置が無い）。一方で据え付けは行を鍵とするため、隠れた行の違反も載る
/// — 絞り込みの判定は「その行の違反」を読むのであり、現在の可視の集合を読むのではない。
#[test]
fn a_violation_on_a_hidden_row_is_counted_but_has_no_position() {
    let (sample, report) = validated(100, 2, 0.1);
    let violating = violating_row_ids(&sample);
    let mut order = RowOrder::default();
    order.recompute(sample.document(), sample.sheet(), &contains(0, "P"));
    let index = ViolationIndex::build(&report, &order);

    // 前提: 隠れた行は違反行の集合そのものであり、可視の行は違反を持たない。
    assert_eq!(90, order.len(), "可視行数が 90 でない");
    assert_eq!(10, order.hidden(), "隠れた行数が 10 でない");
    for row in &violating {
        assert_eq!(None, order.ordinal_of(*row), "違反行が可視である");
    }
    for ordinal in (0..order.len()).map(RowOrdinal::new) {
        let row = order.row_at(ordinal).expect("序数が引けない");
        assert!(
            !violating.contains(&row),
            "可視の行が違反を持っている（前提が崩れている）"
        );
    }

    // 総数は絞り込みに依らない（シートの違反の総数である）。
    assert_eq!(
        report.total_violations(),
        index.violation_total(),
        "隠れた行の違反が総数から落ちている"
    );
    // 序数が無いため探索では届かない。
    assert_eq!(
        None,
        index.find(RowOrdinal::new(0), SearchDirection::Forward),
        "隠れた違反に前方から到達した"
    );
    assert_eq!(
        None,
        index.find(RowOrdinal::new(order.len() - 1), SearchDirection::Backward),
        "隠れた違反に後方から到達した"
    );
    // 据え付けには載る（隠れた行の違反も落ちない）。
    let presence = index.presence();
    for row in &violating {
        assert!(
            presence.has_any(*row),
            "隠れた違反行の据え付けが落ちている"
        );
        assert!(
            presence.has_column(*row, ColumnIndex::new(0)),
            "隠れた違反行の列の印が落ちている"
        );
    }
    assert_eq!(
        violating.len(),
        presence.len(),
        "据え付けられた行の数が違反行の数と違う"
    );
}

// ---------------------------------------------------------------------------
// 7. 列そのものの問題（`Violation::row` が `None`）
// ---------------------------------------------------------------------------

/// 行に属さない違反（列そのものの問題）は、総数に数えて別に保持し、探索でも据え付けでも
/// 届かない。
///
/// シート全体の検証はこの形の違反を報告しない（書き込み経路が行に属さない値を判定するとき
/// だけ現れる）ため、この場合分けは**報告を手で組み立てて**確かめる。
#[test]
fn a_column_level_violation_is_counted_but_cannot_be_located() {
    let sample = sample(&SampleOptions::new(4, 2).with_ratio(0.0));
    let row = row_id(&sample, 2);
    let mut report = ViolationReport::new(&ValidationOptions::unlimited());
    // 行に属する違反（行 2 の列 1）。
    report.push(violation_at_path(
        row,
        ColumnIndex::new(1),
        vec![ValuePathSegment::Field("内側".into())],
    ));
    // 行に属さない違反（列 1 そのものの問題）。
    report.push(Violation::at_cell(
        None,
        ColumnIndex::new(1),
        "数量",
        ViolationReason::UnusableColumn {
            expected: Expected::Usable,
            actual: CellValue::Null,
            kind: "future-kind".into(),
        },
    ));
    let report = report.finish(sample.sheet());
    assert_eq!(2, report.total_violations(), "総数が 2 でない");

    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);

    // 総数は両方を数える（行に属さない違反も「表示中のシートに存在する違反」である）。
    assert_eq!(2, index.violation_total(), "行に属さない違反が総数から落ちている");
    assert!(index.is_complete(), "取りこぼしがある");
    assert_eq!(2, indexed_violations(&index, &order), "索引の件数が違う");

    // 別に保持されており、列の添字と内側の位置が読める。
    assert_eq!(1, index.column_violations().len(), "列の問題が保持されていない");
    let column = &index.column_violations()[0];
    assert_eq!(ColumnIndex::new(1), column.column(), "列の添字が違う");
    assert!(
        column.paths()[0].is_root(),
        "セル直下の列の問題が内側の位置を持っている"
    );

    // 探索は行に属する違反だけを返す（列そのものの問題には位置が無い）。
    let found = index
        .find(RowOrdinal::new(0), SearchDirection::Forward)
        .expect("行に属する違反に到達できない");
    assert_eq!(CellAddress::new(row, ColumnIndex::new(1)), found, "到達点が違う");
    // 据え付けは行を鍵とするため、行に属さない違反は載せられない。
    let presence = index.presence();
    assert_eq!(1, presence.len(), "行に属さない違反が据え付けに載っている");
    assert!(presence.has_column(row, ColumnIndex::new(1)), "行の印が落ちている");
}

// ---------------------------------------------------------------------------
// 8. 据え付けの端から端（2.2 と 2.4 の継ぎ目）
// ---------------------------------------------------------------------------

/// 索引が作った据え付けを `RowOrder` へ据え付けると、`HasViolation` の絞り込みがちょうど
/// 期待どおりの行を選ぶ。
///
/// これは 2.2 が据え付けの受け取り側として定めた意味論（列を問わない側は「1 つでも違反が
/// あれば」、列を指定した側はその列の違反）が、2.4 の作る情報でそのまま働くことの検査である。
#[test]
fn the_installed_presence_selects_exactly_the_violating_rows() {
    let (sample, report) = validated(97, 5, 0.05);
    let mut order = RowOrder::default();
    order.recompute(sample.document(), sample.sheet(), &no_view());
    assert_order_is_the_document_order(&sample, &order);
    let index = ViolationIndex::build(&report, &order);
    assert!(index.is_complete(), "取りこぼしがある");

    let scheduled = scheduled(&sample);
    assert_eq!(
        24,
        scheduled.len(),
        "仕込んだ違反の件数が 24 でない（前提が崩れている）"
    );

    // 据え付けを置き、列を問わない違反ありで絞り込む。
    index.install(&mut order);
    let view = ViewSpec {
        sort: Vec::new(),
        filters: vec![FilterSpec::HasViolation { column: None }],
    };
    let summary = order.recompute(sample.document(), sample.sheet(), &view);
    let selected: BTreeSet<usize> = (0..order.len())
        .map(RowOrdinal::new)
        .filter_map(|ordinal| order.row_at(ordinal))
        .map(|row| {
            sample
                .row_ids()
                .iter()
                .position(|candidate| *candidate == row)
                .expect("可視の行が標本の行に無い")
        })
        .collect();
    let expected: BTreeSet<usize> = violating_row_indices(&sample).into_iter().collect();
    assert_eq!(
        expected, selected,
        "違反ありの絞り込みが選んだ行が違反行の集合と一致しない"
    );
    assert_eq!(
        expected.len(),
        summary.visible,
        "可視行数が違反行の数と一致しない"
    );
    assert_eq!(
        sample.rows() - expected.len(),
        summary.hidden,
        "隠れた行数が違う"
    );

    // 列を指定した絞り込みは、その列が違反している行だけを選ぶ。
    for column in 0..sample.column_count() {
        let expected: BTreeSet<usize> = scheduled
            .iter()
            .filter(|(_, at)| *at == column)
            .map(|(row, _)| *row)
            .collect();
        assert!(
            !expected.is_empty(),
            "列 {column} に違反が 1 件も無い（前提が崩れている）"
        );
        let view = ViewSpec {
            sort: Vec::new(),
            filters: vec![FilterSpec::HasViolation {
                column: Some(ColumnIndex::new(column)),
            }],
        };
        let summary = order.recompute(sample.document(), sample.sheet(), &view);
        assert_eq!(
            expected.len(),
            summary.visible,
            "列 {column} の違反ありの絞り込みの可視行数が違う"
        );
        for ordinal in (0..order.len()).map(RowOrdinal::new) {
            let row = order.row_at(ordinal).expect("序数が引けない");
            let at = sample
                .row_ids()
                .iter()
                .position(|candidate| *candidate == row)
                .expect("可視の行が標本の行に無い");
            assert!(
                expected.contains(&at),
                "列 {column} の違反ありの絞り込みが違反の無い行を選んだ"
            );
        }
    }

    // 違反を持たない列を指定すると、どの行も選ばれない（据え付けに無い列である）。
    let view = ViewSpec {
        sort: Vec::new(),
        filters: vec![FilterSpec::HasViolation {
            column: Some(ColumnIndex::new(sample.column_count())),
        }],
    };
    let summary = order.recompute(sample.document(), sample.sheet(), &view);
    assert_eq!(0, summary.visible, "違反の無い列で行が選ばれた");
}

// ---------------------------------------------------------------------------
// 9. 決定性
// ---------------------------------------------------------------------------

/// 同じ報告と同じ順序からは常に同じ索引が出る。
///
/// 走査の向きと序数を全件確かめ、据え付けも比較する。標本の組み立てが違っても（行識別子が
/// 違っても）**構造**は同じであることを、序数ごとの列と内側の位置の突き合わせで見る。
#[test]
fn the_same_report_and_order_build_the_same_index() {
    let (sample, report) = validated(97, 13, 0.05);
    let (order, first) = indexed(&sample, &report);
    let second = ViolationIndex::build(&report, &order);

    assert_eq!(first.violation_total(), second.violation_total(), "総数が違う");
    assert_eq!(
        first.column_violations(),
        second.column_violations(),
        "列の問題が違う"
    );
    assert_eq!(first.presence(), second.presence(), "据え付けが違う");
    for pointer in 0..=order.len() {
        for direction in [SearchDirection::Forward, SearchDirection::Backward] {
            assert_eq!(
                first.find(RowOrdinal::new(pointer), direction),
                second.find(RowOrdinal::new(pointer), direction),
                "序数 {pointer} の探索の結果が違う"
            );
        }
    }

    // 同じ引数の 2 回目の標本は識別子が違う。索引の構造（序数ごとの列と内側の位置）を、
    // 標本の中での行の位置へ畳んで比べる。
    let (other, other_report) = validated(97, 13, 0.05);
    let (other_order, third) = indexed(&other, &other_report);
    assert_eq!(order.len(), other_order.len(), "可視行数が違う");
    let shape = |index: &ViolationIndex, sample: &Sample, order: &RowOrder| {
        let mut out: BTreeMap<usize, BTreeMap<usize, Vec<Vec<NestedPathSegment>>>> = BTreeMap::new();
        for ordinal in (0..order.len()).map(RowOrdinal::new) {
            let Some(entry) = index.row_violations(ordinal) else {
                continue;
            };
            let at = sample
                .row_ids()
                .iter()
                .position(|candidate| *candidate == entry.row())
                .expect("索引の行が標本の行に無い");
            let mut cells = BTreeMap::new();
            for cell in entry.columns() {
                cells.insert(
                    cell.column().index(),
                    cell.paths()
                        .iter()
                        .map(|path| path.segments().to_vec())
                        .collect(),
                );
            }
            out.insert(at, cells);
        }
        out
    };
    assert_eq!(
        shape(&first, &sample, &order),
        shape(&third, &other, &other_order),
        "同じ引数の標本から違う構造の索引が出た"
    );
}

// ---------------------------------------------------------------------------
// 10. 保持が上限で切られた報告
// ---------------------------------------------------------------------------

/// 報告の保持が上限で切られていても、総数は切られない。索引に載るのは保持された分だけで
/// あり、その差が [`ViolationIndex::is_complete`] で観測できる。
#[test]
fn a_capped_report_keeps_the_total_but_not_the_index() {
    let sample = sample(&SampleOptions::new(2_000, 1).with_ratio(0.001));
    let schema = sample.compiled();
    let report = SchemaEngine::new().validate_sheet(
        sample.document(),
        sample.sheet(),
        &schema,
        &ValidationOptions::capped(1),
    );
    // 前提: 標本は違反を 2 件持ち、報告は保持を 1 件で切っている。
    assert_eq!(2, scheduled(&sample).len(), "違反の件数が 2 でない");
    assert_eq!(2, report.total_violations(), "総件数が切られている");
    assert_eq!(1, report.violations().len(), "保持が切られていない");
    assert!(report.is_truncated(), "切られたことが観測できない");

    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);
    assert_eq!(2, index.violation_total(), "総数が切られている");
    assert!(!index.is_complete(), "取りこぼしが観測できない");
    assert_eq!(1, indexed_violations(&index, &order), "索引の件数が違う");
    // 保持された側の違反（行 999）には到達でき、切られた側（行 1999）には到達できない。
    assert_eq!(
        Some(address(&sample, 999, 0)),
        index.find(RowOrdinal::new(0), SearchDirection::Forward),
        "保持された違反に到達できない"
    );
    assert_eq!(
        Some(address(&sample, 999, 0)),
        index.find(RowOrdinal::new(1_999), SearchDirection::Backward),
        "切られた違反が索引に載っている"
    );
}

// ---------------------------------------------------------------------------
// 宣言から期待値を導く補助
// ---------------------------------------------------------------------------

/// 入れ子のオブジェクトの列のフィールド名を、宣言順に読み出す。
///
/// 配列の列を渡した場合は、要素のオブジェクトのフィールド名を読み出す（標本の明細の列は
/// この形である）。入れ子でない列・使用不能な列は空を返す。
fn object_fields(schema: &schema_engine::CompiledSchema, column: usize) -> Vec<String> {
    let Some(validator) = schema.validator(ColumnIndex::new(column)) else {
        return Vec::new();
    };
    let fields = match validator {
        ColumnValidator::Object { fields } => fields,
        ColumnValidator::Array { items, .. } => match items.as_ref() {
            ColumnValidator::Object { fields } => fields,
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    fields.iter().map(|field| field.name().to_owned()).collect()
}

/// 列 `column` に違反を仕込んだ標本の行添字（違反が無ければ panic する）。
fn violating_row_for(sample: &Sample, column: usize) -> usize {
    let found = (0..sample.rows())
        .find(|row| sample.violation_at(*row, column))
        .unwrap_or_else(|| panic!("列 {column} に違反を仕込んだ行が無い"));
    found
}

/// 索引から、標本の行添字と列添字のセルの違反を読む（絞り込みも並べ替えも無い順序である
/// ことを前提とする。行添字がそのまま序数になる）。
fn cell_at(
    index: &ViolationIndex,
    row: usize,
    column: usize,
) -> &data_grid::CellViolations {
    index
        .row_violations(RowOrdinal::new(row))
        .unwrap_or_else(|| panic!("序数 {row} に違反行が無い"))
        .cell(ColumnIndex::new(column))
        .unwrap_or_else(|| panic!("行 {row} の列 {column} に違反が無い"))
}

/// 指定した内側の位置を持つ違反を 1 件組み立てる（行に属する）。
fn violation_at_path(
    row: RowId,
    column: ColumnIndex,
    path: Vec<ValuePathSegment>,
) -> Violation {
    Violation::new(
        Some(row),
        column,
        "列",
        ValuePath::from(path),
        ViolationReason::TypeMismatch {
            expected: Expected::Kind("text".into()),
            actual: CellValue::Null,
        },
    )
}

// ---------------------------------------------------------------------------
// 8. 併合の副次的な取り決め（レビューの変異試験が捕まえられなかった 2 点）
// ---------------------------------------------------------------------------

/// 併合した行の**列の並びは昇順**であり、探索が返すのは最も小さい列である。
///
/// 報告の並びを (行1, 列2) → (行2, 列0) → (行1, 列0) とする。行 1 には列 2 が先に入り、
/// 後から列 0 が来る — 併合が「新しい列を後ろへ足す」だけの実装だと `[2, 0]` になり、
/// 探索は列 2 を返してしまう（列 0 が生きているのに到達できない）。
///
/// 2 列の例（`a_row_appearing_twice_in_the_report_keeps_all_of_its_violations`）では
/// 挿入位置がたまたま正しくなるため、**3 列を逆順に与える**この検査が要る。
#[test]
fn merged_columns_stay_ascending_even_when_the_report_gives_them_out_of_order() {
    let sample = sample(&SampleOptions::new(4, 3).with_ratio(0.0));
    let first = row_id(&sample, 1);
    let second = row_id(&sample, 2);
    let mut report = ViolationReport::new(&ValidationOptions::unlimited());
    report.push(violation_at_path(first, ColumnIndex::new(2), Vec::new()));
    report.push(violation_at_path(second, ColumnIndex::new(0), Vec::new()));
    report.push(violation_at_path(first, ColumnIndex::new(0), Vec::new()));
    let report = report.finish(sample.sheet());
    assert_eq!(3, report.total_violations(), "報告の総数が 3 でない");

    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);
    assert_eq!(
        report.total_violations(),
        index.indexed_violations(),
        "件数が違う"
    );
    assert!(index.is_complete(), "取りこぼしがある");

    let entry = index
        .row_violations(RowOrdinal::new(1))
        .expect("行 1 の違反が無い");
    assert_eq!(
        vec![ColumnIndex::new(0), ColumnIndex::new(2)],
        entry
            .columns()
            .iter()
            .map(|cell| cell.column())
            .collect::<Vec<_>>(),
        "併合した列の並びが昇順でない"
    );
    // 最も小さい列（0）に到達できること。`[2, 0]` のままなら列 2 が返る。
    assert_eq!(
        Some(CellAddress::new(first, ColumnIndex::new(0))),
        index.find(RowOrdinal::new(1), SearchDirection::Forward),
        "併合した行で最も小さい列に到達できない"
    );
}

/// `indexed_violations` は**同じセルに載った複数の経路をそれぞれ数える**（件数であって列数ではない）。
///
/// 1 つのセルに内側の位置を 3 つ持つ違反を 1 行へ載せる。`indexed` を「列の数」で数える実装だと
/// ここが 1 になり、`is_complete()` が偽（報告が切られた）と誤って報告される。
#[test]
fn indexed_violations_counts_every_inner_path_not_every_column() {
    let sample = sample(&SampleOptions::new(4, 2).with_ratio(0.0));
    let row = row_id(&sample, 1);
    let mut report = ViolationReport::new(&ValidationOptions::unlimited());
    for name in ["一", "二", "三"] {
        report.push(violation_at_path(
            row,
            ColumnIndex::new(0),
            vec![ValuePathSegment::Field(name.into())],
        ));
    }
    let report = report.finish(sample.sheet());
    assert_eq!(3, report.total_violations(), "報告の総数が 3 でない");

    let (order, index) = indexed(&sample, &report);
    assert_order_is_the_document_order(&sample, &order);

    // 前提: 3 件とも同じ行の同じセルに載っている（列は 1 つである）。
    let entry = index
        .row_violations(RowOrdinal::new(1))
        .expect("行 1 の違反が無い");
    assert_eq!(1, entry.columns().len(), "セルが 1 つにまとまっていない");
    assert_eq!(3, entry.len(), "同じセルの経路の件数が 3 でない");

    // 件数は経路の数であり、列の数ではない。
    assert_eq!(3, index.indexed_violations(), "載せた件数が経路の数と違う");
    assert_eq!(
        report.total_violations(),
        index.indexed_violations(),
        "載せた件数が報告の総数と違う"
    );
    assert!(
        index.is_complete(),
        "取りこぼしがある（列の数で数えている）"
    );
    assert_eq!(
        report.violations().len(),
        indexed_violations(&index, &order),
        "索引に載った違反の件数が報告の保持した件数と違う"
    );
}

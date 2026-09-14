//! 絞り込みの行集合の導出（データグリッドのタスク 2.2。data-grid 要件 8.4, 8.5, 8.7）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のものである。
//!
//! 1. **5 種の絞り込み**（一致・部分一致・値なし・値あり・違反あり）が、それぞれ単独で
//!    期待どおりの行を選ぶ（要件 8.4）。
//! 2. **複数の絞り込みは積**である（すべてに一致する行だけが可視）。一致する行が 1 つも無い
//!    組み合わせ（空の積）も固定する。
//! 3. **可視行数と隠された行数の双方**が返り（要件 8.7）、`visible + hidden` が**常に**
//!    シートの行数に一致する（それぞれの数ではなく、この恒等式そのものを検査する）。
//! 4. **絞り込みが 0 件なら恒等**である（全行が可視、隠された行は 0）。
//! 5. **並べ替えは絞り込んだ集合に対して定まる**。除外された行は順序に現れず、残った行は
//!    値の順に並ぶ（文書の行順を写しただけになっていないことを観測する）。
//! 6. **すべてを除外する絞り込み**は可視 0・隠された行 = 全行であり、どの序数も引けない。
//! 7. **値なし・値ありは表示文字列の規則と一致**する（`Null`・空のテキスト・値の数が列の
//!    添字に届かない行のいずれについても。2.1 と同じ「届かない値は値なし」の規約）。
//! 8. **違反ありは据え付けられるまで何にも一致しない**（空は「違反が無い」ではなく「情報が
//!    無い」）。据え付けた後は `None`（列を問わない）と `Some(列)` がそれぞれ正しい行に一致し、
//!    **絞り込みは文書を書き換えない**。
//! 9. **決定性**: 同一の文書と同一の指定から常に同一の可視の並びが出る。
//! 10. **文書の行の並びと保存された値は導出の前後で変わらない**（要件 8.5）。
//!
//! 表示文字列そのものの規則（変種ごと）も 1 つのテストで固定する — 絞り込みは値ではなく
//! 表示文字列に対して行うため、規則が変われば絞り込みの結果も変わる（規則の正典は
//! `crates/data-grid/src/view/mod.rs` の [`DisplayText`] / [`display_text`] である）。
//!
//! # Dependencies
//!
//! `view/violations.rs`（タスク 2.4）は**まだ無い**ため、`HasViolation` の検査は
//! [`ViolationPresence`] を**テスト自身が組み立てて据え付ける**。違反の索引が入った後も
//! この据え付けの意味論（空 = 情報なし、`None` = 列を問わない、`Some(列)` = 列の一致）は
//! 変わらないため、この検査はそのまま生きる。
//!
//! # 識別子を期待値に書かない
//!
//! 標本（`tests/common/sample.rs`）の行識別子は発行時刻を含む ULID であり、組み立てのたびに
//! 変わる。標本を使う検査は**識別子そのものを期待値に書かず**、標本が公開する行識別子の並び
//! （`row_ids`）と突き合わせる。手で組んだ文書は 1 回の組み立ての中で識別子を読み出すため、
//! この制約に触れない（`tests/sort_order.rs` と同じ規律）。

mod common;

use common::sample::{sample, SampleOptions};
use data_grid::{
    display_text, ColumnIndex, DisplayText, FilterSpec, RowOrder, RowOrdinal, RowSpan, SortKey,
    ViewSpec, ViewSummary, ViolationPresence,
};
use document_format::{AttachmentId, CellValue, Document, NestedValue, RowId, SheetId};

// ---------------------------------------------------------------------------
// 表示の指定を短く書く補助
// ---------------------------------------------------------------------------

/// 一致の絞り込み。
fn equals(column: usize, text: &str) -> FilterSpec {
    FilterSpec::Equals {
        column: ColumnIndex::new(column),
        text: text.to_owned(),
    }
}

/// 部分一致の絞り込み。
fn contains(column: usize, text: &str) -> FilterSpec {
    FilterSpec::Contains {
        column: ColumnIndex::new(column),
        text: text.to_owned(),
    }
}

/// 値なしの絞り込み。
fn is_empty(column: usize) -> FilterSpec {
    FilterSpec::IsEmpty {
        column: ColumnIndex::new(column),
    }
}

/// 値ありの絞り込み。
fn is_not_empty(column: usize) -> FilterSpec {
    FilterSpec::IsNotEmpty {
        column: ColumnIndex::new(column),
    }
}

/// 違反ありの絞り込み（列を問わない）。
fn any_violation() -> FilterSpec {
    FilterSpec::HasViolation { column: None }
}

/// 違反ありの絞り込み（列を指定する）。
fn column_violation(column: usize) -> FilterSpec {
    FilterSpec::HasViolation {
        column: Some(ColumnIndex::new(column)),
    }
}

/// 絞り込みだけの指定（並べ替えの基準列は 0 本）。
fn filtered(filters: Vec<FilterSpec>) -> ViewSpec {
    ViewSpec {
        sort: Vec::new(),
        filters,
    }
}

/// 絞り込みと並べ替えの指定（基準列は「列の添字と降順かどうか」の並び）。
fn view(sort: &[(usize, bool)], filters: Vec<FilterSpec>) -> ViewSpec {
    ViewSpec {
        sort: sort
            .iter()
            .map(|&(column, descending)| SortKey {
                column: ColumnIndex::new(column),
                descending,
            })
            .collect(),
        filters,
    }
}

// ---------------------------------------------------------------------------
// 文書を組み立てて可視の並びを読み出す補助（`tests/sort_order.rs` と同じ形）
// ---------------------------------------------------------------------------

/// 指定した値の行を持つ文書を組み立てる（列数は `columns`、行は与えられた順に並ぶ）。
///
/// 行の識別子は組み立ての中で発行されるため、この関数の戻り値を使う検査は 1 回の組み立ての
/// 中で完結する。
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

/// 違反の有無を据え付けた順序で、可視の並びを導出して返す。
fn visible_with_presence(
    document: &Document,
    sheet: SheetId,
    spec: &ViewSpec,
    presence: ViolationPresence,
) -> Vec<RowId> {
    let mut order = RowOrder::default();
    order.set_violation_presence(presence);
    order.recompute(document, sheet, spec);
    visible_of(&order)
}

/// 文書の行順の行識別子と、行ごとの値（導出の前後で変わらないことを見るために使う）。
fn document_snapshot(document: &Document, sheet: SheetId) -> (Vec<RowId>, Vec<Vec<CellValue>>) {
    let sheet_ref = document.sheet_by_id(sheet).expect("シートは文書にある");
    (
        sheet_ref.rows().iter().map(|row| row.id()).collect(),
        sheet_ref
            .rows()
            .iter()
            .map(|row| row.values().to_vec())
            .collect(),
    )
}

/// 表示文字列を所有の文字列として取り出す（比較を読みやすくする補助）。
fn rendered(value: &CellValue) -> String {
    display_text(value).into_owned()
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

// ---------------------------------------------------------------------------
// 表示文字列の規則
// ---------------------------------------------------------------------------

/// 変種ごとの表示文字列の規則（正典は `display_text` の docs の表）。
///
/// 絞り込みはこの文字列に対して行うため、ここが崩れれば絞り込みの結果も崩れる。
#[test]
fn the_display_rendering_is_the_documented_one_per_variant() {
    // 値なしは空文字。
    assert_eq!("", rendered(&CellValue::Null));
    // 真偽は `true` / `false`。
    assert_eq!("true", rendered(&CellValue::Bool(true)));
    assert_eq!("false", rendered(&CellValue::Bool(false)));
    // 整数は十進の数字列。
    assert_eq!("0", rendered(&int(0)));
    assert_eq!("-12", rendered(&int(-12)));
    assert_eq!("9223372036854775807", rendered(&int(i64::MAX)));

    // 浮動小数: 最短の往復可能な十進表記であり、**指数表記を使わない**（`{:?}` は `1e21` と
    // 書くため、`{:?}` を規則にしていないことがこの検査で観測できる）。
    assert_eq!("2.5", rendered(&float(2.5)));
    assert_eq!("2", rendered(&float(2.0)));
    assert_eq!("-0.5", rendered(&float(-0.5)));
    let large = rendered(&float(1e21));
    assert!(
        !large.contains('e') && !large.contains('E'),
        "指数表記を使わない: {large}"
    );
    assert_eq!(format!("1{}", "0".repeat(21)), large);
    // `-0.0` は `0` へ畳む（`CellValue` の `PartialEq` が `-0.0 == 0.0` であることに合わせる）。
    assert_eq!("0", rendered(&CellValue::Float(-0.0)));
    // 非有限はそのまま書く（保存の門が拒むため本来は現れないが、復号の門を迂回した値でも
    // 表示が破綻しない。`{}` の規則をそのまま当てる）。
    assert_eq!("NaN", rendered(&CellValue::Float(f64::NAN)));
    assert_eq!("inf", rendered(&CellValue::Float(f64::INFINITY)));
    assert_eq!("-inf", rendered(&CellValue::Float(f64::NEG_INFINITY)));

    // 10 進数は**保持された文字列そのまま**（正規化しない。逐語で往復する契約）。
    assert_eq!("007.50", rendered(&decimal("007.50")));
    assert_eq!("1e3", rendered(&decimal("1e3")));
    assert_eq!("+1.5", rendered(&decimal("+1.5")));
    assert_eq!("", rendered(&decimal("")));

    // テキストはそのもの。
    assert_eq!("りんご", rendered(&text("りんご")));
    assert_eq!("", rendered(&text("")));

    // 入れ子は最上位の要素数の要約（中身は含めない。要件 5.6）。
    assert_eq!(
        "2項目",
        rendered(&object(vec![("a", int(1)), ("b", int(2))]))
    );
    assert_eq!("0項目", rendered(&object(Vec::new())));
    assert_eq!("3要素", rendered(&array(vec![int(1), int(2), int(3)])));
    assert_eq!("0要素", rendered(&array(Vec::new())));

    // 添付は正準の小文字 hex（64 文字）。
    let attachment = AttachmentId::from_bytes(b"filter-order");
    assert_eq!(
        attachment.to_hex(),
        rendered(&CellValue::Attachment(attachment))
    );
    assert_eq!(64, rendered(&CellValue::Attachment(attachment)).len());
}

/// [`DisplayText`] を**直に**通した表示文字列の規則（5.1 が使う型そのもの）。
///
/// [`display_text`] も [`RowOrder::recompute`] の内側も `Text` と `Decimal` を借用で短絡する
/// ため、**その 2 変種の [`DisplayText`] の腕は上のテストからは踏まれない**。5.1 の
/// `WindowCodec` は [`DisplayText`] を直に呼ぶ（モジュール docs「表示文字列の写しは本層が
/// 1 つだけ持つ」）ので、8 変種すべてをこの型を通して固定する。ここが崩れると、画面に出る
/// 文字列と絞り込みが見る文字列が食い違う。
#[test]
fn every_variant_renders_through_display_text_itself() {
    /// [`DisplayText`] を直に書式化する（短絡の経路を通さない）。
    fn via_display_text(value: &CellValue) -> String {
        DisplayText(value).to_string()
    }

    // 値なし。
    assert_eq!("", via_display_text(&CellValue::Null));
    // 真偽。
    assert_eq!("true", via_display_text(&CellValue::Bool(true)));
    assert_eq!("false", via_display_text(&CellValue::Bool(false)));
    // 整数（`i64` の両端）。
    assert_eq!("0", via_display_text(&int(0)));
    assert_eq!("-12", via_display_text(&int(-12)));
    assert_eq!("9223372036854775807", via_display_text(&int(i64::MAX)));
    assert_eq!("-9223372036854775808", via_display_text(&int(i64::MIN)));
    // 浮動小数（指数表記を使わない。`-0.0` は `0`）。
    assert_eq!("2.5", via_display_text(&float(2.5)));
    assert_eq!("-0.5", via_display_text(&float(-0.5)));
    assert!(
        !via_display_text(&float(1e21)).contains('e'),
        "指数表記を使わない"
    );
    assert_eq!("0", via_display_text(&CellValue::Float(-0.0)));
    // 10 進数: **保持された文字列そのまま**（先頭の 0 を落とさず、指数表記も畳まない）。
    assert_eq!("1.50", via_display_text(&decimal("1.50")));
    assert_eq!("1e3", via_display_text(&decimal("1e3")));
    assert_eq!("007.50", via_display_text(&decimal("007.50")));
    assert_eq!("+1.5", via_display_text(&decimal("+1.5")));
    assert_eq!("", via_display_text(&decimal("")));
    // テキスト: そのまま（大文字小文字を畳まない）。
    assert_eq!("Apple", via_display_text(&text("Apple")));
    assert_eq!("りんご", via_display_text(&text("りんご")));
    assert_eq!("", via_display_text(&text("")));
    // 入れ子の要約。
    assert_eq!(
        "2項目",
        via_display_text(&object(vec![("a", int(1)), ("b", int(2))]))
    );
    assert_eq!(
        "3要素",
        via_display_text(&array(vec![int(1), int(2), int(3)]))
    );
    // 添付。
    let attachment = AttachmentId::from_bytes(b"display-text");
    assert_eq!(
        attachment.to_hex(),
        via_display_text(&CellValue::Attachment(attachment))
    );

    // 短絡する経路（`display_text`）と直の経路（`DisplayText`）が**同じ文字列**を返す。
    // 借用で返す変種と [`DisplayText`] の腕の値が食い違わないことの観測である。
    for value in [
        CellValue::Null,
        CellValue::Bool(true),
        int(-12),
        float(2.5),
        decimal("1.50"),
        text("Apple"),
        object(vec![("a", int(1))]),
        array(vec![int(1)]),
        CellValue::Attachment(attachment),
    ] {
        assert_eq!(
            via_display_text(&value),
            display_text(&value).into_owned(),
            "DisplayText と display_text が食い違う: {value:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 5 種の絞り込み
// ---------------------------------------------------------------------------

#[test]
fn each_filter_kind_selects_the_rows_it_describes() {
    let (document, sheet, ids) = build(
        vec![
            vec![text("りんご"), int(1)],
            vec![text("ぶどう"), int(2)],
            vec![text(""), int(3)],
            vec![CellValue::Null, int(4)],
            vec![text("りんご"), int(5)],
        ],
        2,
    );

    // 一致。
    assert_eq!(
        vec![ids[0], ids[4]],
        visible(&document, sheet, &filtered(vec![equals(0, "りんご")])),
        "一致は列 0 の表示文字列が「りんご」である行を選ぶ"
    );
    // 部分一致。
    assert_eq!(
        vec![ids[1]],
        visible(&document, sheet, &filtered(vec![contains(0, "どう")])),
        "部分一致は表示文字列に「どう」を含む行を選ぶ"
    );
    // 値なし（空のテキストと値なしの双方が空の表示文字列になる）。
    assert_eq!(
        vec![ids[2], ids[3]],
        visible(&document, sheet, &filtered(vec![is_empty(0)])),
        "値なしは表示文字列が空の行を選ぶ"
    );
    // 値あり。
    assert_eq!(
        vec![ids[0], ids[1], ids[4]],
        visible(&document, sheet, &filtered(vec![is_not_empty(0)])),
        "値ありは表示文字列が空でない行を選ぶ"
    );
    // 違反あり: 違反の情報が据え付けられていないため、どの行にも一致しない。
    assert_eq!(
        Vec::<RowId>::new(),
        visible(&document, sheet, &filtered(vec![any_violation()])),
        "違反の情報が無いとき、違反ありはどの行にも一致しない"
    );

    // 違反あり: 1 行だけに据え付けると、その行だけが可視になる。
    let mut presence = ViolationPresence::new();
    assert!(presence.is_empty());
    presence.mark_row(ids[3]);
    assert_eq!(1, presence.len());
    assert!(!presence.is_empty());
    assert!(presence.has_any(ids[3]));
    assert!(
        !presence.has_column(ids[3], ColumnIndex::new(0)),
        "どの列かは与えられていないため、列を指定した絞り込みには一致しない"
    );
    assert_eq!(
        vec![ids[3]],
        visible_with_presence(&document, sheet, &filtered(vec![any_violation()]), presence),
    );
}

// ---------------------------------------------------------------------------
// 積（AND）
// ---------------------------------------------------------------------------

#[test]
fn multiple_filters_compose_as_a_product() {
    let (document, sheet, ids) = build(
        vec![
            vec![text("a"), int(1)],
            vec![text("a"), int(2)],
            vec![text("b"), int(2)],
            vec![text("b"), int(1)],
            vec![text("ab"), int(2)],
        ],
        2,
    );

    // 積: 列 0 が "a" かつ 列 1 が "2" である行だけ。
    assert_eq!(
        vec![ids[1]],
        visible(
            &document,
            sheet,
            &filtered(vec![equals(0, "a"), equals(1, "2")])
        ),
        "複数の絞り込みは積である"
    );
    // 部分一致と一致の積。
    assert_eq!(
        vec![ids[1], ids[4]],
        visible(
            &document,
            sheet,
            &filtered(vec![contains(0, "a"), equals(1, "2")])
        ),
        "部分一致も積の 1 項である"
    );
    // 絞り込みの並び順は集合を変えない（積は可換である）。
    assert_eq!(
        visible(
            &document,
            sheet,
            &filtered(vec![equals(1, "2"), contains(0, "a")])
        ),
        visible(
            &document,
            sheet,
            &filtered(vec![contains(0, "a"), equals(1, "2")])
        ),
        "積の結果は絞り込みの並び順に依らない"
    );
    // 空の積: 同じ列に同時に成り立たない 2 つの一致。
    let mut order = RowOrder::default();
    let summary = order.recompute(
        &document,
        sheet,
        &filtered(vec![equals(0, "a"), equals(0, "b")]),
    );
    assert_eq!(
        ViewSummary {
            visible: 0,
            hidden: 5
        },
        summary
    );
    assert_eq!(5, summary.visible + summary.hidden);
    assert!(order.is_empty());
}

// ---------------------------------------------------------------------------
// 可視行数と隠れた行数
// ---------------------------------------------------------------------------

#[test]
fn hidden_is_the_complement_of_visible_and_the_two_sum_to_the_row_count() {
    let (document, sheet, ids) = build(
        vec![
            vec![text("a"), int(1)],
            vec![text("a"), int(2)],
            vec![text("b"), int(3)],
            vec![text("b"), int(4)],
            vec![text("a"), int(5)],
        ],
        2,
    );
    let sheet_rows = document
        .sheet_by_id(sheet)
        .expect("シートは文書にある")
        .rows()
        .len();

    // 3 種の指定について、**数ではなく恒等式**（可視 + 隠れ = 行数）を検査する。
    for spec in [
        ViewSpec::default(),
        filtered(vec![equals(0, "a")]),
        view(&[(1, true)], vec![contains(0, "a")]),
        filtered(vec![equals(0, "該当なし")]),
    ] {
        let mut order = RowOrder::default();
        let summary = order.recompute(&document, sheet, &spec);
        assert_eq!(
            sheet_rows,
            summary.visible + summary.hidden,
            "可視と隠れた行の和が文書の行数と一致しない: {spec:?}"
        );
        assert_eq!(
            summary.visible,
            order.len(),
            "要約と順序の可視行数が食い違う"
        );
        assert_eq!(
            summary.hidden,
            order.hidden(),
            "要約と順序の隠れた行数が食い違う"
        );
        // 可視行数を**序数を辿って**独立に数える（要約の数をそのまま読み返さない）。
        // `visible_of` は `order.len()` を区間の長さに渡すため、`span` の切り落としに依る
        // だけの比較になり得る。`row_at` を `None` まで辿った数は順序そのものから出る。
        let reached = (0..)
            .take_while(|n| order.row_at(RowOrdinal::new(*n)).is_some())
            .count();
        assert_eq!(
            summary.visible, reached,
            "可視行数と row_at で辿れる序数の数が食い違う: {spec:?}"
        );
        assert_eq!(
            None,
            order.row_at(RowOrdinal::new(reached)),
            "可視行数の序数は引けない（可視行がその数を超えている）"
        );
    }

    // 絞り込み 1 本（"a" の 3 行）の可視・隠れの内訳。
    let mut order = RowOrder::default();
    let summary = order.recompute(&document, sheet, &filtered(vec![equals(0, "a")]));
    assert_eq!(
        ViewSummary {
            visible: 3,
            hidden: 2
        },
        summary
    );
    assert_eq!(sheet_rows, summary.visible + summary.hidden);
    assert_eq!(vec![ids[0], ids[1], ids[4]], visible_of(&order));
}

#[test]
fn an_empty_filter_list_is_the_identity() {
    let (document, sheet, ids) = build(vec![vec![int(2)], vec![int(1)]], 1);

    // 絞り込みも並べ替えも 0 件: 全行が可視であり、文書の行順のまま。
    let mut order = RowOrder::default();
    let summary = order.recompute(&document, sheet, &ViewSpec::default());
    assert_eq!(
        ViewSummary {
            visible: 2,
            hidden: 0
        },
        summary
    );
    assert_eq!(2, summary.visible + summary.hidden);
    assert_eq!(
        ids,
        visible_of(&order),
        "絞り込みが 0 件なら全行が可視であり、文書の行順を保つ"
    );

    // 並べ替えだけを指定しても隠れた行は 0（絞り込みが恒等である）。
    let summary = order.recompute(&document, sheet, &view(&[(0, false)], Vec::new()));
    assert_eq!(
        ViewSummary {
            visible: 2,
            hidden: 0
        },
        summary
    );
    assert_eq!(2, summary.visible + summary.hidden);
    assert_eq!(vec![ids[1], ids[0]], visible_of(&order));

    // 絞り込みだけを指定しても、可視の並びは文書の行順の部分列である。
    let summary = order.recompute(&document, sheet, &filtered(vec![is_not_empty(0)]));
    assert_eq!(
        ViewSummary {
            visible: 2,
            hidden: 0
        },
        summary
    );
    assert_eq!(ids, visible_of(&order));
}

// ---------------------------------------------------------------------------
// 絞り込みと並べ替えの同時指定
// ---------------------------------------------------------------------------

#[test]
fn the_order_is_derived_over_the_filtered_set() {
    let (document, sheet, ids) = build(
        vec![
            vec![text("x"), int(3)],
            vec![text("x"), int(1)],
            vec![text("y"), int(2)],
            vec![text("x"), int(2)],
        ],
        2,
    );

    // 絞り込んだ集合 {ids[0], ids[1], ids[3]} を列 1 の昇順で並べる（値は 3, 1, 2）。
    let sorted = view(&[(1, false)], vec![equals(0, "x")]);
    assert_eq!(
        vec![ids[1], ids[3], ids[0]],
        visible(&document, sheet, &sorted),
        "順序は絞り込んだ集合に対して定まる"
    );
    // 除外された行は順序に現れない。除外される行の値（列 1 が 2 の ids[2]）は、含まれる行の
    // 値の**間**にある — 絞り込みを無視して全行を並べ替える実装なら、この行が可視の並びに
    // 現れてしまう。
    let sorted_rows = visible(&document, sheet, &sorted);
    assert!(
        !sorted_rows.contains(&ids[2]),
        "絞り込みで除外された行が可視の並びに現れた"
    );
    // 含まれる行は**値の順**に並ぶ。期待は手で書かず、保存された値から導く。
    let mut expected: Vec<(i64, RowId)> = ids
        .iter()
        .zip(document.sheet_by_id(sheet).expect("シート").rows())
        .filter(|(_, row)| matches!(row.values().first(), Some(CellValue::Text(v)) if v == "x"))
        .map(|(id, row)| match row.values().get(1) {
            Some(CellValue::Int(value)) => (*value, *id),
            other => panic!("列 1 は整数である: {other:?}"),
        })
        .collect();
    // 前提: 期待が空でなく、除外される行が期待に含まれない（この検査が空にならない）。
    assert_eq!(
        3,
        expected.len(),
        "前提が崩れた: 絞り込みに一致する行が少ない"
    );
    expected.sort();
    let expected: Vec<RowId> = expected.into_iter().map(|(_, id)| id).collect();
    assert_eq!(expected, sorted_rows, "含まれる行が値の順に並んでいない");
    assert_eq!(
        Some(&CellValue::Int(2)),
        document.sheet_by_id(sheet).expect("シート").rows()[2]
            .values()
            .get(1),
        "前提が崩れた: 除外される行の値が含まれる行の値の間に入っていない"
    );

    // 絞り込みだけなら文書の行順（3, 1, 2 の出現順）であり、上の結果とは**異なる**。
    // この不一致があるから、絞り込みを無視した実装はこの検査で落ちる。
    let only_filter = filtered(vec![equals(0, "x")]);
    assert_eq!(
        vec![ids[0], ids[1], ids[3]],
        visible(&document, sheet, &only_filter)
    );
    assert_ne!(
        visible(&document, sheet, &only_filter),
        visible(&document, sheet, &sorted),
        "並べ替えが効いていない（文書の行順のままである）"
    );

    // 降順にすると、絞り込んだ集合が値の降順になる。
    let descending = view(&[(1, true)], vec![equals(0, "x")]);
    assert_eq!(
        vec![ids[0], ids[3], ids[1]],
        visible(&document, sheet, &descending)
    );

    // 絞り込みと並べ替えを同時に指定したときも、可視と隠れの和は行数に一致する。
    let mut order = RowOrder::default();
    let summary = order.recompute(&document, sheet, &sorted);
    assert_eq!(
        ViewSummary {
            visible: 3,
            hidden: 1
        },
        summary
    );
    assert_eq!(4, summary.visible + summary.hidden);
}

#[test]
fn a_filter_that_excludes_everything_leaves_no_visible_rows() {
    let (document, sheet, ids) = build(vec![vec![text("a")], vec![text("b")], vec![text("a")]], 1);
    let mut order = RowOrder::default();
    let summary = order.recompute(&document, sheet, &filtered(vec![equals(0, "該当なし")]));

    assert_eq!(
        ViewSummary {
            visible: 0,
            hidden: 3
        },
        summary
    );
    assert_eq!(3, summary.visible + summary.hidden);
    assert_eq!(0, order.len());
    assert!(order.is_empty());
    assert_eq!(3, order.hidden());
    // どの序数も引けない。
    for ordinal in 0..4 {
        assert_eq!(
            None,
            order.row_at(RowOrdinal::new(ordinal)),
            "序数 {ordinal}"
        );
    }
    assert_eq!(None, order.ordinal_of(ids[0]));
    // 区間は常に空である。
    assert!(order.span(RowSpan::new(RowOrdinal::new(0), 3)).is_empty());
    assert!(order.span(RowSpan::new(RowOrdinal::new(0), 0)).is_empty());
    assert!(order.span(RowSpan::new(RowOrdinal::new(2), 5)).is_empty());

    // すべてを除外した後に別の指定へ戻すと、元の行が再び可視になる（導出は入力だけから決まる）。
    let summary = order.recompute(&document, sheet, &filtered(vec![equals(0, "a")]));
    assert_eq!(
        ViewSummary {
            visible: 2,
            hidden: 1
        },
        summary
    );
    assert_eq!(vec![ids[0], ids[2]], visible_of(&order));
}

// ---------------------------------------------------------------------------
// 値なし・値あり
// ---------------------------------------------------------------------------

#[test]
fn is_empty_and_is_not_empty_follow_the_rendering_rule() {
    let (document, sheet, ids) = build(
        vec![
            vec![CellValue::Null, int(1)],
            vec![text(""), int(2)],
            vec![text("x"), int(3)],
            vec![int(0)],
            vec![],
        ],
        2,
    );

    // 値なし = 表示文字列が空。値なし・空のテキスト・値の数が届かない行が同じ結果になる。
    assert_eq!(
        vec![ids[0], ids[1], ids[4]],
        visible(&document, sheet, &filtered(vec![is_empty(0)])),
        "値なしは Null・空のテキスト・届かない値のいずれも選ぶ"
    );
    assert_eq!(
        vec![ids[2], ids[3]],
        visible(&document, sheet, &filtered(vec![is_not_empty(0)])),
        "値ありは空でない表示文字列を持つ行を選ぶ（Int(0) の表示は \"0\" であり空でない）"
    );

    // 値の数が列の添字に届かない行は値なしとして扱う（2.1 と同じ規約）。
    assert_eq!(
        vec![ids[3], ids[4]],
        visible(&document, sheet, &filtered(vec![is_empty(1)])),
        "列 1 の値を持たない行は値なしである"
    );
    assert_eq!(
        vec![ids[0], ids[1], ids[2]],
        visible(&document, sheet, &filtered(vec![is_not_empty(1)]))
    );

    // 値なしは**空文字との一致と常に同じ行を選ぶ**（規則が食い違っていないことの観測）。
    for column in 0..2 {
        assert_eq!(
            visible(&document, sheet, &filtered(vec![is_empty(column)])),
            visible(&document, sheet, &filtered(vec![equals(column, "")])),
            "列 {column}: 値なしは空文字との一致と同じ行を選ぶ"
        );
        // 値ありは値なしの補集合である。
        let mut empty = visible(&document, sheet, &filtered(vec![is_empty(column)]));
        let mut not_empty = visible(&document, sheet, &filtered(vec![is_not_empty(column)]));
        empty.sort();
        not_empty.sort();
        let mut both: Vec<RowId> = empty.into_iter().chain(not_empty).collect();
        both.sort();
        let mut all = ids.clone();
        all.sort();
        assert_eq!(all, both, "列 {column}: 値なしと値ありの和が全行である");
    }

    // 部分一致に空文字を与えると全行に一致する（空文字列はすべての文字列の部分列である）。
    assert_eq!(
        ids,
        visible(&document, sheet, &filtered(vec![contains(0, "")])),
        "空文字の部分一致は全行に一致する"
    );
}

// ---------------------------------------------------------------------------
// 一致・部分一致は表示文字列に対して行う
// ---------------------------------------------------------------------------

#[test]
fn equals_compares_the_rendering_of_the_value_not_the_value() {
    let (document, sheet, ids) = build(vec![vec![int(10)], vec![int(9)], vec![int(100)]], 1);

    // **表示文字列の完全一致である。**`"9"` は `Int(9)` に一致し、`Int(10)`（表示は `"10"`）には
    // 一致しない。値の等値で比べる実装なら `Int(10)` と `9` が数値として一致してしまう。
    assert_eq!(
        vec![ids[1]],
        visible(&document, sheet, &filtered(vec![equals(0, "9")])),
        "\"9\" は Int(10) に一致しない"
    );
    assert_eq!(
        vec![ids[0]],
        visible(&document, sheet, &filtered(vec![equals(0, "10")]))
    );
    // 表示文字列そのものの一致であり、数の表記の揺れは受け付けない。
    assert!(visible(&document, sheet, &filtered(vec![equals(0, "09")])).is_empty());
    assert!(visible(&document, sheet, &filtered(vec![equals(0, "9.0")])).is_empty());
    assert!(visible(&document, sheet, &filtered(vec![equals(0, " 9")])).is_empty());

    // 部分一致も表示文字列に対して行う。
    assert_eq!(
        vec![ids[0], ids[2]],
        visible(&document, sheet, &filtered(vec![contains(0, "0")])),
        "表示文字列 \"10\" と \"100\" は \"0\" を含み、\"9\" は含まない"
    );
    assert_eq!(
        vec![ids[0], ids[2]],
        visible(&document, sheet, &filtered(vec![contains(0, "1")]))
    );
    assert_eq!(
        vec![ids[1]],
        visible(&document, sheet, &filtered(vec![contains(0, "9")])),
        "表示文字列 \"9\" だけが \"9\" を含む"
    );

    // 真偽は `true` / `false` として比較する。
    let (document, sheet, ids) = build(
        vec![vec![CellValue::Bool(true)], vec![CellValue::Bool(false)]],
        1,
    );
    assert_eq!(
        vec![ids[0]],
        visible(&document, sheet, &filtered(vec![equals(0, "true")]))
    );
    assert_eq!(
        vec![ids[1]],
        visible(&document, sheet, &filtered(vec![equals(0, "false")]))
    );
    assert!(
        visible(&document, sheet, &filtered(vec![equals(0, "True")])).is_empty(),
        "大文字小文字は区別する"
    );
    assert!(visible(&document, sheet, &filtered(vec![equals(0, "1")])).is_empty());

    // テキストも大文字小文字を区別する（バイト列の一致であり、地域の並び替え規則を使わない）。
    let (document, sheet, ids) = build(vec![vec![text("Apple")], vec![text("apple")]], 1);
    assert_eq!(
        vec![ids[0]],
        visible(&document, sheet, &filtered(vec![equals(0, "Apple")]))
    );
    assert_eq!(
        vec![ids[1]],
        visible(&document, sheet, &filtered(vec![equals(0, "apple")]))
    );
    assert!(
        visible(&document, sheet, &filtered(vec![contains(0, "PP")])).is_empty(),
        "部分一致も大文字小文字を区別する（\"Apple\" も \"apple\" も \"PP\" を含まない）"
    );
    assert_eq!(
        vec![ids[0]],
        visible(&document, sheet, &filtered(vec![contains(0, "App")])),
        "部分一致は表示文字列のそのままの並びを探す"
    );
    assert_eq!(
        vec![ids[1]],
        visible(&document, sheet, &filtered(vec![contains(0, "app")]))
    );
    // 10 進数は保持された文字列そのままで比較する（正規化しない）。
    let (document, sheet, ids) = build(vec![vec![decimal("007.50")], vec![decimal("7.5")]], 1);
    assert_eq!(
        vec![ids[0]],
        visible(&document, sheet, &filtered(vec![equals(0, "007.50")])),
        "10 進数は保持された文字列そのままで比較する"
    );
    assert_eq!(
        vec![ids[1]],
        visible(&document, sheet, &filtered(vec![equals(0, "7.5")]))
    );
}

// ---------------------------------------------------------------------------
// 違反あり
// ---------------------------------------------------------------------------

#[test]
fn has_violation_matches_nothing_until_a_presence_is_installed() {
    let (document, sheet, ids) = build(
        vec![
            vec![int(1), int(4)],
            vec![int(3), int(4)],
            vec![int(5), int(6)],
            vec![int(7), int(8)],
        ],
        2,
    );

    // 既定（据え付けなし）では、列を問わない絞り込みも列を指定した絞り込みも一致しない。
    let mut order = RowOrder::default();
    let summary = order.recompute(&document, sheet, &filtered(vec![any_violation()]));
    assert_eq!(
        ViewSummary {
            visible: 0,
            hidden: 4
        },
        summary,
        "違反の情報が無いときはどの行も可視にならない"
    );
    assert_eq!(4, summary.visible + summary.hidden);
    assert!(order.is_empty());
    assert!(order.violation_presence().is_empty());
    let summary = order.recompute(&document, sheet, &filtered(vec![column_violation(0)]));
    assert_eq!(
        ViewSummary {
            visible: 0,
            hidden: 4
        },
        summary
    );

    // 据え付けた後は、列を問わない絞り込みが違反を持つ行を選ぶ（行の印と列の印の双方）。
    assert_eq!(
        vec![ids[0], ids[1]],
        visible_with_presence(
            &document,
            sheet,
            &filtered(vec![any_violation()]),
            presence_two_rows(ids[0], ids[1]),
        ),
        "違反ありは違反を持つ行を選ぶ"
    );
    // 列を指定した絞り込みは**その列の違反を持つ行**だけを選ぶ。行の印（どの列かが
    // 与えられていない違反）は列を指定した絞り込みには一致しない。
    assert_eq!(
        vec![ids[1]],
        visible_with_presence(
            &document,
            sheet,
            &filtered(vec![column_violation(1)]),
            presence_two_columns(ids[0], ids[1]),
        ),
        "列を指定した違反ありはその列の違反を持つ行だけを選ぶ（行の印は一致しない）"
    );
    // 列を問わない絞り込みは、行の印と列の印の**双方**に一致する。
    assert_eq!(
        vec![ids[0], ids[1]],
        visible_with_presence(
            &document,
            sheet,
            &filtered(vec![any_violation()]),
            presence_two_columns(ids[0], ids[1]),
        ),
        "列を問わない違反ありは列の印にも一致する"
    );
    // 違反を持たない行はどの絞り込みでも選ばれない。
    let visible_rows = visible_with_presence(
        &document,
        sheet,
        &filtered(vec![any_violation()]),
        presence_two_columns(ids[0], ids[1]),
    );
    assert_eq!(
        Vec::<RowId>::new(),
        visible_rows
            .iter()
            .copied()
            .filter(|row| *row == ids[2] || *row == ids[3])
            .collect::<Vec<RowId>>(),
        "違反を持たない行は選ばれない"
    );
    assert_eq!(
        vec![ids[0], ids[1]],
        visible_rows,
        "違反を持つ 2 行だけが可視になる"
    );

    // 積: 違反ありと別の絞り込みを重ねられる（列 1 が "4" である行は ids[0] と ids[1]）。
    assert_eq!(
        vec![ids[1]],
        visible_with_presence(
            &document,
            sheet,
            &filtered(vec![column_violation(1), equals(1, "4")]),
            presence_two_columns(ids[0], ids[1]),
        ),
        "違反ありも積の 1 項である"
    );

    // 据え付けは順序の状態であり、導出のたびに消えない。
    let mut order = RowOrder::default();
    order.set_violation_presence(presence_two_columns(ids[0], ids[1]));
    assert_eq!(2, order.violation_presence().len());
    order.recompute(&document, sheet, &filtered(vec![any_violation()]));
    assert_eq!(
        vec![ids[0], ids[1]],
        visible_of(&order),
        "据え付けは導出のたびに消えない"
    );
    // 新しい据え付けは古い据え付けを**丸ごと置き換える**（差分を積まない）。
    order.set_violation_presence(presence_two_rows(ids[3], ids[3]));
    order.recompute(&document, sheet, &filtered(vec![any_violation()]));
    assert_eq!(
        vec![ids[3]],
        visible_of(&order),
        "新しい据え付けが古い据え付けを置き換える"
    );

    // 据え付けは文書を書き換えない（違反の情報は表示の状態である）。
    let before = document_snapshot(&document, sheet);
    let mut order = RowOrder::default();
    order.set_violation_presence(presence_two_columns(ids[0], ids[1]));
    order.recompute(&document, sheet, &filtered(vec![column_violation(1)]));
    assert_eq!(before, document_snapshot(&document, sheet));
}

/// 行 `first` を「列を問わない違反あり」、行 `second` を列 1 の違反ありとして据え付ける。
fn presence_two_columns(first: RowId, second: RowId) -> ViolationPresence {
    let mut presence = ViolationPresence::new();
    presence.mark_row(first);
    presence.mark_column(second, ColumnIndex::new(1));
    presence
}

/// 2 つの行を「列を問わない違反あり」として据え付ける。
fn presence_two_rows(first: RowId, second: RowId) -> ViolationPresence {
    let mut presence = ViolationPresence::new();
    presence.mark_row(first);
    presence.mark_row(second);
    presence
}

// ---------------------------------------------------------------------------
// 決定性と、文書を書き換えないこと
// ---------------------------------------------------------------------------

#[test]
fn repeated_derivations_with_filters_are_identical() {
    let (document, sheet, _) = build(
        vec![
            vec![text("a"), int(1)],
            vec![text("b"), int(1)],
            vec![text("a"), int(2)],
            vec![text("b"), int(2)],
            vec![text("a"), int(1)],
        ],
        2,
    );
    let spec = view(
        &[(1, false), (0, true)],
        vec![contains(0, "a"), is_not_empty(1)],
    );

    let first = visible(&document, sheet, &spec);
    let second = visible(&document, sheet, &spec);
    assert_eq!(first, second, "同一の入力から同一の可視の並びが出ない");

    // 別の指定で上書きした後に戻しても同じ（導出は呼び出しの履歴に依らない）。
    let mut order = RowOrder::default();
    order.recompute(&document, sheet, &ViewSpec::default());
    order.recompute(&document, sheet, &filtered(vec![equals(0, "b")]));
    order.recompute(&document, sheet, &spec);
    assert_eq!(first, visible_of(&order));

    // 違反の据え付けを挟んでも、据え付けが同じなら同じ並びが出る。
    let mut order = RowOrder::default();
    order.set_violation_presence(ViolationPresence::new());
    order.recompute(&document, sheet, &spec);
    assert_eq!(
        first,
        visible_of(&order),
        "据え付けが空でも絞り込みの結果は同じ"
    );
}

#[test]
fn filtering_only_borrows_the_document_and_leaves_it_unchanged() {
    let (document, sheet, ids) = build(
        vec![
            vec![text("a"), int(1)],
            vec![text("b"), int(2)],
            vec![text("a"), int(3)],
        ],
        2,
    );
    let before = document_snapshot(&document, sheet);
    let spec = view(&[(1, true)], vec![equals(0, "a")]);

    // ドキュメントの**共有借用を生かしたまま**導出する。`&mut Document` を取る形ならここで
    // 借用が衝突し、このテストはコンパイルに失敗する（要件 8.5 の型の上の証明）。
    let sheet_ref = document.sheet_by_id(sheet).expect("シートは文書にある");
    let mut order = RowOrder::default();
    let summary = order.recompute(&document, sheet, &spec);

    assert_eq!(
        ViewSummary {
            visible: 2,
            hidden: 1
        },
        summary
    );
    assert_eq!(vec![ids[2], ids[0]], visible_of(&order));
    // 同じ共有借用で読める（可視行は行そのものの順ではない）。
    assert_eq!(3, sheet_ref.rows().len());
    let after = document_snapshot(&document, sheet);
    assert_eq!(
        before, after,
        "ドキュメントの行の並びまたは値が導出で変わった"
    );

    // 値なしの絞り込みでも同じ。
    let mut order = RowOrder::default();
    order.set_violation_presence(ViolationPresence::new());
    order.recompute(&document, sheet, &filtered(vec![is_empty(0)]));
    assert!(order.is_empty());
    assert_eq!(before, document_snapshot(&document, sheet));
}

#[test]
fn the_shared_sample_filters_by_the_values_of_a_column() {
    let sample = sample(&SampleOptions::new(512, 5).with_ratio(0.0));
    let columns = sample.column_count();

    // 標本の値を写して自分の文書を組む（`Sample` は文書を可変で貸さないため、行順を
    // `RowId` の順と食い違わせるには自分で組む必要がある。`tests/sort_order.rs` と同じ手順）。
    let (mut document, sheet, ids) = build(sample.row_values(), columns);

    // 前提: 列 4（検査済み）は真偽であり、真の行が複数ある（この検査が空にならない）。
    let trues: Vec<RowId> = ids
        .iter()
        .zip(document.sheet_by_id(sheet).expect("シート").rows())
        .filter(|(_, row)| matches!(row.values().get(4), Some(CellValue::Bool(true))))
        .map(|(id, _)| *id)
        .collect();
    assert!(
        trues.len() >= 2,
        "前提が崩れた: 真の行が少ない（{} 行）— 絞り込みを検査できない",
        trues.len()
    );

    // **行順を `RowId` の順と食い違わせる。**これが無いと「文書の行順を保つ」結果と
    // 「`RowId` の順に並べ直す」結果が一致してしまい、絞り込みの分岐が
    // `RowId` で並べ替えてもこの検査が通る（標本の文書の行順は `RowId` の順と一致するため）。
    let mut reversed = ids.clone();
    reversed.reverse();
    document
        .reorder_rows(sheet, &reversed)
        .expect("行順を置き換えられない");
    let mut sorted_ids = ids.clone();
    sorted_ids.sort();
    assert_eq!(
        sorted_ids, ids,
        "前提が崩れた: 文書の行順が RowId の順でない"
    );
    assert_ne!(
        reversed, sorted_ids,
        "前提が崩れた: 反転した行順が RowId の順と一致してしまった（区別できない）"
    );

    // 真の行だけが可視になり、可視と隠れの和は標本の行数に一致する。
    let mut order = RowOrder::default();
    let summary = order.recompute(&document, sheet, &filtered(vec![equals(4, "true")]));
    assert_eq!(trues.len(), summary.visible);
    assert_eq!(sample.rows(), summary.visible + summary.hidden);
    assert_eq!(sample.rows() - trues.len(), summary.hidden);

    // 可視の並びは**反転した文書の行順の部分列**である（`RowId` の順ではない）。
    let expected: Vec<RowId> = reversed
        .iter()
        .copied()
        .filter(|id| trues.contains(id))
        .collect();
    let actual = visible_of(&order);
    assert_eq!(
        expected, actual,
        "絞り込みは文書の行順の部分列を返す（RowId の順に並べ直していない）"
    );
    // 期待と `RowId` の順に並べたものは**異なる**。この不一致があるから、絞り込みの分岐が
    // `RowId` で並べ替える実装はここで落ちる（前提の `assert_ne!` がそれを保証する）。
    let mut by_row_id = expected.clone();
    by_row_id.sort();
    assert_ne!(
        by_row_id, expected,
        "反転した行順の部分列が RowId の順の部分列と一致してしまった（検査が空である）"
    );
    // 偽の行は 1 つも現れない。
    assert!(
        actual.iter().all(|id| trues.contains(id)),
        "絞り込みで除外された行が可視の並びに現れた"
    );

    // 積: 真であり、かつ列 0（品番）が標本の接頭辞 P で始まる行。
    // **前提を自分で表明する**: 違反を混ぜていない（割合 0）ため列 0 は適合する値だけであり、
    // その値はすべて接頭辞 `P` で始まる（`tests/common/sample.rs` の列 0 の規則）。
    // この前提が崩れると、下の `contains(0, "P")` が真の行を取り落として数が合わなくなる。
    let every_part_starts_with_p: bool = document
        .sheet_by_id(sheet)
        .expect("シート")
        .rows()
        .iter()
        .all(|row| matches!(row.values().first(), Some(CellValue::Text(v)) if v.starts_with('P')));
    assert!(
        every_part_starts_with_p,
        "前提が崩れた: 品番が P で始まらない"
    );
    let mut order = RowOrder::default();
    order.recompute(
        &document,
        sheet,
        &filtered(vec![equals(4, "true"), contains(0, "P")]),
    );
    assert_eq!(trues.len(), order.len(), "品番はすべて P で始まる");
    // 偽の行を含む積は空になる（真かつ偽は成り立たない）。
    let mut order = RowOrder::default();
    let summary = order.recompute(
        &document,
        sheet,
        &filtered(vec![equals(4, "true"), equals(4, "false")]),
    );
    assert_eq!(0, summary.visible);
    assert_eq!(sample.rows(), summary.hidden);
    assert_eq!(sample.rows(), summary.visible + summary.hidden);
}

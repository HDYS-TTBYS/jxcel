//! クレート外から見た横断する座標の型と誤り型（データグリッドのタスク 1.3。data-grid 要件
//! 1.6, 2.3, 2.5, 4.5）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。`data_grid::` 直下の
//! 再エクスポート（`RowOrdinal` / `RowSpan` / `CellPosition` / `CellAddress` / `CellRange` /
//! `NestedPath` / `NestedPathSegment` / `SearchDirection` / `GridError` と、上流
//! `schema-engine` の `ColumnIndex`）がクレート外から見えることをコンパイル時に示す。
//!
//! # 何を固定するか
//!
//! * **可視行の序数と物理の行の位置が型の上で別であること**。`RowOrdinal` は `usize` / `RowId` /
//!   `ColumnIndex` のいずれとも別の型であり、素の数との行き来には `new` / `get` の明示が要る
//!   （取り違えると、絞り込みが効いている間に**別の行を編集する**。要件 8.6）
//! * `RowOrdinal` が `Copy` / `Ord` / `Hash` を持つこと（序数は並べ替えの鍵であり、
//!   区間の端であり、索引の鍵である）
//! * `RowSpan` が**半開区間**（開始を含み、終端を含まない）であること。`contains` の境界、
//!   空の区間が何も含まないこと、可視 1 行目と最終行という両端の場合
//! * `CellRange` の角の正規化（与えられた順に依らず、軸ごとに独立して昇順へ揃う）と、
//!   単一セル・単一行・単一列・矩形のそれぞれの**行数・列数・セル数**（要件 2.5 の数）
//! * 範囲の数は**序数だけ**で決まり、その序数を占める物理行が入れ替わっても変わらないこと。
//!   一方でセルの位置（物理の同一性）は `RowId` で決まること
//! * `NestedPath` が上流 `schema_engine::ValuePath` の経路を写すこと、上流の空の経路
//!   （セル直下）が根へ写ること（要件 4.5 の「入れ子のどの位置が違反しているか」）
//! * `SearchDirection` が 2 つの区別できる変種を持つこと。**向きの意味はここでは固定しない**
//!   （`find_violation` が実装される群 2 のタスク 2.4 で初めて観測できる。`SearchDirection`
//!   の doc を参照）
//! * `GridError` の 5 変種が判別可能であり、**値の不適合の変種を持たない**こと。
//!   下の `match` は 5 変種をすべて列挙しているため、6 つ目の変種が足されれば
//!   この統合テストがまずコンパイルに失敗する（これが不変条件の固定である）
//!
//! 座標の値そのもの（`RowOrdinal::new(3).get() == 3` のような往復）はここで固定しない。
//! 固定するのは**空間の違いが観測に出る形**である（序数は表示空間、行識別子は文書の位置）。

use std::any::TypeId;
use std::collections::HashSet;

use data_grid::{
    CellAddress, CellPosition, CellRange, ColumnIndex, GridError, NestedPath, NestedPathSegment,
    RowOrdinal, RowSpan, SearchDirection,
};
use document_format::{IdFactory, RowId};
use schema_engine::ValuePath;

/// 表示 1 マスぶんの角を短く書く。
fn at(row: usize, column: usize) -> CellPosition {
    CellPosition::new(RowOrdinal::new(row), ColumnIndex::new(column))
}

#[test]
fn row_ordinal_is_a_distinct_type_from_a_physical_row_and_a_column() {
    // 型の区別は値ではなく型そのものの性質である（`TypeId` は `'static` な型に対して取れる）。
    assert_ne!(TypeId::of::<RowOrdinal>(), TypeId::of::<usize>());
    assert_ne!(TypeId::of::<RowOrdinal>(), TypeId::of::<RowId>());
    assert_ne!(TypeId::of::<RowOrdinal>(), TypeId::of::<ColumnIndex>());
    assert_ne!(TypeId::of::<RowId>(), TypeId::of::<usize>());

    // 序数と素の `usize` の行き来には `new` / `get` の明示が要る。次の形はどれも
    // コンパイルが通らない（`From` / `Into` を実装していないため）:
    //     let ordinal: RowOrdinal = 3_usize.into();
    //     let raw: usize = ordinal.into();
    //     let address = CellAddress::new(ordinal, ColumnIndex::new(0));  // 行は `RowId` を要求する
    let ordinal = RowOrdinal::new(3);
    assert_eq!(ordinal.get(), 3);
    let raw: usize = ordinal.get();
    assert_eq!(RowOrdinal::new(raw), ordinal);

    // 列の添字も別の型である。値（どちらも `3`）ではなく**型そのもの**が違うことを見る。
    let column = ColumnIndex::new(3);
    assert_eq!(column.index(), ordinal.get(), "値は同じ 3 である");
    assert_ne!(
        TypeId::of::<ColumnIndex>(),
        TypeId::of::<RowOrdinal>(),
        "値が同じでも型が違う（取り違えは型検査で止まる）"
    );
    assert_ne!(TypeId::of::<ColumnIndex>(), TypeId::of::<RowId>());

    // 取り違えが実害になる形: 絞り込みが効いていると、同じ添字が**別の物理行**を指す。
    let mut ids = IdFactory::new();
    let all: Vec<RowId> = (0..5).map(|_| ids.new_row_id()).collect();
    let visible: Vec<RowId> = vec![all[0], all[2], all[4]]; // 2 行目と 4 行目は隠れている

    let second_visible = RowOrdinal::new(1);
    let by_ordinal = visible[second_visible.get()]; // 可視の 2 行目 = all[2]
    let by_position = all[second_visible.get()]; // 物理の 2 行目 = all[1]
    assert_ne!(by_ordinal, by_position);
    assert_eq!(by_ordinal, all[2]);
    assert_eq!(CellAddress::new(by_ordinal, column).row(), all[2]);
}

#[test]
fn column_index_is_the_upstream_type_itself_not_a_second_definition() {
    // 本クレートが `ColumnIndex` を定義し直していれば、この 2 つの `TypeId` は異なる。
    assert_eq!(
        TypeId::of::<data_grid::ColumnIndex>(),
        TypeId::of::<schema_engine::ColumnIndex>()
    );

    // 同じ型であることの実害のない確認: 相互に代入できる（別の型ならコンパイルが通らない）。
    let via_grid = data_grid::ColumnIndex::new(1);
    let via_upstream: schema_engine::ColumnIndex = via_grid;
    assert_eq!(via_upstream.index(), 1);
    assert_eq!(data_grid::ColumnIndex::new(via_upstream.index()), via_upstream);
}

#[test]
fn row_ordinal_is_ordered_copyable_and_hashable() {
    let mut ordinals = vec![RowOrdinal::new(2), RowOrdinal::new(0), RowOrdinal::new(1)];
    ordinals.sort(); // `Ord` が要る（序数は並べ替えの鍵である）
    assert_eq!(
        ordinals,
        vec![RowOrdinal::new(0), RowOrdinal::new(1), RowOrdinal::new(2)]
    );

    let source = ordinals[0];
    let copied = source; // `Copy` が要る（move されない）
    assert_eq!(source, copied); // `PartialEq` が要る

    let unique: HashSet<RowOrdinal> = ordinals.iter().copied().collect(); // `Hash` が要る
    assert_eq!(unique.len(), 3);
    assert!(unique.contains(&RowOrdinal::new(1)));
    assert!(!unique.contains(&RowOrdinal::new(9)));
}

#[test]
fn row_span_covers_a_half_open_interval_of_ordinals() {
    let span = RowSpan::new(RowOrdinal::new(5), 3);
    assert_eq!(span.start(), RowOrdinal::new(5));
    assert_eq!(span.count(), 3);
    assert!(!span.is_empty());

    // 終端は**排他**である（5 + 3 = 8 であり、8 は区間に含まれない）。
    assert_eq!(span.end(), 8);
    assert!(!span.contains(RowOrdinal::new(4))); // 開始の手前は含まない
    assert!(span.contains(RowOrdinal::new(5))); // 開始を含む
    assert!(span.contains(RowOrdinal::new(7))); // 最後の 1 つを含む
    assert!(!span.contains(RowOrdinal::new(8))); // 終端を含まない

    // 空の区間（`[5, 5)`）はどの序数も含まない。
    let empty = RowSpan::new(RowOrdinal::new(5), 0);
    assert!(empty.is_empty());
    assert_eq!(empty.count(), 0);
    assert_eq!(empty.end(), 5);
    assert!(!empty.contains(RowOrdinal::new(4)));
    assert!(!empty.contains(RowOrdinal::new(5)));
    assert!(!empty.contains(RowOrdinal::new(6)));

    // 両端の場合: 可視 100 行のうち先頭の 1 行と、最終行の 1 行。
    let visible = 100_usize;
    let first = RowSpan::new(RowOrdinal::new(0), 1);
    assert!(first.contains(RowOrdinal::new(0)));
    assert_eq!(first.end(), 1);
    assert!(!first.contains(RowOrdinal::new(1)));

    let last = RowSpan::new(RowOrdinal::new(visible - 1), 1);
    assert!(last.contains(RowOrdinal::new(visible - 1)));
    assert_eq!(last.end(), visible);
    assert!(!last.contains(RowOrdinal::new(visible))); // 可視行数の外は含まない

    // 窓の符号化と無効化は同じ区間の型を受け取る（`Copy` であり、持ち回れる）。
    let window = span;
    assert_eq!(window.end(), 8);
    assert_eq!(span.contains(RowOrdinal::new(6)), window.contains(RowOrdinal::new(6)));
}

#[test]
fn cell_range_normalizes_corners_into_a_single_rectangle() {
    let top_left = at(2, 1);
    let bottom_right = at(5, 4);

    // 逆方向に引いた選択（右下から左上へ）も同じ矩形である。
    let dragged_backwards = CellRange::new(bottom_right, top_left);
    assert_eq!(dragged_backwards, CellRange::new(top_left, bottom_right));
    assert_eq!(dragged_backwards.start(), top_left);
    assert_eq!(dragged_backwards.end(), bottom_right);

    // 行だけ逆・列だけ逆の 2 通りも同じ矩形になる（角は軸ごとに独立して正規化する）。
    let crossed = CellRange::new(at(5, 1), at(2, 4));
    assert_eq!(crossed, dragged_backwards);
    assert_eq!(crossed.start(), top_left);
    assert_eq!(crossed.end(), bottom_right);
    assert_eq!(
        CellRange::new(at(2, 4), at(5, 1)),
        dragged_backwards,
        "角の与え方に依らず同じ矩形を表す"
    );
}

#[test]
fn cell_range_counts_cells_for_each_selection_shape() {
    // 1 セル（要件 2.5 の最小の場合）
    let single = CellRange::new(at(3, 2), at(3, 2));
    assert_eq!(
        (single.row_count(), single.column_count(), single.cell_count()),
        (1, 1, 1)
    );

    // 1 行（行の全体を選ぶ。要件 2.3）
    let single_row = CellRange::new(at(0, 0), at(0, 9));
    assert_eq!(
        (
            single_row.row_count(),
            single_row.column_count(),
            single_row.cell_count()
        ),
        (1, 10, 10)
    );

    // 1 列（列の全体を選ぶ。要件 2.3）
    let single_column = CellRange::new(at(0, 3), at(7, 3));
    assert_eq!(
        (
            single_column.row_count(),
            single_column.column_count(),
            single_column.cell_count()
        ),
        (8, 1, 8)
    );

    // 矩形
    let rectangle = CellRange::new(at(2, 1), at(4, 5));
    assert_eq!(
        (
            rectangle.row_count(),
            rectangle.column_count(),
            rectangle.cell_count()
        ),
        (3, 5, 15)
    );

    // 逆順に引いた矩形でも同じ数になる（正規化が効いていなければ、数は負の差になり
    // 桁溢れとして現れる）。
    let dragged_backwards = CellRange::new(at(4, 5), at(2, 1));
    assert_eq!(
        (
            dragged_backwards.row_count(),
            dragged_backwards.column_count(),
            dragged_backwards.cell_count()
        ),
        (3, 5, 15)
    );
    assert_eq!(dragged_backwards.cell_count(), rectangle.cell_count());

    // 要件 2.6: 範囲は複製・貼り付け・削除・取り消しの対象として持ち回られる（`Copy` である）。
    let carried_away = rectangle;
    assert_eq!(carried_away, rectangle);
    assert_eq!(rectangle.cell_count(), 15);
}

#[test]
fn cell_range_counts_depend_on_ordinals_and_cell_address_identity_on_row_ids() {
    let mut ids = IdFactory::new();
    let before: Vec<RowId> = (0..8).map(|_| ids.new_row_id()).collect();
    let after: Vec<RowId> = (0..8).map(|_| ids.new_row_id()).collect();

    // 同じ表示矩形（可視 1..=3 行 × 列 0..=1）。
    let range = CellRange::new(at(1, 0), at(3, 1));
    let cells_over = |rows: &[RowId]| -> Vec<CellAddress> {
        let mut cells = Vec::new();
        for ordinal in range.start().row().get()..=range.end().row().get() {
            for column in range.start().column().index()..=range.end().column().index() {
                cells.push(CellAddress::new(rows[ordinal], ColumnIndex::new(column)));
            }
        }
        cells
    };

    // その序数を占める物理行がそっくり入れ替わっても、範囲の数は変わらない
    // （数は序数だけで決まる = 表示空間の数である）。
    let before_cells = cells_over(&before);
    let after_cells = cells_over(&after);
    assert_eq!(before_cells.len(), range.cell_count());
    assert_eq!(after_cells.len(), range.cell_count());
    assert_eq!(before_cells.len(), after_cells.len());

    // 一方、セルの位置（物理の同一性）は行識別子で決まる。同じ序数のセルでも別のセルである。
    let column = range.start().column();
    assert_ne!(before_cells[0], after_cells[0]);
    assert_eq!(before_cells[0], CellAddress::new(before[1], column));
    assert_eq!(after_cells[0], CellAddress::new(after[1], column));
    assert_eq!(before_cells[1].column(), ColumnIndex::new(1));
    assert_eq!(before_cells[1].row(), before[1]);
    assert_eq!(before_cells[2].row(), before[2], "3 番目は次の可視行である");
}

#[test]
fn nested_path_maps_the_upstream_value_path() {
    // 上流の公開 API（`root` / `push_field` / `push_index`）で経路を組み立てる。
    let mut upstream = ValuePath::root();
    upstream.push_field("明細");
    upstream.push_index(1);
    upstream.push_field("色");

    let path = NestedPath::from(&upstream);
    assert_eq!(path.len(), 3);
    assert!(!path.is_empty());
    assert!(!path.is_root());
    assert_eq!(path.segments()[0], NestedPathSegment::Field("明細".into()));
    assert_eq!(
        path.segments(),
        [
            NestedPathSegment::Field("明細".into()),
            NestedPathSegment::Index(1),
            NestedPathSegment::Field("色".into()),
        ]
        .as_slice()
    );

    // 上流の空の経路（セル直下）は根へ写る。
    let root = NestedPath::from(&ValuePath::root());
    assert!(root.is_root());
    assert!(root.is_empty());
    assert_eq!(root.len(), 0);
    assert!(root.segments().is_empty());
    assert_eq!(root, NestedPath::root());
}

#[test]
fn search_direction_has_two_distinct_variants() {
    // **この型の振る舞いはここでは固定できない。**向きの意味は `find_violation` が実装されて
    // 初めて観測できる（`design.md`「Components and Interfaces / GridSession」の
    // `find_violation(&self, from: RowOrdinal, direction: SearchDirection) -> Option<CellAddress>`。
    // 実装は群 2 のタスク 2.4、要件 4.4）。
    //
    // 以前ここに置いていた「違反を探す」局所の閉包とその検査は、**本クレートのコードを
    // 一切通っていなかった**ため削除した。向きの意味を `src/types/mod.rs` の doc で入れ替えても
    // 検査は緑のままであり、固定しているという主張だけが偽になっていた（レビューの指摘）。
    // 同じ形の偽の固定をここへ置き直さない。
    //
    // ここで固定できるのは**2 つの変種が区別できること**だけである（`Backward` を消して
    // 1 つの変種に畳めば、この行はコンパイルに失敗する）。
    assert_ne!(SearchDirection::Forward, SearchDirection::Backward);
}

#[test]
fn grid_error_variants_are_discriminable_and_carry_only_their_context() {
    let mut ids = IdFactory::new();
    let sheet_id = ids.new_sheet_id();
    let row_id = ids.new_row_id();
    let cell = CellAddress::new(row_id, ColumnIndex::new(1));

    // **この `match` は 5 変種のすべてを列挙する。** `GridError` に 6 つ目の変種
    // （たとえば値の不適合）が足されれば、網羅性の検査により本テストはまずコンパイルに
    // 失敗する。これが design.md「値の不適合は `GridError` に含まれない」の固定である。
    let kind = |error: &GridError| -> &'static str {
        match error {
            GridError::SchemaUnusable { sheet } => {
                assert_eq!(*sheet, sheet_id);
                "schema-unusable"
            }
            GridError::UnknownRow { row } => {
                assert_eq!(*row, row_id);
                "unknown-row"
            }
            GridError::ColumnOutOfRange { column, count } => {
                assert_eq!(*column, ColumnIndex::new(4));
                assert_eq!(*count, 3);
                "column-out-of-range"
            }
            GridError::SpanOutOfRange { span, visible } => {
                assert_eq!(*span, RowSpan::new(RowOrdinal::new(90_000), 2_000));
                assert_eq!(*visible, 90_000);
                "span-out-of-range"
            }
            GridError::NestedDecode { cell } => {
                assert_eq!(*cell, CellAddress::new(row_id, ColumnIndex::new(1)));
                "nested-decode"
            }
        }
    };

    let errors = [
        GridError::SchemaUnusable { sheet: sheet_id },
        GridError::UnknownRow { row: row_id },
        GridError::ColumnOutOfRange {
            column: ColumnIndex::new(4),
            count: 3,
        },
        GridError::SpanOutOfRange {
            span: RowSpan::new(RowOrdinal::new(90_000), 2_000),
            visible: 90_000,
        },
        GridError::NestedDecode { cell },
    ];

    let mut kinds: Vec<&str> = errors.iter().map(kind).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(kinds.len(), 5, "5 変種が互いに判別できる");

    // 同じ入力からは同じ誤りになり、別の変種とは等しくない（`PartialEq` が要る）。
    assert_eq!(errors[1], GridError::UnknownRow { row: row_id });
    assert_ne!(errors[0], errors[1]);
    assert_ne!(errors[4], GridError::NestedDecode { cell: CellAddress::new(row_id, ColumnIndex::new(2)) });

    // 文言は呼び出し元が組み立てる（structure.md）。`Display` は持ち物をそのまま写すだけである。
    assert!(errors[0].to_string().contains(&sheet_id.to_string()));
    assert!(errors[1].to_string().contains(&row_id.to_string()));
    assert_ne!(
        errors[2].to_string(),
        GridError::ColumnOutOfRange {
            column: ColumnIndex::new(5),
            count: 3,
        }
        .to_string(),
        "持ち物が違えば提示も違う"
    );
    assert_ne!(
        errors[3].to_string(),
        GridError::SpanOutOfRange {
            span: RowSpan::new(RowOrdinal::new(0), 1),
            visible: 90_000,
        }
        .to_string()
    );
    assert_ne!(
        errors[4].to_string(),
        GridError::NestedDecode {
            cell: CellAddress::new(row_id, ColumnIndex::new(2)),
        }
        .to_string()
    );

    // 封筒の失敗腕へ載せられる（`std::error::Error` を実装している）。
    let as_error: &dyn std::error::Error = &errors[0];
    assert!(as_error.to_string().contains(&sheet_id.to_string()));
}

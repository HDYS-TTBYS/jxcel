//! クレート外から見た一括のセル書き換え（タスク 1.2。要件 3.5, 3.7）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。`document_format::`
//! 直下の再エクスポート（`set_cells` は [`Document`] のメソッド、誤り型は
//! [`CellWriteError`]）がクレート外から見えることをコンパイル時に示す。
//!
//! # 何を固定するか
//!
//! * 1 回の呼び出しで複数行・複数列が書き換わること
//! * 同じ入力の再適用が同じ結果になること（置換であって加算ではない）
//! * **変更しない行と列の値に触れない**こと
//! * 未知のシート・未知の行・範囲外の列のそれぞれで、**どのセルも変更されない**こと
//!   （部分適用なし。事前検査は 1 パスで、1 つでも不正なら全部を書かない）
//! * 行の現在の値数より後ろへの書き込みが、間を [`CellValue::Null`] で埋めること
//!   （埋めた後も値数が列数を超えないこと）
//! * 行の集合・並び・識別子が変わらないこと
//! * 10 万行規模のシートでも 1 回の呼び出しで適用できること
//!
//! **時間の閾値はここに置かない**（行数の二乗で増えないことは設計の実装と 5.1 の予算の
//! ゲートが担保する。design「Performance」の裁定）。
//!
//! 列の添字は `Sheet::columns` の並びに対する位置であり、行の値数と列数の一致は保存時の
//! 門（`RowsCodec::encode`）が担う。本テストは書き込み後の値だけを見る。

use document_format::{CellValue, CellWriteError, Document, Row, RowId, SheetId};

/// シートの（行識別子, 列順の値）を行順に写す（比較と不変の観測に使う）。
fn values_of(document: &Document, sheet: SheetId) -> Vec<(RowId, Vec<CellValue>)> {
    document
        .sheet_by_id(sheet)
        .expect("対象シートは存在する")
        .rows()
        .iter()
        .map(|row| (row.id(), row.values().to_vec()))
        .collect()
}

/// 行識別子を行順に写す（行の集合・並び・識別子が変わらないことの観測）。
fn row_ids(document: &Document, sheet: SheetId) -> Vec<RowId> {
    document
        .sheet_by_id(sheet)
        .expect("対象シートは存在する")
        .rows()
        .iter()
        .map(Row::id)
        .collect()
}

/// 列名を宣言し、与えられた値列で行を積んだ 1 シートの文書と、そのシート識別子を返す。
fn document_with(columns: &[&str], rows: Vec<Vec<CellValue>>) -> (Document, SheetId) {
    let mut document = Document::new();
    let sheet = document.add_sheet("標本");
    document
        .set_sheet_columns(
            sheet,
            columns.iter().map(|name| (*name).to_owned()).collect(),
        )
        .expect("対象シートは存在する");
    for values in rows {
        let row = document.add_row(sheet).expect("シートは存在する");
        document
            .set_row_values(sheet, row, values)
            .expect("行は存在する");
    }
    (document, sheet)
}

/// 3 行 × 3 列の標本。
fn three_by_three() -> (Document, SheetId, Vec<RowId>) {
    let (document, sheet) = document_with(
        &["a", "b", "c"],
        vec![
            vec![CellValue::Int(1), CellValue::Int(2), CellValue::Int(3)],
            vec![CellValue::Int(4), CellValue::Int(5), CellValue::Int(6)],
            vec![CellValue::Int(7), CellValue::Int(8), CellValue::Int(9)],
        ],
    );
    let ids = row_ids(&document, sheet);
    (document, sheet, ids)
}

#[test]
fn set_cells_writes_multiple_rows_and_columns_in_one_call() {
    // 1 回の呼び出しで複数行・複数列が書き換わる（要件 3.7 の受け口）。
    let (mut document, sheet, ids) = three_by_three();
    document
        .set_cells(
            sheet,
            &[
                (ids[0], 2, CellValue::Text("x".to_owned())),
                (ids[1], 0, CellValue::Text("y".to_owned())),
                (ids[2], 1, CellValue::Null),
            ],
        )
        .expect("妥当な変更は適用できる");

    let observed = values_of(&document, sheet);
    assert_eq!(
        vec![
            CellValue::Int(1),
            CellValue::Int(2),
            CellValue::Text("x".to_owned())
        ],
        observed[0].1
    );
    assert_eq!(
        vec![
            CellValue::Text("y".to_owned()),
            CellValue::Int(5),
            CellValue::Int(6)
        ],
        observed[1].1
    );
    assert_eq!(
        vec![CellValue::Int(7), CellValue::Null, CellValue::Int(9)],
        observed[2].1
    );
}

#[test]
fn set_cells_reapplication_is_idempotent() {
    // 置換であり加算ではない: 同じ入力の 2 回目は 1 回目の結果を変えない。
    let (mut document, sheet, ids) = three_by_three();
    let cells = [
        (ids[0], 0, CellValue::Bool(true)),
        (ids[2], 2, CellValue::Float(1.5)),
    ];

    document.set_cells(sheet, &cells).expect("1 回目");
    let once = values_of(&document, sheet);
    document.set_cells(sheet, &cells).expect("2 回目");
    let twice = values_of(&document, sheet);

    assert_eq!(once, twice, "再適用で結果が変わってはならない");
}

#[test]
fn set_cells_leaves_untouched_rows_and_columns_unchanged() {
    // 変更しない行と列の値に触れない（行の索引を 1 度だけ作り、対象のセルだけを書く）。
    let (mut document, sheet, ids) = three_by_three();
    document
        .set_cells(sheet, &[(ids[1], 1, CellValue::Text("only".to_owned()))])
        .expect("妥当な変更は適用できる");

    let observed = values_of(&document, sheet);
    // 行 0 と行 2 は 1 つも変わらない。
    assert_eq!(
        vec![CellValue::Int(1), CellValue::Int(2), CellValue::Int(3)],
        observed[0].1
    );
    assert_eq!(
        vec![CellValue::Int(7), CellValue::Int(8), CellValue::Int(9)],
        observed[2].1
    );
    // 行 1 でも対象の列以外は変わらない。
    assert_eq!(CellValue::Int(4), observed[1].1[0]);
    assert_eq!(CellValue::Text("only".to_owned()), observed[1].1[1]);
    assert_eq!(CellValue::Int(6), observed[1].1[2]);
}

#[test]
fn unknown_sheet_changes_no_cell() {
    // 未知のシートは判別可能な変種として返り、どのセルも変更しない（部分適用なし）。
    let (mut document, sheet, ids) = three_by_three();
    let ghost = Document::new().add_sheet("別文書");
    let before = values_of(&document, sheet);

    // 先に妥当な変更を並べる: 事前検査を怠ればこれが適用されてしまう。
    let result = document.set_cells(
        ghost,
        &[
            (ids[0], 0, CellValue::Int(99)),
            (ids[1], 1, CellValue::Int(98)),
        ],
    );

    assert_eq!(Err(CellWriteError::UnknownSheet { sheet: ghost }), result);
    assert_eq!(before, values_of(&document, sheet), "どのセルも変わらない");
}

#[test]
fn unknown_row_changes_no_cell() {
    // 対象シートに属さない行（他シートの行）も同じ失敗として報告する。
    let (mut document, sheet, ids) = three_by_three();
    let other = document.add_sheet("別シート");
    let ghost = document.add_row(other).expect("別シートは存在する");
    let before = values_of(&document, sheet);

    let result = document.set_cells(
        sheet,
        &[
            (ids[0], 0, CellValue::Int(99)),
            (ghost, 0, CellValue::Int(1)),
        ],
    );

    assert_eq!(Err(CellWriteError::UnknownRow { row: ghost }), result);
    assert_eq!(before, values_of(&document, sheet), "どのセルも変わらない");
}

#[test]
fn unknown_column_changes_no_cell() {
    // 列の範囲は `Sheet::columns` の数で判定する（3 列なら添字 3 は範囲外）。
    let (mut document, sheet, ids) = three_by_three();
    let before = values_of(&document, sheet);

    let result = document.set_cells(
        sheet,
        &[
            (ids[0], 0, CellValue::Int(99)),
            (ids[1], 3, CellValue::Int(1)),
        ],
    );

    assert_eq!(
        Err(CellWriteError::UnknownColumn {
            column: 3,
            columns: 3,
        }),
        result
    );
    assert_eq!(before, values_of(&document, sheet), "どのセルも変わらない");
}

#[test]
fn write_past_the_current_value_count_fills_gaps_with_null() {
    // 行の値数より後ろへの書き込みは間を値なしで埋め、値数は列数を超えない。
    let (mut document, sheet) = document_with(&["a", "b", "c"], vec![Vec::new()]);
    let row = row_ids(&document, sheet)[0];

    document
        .set_cells(sheet, &[(row, 2, CellValue::Int(7))])
        .expect("妥当な変更は適用できる");

    let values = values_of(&document, sheet)[0].1.clone();
    assert_eq!(
        vec![CellValue::Null, CellValue::Null, CellValue::Int(7)],
        values,
        "間は値なしで埋まり、値数は列数と一致する"
    );
    let columns = document
        .sheet_by_id(sheet)
        .expect("対象シートは存在する")
        .columns()
        .len();
    assert_eq!(columns, values.len());
}

#[test]
fn existing_prefix_is_not_padded_when_the_write_is_inside_it() {
    // 既存の値数の内側への書き込みは埋めない（変更しない列に触れない）。
    let (mut document, sheet) = document_with(
        &["a", "b", "c"],
        vec![vec![CellValue::Int(1), CellValue::Int(2)]],
    );
    let row = row_ids(&document, sheet)[0];

    document
        .set_cells(sheet, &[(row, 1, CellValue::Int(20))])
        .expect("妥当な変更は適用できる");

    assert_eq!(
        vec![CellValue::Int(1), CellValue::Int(20)],
        values_of(&document, sheet)[0].1
    );
}

#[test]
fn set_cells_preserves_row_identity_and_order() {
    // 行の集合・並び・識別子を変えない（置換であって追加ではない）。
    let (mut document, sheet, before) = three_by_three();

    document
        .set_cells(
            sheet,
            &[
                (before[2], 0, CellValue::Int(-1)),
                (before[0], 2, CellValue::Int(-2)),
            ],
        )
        .expect("妥当な変更は適用できる");

    assert_eq!(
        before,
        row_ids(&document, sheet),
        "行の集合・並び・識別子は変わらない"
    );
}

#[test]
fn set_cells_applies_to_a_large_sheet_in_one_call() {
    // 10 万行規模でも 1 回の呼び出しで適用できる（時間の閾値は置かない）。
    const ROWS: usize = 100_000;
    let (mut document, sheet) = document_with(&["a", "b"], vec![Vec::new(); ROWS]);
    let ids = row_ids(&document, sheet);
    assert_eq!(ROWS, ids.len());

    let cells: Vec<(RowId, usize, CellValue)> = ids
        .iter()
        .enumerate()
        .map(|(position, id)| (*id, 0, CellValue::Int(position as i64)))
        .collect();
    document
        .set_cells(sheet, &cells)
        .expect("10 万行の一括書き換えは 1 回で適用できる");

    let observed = values_of(&document, sheet);
    assert_eq!(ids, row_ids(&document, sheet), "行の識別子と並びは不変");
    assert_eq!(
        vec![CellValue::Int(0)],
        observed[0].1,
        "先頭行にも末尾行にも 1 回の呼び出しで適用される"
    );
    assert_eq!(
        vec![CellValue::Int((ROWS - 1) as i64)],
        observed[ROWS - 1].1
    );
    assert_eq!(ROWS, observed.len());
}

/// クレート外から `document_format::model::CellWriteError` 経由でも見えること
/// （根の再輸出とモジュール経由が同一の型であることのコンパイル時の確認）。
#[test]
fn cell_write_error_is_reachable_through_the_model_module() {
    let ghost = Document::new().add_sheet("別文書");
    let error: document_format::model::CellWriteError =
        document_format::model::CellWriteError::UnknownSheet { sheet: ghost };
    assert_eq!(
        CellWriteError::UnknownSheet { sheet: ghost },
        error,
        "根の再輸出と model 経由は同一の型である"
    );
}

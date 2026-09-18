//! クレート外から見た一括の行の値の置換（[`Document::set_rows_values`]）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。`document_format::`
//! 直下の再エクスポート（`set_rows_values` は [`Document`] のメソッド、誤り型は
//! [`RowValuesError`]）がクレート外から見えることをコンパイル時に示す。
//!
//! # 何を固定するか
//!
//! * 1 回の呼び出しで複数行の値の並びが置き換わり、**材料に無い行には触れない**こと
//! * **材料の長さがそのまま行の幅になる**こと — 短い材料は列数まで埋めず、幅 0 の材料は
//!   幅 0 の行にする（セル単位の書き込み `Document::set_cells` が決して縮めないのに対し、
//!   本メソッドは縮められる。`crates/data-grid` の取り消しがこの 2 つに依存する）
//! * **同じ行が材料に重複して現れたら入力順の last-wins** であること（決定的であり、
//!   順序を入れ替えれば結果も変わる。`Document::set_cells` と同じ契約）
//! * 未知のシート・未知の行のそれぞれで、**どの行の値も変更されない**こと（部分適用なし。
//!   事前検査は 1 パスで、1 つでも不正なら全部を書かない）
//! * 行の集合・並び・識別子が変わらないこと
//!
//! **時間の閾値はここに置かない**（行の探索が 1 度の走査で済むことは `crates/data-grid` の
//! `benches/large_grid.rs` の `undo_100k` が計測する）。

use document_format::{CellValue, Document, Row, RowId, RowValuesError, SheetId};

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
fn set_rows_values_replaces_whole_value_lists_in_one_call() {
    // 1 回の呼び出しで複数行が置き換わり、材料に無い行には触れない。
    let (mut document, sheet, ids) = three_by_three();
    let before = row_ids(&document, sheet);

    document
        .set_rows_values(
            sheet,
            &[
                (ids[0], vec![CellValue::Text("x".to_owned())]),
                (ids[2], vec![CellValue::Bool(true), CellValue::Null]),
            ],
        )
        .expect("妥当な材料は適用できる");

    let observed = values_of(&document, sheet);
    assert_eq!(
        vec![CellValue::Text("x".to_owned())],
        observed[0].1,
        "材料の長さがそのまま行の幅になる（列数まで埋めない）"
    );
    assert_eq!(
        vec![CellValue::Int(4), CellValue::Int(5), CellValue::Int(6)],
        observed[1].1,
        "材料に無い行の値が変わった"
    );
    assert_eq!(vec![CellValue::Bool(true), CellValue::Null], observed[2].1);
    assert_eq!(
        before,
        row_ids(&document, sheet),
        "行の集合・並び・識別子が変わった"
    );
}

#[test]
fn a_material_of_width_zero_empties_the_row() {
    // 幅 0 の材料は「値なし 1 件」ではなく「値なしを 1 件も持たない」である（縮められる）。
    let (mut document, sheet, ids) = three_by_three();

    document
        .set_rows_values(sheet, &[(ids[1], Vec::new())])
        .expect("妥当な材料は適用できる");

    let observed = values_of(&document, sheet);
    assert!(
        observed[1].1.is_empty(),
        "幅 0 の材料が値なし 1 件になった（{} 件）",
        observed[1].1.len()
    );
    assert_eq!(3, observed[0].1.len(), "別の行の幅が変わった");
}

#[test]
fn a_repeated_row_takes_the_last_material_in_input_order() {
    // 同じ行が 2 度現れたら後ろの材料が残る（last-wins。先勝ちでも最長勝ちでもない）。
    let (mut document, sheet, ids) = three_by_three();
    document
        .set_rows_values(
            sheet,
            &[
                (ids[0], vec![CellValue::Int(1)]),
                (ids[0], vec![CellValue::Int(2), CellValue::Int(3)]),
            ],
        )
        .expect("妥当な材料は適用できる");
    assert_eq!(
        vec![CellValue::Int(2), CellValue::Int(3)],
        values_of(&document, sheet)[0].1
    );

    // 順序を入れ替えれば結果も入れ替わる（入力順に依る契約であること）。
    let (mut reversed, sheet, ids) = three_by_three();
    reversed
        .set_rows_values(
            sheet,
            &[
                (ids[0], vec![CellValue::Int(2), CellValue::Int(3)]),
                (ids[0], vec![CellValue::Int(1)]),
            ],
        )
        .expect("妥当な材料は適用できる");
    assert_eq!(vec![CellValue::Int(1)], values_of(&reversed, sheet)[0].1);
}

#[test]
fn unknown_sheet_changes_no_row() {
    // 未知のシートは判別可能な変種として返り、どの行の値も変更しない（部分適用なし）。
    let (mut document, sheet, ids) = three_by_three();
    let ghost = Document::new().add_sheet("別文書");
    let before = values_of(&document, sheet);

    // 先に妥当な材料を並べる: 事前検査を怠ればこれが適用されてしまう。
    let result = document.set_rows_values(
        ghost,
        &[
            (ids[0], vec![CellValue::Int(99)]),
            (ids[1], vec![CellValue::Int(98)]),
        ],
    );

    assert_eq!(Err(RowValuesError::UnknownSheet { sheet: ghost }), result);
    assert_eq!(
        before,
        values_of(&document, sheet),
        "どの行の値も変わらない"
    );
}

#[test]
fn unknown_row_changes_no_row() {
    // 対象シートに属さない行（他シートの行）も同じ失敗として報告し、1 行も書かない。
    let (mut document, sheet, ids) = three_by_three();
    let other = document.add_sheet("別シート");
    let ghost = document.add_row(other).expect("別シートは存在する");
    let before = values_of(&document, sheet);

    let result = document.set_rows_values(
        sheet,
        &[
            (ids[0], vec![CellValue::Int(99)]),
            (ghost, vec![CellValue::Int(1)]),
        ],
    );

    assert_eq!(Err(RowValuesError::UnknownRow { row: ghost }), result);
    assert_eq!(
        before,
        values_of(&document, sheet),
        "どの行の値も変わらない"
    );
}

/// クレート外から `document_format::model::RowValuesError` 経由でも見えること
/// （根の再輸出とモジュール経由が同一の型であることのコンパイル時の確認）。
#[test]
fn row_values_error_is_reachable_through_the_model_module() {
    let ghost = Document::new().add_sheet("別文書");
    let (mut document, sheet, ids) = three_by_three();
    let result = document.set_rows_values(ghost, &[(ids[0], Vec::new())]);
    assert_eq!(
        Err(document_format::model::RowValuesError::UnknownSheet { sheet: ghost }),
        result
    );
    assert!(
        document.sheet_by_id(sheet).is_some(),
        "対象シートは存在する"
    );
}

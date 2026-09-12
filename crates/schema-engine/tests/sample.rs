//! 標本の生成器そのものの検証（タスク 1.4。要件 10.6）。
//!
//! `tests/common/mod.rs` は複数のテストとベンチが取り込む共有モジュールであり、
//! それ自身の正しさ（10 万行が生成できること・同じ引数なら常に同じ内容になること・
//! 指定した割合どおりに違反を混ぜられること）はこのファイルが検証する。
//! 設計が挙げる機能別のテスト（`validation.rs` 等）は、この共有モジュールを
//! 標本の組み立てに使う側である。

mod common;

use common::{sample, sample_with_violations, SampleSpec};
use document_format::CellValue;

/// 標本の列数（4 列。`SampleSpec::new` の既定の列名は `c00` … `c03`）。
const COLUMNS: usize = 4;

/// 10 万行の標本が生成でき、行数・列数・行識別子・各列の値の個数が揃う
/// （要件 10.6: 1 シートにつき合計 10 万行）。
#[test]
fn generates_a_hundred_thousand_rows() {
    let spec = SampleSpec::new(100_000, COLUMNS);
    let generated = sample(&spec, |row, column| {
        CellValue::Int((row * COLUMNS + column) as i64)
    });

    assert_eq!(generated.row_count(), 100_000, "行数が指定どおりでない");
    assert_eq!(generated.column_count(), COLUMNS);
    assert_eq!(
        generated.row_ids().len(),
        100_000,
        "行識別子が行数と一致しない"
    );
    assert_eq!(generated.document().sheets().len(), 1);

    let sheet = &generated.document().sheets()[0];
    assert_eq!(sheet.columns().len(), COLUMNS);
    assert_eq!(sheet.rows().len(), 100_000);
    assert!(
        sheet.rows().iter().all(|row| row.values().len() == COLUMNS),
        "値の個数が列数と一致しない行がある"
    );

    // 先頭行・中間行・最終行の値が規則どおりであること（順序が崩れていないこと）。
    for (row_index, expected_base) in [(0_usize, 0_i64), (50_000, 200_000), (99_999, 399_996)] {
        let values = sheet.rows()[row_index].values();
        for (column, value) in values.iter().enumerate() {
            assert_eq!(
                *value,
                CellValue::Int(expected_base + column as i64),
                "行 {row_index} 列 {column} の値が規則どおりでない"
            );
        }
    }
}

/// 同じ引数の標本は常に同じ内容になる（決定性。要件 10.6 の標本の前提）。
#[test]
fn same_arguments_produce_identical_content() {
    let spec = SampleSpec::new(100_000, COLUMNS);
    let rule = |row: usize, column: usize| CellValue::Text(format!("v{}-{}", row % 97, column));

    let first = sample(&spec, rule);
    let second = sample(&spec, rule);

    assert_eq!(first.row_count(), second.row_count());
    assert_eq!(first.row_ids().len(), second.row_ids().len());
    assert_eq!(
        first.row_values(),
        second.row_values(),
        "同じ引数から異なる内容の標本が生成された"
    );
}

/// 違反になる値を指定した割合どおりに混ぜ、その位置も決定的に選ぶ
/// （要件 10.6。5.6 の違反の上限・総件数の検証と 9.3 の決定性が使う）。
#[test]
fn mixes_violations_at_the_requested_ratio() {
    let spec = SampleSpec::new(1_000, COLUMNS);
    let total = 1_000 * COLUMNS;
    let normal = |row: usize, column: usize| CellValue::Int((row * COLUMNS + column) as i64);
    let violating = |row: usize, column: usize| CellValue::Text(format!("違反{row}-{column}"));

    // 端数は切り捨てる: 4000 セルの 25% ちょうど、および 33.3% → 1332 件。
    let quarter = sample_with_violations(&spec, normal, 0.25, violating);
    assert_eq!(quarter.violating_cells(), 1_000);
    assert_eq!(
        quarter
            .row_values()
            .iter()
            .enumerate()
            .flat_map(|(row, values)| {
                values
                    .iter()
                    .enumerate()
                    .filter(|(_, value)| matches!(value, CellValue::Text(_)))
                    .map(move |(column, _)| (row, column))
            })
            .count(),
        1_000,
        "違反の値の個数が指定した割合と一致しない"
    );
    // 等間隔の選定なので最初の違反は平坦添字 3（行 0・列 3）である。
    // 位置が引数だけで決まることを、値の側からも 1 点だけ確かめる。
    assert!(!quarter.violation_at(0, 0));
    assert!(quarter.violation_at(0, 3));
    assert_eq!(
        quarter.row_values()[0][3],
        CellValue::Text("違反0-3".to_owned())
    );

    let third = sample_with_violations(&spec, normal, 0.333, violating);
    assert_eq!(third.violating_cells(), 1_332);

    // 両端: 0 は混ぜず、1.0 は全セルを違反にする。
    let none = sample_with_violations(&spec, normal, 0.0, violating);
    assert_eq!(none.violating_cells(), 0);
    assert!(
        none.document().sheets()[0].rows().iter().all(|row| row
            .values()
            .iter()
            .all(|value| matches!(value, CellValue::Int(_)))),
        "割合 0 で違反値が混ざっている"
    );

    let all = sample_with_violations(&spec, normal, 1.0, violating);
    assert_eq!(all.violating_cells(), total);
    assert!(
        all.document().sheets()[0].rows().iter().all(|row| row
            .values()
            .iter()
            .all(|value| matches!(value, CellValue::Text(_)))),
        "割合 1.0 で通常値が残っている"
    );
}

/// 違反の位置（どこを違反として仕込むか）も同じ引数なら常に同じである
/// （要件 10.6。9.3 が跨る経路の決定性を検査する前提）。
#[test]
fn violation_positions_are_deterministic() {
    let spec = SampleSpec::new(500, COLUMNS);
    let normal = |_: usize, _: usize| CellValue::Bool(true);
    let violating = |_: usize, _: usize| CellValue::Null;

    let first = sample_with_violations(&spec, normal, 0.1, violating);
    let second = sample_with_violations(&spec, normal, 0.1, violating);

    assert_eq!(first.violating_cells(), second.violating_cells());
    let first_positions: Vec<(usize, usize)> = (0..500)
        .flat_map(|row| (0..COLUMNS).map(move |column| (row, column)))
        .filter(|&(row, column)| first.violation_at(row, column))
        .collect();
    let second_positions: Vec<(usize, usize)> = (0..500)
        .flat_map(|row| (0..COLUMNS).map(move |column| (row, column)))
        .filter(|&(row, column)| second.violation_at(row, column))
        .collect();
    assert_eq!(
        first_positions, second_positions,
        "違反の位置が実行ごとに変わる"
    );
    assert_eq!(first_positions.len(), first.violating_cells());
}

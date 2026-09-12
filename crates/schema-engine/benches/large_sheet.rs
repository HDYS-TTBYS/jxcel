//! 性能ベンチマーク: 10 万行 × 30 列の全件検証（要件 10.1, 10.3。tasks.md 9.1 / 9.2）。
//!
//! ここはハーネスの結線だけを置いた足場である（タスク 1.1）。実測（10 万行 × 30 列の
//! 全件検証と、一意制約を持つ列 1 本だけの再検証）は、タスク 9.1 の標本と 9.2 の計測が
//! このファイルへ入れる。計測値は `target/criterion/large_sheet/*/new/estimates.json` に
//! 残り、`scripts/check-bench-budget.sh` が予算（1 秒）と比較する（結線は 9.2。
//! design.md「Modified Files」）。
//!
//! 計測環境の規定（要件 10.3）は「SSD を搭載した 4 コア以上の一般的なデスクトップ
//! 環境」である。判定は CI ランナーを保守的な代理として要件値そのものに対して行う
//! （`document-format` の `benches/large_document.rs` と同じ扱い）。
use criterion::{criterion_group, criterion_main, Criterion};
use document_format::CellValue;

// `benches/` から `tests/` のモジュールをそのままでは取り込めないため、相対パスで
// 取り込む（tasks.md 1.4）。10 万行 × 30 列の標本の組み立てはタスク 9.1 が、
// 計測は 9.2 がこの共有モジュールの上に載せる。
#[path = "../tests/common/mod.rs"]
mod common;

/// ハーネスの結線確認。実測はタスク 9.1 / 9.2 が置き換える。
///
/// 共有の生成器がベンチのターゲットから実際に取り込めることも、ここで 1 回だけ
/// 組み立てて確かめる（4 列 × 8 行の小さな標本。測定の対象ではない）。
fn large_sheet_scaffold(c: &mut Criterion) {
    let spec = common::SampleSpec::new(8, 4);
    let probe = common::sample(&spec, |row, column| {
        CellValue::Int((row * 4 + column) as i64)
    });
    assert_eq!(probe.row_count(), 8);
    assert_eq!(probe.column_count(), 4);

    c.bench_function("large_sheet_scaffold", |b| {
        b.iter(|| std::hint::black_box(1_u64).wrapping_add(1))
    });
}

criterion_group!(benches, large_sheet_scaffold);
criterion_main!(benches);

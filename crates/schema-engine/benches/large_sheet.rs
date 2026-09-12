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

/// ハーネスの結線確認。実測はタスク 9.1 / 9.2 が置き換える。
fn large_sheet_scaffold(c: &mut Criterion) {
    c.bench_function("large_sheet_scaffold", |b| {
        b.iter(|| std::hint::black_box(1_u64).wrapping_add(1))
    });
}

criterion_group!(benches, large_sheet_scaffold);
criterion_main!(benches);

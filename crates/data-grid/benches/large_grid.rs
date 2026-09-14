//! ベンチハーネスのスキャフォールド（criterion 結線確認用）。
//!
//! 実ベンチマーク（窓の符号化・並べ替え・絞り込み・貼り付け、requirements 11.5/11.6/11.7）は
//! 後続のタスクが同じ `large_grid` の群の下に bench id として追加する
//! （`crates/document-format/benches/large_document.rs` と同じ形）。
//! 計測値は `target/criterion/large_grid/*/new/estimates.json` に残り、
//! `scripts/check-bench-budget.sh` が `large_grid/` の下の bench id を読んで予算を機械判定する
//! （10 万行 × 30 列の窓の符号化、順序の再計算、1 万行の貼り付け）。
//!
//! # 標本はテストと共有する（tasks.md 1.4）
//!
//! 10 万行 × 30 列の標本の組み立てはタスク 1.4 が `tests/common/sample.rs` に置いており、
//! 本ベンチはその入口（[`common::sample::sample`]）だけを使う（写しを作らない）。`benches/` から
//! `tests/` のモジュールをそのままでは取り込めないため、相対パスで取り込む
//! （tasks.md 1.4「ベンチからは相対パス指定でこの共有モジュールを取り込む」。
//! `crates/document-format/benches/large_document.rs` と
//! `crates/schema-engine/benches/large_sheet.rs` が同じ形である）。
#[path = "../tests/common/mod.rs"]
mod common;

use common::sample::{sample, SampleOptions, SAMPLE_COLUMNS};
use criterion::{criterion_group, criterion_main, Criterion};

fn large_grid_scaffold_black_box(c: &mut Criterion) {
    // 標本の入口がベンチから届くことを確かめる。**ここでは小さな標本だけを組む** — 実ベンチは
    // タスク 9.1 が 10 万行 × 30 列の標本を計測の外で 1 回だけ組み立てて使う（本スキャフォールドが
    // 10 万行を組むと、9.1 の実装を待たずに毎回の計測へ数秒の準備を足すことになる）。
    let sample = sample(&SampleOptions::new(8, SAMPLE_COLUMNS));
    assert_eq!(
        SAMPLE_COLUMNS,
        sample.column_count(),
        "標本の列数が変わった"
    );

    let mut group = c.benchmark_group("large_grid");
    group.bench_function("scaffold_black_box", |b| {
        b.iter(|| std::hint::black_box(1_u64).wrapping_add(1))
    });
    group.finish();
}

criterion_group!(benches, large_grid_scaffold_black_box);
criterion_main!(benches);

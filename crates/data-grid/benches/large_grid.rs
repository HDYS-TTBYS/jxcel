//! ベンチハーネスのスキャフォールド（criterion 結線確認用）。
//!
//! 実ベンチマーク（窓の符号化・並べ替え・絞り込み・貼り付け、requirements 11.5/11.6/11.7）は
//! 後続のタスクが同じ `large_grid` の群の下に bench id として追加する
//! （`crates/document-format/benches/large_document.rs` と同じ形）。
//! 計測値は `target/criterion/large_grid/*/new/estimates.json` に残り、
//! `scripts/check-bench-budget.sh` が `large_grid/` の下の bench id を読んで予算を機械判定する
//! （10 万行 × 30 列の窓の符号化、順序の再計算、1 万行の貼り付け）。
use criterion::{criterion_group, criterion_main, Criterion};

fn large_grid_scaffold_black_box(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_grid");
    group.bench_function("scaffold_black_box", |b| {
        b.iter(|| std::hint::black_box(1_u64).wrapping_add(1))
    });
    group.finish();
}

criterion_group!(benches, large_grid_scaffold_black_box);
criterion_main!(benches);

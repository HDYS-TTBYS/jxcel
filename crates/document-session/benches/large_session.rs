//! ベンチハーネスのスキャフォールド（criterion 結線確認用）。
//! 実ベンチマーク（10 万行 × 30 列の読み込み・一括の適用・保存、要件 8.1）は
//! タスク 5.1 でこのファイルに追加する。
use criterion::{criterion_group, criterion_main, Criterion};

fn scaffold_black_box(c: &mut Criterion) {
    c.bench_function("scaffold_black_box", |b| {
        b.iter(|| std::hint::black_box(1_u64).wrapping_add(1))
    });
}

criterion_group!(benches, scaffold_black_box);
criterion_main!(benches);

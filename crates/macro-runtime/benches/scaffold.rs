//! ベンチハーネスのスキャフォールド（criterion 結線確認用）。
//!
//! 実ベンチ（10 万行 × 30 列の全行読み + 集計、1 万行の書き換え。要件 11.1, 11.2）は
//! タスク 5.3 が `benches/bulk.rs` に追加する。`Cargo.toml` の `[[bench]]` は
//! `harness = false` の宣言であり、**対応する実体のファイルが要る**（宣言だけでは
//! `cargo bench --no-run` が落ちる）。
use criterion::{criterion_group, criterion_main, Criterion};

fn scaffold_black_box(c: &mut Criterion) {
    c.bench_function("scaffold_black_box", |b| {
        b.iter(|| std::hint::black_box(1_u64).wrapping_add(1))
    });
}

criterion_group!(benches, scaffold_black_box);
criterion_main!(benches);

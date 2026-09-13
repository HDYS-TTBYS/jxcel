//! ベンチハーネスのスキャフォールド（criterion 結線確認用）。
//! 実ベンチマーク（10 万行 × 30 列の読み込み・一括の適用・保存、要件 8.1）は
//! タスク 5.1 でこのファイルに追加する。
//!
//! 標本の生成器はテストと共有する（`tests/common/mod.rs`。tasks.md 1.4）。`tests/` の
//! モジュールはそのままではベンチから参照できないため、`schema-engine` の
//! `benches/large_sheet.rs` と同じく**相対パス指定**で取り込む（design「File Structure
//! Plan」の「ベンチからは相対パス指定でこの共有モジュールを取り込む」）。
#[path = "../tests/common/mod.rs"]
mod common;

use criterion::{criterion_group, criterion_main, Criterion};

fn scaffold_black_box(c: &mut Criterion) {
    // 共有モジュールがベンチから取り込めていることの結線確認（計測対象の読み込み・
    // 一括の適用・保存はタスク 5.1 がここへ足す）。
    let columns = common::SampleSpec::new(3, 2).column_count();
    c.bench_function("scaffold_black_box", |b| {
        b.iter(|| std::hint::black_box(columns).wrapping_add(1))
    });
}

criterion_group!(benches, scaffold_black_box);
criterion_main!(benches);

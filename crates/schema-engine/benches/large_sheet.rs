//! 性能ベンチマーク: 10 万行 × 30 列の全件検証（要件 10.1, 10.3。tasks.md 9.1 / 9.2）。
//!
//! **このファイルはまだ計測を持たない。** いまここにあるのはハーネスの到達性の確認だけで
//! あり、予算（1 秒）の判定に使う「大きいシートの全件検証」ではない。実測はタスク 9.2 が
//! このファイルの関数の中身を置き換えて入れる（置き換える範囲は
//! [`large_sheet_scaffold`] の docs に書いてある）。計測値は
//! `target/criterion/large_sheet/*/new/estimates.json` に残り、
//! `scripts/check-bench-budget.sh` が予算（1 秒）と比較する（結線は 9.2。
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

/// ハーネスの結線確認。**この関数は計測の置き場所ではない。**
///
/// 共有の生成器がベンチのターゲットから実際に取り込めることも、ここで 2 回だけ組み立てて
/// 確かめる（4 列 × 8 行の小さな標本と、8 行の 30 列の標本。どちらも測定の対象ではない）。
/// 下の `bench_function` は criterion の結線が生きていることだけを見る**ダミー**であり、
/// 予算の判定に使う計測値を作らない — 名前に `scaffold` と付けてあるのはそのためである。
///
/// # 9.2 が置き換える範囲
///
/// この関数の本体（到達性の assert と下のダミーの `bench_function`）が置き換えの対象で
/// ある。9.2 はここへ、9.1 の標本を使う 2 つの実測を置く:
///
/// 1. 10 万行 × 30 列の全件検証（`SchemaEngineApi::validate_sheet`）— 予算 1 秒の対象。
/// 2. 一意制約を持つ列 1 本だけの再検証（`SchemaEngineApi::validate_columns`）—
///    要件 10.5 の経路が編集直後に成立することの裏付け。
///
/// `criterion_group!` の対象はこの関数のままなので、その行は書き換えずに済む。
fn large_sheet_scaffold(c: &mut Criterion) {
    let spec = common::SampleSpec::new(8, 4);
    let probe = common::sample(&spec, |row, column| {
        CellValue::Int((row * 4 + column) as i64)
    });
    assert_eq!(probe.row_count(), 8);
    assert_eq!(probe.column_count(), 4);

    // タスク 9.1 の 30 列の標本（`common` の子モジュール）もこの経路から届く。ここで見るのは
    // 到達性だけである（計測は 9.2 がこの標本の上に載せる）。
    let schema_sample = common::schema::schema_sample(&common::schema::SchemaSampleOptions::new(8));
    assert_eq!(8, schema_sample.rows());
    assert_eq!(
        common::schema::SAMPLE_COLUMNS,
        schema_sample.columns().len()
    );

    // ここから下はダミーである。足し算を 1 回するだけで、criterion の結線が生きていること
    // しか示さない。9.2 はこの closure を消し、上の「9.2 が置き換える範囲」の 2 つの実測に
    // 置き換える。
    c.bench_function("scaffold_reachability_only", |b| {
        b.iter(|| std::hint::black_box(1_u64).wrapping_add(1))
    });
}

criterion_group!(benches, large_sheet_scaffold);
criterion_main!(benches);

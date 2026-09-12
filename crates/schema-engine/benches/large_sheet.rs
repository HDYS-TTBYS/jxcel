//! 性能ベンチマーク: 10 万行 × 30 列のシートの全件検証（要件 10.1, 10.3。tasks.md 9.1 / 9.2）。
//!
//! criterion で計測する対象は 2 つである:
//!
//! 1. [`large_sheet`] の `validate_100k_rows_x_30_columns` — シート全体の一括検証
//!    （`SchemaEngineApi::validate_sheet`）。**予算 1 秒の判定対象**である（要件 10.1, 10.3。
//!    開く 3 秒の内側で走るため、その予算を圧迫しない上限）。
//! 2. [`large_sheet`] の `revalidate_unique_column` — 一意制約を持つ列 1 本だけの再検証
//!    （`SchemaEngineApi::validate_columns`）。**予算の判定対象ではない**（要件 10.5 は
//!    経路の存在を求めるのみで、時間の上限を定めていない）。編集直後の重複検出がこの経路で
//!    成立することの裏付けとして計測する（design.md「Performance」）。
//!
//! 計測値は `target/criterion/large_sheet/<bench id>/new/estimates.json` に残り、
//! `scripts/check-bench-budget.sh` が全件検証の平均を予算（1 秒）と比較する（結線は 9.2。
//! design.md「Modified Files」）。**ゲートが読む相対パスは bench id そのものである** —
//! `large_sheet/validate_100k_rows_x_30_columns` を変えるときは同じタスクで
//! `scripts/check-bench-budget.sh` の `check_budget` の行も合わせること。
//!
//! 標本は 9.1 の [`common::schema::schema_sample`] 1 つを共有する（10 進数・日時・書式つき
//! 文字列・シート間参照・入れ子・拡張型を含む 30 列。割合 0.1% の違反が混ざる）。
//! **標本の組み立てと計画のコンパイルは測定の外で 1 回だけ**行い、測定するのは検証だけである
//! — 予算が対象とするのは検証の所要時間であり、入力の準備ではない。
//!
//! 計測環境の規定（要件 10.3）は「SSD を搭載した 4 コア以上の一般的なデスクトップ環境」で
//! ある。判定は CI ランナーを保守的な代理として要件値そのものに対して行う
//! （`document-format` の `benches/large_document.rs` と同じ扱い）。
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use schema_engine::{SchemaEngine, SchemaEngineApi, ValidationOptions};

// `benches/` から `tests/` のモジュールをそのままでは取り込めないため、相対パスで
// 取り込む（tasks.md 1.4）。10 万行 × 30 列の標本の組み立てはタスク 9.1 が共有モジュールへ
// 置いており、本ベンチはその入口だけを使う（写しを作らない）。
#[path = "../tests/common/mod.rs"]
mod common;

/// 計測条件の行数（要件 10.1, 10.6）。**標本の定数ではなくリテラルで置く** — 標本の規模を
/// 黙って縮める変更をここで検出する（`document-format` の `benches/large_document.rs` と
/// 同じ方針）。
const ROWS: usize = 100_000;

/// 計測条件の列数（design.md「Performance」の 10 万行 × 30 列）。
const COLUMNS: usize = 30;

/// criterion の標本数。1 回の検証が予算に対して十分大きいため、`document-format` の
/// ベンチと同じ 10 に抑える（既定の 100 では計測が長引くだけで精度は上がらない）。
const SAMPLE_SIZE: usize = 10;

/// 10 万行 × 30 列の全件検証（予算 1 秒の判定対象）と、一意制約を持つ列 1 本の再検証。
///
/// 標本（[`common::schema::schema_sample`] の既定 = 10 万行 × 30 列・違反 0.1%・重複なし）と
/// 計画のコンパイルは測定の外で 1 回だけ行う。測定するのは `validate_sheet` と
/// `validate_columns` の呼び出しだけである。
fn large_sheet(c: &mut Criterion) {
    let sample = common::schema::schema_sample(&common::schema::SchemaSampleOptions::default());

    // 計測条件そのものを確かめる。要件 10.1 / 10.6 の「10 万行 × 30 列」と食い違えば、
    // 予算の判定が別の規模に対して行われることになるため、測定の前に落とす。
    assert_eq!(
        ROWS,
        sample.rows(),
        "標本の行数が要件 10.1 / 10.6 の計測条件（10 万行）と一致しない"
    );
    assert_eq!(
        COLUMNS,
        sample.columns().len(),
        "標本の列数が design.md「Performance」の計測条件（30 列）と一致しない"
    );

    let engine = SchemaEngine::new();
    let schema = sample.compiled();
    let document = sample.document();
    let sheet = sample.sheet();
    let options = ValidationOptions::default();

    let mut group = c.benchmark_group("large_sheet");
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(Duration::from_secs(3));
    group.measurement_time(Duration::from_secs(10));

    // 予算 1 秒の判定対象（要件 10.1, 10.3）。`scripts/check-bench-budget.sh` が読む相対パスは
    // この bench id である（モジュール docs）。
    group.bench_function("validate_100k_rows_x_30_columns", |b| {
        b.iter(|| {
            let report = engine.validate_sheet(
                std::hint::black_box(document),
                sheet,
                std::hint::black_box(&schema),
                &options,
            );
            // 検証を実際に走らせる（結果を最適化で消させない）。
            std::hint::black_box(report.total_violations())
        })
    });

    // 一意制約を持つ列 1 本の再検証（要件 10.5 の経路。予算の判定対象ではない）。
    // 対象の列は計画から引く — 標本の列添字を手で写さない（`CompiledSchema::unique_columns` が
    // 唯一の源である）。
    group.bench_function("revalidate_unique_column", |b| {
        let columns = schema.unique_columns();
        assert_eq!(
            1,
            columns.len(),
            "標本の一意制約を持つ列が 1 本でない（再検証の計測対象が定まらない）"
        );
        b.iter(|| {
            let report = engine.validate_columns(
                std::hint::black_box(document),
                sheet,
                std::hint::black_box(&schema),
                columns,
                &options,
            );
            std::hint::black_box(report.total_violations())
        })
    });

    group.finish();
}

criterion_group!(benches, large_sheet);
criterion_main!(benches);

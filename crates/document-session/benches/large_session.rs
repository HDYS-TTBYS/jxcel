//! 性能ベンチマーク: 10 万行 × 30 列のセッション経路の 3 つの予算
//! （tasks.md 5.1。要件 3.5, 5.6, 8.1）。
//!
//! # 計測する 3 つの予算
//!
//! 操作口 [`DocumentSessionsApi`] の経路で次の 3 つを計測する。**予算の判定対象はこの 3 つ
//! だけ**であり（design.md「Benchmarks（予算のゲート）」）、criterion の group は
//! `large_session`、bench id は下の 3 つである:
//!
//! 1. [`large_session`] の `open_and_hold_100k_rows_x_30_columns` — [`DocumentSessionsApi::resolve`]
//!    が 10 万行 × 30 列のファイルを読み込み、セッションが保持するまで（**予算 3 秒**。
//!    形式の側の `open` の予算を写す）。
//! 2. [`large_session`] の `save_100k_rows_x_30_columns` — [`DocumentSessionsApi::save`] が
//!    出所へ書き出すまで（**予算 2 秒**。形式の側の `save` の予算を写す。要件 5.6）。
//! 3. [`large_session`] の `apply_bulk_edit_100k_rows_x_30_columns` — 全セル（10 万行 ×
//!    30 列 = 300 万セル）を置き換える変更を、**1 回の** [`DocumentSessionsApi::edit`] として
//!    適用するまで（**予算 1 秒**。本スペックの新しい予算であり、要件 3.5 が「10 万行かつ
//!    30 列のシートのすべてのセルを置き換える変更が 1 度に要求されたとき」1 秒以内と定める）。
//!
//! 計測値は `target/criterion/large_session/<bench id>/new/estimates.json` に残り、
//! `scripts/check-bench-budget.sh` が平均（mean）の点推定値を予算と比較する。**ゲートの結線は
//! 計測を入れるのと同じタスク 5.1 で行う** — 計測より先に結線すると判定器が入力を読めずに
//! 赤のままになり、逆に結線を忘れると計測が 1 度も走らない（`bench.yml` の
//! `cargo bench -p …` に `document-session` が入っていない場合）。**ゲートが読む相対パスは
//! bench id そのものである** — ここを変えるときは同じタスクで
//! `scripts/check-bench-budget.sh` の `check_budget` の行も合わせること（欠けると判定器は
//! 2 を返して落ちる。fail-closed）。
//!
//! # 標本（測定条件）
//!
//! 標本は 1.4 の共有の生成器（`common::sample`）が作る 10 万行 × 30 列の 1 シート文書であり、
//! **測定の外で 1 回だけ**組み立ててファイルへ書き出す（予算が対象とするのは読み込み・適用・
//! 保存の所要時間であり、標本の組み立てではない）。規模は測定の前に [`ROWS`] / [`COLUMNS`] の
//! リテラルと照合する — 標本を黙って縮める変更はここで失敗する（`schema-engine` の
//! `benches/large_sheet.rs` と同じ規律）。
//!
//! # 一括の適用の測り方（要件 3.5 の「1 度に」）
//!
//! 置き換えるセル（行識別子・列の添字・値の並び）は**測定の外で 1 回だけ**組み立て、測定する
//! のは [`DocumentSessionsApi::edit`] の 1 回の呼び出し（閉包の内側の
//! [`Document::set_cells`] 1 回）だけにする。測定の前に 1 度だけ、その 1 回の適用が**版を
//! ちょうど 1 進める**こと（10 万行の一括が 1 回の適用として運ばれること）と、書き換えた
//! セルが実際に読めることを確かめる。
//!
//! # 実行時間（既定設定では長すぎる）
//!
//! criterion の既定（100 サンプル・3 秒目標）は 10 万行規模では長すぎるため、
//! `sample_size` / `warm_up_time` / `measurement_time` を明示的に絞る（`document-format` /
//! `schema-engine` のベンチと同じ設定）。
//!
//! # 計測環境の規定（要件 8.3）
//!
//! 計測条件は「SSD を搭載した 4 コア以上の一般的なデスクトップ環境」である。CI の
//! GitHub-hosted ランナー（`ubuntu-latest` / `macos-latest` / `windows-latest`）は、本
//! リポジトリが private であるため 2 vCPU（macOS は 3 コア M1）であり、この条件より**弱い**。
//! 予算判定はこの弱い環境で要件値そのものに対して行う（通れば要件の環境でも通るとみなす
//! 保守的な代理。`.github/workflows/bench.yml` 冒頭の「計測環境」）。

// 標本の生成器はテストと共有する（`tests/common/mod.rs`。tasks.md 1.4）。`tests/` の
// モジュールはそのままではベンチから参照できないため、`schema-engine` の
// `benches/large_sheet.rs` と同じく**相対パス指定**で取り込む。
#[path = "../tests/common/mod.rs"]
mod common;

use std::time::Duration;

use app_shell::ipc::WindowLabel;
use criterion::{criterion_group, criterion_main, Criterion};
use document_format::{CellValue, Document, DocumentFormatApi, RowId};
use document_session::{DocumentSessions, DocumentSessionsApi, SaveReport};

/// 計測条件の行数（要件 3.5, 5.6, 8.1）。**標本の定数ではなくリテラルで置く** — 標本の規模を
/// 黙って縮める変更をここで検出する（`document-format` / `schema-engine` のベンチと同じ方針）。
const ROWS: usize = 100_000;

/// 計測条件の列数（design.md「Performance & Scalability」の 10 万行 × 30 列）。
const COLUMNS: usize = 30;

/// criterion のサンプル数。1 回の操作が予算に対して十分大きいため、`document-format` の
/// ベンチと同じ 10 に抑える（既定の 100 では計測が長引くだけで精度は上がらない）。
const SAMPLE_SIZE: usize = 10;

/// ウィンドウの識別子（境界の型は上流 `app-shell` の単一の定義を借りる）。
fn window(label: &str) -> WindowLabel {
    WindowLabel::new(label)
}

/// 標本の全セルを置き換える変更（行識別子・列の添字・値の並び）を組み立てる。
///
/// 値は**現在の値と異なる**ものを選ぶ（1 つずらした行の [`common::value_at`]）。同じ値で
/// 上書きすると、書き込みが実際に起きたことを読み戻しで確かめられない。組み立ては
/// 10 万行 × 30 列 = 300 万件の `Vec` であり、**測定の外**で 1 回だけ行う。
fn bulk_cells(rows: &[RowId]) -> Vec<(RowId, usize, CellValue)> {
    let mut cells = Vec::with_capacity(rows.len() * COLUMNS);
    for (row, id) in rows.iter().enumerate() {
        for column in 0..COLUMNS {
            cells.push((*id, column, common::value_at(row + 1, column)));
        }
    }
    cells
}

/// 10 万行 × 30 列の読み込みと保持・保存・一括の適用を計測する
/// （予算: 読み込み 3 秒 / 保存 2 秒 / 一括の適用 1 秒）。
fn large_session(c: &mut Criterion) {
    let scratch = common::Scratch::new("bench_large_session");
    let sample = common::sample(&common::SampleSpec::new(ROWS, COLUMNS));

    // 計測条件そのものを確かめる。要件 3.5 / 5.6 / 8.1 の「10 万行 × 30 列」と食い違えば、
    // 予算の判定が別の規模に対して行われることになるため、測定の前に落とす。
    assert_eq!(
        ROWS,
        sample.row_count(),
        "標本の行数が要件 3.5 / 5.6 / 8.1 の計測条件（10 万行）と一致しない"
    );
    assert_eq!(
        COLUMNS,
        sample.column_count(),
        "標本の列数が design.md「Performance & Scalability」の計測条件（30 列）と一致しない"
    );

    let sheet = sample.sheet_id();
    let cells = bulk_cells(&sample.row_ids());
    // 一括の適用が**全セル**を対象にしていることを確かめる（一部だけの適用を「一括」として
    // 測ってしまわないため）。
    assert_eq!(
        cells.len(),
        ROWS * COLUMNS,
        "一括の適用の対象が全セル（10 万行 × 30 列）になっていない"
    );

    // 標本のファイルは読み込み用と保存用に 1 つずつ用意する（保存は出所のファイルを
    // 上書きするため、計測の対象を分けて互いに干渉させない）。書き出しは測定の外である。
    let open_path = scratch.file("open.jxcel");
    let save_path = scratch.file("save.jxcel");
    common::api()
        .save(sample.document(), &open_path)
        .expect("標本は保存できる");
    std::fs::copy(&open_path, &save_path).expect("標本のファイルを複製できる");
    drop(sample);

    let mut group = c.benchmark_group("large_session");
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(Duration::from_secs(3));
    group.measurement_time(Duration::from_secs(10));

    // 1. 読み込みと保持（予算 3 秒）。
    let open_window = window("bench-open");
    group.bench_function("open_and_hold_100k_rows_x_30_columns", |b| {
        b.iter(|| {
            // 反復ごとに新しい表とセッションを作る（形式の側の `open` の計測と同じく、
            // 「読み込んで保持する」1 回分を毎回測る。反復の終わりには保持した文書が表ごと
            // 落ちる）。
            let sessions = DocumentSessions::new();
            sessions
                .resolve(&open_window, Some(std::hint::black_box(&open_path)))
                .expect("標本は読み込める");
            // 保持している文書を実際に読む（`resolve` が復元した文書の行数。呼び出しが
            // 最適化で消えないようにする）。
            let held = sessions
                .read(&open_window, &mut |document| document.sheets()[0].rows().len())
                .expect("読み込んだ文書を読める");
            std::hint::black_box(held)
        })
    });

    // 2. 保存（予算 2 秒）。出所がある状態を作ってから、出所への書き出しだけを測る。
    let save_window = window("bench-save");
    let saving = DocumentSessions::new();
    saving
        .resolve(&save_window, Some(&save_path))
        .expect("標本は読み込める");
    group.bench_function("save_100k_rows_x_30_columns", |b| {
        b.iter(|| {
            let report = saving.save(&save_window).expect("保存の指示は受理される");
            // 出所があるため書き出しまで進む（`NeedsLocation` でも `Failed` でもない）。
            assert!(
                matches!(report, SaveReport::Saved { .. }),
                "保存が完了しなかった: {report:?}"
            );
            std::hint::black_box(report)
        })
    });

    // 3. 一括の適用（予算 1 秒。本スペックの新しい予算）。
    let apply_window = window("bench-apply");
    let editing = DocumentSessions::new();
    editing
        .resolve(&apply_window, Some(&open_path))
        .expect("標本は読み込める");

    // 測定の前に 1 度だけ: 全セルの置き換えが **1 回の適用として**運ばれること（版がちょうど
    // 1 進む。要件 3.5 の「1 度に要求されたとき」）と、書き換えたセルが実際に読めることを
    // 確かめる。
    let edited = editing
        .edit(&apply_window, &mut |document: &mut Document| {
            document.set_cells(sheet, &cells)
        })
        .expect("適用の要求は受理される");
    assert!(
        edited.value.is_ok(),
        "全セルの置き換えが失敗した: {:?}",
        edited.value
    );
    assert_eq!(
        edited.revision, 2,
        "読み込みの 1 に、10 万行の一括の適用が 1 回として積まれる（要件 3.5）"
    );
    assert!(
        edited.unsaved,
        "変更の適用で未保存の印が立つ（要件 3.4, 4.1）"
    );
    let rewritten = editing
        .read(&apply_window, &mut |document| {
            document.sheets()[0].rows()[0].values()[0].clone()
        })
        .expect("適用後の文書を読める");
    assert_eq!(
        rewritten,
        common::value_at(1, 0),
        "全セルの置き換えが先頭のセルに届いていない"
    );

    group.bench_function("apply_bulk_edit_100k_rows_x_30_columns", |b| {
        b.iter(|| {
            let edited = editing
                .edit(&apply_window, &mut |document| {
                    document.set_cells(sheet, std::hint::black_box(&cells))
                })
                .expect("適用の要求は受理される");
            // 置き換えを実際に走らせる（結果を最適化で消させない）。
            std::hint::black_box(edited.value.is_ok())
        })
    });

    group.finish();
}

// スキャフォールドの `scaffold_black_box`（1.1 が `cargo bench --no-run` を通すために置いた
// 結線確認）はここで**削除した**: 結線の確認という役目は上の 3 つの実測が引き継ぎ、
// 残せばゲートが読まないベンチを 1 つ増やすだけになる（何も計測しない反復は、予算の
// 判定にも回帰の検出にも寄与しない）。
criterion_group!(benches, large_session);
criterion_main!(benches);

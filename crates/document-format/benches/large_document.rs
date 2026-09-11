//! 性能ベンチマーク: 10 万行 × 30 列のドキュメントの `open` / `save`
//! （tasks.md 8.9。要件 8.1, 8.2, 8.3, 8.5）。
//!
//! # 計測する予算（要件 8.1, 8.2）
//!
//! 10 万行 × 30 列のドキュメントについて、**開く（[`DocumentFormatApi::open`]）が 3 秒以内**、
//! **保存（[`DocumentFormatApi::save`]）が 2 秒以内**であることを計測する。`open` / `save` は
//! 公開 API のフル経路である（`save` = 不変条件検証 → パート構築 → ダイジェスト算出 →
//! 決定的符号化 → 原子的書き込み。`open` = ファイル読み込み → コンテナ復号 → バージョン
//! ゲート → 移行 → ダイジェスト照合 → 構造検証 → モデル構築 → 規模の通知）。ファイル I/O を
//! 含めて測る（`Scratch` が用意する一時ディレクトリは**リポジトリ内**に作り、終了時に
//! `Drop` で削除する。`tests/common/mod.rs` の [`common::Scratch`] を再利用する）。
//!
//! 計測値は criterion の出力
//! （`target/criterion/large_document/*/new/estimates.json`）に残り、CI では
//! `scripts/check-bench-budget.sh` がその平均値を予算と比較する（`bench.yml` 参照）。
//! 予算超過はジョブの失敗として検出される。
//!
//! # 計測環境の規定（要件 8.3）
//!
//! 計測条件は「**SSD を搭載した 4 コア以上の一般的なデスクトップ環境**」である。CI の
//! GitHub-hosted ランナー（`ubuntu-latest` / `macos-latest` / `windows-latest`）は、
//! 本リポジトリが private であるため 2 vCPU（macOS は 3 コア M1）であり、この条件より
//! **弱い**。予算判定はこの弱い環境で要件値そのものに対して行う（通れば要件の環境でも
//! 通るとみなす保守的な代理。`.github/workflows/bench.yml` 冒頭の「計測環境」）。
//! 本ベンチは予算判定の根拠として release プロファイル（`cargo bench`）の計測値を
//! 用いる。debug プロファイルのテスト実行（`tests/row_granular_diff.rs` が 10 万行のフル経路を走らせる。同ファイルの
//! docs「3 位置のループを別の（小さい）標本で回す理由」参照）は予算判定に使わない。
//!
//! # 標本（測定条件）
//!
//! * **1 シート・10 万行 × 30 列**（要件 8.1 / 8.2 の規模そのもの）。
//! * **添付なし**（添付バイト列は `from_parts` の借用契約で 1 回複製されるため、
//!   測定条件から除いて `open` / `save` の本体費用だけを測る。tasks.md の申し送り
//!   「task 8.9 の計測対象」参照）。
//! * セル値は行と列で一意な十進整数（決定性があり、値の内容が計測に影響しない）。
//! * 標本の構築は**測定の外**で 1 回だけ行う。統合テスト/ベンチからは `Sheet::push_row`
//!   等が `pub(crate)` で使えず、`set_row_values` の反復は O(n²)（10 万行で分単位）なので、
//!   `tests/row_granular_diff.rs`（タスク 8.3）と同じ公開経路
//!   （`DocumentParts::from_entries` + `from_parts` の一括復元）で組み立てる。
//!
//! # 規模の証拠と 10 万行超の通知（要件 8.4, 8.5）
//!
//! 測定の前に 1 回だけ、標本が本当に 10 万行 × 30 列で復元されること
//! （[`SUPPORTED_ROW_LIMIT`] と一致すること）を確かめる。これにより、標本の行数を
//! 減らす変更（規模の証拠を壊す変更）はベンチの実行時に失敗する。
//! さらに、10 万行ちょうどの標本は保証対象内（`beyond_supported_scale == false`）であり、
//! 10 万行 + 1 行の標本は**拒否されず**（`Ok` のまま）`beyond_supported_scale == true` に
//! なることを実測する（要件 8.5。境界そのものの回帰は `tests/api.rs` が担うため、
//! ここではベンチと同じ規模・形で 1 回だけ確かめる）。
//!
//! # 実行時間（既定設定では長すぎる）
//!
//! criterion の既定（100 サンプル・3 秒目標）は 10 万行規模では長すぎるため、
//! `sample_size` / `warm_up_time` / `measurement_time` を明示的に絞る。

#[path = "../tests/common/mod.rs"]
mod common;

use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use document_format::container::{AtomicWriter, ContainerCodec};
use document_format::parts::{to_parts, DocumentParts};
use document_format::{
    Document, DocumentFormatApi, EntryName, IdFactory, SchemaPart, SUPPORTED_ROW_LIMIT,
};

use common::{api, entries_of, with_rebuilt_manifest, Scratch, SCHEMA_EMPTY};

/// 標本の行数（要件 8.4 の保証対象そのもの）。
const ROWS: usize = 100_000;

/// 標本の列数（10 万行 × 30 列）。
const COLUMNS: usize = 30;

/// criterion のサンプル数（既定 100 は 10 万行規模では長すぎる）。
const SAMPLE_SIZE: usize = 10;

/// 標本の列名（`c00` … `c29`）。行オブジェクトのキー順になる。
fn columns() -> Vec<String> {
    (0..COLUMNS).map(|index| format!("c{index:02}")).collect()
}

/// 10 万行級の行エントリのバイト列を、決定的な整数値で組み立てる。
///
/// 1 行 = 1 テキスト行（LF 終端）の NDJSON であり、`$id` を先頭キー、続いて列順に
/// `(行位置 * 列数 + 列位置)` の整数を書く。行識別子は公開の [`IdFactory`] から発行する。
/// 構築は `String` への追記だけで行う（O(n)）。
fn rows_ndjson(columns: &[String], row_count: usize) -> Vec<u8> {
    let mut ids = IdFactory::new();
    let mut out = String::with_capacity(row_count * (32 + columns.len() * 12));
    for row_index in 0..row_count {
        out.push_str("{\"$id\":\"");
        out.push_str(&ids.new_row_id().to_string());
        out.push('"');
        for (column_index, column) in columns.iter().enumerate() {
            out.push_str(",\"");
            out.push_str(column);
            out.push_str("\":");
            out.push_str(&(row_index * columns.len() + column_index).to_string());
        }
        out.push_str("}\n");
    }
    out.into_bytes()
}

/// `row_count` 行 × 30 列の文書を公開 API の一括経路で組み立てる。
///
/// 30 列の骨格（`to_parts` が作る `document.json` / `schemas/*`）を取り、行エントリの
/// バイト列だけを差し替えて `from_parts` で復元する。`from_parts` は復号済みの行を
/// `Sheet::extend_rows` の一括経路で入れるため O(n) である（モジュール docs
/// 「標本（測定条件）」参照）。
fn sample_with_rows(row_count: usize) -> Document {
    let mut skeleton = Document::new();
    let sheet = skeleton.add_sheet("大量");
    skeleton
        .set_sheet_columns(sheet, columns())
        .expect("標本のシートは実在する");
    skeleton
        .set_root_schema(sheet, SchemaPart::parse(SCHEMA_EMPTY).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    let parts = to_parts(&skeleton).expect("骨格はパート集合へ取り出せる");
    let version = parts.format_version();

    let mut entries = entries_of(&parts);
    let rows_entry = EntryName::Rows { sheet };
    let slot = entries
        .iter()
        .position(|(name, _)| *name == rows_entry)
        .expect("骨格は行エントリを持つ");
    entries[slot].1 = rows_ndjson(&columns(), row_count);

    let parts = DocumentParts::from_entries(with_rebuilt_manifest(version, entries))
        .expect("行を差し替えた集合は妥当");
    api()
        .from_parts(&parts)
        .expect("行を差し替えた集合は復元できる")
}

/// 10 万行 × 30 列の `open` / `save` を計測する（予算: 開く 3 秒 / 保存 2 秒）。
fn large_document(c: &mut Criterion) {
    let scratch = Scratch::new("bench_large_document");
    let sample = sample_with_rows(ROWS);

    // 規模の証拠（測定の外で 1 回だけ検証する）。標本の行数を減らす変更や列数を変える
    // 変更はここで失敗する。行数は要件 8.4 の公開定数と比較し、列数は要件
    // 8.1 / 8.2 が定める計測条件のリテラル（30）と比較する（標本の定数ではなく
    // 要件値を literal で置くことで、標本の規模を黙って縮める変更を検出する）。
    assert_eq!(
        sample.sheets()[0].rows().len(),
        SUPPORTED_ROW_LIMIT,
        "標本の行数が保証対象（要件 8.4）と一致しない"
    );
    assert_eq!(
        sample.sheets()[0].columns().len(),
        30,
        "標本の列数が要件 8.1 / 8.2 の計測条件（10 万行 × 30 列）と一致しない"
    );

    let path = scratch.file("large.jxcel");
    api().save(&sample, &path).expect("標本は保存できる");

    // 要件 8.5: 10 万行 × 30 列（ちょうど保証対象の上限）は保証対象内である。
    let outcome = api().open(&path).expect("標本は開ける");
    assert!(
        !outcome.beyond_supported_scale,
        "10 万行 × 30 列が保証対象外とされた（要件 8.5）"
    );
    drop(outcome);

    // 要件 8.5: 上限を超えても拒否されず、保証対象外の通知が立つ。測定の外で 1 回だけ
    // 確かめる（境界そのものの回帰は tests/api.rs が担う）。
    {
        let over = sample_with_rows(ROWS + 1);
        let over_path = scratch.file("over.jxcel");
        api()
            .save(&over, &over_path)
            .expect("超過した標本も保存できる");
        let outcome = api()
            .open(&over_path)
            .expect("超過しても拒否しない（要件 8.5）");
        assert!(
            outcome.beyond_supported_scale,
            "10 万行 + 1 行が保証対象内とされた（要件 8.5）"
        );
    }

    // 測定の外で作った標本とファイルを使い、release プロファイルで計測する。
    let mut group = c.benchmark_group("large_document");
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(Duration::from_secs(3));
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("open_100k_rows_x_30_columns", |b| {
        b.iter(|| {
            let outcome = api().open(&path).expect("標本は開ける");
            // モデルを実際に構築させる（呼び出しが最適化で消えないようにする）。
            std::hint::black_box(outcome.document.sheets()[0].rows().len())
        })
    });

    group.bench_function("save_100k_rows_x_30_columns", |b| {
        b.iter(|| {
            api()
                .save(std::hint::black_box(&sample), &path)
                .expect("標本は保存できる")
        })
    });

    group.finish();
}

/// 10 万行 × 30 列の `save` を 3 段に分けて計測する（予算判定には使わない）。
///
/// `save` は「パート構築（不変条件の検証・行の JSON 符号化・ダイジェスト）→ コンテナ化
/// （ZIP と圧縮）→ 原子的書き込み（一時ファイル・`sync_all`・`rename`）」の 3 段である
/// （`DocumentFormatApi::save`）。予算超過や回帰が起きたときに、どの段が時間を使って
/// いるかを CI の記録から直接読めるようにする。各段の入力は測定の外で 1 回だけ作る。
fn save_phases(c: &mut Criterion) {
    let scratch = Scratch::new("bench_save_phases");
    let sample = sample_with_rows(ROWS);
    let parts = api()
        .to_parts(&sample)
        .expect("標本はパート集合へ取り出せる");
    let bytes = ContainerCodec::encode(&parts).expect("標本は符号化できる");
    let path = scratch.file("phases.jxcel");

    let mut group = c.benchmark_group("save_phases");
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(Duration::from_secs(3));
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("1_to_parts", |b| {
        b.iter(|| {
            api()
                .to_parts(std::hint::black_box(&sample))
                .expect("標本はパート集合へ取り出せる")
        })
    });

    group.bench_function("2_container_encode", |b| {
        b.iter(|| ContainerCodec::encode(std::hint::black_box(&parts)).expect("標本は符号化できる"))
    });

    group.bench_function("3_atomic_commit", |b| {
        b.iter(|| {
            AtomicWriter::commit(&path, std::hint::black_box(&bytes)).expect("標本は書き込める")
        })
    });

    group.finish();
}

criterion_group!(benches, large_document, save_phases);
criterion_main!(benches);

//! 性能ベンチマーク: 10 万行 × 30 列のシートの窓の符号化・順序の再計算・1 万行の貼り付け
//! （tasks.md 9.1。要件 11.5, 11.6, 11.7）。
//!
//! # 計測する対象（design.md「Performance/Load」の 4 本。うち予算の判定は 1 本）
//!
//! | bench id | 計測 | 予算 |
//! |---|---|---|
//! | `large_grid/encode_window` | 10 万行 × 30 列のシートの**可視 1 窓**（256 行 × 30 列）の符号化 | 絶対値なし（要件 11.2 の 1 秒に収まることを記録する） |
//! | `large_grid/encode_window_10k` | 同じ符号化を**1 万行**の標本で測る（10 倍の行数との比較。要件 11.6） | 絶対値なし |
//! | `large_grid/recompute_order` | 10 万行 × **2 基準列**の並べ替えと絞り込み（`GridSession::set_view` のフル経路） | 絶対値なし |
//! | `large_grid/paste_10k` | **1 万行 × 30 列**の貼り付けの適用（`EditCommand::PasteRange`） | **3 秒**（要件 11.5） |
//!
//! **予算として機械判定するのは `large_grid/paste_10k` だけである**（要件 11.5 が絶対値を
//! 定める唯一の計測であり、`scripts/check-bench-budget.sh` がその平均を要件値と比較する）。
//! 残る 3 本は**計測値を criterion のレポートに残す**ためのものであり、判定器へは足さない
//! （要件 11.1 / 11.2 / 11.3 の判定の場は**実画面の観測**であり、ベンチではない —
//! design.md「Performance & Scalability」の表）。
//!
//! # 要件 11.6（表示のための資源を行数に比例させない）の材料
//!
//! `encode_window`（10 万行）と `encode_window_10k`（1 万行）は**同じ 1 窓**を符号化する。
//! 符号化は[`GridSession::encode_window`] が可視の順序から窓の行だけを取り出すため、
//! **行数を 10 倍にしても費用が増えない**ことをこの 2 本の比が示す（実測は
//! `.kiro/specs/data-grid/research.md`「窓の符号化の費用は行数に比例しない」）。
//!
//! # 標本はテストと共有する（tasks.md 1.4）
//!
//! 10 万行 × 30 列の標本の組み立ては `tests/common/sample.rs` の生成器が唯一の源であり、
//! 本ベンチはその入口（[`common::sample::sample`]）だけを使う（写しを作らない）。`benches/`
//! から `tests/` のモジュールをそのままでは取り込めないため、相対パスで取り込む
//! （`crates/document-format/benches/large_document.rs` と同じ形）。
//!
//! 標本の規模は**要件値のリテラル**（10 万行・30 列・1 万行）で組み立て、組み上がった標本の
//! 形を表明する — 標本の行数を黙って縮める変更はベンチの実行時に失敗する。
//!
//! # 実行時間（既定設定では長すぎる）
//!
//! criterion の既定（100 サンプル・3 秒目標）は 10 万行規模では長すぎるため、
//! `sample_size` / `warm_up_time` / `measurement_time` を明示的に絞る（同じ形は
//! `crates/document-format/benches/large_document.rs` にある）。1 万行の貼り付けは 1 回の
//! 適用に秒単位かかるため、目標の計測時間を長く取る。

#[path = "../tests/common/mod.rs"]
mod common;

use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use data_grid::{
    decode_window, CellAddress, ColumnIndex, EditCommand, FilterSpec, GridSession, PasteCodec,
    RowOrdinal, RowSpan, SortKey, UndoStack, ViewSpec,
};

use common::sample::{sample, SampleOptions};

/// 標本の行数（要件 11.7 が定める計測条件そのもの。10 万行 × 30 列）。
const ROWS: usize = 100_000;

/// 標本の列数（要件 11.7 の計測条件の列数の側）。
const COLUMNS: usize = 30;

/// 可視 1 窓の行数（`src/features/grid/windowCache.ts` の `WINDOW_ROWS`）。
///
/// 窓の幅は移植口の先読みの単位であり、画面の高さではない（design.md「WindowCache」の表・
/// research.md「窓の行数は 256 行である」）。
const WINDOW_ROWS: usize = 256;

/// 1 万行の貼り付けの行数（要件 7.7 / 11.5 の規模そのもの）。
const PASTE_ROWS: usize = 10_000;

/// 貼り付けるテキストの元にする行の添字（標本の行の値をそのまま写すため）。
///
/// 行 0 は列 0 に違反を 1 件持つ（標本は列優先の平坦添字で等間隔に違反を仕込むため、
/// 平坦添字 0 = 行 0 列 0 が必ず違反になる）。**適合する値だけを貼る**ために、違反を 1 つも
/// 持たない行 1 を使う（行 1〜999 には 30 列のどこにも違反が無い）。
const PASTE_SOURCE_ROW: usize = 1;

/// 並べ替えの第 1 基準の列（`備考`。値の種類が少なく同値が多いため、第 2 基準が効く）。
const SORT_PRIMARY: usize = 19;

/// 並べ替えの第 2 基準の列（`ロット番号`。書式つきの文字列であり、ほぼ一意である）。
const SORT_SECONDARY: usize = 21;

/// 絞り込みの対象の列（`カテゴリ`。4 つの選択肢を持つ）。
const FILTER_COLUMN: usize = 18;

/// 絞り込みが残す値（4 分の 1 の行が可視に残る。並べ替えの対象が 0 行にならない）。
const FILTER_TEXT: &str = "資材";

/// criterion のサンプル数（既定 100 は 10 万行規模では長すぎる）。
const SAMPLE_SIZE: usize = 10;

/// 窓の符号化と順序の再計算の目標計測時間。
const MEASUREMENT: Duration = Duration::from_secs(10);

/// 1 万行の貼り付けの目標計測時間（1 回の適用に秒単位かかるため長く取る）。
const PASTE_MEASUREMENT: Duration = Duration::from_secs(30);

/// 10 万行 × 30 列の標本を組み立て、形を要件値と突き合わせる。
fn large_sample() -> common::sample::Sample {
    let sample = sample(&SampleOptions::new(ROWS, COLUMNS));
    assert_eq!(
        sample.rows(),
        ROWS,
        "標本の行数が要件 11.7 の計測条件と一致しない"
    );
    assert_eq!(
        sample.column_count(),
        COLUMNS,
        "標本の列数が要件 11.7 の計測条件と一致しない"
    );
    sample
}

/// 可視 1 窓の符号化を、10 万行と 1 万行の 2 つの標本で計測する（要件 11.6 の比較）。
///
/// どちらの標本も**計測の外で 1 回だけ**組み立て、順序の導出（`set_view`。索引の組み立てを
/// 含む）も計測の外で 1 回だけ行う — 測るのは [`GridSession::encode_window`] の費用である。
fn encode_window(c: &mut Criterion) {
    let large = large_sample();
    let mut large_session =
        GridSession::open(large.sheet(), large.compiled()).expect("セッションを開ける");
    large_session
        .set_view(large.document(), ViewSpec::default())
        .expect("表示を指定できる");
    assert_eq!(
        large_session.visible_row_count(),
        ROWS,
        "表示の指定（恒等）で可視の行数が変わった"
    );

    let small = sample(&SampleOptions::new(PASTE_ROWS, COLUMNS));
    assert_eq!(
        small.rows(),
        PASTE_ROWS,
        "比較用の標本の行数が 1 万行でない"
    );
    let mut small_session =
        GridSession::open(small.sheet(), small.compiled()).expect("セッションを開ける");
    small_session
        .set_view(small.document(), ViewSpec::default())
        .expect("表示を指定できる");

    let span = RowSpan::new(RowOrdinal::new(0), WINDOW_ROWS);
    // 窓が実際に 256 行 × 30 列を運ぶことを計測の外で確かめる（窓を空にする変更・行数を
    // 縮める変更を検出する）。復号の結果を表明するのは、バイト列の長さだけでは
    // 「行が入っていない窓」を排除できないためである。
    for (label, session, document) in [
        ("10万行", &large_session, large.document()),
        ("1万行", &small_session, small.document()),
    ] {
        let bytes = session
            .encode_window(document, span)
            .expect("窓を符号化できる");
        let decoded = decode_window(&bytes).expect("窓を復号できる");
        assert_eq!(
            (decoded.row_count(), decoded.columns()),
            (WINDOW_ROWS, COLUMNS),
            "{label}の標本: 窓が 256 行 × 30 列を運んでいない"
        );
    }

    let mut group = c.benchmark_group("large_grid");
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(Duration::from_secs(3));
    group.measurement_time(MEASUREMENT);

    group.bench_function("encode_window", |b| {
        b.iter(|| {
            std::hint::black_box(
                large_session
                    .encode_window(large.document(), span)
                    .expect("窓を符号化できる"),
            )
        })
    });

    // 行数を 10 分の 1 にした標本の同じ符号化（要件 11.6 の比較。判定器へは足さない）。
    group.bench_function("encode_window_10k", |b| {
        b.iter(|| {
            std::hint::black_box(
                small_session
                    .encode_window(small.document(), span)
                    .expect("窓を符号化できる"),
            )
        })
    });

    group.finish();
}

/// 10 万行 × 2 基準列の並べ替えと絞り込みを計測する（`GridSession::set_view` のフル経路）。
///
/// 計測するのは**表示の指定を変える費用**そのものである: 絞り込みで可視の行を選び、2 本の
/// 基準列で並べ替え、違反の索引の鍵を新しい順序へ張り直す（`GridSession::set_view`）。
/// 索引の組み立て（シート全件の検証）は計測の外で 1 回だけ行う（同じ理由は
/// `crates/document-format/benches/large_document.rs` の「実行時間」にある）。
fn recompute_order(c: &mut Criterion) {
    let sample = large_sample();
    let doc = sample.document();
    let mut session =
        GridSession::open(sample.sheet(), sample.compiled()).expect("セッションを開ける");
    session
        .set_view(doc, ViewSpec::default())
        .expect("表示を指定できる");

    let spec = ViewSpec {
        sort: vec![
            SortKey {
                column: ColumnIndex::new(SORT_PRIMARY),
                descending: false,
            },
            SortKey {
                column: ColumnIndex::new(SORT_SECONDARY),
                descending: true,
            },
        ],
        filters: vec![FilterSpec::Contains {
            column: ColumnIndex::new(FILTER_COLUMN),
            text: FILTER_TEXT.to_owned(),
        }],
    };

    // 計測の外で 1 度適用し、**絞り込みが実際に効いている**こと（可視の行が減り、0 には
    // ならないこと）を確かめる。絞り込みが全行を通す変更はここで失敗する。
    let summary = session
        .set_view(doc, spec.clone())
        .expect("表示を指定できる");
    assert!(
        summary.visible > 0 && summary.visible < ROWS,
        "絞り込みが可視の行を選んでいない（可視 {} 行）",
        summary.visible
    );
    assert_eq!(
        summary.visible + summary.hidden,
        ROWS,
        "可視と隠された行の和がシートの行数と一致しない"
    );

    let mut group = c.benchmark_group("large_grid");
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(Duration::from_secs(3));
    group.measurement_time(MEASUREMENT);

    group.bench_function("recompute_order", |b| {
        b.iter(|| {
            std::hint::black_box(
                session
                    .set_view(doc, spec.clone())
                    .expect("表示を指定できる")
                    .visible,
            )
        })
    });

    group.finish();
}

/// 1 万行 × 30 列の貼り付けの適用を計測する（予算: 3 秒。要件 7.7, 11.5）。
///
/// # 標本（測定条件）
///
/// * **1 万行 × 30 列**（要件 7.7 / 11.5 の規模そのもの）を、10 万行 × 30 列のシートの先頭へ
///   貼る（行の補充は起きない — 貼り付けの範囲は既存の行数の内側である。要件 7.4 の腕は
///   範囲が行数を超えるときに働く）。
/// * 貼り付けるテキストは**標本の行の値を [`PasteCodec::write`] で表形式へ書いたもの**で
///   あり、形式の規則はその 1 箇所（`edit/paste.rs`）に任せる（本ベンチは写しを作らない）。
///   元にするのは違反を 1 つも持たない行である（適合する値の貼り付けを測る）。
/// * 計測の外で 1 度適用し、**矩形の行数ぶんを書いたこと**（`affected` が 1 万行であること）
///   と、行数が変わっていないことを確かめる。
///
/// # 履歴の上限
///
/// **上限が変えるのは保持する件数だけである** — 逆命令の材料（覆った行の変更前の値）は
/// 適用の側で組み立てられ、**本計測の内側**にある（`edit` 層の `paste_range_with_inverse` の
/// `restore_values`。`UndoStack` はそれを積むか捨てるかだけを決める）。しかし同じ貼り付けを
/// 反復するベンチでは、既定の上限（1,000）が 1 万行 × 30 列ぶんの逆命令を反復ごとに積み上げ、
/// **計測が資源で落ちる**。したがって保持の件数だけを 0 にし、「貼り付けが完了するまでの
/// 費用」（要件 11.5 が定めるもの）を測る。
fn paste_10k(c: &mut Criterion) {
    let sample = large_sample();
    // 貼り付ける値の元にする行が**違反を 1 つも持たない**ことを確かめる（適合する値の
    // 貼り付けを測るという測定条件そのもの。行 0 は平坦添字 0 の違反を持つ）。
    assert!(
        (0..COLUMNS).all(|column| !sample.violation_at(PASTE_SOURCE_ROW, column)),
        "貼り付ける元の行（行 {PASTE_SOURCE_ROW}）が違反を持っている"
    );
    let plan = sample.compiled();
    let parts = sample.into_edit_parts();
    let source = parts
        .document
        .sheet_by_id(parts.sheet)
        .expect("標本のシートは文書にある")
        .rows()[PASTE_SOURCE_ROW]
        .values()
        .to_vec();

    // 貼り付けるテキスト（表形式の規則は `PasteCodec` が唯一の源である）。
    let line = PasteCodec::write(&[source]);
    let mut text = String::with_capacity((line.len() + 1) * PASTE_ROWS);
    for index in 0..PASTE_ROWS {
        if index > 0 {
            text.push('\n');
        }
        text.push_str(&line);
    }
    assert_eq!(
        text.lines().count(),
        PASTE_ROWS,
        "貼り付けるテキストの行数が 1 万行でない"
    );
    assert_eq!(
        PasteCodec::parse(&line)[0].len(),
        COLUMNS,
        "貼り付けるテキストの列数が 30 列でない"
    );

    let mut doc = parts.document;
    let mut session = GridSession::open(parts.sheet, plan).expect("セッションを開ける");
    session
        .set_view(&doc, ViewSpec::default())
        .expect("表示を指定できる");
    let mut history = UndoStack::new(0);

    // **表示の並びは呼び出し側が渡さない** — 空の並びを渡すとセッションが自分の可視の順序で
    // 埋める（`GridSession::fill_paste`）。**画面はこれと違う形で送る**: 画面は矩形ぶんの可視行を
    // 明示して `rows` に載せる（`./clipboard` の `planPaste`）。**この計測は空の並びを通るぶん
    // 生産より重い（保守側）** ので、3 秒のゲートは弱まらない（research.md の「限界」に記録）。
    let command = EditCommand::PasteRange {
        anchor: CellAddress::new(parts.row_ids[0], ColumnIndex::new(0)),
        rows: Vec::new(),
        text,
    };

    let outcome = session
        .apply(&mut doc, &mut history, command.clone())
        .expect("貼り付けは適用できる");
    assert_eq!(
        outcome.affected.len(),
        PASTE_ROWS,
        "貼り付けが 1 万行を書いていない"
    );
    assert_eq!(
        outcome.row_count, ROWS,
        "貼り付けで行数が変わった（行の補充が起きた）"
    );

    let mut group = c.benchmark_group("large_grid");
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(Duration::from_secs(3));
    group.measurement_time(PASTE_MEASUREMENT);

    group.bench_function("paste_10k", |b| {
        b.iter(|| {
            let outcome = session
                .apply(&mut doc, &mut history, command.clone())
                .expect("貼り付けは適用できる");
            // 書いた行を実際に読ませる（適用が最適化で消えないようにする）。
            std::hint::black_box(outcome.affected.len())
        })
    });

    group.finish();
}

criterion_group!(benches, encode_window, recompute_order, paste_10k);
criterion_main!(benches);

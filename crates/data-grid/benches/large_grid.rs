//! 性能ベンチマーク: 10 万行 × 30 列のシートの窓の符号化・順序の再計算・1 万行の貼り付け・
//! 10 万行の変更の取り消し（tasks.md 9.1。要件 11.5, 11.6, 11.7）。
//!
//! # 計測する対象（design.md「Performance/Load」の 4 本 + 取り消しの 1 本。うち予算の判定は 1 本）
//!
//! | bench id | 計測 | 予算 |
//! |---|---|---|
//! | `large_grid/encode_window` | 10 万行 × 30 列のシートの**可視 1 窓**（256 行 × 30 列）の符号化 | 絶対値なし（要件 11.2 の 1 秒に収まることを記録する） |
//! | `large_grid/encode_window_10k` | 同じ符号化を**1 万行**の標本で測る（10 倍の行数との比較。要件 11.6） | 絶対値なし |
//! | `large_grid/recompute_order` | 10 万行 × **2 基準列**の並べ替えと絞り込み（`GridSession::set_view` のフル経路） | 絶対値なし |
//! | `large_grid/paste_10k` | **1 万行 × 30 列**の貼り付けの適用（`EditCommand::PasteRange`） | **3 秒**（要件 11.5） |
//! | `large_grid/undo_100k` | **10 万行 × 30 列**の変更の**取り消し**（`GridSession::undo`。要件 11.7 の規模） | 絶対値なし |
//!
//! **予算として機械判定するのは `large_grid/paste_10k` だけである**（要件 11.5 が絶対値を
//! 定める唯一の計測であり、`scripts/check-bench-budget.sh` がその平均を要件値と比較する）。
//! 残る 4 本は**計測値を criterion のレポートに残す**ためのものであり、判定器へは足さない
//! （要件 11.1 / 11.2 / 11.3 の判定の場は**実画面の観測**であり、ベンチではない —
//! design.md「Performance & Scalability」の表）。`large_grid/undo_100k` は 10 万行の変更の
//! 取り消しの費用を固定する（`edit` 層の `EditApply::write_material_rows` が材料ごとに行を
//! 線形探索していた欠陥の再発を、値として見えるようにする）。
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

use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion};
use data_grid::{
    decode_window, display_text, CellAddress, ColumnIndex, EditCommand, FilterSpec, GridSession,
    PasteCodec, RowOrdinal, RowSpan, SortKey, UndoStack, ViewSpec,
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
/// 標本で唯一の一意制約つきの列（`品番`。標本の宣言の並びの先頭である）。
///
/// 取り消しの計測（[`undo_100k`]）だけが使う — その列だけは**行ごとの値**を打つ。
/// 1 行の値を全行へ写すと一意制約の違反が 10 万件生まれ、計測が取り消しの書き込みではなく
/// **違反の索引の更新**になる（同ベンチの「標本」）。
const UNIQUE_COLUMN: usize = 0;

/// 取り消しの計測（[`undo_100k`]）が打つ列 — 標本の 30 列から、**表形式のテキストから値を
/// 復元できない 4 列**を除いた 26 列。
///
/// 除くのは `添付`（列 9）と入れ子の 3 列（`届け先` 10・`明細` 11・`改訂履歴` 27）である。
/// これらの列に打てる文字は値（添付の識別子・入れ子の構造）を復元できず、**書けば 1 列あたり
/// 10 万件の違反が生まれる** — 計測が取り消しの書き込みではなく違反の索引の更新になる。
/// 打つ列を絞っても**計測する経路は変わらない**: 取り消しの材料は触れた行の**値の並び全体**
/// （30 列）であり、差し戻しは 10 万行 × 30 列を書く（同ベンチの assertion が
/// `undone.revalidated_columns` で 30 列を見たことを確かめる）。
const EDITED_COLUMNS: [usize; 26] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 28, 29,
];

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

/// 10 万行 × 30 列の変更の**取り消し**を計測する（要件 11.7 の規模そのもの）。
///
/// # 測るもの
///
/// [`GridSession::undo`] の費用**そのもの**である。適用（[`GridSession::apply`]）は計測の外に
/// 置く — 取り消しの直後は適用前の状態へ戻っているため、反復ごとに「適用 → 取り消し」を
/// 1 対で走らせ、**取り消しの側だけを時計で囲む**（`Bencher::iter_custom`。criterion は
/// 返した合計を反復数で割るため、報告される mean は 1 回の取り消しの時間である）。適用を
/// 計測に含めると、本計測の対象（取り消し）の費用が適用の費用に埋もれる。
///
/// # なぜこの計測が要るか
///
/// 取り消しの逆命令（`HistoryCommand::RestoreValues`）は、覆った行の**値の並び**を材料として
/// 運ぶ（`edit` 層の `EditApply::write_material_rows`）。材料ごとに `Document::set_row_values`
/// を呼ぶ形は、上流が材料ごとに対象行を線形探索するため O(行数 × 材料数) になり、10 万行の
/// 取り消しが実測で 35 秒台に落ちる（材料も 10 万行である）。本計測はこの経路の費用を固定する。
///
/// # 標本（測定条件）
///
/// * **10 万行**の全行に `SetCells` を打つ。打つのは [`EDITED_COLUMNS`] の 26 列である
///   （標本の 30 列から、表形式のテキストから値を復元できない 4 列を除く。理由は同定数の
///   docs）。**取り消しの材料は触れた行の値の並び全体（30 列）である**ため、差し戻しは
///   10 万行 × 30 列を書く — 本計測の対象はこの書き込みである。
/// * 打つ文字は 2 つの源から作る（どちらも本ベンチは値の写しを作らない）:
///   **一意な列**（[`UNIQUE_COLUMN`]）は**その行自身の値**の表示文字列（`display_text` が
///   唯一の源）を打ち、**残りの列**は違反を 1 つも持たない行（[`PASTE_SOURCE_ROW`]）の値の
///   表形式表現（[`PasteCodec`] が唯一の源）を打つ。行ごとに違う値を打つのは、標本の一意な列を
///   一意なまま保つためである — 1 行の値を全行へ写すと 10 万件の重複の違反が生まれ、計測が
///   **取り消しの書き込み**ではなく**違反の索引の更新**（`view` 層の差分）を測ってしまう。
/// * 計測の外で 1 度適用し、**規模と経路**を確かめる — 適用が 10 万行を書き・打った列を
///   再検証し・違反の総数が行数に対して小さいこと、取り消しが 10 万行を戻し・**30 列**を
///   見ていること（材料が行の値の並び全体であること）。規模を黙って縮める変更・計測の対象を
///   取り違える変更はここで失敗する。
/// * 履歴の上限は 1 である（材料が 10 万行 × 30 列ぶんあるため、上限を既定の 1,000 のままに
///   すると保持が積み上がる。取り消しの費用は保持する件数に依らない）。
fn undo_100k(c: &mut Criterion) {
    let sample = large_sample();
    // 打つ文字の元にする行が**違反を 1 つも持たない**ことを確かめる（適合する値を打つという
    // 測定条件そのもの。行 0 は平坦添字 0 の違反を持つ）。
    assert!(
        (0..COLUMNS).all(|column| !sample.violation_at(PASTE_SOURCE_ROW, column)),
        "打つ文字の元にする行（行 {PASTE_SOURCE_ROW}）が違反を持っている"
    );
    let plan = sample.compiled();
    let parts = sample.into_edit_parts();
    assert_eq!(parts.row_ids.len(), ROWS, "標本の行数が 10 万行でない");
    assert_eq!(parts.columns.len(), COLUMNS, "標本の列数が 30 列でない");

    // 打つ文字（上の「標本」の 2 つの源。値の写しを作らない）。
    let sheet = parts
        .document
        .sheet_by_id(parts.sheet)
        .expect("標本のシートは文書にある");
    let source = sheet.rows()[PASTE_SOURCE_ROW].values().to_vec();
    let texts = PasteCodec::parse(&PasteCodec::write(&[source]));
    assert_eq!(texts.len(), 1, "表形式の往復が 1 行を返さない");
    assert_eq!(texts[0].len(), COLUMNS, "打つ文字の列数が 30 列でない");
    let conforming = texts.into_iter().next().expect("1 行が返る");

    // 10 万行 × [`EDITED_COLUMNS`] のセルを名指す命令（行の順に、列の昇順に並べる）。
    let mut cells: Vec<(CellAddress, String)> = Vec::with_capacity(ROWS * EDITED_COLUMNS.len());
    for row in sheet.rows() {
        let key = display_text(&row.values()[UNIQUE_COLUMN]).into_owned();
        for &column in &EDITED_COLUMNS {
            let text = if column == UNIQUE_COLUMN {
                key.clone()
            } else {
                conforming[column].clone()
            };
            cells.push((CellAddress::new(row.id(), ColumnIndex::new(column)), text));
        }
    }
    assert_eq!(
        cells.len(),
        ROWS * EDITED_COLUMNS.len(),
        "命令のセル数が 10 万行 × 26 列でない"
    );
    let command = EditCommand::SetCells { cells };

    let mut doc = parts.document;
    let mut session = GridSession::open(parts.sheet, plan).expect("セッションを開ける");
    session
        .set_view(&doc, ViewSpec::default())
        .expect("表示を指定できる");
    let mut history = UndoStack::new(1);

    // 計測の外で 1 度適用し、**規模と経路**を確かめる（適用の行数・列数と、取り消しが戻す
    // 行数）。取り消しの直後は適用前の状態であり、計測の反復の初期状態でもある。
    let applied = session
        .apply(&mut doc, &mut history, command.clone())
        .expect("適用できる");
    assert_eq!(applied.affected.len(), ROWS, "適用が 10 万行を書いていない");
    assert_eq!(
        applied.revalidated_columns.len(),
        EDITED_COLUMNS.len(),
        "適用が打った列を再検証していない"
    );
    assert!(
        applied.violation_total < ROWS / 10,
        "適用後の違反の総数が行数に対して大きすぎる（{} 件）— 計測が違反の索引の更新を測ってしまう",
        applied.violation_total
    );
    assert_eq!(applied.row_count, ROWS, "適用で行数が変わった");
    let undone = session
        .undo(&mut doc, &mut history)
        .expect("取り消せる")
        .expect("履歴に対がある");
    assert_eq!(
        undone.affected.len(),
        ROWS,
        "取り消しが 10 万行を戻していない"
    );
    assert_eq!(
        undone.revalidated_columns.len(),
        COLUMNS,
        "取り消しが 30 列を見ていない（材料が行の値の並び全体でない）"
    );
    assert_eq!(undone.row_count, ROWS, "取り消しで行数が変わった");

    let mut group = c.benchmark_group("large_grid");
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(Duration::from_secs(3));
    group.measurement_time(PASTE_MEASUREMENT);

    group.bench_function("undo_100k", |b| {
        b.iter_custom(|iterations| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iterations {
                // 適用は計測の外（取り消しの直後は適用前の状態である）。
                session
                    .apply(&mut doc, &mut history, command.clone())
                    .expect("適用できる");
                let started = Instant::now();
                let outcome = session.undo(&mut doc, &mut history).expect("取り消せる");
                elapsed += started.elapsed();
                // 戻した行を実際に読ませる（取り消しが最適化で消えないようにする）。
                std::hint::black_box(outcome.expect("履歴に対がある").affected.len());
            }
            elapsed
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    encode_window,
    recompute_order,
    paste_10k,
    undo_100k
);
criterion_main!(benches);

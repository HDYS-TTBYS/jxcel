//! 現実に寄せた 30 列の標本そのものの検証（タスク 9.1。要件 10.6）。
//!
//! `tests/common/schema.rs` は群 9 の 3 つの検証（9.2 の予算の計測・9.3 の順序の決定性・
//! 9.4 の拡張型の隔離）が共有する標本の組み立てである。その正しさ — 10 万行 × 30 列が
//! コンパイルと検証を通り、**指定した割合の違反が指定した位置に**現れること — は
//! このファイルが検証する。設計が挙げる機能別のテスト（`validation.rs` 等）は、この共有
//! モジュールを標本の組み立てに使う側である。
//!
//! 3 つの検証がこの標本を共有するという要求（tasks.md 9.1）は、標本の入口が 1 つ
//! （[`schema_sample`]）であり、行数・違反の割合・一意制約の重複の有無・一括判定の失敗の
//! 有無だけが [`SchemaSampleOptions`] で変わることで満たしている。
//!
//! **生の値は比較に使えない。** 標本の値と宣言には実行ごとに発行される識別子（参照先の行の
//! ULID と型定義の識別子）が現れるため、同じ引数の 2 回の組み立てでもバイト列は変わる。
//! 9.3 が依拠してよいのは**違反の位置と理由の種別**の並びであり、このファイルの決定性の
//! 検査はその形を実際に固定する（`tests/common/schema.rs` の docs「決定性 — 何が同一で、
//! 何が同一でないか」）。

mod common;

use std::collections::{BTreeSet, HashMap};

use common::schema::{
    schema_sample, SchemaSample, SchemaSampleOptions, BATCH_CUSTOM_COLUMN, SAMPLE_COLUMNS,
    SAMPLE_ROWS, SAMPLE_VIOLATION_RATIO, SIMPLE_CUSTOM_COLUMN, UNIQUE_GROUPS,
};
use document_format::RowId;
use schema_engine::{
    SchemaEngine, SchemaEngineApi, SheetReport, ValidationOptions, ViolationReason,
};

/// 標本の 30 列が、タスク 9.1 が挙げる要素（10 進数・日時・書式つき文字列・シート間参照・
/// 入れ子・拡張型）を実際に持ち、どの列も使用不能にならない（要件 10.6）。
///
/// 小さな行数で見るのは標本の**形**であり、規模は
/// [`the_hundred_thousand_row_sample_compiles_and_validates_at_the_requested_ratio`] が見る。
#[test]
fn the_sample_declares_the_thirty_realistic_columns() {
    let engine = SchemaEngine::new();
    let sample = schema_sample(&SchemaSampleOptions::new(64));
    let schema = sample.compiled();

    assert_eq!(SAMPLE_COLUMNS, sample.columns().len(), "列数が 30 でない");
    assert_eq!(SAMPLE_COLUMNS, schema.column_count(), "計画の列数が違う");
    assert_eq!(
        sample.columns(),
        engine.columns(&schema),
        "供給した列の並び順が宣言と違う"
    );
    assert!(
        schema.unusable_columns().is_empty(),
        "使用不能な列がある: {:?}",
        schema.unusable_columns()
    );

    // タスク 9.1 が列挙する要素の代表（種別ごとに 1 列以上）。
    for name in [
        "単価",           // 10 進数（桁つき）
        "金額",           // 10 進数（範囲なし）
        "確定日時",       // 日時（オフセットを要求）
        "登録日時",       // 日時（オフセットを禁ずる）
        "検査日時",       // 日時（範囲つき）
        "品番",           // 書式つき文字列（かつ一意）
        "ロット番号",     // 書式つき文字列
        "仕入先",         // シート間参照
        "担当者",         // シート間参照
        "届け先",         // 入れ子（名前つき型定義への参照）
        "明細",           // 入れ子（配列）
        "改訂履歴",       // 入れ子（配列）
        "届け先郵便番号", // 拡張型（一括判定を上書きした実装）
        "請求先郵便番号", // 拡張型（既定実装のままの実装）
        "添付",           // 添付参照
        "機械可読値",     // ANY
        "予備率",         // 小数
    ] {
        assert!(
            sample.columns().iter().any(|column| column == name),
            "列 {name} が標本に無い"
        );
    }

    // 一意制約は 1 列（品番）だけであり、9.3 の決定性が見る「2 段の併合」の対象になる。
    assert_eq!(1, schema.unique_columns().len(), "一意制約の列数が違う");

    // 参照先シートは同じ文書にあり、行を持つ（参照が実在する）。
    assert_eq!(64, sample.rows(), "行数が指定どおりでない");
    assert_eq!(64, sample.row_ids().len(), "行識別子が行数と一致しない");
}

/// 10 万行 × 30 列の標本がコンパイルと全件検証を通り、**指定した割合の違反が
/// 違反として仕込んだセルにちょうど 1 件ずつ**現れる（タスク 9.1。要件 10.6）。
///
/// この一致は「適合する値は 1 件も違反を生まない」ことと「違反する値はちょうど 1 件の
/// 違反を生む」ことの両方を同時に示す — 30 列のどれか 1 つでも規則を外せば、
/// 件数か位置のどちらかがずれる。
#[test]
fn the_hundred_thousand_row_sample_compiles_and_validates_at_the_requested_ratio() {
    let engine = SchemaEngine::new();
    let sample = schema_sample(&SchemaSampleOptions::default());
    let schema = sample.compiled();

    assert_eq!(SAMPLE_ROWS, sample.rows(), "行数が 10 万でない");
    assert_eq!(SAMPLE_COLUMNS, schema.column_count(), "列数が 30 でない");
    assert!(schema.unusable_columns().is_empty(), "使用不能な列がある");

    // 仕込んだ違反の件数は「セル数 × 割合（切り捨て）」である（3,000,000 セルの 0.1%）。
    let cells = sample.rows() * SAMPLE_COLUMNS;
    assert_eq!(
        3_000,
        sample.injected_violations(),
        "仕込んだ違反の件数が {cells} セルの {SAMPLE_VIOLATION_RATIO} と一致しない"
    );

    let report = engine.validate_sheet(
        sample.document(),
        sample.sheet(),
        &schema,
        &ValidationOptions::default(),
    );

    // 総件数は仕込んだ件数と一致する（適合する値が違反を生んでいない）。
    assert_eq!(
        sample.expected_violations(),
        report.total_violations(),
        "違反の総件数が仕込んだ件数と違う"
    );
    assert!(!report.is_truncated(), "違反が上限で切れている");

    // 違反の位置は仕込んだセルそのものである（1 セルにつきちょうど 1 件）。
    let index_of: HashMap<RowId, usize> = sample
        .row_ids()
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let reported: Vec<(usize, usize)> = report
        .violations()
        .iter()
        .map(|violation| {
            let row = violation.row().expect("シート全体の検証の違反は行に属する");
            let index = index_of
                .get(&row)
                .copied()
                .expect("違反の行は標本の行である");
            (index, violation.column().index())
        })
        .collect();
    let mut injected: Vec<(usize, usize)> = (0..sample.rows())
        .flat_map(|row| (0..SAMPLE_COLUMNS).map(move |column| (row, column)))
        .filter(|(row, column)| sample.violation_at(*row, *column))
        .collect();
    injected.sort_unstable();
    assert_eq!(
        injected, reported,
        "違反の位置が仕込んだセルと一致しない（適合する値の違反か、違反の重複・取りこぼし）"
    );

    // 違反を持つ行は、違反を仕込んだ行と正確に一致する（要件 5.4 の区別）。
    let injected_rows: BTreeSet<usize> = injected.iter().map(|(row, _)| *row).collect();
    assert_eq!(
        injected_rows.len(),
        report.invalid_rows().len(),
        "違反を持つ行の数が違反を仕込んだ行の数と違う"
    );

    // 30 列が実際に違反の経路を通っている（仕込んだ値の種類が標本の中で生きている）。
    let reasons = report.violations();
    for (name, present) in [
        (
            "範囲外",
            reasons
                .iter()
                .any(|violation| matches!(violation.reason(), ViolationReason::OutOfRange { .. })),
        ),
        (
            "桁超過",
            reasons.iter().any(|violation| {
                matches!(
                    violation.reason(),
                    ViolationReason::PrecisionExceeded { .. }
                )
            }),
        ),
        (
            "長さ超過",
            reasons.iter().any(|violation| {
                matches!(violation.reason(), ViolationReason::LengthOutOfRange { .. })
            }),
        ),
        (
            "書式不一致",
            reasons.iter().any(|violation| {
                matches!(violation.reason(), ViolationReason::PatternMismatch { .. })
            }),
        ),
        (
            "選択肢外",
            reasons.iter().any(|violation| {
                matches!(violation.reason(), ViolationReason::ChoiceNotAllowed { .. })
            }),
        ),
        (
            "型の不一致",
            reasons.iter().any(|violation| {
                matches!(violation.reason(), ViolationReason::TypeMismatch { .. })
            }),
        ),
        (
            "値なし",
            reasons.iter().any(|violation| {
                matches!(violation.reason(), ViolationReason::MissingValue { .. })
            }),
        ),
        (
            "実在しない参照",
            reasons.iter().any(|violation| {
                matches!(violation.reason(), ViolationReason::BrokenReference { .. })
            }),
        ),
        (
            "拡張型の拒否",
            reasons.iter().any(|violation| {
                matches!(violation.reason(), ViolationReason::CustomRejected { .. })
            }),
        ),
        (
            "拡張型の失敗",
            reasons.iter().any(|violation| {
                matches!(violation.reason(), ViolationReason::CustomFailed { .. })
            }),
        ),
    ] {
        assert!(present, "違反の理由 {name} が標本に現れない");
    }
}

/// 同じ標本が**値の違反と一意制約の違反の両方**を出せる（9.3 が跨る経路の決定性を
/// 検査するのに使う形。タスク 9.1 の「群 9 がこの同じ標本を共有する」）。
///
/// 一意制約の違反は [`SchemaSampleOptions::with_unique_collisions`] で混ぜる。ここでは
/// 件数だけを確かめる（並びの決定性そのものは 9.3 の検査である）。
#[test]
fn the_shared_sample_can_carry_value_and_unique_violations_together() {
    let engine = SchemaEngine::new();
    let rows = UNIQUE_GROUPS * 2;
    let sample = schema_sample(&SchemaSampleOptions::new(rows).with_unique_collisions(true));
    let schema = sample.compiled();
    let report = engine.validate_sheet(
        sample.document(),
        sample.sheet(),
        &schema,
        &ValidationOptions::default(),
    );

    assert!(
        sample.unique_violations() > 0,
        "重複による違反が 1 件も無い"
    );
    assert_eq!(
        sample.expected_violations(),
        report.total_violations(),
        "総件数が仕込んだ件数と違う"
    );

    let duplicates = report
        .violations()
        .iter()
        .filter(|violation| matches!(violation.reason(), ViolationReason::Duplicate { .. }))
        .count();
    assert_eq!(
        sample.unique_violations(),
        duplicates,
        "標本が数えた重複の件数と検証の結果が違う"
    );
    assert!(
        report
            .violations()
            .iter()
            .any(|violation| !matches!(violation.reason(), ViolationReason::Duplicate { .. })),
        "値の違反が 1 件も無い"
    );

    // 重複の違反は重複するすべての行を運ぶ（要件 4.7）。
    let duplicate = report
        .violations()
        .iter()
        .find_map(|violation| match violation.reason() {
            ViolationReason::Duplicate { rows, .. } => Some(rows),
            _ => None,
        })
        .expect("重複の違反がある");
    assert_eq!(2, duplicate.len(), "重複する行がすべて載っていない");
}

/// 同じ引数の 2 回の組み立ては**形**と**違反の集合**（位置と理由の種別）が一致する
/// （tasks.md 9.1。9.3 が依拠してよい性質そのものである）。
///
/// 生の値は比較しない — 参照列の値（参照先の行の ULID）と型定義の識別子は実行ごとに
/// 発行され、2 回の組み立てで変わるためである（`tests/common/schema.rs` の docs
/// 「決定性 — 何が同一で、何が同一でないか」）。
#[test]
fn the_same_options_yield_the_same_shape_and_violation_positions() {
    let engine = SchemaEngine::new();
    let validation = ValidationOptions::default();
    let rows = UNIQUE_GROUPS * 2;
    for collisions in [false, true] {
        let source = SchemaSampleOptions::new(rows).with_unique_collisions(collisions);
        let first = schema_sample(&source);
        let second = schema_sample(&source);

        // 形（列数・行数・仕込む違反の数・宣言の構造）は同じ引数で常に同じである。
        assert_eq!(first.rows(), second.rows(), "行数が一致しない");
        assert_eq!(first.columns(), second.columns(), "列の並びが一致しない");
        let (left, right) = (first.compiled(), second.compiled());
        assert_eq!(
            left.column_count(),
            right.column_count(),
            "計画の列数が一致しない"
        );
        assert_eq!(
            left.unique_columns().len(),
            right.unique_columns().len(),
            "一意制約の列数が一致しない"
        );
        assert_eq!(
            first.injected_violations(),
            second.injected_violations(),
            "仕込んだ違反の数が一致しない"
        );
        assert_eq!(
            first.unique_violations(),
            second.unique_violations(),
            "重複によって生じる違反の数が一致しない"
        );

        // 違反の集合は**位置と理由の種別**で一致する（生の値は比較しない）。
        assert!(
            first.expected_violations() > 0,
            "違反が 1 件も無く決定性を検査できない"
        );
        let left_report =
            engine.validate_sheet(first.document(), first.sheet(), &left, &validation);
        let right_report =
            engine.validate_sheet(second.document(), second.sheet(), &right, &validation);
        assert_eq!(
            violation_signature(&first, &left_report),
            violation_signature(&second, &right_report),
            "同じ引数の 2 回の組み立てで違反の位置と理由の種別が変わった"
        );
    }
}

/// 拡張型の一括判定が `Err` を返す値を標本の列に仕込める（tasks.md 9.1 / 9.4 の前提）。
///
/// [`SchemaSampleOptions::with_batch_failure`] を立てると、失敗を**一括判定を上書きした
/// 実装の列**（[`BATCH_CUSTOM_COLUMN`]）で起こす。既定では既定実装のままの列
/// （[`SIMPLE_CUSTOM_COLUMN`]）で起こり、どちらの設定でも標本は `CustomRejected` と
/// `CustomFailed` の両方を含む。9.4 はこの切り替えで一括実装の失敗隔離を見る。
#[test]
fn the_sample_can_carry_a_batch_failure() {
    let engine = SchemaEngine::new();
    let validation = ValidationOptions::default();
    let rows = UNIQUE_GROUPS * 2;

    // 既定: 失敗は既定実装のままの列で起こり、一括実装の列は実装の拒否を運ぶ。
    let plain = schema_sample(&SchemaSampleOptions::new(rows));
    let schema = plain.compiled();
    let report = engine.validate_sheet(plain.document(), plain.sheet(), &schema, &validation);
    assert_eq!(
        injected_in(&plain, SIMPLE_CUSTOM_COLUMN),
        positions_with_reason(&plain, &report, "CustomFailed"),
        "既定で失敗を起こす列が既定実装のままの列でない"
    );
    assert_eq!(
        injected_in(&plain, BATCH_CUSTOM_COLUMN),
        positions_with_reason(&plain, &report, "CustomRejected"),
        "既定で一括実装の列が運ぶ拒否の位置が仕込みと違う"
    );

    // `batch_failure`: 失敗を**一括判定を上書きした実装の列**で起こす（9.4 の入口）。
    let batch = schema_sample(&SchemaSampleOptions::new(rows).with_batch_failure(true));
    let schema = batch.compiled();
    let report = engine.validate_sheet(batch.document(), batch.sheet(), &schema, &validation);
    assert!(
        !injected_in(&batch, BATCH_CUSTOM_COLUMN).is_empty(),
        "一括実装の列に違反が仕込まれていない"
    );
    assert_eq!(
        injected_in(&batch, BATCH_CUSTOM_COLUMN),
        positions_with_reason(&batch, &report, "CustomFailed"),
        "一括実装の列の失敗が違反として報告されない（隔離できていない）"
    );
    assert_eq!(
        injected_in(&batch, SIMPLE_CUSTOM_COLUMN),
        positions_with_reason(&batch, &report, "CustomRejected"),
        "既定実装のままの列の拒否が違反として報告されない"
    );
    assert_eq!(
        batch.expected_violations(),
        report.total_violations(),
        "失敗が同じ列の後続の判定を巻き込んでいる（隔離できていない）"
    );
}

/// 違反の並びを「行添字・列添字・入れ子の位置・理由の種別」へ畳む（生の値は含めない）。
///
/// これが 9.3 の比較してよい形そのものである（`tests/common/schema.rs` の docs）。
fn violation_signature(
    sample: &SchemaSample,
    report: &SheetReport,
) -> Vec<(usize, usize, String, &'static str)> {
    let index_of = row_index(sample);
    report
        .violations()
        .iter()
        .map(|violation| {
            let row = violation.row().expect("シート全体の検証の違反は行に属する");
            (
                index_of[&row],
                violation.column().index(),
                format!("{:?}", violation.path()),
                reason_kind(violation.reason()),
            )
        })
        .collect()
}

/// 理由の種別が `kind` である違反の位置（行添字・列添字）を昇順で返す。
fn positions_with_reason(
    sample: &SchemaSample,
    report: &SheetReport,
    kind: &str,
) -> Vec<(usize, usize)> {
    let index_of = row_index(sample);
    let mut positions: Vec<(usize, usize)> = report
        .violations()
        .iter()
        .filter(|violation| reason_kind(violation.reason()) == kind)
        .map(|violation| {
            let row = violation.row().expect("シート全体の検証の違反は行に属する");
            (index_of[&row], violation.column().index())
        })
        .collect();
    positions.sort_unstable();
    positions
}

/// 列 `column` で違反を仕込んだセルの位置（行添字の昇順）。
fn injected_in(sample: &SchemaSample, column: usize) -> Vec<(usize, usize)> {
    (0..sample.rows())
        .filter(|row| sample.violation_at(*row, column))
        .map(|row| (row, column))
        .collect()
}

/// 行識別子から行添字への索引（標本の行の並びから作る）。
fn row_index(sample: &SchemaSample) -> HashMap<RowId, usize> {
    sample
        .row_ids()
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect()
}

/// 違反の理由の**種別**（値を持たない安定なトークン）。
///
/// `ViolationReason` の `Debug` は実際の値・参照先シートの識別子・重複する行の識別子を
/// 含み、それらは実行ごとに変わる（`tests/common/schema.rs` の docs）。種別だけを取り出す。
fn reason_kind(reason: &ViolationReason) -> &'static str {
    match reason {
        ViolationReason::TypeMismatch { .. } => "TypeMismatch",
        ViolationReason::OutOfRange { .. } => "OutOfRange",
        ViolationReason::LengthOutOfRange { .. } => "LengthOutOfRange",
        ViolationReason::PatternMismatch { .. } => "PatternMismatch",
        ViolationReason::ChoiceNotAllowed { .. } => "ChoiceNotAllowed",
        ViolationReason::PrecisionExceeded { .. } => "PrecisionExceeded",
        ViolationReason::MissingValue { .. } => "MissingValue",
        ViolationReason::Duplicate { .. } => "Duplicate",
        ViolationReason::BrokenReference { .. } => "BrokenReference",
        ViolationReason::UnusableColumn { .. } => "UnusableColumn",
        ViolationReason::CustomRejected { .. } => "CustomRejected",
        ViolationReason::CustomFailed { .. } => "CustomFailed",
    }
}

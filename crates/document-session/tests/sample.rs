//! テストとベンチが共有する標本の生成器のテスト（タスク 1.4。要件 8.1）。
//!
//! # 何を主張し、何を主張しないか
//!
//! 標本の決定性の主張は**形（行数・列数・列名）と値**についてだけ行う。**行識別子
//! （ULID）とバイト列の同一性は主張しない**: 行識別子は公開の [`IdFactory`] が発行時刻を
//! 含めて発行するため、同じ引数でも 2 回の生成で異なる（`verification.md`「標本が識別子
//! （ULID）を含むなら『同一バイト列』を主張しない」）。
//!
//! # 10 万行の生成に時間の閾値を置かない理由
//!
//! [`large_sample_is_generated_with_the_requested_shape`] は 10 万行 × 30 列を実際に
//! 組み立て、引数どおりの形になることだけを確かめる。**所要時間の閾値はテストに置かない**
//! （計測環境に依存する値をテストの合否にすると、回帰ではなく環境差で落ちる。所要時間の
//! 判定はタスク 5.1 のベンチと `scripts/check-bench-budget.sh` のゲートが担う）。
//! 生成器が「現実的な時間で終わる」ことは、このテストが制限時間内に完了すること自体が示す。
//!
//! [`IdFactory`]: document_format::IdFactory

mod common;

use std::collections::BTreeSet;

use common::{sample, SampleSpec, Scratch};
use document_format::parts::RowsCodec;
use document_format::{CellValue, DocumentFormatApi, EntryName};

/// 10 万行 × 30 列の標本が生成でき、列数と行数が引数どおりである（tasks.md 1.4）。
///
/// 生成が現実的な時間で終わることも同時に確かめる（時間の閾値は置かない。モジュール docs
/// 「10 万行の生成に時間の閾値を置かない理由」）。
#[test]
fn large_sample_is_generated_with_the_requested_shape() {
    let spec = SampleSpec::new(100_000, 30);
    let generated = sample(&spec);

    assert_eq!(generated.row_count(), 100_000, "行数が引数どおりでない");
    assert_eq!(generated.column_count(), 30, "列数が引数どおりでない");
    assert_eq!(generated.sheet().rows().len(), 100_000);
    assert_eq!(generated.sheet().columns(), spec.columns());
    assert!(
        generated
            .sheet()
            .rows()
            .iter()
            .all(|row| row.values().len() == 30),
        "全行の値の個数が列数と一致しない"
    );
}

/// 同じ引数で 2 回生成したとき、形（行数・列数・列名）と各行の値が一致する（tasks.md 1.4）。
///
/// **識別子とバイト列の同一性は主張しない**（モジュール docs 参照）。標本の規模は
/// 決定性の主張に必要な最小限に留める（10 万行 × 30 列を 2 つ同時に保持しない）。
#[test]
fn same_arguments_produce_the_same_shape_and_values() {
    let spec = SampleSpec::new(2_000, 30);
    let first = sample(&spec);
    let second = sample(&spec);

    assert_eq!(first.row_count(), second.row_count());
    assert_eq!(first.column_count(), second.column_count());
    assert_eq!(first.sheet().columns(), second.sheet().columns());
    assert_eq!(
        first.row_values(),
        second.row_values(),
        "同じ引数の 2 回の生成で行の値が一致しない"
    );
}

/// 列名と値の型が現実に寄っており、複数の [`CellValue`] 変種が混ざる（tasks.md 1.4）。
///
/// 30 列の標本は整数・テキスト・10 進数・浮動小数・真偽・入れ子（配列）と、まばらな欠測
/// （`Null`）をすべて含む。単一の変種だけの標本では、下流のテスト（保存の門・往復）が
/// 値の型による分岐を踏めない。
#[test]
fn sample_mixes_value_variants() {
    let spec = SampleSpec::new(4, 30);
    let generated = sample(&spec);

    let mut seen = BTreeSet::new();
    for row in generated.sheet().rows() {
        for value in row.values() {
            seen.insert(variant_name(value));
        }
    }

    for expected in ["Null", "Bool", "Int", "Float", "Decimal", "Text", "Nested"] {
        assert!(
            seen.contains(expected),
            "標本に {expected} 変種が現れない（現れた変種: {seen:?}）"
        );
    }
}

/// 小さな標本（3 行 × 2 列）で、行の値の個数が列数と一致する（tasks.md 1.4）。
///
/// 値の個数が列数と一致しない行は保存の門（[`RowsCodec::encode`]）を通らない。生成器が
/// 穴埋めも切詰めもしないことを、公開の門を実際に通して確かめる。
#[test]
fn small_sample_rows_match_the_column_count_for_the_rows_codec() {
    let spec = SampleSpec::new(3, 2);
    let generated = sample(&spec);
    let sheet = generated.sheet();

    assert_eq!(sheet.columns(), spec.columns());
    assert_eq!(sheet.rows().len(), 3);
    assert!(sheet.rows().iter().all(|row| row.values().len() == 2));

    // 値は生成器の規則（行添字と列添字だけで決まる）どおりである。
    assert_eq!(sheet.rows()[1].values()[0], common::value_at(1, 0));

    let (entry, bytes) = RowsCodec::encode(sheet.id(), sheet.columns(), sheet.rows())
        .expect("標本の行は列数と一致するので保存の門を通る");
    assert_eq!(entry, EntryName::Rows { sheet: sheet.id() });
    assert!(!bytes.is_empty());
}

/// 一時ディレクトリへの書き出しと [`DocumentFormatApi::open`] での読み戻しが往復する
/// （tasks.md 1.4）。
///
/// 生成器が組み立てた文書が、上流の保存と読み込みのフル経路を通っても列名と行の値を
/// 保つことを確かめる。一時ディレクトリは [`Scratch`] が終了時に削除する。
#[test]
fn sample_round_trips_through_a_temporary_file() {
    let scratch = Scratch::new("sample_round_trip");
    let spec = SampleSpec::new(5, 3);
    let generated = sample(&spec);
    let path = scratch.file("sample.jxcel");

    common::api()
        .save(generated.document(), &path)
        .expect("標本は保存できる");
    let outcome = common::api().open(&path).expect("保存した標本は開ける");

    let reopened = &outcome.document.sheets()[0];
    assert_eq!(reopened.columns(), spec.columns());
    assert_eq!(reopened.rows().len(), 5);
    let values: Vec<Vec<CellValue>> = reopened
        .rows()
        .iter()
        .map(|row| row.values().to_vec())
        .collect();
    assert_eq!(values, generated.row_values(), "往復で行の値が変わった");
}

/// 一時ディレクトリが `Drop` で確実に片付く（tasks.md 1.4）。
#[test]
fn scratch_removes_its_directory_on_drop() {
    let path = {
        let scratch = Scratch::new("scratch_cleanup");
        std::fs::write(scratch.file("残る"), b"x").expect("一時ファイルを書ける");
        assert!(scratch.path().is_dir());
        scratch.path().to_path_buf()
    };

    assert!(!path.exists(), "一時ディレクトリが片付いていない");
}

/// セル値の変種名（[`sample_mixes_value_variants`] の観測用）。
fn variant_name(value: &CellValue) -> &'static str {
    match value {
        CellValue::Null => "Null",
        CellValue::Bool(_) => "Bool",
        CellValue::Int(_) => "Int",
        CellValue::Float(_) => "Float",
        CellValue::Decimal(_) => "Decimal",
        CellValue::Text(_) => "Text",
        CellValue::Nested(_) => "Nested",
        CellValue::Attachment(_) => "Attachment",
    }
}

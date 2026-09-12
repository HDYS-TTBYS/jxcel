//! 拡張型の一括経路と失敗の隔離（tasks.md 9.4。要件 11.5, 11.6）。
//!
//! 設計の Testing Strategy が `custom_types.rs` に求める 2 つ — **一括判定を上書きした
//! 実装と既定実装のままの実装が同じ結果を返すこと**と、**片方が `Err` を返してもシート
//! 全体の検証が完走すること** — を、9.1 の 30 列の標本の上で確かめる。あわせて、
//! **拡張型を含む列の一括検証が組込型のみの場合と同一の一括経路を通ること**（要件 11.6）
//! を、標本の実装を観測用に包んで**呼び出しの形**を数えることで示す。
//!
//! # 何が「一括経路」の証拠か
//!
//! 速度ではなく呼び出しの形を証拠にする（tasks.md 5.4 の申し送り）。`validate_sheet` の
//! **1 回の呼び出しの内側**で、拡張型の列は `CustomType::validate_batch` が**列ごとに
//! 1 回**呼ばれ、その 1 回に列の値がまとめて渡る。値ごとの判定（`CustomType::validate`）は
//! 最上位の列では 1 度も呼ばれない。組込型だけのシートも同じ 1 回の入口
//! （`validate_sheet`）を通り、拡張型の列だけが別の経路へ落ちることはない。
//! 一括判定の呼び出し元は `ColumnValidator::Custom` の単一の変種であるため、行ごとの
//! 動的ディスパッチも起きない（structure.md「そのバッチ経路の内側では動的ディスパッチを
//! 使わない」）。
//!
//! # 実装を書き換えずに測る
//!
//! 標本の拡張型の実装（[`BATCH_CUSTOM_ID`] / [`SIMPLE_CUSTOM_ID`]）は台帳へ登録済みで
//! あり、標本自身は変更しない。そこで標本の台帳から [`TypeRegistry::resolve`] で実装を
//! 取り出し、回数を数える [`Observed`] で包んだ**別の台帳**を組み立て、同じ文書を
//! コンパイルし直す。判定は内側の実装へそのまま委ねるため、結果は標本の台帳で
//! コンパイルした場合と一致する（[`the_observation_wrapper_does_not_change_the_report`]）。
//!
//! # 生の値は比較に使えない
//!
//! 標本の値と宣言には実行ごとに発行される識別子（参照先の行の ULID と型定義の識別子）が
//! 現れる。比較に使うのは**違反の位置と理由の種別**だけである（`tests/common/schema.rs`
//! の docs「決定性 — 何が同一で、何が同一でないか」）。

mod common;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::schema::{
    schema_sample, SchemaSample, SchemaSampleOptions, BATCH_CUSTOM_COLUMN, BATCH_CUSTOM_ID,
    SIMPLE_CUSTOM_COLUMN, SIMPLE_CUSTOM_ID, UNIQUE_GROUPS,
};
use document_format::{CellValue, RowId};
use schema_engine::{
    CompiledSchema, CustomType, CustomTypeFailure, CustomTypeId, CustomVerdict, SchemaEngine,
    SchemaEngineApi, SheetReport, TypeRegistry, ValidationOptions, ViolationReason,
};

/// 標本の行数。標本自身の検証（`tests/schema_sample.rs`）と同じ規模に合わせる。
fn sample_rows() -> usize {
    UNIQUE_GROUPS * 2
}

/// 一括判定を上書きした実装と、既定実装のままの実装が**同じ値に同じ判定を返す**
/// （tasks.md 9.4。design.md「Testing Strategy / Integration Tests」の `custom_types.rs`。
/// 要件 11.6）。
///
/// 比較する値は標本の 2 つの拡張型の列が実際に持つ値である。したがって 3 つの判定 —
/// 適合・拒否・判定の失敗（`Err`）— が同じ比較に載る（標本の規則は両実装で同一であり、
/// `with_batch_failure` が失敗を一括実装の列へ仕込む）。
#[test]
fn the_batch_override_and_the_default_implementation_return_the_same_verdicts() {
    let sample = schema_sample(&SchemaSampleOptions::new(sample_rows()).with_batch_failure(true));
    let registry = sample.registry();
    let overridden = registry
        .resolve(BATCH_CUSTOM_ID)
        .expect("標本の一括実装が台帳にある");
    let plain = registry
        .resolve(SIMPLE_CUSTOM_ID)
        .expect("標本の既定実装が台帳にある");

    // 一括実装の列: 適合と、判定の失敗（`?` で始まる値）が載る。
    let undecidable = column_values(&sample, BATCH_CUSTOM_COLUMN);
    let (overridden_result, overridden_verdicts) = batch_of(overridden.as_ref(), &undecidable);
    let (plain_result, plain_verdicts) = batch_of(plain.as_ref(), &undecidable);
    assert_eq!(
        plain_result, overridden_result,
        "上書き実装と既定実装で一括判定の成否が違う"
    );
    assert_eq!(
        plain_verdicts, overridden_verdicts,
        "上書き実装と既定実装で失敗より前の判定が違う"
    );
    assert!(
        overridden_result.is_err(),
        "標本は一括実装の列に判定の失敗を仕込む（比較の対象から失敗が抜けている）"
    );

    // 既定実装の列: 適合と、実装の拒否（形の違う値）が載る。
    let malformed = column_values(&sample, SIMPLE_CUSTOM_COLUMN);
    let (overridden_result, overridden_verdicts) = batch_of(overridden.as_ref(), &malformed);
    let (plain_result, plain_verdicts) = batch_of(plain.as_ref(), &malformed);
    assert_eq!(
        plain_result, overridden_result,
        "上書き実装と既定実装で一括判定の成否が違う"
    );
    assert_eq!(
        plain_verdicts, overridden_verdicts,
        "上書き実装と既定実装で一括判定の結果が違う"
    );
    assert!(
        plain_result.is_ok() && overridden_result.is_ok(),
        "標本の既定実装の列に判定の失敗が混ざっている"
    );
    assert!(
        plain_verdicts
            .iter()
            .any(|(_, verdict)| matches!(verdict, CustomVerdict::Rejected { .. })),
        "拒否の判定が比較に載っていない"
    );
    assert!(
        plain_verdicts
            .iter()
            .any(|(_, verdict)| *verdict == CustomVerdict::Accepted),
        "適合の判定が比較に載っていない"
    );
}

/// 一括判定が `Err` を返しても、シート全体の検証は**完走**し、失敗はその値の違反に
/// 閉じ込められる（tasks.md 9.4。要件 11.5）。
///
/// 標本は `with_batch_failure` により、**一括判定を上書きした実装の列**へ失敗を 2 件以上
/// 仕込む。失敗のたびに次の値から一括判定をやり直すため、2 件目以降も報告される
/// （打ち切りとの区別がこれである）。同時に、もう一方の拡張型の拒否も同じ検証の中で
/// 報告され続ける。
#[test]
fn a_failing_batch_is_confined_to_its_value_and_the_sheet_validation_completes() {
    let sample = schema_sample(&SchemaSampleOptions::new(sample_rows()).with_batch_failure(true));
    let failures = injected_in(&sample, BATCH_CUSTOM_COLUMN);
    assert!(
        failures.len() >= 2,
        "同じ列に複数の失敗が要る（打ち切りでないことを示せない）: {failures:?}"
    );

    let (registry, batch, _) = observing_registry(&sample);
    let schema = compiled_with(&sample, &registry);
    let report = SchemaEngine::new().validate_sheet(
        sample.document(),
        sample.sheet(),
        &schema,
        &ValidationOptions::default(),
    );

    // 完走: 仕込んだ違反がちょうど全部報告される（1 セルにつき 1 件）。
    assert_eq!(
        sample.expected_violations(),
        report.total_violations(),
        "失敗がシート全体の検証を中断させている（完走していない）"
    );
    // 失敗はすべてその値の違反になり、位置は仕込みと一致する。2 件以上あるので、
    // 失敗のたびに次の値から再開していることがこの一致で示される。
    assert_eq!(
        failures,
        positions_with_reason(&sample, &report, "CustomFailed"),
        "失敗した値の位置が仕込みと違う（失敗より後を打ち切っている）"
    );
    // もう一方の拡張型（既定実装）の拒否も同じ検証の中で報告される。
    assert_eq!(
        injected_in(&sample, SIMPLE_CUSTOM_COLUMN),
        positions_with_reason(&sample, &report, "CustomRejected"),
        "既定実装のままの列の拒否が検証から落ちている"
    );

    // 隔離の形: 一括判定は失敗 1 件につき 1 回だけやり直す。したがって呼び出しの回数は
    // 失敗の件数で抑えられ（全行を舐め直す値ごとの経路へ落ちない）、失敗が 2 件以上あれば
    // 必ず 2 回以上になる（1 回で打ち切っていない）。
    assert!(
        batch.batch_calls() >= 2,
        "失敗のあとに一括判定がやり直されていない（打ち切っている）"
    );
    assert!(
        batch.batch_calls() <= failures.len() + 1,
        "一括判定が失敗の件数より多く呼ばれている（値ごとの経路へ落ちている）"
    );
    assert_eq!(
        0,
        batch.one_by_one(),
        "最上位の拡張型の列が値ごとの判定へ落ちている"
    );
}

/// 拡張型を含む列の一括検証は、組込型のみの場合と**同一の一括経路**を通る
/// （tasks.md 9.4。要件 11.6）。
///
/// `validate_sheet` の 1 回の呼び出しの内側で、拡張型の列は `validate_batch` が列ごとに
/// 1 回だけ呼ばれ、その 1 回に**列の値がまとめて**渡る。値ごとの判定は最上位の列では
/// 起きない。一括判定を上書きした実装でも、既定実装のままの実装でも同じ形になる。
///
/// 標本は違反を 1 つも仕込まない（`with_ratio(0.0)`）— 標本の既定の値の規則は、仕込んだ
/// 違反のセルにだけ判定の失敗（`?` で始まる値）を置くため、失敗を混ぜずに「列ごとに
/// 1 回」を数えるにはこれが要る。失敗がある場合の呼び出しの形は
/// [`a_failing_batch_is_confined_to_its_value_and_the_sheet_validation_completes`] が見る。
#[test]
fn a_custom_column_is_judged_in_one_batch_per_column_not_value_by_value() {
    let sample = schema_sample(&SchemaSampleOptions::new(sample_rows()).with_ratio(0.0));
    let (registry, batch, simple) = observing_registry(&sample);
    let schema = compiled_with(&sample, &registry);
    let report = SchemaEngine::new().validate_sheet(
        sample.document(),
        sample.sheet(),
        &schema,
        &ValidationOptions::default(),
    );

    // 適合する値しか無い標本である（この検査が「違反が無いこと」を混ぜない）。
    assert_eq!(
        0,
        report.total_violations(),
        "違反を仕込んでいない標本から違反が出ている"
    );

    // 上書き実装の列: 列ごとに 1 回、値ごとの判定は 0 回（この識別子は最上位の列だけが使う）。
    assert_eq!(
        1,
        batch.batch_calls(),
        "上書き実装の列が列ごとに 1 回の一括判定になっていない"
    );
    assert_eq!(
        0,
        batch.one_by_one(),
        "上書き実装の列が値ごとの判定へ落ちている"
    );
    assert_eq!(
        vec![column_values(&sample, BATCH_CUSTOM_COLUMN).len()],
        batch.batch_sizes(),
        "上書き実装の列の値が 1 回にまとめて渡されていない"
    );

    // 既定実装のままの列（最上位）も同じ形である。入れ子の内側のフィールド
    // （`届け先.郵便番号`）は値ごとに判定される — 一括の継ぎ目は最上位の列だからである
    // （tasks.md 5.4 の裁定）。したがって一括判定の回数は最上位の 1 列だけを数える。
    assert_eq!(
        1,
        simple.batch_calls(),
        "既定実装の列が列ごとに 1 回の一括判定になっていない"
    );
    assert_eq!(
        vec![column_values(&sample, SIMPLE_CUSTOM_COLUMN).len()],
        simple.batch_sizes(),
        "既定実装の列の値が 1 回にまとめて渡されていない"
    );
}

/// 組込型の列と拡張型の列は、同じ 1 回の一括検証の中で同じ報告に併合される
/// （tasks.md 9.4。要件 11.6）。
///
/// 標本の台帳でコンパイルした計画と、包んだ実装を登録した台帳でコンパイルした計画は、
/// 同じ文書・同じ `validate_sheet` の 1 回の呼び出しで、同じ違反を同じ順で報告する。
/// 報告には**組込型の列の違反と拡張型の列の違反の両方**が載る — 拡張型の列が別の経路へ
/// 落ちていれば、この一致は成り立たない。
#[test]
fn the_observation_wrapper_does_not_change_the_report() {
    let sample = schema_sample(&SchemaSampleOptions::new(sample_rows()).with_batch_failure(true));
    let (registry, batch, _) = observing_registry(&sample);
    let observed = compiled_with(&sample, &registry);
    let plain = sample.compiled();

    let options = ValidationOptions::default();
    let engine = SchemaEngine::new();
    let observed_report =
        engine.validate_sheet(sample.document(), sample.sheet(), &observed, &options);
    let plain_report = engine.validate_sheet(sample.document(), sample.sheet(), &plain, &options);

    assert_eq!(
        violation_signature(&sample, &plain_report),
        violation_signature(&sample, &observed_report),
        "包んだ実装の判定が標本の実装と違う"
    );
    assert_eq!(
        sample.expected_violations(),
        observed_report.total_violations(),
        "仕込んだ違反がすべて報告されていない"
    );
    // この検査が空振りでないこと。報告に組込型の列の違反と拡張型の列の違反の両方が載り、
    // 一括判定も実際に走っている。
    let kinds: Vec<&'static str> = observed_report
        .violations()
        .iter()
        .map(|violation| reason_kind(violation.reason()))
        .collect();
    assert!(
        kinds.iter().any(|kind| !kind.starts_with("Custom")),
        "組込型の列の違反が同じ報告に載っていない"
    );
    assert!(
        kinds.iter().any(|kind| kind.starts_with("Custom")),
        "拡張型の列の違反が同じ報告に載っていない"
    );
    assert!(
        batch.batch_calls() > 0,
        "比較した検証で一括判定が走っていない"
    );
}

/// 拡張型の実装を包み、一括判定と値ごとの判定の呼ばれ方を数える。
///
/// 判定は内側の実装へそのまま委ねる（規則は標本のものが唯一の源である。写しを作らない）。
struct Observed {
    id: CustomTypeId,
    inner: Arc<dyn CustomType>,
    observation: Observation,
}

impl Observed {
    /// 実体と、その観測点を組み立てる。
    fn new(id: &str, inner: Arc<dyn CustomType>) -> (Self, Observation) {
        let observation = Observation::default();
        (
            Self {
                id: CustomTypeId::new(id),
                inner,
                observation: observation.clone(),
            },
            observation,
        )
    }
}

impl CustomType for Observed {
    fn id(&self) -> &CustomTypeId {
        &self.id
    }

    fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
        self.observation.one_by_one.fetch_add(1, Ordering::SeqCst);
        self.inner.validate(value)
    }

    fn validate_batch(
        &self,
        values: &[CellValue],
        out: &mut dyn FnMut(usize, CustomVerdict),
    ) -> Result<(), CustomTypeFailure> {
        self.observation.batch_calls.fetch_add(1, Ordering::SeqCst);
        self.observation
            .sizes
            .lock()
            .expect("観測点の記録が壊れていない")
            .push(values.len());
        self.inner.validate_batch(values, out)
    }
}

/// [`Observed`] の観測点（一括判定の回数・値ごとの判定の回数・一括判定へ渡った値の個数）。
#[derive(Clone, Default)]
struct Observation {
    batch_calls: Arc<AtomicUsize>,
    one_by_one: Arc<AtomicUsize>,
    sizes: Arc<Mutex<Vec<usize>>>,
}

impl Observation {
    /// 一括判定が呼ばれた回数。
    fn batch_calls(&self) -> usize {
        self.batch_calls.load(Ordering::SeqCst)
    }

    /// 値ごとの判定が呼ばれた回数。
    fn one_by_one(&self) -> usize {
        self.one_by_one.load(Ordering::SeqCst)
    }

    /// 一括判定へ渡った値の個数の並び（呼び出しの順）。
    fn batch_sizes(&self) -> Vec<usize> {
        self.sizes
            .lock()
            .expect("観測点の記録が壊れていない")
            .clone()
    }
}

/// 標本の 2 つの拡張型を観測用に包んだ台帳を組み立てる。
///
/// 標本の台帳（[`SchemaSample::registry`]）は読み取り専用であるため、そこから実装を
/// 解決して包み、同じ識別子で登録し直す。文書の宣言は識別子で解決するので、同じ列が
/// 同じ規則で判定される。
fn observing_registry(sample: &SchemaSample) -> (TypeRegistry, Observation, Observation) {
    let (batch, batch_observation) = Observed::new(
        BATCH_CUSTOM_ID,
        sample
            .registry()
            .resolve(BATCH_CUSTOM_ID)
            .expect("標本の一括実装が台帳にある"),
    );
    let (simple, simple_observation) = Observed::new(
        SIMPLE_CUSTOM_ID,
        sample
            .registry()
            .resolve(SIMPLE_CUSTOM_ID)
            .expect("標本の既定実装が台帳にある"),
    );

    let mut registry = TypeRegistry::new();
    registry
        .register(Arc::new(batch))
        .expect("標本の識別子は重複しない");
    registry
        .register(Arc::new(simple))
        .expect("標本の識別子は重複しない");
    (registry, batch_observation, simple_observation)
}

/// 与えられた台帳で標本のデータシートの計画を組み立てる。
fn compiled_with(sample: &SchemaSample, registry: &TypeRegistry) -> CompiledSchema {
    let sheet = sample
        .document()
        .sheet_by_id(sample.sheet())
        .expect("標本のシートは文書にある");
    SchemaEngine::new()
        .compile(sheet, registry)
        .expect("標本の宣言はコンパイルできる")
}

/// 標本の列 `column` の値を行の並び順に取り出す。
fn column_values(sample: &SchemaSample, column: usize) -> Vec<CellValue> {
    let sheet = sample
        .document()
        .sheet_by_id(sample.sheet())
        .expect("標本のシートは文書にある");
    sheet
        .rows()
        .iter()
        .map(|row| row.values()[column].clone())
        .collect()
}

/// 1 件用の判定を 1 回呼ばずに、一括判定だけを回して結果と渡された判定を返す。
fn batch_of(
    ty: &dyn CustomType,
    values: &[CellValue],
) -> (Result<(), CustomTypeFailure>, Vec<(usize, CustomVerdict)>) {
    let mut delivered = Vec::new();
    let outcome = ty.validate_batch(values, &mut |index, verdict| {
        delivered.push((index, verdict));
    });
    (outcome, delivered)
}

/// 列 `column` で違反を仕込んだセルの位置（行添字の昇順）。
fn injected_in(sample: &SchemaSample, column: usize) -> Vec<(usize, usize)> {
    (0..sample.rows())
        .filter(|row| sample.violation_at(*row, column))
        .map(|row| (row, column))
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

/// 違反の並びを「行添字・列添字・入れ子の位置・理由の種別」へ畳む（生の値は含めない）。
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

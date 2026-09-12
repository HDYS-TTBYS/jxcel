//! 違反の順序の決定性を、行を跨ぐ経路で検査する（タスク 9.3。要件 5.5）。
//!
//! 一括検証は 2 段である — 第 1 段が行ごとの値の判定（`CellValidator`）、第 2 段が行を
//! **跨ぐ**性質の判定（`UniqueScan` と `ReferenceScan`）。第 2 段は全行を走査し終えてから
//! 違反を返すため、両段の結果をそのまま連結すると同じ行の違反が段を跨いで離れて並ぶ。
//! `SheetValidator` は最後に同じ基準（行の並び順 → 列の添字 → 入れ子の位置）で安定併合して
//! 順序を決める（design.md「System Flows / 一括検証の 2 段構成」）。
//!
//! 本ファイルはその併合が**値の違反と一意制約の違反が同時に出るシート**で成り立つこと、
//! 同じ入力に対して複数回の実行で並びが変わらないこと、そして**プロセスを分けて実行しても
//! 並びが一致する**ことを確かめる（tasks.md 9.3。要件 5.5）。
//!
//! # プロセスを分ける理由
//!
//! 一意制約の判定はハッシュ表（`HashMap`）を使う。ハッシュの反復順は種（seed）に依存し、
//! その種はプロセスごとに変わるため、**同一プロセス内の繰り返しでは反復順の漏れを検出
//! できない**。子プロセスでテストバイナリ自身を再実行し、親と同じ手順で得た並びを突き合わせる
//! （`document-format` の `src/json/determinism.rs` と `tests/atomic_save.rs` が同じ理由で
//! 同じ機構を使う。`current_exe()` + `--exact` + 環境変数の厳密一致で子分岐を選ぶ）。
//!
//! # 比較してよい形（tasks.md 9.1 の裁定）
//!
//! 標本の値には実行ごとに発行される識別子（参照先シートの行の ULID と型定義の識別子）が
//! 現れるため、**生の値を比較してはならない**。同一なのは**違反の位置（行添字・列添字・
//! 入れ子の位置）と理由の種別**であり、本ファイルはその並びだけを比較する
//! （`tests/common/schema.rs` の docs「決定性 — 何が同一で、何が同一でないか」）。
//!
//! # 何を確かめないか — 一意制約の判定そのもの
//!
//! 重複の件数・重複するすべての行の報告（要件 4.7）はタスク 9.1 の標本の検証と
//! `validate::unique` の単体テストが持つ。本ファイルは**並び**だけを見る。

mod common;

use std::collections::HashMap;
use std::process::Command;

use common::schema::{schema_sample, SchemaSample, SchemaSampleOptions, UNIQUE_GROUPS};
use document_format::RowId;
use schema_engine::{
    SchemaEngine, SchemaEngineApi, SheetReport, ValidationOptions, ValuePathSegment,
    ViolationReason,
};

/// 決定性の検査に使う標本の行数（`tests/common/schema.rs` の [`UNIQUE_GROUPS`] 個の値が
/// 2 巡する）。品番の値は 1,000 個を巡回するため大半が 2 回現れ、一意制約の違反が値の
/// 違反より多く出る。
const SAMPLE_ROWS: usize = UNIQUE_GROUPS * 2;

/// 違反として仕込むセルの割合。
///
/// 既定（0.001）より上げるのは、**2 段の結果が混ざって並ぶ**ことを観測可能にするためで
/// ある。既定では 2,000 行 × 30 列で仕込まれる 60 件がすべて行 999 と行 1999 に落ち
/// （行優先の等間隔の選定が行数の半分の歩幅になるため）、一意制約の違反（最初に現れた
/// 行 0..998 に属する）との切り替わりが 1 回しか起きない — 段ごとの連結でも同じ並びに
/// なり得て、併合を検査できない。
const SAMPLE_RATIO: f64 = 0.01;

/// 値の違反と一意制約の違反の**両方**が出る標本（tasks.md 9.3 が用意するシート）。
fn sample() -> SchemaSample {
    schema_sample(
        &SchemaSampleOptions::new(SAMPLE_ROWS)
            .with_ratio(SAMPLE_RATIO)
            .with_unique_collisions(true),
    )
}

/// 標本を 1 回の一括検証にかける（`SheetValidator` の一括経路。要件 10.4）。
fn report(sample: &SchemaSample) -> SheetReport {
    let schema = sample.compiled();
    SchemaEngine::new().validate_sheet(
        sample.document(),
        sample.sheet(),
        &schema,
        &ValidationOptions::default(),
    )
}

/// 違反 1 件を比較するための鍵（行添字・列添字・入れ子の位置・理由の種別）。
///
/// **生の値を含まない**（モジュール docs「比較してよい形」）。位置と種別だけが実行と
/// プロセスを跨いで同一である。
type ViolationKey = (usize, usize, Vec<PathKey>, &'static str);

/// 入れ子の位置の 1 段を比較可能にしたもの。
///
/// `Ord` の導出は `validate` 層の併合の比較（`validate/mod.rs` の `compare_segments`）と
/// 同じ規則を与える — フィールド名どうしは文字列として、添字どうしは数値として比べ、
/// **フィールドを添字より先に置く**（列挙体の宣言順がこの規則をそのまま表す）。
/// この比較は計画の木を引かずに違反だけで順序を決めるための規則である。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum PathKey {
    /// オブジェクトのフィールド名。
    Field(String),
    /// 配列の 0 起点の要素位置。
    Index(usize),
}

/// 違反の並びを、行添字・列添字・入れ子の位置・理由の種別の並びへ畳む。
fn signature(sample: &SchemaSample, report: &SheetReport) -> Vec<ViolationKey> {
    let index_of: HashMap<RowId, usize> = sample
        .row_ids()
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    report
        .violations()
        .iter()
        .map(|violation| {
            let row = violation.row().expect("一括検証の違反は行に属する");
            (
                index_of[&row],
                violation.column().index(),
                violation.path().segments().iter().map(path_key).collect(),
                reason_kind(violation.reason()),
            )
        })
        .collect()
}

/// 入れ子の位置の 1 段を鍵へ写す。
fn path_key(segment: &ValuePathSegment) -> PathKey {
    match segment {
        ValuePathSegment::Field(name) => PathKey::Field(name.to_string()),
        ValuePathSegment::Index(index) => PathKey::Index(*index),
    }
}

/// 違反の並びを、プロセスを跨いで比較できる 1 行 1 件のテキストへ畳む。
///
/// 1 行目は違反の総数である（件数が変われば並びの比較より先に気づける）。理由の種別は
/// 値を持たない安定なトークンであり、位置はタブ区切り、入れ子の位置は `>` 区切りで
/// 1 段ずつ符号化する（フィールドは `f`、添字は `i` を前置する）。
fn encode(sample: &SchemaSample, report: &SheetReport) -> String {
    let mut out = format!("total={}", report.total_violations());
    for key in signature(sample, report) {
        let path: Vec<String> = key
            .2
            .iter()
            .map(|segment| match segment {
                PathKey::Field(name) => format!("f{name}"),
                PathKey::Index(index) => format!("i{index}"),
            })
            .collect();
        out.push('\n');
        out.push_str(&format!(
            "{}\t{}\t{}\t{}",
            key.0,
            key.1,
            path.join(">"),
            key.3
        ));
    }
    out
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

/// 値の違反と一意制約の違反が**1 つの全順序へ安定併合**される（tasks.md 9.3。要件 5.5）。
///
/// 併合の基準は 行の並び順 → 列の添字 → 入れ子の位置 である（design.md「検証結果の
/// 表現」）。第 2 段（一意制約）の違反は全行を走査したあとに得られるため、これを単純に
/// 連結すると段ごとに固まって並ぶ。実際の並びが**鍵の全順序と厳密に一致**すること、そして
/// 2 つの種別が段ごとに固まらず**複数回切り替わる**ことを確かめる。
#[test]
fn the_two_stages_are_merged_into_one_order() {
    let sample = sample();
    let report = report(&sample);

    assert_eq!(
        sample.expected_violations(),
        report.total_violations(),
        "総件数が仕込んだ違反と違う（適合する値が違反を生んでいる）"
    );

    let merged = signature(&sample, &report);
    assert!(merged.len() > 1, "違反が 1 件以下では順序を検査できない");

    let duplicates = merged.iter().filter(|key| key.3 == "Duplicate").count();
    assert_eq!(
        sample.unique_violations(),
        duplicates,
        "一意制約の違反の件数が標本の数え方と違う"
    );
    assert!(
        merged.iter().any(|key| key.3 != "Duplicate"),
        "値の違反が 1 件も無い（併合を跨る経路の検査にならない）"
    );

    // 併合の結果は 行 → 列 → 入れ子の位置 の全順序である。段ごとの連結ならここで崩れる
    // （一意制約の違反は先頭の行に属するため、値の違反より後ろに固まって並ぶ）。
    let mut ordered = merged.clone();
    ordered.sort();
    assert_eq!(
        ordered, merged,
        "併合の並びが 行 → 列 → 入れ子の位置 の全順序になっていない"
    );

    // 段ごとの連結（値の違反の後ろに一意制約の違反が固まる、またはその逆）ではない。
    // 併合された並びでは 2 つの種別が複数回切り替わる。
    let transitions = merged
        .windows(2)
        .filter(|pair| (pair[0].3 == "Duplicate") != (pair[1].3 == "Duplicate"))
        .count();
    assert!(
        transitions >= 2,
        "一意制約の違反と値の違反が段ごとに固まっている（切り替わりが {transitions} 回）"
    );
}

/// 同じ入力に対する複数回の実行で、違反の並びが変わらない（tasks.md 9.3。要件 5.5）。
///
/// 標本を組み立て直した場合（識別子は発行し直される）も並びは一致する。生の値は比較せず、
/// 位置と理由の種別だけを比較する（モジュール docs「比較してよい形」）。
#[test]
fn the_same_input_always_yields_the_same_order() {
    let first = sample();
    let expected = signature(&first, &report(&first));
    assert!(expected.len() > 1, "違反が 1 件以下では順序を検査できない");

    for run in 1..=4 {
        assert_eq!(
            expected,
            signature(&first, &report(&first)),
            "{run} 回目の実行で違反の並びが変わった"
        );
    }

    let rebuilt = sample();
    assert_eq!(
        expected,
        signature(&rebuilt, &report(&rebuilt)),
        "標本を組み立て直すと違反の並びが変わった（値の識別子が順序へ漏れている）"
    );
}

/// プロセスを分けて実行しても違反の並びが一致する（tasks.md 9.3。要件 5.5）。
///
/// 一意制約の判定が使うハッシュの反復順はプロセスごとに変わるため、これが
/// **反復順が結果へ漏れていないこと**の検査である（モジュール docs「プロセスを分ける理由」）。
#[test]
fn the_violation_order_is_identical_across_processes() {
    const CHILD_ENV: &str = "SCHEMA_ENGINE_DETERMINISM_CHILD";
    const CHILD_VALUE: &str = "child";
    const CHILD_TEST: &str = "the_violation_order_is_identical_across_processes";
    const LINE: &str = "VIOLATION\t";

    let sample = sample();
    let expected = encode(&sample, &report(&sample));
    assert!(
        expected.lines().count() > 2,
        "違反が少なすぎて順序を検査できない"
    );

    // 子分岐は**値の厳密一致**で選ぶ。変数の存在だけで分岐すると、同名の環境変数が外部に
    // 定義されている環境では親が子分岐へ入り、比較が空振りして緑になる
    // （`document-format` の `json/determinism.rs` と同じ理由）。
    if std::env::var_os(CHILD_ENV).is_some_and(|value| value == CHILD_VALUE) {
        for line in expected.lines() {
            println!("{LINE}{line}");
        }
        return;
    }

    let child = Command::new(std::env::current_exe().expect("テスト実行ファイルのパス取得"))
        .args(["--exact", CHILD_TEST, "--nocapture"])
        .env(CHILD_ENV, CHILD_VALUE)
        .output()
        .expect("子プロセスの起動");
    assert!(
        child.status.success(),
        "子プロセスが失敗した: {}",
        String::from_utf8_lossy(&child.stderr)
    );

    let stdout = String::from_utf8(child.stdout).expect("子プロセスの出力は UTF-8");
    let reported: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix(LINE))
        .collect();
    assert!(
        !reported.is_empty(),
        "子プロセスが違反の並びを報告しない（--exact のテスト名が古い可能性）"
    );
    assert_eq!(
        expected.lines().collect::<Vec<_>>(),
        reported,
        "プロセスを跨ぐと違反の並びが変わる（ハッシュの反復順が結果へ漏れている）"
    );
}

/// 列指定の再検証（第 2 段を指定した列に閉じる経路）でも、全件検証と同じ並びになる
/// （tasks.md 9.3 の「跨る経路」の突き合わせ。要件 5.5, 10.5）。
///
/// 全件検証の併合結果から一意制約の列だけを取り出した並びと、その列を指定した再検証の
/// 並びが一致することを確かめる。**一意制約の判定を通る経路が 2 つあり、その並びが
/// 一致する**ことが、経路の選択が順序へ影響しないことの証拠である。
#[test]
fn the_column_restricted_path_keeps_the_merged_order() {
    let sample = sample();
    let schema = sample.compiled();
    let engine = SchemaEngine::new();
    let options = ValidationOptions::default();

    let full = signature(
        &sample,
        &engine.validate_sheet(sample.document(), sample.sheet(), &schema, &options),
    );

    let unique = schema.unique_columns();
    assert_eq!(1, unique.len(), "標本の一意制約の列が 1 本でない");
    let expected: Vec<ViolationKey> = full
        .iter()
        .filter(|key| key.1 == unique[0].index())
        .cloned()
        .collect();
    assert!(
        expected.len() > 1,
        "一意制約の列の違反が 1 件以下では順序を検査できない"
    );

    let restricted = signature(
        &sample,
        &engine.validate_columns(sample.document(), sample.sheet(), &schema, unique, &options),
    );
    assert_eq!(expected, restricted, "列指定の再検証で違反の並びが変わった");
}

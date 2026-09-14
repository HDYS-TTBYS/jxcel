//! テストとベンチが共有する標本の生成器そのものの検証（タスク 1.4。要件 1.1, 11.7）。
//!
//! `tests/common/sample.rs` は、テストとベンチが共有する「指定した行数・列数のシートと、
//! それに適合するスキーマ宣言」の生成器である。このファイルはその正しさを 6 つの面から
//! 固定する:
//!
//! 1. **宣言の網羅**（[`the_sample_declares_the_thirty_columns`]）— タスク 1.4 が挙げる
//!    組込の各型・入れ子（オブジェクトと配列）・一意制約・必須が実際に宣言され、どの列も
//!    使用不能にならず、`schema-engine` がコンパイルできること。
//! 2. **列の並びの保証**（[`a_narrower_sample_keeps_the_coverage_of_the_documented_prefix`]）
//!    — 列数を減らすと先頭からの前置になり、先頭 13 列までは網羅が保たれること。
//! 3. **要求した規模**（[`the_hundred_thousand_row_sample_builds_at_the_requested_ratio`]）
//!    — 10 万行 × 30 列が実際に組み立てられ、全件検証が通ること（要件 1.1）。
//! 4. **違反の位置**（[`the_hundred_thousand_row_sample_builds_at_the_requested_ratio`] と
//!    [`the_requested_ratio_decides_the_violation_counts_and_positions`]）— 要求した割合の
//!    違反が、**仕込んだセルにちょうど 1 件ずつ**現れること。割合が `0.0` なら適合する値が
//!    違反を 1 件も生まないこと。
//! 5. **一意制約の違反**（[`the_shared_sample_can_carry_unique_violations`]）— 重複を
//!    混ぜたときの件数と位置が、標本自身の数え方と一致すること。
//! 6. **決定性**（[`the_same_options_yield_the_same_values_and_violations`] と
//!    [`a_different_seed_changes_the_values_but_not_the_violations`]）— 同じ引数の 2 回の
//!    組み立てが**識別子を除いて**一致し、種を変えると**値だけ**が変わること。
//!
//! # 決定性 — 何を比較してよく、何を比較してはいけないか
//!
//! **同じ引数の 2 回の [`sample`] は、バイト列としては一致しない。** `document-format` の
//! 行識別子は発行時刻を含む ULID であり、組み立てのたびに変わる（`schema-engine` の
//! `tests/common/schema.rs` の docs「決定性 — 何が同一で、何が同一でないか」が同じ危険を
//! 記録している）。標本はその識別子を 2 箇所に載せる:
//!
//! - **シート間参照の値** — 参照先シートの行識別子（トップレベルの `ref` 列と、入れ子の
//!   内側の `ref` フィールド）。
//! - **`ref` 列の宣言が運ぶ参照先シートの識別子**（シートの ULID）。
//!
//! したがって比較してよいのは次の 4 つだけであり、このファイルはこの 4 つだけを突き合わせる:
//!
//! - **形**: 列数・行数・列名の並び。
//! - **宣言**: 列の宣言（名前・型・制約・必須・一意・既定値）と入れ子のフィールドの宣言。
//!   ただし `ref` の制約が運ぶ**シートの識別子だけ**は伏せて比較する
//!   （[`masked_declaration`]）。
//! - **値**: セルの値。ただし参照先の行識別子を運ぶ値は**参照先シートの行位置**へ畳んでから
//!   比較する（[`masked_values`]）。添付の識別子は内容から決まる（content-addressed）ため、
//!   伏せる必要が無い。
//! - **違反**: 行添字・列添字・入れ子の位置（[`ValuePath`]）と、理由の**種別**
//!   （[`reason_kind`]）。理由が運ぶ実際の値（違反した値・重複する行の識別子）は比較に
//!   使わない。
//!
//! 逆に、**生の値・生の宣言テキスト・文書のバイト列は比較しない**（必ず食い違う）。

mod common;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::str::FromStr;

use common::sample::{
    sample, Sample, SampleOptions, COVERAGE_COLUMNS, REFERENCE_ROWS, SAMPLE_COLUMNS, SAMPLE_ROWS,
    SAMPLE_SEED, SAMPLE_VIOLATION_RATIO, UNIQUE_GROUPS,
};
use document_format::{CellValue, NestedValue, RowId, SchemaPart, SheetId};
use schema_engine::{
    parse_schema, ColumnDecl, DeclaredKind, Schema, SchemaEngine, SchemaEngineApi, SheetReport,
    TypeDecl, TypeKind, ValidationOptions, ValuePath, ViolationReason,
};

/// 標本の 30 列が、タスク 1.4 が挙げる要素（組込の各型・入れ子・一意制約・必須）を実際に
/// 持ち、宣言が `schema-engine` でコンパイルできる（どの列も使用不能にならない）。
///
/// 小さな行数で見るのは標本の**形**であり、規模は
/// [`the_hundred_thousand_row_sample_builds_at_the_requested_ratio`] が見る。
#[test]
fn the_sample_declares_the_thirty_columns() {
    let engine = SchemaEngine::new();
    let sample = sample(&SampleOptions::new(64, SAMPLE_COLUMNS));
    let schema = sample.compiled();

    assert_eq!(64, sample.rows(), "行数が指定どおりでない");
    assert_eq!(SAMPLE_COLUMNS, sample.column_count(), "列数が 30 でない");
    assert_eq!(SAMPLE_COLUMNS, schema.column_count(), "計画の列数が違う");
    assert!(
        schema.unusable_columns().is_empty(),
        "使用不能な列がある: {:?}",
        schema.unusable_columns()
    );

    // 行データを永続化する呼び出し元へ渡す列の並び（`schema-engine` が供給する）が、宣言と
    // 同じ順である（行オブジェクトのキー順になる並びそのもの）。
    assert_eq!(
        sample.columns(),
        engine.columns(&schema),
        "供給した列の並び順が宣言と違う"
    );

    // 宣言は上流の不透明なペイロード（`SchemaPart`）として取り出せる。名前つき型定義を
    // 使わない（入れ子はすべてインラインで宣言する）ため、宣言には型定義が現れない。
    let part: SchemaPart = sample.schema_part().clone();
    assert!(
        part.type_defs().is_empty(),
        "標本は名前つき型定義を使わない（入れ子はインラインで宣言する）"
    );
    let declaration = declaration(&sample);
    assert_eq!(SAMPLE_COLUMNS, declaration.columns.len(), "宣言の列数が違う");

    // タスク 1.4 が挙げる組込の各型が、それぞれ 1 列以上ある（列名で対応を示す）。
    for (kind, name) in [
        (TypeKind::Text, "品番"),        // 一意制約かつ必須の文字列
        (TypeKind::Int, "数量"),         // 範囲つきの整数（必須）
        (TypeKind::Float, "予備率"),     // 範囲つきの小数
        (TypeKind::Decimal, "単価"),     // 桁と範囲つきの 10 進数
        (TypeKind::Bool, "検査済み"),    // 真偽
        (TypeKind::Date, "発注日"),      // 範囲つきの日付
        (TypeKind::DateTime, "確定日時"), // オフセットを要求する日時
        (TypeKind::Enum, "状態"),        // 選択肢
        (TypeKind::Ref, "仕入先"),       // シート間参照（必須）
        (TypeKind::Attachment, "添付"),  // 添付参照
        (TypeKind::Object, "届け先"),    // 入れ子（フィールドの集合）
        (TypeKind::Array, "明細"),       // 入れ子（要素の並び）
        (TypeKind::Any, "機械可読値"),   // 任意（必須）
    ] {
        let column = column_named(&declaration, name);
        assert_eq!(kind, declared_kind(column), "列 {name} の型が違う");
    }

    // 一意制約はちょうど 1 列（品番）である。必須の列は 5 列である。
    let unique: Vec<&str> = declaration
        .columns
        .iter()
        .filter(|column| column.unique)
        .map(|column| column.name.as_ref())
        .collect();
    assert_eq!(vec!["品番"], unique, "一意制約の列が品番 1 列でない");
    let required: Vec<&str> = declaration
        .columns
        .iter()
        .filter(|column| column.required)
        .map(|column| column.name.as_ref())
        .collect();
    assert_eq!(
        vec!["品番", "数量", "仕入先", "届け先", "機械可読値"],
        required,
        "必須の列が違う"
    );

    // 入れ子（オブジェクト）: フィールドの宣言を順に持ち、必須のフィールドと、入れ子の
    // 内側の参照（`ref` の制約が参照先シートを持つ）を含む。
    let destination = column_named(&declaration, "届け先");
    let fields = match &destination.ty {
        TypeDecl::Kind { constraints, .. } => &constraints.fields,
        other => panic!("届け先が種別の宣言でない: {other:?}"),
    };
    assert_eq!(
        vec!["郵便番号", "都道府県", "住所", "建物", "最寄り倉庫"],
        fields
            .iter()
            .map(|field| field.name.as_ref())
            .collect::<Vec<&str>>(),
        "届け先のフィールドが違う"
    );
    assert!(
        fields.iter().any(|field| field.required),
        "届け先に必須のフィールドが無い"
    );
    assert!(
        matches!(&fields[4].ty, TypeDecl::Kind { constraints, .. } if constraints.sheet.is_some()),
        "入れ子の内側に参照が無い"
    );

    // 入れ子（配列）: 要素の型の宣言を持ち、要素はインラインのオブジェクトである。
    for (name, expected) in [
        ("明細", vec!["商品コード", "数量", "単価"]),
        ("改訂履歴", vec!["改訂日", "版", "理由"]),
    ] {
        let column = column_named(&declaration, name);
        let items = match &column.ty {
            TypeDecl::Kind { constraints, .. } => constraints
                .items
                .as_deref()
                .expect("配列の要素の型が宣言されていない"),
            other => panic!("{name} が種別の宣言でない: {other:?}"),
        };
        let inner = match items {
            TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Object),
                constraints,
            } => &constraints.fields,
            other => panic!("{name} の要素がオブジェクトでない: {other:?}"),
        };
        assert_eq!(
            expected,
            inner
                .iter()
                .map(|field| field.name.as_ref())
                .collect::<Vec<&str>>(),
            "{name} の要素のフィールドが違う"
        );
    }

    // 参照先シートは同じ文書にあり、標本の行を持つ（`ref` 列の適合する値が実在する）。
    let reference = sample
        .document()
        .sheet_by_id(sample.reference_sheet())
        .expect("参照先シートが文書に無い");
    assert_eq!(
        REFERENCE_ROWS,
        reference.rows().len(),
        "参照先シートの行数が違う"
    );
    assert_eq!(
        sample.row_ids().len(),
        sample.rows(),
        "行識別子が行数と一致しない"
    );
}

/// 列数を減らすと標本は**先頭からの前置**になり、先頭 [`COVERAGE_COLUMNS`] 列までは網羅
/// （組込の各型・入れ子・一意制約・必須）が保たれる（タスク 1.4 の「現実に寄せた列構成」の
/// 並びの保証）。
///
/// 並びの保証は「先頭から取る」ことそのものであるため、さらに狭い標本では先頭の列だけが
/// 残る（1 列なら一意制約かつ必須の品番だけになる）。
#[test]
fn a_narrower_sample_keeps_the_coverage_of_the_documented_prefix() {
    let engine = SchemaEngine::new();
    let narrow = sample(&SampleOptions::new(8, COVERAGE_COLUMNS));
    let schema = narrow.compiled();
    assert_eq!(
        COVERAGE_COLUMNS,
        narrow.column_count(),
        "前置の列数が違う"
    );
    assert!(
        schema.unusable_columns().is_empty(),
        "前置に使用不能な列がある"
    );

    let declaration = declaration(&narrow);
    let kinds: HashSet<TypeKind> = declaration
        .columns
        .iter()
        .filter_map(known_kind)
        .collect();
    for kind in TypeKind::ALL {
        // 標本は拡張型を使わない（登録を要する標本は群 9 の関心である）。
        if kind == TypeKind::Custom {
            continue;
        }
        assert!(
            kinds.contains(&kind),
            "前置 {COVERAGE_COLUMNS} 列に {kind:?} が無い"
        );
    }
    assert!(
        declaration.columns.iter().any(|column| column.unique),
        "前置に一意制約の列が無い"
    );
    assert!(
        declaration.columns.iter().any(|column| column.required),
        "前置に必須の列が無い"
    );

    let smallest = sample(&SampleOptions::new(8, 1));
    assert_eq!(
        vec!["品番".to_owned()],
        smallest.columns(),
        "1 列の標本が前置の先頭（品番）でない"
    );
    assert_eq!(
        1,
        engine.columns(&smallest.compiled()).len(),
        "1 列の標本の計画が 1 列でない"
    );
}

/// 10 万行 × 30 列の標本が実際に組み立てられ、コンパイルと全件検証を通り、**指定した割合の
/// 違反が違反として仕込んだセルにちょうど 1 件ずつ**現れる（タスク 1.4。要件 1.1）。
///
/// この一致は「適合する値は違反を 1 件も生まない」ことと「違反する値はちょうど 1 件を生む」
/// ことの両方を同時に示す — 30 列のどれか 1 つでも規則を外せば、件数か位置のどちらかがずれる。
#[test]
fn the_hundred_thousand_row_sample_builds_at_the_requested_ratio() {
    let engine = SchemaEngine::new();
    let sample = sample(&SampleOptions::default());
    let schema = sample.compiled();

    assert_eq!(SAMPLE_ROWS, sample.rows(), "行数が 10 万でない");
    assert_eq!(SAMPLE_COLUMNS, sample.column_count(), "列数が 30 でない");
    assert!(
        schema.unusable_columns().is_empty(),
        "使用不能な列がある: {:?}",
        schema.unusable_columns()
    );

    // 仕込んだ違反の件数は「セル数 × 割合（切り捨て）」である（3,000,000 セルの 0.1%）。
    let cells = sample.rows() * sample.column_count();
    assert_eq!(
        (cells as f64 * SAMPLE_VIOLATION_RATIO) as usize,
        sample.injected_violations(),
        "仕込んだ違反の件数が割合と一致しない"
    );
    assert_eq!(
        3_000,
        sample.injected_violations(),
        "10 万行 × 30 列の 0.1% が 3,000 件でない"
    );

    let report = engine.validate_sheet(
        sample.document(),
        sample.sheet(),
        &schema,
        &ValidationOptions::default(),
    );
    assert_eq!(
        sample.expected_violations(),
        report.total_violations(),
        "違反の総件数が仕込んだ件数と違う（適合する値の違反か、違反の重複・取りこぼし）"
    );
    assert!(!report.is_truncated(), "違反が上限で切れている");

    // 違反の位置は仕込んだセルそのものである（1 セルにつきちょうど 1 件）。
    let index = row_index(&sample);
    assert_eq!(
        injected_positions(&sample),
        reported_positions(&index, &report, None),
        "違反の位置が仕込んだセルと一致しない"
    );

    // 違反を持つ行の数は、違反を仕込んだ行の数と一致する（要件 5.4 の区別）。
    let injected_rows: BTreeSet<usize> = injected_positions(&sample)
        .into_iter()
        .map(|(row, _)| row)
        .collect();
    assert_eq!(
        injected_rows.len(),
        report.invalid_rows().len(),
        "違反を持つ行の数が違反を仕込んだ行の数と違う"
    );

    // 理由の種別は、標本が仕込む種類と正確に一致する（一意制約の重複は既定では混ぜない）。
    let kinds: HashSet<&'static str> = report
        .violations()
        .iter()
        .map(|violation| reason_kind(violation.reason()))
        .collect();
    let expected: HashSet<&'static str> = [
        "TypeMismatch",
        "OutOfRange",
        "LengthOutOfRange",
        "PatternMismatch",
        "ChoiceNotAllowed",
        "PrecisionExceeded",
        "MissingValue",
        "BrokenReference",
    ]
    .into_iter()
    .collect();
    assert_eq!(expected, kinds, "違反の理由の種別が仕込んだ種類と違う");

    // 入れ子の列の違反は**入れ子の位置**を運ぶ（要件 3.4。群 2 の違反の索引がこの形を使う）。
    let mut postal_code = ValuePath::default();
    postal_code.push_field("郵便番号");
    let mut line_quantity = ValuePath::default();
    line_quantity.push_index(0);
    line_quantity.push_field("数量");
    let mut revision_version = ValuePath::default();
    revision_version.push_index(0);
    revision_version.push_field("版");
    for (name, expected_path) in [
        ("届け先", postal_code),
        ("明細", line_quantity),
        ("改訂履歴", revision_version),
    ] {
        let column = column_index(&sample, name);
        let reported: Vec<usize> = report
            .violations()
            .iter()
            .filter(|violation| violation.column().index() == column)
            .map(|violation| {
                let row = violation.row().expect("シート全体の検証の違反は行に属する");
                assert_eq!(
                    expected_path,
                    *violation.path(),
                    "列 {name} の違反が入れ子の位置を運んでいない"
                );
                index[&row]
            })
            .collect();
        assert_eq!(
            injected_positions_in(&sample, column).len(),
            reported.len(),
            "列 {name} の違反の件数が仕込んだ件数と違う"
        );
    }
}

/// 同じ標本が**値の違反と一意制約の違反の両方**を出せる（群 2 の違反の索引と群 9 の検証が
/// 跨る経路。タスク 1.4 の「違反になる値を任意の割合で混ぜられる」）。
///
/// 一意制約の違反は [`SampleOptions::with_unique_collisions`] で混ぜる（既定では混ぜない —
/// 重複の違反は**値ごとに 1 件**であり、仕込んだセルと 1 対 1 にならないためである）。
/// 重複の位置は「重複する値が最初に現れた行」であり、標本が同じ算術から数える
/// （[`Sample::duplicate_first_rows`]）。
#[test]
fn the_shared_sample_can_carry_unique_violations() {
    let engine = SchemaEngine::new();
    let options = SampleOptions::new(UNIQUE_GROUPS * 2, SAMPLE_COLUMNS).with_unique_collisions(true);
    let sample = sample(&options);
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

    let index = row_index(&sample);
    let duplicates: Vec<usize> = report
        .violations()
        .iter()
        .filter_map(|violation| match violation.reason() {
            ViolationReason::Duplicate { .. } => {
                let row = violation.row().expect("シート全体の検証の違反は行に属する");
                Some(index[&row])
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        sample.duplicate_first_rows(),
        duplicates,
        "重複の違反の位置が標本の数え方と違う"
    );

    // 重複の違反は重複するすべての行を運ぶ（要件 4.7）。
    let rows = report
        .violations()
        .iter()
        .find_map(|violation| match violation.reason() {
            ViolationReason::Duplicate { rows, .. } => Some(rows),
            _ => None,
        })
        .expect("重複の違反がある");
    assert!(rows.len() >= 2, "重複する行がすべて載っていない");

    // 値の違反も同じ標本に同居する（両方の経路が 1 つの標本で踏める）。
    assert!(
        report
            .violations()
            .iter()
            .any(|violation| !matches!(violation.reason(), ViolationReason::Duplicate { .. })),
        "値の違反が 1 件も無い"
    );
}

/// 違反の割合を変えると、**仕込む件数と位置が要求した割合どおりに**変わる（タスク 1.4 の
/// 「違反になる値を任意の割合で混ぜられる」）。
///
/// 割合が `0.0` のときは違反を 1 件も仕込まない — 適合する値が違反を生まないこと
/// （＝件数が要求した割合ちょうどであること）は、この端で最も鋭く出る: 適合する値の規則が
/// 1 つでも制約を外していれば、総件数が 0 にならない。
#[test]
fn the_requested_ratio_decides_the_violation_counts_and_positions() {
    let engine = SchemaEngine::new();
    let validation = ValidationOptions::default();

    // 割合 0.0: 適合する値だけになり、違反は 1 件も出ない。
    let clean = sample(&SampleOptions::new(400, SAMPLE_COLUMNS).with_ratio(0.0));
    assert_eq!(0, clean.injected_violations(), "違反を仕込んでしまっている");
    let report = engine.validate_sheet(
        clean.document(),
        clean.sheet(),
        &clean.compiled(),
        &validation,
    );
    assert_eq!(
        0,
        report.total_violations(),
        "適合する値が違反を生んでいる（割合 0.0 の標本が汚れている）"
    );
    assert!(report.invalid_rows().is_empty(), "違反を持つ行がある");

    // 割合 0.05: 件数はセル数 × 割合（切り捨て）であり、位置は仕込んだセルそのものである。
    for (rows, ratio) in [(400, 0.05), (200, 0.25), (1_000, 0.001)] {
        let sample = sample(&SampleOptions::new(rows, SAMPLE_COLUMNS).with_ratio(ratio));
        let cells = sample.rows() * sample.column_count();
        let expected = (cells as f64 * ratio) as usize;
        assert_eq!(
            expected,
            sample.injected_violations(),
            "{rows} 行の {ratio} で仕込む件数が違う"
        );
        assert!(expected > 0, "{rows} 行の {ratio} で違反が 1 件も無い");

        let report = engine.validate_sheet(
            sample.document(),
            sample.sheet(),
            &sample.compiled(),
            &validation,
        );
        let index = row_index(&sample);
        assert_eq!(
            injected_positions(&sample),
            reported_positions(&index, &report, None),
            "{rows} 行の {ratio} で違反の位置が仕込んだセルと一致しない"
        );
    }
}

/// 同じ引数の 2 回の組み立ては、**識別子を除いて**一致する（タスク 1.4 の「同じ引数なら
/// 常に同じ内容になる」そのものである）。
///
/// 比較するのは形・宣言（参照先シートの識別子を伏せる）・値（参照先の行識別子を行位置へ
/// 畳む）・違反（位置と理由の種別）の 4 つだけである。生の値と文書のバイト列は比較しない
/// （モジュール docs「決定性 — 何を比較してよく、何を比較してはいけないか」）。
#[test]
fn the_same_options_yield_the_same_values_and_violations() {
    let engine = SchemaEngine::new();
    let validation = ValidationOptions::default();
    for options in [
        SampleOptions::new(2_000, SAMPLE_COLUMNS),
        SampleOptions::new(UNIQUE_GROUPS * 2, SAMPLE_COLUMNS).with_unique_collisions(true),
    ] {
        let first = sample(&options);
        let second = sample(&options);

        // 形（列数・行数・列の並び）は同じ引数で常に同じである。
        assert_eq!(first.rows(), second.rows(), "行数が一致しない");
        assert_eq!(first.columns(), second.columns(), "列の並びが一致しない");

        // 宣言は、参照先シートの識別子だけを伏せれば一致する。
        assert_eq!(
            masked_declaration(&first),
            masked_declaration(&second),
            "同じ引数の 2 回の組み立てで宣言が変わった"
        );

        // 値は、参照先の行識別子を行位置へ畳めば一致する。
        assert_eq!(
            masked_values(&first),
            masked_values(&second),
            "同じ引数の 2 回の組み立てで値が変わった"
        );

        // 違反の集合は**位置と理由の種別**で一致する（生の値は比較しない）。
        assert!(
            first.expected_violations() > 0,
            "違反が 1 件も無く決定性を検査できない"
        );
        assert_eq!(
            first.expected_violations(),
            second.expected_violations(),
            "違反の件数が一致しない"
        );
        let (left, right) = (first.compiled(), second.compiled());
        assert_eq!(
            left.column_count(),
            right.column_count(),
            "計画の列数が一致しない"
        );
        let left_report = engine.validate_sheet(
            first.document(),
            first.sheet(),
            &left,
            &validation,
        );
        let right_report = engine.validate_sheet(
            second.document(),
            second.sheet(),
            &right,
            &validation,
        );
        assert_eq!(
            violation_signature(&first, &left_report),
            violation_signature(&second, &right_report),
            "同じ引数の 2 回の組み立てで違反の位置と理由の種別が変わった"
        );
    }
}

/// 種を変えると**値だけ**が変わり、違反の位置は変わらない（タスク 1.4 の「同じ引数なら常に
/// 同じ内容になる」の裏取り）。
///
/// 前の検査だけでは「常に同じ」ことは示せても、生成器が引数を見ていることは示せない —
/// 種を無視する生成器でも通ってしまう。ここでは同じ他の引数で種だけを変え、値が変わること
/// （種が結線されていること）と、違反の位置・理由の種別が変わらないこと（位置は添字の
/// 算術だけで決まること）を同時に示す。
#[test]
fn a_different_seed_changes_the_values_but_not_the_violations() {
    let engine = SchemaEngine::new();
    let validation = ValidationOptions::default();
    let base = SampleOptions::new(2_000, SAMPLE_COLUMNS);
    let first = sample(&base.with_seed(SAMPLE_SEED));
    let second = sample(&base.with_seed(SAMPLE_SEED + 1));

    // 既定の種を明示した標本は、`new` が作る標本と一致する（既定が結線されている）。
    assert_eq!(
        masked_values(&first),
        masked_values(&sample(&base)),
        "既定の種が結線されていない"
    );

    // 種を変えると値が変わる（生成器が種を見ている）。
    assert_ne!(
        masked_values(&first),
        masked_values(&second),
        "種を変えても値が同じ（種が無視されている）"
    );

    // 一方、違反の位置は種に依らない（位置を選ぶのは添字の算術だけである）。
    assert_eq!(
        injected_positions(&first),
        injected_positions(&second),
        "種で違反の位置が変わった"
    );
    let (left, right) = (first.compiled(), second.compiled());
    let left_report = engine.validate_sheet(first.document(), first.sheet(), &left, &validation);
    let right_report = engine.validate_sheet(second.document(), second.sheet(), &right, &validation);
    assert_eq!(
        violation_signature(&first, &left_report),
        violation_signature(&second, &right_report),
        "種で違反の位置や理由の種別が変わった"
    );
    assert_eq!(
        masked_declaration(&first),
        masked_declaration(&second),
        "種で宣言が変わった（宣言は種に依らない）"
    );
}

/// 標本の宣言を上流のルート宣言として読み出す（`schema-engine` の公開面だけを使う）。
fn declaration(sample: &Sample) -> Schema {
    parse_schema(sample.schema_part().root().as_str()).expect("標本のルート宣言は解析できる")
}

/// 名前で列の宣言を引く。
fn column_named<'a>(declaration: &'a Schema, name: &str) -> &'a ColumnDecl {
    declaration
        .columns
        .iter()
        .find(|column| column.name.as_ref() == name)
        .unwrap_or_else(|| panic!("列 {name} が宣言に無い"))
}

/// 標本の列の添字を名前で引く。
fn column_index(sample: &Sample, name: &str) -> usize {
    sample
        .columns()
        .iter()
        .position(|column| column == name)
        .unwrap_or_else(|| panic!("列 {name} が標本に無い"))
}

/// 種別による宣言の種別（既知の `kind` だけを対象にする）。
fn declared_kind(column: &ColumnDecl) -> TypeKind {
    known_kind(column).unwrap_or_else(|| panic!("列 {} が既知の種別でない", column.name))
}

/// 列の宣言が既知の種別ならその種別（名前つき型定義への参照は持たない）。
fn known_kind(column: &ColumnDecl) -> Option<TypeKind> {
    match &column.ty {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(kind),
            ..
        } => Some(*kind),
        _ => None,
    }
}

/// 伏せた参照先シートの識別子（比較用の固定値。ULID の nil）。
const MASKED_SHEET_ID: &str = "00000000000000000000000000";

/// 参照先シートの識別子を伏せた宣言（比較用）。
///
/// `ref` の制約が運ぶシートの識別子だけが実行ごとに変わる（発行時刻を含む ULID である）。
/// それ以外は同じ引数で常に同じである（モジュール docs）。
fn masked_declaration(sample: &Sample) -> Schema {
    let mut schema = declaration(sample);
    for column in &mut schema.columns {
        mask_sheet_ids(&mut column.ty);
    }
    schema
}

/// 型の宣言の内側（入れ子のフィールドと配列の要素）まで、参照先シートの識別子を伏せる。
fn mask_sheet_ids(ty: &mut TypeDecl) {
    let TypeDecl::Kind { constraints, .. } = ty else {
        return;
    };
    if constraints.sheet.is_some() {
        constraints.sheet =
            Some(SheetId::from_str(MASKED_SHEET_ID).expect("伏せる識別子は ULID である"));
    }
    for field in &mut constraints.fields {
        mask_sheet_ids(&mut field.ty);
    }
    if let Some(items) = constraints.items.as_deref_mut() {
        mask_sheet_ids(items);
    }
}

/// 標本の全行の値を、識別子を伏せた比較用の形で返す（決定性の比較用）。
fn masked_values(sample: &Sample) -> Vec<Vec<CellValue>> {
    sample
        .row_values()
        .into_iter()
        .map(|row| row.into_iter().map(|value| masked(sample, &value)).collect())
        .collect()
}

/// セルの値を、識別子を伏せた比較用の形へ写す。
///
/// 参照先シートの行識別子として解釈できるテキストは、**参照先シートの行位置**へ畳む
/// （意味が同じなら、識別子の値が違っても同じ形になる）。入れ子の内側は再帰する。
fn masked(sample: &Sample, value: &CellValue) -> CellValue {
    match value {
        CellValue::Text(text) => match sample.reference_position(text) {
            Some(position) => CellValue::Int(position as i64),
            None => value.clone(),
        },
        CellValue::Nested(NestedValue::Object(entries)) => {
            CellValue::Nested(NestedValue::Object(
                entries
                    .iter()
                    .map(|(name, inner)| (name.clone(), masked(sample, inner)))
                    .collect(),
            ))
        }
        CellValue::Nested(NestedValue::Array(items)) => CellValue::Nested(NestedValue::Array(
            items.iter().map(|item| masked(sample, item)).collect(),
        )),
        other => other.clone(),
    }
}

/// 違反を仕込んだセルの位置（行添字・列添字）を昇順で返す。
fn injected_positions(sample: &Sample) -> Vec<(usize, usize)> {
    (0..sample.rows())
        .flat_map(|row| (0..sample.column_count()).map(move |column| (row, column)))
        .filter(|(row, column)| sample.violation_at(*row, *column))
        .collect()
}

/// 列 `column` で違反を仕込んだセルの位置（行添字・列添字）。全体は行優先の昇順である。
fn injected_positions_in(sample: &Sample, column: usize) -> Vec<(usize, usize)> {
    injected_positions(sample)
        .into_iter()
        .filter(|(_, stored)| *stored == column)
        .collect()
}

/// 検証が報告した違反の位置（行添字・列添字）を昇順で返す。`kind` を与えるとその種別だけ。
fn reported_positions(
    index: &HashMap<RowId, usize>,
    report: &SheetReport,
    kind: Option<&str>,
) -> Vec<(usize, usize)> {
    let mut positions: Vec<(usize, usize)> = report
        .violations()
        .iter()
        .filter(|violation| kind.is_none_or(|wanted| reason_kind(violation.reason()) == wanted))
        .map(|violation| {
            let row = violation.row().expect("シート全体の検証の違反は行に属する");
            (index[&row], violation.column().index())
        })
        .collect();
    positions.sort_unstable();
    positions
}

/// 違反の並びを「行添字・列添字・入れ子の位置・理由の種別」へ畳む（生の値は含めない）。
fn violation_signature(
    sample: &Sample,
    report: &SheetReport,
) -> Vec<(usize, usize, ValuePath, &'static str)> {
    let index = row_index(sample);
    report
        .violations()
        .iter()
        .map(|violation| {
            let row = violation.row().expect("シート全体の検証の違反は行に属する");
            (
                index[&row],
                violation.column().index(),
                violation.path().clone(),
                reason_kind(violation.reason()),
            )
        })
        .collect()
}

/// 行識別子から行添字への索引（標本の行の並びから作る）。
fn row_index(sample: &Sample) -> HashMap<RowId, usize> {
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
/// 含み、それらは実行ごとに変わる（モジュール docs）。種別だけを取り出す。
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
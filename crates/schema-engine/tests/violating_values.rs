//! 違反する値の上流の往復（design.md「Testing Strategy / Integration Tests」の最後の項目。
//! tasks.md 8.2。要件 6.5, 6.6）。
//!
//! 編集経路で数値の列に打ち込まれた**文字列**は、強制できないため文字列のセル値のまま
//! 保持される（[`EditVerdict::AcceptedWithViolations`]。要件 6.1）。本ファイルは、その
//! 値が上流の `document-format` の保存・再読込を経ても入力どおりに保たれ（要件 6.6）、
//! 保存が妨げられず（要件 6.5）、再読込の後も同じ違反として報告されることを確かめる。
//!
//! あわせて、**違反を記録する格納先をドキュメントに一切作っていない**ことを往復の結果で
//! 示す。違反は保持されず常に値から導出される（design.md「Write Layer」の「違反する値を
//! どこに置くか」）ため、ドキュメントのパート集合は違反の有無で変わらない。
//!
//! 保存・再読込は `document-format` の公開面（[`DocumentFormatApi::save`] /
//! [`DocumentFormatApi::open`]）を通す。上流の内部経路には触れない。検証は `schema_engine`
//! の根の再輸出（公開面）だけを使う。

use std::fs;
use std::path::PathBuf;

use document_format::parts::DocumentParts;
use document_format::{
    CellValue, Document, DocumentFormat, DocumentFormatApi, SchemaPart, SheetId,
};
use schema_engine::{
    CompiledSchema, EditVerdict, Expected, SchemaEngine, SchemaEngineApi, TypeRegistry,
    ValidationOptions, ViolationReason, WriteOrigin, WriteVerdict,
};

/// ルートスキーマの標本（不透明ペイロードとして上流へ渡す）。
///
/// `数量` は 0〜10 の整数である。列名は行オブジェクトのキーと同じ並びになる。
const ROOT: &str = concat!(
    r#"{"columns":["#,
    r#"{"name":"品番","type":{"kind":"text","maxLength":8},"required":true},"#,
    r#"{"name":"数量","type":{"kind":"int","min":0,"max":10}}"#,
    r#"]}"#,
);

/// 整数のセル値。
fn int(value: i64) -> CellValue {
    CellValue::Int(value)
}

/// 文字列のセル値。
fn text(value: &str) -> CellValue {
    CellValue::Text(value.to_owned())
}

/// ルートスキーマを載せたシートを持つ文書（行はまだ無い）。
fn document() -> (Document, SheetId) {
    let mut doc = Document::new();
    let sheet = doc.add_sheet("発注");
    let envelope = format!(r#"{{"root":{ROOT},"types":[]}}"#);
    doc.set_root_schema(
        sheet,
        SchemaPart::parse(&envelope).expect("標本のエンベロープは妥当"),
    )
    .expect("標本のシートが文書にある");
    (doc, sheet)
}

/// 標本の計画を組み立てる（上流のシートの宣言から列添字で引ける検証器へ落とす）。
fn compiled(doc: &Document, sheet: SheetId) -> CompiledSchema {
    let engine = SchemaEngine::new();
    engine
        .compile(
            doc.sheet_by_id(sheet).expect("標本のシートが文書にある"),
            &TypeRegistry::new(),
        )
        .expect("標本の宣言はコンパイルできる")
}

/// このテスト専用の作業ディレクトリを作る（テストごとに一意。終了時に消す）。
fn scratch_directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "schema-engine-violating-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("作業ディレクトリを作れる");
    path
}

/// パート名をエントリ名の昇順で列挙する（`DocumentParts` の不変条件そのもの）。
fn part_names(parts: &DocumentParts) -> Vec<String> {
    parts.iter().map(|part| part.name.to_string()).collect()
}

/// 数値の列に打ち込まれた文字列が、文字列のセル値として保存・復元される（要件 6.6）。
///
/// 編集経路の値を上流の行データへ書き、`save` → `open` を経てから、再読込後の値と違反が
/// 保存前と一致することを確かめる。違反する値を含んでも保存は妨げられない（要件 6.5）。
#[test]
fn a_string_typed_into_a_numeric_column_survives_the_upstream_round_trip() {
    let engine = SchemaEngine::new();
    let registry = TypeRegistry::new();
    let (mut doc, sheet) = document();
    let schema = compiled(&doc, sheet);
    doc.set_sheet_columns(sheet, engine.columns(&schema).to_vec())
        .expect("標本のシートが文書にある");

    // 編集経路で「数量」に打ち込まれた文字列。整数として解釈できないため強制されず、
    // 文字列のセル値のまま**保持される**（要件 6.1）。
    let typed = vec![text("A"), text("abc")];
    let values = match engine.validate_write(WriteOrigin::Edit, &schema, typed.clone()) {
        WriteVerdict::Edit(EditVerdict::AcceptedWithViolations { values, .. }) => values,
        other => panic!("編集経路が違反する文字列を保持できる形で返さなかった: {other:?}"),
    };
    assert_eq!(
        typed, values,
        "編集経路が打ち込まれた値を入力どおりに返さなかった"
    );

    let row = doc.add_row(sheet).expect("標本のシートが文書にある");
    doc.set_row_values(sheet, row, values)
        .expect("標本の行がシートにある");

    let before = engine.validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
    assert_eq!(1, before.total_violations(), "保存前の違反件数が違う");
    assert_eq!(
        ViolationReason::TypeMismatch {
            expected: Expected::Kind("int".into()),
            actual: text("abc"),
        },
        *before.violations()[0].reason(),
        "保存前の違反の理由が違う"
    );

    // 保存（要件 6.5）。違反する値を含む文書でも保存は妨げられない。
    let directory = scratch_directory();
    let path = directory.join("violating.jxcel");
    let format = DocumentFormat::new();
    format
        .save(&doc, &path)
        .expect("違反する値を含む文書も保存できる");

    // 再読込（要件 6.6）。値は入力どおり（文字列のセル値）に戻る。
    let restored = format
        .open(&path)
        .expect("保存した文書は読み戻せる")
        .document;
    let restored_sheet = restored
        .sheet_by_id(sheet)
        .expect("標本のシートが読み戻せる");
    let restored_row = restored_sheet.rows()[0].id();
    assert_eq!(row, restored_row, "行の識別子が再読込で変わった");
    assert_eq!(
        vec![text("A"), text("abc")],
        restored_sheet.rows()[0].values(),
        "再読込で値が入力どおりに保たれていない"
    );
    assert_eq!(
        ROOT,
        restored_sheet.root_schema().root().as_str(),
        "宣言が再読込で変わった"
    );

    // 再読込後も、同じ位置・同じ理由の同じ違反として報告される（要件 6.6）。
    let restored_schema = engine
        .compile(restored_sheet, &registry)
        .expect("読み戻した宣言もコンパイルできる");
    let after = engine.validate_sheet(
        &restored,
        sheet,
        &restored_schema,
        &ValidationOptions::default(),
    );
    assert_eq!(before, after, "再読込後の違反が保存前と一致しない");

    let _ = fs::remove_dir_all(&directory);
}

/// 違反を記録する格納先をドキュメントに一切作っていない（要件 6.5, 6.6）。
///
/// 3 つの観測で示す。第一に、違反する値を含むドキュメントのパート集合は**閉じた許可リストの
/// 標準の 4 件**そのものであり、違反のためのエントリが存在しない。第二に、保存と再読込を
/// 経てもパート集合は 1 件も変わらない（往復が格納先を足さない）。第三に、**値の側で違反を
/// 取り除いてもパート集合は変わらない** — 違反は値から導出されるのであり、どこにも
/// 格納されていない（design.md「Write Layer」の「違反は保持されず、常に導出される」）。
#[test]
fn the_document_has_no_place_to_record_violations() {
    let engine = SchemaEngine::new();
    let registry = TypeRegistry::new();
    let (mut doc, sheet) = document();
    let schema = compiled(&doc, sheet);
    doc.set_sheet_columns(sheet, engine.columns(&schema).to_vec())
        .expect("標本のシートが文書にある");

    let row = doc.add_row(sheet).expect("標本のシートが文書にある");
    doc.set_row_values(sheet, row, vec![text("A"), text("abc")])
        .expect("標本の行がシートにある");
    assert_eq!(
        1,
        engine
            .validate_sheet(&doc, sheet, &schema, &ValidationOptions::default())
            .total_violations(),
        "標本が違反を持っていない"
    );

    let format = DocumentFormat::new();
    let violating = part_names(&format.to_parts(&doc).expect("標本はパートへ落とせる"));
    assert_eq!(
        vec![
            "document.json".to_owned(),
            "manifest.json".to_owned(),
            format!("schemas/{sheet}.json"),
            format!("sheets/{sheet}.jsonl"),
        ],
        violating,
        "違反する値の保存のために標準でないエントリが作られた"
    );

    // 往復してもパート集合は増えない（要件 6.5, 6.6）。
    let directory = scratch_directory();
    let path = directory.join("no-store.jxcel");
    format
        .save(&doc, &path)
        .expect("違反する値を含む文書も保存できる");
    let mut restored = format
        .open(&path)
        .expect("保存した文書は読み戻せる")
        .document;
    assert_eq!(
        violating,
        part_names(
            &format
                .to_parts(&restored)
                .expect("読み戻した標本はパートへ落とせる")
        ),
        "保存と再読込でドキュメントに格納先が足された"
    );

    // 値の側で違反を取り除く。ドキュメントの構造は 1 件も変わらない。
    restored
        .set_row_values(sheet, row, vec![text("A"), int(7)])
        .expect("標本の行がシートにある");
    let restored_schema = engine
        .compile(
            restored
                .sheet_by_id(sheet)
                .expect("標本のシートが読み戻せる"),
            &registry,
        )
        .expect("読み戻した宣言もコンパイルできる");
    assert_eq!(
        0,
        engine
            .validate_sheet(
                &restored,
                sheet,
                &restored_schema,
                &ValidationOptions::default()
            )
            .total_violations(),
        "違反を取り除けていない"
    );
    assert_eq!(
        violating,
        part_names(&format.to_parts(&restored).expect("標本はパートへ落とせる")),
        "違反の有無でドキュメントのパート集合が変わる（違反の格納先が存在する）"
    );

    let _ = fs::remove_dir_all(&directory);
}

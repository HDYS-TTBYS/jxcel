//! 宣言テキストの正準出力（design.md「File Structure Plan」の
//! `tests/declaration_codec.rs`「宣言テキストの往復と決定性」。tasks.md 3.3。要件 1.4）。
//!
//! このファイルは統合テストであり、クレートの公開面だけを使う。確かめるのは 3 つである:
//!
//! 1. **正準形**: 列とフィールドのキーが `name` → `type` → `required` → `unique` →
//!    `default` → `description` の順に固定され、余分な空白を含まない
//!    （design.md「スキーマ宣言の文法」）。設計のルート例・型定義例がそのまま期待値である。
//! 2. **決定性と往復**: 同一内容の宣言が常に同一のバイト列になり（要件 1.4）、
//!    宣言 → テキスト → 宣言で内容が一致する。
//! 3. **上流の保持経路を生き延びること**: 正準テキストを `document-format` の不透明
//!    ペイロードとして文書に載せ、**実際にファイルへ保存して読み戻す**。上流は
//!    「このペイロードを逐語のバイト列として保持する」ため、正準形の責任は本クレートに
//!    ある（design.md「スキーマ宣言の文法」。要件 1.4）。
//!
//! 保存・再読込は `document-format` の公開面（[`DocumentFormatApi::save`] /
//! [`DocumentFormatApi::open`]）を通す。上流の内部経路（`SchemaCodec` など）には触れない。

use document_format::{CellValue, Document, DocumentFormat, DocumentFormatApi, SchemaPart};
use schema_engine::declaration::codec::{
    parse_schema, parse_type_definition, schema_to_text, type_definition_to_text,
};
use schema_engine::declaration::{
    ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl,
};
use schema_engine::types::datetime::OffsetPolicy;
use schema_engine::types::decimal::DecimalDigits;
use schema_engine::types::TypeKind;
use std::fs;
use std::path::PathBuf;

/// 設計例が使うシート識別子（正準 ULID の 26 文字）。
const SHEET: &str = "01K4ANRRG004HMASW9NF6YY091";
/// 設計例が使う型定義識別子（正準 ULID の 26 文字）。
const TYPE_DEF: &str = "01K4ANRRG004HMASW9NF6YY092";

/// 種別による指定の型を組み立てる。
fn kind(kind: TypeKind, constraints: Constraints) -> TypeDecl {
    TypeDecl::Kind {
        kind: DeclaredKind::Known(kind),
        constraints,
    }
}

/// 既定値も説明も持たない列を組み立てる。
fn column(name: &str, ty: TypeDecl) -> ColumnDecl {
    ColumnDecl {
        name: name.into(),
        ty,
        required: false,
        unique: false,
        default: None,
        description: None,
    }
}

/// 正準テキスト（`Err` は宣言が読み戻せない場合だけである）。
fn text_of(schema: &Schema) -> String {
    schema_to_text(schema).expect("標本の宣言は読み戻せる")
}

/// 設計のルート例（design.md「スキーマ宣言の文法」）をそのまま書いた宣言。
///
/// 期待値と突き合わせることで、キーの順序・余分な空白の不在・`false` の省略が
/// 1 つの文字列で固定される。
const ROOT_EXAMPLE: &str = r#"{
    "columns": [
        { "name": "品番", "type": { "kind": "text", "maxLength": 32 }, "required": true, "unique": true },
        { "name": "数量", "type": { "kind": "int", "min": 0 }, "default": 0 },
        { "name": "単価", "type": { "kind": "decimal", "precision": 12, "scale": 2 } },
        { "name": "納品日", "type": { "kind": "date" } },
        { "name": "仕入先", "type": { "kind": "ref", "sheet": "01K4ANRRG004HMASW9NF6YY091" } },
        { "name": "属性", "type": { "$ref": "01K4ANRRG004HMASW9NF6YY092" } },
        { "name": "備考", "type": { "kind": "any" } }
    ]
}"#;

/// 設計の型定義例（design.md「スキーマ宣言の文法」）。
const TYPE_DEFINITION_EXAMPLE: &str = r#"{
    "kind": "object",
    "fields": [ { "name": "色", "type": { "kind": "enum", "choices": ["赤", "青"] }, "required": true } ]
}"#;

/// 全 14 組込種別と、未知の種別・入れ子・配列・参照を 1 つの宣言に詰めた標本。
///
/// 決定性と往復の標本であり、設計の型カタログ表（design.md「組込型カタログと
/// `CellValue` への写像」）の各行を 1 回以上通る。
fn sample_schema() -> Schema {
    let nested = kind(
        TypeKind::Object,
        Constraints {
            fields: vec![
                FieldDecl {
                    name: "色".into(),
                    ty: kind(
                        TypeKind::Enum,
                        Constraints {
                            choices: vec!["赤".into(), "青".into()],
                            ..Constraints::default()
                        },
                    ),
                    required: true,
                    default: Some(CellValue::Text("赤".into())),
                    description: None,
                },
                FieldDecl {
                    name: "寸法".into(),
                    ty: kind(
                        TypeKind::Array,
                        Constraints {
                            items: Some(Box::new(kind(TypeKind::Int, Constraints::default()))),
                            min_items: Some(1),
                            max_items: Some(3),
                            ..Constraints::default()
                        },
                    ),
                    required: false,
                    default: None,
                    description: Some("3 辺".into()),
                },
            ],
            ..Constraints::default()
        },
    );
    Schema {
        columns: vec![
            ColumnDecl {
                name: "整数".into(),
                ty: kind(
                    TypeKind::Int,
                    Constraints {
                        min: Some(CellValue::Int(-5)),
                        max: Some(CellValue::Int(5)),
                        ..Constraints::default()
                    },
                ),
                required: true,
                unique: true,
                default: Some(CellValue::Int(0)),
                description: Some("範囲つきの整数".into()),
            },
            ColumnDecl {
                name: "小数".into(),
                ty: kind(
                    TypeKind::Float,
                    Constraints {
                        min: Some(CellValue::Float(0.0)),
                        max: Some(CellValue::Float(1.0)),
                        ..Constraints::default()
                    },
                ),
                required: false,
                unique: false,
                default: None,
                description: None,
            },
            ColumnDecl {
                name: "単価".into(),
                ty: kind(
                    TypeKind::Decimal,
                    Constraints {
                        digits: Some(DecimalDigits::new(12, 2).expect("12 >= 1 かつ 2 <= 12")),
                        min: Some(CellValue::Decimal("-100.00".into())),
                        max: Some(CellValue::Decimal("100.00".into())),
                        ..Constraints::default()
                    },
                ),
                required: false,
                unique: false,
                default: Some(CellValue::Decimal("0.00".into())),
                description: None,
            },
            ColumnDecl {
                name: "文字列".into(),
                ty: kind(
                    TypeKind::Text,
                    Constraints {
                        min_length: Some(1),
                        max_length: Some(64),
                        pattern: Some(r"^[A-Z0-9-]+$".into()),
                        ..Constraints::default()
                    },
                ),
                required: false,
                unique: false,
                // 10 進数と読めてしまう `Text`。wire 形では脱出口
                // （`{"$t":"text","v":"5"}`）が要る（design.md「既定値はセル値と同一の
                // wire 形」。往復で `Decimal` に化けないことを確かめる標本）。
                default: Some(CellValue::Text("5".into())),
                // 引用符・制御文字・非 ASCII を含む説明（書き出しの脱出を往復で確かめる）。
                description: Some("改行と\"引用符\"を含む説明\n2 行目".into()),
            },
            column("真偽", kind(TypeKind::Bool, Constraints::default())),
            ColumnDecl {
                name: "納品日".into(),
                ty: kind(
                    TypeKind::Date,
                    Constraints {
                        min: Some(CellValue::Text("2026-01-01".into())),
                        max: Some(CellValue::Text("2026-12-31".into())),
                        ..Constraints::default()
                    },
                ),
                required: false,
                unique: false,
                default: None,
                description: None,
            },
            ColumnDecl {
                name: "時刻".into(),
                ty: kind(
                    TypeKind::DateTime,
                    Constraints {
                        offset: Some(OffsetPolicy::Required),
                        min: Some(CellValue::Text("2026-01-01T00:00:00Z".into())),
                        max: Some(CellValue::Text("2026-12-31T23:59:59+09:00".into())),
                        ..Constraints::default()
                    },
                ),
                required: false,
                unique: false,
                default: None,
                description: None,
            },
            ColumnDecl {
                name: "現地時刻".into(),
                ty: kind(
                    TypeKind::DateTime,
                    Constraints {
                        offset: Some(OffsetPolicy::Forbidden),
                        ..Constraints::default()
                    },
                ),
                required: false,
                unique: false,
                default: None,
                description: None,
            },
            ColumnDecl {
                name: "区分".into(),
                ty: kind(
                    TypeKind::Enum,
                    Constraints {
                        choices: vec!["赤".into(), "青".into()],
                        ..Constraints::default()
                    },
                ),
                required: false,
                unique: false,
                default: None,
                description: None,
            },
            ColumnDecl {
                name: "仕入先".into(),
                ty: kind(
                    TypeKind::Ref,
                    Constraints {
                        sheet: Some(SHEET.parse().expect("正準 ULID")),
                        ..Constraints::default()
                    },
                ),
                required: false,
                unique: false,
                default: None,
                description: None,
            },
            ColumnDecl {
                name: "添付".into(),
                ty: kind(TypeKind::Attachment, Constraints::default()),
                required: false,
                unique: false,
                default: None,
                description: None,
            },
            ColumnDecl {
                name: "属性".into(),
                ty: nested,
                required: false,
                unique: false,
                default: None,
                description: None,
            },
            ColumnDecl {
                name: "明細".into(),
                ty: kind(
                    TypeKind::Array,
                    Constraints {
                        items: Some(Box::new(kind(
                            TypeKind::Object,
                            Constraints {
                                fields: vec![FieldDecl {
                                    name: "数量".into(),
                                    ty: kind(TypeKind::Int, Constraints::default()),
                                    required: true,
                                    default: None,
                                    description: None,
                                }],
                                ..Constraints::default()
                            },
                        ))),
                        min_items: Some(1),
                        max_items: Some(100),
                        ..Constraints::default()
                    },
                ),
                required: false,
                unique: false,
                default: None,
                description: None,
            },
            ColumnDecl {
                name: "任意".into(),
                ty: kind(TypeKind::Any, Constraints::default()),
                required: false,
                unique: false,
                // 値なしの既定値（design.md の型カタログ表: `Null` は「値なし」を表す）。
                default: Some(CellValue::Null),
                description: None,
            },
            ColumnDecl {
                name: "郵便番号".into(),
                ty: kind(
                    TypeKind::Custom,
                    Constraints {
                        custom_type: Some("postal-code".into()),
                        ..Constraints::default()
                    },
                ),
                required: false,
                unique: false,
                default: None,
                description: None,
            },
            // 未知の種別は拒否せずそのまま運ぶ（要件 11.7。正準形でも種類が保たれる）。
            column(
                "座標",
                TypeDecl::Kind {
                    kind: DeclaredKind::Unknown("geo-point".into()),
                    constraints: Constraints::default(),
                },
            ),
            ColumnDecl {
                name: "属性参照".into(),
                ty: TypeDecl::Ref(TYPE_DEF.parse().expect("正準 ULID")),
                required: false,
                unique: false,
                default: None,
                description: None,
            },
        ],
    }
}

/// 標本の宣言に現れる型（入れ子の内側も含む）を列挙する。
fn sample_types() -> Vec<TypeDecl> {
    let mut types = vec![TypeDecl::Ref(TYPE_DEF.parse().expect("正準 ULID"))];
    for column in sample_schema().columns {
        let mut stack = vec![column.ty];
        while let Some(ty) = stack.pop() {
            if let TypeDecl::Kind { constraints, .. } = &ty {
                stack.extend(constraints.items.iter().map(|items| items.as_ref().clone()));
                stack.extend(constraints.fields.iter().map(|field| field.ty.clone()));
            }
            types.push(ty);
        }
    }
    types
}

/// 列とフィールドのキーが宣言順に固定され、余分な空白を含まない（design.md
/// 「スキーマ宣言の文法」。tasks.md 3.3。要件 1.4）。
///
/// 期待値は設計のルート例そのものであり、キーの順序（`name` → `type` → `required` →
/// `unique` → `default` → `description`）、`false` を書かないこと、`type` の中の
/// パラメータの位置がここで 1 つのバイト列として固定される。
#[test]
fn the_canonical_root_matches_the_design_example() {
    let schema = parse_schema(ROOT_EXAMPLE).expect("設計例は妥当な宣言");
    assert_eq!(
        concat!(
            r#"{"columns":[{"name":"品番","type":{"kind":"text","maxLength":32},"required":true,"unique":true},"#,
            r#"{"name":"数量","type":{"kind":"int","min":0},"default":0},"#,
            r#"{"name":"単価","type":{"kind":"decimal","precision":12,"scale":2}},"#,
            r#"{"name":"納品日","type":{"kind":"date"}},"#,
            r#"{"name":"仕入先","type":{"kind":"ref","sheet":"01K4ANRRG004HMASW9NF6YY091"}},"#,
            r#"{"name":"属性","type":{"$ref":"01K4ANRRG004HMASW9NF6YY092"}},"#,
            r#"{"name":"備考","type":{"kind":"any"}}]}"#,
        ),
        text_of(&schema),
        "設計例の正準形が違う"
    );
}

/// 型定義の `definition` ペイロードも同じ規律で書き出す（入れ子のフィールドは
/// `name` → `type` → `required` → `default` → `description`。tasks.md 3.1 の申し送り）。
#[test]
fn the_canonical_type_definition_matches_the_design_example() {
    let ty = parse_type_definition(TYPE_DEFINITION_EXAMPLE).expect("設計例は妥当な型定義");
    assert_eq!(
        r#"{"kind":"object","fields":[{"name":"色","type":{"kind":"enum","choices":["赤","青"]},"required":true}]}"#,
        type_definition_to_text(&ty).expect("標本の型定義は読み戻せる"),
        "設計例の正準形が違う"
    );
}

/// 同一内容の宣言は、書かれた空白とキーの順序に依らず常に同一のバイト列になる
/// （要件 1.4）。
///
/// 2 つの綴り（余分な空白を含む・キーの順序が逆・`required` / `unique` を `false` と
/// 明示する）が同じ宣言へ解析されることを先に示し、そのうえで正準形が一致することを
/// 示す。片方だけを書く実装ならここで落ちる。
#[test]
fn the_same_declaration_always_produces_the_same_bytes() {
    let spaced = r#"{
        "columns": [
            {
                "name": "c",
                "type": { "kind": "text", "maxLength": 8 },
                "required": true,
                "unique": false,
                "default": "x",
                "description": "説明"
            }
        ]
    }"#;
    let reordered = r#"{"columns":[{"description":"説明","default":"x","unique":false,"type":{"maxLength":8,"kind":"text"},"required":true,"name":"c"}]}"#;

    let first = parse_schema(spaced).expect("空白つきも妥当");
    let second = parse_schema(reordered).expect("順序違いも妥当");
    assert_eq!(first, second, "同じ内容が別の宣言へ解析されている");

    assert_eq!(
        text_of(&first),
        text_of(&second),
        "同一内容でバイト列が違う"
    );
    // 期待する正準形（`false` は書かず、キーは宣言順、余分な空白なし）。
    assert_eq!(
        r#"{"columns":[{"name":"c","type":{"kind":"text","maxLength":8},"required":true,"default":"x","description":"説明"}]}"#,
        text_of(&first)
    );

    // 書き出しは冪等である（正準形を読み戻して書き直しても同じバイト列）。
    let round = parse_schema(&text_of(&first)).expect("正準形は読み戻せる");
    assert_eq!(
        text_of(&first),
        text_of(&round),
        "書き直しでバイト列が変わった"
    );

    // 空の宣言（要件 1.8）も決定的である。
    assert_eq!(
        r#"{"columns":[]}"#,
        text_of(&Schema::default()),
        "列 0 本の宣言"
    );
}

/// 宣言 → テキスト → 宣言の往復で内容が一致する（要件 1.4）。
///
/// 全 14 組込種別・未知の種別・入れ子・配列・参照・制約を 1 つの標本で通り、列の
/// 宣言そのもの（名前・必須・一意・既定値・説明）と、型と制約のすべてが保たれることを
/// 示す。同じ標本の各型は単体の `definition` ペイロードとしても往復させる。
#[test]
fn a_declaration_round_trips_through_text() {
    let schema = sample_schema();
    let written = text_of(&schema);
    assert_eq!(
        schema,
        parse_schema(&written).expect("正準形は読み戻せる"),
        "列の宣言が往復で変わった"
    );

    for ty in sample_types() {
        let written = type_definition_to_text(&ty).expect("標本の型は読み戻せる");
        assert_eq!(
            ty,
            parse_type_definition(&written).expect("正準形は読み戻せる"),
            "型が往復で変わった: {written}"
        );
    }

    // 読み戻せない宣言は誤りとして拒否する（黙って壊れたテキストを返さない）。
    // 空の列名は要件 1.6 の拒否対象であり、正準出力の側でも同じ判断になる。
    let broken = Schema {
        columns: vec![column("", kind(TypeKind::Any, Constraints::default()))],
    };
    assert!(
        schema_to_text(&broken).is_err(),
        "読み戻せない宣言をテキストにしてはならない"
    );
}

/// 既定値はセル値と同一の wire 形で書く（design.md「既定値はセル値と同一の wire 形」）。
///
/// 特に、**10 進数と読めてしまう `Text`** は脱出口を通らなければ往復で `Decimal` に化ける。
/// 文字列をそのまま並べるだけの書き出しではこの行で落ちる。
#[test]
fn defaults_use_the_cell_value_wire_form() {
    let text_defaults = Schema {
        columns: vec![
            ColumnDecl {
                name: "見かけは数値".into(),
                ty: kind(TypeKind::Text, Constraints::default()),
                required: false,
                unique: false,
                default: Some(CellValue::Text("5".into())),
                description: None,
            },
            ColumnDecl {
                name: "小数".into(),
                ty: kind(TypeKind::Decimal, Constraints::default()),
                required: false,
                unique: false,
                default: Some(CellValue::Decimal("5".into())),
                description: None,
            },
            ColumnDecl {
                name: "値なし".into(),
                ty: kind(TypeKind::Any, Constraints::default()),
                required: false,
                unique: false,
                default: Some(CellValue::Null),
                description: None,
            },
        ],
    };
    let written = text_of(&text_defaults);
    assert_eq!(
        concat!(
            r#"{"columns":[{"name":"見かけは数値","type":{"kind":"text"},"default":{"$t":"text","v":"5"}},"#,
            r#"{"name":"小数","type":{"kind":"decimal"},"default":"5"},"#,
            r#"{"name":"値なし","type":{"kind":"any"},"default":null}]}"#,
        ),
        written,
        "既定値の wire 形が違う"
    );
    assert_eq!(
        text_defaults,
        parse_schema(&written).expect("正準形は読み戻せる"),
        "既定値が往復で変わった（`Text` が `Decimal` に化けていないか）"
    );
}

/// 正準テキストを上流のスキーマ保持の経路に載せ、実際にファイルへ保存して読み戻しても
/// バイト列が変わらない（要件 1.4。design.md「スキーマ宣言の文法」の「`document-format`
/// はこのペイロードを逐語のバイト列として保持する」）。
///
/// 観測するのは `save` → `open` を経た文書のルートスキーマと型定義の
/// **生のバイト列**である。上流が再シリアライズすればここで落ちる。
#[test]
fn the_canonical_payload_survives_the_upstream_holding_path() {
    let schema = sample_schema();
    let root = text_of(&schema);
    let definition = type_definition_to_text(&kind(TypeKind::Object, Constraints::default()))
        .expect("空のオブジェクトは読み戻せる");
    let envelope =
        format!(r#"{{"root":{root},"types":[{{"id":"{TYPE_DEF}","definition":{definition}}}]}}"#);

    let mut document = Document::new();
    let sheet = document.add_sheet("標本");
    document
        .set_sheet_columns(sheet, Vec::new())
        .expect("標本のシートは実在する");
    document
        .set_root_schema(
            sheet,
            SchemaPart::parse(&envelope).expect("標本のエンベロープは妥当"),
        )
        .expect("標本のシートは実在する");

    let directory = scratch_directory();
    let path = directory.join("canonical.jxcel");
    let api = DocumentFormat::new();
    api.save(&document, &path).expect("標本は保存できる");
    let restored = api.open(&path).expect("保存した標本は読み戻せる").document;
    let _ = fs::remove_dir_all(&directory);

    let restored_schema = restored.sheets()[0].root_schema();
    assert_eq!(
        root,
        restored_schema.root().as_str(),
        "ルートスキーマのバイト列が保存・再読込で変わった"
    );
    let restored_defs = restored_schema.type_defs();
    assert_eq!(1, restored_defs.len(), "型定義の件数が変わった");
    assert_eq!(
        definition,
        restored_defs[0].definition().as_str(),
        "型定義のバイト列が保存・再読込で変わった"
    );
    assert_eq!(
        schema,
        parse_schema(restored_schema.root().as_str()).expect("読み戻したルートは解析できる"),
        "保存・再読込の後の宣言が変わった"
    );
}

/// このテスト専用の作業ディレクトリを作る（テストごとに一意。終了時に消す）。
fn scratch_directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "schema-engine-codec-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("作業ディレクトリを作れる");
    path
}

//! 宣言テキストの解析（design.md「コンポーネントとファイルの対応」の
//! `DeclarationCodec`。tasks.md 3.2。要件 1.5, 1.6, 1.7, 4.8）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは `declaration` 層の内側にあり、[`crate::types`]
//! （型カタログ・桁・日時・書式）と [`crate::error`]（宣言を拒否する理由）、上流の
//! `document-format`（セル値の wire 形と識別子）だけを参照する。**`registry` 以降は
//! 参照しない** — 型定義の解決（`$ref` の先）と拡張型の登録は本モジュールの知らない
//! ところにあり、それが本モジュールの責務の境界を決めている（下記「責務の境界」）。
//!
//! # 所有するもの: 不透明ペイロードの文法（design.md「スキーマ宣言の文法」）
//!
//! `document-format` はスキーマを不透明なバイト列として保持する（design.md
//! 「This Spec Owns」）。その**中身**の文法は本機能が所有し、本モジュールが宣言テキスト
//! （ルートスキーマの `root` ペイロードと、各型定義の `definition` ペイロード）を
//! [`Schema`] / [`TypeDecl`](super::TypeDecl) へ読み下す唯一の場所である。正準出力は
//! `declaration::codec` の出力側（tasks.md 3.3）が受け持つ。
//!
//! 文法は design.md の 3 つの規則に従う:
//!
//! - **型は常にオブジェクト**であり、`kind` を持つか `$ref` を持つかの**いずれか一方**
//!   である。両方を持つ型も、どちらも持たない型も誤りとして拒否する（`declaration`
//!   層の型 [`TypeDecl`](super::TypeDecl) がこの二者択一そのものを表す）。
//! - **型定義への参照は `{"$ref": "<TypeDefId>"}` だけ**で書く。上流が参照を構造として
//!   追跡できる唯一の形であるため、`$ref` に他のキーを併記することも拒否する。
//! - **既定値はセル値と同一の wire 形**で読む（`document-format` の `value` が定める形。
//!   `{"$t":"text","v":"…"}` の脱出口を含む）。宣言のための第 2 の値表現を作らない。
//!
//! # 責務の境界 — その場で判定できる既定値の適合（tasks.md 3.2）
//!
//! 要件 4.8 は「宣言された既定値がその列の型または制約に適合しないとき」に宣言を拒否
//! することを求める。しかし既定値の適合判定には**型が解決されていること**が要る場合が
//! あり、本モジュールは型定義の集合（上流のエンベロープ）も拡張型の登録（`registry`
//! 層）も見られない。design.md「Compile Layer / SchemaCompiler」の分担に従い:
//!
//! - **組込種別が直接書かれた列**（[`TypeDecl::Kind`](super::TypeDecl::Kind) の
//!   [`DeclaredKind::Known`](super::DeclaredKind::Known)）の既定値は**ここで検査**する。
//!   型の変種と、その場で判定できる値の制約（範囲・桁・長さ・書式・選択肢・暦）が対象
//!   である。
//! - **識別子参照の先**（[`TypeDecl::Ref`](super::TypeDecl::Ref)）と**拡張型**
//!   （`custom`）の既定値は、型が解決されるまで判定できないため `compile` 層
//!   （タスク 4.4）が受け持つ。本モジュールはこれらを**素通しする**。
//!
//! 素通しの対象を「判定しない」と明示することで、二重判定も判定漏れも起こさない。
//!
//! # 未知の種別・未知のキー・意味を持たないパラメータ（tasks.md 3.1 の裁定）
//!
//! - **未知の `kind` トークンは拒否しない。** 将来の版が足した種別を古い版が読んだ
//!   場合、スキーマ全体を破棄してはならない（要件 11.7）。[`DeclaredKind::Unknown`](
//!   super::DeclaredKind::Unknown) としてそのまま保持し、**その列だけを使用不能**に
//!   する判断は `compile` 層（タスク 4.4）が行う。ただし型オブジェクトが持てるのは
//!   `kind` だけであり、それ以外のキーは文法が定義しない以上ここで拒否する。
//! - **型オブジェクト・列オブジェクト・フィールドオブジェクトの中で文法が定義しない
//!   キーは拒否する**（要件 1.7）。黙って読み飛ばすと「宣言 → テキスト → 宣言」の
//!   往復で内容が落ちる。
//! - **その種別で意味を持たないパラメータの位置も拒否する**（例 `bool` に `choices`）。
//!   `Constraints` は種別ごとの意味を持たない位置も設定できる平坦な構造であるため
//!   （tasks.md 3.1）、黙って無視すると往復で内容差になる。
//!
//! # 列名とフィールド名（要件 1.5, 1.6）
//!
//! 空の列名は [`SchemaError::EmptyColumnName`]（位置つき）、重複する列名は
//! [`SchemaError::DuplicateColumnName`]（列名と全出現位置つき）で拒否する。列名の中身を
//! 上流は解釈しないため（design.md「Existing Architecture Analysis」）、妥当性は本機能が
//! 決める。入れ子のフィールド名も空は拒否し、1 つの `fields` 配列の中での重複も拒否する
//! （専用の変種が無いため [`SchemaError::MalformedDeclaration`] に位置と理由を載せる。
//! 重複は重複した 2 つ目の出現を指す）。
//!
//! # 空のルートスキーマ（要件 1.8）
//!
//! 上流の `SchemaPart::empty()` は新規シートのルートを **`null`** として持つ。本モジュール
//! は `null` と、`columns` を持たない／空のオブジェクトを、**列 0 本のシート**として受理
//! する（エラーとしない）。
//!
//! # 汎用 JSON 値型を内部表現にしない（design.md「Allowed Dependencies」）
//!
//! 解析は `serde_json::value::RawValue`（原文のソーステキストを捕捉する公認機構）で行い、
//! セル値は `document-format` の `from_json_bytes`（整数リテラルの範囲検査と `$t` 脱出口の
//! 復号を含む唯一の経路）へ渡す。**`serde_json::Value` を内部表現にしない** — 汎用値型は
//! 整数と浮動小数の区別を保持できず、既定値の wire 形が化ける（`document-format` の
//! `value` と同じ理由）。

use super::{ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl};
use crate::error::SchemaError;
use crate::types::datetime::{OffsetPolicy, TemporalForm};
use crate::types::decimal::{self, DecimalDigits};
use crate::types::text::{TextConstraints, TextPattern};
use crate::types::{Acceptance, TypeKind};
use document_format::{from_json_bytes, CellValue, NestedValue, RowId, SheetId, TypeDefId};
use serde_json::value::RawValue;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

/// ルートスキーマの `columns` キー。
const COLUMNS_KEY: &str = "columns";
/// 列・フィールドの名前のキー。
const NAME_KEY: &str = "name";
/// 型のキー（`custom` の拡張型の識別子も同じキーを使う）。
const TYPE_KEY: &str = "type";
/// 値なしを許さないかのキー。
const REQUIRED_KEY: &str = "required";
/// 一意制約のキー。
const UNIQUE_KEY: &str = "unique";
/// 既定値のキー。
const DEFAULT_KEY: &str = "default";
/// 説明のキー。
const DESCRIPTION_KEY: &str = "description";
/// 種別トークンのキー。
const KIND_KEY: &str = "kind";
/// 型定義への参照のキー（上流が構造として追跡できる唯一の形）。
const REF_KEY: &str = "$ref";
/// 範囲の下限のキー。
const MIN_KEY: &str = "min";
/// 範囲の上限のキー。
const MAX_KEY: &str = "max";
/// 文字列の最小長のキー。
const MIN_LENGTH_KEY: &str = "minLength";
/// 文字列の最大長のキー。
const MAX_LENGTH_KEY: &str = "maxLength";
/// 文字列の書式のキー。
const PATTERN_KEY: &str = "pattern";
/// 10 進数の有効桁数のキー。
const PRECISION_KEY: &str = "precision";
/// 10 進数の小数点以下の桁数のキー。
const SCALE_KEY: &str = "scale";
/// 列挙の選択肢のキー。
const CHOICES_KEY: &str = "choices";
/// 参照先シートのキー。
const SHEET_KEY: &str = "sheet";
/// 日時のオフセットの扱いのキー。
const OFFSET_KEY: &str = "offset";
/// オブジェクトのフィールドのキー。
const FIELDS_KEY: &str = "fields";
/// 配列の要素の型のキー。
const ITEMS_KEY: &str = "items";
/// 配列の要素数の下限のキー。
const MIN_ITEMS_KEY: &str = "minItems";
/// 配列の要素数の上限のキー。
const MAX_ITEMS_KEY: &str = "maxItems";

/// 型定義の `definition` ペイロードの位置の起点。
const TYPE_DEFINITION_POSITION: &str = "definition";

/// 列オブジェクトが持てるキー（tasks.md 3.1 の裁定「未知のキーは拒否する」）。
const COLUMN_KEYS: [&str; 6] = [
    NAME_KEY,
    TYPE_KEY,
    REQUIRED_KEY,
    UNIQUE_KEY,
    DEFAULT_KEY,
    DESCRIPTION_KEY,
];

/// 入れ子のフィールドオブジェクトが持てるキー（`unique` を持たない）。
const FIELD_KEYS: [&str; 5] = [
    NAME_KEY,
    TYPE_KEY,
    REQUIRED_KEY,
    DEFAULT_KEY,
    DESCRIPTION_KEY,
];

/// ルートスキーマのペイロード（`root` の中身）を解析する（tasks.md 3.2）。
///
/// `null` と、`columns` を持たない／空のオブジェクトは**列 0 本のシート**として受理する
/// （要件 1.8。上流の `SchemaPart::empty()` のルートは `null` である）。
pub fn parse_schema(text: &str) -> Result<Schema, SchemaError> {
    let root: Option<BTreeMap<String, Box<RawValue>>> =
        serde_json::from_str(text).map_err(|error| {
            malformed(
                "",
                &format!("schema root must be a JSON object or null: {error}"),
            )
        })?;
    let Some(map) = root else {
        return Ok(Schema::default());
    };
    for key in map.keys() {
        if key.as_str() != COLUMNS_KEY {
            return Err(malformed(key, "unknown key in schema root"));
        }
    }
    let Some(raw) = map.get(COLUMNS_KEY) else {
        return Ok(Schema::default());
    };
    let raws: Vec<Box<RawValue>> = serde_json::from_str(raw.get())
        .map_err(|error| malformed(COLUMNS_KEY, &format!("`columns` must be an array: {error}")))?;

    let mut columns = Vec::with_capacity(raws.len());
    let mut occurrences: HashMap<String, Vec<String>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for (index, raw) in raws.iter().enumerate() {
        let position = format!("{COLUMNS_KEY}[{index}]");
        let column = parse_column(raw, &position)?;
        let name = column.name.to_string();
        if !occurrences.contains_key(&name) {
            order.push(name.clone());
        }
        occurrences
            .entry(name)
            .or_default()
            .push(child(&position, NAME_KEY));
        columns.push(column);
    }
    for name in order {
        let positions = &occurrences[&name];
        if positions.len() > 1 {
            return Err(SchemaError::DuplicateColumnName {
                name,
                occurrences: positions.clone(),
            });
        }
    }
    Ok(Schema { columns })
}

/// 名前付き型定義の `definition` ペイロードを解析する（tasks.md 3.2。要件 3.1, 3.3）。
///
/// 位置は `definition` を起点に付く（入れ子のフィールドは `definition.fields[0].name`）。
/// 定義の集合（上流のエンベロープの `types` 配列）は `compile` 層（タスク 4.3, 4.4）が扱う。
pub fn parse_type_definition(text: &str) -> Result<TypeDecl, SchemaError> {
    let map: BTreeMap<String, Box<RawValue>> = serde_json::from_str(text).map_err(|error| {
        malformed(
            TYPE_DEFINITION_POSITION,
            &format!("type definition payload must be a JSON object: {error}"),
        )
    })?;
    parse_type_object(&map, TYPE_DEFINITION_POSITION)
}

/// 列オブジェクト 1 つを解析する（要件 1.3, 4.1, 4.2, 4.8）。
fn parse_column(raw: &RawValue, position: &str) -> Result<ColumnDecl, SchemaError> {
    let map = as_object(raw, position)?;
    for key in map.keys() {
        if !COLUMN_KEYS.contains(&key.as_str()) {
            return Err(malformed(
                &child(position, key),
                "unknown key in column declaration",
            ));
        }
    }
    let name = string_at(&map, NAME_KEY, position)?;
    if name.is_empty() {
        return Err(SchemaError::EmptyColumnName {
            position: child(position, NAME_KEY),
        });
    }
    let ty = parse_type(type_raw(&map, position)?, &child(position, TYPE_KEY))?;
    let required = bool_at(&map, REQUIRED_KEY, position)?;
    let unique = bool_at(&map, UNIQUE_KEY, position)?;
    let default = match map.get(DEFAULT_KEY) {
        Some(raw) => Some(cell_value(raw, &child(position, DEFAULT_KEY))?),
        None => None,
    };
    let description = optional_string_at(&map, DESCRIPTION_KEY, position)?;
    if let Some(value) = &default {
        check_default(&ty, value, required, &name, &child(position, TYPE_KEY))?;
    }
    Ok(ColumnDecl {
        name: name.into_boxed_str(),
        ty,
        required,
        unique,
        default,
        description,
    })
}

/// 入れ子のフィールドオブジェクト 1 つを解析する（要件 3.1, 3.2, 4.1, 4.2）。
fn parse_field(raw: &RawValue, position: &str) -> Result<FieldDecl, SchemaError> {
    let map = as_object(raw, position)?;
    for key in map.keys() {
        if !FIELD_KEYS.contains(&key.as_str()) {
            return Err(malformed(
                &child(position, key),
                "unknown key in field declaration",
            ));
        }
    }
    let name = string_at(&map, NAME_KEY, position)?;
    if name.is_empty() {
        return Err(malformed(
            &child(position, NAME_KEY),
            "field name must not be empty",
        ));
    }
    let ty = parse_type(type_raw(&map, position)?, &child(position, TYPE_KEY))?;
    let required = bool_at(&map, REQUIRED_KEY, position)?;
    let default = match map.get(DEFAULT_KEY) {
        Some(raw) => Some(cell_value(raw, &child(position, DEFAULT_KEY))?),
        None => None,
    };
    let description = optional_string_at(&map, DESCRIPTION_KEY, position)?;
    if let Some(value) = &default {
        check_default(&ty, value, required, position, &child(position, TYPE_KEY))?;
    }
    Ok(FieldDecl {
        name: name.into_boxed_str(),
        ty,
        required,
        default,
        description,
    })
}

/// 型オブジェクトの生テキストを解析する（design.md「型は常にオブジェクト」）。
fn parse_type(raw: &RawValue, position: &str) -> Result<TypeDecl, SchemaError> {
    let map = as_object(raw, position)?;
    parse_type_object(&map, position)
}

/// 型オブジェクトを、`kind` と `$ref` の二者択一として解析する。
fn parse_type_object(
    map: &BTreeMap<String, Box<RawValue>>,
    position: &str,
) -> Result<TypeDecl, SchemaError> {
    let has_kind = map.contains_key(KIND_KEY);
    let has_ref = map.contains_key(REF_KEY);
    match (has_kind, has_ref) {
        (true, true) => Err(malformed(
            position,
            "type object must carry either `kind` or `$ref`, not both",
        )),
        (false, false) => Err(malformed(
            position,
            "type object must carry either `kind` or `$ref`",
        )),
        (false, true) => {
            if map.len() != 1 {
                let extra = map
                    .keys()
                    .find(|key| key.as_str() != REF_KEY)
                    .expect("map.len() > 1 なら $ref 以外のキーがある");
                return Err(malformed(
                    &child(position, extra),
                    "`$ref` must be the only key of a type reference",
                ));
            }
            let reference = map.get(REF_KEY).expect("contains_key で確認済み");
            let reference_position = child(position, REF_KEY);
            let text = string_from(reference, &reference_position)?;
            let id = TypeDefId::from_str(&text).map_err(|error| {
                malformed(
                    &reference_position,
                    &format!("`$ref` is not a type definition id: {error}"),
                )
            })?;
            Ok(TypeDecl::Ref(id))
        }
        (true, false) => {
            let kind_raw = map.get(KIND_KEY).expect("contains_key で確認済み");
            let kind_position = child(position, KIND_KEY);
            let token = string_from(kind_raw, &kind_position)?;
            if token.is_empty() {
                return Err(malformed(
                    &kind_position,
                    "`kind` must be a non-empty string",
                ));
            }
            match kind_from_token(&token) {
                Some(kind) => Ok(TypeDecl::Kind {
                    kind: DeclaredKind::Known(kind),
                    constraints: parse_constraints(map, kind, position)?,
                }),
                // 未知の種別は拒否しない（要件 11.7）。その列だけを使用不能にする判断は
                // `compile` 層（タスク 4.4）が行う。ただし型オブジェクトが持てるのは
                // `kind` だけで、それ以外のキーは文法が定義しない以上ここで拒否する
                // （tasks.md 3.1 の裁定。読み飛ばすと往復で内容が落ちる）。
                None => {
                    if let Some(extra) = map.keys().find(|key| key.as_str() != KIND_KEY) {
                        return Err(malformed(
                            &child(position, extra),
                            &format!("`{extra}` is not a key of a type object"),
                        ));
                    }
                    Ok(TypeDecl::Kind {
                        kind: DeclaredKind::Unknown(token.into_boxed_str()),
                        constraints: Constraints::default(),
                    })
                }
            }
        }
    }
}

/// 種別ごとの値の制約を解析する（design.md「組込型カタログと `CellValue` への写像」表）。
///
/// その種別で意味を持たない位置は拒否する（tasks.md 3.1 の裁定。`Constraints` は平坦な
/// 構造であるため、黙って無視すると往復で内容差になる）。
fn parse_constraints(
    map: &BTreeMap<String, Box<RawValue>>,
    kind: TypeKind,
    position: &str,
) -> Result<Constraints, SchemaError> {
    let allowed = allowed_parameters(kind);
    for key in map.keys() {
        let key = key.as_str();
        if key != KIND_KEY && !allowed.contains(&key) {
            return Err(malformed(
                &child(position, key),
                &format!("`{key}` is not a parameter of `{}`", kind_token(kind)),
            ));
        }
    }

    let mut constraints = Constraints::default();
    match kind {
        TypeKind::Int | TypeKind::Float => {
            constraints.min = numeric_endpoint(map, MIN_KEY, position, kind)?;
            constraints.max = numeric_endpoint(map, MAX_KEY, position, kind)?;
        }
        TypeKind::Decimal => {
            constraints.digits = parse_digits(map, position)?;
            constraints.min = numeric_endpoint(map, MIN_KEY, position, kind)?;
            constraints.max = numeric_endpoint(map, MAX_KEY, position, kind)?;
        }
        TypeKind::Text => {
            constraints.min_length = size_at(map, MIN_LENGTH_KEY, position)?;
            constraints.max_length = size_at(map, MAX_LENGTH_KEY, position)?;
            if let (Some(min), Some(max)) = (constraints.min_length, constraints.max_length) {
                if min > max {
                    return Err(malformed(
                        &child(position, MIN_LENGTH_KEY),
                        "`minLength` exceeds `maxLength`",
                    ));
                }
            }
            constraints.pattern = optional_string_at(map, PATTERN_KEY, position)?;
        }
        TypeKind::Bool | TypeKind::Attachment | TypeKind::Any => {}
        TypeKind::Date => {
            constraints.min = temporal_endpoint(map, MIN_KEY, position, TemporalForm::Date)?;
            constraints.max = temporal_endpoint(map, MAX_KEY, position, TemporalForm::Date)?;
        }
        TypeKind::DateTime => {
            let offset = offset_at(map, position)?;
            constraints.offset = Some(offset);
            let form = TemporalForm::DateTime { offset };
            constraints.min = temporal_endpoint(map, MIN_KEY, position, form)?;
            constraints.max = temporal_endpoint(map, MAX_KEY, position, form)?;
        }
        TypeKind::Enum => {
            constraints.choices = choices_at(map, position)?;
        }
        TypeKind::Ref => {
            constraints.sheet = Some(sheet_at(map, position)?);
        }
        TypeKind::Object => {
            constraints.fields = fields_at(map, position)?;
        }
        TypeKind::Array => {
            let item_position = child(position, ITEMS_KEY);
            let item_raw = map
                .get(ITEMS_KEY)
                .ok_or_else(|| malformed(&item_position, "missing `items`"))?;
            constraints.items = Some(Box::new(parse_type(item_raw, &item_position)?));
            constraints.min_items = size_at(map, MIN_ITEMS_KEY, position)?;
            constraints.max_items = size_at(map, MAX_ITEMS_KEY, position)?;
            if let (Some(min), Some(max)) = (constraints.min_items, constraints.max_items) {
                if min > max {
                    return Err(malformed(
                        &child(position, MIN_ITEMS_KEY),
                        "`minItems` exceeds `maxItems`",
                    ));
                }
            }
        }
        TypeKind::Custom => {
            constraints.custom_type = Some(custom_type_at(map, position)?);
        }
    }
    Ok(constraints)
}

/// 既知の種別が持てるパラメータのキー（design.md の型カタログ表の「パラメータ」欄）。
fn allowed_parameters(kind: TypeKind) -> &'static [&'static str] {
    match kind {
        TypeKind::Int | TypeKind::Float => &[MIN_KEY, MAX_KEY],
        TypeKind::Decimal => &[PRECISION_KEY, SCALE_KEY, MIN_KEY, MAX_KEY],
        TypeKind::Text => &[MIN_LENGTH_KEY, MAX_LENGTH_KEY, PATTERN_KEY],
        TypeKind::Bool | TypeKind::Attachment | TypeKind::Any => &[],
        TypeKind::Date => &[MIN_KEY, MAX_KEY],
        TypeKind::DateTime => &[OFFSET_KEY, MIN_KEY, MAX_KEY],
        TypeKind::Enum => &[CHOICES_KEY],
        TypeKind::Ref => &[SHEET_KEY],
        TypeKind::Object => &[FIELDS_KEY],
        TypeKind::Array => &[ITEMS_KEY, MIN_ITEMS_KEY, MAX_ITEMS_KEY],
        TypeKind::Custom => &[TYPE_KEY],
    }
}

/// 種別トークン（宣言テキストの `kind`）を [`TypeKind`] へ写す（本層が表記を所有する）。
fn kind_from_token(token: &str) -> Option<TypeKind> {
    Some(match token {
        "int" => TypeKind::Int,
        "float" => TypeKind::Float,
        "decimal" => TypeKind::Decimal,
        "text" => TypeKind::Text,
        "bool" => TypeKind::Bool,
        "date" => TypeKind::Date,
        "datetime" => TypeKind::DateTime,
        "enum" => TypeKind::Enum,
        "ref" => TypeKind::Ref,
        "attachment" => TypeKind::Attachment,
        "object" => TypeKind::Object,
        "array" => TypeKind::Array,
        "any" => TypeKind::Any,
        "custom" => TypeKind::Custom,
        _ => return None,
    })
}

/// [`TypeKind`] の種別トークン（診断メッセージに使う）。
fn kind_token(kind: TypeKind) -> &'static str {
    match kind {
        TypeKind::Int => "int",
        TypeKind::Float => "float",
        TypeKind::Decimal => "decimal",
        TypeKind::Text => "text",
        TypeKind::Bool => "bool",
        TypeKind::Date => "date",
        TypeKind::DateTime => "datetime",
        TypeKind::Enum => "enum",
        TypeKind::Ref => "ref",
        TypeKind::Attachment => "attachment",
        TypeKind::Object => "object",
        TypeKind::Array => "array",
        TypeKind::Any => "any",
        TypeKind::Custom => "custom",
    }
}

/// 生の JSON 値をオブジェクトとして読む（型・列・フィールドの共通の入口）。
fn as_object(
    raw: &RawValue,
    position: &str,
) -> Result<BTreeMap<String, Box<RawValue>>, SchemaError> {
    serde_json::from_str(raw.get())
        .map_err(|error| malformed(position, &format!("expected a JSON object: {error}")))
}

/// 列・フィールドの `type` の生テキストを読む（型は必須。要件 1.3）。
fn type_raw<'a>(
    map: &'a BTreeMap<String, Box<RawValue>>,
    position: &str,
) -> Result<&'a RawValue, SchemaError> {
    map.get(TYPE_KEY)
        .map(|raw| raw.as_ref())
        .ok_or_else(|| malformed(&child(position, TYPE_KEY), "missing `type`"))
}

/// セル値の wire 形を読む（`document-format` の `value` が定める唯一の経路。
/// 整数リテラルの範囲検査と `$t` の脱出口の復号を含む）。
fn cell_value(raw: &RawValue, position: &str) -> Result<CellValue, SchemaError> {
    from_json_bytes(raw.get().as_bytes())
        .map_err(|error| malformed(position, &format!("invalid cell value: {error}")))
}

/// 文字列のキーを読む（欠落も型違いも [`SchemaError::MalformedDeclaration`]）。
fn string_at(
    map: &BTreeMap<String, Box<RawValue>>,
    key: &str,
    position: &str,
) -> Result<String, SchemaError> {
    let raw = map
        .get(key)
        .ok_or_else(|| malformed(&child(position, key), &format!("missing `{key}`")))?;
    string_from(raw, &child(position, key))
}

/// 生の JSON 値を文字列として読む。
fn string_from(raw: &RawValue, position: &str) -> Result<String, SchemaError> {
    serde_json::from_str(raw.get())
        .map_err(|error| malformed(position, &format!("expected a string: {error}")))
}

/// 真偽のキーを読む（欠落は `false`）。
fn bool_at(
    map: &BTreeMap<String, Box<RawValue>>,
    key: &str,
    position: &str,
) -> Result<bool, SchemaError> {
    match map.get(key) {
        None => Ok(false),
        Some(raw) => serde_json::from_str(raw.get()).map_err(|error| {
            malformed(
                &child(position, key),
                &format!("`{key}` must be a boolean: {error}"),
            )
        }),
    }
}

/// 文字列のキーを任意として読む（欠落と `null` は未宣言）。
fn optional_string_at(
    map: &BTreeMap<String, Box<RawValue>>,
    key: &str,
    position: &str,
) -> Result<Option<Box<str>>, SchemaError> {
    match map.get(key) {
        None => Ok(None),
        Some(raw) => serde_json::from_str::<Option<String>>(raw.get())
            .map(|value| value.map(String::into_boxed_str))
            .map_err(|error| {
                malformed(
                    &child(position, key),
                    &format!("`{key}` must be a string: {error}"),
                )
            }),
    }
}

/// 非負整数のキーを任意として読む（長さ・要素数）。
fn size_at(
    map: &BTreeMap<String, Box<RawValue>>,
    key: &str,
    position: &str,
) -> Result<Option<usize>, SchemaError> {
    match map.get(key) {
        None => Ok(None),
        Some(raw) => serde_json::from_str::<usize>(raw.get())
            .map(Some)
            .map_err(|error| {
                malformed(
                    &child(position, key),
                    &format!("`{key}` must be a non-negative integer: {error}"),
                )
            }),
    }
}

/// 数値の型の範囲の端点を読む（`int` / `float` / `decimal`）。
///
/// 端点はその型が受け入れるセル値の変種でなければならない（`int` の端点は整数、
/// `decimal` の端点は文法に一致する 10 進数の文字列）。宣言のための第 2 の値表現を
/// 作らないため、端点もセル値の wire 形で読む。
fn numeric_endpoint(
    map: &BTreeMap<String, Box<RawValue>>,
    key: &str,
    position: &str,
    kind: TypeKind,
) -> Result<Option<CellValue>, SchemaError> {
    let Some(raw) = map.get(key) else {
        return Ok(None);
    };
    let endpoint_position = child(position, key);
    let value = cell_value(raw, &endpoint_position)?;
    let fits = match (&value, kind) {
        (CellValue::Int(_), TypeKind::Int) => true,
        (CellValue::Float(_), TypeKind::Float) => true,
        (CellValue::Decimal(text), TypeKind::Decimal) => decimal::scan(text).is_some(),
        _ => false,
    };
    if !fits {
        return Err(malformed(
            &endpoint_position,
            &format!("`{key}` must be a value of type `{}`", kind_token(kind)),
        ));
    }
    Ok(Some(value))
}

/// 日時の型の範囲の端点を読む（正準表記に一致する文字列だけ）。
fn temporal_endpoint(
    map: &BTreeMap<String, Box<RawValue>>,
    key: &str,
    position: &str,
    form: TemporalForm,
) -> Result<Option<CellValue>, SchemaError> {
    let Some(raw) = map.get(key) else {
        return Ok(None);
    };
    let endpoint_position = child(position, key);
    let value = cell_value(raw, &endpoint_position)?;
    match &value {
        CellValue::Text(text) if form.accepts(text) == Acceptance::Conforming => Ok(Some(value)),
        _ => Err(malformed(
            &endpoint_position,
            &format!(
                "`{key}` must be a canonical value of `{}`",
                kind_token(match form {
                    TemporalForm::Date => TypeKind::Date,
                    TemporalForm::DateTime { .. } => TypeKind::DateTime,
                })
            ),
        )),
    }
}

/// 10 進数の桁数を読む（`precision` と `scale` は同時に宣言する）。
///
/// 片方だけの宣言は [`DecimalDigits`] を組み立てられない（`declaration` 層の `Constraints`
/// に桁数の専用変種が無いため [`SchemaError::MalformedDeclaration`] に落とす。tasks.md 2.2）。
fn parse_digits(
    map: &BTreeMap<String, Box<RawValue>>,
    position: &str,
) -> Result<Option<DecimalDigits>, SchemaError> {
    match (map.get(PRECISION_KEY), map.get(SCALE_KEY)) {
        (None, None) => Ok(None),
        (Some(_), None) => Err(malformed(
            &child(position, SCALE_KEY),
            "`scale` is required when `precision` is declared",
        )),
        (None, Some(_)) => Err(malformed(
            &child(position, PRECISION_KEY),
            "`precision` is required when `scale` is declared",
        )),
        (Some(precision), Some(scale)) => {
            let precision = unsigned_at(precision, PRECISION_KEY, position)?;
            let scale = unsigned_at(scale, SCALE_KEY, position)?;
            DecimalDigits::new(precision, scale).map(Some).ok_or_else(|| {
                malformed(
                    &child(position, PRECISION_KEY),
                    &format!(
                        "impossible decimal digits: precision {precision} must be >= 1 and scale {scale} <= precision"
                    ),
                )
            })
        }
    }
}

/// 非負整数のキーを読む（`precision` / `scale`）。
fn unsigned_at(raw: &RawValue, key: &str, position: &str) -> Result<u32, SchemaError> {
    serde_json::from_str::<u32>(raw.get()).map_err(|error| {
        malformed(
            &child(position, key),
            &format!("`{key}` must be a non-negative integer: {error}"),
        )
    })
}

/// 列挙の選択肢を読む（欠落は空。宣言順を保つ）。
fn choices_at(
    map: &BTreeMap<String, Box<RawValue>>,
    position: &str,
) -> Result<Vec<Box<str>>, SchemaError> {
    let Some(raw) = map.get(CHOICES_KEY) else {
        return Ok(Vec::new());
    };
    let choices: Vec<String> = serde_json::from_str(raw.get()).map_err(|error| {
        malformed(
            &child(position, CHOICES_KEY),
            &format!("`choices` must be an array of strings: {error}"),
        )
    })?;
    Ok(choices.into_iter().map(String::into_boxed_str).collect())
}

/// 参照先シートの識別子を読む（`ref` 種別では必須。要件 9.1）。
fn sheet_at(map: &BTreeMap<String, Box<RawValue>>, position: &str) -> Result<SheetId, SchemaError> {
    let sheet_position = child(position, SHEET_KEY);
    let raw = map
        .get(SHEET_KEY)
        .ok_or_else(|| malformed(&sheet_position, "missing `sheet`"))?;
    let text = string_from(raw, &sheet_position)?;
    SheetId::from_str(&text).map_err(|error| {
        malformed(
            &sheet_position,
            &format!("`sheet` is not a sheet id: {error}"),
        )
    })
}

/// 日時のオフセットの扱いを読む（`datetime` 種別では必須。要件 2.4）。
fn offset_at(
    map: &BTreeMap<String, Box<RawValue>>,
    position: &str,
) -> Result<OffsetPolicy, SchemaError> {
    let offset_position = child(position, OFFSET_KEY);
    let raw = map
        .get(OFFSET_KEY)
        .ok_or_else(|| malformed(&offset_position, "missing `offset`"))?;
    let text = string_from(raw, &offset_position)?;
    match text.as_str() {
        "forbidden" => Ok(OffsetPolicy::Forbidden),
        "required" => Ok(OffsetPolicy::Required),
        other => Err(malformed(
            &offset_position,
            &format!("`offset` must be \"forbidden\" or \"required\": {other}"),
        )),
    }
}

/// オブジェクトのフィールドを読む（欠落は 0 個。空のオブジェクトはフィールド 0 個）。
///
/// 同じ配列の中で `name` が重複する宣言は、重複した 2 つ目の出現の位置つきで拒否する
/// （要件 1.5, 1.7）。位置の表現が名前と添字の並びである以上、同じ名前の 2 つの
/// フィールドは区別できず、要件 3.4 の位置特定が成立しないためである。
fn fields_at(
    map: &BTreeMap<String, Box<RawValue>>,
    position: &str,
) -> Result<Vec<FieldDecl>, SchemaError> {
    let Some(raw) = map.get(FIELDS_KEY) else {
        return Ok(Vec::new());
    };
    let fields_position = child(position, FIELDS_KEY);
    let raws: Vec<Box<RawValue>> = serde_json::from_str(raw.get()).map_err(|error| {
        malformed(
            &fields_position,
            &format!("`fields` must be an array: {error}"),
        )
    })?;
    let fields: Vec<FieldDecl> = raws
        .iter()
        .enumerate()
        .map(|(index, raw)| parse_field(raw, &format!("{fields_position}[{index}]")))
        .collect::<Result<_, _>>()?;
    // 同じ `fields` 配列の中で名前が重複すると、位置の表現（`ValuePath` はフィールド名と
    // 添字の並び）が 2 つの別のフィールドを同じ位置に写し、要件 3.4 の「入れ子の内側の
    // 位置を特定できる形で違反を報告する」が構造上満たせなくなる。列名の重複と同じ扱いで、
    // 重複した 2 つ目の出現を位置として宣言ごと拒否する（要件 1.5, 1.7）。
    let mut seen: HashSet<&str> = HashSet::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        if !seen.insert(field.name.as_ref()) {
            return Err(malformed(
                &child(&format!("{fields_position}[{index}]"), NAME_KEY),
                &format!("duplicate field name `{}`", field.name),
            ));
        }
    }
    Ok(fields)
}

/// 拡張型の識別子を読む（`custom` 種別では必須。要件 11.1, 11.2）。
fn custom_type_at(
    map: &BTreeMap<String, Box<RawValue>>,
    position: &str,
) -> Result<Box<str>, SchemaError> {
    let type_position = child(position, TYPE_KEY);
    let raw = map
        .get(TYPE_KEY)
        .ok_or_else(|| malformed(&type_position, "missing `type`"))?;
    let text = string_from(raw, &type_position)?;
    if text.is_empty() {
        return Err(malformed(
            &type_position,
            "`type` must be a non-empty string",
        ));
    }
    Ok(text.into_boxed_str())
}

/// 列・フィールドの既定値が、その型とその場で判定できる制約に適合するか検査する
/// （要件 4.8。tasks.md 3.2 の責務の境界）。
fn check_default(
    ty: &TypeDecl,
    value: &CellValue,
    required: bool,
    label: &str,
    type_position: &str,
) -> Result<(), SchemaError> {
    if conforms_type(ty, value, required, type_position)? {
        Ok(())
    } else {
        Err(SchemaError::InvalidDefault {
            column: label.to_owned(),
        })
    }
}

/// 値が型に適合するか。**その場で判定できる範囲**だけを見る（tasks.md 3.2）。
///
/// 識別子参照の先（[`TypeDecl::Ref`]）と拡張型（`custom`）は型が解決されるまで判定できず、
/// 未知の種別は文法が未知であるため、いずれも適合として素通しし、`compile` 層（タスク 4.4）
/// へ委ねる。値なし（`CellValue::Null`）は型の段階では適合であり、必須の指定に反するとき
/// だけ違反になる（design.md の型カタログ表）。
fn conforms_type(
    ty: &TypeDecl,
    value: &CellValue,
    required: bool,
    type_position: &str,
) -> Result<bool, SchemaError> {
    if matches!(value, CellValue::Null) {
        return Ok(!required);
    }
    match ty {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(kind),
            constraints,
        } => conforms_kind(*kind, constraints, value, type_position),
        TypeDecl::Kind {
            kind: DeclaredKind::Unknown(_),
            ..
        }
        | TypeDecl::Ref(_) => Ok(true),
    }
}

/// 既知の組込種別に対する適合判定（変種と、その場で判定できる値の制約）。
fn conforms_kind(
    kind: TypeKind,
    constraints: &Constraints,
    value: &CellValue,
    type_position: &str,
) -> Result<bool, SchemaError> {
    if kind.accepts(value) == Acceptance::Violating {
        return Ok(false);
    }
    match kind {
        TypeKind::Int => Ok(int_conforms(value, constraints)),
        TypeKind::Float => Ok(float_conforms(value, constraints)),
        TypeKind::Decimal => Ok(decimal_conforms(value, constraints)),
        TypeKind::Text => text_conforms(value, constraints, type_position),
        TypeKind::Bool | TypeKind::Attachment | TypeKind::Any | TypeKind::Custom => Ok(true),
        TypeKind::Date => Ok(temporal_conforms(value, constraints, TemporalForm::Date)),
        TypeKind::DateTime => Ok(constraints.offset.is_some_and(|offset| {
            temporal_conforms(value, constraints, TemporalForm::DateTime { offset })
        })),
        TypeKind::Enum => Ok(enum_conforms(value, constraints)),
        TypeKind::Ref => Ok(ref_conforms(value)),
        TypeKind::Object => object_conforms(value, constraints, type_position),
        TypeKind::Array => array_conforms(value, constraints, type_position),
    }
}

/// 整数の既定値が範囲に収まるか。
fn int_conforms(value: &CellValue, constraints: &Constraints) -> bool {
    let CellValue::Int(value) = value else {
        return false;
    };
    let min_ok = match &constraints.min {
        None => true,
        Some(CellValue::Int(min)) => value >= min,
        Some(_) => false,
    };
    let max_ok = match &constraints.max {
        None => true,
        Some(CellValue::Int(max)) => value <= max,
        Some(_) => false,
    };
    min_ok && max_ok
}

/// 倍精度小数の既定値が範囲に収まるか。
fn float_conforms(value: &CellValue, constraints: &Constraints) -> bool {
    let CellValue::Float(value) = value else {
        return false;
    };
    let min_ok = match &constraints.min {
        None => true,
        Some(CellValue::Float(min)) => value >= min,
        Some(_) => false,
    };
    let max_ok = match &constraints.max {
        None => true,
        Some(CellValue::Float(max)) => value <= max,
        Some(_) => false,
    };
    min_ok && max_ok
}

/// 10 進数の既定値が桁数の宣言と範囲に収まるか（値の比較は正準形で行う。tasks.md 2.2）。
fn decimal_conforms(value: &CellValue, constraints: &Constraints) -> bool {
    let CellValue::Decimal(text) = value else {
        return false;
    };
    if let Some(digits) = constraints.digits {
        if digits.accepts(text) == Acceptance::Violating {
            return false;
        }
    }
    let Some(canonical) = decimal::canonicalize(text) else {
        return false;
    };
    let within = |bound: &Option<CellValue>, lower: bool| match bound {
        None => true,
        Some(CellValue::Decimal(text)) => match decimal::canonicalize(text) {
            Some(bound) if lower => canonical >= bound,
            Some(bound) => canonical <= bound,
            None => false,
        },
        Some(_) => false,
    };
    within(&constraints.min, true) && within(&constraints.max, false)
}

/// 日時の既定値が正準表記と範囲に収まるか（比較は瞬時。tasks.md 2.3）。
fn temporal_conforms(value: &CellValue, constraints: &Constraints, form: TemporalForm) -> bool {
    let CellValue::Text(text) = value else {
        return false;
    };
    let Some(parsed) = form.parse(text) else {
        return false;
    };
    let within = |bound: &Option<CellValue>, lower: bool| match bound {
        None => true,
        Some(CellValue::Text(text)) => match form.parse(text) {
            Some(bound) if lower => parsed >= bound,
            Some(bound) => parsed <= bound,
            None => false,
        },
        Some(_) => false,
    };
    within(&constraints.min, true) && within(&constraints.max, false)
}

/// 列挙の既定値が選択肢のいずれかであるか。
fn enum_conforms(value: &CellValue, constraints: &Constraints) -> bool {
    let CellValue::Text(text) = value else {
        return false;
    };
    constraints
        .choices
        .iter()
        .any(|choice| choice.as_ref() == text.as_str())
}

/// 参照の既定値が行識別子であるか（参照先の実在は一括判定（タスク 5.3）が受け持つ）。
fn ref_conforms(value: &CellValue) -> bool {
    matches!(value, CellValue::Text(text) if RowId::from_str(text).is_ok())
}

/// 文字列の既定値が長さと書式に適合するか（長さは文字数。tasks.md 2.4）。
fn text_conforms(
    value: &CellValue,
    constraints: &Constraints,
    type_position: &str,
) -> Result<bool, SchemaError> {
    let CellValue::Text(text) = value else {
        return Ok(false);
    };
    let pattern = match &constraints.pattern {
        Some(source) => Some(TextPattern::compile(
            source,
            &child(type_position, PATTERN_KEY),
        )?),
        None => None,
    };
    let Some(text_constraints) =
        TextConstraints::new(constraints.min_length, constraints.max_length, pattern)
    else {
        return Ok(false);
    };
    Ok(text_constraints.accepts(text) == Acceptance::Conforming)
}

/// オブジェクトの既定値が、宣言されたフィールドの型と制約に適合するか。
///
/// 宣言に無い名前の成員は適合しない。既定値に現れないフィールドは、既定値を持つ必須
/// フィールドだけが入れ子の既定値の適用（要件 4.3）で埋まるため適合とし、**既定値も
/// 持たない必須フィールド**はその既定値から完全なオブジェクトを作れないため適合しない
/// （要件 4.8）。
fn object_conforms(
    value: &CellValue,
    constraints: &Constraints,
    type_position: &str,
) -> Result<bool, SchemaError> {
    let CellValue::Nested(NestedValue::Object(entries)) = value else {
        return Ok(false);
    };
    for (name, member) in entries {
        let Some((index, field)) = constraints
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name.as_ref() == name.as_str())
        else {
            return Ok(false);
        };
        let field_position = format!("{type_position}.{FIELDS_KEY}[{index}]");
        if !conforms_type(
            &field.ty,
            member,
            field.required,
            &child(&field_position, TYPE_KEY),
        )? {
            return Ok(false);
        }
    }
    for field in &constraints.fields {
        let present = entries
            .iter()
            .any(|(name, _)| name.as_str() == field.name.as_ref());
        if field.required && field.default.is_none() && !present {
            return Ok(false);
        }
    }
    Ok(true)
}

/// 配列の既定値が要素の型と要素数の制約に適合するか。
fn array_conforms(
    value: &CellValue,
    constraints: &Constraints,
    type_position: &str,
) -> Result<bool, SchemaError> {
    let CellValue::Nested(NestedValue::Array(items)) = value else {
        return Ok(false);
    };
    if constraints.min_items.is_some_and(|min| items.len() < min) {
        return Ok(false);
    }
    if constraints.max_items.is_some_and(|max| items.len() > max) {
        return Ok(false);
    }
    let Some(item_type) = constraints.items.as_deref() else {
        return Ok(true);
    };
    let item_position = child(type_position, ITEMS_KEY);
    for item in items {
        if !conforms_type(item_type, item, false, &item_position)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// 宣言上の位置を組み立てる（空の起点ではキーそのもの）。
fn child(position: &str, key: &str) -> String {
    if position.is_empty() {
        key.to_owned()
    } else {
        format!("{position}.{key}")
    }
}

/// 解釈できない構造を拒否する誤り（要件 1.7）。
fn malformed(position: &str, reason: &str) -> SchemaError {
    SchemaError::MalformedDeclaration {
        position: position.to_owned(),
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TypeKind;

    /// 列 1 本だけのルートスキーマを組み立てる。`type_object` は `type` の値そのもの、
    /// `extra` は列オブジェクトへ足す残りのキー（例 `,"default":0`）。
    fn one_column(type_object: &str, extra: &str) -> String {
        format!(r#"{{"columns":[{{"name":"c","type":{type_object}{extra}}}]}}"#)
    }

    /// 型定義の識別子。ULID の正準テキスト形である。
    fn type_id(text: &str) -> TypeDefId {
        text.parse().expect("正準 ULID")
    }

    /// シート識別子。ULID の正準テキスト形である。
    fn sheet_id(text: &str) -> SheetId {
        text.parse().expect("正準 ULID")
    }

    /// 設計例のルートスキーマ（design.md「スキーマ宣言の文法」のルートスキーマ）。
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

    /// 設計例がそのまま宣言のデータ構造へ落ちる（tasks.md 3.2「不透明ペイロードの文法を
    /// 解析する」）。
    #[test]
    fn parses_the_design_root_example() {
        let schema = parse_schema(ROOT_EXAMPLE).expect("設計例は妥当な宣言");
        assert_eq!(7, schema.columns.len());
        assert_eq!(
            vec!["品番", "数量", "単価", "納品日", "仕入先", "属性", "備考"],
            schema
                .columns
                .iter()
                .map(|column| column.name.as_ref())
                .collect::<Vec<_>>()
        );

        let part_number = &schema.columns[0];
        assert!(part_number.required);
        assert!(part_number.unique);
        assert_eq!(None, part_number.default);
        match &part_number.ty {
            TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Text),
                constraints,
            } => assert_eq!(Some(32), constraints.max_length),
            other => panic!("品番 は text のはず: {other:?}"),
        }

        let quantity = &schema.columns[1];
        assert!(!quantity.required);
        assert!(!quantity.unique);
        assert_eq!(Some(CellValue::Int(0)), quantity.default);
        match &quantity.ty {
            TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Int),
                constraints,
            } => {
                assert_eq!(Some(CellValue::Int(0)), constraints.min);
                assert_eq!(None, constraints.max);
            }
            other => panic!("数量 は int のはず: {other:?}"),
        }

        match &schema.columns[2].ty {
            TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Decimal),
                constraints,
            } => assert_eq!(
                Some(DecimalDigits::new(12, 2).expect("12 >= 1 かつ 2 <= 12")),
                constraints.digits
            ),
            other => panic!("単価 は decimal のはず: {other:?}"),
        }

        match &schema.columns[4].ty {
            TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Ref),
                constraints,
            } => assert_eq!(
                Some(sheet_id("01K4ANRRG004HMASW9NF6YY091")),
                constraints.sheet
            ),
            other => panic!("仕入先 は ref のはず: {other:?}"),
        }

        assert_eq!(
            TypeDecl::Ref(type_id("01K4ANRRG004HMASW9NF6YY092")),
            schema.columns[5].ty
        );
    }

    /// 列 0 本のルートスキーマをエラーとしない（要件 1.8）。上流の空ルートは `null` である。
    #[test]
    fn empty_roots_are_accepted_as_zero_columns() {
        assert_eq!(Schema::default(), parse_schema("null").expect("null は空"));
        assert_eq!(
            Schema::default(),
            parse_schema(r#"{"columns":[]}"#).expect("列 0 本は空")
        );
        assert_eq!(
            Schema::default(),
            parse_schema("{}").expect("columns 無しは空")
        );
    }

    /// 重複する列名を、列名と全出現位置つきで拒否する（要件 1.5）。
    #[test]
    fn duplicate_column_names_are_rejected_with_all_positions() {
        let error = parse_schema(
            r#"{"columns":[
                {"name":"a","type":{"kind":"any"}},
                {"name":"b","type":{"kind":"any"}},
                {"name":"a","type":{"kind":"any"}}
            ]}"#,
        )
        .expect_err("重複は拒否される");
        match error {
            SchemaError::DuplicateColumnName { name, occurrences } => {
                assert_eq!("a", name);
                assert_eq!(
                    occurrences,
                    ["columns[0].name", "columns[2].name"],
                    "全出現位置が宣言順に並ぶ"
                );
            }
            other => panic!("DuplicateColumnName のはず: {other:?}"),
        }
    }

    /// 空の列名を位置つきで拒否する（要件 1.6）。
    #[test]
    fn empty_column_names_are_rejected_with_position() {
        let error = parse_schema(r#"{"columns":[{"name":"","type":{"kind":"any"}}]}"#)
            .expect_err("空の列名は拒否される");
        match error {
            SchemaError::EmptyColumnName { position } => {
                assert_eq!("columns[0].name", position);
            }
            other => panic!("EmptyColumnName のはず: {other:?}"),
        }
    }

    /// 種別と識別子参照の**両方**を持つ型を拒否する（tasks.md 3.2 の明示の要求）。
    #[test]
    fn a_type_carrying_both_kind_and_ref_is_rejected() {
        let error = parse_schema(&one_column(
            r#"{"kind":"int","$ref":"01K4ANRRG004HMASW9NF6YY092"}"#,
            "",
        ))
        .expect_err("両方は拒否される");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("columns[0].type", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }
    }

    /// 種別と識別子参照の**どちらも**持たない型を拒否する（tasks.md 3.2 の明示の要求）。
    #[test]
    fn a_type_carrying_neither_kind_nor_ref_is_rejected() {
        let error = parse_schema(&one_column("{}", "")).expect_err("どちらも無い型は拒否される");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("columns[0].type", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }
    }

    /// 型がオブジェクトでないものは拒否する（design.md「型は常にオブジェクト」）。
    #[test]
    fn a_non_object_type_is_rejected() {
        let error = parse_schema(&one_column("7", "")).expect_err("数値は型でない");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("columns[0].type", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }
    }

    /// 未知の `kind` は拒否せず保持する（要件 11.7 の前提。その列だけを使用不能にする判断は
    /// 4.4 が行う）。ただし型オブジェクトが持てるのは `kind` だけであり、未知の種別でも
    /// 文法が定義しないキーは読み飛ばさず拒否する（tasks.md 3.1 の裁定。読み飛ばすと
    /// 「宣言 → テキスト → 宣言」の往復で内容が落ちる）。
    #[test]
    fn an_unknown_kind_is_preserved_and_its_extra_keys_are_rejected() {
        let schema = parse_schema(&one_column(r#"{"kind":"geo-point"}"#, ""))
            .expect("未知の種別でも宣言は読める");
        assert_eq!(
            TypeDecl::Kind {
                kind: DeclaredKind::Unknown("geo-point".into()),
                constraints: Constraints::default(),
            },
            schema.columns[0].ty
        );

        let error = parse_schema(&one_column(r#"{"kind":"geo-point","x":1}"#, ""))
            .expect_err("未知の種別の追加キーは拒否される");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("columns[0].type.x", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }
    }

    /// `$ref` は識別子を運ぶ専用のキーであり、型定義の識別子へ解決される（design.md
    /// 「型定義への参照は必ず `{"$ref": …}` と書く」）。
    #[test]
    fn a_ref_type_carries_the_type_definition_id() {
        let schema = parse_schema(&one_column(r#"{"$ref":"01K4ANRRG004HMASW9NF6YY092"}"#, ""))
            .expect("参照は妥当");
        assert_eq!(
            TypeDecl::Ref(type_id("01K4ANRRG004HMASW9NF6YY092")),
            schema.columns[0].ty
        );
    }

    /// `$ref` に他のキーを併記する形は拒否する（参照は専用のキーだけのオブジェクトである）。
    #[test]
    fn a_ref_combined_with_other_keys_is_rejected() {
        let error = parse_schema(&one_column(
            r#"{"$ref":"01K4ANRRG004HMASW9NF6YY092","kind":"int"}"#,
            "",
        ))
        .expect_err("両方のキーは拒否される");
        assert!(
            matches!(error, SchemaError::MalformedDeclaration { .. }),
            "MalformedDeclaration のはず: {error:?}"
        );
    }

    /// 文法が定義しないキーを、位置つきで拒否する（要件 1.7。tasks.md 3.1 の裁定）。
    #[test]
    fn unknown_keys_in_declarations_are_rejected_with_position() {
        let error = parse_schema(&one_column(r#"{"kind":"any"}"#, r#","wat":1"#))
            .expect_err("未知のキーは拒否される");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("columns[0].wat", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }

        let error = parse_schema(&one_column(r#"{"kind":"int","nope":1}"#, ""))
            .expect_err("未知のパラメータは拒否される");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("columns[0].type.nope", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }
    }

    /// その種別で意味を持たないパラメータの位置を拒否する（tasks.md 3.1 の裁定）。
    #[test]
    fn parameters_meaningless_for_a_kind_are_rejected() {
        let error = parse_schema(&one_column(r#"{"kind":"bool","choices":["x"]}"#, ""))
            .expect_err("bool に choices は無い");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("columns[0].type.choices", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }
    }

    /// 既定値はセル値と同一の wire 形で読む（design.md「既定値はセル値と同一の wire 形」）。
    /// 裸の文字列は wire の判別規則で `Decimal` になり、`Text` は脱出口で書く。
    #[test]
    fn defaults_are_read_in_the_cell_value_wire_form() {
        let schema = parse_schema(
            r#"{"columns":[
                {"name":"i","type":{"kind":"int"},"default":7},
                {"name":"d","type":{"kind":"decimal"},"default":"1.50"},
                {"name":"t","type":{"kind":"text"},"default":{"$t":"text","v":"123"}}
            ]}"#,
        )
        .expect("wire 形の既定値");
        assert_eq!(Some(CellValue::Int(7)), schema.columns[0].default);
        assert_eq!(
            Some(CellValue::Decimal("1.50".to_owned())),
            schema.columns[1].default
        );
        assert_eq!(
            Some(CellValue::Text("123".to_owned())),
            schema.columns[2].default
        );
    }

    /// 組込種別が直接書かれた列の既定値の適合をその場で検査する（要件 4.8）。
    #[test]
    fn defaults_of_directly_written_builtin_kinds_are_checked() {
        // 型の変種に合わない（int に文字列）。
        let error = parse_schema(&one_column(r#"{"kind":"int"}"#, r#","default":"abc""#))
            .expect_err("型に合わない既定値は拒否される");
        match error {
            SchemaError::InvalidDefault { column } => assert_eq!("c", column),
            other => panic!("InvalidDefault のはず: {other:?}"),
        }

        // 制約に合わない（int の min 0 に対して -1）。
        let error = parse_schema(&one_column(r#"{"kind":"int","min":0}"#, r#","default":-1"#))
            .expect_err("制約に合わない既定値は拒否される");
        assert!(
            matches!(error, SchemaError::InvalidDefault { .. }),
            "InvalidDefault のはず: {error:?}"
        );

        // 文字列の長さ制約に合わない。
        let error = parse_schema(&one_column(
            r#"{"kind":"text","maxLength":3}"#,
            r#","default":"abcd""#,
        ))
        .expect_err("長さ制約に合わない既定値は拒否される");
        assert!(
            matches!(error, SchemaError::InvalidDefault { .. }),
            "InvalidDefault のはず: {error:?}"
        );

        // 列挙の選択肢に無い。
        let error = parse_schema(&one_column(
            r#"{"kind":"enum","choices":["赤","青"]}"#,
            r#","default":"緑""#,
        ))
        .expect_err("選択肢に無い既定値は拒否される");
        assert!(
            matches!(error, SchemaError::InvalidDefault { .. }),
            "InvalidDefault のはず: {error:?}"
        );

        // 桁数制約に合わない（decimal(2,1) に "1.50" は scale 超過）。
        let error = parse_schema(&one_column(
            r#"{"kind":"decimal","precision":2,"scale":1}"#,
            r#","default":"1.50""#,
        ))
        .expect_err("桁数制約に合わない既定値は拒否される");
        assert!(
            matches!(error, SchemaError::InvalidDefault { .. }),
            "InvalidDefault のはず: {error:?}"
        );

        // 日時の形に合わない（offset: forbidden に Z 付き）。
        let error = parse_schema(&one_column(
            r#"{"kind":"datetime","offset":"forbidden"}"#,
            r#","default":"2026-09-12T10:30:00Z""#,
        ))
        .expect_err("形に合わない既定値は拒否される");
        assert!(
            matches!(error, SchemaError::InvalidDefault { .. }),
            "InvalidDefault のはず: {error:?}"
        );

        // 必須の入れ子フィールドが既定値を持たず、オブジェクトの既定値にも現れない。
        let error = parse_schema(&one_column(
            r#"{"kind":"object","fields":[{"name":"a","type":{"kind":"int"},"required":true}]}"#,
            r#","default":{}"#,
        ))
        .expect_err("必須フィールドを欠くオブジェクトの既定値は拒否される");
        assert!(
            matches!(error, SchemaError::InvalidDefault { .. }),
            "InvalidDefault のはず: {error:?}"
        );

        // 値なしを許さない列の既定値が値なしである。
        let error = parse_schema(&format!(
            r#"{{"columns":[{{"name":"c","type":{{"kind":"int"}},"required":true,"default":null}}]}}"#
        ))
        .expect_err("必須列の null 既定値は拒否される");
        assert!(
            matches!(error, SchemaError::InvalidDefault { .. }),
            "InvalidDefault のはず: {error:?}"
        );
    }

    /// 適合する既定値は通す（要件 4.8 の肯定側）。
    #[test]
    fn conforming_defaults_are_accepted() {
        let schema = parse_schema(
            r#"{"columns":[
                {"name":"i","type":{"kind":"int","min":0,"max":10},"default":5},
                {"name":"f","type":{"kind":"float","min":0.5},"default":1.5},
                {"name":"d","type":{"kind":"decimal","precision":3,"scale":1},"default":"1.5"},
                {"name":"t","type":{"kind":"text","maxLength":4},"default":"abcd"},
                {"name":"e","type":{"kind":"enum","choices":["赤","青"]},"default":"赤"},
                {"name":"dt","type":{"kind":"datetime","offset":"required"},"default":"2026-09-12T10:30:00+09:00"},
                {"name":"ob","type":{"kind":"object","fields":[{"name":"色","type":{"kind":"text"}}]},"default":{"色":"赤"}},
                {"name":"ar","type":{"kind":"array","items":{"kind":"int"},"minItems":1},"default":[1,2]}
            ]}"#,
        )
        .expect("適合する既定値");
        assert_eq!(Some(CellValue::Int(5)), schema.columns[0].default);
    }

    /// 識別子参照の先と拡張型の既定値は、型が解決されるまで判定できないためここでは
    /// 素通しする（tasks.md 3.2。4.4 が受け持つ）。
    #[test]
    fn defaults_behind_refs_and_custom_types_are_deferred() {
        let schema = parse_schema(
            r#"{"columns":[
                {"name":"r","type":{"$ref":"01K4ANRRG004HMASW9NF6YY092"},"default":"whatever"},
                {"name":"x","type":{"kind":"custom","type":"postal-code"},"default":{"deep":[1,2]}}
            ]}"#,
        )
        .expect("解決前の既定値はここでは判定しない");
        assert_eq!(2, schema.columns.len());
    }

    /// `ref` 列の既定値は行識別子でなければならない（組込種別が直接書かれているため、
    /// その場で判定できる。要件 4.8）。
    #[test]
    fn a_ref_default_must_be_a_row_identifier() {
        let error = parse_schema(&one_column(
            r#"{"kind":"ref","sheet":"01K4ANRRG004HMASW9NF6YY091"}"#,
            r#","default":"not-a-row-id""#,
        ))
        .expect_err("行識別子でない既定値は拒否される");
        assert!(
            matches!(error, SchemaError::InvalidDefault { .. }),
            "InvalidDefault のはず: {error:?}"
        );

        let row = document_format::IdFactory::new().new_row_id();
        let schema = parse_schema(&one_column(
            r#"{"kind":"ref","sheet":"01K4ANRRG004HMASW9NF6YY091"}"#,
            &format!(r#","default":"{row}""#),
        ))
        .expect("行識別子の既定値は通る");
        assert_eq!(
            Some(CellValue::Text(row.to_string())),
            schema.columns[0].default
        );
    }

    /// `ref` 種別は参照先シートを必須とする（要件 9.1）。
    #[test]
    fn a_ref_without_a_sheet_is_rejected() {
        let error =
            parse_schema(&one_column(r#"{"kind":"ref"}"#, "")).expect_err("sheet 欠落は拒否される");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("columns[0].type.sheet", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }
    }

    /// `datetime` 種別はオフセットの扱いを必須とする（要件 2.4）。
    #[test]
    fn a_datetime_without_an_offset_policy_is_rejected() {
        let error = parse_schema(&one_column(r#"{"kind":"datetime"}"#, ""))
            .expect_err("offset 欠落は拒否される");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("columns[0].type.offset", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }
    }

    /// `array` 種別は要素の型を必須とする（要件 3.1）。
    #[test]
    fn an_array_without_items_is_rejected() {
        let error = parse_schema(&one_column(r#"{"kind":"array"}"#, ""))
            .expect_err("items 欠落は拒否される");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("columns[0].type.items", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }
    }

    /// 実現しえない 10 進数の桁数宣言を拒否する（tasks.md 2.2 の裁定。`DecimalDigits` は
    /// `precision >= 1` かつ `scale <= precision` を不変条件とする）。
    #[test]
    fn impossible_decimal_digits_are_rejected() {
        let error = parse_schema(&one_column(
            r#"{"kind":"decimal","precision":2,"scale":3}"#,
            "",
        ))
        .expect_err("scale > precision は拒否される");
        assert!(
            matches!(error, SchemaError::MalformedDeclaration { .. }),
            "MalformedDeclaration のはず: {error:?}"
        );

        // 片方だけの宣言は `DecimalDigits` を組み立てられないため拒否する。
        let error = parse_schema(&one_column(r#"{"kind":"decimal","precision":2}"#, ""))
            .expect_err("scale 欠落は拒否される");
        assert!(
            matches!(error, SchemaError::MalformedDeclaration { .. }),
            "MalformedDeclaration のはず: {error:?}"
        );
    }

    /// 解釈できないルート（オブジェクトでも `null` でもない）を拒否する（要件 1.7）。
    #[test]
    fn a_non_object_root_is_rejected() {
        let error = parse_schema("[1,2,3]").expect_err("配列はルートでない");
        assert!(
            matches!(error, SchemaError::MalformedDeclaration { .. }),
            "MalformedDeclaration のはず: {error:?}"
        );
    }

    /// 型定義の `definition` ペイロードを解析し、入れ子のフィールドまで読み下す
    /// （要件 3.1, 3.2）。位置は `definition` を起点に付く。
    #[test]
    fn parses_a_type_definition_payload_with_nested_fields() {
        let ty = parse_type_definition(
            r#"{"kind":"object","fields":[
                {"name":"色","type":{"kind":"enum","choices":["赤","青"]},"required":true},
                {"name":"備考","type":{"kind":"text"},"description":"任意"}
            ]}"#,
        )
        .expect("型定義は妥当");
        match ty {
            TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Object),
                constraints,
            } => {
                assert_eq!(2, constraints.fields.len());
                assert_eq!("色", constraints.fields[0].name.as_ref());
                assert!(constraints.fields[0].required);
                assert_eq!(Some("任意".into()), constraints.fields[1].description);
            }
            other => panic!("object のはず: {other:?}"),
        }

        // 入れ子のフィールドの誤りも `definition` 起点の位置つきで拒否する。
        let error = parse_type_definition(
            r#"{"kind":"object","fields":[{"name":"","type":{"kind":"text"}}]}"#,
        )
        .expect_err("空のフィールド名は拒否される");
        match error {
            SchemaError::MalformedDeclaration { position, .. } => {
                assert_eq!("definition.fields[0].name", position);
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }
    }

    /// `array` の `items` は入れ子の型として解析される（要件 3.2）。
    #[test]
    fn parses_nested_array_items() {
        let ty = parse_type_definition(
            r#"{"kind":"array","items":{"kind":"object","fields":[{"name":"n","type":{"kind":"int"}}]},"minItems":1}"#,
        )
        .expect("入れ子の配列");
        match ty {
            TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Array),
                constraints,
            } => {
                assert_eq!(Some(1), constraints.min_items);
                assert!(matches!(
                    constraints.items.as_deref(),
                    Some(TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Object),
                        ..
                    })
                ));
            }
            other => panic!("array のはず: {other:?}"),
        }
    }

    /// 1 つのオブジェクト型の中で入れ子のフィールド名が重複する宣言を、**重複した 2 つ目の
    /// 出現**の位置つきで拒否する（要件 1.5, 1.7, 3.4）。位置の表現（`ValuePath` は
    /// フィールド名と添字の並び）では同じ名前の 2 つのフィールドを区別できず、要件 3.4 の
    /// 「入れ子の内側の位置を特定できる形で違反を報告する」が成立しないため、列名の重複と
    /// 同じ扱いで宣言ごと拒否する。名前が重複しなければ同じ形が受理される（対照）。
    #[test]
    fn duplicate_field_names_are_rejected_at_the_second_occurrence() {
        let error = parse_type_definition(
            r#"{"kind":"object","fields":[
                {"name":"a","type":{"kind":"int"}},
                {"name":"b","type":{"kind":"int"}},
                {"name":"a","type":{"kind":"int"}}
            ]}"#,
        )
        .expect_err("重複するフィールド名は拒否される");
        match error {
            SchemaError::MalformedDeclaration { position, reason } => {
                assert_eq!("definition.fields[2].name", position);
                assert!(
                    reason.contains("duplicate") && reason.contains('a'),
                    "重複した名前を含む: {reason}"
                );
            }
            other => panic!("MalformedDeclaration のはず: {other:?}"),
        }

        // 名前が重複しない同じ形は受理される。
        let ty = parse_type_definition(
            r#"{"kind":"object","fields":[
                {"name":"a","type":{"kind":"int"}},
                {"name":"b","type":{"kind":"int"}}
            ]}"#,
        )
        .expect("重複が無ければ受理される");
        match ty {
            TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Object),
                constraints,
            } => assert_eq!(2, constraints.fields.len()),
            other => panic!("object のはず: {other:?}"),
        }
    }
}

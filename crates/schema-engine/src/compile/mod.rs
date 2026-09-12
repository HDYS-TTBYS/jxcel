//! コンパイル層（design.md「コンポーネントとファイルの対応」の `SchemaCompiler`。
//! tasks.md 4.4。要件 1.1, 1.2, 1.8, 4.8, 11.7）。
//!
//! 宣言を一度だけ解決し、以後の実行から宣言の走査を排除する（design.md
//! 「Compile Layer / SchemaCompiler」）。本層が生む計画（列添字で引ける検証器の配列）が、
//! 10 万行の走査の内側で引かれる唯一のものである。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本層は `registry` までを参照でき、`coerce` / `validate` 以降へは
//! **依存しない**。本層を参照するのは `{ coerce, validate }` 以降である。
//!
//! 列の添字の型（[`ColumnIndex`]）は [`plan`] が定義し、本モジュールが再輸出する。列添字を
//! 所有するのは、その添字で引ける配列（`columns` と `validators`）を持つ本層だからであり、
//! 鎖の向きと一致する（`validate` 層の `Violation` はこの型を参照する側である）。
//!
//! # 本モジュールが受け持つこと（design.md「Compile Layer / SchemaCompiler」）
//!
//! - **列の集合と並び順の決定と供給**（要件 1.1, 1.2）。並び順は宣言の `columns` 配列の
//!   順そのものであり、[`CompiledSchema::columns`] が行データを永続化する呼び出し元へ
//!   渡す唯一の経路である。上流の `Sheet::columns()` はこの並びを受け取って出力する側で
//!   あり、本クレートはそれを書かない（design.md「Out of Boundary」）。
//! - **`$ref` の解決**（要件 3.5）と、値が有限の大きさで存在しえない循環の検出（要件 3.6）。
//!   どちらも [`resolve::resolve`] が行い、本モジュールはその結果を計画へ落とす。
//! - **解決済みの型の検証器への落とし込み**（[`plan::ColumnValidator`]）。
//! - **書式のパターンのコンパイル**（要件 4.5）。宣言はパターンを生文字列で持ち
//!   （`Constraints::pattern`）、コンパイルは**ここで一度だけ**行う。走査の内側では
//!   照合だけが起きる（design.md 同節）。
//! - **列を 1 本も宣言していないルートスキーマの受理**（要件 1.8）。列 0 本のシートは
//!   正当であり、新規シートの初期状態がこれにあたる。
//! - **既定値の適合検査のうち、型が解決されて初めて判定できる分**（要件 4.8）。層の鎖は
//!   `declaration → registry → compile` であり、`declaration::codec` は型定義参照の先も
//!   拡張型も見られない。組込種別が直接書かれた列の既定値は codec が検査し、**参照の先と
//!   拡張型の既定値はここで検査する**（design.md 同節）。
//!
//! # 解決できない列は使用不能にする（design.md「解決できない型の扱い」。要件 11.7）
//!
//! 未知の `kind`（将来の版が足した種別）と未登録の拡張型は、**その列だけを使用不能**に
//! する。[`CompiledSchema::validators`] の該当の添字が `None` になり、
//! [`CompiledSchema::unusable_columns`] がその添字を配る。シートは開ける（要件 11.7）。
//! [`CompiledSchema::unusable_kind`] は解釈できなかった宣言の識別子（未知の `kind` トークン /
//! 未登録の拡張型の識別子）を配り、`validate` 層がそれを
//! `ViolationReason::UnusableColumn` の `kind` 欄へ流す（要件 11.7 の「該当する列と型の
//! 識別子を含む」）。
//!
//! 使用不能な列を一意制約の対象から外すのは、一意性の比較に要る正準化の手段が
//! 未登録の拡張型には無いためである（[`CompiledSchema::unique_columns`]）。
//!
//! **入れ子の内側の未知・未登録は列全体を落とす。** [`plan::ColumnValidator::Object`] の
//! `fields` と [`plan::ColumnValidator::Array`] の `items` は検証器を値として内包するため、
//! 内側の 1 つが解決できないならその列は値の全体を判定できない。使用不能の単位は要件
//! 11.7 の「列」である。
//!
//! # `decimal` の桁数は宣言できるものであり必須ではない（要件 2.3）
//!
//! `{"kind":"decimal"}`（`precision` も `scale` も無い）は**正当な宣言**である。要件 2.3 は
//! 「有効桁数と小数点以下の桁数を宣言**できるようにする**」であり、宣言を必須にしていない。
//! 桁数は [`plan::ColumnValidator::Decimal`] の `digits` に `Option` のまま渡り、宣言が
//! 無ければ**桁の判定を行わない**（`min` / `max` の範囲は従来どおり判定する。10 進数の
//! 正準形は宣言された `scale` に依存しないため、桁が無くても範囲比較は成立する）。
//! 文法に一致しない `Decimal` は桁の宣言の有無に依らず違反であり、宣言が無いときは
//! `PrecisionExceeded` ではなく型の不一致に落ちる（`plan` の docs）。
//!
//! # 有限に展開できない型定義（本モジュールの限界。要件 11.7）
//!
//! [`plan::ColumnValidator`] は入れ子の検証器を**値として内包する**ため、値が再帰する型
//! 定義は**有限の検証器へ展開できない**。本モジュールは
//! `unexpandable_definitions` で「**すべての**参照辺に循環がある定義」と「それを推移的に
//! 参照する定義」を求め、展開がそこへ到達した時点で**その列だけを使用不能**に落とす —
//! スキーマを破棄せず（要件 11.7 の原則）、残りの列は正しく検証できる状態を保つ。
//!
//! この判定は [`resolve::resolve`] の循環検出（要件 3.6）とは**別物**である。あちらは「値が有限の
//! 大きさで存在しえない循環」を探すために**必須かつ非配列**の辺だけを見るが、展開可能性を
//! 決めるのは `required: false` の辺も配列の `items` の辺も含めた**すべての参照辺**である。
//! `required: false` や配列を経由する再帰は正当な宣言であり（design.md「Compile Layer /
//! SchemaCompiler」）、コンパイルエラーにしてはならない。
//!
//! 到達した定義の識別子は [`CompiledSchema::unusable_kind`] が運ぶ（未知の `kind` や未登録の
//! 拡張型と同じ扱い）。
//!
//! # 誤りと違反の分離（design.md「Components and Interfaces」）
//!
//! 本モジュールが返すのは宣言が壊れていることを表す [`SchemaError`] だけである。値が
//! 宣言に合わないことは `validate` 層の違反が表し、コンパイルは止めない。既定値の不適合
//! （要件 4.8）は**宣言の側の誤り**であるため、`InvalidDefault` としてコンパイルを止める。

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use document_format::{CellValue, NestedValue, RowId, Sheet, TypeDefId};

use crate::declaration::codec::{parse_schema, parse_type_definition};
use crate::declaration::{Constraints, DeclaredKind, Schema, TypeDecl, TypeDefinition};
use crate::error::SchemaError;
use crate::registry::{CustomTypeId, TypeRegistry};
use crate::types::datetime::{TemporalForm, TemporalValue};
use crate::types::decimal;
use crate::types::text::{TextConstraints, TextPattern};
use crate::types::TypeKind;

use plan::{Bounds, ColumnValidator, ColumnVerdict, FieldValidator};
use resolve::{resolve, Resolver};

/// 列の添字を本層の公開面へ出す（定義は [`plan`]。列添字を所有するのは、その添字で引ける
/// 配列を持つ `compile` 層である）。
pub use plan::ColumnIndex;

pub mod plan;
pub mod resolve;

/// ルートスキーマの列の配列のキー（`declaration::codec` と同じ）。
const COLUMNS: &str = "columns";
/// 型のキー。
const TYPE_KEY: &str = "type";
/// オブジェクトのフィールドのキー。
const FIELDS_KEY: &str = "fields";
/// 配列の要素の型のキー。
const ITEMS_KEY: &str = "items";
/// 範囲の下限・上限のキー。
const MIN_KEY: &str = "min";
const MAX_KEY: &str = "max";
/// 文字列の最小長のキー。
const MIN_LENGTH_KEY: &str = "minLength";
/// 書式のパターンのキー。
const PATTERN_KEY: &str = "pattern";
/// 日時のオフセットの扱いのキー。
const OFFSET_KEY: &str = "offset";
/// 参照先シートのキー。
const SHEET_KEY: &str = "sheet";
/// 型定義の本文ペイロードの位置の起点（`compile::resolve` と同じ）。
const DEFINITION_KEY: &str = "definition";

/// シートのルートスキーマと型定義を解決し、列添字で引ける計画へ落とす
/// （要件 1.1, 1.2, 1.8, 4.8, 11.7）。
///
/// 上流が不透明なペイロードとして保持する `root` と各 `definition` の中身を、
/// [`crate::declaration::codec`] で読み出して [`compile_declaration`] へ渡す。上流の
/// エンベロープ（`{ "root": …, "types": [ … ] }`）と決定的なバイト出力は
/// `document-format` が所有する（design.md「Out of Boundary」）。
pub fn compile(sheet: &Sheet, registry: &TypeRegistry) -> Result<CompiledSchema, SchemaError> {
    let schema_part = sheet.root_schema();
    let schema = parse_schema(schema_part.root().as_str())?;
    let mut definitions = Vec::with_capacity(schema_part.type_defs().len());
    for definition in schema_part.type_defs() {
        definitions.push(TypeDefinition {
            id: definition.id(),
            definition: parse_type_definition(definition.definition().as_str())?,
        });
    }
    compile_declaration(&schema, &definitions, registry)
}

/// ルートスキーマの宣言と型定義の集合から計画を組み立てる
/// （要件 1.1, 1.2, 1.8, 4.8, 11.7）。
///
/// 列の並び順は引数の `schema.columns` の並びそのものである。**列名の配列と検証器の配列は
/// 常に同じ長さで同じ添字**になり（design.md「Data Models / Domain Model」の不変条件）、
/// 使用不能な列も添字を保つ。
pub fn compile_declaration(
    schema: &Schema,
    definitions: &[TypeDefinition],
    registry: &TypeRegistry,
) -> Result<CompiledSchema, SchemaError> {
    let resolver = resolve(schema, definitions)?;
    let unexpandable = unexpandable_definitions(definitions);
    let mut compiler = Compiler {
        resolver: &resolver,
        registry,
        unexpandable: &unexpandable,
        patterns: BTreeMap::new(),
    };

    let count = schema.columns.len();
    let mut columns = Vec::with_capacity(count);
    let mut validators = Vec::with_capacity(count);
    let mut required = Vec::with_capacity(count);
    let mut defaults = Vec::with_capacity(count);
    let mut unique = Vec::new();
    let mut unusable = Vec::new();
    let mut unusable_kinds = Vec::new();

    for (index, column) in schema.columns.iter().enumerate() {
        let position = child(&indexed(COLUMNS, index), TYPE_KEY);
        let validator = match compiler.validator(&column.ty, &position)? {
            Built::Validator(built) => {
                if let Some(value) = &column.default {
                    if !compiler.conforms(value, &column.ty, column.required, &built)? {
                        return Err(SchemaError::InvalidDefault {
                            column: column.name.to_string(),
                        });
                    }
                }
                if column.unique {
                    unique.push(ColumnIndex::new(index));
                }
                Some(built)
            }
            // 未知の種別・未登録の拡張型・再帰する型定義。その列だけを使用不能にする
            // （要件 11.7）。値は `validate` 層が `ViolationReason::UnusableColumn` に落とし、
            // `kind` 欄にはここで記録した識別子が入る。
            Built::Unusable(kind) => {
                unusable.push(ColumnIndex::new(index));
                unusable_kinds.push(kind);
                None
            }
        };
        columns.push(column.name.to_string());
        validators.push(validator);
        required.push(column.required);
        defaults.push(column.default.clone());
    }

    Ok(CompiledSchema {
        columns: columns.into_boxed_slice(),
        validators: validators.into_boxed_slice(),
        required: required.into_boxed_slice(),
        defaults: defaults.into_boxed_slice(),
        unique: unique.into_boxed_slice(),
        unusable: unusable.into_boxed_slice(),
        unusable_kinds: unusable_kinds.into_boxed_slice(),
    })
}

/// 宣言を一度だけ解決した計画（design.md「Compile Layer / SchemaCompiler」の
/// State Management。要件 1.1, 1.2, 1.8, 11.7）。
///
/// **不変条件**: 列名の配列（[`CompiledSchema::columns`]）と検証器の配列
/// （[`CompiledSchema::validators`]）は常に同じ長さで同じ添字であり、`Row::values()` も
/// この添字で並ぶ（design.md「Data Models / Domain Model」）。使用不能な列は検証器の
/// 側で `None` になり、添字は動かない（[`CompiledSchema::unusable_columns`]）。
///
/// 計画は不変である。スキーマが変われば作り直す（design.md 同節の State Management）。
///
/// # 列ごとの既定値を計画が持つ理由（design.md の記載漏れを補う）
///
/// design.md の状態一覧（列名・検証器・一意制約の列・使用不能な列）には既定値が
/// 無いが、`default_row(&self, schema: &CompiledSchema) -> Vec<CellValue>` は**計画だけ**を
/// 引数に取る。行の初期値（要件 4.3）を供給するには、既定値が計画から引けねばならない
/// （宣言を引き直す経路を残すと、コンパイル層を置いた意味が消える）。適合の検査は
/// コンパイル時（要件 4.8）に済んでいるため、[`CompiledSchema::default_value`] は検査済みの
/// 値をそのまま返す。
#[derive(Debug, Clone)]
pub struct CompiledSchema {
    /// 宣言順の列名。この並びが行データのキー順になる（要件 1.1, 1.2）。
    columns: Box<[String]>,
    /// 列添字で引ける検証器。使用不能な列は `None`（要件 11.7）。
    validators: Box<[Option<ColumnValidator>]>,
    /// 値なしを許さないか（要件 4.1, 4.4）。列の側の制約であり、型の検証器は持たない。
    required: Box<[bool]>,
    /// 列の既定値（要件 4.2, 4.3）。未宣言は `None`。
    defaults: Box<[Option<CellValue>]>,
    /// 一意制約を持つ列の添字（要件 4.6, 4.7）。使用不能な列は含まない。
    unique: Box<[ColumnIndex]>,
    /// 使用不能な列の添字（昇順）。
    unusable: Box<[ColumnIndex]>,
    /// `unusable` と同じ並びの、解釈できなかった宣言の識別子
    /// （未知の `kind` トークン / 未登録の拡張型の識別子 / 再帰した型定義の識別子）。
    unusable_kinds: Box<[Box<str>]>,
}

impl CompiledSchema {
    /// 行データを永続化する呼び出し元へ渡す列名（並び順そのもの。要件 1.1, 1.2）。
    ///
    /// 上流の `Sheet::columns()` と同じ形（`&[String]`）であり、呼び出し元はこれを
    /// そのまま行データのキー順として出力する。
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// 列数（[`CompiledSchema::columns`] と [`CompiledSchema::validators`] に共通の長さ）。
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// 列添字で引ける検証器の配列。使用不能な列は `None`（要件 11.7）。
    pub fn validators(&self) -> &[Option<ColumnValidator>] {
        &self.validators
    }

    /// 列 1 本分の検証器。使用不能な列と計画の外の添字は `None`。
    pub fn validator(&self, column: ColumnIndex) -> Option<&ColumnValidator> {
        self.validators.get(column.index())?.as_ref()
    }

    /// その列が使用不能であるか（未知の種別・未登録の拡張型・再帰する型定義。要件 11.7）。
    ///
    /// 使用不能な列の値は、理由によらずすべて `ViolationReason::UnusableColumn` になる
    /// （design.md「解決できない型の扱い」）。計画の外の添字も使用不能として扱う。
    pub fn is_unusable(&self, column: ColumnIndex) -> bool {
        !matches!(self.validators.get(column.index()), Some(Some(_)))
    }

    /// 使用不能な列の添字（昇順。要件 11.7）。
    pub fn unusable_columns(&self) -> &[ColumnIndex] {
        &self.unusable
    }

    /// 使用不能な列を解釈できなかった識別子（要件 11.7 の報告が運ぶ）。
    ///
    /// 未知の `kind` ではそのトークン、未登録の拡張型では宣言の `type` の識別子、再帰する
    /// 型定義では再入した型定義の識別子である。`validate` 層はこれを
    /// `ViolationReason::UnusableColumn` の `kind` 欄へそのまま流す（要件 11.7 の「該当する
    /// 列と型の識別子を含む」）。使用可能な列と計画の外の添字は `None`。
    pub fn unusable_kind(&self, column: ColumnIndex) -> Option<&str> {
        let found = self.unusable.binary_search(&column).ok()?;
        self.unusable_kinds.get(found).map(Box::as_ref)
    }

    /// その列が値なしを許さないか（要件 4.1, 4.4）。計画の外の添字は `false`。
    ///
    /// 必須の判定は値の型の判定ではないため、型の検証器（[`plan::ColumnValidator`]）では
    /// なく計画が持つ（tasks.md 3.1 の裁定）。
    pub fn required(&self, column: ColumnIndex) -> bool {
        self.required.get(column.index()).copied().unwrap_or(false)
    }

    /// その列の既定値（要件 4.2, 4.3）。未宣言と計画の外の添字は `None`。
    ///
    /// 行の初期値の組み立て（タスク 6.3）はこの経路で値を引き、既定値の無い列へ値なしを
    /// 与える。既定値の適合は**使用可能な列では**コンパイル時に検査済みである（要件 4.8。
    /// 使用不能な列の扱いは [`CompiledSchema::default_row`] を参照）。
    pub fn default_value(&self, column: ColumnIndex) -> Option<&CellValue> {
        self.defaults.get(column.index())?.as_ref()
    }

    /// 行を追加するときの初期値の列（要件 4.2, 4.3）。
    ///
    /// 宣言された既定値を列の並び順に並べ、既定値が宣言されていない列には値なし
    /// （[`CellValue::Null`]）を与える。返す列の長さは**常に**
    /// [`CompiledSchema::column_count`] と一致する — 行の値は列の添字で並ぶため
    /// （design.md「Data Models / Domain Model」の不変条件）、使用不能な列も含めて
    /// すべての列に値が 1 つずつ入る。
    ///
    /// **行の追加そのものは本クレートの仕事ではない。** 上流の `Document::add_row` が行を
    /// 発行し、`Document::set_row_values` がこの値を書く（design.md「Public API Layer /
    /// SchemaEngineApi」の `default_row`）。本クレートは値だけを供給する。
    ///
    /// 既定値の適合は**使用可能な列では**コンパイル時に検査済みであるため（要件 4.8）、
    /// ここでは [`CompiledSchema::default_value`] が返す検査済みの値をそのまま複製する。
    /// **使用不能な列（未知の種別・未登録の拡張型・展開できない再帰型。要件 11.7）では
    /// 適合検査が走らない**（検証器が無いため）ので、宣言された既定値をそのまま供給する。
    /// その列の値は既定値を含めてすべて `ViolationReason::UnusableColumn` の違反になるため、
    /// 実害は無い（タスク 6.3 の裁定）。
    pub fn default_row(&self) -> Vec<CellValue> {
        self.defaults
            .iter()
            .map(|value| value.clone().unwrap_or(CellValue::Null))
            .collect()
    }

    /// 一意制約を持つ列の添字（要件 4.6, 4.7）。
    ///
    /// 使用不能な列は含まない — 一意性の比較に要る正準形を作る手段が、未登録の拡張型には
    /// 無いためである（[`plan::ColumnValidator::Custom`] の実装が正準化を持つ）。
    pub fn unique_columns(&self) -> &[ColumnIndex] {
        &self.unique
    }
}

/// 宣言を検証器へ落とす作業者（1 回のコンパイルの間だけ生きる）。
struct Compiler<'a> {
    /// 参照を解決済みの型定義の集合（宣言の誤りは [`resolve`] が拒否済み）。
    resolver: &'a Resolver,
    /// 拡張型の登録。
    registry: &'a TypeRegistry,
    /// 有限の検証器へ展開できない型定義（[`unexpandable_definitions`]）。
    unexpandable: &'a BTreeSet<TypeDefId>,
    /// コンパイル済みの書式（生文字列 → コンパイル済み）。同じパターンを 2 度コンパイル
    /// しない — 型定義は複数の列から参照されうるため、参照のたびに `regex` を組み立てると
    /// 「コンパイルはここで一度だけ」が参照の数だけ繰り返される。
    patterns: BTreeMap<Box<str>, TextPattern>,
}

/// 型 1 つを計画へ落とした結果。
///
/// 使用不能は**値の違反ではなく計画の性質**であり、その列の値はすべて
/// `ViolationReason::UnusableColumn` になる（design.md「解決できない型の扱い」。要件 11.7）。
/// 報告に要る「解釈できなかった宣言の識別子」をここで運ぶ（未知の `kind` トークン /
/// 未登録の拡張型の識別子 / 再帰した型定義の識別子）。
enum Built {
    /// 検証器へ落ちた。
    Validator(ColumnValidator),
    /// 使用不能。解釈できなかった宣言の識別子を運ぶ。
    Unusable(Box<str>),
}

impl Compiler<'_> {
    /// 型 1 つを計画へ落とす（要件 11.7）。
    ///
    /// `position` は宣言上の位置であり、パターンのコンパイルが拒否したときの診断にそのまま
    /// 載る（`declaration::codec` と同じ文法）。
    fn validator(&mut self, ty: &TypeDecl, position: &str) -> Result<Built, SchemaError> {
        match ty {
            TypeDecl::Ref(id) => {
                // `resolve` が実在を検査済みであるため `None` は防御である（起こらない）。
                let Some(body) = self.resolver.get(*id) else {
                    return Ok(Built::Unusable(format!("{id}").into()));
                };
                if self.unexpandable.contains(id) {
                    // 有限の検証器へ展開できない型定義（再帰、または再帰を推移的に参照する
                    // 定義）。スキーマは破棄せず、その列だけを使用不能にする（要件 11.7）。
                    // 報告には**展開を断ったこの型定義の識別子**を運ぶ。
                    return Ok(Built::Unusable(format!("{id}").into()));
                }
                self.validator(body, &definition_position(*id))
            }
            TypeDecl::Kind { kind, constraints } => {
                self.kind_validator(kind, constraints, position)
            }
        }
    }

    /// 種別による指定 1 つを検証器へ落とす（design.md「組込型カタログと `CellValue` への
    /// 写像」の表の 14 種別と 1 対 1。`plan::ColumnValidator` の変種と同じ並び）。
    ///
    /// 種別ごとのパラメータの取り出しは `declaration::codec` が検査済みの値を前提にできる
    /// ため、ここでの欠落は内部の食い違いとしてだけ現れる（欠落は宣言の誤りとして報告する）。
    fn kind_validator(
        &mut self,
        kind: &DeclaredKind,
        constraints: &Constraints,
        position: &str,
    ) -> Result<Built, SchemaError> {
        let known = match kind {
            DeclaredKind::Known(known) => *known,
            // 未知の種別はその列だけを使用不能にする（要件 11.7）。トークンは宣言テキストの
            // 往復を担う `declaration` 層が保持しており、報告のためにここへ写す。
            DeclaredKind::Unknown(token) => return Ok(Built::Unusable(token.clone())),
        };
        let validator = match known {
            TypeKind::Int => ColumnValidator::Int {
                bounds: Bounds::new(
                    constraints.min.clone(),
                    constraints.max.clone(),
                    endpoint(
                        constraints.min.as_ref(),
                        &child(position, MIN_KEY),
                        |value| match value {
                            CellValue::Int(found) => Some(*found),
                            _ => None,
                        },
                    )?,
                    endpoint(
                        constraints.max.as_ref(),
                        &child(position, MAX_KEY),
                        |value| match value {
                            CellValue::Int(found) => Some(*found),
                            _ => None,
                        },
                    )?,
                ),
            },
            TypeKind::Float => ColumnValidator::Float {
                bounds: Bounds::new(
                    constraints.min.clone(),
                    constraints.max.clone(),
                    endpoint(
                        constraints.min.as_ref(),
                        &child(position, MIN_KEY),
                        |value| match value {
                            CellValue::Float(found) => Some(*found),
                            _ => None,
                        },
                    )?,
                    endpoint(
                        constraints.max.as_ref(),
                        &child(position, MAX_KEY),
                        |value| match value {
                            CellValue::Float(found) => Some(*found),
                            _ => None,
                        },
                    )?,
                ),
            },
            TypeKind::Decimal => {
                // 桁数は宣言**できる**ものであり必須ではない（要件 2.3）。宣言が無ければ
                // `None` のまま計画へ渡り、桁の判定を行わない（`plan::ColumnValidator`）。
                let min = endpoint(
                    constraints.min.as_ref(),
                    &child(position, MIN_KEY),
                    |value| match value {
                        CellValue::Decimal(text) => decimal::canonicalize(text),
                        _ => None,
                    },
                )?;
                let max = endpoint(
                    constraints.max.as_ref(),
                    &child(position, MAX_KEY),
                    |value| match value {
                        CellValue::Decimal(text) => decimal::canonicalize(text),
                        _ => None,
                    },
                )?;
                ColumnValidator::Decimal {
                    digits: constraints.digits,
                    bounds: Bounds::new(constraints.min.clone(), constraints.max.clone(), min, max),
                }
            }
            TypeKind::Text => {
                // パターンのコンパイルはここで一度だけ行う（design.md「Compile Layer /
                // SchemaCompiler」）。上限を超えるパターンはこの時点で拒否され、走査の
                // 内側では照合だけが起きる。
                let pattern = match &constraints.pattern {
                    Some(source) => Some(self.pattern(source, &child(position, PATTERN_KEY))?),
                    None => None,
                };
                let text =
                    TextConstraints::new(constraints.min_length, constraints.max_length, pattern)
                        .ok_or_else(|| {
                        malformed(
                            &child(position, MIN_LENGTH_KEY),
                            "`minLength` exceeds `maxLength`",
                        )
                    })?;
                ColumnValidator::Text { constraints: text }
            }
            TypeKind::Bool => ColumnValidator::Bool,
            TypeKind::Date => {
                let form = TemporalForm::Date;
                ColumnValidator::Date {
                    bounds: Bounds::new(
                        constraints.min.clone(),
                        constraints.max.clone(),
                        temporal_endpoint(
                            constraints.min.as_ref(),
                            &child(position, MIN_KEY),
                            form,
                        )?,
                        temporal_endpoint(
                            constraints.max.as_ref(),
                            &child(position, MAX_KEY),
                            form,
                        )?,
                    ),
                }
            }
            TypeKind::DateTime => {
                let offset = constraints.offset.ok_or_else(|| {
                    malformed(&child(position, OFFSET_KEY), "`datetime` requires `offset`")
                })?;
                let form = TemporalForm::DateTime { offset };
                ColumnValidator::DateTime {
                    form,
                    bounds: Bounds::new(
                        constraints.min.clone(),
                        constraints.max.clone(),
                        temporal_endpoint(
                            constraints.min.as_ref(),
                            &child(position, MIN_KEY),
                            form,
                        )?,
                        temporal_endpoint(
                            constraints.max.as_ref(),
                            &child(position, MAX_KEY),
                            form,
                        )?,
                    ),
                }
            }
            TypeKind::Enum => ColumnValidator::Enum {
                choices: constraints.choices.clone().into_boxed_slice(),
            },
            TypeKind::Ref => ColumnValidator::Ref {
                sheet: constraints.sheet.ok_or_else(|| {
                    malformed(&child(position, SHEET_KEY), "`ref` requires `sheet`")
                })?,
            },
            TypeKind::Attachment => ColumnValidator::Attachment,
            TypeKind::Object => {
                let fields_key = child(position, FIELDS_KEY);
                let mut fields = Vec::with_capacity(constraints.fields.len());
                for (index, field) in constraints.fields.iter().enumerate() {
                    let field_position = indexed(&fields_key, index);
                    let type_position = child(&field_position, TYPE_KEY);
                    let validator = match self.validator(&field.ty, &type_position)? {
                        Built::Validator(validator) => validator,
                        // 入れ子の内側が使用不能なら、その列は値の全体を判定できない
                        // （使用不能の単位は要件 11.7 の「列」である）。
                        Built::Unusable(kind) => return Ok(Built::Unusable(kind)),
                    };
                    // 入れ子のフィールドの既定値も、型が解決されて初めて判定できる分は
                    // ここで検査する（要件 4.8）。不適合はそのフィールドの宣言上の位置で
                    // 報告する（codec が入れ子のフィールドに使うのと同じ形）。
                    if let Some(value) = &field.default {
                        if !self.conforms(value, &field.ty, field.required, &validator)? {
                            return Err(SchemaError::InvalidDefault {
                                column: field_position,
                            });
                        }
                    }
                    fields.push(FieldValidator::new(
                        field.name.clone(),
                        field.required,
                        validator,
                    ));
                }
                ColumnValidator::Object {
                    fields: fields.into_boxed_slice(),
                }
            }
            TypeKind::Array => {
                let item_position = child(position, ITEMS_KEY);
                let item_type = constraints
                    .items
                    .as_deref()
                    .ok_or_else(|| malformed(&item_position, "`array` requires `items`"))?;
                let items = match self.validator(item_type, &item_position)? {
                    Built::Validator(items) => items,
                    Built::Unusable(kind) => return Ok(Built::Unusable(kind)),
                };
                ColumnValidator::Array {
                    items: Box::new(items),
                    min_items: constraints.min_items,
                    max_items: constraints.max_items,
                }
            }
            TypeKind::Any => ColumnValidator::Any,
            TypeKind::Custom => {
                let id = constraints.custom_type.as_deref().ok_or_else(|| {
                    malformed(&child(position, TYPE_KEY), "`custom` requires `type`")
                })?;
                // 未登録の拡張型はその列だけを使用不能にする（要件 11.7）。登録されれば
                // 同じ宣言が解決しうるため、スキーマは破棄しない。識別子は報告へ運ぶ。
                let Some(implementation) = self.registry.resolve(id) else {
                    return Ok(Built::Unusable(id.into()));
                };
                ColumnValidator::Custom {
                    id: CustomTypeId::new(id),
                    imp: implementation,
                }
            }
        };
        Ok(Built::Validator(validator))
    }

    /// 書式のパターンをコンパイルする（design.md「Compile Layer / SchemaCompiler」）。
    ///
    /// **同じ生文字列は 1 回だけコンパイルする**（[`Compiler::patterns`]）。型定義は複数の
    /// 列から参照されうるため、参照のたびに `regex` を組み立てると「コンパイルはここで
    /// 一度だけ」が参照の数だけ繰り返される。拒否したパターンはキャッシュしない —
    /// 誤りは位置つきでそのまま呼び出し元へ伝わる。
    fn pattern(&mut self, source: &str, position: &str) -> Result<TextPattern, SchemaError> {
        if let Some(compiled) = self.patterns.get(source) {
            return Ok(compiled.clone());
        }
        let compiled = TextPattern::compile(source, position)?;
        self.patterns.insert(source.into(), compiled.clone());
        Ok(compiled)
    }

    /// 既定値が型と制約に適合するか（要件 4.8）。
    ///
    /// **規則は検証器が所有する**（[`ColumnValidator::check`]）。本関数が足すのは入れ子
    /// （オブジェクトと配列）を降りる構造だけである。既定値の入れ子の適合規則は
    /// `declaration::codec` の `object_conforms` / `array_conforms` と同じ意味であり、
    /// 「宣言に無い成員は適合しない」「必須で既定値も持たないフィールドの欠落は適合しない」
    /// を保つ（宣言の往復と矛盾しないため）。
    fn conforms(
        &self,
        value: &CellValue,
        ty: &TypeDecl,
        required: bool,
        validator: &ColumnValidator,
    ) -> Result<bool, SchemaError> {
        // 値なしは型の側では適合であり、必須の指定に反するときだけ適合しない
        // （design.md「組込型カタログと `CellValue` への写像」）。
        if matches!(value, CellValue::Null) {
            return Ok(!required);
        }
        // 入れ子の構造の判定には宣言の制約（フィールドの名前・必須・既定値）が要る。
        // 検証器は参照を解決済みであるため、宣言側も 1 段だけ解決して同じ形にする。
        let resolved = self.resolver.follow(ty).unwrap_or(ty);
        match validator {
            ColumnValidator::Object { fields } => {
                let (
                    CellValue::Nested(NestedValue::Object(entries)),
                    TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Object),
                        constraints,
                    },
                ) = (value, resolved)
                else {
                    return Ok(false);
                };
                for (name, member) in entries {
                    let Some(index) = constraints
                        .fields
                        .iter()
                        .position(|field| field.name.as_ref() == name.as_str())
                    else {
                        return Ok(false);
                    };
                    let field = &constraints.fields[index];
                    if !self.conforms(
                        member,
                        &field.ty,
                        field.required,
                        &fields[index].validator(),
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
            ColumnValidator::Array {
                items,
                min_items,
                max_items,
            } => {
                let CellValue::Nested(NestedValue::Array(members)) = value else {
                    return Ok(false);
                };
                if min_items.is_some_and(|min| members.len() < min)
                    || max_items.is_some_and(|max| members.len() > max)
                {
                    return Ok(false);
                }
                let TypeDecl::Kind {
                    kind: DeclaredKind::Known(TypeKind::Array),
                    constraints,
                } = resolved
                else {
                    return Ok(false);
                };
                let Some(item_type) = constraints.items.as_deref() else {
                    return Ok(true);
                };
                for member in members {
                    if !self.conforms(member, item_type, false, items)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            // 参照の既定値は行識別子でなければならない（`declaration::codec` の
            // `ref_conforms` と同じ規則）。参照先の実在は一括判定（タスク 5.3）が受け持つ。
            ColumnValidator::Ref { .. } => {
                Ok(matches!(value, CellValue::Text(text) if RowId::from_str(text).is_ok()))
            }
            // 残りは検証器の判定そのものである（組込種別の値の規則と、拡張型の実装の判定）。
            _ => Ok(validator.check(value) == ColumnVerdict::Conforming),
        }
    }
}

/// 宣言された範囲の端点を、その型の比較用の形へ取り出す。
///
/// 端点の変種は `declaration::codec` が種別ごとに検査済みである（その型の wire 変種に
/// 厳密一致させる。tasks.md 3.2）。ここで取り出せないのは内部の食い違いであり、黙って
/// 端点を落とすと範囲制約が消えるため、宣言の誤りとして報告する。
fn endpoint<T>(
    declared: Option<&CellValue>,
    position: &str,
    extract: impl FnOnce(&CellValue) -> Option<T>,
) -> Result<Option<T>, SchemaError> {
    match declared {
        None => Ok(None),
        Some(value) => extract(value).map(Some).ok_or_else(|| {
            malformed(
                position,
                "range endpoint must be a value of the declared type",
            )
        }),
    }
}

/// 日時の範囲の端点を、その形の解釈済みの値へ取り出す。
fn temporal_endpoint(
    declared: Option<&CellValue>,
    position: &str,
    form: TemporalForm,
) -> Result<Option<TemporalValue>, SchemaError> {
    endpoint(declared, position, |value| match value {
        CellValue::Text(text) => form.parse(text),
        _ => None,
    })
}

/// 宣言の誤りを、位置と理由つきで組み立てる（要件 1.7）。
fn malformed(position: &str, reason: &str) -> SchemaError {
    SchemaError::MalformedDeclaration {
        position: position.to_owned(),
        reason: reason.to_owned(),
    }
}

/// 有限の検証器へ展開できない型定義（要件 11.7）。
///
/// # 何を展開できないか
///
/// 検証器は入れ子の検証器を**値として内包する**（[`plan::ColumnValidator::Object`] の
/// `fields` と [`plan::ColumnValidator::Array`] の `items`）ため、展開は値の再帰と同じ形を
/// 持たない。したがって**すべての参照辺**（`required: false` のフィールドの辺も、配列の
/// `items` の辺も、裸の参照の辺も数える）に循環がある型定義は、展開すると無限になる。
/// これは [`resolve`] の循環検出とは**別物**である — あちらは「値が有限の大きさで存在し
/// えない循環」を探すために**必須かつ非配列**の辺だけを見る（要件 3.6）。`required: false`
/// や配列を経由する再帰は正当な宣言であり（design.md「Compile Layer / SchemaCompiler」）、
/// 宣言ごと拒否してはならない。
///
/// 循環に含まれる定義を**推移的に参照する**定義も展開できない。`Tree` が再帰するとき、
/// `{ "tree": Tree }` という定義は `Tree` を展開しないと自身の検証器を組めないためである。
///
/// # 求め方（出次数 0 の除去）
///
/// 辺 `A → B` を「`A` の本文が `B` を参照する」とする。出次数 0 の頂点（他の定義を参照
/// しない定義）を繰り返し取り除くと、残る頂点はちょうど「循環へ到達できる頂点」である —
/// 有限の大きさの展開を持つ頂点は必ずこの除去で消える。推移的な参照も同時に拾える。
/// 定義の並びに依らず結果が一意であり、計算量は定義数と辺数の和に比例する。
fn unexpandable_definitions(definitions: &[TypeDefinition]) -> BTreeSet<TypeDefId> {
    // 参照の重複は辺の数え上げを狂わせるため畳む。識別子の重複は上流が検査し、`resolve` も
    // 最初の出現を採る（`compile::resolve` の `collect` と同じ扱い）。
    let mut bodies: BTreeMap<TypeDefId, &TypeDecl> = BTreeMap::new();
    for definition in definitions {
        bodies
            .entry(definition.id)
            .or_insert(&definition.definition);
    }
    let mut edges: BTreeMap<TypeDefId, BTreeSet<TypeDefId>> = BTreeMap::new();
    for (id, body) in &bodies {
        let mut refs = BTreeSet::new();
        collect_refs(body, &mut refs);
        edges.insert(*id, refs);
    }
    // 逆辺（`B` を参照している定義の一覧）。出次数 0 の除去で前の頂点を起こすために要る。
    let mut reverse: BTreeMap<TypeDefId, Vec<TypeDefId>> = BTreeMap::new();
    for (id, refs) in &edges {
        for target in refs {
            reverse.entry(*target).or_default().push(*id);
        }
    }

    let mut remaining: BTreeSet<TypeDefId> = edges.keys().copied().collect();
    let mut queue: Vec<TypeDefId> = edges
        .iter()
        .filter(|(_, refs)| refs.is_empty())
        .map(|(id, _)| *id)
        .collect();
    while let Some(id) = queue.pop() {
        remaining.remove(&id);
        for source in reverse.get(&id).into_iter().flatten() {
            // 参照先が取り除かれたので、この辺はもう「残る定義」へ向かっていない。
            if let Some(out) = edges.get_mut(source) {
                out.remove(&id);
                if out.is_empty() && remaining.contains(source) {
                    queue.push(*source);
                }
            }
        }
    }
    remaining
}

/// 型の本文が参照する型定義の識別子をすべて集める（入れ子のフィールドと配列の要素を降りる）。
///
/// **すべての参照辺を数える** — `required` の有無も配列かどうかも見ない
/// （[`unexpandable_definitions`] の docs）。
fn collect_refs(ty: &TypeDecl, out: &mut BTreeSet<TypeDefId>) {
    match ty {
        TypeDecl::Ref(id) => {
            out.insert(*id);
        }
        TypeDecl::Kind { constraints, .. } => {
            for field in &constraints.fields {
                collect_refs(&field.ty, out);
            }
            if let Some(items) = constraints.items.as_deref() {
                collect_refs(items, out);
            }
        }
    }
}

/// 型定義の本文ペイロードの宣言上の位置（`compile::resolve` と同じ文法）。
fn definition_position(id: TypeDefId) -> String {
    format!("type {id}.{DEFINITION_KEY}")
}

/// 宣言上の位置にキーを 1 段足す（`declaration::codec` と同じ文法）。
fn child(position: &str, key: &str) -> String {
    format!("{position}.{key}")
}

/// 宣言上の位置に添字を 1 段足す。
fn indexed(position: &str, index: usize) -> String {
    format!("{position}[{index}]")
}

#[cfg(test)]
mod tests {
    use super::plan::{ColumnValidator, ColumnVerdict};
    use super::{compile, compile_declaration, plan, ColumnIndex};
    use crate::declaration::codec::{schema_to_text, type_definition_to_text};
    use crate::declaration::{
        ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl, TypeDefinition,
    };
    use crate::error::{PatternLimit, SchemaError};
    use crate::registry::{
        CustomType, CustomTypeFailure, CustomTypeId, CustomVerdict, TypeRegistry,
    };
    use crate::types::datetime::OffsetPolicy;
    use crate::types::decimal::DecimalDigits;
    use crate::types::text::SOURCE_LENGTH_LIMIT;
    use crate::types::TypeKind;
    use document_format::{CellValue, Document, SchemaPart, TypeDefId};
    use std::str::FromStr;
    use std::sync::Arc;

    /// 標本のシート識別子（ULID の正準テキスト形）。
    const SHEET: &str = "01K4ANRRG004HMASW9NF6YY091";
    /// 標本の型定義識別子（ULID の正準テキスト形）。
    const DEF: &str = "01K4ANRRG004HMASW9NF6YY092";
    /// 再帰する型定義の識別子（標本）。
    const TREE: &str = "01K4ANRRG004HMASW9NF6YY093";
    /// 再帰する型定義を推移的に参照する型定義の識別子（標本）。
    const HOLDER: &str = "01K4ANRRG004HMASW9NF6YY094";
    /// 配列の要素を経由して自分自身を参照する型定義の識別子（標本）。
    const LIST: &str = "01K4ANRRG004HMASW9NF6YY095";
    /// 標本の拡張型の識別子。
    const CUSTOM_ID: &str = "postal-code";

    /// 標本の型定義識別子。
    fn def_id() -> TypeDefId {
        TypeDefId::from_str(DEF).expect("標本の型定義識別子が解析できない")
    }

    /// 標本のシート識別子。
    fn sheet_id() -> document_format::SheetId {
        document_format::SheetId::from_str(SHEET).expect("標本のシート識別子が解析できない")
    }

    /// 種別による指定の型を組み立てる。
    fn declared(kind: TypeKind, constraints: Constraints) -> TypeDecl {
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

    /// ルートスキーマを組み立てる。
    fn schema(columns: Vec<ColumnDecl>) -> Schema {
        Schema { columns }
    }

    /// 型定義 1 件を組み立てる。
    fn definition(id: TypeDefId, body: TypeDecl) -> TypeDefinition {
        TypeDefinition {
            id,
            definition: body,
        }
    }

    /// 標本の拡張型: `〒` で始まるテキストだけを受理する。
    struct PostalCode {
        id: CustomTypeId,
    }

    impl CustomType for PostalCode {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            match value {
                CellValue::Text(text) if text.starts_with('〒') => Ok(CustomVerdict::Accepted),
                _ => Ok(CustomVerdict::rejected("〒 で始まらない")),
            }
        }
    }

    /// 標本の拡張型を登録した辞書。
    fn registry_with_postal_code() -> TypeRegistry {
        let mut registry = TypeRegistry::new();
        registry
            .register(Arc::new(PostalCode {
                id: CustomTypeId::new(CUSTOM_ID),
            }))
            .expect("標本の拡張型は登録できる");
        registry
    }

    /// 設計の型カタログ表の各種別を、その種別で意味を持つパラメータつきで組み立てる。
    fn catalog_constraints(kind: TypeKind) -> Constraints {
        match kind {
            TypeKind::Int => Constraints {
                min: Some(CellValue::Int(0)),
                ..Constraints::default()
            },
            TypeKind::Float => Constraints {
                min: Some(CellValue::Float(0.0)),
                ..Constraints::default()
            },
            TypeKind::Decimal => Constraints {
                digits: Some(DecimalDigits::new(12, 2).expect("12 >= 1 かつ 2 <= 12")),
                ..Constraints::default()
            },
            TypeKind::Text => Constraints {
                max_length: Some(32),
                pattern: Some("^[a-z]+$".into()),
                ..Constraints::default()
            },
            TypeKind::Bool | TypeKind::Attachment | TypeKind::Any => Constraints::default(),
            TypeKind::Date => Constraints {
                min: Some(CellValue::Text("2026-01-01".into())),
                ..Constraints::default()
            },
            TypeKind::DateTime => Constraints {
                offset: Some(OffsetPolicy::Required),
                ..Constraints::default()
            },
            TypeKind::Enum => Constraints {
                choices: vec!["赤".into(), "青".into()],
                ..Constraints::default()
            },
            TypeKind::Ref => Constraints {
                sheet: Some(sheet_id()),
                ..Constraints::default()
            },
            TypeKind::Object => Constraints {
                fields: vec![FieldDecl {
                    name: "色".into(),
                    ty: declared(
                        TypeKind::Enum,
                        Constraints {
                            choices: vec!["赤".into()],
                            ..Constraints::default()
                        },
                    ),
                    required: true,
                    default: None,
                    description: None,
                }],
                ..Constraints::default()
            },
            TypeKind::Array => Constraints {
                items: Some(Box::new(declared(TypeKind::Int, Constraints::default()))),
                min_items: Some(1),
                ..Constraints::default()
            },
            TypeKind::Custom => Constraints {
                custom_type: Some(CUSTOM_ID.into()),
                ..Constraints::default()
            },
        }
    }

    /// 検証器の変種を設計表の種別へ写す（**ワイルドカードの無い網羅マッチ**）。
    fn validator_kind(validator: &ColumnValidator) -> TypeKind {
        match validator {
            ColumnValidator::Int { .. } => TypeKind::Int,
            ColumnValidator::Float { .. } => TypeKind::Float,
            ColumnValidator::Decimal { .. } => TypeKind::Decimal,
            ColumnValidator::Text { .. } => TypeKind::Text,
            ColumnValidator::Bool => TypeKind::Bool,
            ColumnValidator::Date { .. } => TypeKind::Date,
            ColumnValidator::DateTime { .. } => TypeKind::DateTime,
            ColumnValidator::Enum { .. } => TypeKind::Enum,
            ColumnValidator::Ref { .. } => TypeKind::Ref,
            ColumnValidator::Attachment => TypeKind::Attachment,
            ColumnValidator::Object { .. } => TypeKind::Object,
            ColumnValidator::Array { .. } => TypeKind::Array,
            ColumnValidator::Any => TypeKind::Any,
            ColumnValidator::Custom { .. } => TypeKind::Custom,
        }
    }

    /// 空でない列名の並びを供給する（要件 1.1, 1.2）。
    #[test]
    fn column_names_are_supplied_in_declaration_order() {
        let root = schema(vec![
            column("数量", declared(TypeKind::Int, Constraints::default())),
            column("品番", declared(TypeKind::Text, Constraints::default())),
            column("備考", declared(TypeKind::Any, Constraints::default())),
        ]);
        let compiled =
            compile_declaration(&root, &[], &TypeRegistry::new()).expect("標本は計画へ落ちる");

        assert_eq!("数量", compiled.columns()[0]);
        assert_eq!("品番", compiled.columns()[1]);
        assert_eq!("備考", compiled.columns()[2]);
        assert_eq!(3, compiled.column_count());
    }

    /// 列名の配列と検証器の配列が常に同じ長さで同じ添字である（design.md「Data Models /
    /// Domain Model」の不変条件。tasks.md 4.4）。
    #[test]
    fn the_column_name_array_and_the_validator_array_share_length_and_index() {
        let kinds = [
            TypeKind::Int,
            TypeKind::Text,
            TypeKind::Bool,
            TypeKind::Date,
            TypeKind::Array,
            TypeKind::Object,
            TypeKind::Any,
        ];
        let columns: Vec<ColumnDecl> = kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| {
                let name = format!("c{index:02}");
                column(&name, declared(*kind, catalog_constraints(*kind)))
            })
            .collect();
        let compiled = compile_declaration(&schema(columns), &[], &registry_with_postal_code())
            .expect("標本は計画へ落ちる");

        assert_eq!(kinds.len(), compiled.columns().len());
        assert_eq!(compiled.columns().len(), compiled.validators().len());
        for (index, expected) in kinds.iter().enumerate() {
            let column = ColumnIndex::new(index);
            let validator = compiled
                .validator(column)
                .expect("使用可能な列は検証器を持つ");
            assert_eq!(
                *expected,
                validator_kind(validator),
                "添字 {index} の検証器"
            );
            assert!(!compiled.is_unusable(column));
        }
    }

    /// 設計表に挙がる全 14 種別が、それぞれの検証器へ落ちる（design.md「組込型カタログと
    /// `CellValue` への写像」）。
    #[test]
    fn every_catalog_kind_is_dropped_into_its_validator() {
        let columns: Vec<ColumnDecl> = TypeKind::ALL
            .iter()
            .enumerate()
            .map(|(index, kind)| {
                let name = format!("c{index:02}");
                column(&name, declared(*kind, catalog_constraints(*kind)))
            })
            .collect();
        let compiled = compile_declaration(&schema(columns), &[], &registry_with_postal_code())
            .expect("標本は計画へ落ちる");

        for (index, expected) in TypeKind::ALL.iter().enumerate() {
            let validator = compiled
                .validator(ColumnIndex::new(index))
                .expect("全種別が使用可能に落ちる");
            assert_eq!(
                *expected,
                validator_kind(validator),
                "添字 {index} の検証器"
            );
        }
        assert!(compiled.unusable_columns().is_empty());
    }

    /// 列を 1 本も宣言していないルートスキーマを、列 0 本のシートとして受理する
    /// （要件 1.8。新規シートの初期状態）。
    #[test]
    fn an_empty_root_schema_is_accepted_as_a_sheet_with_no_columns() {
        let compiled = compile_declaration(&Schema::default(), &[], &TypeRegistry::new())
            .expect("空の宣言はエラーではない");
        assert_eq!(0, compiled.column_count());
        assert!(compiled.columns().is_empty());
        assert!(compiled.validators().is_empty());
        assert!(compiled.unusable_columns().is_empty());
    }

    /// 未知の組込種別はその列だけを使用不能にし、コンパイルは成功する（要件 11.7）。
    #[test]
    fn an_unknown_kind_leaves_only_its_own_column_unusable() {
        let unknown = ColumnDecl {
            name: "将来の型".into(),
            ty: TypeDecl::Kind {
                kind: DeclaredKind::Unknown("geo-point".into()),
                constraints: Constraints::default(),
            },
            required: false,
            unique: true,
            default: None,
            description: None,
        };
        let root = schema(vec![
            unknown,
            column("数", declared(TypeKind::Int, Constraints::default())),
        ]);
        let compiled = compile_declaration(&root, &[], &TypeRegistry::new())
            .expect("使用不能な列があってもコンパイルは成功する");

        assert_eq!(2, compiled.column_count());
        assert_eq!(2, compiled.validators().len());
        assert_eq!(&[ColumnIndex::new(0)], compiled.unusable_columns());
        assert_eq!(
            Some("geo-point"),
            compiled.unusable_kind(ColumnIndex::new(0)),
            "報告に要る未知の種別のトークンが計画に無い（要件 11.7）"
        );
        assert_eq!(None, compiled.unusable_kind(ColumnIndex::new(1)));
        assert!(compiled.validator(ColumnIndex::new(0)).is_none());
        assert!(compiled.is_unusable(ColumnIndex::new(0)));
        assert!(!compiled.is_unusable(ColumnIndex::new(1)));
        assert_eq!(
            TypeKind::Int,
            validator_kind(
                compiled
                    .validator(ColumnIndex::new(1))
                    .expect("使用可能な列は検証器を持つ")
            )
        );
        assert!(
            compiled.unique_columns().is_empty(),
            "使用不能な列は一意制約の対象にならない（正準化の手段が無い）"
        );
    }

    /// 未登録の拡張型はその列だけを使用不能にし、登録済みなら使用可能になる（要件 11.7）。
    #[test]
    fn an_unregistered_custom_type_is_unusable_until_it_is_registered() {
        let root = schema(vec![column(
            "郵便番号",
            declared(
                TypeKind::Custom,
                Constraints {
                    custom_type: Some(CUSTOM_ID.into()),
                    ..Constraints::default()
                },
            ),
        )]);

        let unregistered = compile_declaration(&root, &[], &TypeRegistry::new())
            .expect("未登録でもスキーマを破棄しない");
        assert_eq!(&[ColumnIndex::new(0)], unregistered.unusable_columns());
        assert_eq!(
            Some(CUSTOM_ID),
            unregistered.unusable_kind(ColumnIndex::new(0)),
            "報告に要る拡張型の識別子が計画に無い（要件 11.7）"
        );

        let registered =
            compile_declaration(&root, &[], &registry_with_postal_code()).expect("登録済みは通る");
        assert!(registered.unusable_columns().is_empty());
        assert_eq!(
            TypeKind::Custom,
            validator_kind(
                registered
                    .validator(ColumnIndex::new(0))
                    .expect("登録済みの拡張型は検証器を持つ")
            )
        );
    }

    /// 書式のパターンはコンパイル時に一度だけコンパイルされ、上限を超える宣言は行を
    /// 1 つも見ないうちに拒否される（design.md「Compile Layer / SchemaCompiler」）。
    #[test]
    fn a_pattern_is_compiled_at_compile_time_not_during_scanning() {
        let too_long = "a".repeat(SOURCE_LENGTH_LIMIT + 1);
        let root = schema(vec![column(
            "書式",
            declared(
                TypeKind::Text,
                Constraints {
                    pattern: Some(too_long.into()),
                    ..Constraints::default()
                },
            ),
        )]);
        match compile_declaration(&root, &[], &TypeRegistry::new()) {
            Err(SchemaError::PatternLimitExceeded {
                position,
                limit,
                max,
                ..
            }) => {
                assert_eq!("columns[0].type.pattern", position);
                assert_eq!(PatternLimit::SourceLength, limit);
                assert_eq!(SOURCE_LENGTH_LIMIT, max);
            }
            other => panic!("上限を超えるパターンが拒否されない: {other:?}"),
        }

        // 有効なパターンは計画に載り、走査の内側では再コンパイルされない。
        let root = schema(vec![column(
            "書式",
            declared(
                TypeKind::Text,
                Constraints {
                    pattern: Some("^[0-9]+$".into()),
                    ..Constraints::default()
                },
            ),
        )]);
        let compiled = compile_declaration(&root, &[], &TypeRegistry::new())
            .expect("有効なパターンはコンパイルできる");
        let validator = compiled
            .validator(ColumnIndex::new(0))
            .expect("使用可能な列は検証器を持つ");
        assert_eq!(
            ColumnVerdict::Conforming,
            validator.check(&CellValue::Text("123".into()))
        );
        assert!(matches!(
            validator.check(&CellValue::Text("abc".into())),
            ColumnVerdict::Violating(_)
        ));
    }

    /// 型が解決されて初めて判定できる既定値（識別子参照の先と拡張型）をここで検査する
    /// （要件 4.8。tasks.md 3.2 の分担）。
    #[test]
    fn defaults_that_need_a_resolved_type_are_checked_here() {
        let definitions = vec![definition(
            def_id(),
            declared(
                TypeKind::Int,
                Constraints {
                    min: Some(CellValue::Int(0)),
                    ..Constraints::default()
                },
            ),
        )];

        // `$ref` の先が `int` の下限 0 であるため、既定値 -1 は適合しない。
        let mut declared_column = column("数量", TypeDecl::Ref(def_id()));
        declared_column.default = Some(CellValue::Int(-1));
        match compile_declaration(
            &schema(vec![declared_column]),
            &definitions,
            &TypeRegistry::new(),
        ) {
            Err(SchemaError::InvalidDefault { column }) => assert_eq!("数量", column),
            other => panic!("参照先の型に合わない既定値が拒否されない: {other:?}"),
        }

        // 適合する既定値は通る。
        let mut declared_column = column("数量", TypeDecl::Ref(def_id()));
        declared_column.default = Some(CellValue::Int(5));
        assert!(compile_declaration(
            &schema(vec![declared_column]),
            &definitions,
            &TypeRegistry::new()
        )
        .is_ok());

        // 拡張型の既定値は、登録された実装の判定で検査する。
        let mut rejected = column(
            "郵便番号",
            declared(
                TypeKind::Custom,
                Constraints {
                    custom_type: Some(CUSTOM_ID.into()),
                    ..Constraints::default()
                },
            ),
        );
        rejected.default = Some(CellValue::Text("100-0001".into()));
        match compile_declaration(&schema(vec![rejected]), &[], &registry_with_postal_code()) {
            Err(SchemaError::InvalidDefault { column }) => assert_eq!("郵便番号", column),
            other => panic!("拡張型が拒否する既定値が通った: {other:?}"),
        }

        let mut accepted = column(
            "郵便番号",
            declared(
                TypeKind::Custom,
                Constraints {
                    custom_type: Some(CUSTOM_ID.into()),
                    ..Constraints::default()
                },
            ),
        );
        accepted.default = Some(CellValue::Text("〒100-0001".into()));
        assert!(
            compile_declaration(&schema(vec![accepted]), &[], &registry_with_postal_code()).is_ok()
        );
    }

    /// 計画が一意制約を列添字で供給する（5.2 の入力。design.md「Compile Layer /
    /// SchemaCompiler」の State Management）。
    #[test]
    fn unique_columns_are_supplied_by_column_index() {
        let mut supplier = column(
            "仕入先",
            declared(
                TypeKind::Ref,
                Constraints {
                    sheet: Some(sheet_id()),
                    ..Constraints::default()
                },
            ),
        );
        supplier.unique = true;
        let root = schema(vec![
            column("数", declared(TypeKind::Int, Constraints::default())),
            supplier,
        ]);
        let compiled =
            compile_declaration(&root, &[], &TypeRegistry::new()).expect("標本は計画へ落ちる");

        assert_eq!(&[ColumnIndex::new(1)], compiled.unique_columns());
    }

    /// シートのルートスキーマと型定義から計画を組み立てる（要件 1.1。列の並び順の決定は
    /// 宣言の列の並びそのものである）。
    #[test]
    fn the_plan_is_built_from_the_sheets_root_schema_and_type_definitions() {
        let quantity = definition(
            def_id(),
            declared(
                TypeKind::Decimal,
                Constraints {
                    digits: Some(DecimalDigits::new(12, 2).expect("12 >= 1 かつ 2 <= 12")),
                    ..Constraints::default()
                },
            ),
        );
        let root = schema(vec![
            ColumnDecl {
                name: "数量".into(),
                ty: TypeDecl::Ref(def_id()),
                required: true,
                unique: false,
                default: Some(CellValue::Decimal("0.00".into())),
                description: Some("入荷数量".into()),
            },
            column("備考", declared(TypeKind::Text, Constraints::default())),
        ]);
        let root_text = schema_to_text(&root).expect("標本のルートは正準形を持つ");
        let definition_text =
            type_definition_to_text(&quantity.definition).expect("標本の型定義は正準形を持つ");
        let envelope = format!(
            r#"{{"root":{root_text},"types":[{{"id":"{DEF}","definition":{definition_text}}}]}}"#
        );

        let mut document = Document::new();
        let sheet = document.add_sheet("標本");
        document
            .set_sheet_columns(sheet, vec!["数量".to_owned(), "備考".to_owned()])
            .expect("標本のシートは実在する");
        document
            .set_root_schema(
                sheet,
                SchemaPart::parse(&envelope).expect("標本は妥当なエンベロープ"),
            )
            .expect("標本のシートは実在する");

        let compiled = compile(&document.sheets()[0], &TypeRegistry::new())
            .expect("シートのルートスキーマから計画が組み立てられる");

        assert_eq!(2, compiled.column_count());
        assert_eq!("数量", compiled.columns()[0]);
        assert_eq!(
            TypeKind::Decimal,
            validator_kind(
                compiled
                    .validator(ColumnIndex::new(0))
                    .expect("参照先の型が解決されている")
            )
        );
        assert!(compiled.required(ColumnIndex::new(0)));
        assert!(!compiled.required(ColumnIndex::new(1)));
        assert_eq!(
            Some(&CellValue::Decimal("0.00".into())),
            compiled.default_value(ColumnIndex::new(0))
        );
        assert_eq!(None, compiled.default_value(ColumnIndex::new(1)));
    }

    /// 同じ型定義を複数の列から参照しても、書式の制約はどの列にも効く（パターンは
    /// コンパイル時に一度だけ組み立てられ、参照のたびには組み立てられない）。
    #[test]
    fn a_shared_type_definition_pattern_applies_to_every_referencing_column() {
        let definitions = vec![definition(
            def_id(),
            declared(
                TypeKind::Text,
                Constraints {
                    pattern: Some("^[0-9]+$".into()),
                    ..Constraints::default()
                },
            ),
        )];
        let root = schema(vec![
            column("左", TypeDecl::Ref(def_id())),
            column("右", TypeDecl::Ref(def_id())),
        ]);
        let compiled = compile_declaration(&root, &definitions, &TypeRegistry::new())
            .expect("同じ型定義を 2 列から参照できる");

        assert!(compiled.unusable_columns().is_empty());
        for index in 0..2 {
            let validator = compiled
                .validator(ColumnIndex::new(index))
                .expect("参照先の型が解決されている");
            assert_eq!(
                ColumnVerdict::Conforming,
                validator.check(&CellValue::Text("123".into()))
            );
            assert!(matches!(
                validator.check(&CellValue::Text("abc".into())),
                ColumnVerdict::Violating(_)
            ));
        }
    }

    /// 参照の連鎖（`A = $ref B`）を辿って種別に着地する。
    #[test]
    fn a_chain_of_type_references_is_followed_to_its_kind() {
        let inner = definition(def_id(), declared(TypeKind::Bool, Constraints::default()));
        let outer_id = TypeDefId::from_str("01K4ANRRG004HMASW9NF6YY093").expect("識別子");
        let outer = definition(outer_id, TypeDecl::Ref(def_id()));
        let root = schema(vec![column("真偽", TypeDecl::Ref(outer_id))]);
        let compiled = compile_declaration(&root, &[outer, inner], &TypeRegistry::new())
            .expect("連鎖は解決できる");
        assert_eq!(
            TypeKind::Bool,
            validator_kind(
                compiled
                    .validator(ColumnIndex::new(0))
                    .expect("連鎖の先の型が落ちる")
            )
        );
    }

    /// 再帰する型定義は有限の検証器へ展開できないため、その列だけを使用不能に落とす
    /// （スキーマは破棄しない。モジュール docs「再帰する型定義」）。
    #[test]
    fn a_recursive_type_definition_leaves_only_its_column_unusable() {
        // `Node = object { next: ($ref Node, 任意) }` は `resolve` が正当な宣言として通す。
        let node_id = def_id();
        let node = definition(
            node_id,
            declared(
                TypeKind::Object,
                Constraints {
                    fields: vec![FieldDecl {
                        name: "next".into(),
                        ty: TypeDecl::Ref(node_id),
                        required: false,
                        default: None,
                        description: None,
                    }],
                    ..Constraints::default()
                },
            ),
        );
        let root = schema(vec![
            column("節", TypeDecl::Ref(node_id)),
            column("数", declared(TypeKind::Int, Constraints::default())),
        ]);
        let compiled = compile_declaration(&root, &[node], &TypeRegistry::new())
            .expect("再帰する型定義でもコンパイルは成功する");

        assert_eq!(&[ColumnIndex::new(0)], compiled.unusable_columns());
        assert_eq!(
            Some(DEF),
            compiled.unusable_kind(ColumnIndex::new(0)),
            "再入した型定義の識別子が報告へ運ばれていない"
        );
        assert_eq!(
            TypeKind::Int,
            validator_kind(
                compiled
                    .validator(ColumnIndex::new(1))
                    .expect("再帰しない列は正しく落ちる")
            )
        );
    }

    /// 桁数を宣言しない `decimal` は正当な宣言であり（要件 2.3 は桁を「宣言できるように
    /// する」であり必須ではない）、桁の判定を行わず範囲だけを判定する。
    #[test]
    fn a_decimal_without_declared_digits_stays_usable_and_checks_only_the_range() {
        let mut price = column(
            "単価",
            declared(
                TypeKind::Decimal,
                Constraints {
                    min: Some(CellValue::Decimal("0".into())),
                    ..Constraints::default()
                },
            ),
        );
        price.default = Some(CellValue::Decimal("12.345".into()));
        let compiled = compile_declaration(&schema(vec![price]), &[], &TypeRegistry::new())
            .expect("桁数の無い 10 進数は正当な宣言である");

        assert!(compiled.unusable_columns().is_empty());
        let validator = compiled
            .validator(ColumnIndex::new(0))
            .expect("使用可能な列は検証器を持つ");
        // 桁の上限が無いため、長い小数部も有効桁の多い値も適合する。
        assert_eq!(
            ColumnVerdict::Conforming,
            validator.check(&CellValue::Decimal("12.345".into()))
        );
        assert_eq!(
            ColumnVerdict::Conforming,
            validator.check(&CellValue::Decimal("1234567890123456789.99999".into()))
        );
        // 範囲は従来どおり判定する。
        assert_eq!(
            ColumnVerdict::Violating(plan::ColumnViolation::OutOfRange {
                min: Some(CellValue::Decimal("0".into())),
                max: None,
            }),
            validator.check(&CellValue::Decimal("-1".into()))
        );
        // 文法に一致しない `Decimal`（上流の脱出口）は 10 進数の値として解釈できない。
        // 桁の宣言が無いため `PrecisionExceeded` は発生し得ず、型の不一致に落ちる。
        assert_eq!(
            ColumnVerdict::Violating(plan::ColumnViolation::TypeMismatch {
                kind: TypeKind::Decimal,
            }),
            validator.check(&CellValue::Decimal("abc".into()))
        );
    }

    /// 検証器が有限に展開できない型定義（すべての参照辺に循環がある定義と、それを
    /// **推移的に**参照する定義）は、到達した時点でその列だけを使用不能にする。
    ///
    /// 判定に使うグラフは 4.3 の循環検出とは別物である — 4.3 は「必須かつ非配列」の辺だけで
    /// 値が有限の大きさで存在しえない循環を探すが、ここでは `required: false` の辺も配列の
    /// `items` の辺も数える（すべての参照辺）。
    #[test]
    fn every_definition_that_cannot_be_expanded_leaves_only_its_column_unusable() {
        let tree_id = TypeDefId::from_str("01K4ANRRG004HMASW9NF6YY093").expect("識別子");
        let holder_id = TypeDefId::from_str("01K4ANRRG004HMASW9NF6YY094").expect("識別子");
        let list_id = TypeDefId::from_str("01K4ANRRG004HMASW9NF6YY095").expect("識別子");

        // `Tree = object { next: ($ref Tree, 任意) }` は 4.3 が**正当な宣言**として通す
        // （要件 3.6 の裏。design.md「Compile Layer / SchemaCompiler」）。
        let tree = definition(
            tree_id,
            declared(
                TypeKind::Object,
                Constraints {
                    fields: vec![FieldDecl {
                        name: "next".into(),
                        ty: TypeDecl::Ref(tree_id),
                        required: false,
                        default: None,
                        description: None,
                    }],
                    ..Constraints::default()
                },
            ),
        );
        // 再帰する定義を参照するだけの**非再帰の**定義も展開できない（`Tree` を展開しないと
        // `Holder` の検証器が組めない）。
        let holder = definition(
            holder_id,
            declared(
                TypeKind::Object,
                Constraints {
                    fields: vec![FieldDecl {
                        name: "tree".into(),
                        ty: TypeDecl::Ref(tree_id),
                        required: true,
                        default: None,
                        description: None,
                    }],
                    ..Constraints::default()
                },
            ),
        );
        // 配列の `items` を経由する循環も展開できない。
        let list = definition(
            list_id,
            declared(
                TypeKind::Object,
                Constraints {
                    fields: vec![FieldDecl {
                        name: "children".into(),
                        ty: declared(
                            TypeKind::Array,
                            Constraints {
                                items: Some(Box::new(TypeDecl::Ref(list_id))),
                                ..Constraints::default()
                            },
                        ),
                        required: false,
                        default: None,
                        description: None,
                    }],
                    ..Constraints::default()
                },
            ),
        );

        let root = schema(vec![
            column("推移的", TypeDecl::Ref(holder_id)),
            column("循環", TypeDecl::Ref(tree_id)),
            column("配列経由", TypeDecl::Ref(list_id)),
            column(
                "入れ子経由",
                declared(
                    TypeKind::Object,
                    Constraints {
                        fields: vec![FieldDecl {
                            name: "木".into(),
                            ty: TypeDecl::Ref(tree_id),
                            required: false,
                            default: None,
                            description: None,
                        }],
                        ..Constraints::default()
                    },
                ),
            ),
            column("数", declared(TypeKind::Int, Constraints::default())),
        ]);
        let compiled = compile_declaration(&root, &[tree, holder, list], &TypeRegistry::new())
            .expect("展開できない定義があってもコンパイルは成功する");

        assert_eq!(
            &[
                ColumnIndex::new(0),
                ColumnIndex::new(1),
                ColumnIndex::new(2),
                ColumnIndex::new(3),
            ],
            compiled.unusable_columns()
        );
        assert_eq!(
            Some(HOLDER),
            compiled.unusable_kind(ColumnIndex::new(0)),
            "展開を断った型定義の識別子が報告へ運ばれていない"
        );
        assert_eq!(Some(TREE), compiled.unusable_kind(ColumnIndex::new(1)));
        assert_eq!(Some(LIST), compiled.unusable_kind(ColumnIndex::new(2)));
        assert_eq!(
            Some(TREE),
            compiled.unusable_kind(ColumnIndex::new(3)),
            "入れ子のフィールド経由でも、展開を断った型定義の識別子を運ぶ"
        );
        assert_eq!(
            TypeKind::Int,
            validator_kind(
                compiled
                    .validator(ColumnIndex::new(4))
                    .expect("展開できる列は正しく落ちる")
            )
        );
    }

    /// 入れ子の内側が解決できないときは、その列**全体**が使用不能になる（要件 11.7）。
    /// フィールドは検証器を値として内包するため、内側の 1 つが未解決なら値の全体を
    /// 判定できない。
    #[test]
    fn an_unresolvable_nested_field_leaves_the_whole_column_unusable() {
        let root = schema(vec![
            column(
                "入れ子",
                declared(
                    TypeKind::Object,
                    Constraints {
                        fields: vec![FieldDecl {
                            name: "郵便番号".into(),
                            ty: declared(
                                TypeKind::Custom,
                                Constraints {
                                    custom_type: Some("未登録".into()),
                                    ..Constraints::default()
                                },
                            ),
                            required: false,
                            default: None,
                            description: None,
                        }],
                        ..Constraints::default()
                    },
                ),
            ),
            column("数", declared(TypeKind::Int, Constraints::default())),
        ]);
        let compiled = compile_declaration(&root, &[], &TypeRegistry::new())
            .expect("内側が未解決でもコンパイルは成功する");

        assert_eq!(&[ColumnIndex::new(0)], compiled.unusable_columns());
        assert_eq!(
            Some("未登録"),
            compiled.unusable_kind(ColumnIndex::new(0)),
            "入れ子の内側の未登録の識別子が報告へ運ばれていない"
        );
        assert_eq!(
            TypeKind::Int,
            validator_kind(
                compiled
                    .validator(ColumnIndex::new(1))
                    .expect("入れ子を持たない列は正しく落ちる")
            )
        );
    }

    /// 入れ子のフィールドの既定値も、型が解決されて初めて判定できるものをここで検査し、
    /// 不適合はそのフィールドの宣言上の位置で報告する（要件 4.8）。
    #[test]
    fn a_nested_field_default_that_needs_resolution_is_reported_at_its_position() {
        let definitions = vec![definition(
            def_id(),
            declared(
                TypeKind::Int,
                Constraints {
                    min: Some(CellValue::Int(0)),
                    ..Constraints::default()
                },
            ),
        )];
        let root = schema(vec![column(
            "入れ子",
            declared(
                TypeKind::Object,
                Constraints {
                    fields: vec![FieldDecl {
                        name: "数量".into(),
                        ty: TypeDecl::Ref(def_id()),
                        required: false,
                        default: Some(CellValue::Int(-1)),
                        description: None,
                    }],
                    ..Constraints::default()
                },
            ),
        )]);
        match compile_declaration(&root, &definitions, &TypeRegistry::new()) {
            Err(SchemaError::InvalidDefault { column }) => {
                assert_eq!("columns[0].type.fields[0]", column)
            }
            other => panic!("入れ子のフィールドの不適合な既定値が通った: {other:?}"),
        }
    }

    /// 行を追加するときの初期値の列を、宣言された既定値から組み立て、既定値の無い列へ
    /// 値なしを与える（要件 4.2, 4.3。design.md「Public API Layer / SchemaEngineApi」の
    /// `default_row`）。
    #[test]
    fn the_default_row_assembles_declared_defaults_and_nulls_for_the_rest() {
        let definitions = vec![definition(
            def_id(),
            declared(TypeKind::Int, Constraints::default()),
        )];
        let mut directly_declared = column("数量", declared(TypeKind::Int, Constraints::default()));
        directly_declared.default = Some(CellValue::Int(5));
        let mut behind_ref = column("参照の既定値", TypeDecl::Ref(def_id()));
        behind_ref.default = Some(CellValue::Int(7));
        let mut custom_default = column(
            "郵便番号",
            declared(
                TypeKind::Custom,
                Constraints {
                    custom_type: Some(CUSTOM_ID.into()),
                    ..Constraints::default()
                },
            ),
        );
        custom_default.default = Some(CellValue::Text("〒100-0001".into()));
        let root = schema(vec![
            directly_declared,
            column(
                "既定値なし",
                declared(TypeKind::Text, Constraints::default()),
            ),
            behind_ref,
            custom_default,
        ]);
        let compiled = compile_declaration(&root, &definitions, &registry_with_postal_code())
            .expect("標本は計画へ落ちる");

        assert_eq!(
            vec![
                CellValue::Int(5),
                // 既定値が宣言されていない列は値なしになる（要件 4.3 の後段）。
                CellValue::Null,
                // 参照の先の型で適合を検査済みの既定値もそのまま供給される（要件 4.8）。
                CellValue::Int(7),
                // 拡張型の既定値も同じ経路で供給される（要件 4.8, 11.2）。
                CellValue::Text("〒100-0001".into()),
            ],
            compiled.default_row()
        );
    }

    /// 供給された初期値の長さが常に列数と一致する（要件 4.3）。行の値は列の添字で並ぶため
    /// （design.md「Data Models / Domain Model」の不変条件）、既定値の有無・使用不能な列・
    /// 入れ子の列が混ざっても長さは動かない。
    #[test]
    fn the_default_row_length_always_matches_the_column_count() {
        // 列を 1 本も宣言していないルートスキーマ（新規シートの初期状態。要件 1.8）。
        let empty = compile_declaration(&schema(Vec::new()), &[], &TypeRegistry::new())
            .expect("列 0 本の宣言は受理される");
        assert!(empty.default_row().is_empty());
        assert_eq!(empty.column_count(), empty.default_row().len());

        // 使用不能な列（未知の `kind`。要件 11.7）と入れ子の列を含む標本。
        let mut unknown = column("未知", declared(TypeKind::Int, Constraints::default()));
        unknown.ty = TypeDecl::Kind {
            kind: DeclaredKind::Unknown("future-kind".into()),
            constraints: Constraints::default(),
        };
        let mut with_default = column("既定値", declared(TypeKind::Int, Constraints::default()));
        with_default.default = Some(CellValue::Int(1));
        let root = schema(vec![
            with_default,
            column(
                "入れ子",
                declared(TypeKind::Object, catalog_constraints(TypeKind::Object)),
            ),
            unknown,
            column(
                "配列",
                declared(TypeKind::Array, catalog_constraints(TypeKind::Array)),
            ),
        ]);
        let compiled = compile_declaration(&root, &[], &TypeRegistry::new())
            .expect("使用不能な列があってもコンパイルは成功する");

        assert_eq!(4, compiled.column_count());
        assert_eq!(4, compiled.default_row().len());
        assert!(compiled.is_unusable(ColumnIndex::new(2)));
        // 使用不能な列には既定値が無いため値なしが与えられる。他の列の既定値は保たれる。
        assert_eq!(CellValue::Int(1), compiled.default_row()[0]);
        assert_eq!(CellValue::Null, compiled.default_row()[2]);
    }

    /// 行の追加そのものは上流のドキュメントモデルの経路で行い、本クレートは値だけを供給
    /// する（要件 4.3。design.md「Public API Layer / SchemaEngineApi」の
    /// `default_row` の注意）。
    #[test]
    fn the_supplied_defaults_are_written_through_the_upstream_row_path() {
        let mut with_default = column("数量", declared(TypeKind::Int, Constraints::default()));
        with_default.default = Some(CellValue::Int(5));
        let root = schema(vec![
            with_default,
            column("備考", declared(TypeKind::Text, Constraints::default())),
        ]);
        let root_text = schema_to_text(&root).expect("標本のルートは正準形を持つ");
        let envelope = format!(r#"{{"root":{root_text},"types":[]}}"#);

        let mut document = Document::new();
        let sheet = document.add_sheet("標本");
        document
            .set_sheet_columns(sheet, vec!["数量".to_owned(), "備考".to_owned()])
            .expect("標本のシートは実在する");
        document
            .set_root_schema(
                sheet,
                SchemaPart::parse(&envelope).expect("標本は妥当なエンベロープ"),
            )
            .expect("標本のシートは実在する");
        let compiled = compile(&document.sheets()[0], &TypeRegistry::new())
            .expect("シートのルートスキーマから計画が組み立てられる");

        // 上流の経路で行を発行し、供給された初期値をそのまま書く。
        let row = document.add_row(sheet).expect("標本のシートは実在する");
        document
            .set_row_values(sheet, row, compiled.default_row())
            .expect("標本の行はシートに属する");

        assert_eq!(
            &[CellValue::Int(5), CellValue::Null],
            document.sheets()[0].rows()[0].values(),
            "供給された初期値が上流の行の値になっていない"
        );
    }
}

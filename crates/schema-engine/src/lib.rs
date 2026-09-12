//! jxcel スキーマエンジン（Tauri 非依存のドメインクレート）。
//!
//! シートのスキーマ宣言に意味を与える唯一の場所である（design.md「Overview」）。列が
//! どの型を持ち、どの値が適合し、適合しない値をどう扱い、スキーマを変えたとき既存データが
//! どうなるかを決める。画面・永続化・履歴・計算は所有しない（requirements.md「Boundary
//! Context」の Out of scope）。
//!
//! # 依存の向き（design.md「Architecture Integration」「Allowed Dependencies」）
//!
//! `document-format → schema-engine → (data-grid / schema-editor / macro-runtime /
//! export-templates / form-builder)` の一方向である。本クレートは上流に
//! `document-format` だけを持ち、`tauri` には推移的にも依存しない（CI の
//! `bash scripts/check-core-deps.sh schema-engine` が `cargo tree` 全体を走査して
//! 機械検査する）。`custom-types` は本クレートに依存する側であり、本クレートは
//! `custom-types` を知らない（structure.md「拡張点は所有者と実装者を分ける」）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write
//! → evolution → api`。各層の `mod.rs` の冒頭にこの鎖を書き、左の層だけを参照する
//! （`document-format` の `Ids / Value / EntryName → Model → Json → Parts → Container
//! → Api` と同じ規約）。本ファイルは鎖の最右（`api`）であり、すべての層を参照する。
//! 各層の冒頭の鎖がこの一方向の順序と食い違っていないことは 8.1 で全ファイルについて
//! 確かめた（鎖の記述そのものは各層のタスクが書いた）。
//!
//! # 公開面（design.md「Public API Layer / SchemaEngineApi」。tasks.md 8.1）
//!
//! 対外的な入口は [`SchemaEngineApi`] とその無状態の具象実装 [`SchemaEngine`] である
//! （`document-format` の `DocumentFormatApi` / `DocumentFormat` と同じ形。design.md は
//! トレイトだけを定めるため、利用側が値を持てるように具象型を添える）。design.md の
//! Service Interface が定める 8 つの操作がこの 1 面に載る:
//!
//! 1. 計画の生成 — [`SchemaEngineApi::compile`]（要件 1.1, 1.2, 1.8, 4.8, 11.7）
//! 2. 列の並び順の供給 — [`SchemaEngineApi::columns`]（要件 1.2）
//! 3. シート全体の検証 — [`SchemaEngineApi::validate_sheet`]（要件 5.4, 5.7, 10.4）
//! 4. 列指定の再検証 — [`SchemaEngineApi::validate_columns`]（要件 10.5）
//! 5. 書き込みの判定 — [`SchemaEngineApi::validate_write`]（要件 6.1, 6.2, 7.1）
//! 6. 行の初期値 — [`SchemaEngineApi::default_row`]（要件 4.3）
//! 7. 変更の集計 — [`SchemaEngineApi::plan_change`]（要件 8.2, 8.4）
//! 8. 変更の適用 — [`SchemaEngineApi::apply_change`]（要件 8.5, 8.6, 8.8）
//!
//! 本ファイルは下位の各層の公開項目を根へ**再輸出**する。これにより他クレートは
//! `schema_engine::compile` のような下位モジュールの経路を書かずに済む（公開範囲の指定が
//! その保証である）。`tests/public_api.rs` は**根の再輸出だけ**を使い、宣言の解析から
//! 計画・全件検証・変更の適用までを通して、この保証を実行可能に示す。
//!
//! **検証を走らせる時機は本クレートが決めない。** ファイルを開いた直後に走らせるか、
//! スキーマを変更した後に走らせるかは呼び出し元が決める（design.md「Validate Layer /
//! SheetValidator」の Batch 契約の Trigger。要件 5.7 は、その操作が存在し**1 回の呼び出し**
//! で全行の結果が得られることを保証する）。
//!
//! 本クレートは**状態を持たない**（design.md 同節の Responsibilities & Constraints）。
//! [`CompiledSchema`] と [`TypeRegistry`] は呼び出し元が保持し、各操作へ渡す。スキーマが
//! 変われば計画は作り直す（[`CompiledSchema`] は不変である）。

pub mod coerce;
pub mod compile;
pub mod declaration;
pub mod error;
pub mod evolution;
pub mod registry;
pub mod types;
pub mod validate;
pub mod write;

pub use coerce::Coercion;
pub use compile::{compile_declaration, ColumnIndex, CompiledSchema};
pub use declaration::codec::{
    parse_schema, parse_type_definition, schema_to_text, type_definition_to_text,
};
pub use declaration::{
    ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl, TypeDefinition,
};
pub use error::SchemaError;
pub use evolution::diff::{diff, ColumnChange, ColumnConstraints, SchemaDiff};
pub use evolution::{
    apply_change, plan_change, AppliedChange, ChangeImpact, ChangePlan, PlanDigest, StalePlan,
    ValueLoss,
};
pub use registry::{CustomType, CustomTypeFailure, CustomTypeId, CustomVerdict, TypeRegistry};
pub use types::datetime::{OffsetPolicy, TemporalForm, TemporalValue};
pub use types::decimal::DecimalDigits;
pub use types::text::TextConstraints;
pub use types::{Acceptance, AcceptedVariants, TypeKind, ValueVariant};
pub use validate::report::{
    Expected, SheetReport, ValidationOptions, ValuePath, ValuePathSegment, Violation,
    ViolationReason,
};
pub use validate::{validate_columns, validate_sheet};
pub use write::{validate_write, CollectVerdict, EditVerdict, WriteOrigin, WriteVerdict};

use document_format::{CellValue, Document, Sheet, SheetId};

/// 本クレートの唯一の入口（design.md「Public API Layer / SchemaEngineApi」。要件 1.2, 4.3,
/// 5.7, 10.4）。
///
/// コンパイル済みスキーマの生成と、その上での検証・書き込み判定・変更の提供をこの面に
/// 束ねる。**他クレートは下位モジュールを直接触らない** — design.md の Intent が
/// 「本クレートの唯一の入口」と定めており、[`SchemaEngine`] がその実装である。
///
/// 事前条件（design.md 同節の Preconditions）: 検証・書き込み判定・行の初期値に渡す
/// `schema` は、**同じ `sheet` から** [`SchemaEngineApi::compile`] したものであること。
/// 計画の列の添字は `Row::values()` の並びと一致するため、別のシートの計画を渡すと
/// 添字が意味を失う。
///
/// 事後条件: `validate_*` はドキュメントを変更しない。[`SchemaEngineApi::plan_change`] も
/// ドキュメントを変更しない（要件 8.4）。
///
/// 不変条件: 同じ入力に対して [`SheetReport`] の違反集合と順序が常に一致する（要件 5.5）。
pub trait SchemaEngineApi {
    /// シートのルートスキーマと型定義を解決し、列添字で引ける計画へ落とす
    /// （design.md「Compile Layer / SchemaCompiler」。要件 1.1, 1.2, 1.8, 4.8, 11.7）。
    ///
    /// 上流が不透明なペイロードとして保持する `root` と各 `definition` の中身を読み出し、
    /// **一度だけ**解決する。以後の走査は宣言をたどらず、計画の検証器を添字で引く。
    ///
    /// 事前条件: `sheet` が上流の文書に属していること。
    /// 事後条件: `Ok` の場合、返る計画の列の並びは宣言の `columns` 配列の順そのものである。
    /// `Err` は**宣言が壊れている**ことを表す（[`SchemaError`]。値の違反とは別の型である）。
    /// 解決できない型（未知の `kind`・未登録の拡張型・展開できない再帰型）は宣言ごとでは
    /// なく**その列だけを使用不能**にする（要件 11.7。スキーマは破棄しない）。
    fn compile(
        &self,
        sheet: &Sheet,
        registry: &TypeRegistry,
    ) -> Result<CompiledSchema, SchemaError>;

    /// 行データを永続化する呼び出し元へ渡す列名（並び順そのもの。要件 1.2）。
    ///
    /// 上流の `Sheet::columns()` と同じ形であり、呼び出し元はこれをそのまま行データの
    /// キー順として出力する（列名は行オブジェクトのキーそのものであり、値と同時に更新
    /// しないと「1 シート内の全行が同一のキー列」という上流の不変条件が壊れる）。
    fn columns<'a>(&self, schema: &'a CompiledSchema) -> &'a [String];

    /// シート全体の一括検証（要件 5.4, 5.7, 10.4）。
    ///
    /// **行ごとの呼び出しを要しない**一括の操作である（design.md「Validate Layer /
    /// SheetValidator」の Batch 契約）。上限までの違反・総件数・違反を持つ行の一覧を返す。
    /// 検証を走らせる時機は決めない — 引き金は呼び出し元が持つ（クレート docs）。
    ///
    /// 事前条件: `schema` が同じ `sheet` から [`SchemaEngineApi::compile`] したものであること。
    /// 事後条件: ドキュメントを変更しない。何度呼んでも同じ結果になる（冪等）。
    /// `options` の上限を超えた違反は保持されないが、総件数は数え続ける（要件 5.6）。
    fn validate_sheet(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        options: &ValidationOptions,
    ) -> SheetReport;

    /// 指定した列だけの再検証（要件 10.5）。
    ///
    /// スキーマの一部が変わったときに全列を舐め直さないための経路である。指定できるのは
    /// 列の添字であり、**並びは結果に影響しない**（列添字の昇順へ正規化される）。
    ///
    /// 事前条件: `schema` が同じ `sheet` から [`SchemaEngineApi::compile`] したものであること。
    /// 事後条件: [`SchemaEngineApi::validate_sheet`] と同じ（指定した列に閉じる）。
    fn validate_columns(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        columns: &[ColumnIndex],
        options: &ValidationOptions,
    ) -> SheetReport;

    /// 1 行分の書き込みに対する経路ごとの判定。強制も含む（要件 6.1, 6.2, 6.3, 6.4, 7.1）。
    ///
    /// 強制（[`crate::coerce`]）と検証の順序は両経路で同じであり、分岐するのは違反が
    /// あったときの扱いだけである。**拒否の実行はしない** — 送信を止めるのは
    /// `form-web-server` の仕事であり、本メソッドは判定を返すだけである（design.md
    /// 「Write Layer / WritePolicy」）。
    ///
    /// 事前条件: `values` の長さが計画の列数と一致すること（短い並びは残りを値なしとして
    /// 扱い、長い並びは計画の外として判定しない）。値の添字は列の添字である。
    /// 事後条件: `Edit` は決して `Rejected` を返さず、`Collect` は決して
    /// `AcceptedWithViolations` を返さない（型で保証される）。値を 1 つも失わない。
    ///
    /// **行を跨ぐ性質（一意性と参照の実在）は判定しない** — 1 行だけを見て判定できない
    /// ためであり、それらは [`SchemaEngineApi::validate_sheet`] /
    /// [`SchemaEngineApi::validate_columns`] が持つ（design.md「Validate Layer /
    /// SheetValidator」の「行を跨ぐ性質は一括経路にだけ置く」）。
    fn validate_write(
        &self,
        origin: WriteOrigin,
        schema: &CompiledSchema,
        values: Vec<CellValue>,
    ) -> WriteVerdict;

    /// 行を追加するときの初期値の列（要件 4.2, 4.3）。
    ///
    /// 宣言された既定値を列の並び順に並べ、既定値が宣言されていない列には値なしを与える。
    /// 返る長さは**常に**計画の列数と一致する（行の値は列の添字で並ぶため）。
    ///
    /// **行の追加そのものは本クレートの仕事ではない。** 上流の `Document::add_row` が行を
    /// 発行し、`Document::set_row_values` がこの値を書く。本クレートは値だけを供給する。
    /// 使用不能な列（要件 11.7）では適合検査が走らないため、宣言された既定値がそのまま
    /// 供給される（その列の値は既定値を含めてすべて使用不能の違反になる）。
    fn default_row(&self, schema: &CompiledSchema) -> Vec<CellValue>;

    /// スキーマ変更の影響を集計する。ドキュメントは変更しない（要件 8.2, 8.4）。
    ///
    /// `to` は適用したい新しい宣言である（design.md はこの引数を `SchemaDeclaration` と
    /// 呼ぶ。Rust のデータ構造は [`Schema`]）。集計は**適用した場合**の変換・違反・喪失の
    /// 行数を返し、値が失われる位置（行識別子と旧列名）を含む（要件 8.3）。適用の前に
    /// 見せることで、取り返しのつかない変換を知らずに走らせない（requirements.md
    /// 「Requirement 8」の Objective）。
    ///
    /// 事前条件: `to` が解決可能な宣言であること。
    /// 事後条件: ドキュメントを変更しない（要件 8.4）。`Err` の場合、宣言が壊れている
    /// （[`SchemaError`]）。
    /// 不変条件: 返る [`ChangePlan`] を適用した結果は、計画が返した [`ChangeImpact`] と
    /// 一致する（要件 8.5）。
    fn plan_change(
        &self,
        doc: &Document,
        sheet: SheetId,
        to: &Schema,
        registry: &TypeRegistry,
    ) -> Result<ChangePlan, SchemaError>;

    /// 計画を適用する。可謬な処理は計画側に済ませてある（要件 8.5, 8.6, 8.8）。
    ///
    /// 適用の前に計画のダイジェストを現在のシートと照合し、食い違えば [`StalePlan`] を
    /// 返して**何も変更しない**（要件 8.5）。一致すれば計画が持つ新しい列名と全行分の
    /// 新しい値を書き戻す（列の追加では既定値または値なしが全行に入る。要件 8.8）。
    ///
    /// **ルートスキーマは設置しない。** 計画はルート宣言しか受け取らず名前付き型定義の集合を
    /// 持たないため、上流のエンベロープを正しく組み立てられない（無理に組み立てると型定義と
    /// 未知フィールドを破壊する。design.md「Out of Boundary」）。呼び出し元は自分が持つ `to`
    /// を `Document::set_root_schema` で設置する（設置を忘れた場合は次のコンパイルが古い宣言を
    /// 使う、観測可能で回復可能な状態になる）。
    ///
    /// 事前条件: 計画が対象シートの現在の状態に対して作られていること。
    /// 事後条件: `Ok` の場合、シートの列名と全行の値が計画どおりになる。`Err` の場合、
    /// ドキュメントは一切変更されない（要件 8.6 の原子性）。
    fn apply_change(
        &self,
        doc: &mut Document,
        plan: ChangePlan,
    ) -> Result<AppliedChange, StalePlan>;
}

/// 公開面の無状態の具象実装（design.md はトレイトのみを指定するため、利用側が値を持てる
/// ように本型を添える。`document-format` の `DocumentFormat` と同じ形）。
///
/// ```text
/// let engine = SchemaEngine::new();
/// let schema = engine.compile(sheet, &registry)?;
/// let report = engine.validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
/// ```
///
/// 状態を持たないため、呼び出しごとに作っても 1 つを持ち回っても同じ結果になる。
#[derive(Debug, Clone, Copy, Default)]
pub struct SchemaEngine;

impl SchemaEngine {
    /// 無状態の実装を作る。
    #[inline]
    pub const fn new() -> Self {
        Self
    }
}

impl SchemaEngineApi for SchemaEngine {
    fn compile(
        &self,
        sheet: &Sheet,
        registry: &TypeRegistry,
    ) -> Result<CompiledSchema, SchemaError> {
        // 宣言の取り出し・`$ref` の解決・検証器への落とし込みは `compile` 層の 1 経路が
        // 運ぶ（規則をこの層に再実装しない）。
        crate::compile::compile(sheet, registry)
    }

    fn columns<'a>(&self, schema: &'a CompiledSchema) -> &'a [String] {
        schema.columns()
    }

    fn validate_sheet(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        options: &ValidationOptions,
    ) -> SheetReport {
        crate::validate::validate_sheet(doc, sheet, schema, options)
    }

    fn validate_columns(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        columns: &[ColumnIndex],
        options: &ValidationOptions,
    ) -> SheetReport {
        crate::validate::validate_columns(doc, sheet, schema, columns, options)
    }

    fn validate_write(
        &self,
        origin: WriteOrigin,
        schema: &CompiledSchema,
        values: Vec<CellValue>,
    ) -> WriteVerdict {
        crate::write::validate_write(origin, schema, values)
    }

    fn default_row(&self, schema: &CompiledSchema) -> Vec<CellValue> {
        schema.default_row()
    }

    fn plan_change(
        &self,
        doc: &Document,
        sheet: SheetId,
        to: &Schema,
        registry: &TypeRegistry,
    ) -> Result<ChangePlan, SchemaError> {
        crate::evolution::plan_change(doc, sheet, to, registry)
    }

    fn apply_change(
        &self,
        doc: &mut Document,
        plan: ChangePlan,
    ) -> Result<AppliedChange, StalePlan> {
        crate::evolution::apply_change(doc, plan)
    }
}

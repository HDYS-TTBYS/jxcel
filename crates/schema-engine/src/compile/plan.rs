//! 列 1 本分の検証器（design.md「コンポーネントとファイルの対応」の `ColumnValidator`。
//! tasks.md 4.2。要件 2.7, 10.6）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは `compile` 層の一部であり、`types` と `registry` の
//! 右隣に置かれる。本モジュールを参照するのは同じ `compile` 層の `SchemaCompiler`
//! （タスク 4.4）と、`validate` 層（タスク 5.1 / 5.2 / 5.3）である。**本モジュールは
//! `validate` 層の型を参照しない** — 検証器を組み立てる側（`DeclarationCodec` と
//! `SchemaCompiler`）が `SchemaError` を返す。本モジュールは `SchemaError` を生成しない。
//!
//! # `TextPattern::compile` をここで呼ばない
//!
//! パターンのコンパイルは `SchemaCompiler`（タスク 4.4）が列ごとに一度だけ行う
//! （design.md「Compile Layer / SchemaCompiler」）。本モジュールは**コンパイル済みの**
//! [`TextConstraints`] を受け取るだけであり、`regex` を直接参照しない。
//!
//! # 閉じた列挙体であり、動的ディスパッチを持たない（design.md「Compile Layer /
//! ColumnValidator」。要件 10.6）
//!
//! [`ColumnValidator`] の変種は design.md「組込型カタログと `CellValue` への写像」の表の
//! `kind` と**1 対 1** に対応する（14 種別）。判定は列挙体の直接マッチ
//! （[`ColumnValidator::check`]）であり、`Box<dyn Fn>` を持たない。10 万行 × 30 列の走査では
//! 動的ディスパッチが実測で数倍の差になるためである。
//!
//! **拡張型だけは実装を保持する必要がある**ため、[`ColumnValidator::Custom`] の 1 変種に
//! `Arc<dyn CustomType>` を閉じ込める。拡張インターフェースは列ごとの一括判定
//! （`CustomType::validate_batch`）を持つため、一括経路の内側で行ごとの動的ディスパッチは
//! 起きない（要件 10.6, 11.6。structure.md「そのバッチ経路の内側では動的ディスパッチを
//! 使わない」）。したがって本モジュールは一括の入口を自前で持たず、`validate` 層
//! （タスク 5.4 / 9.4）が [`ColumnValidator::Custom`] の実装を取り出して一括判定へ載せる。
//!
//! # パラメータはインラインに持つ
//!
//! 範囲・桁・選択肢・コンパイル済みパターンは検証器の内側に持つ（design.md 同節）。
//! 走査の内側で宣言を引き直さず、間接参照も増やさない。範囲は [`Bounds`] が
//! **宣言された端点**と**比較用の端点**を対で持つ。後者を持つのは、10 進数の正準化
//! （[`decimal::canonicalize`]）と日時の解析（[`TemporalForm::parse`]）を値ごとに繰り返さ
//! ないためである（要件 10.6）。
//!
//! # 値なしは型の側では判定しない
//!
//! [`CellValue::Null`] はどの変種でも適合である（design.md の型カタログ表「`Null` はどの型
//! でも「値なし」を表し、受理されるかは列の `required` が決める（型の側では決めない）」）。
//! [`ColumnValidator::check`] は値なしを適合として返し、拡張型の実装へも渡さない。必須の
//! 判定は本モジュールの外にある — `required` は値の制約ではなく列とフィールドの側の制約で
//! あり（tasks.md 3.1）、列の側の `required` は計画（`CompiledSchema`。タスク 4.4）が持ち、
//! 入れ子のフィールドの `required` は [`FieldValidator`] が持つ。`ViolationReason::
//! MissingValue` を組み立てるのは `validate` 層（タスク 5.1）である。
//!
//! # 適合または違反のいずれか一方（要件 2.7）
//!
//! [`ColumnVerdict`] は**適合**と**違反**の 2 値であり、「判定できない」という第 3 の状態を
//! 持たない（`types` 層の `Acceptance` と同じ 2 値の規約）。違反は種類
//! （[`ColumnViolation`]）を持ち、`validate` 層はそれと実際の値から `ViolationReason` を
//! 組み立てる。層の鎖が一方向である以上、違反の理由の型（`Expected` / `ViolationReason`。
//! `src/validate/report.rs`）を本層から参照できないため、**本層の語彙で文脈を運ぶ**。
//! 各変種の docs が対応する `ViolationReason` を示す。
//!
//! # 本モジュールが判定しないもの
//!
//! - **入れ子の再帰**: [`ColumnValidator::Object`] の `fields` と [`ColumnValidator::Array`]
//!   の `items` を降りるのは `validate` 層（タスク 5.1）である。本モジュールは値の**形**と
//!   配列の**要素数**だけを判定する（違反の位置をフィールド名と添字の並びで運ぶには、降りる
//!   側が [`ValuePath`](crate::validate::report::ValuePath) を組み立てる必要がある）。
//! - **参照先の実在**: [`ColumnValidator::Ref`] は参照先シートを持つだけで、行が実在するかを
//!   判定しない（要件 9.2 の一括判定はタスク 5.3）。
//! - **一意性**: 行を跨ぐため `validate` 層（タスク 5.2）が判定する。
//! - **型強制**: 与えられた値を変換するのは `coerce` 層（群 7）である。本モジュールは変換
//!   せず、適合しない値を違反として返す（要件 2.6 の「ANY は強制しない」も同じ線引き）。

use core::fmt;
use std::sync::Arc;

use document_format::{CellValue, NestedValue, SheetId};

use crate::registry::{CustomType, CustomTypeId, CustomVerdict};
use crate::types::datetime::{TemporalForm, TemporalValue};
use crate::types::decimal::{self, DecimalCanonical, DecimalDigits};
use crate::types::text::TextConstraints;
use crate::types::{Acceptance, TypeKind};

/// 範囲制約（design.md「Compile Layer / ColumnValidator」の「範囲をインラインに持つ」）。
///
/// 2 つの形を対で持つ。**宣言された端点**は値の制約が書かれたとおりの [`CellValue`] であり、
/// 違反の理由（[`ColumnViolation::OutOfRange`]）がそのまま運ぶ。**比較用の端点**（`V`）は
/// 走査の内側で再解釈しないための形であり、10 進数の正準化と日時の解析を行ごとに繰り返さ
/// ないために持つ（要件 10.6）。端点が宣言されていない側は `None`（開いた端点）である。
#[derive(Debug, Clone)]
pub struct Bounds<V> {
    declared_min: Option<CellValue>,
    declared_max: Option<CellValue>,
    min: Option<V>,
    max: Option<V>,
}

impl<V> Bounds<V> {
    /// 端点を組にする。開いている側は `None`。
    ///
    /// `min` / `max` は `declared_min` / `declared_max` をそれぞれ同じ型へ解釈した結果である
    /// （宣言された端点が無ければ比較用の端点も無い）。解釈の仕方 — 整数の取り出し・10 進数の
    /// 正準化・日時の解析 — は検証器を組み立てる側（`SchemaCompiler`。タスク 4.4）が所有する。
    pub fn new(
        declared_min: Option<CellValue>,
        declared_max: Option<CellValue>,
        min: Option<V>,
        max: Option<V>,
    ) -> Self {
        Self {
            declared_min,
            declared_max,
            min,
            max,
        }
    }

    /// 宣言された下限（含む）。違反の理由が運ぶ。
    pub fn declared_min(&self) -> Option<&CellValue> {
        self.declared_min.as_ref()
    }

    /// 宣言された上限（含む）。違反の理由が運ぶ。
    pub fn declared_max(&self) -> Option<&CellValue> {
        self.declared_max.as_ref()
    }
}

impl<V: PartialOrd> Bounds<V> {
    /// 値が範囲の外にあるか。端点は**含む**（design.md の `min` / `max`）。
    fn excludes(&self, value: &V) -> bool {
        self.min.as_ref().is_some_and(|min| value < min)
            || self.max.as_ref().is_some_and(|max| value > max)
    }
}

impl<V> Bounds<V> {
    /// 範囲外の違反を組み立てる（宣言された端点を運ぶ）。
    fn violation(&self) -> ColumnViolation {
        ColumnViolation::OutOfRange {
            min: self.declared_min.clone(),
            max: self.declared_max.clone(),
        }
    }
}

/// 入れ子のオブジェクトのフィールド 1 本分の検証器（design.md「Compile Layer /
/// ColumnValidator」の `object` の `fields`。tasks.md 3.1、要件 3.2, 4.4）。
///
/// フィールドは名前を持つ位置であるため、名前と存在の制約（`required`）を型の判定
/// （[`ColumnValidator`]）と一緒に持つ。入れ子の内側には列名の並びが無く、計画
/// （`CompiledSchema`。タスク 4.4）がフィールドごとの位置を持たないため、この 2 つは
/// ここに置く。列そのものの `required` は計画が列ごとに持つ。
#[derive(Debug, Clone)]
pub struct FieldValidator {
    name: Box<str>,
    required: bool,
    validator: ColumnValidator,
}

impl FieldValidator {
    /// フィールド 1 本分の検証器を組にする。
    pub fn new(name: impl Into<Box<str>>, required: bool, validator: ColumnValidator) -> Self {
        Self {
            name: name.into(),
            required,
            validator,
        }
    }

    /// フィールド名（違反の位置が運ぶ）。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 値なしを許さないか（要件 4.1, 4.4）。
    pub const fn required(&self) -> bool {
        self.required
    }

    /// 型の判定。
    pub fn validator(&self) -> &ColumnValidator {
        &self.validator
    }
}

/// 列 1 本分の検証器（design.md「Compile Layer / ColumnValidator」。tasks.md 4.2。
/// 要件 2.7, 10.6）。
///
/// 変種は design.md「組込型カタログと `CellValue` への写像」の表の `kind` と 1 対 1 に
/// 対応し、それぞれがその型のパラメータ（範囲・桁・選択肢・コンパイル済みパターン・
/// 入れ子の形・拡張型の実装）を**インラインに**持つ。
///
/// 判定は [`ColumnValidator::check`] の直接マッチであり、`Custom` 以外に間接参照を持たない。
/// 列の側の制約（`required` / `unique` / `default`）はここに無い — 値の制約ではないためで
/// ある（tasks.md 3.1。`unique` は行を跨ぐので `validate` 層の一括経路が持つ）。
#[derive(Clone)]
pub enum ColumnValidator {
    /// 整数（design.md の `int`）。`Int` だけを受け入れる。
    Int {
        /// 範囲（`min` / `max`）。
        bounds: Bounds<i64>,
    },
    /// 小数（design.md の `float`）。`Float` だけを受け入れる。
    ///
    /// `Int` は受け入れない（整数と浮動小数の型ドリフトは `document-format` が構造で
    /// 禁じており、本クレートも同じ規則を守る）。**非有限の浮動小数は本クレートの関心事では
    /// ない** — 書き出し時に拒否するのは `document-format` であり（design.md「Existing
    /// Architecture Analysis」）、本モジュールは範囲の端点と比較するだけである。
    Float {
        /// 範囲（`min` / `max`）。
        bounds: Bounds<f64>,
    },
    /// 10 進数（design.md の `decimal`）。`Decimal` だけを受け入れる。
    ///
    /// 桁は**書かれた形**で判定する（[`DecimalDigits`] の docs と tasks.md 2.2 の裁定）。
    /// したがって `decimal(2,1)` は `"1.5"` を適合とし `"1.50"` を違反とする。
    Decimal {
        /// 宣言された有効桁数と小数点以下の桁数（`precision` / `scale`）。
        digits: DecimalDigits,
        /// 範囲（`min` / `max`）。比較用の端点は正準形である。
        bounds: Bounds<DecimalCanonical>,
    },
    /// 文字列（design.md の `text`）。`Text` だけを受け入れる。
    Text {
        /// 長さと書式（`minLength` / `maxLength` / `pattern`。コンパイル済み）。
        constraints: TextConstraints,
    },
    /// 真偽（design.md の `bool`）。`Bool` だけを受け入れる。
    Bool,
    /// 日付（design.md の `date`）。`Text` を `YYYY-MM-DD` として**厳密に**受け入れる。
    ///
    /// 正準表記に一致しない `Text`（`2026/09/12` や時刻を含むもの）は [`ColumnViolation::TypeMismatch`]
    /// である（要件 2.4, 7.3）。
    Date {
        /// 範囲（`min` / `max`）。比較用の端点は解釈済みの値である。
        bounds: Bounds<TemporalValue>,
    },
    /// 日時（design.md の `datetime`）。`Text` を `form` の形として厳密に受け入れる。
    DateTime {
        /// 時刻の有無とオフセットの扱い（`offset` の 2 値）。
        form: TemporalForm,
        /// 範囲（`min` / `max`）。比較は瞬時（または civil の暦・時計）の順序で行う。
        bounds: Bounds<TemporalValue>,
    },
    /// 列挙（design.md の `enum`）。`Text` を `choices` のいずれかとして受け入れる。
    Enum {
        /// 選択肢（`choices`）。宣言順を保つ。
        choices: Box<[Box<str>]>,
    },
    /// シート間参照（design.md の `ref`）。`Text` を行識別子として受け入れる。
    ///
    /// **行の実在は判定しない**（要件 9.2 の一括判定はタスク 5.3）。ここでは参照先シートを
    /// 持つだけである。
    Ref {
        /// 参照先のシート（`sheet`）。
        sheet: SheetId,
    },
    /// 添付参照（design.md の `attachment`）。`Attachment` だけを受け入れる。
    ///
    /// 添付の実在は判定しない（design.md「Out of Boundary」— 実在は `document-format` が
    /// 所有する）。
    Attachment,
    /// 名前つきフィールドの集合（design.md の `object`）。`Nested` のオブジェクトだけを
    /// 受け入れる。**内側のフィールドを降りるのは `validate` 層**（タスク 5.1）である。
    Object {
        /// フィールド（`fields`）。宣言順を保つ。
        fields: Box<[FieldValidator]>,
    },
    /// 同一型の並び（design.md の `array`）。`Nested` の配列だけを受け入れる。
    ///
    /// 要素数（`minItems` / `maxItems`）はここで判定し、**要素 1 つずつの判定は `validate` 層**
    /// （タスク 5.1）が `items` を降りて行う。
    Array {
        /// 要素の型（`items`）。
        items: Box<ColumnValidator>,
        /// 要素数の下限（`minItems`）。
        min_items: Option<usize>,
        /// 要素数の上限（`maxItems`）。
        max_items: Option<usize>,
    },
    /// 任意の値（design.md の `any`）。保持可能なすべての変種を受け入れ、型強制もしない
    /// （要件 2.5, 2.6）。
    Any,
    /// 拡張型（design.md の `custom`。要件 11.1, 11.2）。
    ///
    /// 判定は実装（[`CustomType`]）へ委ねる。**本変種だけが間接参照を持つ**が、拡張
    /// インターフェースが列ごとの一括判定を持つため、一括経路の内側で行ごとの動的
    /// ディスパッチにはならない（要件 10.6, 11.6）。
    Custom {
        /// 宣言された拡張型の識別子（`type`）。違反の理由（`Expected::AcceptedBy`）が運ぶ。
        id: CustomTypeId,
        /// 登録された実装。一括判定（`CustomType::validate_batch`）は `validate` 層が
        /// この実装を列ごとに 1 回呼ぶ。
        imp: Arc<dyn CustomType>,
    },
}

impl ColumnValidator {
    /// 1 つの値の判定（要件 2.7）。**適合か違反のいずれか一方**を返す。
    ///
    /// 値なし（[`CellValue::Null`]）はどの変種でも適合であり、拡張型の実装へも渡さない
    /// （モジュール docs「値なしは型の側では判定しない」）。必須の判定は本メソッドの外で
    /// 行われる。
    ///
    /// 拡張型（[`ColumnValidator::Custom`]）だけが実装へ委ね、他の変種は列挙体の直接マッチで
    /// 判定する（要件 10.6）。入れ子の内側（`Object` の `fields`・`Array` の `items`）へは
    /// 降りない。
    pub fn check(&self, value: &CellValue) -> ColumnVerdict {
        // 値なしはどの型でも「値なし」を表し、受理されるかは列の `required` が決める。
        // 型の側では判定せず、拡張型の実装へも渡さない（design.md の型カタログ表）。
        if matches!(value, CellValue::Null) {
            return ColumnVerdict::Conforming;
        }
        match self {
            ColumnValidator::Int { bounds } => match value {
                CellValue::Int(found) => judge_range(bounds, found),
                _ => ColumnVerdict::type_mismatch(TypeKind::Int),
            },
            ColumnValidator::Float { bounds } => match value {
                CellValue::Float(found) => judge_range(bounds, found),
                _ => ColumnVerdict::type_mismatch(TypeKind::Float),
            },
            ColumnValidator::Decimal { digits, bounds } => match value {
                CellValue::Decimal(found) => {
                    // 桁は書かれた形で判定する（tasks.md 2.2 の裁定）。
                    if !matches!(digits.accepts(found), Acceptance::Conforming) {
                        ColumnVerdict::Violating(ColumnViolation::PrecisionExceeded {
                            precision: digits.precision(),
                            scale: digits.scale(),
                        })
                    } else if decimal::canonicalize(found)
                        .is_some_and(|canonical| bounds.excludes(&canonical))
                    {
                        ColumnVerdict::Violating(bounds.violation())
                    } else {
                        ColumnVerdict::Conforming
                    }
                }
                _ => ColumnVerdict::type_mismatch(TypeKind::Decimal),
            },
            ColumnValidator::Text { constraints } => match value {
                CellValue::Text(found) => {
                    // 長さを先に、書式を後に見る（`TextConstraints::accepts` と同じ順。
                    // 長さの判定のほうが安い）。述語は `TextConstraints` が所有する
                    // （規則の単一の源。同じ判断を写すと片方だけ直ったときに食い違う）。
                    if !constraints.length_fits(found) {
                        ColumnVerdict::Violating(ColumnViolation::LengthOutOfRange {
                            min: constraints.min_length(),
                            max: constraints.max_length(),
                        })
                    } else {
                        match constraints.pattern() {
                            Some(pattern) if !pattern.is_match(found) => {
                                ColumnVerdict::Violating(ColumnViolation::PatternMismatch {
                                    pattern: pattern.as_str().into(),
                                })
                            }
                            _ => ColumnVerdict::Conforming,
                        }
                    }
                }
                _ => ColumnVerdict::type_mismatch(TypeKind::Text),
            },
            ColumnValidator::Bool => match value {
                CellValue::Bool(_) => ColumnVerdict::Conforming,
                _ => ColumnVerdict::type_mismatch(TypeKind::Bool),
            },
            ColumnValidator::Date { bounds } => match value {
                // 日付は正準表記に厳密一致するものだけを値として解釈する（要件 2.4, 7.3）。
                // 解釈できない `Text` はその型の値ではないため型の不一致に落ちる。
                CellValue::Text(found) => match TemporalForm::Date.parse(found) {
                    Some(parsed) => judge_range(bounds, &parsed),
                    None => ColumnVerdict::type_mismatch(TypeKind::Date),
                },
                _ => ColumnVerdict::type_mismatch(TypeKind::Date),
            },
            ColumnValidator::DateTime { form, bounds } => match value {
                CellValue::Text(found) => match form.parse(found) {
                    Some(parsed) => judge_range(bounds, &parsed),
                    None => ColumnVerdict::type_mismatch(TypeKind::DateTime),
                },
                _ => ColumnVerdict::type_mismatch(TypeKind::DateTime),
            },
            ColumnValidator::Enum { choices } => match value {
                CellValue::Text(found) => {
                    if choices
                        .iter()
                        .any(|choice| choice.as_ref() == found.as_str())
                    {
                        ColumnVerdict::Conforming
                    } else {
                        ColumnVerdict::Violating(ColumnViolation::ChoiceNotAllowed {
                            choices: choices.clone(),
                        })
                    }
                }
                _ => ColumnVerdict::type_mismatch(TypeKind::Enum),
            },
            ColumnValidator::Ref { .. } => match value {
                // 行の実在は一括経路（タスク 5.3）が判定する。ここでは行識別子の形だけを見る。
                CellValue::Text(_) => ColumnVerdict::Conforming,
                _ => ColumnVerdict::type_mismatch(TypeKind::Ref),
            },
            ColumnValidator::Attachment => match value {
                CellValue::Attachment(_) => ColumnVerdict::Conforming,
                _ => ColumnVerdict::type_mismatch(TypeKind::Attachment),
            },
            ColumnValidator::Object { .. } => match value {
                // 内側のフィールドへ降りるのは `validate` 層（タスク 5.1）。
                CellValue::Nested(NestedValue::Object(_)) => ColumnVerdict::Conforming,
                _ => ColumnVerdict::type_mismatch(TypeKind::Object),
            },
            ColumnValidator::Array {
                min_items,
                max_items,
                ..
            } => match value {
                CellValue::Nested(NestedValue::Array(items)) => {
                    let count = items.len();
                    if min_items.is_some_and(|min| count < min)
                        || max_items.is_some_and(|max| count > max)
                    {
                        ColumnVerdict::Violating(ColumnViolation::LengthOutOfRange {
                            min: *min_items,
                            max: *max_items,
                        })
                    } else {
                        ColumnVerdict::Conforming
                    }
                }
                _ => ColumnVerdict::type_mismatch(TypeKind::Array),
            },
            // ANY は保持可能なすべての変種を受け入れる（要件 2.5, 2.6）。
            ColumnValidator::Any => ColumnVerdict::Conforming,
            // 拡張型だけが実装へ委ねる。失敗（panic・打ち切り）はその値の違反に閉じ込め、
            // シート全体の検証を中断しない（要件 11.3, 11.5）。
            ColumnValidator::Custom { id, imp } => match imp.validate(value) {
                Ok(CustomVerdict::Accepted) => ColumnVerdict::Conforming,
                Ok(CustomVerdict::Rejected { reason }) => {
                    ColumnVerdict::Violating(ColumnViolation::CustomRejected {
                        id: id.as_str().into(),
                        reason,
                    })
                }
                Err(failure) => ColumnVerdict::Violating(ColumnViolation::CustomFailed {
                    id: id.as_str().into(),
                    reason: failure.reason().into(),
                }),
            },
        }
    }
}

/// 範囲の判定（端点を含む）。範囲の外なら宣言された端点を運ぶ違反を返す。
fn judge_range<V: PartialOrd>(bounds: &Bounds<V>, found: &V) -> ColumnVerdict {
    if bounds.excludes(found) {
        ColumnVerdict::Violating(bounds.violation())
    } else {
        ColumnVerdict::Conforming
    }
}

/// 1 つの値の判定結果（design.md「Compile Layer / ColumnValidator」。要件 2.7）。
///
/// **適合**と**違反**の 2 値であり、「判定できない」という第 3 の状態を持たない
/// （`types` 層の `Acceptance` と同じ規約。モジュール docs「適合または違反のいずれか一方」）。
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnVerdict {
    /// 値が列の型に適合する。
    Conforming,
    /// 値が列の型に適合しない。種類（[`ColumnViolation`]）を持つ。
    Violating(ColumnViolation),
}

impl ColumnVerdict {
    /// 型の不一致の違反（design.md「Compile Layer / ColumnValidator」の `TypeMismatch`）。
    fn type_mismatch(kind: TypeKind) -> Self {
        Self::Violating(ColumnViolation::TypeMismatch { kind })
    }
}

/// 値が適合しなかった種類（design.md「Compile Layer / ColumnValidator」の判定。
/// tasks.md 4.2）。
///
/// 各変種は `validate` 層の `ViolationReason` と `Expected`（`src/validate/report.rs`）へ
/// **1 対 1 に写る** — 層の鎖が一方向であるため本層はそれらを参照できず、同じ文脈を本層の
/// 語彙で運ぶ。対応は各変種の docs が示す。表示用の文言は持たない（呼び出し元が組み立てる。
/// design.md「Error Handling」）。
///
/// 文脈の複製は**違反のときにだけ**起きる。適合の経路（10 万行の大半）は [`ColumnVerdict::Conforming`]
/// を返すだけで、複製も確保もしない（要件 10.6）。
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnViolation {
    /// 値の変種が列の型と食い違う。日付・日時の値が正準表記に一致しない場合もここに落ちる
    /// （`Text` ではあるが、その型の値として解釈できないため）。
    ///
    /// `ViolationReason::TypeMismatch { expected: Expected::Kind(<kind トークン>), .. }` へ写る。
    /// 種別トークン（`"int"` など）の語彙は `declaration::codec` が単一の源である。
    TypeMismatch {
        /// 期待した種別。
        kind: TypeKind,
    },
    /// 値が宣言された範囲の外にある（`min` / `max`。要件 4.5）。
    ///
    /// `ViolationReason::OutOfRange { expected: Expected::Range { min, max }, .. }` へ写る。
    /// 端点は**宣言されたとおりの** [`CellValue`] である（比較用の形ではない）。
    OutOfRange {
        /// 宣言された下限（含む）。開いている側は `None`。
        min: Option<CellValue>,
        /// 宣言された上限（含む）。開いている側は `None`。
        max: Option<CellValue>,
    },
    /// 値の長さ（文字列の文字数）または配列の要素数が範囲の外にある（要件 4.5）。
    ///
    /// `ViolationReason::LengthOutOfRange { expected: Expected::Length { min, max }, .. }` へ
    /// 写る。長さは**文字数**で数える（tasks.md 2.4 の裁定）。
    LengthOutOfRange {
        /// 宣言された下限。開いている側は `None`。
        min: Option<usize>,
        /// 宣言された上限。開いている側は `None`。
        max: Option<usize>,
    },
    /// 値が宣言された書式に一致しない（`pattern`。要件 4.5）。
    ///
    /// 照合は `regex` の標準の**部分一致**である（tasks.md 2.4 の裁定）。全体一致を求める
    /// 宣言は `^…$` を書く。
    ///
    /// `ViolationReason::PatternMismatch { expected: Expected::Pattern(<生文字列>), .. }` へ写る。
    PatternMismatch {
        /// 宣言されたパターンの生文字列。
        pattern: Box<str>,
    },
    /// 値が列挙の選択肢に無い（`choices`。要件 4.5）。
    ///
    /// `ViolationReason::ChoiceNotAllowed { expected: Expected::Choices(..), .. }` へ写る。
    ChoiceNotAllowed {
        /// 宣言された選択肢（宣言順）。
        choices: Box<[Box<str>]>,
    },
    /// 10 進数の値が文法に一致しないか、宣言された有効桁数・小数点以下の桁数を超える
    /// （`precision` / `scale`。要件 2.3）。
    ///
    /// 文法外の `Decimal`（上流の脱出口で書かれた値）もここに落ちる（tasks.md 2.2 の裁定）。
    ///
    /// `ViolationReason::PrecisionExceeded { expected: Expected::Decimal { .. }, .. }` へ写る。
    PrecisionExceeded {
        /// 宣言された有効桁数。
        precision: u32,
        /// 宣言された小数点以下の桁数。
        scale: u32,
    },
    /// 拡張型が値を拒否した（要件 11.3）。
    ///
    /// `ViolationReason::CustomRejected { expected: Expected::AcceptedBy(..), .. }` へ写る。
    /// `reason` は実装が返した文脈であり、表示用の文言ではない。
    CustomRejected {
        /// 拡張型の識別子。
        id: Box<str>,
        /// 実装が返した文脈。
        reason: Box<str>,
    },
    /// 拡張型の判定が失敗した（panic・打ち切り。要件 11.5）。
    ///
    /// その値の違反に閉じ込め、シート全体の検証を中断しない。
    /// `ViolationReason::CustomFailed { expected: Expected::AcceptedBy(..), .. }` へ写る。
    CustomFailed {
        /// 拡張型の識別子。
        id: Box<str>,
        /// 実装が返した文脈。
        reason: Box<str>,
    },
}

impl fmt::Debug for ColumnValidator {
    /// 拡張型の実装（`dyn CustomType`）は `Debug` を持たないため、識別子だけを出す。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int { bounds } => f.debug_struct("Int").field("bounds", bounds).finish(),
            Self::Float { bounds } => f.debug_struct("Float").field("bounds", bounds).finish(),
            Self::Decimal { digits, bounds } => f
                .debug_struct("Decimal")
                .field("digits", digits)
                .field("bounds", bounds)
                .finish(),
            Self::Text { constraints } => f
                .debug_struct("Text")
                .field("constraints", constraints)
                .finish(),
            Self::Bool => f.write_str("Bool"),
            Self::Date { bounds } => f.debug_struct("Date").field("bounds", bounds).finish(),
            Self::DateTime { form, bounds } => f
                .debug_struct("DateTime")
                .field("form", form)
                .field("bounds", bounds)
                .finish(),
            Self::Enum { choices } => f.debug_struct("Enum").field("choices", choices).finish(),
            Self::Ref { sheet } => f.debug_struct("Ref").field("sheet", sheet).finish(),
            Self::Attachment => f.write_str("Attachment"),
            Self::Object { fields } => f.debug_struct("Object").field("fields", fields).finish(),
            Self::Array {
                items,
                min_items,
                max_items,
            } => f
                .debug_struct("Array")
                .field("items", items)
                .field("min_items", min_items)
                .field("max_items", max_items)
                .finish(),
            Self::Any => f.write_str("Any"),
            Self::Custom { id, .. } => f
                .debug_struct("Custom")
                .field("id", id)
                .finish_non_exhaustive(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::CustomTypeFailure;
    use crate::types::datetime::OffsetPolicy;
    use crate::types::text::TextPattern;
    use document_format::AttachmentId;
    use std::str::FromStr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// パターンを宣言した位置（[`TextPattern::compile`] が拒否の診断へ載せる）。
    const POSITION: &str = "columns[0].type.pattern";
    /// 標本のシート識別子（ULID の正準テキスト形）。
    const SHEET: &str = "01K4ANRRG004HMASW9NF6YY091";
    /// 標本の拡張型の識別子。
    const CUSTOM_ID: &str = "郵便番号";
    /// オフセットを持たない日時の形（標本）。
    const CIVIL_FORM: TemporalForm = TemporalForm::DateTime {
        offset: OffsetPolicy::Forbidden,
    };
    /// オフセットを要求する日時の形（標本）。
    const INSTANT_FORM: TemporalForm = TemporalForm::DateTime {
        offset: OffsetPolicy::Required,
    };

    fn sheet_id() -> SheetId {
        SheetId::from_str(SHEET).expect("標本のシート識別子が解析できない")
    }

    fn text(value: &str) -> CellValue {
        CellValue::Text(value.to_owned())
    }

    fn decimal(value: &str) -> CellValue {
        CellValue::Decimal(value.to_owned())
    }

    fn attachment_id() -> AttachmentId {
        AttachmentId::from_bytes(b"attachment")
    }

    /// 比較用の 10 進数の端点（値の正準形）。
    fn canonical(value: &str) -> DecimalCanonical {
        decimal::canonicalize(value).expect("標本の 10 進数が解析できない")
    }

    /// 比較用の日付の端点。
    fn date(value: &str) -> TemporalValue {
        TemporalForm::Date
            .parse(value)
            .expect("標本の日付が解析できない")
    }

    /// 比較用の civil 日時の端点。
    fn civil(value: &str) -> TemporalValue {
        CIVIL_FORM
            .parse(value)
            .expect("標本の civil 日時が解析できない")
    }

    /// コンパイル済みの書式（標本）。
    fn pattern(source: &str) -> TextPattern {
        TextPattern::compile(source, POSITION).expect("標本のパターンがコンパイルできない")
    }

    /// 設計表の各種別に対応する変種を、それぞれのパラメータつきで構築する。
    ///
    /// **ワイルドカードの無い網羅マッチ**であるため、設計表に種別が増えればここで
    /// コンパイルが壊れる（変種の取りこぼしを型の上で検出する）。
    fn validator_for(kind: TypeKind) -> ColumnValidator {
        match kind {
            TypeKind::Int => ColumnValidator::Int {
                bounds: Bounds::new(
                    Some(CellValue::Int(0)),
                    Some(CellValue::Int(10)),
                    Some(0),
                    Some(10),
                ),
            },
            TypeKind::Float => ColumnValidator::Float {
                bounds: Bounds::new(
                    Some(CellValue::Float(0.0)),
                    Some(CellValue::Float(1.0)),
                    Some(0.0),
                    Some(1.0),
                ),
            },
            TypeKind::Decimal => ColumnValidator::Decimal {
                digits: DecimalDigits::new(4, 1).expect("4 >= 1 かつ 1 <= 4"),
                bounds: Bounds::new(
                    Some(decimal("0.0")),
                    Some(decimal("9.9")),
                    Some(canonical("0.0")),
                    Some(canonical("9.9")),
                ),
            },
            TypeKind::Text => ColumnValidator::Text {
                constraints: TextConstraints::new(Some(1), Some(8), Some(pattern("^[a-z]+$")))
                    .expect("1 <= 8"),
            },
            TypeKind::Bool => ColumnValidator::Bool,
            TypeKind::Date => ColumnValidator::Date {
                bounds: Bounds::new(
                    Some(text("2026-01-01")),
                    Some(text("2026-12-31")),
                    Some(date("2026-01-01")),
                    Some(date("2026-12-31")),
                ),
            },
            TypeKind::DateTime => ColumnValidator::DateTime {
                form: CIVIL_FORM,
                bounds: Bounds::new(
                    Some(text("2026-01-01T00:00:00")),
                    None,
                    Some(civil("2026-01-01T00:00:00")),
                    None,
                ),
            },
            TypeKind::Enum => ColumnValidator::Enum {
                choices: vec!["赤".into(), "青".into()].into_boxed_slice(),
            },
            TypeKind::Ref => ColumnValidator::Ref { sheet: sheet_id() },
            TypeKind::Attachment => ColumnValidator::Attachment,
            TypeKind::Object => ColumnValidator::Object {
                fields: vec![FieldValidator::new(
                    "色",
                    true,
                    ColumnValidator::Enum {
                        choices: vec!["赤".into()].into_boxed_slice(),
                    },
                )]
                .into_boxed_slice(),
            },
            TypeKind::Array => ColumnValidator::Array {
                items: Box::new(ColumnValidator::Int {
                    bounds: Bounds::new(None, None, None, None),
                }),
                min_items: Some(1),
                max_items: Some(3),
            },
            TypeKind::Any => ColumnValidator::Any,
            TypeKind::Custom => ColumnValidator::Custom {
                id: CustomTypeId::new(CUSTOM_ID),
                imp: Arc::new(PostalCode::new(CUSTOM_ID)),
            },
        }
    }

    /// 検証器の変種を設計表の種別へ写す。**ワイルドカードの無い網羅マッチ**であるため、
    /// 変種が増えればここでコンパイルが壊れる。
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

    /// 検証器が実装（`Arc<dyn CustomType>`）を保持するか。**ワイルドカードの無い網羅マッチ**
    /// であるため、変種が増えればここでコンパイルが壊れる。
    fn holds_an_implementation(validator: &ColumnValidator) -> bool {
        match validator {
            ColumnValidator::Custom { .. } => true,
            ColumnValidator::Int { .. }
            | ColumnValidator::Float { .. }
            | ColumnValidator::Decimal { .. }
            | ColumnValidator::Text { .. }
            | ColumnValidator::Bool
            | ColumnValidator::Date { .. }
            | ColumnValidator::DateTime { .. }
            | ColumnValidator::Enum { .. }
            | ColumnValidator::Ref { .. }
            | ColumnValidator::Attachment
            | ColumnValidator::Object { .. }
            | ColumnValidator::Array { .. }
            | ColumnValidator::Any => false,
        }
    }

    /// 名前つきフィールドの集合（`object` の標本）。
    fn nested_object() -> CellValue {
        CellValue::Nested(NestedValue::Object(vec![("色".to_owned(), text("赤"))]))
    }

    /// 同一型の並び（`array` の標本）。
    fn nested_array() -> CellValue {
        CellValue::Nested(NestedValue::Array(vec![CellValue::Int(1)]))
    }

    /// `CellValue` の閉じた 8 変種（入れ子はオブジェクトと配列の双方）。
    fn every_cell_variant() -> Vec<CellValue> {
        vec![
            CellValue::Null,
            CellValue::Bool(true),
            CellValue::Int(-7),
            CellValue::Float(1.5),
            decimal("1.50"),
            text("あ"),
            nested_object(),
            nested_array(),
            CellValue::Attachment(attachment_id()),
        ]
    }

    /// 標本の拡張型: `〒` で始まるテキストだけを受理する。判定の回数を数えるため、
    /// 値なしが実装へ渡らないことの検査にも使う。
    struct PostalCode {
        id: CustomTypeId,
        calls: AtomicUsize,
    }

    impl PostalCode {
        fn new(id: &str) -> Self {
            Self {
                id: CustomTypeId::new(id),
                calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl CustomType for PostalCode {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match value {
                CellValue::Text(found) if found.starts_with('〒') => Ok(CustomVerdict::Accepted),
                CellValue::Text(_) => Ok(CustomVerdict::rejected("〒 で始まらない")),
                _ => Ok(CustomVerdict::rejected("テキストでない")),
            }
        }
    }

    /// 標本の拡張型: 判定が必ず失敗する（要件 11.5 の隔離を検査する）。
    struct Unresponsive {
        id: CustomTypeId,
    }

    impl Unresponsive {
        fn new(id: &str) -> Self {
            Self {
                id: CustomTypeId::new(id),
            }
        }
    }

    impl CustomType for Unresponsive {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, _value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            Err(CustomTypeFailure::new("打ち切った"))
        }
    }

    /// 標本の拡張型: 一括判定だけを上書きする（1 件用の判定は呼ばれない）。
    struct BatchOnly {
        id: CustomTypeId,
        batch_calls: AtomicUsize,
    }

    impl BatchOnly {
        fn new(id: &str) -> Self {
            Self {
                id: CustomTypeId::new(id),
                batch_calls: AtomicUsize::new(0),
            }
        }

        fn batch_calls(&self) -> usize {
            self.batch_calls.load(Ordering::SeqCst)
        }
    }

    impl CustomType for BatchOnly {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, _value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            Err(CustomTypeFailure::new("1 件用の判定は使わない"))
        }

        fn validate_batch(
            &self,
            values: &[CellValue],
            out: &mut dyn FnMut(usize, CustomVerdict),
        ) -> Result<(), CustomTypeFailure> {
            self.batch_calls.fetch_add(1, Ordering::SeqCst);
            for (index, value) in values.iter().enumerate() {
                let verdict = match value {
                    CellValue::Text(found) if found.starts_with('〒') => CustomVerdict::Accepted,
                    _ => CustomVerdict::rejected("〒 で始まらない"),
                };
                out(index, verdict);
            }
            Ok(())
        }
    }

    /// 設計の型カタログ表の全種別に、対応する変種が 1 つずつ存在する（tasks.md 4.2）。
    #[test]
    fn every_catalog_kind_has_its_own_variant() {
        for kind in TypeKind::ALL {
            let validator = validator_for(kind);
            assert_eq!(
                validator_kind(&validator),
                kind,
                "設計表の種別 {kind:?} に対応する変種が無い"
            );
        }
    }

    /// 範囲を持つ変種が、宣言された端点と比較用の端点をインラインに保持する（tasks.md 4.2）。
    #[test]
    fn range_variants_hold_the_declared_and_the_comparable_bounds() {
        match validator_for(TypeKind::Int) {
            ColumnValidator::Int { bounds } => {
                assert_eq!(bounds.declared_min(), Some(&CellValue::Int(0)));
                assert_eq!(bounds.declared_max(), Some(&CellValue::Int(10)));
                assert_eq!(bounds.min, Some(0));
                assert_eq!(bounds.max, Some(10));
            }
            other => panic!("整数の変種でない: {other:?}"),
        }
        match validator_for(TypeKind::Decimal) {
            ColumnValidator::Decimal { digits, bounds } => {
                assert_eq!(digits.precision(), 4);
                assert_eq!(digits.scale(), 1);
                assert_eq!(bounds.declared_max(), Some(&decimal("9.9")));
                assert_eq!(bounds.max.as_ref(), Some(&canonical("9.9")));
            }
            other => panic!("10 進数の変種でない: {other:?}"),
        }
        match validator_for(TypeKind::Date) {
            ColumnValidator::Date { bounds } => {
                assert_eq!(bounds.declared_min(), Some(&text("2026-01-01")));
                assert_eq!(bounds.min, Some(date("2026-01-01")));
            }
            other => panic!("日付の変種でない: {other:?}"),
        }
        match validator_for(TypeKind::DateTime) {
            ColumnValidator::DateTime { form, bounds } => {
                assert_eq!(form, CIVIL_FORM);
                assert_eq!(bounds.min, Some(civil("2026-01-01T00:00:00")));
                assert_eq!(bounds.max, None);
            }
            other => panic!("日時の変種でない: {other:?}"),
        }
    }

    /// 文字列の変種が、文字数の範囲とコンパイル済みのパターンを保持する（tasks.md 4.2）。
    #[test]
    fn text_variant_holds_the_length_bounds_and_the_compiled_pattern() {
        match validator_for(TypeKind::Text) {
            ColumnValidator::Text { constraints } => {
                assert_eq!(constraints.min_length(), Some(1));
                assert_eq!(constraints.max_length(), Some(8));
                let pattern = constraints.pattern().expect("書式が保持されていない");
                assert_eq!(pattern.as_str(), "^[a-z]+$");
            }
            other => panic!("文字列の変種でない: {other:?}"),
        }
    }

    /// 列挙の変種が選択肢を保持する（tasks.md 4.2）。
    #[test]
    fn enum_variant_holds_the_choices() {
        match validator_for(TypeKind::Enum) {
            ColumnValidator::Enum { choices } => {
                assert_eq!(choices.len(), 2);
                assert_eq!(choices[0].as_ref(), "赤");
                assert_eq!(choices[1].as_ref(), "青");
            }
            other => panic!("列挙の変種でない: {other:?}"),
        }
    }

    /// 入れ子のオブジェクトの変種が、フィールドの名前・必須・検証器を保持する（tasks.md 4.2）。
    #[test]
    fn object_variant_holds_its_fields_with_presence() {
        match validator_for(TypeKind::Object) {
            ColumnValidator::Object { fields } => {
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].name(), "色");
                assert!(fields[0].required());
                assert_eq!(validator_kind(fields[0].validator()), TypeKind::Enum);
            }
            other => panic!("オブジェクトの変種でない: {other:?}"),
        }
    }

    /// 配列の変種が、要素の型の検証器と要素数の範囲を保持する（tasks.md 4.2）。
    #[test]
    fn array_variant_holds_its_item_validator_and_item_bounds() {
        match validator_for(TypeKind::Array) {
            ColumnValidator::Array {
                items,
                min_items,
                max_items,
            } => {
                assert_eq!(validator_kind(&items), TypeKind::Int);
                assert_eq!(min_items, Some(1));
                assert_eq!(max_items, Some(3));
            }
            other => panic!("配列の変種でない: {other:?}"),
        }
    }

    /// シート間参照の変種が参照先シートを保持する（tasks.md 4.2。要件 9.1）。
    #[test]
    fn ref_variant_holds_the_sheet() {
        match validator_for(TypeKind::Ref) {
            ColumnValidator::Ref { sheet } => assert_eq!(sheet, sheet_id()),
            other => panic!("参照の変種でない: {other:?}"),
        }
    }

    /// 拡張型の変種が識別子と実装を保持する（tasks.md 4.2。要件 11.2）。
    #[test]
    fn custom_variant_holds_the_identifier_and_the_implementation() {
        match validator_for(TypeKind::Custom) {
            ColumnValidator::Custom { id, imp } => {
                assert_eq!(id.as_str(), CUSTOM_ID);
                assert_eq!(imp.id().as_str(), CUSTOM_ID);
            }
            other => panic!("拡張型の変種でない: {other:?}"),
        }
    }

    /// 実装（間接参照）を持つのは拡張型の変種だけである（tasks.md 4.2。要件 10.6）。
    #[test]
    fn only_the_custom_variant_holds_an_implementation() {
        for kind in TypeKind::ALL {
            let validator = validator_for(kind);
            assert_eq!(
                holds_an_implementation(&validator),
                kind == TypeKind::Custom,
                "{kind:?} の実装の保持が設計と食い違う"
            );
        }
    }

    /// 判定の結果は適合か違反のいずれか一方である（要件 2.7）。
    #[test]
    fn a_value_is_judged_either_conforming_or_violating() {
        let validator = validator_for(TypeKind::Int);
        assert_eq!(
            validator.check(&CellValue::Int(5)),
            ColumnVerdict::Conforming
        );
        assert_eq!(
            validator.check(&text("5")),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::Int
            })
        );
    }

    /// 整数の範囲の端点は含む（design.md の `min` / `max`。要件 4.5）。
    #[test]
    fn integer_bounds_are_inclusive() {
        let validator = ColumnValidator::Int {
            bounds: Bounds::new(
                Some(CellValue::Int(0)),
                Some(CellValue::Int(10)),
                Some(0),
                Some(10),
            ),
        };
        assert_eq!(
            validator.check(&CellValue::Int(0)),
            ColumnVerdict::Conforming
        );
        assert_eq!(
            validator.check(&CellValue::Int(10)),
            ColumnVerdict::Conforming
        );
        let expected = ColumnVerdict::Violating(ColumnViolation::OutOfRange {
            min: Some(CellValue::Int(0)),
            max: Some(CellValue::Int(10)),
        });
        assert_eq!(validator.check(&CellValue::Int(-1)), expected);
        assert_eq!(validator.check(&CellValue::Int(11)), expected);
    }

    /// 開いた端点はその側を制限しない（要件 4.5）。
    #[test]
    fn an_open_bound_does_not_restrict_its_side() {
        let validator = ColumnValidator::Int {
            bounds: Bounds::new(None, Some(CellValue::Int(0)), None, Some(0)),
        };
        assert_eq!(
            validator.check(&CellValue::Int(-1_000)),
            ColumnVerdict::Conforming
        );
        assert_eq!(
            validator.check(&CellValue::Int(1)),
            ColumnVerdict::Violating(ColumnViolation::OutOfRange {
                min: None,
                max: Some(CellValue::Int(0)),
            })
        );
    }

    /// 小数の変種は整数を受け入れない（型ドリフトを構造で禁じる。要件 2.1, 2.2）。
    #[test]
    fn float_does_not_accept_an_integer() {
        let validator = validator_for(TypeKind::Float);
        assert_eq!(
            validator.check(&CellValue::Float(0.5)),
            ColumnVerdict::Conforming
        );
        assert_eq!(
            validator.check(&CellValue::Int(0)),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::Float,
            })
        );
        assert_eq!(
            validator.check(&CellValue::Float(1.5)),
            ColumnVerdict::Violating(ColumnViolation::OutOfRange {
                min: Some(CellValue::Float(0.0)),
                max: Some(CellValue::Float(1.0)),
            })
        );
    }

    /// 10 進数は書かれた形の桁で判定し、そのうえで範囲を見る（tasks.md 2.2 の裁定。要件 2.3）。
    #[test]
    fn decimal_checks_the_written_digits_before_the_range() {
        let validator = ColumnValidator::Decimal {
            digits: DecimalDigits::new(4, 1).expect("4 >= 1 かつ 1 <= 4"),
            bounds: Bounds::new(
                Some(decimal("0.0")),
                Some(decimal("9.9")),
                Some(canonical("0.0")),
                Some(canonical("9.9")),
            ),
        };
        assert_eq!(validator.check(&decimal("1.5")), ColumnVerdict::Conforming);
        // 値は同じでも、書かれた小数部が宣言より長い（`scale = 1`）
        let exceeded = ColumnVerdict::Violating(ColumnViolation::PrecisionExceeded {
            precision: 4,
            scale: 1,
        });
        assert_eq!(validator.check(&decimal("1.50")), exceeded);
        // 文法に一致しない `Decimal`（上流の脱出口）も桁の違反に落ちる
        assert_eq!(validator.check(&decimal("abc")), exceeded);
        assert_eq!(
            validator.check(&decimal("99.0")),
            ColumnVerdict::Violating(ColumnViolation::OutOfRange {
                min: Some(decimal("0.0")),
                max: Some(decimal("9.9")),
            })
        );
        assert_eq!(
            validator.check(&CellValue::Int(1)),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::Decimal,
            })
        );
    }

    /// 文字列は文字数で長さを数え、書式はコンパイル済みのパターンで照合する
    /// （tasks.md 2.4 の裁定。要件 4.5）。
    #[test]
    fn text_checks_the_length_in_characters_then_the_pattern() {
        let validator = ColumnValidator::Text {
            constraints: TextConstraints::new(Some(2), Some(4), Some(pattern("^[a-z]+$")))
                .expect("2 <= 4"),
        };
        assert_eq!(validator.check(&text("ab")), ColumnVerdict::Conforming);
        let out_of_range = ColumnVerdict::Violating(ColumnViolation::LengthOutOfRange {
            min: Some(2),
            max: Some(4),
        });
        assert_eq!(validator.check(&text("a")), out_of_range);
        assert_eq!(validator.check(&text("abcde")), out_of_range);
        assert_eq!(
            validator.check(&text("AB")),
            ColumnVerdict::Violating(ColumnViolation::PatternMismatch {
                pattern: "^[a-z]+$".into(),
            })
        );
        assert_eq!(
            validator.check(&CellValue::Int(1)),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::Text,
            })
        );
        // 長さはバイト長ではなく文字数である（日本語が 3 倍に数えられない）
        let japanese = ColumnValidator::Text {
            constraints: TextConstraints::new(Some(2), Some(2), None).expect("2 <= 2"),
        };
        assert_eq!(japanese.check(&text("日本")), ColumnVerdict::Conforming);
    }

    /// 日付は正準表記に厳密一致する `Text` だけを受け入れる（tasks.md 2.3 の裁定。要件 2.4, 7.3）。
    #[test]
    fn date_requires_the_canonical_spelling() {
        let validator = validator_for(TypeKind::Date);
        assert_eq!(
            validator.check(&text("2026-09-12")),
            ColumnVerdict::Conforming
        );
        let not_a_date = ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
            kind: TypeKind::Date,
        });
        assert_eq!(validator.check(&text("2026/09/12")), not_a_date);
        assert_eq!(validator.check(&text("2026-9-12")), not_a_date);
        assert_eq!(validator.check(&text("2026-09-12T00:00:00")), not_a_date);
        assert_eq!(
            validator.check(&text("2025-12-31")),
            ColumnVerdict::Violating(ColumnViolation::OutOfRange {
                min: Some(text("2026-01-01")),
                max: Some(text("2026-12-31")),
            })
        );
    }

    /// 日時は宣言されたオフセットの扱いに応じて形を分ける（要件 2.4。design.md の `offset`）。
    #[test]
    fn datetime_distinguishes_the_offset_policies() {
        let civil_column = ColumnValidator::DateTime {
            form: CIVIL_FORM,
            bounds: Bounds::new(None, None, None, None),
        };
        assert_eq!(
            civil_column.check(&text("2026-09-12T10:30:00")),
            ColumnVerdict::Conforming
        );
        assert_eq!(
            civil_column.check(&text("2026-09-12T10:30:00Z")),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::DateTime,
            })
        );
        let instant_column = ColumnValidator::DateTime {
            form: INSTANT_FORM,
            bounds: Bounds::new(None, None, None, None),
        };
        assert_eq!(
            instant_column.check(&text("2026-09-12T10:30:00Z")),
            ColumnVerdict::Conforming
        );
        assert_eq!(
            instant_column.check(&text("2026-09-12T10:30:00+09:00")),
            ColumnVerdict::Conforming
        );
        assert_eq!(
            instant_column.check(&text("2026-09-12T10:30:00")),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::DateTime,
            })
        );
    }

    /// 列挙は選択肢そのものを見る（要件 4.5）。
    #[test]
    fn enum_checks_membership() {
        let validator = validator_for(TypeKind::Enum);
        assert_eq!(validator.check(&text("赤")), ColumnVerdict::Conforming);
        assert_eq!(
            validator.check(&text("緑")),
            ColumnVerdict::Violating(ColumnViolation::ChoiceNotAllowed {
                choices: vec!["赤".into(), "青".into()].into_boxed_slice(),
            })
        );
        assert_eq!(
            validator.check(&CellValue::Int(0)),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::Enum,
            })
        );
    }

    /// シート間参照と添付参照は値の**形**だけを見る（実在は一括経路。要件 9.2, 9.6）。
    #[test]
    fn ref_and_attachment_check_the_shape_only() {
        let reference = validator_for(TypeKind::Ref);
        assert_eq!(
            reference.check(&text("01K4ANRRG004HMASW9NF6YY101")),
            ColumnVerdict::Conforming
        );
        assert_eq!(
            reference.check(&CellValue::Int(0)),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::Ref,
            })
        );
        let attachment = validator_for(TypeKind::Attachment);
        assert_eq!(
            attachment.check(&CellValue::Attachment(attachment_id())),
            ColumnVerdict::Conforming
        );
        assert_eq!(
            attachment.check(&text("01K4ANRRG004HMASW9NF6YY101")),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::Attachment,
            })
        );
    }

    /// 入れ子は形と要素数だけをここで判定する（内側へ降りるのは `validate` 層。タスク 5.1）。
    #[test]
    fn object_and_array_check_the_shape_and_the_item_count() {
        let object = validator_for(TypeKind::Object);
        assert_eq!(object.check(&nested_object()), ColumnVerdict::Conforming);
        assert_eq!(
            object.check(&nested_array()),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::Object,
            })
        );
        let array = ColumnValidator::Array {
            items: Box::new(ColumnValidator::Any),
            min_items: Some(1),
            max_items: Some(2),
        };
        assert_eq!(array.check(&nested_array()), ColumnVerdict::Conforming);
        let out_of_range = ColumnVerdict::Violating(ColumnViolation::LengthOutOfRange {
            min: Some(1),
            max: Some(2),
        });
        assert_eq!(
            array.check(&CellValue::Nested(NestedValue::Array(vec![]))),
            out_of_range
        );
        assert_eq!(
            array.check(&CellValue::Nested(NestedValue::Array(vec![
                CellValue::Null,
                CellValue::Null,
                CellValue::Null,
            ]))),
            out_of_range
        );
        assert_eq!(
            array.check(&nested_object()),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::Array,
            })
        );
    }

    /// 真偽は真偽値だけを受け入れる（要件 2.1, 2.2）。
    #[test]
    fn bool_accepts_only_a_boolean() {
        let validator = ColumnValidator::Bool;
        assert_eq!(
            validator.check(&CellValue::Bool(false)),
            ColumnVerdict::Conforming
        );
        assert_eq!(
            validator.check(&CellValue::Int(0)),
            ColumnVerdict::Violating(ColumnViolation::TypeMismatch {
                kind: TypeKind::Bool,
            })
        );
    }

    /// ANY は保持可能なすべての値を適合として扱う（要件 2.5）。
    #[test]
    fn any_accepts_every_cell_variant() {
        let validator = validator_for(TypeKind::Any);
        for value in every_cell_variant() {
            assert_eq!(
                validator.check(&value),
                ColumnVerdict::Conforming,
                "ANY が {value:?} を拒否した"
            );
        }
    }

    /// 値なしはどの変種でも適合であり、拡張型の実装へも渡らない（design.md の型カタログ表）。
    #[test]
    fn null_is_conforming_and_is_not_handed_to_the_implementation() {
        for kind in TypeKind::ALL {
            let validator = validator_for(kind);
            assert_eq!(
                validator.check(&CellValue::Null),
                ColumnVerdict::Conforming,
                "{kind:?} が値なしを拒否した"
            );
        }
        let spy = Arc::new(PostalCode::new(CUSTOM_ID));
        let imp: Arc<dyn CustomType> = spy.clone();
        let validator = ColumnValidator::Custom {
            id: CustomTypeId::new(CUSTOM_ID),
            imp,
        };
        assert_eq!(validator.check(&CellValue::Null), ColumnVerdict::Conforming);
        assert_eq!(spy.calls(), 0, "値なしが拡張型の実装へ渡った");
    }

    /// 拡張型の判定は組込型と同じ形（適合・拒否・失敗の 3 経路）で返る（要件 11.3, 11.5）。
    #[test]
    fn the_custom_variant_reports_the_implementation_verdict_in_the_common_shape() {
        let spy = Arc::new(PostalCode::new(CUSTOM_ID));
        let imp: Arc<dyn CustomType> = spy.clone();
        let validator = ColumnValidator::Custom {
            id: CustomTypeId::new(CUSTOM_ID),
            imp,
        };
        assert_eq!(
            validator.check(&text("〒100-0001")),
            ColumnVerdict::Conforming
        );
        assert_eq!(spy.calls(), 1, "1 件の判定が 1 回でない");
        assert_eq!(
            validator.check(&text("100-0001")),
            ColumnVerdict::Violating(ColumnViolation::CustomRejected {
                id: CUSTOM_ID.into(),
                reason: "〒 で始まらない".into(),
            })
        );
        let failing: Arc<dyn CustomType> = Arc::new(Unresponsive::new(CUSTOM_ID));
        let validator = ColumnValidator::Custom {
            id: CustomTypeId::new(CUSTOM_ID),
            imp: failing,
        };
        assert_eq!(
            validator.check(&text("〒100-0001")),
            ColumnVerdict::Violating(ColumnViolation::CustomFailed {
                id: CUSTOM_ID.into(),
                reason: "打ち切った".into(),
            })
        );
    }

    /// 拡張型の実装は検証器から取り出せ、一括判定を列ごとに 1 回だけ呼べる
    /// （要件 10.6, 11.6。行ごとの動的ディスパッチを避ける経路）。
    #[test]
    fn the_custom_variant_carries_the_implementation_onto_the_batch_path() {
        let spy = Arc::new(BatchOnly::new(CUSTOM_ID));
        let imp: Arc<dyn CustomType> = spy.clone();
        let validator = ColumnValidator::Custom {
            id: CustomTypeId::new(CUSTOM_ID),
            imp,
        };
        let values = [text("〒100-0001"), text("100-0001"), text("〒100-0002")];
        let ColumnValidator::Custom { imp, .. } = &validator else {
            panic!("拡張型の変種でない");
        };
        let mut verdicts = Vec::new();
        imp.validate_batch(&values, &mut |index, verdict| {
            verdicts.push((index, verdict))
        })
        .expect("一括判定が失敗した");
        assert_eq!(spy.batch_calls(), 1, "一括判定が列ごとに 1 回でない");
        assert_eq!(verdicts.len(), values.len());
        assert_eq!(verdicts[0].1, CustomVerdict::Accepted);
        assert_eq!(verdicts[1].1, CustomVerdict::rejected("〒 で始まらない"));
        assert_eq!(verdicts[2].1, CustomVerdict::Accepted);
    }

    /// 検証器はスレッドを跨いで共有できる（拡張型の実装が `Send + Sync` を要求するため）。
    #[test]
    fn the_validator_can_be_shared_across_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ColumnValidator>();
        assert_send_sync::<FieldValidator>();
        assert_send_sync::<ColumnVerdict>();
    }
}

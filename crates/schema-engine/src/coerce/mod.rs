//! 型強制の規則表（design.md「コンポーネントとファイルの対応」の `Coercer`。tasks.md 6.1。
//! 要件 2.6, 7.1, 7.2, 7.3, 7.4, 7.5, 7.6）。
//!
//! 打ち込まれた値を列の型として解釈し直す唯一の場所である。変換するのは**情報が失われない
//! 組み合わせ**だけで、解釈に迷う綴りは値を変えずに残す（design.md「Coerce Layer / Coercer」
//! の「Intent」）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本層は `compile` 層の右隣であり、[`ColumnValidator`] と
//! [`CompiledSchema`] を参照する。本層を参照するのは `write` 層（タスク 6.2）である。
//! **`validate` 層とは互いに依存しない** — 強制してから検証する順序は `write` 層が組み立てる
//! （design.md「System Flows / 書き込み経路の判定」）。したがって本モジュールは違反の型
//! （`validate::report`）を参照せず、宣言の誤り（[`SchemaError`](crate::error::SchemaError)）
//! も組み立てない。
//!
//! # 変換の規則表（要件 7.5 の「一意な規則」。design.md の表そのもの）
//!
//! | 入力の変種 | 列の型 | 変換 |
//! |---|---|---|
//! | `Text` | `int` | 10 進整数の文法に厳密一致すれば [`CellValue::Int`] |
//! | `Text` | `float` | 10 進小数の文法に一致し、総量が失われない（有限で、非ゼロの値が `0` にならない）とき [`CellValue::Float`] |
//! | `Text` | `decimal` | 文法と宣言された桁に収まれば [`CellValue::Decimal`]（**綴りは逐語のまま**） |
//! | `Text` | `bool` | `true` / `false` の 2 綴りだけ [`CellValue::Bool`] |
//! | `Text` | `date` / `datetime` | 変換しない（正準表記に一致すればそのまま適合する） |
//! | `Int` | `float` | `f64` で厳密に表せるときだけ [`CellValue::Float`] |
//! | `Int` | `decimal` | 宣言された桁に収まるときだけ [`CellValue::Decimal`] |
//! | `Float` | `decimal` | **変換しない**（2 進小数から 10 進へ移す時点で桁が決まらない） |
//! | `Float` | `int` | **変換しない**（小数部の切り捨ては情報が失われる） |
//! | `Decimal` | `int` / `float` | **変換しない**（同上） |
//! | 任意 | `any` | **変換しない**（要件 2.6。`any` は解釈しない） |
//! | 任意 | `custom` | 変換しない（下記「拡張型の行」） |
//!
//! **表に行が無い組み合わせは変換しない。** `Text` は `text` の列でそのまま、`Int` は `int` の
//! 列でそのままであり、`Int` → `text` や `Text` → `enum` のような組も表に無い（組込種別の
//! 間を勝手に行き来させない）。値の変種が既に列の型と一致している場合も変換しない。
//!
//! # 変換しない場合は違反になる（要件 7.2, 7.3）
//!
//! 情報が失われる組み合わせ（小数 → 整数、2 進小数 → 10 進数、宣言より桁の多い値）と、
//! 地域設定・実行環境で結果が変わりうる綴り（桁区切り、`2026/09/12` や和暦、曜日名、
//! タイムゾーンの略号、全角の数字）は、**値を変えずに返す**。返された値は列の型に適合しない
//! ため、続く検証（`validate` 層）が違反として報告する。本モジュール自身が違反を組み立てない
//! のは、違反の表現が `validate` 層の所有物であり、層の鎖が一方向だからである。
//!
//! `float` の列はこの規則の線引きが最も微妙な行である。[`CellValue::Float`] は近似を許す
//! 型であり、10 進表記を最近傍の `f64` へ丸めること自体は**変換の定義に含める** —
//! `0.1` が厳密に表せないことまで拒否すると、`float` への変換がほとんど成立しなくなる。
//! 拒否するのは**値が総量として失われる**場合だけである: 結果が非有限になるオーバーフロー
//! （`1e400`。`inf` / `NaN` の綴りも同じ門）と、非ゼロの有効数字を持つ入力が `0.0` に
//! なるアンダーフロー（`1e-400`）である。値が 0 の綴り（`0.0` / `0e10`）はアンダーフロー
//! ではない。判定は仮数部の数字を ASCII の 1 パスで走査して行い、指数部の数字は数えない
//! （[`decimal::scan`] は指数が `i64` を超える入力を `None` とするため、その場合の
//! アンダーフローを見逃す。判定は `f64` が受理した綴りだけを対象にするので仮数部で足りる）。
//!
//! 本モジュールが門を二重に持たないことも同じ理由である。10 進数の文法と桁は
//! [`DecimalDigits::accepts`]、日時の正準表記は `TemporalForm::accepts`、真偽の綴りと
//! 範囲・長さ・選択肢は `ColumnValidator` が単一の源として持つ。**規則表は組み合わせごとに
//! 「変換するかしないか」だけを決める。**
//!
//! # 決定性（要件 7.6）
//!
//! 規則は**入力の変種と列の型の組**だけで決まり、時計・環境変数・地域設定・乱数を見ない。
//! 同じ入力は常に同じ結果になる。整数と小数の綴りの解析は Rust の標準の実装
//! （`i64::from_str` / `f64::from_str`）に委ね、自前の走査を持たない — 標準の実装は地域設定を
//! 見ず、同じ綴りを常に同じ値へ写す。
//!
//! # 入れ子の内側は規則表に無い
//!
//! 規則表は**列の型と値の形の組**で決まる。`object` / `array` の内側のフィールドは表に
//! 行が無いため変換しない。内側の値の判定は `validate` 層の 1 セル判定（タスク 5.1）が
//! 位置つきで行う。
//!
//! # 拡張型の行（design.md の表との差）
//!
//! design.md の表は `custom` を「拡張型の実装に委ねる」とするが、`CustomType` トレイト
//! （タスク 4.1 が確定）は値を受け取って変換する口を持たない（`id` / `validate` /
//! `validate_batch` / `canonicalize` のみ）。**読む手段の無い規則を作らない**ため、本モジュールは
//! `custom` の列を変換しない。値は書かれたとおりに実装へ渡り、受け入れ可否は実装の `validate`
//! が決める — 変換の可否を実装が持つという点で表の意図は満たされる。実装が変換を必要とするなら、
//! design.md の `Revalidation Triggers` が挙げる「`CustomType` トレイトのシグネチャ」の変更と
//! してトレイトと本表を同時に足す。
//!
//! # 使用不能な列は変換しない（要件 11.7）
//!
//! 未知の `kind`・未登録の拡張型・有限に展開できない型定義の列は、列の型が決まらないため
//! 変換しない（[`coerce`]）。値は書かれたとおりに残り、`validate` 層が `UnusableColumn` として
//! 報告する。

use crate::compile::plan::ColumnValidator;
use crate::compile::{ColumnIndex, CompiledSchema};
use crate::types::decimal::{self, DecimalDigits};
use crate::types::Acceptance;
use document_format::CellValue;

/// 変換の記録（design.md「Coerce Layer / Coercer」の Service Interface。要件 7.4）。
///
/// 呼び出し元が「変換が起きたこと」と「変換前の値」を確認できる形である（要件 7.4）。
#[derive(Debug, Clone, PartialEq)]
pub enum Coercion {
    /// 変換していない（表に行が無い組み合わせ、または変換の条件を満たさなかった）。
    Unchanged,
    /// 変換した。変換前の値を持つ（要件 7.4）。
    Converted {
        /// 変換前の値（書かれたとおり）。
        from: CellValue,
    },
}

/// 1 つの値の強制の結果（要件 7.4）。
///
/// 結果の値と、その値が入力から変わったかどうかを分けて持つ。変換しなかった場合は
/// [`Coerced::value`] が入力そのものであり、複製も確保もしない。
#[derive(Debug, Clone, PartialEq)]
pub struct Coerced {
    /// 強制後の値。変換しなかったときは入力そのもの。
    pub value: CellValue,
    /// 変換が起きたかと、起きたときの変換前の値（要件 7.4）。
    pub coercion: Coercion,
}

/// 計画の列添字を指定して 1 つの値を強制する（design.md「System Flows / 書き込み経路の判定」
/// の強制の段。要件 7.1〜7.6）。
///
/// **使用不能な列**（未知の種別・未登録の拡張型・有限に展開できない型定義。要件 11.7）と
/// 計画の外の添字は、列の型が決まらないため変換しない。何も変わらない値が返り、その値は
/// `validate` 層で `UnusableColumn` の違反になる。
pub fn coerce(schema: &CompiledSchema, column: ColumnIndex, value: CellValue) -> Coerced {
    match schema.validator(column) {
        Some(target) => coerce_by(target, value),
        None => Coerced {
            value,
            coercion: Coercion::Unchanged,
        },
    }
}

/// 列の型（検証器）と値の組から変換を決める（design.md「Coerce Layer / Coercer」の規則表
/// そのもの。要件 7.1〜7.6）。
///
/// 表に行が無い組み合わせと、行はあるが条件を満たさない値は [`Coercion::Unchanged`] を返す。
/// どちらも値は入力のままである。
pub fn coerce_by(target: &ColumnValidator, value: CellValue) -> Coerced {
    // 表の行だけをここに並べる。`None` は「変換しない」であり、表に行が無い組み合わせ
    // （`_` の腕）と同じ行き先である。
    let converted = match (target, &value) {
        // `any` は保持可能なすべての値をそのまま受理し、解釈しない（要件 2.6）。
        (ColumnValidator::Any, _) => None,
        // `custom` は変換の口を持たないため、値を書かれたとおりに実装へ渡す
        // （モジュール docs「拡張型の行」）。
        (ColumnValidator::Custom { .. }, _) => None,
        // `Text` → `int`。
        (ColumnValidator::Int { .. }, CellValue::Text(text)) => {
            int_from_text(text).map(CellValue::Int)
        }
        // `Text` → `float`。
        (ColumnValidator::Float { .. }, CellValue::Text(text)) => {
            float_from_text(text).map(CellValue::float)
        }
        // `Int` → `float`。
        (ColumnValidator::Float { .. }, CellValue::Int(found)) => {
            int_to_float(*found).map(CellValue::float)
        }
        // `Text` → `decimal`。
        (ColumnValidator::Decimal { digits, .. }, CellValue::Text(text)) => {
            text_to_decimal(*digits, text)
        }
        // `Int` → `decimal`。
        (ColumnValidator::Decimal { digits, .. }, CellValue::Int(found)) => {
            int_to_decimal(*digits, *found)
        }
        // `Text` → `bool`。
        (ColumnValidator::Bool, CellValue::Text(text)) => bool_from_text(text).map(CellValue::Bool),
        // 残りは表に行が無い組み合わせである。`Text` → `date` / `datetime` もここに落ちる —
        // 正準表記に一致する `Text` はそのまま適合し、一致しない `Text` は書式を推測せずに
        // 違反になる（要件 7.3）。`Float` → `int` / `decimal`、`Decimal` → `int` / `float`、
        // `Int` → `text`、`Text` → `enum` / `ref`、入れ子の値も同じである。
        _ => None,
    };
    match converted {
        Some(converted) => Coerced {
            value: converted,
            coercion: Coercion::Converted { from: value },
        },
        None => Coerced {
            value,
            coercion: Coercion::Unchanged,
        },
    }
}

/// `Text` を整数の値へ読み替える（規則表の `Text` → `int` の行。要件 7.1, 7.2）。
///
/// 文法は [`i64::from_str`] が定めるもの**そのもの**であり、本モジュールは自前の走査を
/// 持たない。すなわち 10 進の ASCII 数字だけを任意の符号 1 つとともに受け付け、前後の空白・
/// 桁区切り・小数点・指数形・全角の数字を受け付けず、あふれる値も受け付けない（`Err` は
/// 変換しないことを意味する。要件 7.2）。
///
/// 先頭の 0（`007`）と明示的な正符号（`+7`）は受理する。**値の情報が変わらない**ためである —
/// [`CellValue::Int`] が持つ情報は数値そのものであり、綴りは情報ではない（綴りを保つ契約を
/// 持つ [`CellValue::Decimal`] とは異なる。`types::decimal` の docs「保存される文字列は
/// 変えない」）。
fn int_from_text(text: &str) -> Option<i64> {
    text.parse().ok()
}

/// `Text` を小数の値へ読み替える（規則表の `Text` → `float` の行。要件 7.1, 7.2, 7.3）。
///
/// 解析は [`f64::from_str`] に委ねる。この実装は地域設定を見ず、同じ綴りを常に同じ値へ
/// 写す（要件 7.3, 7.6）。**値が総量として失われる綴りだけ**を拒否する（線引きの根拠は
/// モジュール docs「変換しない場合は違反になる」）。
///
/// - 結果が非有限（`±Infinity` / `NaN`）になるオーバーフロー。`1e400` は `f64` の範囲を
///   越えて `inf` になり、`inf` / `NaN` の綴りも同じ門で落ちる。非有限の浮動小数は
///   `document-format` が書き出し時に拒否する値でもある（[`CellValue::Float`] の docs）。
/// - アンダーフロー。非ゼロの有効数字を持つ入力が `0.0` になる場合である。判定は
///   **仮数部**（`e` / `E` より前）に `1`〜`9` の数字があるかで行い、値が 0 の綴り
///   （`0.0` / `0e10`）と区別する。指数部の数字（`0e10` の `10`）は値の有効数字ではない。
///
/// 有効桁が `f64` に収まらない綴りは最近傍へ丸められるが、これは**変換の定義に含める**
/// — `0.1` は 2 進小数で厳密に表せず、`9007199254740993` は 2^53 へ丸められる。丸めの
/// 仕方も一意である（同じ綴りが同じ `f64` になる）。失われるのは綴りの精度であって、
/// 値の総量ではない。
fn float_from_text(text: &str) -> Option<f64> {
    let parsed: f64 = text.parse().ok()?;
    if !parsed.is_finite() {
        return None;
    }
    // 非ゼロの仮数が 0 になるのはアンダーフローであり、値が総量として失われている。
    if parsed == 0.0 && mantissa_has_nonzero_digit(text) {
        return None;
    }
    Some(parsed)
}

/// 仮数部に `1`〜`9` の数字があるか（指数部は数えない）。
///
/// [`float_from_text`] のアンダーフロー判定だけが使う。`f64::from_str` が受理した綴りだけが
/// 渡るため、`e` / `E` より前が仮数部である。**[`decimal::scan`] は使わない** — あれは指数が
/// `i64` を超える入力を `None` とするため、`1e-99999999999999999999` のような綴りの
/// アンダーフローを見逃す（`f64` はこれを `0.0` に落とし、値の総量が失われる）。指数の
/// 大きさに依らず仮数部だけを見れば足りるので、ASCII の 1 パスで走査し確保もしない。
fn mantissa_has_nonzero_digit(text: &str) -> bool {
    for byte in text.bytes() {
        match byte {
            b'e' | b'E' => return false,
            b'1'..=b'9' => return true,
            _ => {}
        }
    }
    false
}

/// 整数の値を小数の値へ写す（規則表の `Int` → `float` の行。要件 7.2）。
///
/// `f64` が厳密に表せる整数は ±2^53 に限らない（`2^54` のような値も表せる）ため、
/// 「2^53 以下」ではなく**丸めが起きないこと**を判定する。`i64` → `f64` は最近接へ丸めるので、
/// 丸めた結果を `i128` へ戻して元の値と比べる。`i64` へ戻すと `i64::MAX` 付近で飽和して
/// 丸めを見逃す（`(i64::MAX as f64) as i64` が `i64::MAX` になる）ため、必ず `i128` へ戻す。
fn int_to_float(value: i64) -> Option<f64> {
    let rounded = value as f64;
    (i128::from(value) == rounded as i128).then_some(rounded)
}

/// `Text` を 10 進数の値へ写す（規則表の `Text` → `decimal` の行。要件 7.1, 7.2）。
///
/// **綴りは逐語のまま** [`CellValue::Decimal`] にする（`Decimal` は文字列のまま往復する契約。
/// `types::decimal` の docs）。文法と桁の判定は [`DecimalDigits::accepts`] に委ね、本モジュールは
/// 桁勘定を写さない — 桁の検査の単一の源は `types::decimal` である。
///
/// 桁を宣言していない列（`{"kind":"decimal"}`。要件 2.3 の裁定）は桁の制約が無いため、
/// **文法だけ**を [`decimal::scan`] で確かめて変換する（宣言に無い制約を捏造しない）。
fn text_to_decimal(digits: Option<DecimalDigits>, text: &str) -> Option<CellValue> {
    let fits = match digits {
        Some(digits) => digits.accepts(text) == Acceptance::Conforming,
        None => decimal::scan(text).is_some(),
    };
    fits.then(|| CellValue::Decimal(text.to_owned()))
}

/// 整数の値を 10 進数の値へ写す（規則表の `Int` → `decimal` の行。要件 7.2）。
///
/// 宣言された桁に収まらない整数は変換しない — `precision` を超える値を移すと桁が失われる
/// （`decimal(3,2)` は `123` を収められない）。桁の判定は [`DecimalDigits::accepts`] に委ね、
/// `i64::to_string` が作る綴り（`-?\d+`）をそのまま渡す。桁を宣言していない列は桁の制約が
/// 無いため変換する（要件 2.3 の裁定）。
fn int_to_decimal(digits: Option<DecimalDigits>, value: i64) -> Option<CellValue> {
    let text = value.to_string();
    let fits = match digits {
        Some(digits) => digits.accepts(&text) == Acceptance::Conforming,
        None => true,
    };
    fits.then(|| CellValue::Decimal(text))
}

/// `Text` を真偽の値へ写す（規則表の `Text` → `bool` の行。要件 7.1, 7.3）。
///
/// 受理するのは `true` と `false` の 2 綴りだけである。`1` / `0` / `TRUE` / `はい` / `真` は
/// 解釈しない — 言語と地域で意味が変わる綴りであり、要件 7.3 が禁じる「地域設定または実行
/// 環境によって結果が変わりうる解釈」にあたる。
fn bool_from_text(text: &str) -> Option<bool> {
    match text {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile_declaration;
    use crate::compile::plan::ColumnVerdict;
    use crate::declaration::{ColumnDecl, Constraints, DeclaredKind, Schema, TypeDecl};
    use crate::registry::{
        CustomType, CustomTypeFailure, CustomTypeId, CustomVerdict, TypeRegistry,
    };
    use crate::types::datetime::OffsetPolicy;
    use crate::types::TypeKind;
    use document_format::NestedValue;
    use std::sync::Arc;

    /// テキストの値を組み立てる。
    fn text(value: &str) -> CellValue {
        CellValue::Text(value.to_owned())
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

    /// 列 1 本だけの計画を組み立てる。
    fn plan_with(kind: TypeKind, constraints: Constraints) -> CompiledSchema {
        compiled_with(
            vec![column("値", declared(kind, constraints))],
            &TypeRegistry::new(),
        )
    }

    /// 列の集合を持つ計画をコンパイルする。
    fn compiled_with(columns: Vec<ColumnDecl>, registry: &TypeRegistry) -> CompiledSchema {
        compile_declaration(&Schema { columns }, &[], registry)
            .expect("標本の宣言はコンパイルできる")
    }

    /// 計画の唯一の列の添字。
    fn first() -> ColumnIndex {
        ColumnIndex::new(0)
    }

    /// 標本の列の型へ値を強制する。
    fn coerce_in(plan: &CompiledSchema, value: CellValue) -> Coerced {
        coerce(plan, first(), value)
    }

    /// 標本の値が列の型に適合しないかを、列の型の判定で確かめる。
    ///
    /// 変換しない値の行き先は `validate` 層（違反）であるが、本層は `validate` 層を参照
    /// できないため、判定の段（[`ColumnVerdict`]）で「適合しない」ことを確かめる。
    /// `validate` 層はこの判定を違反の理由へ写す（タスク 5.1）。
    fn violates(plan: &CompiledSchema, value: &CellValue) -> bool {
        let target = plan.validator(first()).expect("標本の列は使用可能");
        matches!(target.check(value), ColumnVerdict::Violating(_))
    }

    /// 値が変換されず、そのまま違反になることを確かめる（要件 7.2, 7.3）。
    fn assert_refused(plan: &CompiledSchema, value: CellValue) {
        let input = value.clone();
        let result = coerce_in(plan, value);
        assert_eq!(
            Coercion::Unchanged,
            result.coercion,
            "変換しない組み合わせが変換された"
        );
        assert_eq!(input, result.value, "変換しない値が書き換えられた");
        assert!(
            violates(plan, &result.value),
            "変換しない値が違反にならない（適合するなら変換の必要が無い）"
        );
    }

    /// 値が入力から変換され、変換前の値が記録されることを確かめる（要件 7.1, 7.4）。
    fn assert_converted(plan: &CompiledSchema, value: CellValue, expected: CellValue) -> Coercion {
        let input = value.clone();
        let result = coerce_in(plan, value);
        assert_eq!(
            Coercion::Converted { from: input },
            result.coercion,
            "変換前の値が記録されていない（要件 7.4）"
        );
        assert_eq!(expected, result.value, "変換の結果が規則表と違う");
        assert!(
            !violates(plan, &result.value),
            "変換した値が列の型に適合しない"
        );
        result.coercion
    }

    /// 表に行が無い組み合わせで値が変えられないことを確かめる。
    fn assert_untouched(plan: &CompiledSchema, value: CellValue) {
        let input = value.clone();
        let result = coerce_in(plan, value);
        assert_eq!(Coercion::Unchanged, result.coercion);
        assert_eq!(input, result.value);
    }

    /// `〒` で始まるテキストだけを受理する標本の拡張型。
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

    /// 標本の拡張型を 1 つ登録した辞書を組み立てる。
    fn registry_with_postal_code() -> TypeRegistry {
        let mut registry = TypeRegistry::new();
        registry
            .register(Arc::new(PostalCode {
                id: CustomTypeId::new("postal-code"),
            }))
            .expect("標本の拡張型は登録できる");
        registry
    }

    /// 10 進数の桁数を宣言する。
    fn digits(precision: u32, scale: u32) -> DecimalDigits {
        DecimalDigits::new(precision, scale).expect("標本の桁数は妥当")
    }

    // 規則表の `Text` → `int` の行。

    /// 10 進整数の文法に厳密一致するテキストが整数になる（要件 7.1）。
    #[test]
    fn text_is_read_as_an_integer_when_it_is_an_integer_literal() {
        let plan = plan_with(TypeKind::Int, Constraints::default());
        assert_converted(&plan, text("42"), CellValue::Int(42));
        assert_converted(&plan, text("-7"), CellValue::Int(-7));
        // 先頭の 0 と明示的な正符号は値の情報を変えない。
        assert_converted(&plan, text("007"), CellValue::Int(7));
        assert_converted(&plan, text("+7"), CellValue::Int(7));
    }

    /// 小数・指数形・桁区切りは整数として解釈しない（要件 7.2, 7.3）。
    #[test]
    fn text_that_is_not_an_integer_literal_is_never_read_as_an_integer() {
        let plan = plan_with(TypeKind::Int, Constraints::default());
        assert_refused(&plan, text("1.5"));
        assert_refused(&plan, text("1.0"));
        assert_refused(&plan, text("1e3"));
        assert_refused(&plan, text("1 000"));
        assert_refused(&plan, text("1,000"));
        assert_refused(&plan, text("１２３"));
        assert_refused(&plan, text(""));
        // あふれる値は情報が失われるため変換しない（要件 7.2）。
        assert_refused(&plan, text("9223372036854775808"));
    }

    // 規則表の `Text` → `float` の行。

    /// 10 進小数の文法に一致するテキストが小数になる（要件 7.1）。
    #[test]
    fn text_is_read_as_a_float_when_it_is_a_decimal_literal() {
        let plan = plan_with(TypeKind::Float, Constraints::default());
        assert_converted(&plan, text("1.5"), CellValue::Float(1.5));
        assert_converted(&plan, text("-0.5"), CellValue::Float(-0.5));
        assert_converted(&plan, text("1e3"), CellValue::Float(1000.0));
        assert_converted(&plan, text("42"), CellValue::Float(42.0));
        // 文法の端（`types::decimal` の文法と同じ綴りを受理する）。
        assert_converted(&plan, text("+1.5"), CellValue::Float(1.5));
        assert_converted(&plan, text(".5"), CellValue::Float(0.5));
        assert_converted(&plan, text("5."), CellValue::Float(5.0));
    }

    /// 有限の値にならない綴りは小数として解釈しない（要件 7.2）。
    #[test]
    fn a_float_literal_outside_the_finite_range_is_never_read_as_a_float() {
        let plan = plan_with(TypeKind::Float, Constraints::default());
        assert_refused(&plan, text("1e400"));
        assert_refused(&plan, text("-1e400"));
        assert_refused(&plan, text("inf"));
        assert_refused(&plan, text("-inf"));
        assert_refused(&plan, text("NaN"));
        assert_refused(&plan, text("1,5"));
        assert_refused(&plan, text("1_000"));
        assert_refused(&plan, text("￥1"));
    }

    /// 値が総量として失われる綴りは小数として解釈しない（要件 7.2）。
    #[test]
    fn a_float_literal_whose_magnitude_is_lost_is_never_read_as_a_float() {
        let plan = plan_with(TypeKind::Float, Constraints::default());
        // アンダーフロー: 有効数字が非ゼロなのに結果が 0 になる（情報が総量として失われる）。
        assert_refused(&plan, text("1e-400"));
        assert_refused(&plan, text("-1e-400"));
        assert_refused(&plan, text("1e-9999"));
        // 指数が `i64` を超える綴りも同じである（`decimal::scan` は `None` になる）。
        assert_refused(&plan, text("1e-99999999999999999999"));
        // オーバーフロー: 結果が非有限になる（同じく総量が失われる）。
        assert_refused(&plan, text("1e400"));
    }

    /// 値が 0 である綴りはアンダーフローではない（要件 7.2）。
    #[test]
    fn a_float_literal_that_is_genuinely_zero_is_read_as_zero() {
        let plan = plan_with(TypeKind::Float, Constraints::default());
        assert_converted(&plan, text("0"), CellValue::Float(0.0));
        assert_converted(&plan, text("0.0"), CellValue::Float(0.0));
        assert_converted(&plan, text("0.000"), CellValue::Float(0.0));
        assert_converted(&plan, text("0e10"), CellValue::Float(0.0));
        // 指数が `i64` を超えても、仮数部が 0 ならアンダーフローではない。
        assert_converted(
            &plan,
            text("0.0000e-99999999999999999999"),
            CellValue::Float(0.0),
        );
    }

    /// 最近傍への丸めは変換の定義に含まれ、拒否しない（裁定の固定。要件 7.2）。
    #[test]
    fn a_float_literal_that_rounds_to_the_nearest_value_is_still_read_as_a_float() {
        let plan = plan_with(TypeKind::Float, Constraints::default());
        // `0.1` は 2 進小数で厳密に表せないが、総量は失われない。
        assert_converted(&plan, text("0.1"), CellValue::Float(0.1));
        // 2^53 + 1 は最近傍の 2^53 へ丸められる（同じく総量は失われない）。
        assert_converted(
            &plan,
            text("9007199254740993"),
            CellValue::Float(9_007_199_254_740_992.0),
        );
    }

    // 規則表の `Text` → `decimal` の行。

    /// 宣言された桁に収まるテキストが、綴りを変えずに 10 進数になる（要件 7.1）。
    #[test]
    fn text_is_read_as_a_decimal_verbatim_when_the_declared_digits_hold_it() {
        let plan = plan_with(
            TypeKind::Decimal,
            Constraints {
                digits: Some(digits(2, 1)),
                ..Constraints::default()
            },
        );
        assert_converted(&plan, text("1.5"), CellValue::Decimal("1.5".into()));
        // 綴りは逐語のまま（`Decimal` は文字列のまま往復する契約）。指数形も明示的な正符号も
        // 書き換えない。
        assert_converted(&plan, text("1.5e0"), CellValue::Decimal("1.5e0".into()));
        assert_converted(&plan, text("+1.5"), CellValue::Decimal("+1.5".into()));
    }

    /// 宣言より桁の多いテキストは変換せず違反にする（要件 7.2。tasks.md 2.2 の裁定）。
    #[test]
    fn text_that_exceeds_the_declared_digits_is_never_read_as_a_decimal() {
        let plan = plan_with(
            TypeKind::Decimal,
            Constraints {
                digits: Some(digits(2, 1)),
                ..Constraints::default()
            },
        );
        assert_refused(&plan, text("1.55"));
        assert_refused(&plan, text("1.50"));
        assert_refused(&plan, text("123.5"));
        assert_refused(&plan, text("abc"));
    }

    /// 桁を宣言していない 10 進数の列でも文法は要求する（要件 2.3 の裁定）。
    #[test]
    fn a_decimal_column_without_declared_digits_still_requires_the_grammar() {
        let plan = plan_with(TypeKind::Decimal, Constraints::default());
        assert_converted(&plan, text("1.5"), CellValue::Decimal("1.5".into()));
        assert_converted(
            &plan,
            text("12345678901234567890"),
            CellValue::Decimal("12345678901234567890".into()),
        );
        assert_refused(&plan, text("abc"));
        assert_refused(&plan, text("1,5"));
    }

    // 規則表の `Text` → `bool` の行。

    /// `true` / `false` の 2 綴りだけが真偽になる（要件 7.1, 7.3）。
    #[test]
    fn only_the_two_boolean_spellings_are_read_as_booleans() {
        let plan = plan_with(TypeKind::Bool, Constraints::default());
        assert_converted(&plan, text("true"), CellValue::Bool(true));
        assert_converted(&plan, text("false"), CellValue::Bool(false));
        assert_refused(&plan, text("1"));
        assert_refused(&plan, text("0"));
        assert_refused(&plan, text("TRUE"));
        assert_refused(&plan, text("True"));
        assert_refused(&plan, text("はい"));
        assert_refused(&plan, text("真"));
    }

    // 規則表の `Text` → `date` / `datetime` の行。

    /// 日付と日時は書式を推測せず、正準表記のまま適合させる（要件 7.1, 7.3）。
    #[test]
    fn a_date_and_a_date_time_are_never_reformatted() {
        let date = plan_with(TypeKind::Date, Constraints::default());
        assert_untouched(&date, text("2026-09-12"));
        assert_refused(&date, text("2026/09/12"));
        assert_refused(&date, text("2026年9月12日"));
        assert_refused(&date, text("12.09.2026"));

        let datetime = plan_with(
            TypeKind::DateTime,
            Constraints {
                offset: Some(OffsetPolicy::Required),
                ..Constraints::default()
            },
        );
        // 書かれたオフセットは書き換えない（正準表記は綴りを保つ）。
        assert_untouched(&datetime, text("2026-09-12T10:30:00+09:00"));
        assert_refused(&datetime, text("2026-09-12 10:30:00Z"));
        assert_refused(&datetime, text("2026-09-12T10:30:00JST"));
        assert_refused(&datetime, text("2026-09-12T10:30:00"));
    }

    // 規則表の `Int` → `float` の行。

    /// `f64` で厳密に表せる整数だけが小数になる（要件 7.2）。
    #[test]
    fn an_integer_is_read_as_a_float_only_when_no_bit_is_lost() {
        let plan = plan_with(TypeKind::Float, Constraints::default());
        assert_converted(&plan, CellValue::Int(2), CellValue::Float(2.0));
        // 2^53 を超えても、累乗の倍数は厳密に表せる（丸めが起きない）。
        assert_converted(
            &plan,
            CellValue::Int(1 << 54),
            CellValue::Float(18_014_398_509_481_984.0),
        );
        // 2^53 + 1 は最近接へ丸められて値が変わるため変換しない。
        assert_refused(&plan, CellValue::Int((1 << 53) + 1));
        assert_refused(&plan, CellValue::Int(i64::MAX));
        assert_refused(&plan, CellValue::Int(i64::MIN + 1));
    }

    // 規則表の `Int` → `decimal` の行。

    /// 宣言された桁に収まる整数だけが 10 進数になる（要件 7.2）。
    #[test]
    fn an_integer_is_read_as_a_decimal_only_when_the_declared_digits_hold_it() {
        let plan = plan_with(
            TypeKind::Decimal,
            Constraints {
                digits: Some(digits(5, 2)),
                ..Constraints::default()
            },
        );
        assert_converted(&plan, CellValue::Int(42), CellValue::Decimal("42".into()));
        assert_converted(&plan, CellValue::Int(-42), CellValue::Decimal("-42".into()));
        assert_refused(&plan, CellValue::Int(1234));
    }

    /// 桁を宣言していない 10 進数の列では整数がそのまま 10 進数になる（要件 2.3 の裁定）。
    #[test]
    fn an_integer_is_read_as_a_decimal_when_no_digits_are_declared() {
        let plan = plan_with(TypeKind::Decimal, Constraints::default());
        assert_converted(
            &plan,
            CellValue::Int(1234),
            CellValue::Decimal("1234".into()),
        );
    }

    // 規則表の「変換しない」行。

    /// 小数は整数にも 10 進数にもしない（要件 7.2）。
    #[test]
    fn a_float_is_never_read_as_an_integer_or_a_decimal() {
        let two_digits_one_decimal = Constraints {
            digits: Some(digits(5, 2)),
            ..Constraints::default()
        };
        assert_refused(
            &plan_with(TypeKind::Int, Constraints::default()),
            CellValue::Float(1.5),
        );
        assert_refused(
            &plan_with(TypeKind::Decimal, two_digits_one_decimal),
            CellValue::Float(1.5),
        );
    }

    /// 10 進数は整数にも小数にもしない（要件 7.2）。
    #[test]
    fn a_decimal_is_never_read_as_an_integer_or_a_float() {
        assert_refused(
            &plan_with(TypeKind::Int, Constraints::default()),
            CellValue::Decimal("1.5".into()),
        );
        assert_refused(
            &plan_with(TypeKind::Float, Constraints::default()),
            CellValue::Decimal("1.5".into()),
        );
    }

    /// `ANY` の列はどんな値も解釈しない（要件 2.6）。
    #[test]
    fn an_any_column_never_coerces_anything() {
        let plan = plan_with(TypeKind::Any, Constraints::default());
        assert_untouched(&plan, text("42"));
        assert_untouched(&plan, CellValue::Int(42));
        assert_untouched(&plan, CellValue::Float(1.5));
        assert_untouched(&plan, CellValue::Decimal("1.5".into()));
        assert_untouched(&plan, CellValue::Bool(true));
        assert_untouched(&plan, CellValue::Null);
    }

    /// 表に行が無い組み合わせは変換しない。
    #[test]
    fn combinations_that_are_absent_from_the_table_are_never_converted() {
        assert_untouched(
            &plan_with(TypeKind::Text, Constraints::default()),
            text("42"),
        );
        assert_untouched(
            &plan_with(TypeKind::Text, Constraints::default()),
            CellValue::Int(42),
        );
        assert_untouched(
            &plan_with(TypeKind::Bool, Constraints::default()),
            CellValue::Bool(true),
        );
        assert_untouched(
            &plan_with(TypeKind::Int, Constraints::default()),
            CellValue::Int(42),
        );
        // 添付参照と入れ子の値は形がそのまま型の値である。
        assert_untouched(
            &plan_with(TypeKind::Any, Constraints::default()),
            CellValue::Nested(NestedValue::Array(vec![CellValue::Int(1)])),
        );
        assert_untouched(
            &plan_with(TypeKind::Any, Constraints::default()),
            CellValue::Null,
        );
    }

    /// 拡張型の列は値を変えず、受け入れ可否を実装に委ねる（規則表の `custom` の行）。
    #[test]
    fn a_custom_column_defers_to_its_implementation() {
        let registry = registry_with_postal_code();
        let constraints = Constraints {
            custom_type: Some("postal-code".into()),
            ..Constraints::default()
        };
        let plan = compiled_with(
            vec![column("住所", declared(TypeKind::Custom, constraints))],
            &registry,
        );
        let value = text("〒100-0001");
        assert_untouched(&plan, value.clone());
        // 実装へそのまま渡り、実装の判定がそのまま結果になる（本層は解釈しない）。
        let target = plan.validator(first()).expect("標本の列は使用可能");
        assert_eq!(
            ColumnVerdict::Conforming,
            target.check(&value),
            "標本の拡張型が値を受理しない"
        );
    }

    /// 使用不能な列は変換しない（要件 11.7）。
    #[test]
    fn an_unusable_column_never_coerces() {
        let plan = compiled_with(
            vec![column(
                "値",
                TypeDecl::Kind {
                    kind: DeclaredKind::Unknown("future-kind".into()),
                    constraints: Constraints::default(),
                },
            )],
            &TypeRegistry::new(),
        );
        assert!(plan.is_unusable(first()));
        assert_untouched(&plan, text("42"));
        assert_untouched(&plan, CellValue::Int(42));
    }

    /// 計画の外の添字も変換しない。
    #[test]
    fn a_column_outside_the_plan_never_coerces() {
        let plan = plan_with(TypeKind::Int, Constraints::default());
        let out = ColumnIndex::new(9);
        assert_eq!(
            Coerced {
                value: text("42"),
                coercion: Coercion::Unchanged,
            },
            coerce(&plan, out, text("42"))
        );
    }

    // 規則表そのものの性質（要件 7.4, 7.5, 7.6）。

    /// 規則表を横断する標本（列の型 × 値）を作る。規則の性質は個別の行ではなく表の全体に
    /// ついて成り立つため、行をまたいで 1 つの並びにまとめる。
    fn corpus() -> Vec<(CompiledSchema, CellValue)> {
        let decimal_2_1 = Constraints {
            digits: Some(digits(2, 1)),
            ..Constraints::default()
        };
        let datetime = Constraints {
            offset: Some(OffsetPolicy::Required),
            ..Constraints::default()
        };
        let kinds = [
            (TypeKind::Int, Constraints::default()),
            (TypeKind::Float, Constraints::default()),
            (TypeKind::Decimal, decimal_2_1),
            (TypeKind::Decimal, Constraints::default()),
            (TypeKind::Text, Constraints::default()),
            (TypeKind::Bool, Constraints::default()),
            (TypeKind::Date, Constraints::default()),
            (TypeKind::DateTime, datetime),
            (TypeKind::Any, Constraints::default()),
        ];
        let values = [
            text("42"),
            text("1.5"),
            text("true"),
            text("abc"),
            text("2026-09-12"),
            CellValue::Int(42),
            CellValue::Float(1.5),
            CellValue::Decimal("1.5".into()),
            CellValue::Bool(true),
            CellValue::Null,
        ];
        let mut corpus = Vec::new();
        for (kind, constraints) in kinds {
            let plan = plan_with(kind, constraints);
            for value in &values {
                corpus.push((plan.clone(), value.clone()));
            }
        }
        corpus
    }

    /// 同じ入力は常に同じ変換結果になる（要件 7.5, 7.6）。
    #[test]
    fn the_same_input_always_yields_the_same_conversion() {
        let corpus = corpus();
        let forward: Vec<Coerced> = corpus
            .iter()
            .map(|(plan, value)| coerce_in(plan, value.clone()))
            .collect();
        let backward: Vec<Coerced> = corpus
            .iter()
            .rev()
            .map(|(plan, value)| coerce_in(plan, value.clone()))
            .collect();
        assert_eq!(
            forward,
            backward.into_iter().rev().collect::<Vec<_>>(),
            "同じ入力の変換結果が実行の順に依存する"
        );

        let again: Vec<Coerced> = corpus
            .iter()
            .map(|(plan, value)| coerce_in(plan, value.clone()))
            .collect();
        assert_eq!(forward, again, "同じ入力の変換結果が繰り返しで変わる");
    }

    /// 値が変わったときだけ変換が記録され、変換前の値が入力そのものである（要件 7.4）。
    #[test]
    fn a_value_changes_only_when_the_conversion_is_reported() {
        for (plan, value) in corpus() {
            let input = value.clone();
            let result = coerce_in(&plan, value);
            match result.coercion {
                Coercion::Unchanged => assert_eq!(
                    input, result.value,
                    "変換していないのに値が変わった（要件 7.4）"
                ),
                Coercion::Converted { from } => {
                    assert_eq!(input, from, "変換前の値が入力と違う（要件 7.4）");
                    assert_ne!(
                        input, result.value,
                        "変換したと記録されたのに値が同じ（要件 7.4）"
                    );
                }
            }
        }
    }

    /// 変換した値は必ず列の型に適合する（規則表の各行が「変換する」と言った結果）。
    #[test]
    fn a_converted_value_always_conforms_to_the_column_type() {
        for (plan, value) in corpus() {
            let result = coerce_in(&plan, value.clone());
            if result.coercion != Coercion::Unchanged {
                let target = plan.validator(first()).expect("標本の列は使用可能");
                assert_eq!(
                    ColumnVerdict::Conforming,
                    target.check(&result.value),
                    "変換した値が列の型に適合しない: {:?}",
                    result.value
                );
            }
        }
    }
}

//! schema / document の値 ⇄ JavaScript の値の写像（tasks.md 2.2。要件 4.2, 4.3, 4.6）。
//!
//! マクロが読む値と書く値の**対応表**と、その往復をここ 1 箇所に置く。V8 との実際の
//! 受け渡し（`serde_v8` の `to_v8` / `from_v8` と op の登録）はタスク 3.2 が担い、
//! 本モジュールは**写像の規則と型**だけを持つ（V8 に依存しない。design.md
//! 「Technology Stack」の Value mapping）。
//!
//! # 2 つの写像（どちらを使うかは列の型が決める）
//!
//! JS の値は型を持たない。`Text("1.50")` と `Decimal("1.50")` は同じ文字列、
//! `Int(5)` と `Float(5.0)` は同じ数値、`Attachment(hex)` も同じ文字列である。したがって
//! 写像の仕方は**値の変種だけでは決まらない**。決めるのは次の 2 つである。
//!
//! 1. **列に宣言された型**（[`TypeKind`]。`schema-engine` の組込型カタログ）— 型が 1 つの
//!    変種だけを受け入れるなら、その型が変種を決める（[`Mapping::Plain`]。要件 4.2）
//! 2. **値自身**（[`Mapping::SelfDescribing`]）— 型が変種を決めない位置では、値が
//!    変種を語る形で渡す（要件 4.3）
//!
//! [`Mapping::of`] が 1 の判定であり、規則は「型が受け入れる変種が**ただ 1 つ**で、それが
//! 素の JS の値を持つ（`null` / 真偽 / 数値 / 文字列 / 添付）こと」である。この規則は
//! `schema-engine` の [`TypeKind::accepted_variants`] から導くため、カタログに種別が
//! 足されてもここに一覧を書き写す必要がない（足された種別が `Nested` だけを受け入れるなら
//! 自動的に自己記述的になり、表と実装がずれない）。
//!
//! | 写像 | 列の型（`TypeKind`） |
//! |---|---|
//! | [`Mapping::Plain`] | `int` / `float` / `decimal` / `text` / `bool` / `date` / `datetime` / `enum` / `ref` / `attachment` |
//! | [`Mapping::SelfDescribing`] | `object` / `array` / `any` / `custom` |
//!
//! **入れ子（`Nested`）は素の写像を使う列でも自己記述的になる**（[`to_js`]）。`Nested` は
//! それ自身が構造であり、「その列の型に対応する JS の値」という素の値が無い。さらに
//! 葉の変種は（列の型が `text` でも）JS では区別できないため、葉まで含めて往復を一致させる
//! には値が変種を語る形が要る。
//!
//! # 読み（`CellValue` → JavaScript の値）
//!
//! | 変種 | [`Mapping::Plain`] の列 | [`Mapping::SelfDescribing`] の列 |
//! |---|---|---|
//! | `Null` | `null` | 自己記述的な形（JS の `null` になる） |
//! | `Bool` | 真偽 | 同上 |
//! | `Int` / `Float` | 数値（JS の数値は 1 種類） | 同上 |
//! | `Text` | 文字列 | 同上 |
//! | `Decimal` | 文字列（**逐語**。桁を失わない） | 同上 |
//! | `Attachment` | 文字列（正準の小文字 hex 64 文字） | 同上 |
//! | `Nested` | 自己記述的な形 | 同上 |
//!
//! # 書き（JavaScript の値 → `CellValue`）
//!
//! [`Mapping::Plain`] の列は、素の JS の値の**形**だけから変種を決める（[`JsScalar::to_value`]。
//! 型が与えるのは変種であって値ではない）:
//!
//! | 列の型 | JS の値 | 戻る変種 |
//! |---|---|---|
//! | `int` | 数値 | 整数（`i64` の範囲内）なら `Int`、それ以外は `Float`（違反として保持） |
//! | `float` | 数値 | `Float`（`-0.0` は `0.0` へ畳む。上流の規則） |
//! | `decimal` | 文字列 | `Decimal`（**逐語**） |
//! | `text` / `date` / `datetime` / `enum` / `ref` | 文字列 | `Text` |
//! | `bool` | 真偽 | `Bool` |
//! | `attachment` | 文字列 | hex として復号できれば `Attachment`、できなければ `Text`（違反として保持） |
//! | どれでも | `null` | `Null`（値なし。受理の可否は列の必須指定が決める） |
//!
//! 形が型に合わない値（数値を `text` の列へ、文字列を `int` の列へ、object を `int` の列へ）も
//! **捨てない**。形のまま写して上流へ渡し、違反として保持・提示できる状態にする（要件 5.2）。
//!
//! [`Mapping::SelfDescribing`] の列と、object / array の値は、変種を形から決められないため
//! **自己記述的な形として復号する**（タスク 3.2 が `CellValue` の復号へ回す。下記）。
//!
//! # 自己記述的な形とは何か（実装をここに置かない理由）
//!
//! 自己記述的な形は `document_format::CellValue` 自身の `Serialize` / `Deserialize` が定める
//! 表現である（[`document_format::to_json_bytes`] / [`document_format::from_json_bytes`] と
//! 同じ規則。オブジェクトの構造はそのまま JS の object / array になり、区別できない葉だけが
//! `$t` のタグと `$` の逃がしで**変種を語る**）。したがって `Text("1.50")` は
//! `{"$t":"text","v":"1.50"}` として、`Decimal("1.50")` は `"1.50"` として渡り、**どの変種も
//! 構造ごと正確に往復する**（[`document_format::from_json_bytes`] の往復保証がそのまま効く）。
//!
//! この規則の実装は上流に**1 つだけ**置く。本モジュールに同じ規則を書き写すと、上流が規則を
//! 直したときにこちらが黙って古い規則で写す状態が生まれる（「第二の規約を作らない」。
//! `document-format` の `json` モジュールと同じ規律）。本モジュールが持つのは
//! **どの位置がこの形になるか**（[`Mapping`]）と、その値への**参照**（[`JsView::SelfDescribing`]）
//! だけである。
//!
//! # コピーを増やさない（要件 11.1, 11.3）
//!
//! [`to_js`] は値への**借用**を返す（[`JsView::Scalar`] の文字列は [`Cow::Borrowed`]、
//! [`JsView::SelfDescribing`] と入れ子は値への参照）。10 万行の範囲の読みで、行ごと・セル
//! ごとにセル値の木を複製しない。唯一の例外は添付である: hex は内容から組み立てる綴りで
//! あり、保持しているバイト列を借りられない（`data-grid` の `display_text` が同じ判断を
//! している）。
//!
//! # 写像が保たないもの（境界の限界。黙って化けさせないために明記する）
//!
//! - **JS の数値は 1 種類**（`f64`）である。2^53 を越える `Int` は JS の数値で正確に
//!   表せない（document-format の design 表が整数を 2^53 の内側に限っている）。また
//!   `Int` / `Float` の区別と、`Text` / `Decimal` / 添付 hex の区別は、**素の写像では
//!   列の型が決める**: 型に適合しない値は、読み戻すと**その列の型が受け入れる変種**に
//!   なる（変種まで一致させたい位置は自己記述的な形を使う。入れ子・`any`・`custom` は
//!   初めからそうである）。
//! - **非有限の数値**（`NaN` / `±Infinity`）は `Float` のまま写す。拒否は書き出し側
//!   （`document-format` の門）が行う（そこで `NaN` を `null` へ黙って変換しないのは
//!   上流の規則である）。
//! - **種別の綴り**（`host.columns` が返す `kind`）も本モジュールが決める
//!   （[`type_kind_name`]）。`.d.ts` の `TypeKind` の合併型（タスク 3.3 の生成器が
//!   `TypeKind::ALL` を走査して組む）と、実行時に op が返す札が**同じ 1 箇所から出る**
//!   ようにするためである（食い違えばマクロ側の型検査が嘘になる）
//! - マクロ向けの型定義（要件 4.6 / 10.1）では、これらの型が 1 つの型として現れる:
//!   `type CellValue = null | boolean | number | string | CellValue[] | { [key: string]:
//!   CellValue };`（タスク 3.3 の生成器が宣言表と本モジュールの型から
//!   `types/macro-host.d.ts` へ出す。境界の型名 `CellValue` は宣言表 2.1 が要求している）。

use std::borrow::Cow;

use document_format::{AttachmentId, CellValue};
use schema_engine::{AcceptedVariants, TypeKind, ValueVariant};

/// 列の型が値の写像をどう決めるか（モジュール docs の表が規則の全体）。
///
/// 判定は [`Mapping::of`] の 1 箇所だけであり、読み（[`to_js`]）と書き（タスク 3.2 の
/// 復号の分岐）が同じ判定を読む。両者が別々に判定すると、片方だけを直したときに
/// 「読めたが書けない値」が生まれる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapping {
    /// 型が変種を決める（素の JS の値として渡し、型が戻りを決める。要件 4.2）。
    Plain,
    /// 型が変種を決めない（値が自分で変種を語る形で渡す。要件 4.3）。
    SelfDescribing,
}

impl Mapping {
    /// 列の型から写像を決める（規則はモジュール docs）。
    ///
    /// 判定は `schema-engine` のカタログ（[`TypeKind::accepted_variants`]）から導く。
    /// 種別の一覧をここへ書き写さないのは、カタログに種別が足されたときに**この関数が
    /// 黙って古くならない**ようにするためである（`Nested` だけを受け入れる種別が足されれば
    /// 自動的に [`Mapping::SelfDescribing`] になる）。
    pub fn of(kind: TypeKind) -> Self {
        match scalar_variant(kind) {
            Some(_) => Mapping::Plain,
            None => Mapping::SelfDescribing,
        }
    }
}

/// 種別の**マクロから見える綴り**（`host.columns` が返す `kind`。要件 4.1, 4.2）。
///
/// 変種名をそのまま使うのは、同じ概念の**兄弟の表**（`crates/app-shell/src/ipc/grid.rs` の
/// `TypeKindTag`。IPC 境界の札）が同じ綴りを採っているためである。**両者の一致は既存の検査が
/// 機械で保っている** — `src-tauri/src/commands/grid.rs` の `type_kind_tag_covers_every_type_kind`
/// が `TypeKind::ALL` の写像と `TypeKindTag::ALL` の並び・綴りを突き合わせる（2026-09-18 に確認）。
/// 綴りをここ 1 箇所に置くのは、`.d.ts` の `TypeKind` の合併型（タスク 3.3 の生成器が
/// `TypeKind::ALL` を走査して組む）と、op が実行時に返す札が**同じ源から出る**ようにする
/// ためである（食い違えばマクロ側の型検査が嘘になる）。
///
/// **申し送り**: 本関数（マクロ境界の綴り）をその検査へ加えるのは、`src-tauri` から本クレートを
/// 参照できるようになってから（層の鎖により本クレートから `app-shell` は参照できない）。
///
/// **網羅的な `match` にしてある**: `schema-engine` のカタログに種別が足されれば
/// ここがコンパイルエラーになり、`.d.ts` の合併型（`TypeKind::ALL` を走査して作る）と
/// 実行時の値のどちらかが黙って古くなることを防ぐ（要件 10.1）。
pub fn type_kind_name(kind: TypeKind) -> &'static str {
    match kind {
        TypeKind::Int => "Int",
        TypeKind::Float => "Float",
        TypeKind::Decimal => "Decimal",
        TypeKind::Text => "Text",
        TypeKind::Bool => "Bool",
        TypeKind::Date => "Date",
        TypeKind::DateTime => "DateTime",
        TypeKind::Enum => "Enum",
        TypeKind::Ref => "Ref",
        TypeKind::Attachment => "Attachment",
        TypeKind::Object => "Object",
        TypeKind::Array => "Array",
        TypeKind::Any => "Any",
        TypeKind::Custom => "Custom",
    }
}

/// 1 つのセルをマクロへ渡すときの、**素の** JS の値（V8 に依存しない鏡）。
///
/// 読みでは [`to_js`] が値への借用で組み立て（文字列は [`Cow::Borrowed`]）、書きでは
/// タスク 3.2 が V8 から取り出した値をこの形で組み立てて [`JsScalar::to_value`] へ渡す。
/// JS の数値は 1 種類（`f64`）であるため、`Int` / `Float` の区別は持たない（区別は列の型が
/// 与える）。
#[derive(Debug, Clone, PartialEq)]
pub enum JsScalar<'a> {
    /// 値なし（JS の `null`）。
    Null,
    /// 真偽（JS の boolean）。
    Bool(bool),
    /// 数値（JS の number。`Int` / `Float` の区別は持たない）。
    Number(f64),
    /// 文字列（JS の string。`Text` / `Decimal` / 添付 hex の区別は列の型が決める）。
    Str(Cow<'a, str>),
}

impl JsScalar<'_> {
    /// 素の JS の値を、列の型が決める変種へ戻す（要件 4.2 の逆。規則はモジュール docs の表）。
    ///
    /// **型が変種を決めない列（[`Mapping::SelfDescribing`]）では `None` を返す** — 変種を
    /// 決めるものが無いためであり、呼び出し側（タスク 3.2）はその値を `CellValue` の復号
    /// へ回す。`Option` にしてあるのは、この分岐を**無視できない**ようにするためである
    /// （`bool` を返すと、開いた型の値が「素の写像で決めた変種」として黙って通る）。
    ///
    /// 形が型に合わない値も写す（捨てない。要件 5.2）。
    pub fn to_value(self, kind: TypeKind) -> Option<CellValue> {
        let variant = scalar_variant(kind)?;
        Some(match (self, variant) {
            // 値なしはどの型でも `Null`（schema-engine の表。受理の可否は列の必須指定が決める）。
            (JsScalar::Null, _) => CellValue::Null,
            (JsScalar::Bool(value), _) => CellValue::Bool(value),
            // 浮動小数の列は `Float`。整数値の `5` でも `Float(5.0)` にするのは、読みが
            // `Int(5)` と `Float(5.0)` を同じ `Number` に畳んでいるためである（行きで
            // 区別を捨てた以上、戻りは型が決める。往復は一致する）。
            (JsScalar::Number(value), ValueVariant::Float) => CellValue::float(value),
            // 整数の列は整数のときだけ `Int`。整数でない数値は `Float` のまま残す（違反として
            // 保持する。要件 5.2）。
            (JsScalar::Number(value), _) => number_of(value),
            // 10 進の列は**逐語の文字列**（`Decimal` は文字列のまま往復する契約であり、
            // 解釈も正規化もしない）。
            (JsScalar::Str(text), ValueVariant::Decimal) => CellValue::Decimal(text.into_owned()),
            // 添付の列は hex として復号できれば `Attachment`、できなければ `Text` として
            // 残す（違反として保持する。要件 5.2）。
            (JsScalar::Str(text), ValueVariant::Attachment) => {
                let text = text.into_owned();
                match AttachmentId::from_hex(&text) {
                    Ok(id) => CellValue::Attachment(id),
                    Err(_) => CellValue::Text(text),
                }
            }
            // 文字列をとる型（`text` / `date` / `datetime` / `enum` / `ref`）は `Text`。
            (JsScalar::Str(text), _) => CellValue::Text(text.into_owned()),
        })
    }
}

/// 1 つのセルがマクロとの境界で取る形。
///
/// どちらの腕になるかは列の型と値の変種が決める（[`to_js`]）。この 2 つの腕が
/// モジュール docs の「2 つの写像」そのものである。
#[derive(Debug, Clone, PartialEq)]
pub enum JsView<'a> {
    /// 素の JS の値（[`Mapping::Plain`] の列のスカラー値。要件 4.2）。
    Scalar(JsScalar<'a>),
    /// 自己記述的な形で渡す値（[`Mapping::SelfDescribing`] の列と、入れ子の値。要件 4.3）。
    ///
    /// 値への参照であり、木を複製しない。JS の値への変換はタスク 3.2 が `CellValue` の
    /// `Serialize`（＝上流の wire の規則）で行う。
    SelfDescribing(&'a CellValue),
}

/// 1 つのセルを JS へ渡す形を決める（要件 4.2, 4.3）。
///
/// 呼ぶ側（タスク 3.2 / 4.1 の範囲の読み）は、セルの列の型を渡す。列の型が分かるのは
/// `columns(sheet)`（ホスト API の `ColumnTypeInfo`。タスク 2.4）が型情報を返すためである。
pub fn to_js(value: &CellValue, kind: TypeKind) -> JsView<'_> {
    if Mapping::of(kind) == Mapping::SelfDescribing {
        return JsView::SelfDescribing(value);
    }
    match value {
        CellValue::Null => JsView::Scalar(JsScalar::Null),
        CellValue::Bool(value) => JsView::Scalar(JsScalar::Bool(*value)),
        // 2^53 を越える整数は JS の数値で正確に表せない（モジュール docs の限界）。
        CellValue::Int(value) => JsView::Scalar(JsScalar::Number(*value as f64)),
        CellValue::Float(value) => JsView::Scalar(JsScalar::Number(*value)),
        // `Text` / `Decimal` は保持している文字列を**借りる**（桁も綴りも変えない）。
        CellValue::Text(text) | CellValue::Decimal(text) => {
            JsView::Scalar(JsScalar::Str(Cow::Borrowed(text)))
        }
        // 添付は正準の小文字 hex（64 文字）。綴りを組み立てるため、ここだけ所有になる。
        CellValue::Attachment(id) => JsView::Scalar(JsScalar::Str(Cow::Owned(id.to_hex()))),
        // 入れ子は型が変種を決める列でも素の値を持たない（モジュール docs）。
        CellValue::Nested(_) => JsView::SelfDescribing(value),
    }
}

/// 列の型が受け入れる**唯一の**変種が、素の JS の値を持つならそれ（写像の規則の全体）。
///
/// `None` は「この型は変種を決めない」を意味する。`schema-engine` のカタログは種別ごとに
/// 受け入れる変種を `Only` / `All` / `Delegated` で持ち、`Only` のうち `Nested` だけを
/// 受け入れる種別（`object` / `array`）と、複数の変種を受け入れる種別（`any`）と、
/// 実装に委ねる種別（`custom`）には、素の値が無い。
fn scalar_variant(kind: TypeKind) -> Option<ValueVariant> {
    let AcceptedVariants::Only(accepted) = kind.accepted_variants() else {
        return None;
    };
    let [variant] = accepted else {
        return None;
    };
    match variant {
        ValueVariant::Bool
        | ValueVariant::Int
        | ValueVariant::Float
        | ValueVariant::Decimal
        | ValueVariant::Text
        | ValueVariant::Attachment => Some(*variant),
        ValueVariant::Null | ValueVariant::Object | ValueVariant::Array => None,
    }
}

/// 数値を `CellValue` にする（整数なら `Int`、それ以外は `Float`）。
///
/// `f64` で `i64` の全域が正確に表せるわけではない（2^53 超）。境界を `i64::MAX as f64`
/// で判定しないのは、この値が 2^63 へ丸め上がるためであり、`2^63` そのものを `Int` に
/// すると `as` の飽和で `i64::MAX` へ化ける（値が変わったことに誰も気づけない）。
fn number_of(value: f64) -> CellValue {
    // 2^63（`i64::MAX as f64` と同じ丸め上がった値）。
    const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;
    if value.fract() == 0.0 && value >= -TWO_POW_63 && value < TWO_POW_63 {
        CellValue::Int(value as i64)
    } else {
        CellValue::float(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_format::NestedValue;
    use schema_engine::Acceptance;

    /// 添付の識別子（内容から決まる。テストの間ずっと同じ）。
    fn attachment() -> AttachmentId {
        AttachmentId::from_bytes(b"jxcel host/value test attachment")
    }

    /// 素の写像を使う列（型が変種を決める）の代表値。モジュール docs の書き表の列である。
    fn plain_cases() -> Vec<(TypeKind, CellValue)> {
        vec![
            (TypeKind::Text, CellValue::Text("在庫 42".to_owned())),
            (TypeKind::Int, CellValue::Int(42)),
            (TypeKind::Float, CellValue::Float(1.5)),
            (TypeKind::Decimal, CellValue::Decimal("1.50".to_owned())),
            (TypeKind::Bool, CellValue::Bool(true)),
            (TypeKind::Date, CellValue::Text("2026-09-18".to_owned())),
            (
                TypeKind::DateTime,
                CellValue::Text("2026-09-18T12:00:00Z".to_owned()),
            ),
            (TypeKind::Enum, CellValue::Text("進行中".to_owned())),
            (
                TypeKind::Ref,
                CellValue::Text("01J3ZK9V2Q0000000000000000".to_owned()),
            ),
            (TypeKind::Attachment, CellValue::Attachment(attachment())),
            // 値なしはどの型でも `null`（欠測）。
            (TypeKind::Text, CellValue::Null),
        ]
    }

    /// 素の写像を使う列のスカラー値は、行きと戻りが一致する。戻りは上流の検証へ渡せる形である
    /// （要件 4.2 / 4.6 の往復）。
    #[test]
    fn plain_values_roundtrip_through_the_declared_type() {
        for (kind, value) in plain_cases() {
            let JsView::Scalar(scalar) = to_js(&value, kind) else {
                panic!("{kind:?} の列が素の写像を使わなかった: {value:?}");
            };
            let restored = scalar
                .to_value(kind)
                .unwrap_or_else(|| panic!("{kind:?} の列で素の写像の戻りが無い: {value:?}"));
            assert_eq!(value, restored, "{kind:?} の列で往復が一致しない");
            // 戻した値が上流の判定で適合すること（違反として残る形でないこと）。
            assert_eq!(
                Acceptance::Conforming,
                kind.accepts(&restored),
                "{kind:?} の列で戻した値が適合しない: {restored:?}"
            );
        }
    }

    /// JS の数値は `Int` と `Float` の区別を持たない。**戻りの変種は列の型が決める**
    /// （行きで `Number` に畳んだ以上、他の決め手が無い）。
    #[test]
    fn numbers_carry_no_variant_the_declared_type_decides() {
        let int_view = to_js(&CellValue::Int(5), TypeKind::Int);
        let float_view = to_js(&CellValue::Float(5.0), TypeKind::Float);
        assert_eq!(int_view, float_view, "同じ数値が別の形になった");
        assert_eq!(
            JsView::Scalar(JsScalar::Number(5.0)),
            int_view,
            "整数が数値にならなかった"
        );

        let JsView::Scalar(scalar) = int_view else {
            panic!("素の写像を使わなかった");
        };
        assert_eq!(
            Some(CellValue::Int(5)),
            scalar.clone().to_value(TypeKind::Int)
        );
        assert_eq!(
            Some(CellValue::Float(5.0)),
            scalar.to_value(TypeKind::Float),
            "浮動小数の列の戻りが `Float` にならなかった"
        );
    }

    /// 整数の境界: `i64` に収まらない数値は `Int` へ飽和させず `Float` として残す
    /// （値が変わったことに誰も気づけないため）。
    #[test]
    fn numbers_beyond_i64_stay_floats() {
        let boundary = 9_223_372_036_854_775_808.0_f64; // 2^63
        assert_eq!(
            Some(CellValue::Int(i64::MIN)),
            JsScalar::Number(-boundary).to_value(TypeKind::Int)
        );
        assert_eq!(
            Some(CellValue::Float(boundary)),
            JsScalar::Number(boundary).to_value(TypeKind::Int)
        );
        // 非有限も `Float` のまま写す（拒否は書き出し側 = 上流の門）。
        assert_eq!(
            Some(CellValue::Float(f64::INFINITY)),
            JsScalar::Number(f64::INFINITY).to_value(TypeKind::Int)
        );
    }

    /// `Decimal` の綴りは逐語で往復する（正規化しない）。
    #[test]
    fn decimal_keeps_its_spelling() {
        let value = CellValue::Decimal("+001.50".to_owned());
        let JsView::Scalar(JsScalar::Str(text)) = to_js(&value, TypeKind::Decimal) else {
            panic!("10 進の列が素の文字列にならなかった");
        };
        assert_eq!("+001.50", text);
        assert_eq!(
            Some(value.clone()),
            JsScalar::Str(text).to_value(TypeKind::Decimal)
        );
    }

    /// 添付は正準の hex を経由して往復し、hex でない文字列は `Text` として残る（捨てない）。
    #[test]
    fn attachments_roundtrip_through_their_hex() {
        let id = attachment();
        let value = CellValue::Attachment(id);
        let JsView::Scalar(JsScalar::Str(hex)) = to_js(&value, TypeKind::Attachment) else {
            panic!("添付の列が素の文字列にならなかった");
        };
        assert_eq!(id.to_hex(), hex);
        assert_eq!(
            Some(CellValue::Attachment(id)),
            JsScalar::Str(hex).to_value(TypeKind::Attachment)
        );
        assert_eq!(
            Some(CellValue::Text("not a hex".to_owned())),
            JsScalar::Str(Cow::Borrowed("not a hex")).to_value(TypeKind::Attachment),
            "hex でない文字列を捨てた"
        );
    }

    /// 形が型に合わない値も写す（捨てない。要件 5.2）。違反は上流の判定が提示する。
    #[test]
    fn values_that_do_not_fit_the_type_are_kept() {
        let restored = JsScalar::Str(Cow::Borrowed("abc"))
            .to_value(TypeKind::Int)
            .expect("整数の列で文字列を捨てた");
        assert_eq!(CellValue::Text("abc".to_owned()), restored);
        assert_eq!(
            Acceptance::Violating,
            TypeKind::Int.accepts(&restored),
            "上流の判定が違反として見ていない"
        );

        let restored = JsScalar::Number(42.0)
            .to_value(TypeKind::Text)
            .expect("文字列の列で数値を捨てた");
        assert_eq!(CellValue::Int(42), restored);
        assert_eq!(Acceptance::Violating, TypeKind::Text.accepts(&restored));
    }

    /// 型が変種を決めない列（`any` / `object` / `array` / `custom`）では、素の写像を使わない。
    /// 読みは自己記述的な形になり、書きは素の値から変種を決めない（`None`）。
    #[test]
    fn open_types_never_use_the_plain_mapping() {
        let value = CellValue::Text("1.50".to_owned());
        for kind in [
            TypeKind::Any,
            TypeKind::Object,
            TypeKind::Array,
            TypeKind::Custom,
        ] {
            assert_eq!(Mapping::SelfDescribing, Mapping::of(kind), "{kind:?}");
            assert_eq!(
                JsView::SelfDescribing(&value),
                to_js(&value, kind),
                "{kind:?} が素の写像を使った"
            );
            assert_eq!(
                None,
                JsScalar::Str(Cow::Borrowed("1.50")).to_value(kind),
                "{kind:?} の列で変種を素の値から決めた"
            );
        }
    }

    /// 入れ子は**構造のまま**、値への参照として渡る（複製しない）。葉の変種は JS では区別
    /// できないため、入れ子はいつでも自己記述的な形になる — 綴りが同じ `Text("1.50")` と
    /// `Decimal("1.50")` が別の値として渡ることが、その証拠である（要件 4.3）。
    /// 素の写像を使う列（`int`）でも、違反として保持された入れ子は同じ道を通る。
    #[test]
    fn nested_values_are_borrowed_with_their_structure_and_variants() {
        let value = CellValue::Nested(NestedValue::Object(vec![
            (
                "amount".to_owned(),
                CellValue::Nested(NestedValue::Array(vec![
                    CellValue::Decimal("1.50".to_owned()),
                    CellValue::Text("1.50".to_owned()),
                ])),
            ),
            ("note".to_owned(), CellValue::Null),
        ]));

        for kind in [TypeKind::Any, TypeKind::Text, TypeKind::Int] {
            let JsView::SelfDescribing(handed) = to_js(&value, kind) else {
                panic!("{kind:?} の列で入れ子が自己記述的な形にならなかった");
            };
            assert!(
                std::ptr::eq(handed, &value),
                "入れ子が複製された（借用で渡っていない）"
            );
            assert_eq!(&value, handed, "{kind:?} の列で構造が変わった");
        }
    }
}

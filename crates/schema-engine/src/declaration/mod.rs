//! スキーマ宣言のデータ構造（design.md「コンポーネントとファイルの対応」の
//! `SchemaDeclaration`。tasks.md 3.1。要件 1.3, 3.1, 3.2, 3.3, 4.1, 4.2, 4.5, 4.6）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本層は [`crate::types`]（型カタログ）と [`crate::error`] の右隣に
//! 置かれ、上流の `document-format` の値と識別子、および `types` の `TypeKind` /
//! `DecimalDigits` / `OffsetPolicy` だけを参照する。`registry` 以降（拡張型の登録・
//! コンパイル・検証）は本層を参照する側であり、**本層はそれらを知らない**。
//!
//! # 型の意味と型の表記を混ぜない（design.md「Architecture Integration」）
//!
//! `types` は「値がその型か」を、`declaration` は「その型をどう書くか」を所有する。
//! したがって組込種別のトークン（`"int"` などの文字列）とその読み書きは**本層**が持つ
//! （design.md の言う「型の**表記**は `declaration` が」）。[`DeclaredKind`] が
//! トークンと [`TypeKind`] の境界である。
//!
//! # 値の制約は型の側、存在と同一性の制約は列とフィールドの側（tasks.md 3.1。要件 4.1, 4.2, 4.5, 4.6）
//!
//! - **値の制約**（範囲・長さ・書式・桁・選択肢・入れ子の形）は [`Constraints`] が持ち、
//!   型（[`TypeDecl`]）の側にだけ置かれる。「どんな値が正しいか」の話である。
//! - **存在と同一性の制約**（`required` / `unique` / `default`）は [`ColumnDecl`] と
//!   [`FieldDecl`] が持つ。「その列・フィールドが何を要求するか」の話である。
//!
//! この 2 つを混ぜると、同じ型を別の列で使い回せなくなる（列ごとに `required` が
//! 違っても型は同じである）。`unique` を持つのは列だけである（要件 4.6 は列についてのみ
//! 一意を要求する。入れ子のフィールドは `required` と `default` を持つ）。
//!
//! # 既定値はセル値と同じ wire 形（design.md「スキーマ宣言の文法」）
//!
//! `default` と範囲の端点（`min` / `max`）は [`CellValue`] そのものとして持つ。
//! 宣言のための第 2 の値表現を作らない（`{"$t":"text","v":"…"}` の脱出口も同じ型を
//! 通る）。範囲の端点が [`CellValue`] であることは、違反の理由
//! （[`Expected::Range`](crate::validate::report::Expected::Range)）が端点を同じ型で
//! 運ぶこととも一致する。
//!
//! # 型は「種別による指定」か「識別子による参照」のいずれか一方（tasks.md 3.1。要件 3.3）
//!
//! design.md の文法は「型は**常にオブジェクト**であり、`kind` を持つか `$ref` を持つかの
//! いずれかである」と定める。この二者択一を [`TypeDecl`] の 2 変種が表す。**両方を持つ型も
//! どちらも持たない型も、この型では構成できない**（構成できてしまう形にすると、誤りを
//! `declaration` 層の型で防げなくなる）。
//!
//! # 未知の種別を保持できる理由（design.md「解決できない型の扱い」。要件 11.7）
//!
//! 将来の版が足した `kind` を古い版が読んだ場合、スキーマ全体を破棄してはならない。
//! **その列だけを使用不能**にして残りを読む（design.md の表）。したがって
//! [`DeclaredKind`] は既知の [`TypeKind`] のほかに**未知のトークンをそのまま保持する**。
//! 未知をここで拒否すると、要件 11.7 の「スキーマを破棄しない」を `compile` 層
//! （タスク 4.4）が実現できなくなる。
//!
//! # 名前付き型定義と参照（tasks.md 3.1。要件 3.3）
//!
//! [`TypeDefinition`] が名前付き型定義 1 件（上流のエンベロープが持つ識別子と、その
//! `definition` ペイロードの中身）を表す。[`TypeDecl::Ref`] は [`TypeDefId`] を運び、
//! **ルートスキーマの列からも、他の型定義の入れ子のフィールドからも**同じ形で参照できる。
//! 定義の集合（エンベロープの `types` 配列）は上流の `document-format` が所有するため
//! （design.md「Out of Boundary」）、本層は 1 件ずつを表し、集合は `compile` 層が扱う。
//!
//! # 純粋なデータであること
//!
//! 宣言は**シリアライズ可能な純粋なデータ**であり（design.md「Architecture Pattern &
//! Boundary Map」）、振る舞いを持たない。したがって各構造体のフィールドは公開し、
//! 派生（`Debug` / `Clone` / `PartialEq` / `Default`）だけを与える。不変条件を持つ型
//! （コンパイル済みパターンなど）は `types` 層が持つ。本層は誤り型
//! （[`SchemaError`](crate::error::SchemaError)）も違反
//! （[`Violation`](crate::validate::report::Violation)）も生成しない。宣言テキストの
//! 解析と正準出力は `declaration::codec`（tasks.md 3.2, 3.3）が所有する。

use crate::types::datetime::OffsetPolicy;
use crate::types::decimal::DecimalDigits;
use crate::types::TypeKind;
use document_format::{CellValue, SheetId, TypeDefId};

pub mod codec;

/// ルートスキーマの宣言（本機能が所有する `root` ペイロードの中身。design.md
/// 「スキーマ宣言の文法」）。
///
/// 列の並びそのものであり、この並びが**シートの列の並び順**になる（要件 1.1。並び順の
/// 決定と供給は `compile` 層のタスク 4.4）。列を 1 つも持たない [`Schema::default()`] は
/// 列 0 本のシートを表し、誤りではない（要件 1.8）。
///
/// 名前付き型定義の集合は上流の `document-format` がエンベロープの `types` 配列として
/// 保持するため（design.md「Out of Boundary」）、本型は持たない。本層は 1 件ずつを
/// [`TypeDefinition`] で表し、集合は `compile` 層が上流から読み出す。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Schema {
    /// 宣言順の列。この順が列の並び順になる（要件 1.1, 1.2）。
    pub columns: Vec<ColumnDecl>,
}

/// 列の宣言（design.md のルートスキーマの `columns` の要素。tasks.md 3.1。要件 1.3）。
///
/// 名前・型・説明を持ち（要件 1.3）、加えて列の側にだけ**存在と同一性の制約**
/// （`required` / `unique` / `default`）を持つ（要件 4.1, 4.2, 4.6）。値の制約
/// （範囲・長さ・書式・桁・選択肢）は型の側（[`TypeDecl`] の [`Constraints`]）にある。
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDecl {
    /// 列名。空でないことと重複しないことは `declaration::codec`（タスク 3.2）が
    /// 宣言ごと拒否する（要件 1.5, 1.6）。上流は列名の中身を解釈しない
    /// （design.md「Existing Architecture Analysis」）。
    pub name: Box<str>,
    /// 列の型。種別による指定か、識別子による参照のいずれかである（要件 3.3）。
    pub ty: TypeDecl,
    /// 値なしを許さないか（要件 4.1, 4.4）。値なしの受理可否は型ではなくここが決める。
    pub required: bool,
    /// 一意制約（要件 4.6, 4.7）。判定は行を跨ぐため `validate` 層の一括経路が行う。
    /// **列だけが持つ**（要件 4.6 は列についてのみ一意を要求する）。
    pub unique: bool,
    /// 既定値（要件 4.2, 4.3, 4.8）。未宣言は `None`。セル値と同じ wire 形で持つ。
    /// 型や制約に適合しない既定値は `declaration::codec` と `compile` 層が拒否する。
    pub default: Option<CellValue>,
    /// 説明（要件 1.3）。未宣言は `None`。
    pub description: Option<Box<str>>,
}

/// 入れ子のオブジェクトのフィールドの宣言（design.md の型定義の `fields` の要素。
/// tasks.md 3.1。要件 3.1, 3.2, 4.1, 4.2）。
///
/// [`ColumnDecl`] と同じく名前・型・説明と存在の制約を持つが、`unique` を持たない。
/// フィールドの一意性は要件に無く、行を跨ぐ同一性の判定は列の単位でしか意味を持たない。
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    /// フィールド名。空でないことは `declaration::codec` が拒否する。
    pub name: Box<str>,
    /// フィールドの型。ここにも種別と制約を宣言できる（要件 3.2）。
    pub ty: TypeDecl,
    /// 値なしを許さないか（要件 4.1）。
    pub required: bool,
    /// 既定値（要件 4.2）。未宣言は `None`。セル値と同じ wire 形で持つ。
    pub default: Option<CellValue>,
    /// 説明。未宣言は `None`。
    pub description: Option<Box<str>>,
}

/// 名前付き型定義 1 件の宣言（tasks.md 3.1。要件 3.3）。
///
/// 識別子は上流の `document-format` のエンベロープ（`types` 配列の要素の `id`）が所有し、
/// 本文はその要素の `definition` ペイロードの中身である（design.md「Out of Boundary」）。
/// 本層はその 2 つを組にして表す。ルートスキーマと他の型定義は [`TypeDecl::Ref`] で
/// この識別子を指す。
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDefinition {
    /// 型定義の識別子（上流のエンベロープが発行・保持する）。
    pub id: TypeDefId,
    /// 型定義の本文。組込種別とその制約、または別の型定義への参照である。
    pub definition: TypeDecl,
}

/// 型の宣言（design.md「スキーマ宣言の文法」。tasks.md 3.1。要件 1.3, 3.1, 3.3）。
///
/// design.md の文法は「型は**常にオブジェクト**であり、`kind` を持つか `$ref` を持つかの
/// いずれかである」と定める。本列挙体の 2 変種がその二者択一そのものであり、
/// **両方を持つ型もどちらも持たない型も構成できない**（誤りを宣言の型で防ぐ）。
///
/// 値の制約（範囲・長さ・書式・桁・選択肢・入れ子の形）は種別による指定の側にだけ置かれ、
/// 必須・一意・既定値は列とフィールドの側にだけ置かれる（tasks.md 3.1）。
#[derive(Debug, Clone, PartialEq)]
pub enum TypeDecl {
    /// 種別による指定（`{ "kind": … }`）。
    Kind {
        /// 種別トークン。既知の組込種別か、将来の版が足した未知のトークンか
        /// （[`DeclaredKind`]）。
        kind: DeclaredKind,
        /// 型のパラメータ（値の制約）。
        constraints: Constraints,
    },
    /// 名前付き型定義への参照（`{ "$ref": "<TypeDefId>" }`。要件 3.3）。
    ///
    /// 上流の `document-format` が参照として構造で追跡できる唯一の形が `$ref` オブジェクト
    /// であるため（design.md「Existing Architecture Analysis」）、宣言テキストでは必ず
    /// この形で書く。本層の型では識別子そのものを運ぶ。
    Ref(TypeDefId),
}

/// 宣言に書かれた種別トークン（tasks.md 3.1。要件 2.1, 11.7）。
///
/// 設計の型カタログ表に挙がる種別は [`TypeKind`]（`types` 層が所有する閉じた集合）に
/// 写る。しかし**未知のトークンでスキーマを破棄してはならない** — 将来の版が足した種別を
/// 古い版が読んだ場合、その列だけを使用不能にして残りを読む（design.md「解決できない型の
/// 扱い」。要件 11.7）。したがって本型は未知のトークンをそのまま保持し、使用不能の判断を
/// `compile` 層（タスク 4.4）へ渡す。
///
/// 種別トークン（`"int"` などの文字列）と [`TypeKind`] の対応は**本層（表記の所有者）**が
/// 持つ（design.md「Architecture Integration」の Domain boundaries）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredKind {
    /// 設計の型カタログ表に挙がる既知の種別。
    Known(TypeKind),
    /// 設計の型カタログ表に無い種別のトークン。その列は使用不能になる（要件 11.7）。
    Unknown(Box<str>),
}

/// 型のパラメータ＝**値の制約**（design.md「組込型カタログと `CellValue` への写像」表の
/// 「パラメータ」欄。tasks.md 3.1。要件 2.3, 2.4, 3.1, 4.5）。
///
/// 「どんな値が正しいか」だけを持ち、「その列が何を要求するか」（`required` / `unique` /
/// `default`）は持たない（design.md「スキーマ宣言の文法」の文法の規則）。どの位置がどの
/// 種別で意味を持つかは設計の表が定める。
///
/// 表に無い位置は未宣言（`None` / 空）のまま保持し、種別との整合の検査は
/// `declaration::codec`（タスク 3.2）と `compile` 層（タスク 4.4）が行う。**未知のキーを
/// ここで表現しない**ため、将来の版がパラメータを足しても本型は壊れない。
///
/// 端点（`min` / `max`）と入れ子を [`CellValue`] / [`TypeDecl`] で持つのは、宣言のための
/// 第 2 の表現を作らないためである（design.md「既定値はセル値と同一の wire 形」）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Constraints {
    /// 値の範囲の下限（含む。design.md の `min`。要件 4.5）。
    ///
    /// `int` / `float` / `decimal` では数値のセル値、`date` / `datetime` では正準表記の
    /// 文字列のセル値として持つ。未宣言は `None`（開いた下限）。
    pub min: Option<CellValue>,
    /// 値の範囲の上限（含む。design.md の `max`。要件 4.5）。未宣言は `None`。
    pub max: Option<CellValue>,
    /// 文字列の最小長（design.md の `minLength`。要件 4.5）。**文字数**で数える
    /// （tasks.md 2.4 の裁定）。未宣言は `None`。
    pub min_length: Option<usize>,
    /// 文字列の最大長（design.md の `maxLength`。要件 4.5）。未宣言は `None`。
    pub max_length: Option<usize>,
    /// 文字列の書式（design.md の `pattern`。要件 4.5）。**生文字列のまま**保持し、
    /// コンパイルは `compile` 層が一度だけ行う（タスク 4.4。上限の検査は
    /// [`TextPattern::compile`](crate::types::text::TextPattern::compile)）。
    pub pattern: Option<Box<str>>,
    /// 10 進数の有効桁数と小数点以下の桁数（design.md の `precision` / `scale`。
    /// 要件 2.3）。[`DecimalDigits`] は `precision >= 1` かつ `scale <= precision` を
    /// 不変条件とするため、これに反する宣言はこの型では構成できず、
    /// `declaration::codec` が `SchemaError::MalformedDeclaration` として拒否する。
    pub digits: Option<DecimalDigits>,
    /// 列挙の選択肢（design.md の `choices`。要件 4.5）。宣言順を保つ。
    pub choices: Vec<Box<str>>,
    /// 参照先のシート（design.md の `ref` の `sheet`。要件 9.1）。`ref` 種別では宣言が
    /// 必須であり、欠落は `declaration::codec` が拒否する。
    pub sheet: Option<SheetId>,
    /// 日時のオフセットの扱い（design.md の `datetime` の `offset`。要件 2.4）。
    /// `datetime` 種別では宣言が必須であり、欠落は `declaration::codec` が拒否する。
    pub offset: Option<OffsetPolicy>,
    /// オブジェクトのフィールド（design.md の `object` の `fields`。要件 3.1, 3.2）。
    /// 宣言順を保つ。空のオブジェクトはフィールド 0 個である。
    pub fields: Vec<FieldDecl>,
    /// 配列の要素の型（design.md の `array` の `items`。要件 3.1）。`array` 種別では宣言が
    /// 必須であり、欠落は `declaration::codec` が拒否する。要素型も入れ子の型であるため、
    /// 内側のフィールドにも型と制約を宣言できる（要件 3.2）。
    pub items: Option<Box<TypeDecl>>,
    /// 配列の要素数の下限（design.md の `minItems`）。未宣言は `None`。
    pub min_items: Option<usize>,
    /// 配列の要素数の上限（design.md の `maxItems`）。未宣言は `None`。
    pub max_items: Option<usize>,
    /// 拡張型の識別子（design.md の `custom` の `type`。要件 11.1, 11.2）。
    ///
    /// 拡張型の登録（`registry` 層のタスク 4.1）は本層より右にあるため、本層は
    /// **識別子を文字列として**保持する。登録済みかどうかの判断と、未登録の場合に
    /// その列を使用不能にすることは `compile` 層（タスク 4.4。要件 11.7）が行う。
    pub custom_type: Option<Box<str>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::datetime::OffsetPolicy;
    use crate::types::decimal::DecimalDigits;
    use crate::types::TypeKind;
    use document_format::IdFactory;

    /// 値の制約は型の側にだけある（tasks.md 3.1）。列・フィールドから取り出す経路が
    /// 無いことを、この関数の存在（型からのみ引けること）が示す。
    fn value_constraints(ty: &TypeDecl) -> &Constraints {
        match ty {
            TypeDecl::Kind { constraints, .. } => constraints,
            TypeDecl::Ref(id) => panic!("参照型は値の制約を持たない: {id}"),
        }
    }

    /// 型が「種別による指定」か「識別子による参照」の**いずれか一方**であることを、
    /// ワイルドカードなしの網羅マッチで示す（変種が増えればここでコンパイルが壊れる）。
    fn declared_shape(ty: &TypeDecl) -> &'static str {
        match ty {
            TypeDecl::Kind { .. } => "kind",
            TypeDecl::Ref(_) => "ref",
        }
    }

    /// 設計の型カタログ表（design.md「組込型カタログと `CellValue` への写像」）の
    /// 各行のパラメータを値の制約として書き下したもの。`custom` は拡張型の識別子を持つ。
    fn table_constraints(kind: TypeKind) -> Constraints {
        match kind {
            TypeKind::Int => Constraints {
                min: Some(CellValue::Int(0)),
                max: Some(CellValue::Int(100)),
                ..Constraints::default()
            },
            TypeKind::Float => Constraints {
                min: Some(CellValue::Float(0.0)),
                max: Some(CellValue::Float(1.0)),
                ..Constraints::default()
            },
            TypeKind::Decimal => Constraints {
                digits: Some(DecimalDigits::new(12, 2).expect("12 >= 1 かつ 2 <= 12")),
                min: Some(CellValue::Decimal("-100.00".to_owned())),
                max: Some(CellValue::Decimal("100.00".to_owned())),
                ..Constraints::default()
            },
            TypeKind::Text => Constraints {
                min_length: Some(1),
                max_length: Some(32),
                pattern: Some("^[A-Z0-9-]+$".into()),
                ..Constraints::default()
            },
            TypeKind::Bool | TypeKind::Attachment | TypeKind::Any => Constraints::default(),
            TypeKind::Date => Constraints {
                min: Some(CellValue::Text("2026-01-01".to_owned())),
                max: Some(CellValue::Text("2026-12-31".to_owned())),
                ..Constraints::default()
            },
            TypeKind::DateTime => Constraints {
                offset: Some(OffsetPolicy::Required),
                min: Some(CellValue::Text("2026-01-01T00:00:00Z".to_owned())),
                max: Some(CellValue::Text("2026-12-31T23:59:59Z".to_owned())),
                ..Constraints::default()
            },
            TypeKind::Enum => Constraints {
                choices: vec!["赤".into(), "青".into()],
                ..Constraints::default()
            },
            TypeKind::Ref => Constraints {
                sheet: Some(IdFactory::default().new_sheet_id()),
                ..Constraints::default()
            },
            TypeKind::Object => Constraints {
                fields: vec![FieldDecl {
                    name: "色".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Enum),
                        constraints: Constraints {
                            choices: vec!["赤".into(), "青".into()],
                            ..Constraints::default()
                        },
                    },
                    required: true,
                    default: None,
                    description: None,
                }],
                ..Constraints::default()
            },
            TypeKind::Array => Constraints {
                items: Some(Box::new(TypeDecl::Kind {
                    kind: DeclaredKind::Known(TypeKind::Int),
                    constraints: Constraints::default(),
                })),
                min_items: Some(1),
                max_items: Some(10),
                ..Constraints::default()
            },
            TypeKind::Custom => Constraints {
                custom_type: Some("postal-code".into()),
                ..Constraints::default()
            },
        }
    }

    /// 設計の型カタログ表に挙がる全種別が宣言でき、その値の制約が型の側に置かれる
    /// （tasks.md 3.1。要件 2.1, 3.1, 3.2, 4.5）。
    #[test]
    fn every_catalog_kind_is_declarable_with_value_constraints_on_the_type() {
        for kind in TypeKind::ALL {
            let expected = table_constraints(kind);
            let column = ColumnDecl {
                name: "列".into(),
                ty: TypeDecl::Kind {
                    kind: DeclaredKind::Known(kind),
                    constraints: expected.clone(),
                },
                required: true,
                unique: false,
                default: None,
                description: None,
            };
            assert_eq!(expected, *value_constraints(&column.ty));
            assert_eq!("kind", declared_shape(&column.ty));
            match &column.ty {
                TypeDecl::Kind {
                    kind: DeclaredKind::Known(declared),
                    ..
                } => assert_eq!(kind, *declared, "宣言した種別が変わっている"),
                other => panic!("種別による指定として宣言できない: {other:?}"),
            }
        }
    }

    /// 存在と同一性の制約（必須・一意・既定値）は列とフィールドの側に置かれる
    /// （tasks.md 3.1。要件 4.1, 4.2, 4.6）。型の側には現れない。
    #[test]
    fn presence_and_identity_constraints_live_on_the_column_and_field() {
        let column = ColumnDecl {
            name: "数量".into(),
            ty: TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Int),
                constraints: Constraints {
                    min: Some(CellValue::Int(0)),
                    ..Constraints::default()
                },
            },
            required: true,
            unique: true,
            default: Some(CellValue::Int(0)),
            description: Some("入荷数量".into()),
        };
        assert!(column.required);
        assert!(column.unique);
        assert_eq!(Some(CellValue::Int(0)), column.default);
        assert_eq!(Some("入荷数量"), column.description.as_deref());

        let field = FieldDecl {
            name: "単価".into(),
            ty: TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Decimal),
                constraints: Constraints::default(),
            },
            required: false,
            default: Some(CellValue::Null),
            description: None,
        };
        assert!(!field.required);
        assert_eq!(Some(CellValue::Null), field.default);
    }

    /// 列の宣言が名前・型・説明を保持する（tasks.md 3.1。要件 1.3）。
    #[test]
    fn a_column_declaration_holds_name_type_and_description() {
        let column = ColumnDecl {
            name: "品番".into(),
            ty: TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Text),
                constraints: Constraints {
                    max_length: Some(32),
                    ..Constraints::default()
                },
            },
            required: false,
            unique: false,
            default: None,
            description: Some("仕入先の品番".into()),
        };
        assert_eq!("品番", &*column.name);
        assert_eq!(Some("仕入先の品番"), column.description.as_deref());
        assert_eq!(Some(32), value_constraints(&column.ty).max_length);
    }

    /// 入れ子としてオブジェクトと配列を宣言でき、内側の各フィールドにも型と制約を
    /// 宣言できる（tasks.md 3.1。要件 3.1, 3.2）。
    #[test]
    fn nested_objects_and_arrays_declare_inner_types_and_constraints() {
        let item = FieldDecl {
            name: "明細".into(),
            ty: TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Object),
                constraints: Constraints {
                    fields: vec![FieldDecl {
                        name: "数量".into(),
                        ty: TypeDecl::Kind {
                            kind: DeclaredKind::Known(TypeKind::Int),
                            constraints: Constraints {
                                min: Some(CellValue::Int(1)),
                                ..Constraints::default()
                            },
                        },
                        required: true,
                        default: Some(CellValue::Int(1)),
                        description: Some("明細ごとの数量".into()),
                    }],
                    ..Constraints::default()
                },
            },
            required: true,
            default: None,
            description: Some("明細の並び".into()),
        };
        assert!(item.required);
        assert_eq!(Some("明細の並び"), item.description.as_deref());
        let array = TypeDecl::Kind {
            kind: DeclaredKind::Known(TypeKind::Array),
            constraints: Constraints {
                items: Some(Box::new(item.ty)),
                min_items: Some(1),
                max_items: None,
                ..Constraints::default()
            },
        };

        let constraints = value_constraints(&array);
        let items = constraints.items.as_ref().expect("配列の要素型");
        let object_constraints = value_constraints(items);
        assert!(object_constraints.fields[0].required);
        assert_eq!(
            Some(CellValue::Int(1)),
            value_constraints(&object_constraints.fields[0].ty).min
        );
        assert_eq!(Some(1), constraints.min_items);
        assert_eq!(None, constraints.max_items);
    }

    /// 名前付き型定義を宣言し、ルートスキーマと**他の型定義**の双方から識別子で参照できる
    /// （tasks.md 3.1。要件 3.3）。
    #[test]
    fn named_type_definitions_are_referenced_by_identifier_from_root_and_definitions() {
        // 名前付き型定義の本文（オブジェクト）。内側のフィールドにも型と制約がある。
        let postal = TypeDefinition {
            id: IdFactory::default().new_type_def_id(),
            definition: TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Object),
                constraints: Constraints {
                    fields: vec![FieldDecl {
                        name: "郵便番号".into(),
                        ty: TypeDecl::Kind {
                            kind: DeclaredKind::Known(TypeKind::Custom),
                            constraints: Constraints {
                                custom_type: Some("postal-code".into()),
                                ..Constraints::default()
                            },
                        },
                        required: true,
                        default: None,
                        description: None,
                    }],
                    ..Constraints::default()
                },
            },
        };
        // 別の型定義が識別子で上の型定義を参照する（要件 3.3 の「他の型定義から」）。
        let address = TypeDefinition {
            id: IdFactory::default().new_type_def_id(),
            definition: TypeDecl::Ref(postal.id),
        };
        // ルートスキーマの列も識別子で参照する（要件 3.3 の「ルートスキーマから」）。
        let root = Schema {
            columns: vec![ColumnDecl {
                name: "住所".into(),
                ty: TypeDecl::Ref(address.id),
                required: false,
                unique: false,
                default: None,
                description: None,
            }],
        };

        match &root.columns[0].ty {
            TypeDecl::Ref(id) => assert_eq!(address.id, *id),
            other => panic!("ルートスキーマから参照できない: {other:?}"),
        }
        match &address.definition {
            TypeDecl::Ref(id) => assert_eq!(postal.id, *id),
            other => panic!("型定義から他の型定義を参照できない: {other:?}"),
        }
        match &value_constraints(&postal.definition).fields[0].ty {
            TypeDecl::Kind { kind, constraints } => {
                assert_eq!(DeclaredKind::Known(TypeKind::Custom), *kind);
                assert_eq!(Some("postal-code"), constraints.custom_type.as_deref());
            }
            other => panic!("入れ子の内側の型が壊れている: {other:?}"),
        }
    }

    /// 将来の版が足した未知の種別を、トークンのまま保持できる（design.md
    /// 「解決できない型の扱い」。要件 11.7 を `compile` 層が実現するための表現）。
    #[test]
    fn an_unknown_kind_token_is_preserved_for_the_compile_layer() {
        let column = ColumnDecl {
            name: "将来の型".into(),
            ty: TypeDecl::Kind {
                kind: DeclaredKind::Unknown("geo-point".into()),
                constraints: Constraints::default(),
            },
            required: false,
            unique: false,
            default: None,
            description: None,
        };
        match &column.ty {
            TypeDecl::Kind {
                kind: DeclaredKind::Unknown(token),
                ..
            } => assert_eq!("geo-point", &**token),
            other => panic!("未知の種別を保持できない: {other:?}"),
        }
    }
}

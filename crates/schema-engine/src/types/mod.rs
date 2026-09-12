//! 組込型の集合とセル値への写像（design.md「コンポーネントとファイルの対応」の
//! `TypeCatalog`。tasks.md 2.1。要件 2.1, 2.2, 2.5, 2.7, 9.1）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本層は `error` と並ぶ最左であり、上流の `document-format` の
//! **値だけ**を参照する。`types` は「値がその型か」を、`declaration` は「その型をどう書くか」を
//! 所有し、両者を混ぜない（design.md「Architecture Integration」の Domain boundaries）。
//! したがって本モジュールは `declaration` 層の型（`TypeDecl` / `ColumnDecl`）を参照しない。
//!
//! # 正典は design.md の型カタログ表
//!
//! 組込型の集合と、各型が受け入れる `CellValue` の変種は、design.md
//! 「組込型カタログと `CellValue` への写像」の表が正典である。本モジュールはその表を
//! コードへ写した**唯一の場所**であり、`ColumnValidator`（タスク 4.2）と `Coercer`
//! （タスク 6.1）がこの写像を参照する。表に種別を足すときは本モジュールだけを直す。
//!
//! # 値の制約はここに置かない
//!
//! 範囲・長さ・書式・桁・選択肢といった**値の制約**は型のパラメータであり、`declaration`
//! 層の `TypeDecl`（タスク 3.1）が所有する。本モジュールが定めるのは「どの変種を受け入れる
//! か」だけである。`10 進数`（[`decimal`]）・`日時`（[`datetime`]）・`文字列`（[`text`]）の
//! 3 つのサブモジュールは、その型に固有の検査（桁・暦・書式）をそれぞれが所有する。本ファイル
//! （tasks.md 2.1 の境界 `TypeCatalog`）はそれらを宣言するだけであり、tasks.md 2.2 以降が
//! 中身を埋める。**3 つをこの時点で宣言する**のは、並行して進む後続が本ファイルを書き換えずに
//! 済むようにするためである。
//!
//! # 値なしは型の側では判定しない
//!
//! `CellValue::Null` はどの型でも「値なし」を表す。受理されるかは**列の必須指定**が決め、
//! 型の側では決めない（design.md 同表）。したがって [`TypeKind::accepts`] は `Null` をどの
//! 種別でも適合として返す。必須の検査は `validate` 層（タスク 5.1）が行う。
//!
//! # 適合または違反のいずれか一方
//!
//! [`Acceptance`] は**適合**と**違反**の 2 変種だけを持つ（要件 2.7）。「判定できない」という
//! 第 3 の状態を作らない。拡張型（`custom`）の実際の可否は拡張インターフェースの実装
//! （タスク 4.1）が決めるが、その実装も同じ 2 値の判定へ落ちる（design.md
//! 「Registry Layer / TypeRegistry」）。[`AcceptedVariants::Delegated`] はその委譲を表し、
//! 本カタログは変種の段階では制限を課さない。
//!
//! # 誤り型を持たない
//!
//! 本層は宣言の誤り（[`SchemaError`](crate::error::SchemaError)）も値の違反
//! （[`Violation`](crate::validate::report::Violation)）も生成しない。種別の集合と写像を
//! 定めるだけで、壊れた宣言の拒否は `declaration` 層が、違反の表現は `validate` 層が
//! それぞれ所有する（design.md「Error Handling」）。

use document_format::{CellValue, NestedValue};

pub mod datetime;
pub mod decimal;
pub mod text;

/// セル値の変種（design.md「組込型カタログと `CellValue` への写像」表の
/// 「受け入れる `CellValue`」欄の語彙。要件 2.2）。
///
/// `CellValue` の 8 変種を、入れ子（[`CellValue::Nested`]）の object と array を分けた
/// 9 分類へ写す。この分類は**変種の形**だけを見ており、値の中身（桁・暦・書式）は見ない。
/// 中身の検査は [`decimal`] / [`datetime`] / [`text`] と、それを使う `ColumnValidator`
/// （タスク 4.2）が所有する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueVariant {
    /// 値なし（[`CellValue::Null`]）。どの型も表せるが、受理可否は列の必須指定が決める。
    Null,
    /// 真偽（[`CellValue::Bool`]）。
    Bool,
    /// 64 ビット整数（[`CellValue::Int`]）。
    Int,
    /// 倍精度小数（[`CellValue::Float`]）。
    Float,
    /// 10 進数（[`CellValue::Decimal`]。文字列のまま逐語で往復する）。
    Decimal,
    /// 文字列（[`CellValue::Text`]）。
    Text,
    /// 名前つきフィールドの集合（[`CellValue::Nested`] の object）。
    Object,
    /// 同一型の並び（[`CellValue::Nested`] の array）。
    Array,
    /// 添付参照（[`CellValue::Attachment`]）。
    Attachment,
}

impl ValueVariant {
    /// セル値の変種を分類する（変種の形だけを見る。中身は見ない）。
    pub fn of(value: &CellValue) -> Self {
        match value {
            CellValue::Null => ValueVariant::Null,
            CellValue::Bool(_) => ValueVariant::Bool,
            CellValue::Int(_) => ValueVariant::Int,
            CellValue::Float(_) => ValueVariant::Float,
            CellValue::Decimal(_) => ValueVariant::Decimal,
            CellValue::Text(_) => ValueVariant::Text,
            CellValue::Nested(NestedValue::Object(_)) => ValueVariant::Object,
            CellValue::Nested(NestedValue::Array(_)) => ValueVariant::Array,
            CellValue::Attachment(_) => ValueVariant::Attachment,
        }
    }
}

/// 型が変種として受け入れる範囲（design.md の表の「受け入れる `CellValue`」欄の正典。
/// 要件 2.1, 2.2, 2.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptedVariants {
    /// 表に挙がる変種だけを受け入れる。
    Only(&'static [ValueVariant]),
    /// 保持可能なすべての変種を受け入れる（`any`。要件 2.5）。
    All,
    /// 拡張型の実装が決める（`custom`。design.md 同表「拡張型の実装が決める」）。
    /// 本カタログは変種の段階では制限を課さず、実際の受理可否を実装へ委ねる。
    Delegated,
}

/// 組込型カタログの種別（design.md「組込型カタログと `CellValue` への写像」表の `kind` 欄。
/// 要件 2.1, 9.1）。
///
/// 宣言の文法が受理する `kind` の集合そのものである。各変種が**どの変種を受け入れるか**は
/// [`TypeKind::accepted_variants`] が表のとおりに定め、値の制約（範囲・桁・選択肢など）は
/// ここには持たない（`declaration` 層の `TypeDecl`。タスク 3.1）。
///
/// `Ref` は**特定のシートの行**を指す（要件 9.1）。参照先シートの識別子は宣言のパラメータで
/// あり（design.md 同表の `sheet` 欄）、`declaration` 層が所有する。本層が定めるのは
/// 「行識別子を `Text` として受け入れる」という変種の写像だけである。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeKind {
    /// 64 ビット整数（`Int` を受け入れる。`min` / `max`）。
    Int,
    /// 倍精度小数（`Float` を受け入れる。`min` / `max`）。
    Float,
    /// 10 進数（`Decimal` を受け入れる。`precision` / `scale` / `min` / `max`）。
    Decimal,
    /// 文字列（`Text` を受け入れる。`minLength` / `maxLength` / `pattern`）。
    Text,
    /// 真偽（`Bool` を受け入れる）。
    Bool,
    /// 日付（`Text` を `YYYY-MM-DD` として受け入れる。`min` / `max`）。
    Date,
    /// 日時（`Text` を civil またはオフセット付きで受け入れる。`offset` / `min` / `max`）。
    DateTime,
    /// 列挙（`Text` を `choices` のいずれかとして受け入れる）。
    Enum,
    /// シート間参照（`Text` を行識別子として受け入れる。`sheet`。要件 9.1）。
    Ref,
    /// 添付参照（`Attachment` を受け入れる）。
    Attachment,
    /// 名前つきフィールドの集合（`Nested` の object を受け入れる。`fields`）。
    Object,
    /// 同一型の並び（`Nested` の array を受け入れる。`items` / `minItems` / `maxItems`）。
    Array,
    /// 任意の値（保持可能なすべての変種を受け入れる。型強制もしない。要件 2.5, 2.6）。
    Any,
    /// 拡張型（変種の範囲は実装が決める。`type` とその任意のパラメータ）。
    Custom,
}

impl TypeKind {
    /// 設計表に挙がるすべての種別（宣言の文法が受理する `kind` の集合。要件 2.1）。
    ///
    /// 並びは design.md の表の順である。すべての種別が**ちょうど 1 回**現れることを
    /// テストが検査する（`ALL` からの取りこぼしを型の上で検出するため）。
    pub const ALL: [TypeKind; 14] = [
        TypeKind::Int,
        TypeKind::Float,
        TypeKind::Decimal,
        TypeKind::Text,
        TypeKind::Bool,
        TypeKind::Date,
        TypeKind::DateTime,
        TypeKind::Enum,
        TypeKind::Ref,
        TypeKind::Attachment,
        TypeKind::Object,
        TypeKind::Array,
        TypeKind::Any,
        TypeKind::Custom,
    ];

    /// この型が変種として受け入れる範囲（design.md の表の正典。要件 2.1, 2.2, 2.5, 9.1）。
    ///
    /// 文字列の形をとる 5 つの種別（`text` / `date` / `datetime` / `enum` / `ref`）は
    /// いずれも `Text` だけを受け入れる。どの `Text` が適合するか（暦・書式・選択肢）は
    /// それぞれのパラメータと `ColumnValidator`（タスク 4.2）の関心事であり、変種の写像は
    /// 同じである。
    pub fn accepted_variants(self) -> AcceptedVariants {
        match self {
            TypeKind::Int => AcceptedVariants::Only(&[ValueVariant::Int]),
            TypeKind::Float => AcceptedVariants::Only(&[ValueVariant::Float]),
            TypeKind::Decimal => AcceptedVariants::Only(&[ValueVariant::Decimal]),
            TypeKind::Text => AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::Bool => AcceptedVariants::Only(&[ValueVariant::Bool]),
            TypeKind::Date => AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::DateTime => AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::Enum => AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::Ref => AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::Attachment => AcceptedVariants::Only(&[ValueVariant::Attachment]),
            TypeKind::Object => AcceptedVariants::Only(&[ValueVariant::Object]),
            TypeKind::Array => AcceptedVariants::Only(&[ValueVariant::Array]),
            TypeKind::Any => AcceptedVariants::All,
            TypeKind::Custom => AcceptedVariants::Delegated,
        }
    }

    /// 値がこの型に適合するか（要件 2.7）。**適合または違反のいずれか一方**を返す。
    ///
    /// `Null`（値なし）はどの種別でも適合を返す。受理されるかは列の必須指定が決めるため、
    /// 型の側では判定しない（design.md 同表）。拡張型（[`TypeKind::Custom`]）は変種の段階で
    /// 制限を課さず、実際の受理可否を実装（タスク 4.1）へ委ねる。
    pub fn accepts(self, value: &CellValue) -> Acceptance {
        if matches!(value, CellValue::Null) {
            // 値なしはどの型でも「値なし」を表す。受理されるかは列の必須指定が決めるため、
            // 型の側では判定しない（design.md の表）。
            return Acceptance::Conforming;
        }
        match self.accepted_variants() {
            AcceptedVariants::Only(allowed) => {
                if allowed.contains(&ValueVariant::of(value)) {
                    Acceptance::Conforming
                } else {
                    Acceptance::Violating
                }
            }
            // `any` は保持可能なすべての値を受け入れる（要件 2.5）。
            AcceptedVariants::All => Acceptance::Conforming,
            // 拡張型は変種の範囲を実装が決める。カタログは変種の段階で制限を課さず、
            // 実際の受理可否は拡張インターフェースの実装（タスク 4.1）の判定に委ねる。
            AcceptedVariants::Delegated => Acceptance::Conforming,
        }
    }
}

/// 型カタログの判定結果（要件 2.7）。
///
/// **適合**と**違反**の 2 変種だけを持つ。「判定できない」という第 3 の状態を作らない
/// （モジュール docs「適合または違反のいずれか一方」）。値の違反の理由
/// （[`ViolationReason`](crate::validate::report::ViolationReason)）は本層では組み立てず、
/// `validate` 層（タスク 5.1）が期待した内容と実際の値から作る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceptance {
    /// 値が型に適合する。
    Conforming,
    /// 値が型に適合しない。
    Violating,
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_format::AttachmentId;

    /// 設計表の `kind` 名。ワイルドカードなしの網羅マッチであるため、種別が増えればここで
    /// コンパイルが壊れる（`error` / `report` の `discriminate` と同じ規約）。
    fn kind_name(kind: TypeKind) -> &'static str {
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

    /// 判定結果を名前へ分解する。ワイルドカードなしで 2 変種を網羅するため、変種が増えれば
    /// ここでコンパイルが壊れ、「適合または違反のいずれか一方」（要件 2.7）が機械的に保証
    /// される。
    fn acceptance_name(acceptance: Acceptance) -> &'static str {
        match acceptance {
            Acceptance::Conforming => "Conforming",
            Acceptance::Violating => "Violating",
        }
    }

    /// 名前つきフィールドの集合（`object` の適合標本）。
    fn object_sample() -> CellValue {
        CellValue::Nested(NestedValue::Object(vec![(
            "色".to_owned(),
            CellValue::Text("赤".to_owned()),
        )]))
    }

    /// `CellValue` の 8 変種をちょうど 1 つずつ（入れ子は object と array の双方）。
    /// 「保持可能なすべての値」（要件 2.5）の標本である。
    fn every_cell_variant() -> Vec<CellValue> {
        vec![
            CellValue::Null,
            CellValue::Bool(true),
            CellValue::Int(-7),
            CellValue::Float(1.5),
            CellValue::Decimal("1.50".to_owned()),
            CellValue::Text("あ".to_owned()),
            object_sample(),
            CellValue::Nested(NestedValue::Array(vec![CellValue::Int(1)])),
            CellValue::Attachment(AttachmentId::from_bytes(b"jxcel attachment payload!")),
        ]
    }

    /// 設計表そのもの（実装とは独立に表を書き下したもの）。ワイルドカードなしの網羅マッチで
    /// あるため、種別が増えればここでコンパイルが壊れる。
    fn expected_variants(kind: TypeKind) -> AcceptedVariants {
        match kind {
            TypeKind::Int => AcceptedVariants::Only(&[ValueVariant::Int]),
            TypeKind::Float => AcceptedVariants::Only(&[ValueVariant::Float]),
            TypeKind::Decimal => AcceptedVariants::Only(&[ValueVariant::Decimal]),
            TypeKind::Text => AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::Bool => AcceptedVariants::Only(&[ValueVariant::Bool]),
            TypeKind::Date => AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::DateTime => AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::Enum => AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::Ref => AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::Attachment => AcceptedVariants::Only(&[ValueVariant::Attachment]),
            TypeKind::Object => AcceptedVariants::Only(&[ValueVariant::Object]),
            TypeKind::Array => AcceptedVariants::Only(&[ValueVariant::Array]),
            TypeKind::Any => AcceptedVariants::All,
            TypeKind::Custom => AcceptedVariants::Delegated,
        }
    }

    /// 設計表の各行の（適合する標本, 違反する標本）。`any` はすべて適合するため違反標本を
    /// 持たず、`custom` は変種の範囲を実装が決めるため標本を持たない。
    fn table_samples(kind: TypeKind) -> Option<(CellValue, CellValue)> {
        match kind {
            TypeKind::Int => Some((CellValue::Int(1), CellValue::Text("1".to_owned()))),
            TypeKind::Float => Some((CellValue::Float(1.5), CellValue::Int(1))),
            TypeKind::Decimal => {
                Some((CellValue::Decimal("1.50".to_owned()), CellValue::Float(1.5)))
            }
            TypeKind::Text => Some((CellValue::Text("a".to_owned()), CellValue::Bool(true))),
            TypeKind::Bool => Some((CellValue::Bool(false), CellValue::Int(0))),
            TypeKind::Date => Some((
                CellValue::Text("2026-09-13".to_owned()),
                CellValue::Int(2026),
            )),
            TypeKind::DateTime => Some((
                CellValue::Text("2026-09-13T10:00:00Z".to_owned()),
                CellValue::Bool(false),
            )),
            TypeKind::Enum => Some((CellValue::Text("赤".to_owned()), CellValue::Int(1))),
            TypeKind::Ref => Some((
                CellValue::Text("01K4ANRRG004HMASW9NF6YY101".to_owned()),
                CellValue::Bool(false),
            )),
            TypeKind::Attachment => Some((
                CellValue::Attachment(AttachmentId::from_bytes(b"x")),
                CellValue::Text("x".to_owned()),
            )),
            TypeKind::Object => Some((
                object_sample(),
                CellValue::Nested(NestedValue::Array(Vec::new())),
            )),
            TypeKind::Array => Some((
                CellValue::Nested(NestedValue::Array(vec![CellValue::Int(1)])),
                object_sample(),
            )),
            TypeKind::Any | TypeKind::Custom => None,
        }
    }

    /// 設計表に挙がる全種別が `ALL` にちょうど 1 回ずつ現れる（要件 2.1）。
    #[test]
    fn the_catalog_lists_every_kind_exactly_once() {
        assert_eq!(
            14,
            TypeKind::ALL.len(),
            "設計表の種別数と ALL の長さが一致しない"
        );
        let mut names: Vec<&'static str> = TypeKind::ALL.iter().copied().map(kind_name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(14, names.len(), "ALL に同じ種別が二度現れる");
    }

    /// 全種別の写像が設計表のとおりである（要件 2.1, 2.2, 2.5, 9.1）。
    #[test]
    fn every_kind_maps_to_the_design_table() {
        for kind in TypeKind::ALL {
            assert_eq!(
                expected_variants(kind),
                kind.accepted_variants(),
                "{} の変種の写像が設計表と一致しない",
                kind_name(kind),
            );
        }
    }

    /// 設計表の各行について、適合する標本は適合、違反する標本は違反になる（要件 2.2, 2.7）。
    #[test]
    fn every_kind_accepts_and_rejects_the_table_samples() {
        for kind in TypeKind::ALL {
            if let Some((conforming, violating)) = table_samples(kind) {
                assert_eq!(
                    Acceptance::Conforming,
                    kind.accepts(&conforming),
                    "{} が表の適合標本を拒否した",
                    kind_name(kind),
                );
                assert_eq!(
                    Acceptance::Violating,
                    kind.accepts(&violating),
                    "{} が表の違反標本を受け入れた",
                    kind_name(kind),
                );
            }
        }
    }

    /// ANY は保持可能なすべての値（8 変種）を適合として扱う（要件 2.5）。
    #[test]
    fn any_accepts_every_representable_value() {
        assert_eq!(AcceptedVariants::All, TypeKind::Any.accepted_variants());
        for value in every_cell_variant() {
            assert_eq!(
                Acceptance::Conforming,
                TypeKind::Any.accepts(&value),
                "any が {:?} を拒否した",
                ValueVariant::of(&value),
            );
        }
    }

    /// 全種別 × 全変種の判定が、適合と違反の 2 値に収まり、双方が現れる（要件 2.7）。
    #[test]
    fn every_judgement_is_one_of_two_reachable_results() {
        let mut seen: Vec<&'static str> = Vec::new();
        for kind in TypeKind::ALL {
            for value in every_cell_variant() {
                let name = acceptance_name(kind.accepts(&value));
                if !seen.contains(&name) {
                    seen.push(name);
                }
            }
        }
        seen.sort_unstable();
        assert_eq!(
            vec!["Conforming", "Violating"],
            seen,
            "判定が適合と違反の 2 値に収まらない",
        );
    }

    /// 値なしはどの型でも同じく適合であり、型の側では受理可否を決めない（要件 2.2）。
    /// 受理されるかは列の必須指定が決める（design.md の表）。
    #[test]
    fn absence_is_uniformly_conforming_so_required_decides() {
        for kind in TypeKind::ALL {
            assert_eq!(
                Acceptance::Conforming,
                kind.accepts(&CellValue::Null),
                "{} が値なしを型の側で拒否した",
                kind_name(kind),
            );
        }
    }

    /// 入れ子の object と array は別の変種であり、取り違えを拒否する（要件 2.2）。
    #[test]
    fn object_and_array_accept_only_their_own_nested_shape() {
        let object = object_sample();
        let array = CellValue::Nested(NestedValue::Array(vec![CellValue::Int(1)]));
        assert_eq!(Acceptance::Conforming, TypeKind::Object.accepts(&object));
        assert_eq!(Acceptance::Violating, TypeKind::Object.accepts(&array));
        assert_eq!(Acceptance::Conforming, TypeKind::Array.accepts(&array));
        assert_eq!(Acceptance::Violating, TypeKind::Array.accepts(&object));
    }

    /// 参照型は行識別子を `Text` として受け入れる（要件 9.1）。参照先シートの識別子は宣言の
    /// パラメータであり（タスク 3.1）、変種の写像には現れない。
    #[test]
    fn ref_accepts_a_row_identifier_as_text() {
        assert_eq!(
            AcceptedVariants::Only(&[ValueVariant::Text]),
            TypeKind::Ref.accepted_variants(),
        );
        assert_eq!(
            Acceptance::Conforming,
            TypeKind::Ref.accepts(&CellValue::Text("01K4ANRRG004HMASW9NF6YY101".to_owned())),
        );
        assert_eq!(
            Acceptance::Violating,
            TypeKind::Ref.accepts(&CellValue::Int(1))
        );
    }

    /// 拡張型は変種の範囲を実装へ委ね、カタログは変種の段階で制限を課さない（実際の 2 値の
    /// 判定は実装が返す。design.md「Registry Layer / TypeRegistry」）。
    #[test]
    fn custom_defers_its_variant_range_to_the_extension_implementation() {
        assert_eq!(
            AcceptedVariants::Delegated,
            TypeKind::Custom.accepted_variants(),
        );
        for value in every_cell_variant() {
            assert_eq!(
                Acceptance::Conforming,
                TypeKind::Custom.accepts(&value),
                "カタログが拡張型の変種に制限を課した",
            );
        }
    }
}

//! 拡張型の登録・解決・呼び出し規約（design.md「コンポーネントとファイルの対応」の
//! `TypeRegistry`。tasks.md 4.1。要件 11.1, 11.2, 11.4）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本層は `declaration` の右隣であり、`error` / `types` /
//! `declaration` と上流 `document-format` の値だけを参照する。本層を参照するのは
//! `compile` 層（タスク 4.4。識別子の解決）と `validate` 層（タスク 5.1。解決済みの実装の
//! 呼び出し）である。
//!
//! # 拡張点は所有者と実装者を分ける（structure.md）
//!
//! 本モジュールが所有するのは**拡張インターフェースだけ**である。実装は `custom-types` が
//! 持ち、依存の向きは `custom-types` → `schema-engine` の一方向である（design.md
//! 「Allowed Dependencies」）。本クレートは `custom-types` を知らない。
//!
//! # 一括経路の継ぎ目（design.md「Registry Layer / TypeRegistry」）
//!
//! 拡張型の実体は JS であることが多く、10 万行でセルごとに境界を越えると性能予算
//! （要件 10.1）に入らない。そこで [`CustomType`] に一括メソッドを置き、**既定実装を
//! 1 件用の判定の繰り返し**にする。境界を越える実装だけがこれを上書きし、呼び出しを列ごとに
//! 1 回へ落とす。既定実装があるため、単純な拡張型は 1 件用の判定だけを書けばよい。
//! `compile` 層は解決した実装を列の計画に 1 つ保持するため、一括経路の内側で行ごとの
//! 動的ディスパッチは起きない（structure.md「そのバッチ経路の内側では動的ディスパッチを
//! 使わない」）。
//!
//! # 識別子は newtype（tasks.md 4.1。要件 11.1, 11.2）
//!
//! [`CustomTypeId`] は登録と解決の鍵である。宣言層（本層の左）は識別子を**文字列として**
//! 保持するため（`Constraints::custom_type`）、解決は文字列を受けて行う。同じ識別子を
//! **ルートスキーマの列にも、入れ子のフィールドにも**同じ形で書ける（要件 11.2）。
//!
//! # 誤りは登録時にだけ生じる
//!
//! 本層が返す誤りは識別子の重複（[`SchemaError::DuplicateCustomTypeId`]。要件 11.4）だけ
//! である。値の違反は本層では表現しない — [`CustomVerdict`] は実装の判定を運ぶだけで、
//! 組込型と同じ違反の形へ落とすのは `validate` 層である（要件 11.3）。未登録の識別子は
//! `None` を返し、誤りではない。その列を使用不能にするかは `compile` 層が決める（要件 11.7）。

use std::collections::BTreeMap;
use std::sync::Arc;

use document_format::CellValue;

use crate::error::SchemaError;

/// 拡張型の識別子（design.md の `custom` の `type`。tasks.md 4.1。要件 11.1, 11.2, 11.4）。
///
/// 登録と解決の鍵である。**組込種別の識別子とは別の名前空間**であり、宣言では
/// `{"kind":"custom","type":"<識別子>"}` と書くため、`kind` のトークン（`"int"` など）とは
/// 衝突しない。`document-format` の識別子（ULID）と違い、利用者が選んだ任意の文字列を
/// そのまま保持する（`Box<str>` を持つのは、宣言層が同じ形
/// `Constraints::custom_type` で識別子を持つためである）。重複の判定は文字列の完全一致で
/// あり、中身は解釈しない（要件 11.4）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomTypeId(Box<str>);

impl CustomTypeId {
    /// 識別子を包む。
    #[inline]
    pub fn new(id: impl Into<Box<str>>) -> Self {
        Self(id.into())
    }

    /// 宣言に書かれたのと同じ文字列。
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 拡張型の 1 件分の判定（design.md「Registry Layer / TypeRegistry」。要件 11.3）。
///
/// **適合と違反の 2 値**であり、「判定できない」という第 3 の値を持たない。判定の失敗は
/// 値の違反ではなく [`CustomTypeFailure`] が表す（別の経路。要件 11.5）。違反の理由は
/// 実装から渡される文脈であり、表示用の文言ではない（`validate` 層が
/// `ViolationReason::CustomRejected` の `reason` 欄へそのまま流す）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomVerdict {
    /// 値が適合する。
    Accepted,
    /// 値が適合しない。
    Rejected {
        /// 実装が返した文脈（表示用の文言ではない）。
        reason: Box<str>,
    },
}

impl CustomVerdict {
    /// 拒否の判定を組み立てる（実装が規則の説明を運ぶための入口）。
    #[inline]
    pub fn rejected(reason: impl Into<Box<str>>) -> Self {
        Self::Rejected {
            reason: reason.into(),
        }
    }
}

/// 拡張型の判定が失敗したこと（design.md「Registry Layer / TypeRegistry」。要件 11.5）。
///
/// 実装の panic を避けたうえで「判定できない」を表す唯一の形である。**その値の違反へ
/// 閉じ込め**、シート全体の検証を中断しない（`validate` 層が
/// `ViolationReason::CustomFailed` へ落とす）。表示用の文言を持たず、実装が渡した文脈
/// （理由）だけを運ぶ（design.md「Error Handling」— 文言は呼び出し元が組み立てる）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomTypeFailure {
    /// 実装が返した文脈（表示用の文言ではない）。
    reason: Box<str>,
}

impl CustomTypeFailure {
    /// 失敗の文脈を包む。
    #[inline]
    pub fn new(reason: impl Into<Box<str>>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// 実装が返した文脈。
    #[inline]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// 組込型と同じ一括経路に載る拡張型（design.md「Registry Layer / TypeRegistry」の
/// Service Interface。tasks.md 4.1。要件 11.1, 11.2）。
///
/// # 契約
///
/// - **実装は panic しない。** 判定できない入力は [`CustomTypeFailure`] を返す。
/// - **応答しない可能性のある実装（JS 呼び出しなど）は、実装側で打ち切って `Err` を返す。**
///   本クレートは打ち切り機構（時間・スレッド）を持たない。時間の管理者は拡張型の実体を
///   知る実装だけであり、`schema-engine` はその中身を知らない。
/// - `Err` はその値の違反になり、シート全体の検証は中断しない（要件 11.5）。ただし
///   [`validate_batch`](Self::validate_batch) の既定実装は `Err` の時点で残りの値を評価せずに
///   戻る。**`Err` までに `out` が受け取った件数が、失敗した値の位置である**（`validate`
///   層はここからその 1 件だけを違反に落とし、次の値から再開できる）。
/// - [`validate_batch`](Self::validate_batch) の上書き実装は、既定実装と**同じ結果**を
///   返さなければならない（design.md の不変条件。境界を越える回数を減らすための上書きで
///   あり、判定規則を変えるためのものではない）。
/// - [`canonicalize`](Self::canonicalize) は一意制約の比較にだけ使う。**保存される値を
///   書き換えない**（要件 7.2。正準形は比較のためだけに作る）。
pub trait CustomType: Send + Sync {
    /// 登録と解決の鍵（要件 11.1, 11.4）。
    fn id(&self) -> &CustomTypeId;

    /// 1 件の判定。
    fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure>;

    /// 1 列分の一括判定。**既定実装は [`validate`](Self::validate) の繰り返し**であり、
    /// 単純な拡張型は 1 件用の判定だけを書けばよい。
    ///
    /// 境界を越える実装はこれを上書きし、列ごとに 1 回の呼び出しへまとめる（要件 11.6。
    /// 10 万行でセルごとに境界を越えると性能予算 10.1 に入らない）。`out` へは値の添字と
    /// 判定を、`values` と同じ順で渡す。
    fn validate_batch(
        &self,
        values: &[CellValue],
        out: &mut dyn FnMut(usize, CustomVerdict),
    ) -> Result<(), CustomTypeFailure> {
        for (index, value) in values.iter().enumerate() {
            out(index, self.validate(value)?);
        }
        Ok(())
    }

    /// 一意制約の比較に使う正準形（design.md「Registry Layer / TypeRegistry」）。
    ///
    /// **既定は値そのもの**であり、正準化を必要としない拡張型は何も書かなくてよい。
    /// 組込型の 10 進数（`types::decimal` の正準形）と同じく、比較のためだけに作り、
    /// 保存される値を変えない。
    fn canonicalize(&self, value: &CellValue) -> Result<CellValue, CustomTypeFailure> {
        Ok(value.clone())
    }
}

/// 拡張型の登録と解決（design.md「コンポーネントとファイルの対応」の `TypeRegistry`。
/// tasks.md 4.1。要件 11.1, 11.2, 11.4）。
///
/// 呼び出し元が保持する（design.md「Service Interface / State の線引き」— `CompiledSchema`
/// と `TypeRegistry` は呼び出し元が持ち、コンパイルと検証の入口は状態を持たない）。
/// [`register`](Self::register) は識別子の重複を拒否し（要件 11.4）、
/// [`resolve`](Self::resolve) は登録済みの実装を返す。**未登録の識別子は `None`** であり、
/// その列を使用不能にするかは `compile` 層が決める（要件 11.7）。
///
/// # 一括経路との関係
///
/// 本型は登録と解決だけを持ち、値の判定を回さない。`compile` 層が [`resolve`](Self::resolve)
/// の結果を列の計画に 1 つ保持するため、行ごとの動的ディスパッチは起きない
/// （structure.md「そのバッチ経路の内側では動的ディスパッチを使わない」）。
#[derive(Default)]
pub struct TypeRegistry {
    /// 登録順ではなく識別子の辞書順に保つ（登録の順序に意味は無く、列挙の順序を実行ごとに
    /// 変えないため）。
    types: BTreeMap<Box<str>, Arc<dyn CustomType>>,
}

impl TypeRegistry {
    /// 登録の無い状態を作る。
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// 拡張型を登録する。**同一の識別子が既に登録されていれば拒否**し、その識別子を運ぶ
    /// 誤りを返す（要件 11.4）。拒否は既存の登録を変えない。
    ///
    /// 同じ実体の再登録も、別の実装による同一識別子の登録も、どちらも同じ誤りである
    /// （どちらを残すかを暗黙に決めない）。
    pub fn register(&mut self, ty: Arc<dyn CustomType>) -> Result<(), SchemaError> {
        let id = ty.id().as_str();
        if self.types.contains_key(id) {
            return Err(SchemaError::DuplicateCustomTypeId { id: id.to_owned() });
        }
        self.types.insert(id.into(), ty);
        Ok(())
    }

    /// 識別子から登録済みの実装を引く（要件 11.2）。未登録は `None`。
    ///
    /// 宣言に書かれた識別子は文字列であるため（`Constraints::custom_type`）、鍵も文字列で
    /// 受ける。列の型としても、入れ子のフィールドの型としても、同じ識別子が同じ実装へ
    /// 解決する。返すのは `Arc` の複製であり、`compile` 層はこれを列の計画に保持して
    /// 一括経路の内側で使う。
    pub fn resolve(&self, id: &str) -> Option<Arc<dyn CustomType>> {
        self.types.get(id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::{ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl};
    use crate::types::TypeKind;
    use document_format::CellValue;
    use std::sync::Arc;

    /// 試験用の拡張型: 文字列が `prefix` で始まるときだけ受理する。
    /// **1 件用の判定だけを実装し、一括判定と正準化は既定実装に任せる。**
    struct Prefixed {
        id: CustomTypeId,
        prefix: &'static str,
    }

    impl Prefixed {
        fn new(id: &str, prefix: &'static str) -> Self {
            Self {
                id: CustomTypeId::new(id),
                prefix,
            }
        }
    }

    impl CustomType for Prefixed {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            match value {
                CellValue::Text(text) if text.starts_with(self.prefix) => {
                    Ok(CustomVerdict::Accepted)
                }
                _ => Ok(CustomVerdict::rejected("値が接頭辞で始まらない")),
            }
        }
    }

    /// 同じ規則を**一括判定の上書き**で判定する実装（境界を越える実装の代役）。
    /// 1 パスで判定を集めてから `out` へ流すため、既定実装とは構造が違う。
    struct PrefixedBatch(Prefixed);

    impl CustomType for PrefixedBatch {
        fn id(&self) -> &CustomTypeId {
            self.0.id()
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            self.0.validate(value)
        }

        fn validate_batch(
            &self,
            values: &[CellValue],
            out: &mut dyn FnMut(usize, CustomVerdict),
        ) -> Result<(), CustomTypeFailure> {
            let mut verdicts = Vec::with_capacity(values.len());
            for value in values {
                verdicts.push(self.0.validate(value)?);
            }
            for (index, verdict) in verdicts.into_iter().enumerate() {
                out(index, verdict);
            }
            Ok(())
        }
    }

    /// 値なしで打ち切る実装（応答しない実装が**実装側で**打ち切る代役。要件 11.5）。
    struct AbortsOnNull {
        id: CustomTypeId,
    }

    impl CustomType for AbortsOnNull {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            match value {
                CellValue::Null => Err(CustomTypeFailure::new("判定を打ち切った")),
                _ => Ok(CustomVerdict::Accepted),
            }
        }
    }

    fn sample_values() -> Vec<CellValue> {
        vec![
            CellValue::Text("〒100-0001".into()),
            CellValue::Text("100-0001".into()),
            CellValue::Null,
            CellValue::Text("〒999".into()),
        ]
    }

    fn batch_of(ty: &dyn CustomType, values: &[CellValue]) -> Vec<(usize, CustomVerdict)> {
        let mut collected = Vec::new();
        ty.validate_batch(values, &mut |index, verdict| {
            collected.push((index, verdict));
        })
        .expect("失敗しない実装である");
        collected
    }

    #[test]
    fn 登録した拡張型を識別子で解決できる() {
        let mut registry = TypeRegistry::new();
        registry
            .register(Arc::new(Prefixed::new("postal-code", "〒")))
            .expect("最初の登録は成功する");

        let resolved = registry
            .resolve("postal-code")
            .expect("登録済みの識別子は解決できる");
        assert_eq!("postal-code", resolved.id().as_str());
        assert!(
            registry.resolve("未登録").is_none(),
            "未登録の識別子は誤りではなく解決不能である（要件 11.7 は compile 層が判断する）"
        );
    }

    #[test]
    fn 既定の一括判定は一件ずつの判定と同じ結果を返す() {
        let ty = Prefixed::new("postal-code", "〒");
        let values = sample_values();

        let batched = batch_of(&ty, &values);
        let one_by_one: Vec<(usize, CustomVerdict)> = values
            .iter()
            .enumerate()
            .map(|(index, value)| (index, ty.validate(value).expect("失敗しない実装である")))
            .collect();

        assert_eq!(one_by_one, batched);
        // 標本が適合と違反の両方を含むこと（同じ結果という主張が自明でないこと）。
        assert_eq!(
            vec![
                (0, CustomVerdict::Accepted),
                (1, CustomVerdict::rejected("値が接頭辞で始まらない")),
                (2, CustomVerdict::rejected("値が接頭辞で始まらない")),
                (3, CustomVerdict::Accepted),
            ],
            batched
        );
    }

    #[test]
    fn 一括判定の上書き実装は既定実装と同じ結果を返す() {
        let values = sample_values();
        let default_impl = Prefixed::new("postal-code", "〒");
        let overridden = PrefixedBatch(Prefixed::new("postal-code", "〒"));

        assert_eq!(
            batch_of(&default_impl, &values),
            batch_of(&overridden, &values)
        );
    }

    #[test]
    fn 一括判定は失敗した時点で戻り失敗までの判定を渡す() {
        let ty = AbortsOnNull {
            id: CustomTypeId::new("postal-code"),
        };
        let values = vec![
            CellValue::Text("a".into()),
            CellValue::Text("b".into()),
            CellValue::Null,
            CellValue::Text("c".into()),
        ];

        let mut delivered = Vec::new();
        let failure = ty
            .validate_batch(&values, &mut |index, verdict| {
                delivered.push((index, verdict));
            })
            .expect_err("打ち切った実装は Err を返す");

        assert_eq!("判定を打ち切った", failure.reason());
        // `Err` までに渡された件数が、失敗した値の位置である（既定実装の契約）。
        assert_eq!(2, delivered.len());
        assert!(delivered
            .iter()
            .all(|(_, verdict)| *verdict == CustomVerdict::Accepted));
    }

    #[test]
    fn 正準化の既定実装は値を逐語でそのまま返す() {
        let ty = Prefixed::new("postal-code", "〒");
        let value = CellValue::Text("〒100-0001".into());

        assert_eq!(Ok(value.clone()), ty.canonicalize(&value));
    }

    #[test]
    fn 同一識別子の再登録は識別子を運ぶ誤りとして拒否される() {
        let mut registry = TypeRegistry::new();
        let first = Arc::new(Prefixed::new("postal-code", "〒"));
        registry
            .register(first.clone())
            .expect("最初の登録は成功する");

        // 同じ実体の再登録も、別の実装による同一識別子の登録も拒否する（要件 11.4）。
        for second in [
            first.clone() as Arc<dyn CustomType>,
            Arc::new(Prefixed::new("postal-code", "〒")) as Arc<dyn CustomType>,
        ] {
            let err = registry
                .register(second)
                .expect_err("同一識別子の再登録は拒否される");
            assert!(
                err.to_string().contains("postal-code"),
                "誤りが該当の識別子を含む: {err}"
            );
            match err {
                SchemaError::DuplicateCustomTypeId { id } => assert_eq!("postal-code", id),
                other => panic!("識別子の重複として拒否されなかった: {other:?}"),
            }
        }

        // 拒否は既存の登録を壊さない。別の識別子は共存できる。
        let registered = first.clone() as Arc<dyn CustomType>;
        let resolved = registry.resolve("postal-code").expect("既存の登録が残る");
        assert!(Arc::ptr_eq(&registered, &resolved));
        registry
            .register(Arc::new(Prefixed::new("product-code", "P-")))
            .expect("別の識別子は登録できる");
        assert_eq!(
            "product-code",
            registry
                .resolve("product-code")
                .expect("登録済み")
                .id()
                .as_str()
        );
    }

    /// 拡張型の識別子だけを持つ型の宣言。
    fn custom_type_decl(id: &str) -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(TypeKind::Custom),
            constraints: Constraints {
                custom_type: Some(id.into()),
                ..Constraints::default()
            },
        }
    }

    /// 宣言に書かれた拡張型の識別子を取り出す（列と入れ子のフィールドの両方で使う）。
    fn declared_custom_id(ty: &TypeDecl) -> Option<&str> {
        match ty {
            TypeDecl::Kind {
                kind: DeclaredKind::Known(TypeKind::Custom),
                constraints,
            } => constraints.custom_type.as_deref(),
            _ => None,
        }
    }

    #[test]
    fn 登録された拡張型を列と入れ子のフィールドの型として指定できる() {
        let mut registry = TypeRegistry::new();
        registry
            .register(Arc::new(Prefixed::new("postal-code", "〒")))
            .expect("登録できる");

        // 列に直接書いた拡張型と、object の入れ子のフィールドに書いた拡張型。
        let schema = Schema {
            columns: vec![
                ColumnDecl {
                    name: "郵便番号".into(),
                    ty: custom_type_decl("postal-code"),
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "連絡先".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Object),
                        constraints: Constraints {
                            fields: vec![FieldDecl {
                                name: "自宅".into(),
                                ty: custom_type_decl("postal-code"),
                                required: false,
                                default: None,
                                description: None,
                            }],
                            ..Constraints::default()
                        },
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
            ],
        };

        let from_column = declared_custom_id(&schema.columns[0].ty).expect("列に識別子がある");
        let nested_ty = match &schema.columns[1].ty {
            TypeDecl::Kind { constraints, .. } => &constraints.fields[0].ty,
            other => panic!("object の宣言ではない: {other:?}"),
        };
        let from_field = declared_custom_id(nested_ty).expect("入れ子のフィールドに識別子がある");
        assert_eq!(from_column, from_field);

        // どちらの位置に書かれた識別子も、同じ登録済み実装へ解決する。
        let column_impl = registry
            .resolve(from_column)
            .expect("列の識別子が解決できる");
        let field_impl = registry
            .resolve(from_field)
            .expect("入れ子のフィールドの識別子が解決できる");
        assert!(Arc::ptr_eq(&column_impl, &field_impl));
        assert_eq!(
            CustomVerdict::Accepted,
            column_impl
                .validate(&CellValue::Text("〒100-0001".into()))
                .expect("失敗しない実装である")
        );
    }
}

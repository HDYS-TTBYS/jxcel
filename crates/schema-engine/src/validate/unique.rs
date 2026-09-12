//! 一意制約の 1 パス判定（design.md「コンポーネントとファイルの対応」の `UniqueScan`。
//! tasks.md 5.2 が実装する）。
//!
//! 一意制約を持つ列を 1 回の走査で判定し、重複が見つかったとき重複するすべての行の
//! 識別子を違反に含める（要件 4.6, 4.7）。比較は正準形の上で行う（10 進数は宣言された
//! 桁数に揃え、拡張型は登録された正準化を通す）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは `compile` 層までと、同じ `validate` 層の
//! [`super::report`] を参照する。
//!
//! # 判定の規則（tasks.md 5.2 と前段の裁定）
//!
//! - **1 パス**: 行を 1 回だけ走査し、その内側で一意制約を持つ列をすべて見る。列ごとに
//!   全行を舐め直さない。
//! - **対象の列に閉じる**: `selected` があるときはその列だけを走査する（要件 10.5。費用が
//!   指定した列数に比例する）。全列のときは計画の一覧を借りたままで、確保を増やさない。
//! - **値なしは対象外**: [`CellValue::Null`] と、列数に満たない行の欠けた値は比較しない
//!   （値なしの扱いは列の `required` が決める。要件 4.4）。一意制約が値なしの重複を報告
//!   すると、任意の一意列を空のまま残しただけで違反になる。
//! - **正準形の上で比較**: 10 進数は値厳密形（`1.5` と `1.50` と `1.5e0` が同じ。桁は
//!   落とさない）、日時は**瞬時**（tasks.md 2.3 の裁定。`+09:00` と `Z` の書き分けで
//!   一意制約を回避できないようにする）、拡張型は登録された
//!   [`CustomType::canonicalize`](crate::registry::CustomType::canonicalize) を通す。
//! - **重複するすべての行を 1 件に載せる**: 値ごとに 1 件の違反を作り、その値を持つ
//!   すべての行の識別子を行の並び順で `rows` に載せる（要件 4.7）。
//! - **並びは決定的**: 重複の違反は「最初に現れた行の位置 → 列の添字」の昇順で返す。
//!   `HashMap` の反復順は結果へ漏らさない（要件 5.5 の安定併合の前提）。
//! - **使用不能な列は載らない**: [`CompiledSchema::unique_columns`] が使用不能な列を
//!   除いている（正準形を作る手段が無いため。要件 11.7）。
//!
//! # 呼び出し元へ返す形
//!
//! 違反の並びを [`Vec`] で返し、[`ViolationReport`](super::report::ViolationReport) へ
//! 直接押し込まない。一意性の違反は行を跨ぐため、第 1 段（[`super::cell`]）の結果と
//! **安定併合してから**順序を確定する必要がある（design.md「検証結果の表現」）。
//! 併合は一括検証（タスク 5.4）の仕事である。

use std::borrow::Cow;
use std::collections::HashMap;

use document_format::{AttachmentId, CellValue, NestedValue, RowId};
use jiff::civil;
use jiff::Timestamp;

use crate::compile::plan::{ColumnIndex, ColumnValidator};
use crate::compile::CompiledSchema;
use crate::types::datetime::{TemporalForm, TemporalValue};
use crate::types::decimal::{self, DecimalCanonical};

use super::report::{Expected, Violation, ViolationReason};

/// 一意制約の 1 パス判定（tasks.md 5.2。要件 4.6, 4.7）。
///
/// `rows` は行の並び順に `(行の識別子, その行の値)` を渡す。値の並びは
/// [`CompiledSchema::columns`] と同じ添字で並ぶ（design.md「Data Models / Domain Model」
/// の不変条件）。列数に満たない行の欠けた値は値なしとして扱い、比較しない。
///
/// 一意制約を持つ列（[`CompiledSchema::unique_columns`]）を 1 回の走査で見て、重複した値
/// ごとに 1 件の [`ViolationReason::Duplicate`] を組にする。違反は「最初に現れた行の位置
/// → 列の添字」の昇順で返す（要件 5.5 の安定併合の基準）。
///
/// `selected` は走査の対象の列である（`None` は全列）。指定があるときは**その列だけ**を
/// 見て、行ごとの費用を対象の列数に閉じる（要件 10.5）。並びは**昇順・重複除去済み**で
/// あること（[`super::validate_columns`] が正規化する）。
pub fn scan<'a, I>(
    schema: &CompiledSchema,
    rows: I,
    selected: Option<&[ColumnIndex]>,
) -> Vec<Violation>
where
    I: IntoIterator<Item = (RowId, &'a [CellValue])>,
{
    // 走査する列を先に閉じる（要件 10.5）。全列のときは計画の一覧を借りたままにして確保を
    // 増やさない（`validate_sheet` が通る経路）。
    let unique: Cow<'_, [ColumnIndex]> = match selected {
        Some(selected) => Cow::Owned(
            schema
                .unique_columns()
                .iter()
                .copied()
                .filter(|column| selected.binary_search(column).is_ok())
                .collect(),
        ),
        None => Cow::Borrowed(schema.unique_columns()),
    };
    if unique.is_empty() {
        return Vec::new();
    }

    // 列ごとの表。1 行につき 1 度だけ全列を見る（1 パス）。
    let mut tables: Vec<HashMap<Canonical<'a>, Entry<'a>>> =
        unique.iter().map(|_| HashMap::new()).collect();

    for (row_index, (row, values)) in rows.into_iter().enumerate() {
        for (slot, column) in unique.iter().enumerate() {
            // 使用不能な列は `unique_columns` に載らないが、計画の外の添字に備えておく。
            let Some(validator) = schema.validator(*column) else {
                continue;
            };
            let Some(value) = values.get(column.index()) else {
                continue;
            };
            // 値なしは一意性の対象外である（モジュール docs「判定の規則」）。
            if matches!(value, CellValue::Null) {
                continue;
            }
            let key = canonical(validator, value);
            match tables[slot].entry(key) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(Entry {
                        first_index: row_index,
                        first_row: row,
                        first_value: value,
                        others: None,
                    });
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    let entry = entry.get_mut();
                    match &mut entry.others {
                        Some(others) => others.push(row),
                        // 2 件目で初めて `Vec` を確保する。100k 行の大半（重複の無い値）で
                        // 行ごとの確保を払わない。
                        None => entry.others = Some(vec![entry.first_row, row]),
                    }
                }
            }
        }
    }

    let mut found: Vec<(usize, Violation)> = Vec::new();
    for (slot, column) in unique.iter().enumerate() {
        for entry in std::mem::take(&mut tables[slot]).into_values() {
            let Some(rows) = entry.others else {
                continue;
            };
            let name = schema
                .columns()
                .get(column.index())
                .map(String::as_str)
                .unwrap_or_default();
            found.push((
                entry.first_index,
                Violation::at_cell(
                    Some(entry.first_row),
                    *column,
                    name,
                    ViolationReason::Duplicate {
                        expected: Expected::Unique,
                        actual: entry.first_value.clone(),
                        rows,
                    },
                ),
            ));
        }
    }
    // `HashMap` の反復順を結果へ漏らさない（要件 5.5）。
    found.sort_by_key(|(first_index, violation)| (*first_index, violation.column()));
    found.into_iter().map(|(_, violation)| violation).collect()
}

/// 一意制約を持つ列の 1 値分の集計。
struct Entry<'a> {
    /// 最初に現れた行の位置（行の並び順。安定併合の基準）。
    first_index: usize,
    /// 最初に現れた行の識別子（違反が属する行）。
    first_row: RowId,
    /// 最初に現れた値そのもの（違反の理由が運ぶ実際の値。書き換えない）。
    first_value: &'a CellValue,
    /// 2 件目以降の行。重複が無ければ `None`。
    others: Option<Vec<RowId>>,
}

/// 一意性の比較に使う正準形（tasks.md 5.2）。
///
/// **値の同一性**が等値（[`PartialEq`]）とハッシュ（[`Hash`]）に一致するように、表記では
/// なく値を畳む。文字列は入力から借用し、走査のたびに複製しない（値の大半はテキストで
/// あり、100k 件では複製の費用がそのまま出る）。
///
/// 畳み方の源は既存の規則である: 10 進数は [`decimal::canonicalize`]（値厳密形）、
/// 日時は [`TemporalValue`] の等値（瞬時。tasks.md 2.3 の裁定）、拡張型は
/// [`CustomType::canonicalize`](crate::registry::CustomType::canonicalize) である。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Canonical<'a> {
    /// 値なし（比較の対象外だが、拡張型の正準化が値なしを返す場合に備えて持つ）。
    Null,
    /// 真偽。
    Bool(bool),
    /// 整数。
    Int(i64),
    /// 浮動小数（`-0.0` を `0.0` へ畳んだビット列）。
    Float(u64),
    /// 10 進数の値厳密形。
    Decimal(DecimalCanonical),
    /// 文法に一致しない 10 進数（上流の脱出口。逐語で比較する）。
    DecimalVerbatim(Cow<'a, str>),
    /// テキスト（`text` / `enum` / `ref` と、解釈できない日時の値）。
    Text(Cow<'a, str>),
    /// 添付参照（内容アドレス）。
    Attachment(AttachmentId),
    /// 日時の値（瞬時または civil の成分）。
    Temporal(TemporalKey),
    /// 入れ子のオブジェクト。値の並びをそのまま保つ（[`CellValue`] の等値と同じ規則）。
    Object(Vec<(Cow<'a, str>, Canonical<'a>)>),
    /// 入れ子の配列。
    Array(Vec<Canonical<'a>>),
}

/// 日時の正準形（[`TemporalValue`] の等値をハッシュへ写す）。
///
/// `Instant` は書かれたローカル日時とオフセットではなく**瞬時**（`at`）で畳む
/// （tasks.md 2.3 の裁定）。`Date` / `Civil` は書かれた成分が値そのものである。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TemporalKey {
    /// 日付のみの値。
    Date(civil::Date),
    /// オフセットを持たない civil 日時の値。
    Civil(civil::DateTime),
    /// オフセット付きの瞬時。
    Instant(Timestamp),
}

/// 値 1 つを検証器の型に合わせて正準形へ写す（tasks.md 5.2）。
fn canonical<'a>(validator: &ColumnValidator, value: &'a CellValue) -> Canonical<'a> {
    match validator {
        // 日時は書かれたオフセットではなく瞬時で比較する（tasks.md 2.3 の裁定）。
        ColumnValidator::Date { .. } => match value {
            CellValue::Text(text) => match TemporalForm::Date.parse(text) {
                Some(parsed) => Canonical::Temporal(temporal_key(parsed)),
                None => Canonical::Text(Cow::Borrowed(text)),
            },
            other => borrowed(other),
        },
        ColumnValidator::DateTime { form, .. } => match value {
            CellValue::Text(text) => match form.parse(text) {
                Some(parsed) => Canonical::Temporal(temporal_key(parsed)),
                None => Canonical::Text(Cow::Borrowed(text)),
            },
            other => borrowed(other),
        },
        // 拡張型は登録された正準化を通す（tasks.md 5.2）。正準化が失敗した値は逐語で
        // 比較する（その値は第 1 段で `CustomFailed` として報告済みである）。
        ColumnValidator::Custom { imp, .. } => match imp.canonicalize(value) {
            Ok(canonicalized) => owned(canonicalized),
            Err(_) => borrowed(value),
        },
        // 10 進数は `borrowed` が値厳密形へ畳む。他の種別は値の変種がそのまま正準形である。
        _ => borrowed(value),
    }
}

/// 借りた値の正準形（[`CellValue`] の等値をそのまま写す。10 進数だけ値厳密形へ畳む）。
fn borrowed<'a>(value: &'a CellValue) -> Canonical<'a> {
    match value {
        CellValue::Null => Canonical::Null,
        CellValue::Bool(inner) => Canonical::Bool(*inner),
        CellValue::Int(inner) => Canonical::Int(*inner),
        CellValue::Float(inner) => Canonical::Float(float_key(*inner)),
        CellValue::Decimal(text) => match decimal::canonicalize(text) {
            Some(canonical) => Canonical::Decimal(canonical),
            None => Canonical::DecimalVerbatim(Cow::Borrowed(text)),
        },
        CellValue::Text(text) => Canonical::Text(Cow::Borrowed(text)),
        CellValue::Attachment(id) => Canonical::Attachment(*id),
        CellValue::Nested(NestedValue::Object(entries)) => Canonical::Object(
            entries
                .iter()
                .map(|(key, value)| (Cow::Borrowed(key.as_str()), borrowed(value)))
                .collect(),
        ),
        CellValue::Nested(NestedValue::Array(items)) => {
            Canonical::Array(items.iter().map(borrowed).collect())
        }
    }
}

/// 所有する値の正準形（拡張型の正準化が返した値を、借用元の寿命に縛られずに持つ）。
fn owned(value: CellValue) -> Canonical<'static> {
    match value {
        CellValue::Null => Canonical::Null,
        CellValue::Bool(inner) => Canonical::Bool(inner),
        CellValue::Int(inner) => Canonical::Int(inner),
        CellValue::Float(inner) => Canonical::Float(float_key(inner)),
        CellValue::Decimal(text) => match decimal::canonicalize(&text) {
            Some(canonical) => Canonical::Decimal(canonical),
            None => Canonical::DecimalVerbatim(Cow::Owned(text)),
        },
        CellValue::Text(text) => Canonical::Text(Cow::Owned(text)),
        CellValue::Attachment(id) => Canonical::Attachment(id),
        CellValue::Nested(NestedValue::Object(entries)) => Canonical::Object(
            entries
                .into_iter()
                .map(|(key, value)| (Cow::Owned(key), owned(value)))
                .collect(),
        ),
        CellValue::Nested(NestedValue::Array(items)) => {
            Canonical::Array(items.into_iter().map(owned).collect())
        }
    }
}

/// 浮動小数の比較キー（`-0.0` は `0.0` と同じ値である）。
fn float_key(value: f64) -> u64 {
    let value = if value == 0.0 { 0.0 } else { value };
    value.to_bits()
}

/// 日時の値を比較キーへ写す（[`TemporalValue`] の等値と同じ基準）。
fn temporal_key(value: TemporalValue) -> TemporalKey {
    match value {
        TemporalValue::Date(date) => TemporalKey::Date(date),
        TemporalValue::Civil(local) => TemporalKey::Civil(local),
        TemporalValue::Instant { at, .. } => TemporalKey::Instant(at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile_declaration;
    use crate::declaration::{ColumnDecl, Constraints, DeclaredKind, Schema, TypeDecl};
    use crate::registry::{
        CustomType, CustomTypeFailure, CustomTypeId, CustomVerdict, TypeRegistry,
    };
    use crate::types::datetime::OffsetPolicy;
    use crate::types::decimal::DecimalDigits;
    use crate::types::TypeKind;
    use document_format::IdFactory;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// 種別と制約による列を組み立てる。
    fn column(name: &str, kind: TypeKind, constraints: Constraints, unique: bool) -> ColumnDecl {
        ColumnDecl {
            name: name.into(),
            ty: TypeDecl::Kind {
                kind: DeclaredKind::Known(kind),
                constraints,
            },
            required: false,
            unique,
            default: None,
            description: None,
        }
    }

    /// 一意制約つきの `text` 列 1 本を組み立てる。
    fn unique_text(name: &str) -> ColumnDecl {
        column(name, TypeKind::Text, Constraints::default(), true)
    }

    /// 一意制約を持たない列を組み立てる。
    fn plain(name: &str, kind: TypeKind) -> ColumnDecl {
        column(name, kind, Constraints::default(), false)
    }

    /// 種別による指定のルートスキーマをコンパイルする。
    fn compiled_with(columns: Vec<ColumnDecl>, registry: &TypeRegistry) -> CompiledSchema {
        compile_declaration(&Schema { columns }, &[], registry)
            .expect("標本の宣言はコンパイルできる")
    }

    /// 拡張型の登録の無いスキーマをコンパイルする。
    fn compiled(columns: Vec<ColumnDecl>) -> CompiledSchema {
        compiled_with(columns, &TypeRegistry::new())
    }

    /// 値の並びを走査し、発行した行識別子と違反を返す。
    ///
    /// 行識別子は ULID を発行する（`document-format` の正典の経路）。標本は小さいため
    /// これで足りる。
    fn scanned(schema: &CompiledSchema, values: &[Vec<CellValue>]) -> (Vec<RowId>, Vec<Violation>) {
        scanned_selected(schema, values, None)
    }

    /// 対象の列を指定して値の並びを走査する（要件 10.5。`None` は全列）。
    fn scanned_selected(
        schema: &CompiledSchema,
        values: &[Vec<CellValue>],
        selected: Option<&[ColumnIndex]>,
    ) -> (Vec<RowId>, Vec<Violation>) {
        let mut factory = IdFactory::new();
        let ids: Vec<RowId> = (0..values.len()).map(|_| factory.new_row_id()).collect();
        let violations = scan(
            schema,
            ids.iter().copied().zip(values.iter().map(Vec::as_slice)),
            selected,
        );
        (ids, violations)
    }

    /// テキストのセル値。
    fn text(value: &str) -> CellValue {
        CellValue::Text(value.to_owned())
    }

    /// 10 進数のセル値。
    fn decimal(value: &str) -> CellValue {
        CellValue::Decimal(value.to_owned())
    }

    /// 重複の違反が運ぶ行の識別子。
    fn duplicate_rows(violation: &Violation) -> &[RowId] {
        match violation.reason() {
            ViolationReason::Duplicate { rows, .. } => rows,
            other => panic!("重複以外の理由が返った: {other:?}"),
        }
    }

    /// `〒` で始まるテキストだけを受理し、正準形では大文字へ畳む標本の拡張型
    /// （要件 11.3。正準化は要件 4.7 の比較に使う）。
    struct Upcase {
        id: CustomTypeId,
    }

    impl CustomType for Upcase {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            match value {
                CellValue::Text(text) if text.starts_with('〒') => Ok(CustomVerdict::Accepted),
                _ => Ok(CustomVerdict::rejected("〒 で始まらない")),
            }
        }

        fn canonicalize(&self, value: &CellValue) -> Result<CellValue, CustomTypeFailure> {
            match value {
                CellValue::Text(text) => Ok(CellValue::Text(text.to_uppercase())),
                other => Ok(other.clone()),
            }
        }
    }

    /// 標本の拡張型を 1 つ登録した辞書を組み立てる。
    fn registry_with(id: &str, implementation: impl CustomType + 'static) -> TypeRegistry {
        debug_assert_eq!(id, implementation.id().as_str());
        let mut registry = TypeRegistry::new();
        registry
            .register(Arc::new(implementation))
            .expect("標本の拡張型は登録できる");
        registry
    }

    /// 正準化の呼び出し回数を数える拡張型（列ごとの走査の費用を観測する）。
    struct Counting {
        id: CustomTypeId,
        calls: Arc<AtomicUsize>,
    }

    impl CustomType for Counting {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, _value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            Ok(CustomVerdict::Accepted)
        }

        fn canonicalize(&self, value: &CellValue) -> Result<CellValue, CustomTypeFailure> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(value.clone())
        }
    }

    /// 一意制約を持つ列を指定したとき、その列**だけ**を走査する（tasks.md 5.4。要件 10.5）。
    ///
    /// 費用は対象の列数に比例しなければならない — 全列を走査してから結果を絞る実装では、
    /// 指定していない列の正準化も走ってしまう。正準化は 1 行 1 列につき 1 回だけ呼ばれる
    /// ため、その回数が「走査した列 × 行数」を直接に観測する。
    #[test]
    fn only_the_selected_unique_columns_are_examined() {
        let left_calls = Arc::new(AtomicUsize::new(0));
        let right_calls = Arc::new(AtomicUsize::new(0));
        let mut registry = TypeRegistry::new();
        for (id, calls) in [
            ("left", Arc::clone(&left_calls)),
            ("right", Arc::clone(&right_calls)),
        ] {
            registry
                .register(Arc::new(Counting {
                    id: CustomTypeId::new(id),
                    calls,
                }))
                .expect("標本の拡張型は登録できる");
        }
        let custom = |name: &str, id: &str| {
            column(
                name,
                TypeKind::Custom,
                Constraints {
                    custom_type: Some(id.into()),
                    ..Constraints::default()
                },
                true,
            )
        };
        let schema = compiled_with(vec![custom("左", "left"), custom("右", "right")], &registry);
        let values: Vec<Vec<CellValue>> = (0..8)
            .map(|index| vec![text(&format!("L{index}")), text(&format!("R{index}"))])
            .collect();

        // 全列の走査は両方の列を見る。
        let (_, all) = scanned_selected(&schema, &values, None);
        assert!(all.is_empty(), "重複が無いのに違反が出た");
        assert_eq!(8, left_calls.load(Ordering::Relaxed));
        assert_eq!(8, right_calls.load(Ordering::Relaxed));

        // 1 列だけの走査はその列だけを見る（指定していない列の値に触れない）。
        left_calls.store(0, Ordering::Relaxed);
        right_calls.store(0, Ordering::Relaxed);
        let (_, one) = scanned_selected(&schema, &values, Some(&[ColumnIndex::new(0)]));
        assert!(one.is_empty(), "重複が無いのに違反が出た");
        assert_eq!(
            8,
            left_calls.load(Ordering::Relaxed),
            "指定した列が走査されていない"
        );
        assert_eq!(
            0,
            right_calls.load(Ordering::Relaxed),
            "指定していない列が走査された"
        );
    }

    /// 重複する値を持つ列は、重複するすべての行の識別子を 1 件の違反に載せる（要件 4.7）。
    #[test]
    fn duplicate_values_report_every_row_that_holds_them() {
        let schema = compiled(vec![unique_text("品番")]);
        let values = vec![
            vec![text("A")],
            vec![text("B")],
            vec![text("A")],
            vec![text("C")],
            vec![text("A")],
            vec![text("B")],
        ];
        let (ids, violations) = scanned(&schema, &values);

        assert_eq!(2, violations.len(), "重複した値の数だけ違反が出ない");
        assert_eq!(0, violations[0].column().index());
        assert_eq!("品番", violations[0].column_name());
        assert_eq!(&Expected::Unique, violations[0].reason().expected());
        assert_eq!(&text("A"), violations[0].reason().actual());
        assert_eq!(&[ids[0], ids[2], ids[4]], duplicate_rows(&violations[0]));

        assert_eq!(&Expected::Unique, violations[1].reason().expected());
        assert_eq!(&text("B"), violations[1].reason().actual());
        assert_eq!(&[ids[1], ids[5]], duplicate_rows(&violations[1]));
    }

    /// 重複しない値だけの列は違反を出さない（要件 4.6, 4.7）。
    #[test]
    fn distinct_values_produce_no_violation() {
        let schema = compiled(vec![unique_text("品番")]);
        let values = vec![vec![text("A")], vec![text("B")], vec![text("C")]];
        let (_, violations) = scanned(&schema, &values);
        assert!(violations.is_empty(), "重複が無いのに違反が出た");
    }

    /// 一意制約を持たない列は走査しない（要件 4.6 は列に一意を宣言できることを求める）。
    #[test]
    fn columns_without_the_unique_constraint_are_not_scanned() {
        let schema = compiled(vec![plain("品番", TypeKind::Text)]);
        let values = vec![vec![text("A")], vec![text("A")]];
        let (_, violations) = scanned(&schema, &values);
        assert!(violations.is_empty(), "一意でない列に違反が出た");
    }

    /// 値なしは一意性の対象外である（要件 4.7 の「重複する値」は値がある場合の話である）。
    ///
    /// 値なしの扱いは列の `required` が決める（要件 4.4）。一意制約が値なしの重複を
    /// 報告すると、任意の一意列を空のまま残しただけで違反になる。
    #[test]
    fn values_without_a_value_are_not_duplicates() {
        let schema = compiled(vec![unique_text("品番")]);
        let values = vec![vec![CellValue::Null], vec![], vec![CellValue::Null]];
        let (_, violations) = scanned(&schema, &values);
        assert!(violations.is_empty(), "値なしが重複として報告された");
    }

    /// 10 進数は値の正準形で比較する（tasks.md 5.2。桁数に揃えた形の値が同じなら重複）。
    #[test]
    fn decimal_values_compare_on_the_canonical_form() {
        let digits = DecimalDigits::new(4, 2).expect("標本の桁数は構成できる");
        let constraints = Constraints {
            digits: Some(digits),
            ..Constraints::default()
        };
        let schema = compiled(vec![column("単価", TypeKind::Decimal, constraints, true)]);
        let values = vec![
            vec![decimal("1.5")],
            vec![decimal("1.50")],
            vec![decimal("1.5e0")],
            vec![decimal("1.51")],
        ];
        let (ids, violations) = scanned(&schema, &values);

        assert_eq!(1, violations.len());
        assert_eq!(&[ids[0], ids[1], ids[2]], duplicate_rows(&violations[0]));
        assert_eq!(&decimal("1.5"), violations[0].reason().actual());
    }

    /// 日時は書かれたオフセットではなく瞬時で比較する（tasks.md 2.3 の裁定）。
    #[test]
    fn datetime_values_compare_by_instant() {
        let constraints = Constraints {
            offset: Some(OffsetPolicy::Required),
            ..Constraints::default()
        };
        let schema = compiled(vec![column(
            "記録時刻",
            TypeKind::DateTime,
            constraints,
            true,
        )]);
        let values = vec![
            vec![text("2026-09-12T10:30:00+09:00")],
            vec![text("2026-09-12T01:30:00Z")],
            vec![text("2026-09-12T01:30:00+09:00")],
        ];
        let (ids, violations) = scanned(&schema, &values);

        assert_eq!(1, violations.len(), "同じ瞬時が別の値として扱われた");
        assert_eq!(&[ids[0], ids[1]], duplicate_rows(&violations[0]));
    }

    /// 拡張型は登録された正準化を通して比較する（tasks.md 5.2。要件 4.7）。
    #[test]
    fn custom_types_compare_through_their_registered_canonical_form() {
        let registry = registry_with(
            "postal-code",
            Upcase {
                id: CustomTypeId::new("postal-code"),
            },
        );
        let constraints = Constraints {
            custom_type: Some("postal-code".into()),
            ..Constraints::default()
        };
        let schema = compiled_with(
            vec![column("郵便番号", TypeKind::Custom, constraints, true)],
            &registry,
        );
        let values = vec![
            vec![text("〒abc")],
            vec![text("〒ABC")],
            vec![text("〒def")],
        ];
        let (ids, violations) = scanned(&schema, &values);

        assert_eq!(1, violations.len(), "正準化を経た重複が検出されない");
        assert_eq!(&[ids[0], ids[1]], duplicate_rows(&violations[0]));
    }

    /// 使用不能な列は一意性の判定に載らない（要件 11.7。正準形を作る手段が無い）。
    #[test]
    fn unusable_columns_are_not_scanned() {
        let mut declaration = column("謎", TypeKind::Text, Constraints::default(), true);
        declaration.ty = TypeDecl::Kind {
            kind: DeclaredKind::Unknown("future-kind".into()),
            constraints: Constraints::default(),
        };
        let schema = compiled(vec![declaration]);
        let values = vec![vec![text("A")], vec![text("A")]];
        let (_, violations) = scanned(&schema, &values);
        assert!(violations.is_empty(), "使用不能な列が走査された");
    }

    /// 2 段の併合に備え、違反は行の並び順 → 列の添字の順に並ぶ（要件 5.5）。
    #[test]
    fn violations_are_ordered_by_row_then_column() {
        let schema = compiled(vec![unique_text("左"), unique_text("右")]);
        let values = vec![
            vec![text("A"), text("P")],
            vec![text("B"), text("P")],
            vec![text("B"), text("Q")],
        ];
        let (_, violations) = scanned(&schema, &values);

        assert_eq!(2, violations.len());
        // 「右」の重複は行 0 から始まり、「左」の重複は行 1 から始まる。
        assert_eq!(1, violations[0].column().index(), "列の順で並んでいない");
        assert_eq!(0, violations[1].column().index());
    }

    /// 1 列 10 万件の判定が一括で完了する（tasks.md 5.2。要件 4.6, 4.7）。
    #[test]
    fn a_hundred_thousand_rows_in_one_column_are_scanned_in_one_pass() {
        let schema = compiled(vec![unique_text("品番")]);
        let values: Vec<Vec<CellValue>> = (0..100_000)
            .map(|index| vec![text(&format!("品番-{index:06}"))])
            .collect();
        let (_, violations) = scanned(&schema, &values);
        assert!(violations.is_empty(), "重複が無いのに違反が出た");
    }
}

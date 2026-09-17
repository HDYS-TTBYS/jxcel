//! シート間参照の一括実在判定（design.md「コンポーネントとファイルの対応」の
//! `ReferenceScan`。tasks.md 5.3。要件 9.2, 9.3, 9.4, 9.5, 9.6）。
//!
//! 参照先シートの行識別子の集合を**シートごとに 1 回だけ**作り、行ごとの個別の問い合わせを
//! 行わない（要件 9.6）。参照先の行が実在しないときは参照元の行と列および参照先の識別子を
//! 含む違反として報告する（要件 9.3）。参照されている行の削除には介入しない（要件 9.4。
//! 本モジュールは [`Document`] を不変で借りるだけで、文書を変更する経路を持たない）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは `compile` 層まで、`document-format` のモデル、同じ
//! `validate` 層の [`super::report`] を参照する。`write` 以降は参照しない。
//!
//! # 判定の規則（tasks.md 5.3 と前段の裁定）
//!
//! - **一括**: 参照先シートの行識別子の集合を、そのシートを指す列の数に依らず 1 回だけ
//!   作る（要件 9.6）。行の走査は 1 回であり、参照先への問い合わせは集合の所属判定だけ
//!   である。
//! - **対象の列に閉じる**: `selected` があるときはその列の形だけを組み立てる（要件 10.5。
//!   費用が指定した列数に比例する）。参照を持たない列だけを指定したときは形が空になり、
//!   行を舐める前に戻る。
//! - **参照の値は行識別子のテキスト**: `ref` 列（design.md「組込型カタログと `CellValue`
//!   への写像」の `ref`）が受け入れるのは `Text` である（`compile::plan` の
//!   [`ColumnValidator::Ref`]）。実在の判定は**識別子として**行い、綴りでは行わない —
//!   同じ識別子を指す小文字の綴りは実在する行への参照である（`document-format` の
//!   識別子の解析は大小文字を問わない）。識別子として解釈できないテキストは、どの行を
//!   も指さない（要件 9.3）。
//! - **値なしと参照でない値は対象外**: [`CellValue::Null`] は参照ではなく（必須の判定は
//!   `validate::cell` が持つ。要件 4.4）、テキストでない値は型の不一致として同じ
//!   `validate::cell` が報告する（要件 2.7）。本モジュールが報告するのは「参照の値が
//!   実在しない」ことだけであり、1 つの値について違反が二重に載ることはない。
//! - **入れ子の内側も判定する**: 参照は `Object` のフィールドや `Array` の `items` の
//!   内側にも置ける（要件 3.2）。要件 9.2 は「参照を持つ値が検証されたとき」実在を判定
//!   することを求めるため、降りた先の参照も判定し、違反は**入れ子の位置**
//!   （[`ValuePath`]。要件 3.4）で報告する。列は内側の参照を含む**最上位の列**であり、
//!   これは `validate::cell` の入れ子の報告と同じ規約である。
//! - **参照先シートが削除されたとき**: そのシートの行集合は空である。したがって
//!   そのシートを指す**すべての参照**が違反になる（要件 9.5）。削除そのものは本モジュール
//!   の外の操作であり、本モジュールは妨げない（要件 9.4）。
//! - **並びは決定的**: 違反は行の並び順 → 列の添字 → 入れ子の位置（深さ優先）の順に返す。
//!   行を跨ぐ判定の結果を安定併合する側（タスク 5.4）が同じ基準で並べられるようにする
//!   （要件 5.5）。
//!
//! # 呼び出し元へ返す形
//!
//! 違反の並びを [`Vec`] で返し、[`ViolationReport`](super::report::ViolationReport) へ
//! 直接押し込まない（[`super::unique`] と同じ規約）。行を跨ぐ判定の結果は第 1 段
//! （`validate::cell`）の結果と**安定併合してから**順序を確定する必要がある
//! （design.md「検証結果の表現」）。併合は一括検証（タスク 5.4）の仕事である。
//!
//! # 参照の形は検証器の木から組む（計画の参照の組を削除した裁定）
//!
//! 計画（[`CompiledSchema`]）は当初、最上位の `ref` 列と参照先シートの対を状態に
//! 持っていた（design.md「Compile Layer / SchemaCompiler」の State Management。
//! タスク 4.4）。しかし入れ子の内側の参照まで判定するには、検証器の木（列ごとの
//! [`ColumnValidator`]）から参照の位置を辿り直す必要があり、最上位の `ref` 列だけを
//! 組にした状態では要件 9.2 を満たせない。本モジュールはその形を [`shape_of`] で 1 回
//! だけ組み立て、行ごとに辿る。この裁定により計画側の当該の状態は**削除**され、
//! design.md の状態一覧の当該項は**本モジュールの検証器の木からの走査に置き換わる**
//! （タスク 5.3）。消費者が無い API を残さないための削除であり、参照の組が要る
//! タスクが現れたなら消費者と共に計画へ戻す。参照を 1 つも含まない計画では
//! [`scan`] が空を返すため、呼び出し元は**判定の要否を自分で確かめずに呼んでよい**。

use std::collections::HashSet;
use std::str::FromStr;

use document_format::{CellValue, Document, NestedValue, Row, RowId, SheetId};

use crate::compile::plan::{ColumnIndex, ColumnValidator};
use crate::compile::CompiledSchema;

use super::report::{Expected, ValuePath, ValuePathSegment, Violation, ViolationReason};

/// シート間参照の一括実在判定（tasks.md 5.3。要件 9.2, 9.3, 9.4, 9.5, 9.6）。
///
/// `rows` は行の並び順に `(行の識別子, その行の値)` を渡す。値の並びは
/// [`CompiledSchema::columns`] と同じ添字で並ぶ（design.md「Data Models / Domain Model」
/// の不変条件）。列数に満たない行の欠けた値は参照でないものとして扱う。
///
/// 参照先の行集合は**シートごとに 1 回だけ** `doc` から作る（要件 9.6）。参照先シートが
/// 文書に無いときは空の集合になり、そこを指すすべての参照が違反になる（要件 9.5）。
/// `doc` は不変で借りるだけであり、参照されている行の削除に介入しない（要件 9.4）。
///
/// 違反は行の並び順 → 列の添字 → 入れ子の位置の順に返す（要件 5.5 の安定併合の基準）。
/// 参照を 1 つも持たない計画では空を返す。
///
/// `selected` は走査の対象の列である（`None` は全列）。指定があるときは**その列だけ**の
/// 形を組み立て、行ごとの費用を対象の列数に閉じる（要件 10.5）。並びは**昇順・重複除去
/// 済み**であること（[`super::validate_columns`] が正規化する）。
pub fn scan<'a, I>(
    doc: &Document,
    schema: &CompiledSchema,
    rows: I,
    selected: Option<&[ColumnIndex]>,
) -> Vec<Violation>
where
    I: IntoIterator<Item = (RowId, &'a [CellValue])>,
{
    // **入口で入力の前提を確かめる。**`selected` は昇順・重複除去済みであること
    // （`validate_columns` が正規化する）。ここで `binary_search` を使うため、未整列の列を
    // 渡すと**黙って取りこぼす** — 本番の経路は常に整列済みだが、crate の外からこの関数を
    // 直接呼ぶ経路（下流の spec）のために、テストの下では落ちるようにしておく。
    debug_assert!(
        selected.map_or(true, |selected| selected.windows(2).all(|pair| match pair {
            [left, right] => left < right,
            _ => true,
        })),
        "selected は昇順・重複除去済みでなければならない（validate_columns が正規化する）"
    );
    // 列ごとの参照の形を 1 回だけ組み立てる（入れ子の内側の参照も含む）。
    let mut sheets: Vec<SheetId> = Vec::new();
    let mut plan: Vec<(ColumnIndex, RefShape<'_>)> = Vec::new();
    for index in 0..schema.column_count() {
        let column = ColumnIndex::new(index);
        // 指定があるときはその列だけを組み立てる（要件 10.5。行ごとの費用を対象の列数に
        // 閉じる）。`selected` は昇順・重複除去済みである（`validate_columns` が正規化する）。
        if let Some(selected) = selected {
            if selected.binary_search(&column).is_err() {
                continue;
            }
        }
        let Some(validator) = schema.validator(column) else {
            // 使用不能な列（未知の `kind`・未登録の拡張型・有限に展開できない再帰型）は
            // 値の種類を問わず `validate::cell` が報告する（要件 11.7）。参照の判定は無い。
            continue;
        };
        if let Some(shape) = shape_of(validator, &mut sheets) {
            plan.push((column, shape));
        }
    }
    if plan.is_empty() {
        return Vec::new();
    }

    // 参照先シートの行識別子の集合（要件 9.6）。削除されたシートは空の集合である
    // （要件 9.5）。`HashSet` は所属判定にだけ使い、反復順を結果へ漏らさない。
    let targets: Vec<HashSet<RowId>> = sheets
        .iter()
        .map(|sheet| match doc.sheet_by_id(*sheet) {
            Some(target) => target.rows().iter().map(Row::id).collect(),
            // 参照先シートが無い（削除された）。そこを指す参照はすべて実在しない。
            None => HashSet::new(),
        })
        .collect();

    let mut found = Vec::new();
    let mut path = Vec::new();
    for (row, values) in rows {
        for (column, shape) in &plan {
            let Some(name) = schema.columns().get(column.index()) else {
                continue;
            };
            let Some(value) = values.get(column.index()) else {
                continue;
            };
            let mut site = |segments: &[ValuePathSegment], slot: usize, value: &CellValue| {
                if !broken(&targets[slot], value) {
                    return;
                }
                found.push(Violation::new(
                    Some(row),
                    *column,
                    name.as_str(),
                    ValuePath::from(segments.to_vec()),
                    ViolationReason::BrokenReference {
                        expected: Expected::RowsOf(sheets[slot]),
                        actual: value.clone(),
                        sheet: sheets[slot],
                    },
                ));
            };
            walk(shape, value, &mut path, &mut site);
            path.clear();
        }
    }
    found
}

/// 参照が実在しない行を指しているか（要件 9.2, 9.3）。
///
/// **参照でない値は `false`** を返す — 値なしの扱いは列の `required` が、テキストでない値は
/// 型の不一致として `validate::cell` が所有する（モジュール docs「判定の規則」）。
fn broken(rows: &HashSet<RowId>, value: &CellValue) -> bool {
    match value {
        CellValue::Null => false,
        // 参照値は行識別子のテキストである。識別子として解釈できないテキストはどの行を
        // も指さないため、実在しない参照である。照合は綴りではなく識別子で行う
        // （`document-format` の解析は大小文字を問わない）。
        CellValue::Text(text) => match RowId::from_str(text) {
            Ok(id) => !rows.contains(&id),
            // 識別子として解釈できないテキストはどの行をも指さない。
            Err(_) => true,
        },
        _ => false,
    }
}

/// 参照を含む位置の形（入れ子の内側も辿る。要件 9.2, 3.4）。
///
/// 検証器の木と同じ形を持ち、値の走査が同じ形を辿る。参照の無い位置は含めない
/// （[`shape_of`] が `None` を返す）。
enum RefShape<'v> {
    /// 参照そのもの。`usize` は参照先シートの集合（[`scan`] の `targets`）の添字。
    Site(usize),
    /// オブジェクトのフィールド（宣言順。値は宣言されたフィールド名で引く）。
    Object(Vec<(&'v str, RefShape<'v>)>),
    /// 配列の要素。
    Array(Box<RefShape<'v>>),
}

/// 検証器の中の参照の形を集める。参照を含まない検証器は `None`（要件 9.2, 3.2）。
///
/// 参照先シートは `sheets` に重複なく積み、`Site` はその添字を運ぶ（同じシートを指す
/// 複数の参照が 1 つの行集合を共有する。要件 9.6）。
fn shape_of<'v>(validator: &'v ColumnValidator, sheets: &mut Vec<SheetId>) -> Option<RefShape<'v>> {
    match validator {
        ColumnValidator::Ref { sheet } => {
            let slot = match sheets.iter().position(|known| known == sheet) {
                Some(slot) => slot,
                None => {
                    sheets.push(*sheet);
                    sheets.len() - 1
                }
            };
            Some(RefShape::Site(slot))
        }
        ColumnValidator::Object { fields } => {
            let inner: Vec<(&str, RefShape<'_>)> = fields
                .iter()
                .filter_map(|field| {
                    shape_of(field.validator(), sheets).map(|shape| (field.name(), shape))
                })
                .collect();
            (!inner.is_empty()).then_some(RefShape::Object(inner))
        }
        ColumnValidator::Array { items, .. } => {
            shape_of(items, sheets).map(|shape| RefShape::Array(Box::new(shape)))
        }
        _ => None,
    }
}

/// 値の形を辿り、参照の位置ごとにその値を渡す（要件 9.2, 3.4）。
///
/// 参照でない値（テキストでない値、値の形が検証器と食い違う位置、宣言されたフィールドが
/// 値に無い位置）は辿らない — それらの違反は `validate::cell` が位置つきで報告する。
fn walk(
    shape: &RefShape<'_>,
    value: &CellValue,
    path: &mut Vec<ValuePathSegment>,
    site: &mut impl FnMut(&[ValuePathSegment], usize, &CellValue),
) {
    match (shape, value) {
        (RefShape::Site(slot), value) => site(path, *slot, value),
        (RefShape::Object(fields), CellValue::Nested(NestedValue::Object(entries))) => {
            for (name, shape) in fields {
                path.push(ValuePathSegment::Field((*name).into()));
                if let Some((_, value)) = entries.iter().find(|(key, _)| key == name) {
                    walk(shape, value, path, site);
                }
                path.pop();
            }
        }
        (RefShape::Array(items), CellValue::Nested(NestedValue::Array(elements))) => {
            for (index, element) in elements.iter().enumerate() {
                path.push(ValuePathSegment::Index(index));
                walk(items, element, path, site);
                path.pop();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile_declaration;
    use crate::compile::plan::ColumnIndex;
    use crate::compile::CompiledSchema;
    use crate::declaration::{ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl};
    use crate::registry::TypeRegistry;
    use crate::types::TypeKind;
    use crate::validate::report::{Expected, ValuePath, ValuePathSegment, ViolationReason};
    use document_format::{CellValue, Document, IdFactory, NestedValue, RowId, SheetId};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// 文字列のセル値。
    fn text(value: &str) -> CellValue {
        CellValue::Text(value.to_owned())
    }

    /// 入れ子のオブジェクトのセル値（キー順をそのまま保つ）。
    fn object(entries: Vec<(&str, CellValue)>) -> CellValue {
        CellValue::Nested(NestedValue::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        ))
    }

    /// 入れ子の配列のセル値。
    fn array(items: Vec<CellValue>) -> CellValue {
        CellValue::Nested(NestedValue::Array(items))
    }

    /// 種別と制約による列を組み立てる。
    fn column(name: &str, kind: TypeKind, constraints: Constraints) -> ColumnDecl {
        ColumnDecl {
            name: name.into(),
            ty: TypeDecl::Kind {
                kind: DeclaredKind::Known(kind),
                constraints,
            },
            required: false,
            unique: false,
            default: None,
            description: None,
        }
    }

    /// 参照先シートを指す `ref` 型の指定。
    fn ref_type(sheet: SheetId) -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(TypeKind::Ref),
            constraints: Constraints {
                sheet: Some(sheet),
                ..Constraints::default()
            },
        }
    }

    /// 参照先シートを指す `ref` 列。
    fn ref_column(name: &str, sheet: SheetId) -> ColumnDecl {
        column(
            name,
            TypeKind::Ref,
            Constraints {
                sheet: Some(sheet),
                ..Constraints::default()
            },
        )
    }

    /// 入れ子のフィールド 1 本分の宣言。
    fn field(name: &str, ty: TypeDecl) -> FieldDecl {
        FieldDecl {
            name: name.into(),
            ty,
            required: false,
            default: None,
            description: None,
        }
    }

    /// 列名を宣言したシートを文書へ足す。
    fn add_sheet(doc: &mut Document, name: &str, columns: &[&str]) -> SheetId {
        let sheet = doc.add_sheet(name);
        let columns = columns.iter().map(|column| (*column).to_owned()).collect();
        doc.set_sheet_columns(sheet, columns)
            .expect("標本のシートが文書にある");
        sheet
    }

    /// シートへ行を足し、発行された識別子を返す。
    fn add_row(doc: &mut Document, sheet: SheetId, values: Vec<CellValue>) -> RowId {
        let row = doc.add_row(sheet).expect("標本のシートが文書にある");
        doc.set_row_values(sheet, row, values)
            .expect("標本の行がシートにある");
        row
    }

    /// シートの行を走査の入力の形（行の並び順）にする。
    fn rows_of(doc: &Document, sheet: SheetId) -> Vec<(RowId, &[CellValue])> {
        doc.sheet_by_id(sheet)
            .expect("標本のシートが文書にある")
            .rows()
            .iter()
            .map(|row| (row.id(), row.values()))
            .collect()
    }

    /// 宣言から計画を組み立てる。
    fn compiled(columns: Vec<ColumnDecl>) -> CompiledSchema {
        compile_declaration(&Schema { columns }, &[], &TypeRegistry::new())
            .expect("標本の宣言はコンパイルできる")
    }

    /// 行の走査を数えるイテレータ（第 2 段の費用を行数で観測する）。
    struct CountedRows<'a> {
        rows: std::vec::IntoIter<(RowId, &'a [CellValue])>,
        consumed: Arc<AtomicUsize>,
    }

    impl<'a> Iterator for CountedRows<'a> {
        type Item = (RowId, &'a [CellValue]);

        fn next(&mut self) -> Option<Self::Item> {
            let item = self.rows.next();
            if item.is_some() {
                self.consumed.fetch_add(1, Ordering::Relaxed);
            }
            item
        }
    }

    /// 対象の列を指定した走査は、指定した列に閉じる（tasks.md 5.4。要件 10.5）。
    ///
    /// 参照を持たない列だけを指定したときは参照の形を 1 つも組み立てないため、**行の走査
    /// そのものを行わない**（[`scan`] は形が空なら行を舐める前に戻る）。全列を走査して
    /// から結果を絞る実装は行を舐めてしまうので、行の消費数がこの違いを直接に観測する。
    #[test]
    fn a_selected_reference_scan_is_closed_to_the_columns_it_did_not_select() {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let source = add_sheet(&mut doc, "発注", &["仕入先", "備考"]);
        let missing = IdFactory::new().new_row_id();
        let schema = compiled(vec![
            ref_column("仕入先", target),
            column("備考", TypeKind::Text, Constraints::default()),
        ]);
        for _ in 0..6 {
            add_row(
                &mut doc,
                source,
                vec![CellValue::Text(missing.to_string()), text("至急")],
            );
        }
        let rows = rows_of(&doc, source);

        // 全列の走査は参照の列を見て、6 行すべてを違反として報告する。
        let consumed = Arc::new(AtomicUsize::new(0));
        let violations = scan(
            &doc,
            &schema,
            CountedRows {
                rows: rows.clone().into_iter(),
                consumed: Arc::clone(&consumed),
            },
            None,
        );
        assert_eq!(6, violations.len(), "参照の違反が出ていない");
        assert_eq!(6, consumed.load(Ordering::Relaxed));

        // 参照を持たない列だけを指定すると、参照の走査は行を舐めない。
        let consumed = Arc::new(AtomicUsize::new(0));
        let violations = scan(
            &doc,
            &schema,
            CountedRows {
                rows: rows.clone().into_iter(),
                consumed: Arc::clone(&consumed),
            },
            Some(&[ColumnIndex::new(1)]),
        );
        assert!(violations.is_empty(), "指定していない列の違反が混ざった");
        assert_eq!(
            0,
            consumed.load(Ordering::Relaxed),
            "指定していない参照の列を走査するために行を舐めている"
        );
    }

    /// 実在する行への参照は違反にならない（要件 9.2）。
    #[test]
    fn a_reference_to_an_existing_row_is_conforming() {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let source = add_sheet(&mut doc, "発注", &["仕入先"]);
        let supplier = add_row(&mut doc, target, vec![text("甲")]);
        add_row(
            &mut doc,
            source,
            vec![CellValue::Text(supplier.to_string())],
        );
        let schema = compiled(vec![ref_column("仕入先", target)]);

        let violations = scan(&doc, &schema, rows_of(&doc, source), None);

        assert!(
            violations.is_empty(),
            "実在する行への参照が違反になった: {violations:?}"
        );
    }

    /// 参照先の行が実在しないとき、参照元の行と列および参照先の識別子を含む違反になる
    /// （要件 9.2, 9.3）。
    #[test]
    fn a_reference_to_a_missing_row_reports_row_column_and_target_identifier() {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let source = add_sheet(&mut doc, "発注", &["仕入先"]);
        add_row(&mut doc, target, vec![text("甲")]);
        // 文書のどの行も持たない識別子（削除された行と同じ状態）。
        let missing = IdFactory::new().new_row_id();
        let first = add_row(&mut doc, source, vec![CellValue::Text(missing.to_string())]);
        let second = add_row(&mut doc, source, vec![CellValue::Text(missing.to_string())]);
        let schema = compiled(vec![ref_column("仕入先", target)]);

        let violations = scan(&doc, &schema, rows_of(&doc, source), None);

        assert_eq!(
            2,
            violations.len(),
            "実在しない参照が 1 行につき 1 件出ていない"
        );
        assert_eq!(Some(first), violations[0].row(), "参照元の行が違反に無い");
        assert_eq!(Some(second), violations[1].row());
        assert_eq!(
            ColumnIndex::new(0),
            violations[0].column(),
            "参照元の列が違反に無い"
        );
        assert_eq!("仕入先", violations[0].column_name());
        assert_eq!(
            &ValuePath::root(),
            violations[0].path(),
            "セル直下の参照に入れ子の位置が付いた"
        );
        assert_eq!(
            ViolationReason::BrokenReference {
                expected: Expected::RowsOf(target),
                actual: CellValue::Text(missing.to_string()),
                sheet: target,
            },
            *violations[0].reason(),
            "参照先のシートと識別子が違反に含まれていない"
        );
    }

    /// 参照値は行識別子として照合する（綴りではない）。同じ識別子を指す小文字の綴りは
    /// 実在する行への参照である（`document-format` の識別子の解析は大小文字を問わない）。
    #[test]
    fn a_reference_is_matched_as_an_identifier_not_as_a_spelling() {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let source = add_sheet(&mut doc, "発注", &["仕入先"]);
        let supplier = add_row(&mut doc, target, vec![text("甲")]);
        add_row(
            &mut doc,
            source,
            vec![text(&supplier.to_string().to_lowercase())],
        );
        let schema = compiled(vec![ref_column("仕入先", target)]);

        let violations = scan(&doc, &schema, rows_of(&doc, source), None);

        assert!(
            violations.is_empty(),
            "同じ行を指す識別子が綴りの違いで実在しないと判定された: {violations:?}"
        );
    }

    /// 参照先のシートが削除されたとき、そのシートを指す**すべての参照**が違反になる
    /// （要件 9.4, 9.5）。削除そのものは妨げられない。
    #[test]
    fn a_reference_to_a_deleted_sheet_is_reported_for_every_reference() {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let source = add_sheet(&mut doc, "発注", &["仕入先", "備考"]);
        let supplier = add_row(&mut doc, target, vec![text("甲")]);
        let schema = compiled(vec![ref_column("仕入先", target)]);
        let first = add_row(
            &mut doc,
            source,
            vec![CellValue::Text(supplier.to_string()), text("至急")],
        );
        assert!(
            scan(&doc, &schema, rows_of(&doc, source), None).is_empty(),
            "削除の前から参照が違反になっている"
        );

        assert!(
            doc.remove_sheet(target).is_some(),
            "参照されていてもシートの削除は通る（要件 9.4）"
        );

        let second = add_row(
            &mut doc,
            source,
            vec![CellValue::Text(supplier.to_string()), text("至急")],
        );
        // 値なしは参照ではない（必須の判定は 5.1 が持つ）。
        let without = add_row(&mut doc, source, vec![CellValue::Null, text("至急")]);

        let violations = scan(&doc, &schema, rows_of(&doc, source), None);

        assert_eq!(
            vec![Some(first), Some(second)],
            violations.iter().map(|v| v.row()).collect::<Vec<_>>(),
            "削除されたシートを指す参照のすべてが違反になっていない"
        );
        assert!(
            violations
                .iter()
                .all(|violation| violation.row() != Some(without)),
            "値なしが参照として報告された"
        );
        for violation in &violations {
            assert_eq!(
                ViolationReason::BrokenReference {
                    expected: Expected::RowsOf(target),
                    actual: CellValue::Text(supplier.to_string()),
                    sheet: target,
                },
                *violation.reason(),
                "削除されたシートを指す違反の理由が違う"
            );
        }
    }

    /// 参照されている行が実在しなくなっても削除は妨げられず、その参照が壊れたものとして
    /// 報告される（tasks.md 5.3。要件 9.3, 9.4）。
    ///
    /// 上流のドキュメントモデルは行の削除の口を公開していない（行の追加・削除・並べ替えは
    /// `document-format` が所有する。design.md「Out of Boundary」）。したがって削除後の状態は
    /// 「参照先シートにその行識別子が無い状態」として組み、削除が通ったこと（参照先シートと
    /// 残りの行がそのまま在ること）と、壊れた参照が報告されることの双方を確かめる。
    /// 走査は [`Document`] を不変で借りるだけであり、削除に介入する経路を持たない。
    #[test]
    fn a_reference_to_a_row_that_is_gone_is_reported_without_touching_the_document() {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let source = add_sheet(&mut doc, "発注", &["仕入先"]);
        let remaining = add_row(&mut doc, target, vec![text("乙")]);
        let deleted = IdFactory::new().new_row_id();
        let row = add_row(&mut doc, source, vec![CellValue::Text(deleted.to_string())]);
        let schema = compiled(vec![ref_column("仕入先", target)]);

        let violations = scan(&doc, &schema, rows_of(&doc, source), None);

        assert_eq!(1, violations.len());
        assert_eq!(Some(row), violations[0].row());
        let sheet = doc.sheet_by_id(target).expect("参照先シートが消えた");
        assert_eq!(
            vec![remaining],
            sheet.rows().iter().map(|row| row.id()).collect::<Vec<_>>(),
            "走査が参照先シートの行を変えた"
        );
        assert_eq!(
            vec![text("乙")],
            sheet.rows()[0].values().to_vec(),
            "走査が参照先シートの値を変えた"
        );
        assert!(
            doc.remove_sheet(target).is_some(),
            "違反を報告したことで削除が妨げられた"
        );
    }

    /// 入れ子の内側の参照も、その位置を特定できる形で報告する（要件 9.2, 9.3）。
    #[test]
    fn references_inside_nested_values_are_judged_at_their_positions() {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let source = add_sheet(&mut doc, "発注", &["明細"]);
        let supplier = add_row(&mut doc, target, vec![text("甲")]);
        let missing = IdFactory::new().new_row_id();
        let line = TypeDecl::Kind {
            kind: DeclaredKind::Known(TypeKind::Object),
            constraints: Constraints {
                fields: vec![
                    field("仕入先", ref_type(target)),
                    field(
                        "履歴",
                        TypeDecl::Kind {
                            kind: DeclaredKind::Known(TypeKind::Array),
                            constraints: Constraints {
                                items: Some(Box::new(ref_type(target))),
                                ..Constraints::default()
                            },
                        },
                    ),
                ],
                ..Constraints::default()
            },
        };
        let schema = compiled(vec![ColumnDecl {
            name: "明細".into(),
            ty: line,
            required: false,
            unique: false,
            default: None,
            description: None,
        }]);
        let row = add_row(
            &mut doc,
            source,
            vec![object(vec![
                ("仕入先", text(&missing.to_string())),
                (
                    "履歴",
                    array(vec![
                        text(&supplier.to_string()),
                        text(&missing.to_string()),
                    ]),
                ),
            ])],
        );

        let violations = scan(&doc, &schema, rows_of(&doc, source), None);

        assert_eq!(2, violations.len(), "入れ子の内側の参照が判定されていない");
        assert_eq!(Some(row), violations[0].row());
        assert_eq!(
            ColumnIndex::new(0),
            violations[0].column(),
            "入れ子の違反の列が違う"
        );
        assert_eq!("明細", violations[0].column_name());
        assert_eq!(
            vec![ValuePathSegment::Field("仕入先".into())],
            violations[0].path().segments().to_vec(),
            "入れ子の位置がフィールド名で示されていない"
        );
        assert_eq!(
            vec![
                ValuePathSegment::Field("履歴".into()),
                ValuePathSegment::Index(1),
            ],
            violations[1].path().segments().to_vec(),
            "配列の要素の位置が添字で示されていない"
        );
    }

    /// 参照でない値は本走査の対象にしない（値なしは [`super::super::cell`] が必須で、
    /// テキストでない値は型の不一致として同走査が報告する）。
    #[test]
    fn values_that_are_not_references_are_left_to_the_cell_scan() {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let source = add_sheet(&mut doc, "発注", &["仕入先"]);
        add_row(&mut doc, target, vec![text("甲")]);
        add_row(&mut doc, source, vec![CellValue::Null]);
        add_row(&mut doc, source, vec![CellValue::Int(7)]);
        let schema = compiled(vec![ref_column("仕入先", target)]);

        let violations = scan(&doc, &schema, rows_of(&doc, source), None);

        assert!(
            violations.is_empty(),
            "参照でない値が参照として報告された: {violations:?}"
        );
    }

    /// 違反は行の並び順 → 列の添字で並ぶ（要件 5.5 の安定併合の基準。同じシートを指す
    /// 複数の列でも参照先の集合は 1 つである）。
    #[test]
    fn references_are_reported_in_row_then_column_order() {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let source = add_sheet(&mut doc, "発注", &["一次", "二次"]);
        let missing = IdFactory::new().new_row_id();
        let schema = compiled(vec![ref_column("一次", target), ref_column("二次", target)]);
        let first = add_row(
            &mut doc,
            source,
            vec![CellValue::Text(missing.to_string()), CellValue::Null],
        );
        let second = add_row(
            &mut doc,
            source,
            vec![
                CellValue::Text(missing.to_string()),
                CellValue::Text(missing.to_string()),
            ],
        );

        let violations = scan(&doc, &schema, rows_of(&doc, source), None);

        assert_eq!(
            vec![(first, 0), (second, 0), (second, 1)],
            violations
                .iter()
                .map(|violation| (
                    violation.row().expect("走査の違反は行を持つ"),
                    violation.column().index()
                ))
                .collect::<Vec<_>>(),
            "違反の並びが行の並び順 → 列の添字になっていない"
        );
    }

    /// 10 万行の参照判定が一括で完了する（tasks.md 5.3。要件 9.6）。
    ///
    /// 参照先シートの行識別子の集合は**1 回だけ**作る。行ごとに参照先へ問い合わせる実装は
    /// 10 万 × 10 万件の照合になり、この上限時間では完了しない（10 万行 × 10 万行の
    /// 線形探索は数分を要する）。上限は 1 セルの判定予算（16 ミリ秒）より桁違いに緩く、
    /// 実測はこの 1/50 未満である。
    ///
    /// 参照元の行は走査の入力（`rows`）として直接与える。上流の文書モデルは行の値を
    /// 1 件ずつしか据えられず（`set_row_values` は行を線形探索するため 10 万行で O(n²)）、
    /// この規模の標本を文書に組む経路が無いためである（`tests/common` の申し送りと同じ理由）。
    #[test]
    fn a_hundred_thousand_references_are_judged_in_one_pass() {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        // 参照先の行は識別子だけを発行する（1 件ずつの `add_row` は O(1)）。
        let targets: Vec<RowId> = (0..100_000)
            .map(|_| doc.add_row(target).expect("標本のシートが文書にある"))
            .collect();
        let mut ids = IdFactory::new();
        let values: Vec<Vec<CellValue>> = targets
            .iter()
            .map(|id| vec![CellValue::Text(id.to_string())])
            .collect();
        let rows: Vec<(RowId, &[CellValue])> = (0..100_000)
            .map(|_| ids.new_row_id())
            .zip(values.iter().map(Vec::as_slice))
            .collect();
        let schema = compiled(vec![ref_column("仕入先", target)]);

        let started = Instant::now();
        let violations = scan(&doc, &schema, rows, None);
        let elapsed = started.elapsed();

        assert!(violations.is_empty(), "実在する行への参照が違反になった");
        assert!(
            elapsed < Duration::from_secs(5),
            "10 万行の参照判定が一括で完了していない: {elapsed:?}"
        );
    }
}

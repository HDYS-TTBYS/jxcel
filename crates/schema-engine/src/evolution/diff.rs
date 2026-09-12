//! 旧宣言と新宣言から変更を抽出する（design.md「File Structure Plan」の
//! `evolution/diff.rs`。tasks.md 7.1。要件 8.1, 8.7）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは [`crate::declaration`] の宣言だけを参照する。同じ層の
//! 影響の集計（タスク 7.2）と計画の適用（タスク 7.3）が本モジュールを参照する側である。
//! 本モジュールは `document-format` の値と識別子、および `std` だけを使う。
//!
//! # 抽出する変更（要件 8.1）
//!
//! 列の追加・削除・改名・型の変更・制約の変更を [`ColumnChange`] の変種として抽出する。
//! 変更は**新宣言の列の並びの順**に並び、名前で一致しなかった旧列の削除がその後に続く。
//! この順序は同じ入力に対して常に同じである（`HashMap` は名前の照合にしか使わず、反復順を
//! 結果に漏らさない）。
//!
//! # 改名の抽出規則（要件 8.7。本モジュールが定める）
//!
//! design.md は「**改名は削除と追加の組ではない**。改名として抽出できたときは既存の値を
//! 運ぶ」（要件 8.7）とだけ定め、抽出の規則は定めていない。本モジュールは次の規則を採る。
//!
//! - 名前が一致する列は**同じ列**である（列名は宣言の中で一意。要件 1.5）。この列には
//!   型の変更と制約の変更だけが起きうる。
//! - 名前で一致しなかった旧列と新列は、**型の同一性が等しいとき**に限り改名として対にする
//!   （[`same_type_identity`]）。対にする相手は、旧宣言の並びで**最初に現れる未使用の列**で
//!   ある。この手順は宣言の並びに対して決定的である。
//! - 型の同一性が等しくない旧列と新列は、改名ではなく**削除と追加**として扱う。
//!
//! 型の同一性を規則に採るのは、**既存の値を運ぶことが意味を持つのは、新しい列がその値を
//! 受け入れうるときだけ**だからである。型が変われば値の意味が変わるため、値を運ばずに
//! 削除と追加として扱う（`int` の列を `text` の列に改名した宣言は、削除して別名で追加した
//! 宣言と区別できない — 区別する材料が宣言の中に無い）。
//!
//! 宣言の**位置**を規則に採らない理由: 列の挿入・削除は以降の列の位置をずらすため、
//! 位置を改名の条件にすると、本物の改名を「削除して別名で追加」と誤って分類する。
//!
//! 型の同一性は、種別（[`DeclaredKind`](crate::declaration::DeclaredKind)）と型定義への参照
//! （[`TypeDefId`](document_format::TypeDefId)）に加えて、**値の解釈を変えるパラメータ**で
//! 決まる（[`same_type_identity`] と [`interpretation_matches`]）。すなわち `ref` の
//! `sheet`、`custom` の `type`、`datetime` の `offset` である。参照先が変われば同じ値が
//! 別のシートの行識別子として解釈され、拡張型の識別子が変われば値の妥当性を決める実装が
//! 変わり、オフセットの扱いが変われば受理される綴りが変わる。**値の解釈を変えない
//! パラメータ**（`min` / `max` / `precision` / `scale` / `minLength` / `maxLength` /
//! `minItems` / `maxItems` / `pattern` / `choices`）は同一性に含めない。これらは同じ解釈の
//! まま値を制約するだけであるから、改名と同時に変えた宣言も改名として抽出され、値を運んだ
//! うえで [`ColumnChange::ConstraintsChanged`] が出る（値を運び、新しい制約に対する違反と
//! して報告できる。要件 8.2, 8.3）。
//!
//! この線引きの**限界**: 入れ子の形（`fields` / `items`）も値の解釈を変えないものとして
//! 同一性から外している。したがって**入れ子の形を変えながら改名した宣言も改名として抽出
//! され、値が運ばれる**。新しい形に外れた値は、適用前の集計（タスク 7.2）が違反として提示
//! する。形そのものを同一性に含めると、入れ子のフィールドを 1 つ足しただけの改名が「削除
//! して別名で追加」になり、既存の値が失われる（要件 8.7 の目的に反する）。
//!
//! # 型の変更と制約の変更の分け方
//!
//! - **型の変更**（[`ColumnChange::TypeChanged`]）: 型の同一性が変わった場合。型そのものが
//!   差し替わるため、**その中の値の制約の差は型の変更に含める**（種別を跨いだ制約の比較は
//!   意味を持たない。`int` の `min` と `text` の `maxLength` は比べられない）。
//! - **制約の変更**（[`ColumnChange::ConstraintsChanged`]）: 列側の制約が変わった場合。
//!   型の同一性が同じときは**値の制約**（[`Constraints`]）と**存在と同一性の制約**
//!   （`required` / `unique` / `default`）のいずれかの差を表す。型の同一性が変わったときは
//!   値の制約の差を型の変更に含めるため、存在と同一性の制約の差だけを表す（型が変わっても
//!   列側の制約は独立に意味を持つため、型の変更と**併出**しうる。要件 4.1, 4.2, 4.5, 4.6,
//!   8.1）。
//! - **説明**（`description`）は型でも制約でもないため、変更として抽出しない（要件 8.1 が
//!   挙げる変更の種類に無い。要件 1.3 は説明を宣言できることを求めるが、それは変更の種類
//!   ではない）。

use std::collections::HashMap;

use document_format::CellValue;

use crate::declaration::{ColumnDecl, Constraints, DeclaredKind, Schema, TypeDecl};
use crate::types::TypeKind;

/// 列 1 本に起きた変更（tasks.md 7.1。要件 8.1）。
///
/// 変更は**新宣言の列の並びの順**に並ぶ。1 本の列に複数の変更が起きたときは、値の行き先を
/// 決める変更（[`ColumnChange::Renamed`]）が型・制約の変更より先に出る。列は 1 本の列に
/// つき複数の変種として現れうる（例: 改名と同時の制約変更）。
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnChange {
    /// 列が追加された。
    ///
    /// 適用時には、既存のすべての行にその列の既定値を、既定値が宣言されていなければ値なしを
    /// 与える（要件 8.8。適用はタスク 7.3）。
    Added {
        /// 追加された列の名前。
        name: Box<str>,
    },
    /// 列が削除された。既存の値は運ばれない（要件 8.3 の「値が失われる」位置になる）。
    Removed {
        /// 削除された列の名前。
        name: Box<str>,
    },
    /// 列が改名された。既存の値を新しい名前のもとに運ぶ対象である（要件 8.7）。
    ///
    /// 削除と追加の組としては表さない（design.md「Evolution Layer / SchemaEvolution」）。
    /// 抽出の規則はモジュール docs を参照。
    Renamed {
        /// 旧宣言での列の名前。
        from: Box<str>,
        /// 新宣言での列の名前。
        to: Box<str>,
    },
    /// 列の型が変わった（名前は同じ）。
    ///
    /// 型の中の値の制約の差は本変種に含める（モジュール docs）。
    TypeChanged {
        /// 列の名前（新旧で同じ）。
        name: Box<str>,
        /// 旧宣言の型。
        from: TypeDecl,
        /// 新宣言の型。
        to: TypeDecl,
    },
    /// 列の制約が変わった。
    ///
    /// 型の同一性が同じときは**値の制約**（[`ColumnConstraints::value_constraints`]）と
    /// **存在と同一性の制約**のいずれかの差を表す。型の変更と**併出**しうる
    /// （[`ColumnChange::TypeChanged`] も参照。モジュール docs）。
    ConstraintsChanged {
        /// 列の名前（新旧で同じ）。
        name: Box<str>,
        /// 旧宣言の制約。
        from: ColumnConstraints,
        /// 新宣言の制約。
        to: ColumnConstraints,
    },
}

/// 列の制約の状態（[`ColumnChange::ConstraintsChanged`] の前後。要件 4.1, 4.2, 4.5, 4.6）。
///
/// **型の同一性を除く**列の制約を 1 つにまとめたものである。値の制約は型の側に、存在と
/// 同一性の制約は列の側にある（design.md「スキーマ宣言の文法」の文法の規則）ため、両方を
/// 束ねないと「制約の変更」を 1 つの比較で表せない。
///
/// 説明（`description`）は制約ではないため含めない。
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnConstraints {
    /// 値なしを許さないか（要件 4.1）。
    pub required: bool,
    /// 一意制約（要件 4.6）。
    pub unique: bool,
    /// 既定値（要件 4.2）。未宣言は `None`。
    pub default: Option<CellValue>,
    /// 型が種別による指定であるときの値の制約（範囲・長さ・書式・桁・選択肢・入れ子の形。
    /// 要件 4.5）。
    ///
    /// 型が識別子による参照（[`TypeDecl::Ref`]）であるときは `None` である — 参照先の
    /// 定義が値の制約を持ち、列の側には無い。
    pub value_constraints: Option<Constraints>,
}

/// 新旧の宣言の差（tasks.md 7.1。要件 8.1, 8.7）。
///
/// [`diff`] だけが作る。変更の並びは [`SchemaDiff::changes`]、既存の値を運ぶ元の列名は
/// [`SchemaDiff::carried_from`] が配る。
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDiff {
    changes: Vec<ColumnChange>,
    /// 新宣言の列名（新宣言の並び順）。
    ///
    /// [`SchemaDiff::carried_from`] の前提（`to` は新宣言に実在する列名であること）を
    /// `debug_assert!` で機械検査するために保持する。変更の抽出そのものには使わない。
    new_columns: Vec<Box<str>>,
}

impl SchemaDiff {
    /// 抽出した変更。新宣言の列の並びの順に並び、その後に旧宣言の並びの順で削除が続く。
    pub fn changes(&self) -> &[ColumnChange] {
        &self.changes
    }

    /// 新しい列 `to` の既存の値を運ぶ元の列名（要件 8.7）。
    ///
    /// `to` は**新宣言に実在する列名**であること（呼び出し元の前提）。この前提と、
    /// 運搬元が新宣言の列ごとに重複しないこと（1 つの旧列が 2 つの新列の運搬元にならない
    /// こと）は `debug_assert!` が表明する。
    ///
    /// - 改名された列 → 旧宣言での名前（[`ColumnChange::Renamed`] の `from`）。
    /// - 追加された列 → `None`（運ぶ既存の値が無い）。
    /// - それ以外（名前が同じ列。型・制約だけが変わった列を含む）→ `to` 自身。
    pub fn carried_from<'a>(&'a self, to: &'a str) -> Option<&'a str> {
        debug_assert!(
            self.new_columns.iter().any(|name| &**name == to),
            "carried_from は新宣言に実在する列名を前提とする: {to:?}"
        );
        let source = self.resolve(to);
        if let Some(source) = source {
            let claimants = self
                .new_columns
                .iter()
                .filter(|name| self.resolve(name) == Some(source))
                .count();
            debug_assert_eq!(
                claimants, 1,
                "carried_from が指す運搬元が重複している: {to:?} -> {source:?}"
            );
        }
        source
    }

    /// 新宣言の列 `to` の運搬元を、前提の検査を伴わずに求める（[`Self::carried_from`] の
    /// 本体。前提の検査からも呼ぶため、ここでは `debug_assert!` を置かない）。
    fn resolve<'a>(&'a self, to: &'a str) -> Option<&'a str> {
        let mut renamed = None;
        for change in &self.changes {
            match change {
                ColumnChange::Added { name } if &**name == to => return None,
                ColumnChange::Renamed { from, to: new } if &**new == to => renamed = Some(&**from),
                _ => {}
            }
        }
        renamed.or(Some(to))
    }
}

/// 旧宣言と新宣言の差を抽出する（tasks.md 7.1。要件 8.1, 8.7）。
///
/// 抽出の規則（改名の判定を含む）はモジュール docs を参照。両方の宣言が正しい宣言である
/// ことを前提とする（列名が空でなく重複しないこと。要件 1.5, 1.6。`declaration::codec` が
/// 解析時に保証する）。
pub fn diff(from: &Schema, to: &Schema) -> SchemaDiff {
    let mut old_used = vec![false; from.columns.len()];
    let mut new_matched = vec![false; to.columns.len()];

    // 名前が一致する列は同じ列である（列名は宣言の中で一意。要件 1.5）。
    let old_by_name: HashMap<&str, usize> = from
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| (&*column.name, index))
        .collect();
    for (index, column) in to.columns.iter().enumerate() {
        if let Some(&old) = old_by_name.get(&*column.name) {
            old_used[old] = true;
            new_matched[index] = true;
        }
    }

    // 名前で一致しなかった列のうち、型の同一性が等しいものを改名として対にする。
    // 相手は旧宣言の並びで最初に現れる未使用の列であり、この手順は決定的である
    // （`HashMap` は名前の照合にしか使わない）。
    let mut renamed_from = vec![None; to.columns.len()];
    for index in 0..to.columns.len() {
        if new_matched[index] {
            continue;
        }
        let candidate = (0..from.columns.len()).find(|&old| {
            !old_used[old] && same_type_identity(&from.columns[old].ty, &to.columns[index].ty)
        });
        if let Some(old) = candidate {
            old_used[old] = true;
            new_matched[index] = true;
            renamed_from[index] = Some(old);
        }
    }

    // 新宣言の列の並びの順に変更を並べる。改名は値の行き先を決める変更であるため、
    // 同じ列の型・制約の変更より先に出す。
    let mut changes = Vec::new();
    for (index, new_column) in to.columns.iter().enumerate() {
        if let Some(&old) = old_by_name.get(&*new_column.name) {
            push_type_or_constraints(&mut changes, &from.columns[old], new_column);
        } else if let Some(old) = renamed_from[index] {
            changes.push(ColumnChange::Renamed {
                from: from.columns[old].name.clone(),
                to: new_column.name.clone(),
            });
            push_type_or_constraints(&mut changes, &from.columns[old], new_column);
        } else {
            changes.push(ColumnChange::Added {
                name: new_column.name.clone(),
            });
        }
    }

    // 名前で一致せず改名の相手にもならなかった旧列は削除である（旧宣言の並びの順）。
    for (old, old_column) in from.columns.iter().enumerate() {
        if !old_used[old] {
            changes.push(ColumnChange::Removed {
                name: old_column.name.clone(),
            });
        }
    }

    SchemaDiff {
        changes,
        new_columns: to
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect(),
    }
}

/// 名前が同じ列について、型の変更または制約の変更を押し込む。
///
/// 型の同一性が変わったときは型の変更を出し、**型の中の値の制約の差は型の変更に含める**
/// （種別を跨いだ制約の比較は意味を持たない）。ただし列側の**存在と同一性の制約**
/// （`required` / `unique` / `default`）は型と独立に意味を持つため、型が変わった場合でも
/// 変わっていれば制約の変更を**併出**する（要件 8.1 は型の変更と制約の変更を別々の変更と
/// して挙げる）。同一性が同じときは制約の全体を比べる。
fn push_type_or_constraints(changes: &mut Vec<ColumnChange>, from: &ColumnDecl, to: &ColumnDecl) {
    let before = column_constraints(from);
    let after = column_constraints(to);
    if same_type_identity(&from.ty, &to.ty) {
        if before != after {
            changes.push(ColumnChange::ConstraintsChanged {
                name: to.name.clone(),
                from: before,
                to: after,
            });
        }
    } else {
        changes.push(ColumnChange::TypeChanged {
            name: to.name.clone(),
            from: from.ty.clone(),
            to: to.ty.clone(),
        });
        // 値の制約（`value_constraints`）の差は型の変更に含めるため、ここでは比較しない。
        // 存在と同一性の制約だけを比べる。
        if existence_constraints_differ(from, to) {
            changes.push(ColumnChange::ConstraintsChanged {
                name: to.name.clone(),
                from: before,
                to: after,
            });
        }
    }
}

/// 列側の**存在と同一性の制約**（`required` / `unique` / `default`）が違うか。
///
/// 型が変わった場合に [`ColumnChange::ConstraintsChanged`] を出すかの判定に使う。値の制約
/// （型の中の制約）は型の変更に含めるため比べない。
fn existence_constraints_differ(left: &ColumnDecl, right: &ColumnDecl) -> bool {
    left.required != right.required || left.unique != right.unique || left.default != right.default
}

/// 2 つの型の**同一性**が等しいか（改名の判定に使う。モジュール docs）。
///
/// 種別（[`DeclaredKind`](crate::declaration::DeclaredKind)）と型定義への参照
/// （[`TypeDefId`](document_format::TypeDefId)）に加えて、**値の解釈を変えるパラメータ**
/// （[`interpretation_matches`]）を比べる。範囲・桁・長さ・書式・選択肢・入れ子の形の
/// ような「同じ解釈のまま値を制約するだけ」のパラメータは含めない。
fn same_type_identity(left: &TypeDecl, right: &TypeDecl) -> bool {
    match (left, right) {
        (
            TypeDecl::Kind {
                kind: left_kind,
                constraints: left_constraints,
            },
            TypeDecl::Kind {
                kind: right_kind,
                constraints: right_constraints,
            },
        ) => {
            left_kind == right_kind
                && interpretation_matches(left_kind, left_constraints, right_constraints)
        }
        (TypeDecl::Ref(left), TypeDecl::Ref(right)) => left == right,
        _ => false,
    }
}

/// 種別のパラメータのうち、**値の解釈を変える**ものが一致するか（モジュール docs）。
///
/// 種別が一致していることは呼び出し元が保証する。次の 3 つだけを比べる。
///
/// - `ref` の `sheet`: 参照先が変われば、同じ値が別のシートの行識別子として解釈される。
/// - `custom` の `type`: 識別子が変われば、値の妥当性を決める実装が変わる。
/// - `datetime` の `offset`: `forbidden` と `required` では受理される綴りが変わる。
///
/// その他のパラメータ（`min` / `max` / `precision` / `scale` / `minLength` /
/// `maxLength` / `minItems` / `maxItems` / `pattern` / `choices` / `fields` / `items`）は
/// 値の解釈を変えず、同じ解釈のまま値を制約するだけである。未知の種別
/// （[`DeclaredKind::Unknown`]）にはどのパラメータが解釈を変えるかを定める規則が無いため、
/// 種別の一致だけを見る（値の解釈を変えると断定できる材料が宣言に無い）。
fn interpretation_matches(kind: &DeclaredKind, left: &Constraints, right: &Constraints) -> bool {
    match kind {
        DeclaredKind::Known(TypeKind::Ref) => left.sheet == right.sheet,
        DeclaredKind::Known(TypeKind::Custom) => left.custom_type == right.custom_type,
        DeclaredKind::Known(TypeKind::DateTime) => left.offset == right.offset,
        _ => true,
    }
}

/// 列の制約の状態を取り出す（[`ColumnConstraints`]）。
fn column_constraints(column: &ColumnDecl) -> ColumnConstraints {
    let value_constraints = match &column.ty {
        TypeDecl::Kind { constraints, .. } => Some(constraints.clone()),
        TypeDecl::Ref(_) => None,
    };
    ColumnConstraints {
        required: column.required,
        unique: column.unique,
        default: column.default.clone(),
        value_constraints,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::DeclaredKind;
    use crate::types::datetime::OffsetPolicy;
    use crate::types::TypeKind;
    use document_format::SheetId;

    fn int_type() -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(TypeKind::Int),
            constraints: Constraints::default(),
        }
    }

    fn text_type() -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(TypeKind::Text),
            constraints: Constraints::default(),
        }
    }

    fn int_column(name: &str) -> ColumnDecl {
        ColumnDecl {
            name: name.into(),
            ty: int_type(),
            required: false,
            unique: false,
            default: None,
            description: None,
        }
    }

    fn text_column(name: &str) -> ColumnDecl {
        ColumnDecl {
            name: name.into(),
            ty: text_type(),
            required: false,
            unique: false,
            default: None,
            description: None,
        }
    }

    fn schema(columns: Vec<ColumnDecl>) -> Schema {
        Schema { columns }
    }

    /// 制約のない `int` 列の制約の状態。
    fn plain_int_constraints() -> ColumnConstraints {
        ColumnConstraints {
            required: false,
            unique: false,
            default: None,
            value_constraints: Some(Constraints::default()),
        }
    }

    /// 種別と値の制約から型を作る。
    fn kind_type(kind: TypeKind, constraints: Constraints) -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(kind),
            constraints,
        }
    }

    /// 型だけを差し替えた列（存在と同一性の制約は既定のまま）。
    fn column_of(name: &str, ty: TypeDecl) -> ColumnDecl {
        ColumnDecl {
            name: name.into(),
            ty,
            required: false,
            unique: false,
            default: None,
            description: None,
        }
    }

    /// 参照先シート `sheet`（ULID の正準テキスト）を指す `ref` 列。
    fn ref_column(name: &str, sheet: &str) -> ColumnDecl {
        let sheet: SheetId = sheet.parse().expect("正準 ULID");
        column_of(
            name,
            kind_type(
                TypeKind::Ref,
                Constraints {
                    sheet: Some(sheet),
                    ..Constraints::default()
                },
            ),
        )
    }

    /// 拡張型 `type_id` を使う `custom` 列。
    fn custom_column(name: &str, type_id: &str) -> ColumnDecl {
        column_of(
            name,
            kind_type(
                TypeKind::Custom,
                Constraints {
                    custom_type: Some(type_id.into()),
                    ..Constraints::default()
                },
            ),
        )
    }

    /// オフセットの扱い `offset` を宣言した `datetime` 列。
    fn datetime_column(name: &str, offset: OffsetPolicy) -> ColumnDecl {
        column_of(
            name,
            kind_type(
                TypeKind::DateTime,
                Constraints {
                    offset: Some(offset),
                    ..Constraints::default()
                },
            ),
        )
    }

    /// 値の範囲の下限だけを宣言した `int` 列。
    fn int_column_with_min(name: &str, min: i64) -> ColumnDecl {
        column_of(
            name,
            kind_type(
                TypeKind::Int,
                Constraints {
                    min: Some(CellValue::Int(min)),
                    ..Constraints::default()
                },
            ),
        )
    }

    /// 同一の宣言からは変更が抽出されない。
    #[test]
    fn identical_declarations_produce_no_changes() {
        let from = schema(vec![text_column("品番"), int_column("数量")]);
        let to = from.clone();
        let result = diff(&from, &to);
        assert!(result.changes().is_empty(), "{:?}", result.changes());
        assert_eq!(Some("品番"), result.carried_from("品番"));
        assert_eq!(Some("数量"), result.carried_from("数量"));
    }

    /// 列の追加が追加として抽出され、運ぶ既存の値を持たない（要件 8.1, 8.8）。
    #[test]
    fn an_added_column_is_extracted_without_a_source_to_carry_from() {
        let from = schema(vec![text_column("品番")]);
        let to = schema(vec![text_column("品番"), int_column("数量")]);
        let result = diff(&from, &to);
        assert_eq!(
            &[ColumnChange::Added {
                name: "数量".into()
            }],
            result.changes()
        );
        assert_eq!(None, result.carried_from("数量"));
    }

    /// 列の削除が削除として抽出される（要件 8.1）。
    #[test]
    fn a_removed_column_is_extracted_as_a_removal() {
        let from = schema(vec![text_column("品番"), int_column("数量")]);
        let to = schema(vec![text_column("品番")]);
        let result = diff(&from, &to);
        assert_eq!(
            &[ColumnChange::Removed {
                name: "数量".into()
            }],
            result.changes()
        );
    }

    /// 改名は削除と追加の組ではなく改名として抽出され、既存の値を運ぶ対象になる
    /// （tasks.md 7.1 の明示要求。要件 8.7）。
    #[test]
    fn a_rename_is_extracted_as_a_rename_not_as_a_removal_and_an_addition() {
        let from = schema(vec![int_column("数量")]);
        let to = schema(vec![int_column("個数")]);
        let result = diff(&from, &to);
        assert_eq!(
            &[ColumnChange::Renamed {
                from: "数量".into(),
                to: "個数".into()
            }],
            result.changes()
        );
        assert!(
            result.changes().iter().all(|change| !matches!(
                change,
                ColumnChange::Added { .. } | ColumnChange::Removed { .. }
            )),
            "改名が削除と追加の組として表れている: {:?}",
            result.changes()
        );
        assert_eq!(Some("数量"), result.carried_from("個数"));
    }

    /// 削除して別名で追加した宣言は改名として抽出されない（tasks.md 7.1 の明示要求）。
    ///
    /// 型が変わると既存の値の意味が変わるため、値を運ばない（モジュール docs の改名の
    /// 抽出規則）。
    #[test]
    fn a_removal_and_an_addition_with_a_different_type_are_not_a_rename() {
        let from = schema(vec![int_column("数量")]);
        let to = schema(vec![text_column("個数")]);
        let result = diff(&from, &to);
        assert_eq!(
            &[
                ColumnChange::Added {
                    name: "個数".into()
                },
                ColumnChange::Removed {
                    name: "数量".into()
                },
            ],
            result.changes()
        );
        assert_eq!(None, result.carried_from("個数"));
    }

    /// 名前が同じ列の型の変更が型の変更として抽出される（要件 8.1）。
    ///
    /// 型そのものが差し替わるため、制約の変更は別に抽出しない（モジュール docs）。
    #[test]
    fn a_type_change_under_the_same_name_is_extracted() {
        let from = schema(vec![int_column("数量")]);
        let to = schema(vec![text_column("数量")]);
        let result = diff(&from, &to);
        assert_eq!(
            &[ColumnChange::TypeChanged {
                name: "数量".into(),
                from: int_type(),
                to: text_type(),
            }],
            result.changes()
        );
        assert_eq!(Some("数量"), result.carried_from("数量"));
    }

    /// 名前が同じ列の制約の変更（値の制約と存在の制約の双方）が制約の変更として
    /// 抽出される（要件 8.1, 4.1, 4.5, 4.6）。
    #[test]
    fn a_constraint_change_under_the_same_name_is_extracted() {
        let from = schema(vec![int_column("数量")]);
        let mut changed = int_column("数量");
        changed.ty = TypeDecl::Kind {
            kind: DeclaredKind::Known(TypeKind::Int),
            constraints: Constraints {
                min: Some(CellValue::Int(0)),
                ..Constraints::default()
            },
        };
        changed.required = true;
        let to = schema(vec![changed]);
        let result = diff(&from, &to);
        assert_eq!(
            &[ColumnChange::ConstraintsChanged {
                name: "数量".into(),
                from: plain_int_constraints(),
                to: ColumnConstraints {
                    required: true,
                    unique: false,
                    default: None,
                    value_constraints: Some(Constraints {
                        min: Some(CellValue::Int(0)),
                        ..Constraints::default()
                    }),
                },
            }],
            result.changes()
        );
    }

    /// 改名と制約の変更が同時のとき、改名が先に出て、値を運びつつ制約の変更が報告される
    /// （要件 8.1, 8.7）。
    #[test]
    fn a_rename_with_a_constraint_change_carries_values_and_reports_the_change() {
        let from = schema(vec![int_column("数量")]);
        let mut changed = int_column("個数");
        changed.unique = true;
        let to = schema(vec![changed]);
        let result = diff(&from, &to);
        assert_eq!(
            &[
                ColumnChange::Renamed {
                    from: "数量".into(),
                    to: "個数".into()
                },
                ColumnChange::ConstraintsChanged {
                    name: "個数".into(),
                    from: plain_int_constraints(),
                    to: ColumnConstraints {
                        required: false,
                        unique: true,
                        default: None,
                        value_constraints: Some(Constraints::default()),
                    },
                },
            ],
            result.changes()
        );
        assert_eq!(Some("数量"), result.carried_from("個数"));
    }

    /// 変更の並びは新宣言の列の並びの順であり、名前で一致しなかった旧列の削除がその後に
    /// 続く（同じ入力に対して常に同じ並びになる）。
    #[test]
    fn changes_follow_the_new_declaration_and_then_the_removals() {
        let from = schema(vec![int_column("A"), text_column("B"), int_column("Z")]);
        let to = schema(vec![int_column("C"), text_column("D")]);
        let result = diff(&from, &to);
        assert_eq!(
            &[
                ColumnChange::Renamed {
                    from: "A".into(),
                    to: "C".into()
                },
                ColumnChange::Renamed {
                    from: "B".into(),
                    to: "D".into()
                },
                ColumnChange::Removed { name: "Z".into() },
            ],
            result.changes()
        );
    }

    /// 参照先のシートが違えば型の同一性は変わらない — 改名ではなく削除と追加である
    /// （要件 8.1, 8.7, 9.1）。
    ///
    /// 参照先が変われば、運んだ値は別のシートの行識別子として解釈されてしまう。正の対照
    /// （参照先が同じ改名）も同じ検査に含める。
    #[test]
    fn a_rename_across_different_referenced_sheets_is_not_a_rename() {
        let sheet_a = "01K4ANRRG004HMASW9NF6YY091";
        let sheet_b = "01K4ANRRG004HMASW9NF6YY092";

        // 正の対照: 参照先が同じなら改名のままである。
        let from = schema(vec![ref_column("仕入先", sheet_a)]);
        let to = schema(vec![ref_column("取引先", sheet_a)]);
        let result = diff(&from, &to);
        assert_eq!(
            &[ColumnChange::Renamed {
                from: "仕入先".into(),
                to: "取引先".into()
            }],
            result.changes()
        );
        assert_eq!(Some("仕入先"), result.carried_from("取引先"));

        // 負の対照: 参照先が違えば値を運ばない。
        let to = schema(vec![ref_column("取引先", sheet_b)]);
        let result = diff(&from, &to);
        assert_eq!(
            &[
                ColumnChange::Added {
                    name: "取引先".into()
                },
                ColumnChange::Removed {
                    name: "仕入先".into()
                },
            ],
            result.changes()
        );
        assert_eq!(None, result.carried_from("取引先"));
    }

    /// 拡張型の識別子が違えば型の同一性は変わらない — 改名ではなく削除と追加である
    /// （要件 8.1, 8.7, 11.1）。
    ///
    /// 識別子が変われば、値の妥当性を決める実装が変わる。
    #[test]
    fn a_rename_across_different_custom_types_is_not_a_rename() {
        // 正の対照: 拡張型が同じなら改名のままである。
        let from = schema(vec![custom_column("郵便番号", "postal-code")]);
        let to = schema(vec![custom_column("〒", "postal-code")]);
        let result = diff(&from, &to);
        assert_eq!(
            &[ColumnChange::Renamed {
                from: "郵便番号".into(),
                to: "〒".into()
            }],
            result.changes()
        );
        assert_eq!(Some("郵便番号"), result.carried_from("〒"));

        // 負の対照: 拡張型が違えば値を運ばない。
        let to = schema(vec![custom_column("〒", "product-code")]);
        let result = diff(&from, &to);
        assert_eq!(
            &[
                ColumnChange::Added { name: "〒".into() },
                ColumnChange::Removed {
                    name: "郵便番号".into()
                },
            ],
            result.changes()
        );
        assert_eq!(None, result.carried_from("〒"));
    }

    /// 日時のオフセットの扱いが違えば型の同一性は変わらない — 改名ではなく削除と追加で
    /// ある（要件 8.1, 8.7, 2.4）。
    ///
    /// `forbidden` と `required` では受理される綴りが変わる。
    #[test]
    fn a_rename_across_different_offset_policies_is_not_a_rename() {
        // 正の対照: オフセットの扱いが同じなら改名のままである。
        let from = schema(vec![datetime_column("納品日", OffsetPolicy::Required)]);
        let to = schema(vec![datetime_column("納入日時", OffsetPolicy::Required)]);
        let result = diff(&from, &to);
        assert_eq!(
            &[ColumnChange::Renamed {
                from: "納品日".into(),
                to: "納入日時".into()
            }],
            result.changes()
        );
        assert_eq!(Some("納品日"), result.carried_from("納入日時"));

        // 負の対照: オフセットの扱いが違えば値を運ばない。
        let to = schema(vec![datetime_column("納入日時", OffsetPolicy::Forbidden)]);
        let result = diff(&from, &to);
        assert_eq!(
            &[
                ColumnChange::Added {
                    name: "納入日時".into()
                },
                ColumnChange::Removed {
                    name: "納品日".into()
                },
            ],
            result.changes()
        );
        assert_eq!(None, result.carried_from("納入日時"));
    }

    /// 型が変わり、かつ列側の制約も変わったときは、型の変更と制約の変更の**両方**が
    /// 抽出される（要件 8.1）。
    #[test]
    fn a_type_change_under_the_same_name_also_reports_the_column_constraint_change() {
        let from = schema(vec![int_column("数量")]);
        let mut changed = text_column("数量");
        changed.required = true;
        let to = schema(vec![changed]);
        let result = diff(&from, &to);
        assert_eq!(
            &[
                ColumnChange::TypeChanged {
                    name: "数量".into(),
                    from: int_type(),
                    to: text_type(),
                },
                ColumnChange::ConstraintsChanged {
                    name: "数量".into(),
                    from: plain_int_constraints(),
                    to: ColumnConstraints {
                        required: true,
                        unique: false,
                        default: None,
                        value_constraints: Some(Constraints::default()),
                    },
                },
            ],
            result.changes()
        );
        assert_eq!(Some("数量"), result.carried_from("数量"));
    }

    /// 値の制約だけが違う改名は改名のままである（要件 8.1, 8.7）。
    ///
    /// 範囲・桁・長さ・書式・選択肢・入れ子の形は値の解釈を変えないため、型の同一性に
    /// 含めない。運んだ値が新しい制約に外れれば、適用前の集計（タスク 7.2）が提示する。
    #[test]
    fn a_rename_that_only_changes_value_constraints_stays_a_rename() {
        let from = schema(vec![int_column_with_min("数量", 0)]);
        let to = schema(vec![int_column_with_min("個数", 10)]);
        let result = diff(&from, &to);
        assert_eq!(
            &[
                ColumnChange::Renamed {
                    from: "数量".into(),
                    to: "個数".into()
                },
                ColumnChange::ConstraintsChanged {
                    name: "個数".into(),
                    from: ColumnConstraints {
                        required: false,
                        unique: false,
                        default: None,
                        value_constraints: Some(Constraints {
                            min: Some(CellValue::Int(0)),
                            ..Constraints::default()
                        }),
                    },
                    to: ColumnConstraints {
                        required: false,
                        unique: false,
                        default: None,
                        value_constraints: Some(Constraints {
                            min: Some(CellValue::Int(10)),
                            ..Constraints::default()
                        }),
                    },
                },
            ],
            result.changes()
        );
        assert_eq!(Some("数量"), result.carried_from("個数"));
    }

    /// 新宣言に実在しない列名を `carried_from` に渡すと、前提の違反として検出される
    /// （要件 8.7）。
    ///
    /// `debug_assert!` は解放ビルドでは消えるため、`debug_assertions` が有効なときだけ検査する。
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "carried_from は新宣言に実在する列名を前提とする")]
    fn carried_from_rejects_a_name_that_is_not_in_the_new_declaration() {
        let from = schema(vec![int_column("数量")]);
        let to = schema(vec![int_column("個数")]);
        let result = diff(&from, &to);
        let _ = result.carried_from("存在しない");
    }
}

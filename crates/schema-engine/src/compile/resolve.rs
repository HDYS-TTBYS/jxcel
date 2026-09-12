//! 型定義参照の解決と、値が有限の大きさで存在しえない循環の検出（design.md
//! 「コンポーネントとファイルの対応」の `SchemaCompiler` の一部。tasks.md 4.3。
//! 要件 3.5, 3.6）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは `compile` 層の内側にあり、`declaration` のデータ
//! 構造（[`Schema`] / [`TypeDecl`] / [`TypeDefinition`]）と鎖の最左 [`crate::error`]
//! だけを参照する。`registry` 以降は参照しない — 拡張型の登録の解決は `SchemaCompiler`
//! （タスク 4.4）の仕事であり、本モジュールは**名前付き型定義の参照だけ**を解決する。
//!
//! # 何を解決するか（要件 3.3, 3.5, 3.6）
//!
//! 宣言の文法は、名前付き型定義を [`TypeDecl::Ref`]（テキストでは `{"$ref": "<識別子>"}`）
//! で参照する。本モジュールは
//!
//! - ルートスキーマの列と、**すべての型定義の本文**を走査し、`$ref` の参照先が実在するか
//!   を検査する（要件 3.5）。実在しない参照は [`SchemaError::DanglingTypeRef`] として、
//!   参照元の宣言上の位置と参照先の識別子を運ぶ。
//! - 定義の間の参照が循環し、その型に適合する値が**有限の大きさで存在しえない**とき、
//!   循環に含まれる型定義の識別子を運ぶ [`SchemaError::ImpossibleCycle`] として拒否する
//!   （要件 3.6）。
//!
//! どちらも**宣言ごと拒否**する（design.md「解決できない型の扱い」の表）。未知の `kind`
//! や未登録の拡張型が「その列だけを使用不能」に落ちるのとは扱いが違う — 壊れた参照と
//! 存在しえない循環は宣言の構造そのものが壊れているのであり、列単位の縮退では直らない。
//!
//! # 値が存在しえない循環の判定（design.md「Compile Layer / SchemaCompiler」）
//!
//! 判定は「必須かつ非配列の参照だけを辺とする」有向グラフの循環検出である。型定義 `A` の
//! 本文から、`required: true` のフィールドと **入れ子のオブジェクト**をだけを通って到達
//! できる `$ref` の先 `B` を辺 `A → B` とする。この辺は「`A` の値は必ず `B` の値を 1 つ
//! 含む」ことを意味する。閉路があれば、どの値も自身より大きい値を要求し続けるため、
//! 有限の大きさの値が存在しない。
//!
//! - **`required: false` のフィールドは辺にならない。** そのフィールドを欠いた値
//!   （`{}`）が常に存在するため、参照は基底に達する。
//! - **配列は辺にならない。** 要素が空の配列が存在するため、`items` の先の循環は常に
//!   基底を持つ。**`minItems` が 1 以上でも同じ扱いにする** — design.md が「必須かつ
//!   非配列の自己参照だけが該当し、`required: false` や配列を経由する再帰は正当な宣言と
//!   して通す」と定めているためである（空配列を許さない配列を経由する循環は、厳密には
//!   有限の値を持たないが、設計の規則をここで独自に強めない）。
//! - **裸の参照**（型定義の本文がそのまま `{"$ref": …}`）は辺になる。`A` の値は `B` の値
//!   そのものであるため基底を持たない。
//! - 未知の `kind`・ANY・拡張型・スカラー種別は辺を持たない（列単位の使用不能や実装の
//!   判断に委ねられ、参照の循環には関与しない。要件 11.7）。
//!
//! 循環の報告順（[`SchemaError::ImpossibleCycle::type_defs`]）は循環の順であり、探索は
//! 識別子の昇順（上流が発行する ULID は時系列順に並ぶ）で行う。同じ入力に対して常に同じ
//! 誤りが返る。
//!
//! # 参照の連鎖（[`Resolver::follow`]）
//!
//! 型定義の本文がさらに別の型定義を参照できる（`A = $ref B`）。[`Resolver::follow`] は
//! この連鎖を辿り、[`TypeDecl::Kind`] に着地させる。**連鎖は有限である** — 裸の参照の
//! 循環は上の規則で値が存在しえない循環として拒否されるためである。`follow` は
//! 防御として定義数を上限に置くが、`resolve` を通った宣言ではその上限に達しない。
//!
//! # 宣言上の位置の規約
//!
//! [`SchemaError::DanglingTypeRef::from`] は、`declaration::codec`（タスク 3.2, 3.3）が
//! 組み立てる位置と同じ文法で書く。ルートスキーマの列は `columns[<添字>]` を起点とし、
//! 型定義の本文は `type <識別子>.definition` を起点とする（識別子を前置するのは、複数の
//! 定義のどれの中かが位置だけでは判別できないため）。型の中は `type` / `fields[<添字>]` /
//! `items` / `$ref` を `.` で辿る。

use std::collections::BTreeMap;

use document_format::TypeDefId;

use crate::declaration::{DeclaredKind, Schema, TypeDecl, TypeDefinition};
use crate::error::SchemaError;
use crate::types::TypeKind;

/// 宣言テキストで型定義への参照を表すキー（`declaration::codec` と同じ）。
const REF_KEY: &str = "$ref";
/// 型のキー。
const TYPE_KEY: &str = "type";
/// オブジェクトのフィールドのキー。
const FIELDS_KEY: &str = "fields";
/// 配列の要素の型のキー。
const ITEMS_KEY: &str = "items";
/// 型定義の本文ペイロードの位置の起点。
const DEFINITION_KEY: &str = "definition";

/// 解決済みの型定義の集合（識別子 → 定義本文）。
///
/// [`resolve`] だけがこれを組み立てる。したがって本型の [`Resolver::follow`] は、
/// 参照が実在し、かつ参照の循環が存在しえないことを**前提にできる**。
#[derive(Debug, Clone, Default)]
pub struct Resolver {
    /// 識別子から定義本文（`definition` ペイロードの中身）への対応。
    definitions: BTreeMap<TypeDefId, TypeDecl>,
}

impl Resolver {
    /// 識別子から定義本文を引く。未登録は `None`。
    ///
    /// 拡張型の識別子（`Constraints::custom_type`）とは別の名前空間である — こちらは
    /// `{"$ref": …}` が指す名前付き型定義の識別子であり、`{"kind":"custom","type":…}` の
    /// 識別子ではない。
    pub fn get(&self, id: TypeDefId) -> Option<&TypeDecl> {
        self.definitions.get(&id)
    }

    /// `$ref` の連鎖を辿り、種別による指定（[`TypeDecl::Kind`]）に着地させる。
    ///
    /// 参照の連鎖は [`resolve`] が循環を拒否済みであるため有限である（モジュール docs
    /// 「参照の連鎖」）。未登録の識別子に当たった場合は `None` を返す — 防御であり、
    /// `resolve` を通った宣言では起こらない。
    pub fn follow<'t>(&'t self, ty: &'t TypeDecl) -> Option<&'t TypeDecl> {
        let mut current = ty;
        // 連鎖は `resolve` が循環を拒否済みなので有限である。上限は防御であり、
        // `resolve` を通った宣言では達しない。
        for _ in 0..=self.definitions.len() {
            match current {
                TypeDecl::Kind { .. } => return Some(current),
                TypeDecl::Ref(id) => current = self.definitions.get(id)?,
            }
        }
        None
    }
}

/// ルートスキーマと型定義の集合の参照を解決する（要件 3.5, 3.6）。
///
/// 実在しない参照を [`SchemaError::DanglingTypeRef`]、値が有限の大きさで存在しえない循環を
/// [`SchemaError::ImpossibleCycle`] として拒否し、通った場合にのみ [`Resolver`] を返す。
pub fn resolve(schema: &Schema, definitions: &[TypeDefinition]) -> Result<Resolver, SchemaError> {
    let resolver = Resolver {
        definitions: collect(definitions),
    };
    check_refs(schema, &resolver)?;
    check_cycles(&resolver)?;
    Ok(resolver)
}

/// 定義の集合を識別子で引ける形に写す。
///
/// 識別子の一意性は上流 `document-format` が検査する（design.md「Out of Boundary」）。
/// 本モジュールは重複した識別子のうち**最初の出現**を採り、以降を無視する — 解決の結果を
/// 入力の並びに対して一意に保つためである。
fn collect(definitions: &[TypeDefinition]) -> BTreeMap<TypeDefId, TypeDecl> {
    let mut definitions_by_id = BTreeMap::new();
    for definition in definitions {
        definitions_by_id
            .entry(definition.id)
            .or_insert_with(|| definition.definition.clone());
    }
    definitions_by_id
}

/// ルートスキーマと**すべての型定義の本文**の `$ref` が実在するかを検査する（要件 3.5）。
///
/// 走査順は列の宣言順、続いて型定義の識別子の昇順である。参照されていない型定義も走査
/// する — 壊れた参照を含む宣言は、使われていなくても宣言として壊れている。
fn check_refs(schema: &Schema, resolver: &Resolver) -> Result<(), SchemaError> {
    for (index, column) in schema.columns.iter().enumerate() {
        let position = child(&indexed("columns", index), TYPE_KEY);
        check_type_refs(&column.ty, &position, resolver)?;
    }
    for (id, body) in &resolver.definitions {
        let position = child(&format!("type {id}"), DEFINITION_KEY);
        check_type_refs(body, &position, resolver)?;
    }
    Ok(())
}

/// 型 1 つの内側の `$ref` を検査する。入れ子のオブジェクトと配列の要素型を降りる。
fn check_type_refs(ty: &TypeDecl, position: &str, resolver: &Resolver) -> Result<(), SchemaError> {
    match ty {
        TypeDecl::Ref(id) => {
            if resolver.get(*id).is_none() {
                return Err(SchemaError::DanglingTypeRef {
                    from: child(position, REF_KEY),
                    to: id.to_string(),
                });
            }
            Ok(())
        }
        TypeDecl::Kind { kind, constraints } => {
            match kind {
                DeclaredKind::Known(TypeKind::Object) => {
                    for (index, field) in constraints.fields.iter().enumerate() {
                        let position =
                            child(&indexed(&child(position, FIELDS_KEY), index), TYPE_KEY);
                        check_type_refs(&field.ty, &position, resolver)?;
                    }
                }
                DeclaredKind::Known(TypeKind::Array) => {
                    if let Some(items) = &constraints.items {
                        check_type_refs(items, &child(position, ITEMS_KEY), resolver)?;
                    }
                }
                // 未知の種別と、内側に型を持たない種別は降りる先がない。
                _ => {}
            }
            Ok(())
        }
    }
}

/// 探索中の型定義の色（白 = 未訪問、灰 = 探索路上、黒 = 探索済み）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    White,
    Gray,
    Black,
}

/// 探索の 1 段。`edges` の `next` 番目までを処理したことを覚える。
struct Frame {
    id: TypeDefId,
    edges: Vec<TypeDefId>,
    next: usize,
}

/// 必須かつ非配列の参照だけを辺とする有向グラフの循環を検出する（要件 3.6）。
///
/// 定義の識別子の昇順に始点を試し、深さ優先で探索する。循環を見つけた時点で、探索路の
/// うち循環に含まれる部分（先頭 = 循環の入口）を識別子の並びとして返す。
fn check_cycles(resolver: &Resolver) -> Result<(), SchemaError> {
    let mut color: BTreeMap<TypeDefId, Color> = resolver
        .definitions
        .keys()
        .map(|id| (*id, Color::White))
        .collect();
    let mut path: Vec<TypeDefId> = Vec::new();
    for start in resolver.definitions.keys() {
        if color[start] != Color::White {
            continue;
        }
        if let Some(cycle) = search(*start, resolver, &mut color, &mut path) {
            return Err(SchemaError::ImpossibleCycle {
                type_defs: cycle.iter().map(|id| id.to_string()).collect(),
            });
        }
    }
    Ok(())
}

/// 1 つの始点から深さ優先で探索し、循環を見つけたら循環の並びを返す。
///
/// 再帰ではなく明示的な [`Frame`] の積みを使う — 型定義の連鎖の長さは文書が決めるため、
/// 再帰では入力次第で計算機のスタックを食い潰す。
fn search(
    start: TypeDefId,
    resolver: &Resolver,
    color: &mut BTreeMap<TypeDefId, Color>,
    path: &mut Vec<TypeDefId>,
) -> Option<Vec<TypeDefId>> {
    color.insert(start, Color::Gray);
    path.push(start);
    let mut stack = vec![Frame {
        id: start,
        edges: required_refs(start, resolver),
        next: 0,
    }];
    while !stack.is_empty() {
        let top = stack.len() - 1;
        if stack[top].next < stack[top].edges.len() {
            let next = stack[top].edges[stack[top].next];
            stack[top].next += 1;
            match color.get(&next).copied().unwrap_or(Color::Black) {
                Color::White => {
                    color.insert(next, Color::Gray);
                    path.push(next);
                    stack.push(Frame {
                        id: next,
                        edges: required_refs(next, resolver),
                        next: 0,
                    });
                }
                Color::Gray => {
                    let entry = path
                        .iter()
                        .position(|id| *id == next)
                        .expect("灰色の定義は探索路上にある");
                    return Some(path[entry..].to_vec());
                }
                Color::Black => {}
            }
        } else {
            let done = stack.pop().expect("空でないことを確認済み");
            color.insert(done.id, Color::Black);
            path.pop();
        }
    }
    None
}

/// 型定義 1 件の本文から出る、必須かつ非配列の参照の辺。
///
/// 同じ識別子への辺は 1 つに畳む（畳まないと、同じ辺の 2 つ目を「探索路上」と誤認する）。
fn required_refs(id: TypeDefId, resolver: &Resolver) -> Vec<TypeDefId> {
    let Some(body) = resolver.get(id) else {
        // 実在しない参照は `check_refs` が先に拒否する。ここは防御であり辺を持たない。
        return Vec::new();
    };
    let mut edges = Vec::new();
    collect_required_refs(body, &mut edges);
    edges
}

/// 必須のフィールドと入れ子のオブジェクトだけを降り、`$ref` の先を辺として集める。
///
/// `required: false` のフィールドと配列は降りない — どちらも値を欠いた基底（`{}` と
/// `[]`）が存在するため、参照の循環をそこで断ち切る（モジュール docs「値が存在しえない
/// 循環の判定」）。
fn collect_required_refs(ty: &TypeDecl, edges: &mut Vec<TypeDefId>) {
    match ty {
        TypeDecl::Ref(id) => {
            if !edges.contains(id) {
                edges.push(*id);
            }
        }
        TypeDecl::Kind { kind, constraints } => {
            if let DeclaredKind::Known(TypeKind::Object) = kind {
                for field in constraints.fields.iter().filter(|field| field.required) {
                    collect_required_refs(&field.ty, edges);
                }
            }
        }
    }
}

/// 宣言上の位置にキーを 1 段足す（`declaration::codec` と同じ文法）。
fn child(position: &str, key: &str) -> String {
    format!("{position}.{key}")
}

/// 宣言上の位置に添字を 1 段足す。
fn indexed(position: &str, index: usize) -> String {
    format!("{position}[{index}]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::{ColumnDecl, Constraints, FieldDecl};
    use document_format::IdFactory;

    /// 識別子を発行する。ULID は発行順に昇順なので、探索順の検証にも使える。
    fn ids(count: usize) -> Vec<TypeDefId> {
        let mut factory = IdFactory::default();
        (0..count).map(|_| factory.new_type_def_id()).collect()
    }

    fn kind(kind: TypeKind) -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(kind),
            constraints: Constraints::default(),
        }
    }

    fn object(fields: Vec<FieldDecl>) -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(TypeKind::Object),
            constraints: Constraints {
                fields,
                ..Constraints::default()
            },
        }
    }

    fn array(items: TypeDecl) -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(TypeKind::Array),
            constraints: Constraints {
                items: Some(Box::new(items)),
                ..Constraints::default()
            },
        }
    }

    fn field(name: &str, ty: TypeDecl, required: bool) -> FieldDecl {
        FieldDecl {
            name: name.into(),
            ty,
            required,
            default: None,
            description: None,
        }
    }

    fn definition(id: TypeDefId, body: TypeDecl) -> TypeDefinition {
        TypeDefinition {
            id,
            definition: body,
        }
    }

    fn schema_with_column(ty: TypeDecl) -> Schema {
        Schema {
            columns: vec![ColumnDecl {
                name: "列".into(),
                ty,
                required: false,
                unique: false,
                default: None,
                description: None,
            }],
        }
    }

    /// 参照先が実在しない `$ref` は、参照元の位置と参照先の識別子つきで拒否される
    /// （要件 3.5）。
    #[test]
    fn dangling_ref_in_root_column_is_rejected_with_position_and_target() {
        let missing = ids(1)[0];
        let schema = schema_with_column(TypeDecl::Ref(missing));
        let error = resolve(&schema, &[]).expect_err("実在しない参照は拒否される");
        match error {
            SchemaError::DanglingTypeRef { from, to } => {
                assert_eq!(from, "columns[0].type.$ref");
                assert_eq!(to, missing.to_string());
            }
            other => panic!("DanglingTypeRef でない: {other:?}"),
        }
    }

    /// 入れ子のフィールドの `$ref` も同じ規則で拒否され、位置は入れ子の内側を指す。
    #[test]
    fn dangling_ref_in_nested_field_is_rejected() {
        let missing = ids(1)[0];
        let schema = schema_with_column(object(vec![field("内側", TypeDecl::Ref(missing), true)]));
        let error = resolve(&schema, &[]).expect_err("入れ子の参照も検査される");
        match error {
            SchemaError::DanglingTypeRef { from, to } => {
                assert_eq!(from, "columns[0].type.fields[0].type.$ref");
                assert_eq!(to, missing.to_string());
            }
            other => panic!("DanglingTypeRef でない: {other:?}"),
        }
    }

    /// 配列の要素型の `$ref` も検査される。
    #[test]
    fn dangling_ref_in_array_items_is_rejected() {
        let missing = ids(1)[0];
        let schema = schema_with_column(array(TypeDecl::Ref(missing)));
        let error = resolve(&schema, &[]).expect_err("配列の要素型も検査される");
        match error {
            SchemaError::DanglingTypeRef { from, to } => {
                assert_eq!(from, "columns[0].type.items.$ref");
                assert_eq!(to, missing.to_string());
            }
            other => panic!("DanglingTypeRef でない: {other:?}"),
        }
    }

    /// 型定義の本文（入れ子のフィールドを含む）の `$ref` も検査され、位置は定義の
    /// 識別子を前置して報告される。
    #[test]
    fn dangling_ref_in_type_definition_is_rejected() {
        let id = ids(2);
        let (owner, missing) = (id[0], id[1]);
        let definitions = vec![definition(
            owner,
            object(vec![field("内側", TypeDecl::Ref(missing), false)]),
        )];
        let error = resolve(&Schema::default(), &definitions).expect_err("定義の参照も検査される");
        match error {
            SchemaError::DanglingTypeRef { from, to } => {
                assert_eq!(from, format!("type {owner}.definition.fields[0].type.$ref"));
                assert_eq!(to, missing.to_string());
            }
            other => panic!("DanglingTypeRef でない: {other:?}"),
        }
    }

    /// 必須のフィールドを経由した自己参照は、有限の大きさの値を持たないため拒否される
    /// （要件 3.6）。
    #[test]
    fn required_self_reference_is_rejected_as_impossible_cycle() {
        let id = ids(1)[0];
        let definitions = vec![definition(
            id,
            object(vec![field("自分", TypeDecl::Ref(id), true)]),
        )];
        let schema = schema_with_column(TypeDecl::Ref(id));
        let error = resolve(&schema, &definitions).expect_err("自己参照の循環は拒否される");
        match error {
            SchemaError::ImpossibleCycle { type_defs } => {
                assert_eq!(type_defs, vec![id.to_string()])
            }
            other => panic!("ImpossibleCycle でない: {other:?}"),
        }
    }

    /// 2 つの定義が必須のフィールドで互いを要求する循環も拒否され、循環の順に識別子が並ぶ。
    #[test]
    fn mutual_required_reference_is_rejected_with_the_cycle_in_order() {
        let id = ids(2);
        let (a, b) = (id[0], id[1]);
        let definitions = vec![
            definition(a, object(vec![field("b", TypeDecl::Ref(b), true)])),
            definition(b, object(vec![field("a", TypeDecl::Ref(a), true)])),
        ];
        let error = resolve(&schema_with_column(TypeDecl::Ref(a)), &definitions)
            .expect_err("相互参照の循環は拒否される");
        match error {
            SchemaError::ImpossibleCycle { type_defs } => {
                assert_eq!(type_defs, vec![a.to_string(), b.to_string()]);
            }
            other => panic!("ImpossibleCycle でない: {other:?}"),
        }
    }

    /// 種別を持たない裸の参照の循環も、値が存在しえないため拒否される。
    #[test]
    fn pure_ref_cycle_is_rejected() {
        let id = ids(2);
        let (a, b) = (id[0], id[1]);
        let definitions = vec![
            definition(a, TypeDecl::Ref(b)),
            definition(b, TypeDecl::Ref(a)),
        ];
        let error = resolve(&schema_with_column(TypeDecl::Ref(a)), &definitions)
            .expect_err("裸の参照の循環は拒否される");
        assert!(
            matches!(error, SchemaError::ImpossibleCycle { .. }),
            "ImpossibleCycle でない: {error:?}"
        );
    }

    /// 入れ子のオブジェクトを経由した必須の循環も検出される。
    #[test]
    fn cycle_through_nested_required_object_is_rejected() {
        let id = ids(1)[0];
        let inner = object(vec![field("自分", TypeDecl::Ref(id), true)]);
        let definitions = vec![definition(id, object(vec![field("入れ子", inner, true)]))];
        let error = resolve(&schema_with_column(TypeDecl::Ref(id)), &definitions)
            .expect_err("入れ子を経由した循環は拒否される");
        match error {
            SchemaError::ImpossibleCycle { type_defs } => {
                assert_eq!(type_defs, vec![id.to_string()])
            }
            other => panic!("ImpossibleCycle でない: {other:?}"),
        }
    }

    /// 必須でないフィールドの自己参照は基底を持つ（`{}` が適合する）ため、正当な宣言として
    /// 通す（要件 3.6。design.md「Compile Layer / SchemaCompiler」）。
    #[test]
    fn optional_self_reference_is_accepted() {
        let id = ids(1)[0];
        let definitions = vec![definition(
            id,
            object(vec![field("自分", TypeDecl::Ref(id), false)]),
        )];
        resolve(&schema_with_column(TypeDecl::Ref(id)), &definitions)
            .expect("必須でない自己参照は通る");
    }

    /// 必須でないフィールドを経由する相互再帰も通る。
    #[test]
    fn optional_mutual_recursion_is_accepted() {
        let id = ids(2);
        let (a, b) = (id[0], id[1]);
        let definitions = vec![
            definition(a, object(vec![field("b", TypeDecl::Ref(b), true)])),
            definition(b, object(vec![field("a", TypeDecl::Ref(a), false)])),
        ];
        resolve(&schema_with_column(TypeDecl::Ref(a)), &definitions)
            .expect("必須でない参照を経由する再帰は通る");
    }

    /// 配列を経由する再帰は、要素が空の配列という基底を持つため通る（要件 3.6）。
    #[test]
    fn array_of_self_is_accepted() {
        let id = ids(1)[0];
        let definitions = vec![definition(id, array(TypeDecl::Ref(id)))];
        resolve(&schema_with_column(TypeDecl::Ref(id)), &definitions)
            .expect("配列を経由する再帰は通る");
    }

    /// 未知の種別は辺を持たない（その列は使用不能に落ちる。要件 11.7）ため、参照の循環には
    /// 関与しない。
    #[test]
    fn unknown_kind_does_not_form_a_cycle() {
        let id = ids(2);
        let (a, b) = (id[0], id[1]);
        let definitions = vec![
            definition(a, object(vec![field("b", TypeDecl::Ref(b), true)])),
            definition(
                b,
                TypeDecl::Kind {
                    kind: DeclaredKind::Unknown("future".into()),
                    constraints: Constraints::default(),
                },
            ),
        ];
        resolve(&schema_with_column(TypeDecl::Ref(a)), &definitions)
            .expect("未知の種別は循環を作らない");
    }

    /// 同じ識別子への必須の辺が複数あっても、循環と誤認しない（辺の重複を畳んでいる）。
    #[test]
    fn duplicate_required_edges_do_not_create_a_false_cycle() {
        let id = ids(2);
        let (a, b) = (id[0], id[1]);
        let definitions = vec![
            definition(
                a,
                object(vec![
                    field("x", TypeDecl::Ref(b), true),
                    field("y", TypeDecl::Ref(b), true),
                ]),
            ),
            definition(b, kind(TypeKind::Text)),
        ];
        resolve(&schema_with_column(TypeDecl::Ref(a)), &definitions)
            .expect("同じ定義への複数の辺は循環ではない");
    }

    /// どの列からも参照されていない型定義の循環も拒否する — 宣言として壊れているためである。
    #[test]
    fn unused_type_definition_cycle_is_still_rejected() {
        let id = ids(1)[0];
        let definitions = vec![definition(id, TypeDecl::Ref(id))];
        let error = resolve(&Schema::default(), &definitions)
            .expect_err("使われていない定義の循環も拒否される");
        match error {
            SchemaError::ImpossibleCycle { type_defs } => {
                assert_eq!(type_defs, vec![id.to_string()])
            }
            other => panic!("ImpossibleCycle でない: {other:?}"),
        }
    }

    /// 参照の連鎖は種別による指定まで辿られる。
    #[test]
    fn ref_chain_resolves_to_the_definition_head() {
        let id = ids(3);
        let (a, b, c) = (id[0], id[1], id[2]);
        let head = kind(TypeKind::Text);
        let definitions = vec![
            definition(a, TypeDecl::Ref(b)),
            definition(b, TypeDecl::Ref(c)),
            definition(c, head.clone()),
        ];
        let resolver =
            resolve(&schema_with_column(TypeDecl::Ref(a)), &definitions).expect("連鎖は解決できる");
        assert_eq!(resolver.get(c), Some(&head));
        assert_eq!(resolver.follow(&TypeDecl::Ref(a)), Some(&head));
        assert_eq!(resolver.follow(&head), Some(&head));
    }

    /// `declaration::codec` が解析した宣言でも、同じ位置の規約で参照元が報告される。
    #[test]
    fn dangling_ref_from_parsed_declaration_reports_the_declaration_position() {
        let schema = crate::declaration::codec::parse_schema(
            r#"{"columns":[{"name":"属性","type":{"$ref":"01K4ANRRG004HMASW9NF6YY092"}}]}"#,
        )
        .expect("宣言は解析できる");
        let error = resolve(&schema, &[]).expect_err("実在しない参照は拒否される");
        match error {
            SchemaError::DanglingTypeRef { from, to } => {
                assert_eq!(from, "columns[0].type.$ref");
                assert_eq!(to, "01K4ANRRG004HMASW9NF6YY092");
            }
            other => panic!("DanglingTypeRef でない: {other:?}"),
        }
    }
}

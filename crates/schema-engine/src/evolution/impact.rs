//! 変更の影響を集計する（design.md「File Structure Plan」の `evolution/impact.rs`。
//! tasks.md 7.2。要件 8.2, 8.3, 8.4）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは `coerce` / `validate` 層までを参照する。同じ層の
//! [`diff`](super::diff) が旧宣言と新宣言の差を出し、本モジュールがその差を**適用した場合に
//! 何が起きるか**へ写す。計画の適用（`evolution/apply.rs`。タスク 7.3）が本モジュールの
//! 型を参照する側である。
//!
//! # 何を返すか（design.md「Evolution Layer / SchemaEvolution」。要件 8.2, 8.3）
//!
//! [`ChangeImpact`] は 3 つの行数と、値が失われる位置を返す。数え方の定義は本モジュールが
//! 定める（design.md は欄の名前しか定めていない）:
//!
//! - **変換される行数**（[`ChangeImpact::converted_rows`]）: 計画の値の計算で**型強制が
//!   起きた**値を持つ行の数（[`Coercion::Converted`]。要件 7.1〜7.6 の「変換」そのものである）。
//!   列の追加で既定値が入ることや、値がそのまま運ばれることは変換ではない。
//! - **違反になる行数**（[`ChangeImpact::violating_rows`]）: 計画の値に対して全件検証を
//!   行ったとき違反を持つ行の数（要件 5.4 と同じ「違反を持つ行」である）。第 1 段の
//!   値ごとの判定と第 2 段の行を跨ぐ判定（一意性と参照の実在）の**両方**を数える。
//!   数え方は [`SheetReport::invalid_rows`](crate::validate::report::SheetReport::invalid_rows)
//!   と同じである — 1 件の違反が複数の行を挙げることがある
//!   ([`ViolationReason::Duplicate`](crate::validate::report::ViolationReason::Duplicate)
//!   は最初に現れた行に属し、理由が重複するすべての行を運ぶ) が、数えるのは**違反が属する
//!   行**だけである。この一致により、集計の違反行数は、適用した状態の全件検証（5.4）の
//!   違反行数と同じ数え方になる。
//! - **値が失われる行数**（[`ChangeImpact::losing_rows`]）: 削除された列に値を持つ行の数。
//!   値が失われる位置は [`ValueLoss`] の並び（[`ChangeImpact::losses`]）で返す（要件 8.3）。
//!   改名は値を運ぶため失われない（要件 8.7）。**値なしは失う値ではない** — 値なしは
//!   そもそも値が無いため、削除された列が値なしである行は数えない（要件 8.3 の「値が
//!   失われる」の素直な読み）。失われる位置は 行の並び順 → 削除された列の宣言の並び順 で
//!   並ぶ（同じ入力に対して常に同じ並びになる）。
//!
//! # 集計はドキュメントを変更しない（要件 8.4）
//!
//! [`plan_change`] は `&Document` だけを取り、変更の経路を持たない。集計は新しい列名の
//! 配列と**全行分の新しい値**を計算しきって [`ChangePlan`] に持たせるため、適用（タスク 7.3）
//! は計算済みの値を書き戻すだけになる。可謬な処理を計画の段に寄せることが、要件 8.6
//! （途中で失敗して半端に適用された状態を残さない）の土台である。
//!
//! # 行ごとの写像・強制・判定もこの段で済ませる
//!
//! 計画は行ごとに「新しい列名 → 運搬元の列名」（[`SchemaDiff::carried_from`]）を引き、
//! 新しい列の型で値を強制し、違反（値ごとの判定と行を跨ぐ判定の両方）を判定する。これは
//! **一括の処理**であり、行ごとに境界を越える呼び出しを公開しない（要件 10.4）。適用
//! （タスク 7.3）は計算済みの値を書き戻すだけになる。
//!
//! # 宣言と型定義（本モジュールの限界。タスク 7.1 の申し送り）
//!
//! [`Schema`] は名前付き型定義の集合を持たない（design.md「Out of Boundary」。集合は上流の
//! エンベロープが所有する）ため、新しい宣言の `$ref` は**シートが今持っている型定義**で
//! 解決する。型定義の本文を同時に変える変更はこの経路では扱えない（扱うには型定義集合を
//! 引数に足す設計変更が要る）。
//!
//! # ダイジェスト（計画の陳腐化。タスク 7.2）
//!
//! [`ChangePlan`] は集計の時点のシートの**行識別子の並びと列名**を [`PlanDigest`] として
//! 持つ。適用（タスク 7.3）が適用の前に現在のシートと照合し、食い違えば何も変更しない。
//! ダイジェストが刻むのはこの 2 つだけである — **値そのものは刻まない**（design.md
//! 「Evolution Layer / SchemaEvolution」の `ChangePlan` の記述どおり）。対象シートの
//! 識別子も併せて刻む（ダイジェストの入力が同じシートの状態であることを明確にするため。
//! 行識別子は固定長、列名は長さ接頭辞つきで並べるので、並びの区切りは曖昧にならない）。
//! 行の追加・削除・並べ替えと列の構成の変化は検出できるが、計画の後に値だけが書き換わった
//! 場合は検出しない（適用は計画の値を書き戻すため、その書き換えは適用で上書きされる）。
//!
//! [`Coercion::Converted`]: crate::coerce::Coercion::Converted
//! [`SchemaDiff::carried_from`]: super::diff::SchemaDiff::carried_from

use std::collections::{HashMap, HashSet};

use document_format::{Blake3Digest, CellValue, Document, Row, RowId, Sheet, SheetId};

use crate::coerce::{coerce, Coercion};
use crate::compile::plan::ColumnIndex;
use crate::compile::{compile_declaration, CompiledSchema};
use crate::declaration::codec::{parse_schema, parse_type_definition};
use crate::declaration::{Schema, TypeDefinition};
use crate::error::SchemaError;
use crate::registry::TypeRegistry;
use crate::validate::report::{ValidationOptions, ViolationReport};
use crate::validate::{cell, refs, unique};

use super::diff::{diff, ColumnChange, SchemaDiff};

/// 行識別子のテキスト形の長さ（`ulid::ULID_LEN`。正準 Crockford base32）。
///
/// `RowId::as_str` / `SheetId::as_str` はこの長さの配列を要求する。本モジュールは
/// ダイジェストのバイト列へ識別子を書き込むために確保なしのテキスト形を使う。
const ROW_ID_TEXT_LEN: usize = 26;

/// 適用した場合の影響（design.md「Evolution Layer / SchemaEvolution」の `ChangeImpact`。
/// 要件 8.2, 8.3）。
///
/// 集計の定義はモジュール docs を参照。数は**行**を数える（同じ行に 2 件の変換があっても
/// 1 である）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeImpact {
    /// 型強制が起きた値を 1 つ以上持つ行の数（要件 8.2）。
    pub converted_rows: usize,
    /// 適用後の状態で違反を 1 つ以上持つ行の数（要件 8.2）。
    pub violating_rows: usize,
    /// 削除される列に値を持つ行の数（要件 8.2）。
    pub losing_rows: usize,
    /// 値が失われる位置（要件 8.3）。行の並び順 → 削除された列の並び順。
    pub losses: Vec<ValueLoss>,
}

/// 値が失われる位置 1 件（design.md「Evolution Layer / SchemaEvolution」の `ValueLoss`。
/// 要件 8.3）。
///
/// 削除される列の、値を持つ行を指す。列は**旧宣言での名前**である（新しい宣言にその列は
/// 無いため、識別には旧宣言の名前を使うほかない）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueLoss {
    /// 値を持つ行の識別子。
    pub row: RowId,
    /// 削除される列の名前（旧宣言での名前）。
    pub column: Box<str>,
}

/// 計画の陳腐化を判定するための、集計の時点のシートの状態のダイジェスト（タスク 7.2）。
///
/// 刻むのは**行識別子の並びと列名**である（モジュール docs「ダイジェスト」）。比較は
/// [`PlanDigest`] の等値で行う（[`ChangePlan::is_stale`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlanDigest(Blake3Digest);

impl PlanDigest {
    /// シートの現在の状態からダイジェストを求める。
    ///
    /// **適用の前に**（列名や行を書き換える前に）呼ぶこと。適用の後に呼ぶと、適用そのものを
    /// 変化として拾う。
    pub fn of(sheet: &Sheet) -> Self {
        Self(digest_of(sheet))
    }

    /// 生の 32 バイト。
    pub fn as_bytes(&self) -> &[u8; Blake3Digest::LEN] {
        self.0.as_bytes()
    }

    /// 小文字 hex（64 文字）。
    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }
}

/// 集計の結果と、適用に必要なすべての計算済みの値（design.md「Evolution Layer /
/// SchemaEvolution」の `ChangePlan`）。
///
/// 計画は**可謬な処理をすべて済ませている**（要件 8.6 の原子性の作り方）。適用（タスク 7.3）
/// は [`ChangePlan::columns`] と [`ChangePlan::rows`] を書き戻すだけであり、[`ChangePlan::digest`]
/// が現在のシートと一致することを先に確かめる。
#[derive(Debug)]
pub struct ChangePlan {
    sheet: SheetId,
    columns: Box<[String]>,
    rows: Box<[(RowId, Vec<CellValue>)]>,
    impact: ChangeImpact,
    digest: PlanDigest,
}

impl ChangePlan {
    /// 変更するシート。
    pub fn sheet(&self) -> SheetId {
        self.sheet
    }

    /// 新しい列名の配列（新しい宣言の並び順）。
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// 全行分の新しい値（行の並び順）。値は [`ChangePlan::columns`] と同じ並びである。
    ///
    /// 改名された列の値は運ばれ、追加された列の値は既定値または値なしであり、型が変わった
    /// 列の値は強制済みである。
    pub fn rows(&self) -> impl Iterator<Item = (RowId, &[CellValue])> {
        self.rows
            .iter()
            .map(|(row, values)| (*row, values.as_slice()))
    }

    /// 集計した影響（要件 8.2, 8.3）。
    ///
    /// 適用した結果はこの内容と一致する（要件 8.5。適用はタスク 7.3）。
    pub fn impact(&self) -> &ChangeImpact {
        &self.impact
    }

    /// 集計の時点のシートの状態のダイジェスト（タスク 7.2）。
    pub fn digest(&self) -> PlanDigest {
        self.digest
    }

    /// ドキュメントが集計の後で変わったか。
    ///
    /// 適用（タスク 7.3）が適用の前に呼ぶ判定である。シートが消えている場合も陳腐化として
    /// 扱う（計画の指す行がもう存在しない）。
    pub fn is_stale(&self, doc: &Document) -> bool {
        match doc.sheet_by_id(self.sheet) {
            Some(sheet) => PlanDigest::of(sheet) != self.digest,
            None => true,
        }
    }
}

/// 新しい宣言を適用した場合の影響を集計する（design.md「Evolution Layer /
/// SchemaEvolution」の Service Interface。tasks.md 7.2。要件 8.2, 8.3, 8.4）。
///
/// `doc` と `sheet` は現在の状態であり、`to` は適用したい新しい宣言である。戻り値の
/// [`ChangePlan`] は新しい列名の配列と全行分の新しい値を含み、**ドキュメントは一切変更
/// しない**（要件 8.4）。
///
/// # 前提と失敗
///
/// `to` は解決可能な宣言であること（design.md 同節の Preconditions）。解析・`$ref` の解決・
/// 既定値の適合のいずれかに失敗すれば [`SchemaError`] を返し、影響は集計しない。対象の
/// シートが文書に無い場合も計画できないため、位置と理由を持つ宣言の誤りとして返す
/// （design.md が `plan_change` に与えた誤り型は `SchemaError` だけである）。
pub fn plan_change(
    doc: &Document,
    sheet: SheetId,
    to: &Schema,
    registry: &TypeRegistry,
) -> Result<ChangePlan, SchemaError> {
    let Some(target) = doc.sheet_by_id(sheet) else {
        return Err(SchemaError::MalformedDeclaration {
            position: sheet.to_string(),
            reason: "the sheet to change does not exist in the document".to_owned(),
        });
    };

    // 新しい宣言を、シートが今持っている型定義で解決する（モジュール docs「宣言と型定義」）。
    let compiled = compile_target(target, to, registry)?;
    let from = parse_schema(target.root_schema().root().as_str())?;
    let changes = diff(&from, to);

    // 旧列名 → 行の値の位置。行の値はシートが持つ列名の並びに従う（design.md
    // 「Data Models / Domain Model」の不変条件 — 宣言の列の並びとシートの列名は一致する）。
    let old_positions: HashMap<&str, usize> = target
        .columns()
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index))
        .collect();

    let row_count = target.rows().len();
    let mut converted_rows = 0usize;
    let mut losing_rows = 0usize;
    let mut losses: Vec<ValueLoss> = Vec::new();
    // 行の並び順で「その行が違反を持つか」を記録する。行の添字は行の並びそのものである。
    let mut violating = vec![false; row_count];
    let mut computed: Vec<(RowId, Vec<CellValue>)> = Vec::with_capacity(row_count);

    // 1 行分だけを確保して使い回す（design.md「検証結果の表現」の上限の存在理由と同じ理由。
    // 上限なしで 1 行分を取り出す）。
    let mut scratch = ViolationReport::new(&ValidationOptions::unlimited());

    for (position, row) in target.rows().iter().enumerate() {
        let mut values = Vec::with_capacity(compiled.column_count());
        let mut converted = false;
        for (index, name) in compiled.columns().iter().enumerate() {
            let column = ColumnIndex::new(index);
            let source = source_value(&compiled, &changes, &old_positions, column, name, row);
            let coerced = coerce(&compiled, column, source);
            converted |= coerced.coercion != Coercion::Unchanged;
            values.push(coerced.value);
        }
        if converted {
            converted_rows += 1;
        }

        // 第 1 段（値ごとの判定）を計算しきった値に当てる。理由の写像は `cell` 層の
        // 1 セルの判定をそのまま使う（写しを作らない。タスク 5.1 の申し送り）。
        cell::validate_row(&compiled, Some(row.id()), &values, &mut scratch);
        if !scratch.take_violations().is_empty() {
            violating[position] = true;
        }

        if collect_losses(&changes, &old_positions, row, &mut losses) {
            losing_rows += 1;
        }

        computed.push((row.id(), values));
    }

    // 第 2 段（行を跨ぐ性質）を、計算しきった値に対して回す。`unique` と `refs` は参照が
    // 無ければ空を返すため、判定の要否を確かめずに呼んでよい（タスク 5.3 の申し送り）。
    // 参照先のシートは本計画の対象ではない（行は増減しない）ため、現在の文書で実在を
    // 判定してよい。参照先がこのシート自身である場合も、行の集合は変わらないので結論は同じ。
    let mut cross: HashSet<RowId> = unique::scan(
        &compiled,
        computed
            .iter()
            .map(|(row, values)| (*row, values.as_slice())),
    )
    .into_iter()
    .chain(refs::scan(
        doc,
        &compiled,
        computed
            .iter()
            .map(|(row, values)| (*row, values.as_slice())),
    ))
    .filter_map(|violation| violation.row())
    .collect();
    let mut violating_rows = 0usize;
    for (position, row) in target.rows().iter().enumerate() {
        if violating[position] || cross.remove(&row.id()) {
            violating_rows += 1;
        }
    }

    Ok(ChangePlan {
        sheet,
        columns: compiled.columns().to_vec().into_boxed_slice(),
        rows: computed.into_boxed_slice(),
        impact: ChangeImpact {
            converted_rows,
            violating_rows,
            losing_rows,
            losses,
        },
        digest: PlanDigest::of(target),
    })
}

/// 新しい列 1 本に入る、強制前の値を取り出す。
///
/// - 改名された列・名前が同じ列（[`SchemaDiff::carried_from`] が運搬元を返す列）→ 旧列の
///   値。シートがその名前の列を持たない、または行がその位置の値を持たないときは値なしである
///   （壊れた状態でも計画そのものは組み立てられるようにする。値なしの報告は検証層が行う）。
/// - 追加された列（`None`）→ 宣言された既定値、既定値が無ければ値なし（要件 8.8）。
///
/// 値の強制は呼び出し元が行う。型が変わった列も運搬元は名前で決まるため、この 1 つの経路で
/// 足りる。
fn source_value(
    compiled: &CompiledSchema,
    changes: &SchemaDiff,
    old_positions: &HashMap<&str, usize>,
    column: ColumnIndex,
    name: &str,
    row: &Row,
) -> CellValue {
    match changes.carried_from(name) {
        Some(origin) => old_positions
            .get(origin)
            .and_then(|&old| row.values().get(old))
            .cloned()
            .unwrap_or(CellValue::Null),
        None => compiled
            .default_value(column)
            .cloned()
            .unwrap_or(CellValue::Null),
    }
}

/// 削除される列の、値を持つ位置を集める（要件 8.3）。
///
/// 値なしは失う値ではない（モジュール docs）。1 件でも集めれば `true` を返す。
fn collect_losses(
    changes: &SchemaDiff,
    old_positions: &HashMap<&str, usize>,
    row: &Row,
    losses: &mut Vec<ValueLoss>,
) -> bool {
    let mut lost = false;
    for change in changes.changes() {
        let ColumnChange::Removed { name } = change else {
            continue;
        };
        let Some(&old) = old_positions.get(&**name) else {
            continue;
        };
        match row.values().get(old) {
            Some(value) if *value != CellValue::Null => {
                losses.push(ValueLoss {
                    row: row.id(),
                    column: name.clone(),
                });
                lost = true;
            }
            _ => {}
        }
    }
    lost
}

/// 新しい宣言を、シートが今持っている型定義で解決して計画へ落とす。
///
/// [`compile::compile`](crate::compile::compile) と同じ読み出しを、シートの外にある宣言に
/// 対して行う（モジュール docs「宣言と型定義」）。
fn compile_target(
    target: &Sheet,
    to: &Schema,
    registry: &TypeRegistry,
) -> Result<CompiledSchema, SchemaError> {
    let part = target.root_schema();
    let mut definitions = Vec::with_capacity(part.type_defs().len());
    for definition in part.type_defs() {
        definitions.push(TypeDefinition {
            id: definition.id(),
            definition: parse_type_definition(definition.definition().as_str())?,
        });
    }
    compile_declaration(to, &definitions, registry)
}

/// シートの状態（行識別子の並びと列名）のダイジェストを求める（モジュール docs
/// 「ダイジェスト」）。
///
/// 行識別子は固定長のテキスト形、列名は長さ接頭辞つきで並べる。区切りが曖昧にならないため、
/// 列名の並びを入れ替えただけでも値が変わる。
fn digest_of(sheet: &Sheet) -> Blake3Digest {
    let rows = sheet.rows();
    let columns = sheet.columns();
    let mut bytes = Vec::with_capacity(
        ROW_ID_TEXT_LEN * (rows.len() + 1)
            + columns.iter().map(|name| name.len() + 4).sum::<usize>(),
    );
    let mut buf = [0u8; ROW_ID_TEXT_LEN];
    bytes.extend_from_slice(sheet.id().as_str(&mut buf).as_bytes());
    for row in rows {
        bytes.extend_from_slice(row.id().as_str(&mut buf).as_bytes());
    }
    for name in columns {
        bytes.extend_from_slice(&(name.len() as u32).to_be_bytes());
        bytes.extend_from_slice(name.as_bytes());
    }
    Blake3Digest::of(&bytes)
}

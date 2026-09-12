//! 検証層（design.md「コンポーネントとファイルの対応」の `SheetValidator` /
//! `CellValidator` / `UniqueScan` / `ReferenceScan` / `ViolationReport`。tasks.md 群 5）。
//!
//! 10 万行を 1 回の呼び出しで検証し、違反を位置と理由つきで報告する。**一括経路であること**
//! が本層の契約である（要件 10.4。`structure.md`「性能はドメイン側で守る」: 行ごとに境界を
//! 越える API を公開しない）。走査は 2 段であり、第 1 段が行ごとの値の判定（[`cell`]）、
//! 第 2 段が行を跨ぐ性質の判定（[`unique`] と [`refs`]）である。最後に安定併合して順序を
//! 決める（design.md「System Flows」）。
//!
//! | モジュール | 責務 | 要件 |
//! |------------|------|------|
//! | [`cell`] | 1 セルの判定と入れ子の再帰 | 2.6, 3.4, 4.4, 5.3, 11.3, 11.5 |
//! | [`unique`] | 一意制約の 1 パス判定 | 4.6, 4.7 |
//! | [`refs`] | シート間参照の一括実在判定 | 9.2, 9.3, 9.4, 9.5, 9.6 |
//! | [`report`] | 違反の表現・順序・上限 | 5.1, 5.2, 5.3, 5.5, 5.6 |
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本層は `compile` 層までを参照でき、`write` 以降へは依存しない。
//! 本層の中で最も下流に置かれる葉は [`report`] であり、他の 3 つのサブモジュールと本
//! ファイルがその型を組み立てる。
//!
//! # 入口（tasks.md 5.4）
//!
//! - [`validate_sheet`]: シート全体の一括検証（要件 5.4, 5.7, 10.4）
//! - [`validate_columns`]: 指定した列だけの再検証（要件 10.5）
//!
//! どちらも 1 回の呼び出しで全行を舐める。呼び出し元が行ごとに本層を呼ぶ経路は用意しない
//! （要件 10.4）。
//!
//! # 2 段の併合（design.md「System Flows / 一括検証の 2 段構成」。要件 5.4, 5.5）
//!
//! 第 2 段（[`unique::scan`] と [`refs::scan`]）は全行を走査し終えてから違反を返す。
//! したがって両段の結果をそのまま連結すると、同じ行の違反が段を跨いで離れて並ぶ。本
//! ファイルは第 2 段の違反を行の識別子でまとめ、**第 1 段を行の並び順に流しながら 1 行
//! ずつ併合する**。並びは 行の並び順 → 列の添字 → 入れ子の位置 である。
//!
//! 行ごとに併合するのは記憶域のためでもある。第 1 段の違反を全行分ためてから並べ替えると、
//! 上限（[`ValidationOptions`]）が守るはずの記憶域が上限の外で膨らむ（design.md「検証結果の
//! 表現」の上限の存在理由）。1 行分だけを確保し、最終の報告は上限までしか保持しない。
//!
//! # 入れ子の位置の順序（[`compare_paths`]）
//!
//! 位置の段を先頭から比べ、フィールド名は文字列として、添字は数値として比べる。**宣言順
//! ではない** — 併合は計画の木を引かずに違反だけで順序を決める必要があり、値の形から
//! 復元できるのは名前と添字だけである。同じ位置の 2 件は第 1 段を先に置く（`sort_by` が
//! 安定であるため）。この順序は同じ入力に対して常に同じ結果を与える（要件 5.5）。
//!
//! # 拡張型の列は列ごとに 1 回の一括判定で判定する（要件 11.6）
//!
//! 第 1 段の行ごとの判定のうち、**列の検証器が [`ColumnValidator::Custom`] である列**は
//! セルごとに実装（`CustomType::validate`）を呼ばず、[`CustomScan`] が列ごとに 1 回だけ
//! 一括判定（`CustomType::validate_batch`）を呼ぶ。拡張型の実体は `custom-types`（JS）に
//! あり、10 万行でセルごとに境界を越えると予算に収まらないためである（design.md「Registry
//! Layer / TypeRegistry」の「一括の継ぎ目」。要件 10.4, 11.6）。
//!
//! 実装の契約により、`Err` を返しても**失敗より前の判定は `out` へ出現順に渡されている**
//! （タスク 4.1 の裁定）。したがって失敗位置は「`out` が受け取った件数 `k`」であり、その
//! 1 件を `CustomFailed` として報告してから `k + 1` 番目以降でやり直す。各回は必ず 1 件
//! 以上を消費するため有限で止まり、健全な実装では呼び出しは列ごとに 1 回のままである
//! （design.md の Postconditions「`Err` はその値の違反になり、走査は次の値へ進む」を、
//! バッチ経路でも**値ごとの隔離**として保つ）。
//!
//! 値は行優先（`Row::values()`）で保持されるのに対し `validate_batch` は列の連続した
//! スライスを要求するため、**拡張型の列の値だけを複製して集める**。掛かるのは拡張型の列
//! だけであり、`CustomType` のシグネチャを変える（design.md の Revalidation Triggers）より
//! 小さい。集めた値のうち保持し続けるのは**違反になる値だけ**であり（適合した値は捨てる）、
//! 記憶域は第 2 段が行ごとに作る違反と同じ桁に収まる。値なし（[`CellValue::Null`]）は実装へ
//! 渡さない — 値なしはどの変種でも適合であり、受理の可否は列の `required` が決める
//! （タスク 4.2 の裁定）。必須の列の値なしは本経路が
//! [`ViolationReason::MissingValue`](report::ViolationReason) として報告する。
//!
//! 併合の並びは拡張型の列でも変わらない（行の並び順 → 列の添字 → 入れ子の位置）。
//! `CustomScan` の結果は行の添字つきで保持し、行ごとに第 1 段の直後へ差し込む
//! （[`CustomScan::append`]）。
//!
//! **入れ子のフィールドの型として指定された拡張型**（`object` の内側など）は本経路の
//! 対象外であり、[`cell`] が値ごとに 1 件ずつ実装へ委ねる。**一括の継ぎ目は最上位の列を
//! 対象とする**ためである — 入れ子の内側の値は行ごとに形が異なりうるので、**同じ位置の
//! 値**を行を跨いで集める収集と位置の写像が要る（`CustomType` のシグネチャの制約ではなく、
//! その収集が未実装である）。入れ子の内側は組込型のフィールドも拡張型のフィールドも
//! **同じく値ごとに判定する**ので、「組込型のみの場合と同一の経路」という要件 11.6 の読みは
//! 最上位の列について満たされている。入れ子の拡張型でも境界を越える回数が問題になったら、
//! `custom-types` 側の要求として収集経路を足す（design.md の Revalidation Triggers 相当）。

pub mod cell;
pub mod refs;
pub mod report;
pub mod unique;

use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};

use document_format::{CellValue, Document, Row, RowId, SheetId};

use crate::compile::plan::{ColumnIndex, ColumnValidator, ColumnViolation};
use crate::compile::CompiledSchema;
use crate::registry::CustomVerdict;

use report::{
    SheetReport, ValidationOptions, ValuePath, ValuePathSegment, Violation, ViolationReport,
};

/// シート全体の一括検証（tasks.md 5.4。要件 5.4, 5.7, 10.4）。
///
/// `sheet` の全行を 1 回の呼び出しで検証し、上限までの違反・総件数・違反を持つ行の一覧を
/// 返す。`schema` は同じシートからコンパイルした計画であること（`Row::values()` の並びが
/// [`CompiledSchema::columns`] と一致する前提。design.md「Data Models / Domain Model」の
/// 不変条件）。
///
/// 文書を変更しない。何度呼んでも同じ結果になる（design.md「Validate Layer /
/// SheetValidator」の Batch 契約）。
pub fn validate_sheet(
    doc: &Document,
    sheet: SheetId,
    schema: &CompiledSchema,
    options: &ValidationOptions,
) -> SheetReport {
    validate(doc, sheet, schema, None, options)
}

/// 指定した列だけの再検証（tasks.md 5.4。要件 10.5）。
///
/// スキーマの一部が変わったときに全列を舐め直さないための経路である。指定できるのは列の
/// 添字であり、`columns` の**並びは結果に影響しない**（列添字の昇順へ正規化し、重複も
/// 畳む）。行を跨ぐ性質（一意性と参照の実在）も指定した列に閉じて判定する。
pub fn validate_columns(
    doc: &Document,
    sheet: SheetId,
    schema: &CompiledSchema,
    columns: &[ColumnIndex],
    options: &ValidationOptions,
) -> SheetReport {
    let mut selected = columns.to_vec();
    selected.sort_unstable();
    selected.dedup();
    validate(doc, sheet, schema, Some(&selected), options)
}

/// 拡張型の列の一括判定（要件 11.6。design.md「Registry Layer / TypeRegistry」の
/// 「一括の継ぎ目」）。
///
/// 検証の対象の列から**列の検証器が [`ColumnValidator::Custom`] である列**を取り出し、
/// 列ごとに 1 回だけ `CustomType::validate_batch` を呼ぶ。結果は行の添字つきで保持し、
/// 行ごとの併合（[`CustomScan::append`]）で第 1 段と同じ位置へ置く。理由はモジュール docs
/// 「拡張型の列は列ごとに 1 回の一括判定で判定する」にある。
///
/// 拡張型の列が 1 つも無ければ空であり、[`CustomScan::remaining`] が `None`（全列）を返す
/// ため、呼び出し元は従来の経路をそのまま通る（確保も増えない）。
struct CustomScan<'a> {
    /// 拡張型の列ごとの結果。対象の列の並びのうち拡張型であるものを順に持つ。
    columns: Vec<CustomColumn<'a>>,
}

/// 拡張型の 1 列分の一括判定の結果（[`CustomScan`]）。
struct CustomColumn<'a> {
    /// 列の添字。
    column: ColumnIndex,
    /// 列名（違反が運ぶ。計画から借りる）。
    name: &'a str,
    /// 宣言された拡張型の識別子（違反の期待内容が運ぶ）。
    id: Box<str>,
    /// 報告すべき判定だけを**行の添字の昇順**に保持する（適合は保持しない）。
    findings: VecDeque<(usize, CustomFinding)>,
}

/// 一括判定が返した、報告すべき 1 件（[`CustomColumn`]）。
enum CustomFinding {
    /// 必須の列に値なしがある（実装へは渡していない）。
    Missing,
    /// 実装が値を拒否した（要件 11.3）。
    Rejected {
        /// 実装が返した文脈（表示用の文言ではない）。
        reason: Box<str>,
        /// 拒否された値。
        actual: CellValue,
    },
    /// 実装の判定が失敗した（要件 11.5）。
    Failed {
        /// 実装が返した文脈（表示用の文言ではない）。
        reason: Box<str>,
        /// 判定できなかった値。
        actual: CellValue,
    },
}

impl<'a> CustomScan<'a> {
    /// 対象の列から拡張型の列を集め、列ごとに 1 回の一括判定を走らせる。
    ///
    /// `selected` は検証の対象の列（`None` は全列）である。列指定の再検証でも同じ一括経路を
    /// 通る（要件 10.5）。対象の外の列は走査しない。
    fn prepare(schema: &'a CompiledSchema, rows: &[Row], selected: Option<&[ColumnIndex]>) -> Self {
        let mut columns = Vec::new();
        match selected {
            Some(selected) => {
                for column in selected {
                    if let Some(found) = CustomColumn::scan(schema, *column, rows) {
                        columns.push(found);
                    }
                }
            }
            None => {
                for index in 0..schema.column_count() {
                    if let Some(found) = CustomColumn::scan(schema, ColumnIndex::new(index), rows) {
                        columns.push(found);
                    }
                }
            }
        }
        Self { columns }
    }

    /// 第 1 段（行ごとの値の判定）が担う列。拡張型の列を除いたものであり、拡張型の列が
    /// 1 つも無ければ `None`（呼び出し元は現在の経路をそのまま通す）。
    ///
    /// 列の並びは `selected`（または全列）の昇順のままである — 第 1 段の違反の並びは列の
    /// 添字で決まるため、並びを変えても結果は変わらない。
    fn remaining(
        &self,
        schema: &CompiledSchema,
        selected: Option<&[ColumnIndex]>,
    ) -> Option<Vec<ColumnIndex>> {
        if self.columns.is_empty() {
            return None;
        }
        let unbatched =
            |column: &ColumnIndex| !self.columns.iter().any(|found| found.column == *column);
        Some(match selected {
            Some(selected) => selected.iter().copied().filter(unbatched).collect(),
            None => (0..schema.column_count())
                .map(ColumnIndex::new)
                .filter(unbatched)
                .collect(),
        })
    }

    /// その行の一括判定の結果を違反として足す（**行の並び順に**呼ばれる）。
    ///
    /// 結果は行の添字の昇順に保持されているため、列ごとに先頭の 1 件だけを見ればよい
    /// （1 つの行は 1 つの列に 1 つの値しか持たない）。
    fn append(&mut self, row: Option<RowId>, row_index: usize, out: &mut Vec<Violation>) {
        for column in &mut self.columns {
            let Some((index, _)) = column.findings.front() else {
                continue;
            };
            if *index != row_index {
                continue;
            }
            let (_, finding) = column.findings.pop_front().expect("先頭を確認済み");
            out.push(column.violation(row, finding));
        }
    }
}

impl<'a> CustomColumn<'a> {
    /// 1 列分の値を集めて一括判定を走らせる（列ごとに 1 回。要件 11.6）。
    ///
    /// 拡張型の列でなければ `None`。
    fn scan(schema: &'a CompiledSchema, column: ColumnIndex, rows: &[Row]) -> Option<Self> {
        let ColumnValidator::Custom { id, imp } = schema.validator(column)? else {
            return None;
        };
        let name = schema.columns().get(column.index())?;
        let required = schema.required(column);

        // 行優先の保持（`Row::values()`）から、この列の値だけを複製して集める
        // （`validate_batch` は連続したスライスを要求する。モジュール docs 参照）。
        // 値なしは実装へ渡さず、必須の列では値なしの違反をここで作る。
        let mut findings = Vec::new();
        let mut values = Vec::new();
        let mut row_indices = Vec::new();
        for (row_index, row) in rows.iter().enumerate() {
            match row.values().get(column.index()) {
                Some(value) if !matches!(value, CellValue::Null) => {
                    row_indices.push(row_index);
                    values.push(value.clone());
                }
                _ if required => findings.push((row_index, CustomFinding::Missing)),
                _ => {}
            }
        }

        // 一括判定。`Err` のときの失敗位置は「`out` が受け取った件数」である（タスク 4.1 の
        // 契約）。その 1 件だけを失敗として報告し、次の値からやり直す。
        let mut offset = 0;
        while offset < values.len() {
            let slice = &values[offset..];
            let mut verdicts: Vec<(usize, CustomVerdict)> = Vec::new();
            let outcome =
                imp.validate_batch(slice, &mut |index, verdict| verdicts.push((index, verdict)));
            let held = verdicts.len();
            for (index, verdict) in verdicts {
                let CustomVerdict::Rejected { reason } = verdict else {
                    continue;
                };
                let Some((row_index, value)) =
                    row_indices.get(offset + index).zip(slice.get(index))
                else {
                    continue;
                };
                findings.push((
                    *row_index,
                    CustomFinding::Rejected {
                        reason,
                        actual: value.clone(),
                    },
                ));
            }
            match outcome {
                Ok(()) => offset += slice.len(),
                Err(failure) => {
                    let failed = offset + held;
                    // 契約に反して失敗位置が値の外を指す実装では帰属先が無いため、そこで止める。
                    let Some(row_index) = row_indices.get(failed).copied() else {
                        break;
                    };
                    findings.push((
                        row_index,
                        CustomFinding::Failed {
                            reason: failure.reason().into(),
                            actual: values[failed].clone(),
                        },
                    ));
                    offset = failed + 1;
                }
            }
        }

        // 値なしの違反と一括判定の結果を行の添字の昇順に揃える（1 つの行は 1 つの列に
        // 1 つの値しか持たないため、同じ添字の 2 件は無い）。
        findings.sort_unstable_by_key(|(row_index, _)| *row_index);
        Some(Self {
            column,
            name,
            id: id.as_str().into(),
            findings: findings.into(),
        })
    }

    /// 1 件の違反を組み立てる。拡張型の値は列の直下にあるため、位置はセル直下である。
    ///
    /// 理由の組み立ては [`cell`] の写像をそのまま使う — 組込型のセルと同じ
    /// [`ViolationReason`](report::ViolationReason) へ落とす規則を 2 箇所に置かない
    /// （タスク 5.1 の申し送り）。
    fn violation(&self, row: Option<RowId>, finding: CustomFinding) -> Violation {
        let reason = match finding {
            CustomFinding::Missing => cell::missing_value_reason(),
            CustomFinding::Rejected { reason, actual } => cell::reason(
                ColumnViolation::CustomRejected {
                    id: self.id.clone(),
                    reason,
                },
                actual,
            ),
            CustomFinding::Failed { reason, actual } => cell::reason(
                ColumnViolation::CustomFailed {
                    id: self.id.clone(),
                    reason,
                },
                actual,
            ),
        };
        Violation::at_cell(row, self.column, self.name, reason)
    }
}

/// 2 段の走査と安定併合（[`validate_sheet`] と [`validate_columns`] が共有する本体）。
///
/// `selected` が `Some` のときはその列の違反だけを結果に載せる（列添字の昇順に正規化済みで
/// あること）。`None` は全列である。
fn validate(
    doc: &Document,
    sheet: SheetId,
    schema: &CompiledSchema,
    selected: Option<&[ColumnIndex]>,
    options: &ValidationOptions,
) -> SheetReport {
    let mut report = ViolationReport::new(options);
    let Some(target) = doc.sheet_by_id(sheet) else {
        // 文書に無いシート（削除されたシートを指した場合）は検証する行を持たない。
        return report.finish(sheet);
    };
    let rows = target.rows();

    // 拡張型の列は列ごとに 1 回の一括判定へ載せる（要件 11.6）。拡張型の列が 1 つも無ければ
    // 何も確保せず、第 1 段は従来どおり全列（または指定列）を判定する。
    let mut custom = CustomScan::prepare(schema, rows, selected);
    let remaining: Option<Vec<ColumnIndex>> = custom.remaining(schema, selected);
    let first_stage = match &remaining {
        Some(columns) => Some(columns.as_slice()),
        None => selected,
    };

    // 第 2 段: 行を跨ぐ性質（一意性と参照の実在）。全行を走査し終えてから違反が返るため、
    // 行ごとに併合できるよう行の識別子でまとめておく。参照を 1 つも含まない計画では
    // どちらの走査も空を返す（呼び出し元は判定の要否を確かめずに呼んでよい）。
    let mut cross_by_row: HashMap<RowId, Vec<Violation>> = HashMap::new();
    let mut orphans: Vec<Violation> = Vec::new();
    let found = unique::scan(schema, rows.iter().map(|row| (row.id(), row.values())))
        .into_iter()
        .chain(refs::scan(
            doc,
            schema,
            rows.iter().map(|row| (row.id(), row.values())),
        ));
    for violation in found {
        if let Some(selected) = selected {
            if selected.binary_search(&violation.column()).is_err() {
                continue;
            }
        }
        match violation.row() {
            Some(row) => cross_by_row.entry(row).or_default().push(violation),
            // 行に属さない違反（列そのものの問題）は行の並びに置けない。末尾へ回す。
            None => orphans.push(violation),
        }
    }

    // 第 1 段: 行ごとの値の判定。行の並び順に流し、その行の第 2 段の違反を併合する。
    // 確保するのは 1 行分だけである（モジュール docs「2 段の併合」）。
    let mut scratch = ViolationReport::new(&ValidationOptions::unlimited());
    for (row_index, row) in rows.iter().enumerate() {
        let mut violations = value_violations(schema, row, first_stage, &mut scratch);
        // 拡張型の列の一括判定の結果（第 1 段の直後へ差し込み、同じ位置の 2 件では第 1 段を
        // 先に置く — セルごとに判定していたときと同じ並びになる）。
        custom.append(Some(row.id()), row_index, &mut violations);
        if let Some(mut extra) = cross_by_row.remove(&row.id()) {
            violations.append(&mut extra);
        }
        // 安定な並べ替えであり、同じ位置の 2 件は第 1 段が先に残る。
        violations.sort_by(compare_violations);
        for violation in violations {
            report.push(violation);
        }
    }

    // 行の一覧に置けなかった違反（列そのものの問題。design.md「検証結果の表現」の
    // `row: Option<RowId>`）。行の一覧に載らないため、行の並びの外へ決定的に並べて置く。
    orphans.sort_by(compare_violations);
    report.extend(orphans);

    // 第 2 段の走査は第 1 段と同じ行の一覧を見るため、行の識別子は必ず一致する。ここに
    // 残りがあれば、その違反は行の一覧に現れない行を指している（報告から落ちる）。
    debug_assert!(
        cross_by_row.is_empty(),
        "第 2 段の違反が行の一覧の外の行を指した"
    );

    report.finish(sheet)
}

/// 1 行分の第 1 段の違反を取り出す（`scratch` は行ごとに使い回す）。
///
/// `columns` は判定する列（`None` は全列）であり、拡張型の列は [`CustomScan`] が担うため
/// **呼び出し元が取り除いた**ものを渡す（モジュール docs「拡張型の列は列ごとに 1 回の
/// 一括判定で判定する」）。
///
/// 確保するのは 1 行分だけであり、`scratch` は行を跨いで空に戻る
/// （[`ViolationReport::take_violations`]）。
fn value_violations(
    schema: &CompiledSchema,
    row: &Row,
    columns: Option<&[ColumnIndex]>,
    scratch: &mut ViolationReport,
) -> Vec<Violation> {
    match columns {
        Some(columns) => {
            cell::validate_row_columns(schema, Some(row.id()), row.values(), columns, scratch)
        }
        None => cell::validate_row(schema, Some(row.id()), row.values(), scratch),
    }
    scratch.take_violations()
}

/// 同じ行の中での違反の順序（design.md「検証結果の表現」: 列の添字 → 入れ子の位置）。
///
/// 行は呼び出し側が行の並び順に処理するため、ここでは行を比べない。
fn compare_violations(left: &Violation, right: &Violation) -> Ordering {
    left.column()
        .cmp(&right.column())
        .then_with(|| compare_paths(left.path(), right.path()))
}

/// 入れ子の位置の順序（モジュール docs「入れ子の位置の順序」）。
///
/// 段を先頭から比べ、短い方が先である（親の位置が子の位置より先に来る）。
fn compare_paths(left: &ValuePath, right: &ValuePath) -> Ordering {
    let mut left = left.segments().iter();
    let mut right = right.segments().iter();
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(field), Some(other)) => match compare_segments(field, other) {
                Ordering::Equal => continue,
                order => return order,
            },
        }
    }
}

/// 位置の 1 段の順序。名前は文字列として、添字は数値として比べる。
///
/// 1 つの値はオブジェクトか配列のどちらかであるため、名前と添字が同じ段で競合することは
/// 無い。それでも全順序にするため、名前を先に置く。
fn compare_segments(left: &ValuePathSegment, right: &ValuePathSegment) -> Ordering {
    match (left, right) {
        (ValuePathSegment::Field(left), ValuePathSegment::Field(right)) => left.cmp(right),
        (ValuePathSegment::Index(left), ValuePathSegment::Index(right)) => left.cmp(right),
        (ValuePathSegment::Field(_), ValuePathSegment::Index(_)) => Ordering::Less,
        (ValuePathSegment::Index(_), ValuePathSegment::Field(_)) => Ordering::Greater,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::compile::compile_declaration;
    use crate::declaration::{ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl};
    use crate::registry::{
        CustomType, CustomTypeFailure, CustomTypeId, CustomVerdict, TypeRegistry,
    };
    use crate::types::TypeKind;
    use crate::validate::report::ViolationReason;
    use document_format::{CellValue, Document, IdFactory, NestedValue, RowId, SheetId};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// 整数のセル値。
    fn int(value: i64) -> CellValue {
        CellValue::Int(value)
    }

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

    /// 種別と制約による型の指定。
    fn kind(kind: TypeKind, constraints: Constraints) -> TypeDecl {
        TypeDecl::Kind {
            kind: DeclaredKind::Known(kind),
            constraints,
        }
    }

    /// 種別と制約による列を組み立てる。
    fn column(name: &str, kind_of: TypeKind, constraints: Constraints) -> ColumnDecl {
        ColumnDecl {
            name: name.into(),
            ty: kind(kind_of, constraints),
            required: false,
            unique: false,
            default: None,
            description: None,
        }
    }

    /// 一意制約を持つ列。
    fn unique_column(name: &str, kind_of: TypeKind) -> ColumnDecl {
        ColumnDecl {
            unique: true,
            ..column(name, kind_of, Constraints::default())
        }
    }

    /// 値なしを許さない列。
    fn required_column(name: &str, kind_of: TypeKind) -> ColumnDecl {
        ColumnDecl {
            required: true,
            ..column(name, kind_of, Constraints::default())
        }
    }

    /// 上限つきの整数の列。
    fn bounded_int_column(name: &str, max: i64) -> ColumnDecl {
        column(
            name,
            TypeKind::Int,
            Constraints {
                max: Some(CellValue::Int(max)),
                ..Constraints::default()
            },
        )
    }

    /// 参照先シートを指す型の指定。
    fn ref_type(sheet: SheetId) -> TypeDecl {
        kind(
            TypeKind::Ref,
            Constraints {
                sheet: Some(sheet),
                ..Constraints::default()
            },
        )
    }

    /// 参照先シートを指す列。
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

    /// 宣言から計画を組み立てる。
    fn compiled(columns: Vec<ColumnDecl>) -> CompiledSchema {
        compile_declaration(&Schema { columns }, &[], &TypeRegistry::new())
            .expect("標本の宣言はコンパイルできる")
    }

    /// 違反の並びを `(行, 列添字, 入れ子の位置)` の形で取り出す。
    fn positions(report: &SheetReport) -> Vec<(Option<RowId>, usize, Vec<ValuePathSegment>)> {
        report
            .violations()
            .iter()
            .map(|violation| {
                (
                    violation.row(),
                    violation.column().index(),
                    violation.path().segments().to_vec(),
                )
            })
            .collect()
    }

    /// フィールド名 1 段の位置。
    fn at_field(name: &str) -> Vec<ValuePathSegment> {
        vec![ValuePathSegment::Field(name.into())]
    }

    /// 2 段の標本（値の違反と行を跨ぐ違反が同じ行に同居する）。
    ///
    /// - `品番` は一意（同じ値を持つ 2 行がある）
    /// - `数量` は上限 10（2 行とも超える）
    /// - `仕入先` は参照（1 行目は実在、2 行目は実在しない）
    /// - `属性` は入れ子（内側に参照と上限つきの整数を持つ）
    fn two_stage_sample() -> (Document, SheetId, CompiledSchema, RowId, RowId) {
        let mut doc = Document::new();
        let target = add_sheet(&mut doc, "仕入先", &["名称"]);
        let sheet = add_sheet(&mut doc, "発注", &["品番", "数量", "仕入先", "属性"]);
        let supplier = add_row(&mut doc, target, vec![text("甲")]);
        let missing = IdFactory::new().new_row_id().to_string();

        let attributes = Constraints {
            fields: vec![
                field("a", ref_type(target)),
                field(
                    "b",
                    kind(
                        TypeKind::Int,
                        Constraints {
                            max: Some(CellValue::Int(10)),
                            ..Constraints::default()
                        },
                    ),
                ),
            ],
            ..Constraints::default()
        };
        let schema = compiled(vec![
            unique_column("品番", TypeKind::Text),
            bounded_int_column("数量", 10),
            ref_column("仕入先", target),
            column("属性", TypeKind::Object, attributes),
        ]);

        let first = add_row(
            &mut doc,
            sheet,
            vec![
                text("A"),
                int(99),
                text(&supplier.to_string()),
                object(vec![("a", text(&supplier.to_string())), ("b", int(1))]),
            ],
        );
        let second = add_row(
            &mut doc,
            sheet,
            vec![
                text("A"),
                int(99),
                text(&missing),
                object(vec![("a", text(&missing)), ("b", int(99))]),
            ],
        );
        (doc, sheet, schema, first, second)
    }

    /// 第 1 段と第 2 段の違反が、行の並び順 → 列の添字 → 入れ子の位置で併合される
    /// （tasks.md 5.4。要件 5.4, 5.5, 10.4）。
    #[test]
    fn the_two_stages_are_merged_by_row_then_column_then_nested_position() {
        let (doc, sheet, schema, first, second) = two_stage_sample();
        let report = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());

        assert_eq!(
            vec![
                (Some(first), 0, vec![]),
                (Some(first), 1, vec![]),
                (Some(second), 1, vec![]),
                (Some(second), 2, vec![]),
                (Some(second), 3, at_field("a")),
                (Some(second), 3, at_field("b")),
            ],
            positions(&report),
            "2 段の併合の並びが違う"
        );

        // 2 行目で初めて現れる重複の違反は、**最初に現れた行**（1 行目）に属する。
        assert!(
            matches!(
                report.violations()[0].reason(),
                ViolationReason::Duplicate { rows, .. } if rows.as_slice() == [first, second].as_slice()
            ),
            "重複の違反が重複するすべての行を運んでいない"
        );
        assert!(
            matches!(
                report.violations()[1].reason(),
                ViolationReason::OutOfRange { .. }
            ) && matches!(
                report.violations()[2].reason(),
                ViolationReason::OutOfRange { .. }
            ) && matches!(
                report.violations()[3].reason(),
                ViolationReason::BrokenReference { .. }
            ) && matches!(
                report.violations()[4].reason(),
                ViolationReason::BrokenReference { .. }
            ) && matches!(
                report.violations()[5].reason(),
                ViolationReason::OutOfRange { .. }
            ),
            "違反の理由が期待と違う"
        );

        // 違反を持つ行だけが行の並び順で載る（要件 5.4）。
        assert_eq!(vec![first, second], report.invalid_rows());
        assert_eq!(6, report.total_violations());
    }

    /// 同一の入力に対して違反集合と順序が常に一致する（tasks.md 5.4。要件 5.5）。
    #[test]
    fn the_same_input_always_yields_the_same_violations_in_the_same_order() {
        let (doc, sheet, schema, _, _) = two_stage_sample();
        let first = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
        for _ in 0..8 {
            let again = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
            assert_eq!(first, again, "同じ入力で違反集合か順序が変わった");
        }
    }

    /// 上限に達しても総件数を数え続け、上限がなければ全件を保持する（要件 5.6）。
    #[test]
    fn the_cap_limits_held_violations_but_not_the_count_or_the_invalid_rows() {
        let mut doc = Document::new();
        let sheet = add_sheet(&mut doc, "空", &["甲", "乙", "丙"]);
        let mut rows = Vec::new();
        for _ in 0..40 {
            rows.push(add_row(&mut doc, sheet, vec![CellValue::Null; 3]));
        }
        let schema = compiled(vec![
            required_column("甲", TypeKind::Text),
            required_column("乙", TypeKind::Int),
            required_column("丙", TypeKind::Bool),
        ]);

        let capped = validate_sheet(&doc, sheet, &schema, &ValidationOptions::capped(7));
        assert_eq!(
            7,
            capped.violations().len(),
            "上限までの違反を保持していない"
        );
        assert_eq!(120, capped.total_violations(), "総件数を数え落としている");
        assert_eq!(
            rows,
            capped.invalid_rows(),
            "違反を持つ行の一覧が欠けている"
        );
        assert!(capped.is_truncated(), "打ち切りが観測できない");

        let unlimited = validate_sheet(&doc, sheet, &schema, &ValidationOptions::unlimited());
        assert_eq!(
            120,
            unlimited.violations().len(),
            "上限なしで全件が残らない"
        );
        assert!(!unlimited.is_truncated(), "上限なしで打ち切りになっている");
    }

    /// 指定した列だけを再検証し、その列に閉じた第 1 段と第 2 段の違反を返す
    /// （tasks.md 5.4。要件 10.5）。
    #[test]
    fn revalidating_selected_columns_reports_only_those_columns() {
        let (doc, sheet, schema, first, second) = two_stage_sample();
        let options = ValidationOptions::default();

        let by_column = |columns: &[ColumnIndex]| {
            positions(&validate_columns(&doc, sheet, &schema, columns, &options))
        };

        // 要求の並びに依らず列添字の順で報告する。
        assert_eq!(
            vec![
                (Some(first), 0, vec![]),
                (Some(second), 3, at_field("a")),
                (Some(second), 3, at_field("b")),
            ],
            by_column(&[ColumnIndex::new(3), ColumnIndex::new(0)]),
            "指定した列の違反だけが列添字の順に出ていない"
        );

        // 第 2 段の違反（重複・壊れた参照）も指定した列に閉じる。
        assert_eq!(
            vec![(Some(first), 0, vec![])],
            by_column(&[ColumnIndex::new(0)]),
            "指定していない列の違反が混ざっている"
        );
        assert!(by_column(&[]).is_empty(), "空の指定で違反が出ている");
        assert!(
            by_column(&[ColumnIndex::new(99)]).is_empty(),
            "計画の外の列が判定された"
        );
    }

    /// 検証する行を持たないシートは、違反の無い結果になる（要件 5.4, 5.7）。
    #[test]
    fn a_sheet_without_rows_or_a_document_without_the_sheet_yields_no_violations() {
        let (doc, _sheet, schema, _, _) = two_stage_sample();
        let options = ValidationOptions::default();

        // 行を 1 つも持たないシート。
        let mut empty_document = Document::new();
        let empty = add_sheet(&mut empty_document, "空", &["品番"]);
        let report = validate_sheet(&empty_document, empty, &schema, &options);
        assert_eq!(
            0,
            report.total_violations(),
            "行の無いシートで違反が出ている"
        );
        assert!(
            report.invalid_rows().is_empty(),
            "行の無いシートで違反行が出ている"
        );

        // 文書に無いシート（削除されたシートを指した場合）。
        let missing = IdFactory::new().new_sheet_id();
        let report = validate_sheet(&doc, missing, &schema, &options);
        assert_eq!(
            0,
            report.total_violations(),
            "文書に無いシートで違反が出ている"
        );
        assert!(
            report.invalid_rows().is_empty(),
            "文書に無いシートで違反行が出ている"
        );
    }

    /// 標本の拡張型の識別子（`〒` で始まるテキストだけを受理する）。
    const POSTAL: &str = "postal-code";

    /// 失敗を返す標本の拡張型の識別子。
    const FLAKY: &str = "flaky";

    /// 拡張型の列の宣言。
    fn custom_column(name: &str, id: &str, required: bool) -> ColumnDecl {
        ColumnDecl {
            required,
            ..column(
                name,
                TypeKind::Custom,
                Constraints {
                    custom_type: Some(id.into()),
                    ..Constraints::default()
                },
            )
        }
    }

    /// 拡張型を 1 つ登録した計画を組み立てる。
    fn compiled_with_custom(
        columns: Vec<ColumnDecl>,
        implementation: Arc<dyn CustomType>,
    ) -> CompiledSchema {
        let mut registry = TypeRegistry::new();
        registry
            .register(implementation)
            .expect("標本の拡張型は登録できる");
        compile_declaration(&Schema { columns }, &[], &registry)
            .expect("標本の宣言はコンパイルできる")
    }

    /// `〒` で始まるときだけ受理する規則。
    fn postal_verdict(value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
        match value {
            CellValue::Text(text) if text.starts_with('〒') => Ok(CustomVerdict::Accepted),
            _ => Ok(CustomVerdict::rejected("〒 で始まらない")),
        }
    }

    /// 一括判定の呼び出し回数と、渡された値の並びを記録する標本の拡張型（要件 11.6）。
    ///
    /// 一括判定は `validate` を経由せずに判定する。**セルごとの判定が起きていないこと**を
    /// `validate` の呼び出し回数で観測できるようにするためである。
    struct BatchSpy {
        id: CustomTypeId,
        calls: Arc<AtomicUsize>,
        cells: Arc<AtomicUsize>,
        seen: Arc<Mutex<Vec<Vec<CellValue>>>>,
    }

    /// [`BatchSpy`] の観測点（呼び出し回数・セルごとの判定の回数・渡された値）。
    type BatchCounters = (
        Arc<AtomicUsize>,
        Arc<AtomicUsize>,
        Arc<Mutex<Vec<Vec<CellValue>>>>,
    );

    impl BatchSpy {
        /// 標本の実体と、その観測点を組み立てる。
        fn new(id: &str) -> (Self, BatchCounters) {
            let calls = Arc::new(AtomicUsize::new(0));
            let cells = Arc::new(AtomicUsize::new(0));
            let seen = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    id: CustomTypeId::new(id),
                    calls: calls.clone(),
                    cells: cells.clone(),
                    seen: seen.clone(),
                },
                (calls, cells, seen),
            )
        }
    }

    impl CustomType for BatchSpy {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            self.cells.fetch_add(1, Ordering::SeqCst);
            postal_verdict(value)
        }

        fn validate_batch(
            &self,
            values: &[CellValue],
            out: &mut dyn FnMut(usize, CustomVerdict),
        ) -> Result<(), CustomTypeFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen
                .lock()
                .expect("標本の記録が壊れていない")
                .push(values.to_vec());
            for (index, value) in values.iter().enumerate() {
                out(index, postal_verdict(value)?);
            }
            Ok(())
        }
    }

    /// 一括判定を上書きしない（既定実装のままの）標本の拡張型。
    struct DefaultBatch {
        id: CustomTypeId,
    }

    impl CustomType for DefaultBatch {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            postal_verdict(value)
        }
    }

    /// `fail` の値で一括判定を失敗させる標本の拡張型（要件 11.5）。
    struct FailingBatch {
        id: CustomTypeId,
        calls: Arc<AtomicUsize>,
        seen: Arc<Mutex<Vec<Vec<CellValue>>>>,
    }

    impl CustomType for FailingBatch {
        fn id(&self) -> &CustomTypeId {
            &self.id
        }

        fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
            if matches!(value, CellValue::Text(text) if text.as_str() == "fail") {
                return Err(CustomTypeFailure::new("応答がない"));
            }
            postal_verdict(value)
        }

        fn validate_batch(
            &self,
            values: &[CellValue],
            out: &mut dyn FnMut(usize, CustomVerdict),
        ) -> Result<(), CustomTypeFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen
                .lock()
                .expect("標本の記録が壊れていない")
                .push(values.to_vec());
            // 4.1 の契約: 失敗より前の判定はすべて `out` へ渡してから `Err` を返す。
            for (index, value) in values.iter().enumerate() {
                out(index, self.validate(value)?);
            }
            Ok(())
        }
    }

    /// 拡張型の列は、列ごとに 1 回の一括判定で判定する（tasks.md 5.4。要件 11.6, 10.4）。
    #[test]
    fn a_custom_column_is_evaluated_in_one_batch_per_column() {
        let (implementation, (calls, cells, seen)) = BatchSpy::new(POSTAL);
        let schema = compiled_with_custom(
            vec![custom_column("郵便番号", POSTAL, false)],
            Arc::new(implementation),
        );

        let mut doc = Document::new();
        let sheet = add_sheet(&mut doc, "宛先", &["郵便番号"]);
        for value in ["〒100-0001", "100-0002", "〒100-0003"] {
            add_row(&mut doc, sheet, vec![text(value)]);
        }

        let report = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
        assert_eq!(
            1,
            calls.load(Ordering::SeqCst),
            "一括判定が列ごとに 1 回呼ばれていない"
        );
        assert_eq!(
            0,
            cells.load(Ordering::SeqCst),
            "セルごとの判定が一括判定と別に走っている"
        );
        assert_eq!(
            vec![vec![
                text("〒100-0001"),
                text("100-0002"),
                text("〒100-0003"),
            ]],
            *seen.lock().expect("標本の記録が壊れていない"),
            "列の値が 1 回にまとめて渡されていない"
        );
        assert_eq!(
            1,
            report.total_violations(),
            "拒否した値だけが違反になっていない"
        );

        // 列指定の再検証も同じ一括経路を通る（要件 10.5）。
        let again = validate_columns(
            &doc,
            sheet,
            &schema,
            &[ColumnIndex::new(0)],
            &ValidationOptions::default(),
        );
        assert_eq!(
            2,
            calls.load(Ordering::SeqCst),
            "列指定で一括判定が呼ばれていない"
        );
        assert_eq!(1, again.total_violations());
    }

    /// 列指定の再検証は、指定した列の拡張型だけを一括判定する（要件 10.5, 11.6）。
    #[test]
    fn revalidating_selected_columns_batches_only_the_selected_custom_columns() {
        let (postal, (postal_calls, _, _)) = BatchSpy::new(POSTAL);
        let (codes, (code_calls, _, _)) = BatchSpy::new(FLAKY);
        let mut registry = TypeRegistry::new();
        registry
            .register(Arc::new(postal))
            .expect("標本の拡張型は登録できる");
        registry
            .register(Arc::new(codes))
            .expect("標本の拡張型は登録できる");
        let schema = compile_declaration(
            &Schema {
                columns: vec![
                    custom_column("郵便番号", POSTAL, false),
                    custom_column("商品コード", FLAKY, false),
                ],
            },
            &[],
            &registry,
        )
        .expect("標本の宣言はコンパイルできる");

        let mut doc = Document::new();
        let sheet = add_sheet(&mut doc, "宛先", &["郵便番号", "商品コード"]);
        add_row(&mut doc, sheet, vec![text("bad"), text("bad")]);
        add_row(&mut doc, sheet, vec![text("bad"), text("bad")]);

        let report = validate_columns(
            &doc,
            sheet,
            &schema,
            &[ColumnIndex::new(1)],
            &ValidationOptions::default(),
        );
        assert_eq!(
            0,
            postal_calls.load(Ordering::SeqCst),
            "指定していない拡張型の列が走査された"
        );
        assert_eq!(
            1,
            code_calls.load(Ordering::SeqCst),
            "指定した拡張型の列が一括判定されていない"
        );
        assert_eq!(2, report.total_violations(), "指定した列の違反が出ていない");
        assert!(
            report
                .violations()
                .iter()
                .all(|violation| violation.column().index() == 1),
            "指定していない列の違反が混ざっている"
        );
    }

    /// 値なしは一括判定へ渡さないが、必須の列では違反として報告する（4.2 の裁定。要件 11.6）。
    #[test]
    fn null_values_are_not_passed_to_the_batch_but_missing_is_reported() {
        let (implementation, (_, _, seen)) = BatchSpy::new(POSTAL);
        let schema = compiled_with_custom(
            vec![custom_column("郵便番号", POSTAL, true)],
            Arc::new(implementation),
        );

        let mut doc = Document::new();
        let sheet = add_sheet(&mut doc, "宛先", &["郵便番号"]);
        add_row(&mut doc, sheet, vec![text("〒100-0001")]);
        add_row(&mut doc, sheet, vec![CellValue::Null]);
        add_row(&mut doc, sheet, vec![text("〒100-0003")]);

        let report = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
        assert_eq!(
            vec![vec![text("〒100-0001"), text("〒100-0003")]],
            *seen.lock().expect("標本の記録が壊れていない"),
            "値なしが実装へ渡されている"
        );
        assert_eq!(
            1,
            report.total_violations(),
            "必須の列の値なしが違反になっていない"
        );
        assert!(
            matches!(
                report.violations()[0].reason(),
                ViolationReason::MissingValue { .. }
            ),
            "値なしの違反が必須の列の違反と違う形で報告されている"
        );
    }

    /// 一括判定を上書きした実装と既定実装は同じ結果を返す（design.md の不変条件）。
    #[test]
    fn an_overridden_batch_yields_the_same_report_as_the_default_batch() {
        let (implementation, (calls, _, _)) = BatchSpy::new(POSTAL);
        let overridden = compiled_with_custom(
            vec![custom_column("郵便番号", POSTAL, false)],
            Arc::new(implementation),
        );
        let default = compiled_with_custom(
            vec![custom_column("郵便番号", POSTAL, false)],
            Arc::new(DefaultBatch {
                id: CustomTypeId::new(POSTAL),
            }),
        );

        let mut doc = Document::new();
        let sheet = add_sheet(&mut doc, "宛先", &["郵便番号"]);
        for value in ["〒100-0001", "100-0002", "〒100-0003"] {
            add_row(&mut doc, sheet, vec![text(value)]);
        }

        let options = ValidationOptions::default();
        let batched = validate_sheet(&doc, sheet, &overridden, &options);
        let cell_by_cell = validate_sheet(&doc, sheet, &default, &options);
        assert_eq!(
            cell_by_cell, batched,
            "上書きした一括判定と既定実装の結果が違う"
        );
        assert_eq!(
            1,
            calls.load(Ordering::SeqCst),
            "上書きした一括判定が使われていない"
        );
    }

    /// 一括判定の失敗はその値だけの違反に閉じ込め、シート全体の検証を完走する
    /// （要件 11.5。4.1 の契約）。
    #[test]
    fn a_batch_failure_is_confined_to_the_failing_value_and_the_scan_completes() {
        let (implementation, calls, seen) = {
            let calls = Arc::new(AtomicUsize::new(0));
            let seen = Arc::new(Mutex::new(Vec::new()));
            (
                FailingBatch {
                    id: CustomTypeId::new(FLAKY),
                    calls: calls.clone(),
                    seen: seen.clone(),
                },
                calls,
                seen,
            )
        };
        let schema = compiled_with_custom(
            vec![
                custom_column("郵便番号", FLAKY, false),
                required_column("数量", TypeKind::Int),
            ],
            Arc::new(implementation),
        );

        let mut doc = Document::new();
        let sheet = add_sheet(&mut doc, "宛先", &["郵便番号", "数量"]);
        let values = [
            ("〒100-0001", int(1)),
            ("100-0002", int(2)),
            ("fail", int(3)),
            ("〒100-0004", CellValue::Null),
            ("100-0005", int(5)),
        ];
        let mut ids = Vec::new();
        for (postal, count) in values {
            ids.push(add_row(&mut doc, sheet, vec![text(postal), count]));
        }

        let report = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());

        // 失敗位置は「`out` が受け取った件数」（0 始まりで 2）である。失敗より前は通常の判定が
        // そのまま残り、失敗した値だけが `CustomFailed` になり、次の値からやり直す。
        assert_eq!(
            vec![
                vec![
                    text("〒100-0001"),
                    text("100-0002"),
                    text("fail"),
                    text("〒100-0004"),
                    text("100-0005"),
                ],
                vec![text("〒100-0004"), text("100-0005")],
            ],
            *seen.lock().expect("標本の記録が壊れていない"),
            "失敗の前後で一括判定へ渡す値が違う"
        );
        assert_eq!(
            2,
            calls.load(Ordering::SeqCst),
            "失敗した値の次からやり直していない"
        );

        // 失敗した値の前後（`k-1` は拒否、`k` は失敗、`k+1` は適合）と、失敗より後の行の
        // 判定が続いていることを固定する。
        assert_eq!(
            vec![(ids[1], 0), (ids[2], 0), (ids[3], 1), (ids[4], 0),],
            report
                .violations()
                .iter()
                .map(|violation| (
                    violation.row().expect("行に属する違反である"),
                    violation.column().index(),
                ))
                .collect::<Vec<_>>(),
            "失敗の前後の違反の並びが違う"
        );
        assert!(matches!(
            report.violations()[1].reason(),
            ViolationReason::CustomFailed { .. }
        ));
        assert!(matches!(
            report.violations()[2].reason(),
            ViolationReason::MissingValue { .. }
        ));
        assert_eq!(4, report.total_violations());
    }

    /// 拡張型の列が混ざっても、違反の並びは行 → 列添字のままである（要件 5.4, 5.5）。
    #[test]
    fn custom_columns_keep_the_row_then_column_order() {
        let (implementation, _) = BatchSpy::new(POSTAL);
        let schema = compiled_with_custom(
            vec![
                bounded_int_column("数量", 10),
                custom_column("郵便番号", POSTAL, false),
                column(
                    "名称",
                    TypeKind::Text,
                    Constraints {
                        max_length: Some(1),
                        ..Constraints::default()
                    },
                ),
            ],
            Arc::new(implementation),
        );

        let mut doc = Document::new();
        let sheet = add_sheet(&mut doc, "宛先", &["数量", "郵便番号", "名称"]);
        let first = add_row(&mut doc, sheet, vec![int(99), text("bad-a"), text("長い")]);
        let second = add_row(&mut doc, sheet, vec![int(99), text("bad-b"), text("長い")]);

        let report = validate_sheet(&doc, sheet, &schema, &ValidationOptions::default());
        assert_eq!(
            vec![
                (Some(first), 0, vec![]),
                (Some(first), 1, vec![]),
                (Some(first), 2, vec![]),
                (Some(second), 0, vec![]),
                (Some(second), 1, vec![]),
                (Some(second), 2, vec![]),
            ],
            positions(&report),
            "拡張型の列が混ざると違反の並びが崩れている"
        );
    }
}

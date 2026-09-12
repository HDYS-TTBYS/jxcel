//! テストとベンチが共有する標本の生成器（タスク 1.4。要件 10.6）。
//!
//! `tests/` 配下の各 `.rs` は独立したテストバイナリであり、あるテストクレートの
//! 関数を別のテストクレートから使う機構が無い。さらに `benches/` から `tests/` の
//! モジュールをそのまま取り込むこともできない。したがって共有する道具はここへ置き、
//! テストは `mod common;`、ベンチは `#[path = "../tests/common/mod.rs"] mod common;`
//! で取り込む（design「Directory Structure」の `tests/common/mod.rs`。tasks.md 1.4）。
//!
//! # 何を依存してよいか（tasks.md 1.4 の境界）
//!
//! 本モジュールは**上流のドキュメントモデルとセル値だけ**に依存する。
//! `schema-engine` のスキーマ宣言（`declaration`）は参照しない。宣言を必要とする
//! 標本（型つきの 30 列など）はこの時点では組み立てられず、生成器の上に群 9 が載せる。
//! ここが提供するのは「指定した行数・列数のシートと行の値」と「違反として仕込む値の
//! 割合」だけである。
//!
//! # 10 万行を O(n) で組み立てる理由
//!
//! [`Document`] は公開 API では行を 1 件ずつしか足せない（`add_row` +
//! `set_row_values`）。`set_row_values` は行を線形探索するため、10 万行では O(n²) に
//! なり分単位かかる（`document-format` の `tests/row_granular_diff.rs` と
//! `benches/large_document.rs` が同じ理由で同じ迂回路を使う）。本生成器は骨格の
//! 文書をパート集合へ取り出し、行エントリのバイト列だけを差し替えてから
//! `from_parts` の一括経路（`Sheet::extend_rows`）で復元する。これにより行数に
//! 対して線形である。
//!
//! # 決定性（要件 10.6 の標本の前提）
//!
//! セルの値と、違反として仕込む位置は、行添字と列添字だけから決まる。同じ引数なら
//! 常に同じ内容になる（行識別子は [`IdFactory`] の ULID であり発行時刻を含むため
//! 実行ごとに変わるが、標本の内容は行の並びと値だけで決まる）。
#![allow(dead_code)]

use document_format::parts::{to_parts, DocumentParts, ManifestEntry, ManifestPart};
use document_format::{
    to_json_bytes, CellValue, Document, DocumentFormat, DocumentFormatApi, EntryName,
    FormatVersion, IdFactory, RowId, SchemaPart, SheetId,
};

/// 標本の仕様（シート名・列名・行数）。
///
/// 列名は行オブジェクトのキー順になり、行数はそのまま標本の行数になる。既定の列名は
/// `c00` … `c<列数-1>` である（`document-format` の大量標本と同じ形）。
#[derive(Debug, Clone)]
pub struct SampleSpec {
    sheet_name: String,
    columns: Vec<String>,
    rows: usize,
}

impl SampleSpec {
    /// 行数と列数を指定する。列名は `c00` からの決定的な連番。
    pub fn new(rows: usize, columns: usize) -> Self {
        Self {
            sheet_name: "標本".to_owned(),
            columns: (0..columns).map(|index| format!("c{index:02}")).collect(),
            rows,
        }
    }

    /// 列名を明示する（列数は与えた列名の個数になる）。
    pub fn with_column_names(mut self, columns: Vec<String>) -> Self {
        self.columns = columns;
        self
    }

    /// シート名を変える。
    pub fn with_sheet_name(mut self, name: impl Into<String>) -> Self {
        self.sheet_name = name.into();
        self
    }

    /// 行数。
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// 列名（順序付き）。
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// 列数。
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// 総セル数（行数 × 列数）。
    pub fn cell_count(&self) -> usize {
        self.rows * self.columns.len()
    }
}

/// 全セルのうちどの位置を違反として仕込むかの決定的な規則（要件 10.6）。
///
/// 総セル数 `total` のうち `ratio` の割合（**切り捨て**）を、行優先の平坦添字で
/// 等間隔に選ぶ。件数は厳密に `count` であり、位置は実行ごとに変わらない
/// （5.6 の違反の上限・総件数の検証と 9.3 の決定性がこの性質に依る）。
#[derive(Debug, Clone, Copy)]
pub struct ViolationPlan {
    total: usize,
    count: usize,
}

impl ViolationPlan {
    /// 違反を 1 つも混ぜない規則。
    pub const fn none(total: usize) -> Self {
        Self { total, count: 0 }
    }

    /// 総セル数 `total` の `ratio` の割合（切り捨て）を違反にする規則。
    ///
    /// `ratio` は `0.0` 以上 `1.0` 以下へ丸める。有限でない値は 0 として扱う
    /// （panic しない）。
    pub fn ratio(total: usize, ratio: f64) -> Self {
        let bounded = if ratio.is_finite() {
            ratio.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let count = ((total as f64) * bounded) as usize;
        Self { total, count }
    }

    /// 仕込む違反の件数。
    pub const fn count(&self) -> usize {
        self.count
    }

    /// 行優先の平坦添字 `index` を違反として仕込むか。
    ///
    /// 添字 `i` の選定は `floor((i+1)*count/total) > floor(i*count/total)` であり、
    /// `0..total` を走査するとちょうど `count` 件が選ばれる（等間隔）。
    pub fn is_violation(&self, index: usize) -> bool {
        if self.count == 0 || self.total == 0 || index >= self.total {
            return false;
        }
        (index + 1) * self.count / self.total > index * self.count / self.total
    }
}

/// 生成された標本（文書・対象シート・行識別子・違反の規則）。
///
/// 対象シートのルートスキーマは空である（宣言を必要とする標本は本モジュールの
/// 境界外であり、群 9 が [`Sample::document_mut`] から差し替える）。
pub struct Sample {
    document: Document,
    sheet: SheetId,
    row_ids: Vec<RowId>,
    plan: ViolationPlan,
}

impl Sample {
    /// 標本の文書。
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// 標本の文書を書き換える（ルートスキーマの差し替えなど）。
    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    /// 標本の対象シートの識別子。
    pub fn sheet(&self) -> SheetId {
        self.sheet
    }

    /// 行識別子（行順）。違反の行を突き合わせるために使う。
    pub fn row_ids(&self) -> &[RowId] {
        &self.row_ids
    }

    /// 行数。
    pub fn row_count(&self) -> usize {
        self.row_ids.len()
    }

    /// 列数。
    pub fn column_count(&self) -> usize {
        self.document.sheets()[0].columns().len()
    }

    /// 違反として仕込んだセルの総数。
    pub fn violating_cells(&self) -> usize {
        self.plan.count()
    }

    /// 行添字と列添字のセルを違反として仕込んだか。
    pub fn violation_at(&self, row: usize, column: usize) -> bool {
        self.plan.is_violation(row * self.column_count() + column)
    }

    /// 行優先のセル値を複製して返す（決定性の比較用）。
    pub fn row_values(&self) -> Vec<Vec<CellValue>> {
        self.document.sheets()[0]
            .rows()
            .iter()
            .map(|row| row.values().to_vec())
            .collect()
    }

    /// 文書と対象シートの識別子を取り出す。
    pub fn into_document(self) -> (Document, SheetId) {
        (self.document, self.sheet)
    }
}

/// 違反を混ぜずに標本を生成する。
///
/// `values` は行添字・列添字（どちらも 0 始まり）からセル値を決める規則である。
pub fn sample<F>(spec: &SampleSpec, values: F) -> Sample
where
    F: Fn(usize, usize) -> CellValue,
{
    build(
        spec,
        ViolationPlan::none(spec.cell_count()),
        |row, column| values(row, column),
    )
}

/// 指定した割合で違反になる値を混ぜて標本を生成する（要件 10.6）。
///
/// どのセルを違反として仕込むかは [`ViolationPlan`] が決め、仕込む値は `violations` が
/// 決める。通常のセルは `values` が決める。仕込んだ件数は
/// [`Sample::violating_cells`] で取得できる。
pub fn sample_with_violations<F, G>(
    spec: &SampleSpec,
    values: F,
    ratio: f64,
    violations: G,
) -> Sample
where
    F: Fn(usize, usize) -> CellValue,
    G: Fn(usize, usize) -> CellValue,
{
    let plan = ViolationPlan::ratio(spec.cell_count(), ratio);
    let columns = spec.column_count();
    build(spec, plan, |row, column| {
        if plan.is_violation(row * columns + column) {
            violations(row, column)
        } else {
            values(row, column)
        }
    })
}

/// 標本の文書を O(n) で組み立てる（モジュール docs「10 万行を O(n) で組み立てる理由」）。
fn build<F>(spec: &SampleSpec, plan: ViolationPlan, value_at: F) -> Sample
where
    F: Fn(usize, usize) -> CellValue,
{
    // 0 行の骨格を公開 API で作り、行エントリだけを差し替える。
    let mut skeleton = Document::new();
    let sheet = skeleton.add_sheet(spec.sheet_name.clone());
    skeleton
        .set_sheet_columns(sheet, spec.columns.clone())
        .expect("標本のシートは実在する");
    skeleton
        .set_root_schema(sheet, SchemaPart::empty())
        .expect("標本のシートは実在する");

    let parts = to_parts(&skeleton).expect("骨格はパート集合へ取り出せる");
    let version = parts.format_version();
    let mut entries: Vec<(EntryName, Vec<u8>)> = parts
        .iter()
        .map(|part| (part.name, part.bytes.clone()))
        .collect();

    let (rows_entry, rows_bytes, row_ids) = encode_rows(sheet, spec, value_at);
    let slot = entries
        .iter()
        .position(|(name, _)| *name == rows_entry)
        .expect("骨格は標本のシートの行エントリを持つ");
    entries[slot].1 = rows_bytes;

    let entries = with_rebuilt_manifest(version, entries);
    let parts = DocumentParts::from_entries(entries).expect("行を差し込んだ集合は妥当");
    let document = DocumentFormat::new()
        .from_parts(&parts)
        .expect("行を差し込んだ集合は復元できる");

    Sample {
        document,
        sheet,
        row_ids,
        plan,
    }
}

/// 1 シート分の行データを `sheets/<sheet-ulid>.jsonl` のバイト列へ符号化し、
/// 発行した行識別子を返す。
///
/// セル値の符号化は [`to_json_bytes`]（value 層の単一の源）に任せる。列名は
/// 利用者が書く任意のテキストでありうるため、[`push_json_string`] で正しく逃がす。
fn encode_rows<F>(
    sheet: SheetId,
    spec: &SampleSpec,
    value_at: F,
) -> (EntryName, Vec<u8>, Vec<RowId>)
where
    F: Fn(usize, usize) -> CellValue,
{
    let entry = EntryName::Rows { sheet };
    let location = entry.to_string();
    let columns = spec.columns.as_slice();
    let mut ids = IdFactory::new();
    let mut row_ids = Vec::with_capacity(spec.rows);
    let mut out = Vec::with_capacity(spec.rows * (40 + columns.len() * 12));
    for row in 0..spec.rows {
        let id = ids.new_row_id();
        row_ids.push(id);
        out.extend_from_slice(b"{\"$id\":");
        push_json_string(&mut out, &id.to_string());
        for (column, name) in columns.iter().enumerate() {
            out.push(b',');
            push_json_string(&mut out, name);
            out.push(b':');
            let bytes = to_json_bytes(&value_at(row, column), &location)
                .expect("標本のセル値は符号化できる");
            out.extend_from_slice(&bytes);
        }
        out.extend_from_slice(b"}\n");
    }
    (entry, out, row_ids)
}

/// JSON 文字列リテラル（前後の `"` を含む）を `out` へ書く。
///
/// 依存を増やさないため本モジュールで完結させる（標本は上流のドキュメントモデルと
/// セル値だけに依存する）。`"`・`\`・制御文字を逃がし、それ以外は UTF-8 のまま書く。
fn push_json_string(out: &mut Vec<u8>, text: &str) {
    out.push(b'"');
    for character in text.chars() {
        match character {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\r' => out.extend_from_slice(b"\\r"),
            '\t' => out.extend_from_slice(b"\\t"),
            '\u{08}' => out.extend_from_slice(b"\\b"),
            '\u{0c}' => out.extend_from_slice(b"\\f"),
            control if (control as u32) < 0x20 => {
                out.extend_from_slice(format!("\\u{:04x}", control as u32).as_bytes());
            }
            other => {
                let mut buffer = [0u8; 4];
                out.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    out.push(b'"');
}

/// 索引（`manifest.json`）を実体から組み直したエントリ集合を返す。
///
/// 行エントリのバイト列を差し替えると索引のダイジェストが古くなるため、復元の前に
/// 組み直す（`from_parts` はダイジェストを照合する）。`document-format` の
/// `tests/common/mod.rs` にある同名の補助と同じ手順である。
fn with_rebuilt_manifest(
    version: FormatVersion,
    mut entries: Vec<(EntryName, Vec<u8>)>,
) -> Vec<(EntryName, Vec<u8>)> {
    entries.retain(|(name, _)| *name != EntryName::Manifest);
    let index: Vec<ManifestEntry> = entries
        .iter()
        .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
        .collect();
    let manifest = ManifestPart::new(version, index)
        .expect("標本の索引は妥当")
        .to_json_bytes()
        .expect("索引は符号化できる");
    entries.push((EntryName::Manifest, manifest));
    entries
}

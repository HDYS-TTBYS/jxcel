//! テストとベンチが共有する標本の生成器（タスク 1.4。要件 8.1）。
//!
//! `tests/` 配下の各 `.rs` は独立したテストバイナリであり、あるテストクレートの関数を
//! 別のテストクレートから使う機構が無い。さらに `benches/` から `tests/` のモジュールを
//! そのまま取り込むこともできない。したがって共有する道具はここへ置き、テストは
//! `mod common;`、ベンチは `#[path = "../tests/common/mod.rs"] mod common;` で取り込む
//! （design「File Structure Plan」の `tests/common/mod.rs`。tasks.md 1.4）。
//!
//! 置くのは次の 3 種である:
//!
//! 1. **標本の生成器**（[`SampleSpec`] / [`Sample`] / [`sample`] / [`value_at`]）。指定した
//!    行数・列数のシートを持つ [`Document`] を組み立てる。
//! 2. **保存と読み込みの往復に使う一時ディレクトリ**（[`Scratch`]）。
//! 3. **上流の公開面を起動する実装**（[`api`]）。
//!
//! # 上流の公開 API だけを使う（tasks.md 1.4 の境界）
//!
//! 本モジュールが `document_format` から取り込むのは、**根の公開名**と
//! **`parts` の公開の再輸出**だけである（冒頭の `use` 宣言が全てである）:
//!
//! * [`document_format::parts`] の公開の再輸出:
//!   [`DocumentParts`]（[`DocumentParts::from_entries`]）・[`ManifestEntry`]・[`ManifestPart`]
//! * 根の公開名: [`DocumentFormat`] / [`DocumentFormatApi`]（`to_parts` / `from_parts` /
//!   `save` / `open`）・[`Document`] / [`CellValue`] / [`NestedValue`] / [`SchemaPart`] /
//!   [`EntryName`] / [`FormatVersion`] / [`IdFactory`] / [`RowId`] / [`Sheet`] / [`SheetId`] /
//!   [`to_json_bytes`]
//!
//! **形式の側の内部モジュールには触れない**: `document_format::model::*` のような層のパスを
//! 直接綴らず、`pub(crate)` の行の構築 API（`Sheet::push_row` / `extend_rows` 等）を使わない。
//! したがって本生成器は、形式の側の内部構造が変わっても公開面が同じなら影響を受けない。
//!
//! # 10 万行を O(n) で組み立てる理由
//!
//! [`Document`] は公開 API では行を 1 件ずつしか足せず（`add_row` + `set_row_values`）、
//! `set_row_values` は行を線形探索するため 10 万行では O(n²) になり分単位かかる
//! （`document-format` の `tests/row_granular_diff.rs` と `benches/large_document.rs`、
//! `schema-engine` の `tests/common/mod.rs` が同じ理由で同じ迂回路を使う）。本生成器は
//! **0 行の骨格**を公開 API で作り、パート集合へ取り出し（[`DocumentFormatApi::to_parts`]）、
//! 行エントリのバイト列だけを差し替えてから一括の復元経路（[`DocumentFormatApi::from_parts`]）
//! で戻す。`from_parts` は復号済みの行を一括で入れるため、行数に対して線形である。
//! 行エントリの符号化（[`to_json_bytes`]）も 1 行 1 走査の線形であり、行数を増やしても
//! 二乗の項は現れない。
//!
//! # 標本の内容（「現実に寄せる」）
//!
//! 列名は [`COLUMN_TYPES`] の 30 列（`id` / `name` / `amount` / …）であり、要件 8.1 の
//! 10 万行 × 30 列の標本がそのままこの表を使う。**列ごとに値の型が決まる**（同じ列は常に
//! 同じ型の値を持つ）ため、整数・テキスト・10 進数の文字列・浮動小数・真偽・入れ子（配列）
//! が混ざり、テキストと 10 進数の列には**まばらな欠測**（`Null`）が現れる。値は
//! **行添字と列添字だけから決まる**（[`value_at`]）。
//!
//! # 決定性の主張の範囲
//!
//! 同じ引数なら**形（行数・列数・列名）と値**は常に同じになる。**行識別子（ULID）と
//! バイト列の同一性は主張しない**: 行識別子は公開の [`IdFactory`] が発行時刻を含めて
//! 発行するため、2 回の生成で異なる（`verification.md`「標本が識別子（ULID）を含むなら
//! 『同一バイト列』を主張しない」）。
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use document_format::parts::{DocumentParts, ManifestEntry, ManifestPart};
use document_format::{
    to_json_bytes, CellValue, Document, DocumentFormat, DocumentFormatApi, EntryName,
    FormatVersion, IdFactory, NestedValue, RowId, SchemaPart, Sheet, SheetId,
};

/// 列の型（標本の値の型を現実の表に寄せる。tasks.md 1.4）。
///
/// 値の型は**列の位置**で決まる（同じ列は常に同じ型の値を持つ）。行ごとに変わるのは値だけ
/// である。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnKind {
    /// 64 ビット整数（識別子・数量）。
    Int,
    /// テキスト（名前・区分・備考）。
    Text,
    /// 10 進数の文字列（単価・金額）。
    Decimal,
    /// 浮動小数（率・点数）。
    Float,
    /// 真偽（有効）。
    Bool,
    /// 配列（タグ）。[`CellValue::Nested`] 変種を混ぜるために持つ。
    Tags,
}

/// 標本の列名と型（30 列。要件 8.1 の 10 万行 × 30 列がそのまま使う）。
///
/// 別の列数が指定されたときは、この表を先頭から使う（`30` を超える分は `_<添字>` を
/// 足した名前で表を巡回する）。列名は行オブジェクトのキー順になる。
const COLUMN_TYPES: [(&str, ColumnKind); 30] = [
    ("id", ColumnKind::Int),
    ("name", ColumnKind::Text),
    ("category", ColumnKind::Text),
    ("quantity", ColumnKind::Int),
    ("unit_price", ColumnKind::Decimal),
    ("amount", ColumnKind::Decimal),
    ("tax_rate", ColumnKind::Float),
    ("discount", ColumnKind::Float),
    ("active", ColumnKind::Bool),
    ("note", ColumnKind::Text),
    ("created_at", ColumnKind::Text),
    ("updated_at", ColumnKind::Text),
    ("owner", ColumnKind::Text),
    ("tags", ColumnKind::Tags),
    ("memo", ColumnKind::Text),
    ("currency", ColumnKind::Text),
    ("region", ColumnKind::Text),
    ("channel", ColumnKind::Text),
    ("status", ColumnKind::Int),
    ("priority", ColumnKind::Int),
    ("due_date", ColumnKind::Text),
    ("reference", ColumnKind::Text),
    ("contact", ColumnKind::Text),
    ("phone", ColumnKind::Text),
    ("email", ColumnKind::Text),
    ("address", ColumnKind::Text),
    ("score", ColumnKind::Float),
    ("rank", ColumnKind::Int),
    ("comment", ColumnKind::Text),
    ("revision", ColumnKind::Int),
];

/// 標本の仕様（シート名・列名・行数）。
///
/// 列名は行オブジェクトのキー順になり、行数はそのまま標本の行数になる。既定の列名は
/// [`COLUMN_TYPES`] の 30 列であり、列数が 30 を超える分は `_<添字>` を足して一意にする。
#[derive(Debug, Clone)]
pub struct SampleSpec {
    sheet_name: String,
    columns: Vec<String>,
    rows: usize,
}

impl SampleSpec {
    /// 行数と列数を指定する。列名は [`COLUMN_TYPES`] から決まる。
    pub fn new(rows: usize, columns: usize) -> Self {
        Self {
            sheet_name: "標本".to_owned(),
            columns: (0..columns).map(column_name).collect(),
            rows,
        }
    }

    /// 列名を明示する（列数は与えた列名の個数になる）。
    ///
    /// 値の型は列名ではなく**列の位置**で決まる（[`value_at`]）。
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
}

/// 生成された標本（文書と対象シート）。
///
/// 文書は上流の全検証を通過している（[`DocumentFormatApi::from_parts`] が返したものだけを
/// 保持する）。
#[derive(Debug)]
pub struct Sample {
    document: Document,
    sheet: SheetId,
}

impl Sample {
    /// 標本の文書。
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// 標本の対象シート。
    pub fn sheet(&self) -> &Sheet {
        self.document
            .sheet_by_id(self.sheet)
            .expect("標本のシートは文書に含まれる")
    }

    /// 標本の対象シートの識別子。
    pub fn sheet_id(&self) -> SheetId {
        self.sheet
    }

    /// 行数。
    pub fn row_count(&self) -> usize {
        self.sheet().rows().len()
    }

    /// 列数。
    pub fn column_count(&self) -> usize {
        self.sheet().columns().len()
    }

    /// 行識別子（行順）。
    ///
    /// 値の行添字との対応を取るために使う（識別子そのものは実行ごとに変わる。
    /// モジュール docs「決定性の主張の範囲」）。
    pub fn row_ids(&self) -> Vec<RowId> {
        self.sheet().rows().iter().map(|row| row.id()).collect()
    }

    /// 行優先のセル値を複製して返す（決定性の比較用）。
    pub fn row_values(&self) -> Vec<Vec<CellValue>> {
        self.sheet()
            .rows()
            .iter()
            .map(|row| row.values().to_vec())
            .collect()
    }

    /// 文書を取り出す。
    pub fn into_document(self) -> Document {
        self.document
    }
}

/// 仕様どおりの標本を生成する（tasks.md 1.4）。
///
/// 行数 × 列数のセルを [`value_at`] の規則で埋めた 1 シートの文書を、公開 API の一括経路
/// だけで O(行数) で組み立てる（モジュール docs「10 万行を O(n) で組み立てる理由」）。
pub fn sample(spec: &SampleSpec) -> Sample {
    // 0 行の骨格を公開 API で作り、行エントリだけを差し替える。
    let mut skeleton = Document::new();
    let sheet = skeleton.add_sheet(spec.sheet_name.clone());
    skeleton
        .set_sheet_columns(sheet, spec.columns.clone())
        .expect("標本のシートは実在する");
    skeleton
        .set_root_schema(sheet, SchemaPart::empty())
        .expect("標本のシートは実在する");

    let parts = api()
        .to_parts(&skeleton)
        .expect("骨格はパート集合へ取り出せる");
    let version = parts.format_version();
    let mut entries: Vec<(EntryName, Vec<u8>)> = parts
        .iter()
        .map(|part| (part.name, part.bytes.clone()))
        .collect();

    let (rows_entry, rows_bytes) = encode_rows(sheet, spec);
    let slot = entries
        .iter()
        .position(|(name, _)| *name == rows_entry)
        .expect("骨格は標本のシートの行エントリを持つ");
    entries[slot].1 = rows_bytes;

    let entries = with_rebuilt_manifest(version, entries);
    let parts = DocumentParts::from_entries(entries).expect("行を差し込んだ集合は妥当");
    let document = api()
        .from_parts(&parts)
        .expect("行を差し込んだ集合は復元できる");

    Sample { document, sheet }
}

/// セル値の規則（行添字と列添字だけから決まる。tasks.md 1.4）。
///
/// 列の型（[`COLUMN_TYPES`]）ごとに現実に寄せた値を作る。テキストと 10 進数の列には
/// まばらな欠測（[`CellValue::Null`]）が現れる（約 100 セルに 1 つ。位置だけで決まる）。
/// 同じ引数なら常に同じ値を返す。
pub fn value_at(row: usize, column: usize) -> CellValue {
    if is_missing(row, column) {
        return CellValue::Null;
    }
    match column_kind(column) {
        ColumnKind::Int => CellValue::Int(row as i64 * 1_000 + column as i64),
        ColumnKind::Text => {
            let name = column_base_name(column);
            CellValue::Text(format!("{name}-{row}"))
        }
        ColumnKind::Decimal => CellValue::Decimal(format!("{row}.{column:02}")),
        ColumnKind::Float => CellValue::float(row as f64 / 8.0 + column as f64 / 100.0),
        ColumnKind::Bool => CellValue::Bool((row + column) % 3 != 0),
        ColumnKind::Tags => CellValue::Nested(NestedValue::Array(vec![
            CellValue::Text(format!("tag{}", row % 7)),
            CellValue::Text(format!("tag{}", (row + column) % 5)),
        ])),
    }
}

/// 上流の公開面を起動する実装（design はトレイトのみを指定するため、無状態の具象型を使う）。
pub fn api() -> DocumentFormat {
    DocumentFormat::new()
}

/// テスト専用の一時ディレクトリ（`Drop` で必ず削除する）。
///
/// テストとベンチは並行に走るため、プロセス ID・単調カウンタ・起動時刻で一意にする
/// （同じ名前の使い回しは、前回の異常終了で残った内容を混ぜる）。`Drop` は panic による
/// 巻き戻しでも走るので、失敗したテストの一時ファイルも残らない。場所は
/// [`std::env::temp_dir`] であり、外部クレートを足さない（tasks.md 1.4）。
pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// `tag` で識別できる一時ディレクトリを作る。
    pub fn new(tag: &str) -> Self {
        static SEQUENCE: AtomicU32 = AtomicU32::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("起動時刻は UNIX 紀元より後である")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "document_session_{tag}_{}_{sequence}_{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
        Self { path }
    }

    /// 一時ディレクトリの位置。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 一時ディレクトリ直下のファイルの位置。
    pub fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // 既に消えていても失敗しない（多重削除・異常終了の後始末）。
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 列の型（表の巡回。列数が 30 を超えても破綻しない）。
fn column_kind(column: usize) -> ColumnKind {
    COLUMN_TYPES[column % COLUMN_TYPES.len()].1
}

/// 列名（`index` が 30 列を超える場合は連番を足して一意にする）。
fn column_name(index: usize) -> String {
    if index < COLUMN_TYPES.len() {
        COLUMN_TYPES[index].0.to_owned()
    } else {
        format!("{}_{index}", column_base_name(index))
    }
}

/// 列の基本名（`_<添字>` を足す前の名前）。
fn column_base_name(index: usize) -> &'static str {
    COLUMN_TYPES[index % COLUMN_TYPES.len()].0
}

/// セルを欠測（[`CellValue::Null`]）にするか。
///
/// テキストと 10 進数の列だけを対象にする（識別子・数量・真偽が欠ける標本は現実に合わない）。
/// 位置だけで決まるため決定的である。
fn is_missing(row: usize, column: usize) -> bool {
    matches!(column_kind(column), ColumnKind::Text | ColumnKind::Decimal)
        && (row * 31 + column * 7 + 3) % 97 == 0
}

/// 標本の行エントリ（`sheets/<sheet-ulid>.jsonl`）を符号化する。
///
/// 1 行 = 1 テキスト行（LF 終端）の NDJSON であり、`$id` を先頭キー、続いて列順に
/// [`value_at`] の値（[`to_json_bytes`] = 値層の単一の源で符号化）を書く。行識別子は
/// 公開の [`IdFactory`] から発行する（**実行ごとに変わる**。モジュール docs「決定性の
/// 主張の範囲」）。構築は `Vec<u8>` への追記だけで行う（O(行数)）。
fn encode_rows(sheet: SheetId, spec: &SampleSpec) -> (EntryName, Vec<u8>) {
    let entry = EntryName::Rows { sheet };
    let location = entry.to_string();
    let columns = spec.columns.as_slice();
    let mut ids = IdFactory::new();
    let mut out = Vec::with_capacity(spec.rows * (48 + columns.len() * 16));
    for row in 0..spec.rows {
        out.extend_from_slice(b"{\"$id\":");
        push_json_string(&mut out, &ids.new_row_id().to_string());
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
    (entry, out)
}

/// JSON 文字列リテラル（前後の `"` を含む）を `out` へ書く。
///
/// 依存を増やさないため本モジュールで完結させる（`schema-engine` の
/// `tests/common/mod.rs` にある同名の補助と同じ規律）。列名は利用者が書く任意のテキストで
/// ありうるため、`"`・`\`・制御文字を逃がし、それ以外は UTF-8 のまま書く。
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
/// 組み直す（[`DocumentFormatApi::from_parts`] はダイジェストを照合する）。`document-format`
/// と `schema-engine` の `tests/common/mod.rs` にある同名の補助と同じ手順である。
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

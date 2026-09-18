//! シート別行データパート（`sheets/<sheet-ulid>.jsonl`）の符号化・復号（タスク 4.5。
//! 要件 2.3, 3.4, 3.5。design「Container Entry Layout」/「Components and Interfaces」の
//! RowsCodec）。
//!
//! 各シートは**独立したエントリ** `sheets/<sheet-ulid>.jsonl` を持つ（要件 2.3: 各シートの
//! データは独立して読み書きできる）。1 データ行 = 1 テキスト行の NDJSON であり、行の
//! フレーミング（LF 終端・空行拒否・行番号付き診断）は [`crate::json::write_ndjson`] /
//! [`crate::json::read_ndjson`]（タスク 3.3）が単一の源である。本モジュールはその上で
//! **行オブジェクトの中身**（列名 → セル値）と [`Row`] の相互変換だけを担う:
//!
//! | 方向 | 入力 | 出力 |
//! |------|------|------|
//! | [`RowsCodec::encode`] | シート識別子 + 列名の順序付きリスト + 行 | エントリ名（`sheets/<ulid>.jsonl`）+ 確定形のバイト列 |
//! | [`RowsCodec::decode`] | エントリ名 + エントリの実バイト列 | [`SheetRows`]（シート識別子 + ファイルが定めた列順 + 行） |
//!
//! # 行オブジェクトの形（本クレートが所有する確定形）
//!
//! 1 行は**列名をキーとするフラットなオブジェクト**である（位置配列ではない。要件 2 の
//! 「専用ツールなしでも内容を確認できる」ため）:
//!
//! ```json
//! {"$id":"01K4ANRRG004HMASW9NF6YY091","amount":1234,"note":"日本語"}
//! ```
//!
//! - 行識別子は予約キー `$id` に 26 文字 ULID テキストとして書く（要件 1.5 の並び替えが
//!   行識別子を保持することと、要件 8.1 の往復同一性のため。行の他の永続化先は無い）。
//!   予約キーの認識は**完全一致**のみである。
//! - キー順は `$id` を先頭、続いて呼び出し元が与えた列順とする（design
//!   `DeterministicJson` の「スキーマ由来の動的な列集合は、スキーマが定める列順序で
//!   明示的に整列する」をそのまま受ける。列順の供給は `schema-engine` の責務であり、
//!   本モジュールはスキーマの内容を一切解釈しない）。
//! - **`$` 始まりの列名は先頭に `$` を 1 つ足して書く**（`$foo` → `$$foo`）。読み側は
//!   `$$` で始まるキーの先頭の `$` を 1 つ剥がす。予約キー `$id` との衝突を避けつつ
//!   **スキーマの列名に一切の制約を課さない**ための措置であり（本クレートは列名の
//!   妥当性を判断できない）、[`crate::value`] の `$` 始まりキー二重化規約と同じ思想で
//!   ある。この写像は単射（`$` 始まりは `$` 始まりへ、それ以外はそのまま）なので、
//!   列名と wire キーは 1 対 1 に対応し、`$id` という列名は wire では `$$id` になる。
//! - **1 シート内の全行は同一のキー列**（`$id` + 同一列集合 + 同一順序）でなければ
//!   ならない。逸脱は構造的不整合として拒否する（下記「エラー対応」。黙って落とす・
//!   並べ替える・穴埋めすることはしない）。
//! - 復号は `$id` の**位置**を要求しない（ファイル自身のキー順が列順を定め、`$id` は
//!   完全一致するキーとして行識別子に取り出される）。ただしその位置は行間で一致して
//!   いなければならない（キー列の一致検査が位置も見る）。
//!
//! # 復号が受理するキー = 書き手の像
//!
//! 復号が受理するキーは、上の符号化規則が作る形だけである（**書き手の像**）:
//! 完全一致の `$id`、`$` で始まらないキー（列名そのもの）、`$$` で始まるキー（`$` 始まりの
//! 列名のエスケープ形）の 3 形に限る。単一 `$` で始まり `$id` でも `$$` でもないキー
//! （例 `$x`）は像の外であり、**正規化せず拒否**する（[`crate::entry_name`] が
//! 「サニタイズではなく拒否」を採っているのと同じ規約）。
//!
//! 像の外のキーを受理してはならない理由は 2 つある: (1) それを列名として読むと `$x` と
//! `$$x`（列名 `$x`）が同居する入力で**列名が重複**し、開いた時点で検出できない破綻が
//! 残る（本クレートは行識別子の重複や行間のキー列不一致も同じ理由で復号時に拒否する）。
//! (2) 像の定義（`encode` の像と `decode` の定義域）が一致していれば、
//! **`encode(decode(W)) == W` が本クレートが書いた形式の入力 `W` に対して全域**になる
//! （復号 → 再符号化でバイト列が変わらない入力に隙間が無い。像外キーを通すと、復号は
//! できるが再符号化できない入力が生まれてしまう）。
//!
//! # 順序（要件 1.5 / 3.5）
//!
//! **行順は与えられた順序のまま**である（ULID 昇順・追加順・辞書順のいずれにも
//! 並べ替えない）。[`crate::model::Sheet::rows`] の保持順（並び替え後の順序を含む）を
//! そのまま [`write_ndjson`] へ渡すため、並び替えの差分は行 ID を変えずにテキスト行の
//! 移動として現れる（design「RowsCodec」）。列の順序も同様に、呼び出し元が与えた順序を
//! そのまま使う。
//!
//! # 決定性（要件 3.6）
//!
//! 符号化は入力の**純関数**である: 同じ入力から常に同じバイト列になり、保存時刻・実行
//! 環境・内部処理順に由来する値を一切含まない。セル値の wire 表現（`-0.0` の `0`
//! 正規化と NaN / Infinity の遮断を含む）は [`crate::value::to_json_bytes`] が**単一の
//! 源**であり、本モジュールは再実装しない（検査・符号化済みの原文を
//! [`RawValue`] の verbatim 経路で差し込む。[`crate::parts::schema_codec`] が不透明
//! ペイロードを差し込むのと同じ機構）。
//!
//! # エラー対応
//!
//! 読み込みの失敗はすべて読み込み全体の中止であり、部分的な結果を返さない（要件 5.4 系）。
//! design のエラー表に本モジュール固有の変種は無いため、コンテンツ解析の失敗はコンテナ
//! 不正 [`DocumentError::InvalidContainer`] へ写す（[`crate::value`] /
//! [`crate::json::determinism`] / [`crate::parts::manifest`] と同じ規約）。`entry` は
//! **対象エントリ名**（`sheets/<ulid>.jsonl`）で始まり、行に帰属する失敗は 1 始まりの
//! 行番号を、セルに帰属する失敗は列名まで含む（[`crate::json::read_ndjson`] の
//! `<location> line <n>: <理由>` 規約をそのまま延長する）。
//!
//! | 失敗 | 返す変種と文脈 |
//! |------|----------------|
//! | エントリ名が `sheets/<ulid>.jsonl` 形でない | [`DocumentError::InvalidContainer`]（`entry` = 与えられたエントリ名 + 理由） |
//! | 空行・JSON として不正な行・行オブジェクトでない行 | 同上（`<entry> line <n>: <理由>`） |
//! | 行間でキー列が一致しない（欠落・順序違い・余分） | 同上（`<entry> line <n>: 期待したキー列と実際のキー列`） |
//! | 同じ行オブジェクト内でキーが重複している | 同上（`<entry> line <n>: ...`） |
//! | キーが書き手の像の外にある（単一 `$` 始まりで `$id` でも `$$` でもない） | 同上（同上） |
//! | エスケープ解除後の列名が重複している | 同上（同上。像の検査を通ったキーは単射に写るため通常は起こらないが、二重の防御として明示的に守る） |
//! | `$id` が無い / 文字列でない / ULID でない | 同上（同上） |
//! | 同一ファイル内で行識別子が重複している | 同上（同上。文書全体の一意性検証はタスク 4.6 の担当であり、本モジュールは 1 ファイル内の自己矛盾だけを閉じる） |
//! | セルの値が `value` 層で拒否された（i64 範囲外整数・非有限値など） | 同上（`<entry> line <n>: column <列名>: <理由>`） |
//! | 書き手側の programming error | [`RowsEncodeError`]（最小のローカル型。下記） |
//!
//! ## i64 範囲外整数の門
//!
//! [`crate::json::read_ndjson`] は汎用コーデックであり、整数リテラルの範囲検査をしない
//! （`-9223372036854775809` のようなリテラルを黙って浮動小数へ回送する）。
//! 本モジュールは行の各セルの**原文**（[`RawValue`] が捕捉したソーステキスト）を
//! [`crate::value::from_json_bytes`] へ通すため、範囲検査は value 層の単一の門で効く
//! （門を二重に持たない）。
//!
//! ## ローカルエラー型 [`RowsEncodeError`]
//!
//! **符号化**の入力が行データの前提を満たさない場合は、design のエラー表に対応する変種が
//! 無い**呼び出し元の programming error** である（`model` の
//! [`UnknownSheet`](crate::model::UnknownSheet) /
//! [`ReorderError`](crate::model::ReorderError) と同じ前例）。したがって `panic` ではなく
//! 最小のローカル型で報告し、失敗時はバイト列を 1 バイトも返さない:
//!
//! - `ValueCountMismatch`: 行の値の個数が列数と一致しない。本クレートはスキーマを
//!   解釈しないため、個数の不一致を意味のある形で解釈できない（黙って穴埋め・切詰めを
//!   しない）。
//! - `DuplicateColumn`: 列名が重複している。同名のキーが 1 つの行オブジェクトに 2 回
//!   現れる wire は復号側が自己矛盾として拒否するため、**復号できないエントリを
//!   書かない**（書き手と読み手で同じ前提を共有する）。
//! - `Document`: 書き出し自体の失敗（[`crate::value::to_json_bytes`] による非有限値の
//!   遮断など）。[`DocumentError`] をそのまま運ぶ。
//!
//! # 依存方向
//!
//! `Ids / Value / EntryName → Model → Json → Parts → Container → Api` の一方向
//! （design「Architecture Integration」）。本モジュールは [`crate::json`] /
//! [`crate::value`] / [`crate::model`] / [`crate::error`] / [`crate::entry_name`] に依存し、
//! `container` には依存しない（ZIP を一切知らない）。

use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt;

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;
use serde_json::value::RawValue;
use thiserror::Error;

use crate::entry_name::EntryName;
use crate::error::DocumentError;
use crate::ids::{RowId, SheetId};
use crate::json::{read_ndjson, write_ndjson};
use crate::model::Row;
use crate::value::{from_json_bytes, to_json_bytes, CellValue};

/// 行オブジェクトの予約キー（行識別子を 26 文字 ULID テキストで持つ）。
const ROW_ID_KEY: &str = "$id";

/// シート別行データの符号化・復号（design「Components and Interfaces」の RowsCodec。
/// Service 契約）。
///
/// 状態を持たない（フィールドの無い型で、関連関数だけを持つ）。[`crate::model::Sheet`] は
/// 列名を知らない（列の意味論は `schema-engine` が決める）ため、列の順序付きリストは
/// 呼び出し元が与える（design `DeterministicJson` の「スキーマ由来の動的な列集合は、
/// スキーマが定める列順序で明示的に整列する」をそのまま受ける）。シート識別子と
/// エントリ名の対応も本層が持つ（依存方向 `Model → Json → Parts`）。
#[derive(Debug, Clone, Copy)]
pub struct RowsCodec;

impl RowsCodec {
    /// 1 シート分の行データを `sheets/<sheet-ulid>.jsonl` エントリとして符号化する。
    ///
    /// `columns` は呼び出し元が与える**列名の順序付きリスト**であり、その順序が行
    /// オブジェクトのキー順になる（並べ替えない）。`rows` の順序がそのまま行順になる
    /// （**ソートしない**）。各行は `$id` を先頭キーとして書かれる。
    ///
    /// セル値は [`to_json_bytes`]（value 層の単一の源）で先に検査・符号化し、その原文を
    /// verbatim でレコードへ差し込む。したがって `-0.0` の正規化と NaN / Infinity の遮断は
    /// value 層の規則のままであり、**部分的なバイト列は一切返らない**（0 行なら 0 バイト）。
    ///
    /// 値の個数が `columns` の個数と一致しない行、および列名の重複は、呼び出し元の
    /// programming error として [`RowsEncodeError`] で報告する（panic しない）。
    pub fn encode(
        sheet: SheetId,
        columns: &[String],
        rows: &[Row],
    ) -> Result<(EntryName, Vec<u8>), RowsEncodeError> {
        let entry = EntryName::Rows { sheet };
        let location = entry.to_string();
        ensure_unique_columns(columns)?;
        // 全セルを先に検査・符号化する（途中で失敗しても部分的なバイト列を残さない。
        // 行ごとの領域は連続させ、レコードはそれを借りるだけにして確保を増やさない）。
        let mut cells: Vec<Box<RawValue>> = Vec::with_capacity(rows.len() * columns.len());
        for (index, row) in rows.iter().enumerate() {
            let line = index + 1;
            let values = row.values();
            if values.len() != columns.len() {
                return Err(RowsEncodeError::ValueCountMismatch {
                    row: row.id(),
                    columns: columns.len(),
                    values: values.len(),
                });
            }
            for (column, value) in columns.iter().zip(values) {
                // セル単位の位置は失敗したときにだけ組み立てる。成功経路で組み立てると
                // 10 万行 × 30 列で 300 万回の文字列確保になる（要件 8.2 の予算を圧迫する）。
                // 符号化は入力だけで決まるので、同じセルをやり直せば同じ失敗が位置付きで返る。
                let cell = to_raw_value(value, &location).or_else(|_| {
                    to_raw_value(value, &format!("{location} line {line}: column {column}"))
                })?;
                cells.push(cell);
            }
        }
        let records: Vec<RowRecord<'_>> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| RowRecord {
                id: row.id(),
                columns,
                cells: &cells[index * columns.len()..(index + 1) * columns.len()],
            })
            .collect();
        // 行のフレーミング（LF 終端・最終行の終端・失敗時に 1 バイトも書かない）は
        // json 層の単一の源に任せる（自前の改行を足さない）。
        let mut bytes = Vec::new();
        write_ndjson(&mut bytes, &location, &records)?;
        Ok((entry, bytes))
    }

    /// `sheets/<sheet-ulid>.jsonl` エントリの実バイト列から復号する。
    ///
    /// エントリ名から取り出したシート識別子と、**ファイル自身のキー順**が定める列順、
    /// ファイルの行順そのままの行を返す（[`SheetRows`]）。セルは原文を
    /// [`from_json_bytes`] へ通して復元するため、i64 範囲外の整数リテラルはここで拒否
    /// される（モジュール docs「i64 範囲外整数の門」）。
    ///
    /// エントリ名が `sheets/<ulid>.jsonl` 形でない場合もコンテナ不正として拒否する
    /// （panic しない）。行間のキー列の不一致・キーの重複・書き手の像の外のキー
    /// （単一 `$` 始まりで `$id` でも `$$` でもない形）・行識別子の重複などの
    /// 自己矛盾は、行番号付きの [`DocumentError::InvalidContainer`] になる
    /// （モジュール docs「復号が受理するキー」「エラー対応」）。
    pub fn decode(entry: &EntryName, bytes: &[u8]) -> Result<SheetRows, DocumentError> {
        let EntryName::Rows { sheet } = entry else {
            return Err(invalid_entry(
                entry,
                "not a sheet rows entry (expected `sheets/<sheet-ulid>.jsonl`)",
            ));
        };
        let location = entry.to_string();
        let raw_rows: Vec<RawRow> = read_ndjson(bytes, &location)?;
        let mut shape: Option<RowShape> = None;
        let mut seen_ids: HashSet<RowId> = HashSet::with_capacity(raw_rows.len());
        let mut rows: Vec<Row> = Vec::with_capacity(raw_rows.len());
        for (index, raw) in raw_rows.iter().enumerate() {
            let line = index + 1;
            if shape.is_none() {
                // 最初の行がそのシートのキー列（`$id` + 列集合 + 順序）を定める。
                shape = Some(RowShape::establish(&raw.entries, &location, line)?);
            }
            let shape = shape.as_ref().expect("直前の分岐で確立済み");
            if !shape.matches(&raw.entries) {
                return Err(line_error(
                    &location,
                    line,
                    format!(
                        "row keys differ from line 1: expected {:?}, found {:?}",
                        shape.keys,
                        keys_of(&raw.entries),
                    ),
                ));
            }
            let id = parse_row_id(&raw.entries[shape.id_position].value, &location, line)?;
            if !seen_ids.insert(id) {
                return Err(line_error(
                    &location,
                    line,
                    format!("duplicate row identifier {id}"),
                ));
            }
            let mut values = Vec::with_capacity(shape.columns.len());
            for (position, column) in shape.positions.iter().zip(&shape.columns) {
                let value = from_json_bytes(raw.entries[*position].value.get().as_bytes())
                    .map_err(|error| cell_error(&location, line, column, error))?;
                values.push(value);
            }
            let mut row = Row::new(id);
            row.set_values(values);
            rows.push(row);
        }
        // 0 行のエントリ（0 バイト）は列順を表現できないため、空の列順を返す。
        let columns = shape.map(|shape| shape.columns).unwrap_or_default();
        Ok(SheetRows {
            sheet: *sheet,
            columns,
            rows,
        })
    }
}

/// 復号された 1 シート分の行データ（[`RowsCodec::decode`] の戻り値）。
///
/// 列順は**ファイル自身のキー順**である（本クレートはスキーマを解釈しないため、
/// 位置 → 列名の対応はファイルが定める）。呼び出し元はこの列順で値を位置に割り当てる。
/// 0 行のエントリは列順を持たない（[`SheetRows::columns`]）。
#[derive(Debug)]
pub struct SheetRows {
    sheet: SheetId,
    columns: Vec<String>,
    rows: Vec<Row>,
}

impl SheetRows {
    /// 行データの所属シート（エントリ名から取り出した識別子）。
    #[inline]
    pub fn sheet(&self) -> SheetId {
        self.sheet
    }

    /// ファイルが定めた列名の順序（`$id` を除く。`$` エスケープは戻した形）。
    ///
    /// 0 行のエントリ（0 バイト）では空になる（列順を書く行が無いため）。
    #[inline]
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// ファイルの行順そのままの 0 個以上の行。
    #[inline]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// 復号済みの行を所有権ごと取り出す（タスク 4.8 の読み込み経路が使う消耗経路）。
    ///
    /// [`Row`] は `Clone` を実装しない（識別子発行の単発行者保証を黙って壊さないため。
    /// model/sheet.rs の「Clone を実装しない理由」参照）ため、モデルへ移すには
    /// 借用ではなく所有権の移動が要る。借用のままセル値を複製して行を組み直す経路は
    /// 10 万行で全セルの深いコピーを作るため使わない（要件 8.1）。
    /// 列順が必要なら先に [`SheetRows::columns`] を読むこと。
    #[inline]
    pub fn into_rows(self) -> Vec<Row> {
        self.rows
    }
}

/// 行データ符号化の入力が行データの前提を満たさないことを表す最小のローカル型
/// （モジュール docs「ローカルエラー型」。design のエラー表に対応変種が無い
/// 呼び出し元の programming error であり、`DocumentError` には含めない）。
#[derive(Debug, Error)]
pub enum RowsEncodeError {
    /// 行の値の個数が列数と一致しない（穴埋めも切詰めもしない）。
    #[error("row {row} has {values} values but {columns} columns were given")]
    ValueCountMismatch {
        /// 該当する行。
        row: RowId,
        /// 呼び出し元が与えた列数。
        columns: usize,
        /// 行が持っていた値の個数。
        values: usize,
    },
    /// 列名が重複しており、同名のキーが 1 つの行オブジェクトに 2 回現れる。
    #[error("duplicate column name at position {column}")]
    DuplicateColumn {
        /// 重複が確定した位置（0 始まり。2 回目の出現）。
        column: usize,
    },
    /// 書き出し自体の失敗（非有限値の遮断など）。[`DocumentError`] をそのまま運ぶ。
    #[error(transparent)]
    Document(#[from] DocumentError),
}

/// 1 行分の wire レコード（`{"$id":"<ULID>", <列>: <値>, ...}`）。
///
/// セル値は value 層が検査・符号化した原文（[`RawValue`]）を**そのまま**差し込む。
/// フィールド宣言順ではなく [`Serialize`] の実装順が出力順を決める（予約キーが先頭）。
struct RowRecord<'a> {
    /// 行識別子（`$id` の値）。
    id: RowId,
    /// 呼び出し元が与えた列名（この順で書く）。
    columns: &'a [String],
    /// `columns` と同じ並びの、符号化済みセル値。
    cells: &'a [Box<RawValue>],
}

impl Serialize for RowRecord<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1 + self.cells.len()))?;
        map.serialize_entry(ROW_ID_KEY, &self.id)?;
        for (column, cell) in self.columns.iter().zip(self.cells) {
            map.serialize_entry(&wire_key(column), cell)?;
        }
        map.end()
    }
}

/// wire 上の 1 行（キー順を保持したまま、値は原文のまま捕捉する）。
#[derive(Debug)]
struct RawRow {
    /// キーと値の対（入力の出現順）。
    entries: Vec<RawEntry>,
}

/// 行オブジェクトの 1 キー分（キーは JSON 文字列として復号したテキスト、値は原文）。
#[derive(Debug)]
struct RawEntry {
    /// wire のキー（`$` エスケープを戻す前）。
    key: String,
    /// 値の原文（[`from_json_bytes`] の門へ通すために保持する）。
    value: Box<RawValue>,
}

impl<'de> Deserialize<'de> for RawRow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// 行オブジェクトをキーの出現順のまま読む Visitor。
        ///
        /// `serde_json` のマップアクセスは入力順にキーを返すため、`preserve_order`
        /// feature に依存せずキー順が保たれる（そもそも本クレートは `serde_json` の
        /// 汎用マップ型を経路に持たない）。
        struct RowVisitor;

        impl<'de> Visitor<'de> for RowVisitor {
            type Value = RawRow;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object of row cells")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<RawRow, M::Error> {
                let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some(key) = map.next_key::<String>()? {
                    let value = map.next_value::<Box<RawValue>>()?;
                    entries.push(RawEntry { key, value });
                }
                Ok(RawRow { entries })
            }
        }

        deserializer.deserialize_map(RowVisitor)
    }
}

/// 1 シート分のキー列（最初の行が定める wire キーの並びと、その解釈）。
struct RowShape {
    /// `$id` の位置（wire キー列内）。
    id_position: usize,
    /// 行間の一致検査に使う wire キー（エスケープされたまま）。
    keys: Vec<String>,
    /// `$id` 以外の wire キー位置（ファイルのキー順）。
    positions: Vec<usize>,
    /// `positions` と同じ並びの列名（`$` エスケープを戻した形）。
    columns: Vec<String>,
}

impl RowShape {
    /// 最初の行からキー列を確立する（自己矛盾と像外キーはその場で拒否する）。
    ///
    /// キーの検査はここで 1 行分だけ行えば足りる: 以降の行はキー列の完全一致が要求される
    /// ため（[`RowShape::matches`]）、像外キーは 2 行目以降では一致検査が先に弾く。
    fn establish(entries: &[RawEntry], location: &str, line: usize) -> Result<Self, DocumentError> {
        let mut keys: Vec<String> = Vec::with_capacity(entries.len());
        let mut positions: Vec<usize> = Vec::with_capacity(entries.len());
        let mut columns: Vec<String> = Vec::with_capacity(entries.len());
        let mut seen_keys: HashSet<&str> = HashSet::with_capacity(entries.len());
        let mut seen_columns: HashSet<&str> = HashSet::with_capacity(entries.len());
        let mut id_position: Option<usize> = None;
        for (position, entry) in entries.iter().enumerate() {
            if !seen_keys.insert(entry.key.as_str()) {
                // 同名キーのどちらがデータか決まらない（自己矛盾）。
                return Err(line_error(
                    location,
                    line,
                    format!("duplicate key `{}`", entry.key),
                ));
            }
            if entry.key == ROW_ID_KEY {
                // 予約キーは完全一致のみ（`$$id` は列名 `$id` になる）。
                id_position = Some(position);
            } else {
                let column = decode_column(&entry.key).ok_or_else(|| {
                    line_error(
                        location,
                        line,
                        format!("key `{}` is not in the row wire form", entry.key),
                    )
                })?;
                if !seen_columns.insert(column) {
                    // 像の検査を通ったキーは単射に列名へ写るため通常は起こらないが、
                    // 像の定義が変わっても列名の一意性が破れないよう明示的に守る。
                    return Err(line_error(
                        location,
                        line,
                        format!("duplicate column name `{column}`"),
                    ));
                }
                positions.push(position);
                columns.push(column.to_owned());
            }
            keys.push(entry.key.clone());
        }
        let Some(id_position) = id_position else {
            return Err(line_error(
                location,
                line,
                format!("row object has no `{ROW_ID_KEY}` key"),
            ));
        };
        Ok(Self {
            id_position,
            keys,
            positions,
            columns,
        })
    }

    /// 行のキー列が最初の行と同一か（`$id` + 同一列集合 + 同一順序）。
    fn matches(&self, entries: &[RawEntry]) -> bool {
        self.keys.len() == entries.len()
            && self
                .keys
                .iter()
                .zip(entries)
                .all(|(key, entry)| key.as_str() == entry.key.as_str())
    }
}

/// 列名を wire のキーへ写す（`$` 始まりは `$` を 1 つ足す）。
///
/// `$` 始まりの列名を作るのはスキーマ側であり、本クレートはいつでもそれをそのまま
/// 受理しなければならない（列名の制約は課さない）。
fn wire_key(column: &str) -> Cow<'_, str> {
    if column.starts_with('$') {
        Cow::Owned(format!("${column}"))
    } else {
        Cow::Borrowed(column)
    }
}

/// wire のキーを列名へ戻す（[`wire_key`] の逆写像の定義域の検査込み）。
///
/// 受理するのは次の 3 形だけである（**書き手の像**。`$id` は呼び出し元が先に
/// 完全一致で取り除くためここへは渡らない）:
///
/// - `$` で始まらないキー: 列名そのもの（[`wire_key`] がそのまま写す形）
/// - `$$` で始まるキー: `$` 始まりの列名のエスケープ形（先頭の `$` を 1 つ剥がす）
///
/// それ以外（単一 `$` で始まり `$id` でも `$$` でもない形。例 `$x`）は `None` を返す。
/// 本クレートの書き出しはその形を決して作らないため、**正規化せず拒否**する
/// （[`crate::entry_name`] の「サニタイズではなく拒否」と同じ規約。受理すると `$x` と
/// `$$x` が同居する入力で列名が重複し、開いた時点で検出できない破綻が残る）。
fn decode_column(key: &str) -> Option<&str> {
    if key.starts_with("$$") {
        Some(&key[1..])
    } else if key.starts_with('$') {
        None
    } else {
        Some(key)
    }
}

/// 列名の重複を拒否する（同名のキーが 1 つの行オブジェクトに 2 回現れる wire を
/// 書かない。復号側が自己矛盾として拒否する内容を書かないための書き手側の門）。
fn ensure_unique_columns(columns: &[String]) -> Result<(), RowsEncodeError> {
    let mut seen: HashSet<&str> = HashSet::with_capacity(columns.len());
    for (position, column) in columns.iter().enumerate() {
        if !seen.insert(column.as_str()) {
            return Err(RowsEncodeError::DuplicateColumn { column: position });
        }
    }
    Ok(())
}

/// セル値を value 層の検査経路で符号化し、verbatim 差し込み用の raw 値にする。
///
/// [`to_json_bytes`] が `-0.0` の正規化と NaN / Infinity の遮断の単一の源であり、
/// 本モジュールは再実装しない。`RawValue` は「原文のバイト列をそのまま書く」唯一の
/// 経路であり、[`crate::parts::schema_codec`] の不透明ペイロードと同じ機構である。
fn to_raw_value(value: &CellValue, location: &str) -> Result<Box<RawValue>, RowsEncodeError> {
    let bytes = to_json_bytes(value, location)?;
    let text = String::from_utf8(bytes).map_err(|error| internal(location, &error))?;
    RawValue::from_string(text).map_err(|error| internal(location, &error))
}

/// 本クレートが生成した JSON の raw 化失敗（起こり得ない経路）をコンテナ不正へ写す。
///
/// `panic` を置かず、「失敗箇所のラベル + 理由」の規約に載せる（[`crate::parts::schema_codec`]
/// の `raw_payload` と同じ規律）。
fn internal(location: &str, reason: &dyn fmt::Display) -> RowsEncodeError {
    RowsEncodeError::Document(DocumentError::InvalidContainer {
        entry: format!("{location}: {reason}"),
    })
}

/// `$id` の原文を 26 文字 ULID テキストとして [`RowId`] へ解決する。
///
/// 受理規則は [`RowId`] の `FromStr`（大小文字を問わない）に従う。正準形は大文字であり、
/// 小文字表記の入力は再符号化で正準形に揃う（[`crate::entry_name`] の完全一致規則とは
/// 意図的に異なる: エントリ名は本クレートだけが書く命名だが、行ファイルの内容は
/// 外部ツールが書いたものも読み戻せなければならない）。
fn parse_row_id(value: &RawValue, location: &str, line: usize) -> Result<RowId, DocumentError> {
    let text = serde_json::from_str::<String>(value.get()).map_err(|_| {
        line_error(
            location,
            line,
            format!("`{ROW_ID_KEY}` is not a JSON string"),
        )
    })?;
    text.parse::<RowId>().map_err(|error| {
        line_error(
            location,
            line,
            format!("`{ROW_ID_KEY}` is not a row: {error}"),
        )
    })
}

/// 行のキー列（診断用。失敗経路でのみ確保する）。
fn keys_of(entries: &[RawEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.key.as_str()).collect()
}

/// 行に帰属する構造的な失敗を、エントリ名 + 1 始まりの行番号付きのコンテナ不正へ写す
/// （[`crate::json::read_ndjson`] の診断規約をそのまま延長する）。
fn line_error(location: &str, line: usize, reason: impl fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("{location} line {line}: {reason}"),
    }
}

/// セルの解析失敗を、エントリ名 + 行番号 + 列名付きのコンテナ不正へ写す。
///
/// セル値の解析失敗は [`crate::value`] が `InvalidContainer` の `entry` に
/// `value: <理由>` として載せる。その理由を捨てずに、どの行のどの列かを前置きする。
fn cell_error(location: &str, line: usize, column: &str, error: DocumentError) -> DocumentError {
    match error {
        DocumentError::InvalidContainer { entry } => DocumentError::InvalidContainer {
            entry: format!("{location} line {line}: column {column}: {entry}"),
        },
        other => other,
    }
}

/// エントリ種別の取り違えなど、エントリ名を文脈にした失敗（復号の入口）。
fn invalid_entry(entry: &EntryName, reason: impl fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("{entry}: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{AttachmentId, IdFactory};
    use crate::model::Sheet;
    use crate::value::NestedValue;

    /// 標本の列名（列順は呼び出し元が与える）。
    fn sample_columns() -> Vec<String> {
        ["name", "count", "amount", "notes"]
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    }

    /// 標本の値（列数 4。`CellValue` の複数変種を混ぜる）。
    fn sample_values(index: usize) -> Vec<CellValue> {
        vec![
            CellValue::Text(format!("行 {index} 🎉")),
            CellValue::Int(index as i64),
            CellValue::float(index as f64 + 0.5),
            match index % 5 {
                0 => CellValue::Null,
                1 => CellValue::Bool(index.is_multiple_of(2)),
                2 => CellValue::Decimal(format!("{index}.25")),
                3 => CellValue::Nested(NestedValue::Object(vec![
                    ("z".into(), CellValue::Int(1)),
                    ("a".into(), CellValue::Text("x".into())),
                ])),
                _ => CellValue::Attachment(AttachmentId::from_bytes(b"probe")),
            },
        ]
    }

    /// 10 万行の往復でモデルが完全に一致すること（タスク 4.5 の中核。要件 3.4）。
    ///
    /// モデルは [`Sheet`] に `Document` と同じクレート可視経路（[`Row::new`] +
    /// [`Row::set_values`] + [`Sheet::push_row`]）で O(n) に組み立てる
    /// （`Document::set_row_values` は対象行を線形探索するため 10 万行では O(n²) に
    /// なる。公開 API 経由での行構築は統合テスト `tests/rows_codec.rs` が担う）。
    /// 行順は並び替え後とし、復元順がシートの行順そのままであることを全行で確かめる。
    #[test]
    fn one_hundred_thousand_rows_round_trip_preserves_the_model() {
        const ROWS: usize = 100_000;
        /// 100_000 と互いに素な歩幅（乗算は 0..ROWS の全単射になる）。
        /// ULID 昇順とも追加順とも異なる行順を作るために使う。
        const STRIDE: usize = 7_919;

        let columns = sample_columns();
        let mut factory = IdFactory::new();
        let sheet_id = factory.new_sheet_id();
        let mut sheet = Sheet::new(sheet_id, "大量".to_owned());
        let mut ids: Vec<RowId> = Vec::with_capacity(ROWS);
        let mut values: Vec<Vec<CellValue>> = Vec::with_capacity(ROWS);
        for index in 0..ROWS {
            let id = factory.new_row_id();
            let row_values = sample_values(index);
            let mut row = Row::new(id);
            row.set_values(row_values.clone());
            sheet.push_row(row);
            ids.push(id);
            values.push(row_values);
        }
        let order: Vec<RowId> = (0..ROWS)
            .map(|position| ids[(position * STRIDE) % ROWS])
            .collect();
        sheet.reorder_rows(&order).expect("並び替え");

        let (entry, bytes) = RowsCodec::encode(sheet_id, &columns, sheet.rows()).expect("符号化");
        assert_eq!(
            ROWS,
            bytes.iter().filter(|&&byte| byte == b'\n').count(),
            "1 データ行 = 1 テキスト行でない"
        );

        let decoded = RowsCodec::decode(&entry, &bytes).expect("復号");
        assert_eq!(sheet_id, decoded.sheet(), "復号でシート識別子が変わった");
        assert_eq!(columns, decoded.columns(), "列順が復号で変わった");
        assert_eq!(ROWS, decoded.rows().len(), "行数が復号で変わった");
        for (position, row) in decoded.rows().iter().enumerate() {
            let origin = (position * STRIDE) % ROWS;
            assert_eq!(
                order[position],
                row.id(),
                "行 {position} の識別子が変わった"
            );
            assert_eq!(values[origin], row.values(), "行 {position} の値が変わった");
        }

        let (re_entry, re_bytes) =
            RowsCodec::encode(sheet_id, &columns, decoded.rows()).expect("再符号化");
        assert_eq!(entry, re_entry);
        assert_eq!(bytes, re_bytes, "同一モデルの再符号化がバイト一致しない");
    }
}

//! ドキュメントパート（タスク 4.3。要件 2.2, 3.6。design「Container Entry Layout」）。
//!
//! `document.json` が持つのは、ドキュメントの**安定メタデータ**である（design 同節
//! 「ドキュメント ID、シート順序、シートのメタデータ」）:
//!
//! - **ドキュメント識別子** [`DocumentId`]: [`crate::ids`] が発行する ULID 新型。
//!   シート識別子 [`SheetId`] とは別の型であり、相互に取り違えられない。
//! - **シート順序**: [`DocumentPart::sheets`] の配列順が**そのまま**ドキュメントの
//!   シート順序である（要件 1.1 のモデル順。タスク 4.8 が `Document::sheets` の反復順と
//!   この順序を結線する）。
//! - **シートのメタデータ**: シート識別子・シート名・順序付きの列名の組 [`SheetMeta`]。
//!   列名を永続化する理由（0 行のシートは行エントリから列順を復元できない）は
//!   [`SheetMeta`] の docs にある。
//!
//! # 派生値も揮発値も持たない
//!
//! 行数・ダイジェスト・保存時刻のような値は本パートに置かない。派生値は真実の第二の源
//! （二重帳簿。design 同節が採らないと決めたもの）になり、要件 3.6（保存時刻・実行環境・
//! 内部処理順序に依存する値を出力に含めない）にも反する。必要な読み手は権威ある源から
//! 計算する（行数は `sheets/<ulid>.jsonl`、ダイジェストは `manifest.json` の索引）。
//! 符号化は入力の**純関数**であり、同じ内容は構築経路や時刻によらず常に同じバイト列に
//! なる（要件 3.1, 3.6）。キー集合は下記の 5 つに固定されており、時刻・環境・乱数に
//! 由来する項目は無い。列名は**派生値ではなく入力**である（本クレートは列名の中身を
//! 解釈できないため、行データからもスキーマからも再計算できない）。
//!
//! # JSON 形式（本クレートが所有する確定形）
//!
//! コンパクトな UTF-8・末尾改行なしで、以下が確定形である（例は読みやすさのため
//! 改行と空白を入れている）:
//!
//! ```json
//! {
//!   "document_id": "<26 文字 ULID>",
//!   "sheets": [
//!     {"sheet_id": "<26 文字 ULID>", "name": "<シート名>", "columns": ["<列名>"]}
//!   ]
//! }
//! ```
//!
//! - トップレベルのキーは `document_id` → `sheets` の順に固定し（[`DocumentPart`] の
//!   フィールド宣言順）、シート要素のキーは `sheet_id` → `name` → `columns` の順に
//!   固定する（[`SheetMeta`] のフィールド宣言順。要件 3.3）。キー集合はこの 5 つだけである。
//! - **`columns` は必須キーである**（タスク 4.8 で追加）。列名 0 個のシートも
//!   `"columns":[]` として書く: 確定形が常に同じキー集合を持つため、欠落を空列として
//!   受け入れる余地（= 2 つ目の受理形）を作らない。列名の並びは与えられた順のままで、
//!   並べ替えも正規化もしない（本クレートは列名の中身を解釈しない）。
//! - 識別子は 26 文字の Crockford base32 テキスト（大文字）として書く
//!   （[`DocumentId`] / [`SheetId`] の `Display`）。解析には両型の `FromStr` を使い、
//!   **正準形に一致しない表記（小文字など）は拒否する**: 受理して書き戻すと表記が
//!   黙って変わり、バイト単位の往復が崩れるためである（[`crate::entry_name`] が
//!   エントリ名と添付 hex に課している「識別子は正準形で書く」規則と同じ）。
//! - シート名は任意の UTF-8 文字列で、**正規化もサニタイズもしない**（空文字列・前後の
//!   空白・制御文字・非 ASCII・絵文字をそのまま往復させる）。文字列のエスケープは
//!   `serde_json` の規則に委ね、自前の規則を持たない。
//! - `serde_json` の汎用 JSON 値型・マップ型・`HashMap` を経由しない（親モジュール
//!   [`crate::json`] の規則。キー順序の固定と決定性の根拠）。
//! - シート要素の組み立ては要素ごとに差し戻し位置が決まるため、要素を
//!   [`PreservingObjectWriter`] で 1 件ずつ書き出して配列を組み、その配列を raw 値として
//!   トップレベルへ差し込む（[`RawValue`] が verbatim の唯一の経路）。`from_string` は
//!   本クレートが生成した妥当な JSON 配列を 1 回検証するだけで、値を再解釈も再整形もしない。
//! - **本パートの列名は不透明である**（どの列がどの型かは `schema-engine` が決める。
//!   design「スキーマ・ペイロードの不透明性」）。本パートは順序付きの文字列として運ぶだけである。
//!
//! # シート順序は並べ替えない（`manifest.json` との違い）
//!
//! `document.json` のシート順序は**それ自体がデータ**であり（要件 1.1 のモデル順）、
//! 与えられた順序をそのまま保持する。ソートも正準化もしない。同じ `parts` 層の
//! [`crate::parts::manifest`] は索引なのでエントリ名の昇順へ整列するが、これは**逆の
//! 規則**である。理由が違うから規則も違う: 索引の並びは引き当ての都合で決まるのに対し、
//! シート順序は利用者が決めたデータそのもので、並べ替えれば意味が変わる。
//!
//! # 未知フィールドの保持（前方互換。要件 6.2 / 6.3）
//!
//! 復号は**トップレベルとシート要素のそれぞれで** [`PreservedFields`] が未知キーを
//! 原文の位置ごと保持し、符号化は [`PreservingObjectWriter`] がその位置へ差し戻す
//! （トップレベルに 1 個、シート要素 1 件につき 1 個。タスク 3.2 の必須条件
//! 「1 オブジェクトにつき 1 個」）。したがって将来の minor 版が文書全体のメタデータを
//! 足しても、シートのメタデータへ省略可能フィールド（例: 表示色や列幅のヒント）を
//! 足しても、本実装を通した往復で失われない。
//!
//! 保持の**対象外**は `manifest.json` と同じ 2 点である: (1) 未知フィールドの**キー**は
//! JSON 文字列としてデコードしたテキストで保持するため、原文が `\uXXXX` のような
//! エスケープ表記を使っていればその表記は保たれない（値はバイト単位で保たれる。
//! `PreservedField` の docs）。(2) 値と区切り・キーと値の間の空白は JSON の値の一部では
//! ないため保たれない。往復のバイト一致が成立する条件は [`PreservedFields`] の docs と
//! 同じ（コンパクトな入力 + 既知フィールドが宣言順）であり、本クレート自身が書いた
//! ファイルはこの条件を満たす。
//!
//! 未知フィールドの差し戻し位置を保つため、既知キーの**重複**（同じ `"sheets"` が 2 回
//! など）はトップレベル・シート要素のどちらでも [`DocumentError::InvalidContainer`] として
//! 拒否する（[`PreservedFields::record_known_field`] と
//! [`PreservingObjectWriter::write_known`] の呼び出し件数を 1 対 1 に保つため。件数が
//! ずれると差し戻し位置がずれる）。
//!
//! # 本パートが強制する不変条件
//!
//! [`DocumentPart::new`] と [`DocumentPart::from_json_bytes`] は同じ検査を通る:
//!
//! - **シート識別子の重複の拒否**: 同一ファイル内で同じシート識別子が 2 回現れる
//!   `document.json` は自己矛盾であり不正である。コンテナの重複パス（要件 2.6）と同じ
//!   分類であり、`DuplicateId`（識別子体系の一意性）ではない。**識別子の全体一意性
//!   （行・型定義・添付をまたぐ検証）は本パートの責務ではなく、タスク 4.6 の担当である。**
//!   報告する `entry` には識別子だけでなく**その識別子を持つ全要素の出現箇所**
//!   （`document.json sheets[i]`。0 始まり）を載せる（要件 4.3 は「重複した識別子と
//!   **その出現箇所**を含むエラー」を要求する。`canonicalize` の docs）。
//! - **順序は検証しない**: シートの任意の並びが正当である（上記「シート順序は
//!   並べ替えない」）。列名の並びも同様に検証しない（本クレートは中身を解釈しない）。
//! - **0 枚のシートは正当である**（要件 1.1: 0 個以上のシート）。
//!
//! `PartialEq` は提供しない（[`DocumentPart`] / [`SheetMeta`] の docs）。
//!
//! # エラー対応
//!
//! 読み込みの失敗はすべて読み込み全体の中止であり、部分的な結果を返さない（要件 5.4 系）。
//! design のエラー表に本モジュール固有の変種は無いため、コンテンツ解析の失敗は
//! コンテナ不正 [`DocumentError::InvalidContainer`] へ写す（[`crate::value`] /
//! [`crate::json::determinism`] と同じ規約）。`entry` は [`EntryName::Document`] の
//! 表示テキスト（`document.json`）と理由である。
//!
//! | 失敗 | 返す変種と文脈 |
//! |------|----------------|
//! | JSON として不正 / トップレベルがオブジェクトでない | [`DocumentError::InvalidContainer`]（`entry` = `document.json: <理由>`） |
//! | 必須キー欠落・型違い（`document_id` / `sheets` / `sheet_id` / `name` / `columns`） | 同上 |
//! | 識別子が正準形の 26 文字 ULID でない | 同上 |
//! | `sheets` が配列でない / シート要素がオブジェクトでない | 同上 |
//! | `columns` が文字列の配列でない | 同上 |
//! | 同一ファイル内でシート識別子が重複（識別子と**全出現箇所**を `entry` へ載せる。要件 4.3） | 同上 |
//! | 既知キーの重複（トップレベル / シート要素） | 同上 |
//! | 内部の組み立てが壊れた場合（本クレートが生成した配列の raw 化失敗） | 同上（起こり得ない経路だが `panic` しない） |

use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

use crate::entry_name::EntryName;
use crate::error::DocumentError;
use crate::ids::{DocumentId, SheetId};
use crate::json::{PreservedFields, PreservingObjectWriter};

/// トップレベルの既知キー: ドキュメント識別子（宣言順の 1 番目）。
const DOCUMENT_ID_KEY: &str = "document_id";
/// トップレベルの既知キー: シート順序とシートのメタデータ（宣言順の 2 番目）。
const SHEETS_KEY: &str = "sheets";
/// シート要素の既知キー: シート識別子（宣言順の 1 番目）。
const SHEET_ID_KEY: &str = "sheet_id";
/// シート要素の既知キー: シート名（宣言順の 2 番目）。
const NAME_KEY: &str = "name";
/// シート要素の既知キー: 順序付きの列名（宣言順の 3 番目。タスク 4.8 で追加）。
const COLUMNS_KEY: &str = "columns";

/// シート 1 枚分のメタデータ: シート識別子・シート名・順序付きの列名
/// （design「Container Entry Layout」の「シートのメタデータ」）。
///
/// **派生値と揮発値を持たない**（行数・ダイジェスト・保存時刻のような値は置かない。
/// モジュール docs「派生値も揮発値も持たない」）。シート名は任意の UTF-8 文字列で、
/// 本パートは正規化もサニタイズもしない（[`SheetMeta::new`] は文字列をそのまま保持し、
/// 符号化は `serde_json` の文字列エスケープ規則で書く）。
///
/// # 列名を持つ理由（タスク 4.8 の親の裁定）
///
/// 行データの wire 形式は列名をキーとする（`sheets/<ulid>.jsonl`。タスク 4.5）ため、
/// 書き出し時に行の列名一覧が必要である。さらに **0 行のシートは行エントリから列順を
/// 復元できない**（0 バイトのエントリは列順を書く行を持たない）ため、`document.json` が
/// その唯一の永続先になる。列名の中身は本クレートにとって不透明である
/// （どの列がどの型かは `schema-engine` が決める。design「スキーマ・ペイロードの不透明性」）。
/// 列名 0 個のシートは正当であり、その場合も `columns` キーは空配列として書く
/// （確定形は常に同じキー集合を持つ。モジュール docs「JSON 形式」）。
///
/// 要素の中の未知キーは 1 件ごとに [`PreservedFields`] が原文の位置ごと保持し、
/// [`SheetMeta::to_json_bytes`] が差し戻す（モジュール docs「未知フィールドの保持」）。
///
/// `PartialEq` は提供しない（[`DocumentPart`] と同じ理由: 保持している未知フィールドの
/// 比較には読み込みカーソルが混じる。内容は [`SheetMeta::sheet_id`] / [`SheetMeta::name`] /
/// [`SheetMeta::columns`] の写像か符号化したバイト列で判定する）。
#[derive(Debug, Clone)]
pub struct SheetMeta {
    sheet_id: SheetId,
    name: String,
    /// 順序付きの列名（本クレートは中身を解釈しない）。空配列も正当である。
    columns: Vec<String>,
    /// 解釈しない要素内のフィールド（前方互換。要件 6.2 / 6.3）。
    preserved: PreservedFields,
}

impl SheetMeta {
    /// シート識別子とシート名から 1 件を組み立てる（保存経路）。
    ///
    /// シート名は正規化もサニタイズもせず、与えられた文字列をそのまま保持する
    /// （空文字列・前後の空白・制御文字・非 ASCII・絵文字がそのまま往復する）。
    /// 列名は 0 個で始まり、必要なら [`SheetMeta::with_columns`] で与える。
    pub const fn new(sheet_id: SheetId, name: String) -> Self {
        Self {
            sheet_id,
            name,
            columns: Vec::new(),
            preserved: PreservedFields::new(),
        }
    }

    /// 列名（順序付き）を与える（ビルダー。[`SheetMeta::new`] の意味は変えない）。
    ///
    /// 順序はそのまま出力のキー順になる（並べ替えない）。
    pub fn with_columns(mut self, columns: Vec<String>) -> Self {
        self.columns = columns;
        self
    }

    /// 保持すべき未知フィールドを据える（読み込み経路の復元用。`parts::from_parts` が
    /// 復号済みの保持内容をそのまま渡す）。
    ///
    /// クレート可視である: 外部の呼び出し元が任意の保持内容を詐称できる経路を作らない
    /// （保持内容は復号の副産物としてのみ生まれる）。
    pub(crate) fn with_preserved(mut self, preserved: PreservedFields) -> Self {
        self.preserved = preserved;
        self
    }

    /// シート識別子。
    pub const fn sheet_id(&self) -> SheetId {
        self.sheet_id
    }

    /// シート名（原文のまま）。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 順序付きの列名（本クレートは中身を解釈しない）。
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// `document.json` のシート要素で保持した未知キー（原文の位置ごと。要件 6.2 / 6.3）。
    ///
    /// 読み込み経路 `parts::from_parts` がこれをモデル（`Sheet`）へ移し、保存経路が
    /// [`SheetMeta::with_preserved`] で戻す。
    pub(crate) fn preserved_fields(&self) -> &PreservedFields {
        &self.preserved
    }

    /// 要素 1 件を確定形の JSON オブジェクトとして書き出す。
    ///
    /// キー順序は宣言順（`sheet_id` → `name` → `columns`）で、未知キーは
    /// [`PreservedFields::record_known_field`] と対になる位置へ差し戻す。失敗したときは
    /// 1 バイトも書かない（[`PreservingObjectWriter`] の規律を継承）。
    fn to_json_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        let mut out = Vec::new();
        let mut writer = PreservingObjectWriter::new(&mut out, &self.preserved);
        writer.write_known(SHEET_ID_KEY, &self.sheet_id)?;
        writer.write_known(NAME_KEY, &self.name)?;
        writer.write_known(COLUMNS_KEY, &self.columns)?;
        writer.finish()?;
        Ok(out)
    }
}

/// ドキュメントパート（`document.json`）: ドキュメント識別子と、シート順序を兼ねる
/// シートのメタデータの列（design「Container Entry Layout」）。
///
/// フィールドは非公開で、[`DocumentPart::new`] と [`DocumentPart::from_json_bytes`] の
/// 2 つの構築経路だけが通る検査（重複したシート識別子の拒否）が不変条件を強制する。
/// **シートの並びはそのままドキュメントのシート順序であり、並べ替えの対象ではない**
/// （モジュール docs「シート順序は並べ替えない」）。
///
/// `PartialEq` は提供しない。保持している未知フィールドの比較には読み込みカーソル
/// （[`PreservedFields`] の内部状態）が混じり、内容の等値を素直に表せないためである。
/// 2 つのパートが同じ内容かどうかは、[`DocumentPart::document_id`] /
/// [`DocumentPart::sheets`] を見るか、符号化したバイト列を比べて判定する
/// （後者が本形式の契約: 同一内容は常に同一バイト列。要件 3.1）。
#[derive(Debug, Clone)]
pub struct DocumentPart {
    document_id: DocumentId,
    /// ドキュメントのシート順序そのもの（並べ替えない）。
    sheets: Vec<SheetMeta>,
    /// 解釈しないトップレベルのフィールド（前方互換。要件 6.2 / 6.3）。
    preserved: PreservedFields,
}

impl DocumentPart {
    /// ドキュメント識別子とシートのメタデータの列から組み立てる（保存経路）。
    ///
    /// シート列は**与えられた順序のまま**保持し（並べ替えない）、同一ファイル内で
    /// シート識別子が重複していれば [`DocumentError::InvalidContainer`] として拒否する
    /// （モジュール docs「本パートが強制する不変条件」）。
    pub fn new(document_id: DocumentId, sheets: Vec<SheetMeta>) -> Result<Self, DocumentError> {
        Ok(Self {
            document_id,
            sheets: Self::canonicalize(sheets)?,
            preserved: PreservedFields::new(),
        })
    }

    /// ドキュメント識別子。
    pub const fn document_id(&self) -> DocumentId {
        self.document_id
    }

    /// ドキュメントのシート順序（配列順がそのまま順序である）。
    pub fn sheets(&self) -> &[SheetMeta] {
        &self.sheets
    }

    /// `document.json` のトップレベルで保持した未知キー（原文の位置ごと。要件 6.2 / 6.3）。
    ///
    /// 読み込み経路 `parts::from_parts` がこれをモデル（`Document`）へ移し、保存経路が
    /// [`DocumentPart::with_preserved`] で戻す。
    pub(crate) fn preserved_fields(&self) -> &PreservedFields {
        &self.preserved
    }

    /// 保持すべき未知フィールドを据える（読み込み経路の復元用。クレート可視）。
    pub(crate) fn with_preserved(mut self, preserved: PreservedFields) -> Self {
        self.preserved = preserved;
        self
    }

    /// 確定形の JSON バイト列へ符号化する（コンパクトな UTF-8・末尾改行なし）。
    ///
    /// キー順序・シート順序・識別子のテキスト形はモジュール docs の確定形に従う。
    /// 未知フィールドはトップレベル・シート要素のそれぞれで原文の位置へ差し戻す。
    /// 失敗したときは `Err` を返し、出力先へ 1 バイトも書かない
    /// （[`PreservingObjectWriter`] の規律を継承）。
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        // シート列は要素ごとに差し戻し位置が決まるため、先に配列全体を組み立ててから
        // raw 値として差し込む（`RawValue` が verbatim の唯一の経路）。`from_string` は
        // 本クレートが生成した妥当な JSON 配列を 1 回検証するだけで、値の再解釈はしない。
        let sheets = self.sheets_json_bytes()?;
        let sheets_raw =
            RawValue::from_string(String::from_utf8(sheets).map_err(invalid_document)?)
                .map_err(invalid_document)?;

        let mut out = Vec::new();
        {
            // 未知フィールドを原文の位置へ差し戻しながら、既知フィールドを宣言順に書く。
            // 書き出しは `finish` まで一時バッファに留まる（失敗時に 1 バイトも出さない）。
            let mut writer = PreservingObjectWriter::new(&mut out, &self.preserved);
            writer.write_known(DOCUMENT_ID_KEY, &self.document_id)?;
            writer.write_known(SHEETS_KEY, sheets_raw.as_ref())?;
            writer.finish()?;
        }
        Ok(out)
    }

    /// シートのメタデータを決定的な JSON 配列として組み立てる。
    ///
    /// 並びは [`DocumentPart::sheets`] の順序そのまま（並べ替えない）。各要素は自分の
    /// 未知キーを原文の位置へ差し戻す（[`SheetMeta::to_json_bytes`]）。
    fn sheets_json_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        let mut out = Vec::new();
        out.push(b'[');
        for (index, sheet) in self.sheets.iter().enumerate() {
            if index > 0 {
                out.push(b',');
            }
            out.extend_from_slice(&sheet.to_json_bytes()?);
        }
        out.push(b']');
        Ok(out)
    }

    /// 確定形の JSON バイト列から復号する。
    ///
    /// 失敗は中止であり、部分的なシート列を返さない（モジュール docs「エラー対応」の表）。
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, DocumentError> {
        let raw: RawDocument = serde_json::from_slice(bytes).map_err(invalid_document)?;
        Self::from_raw(raw)
    }

    /// 構文レベルの読み取り結果を検証し、不変条件を強制して組み立てる。
    ///
    /// 構文・型の失敗（serde）は [`invalid_document`] がコンテナ不正へ写し、
    /// ドメイン検証（識別子の正準形・重複したシート識別子）はここが型付きの文脈で返す。
    fn from_raw(raw: RawDocument) -> Result<Self, DocumentError> {
        if let Some(key) = raw.duplicate_known {
            return Err(invalid_document(format!("duplicate field `{key}`")));
        }
        let document_id_text = raw
            .document_id
            .ok_or_else(|| invalid_document(format!("missing field `{DOCUMENT_ID_KEY}`")))?;
        let raw_sheets = raw
            .sheets
            .ok_or_else(|| invalid_document(format!("missing field `{SHEETS_KEY}`")))?;

        let mut sheets = Vec::with_capacity(raw_sheets.len());
        for raw_sheet in raw_sheets {
            let RawSheet {
                sheet_id: sheet_id_text,
                name,
                columns,
                duplicate_known,
                preserved,
            } = raw_sheet;
            // 既知キーの重複は差し戻し位置の基準を壊すため拒否する（トップレベルと同じ規則）。
            if let Some(key) = duplicate_known {
                return Err(invalid_document(format!(
                    "duplicate field `{key}` in a sheet element"
                )));
            }
            sheets.push(SheetMeta {
                sheet_id: parse_canonical_sheet_id(&sheet_id_text)?,
                name,
                columns,
                preserved,
            });
        }

        Ok(Self {
            document_id: parse_canonical_document_id(&document_id_text)?,
            sheets: Self::canonicalize(sheets)?,
            preserved: raw.preserved,
        })
    }

    /// シート列の不変条件（重複したシート識別子の拒否）を強制する。両構築経路の共通の門。
    ///
    /// **並べ替えはしない**（シート順序はデータ。モジュール docs「シート順序は
    /// 並べ替えない」）。したがって重複の検出も `manifest.json` の索引のような昇順への
    /// 整列後の隣接比較では行えず、出現位置の収集で行う（1 ドキュメントのシート数は
    /// 高々数十であり、順序を壊さない検査の方を優先する）。
    ///
    /// 重複した識別子は**その識別子を持つ全要素の位置**（`sheets[<0 始まりの添字>]`）を
    /// 添えて報告する（要件 4.3「重複した識別子と**その出現箇所**を含むエラーとして報告」）。
    /// 出現箇所の綴りは [`crate::parts::DocumentParts`] の目録が使う
    /// `document.json sheets[i]` と揃える（[`EntryName::Document`] の表示テキスト +
    /// ` sheets[i]`。両者の語彙を一致させ、同じ違反が層によって別の綴りにならないように
    /// する）。最初に重複した（初出順で最初の）識別子について、出現順に全てを載せる。
    fn canonicalize(sheets: Vec<SheetMeta>) -> Result<Vec<SheetMeta>, DocumentError> {
        // 識別子 → その識別子を持つ全要素の添字（出現順）。件数が小さいため線形探索で足り、
        // 出現順のままの `Vec` は `HashMap` の反復順に依存しない（決定性。要件 3.6）。
        let mut occurrences: Vec<(SheetId, Vec<usize>)> = Vec::new();
        for (index, sheet) in sheets.iter().enumerate() {
            match occurrences.iter_mut().find(|(id, _)| *id == sheet.sheet_id) {
                Some((_, positions)) => positions.push(index),
                None => occurrences.push((sheet.sheet_id, vec![index])),
            }
        }

        if let Some((id, positions)) = occurrences
            .iter()
            .find(|(_, positions)| positions.len() > 1)
        {
            let locations = positions
                .iter()
                .map(|index| format!("{} sheets[{index}]", EntryName::Document))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(invalid_document(format!(
                "duplicate sheet identifier `{id}` at {locations}"
            )));
        }
        Ok(sheets)
    }
}

/// シート要素の構文レベルの読み取り結果（検証前）。
///
/// 要素内の未知キーは [`PreservedFields`] が原文の位置ごと保持する（要素 1 件につき 1 個）。
struct RawSheet {
    sheet_id: String,
    name: String,
    columns: Vec<String>,
    /// 2 回以上現れた既知キー（診断のための記録。件数を 1 対 1 に保つため拒否する）。
    duplicate_known: Option<&'static str>,
    preserved: PreservedFields,
}

impl<'de> Deserialize<'de> for RawSheet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RawSheetVisitor)
    }
}

/// シート要素（`{"sheet_id":..,"name":..,"columns":[..]}`）を読む訪問者。
struct RawSheetVisitor;

impl<'de> Visitor<'de> for RawSheetVisitor {
    type Value = RawSheet;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a document part sheet element")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawSheet, A::Error> {
        let mut sheet_id: Option<String> = None;
        let mut name: Option<String> = None;
        let mut columns: Option<Vec<String>> = None;
        let mut duplicate_known: Option<&'static str> = None;
        let mut preserved = PreservedFields::new();

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                SHEET_ID_KEY => {
                    if sheet_id.is_some() {
                        duplicate_known = Some(SHEET_ID_KEY);
                    }
                    sheet_id = Some(map.next_value()?);
                    // 書き出し側の `write_known` と同じ順序で件数を進める（必須条件）。
                    preserved.record_known_field();
                }
                NAME_KEY => {
                    if name.is_some() {
                        duplicate_known = Some(NAME_KEY);
                    }
                    name = Some(map.next_value()?);
                    preserved.record_known_field();
                }
                COLUMNS_KEY => {
                    if columns.is_some() {
                        duplicate_known = Some(COLUMNS_KEY);
                    }
                    columns = Some(map.next_value()?);
                    preserved.record_known_field();
                }
                _ => preserved.capture(&key, &mut map)?,
            }
        }

        let Some(sheet_id) = sheet_id else {
            return Err(de::Error::missing_field(SHEET_ID_KEY));
        };
        let Some(name) = name else {
            return Err(de::Error::missing_field(NAME_KEY));
        };
        // `columns` は確定形の必須キーである（列名 0 個でも空配列として常に書く。
        // モジュール docs「JSON 形式」）。欠落を空列として受け入れると、確定形が 2 つに
        // 分かれて往復のバイト同一性が条件付きになる。
        let Some(columns) = columns else {
            return Err(de::Error::missing_field(COLUMNS_KEY));
        };

        Ok(RawSheet {
            sheet_id,
            name,
            columns,
            duplicate_known,
            preserved,
        })
    }
}

/// `document.json` 全体の構文レベルの読み取り結果（検証前）。
///
/// 未知のトップレベルキーは [`PreservedFields`] が原文の位置ごと保持する。
struct RawDocument {
    document_id: Option<String>,
    sheets: Option<Vec<RawSheet>>,
    /// 2 回以上現れた既知キー（診断のための記録。件数を 1 対 1 に保つため拒否する）。
    duplicate_known: Option<&'static str>,
    preserved: PreservedFields,
}

impl<'de> Deserialize<'de> for RawDocument {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RawDocumentVisitor)
    }
}

/// `document.json` のトップレベルを読む訪問者（既知キー `document_id` / `sheets` だけを
/// 解釈し、他は [`PreservedFields`] へ回す）。
struct RawDocumentVisitor;

impl<'de> Visitor<'de> for RawDocumentVisitor {
    type Value = RawDocument;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a document part object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawDocument, A::Error> {
        let mut document_id: Option<String> = None;
        let mut sheets: Option<Vec<RawSheet>> = None;
        let mut duplicate_known: Option<&'static str> = None;
        let mut preserved = PreservedFields::new();

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                DOCUMENT_ID_KEY => {
                    if document_id.is_some() {
                        duplicate_known = Some(DOCUMENT_ID_KEY);
                    }
                    document_id = Some(map.next_value()?);
                    // 書き出し側の `write_known` と同じ順序で既知フィールドの件数を
                    // 進める（未知フィールドの差し戻し位置の基準。タスク 3.2 の必須条件）。
                    preserved.record_known_field();
                }
                SHEETS_KEY => {
                    if sheets.is_some() {
                        duplicate_known = Some(SHEETS_KEY);
                    }
                    sheets = Some(map.next_value()?);
                    preserved.record_known_field();
                }
                _ => preserved.capture(&key, &mut map)?,
            }
        }

        Ok(RawDocument {
            document_id,
            sheets,
            duplicate_known,
            preserved,
        })
    }
}

/// ドキュメント識別子を正準形（26 文字 Crockford base32 大文字）に限って解析する。
///
/// [`DocumentId`] の `FromStr` は `ulid` デコーダの性質により大小文字を問わないため、
/// 正準形との一致をここで検査する（モジュール docs「JSON 形式」。シート識別子も同じ規則）。
fn parse_canonical_document_id(text: &str) -> Result<DocumentId, DocumentError> {
    match text.parse::<DocumentId>() {
        Ok(id) if id.to_string() == text => Ok(id),
        _ => Err(invalid_document(format!(
            "invalid document identifier `{text}`"
        ))),
    }
}

/// シート識別子を正準形（26 文字 Crockford base32 大文字）に限って解析する。
fn parse_canonical_sheet_id(text: &str) -> Result<SheetId, DocumentError> {
    match text.parse::<SheetId>() {
        Ok(id) if id.to_string() == text => Ok(id),
        _ => Err(invalid_document(format!(
            "invalid sheet identifier `{text}`"
        ))),
    }
}

/// 読み込みのコンテンツ解析失敗を [`DocumentError::InvalidContainer`] へ写す。
///
/// design のエラー表に本モジュール固有の変種が無いための写像であり、`entry` に
/// `document.json: <理由>` を残す（[`crate::value`] / [`crate::json::determinism`] と
/// 同じ規約）。エントリ名は [`EntryName::Document`] の表示テキストを使う
/// （文字列リテラルを経路へ散在させない）。
fn invalid_document(reason: impl fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("{}: {reason}", EntryName::Document),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    /// 標本のドキュメント識別子（正準 Crockford base32 大文字 26 文字）。
    const DOC_ID_TEXT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    /// 標本のシート識別子。`SHEET_IDS[i] < SHEET_IDS[i + 1]`（ULID 昇順 = テキストの
    /// 辞書順）であり、入力順と昇順を食い違わせて順序規則を実測できるようにしてある。
    const SHEET_IDS: [&str; 5] = [
        "01K4ANRRG004HMASW9NF6YY093",
        "01K4ANRSF804HMASW9QKFG04HM",
        "01K4ANRTEG04HMASW9SQR128T5",
        "01K4ANRVDR04HMASW9VW0J4D2P",
        "01K4ANRWD004HMASW9Y0936HB7",
    ];

    /// 標本のシート名。**正規化・サニタイズの対象にならない**入力を選ぶ: 非 ASCII
    /// （日本語）・絵文字・空文字列・前後の空白・制御文字（改行とタブ）・引用符。
    /// 添字は [`SHEET_IDS`] と対応する（名前の辞書順は識別子の昇順と一致しない）。
    const SHEET_NAMES: [&str; 5] = [
        "在庫",
        "📊 データ",
        "",
        "  前後空白  ",
        "改行\nと\tタブと\"引用符\"",
    ];

    /// 確定形のキー集合（宣言順）。出力に時刻・実行環境・乱数由来の項目が無いことを
    /// 回帰として固定する（要件 3.6）。
    const TOP_LEVEL_KEYS: [&str; 2] = ["document_id", "sheets"];
    const SHEET_KEYS: [&str; 3] = ["sheet_id", "name", "columns"];

    /// 標本のシート識別子 1 個。
    fn sheet_id(index: usize) -> SheetId {
        SHEET_IDS[index].parse().expect("標本は正準 ULID")
    }

    /// 標本の列名（順序付き。`index` ごとに異なる並びにして、列名の取り違えを検出できる
    /// ようにする。空のシートも含む）。
    fn columns(index: usize) -> Vec<String> {
        match index % 3 {
            0 => ["a", "b"].iter().map(|name| (*name).to_string()).collect(),
            1 => Vec::new(),
            _ => ["量", "$id", "notes"]
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        }
    }

    /// 標本のシートのメタデータ 1 件。
    fn sheet(index: usize) -> SheetMeta {
        SheetMeta::new(sheet_id(index), SHEET_NAMES[index].to_owned()).with_columns(columns(index))
    }

    /// 標本のシートのメタデータを `order` の添字順に並べる。
    fn sheets(order: &[usize]) -> Vec<SheetMeta> {
        order.iter().copied().map(sheet).collect()
    }

    fn document_id() -> DocumentId {
        DOC_ID_TEXT.parse().expect("標本は正準 ULID")
    }

    /// 標本のドキュメントパート（`order` の添字順のシート順序）。
    fn part(order: &[usize]) -> DocumentPart {
        DocumentPart::new(document_id(), sheets(order)).expect("標本は妥当")
    }

    /// 内容の比較可能な写像。
    ///
    /// [`DocumentPart`] / [`SheetMeta`] は保持している未知フィールド（内部カーソルを
    /// 含む）を等値比較に持ち込まないため `PartialEq` を持たない。内容の比較はこの写像か
    /// 符号化したバイト列で行う。
    fn fingerprint(part: &DocumentPart) -> (DocumentId, Vec<(SheetId, String, Vec<String>)>) {
        (
            part.document_id(),
            part.sheets()
                .iter()
                .map(|sheet| {
                    (
                        sheet.sheet_id(),
                        sheet.name().to_owned(),
                        sheet.columns().to_vec(),
                    )
                })
                .collect(),
        )
    }

    fn ids(part: &DocumentPart) -> Vec<SheetId> {
        part.sheets().iter().map(SheetMeta::sheet_id).collect()
    }

    fn names(part: &DocumentPart) -> Vec<String> {
        part.sheets()
            .iter()
            .map(|sheet| sheet.name().to_owned())
            .collect()
    }

    /// 確定形のシート要素 1 件（キー名と順序をテスト側の定数で組み立てる。
    /// 実装の経路には依存しない）。
    fn element_json(id_text: &str, name: &str, columns: &[String]) -> String {
        let id_key = SHEET_KEYS[0];
        let name_key = SHEET_KEYS[1];
        let columns_key = SHEET_KEYS[2];
        let quoted = serde_json::to_string(name).expect("シート名のエスケープ");
        let encoded = serde_json::to_string(columns).expect("列名のエスケープ");
        format!(r#"{{"{id_key}":"{id_text}","{name_key}":{quoted},"{columns_key}":{encoded}}}"#)
    }

    /// 確定形の全体（キー `document_id` → `sheets` の順、要素は与えられた順のまま）。
    fn expected_json(id_text: &str, order: &[usize]) -> String {
        let id_key = TOP_LEVEL_KEYS[0];
        let sheets_key = TOP_LEVEL_KEYS[1];
        let elements: Vec<String> = order
            .iter()
            .map(|&index| element_json(SHEET_IDS[index], SHEET_NAMES[index], &columns(index)))
            .collect();
        format!(
            r#"{{"{id_key}":"{id_text}","{sheets_key}":[{}]}}"#,
            elements.join(",")
        )
    }

    /// 往復同一: `DocumentPart` → JSON → `DocumentPart` が内容一致し、バイト列も変わらない。
    /// 5 枚のシート（非 ASCII・絵文字・空文字列・前後の空白・制御文字を含む）と 2 通りの
    /// 順序で確かめる（要件 2.2, 3.1, 3.6）。
    #[test]
    fn encodes_and_decodes_byte_identically_and_deterministically() {
        for order in [vec![0, 1, 2, 3, 4], vec![4, 2, 0, 3, 1]] {
            let part = part(&order);
            let bytes = part.to_json_bytes().expect("符号化");

            assert_eq!(
                bytes,
                part.to_json_bytes().expect("符号化"),
                "同一内容の 2 回の符号化がバイト一致しない"
            );
            assert_eq!(
                expected_json(DOC_ID_TEXT, &order).as_bytes(),
                bytes.as_slice(),
                "確定形が変わっている"
            );

            let decoded = DocumentPart::from_json_bytes(&bytes).expect("復号");
            assert_eq!(
                fingerprint(&part),
                fingerprint(&decoded),
                "復号で内容が変わった"
            );
            // シート名は正規化もサニタイズもされない（空文字列・前後の空白・制御文字）
            let expected_names: Vec<String> = order
                .iter()
                .map(|&index| SHEET_NAMES[index].to_owned())
                .collect();
            assert_eq!(expected_names, names(&decoded), "シート名が書き換わった");
            assert_eq!(
                bytes,
                decoded.to_json_bytes().expect("再符号化"),
                "往復でバイト列が変わった"
            );
        }
    }

    /// シート順序は**与えられた順序のまま**である（要件 1.1 のモデル順。索引である
    /// `manifest.json` のような並べ替えをしない）。
    ///
    /// 2 通りの入力順（いずれも ULID 昇順ともシート名の辞書順とも異なる）で、出力配列の
    /// 順序が入力順と一致することを確かめる。シート 1 枚の検証では順序規則の回帰を
    /// 検出できないため、5 枚を分散させ、さらに「入力順が違えば出力も違う」こと
    /// （順序を捨てて並べ替えていたら一致してしまう）を確かめる。
    #[test]
    fn sheet_order_is_preserved_verbatim() {
        let ascending = vec![0, 1, 2, 3, 4];
        let orders = [vec![4, 2, 0, 3, 1], vec![2, 4, 1, 0, 3]];
        // シート名の辞書順（UTF-8 バイト順）も入力順とは異なる。
        let by_name = {
            let mut indices = ascending.clone();
            indices.sort_by_key(|&index| SHEET_NAMES[index]);
            indices
        };
        assert_ne!(
            ascending, by_name,
            "標本の識別子順とシート名の辞書順が一致している"
        );
        let mut encodings = Vec::new();

        for order in &orders {
            assert_ne!(
                ascending, *order,
                "標本の入力順が既に ULID 昇順で、順序の検証にならない"
            );
            assert_ne!(
                by_name, *order,
                "標本の入力順がシート名の辞書順と一致し、順序の検証にならない"
            );
            let expected_ids: Vec<SheetId> = order.iter().map(|&index| sheet_id(index)).collect();

            let part = part(order);
            assert_eq!(
                expected_ids,
                ids(&part),
                "シート順序が入力順と違う（並べ替えられた）"
            );

            let bytes = part.to_json_bytes().expect("符号化");
            let decoded = DocumentPart::from_json_bytes(&bytes).expect("復号");
            assert_eq!(expected_ids, ids(&decoded), "復号でシート順序が変わった");
            encodings.push(bytes);
        }

        assert_ne!(
            encodings[0], encodings[1],
            "入力順の違いが出力へ現れていない（順序が捨てられた可能性がある）"
        );
    }

    /// 符号化は入力の**純関数**であり、揮発値を含まない（要件 3.6）。
    ///
    /// 実時刻を進めてから再符号化しても出力は 1 バイトも変わらず、確定形（キー集合を
    /// 列挙したテスト側の組み立て）と一致し続ける。別の構築経路（復号 → 再符号化）でも
    /// 同じバイト列になる。
    #[test]
    fn encoding_is_a_pure_function_free_of_volatile_values() {
        let order = vec![3, 1, 4, 0, 2];
        let part = part(&order);
        let before = SystemTime::now();
        let first = part.to_json_bytes().expect("符号化");

        // 実時刻を進める（ミリ秒の桁が変わるまで待つ）。保存時刻のような揮発値を
        // 出力へ混ぜていれば、ここで差分が出る。
        std::thread::sleep(Duration::from_millis(2));
        assert!(
            SystemTime::now() > before,
            "時刻が進んでおらず、時刻依存の検証になっていない"
        );

        let second = part.to_json_bytes().expect("符号化");
        assert_eq!(
            first, second,
            "時刻を進めたら出力が変わった（時刻が漏れている）"
        );
        assert_eq!(
            expected_json(DOC_ID_TEXT, &order).as_bytes(),
            first.as_slice(),
            "確定形が変わっている（キー集合に時刻・環境・乱数由来の項目が混ざった可能性）"
        );

        let decoded = DocumentPart::from_json_bytes(&first).expect("復号");
        assert_eq!(first, decoded.to_json_bytes().expect("再符号化"));
    }

    /// 確定形: キー名・キー順序・識別子のテキスト表記・列名の配列を文字列リテラルで固定する
    /// （モジュール docs「JSON 形式」の確定形そのもの。task 5.2 / 8.2 のゴールデンの基準）。
    #[test]
    fn wire_form_is_the_documented_confirmed_shape() {
        let part = DocumentPart::new(
            document_id(),
            vec![SheetMeta::new(sheet_id(0), "在庫".to_owned()), sheet(1)],
        )
        .expect("標本は妥当");
        let text = String::from_utf8(part.to_json_bytes().expect("符号化")).expect("UTF-8");
        assert_eq!(
            concat!(
                r#"{"document_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","sheets":["#,
                r#"{"sheet_id":"01K4ANRRG004HMASW9NF6YY093","name":"在庫","columns":[]},"#,
                r#"{"sheet_id":"01K4ANRSF804HMASW9QKFG04HM","name":"📊 データ","columns":[]}"#,
                r#"]}"#
            ),
            text,
            "確定形が変わっている"
        );

        // 0 枚のシートも正当である（要件 1.1: 0 個以上のシート）。
        let empty = DocumentPart::new(document_id(), Vec::new()).expect("空のシート列は正当");
        let text = String::from_utf8(empty.to_json_bytes().expect("符号化")).expect("UTF-8");
        assert_eq!(
            r#"{"document_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","sheets":[]}"#, text,
            "空のシート列の確定形が変わっている"
        );
        let decoded = DocumentPart::from_json_bytes(text.as_bytes()).expect("復号");
        assert_eq!(0, decoded.sheets().len());
        assert_eq!(document_id(), decoded.document_id());
    }

    /// 列名は確定形の必須キーであり、**空配列として常に書かれる**（タスク 4.8 で追加）。
    /// 列名の並びは与えられた順のままで、並べ替えも正規化もされない（本クレートは列名の
    /// 中身を解釈しない）。0 個・複数個・非 ASCII・`$` 始まり（行データの予約キーと衝突する
    /// 名前）を分散させて確かめる。
    #[test]
    fn columns_are_mandatory_and_preserved_verbatim() {
        let expected = vec![
            ("01K4ANRRG004HMASW9NF6YY093", Vec::new()),
            (
                "01K4ANRSF804HMASW9QKFG04HM",
                vec!["zone".to_owned(), "a".to_owned()],
            ),
            (
                "01K4ANRTEG04HMASW9SQR128T5",
                vec!["$id".to_owned(), "量".to_owned()],
            ),
        ];
        let part = DocumentPart::new(
            document_id(),
            vec![
                SheetMeta::new(
                    expected[0].0.parse().expect("標本は正準 ULID"),
                    "空".to_owned(),
                )
                .with_columns(expected[0].1.clone()),
                SheetMeta::new(
                    expected[1].0.parse().expect("標本は正準 ULID"),
                    "在庫".to_owned(),
                )
                .with_columns(expected[1].1.clone()),
                SheetMeta::new(
                    expected[2].0.parse().expect("標本は正準 ULID"),
                    "予約".to_owned(),
                )
                .with_columns(expected[2].1.clone()),
            ],
        )
        .expect("標本は妥当");

        let bytes = part.to_json_bytes().expect("符号化");
        let text = String::from_utf8(bytes.clone()).expect("UTF-8");
        assert_eq!(
            concat!(
                r#"{"document_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","sheets":["#,
                r#"{"sheet_id":"01K4ANRRG004HMASW9NF6YY093","name":"空","columns":[]},"#,
                r#"{"sheet_id":"01K4ANRSF804HMASW9QKFG04HM","name":"在庫","columns":["zone","a"]},"#,
                r#"{"sheet_id":"01K4ANRTEG04HMASW9SQR128T5","name":"予約","columns":["$id","量"]}"#,
                r#"]}"#
            ),
            text,
            "列名の確定形が変わっている"
        );

        // 列名が往復で保たれ（順序も含む）、要素ごとに取り違えられない。
        let decoded = DocumentPart::from_json_bytes(&bytes).expect("復号");
        let observed: Vec<Vec<String>> = decoded
            .sheets()
            .iter()
            .map(|sheet| sheet.columns().to_vec())
            .collect();
        let wanted: Vec<Vec<String>> = expected
            .iter()
            .map(|(_, columns)| columns.clone())
            .collect();
        assert_eq!(wanted, observed, "列名が往復で変わった");
        assert_eq!(bytes, decoded.to_json_bytes().expect("再符号化"));

        // `columns` の欠落は確定形の逸脱として拒否する（空列として黙って受け入れない）。
        let without_columns = text.replace(r#","columns":[]"#, "");
        assert_ne!(
            text, without_columns,
            "標本に空の列名が無く、欠落の検証にならない"
        );
        match DocumentPart::from_json_bytes(without_columns.as_bytes()) {
            Err(DocumentError::InvalidContainer { entry }) => {
                assert!(
                    entry.starts_with("document.json: "),
                    "entry が違う: {entry}"
                );
            }
            other => panic!("列名の欠落が拒否されない: {other:?}"),
        }
    }

    /// 未知フィールド保持: 未知キーがトップレベルに現れても（`document_id` の前・
    /// `document_id` と `sheets` の間・`sheets` の後ろ）、読み→書き戻しが**バイト単位**に
    /// 元へ戻る（要件 6.2 / 6.3）。
    #[test]
    fn unknown_top_level_fields_round_trip_byte_for_byte() {
        let elements = format!(
            "{},{}",
            element_json(SHEET_IDS[0], SHEET_NAMES[0], &columns(0)),
            element_json(SHEET_IDS[1], SHEET_NAMES[1], &columns(1))
        );
        let cases = [
            // 前（既知フィールドの手前）
            format!(
                r#"{{"format_note":"future","document_id":"{DOC_ID_TEXT}","sheets":[{elements}]}}"#
            ),
            // 間（document_id と sheets のあいだ）
            format!(
                r#"{{"document_id":"{DOC_ID_TEXT}","git":{{"filter":"jxcel"}},"sheets":[{elements}]}}"#
            ),
            // 後ろ（既知フィールドの後ろ）
            format!(
                r#"{{"document_id":"{DOC_ID_TEXT}","sheets":[{elements}],"created_with":{{"app":"jxcel"}}}}"#
            ),
        ];

        for input in cases {
            let decoded = DocumentPart::from_json_bytes(input.as_bytes()).expect("復号");
            assert_eq!(document_id(), decoded.document_id());
            assert_eq!(
                2,
                decoded.sheets().len(),
                "未知キーが既知フィールドを隠している"
            );
            let written = decoded.to_json_bytes().expect("再符号化");
            assert_eq!(
                input.as_bytes(),
                written.as_slice(),
                "未知フィールドが往復で消えたか位置が変わった: {input}"
            );
        }
    }

    /// 要素内の未知フィールド保持: シート要素の中の未知キーが「先頭・間・末尾」のどこに
    /// あっても、読み→書き戻しが**バイト単位**に一致する（要件 6.2 / 6.3。将来の minor が
    /// シートのメタデータへ足した省略可能フィールドが消えない）。
    ///
    /// 未知キーを持つ要素 3 つと持たない要素 1 つを混ぜる（1 要素だけの検証では、要素ごとの
    /// 保持が壊れても検出できない）。
    #[test]
    fn unknown_keys_inside_sheet_elements_round_trip_byte_for_byte() {
        let name_a = serde_json::to_string(SHEET_NAMES[0]).expect("エスケープ");
        let name_b = serde_json::to_string(SHEET_NAMES[1]).expect("エスケープ");
        let name_c = serde_json::to_string(SHEET_NAMES[2]).expect("エスケープ");
        // 先頭（sheet_id の手前）と間（sheet_id と name のあいだ。列名は末尾）
        let first = format!(
            r#"{{"future_display":"wide","sheet_id":"{}","declared_rows":3,"name":{name_a},"columns":["a","b"]}}"#,
            SHEET_IDS[0]
        );
        // 間（sheet_id と name のあいだ）。列名は空配列（必須キー）。
        let second = format!(
            r#"{{"sheet_id":"{}","color":"magenta","name":{name_b},"columns":[]}}"#,
            SHEET_IDS[1]
        );
        // 末尾（columns の後ろ）
        let third = format!(
            r#"{{"sheet_id":"{}","name":{name_c},"columns":["$id","量"],"version_added":2}}"#,
            SHEET_IDS[2]
        );
        // 未知キーを持たない要素
        let fourth = element_json(SHEET_IDS[3], SHEET_NAMES[3], &columns(3));

        let input = format!(
            r#"{{"document_id":"{DOC_ID_TEXT}","sheets":[{first},{second},{third},{fourth}]}}"#
        );

        let decoded = DocumentPart::from_json_bytes(input.as_bytes()).expect("復号");
        assert_eq!(4, decoded.sheets().len());
        // 未知キーは既知フィールドを隠さない（シートのメタデータとして正しく読める）。
        let expected_ids: Vec<SheetId> = (0..4).map(sheet_id).collect();
        assert_eq!(expected_ids, ids(&decoded));
        assert_eq!(SHEET_NAMES[0], decoded.sheets()[0].name());

        let written = decoded.to_json_bytes().expect("再符号化");
        assert_eq!(
            input.as_bytes(),
            written.as_slice(),
            "要素内の未知フィールドが往復で消えたか位置が変わった: {input}"
        );

        // raw 値の差し込みを含む再符号化が安定している（復号 → 符号化を繰り返しても同じ）。
        let redecoded = DocumentPart::from_json_bytes(&written).expect("復号");
        assert_eq!(written, redecoded.to_json_bytes().expect("再符号化"));
        assert_eq!(fingerprint(&decoded), fingerprint(&redecoded));
    }

    /// 不正入力はすべて [`DocumentError::InvalidContainer`] で拒否され、`entry` に本パートの
    /// エントリ名と理由が載る（モジュール docs のエラー表）。
    #[test]
    fn invalid_document_inputs_are_rejected_as_invalid_container() {
        let id = DOC_ID_TEXT;
        let a = SHEET_IDS[0];
        let b = SHEET_IDS[1];
        let element = element_json(a, SHEET_NAMES[0], &columns(0));
        let lowercase_id = id.to_lowercase();
        let lowercase_sheet = a.to_lowercase();
        let short_id = &id[..25];
        let not_ulid = "not-a-ulid";

        let cases: Vec<(&str, String)> = vec![
            ("JSON として不正", "{".to_owned()),
            ("トップレベルがオブジェクトでない", "[]".to_owned()),
            ("トップレベルが空", String::new()),
            ("document_id 欠落", format!(r#"{{"sheets":[{element}]}}"#)),
            ("sheets 欠落", format!(r#"{{"document_id":"{id}"}}"#)),
            (
                "document_id が数値",
                format!(r#"{{"document_id":7,"sheets":[]}}"#),
            ),
            (
                "document_id が null",
                format!(r#"{{"document_id":null,"sheets":[]}}"#),
            ),
            (
                "document_id が ULID でない",
                format!(r#"{{"document_id":"{not_ulid}","sheets":[]}}"#),
            ),
            (
                "document_id が 25 文字",
                format!(r#"{{"document_id":"{short_id}","sheets":[]}}"#),
            ),
            (
                "document_id が小文字表記",
                format!(r#"{{"document_id":"{lowercase_id}","sheets":[]}}"#),
            ),
            (
                "sheets が配列でない",
                format!(r#"{{"document_id":"{id}","sheets":{{}}}}"#),
            ),
            (
                "sheets が null",
                format!(r#"{{"document_id":"{id}","sheets":null}}"#),
            ),
            (
                "シート要素がオブジェクトでない",
                format!(r#"{{"document_id":"{id}","sheets":["{a}"]}}"#),
            ),
            (
                "シート要素が配列",
                format!(r#"{{"document_id":"{id}","sheets":[[1,2]]}}"#),
            ),
            (
                "シート要素に sheet_id が無い",
                format!(r#"{{"document_id":"{id}","sheets":[{{"name":"x"}}]}}"#),
            ),
            (
                "シート要素に name が無い",
                format!(r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{a}"}}]}}"#),
            ),
            (
                "シート要素に columns が無い（必須キー）",
                format!(r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{a}","name":"x"}}]}}"#),
            ),
            (
                "columns が配列でない",
                format!(
                    r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{a}","name":"x","columns":"a"}}]}}"#
                ),
            ),
            (
                "columns の要素が文字列でない",
                format!(
                    r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{a}","name":"x","columns":[1]}}]}}"#
                ),
            ),
            (
                "columns が null",
                format!(
                    r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{a}","name":"x","columns":null}}]}}"#
                ),
            ),
            (
                "既知キーの重複（シート要素の columns）",
                format!(
                    r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{a}","name":"x","columns":[],"columns":[]}}]}}"#
                ),
            ),
            (
                "シート名が文字列でない",
                format!(r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{a}","name":7}}]}}"#),
            ),
            (
                "シート識別子が ULID でない",
                format!(
                    r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{not_ulid}","name":"x"}}]}}"#
                ),
            ),
            (
                "シート識別子が小文字表記",
                format!(
                    r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{lowercase_sheet}","name":"x"}}]}}"#
                ),
            ),
            (
                "シート識別子が重複",
                format!(r#"{{"document_id":"{id}","sheets":[{element},{element}]}}"#),
            ),
            (
                "シート識別子が離れて重複",
                format!(
                    r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{a}","name":"x"}},{{"sheet_id":"{b}","name":"y"}},{{"sheet_id":"{a}","name":"z"}}]}}"#
                ),
            ),
            (
                "既知キーの重複（トップレベル）",
                format!(r#"{{"document_id":"{id}","document_id":"{id}","sheets":[]}}"#),
            ),
            (
                "既知キーの重複（シート要素）",
                format!(
                    r#"{{"document_id":"{id}","sheets":[{{"sheet_id":"{a}","sheet_id":"{a}","name":"x"}}]}}"#
                ),
            ),
        ];

        for (why, input) in cases {
            match DocumentPart::from_json_bytes(input.as_bytes()) {
                Err(DocumentError::InvalidContainer { entry }) => {
                    assert!(
                        entry.starts_with("document.json: "),
                        "{why}: entry が document.json で始まらない: {entry}"
                    );
                    assert!(
                        entry.len() > "document.json: ".len(),
                        "{why}: entry に理由が無い: {entry}"
                    );
                }
                other => panic!("{why}: InvalidContainer 以外が返った: {other:?}"),
            }
        }
    }

    /// 構築経路（[`DocumentPart::new`]）も復号と同じ不変条件を強制する: 同一ファイル内で
    /// シート識別子が重複するドキュメントは不正である（4.2 の重複エントリ名拒否と同じ分類。
    /// 全体の一意性検証はタスク 4.6 の担当であり、ここでは自己矛盾だけを拒む）。
    #[test]
    fn constructor_rejects_duplicate_sheet_identifiers() {
        let distinct_names = vec![
            sheet(0),
            SheetMeta::new(sheet_id(0), "別名".to_owned()),
            sheet(1),
        ];

        let duplicate_cases = [
            vec![sheet(0), sheet(0)],
            vec![sheet(0), sheet(1), sheet(0)],
            distinct_names,
        ];

        for sheets in duplicate_cases {
            assert!(
                matches!(
                    DocumentPart::new(document_id(), sheets),
                    Err(DocumentError::InvalidContainer { .. })
                ),
                "重複したシート識別子が拒否されない"
            );
        }
    }
}

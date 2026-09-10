//! JSON の書き出し口と未知フィールドの保持（タスク 3.1 / 3.2。要件 2.4, 3.3, 3.6, 6.2, 6.3）。
//!
//! 出力の規則は親モジュール [`crate::json`] の docs にある。本ファイルは
//! **書き出し口 3 つ**と**未知フィールド保持のプリミティブ**を持つ:
//!
//! | 書き出し口 | 対象 | キー順序 |
//! |------------|------|----------|
//! | [`write_json`] | `Serialize` を実装する型（構造体） | フィールド宣言順 |
//! | [`write_ordered_object`] | 動的な列集合（キーと [`CellValue`] の列） | 呼び出し元が与えた順序 |
//! | [`write_cell`] | セル値 1 個 | （キーを持たない） |
//! | [`PreservingObjectWriter`] | 未知フィールドを保持するバージョン付きパート | 既知フィールドは宣言順、未知フィールドは原文の位置 |
//!
//! # 未知フィールドの保持（前方互換。要件 6.2 / 6.3）
//!
//! バージョン付きのパート構造体（4.2 以降）は、自分のバージョンが解釈しない
//! フィールドを破棄してはならない。落とすと、新しい読み手が書いた minor 追加の
//! フィールドが古い読み手の往復で消え、移行チェーン（6.x）が「消えたフィールドを
//! 復元する」作業を強いられる。読み込み側は [`PreservedFields`] がキーと値の
//! **原文のバイト列**を保持し、書き出し側は [`PreservingObjectWriter`] が
//! **原文の位置**へ差し戻す。この 2 つが保持プリミティブの公開面である。
//!
//! 位置の表現方式は**前方カウント方式**（「その未知フィールドより前に現れた既知
//! フィールドの件数」。入力キー列の絶対位置は保存しない）であり、件数は取り込み順に
//! **単調非減少**である。この不変条件は集合の内側で保たれる（方式の理由と往復の
//! 成立範囲は [`PreservedFields`] の docs に明記）。
//!
//! スキーマ・エンベロープ（`src/model/schema_part.rs`）は、この一般形を
//! **エンベロープの全階層**（トップレベルと各型定義要素）でそのまま使う。タスク 2.2 は
//! エンベロープ・トップレベル限定の特殊化をモデル層に持っていたが、タスク 4.4 で本ファイルの
//! プリミティブへ一本化した（二重実装は残っていない）。`schemas/<sheet-ulid>.json` の
//! 最終的なバイト再構成はタスク 4.4 の `parts::schema_codec::SchemaCodec` が
//! [`PreservingObjectWriter`] で行う。
//!
//! # 失敗時の保証
//!
//! 3 つとも「失敗したら書き出し先へ 1 バイトも書かない」を守る。セル値の検査は
//! [`crate::value::to_json_bytes`]（ツリー全体の非有限値走査）が、構造体と列集合は
//! 一時バッファが担う。これは要件 3.3 系「NaN と Infinity は ... 拒否し、無効な
//! JSON を書かない」を部分的な出力にまで広げたものである。
//!
//! # エラー対応
//!
//! - セル値の NaN / Infinity: [`crate::value::to_json_bytes`] が返す
//!   [`DocumentError::NonRepresentableNumber`] をそのまま伝える。`location` は
//!   呼び出し元が与えたセル位置であり、列集合では `<location> column <キー>` に
//!   組み立てる（原因セルが特定できる）。数値の判定は本モジュールでは行わない。
//! - 直列化の失敗（`Serialize` 実装が返すエラー）: design 表に固有の応答変種が
//!   無いため [`DocumentError::InvalidContainer`] に写し、`entry` へ `json: <理由>`
//!   を残す（[`crate::value`] の `parse_error` と同じ規約）。[`CellValue`] として
//!   NaN が届いた場合はセル位置を運べないこの経路で型付きエラーにできない
//!   （セル単位の書き出し口を使えば [`DocumentError::NonRepresentableNumber`] に
//!   なる）。生の `f64` はさらに悪く、**エラーにならず `null` として書かれる**
//!   （[`write_json`] の docs を参照 — 出力する構造体に生の `f64` を置かないこと）。
//!   `InvalidContainer` の `entry` は本来エントリ名だが、design 表に
//!   対応変種が無い失敗の理由を載せる場所として [`crate::value`] と同じ使い方を
//!   している。
//! - 入出力の失敗: [`DocumentError::Io`]。`retried` は rename リトライの枯渇を
//!   表すフィールドであり、本モジュールは常に `false` を渡す（呼び出し元の
//!   保存経路だけが `true` を構築し得る）。

use core::fmt;
use std::io::{self, Write};

use serde::de::MapAccess;
use serde::Serialize;
use serde_json::value::RawValue;

use crate::error::DocumentError;
use crate::value::{self, CellValue};

/// `Serialize` を実装する型（構造体）を 1 つの JSON 値として書き出す。
///
/// キー順序は **フィールド宣言順**である（`serde` の derive は宣言順に
/// `serialize_field` を呼ぶ。辞書順にはならない）。出力はコンパクトな UTF-8 で、
/// 末尾改行を付与しない。失敗したときは `out` へ 1 バイトも書かない。
///
/// 数値の正規化（`-0.0` は `0`、非有限値の拒否）は値の型の `Serialize` 実装が
/// 担う: [`CellValue`] を含む型では [`crate::value`] の唯一の実装が適用され、
/// `-0.0` は符号なしで書かれ、NaN / Infinity は直列化エラーになる（本関数はそれを
/// [`DocumentError::InvalidContainer`] に写す）。
///
/// **生の `f64` をフィールドに持つ型は保護されない。** `serde_json` の
/// `serialize_f64` は非有限値をエラーにせず **`null` として書く**（`NaN` /
/// `+Infinity` / `-Infinity` のいずれも `{"n":null}` になり、書き出しは成功する）。
/// 本関数は型の `Serialize` 実装を書き換えないため、これを検出も遮断もできない。
/// これは [`crate::value`] が禁じている「NaN を `null` へ黙って変換する」のと同型の
/// 落とし穴であり、**出力する構造体に生の `f64` を置いてはならない**という制約が
/// 呼び出し元の責務である（数値は必ず [`CellValue`] として運び、非有限値を
/// [`DocumentError::NonRepresentableNumber`] として拒否させる）。`-0.0` も生の
/// `f64` では符号付きのまま書かれる（要件 3.3, 3.6 の正規化は [`CellValue`] 経路で
/// のみ成立する）。
pub fn write_json<W: Write, T: Serialize + ?Sized>(
    out: &mut W,
    value: &T,
) -> Result<(), DocumentError> {
    // 直列化の途中で失敗しても書き出し先へ部分的な JSON を残さないため、
    // 一時バッファへ組み立ててから書き出す（モジュール docs「失敗時の保証」）。
    let mut buffer = Vec::new();
    serde_json::to_writer(&mut buffer, value).map_err(|err| write_error(&err))?;
    out.write_all(&buffer).map_err(io_error)
}

/// 動的な列集合を 1 つの JSON オブジェクトとして、**与えられた順序のまま**書き出す。
///
/// `entries` は「キーの順序付き列 + 値の列」であり、出力のキー順序はこの列と
/// 完全に一致する。**並べ替え・ソート・辞書順化・キーの重複検査を一切しない**
/// （順序はスキーマ側 = 呼び出し元が決める。親モジュール docs「列順序は呼び出し元が
/// 与える」）。本クレートはスキーマを解釈しないため、列順序を自分で決める経路を
/// 持たない。
///
/// `location` は行などの文脈（例 `sheet <ULID> row <ULID>`）であり、失敗したセルの
/// 位置は `<location> column <キー>` としてエラーに載る。出力はコンパクトな UTF-8 の
/// 1 オブジェクトで、末尾改行を付与しない。失敗したときは `out` へ 1 バイトも
/// 書かない（列の途中で失敗しても部分的なオブジェクトを残さない）。
pub fn write_ordered_object<W: Write>(
    out: &mut W,
    location: &str,
    entries: &[(&str, &CellValue)],
) -> Result<(), DocumentError> {
    // 列の途中で失敗しても部分的なオブジェクトを書き出し先へ残さないため、
    // 一時バッファへ組み立てる。
    let mut buffer = Vec::new();
    // セル位置の組み立てに使う。セルごとに作り直さず容量を使い回す。
    let mut cell_location = String::new();

    buffer.push(b'{');
    for (index, &(key, value)) in entries.iter().enumerate() {
        if index > 0 {
            buffer.push(b',');
        }
        // キーのエスケープ規則は serde_json に任せる（本モジュールは自前の
        // エスケープ規則を持たない）。
        serde_json::to_writer(&mut buffer, key).map_err(|err| write_error(&err))?;
        buffer.push(b':');

        cell_location.clear();
        cell_location.push_str(location);
        cell_location.push_str(CELL_LOCATION_SEPARATOR);
        cell_location.push_str(key);
        buffer.extend_from_slice(&value::to_json_bytes(value, &cell_location)?);
    }
    buffer.push(b'}');

    out.write_all(&buffer).map_err(io_error)
}

/// セル値 1 個を書き出す（数値の正規化と非有限値の遮断は [`crate::value`] に委譲）。
///
/// 本関数はセル値の wire 表現を **再実装しない**: 出力バイト列は
/// [`crate::value::to_json_bytes`] の戻り値そのものである（`-0.0` は `0.0`、
/// 非有限値は [`DocumentError::NonRepresentableNumber`] と `location`）。
/// 失敗したときは `out` へ 1 バイトも書かない。
pub fn write_cell<W: Write>(
    out: &mut W,
    value: &CellValue,
    location: &str,
) -> Result<(), DocumentError> {
    let bytes = value::to_json_bytes(value, location)?;
    out.write_all(&bytes).map_err(io_error)
}

/// セル位置の文脈にキーを繋ぐ区切り（`<行の文脈> column <キー>`）。
const CELL_LOCATION_SEPARATOR: &str = " column ";

/// 直列化の失敗をクレート共通エラーへ写す（[`crate::value`] の `parse_error` と
/// 同じ規約: design 表に固有の応答変種が無い失敗は理由を文字列で残す）。
fn write_error(reason: &dyn fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("json: {reason}"),
    }
}

/// 書き出しの入出力失敗を写す。`retried` は rename リトライの枯渇を表すため
/// 本モジュールは常に `false`（保存経路だけが `true` を構築し得る）。
///
/// NDJSON コーデック（タスク 3.3）も書き出しの最後の 1 回で同じ写像を使う
/// （入出力失敗の写像を 2 つ持たない）。
pub(crate) fn io_error(source: io::Error) -> DocumentError {
    DocumentError::Io {
        source,
        retried: false,
    }
}

// ---------------------------------------------------------------------------
// 未知フィールドの保持と差し戻し（タスク 3.2。要件 6.2, 6.3）
// ---------------------------------------------------------------------------

/// 未知フィールド 1 件（前方互換の保持単位。要件 6.2 / 6.3）。
///
/// * **キー**は JSON 文字列として**デコード済み**のテキストである。書き出しでは
///   JSON 文字列として再エンコードするため、原文がキーに `\uXXXX` のような
///   エスケープ表記を使っていれば、その**表記**は保たれない（デコード後の文字列が
///   同じなら JSON として等価）。値は表記まで保つので、往復のバイト一致が崩れ得るのは
///   キー側だけである。
/// * **値**は [`RawValue`] が捕捉した**原文のバイト列**そのもので、再シリアライズ
///   しない（[`PreservedField::value_bytes`]）。内部の空白・`\uXXXX` エスケープ・
///   i64 を超える整数リテラル・入れ子のオブジェクト/配列も原文のまま残る。
/// * **位置**は [`PreservedField::preceding_known_fields`] だけが表す（方式の理由は
///   [`PreservedFields`] の docs）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreservedField {
    key: String,
    value: Vec<u8>,
    preceding_known_fields: usize,
}

impl PreservedField {
    /// キー（デコード済みテキスト）。
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 値の生バイト列（原文のまま。再シリアライズしていない）。
    pub fn value_bytes(&self) -> &[u8] {
        &self.value
    }

    /// 元の入力で、この未知フィールドより前に現れた**既知フィールドの件数**。
    ///
    /// [`PreservingObjectWriter`] はこの値だけを見て差し戻し位置を決める。
    pub fn preceding_known_fields(&self) -> usize {
        self.preceding_known_fields
    }
}

/// 原文の順序を保つ未知フィールドの集合（前方互換の保持領域。要件 6.2 / 6.3）。
///
/// # 位置の表現方式（前方カウント方式）
///
/// 本集合は各未知フィールドの位置を「**その未知フィールドより前に現れた既知
/// フィールドの件数**」で表す（[`PreservedField::preceding_known_fields`]）。入力キー列
/// における絶対位置（位置インデックス方式）は保存しない。
///
/// 理由: 書き出しは既知フィールドを**宣言順 = 正準順**に固定するため（要件 3.3）、
/// 既知キーの絶対位置は保存後の意味を持たない。必要なのは「既知フィールド何個ぶん
/// 後ろか」だけであり、絶対位置を併せて保存すると、既知キーが並び替わった入力に対して
/// 矛盾する位置情報が残る（どちらを優先するかを決める規則がもう 1 つ増える）。
///
/// 不変条件: 件数は取り込み順に**単調非減少**である。カウンタは本集合の内部にあり、
/// [`PreservedFields::record_known_field`] が増やした値を [`PreservedFields::capture`] が
/// そのまま記録するため、呼び出し元が不変条件を守る必要はない。**1 回の読み込みに
/// つき 1 つの本集合を使う**（読み込みをまたいで使い回すとカウンタが引き継がれる）。
///
/// # バイト往復が成立する範囲
///
/// 読み書きのバイト単位の一致（テスト
/// `unknown_fields_before_between_and_after_known_fields_round_trip_byte_for_byte`）が
/// 成立するのは、**入力がコンパクト（値の前後に空白を置かない）で、既知フィールドが
/// 既に宣言順で並んでいる場合**である（本クレート自身が書いたファイルはこの条件を
/// 満たす）。キーと値の間・値と区切りの間の空白は JSON の**値の一部ではない**ため、
/// `RawValue` の捕捉にも含まれず、その表記は往復で消える（値**内部**の空白は値の一部
/// であり原文のまま残る）。宣言順でない入力は、既知フィールドだけが正準順へ
/// 並び替わった出力になる（テスト `known_fields_are_written_in_declaration_order` で
/// 文書化した仕様）。値の表記（エスケープ・数値リテラル）とキーのデコードは往復で
/// 保たれる（[`PreservedField`] の docs 参照）。
///
/// # 使用例（4.2 / 6.x のバージョン付きパート構造体がどう使うか）
///
/// 読み込み側は既知キーを読むたびに [`PreservedFields::record_known_field`] を呼び、
/// 未知キーは [`PreservedFields::capture`] に渡す。書き出し側は
/// [`PreservingObjectWriter`] へ**宣言順**で既知フィールドを書く。
///
/// ```
/// use std::fmt;
///
/// use serde::de::{self, MapAccess, Visitor};
/// use serde::{Deserialize, Deserializer};
/// use document_format::json::{PreservedFields, PreservingObjectWriter};
///
/// /// バージョン付きパート（既知フィールドの宣言順は `version`, `parts`）。
/// struct ManifestPart {
///     version: u64,
///     parts: Vec<String>,
///     preserved: PreservedFields,
/// }
///
/// impl<'de> Deserialize<'de> for ManifestPart {
///     fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
///         deserializer.deserialize_map(PartVisitor)
///     }
/// }
///
/// struct PartVisitor;
///
/// impl<'de> Visitor<'de> for PartVisitor {
///     type Value = ManifestPart;
///
///     fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         f.write_str("a manifest part object")
///     }
///
///     fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ManifestPart, A::Error> {
///         let (mut version, mut parts): (Option<u64>, Option<Vec<String>>) = (None, None);
///         let mut preserved = PreservedFields::new();
///         while let Some(key) = map.next_key::<String>()? {
///             match key.as_str() {
///                 "version" => {
///                     version = Some(map.next_value()?);
///                     preserved.record_known_field();
///                 }
///                 "parts" => {
///                     parts = Some(map.next_value()?);
///                     preserved.record_known_field();
///                 }
///                 _ => preserved.capture(&key, &mut map)?,
///             }
///         }
///         let Some(version) = version else {
///             return Err(de::Error::custom("manifest part is missing the `version` key"));
///         };
///         Ok(ManifestPart { version, parts: parts.unwrap_or_default(), preserved })
///     }
/// }
///
/// // 読み込み: 将来のバージョンが足したフィールドも破棄せず保持される。
/// let input = r#"{"future_meta":{"unit":"mm"},"version":4,"parts":["p"]}"#;
/// let part: ManifestPart = serde_json::from_str(input).expect("parse");
/// assert_eq!(1, part.preserved.len());
///
/// // 書き出し: 既知フィールドは宣言順、未知フィールドは原文の位置へ差し戻す。
/// let mut out = Vec::new();
/// let mut writer = PreservingObjectWriter::new(&mut out, &part.preserved);
/// writer.write_known("version", &part.version).expect("write");
/// writer.write_known("parts", &part.parts).expect("write");
/// writer.finish().expect("write");
/// assert_eq!(input, String::from_utf8(out).expect("utf-8"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreservedFields {
    fields: Vec<PreservedField>,
    /// ここまでに読んだ既知フィールドの件数（差し戻し位置の基準）。
    known_seen: usize,
}

impl PreservedFields {
    /// 空の集合を作る。
    pub const fn new() -> Self {
        Self { fields: Vec::new(), known_seen: 0 }
    }

    /// 保持している未知フィールドの件数。
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// 未知フィールドを 1 件も保持していないか。
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// 原文順の未知フィールド（値は生バイト列）。
    pub fn iter(&self) -> impl Iterator<Item = &PreservedField> {
        self.fields.iter()
    }

    /// **既知**フィールドを 1 件読んだことを記録する（差し戻し位置の基準を進める）。
    ///
    /// 既知キーを処理するたびに呼ばないと、後続の未知フィールドがすべて先頭側へ
    /// 差し戻される（呼び出し漏れは位置の劣化として現れ、フィールド自体は失われない）。
    pub fn record_known_field(&mut self) {
        self.known_seen += 1;
    }

    /// **未知**キー 1 件を、値の生バイト列ごと取り込む。
    ///
    /// 値は [`RawValue`] で捕捉する: 構文検証済みの**原文のバイト列**がそのまま得られ、
    /// `serde_json::Value` を経由しない（内部の空白・`\uXXXX` エスケープ・i64 を
    /// 超える整数リテラルがそのまま残る）。`key` はデコード済みのテキストであり、
    /// [`MapAccess::next_key`] が返したものをそのまま渡す。
    pub fn capture<'de, A: MapAccess<'de>>(
        &mut self,
        key: &str,
        map: &mut A,
    ) -> Result<(), A::Error> {
        let value = map.next_value::<Box<RawValue>>()?;
        self.fields.push(PreservedField {
            key: key.to_owned(),
            value: value.get().as_bytes().to_vec(),
            preceding_known_fields: self.known_seen,
        });
        Ok(())
    }
}

/// 既知フィールドを**宣言順 = 正準順**で書き出しつつ、未知フィールドを
/// [`PreservedField::preceding_known_fields`] の位置へ差し戻して 1 つの JSON オブジェクトを
/// 組み立てる書き出し口（タスク 3.2。要件 6.2 / 6.3）。
///
/// バージョン付きパート構造体（4.2 以降）は、自分の宣言順で
/// [`PreservingObjectWriter::write_known`] を呼び、最後に
/// [`PreservingObjectWriter::finish`] を呼ぶ（使い方は [`PreservedFields`] の使用例）。
///
/// # 差し戻しの規則
///
/// `write_known` が i 回目（0 始まり）に呼ばれたとき、`preceding_known_fields <= i` の
/// 未知フィールドが、その既知フィールドの**直前**へ原文順に差し戻される。残りは
/// `finish` が末尾へ原文順に書き出す。したがって既知フィールドが 1 件も書かれない
/// 場合（書き出し対象を持たないパート）でも、未知フィールドは末尾へすべて出る
/// （**落ちることはない**）。
///
/// 差し戻し位置の基準は「**書き出した**既知フィールドの件数」である。したがって
/// `write_known` を宣言順に呼ばない場合や、省略可能な既知フィールドの
/// `write_known` を省いた場合は、未知フィールドの位置が元の入力と一致しなくなり得る
/// （フィールド自体は失われない）。どの既知フィールドをどの順序で書くかは呼び出し元
/// （パート構造体）の責務であり、本型は関与しない。
///
/// # 失敗時の保証
///
/// 3.1 の書き出し口と同じ規律を守る: 組み立ては一時バッファで行い、`finish` が最後に
/// 一度だけ書き出す。途中の `write_known` が失敗した場合も、`finish` を呼ばずに
/// 捨てた場合も、書き出し先へ 1 バイトも書かれない。既知フィールドの値の直列化失敗は
/// [`write_json`] と同じく [`DocumentError::InvalidContainer`] に写る（生の `f64` を
/// 置かない制約も [`write_json`] と共通）。
pub struct PreservingObjectWriter<'a, W: Write> {
    out: &'a mut W,
    /// 確定前のオブジェクト（`finish` まで書き出し先へ出さない）。
    buffer: Vec<u8>,
    preserved: &'a PreservedFields,
    /// 次に差し戻す未知フィールドの添字。
    next: usize,
    /// 書き出したエントリ数（カンマの判定）。
    entries: usize,
    /// 書き出した既知フィールドの件数（差し戻し位置の基準）。
    written: usize,
}

impl<'a, W: Write> PreservingObjectWriter<'a, W> {
    /// 未知フィールドの集合を差し戻しながら書くオブジェクトを開始する。
    pub fn new(out: &'a mut W, preserved: &'a PreservedFields) -> Self {
        Self { out, buffer: vec![b'{'], preserved, next: 0, entries: 0, written: 0 }
    }

    /// 既知フィールド 1 件を書き出す（**宣言順**に呼ぶこと）。
    ///
    /// キーは JSON 文字列としてエスケープされる（エスケープ規則を再実装しない）。
    /// 値は `serde_json` の直列化に委ね、失敗は [`DocumentError::InvalidContainer`] に写す。
    pub fn write_known<T: Serialize + ?Sized>(
        &mut self,
        key: &str,
        value: &T,
    ) -> Result<(), DocumentError> {
        // この既知フィールドの手前で位置が確定した未知フィールドを先に差し戻す。
        self.flush_preserved(self.written)?;
        self.begin_entry(key)?;
        serde_json::to_writer(&mut self.buffer, value).map_err(|err| write_error(&err))?;
        self.written += 1;
        Ok(())
    }

    /// オブジェクトを閉じ、残りの未知フィールドを末尾へ差し戻して書き出す。
    pub fn finish(mut self) -> Result<(), DocumentError> {
        // 既知フィールドより後ろに位置する未知フィールドを原文順にすべて出す。
        self.flush_preserved(usize::MAX)?;
        self.buffer.push(b'}');
        self.out.write_all(&self.buffer).map_err(io_error)
    }

    /// キーと区切りを書き出す（エントリ数でカンマを決める）。
    fn begin_entry(&mut self, key: &str) -> Result<(), DocumentError> {
        if self.entries > 0 {
            self.buffer.push(b',');
        }
        self.entries += 1;
        serde_json::to_writer(&mut self.buffer, key).map_err(|err| write_error(&err))?;
        self.buffer.push(b':');
        Ok(())
    }

    /// `preceding_known_fields <= known_bound` の未知フィールドを、原文順に差し戻す。
    ///
    /// 件数は取り込み順に単調非減少（[`PreservedFields`] の不変条件）なので、先頭から
    /// 見て条件を満たす範囲を出せばよい。
    fn flush_preserved(&mut self, known_bound: usize) -> Result<(), DocumentError> {
        // `preserved` は独立した参照なので、`self` の可変借用と同時に保持できる。
        let preserved = self.preserved;
        while let Some(field) = preserved.fields.get(self.next) {
            if field.preceding_known_fields > known_bound {
                break;
            }
            self.begin_entry(&field.key)?;
            self.buffer.extend_from_slice(&field.value);
            self.next += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde::de::{self, MapAccess, Visitor};
    use serde::{Deserialize, Deserializer, Serialize};

    use super::*;
    use crate::value::NestedValue;

    /// 呼び出し元が与える行の文脈。セル位置は `<LOC> column <キー>` になる。
    const LOC: &str = "sheet 01J2X3Z0000000000000000000 row 01J2X3Z0000000000000000001";

    /// フィールド宣言順が辞書順と異なる構造体（要件 3.3 の検証用）。
    #[derive(Serialize)]
    struct ProbeRow {
        zulu: u8,
        alpha: String,
        mike: CellValue,
    }

    /// 非有限値のセルを途中に挟む構造体（部分的な出力を書かないことの検証用）。
    #[derive(Serialize)]
    struct ProbeWithNonFinite {
        before: u8,
        value: CellValue,
        after: u8,
    }

    /// 書き出し結果をテキストとして読む（出力は UTF-8 の JSON）。
    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).expect("出力は UTF-8 の JSON")
    }

    /// セル値 1 個を [`write_cell`] で書く。
    fn encode_cell(value: &CellValue) -> String {
        let mut out = Vec::new();
        write_cell(&mut out, value, LOC).expect("妥当なセルの書き出しが失敗した");
        text(&out)
    }

    /// 列集合を [`write_ordered_object`] で書く。
    fn encode_object(entries: &[(&str, &CellValue)]) -> String {
        let mut out = Vec::new();
        write_ordered_object(&mut out, LOC, entries).expect("妥当な列集合の書き出しが失敗した");
        text(&out)
    }

    /// バイト列を 16 進表記へ（プロセス間比較の報告用）。
    fn hex(bytes: &[u8]) -> String {
        use core::fmt::Write as _;

        let mut text = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(text, "{byte:02x}").expect("String への書き込みは失敗しない");
        }
        text
    }

    /// 決定性の検証に使う代表的な書き出し（構造体 + 列集合 + 非 ASCII + 負のゼロ）。
    ///
    /// 親子の両プロセスが同じ手順で組み立てるため、出力が環境に依存すれば比較が崩れる。
    fn probe_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        let row = ProbeRow {
            zulu: 7,
            alpha: "日本語😀".into(),
            mike: CellValue::Nested(NestedValue::Object(vec![
                ("価格".into(), CellValue::Float(-0.0)),
                ("$t".into(), CellValue::Text("0".into())),
            ])),
        };
        write_json(&mut out, &row).expect("構造体の書き出しが失敗した");

        let count = CellValue::Int(-3);
        let note = CellValue::Text("改行\nと\"引用\"".into());
        write_ordered_object(&mut out, LOC, &[("zeta", &count), ("alpha", &note)])
            .expect("列集合の書き出しが失敗した");

        write_cell(&mut out, &CellValue::Float(-0.0), LOC).expect("セルの書き出しが失敗した");
        out
    }

    // --- (a) 同一入力 → 同一バイト列（要件 3.3） ---------------------------------

    #[test]
    fn repeated_writes_are_byte_identical() {
        let first = probe_bytes();
        assert!(!first.is_empty(), "書き出しが空（比較が無意味になる）");
        assert_eq!(first, probe_bytes(), "2 回の書き出しがバイト一致しない");
    }

    /// 異なるプロセスでも同一バイト列であること。
    ///
    /// 決定性は「同一プロセス内で 2 回」では足りない: ハッシュコンテナの反復順や
    /// アドレス空間の配置は**プロセスごとに変わる**ため、それらに依存した実装は
    /// 同一プロセス内の反復では検出できない。子プロセスで自身を再実行し、
    /// 子が報告したバイト列と親のバイト列を比較する。
    #[test]
    fn identical_bytes_across_processes() {
        const PROBE_ENV: &str = "DOCUMENT_FORMAT_JSON_PROBE";
        const PROBE_CHILD: &str = "child";
        const PROBE_TEST: &str = "json::determinism::tests::identical_bytes_across_processes";
        const PROBE_PREFIX: &str = "PROBE=";

        let bytes = probe_bytes();
        assert!(!bytes.is_empty(), "書き出しが空（比較が無意味になる）");
        // 子分岐は**値の厳密一致**で選ぶ。変数の存在だけで分岐すると、同名の環境変数が
        // 外部に定義されている環境では親が子分岐へ入り、比較が空振りして緑になる。
        if std::env::var_os(PROBE_ENV).is_some_and(|value| value == PROBE_CHILD) {
            // 子プロセス: 親と同じ手順で組み立てたバイト列を 1 行で報告する。
            println!("{PROBE_PREFIX}{}", hex(&bytes));
            return;
        }

        let exe = std::env::current_exe().expect("テスト実行ファイルのパス取得");
        let child = std::process::Command::new(exe)
            .args(["--exact", PROBE_TEST, "--nocapture"])
            .env(PROBE_ENV, PROBE_CHILD)
            .output()
            .expect("子プロセスの起動");
        assert!(
            child.status.success(),
            "子プロセスが失敗した: {}",
            String::from_utf8_lossy(&child.stderr),
        );

        let stdout = String::from_utf8(child.stdout).expect("子プロセスの出力は UTF-8");
        let reported = stdout
            .lines()
            .find_map(|line| line.strip_prefix(PROBE_PREFIX))
            .expect("子プロセスが PROBE= を報告しない（--exact のテスト名が古い可能性）");
        assert_eq!(hex(&bytes), reported, "プロセスをまたぐと出力バイト列が変わる");
    }

    // --- (b) 非 ASCII とエスケープ（要件 2.4） -----------------------------------

    #[test]
    fn non_ascii_keys_and_values_are_raw_utf8() {
        let japanese = CellValue::Text("日本語".into());
        let emoji = CellValue::Text("😀".into());
        let escaped = CellValue::Text("a\"b\\c\nd".into());
        let control = CellValue::Text("\u{1}".into());
        let entries: &[(&str, &CellValue)] = &[
            ("名前", &japanese),
            ("絵文字", &emoji),
            ("quote\"key", &escaped),
            ("ctrl", &control),
        ];

        let encoded = encode_object(entries);
        // 期待する UTF-8 バイト列そのもの: 非 ASCII は `\u` へ落とさず生の UTF-8 で書き、
        // 制御文字は `\uXXXX` にする（JSON のエスケープ規則は serde_json へ委譲している）。
        assert_eq!(
            "{\"名前\":\"日本語\",\"絵文字\":\"😀\",\"quote\\\"key\":\"a\\\"b\\\\c\\nd\",\"ctrl\":\"\\u0001\"}",
            encoded,
        );
    }

    // --- (c) 構造体のフィールド宣言順（要件 3.3） --------------------------------

    #[test]
    fn struct_fields_are_written_in_declaration_order() {
        let row = ProbeRow {
            zulu: 7,
            alpha: "日本語".into(),
            mike: CellValue::Bool(true),
        };
        let mut out = Vec::new();
        write_json(&mut out, &row).expect("構造体の書き出しが失敗した");

        // 宣言順は zulu, alpha, mike。辞書順（alpha, mike, zulu）ではない。
        assert_eq!("{\"zulu\":7,\"alpha\":\"日本語\",\"mike\":true}", text(&out));
    }

    #[test]
    fn write_json_is_compact_and_has_no_trailing_newline() {
        let row = ProbeRow {
            zulu: 1,
            alpha: "x".into(),
            mike: CellValue::Null,
        };
        let mut out = Vec::new();
        write_json(&mut out, &row).expect("構造体の書き出しが失敗した");

        // 区切りの空白も改行も末尾改行も無い（コンパクト、要件 3.4 の行末規則は NDJSON 側）。
        assert_eq!("{\"zulu\":1,\"alpha\":\"x\",\"mike\":null}", text(&out));
    }

    // --- (d) 動的な列集合は与えられた順序のまま（要件 3.3, 3.4 の前提） ----------

    #[test]
    fn ordered_object_preserves_the_caller_order() {
        let int = CellValue::Int(1);
        let text_value = CellValue::Text("x".into());
        let bool_value = CellValue::Bool(true);

        // キーは辞書順（alpha, mike, zeta）ともスキーマの登録順とも異なる並び。
        let forward = encode_object(&[("zeta", &int), ("alpha", &text_value), ("mike", &bool_value)]);
        assert_eq!("{\"zeta\":1,\"alpha\":\"x\",\"mike\":true}", forward);

        // 同じ集合を別の順序で与えれば、出力もその順序に従う（並べ替えない）。
        let reversed = encode_object(&[("mike", &bool_value), ("alpha", &text_value), ("zeta", &int)]);
        assert_eq!("{\"mike\":true,\"alpha\":\"x\",\"zeta\":1}", reversed);
    }

    #[test]
    fn ordered_object_cells_delegate_to_the_value_wire_form() {
        let values = [
            CellValue::Null,
            CellValue::Bool(true),
            CellValue::Int(-42),
            CellValue::Float(-0.0),
            CellValue::Decimal("1.5".into()),
            // `0` は規則 1 で `Decimal` に決まるため、`Text` は脱出口で書かれる。
            CellValue::Text("0".into()),
            CellValue::Nested(NestedValue::Array(vec![CellValue::Int(1)])),
        ];

        for value in &values {
            let cell = value::to_json_bytes(value, LOC).expect("妥当なセルの書き出しが失敗した");
            let expected = format!("{{\"k\":{}}}", text(&cell));
            assert_eq!(expected, encode_object(&[("k", value)]), "委譲になっていない");
        }
    }

    // --- (e) 数値の正規化: `-0.0` は `0`（要件 3.3, 3.6） -------------------------

    #[test]
    fn negative_zero_is_written_without_a_sign() {
        assert_eq!("0.0", encode_cell(&CellValue::Float(-0.0)));
        assert_eq!("{\"k\":0.0}", encode_object(&[("k", &CellValue::Float(-0.0))]));
        assert_eq!(
            "{\"price\":0.0}",
            encode_object(&[("price", &CellValue::float(-0.0))]),
        );
    }

    // --- (f) 非有限値の拒否と「部分的な JSON を書かない」（要件 3.3） ------------

    #[test]
    fn non_finite_cells_are_rejected_with_the_cell_location() {
        let ok = CellValue::Int(1);
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let cell = CellValue::Float(bad);
            let mut out = Vec::new();
            let err = write_ordered_object(&mut out, LOC, &[("alpha", &ok), ("beta", &cell)])
                .expect_err("非有限値の書き出しが成功した");
            match err {
                DocumentError::NonRepresentableNumber { location } => {
                    assert_eq!(format!("{LOC} column beta"), location);
                }
                other => panic!("{other:?} は NaN/Inf のエラーではない"),
            }
            assert!(out.is_empty(), "失敗時に部分的なバイト列が書かれた");
        }

        let nested = CellValue::Nested(NestedValue::Array(vec![CellValue::Float(f64::NAN)]));
        let mut out = Vec::new();
        let err = write_cell(&mut out, &nested, LOC).expect_err("入れ子の NaN が通った");
        assert!(matches!(err, DocumentError::NonRepresentableNumber { .. }));
        assert!(out.is_empty(), "失敗時に部分的なバイト列が書かれた");
    }

    #[test]
    fn a_failure_never_leaves_partial_json_behind() {
        let row = ProbeWithNonFinite {
            before: 1,
            value: CellValue::Float(f64::NAN),
            after: 3,
        };
        let mut out = Vec::new();
        let err = write_json(&mut out, &row).expect_err("非有限値を含む構造体の書き出しが成功した");
        match err {
            DocumentError::InvalidContainer { entry } => assert!(
                entry.contains("NonRepresentableNumber"),
                "理由に変種名が現れない: {entry}",
            ),
            other => panic!("{other:?} は直列化失敗のエラーではない"),
        }
        assert!(out.is_empty(), "失敗時に部分的なバイト列が書かれた");
    }

    // --- (g) 未知フィールドの保持と差し戻し（タスク 3.2。要件 6.2, 6.3） ---------

    /// バージョン付きパートの模型（4.2 / 6.x の構造体が本プリミティブをどう使うか）。
    ///
    /// 既知フィールドは宣言順に `version`, `parts`。[`PreservedFields`] は書き出し
    /// 対象のフィールドではない（構造体は保持するだけで、オブジェクトへの差し戻しは
    /// [`PreservingObjectWriter`] が担う）。
    struct ProbePart {
        version: u64,
        parts: Vec<String>,
        preserved: PreservedFields,
    }

    impl<'de> Deserialize<'de> for ProbePart {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_map(ProbePartVisitor)
        }
    }

    /// パートの visitor: 既知キーは [`PreservedFields::record_known_field`]、未知キーは
    /// [`PreservedFields::capture`]（この 2 つが保持プリミティブの読み込み側の口）。
    struct ProbePartVisitor;

    impl<'de> Visitor<'de> for ProbePartVisitor {
        type Value = ProbePart;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a versioned part object with `version` and `parts`")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ProbePart, A::Error> {
            let mut version: Option<u64> = None;
            let mut parts: Option<Vec<String>> = None;
            let mut preserved = PreservedFields::new();
            while let Some(key) = map.next_key::<String>()? {
                match key.as_str() {
                    "version" => {
                        version = Some(map.next_value()?);
                        preserved.record_known_field();
                    }
                    "parts" => {
                        parts = Some(map.next_value()?);
                        preserved.record_known_field();
                    }
                    _ => preserved.capture(&key, &mut map)?,
                }
            }
            let Some(version) = version else {
                return Err(de::Error::custom("part is missing the `version` key"));
            };
            Ok(ProbePart { version, parts: parts.unwrap_or_default(), preserved })
        }
    }

    /// 模型を読み、宣言順に書き戻す（4.2 / 6.x の I/O 経路の模型）。
    fn rewrite_part(input: &str) -> (ProbePart, Vec<u8>) {
        let part: ProbePart = serde_json::from_str(input).expect("パートの読み込みが失敗した");
        let mut out = Vec::new();
        let mut writer = PreservingObjectWriter::new(&mut out, &part.preserved);
        writer.write_known("version", &part.version).expect("既知フィールドの書き出しが失敗した");
        writer.write_known("parts", &part.parts).expect("既知フィールドの書き出しが失敗した");
        writer.finish().expect("オブジェクトの確定が失敗した");
        (part, out)
    }

    /// 未知フィールドが既知フィールドの**前・間・後**のすべてにある入力でも、
    /// 読み書きでバイト単位の元に戻ること（位置復元の核心。不変条件 1）。
    #[test]
    fn unknown_fields_before_between_and_after_known_fields_round_trip_byte_for_byte() {
        const INPUT: &str =
            r#"{"future_a":{"x":1},"version":4,"future_b":[1,2],"parts":["p","q"],"future_c":null}"#;
        let (part, out) = rewrite_part(INPUT);

        assert_eq!(3, part.preserved.len(), "未知フィールドの件数が一致しない");
        assert_eq!(
            vec!["future_a", "future_b", "future_c"],
            part.preserved.iter().map(PreservedField::key).collect::<Vec<_>>(),
            "保持順が原文順でない",
        );
        assert_eq!(
            vec![0, 1, 2],
            part.preserved
                .iter()
                .map(PreservedField::preceding_known_fields)
                .collect::<Vec<_>>(),
            "既知フィールドに対する元の位置（前方カウント）が記録されていない",
        );
        assert_eq!(INPUT.as_bytes(), out.as_slice(), "書き戻しがバイト単位で元に戻らない");
    }

    /// 未知フィールドの値は**原文のバイト列のまま**出る（`serde_json::Value` を経由して
    /// いないことの証拠。不変条件 2）: 値**内部**の独自の空白・`\uXXXX` エスケープ・
    /// サロゲート対・生の絵文字・i64 を超える整数リテラル・入れ子を再シリアライズしない
    /// （キーと値の間の空白は JSON の値の一部ではないため、コンパクトな入力を使う）。
    #[test]
    fn preserved_values_are_verbatim_without_re_serialization() {
        const INPUT: &str = r#"{"weird":{  "a" : [ 1 , 2 ] , "b" : "\u65e5\u672c" },"version":1,"parts":["p"],"big":123456789012345678901234567890,"emoji":"😀","surrogate":"\ud83d\ude00","tab":"\t"}"#;
        let (part, out) = rewrite_part(INPUT);

        let first = part.preserved.iter().next().expect("未知フィールドが保持されていない");
        assert_eq!(
            br#"{  "a" : [ 1 , 2 ] , "b" : "\u65e5\u672c" }"#.as_slice(),
            first.value_bytes(),
            "値の生バイト列が原文と異なる（正規化・再直列化された）",
        );
        assert_eq!(INPUT.as_bytes(), out.as_slice(), "書き戻しで値が再直列化された");
    }

    /// 既知フィールドは未知フィールドの有無に関わらず宣言順で出る（不変条件 3）。
    /// 入力の既知フィールドが宣言順でない場合、既知側だけが正準順へ直る
    /// （バイト往復が成立する条件は [`PreservedFields`] の docs に明記した）。
    #[test]
    fn known_fields_are_written_in_declaration_order() {
        const INPUT: &str = r#"{"parts":["p"],"future_a":1,"version":4}"#;
        let (_, out) = rewrite_part(INPUT);

        assert_eq!(r#"{"version":4,"future_a":1,"parts":["p"]}"#, text(&out));
    }

    /// 未知フィールドが無い入力は従来どおりの出力（回帰なし。不変条件 4）。
    #[test]
    fn without_unknown_fields_nothing_changes_and_the_set_is_empty() {
        const INPUT: &str = r#"{"version":4,"parts":["p"]}"#;
        let (part, out) = rewrite_part(INPUT);

        assert!(part.preserved.is_empty(), "未知フィールドが無いのに保持された");
        assert_eq!(0, part.preserved.len());
        assert!(part.preserved.iter().next().is_none(), "空の集合が要素を返した");
        assert_eq!(INPUT.as_bytes(), out.as_slice(), "未知フィールドの無い入力の出力が変わった");
    }

    /// 移行チェーンの依存の模擬（不変条件 5。要件 6.2 / 6.3）: 「将来バージョンが足した
    /// フィールド」を含む入力を現行の構造体で読んで書き戻しても消えず、書き出したものを
    /// もう一度読んで書き戻しても同じバイト列になる（不動点）。
    #[test]
    fn fields_added_by_a_future_version_survive_repeated_rewrites() {
        const FUTURE: &str =
            r#"{"future_meta":{"added_by":"minor","unit":"mm"},"version":4,"parts":["p"]}"#;
        let (part, first) = rewrite_part(FUTURE);
        assert_eq!(FUTURE.as_bytes(), first.as_slice(), "将来バージョンのフィールドが往復で失われた");

        let (_, second) = rewrite_part(&text(&first));
        assert_eq!(first, second, "再読込で将来バージョンのフィールドが変化した");
        assert_eq!(1, part.preserved.len());
    }

    /// 未知フィールドの値が `null` でも保持される（不変条件 6。「値が null だから落とす」
    /// 実装を排除する）。
    #[test]
    fn unknown_fields_with_a_null_value_are_preserved() {
        const INPUT: &str = r#"{"future_null":null,"version":1,"parts":[]}"#;
        let (part, out) = rewrite_part(INPUT);

        let field = part.preserved.iter().next().expect("null 値の未知フィールドが保持されていない");
        assert_eq!("future_null", field.key());
        assert_eq!(b"null".as_slice(), field.value_bytes());
        assert_eq!(INPUT.as_bytes(), out.as_slice());
    }

    /// 差し戻しヘルパも 3.1 の書き出し口と同じ規律を守る（失敗時に部分出力を残さない）。
    #[test]
    fn preserving_writer_leaves_no_partial_output_on_failure() {
        let preserved = PreservedFields::new();
        let mut out = Vec::new();
        let mut writer = PreservingObjectWriter::new(&mut out, &preserved);
        writer.write_known("version", &1u64).expect("既知フィールドの書き出しが失敗した");

        let err = writer
            .write_known("bad", &CellValue::Float(f64::NAN))
            .expect_err("非有限値の書き出しが成功した");
        match err {
            DocumentError::InvalidContainer { entry } => assert!(
                entry.contains("NonRepresentableNumber"),
                "理由に変種名が現れない: {entry}",
            ),
            other => panic!("{other:?} は直列化失敗のエラーではない"),
        }
        drop(writer);
        assert!(out.is_empty(), "失敗時に部分的なバイト列が書かれた");
    }
}

//! セル値のモデルと JSON 表現(タスク 1.5。要件 3.6, 7.3)。
//!
//! design「Logical Data Model」の `CellValue` 表が挙げる 8 変種だけの**閉じた集合**。
//! どの列がどれを使うかは schema-engine が決める。本モジュールが所有するのは
//! **変種集合 / wire 表現 / ラウンドトリップ保証**の 3 つである。
//!
//! # ジェネリックな JSON 値型を内部表現にしない
//!
//! `serde_json` の汎用 JSON 値型(`Value` / `Map` / `Number`)を内部表現に使わない:
//! 整数と浮動小数の区別を保持できない型が混入すると、`5`(整数)と `5.0`(浮動小数)の
//! 区別が書き出しで化ける(要件 3.6)。数値は [`CellValue::Int`] / [`CellValue::Float`]
//! に保持し、書き出しも `serialize_i64` / `serialize_f64` を直接呼ぶ。
//! `serde_json` はシリアライザ / デシリアライザの配線にのみ使う。
//!
//! # wire 形式(design 表 + 決定可能な脱出口)
//!
//! design 表の JSON 形をそのまま採用する。
//!
//! | 変種         | wire                                         |
//! |--------------|--------------------------------------------------|
//! | `Null`       | `null`                             |
//! | `Bool`       | `true` / `false`                   |
//! | `Int`        | 整数リテラル(64 ビット)          |
//! | `Float`      | 数値リテラル(`f64`。`-0.0` は `0`)     |
//! | `Decimal`    | 文字列                                       |
//! | `Text`       | 文字列(数値に見える内容は規則 2 の脱出口)  |
//! | `Nested`     | object / array(キー順はそのまま)             |
//! | `Attachment` | BLAKE3 hex のむき出し文字列(64 文字)  |
//!
//! 「`-0.0` は `0`」は**符号を書かない**という意味である(むき出しの `0` と書くと
//! 読み側は `Int` と判別するため、浮動小数の 0 は `0.0` のまま書く。テスト
//! `negative_zero_is_written_as_zero`)。
//!
//! design 表では `Decimal` / `Text` / `Attachment` の 3 変種が同じ「文字列」に
//! 落ちる。本クレートはスキーマを復号しない(行エントリはスキーマ解釈なしで
//! モデルへ戻らなければならない)ため、読み側は**スキーマ不要の決定手順**で
//! 3 変種を分ける。以下が wire 規則のすべてである:
//!
//! 1. むき出しの文字列は次の優先順で 1 つの変種に決まる(入力が同じなら常に同じ
//!    結果になり、推測による修復はしない)。
//!    - 正準 64 文字の小文字 hex かつ [`AttachmentId`] に復号できる → `Attachment`
//!      (design 表どおり。要件 7.3 の行→添付参照)
//!    - 10 進文法 `^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$` に一致 → `Decimal`
//!      (正準形である必要はなく、一致すれば `Decimal`)
//!    - それ以外 → `Text`
//! 2. 1 の規則で `Decimal` / `Attachment` と読まれてしまう **Text** は、めずらしい
//!    脱出口としてタグ付きオブジェクトで書く: `{"$t":"text","v":"<内容>"}`。
//!    `Decimal` は逆に、1 の規則で `Decimal` に決まらない中身(添付 hex に一致する
//!    もの・10 進文法外のテキスト・空文字列など)をすべて
//!    `{"$t":"decimal","v":"<内容>"}` と折り返す。読み側はタグが `decimal` の
//!    オブジェクトを、本体 `v` が文字列のとき**内容そのまま**(`Decimal` の文字列を
//!    一切解釈せず) `Decimal` に復元する。これにより 3 変種のどの値に対しても
//!    `decode(encode(v)) == v` が成立する(design 表は可能な限り文字どおり保ち、
//!    判別できないケースだけに脱出口を使う)。
//! 3. オブジェクトのキーは値の判別に使わない。ただしマーカー `$t` と衝突し得る
//!    `$` 始まりのキーは、書き出し時に `$` を 1 つ足して `$$...` にし、読み時に
//!    1 つ剥がす。本クレート自身の書き出しは `$` 始まりのキーを出さないため、
//!    `{"$t": ...}` は常にマーカーであり、データ側の `$t` キーは書き出しで必ず
//!    `$$t` になる(一度 `$$t` になった wire は読み書きを繰り返しても変わらない)。
//! 4. 上記のマーカー形(`$t` = `text` / `decimal`)に一致しないオブジェクトは
//!    すべて `Nested` の object として読む(`$t` の値がタグでなくてもエラーに
//!    せず object に残す。決定的な手順であり、推測による修復は行わない)。
//!
//! `Nested` のキー順は入力どおり保持する(`HashMap` は反復順が実行ごとに変わる
//! ため決定性を壊し、禁止)。
//!
//! # エラー対応
//!
//! - 書き出し: 非有限の浮動小数(`NaN` / `±Infinity`、`Nested` の内側を含む)は
//!   [`to_json_bytes`] が事前に走査して [`DocumentError::NonRepresentableNumber`]
//!   を返し、**バイト列を 1 バイトも生成しない**(要件 3.3 系「不正な JSON を
//!   書かない」)。`Serialize` 実装は location を運べないため同じ検査で
//!   `ser::Error` に落とす(型付きエラーが必要な経路は必ず [`to_json_bytes`] を
//!   使う。NaN を `null` へ黙って変換することは禁止)。
//! - 読み込み: JSON として不正な入力、`i64` に収まらない整数、非有限になる数値
//!   リテラルなどは design 表に専用の応答が無いため [`DocumentError::InvalidContainer`] に写す。
//!   `entry` は解析対象を指す説明文(`value: <理由>`)で、失敗理由の文字列を
//!   そのまま含む(値ストリームそのものの解析失敗 = コンテンツ解析の失敗なので
//!   `InvalidContainer` が design 表の対応変種である)。`i64` に収まらない整数
//!   リテラルは serde_json が浮動小数リテラルへ黙って回送し、Visitor では
//!   むき出しの小数リテラルと区別できないため、[`from_json_bytes`] が文字列外の
//!   整数リテラルを事前走査して**解析した大きさ**を両方向に範囲検査する
//!   ([`check_integer_literals`])。

use serde::de::{Error as DeError, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

use crate::error::DocumentError;
use crate::ids::{AttachmentId, Blake3Digest};

/// wire 上のマーカーキー名(design 補完: むき出し文字列 3 変種の判別不能への脱出口)。
const TAG_KEY: &str = "$t";

/// マーカーと対で使う値本体のキー名。
const BODY_KEY: &str = "v";

/// `Text` の脱出口タグ。
const TAG_TEXT: &str = "text";

/// `Decimal` の中身が `Attachment` と読めてしまう場合の脱出口タグ。
const TAG_DECIMAL: &str = "decimal";

/// [`AttachmentId`] の正準 hex 長(64 文字)。
const ATTACHMENT_HEX_LEN: usize = Blake3Digest::LEN * 2;

/// セル値の閉じた集合(design「Logical Data Model」`CellValue` 表の 8 変種のみ)。
///
/// 汎用 JSON 値型を含まないため、整数と浮動小数の型ドリフトは構造上起こり得ない
/// (テスト `integer_and_float_literals_never_drift`)。
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    /// 値なし(wire: `null`)。
    Null,
    /// ブール(wire: `true` / `false`)。
    Bool(bool),
    /// 64 ビット整数(design 表の整数リテラル。2^53 の壁を越えない)。
    Int(i64),
    /// 浮動小数(design 表の数値リテラル)。非有限値は書き出し時に型付きエラーで
    /// 拒否し、`-0.0` は常に `0` として書く。
    Float(f64),
    /// 10 進数(design 表: 文字列)。桁数・有効性の妥当性は schema-engine が決める。
    /// 中身は検査せずにそのまま保持し、規則 1 で `Decimal` に決まらない中身は
    /// 脱出口(`{"$t":"decimal",...}`)で書くため、どんな文字列でも `Decimal` のまま
    /// 往復する(テスト `decimal_outside_the_grammar_roundtrips_verbatim`)。
    Decimal(String),
    /// テキスト(design 表: 文字列)。
    Text(String),
    /// オブジェクト / 配列(design 表: object / array。要件 3.6)。
    Nested(NestedValue),
    /// 添付参照(design 表: BLAKE3 hex のむき出し文字列。要件 7.3)。
    Attachment(AttachmentId),
}

impl CellValue {
    /// 浮動小数の値を作る。`-0.0` を `0.0` へ正規化する(設計表の「`-0.0` は `0`」)。
    #[inline]
    pub fn float(value: f64) -> Self {
        CellValue::Float(canonicalize_zero(value))
    }

    /// JSON に書けない値(非有限の浮動小数)を 1 つ以上含むか。
    /// [`to_json_bytes`] が書き出し前に走査する(不正な JSON を一切書かない)。
    fn contains_non_finite(&self) -> bool {
        match self {
            CellValue::Float(value) => !value.is_finite(),
            CellValue::Nested(nested) => nested.contains_non_finite(),
            _ => false,
        }
    }
}

/// `Nested` の中身: オブジェクトと配列(要件 3.6)。
#[derive(Debug, Clone, PartialEq)]
pub enum NestedValue {
    /// キー順をそのまま保持するオブジェクト。`HashMap` は反復順が実行ごとに
    /// 変わるため決定性を壊し、使用を禁止する(design「決定的出力」)。
    Object(Vec<(String, CellValue)>),
    /// 配列。
    Array(Vec<CellValue>),
}

impl NestedValue {
    fn contains_non_finite(&self) -> bool {
        match self {
            NestedValue::Object(entries) => entries
                .iter()
                .any(|(_, value)| value.contains_non_finite()),
            NestedValue::Array(items) => items.iter().any(CellValue::contains_non_finite),
        }
    }
}

/// `0` の符号を落とす(`-0.0 == 0.0` であることから、符号だけを検査できる)。
#[inline]
fn canonicalize_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

/// wire 上のむき出し文字列がどの変種に決まるか(モジュール docs の規則 1)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringKind {
    Attachment,
    Decimal,
    Text,
}

/// むき出しの文字列を変種へ決める。優先順は `Attachment` → `Decimal` → `Text`。
fn classify(text: &str) -> StringKind {
    if is_attachment_hex(text) {
        StringKind::Attachment
    } else if is_decimal(text) {
        StringKind::Decimal
    } else {
        StringKind::Text
    }
}

/// [`AttachmentId`] の正準形(64 文字の lowercase hex)か。
///
/// [`AttachmentId::from_hex`] は大文字も受理するため、正準形の判定は小文字である
/// ことまでこちらで確認する(大文字 hex は `Text` 側のデータである)。
fn is_attachment_hex(text: &str) -> bool {
    text.len() == ATTACHMENT_HEX_LEN
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && AttachmentId::from_hex(text).is_ok()
}

/// 10 進文法 `^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$` に一致するか。
///
/// 正準形は要求しない(`00.10` や `1E+3` も受理)。末尾の点(`+1.`)は受理し、
/// 数字を含まない `.` だけの表記は拒否する。
fn is_decimal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut cursor = 0usize;
    if matches!(byte_at(bytes, cursor), Some(b'+') | Some(b'-')) {
        cursor += 1;
    }
    let mut digits = 0usize;
    while matches!(byte_at(bytes, cursor), Some(b'0'..=b'9')) {
        cursor += 1;
        digits += 1;
    }
    if byte_at(bytes, cursor) == Some(b'.') {
        cursor += 1;
        while matches!(byte_at(bytes, cursor), Some(b'0'..=b'9')) {
            cursor += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return false;
    }
    if matches!(byte_at(bytes, cursor), Some(b'e') | Some(b'E')) {
        cursor += 1;
        if matches!(byte_at(bytes, cursor), Some(b'+') | Some(b'-')) {
            cursor += 1;
        }
        let exponent_start = cursor;
        while matches!(byte_at(bytes, cursor), Some(b'0'..=b'9')) {
            cursor += 1;
        }
        if cursor == exponent_start {
            return false;
        }
    }
    cursor == bytes.len()
}

/// 範囲外を `None` と読む(ASCII 比較なので UTF-8 の継続バイトはすべて非該当)。
#[inline]
fn byte_at(bytes: &[u8], index: usize) -> Option<u8> {
    bytes.get(index).copied()
}

/// セル値を JSON バイト列へ書き出す(**検査経路**。要件 3.3 系、3.6, 7.3)。
///
/// 書き出し前にツリー全体を走査し、非有限の浮動小数(`NaN` / `±Infinity`)が 1 つ
/// でもあれば [`DocumentError::NonRepresentableNumber`] を返す。このとき返るのは
/// エラーだけであり、**部分的なバイト列は生成されない**。`location` は診断に
/// 載せるセル位置を表す呼び出し元定義の文字列で、エラーへそのまま転記される。
///
/// 出力は決定的である(同じモデル → 同じバイト列): オブジェクトのキー順は保持し、
/// 整数は整数リテラル、浮動小数は最短位表記(`5.0` は `5.0` のまま。`5` と区別
/// される)、`-0.0` は `0` として書く。
pub fn to_json_bytes(value: &CellValue, location: &str) -> Result<Vec<u8>, DocumentError> {
    if value.contains_non_finite() {
        return Err(DocumentError::NonRepresentableNumber {
            location: location.into(),
        });
    }
    // 非有限値は上記で弾いたので、Serialize 側の検査は発火しない(型付きエラーと
    // location を運べるのはこちらの経路だけである)。
    serde_json::to_vec(value).map_err(|err| parse_error(&err))
}

/// JSON バイト列をセル値へ復元する(スキーマ解釈は行わない)。
///
/// モジュール docs の規則(むき出し文字列の 3 変種判別と `$t` マーカー)だけで
/// 決定的に読む。入力が JSON でない場合、`i64` に収まらない整数、非有限の数列
/// リテラルなどは [`DocumentError::InvalidContainer`] を返す(design 表に専用の
/// 応答変種が無いための写像であり、`entry` は `value: <理由>` となる)。整数は
/// [`check_integer_literals`] の範囲検査を先に通す(serde_json は範囲外の整数を
/// 黙って浮動小数へ回送するため、Visitor だけでは検出できない)。
pub fn from_json_bytes(bytes: &[u8]) -> Result<CellValue, DocumentError> {
    check_integer_literals(bytes)?;
    serde_json::from_slice(bytes).map_err(|err| parse_error(&err))
}

/// `i64` 範囲外の整数リテラルの事前範囲検査(読み取り側の門)。
///
/// serde_json の数値パーサは、`i64` に収まらない整数リテラル(正は `u64` 経路を
/// 越えるもの、負は `-9223372036854775809` のように符号付き変換で溢れるもの)を
/// 黙って `visit_f64` へ回送する。Visitor はソース文字列を持たないため
/// 正当な小数リテラル(`1e300`)と区別できず、Visitor 内では拒否できない。そこで
/// 文字列の外側だけを通り、点・指数を含まない整数リテラルに限定して**解析される
/// 大きさ**を両方向(`i64::MAX` 超 / `i64::MIN` 未満)で検査する。`.` や `e` を
/// 含む小数リテラルは `Float` の対象なのでそのまま通す。走査は 1 回・生経路は
/// ヒープ確保なし(確保は失敗時の診断文字だけ)。文字列内の数字はデータであり
/// 対象外(`Decimal` / `Text` の中身に範囲外の数字を書いてよい)。
fn check_integer_literals(bytes: &[u8]) -> Result<(), DocumentError> {
    let end = bytes.len();
    let mut cursor = 0usize;
    while cursor < end {
        match bytes[cursor] {
            b'"' => {
                // 文字列の中身はデータ(エスケープを含めて読み飛ばす)。
                cursor += 1;
                while cursor < end {
                    match bytes[cursor] {
                        b'"' => break,
                        b'\\' => cursor += 2,
                        _ => cursor += 1,
                    }
                }
                cursor += 1; // 閉じ引用符の外へ(未閉じなら serde_json が後で弾く)
            }
            b'-' | b'0'..=b'9' => {
                let start = cursor;
                cursor += usize::from(bytes[cursor] == b'-');
                while cursor < end && bytes[cursor].is_ascii_digit() {
                    cursor += 1;
                }
                if matches!(bytes.get(cursor), Some(b'.') | Some(b'e') | Some(b'E')) {
                    // 小数リテラル: 小数部と指数部まで進める(指数の数字を
                    // 別個の整数リテラルと誤認させない)。
                    if bytes[cursor] == b'.' {
                        cursor += 1;
                        while cursor < end && bytes[cursor].is_ascii_digit() {
                            cursor += 1;
                        }
                    }
                    if matches!(bytes.get(cursor), Some(b'e') | Some(b'E')) {
                        cursor += 1;
                        cursor += usize::from(matches!(bytes.get(cursor), Some(b'+') | Some(b'-')));
                        while cursor < end && bytes[cursor].is_ascii_digit() {
                            cursor += 1;
                        }
                    }
                } else if magnitude_beyond_i64(&bytes[start..cursor]) {
                    return Err(DocumentError::InvalidContainer {
                        entry: format!(
                            "value: integer `{}` is out of i64 range",
                            String::from_utf8_lossy(&bytes[start..cursor])
                        ),
                    });
                }
            }
            _ => cursor += 1,
        }
    }
    Ok(())
}

/// [`check_integer_literals`] が切り出した整数リテラル(`-?` に ASCII 数字だけが
/// 続く)が `i64` の範囲外か。先頭の 0 を落としてから `i64` の
/// 境界(`i64::MAX` / 負は `2^63`)と比較する。大きさは `u128` の飽和計算で求める
/// ため、どれほど長い桁列でもオーバーフローしない(飽和値は常に境界を超える)。
fn magnitude_beyond_i64(literal: &[u8]) -> bool {
    let negative = literal.first() == Some(&b'-');
    let digits = literal.strip_prefix(b"-").unwrap_or(literal);
    let zeros = digits.iter().take_while(|byte| **byte == b'0').count();
    let limit: u128 = if negative {
        1u128 << 63 // i64::MIN の絶対値は 2^63 まで受理
    } else {
        i64::MAX as u128
    };
    let mut magnitude: u128 = 0;
    for byte in &digits[zeros.min(digits.len())..] {
        magnitude = magnitude
            .saturating_mul(10)
            .saturating_add(u128::from(byte - b'0'));
    }
    magnitude > limit
}

/// 解析失敗をクレート共通エラーへ写す(design 表に対応変種の無いコンテンツ解析
/// 失敗はコンテナ不正として扱う。理由は文字列のまま `entry` に残す)。
fn parse_error(reason: &dyn fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("value: {reason}"),
    }
}

/// 書き出し側の変換。**未チェックの意味論**に注意:
///
/// [`serde::Serializer`] はクレート共通エラーを運べないため、非有限値は同じ
/// 意味の文字列エラー(`S::Error`)になる。NaN を `null` などの妥当な値へ黙って
/// 変換することはしない(型付きエラーが必要な経路は [`to_json_bytes`] を使う)。
impl Serialize for CellValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            CellValue::Null => serializer.serialize_none(),
            CellValue::Bool(value) => serializer.serialize_bool(*value),
            // 整数は整数リテラルで書く(serialize_f64 に渡すと `5.0` に化ける)。
            CellValue::Int(value) => serializer.serialize_i64(*value),
            CellValue::Float(value) => {
                if value.is_finite() {
                    serializer.serialize_f64(canonicalize_zero(*value))
                } else {
                    Err(<S::Error as serde::ser::Error>::custom(format_args!(
                        "NonRepresentableNumber: {value} is not representable in JSON"
                    )))
                }
            }
            CellValue::Decimal(text) => match classify(text) {
                // むき出しで書けるのは 1 の規則で `Decimal` に決まる中身だけ。
                // それ以外(Attachment hex・文法外・空文字列)は折り返し、
                // `decode(encode(Decimal)) == Decimal` を常に成立させる(規則 2)。
                StringKind::Decimal => serializer.serialize_str(text),
                _ => serialize_escape(serializer, TAG_DECIMAL, text),
            },
            CellValue::Text(text) => {
                if classify(text) == StringKind::Text {
                    serializer.serialize_str(text)
                } else {
                    serialize_escape(serializer, TAG_TEXT, text)
                }
            }
            // 添付参照は常にむき出しの hex(64 文字)。折り返しは不要で、行エントリの
            // 値としてそのまま添付を参照できる(要件 7.3)。
            CellValue::Attachment(id) => id.serialize(serializer),
            CellValue::Nested(nested) => nested.serialize(serializer),
        }
    }
}

impl Serialize for NestedValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            NestedValue::Object(entries) => write_object(serializer, entries),
            NestedValue::Array(items) => {
                let mut sequence = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    sequence.serialize_element(item)?;
                }
                sequence.end()
            }
        }
    }
}

/// オブジェクトを書く。`$` 始まりのキーはマーカーと衝突するため `$` を 1 つ足す
/// (読み側で 1 つ剥がす。モジュール docs 規則 3)。
fn write_object<S: Serializer>(
    serializer: S,
    entries: &[(String, CellValue)],
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(entries.len()))?;
    for (key, value) in entries {
        if key.starts_with('$') {
            map.serialize_entry(&format!("${key}"), value)?;
        } else {
            map.serialize_entry(key, value)?;
        }
    }
    map.end()
}

/// 脱出口 `{"$t": <tag>, "v": <内容>}` を書く。キーは常にこの順序で書く。
fn serialize_escape<S: Serializer>(
    serializer: S,
    tag: &str,
    text: &str,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(2))?;
    map.serialize_entry(TAG_KEY, tag)?;
    map.serialize_entry(BODY_KEY, text)?;
    map.end()
}

/// wire からモデルへ復元する(規則のすべてはモジュール docs。推測による修復はしない)。
impl<'de> Deserialize<'de> for CellValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

/// 自己記述的な JSON から [`CellValue`] へ復元する Visitor。
struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = CellValue;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a jxcel cell value")
    }

    fn visit_unit<E: DeError>(self) -> Result<CellValue, E> {
        Ok(CellValue::Null)
    }

    fn visit_none<E: DeError>(self) -> Result<CellValue, E> {
        Ok(CellValue::Null)
    }

    fn visit_bool<E: DeError>(self, value: bool) -> Result<CellValue, E> {
        Ok(CellValue::Bool(value))
    }

    /// 整数リテラルは `Int`(`Float` への化けは構造上あり得ない)。
    fn visit_i64<E: DeError>(self, value: i64) -> Result<CellValue, E> {
        Ok(CellValue::Int(value))
    }

    /// `i64` に収まらない整数は型付きエラー(黙って `Float` へ丸めない:
    /// 精度を落とす型変換は復元の過程で黙って起こってはならない)。
    fn visit_u64<E: DeError>(self, value: u64) -> Result<CellValue, E> {
        i64::try_from(value)
            .map(CellValue::Int)
            .map_err(|_| E::custom(format_args!("integer `{value}` is out of i64 range")))
    }

    /// 数値リテラルは `Float`。`-0.0` はここで `0.0` になる。非有限になる数値
    /// リテラル(例: `1e999`)は復元対象としない。
    fn visit_f64<E: DeError>(self, value: f64) -> Result<CellValue, E> {
        if value.is_finite() {
            Ok(CellValue::Float(canonicalize_zero(value)))
        } else {
            Err(E::custom(format_args!(
                "non-representable number literal `{value}` in JSON"
            )))
        }
    }

    fn visit_str<E: DeError>(self, value: &str) -> Result<CellValue, E> {
        Ok(text_to_value(value.to_owned()))
    }

    fn visit_string<E: DeError>(self, value: String) -> Result<CellValue, E> {
        Ok(text_to_value(value))
    }

    fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<CellValue, M::Error> {
        read_map(map)
    }

    fn visit_seq<S: SeqAccess<'de>>(self, seq: S) -> Result<CellValue, S::Error> {
        Ok(CellValue::Nested(NestedValue::Array(read_seq(seq)?)))
    }
}

/// むき出しの文字列を決定的に変種へ決める(モジュール docs 規則 1)。
fn text_to_value(text: String) -> CellValue {
    match classify(&text) {
        StringKind::Attachment => AttachmentId::from_hex(&text)
            .map_or(CellValue::Text(text), CellValue::Attachment),
        StringKind::Decimal => CellValue::Decimal(text),
        StringKind::Text => CellValue::Text(text),
    }
}

/// オブジェクトを読む。マーカー形 `{"$t": "text"|"decimal", "v": <文字列>}` に
/// 一致するときだけその変種へ復元し、それ以外(`$t` がタグでない、本体が文字列で
/// ない、キー順が違う、キーが足りない)はすべて `Nested` の object のままとする。
fn read_map<'de, M: MapAccess<'de>>(mut map: M) -> Result<CellValue, M::Error> {
    let mut entries: Vec<(String, CellValue)> = Vec::new();
    while let Some(key) = map.next_key::<String>()? {
        entries.push((key, map.next_value::<CellValue>()?));
    }
    // マーカー判定は**キーを書き換える前**の形でやる(`$$t` はユーザデータなので
    // マーカーと衝突しない。本クレートの書き出しは `$` 始まりのキーを出さない)。
    let escaped = if entries.len() == 2 && entries[0].0 == TAG_KEY && entries[1].0 == BODY_KEY {
        marker_tag(&entries[0].1).and_then(|tag| wire_text(&entries[1].1).map(|body| (tag, body)))
    } else {
        None
    };
    Ok(match escaped {
        Some((TAG_DECIMAL, body)) => CellValue::Decimal(body),
        Some((_, body)) => CellValue::Text(body),
        None => CellValue::Nested(NestedValue::Object(
            entries
                .into_iter()
                .map(|(key, value)| (unescape_key(key), value))
                .collect(),
        )),
    })
}

/// 配列を読む(要素は同じ手順で再帰的に復元する)。
fn read_seq<'de, S: SeqAccess<'de>>(mut seq: S) -> Result<Vec<CellValue>, S::Error> {
    let mut items = Vec::new();
    while let Some(item) = seq.next_element::<CellValue>()? {
        items.push(item);
    }
    Ok(items)
}

/// `$t` の値が既知のマーカー名か(それ以外の値は object のデータとして残す)。
fn marker_tag(value: &CellValue) -> Option<&'static str> {
    let CellValue::Text(text) = value else {
        return None;
    };
    if text == TAG_TEXT {
        Some(TAG_TEXT)
    } else if text == TAG_DECIMAL {
        Some(TAG_DECIMAL)
    } else {
        None
    }
}

/// 脱出口の本体(むき出し文字列変種のテキスト)。
fn wire_text(value: &CellValue) -> Option<String> {
    match value {
        CellValue::Text(text) | CellValue::Decimal(text) => Some(text.clone()),
        CellValue::Attachment(id) => Some(id.to_hex()),
        _ => None,
    }
}

/// 書き出しで `$` を 1 つ足したキーを戻す(モジュール docs 規則 3)。
fn unescape_key(mut key: String) -> String {
    if key.starts_with("$$") {
        key.remove(0);
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value;
    use crate::{AttachmentId, DocumentError};

    /// 診断ロケーションとしてテストが使うセル位置。
    const LOC: &str = "document.json#/rows/01HQ000000000000000000000000/values/price";

    fn attach(bytes: &[u8]) -> AttachmentId {
        AttachmentId::from_bytes(bytes)
    }

    /// 妥当な値の書き出しは成功する前提で文字列化する。
    fn encode(value: &CellValue) -> String {
        let bytes = value::to_json_bytes(value, LOC).expect("妥当な値の書き出しが失敗した");
        String::from_utf8(bytes).expect("出力は UTF-8 の JSON")
    }

    fn decode(text: &str) -> CellValue {
        value::from_json_bytes(text.as_bytes())
            .unwrap_or_else(|err| panic!("デコード失敗 {text}: {err}"))
    }

    fn decode_err(text: &str) -> DocumentError {
        value::from_json_bytes(text.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("不正な入力が通った: {text}"))
    }

    /// `decode(encode(v)) == v` と再エンコードのバイト同一性を検証し、wire を返す。
    fn roundtrip(value: &CellValue) -> String {
        let wire = encode(value);
        let back = decode(&wire);
        assert_eq!(*value, back, "decode(encode(v)) == v が成立しない: {wire}");
        let again = encode(&back);
        assert_eq!(wire, again, "再エンコードがバイト単位で一致しない");
        again
    }

    // --- (a) int / float の型ドリフトは起こり得ない ----------------------------------

    #[test]
    fn integer_and_float_literals_never_drift() {
        assert_eq!("5", encode(&CellValue::Int(5)));
        assert_eq!("5.0", encode(&CellValue::Float(5.0)));
        assert!(matches!(decode("5"), CellValue::Int(5)), "`5` は Int");
        assert!(!matches!(decode("5"), CellValue::Float(_)));
        assert!(matches!(decode("5.0"), CellValue::Float(_)), "`5.0` は Float");
        assert!(!matches!(decode("5.0"), CellValue::Int(_)));
        for value in [
            CellValue::Int(5),
            CellValue::Int(i64::MIN),
            CellValue::Int(i64::MAX),
            CellValue::Float(5.0),
            CellValue::Float(-0.25),
            CellValue::Float(1e300),
        ] {
            roundtrip(&value);
        }
    }

    #[test]
    fn numeric_text_stays_text_through_the_escape_hatch() {
        // むき出しの `"5"` は Decimal に決まる(モジュール docs 規則 1)。Text として
        // 保持した値は脱出口を通っても Text のまま復元される(要件 3.6)。
        assert_eq!(CellValue::Decimal("5".into()), decode("\"5\""));
        assert_eq!(
            r#"{"$t":"text","v":"5"}"#,
            roundtrip(&CellValue::Text("5".into()))
        );
        assert_eq!(
            r#"{"$t":"text","v":"5.0"}"#,
            roundtrip(&CellValue::Text("5.0".into()))
        );
    }

    #[test]
    fn null_and_bool_wire_forms() {
        assert_eq!("null", encode(&CellValue::Null));
        assert!(matches!(decode("null"), CellValue::Null));
        assert_eq!("true", roundtrip(&CellValue::Bool(true)));
        assert_eq!("false", roundtrip(&CellValue::Bool(false)));
    }

    // --- (b) NaN / Infinity は型付きエラーで拒否(入れ子を含む) ----------------------

    #[test]
    fn non_finite_floats_are_rejected_with_location() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = value::to_json_bytes(&CellValue::Float(bad), LOC).unwrap_err();
            match err {
                DocumentError::NonRepresentableNumber { location } => assert_eq!(LOC, location),
                other => panic!("{other:?} は NaN/Inf のエラーではない"),
            }
        }
    }

    #[test]
    fn non_finite_floats_inside_nested_values_are_rejected() {
        let in_object = CellValue::Nested(NestedValue::Object(vec![
            ("ok".into(), CellValue::Int(1)),
            ("bad".into(), CellValue::Float(f64::INFINITY)),
        ]));
        let in_array = CellValue::Nested(NestedValue::Array(vec![
            CellValue::Text("x".into()),
            CellValue::Nested(NestedValue::Array(vec![CellValue::Float(f64::NEG_INFINITY)])),
        ]));
        for nested in [in_object, in_array] {
            let err = value::to_json_bytes(&nested, LOC).unwrap_err();
            match err {
                DocumentError::NonRepresentableNumber { location } => assert_eq!(LOC, location),
                other => panic!("入れ子の NaN/Inf が別エラーになった: {other:?}"),
            }
        }
        // 有限値だけからなる入れ子は書き出せる(拒否が行き過ぎていないこと)。
        assert_eq!(
            r#"{"ok":1}"#,
            encode(&CellValue::Nested(NestedValue::Object(vec![(
                "ok".into(),
                CellValue::Int(1)
            )])))
        );
    }

    #[test]
    fn serialize_reports_the_variant_name_and_never_null() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = serde_json::to_string(&CellValue::Float(bad))
                .err()
                .expect("NaN/Inf が Serialize で黙って通った");
            assert!(
                err.to_string().contains("NonRepresentableNumber"),
                "シリアライズエラーに変種名が現れない: {err}"
            );
        }
    }

    // --- (c) -0.0 は 0 として出力される ----------------------------------------------

    #[test]
    fn negative_zero_is_written_as_zero() {
        assert!(
            matches!(CellValue::float(-0.0), CellValue::Float(f) if f.to_bits() == 0.0f64.to_bits()),
            "float() は -0.0 を 0.0 へ正規化する"
        );
        // 構造体を直接構築した場合も符号は書き出されない。
        assert_eq!("0.0", encode(&CellValue::Float(-0.0)));
        match decode("0.0") {
            CellValue::Float(f) => assert_eq!(0.0f64.to_bits(), f.to_bits()),
            other => panic!("0.0 が Float でない: {other:?}"),
        }
        roundtrip(&CellValue::Float(-0.0));
    }

    // --- (c2) Decimal 文法と Text のエスケープ ---------------------------------------

    #[test]
    fn decimal_grammar_strings_decode_as_decimal() {
        for text in ["1.5", "+1.", ".5", "1e10", "-0.25", "42"] {
            let wire = format!("\"{text}\"");
            let value = decode(&wire);
            assert_eq!(CellValue::Decimal(text.into()), value, "{text} の分類が不正");
            assert_eq!(wire, encode(&value), "Decimal はむき出しの文字列で書く");
            roundtrip(&value);
        }
    }

    #[test]
    fn strings_outside_the_decimal_grammar_are_text() {
        assert_eq!(CellValue::Text("1.5abc".into()), decode("\"1.5abc\""));
        assert_eq!("\"1.5abc\"", roundtrip(&CellValue::Text("1.5abc".into())));
        assert!(matches!(decode("\"\""), CellValue::Text(_)));
    }

    #[test]
    fn text_that_reads_as_decimal_is_escaped() {
        for content in ["1.5", "1E5", "+1.", ".5", "1e10"] {
            let value = CellValue::Text(content.into());
            assert_eq!(
                format!("{{\"$t\":\"text\",\"v\":\"{content}\"}}"),
                encode(&value),
                "Decimal と読める Text はエスケープ対象"
            );
            roundtrip(&value);
        }
    }

    #[test]
    fn text_that_reads_as_attachment_is_escaped() {
        let hex = attach(b"payload").to_hex();
        let value = CellValue::Text(hex.clone());
        assert_eq!(
            format!("{{\"$t\":\"text\",\"v\":\"{hex}\"}}"),
            encode(&value),
            "Attachment と読める Text はエスケープ対象"
        );
        roundtrip(&value);
    }

    #[test]
    fn attachment_is_a_bare_hex_value() {
        let id = attach(b"attachment payload");
        let hex = id.to_hex();
        assert_eq!(64, hex.len());
        let value = CellValue::Attachment(id);
        // むき出しの 64 文字 hex(エスケープ对象ではない) — 要件 7.3 の行→添付参照。
        assert_eq!(format!("\"{hex}\""), encode(&value));
        assert_eq!(value, decode(&format!("\"{hex}\"")));
        roundtrip(&value);
        // 行エントリの値としての形(入れ子の内でも同じ)。
        let row_entry = CellValue::Nested(NestedValue::Object(vec![("file".into(), value)]));
        assert_eq!(format!("{{\"file\":\"{hex}\"}}"), roundtrip(&row_entry));
    }

    #[test]
    fn hex_like_strings_are_text_not_attachments() {
        let hex = attach(b"payload").to_hex();
        let upper = hex.to_uppercase();
        assert_ne!(hex, upper, "テスト前提: 小文字 hex に英字が含まれる");
        // 正準形でない 64 文字は Attachment でない → Text むき出し。
        assert_eq!(CellValue::Text(upper.clone()), decode(&format!("\"{upper}\"")));
        assert_eq!(format!("\"{upper}\""), roundtrip(&CellValue::Text(upper)));
        // 63 文字も Text(長さが決定的な判別基準)。
        let short = "f".repeat(63);
        assert_eq!(CellValue::Text(short.clone()), decode(&format!("\"{short}\"")));
        roundtrip(&CellValue::Text(short));
    }

    // --- (d) Nested のキー順はそのまま保たれる ---------------------------------------

    #[test]
    fn nested_object_key_order_is_preserved_verbatim() {
        let value = CellValue::Nested(NestedValue::Object(vec![
            ("b".into(), CellValue::Int(1)),
            ("a".into(), CellValue::Bool(true)),
            ("b2".into(), CellValue::Null),
        ]));
        assert_eq!(r#"{"b":1,"a":true,"b2":null}"#, roundtrip(&value));
        match decode(r#"{"b":1,"a":true,"b2":null}"#) {
            CellValue::Nested(NestedValue::Object(entries)) => assert_eq!(
                vec!["b", "a", "b2"],
                entries.iter().map(|(key, _)| key.as_str()).collect::<Vec<_>>()
            ),
            other => panic!("Nested でない: {other:?}"),
        }
    }

    #[test]
    fn mixed_nested_structures_roundtrip() {
        let value = CellValue::Nested(NestedValue::Object(vec![
            (
                "arr".into(),
                CellValue::Nested(NestedValue::Array(vec![
                    CellValue::Int(-7),
                    CellValue::Float(1.25),
                    CellValue::Text("1.5".into()),
                    CellValue::Decimal("1.5".into()),
                    CellValue::Attachment(attach(b"nested")),
                    CellValue::Null,
                ])),
            ),
            ("empty".into(), CellValue::Nested(NestedValue::Array(vec![]))),
        ]));
        roundtrip(&value);
    }

    #[test]
    fn data_keys_colliding_with_the_marker_are_escaped() {
        // マーカー `$t` と同じ名前のキーは `$` を 1 つ足して書き、読み時に 1 つ
        // 剥がして戻す(モジュール docs 規則 3)。これでマーカー形との衝突が起きない。
        let value = CellValue::Nested(NestedValue::Object(vec![
            ("$t".into(), CellValue::Text("text".into())),
            ("v".into(), CellValue::Text("hi".into())),
        ]));
        assert_eq!(r#"{"$$t":"text","v":"hi"}"#, encode(&value));
        roundtrip(&value);
        // マーカーと無関係な `$` 始まりのキーも同じ規則で往復する。
        let dollars = CellValue::Nested(NestedValue::Object(vec![
            ("$$id".into(), CellValue::Text("x".into())),
            ("$schema".into(), CellValue::Int(1)),
        ]));
        assert_eq!(r#"{"$$$id":"x","$$schema":1}"#, encode(&dollars));
        roundtrip(&dollars);
    }

    #[test]
    fn decimal_written_as_attachment_hex_is_escaped() {
        // 64 桁の 10 進文字列は Attachment の hex 文法にも一致するため折り返す。
        let text = "1".repeat(64);
        let value = CellValue::Decimal(text.clone());
        assert_eq!(format!("{{\"$t\":\"decimal\",\"v\":\"{text}\"}}"), encode(&value));
        roundtrip(&value);
    }

    #[test]
    fn malformed_or_reordered_escape_objects_are_nested() {
        // マーカー形に一致しないオブジェクトはすべて `Nested` の object に決まる
        // (タグでもない値の推測・修復はしない)。`$t` を含む外部入力の再エンコードは
        // `$$t` になる(本クレート自身の書き出しはむき出しの `$t` キーを持たない)。
        for wire in [
            r#"{"$t":"text"}"#,
            r#"{"$t":"text","v":1}"#,
            r#"{"v":"1.5","$t":"text"}"#,
            r#"{"$t":"other","v":"x"}"#,
            r#"{"$t":"text","v":"a","v":"b"}"#,
        ] {
            let value = decode(wire);
            assert!(
                matches!(value, CellValue::Nested(NestedValue::Object(_))),
                "{wire} は Nested で読む: {value:?}"
            );
            roundtrip(&value);
        }
    }

    #[test]
    fn text_mimicking_the_escape_object_stays_bare_text() {
        let value = CellValue::Text(r#"{"$t":"text","v":"x"}"#.into());
        let wire = encode(&value);
        assert!(wire.starts_with('"'), "Text は常に文字列: {wire}");
        roundtrip(&value);
    }

    // --- 不正入力の型付きエラー -----------------------------------------------------

    #[test]
    fn invalid_json_is_an_invalid_container() {
        for garbage in ["{", "[1,", "nul", "", "1e999", "[", "\"unterminated"] {
            match decode_err(garbage) {
                DocumentError::InvalidContainer { .. } => {}
                other => panic!("{garbage} が InvalidContainer でない: {other:?}"),
            }
        }
    }

    #[test]
    fn integer_beyond_i64_is_an_invalid_container() {
        match decode_err("9223372036854775808") {
            DocumentError::InvalidContainer { entry } => {
                assert!(entry.contains("i64"), "診断に範囲情報を含める: {entry}")
            }
            other => panic!("i64 超の整数が別エラー: {other:?}"),
        }
    }

    // --- (e) Decimal の全単射: 文法外の中身は Text へ漂着してはならない ----------------

    #[test]
    fn decimal_outside_the_grammar_roundtrips_verbatim() {
        // 回帰(回帰): 10 進文法外の Decimal は折り返されず、むき出しの文字列に
        // 化けて Text に化けていた(空文字列も同じ)。今は常に折り返しである。
        assert_eq!(
            r#"{"$t":"decimal","v":"hello"}"#,
            encode(&CellValue::Decimal("hello".into()))
        );
        assert_eq!(
            r#"{"$t":"decimal","v":""}"#,
            encode(&CellValue::Decimal(String::new()))
        );
        for text in [
            "hello", "", " ", "1.5abc", "1e", "1E+5x", "-", ".", "١٢٣", "3.1.4",
            "1_000", "1.5\"", "\\1.5", "NaN", "Infinity", "0x10",
        ] {
            roundtrip(&CellValue::Decimal(text.into()));
        }
        // 文法に収まる中身はむき出しの文字列のまま(design 表を可能な限り文字どおり)。
        assert_eq!("\"1.5\"", encode(&CellValue::Decimal("1.5".into())));
    }

    #[test]
    fn decimal_hatch_accepts_verbatim_bodies() {
        // 読み側はタグ `decimal` の本体(文字列)を文法で検証・修復せずそのまま
        // Decimal にする。むき出しでは Text / Attachment に読まれる内容でも成立する。
        let hex = attach(b"hatch").to_hex();
        let long = "1".repeat(64);
        for text in ["not-decimal", "", "5", "1E5", "hello", &hex, &long] {
            let wire = format!(r#"{{"$t":"decimal","v":"{text}"}}"#);
            assert_eq!(CellValue::Decimal(text.into()), decode(&wire), "{wire}");
        }
        // 本体が文字列でなければ従来どおり Nested の object(既知の規則)。
        for wire in [
            r#"{"$t":"decimal","v":1}"#,
            r#"{"$t":"decimal","v":true}"#,
            r#"{"$t":"decimal","v":["5"]}"#,
            r#"{"$t":"decimal","v":{"a":"5"}}"#,
        ] {
            let value = decode(wire);
            assert!(
                matches!(value, CellValue::Nested(NestedValue::Object(_))),
                "{wire} は Nested で読む: {value:?}"
            );
        }
    }

    // --- (f) f64 リテラルのビット単位一致(float_roundtrip の回帰) ----------------------

    #[test]
    fn float_literals_survive_with_bit_identity() {
        // 既定パースで 1 ULP 漂う約 3 割の代表値と境界値の一組を、wire を介した
        // 往復が to_bits() 単位で一致し、再エンコードがバイト単位で同じことまで検証する。
        let cases: [f64; 9] = [
            0.1 + 0.2,              // 0.30000000000000004(17 有効数字)
            1.0000000000000002,     // 1.0 の次の浮動小数
            -4.0000000000000009,    // -4 の 1 ULP 下
            9007199254740993.0,     // 2^53 + 1(奇数の壁)
            5e-324,                 // 最小非正規数
            f64::MIN_POSITIVE,      // 正規数の最小
            2.225073970955251e-308, // MIN_POSITIVE から 1 ULP 下(非正規)
            1e300,
            f64::MAX, // 有限の最大
        ];
        for value in cases {
            let wire = encode(&CellValue::Float(value));
            let back = decode(&wire);
            match back {
                CellValue::Float(restored) => assert_eq!(
                    value.to_bits(),
                    restored.to_bits(),
                    "`{wire}` が 1 ULP 漂った({value} -> {restored})"
                ),
                other => panic!("`{wire}` の復元が Float でない: {other:?}"),
            }
            assert_eq!(wire, encode(&back), "Float の再エンコードがバイト一致しない");
        }
        // 単純な浮動小数の wire は float_roundtrip の導入で変わらない
        // (書き出しは zmij の最短位表記で feature 有無と無関係。指数は `1e+300` の
        // ように符号付きになるが、これは float_roundtrip の導入前と同じ wire である)。
        assert_eq!("5.0", encode(&CellValue::Float(5.0)));
        assert_eq!("0.0", encode(&CellValue::Float(-0.0)));
        assert_eq!("1e+300", encode(&CellValue::Float(1e300)));
        assert_eq!("5e-324", encode(&CellValue::Float(5e-324)));
    }

    // --- (g) i64 範囲外の整数リテラルは両方向でエラー(型ドリフト禁止) ------------------

    #[test]
    fn integer_literals_beyond_i64_error_in_both_directions() {
        // 回帰: 正の 2^63 超は visit_u64 で弾いていたが、それ以下(負側)と
        // 2^64 以上の正は serde_json が浮動小数へ回送して Float に化けていた。
        for literal in [
            "9223372036854775808",    // i64::MAX + 1
            "-9223372036854775809",   // i64::MIN - 1
            "18446744073709551615",   // u64::MAX(u64 に収まるが i64 に収まらない)
            "18446744073709551616",   // 2^64
            "-18446744073709551617",  // -(2^64 + 1)
            "99999999999999999999999999999999999999999999999999", // 50 桁
        ] {
            for wire in [
                literal.to_string(),
                format!("[{literal},1]"),
                format!(r#"{{"k":{literal}}}"#),
            ] {
                match decode_err(&wire) {
                    DocumentError::InvalidContainer { entry } => {
                        assert!(entry.contains("i64"), "診断に範囲情報を含める: {entry}")
                    }
                    other => panic!("範囲外の整数が別エラー: {wire} → {other:?}"),
                }
            }
        }
        // 境界そのものは従来どおり復元できる。
        assert!(matches!(decode("9223372036854775807"), CellValue::Int(i64::MAX)));
        assert!(matches!(decode("-9223372036854775808"), CellValue::Int(i64::MIN)));
        // 範囲外の数字は**文字列データ**なら対象外(文字列データはそのまま復元)。
        assert!(matches!(
            decode("\"18446744073709551616\""),
            CellValue::Decimal(_)
        ));
        assert!(matches!(
            decode(r#"{"a":"-9223372036854775809"}"#),
            CellValue::Nested(_)
        ));
        roundtrip(&CellValue::Decimal("18446744073709551616".into()));
        // 整数だが整数リテラルでない小数表記は Float のまま(有効な f64 である)。
        for wire in ["9223372036854775808.0", "1e300", "-1e300"] {
            assert!(matches!(decode(wire), CellValue::Float(_)), "{wire} は Float");
        }
    }
}

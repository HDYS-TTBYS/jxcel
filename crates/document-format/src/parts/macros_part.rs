//! マクロパート(`macros.json`)の符号化・復号(タスク 1.2。要件 1.1, 1.2, 1.5)。
//!
//! マクロの記録の並びを運ぶ**形だけ**のパートである。解釈(ソースの検証・能力宣言の
//! 読み取り・名前の一意性)は本クレートでは行わない([`crate::model::MacroRecord`] の
//! モジュール docs。design 決定 4)。本モジュールが所有するのは wire 形と、その形に
//! ついての検査(型・必須キー・種別テキストの閉じた集合)である。
//!
//! # JSON 形式(確定形)
//!
//! ```json
//! {
//!   "macros": [
//!     {"name": "棚卸し", "kind": "typescript", "source": "const xs = readRows(...);"}
//!   ]
//! }
//! ```
//!
//! - トップレベルはオブジェクトで、既知キーは `macros` の 1 つだけである。要素は
//!   `name` → `kind` → `source` の宣言順で書き、**並びは与えられた順のまま**保持する
//!   (保存順が一覧の提示順である。design「Logical Data Model」)。
//! - `kind` のテキストは **`typescript` / `javascript`** の 2 つだけである
//!   ([`MacroKind`] の閉じた集合の写し)。未知のテキストは**拒否**する(既定値へ丸める
//!   経路を作らない)。
//! - `source` は文字列として、エスケープ以外の変換なしに書く(整形しない。要件 1.5)。
//! - 出力はコンパクトな UTF-8 で、末尾改行を付けない(決定性の層の規則。
//!   [`crate::json`] のモジュール docs)。
//!
//! ファインダビリティのため**空の並びも表現できる**(`{"macros":[]}` は妥当な入力である)。
//! ただし保存経路([`crate::parts::to_parts`])はマクロを 1 件も持たない文書でこのパートを
//! **書かない**([`crate::parts::to_parts`] の docs「省略可能なパート」)。
//!
//! # 未知フィールドの保持(前方互換。要件 6.2 / 6.3)
//!
//! トップレベルと要素のそれぞれで、解釈しないキーを破棄せず**原文の位置**へ差し戻して
//! 書き戻す([`crate::parts::DocumentPart`] と同じ扱い。機構は [`crate::json`] の
//! [`PreservingObjectWriter`] 1 か所にある)。
//!
//! # 本パートが強制する不変条件
//!
//! 型式だけである: `macros` が配列であること、要素がオブジェクトであること、
//! `name` / `kind` / `source` が存在して文字列であること、`kind` が 2 つのテキストの
//! いずれかであること、同じオブジェクト内でキーが重複しないこと。**名前の一意性は
//! 強制しない**(並びの中に同じ名前が 2 件あっても本クレートは拒否しない。一意性と
//! 置き換えは下流の規則である)。
//!
//! # 依存方向
//!
//! `Ids / Value / EntryName → Model → Json → Parts → Container → Api` の一方向
//! (`parts/mod.rs` のモジュール docs)。本モジュールは [`crate::model::MacroRecord`] /
//! [`crate::json`] / [`crate::error`] / [`crate::entry_name`] に依存し、`container` には
//! 依存しない。
//!
//! # エラー対応
//!
//! 復号の失敗は中止であり、部分的な記録の並びを返さない。design のエラー表に本パート固有の
//! 変種が無いため、既存規約どおり [`DocumentError::InvalidContainer`] の `entry` に
//! `macros.json: <理由>` を載せる([`crate::parts::document_part`] と同じ規約)。
//! 新しい [`DocumentError`] 変種は足さない。

use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

use crate::entry_name::EntryName;
use crate::error::DocumentError;
use crate::json::{KnownFields, PreservedFields, PreservingObjectWriter};
use crate::model::{MacroKind, MacroRecord};

/// トップレベルの既知キー: マクロの記録の並び(宣言順の 1 番目)。
const MACROS_KEY: &str = "macros";
/// 要素の既知キー: 名前(宣言順の 1 番目)。
const NAME_KEY: &str = "name";
/// 要素の既知キー: 種別(宣言順の 2 番目)。
const KIND_KEY: &str = "kind";
/// 要素の既知キー: ソース(宣言順の 3 番目)。
const SOURCE_KEY: &str = "source";

/// 種別 [`MacroKind::TypeScript`] の wire テキスト。
const TYPESCRIPT_TEXT: &str = "typescript";
/// 種別 [`MacroKind::JavaScript`] の wire テキスト。
const JAVASCRIPT_TEXT: &str = "javascript";

/// マクロパート(`macros.json`): マクロの記録(名前・種別・ソース)の並び。
///
/// フィールドは非公開で、[`MacrosPart::new`]（保存経路）と
/// [`MacrosPart::from_json_bytes`]（読み込み経路）の 2 経路だけが構築する。どちらも並びを
/// **与えられた順のまま**保持し、並べ替えない(モジュール docs「JSON 形式」)。
///
/// `PartialEq` は提供しない: 保持している未知フィールドの比較には読み込みカーソル
/// ([`PreservedFields`] の内部状態)が混じるためである([`crate::parts::DocumentPart`] と
/// 同じ方針)。同じ内容かどうかは [`MacrosPart::records`] を見るか、符号化したバイト列を
/// 比べて判定する(後者が本形式の契約: 同一内容は常に同一バイト列)。
#[derive(Debug)]
pub struct MacrosPart {
    /// マクロの記録の並び(保存順。並べ替えない)。
    records: Vec<MacroRecord>,
    /// 解釈しないトップレベルのフィールド(前方互換。要件 6.2 / 6.3)。
    preserved: PreservedFields,
}

impl MacrosPart {
    /// マクロの記録の列から組み立てる(保存経路)。
    ///
    /// 列は**与えられた順序のまま**保持する(並べ替えない)。内容の検査は行わない
    /// (モジュール docs「本パートが強制する不変条件」)。
    #[inline]
    pub fn new(records: Vec<MacroRecord>) -> Self {
        Self {
            records,
            preserved: PreservedFields::new(),
        }
    }

    /// マクロの記録の並び(保存順がそのまま一覧の提示順である)。
    #[inline]
    pub fn records(&self) -> &[MacroRecord] {
        &self.records
    }

    /// `macros.json` のトップレベルで保持した未知キー(原文の位置ごと。要件 6.2 / 6.3)。
    ///
    /// 読み込み経路 `parts::from_parts` がこれをモデル([`crate::model::Document`])へ移し、
    /// 保存経路が [`MacrosPart::with_preserved`] で戻す。
    #[inline]
    pub(crate) fn preserved_fields(&self) -> &PreservedFields {
        &self.preserved
    }

    /// 保持すべき未知フィールドを据える(読み込み経路の復元用。クレート可視)。
    #[inline]
    pub(crate) fn with_preserved(mut self, preserved: PreservedFields) -> Self {
        self.preserved = preserved;
        self
    }

    /// 確定形の JSON バイト列へ符号化する(コンパクトな UTF-8・末尾改行なし)。
    ///
    /// キー順序・並びの順序・種別テキストはモジュール docs の確定形に従う。未知フィールドは
    /// トップレベルと要素のそれぞれで原文の位置へ差し戻す。失敗したときは `Err` を返し、
    /// 出力先へ 1 バイトも書かない([`PreservingObjectWriter`] の規律を継承)。
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        // 要素ごとに差し戻し位置が決まるため、先に配列全体を組み立ててから raw 値として
        // 差し込む(`RawValue` が verbatim の唯一の経路)。`from_string` は本クレートが
        // 生成した妥当な JSON 配列を 1 回検証するだけで、値の再解釈はしない。
        let macros = self.macros_json_bytes()?;
        let macros_raw = RawValue::from_string(String::from_utf8(macros).map_err(invalid_macros)?)
            .map_err(invalid_macros)?;

        let mut out = Vec::new();
        {
            // 未知フィールドを原文の位置へ差し戻しながら、既知キーを宣言順に書く。
            // 書き出しは `finish` まで一時バッファに留まる(失敗時に 1 バイトも出さない)。
            let mut writer = PreservingObjectWriter::new(&mut out, &self.preserved);
            writer.write_known(MACROS_KEY, macros_raw.as_ref())?;
            writer.finish()?;
        }
        Ok(out)
    }

    /// マクロの記録を決定的な JSON 配列として組み立てる。
    ///
    /// 並びは [`MacrosPart::records`] の順序そのまま(並べ替えない)。各要素は自分の
    /// 未知キーを原文の位置へ差し戻す([`record_json_bytes`])。
    fn macros_json_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        let mut out = Vec::new();
        out.push(b'[');
        for (index, record) in self.records.iter().enumerate() {
            if index > 0 {
                out.push(b',');
            }
            out.extend_from_slice(&record_json_bytes(record)?);
        }
        out.push(b']');
        Ok(out)
    }

    /// 確定形の JSON バイト列から復号する。
    ///
    /// 失敗は中止であり、部分的な記録の並びを返さない(モジュール docs「エラー対応」)。
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, DocumentError> {
        let raw: RawMacros = serde_json::from_slice(bytes).map_err(invalid_macros)?;
        Self::from_raw(raw)
    }

    /// 構文レベルの読み取り結果を検証して組み立てる。
    ///
    /// 型式の失敗(serde が返すもの)は [`invalid_macros`] がコンテナ不正へ写し、
    /// キーの重複と `kind` の未知テキストはここが型付きの文脈で返す。
    fn from_raw(raw: RawMacros) -> Result<Self, DocumentError> {
        if let Some(key) = raw.known.duplicate() {
            return Err(invalid_macros(format!("duplicate field `{key}`")));
        }
        let raw_records = raw
            .macros
            .ok_or_else(|| invalid_macros(format!("missing field `{MACROS_KEY}`")))?;
        let mut records = Vec::with_capacity(raw_records.len());
        for raw_record in raw_records {
            if let Some(key) = raw_record.known.duplicate() {
                return Err(invalid_macros(format!("duplicate field `{key}`")));
            }
            records.push(
                MacroRecord::new(
                    raw_record.name,
                    parse_kind(&raw_record.kind)?,
                    raw_record.source,
                )
                .with_preserved(raw_record.known.into_preserved()),
            );
        }
        Ok(Self {
            records,
            preserved: raw.known.into_preserved(),
        })
    }
}

/// マクロの記録 1 件を確定形の JSON オブジェクトとして書き出す。
///
/// キー順序は宣言順(`name` → `kind` → `source`)で、未知キーは
/// [`PreservedFields::record_known_field`] と対になる位置へ差し戻す。失敗したときは
/// 1 バイトも書かない([`PreservingObjectWriter`] の規律を継承)。
fn record_json_bytes(record: &MacroRecord) -> Result<Vec<u8>, DocumentError> {
    let mut out = Vec::new();
    let mut writer = PreservingObjectWriter::new(&mut out, record.preserved_fields());
    writer.write_known(NAME_KEY, record.name())?;
    writer.write_known(KIND_KEY, kind_text(record.kind()))?;
    writer.write_known(SOURCE_KEY, record.source())?;
    writer.finish()?;
    Ok(out)
}

/// 種別の wire テキスト(閉じた集合の写し。モジュール docs「JSON 形式」)。
fn kind_text(kind: MacroKind) -> &'static str {
    match kind {
        MacroKind::TypeScript => TYPESCRIPT_TEXT,
        MacroKind::JavaScript => JAVASCRIPT_TEXT,
    }
}

/// wire テキストから種別を読む。未知のテキストは受理しない(既定値へ丸めない)。
fn parse_kind(text: &str) -> Result<MacroKind, DocumentError> {
    match text {
        TYPESCRIPT_TEXT => Ok(MacroKind::TypeScript),
        JAVASCRIPT_TEXT => Ok(MacroKind::JavaScript),
        other => Err(invalid_macros(format!("unknown macro kind `{other}`"))),
    }
}

/// マクロ要素(`{"name":..,"kind":..,"source":..}`)の構文レベルの読み取り結果(検証前)。
///
/// 要素内の未知キーは [`PreservedFields`] が原文の位置ごと保持する(要素 1 件につき 1 個)。
struct RawMacro {
    name: String,
    kind: String,
    source: String,
    /// 既知キーの帳簿(重複の記録と、未知フィールドの差し戻し位置)。
    known: KnownFields,
}

impl<'de> Deserialize<'de> for RawMacro {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RawMacroVisitor)
    }
}

/// マクロ要素を読む訪問者。
struct RawMacroVisitor;

impl<'de> Visitor<'de> for RawMacroVisitor {
    type Value = RawMacro;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a macros part element")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawMacro, A::Error> {
        let mut name: Option<String> = None;
        let mut kind: Option<String> = None;
        let mut source: Option<String> = None;
        // 既知キーの読み取りは順序が意味を持つ(重複の記録 → 値 → 差し戻し位置の前進)。
        // [`KnownFields`] がその順序ごと持つ。
        let mut known = KnownFields::new();

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                NAME_KEY => known.read(&mut name, NAME_KEY, &mut map)?,
                KIND_KEY => known.read(&mut kind, KIND_KEY, &mut map)?,
                SOURCE_KEY => known.read(&mut source, SOURCE_KEY, &mut map)?,
                _ => known.capture(&key, &mut map)?,
            }
        }

        let Some(name) = name else {
            return Err(de::Error::missing_field(NAME_KEY));
        };
        let Some(kind) = kind else {
            return Err(de::Error::missing_field(KIND_KEY));
        };
        // ソースは空文字列も正当である(空のソースを拒否するのは意味の判断であり、
        // 下流の規則である)。キーの欠落だけを拒否する。
        let Some(source) = source else {
            return Err(de::Error::missing_field(SOURCE_KEY));
        };

        Ok(RawMacro {
            name,
            kind,
            source,
            known,
        })
    }
}

/// `macros.json` 全体の構文レベルの読み取り結果(検証前)。
///
/// 未知のトップレベルキーは [`PreservedFields`] が原文の位置ごと保持する。
struct RawMacros {
    macros: Option<Vec<RawMacro>>,
    /// 既知キーの帳簿(重複の記録と、未知フィールドの差し戻し位置)。
    known: KnownFields,
}

impl<'de> Deserialize<'de> for RawMacros {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RawMacrosVisitor)
    }
}

/// `macros.json` のトップレベルを読む訪問者(既知キー `macros` だけを解釈し、他は
/// [`PreservedFields`] へ回す)。
struct RawMacrosVisitor;

impl<'de> Visitor<'de> for RawMacrosVisitor {
    type Value = RawMacros;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a macros part object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawMacros, A::Error> {
        let mut macros: Option<Vec<RawMacro>> = None;
        // 既知キーの読み取りは順序が意味を持つ(重複の記録 → 値 → 差し戻し位置の前進)。
        let mut known = KnownFields::new();

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                MACROS_KEY => known.read(&mut macros, MACROS_KEY, &mut map)?,
                _ => known.capture(&key, &mut map)?,
            }
        }

        Ok(RawMacros { macros, known })
    }
}

/// 読み込みのコンテンツ解析失敗を [`DocumentError::InvalidContainer`] へ写す。
///
/// design のエラー表に本パート固有の変種が無いための写像であり、`entry` に
/// `macros.json: <理由>` を残す([`crate::parts::document_part`] と同じ規約)。エントリ名は
/// [`EntryName::Macros`] の表示テキストを使う(文字列リテラルを経路へ散在させない)。
fn invalid_macros(reason: impl fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("{}: {reason}", EntryName::Macros),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 標本のマクロ 1 件。
    fn record(name: &str, kind: MacroKind, source: &str) -> MacroRecord {
        MacroRecord::new(name, kind, source)
    }

    /// 標本の並び(順序が意味を持つことを見るため、名前の辞書順とは違う順で並べる)。
    fn records() -> Vec<MacroRecord> {
        vec![
            record("棚卸し", MacroKind::TypeScript, "const xs = 1;\n"),
            record("alert", MacroKind::JavaScript, "alert(1)"),
        ]
    }

    /// 符号化と復号がバイト単位で往復し、同じ内容からは常に同じバイト列になる。
    ///
    /// 復号の比較は並び・名前・種別・ソースの 4 点である(`MacrosPart` は `PartialEq` を
    /// 持たない。保持カーソルが等値に混じるため)。
    #[test]
    fn encodes_and_decodes_byte_identically_and_deterministically() {
        let part = MacrosPart::new(records());
        let bytes = part.to_json_bytes().expect("符号化");

        assert_eq!(
            bytes,
            part.to_json_bytes().expect("符号化"),
            "2 回の出力が違う"
        );
        let decoded = MacrosPart::from_json_bytes(&bytes).expect("復号");
        assert_eq!(
            bytes,
            decoded.to_json_bytes().expect("再符号化"),
            "復号して書き戻すとバイト列が変わる"
        );

        assert_eq!(2, decoded.records().len(), "件数が違う");
        for (expected, actual) in records().iter().zip(decoded.records()) {
            assert_eq!(expected.name(), actual.name());
            assert_eq!(expected.kind(), actual.kind());
            assert_eq!(expected.source().as_bytes(), actual.source().as_bytes());
        }
    }

    /// 確定形: トップレベルは `macros` 1 キー、要素は `name` → `kind` → `source` の順で、
    /// 種別は `typescript` / `javascript` のテキストになる。並びは入力順のまま。
    #[test]
    fn wire_form_is_the_documented_confirmed_shape() {
        let part = MacrosPart::new(vec![
            record("棚卸し", MacroKind::TypeScript, "const xs = 1;"),
            record("挨拶", MacroKind::JavaScript, "console.log(\"hi\")"),
        ]);
        let text = String::from_utf8(part.to_json_bytes().expect("符号化")).expect("UTF-8");

        assert_eq!(
            r#"{"macros":[{"name":"棚卸し","kind":"typescript","source":"const xs = 1;"},{"name":"挨拶","kind":"javascript","source":"console.log(\"hi\")"}]}"#,
            text,
            "確定形が変わっている"
        );
    }

    /// ソースはバイト単位で保たれる: エスケープを要する文字(引用符・逆斜体・改行・タブ・
    /// 非 ASCII・絵文字)を含むソースでも、復号すると同じバイト列へ戻る。
    #[test]
    fn source_text_survives_verbatim() {
        let source = "const s = \"a\\b\";\r\n\t// 日本語 😀\n\n";
        let part = MacrosPart::new(vec![record("記号", MacroKind::TypeScript, source)]);
        let decoded =
            MacrosPart::from_json_bytes(&part.to_json_bytes().expect("符号化")).expect("復号");

        assert_eq!(
            source.as_bytes(),
            decoded.records()[0].source().as_bytes(),
            "ソースがバイト単位で一致しない"
        );
        assert_eq!(
            source.matches('\n').count(),
            decoded.records()[0].source().matches('\n').count(),
            "改行の数が変わっている"
        );
    }

    /// 空の並びも妥当な入力である(`{"macros":[]}`)。保存経路がこの形を書くかどうかは
    /// [`crate::parts::to_parts`] が決める(本パートは表現できる)。
    #[test]
    fn an_empty_sequence_is_accepted() {
        let decoded = MacrosPart::from_json_bytes(br#"{"macros":[]}"#).expect("復号");
        assert!(decoded.records().is_empty());
        assert_eq!(
            br#"{"macros":[]}"#,
            decoded.to_json_bytes().expect("再符号化").as_slice()
        );
    }

    /// 未知フィールドはトップレベルと要素の両方で原文の位置へ差し戻される
    /// (前方互換。要件 6.2 / 6.3)。
    #[test]
    fn unknown_fields_round_trip_byte_for_byte() {
        const INPUT: &str = concat!(
            r#"{"future_top":{"unit":"mm"},"macros":["#,
            r#"{"name":"a","future_a":1,"kind":"typescript","source":"x","future_b":[2]}"#,
            r#"],"future_tail":null}"#
        );
        let decoded = MacrosPart::from_json_bytes(INPUT.as_bytes()).expect("復号");

        assert_eq!(
            INPUT.as_bytes(),
            decoded.to_json_bytes().expect("再符号化").as_slice(),
            "未知フィールドが原文の位置へ戻っていない"
        );
    }

    /// 型式の違反は `macros.json` を指す `InvalidContainer` として拒否され、部分的に
    /// 読まれた並びは返らない。
    #[test]
    fn malformed_inputs_are_rejected_with_the_entry_name() {
        let cases = [
            // トップレベルがオブジェクトでない / `macros` 欠落 / `macros` が配列でない
            r#"[]"#,
            r#"{"other":[]}"#,
            r#"{"macros":{}}"#,
            r#"{"macros":null}"#,
            // 要素がオブジェクトでない
            r#"{"macros":["a"]}"#,
            // 必須キーの欠落
            r#"{"macros":[{"kind":"typescript","source":"x"}]}"#,
            r#"{"macros":[{"name":"a","source":"x"}]}"#,
            r#"{"macros":[{"name":"a","kind":"typescript"}]}"#,
            // 型違い
            r#"{"macros":[{"name":1,"kind":"typescript","source":"x"}]}"#,
            r#"{"macros":[{"name":"a","kind":"typescript","source":null}]}"#,
            // 未知の種別(既定値へ丸めない)
            r#"{"macros":[{"name":"a","kind":"python","source":"x"}]}"#,
            r#"{"macros":[{"name":"a","kind":"TS","source":"x"}]}"#,
            // キーの重複(トップレベル・要素のいずれも)
            r#"{"macros":[],"macros":[]}"#,
            r#"{"macros":[{"name":"a","name":"b","kind":"typescript","source":"x"}]}"#,
        ];
        for input in cases {
            match MacrosPart::from_json_bytes(input.as_bytes()) {
                Err(DocumentError::InvalidContainer { entry }) => {
                    assert!(
                        entry.starts_with("macros.json: "),
                        "失敗箇所が macros.json を指していない: {entry} (入力 {input})"
                    );
                }
                other => panic!("拒否されるはずの入力が読まれた: {input} -> {other:?}"),
            }
        }
    }
}

//! シート別スキーマパート（`schemas/<sheet-ulid>.json`）の符号化・復号（タスク 4.4。
//! 要件 1.3, 2.2。design「Container Entry Layout」/「Components and Interfaces」の
//! SchemaCodec）。
//!
//! 各シートは**独立したエントリ** `schemas/<sheet-ulid>.json` を持つ（要件 2.2: 各シートの
//! データは独立して読み書きできる）。本モジュールはその 1 エントリと
//! [`SchemaPart`]（ルートスキーマとネスト型定義の不透明な保持。タスク 2.2）の相互変換だけを
//! 担う:
//!
//! | 方向 | 入力 | 出力 |
//! |------|------|------|
//! | [`SchemaCodec::encode`] | シート識別子 + `SchemaPart` | エントリ名（`schemas/<ulid>.json`）+ 確定形のバイト列 |
//! | [`SchemaCodec::decode`] | エントリ名 + エントリの実バイト列 | シート識別子 + `SchemaPart` |
//!
//! # 不透明ペイロードは 1 バイトも変えない
//!
//! ルートスキーマと各型定義本文は本スペックにとって**不透明**である（design
//! 「スキーマ・ペイロードの不透明性」。意味論は `schema-engine`）。符号化は
//! [`RawJson`] が保持する**原文のバイト列をそのまま**書き出し、
//! 再シリアライズも再整形もしない（`serde_json::Value` を経由しない）。したがって
//! ペイロード**内部**の空白・`\uXXXX` エスケープ表記・i64 を超える整数リテラル・入れ子は
//! 復号 → 符号化を往復しても 1 バイトも変わらない（テスト
//! `opaque_payloads_round_trip_byte_for_byte`）。verbatim の唯一の経路は
//! [`RawValue`] であり、[`PreservingObjectWriter`] の既知フィールドとして差し込む
//! （`manifest.json` / `document.json` が配列を差し込むのと同じ機構）。
//!
//! # JSON 形式（本クレートが所有する確定形。タスク 2.2 が確定させた形を引き継ぐ）
//!
//! コンパクトな UTF-8・末尾改行なしで、以下が確定形である（例は読みやすさのため改行と
//! 空白を入れている）:
//!
//! ```json
//! { "root": <opaque>, "types": [ { "id": "<ULID-26>", "definition": <opaque> } ] }
//! ```
//!
//! - トップレベルのキーは `root` → `types` の順に固定する（既知キーの宣言順。要件 3.3）。
//!   型定義要素のキーは `id` → `definition` の順に固定する。入力の既知キーが宣言順に
//!   並んでいなくても、出力は常に宣言順へ揃う（[`PreservedFields`](crate::json::PreservedFields)
//!   の docs にある書き出し規則と同じであり、同一内容が常に同一バイト列になる根拠）。
//! - `types` キーが欠落した入力は 0 件と解釈する（タスク 2.2 の文書化済み決定）が、
//!   **符号化は常に `types` を書く**（確定形は常に両キーを持つ）。したがって
//!   `types` を省いた入力は正準形へ揃う。
//! - `id` は [`TypeDefId`](crate::ids::TypeDefId) の `Display`（26 文字 Crockford base32
//!   大文字）として書く。タスク 2.2 の解析は大小文字を問わないため、小文字表記の入力は
//!   正準形（大文字）へ揃う（不透明ペイロードのバイトは揃わない — 変わるのは識別子の
//!   表記だけである）。
//! - `serde_json` の汎用 JSON 値型・マップ型・`HashMap` を経由しない（親モジュール
//!   [`crate::json`] の規則。キー順序の固定と決定性の根拠）。
//!
//! # 未知フィールドの保持（前方互換。要件 6.2 / 6.3）
//!
//! 復号は**エンベロープの全階層**（トップレベルと型定義要素のそれぞれ）で
//! [`PreservedFields`](crate::json::PreservedFields) が未知キーを原文の位置ごと保持し、
//! 符号化は [`PreservingObjectWriter`] がその位置へ差し戻す（トップレベルに 1 個、型定義要素
//! 1 件につき 1 個。タスク 3.2 の必須条件「1 オブジェクトにつき 1 個」）。タスク 2.2 は
//! エンベロープ・トップレベルだけを保持し、型定義要素の未知キーは拒否していたが、
//! **本タスクで保持へ一本化した**（[`crate::model::SchemaPart`] の docs
//! 「未知フィールドの保持」参照）。将来の minor が文書全体のメタデータを足しても、
//! 型定義へ省略可能フィールドを足しても、本実装を通した往復で失われない。
//!
//! 保持の**対象外**は `manifest.json` / `document.json` と同じ 2 点である:
//! (1) 未知フィールドの**キー**は JSON 文字列としてデコードしたテキストで保持するため、
//! 原文が `\uXXXX` のようなエスケープ表記を使っていればその表記は保たれない（値は
//! バイト単位で保たれる。[`PreservedField`](crate::json::PreservedField) の docs）。
//! (2) 値と区切り・キーと値の間の空白は JSON の値の一部ではないため保たれない。
//! 往復のバイト一致が成立する条件は [`PreservedFields`](crate::json::PreservedFields) の
//! docs と同じ（**コンパクトな入力 + 既知キーが宣言順**）であり、本クレート自身が書いた
//! ファイルはこの条件を満たす。
//!
//! 未知フィールドの差し戻し位置を保つため、既知キーの**重複**（同じ `"root"` が 2 回など）は
//! タスク 2.2 が既に拒否している
//! （[`PreservedFields::record_known_field`](crate::json::PreservedFields::record_known_field)
//! と [`PreservingObjectWriter::write_known`] の呼び出し件数を 1 対 1 に保つため。件数が
//! ずれると差し戻し位置がずれる）。
//!
//! # 決定性（要件 3.6）
//!
//! 符号化は入力の**純関数**である: 同じ `SchemaPart` は、構築経路や時刻によらず常に同じ
//! バイト列になる（保存時刻・実行環境・内部処理順に由来する値を一切含めない。テスト
//! `encoding_is_deterministic_and_free_of_volatile_values`）。
//!
//! # エラー対応
//!
//! 読み込みの失敗はすべて読み込み全体の中止であり、部分的な結果を返さない（要件 5.4 系）。
//! design のエラー表に本モジュール固有の変種は無いため、コンテンツ解析の失敗はコンテナ不正
//! [`DocumentError::InvalidContainer`] へ写す（[`crate::value`] / [`crate::json::determinism`] /
//! [`crate::parts::manifest`] と同じ規約）。`entry` には**対象エントリ名**
//! （`schemas/<ulid>.json`）と、[`SchemaPart::parse`] が返した失敗箇所のラベル＋理由を残す。
//!
//! | 失敗 | 返す変種と文脈 |
//! |------|----------------|
//! | エントリ名が `schemas/<ulid>.json` 形でない | [`DocumentError::InvalidContainer`]（`entry` = 与えられたエントリ名 + 理由） |
//! | バイト列が UTF-8 でない / JSON として不正 | 同上（エントリ名 + 理由） |
//! | エンベロープ構造が不正（`root` 欠落・重複、型定義要素の形、`id` が ULID でない 等） | 同上（エントリ名 + `SchemaPart::parse` の失敗箇所のラベル） |
//! | 参照構造が不正（`$ref` の値が非文字列） | 同上 |
//! | 内部の組み立てが壊れた場合（本クレートが生成した配列・ペイロードの raw 化失敗） | 同上（起こり得ない経路だが `panic` しない） |

use std::fmt;

use serde_json::value::RawValue;

use crate::entry_name::EntryName;
use crate::error::DocumentError;
use crate::ids::SheetId;
use crate::json::PreservingObjectWriter;
use crate::model::{RawJson, SchemaPart, TypeDef, KEY_DEFINITION, KEY_ID, KEY_ROOT, KEY_TYPES};

/// 符号化・復号の内部失敗に添える文脈（エントリ名を前置する前の理由）。
const CODEC_CONTEXT: &str = "schema part";

/// シート別スキーマパートの符号化・復号（design「Components and Interfaces」の
/// SchemaCodec。Service 契約）。
///
/// 状態を持たない（フィールドの無い型で、関連関数だけを持つ）。[`SchemaPart`] 自身は
/// エントリ名を知らない（model 層はコンテナのレイアウトを知らない）ため、シート識別子と
/// エントリ名の対応は本層が持つ（依存方向 `Model → Json → Parts`。`parts` モジュール docs
/// 「依存方向」）。
#[derive(Debug, Clone, Copy)]
pub struct SchemaCodec;

impl SchemaCodec {
    /// 1 シート分のスキーマを `schemas/<sheet-ulid>.json` エントリとして符号化する。
    ///
    /// エントリ名は与えられたシート識別子から作る（表示テキストは
    /// [`EntryName`] の `Display` が供給する。`schemas/` や
    /// `.json` の文字列リテラルを本モジュールの経路へ散在させない）。バイト列は確定形
    /// （モジュール docs「JSON 形式」）であり、未知フィールドを原文の位置へ差し戻す。
    ///
    /// 失敗したときは `Err` を返し、部分的なバイト列を返さない
    /// （[`PreservingObjectWriter`] の「失敗したら書き出し先へ 1 バイトも書かない」規律を
    /// 継承する）。
    pub fn encode(
        sheet: SheetId,
        part: &SchemaPart,
    ) -> Result<(EntryName, Vec<u8>), DocumentError> {
        let entry = EntryName::Schema { sheet };
        // 内部の失敗（エントリ名を持たない経路）へ、書き出し先のエントリ名を前置する。
        let bytes = envelope_json_bytes(part).map_err(|error| with_entry(&entry, error))?;
        Ok((entry, bytes))
    }

    /// `schemas/<sheet-ulid>.json` エントリの実バイト列から復号する。
    ///
    /// エントリ名から取り出したシート識別子と、[`SchemaPart::parse`] が復元したスキーマを
    /// 返す（`$ref` の抽出順・重複保持・検証の意味はタスク 2.2 のまま変えない）。
    /// エントリ名が `schemas/<ulid>.json` 形でない場合もコンテナ不正として拒否する
    /// （panic しない。モジュール docs「エラー対応」）。
    pub fn decode(entry: &EntryName, bytes: &[u8]) -> Result<(SheetId, SchemaPart), DocumentError> {
        let EntryName::Schema { sheet } = entry else {
            return Err(invalid_entry(
                entry,
                "not a sheet schema entry (expected `schemas/<sheet-ulid>.json`)",
            ));
        };
        let text = std::str::from_utf8(bytes).map_err(|error| invalid_entry(entry, error))?;
        let part = SchemaPart::parse(text).map_err(|error| with_entry(entry, error))?;
        Ok((*sheet, part))
    }
}

/// エンベロープ全体（`{"root":..,"types":[..]}`）を確定形のバイト列へ符号化する。
///
/// 既知キーは宣言順（`root` → `types`）に書き、未知フィールドは
/// [`PreservingObjectWriter`] が [`PreservedFields`](crate::json::PreservedFields) の
/// 位置へ差し戻す（モジュール docs「未知フィールドの保持」）。
fn envelope_json_bytes(part: &SchemaPart) -> Result<Vec<u8>, DocumentError> {
    let root = raw_payload(part.root())?;
    let types = raw_types(part)?;
    let mut out = Vec::new();
    {
        let mut writer = PreservingObjectWriter::new(&mut out, part.preserved_fields());
        writer.write_known(KEY_ROOT, root.as_ref())?;
        writer.write_known(KEY_TYPES, types.as_ref())?;
        writer.finish()?;
    }
    Ok(out)
}

/// 型定義の配列を決定的な JSON 配列として組み立て、raw 値へ変換する。
///
/// 型定義の並びは [`SchemaPart::type_defs`] の順序そのまま（エンベロープ内の出現順。
/// 並べ替えない）。各要素は自分の未知キーを原文の位置へ差し戻す（[`type_def_json_bytes`]）。
/// 配列全体を raw 値としてトップレベルへ差し込むのは `document.json` のシート配列と
/// 同じ機構である（[`RawValue`] が verbatim の唯一の経路）。
fn raw_types(part: &SchemaPart) -> Result<Box<RawValue>, DocumentError> {
    let mut out = Vec::new();
    out.push(b'[');
    for (index, def) in part.type_defs().iter().enumerate() {
        if index > 0 {
            out.push(b',');
        }
        out.extend_from_slice(&type_def_json_bytes(def)?);
    }
    out.push(b']');
    // 本クレートが生成した妥当な JSON 配列を 1 回検証するだけ（再解釈も再整形もしない）。
    let text = String::from_utf8(out).map_err(invalid_schema)?;
    RawValue::from_string(text).map_err(invalid_schema)
}

/// 型定義 1 件を確定形の JSON オブジェクトとして書き出す（`id` → `definition` の順）。
fn type_def_json_bytes(def: &TypeDef) -> Result<Vec<u8>, DocumentError> {
    let definition = raw_payload(def.definition())?;
    let mut out = Vec::new();
    {
        let mut writer = PreservingObjectWriter::new(&mut out, def.preserved_fields());
        writer.write_known(KEY_ID, &def.id())?;
        writer.write_known(KEY_DEFINITION, definition.as_ref())?;
        writer.finish()?;
    }
    Ok(out)
}

/// 不透明ペイロードを、書き出し先へ verbatim に差し込める raw 値へ変換する。
///
/// `RawValue` が「原文のバイト列をそのまま書き出す」唯一の経路であり、`from_string` は
/// 構文を 1 回検証するだけである（ペイロードは [`SchemaPart::parse`] が既に検証済みなので
/// 失敗し得ないが、起こり得ない経路に `panic` を置かない）。値は再シリアライズせず、
/// `serde_json::Value` を経由しない（モジュール docs「不透明ペイロード」）。
fn raw_payload(payload: &RawJson) -> Result<Box<RawValue>, DocumentError> {
    RawValue::from_string(payload.as_str().to_owned()).map_err(invalid_schema)
}

/// 符号化・復号の内部失敗をコンテナ不正へ写す（モジュール docs「エラー対応」の規約）。
///
/// `entry` はエントリ名を前置する前の理由であり、[`with_entry`] が対象エントリ名を
/// 付ける（復号の入口は [`invalid_entry`] を直接使う）。
fn invalid_schema(reason: impl fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("{CODEC_CONTEXT}: {reason}"),
    }
}

/// エントリ名を持たない内部失敗へ、対象エントリ名を前置する。
fn with_entry(entry: &EntryName, error: DocumentError) -> DocumentError {
    match error {
        DocumentError::InvalidContainer { entry: reason } => invalid_entry(entry, reason),
        other => other,
    }
}

/// エントリ名を文脈にした失敗（復号の入口と、エントリ種別の取り違え）。
fn invalid_entry(entry: &EntryName, reason: impl fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("{entry}: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 標本のシート識別子（正準 Crockford base32 大文字 26 文字）。
    const SHEET_A: &str = "01K4ANRRG004HMASW9NF6YY093";
    const SHEET_B: &str = "01K4ANRSF804HMASW9QKFG04HM";

    /// 標本の型定義識別子。
    const DEF_A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const DEF_B: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA6";
    const DEF_C: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA7";

    /// 標本ルート: 内部の空白・`\uXXXX` エスケープ表記・日本語・絵文字・i64 を超える整数
    /// リテラル・入れ子・`$ref` を含む（不透明ペイロードの verbatim 保持の検証）。
    const ROOT: &str = r#"{ "cols" : [ {"name":"金額","kind":9999999999999999999999999} ,"e🎉\n\u00e9" ] ,"z": null ,"r":{"$ref":"R1"} }"#;

    /// 標本の型定義 A: 入れ子・配列・指数表記・`$ref` を含む。
    const DEF_A_JSON: &str = r#"[ {"$ref":"R2"} ,"\u00e9",1e30,{"deep":[[null]],"empty":{}} ]"#;

    /// 標本の型定義 B: オブジェクトと非 ASCII を含む。
    const DEF_B_JSON: &str = r#"{"fields":{"a":{"kind":"text"}},"emoji":"📊","n":-0.0}"#;

    /// 標本のシート識別子。
    fn sheet(text: &str) -> SheetId {
        text.parse().expect("標本は正準 ULID")
    }

    /// 標本のエントリ名（**テキストから解析する**。実装の変種構成に依存しない）。
    fn entry_for(sheet_text: &str) -> EntryName {
        EntryName::parse(&format!("schemas/{sheet_text}.json")).expect("標本は許可リスト内")
    }

    /// バイト列をテキストとして読む（出力は UTF-8 の JSON）。
    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).expect("出力は UTF-8 の JSON")
    }

    /// 型定義要素 1 件を確定形（`id` → `definition`）で組み立てる。
    fn element(id: &str, definition: &str) -> String {
        format!(r#"{{"id":"{id}","definition":{definition}}}"#)
    }

    /// 型定義 `defs` 件のエンベロープを確定形で組み立てる（未知フィールド無し）。
    fn envelope(root: &str, defs: &[(&str, &str)]) -> String {
        let elements: Vec<String> = defs
            .iter()
            .map(|(id, definition)| element(id, definition))
            .collect();
        format!(r#"{{"root":{root},"types":[{}]}}"#, elements.join(","))
    }

    /// 標本のスキーマ（ルート + 型定義 2 件）を解析する。
    fn sample_part() -> SchemaPart {
        SchemaPart::parse(&envelope(ROOT, &[(DEF_A, DEF_A_JSON), (DEF_B, DEF_B_JSON)]))
            .expect("標本は妥当なエンベロープ")
    }

    /// エンベロープ 1 本を往復させ、入力と 1 バイトも変わらないことを確かめて復号結果を返す。
    ///
    /// `decode` → `encode` は**入力バイト列をそのまま再構成**しなければならない
    /// （コンパクト入力 + 既知キーが宣言順の条件。モジュール docs「未知フィールドの保持」）。
    fn assert_round_trip(sheet_text: &str, input: &str) -> SchemaPart {
        let sheet_id = sheet(sheet_text);
        let part = SchemaPart::parse(input).expect("標本は妥当なエンベロープ");
        let (encoded_entry, bytes) = SchemaCodec::encode(sheet_id, &part).expect("符号化");
        assert_eq!(
            entry_for(sheet_text),
            encoded_entry,
            "エントリ名がシート識別子と対応していない"
        );
        assert_eq!(input, text(&bytes), "符号化の結果が入力と一致しない");

        let (decoded_sheet, decoded) = SchemaCodec::decode(&encoded_entry, &bytes).expect("復号");
        assert_eq!(sheet_id, decoded_sheet, "復号でシート識別子が変わった");
        let (re_entry, re_bytes) = SchemaCodec::encode(decoded_sheet, &decoded).expect("再符号化");
        assert_eq!(encoded_entry, re_entry, "再符号化でエントリ名が変わった");
        assert_eq!(bytes, re_bytes, "復号 → 再符号化でバイト列が変わった");
        decoded
    }

    /// 参照ターゲットを生テキストの列として取り出す。
    fn targets(part: &SchemaPart) -> Vec<String> {
        part.type_ref_targets().to_vec()
    }

    /// 型定義識別子をテキストの列として取り出す。
    fn type_def_ids(part: &SchemaPart) -> Vec<String> {
        part.type_def_ids()
            .iter()
            .map(|id| id.to_string())
            .collect()
    }

    // --- 不透明ペイロードの完全な往復（要件 1.3, 2.2） -----------------------

    /// 不透明ペイロード（ルートと各型定義本文）はエンコード → デコード → エンコードで
    /// **1 バイトも変わらない**。
    ///
    /// 内部の空白・`\uXXXX` 表記・日本語・絵文字・i64 を超える整数リテラル・入れ子を含む
    /// 標本で確かめる（再シリアライズしていれば漂移する）。
    #[test]
    fn opaque_payloads_round_trip_byte_for_byte() {
        let input = envelope(ROOT, &[(DEF_A, DEF_A_JSON), (DEF_B, DEF_B_JSON)]);
        let part = assert_round_trip(SHEET_A, &input);

        assert_eq!(2, part.type_defs().len(), "型定義が復号されていない");
        assert_eq!(ROOT, part.root().as_str(), "ルートのバイト列が変わった");
        assert_eq!(DEF_A_JSON, part.type_defs()[0].definition().as_str());
        assert_eq!(DEF_B_JSON, part.type_defs()[1].definition().as_str());
        // 参照抽出（タスク 2.2 の意味のまま: root 先 → 型定義保持順、ペイロード内は
        // ドキュメント順）。
        assert_eq!(vec!["R1".to_owned(), "R2".to_owned()], targets(&part));
    }

    /// 確定形: キー名・キー順序・識別子のテキスト表記を文字列リテラルで固定する
    /// （モジュール docs「JSON 形式」。タスク 5.2 / 8.2 のゴールデンの基準）。
    #[test]
    fn wire_form_is_the_documented_confirmed_shape() {
        let part =
            SchemaPart::parse(&envelope(r#"{"a":1}"#, &[(DEF_A, "null")])).expect("標本は妥当");
        let (entry_name, bytes) = SchemaCodec::encode(sheet(SHEET_A), &part).expect("符号化");
        assert_eq!(
            "schemas/01K4ANRRG004HMASW9NF6YY093.json",
            entry_name.to_string()
        );
        assert_eq!(
            r#"{"root":{"a":1},"types":[{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","definition":null}]}"#,
            text(&bytes),
            "確定形が変わっている"
        );

        // 空のスキーマも同じ確定形になる（`types` は常に書く）。
        let (_, empty_bytes) =
            SchemaCodec::encode(sheet(SHEET_B), &SchemaPart::empty()).expect("符号化");
        assert_eq!(r#"{"root":null,"types":[]}"#, text(&empty_bytes));

        // 入力の既知キーが宣言順でなくても、出力は常に宣言順へ揃う（`types` → `root` の
        // 入力でも `root` → `types` で書く。モジュール docs「JSON 形式」）。
        let reordered = SchemaPart::parse(
            r#"{"types":[{"definition":1,"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}],"root":{"a":1}}"#,
        )
        .expect("標本は妥当");
        let (_, normalized) = SchemaCodec::encode(sheet(SHEET_A), &reordered).expect("符号化");
        assert_eq!(
            r#"{"root":{"a":1},"types":[{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","definition":1}]}"#,
            text(&normalized)
        );
    }

    /// `types` キーの欠落は 0 件と解釈し（タスク 2.2 の文書化済み決定）、確定形では常に
    /// `types` を書く。
    #[test]
    fn a_missing_types_key_means_zero_definitions_and_normalizes_to_an_empty_array() {
        let bytes = br#"{"root":{"a":1}}"#;
        let (_, part) = SchemaCodec::decode(&entry_for(SHEET_A), bytes).expect("復号");
        assert!(
            part.type_defs().is_empty(),
            "`types` 欠落が 0 件と解釈されていない"
        );
        let (_, written) = SchemaCodec::encode(sheet(SHEET_A), &part).expect("符号化");
        assert_eq!(r#"{"root":{"a":1},"types":[]}"#, text(&written));
    }

    // --- 未知フィールドの保持: エンベロープ全階層（要件 6.2, 6.3） -----------

    /// トップレベルの未知キーが `root` の前・`root` と `types` の間・`types` の後ろの
    /// どこにあっても、読み → 書き戻しが**バイト単位**に元へ戻る。
    #[test]
    fn unknown_top_level_fields_round_trip_byte_for_byte() {
        let elements = format!(
            "{},{}",
            element(DEF_A, DEF_A_JSON),
            element(DEF_B, DEF_B_JSON)
        );
        let cases = [
            // 前（既知フィールドの手前）
            format!(r#"{{"future":{{"rev":2}},"root":{ROOT},"types":[{elements}]}}"#),
            // 間（root と types のあいだ）
            format!(r#"{{"root":{ROOT},"git":{{"filter":"jxcel"}},"types":[{elements}]}}"#),
            // 後ろ（既知フィールドの後ろ）
            format!(r#"{{"root":{ROOT},"types":[{elements}],"created_with":"jxcel 1.1"}}"#),
        ];

        for input in cases {
            let part = assert_round_trip(SHEET_A, &input);
            assert_eq!(
                2,
                part.type_defs().len(),
                "未知キーが既知フィールドを隠している"
            );
            assert_eq!(
                ROOT,
                part.root().as_str(),
                "未知キーの保持でルートが変わった"
            );
        }
    }

    /// 型定義要素**内部**の未知キーが `id` の前・`id` と `definition` の間・`definition` の
    /// 後ろのどこにあっても、読み → 書き戻しが**バイト単位**に元へ戻る。
    ///
    /// タスク 2.2 はこの位置の未知キーを拒否していたが、要件 6.2 / 6.3（未知フィールドは
    /// 破棄せず保持し書き戻す）に忠実な側へ変更して保持する（将来の minor が型定義へ
    /// 省略可能フィールドを足しても読めなくなるため）。要素 1 件だけでは保持が壊れても
    /// 検出できないので、未知キーの位置が異なる 2 件 + 未知キーを持たない 1 件で分散させる。
    #[test]
    fn unknown_keys_inside_type_definitions_are_preserved_and_round_trip() {
        let first = format!(
            r#"{{"future_optional":"wide","id":"{DEF_A}","declared":3,"definition":{DEF_A_JSON}}}"#
        );
        let second = format!(r#"{{"id":"{DEF_B}","definition":{DEF_B_JSON},"added_in":"1.1"}}"#);
        let third = element(DEF_C, "null");
        let input = format!(r#"{{"root":{ROOT},"types":[{first},{second},{third}]}}"#);

        let part = assert_round_trip(SHEET_A, &input);

        // 未知キーは既知フィールドを隠さない（識別子も本文も正しく読める）。
        assert_eq!(
            vec![DEF_A.to_owned(), DEF_B.to_owned(), DEF_C.to_owned()],
            type_def_ids(&part)
        );
        assert_eq!(DEF_A_JSON, part.type_defs()[0].definition().as_str());
        assert_eq!(DEF_B_JSON, part.type_defs()[1].definition().as_str());
        assert_eq!("null", part.type_defs()[2].definition().as_str());
    }

    // --- 既存の意味の回帰（タスク 2.2 が確定させた検証） --------------------

    /// タスク 2.2 が確定させた検証の意味が本コーデック経由でも変わらない:
    /// `root` はちょうど 1 つ（欠落・重複を拒否）/ 型定義の形 / `id` は ULID-26 テキスト /
    /// 参照に見える文字列は参照ではない / 参照の値が非文字列なら拒否 / 異常に深いネストを拒否。
    #[test]
    fn existing_envelope_and_reference_semantics_are_unchanged() {
        let deep = format!(r#"{{"root":{}{}}}"#, "[".repeat(5_000), "]".repeat(5_000));
        let cases: Vec<(&str, String)> = vec![
            ("root 欠落", r#"{"types":[]}"#.to_owned()),
            ("root 重複", r#"{"root":1,"root":2,"types":[]}"#.to_owned()),
            (
                "types 重複",
                r#"{"root":{},"types":[],"types":[]}"#.to_owned(),
            ),
            ("types が配列でない", r#"{"root":{},"types":{}}"#.to_owned()),
            (
                "types の要素がオブジェクトでない",
                r#"{"root":{},"types":[1]}"#.to_owned(),
            ),
            (
                "型定義に id が無い",
                format!(r#"{{"root":0,"types":[{}]}}"#, element("", "{}")),
            ),
            (
                "型定義に definition が無い",
                format!(r#"{{"root":0,"types":[{{"id":"{DEF_A}"}}]}}"#),
            ),
            (
                "id が ULID でない",
                format!(r#"{{"root":0,"types":[{}]}}"#, element("not-a-ulid", "{}")),
            ),
            (
                "id が重複",
                format!(
                    r#"{{"root":0,"types":[{{"id":"{DEF_A}","id":"{DEF_B}","definition":{{}}}}]}}"#
                ),
            ),
            (
                "参照の値が非文字列",
                r#"{"root":{"a":{"$ref":123}}}"#.to_owned(),
            ),
            ("参照の値が null", r#"{"root":{"$ref":null}}"#.to_owned()),
            ("JSON として不正", r#"{"root":}"#.to_owned()),
            ("末尾にゴミ", r#"{"root":{}}x"#.to_owned()),
            ("異常に深いネスト", deep),
        ];

        for (why, input) in cases {
            let label = match SchemaCodec::decode(&entry_for(SHEET_A), input.as_bytes()) {
                Err(DocumentError::InvalidContainer { entry }) => entry,
                other => panic!("{why}: InvalidContainer 以外が返った: {other:?}"),
            };
            assert!(
                label.starts_with(&format!("{}: ", entry_for(SHEET_A))),
                "{why}: entry がエントリ名で始まらない: {label}"
            );
            assert!(
                label.len() > entry_for(SHEET_A).to_string().len() + 2,
                "{why}: entry に理由が無い: {label}"
            );
        }

        // 失敗箇所のラベルはタスク 2.2 のまま（root の失敗は root、型定義の失敗は id）。
        let root_label = invalid_label(r#"{"root":{"$ref":1}}"#);
        assert!(
            root_label.contains("root"),
            "root ラベルが無い: {root_label}"
        );
        assert!(
            root_label.contains("column"),
            "位置情報が無い: {root_label}"
        );
        let type_label = invalid_label(&format!(
            r#"{{"root":{{}},"types":[{{"id":"{DEF_A}","definition":{{"$ref":[]}}}}]}}"#
        ));
        assert!(
            type_label.contains(DEF_A),
            "型定義 id が診断に無い: {type_label}"
        );
    }

    /// 参照に見える文字列は参照ではない（意味論ブラインド）。`$ref` キーを持つオブジェクト
    /// だけが参照であり、抽出順は root 先 → 型定義保持順、重複は保持する。
    #[test]
    fn reference_extraction_is_unchanged_through_the_codec() {
        let input = format!(
            r#"{{"root":{{"note":"{{\"$ref\":\"x\"}}","also":"$ref","$refx":"{DEF_A}","$REF":"x","nested":{{"$ref":"real"}},"list":[{{"$ref":"real"}}]}},"types":[{{"id":"{DEF_A}","definition":{{"$ref":"in-type"}}}}]}}"#
        );
        let (_, part) = SchemaCodec::decode(&entry_for(SHEET_A), input.as_bytes()).expect("復号");
        assert_eq!(
            vec!["real".to_owned(), "real".to_owned(), "in-type".to_owned()],
            targets(&part),
            "抽出順・重複保持・非参照文字列の扱いが変わった"
        );
    }

    // --- 決定性（要件 3.6） --------------------------------------------------

    /// 同一の `SchemaPart` から常に同一バイト列になる。別々に構築した同じ内容のスキーマも
    /// 同じバイト列になり、復号 → 再符号化でも変わらない（揮発値を含まない）。
    #[test]
    fn encoding_is_deterministic_and_free_of_volatile_values() {
        let input = format!(
            r#"{{"future":1,"root":{ROOT},"types":[{{"extra":2,"id":"{DEF_A}","definition":{DEF_A_JSON}}}]}}"#
        );
        let part = SchemaPart::parse(&input).expect("標本は妥当");
        let (entry, first) = SchemaCodec::encode(sheet(SHEET_A), &part).expect("符号化");
        let (_, second) = SchemaCodec::encode(sheet(SHEET_A), &part).expect("符号化");
        assert_eq!(entry, entry_for(SHEET_A));
        assert_eq!(
            first, second,
            "同一の SchemaPart の 2 回の符号化がバイト一致しない"
        );

        // 別経路で組み立てた同じ内容（解析 → 復号 → 再符号化）も同じバイト列になる。
        let (_, decoded) = SchemaCodec::decode(&entry, &first).expect("復号");
        let (_, third) = SchemaCodec::encode(sheet(SHEET_A), &decoded).expect("再符号化");
        assert_eq!(first, third);

        // 同じ入力から独立に解析した 2 つも同じバイト列（順序が環境依存なら崩れる）。
        let other = SchemaPart::parse(&input).expect("標本は妥当");
        let (_, fourth) = SchemaCodec::encode(sheet(SHEET_A), &other).expect("符号化");
        assert_eq!(first, fourth);
        assert_eq!(
            input,
            text(&first),
            "未知フィールドを含む入力が往復で変わった"
        );
    }

    // --- シートごとに独立したエントリ（要件 2.2） ---------------------------

    /// 異なるシート識別子は異なるエントリ名になり、復号で正しいシート識別子が戻る
    /// （別シートのスキーマを取り違えない）。
    #[test]
    fn sheets_get_independent_entries_and_recover_their_identifiers() {
        let sheet_a = sheet(SHEET_A);
        let sheet_b = sheet(SHEET_B);
        assert_ne!(
            sheet_a, sheet_b,
            "標本のシート識別子が同一で、検証にならない"
        );

        let part = sample_part();
        let (entry_a, bytes_a) = SchemaCodec::encode(sheet_a, &part).expect("符号化");
        let (entry_b, bytes_b) = SchemaCodec::encode(sheet_b, &part).expect("符号化");
        assert_ne!(entry_a, entry_b, "異なるシートが同じエントリ名になった");
        assert_eq!(
            "schemas/01K4ANRRG004HMASW9NF6YY093.json",
            entry_a.to_string()
        );
        assert_eq!(
            "schemas/01K4ANRSF804HMASW9QKFG04HM.json",
            entry_b.to_string()
        );
        assert_eq!(bytes_a, bytes_b, "同じスキーマのバイト列がシートで変わった");

        for (entry_name, expected) in [(&entry_a, sheet_a), (&entry_b, sheet_b)] {
            let (sheet_id, decoded) = SchemaCodec::decode(entry_name, &bytes_a).expect("復号");
            assert_eq!(expected, sheet_id, "復号で別のシート識別子が戻った");
            assert_eq!(ROOT, decoded.root().as_str());
        }

        // 別シートのエントリ名で復号しても、そのエントリ名のシートが戻る（名前と識別子は
        // 常に 1 対 1）。
        let (sheet_id, _) = SchemaCodec::decode(&entry_b, &bytes_a).expect("復号");
        assert_eq!(sheet_b, sheet_id);
    }

    /// エントリ名が `schemas/<ulid>.json` 形でなければ復号しない（別のパートを
    /// スキーマとして読む事故を型で防ぐ）。
    #[test]
    fn decoding_a_non_schema_entry_is_rejected() {
        let wrong_entries = [
            EntryName::parse("manifest.json").expect("標本は許可リスト内"),
            EntryName::parse(&format!("sheets/{SHEET_A}.jsonl")).expect("標本は許可リスト内"),
            EntryName::parse("jxcel").expect("標本は許可リスト内"),
        ];
        for wrong in wrong_entries {
            let label = match SchemaCodec::decode(&wrong, br#"{"root":{}}"#) {
                Err(DocumentError::InvalidContainer { entry }) => entry,
                other => panic!("{wrong}: InvalidContainer 以外が返った: {other:?}"),
            };
            assert!(
                label.starts_with(&format!("{wrong}: ")),
                "{wrong}: entry がエントリ名で始まらない: {label}"
            );
        }

        // UTF-8 でないバイト列もコンテナ不正（panic しない）。
        let label = invalid_label_bytes(b"\xff");
        assert!(
            label.starts_with(&format!("{}: ", entry_for(SHEET_A))),
            "entry がエントリ名で始まらない: {label}"
        );
    }

    /// 復号失敗の `entry` を取り出す（不正入力を `InvalidContainer` に限る）。
    fn invalid_label(input: &str) -> String {
        invalid_label_bytes(input.as_bytes())
    }

    /// 復号失敗の `entry` を取り出す（バイト列入力）。
    fn invalid_label_bytes(input: &[u8]) -> String {
        match SchemaCodec::decode(&entry_for(SHEET_A), input) {
            Err(DocumentError::InvalidContainer { entry }) => entry,
            other => panic!("InvalidContainer を期待: {other:?}"),
        }
    }
}

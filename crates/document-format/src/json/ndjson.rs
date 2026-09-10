//! NDJSON の符号化（タスク 3.3。要件 3.4, 3.5）。
//!
//! 行データエントリ（`sheets/<sheet-id>.jsonl`）のような**行指向のエントリ**を、
//! 1 レコード = 1 テキスト行の NDJSON（JSON Lines）として読み書きする。本モジュールは
//! **json 層の汎用コーデック**であり、扱うのは「レコードの列」と「テキスト」だけである。
//! 行データ特有の知識（シート、列順、行 ID、スキーマ解釈）を持ち込まないため、
//! シートと結線する `RowsCodec`（タスク 4.5）は本モジュールの上に載る。
//!
//! # 書き出しの規則
//!
//! 1. 1 レコード = 出力の 1 テキスト行。**行末は常に `\n`（LF）**に固定し、OS の
//!    改行規約に依存しない（`\r` を出力せず、BOM も付けない）。**最後の行も `\n` で
//!    終端する**（レコード数 = 出力に含まれる `\n` の個数）。0 レコードの出力は
//!    0 バイトである。
//! 2. レコードの直列化は 3.1 の経路（`serde_json::to_writer` 直行、汎用 JSON 値型を
//!    経由しない）である。キー順はレコード型のフィールド宣言順に従い、セル値は
//!    [`crate::value`] の wire 表現で書かれる（いずれも 3.1 の規則）。
//! 3. レコード内の値に改行が含まれても、JSON の文字列エスケープにより 1 レコードが
//!    複数行に割れることはない（不正な JSON を書かない）。U+2028 / U+2029 は生の
//!    UTF-8 で書かれるが、`\n` ではないため行は割れない。
//! 4. **順序を一切並べ替えない**: 入力のレコード列の順序がそのまま行順になる。
//! 5. 途中のレコードで失敗したときは、書き出し先へ **1 バイトも書かない**
//!    （一時バッファへ組み立て、成功時に一括で書き出す。3.1 / 3.2 と同じ規律）。
//!
//! # 読み込みの規則
//!
//! 1. 入力を行に分割し、1 行 1 レコードとして復元する。**末尾の `\n` は空レコードを
//!    生まない**（`"a\nb\n"` は 2 レコード、`"a\nb"` も 2 レコード）。空入力は
//!    0 レコードであり、エラーではない。
//! 2. 行の分割は `\n` のみで行い、**`\r` の除去・正規化はしない**。行内に残った `\r`
//!    は JSON の空白であるため、`\r\n` の入力はそのまま読める（本クレートの書き出しは
//!    `\n` のみであり、これは改行を変換した外部ツールの入力への寛容さである）。
//!    ただし**文字列リテラルの中の生の `\r`** は JSON が禁じる制御文字であり、
//!    JSON として不正な行として拒否される（テスト
//!    `carriage_returns_are_not_normalized` がこの決定を固定する）。
//! 3. **空行・空白のみの行はエラー**、**JSON として不正な行もエラー**。レコード型の
//!    形に合わない行（配列、フィールド欠落など）も同じ経路でエラーになる。いずれも
//!    [`DocumentError::InvalidContainer`] へ写し、`entry` に
//!    `<location> line <1 始まりの行番号>: <理由>` を載せる（「失敗箇所のラベル +
//!    理由」という既存規約の踏襲）。理由は `serde_json` のメッセージをそのまま使う
//!    ため、その中の位置は**行内の相対位置**である。絶対行は本モジュールが前置きする
//!    行番号である。
//! 4. 数値の解釈はレコード型の `Deserialize` が決める（本モジュールは JSON テキストの
//!    行分割と 1 行 = 1 レコードの復元だけを担い、数値リテラルの範囲検査をしない）。
//!
//! # 行順序の契約（タスク 4.5 への申し送り）
//!
//! design「RowsCodec」は「行の出力順はシートが保持する行順序に従う。並び替えは行 ID を
//! 変えないため、並び替えの差分は行の移動として現れる」と定める。しかし本モジュールは
//! `Sheet` を参照できない（design「Architecture Integration」の一方向依存
//! `Model → Json` により、`json` 層は `model` / `parts` / `container` へ依存しない）。
//! 本モジュールが保証するのは「**与えられたレコード列の順序がそのまま行順になる**
//! （コーデックが ULID 昇順・辞書順・追加順などの順序を一切課さない）」までである。
//! タスク 4.5 は、シートの行順序（並び替え後の順序を含む）から作ったレコード列を
//! そのまま渡すことで `sheets/<sheet-id>.jsonl` の行順がシートの行順と一致すること、
//! および行の並び替えが行 ID を変えずにテキスト行の移動として現れることを示す必要が
//! ある（本モジュール側の証拠は、逆順とシャッフル順の 2 通りで入力順が保存される
//! ことである）。
//!
//! # 使用例
//!
//! ```
//! use document_format::json::{read_ndjson, write_ndjson};
//! use document_format::CellValue;
//! use serde::{Deserialize, Serialize};
//!
//! /// 行を模したレコード（フィールド宣言順 = スキーマが決める列順）。
//! #[derive(Debug, PartialEq, Serialize, Deserialize)]
//! struct Row {
//!     id: String,
//!     label: CellValue,
//! }
//!
//! let rows = vec![
//!     Row { id: "01".into(), label: CellValue::Text("a\nb".into()) },
//!     Row { id: "02".into(), label: CellValue::Null },
//! ];
//!
//! // 書き出し: 1 レコード = 1 行、行末は `\n` に固定（最後の行も終端する）。
//! let mut out = Vec::new();
//! write_ndjson(&mut out, "sheets/01J2X3Z000000000000000000.jsonl", &rows).expect("write");
//! assert_eq!(
//!     "{\"id\":\"01\",\"label\":\"a\\nb\"}\n{\"id\":\"02\",\"label\":null}\n",
//!     String::from_utf8(out.clone()).expect("utf-8"),
//! );
//!
//! // 読み込み: 与えた順序のまま復元される。
//! let decoded: Vec<Row> =
//!     read_ndjson(&out, "sheets/01J2X3Z000000000000000000.jsonl").expect("read");
//! assert_eq!(rows, decoded);
//! ```
//!
//! # 依存方向
//!
//! 本モジュールは [`crate::error`] と [`crate::json::determinism`]（3.1 の入出力
//! エラーの写像）にのみ依存し、`model` / `parts` / `container` に依存しない。

use core::fmt;
use std::io::Write;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::DocumentError;
use crate::json::determinism;

/// 行末に固定する改行（LF。OS の改行規約に依存しない）。
const LINE_TERMINATOR: u8 = b'\n';

/// 空行（空白のみの行）を拒否する理由（`entry` の理由部分に載る）。
const BLANK_LINE_REASON: &str = "blank line is not a record";

/// レコード列を NDJSON（1 レコード = 1 テキスト行、行末は `\n`）として書き出す。
///
/// `records` の順序がそのまま行順になる（**並べ替えを一切しない**）。各行の直列化は
/// 3.1 の書き出し口 [`crate::json::determinism::write_json`] と同じ経路（`serde_json::to_writer`
/// 直行、汎用 JSON 値型を経由しない）で行い、行末に `\n` を 1 バイト付ける（最後の行も
/// 終端する。0 レコードなら 0 バイトを書く）。
///
/// 出力は UTF-8 のコンパクトな JSON であり、`\r` も BOM も含まない。レコード内の値に
/// 改行が含まれても JSON の文字列エスケープとして書かれるため、1 レコードが複数行に
/// 割れることはない（規則の全体はモジュール docs）。
///
/// 直列化に失敗したときは [`DocumentError::InvalidContainer`] を返し、書き出し先へ
/// **1 バイトも書かない**。`location` は失敗の診断に載せるエントリの文脈（例
/// `sheets/<sheet-id>.jsonl`）であり、`entry` は
/// `<location> line <1 始まりの行番号>: <理由>` となって原因のレコードが特定できる。
///
/// **生の `f64` をフィールドに持つレコード型は保護されない**（`serde_json` が非有限値を
/// `null` として書いてしまう。3.1 と同じ制約であり、数値は [`crate::value::CellValue`]
/// として運ぶこと）。
pub fn write_ndjson<W: Write, T: Serialize>(
    out: &mut W,
    location: &str,
    records: &[T],
) -> Result<(), DocumentError> {
    // 途中のレコードで失敗しても部分的なテキストを残さないため、一時バッファへ
    // 組み立ててから一括で書き出す（モジュール docs の規則 5）。
    let mut text = Vec::new();
    for (index, record) in records.iter().enumerate() {
        serde_json::to_writer(&mut text, record)
            .map_err(|err| line_error(location, index + 1, &err))?;
        text.push(LINE_TERMINATOR);
    }
    out.write_all(&text).map_err(determinism::io_error)
}

/// NDJSON のテキスト（エントリの生バイト列）をレコード列へ復元する。
///
/// 行の分割は `\n` のみで行い、`\r` の除去・正規化はしない（規則の全体はモジュール
/// docs）。**末尾の `\n` は終端であり空レコードを生まない**（`"a\nb\n"` も `"a\nb"` も
/// 2 レコード）。空入力は 0 レコードを返し、エラーではない。順序は入力の行順のまま
/// である（並べ替えない）。
///
/// 空行・空白のみの行、JSON として不正な行、`T` の形に合わない行（配列、フィールド
/// 欠落など）は [`DocumentError::InvalidContainer`] になる。`location` は呼び出し元が
/// 与えるエントリの文脈（例 `sheets/<sheet-id>.jsonl`）であり、`entry` は
/// `<location> line <1 始まりの行番号>: <理由>` である。理由は `serde_json` の
/// メッセージであり、その中の位置は**行内の相対位置**である（絶対行は前置きした行番号）。
///
/// 数値の解釈は `T` の `Deserialize` が決める（本モジュールは数値リテラルの範囲検査を
/// しない。モジュール docs の規則 4）。
pub fn read_ndjson<T: DeserializeOwned>(
    input: &[u8],
    location: &str,
) -> Result<Vec<T>, DocumentError> {
    // 空入力は 0 レコード（エラーではない）。
    if input.is_empty() {
        return Ok(Vec::new());
    }
    // 末尾の `\n` は終端であり、空レコードを生まない（`"a\nb\n"` も `"a\nb"` も 2 レコード）。
    let body = input.strip_suffix(&[LINE_TERMINATOR]).unwrap_or(input);
    let mut records = Vec::new();
    for (index, line) in body.split(|&byte| byte == LINE_TERMINATOR).enumerate() {
        // 行番号は 1 始まり（書き出し側の診断と揃える）。
        let line_number = index + 1;
        if is_blank(line) {
            return Err(line_error(location, line_number, &BLANK_LINE_REASON));
        }
        let record: T = serde_json::from_slice(line)
            .map_err(|err| line_error(location, line_number, &err))?;
        records.push(record);
    }
    Ok(records)
}

/// 行が JSON の空白だけから成るか（空行を含む。空のスライスも真）。
///
/// 行は `\n` で分割済みなので LF は現れないが、判定は JSON の空白の定義
/// （space / tab / LF / CR）をそのまま写す。
fn is_blank(line: &[u8]) -> bool {
    line.iter().all(|&byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
}

/// 行の失敗を [`DocumentError::InvalidContainer`] へ写す。
///
/// `entry` は `<location> line <1 始まりの行番号>: <理由>`。型付き変種を持たない
/// コンテンツ解析失敗を「失敗箇所のラベル + 理由」の文字列として載せる規約は
/// [`crate::value`] / [`crate::model::SchemaPart`] と同じである。
fn line_error(location: &str, line_number: usize, reason: &dyn fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("{location} line {line_number}: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{CellValue, NestedValue};
    use crate::AttachmentId;
    use serde::{Deserialize, Serialize};

    /// 呼び出し元が与えるエントリの文脈（`sheets/<sheet-id>.jsonl` を模す）。
    const LOC: &str = "sheets/01J2X3Z000000000000000000.jsonl";

    /// 行を模したレコード（フィールド宣言順 = スキーマが決める列順）。
    ///
    /// 本モジュールはこの型の中身を知らない（汎用コーデックである）ため、
    /// テスト側で「行らしい型」を用意して振る舞いだけを確かめる。
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct ProbeRecord {
        id: String,
        label: CellValue,
        amount: CellValue,
    }

    /// セルが `Text` / `Int` の単純なレコード。
    fn record(id: &str, label: &str, amount: i64) -> ProbeRecord {
        ProbeRecord {
            id: id.into(),
            label: CellValue::Text(label.into()),
            amount: CellValue::Int(amount),
        }
    }

    /// レコード列を [`write_ndjson`] で符号化する。
    fn encode(records: &[ProbeRecord]) -> Vec<u8> {
        let mut out = Vec::new();
        write_ndjson(&mut out, LOC, records).expect("妥当なレコード列の書き出しが失敗した");
        out
    }

    /// レコード 1 件だけを符号化した 1 行（末尾の `\n` を落とす）。
    fn encoded_line(record: &ProbeRecord) -> String {
        let mut line = text(&encode(std::slice::from_ref(record)));
        line.pop();
        line
    }

    /// [`read_ndjson`] で復元する。
    fn decode(bytes: &[u8]) -> Vec<ProbeRecord> {
        read_ndjson(bytes, LOC).expect("妥当な NDJSON の読み込みが失敗した")
    }

    /// 出力をテキストとして読む（出力は UTF-8 の JSON）。
    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).expect("出力は UTF-8")
    }

    /// `\n` で行に割る（末尾の終端 `\n` は空文字列の行として現れる）。
    fn lines(text: &str) -> Vec<&str> {
        text.split('\n').collect()
    }

    // --- 書き出し（不変条件 1・2: 1 行 1 オブジェクト、行末の固定） --------------

    /// 1 レコード = 1 テキスト行、全行が `\n` 終端、`\r` を 1 つも出さない
    /// （不変条件 1・2）。行数は `\n` の個数で数える。
    #[test]
    fn every_record_is_one_line_and_the_last_line_is_terminated() {
        let records = [record("01", "a", 1), record("02", "b", 2), record("03", "c", 3)];
        let bytes = encode(&records);

        // 行末は常に `\n`（最後の行も終端する）。`\r` も BOM も付かない。
        assert_eq!(
            "{\"id\":\"01\",\"label\":\"a\",\"amount\":1}\n\
             {\"id\":\"02\",\"label\":\"b\",\"amount\":2}\n\
             {\"id\":\"03\",\"label\":\"c\",\"amount\":3}\n",
            text(&bytes),
        );
        assert_eq!(
            records.len(),
            bytes.iter().filter(|&&byte| byte == b'\n').count(),
            "レコード数と `\\n` の個数が一致しない",
        );
        assert!(!bytes.contains(&b'\r'), "出力に `\\r` が含まれる");
        assert_eq!(Some(&b'{'), bytes.first(), "先頭に BOM など余分なバイトが付いている");

        // 各行はそれ自身が単一の JSON オブジェクトである（前後の行に依存しない）。
        let encoded = text(&bytes);
        assert_eq!(records.len() + 1, lines(&encoded).len());
        for line in encoded.split('\n').filter(|line| !line.is_empty()) {
            serde_json::from_str::<ProbeRecord>(line).expect("行が単一の JSON オブジェクトでない");
        }
    }

    /// 値の中の改行・`\r`・U+2028 / U+2029 は 1 行を割らない（不変条件 1・2）。
    #[test]
    fn newlines_in_values_never_split_a_record() {
        let nested = CellValue::Nested(NestedValue::Object(vec![
            ("z".into(), CellValue::Text("改行\nと\rと\u{2028}と\u{2029}".into())),
            ("a".into(), CellValue::Text("x\ry".into())),
        ]));
        let records = [
            ProbeRecord {
                id: "01".into(),
                label: CellValue::Text("a\nb\r\n c\u{2028}d".into()),
                amount: CellValue::Null,
            },
            ProbeRecord {
                id: "02".into(),
                label: nested,
                amount: CellValue::Text("0\n1".into()),
            },
        ];
        let bytes = encode(&records);
        let encoded = text(&bytes);

        assert_eq!(records.len(), bytes.iter().filter(|&&byte| byte == b'\n').count());
        assert_eq!(records.len() + 1, lines(&encoded).len());
        assert!(!bytes.contains(&b'\r'), "生の `\\r` が出力された（エスケープされていない）");
        // 改行・`\r` はエスケープとして現れ、U+2028 / U+2029 は生の UTF-8 で出る。
        assert!(
            encoded.contains("a\\nb\\r\\n c\u{2028}d"),
            "改行がエスケープされていない: {encoded}",
        );
        assert_eq!(records.to_vec(), decode(&bytes), "行が割れて復元できなかった");
    }

    // --- 順序（不変条件 3: 出力順 = 入力順） ------------------------------------

    /// 出力順 = 入力順。コーデックは独自の順序（ULID 昇順・辞書順・追加順）を課さない:
    /// 逆順とシャッフル順の 2 通りで、与えた順序がそのまま行順になる（不変条件 3）。
    #[test]
    fn the_codec_keeps_the_given_order_and_imposes_none() {
        // ID は ULID の正準形（26 文字。辞書順 = 時系列順）。
        let ascending = [
            record("01J2X3Z000000000000000001", "a", 1),
            record("01J2X3Z000000000000000002", "b", 2),
            record("01J2X3Z000000000000000003", "c", 3),
            record("01J2X3Z000000000000000004", "d", 4),
            record("01J2X3Z000000000000000005", "e", 5),
        ];
        let ascending_ids: Vec<&str> = ascending.iter().map(|row| row.id.as_str()).collect();

        for (label, records) in [
            ("逆順", ascending.iter().cloned().rev().collect::<Vec<_>>()),
            ("シャッフル順", [3, 0, 4, 1, 2].map(|index| ascending[index].clone()).to_vec()),
        ] {
            let input_ids: Vec<&str> = records.iter().map(|row| row.id.as_str()).collect();
            assert_ne!(ascending_ids, input_ids, "{label}: 入力が ULID 昇順のまま（検証が空振りする）");

            let bytes = encode(&records);
            // 行のテキストそのものが入力順であること（行ごとの符号化の連結 = 出力全体）。
            let expected: String = records
                .iter()
                .map(|row| text(&encode(std::slice::from_ref(row))))
                .collect();
            assert_eq!(expected, text(&bytes), "{label}: 行の並びが入力順と異なる");
            // 読み戻しても同じ順序であること。
            assert_eq!(records, decode(&bytes), "{label}: 読み戻しで順序が変わった");
        }
    }

    // --- 往復（不変条件 4） -----------------------------------------------------

    /// レコード列 → テキスト → レコード列が完全に一致する（不変条件 4）。
    /// 非 ASCII・入れ子（キー順が辞書順でない）・添付参照・`null`・脱出口の折り返し・
    /// 整数と浮動小数の区別を含む。
    #[test]
    fn records_round_trip_without_loss() {
        let records = vec![
            record("01", "日本語と😀", 1),
            ProbeRecord {
                id: "02".into(),
                label: CellValue::Nested(NestedValue::Object(vec![
                    // 辞書順（a, z）ではない並び。汎用 JSON 値型を経由すると崩れる。
                    ("z".into(), CellValue::Int(1)),
                    (
                        "a".into(),
                        CellValue::Nested(NestedValue::Array(vec![
                            CellValue::Null,
                            CellValue::Bool(false),
                            // 規則 1 では `Decimal` に決まるため、`Text` は脱出口で書かれる。
                            CellValue::Text("0".into()),
                            // 10 進文法の外なので `Decimal` も脱出口で書かれる。
                            CellValue::Decimal("abc".into()),
                        ])),
                    ),
                ])),
                amount: CellValue::Float(5.0),
            },
            ProbeRecord {
                id: "03".into(),
                label: CellValue::Attachment(AttachmentId::from_bytes(b"payload")),
                // `-0.0` は `0.0` として書かれる（3.1 の正規化が効いている）。
                amount: CellValue::Float(-0.0),
            },
            ProbeRecord {
                id: "04".into(),
                label: CellValue::Decimal("1.50".into()),
                amount: CellValue::Null,
            },
        ];

        let bytes = encode(&records);
        assert_eq!(records, decode(&bytes), "往復でレコードが変わった");

        // 整数 `5` と浮動小数 `5.0` は行のテキストとして区別される（要件 3.6）。
        let int_line = encoded_line(&record("05", "x", 5));
        let float_line = encoded_line(&ProbeRecord {
            id: "05".into(),
            label: CellValue::Text("x".into()),
            amount: CellValue::Float(5.0),
        });
        assert_eq!("{\"id\":\"05\",\"label\":\"x\",\"amount\":5}", int_line);
        assert_eq!("{\"id\":\"05\",\"label\":\"x\",\"amount\":5.0}", float_line);
    }

    // --- 行粒度の差分（不変条件 5） ---------------------------------------------

    /// 1 レコードのセルを変更して書き出すと、出力テキストの差分はちょうど 1 行である
    /// （不変条件 5。要件 3.5）。変更対象は先頭と末尾の 2 通りで確かめる。
    #[test]
    fn changing_one_record_changes_exactly_one_line() {
        let base: Vec<ProbeRecord> = (0..100)
            .map(|index| record(&format!("{index:04}"), &format!("value-{index}"), index))
            .collect();
        let base_text = text(&encode(&base));
        let before = lines(&base_text);

        for index in [0usize, 99] {
            let mut changed = base.clone();
            changed[index].amount = CellValue::Int(-1);
            let after_text = text(&encode(&changed));
            let after = lines(&after_text);

            assert_eq!(before.len(), after.len(), "レコード数が同じなのに行数が変わった");
            let differing: Vec<usize> =
                (0..before.len()).filter(|&i| before[i] != after[i]).collect();
            assert_eq!(vec![index], differing, "1 セルの変更が現れた行（対象 {index}）");
            assert_eq!(
                encoded_line(&changed[index]),
                after[index],
                "変更行が変更後のレコードと一致しない",
            );
        }
    }

    // --- 読み込みの境界（不変条件: 空入力・末尾終端・空行・不正行） -------------

    /// 空入力は 0 レコード（エラーではない）。0 レコードの書き出しは 0 バイトである。
    #[test]
    fn empty_input_is_zero_records() {
        let empty: Vec<ProbeRecord> = Vec::new();
        assert!(encode(&empty).is_empty(), "0 レコードで 1 バイト書き出された");
        assert_eq!(empty, decode(b""));
    }

    /// 末尾の `\n` は空レコードを生まない（`"a\nb\n"` も `"a\nb"` も 2 レコード）。
    #[test]
    fn a_trailing_newline_does_not_create_an_empty_record() {
        let terminated = b"{\"id\":\"01\",\"label\":\"a\",\"amount\":1}\n\
                           {\"id\":\"02\",\"label\":\"b\",\"amount\":2}\n";
        let unterminated = b"{\"id\":\"01\",\"label\":\"a\",\"amount\":1}\n\
                             {\"id\":\"02\",\"label\":\"b\",\"amount\":2}";

        assert_eq!(vec![record("01", "a", 1), record("02", "b", 2)], decode(terminated));
        assert_eq!(decode(terminated), decode(unterminated));
    }

    /// 空行・空白のみの行は、1 始まりの行番号と理由を載せた `InvalidContainer` になる。
    /// `\n` だけの入力は 1 行目の空行である（末尾の `\n` が落とすのは終端だけ）。
    #[test]
    fn blank_lines_are_rejected_with_their_line_number() {
        let first = "{\"id\":\"01\",\"label\":\"a\",\"amount\":1}";
        let third = "{\"id\":\"03\",\"label\":\"c\",\"amount\":3}";

        for (input, line) in [
            (format!("{first}\n\n{third}\n"), 2usize),
            (format!("{first}\n \t \n{third}"), 2),
            ("\n".to_string(), 1),
            ("  \r\n".to_string(), 1),
        ] {
            let err = read_ndjson::<ProbeRecord>(input.as_bytes(), LOC).expect_err("空行が通った");
            match err {
                DocumentError::InvalidContainer { entry } => assert_eq!(
                    format!("{LOC} line {line}: blank line is not a record"),
                    entry,
                ),
                other => panic!("{other:?} は InvalidContainer ではない"),
            }
        }
    }

    /// JSON として不正な行、レコード型の形に合わない行（配列、フィールド欠落）は、
    /// 1 始まりの行番号と理由を載せた `InvalidContainer` になる。前後の行が正しくても
    /// 部分的な結果は返さない。
    #[test]
    fn invalid_records_are_rejected_with_their_line_number() {
        let first = "{\"id\":\"01\",\"label\":\"a\",\"amount\":1}";

        for (input, line) in [
            (format!("{first}\nnot json\n"), 2usize),
            (format!("{first}\n{{\"id\":}}\n"), 2),
            (format!("{first}\n[1,2,3]\n"), 2),
            (format!("{first}\n{{}}\n"), 2),
            // 1 行目から不正な場合も行番号は 1 である。
            ("{\"id\":".to_string(), 1),
        ] {
            let err = read_ndjson::<ProbeRecord>(input.as_bytes(), LOC).expect_err("不正な行が通った");
            match err {
                DocumentError::InvalidContainer { entry } => {
                    let prefix = format!("{LOC} line {line}: ");
                    assert!(entry.starts_with(&prefix), "行番号と文脈が entry に無い: {entry}");
                    assert!(entry.len() > prefix.len(), "理由が entry に無い: {entry}");
                }
                other => panic!("{other:?} は InvalidContainer ではない"),
            }
        }
    }

    /// 改行コードの扱い（本モジュールの決定）: 行分割は `\n` のみで行い、`\r` の除去・
    /// 正規化はしない。行内に残る `\r` は JSON の空白であるため `\r\n` 入力はそのまま
    /// 読める。一方、文字列リテラルの中の生の `\r` は JSON が禁じる制御文字であり、
    /// 不正な行として拒否される。
    #[test]
    fn carriage_returns_are_not_normalized() {
        let lf = "{\"id\":\"01\",\"label\":\"a\",\"amount\":1}\n\
                  {\"id\":\"02\",\"label\":\"b\",\"amount\":2}\n";
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(decode(lf.as_bytes()), decode(crlf.as_bytes()), "CRLF 入力が読めない");

        // 文字列の中の生の `\r`（不正な制御文字）は拒否される。
        let raw_cr = "{\"id\":\"01\",\"label\":\"a\rb\",\"amount\":1}\n";
        let err = read_ndjson::<ProbeRecord>(raw_cr.as_bytes(), LOC).expect_err("生の `\\r` が通った");
        match err {
            DocumentError::InvalidContainer { entry } => {
                assert!(entry.starts_with(&format!("{LOC} line 1: ")), "{entry}");
            }
            other => panic!("{other:?} は InvalidContainer ではない"),
        }
    }

    // --- 失敗時の無出力（不変条件 6） -------------------------------------------

    /// 書き出せないレコード（非有限値を含むセル）があると、書き出し先へ 1 バイトも
    /// 書かれない（不変条件 6。3.1 / 3.2 と同じ規律）。エラーは失敗した行を指す。
    /// 失敗位置は先頭と末尾の 2 通りで確かめる。
    #[test]
    fn a_failing_record_writes_nothing_and_names_its_line() {
        let nan = CellValue::Float(f64::NAN);
        let cases = [
            (
                1usize,
                vec![
                    ProbeRecord { id: "01".into(), label: nan.clone(), amount: CellValue::Null },
                    record("02", "b", 2),
                ],
            ),
            (
                3,
                vec![
                    record("01", "a", 1),
                    record("02", "b", 2),
                    ProbeRecord {
                        id: "03".into(),
                        label: CellValue::Nested(NestedValue::Array(vec![nan.clone()])),
                        amount: CellValue::Null,
                    },
                ],
            ),
        ];

        for (line, records) in cases {
            let mut out = Vec::new();
            let err = write_ndjson(&mut out, LOC, &records).expect_err("非有限値の書き出しが成功した");
            assert!(out.is_empty(), "失敗時に部分的な出力が残った（{} バイト）", out.len());
            match err {
                DocumentError::InvalidContainer { entry } => {
                    let prefix = format!("{LOC} line {line}: ");
                    assert!(entry.starts_with(&prefix), "行番号と文脈が entry に無い: {entry}");
                    assert!(entry.contains("NonRepresentableNumber"), "理由に変種名が無い: {entry}");
                }
                other => panic!("{other:?} は InvalidContainer ではない"),
            }
        }
    }
}

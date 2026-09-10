//! JSON の書き出し口（タスク 3.1。要件 2.4, 3.3, 3.6）。
//!
//! 出力の規則は親モジュール [`crate::json`] の docs にある。本ファイルは
//! **書き出し口 3 つ**だけを持つ:
//!
//! | 書き出し口 | 対象 | キー順序 |
//! |------------|------|----------|
//! | [`write_json`] | `Serialize` を実装する型（構造体） | フィールド宣言順 |
//! | [`write_ordered_object`] | 動的な列集合（キーと [`CellValue`] の列） | 呼び出し元が与えた順序 |
//! | [`write_cell`] | セル値 1 個 | （キーを持たない） |
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

use serde::Serialize;

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
fn io_error(source: io::Error) -> DocumentError {
    DocumentError::Io {
        source,
        retried: false,
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

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
}

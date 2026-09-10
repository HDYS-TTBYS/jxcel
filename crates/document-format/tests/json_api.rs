//! クレート外から見た決定的 JSON 出力・未知フィールドの保持・NDJSON の符号化
//! （タスク 3.1 / 3.2 / 3.3。要件 2.4, 3.3, 3.4, 3.5, 3.6, 6.2, 6.3）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。
//! `document_format::json::` 直下の再エクスポートと `document_format::json::determinism::` /
//! `document_format::json::ndjson::` 経由の双方がクレート外から見えることをコンパイル時に
//! 示す（`write_json` / `write_ordered_object` / `write_cell` / `PreservedFields` /
//! `PreservingObjectWriter` / `write_ndjson` / `read_ndjson`）。
//!
//! # 列順序は呼び出し元（スキーマ側）が与える
//!
//! 本クレートはスキーマを解釈しないため、動的な列集合の順序を自分では決められない。
//! ここでは schema-engine が決める列順序を**呼び出し元の責務**として模し、
//! 「与えた順序がそのまま出る」「順序を変えれば出力も変わる」をクレート外から確かめる
//! （design「DeterministicJson」Risks の契約）。
//!
//! # 未知フィールドの保持（前方互換。要件 6.2 / 6.3）
//!
//! バージョン付きパート構造体の読み書きで、自分のバージョンが解釈しないフィールドが
//! 往復で消えないことをクレート外から確かめる（design「DeterministicJson」の
//! 「未知フィールドは破棄せず保持し、書き戻す」）。
//!
//! キー順序の決定性そのもの（構造体のフィールド宣言順、非 ASCII の UTF-8 出力、
//! `-0.0` の正規化、非有限値の拒否）と、保持の詳細（値の verbatim 性・位置の表現・
//! 失敗時の無出力）は `src/json/determinism.rs` の単体テストと doctest が網羅する。
//! NDJSON の規則（行末の固定、値の中の改行、順序の保存、行粒度の差分、空行・不正行の
//! 拒否）は `src/json/ndjson.rs` の単体テストと doctest が網羅する。ここは公開経路が
//! 機能することを示す最小の確認に留める。

use document_format::json::determinism;
use document_format::json::{
    read_ndjson, write_cell, write_json, write_ordered_object, write_ndjson, PreservedFields,
    PreservingObjectWriter,
};
use document_format::{CellValue, DocumentError};
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// 行の文脈。セル位置は `<ROW_LOCATION> column <キー>` としてエラーに載る。
const ROW_LOCATION: &str =
    "sheet 01J2X3Z0000000000000000001 row 01J2X3Z0000000000000000002";

/// 宣言順（`version`, `document_id`, `sheet_order`）が辞書順と異なる構造体。
#[derive(Serialize)]
struct DocumentPartProbe {
    version: u32,
    document_id: String,
    sheet_order: Vec<String>,
}

/// schema-engine が決める列順序を模した行のセル（並びは辞書順ではない）。
///
/// キー（列名）はスキーマ側の語彙、値は本クレートが解釈しないセル値であり、
/// この対応関係を決めるのは呼び出し元である。
fn row_cells() -> Vec<(&'static str, CellValue)> {
    vec![
        ("数量", CellValue::Decimal("12.50".into())),
        ("alpha", CellValue::Int(7)),
        ("備考", CellValue::Text("備考です".into())),
    ]
}

/// セルを書き出し口が要求する「キー + 値への参照」の列にする。
fn columns<'a>(cells: &'a [(&'static str, CellValue)]) -> Vec<(&'static str, &'a CellValue)> {
    cells.iter().map(|(key, value)| (*key, value)).collect()
}

#[test]
fn json_writers_are_usable_from_outside_the_crate() {
    // 要件 3.3: 構造体はフィールド宣言順で書かれる。
    let part = DocumentPartProbe {
        version: 1,
        document_id: "01J2X3Z0000000000000000000".into(),
        sheet_order: vec!["売上".into()],
    };
    let mut out = Vec::new();
    write_json(&mut out, &part).expect("構造体の書き出しが失敗した");
    assert_eq!(
        "{\"version\":1,\"document_id\":\"01J2X3Z0000000000000000000\",\"sheet_order\":[\"売上\"]}",
        String::from_utf8(out).expect("出力は UTF-8"),
    );
}

#[test]
fn ordered_columns_follow_the_caller_supplied_schema_order() {
    let cells = row_cells();

    let mut out = Vec::new();
    write_ordered_object(&mut out, ROW_LOCATION, &columns(&cells))
        .expect("列集合の書き出しが失敗した");
    assert_eq!(
        "{\"数量\":\"12.50\",\"alpha\":7,\"備考\":\"備考です\"}",
        String::from_utf8(out).expect("出力は UTF-8"),
    );

    // スキーマの列順序が変われば出力の順序も変わる（本クレートは並べ替えない）。
    // 列名と値は組のまま入れ替わる。
    let reversed: Vec<(&str, &CellValue)> = columns(&cells).into_iter().rev().collect();
    let mut other = Vec::new();
    write_ordered_object(&mut other, ROW_LOCATION, &reversed)
        .expect("列集合の書き出しが失敗した");
    assert_eq!(
        "{\"備考\":\"備考です\",\"alpha\":7,\"数量\":\"12.50\"}",
        String::from_utf8(other).expect("出力は UTF-8"),
    );
}

#[test]
fn non_representable_numbers_are_rejected_without_writing() {
    // 要件 3.3 系: NaN は型付きエラーで拒否し、部分的な JSON を 1 バイトも書かない。
    let ok = CellValue::Int(1);
    let bad = CellValue::Float(f64::NAN);
    let mut out = Vec::new();
    let outcome = determinism::write_ordered_object(
        &mut out,
        ROW_LOCATION,
        &[("alpha", &ok), ("備考", &bad)],
    );
    match outcome {
        Err(DocumentError::NonRepresentableNumber { location }) => {
            assert_eq!(format!("{ROW_LOCATION} column 備考"), location);
        }
        other => panic!("{other:?} は非有限値のエラーではない"),
    }
    assert!(out.is_empty(), "失敗時に部分的なバイト列が書かれた");

    // セル単位の書き出し口も同じ型付きエラーを返す。
    let mut sink = Vec::new();
    assert!(matches!(
        write_cell(&mut sink, &bad, ROW_LOCATION),
        Err(DocumentError::NonRepresentableNumber { .. })
    ));
    assert!(sink.is_empty(), "失敗時に部分的なバイト列が書かれた");
}

// ---------------------------------------------------------------------------
// 未知フィールドの保持（タスク 3.2。要件 6.2, 6.3）
// ---------------------------------------------------------------------------

/// 未知フィールドを保持するバージョン付きパートの最小模型。
///
/// 既知フィールドの宣言順は `version`, `parts`。未知フィールドが `version` の前と
/// `parts` の後にあっても、既知側が宣言順で出て未知側が原文の位置へ戻ることを
/// クレート外から確かめる（保持の詳細は `src/json/determinism.rs` の単体テスト）。
struct ManifestPartProbe {
    version: u32,
    parts: Vec<String>,
    preserved: PreservedFields,
}

impl<'de> Deserialize<'de> for ManifestPartProbe {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(ManifestPartProbeVisitor)
    }
}

/// パートの visitor: 既知キーは [`PreservedFields::record_known_field`]、未知キーは
/// [`PreservedFields::capture`]。
struct ManifestPartProbeVisitor;

impl<'de> Visitor<'de> for ManifestPartProbeVisitor {
    type Value = ManifestPartProbe;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a manifest part object with `version` and `parts`")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ManifestPartProbe, A::Error> {
        let (mut version, mut parts): (Option<u32>, Option<Vec<String>>) = (None, None);
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
            return Err(de::Error::custom("manifest part is missing the `version` key"));
        };
        Ok(ManifestPartProbe { version, parts: parts.unwrap_or_default(), preserved })
    }
}

#[test]
fn unknown_fields_survive_a_read_write_cycle_from_outside_the_crate() {
    // 要件 6.2 / 6.3: 現行の構造体が解釈しないフィールド（将来の minor が足したもの）を
    // 破棄せず、原文の位置へ差し戻して書き戻す。値は原文のバイト列のまま戻る。
    const INPUT: &str = r#"{"future_a":1,"version":4,"parts":["p"],"future_b":{"unit":"mm"}}"#;

    let part: ManifestPartProbe = serde_json::from_str(INPUT).expect("読み込みが失敗した");
    assert_eq!(2, part.preserved.len(), "未知フィールドが保持されていない");
    assert_eq!(
        vec!["future_a", "future_b"],
        part.preserved.iter().map(|field| field.key()).collect::<Vec<_>>(),
        "保持順が原文順でない",
    );

    let mut out = Vec::new();
    let mut writer = PreservingObjectWriter::new(&mut out, &part.preserved);
    writer.write_known("version", &part.version).expect("既知フィールドの書き出しが失敗した");
    writer.write_known("parts", &part.parts).expect("既知フィールドの書き出しが失敗した");
    writer.finish().expect("オブジェクトの確定が失敗した");

    assert_eq!(INPUT, String::from_utf8(out).expect("出力は UTF-8"), "往復でバイト列が変わった");
}

// ---------------------------------------------------------------------------
// NDJSON コーデック（タスク 3.3。要件 3.4, 3.5）
// ---------------------------------------------------------------------------

/// シートの行を模したレコード（フィールド宣言順 = スキーマが決める列順）。
///
/// 本クレートはこの型を知らない（コーデックはレコード列とテキストだけを扱う）。
/// 行データ特有の結線（`Sheet` の行順序）はタスク 4.5 の RowsCodec が担う。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RowProbe {
    id: String,
    label: CellValue,
    amount: CellValue,
}

/// 呼び出し元が与えるエントリの文脈（`sheets/<sheet-id>.jsonl`）。
const SHEET_ENTRY: &str = "sheets/01J2X3Z000000000000000000.jsonl";

/// クレート外から見た経路: `json::` 直下の再エクスポートと `json::ndjson::` の双方で
/// 書き出しと読み込みができ、1 レコード = 1 行・行末 `\n`・往復同一が成立する。
#[test]
fn ndjson_codec_is_usable_from_outside_the_crate() {
    let rows = vec![
        RowProbe {
            id: "01".into(),
            label: CellValue::Text("a\nb".into()),
            amount: CellValue::Int(1),
        },
        RowProbe { id: "02".into(), label: CellValue::Null, amount: CellValue::Float(-0.0) },
    ];

    let mut out = Vec::new();
    write_ndjson(&mut out, SHEET_ENTRY, &rows).expect("書き出しが失敗した");
    // 要件 3.4: 1 レコード = 1 テキスト行、行末は `\n` に固定（最後の行も終端する）。
    // 値の中の改行はエスケープされ、`-0.0` は `0.0` として書かれる（3.1 の正規化）。
    assert_eq!(
        "{\"id\":\"01\",\"label\":\"a\\nb\",\"amount\":1}\n\
         {\"id\":\"02\",\"label\":null,\"amount\":0.0}\n",
        String::from_utf8(out.clone()).expect("出力は UTF-8"),
    );
    assert_eq!(rows.len(), out.iter().filter(|&&byte| byte == b'\n').count());
    assert!(!out.contains(&b'\r'), "出力に `\\r` が含まれる");

    let decoded: Vec<RowProbe> = read_ndjson(&out, SHEET_ENTRY).expect("読み込みが失敗した");
    assert_eq!(rows, decoded, "往復でレコードが変わった");

    // `json::ndjson::` 直下経由でも同じ公開面が見える（0 レコードは 0 バイト）。
    let empty: Vec<RowProbe> = Vec::new();
    let mut second = Vec::new();
    document_format::json::ndjson::write_ndjson(&mut second, SHEET_ENTRY, &empty)
        .expect("空の書き出しが失敗した");
    assert!(second.is_empty());
    let decoded_empty: Vec<RowProbe> =
        document_format::json::ndjson::read_ndjson(&second, SHEET_ENTRY).expect("読み込みが失敗した");
    assert!(decoded_empty.is_empty());
}

/// クレート外から見た順序と行粒度: 呼び出し元が与えたレコード列の順序（シートが保持する
/// 行順序を模し、ULID 昇順でも追加順でもない）がそのまま行順になり、1 レコードのセル
/// 変更はちょうど 1 行だけを変える（要件 3.4 / 3.5）。
#[test]
fn ndjson_keeps_the_row_order_and_line_granularity_the_caller_supplies() {
    let rows = vec![
        RowProbe {
            id: "01J2X3Z000000000000000002".into(),
            label: CellValue::Text("b".into()),
            amount: CellValue::Int(2),
        },
        RowProbe {
            id: "01J2X3Z000000000000000003".into(),
            label: CellValue::Text("c".into()),
            amount: CellValue::Int(3),
        },
        RowProbe {
            id: "01J2X3Z000000000000000001".into(),
            label: CellValue::Text("a".into()),
            amount: CellValue::Int(1),
        },
    ];
    let mut out = Vec::new();
    write_ndjson(&mut out, SHEET_ENTRY, &rows).expect("書き出しが失敗した");
    let encoded = String::from_utf8(out).expect("出力は UTF-8");

    let ids: Vec<String> = encoded
        .lines()
        .map(|line| {
            serde_json::from_str::<RowProbe>(line).expect("行が単一の JSON オブジェクトでない").id
        })
        .collect();
    assert_eq!(
        vec![
            "01J2X3Z000000000000000002",
            "01J2X3Z000000000000000003",
            "01J2X3Z000000000000000001",
        ],
        ids,
        "行順が入力順と異なる（コードックが独自の順序を課している）",
    );

    // 要件 3.5: 1 セルの変更 → 変化する行はちょうど 1 行。
    let mut changed = rows.clone();
    changed[1].label = CellValue::Text("changed".into());
    let mut changed_out = Vec::new();
    write_ndjson(&mut changed_out, SHEET_ENTRY, &changed).expect("書き出しが失敗した");
    let changed_text = String::from_utf8(changed_out).expect("出力は UTF-8");

    let before: Vec<&str> = encoded.lines().collect();
    let after: Vec<&str> = changed_text.lines().collect();
    assert_eq!(before.len(), after.len(), "レコード数が同じなのに行数が変わった");
    assert_eq!(
        1,
        before.iter().zip(&after).filter(|(before, after)| before != after).count(),
        "1 セルの変更が 1 行に収まっていない",
    );
}

//! クレート外から見た行データパートの符号化・復号（タスク 4.5。要件 2.3, 3.4, 3.5）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。
//! `document_format::parts::` 直下と `document_format::parts::rows_codec::` 経由の
//! 双方から `RowsCodec` が見えることをコンパイル時に示す（パート層の公開面は
//! `parts` 配下に統一されており、クレート根には出さない — `ManifestPart` /
//! `DocumentPart` / `SchemaCodec` と同じ扱い）。
//!
//! ここで固定するのは要件の中核である: (1) 行順は与えられた順序のままで一切ソートされない
//! （要件 1.5 / 3.5）、(2) 1 行変更がちょうど 1 テキスト行の差分になる（要件 3.5）、
//! (3) シートごとに独立したエントリになる（要件 2.3）、(4) 行オブジェクトの wire 形式
//! （`$id` 予約キー・列名の `$` エスケープ・行間のキー列一致）、(5) i64 範囲外整数の門。
//!
//! 10 万行の往復（要件 3.4）は `src/parts/rows_codec.rs` の単体テストが担う。10 万行の
//! モデルを公開経路 `Document::set_row_values` で組み立てると、対象行の線形探索により
//! O(n²)（実測 36 秒）になるためである。本ファイルの各テストは `Document` の公開経路で
//! モデルを組み立て、公開面だけが機能することを示す。

use document_format::parts::{rows_codec, RowsCodec, RowsEncodeError, SheetRows};
use document_format::{
    to_json_bytes, AttachmentId, CellValue, Document, DocumentError, EntryName, NestedValue, RowId,
    SheetId,
};

/// 標本のシート識別子（正準 Crockford base32 大文字 26 文字）。
const SHEET_A: &str = "01K4ANRRG004HMASW9NF6YY093";
const SHEET_B: &str = "01K4ANRSF804HMASW9QKFG04HM";

/// 標本の行識別子（同上）。手書きの NDJSON 入力に使う。
const ROW_1: &str = "01K4ANRRG004HMASW9NF6YY091";
const ROW_2: &str = "01K4ANRRG004HMASW9NF6YY092";
const ROW_3: &str = "01K4ANRRG004HMASW9NF6YY094";

/// 標本シートの行データエントリ名。
fn rows_entry(sheet: SheetId) -> EntryName {
    EntryName::parse(&format!("sheets/{sheet}.jsonl")).expect("許可リスト内")
}

/// 出力の行順を、各行頭の `$id` から独立に取り出す（復号を経由しない観測）。
///
/// 行末は常に LF で最終行も終端されるため、末尾の `\n` を 1 つ剥がしてから分割する
/// （二重改行があれば空行として現れ、この関数が失敗する）。
fn line_ids(bytes: &[u8]) -> Vec<String> {
    lines(bytes)
        .into_iter()
        .map(|line| {
            let rest = line
                .strip_prefix(b"{\"$id\":\"")
                .expect("`$id` が先頭キーでない");
            let end = rest
                .iter()
                .position(|&byte| byte == b'"')
                .expect("識別子の終端が無い");
            String::from_utf8(rest[..end].to_vec()).expect("識別子は UTF-8")
        })
        .collect()
}

/// `\n` で分割した行（末尾 LF は終端であり空行を生まない）。
fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    assert!(bytes.ends_with(b"\n"), "行末が LF でない");
    bytes[..bytes.len() - 1]
        .split(|&byte| byte == b'\n')
        .collect()
}

/// 指定した値列の行を追加順に持つ標本ドキュメントを作る。
fn document_with_rows(values: Vec<Vec<CellValue>>) -> (Document, SheetId, Vec<RowId>) {
    let mut doc = Document::new();
    let sheet = doc.add_sheet("標本");
    let mut ids = Vec::with_capacity(values.len());
    for row_values in values {
        let row = doc.add_row(sheet).expect("行の追加");
        doc.set_row_values(sheet, row, row_values)
            .expect("値の設定");
        ids.push(row);
    }
    (doc, sheet, ids)
}

/// 行順は与えられた順序のままで、ULID 昇順にも追加順にもソートされない（要件 1.5 / 3.5）。
///
/// 2 通りの入力順（ULID 降順・任意の並び）について、出力テキストの行順と復号後の
/// 行順の双方を確かめ、行の内容（値）が行と一緒に移動することも確かめる。
#[test]
fn row_order_is_preserved_and_never_sorted() {
    const ROWS: usize = 8;
    let values: Vec<Vec<CellValue>> = (0..ROWS)
        .map(|index| vec![CellValue::Text(format!("v{index}"))])
        .collect();
    let (mut doc, sheet, ids) = document_with_rows(values);
    let columns = vec!["label".to_string()];

    let reversed: Vec<RowId> = ids.iter().rev().copied().collect();
    let shuffled: Vec<RowId> = [3, 0, 7, 1, 6, 2, 5, 4]
        .iter()
        .map(|&position| ids[position])
        .collect();

    for order in [&reversed, &shuffled] {
        doc.reorder_rows(sheet, order).expect("並び替え");
        let (entry, bytes) = RowsCodec::encode(
            sheet,
            &columns,
            doc.sheet_by_id(sheet).expect("シート").rows(),
        )
        .expect("符号化");

        // 出力テキストの行頭 `$id` が入力順と一致する（復号とは独立の観測）。
        let expected_ids: Vec<String> = order.iter().map(|id| id.to_string()).collect();
        assert_eq!(expected_ids, line_ids(&bytes), "出力の行順が入力順と違う");

        // 復号後の行順も一致し、行と値が一緒に移動している。
        let decoded = RowsCodec::decode(&entry, &bytes).expect("復号");
        let observed: Vec<RowId> = decoded.rows().iter().map(|row| row.id()).collect();
        assert_eq!(
            order.as_slice(),
            observed.as_slice(),
            "復号の行順が入力順と違う"
        );
        for (position, &id) in order.iter().enumerate() {
            let origin = ids
                .iter()
                .position(|candidate| *candidate == id)
                .expect("既知の行");
            assert_eq!(
                vec![CellValue::Text(format!("v{origin}"))],
                decoded.rows()[position].values().to_vec(),
                "行 {position} の値が行と一緒に移動していない"
            );
        }
    }
}

/// シートごとに独立したエントリ名になり、復号で正しいシート識別子が戻る（要件 2.3）。
///
/// `sheets/*.jsonl` 以外のエントリを復号しようとするとコンテナ不正として拒否される。
#[test]
fn each_sheet_has_its_own_rows_entry() {
    let mut doc = Document::new();
    let sheet_a = doc.add_sheet("A");
    let sheet_b = doc.add_sheet("B");
    let row_a = doc.add_row(sheet_a).expect("行の追加");
    doc.set_row_values(sheet_a, row_a, vec![CellValue::Int(1)])
        .expect("値の設定");
    let row_b = doc.add_row(sheet_b).expect("行の追加");
    doc.set_row_values(sheet_b, row_b, vec![CellValue::Int(2)])
        .expect("値の設定");

    let columns = vec!["v".to_string()];
    let (entry_a, bytes_a) = RowsCodec::encode(
        sheet_a,
        &columns,
        doc.sheet_by_id(sheet_a).expect("シート").rows(),
    )
    .expect("符号化");
    let (entry_b, bytes_b) = RowsCodec::encode(
        sheet_b,
        &columns,
        doc.sheet_by_id(sheet_b).expect("シート").rows(),
    )
    .expect("符号化");

    assert_ne!(entry_a, entry_b, "異なるシートが同じエントリ名になった");
    assert_eq!(rows_entry(sheet_a), entry_a);
    assert_eq!(rows_entry(sheet_b), entry_b);

    let decoded_a = RowsCodec::decode(&entry_a, &bytes_a).expect("復号");
    let decoded_b = RowsCodec::decode(&entry_b, &bytes_b).expect("復号");
    assert_eq!(sheet_a, decoded_a.sheet());
    assert_eq!(sheet_b, decoded_b.sheet());
    assert_eq!(
        vec![row_a],
        decoded_a
            .rows()
            .iter()
            .map(|row| row.id())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        vec![CellValue::Int(1)],
        decoded_a.rows()[0].values().to_vec()
    );
    assert_eq!(
        vec![CellValue::Int(2)],
        decoded_b.rows()[0].values().to_vec()
    );
    assert_eq!(columns, decoded_a.columns());

    // 再エクスポートと定義元が同一の型であること（`parts::` 配下の公開面）。
    fn same_type(codec: rows_codec::RowsCodec) -> RowsCodec {
        codec
    }
    let _: rows_codec::RowsCodec = same_type(RowsCodec);

    // `sheets/*.jsonl` 以外のエントリは復号を拒否する。
    let hex = "0".repeat(64);
    let others = [
        "manifest.json".to_string(),
        "document.json".to_string(),
        format!("schemas/{sheet_a}.json"),
        format!("attachments/{hex}.bin"),
    ];
    for other in others {
        let entry = EntryName::parse(&other).expect("許可リスト内");
        let Err(DocumentError::InvalidContainer { entry: label }) =
            RowsCodec::decode(&entry, &bytes_a)
        else {
            panic!("{other} の復号が受理された");
        };
        assert!(
            label.starts_with(&format!("{other}: ")),
            "entry がエントリ名で始まらない: {label}"
        );
    }
}

/// 1 セル変更 → 変化するのはちょうど 1 テキスト行（要件 3.5 の行粒度）。
#[test]
fn changing_one_cell_changes_exactly_one_line() {
    const ROWS: usize = 100;
    let values: Vec<Vec<CellValue>> = (0..ROWS)
        .map(|index| {
            vec![
                CellValue::Text(format!("行 {index}")),
                CellValue::Int(index as i64),
            ]
        })
        .collect();
    let (mut doc, sheet, ids) = document_with_rows(values);
    let columns = vec!["label".to_string(), "n".to_string()];

    let (_, before) = RowsCodec::encode(
        sheet,
        &columns,
        doc.sheet_by_id(sheet).expect("シート").rows(),
    )
    .expect("符号化");
    doc.set_row_values(
        sheet,
        ids[57],
        vec![CellValue::Text("変更後".into()), CellValue::Int(57)],
    )
    .expect("値の設定");
    let (_, after) = RowsCodec::encode(
        sheet,
        &columns,
        doc.sheet_by_id(sheet).expect("シート").rows(),
    )
    .expect("符号化");

    let before_lines = lines(&before);
    let after_lines = lines(&after);
    assert_eq!(ROWS, before_lines.len());
    assert_eq!(before_lines.len(), after_lines.len(), "行数が変わった");
    let changed: Vec<usize> = (0..before_lines.len())
        .filter(|&index| before_lines[index] != after_lines[index])
        .collect();
    assert_eq!(vec![57], changed, "変化したテキスト行がちょうど 1 行でない");
}

/// 列名の `$` エスケープと予約キー `$id` の扱い。
///
/// `$` 始まりの列名は wire で `$` が 1 つ増え、`$id` は完全一致のときだけ行識別子として
/// 扱われる（列名としての `$id` は `$$id` になるため衝突しない）。
#[test]
fn leading_dollar_column_names_are_escaped_and_round_trip() {
    let values = vec![
        CellValue::Text("合計".into()),
        CellValue::Text("深い".into()),
        CellValue::Text("これは行識別子ではない".into()),
        CellValue::Int(4),
    ];
    let (doc, sheet, ids) = document_with_rows(vec![values.clone()]);
    let row = ids[0];
    let columns: Vec<String> = ["$total", "$$deep", "$id", "plain"]
        .iter()
        .map(|name| (*name).to_string())
        .collect();

    let (entry, bytes) = RowsCodec::encode(
        sheet,
        &columns,
        doc.sheet_by_id(sheet).expect("シート").rows(),
    )
    .expect("符号化");
    let text = String::from_utf8(bytes.clone()).expect("UTF-8");
    assert!(
        text.starts_with(&format!("{{\"$id\":\"{row}\",")),
        "予約キーが先頭でない: {text}"
    );
    assert!(
        text.contains(r#""$$total":"合計""#),
        "`$` 始まりの列名がエスケープされない: {text}"
    );
    assert!(
        text.contains(r#""$$$deep":"深い""#),
        "`$$` 始まりの列名がエスケープされない: {text}"
    );
    assert!(
        text.contains(r#""$$id":"これは行識別子ではない""#),
        "予約キーに見える列名が壊れた: {text}"
    );

    let decoded = RowsCodec::decode(&entry, &bytes).expect("復号");
    assert_eq!(columns, decoded.columns(), "列名が往復で変わった");
    assert_eq!(
        row,
        decoded.rows()[0].id(),
        "予約キーでない列が行識別子として読まれた"
    );
    assert_eq!(values, decoded.rows()[0].values().to_vec());
}

/// 行間でキー列（`$id` + 同一列集合 + 同一順序）が一致しない入力は、行番号付きで拒否する。
///
/// 欠落・順序違い・余分のいずれも黙って穴埋めせず、`InvalidContainer` にする。
#[test]
fn rows_with_different_key_columns_are_rejected() {
    let sheet: SheetId = SHEET_A.parse().expect("標本 ULID");
    let entry = rows_entry(sheet);
    let first = format!(r#"{{"$id":"{ROW_1}","a":1,"b":2}}"#);
    // 対照: 3 行が同じ形なら通る（拒否が形の不一致だけを狙っていること）。
    let uniform = format!(
        "{first}\n{}\n{}\n",
        format!(r#"{{"$id":"{ROW_2}","a":3,"b":4}}"#),
        format!(r#"{{"$id":"{ROW_3}","a":5,"b":6}}"#),
    );
    let decoded = RowsCodec::decode(&entry, uniform.as_bytes()).expect("同一形の行は通る");
    assert_eq!(3, decoded.rows().len());

    let deviants = [
        // 列が欠けている。
        format!(r#"{{"$id":"{ROW_2}","a":1}}"#),
        // キーの順序が違う。
        format!(r#"{{"a":1,"b":2,"$id":"{ROW_2}"}}"#),
        // 余分なキーがある。
        format!(r#"{{"$id":"{ROW_2}","a":1,"b":2,"c":3}}"#),
    ];
    for (index, deviant) in deviants.iter().enumerate() {
        let input = format!("{first}\n{deviant}\n");
        let Err(DocumentError::InvalidContainer { entry: label }) =
            RowsCodec::decode(&entry, input.as_bytes())
        else {
            panic!("形の違う行 {index} が受理された");
        };
        assert!(
            label.starts_with(&format!("{entry} line 2: ")),
            "行番号付きの診断でない: {label}"
        );
    }
}

/// 同一ファイル内の自己矛盾（行識別子の重複・同じ行オブジェクト内のキー重複）は拒否する。
#[test]
fn self_contradictions_are_rejected() {
    let sheet: SheetId = SHEET_A.parse().expect("標本 ULID");
    let entry = rows_entry(sheet);
    let line = format!(r#"{{"$id":"{ROW_1}","a":1}}"#);

    let duplicated_id = format!("{line}\n{line}\n");
    let Err(DocumentError::InvalidContainer { entry: label }) =
        RowsCodec::decode(&entry, duplicated_id.as_bytes())
    else {
        panic!("行識別子の重複が受理された");
    };
    assert!(
        label.starts_with(&format!("{entry} line 2: ")),
        "行番号付きの診断でない: {label}"
    );

    let duplicated_key = format!(r#"{{"$id":"{ROW_1}","a":1,"a":2}}"#) + "\n";
    let Err(DocumentError::InvalidContainer { entry: label }) =
        RowsCodec::decode(&entry, duplicated_key.as_bytes())
    else {
        panic!("同じ行オブジェクト内のキー重複が受理された");
    };
    assert!(
        label.starts_with(&format!("{entry} line 1: ")),
        "行番号付きの診断でない: {label}"
    );
}

/// i64 範囲外の整数リテラルは黙って浮動小数へ落ちず、コンテナ不正として拒否される。
///
/// `json::read_ndjson` は汎用コーデックであり整数リテラルの範囲検査をしないため、
/// 各セルの**原文**を `value::from_json_bytes` へ通す門が効いている必要がある
/// （task 3.3 レビューの必須条件）。
#[test]
fn out_of_range_integer_literals_are_rejected() {
    let sheet: SheetId = SHEET_A.parse().expect("標本 ULID");
    let entry = rows_entry(sheet);

    for literal in ["-9223372036854775809", "99999999999999999999999999"] {
        let input = format!(r#"{{"$id":"{ROW_1}","n":{literal}}}"#) + "\n";
        let Err(DocumentError::InvalidContainer { entry: label }) =
            RowsCodec::decode(&entry, input.as_bytes())
        else {
            panic!("{literal} が拒否されなかった（黙って浮動小数へ落ちた）");
        };
        assert!(
            label.starts_with(&format!("{entry} line 1: ")),
            "行番号付きでない: {label}"
        );
        assert!(label.contains("column n"), "列が特定できない: {label}");
    }

    // 境界の内側は通る（門が過剰でないこと）。
    for (literal, expected) in [
        ("9223372036854775807", i64::MAX),
        ("-9223372036854775808", i64::MIN),
    ] {
        let input = format!(r#"{{"$id":"{ROW_1}","n":{literal}}}"#) + "\n";
        let decoded = RowsCodec::decode(&entry, input.as_bytes()).expect("境界内は通る");
        assert_eq!(
            vec![CellValue::Int(expected)],
            decoded.rows()[0].values().to_vec()
        );
    }
}

/// セル値の忠実な往復（非 ASCII・絵文字・入れ子・添付参照・`null`・空文字列・`-0.0`）。
///
/// 1 行の確定形をバイト単位で固定する（列順・`$id` 先頭・`null` や空文字列を
/// 間引かないことの証拠）。
#[test]
fn cell_values_round_trip_verbatim() {
    let attachment = AttachmentId::from_bytes(b"probe");
    let values = vec![
        CellValue::Text("日本語 🎉 \u{2028} \"x\"\n".into()),
        CellValue::Nested(NestedValue::Object(vec![
            ("z".into(), CellValue::Int(1)),
            ("a".into(), CellValue::Text("x".into())),
        ])),
        CellValue::Attachment(attachment),
        CellValue::Null,
        CellValue::Text(String::new()),
        CellValue::float(-0.0),
    ];
    let columns: Vec<String> = ["text", "nested", "attachment", "null", "empty", "zero"]
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    let (doc, sheet, ids) = document_with_rows(vec![values.clone()]);
    let row = ids[0];

    let (entry, bytes) = RowsCodec::encode(
        sheet,
        &columns,
        doc.sheet_by_id(sheet).expect("シート").rows(),
    )
    .expect("符号化");
    let expected = format!(
        "{{\"$id\":\"{row}\",\"text\":\"日本語 🎉 \u{2028} \\\"x\\\"\\n\",\
         \"nested\":{{\"z\":1,\"a\":\"x\"}},\"attachment\":\"{attachment}\",\
         \"null\":null,\"empty\":\"\",\"zero\":0.0}}\n"
    );
    assert_eq!(expected, String::from_utf8(bytes.clone()).expect("UTF-8"));

    let decoded = RowsCodec::decode(&entry, &bytes).expect("復号");
    assert_eq!(columns, decoded.columns());
    assert_eq!(values, decoded.rows()[0].values().to_vec());
    assert_eq!(row, decoded.rows()[0].id());
    // 再符号化しても同一バイト列（同一入力 → 同一バイト列）。
    let (_, again) = RowsCodec::encode(sheet, &columns, decoded.rows()).expect("再符号化");
    assert_eq!(bytes, again);
}

/// 0 行のシートは 0 バイトになり、復号は 0 行を返す（`write_ndjson` の規約を継承）。
#[test]
fn an_empty_sheet_encodes_to_zero_bytes() {
    let (doc, sheet, ids) = document_with_rows(Vec::new());
    assert!(ids.is_empty());
    let columns = vec!["a".to_string()];
    let (entry, bytes) = RowsCodec::encode(
        sheet,
        &columns,
        doc.sheet_by_id(sheet).expect("シート").rows(),
    )
    .expect("符号化");
    assert!(bytes.is_empty(), "0 行の出力が 0 バイトでない");
    let decoded = RowsCodec::decode(&entry, &bytes).expect("復号");
    assert_eq!(sheet, decoded.sheet());
    assert!(decoded.rows().is_empty());
    assert!(
        decoded.columns().is_empty(),
        "0 行のエントリは列順を持たない"
    );
}

/// 書き手側の programming error（値の個数不一致・列名の重複・非有限値）は `panic` せず
/// 最小のローカル型 [`RowsEncodeError`] で報告され、バイト列を一切返さない。
#[test]
fn encode_reports_input_errors_with_the_local_type() {
    let values = vec![vec![CellValue::Int(1)]];
    let (mut doc, sheet, ids) = document_with_rows(values);
    let row = ids[0];
    let columns = vec!["a".to_string(), "b".to_string()];

    let Err(error) = RowsCodec::encode(
        sheet,
        &columns,
        doc.sheet_by_id(sheet).expect("シート").rows(),
    ) else {
        panic!("値の個数不一致が受理された");
    };
    match error {
        RowsEncodeError::ValueCountMismatch {
            row: reported,
            columns: got,
            values: given,
        } => {
            assert_eq!(row, reported);
            assert_eq!(2, got);
            assert_eq!(1, given);
        }
        other => panic!("想定外のエラー: {other:?}"),
    }

    doc.set_row_values(sheet, row, vec![CellValue::Int(1), CellValue::Int(2)])
        .expect("値の設定");
    let duplicated = vec!["a".to_string(), "a".to_string()];
    let Err(error) = RowsCodec::encode(
        sheet,
        &duplicated,
        doc.sheet_by_id(sheet).expect("シート").rows(),
    ) else {
        panic!("列名の重複が受理された");
    };
    match error {
        RowsEncodeError::DuplicateColumn { column } => assert_eq!(1, column),
        other => panic!("想定外のエラー: {other:?}"),
    }

    // 正常系は通る。
    let (_, bytes) = RowsCodec::encode(
        sheet,
        &columns,
        doc.sheet_by_id(sheet).expect("シート").rows(),
    )
    .expect("符号化");
    assert_eq!(1, lines(&bytes).len());

    // 非有限値は value 層の単一の源（`to_json_bytes`）が遮断し、型付きエラーになる。
    doc.set_row_values(
        sheet,
        row,
        vec![CellValue::Float(f64::NAN), CellValue::Int(2)],
    )
    .expect("値の設定");
    let Err(error) = RowsCodec::encode(
        sheet,
        &columns,
        doc.sheet_by_id(sheet).expect("シート").rows(),
    ) else {
        panic!("NaN が受理された");
    };
    match error {
        RowsEncodeError::Document(DocumentError::NonRepresentableNumber { location }) => {
            assert!(
                location.starts_with(&format!("sheets/{sheet}.jsonl line 1: column a")),
                "位置が特定できない: {location}"
            );
        }
        other => panic!("想定外のエラー: {other:?}"),
    }
}

/// 復号は `SheetRows` として、シート識別子・列順・行をすべてアクセサで観測できること。
/// 列順は**ファイル自身のキー順**であり、呼び出し元が与えた順序に依存しない。
#[test]
fn decoded_rows_expose_the_files_own_column_order() {
    let sheet: SheetId = SHEET_B.parse().expect("標本 ULID");
    let entry = rows_entry(sheet);
    // 列名の並びが呼び出し元の与える順序と違っても、ファイルのキー順がそのまま列順になる。
    let input = format!(
        "{}\n{}\n",
        format!(r#"{{"$id":"{ROW_1}","b":1,"a":2}}"#),
        format!(r#"{{"$id":"{ROW_2}","b":3,"a":4}}"#),
    );
    let decoded: SheetRows = RowsCodec::decode(&entry, input.as_bytes()).expect("復号");
    assert_eq!(sheet, decoded.sheet());
    assert_eq!(vec!["b".to_string(), "a".to_string()], decoded.columns());
    assert_eq!(2, decoded.rows().len());
    assert_eq!(
        vec![CellValue::Int(1), CellValue::Int(2)],
        decoded.rows()[0].values().to_vec()
    );
    assert_eq!(
        ROW_1.parse::<RowId>().expect("標本 ULID"),
        decoded.rows()[0].id()
    );
}

/// 復号が受理するキーは**書き手の像**に限る（単一 `$` で始まり `$id` でも `$$` でもない
/// 形のキーを、サニタイズせず行番号付きで拒否する）。
///
/// `entry_name.rs` の「サニタイズではなく拒否」と同じ規約である。とくに `"$x"` と `"$$x"`
/// の同居はエスケープ解除後に列名が重複する入力であり、開いた時点で破綻として検出する。
#[test]
fn keys_outside_the_writers_image_are_rejected() {
    let sheet: SheetId = SHEET_A.parse().expect("標本 ULID");
    let entry = rows_entry(sheet);

    // 像内: 素の列名・`$$` 始まり（`$` 始まり列名のエスケープ形）・完全一致の `$id`。
    let inside = format!(
        "{}\n{}\n",
        format!(r#"{{"$id":"{ROW_1}","plain":1,"$$escaped":2}}"#),
        format!(r#"{{"$id":"{ROW_2}","plain":3,"$$escaped":4}}"#),
    );
    let decoded = RowsCodec::decode(&entry, inside.as_bytes()).expect("像内のキーは通る");
    assert_eq!(
        vec!["plain".to_string(), "$escaped".to_string()],
        decoded.columns(),
        "エスケープ形の列名が正しく戻らない"
    );
    assert_eq!(2, decoded.rows().len());

    // 像外: 単一 `$` で始まり `$id` でも `$$` でもないキー。
    let foreigners = [
        format!(r#"{{"$id":"{ROW_1}","$x":1}}"#),
        format!(r#"{{"$id":"{ROW_1}","$":1}}"#),
        // 像外キー `$x` と エスケープ形 `$$x`（列名 `$x`）の同居 = 列名が重複する入力。
        format!(r#"{{"$id":"{ROW_1}","$x":1,"$$x":2}}"#),
    ];
    for (index, foreign) in foreigners.iter().enumerate() {
        let input = format!("{foreign}\n");
        let Err(DocumentError::InvalidContainer { entry: label }) =
            RowsCodec::decode(&entry, input.as_bytes())
        else {
            panic!("像外のキーを含む行 {index} が受理された: {foreign}");
        };
        assert!(
            label.starts_with(&format!("{entry} line 1: ")),
            "行番号付きの診断でない: {label}"
        );
    }

    // 2 行目以降でも像外キーは拒否される（キー列の一致検査が先に弾く）。
    let second = format!(
        "{}\n{}\n",
        format!(r#"{{"$id":"{ROW_1}","a":1}}"#),
        format!(r#"{{"$id":"{ROW_2}","$x":1}}"#),
    );
    let Err(DocumentError::InvalidContainer { entry: label }) =
        RowsCodec::decode(&entry, second.as_bytes())
    else {
        panic!("2 行目の像外キーが受理された");
    };
    assert!(
        label.starts_with(&format!("{entry} line 2: ")),
        "行番号付きの診断でない: {label}"
    );
}

/// `to_json_bytes` を通したセル値の wire 表現が行エントリの値と一致すること
/// （本モジュールが value 層の単一の源を使っていることの観測）。
#[test]
fn cell_wire_bytes_match_the_value_layer() {
    let values = vec![
        vec![CellValue::Decimal("1e3".into())],
        vec![CellValue::Text("1e3".into())],
        vec![CellValue::float(-0.0)],
    ];
    let (doc, sheet, _) = document_with_rows(values.clone());
    let columns = vec!["v".to_string()];
    let (_, bytes) = RowsCodec::encode(
        sheet,
        &columns,
        doc.sheet_by_id(sheet).expect("シート").rows(),
    )
    .expect("符号化");
    let encoded: Vec<String> = lines(&bytes)
        .into_iter()
        .map(|line| {
            let text = String::from_utf8(line.to_vec()).expect("UTF-8");
            let (_, cell) = text.split_once("\"v\":").expect("列 v が無い");
            cell.strip_suffix('}').expect("レコードの終端").to_string()
        })
        .collect();
    let expected: Vec<String> = values
        .iter()
        .map(|row| {
            let raw = to_json_bytes(&row[0], "probe").expect("値の符号化");
            String::from_utf8(raw).expect("UTF-8")
        })
        .collect();
    assert_eq!(expected, encoded, "セルの wire 表現が value 層と一致しない");
}

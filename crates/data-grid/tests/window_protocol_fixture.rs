//! 窓の二進形式の**言語をまたぐ固定**（データグリッドのタスク 7.3。data-grid 要件 1.1, 1.2）。
//!
//! # 何を固定するか
//!
//! TS 側（`src/features/grid/windowCache.ts`）は、本クレートの `WindowCodec` が書く窓を
//! **同じバイト列として**読めなければならない。両側が同じ表を写しているだけでは、写し違い
//! （エンディアン・欄の幅・並び）がどちらの検査でも見えない — とくに TS 側だけで符号化と
//! 復号を往復させても、**両方が同じ誤りを共有していれば緑になる**。
//!
//! したがって:
//!
//! 1. **本物の符号化器が書いたバイト列を固定ファイル**
//!    （`tests/fixtures/window_protocol.txt`）へ置く。値は**手で書いたものではなく**、
//!    下の [`fixture_window`] が本物の `WindowCodec::encode` から出したものである
//!    （作り方は固定ファイルのヘッダに書いてある）。
//! 2. 本ファイルが「符号化器は**いまも**それを書く」ことを表明する。
//! 3. TS 側が「**そのバイト列を**そう読める」ことを表明する。
//!
//! 片側だけが表を変えると、**変えた側の検査が落ちる**。
//!
//! # 何を固定しないか（正直な限界）
//!
//! **要求の頭**（`grid_rows_window` の引数。`src-tauri/src/commands/grid.rs` が唯一の源）は
//! 本ファイルの対象外である。要求の符号化を持つのは 6.3 の適応層（`src-tauri`）であり、
//! 本クレートは要求のバイト列を読まない（`transport` のモジュール docs「本モジュールが
//! 持たないもの」）。本クレートのテストから 6.3 の符号化器を呼ぶ経路は無く、境界の外でも
//! あるため、**要求側の固定は TS 側のオフセットの表明だけ**である（`windowCache.test.ts` の
//! 表を参照）。
//!
//! # 固定の内容（何が窓に現れるか）
//!
//! | 位置 | 内容 |
//! |---|---|
//! | 頭 | 版 1・世代 7・開始序数 0・行 3・列 4 |
//! | 行 0 | 固定の ULID。`Text("日本語")` / `Int(3)`（**違反。下限 4**）/ `Bool(true)` / `Null` |
//! | 行 1 | 固定の ULID。`Text("x")` / `Int(12)` / `Bool(false)` / `Text("備考2")` |
//! | 行 2 | 固定の ULID。`Text("")` / `Int(5)` / `Bool(true)` / `Null` |
//!
//! 非 ASCII（`日本語` / `備考2`）は表示文字列の長さが**バイト数**であることを、`Null` は
//! 長さ 0 の表示文字列を、行 0 の `Int` の違反は違反の有無のバイトと札の塊（段 0）を固定する。
//! 行 0 と行 2 の `Null` は同じ並びの別の位置にも現れる（位置がずれれば気づける）。

use std::collections::HashMap;
use std::str::FromStr;

use data_grid::{
    decode_window, Generation, NestedPath, RowOrder, RowOrdinal, RowSpan, VariantTag, ViewSpec,
    ViolationIndex, WindowCodec, WindowRequest, WindowRowSource, HEADER_LEN, ROW_KEY_LEN,
};
use document_format::parts::RowsCodec;
use document_format::{CellValue, Document, EntryName, RowId, SheetId};
use schema_engine::{
    compile_declaration, validate_sheet, ColumnDecl, CompiledSchema, Constraints, DeclaredKind,
    Schema, TypeDecl, TypeKind, TypeRegistry, ValidationOptions,
};

/// 行データのエントリ名にだけ要るシート識別子（文書のシートは `Document::add_sheet` が発行する）。
const ENTRY_SHEET: &str = "01ARZ3NDEKTSV4RRFFQ69G5FB0";

/// 固定の行識別子（標本の ULID は発行のたびに変わるため、**窓へ入る値を固定する**）。
const ROW_IDS: [&str; 3] = [
    "01ARZ3NDEKTSV4RRFFQ69G5FAW",
    "01ARZ3NDEKTSV4RRFFQ69G5FAX",
    "01ARZ3NDEKTSV4RRFFQ69G5FAY",
];

/// 列名（並びが窓のセルの並びになる）。
const COLUMNS: [&str; 4] = ["名前", "数量", "区分", "備考"];

/// 行データの NDJSON（`RowsCodec` の表に従ってテスト側が手で書いたもの）。
///
/// **本物の復号器がこれを読む**ので、値の変種（`Text` / `Int` / `Bool` / `Null`）と
/// 非 ASCII の表示文字列、そして 1 件の違反（`数量` の下限 4 に対して 3）が窓に現れる。
const ROWS_JSONL: &str = concat!(
    r#"{"$id":"01ARZ3NDEKTSV4RRFFQ69G5FAW","名前":"日本語","数量":3,"区分":true,"備考":null}"#,
    "\n",
    r#"{"$id":"01ARZ3NDEKTSV4RRFFQ69G5FAX","名前":"x","数量":12,"区分":false,"備考":"備考2"}"#,
    "\n",
    r#"{"$id":"01ARZ3NDEKTSV4RRFFQ69G5FAY","名前":"","数量":5,"区分":true,"備考":null}"#,
    "\n",
);

/// 固定の世代（頭の 8 バイトに現れる値）。
const GENERATION: u64 = 7;

/// 固定ファイル（両側が読む唯一のバイト列の源）。
const FIXTURE: &str = include_str!("fixtures/window_protocol.txt");

/// 固定ファイルを `key = value` の表として読む（`#` の行と空行は読み飛ばす）。
fn fixture() -> HashMap<String, String> {
    let mut fields = HashMap::new();
    for line in FIXTURE.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("固定ファイルの行が `key = value` ではない: {line}"));
        fields.insert(key.trim().to_owned(), value.trim().to_owned());
    }
    fields
}

/// 固定ファイルの 16 進をバイト列へ戻す。
fn fixture_bytes(fields: &HashMap<String, String>, key: &str) -> Vec<u8> {
    let hex = fields
        .get(key)
        .unwrap_or_else(|| panic!("固定ファイルに `{key}` が無い"));
    assert_eq!(0, hex.len() % 2, "`{key}` の 16 進が偶数長でない");
    (0..hex.len() / 2)
        .map(|index| {
            u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
                .unwrap_or_else(|error| panic!("`{key}` の 16 進が読めない: {error}"))
        })
        .collect()
}

/// バイト列の 16 進表記（小文字。固定ファイルと同じ形）。
fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 整数の型（既定の制約）。
fn kind(kind: TypeKind) -> TypeDecl {
    TypeDecl::Kind {
        kind: DeclaredKind::Known(kind),
        constraints: Constraints::default(),
    }
}

/// 列の宣言（必須・一意・既定値・説明は使わない）。
fn column(name: &str, ty: TypeDecl) -> ColumnDecl {
    ColumnDecl {
        name: name.into(),
        ty,
        required: false,
        unique: false,
        default: None,
        description: None,
    }
}

/// 固定の文書と計画（同じ入力からは常に同じ窓のバイト列が出る）。
///
/// 行は `RowsCodec` の表に従って手で書いた NDJSON を**本物の復号器**に読ませて得る
/// （`Row::new` はクレート内に閉じているため、外から識別子を指定して行を作る唯一の公開経路
/// である）。文書のシート識別子は発行のたびに変わるが、**窓には現れない**（窓が運ぶのは
/// 行識別子とセルだけである）。
fn fixture_document() -> (Document, SheetId, CompiledSchema) {
    let mut document = Document::new();
    let sheet = document.add_sheet("標本");
    let names: Vec<String> = COLUMNS.iter().map(|name| (*name).to_owned()).collect();
    document
        .set_sheet_columns(sheet, names)
        .expect("標本のシートは骨格にある");

    let entry = EntryName::Rows {
        sheet: SheetId::from_str(ENTRY_SHEET).expect("正準のシート識別子"),
    };
    let rows = RowsCodec::decode(&entry, ROWS_JSONL.as_bytes())
        .expect("手で書いた行データを本物の復号器が読める")
        .into_rows();
    // 行の識別子が固定されていることを先に表明する（標本の ULID を使い回さない）。
    let ids: Vec<RowId> = rows.iter().map(|row| row.id()).collect();
    let expected: Vec<RowId> = ROW_IDS
        .iter()
        .map(|id| RowId::from_str(id).expect("正準の行識別子"))
        .collect();
    assert_eq!(expected, ids, "行の識別子が固定されていない");
    document
        .insert_rows_at(sheet, 0, rows)
        .expect("固定の行識別子は文書内で一意である");

    let schema = compile_declaration(
        &Schema {
            columns: vec![
                column("名前", kind(TypeKind::Text)),
                column(
                    "数量",
                    TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Int),
                        // 下限 4（既定の制約ではないため `kind` を使わない）。
                        constraints: Constraints {
                            min: Some(CellValue::Int(4)),
                            ..Constraints::default()
                        },
                    },
                ),
                column("区分", kind(TypeKind::Bool)),
                column("備考", kind(TypeKind::Any)),
            ],
        },
        &[],
        &TypeRegistry::new(),
    )
    .expect("標本の宣言は妥当である");
    assert_eq!(COLUMNS.len(), schema.column_count(), "列の数が食い違う");
    (document, sheet, schema)
}

/// 行の値の口（窓の符号化は行の値の在処を知らない）。
struct Source<'a> {
    document: &'a Document,
    sheet: SheetId,
}

impl WindowRowSource for Source<'_> {
    fn values(&self, row: RowId) -> Option<&[CellValue]> {
        self.document
            .sheets()
            .iter()
            .find(|sheet| sheet.id() == self.sheet)
            .and_then(|sheet| sheet.rows().iter().find(|candidate| candidate.id() == row))
            .map(|row| row.values())
    }
}

/// 固定の文書から、固定の区間の窓を 1 つ符号化する（**本物の符号化器**）。
fn fixture_window(start: usize, count: usize) -> Vec<u8> {
    let (document, sheet, schema) = fixture_document();
    let mut order = RowOrder::default();
    let report = validate_sheet(&document, sheet, &schema, &ValidationOptions::unlimited());
    let mut index = ViolationIndex::build(&report, &RowOrder::default());
    index.install(&mut order);
    order.recompute(&document, sheet, &ViewSpec::default());
    index.rekey(&order);
    assert_eq!(3, order.len(), "可視行が 3 行ではない");

    let codec = WindowCodec::new(Generation::new(GENERATION));
    let request = WindowRequest::new(
        Generation::new(GENERATION),
        RowSpan::new(RowOrdinal::new(start), count),
    );
    let source = Source {
        document: &document,
        sheet,
    };
    codec
        .encode(&order, &index, COLUMNS.len(), &source, &request)
        .expect("固定の窓が符号化できる")
}

/// **符号化器はいまも固定ファイルのバイト列を書く**（片側だけの変更はここで落ちる）。
#[test]
fn the_encoder_still_produces_the_committed_fixture_bytes() {
    let fields = fixture();

    let window = fixture_window(0, 3);
    assert_eq!(
        fields.get("window").map(String::as_str),
        Some(to_hex(&window).as_str()),
        "窓のバイト列が固定ファイルと食い違う（表を変えたら固定ファイルと TS 側を同時に直すこと）"
    );

    // 可視行の末尾に接する要求は**行 0 の窓**（頭だけの 33 バイト）であり、空の窓（長さ 0）
    // ではない — 画面は「端に達した」と「要求が通らなかった」を別に扱う。
    let tail = fixture_window(3, 2);
    assert_eq!(HEADER_LEN, tail.len(), "行 0 の窓が頭だけになっていない");
    assert_eq!(
        fields.get("window_rows_0").map(String::as_str),
        Some(to_hex(&tail).as_str()),
        "行 0 の窓のバイト列が固定ファイルと食い違う"
    );
}

/// **固定ファイルのバイト列は、本物の復号器が表のとおりに読める**（内容の期待値を固定する）。
///
/// TS 側は同じバイト列を読み、**同じ期待値**（表の内容）を表明する。両側が同じ期待値へ
/// 到達することが「言語をまたいで同じ形式を読んでいる」ことの実体である。
#[test]
fn the_committed_fixture_bytes_decode_into_the_documented_content() {
    let fields = fixture();
    let bytes = fixture_bytes(&fields, "window");
    let decoded = decode_window(&bytes).expect("固定ファイルの窓が復号できる");

    assert_eq!(1, decoded.version());
    assert_eq!(Generation::new(GENERATION), decoded.generation());
    assert_eq!(RowOrdinal::new(0), decoded.start());
    assert_eq!(3, decoded.row_count());
    assert_eq!(COLUMNS.len(), decoded.columns());
    assert!(
        bytes.len() > HEADER_LEN + decoded.row_count() * ROW_KEY_LEN,
        "固定ファイルの窓にセルが 1 つも無い"
    );

    // 内容の表（固定ファイルのヘッダと同じ並び。**期待値はここが唯一の源である**）。
    let expected: [[(&str, u8, bool, &str); 4]; 3] = [
        [
            ("日本語", VariantTag::TEXT.byte(), false, "0"),
            ("3", VariantTag::INT.byte(), true, "0"),
            ("true", VariantTag::BOOL.byte(), false, "0"),
            ("", VariantTag::NULL.byte(), false, "0"),
        ],
        [
            ("x", VariantTag::TEXT.byte(), false, "0"),
            ("12", VariantTag::INT.byte(), false, "0"),
            ("false", VariantTag::BOOL.byte(), false, "0"),
            ("備考2", VariantTag::TEXT.byte(), false, "0"),
        ],
        [
            ("", VariantTag::TEXT.byte(), false, "0"),
            ("5", VariantTag::INT.byte(), false, "0"),
            ("true", VariantTag::BOOL.byte(), false, "0"),
            ("", VariantTag::NULL.byte(), false, "0"),
        ],
    ];

    let root = NestedPath::root();
    for (row_index, row) in decoded.rows().iter().enumerate() {
        let expected_row = &expected[row_index];
        // 行の識別子は**固定ファイルが名指す鍵そのもの**である（TS 側はこれを ULID の
        // 26 文字へ写して `EditOutcome.affected` と突き合わせる）。
        let expected_key: RowId = ROW_IDS[row_index].parse().expect("正準の行識別子");
        assert_eq!(
            expected_key.ulid().to_bytes(),
            row.key(),
            "行 {row_index} の識別子の生バイトが違う"
        );
        for (column_index, cell) in row.cells().iter().enumerate() {
            let (text, tag, violated, _unused) = expected_row[column_index];
            assert_eq!(
                text,
                cell.text(),
                "行 {row_index} 列 {column_index} の表示文字列"
            );
            assert_eq!(
                tag,
                cell.tag().byte(),
                "行 {row_index} 列 {column_index} の変種の札"
            );
            assert_eq!(
                violated,
                cell.violated(),
                "行 {row_index} 列 {column_index} の違反の有無"
            );
            if violated {
                // セル直下の違反は**段 0 の札 1 つ**として現れる（違反なしと区別できる）。
                assert_eq!(
                    vec![root.clone()],
                    cell.marks().to_vec(),
                    "行 {row_index} 列 {column_index} の違反の札"
                );
            } else {
                assert!(cell.marks().is_empty(), "違反なしに札が付いている");
            }
        }
    }

    // 違反の総数（固定ファイルが名指す 1 件）と、窓が運ぶ違反の数が一致する。
    let violated: usize = decoded
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .filter(|cell| cell.violated())
        .count();
    assert_eq!(1, violated, "窓が運ぶ違反の数が違う");
}

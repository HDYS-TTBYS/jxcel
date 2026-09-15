//! 可視範囲の窓の二進形式（データグリッドのタスク 5.1。data-grid 要件 1.1, 1.2, 4.5, 11.2,
//! 11.6）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。固定するのは次のもので
//! ある。
//!
//! 1. **頭の欄**（版・世代・開始序数・行数・列数）。復号がそれらを晒し、**バイト位置と
//!    エンディアン**（[`WINDOW_FORMAT_VERSION`] と [`HEADER_LEN`]）を生バイトから読んで
//!    突き合わせる — 表だけを docs に書いて実装が別の位置へ書く状態を落とすためである。
//! 2. **往復**。実在の文書にまたがる区間を符号化し、復号した行の
//!    識別子・表示文字列・変種の札・違反の札が**符号化の入力と一致する**ことを見る
//!    （要件 1.1, 1.2, 4.1）。表示文字列の期待値は `view` 層の [`display_text`] から導き、
//!    5.1 が写しを作っていないことを固定する（tasks.md の Implementation Notes の規則）。
//! 3. **64 ビット整数と 10 進数が文字列のまま保たれる**（タスク 5.1 の強調点。要件 11.6）。
//!    `i64::MAX` と長い 10 進数を置いた文書を符号化し、復号した文字列が**元の文字列そのもの**
//!    であることを見る。加えて**数値として復元できない**ことを、桁が UTF-8 として現れること
//!    と、その値の 64 ビット表現が窓のどこにも現れないことの**双方**で確かめる（負の対照:
//!    f64 へ落とすと同じ文字列には戻らないことも表明する）。
//! 4. **入れ子のセルは要約を運び、構造を運ばない**（要件 5.6 の要約）。要約の文字列は
//!    [`display_text`] の規則そのものであり、**内側の値の中身は窓に現れない**ことを
//!    実際のバイト列の探索で確かめる。
//! 5. **内側のどの位置が違反しているかの札**（要件 4.5）。違反した入れ子のセルは、内側の
//!    位置を [`NestedPath`] として運び、その位置が**宣言が名指すフィールド**と一致することを
//!    確かめる（位置の正しさを、値の写しではなく宣言から導く）。
//! 6. **冪等性と決定性**（design.md の Idempotency 句）。同じ世代・同じ区間の 2 回の符号化が
//!    **バイト単位で一致**する。
//! 7. **世代が古い要求は空の窓**（design.md の Batch 契約）。空の窓の表現（長さ 0 のバイト列）
//!    と、**行 0 の窓（頭を持つ 33 バイト）とが区別できる**ことを見る。
//! 8. **範囲の検査**。可視行数の外を求めた要求は [`GridError::SpanOutOfRange`] であり、
//!    **1 行も書かず・panic しない**（文書が変わらないことを前後で比べる）。端に掛かる要求は
//!    切り落とす（`RowOrder::span` の規約）。
//! 9. **壊れた窓を panic せずに拒む**。未知の版と、**すべての真の接頭辞**（切り詰め）が
//!    `Err` になることを見る。**限界**: 接頭辞の拒否は「後ろを読む欄が無い」ことの証拠には
//!    ならない（接頭辞の長さが `>= 8` の位置を指す後方参照は、その接頭辞の中で完結しうる）。
//!    「1 回の走査」の主張は実装の形（`Cursor` が前方向のみ）に依り、この検査の証拠ではない
//!    — 詳細は `src/transport/mod.rs` のモジュール docs「1 回の走査の何が検査で固定され、
//!    何が固定されないか（正直な限界）」（**接頭辞の検査だけ**では捉えられず、他の検査と
//!    組にして初めて落ちることを実測して確かめてある）。
//! 10. **費用の形**（要件 11.2）。10 万行のシートから 40 行の窓を符号化するとき、符号化が
//!    行の取得に問い合わせる回数が**ちょうど窓の行数**であることを数えて固定する。
//!    **これは時間の主張ではない**（`verification.md`「速度を証拠にしない。証拠は『呼び出しの
//!    形』で取る」）。符号化は `Document` を一切受け取らず、`RowOrder::span` が返す高々
//!    `count` 行の切片だけを見る — 10 万行を走査する経路が**型の上に存在しない**。
//!
//! # 前提を先に表明する
//!
//! 標本（`tests/common/sample.rs`）を使う検査は、**標本がその検査の前提を満たしていることを
//! 最初に検査する**（tasks.md の Implementation Notes の規則）。「この窓には違反したセルが
//! 入っている」「この並べ替えは文書の並びを実際に変える」といった前提を表明してから依拠し、
//! 期待値を手書きの写しにしない。
//!
//! **標本の識別子は発行のたびに変わる**ため、期待値に生の識別子を書かない。行は
//! 標本が公開する行の並び（[`Sample::row_ids`]）と突き合わせ、比較するのは**構造**
//! （行の位置・列の添字・入れ子の位置）である。

mod common;

use std::cell::Cell;
use std::collections::HashMap;

use common::sample::{sample, Sample, SampleOptions};
use data_grid::{
    decode_window, display_text, ColumnIndex, Generation, GridError, NestedPath, RowOrder,
    RowOrdinal, RowSpan, SortKey, VariantTag, ViewSpec, ViolationIndex, WindowCodec,
    WindowDecodeError, WindowRequest, WindowRowSource, EMPTY_WINDOW, HEADER_LEN, ROW_KEY_LEN,
    WINDOW_FORMAT_VERSION,
};
use document_format::{CellValue, Document, Row, RowId, Sheet, SheetId};
use schema_engine::compile::plan::ColumnValidator;
use schema_engine::{SchemaEngine, SchemaEngineApi, ValidationOptions};

// ---------------------------------------------------------------------------
// 標本と索引を組み立てる補助
// ---------------------------------------------------------------------------

/// 窓の符号化に要る 3 つ（表示の順序・違反の索引・文書を持つ標本）。
struct Prepared {
    sample: Sample,
    order: RowOrder,
    index: ViolationIndex,
}

impl Prepared {
    /// 標本のデータシート。
    fn sheet(&self) -> &Sheet {
        self.sample
            .document()
            .sheet_by_id(self.sample.sheet())
            .expect("標本のシートは文書にある")
    }

    /// 窓が運ぶ列の数（宣言の列数。tasks.md 5.1 の依存に 2.3 が無い理由は
    /// `src/transport/mod.rs` のモジュール docs を参照）。
    fn columns(&self) -> usize {
        self.sample.column_count()
    }

    /// 標本を組み立て、全件検証の報告から索引を作り、`spec` で順序を導出する。
    ///
    /// 検証は上限を設けずに走らせる（標本は 64 行 × 13 列の規模であり、保持は切られない）。
    fn new(rows: usize, columns: usize, ratio: f64, spec: &ViewSpec) -> Self {
        let sample = sample(&SampleOptions::new(rows, columns).with_ratio(ratio));
        let report = SchemaEngine::new().validate_sheet(
            sample.document(),
            sample.sheet(),
            &sample.compiled(),
            &ValidationOptions::default(),
        );
        let mut order = RowOrder::default();
        order.recompute(sample.document(), sample.sheet(), spec);
        let index = ViolationIndex::build(&report, &order);
        Self {
            sample,
            order,
            index,
        }
    }
}

/// 標本の行を、識別子の生 16 バイトを鍵とする表にする（復号した行から値へ戻るため）。
fn rows_by_key(sample: &Sample) -> HashMap<[u8; ROW_KEY_LEN], &Row> {
    sample
        .document()
        .sheet_by_id(sample.sheet())
        .expect("標本のシートは文書にある")
        .rows()
        .iter()
        .map(|row| (row_key(row.id()), row))
        .collect()
}

/// `RowId` の**生 16 バイト**（ULID の正準バイト列 = `u128` のビッグエンディアン）。
///
/// 窓は識別子をこの形で運ぶ（design.md「Data Models / 窓の二進形式」）。`RowId` は新しい
/// 型であり、その生バイトを取る経路は `ulid` の型を名指さずに書ける（下の 1 行）。
fn row_key(row: RowId) -> [u8; ROW_KEY_LEN] {
    row.ulid().to_bytes()
}

/// 標本のシートを走査して行の値を引く [`WindowRowSource`]。
///
/// **テスト専用の素朴な実装である** — 引くたびにシートの行を線形に走査する。本番の
/// 呼び出し側（5.2 の `GridSession`）は行の索引を保つ側であり、本テストが固定するのは
/// 「符号化が**行の数の分しか**問い合わせないこと」である（問い合わせの回数は
/// [`CountingSource`] が数える）。
struct SheetSource<'a> {
    sheet: &'a Sheet,
}

impl WindowRowSource for SheetSource<'_> {
    fn values(&self, row: RowId) -> Option<&[CellValue]> {
        self.sheet
            .rows()
            .iter()
            .find(|candidate| candidate.id() == row)
            .map(Row::values)
    }
}

/// 窓の行だけを許す [`WindowRowSource`]（**窓の外の行を引いたら panic する**）。
///
/// 費用の形の観測に使う。総当たりで行を引く実装（10 万行を走査する実装）はここで落ちる —
/// 「窓の行数だけを引く」ことを、回数ではなく**引ける範囲**で固定する。
struct WindowOnlySource<'a> {
    sheet: &'a Sheet,
    allowed: Vec<RowId>,
    lookups: Cell<usize>,
}

impl WindowRowSource for WindowOnlySource<'_> {
    fn values(&self, row: RowId) -> Option<&[CellValue]> {
        assert!(
            self.allowed.contains(&row),
            "窓の外の行 ({row}) を引いた（費用が窓の行数に比例していない）"
        );
        self.lookups.set(self.lookups.get() + 1);
        self.sheet
            .rows()
            .iter()
            .find(|candidate| candidate.id() == row)
            .map(Row::values)
    }
}

/// 1 行も知らない [`WindowRowSource`]（未知の行の検査）。
struct EmptySource;

impl WindowRowSource for EmptySource {
    fn values(&self, _row: RowId) -> Option<&[CellValue]> {
        None
    }
}

/// 現在の世代で `prepared` の窓を符号化する（成功を前提とする）。
fn encode(prepared: &Prepared, codec: &WindowCodec, span: RowSpan) -> Vec<u8> {
    let source = SheetSource {
        sheet: prepared.sheet(),
    };
    let request = WindowRequest::new(codec.generation(), span);
    codec
        .encode(
            &prepared.order,
            &prepared.index,
            prepared.columns(),
            &source,
            &request,
        )
        .expect("窓を符号化できる")
}

/// 標本の値の変種が、窓の変種の札の値と一致するか（**期待値はテスト側にも書く**）。
///
/// 実装の `match` を写すのではなく、**wire ABI の数値をここに独立に書く** — 表の数値と
/// 実装が食い違えば、この検査が落ちる（同じ関数を呼ぶ検査にしない）。
fn expected_tag(value: &CellValue) -> u8 {
    match value {
        CellValue::Null => 0,
        CellValue::Bool(_) => 1,
        CellValue::Int(_) => 2,
        CellValue::Float(_) => 3,
        CellValue::Decimal(_) => 4,
        CellValue::Text(_) => 5,
        CellValue::Nested(_) => 6,
        CellValue::Attachment(_) => 7,
    }
}

/// `haystack` が `needle` を部分列として含むか（窓に数値が載っていないことの検査に使う）。
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}

/// その列が宣言するオブジェクトのフィールド名（入れ子でなければ空）。
fn object_fields(sample: &Sample, column: usize) -> Vec<String> {
    let schema = sample.compiled();
    let Some(validator) = schema.validator(ColumnIndex::new(column)) else {
        return Vec::new();
    };
    match validator {
        ColumnValidator::Object { fields } => {
            fields.iter().map(|field| field.name().to_owned()).collect()
        }
        _ => Vec::new(),
    }
}

/// 絞り込みも並べ替えも指定しない表示の指定。
fn no_view() -> ViewSpec {
    ViewSpec {
        sort: Vec::new(),
        filters: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// 頭の欄（版・世代・開始序数・行数・列数）
// ---------------------------------------------------------------------------

/// 頭の欄が復号から読め、**生バイトの位置と幅**が公表された配置と一致する。
///
/// 位置とエンディアンを生バイトから直接読むので、実装が別の位置へ書いてもこの検査は落ちる
/// （復号だけを見る検査では、符号化と復号が同じ誤りを共有していても緑になる）。
#[test]
fn the_header_carries_the_version_generation_start_row_count_and_column_count() {
    assert_eq!(1, WINDOW_FORMAT_VERSION, "版が変わっている");
    assert_eq!(33, HEADER_LEN, "頭の幅が変わっている");
    assert_eq!(16, ROW_KEY_LEN, "行の識別子の幅が変わっている");

    let prepared = Prepared::new(64, 13, 0.05, &no_view());
    let codec = WindowCodec::new(Generation::new(7));
    let span = RowSpan::new(RowOrdinal::new(8), 16);
    let bytes = encode(&prepared, &codec, span);

    // 生バイトから読む（位置とエンディアンの固定）。
    assert_eq!(WINDOW_FORMAT_VERSION, bytes[0], "版の位置が違う");
    assert_eq!(
        7,
        u64::from_le_bytes(bytes[1..9].try_into().expect("8 バイト")),
        "世代の位置かエンディアンが違う"
    );
    assert_eq!(
        8,
        u64::from_le_bytes(bytes[9..17].try_into().expect("8 バイト")),
        "開始序数の位置かエンディアンが違う"
    );
    assert_eq!(
        16,
        u64::from_le_bytes(bytes[17..25].try_into().expect("8 バイト")),
        "行数の位置かエンディアンが違う"
    );
    assert_eq!(
        13,
        u64::from_le_bytes(bytes[25..33].try_into().expect("8 バイト")),
        "列数の位置かエンディアンが違う"
    );
    assert_eq!(
        WINDOW_FORMAT_VERSION,
        bytes[0],
        "版のバイトが定数と食い違う"
    );

    // 復号も同じ欄を晒す。
    let decoded = decode_window(&bytes).expect("復号できる");
    assert_eq!(WINDOW_FORMAT_VERSION, decoded.version());
    assert_eq!(Generation::new(7), decoded.generation());
    assert_eq!(span.start(), decoded.start());
    assert_eq!(span.count(), decoded.row_count());
    assert_eq!(prepared.columns(), decoded.columns());
    assert_eq!(
        prepared.order.len(),
        prepared.sample.rows(),
        "可視行数が行数と食い違う（前提）"
    );
}

// ---------------------------------------------------------------------------
// 往復（符号化と復号）
// ---------------------------------------------------------------------------

/// 実在の文書にまたがる区間を符号化し、復号した**行の識別子・表示文字列・変種の札・
/// 違反の札**が入力と一致する（要件 1.1, 1.2, 4.1, 4.5）。
///
/// 期待値はすべて入力側（標本の文書・表示の順序・違反の索引）から導く。表示文字列の期待値
/// だけは `view` 層の [`display_text`] であり、これは「5.1 が表示文字列の写しを作っていない」
/// ことの検査でもある（写しを作れば、絞り込みが見る文字列と窓が運ぶ文字列が食い違いうる）。
#[test]
fn a_window_round_trips_forward_in_a_single_pass() {
    let prepared = Prepared::new(64, 13, 0.05, &no_view());
    let codec = WindowCodec::new(Generation::new(1));
    let span = RowSpan::new(RowOrdinal::new(0), 16);
    let bytes = encode(&prepared, &codec, span);
    let decoded = decode_window(&bytes).expect("復号できる");

    assert_eq!(span.count(), decoded.row_count(), "行数が違う");
    assert_eq!(
        span.count(),
        prepared.order.span(span).len(),
        "窓の行数が順序の切片と違う（前提）"
    );

    let rows = rows_by_key(&prepared.sample);
    let mut violated_cells = 0usize;
    let mut nested_cells = 0usize;
    let mut variants: Vec<u8> = Vec::new();

    for (offset, decoded_row) in decoded.rows().iter().enumerate() {
        let ordinal = RowOrdinal::new(span.start().get() + offset);
        let expected_row = prepared
            .order
            .span(span)
            .get(offset)
            .copied()
            .expect("窓の行がある");
        assert_eq!(
            row_key(expected_row),
            decoded_row.key(),
            "窓の {offset} 行目の識別子が違う"
        );
        assert_eq!(
            Some(expected_row),
            prepared.order.row_at(ordinal),
            "窓の行が表示の序数と対応しない"
        );

        let source = rows.get(&decoded_row.key()).expect("標本の行がある");
        assert_eq!(prepared.columns(), decoded_row.cells().len(), "セル数が違う");

        for (column, cell) in decoded_row.cells().iter().enumerate() {
            let value = source
                .values()
                .get(column)
                .expect("標本の行はすべての列に値を持つ（前提）");
            assert_eq!(
                display_text(value).as_ref(),
                cell.text(),
                "{offset} 行 {column} 列の表示文字列が違う"
            );
            assert_eq!(
                expected_tag(value),
                cell.tag().byte(),
                "{offset} 行 {column} 列の変種の札が違う"
            );
            assert_eq!(
                VariantTag::from_byte(expected_tag(value)),
                Some(cell.tag()),
                "札の往復が閉じない"
            );

            // 違反の札は索引の経路そのものである（空の経路 = セル直下の違反）。
            let expected: Vec<NestedPath> = prepared
                .index
                .row_violations(ordinal)
                .and_then(|entry| entry.cell(ColumnIndex::new(column)))
                .map(|cell| cell.paths().to_vec())
                .unwrap_or_default();
            assert_eq!(expected, cell.marks(), "{offset} 行 {column} 列の違反の札が違う");
            // **違反の有無のバイトが、索引と突き合わせて正しいこと**を確かめる。
            //
            // `assert_eq!(!cell.marks().is_empty(), cell.violated())` は**恒真である**
            // （`DecodedCell::violated` は `!marks.is_empty()` として定義されている）ため、
            // ここでは索引から導いた期待値と比べる（どちらも「札がある ⇔ 違反がある」を
            // 独立に表する）。
            assert_eq!(
                !expected.is_empty(),
                cell.violated(),
                "{offset} 行 {column} 列の違反の有無のバイトが索引と食い違う"
            );
            if cell.violated() {
                violated_cells += 1;
            }
            if matches!(value, CellValue::Nested(_)) {
                nested_cells += 1;
            }
            if !variants.contains(&cell.tag().byte()) {
                variants.push(cell.tag().byte());
            }
        }
    }

    // 前提: この窓には違反したセルと入れ子のセルが実際に入っている（空の検査にしない）。
    assert!(
        violated_cells > 0,
        "この窓に違反したセルが 1 つも無い（前提が崩れている）"
    );
    assert!(
        nested_cells > 0,
        "この窓に入れ子のセルが 1 つも無い（前提が崩れている）"
    );
    assert!(
        variants.len() >= 5,
        "この窓の変種が少なすぎる（前提が崩れている）: {variants:?}"
    );
}

// ---------------------------------------------------------------------------
// 64 ビット整数と 10 進数（タスク 5.1 の強調点）
// ---------------------------------------------------------------------------

/// 手組みの文書（列 3 本）を返す。
///
/// 標本は**適合する値**しか持たないため、`i64` の全域や逐語の 10 進数のように標本が
/// 作らない値は、行の値を直接置ける公開経路（`add_row` + `set_row_values`）で組み立てる。
fn hand_built(rows: &[Vec<CellValue>], columns: usize) -> (Document, SheetId, Vec<RowId>) {
    let mut document = Document::new();
    let sheet = document.add_sheet("手組み");
    let names: Vec<String> = (0..columns).map(|index| format!("列{index}")).collect();
    document
        .set_sheet_columns(sheet, names)
        .expect("手組みのシートは文書にある");
    let mut ids = Vec::new();
    for values in rows {
        let id = document.add_row(sheet).expect("行を足せる");
        document
            .set_row_values(sheet, id, values.clone())
            .expect("値は対象シートの行に書ける");
        ids.push(id);
    }
    (document, sheet, ids)
}

/// 手組みの文書を表示の順序へ写し、空の索引と組にする（違反を持たない文書である）。
fn ordered(document: &Document, sheet: SheetId) -> RowOrder {
    let mut order = RowOrder::default();
    order.recompute(document, sheet, &no_view());
    order
}

/// `i64::MAX` と長い 10 進数が**文字列のまま**保たれる（要件 11.6）。
///
/// 3 つの側面を同時に見る:
///
/// 1. 復号した文字列が**元の文字列そのもの**である（桁落ちも正規化も起きない）
/// 2. 桁が **UTF-8 として窓に載っている**（数値の欄ではなく文字列の本体として運ばれる）
/// 3. その値の **64 ビット表現が窓のどこにも現れない**（数値として運ばれていない）
///
/// さらに**負の対照**として、f64 へ落とすと同じ文字列には戻らないことを表明する —
/// この 2 つの値は文字列で運ばなければ復元できない（検査に歯があることの表明）。
#[test]
fn int_and_decimal_survive_the_round_trip_as_strings() {
    const LONG_DECIMAL: &str = "12345678901234567890.123456789";
    const VERBATIM_DECIMAL: &str = "007.50";

    let values = vec![
        CellValue::Int(i64::MAX),
        CellValue::Decimal(LONG_DECIMAL.to_owned()),
        CellValue::Decimal(VERBATIM_DECIMAL.to_owned()),
        CellValue::Int(i64::MIN),
    ];
    let (document, sheet, _ids) = hand_built(&[values.clone()], values.len());
    let order = ordered(&document, sheet);
    let index = ViolationIndex::default();
    let codec = WindowCodec::new(Generation::new(3));
    let span = RowSpan::new(RowOrdinal::new(0), 1);
    let source = SheetSource {
        sheet: document.sheet_by_id(sheet).expect("シートがある"),
    };
    let bytes = codec
        .encode(
            &order,
            &index,
            values.len(),
            &source,
            &WindowRequest::new(codec.generation(), span),
        )
        .expect("窓を符号化できる");

    let decoded = decode_window(&bytes).expect("復号できる");
    assert_eq!(1, decoded.row_count(), "行数が違う");
    let cells = decoded.rows()[0].cells();
    assert_eq!(values.len(), cells.len(), "セル数が違う");

    for (column, value) in values.iter().enumerate() {
        let expected = display_text(value);
        assert_eq!(
            expected.as_ref(),
            cells[column].text(),
            "{column} 列の表示文字列が元と違う"
        );
        assert_eq!(
            expected_tag(value),
            cells[column].tag().byte(),
            "{column} 列の札が違う"
        );
        // 1. 桁は UTF-8 の本体として窓に載っている。
        assert!(
            contains(&bytes, expected.as_bytes()),
            "{column} 列の桁が窓に UTF-8 として載っていない"
        );
    }

    // 2. 復号した文字列は**元の値そのもの**である（取り違えの余地が無い形で確かめる）。
    assert_eq!(
        i64::MAX.to_string(),
        cells[0].text(),
        "i64::MAX が文字列として保たれていない"
    );
    assert_eq!(LONG_DECIMAL, cells[1].text(), "長い 10 進数が変わっている");
    assert_eq!(
        VERBATIM_DECIMAL, cells[2].text(),
        "10 進数が正規化されている（逐語で往復しない）"
    );
    assert_eq!(i64::MIN.to_string(), cells[3].text(), "i64::MIN が変わっている");

    // 3. 数値として運ばれていない: 値の 64 ビット表現が窓のどこにも現れない。
    for value in [i64::MAX, i64::MIN] {
        assert!(
            !contains(&bytes, &value.to_le_bytes()),
            "{value} がリトルエンディアンの数値として窓に載っている"
        );
        assert!(
            !contains(&bytes, &value.to_be_bytes()),
            "{value} がビッグエンディアンの数値として窓に載っている"
        );
    }

    // 負の対照: f64 へ落とすと元の文字列には戻らない（文字列で運ぶ必要がある値である）。
    for (column, text) in [(0usize, i64::MAX.to_string()), (1, LONG_DECIMAL.to_owned())] {
        let as_float: f64 = text.parse().expect("数として読める");
        assert_ne!(
            text,
            format!("{as_float}"),
            "{column} 列の値は f64 で往復できてしまう（検査の前提が崩れている）"
        );
    }
}

// ---------------------------------------------------------------------------
// 入れ子のセル（要約と、内側の位置の札）
// ---------------------------------------------------------------------------

/// 入れ子のセルは**要約**（要素数）を運び、**構造そのものを運ばない**（要件 5.6）。
///
/// 要約の文字列が [`display_text`] の規則そのものであること（写しを作っていないこと）と、
/// 内側の値の中身が窓のどこにも現れないことを、実際のバイト列の探索で確かめる。
#[test]
fn a_nested_cell_carries_a_summary_and_not_the_structure() {
    let prepared = Prepared::new(64, 13, 0.0, &no_view());
    let codec = WindowCodec::new(Generation::new(1));
    // 届け先（入れ子のオブジェクト）の列を探す。
    let nested_column = (0..prepared.columns())
        .find(|column| !object_fields(&prepared.sample, *column).is_empty())
        .expect("入れ子のオブジェクトの列がある（前提）");
    let span = RowSpan::new(RowOrdinal::new(0), 4);
    let bytes = encode(&prepared, &codec, span);
    let decoded = decode_window(&bytes).expect("復号できる");

    let rows = rows_by_key(&prepared.sample);
    let mut checked = 0usize;
    for (offset, decoded_row) in decoded.rows().iter().enumerate() {
        let source = rows.get(&decoded_row.key()).expect("標本の行がある");
        let value = &source.values()[nested_column];
        let CellValue::Nested(inner) = value else {
            panic!("{nested_column} 列が入れ子でない（前提が崩れている）");
        };
        let cell = &decoded_row.cells()[nested_column];
        assert_eq!(
            display_text(value).as_ref(),
            cell.text(),
            "入れ子の要約が表示文字列の規則と違う"
        );

        // 要約は要素数であり、構造ではない。
        let expected_summary = match inner {
            document_format::NestedValue::Object(entries) => format!("{}項目", entries.len()),
            document_format::NestedValue::Array(items) => format!("{}要素", items.len()),
        };
        assert_eq!(
            expected_summary,
            cell.text(),
            "{offset} 行の入れ子の要約が要素数と違う"
        );

        // 内側の**値の中身**は窓のどこにも現れない（構造を運んでいない）。
        let inner_values: Vec<&CellValue> = match inner {
            document_format::NestedValue::Object(entries) => {
                entries.iter().map(|(_, value)| value).collect()
            }
            document_format::NestedValue::Array(items) => items.iter().collect(),
        };
        for inner_value in inner_values {
            let text = display_text(inner_value);
            if text.is_empty() {
                continue;
            }
            assert!(
                !contains(&bytes, text.as_bytes()),
                "{offset} 行の内側の値 {text:?} が窓に載っている（構造を運んでいる）"
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "入れ子のセルを 1 つも検査していない");
}

/// 違反した入れ子のセルは、**内側のどの位置が違反しているかの札**を運ぶ（要件 4.5）。
///
/// 位置の正しさは、索引の写しではなく**宣言が名指すフィールド**と突き合わせて確かめる —
/// 標本の `届け先` は内側の 1 つのフィールド（郵便番号）だけを外した値を仕込むため、
/// 札は宣言のその位置を指す。
#[test]
fn a_violated_nested_cell_carries_the_inner_position_mark() {
    let prepared = Prepared::new(64, 13, 0.05, &no_view());
    let codec = WindowCodec::new(Generation::new(1));
    let span = RowSpan::new(RowOrdinal::new(0), 16);
    let bytes = encode(&prepared, &codec, span);
    let decoded = decode_window(&bytes).expect("復号できる");

    // 入れ子の違反を持つセルを、索引から探す（窓の中に在ることを前提として表明する）。
    let mut found = 0usize;
    for (offset, decoded_row) in decoded.rows().iter().enumerate() {
        let ordinal = RowOrdinal::new(span.start().get() + offset);
        for (column, cell) in decoded_row.cells().iter().enumerate() {
            if cell.marks().is_empty() {
                continue;
            }
            let fields = object_fields(&prepared.sample, column);
            let inner_marks: Vec<&NestedPath> = cell
                .marks()
                .iter()
                .filter(|mark| !mark.is_empty())
                .collect();
            assert_eq!(
                cell.marks().len(),
                prepared
                    .index
                    .row_violations(ordinal)
                    .and_then(|entry| entry.cell(ColumnIndex::new(column)))
                    .expect("違反したセルは索引に載っている")
                    .paths()
                    .len(),
                "札の数が索引の件数と違う"
            );
            if fields.is_empty() || inner_marks.is_empty() {
                continue;
            }
            // 内側の位置の札は、宣言が名指す最初のフィールドと一致しなければならない。
            for mark in inner_marks {
                let segments = mark.segments();
                assert_eq!(1, segments.len(), "標本の入れ子の違反は 1 段である（前提）");
                assert_eq!(
                    data_grid::NestedPathSegment::Field(fields[0].clone().into()),
                    segments[0],
                    "内側の位置の札が宣言のフィールドと違う"
                );
            }
            found += 1;
        }
    }
    assert!(
        found > 0,
        "この窓に違反した入れ子のセルが 1 つも無い（前提が崩れている）"
    );

    // 前提: 内側の違反を 1 件だけ仕込んだ標本であり、窓の中の違反セルはセル直下と内側の
    // 両方を含む（内側の札だけを見て緑になる検査にしない）。
    let root_marks = decoded
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .filter(|cell| cell.marks().iter().any(|mark| mark.is_empty()))
        .count();
    assert!(
        root_marks > 0,
        "セル直下の違反がこの窓に 1 つも無い（前提が崩れている）"
    );
}

// ---------------------------------------------------------------------------
// 冪等性・決定性・世代
// ---------------------------------------------------------------------------

/// 同じ世代・同じ区間の 2 回の符号化は**バイト単位で一致する**（design.md の Idempotency 句）。
///
/// 決定性は「同じ入力から同じ出力」であり、冪等性は「同じ要求に対して同じ結果」である。
/// ここでは両方を、符号化器を使い回した場合と、同じ世代の別の符号化器で符号化した場合の
/// 双方で見る（内部に要求ごとの状態を溜めていないことの検査でもある）。
#[test]
fn the_same_generation_and_span_encode_to_the_same_bytes() {
    let prepared = Prepared::new(64, 13, 0.05, &no_view());
    let codec = WindowCodec::new(Generation::new(11));
    let span = RowSpan::new(RowOrdinal::new(4), 24);

    let first = encode(&prepared, &codec, span);
    let second = encode(&prepared, &codec, span);
    assert_eq!(first, second, "同じ要求の 2 回の符号化が一致しない");
    assert!(!first.is_empty(), "窓が空になっている（前提が崩れている）");

    let another = WindowCodec::new(Generation::new(11));
    let third = encode(&prepared, &another, span);
    assert_eq!(first, third, "同じ世代の別の符号化器が違う結果を返した");

    // 区間が違えば窓も違う（同じ結果を返す実装を素通りさせない）。
    let other_span = RowSpan::new(RowOrdinal::new(5), 24);
    assert_ne!(
        first,
        encode(&prepared, &codec, other_span),
        "区間を変えても同じ窓が返る"
    );
}

/// 世代が古い要求は**空の窓**を返し、呼び出し側が再要求する（design.md の Batch 契約）。
///
/// 空の窓は失敗と同じ表現である（生バイト経路は封筒を運べないため、design.md は
/// 「失敗と世代違いを空の窓で表す」を 6.3 の要件にしている）。呼び出し側が古さを**先に**
/// 知れるよう、比較の口（`is_stale`）も晒す — 6.3 は診断のためにそれを使える。
#[test]
fn a_stale_generation_yields_the_empty_window() {
    let prepared = Prepared::new(64, 13, 0.05, &no_view());
    let codec = WindowCodec::new(Generation::new(20));
    let span = RowSpan::new(RowOrdinal::new(0), 8);

    // いまの世代の要求は窓になる。
    let fresh = WindowRequest::new(Generation::new(20), span);
    let source = SheetSource {
        sheet: prepared.sheet(),
    };
    assert!(!codec.is_stale(&fresh), "いまの世代が古いと判定された");
    let bytes = codec
        .encode(
            &prepared.order,
            &prepared.index,
            prepared.columns(),
            &source,
            &fresh,
        )
        .expect("窓を符号化できる");
    assert!(!bytes.is_empty(), "いまの世代の窓が空である");

    // 古い世代の要求は空の窓であり、失敗ではない（Err を返さない）。
    let stale = WindowRequest::new(Generation::new(19), span);
    assert!(codec.is_stale(&stale), "古い世代が古いと判定されない");
    let stale_bytes = codec
        .encode(
            &prepared.order,
            &prepared.index,
            prepared.columns(),
            &source,
            &stale,
        )
        .expect("古い世代は失敗ではなく空の窓になる");
    assert_eq!(EMPTY_WINDOW, stale_bytes.as_slice(), "古い世代が空の窓を返さない");
    assert!(stale_bytes.is_empty(), "空の窓の表現が違う");

    // 世代を進めると、進める前の要求が古くなる（5.2 が世代を進める側である）。
    let mut advanced = WindowCodec::new(Generation::new(20));
    advanced.set_generation(Generation::new(21));
    assert_eq!(Generation::new(21), advanced.generation());
    let after = advanced
        .encode(
            &prepared.order,
            &prepared.index,
            prepared.columns(),
            &source,
            &fresh,
        )
        .expect("世代が古くなった要求も失敗しない");
    assert!(after.is_empty(), "世代を進めたのに古い要求が窓になった");

    // 世代は単調に進む（`next` の規則）。
    assert_eq!(Generation::new(21), Generation::new(20).next());
    assert_eq!(0, Generation::new(0).get());
    // 最初の世代は `Generation::FIRST` であり、それ自身の要求は古くない。
    let fresh_codec = WindowCodec::new(Generation::FIRST);
    assert_eq!(0, fresh_codec.generation().get(), "最初の世代が 0 でない");
    assert!(
        !fresh_codec.is_stale(&WindowRequest::new(Generation::FIRST, span)),
        "最初の世代の要求が古いと判定された"
    );
}

/// 空の窓の表現は**長さ 0 のバイト列**であり、**行 0 の窓（頭を持つ 33 バイト）と区別できる**。
///
/// この区別が要るのは、フロントエンドが「端に達した（行が無い）」と「要求が通らなかった
/// （読み込み中のまま再試行する）」を別に扱うためである（design.md の Error Categories）。
#[test]
fn the_empty_window_is_a_zero_length_byte_string_and_a_zero_row_window_is_not_it() {
    assert!(EMPTY_WINDOW.is_empty(), "空の窓が空でない");
    assert!(!decode_window(EMPTY_WINDOW).is_ok(), "空の窓が窓として復号できてしまう");

    let prepared = Prepared::new(8, 3, 0.0, &no_view());
    let codec = WindowCodec::new(Generation::new(1));
    let source = SheetSource {
        sheet: prepared.sheet(),
    };

    // 可視行の先（開始序数 = 可視行数）を求めた要求は、**行 0 の窓**になる（切り落としの帰結）。
    let beyond = RowSpan::new(RowOrdinal::new(8), 4);
    let bytes = codec
        .encode(
            &prepared.order,
            &prepared.index,
            prepared.columns(),
            &source,
            &WindowRequest::new(codec.generation(), beyond),
        )
        .expect("可視行の先の要求は失敗ではない");
    assert!(
        !bytes.is_empty(),
        "行 0 の窓が空の窓（長さ 0）になっている — 2 つが区別できない"
    );
    assert_eq!(HEADER_LEN, bytes.len(), "行 0 の窓が頭だけになっていない");
    let decoded = decode_window(&bytes).expect("復号できる");
    assert_eq!(0, decoded.row_count(), "行 0 の窓の行数が 0 でない");
    assert!(decoded.rows().is_empty(), "行 0 の窓に行がある");
    assert_eq!(
        beyond.start(),
        decoded.start(),
        "行 0 の窓の開始序数が要求と違う"
    );
    assert_eq!(
        Generation::new(1),
        decoded.generation(),
        "行 0 の窓の世代が違う"
    );
}

// ---------------------------------------------------------------------------
// 範囲の検査
// ---------------------------------------------------------------------------

/// 可視行数の外を求めた要求は [`GridError::SpanOutOfRange`] であり、**文書を 1 行も変えない**。
///
/// 誤りの規則（design.md「Error Handling」の「範囲外の窓の要求」）は、① 失敗腕で `GridError`
/// を返す、② 画面が再要求する、である。panic しないことも同時に見る。
#[test]
fn a_span_past_the_visible_rows_is_rejected_without_touching_the_document() {
    let prepared = Prepared::new(8, 3, 0.0, &no_view());
    let codec = WindowCodec::new(Generation::new(1));
    let source = SheetSource {
        sheet: prepared.sheet(),
    };
    let before = prepared.sample.row_values();

    // 開始序数が可視行数より後ろ（9 > 8）は、この世代の表示と整合しない要求である。
    let span = RowSpan::new(RowOrdinal::new(9), 4);
    assert!(
        span.start().get() > prepared.order.len(),
        "前提: 開始序数が可視行数の外にある"
    );
    let outcome = codec.encode(
        &prepared.order,
        &prepared.index,
        prepared.columns(),
        &source,
        &WindowRequest::new(codec.generation(), span),
    );
    assert_eq!(
        Err(GridError::SpanOutOfRange {
            span,
            visible: prepared.order.len(),
        }),
        outcome,
        "範囲外の要求が SpanOutOfRange にならない"
    );

    // 文書は 1 行も変わっていない（前後で値の並びを比べる。符号化は `Document` を
    // 受け取らないため型の上でも書き換えられないが、実際に変わらないことを観測する）。
    assert_eq!(
        before,
        prepared.sample.row_values(),
        "失敗した要求が文書を変えた"
    );
}

/// 世代の不一致は**範囲の検査より先**に見る（古い世代の要求は、その世代の表示では妥当だった
/// 範囲を運びうる）。
///
/// 順序が問題になるのは、**両方の条件を同時に満たす要求**である。世代の不一致を範囲の検査の
/// 後に見る実装は、古い世代の要求を [`GridError::SpanOutOfRange`] として返す — 呼び出し側
/// （6.3 のコマンド）はそれを「再要求」ではなく「誤り」として扱うため、**再要求の経路が
/// 塞がる**（design.md の Batch 契約が「世代が古い要求は空の窓を返し、呼び出し側が再要求
/// する」と定める理由そのものである）。
///
/// したがってこの検査は、**古い世代かつ範囲外**という要求 1 つで両方の条件を同時に立て、
/// 空の窓が返ることを見る（どちらか片方だけを立てる検査では順序を固定できない）。
#[test]
fn a_stale_generation_wins_over_an_out_of_range_span() {
    let prepared = Prepared::new(8, 3, 0.0, &no_view());
    let codec = WindowCodec::new(Generation::new(30));
    let source = SheetSource {
        sheet: prepared.sheet(),
    };

    // 古い世代かつ範囲外（開始序数 9 > 可視行数 8）。両方の条件を同時に立てる。
    let span = RowSpan::new(RowOrdinal::new(9), 4);
    let stale = WindowRequest::new(Generation::new(29), span);
    assert!(codec.is_stale(&stale), "前提: この要求は古い世代である");
    assert!(
        span.start().get() > prepared.order.len(),
        "前提: この要求は範囲外である"
    );

    let outcome = codec.encode(
        &prepared.order,
        &prepared.index,
        prepared.columns(),
        &source,
        &stale,
    );
    assert_eq!(
        Ok(EMPTY_WINDOW.to_vec()),
        outcome,
        "古い世代かつ範囲外の要求が空の窓でなく SpanOutOfRange になった（世代の検査が範囲の検査より後ろにある）"
    );

    // 対照: いまの世代で同じ範囲外の要求をすると、こちらは誤りである（範囲の検査そのものは
    // 生きている。上の検査が「範囲を見なくなった」ことの検査になっていないことの表明）。
    let fresh_codec = WindowCodec::new(Generation::new(30));
    assert_eq!(
        Err(GridError::SpanOutOfRange {
            span,
            visible: prepared.order.len(),
        }),
        fresh_codec.encode(
            &prepared.order,
            &prepared.index,
            prepared.columns(),
            &source,
            &WindowRequest::new(Generation::new(30), span),
        ),
        "いまの世代の範囲外の要求が誤りにならない"
    );
}

/// 表示範囲の端に掛かる要求は**切り落とす**（`RowOrder::span` の規約）。
///
/// 窓の要求は表示範囲の端で必ず短くなる。開始序数が可視行の内側であり終端が外へ出る要求は
/// 失敗ではなく、可視行の最後までを運ぶ窓になる（design.md の `RowOrder::span` の規約が
/// 「窓の符号化 5.1 と窓の記憶 7.3 が依存する」と名指す振る舞い）。
#[test]
fn a_span_crossing_the_end_is_clamped() {
    let prepared = Prepared::new(8, 3, 0.0, &no_view());
    let codec = WindowCodec::new(Generation::new(1));
    let source = SheetSource {
        sheet: prepared.sheet(),
    };

    let span = RowSpan::new(RowOrdinal::new(6), 10);
    let bytes = codec
        .encode(
            &prepared.order,
            &prepared.index,
            prepared.columns(),
            &source,
            &WindowRequest::new(codec.generation(), span),
        )
        .expect("端に掛かる要求は失敗ではない");
    let decoded = decode_window(&bytes).expect("復号できる");
    assert_eq!(2, decoded.row_count(), "切り落とした行数が違う");
    assert_eq!(span.start(), decoded.start(), "開始序数が要求と違う");
    assert_eq!(
        prepared.order.span(span),
        decoded
            .rows()
            .iter()
            .map(|row| row_key_to_id(prepared.sample.document(), prepared.sample.sheet(), row.key()))
            .collect::<Vec<_>>(),
        "切り落とした窓の行が順序の切片と違う"
    );
}

/// 窓の中の行を、標本の文書の行識別子へ戻す（識別子は発行のたびに変わるため、比較は
/// 標本が公開する並びとの突き合わせで行う）。
fn row_key_to_id(document: &Document, sheet: SheetId, key: [u8; ROW_KEY_LEN]) -> RowId {
    document
        .sheet_by_id(sheet)
        .expect("シートがある")
        .rows()
        .iter()
        .map(Row::id)
        .find(|row| row_key(*row) == key)
        .expect("窓の行は標本の行である")
}

// ---------------------------------------------------------------------------
// 壊れた窓・未知の版
// ---------------------------------------------------------------------------

/// 未知の版と切り詰められた窓を **panic せずに** 拒む。
///
/// 行とセルの長さが**直前の**長さの欄で決まるため、途中で切れた入力は必ず「長さが足りない」
/// として現れる — 頭が宣言する行数に足りない接頭辞はすべて拒まれる。
///
/// **この検査が固定しないもの**: 後方参照の不在そのものである（接頭辞の長さが `>= 8` の
/// 位置を指す後方参照は、その接頭辞の中で完結しうる）。「1 回の走査」の根拠は実装の形
/// （`Cursor` が前方向のみ）であり、この検査ではない（`src/transport/mod.rs` のモジュール docs）。
#[test]
fn an_unknown_version_and_a_truncated_window_are_rejected_without_panicking() {
    let prepared = Prepared::new(8, 3, 0.05, &no_view());
    let codec = WindowCodec::new(Generation::new(1));
    let bytes = encode(&prepared, &codec, RowSpan::new(RowOrdinal::new(0), 4));
    decode_window(&bytes).expect("完全な窓は復号できる");

    // 未知の版。
    let mut unknown = bytes.clone();
    unknown[0] = WINDOW_FORMAT_VERSION + 1;
    assert_eq!(
        Err(WindowDecodeError::UnknownVersion(WINDOW_FORMAT_VERSION + 1)),
        decode_window(&unknown),
        "未知の版が拒まれない"
    );

    // すべての**真の接頭辞**（完全な窓より短いもの）が拒まれる。
    for length in 0..bytes.len() {
        let prefix = &bytes[..length];
        assert!(
            decode_window(prefix).is_err(),
            "長さ {length} の接頭辞が窓として復号できてしまう"
        );
    }

    // 末尾に余分なバイトがある窓も拒まれる（余りを黙って捨てない）。
    let mut extended = bytes.clone();
    extended.push(0);
    assert_eq!(
        Err(WindowDecodeError::TrailingBytes),
        decode_window(&extended),
        "余分なバイトを持つ窓が復号できてしまう"
    );
}

// ---------------------------------------------------------------------------
// 配置の独立な検査（テスト側がバイト列を手で組み立てる）
// ---------------------------------------------------------------------------

/// テスト側が手で組み立てた**違反ありの入れ子のセル 1 つ**を持つ窓（1 行 1 列）。
///
/// **これは実装の写しではない** — 本関数が返すバイト列は、モジュール docs の配置の表だけを
/// 頼りにテスト側で組み立てたものである。したがって復号がこれを受け入れて期待どおりの値を
/// 返せば、**配置の表と実装が一致している**ことの独立な証拠になる（符号化と復号が同じ誤りを
/// 共有していても、この検査は落ちる）。返す各部の位置も併せて返し、誤りの検査がそこを狙える
/// ようにする。
struct HandBuilt {
    bytes: Vec<u8>,
}

impl HandBuilt {
    /// 版 1・世代 5・開始序数 2・1 行 1 列の窓を組み立てる。
    ///
    /// セルは `Text("x")` であり、違反の札は `[Field("a"), Index(7)]` の 1 つである。
    fn new() -> Self {
        let mut bytes = Vec::new();
        bytes.push(WINDOW_FORMAT_VERSION);
        bytes.extend_from_slice(&5u64.to_le_bytes()); // 世代
        bytes.extend_from_slice(&2u64.to_le_bytes()); // 開始序数
        bytes.extend_from_slice(&1u64.to_le_bytes()); // 行数
        bytes.extend_from_slice(&1u64.to_le_bytes()); // 列数
        assert_eq!(HEADER_LEN, bytes.len(), "頭の幅が表と違う");
        bytes.extend_from_slice(&[0xAB; ROW_KEY_LEN]); // 行の識別子の生バイト
        bytes.push(VariantTag::TEXT.byte()); // 変種の札
        bytes.push(1); // 違反の有無
        bytes.extend_from_slice(&1u64.to_le_bytes()); // 札の数
        bytes.extend_from_slice(&2u64.to_le_bytes()); // 段の数
        bytes.push(0); // 段: Field
        bytes.extend_from_slice(&1u64.to_le_bytes()); // 名前の長さ
        bytes.push(b'a'); // 名前
        bytes.push(1); // 段: Index
        bytes.extend_from_slice(&7u64.to_le_bytes()); // 添字
        bytes.extend_from_slice(&1u64.to_le_bytes()); // 表示文字列の長さ
        bytes.push(b'x'); // 本体
        Self { bytes }
    }
}

/// 手で組み立てた窓が復号でき、すべての欄が**表のとおりに**読める。
///
/// 併せて [`VariantTag::ALL`] が表の全体であること（0..=7 が隙間なく並び、
/// [`VariantTag::from_byte`] と一致すること）を固定する — 表に値を足して `ALL` を直し忘れる
/// 状態と、`from_byte` だけを直す状態の双方が落ちる。
#[test]
fn the_documented_byte_layout_decodes_into_the_documented_fields() {
    for (expected, tag) in VariantTag::ALL.iter().enumerate() {
        assert_eq!(
            expected as u8,
            tag.byte(),
            "表の並びと札の値が食い違う（wire ABI を動かしてはいけない）"
        );
        assert_eq!(
            Some(*tag),
            VariantTag::from_byte(tag.byte()),
            "札の往復が閉じない"
        );
    }
    assert_eq!(8, VariantTag::ALL.len(), "表の数が違う");
    assert_eq!(None, VariantTag::from_byte(8), "表の外の値が読めてしまう");

    let hand = HandBuilt::new();
    let decoded = decode_window(&hand.bytes).expect("手組みの窓が復号できる");
    assert_eq!(WINDOW_FORMAT_VERSION, decoded.version());
    assert_eq!(Generation::new(5), decoded.generation());
    assert_eq!(RowOrdinal::new(2), decoded.start());
    assert_eq!(1, decoded.row_count());
    assert_eq!(1, decoded.columns());

    let row = &decoded.rows()[0];
    assert_eq!([0xAB; ROW_KEY_LEN], row.key(), "行の識別子の生バイトが違う");
    let cell = &row.cells()[0];
    assert_eq!(VariantTag::TEXT, cell.tag(), "変種の札が違う");
    assert!(cell.violated(), "違反の有無が読めていない");
    assert_eq!("x", cell.text(), "表示文字列が違う");
    assert_eq!(
        vec![NestedPath::from(&{
            let mut path = schema_engine::ValuePath::root();
            path.push_field("a");
            path.push_index(7);
            path
        })],
        cell.marks(),
        "内側の位置の札が表のとおりに読めない"
    );

    // 違反の有無のバイトだけを落とすと、札は読まれず違反なしになる（表の相対順の検査）。
    let mut without_marks = Vec::new();
    without_marks.extend_from_slice(&hand.bytes[..HEADER_LEN + ROW_KEY_LEN + 1]);
    without_marks.push(0);
    without_marks.extend_from_slice(&1u64.to_le_bytes());
    without_marks.push(b'x');
    let decoded = decode_window(&without_marks).expect("違反なしの窓が復号できる");
    let cell = &decoded.rows()[0].cells()[0];
    assert!(!cell.violated(), "違反なしが違反として読まれた");
    assert!(cell.marks().is_empty(), "違反なしに札が付いている");
    assert_eq!("x", cell.text(), "違反なしの表示文字列が違う");
}

/// 表に無い値は **panic せずに** 拒まれる（変種の札・違反の有無・段の種類・UTF-8）。
///
/// 窓は webview から届くバイト列であり、壊れた入力で落ちてはならない（6.3 が失敗を空の窓へ
/// 写す）。ここでは手組みの窓の**狙った 1 バイト**だけを変えて、各変種が返ることを見る。
#[test]
fn an_unknown_tag_flag_or_segment_kind_is_rejected() {
    let hand = HandBuilt::new();
    let tag_at = HEADER_LEN + ROW_KEY_LEN;
    let flag_at = tag_at + 1;
    // 違反の札の段の種類は、札の数（8）と段の数（8）の後ろ。
    let kind_at = flag_at + 1 + 8 + 8;

    let mutate = |at: usize, value: u8| {
        let mut bytes = hand.bytes.clone();
        bytes[at] = value;
        bytes
    };

    assert_eq!(
        Err(WindowDecodeError::UnknownTag(9)),
        decode_window(&mutate(tag_at, 9)),
        "知らない変種の札が拒まれない"
    );
    assert_eq!(
        Err(WindowDecodeError::UnknownViolationFlag(2)),
        decode_window(&mutate(flag_at, 2)),
        "知らない違反の有無のバイトが拒まれない"
    );
    assert_eq!(
        Err(WindowDecodeError::UnknownSegmentKind(2)),
        decode_window(&mutate(kind_at, 2)),
        "知らない段の種類が拒まれない"
    );

    // 表示文字列の本体が UTF-8 でない（本体は末尾の 1 バイト）。
    let mut bytes = hand.bytes.clone();
    *bytes.last_mut().expect("本体がある") = 0xFF;
    assert_eq!(
        Err(WindowDecodeError::NotUtf8),
        decode_window(&bytes),
        "UTF-8 でない本体が拒まれない"
    );

    // 空の窓は「切り詰め」ではなく「空」として返る（失敗の表現であることの表明）。
    assert_eq!(Err(WindowDecodeError::Empty), decode_window(EMPTY_WINDOW));
}

// ---------------------------------------------------------------------------
// 列の数と行の幅
// ---------------------------------------------------------------------------

/// 窓の列数は**宣言の列数**であり、行の値の数ではない。値を持たない列は**値なし**として運ぶ。
///
/// 列の添字が行の値の数に届かない行は、標本では作れない（標本の行は全列に値を持つ）。
/// したがって手組みの文書で、値の数が宣言より少ない行を置いて確かめる — 値なしの扱いが
/// 行ごとに分かれると、画面の「値なし」と「空文字」の区別が崩れる。
#[test]
fn a_row_shorter_than_the_declared_columns_encodes_as_valueless() {
    let (document, sheet, ids) = hand_built(
        &[
            vec![CellValue::Int(1), CellValue::Text("先頭".to_owned())],
            vec![CellValue::Int(2)],
        ],
        3,
    );
    let order = ordered(&document, sheet);
    assert_eq!(2, order.len(), "可視行数が 2 でない（前提）");
    let codec = WindowCodec::new(Generation::new(1));
    let span = RowSpan::new(RowOrdinal::new(0), 2);
    let source = SheetSource {
        sheet: document.sheet_by_id(sheet).expect("シートがある"),
    };
    let bytes = codec
        .encode(
            &order,
            &ViolationIndex::default(),
            3,
            &source,
            &WindowRequest::new(codec.generation(), span),
        )
        .expect("窓を符号化できる");

    let decoded = decode_window(&bytes).expect("復号できる");
    assert_eq!(3, decoded.columns(), "列数が宣言の列数でない");
    assert_eq!(2, decoded.row_count(), "行数が違う");
    for (offset, row) in decoded.rows().iter().enumerate() {
        assert_eq!(row_key(ids[offset]), row.key(), "{offset} 行目の識別子が違う");
        assert_eq!(3, row.cells().len(), "セル数が列数と違う");
        let missing = &row.cells()[2];
        assert_eq!(VariantTag::NULL, missing.tag(), "値なしの札が違う");
        assert_eq!("", missing.text(), "値なしの表示文字列が空でない");
        assert!(!missing.violated(), "値なしが違反として運ばれている");
        assert!(missing.marks().is_empty(), "値なしに違反の札が付いている");
    }
    assert_eq!(
        VariantTag::NULL,
        decoded.rows()[1].cells()[1].tag(),
        "値の数に届かない列が値なしでない"
    );
}

/// 順序が指す行が文書に無いとき、符号化は [`GridError::UnknownRow`] で止まる。
///
/// 行の順序は文書から導かれるため、これは**表示の状態と文書が食い違っている**場合である
/// （読み込み中の空の窓ではなく、誤りとして返す — 6.3 が生バイト経路の空の窓へ写す）。
#[test]
fn a_row_the_document_does_not_have_is_rejected() {
    let (document, sheet, _ids) = hand_built(&[vec![CellValue::Int(1)]], 1);
    let order = ordered(&document, sheet);
    assert_eq!(1, order.len(), "可視行数が 1 でない（前提）");
    let codec = WindowCodec::new(Generation::new(1));
    let span = RowSpan::new(RowOrdinal::new(0), 1);
    let outcome = codec.encode(
        &order,
        &ViolationIndex::default(),
        1,
        &EmptySource,
        &WindowRequest::new(codec.generation(), span),
    );
    assert_eq!(
        Err(GridError::UnknownRow {
            row: order.row_at(RowOrdinal::new(0)).expect("行がある"),
        }),
        outcome,
        "文書に無い行が Err にならない"
    );
}

// ---------------------------------------------------------------------------
// 表示の順序（並べ替えの下での窓）
// ---------------------------------------------------------------------------

/// 窓は**表示の順序**を運び、文書の並びを運ばない（要件 8.5 の帰結）。
///
/// 並べ替えが効いているとき、窓の行は可視行の序数の順であり、文書の位置とは一致しない。
/// 前提として「この標本の文書の並びは `RowId` の昇順と一致する」ことと「この並べ替えは
/// その並びを実際に変える」ことを表明してから依拠する（tasks.md の Implementation Notes）。
#[test]
fn a_window_follows_the_visible_order_not_the_document_order() {
    let spec = ViewSpec {
        sort: vec![SortKey {
            column: ColumnIndex::new(1),
            descending: true,
        }],
        filters: Vec::new(),
    };
    let prepared = Prepared::new(64, 5, 0.0, &spec);

    // 前提 1: 標本の文書の並びは `RowId` の昇順と一致する。
    let document_ids = prepared.sample.row_ids();
    assert!(
        document_ids.windows(2).all(|pair| pair[0] < pair[1]),
        "標本の文書の並びが `RowId` の昇順でない（前提が崩れている）"
    );
    // 前提 2: この並べ替えは文書の並びを実際に変える。
    let moved = (0..prepared.order.len())
        .filter(|position| {
            prepared.order.row_at(RowOrdinal::new(*position)) != Some(document_ids[*position])
        })
        .count();
    assert!(moved > 0, "並べ替えが文書の並びを 1 つも動かしていない（前提が崩れている）");

    let codec = WindowCodec::new(Generation::new(1));
    let span = RowSpan::new(RowOrdinal::new(8), 16);
    let bytes = encode(&prepared, &codec, span);
    let decoded = decode_window(&bytes).expect("復号できる");

    let rows = rows_by_key(&prepared.sample);
    for (offset, decoded_row) in decoded.rows().iter().enumerate() {
        let expected = prepared.order.span(span)[offset];
        assert_eq!(
            row_key(expected),
            decoded_row.key(),
            "{offset} 行目が表示の順序の行でない"
        );
        // 値もその行のものである（識別子だけを差し替えた窓にしない）。
        let source = rows.get(&decoded_row.key()).expect("標本の行がある");
        for (column, cell) in decoded_row.cells().iter().enumerate() {
            assert_eq!(
                display_text(&source.values()[column]).as_ref(),
                cell.text(),
                "{offset} 行 {column} 列の値が表示の順序の行のものでない"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 費用の形（10 万行のシート）
// ---------------------------------------------------------------------------

/// 10 万行のシートから 40 行の窓を符号化するとき、行の取得の問い合わせは**ちょうど 40 回**で
/// あり、**窓の外の行は 1 度も引かれない**。
///
/// **これは時間の主張ではない**（`verification.md`「速度を証拠にしない。証拠は『呼び出しの
/// 形』で取る」）。数えるのは呼び出しの形である:
///
/// - 符号化は `Document` を**一切受け取らない**。10 万行を走査する経路が型の上に存在しない
///   （受け取るのは表示の順序・違反の索引・列数・[`WindowRowSource`] である）
/// - 行の値は [`WindowRowSource`] に 1 行につき 1 回だけ問い合わせ、**窓の外の行は
///   1 度も引かない**（窓の外を引くと panic する source を差し込んで観測する）
/// - 窓の行数は [`RowOrder::span`] の切片の長さであり、要求の行数を超えない
///
/// # この観測が捉えないもの（正直な限界）
///
/// [`RowOrder::span`] の呼び出し回数は**数えられない**（`RowOrder` に観測の縫い目を足すのは
/// 本タスクの境界の外であり、縫い目を足せば「本番が呼んでいない模擬」を数える危険も生じる）。
/// したがって「可視の並びを丸ごと走査してから窓の行だけを拾う」実装は、**この検査では
/// 区別できない**（値の問い合わせは窓の行だけになるためである）。区別できるのは費用が
/// **シートの行数に比例しない**という構造の主張までであり、それを支えるのは上の 3 点 —
/// とくに「符号化が `Document` を受け取らない」ことと「窓の外の行を引かない」ことである。
#[test]
fn encoding_a_window_asks_the_source_for_exactly_the_window_rows() {
    let sample = sample(&SampleOptions::new(100_000, 30).with_ratio(0.0));
    let mut order = RowOrder::default();
    order.recompute(sample.document(), sample.sheet(), &no_view());
    assert_eq!(
        100_000,
        sample.rows(),
        "標本が 10 万行でない（前提が崩れている）"
    );
    assert_eq!(100_000, order.len(), "可視行数が 10 万でない（前提）");

    let codec = WindowCodec::new(Generation::new(1));
    let sheet = sample
        .document()
        .sheet_by_id(sample.sheet())
        .expect("標本のシートは文書にある");
    // 走査の途中の窓（先頭でも末尾でもない位置を選ぶ）。
    let span = RowSpan::new(RowOrdinal::new(90_000), 40);
    let window_rows: Vec<RowId> = order.span(span).to_vec();
    assert_eq!(40, window_rows.len(), "窓の行数が 40 でない（前提）");
    let source = WindowOnlySource {
        sheet,
        allowed: window_rows.clone(),
        lookups: Cell::new(0),
    };
    let bytes = codec
        .encode(
            &order,
            &ViolationIndex::default(),
            sample.column_count(),
            &source,
            &WindowRequest::new(codec.generation(), span),
        )
        .expect("窓を符号化できる");

    assert_eq!(40, source.lookups.get(), "行の取得の回数が窓の行数と違う");
    let decoded = decode_window(&bytes).expect("復号できる");
    assert_eq!(40, decoded.row_count(), "窓の行数が違う");
    assert_eq!(30, decoded.columns(), "窓の列数が違う");
    assert_eq!(span.start(), decoded.start(), "開始序数が違う");
    assert_eq!(
        order.span(span).len(),
        40,
        "順序の切片の長さが窓の行数と違う（前提）"
    );
    for (offset, row) in decoded.rows().iter().enumerate() {
        assert_eq!(row_key(window_rows[offset]), row.key(), "{offset} 行目が違う");
    }
}

/// **現在より新しい世代**を名乗る要求も空の窓で答える（世代の判定は大小ではなく一致である）。
///
/// 窓はつねに**現在の世代**のものであるため、「古い」側だけでなく「新しい」側も現在の世代と
/// 一致しない。一致だけを見る実装（`!=`）では両方とも空の窓になり、大小で見る実装（`<`）では
/// 新しい世代が素通りして**現在の世代の窓が返る**。後者は、呼び出し側が自分の見ている世代と
/// 食い違うデータを受け取る経路である（レビューの変異試験が、この半分が無検査であることを
/// 実測した）。
#[test]
fn a_newer_generation_is_also_answered_with_the_empty_window() {
    let prepared = Prepared::new(8, 3, 0.0, &no_view());
    let current = Generation::new(30);
    let codec = WindowCodec::new(current);
    let source = SheetSource {
        sheet: prepared.sheet(),
    };

    // 範囲そのものは妥当である（世代だけが一致しない要求を作る。範囲の誤りと混ざらない）。
    let span = RowSpan::new(RowOrdinal::new(0), 4);
    let newer = WindowRequest::new(Generation::new(current.get() + 1), span);
    assert!(codec.is_stale(&newer), "前提: この要求は現在の世代と一致しない");
    assert!(
        span.start().get() + span.count() <= prepared.order.len(),
        "前提: この要求の範囲は妥当である"
    );

    assert_eq!(
        Ok(EMPTY_WINDOW.to_vec()),
        codec.encode(
            &prepared.order,
            &prepared.index,
            prepared.columns(),
            &source,
            &newer,
        ),
        "現在より新しい世代の要求に窓を返した（世代の判定が大小になっている）"
    );
    assert!(
        !codec.is_stale(&WindowRequest::new(current, span)),
        "対照: 現在の世代の要求は古くない"
    );
}

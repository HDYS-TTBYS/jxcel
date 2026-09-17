//! 表示の指定を変えても、**保存される順序が変わらない**（タスク 8.8。data-grid 要件 8.5、8.8）。
//!
//! tasks.md 8.8 の最後の受け入れ項目は、**画面からは確かめられない**ことを名指ししている —
//! 保存の経路は本スペックの外（`document-session` と `src-tauri`）にあり、画面が持つのは表示の
//! 指定だけである。したがって「表示の指定を変える前後で、ドキュメント形式の**直列化結果**が
//! 一致すること」は、ドメインの側で確かめる。このファイルがその 1 本である。
//!
//! # 何と比較するのか（**バイト列である**）
//!
//! 比較するのは **保存の経路が書くバイト列そのもの**である。`document-format` の module docs
//! 「保存の順序」のとおり、`save` は
//!
//! ```text
//! validate_document → to_parts → ContainerCodec::encode → 原子的書き込み
//! ```
//!
//! の 1 経路であり、`to_parts` → `ContainerCodec::encode` が**バイト列を決める**段である
//! （書き込みは同じバイト列をファイルへ置くだけである）。本ファイルは両方を通す:
//!
//! 1. **メモリ上の直列化**（`to_parts` → `ContainerCodec::encode`）の前後比較 — 1 バイトも
//!    違わないこと
//! 2. **実際の保存経路**（`DocumentFormatApi::save`）が置くファイルのバイト列の比較 — 表示の
//!    指定を変える前と後で、書き出されたファイルが**同一であること**。経路の途中
//!    （`to_parts`）だけを見て「保存は変わらない」と言わないためである
//!
//! **論理エントリの集合**（行データのエントリのバイト列、行の識別子の並び、行の値の並び）も
//! 突き合わせる（直列化の一致を「たまたま同じバイト列になった」で済ませず、**順序と値のどちらも
//! 動いていないこと**をエントリの粒度で読めるようにするためである）。
//!
//! # 空振りしないこと（**このファイルが買っているもの**）
//!
//! 「変わらない」と言う検査は、対象が動きうる形になっていなければ**何も見ていない**。
//! 本ファイルは 3 段で空振りを防ぐ。
//!
//! 1. **表示の順序が実際に変わることを、検査自身が独立に導いて表明する**。並べ替え（基準列 0 の
//!    降順）は文書の 40 行の**置換**であり、絞り込みは行を**隠す**。導いた可視の並びが文書の
//!    並びと違うことを確かめてから直列化の一致を見る — 可視の並びをそのまま文書へ書き戻す
//!    実装（この検査が落とすべき故障）なら、**必ずバイト列が変わる**配置である
//! 2. **可視の並びが変わらない指定**（同じ指定の再適用・解除）でも比較する（対照である）
//! 3. **並べ替えの基準列の値を、その列の最大（100.0）へ書き換える**。順序を導き直す実装なら
//!    その行は可視の先頭へ移る（検査自身が `RowOrder::recompute` で導いて確かめる）。
//!    したがって「行が動かないこと」は、動きうる形の上で確かめている
//!
//! 変異の実測は `research.md`「実測と固定: 画面の表示の操作（タスク 8.8）」にある
//! （可視の並びを文書へ書き戻す形へ変えると、このファイルが落ちる）。
//!
//! # 標本の前提（**先に表明してから依拠する**）
//!
//! `tests/common/sample.rs` の決定性の契約どおり、標本の識別子（ULID）は組み立てのたびに
//! 変わる。したがって本ファイルは**標本の生のバイト列を期待値に書かない** — 比較はすべて
//! **同じ 1 つの標本の前後**（同じ文書の 2 回の直列化）で行う。標本に依拠する前提は次の 4 つで
//! あり、検査が最初に表明する:
//!
//! - 一意制約つきの列は列 0 だけであり、その値は同値を持たない（基準列 0 の降順が**一意に**決まる）
//! - 文書の行順は行識別子の順である（決着の観測が文書の順に依らない）
//! - 列 3（予備率。`Float`、範囲 0.0〜100.0）の適合する値は 0.0〜99.5 であり、違反する値は
//!   -1.0 だけである（**100.0 がその列の最大**であることが、下の編集の前提である）
//! - 計画の列数は標本の列数と一致する
//!
//! # 単体テストが観測しないもの（**正直に書く**）
//!
//! - **画面から保存の経路へ値が渡ること**は観測しない（保存の経路は本スペックの外にあり、
//!   画面は保存を呼ばない）。本ファイルが固定するのは**表示の指定が文書へ届かないこと**で
//!   ある — 画面側の主張（幅と並びが境界へ渡らないこと）は `viewOps.test.ts` /
//!   `GridScreen.test.ts` が固定する
//! - `DocumentFormatApi::save` の決定性（同じ内容が同じバイト列になること）は `document-format`
//!   の `tests/api.rs` が固定する。本ファイルはそれを**前提として**使い、表示の指定による差が
//!   **無いこと**だけを見る

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use common::sample::{sample, SampleOptions};
use data_grid::{
    decode_window, display_text, CellAddress, ColumnIndex, EditCommand, ExpansionState, FilterSpec,
    GridSession, RowOrder, RowOrdinal, RowSpan, SortKey, UndoStack, ViewSpec, ViewSummary,
    DEFAULT_UNDO_LIMIT,
};
use document_format::container::ContainerCodec;
use document_format::parts::{to_parts, RowsCodec};
use document_format::{
    CellValue, Document, DocumentFormat, DocumentFormatApi, EntryName, Row, RowId, SheetId,
};

// ---------------------------------------------------------------------------
// 表示の指定（短く書く）
// ---------------------------------------------------------------------------

/// 並べ替えも絞り込みも無い指定（順序は文書の順そのものである）。
fn no_view() -> ViewSpec {
    ViewSpec {
        sort: Vec::new(),
        filters: Vec::new(),
    }
}

/// 列 `column` を降順に並べ替える指定。
fn sorted_descending(column: usize) -> ViewSpec {
    ViewSpec {
        sort: vec![SortKey {
            column: ColumnIndex::new(column),
            descending: true,
        }],
        filters: Vec::new(),
    }
}

/// 列 `column` の表示文字列に `text` を含む行だけを通す指定（並べ替えは無し）。
fn filtered(column: usize, text: &str) -> ViewSpec {
    ViewSpec {
        sort: Vec::new(),
        filters: vec![FilterSpec::Contains {
            column: ColumnIndex::new(column),
            text: text.to_owned(),
        }],
    }
}

// ---------------------------------------------------------------------------
// 保存の経路（直列化と、実際の書き出し）
// ---------------------------------------------------------------------------

/// **保存の経路が書くバイト列**（`to_parts` → `ContainerCodec::encode`）。
///
/// `save` はこのバイト列をファイルへ置くだけである（`document-format` の module docs
/// 「保存の順序」）ので、前後比較の主たる対象はこれである。
fn saved_bytes(document: &Document) -> Vec<u8> {
    let parts = to_parts(document).expect("標本は保存できる");
    ContainerCodec::encode(&parts).expect("標本は符号化できる")
}

/// 保存の経路が組む論理エントリの集合（**名前の昇順**。`to_parts` の正準な並び）。
fn saved_entries(document: &Document) -> Vec<(EntryName, Vec<u8>)> {
    to_parts(document)
        .expect("標本は保存できる")
        .iter()
        .map(|part| (part.name, part.bytes.clone()))
        .collect()
}

/// 行データのエントリを復号したもの（**ファイルの行順そのまま**）。
///
/// 直列化の一致を「バイト列が同じ」だけで済ませず、**行の並びが動いていないこと**として読む
/// ための口である（行データは 1 シート 1 エントリの JSONL であり、行順がそのまま並びである）。
fn decoded_rows(document: &Document, sheet: SheetId) -> Vec<Row> {
    let entry = EntryName::Rows { sheet };
    let bytes = to_parts(document)
        .expect("標本は保存できる")
        .get(&entry)
        .expect("行データのエントリがある")
        .bytes
        .clone();
    RowsCodec::decode(&entry, &bytes)
        .expect("行データは復号できる")
        .into_rows()
}

/// 行データのエントリが運ぶ行の識別子（**ファイルの行順**）。
fn saved_row_ids(document: &Document, sheet: SheetId) -> Vec<RowId> {
    decoded_rows(document, sheet).iter().map(Row::id).collect()
}

/// 行データのエントリが運ぶ行の値（**ファイルの行順**）。
fn saved_row_values(document: &Document, sheet: SheetId) -> Vec<Vec<CellValue>> {
    decoded_rows(document, sheet)
        .iter()
        .map(|row| row.values().to_vec())
        .collect()
}

/// テスト専用の作業ディレクトリ（終了時に必ず削除する）。
///
/// `document-format` の `tests/common/mod.rs` の同名の補助と**同じ規律**である: テストは並行に
/// 走るのでプロセス ID と単調カウンタで一意にし、`Drop` が後始末をする（panic による巻き戻し
/// でも走る）。`std::env::temp_dir()` を使わないのは、コンテナ内とホストで見え方が違うためで
/// ある（リポジトリ内に閉じる）。
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Self {
        static SEQUENCE: AtomicU32 = AtomicU32::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(format!("scratch_{tag}_{}_{sequence}", std::process::id()));
        fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
        Self { path }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // 既に消えていても失敗しない（多重削除・異常終了の後始末）。
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 実際の保存経路（`DocumentFormatApi::save`）で書き出し、置かれたバイト列を返す。
fn saved_file_bytes(document: &Document, path: &Path) -> Vec<u8> {
    DocumentFormat::new()
        .save(document, path)
        .expect("標本は保存できる");
    fs::read(path).expect("保存したファイルが読める")
}

// ---------------------------------------------------------------------------
// 標本の前提と、順序の導出
// ---------------------------------------------------------------------------

/// 標本の前提（モジュール docs「標本の前提」）。**検査が依拠する前に表明する。**
fn assert_premises(options: &SampleOptions) {
    let sample = sample(options);
    let plan = sample.compiled();
    assert_eq!(
        vec![ColumnIndex::new(0)],
        plan.unique_columns().to_vec(),
        "標本の一意制約つきの列は列 0 だけ"
    );
    assert_eq!(
        sample.column_count(),
        plan.column_count(),
        "計画の列数は標本の列数"
    );
    let ids = document_ids(sample.document(), sample.sheet());
    assert_eq!(
        sample.row_ids().to_vec(),
        ids,
        "文書の行順は行識別子の順である（決着の観測が文書の順に依らない）"
    );

    // 列 0（品番）は同値を持たない（基準列 0 の降順が一意に決まる）。
    let mut texts: Vec<String> = sample
        .document()
        .sheet_by_id(sample.sheet())
        .expect("シートは文書にある")
        .rows()
        .iter()
        .map(|row| display_text(row.values().first().unwrap_or(&CellValue::Null)).into_owned())
        .collect();
    texts.sort();
    texts.dedup();
    assert_eq!(
        sample.rows(),
        texts.len(),
        "列 0 の値は行数と同じ個数（同値が 1 件も無い）"
    );

    // 列 3（予備率）: 適合する値は 0.0〜99.5、違反する値は -1.0 だけである。**100.0 がその列の
    // 最大**であることが、下の編集（基準列の値を最大へ書き換える）の前提である。
    let floats: Vec<f64> = sample
        .document()
        .sheet_by_id(sample.sheet())
        .expect("シートは文書にある")
        .rows()
        .iter()
        .map(|row| match row.values().get(3) {
            Some(CellValue::Float(value)) => *value,
            other => panic!("列 3 は浮動小数である（実際: {other:?}）"),
        })
        .collect();
    assert_eq!(sample.rows(), floats.len(), "列 3 の値はすべての行にある");
    let largest = floats.iter().copied().fold(f64::MIN, f64::max);
    let smallest = floats.iter().copied().fold(f64::MAX, f64::min);
    assert!(
        largest <= 99.5,
        "前提: 列 3 の適合する値は 99.5 以下（実際の最大: {largest}）"
    );
    assert!(
        smallest >= -1.0,
        "前提: 列 3 の違反する値は -1.0 以上（実際の最小: {smallest}）"
    );
}

/// 文書の行識別子を**文書の順**に取り出す。
fn document_ids(document: &Document, sheet: SheetId) -> Vec<RowId> {
    document
        .sheet_by_id(sheet)
        .expect("シートは文書にある")
        .rows()
        .iter()
        .map(Row::id)
        .collect()
}

/// 検査自身が導いた可視の並び（**セッションを経由しない独立な導出**。前提の表明に使う）。
fn derived_order(document: &Document, sheet: SheetId, spec: &ViewSpec) -> Vec<RowId> {
    let mut order = RowOrder::default();
    order.recompute(document, sheet, spec);
    order
        .span(RowSpan::new(RowOrdinal::new(0), order.len()))
        .to_vec()
}

/// セッションの窓を復号し、**表示の順**の行識別子を取り出す。
fn displayed_ids(session: &GridSession, document: &Document, sheet: SheetId) -> Vec<RowId> {
    let span = RowSpan::new(RowOrdinal::new(0), session.visible_row_count());
    let bytes = session
        .encode_window(document, span)
        .expect("窓を符号化できる");
    let decoded = decode_window(&bytes).expect("窓を復号できる");
    // 窓が運ぶのは行識別子の**生 16 バイト**である（`RowId` の値ではない）。文書の側の鍵で
    // 引いて `RowId` へ戻す（標本の識別子を期待値に書かないための手順である）。
    let mut candidates: Vec<(RowId, [u8; 16])> = document_ids(document, sheet)
        .into_iter()
        .map(|id| (id, id.ulid().to_bytes()))
        .collect();
    decoded
        .rows()
        .iter()
        .map(|row| {
            let found = candidates
                .iter()
                .position(|(_, key)| *key == row.key())
                .expect("窓の行は文書の行である");
            candidates.remove(found).0
        })
        .collect()
}

/// 標本 1 つと、その標本を操作するセッション（標本は 1 回だけ組み立てる）。
struct Fixture {
    document: Document,
    sheet: SheetId,
    session: GridSession,
    /// 取り消し履歴（**所有者は呼び出し側である** — 要件 9.5。セッションは所有しない）。
    history: UndoStack,
    /// 標本の行数（文書を借りたまま読めるように控える）。
    rows: usize,
}

impl Fixture {
    fn new(options: &SampleOptions) -> Self {
        assert_premises(options);
        let sample = sample(options);
        let plan = sample.compiled();
        let session = GridSession::open(sample.sheet(), plan).expect("セッションを開ける");
        let parts = sample.into_edit_parts();
        let rows = parts.row_ids.len();
        Self {
            document: parts.document,
            sheet: parts.sheet,
            session,
            history: UndoStack::new(DEFAULT_UNDO_LIMIT),
            rows,
        }
    }

    /// 標本（40 行 × 13 列、違反の割合 0.2）。**列 0〜12 が組込の各型を 1 列ずつ持つ。**
    fn specimen() -> Self {
        Self::new(&SampleOptions::new(40, 13).with_ratio(0.2))
    }

    fn set_view(&mut self, spec: ViewSpec) -> ViewSummary {
        self.session
            .set_view(&self.document, spec)
            .expect("表示を指定できる")
    }

    /// 行 `row` の列 `column` へ、打たれた文字を書く（要件 3.3 の編集経路そのもの）。
    fn write(&mut self, row: RowId, column: usize, text: &str) {
        let Self {
            session,
            document,
            history,
            ..
        } = self;
        session
            .apply(
                document,
                history,
                EditCommand::SetCells {
                    cells: vec![(
                        CellAddress::new(row, ColumnIndex::new(column)),
                        text.to_owned(),
                    )],
                },
            )
            .expect("編集は適用できる");
    }

    fn displayed(&self) -> Vec<RowId> {
        displayed_ids(&self.session, &self.document, self.sheet)
    }

    fn row_ids(&self) -> Vec<RowId> {
        document_ids(&self.document, self.sheet)
    }

    /// 行 `row` の列 `column` の値（文書から直接読む。**セッションを通さない**）。
    fn value_of(&self, row: RowId, column: usize) -> CellValue {
        self.document
            .sheet_by_id(self.sheet)
            .expect("シートは文書にある")
            .rows()
            .iter()
            .find(|found| found.id() == row)
            .expect("行はシートにある")
            .values()
            .get(column)
            .cloned()
            .unwrap_or(CellValue::Null)
    }
}

// ---------------------------------------------------------------------------
// 1. 並べ替えだけを指定する（可視の並びが文書の並びの**置換**になる）
// ---------------------------------------------------------------------------

/// **並べ替えを指定しても、保存の経路が書くバイト列は 1 バイトも変わらない**（要件 8.5、8.8）。
///
/// 並べ替えだけの指定（絞り込み無し）は可視の並びが文書の 40 行の**置換**になるので、可視の
/// 並びを文書へ書き戻す実装なら**必ず**バイト列が変わる — この検査が落とすべき故障の形である。
#[test]
fn a_sort_alone_leaves_every_saved_byte_identical() {
    let mut fixture = Fixture::specimen();
    let sheet = fixture.sheet;

    // 比較の相手は**同じ標本の前後**である（標本の生のバイト列を期待値に書かない）。
    let before_bytes = saved_bytes(&fixture.document);
    let before_entries = saved_entries(&fixture.document);
    let before_ids = fixture.row_ids();
    assert_eq!(
        before_ids,
        saved_row_ids(&fixture.document, sheet),
        "前提: 行データのエントリの行順は文書の行順そのもの"
    );

    // **実際の保存経路**で 1 度書き出す（`save` が置いたファイルのバイト列そのもの）。
    let scratch = Scratch::new("view_change");
    let before_file = saved_file_bytes(&fixture.document, &scratch.file("before.zip"));
    assert_eq!(
        before_bytes, before_file,
        "前提: `save` が置くファイルは to_parts → encode のバイト列そのもの"
    );

    // 並べ替えを指定する（基準列 0 の降順 — 同値が無いので順序が一意に決まる）。
    let spec = sorted_descending(0);
    let summary = fixture.set_view(spec.clone());

    // 前提: **可視の並びが実際に変わる**（空振りしないことの表明）。並べ替えは行を隠さないので、
    // 可視の並びは行数と同じ長さの置換である。
    assert_eq!(fixture.rows, summary.visible, "並べ替えは行を隠さない");
    assert_eq!(0, summary.hidden, "並べ替えは行を隠さない（隠れは 0）");
    let derived = derived_order(&fixture.document, sheet, &spec);
    assert_eq!(
        derived,
        fixture.displayed(),
        "セッションの表示の並びは、同じ指定から独立に導いた並びと一致する"
    );
    assert_ne!(
        before_ids, derived,
        "前提: この並べ替えは表示の並びを実際に変える（文書へ書き戻せばバイト列が変わる配置である）"
    );

    // 本題: 直列化は前後で 1 バイトも変わらない。
    assert_eq!(
        before_bytes,
        saved_bytes(&fixture.document),
        "並べ替えの指定は保存の経路のバイト列を 1 バイトも変えない（要件 8.5）"
    );
    assert_eq!(
        before_entries,
        saved_entries(&fixture.document),
        "論理エントリの集合（行データのエントリを含む）も 1 バイトも変わらない"
    );
    assert_eq!(
        before_ids,
        fixture.row_ids(),
        "文書の行順は動かない（表示の並びは文書の並びではない）"
    );
    assert_eq!(
        before_ids,
        saved_row_ids(&fixture.document, sheet),
        "行データのエントリが運ぶ行の並びも動かない"
    );

    // **実際の保存経路**の比較（表示の指定を変えた後に書き出したファイルが同一であること）。
    let after_file = saved_file_bytes(&fixture.document, &scratch.file("after.zip"));
    assert_eq!(
        before_file, after_file,
        "表示の指定を変える前後で、保存されたファイルは 1 バイトも違わない"
    );

    // 同じ指定を適用し直しても、解除しても同じである（可視の並びが動く指定と動かない指定の
    // 双方を、同じ比較で通す）。
    let again = fixture.set_view(spec);
    assert_eq!(fixture.rows, again.visible);
    assert_eq!(
        before_bytes,
        saved_bytes(&fixture.document),
        "同じ指定の再適用でもバイト列は変わらない"
    );
    let cleared = fixture.set_view(no_view());
    assert_eq!(fixture.rows, cleared.visible, "解除で行は減らない");
    assert_eq!(
        before_ids,
        fixture.displayed(),
        "解除すると表示の並びは文書の並びへ戻る"
    );
    assert_eq!(
        before_bytes,
        saved_bytes(&fixture.document),
        "並べ替えの解除でもバイト列は変わらない"
    );
}

// ---------------------------------------------------------------------------
// 2. 絞り込みと展開を混ぜる（可視の行集合と、描かれる列の構成が変わる指定）
// ---------------------------------------------------------------------------

/// **絞り込み（行を隠す）と入れ子の展開（描かれる列を変える）を指定しても、保存の経路が書く
/// バイト列は 1 バイトも変わらない**（要件 8.5、8.7、5.1）。
///
/// 絞り込みは**行を隠す**ので、可視の並びは文書の行の置換ですらない（それでも直列化は変わらない）。
/// 展開は列の構成を導出する側の話であり、**文書の宣言にも行データにも届かない** — そのことを
/// 同じ比較で確かめる（`GridViewSpec` が運ぶ 3 つ目の指定である）。
#[test]
fn a_filter_and_an_expansion_leave_every_saved_byte_identical() {
    let mut fixture = Fixture::specimen();
    let sheet = fixture.sheet;

    let before_bytes = saved_bytes(&fixture.document);
    let before_entries = saved_entries(&fixture.document);
    let before_values = saved_row_values(&fixture.document, sheet);
    let before_ids = fixture.row_ids();

    // ① 行を隠さない部分一致（品番が `0000` を含む行 = 行 0〜9。標本では `P0000000` … `P0000039`）。
    let matching = filtered(0, "0000");
    let summary = fixture.set_view(matching.clone());
    assert_eq!(fixture.rows, summary.visible, "この部分一致は行を隠さない");
    assert_eq!(0, summary.hidden);
    assert_eq!(
        derived_order(&fixture.document, sheet, &matching),
        fixture.displayed(),
        "絞り込みの可視の並びも独立な導出と一致する"
    );
    assert_eq!(
        before_bytes,
        saved_bytes(&fixture.document),
        "行を隠さない絞り込みでもバイト列は変わらない"
    );

    // ② 行を実際に隠す絞り込み（列 4「検査済み」が真の行だけを見せる = 積の片方）。
    let hiding = filtered(4, "true");
    let summary = fixture.set_view(hiding.clone());
    assert!(
        summary.visible > 0 && summary.hidden > 0,
        "前提: この絞り込みは行を実際に隠す（可視 {} / 隠れ {}）",
        summary.visible,
        summary.hidden
    );
    assert_eq!(
        fixture.rows,
        summary.visible + summary.hidden,
        "可視行数と隠れた行数の和は文書の行数（要件 8.7）"
    );
    let hidden_order = derived_order(&fixture.document, sheet, &hiding);
    assert_eq!(hidden_order, fixture.displayed());
    assert_ne!(
        before_ids, hidden_order,
        "前提: この絞り込みは可視の集合を実際に変える（空振りしない）"
    );
    assert_eq!(
        before_bytes,
        saved_bytes(&fixture.document),
        "行を隠す絞り込みでもバイト列は変わらない"
    );

    // ③ 絞り込みと並べ替えの同時指定（順序が絞り込み後の集合に対して定まる。要件 8.5、8.7）。
    let combined = ViewSpec {
        sort: sorted_descending(0).sort,
        filters: hiding.filters.clone(),
    };
    let summary = fixture.set_view(combined.clone());
    let combined_order = derived_order(&fixture.document, sheet, &combined);
    assert_eq!(combined_order, fixture.displayed());
    assert_eq!(summary.visible, combined_order.len());
    assert_ne!(
        hidden_order, combined_order,
        "前提: 並べ替えを足すと表示の並びが実際に変わる"
    );
    assert_eq!(
        before_bytes,
        saved_bytes(&fixture.document),
        "絞り込みと並べ替えの同時指定でもバイト列は変わらない"
    );

    // ④ 入れ子の展開（要件 5.1、5.2）。**描かれる列の構成が変わる**指定である。
    let collapsed = fixture.session.columns().to_vec();
    fixture
        .session
        .set_expansion(ExpansionState::expanded_to(ColumnIndex::new(10), 1));
    let expanded = fixture.session.columns().to_vec();
    assert!(
        expanded.len() > collapsed.len(),
        "前提: 展開すると列の構成が増える（{} → {}）",
        collapsed.len(),
        expanded.len()
    );
    assert!(
        expanded.iter().any(|column| column.name.contains('.')),
        "前提: 展開した列の表示名は位置を `.` で連結した形になる"
    );
    // **可視の行の集合と並びは展開で動かない**（要件 5.3 が名指しした「走査で失われない」）。
    assert_eq!(
        combined_order,
        fixture.displayed(),
        "展開は表示の並びを変えない"
    );
    assert_eq!(
        before_bytes,
        saved_bytes(&fixture.document),
        "入れ子の展開でもバイト列は変わらない"
    );
    assert_eq!(
        before_entries,
        saved_entries(&fixture.document),
        "論理エントリの集合も（スキーマのエントリを含めて）変わらない"
    );
    assert_eq!(
        before_values,
        saved_row_values(&fixture.document, sheet),
        "行データのエントリの値も 1 つも変わらない"
    );
    assert_eq!(before_ids, fixture.row_ids());
    assert_eq!(before_ids, saved_row_ids(&fixture.document, sheet));
}

// ---------------------------------------------------------------------------
// 3. 並べ替えが効いている間の編集（**確定と同時に行が動かない**）
// ---------------------------------------------------------------------------

/// **並べ替えの基準列の値を編集しても、行は動かず、保存の経路が書く順序も変わらない**
/// （要件 8.5、8.8）。
///
/// 順序が導出し直されるのは [`GridSession::set_view`] のときだけである（design.md「RowOrder」の
/// 決定「順序の再計算は `set_view` でのみ起きる」）。本検査はその決定を、**動きうる形**で確かめる:
///
/// 1. 基準列（列 3 予備率）の降順を指定する
/// 2. **その列の最大（100.0）**を、表示の並びの最後の行へ書く。順序を導き直す実装なら、その行は
///    先頭へ移る（検査自身が `RowOrder::recompute` で導いて表明する）
/// 3. 実際には**行は動かない**（表示の並びが前後で同一である）。文書の行順も、保存の経路が
///    書く行の並びも動かない — 変わるのは**その行の値の欄だけ**である
#[test]
fn an_edit_under_an_active_sort_moves_neither_the_row_nor_the_saved_order() {
    let mut fixture = Fixture::specimen();
    let sheet = fixture.sheet;

    // 基準列 3（予備率）の降順。行は隠れない。
    let spec = sorted_descending(3);
    let summary = fixture.set_view(spec.clone());
    assert_eq!(fixture.rows, summary.visible, "並べ替えは行を隠さない");
    assert_eq!(0, summary.hidden);

    let order_before = fixture.displayed();
    let ids_before = fixture.row_ids();
    let bytes_before = saved_bytes(&fixture.document);
    let values_before = saved_row_values(&fixture.document, sheet);
    let entries_before = saved_entries(&fixture.document);

    // 表示の並びの**最後**の行を選ぶ（前提: 先頭ではないので、動けば位置が変わる）。
    let row = *order_before.last().expect("標本は 1 行以上ある");
    let ordinal_before = RowOrdinal::new(order_before.len() - 1);
    let mut order = RowOrder::default();
    order.recompute(&fixture.document, sheet, &spec);
    assert_eq!(
        Some(ordinal_before),
        order.ordinal_of(row),
        "前提: 選んだ行は表示の並びの最後にある"
    );

    // その列の**最大**（100.0）を書く（前提: 既存の値はすべてこれより小さい）。
    fixture.write(row, 3, "100.0");

    // 前提: 書けた値がその列の最大である（そうでなければ「動くはず」の主張が崩れる）。
    assert_eq!(
        CellValue::float(100.0),
        fixture.value_of(row, 3),
        "前提: 打った文字は浮動小数 100.0 として書かれる（列 3 の最大）"
    );

    // 前提: **順序を導き直す実装なら、この行は先頭へ移る**（検査自身が導く）。
    let recomputed = derived_order(&fixture.document, sheet, &spec);
    assert_eq!(
        Some(RowOrdinal::new(0)),
        {
            let mut fresh = RowOrder::default();
            fresh.recompute(&fixture.document, sheet, &spec);
            fresh.ordinal_of(row)
        },
        "前提: 編集後の値はその列の最大なので、順序を導き直せば先頭へ移る"
    );
    assert_eq!(recomputed.first().copied(), Some(row));
    assert_ne!(
        order_before, recomputed,
        "前提: 導き直した並びは、いまの表示の並びと実際に違う（空振りしない）"
    );

    // 本題 1: **行は動かない**（確定と同時に表示位置が動かない。要件 8.8）。
    assert_eq!(
        order_before,
        fixture.displayed(),
        "編集では順序が導出し直されない（表示の位置が動かない。要件 8.8）"
    );
    assert_eq!(
        summary.visible,
        fixture.session.visible_row_count(),
        "可視行数も動かない"
    );

    // 本題 2: **保存の経路が書く順序も動かない**。文書の行順は編集で変わらない。
    assert_eq!(
        ids_before,
        fixture.row_ids(),
        "編集は文書の行順を変えない（保存される順序である）"
    );
    assert_eq!(
        ids_before,
        saved_row_ids(&fixture.document, sheet),
        "行データのエントリが運ぶ行の並びも動かない"
    );
    // 本題 3: 直列化の差は**編集した行の値の欄だけ**である。論理エントリの粒度では、差は
    // 行データのエントリと、そのダイジェストを載せる索引（`manifest.json`）の 2 つに限られる
    // （索引は**行データのバイト列から導かれる**ものであり、順序を運ぶ欄ではない）。
    let entries_after = saved_entries(&fixture.document);
    let differing: Vec<EntryName> = entries_before
        .iter()
        .zip(&entries_after)
        .filter(|(before, after)| before != after)
        .map(|(before, _)| before.0)
        .collect();
    assert_eq!(
        vec![
            EntryName::Manifest,
            EntryName::Rows { sheet },
        ],
        differing,
        "差は行データと、そのダイジェストを載せる索引だけである（他のエントリは 1 バイトも動かない）"
    );
    assert_eq!(
        1,
        entries_after
            .iter()
            .filter(|(name, _)| matches!(name, EntryName::Rows { sheet: found } if *found == sheet))
            .count(),
        "標本のシートの行データのエントリは 1 つだけである（並びを運ぶのはそれ 1 つ）"
    );
    let values_after = saved_row_values(&fixture.document, sheet);
    let changed: Vec<usize> = (0..ids_before.len())
        .filter(|index| values_before[*index] != values_after[*index])
        .collect();
    assert_eq!(
        vec![ids_before
            .iter()
            .position(|id| *id == row)
            .expect("行は文書にある")],
        changed,
        "値が変わった行は編集した行だけ"
    );
    assert_eq!(
        Some(&CellValue::float(100.0)),
        values_after[changed[0]].get(3),
        "変わったのは編集した列の値である"
    );
    assert_ne!(
        bytes_before,
        saved_bytes(&fixture.document),
        "値の編集なのでバイト列は変わる（変わらないなら、そもそも比較が空振りである）"
    );

    // 本題 4: 編集の後に表示の指定を変えても、順序は導出し直されるだけで**文書は動かない**。
    let bytes_after_edit = saved_bytes(&fixture.document);
    let cleared = fixture.set_view(no_view());
    assert_eq!(fixture.rows, cleared.visible, "解除で行は減らない");
    assert_eq!(
        ids_before,
        fixture.displayed(),
        "解除すると表示の並びは文書の並びへ戻る（編集した行の位置も文書の位置である）"
    );
    assert_eq!(
        bytes_after_edit,
        saved_bytes(&fixture.document),
        "編集の後の表示の指定も、保存の経路のバイト列を 1 バイトも変えない"
    );
}

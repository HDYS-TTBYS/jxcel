//! クレート外から見た行の削除と位置指定の挿入（データグリッドのタスク 1.2。data-grid 要件 6.1, 6.2）。
//!
//! このファイルは統合テストであり、クレートの**公開面だけ**を使う。`document_format::`
//! 直下の再エクスポート（`remove_rows` / `insert_row_at` / `insert_rows_at` は [`Document`]
//! のメソッド、誤り型は [`RowRemovalError`] / [`RowInsertionError`]）がクレート外から
//! 見えることをコンパイル時に示す。
//!
//! # 何を固定するか
//!
//! * `remove_rows` が**取り除いた行を所有権ごと返す**こと（値と識別子の双方。data-grid 要件 6.2 の
//!   取り消しが要求する。`Row` は `Clone` を持たない）
//! * 返る順序が**シートの順序**であり、要求引数の並びに依らないこと（同じ行集合の要求は
//!   常に同じ結果になる）
//! * 残った行の相対順序と値に触れないこと、返る行数が要求した行数と一致すること
//! * 要求に同じ識別子が複数回現れても 1 回の削除であること（重複は畳む）
//! * `insert_row_at` が 0 番目・中間・末尾（`index == 行数`）に挿入し、**新しい一意の
//!   識別子**を返し、挿入された行が値を持たないこと
//! * `insert_rows_at` が与えられた行を**識別子も値も作り直さずに**与えられた順で
//!   差し込むこと
//! * **削除した行を挿入で戻すと、識別子・値・位置が元に戻ること**（タスク 1.2 の
//!   明示の受け入れ条件。data-grid 要件 6.6 / 9.2 の逆命令が要求する往復。返る順序がシート順で
//!   あるため、離れた位置の行は自分の位置へ昇順に戻せば全体が復元する）
//! * 未知のシート・未知の行・範囲外の位置のそれぞれで、**文書が一切変わらない**こと
//!   （部分適用なし。事前検査を通過するまで 1 行も動かさない）
//! * **他のシートに現存する識別子**を差し込もうとしたとき、`DuplicateRow` になり文書が
//!   一切変わらないこと（一意性は文書単位の不変条件。検査がシート局所だと壊れる）
//!
//! 他のシートに現存する識別子を持つ `Row` は、**`sheets/<ulid>.jsonl` のバイト列から
//! 復号して得る**（`parts::RowsCodec::decode` + `parts::SheetRows::into_rows` は公開面に
//! ある）。`remove_rows` が返す行はその時点で文書から消えているため、この状況は作れない
//! （`remove_rows` の結果を別シートへ入れること自体は一意性を壊さず、成功する）。
//!
//! 挿入位置は**挿入前の行順**に対する添字であり、`index == 行数` は末尾への追加である
//! （`add_row` と同じ結果になる）。列の値の個数と列数の一致は保存時の門
//! （`RowsCodec::encode`）が担うため、本テストは行の集合・並び・識別子・値だけを見る。

use document_format::parts::{RowsCodec, SheetRows};
use document_format::{
    CellValue, Document, EntryName, Row, RowId, RowInsertionError, RowRemovalError, SheetId,
};

/// シートの（行識別子, 列順の値）を行順に写す（比較と不変の観測に使う）。
fn values_of(document: &Document, sheet: SheetId) -> Vec<(RowId, Vec<CellValue>)> {
    document
        .sheet_by_id(sheet)
        .expect("対象シートは存在する")
        .rows()
        .iter()
        .map(|row| (row.id(), row.values().to_vec()))
        .collect()
}

/// 行識別子を行順に写す（行の集合・並び・識別子が変わらないことの観測）。
fn row_ids(document: &Document, sheet: SheetId) -> Vec<RowId> {
    document
        .sheet_by_id(sheet)
        .expect("対象シートは存在する")
        .rows()
        .iter()
        .map(Row::id)
        .collect()
}

/// 与えられた行の値を行順に写す（`remove_rows` が返した行の観測に使う）。
fn values_of_rows(rows: &[Row]) -> Vec<(RowId, Vec<CellValue>)> {
    rows.iter()
        .map(|row| (row.id(), row.values().to_vec()))
        .collect()
}

/// 与えられた行の識別子を並び順に写す（返る順序の観測に使う）。
fn row_ids_of_rows(rows: &[Row]) -> Vec<RowId> {
    rows.iter().map(Row::id).collect()
}

/// 列名を宣言し、与えられた値列で行を積んだ 1 シートの文書と、そのシート識別子を返す。
fn document_with(columns: &[&str], rows: Vec<Vec<CellValue>>) -> (Document, SheetId) {
    let mut document = Document::new();
    let sheet = document.add_sheet("標本");
    document
        .set_sheet_columns(
            sheet,
            columns.iter().map(|name| (*name).to_owned()).collect(),
        )
        .expect("対象シートは存在する");
    for values in rows {
        let row = document.add_row(sheet).expect("シートは存在する");
        document
            .set_row_values(sheet, row, values)
            .expect("行は存在する");
    }
    (document, sheet)
}

/// 6 行 × 2 列の標本。値は行番号から決まるため、行の取り違えが値の比較で露見する。
fn six_by_two() -> (Document, SheetId, Vec<RowId>) {
    let (document, sheet) = document_with(
        &["a", "b"],
        (0..6)
            .map(|index| {
                vec![
                    CellValue::Int(index),
                    CellValue::Text(format!("行{index}")),
                ]
            })
            .collect(),
    );
    let ids = row_ids(&document, sheet);
    (document, sheet, ids)
}

#[test]
fn remove_rows_returns_removed_rows_with_ids_and_values_in_sheet_order() {
    // data-grid 要件 6.2: 選択した複数行の削除。返る行は取り消しが値と識別子の双方を要するため
    // 所有権ごと返り、その順序は要求引数の並びではなく**シートの順序**である。
    let (mut document, sheet, ids) = six_by_two();

    // シート順では ids[1] が先、ids[3] が後である。要求は逆順に渡す。
    let removed = document
        .remove_rows(sheet, &[ids[3], ids[1]])
        .expect("実在する行の削除は成功する");

    assert_eq!(
        vec![
            (ids[1], vec![CellValue::Int(1), CellValue::Text("行1".to_owned())]),
            (ids[3], vec![CellValue::Int(3), CellValue::Text("行3".to_owned())]),
        ],
        values_of_rows(&removed),
        "要求引数の並びではなくシートの順序で返る"
    );
    assert_eq!(
        vec![ids[0], ids[2], ids[4], ids[5]],
        row_ids(&document, sheet),
        "残った行は相対順序を保つ"
    );
}

#[test]
fn remove_rows_leaves_remaining_rows_and_their_values_untouched() {
    let (mut document, sheet, ids) = six_by_two();
    let before = values_of(&document, sheet);

    let removed = document
        .remove_rows(sheet, &[ids[2], ids[4]])
        .expect("実在する行の削除は成功する");

    assert_eq!(2, removed.len(), "要求した行数だけが返る");
    let after = values_of(&document, sheet);
    let expected: Vec<(RowId, Vec<CellValue>)> = before
        .iter()
        .filter(|(id, _)| *id != ids[2] && *id != ids[4])
        .cloned()
        .collect();
    assert_eq!(
        expected, after,
        "取り除いた行以外の識別子・並び・値は 1 つも変わらない"
    );
}

#[test]
fn remove_rows_deduplicates_repeated_identifiers() {
    // 同じ行を 2 度消すのは同じ 1 回の削除である（誤りではない）。
    let (mut document, sheet, ids) = six_by_two();

    let removed = document
        .remove_rows(sheet, &[ids[3], ids[1], ids[3], ids[1], ids[1]])
        .expect("重複した識別子は誤りではない");

    assert_eq!(
        vec![ids[1], ids[3]],
        row_ids_of_rows(&removed),
        "同じ識別子は 1 回だけ取り除かれ、シート順で返る"
    );
    assert_eq!(
        vec![ids[0], ids[2], ids[4], ids[5]],
        row_ids(&document, sheet),
        "畳んだ結果は 1 回だけ渡した場合と同じ行集合・並びになる"
    );
}

#[test]
fn remove_rows_with_unknown_sheet_returns_unknown_sheet_and_changes_nothing() {
    let (mut document, sheet, ids) = six_by_two();
    // 別の文書で発行した識別子は、この文書のどのシートでもない。
    let ghost = Document::new().add_sheet("別文書");
    let before = values_of(&document, sheet);

    let error = document
        .remove_rows(ghost, &[ids[0]])
        .expect_err("未知のシートは失敗する");

    assert_eq!(RowRemovalError::UnknownSheet { sheet: ghost }, error);
    assert_eq!(before, values_of(&document, sheet), "文書は 1 行も変わらない");
}

#[test]
fn remove_rows_with_unknown_row_returns_unknown_row_and_changes_nothing() {
    let (mut document, sheet, ids) = six_by_two();
    let mut foreign = Document::new();
    let foreign_sheet = foreign.add_sheet("別文書");
    let ghost_row = foreign
        .add_row(foreign_sheet)
        .expect("別文書のシートは存在する");
    let before = values_of(&document, sheet);

    let error = document
        .remove_rows(sheet, &[ids[1], ghost_row])
        .expect_err("未知の行を含む要求は失敗する");

    assert_eq!(RowRemovalError::UnknownRow { row: ghost_row }, error);
    assert_eq!(
        before,
        values_of(&document, sheet),
        "実在する行も含めて 1 行も取り除かれない（部分適用なし）"
    );
}

#[test]
fn insert_row_at_inserts_at_position_zero_middle_and_end() {
    let (mut document, sheet, ids) = six_by_two();

    let first = document
        .insert_row_at(sheet, 0)
        .expect("0 番目への挿入は成功する");
    let middle = document
        .insert_row_at(sheet, 3)
        .expect("中間への挿入は成功する");
    let count = document
        .sheet_by_id(sheet)
        .expect("対象シートは存在する")
        .rows()
        .len();
    let last = document
        .insert_row_at(sheet, count)
        .expect("index == 行数 は末尾への追加である");

    assert_eq!(
        vec![first, ids[0], ids[1], middle, ids[2], ids[3], ids[4], ids[5], last],
        row_ids(&document, sheet),
        "指定した位置に、指定した順で入る"
    );
    let inserted: std::collections::HashSet<RowId> = [first, middle, last].into_iter().collect();
    assert_eq!(3, inserted.len(), "新しい識別子はそれぞれ異なる");
    assert!(
        inserted.iter().all(|id| !ids.contains(id)),
        "発行された識別子は既存の行と重ならない"
    );
    for (id, values) in values_of(&document, sheet) {
        if inserted.contains(&id) {
            assert!(values.is_empty(), "挿入した行は値を持たない");
        }
    }
}

#[test]
fn insert_row_at_with_index_beyond_row_count_is_out_of_range_and_changes_nothing() {
    let (mut document, sheet, ids) = six_by_two();
    let before = values_of(&document, sheet);
    let count = ids.len();

    let error = document
        .insert_row_at(sheet, count + 1)
        .expect_err("行数を超える位置は失敗する");

    assert_eq!(
        RowInsertionError::IndexOutOfRange {
            index: count + 1,
            rows: count,
        },
        error
    );
    assert_eq!(before, values_of(&document, sheet), "文書は 1 行も変わらない");
}

#[test]
fn insert_rows_at_restores_removed_rows_with_identifiers_values_and_positions() {
    // タスク 1.2 の明示の受け入れ条件: 削除した行を挿入で戻すと、識別子・値・位置が
    // 元に戻る（data-grid 要件 6.6 / 9.2 の逆命令がこれで作れる）。
    let (mut document, sheet, ids) = six_by_two();
    let before = values_of(&document, sheet);

    // 位置 1 から連続する 2 行を取り除く（返る順序はシート順 = 取り除いた位置順）。
    let removed = document
        .remove_rows(sheet, &[ids[2], ids[1]])
        .expect("実在する行の削除は成功する");
    assert_eq!(vec![ids[1], ids[2]], row_ids_of_rows(&removed), "シート順で返る");
    assert_eq!(
        vec![ids[0], ids[3], ids[4], ids[5]],
        row_ids(&document, sheet),
        "削除直後の並び"
    );

    document
        .insert_rows_at(sheet, 1, removed)
        .expect("取り除いた行を元の位置へ戻せる");

    assert_eq!(ids, row_ids(&document, sheet), "識別子と位置が元に戻る");
    assert_eq!(before, values_of(&document, sheet), "値も元に戻る");
}

#[test]
fn insert_rows_at_restores_non_contiguous_rows_at_their_own_positions() {
    // 離れた位置の行も、返る順序（シート順）どおりに元の位置へ昇順に戻せば全体が復元する。
    let (mut document, sheet, ids) = six_by_two();
    let before = values_of(&document, sheet);

    let removed = document
        .remove_rows(sheet, &[ids[3], ids[1]])
        .expect("実在する行の削除は成功する");

    for (position, row) in [1usize, 3].into_iter().zip(removed) {
        document
            .insert_rows_at(sheet, position, vec![row])
            .expect("元の位置へ戻せる");
    }

    assert_eq!(ids, row_ids(&document, sheet), "識別子と位置が元に戻る");
    assert_eq!(before, values_of(&document, sheet), "値も元に戻る");
}

#[test]
fn insert_rows_at_appends_at_end_keeping_the_given_order() {
    let (mut document, sheet, ids) = six_by_two();
    let source = document
        .remove_rows(sheet, &[ids[0], ids[1]])
        .expect("実在する行の削除は成功する");
    let count = row_ids(&document, sheet).len();

    document
        .insert_rows_at(sheet, count, source)
        .expect("index == 行数 は末尾への追加である");

    assert_eq!(
        vec![ids[2], ids[3], ids[4], ids[5], ids[0], ids[1]],
        row_ids(&document, sheet),
        "与えられた順のまま末尾に足される"
    );
}

#[test]
fn insert_rows_at_with_index_beyond_row_count_is_out_of_range_and_changes_nothing() {
    let (mut document, sheet, ids) = six_by_two();
    let source = document
        .remove_rows(sheet, &[ids[4]])
        .expect("実在する行の削除は成功する");
    let before = values_of(&document, sheet);
    let count = row_ids(&document, sheet).len();

    let error = document
        .insert_rows_at(sheet, count + 3, source)
        .expect_err("行数を超える位置は失敗する");

    assert_eq!(
        RowInsertionError::IndexOutOfRange {
            index: count + 3,
            rows: count,
        },
        error
    );
    assert_eq!(before, values_of(&document, sheet), "文書は 1 行も変わらない");
}

/// 2 シートを持つ文書と、それぞれの行識別子を返す（文書単位の一意性の観測に使う）。
fn two_sheets_of_three() -> (Document, SheetId, SheetId, Vec<RowId>, Vec<RowId>) {
    let mut document = Document::new();
    let first = document.add_sheet("第1");
    let second = document.add_sheet("第2");
    for sheet in [first, second] {
        document
            .set_sheet_columns(sheet, vec!["a".to_owned()])
            .expect("対象シートは存在する");
        for index in 0..3 {
            let row = document.add_row(sheet).expect("シートは存在する");
            document
                .set_row_values(sheet, row, vec![CellValue::Int(index)])
                .expect("行は存在する");
        }
    }
    let first_ids = row_ids(&document, first);
    let second_ids = row_ids(&document, second);
    (document, first, second, first_ids, second_ids)
}

/// 文書内の全シートの行識別子を集めた多重集合（文書単位の一意性の観測に使う）。
fn all_row_ids(document: &Document) -> Vec<RowId> {
    document
        .sheets()
        .iter()
        .flat_map(|sheet| sheet.rows())
        .map(Row::id)
        .collect()
}

/// 文書内で `RowId` が重複していないことを確かめる（design「Domain Model」の不変条件:
/// 文書内で `SheetId` / `RowId` / `TypeDefId` はそれぞれ一意）。
fn assert_row_ids_are_unique_in_the_document(document: &Document) {
    let ids = all_row_ids(document);
    let unique: std::collections::HashSet<RowId> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "文書内に同じ行識別子が 2 つ現れてはならない");
}

#[test]
fn insert_rows_at_rejects_duplicate_identifiers_inside_one_batch() {
    // **1 回の差し込みの中**での識別子の重複も拒む。この検査は文書全体の集合を引く検査とは
    // 別の分岐であり、単独で必要である: 両方の `Row` が文書内に**現存しない**識別子でも、
    // 同じ識別子が 2 つ入れば文書の一意性が壊れる。
    //
    // そのようなバッチは `sheets/<ulid>.jsonl` の**同じバイト列を 2 回復号して連結する**
    // ことで作れる（`decode` が拒むのは 1 エントリの中の重複だけであり、別々の復号の結果を
    // 1 つの要求へまとめる経路は塞がっていない）。
    let (mut document, sheet, ids) = six_by_two();
    let entry = EntryName::Rows { sheet };
    let bytes = b"{\"$id\":\"01K4ANRRG004HMASW9NF6YY091\",\"a\":1234}\n";

    let first = RowsCodec::decode(&entry, &bytes[..])
        .expect("行エントリは復号できる")
        .into_rows();
    let second = RowsCodec::decode(&entry, &bytes[..])
        .expect("同じバイト列の 2 回目の復号も成功する")
        .into_rows();
    let duplicate_id = first[0].id();
    assert_eq!(
        duplicate_id,
        second[0].id(),
        "同じバイト列からは同じ識別子が出る"
    );
    assert!(
        !ids.contains(&duplicate_id),
        "この識別子は文書内に現存しない（文書全体の検査では捕まらない）"
    );

    let mut batch = first;
    batch.extend(second);
    assert_eq!(2, batch.len(), "1 つの要求に同じ識別子が 2 つ入る");

    let before = row_ids(&document, sheet);
    let error = document
        .insert_rows_at(sheet, 0, batch)
        .expect_err("バッチ内で重複した識別子は失敗する");

    assert_eq!(RowInsertionError::DuplicateRow { row: duplicate_id }, error);
    assert_eq!(
        before,
        row_ids(&document, sheet),
        "差し込み先のシートの行識別子は 1 つも変わらない（部分適用なし）"
    );
}

#[test]
fn insert_rows_at_rejects_a_row_identifier_still_live_in_another_sheet() {
    // 一意性は**文書単位**の不変条件である（design「Domain Model」: 文書内で `SheetId` /
    // `RowId` / `TypeDefId` はそれぞれ一意）。差し込みの検査が挿入先のシートだけを見て
    // いると、**他のシートに現存する**識別子を差し込めてしまい、同じ識別子が文書内に
    // 2 つ存在する状態が作れる（その文書は `to_parts` の検証で `DuplicateId` になる）。
    //
    // その識別子を持つ `Row` は `sheets/<ulid>.jsonl` のバイト列を復号して得る
    // （公開面の経路。`remove_rows` の結果では作れない: 取り除いた時点で識別子は
    // 文書から消えているため）。
    let (mut document, first, second, first_ids, _second_ids) = two_sheets_of_three();

    // 第 1 シートの先頭行の識別子を `$id` に持つ行エントリを復号する。
    let entry = EntryName::Rows { sheet: second };
    let bytes = format!(
        "{{\"$id\":\"{}\",\"a\":0}}\n",
        first_ids[0].ulid().to_string()
    );
    let decoded: SheetRows =
        RowsCodec::decode(&entry, bytes.as_bytes()).expect("行エントリは復号できる");
    let live_elsewhere: Vec<Row> = decoded.into_rows();
    assert_eq!(
        first_ids[0],
        live_elsewhere[0].id(),
        "復号した行は他シートに現存する識別子を持つ"
    );

    // 文書は復号前後で変わらない（復号は文書に触れない）。両シートの行識別子を控える。
    let first_before = row_ids(&document, first);
    let second_before = row_ids(&document, second);

    let error = document
        .insert_rows_at(second, 0, live_elsewhere)
        .expect_err("他シートに現存する識別子の差し込みは失敗する");

    assert_eq!(
        RowInsertionError::DuplicateRow { row: first_ids[0] },
        error,
        "文書内に現存する識別子は DuplicateRow になる"
    );
    assert_eq!(
        first_before,
        row_ids(&document, first),
        "第 1 シートの行識別子は 1 つも変わらない"
    );
    assert_eq!(
        second_before,
        row_ids(&document, second),
        "差し込み先の第 2 シートの行識別子も 1 つも変わらない（部分適用なし）"
    );
    assert_row_ids_are_unique_in_the_document(&document);
}

#[test]
fn inserting_a_row_removed_elsewhere_keeps_the_document_unique() {
    // 一意性が禁じるのは「同じ文書内で同時に 2 箇所に現れること」だけである。
    // `remove_rows` が返した行はその時点で文書から消えているため、別のシートへ入れて
    // も文書内の出現は 1 つであり、成功する（取り消しの往復がこれである）。
    let (mut document, first, second, first_ids, _second_ids) = two_sheets_of_three();
    assert_row_ids_are_unique_in_the_document(&document);

    let removed = document
        .remove_rows(first, &[first_ids[1]])
        .expect("実在する行の削除は成功する");
    document
        .insert_rows_at(second, 0, removed)
        .expect("文書から消えた識別子の差し込みは成功する");

    assert_eq!(
        vec![first_ids[0], first_ids[2]],
        row_ids(&document, first),
        "取り除いた行は元のシートから消えたままである"
    );
    assert_eq!(
        first_ids[1],
        row_ids(&document, second)[0],
        "差し込んだ行は指定した位置に入る"
    );
    assert_eq!(
        1,
        all_row_ids(&document)
            .iter()
            .filter(|id| **id == first_ids[1])
            .count(),
        "差し込んだ行は文書の中にちょうど 1 つ存在する"
    );
    assert_row_ids_are_unique_in_the_document(&document);
}

/// クレート外から `document_format::model::*` 経由でも見えること
/// （根の再輸出とモジュール経由が同一の型であることのコンパイル時の確認）。
#[test]
fn row_operation_errors_are_reachable_through_the_model_module() {
    let ghost = Document::new().add_sheet("別文書");
    let removal: document_format::model::RowRemovalError =
        document_format::model::RowRemovalError::UnknownSheet { sheet: ghost };
    assert_eq!(
        RowRemovalError::UnknownSheet { sheet: ghost },
        removal,
        "根の再輸出と model 経由は同一の型である"
    );
    let insertion: document_format::model::RowInsertionError =
        document_format::model::RowInsertionError::IndexOutOfRange { index: 1, rows: 0 };
    assert_eq!(
        RowInsertionError::IndexOutOfRange { index: 1, rows: 0 },
        insertion,
        "根の再輸出と model 経由は同一の型である"
    );
}

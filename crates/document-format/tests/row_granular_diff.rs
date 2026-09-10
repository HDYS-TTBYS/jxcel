//! 行粒度の差分の検証（タスク 8.3。要件 3.4, 3.5）。
//!
//! design「File Structure Plan」の `tests/row_granular_diff.rs`（`1 行変更で 1 テキスト行のみ
//! 変化`）と「Testing Strategy / Integration Tests」の「**行粒度差分**: 10 万行のうち 1 行を
//! 変更した保存の差分が 1 テキスト行のみである」を実装する。検証する性質は 2 つである:
//!
//! 1. **行の独立性**（要件 3.4）: `sheets/<ulid>.jsonl` の**テキスト行数 == 行数**であること。
//!    値に改行（LF / CRLF）を含む行でも 1 テキスト行のままであること（値の中の改行が
//!    JSON 文字列としてエスケープされ、NDJSON が壊れないこと）を実データで確かめる。
//! 2. **1 行変更の差分**（要件 3.5。本タスクの中核）: 10 万行 × 30 列の文書のうち 1 行の
//!    1 セルだけを変更して保存し、保存された 2 つのコンテナから露出させたパート集合を
//!    比べて、**バイト列が変化したエントリが「変更した行を持つシートの
//!    `sheets/<ulid>.jsonl`」と `manifest.json` の 2 つだけ**であり、行エントリの
//!    **テキスト行数が不変で、異なる行がちょうど 1 行**であることを確かめる。
//!
//! # 「同じ識別子の文書を `open` してから変更する」理由
//!
//! `Document::new` / `add_sheet` / `add_row` は `IdFactory` から新しい ULID を発行する
//! （ULID は 48 bit ミリ秒時刻 + 80 bit ランダム値）ため、**同じ内容を 2 回新規構築した
//! 文書は識別子が違い、別の内容である**（`tests/determinism.rs` の「決定性の意味」参照）。
//! したがって「1 行だけ変えた 2 つの状態」を比べるには、片方を `open` で読み戻して
//! **識別子まで同一の文書**を得てから 1 セルだけ変更する必要がある。新規構築した 2 つの
//! 文書を比べると全行が差分になり、本テストは空振りする。
//!
//! # `manifest.json` も変化する理由（欠陥と誤解しないこと）
//!
//! `manifest.json` は全パートの索引であり、各エントリの BLAKE3 ダイジェストを記録する
//! （design「Container Entry Layout」。タスク 4.2 / 4.8）。行エントリのバイト列が 1 行分
//! 変わればそのダイジェストも変わるため、**`manifest.json` が変化するのは正しい挙動**で
//! ある。本テストが要求する「変化するエントリは 2 つだけ」は、この 2 つ
//! （変更した行を持つ `sheets/<ulid>.jsonl` と `manifest.json`）を指す。他のシートの
//! `sheets/*.jsonl` / すべての `schemas/*.json` / `document.json` / `attachments/*` は
//! **バイト単位で不変**でなければならない。
//!
//! # 10 万行の構築をパート集合経由で行う理由
//!
//! 公開 API には行の値を一括で据える経路が無く、`Document::set_row_values` は 1 回ごとに
//! シート内の行を線形探索する（O(行数)）。10 万行へ 1 行ずつ呼ぶと O(n²)（分単位）に
//! なる。一方 `parts::from_parts` は復号済みの行を `Sheet::extend_rows` の一括経路で
//! 入れる（O(n)。`document_parts.rs` の処理順 7）。したがって本テストは、30 列の骨格
//! （`to_parts` が作る `document.json` / `schemas/*` / 添付）を取り、行エントリのバイト列
//! だけを 10 万行の NDJSON に差し替えて `from_parts` で復元する。変更後は
//! `set_row_values` を**1 行につき 1 回だけ**呼ぶ（1 回なら O(n)）。
//!
//! # 標本の構成（差分の範囲を正直に測るため）
//!
//! 文書は 2 シートと 1 添付を持つ: 10 万行 × 30 列の「大量」シート（変更対象）と、
//! 2 行 × 2 列の「小」シート（**変更してはならない他シートの実体**）、および小シートの
//! 行から参照される添付である。他シート・添付・スキーマ・メタデータが不変であることを
//! 空でない実体で確かめられる。
//!
//! # 3 位置のループを別の（小さい）標本で回す理由
//!
//! 「変更した行の位置に応じて変化するテキスト行が移る」ことは、10 万行の文書で 3 位置を
//! 回すと保存（＝ 3M セルの符号化 + 36 MB 級の Deflate）を余分に 2 回走らせることになり、
//! デバッグプロファイルのテスト実行が分単位に伸びる（実測はタスク完了報告にある）。
//! 位置と差分の対応は行数に依存しないコーデックの性質であるため、10 万行での検証は
//! **代表 1 位置（中間行）**のフル経路に絞り、位置追従そのものは小さい標本
//! （[`POSITION_ROWS`] 行）で先頭・中間・最終の 3 位置を**同じ比較ヘルパ**でループして
//! 確かめる。比較規約を 2 つ持たないため、位置追従の検証も本ファイルの
//! [`assert_only_the_matching_line_changed`] が唯一の口である。
//!
//! # 観測する差分（本テストが固定する契約）
//!
//! 1. エントリ名の集合が同一であること。
//! 2. バイト列が変化したエントリの集合が `{ sheets/<大量>.jsonl, manifest.json }` に
//!    一致すること。
//! 3. 行エントリのテキスト行数が不変（== 行数）で、異なる行がちょうど 1 行であること。
//! 4. 異なる行の位置（0 始まり）が、変更した行の位置（`Sheet::rows` の文書順）と一致する
//!    こと。
//! 5. 異なる行の内容が期待どおり（変更後のセル値を含む 1 行）であること。
//! 6. 行の順序が変わらないこと（`$id` の列が前後で一致 = 差分は「移動」ではなく「置換」）。
//!
//! 一時ファイルはリポジトリ内の `tests/scratch_*` に作り、[`Scratch`] の `Drop` で削除する
//! （`tests/common/mod.rs` の docs 参照）。標本・比較ヘルパも `common` を再利用する。

mod common;

use std::fs;
use std::path::Path;

use document_format::container::ContainerCodec;
use document_format::parts::{to_parts, DocumentParts, RowsCodec};
use document_format::{
    CellValue, Document, DocumentFormatApi, EntryName, IdFactory, SchemaPart, SheetId,
};

use common::{
    api, assert_same_document, entries_of, with_rebuilt_manifest, Scratch, SCHEMA_EMPTY,
    SCHEMA_WITH_REF,
};

/// 大量シートの行数（要件 8.4 の保証対象である 10 万行）。
const BIG_ROWS: usize = 100_000;

/// 大量シートの列数（10 万行 × 30 列の文書を作る）。
const COLUMN_COUNT: usize = 30;

/// 変更するセルの列位置（0 始まり）。
const MODIFIED_COLUMN: usize = 7;

/// 位置追従のループに使う標本の行数（位置と差分の対応は行数に依存しない）。
const POSITION_ROWS: usize = 1_000;

/// 大量シートの列名（`c00` … `c29`）。行オブジェクトのキー順になる。
fn big_columns() -> Vec<String> {
    (0..COLUMN_COUNT).map(|index| format!("c{index:02}")).collect()
}

/// 大量シートの行エントリのバイト列を、決定的な整数値で組み立てる。
///
/// 1 行 = 1 テキスト行（LF 終端）の NDJSON であり、`$id` を先頭キー、続いて列順に
/// `(行位置 * 列数 + 列位置)` の整数を書く（値は行と列で一意なので、どの行が変わったかを
/// 行の内容からも特定できる）。行識別子は公開の [`IdFactory`] から発行する
/// （`RowId` は 26 文字 ULID として `$id` に載る）。
fn rows_ndjson(columns: &[String], row_count: usize) -> Vec<u8> {
    let mut ids = IdFactory::new();
    let mut out = String::with_capacity(row_count * (32 + columns.len() * 12));
    for row_index in 0..row_count {
        out.push_str("{\"$id\":\"");
        out.push_str(&ids.new_row_id().to_string());
        out.push('"');
        for (column_index, column) in columns.iter().enumerate() {
            out.push_str(",\"");
            out.push_str(column);
            out.push_str("\":");
            out.push_str(&(row_index * columns.len() + column_index).to_string());
        }
        out.push_str("}\n");
    }
    out.into_bytes()
}

/// 30 列の大量シート（0 行）と、2 行の小シート・1 添付を持つ骨格を公開 API で組み立てる。
///
/// 大量シートの行は [`document_with_rows`] がパート集合経由で差し込む（O(n) の一括経路）。
/// 小シートと添付は「変更してはならない他のエントリ」の実体である。
fn skeleton() -> Document {
    let mut document = Document::new();

    let big = document.add_sheet("大量");
    document.set_sheet_columns(big, big_columns()).expect("標本のシートは実在する");
    document
        .set_root_schema(big, SchemaPart::parse(SCHEMA_EMPTY).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    let small = document.add_sheet("小");
    document
        .set_sheet_columns(small, vec!["name".to_owned(), "blob".to_owned()])
        .expect("標本のシートは実在する");
    document
        .set_root_schema(small, SchemaPart::parse(SCHEMA_WITH_REF).expect("標本は妥当"))
        .expect("標本のシートは実在する");
    let attachment = document.add_attachment(vec![0xff, 0x00, b'j', 0x80]);
    for name in ["一", "二"] {
        let row = document.add_row(small).expect("標本のシートは実在する");
        document
            .set_row_values(
                small,
                row,
                vec![CellValue::Text(name.to_owned()), CellValue::Attachment(attachment)],
            )
            .expect("標本の行は実在する");
    }

    document
}

/// `row_count` 行 × 30 列の文書を、骨格の行エントリだけを差し替えて組み立てる
/// （モジュール docs「10 万行の構築をパート集合経由で行う理由」参照）。
fn document_with_rows(row_count: usize) -> Document {
    let skeleton = skeleton();
    let big = skeleton.sheets()[0].id();
    let parts = to_parts(&skeleton).expect("骨格はパート集合へ取り出せる");
    let version = parts.format_version();

    let mut entries = entries_of(&parts);
    let rows_entry = EntryName::Rows { sheet: big };
    let slot = entries
        .iter()
        .position(|(name, _)| *name == rows_entry)
        .expect("骨格は大量シートの行エントリを持つ");
    entries[slot].1 = rows_ndjson(&big_columns(), row_count);

    // 索引を実体から組み直してから復元する（from_parts が extend_rows の一括経路を使う）。
    let parts = DocumentParts::from_entries(with_rebuilt_manifest(version, entries))
        .expect("行を差し替えた集合は妥当");
    api().from_parts(&parts).expect("行を差し替えた集合は復元できる")
}

/// 保存されたコンテナのバイト列からパート集合を露出させ、（エントリ名, バイト列）へ写す。
///
/// `open` を経由せず `ContainerCodec::decode` を使うため、モデルを組み立て直さずに
/// **ディスクに落ちた実体**をそのまま比較できる。
fn decoded_entries(path: &Path) -> Vec<(EntryName, Vec<u8>)> {
    let bytes = fs::read(path).expect("保存されたコンテナが読める");
    let parts = ContainerCodec::decode(&bytes).expect("コンテナが復号できる");
    entries_of(&parts)
}

/// NDJSON のテキストをテキスト行へ分割する（末尾の LF は終端であり空行を生まない）。
///
/// 値の中の改行は JSON 文字列としてエスケープされるため、生の LF はレコード区切りだけに
/// 現れる（`json/ndjson.rs` の書き出し規則）。0 行のエントリ（0 バイト）は 0 行を返す。
fn text_lines(bytes: &[u8]) -> Vec<&[u8]> {
    if bytes.is_empty() {
        return Vec::new();
    }
    assert!(bytes.ends_with(b"\n"), "NDJSON は各レコードを LF で終端する");
    bytes[..bytes.len() - 1].split(|byte| *byte == b'\n').collect()
}

/// 行オブジェクトの `$id` 値（先頭キー。26 文字 ULID テキスト）を取り出す。
fn line_id(line: &[u8]) -> &[u8] {
    const PREFIX: &[u8] = b"{\"$id\":\"";
    let rest = line.strip_prefix(PREFIX).expect("行は $id を先頭キーに持つ");
    let end = rest.iter().position(|byte| *byte == b'"').expect("$id は文字列である");
    &rest[..end]
}

/// 1 行だけ変えた 2 つのパート集合を比較し、差分が「変更した行の 1 テキスト行」だけに
/// 閉じていることを確かめる（モジュール docs「観測する差分」の契約）。
///
/// `old_token` は変更前のセル値の wire 表現（十進整数）であり、変更後の行の期待内容を
/// 変更前の行から組み立てるのに使う。`expected_rows` は行エントリの行数（＝標本の行数）。
fn assert_only_the_matching_line_changed(
    before: &[(EntryName, Vec<u8>)],
    after: &[(EntryName, Vec<u8>)],
    sheet: SheetId,
    target: usize,
    old_token: &str,
    expected_rows: usize,
) {
    // 1. エントリ名の集合が同一である。
    let names_before: Vec<String> = before.iter().map(|(name, _)| name.to_string()).collect();
    let names_after: Vec<String> = after.iter().map(|(name, _)| name.to_string()).collect();
    assert_eq!(names_before, names_after, "エントリ名の集合が変わった");
    assert!(before.len() >= 6, "比較対象のエントリが少なすぎる: {}", before.len());

    // 2. バイト列が変化したエントリは行エントリと manifest.json だけである。
    let mut changed: Vec<String> = before
        .iter()
        .zip(after)
        .filter(|((_, left), (_, right))| left != right)
        .map(|((name, _), _)| name.to_string())
        .collect();
    let rows_name = EntryName::Rows { sheet }.to_string();
    let mut expected = vec![EntryName::Manifest.to_string(), rows_name];
    expected.sort();
    changed.sort();
    assert_eq!(
        changed, expected,
        "バイト列が変わったエントリが「変更したシートの行エントリ + manifest.json」に限られない \
         （他のシートの行・スキーマ・document.json・添付は不変でなければならない。要件 3.5）"
    );

    let rows_bytes = |entries: &[(EntryName, Vec<u8>)]| -> Vec<u8> {
        entries
            .iter()
            .find(|(name, _)| *name == EntryName::Rows { sheet })
            .expect("行エントリが実在する")
            .1
            .clone()
    };
    let before_rows_bytes = rows_bytes(before);
    let after_rows_bytes = rows_bytes(after);
    let before_lines = text_lines(&before_rows_bytes);
    let after_lines = text_lines(&after_rows_bytes);

    // 3. テキスト行数 == 行数（要件 3.4）で、変更前後で不変。
    assert_eq!(before_lines.len(), expected_rows, "1 行 = 1 テキスト行になっていない（要件 3.4）");
    assert_eq!(before_lines.len(), after_lines.len(), "行エントリのテキスト行数が変わった");
    assert!(
        before_lines.iter().all(|line| line.starts_with(b"{\"$id\":\"")),
        "行エントリの行が行オブジェクトの形をしていない"
    );

    // 4. 異なるテキスト行がちょうど 1 行で、その位置が変更した行の位置に一致する。
    let differing: Vec<usize> = (0..before_lines.len())
        .filter(|&index| before_lines[index] != after_lines[index])
        .collect();
    assert_eq!(
        differing, vec![target],
        "変化したテキスト行がちょうど 1 行でないか、位置が変更した行と一致しない \
         （変更した行位置 {target}。要件 3.5）"
    );

    // 5. 異なる行の内容が期待どおり（変更前の行の該当セルだけを置き換えた 1 行）である。
    let column = &big_columns()[MODIFIED_COLUMN];
    let old_pair = format!("\"{column}\":{old_token}");
    let new_pair = format!("\"{column}\":-1");
    let before_line = std::str::from_utf8(before_lines[target]).expect("行は UTF-8");
    assert!(before_line.contains(&old_pair), "変更前の行に期待したセルが無い: {before_line}");
    let expected_line = before_line.replacen(&old_pair, &new_pair, 1);
    let after_line = std::str::from_utf8(after_lines[target]).expect("行は UTF-8");
    assert_eq!(
        after_line, expected_line,
        "変化した行の内容が期待どおりでない（変更したセル以外も変わっている可能性）"
    );

    // 6. 行の順序が同一（`$id` の列が前後で一致 = 差分は「移動」ではなく「置換」）。
    let ids_before: Vec<&[u8]> = before_lines.iter().map(|line| line_id(line)).collect();
    let ids_after: Vec<&[u8]> = after_lines.iter().map(|line| line_id(line)).collect();
    assert_eq!(ids_before, ids_after, "行の順序が変わった（差分が移動として現れている）");
}

/// 対象行の 1 セルを変更して保存し、保存出力の差分を検証する一連の手順（テスト本体の共通部）。
///
/// `before` は変更前の保存出力から露出させたパート集合であり、変更のたびに
/// **同じ基準**と比較する。`row_count` は標本の行数、`target` は変更する行の位置である。
fn change_one_cell_and_check(
    scratch: &Scratch,
    row_count: usize,
    target: usize,
    before: &[(EntryName, Vec<u8>)],
    modified: &mut Document,
    sheet: SheetId,
) {
    let row_id = modified.sheets()[0].rows()[target].id();
    let original = modified.sheets()[0].rows()[target].values().to_vec();
    let expected_value = (target * COLUMN_COUNT + MODIFIED_COLUMN) as i64;
    assert_eq!(
        original[MODIFIED_COLUMN],
        CellValue::Int(expected_value),
        "行 {target} の変更対象セルの初期値が期待と違う（標本の組み立てが壊れている）"
    );

    let mut values = original.clone();
    values[MODIFIED_COLUMN] = CellValue::Int(-1);
    modified.set_row_values(sheet, row_id, values).expect("変更対象の行は実在する");

    let after_path = scratch.file(&format!("after_{row_count}_{target}.jxcel"));
    api().save(modified, &after_path).expect("変更した文書は保存できる");
    let after = decoded_entries(&after_path);
    assert_only_the_matching_line_changed(
        before,
        &after,
        sheet,
        target,
        &expected_value.to_string(),
        row_count,
    );

    // 次の位置を試すため元のセル値へ戻す（比較の基準を 1 つに保つ）。
    modified.set_row_values(sheet, row_id, original).expect("変更を戻せる");
}

/// 1 行のデータが出力テキスト上で独立した 1 行として現れる（要件 3.4）。
///
/// 値に改行（LF / CRLF）を含む行を実データに入れ、行エントリの**テキスト行数 == 行数**で
/// あること、生の LF がレコード区切りだけであること、値の中の改行がエスケープされて
/// 復号時にそのまま戻ることを確かめる。保存 → 開き直しでも値が変わらないことまで見る。
#[test]
fn one_data_row_is_always_exactly_one_text_line() {
    let mut document = Document::new();
    let sheet = document.add_sheet("行独立性");
    document
        .set_sheet_columns(sheet, vec!["text".to_owned(), "note".to_owned()])
        .expect("標本のシートは実在する");
    document
        .set_root_schema(sheet, SchemaPart::parse(SCHEMA_EMPTY).expect("標本は妥当"))
        .expect("標本のシートは実在する");

    let row_values = [
        vec![CellValue::Text("一行目\n二行目".to_owned()), CellValue::Text("a\r\nb".to_owned())],
        vec![CellValue::Int(1), CellValue::Text("plain".to_owned())],
    ];
    for values in row_values {
        let row = document.add_row(sheet).expect("標本のシートは実在する");
        document.set_row_values(sheet, row, values).expect("標本の行は実在する");
    }

    let parts = to_parts(&document).expect("標本はパート集合へ取り出せる");
    let entry = EntryName::Rows { sheet };
    let bytes = parts.get(&entry).expect("標本は行エントリを持つ").bytes.clone();

    let lines = text_lines(&bytes);
    assert_eq!(
        lines.len(),
        document.sheets()[0].rows().len(),
        "1 行のデータが 1 テキスト行として現れていない（要件 3.4）"
    );
    assert_eq!(lines.len(), 2, "2 行のデータが 2 テキスト行にならない");
    // 生の LF はレコード区切りだけ（値の中の改行はエスケープされる）。CR は生のまま出ない。
    assert_eq!(
        bytes.iter().filter(|byte| **byte == b'\n').count(),
        2,
        "生の LF が行区切り以外にある"
    );
    assert_eq!(bytes.iter().filter(|byte| **byte == b'\r').count(), 0, "CR が生のまま出力された");
    assert!(
        lines.iter().all(|line| line.starts_with(b"{\"$id\":\"")),
        "行が行オブジェクトの形をしていない"
    );

    let text = String::from_utf8(bytes.clone()).expect("行エントリは UTF-8");
    assert!(text.contains(r#"一行目\n二行目"#), "LF がエスケープされていない: {text}");
    assert!(text.contains(r#"a\r\nb"#), "CRLF がエスケープされていない: {text}");

    // 復号すると値の中の改行がそのまま戻る（エスケープは wire の都合でデータを変えない）。
    let decoded = RowsCodec::decode(&entry, &bytes).expect("行エントリは復号できる");
    assert_eq!(decoded.rows().len(), 2);
    assert_eq!(decoded.rows()[0].values()[0], CellValue::Text("一行目\n二行目".to_owned()));
    assert_eq!(decoded.rows()[0].values()[1], CellValue::Text("a\r\nb".to_owned()));

    // 保存 → 開き直しでも値が変わらない（経路全体での確認）。
    let scratch = Scratch::new("row_granular_diff_newline");
    let path = scratch.file("newline.jxcel");
    api().save(&document, &path).expect("保存できる");
    let reopened = api().open(&path).expect("開ける").document;
    assert_same_document(&document, &reopened);
}

/// 10 万行 × 30 列のうち 1 行のセルを変更して保存すると、差分が対応する 1 テキスト行のみに
/// なる（要件 3.5。本タスクの中核）。
///
/// 前後 2 つの保存出力（`pA` / `pB`）を [`ContainerCodec::decode`] で露出させ、
/// [`assert_only_the_matching_line_changed`] の契約（エントリ集合・変化エントリの範囲・
/// テキスト行数・変化行の位置と内容・行順序）を確かめる。変更するのは中間行の 1 セルで
/// ある（位置追従の 3 位置ループは
/// [`the_changed_text_line_follows_the_changed_row_position`] が担う。モジュール docs
/// 「3 位置のループを別の（小さい）標本で回す理由」参照）。
#[test]
fn changing_one_cell_in_a_large_document_changes_only_the_matching_text_line() {
    let document = document_with_rows(BIG_ROWS);
    let sheet = document.sheets()[0].id();
    assert_eq!(document.sheets()[0].rows().len(), BIG_ROWS, "10 万行の文書を組み立てられていない");

    let scratch = Scratch::new("row_granular_diff_large");
    let before_path = scratch.file("before.jxcel");
    api().save(&document, &before_path).expect("10 万行の文書は保存できる");
    let before = decoded_entries(&before_path);

    // 同じ識別子を持つ文書を開き直してから変更する（新規構築では ULID が変わる。
    // モジュール docs「同じ識別子の文書を open してから変更する理由」）。
    let mut modified = api().open(&before_path).expect("保存した文書は開ける").document;

    change_one_cell_and_check(&scratch, BIG_ROWS, BIG_ROWS / 2, &before, &mut modified, sheet);
}

/// 変更する行の位置に応じて、変化するテキスト行がその位置へ移る（要件 3.5）。
///
/// 先頭行・中間行・最終行の 3 位置をループし、それぞれの保存出力で
/// [`assert_only_the_matching_line_changed`] の「変化行がちょうど 1 行で、その位置が変更した
/// 行と一致する」ことを確かめる。行の順序が前後で一致すること（移動ではなく置換）も
/// 同じヘルパが固定する。標本の規模を [`POSITION_ROWS`] 行に抑える理由はモジュール docs
/// 「3 位置のループを別の（小さい）標本で回す理由」参照。
#[test]
fn the_changed_text_line_follows_the_changed_row_position() {
    let document = document_with_rows(POSITION_ROWS);
    let sheet = document.sheets()[0].id();

    let scratch = Scratch::new("row_granular_diff_positions");
    let before_path = scratch.file("before.jxcel");
    api().save(&document, &before_path).expect("標本は保存できる");
    let before = decoded_entries(&before_path);

    let mut modified = api().open(&before_path).expect("保存した文書は開ける").document;

    for target in [0, POSITION_ROWS / 2, POSITION_ROWS - 1] {
        change_one_cell_and_check(&scratch, POSITION_ROWS, target, &before, &mut modified, sheet);
    }
}

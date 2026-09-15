//! 表形式テキストとセル値の相互変換: [`PasteCodec`]（data-grid のタスク 3.4。要件 7.2, 7.3,
//! 7.4）。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本モジュールは `edit` 層の
//! 内側にあり、左の `types` / `view` だけを参照する**（design.md「内部の依存の向き」が
//! File Structure Plan の `edit/paste.rs` に置く「表形式テキストとセル値の相互変換」。
//! 層の鎖の文言を各層の冒頭に置く規約は `structure.md`「ドメインクレートの内部構造」）。
//!
//! # 判定の分岐を持たない
//!
//! 本モジュールは**文字列とセル値の写ししか作らない**。「その値が列の型に適合するか」も
//! 「どの値へ変換されるか」も決めない — 決めるのは `schema-engine` の規則表であり、貼り付けの
//! 適用（[`super::EditApply`]）が**変換を一括で引き、受理を書いた後の再検証に委ねる**
//! （`edit` のモジュール docs「貼り付け」）。したがって本モジュールに列の型を見る箇所は
//! 1 つも無い。
//!
//! # 規則（正典）
//!
//! | 役割 | 綴り | 意味 |
//! |---|---|---|
//! | 列の区切り | `\t`（TAB） | 同じ行の次のセルへ進む |
//! | 行の区切り | `\n`（LF）と `\r\n`（CRLF） | 次の行へ進む |
//! | 囲み | `"`（値の**先頭**にあるもの） | 囲まれた範囲では区切りが区切りにならない |
//! | 囲みの中の `""` | `""` | 1 つの `"` |
//!
//! この 2 つの区切りは、他の表計算アプリケーションがクリップボードへ置く綴りそのものである
//! （要件 7.2「行の区切りと列の区切りを持つ一般的な表形式」）。
//!
//! **`\r` 単独は行の区切りではない**（値の文字である）。`\r\n` を行の区切りとして受けるのは、
//! Windows のアプリケーションが置く綴りをそのまま貼れるようにするためであり、`\r` 単独まで
//! 区切りにすると「値の中の復帰」と「行の終わり」を区別できなくなる。
//!
//! ## 囲みの解釈（[`PasteCodec::parse`]）
//!
//! - 囲みは**値の先頭**の `"` だけで始まる。値の途中の `"` は**値の文字**である
//!   （`a"b` は 1 つの値 `a"b`。表計算アプリケーションが囲みを使わない綴りを壊さないため）。
//! - 囲みの中では `\t` / `\n` / `\r\n` が**値の文字**になり、`""` が 1 つの `"` になる。
//! - 囲みを閉じた後の文字は**同じ値へ足す**（`"ab"cd` は `abcd`）。
//! - 囲みが閉じないまま入力が終わった場合、**残り全部が値**になる。解釈は全域であり、
//!   失敗する変種を [`crate::GridError`] に足さない — 表形式テキストは「壊れる」ものではなく、
//!   綴りが違えばそう読めるだけである（利用者の貼り付けを 1 つの解釈の違いで拒まない）。
//!
//! ## 行の数え方（[`PasteCodec::parse`]）
//!
//! - 空のテキストは**行 0 件**である（貼り付けるものが無い）。
//! - 入力の**末尾**の行の区切りは**行を作らない**（`a\n` は 1 行）。他のアプリケーションが
//!   末尾に足す改行が、空の行を 1 つ余分に書くことを防ぐ。
//! - 連続する行の区切りは**空の行**として残る（`a\n\n` は 2 行目が空の値 1 つ）。
//! - 行の値の個数は**行ごとに違ってよい**（矩形の行が短い場合の扱いは適用側が決める。
//!   `super::EditApply` のモジュール docs「貼り付け」）。
//!
//! # 往復（[`PasteCodec::write`] は [`PasteCodec::parse`] の逆方向）
//!
//! 値の表示文字列は `view` 層の [`display_text`] が唯一の源である（3.1 の変換の記録と
//! 5.1 の窓の符号化が同じものを読む。本モジュールは 2 つ目の写しを作らない）。書き出しは
//! **区切りと `"` を含む値**を囲み、囲みの中の `"` を `""` へ倍にする。`\r` を含む値も囲む —
//! 囲まなければ、値の末尾の `\r` と次の行の区切りの `\n` が繋がって**行が 1 つ増える**
//! （往復が壊れる）。
//!
//! この規則により、**区切りや `"` を含む値はテキスト → セル → テキストで保たれる**
//! （design.md「Testing Strategy」の `PasteCodec`: 「表形式テキストの解釈と、行と列の区切りを
//! 含む値の往復」）。1 つだけ保たれない場合がある: **1 行 1 列でその値が空の矩形**は空の
//! テキストになり、読み直すと行 0 件になる（値なしと空のテキストは表示文字列の上で区別できず、
//! 空のテキストは「貼り付けるものが無い」であるためである）。空の値を `""` として書き出せば
//! この 1 件は救えるが、一般の表では空のセルがすべて `""` になり、他のアプリケーションへ渡す
//! 綴りとして**通常と違う**ものになる（要件 7.2 の「一般的な表形式」に反する）。したがって
//! 空の値は空の綴りで書き、この 1 件だけを既知の限界として残す。
//!
//! # 費用の形
//!
//! [`PasteCodec::parse`] は入力の文字列に対する**1 パス**であり、[`PasteCodec::write`] は
//! セル数に対する 1 パスである。どちらも判定を呼ばず、ドキュメントを見ない — 貼り付けの
//! 費用の形（1 万行で縫い目 2 回。要件 7.7, 11.5）はこの 2 つが**セル数に比例するだけ**で
//! あることの上に成り立つ。
//!
//! # 決定性
//!
//! 規則は入力の文字だけから決まり、時計・環境変数・地域設定・乱数を見ない。したがって同じ
//! テキストは常に同じ矩形になり、同じ矩形は常に同じテキストになる。

use document_format::CellValue;

use crate::view::display_text;

/// 表形式テキストとセル値の相互変換器（design.md「File Structure Plan」の `edit/paste.rs`。
/// 要件 7.2, 7.3, 7.4）。
///
/// 状態を持たない（規則はモジュール docs が唯一の源である）。[`PasteCodec::parse`] が
/// テキストを行と列の矩形へ読み、[`PasteCodec::write`] がセル値の矩形をテキストへ書く —
/// 後者は要件 7.2（複製した内容を他のアプリケーションへ渡す）の経路であり、5.2 の
/// `GridSession` が選択範囲の値をこの 1 つの写しで書き出す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteCodec;

impl PasteCodec {
    /// 表形式テキストをセルの矩形（行 → 列の値）として読む（要件 7.3, 7.4）。
    ///
    /// 規則はモジュール docs「規則（正典）」が唯一の源である。**列の型は見ない** — 値は
    /// 文字列のまま返り、セル値への写しと変換は適用の経路が行う（`edited_value` と
    /// `schema-engine` の規則表）。
    ///
    /// 空のテキストは行 0 件を返す。**失敗しない**（解釈は全域である）。
    pub fn parse(text: &str) -> Vec<Vec<String>> {
        if text.is_empty() {
            return Vec::new();
        }
        let bytes = text.as_bytes();
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut row: Vec<String> = Vec::new();
        let mut field = String::new();
        // 値の先頭に居るか（囲みはここでだけ始まる）。
        let mut at_field_start = true;
        // 直前の行の区切りの後に何かを読んだか（末尾の区切りが余分な行を作らないための印）。
        let mut started = false;
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\t' => {
                    row.push(core::mem::take(&mut field));
                    at_field_start = true;
                    started = true;
                    index += 1;
                }
                b'\n' => {
                    row.push(core::mem::take(&mut field));
                    rows.push(core::mem::take(&mut row));
                    at_field_start = true;
                    started = false;
                    index += 1;
                }
                // `\r\n` を行の区切りとして読む（1 パスで 2 バイト進める。`\r` 単独は下の腕へ
                // 落ちて値の文字になる）。
                b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                    row.push(core::mem::take(&mut field));
                    rows.push(core::mem::take(&mut row));
                    at_field_start = true;
                    started = false;
                    index += 2;
                }
                b'"' if at_field_start => {
                    index = read_quoted(text, index + 1, &mut field);
                    at_field_start = false;
                    started = true;
                }
                _ => {
                    // 区切りはすべて ASCII であり、UTF-8 の非 ASCII バイトは ASCII と
                    // 一致しないため、バイト走査で添字は常に文字の境界にある。
                    let character = next_character(text, index);
                    field.push(character);
                    index += character.len_utf8();
                    at_field_start = false;
                    started = true;
                }
            }
        }
        // 末尾の区切りの後に何か読んでいれば、最後の行を確定する（`a` は 1 行、`a\n` も 1 行）。
        if started {
            row.push(field);
            rows.push(row);
        }
        rows
    }

    /// セル値の矩形を表形式テキストへ書く（要件 7.2。[`PasteCodec::write`] の逆方向）。
    ///
    /// 値の表示文字列は [`display_text`] が唯一の源である（本モジュールは写しを持たない）。
    /// 区切り（`\t` / `\n` / `\r`）と `"` を含む値は囲み、囲みの中の `"` を `""` へ倍にする。
    /// 空の値は**空の綴り**で書く（モジュール docs「往復」の既知の限界）。
    ///
    /// 行の値の個数は行ごとに違ってよい（行は `\n` で、値は `\t` で繋ぐ）。空の矩形は
    /// 空のテキストになる（読み直すと行 0 件）。
    pub fn write(rows: &[Vec<CellValue>]) -> String {
        let mut text = String::new();
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            for (column, value) in row.iter().enumerate() {
                if column > 0 {
                    text.push('\t');
                }
                let rendered = display_text(value);
                match rendered.contains(['\t', '\n', '\r', '"']) {
                    true => push_quoted(&mut text, &rendered),
                    false => text.push_str(&rendered),
                }
            }
        }
        text
    }
}

/// 囲みの中を読み、次の位置を返す（`index` は囲みの開始の `"` の次を指す）。
///
/// `""` は 1 つの `"` として読み、次が `"` でない `"` で囲みを閉じる。閉じないまま入力が
/// 終われば**残り全部が値**になる（モジュール docs「囲みの解釈」）。区切りはすべて値の文字に
/// なる（囲みの中では区切りが区切りにならない）。
fn read_quoted(text: &str, mut index: usize, field: &mut String) -> usize {
    let bytes = text.as_bytes();
    while index < bytes.len() {
        if bytes[index] == b'"' {
            if bytes.get(index + 1) == Some(&b'"') {
                field.push('"');
                index += 2;
                continue;
            }
            // 囲みの終わり。閉じる `"` は値に入れない（その後に続く文字は値へ足される）。
            return index + 1;
        }
        let character = next_character(text, index);
        field.push(character);
        index += character.len_utf8();
    }
    index
}

/// `index` から始まる 1 文字を返す。
///
/// 呼び出し元は常に文字の境界を渡す（区切りはすべて ASCII であり、`index` は区切りの分か
/// 1 文字分だけ進む）。
fn next_character(text: &str, index: usize) -> char {
    text[index..]
        .chars()
        .next()
        .expect("添字は入力の内側の文字の境界を指す")
}

/// 値を囲みとして `text` へ書く（中の `"` を `""` へ倍にする）。
fn push_quoted(text: &mut String, value: &str) {
    text.push('"');
    for character in value.chars() {
        if character == '"' {
            text.push('"');
        }
        text.push(character);
    }
    text.push('"');
}

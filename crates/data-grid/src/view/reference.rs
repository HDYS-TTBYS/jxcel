//! 参照先のシートの行を**頁ごとに**読む（タスク 10.3。要件 3.8）。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本モジュールは `view` 層の
//! 一部であり、左の `types` と上流の `document-format` だけを参照する**（design.md
//! 「内部の依存の向き」。`view/mod.rs` と同じ位置である）。
//!
//! # 何のためにあるか
//!
//! 要件 3.8 は「参照先のシートに存在する行から選ぶ入力手段」を求める。参照先は**別の
//! シート**であり、その行数は表示中のシートの行数と無関係である（1 万行の参照先は普通に
//! ありうる）。したがって**一度に全部を読まない** — 要求は「開始位置」と「件数」を持ち、
//! 応答はその窓に閉じた行と、**総数**（続きがあるかを決める唯一の材料）を返す。
//!
//! **頁の切り出しは本モジュールが唯一の源である。**「総数を数える」「検索の文字で絞る」
//! 「開始位置から件数だけ取る」の 3 つを呼び出し側（適応層）が別々に書くと、総数と頁が
//! 食い違いうる（絞り込みの規則が 2 つになる）。件数の**上限**は境界（`app-shell` の
//! `GRID_REFERENCE_PAGE_LIMIT`）が強制する — 本モジュールは与えられた件数をそのまま使う。
//!
//! # 表示の名（`label`）
//!
//! 人が行を見分けるための文字列であり、**規則をここ 1 つに閉じる**: その行の値の表示文字列
//! （[`DisplayText`]。`view` 層が唯一の源である）を、列の順に**空白 1 つ**で連結したもので
//! ある。行そのものは名前を持たない（`document-format` の `Row` は識別子と値だけを持つ）ため、
//! 「人が読める形」を決めるのは表示を扱う本層の仕事である。
//!
//! 列を選ばない（先頭の列だけを見る、など）のは**推測**である — どの列がその行を代表するかは
//! 宣言から決まらない（`ref` の宣言は参照先の**シート**しか指さない）。すべての値を並べれば、
//! どの列の値でも行を見分けられる。
//!
//! # 検索の文字
//!
//! **空なら絞り込まない。**空でなければ、表示の名がその文字列を**含む**行だけが頁に入る
//! （規則は `view/mod.rs` の `FilterSpec::Contains` と同じ — **バイト列の一致であり、大文字と
//! 小文字を畳まない**。畳む規則を 2 つ持つと、画面に見えている文字列で絞り込めなくなる）。
//!
//! # 費用
//!
//! **総数を数えるためにすべての行を 1 度走査する**（総数は要求そのものであり、頁だけを
//! 読んで答えることはできない）。走査の内側では**確保を起こさない** — 表示の名は 1 本の
//! 緩衝へ書き直し、頁に入る行の分だけ複製する（10 万行の参照先で行ごとに `String` を作ると、
//! 要件 11.6 の費用が頁の大きさに依らなくなる）。

use core::fmt::Write as _;

use document_format::{Row, Sheet};

use super::DisplayText;

/// 参照先の 1 行（要件 3.8）。
///
/// **確定するのは `id`（行の識別子）であり、人が読むのは `label` である。**参照の値は
/// 「どの行か」を指すのであって、その行の表示文字列ではない（表示は並べ替えや絞り込みで
/// 変わる。要件 8.5、8.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceRow {
    /// 行の識別子（境界では文字列として運ぶ）。
    pub id: Box<str>,
    /// 人が行を見分けるための表示の名（モジュール docs「表示の名」）。
    pub label: String,
}

/// 参照先の行の**頁**（要件 3.8）。
///
/// `total` は**絞り込んだあとの総数**であり、`rows` はそのうち `start` から `count` 件である。
/// `has_more` は「`rows` の後ろにまだ行があるか」であり、`total` と `rows.len()` と `start` から
/// 導出する（3 つを別々に数える経路を作らない）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencePage {
    /// この頁の行（文書の行順）。
    pub rows: Vec<ReferenceRow>,
    /// 絞り込んだあとの総数（この頁の大きさではない）。
    pub total: usize,
    /// この頁の後ろにまだ行があるか。
    pub has_more: bool,
}

impl ReferencePage {
    /// 行が 1 つも無い頁（**材料が無い**ことを表す。`total` は 0 である）。
    #[inline]
    #[must_use]
    pub fn empty() -> Self {
        Self {
            rows: Vec::new(),
            total: 0,
            has_more: false,
        }
    }
}

/// 参照先のシートの行から、検索の文字に一致する行の頁を組む（要件 3.8）。
///
/// 規則はモジュール docs の 3 節（表示の名・検索の文字・費用）が唯一の源である。
/// `sheet` は参照先のシートであり、**表示中のシートではない**（呼び出し側が宣言の参照先から
/// 引く）。行の並びは文書の行順であり、並べ替えや絞り込みの影響を受けない（あれらは表示の
/// 状態であり、参照先の選び方ではない。要件 8.5）。
#[must_use]
pub fn reference_page(sheet: &Sheet, search: &str, start: usize, count: usize) -> ReferencePage {
    let mut rows = Vec::new();
    let mut total = 0_usize;
    // 走査の内側では 1 本の緩衝を使い回す（行ごとに確保しない。モジュール docs「費用」）。
    let mut label = String::new();
    for row in sheet.rows() {
        label_of(row, &mut label);
        if !search.is_empty() && !label.contains(search) {
            continue;
        }
        // **総数は絞り込んだあとのすべての行を数える**（頁の外も数える）。
        if total >= start && rows.len() < count {
            rows.push(ReferenceRow {
                id: row.id().to_string().into_boxed_str(),
                label: label.clone(),
            });
        }
        total += 1;
    }

    ReferencePage {
        has_more: start.saturating_add(rows.len()) < total,
        rows,
        total,
    }
}

/// 行の表示の名を `buffer` へ書く（規則はモジュール docs「表示の名」）。
///
/// 表示文字列の規則は [`DisplayText`] が唯一の源である（本関数は並べ方だけを決める —
/// 値の種別ごとの書き方は `view/mod.rs` の表のままである）。
fn label_of(row: &Row, buffer: &mut String) {
    buffer.clear();
    for (index, value) in row.values().iter().enumerate() {
        if index > 0 {
            buffer.push(' ');
        }
        // `fmt::Write` は `String` への書き込みで失敗しない。
        let _ = write!(buffer, "{}", DisplayText(value));
    }
}

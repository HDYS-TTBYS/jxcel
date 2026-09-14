//! 本クレートの誤り型: [`GridError`]。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本層は鎖の最も左であり、
//! 本クレートの他のどの層にも依存しない**（design.md「内部の依存の向き」。層の鎖の文言を
//! 各層の冒頭に置く規約は `structure.md`「ドメインクレートの内部構造」）。
//!
//! # 宣言の誤りと値の不適合を混ぜない
//!
//! `structure.md` の規律に従い、**「宣言・入力が壊れている」と「値が合わない」を別の型に
//! する**。前者は処理を止め、後者は止めない。1 つの型にすると「1 件の不正な値で全体が
//! 開けない」という振る舞いが型として表現できてしまう。
//!
//! したがって本モジュールが持つのは前者だけである（design.md「Error Handling」の 5 変種）。
//! 判定の誤りは `document-format` の [`CellWriteError`](document_format::CellWriteError) /
//! [`RowRemovalError`](document_format::RowRemovalError) /
//! [`RowInsertionError`](document_format::RowInsertionError) と同じ規律の判別可能な
//! 列挙体であり、**診断に必要な文脈だけを持ち、表示用の文言を持たない**。文言は呼び出し元
//! （境界の封筒を組み立てる適応層）が組み立てる。`Display` は持ち物を機械的に写すだけである。
//!
//! # 依存を足さない理由
//!
//! 兄弟クレートは `thiserror` 2.0 で同じ規律を書いているが、本クレートの `Cargo.toml` は
//! 「依存してよい兄弟は `document-format` と `schema-engine` の 2 つだけ」であり、依存の
//! 追加は依存方針の変更を意味する。5 変種の `Display` は 30 行に満たず（実測 27 行）、
//! `document-format` の `IdParseError` が既に手書きの前例であるため、**手書きを選ぶ**。

use core::fmt;

use document_format::{RowId, SheetId};

use crate::types::{CellAddress, ColumnIndex, RowSpan};

/// 表示状態を進められない誤り（宣言・指定が壊れている。処理を止める）。
///
/// **値の不適合はここに含まれない。**違反は `schema-engine` の
/// [`Violation`](schema_engine::Violation) として運ばれ、**処理を止めない**。
/// 型に適合しない値は破棄されずドキュメントに残り、違反として提示されるだけである
/// （要件 3.5）。この 2 つを 1 つの型に混ぜると、編集を 1 件拒否する経路が「値が壊れて
/// いる」と同じ型で表現できてしまう（`structure.md`「ドメインクレートの内部構造」）。
///
/// 5 変種は互いに判別可能である。診断に要る文脈（どのシート・どの行・どの列・どの区間・
/// どのセル）だけを持ち、提示の文言は持たない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GridError {
    /// 宣言が使えず、セッションを開けない（列が 1 件も宣言されていない等）。
    SchemaUnusable {
        /// 対象のシート識別子。
        sheet: SheetId,
    },
    /// 指定された行がシートに属さない（他シートの行・削除済みの行）。
    UnknownRow {
        /// 指定されたが存在しなかった行識別子。
        row: RowId,
    },
    /// 列の添字が列の数を超えている。
    ColumnOutOfRange {
        /// 指定された列の添字。
        column: ColumnIndex,
        /// 対象シートが持つ列の数（範囲の上界）。
        count: usize,
    },
    /// 要求された行の区間が可視行の範囲を超えている。
    SpanOutOfRange {
        /// 要求された区間（可視行の序数）。
        span: RowSpan,
        /// そのときの可視行数。
        visible: usize,
    },
    /// 入れ子の値を構造として解釈できなかった。
    NestedDecode {
        /// 解釈できなかったセルの位置。
        cell: CellAddress,
    },
}

// `Display` は**持ち物を機械的に写すだけ**である（`CellWriteError` / `RowRemovalError` /
// `RowInsertionError` の `thiserror` の書式と同じ形）。提示の文言は呼び出し元が組み立てる。
impl fmt::Display for GridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaUnusable { sheet } => {
                write!(f, "schema of sheet {sheet} is unusable")
            }
            Self::UnknownRow { row } => write!(f, "no row {row} in sheet"),
            Self::ColumnOutOfRange { column, count } => write!(
                f,
                "column {} is out of range: sheet has {count} columns",
                column.index()
            ),
            Self::SpanOutOfRange { span, visible } => write!(
                f,
                "span {}..{} is out of range: {visible} rows are visible",
                span.start(),
                span.end()
            ),
            Self::NestedDecode { cell } => write!(
                f,
                "nested value of cell ({}, {}) is not decodable",
                cell.row(),
                cell.column().index()
            ),
        }
    }
}

impl std::error::Error for GridError {}

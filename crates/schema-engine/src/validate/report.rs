//! 違反の表現・順序・上限（design.md「コンポーネントとファイルの対応」の
//! `ViolationReport`。tasks.md 1.3。要件 5.1, 5.2, 5.6）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは `compile` 層までを参照でき、`write` 以降を参照しない。
//! `validate` 層の中では**最も下流に置かれる葉**であり、同じ層の
//! [`cell`](super::cell) / [`unique`](super::unique) / [`refs`](super::refs) と、本層の
//! 検証の入口（タスク 5.4）が本モジュールの型を組み立てる。本モジュール自身は `validate`
//! 層の他のサブモジュールにも上流の層にも依存しない（`document-format` の値と識別子、
//! および `std` だけを使う）。
//!
//! # 「誤り」と「違反」を型で分ける（design.md「Components and Interfaces」）
//!
//! [`SchemaError`](crate::error::SchemaError) は**宣言が壊れている**ことを表し、
//! コンパイルを止める。本モジュールの [`Violation`] は**値が宣言に合わない**ことを表し、
//! 何も止めない。この 2 つを 1 つの型にすると、「1 件の不正な値でシート全体が開けない」
//! という振る舞いが型の上で表現できてしまう（design.md 同節）。したがって本モジュールは
//! `error` 層の型を参照せず、`error` 層も本モジュールを参照しない。
//!
//! # 文脈だけを持ち、表示用の文言を持たない
//!
//! [`ViolationReason`] は**期待した内容**（[`Expected`]）と**実際の値**
//! （[`CellValue`]）の双方を構造として保持し（要件 5.2）、`Display` を実装しない。
//! 利用者向けの提示（ロケール化メッセージを含む）は、呼び出し元が `match` で変種と文脈を
//! 取り出して組み立てる（`document-format` の `DocumentError` と同じ規約。本スペックは
//! UI を持たない。design.md「Error Handling」の Out of Boundary）。
//!
//! # 上流のキー順への依存
//!
//! [`Violation::row`] が `None` になるのは**列そのものの問題**であるときだけであり
//! （design.md「検証結果の表現」の `row: Option<RowId>`）、値に属する違反は必ず行を
//! 持つ（要件 5.1）。
//!
//! # 上限を持つ理由（design.md「検証結果の表現」）
//!
//! 10 万行 × 30 列がすべて違反しうる。理由は実際の値を複製して持つため、上限がなければ
//! 報告そのものが予算と記憶域を食う。したがって違反は [`ValidationOptions`] が指定する
//! 件数まで [`ViolationReport`] が蓄積し、**総件数だけは上限を超えても数え続ける**
//! （要件 5.6）。違反を持つ行の一覧は上限に依らず保つ（行あたり 1 件であり、呼び出し元が
//! 印を付けるために必要である）。

use document_format::{CellValue, RowId, SheetId};

// 列の添字の定義は `compile` 層にある（列添字を所有するのは、その添字で引ける配列
// `columns` と `validators` を持つ側である）。本層はその型を参照する側であり、層の鎖
// （`… → compile → { coerce, validate } → …`）と向きが一致する。
use crate::compile::plan::ColumnIndex;

/// 入れ子の内側の位置の 1 段（design.md「検証結果の表現」の `ValuePath` の要素）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValuePathSegment {
    /// オブジェクトのフィールド名。
    Field(Box<str>),
    /// 配列の 0 起点の要素位置。
    Index(usize),
}

/// 入れ子の内側の位置。フィールド名と添字の並びであり、**空ならセル直下**を指す
/// （design.md「検証結果の表現」。要件 3.4 の位置特定を支える）。
///
/// 入れ子を再帰する検証（[`cell`](super::cell)）は、降りるときに [`ValuePath::push_field`] /
/// [`ValuePath::push_index`] を積み、戻るときに [`ValuePath::pop`] で畳む。セル直下の
/// 違反は [`ValuePath::root`] を使う。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValuePath(Vec<ValuePathSegment>);

impl ValuePath {
    /// セル直下を指す空の位置。
    #[inline]
    pub const fn root() -> Self {
        Self(Vec::new())
    }

    /// 位置の並び。
    #[inline]
    pub fn segments(&self) -> &[ValuePathSegment] {
        &self.0
    }

    /// 位置の段数。
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 位置が空であるか（[`ValuePath::is_root`] と同じ）。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 位置が空であり、セル直下を指すか。
    #[inline]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// 1 段降りる: オブジェクトのフィールド名を積む。
    #[inline]
    pub fn push_field(&mut self, name: impl Into<Box<str>>) {
        self.0.push(ValuePathSegment::Field(name.into()));
    }

    /// 1 段降りる: 配列の 0 起点の要素位置を積む。
    #[inline]
    pub fn push_index(&mut self, index: usize) {
        self.0.push(ValuePathSegment::Index(index));
    }

    /// 1 段戻る。
    #[inline]
    pub fn pop(&mut self) -> Option<ValuePathSegment> {
        self.0.pop()
    }
}

impl From<Vec<ValuePathSegment>> for ValuePath {
    #[inline]
    fn from(segments: Vec<ValuePathSegment>) -> Self {
        Self(segments)
    }
}

/// 違反の理由のうち、**期待した内容**（宣言側から来る文脈。要件 5.2）。
///
/// 表示用の文言を持たない。呼び出し元は `match` で種別を取り出し、提示を組み立てる。
#[derive(Debug, Clone, PartialEq)]
pub enum Expected {
    /// 型の種類（design.md「組込型カタログと `CellValue` への写像」の `kind` トークン）。
    Kind(Box<str>),
    /// 値の範囲。開いている側は `None`。
    Range {
        /// 下限（含む）。
        min: Option<CellValue>,
        /// 上限（含む）。
        max: Option<CellValue>,
    },
    /// 文字列の長さの範囲（design.md の `minLength` / `maxLength`）。開いている側は `None`。
    Length {
        /// 最小長。
        min: Option<usize>,
        /// 最大長。
        max: Option<usize>,
    },
    /// 文字列の書式（宣言されたパターンの生文字列）。
    Pattern(Box<str>),
    /// 10 進数の有効桁数と小数点以下の桁数（design.md の `precision` / `scale`）。
    Decimal {
        /// 有効桁数。
        precision: u32,
        /// 小数点以下の桁数。
        scale: u32,
    },
    /// 列挙の選択肢。
    Choices(Vec<Box<str>>),
    /// 値なしを許さないこと（design.md の `required`）。
    Present,
    /// 一意であること（design.md の `unique`）。
    Unique,
    /// 参照先シート（design.md の `ref` の `sheet`）に実在する行を指すこと。
    RowsOf(SheetId),
    /// 列が使用可能であること（未知の種別・未登録の拡張型では使用不能になる。
    /// design.md「解決できない型の扱い」）。
    Usable,
    /// 拡張型（識別子）が受理すること。
    AcceptedBy(Box<str>),
}

/// 値が宣言に合わない理由（design.md「検証結果の表現」の `ViolationReason`。要件 5.2）。
///
/// 変種は**期待した内容**（[`Expected`]）と**実際の値**（[`CellValue`]）の双方を持ち、
/// 表示用の文言を持たない（モジュール docs 参照）。行を跨ぐ性質の判定から生じる変種
/// （[`ViolationReason::Duplicate`] / [`ViolationReason::BrokenReference`]）は、判定に
/// 必要な対象（重複するすべての行、参照先のシート）を追加で持つ。
#[derive(Debug, Clone, PartialEq)]
pub enum ViolationReason {
    /// 値の変種が列の型と食い違う（要件 2.7。design.md「Compile Layer / ColumnValidator」）。
    TypeMismatch {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
    },
    /// 値が宣言された範囲の外にある（要件 4.5）。
    OutOfRange {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
    },
    /// 値の長さが宣言された範囲の外にある（要件 4.5）。
    LengthOutOfRange {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
    },
    /// 値が宣言された書式に一致しない（要件 4.5）。
    PatternMismatch {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
    },
    /// 値が列挙の選択肢にない（要件 4.5）。
    ChoiceNotAllowed {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
    },
    /// 10 進数の値が宣言された有効桁数または小数点以下の桁数を超える（要件 2.3）。
    PrecisionExceeded {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
    },
    /// 値なしを許さない列またはフィールドに値なしがある（要件 4.4）。
    MissingValue {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
    },
    /// 一意制約を持つ列に重複する値がある（要件 4.7）。
    Duplicate {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
        /// 重複するすべての行の識別子（要件 4.7）。
        rows: Vec<RowId>,
    },
    /// 参照先の行またはシートが実在しない（要件 9.3, 9.5）。
    BrokenReference {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値（参照先の識別子）。
        actual: CellValue,
        /// 参照先のシート。
        sheet: SheetId,
    },
    /// 列が使用不能である（未知の種別・未登録の拡張型。要件 11.7）。
    /// その列の値はすべてこの理由になる（design.md「解決できない型の扱い」）。
    UnusableColumn {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
        /// 解釈できなかった宣言の種別（未知の `kind`・未登録の拡張型の識別子・展開できない
        /// 再帰型の識別子）。
        kind: Box<str>,
    },
    /// 拡張型が値を拒否した（要件 11.3）。
    CustomRejected {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
        /// 拡張型が返した理由（表示用の文言ではなく、実装から渡された文脈）。
        reason: Box<str>,
    },
    /// 拡張型の判定が失敗した（panic・打ち切り。要件 11.5）。
    /// その値の違反に閉じ込め、シート全体の検証を中断しない。
    CustomFailed {
        /// 期待した内容。
        expected: Expected,
        /// 実際の値。
        actual: CellValue,
        /// 拡張型が返した理由（表示用の文言ではなく、実装から渡された文脈）。
        reason: Box<str>,
    },
}

impl ViolationReason {
    /// 期待した内容（要件 5.2）。
    pub fn expected(&self) -> &Expected {
        match self {
            Self::TypeMismatch { expected, .. }
            | Self::OutOfRange { expected, .. }
            | Self::LengthOutOfRange { expected, .. }
            | Self::PatternMismatch { expected, .. }
            | Self::ChoiceNotAllowed { expected, .. }
            | Self::PrecisionExceeded { expected, .. }
            | Self::MissingValue { expected, .. }
            | Self::Duplicate { expected, .. }
            | Self::BrokenReference { expected, .. }
            | Self::UnusableColumn { expected, .. }
            | Self::CustomRejected { expected, .. }
            | Self::CustomFailed { expected, .. } => expected,
        }
    }

    /// 実際の値（要件 5.2）。
    pub fn actual(&self) -> &CellValue {
        match self {
            Self::TypeMismatch { actual, .. }
            | Self::OutOfRange { actual, .. }
            | Self::LengthOutOfRange { actual, .. }
            | Self::PatternMismatch { actual, .. }
            | Self::ChoiceNotAllowed { actual, .. }
            | Self::PrecisionExceeded { actual, .. }
            | Self::MissingValue { actual, .. }
            | Self::Duplicate { actual, .. }
            | Self::BrokenReference { actual, .. }
            | Self::UnusableColumn { actual, .. }
            | Self::CustomRejected { actual, .. }
            | Self::CustomFailed { actual, .. } => actual,
        }
    }
}

/// 違反 1 件（design.md「検証結果の表現」の `Violation`。要件 5.1）。
///
/// 位置は 4 つに分かれる: 行の識別子（[`Violation::row`]）、列の添字
/// （[`Violation::column`]）、列名（[`Violation::column_name`]）、入れ子の内側の位置
/// （[`Violation::path`]）。行が `None` なのは**列そのものの問題**であるときだけである。
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    row: Option<RowId>,
    column: ColumnIndex,
    column_name: Box<str>,
    path: ValuePath,
    reason: ViolationReason,
}

impl Violation {
    /// 位置と理由を明示して 1 件を組み立てる（入れ子の内側の違反）。
    pub fn new(
        row: Option<RowId>,
        column: ColumnIndex,
        column_name: impl Into<Box<str>>,
        path: ValuePath,
        reason: ViolationReason,
    ) -> Self {
        Self {
            row,
            column,
            column_name: column_name.into(),
            path,
            reason,
        }
    }

    /// セル直下の違反を組み立てる（[`ValuePath::root`] を使う短縮形）。
    pub fn at_cell(
        row: Option<RowId>,
        column: ColumnIndex,
        column_name: impl Into<Box<str>>,
        reason: ViolationReason,
    ) -> Self {
        Self::new(row, column, column_name, ValuePath::root(), reason)
    }

    /// 違反の属する行。`None` は列そのものの問題を表す。
    #[inline]
    pub fn row(&self) -> Option<RowId> {
        self.row
    }

    /// 列の添字。
    #[inline]
    pub fn column(&self) -> ColumnIndex {
        self.column
    }

    /// 列名（`ColumnIndex` だけでは宣言が変わった後に対応を失うため、名前も持つ）。
    #[inline]
    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    /// 入れ子の内側の位置。空ならセル直下である。
    #[inline]
    pub fn path(&self) -> &ValuePath {
        &self.path
    }

    /// 違反の理由（期待した内容と実際の値）。
    #[inline]
    pub fn reason(&self) -> &ViolationReason {
        &self.reason
    }
}

/// 検証の呼び出し方が指定する条件（design.md「Public API Layer」の `ValidationOptions`。
/// 要件 5.6）。
///
/// 指定できるのは**違反の件数の上限**である。上限に達したあとも総件数は数え続けるため、
/// 呼び出し元は「上限までの違反」と「総件数」の双方を得る（[`SheetReport`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationOptions {
    violation_limit: Option<usize>,
}

impl ValidationOptions {
    /// 上限を設けない。すべての違反を保持する。
    #[inline]
    pub const fn unlimited() -> Self {
        Self {
            violation_limit: None,
        }
    }

    /// 違反を `limit` 件まで保持する。`0` は違反を保持しないが、総件数と違反を持つ行の
    /// 一覧は保たれる。
    #[inline]
    pub const fn capped(limit: usize) -> Self {
        Self {
            violation_limit: Some(limit),
        }
    }

    /// 指定された上限。`None` は無制限である。
    #[inline]
    pub const fn violation_limit(&self) -> Option<usize> {
        self.violation_limit
    }
}

impl Default for ValidationOptions {
    /// 上限を指定しない既定は**無制限**である。上限を設けるかどうかは呼び出し元が決める
    /// （design.md「検証結果の表現」: 上限は 10 万行規模の報告から記憶域を守るためにある）。
    #[inline]
    fn default() -> Self {
        Self::unlimited()
    }
}

/// シート全体の検証結果（design.md「検証結果の表現」の `SheetReport`。要件 5.4, 5.6）。
///
/// 違反は**保持されない**（宣言と値から常に導出される。design.md「Data Models / 不変条件」）
/// ため、この型はある時点の検証の結果そのものである。
#[derive(Debug, Clone, PartialEq)]
pub struct SheetReport {
    sheet: SheetId,
    violations: Vec<Violation>,
    total_violations: usize,
    invalid_rows: Vec<RowId>,
}

impl SheetReport {
    /// 検証したシート。
    #[inline]
    pub fn sheet(&self) -> SheetId {
        self.sheet
    }

    /// 上限までの違反。`total_violations` より少ないことがある。
    #[inline]
    pub fn violations(&self) -> &[Violation] {
        &self.violations
    }

    /// 上限を超えても数え続けた違反の総件数（要件 5.6）。
    #[inline]
    pub fn total_violations(&self) -> usize {
        self.total_violations
    }

    /// 違反を持つ行の識別子。行の並び順であり、重複しない（要件 5.4）。
    /// 違反の保持が上限で打ち切られていても、この一覧は打ち切られない。
    #[inline]
    pub fn invalid_rows(&self) -> &[RowId] {
        &self.invalid_rows
    }

    /// 上限によって違反の保持が打ち切られたか（[`Self::total_violations`] が
    /// [`Self::violations`] の長さを超えているか）。
    #[inline]
    pub fn is_truncated(&self) -> bool {
        self.total_violations > self.violations.len()
    }
}

/// 上限と総件数を数えながら違反を蓄積する（design.md「コンポーネントとファイルの対応」の
/// `ViolationReport`。要件 5.6）。
///
/// 検証の走査（タスク 5.4）が本型へ違反を押し込み、最後に [`ViolationReport::finish`] で
/// [`SheetReport`] に確定する。**上限に達したあとも押し込みは受け付け、総件数と違反を持つ
/// 行の一覧だけを更新する**（報告の大きさを上限に閉じ込めつつ、数え落とさないため）。
///
/// 蓄積そのものは**シートに属さない**。違反が属するシートが決まるのは確定のときであり、
/// 書き込み経路（タスク 6.2）は行に属さない値（`row: None`）を集めるため、蓄積の段が
/// シートを要求すると判定の入口が無くなる。したがってシートは
/// [`ViolationReport::finish`] へ渡す。
pub struct ViolationReport {
    limit: Option<usize>,
    violations: Vec<Violation>,
    total: usize,
    invalid_rows: Vec<RowId>,
}

impl ViolationReport {
    /// 条件を与えて蓄積を始める。
    pub fn new(options: &ValidationOptions) -> Self {
        Self {
            limit: options.violation_limit(),
            violations: Vec::new(),
            total: 0,
            invalid_rows: Vec::new(),
        }
    }

    /// 違反を 1 件加える。
    ///
    /// **行は行の並び順で押し込むこと**（design.md「検証結果の表現」の順序の決定性:
    /// 値の違反は行の並び順 → 列添字 → 入れ子の位置で出て、行を跨ぐ判定の結果は最後に
    /// 同じ基準で安定併合される）。`RowId` からはシート上の行順を復元できないため、
    /// 一覧の並びは押し込み順が源である。
    pub fn push(&mut self, violation: Violation) {
        self.total += 1;

        if let Some(row) = violation.row() {
            // 同じ行の 2 件目以降は一覧へ足さない（押し込み順が行順なので直前と比べれば足りる）。
            if self.invalid_rows.last() != Some(&row) {
                self.invalid_rows.push(row);
            }
        }

        if self.limit.is_none_or(|limit| self.violations.len() < limit) {
            self.violations.push(violation);
        }
    }

    /// 違反をまとめて加える（[`ViolationReport::push`] と同じ規則）。
    pub fn extend(&mut self, violations: impl IntoIterator<Item = Violation>) {
        for violation in violations {
            self.push(violation);
        }
    }

    /// 蓄積した違反を取り出し、総件数と違反を持つ行の一覧を空に戻す（タスク 5.4）。
    ///
    /// 一括検証（`validate` 層の入口）が**行ごとに**第 1 段の違反を取り出し、第 2 段の違反と
    /// 併合してから最終の報告へ押し込み直すために使う。行を跨いで第 1 段の違反をためると、
    /// 上限が守るはずの記憶域が上限の外で膨らむ（モジュール docs「上限を持つ理由」）。
    /// 取り出した [`Vec`] は空のとき確保を行っていないため、違反の無い行では確保が起きない。
    pub(crate) fn take_violations(&mut self) -> Vec<Violation> {
        self.total = 0;
        self.invalid_rows.clear();
        std::mem::take(&mut self.violations)
    }

    /// 蓄積した違反をそのまま取り出す（書き込み経路。タスク 6.2）。
    ///
    /// シートに属さない判定（[`ViolationReport::new`] で集めたもの）の結果を取り出す唯一の
    /// 経路である。違反の位置と理由（要件 6.4）だけを返し、総件数と違反を持つ行の一覧は
    /// 捨てる — 行に属さない値の違反は行の一覧を持たないためである。
    pub(crate) fn into_violations(self) -> Vec<Violation> {
        self.violations
    }

    /// ここまでに加えた違反の総件数。
    #[inline]
    pub fn total(&self) -> usize {
        self.total
    }

    /// 結果を確定する。違反が属するシートは確定のときに与える。
    pub fn finish(self, sheet: SheetId) -> SheetReport {
        SheetReport {
            sheet,
            violations: self.violations,
            total_violations: self.total,
            invalid_rows: self.invalid_rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    /// 標本の行識別子（ULID の正準テキスト形）。
    const ROW_A: &str = "01K4ANRRG004HMASW9NF6YY101";
    /// 標本の行識別子（ULID の正準テキスト形）。重複と入れ子の標本に使う。
    const ROW_B: &str = "01K4ANRRG004HMASW9NF6YY102";

    fn sheet_id() -> SheetId {
        SheetId::from_str("01K4ANRRG004HMASW9NF6YY091").expect("標本のシート識別子が解析できない")
    }

    fn row_id(text: &str) -> RowId {
        RowId::from_str(text).expect("標本の行識別子が解析できない")
    }

    fn text(value: &str) -> CellValue {
        CellValue::Text(value.to_owned())
    }

    /// 1 セルの型不一致の違反（入れ子の位置はセル直下）。
    fn type_mismatch(row: RowId, column: usize, column_name: &str) -> Violation {
        Violation::at_cell(
            Some(row),
            ColumnIndex::new(column),
            column_name,
            ViolationReason::TypeMismatch {
                expected: Expected::Kind("int".into()),
                actual: text("abc"),
            },
        )
    }

    /// 違反 1 件が位置（行・列の添字と名前・入れ子の位置）と理由を保持する（要件 5.1）。
    #[test]
    fn a_violation_carries_row_column_name_path_and_reason() {
        let row = row_id(ROW_A);
        let mut path = ValuePath::root();
        path.push_field("明細");
        path.push_index(1);

        let violation = Violation::new(
            Some(row),
            ColumnIndex::new(3),
            "数量",
            path,
            ViolationReason::TypeMismatch {
                expected: Expected::Kind("int".into()),
                actual: text("abc"),
            },
        );

        assert_eq!(Some(row), violation.row(), "行の識別子が保持されていない");
        assert_eq!(ColumnIndex::new(3), violation.column());
        assert_eq!(3, violation.column().index(), "列の添字が 0 起点でない");
        assert_eq!("数量", violation.column_name(), "列名が保持されていない");
        assert_eq!(
            vec![
                ValuePathSegment::Field("明細".into()),
                ValuePathSegment::Index(1),
            ],
            violation.path().segments().to_vec(),
            "入れ子の内側の位置が保持されていない"
        );
        assert!(
            matches!(
                violation.reason(),
                ViolationReason::TypeMismatch {
                    expected: Expected::Kind(_),
                    ..
                }
            ),
            "違反の理由が保持されていない"
        );

        // 列そのものの問題は行を持たない（design.md「検証結果の表現」）。
        let column_level = Violation::at_cell(
            None,
            ColumnIndex::new(0),
            "備考",
            ViolationReason::UnusableColumn {
                expected: Expected::Usable,
                actual: text("不明な種別の値"),
                kind: "future-kind".into(),
            },
        );
        assert_eq!(None, column_level.row());
        assert!(
            column_level.path().is_root(),
            "セル直下の違反の位置が空でない"
        );
    }

    /// 入れ子の内側の位置がフィールド名と添字の並びで表現でき、空ならセル直下を指す
    /// （tasks.md 1.3。要件 3.4 の位置特定を支える）。
    #[test]
    fn nested_path_is_a_sequence_of_field_names_and_indices() {
        let root = ValuePath::root();
        assert!(root.is_root(), "空の位置がセル直下を指していない");
        assert!(root.is_empty());
        assert_eq!(0, root.len());
        assert_eq!(0, root.segments().len());
        assert_eq!(ValuePath::default(), ValuePath::root());

        let mut path = ValuePath::root();
        path.push_field("属性");
        path.push_index(0);
        path.push_field("色");

        assert_eq!(3, path.len());
        assert!(!path.is_root());
        assert_eq!(
            vec![
                ValuePathSegment::Field("属性".into()),
                ValuePathSegment::Index(0),
                ValuePathSegment::Field("色".into()),
            ],
            path.segments().to_vec(),
            "フィールド名と添字の並びが保たれていない"
        );

        // 再帰から戻るときに 1 段ずつ畳める（入れ子の再帰がこの操作で位置を組み立てる）。
        assert_eq!(Some(ValuePathSegment::Field("色".into())), path.pop());
        assert_eq!(Some(ValuePathSegment::Index(0)), path.pop());
        assert_eq!(Some(ValuePathSegment::Field("属性".into())), path.pop());
        assert!(path.is_root());
        assert_eq!(None, path.pop());
    }

    /// 違反の理由のすべての変種が、期待した内容と実際の値の双方を保持する（要件 5.2）。
    #[test]
    fn every_reason_variant_carries_expected_and_actual() {
        let sheet = sheet_id();
        let row_a = row_id(ROW_A);
        let row_b = row_id(ROW_B);

        let cases: Vec<(ViolationReason, Expected, CellValue)> = vec![
            (
                ViolationReason::TypeMismatch {
                    expected: Expected::Kind("int".into()),
                    actual: text("abc"),
                },
                Expected::Kind("int".into()),
                text("abc"),
            ),
            (
                ViolationReason::OutOfRange {
                    expected: Expected::Range {
                        min: Some(CellValue::Int(0)),
                        max: Some(CellValue::Int(100)),
                    },
                    actual: CellValue::Int(-1),
                },
                Expected::Range {
                    min: Some(CellValue::Int(0)),
                    max: Some(CellValue::Int(100)),
                },
                CellValue::Int(-1),
            ),
            (
                ViolationReason::LengthOutOfRange {
                    expected: Expected::Length {
                        min: Some(2),
                        max: Some(8),
                    },
                    actual: text("abcdefghi"),
                },
                Expected::Length {
                    min: Some(2),
                    max: Some(8),
                },
                text("abcdefghi"),
            ),
            (
                ViolationReason::PatternMismatch {
                    expected: Expected::Pattern("^[0-9]{3}$".into()),
                    actual: text("12a"),
                },
                Expected::Pattern("^[0-9]{3}$".into()),
                text("12a"),
            ),
            (
                ViolationReason::ChoiceNotAllowed {
                    expected: Expected::Choices(vec!["赤".into(), "青".into()]),
                    actual: text("緑"),
                },
                Expected::Choices(vec!["赤".into(), "青".into()]),
                text("緑"),
            ),
            (
                ViolationReason::PrecisionExceeded {
                    expected: Expected::Decimal {
                        precision: 12,
                        scale: 2,
                    },
                    actual: CellValue::Decimal("1.234".to_owned()),
                },
                Expected::Decimal {
                    precision: 12,
                    scale: 2,
                },
                CellValue::Decimal("1.234".to_owned()),
            ),
            (
                ViolationReason::MissingValue {
                    expected: Expected::Present,
                    actual: CellValue::Null,
                },
                Expected::Present,
                CellValue::Null,
            ),
            (
                ViolationReason::Duplicate {
                    expected: Expected::Unique,
                    actual: text("A-1"),
                    rows: vec![row_a, row_b],
                },
                Expected::Unique,
                text("A-1"),
            ),
            (
                ViolationReason::BrokenReference {
                    expected: Expected::RowsOf(sheet),
                    actual: text("01K4ANRRG004HMASW9NF6YY199"),
                    sheet,
                },
                Expected::RowsOf(sheet),
                text("01K4ANRRG004HMASW9NF6YY199"),
            ),
            (
                ViolationReason::UnusableColumn {
                    expected: Expected::Usable,
                    actual: text("不明な種別の値"),
                    kind: "future-kind".into(),
                },
                Expected::Usable,
                text("不明な種別の値"),
            ),
            (
                ViolationReason::CustomRejected {
                    expected: Expected::AcceptedBy("postal-code".into()),
                    actual: text("000-0000"),
                    reason: "書式が合わない".into(),
                },
                Expected::AcceptedBy("postal-code".into()),
                text("000-0000"),
            ),
            (
                ViolationReason::CustomFailed {
                    expected: Expected::AcceptedBy("postal-code".into()),
                    actual: text("123-4567"),
                    reason: "応答がない".into(),
                },
                Expected::AcceptedBy("postal-code".into()),
                text("123-4567"),
            ),
        ];

        let kinds: Vec<&'static str> = cases
            .iter()
            .map(|(reason, _, _)| reason_kind(reason))
            .collect();
        assert_eq!(
            vec![
                "TypeMismatch",
                "OutOfRange",
                "LengthOutOfRange",
                "PatternMismatch",
                "ChoiceNotAllowed",
                "PrecisionExceeded",
                "MissingValue",
                "Duplicate",
                "BrokenReference",
                "UnusableColumn",
                "CustomRejected",
                "CustomFailed",
            ],
            kinds,
            "違反の理由の全変種が判別可能でない"
        );

        for (reason, expected, actual) in &cases {
            assert_eq!(
                expected,
                reason.expected(),
                "{} が期待した内容を保持していない",
                reason_kind(reason)
            );
            assert_eq!(
                actual,
                reason.actual(),
                "{} が実際の値を保持していない",
                reason_kind(reason)
            );
        }
    }

    /// シート全体の結果が、上限までの違反・違反の総件数・違反を持つ行の一覧を持ち、
    /// 違反を持つ行と持たない行を区別できる（要件 5.4, 5.6）。
    #[test]
    fn sheet_report_holds_capped_violations_total_and_invalid_rows() {
        let sheet = sheet_id();
        let row_a = row_id(ROW_A);
        let row_b = row_id(ROW_B);

        let mut report = ViolationReport::new(&ValidationOptions::capped(2));
        report.push(type_mismatch(row_a, 0, "数量"));
        report.push(type_mismatch(row_a, 1, "単価"));
        report.push(type_mismatch(row_b, 0, "数量"));
        assert_eq!(3, report.total(), "総件数が蓄積の途中で数えられていない");

        let report = report.finish(sheet);
        assert_eq!(sheet, report.sheet());
        assert_eq!(
            2,
            report.violations().len(),
            "上限までに切り詰められていない"
        );
        assert_eq!(
            3,
            report.total_violations(),
            "上限を超えても総件数を数えていない"
        );
        assert!(report.is_truncated(), "切り詰められたことが分からない");
        assert_eq!(
            vec![row_a, row_b],
            report.invalid_rows(),
            "違反を持つ行の一覧が重複なく保たれていない"
        );

        // 上限がなければすべて保持し、切り詰められない。
        let mut unlimited = ViolationReport::new(&ValidationOptions::unlimited());
        unlimited.extend([
            type_mismatch(row_a, 0, "数量"),
            type_mismatch(row_b, 1, "単価"),
        ]);
        let unlimited = unlimited.finish(sheet);
        assert_eq!(2, unlimited.violations().len());
        assert_eq!(2, unlimited.total_violations());
        assert!(!unlimited.is_truncated());
        assert_eq!(vec![row_a, row_b], unlimited.invalid_rows());
    }

    /// 違反の件数の上限を呼び出し元が指定できる（要件 5.6）。
    #[test]
    fn the_caller_specifies_the_violation_limit() {
        let sheet = sheet_id();
        let row_a = row_id(ROW_A);
        let row_b = row_id(ROW_B);

        assert_eq!(Some(0), ValidationOptions::capped(0).violation_limit());
        assert_eq!(Some(7), ValidationOptions::capped(7).violation_limit());
        assert_eq!(None, ValidationOptions::unlimited().violation_limit());
        assert_eq!(
            None,
            ValidationOptions::default().violation_limit(),
            "上限を指定しない既定は無制限である"
        );

        // 上限 0 でも総件数と違反を持つ行の一覧は保たれる（呼び出し元は印を付けられる）。
        let mut report = ViolationReport::new(&ValidationOptions::capped(0));
        report.push(type_mismatch(row_a, 0, "数量"));
        report.push(type_mismatch(row_b, 0, "数量"));
        let report = report.finish(sheet);
        assert!(report.violations().is_empty());
        assert_eq!(2, report.total_violations());
        assert!(report.is_truncated());
        assert_eq!(vec![row_a, row_b], report.invalid_rows());
    }

    /// 違反の理由の変種名（ワイルドカードなし。変種の追加漏れを型検査で気づけるようにする）。
    fn reason_kind(reason: &ViolationReason) -> &'static str {
        match reason {
            ViolationReason::TypeMismatch { .. } => "TypeMismatch",
            ViolationReason::OutOfRange { .. } => "OutOfRange",
            ViolationReason::LengthOutOfRange { .. } => "LengthOutOfRange",
            ViolationReason::PatternMismatch { .. } => "PatternMismatch",
            ViolationReason::ChoiceNotAllowed { .. } => "ChoiceNotAllowed",
            ViolationReason::PrecisionExceeded { .. } => "PrecisionExceeded",
            ViolationReason::MissingValue { .. } => "MissingValue",
            ViolationReason::Duplicate { .. } => "Duplicate",
            ViolationReason::BrokenReference { .. } => "BrokenReference",
            ViolationReason::UnusableColumn { .. } => "UnusableColumn",
            ViolationReason::CustomRejected { .. } => "CustomRejected",
            ViolationReason::CustomFailed { .. } => "CustomFailed",
        }
    }
}

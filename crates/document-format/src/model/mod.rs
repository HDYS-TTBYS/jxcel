//! Document 集約ルートと構造的不変条件(タスク 2.1。要件 1.1, 1.5, 1.6, 8.4)。
//!
//! # 集約ルート
//!
//! design「Domain Model」: **集約ルートは [`Document`]**。すべての変更は `Document` を
//! 経由し、シート・行は `Document` の外で独立に存在しない。[`Sheet`] の変更メソッドは
//! クレート可視(`Document` が委譲する経路)で、外部から可変のシートを得る口は無い
//! ([`Document::sheet_by_id`] は共有参照しか返さない)。
//!
//! # 順序の保持(要件 1.1, 1.5)
//!
//! * シート順序 = [`Document::sheets`] の反復順(`document.json` へのこの順序の永続化は
//!   タスク 4.3)。0 シートは妥当な状態(要件 1.1)。
//! * 行順序 = [`Sheet::rows`] の添字順(model/sheet.rs。NDJSON の行オブジェクトの形は
//!   タスク 3.4 / 3.5 の責務)。
//!
//! # 識別子は並び替え・改名で変わらない(要件 1.5, 1.6)
//!
//! 並び替えは位置の順列置換のみ([`Document::reorder_rows`])、改名は `name` のみの
//! 書き換え([`Document::rename_sheet`])で、いずれも `SheetId` / `RowId` に触れない
//! (テスト `reorder_rows_permutes_positions_without_changing_identifiers`、
//! `rename_sheet_keeps_identifier`)。
//!
//! # 一意性不変条件の強制地点
//!
//! 「文書内で `SheetId` / `RowId` はそれぞれ一意」は**構築経路による構造保証**である:
//! 識別子は [`Document`] が所有する [`IdFactory`] からの発行のみで得られ(厳密昇順)、
//! 重複注入の API 口は存在しない(他文書で発行した識別子は対象文書に挿入する経路が
//! 無いため入り込めない)。読み込み時の重複検出・報告(要件 4.3 の `DuplicateId`)は
//! 読み込み経路 `StructuralValidator` の役割で、本モデルの責務ではない。
//!
//! # design エラー表との関係
//!
//! design エラー表の 10 変種([`DocumentError`](crate::error::DocumentError))は I/O・
//! 形式破損の診断である。モデル操作の失敗(実在しないシートの指定、順列でない並び替え
//! 要求)は表のどの変種にも対応しないため、`DocumentError` に増やさず本モジュールの
//! 最小ローカル型 [`UnknownSheet`] / [`ReorderError`] とする([`IdParseError`](crate::ids::IdParseError) と同じ
//! 「表に無いものはローカルに暫く置く」パターン)。panic にしないので呼び出し元が
//! 実行時エラーとして扱える。
//!
//! # 規模(要件 8.4)
//!
//! 文書は全件オンメモリ(`Vec<Sheet>` / `Vec<Row>`。遅延ロードは対象外)で、合計 10 万行
//! の保持を保証する。10 万行超をモデル側で拒否はしない(要件 8.5 の超過通知は読み込み
//! 経路 `OpenOutcome::beyond_supported_scale` の役割)。
//!
//! # 後続タスクとの境界
//!
//! * 各シートがちょうど 1 つ持つルートスキーマ(要件 1.2)は `SchemaPart` の所有であり、
//!   タスク 2.2 で `Sheet` に `root_schema` として追加される。本タスク(2.1)はフィールドを
//!   持たない(検証も 2.2 の担当)。
//! * 添付レジストリ(タスク 2.3)、永続化(4.x)、行に値を設定する経路(parts 復路 3.x)は
//!   本タスクの範囲外。
//! * [`Document`] / [`Sheet`] / [`Row`] は `Clone` を実装しない(識別子発行状態の clone
//!   方針が未定。model/sheet.rs の「Clone を実装しない理由」参照)。

mod sheet;

use thiserror::Error;

use crate::ids::{IdFactory, RowId, SheetId};

pub use sheet::{Row, Sheet};

/// [`Document::reorder_rows`] の失敗。
///
/// モデル操作(順列引数)の programming-error 面であり、design エラー表(I/O・形式
/// 診断の 10 変種)に対応変種が無いため `DocumentError` には含めない。
/// コントラクト契約の 3 変種に加えて `UnknownSheet` 変種を持つ: シート指定の並び替えで
/// シート自体が未知の失敗は行単位の 3 変種では表現でき、結果を 1 つの `Result` に
/// 統合するほうが呼び出し元が素直に扱えるため(契約偏差として記録)。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReorderError {
    /// 指定シート自体が文書に存在しない。
    #[error("no such sheet in document: {sheet}")]
    UnknownSheet {
        /// 指定されたシート識別子。
        sheet: SheetId,
    },
    /// 順列にそのシート所属でない識別子が含まれていた。
    #[error("row {0} is not a row of this sheet")]
    UnknownRow(RowId),
    /// 同一の行識別子が順列に 2 回以上含まれている。
    #[error("row {0} appears more than once in reorder")]
    DuplicateRow(RowId),
    /// 順列が一部の行を省略している(ちょうど現在の行集合の順列でない)。
    #[error("reorder omits rows: {missing:?}")]
    Incomplete {
        /// 省略された行識別子(ULID 昇順。診断を決定的にするためソート済み)。
        missing: Vec<RowId>,
    },
}

/// 実在しないシートの指定([`Document::add_row`] / [`Document::rename_sheet`])。
///
/// [`ReorderError::UnknownSheet`] と同じ意味(未知シート)で、シート指定の操作は
/// どの経路でも panic ではなくこの型で報告される。
#[derive(Debug, Error, PartialEq, Eq)]
#[error("no such sheet in document: {sheet}")]
pub struct UnknownSheet {
    /// 指定されたが存在しなかったシート識別子。
    pub sheet: SheetId,
}

/// jxcel ドキュメントの集約ルート。
///
/// 0 個以上のシートを保持し(要件 1.1)、シート順序は [`Document::sheets`] の順である。
/// すべての変更はこの型経由であり、識別子は所有する [`IdFactory`] から発行される
/// (一意性が構築で保証される所以。モジュール docs 参照)。
#[derive(Debug)]
pub struct Document {
    /// 識別子発行口(シート・行で単調カウンタを共有する単発行者)。
    ids: IdFactory,
    /// シート順そのものの列。
    sheets: Vec<Sheet>,
}

impl Document {
    /// 0 シートの文書を作る(要件 1.1)。
    #[inline]
    pub fn new() -> Self {
        Self { ids: IdFactory::new(), sheets: Vec::new() }
    }

    /// シート順で反復する(要件 1.1。traceability `Document::sheets`)。
    /// 共有スライスであり、変更口はこの型のメソッドだけである。
    #[inline]
    pub fn sheets(&self) -> &[Sheet] {
        &self.sheets
    }

    /// シート識別子で引く。存在しなければ `None`。
    #[inline]
    pub fn sheet_by_id(&self, sheet: SheetId) -> Option<&Sheet> {
        self.sheets.iter().find(|s| s.id() == sheet)
    }

    /// 名前 `name` のシートを末尾に追加し、発行した識別子を返す(要件 1.1, 1.4)。
    pub fn add_sheet(&mut self, name: impl Into<String>) -> SheetId {
        let id = self.ids.new_sheet_id();
        self.sheets.push(Sheet::new(id, name.into()));
        id
    }

    /// シートを取り除く。存在しなければ `None`。残りシートの順序は保持される。
    ///
    /// 返されたシートは分離され、再追加の口は無いため、分離シートが後から文書の
    /// 一意性を壊す経路にはならない。
    pub fn remove_sheet(&mut self, sheet: SheetId) -> Option<Sheet> {
        let index = self.sheets.iter().position(|s| s.id() == sheet)?;
        Some(self.sheets.remove(index))
    }

    /// シート名を変える。識別子は変わらない(要件 1.6)。
    pub fn rename_sheet(
        &mut self,
        sheet: SheetId,
        new_name: impl Into<String>,
    ) -> Result<(), UnknownSheet> {
        match self.sheets.iter_mut().find(|s| s.id() == sheet) {
            Some(target) => {
                target.rename(new_name.into());
                Ok(())
            }
            None => Err(UnknownSheet { sheet }),
        }
    }

    /// 指定シートの末尾に空値の行を追加し、発行識別子を返す(要件 1.4, 1.5)。
    ///
    /// 先に発行してから対象シートを選ぶ: シートが存在しない場合、発行済みの識別子は
    /// 破棄される(発行者の状態が進むだけで、文書に痕跡は残らない)。
    pub fn add_row(&mut self, sheet: SheetId) -> Result<RowId, UnknownSheet> {
        let id = self.ids.new_row_id();
        match self.sheets.iter_mut().find(|s| s.id() == sheet) {
            Some(target) => {
                target.push_row(Row::new(id));
                Ok(id)
            }
            None => Err(UnknownSheet { sheet }),
        }
    }

    /// 指定シートの行順序を与えた順列で置き換える。行識別子は一切変わらない(要件 1.5)。
    ///
    /// `order` が現在の行識別子集合**ちょうど**の順列でなければ [`ReorderError`] を返し、
    /// 順序は 1 つも変わらない(部分適用なし)。
    pub fn reorder_rows(
        &mut self,
        sheet: SheetId,
        order: &[RowId],
    ) -> Result<(), ReorderError> {
        self.sheets
            .iter_mut()
            .find(|s| s.id() == sheet)
            .ok_or(ReorderError::UnknownSheet { sheet })?
            .reorder_rows(order)
    }
}

impl Default for Document {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{Document, ReorderError, Row, Sheet, UnknownSheet};
    use crate::ids::{RowId, SheetId};

    #[test]
    fn empty_document_is_valid() {
        // 要件 1.1: 0 個以上のシート。0 シートは正準的な空の状態であり不作成ではない。
        let doc = Document::new();
        assert!(doc.sheets().is_empty());
    }

    #[test]
    fn sheet_order_is_insertion_order_not_key_order() {
        let mut doc = Document::new();
        let issued = [
            doc.add_sheet("zeta"),
            doc.add_sheet("alpha"),
            doc.add_sheet("mid"),
        ];
        let observed: Vec<SheetId> = doc.sheets().iter().map(Sheet::id).collect();
        assert_eq!(issued.to_vec(), observed, "sheets() は追加順を保持しなければならない");
        let names: Vec<&str> = doc.sheets().iter().map(Sheet::name).collect();
        assert_eq!(["zeta", "alpha", "mid"], names.as_slice());
    }

    #[test]
    fn row_order_is_issuance_order() {
        let mut doc = Document::new();
        let sheet = doc.add_sheet("rows");
        let issued: Vec<RowId> = (0..20).map(|_| doc.add_row(sheet).unwrap()).collect();
        let observed: Vec<RowId> = doc
            .sheet_by_id(sheet)
            .unwrap()
            .rows()
            .iter()
            .map(Row::id)
            .collect();
        assert_eq!(issued, observed);
    }

    #[test]
    fn row_ids_are_unique_within_sheet() {
        // design「Domain Model 不変条件」: 文書内で RowId は一意(構築経路による保証)。
        let mut doc = Document::new();
        let sheet = doc.add_sheet("uniq");
        let issued: Vec<RowId> = (0..1_000).map(|_| doc.add_row(sheet).unwrap()).collect();
        let unique: std::collections::HashSet<RowId> = issued.iter().copied().collect();
        assert_eq!(1_000, unique.len());
    }

    #[test]
    fn reorder_rows_permutes_positions_without_changing_identifiers() {
        // 要件 1.5 / design 不変条件「行の並び替えは識別子を変更しない」。
        let mut doc = Document::new();
        let sheet = doc.add_sheet("reorder");
        let before: Vec<RowId> = (0..12).map(|_| doc.add_row(sheet).unwrap()).collect();

        // 先頭と末尾を入れ替えた順列を要求する。
        let mut order = before.clone();
        let last = order.len() - 1;
        order.swap(0, last);
        doc.reorder_rows(sheet, &order).unwrap();

        let after: Vec<RowId> = doc
            .sheet_by_id(sheet)
            .unwrap()
            .rows()
            .iter()
            .map(Row::id)
            .collect();
        assert_eq!(order, after, "要求した順列がそのまま順序になる");
        assert_eq!(before.len(), after.len(), "行の増減があってはならない");
        let mut sorted_before = before.clone();
        let mut sorted_after = after.clone();
        sorted_before.sort();
        sorted_after.sort();
        assert_eq!(sorted_before, sorted_after, "並び替えは識別子を変更しない(同一集合)");
        assert_ne!(before, after, "位置は実際に変わっている(空振りの確認)");
    }

    #[test]
    fn failed_reorder_leaves_order_untouched() {
        let mut doc = Document::new();
        let sheet = doc.add_sheet("strict");
        let rows: Vec<RowId> = (0..3).map(|_| doc.add_row(sheet).unwrap()).collect();

        // 別シート発行の行識別子(このシートには存在しない)。
        let other = doc.add_sheet("other");
        let stranger = doc.add_row(other).unwrap();
        assert_eq!(
            Err(ReorderError::UnknownRow(stranger)),
            doc.reorder_rows(sheet, &[stranger])
        );

        // 重複(同じ行を 2 回指定)。
        assert_eq!(
            Err(ReorderError::DuplicateRow(rows[0])),
            doc.reorder_rows(sheet, &[rows[0], rows[0]])
        );

        // 不足(一部を省略)。欠落は決定的診断のため ULID 順にソートされる。
        assert_eq!(
            Err(ReorderError::Incomplete { missing: vec![rows[0], rows[2]] }),
            doc.reorder_rows(sheet, &[rows[1]])
        );

        // どれでも失敗後も順序は無変更(部分適用なし)。
        let observed: Vec<RowId> = doc
            .sheet_by_id(sheet)
            .unwrap()
            .rows()
            .iter()
            .map(Row::id)
            .collect();
        assert_eq!(rows, observed);
    }

    #[test]
    fn rename_sheet_keeps_identifier() {
        // 要件 1.6 / design 不変条件「シートの改名は識別子を変更しない」。
        let mut doc = Document::new();
        let target = doc.add_sheet("旧名");
        let other = doc.add_sheet("他シート");

        doc.rename_sheet(target, "新名").unwrap();

        let sheet = doc.sheet_by_id(target).expect("改名後も同一識別子で引ける");
        assert_eq!(target, sheet.id());
        assert_eq!("新名", sheet.name());
        // シート順(位置)も他のシートも無変更。
        assert_eq!(target, doc.sheets()[0].id());
        assert_eq!(other, doc.sheets()[1].id());
        assert_eq!("他シート", doc.sheets()[1].name());
        // 複数回の改名でも識別子は不変。
        doc.rename_sheet(target, "さらに新名").unwrap();
        assert_eq!(target, doc.sheet_by_id(target).unwrap().id());
        assert_eq!("さらに新名", doc.sheet_by_id(target).unwrap().name());
    }

    #[test]
    fn remove_sheet_detaches_once() {
        let mut doc = Document::new();
        let a = doc.add_sheet("a");
        let b = doc.add_sheet("b");
        let c = doc.add_sheet("c");

        let removed = doc.remove_sheet(b).expect("存在するシートは除去できる");
        assert_eq!(b, removed.id());
        assert_eq!("b", removed.name());
        // 残りのシート順は保持される。
        let observed: Vec<SheetId> = doc.sheets().iter().map(Sheet::id).collect();
        assert_eq!(vec![a, c], observed);
        // 2 回目の除去対象は存在しない。
        assert!(doc.remove_sheet(b).is_none());
    }

    #[test]
    fn unknown_sheet_targets_are_reported() {
        // シート指定の全経路は未知の識別子を実行時エラーとして報告する(panic なし)。
        let stranger = {
            let mut scratch = Document::new();
            scratch.add_sheet("stranger")
        };
        let mut doc = Document::new();
        assert_eq!(Err(UnknownSheet { sheet: stranger }), doc.add_row(stranger));
        assert_eq!(Err(UnknownSheet { sheet: stranger }), doc.rename_sheet(stranger, "x"));
        assert_eq!(
            Err(ReorderError::UnknownSheet { sheet: stranger }),
            doc.reorder_rows(stranger, &[])
        );
        assert!(doc.remove_sheet(stranger).is_none());
        assert!(doc.sheet_by_id(stranger).is_none());
    }

    #[test]
    fn empty_permutation_on_empty_sheet_is_identity() {
        // 空順列 = 空行集合の順列。0 行シートは valid(要件 1.1 の 0 個以上)。
        let mut doc = Document::new();
        let sheet = doc.add_sheet("空");
        assert!(doc.sheet_by_id(sheet).unwrap().rows().is_empty());
        doc.reorder_rows(sheet, &[]).unwrap();
        assert!(doc.sheet_by_id(sheet).unwrap().rows().is_empty());
    }

    #[test]
    fn holds_100_000_rows_across_ten_sheets() {
        // 要件 8.4: 10 万行を保持できることの保証(全件オンメモリ)。
        const SHEETS: usize = 10;
        const ROWS_PER_SHEET: usize = 10_000;

        let mut doc = Document::new();
        let sheet_ids: Vec<SheetId> =
            (0..SHEETS).map(|i| doc.add_sheet(format!("sheet-{i}"))).collect();
        let mut issued: Vec<Vec<RowId>> = Vec::with_capacity(SHEETS);
        for &sheet in &sheet_ids {
            let rows: Vec<RowId> =
                (0..ROWS_PER_SHEET).map(|_| doc.add_row(sheet).unwrap()).collect();
            issued.push(rows);
        }

        assert_eq!(
            100_000,
            doc.sheets().iter().map(|s| s.rows().len()).sum::<usize>(),
            "合計 10 万行を保持できる"
        );

        // 1 シートを先頭・末尾入替で並び替え、他シートが無変更であることを確認する。
        let mut order = issued[0].clone();
        let last = order.len() - 1;
        order.swap(0, last);
        doc.reorder_rows(sheet_ids[0], &order).unwrap();
        let after: Vec<RowId> = doc
            .sheet_by_id(sheet_ids[0])
            .unwrap()
            .rows()
            .iter()
            .map(Row::id)
            .collect();
        assert_eq!(order, after);
        for i in 1..SHEETS {
            let observed: Vec<RowId> = doc
                .sheet_by_id(sheet_ids[i])
                .unwrap()
                .rows()
                .iter()
                .map(Row::id)
                .collect();
            assert_eq!(issued[i], observed, "他シートの行順序は無変更");
        }
    }

}

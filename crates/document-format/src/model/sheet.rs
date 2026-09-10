//! シートと行コレクション(タスク 2.1。要件 1.1, 1.5, 8.4)。
//!
//! # 行の順序 = `Vec` の順序
//!
//! [`Sheet`] は行を `Vec<Row>` で保持し、`rows` の添字順がそのままシートの行順序である。
//! 順序の第二キーは持たない。メモリ上の行順がすなわちそのシートの行順であり、
//! 並び替えは位置の順列置換だけを行う(要件 1.5。[`Sheet::reorder_rows`])。
//! シート順(`Vec<Sheet>` 側)の不変条件は [`super::Document`] のモジュール docs 参照。
//!
//! # ルートスキーマをちょうど 1 つ持つ(要件 1.2)
//!
//! 各シートは [`SchemaPart`] を `root_schema` として厳密に 1 つ持つ(design ER 図
//! `Sheet ||--|| SchemaPart : has_root`)。フィールドは `Option` でもリストでもないため
//! 0 個・2 個の状態は表現できず、差し替えは集約ルート経由の
//! [`Document::set_root_schema`](super::Document::set_root_schema) による置換のみである
//! (個数は増えない)。新規シートは空のルートスキーマ([`SchemaPart::empty`])で始まり、
//! その内容は不透明である(型定義の識別子と参照構造のみを本クレートが扱う)。
//!
//! # 行の同一性
//!
//! [`Row`] はこのモジュール外では-opaque(構築も変更もクレート可視のみ)であり、
//! すべての変更は集約ルート [`super::Document`] を経由する(design「Domain Model」)。
//! 行の値は列順の [`Vec<CellValue>`] として保持し、列の識別は位置である
//! (何番目の値がどの列に対応するかは `schema-engine` が決める。本モデルは判断しない)。
//! `sheets/<sheet-id>.jsonl` の NDJSON 行オブジェクトの形(行識別子をオブジェクトに
//! 含めるかどうか)は parts 層 rows_codec の責務(タスク 3.4, 3.5)で、本モジュールの
//! 所有するところではない。
//!
//! # Clone を実装しない理由
//!
//! [`Sheet`] / [`Row`] は `Clone` を実装しない。識別子発行は [`super::Document`] が所有する
//! [`IdFactory`](crate::ids::IdFactory) の単発行者保証であり、clone で生成された
//! シート・行がどの発行状態を継承するか(元と同一の発行者を共有するのか独立か)が
//! 未定義のまま型を公開すると、その保証を黙って壊す経路を作る。タスク 2.1 に clone の
//! 要求は無いため、方針が決まるまで用意しない(design「Domain Model」: シート・行は
//! `Document` の外で独立に存在しない)。

use std::collections::{HashMap, HashSet};

use crate::ids::{RowId, SheetId};
use crate::value::CellValue;

use super::schema_part::SchemaPart;
use super::ReorderError;

/// シート内の行。列順の [`CellValue`] を保持する(design「Domain Model」の
/// `Row ||--o{ CellValue : holds`)。
///
/// 識別子 [`RowId`] は不変である: 行の構築後に識別子を書き換える経路は存在しない
/// (列の追加・削除・並び替えは行の同一性を変えない。変える経路は行の削除 + 再作成であり、
/// それは別行になることである)。
#[derive(Debug)]
pub struct Row {
    id: RowId,
    values: Vec<CellValue>,
}

impl Row {
    /// 行識別子(発行後に不変)。
    #[inline]
    pub fn id(&self) -> RowId {
        self.id
    }

    /// 列順のセル値。列の同一性は位置であり、その解釈は `schema-engine` が行う。
    #[inline]
    pub fn values(&self) -> &[CellValue] {
        &self.values
    }

    /// クレート内構築口。タスク 2.1 で値を持つ行の公開経路は無い
    /// ([`Document::add_row`](super::Document::add_row) が空の値の行を作る。
    /// 値の入った行の構築は parts 層からのモデル構築(タスク 3.x / 4.x)で、
    /// その入口はこの関数に `pub(crate)` のコンストラクタを追加する)。
    #[inline]
    pub(crate) fn new(id: RowId) -> Self {
        Self { id, values: Vec::new() }
    }
}

/// シート: 識別子・名前・ルートスキーマ・順序づけられた 0 個以上の行(design
///「Domain Model」の ER 図)。
///
/// ルートスキーマは [`SchemaPart`] を**ちょうど 1 つ**持つ(要件 1.2 / design ER 図
/// `Sheet ||--|| SchemaPart : has_root`)。`Option` でもリストでもないため、0 個・
/// 2 個の状態は型として表現できない。差し替えは集約ルート経由の
/// [`super::Document::set_root_schema`] のみで、常に置換である(個数は増えない)。
///
/// 変更は集約ルート [`super::Document`] を経由する。この型の公開メソッドは読み取り
/// アクセッサのみで、変更メソッドは [`super::Document`] が経路として使うための
/// クレート可視である(タスク 2.1 の可視性規律: すべての変更は `Document` を経由する)。
#[derive(Debug)]
pub struct Sheet {
    id: SheetId,
    name: String,
    root_schema: SchemaPart,
    rows: Vec<Row>,
}

impl Sheet {
    pub(crate) fn new(id: SheetId, name: String) -> Self {
        Self { id, name, root_schema: SchemaPart::empty(), rows: Vec::new() }
    }

    /// シート識別子(発行後に不変。改名でも変わらない — 要件 1.6)。
    #[inline]
    pub fn id(&self) -> SheetId {
        self.id
    }

    /// シート名。変更は [`Document::rename_sheet`](super::Document::rename_sheet) のみ。
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// このシートのルートスキーマ(厳密に 1 つ。要件 1.2)。
    ///
    /// 内容は不透明である: 本クレートが扱うのは型定義の識別子と参照構造のみで、
    /// 型の意味論は `schema-engine` の所有である([`SchemaPart`] の docs 参照)。
    #[inline]
    pub fn root_schema(&self) -> &SchemaPart {
        &self.root_schema
    }

    /// ルートスキーマを差し替える経路(`Document::set_root_schema` が呼ぶ)。
    ///
    /// 置換であり追加ではないため、シートは常にちょうど 1 つを持つ(要件 1.2)。
    #[inline]
    pub(crate) fn set_root_schema(&mut self, schema: SchemaPart) {
        self.root_schema = schema;
    }

    /// 行順序そのものの添字順で反復する 0 個以上の行(要件 1.1)。
    #[inline]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// 名前を書き換める経路(`Document::rename_sheet` が呼ぶ)。
    ///
    /// `name` だけを更新し `id` に触れないため、改名で識別子は変わらない(要件 1.6)。
    #[inline]
    pub(crate) fn rename(&mut self, new_name: String) {
        self.name = new_name;
    }

    /// 行順序の末尾に行を追加する(`Document::add_row` が呼ぶ)。
    #[inline]
    pub(crate) fn push_row(&mut self, row: Row) {
        self.rows.push(row);
    }

    /// 行順序を与えられた順列で置き換える。識別子は一切変更しない(要件 1.5)。
    ///
    /// `order` は**ちょうど**現在の行識別子集合の順列でなければならない
    /// (既知の id のみ・重複なし・全行を含む)。違反は [`ReorderError`] を返し、
    /// 順序を 1 つも変更しない(失敗時の部分適用なし)。
    ///
    /// 実装は検証を先に完了させてから行を move で再配置する(clone しない。
    /// 10 万行でも移動はポインタ級の移動だけで、値の clone は発生しない)。
    pub(crate) fn reorder_rows(&mut self, order: &[RowId]) -> Result<(), ReorderError> {
        // 検証フェーズ(失敗時は self を一切変更しない)。
        // 行 id の一意性は構築経路の不変条件(IdFactory の厳密昇順発行、重複注入 API なし)
        // なので、既知集合は集合として作れる。
        let known: HashSet<RowId> = self.rows.iter().map(Row::id).collect();
        let mut seen: HashSet<RowId> = HashSet::with_capacity(order.len());
        for &id in order {
            if !known.contains(&id) {
                return Err(ReorderError::UnknownRow(id));
            }
            if !seen.insert(id) {
                return Err(ReorderError::DuplicateRow(id));
            }
        }
        if seen.len() != known.len() {
            // 順列が行数に足りない(未知も重複も無いのは、一部欠落のときだけ)。
            // 欠落 id は ULID の数値順(正準テキストの辞書順)に並べて決定的診断にする。
            let mut missing: Vec<RowId> = known.difference(&seen).copied().collect();
            missing.sort();
            return Err(ReorderError::Incomplete { missing });
        }
        // ここを通ったら `order` は現在の行 id 集合の順列である。以降に失敗経路は
        // 無いため、所有権を移してからの再構築で clone なしに再配置する。
        let current = std::mem::take(&mut self.rows);
        let mut pool: HashMap<RowId, Row> = HashMap::with_capacity(current.len());
        for row in current {
            pool.insert(row.id, row);
        }
        self.rows = order
            .iter()
            .map(|id| pool.remove(id).expect("order は検証済みの順列"))
            .collect();
        Ok(())
    }
}

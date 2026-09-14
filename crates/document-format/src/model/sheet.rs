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
//! # 列名を保持する(タスク 4.8 の親の裁定)
//!
//! [`Sheet`] は**順序付きの列名**([`Sheet::columns`])を持つ。本クレートは列名の中身を
//! 解釈しない(スキーマの意味論は `schema-engine`)が、行データの wire 形式
//! (`{"$id":..., <列名>: <値>}`)は列名をキーとするため(tasks 4.5)、
//! **書き出し時に行の列名一覧が必要**であり、`to_parts` / `save` は列名を外から
//! 受け取らない(design `DocumentFormatApi` の Service Interface)。さらに 0 行の
//! シートは行エントリから列順を復元できないため、列名はモデルが起点となって
//! `document.json` へ永続化される(parts 層 `SheetMeta`)。
//!
//! 設定経路は集約ルート経由の [`Document::set_sheet_columns`](super::Document::set_sheet_columns)
//! だけであり、新規シートは列名 0 個で始まる(既存の `add_sheet` のシグネチャは
//! 変えない。タスク 4.8 の制約)。
//!
//! # 未知フィールドの保持(前方互換。要件 6.2 / 6.3)
//!
//! [`Sheet`] は `document.json` のシート要素で解釈しなかったフィールド
//! ([`crate::json::PreservedFields`])を保持する。読み込み経路が復号済みの保持内容を
//! ここへ移し、保存経路がそれを `document.json` の同じ位置へ差し戻すことで、
//! **モデルを経由した往復でも前方互換データが落ちない**(タスク 4.8)。
//! `PreservedFields` は内部カーソルを等値に含むため、[`Sheet`] に `PartialEq` は
//! 提供しない(モジュール末尾の規律)。
//!
//! 依存方向は `Ids / Value / EntryName → Model → Json → Parts → Container → Api` だが、
//! この保持だけは `model → json` を引く。**`json` が `model` に依存する向きは無い**
//! (規律は維持される)。親の裁定による許容である(タスク 4.8)。
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
use crate::json::PreservedFields;
use crate::value::CellValue;

use super::schema_part::SchemaPart;
use super::{CellWriteError, ReorderError, RowInsertionError, RowRemovalError, UnknownRow};

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

    /// クレート内構築口。空の値の行を作る([`Document::add_row`](super::Document::add_row))。
    /// 値の設定は [`Row::set_values`] が担い、その公開経路は
    /// [`Document::set_row_values`](super::Document::set_row_values) だけである
    /// (行データの復号 = タスク 4.5 が消費する)。
    #[inline]
    pub(crate) fn new(id: RowId) -> Self {
        Self {
            id,
            values: Vec::new(),
        }
    }

    /// 列順のセル値を置き換える経路(`Document::set_row_values` が呼ぶ)。
    ///
    /// 追加ではなく置換である(行 1 件分の値列をそのまま復元するため)。行の識別子には
    /// 触れない。
    #[inline]
    pub(crate) fn set_values(&mut self, values: Vec<CellValue>) {
        self.values = values;
    }

    /// 1 つの列の値を置き換える経路(`Sheet::set_cells` が呼ぶ)。
    ///
    /// 列の添字は `Sheet::columns` の並びに対する位置であり、呼び出し元
    /// ([`Sheet::set_cells`])が列数の範囲内であることを事前検査で保証している。
    /// 現在の値数より後ろへの書き込みでは、間を [`CellValue::Null`] で埋める
    /// (値数は `column + 1` までしか伸びない)。行の識別子・値数以外には触れない。
    #[inline]
    pub(crate) fn set_cell(&mut self, column: usize, value: CellValue) {
        if self.values.len() <= column {
            self.values.resize(column + 1, CellValue::Null);
        }
        self.values[column] = value;
    }
}

/// シート: 識別子・名前・ルートスキーマ・順序付きの列名・順序づけられた 0 個以上の行
/// (design「Domain Model」の ER 図)。
///
/// ルートスキーマは [`SchemaPart`] を**ちょうど 1 つ**持つ(要件 1.2 / design ER 図
/// `Sheet ||--|| SchemaPart : has_root`)。`Option` でもリストでもないため、0 個・
/// 2 個の状態は型として表現できない。差し替えは集約ルート経由の
/// [`super::Document::set_root_schema`] のみで、常に置換である(個数は増えない)。
///
/// 列名([`Sheet::columns`])は**順序付きの文字列**であり、本クレートは中身を解釈しない
/// (何番目の値がどの列かは `schema-engine` が決める)。行データの wire 形式が列名を
/// キーとするため、書き出しにはこの列名一覧が必要である(モジュール docs
/// 「列名を保持する」)。
///
/// 変更は集約ルート [`super::Document`] を経由する。この型の公開メソッドは読み取り
/// アクセッサのみで、変更メソッドは [`super::Document`] が経路として使うための
/// クレート可視である(タスク 2.1 の可視性規律: すべての変更は `Document` を経由する)。
#[derive(Debug)]
pub struct Sheet {
    id: SheetId,
    name: String,
    root_schema: SchemaPart,
    /// 順序付きの列名(本クレートは中身を解釈しない)。
    columns: Vec<String>,
    rows: Vec<Row>,
    /// 解釈しないシート要素のフィールド(前方互換。要件 6.2 / 6.3)。
    preserved: PreservedFields,
}

impl Sheet {
    pub(crate) fn new(id: SheetId, name: String) -> Self {
        Self {
            id,
            name,
            root_schema: SchemaPart::empty(),
            columns: Vec::new(),
            rows: Vec::new(),
            preserved: PreservedFields::new(),
        }
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

    /// 順序付きの列名(設計上、本クレートは中身を解釈しない)。
    ///
    /// この順序が行データのキー順になる(`sheets/<ulid>.jsonl`)。0 個のシートは
    /// 列を持たない正当な状態である(0 行のシートの列名は `document.json` が唯一の
    /// 永続先。モジュール docs「列名を保持する」)。
    #[inline]
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// 列名を置き換える経路(`Document::set_sheet_columns` が呼ぶ)。
    ///
    /// 追加ではなく置換である(列名の中身の妥当性は本クレートでは判定しない)。
    #[inline]
    pub(crate) fn set_columns(&mut self, columns: Vec<String>) {
        self.columns = columns;
    }

    /// `document.json` のシート要素で保持した未知フィールド(要件 6.2 / 6.3)。
    ///
    /// 読み込み経路(`parts::from_parts`)が復号済みの保持内容をここへ移し、
    /// 保存経路(`parts::to_parts`)が `parts::SheetMeta` へ戻す。
    #[inline]
    pub(crate) fn preserved_fields(&self) -> &PreservedFields {
        &self.preserved
    }

    /// 保持すべき未知フィールドを据える経路(`parts::from_parts` が呼ぶ)。
    #[inline]
    pub(crate) fn set_preserved_fields(&mut self, preserved: PreservedFields) {
        self.preserved = preserved;
    }

    /// 行順序そのものの添字順で反復する 0 個以上の行(要件 1.1)。
    #[inline]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// 名前を書き換える経路(`Document::rename_sheet` が呼ぶ)。
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

    /// 復号済みの行の列を**一度に**末尾へ足す(`parts::from_parts` が呼ぶ一括経路)。
    ///
    /// 行ごとの探索をしないため O(n) である(要件 8.1)。行順は与えられた順のまま
    /// (並べ替えない。要件 1.5)。
    #[inline]
    pub(crate) fn extend_rows(&mut self, rows: Vec<Row>) {
        self.rows.extend(rows);
    }

    /// 指定行のセル値を置き換える(`Document::set_row_values` が呼ぶ)。
    ///
    /// 対象の行がこのシートに無ければ [`UnknownRow`](super::UnknownRow) を返し、どの行の
    /// 値も変更しない(部分適用なし)。行の識別子と行順序は変わらない。
    pub(crate) fn set_row_values(
        &mut self,
        row: RowId,
        values: Vec<CellValue>,
    ) -> Result<(), UnknownRow> {
        match self.rows.iter_mut().find(|r| r.id() == row) {
            Some(target) => {
                target.set_values(values);
                Ok(())
            }
            None => Err(UnknownRow { row }),
        }
    }

    /// 複数のセルを**1 回の呼び出しで**書き換える(`Document::set_cells` が呼ぶ)。
    ///
    /// 各変更は（行識別子, 列の添字, 値）であり、列の添字は [`Sheet::columns`] の並びに
    /// 対する位置である。**事前検査を 1 パスで行う**: 行の索引（[`RowId`] → `rows` の
    /// 位置）を 1 度だけ作り（O(行数)）、各変更を O(1) で検証するため、合計は
    /// O(行数 + 変更数) になる([`Sheet::set_row_values`] を変更数だけ繰り返すと対象行の
    /// 線形探索が毎回走り O(行数 × 変更数) になる)。**検証を通過するまで self を一切
    /// 変更しない**: 未知のシートは呼び出し元([`super::Document`])が、未知の行
    /// ([`CellWriteError::UnknownRow`])・範囲外の列
    /// ([`CellWriteError::UnknownColumn`])はここが判別可能な変種として返し、1 つでも
    /// 不正ならどのセルも変更しない(部分適用なし)。
    ///
    /// 適用は事前検査で作った索引を再利用して変更ごとに O(1) で行の位置を引く
    /// (索引を 2 度作らない)。行の集合・並び・識別子は変えず、1 つの変更が触れるのは
    /// その行のその列の値だけである(置換であって追加ではない)。行の現在の値数より
    /// 後ろへの書き込みでは間を [`CellValue::Null`] で埋める([`Row::set_cell`])。
    /// 同じ入力の再適用は同じ結果になる(冪等)。
    pub(crate) fn set_cells(
        &mut self,
        cells: &[(RowId, usize, CellValue)],
    ) -> Result<(), CellWriteError> {
        // 事前検査フェーズ(失敗時は self を一切変更しない)。
        let columns = self.columns.len();
        let index: HashMap<RowId, usize> = self
            .rows
            .iter()
            .enumerate()
            .map(|(position, row)| (row.id(), position))
            .collect();
        for (row, column, _) in cells {
            if !index.contains_key(row) {
                return Err(CellWriteError::UnknownRow { row: *row });
            }
            if *column >= columns {
                return Err(CellWriteError::UnknownColumn {
                    column: *column,
                    columns,
                });
            }
        }
        // ここを通ったら全変更が妥当である。索引は上のものを再利用するため、以降の
        // 失敗経路は無く、行の探索は変更ごとに O(1) である。
        for (row, column, value) in cells {
            let position = index[row];
            self.rows[position].set_cell(*column, value.clone());
        }
        Ok(())
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
        // 行 id の一意性はモデルの不変条件である(発行は `IdFactory` の厳密昇順発行のみで、
        // 識別子を受け取る唯一の挿入経路 `Sheet::insert_rows_at` が文書内の現存集合と
        // バッチ内の重複を検査して守る)ため、既知集合は集合として作れる。
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

    /// 複数の行を**1 回の呼び出しで**取り除き、取り除いた行を返す
    /// (`Document::remove_rows` が呼ぶ。data-grid 要件 6.1, 6.2)。
    ///
    /// `rows` は取り除く行の識別子である。**同じ識別子の重複は畳む**(同じ行を 2 度消すのは
    /// 同じ 1 回の削除であり、誤りではない)。返る [`Row`] は**取り除いた行そのもの**で、
    /// その順序は要求引数の並びではなく**シートの順序**である(同じ行集合の要求は、引数の
    /// 並びに依らず常に同じ結果になる)。行は所有権ごと返るため値も識別子も複製しない
    /// ([`Row`] は `Clone` を持たない。取り消しが値と識別子の双方を要するため、この形で
    /// 返す = data-grid 要件 6.6 / 9.2)。
    ///
    /// **事前検査を通過するまで self を変更しない**: このシートに属さない識別子が 1 つでも
    /// あれば [`RowRemovalError::UnknownRow`] を返し、1 行も取り除かない(部分適用なし。
    /// 未知のシートは呼び出し元 [`super::Document`] が判別する)。
    ///
    /// 実装は [`Sheet::set_cells`] と同じ 2 段である: まず行識別子の集合を作って要求の
    /// 実在を 1 パスで検査し(失敗時は self を一切変更しない)、次に所有権を移して
    /// 1 パスで振り分ける。合計 O(行数 + 削除数) であり、**行ごとに削除を繰り返す形には
    /// しない**: その形は 1 行ごとに並びの作り直し(残りの行の詰め直し)が走るため、
    /// 範囲削除が O(行数 × 削除数) になる(10 万行の範囲削除が data-grid 要件 6.2 の対象である)。
    /// 値の `clone` は生じない(移すのは [`Row`] の所有権だけである)。
    pub(crate) fn remove_rows(&mut self, rows: &[RowId]) -> Result<Vec<Row>, RowRemovalError> {
        // 要求を集合へ畳む(重複した識別子は同じ 1 回の削除である)。
        let wanted: HashSet<RowId> = rows.iter().copied().collect();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        // 事前検査フェーズ(失敗時は self を一切変更しない)。
        // 行識別子の一意性はモデルの不変条件である(発行は `IdFactory` の厳密昇順発行のみで、
        // 識別子を受け取る唯一の挿入経路 `Sheet::insert_rows_at` が守る)ため、所属の判定は
        // 集合で行える(`set_cells` の索引と同じ役割)。
        let known: HashSet<RowId> = self.rows.iter().map(Row::id).collect();
        if !wanted.is_subset(&known) {
            // 未知の識別子のうち最小のものを報告する(要求引数の並びに依らず決定的診断)。
            let row = *wanted
                .difference(&known)
                .min()
                .expect("未知の識別子が 1 つ以上ある");
            return Err(RowRemovalError::UnknownRow { row });
        }
        // ここを通ったら要求された識別子はすべて実在する。以降に失敗経路は無い。
        // シートの順に走査して振り分けるため、`removed` はシート順・`kept` は元の相対順に
        // なる(clone は発生しない)。
        let mut kept: Vec<Row> = Vec::with_capacity(self.rows.len() - wanted.len());
        let mut removed: Vec<Row> = Vec::with_capacity(wanted.len());
        for row in std::mem::take(&mut self.rows) {
            if wanted.contains(&row.id) {
                removed.push(row);
            } else {
                kept.push(row);
            }
        }
        self.rows = kept;
        Ok(removed)
    }

    /// 複数の行を**1 回の呼び出しで**位置 `index` へ差し込む
    /// (`Document::insert_rows_at` が呼ぶ。data-grid 要件 6.1, 6.6, 9.2)。
    ///
    /// `index` は**挿入前の行順**に対する添字であり、`index == 行数` は末尾への追加である
    /// ([`super::Document::add_row`] と同じ位置に入る)。`index > 行数` は
    /// [`RowInsertionError::IndexOutOfRange`] として拒む。
    ///
    /// 差し込む行の順序・識別子・値は**与えられたまま**であり、作り直さない(所有権ごと
    /// move するため `clone` は生じない)。**事前検査を通過するまで self を変更しない**:
    /// 渡された行の識別子が文書内に既にあれば [`RowInsertionError::DuplicateRow`]、位置が
    /// 範囲外なら `IndexOutOfRange` を返し、1 行も差し込まない(部分適用なし)。
    ///
    /// **一意性の検査は文書単位である**: `document_rows` は文書内の**全シート**の行識別子で
    /// あり、呼び出し元 [`super::Document::insert_rows_at`] が 1 回だけ作って渡す。行識別子の
    /// 一意性は**文書単位**の不変条件であり(design「Domain Model」の不変条件)、シート局所の
    /// 集合では強制できない: 文書内の**別のシートに現存する**識別子をこのシートへ差し込むと、
    /// 同じ識別子が文書内に 2 つ存在する状態が作れてしまう。文書全体の集合はこのシートの行を
    /// 含むため、シート局所の集合を別に作る必要はない(1 回の検査で両方を覆う)。
    /// 渡された行の間の識別子の重複も同じ変種で拒む。この検査は
    /// [`Sheet::reorder_rows`] が依存する一意性を守るためでもある。
    ///
    /// 空の `rows` は何もしない(位置が妥当なら成功する)。
    pub(crate) fn insert_rows_at(
        &mut self,
        index: usize,
        rows: Vec<Row>,
        document_rows: &HashSet<RowId>,
    ) -> Result<(), RowInsertionError> {
        self.validate_insertion_index(index)?;
        // 事前検査フェーズ(失敗時は self を一切変更しない)。
        let mut incoming: HashSet<RowId> = HashSet::with_capacity(rows.len());
        for row in &rows {
            let id = row.id;
            if document_rows.contains(&id) || !incoming.insert(id) {
                return Err(RowInsertionError::DuplicateRow { row: id });
            }
        }
        // ここを通ったら失敗経路は無い。所有権を移して差し込む(1 回の移動で位置を空ける。
        // 行ごとの [`Vec::insert`] は残りの行を毎回ずらす)。
        self.rows.splice(index..index, rows);
        Ok(())
    }

    /// 空の行を位置 `index` へ差し込み、渡された識別子をそのまま返す
    /// (`Document::insert_row_at` が呼ぶ。data-grid 要件 6.1)。
    ///
    /// `index` は挿入前の行順に対する添字であり、`index == 行数` は末尾への追加である。
    /// 識別子は呼び出し元 [`super::Document`] が所有する
    /// [`IdFactory`](crate::ids::IdFactory) から発行済みのものを受け取る(発行の責務は
    /// 集約ルートにある)。**文書内で識別子を発行するのはその 1 者だけ**なので、発行直後の
    /// 識別子は文書のどのシートにも無く、一意性は構築で保証される(ここで集合を引く検査は
    /// 要らない。引いても常に空振りになる冗長な探索である)。失敗経路は位置が範囲外の
    /// 場合だけであり、そのとき 1 行も差し込まない。
    pub(crate) fn insert_row_at(
        &mut self,
        index: usize,
        id: RowId,
    ) -> Result<RowId, RowInsertionError> {
        self.validate_insertion_index(index)?;
        // 空の行を差し込む。既存の行は 1 つも動かさず、相対順序も識別子も変わらない。
        self.rows.insert(index, Row::new(id));
        Ok(id)
    }

    /// 挿入位置の事前検査(挿入前の行順に対する添字)。
    ///
    /// `index == 行数` は末尾への追加として妥当であり、`index > 行数` だけを
    /// [`RowInsertionError::IndexOutOfRange`] として拒む(行数は挿入前のもの = `rows` の
    /// 現在の長さを診断に載せる)。
    #[inline]
    fn validate_insertion_index(&self, index: usize) -> Result<(), RowInsertionError> {
        let rows = self.rows.len();
        if index > rows {
            return Err(RowInsertionError::IndexOutOfRange { index, rows });
        }
        Ok(())
    }
}

//! 読みの重ね合わせ（tasks.md 2.4。要件 4.1, 4.2, 4.4, 4.5, 11.1, 11.3）。
//!
//! マクロの**読み**の境界と、**自分の書き込みを読む**ための重ね合わせをここに置く
//! （design.md「File Structure Plan」の `host/overlay.rs`「読みの重ね合わせ（自分の
//! 書き込みを読む）」）。design.md のファイル構成に `host/read.rs` は無いため、読みの
//! 境界型（[`SheetInfo`] / [`ColumnTypeInfo`] / [`RowSpan`] / [`RowPage`] と、その中身の
//! [`ReadRow`]）も本モジュールが持つ。前の 4 つの名前は、ホスト API の宣言表
//! （`surface/declaration.rs` の `sheets` / `columns` / `readRange`）が要求する名前そのもので
//! ある（表が要求する型名の一覧はそちらの doc にある）。
//!
//! # 読みは何を見るか（重ね合わせの規則）
//!
//! 読みは**文書から読んだ行**（アダプタが `HostPort` の内側で供給する）に、**自分のセルの
//! 書き込み**（[`ChangeSet`]。タスク 2.3）を重ねたものである。規則は 3 つである。
//!
//! 1. **書いたセルは書いた値**（要件 5.1 の裏面）。同じセルへ複数回書いた場合は
//!    [`ChangeSet`] が解決済みの写像を持つため**最後の値**が返る（design.md の不変条件
//!    「1 つのセルへの複数の書き込みは後ろが残る」）
//! 2. **書いていないセルは文書の値**。重ね合わせは差分だけを見る
//! 3. **行の追加・削除・複製は読みに現れない**。適用の順序（追加 → 値 → 削除）はアダプタが
//!    適用するときに効く（design.md 決定 2）。理由は 2 つある:
//!    - 追加した行は**適用まで識別子を持たない**（design.md「HostPort」）。[`RowSpan`] は
//!      位置で範囲を指すため、追加した行を後から読む要求は「存在しない行を指した」ことに
//!      なり、読み口が理由つきで拒む（要件 5.4 の理由に含まれる）
//!    - 削除した行は適用まで文書に存在する。[`SheetInfo::row_count`] は**重ね合わせを
//!      見ない**（design.md の `HostPort::sheets` に重ね合わせの記述が無い）ため、読みだけ
//!      行集合を縮めると「行数 97 と言いながら 100 行返る」の食い違いが出る。削除は適用の
//!      ときに効く
//!
//! この 3 番目は**非対称**である: 削除した行への**書き込み**は 2.3 が理由つきで拒む
//! （適用のときに消える行への書き込みは結果に現れず、黙って捨てるわけにいかない）。読みは
//! 適用前の文書の現状を返す。どちらも「適用前の状態について嘘をつかない」で一貫する。
//!
//! # 費用の形（要件 4.4, 11.1, 11.3）
//!
//! 範囲の読み 1 回の費用は `O(行数 + log 変更数 + 窓の内側の変更数)` である。
//!
//! | 項 | 何に比例するか | どこで払うか |
//! |---|---|---|
//! | `行数` | 要求した範囲の行数（文書から読む費用） | アダプタの `HostPort` |
//! | `log 変更数` | 変更集合の写像の大きさの対数 | [`Overlay::read_range`] の `BTreeMap::range` |
//! | `窓の内側の変更数` | 要求した範囲に含まれる書き込みの件数 | 同上（値の clone もこの件数だけ） |
//!
//! **変更集合の走査は行数に比例しない。** 走査は範囲のキー
//! `(シート, 行識別子の窓, 列)` で `BTreeMap::range` を引くため、範囲の外の書き込みは 1 つも
//! 見ない（変更集合全体をなめる `O(変更数)` の走査をしない）。さらに**窓の内側に 1 つも
//! 書き込みが無ければ行 → 位置の索引を作らない**（[`Overlay::read_range`] の遅延生成）。
//!
//! 10 万行 × 30 列の全行の読み（要件 11.1）で重ね合わせが上乗せするのは、窓の内側の
//! 書き込み `k` 件の clone だけである。文書の値は clone せずそのまま持ち回る（[`ReadRow`] を
//! 所有で受け取り、書いたセルだけを置き換える）。20 万セル級でも 1 回の呼び出しで読めること
//! はテスト `a_two_hundred_thousand_cell_range_is_read_in_one_call` が固定する。
//!
//! # 何をしないか
//!
//! - **列の宣言の書き換え**: 公開しない。宣言表（`surface/declaration.rs`）に書き換えの API が
//!   無いため、門（`surface/gate.rs`）が `CallRefusal::UnknownApi` として**名前つきで**拒む
//!   （要件 4.5）。本モジュールに拒否の判断は無い
//! - **値の写像**: `host/value.rs`（タスク 2.2）が持つ。本モジュールは [`CellValue`] のまま
//!   重ね、JS の値への変換は行わない（変換はタスク 3.2 が `to_js` を通して行う）
//! - **文書の読み**: アダプタ（`src-tauri/src/macro_host.rs`。タスク 4.1）が
//!   `document-session` から供給する。エンジンは文書を知らない（design.md 決定 3）。
//!   存在しない行を指した読みの拒否（要件 5.4）も、文書を引ける側であるアダプタが行う
//! - **`.d.ts` への写し**: `ts-rs` の導出はタスク 3.3 が足す（`CellValue` の綴りや
//!   `TypeKind` の写し方を本モジュールが決めると、生成器の側と二重になる）。本モジュールが
//!   固定するのは**境界の型名**だけである

use std::collections::HashMap;
use std::ops::Range;

use document_format::{CellValue, RowId, SheetId};
use schema_engine::{ColumnIndex, TypeKind};

use crate::host::changes::ChangeSet;

/// マクロから見たシート 1 枚（要件 4.1 の「シートの一覧」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetInfo {
    /// シート識別子。読み書きの要求（`columns` / `readRange` / `setCells`）へ渡す。
    pub id: SheetId,
    /// シート名（利用者に見えている名前）。
    pub name: String,
    /// 行数。**重ね合わせを見ない**（文書の行数。モジュール docs の規則 3）。
    pub row_count: usize,
}

/// マクロから見た列の宣言 1 列（要件 4.1 の「列の宣言」、4.2 の「宣言された型」）。
///
/// 型情報（[`TypeKind`]）を持つのは、値の写像（`host/value.rs` の [`to_js`]）が
/// 「この列の型」を必要とするためである（要件 4.2, 4.3）。
///
/// **制約**（範囲・長さ・書式・選択肢）は渡さない。値が型に適合するかの判定は
/// `schema-engine` の持ち物であり、マクロは判定規則を持たない（design.md「Out of Boundary」
/// の「値の正否の判定」）。違反は適用のときに画面と同じ形で提示される（要件 5.2）。
///
/// [`to_js`]: crate::host::value::to_js
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnTypeInfo {
    /// 列名（`RowPage` のセルの並びはこの並びと同じ順である）。
    pub name: String,
    /// 宣言された型。名前付き型定義への参照（`TypeDecl::Ref`）は、アダプタが解決した先の
    /// 種別をここへ渡す（`schema-engine` の compile / resolve 層が解決を持つ）。
    pub kind: TypeKind,
    /// 値なしを許さないか（上流の宣言の `required`）。
    pub required: bool,
    /// 一意制約（上流の宣言の `unique`）。
    pub unique: bool,
}

/// 行の範囲（要件 4.4 の「行の範囲の読み」）。**0 起点の位置**で、両端を含む。
///
/// 位置で指すのは、**行ごとの呼び出しを強いないため**である（要件 4.4 / 11.3）。識別子で
/// 指す形にすると、全行を読むには先に全行の識別子を集める必要があり、それは行ごとの
/// 呼び出しそのものになる。書き込み（`host.setCells` の `CellWrite`）は行の識別子で指す —
/// 読みで受け取った [`ReadRow::id`] をそのまま渡せる。
///
/// 位置は文書の行の並び（表示の順）であり、行の識別子の順とは一致しない
/// （`document-format` は行の並べ替えを持つ）。[`RowSpan::resolve`] が位置を行数へ当てる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowSpan {
    /// 始まりの位置（0 起点。含む）。
    pub from: usize,
    /// 終わりの位置（0 起点。含む）。
    pub to: usize,
}

impl RowSpan {
    /// 両端を含む範囲を作る。
    pub const fn new(from: usize, to: usize) -> Self {
        Self { from, to }
    }

    /// 行数 `row_count` のシートに対する 0 起点の半開区間（読む行が無ければ `None`）。
    ///
    /// **範囲が行数を越えたら末尾で止める**（拒まない）。全行を読む要求は
    /// `RowSpan::new(0, usize::MAX)` で書ける。行数の取得は `sheets()`（要件 4.1）が行う。
    /// 逆転した範囲（`from > to`）と、始まりが末尾の外にある範囲は空である。
    pub fn resolve(self, row_count: usize) -> Option<Range<usize>> {
        if self.from > self.to {
            return None;
        }
        let start = self.from.min(row_count);
        let end = self.to.saturating_add(1).min(row_count);
        (start < end).then_some(start..end)
    }
}

/// 範囲の読みで返る 1 行（要件 4.4）。
#[derive(Debug, Clone, PartialEq)]
pub struct ReadRow {
    /// 行識別子。書き込み（`host.setCells` の `CellWrite`）へそのまま渡せる。
    pub id: RowId,
    /// 列の並び順のセル値（[`ColumnTypeInfo`] の並びと同じ順）。
    pub cells: Vec<CellValue>,
}

/// 範囲の読みの結果（要件 4.4）。**1 回の呼び出しで範囲の全部を返す。**
#[derive(Debug, Clone, PartialEq)]
pub struct RowPage {
    /// 要求した範囲の行（要求した順のまま）。
    pub rows: Vec<ReadRow>,
}

/// 自分の書き込みを読むための重ね合わせ（design.md「HostPort」の `overlay`）。
///
/// [`Overlay::new`] に未適用の変更集合（[`ChangeSet`]）を借りて作る。読みの入口は
/// [`Overlay::read_range`] 1 つである。変更集合は**借りるだけ**であり、読みは適用しない
/// （適用するのはアダプタ。design.md 決定 2 / 3）。
#[derive(Debug, Clone, Copy)]
pub struct Overlay<'a> {
    changes: &'a ChangeSet,
}

impl<'a> Overlay<'a> {
    /// 変更集合を重ねるビューを作る。
    pub const fn new(changes: &'a ChangeSet) -> Self {
        Self { changes }
    }

    /// 文書から読んだ範囲の行に、自分の書き込みを重ねて返す（要件 4.4, 5.1）。
    ///
    /// `base` は**要求した範囲の行を文書の順のまま**渡す（アダプタが `RowSpan::resolve` で
    /// 切った範囲）。順序は結果に影響しない: 重ね合わせは行の**識別子**で引く（モジュール
    /// docs の費用の形）。文書の順と識別子の順は一致しないため、識別子で引かないと
    /// 並べ替えられた文書で別の行へ書いた値が現れる。
    ///
    /// 書いたセルだけを置き換え、書いていないセルは文書の値のまま返す。書き込みが 1 つも
    /// 無ければ `base` をそのまま返す（clone も索引の生成もしない）。
    pub fn read_range(&self, sheet: SheetId, base: Vec<ReadRow>) -> RowPage {
        let mut rows = base;
        let Some((first, last)) = row_window(&rows) else {
            // 読む行が無い。重ねる先も無い。
            return RowPage { rows };
        };
        // 行の識別子の窓で走査を切る。列の添字は実際の列数を越えない（列数は
        // `usize::MAX` より遥かに小さい）ため、上限は「その行のすべての列」になる。
        let writes = self.changes.cell_writes().range(
            (sheet, first, ColumnIndex::new(0))..=(sheet, last, ColumnIndex::new(usize::MAX)),
        );
        // 書き込みが 1 つも無ければ索引を作らない（費用の形。モジュール docs）。
        let mut positions: Option<HashMap<RowId, usize>> = None;
        for ((_, row, column), value) in writes {
            let positions = positions.get_or_insert_with(|| positions_of(&rows));
            if let Some(&at) = positions.get(row) {
                write_cell(&mut rows[at].cells, column.index(), value.clone());
            }
        }
        RowPage { rows }
    }
}

/// 重ねる先の行の識別子の窓（最小と最大）。読む行が無ければ `None`。
///
/// 変更集合の走査をこの窓に限るために使う。窓は識別子の順の範囲であり、文書の位置の順
/// （`base` の並び）とは一致しないため、**窓の内側に読まない行が混じりうる**。それは
/// 走査が余分に進むだけであり、結果は変わらない（[`Overlay::read_range`] が位置の索引に
/// 無い行を飛ばす）。窓の外の書き込みは 1 つも見ない。
fn row_window(rows: &[ReadRow]) -> Option<(RowId, RowId)> {
    let first = rows.first()?.id;
    let mut window = (first, first);
    for row in &rows[1..] {
        window = (window.0.min(row.id), window.1.max(row.id));
    }
    Some(window)
}

/// 行の識別子 → `base` の位置。書き込みが窓の内側に 1 つ以上あるときだけ作る。
fn positions_of(rows: &[ReadRow]) -> HashMap<RowId, usize> {
    rows.iter()
        .enumerate()
        .map(|(at, row)| (row.id, at))
        .collect()
}

/// 1 セルを重ねる。行の値数より後ろへの書き込みは、間を [`CellValue::Null`] で埋める。
///
/// 適用する側（`document-format` の `Sheet::set_cell`）が同じ規則で埋めるため、読みと
/// 適用後の文書が同じ形になる。
fn write_cell(cells: &mut Vec<CellValue>, column: usize, value: CellValue) {
    if cells.len() <= column {
        cells.resize(column + 1, CellValue::Null);
    }
    cells[column] = value;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::changes::{CellWrite, Change};
    use document_format::IdFactory;

    /// 読み書きの対象にするシート。
    fn sheet() -> SheetId {
        "01ARZ3NDEKTSV4RRFFQ69G5FA0"
            .parse()
            .expect("ULID として読める")
    }

    /// 別のシート（同じ行識別子を持つ書き込みが混ざらないことの検査に使う）。
    fn other_sheet() -> SheetId {
        "01ARZ3NDEKTSV4RRFFQ69G5FA1"
            .parse()
            .expect("ULID として読める")
    }

    /// 識別子の順に並んだ 4 つの行識別子（末尾 1 文字が Crockford base32 の連続）。
    fn row_ids() -> [RowId; 4] {
        [
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "01ARZ3NDEKTSV4RRFFQ69G5FAW",
            "01ARZ3NDEKTSV4RRFFQ69G5FAX",
            "01ARZ3NDEKTSV4RRFFQ69G5FAY",
        ]
        .map(|text| text.parse().expect("ULID として読める"))
    }

    /// セルの書き込みを 1 回の呼び出しで集約した変更集合を作る。
    fn staged(sheet: SheetId, writes: &[(RowId, usize, CellValue)]) -> ChangeSet {
        let mut changes = ChangeSet::new();
        stage(&mut changes, sheet, writes);
        changes
    }

    /// 変更集合へセルの書き込みを 1 回集約する（同じ集合へ繰り返し積める）。
    fn stage(changes: &mut ChangeSet, sheet: SheetId, writes: &[(RowId, usize, CellValue)]) {
        changes
            .stage(Change::SetCells {
                sheet,
                writes: writes
                    .iter()
                    .map(|(row, column, value)| CellWrite {
                        row: *row,
                        column: ColumnIndex::new(*column),
                        value: value.clone(),
                    })
                    .collect(),
            })
            .expect("セルの書き込みを集約できる");
    }

    /// 書いたセルは書いた値で読み、書いていないセルは文書の値のまま読む（受け入れの中心）。
    #[test]
    fn written_cells_are_read_back_and_the_rest_stays_the_documents() {
        let ids = row_ids();
        let base: Vec<ReadRow> = ids
            .iter()
            .enumerate()
            .map(|(position, id)| ReadRow {
                id: *id,
                cells: vec![
                    CellValue::Text(format!("文書{position}-0")),
                    CellValue::Int(position as i64),
                    CellValue::Text(format!("文書{position}-2")),
                ],
            })
            .collect();
        let changes = staged(
            sheet(),
            &[
                (ids[1], 1, CellValue::Int(999)),
                (ids[3], 0, CellValue::Text("書いた".to_owned())),
            ],
        );

        let page = Overlay::new(&changes).read_range(sheet(), base.clone());

        assert_eq!(CellValue::Int(999), page.rows[1].cells[1]);
        assert_eq!(CellValue::Text("書いた".to_owned()), page.rows[3].cells[0]);
        // 同じ行の中の書いていないセル。
        assert_eq!(CellValue::Text("文書1-0".to_owned()), page.rows[1].cells[0]);
        assert_eq!(CellValue::Text("文書1-2".to_owned()), page.rows[1].cells[2]);
        assert_eq!(CellValue::Int(3), page.rows[3].cells[1]);
        // 書いていない行は丸ごと文書の値。
        assert_eq!(base[0], page.rows[0]);
        assert_eq!(base[2], page.rows[2]);
    }

    /// 書き込みが 1 つも無ければ、読みは文書の行をそのまま返す（値も並びも変えない）。
    #[test]
    fn a_read_without_pending_writes_returns_the_document_untouched() {
        let base: Vec<ReadRow> = row_ids()
            .iter()
            .map(|id| ReadRow {
                id: *id,
                cells: vec![CellValue::Int(1)],
            })
            .collect();

        let page = Overlay::new(&ChangeSet::new()).read_range(sheet(), base.clone());

        assert_eq!(base, page.rows);
    }

    /// 同じセルへ複数回書いたら最後の値が読める。読みは clone を渡すだけであり、変更集合は
    /// 適用のために最後の値を保持し続ける（読みが変更集合を食い潰さない）。
    #[test]
    fn the_last_write_wins_and_the_change_set_keeps_it_for_apply() {
        let row = row_ids()[0];
        let mut changes = staged(sheet(), &[(row, 0, CellValue::Int(1))]);
        stage(
            &mut changes,
            sheet(),
            &[(row, 0, CellValue::Text("最後".to_owned()))],
        );
        let base = vec![ReadRow {
            id: row,
            cells: vec![CellValue::Int(0)],
        }];

        let page = Overlay::new(&changes).read_range(sheet(), base);

        assert_eq!(CellValue::Text("最後".to_owned()), page.rows[0].cells[0]);
        assert_eq!(
            Some(&CellValue::Text("最後".to_owned())),
            changes.cell_value(sheet(), row, ColumnIndex::new(0))
        );
    }

    /// 重ね合わせは行の**識別子**で引く。文書の位置の順は識別子の順と一致しない
    /// （`document-format` は行の並べ替えを持つ）ため、位置で引くと別の行に現れる。
    #[test]
    fn writes_land_on_the_row_with_that_identifier_not_on_that_position() {
        let ids = row_ids();
        let base = vec![
            ReadRow {
                id: ids[2],
                cells: vec![CellValue::Int(2)],
            },
            ReadRow {
                id: ids[0],
                cells: vec![CellValue::Int(0)],
            },
            ReadRow {
                id: ids[1],
                cells: vec![CellValue::Int(1)],
            },
        ];
        let changes = staged(sheet(), &[(ids[0], 0, CellValue::Int(100))]);

        let page = Overlay::new(&changes).read_range(sheet(), base);

        assert_eq!(CellValue::Int(2), page.rows[0].cells[0]);
        assert_eq!(CellValue::Int(100), page.rows[1].cells[0]);
        assert_eq!(CellValue::Int(1), page.rows[2].cells[0]);
    }

    /// 走査はシートと行識別子の窓で切られている。別のシートの書き込み（同じ行識別子）と、
    /// 窓の外の行への書き込みは、読んだ範囲に現れない。
    #[test]
    fn writes_outside_the_sheet_or_the_row_window_do_not_appear() {
        let ids = row_ids();
        let base = vec![
            ReadRow {
                id: ids[1],
                cells: vec![CellValue::Int(1)],
            },
            ReadRow {
                id: ids[2],
                cells: vec![CellValue::Int(2)],
            },
        ];
        let mut changes = staged(other_sheet(), &[(ids[1], 0, CellValue::Int(-1))]);
        stage(&mut changes, sheet(), &[(ids[0], 0, CellValue::Int(-2))]);

        let page = Overlay::new(&changes).read_range(sheet(), base);

        assert_eq!(CellValue::Int(1), page.rows[0].cells[0]);
        assert_eq!(CellValue::Int(2), page.rows[1].cells[0]);
    }

    /// 行の値数より後ろへの書き込みは、適用する側と同じ規則で間を `Null` で埋める
    /// （読みと適用後の文書が同じ形になる）。
    #[test]
    fn a_write_beyond_the_rows_values_pads_with_null_like_apply_does() {
        let row = row_ids()[0];
        let base = vec![ReadRow {
            id: row,
            cells: vec![CellValue::Text("先頭".to_owned())],
        }];
        let changes = staged(sheet(), &[(row, 3, CellValue::Int(7))]);

        let page = Overlay::new(&changes).read_range(sheet(), base);

        assert_eq!(
            vec![
                CellValue::Text("先頭".to_owned()),
                CellValue::Null,
                CellValue::Null,
                CellValue::Int(7),
            ],
            page.rows[0].cells
        );
    }

    /// 要件 11.1 の形（1 万行 × 20 列 = 20 万セル）が**1 回の呼び出し**で読める。
    /// 重ね合わせが上乗せするのは窓の内側の書き込みの件数だけである。
    #[test]
    fn a_two_hundred_thousand_cell_range_is_read_in_one_call() {
        let mut factory = IdFactory::new();
        let rows: Vec<ReadRow> = (0..10_000)
            .map(|position| ReadRow {
                id: factory.new_row_id(),
                cells: vec![CellValue::Int(position as i64); 20],
            })
            .collect();
        let base = rows.clone();
        let changes = staged(
            sheet(),
            &[
                (rows[0].id, 0, CellValue::Int(-1)),
                (rows[5_000].id, 19, CellValue::Int(-2)),
                (rows[9_999].id, 7, CellValue::Int(-3)),
            ],
        );

        let page = Overlay::new(&changes).read_range(sheet(), rows);

        assert_eq!(10_000, page.rows.len());
        assert_eq!(20, page.rows[0].cells.len());
        assert_eq!(CellValue::Int(-1), page.rows[0].cells[0]);
        assert_eq!(CellValue::Int(-2), page.rows[5_000].cells[19]);
        assert_eq!(CellValue::Int(-3), page.rows[9_999].cells[7]);
        // 窓の内側でも、書いていないセルは文書の値のまま。
        assert_eq!(CellValue::Int(0), page.rows[0].cells[1]);
        assert_eq!(base[4_999], page.rows[4_999]);
    }

    /// 範囲は行数を越えたら末尾で止まる（全行の読みは `0..=usize::MAX` で書ける）。
    #[test]
    fn a_row_span_beyond_the_row_count_stops_at_the_end() {
        assert_eq!(Some(0..5), RowSpan::new(0, usize::MAX).resolve(5));
        assert_eq!(Some(2..5), RowSpan::new(2, 9).resolve(5));
        assert_eq!(Some(4..5), RowSpan::new(4, 4).resolve(5));
        // 始まりが末尾の外、逆転した範囲、行が無いシートは空。
        assert_eq!(None, RowSpan::new(9, 12).resolve(5));
        assert_eq!(None, RowSpan::new(3, 1).resolve(5));
        assert_eq!(None, RowSpan::new(0, usize::MAX).resolve(0));
        assert_eq!(None, RowSpan::new(usize::MAX, usize::MAX).resolve(5));
    }
}

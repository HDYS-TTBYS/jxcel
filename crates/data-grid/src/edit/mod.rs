//! 編集命令の定義と適用: [`EditApply`]（data-grid のタスク 3.1。要件 3.3, 3.4, 3.5, 3.7,
//! 11.4）。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本層は左の `types` /
//! `view` と、上流の `schema-engine` / `document-format` を参照する**（design.md「内部の
//! 依存の向き」。層の鎖の文言を各層の冒頭に置く規約は `structure.md`
//! 「ドメインクレートの内部構造」）。
//!
//! # 判定の分岐を持たない（本層の主題）
//!
//! 値が列の型に適合するか、どの値へ変換されるかは `schema-engine` が決める。本層は
//! **判定を依頼し、返った [`EditVerdict`] を写すだけである**:
//!
//! | 本層が決めること | 本層が決めないこと |
//! |---|---|
//! | どの行のどの列へ書くか（[`CellAddress`]） | 打たれた文字が列の型に適合するか |
//! | 打たれた文字を値の**在否**へ写すこと（空文字は値なし。理由は `edited_value`） | 変換の規則（`schema-engine` の規則表が唯一の源） |
//! | 判定の結果をどの順でドキュメントへ書くか | 違反があるときに拒否するか（**編集経路は拒否しない**） |
//!
//! **`WriteOrigin::Edit` は決して拒否しない**（`schema-engine` 要件 6.1）。したがって本層に
//! 「編集が失敗して値が戻る」分岐は存在せず、書き込み値はつねに判定が返した値そのものである
//! （design.md「System Flows / 編集の適用と判定」）。型に適合しない値も**破棄されず**、
//! ドキュメントに残ったうえで違反として報告される（要件 3.5）。
//!
//! 本層が列の型を見る箇所は 1 つも無い。`schema-engine` の検証器
//! （`CompiledSchema::validator`）を引く経路も持たない — 引けば「この列は int だから…」と
//! いう分岐を書く誘因が生まれ、規則の二重化（遅かれ早かれ食い違う）が始まる。
//!
//! # 1 セルの編集の費用の形（要件 11.4）
//!
//! 1 セルの編集が呼ぶのは次の 2 つだけである:
//!
//! 1. **書き込みの判定 1 回**（[`EditSchemaQuery::judge_write`]）。`validate_write` は
//!    **1 行分の値**を受け取り、値の添字を列の添字として読む（`schema-engine` の不変条件）。
//!    したがって「1 セルの編集」は **行を読む → その列の値を打たれた文字へ置き換える →
//!    判定を呼ぶ**であり、渡すのは当該の 1 セルだけではない（1 セル分の値を単独で判定する
//!    口は上流に無い）。
//! 2. **当該列に限定した再検証 1 回**（[`EditSchemaQuery::revalidate_columns`]）。
//!
//! **シート全件の検証（[`EditSchemaQuery::validate_sheet`]）は呼ばない。**これが要件 11.4 の
//! 内容そのものである。仮にここで全件検証を呼ぶと、10 万行 × 30 列で 255 ミリ秒（上流の
//! 実測）が編集のたびに掛かり、要件 11.3 の 100 ミリ秒に入らない（design.md
//! 「Performance & Scalability」）。
//!
//! 費用の形は「行数に対する 1 パス」＋「編集した列 1 本の走査」である。前者は行の位置の索引
//! （[`EditApply::apply`] 参照。上流の `Document::set_cells` が同じ索引を 1 度作るのと同じ
//! 規律）であり、後者は [`EditSchemaQuery::revalidate_columns`] が**指定した列だけ**を舐める
//! ことによる（`schema-engine` の契約）。**編集したセルの数にも、まして列数にも比例しない。**
//!
//! # 観測の縫い目（[`EditSchemaQuery`]）
//!
//! 「全件検証を呼んでいない」ことは**速度では示せない**（`verification.md`
//! 「速度を証拠にしない。証拠は『呼び出しの形』で取る」）。示せるのは**呼び出しを数える**
//! ことだけであり、そのためには数えられる縫い目が要る。本層は `schema-engine` へ
//! **必ずこの縫い目を通して**問い合わせる:
//!
//! - 本番の実装は [`SchemaEngineQuery`] であり、`schema-engine` の公開面をそのまま呼ぶ。
//!   [`EditApply::new`] はこれを使う（**本番の経路が縫い目を通る**）。
//! - 検査は [`EditApply::with_query`] で**数える実装**を差し込み、呼び出しの形
//!   （回数・渡った値・列の集合）を観測する（`tests/edit_apply.rs`）。
//!
//! 縫い目が公開面にあるのは、数える側が**本番の実装を包めるようにする**ためである
//! （数える対象が本番の経路であることを、委譲先を写しではなく本番の実装にすることで保証する。
//! 「本番が呼んでいない模擬」を作らない、という `structure.md`
//! 「一括メソッドを置くだけでは足りない。本番の一括経路からそれが呼ばれていることを示すこと」
//! の規律そのものである）。
//!
//! 3 つ目の [`EditSchemaQuery::validate_sheet`] は本層が**呼ばない**。縫い目が
//! `schema-engine` の 3 つの入口を数えられる形で並べているのは、**呼ばないことを数えて
//! 示すため**である（口を置かなければ「呼ばない」ことも数えられない）。本番の実装はこれを
//! `validate_sheet` へ委ねる。後続（5.2 の `GridSession` の開設）が全件検証を要するときも、
//! 同じ縫い目を通れば呼び出しの形が観測できる。
//!
//! 縫い目は `Send + Sync` を要求する。操作口（5.2 の `GridSession`）はウィンドウごとの文脈
//! から呼ばれるため、保持する値はスレッドを跨げなければならない（`src-tauri` の
//! `manage`（`Send + Sync + 'static` を要求する）と同じ理由）。実装の状態は
//! **引数と返り値に現れない**ため、数を数える側は内部可変性（`Mutex`）で記録する
//! （`tests/edit_apply.rs` の `CountingQuery`）。
//!
//! # ドキュメントへの書き込みは 1 回であり、部分適用が無い
//!
//! 判定をすべて済ませてから、[`Document::set_cells`] を**1 回**呼ぶ（1 セルの編集でも
//! 複数セルの編集でも同じである）。事前検査（対象シートの存在・列数の一致・列の範囲・行の
//! 存在）は書き込みの前に済ませるため、1 つでも不正なら**判定も呼ばず、1 つのセルも書かない**。
//! 判定が返した値のうちドキュメントへ書くのは**編集した列の値**だけであり、行の他の列の値は
//! 判定が返したもの（＝入力そのもの）と一致するため触れない。
//!
//! ## 同じセルを 2 度書く命令
//!
//! 同じセルが複数回現れた命令は、**後ろのものを残す 1 回の書き込み**として扱う
//! （上流の一括経路（`Document::set_cells`）の契約と同じ last-wins）。判定に渡る値は
//! **実際に書かれる値**そのものである — 重複を畳んでから判定するため、「判定した値」と
//! 「書いた値」が食い違う経路は無い。
//!
//! # 変換の記録は表示文字列で運ぶ（要件 3.4）
//!
//! [`CoercionNotice`] の `before` / `after` は [`String`] である。境界（6.1）は
//! `CellValue` をそのまま運べず（`CellValue::Int` の 64 ビット整数と識別子を出せない。
//! design.md「Existing Architecture Analysis」）、`Document` の行も `Clone` を持たない。
//! したがって**表示文字列へ写してから**運ぶ。写しの規則は `view` 層の [`display_text`] が
//! 唯一の源であり（2.2 が定め、5.1 の `WindowCodec` も同じものを再利用する）、本層は
//! **2 つ目の写しを作らない**。画面に見えている文字列と、変換の前後として提示する文字列が
//! 食い違わないことは、この 1 つの源が保証する。
//!
//! # 違反の総数の源（[`EditOutcome::violation_total`]）
//!
//! 本タスクが `violation_total` に入れるのは、**再検証のために呼んだ列に閉じた総数**
//! （[`SheetReport::total_violations`]）である。要件 4.3 が求める「表示中のシートに存在する
//! 違反の総数」は**シート全体**の数であり、それを答えるのは 5.2 の `GridSession` である
//! （design.md の Invariants「`violation_total` は `apply` / `undo` / `redo` の直後につねに
//! 最新である。全件検証の再実行ではなく、判定が返した違反との差分で索引を更新する」）。
//! 群 3 の残りのタスク（3.2 の行の追加・削除・複製、3.4 の貼り付け）は再検証する列の集合を
//! 広げることでこの数を広げる。
//!
//! 本層が再検証の報告から読むのは**総数だけ**である。したがって保持の上限 0
//! （[`ValidationOptions::capped`]）で呼ぶ — 10 万行 × 1 列の違反一覧を保持する理由が無く、
//! 総数と違反を持つ行の一覧は上限に関わらず保たれる（`schema-engine` の契約）。違反の
//! **一覧**（どの行のどの位置か）が要るのは 5.2 であり、そちらは判定が返した違反を使う
//! （design.md の Invariants）。
//!
//! 1 行分の判定（[`EditVerdict`]）も違反を持つが、本層はそれを `violation_total` に写さない。
//! 書き込みの後に当該列を再検証した報告が**同じ位置・同じ理由**を持つためである（どちらも
//! 同じ値と同じ計画から導かれ、連続する違反の有無を除けば報告のほうが広い — 報告は行を跨ぐ
//! 性質（一意性と参照の実在）も含むが、1 行分の判定は含まない）。2 つを足すと同じ違反を
//! 二重に数える。
//!
//! # 履歴（4.x）との境目
//!
//! 本層は履歴を積まない（`UndoStack` は 4.1 が足す）。**空の `SetCells`（セル 0 個）は
//! 何も変えないため、適用の結果を「影響を受けた行なし・違反の総数 0」として返し、判定も
//! 再検証も呼ばない。** 4.1 はこれを履歴に積まないこと — 状態を変えない命令を積むと、
//! 取り消しが何も戻さない操作になる。
//!
//! # 群 3 の残りのタスクへの拡張
//!
//! [`EditCommand`] は `SetCells` だけを持つ。3.2（`InsertRows` / `RemoveRows` /
//! `DuplicateRows`）、3.3（`SetNested`）、3.4（`PasteRange`）は**変種を足す**ことで進み、
//! 本モジュールの構造（事前検査 → 判定 → 1 回の書き込み → 列を限定した再検証 → 写し）を
//! 作り直さない。行を増減する命令は `row_count` が変わり、`affected` に増減した行が加わる。
//! `SetNested` は打たれた文字が JSON になるが、判定を呼ぶ形は変わらない
//! （design.md「EditApply」の Implementation Notes）。

use std::collections::HashMap;

use document_format::{CellValue, CellWriteError, Document, RowId, Sheet, SheetId};
use schema_engine::{
    validate_columns, validate_sheet, validate_write, Coercion, ColumnIndex, CompiledSchema,
    EditVerdict, SheetReport, ValidationOptions, WriteOrigin, WriteVerdict,
};

use crate::error::GridError;
use crate::types::CellAddress;
use crate::view::display_text;

/// 編集命令（design.md「EditApply」の Service Interface。要件 3.3）。
///
/// **本タスクが持つのは `SetCells` だけである。** `SetNested`（3.3）、`InsertRows` /
/// `RemoveRows` / `DuplicateRows`（3.2）、`PasteRange`（3.4）は後続のタスクが**変種として
/// 足す** — 既存の変種の形（セルの位置と打たれた文字の対）を変えないため、適用の経路
/// （事前検査 → 判定 → 書き込み → 再検証）も作り直しにならない。
///
/// 値は**打たれた文字**として運ぶ。数値・真偽・日付として解釈するのは `schema-engine`
/// であり、本層もフロントエンドも値を型として扱わない（design.md 同節の Implementation
/// Notes）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditCommand {
    /// 指定したセルへ打たれた文字を書く。
    ///
    /// 同じ行の同じ列が複数回現れた場合、**後ろのものが残る**（適用は
    /// [`Document::set_cells`] の一括経路であり、その契約が last-wins である）。判定へ
    /// 渡る値も**実際に書かれる値**である（モジュール docs「同じセルを 2 度書く命令」）。
    SetCells {
        /// 書くセルと、そこへ打たれた文字。
        cells: Vec<(CellAddress, String)>,
    },
}

/// 編集を適用した結果（design.md「EditApply」の Service Interface。要件 3.4）。
///
/// **判定が返したものを写しただけ**であり、本層が足す解釈は無い。`affected` と `row_count` は
/// 画面側が窓の記憶を捨てる（要件 1.7）ためと、行数の変化を提示する（要件 6.2）ための要約で
/// ある。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOutcome {
    /// 影響を受けた行の識別子（重複を畳み、命令に現れた順）。
    ///
    /// `SetCells` では**編集したセルの行**である（行の増減は無い）。design.md の
    /// Postconditions は `apply` がこれを必ず含むことを求める。
    pub affected: Vec<RowId>,
    /// 型強制によって値が変換されたセル（要件 3.4）。変換が起きなければ空である。
    pub coercions: Vec<CoercionNotice>,
    /// 違反の総数（本タスクでは**再検証した列に閉じた総数**。モジュール docs
    /// 「違反の総数の源」）。
    pub violation_total: usize,
    /// 適用の**後**のシートの行数。`SetCells` は行を増減しないため、適用の前後で変わらない
    /// （行を増減する命令（3.2）がこの欄に変化を載せる）。
    pub row_count: usize,
}

/// 型強制によって値が変換されたことの記録（design.md「EditApply」の Service Interface。
/// 要件 3.4）。
///
/// 変換の**前と後**の双方を表示文字列として持つ。「変換が起きたこと」と「変換前の値」を
/// 人が確認できる形にするためである（要件 3.4）。表示文字列の写しは `view` 層の
/// [`display_text`] が唯一の源であり（モジュール docs「変換の記録は表示文字列で運ぶ」）、
/// 本層は 2 つ目の写しを持たない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoercionNotice {
    /// 変換が起きたセルの位置。
    pub cell: CellAddress,
    /// 変換**前**の値の表示文字列（打たれた文字そのもの）。
    pub before: String,
    /// 変換**後**の値の表示文字列（ドキュメントへ書かれた値）。
    pub after: String,
}

/// 編集経路が `schema-engine` へ問い合わせる口（要件 11.4 の観測の縫い目）。
///
/// 本層が `schema-engine` を呼ぶ経路はこの 3 つだけであり、[`EditApply`] は必ずこの縫い目を
/// 通る（モジュール docs「観測の縫い目」）。メソッドの意味は上流の同名の関数と同じである
/// （本層は判定も検証も持たず、依頼と写ししかしない）。
///
/// # 実装の契約
///
/// - [`EditSchemaQuery::judge_write`] は `WriteOrigin::Edit` の判定を返す。**`Rejected` を
///   持たない**（[`EditVerdict`] は 2 変種しか持たない）。返る `values` の長さは入力と同じで
///   あり、`coercions` はその並びと同じ長さで値ごとに 1 件が対応する（`schema-engine` の
///   契約。本条項は **本層が列の位置で値を読む**根拠である）。
/// - [`EditSchemaQuery::revalidate_columns`] は指定した列に閉じた報告を返す。列の並びは
///   結果に影響しない（上流が列添字の昇順へ正規化する）。
/// - [`EditSchemaQuery::validate_sheet`] は全件の報告を返す。**編集経路はこれを呼ばない。**
pub trait EditSchemaQuery: Send + Sync {
    /// 1 行分の書き込み判定（編集経路）。強制も含む（要件 3.3, 3.4, 3.5）。
    fn judge_write(&self, schema: &CompiledSchema, values: Vec<CellValue>) -> EditVerdict;

    /// 指定した列だけの再検証（要件 11.4）。
    fn revalidate_columns(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        columns: &[ColumnIndex],
        options: &ValidationOptions,
    ) -> SheetReport;

    /// シート全件の検証。**編集経路は呼ばない**（要件 11.4。数えて示すための口である）。
    fn validate_sheet(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        options: &ValidationOptions,
    ) -> SheetReport;
}

/// 本番の縫い目: `schema-engine` の公開面をそのまま呼ぶ（モジュール docs「観測の縫い目」）。
///
/// 状態を持たない。公開しているのは、数を数える検査が**この実装を包んで**呼び出しの形を
/// 観測できるようにするためである（数える対象が本番の経路であることを、委譲先を写しではなく
/// 本番の実装にすることで保証する）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SchemaEngineQuery;

impl EditSchemaQuery for SchemaEngineQuery {
    fn judge_write(&self, schema: &CompiledSchema, values: Vec<CellValue>) -> EditVerdict {
        match validate_write(WriteOrigin::Edit, schema, values) {
            WriteVerdict::Edit(verdict) => verdict,
            // `validate_write` は渡された経路の腕をそのまま返す（`write` 層の
            // `match origin`）ため、`WriteOrigin::Edit` を渡した本経路で収集側の腕は
            // 起こりえない。ここへ来たなら上流の写像が変わっている（判定の分岐ではなく
            // **不変条件の表明**である）。
            WriteVerdict::Collect(_) => {
                unreachable!("WriteOrigin::Edit の判定が収集経路の判定になった")
            }
        }
    }

    fn revalidate_columns(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        columns: &[ColumnIndex],
        options: &ValidationOptions,
    ) -> SheetReport {
        validate_columns(doc, sheet, schema, columns, options)
    }

    fn validate_sheet(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        options: &ValidationOptions,
    ) -> SheetReport {
        validate_sheet(doc, sheet, schema, options)
    }
}

/// 編集命令を適用し、判定を `schema-engine` に委ねる唯一の経路（design.md「EditApply」）。
///
/// 対象シートと計画（[`CompiledSchema`]）を開いたときに 1 度だけ受け取り、以後の適用で
/// 使い回す。**計画は不変であり、スキーマが変われば作り直す**（design.md「GridSession」の
/// State Management「スキーマが変わったらセッションを作り直す」）— 作り直しを忘れると列の
/// 添字がずれる。本層は計画と対象シートの**列数の一致**を適用のたびに確かめるため、ずれた
/// 計画は静かに書き込まずに [`GridError::SchemaUnusable`] として止まる（列の添字が
/// ドキュメントの列名と食い違ったまま書くより、止まるほうが回復可能である）。
///
/// 一連の適用（[`EditApply::apply`]）は対象シートの行数を 1 度走査して行の位置の索引を作り
/// （費用は行数に対する 1 パスであり、セル数には依らない）、判定を呼び、ドキュメントへの
/// 書き込みを 1 回だけ行い、編集した列に限定した再検証を 1 回呼ぶ。
pub struct EditApply {
    /// 対象のシート。
    sheet: SheetId,
    /// 開いたときに解決した計画。
    schema: CompiledSchema,
    /// 判定の問い合わせ先（本番は [`SchemaEngineQuery`]）。
    query: Box<dyn EditSchemaQuery>,
}

impl EditApply {
    /// 本番の縫い目（[`SchemaEngineQuery`]）で適用の経路を作る。
    pub fn new(sheet: SheetId, schema: CompiledSchema) -> Self {
        Self::with_query(sheet, schema, Box::new(SchemaEngineQuery))
    }

    /// 問い合わせ先を差し替えて適用の経路を作る。
    ///
    /// 判定の**依頼の形**（何回・何を渡して呼んだか）を観測するための入口である
    /// （モジュール docs「観測の縫い目」。`tests/edit_apply.rs` が数える実装を差し込む）。
    pub fn with_query(
        sheet: SheetId,
        schema: CompiledSchema,
        query: Box<dyn EditSchemaQuery>,
    ) -> Self {
        Self {
            sheet,
            schema,
            query,
        }
    }

    /// 編集命令を適用し、判定が返した値・変換・違反をそのまま写した結果を返す
    /// （design.md「EditApply」の Service Interface。要件 3.3, 3.4, 3.5）。
    ///
    /// 失敗するのは**宣言・宛先が壊れている**場合だけである
    /// （[`GridError::SchemaUnusable`] / [`GridError::UnknownRow`] /
    /// [`GridError::ColumnOutOfRange`]）。値が型に適合しないことは失敗ではない — 値は
    /// ドキュメントに残り、違反として報告される（要件 3.5）。
    ///
    /// 失敗したときは**1 つのセルも書かない**（事前検査を書き込みの前に済ませる。モジュール
    /// docs「ドキュメントへの書き込みは 1 回であり、部分適用が無い」）。
    pub fn apply(
        &mut self,
        doc: &mut Document,
        command: EditCommand,
    ) -> Result<EditOutcome, GridError> {
        match command {
            EditCommand::SetCells { cells } => self.set_cells(doc, cells),
        }
    }

    /// `SetCells` の適用（[`EditApply::apply`] の本体）。
    fn set_cells(
        &mut self,
        doc: &mut Document,
        cells: Vec<(CellAddress, String)>,
    ) -> Result<EditOutcome, GridError> {
        // セッションの前提を先に検査する（命令の中身に依らない）。
        //
        // 1. 計画が列を 1 本も持たない場合、セルの宛先が存在しない。要件 1.6 は列 0 本の
        //    シートを正当とする（表を描かない）ため、これは「編集できないスキーマ」で
        //    あって壊れた宣言ではない。
        // 2. 計画の列数と対象シートの列数が食い違う場合、列の添字がドキュメントの列名と
        //    対応しない（design.md の事前条件「`schema` は同じ `sheet` から `compile` した
        //    ものであること」が破れている）。
        let columns = self.schema.column_count();
        if columns == 0 {
            return Err(GridError::SchemaUnusable { sheet: self.sheet });
        }
        // 空の命令は何も変えない（判定も再検証も呼ばない。モジュール docs「履歴（4.x）との
        // 境目」）。ただしセッションの前提は空の命令でも検査する（前提は命令に依らない）。
        if cells.is_empty() {
            let row_count = self.target_sheet(doc)?.rows().len();
            return Ok(EditOutcome {
                affected: Vec::new(),
                coercions: Vec::new(),
                violation_total: 0,
                row_count,
            });
        }

        // 事前検査（読み）。行の位置の索引を 1 度だけ作り、列の範囲と行の存在を確かめながら、
        // **編集の対象になった行ごとに**その行の値（打たれた文字を当該の列へ置いたもの）を
        // 組み立てる。**1 つでも不正なら判定も呼ばず、1 つのセルも書かない。**
        //
        // 同じセルが 2 度現れた命令はここで**後ろのものを残す 1 つの編集**へ畳む（モジュール
        // docs「同じセルを 2 度書く命令」）。畳んでから判定するため、判定に渡る値と実際に
        // 書かれる値が食い違う経路が無い。
        //
        // 行ごとにまとめるのは、上流の判定（`validate_write`）が**1 行分の値**を受け取る
        // ためである。1 つの行の複数のセルの編集は 1 回の判定で足りる（判定の回数は命令の
        // セル数ではなく**編集の対象になった行数**に等しい。要件 11.4 の費用の形）。
        let mut affected: Vec<RowId> = Vec::new();
        let mut rows: Vec<RowEdit> = Vec::new();
        {
            let sheet = self.target_sheet(doc)?;
            let positions: HashMap<RowId, usize> = sheet
                .rows()
                .iter()
                .enumerate()
                .map(|(position, row)| (row.id(), position))
                .collect();
            let mut seen: HashMap<RowId, usize> = HashMap::with_capacity(cells.len());
            for (address, text) in cells {
                let column = address.column();
                if column.index() >= columns {
                    return Err(GridError::ColumnOutOfRange {
                        column,
                        count: columns,
                    });
                }
                let Some(position) = positions.get(&address.row()).copied() else {
                    return Err(GridError::UnknownRow { row: address.row() });
                };
                match seen.get(&address.row()).copied() {
                    Some(index) => rows[index].edit(column, text),
                    None => {
                        seen.insert(address.row(), rows.len());
                        affected.push(address.row());
                        rows.push(RowEdit::new(
                            address.row(),
                            sheet.rows()[position].values().to_vec(),
                            column,
                            text,
                        ));
                    }
                }
            }
        }

        // 判定（型システム）。編集の対象になった行ごとに、その行の値を（当該の列を打たれた
        // 文字へ置き換えて）渡し、返った値と変換の記録をそのまま写す。列の型を見て受理を
        // 決める分岐はここに無い。
        let mut writes: Vec<(RowId, usize, CellValue)> = Vec::new();
        let mut coercions: Vec<CoercionNotice> = Vec::new();
        for row in rows {
            let (decided, recorded) = match self.query.judge_write(&self.schema, row.edited_values())
            {
                // どちらの腕でも、返った値と変換の記録をそのまま受け取る（**判定の分岐を
                // 書かない**）。判定が運ぶ違反は、書き込みの後の再検証が同じ位置・同じ理由で
                // 持つ（モジュール docs「違反の総数の源」）。
                EditVerdict::Accepted { values, coercions } => (values, coercions),
                EditVerdict::AcceptedWithViolations {
                    values, coercions, ..
                } => (values, coercions),
            };
            for (column, _) in &row.edits {
                // `decided` の長さは渡した値の並びと同じであり、`coercions` は値ごとに 1 件が
                // 対応する（`schema-engine` の契約。縫い目の docs 参照）。
                let written = decided[column.index()].clone();
                if let Some(Coercion::Converted { from }) = recorded.get(column.index()) {
                    coercions.push(CoercionNotice {
                        cell: CellAddress::new(row.row, *column),
                        before: display_text(from).into_owned(),
                        after: display_text(&written).into_owned(),
                    });
                }
                writes.push((row.row, column.index(), written));
            }
        }

        // 書き込みは 1 回（置換であって追加ではない。行の集合・並び・識別子は変わらない）。
        doc.set_cells(self.sheet, &writes).map_err(write_error)?;

        // 当該列に限定した再検証（**全件検証は呼ばない**。要件 11.4）。列の集合は編集した列を
        // 昇順に畳んだものであり、1 セルの編集では 1 列だけである。
        let mut revalidated: Vec<ColumnIndex> = writes
            .iter()
            .map(|(_, column, _)| ColumnIndex::new(*column))
            .collect();
        revalidated.sort_unstable();
        revalidated.dedup();
        let report = self.query.revalidate_columns(
            doc,
            self.sheet,
            &self.schema,
            &revalidated,
            &ValidationOptions::capped(0),
        );

        Ok(EditOutcome {
            affected,
            coercions,
            violation_total: report.total_violations(),
            // `SetCells` は行を増減しないため、適用の前後で同じ数になる。適用の後の行数を
            // 改めて読む（行数を変える命令（3.2）がこの経路をそのまま使えるようにする）。
            row_count: self.target_sheet(doc)?.rows().len(),
        })
    }

    /// 対象シートを引く。文書に無い場合と、計画の列数と食い違う場合は使用不能として返す。
    ///
    /// 文書にシートが無い状態・列数の食い違う計画は、セッションが前提とするシート（とその
    /// 計画）が使えない状態である（[`GridError`] の 5 変種にこれ以上適切な変種は無い。
    /// `error.rs` の docs「宣言・指定が壊れている」）。
    fn target_sheet<'d>(&self, doc: &'d Document) -> Result<&'d Sheet, GridError> {
        let sheet = doc
            .sheet_by_id(self.sheet)
            .ok_or(GridError::SchemaUnusable { sheet: self.sheet })?;
        if sheet.columns().len() != self.schema.column_count() {
            return Err(GridError::SchemaUnusable { sheet: self.sheet });
        }
        Ok(sheet)
    }
}

/// 打たれた文字を、判定へ渡すセル値へ写す。
///
/// **本層が行う唯一の解釈である。**空の文字列は**値なし**（[`CellValue::Null`]）とする —
/// 要件 3.7 の「値を消して値なしへ戻す」を、境界が運ぶ文字列だけで表せるようにするためで
/// ある。空でない文字列は [`CellValue::Text`] としてそのまま渡す（数値や日付として読むのは
/// `schema-engine` の規則表である）。
///
/// **列の型は見ない。**列ごとに変わるのは「適合するか」であり、それは `schema-engine` が
/// 決める。本層が写すのは値の**在否**だけであり、この規則はどの列にも同じく適用される。
///
/// 値なしと空のテキストは**画面で区別できない**（`view` 層の [`display_text`] は両者を空
/// 文字列に写し、「違反あり」以外の絞り込みも両者を同じ行として選ぶ）。したがってこの写しが
/// 画面から見える情報を失うことはなく、境界に「区別を運ぶ手段」も無い（design.md
/// 「EditApply」の Implementation Notes は値を文字列で運ぶと定めている）。
fn edited_value(text: &str) -> CellValue {
    if text.is_empty() {
        CellValue::Null
    } else {
        CellValue::Text(text.to_owned())
    }
}

/// 編集の対象になった **1 行分**の編集（判定へ渡す値と、書き込むセルの並び）。
///
/// 上流の判定は 1 行分の値を受け取るため、編集を**行ごとにまとめてから**判定する。行ごとに
/// まとめると、同じ行の複数のセルを編集しても判定は 1 回で済み、かつ**同じセルが 2 度現れた
/// 命令**を後ろのものを残す 1 つの編集へ畳める（モジュール docs「同じセルを 2 度書く命令」）。
struct RowEdit {
    /// 対象の行。
    row: RowId,
    /// 行の**適用前**の値（列の添字で並ぶ）。
    values: Vec<CellValue>,
    /// 書くセル（列の添字）と、そこへ打たれた文字（命令に現れた順）。
    edits: Vec<(ColumnIndex, String)>,
}

impl RowEdit {
    /// 行の値を読み取り、1 つ目の編集を載せて作る。
    fn new(row: RowId, mut values: Vec<CellValue>, column: ColumnIndex, text: String) -> Self {
        // 行が値を持たない列への書き込みは、値なしを挟んで位置を合わせる（`validate_write` は
        // 値の添字を列の添字として読む。値なしの列は上流でも値なしとして扱われる）。
        values.resize(values.len().max(column.index() + 1), CellValue::Null);
        Self {
            row,
            values,
            edits: vec![(column, text)],
        }
    }

    /// 同じ行の別のセル（または同じセルの 2 度目）の編集を載せる。
    ///
    /// 同じセルが既に載っている場合は**後ろのものを残す**（上流の一括経路の last-wins と
    /// 同じ規則。モジュール docs「同じセルを 2 度書く命令」）。
    fn edit(&mut self, column: ColumnIndex, text: String) {
        self.values
            .resize(self.values.len().max(column.index() + 1), CellValue::Null);
        match self
            .edits
            .iter_mut()
            .find(|(existing, _)| *existing == column)
        {
            Some(found) => found.1 = text,
            None => self.edits.push((column, text)),
        }
    }

    /// 判定へ渡す 1 行分の値（打たれた文字を当該の列へ置いたもの）。
    fn edited_values(&self) -> Vec<CellValue> {
        let mut values = self.values.clone();
        for (column, text) in &self.edits {
            values[column.index()] = edited_value(text);
        }
        values
    }
}

/// `document-format` の書き込みの誤りを本クレートの誤り型へ写す。
///
/// 本層の事前検査（列の範囲と行の存在）が同じ判定を先に通しているため、ここへ来るのは
/// **事前検査の後にドキュメントが変わった**場合だけである。それでも写しを置くのは、上流の
/// 誤りを捨てる（握り潰す）経路を作らないためである。文書にシートが無い場合は
/// [`GridError::SchemaUnusable`] へ写す（[`EditApply::target_sheet`] と同じ理由）。
fn write_error(error: CellWriteError) -> GridError {
    match error {
        CellWriteError::UnknownSheet { sheet } => GridError::SchemaUnusable { sheet },
        CellWriteError::UnknownRow { row } => GridError::UnknownRow { row },
        CellWriteError::UnknownColumn { column, columns } => GridError::ColumnOutOfRange {
            column: ColumnIndex::new(column),
            count: columns,
        },
    }
}

//! Document 集約ルートと構造的不変条件(タスク 2.1 / 2.3。要件 1.1, 1.5, 1.6, 7.1, 7.5, 7.6, 8.4)。
//!
//! # 集約ルート
//!
//! design「Domain Model」: **集約ルートは [`Document`]**。すべての変更は `Document` を
//! 経由し、シート・行・添付は `Document` の外で独立に存在しない。[`Sheet`] の変更
//! メソッドはクレート可視(`Document` が委譲する経路)で、外部から可変のシートを得る
//! 口は無い([`Document::sheet_by_id`] は共有参照しか返さない)。
//!
//! # 添付の集約(要件 7.1, 7.5, 7.6)
//!
//! 添付は [`AttachmentRegistry`] が一手に保持し、[`Document`] が所有する
//! (design ER 図 `Document ||--o{ Attachment : holds`)。追加は
//! [`Document::add_attachment`](content-addressed で冪等)、取得は
//! [`Document::attachment`]、未参照の一覧は [`Document::unreferenced_attachments`] である。
//! 後者は全シート・全行のセル値を集計して [`AttachmentId`] 昇順で
//! 返す**読み取り専用**の操作で、添付を削除も書き換えもしない(削除の API は存在しない)。
//! 集計の詳細と不透明バイト列の扱いは model/attachment.rs のモジュール docs 参照。
//!
//! # マクロの記録(タスク 1.2。要件 1.1, 1.2, 1.5)
//!
//! マクロの記録(名前・種別・ソース)[`MacroRecord`] の並びも [`Document`] が保持し
//! (design「Logical Data Model」の `macros.json`)、読み書きの口は [`Document::macros`] /
//! [`Document::set_macros`] である。**本クレートは形だけを持ち、意味を持たない**:
//! ソースの検証・能力宣言の読み取り・名前の一意性と置き換えの規則は下流
//! (`crates/macro-runtime`)が所有する(design 決定 4。model/macro_record.rs の
//! モジュール docs)。並びは保存順のままであり、**並べ替えも重複の検査もしない**。
//! 未知フィールドの保持は `macros.json` のトップレベルとマクロ 1 件のそれぞれで行う
//! (前者は `macros_preserved`、後者は [`MacroRecord`] の中。要件 6.2 / 6.3)。
//!
//! # ドキュメント識別子を保持する(タスク 4.8 の親の裁定)
//!
//! [`Document`] は自分の [`DocumentId`] を持つ([`Document::document_id`])。
//! `document.json` はこの識別子を永続化する(タスク 4.3)ため、モデルが持たないと
//! `Document → Parts → Document → Parts` の往復で識別子が再発行され、
//! 要件 3.1(同一内容 → 同一バイト列)が壊れる。新規文書は [`Document::new`] が
//! 所有する [`IdFactory`] から 1 個発行し、読み込み経路は
//! 復号済みの識別子を据える(クレート可視の [`Document::with_document_id`])。
//!
//! 列名と未知フィールドも同じ理由でモデルが保持する(それぞれ
//! [`Document::set_sheet_columns`] と [`Document::set_preserved_fields`]。
//! model/sheet.rs のモジュール docs「列名を保持する」「未知フィールドの保持」参照)。
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
//! 「文書内で `SheetId` / `RowId` はそれぞれ一意」は**構築経路による構造保証**が主である:
//! 識別子は [`Document`] が所有する [`IdFactory`] からの発行のみで得られ(厳密昇順)、
//! **文書内で識別子を発行するのはその 1 者だけ**であり、種別ごとの一意性はそこから従う。
//! ただし**識別子を受け取って挿入する経路が 1 つだけ存在する**:
//! [`Document::insert_rows_at`] である。この経路は識別子を発行せず、外から来た [`Row`] を
//! そのまま受け取る。渡される識別子は [`Document::remove_rows`] が返した行に限らない:
//! [`crate::parts::RowsCodec::decode`] と [`crate::parts::SheetRows::into_rows`] が公開面に
//! あるため、**`sheets/<ulid>.jsonl` のバイト列から復号した任意の識別子**も外から持ち込める
//! (どのシートのものでもよく、他文書で発行した識別子でもよい)。したがってこの経路は
//! 構築による保証が及ばず、**文書内の全シートの行識別子の集合に対して検査**する:
//! 現存する識別子と重複する要求は [`RowInsertionError::DuplicateRow`] として拒む
//! (検査は挿入の前に 1 パスで行い、失敗時は 1 行も挿入しない)。この検査により、
//! **文書内で同じ行識別子が 2 箇所に存在する状態は作れない**。
//!
//! 検査は「現存する識別子の集合」との比較であるため、**取り除かれて文書から消えた識別子**
//! は再び持ち込める(取り除いた行を元のシートへ戻す取り消しがこれであり、入れた時点で
//! 文書内に同じ識別子は 1 つしか無い)。これは不変条件に反しない: 一意性が禁じるのは
//! **同じ文書内で同時に 2 箇所に現れること**だけである。
//!
//! [`Document::insert_row_at`] は空の行を作り、識別子は発行元から受け取るため、
//! 一意性は発行の単一性による構造保証のままであり、集合を引く検査を持たない
//! (引いても常に空振りになる)。
//! 読み込み時の重複検出・報告(要件 4.3 の `DuplicateId`)は
//! 読み込み経路 `StructuralValidator` の役割で、本モデルの責務ではない。
//! 添付の識別子は content-addressed(内容の BLAKE3)なので、同一バイト列の再登録は
//! 同一エントリへの冪等な登録であり、重複した識別子を作る経路が無い。
//!
//! # design エラー表との関係
//!
//! design エラー表の 10 変種([`DocumentError`](crate::error::DocumentError))は I/O・
//! 形式破損の診断である。モデル操作の失敗(実在しないシート・行の指定、順列でない並び替え
//! 要求、位置指定の挿入の範囲外)は表のどの変種にも対応しないため、`DocumentError` に
//! 増やさず本モジュールの最小ローカル型 [`UnknownSheet`] / [`UnknownRow`] /
//! [`ReorderError`] / [`CellWriteError`] / [`RowRemovalError`] / [`RowInsertionError`]
//! とする
//! ([`IdParseError`](crate::ids::IdParseError) と同じ
//! 「表に無いものはローカルに暫く置く」パターン)。panic にしないので呼び出し元が
//! 実行時エラーとして扱える。
//!
//! # 規模(要件 8.4)
//!
//! 文書は全件オンメモリ(`Vec<Sheet>` / `Vec<Row>`。遅延ロードは対象外)で、合計 10 万行
//! の保持を保証する。10 万行超をモデル側で拒否はしない(要件 8.5 の超過通知は読み込み
//! 経路 `OpenOutcome::beyond_supported_scale` の役割)。
//!
//! # 読み込み時の形式変換を覚える状態(要件 6.4)
//!
//! [`Document::was_converted_from_an_older_format`] は、そのモデルが読み込み経路
//! ([`crate::parts::from_parts`])で移行チェーンを適用して得られたものかを表す。設定は
//! クレート内部の `mark_converted_from_an_older_format` だけであり、
//! 移行が実際に適用された 1 箇所([`crate::parts::from_parts`])に限る。**この状態は
//! wire 形式に含めない**: ドキュメントの内容ではないため `document.json` へ書かず、
//! [`crate::parts::to_parts`] も無視する。したがって `open → save` を繰り返しても
//! 退避は初回だけであり(保存後の読み直しでは `false`)、既存の往復・決定性テストの
//! バイト列は変わらない。
//!
//! # 後続タスクとの境界
//!
//! * 各シートがちょうど 1 つ持つルートスキーマ(要件 1.2)は [`SchemaPart`] の所有であり、
//!   タスク 2.2 で [`Sheet::root_schema`] として結線済みである(新規シートは
//!   [`SchemaPart::empty`] で初期化し、差し替えは [`Document::set_root_schema`] の置換
//!   のみ = 0 個・2 個を作る口は無い)。
//! * 添付はタスク 2.3 で結線済みである: 保持と参照集計は [`AttachmentRegistry`] が行い、
//!   `attachments/<hex>.bin` への符号化は parts 層(タスク 4.3)、参照の実在検証
//!   (要件 7.4 の `DanglingAttachmentRef`)は読み込み経路 `StructuralValidator`
//!   (タスク 4.7)の責務で、本モジュールは添付の内容も参照整合性も解釈しない。
//! * 行に値を設定する経路は [`Document::set_row_values`] として用意した(行データの
//!   復号 = タスク 4.5 が消費する)。永続化(4.x)は本モジュールの範囲外。
//! * [`Document`] / [`Sheet`] / [`Row`] は `Clone` を実装しない(識別子発行状態の clone
//!   方針が未定。model/sheet.rs の「Clone を実装しない理由」参照)。

mod attachment;
mod macro_record;
mod schema_part;
mod sheet;

use std::collections::HashSet;

use thiserror::Error;

use crate::ids::{AttachmentId, DocumentId, IdFactory, RowId, SheetId};
use crate::json::PreservedFields;
use crate::value::CellValue;

pub use attachment::{Attachment, AttachmentRegistry};
pub use macro_record::{MacroKind, MacroRecord};
pub use schema_part::{RawJson, SchemaPart, TypeDef, TypeRef};
// エンベロープ文法のキーは parse(本モジュール)と符号化(タスク 4.4 の `SchemaCodec`)が
// 共有する。文字列リテラルを両実装へ散在させないため、`parts` 層へ同じ定数を渡す。
pub(crate) use schema_part::{KEY_DEFINITION, KEY_ID, KEY_ROOT, KEY_TYPES};
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

/// 実在しないシートの指定([`Document::add_row`] / [`Document::rename_sheet`] /
/// [`Document::set_sheet_columns`] / [`Document::set_root_schema`])。
///
/// [`ReorderError::UnknownSheet`] と同じ意味(未知シート)で、シート指定の操作は
/// どの経路でも panic ではなくこの型で報告される。行を対象にする操作
/// ([`Document::remove_rows`] / [`Document::insert_row_at`] / [`Document::insert_rows_at`])
/// は呼び出し元が 1 つの `Result` に統合できるよう、それぞれの誤り型の `UnknownSheet`
/// 変種で報告する(表に無いものをローカルに置く規律は同じ)。
#[derive(Debug, Error, PartialEq, Eq)]
#[error("no such sheet in document: {sheet}")]
pub struct UnknownSheet {
    /// 指定されたが存在しなかったシート識別子。
    pub sheet: SheetId,
}

/// 実在しない行の指定([`Document::set_row_values`])。
///
/// [`UnknownSheet`] と同じく、design エラー表(I/O・形式診断の 10 変種)に対応変種が
/// 無いモデル操作の失敗である。指定シートに属さない行(他シートの行・他文書の行)も
/// 同じ失敗として報告し、panic しない。
#[derive(Debug, Error, PartialEq, Eq)]
#[error("no row {row} in sheet")]
pub struct UnknownRow {
    /// 指定されたが存在しなかった行識別子。
    pub row: RowId,
}

/// [`Document::set_cells`] の失敗。
///
/// [`UnknownRow`] / [`ReorderError`] と同じ規律のモデル局所の誤り型である: 判別可能な
/// 変種がそれぞれ文脈(どのシート・どの行・どの列)だけを持ち、表示用の文言を持たない
/// (文言は呼び出し元が組み立てる)。design エラー表(I/O・形式診断の 10 変種)に対応
/// 変種が無いため `DocumentError` には含めない。
///
/// 3 変種は[`Document::set_cells`]の事前検査(1 パス)が判別する:
/// シート自体が未知・対象シートに属さない行・`Sheet::columns` の数を超える列の添字。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CellWriteError {
    /// 指定シート自体が文書に存在しない。
    #[error("no such sheet in document: {sheet}")]
    UnknownSheet {
        /// 指定されたシート識別子。
        sheet: SheetId,
    },
    /// 指定行が対象シートに属さない(他シートの行・他文書の行)。
    #[error("no row {row} in sheet")]
    UnknownRow {
        /// 指定されたが存在しなかった行識別子。
        row: RowId,
    },
    /// 列の添字が `Sheet::columns` の範囲外である。
    #[error("column {column} is out of range: sheet has {columns} columns")]
    UnknownColumn {
        /// 指定された列の添字(`Sheet::columns` の並びに対する位置)。
        column: usize,
        /// 対象シートが持つ列の数(範囲の上界)。
        columns: usize,
    },
}

/// [`Document::remove_rows`] の失敗。
///
/// [`UnknownRow`] / [`CellWriteError`] と同じ規律のモデル局所の誤り型である: 判別可能な
/// 変種がそれぞれ文脈(どのシート・どの行)だけを持ち、表示用の文言を持たない(文言は
/// 呼び出し元が組み立てる)。design エラー表(I/O・形式診断の 10 変種)に対応変種が無い
/// ため `DocumentError` には含めない。
///
/// 2 変種は[`Document::remove_rows`]の事前検査(1 パス)が判別する: シート自体が未知・
/// 対象シートに属さない行。削除要求に同じ識別子が複数回現れることは誤りではない
/// (同じ行を 2 度消すのは同じ 1 回の削除であり、畳まれる)。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RowRemovalError {
    /// 指定シート自体が文書に存在しない。
    #[error("no such sheet in document: {sheet}")]
    UnknownSheet {
        /// 指定されたシート識別子。
        sheet: SheetId,
    },
    /// 指定行が対象シートに属さない(他シートの行・他文書の行)。
    #[error("no row {row} in sheet")]
    UnknownRow {
        /// 指定されたが存在しなかった行識別子。
        row: RowId,
    },
}

/// [`Document::insert_row_at`] / [`Document::insert_rows_at`] の失敗。
///
/// [`RowRemovalError`] と同じ規律のモデル局所の誤り型である: 判別可能な変種がそれぞれ
/// 文脈(どのシート・どの位置・どの行)だけを持ち、表示用の文言を持たない(文言は
/// 呼び出し元が組み立てる)。design エラー表(I/O・形式診断の 10 変種)に対応変種が無い
/// ため `DocumentError` には含めない。
///
/// 3 変種は挿入の事前検査(1 パス)が判別する: シート自体が未知・挿入位置が挿入前の
/// 行数を超える・差し込む行の識別子が**文書内のいずれかのシート**に既にある
/// (識別子の一意性は**文書単位**の不変条件であり、[`Document::reorder_rows`] がこれに
/// 依存している。シート局所の集合では強制できないため、検査は文書全体に対して行う)。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RowInsertionError {
    /// 指定シート自体が文書に存在しない。
    #[error("no such sheet in document: {sheet}")]
    UnknownSheet {
        /// 指定されたシート識別子。
        sheet: SheetId,
    },
    /// 挿入位置が挿入前の行数を超えている(`index == 行数` は末尾への追加として妥当)。
    #[error("row index {index} is out of range: sheet has {rows} rows")]
    IndexOutOfRange {
        /// 指定された挿入位置(挿入前の行順に対する添字)。
        index: usize,
        /// 挿入前の対象シートの行数(範囲の上界)。
        rows: usize,
    },
    /// 差し込む行の識別子が**文書内のいずれかのシート**に現存する
    /// (または同じ呼び出しの中で重複している)。
    #[error("row {row} already exists in sheet")]
    DuplicateRow {
        /// 重複した行識別子。
        row: RowId,
    },
}

/// jxcel ドキュメントの集約ルート。
///
/// 0 個以上のシートを保持し(要件 1.1)、シート順序は [`Document::sheets`] の順である。
/// 添付も [`Document`] が保持し(design ER 図 `Document ||--o{ Attachment : holds`)、
/// 行のセル値からの参照は [`Document::unreferenced_attachments`] で集計される
/// (要件 7.1, 7.5, 7.6。モジュール docs の「添付の集約」参照)。マクロの記録の並びも
/// [`Document`] が保持する([`Document::macros`]。`macros.json` が永続化する**形だけ**で、
/// 解釈は下流が持つ)。すべての変更はこの型
/// 経由であり、識別子は所有する [`IdFactory`] から発行される
/// (一意性が構築で保証される所以。モジュール docs 参照)。
///
/// `PartialEq` は提供しない: `document.json` のトップレベルで保持する未知フィールド
/// ([`crate::json::PreservedFields`])は内部カーソルを等値に含むため、内容の等値を
/// 素直に表せない([`SchemaPart`] と同じ方針。比較はアクセサか `parts::to_parts` の
/// バイト列で行う)。
#[derive(Debug)]
pub struct Document {
    /// ドキュメント識別子(`document.json` が永続化する)。
    id: DocumentId,
    /// 識別子発行口(シート・行で単調カウンタを共有する単発行者)。
    ids: IdFactory,
    /// シート順そのものの列。
    sheets: Vec<Sheet>,
    /// 添付の保持と参照集計(要件 7.1, 7.5, 7.6)。
    attachments: AttachmentRegistry,
    /// マクロの記録の並び(`macros.json`。保存順が一覧の提示順。タスク 1.2)。
    ///
    /// **形だけを保持する**: ソースの検証・能力宣言の読み取り・名前の一意性は本クレートでは
    /// 行わない(model/macro_record.rs のモジュール docs。design 決定 4)。
    macros: Vec<MacroRecord>,
    /// `macros.json` のトップレベルで保持した未知フィールド(要件 6.2 / 6.3)。
    macros_preserved: PreservedFields,
    /// 解釈しない `document.json` トップレベルのフィールド(前方互換。要件 6.2 / 6.3)。
    preserved: PreservedFields,
    /// 読み込み時に形式変換(移行)が適用されたか(要件 6.4)。
    ///
    /// **ドキュメントの内容ではない**: wire 形式(`document.json` 等)には含めず、
    /// [`crate::parts::to_parts`] も無視する。したがって `Document → Parts → Document` の
    /// 往復でこの状態は失われ、再読み込みでは常に `false` から始まる(「変換後の初回保存」を
    /// 判定できるのは読み込み経路から直接得たモデルだけである)。設定は
    /// [`crate::parts::from_parts`] が移行を実際に適用した 1 箇所だけで行う。
    converted_from_an_older_format: bool,
}

impl Document {
    /// 0 シート・0 添付の文書を作る(要件 1.1)。
    ///
    /// ドキュメント識別子は所有する [`IdFactory`] から 1 個発行する
    /// (モジュール docs「ドキュメント識別子を保持する」)。
    #[inline]
    pub fn new() -> Self {
        let mut ids = IdFactory::new();
        let id = ids.new_document_id();
        Self {
            id,
            ids,
            sheets: Vec::new(),
            attachments: AttachmentRegistry::new(),
            macros: Vec::new(),
            macros_preserved: PreservedFields::new(),
            preserved: PreservedFields::new(),
            converted_from_an_older_format: false,
        }
    }

    /// 復号済みの識別子を持つ空の文書を作る(`parts::from_parts` が呼ぶ読み込み経路)。
    ///
    /// [`Document::new`] は識別子を新規発行するため、ファイルから読んだ識別子を持つ文書には
    /// 使えない(使えば要件 3.1 の往復同一性が壊れる)。本経路だけが発行済み識別子で
    /// 文書を始める。
    pub(crate) fn with_document_id(id: DocumentId) -> Self {
        Self {
            id,
            ids: IdFactory::new(),
            sheets: Vec::new(),
            attachments: AttachmentRegistry::new(),
            macros: Vec::new(),
            macros_preserved: PreservedFields::new(),
            preserved: PreservedFields::new(),
            converted_from_an_older_format: false,
        }
    }

    /// ドキュメント識別子(発行後に不変。`document.json` が永続化する)。
    #[inline]
    pub const fn document_id(&self) -> DocumentId {
        self.id
    }

    /// 読み込み時に形式変換(移行)が適用されたか(要件 6.4)。
    ///
    /// `true` のとき、[`crate::DocumentFormatApi::save`] は**初回の保存**で変換前の
    /// ファイルを `<ファイル名>.bak` として退避する(要件 6.4)。
    ///
    /// **この状態は wire 形式に含めない**: ドキュメントの内容ではなく、読み込み経路が
    /// 「古いファイルを変換した」ことを覚えているための一時的な標識である。
    /// [`crate::parts::to_parts`] はこの値を無視するため、保存して読み直した文書は
    /// `false` に戻る(この設計により、既存の往復・決定性テストのバイト列は変わらない)。
    /// この値が `true` になるのは、読み込み経路
    /// ([`crate::parts::from_parts`])が移行チェーンを実際に適用した場合だけである。
    /// [`Document::new`] と [`crate::parts::from_parts`] の現行版の読み込みは `false` を
    /// 返す。
    #[inline]
    pub const fn was_converted_from_an_older_format(&self) -> bool {
        self.converted_from_an_older_format
    }

    /// 「読み込み時に形式変換が適用された」ことを立てる
    /// ([`crate::parts::from_parts`] が移行を適用した 1 箇所だけが呼ぶ)。
    ///
    /// 公開の読み取りは [`Document::was_converted_from_an_older_format`] であり、設定経路を
    /// 公開面へ出さない(利用側が「変換済み」を自称できると退避の分岐が意味を失う)。
    #[inline]
    pub(crate) fn mark_converted_from_an_older_format(&mut self) {
        self.converted_from_an_older_format = true;
    }

    /// `document.json` のトップレベルで保持した未知フィールド(要件 6.2 / 6.3)。
    ///
    /// 読み込み経路(`parts::from_parts`)が復号済みの保持内容をここへ移し、
    /// 保存経路(`parts::to_parts`)が `parts::DocumentPart` へ戻す。
    #[inline]
    pub(crate) fn preserved_fields(&self) -> &PreservedFields {
        &self.preserved
    }

    /// 保持すべき未知フィールドを据える経路(`parts::from_parts` が呼ぶ)。
    #[inline]
    pub(crate) fn set_preserved_fields(&mut self, preserved: PreservedFields) {
        self.preserved = preserved;
    }

    /// 復号済みのシート列を文書のシート列として据える(`parts::from_parts` が呼ぶ)。
    ///
    /// 順序は**与えられた順のまま**であり、`document.json` の配列順がそのまま
    /// ドキュメントのシート順序になる(要件 1.1)。識別子を発行しないため、
    /// [`Document::add_sheet`] では作れない(発行済み識別子が変わってしまう)復元に使う。
    #[inline]
    pub(crate) fn restore_sheets(&mut self, sheets: Vec<Sheet>) {
        self.sheets = sheets;
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

    /// 指定シートの列名(順序付き)を置き換える(タスク 4.8 の親の裁定)。
    ///
    /// 列名は行データのキー順そのものであり(`sheets/<ulid>.jsonl`)、本クレートは中身を
    /// 解釈しない(どの列がどの型かは `schema-engine` が決める)。**書き出し経路が
    /// 列名を外から受け取らない**ため(design `DocumentFormatApi` の Service Interface)、
    /// 列を持つ文書をメモリ上で組み立てるにはこの経路が要る。既存の
    /// [`Document::add_sheet`] のシグネチャは変えず、既定を列名 0 個のままにしてある
    /// (タスク 4.8 の制約)。未知のシートは [`UnknownSheet`] を返し、どのシートの
    /// 列名も変更しない。
    pub fn set_sheet_columns(
        &mut self,
        sheet: SheetId,
        columns: Vec<String>,
    ) -> Result<(), UnknownSheet> {
        match self.sheets.iter_mut().find(|s| s.id() == sheet) {
            Some(target) => {
                target.set_columns(columns);
                Ok(())
            }
            None => Err(UnknownSheet { sheet }),
        }
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

    /// 指定シートのルートスキーマを差し替える(要件 1.2)。
    ///
    /// 置換であり追加ではない: 対象シートは交換後もちょうど 1 つのルートスキーマを持つ
    /// (2 個目を足す口は無い)。未知のシートは実行時エラーとして報告し、既存の
    /// ルートスキーマは無変更のまま残る。
    pub fn set_root_schema(
        &mut self,
        sheet: SheetId,
        schema: SchemaPart,
    ) -> Result<(), UnknownSheet> {
        match self.sheets.iter_mut().find(|s| s.id() == sheet) {
            Some(target) => {
                target.set_root_schema(schema);
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
    pub fn reorder_rows(&mut self, sheet: SheetId, order: &[RowId]) -> Result<(), ReorderError> {
        self.sheets
            .iter_mut()
            .find(|s| s.id() == sheet)
            .ok_or(ReorderError::UnknownSheet { sheet })?
            .reorder_rows(order)
    }

    /// 任意のバイト列を添付として登録し、その識別子を返す(要件 7.1, 7.2, 7.5)。
    ///
    /// バイト列は解釈・変換・再圧縮せずそのまま保持する。識別子は content-addressed
    /// (内容の BLAKE3)なので再登録は冪等であり、同一バイト列ではエントリが増えず
    /// 同一の識別子が返る。未知のシート指定のような失敗経路は無い。
    #[inline]
    pub fn add_attachment(&mut self, bytes: Vec<u8>) -> AttachmentId {
        self.attachments.add(bytes)
    }

    /// 識別子で添付を引く。未登録なら `None`(panic しない)。
    #[inline]
    pub fn attachment(&self, id: AttachmentId) -> Option<&Attachment> {
        self.attachments.get(id)
    }

    /// 添付レジストリの共有参照(昇順反復・件数・参照集計)。
    #[inline]
    pub fn attachments(&self) -> &AttachmentRegistry {
        &self.attachments
    }

    /// マクロの記録の並び(保存順。`macros.json` が永続化する。タスク 1.2)。
    ///
    /// 並びは**与えられた順のまま**であり(保存順が一覧の提示順)、本クレートは内容を
    /// 解釈しない: ソースの検証・能力宣言の読み取り・名前の一意性は下流
    /// (`crates/macro-runtime`)の規則である(model/macro_record.rs のモジュール docs)。
    /// マクロを 1 件も持たない文書は空のスライスを返す。
    #[inline]
    pub fn macros(&self) -> &[MacroRecord] {
        &self.macros
    }

    /// マクロの記録の並びを**置き換える**(要件 1.2 の保存・削除の経路)。
    ///
    /// 並びは与えられた順序のまま保持し(並べ替えない)、内容の検査も正規化もしない
    /// (「同じ名前の保存は置き換え」という規則は下流が適用し、その結果の並びを受け取る)。
    /// 保存経路([`crate::parts::to_parts`])は空の並びのとき `macros.json` を書かない
    /// (省略可能なパート)。
    #[inline]
    pub fn set_macros(&mut self, macros: Vec<MacroRecord>) {
        self.macros = macros;
    }

    /// `macros.json` のトップレベルで保持した未知キー(要件 6.2 / 6.3)。
    ///
    /// 読み込み経路(`parts::from_parts`)が復号済みの保持内容をここへ移し、保存経路
    /// (`parts::to_parts`)が `parts::MacrosPart` へ戻す。
    #[inline]
    pub(crate) fn macros_preserved_fields(&self) -> &PreservedFields {
        &self.macros_preserved
    }

    /// 保持すべき未知フィールドを据える経路(`parts::from_parts` が呼ぶ)。
    #[inline]
    pub(crate) fn set_macros_preserved_fields(&mut self, preserved: PreservedFields) {
        self.macros_preserved = preserved;
    }

    /// どの行のどのセル値からも参照されていない添付の識別子を昇順で返す(要件 7.6)。
    ///
    /// 全シート・全行のセル値を集計し、[`CellValue::Attachment`] の参照を
    /// [`NestedValue`](crate::value::NestedValue) の内側まで再帰的にたどる。**削除も
    /// 書き換えもしない**: 参照が無くなった添付は登録済みのまま残り、
    /// [`Document::attachment`] で取得できる(削除の API は存在しない)。読むだけで
    /// あるため、結果はシート順・行順・登録順に依存せず識別子の昇順に決まる。
    ///
    /// 実在しない識別子への参照は本集計の対象外である(報告は要件 7.4 の
    /// `DanglingAttachmentRef` = 読み込み経路 `StructuralValidator` の責務)。
    pub fn unreferenced_attachments(&self) -> Vec<AttachmentId> {
        let values = self
            .sheets
            .iter()
            .flat_map(|sheet| sheet.rows().iter())
            .flat_map(Row::values);
        self.attachments.unreferenced_attachments(values)
    }

    /// 指定行のセル値を列順の値で**置き換える**(要件 7.3 の参照を作る公開経路)。
    ///
    /// 行データの復号(タスク 4.5)が行 1 件分の値列をそのまま復元するための入口であり、
    /// 追加ではなく置換である(行の識別子は変わらない)。指定シートに属さない行は
    /// [`UnknownRow`] を返し、どの行の値も変更しない。
    pub fn set_row_values(
        &mut self,
        sheet: SheetId,
        row: RowId,
        values: Vec<CellValue>,
    ) -> Result<(), UnknownRow> {
        self.sheets
            .iter_mut()
            .find(|s| s.id() == sheet)
            .ok_or(UnknownRow { row })?
            .set_row_values(row, values)
    }

    /// 指定シートの複数のセルを**1 回の呼び出しで**書き換える(要件 3.5, 3.7)。
    ///
    /// `cells` の各要素は（行識別子, 列の添字, 値）であり、列の添字は
    /// [`Sheet::columns`] の並びに対する位置である。値の個数と列数の一致は**保存時の
    /// 門**(`RowsCodec::encode`)が担うため、本メソッドは判定しない(既存規約)。
    ///
    /// **行の索引を 1 度だけ作り、事前検査を 1 パスで行う**: 合計 O(行数 + 変更数) に
    /// なる([`Document::set_row_values`] を変更数だけ繰り返すと対象行の線形探索が毎回
    /// 走り O(行数 × 変更数) になる。10 万行 × 30 列の一括書き換えが要件 3.5 の対象で
    /// ある）。未知のシート([`CellWriteError::UnknownSheet`])・対象シートに属さない行
    /// ([`CellWriteError::UnknownRow`])・範囲外の列([`CellWriteError::UnknownColumn`])
    /// は判別可能な変種として返し、**1 つでも不正ならどのセルも変更しない**
    /// (部分適用なし)。同じ入力の再適用は同じ結果になる(置換であり加算ではない)。
    ///
    /// 書き込みが行の現在の値数より後ろに及ぶ場合は、間を [`CellValue::Null`] で埋める。
    /// **行の集合・並び・識別子は変えない**(置換であって追加ではない)。
    ///
    /// **同じ行・同じ列が 1 回の呼び出しに重複して現れた場合は、入力順の last-wins** で
    /// ある(決定的であり、順序を入れ替えれば結果も変わる)。重複を弾くことはしない。
    /// 下流の `data-grid` の貼り付けは**重複を畳んでから**渡すため、この腕に依存しない
    /// (`crates/data-grid/src/edit/mod.rs` の doc)。この契約は 2026-09-18 の棚卸しで
    /// 明記した(`document-session/tasks.md` の 1.x の Implementation Notes)。
    pub fn set_cells(
        &mut self,
        sheet: SheetId,
        cells: &[(RowId, usize, CellValue)],
    ) -> Result<(), CellWriteError> {
        self.sheets
            .iter_mut()
            .find(|s| s.id() == sheet)
            .ok_or(CellWriteError::UnknownSheet { sheet })?
            .set_cells(cells)
    }

    /// 指定シートから複数の行を**1 回の呼び出しで**取り除き、**取り除いた行を返す**
    /// (data-grid 要件 6.1, 6.2)。
    ///
    /// 取り除いた行を所有権ごと返すのは、行の削除の取り消しが**値と識別子の双方**を要する
    /// ためである([`Row`] は `Clone` を持たず、空の行を挿入する [`Document::insert_row_at`]
    /// では識別子も値も戻せない)。返された行は [`Document::insert_rows_at`] にそのまま
    /// 渡すことで、識別子・値・位置を元へ戻せる(data-grid 要件 6.6 / 9.2)。
    ///
    /// 返る順序は要求引数の並びではなく**シートの順序**である(同じ行集合の要求は、引数の
    /// 並びに依らず常に同じ結果になる)。`rows` に同じ識別子が複数回現れることは誤りでは
    /// なく、1 回の削除に畳まれる。
    ///
    /// 一括で受けるのは、範囲削除が 1 操作であり、行ごとに呼ぶと並びの作り直しが繰り返される
    /// ためである(実装は行数に対する 1 パス。`set_cells` と同じ規律)。未知のシート
    /// ([`RowRemovalError::UnknownSheet`])・対象シートに属さない行
    /// ([`RowRemovalError::UnknownRow`]) は判別可能な変種として返し、**1 つでも不正なら
    /// 1 行も取り除かない**(部分適用なし)。**決定的な出力と往復の契約には触れない**:
    /// 変わるのは行の集合と並びだけである。
    pub fn remove_rows(
        &mut self,
        sheet: SheetId,
        rows: &[RowId],
    ) -> Result<Vec<Row>, RowRemovalError> {
        self.sheets
            .iter_mut()
            .find(|s| s.id() == sheet)
            .ok_or(RowRemovalError::UnknownSheet { sheet })?
            .remove_rows(rows)
    }

    /// 指定シートの位置 `index` に空の行を挿入し、発行した識別子を返す(data-grid 要件 6.1)。
    ///
    /// `index` は**挿入前の行順**に対する添字であり、`index == 行数` は末尾への追加である
    /// ([`Document::add_row`] と同じ位置に入る)。挿入した行は値を持たない(値が与えられて
    /// いない列へのスキーマの既定値の適用は編集経路 = データグリッド側のタスク 3.2 の
    /// 責務であり、本モデルは列の型を知らない)。
    ///
    /// 識別子は [`Document::add_row`] と同じく所有する [`IdFactory`] から発行する: 先に
    /// 発行してから対象シートを選ぶため、シートが存在しない場合、発行済みの識別子は破棄
    /// される(発行者の状態が進むだけで、文書に痕跡は残らない。位置が範囲外の場合も同じ)。
    /// 未知のシート([`RowInsertionError::UnknownSheet`])・行数を超える位置
    /// ([`RowInsertionError::IndexOutOfRange`]) は判別可能な変種として返し、**失敗したときは
    /// 1 行も挿入しない**(部分適用なし)。
    pub fn insert_row_at(
        &mut self,
        sheet: SheetId,
        index: usize,
    ) -> Result<RowId, RowInsertionError> {
        let id = self.ids.new_row_id();
        self.sheets
            .iter_mut()
            .find(|s| s.id() == sheet)
            .ok_or(RowInsertionError::UnknownSheet { sheet })?
            .insert_row_at(index, id)
    }

    /// 指定シートの位置 `index` に、与えられた行を**識別子も値も作り直さずに**差し込む
    /// (data-grid 要件 6.1, 6.6, 9.2)。
    ///
    /// これは [`Document::remove_rows`] の逆を行う入口である: 取り除いた [`Row`] をそのまま
    /// 渡すことで、識別子・値・位置が元に戻る(空の行を挿入する [`Document::insert_row_at`]
    /// では、発行済みの識別子と値を戻せない)。行の複製(data-grid 要件 6.3)がこの口を使う場合は、
    /// 値を作り直す必要があるため複製側の責務となる(本モデルは行の複製 API を持たない)。
    ///
    /// `index` は**挿入前の行順**に対する添字であり、`index == 行数` は末尾への追加である。
    /// 渡された行の順序は保たれる。未知のシート
    /// ([`RowInsertionError::UnknownSheet`])・行数を超える位置
    /// ([`RowInsertionError::IndexOutOfRange`])・**文書内のいずれかのシートに既にある
    /// 識別子**([`RowInsertionError::DuplicateRow`])は判別可能な変種として返し、
    /// **1 つでも不正なら 1 行も差し込まない**(部分適用なし)。
    ///
    /// 一意性の検査は**文書単位**である: 行識別子の一意性は文書内で成立する不変条件であり
    /// (design「Domain Model」)、シート局所の集合では強制できない。したがって全シートの
    /// 行識別子の集合を**1 回だけ**作って渡す(O(文書の行数))。1 操作あたり 1 回の費用で
    /// あり、data-grid 要件 7.7 / 11.5 の予算は操作単位であるため許容される(貼り付けは 1 操作であり、
    /// 行ごとにこの集合を作らない)。この検査が禁じるのは「**文書内に現存する識別子を
    /// 差し込む**」ことであり、取り除かれて文書から消えた識別子を別のシートへ入れることは
    /// 禁じない(入れた時点で文書内に同じ識別子は 1 つしか無いため不変条件に反しない)。
    pub fn insert_rows_at(
        &mut self,
        sheet: SheetId,
        index: usize,
        rows: Vec<Row>,
    ) -> Result<(), RowInsertionError> {
        // 文書内の全シートの行識別子(1 回だけ作る)。対象シートの行も含むため、シート局所の
        // 集合を別に作る必要はない。可変借用を取る前に読む(借用の重複を避ける)。
        let document_rows: HashSet<RowId> = self
            .sheets
            .iter()
            .flat_map(|sheet| sheet.rows())
            .map(Row::id)
            .collect();
        self.sheets
            .iter_mut()
            .find(|s| s.id() == sheet)
            .ok_or(RowInsertionError::UnknownSheet { sheet })?
            .insert_rows_at(index, rows, &document_rows)
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
    use super::{
        Document, ReorderError, Row, RowInsertionError, RowRemovalError, SchemaPart, Sheet,
        UnknownRow, UnknownSheet,
    };
    use crate::ids::{AttachmentId, RowId, SheetId};
    use crate::value::{CellValue, NestedValue};

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
        assert_eq!(
            issued.to_vec(),
            observed,
            "sheets() は追加順を保持しなければならない"
        );
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
        assert_eq!(
            sorted_before, sorted_after,
            "並び替えは識別子を変更しない(同一集合)"
        );
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
            Err(ReorderError::Incomplete {
                missing: vec![rows[0], rows[2]]
            }),
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
        assert_eq!(
            Err(UnknownSheet { sheet: stranger }),
            doc.rename_sheet(stranger, "x")
        );
        assert_eq!(
            Err(ReorderError::UnknownSheet { sheet: stranger }),
            doc.reorder_rows(stranger, &[])
        );
        // `remove_rows` は `Vec<Row>` を返し、`Row` は `PartialEq` を持たないため、
        // `Err(..)` との比較ではなく誤り値そのものを比較する。
        assert_eq!(
            RowRemovalError::UnknownSheet { sheet: stranger },
            doc.remove_rows(stranger, &[])
                .expect_err("未知のシートは失敗する")
        );
        assert_eq!(
            Err(RowInsertionError::UnknownSheet { sheet: stranger }),
            doc.insert_row_at(stranger, 0)
        );
        assert_eq!(
            Err(RowInsertionError::UnknownSheet { sheet: stranger }),
            doc.insert_rows_at(stranger, 0, Vec::new())
        );
        assert!(doc.remove_sheet(stranger).is_none());
        assert!(doc.sheet_by_id(stranger).is_none());
    }

    #[test]
    fn insert_rows_at_rejects_a_row_id_already_in_the_document() {
        // 行識別子の一意性は**文書単位**の不変条件であり、`reorder_rows` がこれに依存して
        // いる。既存の識別子を持つ行は差し込めない。`insert_rows_at` は要求が不正なら
        // 1 行も差し込まない。
        //
        // ここで固定するのは「文書内に現存する識別子」の枝である。**バッチ内の重複**の枝は
        // `tests/row_removal.rs` の `insert_rows_at_rejects_duplicate_identifiers_inside_one_batch`
        // が公開面だけで固定する(`decode` は 1 エントリの中の重複しか拒まないため、2 回の
        // 復号結果を 1 つの要求へまとめれば、文書内に現存しない識別子の重複を作れる)。
        // 同じ `Row::new` の識別子を 2 つ並べただけでは文書内に現存する枝が先に発火して
        // バッチ内の枝を通らないため、ここでは並べない。
        let mut doc = Document::new();
        let sheet = doc.add_sheet("dup");
        let rows: Vec<RowId> = (0..2).map(|_| doc.add_row(sheet).unwrap()).collect();

        assert_eq!(
            Err(RowInsertionError::DuplicateRow { row: rows[1] }),
            doc.insert_rows_at(sheet, 0, vec![Row::new(rows[1])])
        );
        // **他シートに属する識別子**も文書全体の検査で拒む(シート局所の集合では
        // 強制できない。ここが文書単位の検査の要る所以である)。
        let other = doc.add_sheet("他シート");
        assert_eq!(
            Err(RowInsertionError::DuplicateRow { row: rows[0] }),
            doc.insert_rows_at(other, 0, vec![Row::new(rows[0])])
        );
        let observed: Vec<RowId> = doc
            .sheet_by_id(sheet)
            .unwrap()
            .rows()
            .iter()
            .map(Row::id)
            .collect();
        assert_eq!(rows, observed, "失敗後も行集合・並びは無変更(部分適用なし)");
        assert!(
            doc.sheet_by_id(other).unwrap().rows().is_empty(),
            "差し込み先のシートにも 1 行も入らない"
        );
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
    fn sheet_always_has_exactly_one_root_schema() {
        // 要件 1.2 / design ER 図 `Sheet ||--|| SchemaPart : has_root`。
        // `Sheet::root_schema()` は `&SchemaPart` を返す(0 個 = `Option` でも
        // 2 個 = スライスでもない)。新規シートは空のルートスキーマで始まる。
        let mut doc = Document::new();
        let sheet = doc.add_sheet("schema");
        let initial: &SchemaPart = doc.sheet_by_id(sheet).unwrap().root_schema();
        assert_eq!("null", initial.root().as_str());
        assert!(initial.type_defs().is_empty());
        assert!(initial.preserved_fields().is_empty());

        // 差し替えは置換: 2 個目を足すのではなく 1 つを入れ替える。
        let first = SchemaPart::parse(r#"{"root":{"v":1},"types":[]}"#).unwrap();
        doc.set_root_schema(sheet, first).unwrap();
        assert_eq!(
            r#"{"v":1}"#,
            doc.sheet_by_id(sheet)
                .unwrap()
                .root_schema()
                .root()
                .as_str()
        );
        let second = SchemaPart::parse(r#"{"root":true}"#).unwrap();
        doc.set_root_schema(sheet, second).unwrap();
        assert_eq!(
            "true",
            doc.sheet_by_id(sheet)
                .unwrap()
                .root_schema()
                .root()
                .as_str(),
            "2 回目の差し替えでもルートはちょうど 1 つ(置換)"
        );

        // 存在しないシートへの設定は panic せずエラーで、既存のルートは無変更。
        let stranger = {
            let mut scratch = Document::new();
            scratch.add_sheet("stranger")
        };
        match doc.set_root_schema(stranger, SchemaPart::empty()) {
            Err(UnknownSheet { sheet: reported }) => assert_eq!(stranger, reported),
            Ok(()) => panic!("未知シートへの設定は失敗しなければならない"),
        }
        assert_eq!(
            "true",
            doc.sheet_by_id(sheet)
                .unwrap()
                .root_schema()
                .root()
                .as_str()
        );
    }

    #[test]
    fn sheet_root_schema_keeps_type_ids_and_refs_only() {
        // 要件 1.2 / 1.3: シートへ結線した後も、本クレートが扱うのは型定義の識別子と
        // 参照構造だけで、定義の意味論は解釈しない(不透明ペイロードはバイト一致)。
        let text = concat!(
            r#"{"root":{"a":{"$ref":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}},"#,
            r#""types":[{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","definition":{"足し算":{"x":1},"note":"{ }"}}]}"#,
        );
        let mut doc = Document::new();
        let sheet = doc.add_sheet("schema");
        doc.set_root_schema(sheet, SchemaPart::parse(text).unwrap())
            .unwrap();

        let stored = doc.sheet_by_id(sheet).unwrap().root_schema();
        // 解釈するのは識別子(ULID へ解決)と参照ターゲット(生テキスト)のみ。
        assert_eq!(1, stored.type_defs().len());
        assert_eq!(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            stored.type_defs()[0].id().to_string()
        );
        let targets: Vec<&str> = stored
            .type_ref_targets()
            .iter()
            .map(String::as_str)
            .collect();
        assert_eq!(targets, ["01ARZ3NDEKTSV4RRFFQ69G5FAV"]);
        // 型の意味論に見えるキー(未知の型演算子)も解釈せず、バイトのまま保持する。
        assert_eq!(
            r#"{"足し算":{"x":1},"note":"{ }"}"#,
            stored.type_defs()[0].definition().as_str()
        );
    }

    #[test]
    fn holds_100_000_rows_across_ten_sheets() {
        // 要件 8.4: 10 万行を保持できることの保証(全件オンメモリ)。
        const SHEETS: usize = 10;
        const ROWS_PER_SHEET: usize = 10_000;

        let mut doc = Document::new();
        let sheet_ids: Vec<SheetId> = (0..SHEETS)
            .map(|i| doc.add_sheet(format!("sheet-{i}")))
            .collect();
        let mut issued: Vec<Vec<RowId>> = Vec::with_capacity(SHEETS);
        for &sheet in &sheet_ids {
            let rows: Vec<RowId> = (0..ROWS_PER_SHEET)
                .map(|_| doc.add_row(sheet).unwrap())
                .collect();
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

    #[test]
    fn attachment_registry_is_reachable_through_the_document() {
        // 要件 7.1 / 7.5: Document 経由で任意のバイト列を保持し、そのまま取り出せる。
        let mut doc = Document::new();
        assert!(doc.attachments().is_empty());
        assert!(doc.unreferenced_attachments().is_empty());

        let payload: Vec<u8> = vec![0xff, 0x00, 0xfe, 0x80];
        let id = doc.add_attachment(payload.clone());
        let stored = doc.attachment(id).expect("登録直後の添付は取得できる");
        assert_eq!(payload, stored.bytes(), "1 バイトも変わらない");
        assert_eq!(
            AttachmentId::from_bytes(&payload),
            id,
            "識別子は内容の BLAKE3"
        );
        assert_eq!(id, stored.id());
        assert_eq!(1, doc.attachments().len());

        // 同一バイト列の再登録は冪等(エントリは増えない)。
        assert_eq!(id, doc.add_attachment(payload.clone()));
        assert_eq!(1, doc.attachments().len());

        // 未登録の識別子は `None`(panic しない)。
        assert!(doc
            .attachment(AttachmentId::from_bytes(b"unknown"))
            .is_none());

        // どの行からも参照されていないため未参照として一覧できる(削除はしない)。
        assert_eq!(vec![id], doc.unreferenced_attachments());
        assert_eq!(payload, doc.attachment(id).unwrap().bytes());
    }

    #[test]
    fn set_row_values_replaces_the_row_values() {
        // parts 復路(タスク 4.5)が行 1 件分の値列を復元するための入口。
        let mut doc = Document::new();
        let sheet = doc.add_sheet("rows");
        let row = doc.add_row(sheet).unwrap();
        assert!(doc.sheet_by_id(sheet).unwrap().rows()[0]
            .values()
            .is_empty());

        let first = vec![CellValue::Int(1), CellValue::Text("x".to_string())];
        doc.set_row_values(sheet, row, first.clone()).unwrap();
        let observed: Vec<CellValue> = doc.sheet_by_id(sheet).unwrap().rows()[0].values().to_vec();
        assert_eq!(first, observed);

        // 追加ではなく置換: 2 回目の設定で値列は置き換わり、行は増えず識別子も不変。
        doc.set_row_values(sheet, row, vec![CellValue::Null])
            .unwrap();
        let sheet_ref = doc.sheet_by_id(sheet).unwrap();
        assert_eq!(1, sheet_ref.rows().len(), "行は増えない");
        assert_eq!(row, sheet_ref.rows()[0].id(), "行識別子は設定で変わらない");
        assert_eq!(vec![CellValue::Null], sheet_ref.rows()[0].values().to_vec());
    }

    #[test]
    fn set_row_values_reports_unknown_targets_without_mutation() {
        let mut doc = Document::new();
        let sheet = doc.add_sheet("rows");
        let row = doc.add_row(sheet).unwrap();
        let other_sheet = doc.add_sheet("other");
        let stranger_row = doc.add_row(other_sheet).unwrap();
        let stranger_sheet = {
            let mut scratch = Document::new();
            scratch.add_sheet("ghost")
        };
        let attachment = doc.add_attachment(b"kept".to_vec());

        // 未知の行(このシートに属さない行)は `UnknownRow` で報告する。
        assert_eq!(
            Err(UnknownRow { row: stranger_row }),
            doc.set_row_values(sheet, stranger_row, vec![CellValue::Attachment(attachment)])
        );
        // 未知のシートも同じ失敗である: 存在しないシートに行は無いため、診断は
        // 「その行が無い」に一本化する(シート専用のエラー型は増やさない)。
        assert_eq!(
            Err(UnknownRow { row }),
            doc.set_row_values(stranger_sheet, row, vec![CellValue::Attachment(attachment)])
        );

        // どちらの失敗でも既存の行は無変更で、添付も削除されない。
        assert!(doc.sheet_by_id(sheet).unwrap().rows()[0]
            .values()
            .is_empty());
        assert_eq!(
            b"kept".to_vec(),
            doc.attachment(attachment).unwrap().bytes()
        );
    }

    #[test]
    fn references_are_aggregated_across_sheets_and_rows_deterministically() {
        // 要件 7.6: 全シート・全行のセル値を集計し、未参照だけを昇順で返す。
        let mut doc = Document::new();
        let first = doc.add_sheet("first");
        let second = doc.add_sheet("second");
        let row_a = doc.add_row(first).unwrap();
        let row_b = doc.add_row(first).unwrap();
        let row_c = doc.add_row(second).unwrap();
        let row_d = doc.add_row(second).unwrap();

        let shared = doc.add_attachment(b"shared".to_vec());
        let only_b = doc.add_attachment(b"only-b".to_vec());
        let only_c = doc.add_attachment(b"only-c".to_vec());
        let orphan = doc.add_attachment(b"orphan".to_vec());

        // 参照なしの時点では 4 件すべてが未参照(昇順)。
        let mut all = vec![shared, only_b, only_c, orphan];
        all.sort();
        assert_eq!(all, doc.unreferenced_attachments());

        doc.set_row_values(first, row_a, vec![CellValue::Attachment(shared)])
            .unwrap();
        doc.set_row_values(
            first,
            row_b,
            vec![CellValue::Nested(NestedValue::Array(vec![
                CellValue::Attachment(only_b),
            ]))],
        )
        .unwrap();
        doc.set_row_values(second, row_c, vec![CellValue::Attachment(shared)])
            .unwrap();
        // `only_c` は 2 枚目のシートからのみ参照される: 全シートを走査しなければ
        // 未参照に見えてしまう添付であり、この集計の回帰検出点である。
        doc.set_row_values(second, row_d, vec![CellValue::Attachment(only_c)])
            .unwrap();

        // 別シートからの参照も参照済みとして集計される: 1 枚目からのみ参照される
        // `only_b` と 2 枚目からのみ参照される `only_c` はいずれも一覧に現れない。
        let mut expected_after = vec![orphan];
        expected_after.sort();
        assert_eq!(expected_after, doc.unreferenced_attachments());

        // 行順を入れ替えても、シート名を変えても結果は同一(順序は識別子で決まる)。
        let reversed: Vec<RowId> = doc
            .sheet_by_id(first)
            .unwrap()
            .rows()
            .iter()
            .map(Row::id)
            .rev()
            .collect();
        doc.reorder_rows(first, &reversed).unwrap();
        doc.rename_sheet(first, "renamed").unwrap();
        assert_eq!(expected_after, doc.unreferenced_attachments());

        // `second` を除去すると `only_c` の参照が消えて未参照に戻るが、エントリは削除されない。
        // `shared` は 1 枚目から参照され続けるため未参照にはならない。
        doc.remove_sheet(second).unwrap();
        let mut expected_removed = vec![only_c, orphan];
        expected_removed.sort();
        assert_eq!(expected_removed, doc.unreferenced_attachments());
        assert_eq!(b"only-c".to_vec(), doc.attachment(only_c).unwrap().bytes());
        assert_eq!(b"shared".to_vec(), doc.attachment(shared).unwrap().bytes());
    }
}

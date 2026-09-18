//! ホストの縫い目（`HostPort`）を実文書へ繋ぐアダプタ（tasks.md 4.1）。
//!
//! マクロ 1 回の実行について [`HostPort`]（`crates/macro-runtime/src/host/mod.rs` が定義）を
//! 実装する。エンジンは文書を知らない — **読みはセッション越しに文書とスキーマから供給し、
//! 書きは未適用の変更集合として集めるだけ**である（適用は 4.2 の `macro_apply.rs`。
//! design.md 決定 2 / 3）。
//!
//! # 何を持つか（実行 1 回ぶんの実体）
//!
//! [`DocumentHost`] は**実行 1 回につき 1 つ**作り、実行が終われば捨てる（変更集合は実行
//! 1 回のトランザクション境界である。`crates/macro-runtime/src/host/changes.rs` の doc）。
//! 持つのは 3 つだけである:
//!
//! | 何 | どこから | 何に使うか |
//! |---|---|---|
//! | セッションの表 | `document-session` の `Arc<DocumentSessions>`（適応層が 1 実体を共有する） | 文書の読み（`read`）と出所（`state`） |
//! | 対象のウィンドウ | `app-shell` の `WindowLabel` | どの文書に対して実行するか |
//! | 変更集合 | 本型の `Mutex<ChangeSet>` | `stage` の集約（`HostPort::stage` は `&self` であるため内部可変性に置く） |
//!
//! `HostPort` は `Send + Sync` を要する（実行は専用スレッドへ渡る。`host/mod.rs` の doc）ため、
//! 変更集合は `Mutex` の内側にある。実行中に触るのはエンジンの専用スレッド 1 つだけであり、
//! 排他は「同じ実行の呼び出しが直列である」ことの保証として効く。
//!
//! # 要件との対応
//!
//! | 要件 | どこで満たすか |
//! |---|---|
//! | 4.1（シートの一覧・列の宣言・行数がマクロから読める） | [`HostPort::sheets`] / [`HostPort::columns`]。行数は**文書の行数**であり、重ね合わせを見ない |
//! | 4.2（値が宣言された型に対応する JS の値で渡る） | [`HostPort::columns`] が宣言の型を渡し、値の写像はエンジンが `host/value.rs` の `to_js` で行う（**本モジュールは写像を写経しない**） |
//! | 4.3（入れ子と ANY は構造として渡る） | 同上（自己記述的な列は値が構造のまま渡る） |
//! | 4.4（範囲の読みは 1 回の呼び出し） | [`HostPort::read_rows`] が範囲の全部を 1 回で返し、自分の書き込みを `Overlay` で重ねる |
//! | 8.1（宣言の無いファイル・ネットワークの利用を許さない） | 門（`crates/macro-runtime/src/surface/gate.rs`）が**呼び出しの前に**拒む。本モジュールへ届くのは宣言のある呼び出しだけである（下の「能力を要する口」） |
//! | 8.3（拒んだ能力の名前を提示する） | 門が能力の名前を理由に含める（エンジン側）。本モジュールが自分で拒む場合も**何を拒んだか**を理由に書く |
//!
//! # 読み: 文書とスキーマから供給する（4.1–4.4）
//!
//! - **シートの一覧**（[`HostPort::sheets`]）: `Document::sheets()` の順に、識別子・
//!   **文書の表示名**（`Sheet::name()`。識別子を名前の代わりに出さない）・行数
//!   （`Sheet::rows().len()`）を返す
//! - **列の宣言**（[`HostPort::columns`]）: シートのルートスキーマを `schema-engine` が解析し、
//!   `$ref` の連鎖を `Resolver` が辿った先の種別を返す。**型の判定をここで作り直さない**
//!   （`schema-engine` の宣言層と解決層が唯一の源である）
//! - **範囲の読み**（[`HostPort::read_rows`]）: `RowSpan::resolve` で位置を行数へ当て、
//!   文書の行をそのまま写してから `Overlay::read_range` を呼ぶ。**重ね合わせの規則は
//!   `host/overlay.rs` が持つ**（本モジュールは呼ぶだけであり、規則を再実装しない）。位置は
//!   文書の行の並び（表示順）であり、行の識別子の順とは一致しない
//!
//! 値の写像（`CellValue` → JS の値）は本モジュールの仕事ではない。エンジンの op が
//! `host/value.rs` の `to_js` を使い、写像の仕方は**列の型が決める** — したがって本モジュールが
//! 渡す `ColumnTypeInfo::kind` の正しさが、そのまま値の形の正しさである。
//!
//! # 書き: 集めるだけ（適用しない）
//!
//! [`HostPort::stage`] は文書を引ける側として**先に文書を見て拒む**（要件 5.4 の分界。
//! `host/changes.rs` の doc「文書に無い行と範囲外の列は、ホストの縫い目（タスク 4.1）が
//! 文書を見て拒む」）:
//!
//! - 対象のシートが文書に無い
//! - 書き込み・削除・複製の対象の行が文書に無い
//! - 書き込みの列が範囲外（列数は `Sheet::columns()` の長さ）
//! - 追加する行の値の数が列の数と一致しない（行の値は列の添字で並ぶため）
//!
//! 通った変更は `ChangeSet::stage` へ渡す（**この実行が既に削除した行**を指した場合の拒否は
//! `ChangeSet` が持つ — 削除は適用の最後に効くが、その行は適用時に存在しない）。**適用は
//! 一切しない** — 集めた変更集合を文書へ書くのは 4.2 である。
//!
//! 文書の読みは 1 回の呼び出しにつき 1 回である（費用は「文書の行数 + その呼び出しの件数」に
//! 比例する。行の実在は集合で引くため、件数の積にならない）。実行を跨いで行の集合を持ち越さ
//! ないのは、**実行中にも利用者が表を編集できる**ためである（要件 2.2 の「表を止めない」の
//! 裏面。古い集合で実在を判定すると、消えた行への書き込みを許してしまう）。
//!
//! # 能力を要する口（8.1, 8.3）
//!
//! 門が呼び出しの前に拒むため、**ここへ届くのは宣言のある呼び出しだけ**である。それでも
//! 実装側で「どこまで許すか」を決める（門は「その能力を宣言したか」しか見ない）:
//!
//! | 口 | 解決 | 許すもの | 上限 |
//! |---|---|---|---|
//! | `file.read` | 相対パスは**文書の位置の親**を基準に解決する。絶対パスはそのまま | 存在するファイル。ディレクトリと空のパスは拒む | 16 MiB。UTF-8 のテキストに限る（返す型が `String` である） |
//! | `file.write` | 同上 | 親ディレクトリが存在する位置。**既存の内容は置き換える** | 16 MiB（書き出す前に拒む） |
//! | `net.fetch` | URL | `http` と `https`。2xx 以外は失敗として拒む | 4 MiB。30 秒で打ち切る |
//!
//! **絶対パスを禁じない理由**: 要件 8.1 の境界は**宣言**であり（利用者は実行の前に宣言された
//! 能力を見せられる。要件 8.2）、ここで第 2 の境界を足すと利用者の同意なしに要件を狭める。
//! 本モジュールが足すのは「相対パスの基準を文書の位置に固定する」ことと、事故を止める運用上の
//! 上限（大きさ・テキスト・時間）だけである。基準が無い文書（未保存）では相対パスを拒む —
//! 基準を勝手に作業ディレクトリへ倒さない。
//!
//! ネットワークの 30 秒は実行の既定の時間の上限（要件 6.1）と同じ値である。**縫い目は同期の
//! 腕であり、V8 の打ち切りはここへ届かない**ため、取得そのものに上限を置かないと遅い相手に
//! 対して実行の上限が効かない。
//!
//! 拒否の理由には**何を拒んだか**（パス・URL・上限の値）を書く（要件 8.3 の精神。理由は実行の
//! 失敗の面に出る。ソースと値は記録へ出さない — 要件 2.6 の記録の規律）。
//!
//! # 何をしないか
//!
//! - **適用しない**: 変更集合を文書へ書くのは 4.2（`macro_apply.rs`）
//! - **`console` の出力を運ばない**: エンジンが実行 1 回ぶんの出力を集め、`RunOutcome::Ran` で
//!   ホストへ戻す（`host/mod.rs` が `emit` を縫い目に置かなかった理由）
//! - **値の写像をしない**: `host/value.rs` が唯一の源である
//! - **診断の記録を書かない**: 実行 1 回の記録は 4.3（`macro_run` の 1 行）
//! - **列の宣言を書き換えない**: 宣言表にその API が無く、門が名前つきで拒む（要件 4.5）

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use app_shell::ipc::WindowLabel;
use document_format::SheetId;
use document_session::{DocumentSessions, DocumentSessionsApi, Origin, SessionError, SessionState};
use macro_runtime::host::changes::{Change, ChangeSet};
use macro_runtime::host::overlay::{ColumnTypeInfo, Overlay, ReadRow, RowPage, RowSpan, SheetInfo};
use macro_runtime::host::{HostError, HostPort};
use schema_engine::compile::resolve::{resolve, Resolver};
use schema_engine::{
    parse_schema, parse_type_definition, DeclaredKind, SchemaError, TypeDecl, TypeDefinition,
    TypeKind,
};

/// ファイルの読みの上限（16 MiB）。
///
/// マクロが文書の隣の CSV やログを読む用途は収まり、**文書そのものより大きな塊を 1 つの
/// JS の文字列として isolate へ持ち込まない**大きさである（実行の既定のメモリ上限 512 MB に
/// 対して十分小さい）。上限を越える要求は読む前に理由つきで拒む。
const MAX_FILE_READ_BYTES: u64 = 16 * 1024 * 1024;

/// ファイルの書きの上限（16 MiB）。上限を越える本文は**書く前に**拒む。
const MAX_FILE_WRITE_BYTES: usize = 16 * 1024 * 1024;

/// ネットワークの取得の上限（4 MiB）。上限を越える本文は読まないまま拒む。
const MAX_NET_BYTES: usize = 4 * 1024 * 1024;

/// ネットワークの 1 回の取得の時間の上限（30 秒。実行の既定の時間の上限と同じ値）。
const NET_TIMEOUT: Duration = Duration::from_secs(30);

/// ホストの縫い目を文書へ繋ぐアダプタ（**実行 1 回ぶん**の実体）。
///
/// 作り方は [`DocumentHost::new`]、使うのはエンジン（`MacroActor::run` が
/// `Arc<dyn HostPort>` として受け取る）。1 つの実行の間だけ生き、実行が終われば変更集合ごと
/// 捨てる（**適用する側は 4.2** であり、本型は文書を書かない）。
pub struct DocumentHost {
    /// セッションの表（適応層が 1 実体を `Arc` で共有する。`document-session` の公開面）。
    documents: Arc<DocumentSessions>,
    /// 対象のウィンドウ（このウィンドウの文書に対して実行する）。
    window: WindowLabel,
    /// この実行で集めた変更（**適用しない**。実行が終われば捨てる）。
    changes: Mutex<ChangeSet>,
}

impl DocumentHost {
    /// ウィンドウの文書に対して実行する 1 回ぶんの縫い目を作る。
    ///
    /// 文書を読むのは各口の呼び出しの時点である（作った時点では読まない — 実行の途中で
    /// 利用者が表を編集しても、読みはそのときの文書を返す）。
    pub fn new(documents: Arc<DocumentSessions>, window: WindowLabel) -> Self {
        Self {
            documents,
            window,
            changes: Mutex::new(ChangeSet::new()),
        }
    }

    /// この実行で集めた変更集合を借りて読ませる（**複製しない**。要件 5.5 / 11.3）。
    ///
    /// 適用する側（4.2）と、実行の結果を組み立てる側（4.3）が使う。`HostPort::with_changes`
    /// と同じ規律であり、`Arc<dyn HostPort>` 越しでなくても呼べるように具象側にも置く。
    pub fn with_changes(&self, read: &mut dyn FnMut(&ChangeSet)) {
        // 毒された場合も読む（変更集合は他のスレッドの panic で壊れるデータを持たない。
        // 実行の失敗を「読めない」に化けさせない）。
        let changes = self.changes.lock().unwrap_or_else(PoisonError::into_inner);
        read(&changes);
    }

    /// 文書に無いシート・行・範囲外の列を理由つきで拒む（要件 5.4）。
    ///
    /// 文書を引けるのはセッションの `read` の閉包の内側だけであり、閉包の引数の型
    /// （`document_format::Document`）は本クレートの通常依存ではない（テストだけが使う）ため、
    /// **型を名指す補助関数を置かず**、判定をこの 1 つの `match` に閉じる。
    fn refuse_missing(&self, change: &Change) -> Result<(), HostError> {
        let read = self.documents.read(&self.window, &mut |document| {
            match change {
                Change::SetCells { sheet, writes } => {
                    if writes.is_empty() {
                        // 空の呼び出しは `ChangeSet` が記録しない（規則 5）。文書も読まない。
                        return Ok(());
                    }
                    let Some(found) = document.sheet_by_id(*sheet) else {
                        return Err(HostError::new(format!("シート {sheet} はこの文書に無い")));
                    };
                    let column_count = found.columns().len();
                    // 行の実在は集合で引く（1 件ずつ文書を走査して件数の積にしない）。
                    let present: HashSet<_> = found.rows().iter().map(|row| row.id()).collect();
                    if let Some(write) = writes.iter().find(|write| !present.contains(&write.row)) {
                        return Err(HostError::new(format!(
                            "行 {} はシート {sheet} に無い",
                            write.row
                        )));
                    }
                    if let Some(write) = writes
                        .iter()
                        .find(|write| write.column.index() >= column_count)
                    {
                        return Err(HostError::new(format!(
                            "列 {} はシート {sheet} の範囲外である（列数 {column_count}）",
                            write.column.index()
                        )));
                    }
                }
                Change::InsertRows { sheet, values } => {
                    if values.is_empty() {
                        return Ok(());
                    }
                    let Some(found) = document.sheet_by_id(*sheet) else {
                        return Err(HostError::new(format!("シート {sheet} はこの文書に無い")));
                    };
                    let column_count = found.columns().len();
                    if let Some((index, _)) = values
                        .iter()
                        .enumerate()
                        .find(|(_, values)| values.len() != column_count)
                    {
                        return Err(HostError::new(format!(
                            "{index} 番目に追加する行の値の数が列の数と一致しない（シート {sheet} の列数は {column_count}）"
                        )));
                    }
                }
                Change::RemoveRows { sheet, rows } | Change::DuplicateRows { sheet, rows } => {
                    if rows.is_empty() {
                        return Ok(());
                    }
                    let Some(found) = document.sheet_by_id(*sheet) else {
                        return Err(HostError::new(format!("シート {sheet} はこの文書に無い")));
                    };
                    let present: HashSet<_> = found.rows().iter().map(|row| row.id()).collect();
                    if let Some(row) = rows.iter().find(|row| !present.contains(*row)) {
                        return Err(HostError::new(format!("行 {row} はシート {sheet} に無い")));
                    }
                }
            }
            Ok(())
        });
        // `Ok(Err(..))` は文書を見た拒否であり、`Err(..)` はセッションの拒否（文書が無い・
        // 読み込めなかった）である。
        read.map_err(|error| session_reason("シートを引けなかった", &error))?
    }

    /// 相対パスを**文書の位置**を基準に解決する（能力を要する 3 つの口の共通の前口）。
    ///
    /// 絶対パスはそのまま返す（モジュール doc「能力を要する口」の理由）。基準が無い文書
    /// （未保存）では相対パスを理由つきで拒む。
    fn resolve_relative(&self, path: &str, capability: &str) -> Result<PathBuf, HostError> {
        if path.is_empty() {
            return Err(HostError::new(format!("{capability} に空のパスは渡せない")));
        }
        let given = Path::new(path);
        if given.is_absolute() {
            return Ok(given.to_path_buf());
        }
        match self.documents.state(&self.window) {
            SessionState::Open {
                origin: Origin::File(document),
                ..
            } => {
                // 保存済みの文書の位置の親（ルート直下のファイル名でも親は "/" になる）。
                let base = document
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from(std::path::MAIN_SEPARATOR_STR));
                Ok(base.join(given))
            }
            _ => Err(HostError::new(format!(
                "{capability} の相対パス {path} を解決できない（この文書はまだ保存されていない）"
            ))),
        }
    }
}

impl HostPort for DocumentHost {
    /// シートの一覧（要件 4.1）。行数は**文書の行数**であり、重ね合わせを見ない。
    fn sheets(&self) -> Result<Vec<SheetInfo>, HostError> {
        let read = self.documents.read(&self.window, &mut |document| {
            Ok(document
                .sheets()
                .iter()
                .map(|sheet| SheetInfo {
                    id: sheet.id(),
                    // **文書の表示名**を使う（識別子を名前の代わりに出さない）。
                    name: sheet.name().to_owned(),
                    row_count: sheet.rows().len(),
                })
                .collect())
        });
        read.map_err(|error| session_reason("シートの一覧を読めなかった", &error))?
    }

    /// 列の宣言（要件 4.1, 4.2）。宣言の型を `schema-engine` の解決を通して返す。
    ///
    /// 上流が不透明なペイロードとして持つルートスキーマと型定義を解析層へ渡し、`$ref` の連鎖を
    /// `Resolver` が辿った先の種別を返す。**判定の規則をここに写さない**。
    fn columns(&self, sheet: SheetId) -> Result<Vec<ColumnTypeInfo>, HostError> {
        let read = self.documents.read(&self.window, &mut |document| {
            let Some(found) = document.sheet_by_id(sheet) else {
                return Err(HostError::new(format!("シート {sheet} はこの文書に無い")));
            };
            let declaration = found.root_schema();
            let schema = parse_schema(declaration.root().as_str()).map_err(|error| {
                HostError::new(format!("シート {sheet} の宣言を解釈できない: {error}"))
            })?;
            let definitions = declaration
                .type_defs()
                .iter()
                .map(|definition| {
                    Ok(TypeDefinition {
                        id: definition.id(),
                        definition: parse_type_definition(definition.definition().as_str())?,
                    })
                })
                .collect::<Result<Vec<TypeDefinition>, SchemaError>>()
                .map_err(|error| {
                    HostError::new(format!("シート {sheet} の型定義を解釈できない: {error}"))
                })?;
            let resolver = resolve(&schema, &definitions).map_err(|error| {
                HostError::new(format!("シート {sheet} の型の参照を解決できない: {error}"))
            })?;
            Ok(schema
                .columns
                .iter()
                .map(|column| ColumnTypeInfo {
                    name: column.name.to_string(),
                    kind: declared_kind(&resolver, &column.ty),
                    required: column.required,
                    unique: column.unique,
                })
                .collect())
        });
        read.map_err(|error| session_reason("列の宣言を読めなかった", &error))?
    }

    /// 行の範囲の読み（要件 4.4）。**1 回の呼び出しで範囲の全部を返す。**
    ///
    /// 読みは**自分の書き込みの重ね合わせ**を見る（要件 5.1 の裏面）。規則は
    /// `host/overlay.rs` が持つ — ここは文書から読んだ行を渡して [`Overlay`] を呼ぶ。
    fn read_rows(&self, sheet: SheetId, span: RowSpan) -> Result<RowPage, HostError> {
        let read = self.documents.read(&self.window, &mut |document| {
            let Some(found) = document.sheet_by_id(sheet) else {
                return Err(HostError::new(format!("シート {sheet} はこの文書に無い")));
            };
            let rows = found.rows();
            // 要求した範囲を文書の位置で切る（両端を含む）。逆転した範囲と、行が 1 つも
            // 無い文書では空の頁を返す。
            let Some(range) = span.resolve(rows.len()) else {
                return Ok(Vec::new());
            };
            Ok(rows[range]
                .iter()
                .map(|row| ReadRow {
                    id: row.id(),
                    // セルは列の並びのまま（`Sheet::columns()` と同じ順であることを、形式が
                    // 読み込みの時に検査している）。
                    cells: row.values().to_vec(),
                })
                .collect())
        });
        let base = read.map_err(|error| session_reason("行を読めなかった", &error))?;
        let base = base?;
        // 重ね合わせは文書の読みの後で作る（ロックを持つ時間を短くする）。
        let changes = self.changes.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(Overlay::new(&changes).read_range(sheet, base))
    }

    /// 1 件の変更を集める（**適用しない**）。文書を見て拒む（要件 5.4）。
    fn stage(&self, change: Change) -> Result<(), HostError> {
        self.refuse_missing(&change)?;
        self.changes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stage(change)
            // この実行が既に削除した行を指した（`ChangeSet` の拒否。要件 5.4）。
            .map_err(|error| HostError::new(error.to_string()))
    }

    /// この実行で集めた変更を**借用のまま**読ませる（要件 5.5 の件数。複製しない）。
    fn with_changes(&self, read: &mut dyn FnMut(&ChangeSet)) {
        DocumentHost::with_changes(self, read);
    }

    /// ファイルを読む（要件 8.1, 8.3）。門を通った呼び出しだけが届く。
    fn file_read(&self, path: &str) -> Result<String, HostError> {
        let resolved = self.resolve_relative(path, "file.read")?;
        let metadata = std::fs::metadata(&resolved)
            .map_err(|error| HostError::new(format!("{path} を読めない: {error}")))?;
        if metadata.is_dir() {
            return Err(HostError::new(format!(
                "{path} はディレクトリである（file.read はテキストのファイルを読む）"
            )));
        }
        if metadata.len() > MAX_FILE_READ_BYTES {
            return Err(HostError::new(format!(
                "{path} は大き過ぎる（{} バイト。file.read の上限は {MAX_FILE_READ_BYTES} バイト）",
                metadata.len()
            )));
        }
        let bytes = std::fs::read(&resolved)
            .map_err(|error| HostError::new(format!("{path} を読めない: {error}")))?;
        String::from_utf8(bytes).map_err(|_| {
            HostError::new(format!(
                "{path} は UTF-8 のテキストではない（file.read はテキストを返す）"
            ))
        })
    }

    /// ファイルへ書く（要件 8.1, 8.3。門は `file.read` と同じ）。**既存の内容は置き換える。**
    fn file_write(&self, path: &str, text: &str) -> Result<(), HostError> {
        if text.len() > MAX_FILE_WRITE_BYTES {
            return Err(HostError::new(format!(
                "{path} へ書く本文が大き過ぎる（{} バイト。file.write の上限は {MAX_FILE_WRITE_BYTES} バイト）",
                text.len()
            )));
        }
        let resolved = self.resolve_relative(path, "file.write")?;
        std::fs::write(&resolved, text)
            .map_err(|error| HostError::new(format!("{path} へ書けない: {error}")))
    }

    /// URL を取る（要件 8.1, 8.3。門は `file.read` と同じ）。
    ///
    /// HTTP クライアントは `reqwest` をアダプタ側で使う（design.md「Open Questions / Risks」
    /// 3。エンジンは門だけを持つ）。縫い目は同期であるため `blocking` を使うが、**取得は
    /// 専用のスレッドへ出す** — `reqwest::blocking` の実体は専用のスレッドと tokio の
    /// ランタイムを作って落とすものであり、この口は**エンジンの実行文脈（tokio の
    /// `block_on`）の中**から呼ばれる。実行文脈の中でランタイムを落とすと tokio が panic し、
    /// V8 の op は巻き戻せない（`extern "C"`）ためプロセスが abort する（実測: 最初の実装で
    /// マクロの `host.netFetch` がプロセスを落とした）。作って落とす場所を文脈の外へ出せば、
    /// 取得が失敗しても**アプリを巻き込まない**（要件 6.4 の精神）。
    fn net_fetch(&self, url: &str) -> Result<String, HostError> {
        install_crypto_provider();
        let parsed = reqwest::Url::parse(url)
            .map_err(|error| HostError::new(format!("{url} は URL として読めない: {error}")))?;
        match parsed.scheme() {
            "http" | "https" => {}
            other => {
                return Err(HostError::new(format!(
                    "net.fetch は http と https だけを取れる（{other} は取れない）: {url}"
                )));
            }
        }
        let target = url.to_owned();
        let fetched = std::thread::Builder::new()
            .name("macro-net-fetch".to_owned())
            .spawn(move || fetch_text(parsed, &target))
            .map_err(|error| HostError::new(format!("{url} の取得を始められない: {error}")))?
            .join();
        match fetched {
            Ok(result) => result,
            // 取得のスレッドが panic した。理由つきの失敗にして実行へ返す（プロセスを
            // 落とさない）。
            Err(_) => Err(HostError::new(format!(
                "{url} の取得が内部で失敗した（実行の外で panic した）"
            ))),
        }
    }
}

/// セッションの拒否を理由つきの拒否へ写す（文書が無い・読み込めなかった）。
fn session_reason(what: &str, error: &SessionError) -> HostError {
    HostError::new(format!("{what}: {error}"))
}

/// TLS の暗号器（`rustls::crypto::CryptoProvider`）をプロセスに 1 度だけ据える。
///
/// `reqwest` の `rustls-no-provider` feature は**暗号器を同梱しない**（既定は aws-lc-rs で
/// あり、CMake と nasm を要するため切ってある）。`rustls` は据え付けが済むまで HTTPS の
/// 接続を組めないため、`net.fetch` の入口で 1 度だけ据える。**既に据えられている場合**
/// （別の経路が先に据えた）は何もしない — 据え付けの失敗は「既にある」ことだけを意味する。
fn install_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// 1 つの URL を取る（[`HostPort::net_fetch`] の本体。**専用のスレッドの上で走る**）。
///
/// ここで HTTP クライアントを作り、使って、落とす（作って落とす場所を実行文脈の外へ出すのが
/// 呼び出し側の役目である）。`url` は理由の文言に使う綴りであり、`parsed` が実際に取る先で
/// ある（呼び出し側で同じ検査を通っている）。
fn fetch_text(parsed: reqwest::Url, url: &str) -> Result<String, HostError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(NET_TIMEOUT)
        .build()
        .map_err(|error| HostError::new(format!("{url} の取得を用意できない: {error}")))?;
    let response = client
        .get(parsed)
        .send()
        .map_err(|error| HostError::new(format!("{url} を取得できない: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(HostError::new(format!(
            "{url} の取得は {status} で終わった（net.fetch は 2xx だけを受け取る）"
        )));
    }
    // 上限を越える本文は読まない（1 バイトだけ余分に読んで超過を知る）。
    let mut body = Vec::new();
    response
        .take(MAX_NET_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| HostError::new(format!("{url} の本文を読めない: {error}")))?;
    if body.len() > MAX_NET_BYTES {
        return Err(HostError::new(format!(
            "{url} の本文が大き過ぎる（net.fetch の上限は {MAX_NET_BYTES} バイト）"
        )));
    }
    String::from_utf8(body)
        .map_err(|_| HostError::new(format!("{url} の本文は UTF-8 のテキストではない")))
}

/// 宣言された型を種別へ落とす（`$ref` の連鎖は `Resolver` が辿る）。
///
/// **解決できない型は [`TypeKind::Any`] として渡す。** 宣言の種別トークンが未来の版のもので
/// ある場合（要件 11.7 が「その列だけを使用不能にして残りを読む」と定める状態）、
/// `ColumnTypeInfo` には生のトークンを載せる欄が無く、`TypeKind` にも「不明」が無い。
/// `Any` は schema-engine のカタログで「保持可能なすべての変種を受け入れ、型強制もしない」
/// 種別であり、値の写像（`host/value.rs`）では自己記述の形になる — **どの変種も失われない**
/// ため、読みの側で嘘をつかない唯一の選択である（値の正否の判定は適用のときに
/// `schema-engine` が行う）。
fn declared_kind(resolver: &Resolver, ty: &TypeDecl) -> TypeKind {
    match resolver.follow(ty) {
        Some(TypeDecl::Kind {
            kind: DeclaredKind::Known(kind),
            ..
        }) => *kind,
        // 未知の種別トークン（要件 11.7）。
        Some(TypeDecl::Kind {
            kind: DeclaredKind::Unknown(_),
            ..
        }) => TypeKind::Any,
        // `resolve` が実在しない参照と不可能な循環を拒否済みであるため、ここへは来ない
        // （`Resolver::follow` の doc）。防御として `Any` に倒す。
        Some(TypeDecl::Ref(_)) | None => TypeKind::Any,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;

    use document_format::{
        CellValue, Document, DocumentFormat, DocumentFormatApi, SchemaPart, SheetId,
    };
    use document_session::{DocumentSessions, DocumentSessionsApi};
    use macro_runtime::host::HostPort;
    use macro_runtime::{
        FailureKind, Limits, MacroActor, MacroKind, MacroName, MacroRecord, RunOutcome, RunRequest,
    };
    use schema_engine::{
        schema_to_text, ColumnDecl, Constraints, DeclaredKind, Schema, TypeDecl, TypeKind,
    };

    use super::*;

    /// 一時ディレクトリ（`session/host.rs` のテストと同じ規律。プロセスごとに一意）。
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("時計は 1970 以降である")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "jxcel-macro-host-{tag}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
            Self { path }
        }

        fn file(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// 標本の宣言: 名前（`text`）・金額（`int`・必須）・比率（`float`）・印（`bool`）・
    /// 添付（`attachment`）。
    ///
    /// 5 列にしてあるのは、**値の写像の規則**（`host/value.rs`。要件 4.2, 4.3）を実文書で
    /// 観測するためである — `int` の列は数値、`attachment` の列は hex の文字列として渡る。
    fn declaration() -> SchemaPart {
        let column = |name: &str, kind: TypeKind| ColumnDecl {
            name: name.into(),
            ty: TypeDecl::Kind {
                kind: DeclaredKind::Known(kind),
                constraints: Constraints::default(),
            },
            required: false,
            unique: false,
            default: None,
            description: None,
        };
        let mut needed = column("金額", TypeKind::Int);
        needed.required = true;
        let schema = Schema {
            columns: vec![
                column("名前", TypeKind::Text),
                needed,
                column("比率", TypeKind::Float),
                column("印", TypeKind::Bool),
                column("証跡", TypeKind::Attachment),
            ],
        };
        let root = schema_to_text(&schema).expect("宣言は正準出力できる");
        SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#)).expect("宣言は解析できる")
    }

    /// 標本の添付にするバイト列（識別子は内容から決まるため、実行のたびに同じになる）。
    const ATTACHMENT_BYTES: &[u8] = b"sample attachment bytes";

    /// 2 行の**本物の文書**を書き、マクロから見える添付の hex を返す
    /// （公開 API だけで組み立てる。手で作った文書を置かない）。
    fn write_document(path: &Path) -> String {
        let mut document = Document::new();
        let sheet = document.add_sheet("台帳");
        // 添付は**登録してから**行へ入れる（参照するだけの識別子を作らない）。
        let attachment = document.add_attachment(ATTACHMENT_BYTES.to_vec());
        document
            .set_sheet_columns(
                sheet,
                vec![
                    "名前".to_owned(),
                    "金額".to_owned(),
                    "比率".to_owned(),
                    "印".to_owned(),
                    "証跡".to_owned(),
                ],
            )
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, declaration())
            .expect("標本のシートは実在する");
        for (name, amount, ratio, mark) in [
            ("甲", 42_i64, 1.5_f64, true),
            ("乙", 7_i64, 0.25_f64, false),
        ] {
            let row = document.add_row(sheet).expect("標本のシートは実在する");
            document
                .set_row_values(
                    sheet,
                    row,
                    vec![
                        CellValue::Text(name.to_owned()),
                        CellValue::Int(amount),
                        CellValue::Float(ratio),
                        CellValue::Bool(mark),
                        CellValue::Attachment(attachment),
                    ],
                )
                .expect("標本の行は実在する");
        }
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
        attachment.to_hex()
    }

    /// 標本（一時ディレクトリ・縫い目・添付の識別子）。
    struct Sample {
        /// 標本の置き場（落ちたら消える）。
        scratch: Scratch,
        /// 標本の文書へ繋いだ縫い目。
        host: Arc<DocumentHost>,
        /// 添付の識別子（マクロからは hex の文字列として見える）。
        attachment: String,
    }

    /// 標本の文書を持つセッションを作り、そのウィンドウの縫い目を返す。
    ///
    /// セッションは縫い目が `Arc` で保持する（文書は実行の間ずっと引ける）。
    fn opened(tag: &str) -> Sample {
        let scratch = Scratch::new(tag);
        let path = scratch.file("標本.jxcel");
        let attachment = write_document(&path);
        let sessions = Arc::new(DocumentSessions::new());
        let window = WindowLabel::new("doc-1");
        sessions
            .resolve(&window, Some(&path))
            .expect("標本を読み込める");
        Sample {
            scratch,
            host: Arc::new(DocumentHost::new(sessions, window)),
            attachment,
        }
    }

    /// マクロを実行し、結末を返す。
    fn run(host: Arc<dyn HostPort>, name: &str, source: &str) -> RunOutcome {
        let actor = MacroActor::spawn().expect("実行のスレッドを起こせる");
        let record = MacroRecord::new(MacroName::from(name), MacroKind::TypeScript, source);
        let request = RunRequest::new(
            record,
            Limits::default(),
            macro_runtime::WindowLabel::from("doc-1"),
        );
        let outcome = actor.run(request, host).expect("実行の要求が届く");
        actor.shutdown().expect("実行のスレッドを畳める");
        outcome
    }

    /// 走り切った結果の戻り値（JSON の文字列）。
    fn ran_value(outcome: &RunOutcome) -> String {
        match outcome {
            RunOutcome::Ran { value, .. } => value.clone(),
            other => panic!("最後まで走り切るはずである: {other:?}"),
        }
    }

    /// 失敗の種別と理由。
    fn failure(outcome: &RunOutcome) -> (FailureKind, String) {
        match outcome {
            RunOutcome::Failed { failure } => (failure.kind.clone(), failure.message.clone()),
            other => panic!("失敗するはずである: {other:?}"),
        }
    }

    /// 実文書のシート・列・行がマクロから読め、値が宣言の型の写像どおりに渡る（要件 4.1–4.4）。
    #[test]
    fn 実文書のシートと列と行がマクロから読める() {
        let sample = opened("read");
        let source = r#"
const シート = host.sheets()[0];
const 列 = host.columns(シート.id);
const 頁 = host.readRange(シート.id, { from: 0, to: 1 });
const 一 = 頁.rows[0];
export default [
  シート.name,
  String(シート.row_count),
  列.map((c) => c.kind).join(","),
  列.map((c) => c.required).join(","),
  typeof 一.cells[1],
  typeof 一.cells[4],
  String(一.cells[1]),
  一.cells[0],
  一.cells[4],
  typeof 頁.rows[1].cells[3],
].join("|");
"#;
        let outcome = run(
            Arc::clone(&sample.host) as Arc<dyn HostPort>,
            "読む",
            source,
        );
        assert_eq!(
            ran_value(&outcome),
            format!(
                "\"台帳|2|Text,Int,Float,Bool,Attachment|false,true,false,false,false|number|string|42|甲|{}|boolean\"",
                sample.attachment
            ),
            "実文書のシート・列・行と値の写像がマクロから見えていない"
        );
    }

    /// 範囲の読みは**要求した範囲だけ**を返し、範囲の外と逆転した範囲は空になる（要件 4.4）。
    #[test]
    fn 範囲の読みは要求した範囲に閉じる() {
        let sample = opened("span");
        let source = r#"
const シート = host.sheets()[0];
const 頁 = host.readRange(シート.id, { from: 1, to: 9 });
const 逆 = host.readRange(シート.id, { from: 5, to: 1 });
export default [String(頁.rows.length), 頁.rows[0].cells[0], String(逆.rows.length)].join("|");
"#;
        let outcome = run(
            Arc::clone(&sample.host) as Arc<dyn HostPort>,
            "範囲",
            source,
        );
        assert_eq!(ran_value(&outcome), "\"1|乙|0\"");
    }

    /// 自分の書き込みは重ね合わせを通して同じ実行の中で読める（要件 5.1 の裏面）。
    ///
    /// 重ね合わせの規則は `host/overlay.rs` が持ち、本アダプタは**呼ぶ**（規則を写経しない）。
    #[test]
    fn 自分の書き込みは重ね合わせを通して読める() {
        let sample = opened("overlay");
        let source = r#"
const シート = host.sheets()[0];
const 頁 = host.readRange(シート.id, { from: 0, to: 1 });
host.setCells(シート.id, [{ row: 頁.rows[0].id, column: 0, value: "書き換えた" }]);
const 後 = host.readRange(シート.id, { from: 0, to: 1 });
export default [頁.rows[0].cells[0], 後.rows[0].cells[0], 後.rows[1].cells[0]].join("|");
"#;
        let outcome = run(
            Arc::clone(&sample.host) as Arc<dyn HostPort>,
            "書く",
            source,
        );
        assert_eq!(
            ran_value(&outcome),
            "\"甲|書き換えた|乙\"",
            "自分の書き込みが読みに重なっていない（または他の行まで変わっている）"
        );
    }

    /// 宣言の無いファイル読みは**門**が呼び出しの前に拒み、縫い目へ届かない（要件 8.1, 8.3）。
    ///
    /// 「届かない」ことは**縫い目の呼び出しの記録**で確かめる（実装が読んでから拒んだのでは
    /// ないこと）。理由には能力の名前（`file.read`）が入る。
    #[test]
    fn 宣言の無いファイル読みは門で拒まれ縫い目へ届かない() {
        let sample = opened("gate");
        let note = sample.scratch.file("メモ.txt");
        std::fs::write(&note, "読めてはいけない").expect("標本のファイルを書ける");
        let recorded = Arc::new(RecordingHost::new(
            Arc::clone(&sample.host) as Arc<dyn HostPort>
        ));
        let source = format!(
            "host.fileRead({:?});",
            note.to_str().expect("一時ディレクトリは UTF-8 である")
        );
        let outcome = run(Arc::clone(&recorded) as Arc<dyn HostPort>, "読む", &source);

        let (kind, message) = failure(&outcome);
        assert_eq!(
            kind,
            FailureKind::HostRejected {
                api: "fileRead".to_owned()
            },
            "拒否が API の名前つきで返っていない: {message}"
        );
        assert!(
            message.contains("file.read"),
            "理由に能力の名前が入っていない: {message}"
        );
        assert!(
            recorded.calls().is_empty(),
            "門が拒んだ呼び出しが縫い目へ届いた: {:?}",
            recorded.calls()
        );
    }

    /// 宣言したファイルの読み書きは**文書の位置を基準に**働く（要件 8.1）。
    #[test]
    fn 宣言したファイルの読み書きは文書の位置を基準に働く() {
        let sample = opened("files");
        std::fs::write(sample.scratch.file("入力.txt"), "本文").expect("標本のファイルを書ける");
        let source = r#"// @grant file.read, file.write
const 読んだ = host.fileRead("入力.txt");
host.fileWrite("出力.txt", 読んだ + "を写した");
export default 読んだ;
"#;
        let outcome = run(
            Arc::clone(&sample.host) as Arc<dyn HostPort>,
            "写す",
            source,
        );
        assert_eq!(
            ran_value(&outcome),
            "\"本文\"",
            "相対パスの読みが働いていない"
        );
        assert_eq!(
            "本文を写した",
            std::fs::read_to_string(sample.scratch.file("出力.txt"))
                .expect("書き出したファイルを読める"),
            "相対パスの書きが働いていない"
        );
    }

    /// 大き過ぎる本文は**書く前に**拒む（ディスクに触らない。要件 8.1, 8.3）。
    #[test]
    fn 大き過ぎる書き出しはディスクに触れる前に拒む() {
        let sample = opened("big-write");
        let oversized = "あ".repeat(MAX_FILE_WRITE_BYTES / 3 + 1);
        assert!(
            oversized.len() > MAX_FILE_WRITE_BYTES,
            "標本が上限を越えていない"
        );
        let refusal = sample
            .host
            .file_write("巨大.txt", &oversized)
            .expect_err("上限を越える本文は拒まれる");
        assert!(
            refusal.reason().contains("大き過ぎる"),
            "理由に上限が書かれていない: {refusal}"
        );
        assert!(
            !sample.scratch.file("巨大.txt").exists(),
            "上限を越える本文がディスクへ書かれた"
        );
    }

    /// ディレクトリは読めない（返す型が `String` であり、ディレクトリに本文は無い）。
    #[test]
    fn ディレクトリは読めない() {
        let sample = opened("directory");
        let refusal = sample
            .host
            .file_read(
                sample
                    .scratch
                    .path
                    .to_str()
                    .expect("一時ディレクトリは UTF-8 である"),
            )
            .expect_err("ディレクトリは読めない");
        assert!(
            refusal.reason().contains("ディレクトリ"),
            "理由にディレクトリであることが書かれていない: {refusal}"
        );
    }

    /// 保存されていない文書では相対パスの基準が無く、理由つきで拒む（要件 8.1, 8.3）。
    ///
    /// 基準を勝手に作業ディレクトリへ倒さない — **どのファイルを触るか**を利用者が読める形に
    /// 残すためである。
    #[test]
    fn 保存されていない文書では相対パスを拒む() {
        let sessions = Arc::new(DocumentSessions::new());
        let window = WindowLabel::new("doc-1");
        sessions.create(&window).expect("新規作成できる");
        let host = Arc::new(DocumentHost::new(sessions, window));
        let source = r#"// @grant file.read
host.fileRead("入力.txt");
"#;
        let outcome = run(host as Arc<dyn HostPort>, "読む", source);
        let (_, message) = failure(&outcome);
        assert!(
            message.contains("保存されていない") && message.contains("入力.txt"),
            "理由に基準が無いことが書かれていない: {message}"
        );
    }

    /// `http` を取得し、`http` / `https` 以外の綴りを理由つきで拒む（要件 8.1, 8.3）。
    #[test]
    fn ネットワークはhttpだけを取り他の綴りを拒む() {
        let sample = opened("net");
        let url = serve_once("取得した本文");

        let source =
            format!("// @grant net\nconst 本文 = host.netFetch({url:?});\nexport default 本文;\n");
        let outcome = run(
            Arc::clone(&sample.host) as Arc<dyn HostPort>,
            "取る",
            &source,
        );
        assert_eq!(
            ran_value(&outcome),
            "\"取得した本文\"",
            "http の取得が働いていない"
        );

        let source = "// @grant net\nhost.netFetch(\"file:///etc/passwd\");\n";
        let outcome = run(
            Arc::clone(&sample.host) as Arc<dyn HostPort>,
            "取る",
            source,
        );
        let (_, message) = failure(&outcome);
        assert!(
            message.contains("file") && message.contains("http"),
            "理由に取れない綴りが書かれていない: {message}"
        );
    }

    /// 文書に無いシート・行への書き込みは理由つきで拒まれ、**変更集合へ入らない**（要件 5.4）。
    #[test]
    fn 文書に無いシートや行への書き込みは拒まれる() {
        let sample = opened("missing");
        let source = r#"
const シート = host.sheets()[0];
host.setCells("00000000000000000000000000", [{ row: "00000000000000000000000000", column: 0, value: 1 }]);
"#;
        let outcome = run(
            Arc::clone(&sample.host) as Arc<dyn HostPort>,
            "書く",
            source,
        );
        let (kind, message) = failure(&outcome);
        assert_eq!(
            kind,
            FailureKind::HostRejected {
                api: "setCells".to_owned()
            },
            "拒否が API の名前つきで返っていない: {message}"
        );
        assert!(
            message.contains("00000000000000000000000000"),
            "理由に無いシートが名指しされていない: {message}"
        );
        sample.host.with_changes(&mut |changes: &ChangeSet| {
            assert!(changes.is_empty(), "拒ばれた書き込みが集約されている");
        });

        let source = r#"
const シート = host.sheets()[0];
host.setCells(シート.id, [{ row: "00000000000000000000000000", column: 0, value: 1 }]);
"#;
        let outcome = run(
            Arc::clone(&sample.host) as Arc<dyn HostPort>,
            "書く",
            source,
        );
        let (_, message) = failure(&outcome);
        assert!(
            message.contains("00000000000000000000000000") && message.contains("無い"),
            "理由に無い行が名指しされていない: {message}"
        );
    }

    /// 列の数と合わない行の追加は理由つきで拒まれる（行の値は列の添字で並ぶ。要件 5.4）。
    #[test]
    fn 列の数と合わない行の追加は拒まれる() {
        let sample = opened("insert-width");
        let source = r#"
const シート = host.sheets()[0];
host.insertRows(シート.id, [["足りない"]]);
"#;
        let outcome = run(
            Arc::clone(&sample.host) as Arc<dyn HostPort>,
            "足す",
            source,
        );
        let (_, message) = failure(&outcome);
        assert!(
            message.contains("列の数"),
            "理由に列数との不一致が書かれていない: {message}"
        );
    }

    /// 呼び出しの記録を挟む縫い目（**門が止めたことを観測するための二重**）。
    ///
    /// `crates/macro-runtime/src/engine/isolate.rs` の `StubHost` と同じ形であり、本番の実装
    /// ではない（委譲して記録するだけである）。
    struct RecordingHost {
        inner: Arc<dyn HostPort>,
        calls: StdMutex<Vec<&'static str>>,
    }

    impl RecordingHost {
        fn new(inner: Arc<dyn HostPort>) -> Self {
            Self {
                inner,
                calls: StdMutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        fn record(&self, call: &'static str) {
            self.calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(call);
        }
    }

    impl HostPort for RecordingHost {
        fn sheets(&self) -> Result<Vec<SheetInfo>, HostError> {
            self.record("sheets");
            self.inner.sheets()
        }

        fn columns(&self, sheet: SheetId) -> Result<Vec<ColumnTypeInfo>, HostError> {
            self.record("columns");
            self.inner.columns(sheet)
        }

        fn read_rows(&self, sheet: SheetId, span: RowSpan) -> Result<RowPage, HostError> {
            self.record("read_rows");
            self.inner.read_rows(sheet, span)
        }

        fn stage(&self, change: Change) -> Result<(), HostError> {
            self.record("stage");
            self.inner.stage(change)
        }

        fn with_changes(&self, read: &mut dyn FnMut(&ChangeSet)) {
            self.inner.with_changes(read)
        }

        fn file_read(&self, path: &str) -> Result<String, HostError> {
            self.record("file_read");
            self.inner.file_read(path)
        }

        fn file_write(&self, path: &str, text: &str) -> Result<(), HostError> {
            self.record("file_write");
            self.inner.file_write(path, text)
        }

        fn net_fetch(&self, url: &str) -> Result<String, HostError> {
            self.record("net_fetch");
            self.inner.net_fetch(url)
        }
    }

    /// 1 回だけ本文を返す最小の HTTP サーバを立て、その URL を返す。
    ///
    /// 外部のネットワークへ出ない（検査が環境に依存しない）。
    fn serve_once(body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("空いているポートを取れる");
        let address = listener.local_addr().expect("待ち受けの住所を取れる");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // 要求を読み切ってから応答する（読まずに書くと、相手が要求を書き終える前に
                // 接続が閉じることがある）。
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{address}/")
    }
}

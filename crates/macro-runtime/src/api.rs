//! 外から見える唯一の面（design.md「Components and Interfaces → MacroRuntimeApi」。
//! tasks.md 4.3。要件 1.1–1.7, 2.1, 2.3, 2.5, 3.1–3.5, 6.1–6.5, 7.1, 9.1–9.4）。
//!
//! アダプタ（`src-tauri` の `commands/macro.rs`）が呼ぶのは**本モジュールの 4 面だけ**である:
//!
//! | 面 | 何をするか | 文書へ触るか |
//! |----|-----------|-------------|
//! | [`MacroRuntimeApi::list`] | 記録の並びを**要約**（名前・種別・宣言・解釈できなかった理由）へ写す | 触らない |
//! | [`MacroRuntimeApi::store`] | 記録の並びへ 1 件を反映する（同じ名前は置き換え。要件 1.6） | 触らない |
//! | [`MacroRuntimeApi::delete`] | 記録の並びから名前で 1 件を取り除く（要件 1.7） | 触らない |
//! | [`MacroRuntimeApi::run`] | 縫い目（[`HostPort`]）越しに実行し、結果を返す | **触らない**（書きは変更集合） |
//!
//! **本クレートは文書を持たない。** 一覧・保存・削除が扱うのは**記録の並び**
//! （`Vec<MacroRecord>`）という値であり、ドキュメントのパート（`document-format` の
//! `macros.json`）への読み書きはアダプタの仕事である（design.md 決定 4。1.6 の
//! `tests/macro_part_roundtrip.rs` も「製品の経路ではこの写像をアダプタ（群 4）が担う」と
//! 明記している）。これが層の鎖（下）を守ったまま「外から見える唯一の面」を作る形である。
//!
//! # 層の鎖（design.md「File Structure Plan」）
//!
//! `error / source → surface → host → engine → types → api`。本層は鎖の最右であり、
//! **すべての層を参照してよい唯一の層**である（[`MacroActor`] を所有し、[`Transpiler`] を
//! 呼び、[`HostPort`] を受け取る）。逆向きの参照（下位の層から `api` を参照する）は無い。
//!
//! # `MacroError`（設計の `Result<_, MacroError>` の実体）
//!
//! 設計の Service Interface は `Result<RunOutcome, MacroError>` と書くが、`MacroError` の
//! 定義はどの層にも無かった（1.3 の `engine/outcome.rs` が「定義しないもの」に挙げている）。
//! 本モジュールが定義する理由は、**本層が唯一 actor を所有する層**だからである —
//! [`MacroError`] が運ぶのは「actor が要求を受け取らなかった」という**面の失敗**だけで、
//! マクロの失敗（例外・構文誤り・打ち切り）は 3 値の [`RunOutcome`] が運ぶ。両者を混ぜると、
//! 「実行できなかった」と「実行したが失敗した」の区別が境界で消える（要件 2.4, 9.1）。
//!
//! # 実行の前の解釈（一覧は実行しない。要件 1.3）
//!
//! [`MacroRuntimeApi::list`] は**実行せずに**マクロを解釈する。解釈は 2 段である:
//!
//! 1. **能力の宣言**（`source/capability.rs`。要件 8.3, 8.4）— 未知の綴りは名前と位置つきで拒む
//! 2. **種別としての構文**（`engine/transpile.rs` の [`Transpiler`]。要件 3.4）
//!
//! どちらの失敗も [`MacroFailure`]（`Source` / `Transpile` と、位置を持つフレーム）として
//! 要約に載り、**マクロは一覧に残る**（要件 1.4。ドキュメントは開ける）。**型の検査はしない**
//! （要件 3.2。エディタを所有する機能の役割である）。
//!
//! 構文の解釈に**変換の口（`Transpiler::transpile`）をそのまま使う**のは意図である:
//! 変換器は TypeScript の型注釈を落とすだけで型を検査せず（要件 3.2）、JavaScript の種別では
//! ソースをバイト単位でそのまま返す（要件 3.3）ため、構文の可否だけを知るのに足りる。
//! **解析だけを行う別の口を足さない** — 変換器（`engine/transpile.rs`）は tasks.md 3.1 の
//! 担当が所有しており、本タスク（4.3）の境界を越えるためである。
//!
//! # 保存と削除は規則だけを行う（文書の印はアダプタが立てる）
//!
//! [`MacroRuntimeApi::store`] / [`MacroRuntimeApi::delete`] は**値の並びを書き換えるだけ**で、
//! 未保存の印も版も触らない（触れない — 文書を持たない）。`structure.md`「セッションの所有の
//! 規約」が求める「未保存の印と版の記録」は、アダプタが `document-session` の `edit` の閉包の
//! 内側で本 2 面を呼ぶことで満たされる（design.md の System Flows）。
//!
//! # 観測（tasks.md 4.3 の受け入れ。テストとして固定してある）
//!
//! | 観測 | テスト |
//! |------|--------|
//! | 解釈できるマクロは宣言つきで、**解釈できないマクロは理由つきで一覧に残る**（要件 1.3, 1.4） | `tests::解釈できないマクロも一覧に残り理由が付く` |
//! | 同じ名前の保存は置き換えであり、位置を保つ（要件 1.6） | `tests::同じ名前の保存は位置を保って置き換える` |
//! | 削除は名前で 1 件を取り除く（要件 1.7） | `tests::削除は名前で1件を取り除く` |
//! | 実行が結果（戻り値・出力・変更の件数）を返す（要件 2.3, 2.5） | `tests::実行が戻り値と出力と件数を返す` |
//! | 宣言の無いソースの解釈の失敗が `Failed`（`Source`）として返る（要件 8.3） | `tests::宣言の誤りは解釈の失敗として返る` |

use std::io;
use std::sync::Arc;
use std::thread;

use crate::engine::actor::{ActorError, MacroActor};
use crate::engine::outcome::{FailureKind, Frame, MacroFailure, RunOutcome, RunRequest};
use crate::engine::transpile::Transpiler;
use crate::host::HostPort;
use crate::source::capability::{self, CapabilitySet};
use crate::source::record::{remove, upsert, MacroKind, MacroName, MacroRecord};

/// 一覧に載る 1 件（要件 1.3, 1.4）。
///
/// **解釈できなかったマクロも 1 件として載る**（要件 1.4）。そのとき [`Self::failure`] が
/// 理由（種別・理由・位置）を持ち、[`Self::declaration`] は `None` である。解釈できたマクロは
/// その逆である。**どちらか一方だけが `Some`** であり、2 つ同時に `Some` にはならない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroSummary {
    /// マクロの名前（要件 1.3）。
    pub name: MacroName,
    /// マクロの種別（要件 1.3）。
    pub kind: MacroKind,
    /// 宣言されている能力（要件 8.2）。解釈できなかったときは `None`。
    pub declaration: Option<CapabilitySet>,
    /// 解釈できなかった理由（要件 1.4）。解釈できたときは `None`。
    pub failure: Option<MacroFailure>,
}

impl MacroSummary {
    /// 解釈できたか（＝実行できる見込みがあるか）。
    ///
    /// 実行の面（4.4）が「実行できるマクロが 1 つも無ければ導線を出さない」（要件 2.7）を
    /// 判断する材料である。**構文が通ることは実行の成功を意味しない** — 実行時の失敗は
    /// [`RunOutcome`] が運ぶ（要件 2.4）。
    pub const fn is_runnable(&self) -> bool {
        self.failure.is_none()
    }
}

/// エンジンの入口（design.md「MacroRuntimeApi」）。
///
/// 実体は [`MacroActor`] 1 つである（isolate を所有する専用スレッド。design.md 決定 1）。
/// **アプリ全体で 1 実体**を作り、ウィンドウごとには作らない — 実行は直列であり（design.md
/// 「State Management」の「1 実行ずつ」）、actor を分けても並列にはならないためである。
#[derive(Debug)]
pub struct MacroRuntime {
    /// V8 isolate を所有する専用スレッドへの口（`engine/actor.rs`）。
    actor: MacroActor,
}

impl MacroRuntime {
    /// 実行基盤（actor）を起こす。
    ///
    /// **isolate はここでは作らない** — 最初の実行まで遅れる（`MacroActor::spawn` の doc。
    /// マクロを 1 度も実行しない起動から isolate の費用を外すためである）。
    ///
    /// # Errors
    ///
    /// 専用スレッドを起こせないとき（資源の枯渇）だけである。
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            actor: MacroActor::spawn()?,
        })
    }

    /// 実行中か（要件 2.2 の提示の材料。実行の面 4.4 が読む）。
    pub fn is_running(&self) -> bool {
        self.actor.is_running()
    }

    /// 後始末: 専用スレッドを畳む（`MacroActor::shutdown` と同じ規律）。
    ///
    /// # Errors
    ///
    /// 専用スレッドが実行の途中で panic したとき。
    pub fn shutdown(&self) -> thread::Result<()> {
        self.actor.shutdown()
    }

    /// 記録 1 件を解釈する（実行しない。要件 1.3, 1.4, 8.3）。
    ///
    /// 返るのは**宣言された能力の集合**であり、解釈できなかったときは [`MacroFailure`] を返す。
    /// 失敗の種別は 2 つに限られる: 宣言の誤りは [`FailureKind::Source`]（実行しない。
    /// design.md「Error Handling」の層「ソースの解釈」）、構文の誤りは
    /// [`FailureKind::Transpile`] である（要件 3.4 の分類をそのまま使う）。
    ///
    /// **`list` の内側だけが呼ぶ**（外から見える面は 4 つである。上の表）。本メソッドを
    /// public にしない理由は、`is_runnable` の判定材料（要約）と解釈の実体が 2 つに割れるのを
    /// 防ぐためである。
    fn interpret(&self, record: &MacroRecord) -> Result<CapabilitySet, MacroFailure> {
        // 1. 宣言（1.6）。`engine/isolate.rs` の `Isolate::build` と同じ分類・同じ文言を使う
        //    （実行の直前の拒否と、一覧の理由が食い違わないようにする）。
        let declaration = capability::parse(&record.source).map_err(|error| {
            let frames = match &error {
                capability::DeclarationError::UnknownCapability { line, column, .. } => vec![
                    // 宣言の誤りにも位置がある（要件 8.3 の提示は名前と位置を出す）。
                    Frame::at(record.name.clone(), None, *line, *column),
                ],
            };
            MacroFailure::new(FailureKind::Source, error.to_string(), frames)
        })?;
        // 2. 構文（種別として解釈できるか。要件 3.4）。型の検査はしない（要件 3.2）。
        Transpiler::new().transpile(record)?;
        Ok(declaration)
    }

    /// 記録 1 件を要約へ写す（[`MacroRuntimeApi::list`] の実体）。
    fn summarize(&self, record: &MacroRecord) -> MacroSummary {
        match self.interpret(record) {
            Ok(declaration) => MacroSummary {
                name: record.name.clone(),
                kind: record.kind,
                declaration: Some(declaration),
                failure: None,
            },
            Err(failure) => MacroSummary {
                name: record.name.clone(),
                kind: record.kind,
                declaration: None,
                failure: Some(failure),
            },
        }
    }
}

/// 外から見える唯一の面（design.md「MacroRuntimeApi」の Service Interface）。
///
/// 4 面であり、いずれも**文書へ触らない**: 記録の並びは値として受け取り、実行の読み書きは
/// [`HostPort`]（アダプタが差し込む縫い目）越しだけである（design.md 決定 2 / 3）。
pub trait MacroRuntimeApi {
    /// 記録の並びを要約へ写す（要件 1.3, 1.4, 8.2）。
    ///
    /// **並びの順序を保つ**（保存順が一覧の提示順である。design.md「Logical Data Model」）。
    /// 解釈できない記録も 1 件として残り、理由が付く（要件 1.4）。
    fn list(&self, records: &[MacroRecord]) -> Vec<MacroSummary>;

    /// 記録の並びへ 1 件を反映する（要件 1.6）。
    ///
    /// **同じ名前があれば置き換え、その位置を保つ**（`source/record.rs` の `upsert` が規則の
    /// 唯一の実体）。新しい名前は末尾へ足す。返るのは反映後の要約であり、**解釈できない
    /// ソースでも保存は成立する**（要件 1.4 は一覧の提示についての要求であり、保存を止めない
    /// — 途中まで書いたマクロを保存できなくすると、利用者は編集の途中で保存できなくなる）。
    fn store(&self, records: &mut Vec<MacroRecord>, record: MacroRecord) -> MacroSummary;

    /// 記録の並びから名前で 1 件を取り除く（要件 1.7）。
    ///
    /// 返るのは取り除いた記録である（無ければ `None`。`source/record.rs` の `remove` と同じ）。
    /// **同じ名前が 2 件ある並び**（上流は一意性を検査しない）では**先頭の 1 件だけ**が落ちる
    /// — `remove` の規則をそのまま使う。
    fn delete(&self, records: &mut Vec<MacroRecord>, name: &MacroName) -> Option<MacroRecord>;

    /// 1 件を実行する（要件 2.1, 2.3, 2.4, 6.1–6.4）。
    ///
    /// 結果は 3 値である（[`RunOutcome::Ran`] / [`RunOutcome::Failed`] /
    /// [`RunOutcome::Aborted`]）。**実行そのものが始まらなかったこと**（実行中の 2 つ目の要求・
    /// 停止済みの actor）だけが [`MacroError`] である。変更集合は縫い目（`port`）の内側にあり、
    /// **本メソッドは適用しない**（アダプタの仕事。要件 5.1）。
    ///
    /// # Errors
    ///
    /// [`MacroError::Busy`]（実行中の要求）または [`MacroError::Stopped`]（専用スレッドが
    /// 居ない）。
    fn run(&self, request: RunRequest, port: Arc<dyn HostPort>) -> Result<RunOutcome, MacroError>;
}

impl MacroRuntimeApi for MacroRuntime {
    fn list(&self, records: &[MacroRecord]) -> Vec<MacroSummary> {
        records
            .iter()
            .map(|record| self.summarize(record))
            .collect()
    }

    fn store(&self, records: &mut Vec<MacroRecord>, record: MacroRecord) -> MacroSummary {
        let summary = self.summarize(&record);
        // 規則は `source/record.rs` の 1 箇所（位置を保つ置き換え。要件 1.6）。
        upsert(records, record);
        summary
    }

    fn delete(&self, records: &mut Vec<MacroRecord>, name: &MacroName) -> Option<MacroRecord> {
        remove(records, name)
    }

    fn run(&self, request: RunRequest, port: Arc<dyn HostPort>) -> Result<RunOutcome, MacroError> {
        // **同期である**（`engine/actor.rs` の doc「呼び出しの形」）。非同期の実行文脈から
        // 直接呼ぶと tokio が panic するため、アダプタは `spawn_blocking` の上で呼ぶ。
        self.actor.run(request, port).map_err(MacroError::from)
    }
}

/// 実行の要求が**受け取られなかった**理由（design.md「Error Handling」の「業務の誤り」）。
///
/// 「マクロが失敗した」ではない（それは [`RunOutcome::Failed`] である）。本型が運ぶのは
/// 「実行という仕事が始まらなかった」ことだけであり、境界では**経路の失敗**として現れる
/// （アダプタの `IpcError::Document`）。提示の文言は持たない（`structure.md` の規約）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroError {
    /// 実行中である（要件 2.2 の裏返し。並行実行しない）。
    Busy,
    /// 専用スレッドが居ない（停止済み、または実行の途中で畳まれた）。
    Stopped,
}

impl MacroError {
    /// 診断の記録に使う安定トークン（ロケール依存なし）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Stopped => "stopped",
        }
    }
}

impl From<ActorError> for MacroError {
    fn from(error: ActorError) -> Self {
        match error {
            ActorError::Busy => Self::Busy,
            ActorError::Stopped => Self::Stopped,
        }
    }
}

impl core::fmt::Display for MacroError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for MacroError {}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use document_format::SheetId;

    use super::*;
    use crate::engine::outcome::Limits;
    use crate::host::changes::{Change, ChangeSet};
    use crate::host::overlay::{ColumnTypeInfo, RowPage, RowSpan, SheetInfo};
    use crate::host::HostError;

    /// ホストを持たないマクロ用の縫い目（**呼ばれたら理由つきで拒む**）。
    ///
    /// 本モジュールの検査は「実行の入口が結果を返すこと」だけを見るため、マクロはホスト API を
    /// 呼ばない。それでも実装が要るのは、op の本体が panic してはならない（`engine/isolate.rs`
    /// の doc。panic はプロセスを abort させる）ためである — **panic ではなく `Err` を返す**。
    struct NoHost {
        changes: Mutex<ChangeSet>,
    }

    impl NoHost {
        fn new() -> Self {
            Self {
                changes: Mutex::new(ChangeSet::new()),
            }
        }

        fn absent(what: &str) -> HostError {
            HostError::new(format!("この検査のマクロは {what} を呼ばない"))
        }
    }

    impl HostPort for NoHost {
        fn sheets(&self) -> Result<Vec<SheetInfo>, HostError> {
            Ok(Vec::new())
        }

        fn columns(&self, _sheet: SheetId) -> Result<Vec<ColumnTypeInfo>, HostError> {
            Err(Self::absent("columns"))
        }

        fn read_rows(&self, _sheet: SheetId, _span: RowSpan) -> Result<RowPage, HostError> {
            Err(Self::absent("read_rows"))
        }

        fn stage(&self, _change: Change) -> Result<(), HostError> {
            Err(Self::absent("stage"))
        }

        fn with_changes(&self, read: &mut dyn FnMut(&ChangeSet)) {
            read(&self.changes.lock().expect("毒されていない"));
        }

        fn file_read(&self, _path: &str) -> Result<String, HostError> {
            Err(Self::absent("file_read"))
        }

        fn file_write(&self, _path: &str, _text: &str) -> Result<(), HostError> {
            Err(Self::absent("file_write"))
        }

        fn net_fetch(&self, _url: &str) -> Result<String, HostError> {
            Err(Self::absent("net_fetch"))
        }
    }

    fn record(name: &str, source: &str) -> MacroRecord {
        MacroRecord::new(MacroName::new(name), MacroKind::TypeScript, source)
    }

    fn runtime() -> MacroRuntime {
        MacroRuntime::new().expect("専用スレッドを起こせる")
    }

    /// 解釈できないマクロも一覧に残り、理由が付く（要件 1.3, 1.4）。**実行しない。**
    ///
    /// 3 種類を 1 つの並びで確かめる: 解釈できるもの（宣言つき）、宣言の綴りが読めないもの
    /// （要件 8.3 の名指し）、種別として構文が通らないもの（要件 3.4 の行・列）。
    #[test]
    fn 解釈できないマクロも一覧に残り理由が付く() {
        let runtime = runtime();
        let records = vec![
            record(
                "棚卸し",
                "// @grant file.read, net\nconst xs = await host.readRange(0, 1);\nexport default xs.length;\n",
            ),
            record("綴り違い", "// @grant file.reaad\nexport default 1;\n"),
            record("構文誤り", "const x = ;\n"),
        ];

        let summaries = runtime.list(&records);
        assert_eq!(summaries.len(), 3, "解釈できないマクロを一覧から落とした");

        // 1 件目: 解釈できる。宣言が綴りの辞書順で載る（要件 8.2）。
        assert!(summaries[0].is_runnable());
        assert_eq!(
            summaries[0]
                .declaration
                .as_ref()
                .map(CapabilitySet::described)
                .as_deref(),
            Some("file.read, net")
        );
        assert_eq!(summaries[0].failure, None);

        // 2 件目: 宣言の綴りが読めない。**理由に名前が入り**、位置がフレームに載る。
        let failure = summaries[1].failure.as_ref().expect("理由が付く");
        assert_eq!(failure.kind, FailureKind::Source);
        assert!(
            failure.message.contains("file.reaad"),
            "拒んだ能力の名前が理由に出ていない: {}",
            failure.message
        );
        assert_eq!(
            failure.innermost().map(|frame| (frame.line, frame.column)),
            Some((1, 11)),
            "宣言の位置がフレームに載っていない"
        );
        assert_eq!(summaries[1].declaration, None);

        // 3 件目: 種別として構文が通らない。行と列が付く（要件 3.4）。
        let failure = summaries[2].failure.as_ref().expect("理由が付く");
        assert_eq!(failure.kind, FailureKind::Transpile);
        assert!(
            failure.innermost().is_some_and(|frame| frame.line == 1),
            "構文の誤りの位置が載っていない: {failure:?}"
        );
        assert!(!summaries[2].is_runnable());
    }

    /// 同じ名前の保存は**位置を保って**置き換え、新しい名前は末尾へ足す（要件 1.6）。
    #[test]
    fn 同じ名前の保存は位置を保って置き換える() {
        let runtime = runtime();
        let mut records = vec![
            record("棚卸し", "export default 1;\n"),
            record("集計", "export default 2;\n"),
        ];

        let summary = runtime.store(&mut records, record("棚卸し", "export default 3;\n"));
        assert_eq!(summary.name.as_str(), "棚卸し");
        assert!(summary.is_runnable());
        assert_eq!(
            records
                .iter()
                .map(|record| (record.name.as_str(), record.source.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("棚卸し", "export default 3;\n"),
                ("集計", "export default 2;\n")
            ],
            "置き換えで位置が動いた、または別の記録が変わった"
        );

        let summary = runtime.store(&mut records, record("検算", "export default 4;\n"));
        assert_eq!(summary.name.as_str(), "検算");
        assert_eq!(records.len(), 3);
        assert_eq!(
            records[2].name.as_str(),
            "検算",
            "新しい名前が末尾に足されていない"
        );

        // **解釈できないソースでも保存は成立する**（要件 1.4 は一覧の提示の要求である）。
        let summary = runtime.store(&mut records, record("書きかけ", "const x = ;\n"));
        assert!(
            !summary.is_runnable(),
            "解釈できないソースに理由が付いていない"
        );
        assert_eq!(records.len(), 4, "解釈できないソースの保存が落ちた");
    }

    /// 削除は名前で 1 件を取り除き、無い名前では何も起きない（要件 1.7）。
    #[test]
    fn 削除は名前で1件を取り除く() {
        let runtime = runtime();
        let mut records = vec![
            record("棚卸し", "export default 1;\n"),
            record("集計", "export default 2;\n"),
        ];

        let removed = runtime
            .delete(&mut records, &MacroName::new("集計"))
            .expect("取り除ける");
        assert_eq!(removed.name.as_str(), "集計");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name.as_str(), "棚卸し", "別の記録が落ちた");

        assert!(runtime
            .delete(&mut records, &MacroName::new("無い"))
            .is_none());
        assert_eq!(records.len(), 1);
    }

    /// 実行が戻り値と出力と変更の件数を返す（要件 2.3, 2.5）。
    ///
    /// マクロはホスト API を呼ばない（縫い目は [`NoHost`]）— 本検査が見るのは**面が結果を
    /// 返すこと**であり、読み書きの経路は 2.x / 4.1 の検査が担う。
    #[test]
    fn 実行が戻り値と出力と件数を返す() {
        let runtime = runtime();
        let request = RunRequest::new(
            record(
                "挨拶",
                "console.log(\"はじめます\");\nexport default 6 * 7;\n",
            ),
            Limits::default(),
            crate::engine::outcome::WindowLabel::from("doc-1"),
        );

        let outcome = runtime
            .run(request, Arc::new(NoHost::new()))
            .expect("実行の要求は受け取られる");
        match outcome {
            RunOutcome::Ran {
                value,
                output,
                changes,
                ..
            } => {
                assert_eq!(value, "42");
                assert_eq!(
                    output
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>(),
                    vec!["はじめます"]
                );
                assert!(changes.is_empty(), "書き込みをしていない実行に件数が付いた");
            }
            other => panic!("成功を期待したが {other:?} を返した"),
        }
    }

    /// 宣言の誤りは**実行の失敗**（`Source`）として返る（要件 8.3）。
    ///
    /// 縫い目を通さずに失敗が決まる（`engine/isolate.rs` の `Isolate::build` が同じ分類を
    /// 返す）ため、結果は 3 値の `Failed` である — 「面の失敗」ではない。
    #[test]
    fn 宣言の誤りは解釈の失敗として返る() {
        let runtime = runtime();
        let request = RunRequest::new(
            record("綴り違い", "// @grant netwerk\nexport default 1;\n"),
            Limits::default(),
            crate::engine::outcome::WindowLabel::from("doc-1"),
        );

        let outcome = runtime
            .run(request, Arc::new(NoHost::new()))
            .expect("実行の要求は受け取られる");
        match outcome {
            RunOutcome::Failed { failure } => {
                assert_eq!(failure.kind, FailureKind::Source);
                assert!(
                    failure.message.contains("netwerk"),
                    "拒んだ能力の名前が理由に出ていない: {}",
                    failure.message
                );
            }
            other => panic!("失敗を期待したが {other:?} を返した"),
        }
    }
}

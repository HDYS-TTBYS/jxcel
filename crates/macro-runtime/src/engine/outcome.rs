//! 実行の要求と、結果・失敗・上限の型（design.md「Components and Interfaces」の Service
//! Interface と「Data Models」。tasks.md 1.3。要件 2.3, 2.4, 5.5, 6.1, 6.2, 9.1, 9.3）。
//!
//! 実行の結果を **`Ran` / `Failed` / `Aborted` の 3 値**とし、**打ち切りを失敗と別の値**に
//! する（要件 6.1 / 6.2 の提示が失敗と異なるため。design.md「Domain Model」）。失敗は
//! **理由・種別・フレーム**を持ち、種別は design.md「Error Handling」の 4 層
//! （ソースの解釈 / 変換 / 実行 / ホスト API の拒否）に対応する。フレームは
//! **TypeScript の原位置**を 1 起点の行・列で指す（要件 9.1, 9.3）。
//!
//! # この層が定義する型 / 定義しないもの（tasks.md 1.3）
//!
//! 型を **1 箇所**に置くことで、並列に進む担当（実行基盤 1.4・上限 1.5・変換 3.1・
//! ホスト 2.x・アダプタ 4.x）が**型で繋がる**ようにする。**実装はここには無い**:
//! 上限の適用と打ち切りはタスク 1.5（`engine/limits.rs`）、変更の集約はタスク 2.3
//! （`host/changes.rs`）、文書への適用はアダプタ（`src-tauri`）が持つ。
//!
//! - 定義する: [`RunRequest`] / [`RunOutcome`] / [`MacroFailure`] / [`Frame`] /
//!   [`FailureKind`] / [`LimitKind`] / [`Limits`] / [`ChangeSummary`] / [`OutputLine`] /
//!   [`WindowLabel`]
//! - 定義しない: `MacroRuntimeApi`（1.4 以降）、`HostPort`（2.x）、`ChangeSet`（2.3）、
//!   `MacroError`（実行の入口を持つタスク）
//!
//! # 提示と、ここに置かないもの
//!
//! [`OutputLine`] の並びは**順序を保つ**（要件 2.3）。利用者へ提示する文言は境界の型に
//! しない（`document-format` / `schema-engine` と同じ規約であり、4.4 の面が組み立てる）。

use core::fmt;
use std::time::Duration;

use crate::source::record::{MacroName, MacroRecord};

/// 実行に課す上限（design.md「Components and Interfaces」の `Limits`。要件 6.1, 6.2, 6.5）。
///
/// 値だけを持つ。**適用**（タイマー・`heap_limits`・打ち切り後の復帰）はタスク 1.5 が
/// `engine/limits.rs` に持つ。上限は設定からアダプタが解決して渡す（要件 6.5、タスク 4.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// 実行に許す時間。超えたら**時間**の上限による打ち切りになる（要件 6.1）。
    pub time: Duration,
    /// 実行に許すメモリの量（バイト）。超えたら**メモリ**の上限による打ち切りになる（要件 6.2）。
    pub memory_bytes: u64,
}

impl Limits {
    /// 既定の時間の上限（30 秒。要件 6.1）。
    pub const DEFAULT_TIME: Duration = Duration::from_secs(30);

    /// 既定のメモリの上限（512 MB。要件 6.2）。
    pub const DEFAULT_MEMORY_BYTES: u64 = 512 * 1024 * 1024;

    /// 上限の対を組み立てる。
    pub const fn new(time: Duration, memory_bytes: u64) -> Self {
        Self { time, memory_bytes }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::new(Self::DEFAULT_TIME, Self::DEFAULT_MEMORY_BYTES)
    }
}

/// 打ち切りの種類（要件 6.1, 6.2）。
///
/// **どちらの上限に当たったか**が提示で変わるため（要件 6.1 / 6.2）、種別として持つ。
/// 種別の記録はタスク 1.5 が行い、[`RunOutcome::Aborted`] に載る。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimitKind {
    /// 時間の上限（既定 30 秒。要件 6.1）。
    Time,
    /// メモリの上限（既定 512 MB。要件 6.2）。
    Memory,
}

impl LimitKind {
    /// 診断の記録に使う安定トークン（ロケール依存なし。タスク 4.3 の記録の 1 行）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::Memory => "memory",
        }
    }
}

/// 失敗に至る呼び出しの 1 段（design.md「Components and Interfaces」の `Frame`。要件 9.1, 9.3）。
///
/// 行と列は **1 起点**であり、`deno_ast` の位置の数え方と同じである。変換（タスク 3.1）が
/// ソースマップを通して **TypeScript の原位置**をここへ入れる（要件 9.1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// どのマクロの位置か。
    pub macro_name: MacroName,
    /// 関数名（無名の位置では `None`）。
    pub function: Option<String>,
    /// 行（1 起点。TypeScript の原位置）。
    pub line: u32,
    /// 列（1 起点。TypeScript の原位置）。
    pub column: u32,
}

impl Frame {
    /// 原位置（1 起点）から 1 段を組み立てる。
    pub fn at(macro_name: MacroName, function: Option<String>, line: u32, column: u32) -> Self {
        Self {
            macro_name,
            function,
            line,
            column,
        }
    }
}

/// 失敗の種別（design.md「Error Handling」の 4 層。要件 3.4, 8.3, 9.2）。
///
/// どの層で失敗したかによって提示と対処が変わるため、判別可能な列挙体とする。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureKind {
    /// ソースの解釈（種別とソースの不一致・能力宣言の誤り）。**実行そのものを行わない**（タスク 1.6）。
    Source,
    /// 変換（TypeScript の構文誤り。行と列は [`Frame`] が持つ。要件 3.4）。
    Transpile,
    /// 実行（マクロが投げた例外。要件 9.1）。
    Execution,
    /// ホスト API の拒否（能力の宣言漏れ・存在しない行や列・読み取り専用の要求。要件 8.3, 9.2）。
    HostRejected {
        /// 拒んだ API の名前（要件 9.2）。宣言表（タスク 2.1）の名前を入れる。
        api: String,
    },
}

impl FailureKind {
    /// 診断の記録に使う安定トークン（ロケール依存なし）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Transpile => "transpile",
            Self::Execution => "execution",
            Self::HostRejected { .. } => "host_rejected",
        }
    }
}

/// マクロの失敗（design.md「Components and Interfaces」の `MacroFailure`。要件 9.1, 9.3）。
///
/// 提示（4.4 の面）は種別ごとに変え、フレームは**内側（投げた位置）から外側へ**並べる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroFailure {
    /// どの層で失敗したか。
    pub kind: FailureKind,
    /// 失敗の理由。例外のメッセージそのもの、または拒否の理由（要件 8.3 の能力の名前を含む）。
    pub message: String,
    /// 失敗に至る呼び出しの並び。**内側から外側へ**（要件 9.3）。
    pub frames: Vec<Frame>,
}

impl MacroFailure {
    /// 失敗を組み立てる。
    pub fn new(kind: FailureKind, message: impl Into<String>, frames: Vec<Frame>) -> Self {
        Self {
            kind,
            message: message.into(),
            frames,
        }
    }

    /// 最も内側の段（投げた位置）。提示する行と列はこれを読む（要件 9.1）。
    pub fn innermost(&self) -> Option<&Frame> {
        self.frames.first()
    }
}

/// `console` の呼び出しの種別（要件 2.3）。
///
/// どの呼び出しだったかで提示の重みが変わる（`error` は警告として見せる等）ため、種別を持つ。
/// 宣言表の外にある `console` の取り込み（タスク 3.2）がこの値を決める。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputLevel {
    /// `console.log`。
    Log,
    /// `console.info`。
    Info,
    /// `console.warn`。
    Warn,
    /// `console.error`。
    Error,
    /// `console.debug`。
    Debug,
}

impl OutputLevel {
    /// 診断の記録に使う安定トークン（ロケール依存なし）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Debug => "debug",
        }
    }
}

/// マクロが出した出力の 1 行（design.md「Components and Interfaces」の `OutputLine`。要件 2.3）。
///
/// **アプリの標準出力へ漏らさず**ホストへ戻す（タスク 3.2）ための運搬の形である。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLine {
    /// 呼び出しの種別。
    pub level: OutputLevel,
    /// 出力の本文。
    pub text: String,
}

impl OutputLine {
    /// 出力の 1 行を組み立てる。
    pub fn new(level: OutputLevel, text: impl Into<String>) -> Self {
        Self {
            level,
            text: text.into(),
        }
    }
}

/// 変更の件数（種別ごと。design.md「Components and Interfaces」の `ChangeSummary`。要件 5.5）。
///
/// **集約そのもの**（未適用の変更集合と、読みの重ね合わせ）はタスク 2.3 が `host/changes.rs`
/// に持つ。ここにあるのは**実行の結果として提示する件数**である（要件 5.5 の「書き込みの
/// 合計」、要件 2.6 の「変更の有無」）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeSummary {
    /// セルへ書き込んだ件数。
    pub set_cells: usize,
    /// 追加した行数。
    pub inserted_rows: usize,
    /// 削除した行数。
    pub removed_rows: usize,
    /// 複製した行数。
    pub duplicated_rows: usize,
}

impl ChangeSummary {
    /// 変更が 1 件も無いか（要件 2.6）。
    pub const fn is_empty(&self) -> bool {
        self.set_cells == 0
            && self.inserted_rows == 0
            && self.removed_rows == 0
            && self.duplicated_rows == 0
    }

    /// 変更の合計（要件 5.5）。
    pub const fn total(&self) -> usize {
        self.set_cells + self.inserted_rows + self.removed_rows + self.duplicated_rows
    }
}

/// 実行の対象となるウィンドウのラベル（design.md「Components and Interfaces」の `RunRequest`）。
///
/// `app-shell` の IPC 境界の `WindowLabel`（`crates/app-shell/src/ipc/mod.rs`）とは**別の型**
/// である。本クレートは `app-shell` に依存しない（依存の向き。design.md「Allowed
/// Dependencies」）ため、アダプタが境界の型から写して渡す。エンジンはこの値をホストの
/// 縫い目（`HostPort`）と診断の記録へ渡すだけで、**文書を引くのはアダプタの仕事**である。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowLabel(String);

impl WindowLabel {
    /// ラベルを組み立てる。
    pub fn new(label: impl Into<String>) -> Self {
        Self(label.into())
    }

    /// ラベルの文字列。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for WindowLabel {
    fn from(label: &str) -> Self {
        Self(label.to_owned())
    }
}

impl From<String> for WindowLabel {
    fn from(label: String) -> Self {
        Self(label)
    }
}

impl fmt::Display for WindowLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 実行の要求（design.md「Components and Interfaces」の Service Interface。要件 2.1, 6.5）。
///
/// アダプタが記録（文書から読んだマクロ）と上限（設定から解決した値）と対象ウィンドウを
/// 解決して渡す。エンジンはこれを受けて actor へ渡し、**文書へは触らない**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    /// 実行するマクロの記録。ソースは**保存されたバイト列のまま**（要件 1.5）。
    pub record: MacroRecord,
    /// 時間とメモリの上限（要件 6.5）。
    pub limits: Limits,
    /// どのウィンドウのドキュメントに対して実行するか。
    pub window: WindowLabel,
}

impl RunRequest {
    /// 実行の要求を組み立てる。
    pub fn new(record: MacroRecord, limits: Limits, window: WindowLabel) -> Self {
        Self {
            record,
            limits,
            window,
        }
    }
}

/// 実行がどう終わったか（design.md「Components and Interfaces」の `RunOutcome`）。
///
/// `Ran` / `Failed` / `Aborted` の 3 値であり、**打ち切りは失敗の一種ではなく別の値**である
/// （要件 6.1 / 6.2 の提示が失敗と異なる。design.md「Domain Model」）。`Failed` / `Aborted` の
/// とき集めた変更は**適用されない**（アダプタが適用しない。要件 6.3 / 7.3）ため、この型は
/// **変更の件数を `Ran` でしか運ばない**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// 最後まで走り切った（要件 2.3, 2.5）。
    Ran {
        /// 戻り値の提示用の表現（**オブジェクトは JSON**。要件 2.3）。
        value: String,
        /// `console` の出力（**並びを保つ**。要件 2.3）。
        output: Vec<OutputLine>,
        /// 変更の件数（種別ごと。要件 5.5）。
        changes: ChangeSummary,
        /// 実行の所要（ミリ秒。要件 11.3）。
        elapsed_ms: u64,
    },
    /// 失敗して終わった（要件 2.4, 9.1）。
    Failed {
        /// 理由・種別・フレーム。
        failure: MacroFailure,
    },
    /// 上限で打ち切られた（要件 6.1, 6.2）。
    Aborted {
        /// どちらの上限に当たったか。
        limit: LimitKind,
        /// 打ち切りまでの所要（ミリ秒）。
        elapsed_ms: u64,
        /// 打ち切りの理由とフレーム。
        failure: MacroFailure,
    },
}

impl RunOutcome {
    /// 診断の記録に使う安定トークン（要件 2.6, 4.3。ロケール依存なし）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Ran { .. } => "ran",
            Self::Failed { .. } => "failed",
            Self::Aborted { .. } => "aborted",
        }
    }

    /// 打ち切りの種類（打ち切りでなければ `None`。要件 6.1, 6.2）。
    pub fn limit(&self) -> Option<LimitKind> {
        match self {
            Self::Aborted { limit, .. } => Some(*limit),
            Self::Ran { .. } | Self::Failed { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::record::MacroKind;

    /// 記録を組み立てる（ソースの中身はこの層の関心ではない）。
    fn record(name: &str) -> MacroRecord {
        MacroRecord::new(
            MacroName::from(name),
            MacroKind::TypeScript,
            "export default 1;",
        )
    }

    /// 実行時の失敗を組み立てる。
    fn execution_failure(frames: Vec<Frame>) -> MacroFailure {
        MacroFailure::new(
            FailureKind::Execution,
            "TypeError: undefined は関数ではありません",
            frames,
        )
    }

    /// 3 値が型で区別され、打ち切りの種類が結果から読める（要件 2.4, 6.1, 6.2）。
    ///
    /// 打ち切りは**同じ失敗の内容を持っていても失敗と同じ値にはならない**（要件 6.1 / 6.2 の
    /// 提示が失敗と異なるため。design.md「Domain Model」）。
    #[test]
    fn 三値は型で区別され打ち切りの種類が結果から読める() {
        let ran = RunOutcome::Ran {
            value: "42".to_owned(),
            output: vec![OutputLine::new(OutputLevel::Log, "集計を始めます")],
            changes: ChangeSummary {
                set_cells: 1,
                ..ChangeSummary::default()
            },
            elapsed_ms: 12,
        };
        let failed = RunOutcome::Failed {
            failure: execution_failure(Vec::new()),
        };

        assert_eq!(ran.as_str(), "ran");
        assert_eq!(failed.as_str(), "failed");
        // 成功と失敗は打ち切りではない（種類を運ばない）
        assert_eq!(ran.limit(), None);
        assert_eq!(failed.limit(), None);

        for (kind, token) in [(LimitKind::Time, "time"), (LimitKind::Memory, "memory")] {
            let aborted = RunOutcome::Aborted {
                limit: kind,
                elapsed_ms: 30_000,
                failure: execution_failure(Vec::new()),
            };
            assert_eq!(aborted.as_str(), "aborted");
            assert_ne!(aborted.as_str(), failed.as_str());
            assert_eq!(aborted.limit(), Some(kind));
            assert_eq!(aborted.limit().map(|kind| kind.as_str()), Some(token));
        }
    }

    /// フレームは内側（投げた位置）から外側へ並び、TypeScript の原位置を 1 起点で指す
    /// （要件 9.1, 9.3）。
    #[test]
    fn 失敗のフレームは内側から外側の順で原位置を指す() {
        let name = MacroName::from("在庫集計");
        let failure = execution_failure(vec![
            Frame::at(name.clone(), Some("集計".to_owned()), 12, 5),
            Frame::at(name.clone(), None, 40, 1),
        ]);

        let innermost = failure.innermost().expect("投げた位置のフレームがある");
        assert_eq!(innermost.macro_name.as_str(), "在庫集計");
        assert_eq!(innermost.function.as_deref(), Some("集計"));
        assert_eq!((innermost.line, innermost.column), (12, 5));

        // 2 段目は外側（呼び出し元）であり、並べ替えない
        assert_eq!(failure.frames.len(), 2);
        let outer = &failure.frames[1];
        assert_eq!(
            (outer.line, outer.column, outer.function.as_deref()),
            (40, 1, None)
        );
    }

    /// ホスト API の拒否は、拒んだ API の名前を種別に持つ（要件 8.3, 9.2）。
    #[test]
    fn ホストの拒否は拒んだ呼び出しの名前を種別に持つ() {
        let failure = MacroFailure::new(
            FailureKind::HostRejected {
                api: "file_read".to_owned(),
            },
            "能力 file.read が宣言されていない",
            vec![Frame::at(MacroName::from("在庫集計"), None, 3, 1)],
        );

        match &failure.kind {
            FailureKind::HostRejected { api } => assert_eq!(api, "file_read"),
            other => panic!("ホスト API の拒否として区別される: {other:?}"),
        }
        assert_eq!(failure.kind.as_str(), "host_rejected");
    }

    /// 変更の件数は種別ごとに数えられ、合計できる（要件 5.5, 2.6）。
    #[test]
    fn 変更の件数は種別ごとに数えられ合計できる() {
        let none = ChangeSummary::default();
        assert!(none.is_empty());
        assert_eq!(none.total(), 0);

        let changes = ChangeSummary {
            set_cells: 3,
            inserted_rows: 2,
            removed_rows: 1,
            duplicated_rows: 4,
        };
        assert!(!changes.is_empty());
        assert_eq!(changes.total(), 10);
    }

    /// 上限の既定は要件の値である（時間 30 秒 / メモリ 512 MB。要件 6.1, 6.2, 6.5）。
    #[test]
    fn 上限の既定は時間30秒メモリ512メガバイト() {
        let limits = Limits::default();
        assert_eq!(limits.time, Duration::from_secs(30));
        assert_eq!(limits.memory_bytes, 512 * 1024 * 1024);
    }

    /// 実行の要求は記録・上限・対象ウィンドウを運ぶ（要件 2.1, 6.5）。
    #[test]
    fn 実行の要求は記録と上限と対象ウィンドウを運ぶ() {
        let request = RunRequest::new(
            record("在庫集計"),
            Limits::new(Duration::from_secs(5), 64 * 1024 * 1024),
            WindowLabel::from("main"),
        );

        assert_eq!(request.record.name.as_str(), "在庫集計");
        assert_eq!(request.record.source, "export default 1;");
        assert_eq!(request.limits.time, Duration::from_secs(5));
        assert_eq!(request.window.as_str(), "main");
    }
}

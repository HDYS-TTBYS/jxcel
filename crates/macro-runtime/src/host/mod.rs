//! ホストの縫い目の層（design.md「File Structure Plan」の `host/`。要件 4.1–4.6, 5.1–5.4）。
//!
//! マクロがドキュメントへ触る経路の**エンジン側の半分**を持つ層である。エンジンは文書を
//! 知らない — 読みは縫い目（`HostPort`）越しに行い、書きは未適用の変更集合として持ち、
//! 文書へ適用するのはアダプタ（`src-tauri`）である（design.md 決定 2 / 3）。この層が持つのは
//! **値の写像**（[`value`]）・**未適用の変更集合**（[`changes`]）・**読みの重ね合わせ**
//! （`overlay`）と、その縫い目 `HostPort` である。
//!
//! # 層の鎖（design.md「File Structure Plan」）
//!
//! `error / source → surface → host → engine → types → api`。本層は鎖の 4 番目であり、
//! `source` / `surface` と上流の 2 クレート（`document-format` / `schema-engine`）を参照する。
//! `engine` は本層を参照する側であり、本層は `engine` を知らない（値の写像が V8 に依存しない
//! ことは、実装ではなく依存の向きで保たれている）。
//!
//! # いまあるもの（tasks.md 2.2 / 2.3 / 2.4 / 3.2）
//!
//! | モジュール | 責務 | 担当 |
//! |---|---|---|
//! | [`value`] | schema / document の値 ⇄ JavaScript の値の写像と対応表 | 2.2 |
//! | [`changes`] | 未適用の変更集合（セルの書き込み・行の追加・削除・複製）と件数・適用の順序 | 2.3 |
//! | [`overlay`] | 自分の書き込みを読む重ね合わせと範囲の読み | 2.4 |
//! | [`HostPort`] | ホストの実装の縫い目（アダプタが実装する。定義は本モジュール） | 3.2（定義）/ 4.1（実装） |
//!
//! # 縫い目（[`HostPort`]）を本モジュールが持つ理由
//!
//! 設計の File Structure Plan が `host/mod.rs` の役割を「ホストの実装の縫い目（読み・書き・
//! 出力・能力の口）」と定めているためである。エンジン（`engine/isolate.rs`。タスク 3.2）は
//! この trait 越しにだけ文書へ触り、**実装を知らない**（design.md 決定 2 / 3）。実装は
//! アダプタ（`src-tauri/src/macro_host.rs`。タスク 4.1）が持ち、実行 1 回ぶんの実体
//! （＝変更集合 1 つ）を差し込む。

pub mod changes;
pub mod overlay;
pub mod value;

use document_format::SheetId;

use crate::host::changes::{Change, ChangeSet};
use crate::host::overlay::{ColumnTypeInfo, RowPage, RowSpan, SheetInfo};

/// ホストの縫い目が呼び出しを拒んだ理由（design.md「Service Interface」の `HostError`）。
///
/// **表示の文言ではなく、アダプタが組み立てた 1 行の理由**である（`host/changes.rs` の
/// `ChangeError` と同じ規律: エンジンは文書を知らないので、どの行・どの列が問題かを
/// 名指しできるのは文書を引ける側だけである）。エンジンはこの理由を
/// [`MacroFailure::message`](crate::engine::outcome::MacroFailure) へそのまま入れ、
/// 種別を [`FailureKind::HostRejected`](crate::engine::outcome::FailureKind) にする
/// （要件 5.4, 8.3, 9.2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    /// 拒んだ理由（アダプタが組み立てる）。
    reason: String,
}

impl HostError {
    /// 理由つきで拒否を組み立てる。
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// 拒んだ理由。
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl core::fmt::Display for HostError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for HostError {}

/// ホストの実装の縫い目（design.md「Components and Interfaces」の `HostPort`）。
///
/// マクロ 1 回の実行について、**読み**（[`HostPort::sheets`] / [`HostPort::columns`] /
/// [`HostPort::read_rows`]）と**書きの集約**（[`HostPort::stage`]）、**能力を要する口**
/// （[`HostPort::file_read`] / [`HostPort::file_write`] / [`HostPort::net_fetch`]）を提供する。
/// 文書へ適用するのはアダプタであり、この trait は**適用しない**
/// （design.md 決定 2 / 3。`src-tauri/src/macro_apply.rs`。タスク 4.2）。
///
/// # 誰が実装し、誰が呼ぶか
///
/// 実装はアダプタ（タスク 4.1）。呼ぶのはエンジンの op（`engine/isolate.rs`）であり、
/// **門（`surface/gate.rs` の `check_call`）を通った呼び出しだけ**がここへ届く
/// （要件 8.1）。アダプタは実行 1 回につき 1 つの実体を作って
/// [`MacroActor::run`](crate::engine::actor::MacroActor::run) へ渡す — 変更集合は実行 1 回の
/// トランザクション境界であり（`host/changes.rs` の doc）、実行が終われば捨てられる。
///
/// # スレッドを越える（`Send + Sync` を要する理由）
///
/// isolate は専用スレッドが所有し、**縫い目はそのスレッドへ渡って使われる**（design.md 決定 1。
/// `engine/actor.rs`）。したがって実体は `Arc<dyn HostPort>` として実行の要求に載り、
/// `Send + Sync` でなければならない（設計の `pub trait HostPort { … }` に付いている束縛である）。
/// **`&self` で変更を集める**（[`HostPort::stage`]）ため、変更集合は実装の内側で
/// 内部可変性（`Mutex` 等）に置かれる。
///
/// # 設計から動かした 2 点（理由つき）
///
/// 1. **`overlay(&self) -> &ChangeSet` は置かない。** 重ね合わせ（`host/overlay.rs` の
///    [`Overlay`](crate::host::overlay::Overlay)）は [`HostPort::read_rows`] の**実装の内側の
///    道具**である（設計の doc も「`read_rows` の内側で使う」と書く）。trait には読みの入口
///    だけを置く。加えて、`stage(&self, …)` が `&self` で変更を集める以上、変更集合は
///    ロックの内側にあり、`&ChangeSet` を外へ返すことはできない（設計の 2 つの要求が
///    両立しない）。エンジンが要るものだけを [`HostPort::with_changes`] で渡す
/// 2. **`emit(&self, line: OutputLine)` は置かない。** 出力の運搬の型（`OutputLine`）は
///    `engine/outcome.rs` にあり、本層は `engine` を参照できない（層の鎖）。エンジンが実行
///    1 回ぶんの出力を集め、`RunOutcome::Ran.output` でホストへ戻す（要件 2.3。アプリの
///    標準出力へは漏らさない）
pub trait HostPort: Send + Sync {
    /// シートの一覧（要件 4.1）。行数は**文書の行数**であり、重ね合わせを見ない
    /// （`host/overlay.rs` の規則 3）。
    fn sheets(&self) -> Result<Vec<SheetInfo>, HostError>;

    /// 列の宣言（名前・型情報。要件 4.1, 4.2）。列の並びは行のセルの並びと同じ順である。
    ///
    /// **書き換えの口は無い**（列の宣言は読み取り専用。要件 4.5）。書き換えようとした
    /// マクロは、そもそも宣言表にその API が無いため門が名前つきで拒む。
    fn columns(&self, sheet: SheetId) -> Result<Vec<ColumnTypeInfo>, HostError>;

    /// 行の範囲の読み（要件 4.4）。**1 回の呼び出しで範囲の全部を返す**（行ごとの呼び出しを
    /// 強いない）。読みは**自分の書き込みの重ね合わせ**を見る（要件 5.1 の裏面）。
    fn read_rows(&self, sheet: SheetId, span: RowSpan) -> Result<RowPage, HostError>;

    /// 1 件の変更を集める（**適用はしない**。design.md「Postconditions」）。
    ///
    /// 文書に元から無い行と範囲外の列は、文書を引ける側（実装）がここで理由つきに拒む
    /// （`host/changes.rs` の「存在しない行を拒むのは誰か」。要件 5.4）。
    fn stage(&self, change: Change) -> Result<(), HostError>;

    /// この実行で集めた変更（読み取り専用）を、**借用のまま**読ませる。
    ///
    /// エンジンが読むのは件数である（`ChangeSet::cell_count` たち →
    /// [`ChangeSummary`](crate::engine::outcome::ChangeSummary)。要件 5.5）。件数の型が
    /// `engine` にあるため本層はそれを返せない（層の鎖）ので、数える口を貸す形にしている。
    /// **変更集合を複製しない** — 10 万行の書き換えでも写像を丸ごと写さないためである
    /// （要件 11.3）。
    fn with_changes(&self, read: &mut dyn FnMut(&ChangeSet));

    /// ファイルを読む（要件 8.1）。**門を通った呼び出しだけ**がここへ届く（宣言が無ければ
    /// エンジンが呼び出しの前に拒む。要件 8.3）。
    fn file_read(&self, path: &str) -> Result<String, HostError>;

    /// ファイルへ書く（要件 8.1。門は [`HostPort::file_read`] と同じ）。
    fn file_write(&self, path: &str, text: &str) -> Result<(), HostError>;

    /// URL を取る（要件 8.1。門は [`HostPort::file_read`] と同じ）。
    fn net_fetch(&self, url: &str) -> Result<String, HostError>;
}

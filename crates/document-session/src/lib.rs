//! jxcel ドキュメントセッション: ウィンドウ単位のドキュメントの保持と変更の唯一の経路。
//!
//! # 層の鎖
//!
//! 本クレートのモジュールは次の鎖の順に並ぶ。**各層は自分の左にある層だけを参照する**
//! （右の層から左を呼ぶことはなく、逆向きの参照も無い。design.md「Architecture Pattern &
//! Boundary Map」・structure.md「ドメインクレートの内部構造」）。
//!
//! ```text
//! error / state → session → change → table → api
//! ```
//!
//! - `error` / `state` — 誤り（保持していない・別の操作が進行中・読み込みの失敗・保存の
//!   結果）と状態（出所・未保存・変更の版・シートの要約）を、判別可能な列挙体として定義する。
//!   表示用の文言は持たない（文言は適応層が組み立てる）。**鎖の最も左**であり、
//!   本クレートの他のどの層にも依存しない
//! - `session` — 1 つのセッションの状態機械。出所と保持する `Document` を 1 対で持ち、
//!   左の `error` / `state` だけを参照する
//! - `change` — 変更の適用口。文書の可変参照を閉包に貸し、**同じ臨界区間の内側で**未保存の
//!   印と版を記録する。左の `error` / `state` / `session` と、上流 `document-format` を参照する
//! - `table` — ウィンドウ → セッションの表。`session` までの左の層だけを参照する
//! - `api` — 公開面の組み立て（本ファイル）。`table` までの左の層だけを参照し、木に属する
//!   操作口（[`DocumentSessionsApi`]）とその具象実装（[`DocumentSessions`]）を置いて、
//!   下位の層の型を根の再輸出へ集める
//!
//! 本ファイル（`lib.rs`）は鎖の最右に当たり、再輸出と公開面の型だけを置く。
//!
//! # 依存方針の要約
//!
//! - **`tauri` に依存しない**（推移的依存も含む）。開いたドキュメントの保持と変更の適用は
//!   GUI を起動せずにテストできなければならない。機械検査は `scripts/check-core-deps.sh`
//!   が引数なしで `crates/*/Cargo.toml` を全列挙して行う
//! - 依存してよい兄弟は `document-format`（文書とその読み書き）と `app-shell`
//!   （ウィンドウ識別子 `WindowLabel` の単一の定義）の 2 つだけである
//! - **行の識別子・列の添字・位置（`PathBuf`）を境界へ出さない**。境界型を置けるのは
//!   `crates/app-shell/src/ipc/` の下だけであり、本クレートは境界の型を持たない
//!
//! 具体的な宣言とその理由は `Cargo.toml` の冒頭コメントを参照。
//!
//! # 現状
//!
//! タスク 1.3 が鎖の最左の 2 層（`error` / `state`）を定義した。`error` は
//! [`SessionError`]（保持していない・別の操作が進行中・未保存のため差し替えできない・
//! 読み込みに失敗した）と [`SaveReport`]（保存した / 保存先が要る / 取り消した / 失敗した）、
//! `state` は [`SessionState`] / [`Origin`] / [`SheetSummary`] / [`CloseAnswer`] / [`Edited`] を
//! 定義する。いずれも表示用の文言を持たない（文言は適応層が組み立てる）。この 2 層は
//! **本クレートの他のどの層にも依存しない**（`std` と `document-format`、`thiserror` だけを
//! 使う）。
//!
//! タスク 2.1 が `session` を定義した（`Slot` の状態機械）。1 つのセッションが出所と文書を
//! 1 対で持ち、`resolve` は起動時に指定された位置を**1 度だけ**読み込み（冪等）、`attach` は
//! 利用者が選んだ位置を**読み直して出所を更新**し（未保存なら拒否する）、`create` は行も列も
//! 無いシートを 1 つ持つ文書を未保存でない状態から用意する。読み取り・可変の借用・破棄の印は
//! 未解決のセッションに対して失敗し（`NoDocument`）、読み込み・引き渡し・新規作成は進行中の
//! 操作を待たずに失敗する（`Busy`）。未保存と変更の版は文書のロックの外の原子値であり、
//! `may_close` はロックを待たない。**版は文書が入れ替わるか変更が適用されたときに 1 進む**
//! （読み込みの完了・新規作成・変更の適用）。未保存の判定と、文書の差し替え・印・版の更新は
//! 文書のロックを保持したまま行う。`Slot` は crate 可視であり、根の再輸出には現れない
//! （公開面はタスク 2.5 が組み立てる）。タスク 2.2 が `Slot` に保存の 2 経路を足した:
//! `save` は出所へ、`save_to` は選ばれた位置へ書き出して以後の出所にする。どちらも
//! **文書のロックの下で 1 パス**として書き出し（進行中の適用は保存の完了を待つ）、成功したら
//! ロックを保持したまま未保存の印を落とし、失敗したら印を保つ。出所が無ければ書き出さずに
//! `SaveReport::NeedsLocation` を返す。**保存は内容を変えないため版は進めない**。
//!
//! タスク 2.3 が `table` を足した（`Sessions`）。1 つの `WindowLabel` につき `Slot` は高々
//! 1 つであり、`slot` は同じ窓に**同じ実体**を返し（挿入は書きロックの下で二重に確認する）、
//! `existing` は作らずに参照し、`forget` はその窓のセッションだけを手放す（他の窓の文書と
//! 未保存の状態を変えない）。表のロックは**参照と挿入・除去のためだけ**に取り、`Slot` の
//! 処理の間は保持しない（10 万行の適用が他のウィンドウを待たせない。要件 3.6）。`Sessions`
//! も crate 可視であり、根の再輸出には現れない。
//!
//! タスク 2.4 が `change` を足した（変更の適用の唯一の口）。`edit` は文書のロックを**保持した
//! まま**閉包へ可変参照を渡し、**同じ臨界区間の内側で**未保存の印を立てて版を 1 進める
//! （判定・閉包・記録を分けない。2.3 のレビューで見つかった TOCTOU を作り込まない）。
//! 閉包が失敗を返しても印は立てる（閉包が文書を変えたかを判定できないため保守側に倒す）。
//! 変更の語彙は持たず（何をどう変えるかは要求側の所有である）、`document-format` の
//! `set_cells` などを呼ぶのは閉包の側である。未解決・保持なしのセッションでは閉包を呼ばずに
//! `NoDocument` を返す。閉包の内側からセッションを呼び返すことは禁じる（再入禁止。doc に
//! 明記）。`change` は crate 可視であり、根の再輸出には現れない。
//!
//! タスク 2.5 が鎖の最右（`api`）を確定させた。本ファイルに、design.md「core →
//! DocumentSessions（公開面）」の逐語の 11 のメソッドを持つ [`DocumentSessionsApi`] と、その
//! 具象実装 [`DocumentSessions`] を置く（`document-format` の `DocumentFormatApi` /
//! `DocumentFormat`、`schema-engine` の `SchemaEngineApi` / `SchemaEngine` と同じ形。
//! 本型は表を保持するため無状態ではない）。各メソッドは**ウィンドウのスロットを得て、その上の
//! 操作へ委譲する**だけである。**セッションを作るのは `resolve` / `attach` / `create` の 3 つ**
//! であり（design.md 同節の Preconditions）、`read` / `edit` / `save` / `save_to` / `discard` は
//! 未解決のウィンドウへ `NoDocument` を返し、`state` / `may_close` / `forget` は表から**作らずに**
//! 答える（`table::Sessions::existing`）。記録しない可変の貸出口（`Slot::with_document_mut`）は
//! production の可変の経路を [`DocumentSessionsApi::edit`] ただ 1 つに固定するため
//! `#[cfg(test)]` へ閉じた。**表・スロット・内部状態・Guard は根へ出さない**（境界の型は適応層が
//! 作る）。層のモジュールは `pub mod` のままであるが、下流が使うのは根に並べた名前だけである。
//!
//! フィーチャーフラグは使わない（`error` / `state` / `session` / `change` / `table` / `api` の
//! いずれも型と状態機械・表・適用口の定義であり、OFF の構成に意味が無い）。

pub mod change;
pub mod error;
pub mod session;
pub mod state;
pub mod table;

// テストとベンチが共有する標本の生成器（tasks.md 1.4）を、本クレートの単体テストからも使える
// ようにする。共有モジュールは `tests/` 配下にあり、単体テスト（`lib` クレートの内側）からは
// `mod common;` では参照できない（`tests/` 配下のモジュールは統合テストという別のクレート
// ルートの持ち物である）。クレート内の**すべての**単体テストから 1 箇所の宣言で使えるよう、
// `lib.rs` に `#[path = "../tests/common/mod.rs"]` を置いて取り込む（`#[path]` は本ファイルの
// ある `src/` を基準に解決される。Rust Reference「The path attribute」）。`include!` は使えない:
// 1.4 のモジュール冒頭の内側属性（`#![allow(dead_code)]`）と内側 doc コメント（`//!`）が展開
// 位置では許されず、コンパイルエラーになる（実測）。`#[cfg(test)]` であるため公開面にも配布物に
// も現れない（`benches/` は同じファイルを `#[path = "../tests/common/mod.rs"]` で指している）。
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod common;

// 下流（`src-tauri` の適応層と、その先の他クレート）は**根の名前だけを使う**
// （design.md「Architecture Integration」の「公開面は根の再輸出に集める」）。下位モジュールは
// `pub mod` のまま公開されるが、利用側が `document_session::error::...` のような層のパスを
// 直接綴ると、層の構成を変えたときに下流が壊れる。根に並べた名前を唯一の入口とする。
pub use error::{SaveReport, SessionError};
pub use state::{CloseAnswer, Edited, Origin, SessionState, SheetSummary};

use std::path::Path;
use std::sync::Arc;

use app_shell::ipc::WindowLabel;
use document_format::Document;

use crate::session::Slot;
use crate::table::Sessions;

/// 木に属する操作口（design.md「core → DocumentSessions（公開面）」の逐語）。
///
/// 本クレートの**唯一の入口**である。下流（`src-tauri` の適応層と、その先の他クレート）は
/// 根の再輸出に並べた名前だけを使い、層のモジュールのパスを綴らない。11 のメソッドが
/// design.md の Service Interface の 11 と 1 対 1 に対応する。
///
/// # セッションを作る 3 つの入口
///
/// [`DocumentSessionsApi::resolve`]（起動時に指定された位置）・
/// [`DocumentSessionsApi::attach`]（利用者が選んだ位置の引き渡し）・
/// [`DocumentSessionsApi::create`]（新規作成）**だけ**がセッションを作る。他のメソッドは
/// セッションを作らない: `read` / `edit` / `save` / `save_to` / `discard` は未解決の
/// ウィンドウへ [`SessionError::NoDocument`] を返し、`state` / `may_close` は
/// [`SessionState::Absent`] / [`CloseAnswer::Allow`] を返し、`forget` は何もしない。コアは
/// `WindowRegistry` を知らないため、生成要求（起動時に指定された位置）を読めるのは呼び出し元
/// だけであり、この一意性が**破棄の購読を伴わないセッションが生まれる経路**を塞ぐ
/// （design.md 同節の Preconditions。要件 1.4）。
///
/// # オブジェクト安全ではない
///
/// [`DocumentSessionsApi::read`] と [`DocumentSessionsApi::edit`] は**ジェネリック**であり
/// （閉包が返す型 `R` をそのまま返す）、トレイトオブジェクト（`dyn DocumentSessionsApi`）には
/// できない。したがって適応層は**具象型 [`DocumentSessions`] を `Arc` で 1 実体だけ保持**して
/// 共有する（design.md 同節の Implementation Notes）。
///
/// # 変更の語彙を持たない
///
/// [`DocumentSessionsApi::edit`] は `&mut dyn FnMut(&mut Document) -> R` を受けて
/// [`Edited<R>`] を返す。何をどう変えるか（上流 `document-format` の一括の書き換え
/// `set_cells` を閉包の内側で呼ぶことを含む）は**要求側の所有**であり、本クレートは適用の口と、
/// 適用されたことの記録だけを持つ（design.md「ChangeApply」）。閉包の内側から同じセッションを
/// 呼び返してはならない（再入禁止。`change` の doc）。
pub trait DocumentSessionsApi {
    /// ウィンドウの生成要求（あれば）を渡してセッションを確定させる。**冪等**である
    /// （解決済みのウィンドウへの 2 度目のアクセスは読み込みを起こさない）。
    ///
    /// `requested` が `None` のときは読み込む対象が無いため何もしない。読み込みに失敗した
    /// ときは状態を `Unavailable` として覚え、理由を [`SessionError::Read`] で返す
    /// （保持していた内容は変えない。要件 2.1）。
    fn resolve(&self, window: &WindowLabel, requested: Option<&Path>) -> Result<(), SessionError>;

    /// **利用者が選んだ位置**をそのウィンドウへ読み込む（design.md 同節。要件 1.3, 2.2, 2.4）。
    ///
    /// 未保存の変更があれば [`SessionError::UnsavedChanges`] を返して拒否し、読み込みに
    /// 失敗しても保持している文書を変えない。起動時の解決と違い、`Resolved` / `Unavailable` の
    /// どちらからでも**読み直して出所を更新する**（利用者の明示の操作であるため、覚えている
    /// 失敗を繰り返さない）。
    fn attach(&self, window: &WindowLabel, location: &Path) -> Result<(), SessionError>;

    /// 保持している文書を読む。**未解決なら [`SessionError::NoDocument`]**（セッションを
    /// 作るのは上記の 3 つの入口だけである）。
    ///
    /// 適用中に到着した読み取りは適用の完了を待ち、**適用済みの最新**を返す（要件 3.4）。
    /// 閉包の内側から同じセッションを呼び返してはならない（再入禁止）。
    fn read<R>(
        &self,
        window: &WindowLabel,
        f: &mut dyn FnMut(&Document) -> R,
    ) -> Result<R, SessionError>;

    /// 変更を適用し、閉包の戻り値・版・未保存を返す（design.md「ChangeApply」。要件 3.1〜3.4,
    /// 4.1, 4.2）。
    ///
    /// 1 回の閉包が 1 回の適用であり、版はちょうど 1 進み、未保存の印が立つ。**閉包が失敗を
    /// 返しても印は立つ**（閉包が文書を変えたかを判定できないため保守側に倒す）。未解決なら
    /// [`SessionError::NoDocument`] を返し、**閉包を呼ばない**。閉包の内側から同じセッションを
    /// 呼び返してはならない（再入禁止）。
    fn edit<R>(
        &self,
        window: &WindowLabel,
        f: &mut dyn FnMut(&mut Document) -> R,
    ) -> Result<Edited<R>, SessionError>;

    /// 状態の写しを返す（要件 1.6, 1.7, 4.3）。**セッションを作らない**: 未解決のウィンドウには
    /// [`SessionState::Absent`] を返す。
    fn state(&self, window: &WindowLabel) -> SessionState;

    /// 出所へ保存する。出所が無ければ [`SaveReport::NeedsLocation`] を返す（保存先の提示は
    /// 適応層の仕事である。要件 5.1, 5.2）。
    fn save(&self, window: &WindowLabel) -> Result<SaveReport, SessionError>;

    /// 選ばれた位置へ保存し、以後の出所にする（要件 5.2, 5.8）。
    fn save_to(&self, window: &WindowLabel, location: &Path) -> Result<SaveReport, SessionError>;

    /// 未保存の印を落とす（**保存しない**。利用者の明示の指示による。要件 6.5）。
    fn discard(&self, window: &WindowLabel) -> Result<(), SessionError>;

    /// 行も列も無いシートを 1 つ持つドキュメントを用意する（未保存の変更があれば拒否する。
    /// 要件 7.1〜7.4）。
    fn create(&self, window: &WindowLabel) -> Result<(), SessionError>;

    /// 閉じてよいか。**ブロックしない**（未保存の原子値だけを読む。要件 2.5, 4.6）。
    /// **セッションを作らない**: 未解決のウィンドウには [`CloseAnswer::Allow`] を返す。
    fn may_close(&self, window: &WindowLabel) -> CloseAnswer;

    /// 破棄されたウィンドウのセッションを忘れる（他のウィンドウの文書と未保存の状態を
    /// 変えない。要件 1.5）。**セッションを作らない。**
    fn forget(&self, window: &WindowLabel);
}

/// 公開面の具象実装（design.md はトレイトだけを定めるため、利用側が値を持てるように本型を
/// 添える。`document-format` の `DocumentFormat` / `schema-engine` の `SchemaEngine` と
/// 同じ形である）。
///
/// 保持するのは**ウィンドウ → セッションの表 1 つ**である（1 つのウィンドウにつきセッションは
/// 高々 1 つ。要件 1.1）。表の型・スロット・内部状態・Guard は根へ出さない（下流が使うのは
/// 本型のメソッドだけであり、境界の型は適応層が作る）。
///
/// ```text
/// let sessions = DocumentSessions::new();
/// sessions.resolve(&window, Some(path))?;
/// let state = sessions.state(&window);
/// ```
///
/// 表を持つが `Send + Sync` である（適応層が `Arc` で 1 実体だけ保持し、ウィンドウごとの
/// コマンドから共有する。`tests/public_api.rs` がコンパイル時に表明する）。
pub struct DocumentSessions {
    /// ウィンドウ → セッションの表（`table` の層。ロックの規律は同モジュールの docs）。
    sessions: Sessions,
}

impl DocumentSessions {
    /// どのウィンドウもドキュメントを持たない状態の操作口を作る。
    pub fn new() -> Self {
        Self {
            sessions: Sessions::new(),
        }
    }

    /// そのウィンドウのセッションを**作らずに**参照し、無ければ
    /// [`SessionError::NoDocument`] を返す。
    ///
    /// 操作の口（[`DocumentSessionsApi`]）がセッションを作るのは `resolve` / `attach` /
    /// `create` の 3 つだけであり、それ以外はこの入口（`table::Sessions::existing`）を通る。
    fn session(&self, window: &WindowLabel) -> Result<Arc<Slot>, SessionError> {
        self.sessions
            .existing(window)
            .ok_or(SessionError::NoDocument)
    }
}

impl DocumentSessionsApi for DocumentSessions {
    fn resolve(&self, window: &WindowLabel, requested: Option<&Path>) -> Result<(), SessionError> {
        // セッションを作る入口の 1 つ（表の `slot` が未挿入なら挿入する）。
        self.sessions.slot(window).resolve(requested)
    }

    fn attach(&self, window: &WindowLabel, location: &Path) -> Result<(), SessionError> {
        // セッションを作る入口の 1 つ（design.md 同節の Preconditions）。
        self.sessions.slot(window).attach(location)
    }

    fn read<R>(
        &self,
        window: &WindowLabel,
        f: &mut dyn FnMut(&Document) -> R,
    ) -> Result<R, SessionError> {
        self.session(window)?.read(f)
    }

    fn edit<R>(
        &self,
        window: &WindowLabel,
        f: &mut dyn FnMut(&mut Document) -> R,
    ) -> Result<Edited<R>, SessionError> {
        let slot = self.session(window)?;
        change::edit(&slot, f)
    }

    fn state(&self, window: &WindowLabel) -> SessionState {
        // **`slot` ではなく `existing` を使う**（問い合わせがセッションを作らない。
        // 未解決のウィンドウの状態は `Absent` のままである）。
        match self.sessions.existing(window) {
            Some(slot) => slot.state(),
            None => SessionState::Absent,
        }
    }

    fn save(&self, window: &WindowLabel) -> Result<SaveReport, SessionError> {
        self.session(window)?.save()
    }

    fn save_to(&self, window: &WindowLabel, location: &Path) -> Result<SaveReport, SessionError> {
        self.session(window)?.save_to(location)
    }

    fn discard(&self, window: &WindowLabel) -> Result<(), SessionError> {
        self.session(window)?.discard()
    }

    fn create(&self, window: &WindowLabel) -> Result<(), SessionError> {
        // セッションを作る入口の 1 つ（要件 7.4）。
        self.sessions.slot(window).create()
    }

    fn may_close(&self, window: &WindowLabel) -> CloseAnswer {
        // **`existing`**: 未解決のウィンドウに読み込みを試みない（読み込みは秒単位かかり、
        // まだ何も変更されていない。design.md「SessionDocumentHost」）。
        match self.sessions.existing(window) {
            Some(slot) => slot.may_close(),
            None => CloseAnswer::Allow,
        }
    }

    fn forget(&self, window: &WindowLabel) {
        // 表から取り除くだけである（セッションを作らない。他のウィンドウに触れない）。
        self.sessions.forget(window);
    }
}

#[cfg(test)]
mod tests {
    // 本モジュールは crate の内側であるため、**公開面からは観測できない不変条件**を固定できる。
    // 対象は「セッションを作るのは `resolve` / `attach` / `create` の 3 つだけ」であり、その
    // 存在の有無を確かめられるのは表そのもの（私有フィールド `sessions` と crate 可視の
    // `Sessions::existing`）だけである。
    use super::*;

    /// セッションを作らない入口（`state` / `may_close` / `forget` / `read` / `edit` / `save` /
    /// `save_to` / `discard`）は、未解決のウィンドウを表へ挿入しない。
    ///
    /// **根の名前だけを使う統合テスト（`tests/public_api.rs`）ではこの不変条件を検出できない**:
    /// 挿入された未解決の `Slot` は `state` から [`SessionState::Absent`]、`read` などから
    /// [`SessionError::NoDocument`] として見え、**表に無いウィンドウと区別がつかない**
    /// （`Inner::Unresolved` の答えが不在の答えと同じである）。したがって、入口が
    /// `Sessions::slot` ではなく `Sessions::existing` を使うことを、表へ到達できるここで固定する
    /// （`state` を `slot` へ取り違える変異は統合テストを緑のまま通ってしまう）。
    #[test]
    fn entrances_that_do_not_create_a_session_never_insert_one() {
        let sessions = DocumentSessions::new();

        // 参照するだけの入口（状態・閉じてよいか・読み取り・変更・保存の 2 経路・破棄の印）を
        // 呼ぶ。戻り値はここでは関心の外である（存在の有無は表そのもので見る）。
        let probe = WindowLabel::new("probe");
        let _ = sessions.state(&probe);
        let _ = sessions.may_close(&probe);
        let _ = sessions.read(&probe, &mut |_document| ());
        let _ = sessions.edit(&probe, &mut |_document| ());
        let _ = sessions.save(&probe);
        let _ = sessions.save_to(&probe, Path::new("/tmp/probe-not-created.jxcel"));
        let _ = sessions.discard(&probe);

        assert!(
            sessions.sessions.existing(&probe).is_none(),
            "セッションを作らない入口が表へ挿入してはならない"
        );
        assert_eq!(sessions.state(&probe), SessionState::Absent);

        // `forget` は**取り除く**入口であるため、他の入口と同じ窓で呼ぶと、それ以前の入口が
        // 挿入していても証拠を消してしまう（上の観測が空振りする）。専用の窓で見る。
        let forgotten = WindowLabel::new("forgotten");
        sessions.forget(&forgotten);
        assert!(
            sessions.sessions.existing(&forgotten).is_none(),
            "forget が表へ挿入してはならない"
        );

        // 対照 1: 作る側の入口（読み込む対象が無い `resolve` でも表へ載る）を通せば挿入される。
        // この対照が無いと、テストが「常に `None` になる経路」だけを見ていても気づけない。
        sessions
            .resolve(&probe, None)
            .expect("読み込む対象が無いため何もしない");
        assert!(
            sessions.sessions.existing(&probe).is_some(),
            "セッションを作る入口は表へ挿入する"
        );

        // 対照 2: `forget` は挿入しないが、載っているセッションは取り除く（no-op ではない）。
        sessions.forget(&probe);
        assert!(
            sessions.sessions.existing(&probe).is_none(),
            "forget はその窓のセッションを取り除く"
        );
    }
}

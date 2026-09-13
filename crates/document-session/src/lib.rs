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
//! - `change` — 変更の適用口。文書の可変参照を閉包に貸し、未保存の印と版を記録する。
//!   左の `error` / `state` と、上流 `document-format` を参照する
//! - `table` — ウィンドウ → セッションの表。`session` までの左の層だけを参照する
//! - `api` — 公開面の組み立て。`table` までの左の層だけを参照し、根の再輸出へ集める
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
//! 使う）。残る `session` / `change` / `table` / `api` は後続タスク（2.x・3.x）が足す。
//!
//! フィーチャーフラグは使わない（型の定義であり、OFF の構成に意味が無い）。

pub mod error;
pub mod state;

// 下流（`src-tauri` の適応層と、その先の他クレート）は**根の名前だけを使う**
// （design.md「Architecture Integration」の「公開面は根の再輸出に集める」）。下位モジュールは
// `pub mod` のまま公開されるが、利用側が `document_session::error::...` のような層のパスを
// 直接綴ると、層の構成を変えたときに下流が壊れる。根に並べた名前を唯一の入口とする。
pub use error::{SaveReport, SessionError};
pub use state::{CloseAnswer, Edited, Origin, SessionState, SheetSummary};

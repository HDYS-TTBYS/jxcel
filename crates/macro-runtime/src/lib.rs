//! jxcel マクロ実行エンジン（Tauri 非依存のドメインクレート）。
//!
//! 利用者が書いた TypeScript / JavaScript のマクロを、**そのウィンドウが開いている
//! ドキュメントに対して**隔離して実行し、結果（戻り値・出力・変更の件数）と失敗
//! （理由・種別・フレーム）を返す（design.md「Overview」。要件 2.3, 2.4, 9.1–9.3）。
//! エンジンは**ドキュメントへ触らない** — 読みは縫い目（`HostPort`）越しに行い、書きは
//! 未適用の変更集合として持ち、文書へ適用するのはアダプタ（`src-tauri`）である
//! （design.md 決定 2 / 3）。
//!
//! # 依存の向き（design.md「Architecture Integration」「Allowed Dependencies」）
//!
//! `document-format → schema-engine → macro-runtime` の一方向である。本クレートは上流に
//! `document-format` と `schema-engine` だけを持ち、`tauri` には推移的にも依存しない
//! （CI の `sh scripts/check-core-deps.sh` が `cargo tree` 全体を走査して機械検査する）。
//! **`data-grid` / `document-session` も依存に持たない** — 変更の適用と、取り消し履歴の
//! 1 対へのまとめはアダプタの仕事である（design.md 決定 3）。
//!
//! # 層の鎖（design.md「File Structure Plan」）
//!
//! `error / source → surface → host → engine → types → api`。各層の `mod.rs` の冒頭に
//! この鎖を書き、左の層だけを参照する（`document-format` の
//! `Ids / Value / EntryName → Model → Json → Parts → Container → Api` と同じ規約）。
//! 本ファイルは鎖の最右（`api`）であり、すべての層を参照して公開面を根へ再輸出する。
//!
//! 本タスク（tasks.md 1.1 / 1.3）が置いたのは次の 2 層である。残りの層は担当タスクが足す。
//!
//! | 層 | モジュール | 担当 |
//! |----|------------|------|
//! | `source` | [`source`] | 1.3（型のみ）/ 1.6（規則と解釈） |
//! | `engine` | [`engine`] | 1.3（型のみ）/ 1.4・1.5（actor・isolate・上限） |
//!
//! # 実行の結果と失敗の型（tasks.md 1.3。設計の正典は design.md「Components and
//! Interfaces」の Service Interface）
//!
//! 実行の要求（[`RunRequest`]）・結果（[`RunOutcome`]）・失敗（[`MacroFailure`]）・
//! 変更の件数（[`ChangeSummary`]）・上限（[`Limits`]）を **1 箇所**に置く。並列に進む
//! 担当（実行基盤・変換・変更の集約・アダプタ）が**型で繋がる**ようにするためである。
//! 上限の**適用**（タスク 1.5）と変更の集約（タスク 2.3）はここには無い。
//!
//! [`RunOutcome`] は `Ran` / `Failed` / `Aborted` の 3 値であり、**打ち切りは失敗の一種
//! ではなく別の値**である（要件 6.1 / 6.2 の提示が失敗と異なるため。design.md
//!「Domain Model」）。
//!
//! # 公開面（design.md「Public API Layer」）
//!
//! 対外的な入口は `MacroRuntimeApi`（`src/api.rs`。タスク 1.4 以降）である。本ファイルは
//! 下位の各層の公開項目を根へ**再輸出**する（`document-format` / `schema-engine` と同じ形）。

pub mod engine;
pub mod source;
pub mod surface;

pub use engine::actor::{ActorError, MacroActor};
pub use engine::outcome::{
    ChangeSummary, FailureKind, Frame, LimitKind, Limits, MacroFailure, OutputLevel, OutputLine,
    RunOutcome, RunRequest, WindowLabel,
};
pub use source::capability::{
    parse as parse_capabilities, Capability, CapabilitySet, DeclarationError,
};
pub use source::record::{MacroKind, MacroName, MacroRecord};
pub use surface::declaration::{
    ApiCapability, ApiDecl, ParamDecl, RegistrationMismatch, HOST_APIS, HOST_NAMESPACE,
};
pub use surface::gate::{check_call, CallRefusal};

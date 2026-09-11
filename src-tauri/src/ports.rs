//! ドキュメント所有者への委譲点 — ウィンドウを閉じてよいかの問い合わせと、選択された
//! ファイルの引き渡しを受け取る契約。
//!
//! 所有: `DocumentHostPort`（design.md「Components and Interfaces → Adapter Layer」）。
//! 契約の形（design.md より）:
//!
//! ```text
//! trait DocumentHost {
//!     fn may_close(&self, window: &WindowId) -> CloseVerdict;
//!     fn attach(&self, window: &WindowId, path: &Path) -> Result<(), AttachError>;
//! }
//! ```
//!
//! 要件: 2.1, 2.6。
//!
//! 本ファイルはタスク 1.3 が置いた空のモジュール骨組みである。実体を埋めるのはタスク 6.2。
//! **常に許可し、引き渡しを受けても何もしない既定実装を同梱する**（本機能の時点では
//! ドキュメントを所有する機能が存在しないため）。下流スペックが差し替える。このポートの
//! 所有権は本スペック（app-shell）にある。

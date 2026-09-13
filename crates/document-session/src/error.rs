//! セッションの誤りと保存の結果（tasks.md 1.3。design.md「型（コア）」「Error Strategy」。
//! 要件 4.3, 5.1, 6.1, 6.6）。
//!
//! # 層の鎖（design.md「Architecture Pattern & Boundary Map」）
//!
//! 本モジュールは `error / state → session → change → table → api` の最左である。
//! **本クレートの他のどの層にも依存しない**: `std` と上流 `document-format`、design 採択の
//! `thiserror` だけを使う。したがってどの層からも参照できる共有の葉である。
//!
//! # 誤りと保存の結果を型で分ける（design.md「Error Strategy」の 3 分類）
//!
//! - [`SessionError`] は**セッションの状態の誤り**である: 保持していない（`NoDocument`）・
//!   別の操作が進行中（`Busy`）・未保存のため差し替えできない（`UnsavedChanges`）・
//!   読み込みに失敗した（`Read`）。**処理を止める**種類である。
//! - [`SaveReport`] は保存の結果という**列挙体であって誤り型ではない**。利用者の取り消し
//!   （`Cancelled`）は正常な結果であり、封筒の成功腕へ載る（design.md「Error Strategy」の
//!   第 3 分類。app-shell の `DocumentPickOutcome::Cancelled` と同じ判断）。したがって誤り型の
//!   derive（`thiserror::Error`）を SaveReport には揃えない: `Debug` だけを導出し、`Display` と
//!   `std::error::Error` は手書きする。`Error` を実装するのは、保存の結果を
//!   `Box<dyn Error>` の境界へ通せるようにするためであり、`Cancelled` を誤りとして扱う
//!   意味ではない（`source()` は `Failed` だけが `Some` を返す）。
//!
//! # 文脈だけを持ち、表示用の文言を持たない
//!
//! 変種は診断に必要な文脈（形式の側の誤り）だけを保持し、利用者向けの文言を持たない。
//! `Display` はログ・デバッグ用の最小限の技術的診断であり、`document-format` の
//! [`DocumentError`] と同じ規約である。提示（ロケール化メッセージを含む）は呼び出し元が
//! `match` で変種と文脈を取り出して組み立てる。形式の側の誤りはそのまま運ぶ
//! （`SessionError::Read` の `#[from]` 注釈と `SaveReport::Failed` の `source`）。
//! [`SessionState::Unavailable`](crate::state::SessionState::Unavailable) の理由だけは、
//! 適応層が形式の側の誤りを利用者へ伝えるために写した文字列であり、状態の側が持つ。

use core::fmt;
use std::path::PathBuf;

use document_format::DocumentError;
use thiserror::Error;

/// セッションの状態の誤り（design.md「型（コア）」・「Error Strategy」の第 2 分類）。
///
/// 変種は診断文脈のみを保持し、提示用の文言を持たない（モジュール docs 参照）。
/// 読み込みに失敗した場合は形式の側の誤り（[`DocumentError`]）を `source` にそのまま運び、
/// 失われた／差し替えを拒んだというセッション固有の理由と区別する。
#[derive(Debug, Error)]
pub enum SessionError {
    /// 別の操作が進行中（読み込みと作成は待たない。design.md「DocumentSessions」）。
    #[error("another operation is in progress")]
    Busy,

    /// そのウィンドウにドキュメントが無い。セッションを作るのは適応層の 3 つの入口だけであり、
    /// 未解決のウィンドウへの読み取り・変更の適用・保存はこれを返す。
    #[error("no document is open for this window")]
    NoDocument,

    /// 未保存の変更があるため、読み込み・作成を受け付けない。
    #[error("unsaved changes must be resolved before replacing the document")]
    UnsavedChanges,

    /// 読み込みに失敗した（形式の側の理由を運ぶ）。セッションは作られない。
    ///
    /// `#[from]` により `?` で [`DocumentError`] から変換できる（読み込み経路は形式の側の
    /// 失敗をこの変種へ写す。design.md「Error Categories and Responses」表の第 1 行）。
    #[error("failed to read the document: {source}")]
    Read {
        /// 形式の側（`document-format`）が返した誤り。
        #[from]
        source: DocumentError,
    },
}

/// 保存の結果（design.md「型（コア）」）。
///
/// **誤り型ではない**: [`SaveReport::Cancelled`]（利用者の取り消し）と
/// [`SaveReport::NeedsLocation`]（保存先の選択を要する提示の要求）は正常な結果であり、
/// 封筒の成功腕へ載る。誤りは [`SaveReport::Failed`] だけで、形式の側の理由を運ぶ。
/// 変種は提示用の文言を持たない（モジュール docs 参照）。
#[derive(Debug)]
pub enum SaveReport {
    /// 書き出しに成功した。以後の出所はこの位置である。
    Saved {
        /// 書き出した位置。
        location: PathBuf,
    },

    /// 出所が無い。保存先の選択を要する（適応層が提示し、`save_to` を呼ぶ）。
    NeedsLocation,

    /// 利用者が取り消した。**誤りではない。**
    Cancelled,

    /// 書き出しに失敗した。未保存の印は保たれる。
    Failed {
        /// 形式の側（`document-format`）が返した誤り。
        source: DocumentError,
    },
}

impl fmt::Display for SaveReport {
    /// ログ・デバッグ用の最小限の技術的診断（モジュール docs 参照）。提示文言ではない。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveReport::Saved { location } => write!(f, "saved to {}", location.display()),
            SaveReport::NeedsLocation => f.write_str("save location required"),
            SaveReport::Cancelled => f.write_str("save cancelled by the user"),
            SaveReport::Failed { source } => write!(f, "failed to save the document: {source}"),
        }
    }
}

impl std::error::Error for SaveReport {
    /// 形式の側の誤りは [`SaveReport::Failed`] だけが運ぶ。`Saved` / `NeedsLocation` /
    /// `Cancelled` は失敗ではないため `None` を返す。
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SaveReport::Failed { source } => Some(source),
            SaveReport::Saved { .. } | SaveReport::NeedsLocation | SaveReport::Cancelled => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_failure() -> DocumentError {
        DocumentError::MissingPart {
            name: "manifest.json".into(),
        }
    }

    /// ワイルドカードなしの `match` で全変種を網羅する（変種が増減すればここで壊れる）。
    fn discriminate(err: &SessionError) -> &'static str {
        match err {
            SessionError::Busy => "Busy",
            SessionError::NoDocument => "NoDocument",
            SessionError::UnsavedChanges => "UnsavedChanges",
            SessionError::Read { .. } => "Read",
        }
    }

    fn all_session_errors() -> Vec<SessionError> {
        vec![
            SessionError::Busy,
            SessionError::NoDocument,
            SessionError::UnsavedChanges,
            SessionError::Read {
                source: read_failure(),
            },
        ]
    }

    #[test]
    fn every_session_error_variant_is_constructible_and_discriminable() {
        let errors = all_session_errors();
        let labels: Vec<&'static str> = errors.iter().map(discriminate).collect();
        assert_eq!(
            vec!["Busy", "NoDocument", "UnsavedChanges", "Read"],
            labels,
            "SessionError の 4 変種が判別可能でない"
        );

        // Read は形式の側の誤りをそのまま（文言を足さずに）運ぶ。
        match &errors[3] {
            SessionError::Read { source } => assert!(matches!(
                source,
                DocumentError::MissingPart { name } if name == "manifest.json"
            )),
            _ => unreachable!(),
        }
    }

    /// ワイルドカードなしの `match` で保存の結果の全変種を網羅する。
    fn discriminate_save(report: &SaveReport) -> &'static str {
        match report {
            SaveReport::Saved { .. } => "Saved",
            SaveReport::NeedsLocation => "NeedsLocation",
            SaveReport::Cancelled => "Cancelled",
            SaveReport::Failed { .. } => "Failed",
        }
    }

    #[test]
    fn every_save_report_variant_is_constructible_and_discriminable() {
        let reports = vec![
            SaveReport::Saved {
                location: PathBuf::from("/tmp/doc.jxcel"),
            },
            SaveReport::NeedsLocation,
            SaveReport::Cancelled,
            SaveReport::Failed {
                source: read_failure(),
            },
        ];
        let labels: Vec<&'static str> = reports.iter().map(discriminate_save).collect();
        assert_eq!(
            vec!["Saved", "NeedsLocation", "Cancelled", "Failed"],
            labels,
            "SaveReport の 4 変種が判別可能でない"
        );

        match &reports[0] {
            SaveReport::Saved { location } => {
                assert_eq!(location, &PathBuf::from("/tmp/doc.jxcel"));
            }
            _ => unreachable!(),
        }
    }

    /// 形式の側の誤りは `?` でセッションの誤りへ変換できる（`From<DocumentError>`）。
    #[test]
    fn a_document_error_converts_into_a_session_error_via_question_mark() {
        fn load() -> Result<(), SessionError> {
            Err(read_failure())?;
            Ok(())
        }

        let err = load().expect_err("読み込みの失敗はエラーになる");
        assert!(matches!(
            err,
            SessionError::Read { source } if matches!(source, DocumentError::MissingPart { .. })
        ));
    }

    /// どちらの型も `std::error::Error` を実装し、source 連鎖が正しく結線される
    /// （`?` で使える）。形式の側の誤りを運ぶ変種だけが source を持つ。
    #[test]
    fn both_types_are_std_errors() {
        use std::error::Error as _;

        let read = SessionError::Read {
            source: read_failure(),
        };
        assert!(read.source().is_some(), "Read は source を持つ");
        for err in [
            SessionError::Busy,
            SessionError::NoDocument,
            SessionError::UnsavedChanges,
        ] {
            assert!(
                err.source().is_none(),
                "Read 以外の変種は source を持たない"
            );
        }

        let failed = SaveReport::Failed {
            source: read_failure(),
        };
        assert!(failed.source().is_some(), "Failed は source を持つ");
        assert!(
            SaveReport::Cancelled.source().is_none(),
            "取り消しは誤りではなく、source を持たない"
        );
        assert!(SaveReport::NeedsLocation.source().is_none());
        assert!(SaveReport::Saved {
            location: PathBuf::from("/tmp/doc.jxcel")
        }
        .source()
        .is_none());
    }

    /// 誤りと保存の結果は、適応層が保持してスレッドを跨げる
    /// （design.md「Implementation Notes」）。
    #[test]
    fn error_types_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SessionError>();
        assert_send_sync::<SaveReport>();
    }
}

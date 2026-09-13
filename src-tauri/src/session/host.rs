//! ドキュメント所有者の宿主 — セッションを**内側の宿主へ連鎖させる** `DocumentHost` の実装。
//!
//! 所有: `SessionDocumentHost`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 2.2、2.5、4.6、6.1。
//!
//! # 連鎖とは何か
//!
//! `app-shell` の `DocumentHostPort` は「閉じてよいかの判定」と「選ばれたファイルの引き渡し」を
//! 委譲する点であり、**現在の宿主を `host()` で取り出して置き換えられる**（`ports.rs` の
//! 「差し替え（下流スペックの接続点）」）。本モジュールはその置き換え先であり、**設置前の宿主を
//! 内側に保つ**。
//!
//! ```text
//! DocumentHostPort → SessionDocumentHost → inner: Arc<dyn DocumentHost>
//!                     （セッションの答えを先に見る）  （設置前の宿主。検証用の宿主など）
//! ```
//!
//! 連鎖にする理由は `session/mod.rs` の doc にある（**検証用の宿主の拒否の実測と引き渡しの
//! 記録を壊さない**）。ここは順序だけを定める: **セッション → 内側**。
//!
//! # 判定（要件 2.2、2.5、4.6）
//!
//! [`may_close`](DocumentHost::may_close) は**セッションの答えを先に見る**:
//!
//! - セッションが拒否したら（未保存）**内側を呼ばずに**拒否を返す（要件 6.1 の理由を
//!   ここで組み立てる）
//! - セッションが許可したら内側へ委ねる（検証用の宿主が名指しのラベルを拒否していても、
//!   その拒否はそのまま生きる）
//!
//! **未解決のウィンドウでは解決を試みない。** セッションの `may_close` は
//! `DocumentSessions::may_close` が表の `existing` 経由で答えるため、未解決の窓には
//! [`CloseAnswer::Allow`] を返し、読み込みを起こさない（design.md「SessionDocumentHost」の
//! 第 2 項）。読み込みは秒単位かかり、**まだ何も変更されていない**のだから、閉じるために
//! 読む必要は無い。
//!
//! **このメソッドはブロックしない**（`ports.rs` の契約）。`may_close` は未保存の原子値だけを
//! 読み、文書のロックを待たない（`document-session` の `Slot::may_close`）。
//!
//! # 引き渡し（要件 2.2、1.3）
//!
//! [`attach`](DocumentHost::attach) は**適応層の引き渡しの入口**
//! （[`WindowDestroyWatch::attach`]）を通す。表を直接触ってはならない — 表の `attach` は
//! 破棄の購読を伴わないセッションを作りうるので、**破棄してもその文書が表に残る**
//! （要件 1.5 が破れる。`session/watch.rs` の「セッションを作る経路は本型の 3 つの入口に
//! 閉じる」）。
//!
//! その入口は**コアの引き渡しの入口**（`DocumentSessions::attach`）を通す。コアの入口は
//! 未保存なら拒否し（要件 2.2）、読み込みに失敗しても保持している文書を変えない（要件 2.1）。
//! **成功したら内側にも渡す** — 内側が検証用の宿主なら、そこで引き渡しの記録が残る
//! （設置しても記録が失われないことが連鎖の目的である）。
//!
//! **失敗したら内側を呼ばない。** 内側は「引き渡しが成立した」ことの記録先であり、
//! セッションが受け取れなかった位置を渡しても、成立していない引き渡しを記録させるだけである。
//!
//! # 理由の文言はここで組み立てる（要件 6.1）
//!
//! `document-session` の誤りと答えは**表示用の文言を持たない**（文言は適応層が組み立てる）。
//! `ports.rs` の「見せ方を決めるのは仲介」に従い、**本モジュールが利用者へ伝える理由を
//! 組み立てる**。形式の側の誤り（[`DocumentError`]）はその `Display` を理由に含める
//! （利用者が「なぜ読めなかったか」を知る唯一の材料である）。

use std::path::Path;
use std::sync::Arc;

use app_shell::ipc::WindowLabel;
use document_session::{CloseAnswer, DocumentSessionsApi, SessionError};

use crate::session::watch::WindowDestroyWatch;

use crate::ports::{AttachError, CloseVerdict, DocumentHost};

/// セッションを内側の宿主へ連鎖させる `DocumentHost`（design.md「SessionDocumentHost」）。
///
/// 設置前の宿主を [`inner`](Self::inner) に保つ。判定と引き渡しの順序はモジュール doc の
/// とおりである（**セッション → 内側**）。
///
/// 保持する 2 つはどちらも `Arc` であり、本型は `Send + Sync` である（`DocumentHostPort` が
/// アプリ全体の管理状態として共有するため）。**セッションは [`WindowDestroyWatch`] を経由して
/// 共有する** — `install` の後に 3.4 のコマンドが同じ実体を `app.state` から取れるようにする
/// ためであり、かつ**引き渡しの経路が破棄の購読を伴う**ためである
/// （`session/watch.rs` の「セッションを作る経路は本型の 3 つの入口に閉じる」）。
pub struct SessionDocumentHost {
    /// セッションの入口と破棄の購読（この機能が所有するドキュメントの唯一の源）。
    entrances: Arc<WindowDestroyWatch>,
    /// 連鎖の内側（設置前の宿主）。検証ビルドでは `VerificationDocumentHost` が入り、
    /// 拒否の実測と引き渡しの記録を担う。
    inner: Arc<dyn DocumentHost>,
}

impl SessionDocumentHost {
    /// セッションの入口と内側の宿主を結びつけた連鎖を作る。
    ///
    /// `inner` は **`DocumentHostPort::host()` で取り出した現在の宿主**でなければならない
    /// （設置前の宿主を置き去りにすると、検証用の宿主の拒否と記録が失われる。
    /// `session/mod.rs` の `install` がその順序を守る）。
    pub fn new(entrances: Arc<WindowDestroyWatch>, inner: Arc<dyn DocumentHost>) -> Self {
        Self { entrances, inner }
    }
}

impl DocumentHost for SessionDocumentHost {
    /// セッションの答えを先に見て、許可なら内側へ委ねる（要件 2.2、4.6、6.1）。
    ///
    /// **未解決のウィンドウでは解決を試みない** — セッションの答えが `Allow` になり、
    /// 読み込みは起きない（design.md「SessionDocumentHost」。読み込みは秒単位かかる）。
    /// **ブロックしない**（`ports.rs` の契約。セッションは未保存の原子値だけを読む）。
    fn may_close(&self, window: &WindowLabel) -> CloseVerdict {
        match self.entrances.sessions().may_close(window) {
            CloseAnswer::Deny => CloseVerdict::Deny {
                reason: unsaved_reason(window),
            },
            // 未保存でない（または未解決の）ウィンドウは内側の答えに委ねる。検証用の宿主の
            // 名指しの拒否はここでそのまま生きる（連鎖が内側を保存する所以である）。
            CloseAnswer::Allow => self.inner.may_close(window),
        }
    }

    /// セッションへ引き渡してから、内側へも渡す（要件 1.3、2.2、2.4）。
    ///
    /// 経路は**適応層の入口**（[`WindowDestroyWatch::attach`]）である。表を直接触ると
    /// 破棄の購読を伴わないセッションが生まれ、破棄しても文書が表に残る（要件 1.5）。
    /// 入口は**コアの引き渡しの入口**（`DocumentSessions::attach`）を通す。コアの入口は
    /// **未保存なら拒否**し、**読み込みに失敗しても保持している文書を変えない**。失敗のときは
    /// [`AttachError`] へ写して返し、**内側を呼ばない**（成立していない引き渡しを記録させない）。
    /// 成功したら内側へも渡す（内側が記録を持つ宿主でも、引き渡しの観測が失われない）。
    fn attach(&self, window: &WindowLabel, path: &Path) -> Result<(), AttachError> {
        match self.entrances.attach(window, path) {
            Ok(()) => self.inner.attach(window, path),
            Err(error) => Err(AttachError::new(attach_reason(window, &error))),
        }
    }
}

/// 未保存のために閉じられないことを伝える理由（要件 6.1）。
///
/// **文言を組み立てるのは適応層の仕事である**（`document-session` の `CloseAnswer` は
/// 2 値しか持たない）。将来 3 択の提示（要件 6.2）が選択肢を出す材料としても読めるよう、
/// 「未保存の変更がある」という事実だけを短く伝える。
fn unsaved_reason(window: &WindowLabel) -> String {
    format!(
        "{} のドキュメントに未保存の変更がある",
        window.as_str()
    )
}

/// 引き渡しを受け取れなかった理由（要件 2.1、2.2）。
///
/// 形式の側の誤り（[`SessionError::Read`] の `source`）は**その `Display` を理由に含める** —
/// 利用者へ「なぜ読めなかったか」を伝える唯一の材料であり、ここが組み立て場所である。
fn attach_reason(window: &WindowLabel, error: &SessionError) -> String {
    match error {
        SessionError::UnsavedChanges => format!(
            "{} のドキュメントに未保存の変更があるため、別のドキュメントを開けない",
            window.as_str()
        ),
        SessionError::Busy => format!(
            "{} のドキュメントで別の操作が進行中である",
            window.as_str()
        ),
        SessionError::NoDocument => format!(
            "{} のドキュメントを受け取れなかった",
            window.as_str()
        ),
        SessionError::Read { source } => format!(
            "{} のドキュメントを読み込めなかった: {source}",
            window.as_str()
        ),
    }
}

// ---------------------------------------------------------------------------
// テスト（タスク 3.2）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use app_shell::ipc::WindowLabel;
    use document_format::{
        CellValue, Document, DocumentFormat, DocumentFormatApi, SchemaPart,
    };
    use document_session::{
        CloseAnswer, DocumentSessions, DocumentSessionsApi, SessionState,
    };

    use super::SessionDocumentHost;
    use crate::ports::{AttachError, CloseVerdict, DocumentHost};
    use crate::session::watch::testing::AlwaysPresent;
    use crate::session::watch::WindowDestroyWatch;

    /// 内側の宿主が委譲を受けたことを記録する二重。
    ///
    /// `ports.rs` の `RecordingHost` と同じ形である（**本番の実装ではない**）。連鎖の順序を
    /// 「セッションが拒否したら内側が呼ばれない」「セッションが受け取ったら内側へも届く」の
    /// 2 面で観測するために、判定の呼び出し回数と引き渡しの組を記録する。
    #[derive(Default)]
    struct RecordingHost {
        /// このラベルのウィンドウだけを拒否する（`None` なら常に許可）。内側の答えが
        /// そのまま返ることを見るために使う。
        deny: Option<(String, String)>,
        /// `may_close` が呼ばれた回数（セッションが拒否したときに 0 のままであることを見る）。
        may_close_calls: Mutex<usize>,
        /// `attach` が受け取った（ウィンドウ, パス）。
        attached: Mutex<Vec<(String, PathBuf)>>,
    }

    impl RecordingHost {
        fn denying(window: &str, reason: &str) -> Self {
            Self {
                deny: Some((window.to_owned(), reason.to_owned())),
                ..Self::default()
            }
        }

        fn may_close_calls(&self) -> usize {
            *self
                .may_close_calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }

        fn attached(&self) -> Vec<(String, PathBuf)> {
            self.attached
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    impl DocumentHost for RecordingHost {
        fn may_close(&self, window: &WindowLabel) -> CloseVerdict {
            *self
                .may_close_calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
            match &self.deny {
                Some((target, reason)) if target == window.as_str() => CloseVerdict::Deny {
                    reason: reason.clone(),
                },
                _ => CloseVerdict::Allow,
            }
        }

        fn attach(&self, window: &WindowLabel, path: &Path) -> Result<(), AttachError> {
            self.attached
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((window.as_str().to_owned(), path.to_path_buf()));
            Ok(())
        }
    }

    fn label(raw: &str) -> WindowLabel {
        WindowLabel::new(raw)
    }

    /// 連鎖を作る（`RecordingHost` とセッションの入口の組）。
    ///
    /// セッションの入口は**破棄の購読**（本番では `TauriWindowEvents`）を要する。破棄そのものは
    /// `session/watch.rs` のテストが確かめるので、ここは常に引けるという二重を渡す
    /// （連鎖の順序を見るのがここの目的である）。
    fn chained(inner: Arc<RecordingHost>) -> (SessionDocumentHost, Arc<DocumentSessions>) {
        let sessions = Arc::new(DocumentSessions::new());
        let watch = Arc::new(WindowDestroyWatch::new(
            Arc::new(AlwaysPresent),
            Arc::clone(&sessions),
        ));
        (
            SessionDocumentHost::new(watch, inner),
            sessions,
        )
    }

    /// 一時ディレクトリを作る（プロセスごとに一意。`document-session` の `Scratch` と同じ規律）。
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
                "jxcel-session-host-{tag}-{}-{unique}",
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

    /// 1 シート 1 行の**本物の文書**を書く（dev-dependency の `document-format` を使う）。
    ///
    /// 公開 API だけで組み立てる（形式の内部モジュールに触れない）。ルートスキーマを
    /// 与えてから列と行を置く順序は、形式の側の検証を満たす最小の形である。
    fn write_document(path: &Path, sheet_name: &str, value: &str) {
        let mut document = Document::new();
        let sheet = document.add_sheet(sheet_name);
        document
            .set_sheet_columns(sheet, vec!["note".to_owned()])
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, SchemaPart::empty())
            .expect("標本のシートは実在する");
        let row = document.add_row(sheet).expect("標本のシートは実在する");
        document
            .set_row_values(sheet, row, vec![CellValue::Text(value.to_owned())])
            .expect("標本の行は実在する");
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
    }

    /// **連鎖の順序**: 未保存のウィンドウではセッションが拒否し、内側が呼ばれない。
    ///
    /// 未保存の印を立てるのは変更の適用（`edit`）である。内側が呼ばれないことは
    /// `may_close_calls` が 0 のままであることで観測する。
    #[test]
    fn the_session_refusal_does_not_reach_the_inner_host() {
        let inner = Arc::new(RecordingHost::default());
        let (host, sessions) = chained(Arc::clone(&inner));

        sessions
            .create(&label("doc-1"))
            .expect("新規作成はセッションを用意する");
        sessions
            .edit(&label("doc-1"), &mut |_| ())
            .expect("変更を適用できる");

        let verdict = host.may_close(&label("doc-1"));
        assert!(
            matches!(verdict, CloseVerdict::Deny { .. }),
            "未保存のウィンドウは拒否される: {verdict:?}"
        );
        assert_eq!(0, inner.may_close_calls(), "内側の宿主が呼ばれた");
        assert_eq!(
            CloseAnswer::Deny,
            sessions.may_close(&label("doc-1")),
            "セッションの答えが拒否でない"
        );
    }

    /// **連鎖の順序**: 未保存でないウィンドウでは内側の答えがそのまま返る。
    ///
    /// 内側に拒否を返す二重を使い、その理由が加工されずに届くことを見る（セッションの
    /// 答えに上書きされたら、検証用の宿主の拒否の実測が壊れる）。
    #[test]
    fn the_inner_answer_passes_through_for_a_clean_window() {
        let inner = Arc::new(RecordingHost::denying("doc-1", "内側の拒否"));
        let (host, sessions) = chained(Arc::clone(&inner));

        sessions
            .create(&label("doc-1"))
            .expect("新規作成はセッションを用意する");
        assert!(
            matches!(sessions.state(&label("doc-1")), SessionState::Open { .. }),
            "新規作成がセッションを用意していない"
        );

        assert_eq!(
            host.may_close(&label("doc-1")),
            CloseVerdict::Deny {
                reason: "内側の拒否".to_owned()
            }
        );
        assert_eq!(1, inner.may_close_calls(), "内側へ委ねられていない");
        // 別のウィンドウは内側の名指しに一致しないので許可が返る（判定はウィンドウ単位である）。
        assert_eq!(CloseVerdict::Allow, host.may_close(&label("doc-2")));
    }

    /// **未解決のウィンドウの `may_close` では解決を試みない**（内側へ委ねられること）。
    ///
    /// セッションは未解決の窓に読み込みを起こさない（`may_close` は `existing` 経由で
    /// `Allow` を返す）。したがって内側が呼ばれ、状態は `Absent` のままである。
    #[test]
    fn may_close_does_not_resolve_an_unresolved_window() {
        let inner = Arc::new(RecordingHost::default());
        let (host, sessions) = chained(Arc::clone(&inner));

        // 起動時の位置を渡しても、`may_close` は `resolve` を呼ばない。
        assert_eq!(CloseVerdict::Allow, host.may_close(&label("empty-1")));
        assert_eq!(1, inner.may_close_calls(), "内側へ委ねられていない");
        assert_eq!(
            SessionState::Absent,
            sessions.state(&label("empty-1")),
            "問い合わせがセッションを作った"
        );
    }

    /// **引き渡しの成功で内側にも渡る**: `attach` が成功し、内側が同じ組を受け取る。
    #[test]
    fn a_successful_attach_reaches_the_inner_host() {
        let scratch = Scratch::new("attach-success");
        let path = scratch.file("chosen.jxcel");
        write_document(&path, "選ばれた", "受け取った");

        let inner = Arc::new(RecordingHost::default());
        let (host, sessions) = chained(Arc::clone(&inner));
        assert_eq!(Ok(()), host.attach(&label("empty-1"), &path));

        assert_eq!(
            vec![("empty-1".to_owned(), path.clone())],
            inner.attached(),
            "内側へ同じ（ウィンドウ, 位置）が届いていない"
        );
        // セッションが本物の文書を保持している（読み込みが起きた）。
        match sessions.state(&label("empty-1")) {
            SessionState::Open { name, sheets, .. } => {
                assert_eq!("chosen.jxcel", name);
                assert_eq!(1, sheets.len(), "標本のシートが 1 枚");
            }
            other => panic!("引き渡しがセッションへ届いていない: {other:?}"),
        }
    }

    /// **引き渡しの失敗では内側を呼ばない**: 存在しない位置を渡すと `AttachError` が返り、
    /// 内側の記録が空のままである（成立していない引き渡しを記録させない）。
    #[test]
    fn a_failed_attach_does_not_reach_the_inner_host() {
        let scratch = Scratch::new("attach-failure");
        let missing = scratch.file("missing.jxcel");

        let inner = Arc::new(RecordingHost::default());
        let (host, sessions) = chained(Arc::clone(&inner));
        let error = host
            .attach(&label("empty-1"), &missing)
            .expect_err("存在しない位置は受け取れない");

        assert!(
            error.message().contains("読み込めなかった"),
            "理由が読み込みの失敗に触れていない: {error}"
        );
        assert!(
            error.message().contains("empty-1"),
            "理由に対象のウィンドウが無い: {error}"
        );
        assert!(inner.attached().is_empty(), "内側が呼ばれた");
        assert!(
            matches!(
                sessions.state(&label("empty-1")),
                SessionState::Unavailable { .. }
            ),
            "読み込みの失敗が覚えられていない"
        );
    }

    /// **未保存のときの `attach` は拒否され、保持している文書が変わらない**。
    ///
    /// 未保存の印を立てた**後の**状態を基準に取り、拒否の後も同じであることを見る。加えて
    /// 保持している内容そのもの（行のテキスト）を読み直し、置き換わっていないことを観測する。
    #[test]
    fn an_unsaved_window_refuses_attach_and_keeps_its_document() {
        let scratch = Scratch::new("attach-unsaved");
        let first = scratch.file("first.jxcel");
        let second = scratch.file("second.jxcel");
        write_document(&first, "最初", "そのまま");
        write_document(&second, "二番目", "置き換え");

        let inner = Arc::new(RecordingHost::default());
        let (host, sessions) = chained(Arc::clone(&inner));

        assert_eq!(Ok(()), host.attach(&label("doc-1"), &first));
        sessions
            .edit(&label("doc-1"), &mut |_| ())
            .expect("変更を適用できる");
        let before = sessions.state(&label("doc-1"));

        let error = host
            .attach(&label("doc-1"), &second)
            .expect_err("未保存のウィンドウは引き渡しを拒否する");
        assert!(
            error.message().contains("未保存"),
            "理由が未保存に触れていない: {error}"
        );
        assert_eq!(
            before,
            sessions.state(&label("doc-1")),
            "拒否が保持している文書を変えた"
        );
        // 保持している内容そのものも変わっていない（状態の写しだけでなく中身を読む）。
        let note = sessions
            .read(&label("doc-1"), &mut |document| {
                let sheet = &document.sheets()[0];
                match &sheet.rows()[0].values()[0] {
                    CellValue::Text(text) => text.clone(),
                    other => panic!("標本の値がテキストでない: {other:?}"),
                }
            })
            .expect("拒否の後も文書を読める");
        assert_eq!("そのまま", note, "拒否が保持している内容を置き換えた");
        // 最初の成功の 1 件だけが内側へ届いている（拒否は内側へ渡らない）。
        assert_eq!(vec![("doc-1".to_owned(), first)], inner.attached());
    }
}

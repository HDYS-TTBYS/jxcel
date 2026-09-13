//! セッションの状態（tasks.md 1.3。design.md「型（コア）」「Logical Data Model」。
//! 要件 4.3, 5.1, 6.1, 6.6）。
//!
//! # 層の鎖（design.md「Architecture Pattern & Boundary Map」）
//!
//! 本モジュールは `error / state → session → change → table → api` の最左である。
//! **本クレートの他のどの層にも依存しない**: `std` と上流 `document-format`
//! （[`SheetId`]）だけを使う。
//!
//! # 状態の写しと、文書そのものを分ける
//!
//! [`SessionState`] は問い合わせのたびに組み立てる**写し**であり、文書の実体
//! （セッションが保持する `Document`）ではない。したがってここには文書の内部表現を
//! 持ち込まず、名前・出所・未保存の有無・シートの要約だけを運ぶ。表示用の文言は持たない
//! （文言は適応層が組み立てる）。例外は [`SessionState::Unavailable`] の `reason` であり、
//! これは形式の側の誤りを利用者へ伝えるために適応層が写した文字列である
//! （design.md「型（コア）」）。
//!
//! # 識別子は文字列化しない
//!
//! [`SheetSummary::id`] は [`SheetId`] のまま持つ。境界（`crates/app-shell/src/ipc/`）へは
//! 文字列として写すが、その写像は境界型の担当であり、本クレートは行わない
//! （design.md「Boundary Commitments」の「位置を境界へ出さない」。`document-session` が
//! `ts-rs` の derive を持てる場所は境界だけである）。

use std::path::PathBuf;

use document_format::SheetId;

/// ドキュメントの出所（design.md「Logical Data Model」）。
///
/// [`Origin::New`] は新規作成された文書であり、保存には保存先の選択を要する
/// （出所が無いため `SaveReport::NeedsLocation` になる）。[`Origin::File`] は読み込んだ
/// 位置であり、保存はこの位置へ書き出す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// 新規作成された文書。出所の位置を持たない。
    New,
    /// 読み込んだファイルの位置。
    File(PathBuf),
}

/// 状態の写しに載せるシートの要約（design.md「型（コア）」）。
///
/// シートの実体（`document_format::Sheet`）は文書が所有し、この型は状態の表示に必要な
/// 最小限（識別子・名前・列数・行数）だけを運ぶ。**識別子は [`SheetId`]** であり、
/// 文字列化は境界型（`crates/app-shell/src/ipc/`）の担当である。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetSummary {
    /// シート識別子。
    pub id: SheetId,
    /// シート名。
    pub name: String,
    /// 列数。
    pub columns: usize,
    /// 行数。
    pub rows: usize,
}

/// ウィンドウの状態の写し（design.md「型（コア）」「Logical Data Model」）。
///
/// 3 つの腕は「保持していない / 保持している / 読み込めなかった」の判別である。
/// 表示用の文言を持たない（モジュール docs 参照）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// そのウィンドウにドキュメントが無い（未解決・未作成）。
    Absent,

    /// ドキュメントを保持している。
    Open {
        /// 名前（ファイル名のみ）。新規は空文字。
        name: String,
        /// 出所。
        origin: Origin,
        /// 未保存の変更があるか。
        unsaved: bool,
        /// シートの要約。
        sheets: Vec<SheetSummary>,
    },

    /// 読み込みに失敗した。理由は適応層が形式の側の誤りから写した文字列である
    /// （セッションは作られず、ウィンドウの状態は変わらない）。
    Unavailable {
        /// 読み込めなかった理由。
        reason: String,
    },
}

/// 変更の適用の結果（design.md「型（コア）」「変更の適用」）。
///
/// 1 回の閉包が 1 回の適用であり、[`Edited::revision`] は適用のたびに 1 進む
/// （10 万行を跨ぐ一括の適用も 1 回として運ばれる）。[`Edited::unsaved`] は適用後の
/// 未保存の印であり、閉包が失敗した場合も保守側に倒して `true` になる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited<R> {
    /// 閉包の戻り値。
    pub value: R,
    /// 適用後の変更の版。
    pub revision: u64,
    /// 適用後の未保存の印。
    pub unsaved: bool,
}

/// 閉じてよいかの答え（design.md「型（コア）」）。
///
/// 2 値であり、**理由の文言を持たない**: 理由の提示は適応層が組み立てる（要件 6.1）。
/// 未保存の原子値だけを読んで答えるため、他の操作が進行中でも待たない（要件 2.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAnswer {
    /// 閉じてよい。
    Allow,
    /// 閉じてはならない（未保存の変更がある）。
    Deny,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> SheetSummary {
        SheetSummary {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV"
                .parse::<SheetId>()
                .expect("正準 ULID のテキスト形"),
            name: "台帳".into(),
            columns: 30,
            rows: 100_000,
        }
    }

    #[test]
    fn session_state_has_absent_open_and_unavailable() {
        let states = vec![
            SessionState::Absent,
            SessionState::Open {
                name: "doc.jxcel".into(),
                origin: Origin::File(PathBuf::from("/tmp/doc.jxcel")),
                unsaved: true,
                sheets: vec![sheet()],
            },
            SessionState::Unavailable {
                reason: "missing part: manifest.json".into(),
            },
        ];

        // ワイルドカードなしの `match` で全変種を網羅する（変種が増減すればここで壊れる）。
        fn discriminate(state: &SessionState) -> &'static str {
            match state {
                SessionState::Absent => "Absent",
                SessionState::Open { .. } => "Open",
                SessionState::Unavailable { .. } => "Unavailable",
            }
        }

        let labels: Vec<&'static str> = states.iter().map(discriminate).collect();
        assert_eq!(
            vec!["Absent", "Open", "Unavailable"],
            labels,
            "SessionState の 3 変種が判別可能でない"
        );

        match &states[0] {
            SessionState::Absent => {}
            SessionState::Open { .. } => panic!("Absent でない"),
            SessionState::Unavailable { .. } => panic!("Absent でない"),
        }
        match &states[1] {
            SessionState::Open {
                name,
                origin,
                unsaved,
                sheets,
            } => {
                assert_eq!(name, "doc.jxcel");
                assert_eq!(origin, &Origin::File(PathBuf::from("/tmp/doc.jxcel")));
                assert!(unsaved);
                assert_eq!(sheets.len(), 1);
                assert_eq!(sheets[0].columns, 30);
                assert_eq!(sheets[0].rows, 100_000);
            }
            SessionState::Absent => panic!("Open でない"),
            SessionState::Unavailable { .. } => panic!("Open でない"),
        }
        match &states[2] {
            SessionState::Unavailable { reason } => {
                assert_eq!(reason, "missing part: manifest.json");
            }
            SessionState::Absent => panic!("Unavailable でない"),
            SessionState::Open { .. } => panic!("Unavailable でない"),
        }
    }

    #[test]
    fn origin_distinguishes_new_from_a_file_location() {
        let new = Origin::New;
        let file = Origin::File(PathBuf::from("/tmp/doc.jxcel"));
        match &new {
            Origin::New => {}
            Origin::File(_) => panic!("New でない"),
        }
        match &file {
            Origin::File(path) => assert_eq!(path, &PathBuf::from("/tmp/doc.jxcel")),
            Origin::New => panic!("File でない"),
        }
    }

    #[test]
    fn close_answer_is_two_valued() {
        let answers = [CloseAnswer::Allow, CloseAnswer::Deny];
        match answers[0] {
            CloseAnswer::Allow => {}
            CloseAnswer::Deny => panic!("Allow でない"),
        }
        match answers[1] {
            CloseAnswer::Deny => {}
            CloseAnswer::Allow => panic!("Deny でない"),
        }
    }

    #[test]
    fn edited_carries_value_revision_and_unsaved() {
        let mut edited = Edited {
            value: 7u32,
            revision: 3,
            unsaved: true,
        };
        assert_eq!(edited.value, 7);
        assert_eq!(edited.revision, 3);
        assert!(edited.unsaved);

        edited.value = 8;
        edited.revision = 4;
        edited.unsaved = false;
        assert_eq!(edited.value, 8);
        assert_eq!(edited.revision, 4);
        assert!(!edited.unsaved);
    }

    /// 適応層が状態を保持してスレッドを跨げる（design.md「Implementation Notes」）。
    #[test]
    fn state_types_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SessionState>();
        assert_send_sync::<Origin>();
        assert_send_sync::<SheetSummary>();
        assert_send_sync::<CloseAnswer>();
        assert_send_sync::<Edited<u32>>();
    }
}

//! 公開面だけを使った、木に属する操作口の結線（tasks.md 2.5。design.md「core →
//! DocumentSessions（公開面）」の逐語。requirements.md 1.4, 3.1）。
//!
//! 本ファイルは `document_session` の**根の名前だけ**を import する。`document_session::table`
//! や `document_session::session` のような下位モジュールの経路を 1 つも書かない — これが
//! 「下流は根の再輸出だけを使う」ことの実行可能な証明である（`document-format` / `schema-engine`
//! の `tests/public_api.rs` と同じ規律）。上流の `document-format`（`CellValue` /
//! `DocumentFormatApi`）と `app-shell`（`WindowLabel`）は本クレートの内部ではなく別クレートで
//! あるため、ここから直接使ってよい。
//!
//! # 通す経路
//!
//! 操作口の全メソッドを通す: 解決（`resolve`）→ 状態（`state`）→ 読み取り（`read`）→
//! 変更の適用（`edit`。上流の一括の書き換え `set_cells` を閉包の内側で呼ぶ）→ 保存（`save`）→
//! 保存先を指定した保存（`save_to`）→ 引き渡し（`attach`）→ 新規作成（`create`）→
//! 破棄の印（`discard`）→ 閉じてよいか（`may_close`）→ 破棄（`forget`）。
//!
//! # 未解決のウィンドウに触れないことの観測
//!
//! [`state_may_close_and_forget_leave_an_unresolved_window_absent`] は、セッションを作らない
//! 3 つの口（`state` / `may_close` / `forget`）を未解決のウィンドウへ呼び、**そのあとの状態が
//! `Absent` のまま**であることと、読み取り・変更・保存が `NoDocument` で失敗することを確かめる。
//! 「セッションが作られなかった」こと自体は根の名前からは区別できない（未解決のセッションは
//! どの口から見ても `Absent` / `Allow` / `NoDocument` である）ため、この不変条件は
//! **操作口が `existing` を使うこと**（`lib.rs` の実装の形状）が担保し、テストは契約の側
//! （未解決のウィンドウの答えが変わらないこと）を固定する。

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use app_shell::ipc::WindowLabel;
use document_format::{CellValue, DocumentFormatApi};

use document_session::{
    CloseAnswer, DocumentSessions, DocumentSessionsApi, Origin, SaveReport, SessionError,
    SessionState, SheetSummary,
};

/// 標本を一時ディレクトリへ書き出し、その位置を返す（`Scratch` は呼び出し元が保つ）。
fn write_sample(scratch: &common::Scratch, name: &str, rows: usize, columns: usize) -> PathBuf {
    let path = scratch.file(name);
    let sample = common::sample(&common::SampleSpec::new(rows, columns));
    common::api()
        .save(sample.document(), &path)
        .expect("標本を保存できる");
    path
}

/// ウィンドウの識別子（境界の型は上流 `app-shell` の単一の定義を借りる）。
fn window(label: &str) -> WindowLabel {
    WindowLabel::new(label)
}

/// `SessionState::Open` の中身を取り出す（他の腕はテストの失敗である）。
fn opened(state: SessionState) -> (String, Origin, bool, Vec<SheetSummary>) {
    match state {
        SessionState::Open {
            name,
            origin,
            unsaved,
            sheets,
        } => (name, origin, unsaved, sheets),
        other => panic!("保持している状態を期待したが {other:?} だった"),
    }
}

/// 標本の先頭シートの行識別子とシート識別子を、文書の内側から取る閉包を組み立てる。
///
/// 変更の語彙は本クレートが持たない（何をどう変えるかは要求側の所有である）。ここでは
/// 上流 `document-format` の一括の書き換え `set_cells` を閉包の内側で呼ぶ。
fn set_first_cell(value: CellValue) -> impl FnMut(&mut document_format::Document) -> bool {
    move |document| {
        let sheet = document.sheets()[0].id();
        let row = document.sheets()[0].rows()[1].id();
        document
            .set_cells(sheet, &[(row, 0, value.clone())])
            .is_ok()
    }
}

/// `resolve` が起動時に指定された位置を読み込み、状態が名前・出所・シートの要約を返し、
/// `read` が同じ経路で文書を貸す（tasks.md 2.5。要件 1.2, 1.4, 1.6, 1.7）。
#[test]
fn resolve_reports_an_open_state_and_read_lends_the_document() {
    let scratch = common::Scratch::new("public-resolve");
    let path = write_sample(&scratch, "book.jxcel", 4, 3);

    let sessions = DocumentSessions::new();
    let main = window("main");
    sessions.resolve(&main, Some(&path)).expect("読み込める");

    let (name, origin, unsaved, sheets) = opened(sessions.state(&main));
    assert_eq!(name, "book.jxcel", "名前はファイル名だけである");
    assert_eq!(origin, Origin::File(path.clone()));
    assert!(!unsaved, "読み込みの完了で未保存でない状態になる");
    assert_eq!(sheets.len(), 1, "標本は 1 シートである");
    assert_eq!(sheets[0].name, "標本");
    assert_eq!(sheets[0].columns, 3);
    assert_eq!(sheets[0].rows, 4);

    let rows = sessions
        .read(&main, &mut |document| document.sheets()[0].rows().len())
        .expect("読み取れる");
    assert_eq!(rows, 4);
    assert_eq!(sessions.may_close(&main), CloseAnswer::Allow);
}

/// `edit` が版と未保存を進め、`save` がその内容を出所へ書き出し、書き出したバイト列が
/// 形式の側へ直接書き出したバイト列と一致する（tasks.md 2.5。要件 3.1, 3.2, 4.1, 4.5, 5.7）。
#[test]
fn edit_records_revision_and_save_writes_the_same_bytes_as_a_direct_write() {
    let scratch = common::Scratch::new("public-edit-save");
    // 同一の内容を持つ 2 つの位置を用意する（片方をセッションが読み、もう片方を直接書き出す）。
    let pristine = write_sample(&scratch, "pristine.jxcel", 4, 3);
    let session_path = scratch.file("session.jxcel");
    fs::copy(&pristine, &session_path).expect("同一の内容を複製できる");

    let sessions = DocumentSessions::new();
    let main = window("main");
    sessions.resolve(&main, Some(&session_path)).expect("読み込める");

    // 1 回の閉包が 1 回の適用である。上流の一括の書き換えを閉包の内側で呼ぶ。
    let edited = sessions
        .edit(&main, &mut set_first_cell(CellValue::Int(42)))
        .expect("適用できる");
    assert!(edited.value, "閉包の戻り値がそのまま返る");
    // 版は**文書が入れ替わるか変更が適用されたとき**に 1 進む（design.md「Slot」の不変条件）。
    // このウィンドウでは解決（読み込みの完了 = 差し替え）が 1 を数え、この適用が 2 つ目である。
    assert_eq!(edited.revision, 2, "解決の 1 に、適用の 1 が積まれる");
    assert!(edited.unsaved, "適用のあとは未保存である");

    let (_, _, unsaved, _) = opened(sessions.state(&main));
    assert!(unsaved);
    assert_eq!(sessions.may_close(&main), CloseAnswer::Deny);

    let SaveReport::Saved { location } = sessions.save(&main).expect("保存できる") else {
        panic!("出所があるため保存に成功するはず");
    };
    assert_eq!(location, session_path);

    // 保存のバイト列は、形式の側へ同じ変更を適用して直接書き出したバイト列と一致する。
    let direct = scratch.file("direct.jxcel");
    let mut document = common::api().open(&pristine).expect("原本を開ける").document;
    let mut apply = set_first_cell(CellValue::Int(42));
    apply(&mut document);
    common::api().save(&document, &direct).expect("直接書き出せる");
    assert_eq!(
        fs::read(&session_path).expect("保存したファイルを読める"),
        fs::read(&direct).expect("直接書き出したファイルを読める"),
    );

    // 保存の成功で未保存が落ち、閉じてよいへ変わる。
    let (_, _, unsaved, _) = opened(sessions.state(&main));
    assert!(!unsaved);
    assert_eq!(sessions.may_close(&main), CloseAnswer::Allow);

    // 2 回目の保存も同じ内容を同じ位置へ書き出す（要件 5.8）。
    let SaveReport::Saved { location } = sessions.save(&main).expect("保存できる") else {
        panic!("出所があるため保存に成功するはず");
    };
    assert_eq!(location, session_path);
}

/// `attach` が利用者の選んだ位置へ文書を差し替え、未保存のときは拒否する（tasks.md 2.5。
/// 要件 1.3, 2.2, 2.4）。
#[test]
fn attach_replaces_the_document_and_is_refused_while_unsaved() {
    let scratch = common::Scratch::new("public-attach");
    let first = write_sample(&scratch, "first.jxcel", 3, 2);
    let second = write_sample(&scratch, "second.jxcel", 5, 4);

    let sessions = DocumentSessions::new();
    let main = window("main");
    sessions.resolve(&main, Some(&first)).expect("読み込める");

    // 別の位置を渡すと差し替わる（未保存でないため受け付ける）。
    sessions.attach(&main, &second).expect("引き渡せる");
    let (name, origin, unsaved, sheets) = opened(sessions.state(&main));
    assert_eq!(name, "second.jxcel");
    assert_eq!(origin, Origin::File(second.clone()));
    assert!(!unsaved);
    assert_eq!(sheets[0].rows, 5);
    assert_eq!(sheets[0].columns, 4);

    // 未保存のときは差し替えを拒み、保持している内容を変えない。
    let edited = sessions
        .edit(&main, &mut set_first_cell(CellValue::Int(7)))
        .expect("適用できる");
    assert!(edited.unsaved);
    assert!(matches!(
        sessions.attach(&main, &first),
        Err(SessionError::UnsavedChanges)
    ));
    let (name, origin, _, _) = opened(sessions.state(&main));
    assert_eq!(name, "second.jxcel");
    assert_eq!(origin, Origin::File(second));
}

/// `create` が空のドキュメントを用意し、未保存なら拒否し、`discard` が印を落として
/// `may_close` の答えを変え、出所の無い保存は `save_to` を要する（tasks.md 2.5。要件 5.2,
/// 5.8, 6.5, 6.6, 7.1, 7.2, 7.3）。
#[test]
fn create_makes_an_empty_document_and_discard_clears_the_unsaved_mark() {
    let scratch = common::Scratch::new("public-create");
    let sessions = DocumentSessions::new();
    let main = window("main");

    sessions.create(&main).expect("新規作成できる");
    let (name, origin, unsaved, sheets) = opened(sessions.state(&main));
    assert_eq!(name, "", "新規の文書は出所を持たない");
    assert_eq!(origin, Origin::New);
    assert!(!unsaved);
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].rows, 0);
    assert_eq!(sheets[0].columns, 0);

    // 出所が無いため、保存は保存先の選択を要する（提示は適応層の仕事である）。
    assert!(matches!(
        sessions.save(&main).expect("保存先が要る"),
        SaveReport::NeedsLocation
    ));

    sessions
        .edit(&main, &mut |document| {
            let sheet = document.sheets()[0].id();
            document.set_cells(sheet, &[]).expect("空の一括適用");
        })
        .expect("適用できる");
    assert_eq!(sessions.may_close(&main), CloseAnswer::Deny);
    assert!(matches!(
        sessions.create(&main),
        Err(SessionError::UnsavedChanges)
    ));

    // 破棄の印で未保存が落ち、閉じてよいへ変わり、以後の新規作成も受け付ける。
    sessions.discard(&main).expect("破棄の印を落とせる");
    assert_eq!(sessions.may_close(&main), CloseAnswer::Allow);
    let (_, _, unsaved, _) = opened(sessions.state(&main));
    assert!(!unsaved);
    sessions.create(&main).expect("再度作れる");

    // 選ばれた位置へ保存すると、以後の出所はその位置になる（要件 5.8）。
    let chosen = scratch.file("chosen.jxcel");
    let SaveReport::Saved { location } = sessions.save_to(&main, &chosen).expect("保存できる") else {
        panic!("選ばれた位置へ保存に成功するはず");
    };
    assert_eq!(location, chosen);
    let (_, origin, _, _) = opened(sessions.state(&main));
    assert_eq!(origin, Origin::File(chosen.clone()));
    let SaveReport::Saved { location } = sessions.save(&main).expect("保存できる") else {
        panic!("出所があるため保存に成功するはず");
    };
    assert_eq!(location, chosen);
}

/// `state` / `may_close` / `forget` は未解決のウィンドウにセッションを作らず、その答えは
/// `Absent` / `Allow` のままであり、`forget` は破棄されたウィンドウのセッションを手放す
/// （tasks.md 2.5。要件 1.4, 1.5, 1.8）。
#[test]
fn state_may_close_and_forget_leave_an_unresolved_window_absent() {
    let sessions = DocumentSessions::new();
    let untouched = window("untouched");

    assert_eq!(sessions.may_close(&untouched), CloseAnswer::Allow);
    assert_eq!(sessions.state(&untouched), SessionState::Absent);
    sessions.forget(&untouched);
    assert_eq!(sessions.state(&untouched), SessionState::Absent);
    assert_eq!(sessions.may_close(&untouched), CloseAnswer::Allow);

    // セッションを作らないため、保持していないウィンドウへの操作は失敗する（要件 1.8）。
    assert!(matches!(
        sessions.read(&untouched, &mut |_document| ()),
        Err(SessionError::NoDocument)
    ));
    assert!(matches!(
        sessions.edit(&untouched, &mut |_document| ()),
        Err(SessionError::NoDocument)
    ));
    assert!(matches!(sessions.save(&untouched), Err(SessionError::NoDocument)));
    assert!(matches!(
        sessions.save_to(&untouched, Path::new("/tmp/never-written.jxcel")),
        Err(SessionError::NoDocument)
    ));
    assert!(matches!(sessions.discard(&untouched), Err(SessionError::NoDocument)));

    // 破棄されたウィンドウのセッションは忘れられる（他のウィンドウには触れない）。
    let scratch = common::Scratch::new("public-forget");
    let path = write_sample(&scratch, "gone.jxcel", 2, 2);
    let open = window("open");
    sessions.resolve(&open, Some(&path)).expect("読み込める");
    let (name, _, _, _) = opened(sessions.state(&open));
    assert_eq!(name, "gone.jxcel");
    sessions.forget(&open);
    assert_eq!(sessions.state(&open), SessionState::Absent);
    assert!(matches!(
        sessions.read(&open, &mut |_document| ()),
        Err(SessionError::NoDocument)
    ));
    // 未解決のウィンドウの答えは、破棄のあとも変わらない。
    assert_eq!(sessions.state(&untouched), SessionState::Absent);
    assert_eq!(sessions.may_close(&untouched), CloseAnswer::Allow);
}

/// 操作口が `Send + Sync` である（適応層が `Arc` で 1 実体を保持できる。tasks.md 2.5）。
#[test]
fn document_sessions_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DocumentSessions>();
}

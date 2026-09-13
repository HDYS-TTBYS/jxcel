//! 検証専用の引き金（セッション経路） — 読み込み → 1 回の一括の適用 → 保存 → 閉じてよいかの答え
//! （tasks.md 5.2。design.md「Testing Strategy → E2E / 3 OS の観測」。要件 8.2、8.3）。
//!
//! # 何のために在るか
//!
//! 要件 8.2 は「3 つの OS のそれぞれについて、ドキュメントを読み込み、変更を適用し、保存した
//! 結果を、**実際に起動して観測した結果**で確認できるようにする」ことを求める。観測するのは
//! 5.3 の検査器（`scripts/check-document-session.sh` と 3 OS の段）であり、**配布物を起動して
//! 人が操作するわけにはいかない** — 画面を操作する手段を持たない CI ランナーでも同じ経路を
//! 踏めなければならない。
//!
//! 通常のビルドには、この経路を踏む入口が無い。起動時に指定されたドキュメントの読み込みは
//! `document_state` の問い合わせが引き金になる（タスク 3.4）が、**変更の適用と保存は画面
//! （データグリッド。4.x）がまだ無い**。そこで本モジュールが、起動から終了までの 1 回の走行と
//! して「読み込み → 1 回の一括の適用 → 保存 → 状態と閉じてよいかの答え」を行い、その事実を
//! 記録へ **1 事実 1 行**で残す。
//!
//! # 引き金の文法（design.md の逐語）
//!
//! 環境変数 [`VERIFY_SESSION_ENV`] の値は `open,edit,<行数>,save` である（design.md
//! 「Testing Strategy → E2E / 3 OS の観測」）。4 つの要素は順に:
//!
//! 1. `open` — 起動引数で指定された位置からドキュメントを読み込む
//! 2. `edit` — 一括の適用を行う
//! 3. `<行数>` — 書き換える行数（**1 以上**。0 行の書き換えは「変更が保存へ届いた」ことを
//!    示せないので解釈できない値として扱う）
//! 4. `save` — 出所（読み込んだファイル）へ保存する
//!
//! **引き金は環境変数 1 系統**である（`.kiro/steering/verification.md`「引き金（環境変数）の
//! 語彙」）。本モジュールは既存の一族（`JXCEL_VERIFICATION_*`）へ 1 つ足すだけで、新しい仕組みを
//! 作らない。終了の引き金（`JXCEL_VERIFICATION_EXIT_AFTER_MS`）は `lifecycle.rs` が持ち、
//! 本モジュールは触らない — 走行を終わらせるのは検査器の側の関心である（時間切れで終了させるか、
//! 検査器がプロセスを終了する）。
//!
//! # 実行の位置と順序（なぜ `RunEvent::Ready` から待つのか）
//!
//! `lifecycle::handle_run_event` が `RunEvent::Ready` で [`arm`] を呼ぶ（既存の
//! `arm_verification_exit_trigger` の隣）。この時点では起動時のウィンドウは**まだ存在しない** —
//! [`crate::window::open`] は生成を非同期ランタイムへ逃がす（同期文脈で
//! `WebviewWindowBuilder::build()` を呼ぶと Windows でデッドロックするため。design.md
//! 「WindowManager」）。したがって [`arm`] は走行を `spawn_blocking` へ渡し、
//! [`WindowRegistry::first_ready_label`] が出来上がり、**そのラベルの生成要求の位置が読める**
//! ようになるまで待つ。これが design.md の「起動時にウィンドウへ指定された位置は、シェルが保持
//! するウィンドウの生成要求から読む」（要件 1.2 の前半）の実体である。
//!
//! # 本体を写さない（既存の入口をそのまま通す）
//!
//! 走行は**コマンド面と同じ本体**を使う:
//!
//! - 読み込み: [`answer_state`]（解決 + 境界の状態 + 「状態が変わったか」の判定）。解決は冪等
//!   なので、フロントエンドが先に問い合わせていても読み込みは 1 度しか起きない（要件 1.2 の
//!   後半）
//! - 状態変化の通知: [`emit_session_changed`]。**送るかどうかの規則はコマンドの側の 1 箇所に
//!   保つ** — 本体が返した真偽だけをここが渡す（規則を 2 つ持たない）
//! - 保存: [`answer_save`]。**保存先の提示は渡された閉包が担う**ため、取り消し・提示不能の
//!   扱いも写さずに済む。本モジュールは**提示を行わない閉包**を渡す（起動引数のドキュメントは
//!   出所を持つので提示は呼ばれない。呼ばれたなら「引き金は提示をしない」という理由つきの
//!   失敗になる）
//!
//! 一括の適用だけはコマンド面に対応する本体が無い — 変更の適用はデータグリッド（4.x）の所有で
//! あり、3.4 の 4 コマンドに含まれない。したがって**コアの適用の口**
//! （[`DocumentSessionsApi::edit`]）を直接呼ぶ。**閉包の内側からセッションを呼び返さない**
//! （再入禁止。`document-session` の `session` の doc）— 本モジュールの閉包は文書だけを触る。
//!
//! # 一括の適用の作り方（なぜ値を回転で作るのか）
//!
//! 要件 3.5 の形（**1 回の適用 = 1 回の `set_cells`**）で `<行数>` 行を書き換える。書き換える
//! セル（行識別子・列の添字・値）は、**先頭のシートの行を読み、選択した行の中で値を 1 つ回転
//! させて**組み立てる:
//!
//! ```text
//! 対象の行 i（i < 行数） ← 行 (i + 1) % 読み込んだ行数 の値（列ごと）
//! ```
//!
//! こうする理由は 2 つある。
//!
//! 1. **値の型をこのクレートへ持ち込まない。** `src-tauri` は `document-format` を**通常依存に
//!    持たない**（テストだけが dev-dependency として使う。`src-tauri/Cargo.toml` の依存方針）。
//!    `CellValue` を名指しすれば通常依存が要る。既存の行の値を複製すれば型は `set_cells` の
//!    引数から推論されるので、**`document-format` の名前を 1 つも書かずに済む**
//! 2. **1.4 の標本の生成器は使えない。** それは
//!    `crates/document-session/tests/common/mod.rs` のテスト専用のモジュールであり、`src-tauri`
//!    のビルドにもテストにも入らない
//!
//! 書き換えるのは**選択した行だけ**であり、選択の外の行の値は読むだけで変えない。次の 4 つは
//! **適用も保存も行わずに**理由を記録して止まる（「変更が保存へ届いた」を満たせない入力で、
//! 成功したように見える記録を残さない）:
//!
//! - 先頭のシートが無い・列が無い
//! - 引き金の行数が読み込んだ行数を超えている
//! - 読み込んだ行数が 2 未満（1 行では回転が恒等写像になる）
//! - 回転しても内容が 1 つも変わらない（選択した行がすべて同じ値である）
//!
//! # 記録（検査器が数える 1 行ずつ）
//!
//! 1 つの事実を 1 行で記録する（`.kiro/steering/verification.md`「主張は区切りの後ろに限定する」
//! — 検査器は走行の直前に取った行数より後ろだけを数える）。行の先頭は常に `[検証] セッション: `
//! であり、検査器は日本語の見出しで照合できる（自由な散文を解釈させない）:
//!
//! | 事実 | 行（`N` は数値） |
//! |---|---|
//! | 引き金を読んだ | `引き金を読んだ: 書き換える行数 = N` |
//! | 読み込み（行数つき） | `読み込んだ行数 = N` |
//! | 適用（版と行数） | `一括の適用を 1 回行った: 版 = N / 行数 = N / セル数 = N` |
//! | 保存（バイト数） | `保存した: バイト数 = N` |
//! | 閉じてよいかの答え（保存の前） | `閉じてよいかの答え（保存の前） = 拒否` |
//! | 閉じてよいかの答え（保存の後） | `閉じてよいかの答え（保存の後） = 許可` |
//!
//! **適用の行は 1 回だけ現れる。** 版は読み込みの完了で 1 に進み、適用で 2 になる（1 回の一括が
//! 1 回の適用として運ばれたことの証拠。design.md「Slot」の不変条件）。失敗した走行は
//! `引き金の走行を完了できなかった: <理由>` を 1 行だけ残す。
//!
//! # 既定のビルドには識別子すら残らない
//!
//! 本モジュール全体（[`arm`] の呼び出しを含む）は `verification-triggers` feature の下にだけ
//! コンパイルされる。既定のビルド（配布物）には [`VERIFY_SESSION_ENV`] の文字列も環境変数の
//! 読み取りも入らない（`.kiro/steering/verification.md`「検証専用のコードの置き場」。既定の
//! ビルドで引き金が働かないことは 5.3 の段の負の対照でもある）。
//!
//! # ローカルで閉じられないもの
//!
//! macOS / Windows での実行はこの開発機では行えない（5.3 の CI の段が担う）。5.3 の検査器そのもの
//! も本タスクの外である — 本モジュールはその要求を満たす記録行を与えるだけである。

use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use app_shell::ipc::{DocumentSaveOutcome, DocumentSessionStatus, WindowLabel};
use document_session::{CloseAnswer, DocumentSessionsApi, Origin, SessionError, SessionState};
use tauri::{AppHandle, Manager};
use tauri_plugin_log::log;

use crate::dialog::SaveLocation;
use crate::session::commands::{
    answer_save, answer_state, describe_save, describe_status, emit_session_changed,
};
use crate::session::watch::WindowDestroyWatch;
use crate::window::WindowRegistry;

/// 検証専用の引き金が読む環境変数の名前（design.md「E2E / 3 OS の観測」の逐語）。
///
/// **通常の利用環境に存在しないことを狙った名前である。** 値の文法は `open,edit,<行数>,save` で
/// あり、設定されているときだけ [`arm`] が走行を用意する（解釈できない値は無視する — 配布物の
/// 既定の振る舞いを変えない）。
///
/// **この定数は `verification-triggers` feature の下にのみ存在する。** 既定のビルドのバイナリには
/// この文字列が 1 つも現れない（`verification.md` の規約。実測は tasks.md 5.2 の完了状態）。
const VERIFY_SESSION_ENV: &str = "JXCEL_VERIFICATION_SESSION";

/// 起動時のウィンドウが現れるのを待つ回数と間隔（[`arm`] の doc を参照）。
///
/// 生成は非同期であり、ネイティブのウィンドウが出来てからでないと生成要求の位置が読めない。
/// 600 × 50 ms = 30 秒を上限にする — ウィンドウの生成が 30 秒かかる環境では、そもそも
/// 検査の対象（起動と描画）が成立していない。**待ち切れなかったことを記録に残して止まる**
/// （無言で何もしないと、引き金を読んだ行だけが残って「働いた」ように見える）。
const WINDOW_POLL_ATTEMPTS: usize = 600;

/// 待ちの間隔（[`WINDOW_POLL_ATTEMPTS`] と合わせて 30 秒）。
const WINDOW_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// 待ちの合計（[`WINDOW_POLL_ATTEMPTS`] × [`WINDOW_POLL_INTERVAL`] の秒数）。
///
/// **[`WINDOW_POLL_INTERVAL`] はミリ秒単位なので `as_secs` では 0 になる** — 「0 秒以内に
/// 現れなかった」という意味の通らない記録を出さないため、掛け算した秒数をここ 1 箇所で作る
/// （待ちの上限を変えたときに記録と実際が食い違わない）。
const WINDOW_POLL_SECONDS: u64 =
    (WINDOW_POLL_ATTEMPTS as u64) * (WINDOW_POLL_INTERVAL.as_millis() as u64) / 1000;

/// 引き金の値（文法を解釈した結果）。
///
/// いまは書き換える行数だけを運ぶ。**将来の要素を足すときも「1 つの環境変数の値を解釈する 1 つ
/// の純粋関数」を増やさない** — 要素を増やすならこの型と [`parse_session_trigger`] を同じ作業で
/// 広げる（検査器が読む記録行の意味が変わるためである）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionTrigger {
    /// 書き換える行数（1 以上）。
    rows: usize,
}

/// `VERIFY_SESSION_ENV` の値を解釈する。**純粋関数**であり、文法をテストで固定する
/// （`tests::the_trigger_grammar_is_open_edit_rows_save`。`lifecycle::parse_verification_trigger`
/// と同じ規律 — 解釈できない値は `None` を返し、呼び出し元が無視する）。
///
/// 文法は `open,edit,<行数>,save` の**ちょうど 4 要素**である。要素の前後の空白は無視する
/// （既存の引き金の一族と同じ扱い）。次は解釈できない:
///
/// - 要素が 4 つでない（欠けている・余分がある）
/// - `open` / `edit` / `save` の綴りが違う（順序も固定である）
/// - `<行数>` が 10 進の整数として読めない、または 0 である
fn parse_session_trigger(value: &str) -> Option<SessionTrigger> {
    let mut elements = value.split(',');
    let open = elements.next()?.trim();
    let edit = elements.next()?.trim();
    let rows = elements.next()?.trim();
    let save = elements.next()?.trim();
    if elements.next().is_some() {
        return None;
    }
    if open != "open" || edit != "edit" || save != "save" {
        return None;
    }
    let rows = rows.parse::<usize>().ok()?;
    if rows == 0 {
        return None;
    }
    Some(SessionTrigger { rows })
}

/// 引き金の 1 回の走行の結果（記録した事実と同じ内容を値としても返す）。
///
/// 記録行はログの側にしか残らないため、**走行の意味をテストで固定できるよう**に値を返す
/// （`tests::the_trigger_loads_applies_once_and_saves` が版・行数・バイト数・答えを直接見る）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionTriggerOutcome {
    /// 読み込んだ先頭のシートの行数（記録行「読み込んだ行数」）。
    rows_loaded: usize,
    /// 一括の適用のあとの版（読み込みの完了で 1、適用で 2）。
    revision: u64,
    /// 引き金が要求した行数（記録行「適用」の行数）。
    rows_applied: usize,
    /// 1 回の `set_cells` へ渡したセルの数。
    cells_applied: usize,
    /// 保存されたファイルのバイト数（記録行「保存した」）。
    bytes_saved: u64,
    /// 保存の前の「閉じてよいか」（適用で未保存が立っているので拒否が正しい）。
    may_close_before: CloseAnswer,
    /// 保存の後の「閉じてよいか」（保存の成功で未保存が落ちるので許可が正しい）。
    may_close_after: CloseAnswer,
}

/// 引き金の走行が完了できなかった理由。
///
/// **表示用の文言を持たせない**規律（`document-session` の誤り型）をここでは適用しない —
/// これは記録へ 1 行を出すための検証専用の型であり、利用者へ提示されない。`Session` は
/// コアの誤りをそのまま運び（写して文言を作らない）、`Unmet` は「引き金の要求を観測対象が
/// 満たせない」というこの走行だけの理由を運ぶ。
#[derive(Debug)]
enum TriggerFailure {
    /// コアのセッション操作（読み取り・適用）が失敗した。
    Session(SessionError),
    /// 引き金の要求を観測対象が満たせない（読み込めなかった・行数不足・書き換えの失敗）。
    Unmet(String),
    /// 保存が完了しなかった（取り消し・書き出しの失敗）。
    Save(DocumentSaveOutcome),
}

impl fmt::Display for TriggerFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(error) => write!(f, "セッションの操作に失敗した: {error}"),
            Self::Unmet(reason) => f.write_str(reason),
            Self::Save(outcome) => write!(f, "保存が完了しなかった: {}", describe_save(outcome)),
        }
    }
}

impl From<SessionError> for TriggerFailure {
    fn from(error: SessionError) -> Self {
        Self::Session(error)
    }
}

/// 閉じてよいかの答えを記録に出す 1 語（`拒否` / `許可`）。
///
/// 検査器は保存の前後で答えが変わったこと（拒否 → 許可）を要求する。**2 値しか無い**ので、
/// 語をここ 1 箇所で決める。
fn describe_close(answer: CloseAnswer) -> &'static str {
    match answer {
        CloseAnswer::Allow => "許可",
        CloseAnswer::Deny => "拒否",
    }
}

/// 引き金を用意する（`lifecycle::handle_run_event` の `RunEvent::Ready` が呼ぶ）。
///
/// 環境変数が無ければ**関数の先頭で即座に戻るので何も起きない**（解釈できない値のときも何もしない。
/// 配布物の既定の振る舞いを変えない）。値があれば「引き金を読んだ」を記録し、走行を
/// `spawn_blocking` へ渡す — 読み込み・適用・保存は秒単位かかりうるので、イベントループと
/// ランタイムのワーカーを塞いではならない（`lifecycle::arm_verification_exit_trigger` と同じ形。
/// あちらは待ちを `std::thread` に置くが、こちらは**セッションの読み書きを伴う**ため、
/// ランタイムの閉塞用スレッドへ渡す）。
///
/// **この関数は検証ビルドにしか存在しない**（`verification-triggers` feature）。既定のビルドには
/// 環境変数の読み取りもこの経路も入らない。
pub fn arm(app: &AppHandle) {
    let Ok(value) = std::env::var(VERIFY_SESSION_ENV) else {
        return;
    };
    let Some(trigger) = parse_session_trigger(&value) else {
        log::warn!("{VERIFY_SESSION_ENV} を解釈できないので無視する: {value:?}");
        return;
    };
    // **走行の前に記録する。** 検査器はこの行で「引き金が読まれたこと」を確かめ、配布物では
    // この行が 1 つも現れないことを負の対照にする（空振りで緑にしないための錠前である）。
    log::info!(
        "[検証] セッション: 引き金を読んだ: 書き換える行数 = {}",
        trigger.rows,
    );
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || drive(&app, trigger));
}

/// ウィンドウが現れるのを待ち、走行を 1 回行う（[`arm`] が渡す側）。
///
/// 待ち切れなかったことと走行の失敗は**記録に残す**（検証の失敗を無言にしない）。成功の記録は
/// 走行の側が出す（[`run_session_trigger`] の各事実と、最後の「閉じてよいかの答え（保存の後）」）。
fn drive(app: &AppHandle, trigger: SessionTrigger) {
    let Some((label, requested)) = await_startup_window(app) else {
        return;
    };
    let watch = Arc::clone(&app.state::<Arc<WindowDestroyWatch>>());
    // 状態変化の通知はコマンド面と同じ関数を通す（**送るかどうかの規則を写さない**）。送り先の
    // ウィンドウは生成済みのはずだが、引けなければ送らない（`emit_session_changed` の契約と同じ
    // 「送れなくても操作の結果は変えない」）。
    let window = app.get_webview_window(label.as_str());
    let mut on_changed = move || {
        if let Some(window) = &window {
            emit_session_changed(window);
        }
    };
    let outcome = run_session_trigger(
        &watch,
        &label,
        &requested,
        trigger.rows,
        // **提示を行わない閉包**を渡す。起動引数のドキュメントは出所を持つので `answer_save` は
        // これを呼ばない（呼ばれたなら、その事実が理由つきの失敗として記録に残る）。
        |suggested: &str| {
            SaveLocation::Unavailable(format!(
                "検証専用の引き金は保存先を提示しない（提案名 = {suggested}）"
            ))
        },
        &mut on_changed,
    );
    if let Err(failure) = outcome {
        log::error!("[検証] セッション: 引き金の走行を完了できなかった: {failure}");
    }
}

/// 生成が完了したウィンドウのラベルと、**その生成要求の位置**を待つ。
///
/// [`WindowRegistry::first_ready_label`] はネイティブのウィンドウが存在する登録だけを返す
/// （生成中の登録は返さない）。位置は [`WindowRegistry::document_of`] から読む — **位置を読める
/// のは生成要求を保持するシェルだけ**であり、これが「読み込みを起こしてよい唯一の場所」である
/// （要件 1.2）。
///
/// ドキュメントを指定しない起動では、引き金の対象が無い（保存が要る新規文書の保存先の提示は
/// 引き金の仕事ではない）。**その事実も記録に残して止まる**。
fn await_startup_window(app: &AppHandle) -> Option<(WindowLabel, std::path::PathBuf)> {
    for _ in 0..WINDOW_POLL_ATTEMPTS {
        {
            let registry = app.state::<WindowRegistry>();
            if let Some(label) = registry.first_ready_label() {
                if app.get_webview_window(label.as_str()).is_some() {
                    let Some(requested) = registry.document_of(label.as_str()) else {
                        log::error!(
                            "[検証] セッション: 起動引数にドキュメントが指定されていないため、\
                             引き金の対象が無い（ドキュメントを指定して起動すること）"
                        );
                        return None;
                    };
                    return Some((label, requested));
                }
            }
        }
        std::thread::sleep(WINDOW_POLL_INTERVAL);
    }
    log::error!(
        "[検証] セッション: 引き金の対象になるウィンドウが {WINDOW_POLL_SECONDS} 秒以内に現れなかった",
    );
    None
}

/// 走行の本体（Tauri に依らない。テストはこれを直接駆動する）。
///
/// 手順は design.md の逐語の 5 段である:
///
/// 1. **読み込み** — [`answer_state`] に生成要求の位置を渡す（コマンド面と同じ本体）
/// 2. **適用** — 先頭のシートの選択した行を、**1 回の `edit` と 1 回の `set_cells`** で書き換える
/// 3. **閉じてよいか（保存の前）** — 適用で未保存が立っているので拒否が正しい答えである
/// 4. **保存** — [`answer_save`] に提示しない閉包を渡す（出所があるのでそのまま書き出される）
/// 5. **閉じてよいか（保存の後）** — 保存の成功で未保存が落ちるので許可が正しい答えである
///
/// `on_changed` は本体が「状態が変わった」と答えたときに 1 回ずつ呼ぶ（読み込みと保存の 2 回が
/// 期待値である。**送る規則そのものはコマンドの側にあり、ここは結果を渡すだけ**）。
fn run_session_trigger<P>(
    watch: &WindowDestroyWatch,
    label: &WindowLabel,
    requested: &Path,
    rows: usize,
    pick: P,
    on_changed: &mut dyn FnMut(),
) -> Result<SessionTriggerOutcome, TriggerFailure>
where
    P: FnOnce(&str) -> SaveLocation,
{
    // 1. 読み込み（コマンド面と同じ本体）。
    let (status, changed) = answer_state(watch, label, Some(requested));
    if changed {
        on_changed();
    }
    if !matches!(status, DocumentSessionStatus::Open(_)) {
        return Err(TriggerFailure::Unmet(format!(
            "引き金の対象のドキュメントを保持できなかった（状態 = {}）",
            describe_status(&status),
        )));
    }

    // 2. 先頭のシートの形を読む（列数と行数）。**セッションの読み取りの口**を通す。
    let shape = watch.sessions().read(label, &mut |document| {
        document
            .sheets()
            .first()
            .map(|sheet| (sheet.columns().len(), sheet.rows().len()))
    })?;
    let Some((columns, rows_loaded)) = shape else {
        return Err(TriggerFailure::Unmet("文書にシートが無い".to_owned()));
    };
    log::info!("[検証] セッション: 読み込んだ行数 = {rows_loaded}");
    if columns == 0 {
        return Err(TriggerFailure::Unmet("先頭のシートに列が無い".to_owned()));
    }
    if rows_loaded < 2 {
        return Err(TriggerFailure::Unmet(format!(
            "行の値を回転させるには 2 行以上要る（読み込んだ行数 = {rows_loaded}）"
        )));
    }
    if rows > rows_loaded {
        return Err(TriggerFailure::Unmet(format!(
            "引き金の行数 {rows} が読み込んだ行数 {rows_loaded} を超えている"
        )));
    }

    // 3. 一括の適用（1 回の `edit` の中の 1 回の `set_cells`。要件 3.5 の形）。
    let edited = watch.sessions().edit(label, &mut |document| {
        // 借用を分けるため、まず読み取りだけで書き換えるセルを組み立てる（値は既存の行から
        // 複製するので、`document-format` の名前をここに書かない。モジュール doc
        // 「一括の適用の作り方」）。
        let Some(sheet) = document.sheets().first() else {
            return Err("文書にシートが無い".to_owned());
        };
        let sheet_id = sheet.id();
        let total = sheet.rows().len();
        let columns = sheet.columns().len();
        if total < 2 || columns == 0 {
            return Err("シートの形が書き換えに足りない".to_owned());
        }
        let mut cells = Vec::with_capacity(rows.saturating_mul(columns));
        let mut changed_cells = 0usize;
        for position in 0..rows {
            let (Some(target), Some(source)) = (
                sheet.rows().get(position),
                sheet.rows().get((position + 1) % total),
            ) else {
                return Err(format!("行 {position} を読めなかった"));
            };
            for column in 0..columns.min(source.values().len()) {
                let value = source.values()[column].clone();
                if target.values().get(column) != Some(&value) {
                    changed_cells += 1;
                }
                cells.push((target.id(), column, value));
            }
        }
        if changed_cells == 0 {
            // **恒等な書き換えを「適用した」と記録しない**（保存のバイト数が変わらない入力を
            // 成功として通すと、5.3 の検査器の錠前が意味を失う）。
            return Err(
                "行の値を回転しても内容が変わらない（選択した行がすべて同じ値である）".to_owned(),
            );
        }
        document
            .set_cells(sheet_id, &cells)
            .map_err(|error| error.to_string())?;
        Ok(cells.len())
    })?;
    // 閉包が失敗を返しても版と未保存は立つ（保守側に倒す記録。`document-session` の契約）。
    // したがって**成功したときだけ**記録する。
    let cells_applied = edited.value.map_err(TriggerFailure::Unmet)?;
    log::info!(
        "[検証] セッション: 一括の適用を 1 回行った: 版 = {} / 行数 = {rows} / セル数 = {cells_applied}",
        edited.revision,
    );

    // 4. 閉じてよいか（保存の前）。保存の直前の答えを記録する（適用で未保存が立っている）。
    let may_close_before = watch.sessions().may_close(label);
    log::info!(
        "[検証] セッション: 閉じてよいかの答え（保存の前） = {}",
        describe_close(may_close_before),
    );

    // 5. 保存（コマンド面と同じ本体）。
    let (outcome, changed) = answer_save(watch, label, pick);
    if changed {
        on_changed();
    }
    if !matches!(outcome, DocumentSaveOutcome::Saved) {
        return Err(TriggerFailure::Save(outcome));
    }
    let bytes_saved = saved_bytes(watch, label)?;
    log::info!("[検証] セッション: 保存した: バイト数 = {bytes_saved}");

    // 6. 閉じてよいか（保存の後）。保存の成功で未保存が落ちる（要件 5.5）。
    let may_close_after = watch.sessions().may_close(label);
    log::info!(
        "[検証] セッション: 閉じてよいかの答え（保存の後） = {}",
        describe_close(may_close_after),
    );

    Ok(SessionTriggerOutcome {
        rows_loaded,
        revision: edited.revision,
        rows_applied: rows,
        cells_applied,
        bytes_saved,
        may_close_before,
        may_close_after,
    })
}

/// 保存の直後の出所のファイルの大きさを読む（記録行「保存した: バイト数 = N」の材料）。
///
/// 保存は形式の側の原子的な書き込み（一時ファイル → `rename`）で行われるため、この時点で
/// 出所の位置には**書き出し済みの完全なファイル**がある。位置は状態の出所から取り出す —
/// 応答（境界）へ位置を出さない規律は本モジュールでも同じであり、記録にも位置は書かない
/// （数えるのはバイト数だけである）。
fn saved_bytes(watch: &WindowDestroyWatch, label: &WindowLabel) -> Result<u64, TriggerFailure> {
    let location = match watch.sessions().state(label) {
        SessionState::Open {
            origin: Origin::File(path),
            ..
        } => path,
        other => {
            return Err(TriggerFailure::Unmet(format!(
                "保存のあとの出所がファイルでない（状態 = {})",
                describe_state(&other),
            )))
        }
    };
    std::fs::metadata(&location)
        .map(|metadata| metadata.len())
        .map_err(|error| {
            TriggerFailure::Unmet(format!("保存されたファイルの大きさを読めなかった: {error}"))
        })
}

/// 状態の写しを記録用の 1 語へ写す（`保存のあとの出所` の診断だけが使う）。
///
/// 境界の状態（[`DocumentSessionStatus`]）ではなく**コアの状態**（[`SessionState`]）を受けるのは、
/// ここが保存の直後の出所を見るためである。境界の語彙（`describe_status`）はコマンド面の
/// ものであり、写像をここで作り直さず、状態の綴りをそのまま出す。
fn describe_state(state: &SessionState) -> &'static str {
    match state {
        SessionState::Absent => "Absent",
        SessionState::Open { .. } => "Open",
        SessionState::Unavailable { .. } => "Unavailable",
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use app_shell::ipc::WindowLabel;
    use document_format::{CellValue, Document, DocumentFormat, DocumentFormatApi, SchemaPart};
    use document_session::{DocumentSessions, DocumentSessionsApi, SessionState};

    use super::{
        parse_session_trigger, run_session_trigger, CloseAnswer, SaveLocation, SessionTrigger,
        TriggerFailure,
    };
    use crate::session::watch::testing::AlwaysPresent;
    use crate::session::watch::WindowDestroyWatch;

    /// 一時ディレクトリ（`session/commands.rs` のテストと同じ規律。プロセスごとに一意）。
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
                "jxcel-session-verification-{tag}-{}-{unique}",
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

    /// **本物の文書**を書く（`document-format` は dev-dependency なのでテストでは使える）。
    ///
    /// 値の規則を差し替えられるようにしてあるのは、回転が恒等になる入力（全行が同じ値）を
    /// テストで作るためである。行数は小さい（適用そのものはベンチが 10 万行で測る）。
    fn write_document(
        path: &Path,
        rows: usize,
        columns: usize,
        value: impl Fn(usize, usize) -> CellValue,
    ) {
        let mut document = Document::new();
        let sheet = document.add_sheet("検証");
        document
            .set_sheet_columns(
                sheet,
                (0..columns).map(|column| format!("列{column}")).collect(),
            )
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, SchemaPart::empty())
            .expect("標本のシートは実在する");
        for row in 0..rows {
            let id = document.add_row(sheet).expect("標本のシートは実在する");
            document
                .set_row_values(
                    sheet,
                    id,
                    (0..columns).map(|column| value(row, column)).collect(),
                )
                .expect("標本の行は実在する");
        }
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
    }

    /// 二重のウィンドウの側から入口を作る（テストの標準の組み立て）。
    fn watch() -> WindowDestroyWatch {
        WindowDestroyWatch::new(Arc::new(AlwaysPresent), Arc::new(DocumentSessions::new()))
    }

    /// 文法は `open,edit,<行数>,save` のちょうど 4 要素である（design.md の逐語）。
    #[test]
    fn the_trigger_grammar_is_open_edit_rows_save() {
        assert_eq!(
            parse_session_trigger("open,edit,100,save"),
            Some(SessionTrigger { rows: 100 }),
        );
        // 要素の前後の空白は既存の引き金の一族と同じく無視する。
        assert_eq!(
            parse_session_trigger(" open , edit , 3 , save "),
            Some(SessionTrigger { rows: 3 }),
        );
        // 解釈できない値（無視される。配布物の振る舞いを変えない）。
        for value in [
            "",
            "open",
            "open,edit",
            "open,edit,3",
            "open,edit,3,save,extra",
            "open,edit,3,close",
            "open,save,3,edit",
            "edit,open,3,save",
            "open,edit,0,save",
            "open,edit,-1,save",
            "open,edit,,save",
            "open,edit,三,save",
            "OPEN,EDIT,3,SAVE",
        ] {
            assert_eq!(
                parse_session_trigger(value),
                None,
                "解釈できてしまった: {value:?}",
            );
        }
    }

    /// **引き金は読み込み → 1 回の一括の適用 → 保存を行い、適用の版で 1 回であることを示す。**
    ///
    /// 観測する事実は記録行と同じものである（版・行数・セル数・バイト数・保存の前後の答え）。
    /// 保存先の提示は**呼ばれない**（出所がある文書であるため。要件 5.1）ことも数える。
    #[test]
    fn the_trigger_loads_applies_once_and_saves() {
        let scratch = Scratch::new("once");
        let path = scratch.file("台帳.jxcel");
        write_document(&path, 5, 2, |row, column| {
            CellValue::Text(format!("{row}-{column}"))
        });
        let before = std::fs::read(&path).expect("標本を読める");

        let watch = watch();
        let label = WindowLabel::new("doc-1");
        let pick_calls = Cell::new(0usize);
        let mut changes = 0usize;
        let mut on_changed = || changes += 1;

        let outcome = run_session_trigger(
            &watch,
            &label,
            &path,
            3,
            |_suggested| {
                pick_calls.set(pick_calls.get() + 1);
                SaveLocation::Cancelled
            },
            &mut on_changed,
        )
        .expect("引き金の走行が完了する");

        assert_eq!(5, outcome.rows_loaded);
        assert_eq!(3, outcome.rows_applied);
        assert_eq!(
            6, outcome.cells_applied,
            "3 行 × 2 列が 1 回の適用で運ばれる"
        );
        assert_eq!(
            2, outcome.revision,
            "読み込みの 1 に、一括の適用が 1 回として積まれる（要件 3.5）",
        );
        assert_eq!(
            CloseAnswer::Deny,
            outcome.may_close_before,
            "適用のあとは未保存であり、閉じてよいと答えてはならない（要件 4.1、6.1）",
        );
        assert_eq!(
            CloseAnswer::Allow,
            outcome.may_close_after,
            "保存の成功で未保存が落ちる（要件 5.5、4.6）",
        );
        assert_eq!(
            0,
            pick_calls.get(),
            "出所がある文書で保存先を尋ねた（要件 5.1）"
        );
        assert_eq!(
            2, changes,
            "状態が変わったのは読み込みと保存の 2 回だけである"
        );

        let after = std::fs::read(&path).expect("保存されたファイルを読める");
        assert_ne!(before, after, "変更が保存へ届いていない（要件 5.1）");
        assert_eq!(
            after.len() as u64,
            outcome.bytes_saved,
            "記録するバイト数が実際のファイルと一致しない",
        );
        match watch.sessions().state(&label) {
            SessionState::Open { unsaved, .. } => {
                assert!(!unsaved, "保存のあとに未保存が残っている")
            }
            other => panic!("保持していると期待した: {other:?}"),
        }
    }

    /// **引き金の行数が読み込んだ行数を超えるときは、適用も保存もしない。**
    ///
    /// 記録に「適用した」を残せない入力であり、成功したように見える記録を残してはならない
    /// （5.3 の検査器の錠前が意味を失う）。
    #[test]
    fn the_trigger_declines_a_row_count_the_document_cannot_supply() {
        let scratch = Scratch::new("too-many");
        let path = scratch.file("台帳.jxcel");
        write_document(&path, 2, 2, |row, column| {
            CellValue::Text(format!("{row}-{column}"))
        });
        let before = std::fs::read(&path).expect("標本を読める");

        let watch = watch();
        let label = WindowLabel::new("doc-1");
        let mut on_changed = || {};

        let failure = run_session_trigger(
            &watch,
            &label,
            &path,
            5,
            |_suggested| SaveLocation::Cancelled,
            &mut on_changed,
        )
        .expect_err("行数が足りない引き金は拒否される");

        assert!(
            matches!(failure, TriggerFailure::Unmet(_)),
            "満たせない要求として拒否されていない: {failure}",
        );
        assert_eq!(
            before,
            std::fs::read(&path).expect("標本を読める"),
            "拒否したのにファイルが変わっている",
        );
    }

    /// **回転が恒等になる入力（全行が同じ値）では適用しない。**
    ///
    /// これは「保存のバイト数が変わった」を満たせない入力であり、書き換えたふうの記録を残すより
    /// 止まるほうが正しい（検査器の錠前を無意味にしない）。
    #[test]
    fn the_trigger_declines_when_the_rotation_cannot_change_any_cell() {
        let scratch = Scratch::new("identical");
        let path = scratch.file("台帳.jxcel");
        write_document(&path, 3, 1, |_row, _column| {
            CellValue::Text("同じ".to_owned())
        });
        let before = std::fs::read(&path).expect("標本を読める");

        let watch = watch();
        let label = WindowLabel::new("doc-1");
        let mut on_changed = || {};

        let failure = run_session_trigger(
            &watch,
            &label,
            &path,
            2,
            |_suggested| SaveLocation::Cancelled,
            &mut on_changed,
        )
        .expect_err("内容が変わらない引き金は拒否される");

        assert!(
            matches!(failure, TriggerFailure::Unmet(_)),
            "満たせない要求として拒否されていない: {failure}",
        );
        assert_eq!(
            before,
            std::fs::read(&path).expect("標本を読める"),
            "拒否したのにファイルが変わっている",
        );
    }
}

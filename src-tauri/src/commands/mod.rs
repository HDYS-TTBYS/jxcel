//! `invoke_handler` の根 — 全機能スペックの共有継ぎ目。
//!
//! 所有: `CommandSurface`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 4.1, 4.4, 4.5, 4.6, 4.7, 7.4。
//!
//! # 規約（全機能スペックが守ること）
//!
//! ハンドラの一覧は**コンパイル時に集中して列挙する必要があり、完全な動的登録はできない**。
//! 各機能は自分のモジュールにコマンド関数を持ち、この根はそれを列挙するだけに留める。ここに
//! 業務ロジックが漏れ出したら誤りである（structure.md「共有される継ぎ目」）。**登録の一覧は
//! [`command_root!`] の 1 箇所だけに書き、`tauri::generate_handler!` へも同じ一覧を渡す。**
//! コマンド名は文字列リテラルで書かず、単一の源（`crates/app-shell/src/ipc/command_names.rs` の
//! 名前定数）を参照する（tasks.md 2.2）。
//!
//! すべてのコマンドは [`IpcResult`] を返し、例外に頼らない（要件 4.4）。例外は 2 つある —
//! どちらも **JSON を経由しない生バイトの経路**を要件が求めるためであり、封筒（`serde` で
//! JSON になる）を返せない:
//!
//! 1. [`bulk::bulk_echo`]（タスク 7.2。要件 4.5）— 大きなペイロードを 1 往復で受け渡す。
//! 2. [`grid::grid_rows_window`]（タスク 6.3。要件 1.1、11.2）— 可視範囲の窓を二進で返す。
//!    この経路は**失敗も世代違いも空の窓で表す**（`grid` のモジュール doc）。
//!
//! 例外の根拠と消費者への見え方は `bulk` と `grid` の**それぞれのモジュール doc** に書いて
//! ある（タスク 7.2、6.3）。呼び出し元ウィンドウは引数として受け取り
//! （[`tauri::WebviewWindow`] が Tauri により注入される）、境界の文脈
//! （`app_shell::ipc::WindowContext`）へ写して呼び出し先が識別できるようにする（要件 4.6）。
//!
//! # 根と `COMMAND_NAMES` の同期
//!
//! `tauri::generate_handler!` と [`COMMAND_NAMES`] の間には**コンパイル時の連動が存在しない**
//! （前者は関数の識別子を、後者は文字列を取る）。そこで [`command_root!`] が両方を 1 つの一覧
//! から作り、そのうえで下のテストが 2 つの性質を機械的に固定する:
//!
//! 1. **登録された名前はすべて `COMMAND_NAMES` の要素である**（[`COMMAND_NAMES`] は「フロント
//!    エンドから呼び出せるコマンド」の単一の源であり、配列に無い名前を登録してはならない）。
//! 2. **登録された名前はハンドラの関数名と一致する**（Tauri は関数の識別子をコマンド名として
//!    公開するため、両者が食い違うとフロントエンドから到達できないコマンドが生まれる）。
//!
//! 逆向き（配列にあるが根が登録していない名前）は**強制しない**。配列は後続タスクが自分の
//! コマンドを実装する前に名前だけを予約する場所であり（例: `render_heartbeat` はタスク 8.2 が
//! 実装する）、「配列 ⊆ 登録」を要求すると未実装の名前を置けなくなる。したがって保証される
//! のは「登録 ⊆ 配列」の方向だけである。
//!
//! # 各機能の持ち場
//!
//! - 本ファイル: 根の列挙と、登録名・コマンド名の同期の検査。
//! - [`shell_cmds`]: app-shell 自身のコマンド（本タスク 7.1 の設定の読み書きと、設定変更の
//!   通知の配線）。
//! - [`bulk`]: 大きなペイロードの経路（タスク 7.2）。封筒を通らないコマンドの 1 つを持つ
//!   （もう 1 つは [`grid`] の `grid_rows_window`。上の 2 つの例外）。
//! - [`crate::window::close`]: 終了拒否の仲介（タスク 7.6）。フロントエンドの購読から呼ばれる
//!   `can_close_window` を持ち、委譲点の判定を境界へ写す。**この機能の実体はウィンドウの
//!   ライフサイクル（`window/`）側にある**ため、コマンド関数もそこに置く（design.md の
//!   ディレクトリ構成に従う）。根はここで列挙するだけである。
//! - [`crate::dialog`]: 親ウィンドウを指定したファイル選択（タスク 7.7）。9.6 の画面から
//!   呼ばれる `pick_document_file` を持ち、**メニューの項目と同じ 1 本の実装**を通る。
//! - [`crate::watchdog`]: 初回描画の監視と判定（タスク 8.2）。フロントエンドが描画フレームの
//!   中から呼ぶ `render_heartbeat` を持ち、判定・記録・8.3 の印を Tauri 非依存の中核
//!   （`app_shell::render`）へ委ねる。
//! - [`diagnostics_cmds`]: 診断の利用者向け導線（タスク 9.5）。保存場所の提示・書き出し・
//!   詳細度の読み書きという 4 つのコマンドを持ち、**実体は Tauri 非依存の中核**
//!   （`app_shell::diagnostics`。タスク 4.4 / 4.5）へ委ねる。あわせて 7.4 の登録口へ
//!   3 つのメニュー項目（診断の部分メニュー）を足し、選択を
//!   `DIAGNOSTICS_REQUESTED_EVENT` として対象ウィンドウへ送る。
//! - [`crate::window::association`]: ウィンドウの関連付けを問い合わせる（タスク 9.6）。
//!   ドキュメントを関連付けていないウィンドウに操作の導線を提示するための判定材料である。
//!   呼び出し元ウィンドウの関連付けを 6.1 のレジストリから読む `window_document_state` を
//!   持ち、**実体がウィンドウのライフサイクル側にある**ためコマンド関数もそこに置く。
//! - [`grid`]: グリッドのコマンド面（タスク 6.2、6.3）。シートを開く・表示の指定を変える・
//!   編集を適用する・履歴を進める・次の違反を探す・**可視範囲の窓を生バイトで返す**、の 6 つを
//!   持ち、**ドメイン型と境界用の型の変換を行う唯一の場所**である（design.md「GridCommands」）。
//!   最後の 1 つだけが封筒を運ばない生バイトの経路である（上の 2 つの例外）。ウィンドウごとの
//!   `GridSession` もこの機能が保持する（表の実体は `commands/grid.rs` の `GridSessions`）。
//!   呼び出し元ウィンドウは基盤が注入する引数から取る（要件 4.6）。

mod bulk;
mod diagnostics_cmds;
mod grid;
// **`macro` は予約語であり、モジュール名にできない。** ファイルの位置は design.md の
// File Structure Plan が定める `commands/macro.rs` のままにしたいので、`#[path]` で
// 実ファイルを指し、モジュール名だけを `macro_commands` にする（名前は Rust の予約語と
// 衝突するため、この 1 点だけ設計の綴りから動かす）。
#[path = "macro.rs"]
mod macro_commands;
mod shell_cmds;

use app_shell::ipc::command_names;

pub use diagnostics_cmds::install as diagnostics_install;
pub use grid::install as grid_install;
pub use macro_commands::install as macro_install;
pub use shell_cmds::start_settings_notifications;

/// 登録一覧から、ハンドラの根と「登録された名前」の一覧を同時に作る。
///
/// 呼び出し側は `名前定数 => ハンドラの関数` の組を並べる。名前は必ず
/// `crates/app-shell/src/ipc/command_names.rs` の定数を参照し、文字列リテラルを書かない
/// （tasks.md 2.2 の拡張規則）。この 1 箇所が `tauri::generate_handler!` へ渡す一覧と、
/// テストが `COMMAND_NAMES` と突き合わせる一覧の**両方**の源になる。
macro_rules! command_root {
    ($($name:expr => $handler:path),+ $(,)?) => {
        /// この根が登録するコマンド名。`COMMAND_NAMES` の部分集合でなければならない。
        /// **同期の検査（下のテスト）のためだけに置く** — 実行時には使わない。
        #[cfg(test)]
        const REGISTERED_COMMAND_NAMES: &[&str] = &[$($name),+];

        /// 登録したハンドラの経路（`stringify!` の結果）。関数名がコマンド名と一致することの
        /// 検査に使う（Tauri は関数の識別子をコマンド名として公開する）。
        #[cfg(test)]
        const REGISTERED_HANDLER_PATHS: &[&str] = &[$(stringify!($handler)),+];

        /// `invoke_handler` の根。**この関数の外に出る唯一の登録経路である。**
        ///
        /// 実行時の型はこのアプリのウィンドウ基盤（`tauri::Wry`）に固定する。`src-tauri` は
        /// そもそも `Builder<tauri::Wry>` で組まれている（`lifecycle::run`）。
        ///
        /// **名前定数は本番の展開でも参照する** — 定数の削除・改名がここでコンパイルを壊し、
        /// 「名前は単一の源から取る」という規約がテストの外でも効く。定数の値とハンドラの
        /// 関数名の一致はテストが固定する（`generate_handler!` は関数の識別子からコマンド名を
        /// 作るため、両者を結ぶコンパイル時の仕組みが存在しない）。
        pub fn invoke_handler(
        ) -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
            const _: &[&str] = &[$($name),+];
            tauri::generate_handler![$($handler),+]
        }
    };
}

command_root! {
    command_names::RENDER_HEARTBEAT => crate::watchdog::render_heartbeat,
    command_names::SETTINGS_GET => shell_cmds::settings_get,
    command_names::SETTINGS_SET => shell_cmds::settings_set,
    command_names::BULK_ECHO => bulk::bulk_echo,
    command_names::CAN_CLOSE_WINDOW => crate::window::close::can_close_window,
    command_names::PICK_DOCUMENT_FILE => crate::dialog::pick_document_file,
    command_names::DIAGNOSTICS_LOG_LOCATION => diagnostics_cmds::diagnostics_log_location,
    command_names::DIAGNOSTICS_EXPORT => diagnostics_cmds::diagnostics_export,
    command_names::DIAGNOSTICS_VERBOSITY_GET => diagnostics_cmds::diagnostics_verbosity_get,
    command_names::DIAGNOSTICS_VERBOSITY_SET => diagnostics_cmds::diagnostics_verbosity_set,
    command_names::WINDOW_DOCUMENT_STATE => crate::window::association::window_document_state,
    command_names::DOCUMENT_STATE => crate::session::commands::document_state,
    command_names::DOCUMENT_SAVE => crate::session::commands::document_save,
    command_names::DOCUMENT_NEW => crate::session::commands::document_new,
    command_names::DOCUMENT_DISCARD => crate::session::commands::document_discard,
    command_names::GRID_OPEN_SHEET => grid::grid_open_sheet,
    command_names::GRID_SET_VIEW => grid::grid_set_view,
    command_names::GRID_APPLY_EDIT => grid::grid_apply_edit,
    command_names::GRID_HISTORY => grid::grid_history,
    command_names::GRID_FIND_VIOLATION => grid::grid_find_violation,
    command_names::GRID_ROWS_WINDOW => grid::grid_rows_window,
    command_names::GRID_REFERENCE_ROWS => grid::grid_reference_rows,
    command_names::DIAGNOSTICS_RECORD_RENDER => diagnostics_cmds::diagnostics_record_render,
    command_names::MACRO_LIST => macro_commands::macro_list,
    command_names::MACRO_STORE => macro_commands::macro_store,
    command_names::MACRO_DELETE => macro_commands::macro_delete,
    command_names::MACRO_RUN => macro_commands::macro_run,
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_shell::ipc::COMMAND_NAMES;

    /// 根が登録した名前は、すべて単一の源（`COMMAND_NAMES`）の要素でなければならない。
    #[test]
    fn registered_commands_are_members_of_the_single_source_array() {
        for name in REGISTERED_COMMAND_NAMES {
            assert!(
                COMMAND_NAMES.contains(name),
                "COMMAND_NAMES に無い名前を登録している: {name}\n\
                 名前は crates/app-shell/src/ipc/command_names.rs の配列へ足すこと"
            );
        }
    }

    /// 登録した名前は、ハンドラの関数名と一致しなければならない。
    ///
    /// Tauri の `generate_handler!` は関数の識別子をコマンド名として公開するため、ここが
    /// 食い違うと**フロントエンドから到達できないコマンド**が生まれる（登録は成功して見える）。
    #[test]
    fn registered_names_match_the_handler_function_names() {
        assert_eq!(
            REGISTERED_COMMAND_NAMES.len(),
            REGISTERED_HANDLER_PATHS.len(),
            "名前とハンドラの数が食い違っている"
        );
        for (name, handler) in REGISTERED_COMMAND_NAMES
            .iter()
            .zip(REGISTERED_HANDLER_PATHS)
        {
            let function = handler
                .rsplit("::")
                .next()
                .expect("ハンドラの経路は少なくとも 1 つの要素を持つ")
                .trim();
            assert_eq!(
                function, *name,
                "登録名 {name} がハンドラの関数名 {function} と一致しない"
            );
        }
    }

    /// 名前の重複登録を許さない（同じコマンドを 2 回登録しても片方しか働かない）。
    #[test]
    fn registered_commands_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for name in REGISTERED_COMMAND_NAMES {
            assert!(seen.insert(*name), "コマンド名が重複している: {name}");
        }
    }
}

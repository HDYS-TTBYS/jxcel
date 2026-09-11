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
//! すべてのコマンドは [`IpcResult`] を返し、例外に頼らない（要件 4.4）。唯一の例外は
//! [`bulk::bulk_echo`] である — 要件 4.5 が JSON を経由しない生バイトの応答を要求するため、
//! 封筒（`serde` で JSON になる）を返せない。例外の根拠と消費者への見え方は `bulk` の
//! モジュール doc に書いてある（タスク 7.2）。呼び出し元ウィンドウは引数として受け取り
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
//! - [`bulk`]: 大きなペイロードの経路（タスク 7.2）。封筒を通らない唯一のコマンドを持つ。
//! - [`crate::window::close`]: 終了拒否の仲介（タスク 7.6）。フロントエンドの購読から呼ばれる
//!   `can_close_window` を持ち、委譲点の判定を境界へ写す。**この機能の実体はウィンドウの
//!   ライフサイクル（`window/`）側にある**ため、コマンド関数もそこに置く（design.md の
//!   ディレクトリ構成に従う）。根はここで列挙するだけである。

mod bulk;
mod shell_cmds;

use app_shell::ipc::command_names;

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
    command_names::SETTINGS_GET => shell_cmds::settings_get,
    command_names::SETTINGS_SET => shell_cmds::settings_set,
    command_names::BULK_ECHO => bulk::bulk_echo,
    command_names::CAN_CLOSE_WINDOW => crate::window::close::can_close_window,
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

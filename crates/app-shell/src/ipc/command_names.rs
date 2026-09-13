//! コマンド名の単一配列（要件 4.1、4.2）。
//!
//! `src-tauri` のハンドラ登録と TypeScript の生成物（`src/ipc/bindings.ts`）が**同じ配列**を
//! 参照することで、名前のドリフトを構造的に塞ぐ。この配列に現れない名前でコマンドを登録しては
//! ならない（design.md「IpcContract」の不変条件）。
//!
//! # 拡張規則（後続タスクと後続スペックが守ること）
//!
//! コマンドを追加する側は、**文字列リテラルでハンドラを登録しない**。まず本配列へ名前を足す。
//! 配列が単一の源であり、`src-tauri` の登録（タスク 7.1）と `src/ipc/bindings.ts` の生成
//! （タスク 2.2、[`render_bindings`](crate::ipc::render_bindings)）の両方がここを参照する。
//!
//! 名前の削除・改名は design.md「Revalidation Triggers」の「IPC の型定義またはコマンド名の
//! 変更」に該当する。生成物を再生成し、フロント側の利用者を追随させ、ドリフト検査（タスク 2.3）
//! を通し直すこと。
//!
//! # 配列に現れない名前
//!
//! 設定変更の通知（タスク 7.1、要件 7.4）は Tauri の**イベント**としてフロントエンドへ届き、
//! `invoke` の宛先を持たない。したがって本配列には現れない。配列は「フロントエンドから
//! 呼び出せるコマンド」の一覧である。イベント名は [`crate::ipc::SETTINGS_CHANGED_EVENT`] に
//! 定義し、生成物へ定数として出す（フロントエンドが文字列リテラルを書かないため）。
//!
//! # 個々の名前の定数
//!
//! 各名前は `pub const` として公開する。**`src-tauri` のハンドラ登録（タスク 7.1）はこの定数を
//! 参照して登録し、文字列リテラルを書かない。** 登録された名前が配列の要素であることは、
//! `src-tauri/src/commands/mod.rs` のテストが機械的に検査する（`generate_handler!` と配列の間に
//! コンパイル時の連動が無いため、そこを埋める唯一の安価な砦である）。

/// 描画のハートビート通知を受け取る（タスク 8.2 が実装し、送信側はタスク 9.7。要件 10.1、10.2）。
pub const RENDER_HEARTBEAT: &str = "render_heartbeat";

/// ウィンドウを閉じてよいかの問い合わせに答える（タスク 7.6。要件 2.6）。
///
/// 判定は非同期であり、フロントエンドはこの往復の結果に従って `destroy` するか何もしない。
pub const CAN_CLOSE_WINDOW: &str = "can_close_window";

/// 名前付き設定値を読み取る（タスク 7.1 が面へ結線し、実体はタスク 4.1。要件 7.1、7.3）。
pub const SETTINGS_GET: &str = "settings_get";

/// 名前付き設定値を書き込む（タスク 7.1 が面へ結線し、実体はタスク 4.2。要件 7.1、7.4）。
///
/// 書き込みは購読している全ウィンドウへ通知される（通知自体はイベント経路であり、
/// 本配列の対象外）。
pub const SETTINGS_SET: &str = "settings_set";

/// 親ウィンドウを指定したファイル選択を提示する（タスク 7.7。要件 2.4）。
///
/// 選ばれた位置はドキュメント所有者へ引き渡すだけで、本機能はパスを読まない。
pub const PICK_DOCUMENT_FILE: &str = "pick_document_file";

/// 大きなペイロードを 1 回の呼び出しで受け渡す（タスク 7.2。要件 4.5）。
///
/// JSON を経由しない生バイトの経路であり、行ごとに境界を越えることを必要としない。
pub const BULK_ECHO: &str = "bulk_echo";

/// 記録の保存場所を返す（タスク 9.5 の導線。実体はタスク 4.4。要件 8.1）。
pub const DIAGNOSTICS_LOG_LOCATION: &str = "diagnostics_log_location";

/// 記録をひとつのファイルにまとめて書き出す（タスク 9.5 の導線。実体はタスク 4.5。要件 8.6）。
pub const DIAGNOSTICS_EXPORT: &str = "diagnostics_export";

/// 記録の詳細度を読み取る（タスク 9.5 の導線。実体はタスク 4.5。要件 8.7）。
pub const DIAGNOSTICS_VERBOSITY_GET: &str = "diagnostics_verbosity_get";

/// 記録の詳細度を変更する（タスク 9.5 の導線。実体はタスク 4.5。要件 8.7）。
pub const DIAGNOSTICS_VERBOSITY_SET: &str = "diagnostics_verbosity_set";

/// 呼び出し元ウィンドウにドキュメントが関連付けられているかを返す（タスク 9.6。要件 2.1、2.2）。
///
/// 関連付けの実体はウィンドウの生成時に確定し、レジストリ（`window/mod.rs` の
/// `WindowRegistry`）がラベルを鍵とする写像として保持する。**ラベルの接頭辞からは推測しない** —
/// 接頭辞は割り当て順の規約であり、`attach` は記録された関連付けを書き換えないため、
/// 接頭辞と記録された事実が食い違いうる（9.6 の画面のモジュール doc を参照）。
///
/// **パスは境界を越えない。** このコマンドが答えるのは関連付けの有無だけであり、どの
/// ドキュメントかは所有者（下流スペック）が持つ（7.7 の `DocumentPickOutcome` と同じ方針）。
pub const WINDOW_DOCUMENT_STATE: &str = "window_document_state";

/// 呼び出し元ウィンドウのセッションの状態を返す（タスク 3.4。要件 1.6、1.7、2.1）。
///
/// **起動時に指定されたドキュメントの読み込みは、この問い合わせが引き金になる**（遅延解決）。
/// 呼び出し元ウィンドウは基盤が注入する引数から取るため、フロントエンドはウィンドウを偽装できない。
pub const DOCUMENT_STATE: &str = "document_state";

/// 呼び出し元ウィンドウのドキュメントを保存する（タスク 3.4。要件 5.1〜5.4）。
///
/// 出所を持たない文書では保存先の選択を提示し、**選ばれた位置を応答へ含めない**
/// （位置は境界を越えない）。非同期コマンドであり、保存と提示は `spawn_blocking` に載る。
pub const DOCUMENT_SAVE: &str = "document_save";

/// 呼び出し元ウィンドウに新しいドキュメントを用意する（タスク 3.4。要件 7.1〜7.4）。
pub const DOCUMENT_NEW: &str = "document_new";

/// 呼び出し元ウィンドウの未保存の印を落とす（タスク 3.4。要件 6.5）。
///
/// **保存しない。** 利用者が「変更を破棄して閉じる」を選んだときの明示の指示である。
pub const DOCUMENT_DISCARD: &str = "document_discard";

/// フロントエンドから呼び出せるコマンド名の一覧（要件 4.1、4.2）。
///
/// `src-tauri` のハンドラ登録（タスク 7.1）と TypeScript の生成物（タスク 2.2）の**両方**が
/// この配列を参照する。順序はそのまま生成物の並びになる。
pub const COMMAND_NAMES: &[&str] = &[
    RENDER_HEARTBEAT,
    CAN_CLOSE_WINDOW,
    SETTINGS_GET,
    SETTINGS_SET,
    PICK_DOCUMENT_FILE,
    BULK_ECHO,
    DIAGNOSTICS_LOG_LOCATION,
    DIAGNOSTICS_EXPORT,
    DIAGNOSTICS_VERBOSITY_GET,
    DIAGNOSTICS_VERBOSITY_SET,
    WINDOW_DOCUMENT_STATE,
    DOCUMENT_STATE,
    DOCUMENT_SAVE,
    DOCUMENT_NEW,
    DOCUMENT_DISCARD,
];

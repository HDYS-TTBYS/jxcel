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

/// 呼び出し元ウィンドウに表示するシートを開く（タスク 6.2。要件 1.1、1.5、1.6）。
///
/// 開いたセッションは**ウィンドウごとに 1 つ**保持され、ウィンドウが閉じたら破棄される
/// （`design.md`「GridCommands」）。呼び出し元ウィンドウは基盤が注入する引数から取るため、
/// フロントエンドはウィンドウを偽装できない（要件 4.6）。
pub const GRID_OPEN_SHEET: &str = "grid_open_sheet";

/// 呼び出し元ウィンドウのグリッドの表示の指定を変える（タスク 6.2。要件 8.3、8.4）。
///
/// 並べ替え・絞り込み・入れ子の展開を 1 つの要求として受け取る。**ドキュメントは変わらない**
/// （要件 8.5）。
pub const GRID_SET_VIEW: &str = "grid_set_view";

/// 呼び出し元ウィンドウのグリッドへ編集命令を 1 つ適用する（タスク 6.2。要件 3.3）。
///
/// 判定は `schema-engine` が行い、適合しない値も破棄せず違反として返す（要件 3.5）。
pub const GRID_APPLY_EDIT: &str = "grid_apply_edit";

/// 呼び出し元ウィンドウのグリッドの履歴を進める（タスク 6.2。要件 9.2、9.3）。
///
/// どちらへ進めるかは要求が言う（取り消し / やり直し）。
pub const GRID_HISTORY: &str = "grid_history";

/// 呼び出し元ウィンドウのグリッドで、指定した位置から次の違反を探す（タスク 6.2。要件 4.4）。
///
/// **表示範囲の外にある違反にも到達する**（要件 4.4）。見つからなければ「これ以上無い」を
/// 正常な結果として返す。
pub const GRID_FIND_VIOLATION: &str = "grid_find_violation";

/// 呼び出し元ウィンドウのグリッドの窓を、生バイトで返す（タスク 6.3。要件 1.1、11.2）。
///
/// **封筒を返さない唯一のグリッドのコマンドである**（`bulk_echo` と同じ生バイトの経路）。
/// 引数は要求の頭を含む二進の 1 つであり、**引数全体でなければならない** — 入れ子にすると
/// Tauri が数値の配列へ変換して JSON として送るため、経路の意味が失われる
/// （`src-tauri/src/commands/grid.rs` のモジュール docs「引数を入れ子にしない」）。
///
/// 失敗と世代違いは**空の窓**で表す（この経路は封筒を運べないため）。
pub const GRID_ROWS_WINDOW: &str = "grid_rows_window";

/// 参照先のシートの行を頁ごとに読む（タスク 10.3。要件 3.8）。
///
/// **参照先は別のシートであり、その行数は表示中のシートと無関係である**（1 万行の参照先は
/// 普通にありうる）ため、応答は頁に閉じる — 要求が件数を運び、境界が上限
/// （`app_shell::ipc::GRID_REFERENCE_PAGE_LIMIT`）へ切り詰める。
///
/// 参照先のシートは要求ではなく**列の宣言**から決まる（要求は文書の列の添字だけを運ぶ）。
/// 参照しない列を指定した場合と、参照先のシートが文書に無い場合は**経路の失敗**である
/// （「行が無い」と混同しない — 6 本の写像の規律と同じ）。
pub const GRID_REFERENCE_ROWS: &str = "grid_reference_rows";

/// 呼び出し元ウィンドウのグリッドの描画の健全性を、診断の記録へ 1 件残す（タスク 9.3。要件 12.2、12.3）。
///
/// **運ぶのは閉じた札と数値だけである。**自由な文字列を記録へ流す口を作らないためであり、
/// 記録の 1 行を組み立てるのは器の側である（記録の注入面を広げない。10.8 がクリップボードの
/// 読み取った文字を記録へ出さなかったのと同じ規律）。
///
/// 呼び出し元ウィンドウは基盤が注入する引数から取るため、フロントエンドはウィンドウを
/// 偽装できない（要件 4.6）。
pub const DIAGNOSTICS_RECORD_RENDER: &str = "diagnostics_record_render";

/// 呼び出し元ウィンドウのドキュメントのマクロの一覧を返す（タスク 4.3。マクロ実行の要件 1.3、1.4）。
///
/// **実行しない。**解釈（能力の宣言と、種別としての構文）だけを行い、解釈できなかったマクロも
/// 一覧に残して理由を添える（要件 1.4 — ドキュメントは開ける）。呼び出し元ウィンドウは基盤が
/// 注入する引数から取るため、フロントエンドはウィンドウを偽装できない。
pub const MACRO_LIST: &str = "macro_list";

/// 呼び出し元ウィンドウのドキュメントへマクロを 1 件保存する（タスク 4.3。要件 1.1、1.6）。
///
/// 同じ名前のマクロは**置き換え**である（要件 1.6）。保存は `document-session` の `edit` の
/// 閉包 1 回で行うため、**未保存の印と版が立つ**（`structure.md`「セッションの所有の規約」）。
pub const MACRO_STORE: &str = "macro_store";

/// 呼び出し元ウィンドウのドキュメントからマクロを 1 件削除する（タスク 4.3。要件 1.7）。
///
/// 保存と同じく `edit` の閉包 1 回で行う。**取り除く相手が無ければ `edit` を呼ばない**
/// （版と未保存の印を動かさない）。
pub const MACRO_DELETE: &str = "macro_delete";

/// 呼び出し元ウィンドウのドキュメントに対してマクロを 1 件実行する（タスク 4.3。要件 2.1–2.5）。
///
/// 上限（時間・メモリ）は**要求に載せない** — 設定から適応層が解決してエンジンへ渡す
/// （要件 6.5。design.md「Data Contracts & Integration」）。実行の失敗（例外・構文誤り・
/// 能力の拒否）と打ち切りは**成功の応答**が 3 値として運ぶ（要件 2.4、6.1、6.2 の提示が
/// 失敗と同じ面に出るためである）。
pub const MACRO_RUN: &str = "macro_run";

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
    GRID_OPEN_SHEET,
    GRID_SET_VIEW,
    GRID_APPLY_EDIT,
    GRID_HISTORY,
    GRID_FIND_VIOLATION,
    GRID_ROWS_WINDOW,
    GRID_REFERENCE_ROWS,
    DIAGNOSTICS_RECORD_RENDER,
    MACRO_LIST,
    MACRO_STORE,
    MACRO_DELETE,
    MACRO_RUN,
];

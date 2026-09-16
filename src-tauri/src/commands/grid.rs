//! グリッドのコマンド面 — 封筒つきの 5 つと生バイトの 1 つ、そして**ドメイン型 ⇄ 境界用の
//! 型の変換を行う唯一の場所**（タスク 6.2、6.3。design.md「GridCommands」の API Contract、
//! 要件 1.1、3.3、4.4、8.3、8.4、9.2、9.3、11.2）。
//!
//! # 7 つの経路
//!
//! | コマンド | 経路 | 何を答えるか |
//! |---|---|---|
//! | [`grid_open_sheet`] | 文書から対象シートを引き、スキーマを計画へ落として [`GridSession::open`] | 列の構成とシートの行数（要件 1.1、1.5、1.6） |
//! | [`grid_set_view`] | [`GridSession::set_view`]（＋展開の適用） | 可視行数・隠された行数・違反の総数（要件 8.3、8.4、8.7） |
//! | [`grid_rows_window`] | 生バイトの要求を読み、[`GridSession::encode_window`] | 可視範囲の窓（二進。要件 1.1、11.2） |
//! | [`grid_apply_edit`] | [`GridSession::apply`] | 影響範囲・型強制・違反・行数（要件 3.3、3.5） |
//! | [`grid_history`] | [`GridSession::undo`] / [`GridSession::redo`] | 同じ要約（要件 9.2、9.3） |
//! | [`grid_find_violation`] | [`GridSession::find_violation`] | 次の違反の位置と理由（要件 4.2、4.4、4.5） |
//! | [`grid_reference_rows`] | 宣言から参照先のシートを引き、[`reference_page`] で頁を組む | 参照先の行の頁と総数（要件 3.8） |
//!
//! **6 つは封筒（[`IpcResult`]）を返し、[`grid_rows_window`] だけが生バイトを返す**
//! （要件 4.5 が JSON を経由しない経路を要求するため。根拠と消費者への見え方は
//! `crate::commands::bulk` のモジュール doc にある）。生バイトの経路は封筒を運べないため、
//! **失敗は空の窓で表す**（本モジュールの「生バイト経路」節）。
//!
//! **呼び出し元ウィンドウは基盤が注入する [`WebviewWindow`] から取る**（ペイロードで
//! 受け取らない ＝ 偽装できない。要件 4.6、`ipc-contract.md`）。したがって 6 つとも要求の型に
//! ウィンドウは現れない — 要求が運ぶのは操作の対象（シートの識別子・表示の指定・編集命令・
//! 進める向き・探索の起点・窓の区間・文書の列の添字）だけである。
//!
//! [`grid_reference_rows`] は**タスク 10.3 が足した 7 本目**である（要件 3.8。7.4 の申し送り 2
//! 「参照先の行を一覧する経路が 6 本のコマンドに無い」を閉じる）。応答は**頁**に閉じ、
//! 件数は境界の上限（[`GRID_REFERENCE_PAGE_LIMIT`]）を超えない。
//!
//! # メニューからの引き金（タスク 8.7、8.9。要件 7.8、9.9）
//!
//! 本モジュールは**コマンド面だけではない** — [`install`] が 7.4 の登録口へ `編集 > 複製`
//! （`data-grid.copy`。非 macOS `Ctrl+C` / macOS `Cmd+C`）を登録し、活性化を
//! [`GRID_COPY_REQUESTED_EVENT`] として**活性化の対象ウィンドウ**（7.5）へ送る。画面側は
//! `src/features/grid/clipboardRequests.ts` が購読して、**打鍵（DOM の `copy`）と同じ入口**
//! （移植口の `RendererHandle.copySelection`）へ渡す — 範囲の決定もテキストの作成も 1 つに
//! 閉じる（9.5 の診断の導線と同じ形であり、新しい設計ではない）。
//!
//! **8.9 が同じ形で 2 つ足した** — `編集 > 元に戻す` / `編集 > やり直し`
//! （`data-grid.undo` / `data-grid.redo`。非 macOS `Ctrl+Z` / `Ctrl+Shift+Z`、macOS
//! `Cmd+Z` / `Cmd+Shift+Z`）。**項目ごとにイベントを分けない** — どちらも
//! [`GRID_HISTORY_REQUESTED_EVENT`] を送り、**どちらの項目かは荷が運ぶ**
//! （`design.md` の「メニューの取り消し・やり直しの結線」）。画面側は
//! `src/features/grid/history.ts` が購読して 1 つの入口へ渡す。
//!
//! **貼り付けの項目は登録しない。**障碍はクリップボードを読む経路が無いことであり、読み口が
//! 無いまま `Ctrl+V` を登録すると、基盤のメニューが打鍵を先に受け取っていま動いている貼り付けを
//! 壊す（[`install`] の doc に実測と併せて記録した。要件 7.8 の後半は未達である）。
//!
//! # 失敗の載せ方（design.md「Error Handling」の表）
//!
//! **利用者の入力の結果は封筒の成功腕に載る。** 型に合わない値を保持して違反として返すこと
//! （要件 3.5）、「これ以上違反が無い」こと（要件 4.4）、「進める履歴が無い」こと
//! （要件 9.2、9.3）は、いずれも**正常な結果**であり、[`IpcError`] の腕には載せない。
//!
//! **封筒の失敗腕へ落ちるのは、操作の誤りと経路そのものの失敗だけである**:
//!
//! - 操作の誤り — [`GridError`]（範囲外のセル・解釈できない入れ子の表現・未知の行）。
//!   `design.md` の同表の「操作の誤り」の行がこれを定める（画面は再要求する）
//! - 経路そのものの失敗 — そのウィンドウにドキュメントが無い（[`SessionError::NoDocument`]）、
//!   要求されたシートが文書に無い、宣言が壊れていて計画へ落とせない、グリッドがまだ
//!   開かれていない、行の識別子が解釈できない。どれも「コマンドの経路が成立しない」であり、
//!   ドメインの判定結果ではない。
//!
//! 理由の文言を組み立てるのは適応層の仕事である（`session/commands.rs` と同じ規律。
//! `GridError` / `SessionError` / `ViolationReason` はいずれも表示用の文言を持たない）。
//!
//! # 生バイト経路（`grid_rows_window`。要件 1.1、11.2）
//!
//! ## 引数の配置（**本節が唯一の源**。design.md の入力の 1 行をここで確定させる）
//!
//! ```text
//! 引数 = 頭 || シートの識別子
//!
//! 頭（33 バイト = [`WINDOW_REQUEST_HEADER_LEN`]。数の欄はすべて u64 リトルエンディアン）:
//!   0       版        u8      = WINDOW_REQUEST_VERSION
//!   1..9    世代      u64     要求が名乗る世代
//!   9..17   開始序数  u64     可視行の序数（文書の位置ではない）
//!   17..25  行数      u64     要求する行の数
//!   25..33  シート長  u64     シートの識別子の UTF-8 のバイト長
//! 33..     シート    UTF-8   シートの識別子（要求の末尾まで）
//! ```
//!
//! 全体の長さは `WINDOW_REQUEST_HEADER_LEN + シート長` であり、**それより長い入力も短い
//! 入力も拒む**（余りを黙って捨てると、壊れた要求が正常に見える — 窓の復号と同じ規律）。
//!
//! 配置を決めた理由は 3 つある:
//!
//! 1. **固定部分の幅を固定する**（33 ＝ 窓の [`HEADER_LEN`] と同じ幅）。数の欄は位置だけで
//!    引けるため、復号は前へ 1 回走査するだけで閉じる
//! 2. **可変長の欄の長さを本体の直前に置く**（窓と同じ規律）。シートの終端を推し量る経路を
//!    作らない
//! 3. 数の欄を u64 にする理由は窓と同じである — 符号化の側に「収まらない」経路を作らない
//!    （復号の側だけが、32 ビットのホストで表せない値を拒む）
//!
//! **版は窓の版（`WINDOW_FORMAT_VERSION`）とは別の体系である。** 要求は本モジュールが
//! 定める配置であり、窓は `data-grid` の `transport` 層が定める配置である。欄を足すときは
//! 版を上げる（知らない版の要求は空の窓である ＝ フロントエンドは窓の記憶を捨てて要求を
//! 組み直す。7.3）。
//!
//! ## 引数を入れ子にしない（`bulk_echo` と同じ罠）
//!
//! **バッファは引数全体でなければならない。** `invoke("grid_rows_window", buffer)` の形で
//! 呼ぶ（`src/ipc/client.ts` の `invokeRaw` がこれを行う）。`{ argument: buffer }` のように
//! 入れ子にすると、Tauri は `Uint8Array` を `Array.from()` で数値の配列へ変換し、JSON
//! （[`InvokeBody::Json`]）として送る — 受け手は生バイトとして読めない。この場合は
//! **例外を投げず**空の窓と警告を返す（長さ 0 で観測できる。`bulk` のモジュール doc「経路の
//! 性質」）。したがって本コマンドの引数は [`Request`] **1 つ**であり、構造体や `serde` の
//! 型で包まない。
//!
//! ## 失敗と世代違いは空の窓（この経路は封筒を運べない）
//!
//! 空の窓は [`EMPTY_WINDOW`]（長さ 0 のバイト列）であり、**行 0 の窓**（頭だけを持つ
//! [`HEADER_LEN`] バイト。可視行の末尾に接する要求や、可視行が 0 のシート）とは区別できる。
//! 画面は「端に達した」と「要求が通らなかった」を別に扱う（前者は読み込み中のまま再試行
//! しない。`transport` のモジュール docs「空の窓の表現」）。
//!
//! | 状態 | 答え |
//! |---|---|
//! | 引数が生バイトでない（入れ子の罠） | 空の窓 |
//! | 引数を読めない（短い・長い・知らない版・UTF-8 でない） | 空の窓 |
//! | グリッドがまだ開かれていない | 空の窓 |
//! | 要求のシートが表示中のシートと違う | 空の窓 |
//! | 世代が一致しない（古い・**新しすぎる**） | 空の窓 |
//! | 文書にシートが無い・開始序数が可視行数より後ろ・行を引けない | 空の窓 |
//! | 可視行の末尾に接する要求 | **行 0 の窓**（空の窓ではない） |
//!
//! ## シートの照合（design.md の入力と 5.1 の `WindowRequest` の食い違いの解決）
//!
//! design.md の Batch 契約は要求の頭が**シート**を運ぶと定める。一方 5.1 の
//! [`WindowRequest`] は**シートを持たない** — `GridSession` は 1 枚のシートに閉じた操作口で
//! あり、どのシートを見ているかはセッションが既に知っているためである（`transport` の
//! モジュール docs が同じ理由を書いている）。
//!
//! **解決は本層で行う**: シートは**要求の側にだけ**現れ、本層が
//!
//! 1. 引数のシートを復号し（[`decode_window_argument`]）
//! 2. **保持しているシート（[`SheetEntry::sheet`]）と突き合わせ**、
//! 3. 一致したときだけ世代と区間を [`WindowRequest`] へ写して
//!    [`GridSession::encode_window`] へ渡す
//!
//! という順序を取る。**一致しなければ空の窓である**（表示していないシートの窓を返す経路を
//! 作らない）。この照合があるため、文書の差し替え（メニュー「開く…」）の後に古いシートを
//! 名指す要求が来ても、窓ではなく空の窓が返る。
//!
//! ## 世代を比べるのは本層である（5.1 が開いたままにした点）
//!
//! [`GridSession::encode_window`] は `WindowRequest` を**自分で組み立てる**（世代は
//! `self.codec.generation()` ＝ いまの世代）。したがって**要求が名乗る世代を見られるのは
//! 呼び出し側だけ**であり、5.1 のモジュール docs の表も「6.3 のコマンドが要求の頭から世代を
//! 読み、`is_stale` を見て空の窓を失敗と同じに扱う」と定めている。比較の規則そのものは
//! [`WindowCodec::is_stale`] が唯一の源であり、本層は**自分で `!=` を書かない**（一致しない
//! 世代＝古い世代と新しい世代の扱いを二重に定めない）。
//!
//! # 唯一の変換の場所
//!
//! 境界の型（`app_shell::ipc::grid`）とドメインの型（`data-grid` / `schema-engine`）を写すのは
//! **本モジュールだけ**である。とくに次の 2 つは本モジュールの責任である:
//!
//! 1. **違反の総数をシート全体へ閉じる。** ドメインの
//!    `EditOutcome::violation_total` は**再検証した列に閉じた**総数であり、境界の
//!    [`GridEditOutcome::violation_total`] はそれを**写さない**（シート全体の数である）。
//!    シート全体の総数を保つのは
//!    [`GridSession`] であり（違反の差分を載せる唯一の所有者）、本モジュールは
//!    [`GridSession::violation_total`] を読んで境界へ載せる（要件 4.3、11.4。
//!    **そのために検証を呼び直すことは無い**）
//! 2. **型の種別の札の対応。** [`TypeKindTag`] は `app-shell` にあり（他のドメインクレートに
//!    依存できない）、`schema-engine` の [`TypeKind`] は本クレートでしか見られない。
//!    したがって**対応を検査できる唯一の場所が本モジュール**であり、[`type_kind_tag`] が
//!    全変種を網羅する `match`（ワイルドカード無し ＝ 総関数）と、その像が
//!    [`TypeKindTag::ALL`] と綴り・件数・並びの 3 点で一致することを検査するテストを持つ。
//!
//! # ウィンドウごとの `GridSession`（design.md「GridCommands」の Integration）
//!
//! **ウィンドウごとに 1 つ保持し、ウィンドウが閉じたら破棄する。** 保持するのは
//! [`GridSessions`] であり、表（ラベル → 保持）と破棄の購読を 1 対で持つ。表のロックは
//! **参照と挿入・除去のためだけ**に取り、処理の間は保持しない（10 万行の適用が他の
//! ウィンドウを待たせない。`document-session` の表と同じ規律）— ウィンドウごとの実体は
//! [`Arc`] で持ち、そこに 1 つずつロックを置く。
//!
//! **保持が持つのはセッション（表示状態）と、そのウィンドウのドキュメントの取り消し履歴
//! である**（要件 9.5）。履歴をセッションに持たせない理由と、シートの切り替えを越えて
//! 引き継ぐ規則は [`SheetEntry`] と [`answer_open`] の docs にある — **要点は、シートを
//! 切り替えても文書の履歴が消えないこと**である（[`grid_open_sheet`] は保持ごと置き換える
//! ため、履歴を持ち出して次の保持へ渡す）。
//!
//! **破棄の購読は [`WindowDestroyEvents`] の縫い目を通す**（セッションの層が確立した形を
//! そのまま使う。テストは二重（`session/watch.rs` の `testing::AlwaysPresent`）を駆動する）。購読は登録済みの
//! ラベルの集合で 1 回に抑え、破棄の通知と、登録できなかったときの掃除が同じ後始末を通る。
//!
//! # 実行モデル（封筒の 5 つは主スレッドの外、生バイトの 1 つは同期）
//!
//! **封筒を返す 5 つは `#[tauri::command(async)]` である。** これは関数を非同期にするのでは
//! なく、**同期の本体を Tauri のブロッキング用のスレッドプールで走らせる**印である（Tauri の
//! 既定では同期コマンドは IPC の処理の中＝主スレッドで走る）。
//!
//! 理由は費用である。`grid_set_view` は最初の 1 回に**シート全体の検証**を行って違反の索引を
//! 組み立て（10 万行 × 30 列で約 255 ミリ秒。design.md「Performance & Scalability」の実測）、
//! `grid_apply_edit` と `grid_history` は 1 万行の貼り付けとその取り消しを運びうる（要件 11.5 の
//! 予算は 3 秒）。主スレッドをその間占めると、**どの操作でも待たされない**という要件 11 の
//! 目的そのものが壊れる（描画も入力も止まる）。
//!
//! **`State` を引数に取らない**のはこのためである（借用はスレッドを跨げない）。管理状態は
//! 本体の中で `app.state::<...>()` から取る — 引数の型は `AppHandle` / `WebviewWindow` /
//! 要求だけで、いずれも所有権ごと渡せる。
//!
//! **応答の形は変わらない。** 封筒（[`IpcResult`]）を返すことは、経路がどこで走るかに
//! 依らない（要件 4.4）。
//!
//! ## `grid_rows_window` だけが同期である理由（型が決めている）
//!
//! [`Request`] は invoke のメッセージを**借用**する型である（`Request<'a>`）。Tauri の
//! コマンドのマクロは `#[tauri::command(async)]` の本体を `async move` の内側へ引数を移して
//! 組むため、借用を含む引数はそこへ持ち込めない（`bulk_echo`（7.2）が同じ理由で同期である）。
//! 生バイトの引数を取る唯一の形が `Request` である以上、この経路は同期に固定される。
//!
//! 費用の形は 5 つと違って**行数に依らない**: 窓の符号化は行の値を窓の行数ぶんだけ引く
//! （10 万行のシートでも窓は画面 1 枚ぶんである。`data-grid` の `transport` のモジュール docs
//! 「費用の形」）。10 万行のシートの末尾の窓を要求したときの実測は
//! `tests` の `a_window_at_an_arbitrary_position_of_a_hundred_thousand_rows_is_retrieved`
//! にあり、要件 11.2 の 1 秒に対して十分な余裕を持つ。
//!
//! # スキーマはどこから来るか（要件 1.1、1.2）
//!
//! [`GridSession::open`] は `SheetId` と [`CompiledSchema`] を要する（design.md「GridSession」の
//! Service Interface）。文書を保持しているのは `document-session` であり、シートとその
//! ルートスキーマは `document-format` の文書が持つ。したがって本モジュールは
//! **[`DocumentSessionsApi::read`] の閉包の内側で**シートを引き、`schema-engine` の
//! [`SchemaEngineApi::compile`] で計画へ落とす — **その文書のそのシートから**落とすことが
//! 計画の事前条件である（別のシートの計画を渡すと列の添字が意味を失う）。
//!
//! 計画を保持し続けるのは、違反の理由（要件 4.2）を組み立てるときに列をもう一度判定する
//! ためである。**セッションは計画を外へ出さない**（`GridSession` の欄はすべて私有）ため、
//! 本モジュールが同じ計画を持つ。
//!
//! # このクレートが `document-format` を通常依存に持たないこと（`session/verification.rs` と同じ規律）
//!
//! `src-tauri` は `document-format` を**通常依存に持たない**（テストだけが dev-dependency と
//! して使う。`src-tauri/Cargo.toml` の依存方針）。したがって本モジュールは
//! **`document-format` の名前を 1 つも書かない** — 文書のシート・行数・識別子は
//! `document-session` の公開面（`read` / `edit` の閉包が与える `&Document`）を通して触り、
//! 型は文脈から推論させる。シートの識別子も、文書の側の文字列表現と要求の文字列を突き合わせる
//! ことで選ぶ（識別子の型を名指しする必要が無い）。
//!
//! # テストの形
//!
//! Tauri の実体（`WebviewWindow` / `AppHandle`）を要するのはコマンド関数の 6 つだけであり、
//! 中身は**すべて本体の関数**（[`answer_open`] / [`answer_set_view`] / [`answer_rows_window`] /
//! [`answer_apply_edit`] / [`answer_history`] / [`answer_find_violation`]）へ切り出してある。
//! テストはそれらを直接駆動する — GUI を起こさず、**本物の文書**（`document-format` は
//! dev-dependency）と、破棄の購読の二重（`session/watch.rs` の `testing::AlwaysPresent`）
//! だけで足りる。生バイトの経路の本体は [`InvokeBody`] を取る（[`Request`] を組む公開の口が
//! `tauri` に無いため）ので、テストは生バイトと**入れ子の JSON** の両方を差し込める。
//! 6 つのコマンド関数そのものの形（注入の 2 引数と要求、応答の型）は、関数の型を
//! 書いたテストがコンパイル時に固定する（[`grid_rows_window`] は [`Response`] を返す ＝
//! 封筒を返さないことがそこに現れる）。
//!
//! モジュール docs が参照するが、本モジュールの `use` に無い名前の宛先（`data-grid` の
//! `transport` のモジュール docs と同じ流儀 — 名前を `use` すると、docs だけの利用が
//! 未使用の import になる）。
//!
//! [`HEADER_LEN`]: data_grid::HEADER_LEN

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use app_shell::ipc::{
    ColumnChoice, ColumnDescriptor, ColumnElementCount, ColumnExpandability,
    ColumnMemberDescriptor, GRID_COPY_REQUESTED_EVENT, GRID_HISTORY_REQUESTED_EVENT,
    GRID_REFERENCE_PAGE_LIMIT, GridCellAddress, GridCoercionNotice, GridEditCommand,
    GridEditOutcome, GridEditRequest, GridEditResponse, GridExpansionState, GridFilterSpec,
    GridHistoryDirection, GridHistoryRequest, GridHistoryRequestedEvent, GridOpenRequest,
    GridOpenResponse, GridPathSegment, GridReferenceRequest, GridReferenceResponse,
    GridReferenceRow, GridSearchDirection, GridSheetSummary, GridViewRequest, GridViewResponse,
    GridViewSpec, GridViolation, GridViolationLocation, GridViolationRequest,
    GridViolationResponse, IpcError, IpcResult, TypeKindTag, WindowContext, WindowLabel,
    command_names,
};
use data_grid::{
    CellAddress, CoercionNotice, ColumnIndex, DEFAULT_UNDO_LIMIT, EMPTY_WINDOW, EditCommand,
    EditOutcome, ElementCount, Expandability, ExpansionState, FilterSpec, Generation, GridError,
    GridSession, LayoutColumn, NestedPathSegment, ReferencePage, RowOrdinal, RowSpan,
    SearchDirection, SortKey, UndoStack, ViewSpec, WindowCodec, WindowRequest, display_text,
    reference_page,
};
use document_session::{DocumentSessions, DocumentSessionsApi, SessionError};
use schema_engine::compile::plan::ColumnValidator;
use schema_engine::{
    CompiledSchema, Expected, SchemaEngine, SchemaEngineApi, TypeKind, TypeRegistry,
    ValidationOptions, ValuePathSegment, Violation, ViolationReason,
};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_log::log;

use crate::menu::{MenuItemSpec, MenuPath, MenuRegistry, MenuSelection};
use crate::session::watch::{TauriWindowEvents, WindowDestroyEvents};

// ---------------------------------------------------------------------------
// ウィンドウごとの保持（design.md「GridCommands」）
// ---------------------------------------------------------------------------

/// 1 つのウィンドウが表示しているシートのセッション（design.md「GridCommands」の
/// 「ウィンドウごとに 1 つ保持する」）。
///
/// セッションのほかに**計画**（[`CompiledSchema`]）、**取り消し履歴**（[`UndoStack`]）、
/// **表示中のシートの識別子**（要求と突き合わせるための文字列）を持つ。計画を持つのは違反の
/// 理由（要件 4.2）を組み立てるときに列をもう一度判定するためであり、識別子を持つのは理由を
/// 組み立てる時点で「どのシートか」を要求からではなく保持から取るためである（要求は開いた
/// ときのものであり、以後のコマンドは持ち回らない）。
///
/// # 履歴の所有者はこの保持である（要件 9.5）
///
/// 履歴は**ドキュメント単位**であり、シートごとではない（要件 9.5）— `macro-runtime` の
/// 実行が複数シートに跨るためである（design.md「UndoStack（拡張点の所有者）」）。したがって
/// 履歴は**セッションの持ち物にできない**: [`GridSession`] は開いたシートの計画
/// （`CompiledSchema`）を固定して持つため、シートを切り替えるたびに作り直される
/// （`grid_open_sheet` がこの保持ごと置き換える）。セッションが持つと、**シートを 1 度
/// 切り替えただけで文書の取り消しが効かなくなる**。
///
/// 置き換えを越えて履歴を保つ規則は [`answer_open`] が持つ:
///
/// - **保つ**: 置き換えの前に保持していたシートが、差し替え後の文書にも在るとき
///   （＝同じ 1 つの文書の中のシートの切り替えである）
/// - **捨てる**: そのシートが文書に無いとき（新規・開く・破棄で**文書が差し替わった**）。
///   この判定は既存の経路（[`answer_find_violation`] が「保持しているシートが文書に無い」を
///   見るのと同じ照合）に揃えてある
///
/// **差し替えの後に古い履歴を適用する経路は残らない**: 文書を触る 3 つの経路
/// （`apply` / `undo` / `redo`）はどれもセッションの側が「保持しているシートが文書に無い」を
/// [`GridError::SchemaUnusable`] として拒む（[`GridSession`] の `sheet_of`）。
struct SheetEntry {
    /// 表示しているシートのセッション（表示状態と違反の索引を所有する）。
    session: GridSession,
    /// **このウィンドウのドキュメントの取り消し履歴**（上限 [`DEFAULT_UNDO_LIMIT`]。
    /// 要件 9.5, 9.6）。セッションは所有しないため、文書を触る経路へは呼び出しごとに貸す
    /// （[`GridSession::apply`] / [`GridSession::undo`] / [`GridSession::redo`]）。
    history: UndoStack,
    /// 開いた時点の計画。**同じシートから**落としたものである
    /// （[`SchemaEngineApi::compile`] の事前条件）。
    schema: CompiledSchema,
    /// 表示しているシートの識別子（文書の側の文字列表現と突き合わせる）。
    sheet: String,
}

/// ウィンドウごとの [`SheetEntry`] の表と、破棄の購読（design.md「GridCommands」）。
///
/// **表のロックは参照と挿入・除去のためだけに取る。** ウィンドウごとの実体は
/// [`Arc<Mutex<SheetEntry>>`] であり、処理（`apply` / `set_view` / 探索）はそちらのロックの
/// 下で行う — 10 万行の適用が**他のウィンドウのコマンドを待たせない**ようにするためである
/// （`document-session` の表と同じ規律。要件 1.4）。
///
/// # 破棄の購読（design.md「GridCommands」の Integration）
///
/// 破棄の通知は**実行時**（イベントループとネイティブウィンドウ）にしか現れないため、
/// ウィンドウの側を [`WindowDestroyEvents`] の縫い目に閉じる（`session/watch.rs` が確立した
/// 形をそのまま使う）。購読は**セッションを置くときに 1 回だけ**登録し、登録できなければ
/// 何も置かない（登録の済んでいない保持を作らない）。通知の側ではそのウィンドウの保持だけを
/// 落とす（他のウィンドウの表示状態と履歴を変えない）。
pub struct GridSessions {
    /// ウィンドウの側（本番は [`TauriWindowEvents`]、テストは二重）。
    events: Arc<dyn WindowDestroyEvents>,
    /// ラベル → 保持。**反復しない**（観測に出る順序を持たない）ため [`HashMap`] で足りる。
    /// 破棄の通知の閉包も同じ表を掴む（通知の側で項目を取り除く）ため、[`Arc`] で共有する。
    entries: Arc<Mutex<HashMap<WindowLabel, Arc<Mutex<SheetEntry>>>>>,
    /// 購読を登録したラベル（「ウィンドウ 1 つにつき購読 1 つ」の実体）。破棄の通知の閉包も
    /// 同じ集合を掴む（通知の側で項目を取り除く）ため、[`Arc`] で共有する。
    subscribed: Arc<Mutex<HashSet<String>>>,
}

impl GridSessions {
    /// ウィンドウの側と空の表を結びつける。
    pub fn new(events: Arc<dyn WindowDestroyEvents>) -> Self {
        Self {
            events,
            entries: Arc::new(Mutex::new(HashMap::new())),
            subscribed: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// そのウィンドウの保持を**表のロックを離してから**使えるように取り出す（無ければ `None`）。
    ///
    /// 返るのはウィンドウごとの実体（[`Arc`]）であり、処理はその内側のロックの下で行う。
    fn entry(&self, label: &WindowLabel) -> Option<Arc<Mutex<SheetEntry>>> {
        lock(&self.entries).get(label).cloned()
    }

    /// セッションを置き（同じウィンドウの前の保持は置き換える）、破棄の購読を登録する。
    ///
    /// 戻り値は「置けたか」。`false` のときは**何も置いていない**（ウィンドウを引けない、
    /// または取得と登録の間に破棄された）ので、呼び出し元はセッションを作らずに失敗を返す。
    fn store(&self, label: &WindowLabel, entry: SheetEntry) -> bool {
        if !self.subscribe(label) {
            return false;
        }
        lock(&self.entries).insert(label.clone(), Arc::new(Mutex::new(entry)));
        true
    }

    /// そのウィンドウの購読の登録を 1 回だけ行う（`session/watch.rs` の `register` と同じ形）。
    ///
    /// **登録を先に行い、保持はそのあとに置く。** こうしないと、挿入だけが済んで購読の無い
    /// 保持が生まれ、ウィンドウが閉じても落ちない（design.md「GridCommands」の
    /// Integration。セッションの層が破棄の購読で同じ順序を守っている）。ウィンドウを
    /// 引けないときは購読を登録せずに `false` を返し、**呼び出し元が置かない**。
    fn subscribe(&self, label: &WindowLabel) -> bool {
        if lock(&self.subscribed).contains(label.as_str()) {
            return true;
        }

        // 閉包は集合と表を掴み、通知では `forget` と同じ後始末をする。
        let subscribed = Arc::clone(&self.subscribed);
        let entries = Arc::clone(&self.entries);
        let handler: crate::session::watch::DestroyHandler =
            Arc::new(move |window: &WindowLabel| {
                release(&subscribed, &entries, window);
                log::info!(
                    "ウィンドウの破棄でグリッドの保持を手放した: label={}",
                    window.as_str()
                );
            });
        if !self.events.subscribe_destroyed(label, handler) {
            // 取得と登録の間に破棄された。**先に後始末をしてから**失敗を返す。
            log::debug!(
                "破棄の購読を登録できないためグリッドの保持を手放す: label={}",
                label.as_str()
            );
            self.forget(label);
            return false;
        }
        lock(&self.subscribed).insert(label.as_str().to_owned());
        log::info!("グリッドの破棄の購読を登録した: label={}", label.as_str());
        true
    }

    /// そのウィンドウの購読の登録と保持を取り除く（通知と掃除の共有部分）。
    fn forget(&self, label: &WindowLabel) {
        release(&self.subscribed, &self.entries, label);
    }
}

/// そのウィンドウの登録と保持を取り除く（**破棄の通知と掃除の経路が共有する唯一の実体**）。
fn release(
    subscribed: &Mutex<HashSet<String>>,
    entries: &Mutex<HashMap<WindowLabel, Arc<Mutex<SheetEntry>>>>,
    window: &WindowLabel,
) {
    lock(subscribed).remove(window.as_str());
    lock(entries).remove(window);
}

/// ロックを取る（毒された場合も中身を使う。パニックの伝播より、表を読めることを優先する）。
///
/// `session/watch.rs` と同じ扱いである — ロックの内側でパニックする経路を持たないため、
/// 毒は実質的に起きないが、起きたときに他のウィンドウの操作まで巻き添えにしない。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// 管理状態（アプリ全体で 1 実体）
// ---------------------------------------------------------------------------

/// 管理状態の生成を直列化する（下の [`grid_state`] を参照）。
static GRID_STATE_CREATION: Mutex<()> = Mutex::new(());

/// アプリ全体で 1 実体の表を取る。**初回に作る**（`lifecycle::run` は本タスクの境界の外に
/// あり、起動時に `manage` する行を足せない）。
///
/// 作るのは 1 回だけである（[`GRID_STATE_CREATION`] が確認と生成を直列化する。
/// `Manager::manage` は既に同じ型があれば上書きせず `false` を返すので、二重に作っても
/// **最初の 1 つが正**である）。ドキュメントの側（`session::install`）が作る
/// `Arc<WindowDestroyWatch>` とは別の実体である — あちらは文書の保持を、こちらは表示状態と
/// 履歴を所有し、寿命も破棄の購読も別である。
///
/// **検証専用の分岐は無い。** 表は常にこの 1 経路で作られる。
fn grid_state(app: &AppHandle) -> State<'_, GridSessions> {
    if app.try_state::<GridSessions>().is_none() {
        let _guard = lock(&GRID_STATE_CREATION);
        if app.try_state::<GridSessions>().is_none() {
            let events: Arc<dyn WindowDestroyEvents> =
                Arc::new(TauriWindowEvents::new(app.clone()));
            let _ = app.manage(GridSessions::new(events));
            log::info!("グリッドの表を管理状態として置いた");
        }
    }
    app.state::<GridSessions>()
}

/// 文書を保持している表（セッションの層が管理状態へ置いたもの）を取り出す。
///
/// **セッションと同じ実体**を使う（別に作ると「開いているドキュメント」の真実が 2 つに割れる。
/// `session/mod.rs` の module doc）。
fn documents_of(app: &AppHandle) -> Arc<DocumentSessions> {
    Arc::clone(
        app.state::<Arc<crate::session::watch::WindowDestroyWatch>>()
            .sessions(),
    )
}

/// Tauri が注入した呼び出し元ウィンドウを、境界の文脈（要件 4.6）へ写す。
///
/// **境界の型を新設しない。** `app_shell::ipc::WindowContext` / `WindowLabel` をそのまま使う
/// （3 つ目の識別子を作らない。`session/commands.rs` と同じ形）。
fn caller_context(window: &WebviewWindow) -> WindowContext {
    WindowContext {
        window: WindowLabel::new(window.label()),
    }
}

// ---------------------------------------------------------------------------
// 失敗の写像（文言を組み立てるのは適応層の仕事である）
// ---------------------------------------------------------------------------

/// 経路そのものが成立しないことを伝える失敗（封筒の失敗腕）。
fn path_failure(command: &str, label: &WindowLabel, reason: &str) -> IpcError {
    IpcError::Document {
        message: format!(
            "{command}: ウィンドウ {} の経路が成立しない: {reason}",
            label.as_str()
        ),
    }
}

/// グリッドがまだ開かれていないことを伝える失敗（要件 1.1 の順序）。
///
/// `grid_open_sheet` を通らずに表示の指定・編集・履歴・探索を呼んだ場合である。**経路の
/// 失敗**であり、ドメインの判定結果ではない（画面は開いてから呼び直す）。
fn not_open(command: &str, label: &WindowLabel) -> IpcError {
    path_failure(command, label, "グリッドがまだ開かれていない")
}

/// 「要求・保持しているシートが文書に無い」の理由（**文言の唯一の源**）。
///
/// 3 つの経路が同じ状態を報告する（開く要求のシートが無い・探索の保持シートが無い・
/// 生バイトの要求のシートが保持と違う）ため、文言を組み立てる場所を 1 つに閉じる。
fn unknown_sheet(sheet: &str) -> String {
    format!("シート {sheet} が文書に無い")
}

/// ドメインの誤りを封筒の失敗腕へ写す（design.md「Error Handling」の「操作の誤り」）。
///
/// **値の不適合はここへ来ない** — 違反は値を保持したまま成功腕の要約に載る（要件 3.5）。
/// ここへ来るのは範囲外のセル・未知の行・解釈できない入れ子の表現であり、画面は要求を
/// 直して呼び直す（`design.md` の同表）。
fn grid_failure(command: &str, label: &WindowLabel, error: &GridError) -> IpcError {
    path_failure(command, label, &format!("グリッドを進められない: {error}"))
}

/// セッションの誤りを封筒の失敗腕へ写す。
///
/// **保持していない**（[`SessionError::NoDocument`]）は利用者が画面を開いた順序の問題であり、
/// 読み込みの失敗は文書そのものの問題である。どちらも「このコマンドの経路が成立しない」
/// であり、判定の結果ではない（`session/commands.rs` の `document_state` が同じ失敗を
/// **状態として**答えるのと対照的である — あちらは状態の問い合わせであり、こちらは操作である）。
fn session_failure(command: &str, label: &WindowLabel, error: &SessionError) -> IpcError {
    let reason = match error {
        SessionError::NoDocument => "ドキュメントを保持していない".to_owned(),
        SessionError::Busy => "別の操作が進行中である".to_owned(),
        SessionError::UnsavedChanges => "未保存の変更が解決されていない".to_owned(),
        SessionError::Read { source } => format!("ドキュメントを読めない: {source}"),
    };
    path_failure(command, label, &reason)
}

/// 件数を境界の [`u32`] へ写す（64 ビット整数を境界へ出さない規約。`ipc/document.rs`）。
///
/// 上限を超える値は**飽和させる** — `as` による切り捨ては、嘘の小さい数を利用者へ見せる
/// （32 ビットを超えるシートは現実には作れないが、写像が黙って壊れる形にはしない。
/// `session/commands.rs` の `count_to_u32` と同じ判断である）。
fn count_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// セッションの世代を、境界の表現（**10 進の文字列**）へ写す（タスク 10.1）。
///
/// `GridSession::generation()` の写しそのものであり、**u64 を数値として境界へ出さない**
/// （生成物の TS の数は 2^53 までであり、上位のバイトが消える）。画面はこの文字列をそのまま
/// 持ち回り、`grid_rows_window` の要求の頭へ載せるときだけ u64 へ戻す。
///
/// **1 つのコマンドの内側で世代は複数回進む**（`answer_set_view` の手順 1 と 3）ので、応答を
/// 組み立てる直前に読む — 呼び出しの途中で読んだ値を控えてはならない（数え直しの規則を
/// 適応層へ持ち込むことになる）。
fn generation_text(generation: Generation) -> String {
    generation.get().to_string()
}

// ---------------------------------------------------------------------------
// 境界からドメインへの変換（要求の側）
// ---------------------------------------------------------------------------

/// 表示の指定をドメインの [`ViewSpec`] へ写す（要件 8.3、8.4）。
fn view_spec(view: &GridViewSpec) -> ViewSpec {
    ViewSpec {
        sort: view
            .sort
            .iter()
            .map(|key| SortKey {
                column: column_index(key.column),
                descending: key.descending,
            })
            .collect(),
        filters: view.filters.iter().map(filter_spec).collect(),
    }
}

/// 並べ替えの基準列 1 本を写す。
fn filter_spec(filter: &GridFilterSpec) -> FilterSpec {
    match filter {
        GridFilterSpec::Equals { column, text } => FilterSpec::Equals {
            column: column_index(*column),
            text: text.clone(),
        },
        GridFilterSpec::Contains { column, text } => FilterSpec::Contains {
            column: column_index(*column),
            text: text.clone(),
        },
        GridFilterSpec::IsEmpty { column } => FilterSpec::IsEmpty {
            column: column_index(*column),
        },
        GridFilterSpec::IsNotEmpty { column } => FilterSpec::IsNotEmpty {
            column: column_index(*column),
        },
        // **列を問わない指定と、列を指定した要求を区別する**（6.1 の同型の doc）。
        GridFilterSpec::HasViolation { column } => FilterSpec::HasViolation {
            column: column.map(column_index),
        },
    }
}

/// 展開の状態 1 列ぶんを写す（要件 5.1〜5.4）。
fn expansion_state(state: &GridExpansionState) -> ExpansionState {
    ExpansionState {
        column: column_index(state.column),
        expanded: state.expanded,
        depth: state.depth,
    }
}

/// 境界の列の添字をドメインの [`ColumnIndex`] へ写す。
///
/// 範囲の検査はここでしない — 範囲外の列はドメインの判定（`ColumnOutOfRange`）が答える
/// （境界で 2 つ目の規則を作らない）。**この入口を使うのは表示の指定と展開である**
/// （範囲外の列は「その要求を拒む」で足りる経路。表の描画は列の構成が変われば追随する）。
fn column_index(column: u32) -> ColumnIndex {
    ColumnIndex::new(column as usize)
}

/// 編集の経路が使う列の写像（**宣言の列数で検査してから**写す）。
///
/// 範囲の規則そのものはドメインが持つ。`edit` 層の `usable_columns` は計画の列数
/// （`CompiledSchema::column_count`）を返し、各命令はその数と列を比べる —
/// 本関数は**同じ数**を同じ源（保持している計画）から取る。したがってこれは 2 つ目の規則
/// ではなく、**同じ判定を閉包の外で先に通す**ものである。
///
/// 先に通すのは、閉包の内側で失敗すると [`DocumentSessionsApi::edit`] が「閉包が文書を
/// 変えたか」を判定できず、**1 つのセルも書いていないのに未保存の印を立てる**ためである
/// （[`answer_apply_edit`] の doc「命令の変換は閉包の外で済ませる」）。
///
/// **文書そのものの前提**（シートが無い・列数が食い違う → `SchemaUnusable`）は依然として
/// 閉包の内側で決まる。あれは要求ではなく**文書の現在の内容**に依る失敗であり、変換の時点
/// では決められない（[`answer_apply_edit`] の doc を参照）。
fn checked_column_index(column: u32, columns: usize) -> Result<ColumnIndex, String> {
    if column as usize >= columns {
        return Err(format!(
            "編集命令の列 {column} が宣言の列数 {columns} の外にある"
        ));
    }
    Ok(ColumnIndex::new(column as usize))
}

/// 行の識別子の文字列を、行の識別子の型へ解釈する。失敗は**理由の文字列**である
/// （封筒への写像は呼び出し元が 1 箇所で行う。`path_failure` の doc）。
///
/// **型を名指ししない**（`src-tauri` は `document-format` を通常依存に持たない。module doc
/// 「このクレートが `document-format` を通常依存に持たないこと」）。解釈できない文字列は
/// 経路の失敗である（ドメインの判定に到達しない）。
fn parse_row<T>(text: &str) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    text.parse()
        .map_err(|error| format!("編集命令の行の識別子 {text} を解釈できない: {error}"))
}

/// 境界のセルの位置をドメインの [`CellAddress`] へ写す（要件 3.3、8.6）。
///
/// 列は [`checked_column_index`] を通す（宣言の列数の外の列はここで失敗する）。
fn cell_address(cell: &GridCellAddress, columns: usize) -> Result<CellAddress, String> {
    Ok(CellAddress::new(
        parse_row(&cell.row)?,
        checked_column_index(cell.column, columns)?,
    ))
}

/// 行の識別子の並びを写す（行を取り除く・複製する命令と、貼り付けの表示の並び）。
fn row_ids<T>(rows: &[String]) -> Result<Vec<T>, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    rows.iter().map(|row| parse_row(row)).collect()
}

/// 編集命令をドメインの [`EditCommand`] へ写す（要件 3.3、5.7、6.1、6.3、7.3、8.9）。
///
/// 6 つの命令を過不足なく写す（ワイルドカードを使わない — `data-grid` に命令が増えれば
/// ここがコンパイルエラーになり、境界の形を追随させ忘れない）。**値を型付きで運ばない**
/// 規約は 6.1 の型が既に守っているため、ここは文字列と位置をそのまま渡す。
///
/// `columns` は**保持している計画の列数**である（[`checked_column_index`] の doc）。
/// 列を運ぶ 3 つの命令（`SetCells` / `SetNested` / `PasteRange` の錨）は、ここで宣言の
/// 列数の外を弾く — 弾かなければ失敗が適用の閉包の内側で起き、文書を 1 つも変えていないのに
/// 未保存の印が立つ。
///
/// 失敗は**理由の文字列**である（封筒への写像は [`answer_apply_edit`] が 1 箇所で行う）。
fn edit_command(command: &GridEditCommand, columns: usize) -> Result<EditCommand, String> {
    Ok(match command {
        GridEditCommand::SetCells { cells } => {
            let mut converted = Vec::with_capacity(cells.len());
            for cell in cells {
                converted.push((cell_address(&cell.cell, columns)?, cell.text.clone()));
            }
            EditCommand::SetCells { cells: converted }
        }
        GridEditCommand::SetNested { cell, json } => EditCommand::SetNested {
            cell: cell_address(cell, columns)?,
            json: json.clone(),
        },
        GridEditCommand::InsertRows { at, count } => EditCommand::InsertRows {
            at: RowOrdinal::new(*at as usize),
            count: *count as usize,
        },
        GridEditCommand::RemoveRows { rows } => EditCommand::RemoveRows {
            rows: row_ids(rows)?,
        },
        GridEditCommand::DuplicateRows { rows } => EditCommand::DuplicateRows {
            rows: row_ids(rows)?,
        },
        GridEditCommand::PasteRange { anchor, rows, text } => EditCommand::PasteRange {
            anchor: cell_address(anchor, columns)?,
            rows: row_ids(rows)?,
            text: text.clone(),
        },
    })
}

// ---------------------------------------------------------------------------
// ドメインから境界への変換（応答の側）
// ---------------------------------------------------------------------------

/// 型の種別を境界の札へ写す（**総関数**。要件 3.1、3.2、10.1〜10.4）。
///
/// ワイルドカードを書かないため、[`TypeKind`] に変種が増えればここがコンパイルエラーになり、
/// 境界の札を追随させ忘れない。**綴りは変種名そのまま**であり（[`TypeKindTag`] の doc が
/// 定める）、`match` の腕が写すのは名前だけである — 綴りが食い違えば
/// 同名のテスト（`type_kind_tag_covers_every_type_kind`）が落ちる。
fn type_kind_tag(kind: TypeKind) -> TypeKindTag {
    match kind {
        TypeKind::Int => TypeKindTag::Int,
        TypeKind::Float => TypeKindTag::Float,
        TypeKind::Decimal => TypeKindTag::Decimal,
        TypeKind::Text => TypeKindTag::Text,
        TypeKind::Bool => TypeKindTag::Bool,
        TypeKind::Date => TypeKindTag::Date,
        TypeKind::DateTime => TypeKindTag::DateTime,
        TypeKind::Enum => TypeKindTag::Enum,
        TypeKind::Ref => TypeKindTag::Ref,
        TypeKind::Attachment => TypeKindTag::Attachment,
        TypeKind::Object => TypeKindTag::Object,
        TypeKind::Array => TypeKindTag::Array,
        TypeKind::Any => TypeKindTag::Any,
        TypeKind::Custom => TypeKindTag::Custom,
    }
}

/// 展開の可否を写す（3 状態を潰さない。要件 5.4）。
fn expandability_to_boundary(expandability: Expandability) -> ColumnExpandability {
    match expandability {
        Expandability::Available => ColumnExpandability::Available,
        Expandability::Capped => ColumnExpandability::Capped,
        Expandability::Leaf => ColumnExpandability::Leaf,
    }
}

/// 要素数の能力を写す（要件 5.6）。**`None` は開いた端点**であり 0 とは違う（6.1 の doc）。
fn element_count_to_boundary(count: &ElementCount) -> ColumnElementCount {
    ColumnElementCount {
        items: type_kind_tag(count.items),
        min: count.min.map(count_to_u32),
        max: count.max.map(count_to_u32),
    }
}

/// 入れ子の内側の位置の 1 段を、境界の段へ写す（`view` 層の段から。要件 4.5、5.5）。
fn nested_segment(segment: &NestedPathSegment) -> GridPathSegment {
    match segment {
        NestedPathSegment::Field(name) => GridPathSegment::Field {
            name: name.to_string(),
        },
        NestedPathSegment::Index(position) => GridPathSegment::Index {
            position: count_to_u32(*position),
        },
    }
}

/// 違反の内側の位置の 1 段を写す（`schema-engine` の段から）。[`nested_segment`] と同じ形へ
/// 落とすが、**別の型から**写す（`data-grid` の段と検証の段は寿命も変更の理由も違う。
/// `types` の module doc）。
fn value_path_segment(segment: &ValuePathSegment) -> GridPathSegment {
    match segment {
        ValuePathSegment::Field(name) => GridPathSegment::Field {
            name: name.to_string(),
        },
        ValuePathSegment::Index(position) => GridPathSegment::Index {
            position: count_to_u32(*position),
        },
    }
}

/// 構成の 1 列を写す（要件 1.1、1.2、3.1、5.1、5.4、5.6。**タスク 10.3 が宣言の材料を足した**
/// — 要件 3.2、3.7、3.8、5.5、10.1、10.4）。
///
/// **本関数が宣言の材料を境界へ出す唯一の場所である。**写すのは `view` 層の
/// `ColumnDeclaration` そのものであり、ここで判断を足さない（材料が無い欄は空／`None` のまま
/// 運び、面が既定へ落ちる道を残す。要件 10.4）。
///
/// `names` は**文書が持つシートの名の表**である。宣言が持つのは参照先のシートの**識別子**
/// であり、人が読む名は文書が持つため、名への写しは文書を見られる本層が行う
/// （[`sheet_names`]）。表に無い識別子は**識別子のまま**載せる（名が引けないことを
/// 「参照していない」と混同させない — 参照先のシートが文書に無い場合でも、画面は
/// 「どこを参照しているか」を名乗れる）。
fn column_to_boundary(column: &LayoutColumn, names: &SheetNames) -> ColumnDescriptor {
    ColumnDescriptor {
        column: count_to_u32(column.column.index()),
        path: column.path.segments().iter().map(nested_segment).collect(),
        name: column.name.clone(),
        kind: column.kind.map(type_kind_tag),
        element_count: column.element_count.as_ref().map(element_count_to_boundary),
        expandability: expandability_to_boundary(column.expandability),
        nullable: column.declaration.nullable,
        // **宣言は 1 つの文字列の並びだけを持つ**ため、値と名は同じ文字列になる（欄を 2 つ
        // 持つのは、面が値と名を別々に描けるようにするためである。`ColumnChoice` の doc）。
        choices: column
            .declaration
            .choices
            .iter()
            .map(|choice| ColumnChoice {
                value: choice.to_string(),
                label: choice.to_string(),
            })
            .collect(),
        reference_sheet: column
            .declaration
            .reference_sheet
            .as_deref()
            .map(|sheet| sheet_name(names, sheet)),
        custom_type_id: column
            .declaration
            .custom_type_id
            .as_deref()
            .map(str::to_owned),
        members: column
            .declaration
            .members
            .iter()
            .map(|member| ColumnMemberDescriptor {
                path: member.path.segments().iter().map(nested_segment).collect(),
                name: member.name.to_string(),
                kind: type_kind_tag(member.kind),
                nullable: member.nullable,
                choices: member
                    .choices
                    .iter()
                    .map(|choice| ColumnChoice {
                        value: choice.to_string(),
                        label: choice.to_string(),
                    })
                    .collect(),
                custom_type_id: member.custom_type_id.as_deref().map(str::to_owned),
            })
            .collect(),
    }
}

/// 文書が持つシートの（識別子, 名）の表（要件 3.8）。
///
/// **宣言が持つのは参照先のシートの識別子であり、人が読む名は文書が持つ。**写しは本層が
/// 行う（文書を見られるのはここだけである）。1 つのコマンドの内側で 1 度だけ組み立て、
/// 列の写しの間で使い回す（列ごとに文書を走査しない）。
/// **`document_format::Document` を名指す関数は置かない** — あれは本クレートの dev-dependency
/// であり（テストだけが標本を組むのに使う）、出荷する経路からは参照できない。表を組むのは
/// 文書を読む 2 箇所（開く経路と表示の指定を変える経路）の内側であり、そこで型は推論される。
type SheetNames = Vec<(String, String)>;

/// シートの識別子から人が読む名を引く（引けなければ**識別子のまま**返す）。
///
/// 引けないことは「参照していない」ではない（参照先のシートが文書に無い場合である）ため、
/// 空文字へ落とさない — 画面は識別子を名乗り、行の一覧は空になる。
fn sheet_name(names: &SheetNames, id: &str) -> String {
    names
        .iter()
        .find(|(candidate, _)| candidate == id)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| id.to_owned())
}

/// **いまの**列の構成を写す（左から右への表示順。要件 1.1、1.2、5.1、5.2、5.4）。
///
/// `GridSession::columns()` が返すのは**導出後**の `ColumnLayout` であり、`set_view` と
/// `set_expansion` を適用したあとの状態を映す（導出そのものはドメインの仕事であり、本モジュールは
/// 写すだけである）。開く経路と表示の指定を変える経路が**同じ 1 つの写し**を使うのは、2 つの
/// 経路で列の構成の意味が食い違わないようにするためである。
///
/// `names`（文書のシートの名の表）を取るのは、参照の列（`Ref`）が**名**を運ぶためである
/// （タスク 10.3。宣言は識別子しか持たない）。
fn layout_columns(session: &GridSession, names: &SheetNames) -> Vec<ColumnDescriptor> {
    session
        .columns()
        .iter()
        .map(|column| column_to_boundary(column, names))
        .collect()
}

/// シートの要約を組み立てる（要件 1.1、1.5、1.6）。
///
/// **列の構成と行数を同じ型に載せる**（2 つの空の状態を列の数で区別する。6.1 の
/// `GridSheetSummary` の doc）。行数は**シートの行数**であり、可視行数ではない
/// （絞り込みの結果は [`GridViewResponse`] が運ぶ）。
fn sheet_summary(session: &GridSession, rows: usize, names: &SheetNames) -> GridSheetSummary {
    GridSheetSummary {
        columns: layout_columns(session, names),
        row_count: count_to_u32(rows),
    }
}

/// セルの位置を写す（境界では行は文字列、列は [`u32`]）。
fn cell_to_boundary(cell: CellAddress) -> GridCellAddress {
    GridCellAddress {
        row: cell.row().to_string(),
        column: count_to_u32(cell.column().index()),
    }
}

/// 型強制の記録を写す（要件 3.4）。**前後の表示文字列はドメインが既に持っている**
/// （本モジュールは組み立て直さない）。
fn coercion_to_boundary(notice: &CoercionNotice) -> GridCoercionNotice {
    GridCoercionNotice {
        cell: cell_to_boundary(notice.cell),
        before: notice.before.clone(),
        after: notice.after.clone(),
    }
}

/// 違反の位置を写す（要件 4.2、4.5、6.4、7.5）。
///
/// 行は**文字列**（64 ビット整数を境界へ出さない規約）、列は [`u32`]、内側の位置は段の並びで
/// ある。行を持たない違反（列そのものの問題）は `None` として写る。
fn violation_location(violation: &Violation) -> GridViolationLocation {
    GridViolationLocation {
        row: violation.row().map(|row| row.to_string()),
        column: count_to_u32(violation.column().index()),
        path: violation
            .path()
            .segments()
            .iter()
            .map(value_path_segment)
            .collect(),
    }
}

/// 編集の要約を写す（要件 3.4、4.3、4.6、6.2、6.4、7.5）。
///
/// **違反の総数はシート全体の数を載せる**（要件 4.3）。[`EditOutcome::violation_total`] は
/// 再検証した列に閉じた総数であり（6.1 の同型の欄の doc）、シート全体へ閉じるのは本モジュール
/// の仕事である — 差分で最新に保たれている数を [`GridSession::violation_total`] から読む。
/// **そのために検証を呼び直すことは無い**（要件 11.4）。
///
/// **適用先が表示中のシートでないときは、この関数を使わない**（[`untouched_sheet_outcome`]）。
fn outcome_to_boundary(session: &GridSession, outcome: &EditOutcome) -> GridEditOutcome {
    GridEditOutcome {
        affected: outcome.affected.iter().map(|row| row.to_string()).collect(),
        coercions: outcome.coercions.iter().map(coercion_to_boundary).collect(),
        violation_total: count_to_u32(session.violation_total()),
        violations: outcome.violations.iter().map(violation_location).collect(),
        revalidated_columns: outcome
            .revalidated_columns
            .iter()
            .map(|column| count_to_u32(column.index()))
            .collect(),
        row_count: count_to_u32(outcome.row_count),
    }
}

/// **表示中のシートを記述する**応答を組み立てる（履歴の 1 歩が別のシートへ落ちたとき。要件 4.3、9.5）。
///
/// 履歴はドキュメント単位であるため（要件 9.5）、`grid_history` が進める 1 歩は**表示して
/// いるシートとは別のシート**を指しうる（`answer_open` が履歴を持ち出し、以後の取り消しは
/// 切り替える前のシートの操作を指すためである）。適用先はドメインが名乗る
/// （`EditOutcome::sheet`）ので、本層はそれを見分けられる。
///
/// そのとき**表示中のシートの中身は 1 つも変わっていない**。したがって応答が運ぶ材料は
/// 表示中のシートのものでなければならない（画面はこの 6 つの欄をそのまま採用する —
/// `./GridScreen` の `appliedRowOperation` と `./history` の `applyHistory`）:
///
/// | 欄 | なにを載せるか | 理由 |
/// |---|---|---|
/// | `violation_total` | 表示中のシートの総数（`session.violation_total()`） | ドメインが索引に触れていないため、この数は表示中のシートのままである（`GridSession::settle` の docs） |
/// | `violations` / `revalidated_columns` | **空** | 別のシートの位置である。画面は印と巡回の材料に使うため、載せれば**別のシートの違反を表示中のセルの印にする** |
/// | `affected` | **空** | 別のシートの行識別子である。画面はこれを現在位置の移動と窓の作り直し（`WindowCache.clear(row_count)`）に使う |
/// | `coercions` | **空** | 同じく別のシートの位置である（復元の経路は強制を行わないため、実際にはつねに空である） |
/// | `row_count` | 表示中のシートの行数（`displayed_rows`） | 画面はこれを**表示している表の行数**として採用する（`ready.summary.row_count`）。別のシートの数を載せると、表示中のシートの行数が別のシートの数になる |
fn untouched_sheet_outcome(session: &GridSession, displayed_rows: usize) -> GridEditOutcome {
    GridEditOutcome {
        affected: Vec::new(),
        coercions: Vec::new(),
        violation_total: count_to_u32(session.violation_total()),
        violations: Vec::new(),
        revalidated_columns: Vec::new(),
        row_count: count_to_u32(displayed_rows),
    }
}

/// 期待した内容を、利用者へ伝える語へ組み立てる（要件 4.2）。
///
/// ドメインの [`Expected`] は表示用の文言を持たない（`schema-engine` の doc）ため、
/// 語を組み立てるのは適応層の仕事である。**変種ごとに書き分ける** — 利用者が取るべき行動が
/// 型・範囲・書式・選択肢で異なるためである（1 つの文言に畳まない）。
///
/// 値の表示文字列は `data-grid` の `display_text` から取る — **表示の規則の写しを作らない**
/// （画面が窓に載せる文字列と同じものである。要件 4.2 の文言が画面と食い違わない）。
/// 値の型（`document-format` の `CellValue`）は**名指ししない**（本クレートは通常依存に
/// 持たないため、閉包と引数の型は文脈から推論させる）。
fn describe_expected(expected: &Expected) -> String {
    match expected {
        Expected::Kind(kind) => format!("{kind} 型"),
        Expected::Range { min, max } => match (min, max) {
            (Some(min), Some(max)) => {
                format!("{}〜{} の範囲", display_text(min), display_text(max))
            }
            (Some(min), None) => format!("{} 以上", display_text(min)),
            (None, Some(max)) => format!("{} 以下", display_text(max)),
            (None, None) => "範囲の指定".to_owned(),
        },
        Expected::Length { min, max } => match (min, max) {
            (Some(min), Some(max)) => format!("長さ {min}〜{max} 文字"),
            (Some(min), None) => format!("長さ {min} 文字以上"),
            (None, Some(max)) => format!("長さ {max} 文字以下"),
            (None, None) => "長さの指定".to_owned(),
        },
        Expected::Pattern(pattern) => format!("書式 {pattern} に一致する文字列"),
        Expected::Decimal { precision, scale } => {
            format!("有効桁数 {precision}・小数点以下 {scale} 桁の 10 進数")
        }
        Expected::Choices(choices) => format!("選択肢 {}", choices.join(" / ")),
        Expected::Present => "値なしを許さないこと".to_owned(),
        Expected::Unique => "一意であること".to_owned(),
        Expected::RowsOf(sheet) => format!("シート {sheet} に実在する行"),
        Expected::Usable => "使用可能な列であること".to_owned(),
        Expected::AcceptedBy(identifier) => format!("拡張型 {identifier} が受理すること"),
    }
}

/// 違反の理由を、利用者へ伝える文言へ組み立てる（要件 4.2）。
///
/// **変種ごとに書き分ける**（12 変種。`Expected` の語は [`describe_expected`] に委ねる）。
/// 値は表示文字列として埋め込む（画面と同じ見え方にする。`display_text` を呼ぶ理由は
/// [`describe_expected`] の doc を参照）。
fn describe_reason(reason: &ViolationReason) -> String {
    match reason {
        ViolationReason::TypeMismatch { expected, actual } => format!(
            "{}の値を期待したが「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::OutOfRange { expected, actual } => format!(
            "{}の外の値「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::LengthOutOfRange { expected, actual } => format!(
            "{}に合わない長さの値「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::PatternMismatch { expected, actual } => format!(
            "{}に一致しない値「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::ChoiceNotAllowed { expected, actual } => format!(
            "{}に無い値「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::PrecisionExceeded { expected, actual } => format!(
            "{}を超える値「{}」がある",
            describe_expected(expected),
            display_text(actual)
        ),
        // 値なしそのものが違反である（`actual` はつねに値なしなので、文言へ埋め込まない）。
        ViolationReason::MissingValue { .. } => {
            "値なしを許さない列またはフィールドに値が無い".to_owned()
        }
        ViolationReason::Duplicate {
            expected,
            actual,
            rows,
        } => format!(
            "{}に反する値「{}」が {} 行にある",
            describe_expected(expected),
            display_text(actual),
            rows.len()
        ),
        ViolationReason::BrokenReference {
            expected, actual, ..
        } => format!(
            "{}が実在しない（参照先の値は「{}」）",
            describe_expected(expected),
            display_text(actual)
        ),
        ViolationReason::UnusableColumn { kind, .. } => {
            format!("列の型 {kind} を解釈できないため、この列の値は使えない")
        }
        ViolationReason::CustomRejected { reason, .. } => {
            format!("拡張型が値を拒否した: {reason}")
        }
        ViolationReason::CustomFailed { reason, .. } => {
            format!("拡張型の判定が失敗した: {reason}")
        }
    }
}

// ---------------------------------------------------------------------------
// 生バイト経路の要求の頭（タスク 6.3）
//
// 引数の配置は**本節が唯一の源**である。`data-grid` の `transport` 層は `WindowRequest` を
// **組み立て済みの値**として受け取り、バイト列を読まない（5.1 が「要求の頭の復号は 6.3 の
// 適応層が行う」と定めている）。全体像はモジュール docs の「生バイト経路」節にある。
// ---------------------------------------------------------------------------

/// 要求の頭の版（1 バイト目）。
///
/// **窓の版（`WINDOW_FORMAT_VERSION`）とは別の体系である** — 要求は要求の配置、窓は窓の配置を
/// 持つ（モジュール docs「引数の配置」）。欄を足すときはこの数を上げる。
pub const WINDOW_REQUEST_VERSION: u8 = 1;

/// 要求の頭の**固定部分**の幅（バイト）: 版 1 + 世代 8 + 開始序数 8 + 行数 8 + シート長 8。
///
/// 引数の全体の長さは `WINDOW_REQUEST_HEADER_LEN + シート長` である。窓の `HEADER_LEN` と
/// 同じ 33 であるのは偶然ではない — 数の欄を位置だけで引ける固定の幅にすると、復号が前へ
/// 1 回走査するだけで閉じる（モジュール docs の理由 1）。
pub const WINDOW_REQUEST_HEADER_LEN: usize = 33;

/// 生バイトの引数から読み取った要求（[`WindowRequest`] へ写す前の値）。
///
/// [`WindowRequest`] がシートを持たない（セッションが 1 枚のシートに閉じている）ため、
/// 要求の側にだけ現れるシートはここで保持し、渡す前に**保持しているシートと突き合わせる**
/// （モジュール docs「シートの照合」）。
struct WindowArgument {
    /// 要求が対象とするシート。**一度も記録へ流さない**（要件 8.4 の規律。
    /// `bulk` のモジュール doc「記録に何を書くか」と同じ扱いである）。
    sheet: String,
    /// 要求が名乗る世代（[`WindowCodec::is_stale`] が現在の世代と比べる）。
    generation: Generation,
    /// 可視行の序数の区間（**文書の位置ではない**）。
    span: RowSpan,
}

/// 生バイトの引数を復号する（**前方 1 回の走査**。モジュール docs「引数の配置」）。
///
/// 失敗は**理由の文字列**である（空の窓への写像と記録は [`answer_rows_window`] が 1 箇所で
/// 行う）。理由に**シートの識別子の値そのものを入れない**のは、記録へ内容を流さないためで
/// ある（要件 8.4）— 入るのは長さ・版・位置だけである。
///
/// # 検査（窓の復号と同じ規律）
///
/// - 頭に満たない入力（`WINDOW_REQUEST_HEADER_LEN` 未満）を拒む
/// - 知らない版を拒む
/// - **シートの識別子が宣言した長さに足りない入力も、余分なバイトを持つ入力も拒む**
///   （余りを黙って捨てると、壊れた要求が正常に見える）
/// - シートの識別子が UTF-8 でなければ拒む
/// - 数の欄は `usize` へ落とす。落ちない値（32 ビットのホストで 64 ビットの値）は拒む —
///   窓の復号が `WindowDecodeError::TooLarge` で同じことをする
///
/// **panic しない。** この引数は webview から届くバイト列であり、壊れた入力は誤りとして
/// 返さなければならない（呼び出し元が空の窓へ写す）。
fn decode_window_argument(bytes: &[u8]) -> Result<WindowArgument, String> {
    /// 位置 `at` から 8 バイトを u64 として読む（呼び出し元が長さを確かめてから呼ぶ）。
    fn u64_at(bytes: &[u8], at: usize) -> u64 {
        let mut field = [0_u8; 8];
        field.copy_from_slice(&bytes[at..at + 8]);
        u64::from_le_bytes(field)
    }

    /// 64 ビットの値を `usize` へ落とす（落ちなければ理由を返す）。
    fn to_usize(value: u64, what: &str) -> Result<usize, String> {
        usize::try_from(value).map_err(|_| format!("要求の{what} {value} がこのホストで表せない"))
    }

    if bytes.len() < WINDOW_REQUEST_HEADER_LEN {
        return Err(format!(
            "要求の頭が {} バイトに足りない（{} バイト以上が要る）",
            bytes.len(),
            WINDOW_REQUEST_HEADER_LEN
        ));
    }
    let version = bytes[0];
    if version != WINDOW_REQUEST_VERSION {
        return Err(format!(
            "知らない版の要求である（版 {version}。知っているのは {WINDOW_REQUEST_VERSION} だけである）"
        ));
    }

    let generation = Generation::new(u64_at(bytes, 1));
    let start = to_usize(u64_at(bytes, 9), "開始序数")?;
    let count = to_usize(u64_at(bytes, 17), "行数")?;
    let sheet_len = to_usize(u64_at(bytes, 25), "シートの識別子の長さ")?;
    let total = WINDOW_REQUEST_HEADER_LEN
        .checked_add(sheet_len)
        .ok_or_else(|| format!("要求のシートの識別子の長さ {sheet_len} が大きすぎる"))?;
    match bytes.len().cmp(&total) {
        core::cmp::Ordering::Less => {
            return Err(format!(
                "要求のシートの識別子が {sheet_len} バイトに足りない（{} バイト在る）",
                bytes.len() - WINDOW_REQUEST_HEADER_LEN
            ));
        }
        core::cmp::Ordering::Greater => {
            return Err(format!(
                "要求の後ろに余分なバイトが {} バイト在る",
                bytes.len() - total
            ));
        }
        core::cmp::Ordering::Equal => {}
    }
    // `RowSpan::new` の前提は「開始序数 + 行数が `usize` を溢れないこと」である
    // （`types` の同型の doc）。ここで確かめるのは、壊れた要求でその前提を破らないためである。
    if start.checked_add(count).is_none() {
        return Err(format!(
            "要求の区間が大きすぎる（開始序数 {start} + 行数 {count}）"
        ));
    }
    let sheet = core::str::from_utf8(&bytes[WINDOW_REQUEST_HEADER_LEN..])
        .map_err(|error| format!("要求のシートの識別子が UTF-8 でない: {error}"))?
        .to_owned();

    Ok(WindowArgument {
        sheet,
        generation,
        span: RowSpan::new(RowOrdinal::new(start), count),
    })
}

// ---------------------------------------------------------------------------
// 本体（Tauri に依らない。テストはここを直接駆動する）
// ---------------------------------------------------------------------------

/// シートを開く本体（[`grid_open_sheet`] の中身。要件 1.1、1.5、1.6）。
///
/// 手順は 4 つである:
///
/// 1. **前の保持**（同じウィンドウ）が表示していたシートの識別子を読む（履歴を引き継ぐかの
///    判定の材料である。後述）
/// 2. **文書を読む**（[`DocumentSessionsApi::read`]）。保持していなければ経路の失敗である
/// 3. 閉包の内側で**シートを識別子で引き**、ルートスキーマを計画へ落とし
///    （[`SchemaEngineApi::compile`]）、[`GridSession::open`] でセッションを作る。あわせて
///    **前のシートが差し替え後の文書に在るか**を写す（履歴の引き継ぎの判定）
/// 4. **履歴を決めて**表へ置く（破棄の購読が登録できなければ**置かずに**失敗を返す）
///
/// **同じウィンドウで 2 度呼ぶと前の保持を置き換える**（別のシートへ切り替える経路である）。
/// 置き換えでも購読は増えない（登録済みの集合が 1 回に抑える）。
///
/// # 履歴は置き換えを越えて引き継ぐ（要件 9.5）
///
/// 履歴は**ドキュメント単位**であり（要件 9.5）、[`GridSession`] はシートごとに作り直される
/// （計画をそのシートの宣言から落とすため）。したがって履歴の所有者は本保持であり、
/// 置き換えのときに**持ち出して**次の保持へ渡す（[`SheetEntry`] の docs）。
///
/// | 状態 | 履歴 | 判定 |
/// |---|---|---|
/// | 同じ文書の別のシートへ切り替えた | **引き継ぐ** | 前の保持のシートが差し替え後の文書にも在る |
/// | 文書が差し替わった（新規・開く・破棄） | **捨てる**（空の履歴を作る） | 前の保持のシートが差し替え後の文書に無い |
///
/// 判定を「保持しているシートが文書に無い」に揃えてあるのは、[`answer_find_violation`] と
/// 同じ 1 つの照合で「表示しているシートがもう無い」を表すためである（シートの識別子は
/// 発行のたびに変わるため、新しい文書が同じ識別子を持つことは無い）。**引き継いだ履歴を
/// 差し替え後の文書へ適用する経路は残らない** — 3 つの経路（`apply` / `undo` / `redo`）は
/// どれも保持しているシートを [`GridSession`] の側で照合する。
pub(crate) fn answer_open(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridOpenRequest,
) -> IpcResult<GridOpenResponse, IpcError> {
    let command = command_names::GRID_OPEN_SHEET;
    // 前の保持（同じウィンドウ）と、それが表示していたシートの識別子。**保持のロックは
    // 文書のロックより先に取り、据え付け（`store`）まで握る**（他の 5 つのコマンドと同じ
    // 順序に保つ — 順序を混ぜると、同じウィンドウの並行したコマンドが互いを待ち合う）。
    //
    // **握り続ける理由**: 履歴の持ち出しと据え付けの間に同じウィンドウの `grid_open_sheet`
    // が割り込むと、その経路も「前の保持」から履歴を持ち出し、**先に据え付けた側の履歴が
    // 表から消える**（持ち出しの跡は空の履歴で埋めるためである。要件 9.5 の履歴が失われる）。
    // ロックを握れば、2 つ目は表の項目が入れ替わるまで待ち、**引き継いだ履歴の側**
    // （いま表にある保持）から持ち出す。
    //
    // この 2 つ目の経路は**同じウィンドウの `grid_open_sheet` が並行した場合**にだけ現れる。
    // 画面は開く要求を 1 つずつ待って送るため（`./GridClient` の `openSheet` を待ってから
    // 表示の指定を送る）、いまは起きない — 起きない前提は design.md「履歴の持ち出し」に
    // 記録してある（前提が破れたときに失われうるのは、この 1 点だけである）。
    let previous = grids.entry(label);
    let mut held = previous.as_ref().map(|entry| lock(entry));
    let previous_sheet = held.as_ref().map(|entry| entry.sheet.clone());
    let opened = documents.read(label, &mut |document| {
        let Some(sheet) = document
            .sheets()
            .iter()
            .find(|sheet| sheet.id().to_string() == request.sheet)
        else {
            return Err(unknown_sheet(&request.sheet));
        };
        // **履歴を引き継ぐか**（差し替え後の文書に前の保持のシートが在るか）。文書の内側で
        // 決めるのは、判定が「差し替え後の文書の中身」に依るためである。
        let carry_history = previous_sheet.as_deref().is_some_and(|previous| {
            document
                .sheets()
                .iter()
                .any(|sheet| sheet.id().to_string() == previous)
        });
        let schema = SchemaEngine::new()
            .compile(sheet, &TypeRegistry::new())
            .map_err(|error| format!("シート {} の宣言を解釈できない: {error}", request.sheet))?;
        let session = GridSession::open(sheet.id(), schema.clone())
            .map_err(|error| format!("シート {} のグリッドを開けない: {error}", request.sheet))?;
        // **文書のシートの名の表**（タスク 10.3）。参照の列が運ぶのは名であり、宣言は
        // 識別子しか持たないため、名への写しは文書を読むこの 1 箇所で組む。
        let names: SheetNames = document
            .sheets()
            .iter()
            .map(|sheet| (sheet.id().to_string(), sheet.name().to_owned()))
            .collect();
        let summary = sheet_summary(&session, sheet.rows().len(), &names);
        Ok((session, schema, carry_history, summary))
    });

    match opened {
        Ok(Ok((session, schema, carry_history, sheet))) => {
            // **履歴を持ち出す**（引き継ぐときだけ）。前の保持はこの直後に表から外れるため、
            // 持ち出した跡は空の履歴で埋める（取り出しと後始末を 1 つの操作にする）。
            // **前の保持のロックはここでも握っている**（この関数の冒頭で取り、`store` まで
            // 離さない — 持ち出しと据え付けの間に割り込む経路を作らない）。
            let history = match (held.as_mut(), carry_history) {
                (Some(previous), true) => {
                    core::mem::replace(&mut previous.history, UndoStack::new(DEFAULT_UNDO_LIMIT))
                }
                _ => UndoStack::new(DEFAULT_UNDO_LIMIT),
            };
            let entry = SheetEntry {
                session,
                history,
                schema,
                sheet: request.sheet.clone(),
            };
            // **開いた時点の世代**（`Generation::FIRST`。`GridSession::open` は進めない）。表へ
            // 置く前に読む — `store` は保持を受け取る（`entry` は move される）。
            let generation = generation_text(entry.session.generation());
            if !grids.store(label, entry) {
                return IpcResult::Err {
                    error: path_failure(command, label, "破棄の購読を登録できない"),
                };
            }
            IpcResult::Ok {
                data: GridOpenResponse {
                    context: WindowContext {
                        window: label.clone(),
                    },
                    // 以後の値は `grid_set_view` の応答が運ぶ。
                    generation,
                    sheet,
                },
            }
        }
        Ok(Err(reason)) => IpcResult::Err {
            error: path_failure(command, label, &reason),
        },
        Err(error) => IpcResult::Err {
            error: session_failure(command, label, &error),
        },
    }
}

/// 表示の指定を変える本体（[`grid_set_view`] の中身。要件 8.3、8.4、8.7）。
///
/// **要求は完全な記述である。** 前の指定のうち今回の要求に現れない展開は折りたたみへ戻す —
/// 戻さないと、要求から消えた列が展開されたまま残り、「空の指定は展開無し」という 6.1 の
/// 規約が破れる（前の指定が黙って生き続ける）。**文書は読むだけである**（要件 8.5）。
pub(crate) fn answer_set_view(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridViewRequest,
) -> IpcResult<GridViewResponse, IpcError> {
    let command = command_names::GRID_SET_VIEW;
    let context = WindowContext {
        window: label.clone(),
    };
    let Some(entry) = grids.entry(label) else {
        return IpcResult::Err {
            error: not_open(command, label),
        };
    };
    let mut entry = lock(&entry);

    // 1. 要求に現れない展開を折りたたみへ戻す（要求は完全な記述である）。
    let requested: HashSet<u32> = request
        .view
        .expansion
        .iter()
        .map(|state| state.column)
        .collect();
    let previous: Vec<ExpansionState> = entry.session.expansion().to_vec();
    for state in previous {
        if !requested.contains(&count_to_u32(state.column.index())) {
            entry
                .session
                .set_expansion(ExpansionState::collapsed(state.column));
        }
    }

    // 2. 順序と索引（`set_view` は可視行の並びと違反の索引を導出する）。**同じ読みの内側で
    //    文書のシートの名の表も組む**（タスク 10.3。列の写しが参照の列に名を載せるため。
    //    読みを 2 度に分けると、その間に文書が差し替わりうる）。
    let spec = view_spec(&request.view);
    let view = documents.read(label, &mut |document| {
        entry
            .session
            .set_view(document, spec.clone())
            .map(|summary| {
                let names: SheetNames = document
                    .sheets()
                    .iter()
                    .map(|sheet| (sheet.id().to_string(), sheet.name().to_owned()))
                    .collect();
                (summary, names)
            })
    });
    let (summary, names) = match view {
        Ok(Ok(answer)) => answer,
        Ok(Err(error)) => {
            return IpcResult::Err {
                error: grid_failure(command, label, &error),
            };
        }
        Err(error) => {
            return IpcResult::Err {
                error: session_failure(command, label, &error),
            };
        }
    };

    // 3. 展開の適用（列の構成が変わる。`set_view` のあとに行う）。
    for state in &request.view.expansion {
        entry.session.set_expansion(expansion_state(state));
    }

    IpcResult::Ok {
        data: GridViewResponse {
            context,
            // **展開の適用（手順 3）まで終えた時点の世代**である。手順 1 と 3 がそれぞれ進めるので、
            // 呼び出しの途中の値（手順 2 の直後など）を控えてはならない。
            generation: generation_text(entry.session.generation()),
            visible_rows: count_to_u32(summary.visible),
            hidden_rows: count_to_u32(summary.hidden),
            violation_total: count_to_u32(entry.session.violation_total()),
            // **導出後の構成を載せる**（要件 5.1、5.2、5.4）。ここが「構成が画面へ届く唯一の
            // 瞬間」である — 展開の適用（手順 3）のあとの `GridSession::columns()` を写すので、
            // 展開した列の内側の位置が並びに現れ、折りたたんだ列は元の 1 本だけに戻る。載せないと、
            // 画面は開いたときの構成を描き続け、**展開を指定しても描かれる列が変わらない**。
            columns: layout_columns(&entry.session, &names),
        },
    }
}

/// 空の窓で答える（**失敗と世代違いの表現**。生バイト経路は封筒を運べない）。
///
/// 表現そのものは `data-grid` の [`EMPTY_WINDOW`]（長さ 0 のバイト列）が唯一の源である —
/// 行 0 の窓（頭だけを持つ `HEADER_LEN` バイト）とは区別でき、画面は「端に達した」と
/// 「要求が通らなかった」を別に扱える（モジュール docs「失敗と世代違いは空の窓」）。
fn empty_window() -> Response {
    Response::new(EMPTY_WINDOW.to_vec())
}

/// 窓の要求に答える本体（[`grid_rows_window`] の中身。要件 1.1、11.2）。
///
/// # 引数は引数全体でなければならない
///
/// 生バイトで届いたときだけ窓を作る。入れ子にすると Tauri は数値の配列へ変換して JSON として
/// 送るため（[`InvokeBody::Json`]）、ここへは生バイトとして届かない — **例外を投げず**空の窓と
/// 警告で答える（`bulk` のモジュール doc「経路の性質」。呼び出し側は長さ 0 で気づく）。
///
/// # 手順
///
/// 1. **引数を復号する**（[`decode_window_argument`]）。読めなければ空の窓
/// 2. **シートを照合する** — 要求のシートが保持しているシート（[`SheetEntry::sheet`]）と
///    一致しなければ空の窓（モジュール docs「シートの照合」）
/// 3. **世代を見る** — 一致しない要求（古い・新しすぎる）は空の窓。規則は
///    [`WindowCodec::is_stale`] が唯一の源である
/// 4. **[`GridSession::encode_window`] が窓を作る**。符号化は文書を**読むだけ**であり
///    （`&Document`）、失敗（範囲外・未知の行・シートが無い）は空の窓へ写す
///
/// **どの失敗でも空の窓であり、封筒は返らない**（要件 4.4 の規則からの例外。モジュール docs）。
///
/// # 記録に何を書くか
///
/// 書くのは**呼び出し元ウィンドウのラベル・受信バイト数・成否・窓の大きさ**だけである。
/// シートの識別子もセルの値も記録へ流さない（要件 8.4。`bulk` と同じ規律）。
pub(crate) fn answer_rows_window(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    body: &InvokeBody,
) -> Response {
    let command = command_names::GRID_ROWS_WINDOW;
    let InvokeBody::Raw(bytes) = body else {
        // 入れ子の罠（モジュール docs「引数を入れ子にしない」）。生バイトではないので返せる
        // ものが無い — 長さ 0 が「要求が通らなかった」の表現である。
        log::warn!(
            "{command}: 引数が生バイトでない（呼び出し元ウィンドウ = {}）— \
             バッファは引数全体でなければならない。入れ子にすると数値の配列へ変換される",
            label.as_str()
        );
        return empty_window();
    };

    let Some(entry) = grids.entry(label) else {
        // 表示状態と違反の索引は `grid_open_sheet` が作り、履歴はその保持が持ち回る
        // （他の 5 つと同じ順序）。
        log::warn!(
            "{command}: 呼び出し元ウィンドウ = {} のグリッドがまだ開かれていない\
             （受信バイト数 = {}）",
            label.as_str(),
            bytes.len()
        );
        return empty_window();
    };

    let argument = match decode_window_argument(bytes) {
        Ok(argument) => argument,
        Err(reason) => {
            log::warn!(
                "{command}: 要求の頭を読めない（呼び出し元ウィンドウ = {}, 受信バイト数 = {}）: \
                 {reason}",
                label.as_str(),
                bytes.len()
            );
            return empty_window();
        }
    };

    let entry = lock(&entry);

    // シートの照合（モジュール docs「シートの照合」）。**表示していないシートの窓は返さない** —
    // 文書の差し替えの後はこの経路で古いシートが弾かれる。
    if argument.sheet != entry.sheet {
        log::warn!(
            "{command}: 要求のシートが表示中のシートと違う（呼び出し元ウィンドウ = {}）— \
             空の窓を返す",
            label.as_str()
        );
        return empty_window();
    }

    // 世代。**比較の規則は `data-grid` の 1 つに閉じる**（本層は `!=` を書かない）。
    // 要求が名乗る世代を見られるのは本層だけである — `encode_window` は自分でいまの世代から
    // `WindowRequest` を組む（モジュール docs「世代を比べるのは本層である」）。
    let codec = WindowCodec::new(entry.session.generation());
    if codec.is_stale(&WindowRequest::new(argument.generation, argument.span)) {
        log::debug!(
            "{command}: 要求の世代がいまの世代と一致しない（呼び出し元ウィンドウ = {}）— \
             空の窓を返す",
            label.as_str()
        );
        return empty_window();
    }

    // 窓は文書を読むだけで作れる（要件 8.5 と同じく、この経路は文書を変えない）。
    let encoded = documents.read(label, &mut |document| {
        entry.session.encode_window(document, argument.span)
    });

    match encoded {
        Ok(Ok(window)) => {
            log::info!(
                "{command}: 呼び出し元ウィンドウ = {} / 開始序数 = {} / 要求行数 = {} / \
                 列 = {} / 応答バイト数 = {}",
                label.as_str(),
                argument.span.start().get(),
                argument.span.count(),
                entry.schema.column_count(),
                window.len()
            );
            Response::new(window)
        }
        Ok(Err(error)) => {
            log::warn!(
                "{command}: 窓を作れない（呼び出し元ウィンドウ = {}）: {error} — 空の窓を返す",
                label.as_str()
            );
            empty_window()
        }
        Err(error) => {
            log::warn!(
                "{command}: 文書を読めない（呼び出し元ウィンドウ = {}）: {error} — 空の窓を返す",
                label.as_str()
            );
            empty_window()
        }
    }
}

/// 編集を適用する本体（[`grid_apply_edit`] の中身。要件 3.3、3.4、3.5、4.3、4.6）。
///
/// **命令の変換は閉包の外で済ませる。** 変換できない要求（解釈できない行の識別子、宣言の
/// 列数の外の列）で文書へ触れると、何も変えていないのに未保存の印が立つ —
/// [`DocumentSessionsApi::edit`] は閉包が失敗を返しても印を立てる（閉包が文書を変えたかを
/// 判定できないため保守側に倒す）からである。したがって境界からドメインへの写像は先に済ませ、
/// 失敗はそこで返す。**列の範囲は同じ計画（[`SheetEntry::schema`]）から取る**ため、
/// ここで弾かれる列はドメインも弾く（[`checked_column_index`] の doc）。
///
/// **閉包の内側に残る失敗は「文書の現在の内容」に依るものだけである**（保持しているシートが
/// 文書に無い・列数が食い違う ＝ [`GridError::SchemaUnusable`]、行が消えている ＝
/// [`GridError::UnknownRow`]）。要求だけで決まる失敗をここへ残さないことが本経路の規律で
/// あり、残せば未保存の印だけが立つ。
///
/// 適用そのものは `document-session` の**可変の貸出口**を通す（文書を変える唯一の経路。
/// 未保存の印と版も同じ臨界区間の内側で記録される）。
pub(crate) fn answer_apply_edit(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridEditRequest,
) -> IpcResult<GridEditResponse, IpcError> {
    let command = command_names::GRID_APPLY_EDIT;
    let context = WindowContext {
        window: label.clone(),
    };
    let Some(entry) = grids.entry(label) else {
        return IpcResult::Err {
            error: not_open(command, label),
        };
    };
    let mut entry = lock(&entry);

    let mut converted = match edit_command(&request.command, entry.schema.column_count()) {
        Ok(converted) => Some(converted),
        Err(reason) => {
            return IpcResult::Err {
                error: path_failure(command, label, &reason),
            };
        }
    };
    let applied = documents.edit(label, &mut |document| {
        // `FnMut` の閉包は持ち物を move できないため、命令は 1 度だけ取り出す。
        // **`edit` の閉包は高々 1 回しか呼ばれない**（`DocumentSessionsApi::edit` の契約）。
        let command = converted.take().expect("適用の閉包は 1 回だけ呼ばれる");
        // 履歴は**このウィンドウの保持が所有する**（要件 9.5）。呼び出しの間だけ貸す —
        // セッションと履歴を同時に可変で借りるため、保持を分解する。
        let SheetEntry {
            session, history, ..
        } = &mut *entry;
        session.apply(document, history, command)
    });

    match applied {
        Ok(edited) => match edited.value {
            Ok(outcome) => IpcResult::Ok {
                data: GridEditResponse {
                    context,
                    // **適用の後**の世代（影響を受けた行があるときだけ進む）。
                    generation: generation_text(entry.session.generation()),
                    outcome: Some(outcome_to_boundary(&entry.session, &outcome)),
                },
            },
            Err(error) => IpcResult::Err {
                error: grid_failure(command, label, &error),
            },
        },
        Err(error) => IpcResult::Err {
            error: session_failure(command, label, &error),
        },
    }
}

/// 履歴を進める本体（[`grid_history`] の中身。要件 9.2、9.3）。
///
/// **進める向きは要求が言う。** 取り消しとやり直しは同じ経路（履歴と適用を束ねた口）を通り、
/// どちらも [`EditOutcome`] を返すため、応答の形は編集の適用と同じである。
///
/// **進める履歴が無いことは失敗ではない** — `outcome` を `None` にして成功腕で答える
/// （要件 9.2、9.3。画面は「取り消せる操作が無い」ことを失敗として扱わない）。
pub(crate) fn answer_history(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridHistoryRequest,
) -> IpcResult<GridEditResponse, IpcError> {
    let command = command_names::GRID_HISTORY;
    let context = WindowContext {
        window: label.clone(),
    };
    let Some(entry) = grids.entry(label) else {
        return IpcResult::Err {
            error: not_open(command, label),
        };
    };
    let mut entry = lock(&entry);

    let direction = request.direction;
    // **表示中のシートの行数**（別のシートへ落ちた 1 歩の応答の材料である。下の
    // [`untouched_sheet_outcome`]）。文書の内側で読む — 応答を組み立てる時点では文書の借用が
    // 切れているためである。**適用の前後で変わらない**（表示中のシートは動かないため、
    // どちらの時点で読んでも同じ数である）。
    let mut displayed_rows = 0usize;
    let advanced = documents.edit(label, &mut |document| {
        // 履歴は**このウィンドウの保持が所有する**（要件 9.5。ドキュメント単位であり、
        // セッションの持ち物ではない）。呼び出しの間だけ貸す — セッションと履歴を同時に
        // 可変で借りるため、保持を分解する。
        let SheetEntry {
            session,
            history,
            sheet,
            ..
        } = &mut *entry;
        // 表示中のシートを識別子で引いて行数を読む（[`answer_find_violation`] と同じ照合で
        // ある）。引けなければ 0 — その状態は経路の失敗として他のコマンドが答える。
        displayed_rows = document
            .sheets()
            .iter()
            .find(|found| found.id().to_string() == sheet.as_str())
            .map_or(0, |found| found.rows().len());
        match direction {
            GridHistoryDirection::Undo => session.undo(document, history),
            GridHistoryDirection::Redo => session.redo(document, history),
        }
    });

    match advanced {
        Ok(edited) => match edited.value {
            Ok(Some(outcome)) => {
                // **適用先が表示中のシートか**で材料の意味が変わる（要件 9.5）。履歴は
                // ドキュメント単位であるため、切り替える前のシートの 1 歩がここへ来る —
                // そのときの応答は表示中のシートを記述する（`untouched_sheet_outcome` の docs）。
                let boundary = if outcome.sheet.to_string() == entry.sheet {
                    outcome_to_boundary(&entry.session, &outcome)
                } else {
                    untouched_sheet_outcome(&entry.session, displayed_rows)
                };
                IpcResult::Ok {
                    data: GridEditResponse {
                        context,
                        // **履歴を進めた後**の世代（影響を受けた行があるときだけ進む）。
                        generation: generation_text(entry.session.generation()),
                        outcome: Some(boundary),
                    },
                }
            }
            Ok(None) => IpcResult::Ok {
                data: GridEditResponse {
                    context,
                    // **何も適用していない**ので、世代も据え置きである（画面はこれを採用するだけ
                    // であり、据え置きかどうかを `outcome` から推し量らない）。
                    generation: generation_text(entry.session.generation()),
                    outcome: None,
                },
            },
            Err(error) => IpcResult::Err {
                error: grid_failure(command, label, &error),
            },
        },
        Err(error) => IpcResult::Err {
            error: session_failure(command, label, &error),
        },
    }
}

/// 次の違反を探す本体（[`grid_find_violation`] の中身。要件 4.2、4.4、4.5）。
///
/// 手順は 4 つである:
///
/// 1. **経路の成立を確かめる** — 保持しているシートが文書に在ること。無ければ**経路の失敗**
///    である（`violation: None` ではない。下の「3 つの状態の区別」）
/// 2. **[`GridSession::find_violation`] が位置を答える**（可視行の序数から最も近い違反セルへ。
///    索引を読むだけなので、表示範囲の外にある違反にも到達する。要件 4.4）
/// 3. **理由を組み立てる**（要件 4.2）。理由を持つのは判定だけであり、セッションは
///    違反そのものを外へ出さない。そこで**見つかった列をもう一度だけ判定**し
///    （[`SchemaEngineApi::validate_columns`]。全件検証ではない）、その列の報告から
///    見つかったセルの違反を選んで文言へ写す
/// 4. **見つからなければ `None` を返す** — 「これ以上違反が無い」は正常な結果である
///
/// # 3 つの状態の区別（6.2 のレビューが残した点。6.3 で揃えた）
///
/// | 状態 | 答え | 根拠 |
/// |---|---|---|
/// | 保持しているシートが文書に無い | 封筒の**失敗腕**（経路の失敗） | [`grid_set_view`] / [`grid_apply_edit`] / [`grid_history`] は同じ状態でセッションの `SchemaUnusable` を失敗腕へ写す |
/// | 違反が 1 件も無い | 成功腕の `violation: None` | 要件 4.4「これ以上違反が無い」＝**正常な結果** |
/// | 違反が在る | 成功腕の `violation: Some(...)` | 要件 4.2、4.4 |
///
/// **1 つ目を 2 つ目と同じ答えにしてはならない。** 文書が差し替わると（メニュー「開く…」→
/// `dialog::hand_off` → [`DocumentSessionsApi::attach`]。未保存でなければ同じウィンドウへ
/// 引き渡せる）、セッションは前のシートを表示したまま残る。その状態は「表示しているシートが
/// もう無い」であり、探す先が無い — 画面が開き直すべき失敗である。
///
/// [`GridSession`] は文書を所有しないため（`data-grid` の `api` のモジュール docs）、
/// **この照合は本層が行う**（他の 5 つは文書を受け取るドメインの操作が同じ照合を内側で
/// 通っている）。照合は 1 回の読みで済み、探索そのものは索引だけを読む（要件 4.4 の費用は
/// 変わらない）。
///
/// 手順 3 の判定は**列に閉じた 1 回**である。理由は利用者が明示的に指示したときにだけ要る
/// （要件 4.2）ため、走査の経路（窓の符号化）でこの費用を払うことはない。
pub(crate) fn answer_find_violation(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridViolationRequest,
) -> IpcResult<GridViolationResponse, IpcError> {
    let command = command_names::GRID_FIND_VIOLATION;
    let context = WindowContext {
        window: label.clone(),
    };
    let Some(entry) = grids.entry(label) else {
        return IpcResult::Err {
            error: not_open(command, label),
        };
    };
    let entry = lock(&entry);

    // 1. 経路の成立（保持しているシートが文書に在ること）。**探索より先に見る** —
    //    索引は文書に依らないため、あとから見ると「違反が無い」に紛れる。
    let present = documents.read(label, &mut |document| {
        document
            .sheets()
            .iter()
            .any(|sheet| sheet.id().to_string() == entry.sheet)
    });
    match present {
        Ok(true) => {}
        Ok(false) => {
            return IpcResult::Err {
                error: path_failure(command, label, &unknown_sheet(&entry.sheet)),
            };
        }
        Err(error) => {
            return IpcResult::Err {
                error: session_failure(command, label, &error),
            };
        }
    }

    let from = RowOrdinal::new(request.from as usize);
    let direction = match request.direction {
        GridSearchDirection::Forward => SearchDirection::Forward,
        GridSearchDirection::Backward => SearchDirection::Backward,
    };
    let Some(found) = entry.session.find_violation(from, direction) else {
        return IpcResult::Ok {
            data: GridViolationResponse {
                context,
                violation: None,
            },
        };
    };

    let column = found.column();
    let row = found.row().to_string();
    // 理由は判定だけが持つ。**見つかった列に閉じた 1 回の判定**で写す（全件検証ではない）。
    // シートを引けない場合は `Err`（**「理由が無い」ではなく経路の失敗**）として返す —
    // 手順 1 の照合と読みが分かれているため、その間に文書が差し替わりうる。
    let composed = documents.read(
        label,
        &mut |document| -> Result<Option<GridViolation>, String> {
            let sheet = document
                .sheets()
                .iter()
                .find(|sheet| sheet.id().to_string() == entry.sheet)
                .ok_or_else(|| unknown_sheet(&entry.sheet))?;
            let report = SchemaEngine::new().validate_columns(
                document,
                sheet.id(),
                &entry.schema,
                &[column],
                &ValidationOptions::default(),
            );
            Ok(report
                .violations()
                .iter()
                .find(|violation| {
                    violation.column() == column
                        && violation.row().map(|row| row.to_string()) == Some(row.clone())
                })
                .map(|violation| GridViolation {
                    location: violation_location(violation),
                    reason: describe_reason(violation.reason()),
                }))
        },
    );

    let violation = match composed {
        Ok(Ok(violation)) => violation,
        Ok(Err(reason)) => {
            return IpcResult::Err {
                error: path_failure(command, label, &reason),
            };
        }
        Err(error) => {
            return IpcResult::Err {
                error: session_failure(command, label, &error),
            };
        }
    };
    IpcResult::Ok {
        data: GridViolationResponse { context, violation },
    }
}

/// 参照先の行を頁ごとに読む本体（[`grid_reference_rows`] の中身。タスク 10.3。要件 3.8）。
///
/// # 手順（他の 6 つと同じ写像の規律である）
///
/// 1. **件数を上限へ切り詰める** — [`GRID_REFERENCE_PAGE_LIMIT`] を超える要求は上限で切る。
///    参照先が 10 万行でも**一度に全部を読まない**（要件 11 の目的。切り詰めは失敗ではない —
///    `has_more` が真になるので、画面は続きを読める）。
/// 2. **経路の成立**（保持しているシートが文書に在ること）。探索（[`answer_find_violation`]）と
///    同じ順序で、頁を組むより先に見る — 「表示しているシートがもう無い」は探す先が無いことで
///    あり、画面が開き直すべき失敗である。
/// 3. **要求の列の宣言から参照先のシートを引く**。要求が運ぶのは文書の列の添字だけであり、
///    参照先は宣言が決める（画面にシートの識別子を持ち回らせない）。**参照の型でない列**と、
///    宣言に現れない列の添字は経路の失敗である。
/// 4. **参照先のシートが文書に無い場合も経路の失敗である**（「行が無い」と混同しない。
///    6 本の写像の規律と同じ — 行が 0 件であることは正常な結果である）。
/// 5. 頁は [`reference_page`] が組む（絞り込みの規則と表示の名の規則はドメインの 1 箇所である）。
pub(crate) fn answer_reference_rows(
    documents: &Arc<DocumentSessions>,
    grids: &GridSessions,
    label: &WindowLabel,
    request: &GridReferenceRequest,
) -> IpcResult<GridReferenceResponse, IpcError> {
    let command = command_names::GRID_REFERENCE_ROWS;
    let context = WindowContext {
        window: label.clone(),
    };
    let Some(entry) = grids.entry(label) else {
        return IpcResult::Err {
            error: not_open(command, label),
        };
    };
    let entry = lock(&entry);

    // 1. 境界が上限を強制する（要求の値ではなく応答の件数に効く）。
    let count = usize::try_from(request.count.min(GRID_REFERENCE_PAGE_LIMIT)).unwrap_or_default();
    let start = usize::try_from(request.start).unwrap_or(usize::MAX);
    let column = ColumnIndex::new(usize::try_from(request.column).unwrap_or(usize::MAX));

    let page = documents.read(label, &mut |document| -> Result<ReferencePage, String> {
        // 2. 経路の成立（保持しているシートが文書に在ること）。
        if !document
            .sheets()
            .iter()
            .any(|sheet| sheet.id().to_string() == entry.sheet)
        {
            return Err(unknown_sheet(&entry.sheet));
        }

        // 3. 宣言から参照先を引く（要求はシートを知らない）。
        let Some(ColumnValidator::Ref { sheet }) = entry.schema.validator(column) else {
            return Err(format!("列 {} は参照の型ではない", request.column));
        };
        // 4. 参照先のシートが文書に無い場合は経路の失敗である。
        let target = document
            .sheets()
            .iter()
            .find(|candidate| candidate.id() == *sheet)
            .ok_or_else(|| unknown_sheet(&sheet.to_string()))?;

        // 5. 頁を組む（絞り込み・表示の名・開始位置と件数の規則は `data_grid` の 1 箇所）。
        Ok(reference_page(target, &request.search, start, count))
    });

    match page {
        Ok(Ok(page)) => IpcResult::Ok {
            data: GridReferenceResponse {
                context,
                rows: page
                    .rows
                    .iter()
                    .map(|row| GridReferenceRow {
                        id: row.id.to_string(),
                        label: row.label.clone(),
                    })
                    .collect(),
                total: count_to_u32(page.total),
                has_more: page.has_more,
            },
        },
        Ok(Err(reason)) => IpcResult::Err {
            error: path_failure(command, label, &reason),
        },
        Err(error) => IpcResult::Err {
            error: session_failure(command, label, &error),
        },
    }
}

// ---------------------------------------------------------------------------
// コマンド面（7 つ）
// ---------------------------------------------------------------------------

/// 呼び出し元ウィンドウに表示するシートを開く（要件 1.1、1.5、1.6、4.6）。
///
/// 呼び出し元は注入された [`WebviewWindow`] から取る（要件 4.6）。**`grid_open_sheet` が
/// 通らなければ他の 5 つは失敗する** — 表示状態と違反の索引はこの経路が作る。取り消し履歴も
/// ここで決まる（引き継ぐか捨てるか。要件 9.5。規則は [`answer_open`]）。
///
/// 開いたセッションは**ウィンドウごとに 1 つ**保持され、ウィンドウが閉じたら破棄される
/// （[`GridSessions`]）。**同じ文書のシートを切り替えても履歴は保たれる**（保持が持ち回る）。
#[tauri::command(async)]
pub fn grid_open_sheet(
    app: AppHandle,
    window: WebviewWindow,
    request: GridOpenRequest,
) -> IpcResult<GridOpenResponse, IpcError> {
    let command = command_names::GRID_OPEN_SHEET;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_open(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / シート = {} / 列 = {} / 行 = {}",
            context.window.as_str(),
            request.sheet,
            data.sheet.columns.len(),
            data.sheet.row_count,
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / シート = {} / 失敗 = {error}",
            context.window.as_str(),
            request.sheet,
        ),
    }
    result
}

/// 呼び出し元ウィンドウのグリッドの表示の指定を変える（要件 8.3、8.4、8.5、8.7）。
///
/// **ドキュメントは変わらない**（要件 8.5）— 文書は読むだけで、表示の指定はセッションが持つ。
#[tauri::command(async)]
pub fn grid_set_view(
    app: AppHandle,
    window: WebviewWindow,
    request: GridViewRequest,
) -> IpcResult<GridViewResponse, IpcError> {
    let command = command_names::GRID_SET_VIEW;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_set_view(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / 可視 = {} / 隠れ = {} / 違反 = {}",
            context.window.as_str(),
            data.visible_rows,
            data.hidden_rows,
            data.violation_total,
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

/// 呼び出し元ウィンドウのグリッドへ編集命令を 1 つ適用する（要件 3.3、3.4、3.5、4.6）。
///
/// 判定は `schema-engine` が行い（本モジュールは分岐を持たない）、**適合しない値も破棄せず
/// 違反として返す**（要件 3.5）。`WriteOrigin::Edit` は決して拒否しないため
/// （`schema-engine` 要件 6.1）、「編集が失敗して値が戻る」経路は存在しない。
#[tauri::command(async)]
pub fn grid_apply_edit(
    app: AppHandle,
    window: WebviewWindow,
    request: GridEditRequest,
) -> IpcResult<GridEditResponse, IpcError> {
    let command = command_names::GRID_APPLY_EDIT;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_apply_edit(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / 影響 = {} 行 / 違反 = {}",
            context.window.as_str(),
            data.outcome
                .as_ref()
                .map_or(0, |outcome| outcome.affected.len()),
            data.outcome
                .as_ref()
                .map_or(0, |outcome| outcome.violation_total),
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

/// 呼び出し元ウィンドウのグリッドの窓を、生バイトで返す（要件 1.1、11.2）。
///
/// # 引数の契約（**入れ子にしない**）
///
/// **バッファは引数全体でなければならない。** フロントエンドは
/// `invoke("grid_rows_window", buffer)` の形で呼ぶ（`src/ipc/client.ts` の `invokeRaw` が
/// これを行う）。`{ argument: buffer }` のように入れ子にすると、Tauri は `Uint8Array` を
/// `Array.from()` で数値の配列へ変換して JSON として送るため、ここへは生バイトが届かない —
/// 受け手は `InvokeBody::Json` を見て空の窓を返す（`bulk_echo` と同じ罠。`bulk` の
/// モジュール doc「経路の性質」）。
///
/// 引数のバイト配置は本モジュールの「生バイト経路の要求の頭」節が唯一の源である
/// （`WINDOW_REQUEST_HEADER_LEN` バイトの頭 ＋ シートの識別子）。**構造体や `serde` の型で
/// 包まない** — 包むと上の入れ子の罠へ落ちる。
///
/// # 応答の契約
///
/// 二進の窓（`application/octet-stream`）を [`Response`] で返す。**封筒（[`IpcResult`]）を
/// 返さない** — 生バイト経路は JSON を通せないため、失敗と世代違いは**空の窓**で表す
/// （モジュール docs「失敗と世代違いは空の窓」）。
///
/// # 実行モデル
///
/// **`#[tauri::command(async)]` を付けない。** あの印は同期の本体を別のスレッドへ移すが、
/// [`Request`] は invoke のメッセージを**借用**する型であり、非同期の本体へ持ち込めない
/// （モジュール docs「`grid_rows_window` だけが同期である理由」）。`bulk_echo`（7.2）と
/// 同じ実行モデルである。
#[tauri::command]
pub fn grid_rows_window(app: AppHandle, window: WebviewWindow, request: Request<'_>) -> Response {
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    answer_rows_window(&documents, &grids, &context.window, request.body())
}

/// 呼び出し元ウィンドウのグリッドの履歴を進める（要件 9.2、9.3、4.6）。
///
/// **進める履歴が無ければ成功腕で `outcome: None` を返す**（失敗ではない）。
#[tauri::command(async)]
pub fn grid_history(
    app: AppHandle,
    window: WebviewWindow,
    request: GridHistoryRequest,
) -> IpcResult<GridEditResponse, IpcError> {
    let command = command_names::GRID_HISTORY;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_history(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / 向き = {:?} / 変わった = {}",
            context.window.as_str(),
            request.direction,
            data.outcome.is_some(),
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

/// 呼び出し元ウィンドウのグリッドで、指定した位置から次の違反を探す（要件 4.2、4.4、4.5、4.6）。
///
/// **表示範囲の外にある違反にも到達する**（要件 4.4。索引が可視行の序数を鍵に持つ）。
/// 見つからなければ「これ以上無い」を正常な結果として返す。
#[tauri::command(async)]
pub fn grid_find_violation(
    app: AppHandle,
    window: WebviewWindow,
    request: GridViolationRequest,
) -> IpcResult<GridViolationResponse, IpcError> {
    let command = command_names::GRID_FIND_VIOLATION;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_find_violation(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / 起点 = {} / 向き = {:?} / 見つかった = {}",
            context.window.as_str(),
            request.from,
            request.direction,
            data.violation.is_some(),
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

/// 呼び出し元ウィンドウのグリッドで、参照の列が指す**参照先のシートの行**を頁ごとに読む
/// （タスク 10.3。要件 3.8、4.6）。
///
/// **参照先は要求ではなく列の宣言から決まる**（要求は文書の列の添字を運ぶ）。応答は頁に閉じ、
/// **件数は境界の上限（[`GRID_REFERENCE_PAGE_LIMIT`]）を超えない** — 参照先が 1 万行でも
/// 一度に全部を読まない。参照の型でない列、参照先のシートが文書に無い場合は経路の失敗である
/// （「行が無い」は正常な結果である）。
#[tauri::command(async)]
pub fn grid_reference_rows(
    app: AppHandle,
    window: WebviewWindow,
    request: GridReferenceRequest,
) -> IpcResult<GridReferenceResponse, IpcError> {
    let command = command_names::GRID_REFERENCE_ROWS;
    let context = caller_context(&window);
    let documents = documents_of(&app);
    let grids = grid_state(&app);

    let result = answer_reference_rows(&documents, &grids, &context.window, &request);
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / 列 = {} / 開始 = {} / 件数 = {} / 行 = {} / 総数 = {} / 続き = {}",
            context.window.as_str(),
            request.column,
            request.start,
            request.count,
            data.rows.len(),
            data.total,
            data.has_more,
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

// ---------------------------------------------------------------------------
// メニューからの引き金（7.4 の登録口を通す。タスク 8.7。要件 7.8）
// ---------------------------------------------------------------------------

/// 登録元の識別子（`AcceleratorOwner`）。**本スペックの項目はこの名前空間を使う。**
///
/// 7.4 の組み込み項目（`app-shell`）と分けるのは、**項目の識別子の重複を登録元ごとに閉じる**
/// ためである（`MenuRegistrationError::ItemIdConflict` は別の登録元の重複を拒否する）。
const OWNER: &str = "data-grid";

/// 複製の項目の識別子。**アプリ全体で一意でなければならない。**
const COPY_ITEM_ID: &str = "data-grid.copy";

/// 複製の項目の表示名。
const COPY_LABEL: &str = "複製";

/// 複製の項目のショートカット（非 macOS）。**プラットフォーム解決済みの綴りである** —
/// `CmdOrCtrl+C` は 4.6 の構文契約が受理せず（`PlatformDependentModifier`）、
/// **競合検査が組み合わせを見分けられなくなる**（`crate::menu` の module doc）。
#[cfg(not(target_os = "macos"))]
const COPY_ACCELERATOR_SPELLING: &str = "Ctrl+C";

/// 複製の項目のショートカット（macOS。メニュー上は `⌘C` と描かれる）。
#[cfg(target_os = "macos")]
const COPY_ACCELERATOR_SPELLING: &str = "Cmd+C";

/// 複製の項目を置く部分メニュー。**7.4 が決めたトップレベルの並び**（`編集`）に従う。
///
/// 位置の名前をここで書き写さず、`menu` モジュールの定数を参照する（並びと名前の食い違いを
/// 作らない。3.6 の「新規」「保存」が `ファイル` を参照するのと同じ形）。
fn copy_menu_path() -> MenuPath {
    MenuPath::new([crate::menu::EDIT_MENU_LABEL]).expect("位置は空でない")
}

/// 複製の項目の登録内容を組み立てる（登録口へ渡す値の組み立てだけを切り出す）。
///
/// `handler` を差し替えられる形にしてあるのは、**GUI 無しで登録の受理と内容を検査できる**
/// ようにするためである（[`MenuRegistry::enroll`] は画面を要しない。9.5 の診断の導線と
/// 同じ形）。
fn copy_item_spec(handler: impl Fn(&MenuSelection) + Send + Sync + 'static) -> MenuItemSpec {
    MenuItemSpec::new(OWNER, COPY_ITEM_ID, copy_menu_path(), COPY_LABEL, handler)
        .with_accelerator(COPY_ACCELERATOR_SPELLING)
}

/// 複製の要求を、**活性化の対象ウィンドウ**（7.5 の振り向け）へ通知する。
///
/// メニューの処理はイベントループのスレッドで走るため、ここでブロックしてはならない。送るのは
/// 1 つのイベント（[`GRID_COPY_REQUESTED_EVENT`]）だけで、送り先は**選ばれた時点で対象に
/// なっているウィンドウ**である（要件 3.5: ショートカットの操作は操作対象のウィンドウにのみ
/// 適用する）。対象が無い場合（どのウィンドウもフォーカスされていない）は何もしない —
/// 送り先が無いのに全ウィンドウへ配ると、触っていないウィンドウの画面が勝手に変わる。
///
/// **荷（ペイロード）を運ばない。**複製は引数を取らない — 対象は「そのとき移植口が持って
/// いる選択」である（`crates/app-shell/src/ipc/mod.rs` の定数の doc）。グリッドの画面を
/// 出していないウィンドウには購読者が居ないので、そこで選んでも何も起きない。
fn request_copy(app: &AppHandle, selection: &MenuSelection) {
    let Some(label) = selection.window().cloned() else {
        log::warn!("グリッドの複製: 対象ウィンドウが無いため画面へ送らない");
        return;
    };
    match app.emit_to(label.as_str(), GRID_COPY_REQUESTED_EVENT, ()) {
        Ok(()) => log::info!(
            "グリッドの複製の要求を送った: ウィンドウ = {}",
            label.as_str(),
        ),
        Err(error) => log::error!(
            "グリッドの複製の要求を送れなかった（ウィンドウ = {}）: {error}",
            label.as_str(),
        ),
    }
}

/// 起動時に 1 回だけ複製の項目を 7.4 の登録口へ登録する。`lifecycle::run` がメニューの構築
/// （[`crate::menu::install`]）の後に呼ぶ。
///
/// 登録は [`MenuRegistry::register`] を通すので、**ショートカットの競合（要件 3.4）と項目の
/// 識別子の重複（7.4 の `ItemIdConflict`）は登録時に検出され、登録元（このモジュール）へ
/// 報告される** — 片方を黙って無効化する経路は無い。登録に失敗しても起動は続ける
/// （メニュー項目が引けないことより、アプリが立ち上がらないことの方が悪い。診断の導線と同じ
/// 判断）。
///
/// # 貼り付けの項目を登録しない理由（**要件 7.8 の後半は未達である**）
///
/// 障碍は**クリップボードを読む経路が無いこと**である。本アプリの読み口は DOM の `paste`
/// イベントだけであり（7.2 が面で捕獲する形に決めた）、メニューの活性化には `ClipboardEvent`
/// が無い。加えて**読み口が無いまま `Ctrl+V` を項目に登録すると、基盤のメニューが打鍵を
/// 先に受け取り、いま動いている貼り付け（DOM の `paste`）が届かなくなる** — 複製はメニュー側に
/// 代わりの経路があるので安全だが、貼り付けにはそれが無い。**動いている半分を守り、動かない
/// 項目は登録しない**（詳細と実測は `design.md`「貼り付けの項目を今 登録しない理由」）。
///
/// # 取り消し・やり直しの項目（タスク 8.9。要件 9.9）
///
/// **複製と同じ形で 2 つ登録する**（`編集 > 元に戻す` / `編集 > やり直し`）。どちらも
/// **同じ 1 つのイベント**（[`GRID_HISTORY_REQUESTED_EVENT`]）を送り、**どちらの項目かは荷が
/// 運ぶ** — 画面の側の入口が 1 つになる（[`install_history`]）。
///
/// **打鍵を奪う心配は無い。**複製の `Ctrl+C` と違い、`Ctrl+Z` を扱う経路は画面の側に
/// **存在しない**（`./selection` の `selectionForKey` は空白と矢印だけを引き受け、`./renderer`
/// の打鍵の聴取は `copy` / `paste` の 2 つだけである）ので、アクセラレータが黙って殺す経路が
/// 無い。**アクセラレータが要件 9.9 の「キーボードからの指示」である**（`design.md` の
/// 「メニューの取り消し・やり直しの結線」）。
pub fn install(app: &AppHandle) {
    let registry = app.state::<MenuRegistry>();
    install_copy(app, &registry);
    install_history(app, &registry, GridHistoryDirection::Undo);
    install_history(app, &registry, GridHistoryDirection::Redo);
}

// ---------------------------------------------------------------------------
// メニューからの取り消しとやり直し（タスク 8.9。要件 9.9）
// ---------------------------------------------------------------------------

/// 取り消しの項目の識別子。**アプリ全体で一意でなければならない。**
const UNDO_ITEM_ID: &str = "data-grid.undo";

/// やり直しの項目の識別子。
const REDO_ITEM_ID: &str = "data-grid.redo";

/// 取り消しの項目の表示名。
const UNDO_LABEL: &str = "元に戻す";

/// やり直しの項目の表示名。
const REDO_LABEL: &str = "やり直し";

/// 取り消しの項目のショートカット（非 macOS）。**プラットフォーム解決済みの綴りである**
/// （複製の [`COPY_ACCELERATOR_SPELLING`] と同じ規律。`CmdOrCtrl+Z` は渡さない）。
#[cfg(not(target_os = "macos"))]
const UNDO_ACCELERATOR_SPELLING: &str = "Ctrl+Z";

/// 取り消しの項目のショートカット（macOS。メニュー上は `⌘Z` と描かれる）。
#[cfg(target_os = "macos")]
const UNDO_ACCELERATOR_SPELLING: &str = "Cmd+Z";

/// やり直しの項目のショートカット（非 macOS）。
///
/// **`Ctrl+Y` ではなく `Ctrl+Shift+Z` を採る。**このアプリの打鍵の意味論は GTK の慣習
/// （`Ctrl+Z` / `Ctrl+Shift+Z`）に揃っており、`Ctrl+Y` は Windows の一部のアプリの慣習である
/// （両方を登録しても利用者の期待は 1 つに定まらず、競合検査の対象が増えるだけである。
/// `design.md` の「メニューの取り消し・やり直しの結線」）。
#[cfg(not(target_os = "macos"))]
const REDO_ACCELERATOR_SPELLING: &str = "Ctrl+Shift+Z";

/// やり直しの項目のショートカット（macOS。メニュー上は `⇧⌘Z` と描かれる）。
#[cfg(target_os = "macos")]
const REDO_ACCELERATOR_SPELLING: &str = "Cmd+Shift+Z";

/// 履歴の項目を置く部分メニュー（複製と同じ `編集`。7.4 が決めたトップレベルの並び）。
fn history_menu_path() -> MenuPath {
    MenuPath::new([crate::menu::EDIT_MENU_LABEL]).expect("位置は空でない")
}

/// 履歴の項目（取り消し・やり直し）の**向きごとの綴り**を返す。
///
/// 2 つの項目は**識別子・表示名・ショートカットだけが違い、経路も活性化の形も同じ**である
/// （向きは荷が運ぶ）。表にしないのは、向きを 1 つ足したときに**網羅的な `match` が
/// コンパイルエラーになる**ようにするためである（`type_kind_tag` と同じ規律）。
fn history_item_parts(
    direction: GridHistoryDirection,
) -> (&'static str, &'static str, &'static str) {
    match direction {
        GridHistoryDirection::Undo => (UNDO_ITEM_ID, UNDO_LABEL, UNDO_ACCELERATOR_SPELLING),
        GridHistoryDirection::Redo => (REDO_ITEM_ID, REDO_LABEL, REDO_ACCELERATOR_SPELLING),
    }
}

/// 履歴の項目の登録内容を組み立てる（向きごとに 1 つ。複製の [`copy_item_spec`] と同じ形）。
///
/// `handler` を差し替えられる形にしてあるのは、**GUI 無しで登録の受理と内容を検査できる**
/// ようにするためである（[`MenuRegistry::enroll`] は画面を要しない）。
fn history_item_spec(
    direction: GridHistoryDirection,
    handler: impl Fn(&MenuSelection) + Send + Sync + 'static,
) -> MenuItemSpec {
    let (item, label, accelerator) = history_item_parts(direction);
    MenuItemSpec::new(OWNER, item, history_menu_path(), label, handler)
        .with_accelerator(accelerator)
}

/// 履歴の要求（向きつき）を、**活性化の対象ウィンドウ**（7.5 の振り向け）へ通知する。
///
/// 送るのは 1 つのイベント（[`GRID_HISTORY_REQUESTED_EVENT`]）であり、**どちらの項目が
/// 選ばれたかは荷が運ぶ**（9.5 の診断の導線と同じ形。複製は引数を取らないので荷を持たない）。
/// 対象が無ければ何もしない — 送り先が無いのに全ウィンドウへ配ると、触っていないウィンドウの
/// 文書が勝手に変わる（要件 3.5）。
///
/// **荷の型は境界の型をそのまま使う**（`app_shell::ipc::GridHistoryRequestedEvent` と同じ形を
/// ここで組み立てる。`serde` の綴りが生成物と一致するのは、同じ `serde` の属性から出るためで
/// ある — 向きは `#[serde(rename_all = "lowercase")]` の `"undo"` / `"redo"`）。
fn request_history(app: &AppHandle, selection: &MenuSelection, direction: GridHistoryDirection) {
    let Some(label) = selection.window().cloned() else {
        log::warn!("グリッドの履歴: 対象ウィンドウが無いため画面へ送らない");
        return;
    };
    let (item, _, _) = history_item_parts(direction);
    match app.emit_to(
        label.as_str(),
        GRID_HISTORY_REQUESTED_EVENT,
        GridHistoryRequestedEvent { direction },
    ) {
        Ok(()) => log::info!(
            "グリッドの履歴の要求を送った: ウィンドウ = {} / 項目 = {item}",
            label.as_str(),
        ),
        Err(error) => log::error!(
            "グリッドの履歴の要求を送れなかった（ウィンドウ = {}、項目 = {item}）: {error}",
            label.as_str(),
        ),
    }
}

/// 履歴の項目を 1 つ登録する（[`install`] が向きごとに 1 回呼ぶ）。
///
/// 登録に失敗しても起動は続ける（複製と同じ判断である — 項目が引けないことより、アプリが
/// 立ち上がらないことの方が悪い）。**失敗は項目ごとに記録する**ので、片方だけが登録できな
/// かった場合も記録から読める。
fn install_history(app: &AppHandle, registry: &MenuRegistry, direction: GridHistoryDirection) {
    let (item, _, _) = history_item_parts(direction);
    let handling_app = app.clone();
    let spec = history_item_spec(direction, move |selection| {
        request_history(&handling_app, selection, direction);
    });
    if let Err(error) = registry.register(app, spec) {
        log::error!("グリッドの履歴のメニュー項目を登録できなかった（{item}）: {error}");
        return;
    }
    log::info!("グリッドの履歴の導線をメニューへ登録した（{item}）");
}

/// 複製の項目を登録する（8.7。`install` から切り出した）。
fn install_copy(app: &AppHandle, registry: &MenuRegistry) {
    let handling_app = app.clone();
    let spec = copy_item_spec(move |selection| {
        request_copy(&handling_app, selection);
    });
    if let Err(error) = registry.register(app, spec) {
        log::error!("グリッドの複製のメニュー項目を登録できなかった（{COPY_ITEM_ID}）: {error}");
        return;
    }
    log::info!("グリッドの複製の導線をメニューへ登録した（{COPY_ITEM_ID}）");
}

// ---------------------------------------------------------------------------
// テスト（タスク 6.2、6.3）
//
// Tauri の実体（`WebviewWindow` / `AppHandle`）を要するのはコマンド関数の 6 つだけであり、
// 中身は**すべて本体の関数**へ切り出してある。したがってテストは本体を直接駆動する —
// GUI を起こさず、**本物の文書**（`document-format` は dev-dependency）と、破棄の購読の
// 二重だけで足りる。6 つのコマンド関数そのものの形は、関数の型を書いた 1 つのテストが
// コンパイル時に固定する。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use std::time::{Duration, Instant};

    use app_shell::ipc::{
        GridCellEdit, GridEditRequest, GridHistoryRequest, GridOpenRequest, GridSearchDirection,
        GridSortKey, GridViewRequest, GridViolationRequest,
    };
    use data_grid::{HEADER_LEN, ROW_KEY_LEN, VariantTag, WINDOW_FORMAT_VERSION, decode_window};
    use document_format::{
        CellValue, Document, DocumentFormat, DocumentFormatApi, IdFactory, NestedValue, RowId,
        SchemaPart,
    };
    use document_session::{DocumentSessions, DocumentSessionsApi, SessionState};
    use schema_engine::{
        ColumnDecl, Constraints, DeclaredKind, FieldDecl, Schema, TypeDecl, TypeKind,
        schema_to_text,
    };
    use tauri::ipc::{InvokeResponseBody, IpcResponse};

    use app_shell::accelerator::{Accelerator, MenuItemId};

    use super::*;
    use crate::menu::EDIT_MENU_LABEL;
    use crate::session::watch::DestroyHandler;
    use crate::session::watch::testing::AlwaysPresent;

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
                "jxcel-grid-commands-{tag}-{}-{unique}",
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

    /// 標本の宣言: 品番（`text`・一意・必須）と数量・単価（`int`・0〜100）。
    ///
    /// 数量と単価の範囲を狭くしてあるのは、境界の外の値（`999`）を編集で書くだけで違反を
    /// 列ごとに作れるようにするためである（要件 4.4 の探索と、**違反の総数を列に閉じた数から
    /// シート全体の数へ閉じる**ことを実物で駆動する）。
    fn declaration() -> SchemaPart {
        let schema = Schema {
            columns: vec![
                ColumnDecl {
                    name: "品番".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Text),
                        constraints: Constraints::default(),
                    },
                    required: true,
                    unique: true,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "数量".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Int),
                        constraints: Constraints {
                            min: Some(CellValue::Int(0)),
                            max: Some(CellValue::Int(100)),
                            ..Constraints::default()
                        },
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "単価".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Int),
                        constraints: Constraints {
                            min: Some(CellValue::Int(0)),
                            max: Some(CellValue::Int(100)),
                            ..Constraints::default()
                        },
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
            ],
        };
        let root = schema_to_text(&schema).expect("宣言は正準出力できる");
        SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#)).expect("宣言は解析できる")
    }

    /// 3 行（品番 = `A` / `B` / `C`、数量 = `1` / `2` / `3`、単価 = `10`〜`30`）の
    /// **本物の文書**を書く。
    fn write_document(path: &Path) {
        write_document_with(path, [1, 2, 3]);
    }

    /// 3 行の標本を書く（**数量の並びだけ**を差し替えられる）。
    ///
    /// 数量は `int` の 0〜100 であるため、`[1, 999, 3]` のように範囲外の値を置くと
    /// **違反をちょうど 1 件だけ持つ文書**になる。編集を 1 度も適用せずに違反の索引を持つ
    /// セッションを作れることが要る — 文書の引き渡し（`DocumentSessions::attach`）は
    /// **未保存でない**文書にしか効かないためである（差し替えの検査はこの形でしか作れない）。
    fn write_document_with(path: &Path, quantities: [i64; 3]) {
        let mut document = Document::new();
        let sheet = document.add_sheet("台帳");
        document
            .set_sheet_columns(
                sheet,
                vec!["品番".to_owned(), "数量".to_owned(), "単価".to_owned()],
            )
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, declaration())
            .expect("標本のシートは実在する");
        for (index, label) in ["A", "B", "C"].into_iter().enumerate() {
            let row = document.add_row(sheet).expect("標本のシートは実在する");
            document
                .set_row_values(
                    sheet,
                    row,
                    vec![
                        CellValue::Text(label.to_owned()),
                        CellValue::Int(quantities[index]),
                        CellValue::Int(index as i64 * 10 + 10),
                    ],
                )
                .expect("標本の行は実在する");
        }
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
    }

    /// **2 つのシート**を持つ標本を書く（要件 9.5 の検査の材料）。
    ///
    /// どちらのシートも [`declaration`] の 3 列を持ち、行は `A` / `B` / `C` の 3 行である。
    /// **数量の並びをシートごとに指定できる**（`[1, 2, 3]` は違反を 1 件も作らず、`999` のような
    /// 範囲外の値を置くと**そのシートだけが違反を持つ**。`write_document_with` と同じ口である）。
    /// 返るのは先頭のシート（`台帳`）と 2 番目のシート（`補助`）の**識別子の文字列**（境界を
    /// 通るのはこの文字列である）。
    ///
    /// **履歴がドキュメント単位であることを観測する唯一の材料である** — シートの切り替えは
    /// 同じウィンドウで `grid_open_sheet` を呼び直すことだから、1 つの文書に 2 つのシートが
    /// 要る（要件 9.5）。
    fn write_two_sheet_document(
        path: &Path,
        ledger_quantities: [i64; 3],
        supplement_quantities: [i64; 3],
    ) -> (String, String) {
        let mut document = Document::new();
        let ledger = document.add_sheet("台帳");
        let supplement = document.add_sheet("補助");
        for (sheet, quantities) in [
            (ledger, ledger_quantities),
            (supplement, supplement_quantities),
        ] {
            document
                .set_sheet_columns(
                    sheet,
                    vec!["品番".to_owned(), "数量".to_owned(), "単価".to_owned()],
                )
                .expect("標本のシートは実在する");
            document
                .set_root_schema(sheet, declaration())
                .expect("標本のシートは実在する");
            for (index, label) in ["A", "B", "C"].into_iter().enumerate() {
                let row = document.add_row(sheet).expect("標本のシートは実在する");
                document
                    .set_row_values(
                        sheet,
                        row,
                        vec![
                            CellValue::Text(label.to_owned()),
                            CellValue::Int(quantities[index]),
                            CellValue::Int(index as i64 * 10 + 10),
                        ],
                    )
                    .expect("標本の行は実在する");
            }
        }
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
        (ledger.to_string(), supplement.to_string())
    }

    /// 入れ子を持つ標本の宣言: 品番（`text`・一意・必須）、提供元（`object`。内側に `name` と
    /// `code`）、数量（`int`・0〜100）。
    ///
    /// **内側のフィールドを持つ列**を作る唯一の口である — 要件 5.1 の展開は、この列の内側の
    /// 位置が**列として**構成へ現れることを求める（`view` 層の `derive_layout` が
    /// `Object` の位置をフィールドへ降ろす）。
    fn nested_declaration() -> SchemaPart {
        /// 内側のフィールド 1 件（型と制約だけを指定する）。
        fn field(name: &str) -> FieldDecl {
            FieldDecl {
                name: name.into(),
                ty: TypeDecl::Kind {
                    kind: DeclaredKind::Known(TypeKind::Text),
                    constraints: Constraints::default(),
                },
                required: false,
                default: None,
                description: None,
            }
        }

        let schema = Schema {
            columns: vec![
                ColumnDecl {
                    name: "品番".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Text),
                        constraints: Constraints::default(),
                    },
                    required: true,
                    unique: true,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "提供元".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Object),
                        constraints: Constraints {
                            fields: vec![field("name"), field("code")],
                            ..Constraints::default()
                        },
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "数量".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Int),
                        constraints: Constraints {
                            min: Some(CellValue::Int(0)),
                            max: Some(CellValue::Int(100)),
                            ..Constraints::default()
                        },
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
            ],
        };
        let root = schema_to_text(&schema).expect("宣言は正準出力できる");
        SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#)).expect("宣言は解析できる")
    }

    /// 入れ子を持つ標本（3 行）を書く。提供元のセルは**オブジェクトの値**である。
    fn write_nested_document(path: &Path) {
        let mut document = Document::new();
        let sheet = document.add_sheet("台帳");
        document
            .set_sheet_columns(
                sheet,
                vec!["品番".to_owned(), "提供元".to_owned(), "数量".to_owned()],
            )
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, nested_declaration())
            .expect("標本のシートは実在する");
        for (index, label) in ["A", "B", "C"].into_iter().enumerate() {
            let row = document.add_row(sheet).expect("標本のシートは実在する");
            document
                .set_row_values(
                    sheet,
                    row,
                    vec![
                        CellValue::Text(label.to_owned()),
                        CellValue::Nested(NestedValue::Object(vec![
                            ("name".to_owned(), CellValue::Text(format!("提供元{label}"))),
                            ("code".to_owned(), CellValue::Text(format!("C-{index}"))),
                        ])),
                        CellValue::Int(index as i64 + 1),
                    ],
                )
                .expect("標本の行は実在する");
        }
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
    }

    /// **入れ子を持つ標本**を開いた状態を作る（内側の位置が構成へ現れることを観測する材料）。
    fn opened_nested(tag: &str) -> (Scratch, Arc<DocumentSessions>, GridSessions, WindowLabel) {
        let scratch = Scratch::new(tag);
        let path = scratch.file("台帳.jxcel");
        write_nested_document(&path);
        let (sessions, label) = documents(&path);
        let grids = grids();
        let opened = answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet_id(&path),
            },
        );
        assert!(matches!(opened, IpcResult::Ok { .. }), "標本は開ける");
        (scratch, sessions, grids, label)
    }

    /// 保存された標本の行の識別子を、文書の順に返す（順序に依拠しない観測の材料）。
    fn stored_rows(path: &Path) -> Vec<String> {
        let opened = DocumentFormat::new().open(path).expect("標本を読める");
        opened
            .document
            .sheets()
            .first()
            .expect("標本にはシートが 1 つある")
            .rows()
            .iter()
            .map(|row| row.id().to_string())
            .collect()
    }

    /// **10 万行**の標本を書く（要件 1.1 の規模）。
    ///
    /// 列は [`declaration`] の 3 本で足りる — 窓の費用は**窓の行数**に比例し、シートの行数には
    /// 依らない（`data-grid` の `transport` のモジュール docs「費用の形」）。品番は一意制約を
    /// 満たし（`P0`〜`P99999`）、数量と単価は 0〜100 に収める（違反を 1 件も作らない）。
    fn write_large_document(path: &Path, rows: usize) {
        let mut document = Document::new();
        let sheet = document.add_sheet("台帳");
        document
            .set_sheet_columns(
                sheet,
                vec!["品番".to_owned(), "数量".to_owned(), "単価".to_owned()],
            )
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, declaration())
            .expect("標本のシートは実在する");
        for index in 0..rows {
            let row = document.add_row(sheet).expect("標本のシートは実在する");
            let within = (index % 100) as i64;
            document
                .set_row_values(
                    sheet,
                    row,
                    vec![
                        CellValue::Text(format!("P{index}")),
                        CellValue::Int(within),
                        CellValue::Int(within),
                    ],
                )
                .expect("標本の行は実在する");
        }
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
    }

    /// **参照の列を持つ標本**（タスク 10.3 の検査の材料。要件 3.2、3.7、3.8、5.5）。
    ///
    /// 台帳（表示するシート）は 4 列である:
    ///
    /// | 列 | 型 | 宣言 |
    /// |---|---|---|
    /// | 品番 | `text` | 必須 |
    /// | 区分 | `enum`（赤・青） | 任意 |
    /// | 仕入先 | `ref`（→ 仕入先シート） | 必須 |
    /// | 提供元 | `object`（内側に `name` 必須・`code` 任意） | 任意 |
    ///
    /// 仕入先シートは `suppliers` 行（`仕入先0` … ）を持つ。返るのは台帳のシートの識別子である。
    fn write_reference_document(path: &Path, suppliers: usize) -> String {
        let mut document = Document::new();
        let ledger = document.add_sheet("台帳");
        let target = document.add_sheet("仕入先");

        let schema = Schema {
            columns: vec![
                ColumnDecl {
                    name: "品番".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Text),
                        constraints: Constraints::default(),
                    },
                    required: true,
                    unique: true,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "区分".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Enum),
                        constraints: Constraints {
                            choices: vec!["赤".into(), "青".into()],
                            ..Constraints::default()
                        },
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "仕入先".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Ref),
                        constraints: Constraints {
                            sheet: Some(target),
                            ..Constraints::default()
                        },
                    },
                    required: true,
                    unique: false,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "提供元".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(TypeKind::Object),
                        constraints: Constraints {
                            fields: vec![
                                FieldDecl {
                                    name: "name".into(),
                                    ty: TypeDecl::Kind {
                                        kind: DeclaredKind::Known(TypeKind::Text),
                                        constraints: Constraints::default(),
                                    },
                                    required: true,
                                    default: None,
                                    description: None,
                                },
                                FieldDecl {
                                    name: "code".into(),
                                    ty: TypeDecl::Kind {
                                        kind: DeclaredKind::Known(TypeKind::Text),
                                        constraints: Constraints::default(),
                                    },
                                    required: false,
                                    default: None,
                                    description: None,
                                },
                            ],
                            ..Constraints::default()
                        },
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
            ],
        };
        let root = schema_to_text(&schema).expect("宣言は正準出力できる");
        document
            .set_sheet_columns(
                ledger,
                vec![
                    "品番".to_owned(),
                    "区分".to_owned(),
                    "仕入先".to_owned(),
                    "提供元".to_owned(),
                ],
            )
            .expect("標本のシートは実在する");
        document
            .set_root_schema(
                ledger,
                SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#))
                    .expect("宣言は解析できる"),
            )
            .expect("標本のシートは実在する");

        // 参照先（仕入先シート）。**参照の値は行の識別子であり、表示の名ではない**。
        document
            .set_sheet_columns(target, vec!["名".to_owned(), "住所".to_owned()])
            .expect("標本のシートは実在する");
        for index in 0..suppliers {
            let row = document.add_row(target).expect("標本のシートは実在する");
            document
                .set_row_values(
                    target,
                    row,
                    vec![
                        CellValue::Text(format!("仕入先{index}")),
                        CellValue::Text(format!("住所{index}")),
                    ],
                )
                .expect("標本の行は実在する");
        }

        // 台帳は 1 行だけ（表示するシートの行数は参照先と無関係である）。
        let row = document.add_row(ledger).expect("標本のシートは実在する");
        document
            .set_row_values(
                ledger,
                row,
                vec![
                    CellValue::Text("A".to_owned()),
                    CellValue::Text("赤".to_owned()),
                    CellValue::Null,
                    CellValue::Null,
                ],
            )
            .expect("標本の行は実在する");

        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
        ledger.to_string()
    }

    /// 参照の標本を開いた状態を作る（`opened` と同じ形で、**開く経路そのものを通す**）。
    ///
    /// 返るのは標本の置き場、文書の保持、グリッドの保持、ウィンドウのラベル、そして**台帳の
    /// シートの識別子**である（開いた応答を組み立て直さずに、列の材料を読む検査が要る）。
    fn opened_reference(
        tag: &str,
        suppliers: usize,
    ) -> (
        Scratch,
        Arc<DocumentSessions>,
        GridSessions,
        WindowLabel,
        String,
    ) {
        let scratch = Scratch::new(tag);
        let path = scratch.file("参照.jxcel");
        let ledger = write_reference_document(&path, suppliers);
        let (sessions, label) = documents(&path);
        let grids = grids();
        let opened = answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: ledger.clone(),
            },
        );
        assert!(matches!(opened, IpcResult::Ok { .. }), "標本は開ける");
        (scratch, sessions, grids, label, ledger)
    }

    /// 参照の標本の**参照先のシート**の行の識別子（文書の側の文字列表現）。
    ///
    /// 頁が運ぶ識別子が**行のものである**こと（表示の名ではない）を固定するために使う。
    fn supplier_row_ids(path: &Path) -> Vec<String> {
        let opened = DocumentFormat::new().open(path).expect("標本を読める");
        opened.document.sheets()[1]
            .rows()
            .iter()
            .map(|row| row.id().to_string())
            .collect()
    }

    /// **参照先のシートが文書に無い**標本を書く（要件 3.8 の経路の失敗の材料）。
    ///
    /// 宣言は存在しないシートの識別子を指す。**宣言そのものは妥当である**（参照先の実在は
    /// 判定しない — `schema-engine` の `ColumnValidator::Ref` の doc）。返るのは台帳の
    /// シートの識別子である。
    fn write_dangling_reference_document(path: &Path) -> String {
        let missing = IdFactory::new().new_sheet_id();
        let mut document = Document::new();
        let ledger = document.add_sheet("台帳");
        let schema = Schema {
            columns: vec![ColumnDecl {
                name: "仕入先".into(),
                ty: TypeDecl::Kind {
                    kind: DeclaredKind::Known(TypeKind::Ref),
                    constraints: Constraints {
                        sheet: Some(missing),
                        ..Constraints::default()
                    },
                },
                required: false,
                unique: false,
                default: None,
                description: None,
            }],
        };
        let root = schema_to_text(&schema).expect("宣言は正準出力できる");
        document
            .set_sheet_columns(ledger, vec!["仕入先".to_owned()])
            .expect("標本のシートは実在する");
        document
            .set_root_schema(
                ledger,
                SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#))
                    .expect("宣言は解析できる"),
            )
            .expect("標本のシートは実在する");
        let row = document.add_row(ledger).expect("標本のシートは実在する");
        document
            .set_row_values(ledger, row, vec![CellValue::Null])
            .expect("標本の行は実在する");
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
        ledger.to_string()
    }

    /// 開いた応答から、名前で列を引く（列の添字を検査に書き写さない）。
    fn column_named(summary: &GridSheetSummary, name: &str) -> ColumnDescriptor {
        summary
            .columns
            .iter()
            .find(|column| column.name == name)
            .unwrap_or_else(|| panic!("列 {name} が構成に無い"))
            .clone()
    }

    /// 保持している文書の、指定した位置の行の識別子を返す（10 万行を写さないための入口）。
    fn stored_rows_at(
        sessions: &Arc<DocumentSessions>,
        label: &WindowLabel,
        positions: &[usize],
    ) -> Vec<String> {
        sessions
            .read(label, &mut |document| {
                let sheet = document
                    .sheets()
                    .first()
                    .expect("標本にはシートが 1 つある");
                positions
                    .iter()
                    .map(|position| sheet.rows()[*position].id().to_string())
                    .collect()
            })
            .expect("保持している文書を読める")
    }

    /// 保持している文書の**先頭のシート**の、行識別子 `row` の列 `column` の値。
    ///
    /// シートの切り替えはグリッドの保持（`SheetEntry`）を動かすが、**文書は動かない** —
    /// 検査は「取り消しが文書のどの値を戻したか」をここで確かめる（表示ではなく文書を見る）。
    fn stored_value(
        sessions: &Arc<DocumentSessions>,
        label: &WindowLabel,
        row: &str,
        column: usize,
    ) -> CellValue {
        sessions
            .read(label, &mut |document| {
                let sheet = document.sheets().first().expect("標本にはシートがある");
                let found = sheet
                    .rows()
                    .iter()
                    .find(|found| found.id().to_string() == row)
                    .expect("行は標本の文書にある");
                found.values()[column].clone()
            })
            .expect("保持している文書を読める")
    }

    /// 保持している文書の**先頭のシート**の、指定した行たちの列 `column` の値を並びで返す
    /// （[`stored_value`] の複数行版）。
    ///
    /// **適用の前後で姿を控えて比べる**ために使う — 同じ時点の値を 2 度読んで比べると、
    /// 比べる相手がどちらも「いまの値」になり、何も検出しない。
    fn values_in_column(
        sessions: &Arc<DocumentSessions>,
        label: &WindowLabel,
        rows: &[String],
        column: usize,
    ) -> Vec<CellValue> {
        rows.iter()
            .map(|row| stored_value(sessions, label, row, column))
            .collect()
    }

    /// グリッドの表（破棄の購読は常に成功し、どのラベルも引ける二重）。
    fn grids() -> GridSessions {
        GridSessions::new(Arc::new(AlwaysPresent))
    }

    /// 文書を 1 つ保持した表（文書の側）と、そのウィンドウのラベル。
    fn documents(path: &Path) -> (Arc<DocumentSessions>, WindowLabel) {
        let sessions = Arc::new(DocumentSessions::new());
        let label = WindowLabel::new("doc-1");
        sessions
            .resolve(&label, Some(path))
            .expect("標本を読み込める");
        (sessions, label)
    }

    /// 標本を開いた状態（表 + 文書 + ラベル）を作る。**開く経路そのものを通す。**
    fn opened(tag: &str) -> (Scratch, Arc<DocumentSessions>, GridSessions, WindowLabel) {
        opened_with(tag, [1, 2, 3])
    }

    /// 標本を開いた状態を作る。数量の並びを指定できる（違反を持つ文書を作る唯一の口）。
    fn opened_with(
        tag: &str,
        quantities: [i64; 3],
    ) -> (Scratch, Arc<DocumentSessions>, GridSessions, WindowLabel) {
        let scratch = Scratch::new(tag);
        let path = scratch.file("台帳.jxcel");
        write_document_with(&path, quantities);
        let (sessions, label) = documents(&path);
        let grids = grids();
        let opened = answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet_id(&path),
            },
        );
        assert!(matches!(opened, IpcResult::Ok { .. }), "標本は開ける");
        (scratch, sessions, grids, label)
    }

    /// 標本のシートの識別子（文書の側の文字列表現。境界を通るのはこの文字列である）。
    fn sheet_id(path: &Path) -> String {
        let opened = DocumentFormat::new().open(path).expect("標本を読める");
        opened
            .document
            .sheets()
            .first()
            .expect("標本にはシートが 1 つある")
            .id()
            .to_string()
    }

    /// 成功の腕からデータを取り出す（期待が外れたときに何を返したかを示す）。
    fn data<T>(result: IpcResult<T, IpcError>) -> T {
        match result {
            IpcResult::Ok { data } => data,
            IpcResult::Err { error } => panic!("成功を期待したが失敗した: {error:?}"),
        }
    }

    /// 失敗の腕から原因を取り出す。
    fn error<T>(result: IpcResult<T, IpcError>) -> IpcError {
        match result {
            IpcResult::Ok { .. } => panic!("失敗を期待したが成功した"),
            IpcResult::Err { error } => error,
        }
    }

    /// 1 セルへ打たれた文字を書く命令を組み立てる。
    fn set_one(row: &str, column: u32, text: &str) -> GridEditCommand {
        GridEditCommand::SetCells {
            cells: vec![GridCellEdit {
                cell: GridCellAddress {
                    row: row.to_owned(),
                    column,
                },
                text: text.to_owned(),
            }],
        }
    }

    /// 空の表示の指定（絞り込み無し・並べ替え無し・展開無し）。
    fn empty_view() -> GridViewSpec {
        GridViewSpec::default()
    }

    // -----------------------------------------------------------------------
    // 生バイト経路の材料（要件 1.1、11.2）
    // -----------------------------------------------------------------------

    /// 生バイト経路の引数を組み立てる。
    ///
    /// **本モジュールが定めた配置の写しである**（モジュール docs「引数の配置」）。フロント
    /// エンド側の符号化器は 7.3 の持ち物であり、ここにあるのは検査の材料である。
    fn window_argument(sheet: &str, generation: u64, start: u64, count: u64) -> Vec<u8> {
        let mut argument = Vec::with_capacity(WINDOW_REQUEST_HEADER_LEN + sheet.len());
        argument.push(WINDOW_REQUEST_VERSION);
        argument.extend_from_slice(&generation.to_le_bytes());
        argument.extend_from_slice(&start.to_le_bytes());
        argument.extend_from_slice(&count.to_le_bytes());
        argument.extend_from_slice(&(sheet.len() as u64).to_le_bytes());
        argument.extend_from_slice(sheet.as_bytes());
        argument
    }

    /// 応答から生バイトを取り出す（**封筒ではないこと**を値でも確かめる）。
    fn window_bytes(response: Response) -> Vec<u8> {
        match response.body().expect("生バイトの応答は常に作れる") {
            InvokeResponseBody::Raw(bytes) => bytes,
            InvokeResponseBody::Json(text) => panic!("封筒が返った: {text}"),
        }
    }

    /// そのウィンドウのグリッドのいまの世代（要求の頭へ載せる値）。
    fn generation_of(grids: &GridSessions, label: &WindowLabel) -> u64 {
        let entry = grids.entry(label).expect("グリッドは開いている");
        let entry = lock(&entry);
        entry.session.generation().get()
    }

    /// 行の識別子の文字列（境界の表現）を、窓の鍵（生 16 バイト）へ写す。
    fn row_key(text: &str) -> [u8; ROW_KEY_LEN] {
        RowId::from_str(text)
            .expect("標本の行の識別子は解釈できる")
            .ulid()
            .to_bytes()
    }

    // -----------------------------------------------------------------------
    // 6 つのコマンド関数の形（コンパイル時の表明）
    // -----------------------------------------------------------------------

    /// **6 つのコマンド関数は、基盤が注入する 2 引数と要求 1 つを取り、応答を返す。**
    ///
    /// これは実行時テストではなく**コンパイル時の表明**である — 6 つを実体
    /// （`WebviewWindow` / `AppHandle`）つきで呼ぶには本物のウィンドウ基盤が要り、単体テスト
    /// では起こせない（`tauri` のモック基盤は `MockRuntime` のアプリしか作れず、コマンドの
    /// 引数は `Wry` に固定されている）。ここで関数の型を書くことで、**呼び出し元ウィンドウを
    /// 引数で受け取る形（要件 4.6）と応答の型（要件 4.4）が変わればコンパイルが壊れる**。
    /// 中身の呼び出し可能性は下の各テストが本体を通して示す。
    ///
    /// [`grid_rows_window`] の行がこのテストの要である: **引数は [`Request`] 1 つ**（生バイトの
    /// バッファそのもの ＝ 入れ子にできない）であり、**戻り値は [`Response`]**（封筒つきの
    /// `IpcResult` ではない）である。`Request` を構造体や `serde` の型で包めば、その型が
    /// 引数の数として現れてこの表明が壊れる。
    #[test]
    fn the_command_wrappers_have_the_injected_window_shape() {
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridOpenRequest,
        ) -> IpcResult<GridOpenResponse, IpcError> = grid_open_sheet;
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridViewRequest,
        ) -> IpcResult<GridViewResponse, IpcError> = grid_set_view;
        let _: fn(AppHandle, WebviewWindow, Request<'_>) -> Response = grid_rows_window;
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridEditRequest,
        ) -> IpcResult<GridEditResponse, IpcError> = grid_apply_edit;
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridHistoryRequest,
        ) -> IpcResult<GridEditResponse, IpcError> = grid_history;
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridViolationRequest,
        ) -> IpcResult<GridViolationResponse, IpcError> = grid_find_violation;
        let _: fn(
            AppHandle,
            WebviewWindow,
            GridReferenceRequest,
        ) -> IpcResult<GridReferenceResponse, IpcError> = grid_reference_rows;
    }

    // -----------------------------------------------------------------------
    // シートを開く（要件 1.1、1.5、1.6）
    // -----------------------------------------------------------------------

    /// **開くと、列の構成とシートの行数が返る**（要件 1.1、1.5）。
    #[test]
    fn opening_a_sheet_answers_the_columns_and_the_row_count() {
        let (_scratch, sessions, grids, label) = opened("open");
        let path = _scratch.file("台帳.jxcel");
        let opened = data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet_id(&path),
            },
        ));

        assert_eq!(label, opened.context.window, "呼び出し元の文脈が返る");
        let names: Vec<&str> = opened
            .sheet
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect();
        assert_eq!(vec!["品番", "数量", "単価"], names, "宣言の順に列が並ぶ");
        assert_eq!(3, opened.sheet.row_count, "シートの行数が返る");
        assert!(!opened.sheet.has_no_columns(), "列は宣言されている");
        assert!(
            !opened.sheet.has_columns_but_no_rows(),
            "行も 3 件ある（要件 1.5 の空の状態ではない）"
        );
        assert_eq!(
            Some(TypeKindTag::Text),
            opened.sheet.columns[0].kind,
            "葉の型の札が境界へ出る"
        );
    }

    /// **文書に無いシートと、ドキュメントを保持していないウィンドウは、経路の失敗になる。**
    ///
    /// どちらも封筒の失敗腕である（ドメインの判定結果ではない。`design.md`「Error Handling」）。
    #[test]
    fn an_unknown_sheet_and_a_window_without_a_document_fail_at_the_path() {
        let scratch = Scratch::new("open-failure");
        let path = scratch.file("台帳.jxcel");
        write_document(&path);
        let (sessions, label) = documents(&path);
        let grids = grids();

        let unknown = error(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: "存在しないシート".to_owned(),
            },
        ));
        match unknown {
            IpcError::Document { message } => {
                assert!(
                    message.contains("存在しないシート"),
                    "原因が名指しされる: {message}"
                );
            }
            other => panic!("経路の失敗を期待した: {other:?}"),
        }

        let absent = error(answer_open(
            &sessions,
            &grids,
            &WindowLabel::new("empty-1"),
            &GridOpenRequest {
                sheet: sheet_id(&path),
            },
        ));
        match absent {
            IpcError::Document { message } => {
                assert!(
                    message.contains("保持していない"),
                    "保持していないことが伝わる: {message}"
                );
            }
            other => panic!("経路の失敗を期待した: {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // 表示の指定を変える（要件 8.3、8.4、8.7）
    // -----------------------------------------------------------------------

    /// **絞り込みを適用すると、可視行数と隠された行数が返る**（要件 8.4、8.7）。
    #[test]
    fn setting_the_view_answers_the_visible_and_hidden_rows() {
        let (_scratch, sessions, grids, label) = opened("view");
        let request = GridViewRequest {
            view: GridViewSpec {
                sort: vec![GridSortKey {
                    column: 1,
                    descending: true,
                }],
                filters: vec![GridFilterSpec::Equals {
                    column: 0,
                    text: "B".to_owned(),
                }],
                expansion: Vec::new(),
            },
        };

        let view = data(answer_set_view(&sessions, &grids, &label, &request));
        assert_eq!(1, view.visible_rows, "品番 B の 1 行だけが可視である");
        assert_eq!(2, view.hidden_rows, "残る 2 行は隠れている");
        assert_eq!(0, view.violation_total, "違反は 1 件も無い");
        assert_eq!(label, view.context.window);

        // **要求は完全な記述である。** 空の指定へ戻すと、前の絞り込みは効かない。
        let cleared = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        assert_eq!(3, cleared.visible_rows, "絞り込みが外れる");
        assert_eq!(0, cleared.hidden_rows);
    }

    /// **表示の指定を変えると、導出後の列の構成が応答に載る**（要件 5.1、5.2。8.5 の申し送り 2）。
    ///
    /// 構成を導出するのは `grid_set_view` そのものであり、これが**画面が描く列を差し替える
    /// 唯一の源**である。載らなければ、展開を指定しても画面は開いたときの構成を描き続ける
    /// （＝展開・折りたたみが見た目に何も変えない）。
    ///
    /// **表示の位置と文書の列が離れること**まで見る — 内側の位置は親と同じ文書の列を指すので、
    /// 画面の写像（`ColumnSpace`）はこの並びから組まなければならない（要件 8.6）。
    #[test]
    fn a_view_change_answers_the_derived_column_layout() {
        let (_scratch, sessions, grids, label) = opened_nested("derived-layout");

        // 導出前: 最上位の 3 列だけであり、内側の位置は 1 つも現れない。
        let flat = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        assert_eq!(names_of(&flat.columns), ["品番", "提供元", "数量"]);
        assert!(
            flat.columns.iter().all(|column| column.path.is_empty()),
            "展開を指定していない構成に内側の位置は現れない"
        );

        // 展開（提供元 = 文書の列 1 を 1 段）: 内側のフィールドが**列として**並ぶ。
        let expanded = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest {
                view: GridViewSpec {
                    expansion: vec![GridExpansionState {
                        column: 1,
                        expanded: true,
                        depth: 1,
                    }],
                    ..GridViewSpec::default()
                },
            },
        ));
        assert_eq!(
            names_of(&expanded.columns),
            ["品番", "提供元.name", "提供元.code", "数量"],
            "展開した列の内側の位置が列として現れる"
        );
        // 内側の位置は**親と同じ文書の列**を指す（表示の位置 1・2 が文書の列 1 である — 恒等では
        // ない）。画面の写像はこの対から組まれる。
        assert_eq!(expanded.columns[0].column, 0);
        assert_eq!(expanded.columns[1].column, 1);
        assert_eq!(expanded.columns[2].column, 1);
        assert_eq!(expanded.columns[3].column, 2);
        assert_eq!(
            expanded.columns[2].path,
            vec![GridPathSegment::Field {
                name: "code".to_owned()
            }],
            "内側の位置は段の並びで運ぶ（フィールド名を潰さない）"
        );
        assert_eq!(
            expanded.columns[1].kind,
            Some(TypeKindTag::Text),
            "内側の位置は葉の型の札を持つ（入力手段がそこで選ばれる）"
        );

        // 折りたたみ: 内側の位置は消え、**元の 1 本**だけに戻る（要件 5.2）。
        let collapsed = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        assert_eq!(names_of(&collapsed.columns), ["品番", "提供元", "数量"]);
        assert!(
            collapsed
                .columns
                .iter()
                .all(|column| column.path.is_empty()),
            "折りたたむと内側の列は隠れる"
        );
    }

    /// 構成の表示名の並び（**表示の順**。検査の読み口）。
    fn names_of(columns: &[ColumnDescriptor]) -> Vec<&str> {
        columns.iter().map(|column| column.name.as_str()).collect()
    }

    /// 展開を 1 件だけ持つ表示の指定（提供元 = 文書の列 1 を 1 段）。
    fn one_expansion() -> GridViewSpec {
        GridViewSpec {
            expansion: vec![GridExpansionState {
                column: 1,
                expanded: true,
                depth: 1,
            }],
            ..GridViewSpec::default()
        }
    }

    // -----------------------------------------------------------------------
    // 世代を運ぶのは境界である（タスク 10.1。design.md「世代を進めるのは境界である」）
    // -----------------------------------------------------------------------

    /// **どの応答も、その時点の世代を 10 進の文字列で運ぶ**（タスク 10.1。要件 1.1、5.1、5.3）。
    ///
    /// **1 つのコマンドの内側で世代は複数回進む。** `answer_set_view` は要求に現れない展開の
    /// 折りたたみ（手順 1）と要求された展開の適用（手順 3）でそれぞれ 1 回進めるので、応答の
    /// 世代は「画面が成功ごとに +1 した数」では表せない。**適応層が
    /// `GridSession::generation()` を写した値だけが唯一の源である** — ここが層をまたぐ
    /// 突き合わせであり、適応層が写さない（定数を返す）変異で落ちる。
    #[test]
    fn the_generation_travels_with_the_response() {
        let scratch = Scratch::new("generation-travels");
        let path = scratch.file("台帳.jxcel");
        write_nested_document(&path);
        let (sessions, label) = documents(&path);
        let grids = grids();

        // 開く: 応答は**開いた時点**の世代（`Generation::FIRST` ＝ 0）を運ぶ。
        let opened = data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet_id(&path),
            },
        ));
        assert_eq!("0", opened.generation, "開いた直後の世代は 0 である");
        assert_eq!(
            0,
            generation_of(&grids, &label),
            "セッションの世代も 0 である（前提）"
        );

        // 展開つきの指定: 適用（手順 3）で進み、順序の導出（手順 2）でも進む。
        let expanded = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest {
                view: one_expansion(),
            },
        ));
        let through_expansion = generation_of(&grids, &label);
        assert!(
            through_expansion > 1,
            "展開の適用で世代は 1 回より多く進む（前提: {through_expansion}）"
        );
        assert_eq!(
            through_expansion.to_string(),
            expanded.generation,
            "応答の世代はその時点のセッションの世代である（画面の数え上げでは表せない）"
        );

        // 折りたたみ（手順 1 が 1 回進める経路）でも同じである。
        let collapsed = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        let after_collapse = generation_of(&grids, &label);
        assert_eq!(
            after_collapse.to_string(),
            collapsed.generation,
            "折りたたみを挟んでも一致する"
        );

        // 適用（要件 3.3）も同じ規律である。
        let rows = stored_rows(&path);
        let edited = data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&rows[0], 2, "5"),
            },
        ));
        assert_eq!(
            generation_of(&grids, &label).to_string(),
            edited.generation,
            "適用の応答もその時点の世代を運ぶ"
        );

        // 履歴（要件 9.2）も同じである。
        let undone = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ));
        assert_eq!(
            generation_of(&grids, &label).to_string(),
            undone.generation,
            "履歴の応答もその時点の世代を運ぶ"
        );
    }

    /// **展開を 1 件適用したあとの窓の要求が、空の窓を受け取らない**（タスク 10.1 の端から端。
    /// 要件 1.1、5.1。8.5 の欠陥の最小の再現）。
    ///
    /// 画面が「`grid_set_view` の成功ごとに +1」で数えると、展開の適用で 2 回進んだセッションの
    /// 世代に追いつかない。要求の頭の世代は一致しないので、Rust 側は `WindowCodec::is_stale` で
    /// **空の窓**を返し、セルは永久に読み込み中のままになる（`transport` のモジュール docs
    /// 「空の窓の表現」）。応答が運ぶ世代をそのまま要求の頭へ載せれば一致し、窓が返る。
    #[test]
    fn the_window_after_applying_an_expansion_is_not_empty() {
        let scratch = Scratch::new("generation-window");
        let path = scratch.file("台帳.jxcel");
        write_nested_document(&path);
        let (sessions, label) = documents(&path);
        let grids = grids();
        let sheet = sheet_id(&path);

        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet.clone(),
            },
        ));

        // **画面の採用の規則そのもの**: 要求の頭へ載せる世代は応答が運んだ文字列である。
        let expanded = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest {
                view: one_expansion(),
            },
        ));
        let claimed: u64 = expanded
            .generation
            .parse()
            .expect("応答の世代は 10 進の文字列である");

        let window = window_bytes(answer_rows_window(
            &sessions,
            &grids,
            &label,
            &InvokeBody::Raw(window_argument(&sheet, claimed, 0, 2)),
        ));
        assert!(
            !window.is_empty(),
            "応答の世代を名乗れば空の窓は返らない（採用しないと永久に読み込み中である）"
        );
        let decoded = decode_window(&window).expect("窓は復号できる");
        assert_eq!(
            claimed,
            decoded.generation().get(),
            "窓の頭の世代も一致する"
        );
        assert_eq!(2, decoded.row_count(), "要求した 2 行が返る");

        // 対照: **数え上げの規則**（開いた直後の 1 回だけを数えた世代）で名乗ると空の窓になる —
        // これが 8.5 の欠陥が画面に現れる形である。
        let naive = claimed
            .checked_sub(1)
            .expect("展開の適用は世代を 1 つより多く進める（前提）");
        assert!(
            window_bytes(answer_rows_window(
                &sessions,
                &grids,
                &label,
                &InvokeBody::Raw(window_argument(&sheet, naive, 0, 2)),
            ))
            .is_empty(),
            "1 つ前の世代を名乗る要求は空の窓になる（数え上げでは足りない）"
        );
    }

    /// **要求に現れない展開は折りたたみへ戻る**（要求は完全な記述である。6.1 の規約）。
    #[test]
    fn a_view_request_replaces_the_expansion_states() {
        let (_scratch, sessions, grids, label) = opened("expansion");
        let expanded = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest {
                view: GridViewSpec {
                    expansion: vec![GridExpansionState {
                        column: 0,
                        expanded: true,
                        depth: 1,
                    }],
                    ..GridViewSpec::default()
                },
            },
        ));
        assert_eq!(3, expanded.visible_rows, "展開は行を隠さない");

        let entry = grids.entry(&label).expect("開いた保持がある");
        let states = lock(&entry).session.expansion().to_vec();
        assert!(
            states.iter().any(|state| state.expanded),
            "展開が適用されている（前提の表明）"
        );

        // 展開を 1 つも含まない指定へ戻す。
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        let entry = grids.entry(&label).expect("開いた保持がある");
        let states = lock(&entry).session.expansion().to_vec();
        assert!(
            states.iter().all(|state| !state.expanded),
            "要求に現れない展開は折りたたみへ戻る"
        );
    }

    /// **開いていないウィンドウへの操作は経路の失敗になる**（要求の順序の契約）。
    #[test]
    fn a_command_before_opening_fails_at_the_path() {
        let scratch = Scratch::new("not-open");
        let path = scratch.file("台帳.jxcel");
        write_document(&path);
        let (sessions, label) = documents(&path);
        let grids = grids();

        let failure = error(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        match failure {
            IpcError::Document { message } => {
                assert!(
                    message.contains("開かれていない"),
                    "原因が伝わる: {message}"
                );
            }
            other => panic!("経路の失敗を期待した: {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // 編集の適用（要件 3.3、3.4、3.5、4.3）
    // -----------------------------------------------------------------------

    /// **編集を適用すると、影響範囲と違反の要約が返る**（要件 3.3、3.4）。
    ///
    /// 違反の総数は**シート全体の数**である（要件 4.3。再検証した列に閉じた数ではない）—
    /// 境界へ閉じるのは本モジュールの仕事である。**2 つの列に違反を置いて区別する**:
    /// 2 度目の応答が「編集した列に閉じた数（1）」ではなく「シート全体の数（2）」を返すこと
    /// を確かめる（`EditOutcome::violation_total` をそのまま載せる実装はここで落ちる）。
    #[test]
    fn applying_an_edit_answers_the_verdict_and_the_sheet_wide_violation_total() {
        let (_scratch, sessions, grids, label) = opened("apply");
        let path = _scratch.file("台帳.jxcel");
        let rows = stored_rows(&path);
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));

        let applied = data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&rows[0], 1, "999"),
            },
        ));
        let outcome = applied.outcome.expect("適用は必ず要約を返す");
        assert_eq!(vec![rows[0].clone()], outcome.affected, "影響した行が返る");
        assert_eq!(3, outcome.row_count, "行数は変わらない");
        assert_eq!(1, outcome.violation_total, "範囲外の 1 件が総数になる");
        assert_eq!(1, outcome.violations.len(), "変わった違反が載る");
        assert_eq!(1, outcome.violations[0].column, "違反した列は数量である");
        assert_eq!(
            Some(rows[0].clone()),
            outcome.violations[0].row,
            "違反した行は編集した行である"
        );
        assert!(
            outcome.violations[0].path.is_empty(),
            "セル直下の違反である"
        );
        assert_eq!(vec![1], outcome.revalidated_columns, "再検証した列が載る");

        // 別の列にも違反を 1 件作る。応答の総数は**シート全体**（2 件）であり、
        // 編集した列に閉じた数（1 件）ではない（要件 4.3）。
        let second = data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&rows[1], 2, "999"),
            },
        ));
        let second = second.outcome.expect("適用は必ず要約を返す");
        assert_eq!(2, second.violation_total, "シート全体の違反の総数である");
        assert_eq!(
            1,
            second.violations.len(),
            "変わったのは編集した列の 1 件だけである"
        );
        assert_eq!(2, second.violations[0].column, "違反した列は単価である");
    }

    /// **範囲外の列は経路の失敗であり、文書へ触れない**（未保存の印も立てない）。
    ///
    /// 境界は列の範囲を検査しない（範囲の判定はドメインが持つ。6.2 が決めたこと）ため、
    /// 列 999 の失敗は**適用の閉包の内側**で起こりうる。`DocumentSessions::edit` は閉包が
    /// 失敗しても未保存の印を立てる（閉包が文書を変えたかを判定できないため保守側に倒す）ので、
    /// そのままでは**1 つのセルも書いていないのに未保存になる** — 利用者が 1 文字も変えていない
    /// のに、閉じるときに保存を求められる。変換を閉包の外で済ませて固定する。
    #[test]
    fn a_column_outside_the_declaration_does_not_touch_the_document() {
        let (_scratch, sessions, grids, label) = opened("column-range");
        let path = _scratch.file("台帳.jxcel");
        let rows = stored_rows(&path);

        let failure = error(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&rows[0], 999, "1"),
            },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));
        assert!(
            matches!(
                sessions.state(&label),
                SessionState::Open { unsaved: false, .. }
            ),
            "範囲外の列で未保存の印を立てない"
        );
        assert_eq!(rows, stored_rows(&path), "文書の本体も変わらない");
    }

    /// **解釈できない行の識別子は経路の失敗であり、文書へ触れない**（未保存の印も立てない）。
    #[test]
    fn a_malformed_row_identifier_does_not_touch_the_document() {
        let (_scratch, sessions, grids, label) = opened("malformed");
        let failure = error(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one("これは識別子ではない", 1, "1"),
            },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));
        assert!(
            matches!(
                sessions.state(&label),
                SessionState::Open { unsaved: false, .. }
            ),
            "変換できない要求で未保存の印を立てない"
        );
    }

    // -----------------------------------------------------------------------
    // 履歴（要件 9.2、9.3）
    // -----------------------------------------------------------------------

    /// **取り消しとやり直しが要約を返し、進める履歴が無いことは失敗ではない。**
    #[test]
    fn history_undoes_and_redoes_and_reports_an_empty_history_as_success() {
        let (_scratch, sessions, grids, label) = opened("history");
        let path = _scratch.file("台帳.jxcel");
        let rows = stored_rows(&path);
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));

        // 1. 何もしていないグリッドの取り消しは「進める履歴が無い」である（要件 9.2）。
        let empty = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ));
        assert_eq!(
            None, empty.outcome,
            "進める履歴が無いことは成功腕の `None` である"
        );

        // 2. 編集 → 取り消し（要件 9.2）。
        data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&rows[1], 0, "Z"),
            },
        ));
        let undone = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ));
        let undone = undone.outcome.expect("取り消しは要約を返す");
        assert_eq!(vec![rows[1].clone()], undone.affected, "取り消した行が返る");

        // 3. やり直し（要件 9.3）。
        let redone = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Redo,
            },
        ));
        let redone = redone.outcome.expect("やり直しは要約を返す");
        assert_eq!(vec![rows[1].clone()], redone.affected, "やり直した行が返る");
    }

    /// **同じウィンドウでシートを切り替えても、履歴はドキュメント単位で保たれる**（要件 9.5）。
    ///
    /// グリッドの保持（`SheetEntry`）は `grid_open_sheet` ごとに置き換わるが、**履歴は置き換えを
    /// 越えて引き継がれる** — 引き継がないと、シートを 1 度切り替えただけで文書の取り消しが
    /// 効かなくなる（要件 9.5 は履歴をシートごとではなくドキュメントごとに保つことを求める）。
    #[test]
    fn switching_sheets_keeps_the_history_of_the_document() {
        let scratch = Scratch::new("history-sheets");
        let path = scratch.file("台帳.jxcel");
        let (ledger, supplement) = write_two_sheet_document(&path, [1, 2, 3], [1, 2, 3]);
        let (sessions, label) = documents(&path);
        let grids = grids();

        // 1. シート A（台帳）を開き、数量のセルを 1 つ編集する（履歴の 1 件目）。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: ledger.clone(),
            },
        ));
        let row = stored_rows_at(&sessions, &label, &[0])[0].clone();
        data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&row, 1, "9"),
            },
        ));
        assert_eq!(
            CellValue::Int(9),
            stored_value(&sessions, &label, &row, 1),
            "前提: 編集はシート A の値を変える"
        );

        // 2. シート B（補助）を開く（同じウィンドウ。**文書は差し替わっていない**）。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: supplement.clone(),
            },
        ));

        // 3. シート A を開き直す。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest { sheet: ledger },
        ));

        // 4. **取り消しは A の編集を戻す**（要件 9.2、9.5）。
        let undone = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ))
        .outcome
        .expect("シートを切り替えても取り消せる操作は残っている");
        assert_eq!(vec![row.clone()], undone.affected, "A で編集した行が返る");
        assert_eq!(
            CellValue::Int(1),
            stored_value(&sessions, &label, &row, 1),
            "取り消しはシート A の編集を戻す"
        );
    }

    /// **別のシートを見たまま取り消しても、表示中のシートの違反は動かない**（要件 4.3、9.5）。
    ///
    /// 履歴はドキュメント単位であるため（要件 9.5）、表示しているシートとは**別のシート**を
    /// 名乗る 1 歩を取り消す経路がある（`answer_open` が履歴を持ち出すので、シートを切り替えて
    /// も取り消しは前のシートの操作を指す）。そのときの応答は**表示中のシートを記述する** —
    /// 表示中のシートの中身は 1 つも変わっていないためである:
    ///
    /// - `violation_total` は索引が載せているシート（表示中）の数である
    /// - 位置を持つ材料（`violations` / `revalidated_columns` / `affected`）は**別のシートの
    ///   もの**であり、表示中のシートの印・行数の材料に使ってはならない（画面はこれらを
    ///   そのまま採用するため、**空で答える**）
    ///
    /// 10.2 のレビューが実測した欠陥は、この 1 つ目である — 別のシートの報告（違反 0 件）を
    /// 表示中のシートの索引へ据えると、**表示中のシートの違反が黙って消える**。
    #[test]
    fn undoing_while_looking_at_another_sheet_keeps_the_displayed_violations() {
        let scratch = Scratch::new("history-other-sheet");
        let path = scratch.file("台帳.jxcel");
        // 台帳（編集するシート）は違反 0 件、補助（表示するシート）は違反 1 件（数量 999）。
        let (ledger, supplement) = write_two_sheet_document(&path, [1, 2, 3], [999, 2, 3]);
        let (sessions, label) = documents(&path);
        let grids = grids();

        // 1. 台帳を開き、数量のセルを 1 つ編集する（履歴の 1 件目）。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: ledger.clone(),
            },
        ));
        let row = stored_rows_at(&sessions, &label, &[0])[0].clone();
        data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&row, 1, "9"),
            },
        ));
        assert_eq!(
            CellValue::Int(9),
            stored_value(&sessions, &label, &row, 1),
            "前提: 編集は台帳の値を変える"
        );

        // 2. 補助を開き、索引を組み立てる（**表示は補助のまま**にする）。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: supplement.clone(),
            },
        ));
        let opened = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        assert_eq!(1, opened.violation_total, "前提: 補助は違反を 1 件持つ");
        let found = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 0,
                direction: GridSearchDirection::Forward,
            },
        ));
        let violation = found.violation.expect("前提: その違反は探索で見つかる");

        // 3. **補助を見たまま取り消す**（履歴の 1 歩は台帳へ落ちる）。
        let undone = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ))
        .outcome
        .expect("シートを切り替えても取り消せる操作は残っている");

        // 4. 台帳の値は戻り、**表示中のシートの材料は 1 つも動かない**。
        assert_eq!(
            CellValue::Int(1),
            stored_value(&sessions, &label, &row, 1),
            "取り消しは台帳の編集を戻す"
        );
        assert_eq!(
            1, undone.violation_total,
            "違反の総数は表示中のシート（補助）の数のままである"
        );
        assert!(
            undone.violations.is_empty(),
            "別のシートの違反の位置を表示中のシートのものとして載せない"
        );
        assert!(
            undone.revalidated_columns.is_empty(),
            "再検証した列も別のシートのものである"
        );
        assert!(
            undone.affected.is_empty(),
            "影響を受けた行も別のシートのものである（画面はこれを現在位置の移動と窓の作り直しに使う）"
        );
        let after = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 0,
                direction: GridSearchDirection::Forward,
            },
        ));
        assert_eq!(
            Some(violation),
            after.violation,
            "表示中のシートの違反の探索も変わらない"
        );
    }

    /// **逆向きでも数の幽霊を作らない**（要件 4.3、9.5）。
    ///
    /// 表示中のシート（補助）は違反 0 件であり、取り消す 1 歩は違反 1 件の台帳へ落ちる。
    /// 台帳の復元は全列を再検証するため、その報告（1 件）を表示中のシートの索引へ据えれば、
    /// **総数だけが 1 になる**（探索は 1 件も返さない幽霊である）。
    #[test]
    fn undoing_another_sheet_does_not_add_its_violations_to_the_displayed_sheet() {
        let scratch = Scratch::new("history-other-sheet-ghost");
        let path = scratch.file("台帳.jxcel");
        // 台帳（編集するシート）は違反 1 件、補助（表示するシート）は違反 0 件。
        let (ledger, supplement) = write_two_sheet_document(&path, [999, 2, 3], [1, 2, 3]);
        let (sessions, label) = documents(&path);
        let grids = grids();

        // 1. 台帳を開き、違反している数量を範囲内の値へ直す（履歴の 1 件目）。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: ledger.clone(),
            },
        ));
        let row = stored_rows_at(&sessions, &label, &[0])[0].clone();
        data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&row, 1, "9"),
            },
        ));

        // 2. 補助を開き、索引を組み立てる（違反 0 件）。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: supplement.clone(),
            },
        ));
        let opened = data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        assert_eq!(
            0, opened.violation_total,
            "前提: 補助は違反を 1 件も持たない"
        );

        // 3. **補助を見たまま取り消す**（台帳の違反が戻る）。
        let undone = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ))
        .outcome
        .expect("取り消せる操作は残っている");

        assert_eq!(
            CellValue::Int(999),
            stored_value(&sessions, &label, &row, 1),
            "取り消しは台帳の違反を戻す"
        );
        assert_eq!(
            0, undone.violation_total,
            "別のシートの違反が表示中のシートの総数へ混ざらない"
        );
        assert!(undone.violations.is_empty(), "位置も別のシートのものである");
        let after = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 0,
                direction: GridSearchDirection::Forward,
            },
        ));
        assert!(
            after.violation.is_none(),
            "探索も幽霊の違反を返さない（数の幽霊を作らない）"
        );
    }

    /// **合成された履歴の 1 歩（行の補充を伴う貼り付けの逆命令）は、表示中のシートを名乗らない
    /// 部品を含むとき、失敗の封筒で拒まれ、保持している文書を 1 つも変えない**（要件 7.4、9.2。
    /// design.md「UndoStack」の限界 (2)）。
    ///
    /// 貼り付けの逆命令は「覆ったセルの値を戻す（材料が**台帳**を名乗る）」と「補充した行を
    /// 取り除く（シートを運ばず、表示中のシートを見る）」の合成である。台帳を見ていないまま
    /// 適用すると、前半が台帳へ届いてから後半が補助の行を知らずに止まり、**台帳の値だけが戻る**
    /// （10.2 の 2 度目の差し戻しが実測した欠陥）。画面はこの失敗を封筒の失敗腕で受け取り、
    /// 表は**変わっていない** — 変わっていれば、利用者には「取り消せなかったのに半分だけ
    /// 戻った」表が見える。
    #[test]
    fn undoing_a_composite_step_while_looking_at_another_sheet_changes_nothing() {
        let scratch = Scratch::new("history-other-sheet-paste");
        let path = scratch.file("台帳.jxcel");
        let (ledger, supplement) = write_two_sheet_document(&path, [1, 2, 3], [1, 2, 3]);
        let (sessions, label) = documents(&path);
        let grids = grids();

        // 1. 台帳を開き、表示を指定したうえで、**行の補充が起きる**貼り付けを適用する
        //    （履歴の 1 件目）。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: ledger.clone(),
            },
        ));
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        let rows = stored_rows_at(&sessions, &label, &[0, 1, 2]);
        data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: GridEditCommand::PasteRange {
                    anchor: GridCellAddress {
                        row: rows[0].clone(),
                        column: 1,
                    },
                    rows: rows.clone(),
                    text: "7\n8\n9\n10\n11".to_owned(),
                },
            },
        ));
        // **貼り付けの後の姿を控える**（行の集合と、行ごとの数量）。控えずに「いまの値」を
        // 2 度読んで比べると、比べる相手がどちらも同じ時点の値になり、常に一致してしまう。
        let after_edit = stored_rows_at(&sessions, &label, &[0, 1, 2, 3, 4]);
        let pasted_quantities = values_in_column(&sessions, &label, &after_edit, 1);
        assert_eq!(
            5,
            after_edit.len(),
            "前提: 貼り付けは行を補充する（要件 7.4）"
        );
        assert_eq!(
            vec![
                CellValue::Int(7),
                CellValue::Int(8),
                CellValue::Int(9),
                CellValue::Int(10),
                CellValue::Int(11)
            ],
            pasted_quantities,
            "前提: 貼り付けは台帳の数量を書く"
        );

        // 2. 補助を開く（同じウィンドウ。**表示は補助のまま**にする）。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: supplement.clone(),
            },
        ));
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));

        // 3. **補助を見たまま取り消す** — 合成の 1 歩は表示中のシートを名乗らない部品を持つ。
        let failure = answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        );

        // 4. 保持している文書は 1 つも変わっていない（**すべての値**を見る。行の識別子だけを
        //    比べても、前半が届いていれば 3 行の値が入れ替わってしまう）。
        let unchanged = stored_rows_at(&sessions, &label, &[0, 1, 2, 3, 4]);
        assert_eq!(after_edit, unchanged, "台帳の行の集合は変わらない");
        assert_eq!(
            pasted_quantities,
            values_in_column(&sessions, &label, &unchanged, 1),
            "台帳の値は 1 つも変わらない（前半の復元だけが届いてはならない）"
        );
        let failure = error(failure);
        assert!(
            matches!(failure, IpcError::Document { .. }),
            "拒みは封筒の失敗腕で返る: {failure:?}"
        );

        // 5. 2 度目も同じ失敗であり、文書はやはり変わらない。
        let again = error(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ));
        let still = stored_rows_at(&sessions, &label, &[0, 1, 2, 3, 4]);
        assert_eq!(after_edit, still, "2 度目も行の集合は変わらない");
        assert_eq!(
            pasted_quantities,
            values_in_column(&sessions, &label, &still, 1),
            "2 度目も値は変わらない"
        );
        assert!(
            matches!(again, IpcError::Document { .. }),
            "2 度目も封筒の失敗腕で返る: {again:?}"
        );
    }

    /// **文書が差し替わったら履歴を捨てる**（要件 9.5）。
    ///
    /// 判定は既存の経路と同じ「**保持しているシートが文書に無い**」であり、その状態では
    /// 古い命令を**新しい文書へ決して適用しない**（取り消しは何も変えない）。引き継ぎを
    /// 無条件にすると、前の文書の行識別子を名乗る命令が新しい文書へ届きうる。
    #[test]
    fn replacing_the_document_discards_the_history() {
        let scratch = Scratch::new("history-replaced");
        let before = scratch.file("前.jxcel");
        let after = scratch.file("後.jxcel");
        write_document_with(&before, [1, 2, 3]);
        write_document_with(&after, [4, 5, 6]);
        let (sessions, label) = documents(&before);
        let grids = grids();

        // 1. 前の文書を開き、履歴を 1 件作る（数量が `9` になる）。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet_id(&before),
            },
        ));
        let edited = stored_rows_at(&sessions, &label, &[0])[0].clone();
        data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&edited, 1, "9"),
            },
        ));
        assert_eq!(
            CellValue::Int(9),
            stored_value(&sessions, &label, &edited, 1),
            "前提: 編集は前の文書の値を変える"
        );

        // 2. 文書を差し替える（利用者の「開く…」と同じ順序: 未保存の印を落としてから引き渡す）。
        sessions.discard(&label).expect("未保存の印を落とせる");
        sessions.attach(&label, &after).expect("文書を引き渡せる");
        let fresh = stored_rows_at(&sessions, &label, &[0])[0].clone();

        // 3. **差し替えた文書へ古い命令を適用しない** — 保持しているシートがもう無いので、
        //    取り消しは経路の失敗であり（`outcome: None` ではない）、新しい文書も動かない。
        let stale = error(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ));
        assert!(
            matches!(stale, IpcError::Document { .. }),
            "古い履歴は差し替え後の文書では経路が成立しない"
        );
        assert_eq!(
            CellValue::Int(4),
            stored_value(&sessions, &label, &fresh, 1),
            "古い命令は新しい文書を変えない"
        );

        // 4. 差し替え後のシートを開くと、履歴は空である（要件 9.5 の「捨てる」）。
        data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet_id(&after),
            },
        ));
        let empty = data(answer_history(
            &sessions,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ));
        assert_eq!(None, empty.outcome, "差し替えの後に引き継ぐ履歴は無い");
    }

    // -----------------------------------------------------------------------
    // 次の違反を探す（要件 4.2、4.4、4.5）
    // -----------------------------------------------------------------------

    /// **表示範囲の外にある違反にも到達し、位置と理由が返る**（要件 4.2、4.4、4.5）。
    #[test]
    fn finding_the_next_violation_answers_the_position_and_the_reason() {
        let (_scratch, sessions, grids, label) = opened("violation");
        let path = _scratch.file("台帳.jxcel");
        let rows = stored_rows(&path);
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));

        // 前提: 最初の探索は何も見つけない（違反を 1 件も作っていない）。
        let none = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 0,
                direction: GridSearchDirection::Forward,
            },
        ));
        assert!(none.violation.is_none(), "違反が無ければ見つからない");

        // 前提: 違反を持つ行を作る（範囲外の値）。
        data(answer_apply_edit(
            &sessions,
            &grids,
            &label,
            &GridEditRequest {
                command: set_one(&rows[2], 1, "-1"),
            },
        ));

        let found = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 0,
                direction: GridSearchDirection::Forward,
            },
        ));
        let violation = found.violation.expect("違反が見つかる");
        assert_eq!(
            Some(rows[2].clone()),
            violation.location.row,
            "違反した行の識別子が返る（文字列である）"
        );
        assert_eq!(1, violation.location.column, "違反した列は数量である");
        assert!(violation.location.path.is_empty(), "セル直下の違反である");
        assert!(
            violation.reason.contains("の外の値"),
            "範囲外であることが理由として伝わる: {}",
            violation.reason
        );

        // 通り過ぎた位置からは見つからない（索引が可視行の序数を鍵に持つため）。
        let past = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 3,
                direction: GridSearchDirection::Forward,
            },
        ));
        assert!(past.violation.is_none(), "末尾より後ろには違反が無い");

        // 逆向きにも到達する。
        let backward = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 2,
                direction: GridSearchDirection::Backward,
            },
        ));
        assert!(
            backward.violation.is_some(),
            "後ろ向きでも同じ違反へ到達する"
        );
    }

    /// **保持しているシートが文書に無いときは経路の失敗である**（「違反が無い」ではない）。
    ///
    /// メニュー「開く…」→ `pick_document_file` → `dialog::hand_off` →
    /// `DocumentSessions::attach` は、同じウィンドウの文書を**差し替える**（未保存でなければ
    /// 通る）。差し替えの後も `GridSession` は前のシートを表示したままであり、そのシートは
    /// 新しい文書に無い。このとき `grid_find_violation` だけが成功腕の `violation: None` を
    /// 返すと、「これ以上違反が無い」（要件 4.4 の**正常な結果**）と「そのシートが文書に無い」
    /// （経路の失敗）が同じ答えになる — `grid_set_view` / `grid_apply_edit` は後者を失敗腕で
    /// 返している。6 つすべての写像を揃える。
    #[test]
    fn a_swapped_document_makes_finding_a_violation_fail_at_the_path() {
        let (scratch, sessions, grids, label) = opened_with("swapped", [1, 999, 3]);
        let path = scratch.file("台帳.jxcel");
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));

        // 前提: いまは違反が 1 件見つかる（2 行目の数量が範囲外である）。
        let found = data(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 0,
                direction: GridSearchDirection::Forward,
            },
        ));
        assert!(found.violation.is_some(), "索引に違反が 1 件ある");

        // 別の文書を同じウィンドウへ引き渡す（未保存でないので通る）。
        let other = scratch.file("別台帳.jxcel");
        write_document(&other);
        assert_ne!(
            sheet_id(&path),
            sheet_id(&other),
            "別の文書は別のシートを持つ（差し替えの前提）"
        );
        sessions
            .attach(&label, &other)
            .expect("未保存でない文書は引き渡せる");

        // 保持しているシートは新しい文書に無い — **経路の失敗**である。
        let failure = error(answer_find_violation(
            &sessions,
            &grids,
            &label,
            &GridViolationRequest {
                from: 0,
                direction: GridSearchDirection::Forward,
            },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));
    }

    // -----------------------------------------------------------------------
    // 列の宣言の材料と参照先の行（要件 3.2、3.7、3.8、5.5。タスク 10.3）
    // -----------------------------------------------------------------------

    /// **開いた応答が、列の宣言から導ける材料を運ぶ**（要件 3.2、3.7、3.8、5.5、10.4）。
    ///
    /// 固定するのは 4 点である: ①選択肢を持つ列が**値と名**の一覧を運ぶ ②参照の列が
    /// **参照先のシートの名**（宣言が持つ識別子ではない）を運ぶ ③値なしを許すかが**宣言どおり**
    /// （必須の列は偽）④入れ子の列が**折りたたみのままでも**内側の宣言を名と型で運ぶ。
    #[test]
    fn opening_a_sheet_answers_the_declaration_material_of_each_column() {
        let (_scratch, sessions, grids, label, sheet) = opened_reference("material", 3);
        let opened = data(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest { sheet },
        ));
        let summary = &opened.sheet;
        assert_eq!(4, summary.columns.len(), "宣言の 4 列がそのまま並ぶ");

        // ① 選択肢（要件 3.2）。値と名は同じ宣言から来る。
        let category = column_named(summary, "区分");
        assert_eq!(Some(TypeKindTag::Enum), category.kind);
        assert_eq!(
            vec![
                ("赤".to_owned(), "赤".to_owned()),
                ("青".to_owned(), "青".to_owned())
            ],
            category
                .choices
                .iter()
                .map(|choice| (choice.value.clone(), choice.label.clone()))
                .collect::<Vec<_>>(),
        );
        assert!(category.nullable, "区分は任意である（宣言どおり）");
        // 選択肢を持たない列は空である（材料が無いことは誤りではない。要件 10.4）。
        assert!(column_named(summary, "品番").choices.is_empty());

        // ② 参照先は**名**である（宣言は識別子しか持たない。名への写しは本層が行う）。
        let supplier = column_named(summary, "仕入先");
        assert_eq!(Some(TypeKindTag::Ref), supplier.kind);
        assert_eq!(
            Some("仕入先".to_owned()),
            supplier.reference_sheet,
            "参照先のシートの名が運ばれる（識別子ではない）"
        );
        assert!(!supplier.nullable, "仕入先は必須である");

        // ③ 値なしを許すか（要件 3.7）。**必須の列は偽である**（「値なしへ戻す」道を出すと、
        //    判定が違反を返す値を作れてしまう）。
        assert!(!column_named(summary, "品番").nullable);

        // ④ 入れ子の内側の宣言（要件 5.5）。**折りたたみのままでも読める**。
        let origin = column_named(summary, "提供元");
        assert_eq!(Some(TypeKindTag::Object), origin.kind);
        assert!(origin.path.is_empty(), "折りたたみでは内側の位置を持たない");
        assert_eq!(
            vec![
                ("提供元.name".to_owned(), TypeKindTag::Text, false),
                ("提供元.code".to_owned(), TypeKindTag::Text, true),
            ],
            origin
                .members
                .iter()
                .map(|member| (member.name.clone(), member.kind, member.nullable))
                .collect::<Vec<_>>(),
            "内側のフィールドが名・型・値なしを許すかとともに並ぶ（name は必須である）"
        );
        assert_eq!(
            vec![GridPathSegment::Field {
                name: "name".to_owned(),
            }],
            origin.members[0].path,
            "内側の位置はセル直下からの絶対の位置である"
        );
    }

    /// **参照先の行を頁ごとに読み、総数を返す**（要件 3.8）。
    ///
    /// 頁が運ぶのは**行の識別子**（参照の列に書かれる値そのもの）と**人が読む名**である。
    #[test]
    fn reference_rows_answer_a_page_and_the_total() {
        let (scratch, sessions, grids, label, sheet) = opened_reference("reference-page", 10);
        let ids = supplier_row_ids(&scratch.file("参照.jxcel"));
        assert_eq!(10, ids.len(), "前提が崩れた: 参照先は 10 行である");
        let column = open_and_column(&sessions, &grids, &label, sheet, "仕入先");

        let page = data(answer_reference_rows(
            &sessions,
            &grids,
            &label,
            &GridReferenceRequest {
                column: column.column,
                search: String::new(),
                start: 0,
                count: 4,
            },
        ));
        assert_eq!(label, page.context.window, "呼び出し元の文脈が返る");
        assert_eq!(10, page.total, "総数は頁の外も数える");
        assert!(page.has_more, "後ろにまだ行がある");
        assert_eq!(
            vec![
                "仕入先0 住所0",
                "仕入先1 住所1",
                "仕入先2 住所2",
                "仕入先3 住所3"
            ],
            page.rows
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            "表示の名は行の値の表示文字列である（列の順に空白で連結する）"
        );
        // **識別子は行のものである**（表示の名ではない。参照の列に書かれる値そのもの）。
        assert_eq!(
            ids[0..4].to_vec(),
            page.rows
                .iter()
                .map(|row| row.id.clone())
                .collect::<Vec<_>>(),
        );

        // 続きを読むと、前の頁の後ろから返る。
        let next = data(answer_reference_rows(
            &sessions,
            &grids,
            &label,
            &GridReferenceRequest {
                column: column.column,
                search: String::new(),
                start: 4,
                count: 4,
            },
        ));
        assert_eq!(10, next.total);
        assert_eq!(
            ids[4..8].to_vec(),
            next.rows
                .iter()
                .map(|row| row.id.clone())
                .collect::<Vec<_>>(),
        );
        assert!(next.has_more);

        // 末尾に達すると続きが無い。
        let last = data(answer_reference_rows(
            &sessions,
            &grids,
            &label,
            &GridReferenceRequest {
                column: column.column,
                search: String::new(),
                start: 8,
                count: 4,
            },
        ));
        assert_eq!(2, last.rows.len());
        assert!(!last.has_more, "末尾に達した");

        // 検索の文字で絞られ、総数も絞ったあとの数になる（要件 3.8）。
        let filtered = data(answer_reference_rows(
            &sessions,
            &grids,
            &label,
            &GridReferenceRequest {
                column: column.column,
                search: "仕入先1".to_owned(),
                start: 0,
                count: 10,
            },
        ));
        assert_eq!(1, filtered.total, "「仕入先1」を含む行は 1 件だけである");
        assert_eq!("仕入先1 住所1", filtered.rows[0].label);
        assert!(!filtered.has_more);
    }

    /// **参照先が 1 万行でも、応答は境界の上限で切られる**（要件 3.8、11 の目的）。
    ///
    /// 要求が上限を超えても、応答の件数は [`GRID_REFERENCE_PAGE_LIMIT`] を超えない。
    /// 続きは `has_more` と `start` で読める（**一度に全部を読む経路が無い**）。
    #[test]
    fn a_reference_request_beyond_the_limit_is_cut_at_the_boundary() {
        let (scratch, sessions, grids, label, sheet) = opened_reference("reference-limit", 10_000);
        let column = open_and_column(&sessions, &grids, &label, sheet, "仕入先");
        let limit = usize::try_from(GRID_REFERENCE_PAGE_LIMIT).expect("上限は usize に収まる");

        let page = data(answer_reference_rows(
            &sessions,
            &grids,
            &label,
            &GridReferenceRequest {
                column: column.column,
                search: String::new(),
                start: 0,
                // **上限をはるかに超える要求**（1 万行を一度に読もうとする）。
                count: 10_000,
            },
        ));
        assert_eq!(
            limit,
            page.rows.len(),
            "応答の件数は境界の上限で切られる（要求の値ではない）"
        );
        assert_eq!(
            10_000, page.total,
            "総数は切られない（続きがあるかを決める材料である）"
        );
        assert!(page.has_more);
        assert_eq!(
            "仕入先0 住所0", page.rows[0].label,
            "先頭の頁は最初の行から始まる"
        );
        assert_eq!(
            10_000,
            supplier_row_ids(&scratch.file("参照.jxcel")).len(),
            "前提が崩れた: 参照先は 1 万行である"
        );

        // 切られた先は `start` を進めて読める（頁の並びが重ならない）。
        let next = data(answer_reference_rows(
            &sessions,
            &grids,
            &label,
            &GridReferenceRequest {
                column: column.column,
                search: String::new(),
                start: GRID_REFERENCE_PAGE_LIMIT,
                count: GRID_REFERENCE_PAGE_LIMIT,
            },
        ));
        assert_eq!(limit, next.rows.len());
        assert!(
            next.rows
                .iter()
                .all(|row| !page.rows.iter().any(|first| first.id == row.id)),
            "2 頁目は 1 頁目と重ならない"
        );
        assert_eq!(
            format!("仕入先{} 住所{}", limit, limit),
            next.rows[0].label,
            "2 頁目は上限の位置の行から始まる"
        );
    }

    /// **参照の型でない列・宣言に無い列・参照先が文書に無い場合は経路の失敗である**
    /// （要件 3.8。「行が無い」は正常な結果であり、混同しない）。
    #[test]
    fn a_reference_request_that_cannot_be_resolved_fails_at_the_path() {
        let (scratch, sessions, state, label, sheet) = opened_reference("reference-failure", 2);
        let text = open_and_column(&sessions, &state, &label, sheet, "品番");

        // 参照の型でない列（品番）。
        let failure = error(answer_reference_rows(
            &sessions,
            &state,
            &label,
            &GridReferenceRequest {
                column: text.column,
                search: String::new(),
                start: 0,
                count: 10,
            },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));

        // 宣言に現れない列の添字。
        let failure = error(answer_reference_rows(
            &sessions,
            &state,
            &label,
            &GridReferenceRequest {
                column: 99,
                search: String::new(),
                start: 0,
                count: 10,
            },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));

        // グリッドを開いていないウィンドウ。
        let other = WindowLabel::new("no-grid");
        let failure = error(answer_reference_rows(
            &sessions,
            &state,
            &other,
            &GridReferenceRequest {
                column: 0,
                search: String::new(),
                start: 0,
                count: 10,
            },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));

        // **参照先のシートが文書に無い場合**（別の標本で、存在しないシートを指す宣言を作る）。
        let missing = scratch.file("欠落.jxcel");
        let dangling = write_dangling_reference_document(&missing);
        let (dangling_sessions, dangling_label) = documents(&missing);
        let dangling_grids = grids();
        let opened = data(answer_open(
            &dangling_sessions,
            &dangling_grids,
            &dangling_label,
            &GridOpenRequest {
                sheet: dangling.clone(),
            },
        ));
        assert_eq!(1, opened.sheet.columns.len());
        let failure = error(answer_reference_rows(
            &dangling_sessions,
            &dangling_grids,
            &dangling_label,
            &GridReferenceRequest {
                column: opened.sheet.columns[0].column,
                search: String::new(),
                start: 0,
                count: 10,
            },
        ));
        // **「行が無い」ではなく経路の失敗である**（文言がそのシートを名乗る）。
        match failure {
            IpcError::Document { message } => {
                assert!(message.contains("が文書に無い"), "実際の文言: {message}")
            }
            other => panic!("経路の失敗を期待したが {other:?} を返した"),
        }
    }

    /// 標本の台帳を開き直して、名前で列を引く（列の添字を検査に書き写さない）。
    fn open_and_column(
        sessions: &Arc<DocumentSessions>,
        grids: &GridSessions,
        label: &WindowLabel,
        sheet: String,
        name: &str,
    ) -> ColumnDescriptor {
        let opened = data(answer_open(
            sessions,
            grids,
            label,
            &GridOpenRequest { sheet },
        ));
        column_named(&opened.sheet, name)
    }
    // -----------------------------------------------------------------------
    // 窓の生バイト経路（要件 1.1、11.2。タスク 6.3）
    // -----------------------------------------------------------------------

    /// **生バイトの要求から、復号できる窓が返る**（要件 1.1、11.2）。
    ///
    /// 要求の世代は**いまの世代**である — `encode_window` は自分でいまの世代から
    /// `WindowRequest` を組むため、要求が名乗る世代を見られるのは本層だけである
    /// （モジュール docs「世代を比べるのは本層である」）。
    #[test]
    fn a_window_request_answers_a_decodable_window() {
        let (_scratch, sessions, grids, label) = opened("window");
        let path = _scratch.file("台帳.jxcel");
        let sheet = sheet_id(&path);
        let rows = stored_rows(&path);
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        let generation = generation_of(&grids, &label);

        // 可視行の 2 番目から 2 行（0 起点の序数である）。
        let argument = window_argument(&sheet, generation, 1, 2);
        let bytes = window_bytes(answer_rows_window(
            &sessions,
            &grids,
            &label,
            &InvokeBody::Raw(argument.clone()),
        ));
        let window = decode_window(&bytes).expect("窓は復号できる");
        assert_eq!(WINDOW_FORMAT_VERSION, window.version(), "窓の版が載る");
        assert_eq!(
            generation,
            window.generation().get(),
            "要求の世代がそのまま載る"
        );
        assert_eq!(1, window.start().get(), "開始序数が載る");
        assert_eq!(2, window.row_count(), "要求した行数が載る");
        assert_eq!(3, window.columns(), "窓は宣言の列数を運ぶ");
        assert_eq!(
            row_key(&rows[1]),
            window.rows()[0].key(),
            "1 行目は可視行の 2 番である（識別子は生バイトのまま運ばれる）"
        );
        assert_eq!(row_key(&rows[2]), window.rows()[1].key());
        let cells: Vec<(&str, VariantTag)> = window.rows()[0]
            .cells()
            .iter()
            .map(|cell| (cell.text(), cell.tag()))
            .collect();
        assert_eq!(
            vec![
                ("B", VariantTag::TEXT),
                ("2", VariantTag::INT),
                ("20", VariantTag::INT)
            ],
            cells,
            "表示文字列と変種の札が列順に載る"
        );
        assert!(
            !window.rows()[0].cells()[1].violated(),
            "違反のないセルである"
        );

        // 冪等: 同じ世代・同じ区間の要求は**同じバイト列**になる（design.md の Idempotency 句）。
        let again = window_bytes(answer_rows_window(
            &sessions,
            &grids,
            &label,
            &InvokeBody::Raw(argument),
        ));
        assert_eq!(bytes, again, "同じ要求は同じ窓になる");

        // **端に接する要求は行 0 の窓**（可視行数と同じ開始序数）であり、空の窓ではない —
        // 画面は「端に達した」と「要求が通らなかった」を別に扱う（5.1 の 2 つの表現）。
        let edge = window_bytes(answer_rows_window(
            &sessions,
            &grids,
            &label,
            &InvokeBody::Raw(window_argument(&sheet, generation, 3, 2)),
        ));
        assert_eq!(HEADER_LEN, edge.len(), "頭だけの窓である");
        let edge = decode_window(&edge).expect("行 0 の窓も復号できる");
        assert_eq!(0, edge.row_count(), "運ぶ行が 0 である");
        assert_eq!(3, edge.start().get(), "開始序数はそのまま載る");
    }

    /// **入れ子の引数は生バイトとして届かない** — 空の窓で答える（例外は投げない）。
    ///
    /// `{ argument: buffer }` の形で呼ぶと Tauri は `Uint8Array` を `Array.from()` で数値の
    /// 配列へ変換し、JSON として送るため、ここへは [`InvokeBody::Json`] が届く（`bulk_echo` と
    /// 同じ罠）。呼び出し側は長さ 0 で気づく（`bulk` のモジュール doc「経路の性質」）。
    ///
    /// **JSON の中身は読まない**（値の型は `tauri` が外へ出していない）ため、ここで組める
    /// 最も近い形＝「JSON の本体」そのものを差し込む。本コマンドは**変種だけを見て**生バイト
    /// でなければ空の窓を返すので、中身が何であっても答えは変わらない — 入れ子の渡し方の
    /// 細部（どの鍵で包むか）に依存しないことがこの検査の要点である。
    #[test]
    fn a_nested_argument_answers_the_empty_window() {
        let (_scratch, sessions, grids, label) = opened("nested");
        let path = _scratch.file("台帳.jxcel");
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));

        // 前提: 生バイトなら同じウィンドウで窓が返る（下の空の窓が「入れ子のため」である証拠）。
        let sheet = sheet_id(&path);
        let generation = generation_of(&grids, &label);
        assert!(
            !window_bytes(answer_rows_window(
                &sessions,
                &grids,
                &label,
                &InvokeBody::Raw(window_argument(&sheet, generation, 0, 1))
            ))
            .is_empty(),
            "生バイトの引数なら窓が返る"
        );

        let nested = InvokeBody::default();
        assert!(
            matches!(nested, InvokeBody::Json(_)),
            "既定の本体は JSON である（入れ子の引数が届く形）"
        );
        let window = window_bytes(answer_rows_window(&sessions, &grids, &label, &nested));
        assert!(window.is_empty(), "生バイトでない引数には空の窓で答える");
    }

    /// **壊れた引数は空の窓で答える**（panic しない。窓の復号と同じ規律）。
    #[test]
    fn a_malformed_argument_answers_the_empty_window() {
        let (_scratch, sessions, grids, label) = opened("broken-argument");
        let path = _scratch.file("台帳.jxcel");
        let sheet = sheet_id(&path);
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        let generation = generation_of(&grids, &label);
        let good = window_argument(&sheet, generation, 0, 1);

        let truncated = good[..WINDOW_REQUEST_HEADER_LEN - 1].to_vec();
        let mut unknown_version = good.clone();
        unknown_version[0] = WINDOW_REQUEST_VERSION + 1;
        let short_sheet = good[..good.len() - 1].to_vec();
        let mut extra_byte = good.clone();
        extra_byte.push(0);
        let mut huge_sheet = good.clone();
        huge_sheet[25..33].copy_from_slice(&u64::MAX.to_le_bytes());
        let mut bad_utf8 = good.clone();
        let last = bad_utf8.len() - 1;
        bad_utf8[last] = 0xFF;
        let mut overflowing_span = good.clone();
        overflowing_span[9..17].copy_from_slice(&u64::MAX.to_le_bytes());

        let cases: [(&str, Vec<u8>); 8] = [
            ("空", Vec::new()),
            ("頭に満たない", truncated),
            ("知らない版", unknown_version),
            ("シートが足りない", short_sheet),
            ("余分なバイト", extra_byte),
            ("シートの長さが過大", huge_sheet),
            ("シートが UTF-8 でない", bad_utf8),
            ("区間が桁あふれする", overflowing_span),
        ];
        for (what, bytes) in cases {
            let window = window_bytes(answer_rows_window(
                &sessions,
                &grids,
                &label,
                &InvokeBody::Raw(bytes),
            ));
            assert!(
                window.is_empty(),
                "{what}: 空の窓で答える（{} バイト返った）",
                window.len()
            );
        }
    }

    /// **表示していないシートと、一致しない世代は空の窓で答える**（要件 1.1、5.1 の世代の規則）。
    ///
    /// 世代の比較は**一致するかどうか**である（5.1 が「古い」を安全側に一般化した）ため、
    /// 前の世代も先の世代も同じ答えになる。
    #[test]
    fn an_unknown_sheet_and_a_mismatched_generation_answer_the_empty_window() {
        let (_scratch, sessions, grids, label) = opened("window-empty");
        let path = _scratch.file("台帳.jxcel");
        let sheet = sheet_id(&path);
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        let generation = generation_of(&grids, &label);
        let previous = generation
            .checked_sub(1)
            .expect("`set_view` は世代を進める（前の世代を作れる）");

        // 前提: 正しい要求は窓を返す（下の 2 つが「別の理由」で空になることの対照）。
        let ok = window_argument(&sheet, generation, 0, 1);
        assert!(
            !window_bytes(answer_rows_window(
                &sessions,
                &grids,
                &label,
                &InvokeBody::Raw(ok)
            ))
            .is_empty(),
            "正しい要求は窓を返す"
        );

        let other_sheet = window_argument("別のシート", generation, 0, 1);
        for (what, argument) in [
            ("表示していないシート", other_sheet),
            ("前の世代", window_argument(&sheet, previous, 0, 1)),
            (
                "知らない世代",
                window_argument(&sheet, generation + 1, 0, 1),
            ),
        ] {
            let window = window_bytes(answer_rows_window(
                &sessions,
                &grids,
                &label,
                &InvokeBody::Raw(argument),
            ));
            assert!(window.is_empty(), "{what}: 空の窓で答える");
        }
    }

    /// **開いていないウィンドウの要求と、可視行数より後ろの要求は空の窓で答える。**
    ///
    /// どちらも封筒なら失敗腕になる状態である（この経路には封筒が無い）。
    #[test]
    fn a_request_before_opening_or_past_the_end_answers_the_empty_window() {
        let scratch = Scratch::new("window-not-open");
        let path = scratch.file("台帳.jxcel");
        write_document(&path);
        let (sessions, label) = documents(&path);
        let empty_grids = grids();
        let sheet = sheet_id(&path);

        let before = window_argument(&sheet, 0, 0, 1);
        let window = window_bytes(answer_rows_window(
            &sessions,
            &empty_grids,
            &label,
            &InvokeBody::Raw(before),
        ));
        assert!(window.is_empty(), "開いていなければ空の窓である");

        // 開いたあと、可視行数より後ろの開始序数（3 行の標本で 4 番目）。
        let (_scratch, sessions, grids, label) = opened("window-range");
        let path = _scratch.file("台帳.jxcel");
        let sheet = sheet_id(&path);
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        let generation = generation_of(&grids, &label);
        let past = window_argument(&sheet, generation, 4, 2);
        let window = window_bytes(answer_rows_window(
            &sessions,
            &grids,
            &label,
            &InvokeBody::Raw(past),
        ));
        assert!(
            window.is_empty(),
            "可視行数より後ろの要求（0 起点で 4 番目）は空の窓である"
        );
    }

    /// **10 万行のシートから任意の位置の窓が取れ、要件 11.2 の 1 秒に収まる**（要件 1.1、11.2）。
    ///
    /// 窓の費用は**窓の行数**に比例し、シートの行数には依らない（5.1 の「費用の形」）。
    /// ここでは末尾に近い任意の位置（可視行の 99,800 番目）から 200 行を要求し、
    /// **その位置の行の識別子と表示文字列**が返ることを確かめる。
    #[test]
    fn a_window_at_an_arbitrary_position_of_a_hundred_thousand_rows_is_retrieved() {
        const ROWS: usize = 100_000;
        const START: usize = 99_800;
        const WINDOW: usize = 200;

        let scratch = Scratch::new("window-100k");
        let path = scratch.file("台帳.jxcel");
        write_large_document(&path, ROWS);
        let (sessions, label) = documents(&path);
        let grids = grids();
        let sheet = sheet_id(&path);
        assert!(
            matches!(
                answer_open(
                    &sessions,
                    &grids,
                    &label,
                    &GridOpenRequest {
                        sheet: sheet.clone()
                    }
                ),
                IpcResult::Ok { .. }
            ),
            "10 万行のシートも開ける"
        );
        data(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        let generation = generation_of(&grids, &label);

        let started = Instant::now();
        let bytes = window_bytes(answer_rows_window(
            &sessions,
            &grids,
            &label,
            &InvokeBody::Raw(window_argument(
                &sheet,
                generation,
                START as u64,
                WINDOW as u64,
            )),
        ));
        let elapsed = started.elapsed();
        println!(
            "10 万行のシートの {START} 番から {WINDOW} 行の窓: {elapsed:?}（応答 {} バイト）",
            bytes.len()
        );

        let window = decode_window(&bytes).expect("窓は復号できる");
        assert_eq!(START, window.start().get());
        assert_eq!(WINDOW, window.row_count());
        let positions = [START, ROWS - 1];
        let expected = stored_rows_at(&sessions, &label, &positions);
        assert_eq!(
            row_key(&expected[0]),
            window.rows()[0].key(),
            "任意の位置（99,800 番目）の行が載る"
        );
        assert_eq!(
            row_key(&expected[1]),
            window.rows()[WINDOW - 1].key(),
            "窓の最後の行は 99,999 番目である"
        );
        let cells: Vec<&str> = window.rows()[0]
            .cells()
            .iter()
            .map(|cell| cell.text())
            .collect();
        assert_eq!(
            vec![format!("P{START}"), "0".to_owned(), "0".to_owned()],
            cells,
            "その位置の値が載る（99,800 % 100 = 0）"
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "要件 11.2 の予算（1 秒）に収まる: {elapsed:?}"
        );
    }

    // -----------------------------------------------------------------------
    // 破棄（design.md「GridCommands」: ウィンドウが閉じたら破棄する）
    // -----------------------------------------------------------------------

    /// テスト専用のウィンドウの側: **破棄の通知を外から起こせる**二重。
    ///
    /// `AlwaysPresent` は破棄を起こせない（購読を捨てないだけ）ため、破棄で保持が落ちる
    /// ことを観測するにはこちらを使う。
    #[derive(Clone, Default)]
    struct Destructible {
        handlers: Arc<Mutex<HashMap<String, DestroyHandler>>>,
        /// 購読の登録に成功するか（偽は「取得と登録の間に破棄された」を表す）。
        subscribes: bool,
    }

    impl Destructible {
        fn new() -> Self {
            Self {
                handlers: Arc::new(Mutex::new(HashMap::new())),
                subscribes: true,
            }
        }

        /// 名指ししたウィンドウの破棄を起こす。
        fn destroy(&self, label: &WindowLabel) {
            let handler = lock(&self.handlers).remove(label.as_str());
            if let Some(handler) = handler {
                handler(label);
            }
        }

        /// 登録された購読の数。
        fn subscriptions(&self) -> usize {
            lock(&self.handlers).len()
        }
    }

    impl WindowDestroyEvents for Destructible {
        fn subscribe_destroyed(&self, label: &WindowLabel, on_destroyed: DestroyHandler) -> bool {
            if !self.subscribes {
                return false;
            }
            lock(&self.handlers).insert(label.as_str().to_owned(), on_destroyed);
            true
        }

        fn has_window(&self, _label: &WindowLabel) -> bool {
            true
        }
    }

    /// **ウィンドウが閉じると、そのウィンドウの保持は落ちる**（design.md「GridCommands」）。
    ///
    /// 他のウィンドウの保持は変わらない（1 つのウィンドウの破棄が他へ及ばない）。
    #[test]
    fn a_destroyed_window_releases_only_its_own_session() {
        let scratch = Scratch::new("destroy");
        let path = scratch.file("台帳.jxcel");
        write_document(&path);
        let sheet = sheet_id(&path);
        // 2 枚のウィンドウが同じ文書を表示している（`doc-2` も同じ標本を読む）。
        let sessions = Arc::new(DocumentSessions::new());
        let label = WindowLabel::new("doc-1");
        let other = WindowLabel::new("doc-2");
        for target in [&label, &other] {
            sessions
                .resolve(target, Some(&path))
                .expect("標本を読み込める");
        }
        let events = Destructible::new();
        let grids = GridSessions::new(Arc::new(events.clone()));

        for target in [&label, &other] {
            assert!(
                matches!(
                    answer_open(
                        &sessions,
                        &grids,
                        target,
                        &GridOpenRequest {
                            sheet: sheet.clone()
                        }
                    ),
                    IpcResult::Ok { .. }
                ),
                "標本は開ける"
            );
        }
        assert_eq!(
            2,
            events.subscriptions(),
            "ウィンドウごとに 1 回だけ購読する"
        );

        events.destroy(&label);
        assert!(
            grids.entry(&label).is_none(),
            "破棄されたウィンドウの保持は落ちる"
        );
        assert!(
            grids.entry(&other).is_some(),
            "他のウィンドウの保持は変わらない"
        );
        assert_eq!(1, events.subscriptions(), "購読も取り除かれる");

        // 落ちたあとの操作は経路の失敗である（開き直しが要る）。
        let failure = error(answer_set_view(
            &sessions,
            &grids,
            &label,
            &GridViewRequest { view: empty_view() },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));
    }

    /// **購読を登録できないときは保持を置かない**（取得と登録の間に破棄された場合）。
    #[test]
    fn a_window_that_vanishes_before_subscribing_is_not_kept() {
        let scratch = Scratch::new("vanished");
        let path = scratch.file("台帳.jxcel");
        write_document(&path);
        let (sessions, label) = documents(&path);
        let grids = GridSessions::new(Arc::new(Destructible {
            subscribes: false,
            ..Destructible::new()
        }));

        let failure = error(answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet_id(&path),
            },
        ));
        assert!(matches!(failure, IpcError::Document { .. }));
        assert!(grids.entry(&label).is_none(), "購読の無い保持を残さない");
    }

    // -----------------------------------------------------------------------
    // 型の種別の札（tasks.md 6.1 の申し送り）
    // -----------------------------------------------------------------------

    /// **`TypeKind` の対応は総関数であり、その像は `TypeKindTag::ALL` と綴り・件数・並びが一致する。**
    ///
    /// この検査がここにしか置けない理由は 2 つある。`app-shell` は他のドメインクレートに
    /// 依存できないため `TypeKind` を見られず、`schema-engine` は境界の型
    /// （`ts-rs` の derive を持つ型）を持てない（置けるのは `crates/app-shell/src/ipc/` の
    /// 下だけである）。**両方を見られる唯一のクレートが `src-tauri` である。**
    ///
    /// # 何を捕まえるか
    ///
    /// - **対応は総関数である** — [`type_kind_tag`] はワイルドカードの無い `match` であり、
    ///   [`TypeKind`] に変種が増えれば**コンパイルが壊れる**（片側だけの追加をその場で止める）。
    ///   したがって「写像が種別を取りこぼす」ことは起こり得ない
    /// - **件数** — [`TypeKindTag::ALL`] に札が増えれば、写像の像（`TypeKind::ALL` の像）の
    ///   件数と食い違い、下の一致で落ちる（境界にだけ種別が増えた場合）
    /// - **綴り** — 変種の名前が食い違えば（改名・綴り間違い）落ちる
    /// - **並び** — 6.1 の doc が定める「並びは `TypeKind::ALL` と同じ」を固定する
    #[test]
    fn type_kind_tag_covers_every_type_kind() {
        assert_eq!(
            TypeKind::ALL.len(),
            TypeKindTag::ALL.len(),
            "種別の数が食い違っている（片方だけに種別が増えていないか）"
        );

        let image: Vec<TypeKindTag> = TypeKind::ALL.into_iter().map(type_kind_tag).collect();
        assert_eq!(
            TypeKindTag::ALL.to_vec(),
            image,
            "並びと綴りが一致しない（写像と境界の札を突き合わせること）"
        );

        // 綴りを明示的に突き合わせる（`Vec` の一致だけでは、両側が同時に同じ綴りへ
        // 間違えられた場合に気づけない）。14 種すべてを 1 つずつ名指しする。
        for (kind, tag) in TypeKind::ALL.into_iter().zip(TypeKindTag::ALL) {
            assert_eq!(format!("{kind:?}"), format!("{tag:?}"), "綴りが食い違う");
        }

        // 像の要素数（同じ札へ 2 つの種別が写っていれば、この数が減る）。
        let mut unique = image;
        unique.sort();
        unique.dedup();
        assert_eq!(
            TypeKindTag::ALL.len(),
            unique.len(),
            "2 つの種別が同じ札へ写っている"
        );
    }
    // -----------------------------------------------------------------------
    // メニューからの引き金（タスク 8.7。要件 7.8）
    // -----------------------------------------------------------------------

    /// 複製の項目が**編集の部分メニュー**へ、プラットフォーム解決済みの綴りで登録されること
    /// （要件 7.8、3.3、3.4）。**GUI を起こさない** — `MenuRegistry::enroll` は画面を要しない
    /// （9.5 の診断の導線と同じ検査の形）。
    #[test]
    fn the_copy_item_is_registered_in_the_edit_submenu() {
        let registry = MenuRegistry::new();
        registry
            .enroll(copy_item_spec(|_: &MenuSelection| {}))
            .expect("競合なく登録できる");

        assert_eq!(
            copy_menu_path().segments(),
            &[EDIT_MENU_LABEL.to_owned()],
            "複製は編集の部分メニューに置く"
        );

        let model = registry.model();
        let items = model.items();
        assert_eq!(
            items.len(),
            1,
            "この module が置くのは複製の 1 件だけである"
        );
        let node = items[0];
        assert_eq!(node.item(), &MenuItemId::new(COPY_ITEM_ID));
        assert_eq!(node.label(), COPY_LABEL);
        assert_eq!(node.owner().as_str(), OWNER);

        let accelerator =
            Accelerator::parse(COPY_ACCELERATOR_SPELLING).expect("プラットフォーム解決済みの綴り");
        assert_eq!(node.accelerator(), Some(&accelerator));
        // 慣習どおりの組み合わせであり、**正準形がプラットフォームごとに違う**こと
        // （非 macOS `Ctrl+C` / macOS `Cmd+C`）がここに現れる。`scripts/check-menu-shortcut.sh`
        // の配置の記録もこの綴りを要求する。
        let expected = if cfg!(target_os = "macos") {
            "super+KeyC"
        } else {
            "ctrl+KeyC"
        };
        assert_eq!(accelerator.as_str(), expected);
        // **`CmdOrCtrl` は綴りとして受理されない**（4.6 の構文契約。受理すると、同じ論理
        // ショートカットがプラットフォームごとに別の組み合わせとして扱われ、競合を見落とす）。
        assert!(Accelerator::parse("CmdOrCtrl+C").is_err());
    }

    /// **貼り付けの項目を登録していない**（要件 7.8 の後半は未達である。理由は [`install`] の doc）。
    ///
    /// 読み口が無いまま `Ctrl+V` を登録すると、基盤のメニューが打鍵を先に受け取り、**いま
    /// 動いている打鍵の貼り付け（DOM の `paste`）が届かなくなる**。この検査は「登録しない」と
    /// いう判断が実装に現れていることを固定する（判断を変えるときは、読み口を先に足す）。
    #[test]
    fn no_paste_item_is_registered() {
        let registry = MenuRegistry::new();
        registry
            .enroll(copy_item_spec(|_: &MenuSelection| {}))
            .expect("競合なく登録できる");

        let model = registry.model();
        let items = model.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item().as_str(), COPY_ITEM_ID);

        let paste = Accelerator::parse("Ctrl+V").expect("正準形");
        assert!(
            items.iter().all(|item| item.accelerator() != Some(&paste)),
            "貼り付けのショートカットを登録していない"
        );
        assert!(
            items
                .iter()
                .all(|item| !item.item().as_str().contains("paste")),
            "貼り付けの項目を作っていない"
        );
    }

    // -----------------------------------------------------------------------
    // メニューからの取り消しとやり直し（タスク 8.9。要件 9.9）
    // -----------------------------------------------------------------------

    /// 取り消しとやり直しの項目が、**編集の部分メニューへ 2 つ**、プラットフォーム解決済みの
    /// 綴りで登録されること（要件 9.9、3.3、3.4）。**GUI を起こさない**（8.7 の複製と同じ形）。
    ///
    /// **2 つの項目が同じ 1 つのイベントを送る**ことは、この検査では「項目が 2 つ在り、それぞれ
    /// の綴りが違う」ことまでで、送り先の一致は [`history_item_spec`] の形（`request_history`
    /// が向きを荷に載せる）が担う — 画面の側は `history.test.ts` が「2 つの項目が同じ入口へ
    /// 着く」ことで固定する。
    #[test]
    fn the_history_items_are_registered_in_the_edit_submenu() {
        let registry = MenuRegistry::new();
        for direction in [GridHistoryDirection::Undo, GridHistoryDirection::Redo] {
            registry
                .enroll(history_item_spec(direction, |_: &MenuSelection| {}))
                .expect("競合なく登録できる");
        }

        assert_eq!(
            history_menu_path().segments(),
            &[EDIT_MENU_LABEL.to_owned()],
            "取り消しとやり直しは編集の部分メニューに置く"
        );

        let model = registry.model();
        let items = model.items();
        assert_eq!(items.len(), 2, "この module が置くのは 2 件である");
        // **並びは登録の順ではない**（登録口の模型は識別子で整列する）ので、識別子で引く。
        let node = |item: &str| {
            items
                .iter()
                .find(|candidate| candidate.item().as_str() == item)
                .copied()
                .unwrap_or_else(|| panic!("{item} が登録されていない"))
        };
        let undo_item = node(UNDO_ITEM_ID);
        assert_eq!(undo_item.label(), UNDO_LABEL);
        let redo_item = node(REDO_ITEM_ID);
        assert_eq!(redo_item.label(), REDO_LABEL);
        for item in &items {
            assert_eq!(item.owner().as_str(), OWNER, "本スペックの名前空間である");
        }

        // **綴りはプラットフォームで解決済みであり、`Accelerator::parse` と一致する。**
        let undo = Accelerator::parse(UNDO_ACCELERATOR_SPELLING).expect("解決済みの綴り");
        let redo = Accelerator::parse(REDO_ACCELERATOR_SPELLING).expect("解決済みの綴り");
        assert_eq!(undo_item.accelerator(), Some(&undo));
        assert_eq!(redo_item.accelerator(), Some(&redo));
        // 正準形も固定する（非 macOS `Ctrl+Z` / `Ctrl+Shift+Z`、macOS `Cmd+Z` /
        // `Cmd+Shift+Z`）。**修飾キーの並びは `MODIFIER_ORDER` が決める**
        // （`ctrl` < `alt` < `shift` < `super`）ので、macOS のやり直しは `shift+super+KeyZ` で
        // ある（`scripts/ci/macos/verify-menu-shortcuts.sh` の配置の記録もこの綴りを要求する。
        // 既存の `Cmd+Shift+J` が `shift+super+KeyJ` であるのと同じ規則である）。
        let (expected_undo, expected_redo) = if cfg!(target_os = "macos") {
            ("super+KeyZ", "shift+super+KeyZ")
        } else {
            ("ctrl+KeyZ", "ctrl+shift+KeyZ")
        };
        assert_eq!(undo.as_str(), expected_undo);
        assert_eq!(redo.as_str(), expected_redo);
        // **2 つは別の組み合わせである**（同じ組み合わせなら、片方は到達できない経路になる）。
        assert_ne!(undo, redo, "取り消しとやり直しが同じ打鍵になっている");
        // **`CmdOrCtrl` は綴りとして受理されない**（4.6 の構文契約。受理すると、同じ論理
        // ショートカットがプラットフォームごとに別の組み合わせとして扱われ、競合を見落とす）。
        assert!(Accelerator::parse("CmdOrCtrl+Z").is_err());
        // **非 macOS で `Ctrl+Y` を採らない**（GTK の慣習に揃える。`design.md` の
        // 「メニューの取り消し・やり直しの結線」）。
        if !cfg!(target_os = "macos") {
            assert_ne!(redo.as_str(), "ctrl+KeyY");
        }
    }
}

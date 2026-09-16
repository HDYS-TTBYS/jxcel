/**
 * グリッド画面が境界へ出す呼び出しの**薄いラッパ**（tasks.md 8.1。design.md「File Structure
 * Plan」の `src/features/grid/gridClient.ts`。6.2 のコマンドと `src/ipc/client.ts` の間に置く）。
 *
 * 所有: `GridClient`（design.md「Components and Interfaces → Frontend Layer」の GridScreen が
 * 使う境界の口）。
 *
 * # なぜ 1 枚はさむのか（画面から綴りを追い出す）
 *
 * 3 つの理由がある。どれも「画面を読めるままに保つ」ためのものである。
 *
 * 1. **コマンドの綴りを 1 箇所に閉じる。** 画面（`./GridScreen`）は
 *    `"document_state"` / `"grid_open_sheet"` / `"grid_rows_window"` という文字列を 1 つも
 *    書かない。綴りを書かないので、境界の名前が変わっても画面は触らない — 触るのは
 *    [`createGridClient`] の 3 行だけである。名前は `src/ipc/client.ts` の `CommandName` /
 *    `RawCommandName` で型付けしてあるので、**生成物（`src/ipc/bindings.ts` の
 *    `COMMAND_NAMES`）に無い綴りはその場で型検査が落ちる**（要件 4.1、4.2）。
 * 2. **画面の検査が境界を呼ばずに済む。** 画面は [`GridClient`] を受け取る（既定は
 *    [`createGridClient`]）。検査は応答を固定した偽の実装を渡せるので、**IPC も Tauri も
 *    DOM も無しに**画面の流れ（開く・失敗・再試行）を読める。
 * 3. **生バイト経路の引数の形を 1 箇所に保つ。** `grid_rows_window` の引数は**バッファ全体**で
 *    あり、入れ子（`{ argument: buffer }`）にすると Tauri が数値の配列へ変換して**例外を
 *    投げずに空の窓を返す**（`src-tauri/src/commands/bulk.rs` のモジュール docs「経路の性質」、
 *    および `./windowCache` の `defaultTransport`）。引数の組み立て（`encodeWindowRequest`）は
 *    7.3 の窓の記憶が持ち、本 module は**その 1 つの引数をそのまま渡す**だけである。
 *
 * # 判定を持たない
 *
 * 本 module は封筒を開けない。成功と失敗は `status` で判別できる型のまま返り、**どちらの腕を
 * どう見せるか**は画面（`./GridScreen`）が決める（design.md「Error Handling」— 失敗の理由の
 * 文言は適応層が組み立て、見せ方を決めるのは呼び出し元である）。例外を投げるのは移送そのものが
 * 拒否されたときだけであり、それも `invoke` の拒否として `src/ipc/client.ts` が封筒の失敗へ
 * 写す（`FrontendIpcError`）。
 */
import { invokeCommand, invokeRaw } from "../../ipc/client";
import type { CommandName, IpcClientResult, RawCommandName } from "../../ipc/client";
import type {
  DocumentStateResponse,
  GridEditCommand,
  GridEditResponse,
  GridHistoryDirection,
  GridOpenResponse,
  GridViewResponse,
  GridViewSpec,
  GridViolationRequest,
  GridViolationResponse,
} from "../../ipc/bindings";

/**
 * セッションの状態を問い合わせるコマンド（タスク 3.1 の口。要件 1.6、1.7）。
 *
 * 画面がここから読むのは**シートの一覧（識別子・名前・列数・行数）**である。列数は要件 1.6 の
 * 判定に要る（後述）。
 */
const DOCUMENT_STATE_COMMAND: CommandName = "document_state";

/** 表示するシートを開くコマンド（6.2。要件 1.1、1.5、1.6）。 */
const GRID_OPEN_SHEET_COMMAND: CommandName = "grid_open_sheet";

/**
 * 表示の指定（並べ替え・絞り込み・展開）を適用するコマンド（6.2。要件 8.3、8.4、8.7）。
 *
 * **開いた直後に 1 度だけ呼ぶ。** `GridSession` は**可視行の順序をこの呼び出しで導出する**
 * ため（`crates/data-grid/src/api.rs` の `set_view` が `recompute_order` を行う）、呼ばずに
 * 窓を要求すると**どの窓も行 0 件（頭だけの 33 バイト）で返る** — 表は読み込み中のままに
 * なる（8.1 の起動観測で実測した）。空の指定は「絞り込み無し・並べ替え無し・展開無し」で
 * あり、文書の行順と宣言の列がそのまま現れる（`GridViewSpec` の doc）。
 */
const GRID_SET_VIEW_COMMAND: CommandName = "grid_set_view";

/** 窓を生バイトで取るコマンド（6.2 / 7.3。要件 1.4、11.2）。 */
const GRID_ROWS_WINDOW_COMMAND: RawCommandName = "grid_rows_window";

/**
 * 次の違反を探すコマンド（6.2。要件 4.2、4.4）。**起点は可視行の序数であり、向きは前向きだけを
 * 本機能が使う**（要件 4.4 は「次の違反への移動」である。8.4）。
 */
const GRID_FIND_VIOLATION_COMMAND: CommandName = "grid_find_violation";

/**
 * 編集命令を 1 つ適用するコマンド（6.2。要件 3.3、3.4、3.5、1.7）。
 *
 * 応答が運ぶのは**判定の結果**である（影響範囲・型強制・違反・行数）— 画面はそれで窓を捨て、
 * 変換と違反を提示する。**適合しない値も破棄されずに返る**（`src-tauri/src/commands/grid.rs` の
 * `grid_apply_edit` の docs。判定するのは `schema-engine` である）。
 */
const GRID_APPLY_EDIT_COMMAND: CommandName = "grid_apply_edit";

/**
 * 履歴を 1 つ進めるコマンド（6.2。要件 9.2、9.3）。
 *
 * **取り消しとやり直しは同じコマンドである** — どちらへ進めるかは要求（`direction`）が運ぶ。
 * 応答の形は編集の適用と同じである（`design.md`「GridCommands」の API Contract が
 * `grid_apply_edit` と `grid_history` の応答を同じ型と定めている）。
 */
const GRID_HISTORY_COMMAND: CommandName = "grid_history";

/**
 * 表示の指定を変えない指定（**空の指定＝絞り込み無し・並べ替え無し・展開無し**）。
 *
 * 8.1 は開いた直後にこれ 1 つだけを適用し、**並べ替え・絞り込み・展開の操作は 8.8 の担当**
 * である（本 module は操作を持たない）。
 */
export const EMPTY_GRID_VIEW: GridViewSpec = { sort: [], filters: [], expansion: [] };

/** 画面が境界へ出す口。**この 7 つだけである。** */
export interface GridClient {
  /** 呼び出し元ウィンドウのセッションの状態（シートの一覧を含む）。 */
  readonly readDocumentState: () => Promise<IpcClientResult<DocumentStateResponse>>;
  /**
   * 表示するシートを開く。**シートは識別子の文字列で選ぶ**（64 ビット整数を境界へ出さない
   * 規約。`GridOpenRequest` の doc）ので、呼び出し側は `DocumentSheet.id` をそのまま渡す。
   *
   * 応答の `sheet`（`GridSheetSummary`）が**列の構成とシートの行数**を運ぶ — 要件 1.5 の
   * 「列はあるが行が無い」はここから読む。要件 1.6 の「列が 1 本も無い」はここへは来ない
   * （列 0 本の計画は Rust 側が `SchemaUnusable` として拒む。`./GridScreen` の module doc）。
   */
  readonly openSheet: (sheet: string) => Promise<IpcClientResult<GridOpenResponse>>;
  /**
   * 表示の指定を適用し、**可視行の順序と列の構成を導出させる**（要件 5.1、5.2、8.3、8.4、8.7）。
   *
   * 応答の `visible_rows` が**窓が覆う行数**である（窓の区間は可視行の序数で表される。
   * `RowSpan` の doc）。応答の `columns` が**導出後**の列の構成であり（要件 5.1、5.2）、
   * **入れ子の展開・折りたたみが見える唯一の源**である（`GridViewResponse` の doc）。8.1 は
   * [`EMPTY_GRID_VIEW`] だけを渡す（展開が無いので、開いたときの構成と同じ並びが返る）。
   */
  readonly setView: (view: GridViewSpec) => Promise<IpcClientResult<GridViewResponse>>;
  /**
   * 窓を 1 つ取る（**引数はバッファそのもの**。上のモジュール doc の 3）。失敗は拒否として
   * 現れ、呼び出し側（7.3 の窓の記憶）は**未取得のまま残して次の引きで再試行する**
   * （設計の誤り表「経路の失敗」）。本 module は拒否を握らない。
   */
  readonly readWindow: (argument: Uint8Array) => Promise<ArrayBuffer>;
  /**
   * 編集命令を 1 つ適用し、**判定の結果**（影響範囲・型強制・違反・行数）を受ける
   * （要件 3.3、3.4、3.5、1.7）。
   *
   * 本口が受けるのは**命令そのもの**であり、宛先（行の識別子と列の添字）も命令の中にある
   * （`GridCellEdit`）。**打たれた文字を解釈しない** — 適合を判定するのは `schema-engine` で
   * あり、本口はその結果を写すだけである（`./cellEdit` の module doc）。
   */
  readonly applyEdit: (
    command: GridEditCommand,
  ) => Promise<IpcClientResult<GridEditResponse>>;
  /**
   * 指定した起点から、その向きで最も近い違反を探す（要件 4.2、4.4）。
   *
   * **起点は可視行の 0 起点の序数**であり、ウィンドウの行ではない（生成物の
   * `GridViolationRequest` の doc）。応答が運ぶのは**行の識別子**（26 文字）と列の添字と理由で
   * あり、**可視行の序数は運ばない** — 序数へ写すのは `./violations` の仕事である
   * （同 module の「序数の失われた欄」の節）。
   *
   * `direction` は生成物の綴り（`"forward"` / `"backward"`）をそのまま渡す。本機能が使うのは
   * 前向きだけである（8.4）が、**本口は境界の薄い写しであり、方針を持たない**。
   */
  readonly findViolation: (
    request: GridViolationRequest,
  ) => Promise<IpcClientResult<GridViolationResponse>>;
  /**
   * 履歴を 1 つ進め、**判定の結果**（影響範囲・型強制・違反・行数）を受ける
   * （8.9。要件 9.2、9.3、1.7）。
   *
   * **向きは生成物の閉じた列挙**（`"undo"` / `"redo"`）をそのまま渡す。取り消しとやり直しは
   * **同じ 1 つの口**であり、本口を 2 つに割らない（割れば、適用のあとの後始末が片方だけに
   * 載る余地ができる。`./history` の module doc）。
   *
   * 応答の `outcome` が `null` でありうるのは**適用と違って正常**である（「進める履歴が
   * 無かった」＝要件 9.2、9.3 の正常な結果。生成物の `GridEditResponse` の doc）。**本口は
   * それを失敗へ写さない** — 失敗かどうかを決めるのは呼び出し側（`./history`）である。
   */
  readonly readHistory: (
    direction: GridHistoryDirection,
  ) => Promise<IpcClientResult<GridEditResponse>>;
}

/**
 * 既定の実装。**`src/ipc/client.ts` の 2 つの入口へ委譲するだけである**（型付け以上のことを
 * しない。判定も文言も持たない）。
 *
 * **要求の型は `request` という名前の引数で渡す。** `grid_open_sheet` の実体は
 * `fn grid_open_sheet(app, window, request: GridOpenRequest)`（`src-tauri/src/commands/grid.rs`）
 * であり、Tauri が縛る payload の鍵は**引数の名前**である（`{ request: { sheet } }`）。
 * `{ sheet }` を直接渡すと**コマンドへ届く前に復号が失敗し**、封筒ではなく `invoke` の拒否と
 * して現れる（画面はそれを失敗として扱うが、原因は引数の形である）。
 */
export function createGridClient(): GridClient {
  return {
    readDocumentState: () => invokeCommand<DocumentStateResponse>(DOCUMENT_STATE_COMMAND),
    openSheet: (sheet) =>
      invokeCommand<GridOpenResponse>(GRID_OPEN_SHEET_COMMAND, { request: { sheet } }),
    setView: (view) => invokeCommand<GridViewResponse>(GRID_SET_VIEW_COMMAND, { request: { view } }),
    readWindow: (argument) => invokeRaw(GRID_ROWS_WINDOW_COMMAND, argument),
    // 編集命令も**`request` という名前の引数で包む**（実体は
    // `fn grid_apply_edit(app, window, request: GridEditRequest)`。`grid_open_sheet` と同じ規律で
    // あり、包まないと復号の失敗が `invoke` の拒否として現れる。この file の module doc）。
    applyEdit: (command) =>
      invokeCommand<GridEditResponse>(GRID_APPLY_EDIT_COMMAND, { request: { command } }),
    // 違反の探索も**`request` という名前の引数で包む**（実体は
    // `fn grid_find_violation(app, window, request: GridViolationRequest)`）。
    findViolation: (request) =>
      invokeCommand<GridViolationResponse>(GRID_FIND_VIOLATION_COMMAND, { request }),
    // 履歴も**`request` という名前の引数で包む**（実体は
    // `fn grid_history(app, window, request: GridHistoryRequest)`）。
    readHistory: (direction) =>
      invokeCommand<GridEditResponse>(GRID_HISTORY_COMMAND, { request: { direction } }),
  };
}

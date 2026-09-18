/**
 * マクロの実行の面が使う境界の口（tasks.md 4.4。要件 1.3、1.4、2.1、2.5）。
 *
 * # 何をするか（**型付け以上のことをしない**）
 *
 * [`src/ipc/client.ts`] の入口（`invokeCommand`）と、ドキュメントセッションの薄いラッパ
 * （[`src/ipc/documentSession.ts`] の `documentState`）へ委譲するだけである。判定も文言も
 * 持たない — 一覧の解釈も結果の提示も [`./surface`] の仕事であり、本 module は
 * 「どのコマンドを、どの引数の形で呼ぶか」だけを持つ（`src/features/grid/gridClient.ts` と
 * 同じ規律）。
 *
 * **コマンド名は生成物（`src/ipc/bindings.ts` の `COMMAND_NAMES`）の型で受ける。** 綴りを
 * 書くのは 2 つの定数（`macro_list` / `macro_run`）だけで、`document_state` の綴りは
 * `src/ipc/documentSession.ts` が持つ（2 つ目の綴りをここへ作らない）。
 * `CommandName` へ代入できない文字列はその場で型検査が落ちる（手書きの文字列が `invoke` へ
 * 届く経路を作らない）。
 *
 * # 何を呼ばないか（**口を狭く保つ**）
 *
 * 使うのは `document_state`・`macro_list`・`macro_run` の 3 つだけである。**保存
 * （`macro_store`）と削除（`macro_delete`）は呼ばない** — マクロを書く面は本機能の外
 * （`macro-editor-lsp` が所有し、本スペックは保存と実行だけを持つ。要件の Boundary Context）
 * であり、4.4 の面は「一覧・能力の提示・実行・結果」の 4 つを担う。
 *
 * `document_state` を読むのは**一覧の前に文書が付いているかを知るため**である（[`./store`]
 * の `refresh` の doc。起動直後に文書がまだ関連付いていないウィンドウで `macro_list` を
 * 呼ぶと、失敗が記録に 1 行残り、面にも一瞬失敗が見える）。
 *
 * **上限（時間とメモリ）は要求に載せない。** 要求の型（`MacroRunRequest`）が運ぶのは名前だけ
 * であり、上限は Rust 側が設定から解決する（要件 6.5。`src-tauri/src/commands/macro.rs` の
 * `resolve_limits`）。
 */
import { invokeCommand } from "../../ipc/client";
import type { CommandName, IpcClientResult } from "../../ipc/client";
import { documentState } from "../../ipc/documentSession";
import type {
  DocumentStateResponse,
  MacroListResponse,
  MacroRunResponse,
} from "../../ipc/bindings";

/**
 * 一覧のコマンド名（要件 1.3、1.4）。**要求の型は無い** — 対象ウィンドウは基盤が注入する
 * （`src-tauri/src/commands/macro.rs` の `macro_list`）。
 */
const MACRO_LIST_COMMAND: CommandName = "macro_list";

/**
 * 実行のコマンド名（要件 2.1、2.5）。実体は
 * `fn macro_run(app, window, request: MacroRunRequest)` であり、**payload の鍵は引数の名前**
 * である（`{ request: { name } }`。包まないとコマンドへ届く前に復号が失敗し、封筒ではなく
 * `invoke` の拒否として現れる。`src/features/grid/gridClient.ts` の同じ注意）。
 */
const MACRO_RUN_COMMAND: CommandName = "macro_run";

/** 実行の面が境界へ出す口。**この 3 つだけである。** */
export interface MacroClient {
  /**
   * このウィンドウのドキュメントのセッションの状態（`document_state`。`src/ipc/documentSession.ts`
   * の薄いラッパへ委譲するだけである）。
   *
   * **一覧を求める前に読む**（[`./store`] の `refresh`）。文書がまだ関連付いていない
   * （`Absent`）うちは一覧を求めない — 求めた呼び出しは失敗し、記録に 1 行残り、面にも
   * 一瞬失敗が見えるためである。`Unavailable` は**読めなかった理由**（`reason`）を運び、
   * 面はそれをそのまま出す（文言を組み立てない）。
   */
  readonly readDocumentState: () => Promise<IpcClientResult<DocumentStateResponse>>;
  /**
   * 呼び出し元ウィンドウの文書が持つマクロの一覧（保存順。要件 1.3）。
   *
   * **解釈できなかったマクロも 1 件として載る**（要件 1.4）。その 1 件は `failure` に理由を
   * 持ち、`capabilities` が空である（`MacroSummary` の doc。解釈できた 1 件は `failure` が
   * `null` であり、宣言が無ければ `capabilities` が空である — 両者は `failure` の有無で
   * 区別できる）。
   */
  readonly list: () => Promise<IpcClientResult<MacroListResponse>>;
  /**
   * 1 件を実行する（要件 2.1、2.5）。
   *
   * **失敗と打ち切りは封筒の成功腕で運ばれる**（`MacroRunResponse.outcome` の 3 値）。
   * 封筒の失敗腕へ落ちるのは経路の失敗だけである（文書が無い・その名前が無い・実行基盤が
   * 要求を受け取らない・変更の適用が拒まれた。`src-tauri/src/commands/macro.rs` の表）。
   * **本 module は両者を区別せずに写す** — 提示の区別は [`./surface`] が持つ。
   */
  readonly run: (name: string) => Promise<IpcClientResult<MacroRunResponse>>;
}

/**
 * 既定の実装。**`src/ipc/client.ts` の入口へ委譲するだけである。**
 */
export function createMacroClient(): MacroClient {
  return {
    readDocumentState: () => documentState(),
    list: () => invokeCommand<MacroListResponse>(MACRO_LIST_COMMAND),
    run: (name) => invokeCommand<MacroRunResponse>(MACRO_RUN_COMMAND, { request: { name } }),
  };
}

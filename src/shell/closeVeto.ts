/**
 * ウィンドウの終了拒否の購読 — 終了要求を受けたらドキュメント所有者へ可否を問い合わせ、
 * 許可されたときだけウィンドウを閉じる。
 *
 * 所有: 終了拒否の仲介のフロントエンド側（design.md「ウィンドウの終了拒否」のシーケンス図）。
 * 要件: 2.6。
 *
 * # なぜ「購読を登録するだけ」で拒否されるのか
 *
 * 基盤（Tauri ランタイム）は `tauri://close-requested` に対する **JS リスナが登録されている
 * ことだけを検出して** `prevent_close()` を呼ぶ（`tauri` 2.11.5 の
 * `src/manager/window.rs`: `if window.has_js_listener(WINDOW_CLOSE_REQUESTED_EVENT) {
 * api.prevent_close(); }`）。判定が非同期に決まる以上、「待ってから拒否する」ことはできない
 * ため、**拒否は先に確定させ、可否の往復を後から載せる**構造になっている。
 *
 * したがって **Rust 側に 2 つ目の拒否は無い**（`src-tauri/src/window/close.rs` のモジュール doc）。
 * この購読が存在しないウィンドウは、上の自動拒否が働かず、ウィンドウマネージャの要求どおり
 * そのまま閉じる（購読の登録に失敗したときの挙動もこれである。下の `installCloseVeto` を参照）。
 *
 * # 閉じる操作は `destroy()` だけ（`close()` は使わない）
 *
 * 許可されたときに使うのは `WebviewWindow.destroy()` である。`close()` は
 * `CloseRequested` を**再発火**するため、上の自動拒否に再突入する（終了要求 → 拒否 → 問い合わせ
 * → `close()` → 終了要求 … の循環になる。research.md「ウィンドウを閉じる操作の拒否」）。
 * `destroy()` は `CloseRequested` を発火せずにウィンドウを破棄し、`Destroyed` の通知だけを
 * 発火する（位置とサイズの保存はその通知で行われる。要件 2.7、タスク 6.3）。
 *
 * # 判定が得られなかったときは閉じない（この選択の理由）
 *
 * `can_close_window` が失敗した場合（封筒が `status: "error"`、または呼び出し自体が拒否された
 * 場合）は**ウィンドウを閉じない**。理由は 2 つある:
 *
 * 1. **不明な判定で閉じる方が危険である。** 閉じた結果は利用者にとって取り消せない。委譲先が
 *    まだ答えていないだけかもしれないのに、未保存の変更を持つウィンドウを破棄しうる。
 *    要件 2.6 も「拒否された場合は閉じない」であり、既定は「閉じない」側に倒すのが整合する。
 * 2. **ウィンドウは開いたままなので、利用者はもう一度閉じる操作をするだけで再試行できる。**
 *    この購読は残り続けるため、次の終了要求でもう一度問い合わせが走る。恒久的に閉じられなく
 *    なる経路は無い。
 *
 * 同じ判断を `destroy()` の失敗にも適用する（ウィンドウが既に無い場合を除き、閉じない）。
 *
 * # 拒否の提示の差し替え口（タスク 4.2）
 *
 * 拒否（`Deny`）の扱いは**差し替え口 1 つ**（[`installCloseDenialHandler`]）を通す。**既定は
 * 現行の記録のみ**（`console.warn` の 1 行）であり、ハンドラを入れない限り app-shell の振る舞いは
 * 1 ビットも変わらない（app-shell の要件 2.6 の再確認は design.md の Revalidation Triggers）。
 *
 * 差し替え口をここに置く理由は、**拒否の材料（ウィンドウと理由）がこの往復の中にしか無い**
 * ためである。利用者へ提示するかどうかはシェルの画面が決める（[`CloseDenial`] の doc を参照）が、
 * 材料を運ぶのはこの購読の仕事である。
 *
 * ハンドラが受け取る [`CloseDenial`] は**閉じる要求をもう一度通す口**（`retry`）を持つ。
 * **判定は作り直さない** — `retry` は下の往復（`can_close_window` の問い合わせと、許可のときの
 * `destroy()`）をそのままやり直すだけであり、**その時点の状態に基づいて答え直される**
 * （要件 6.7）。保存や破棄のあとにこれを通すと、未保存の印が落ちているので `Allow` に到達し、
 * 既存の判断どおりウィンドウが破棄される（要件 6.3、6.5）。
 *
 * **`close()` を再試行に使わない**のは冒頭の理由と同じである（`CloseRequested` を再発火して
 * 自動拒否に再突入する）。加えて `close` のコマンドは権限集合（`src-tauri/capabilities/`）に
 * 含まれない — 与えられているのは `core:window:allow-destroy` だけである。
 */

import { getCurrentWindow } from "@tauri-apps/api/window";

import type { CanCloseWindowResponse } from "../ipc/bindings";
import { invokeCommand, type CommandName } from "../ipc/client";

/**
 * 終了可否の問い合わせコマンドの名前。
 *
 * **文字列を直接 `invoke` へ渡さない。** 型注釈（[`CommandName`]）は生成物
 * `src/ipc/bindings.ts` の `COMMAND_NAMES` から導かれた合併型であるため、
 * `crates/app-shell/src/ipc/command_names.rs` からこの名前が消えると**この行で型検査が落ちる**
 * （tasks.md 2.2 / 2.4 の拡張規則。手書きの名前を許さない）。
 */
const CAN_CLOSE_WINDOW_COMMAND: CommandName = "can_close_window";

/**
 * 差し替え口が受け取る拒否（design.md「System Flows → 終了前の問い」の矢印
 * `Veto->>Prompt: 3 択を提示`）。
 *
 * 運ぶのは**提示の材料だけ**である。文言の組み立ては受け取った側（シェルの画面）が行う —
 * このファイルは app-shell の持ち物であり、利用者へ何を見せるかを決めない
 * （`WindowCloseVerdict` の doc と同じ判断）。
 */
export interface CloseDenial {
  /** 拒否した判定を返したウィンドウのラベル（境界の応答の `context.window`）。 */
  readonly window: string;
  /** 委譲先が示した拒否の理由（`WindowCloseVerdict::Deny` の `reason`）。 */
  readonly reason: string;
  /**
   * 閉じる要求をもう一度通す。**この購読の往復をそのままやり直す**ので、判定は固定されず、
   * その時点の状態に基づいて答え直される（要件 6.7）。
   *
   * 呼び出しは非同期の往復を始めるだけであり、`await` しても意味のある完了は無い
   * （拒否された場合は新しい [`CloseDenial`] がハンドラへ届く）。
   */
  readonly retry: () => void;
}

/**
 * 拒否の受け取り手。**例外を投げないこと**が契約である（この購読は終了要求の処理の中から
 * 呼ぶので、投げると基盤のイベント処理へ漏れる）。
 */
export type CloseDenialHandler = (denial: CloseDenial) => void;

/** 設置済みの受け取り手。`null` なら既定の記録のみ（下の [`installCloseDenialHandler`]）。 */
let denialHandler: CloseDenialHandler | null = null;

/**
 * 拒否の提示を差し替える。**`null` を渡すと既定（記録のみ）へ戻る。**
 *
 * 既定（設置しない場合）は `console.warn` の 1 行だけである — 現在の振る舞いをそのまま保ち、
 * **この差し替え口の追加が app-shell の観測可能な振る舞いを変えない**ようにする。
 * 最後に設置したハンドラだけが有効であり、戻り値で解除しない（起動時に 1 回入れる用途だけを
 * 想定する。`src/main.tsx` の `installSessionClosePrompt`）。
 */
export function installCloseDenialHandler(handler: CloseDenialHandler | null): void {
  denialHandler = handler;
}

/**
 * 拒否を提示へ渡す。**受け取り手が居なければ既定の記録を行う**（モジュール doc の差し替え口）。
 *
 * 受け取り手の中で投げられた例外はここで受け止める — 契約違反であっても、終了要求の処理を
 * 壊して「閉じられなくなる」経路を作らないためである（既定の記録へは戻らない。
 * どちらを選ぶかは設置した側が決めており、ここで二重に見せない）。
 */
function publishDenial(denial: CloseDenial): void {
  const handler = denialHandler;
  if (handler === null) {
    console.warn(
      `ドキュメント所有者が ${denial.window} の終了を拒否した: ${denial.reason}`,
    );
    return;
  }
  try {
    handler(denial);
  } catch (error: unknown) {
    console.error("終了拒否の提示に失敗した", error);
  }
}

/**
 * 終了要求 1 回分の往復 — 閉じてよいかを問い合わせ、**許可されたときだけ** `destroy()` する。
 *
 * `CloseRequested` の購読と [`CloseDenial.retry`] の両方から呼ばれる**唯一の判定経路**である
 * （判定を 2 箇所に写すと、片方だけ直したときに「提示では閉じるが購読では閉じない」という
 * 食い違いが生まれる）。
 *
 * **例外を外へ出さない。** 判定が得られない場合は閉じない（冒頭の
 * 「判定が得られなかったときは閉じない」）。
 */
async function evaluateCloseRequest(): Promise<void> {
  const result = await invokeCommand<CanCloseWindowResponse>(
    CAN_CLOSE_WINDOW_COMMAND,
  );

  if (result.status === "error") {
    // 判定が得られなかった → 閉じない（冒頭の「判定が得られなかったときは閉じない」）。
    console.warn(
      `終了可否を問い合わせられなかったため、ウィンドウを閉じない: ${result.error.kind}`,
    );
    return;
  }

  const verdict = result.data.verdict;
  if (verdict.verdict !== "Allow") {
    // 委譲先が拒否した → 何もしない（ウィンドウは開いたまま）。`reason` は利用者へ提示する
    // ための材料であり、提示の仕方はこのシェルの画面（9.x）が決める。既定では記録に残す。
    publishDenial({
      window: result.data.context.window,
      reason: verdict.reason,
      // もう一度往復するだけであり、**判定はここで固定しない**（要件 6.7）。
      retry: () => {
        void evaluateCloseRequest();
      },
    });
    return;
  }

  // 許可された → **終了要求を再発火しない方の操作で閉じる**（`destroy()`。冒頭を参照）。
  try {
    await getCurrentWindow().destroy();
  } catch (error: unknown) {
    // 破棄できなかった場合もウィンドウは開いたままである（利用者は再試行できる）。
    console.warn("ウィンドウを破棄できなかった", error);
  }
}

/**
 * このウィンドウの終了要求を購読する。**ウィンドウごとに 1 回だけ呼ぶ。**
 *
 * 呼び出しは起動時（`src/main.tsx`）である。ウィンドウごとに別の JS 実行文脈なので、
 * それぞれの文脈が自分の購読を持つ（`getCurrentWindow()` はその文脈のウィンドウを返す）。
 *
 * **例外を外へ出さない。** 購読の登録に失敗した場合（IPC が無い等）は記録だけして戻る。その
 * とき基盤は「JS リスナが無い」と判断するため、ウィンドウマネージャの終了要求どおり
 * ウィンドウはそのまま閉じる — **閉じられなくなるより安全側である**。
 */
export async function installCloseVeto(): Promise<void> {
  try {
    const window = getCurrentWindow();
    await window.onCloseRequested(async (event) => {
      // **常に拒否を立てる。** これにより基盤側の「自動 destroy」は走らず、閉じるかどうかの
      // 決定は往復の結果だけが決める（拒否のときに何もしないのが要件 2.6 である）。
      event.preventDefault();
      await evaluateCloseRequest();
    });
  } catch (error: unknown) {
    console.error("終了要求の購読を登録できなかった", error);
  }
}

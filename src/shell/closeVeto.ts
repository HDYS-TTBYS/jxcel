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
      // 決定は下の往復の結果だけが決める（拒否のときに何もしないのが要件 2.6 である）。
      event.preventDefault();

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
        // ための材料であり、提示の仕方はこのシェルの画面（9.x）が決める。ここでは記録に残す。
        console.warn(
          `ドキュメント所有者が ${result.data.context.window} の終了を拒否した: ${verdict.reason}`,
        );
        return;
      }

      // 許可された → **終了要求を再発火しない方の操作で閉じる**（`destroy()`。冒頭を参照）。
      try {
        await window.destroy();
      } catch (error: unknown) {
        // 破棄できなかった場合もウィンドウは開いたままである（利用者は再試行できる）。
        console.warn("ウィンドウを破棄できなかった", error);
      }
    });
  } catch (error: unknown) {
    console.error("終了要求の購読を登録できなかった", error);
  }
}

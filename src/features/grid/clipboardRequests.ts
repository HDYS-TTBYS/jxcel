/**
 * メニューの活性化（範囲の複製）を、画面の入口へ引き渡す（tasks.md 8.7。data-grid 要件 7.8）。
 *
 * # 何をするか（**器は登録し、画面は購読する**）
 *
 * メニューの登録口（`src-tauri/src/menu.rs` の `MenuRegistry`）は Rust の口であり、TS の画面から
 * は触れない。**触れないことと、活性化を画面へ届けられないことは別である** — 器は選択を
 * 7.5 の振り向けで解決した対象ウィンドウへ、**Tauri のイベント**として送れる。9.5 の診断の
 * 導線（`src/features/diagnostics/requests.ts`）が同じ形で成立しており、本 module はそれを
 * 写したものである。
 *
 * | 段 | どこ |
 * |---|---|
 * | 項目の登録（`編集 > 複製`。`Ctrl+C` / `Cmd+C`） | `src-tauri/src/commands/grid.rs` の `install` |
 * | 活性化を対象ウィンドウへ送る（`emit_to`） | 同（イベント名は [`GRID_COPY_REQUESTED_EVENT`]） |
 * | 購読して入口を呼ぶ | **本 module** |
 * | 複製そのもの（範囲の決定・表形式テキスト・クリップボード） | `RendererHandle.copySelection`（移植口） |
 *
 * **イベントの本文は読まない。**複製は引数を取らない（対象は「そのとき移植口が持っている選択」
 * である）ので、本文を解釈する余地が無い — 本文が何であっても複製は壊れない
 * （`installDocumentSessionChanged` と同じ判断である。診断の導線だけは本文で分岐するので
 * `parseSection` を持つ）。
 *
 * # 入口は 1 つである（**打鍵と同じ**）
 *
 * [`CopyEntry`] は**移植口の `copySelection` を呼ぶ関数**であり、打鍵（DOM の `copy`）が着く
 * のと同じメソッドである（`glideAdapter.tsx` の `attachCopyKeystroke`）。2 本の経路が同じ
 * 入口を持つので、**範囲の決め方もテキストの作り方も 1 つに閉じる**（別々に計算すると、同じ
 * 選択から別の範囲が出る日が来る）。複製できないときの理由は、打鍵と同じく `RendererSpec.onCopy`
 * の拒否 → 画面の告知として出る（本 module は何も判断しない）。
 *
 * # 失敗を外へ出さない
 *
 * 購読の登録に失敗しても（IPC が無い素のブラウザ等）例外を外へ出さない — 画面はメニュー無しでも
 * 開ける（利用者は打鍵と画面の中の操作をそのまま使える）。`installDiagnosticsRequests` と同じ
 * 2 段（`cancelled` / `unlisten`）で、解除が先に来た場合は登録完了を待ってから解除する。
 */
import { listen } from "@tauri-apps/api/event";

import { GRID_COPY_REQUESTED_EVENT } from "../../ipc/bindings";

/**
 * メニューの活性化を受けて複製を起こす入口。**打鍵が着くのと同じ入口**（移植口の
 * `RendererHandle.copySelection`）を指す。
 *
 * 失敗（拒否）の扱いは呼ぶ側が持つ — 画面は打鍵の面（`attachCopyKeystroke`）と同じく記録だけを
 * 行い、理由は移植口が告知へ出している。
 */
export type CopyEntry = () => void;

/**
 * メニューからの複製の要求を購読する。**表の面（`GridSurface`）のマウント時に 1 回だけ呼ぶ。**
 *
 * 返る関数は購読を解除する（React の `useEffect` の後始末）。1 回の活性化につき [`CopyEntry`] を
 * 1 回呼ぶ（間引かない — 2 回届けば 2 回複製する。内容は同じなので害は無いが、**器が送った回数を
 * そのまま写す**方が、記録と画面の突き合わせが狂わない）。
 */
export function installGridCopyRequests(entry: CopyEntry): () => void {
  let cancelled = false;
  let unlisten: (() => void) | null = null;

  void (async () => {
    try {
      const stop = await listen(GRID_COPY_REQUESTED_EVENT, () => {
        entry();
      });
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    } catch (error: unknown) {
      console.warn("メニューからの複製の要求を購読できなかった", error);
    }
  })();

  return () => {
    cancelled = true;
    unlisten?.();
  };
}

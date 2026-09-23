/**
 * 実行の面の 2 つの購読（tasks.md 4.4。要件 1.3、2.1）。
 *
 * # 1. メニューからの要求（要件 2.1）
 *
 * Rust 側のメニュー項目「マクロ > 実行…」は、選択されると活性化の対象ウィンドウへ
 * [`MACRO_RUN_REQUESTED_EVENT`] を送るだけである（`src-tauri/src/commands/macro.rs` の
 * `install`。項目は有効・無効の述語を持たない — 述語はフォーカスが移動するたびに評価され、
 * その時点で文書を読むと**実行中のマクロのロックを待つ**ためである。要件 2.2）。
 *
 * したがって**実行の導線を出すかどうかを決めるのは面**である（要件 2.7）。本 module は
 * 要求を面へ渡し、**遷移をシェルの 1 つの入口へ委ねる** — 遷移の仕組みは
 * `src/shell/router.tsx` に 1 つだけであり、ここに 2 つ目を作らない（要件 9.2 の規律は
 * 診断の導線と同じである。`src/features/diagnostics/requests.ts`）。
 *
 * 購読の設置は面のマウントより早い必要がある（`src/shell/Layout.tsx` のマウント時） —
 * 別の画面を見ている間に選ばれたメニュー項目を落とさないためである（選ばれた時点で
 * 対象ウィンドウの画面へ遷移し、面が選択を提示する）。
 *
 * # 2. 文書の差し替え（要件 1.3）
 *
 * 一覧は**その文書の中身**である。メニューの「開く…」「新規」は Rust 側で完結するので、
 * 画面は結果を知らない — 適応層が状態を変えた操作のあとに
 * [`DOCUMENT_SESSION_CHANGED_EVENT`] を 1 回送る（`src-tauri/src/session/commands.rs` の
 * `emit_session_changed`）。本 module はそれを購読して**一覧を取り直す**。
 *
 * **本文は読まない。** 通知は「変わった」ことだけを運び、状態そのものを運ばない。状態の源は
 * `macro_list` ただ 1 つである（`src/features/grid/documentRequests.ts` と同じ判断）。
 * 同じイベントをグリッド画面も購読するが、**それぞれが自分の責務のために読む**のであり、
 * 状態の源が 2 つになるわけではない。
 *
 * # 投げない
 *
 * 購読の設置の失敗（IPC が無い素のブラウザ等）を外へ出さない — 面はマクロ無しでも開ける。
 * 登録は非同期なので、解除が先に来た場合は登録の完了を待ってから解除する（`cancelled` の守り。
 * 既存の 2 つの購読と同じ形である）。
 */
import { listen } from "@tauri-apps/api/event";

import { DOCUMENT_SESSION_CHANGED_EVENT, MACRO_RUN_REQUESTED_EVENT } from "../../ipc/bindings";
import type { MacroSurfaceStore } from "./store";

/**
 * メニューからの実行の要求を購読する。**`src/shell/Layout.tsx` のマウント時に 1 回だけ呼ぶ。**
 *
 * `destination` は要求を受けたときに移る画面の識別子である（**本 module は画面の識別子を
 * 知らない** — 面が載っているのはグリッド画面であり、その綴りを知っているのは器である。
 * ここで `src/features/grid` を取り込むと、グリッド画面が本機能のパネルを取り込む向きと
 * 合わせて**相互の取り込み**になる）。
 *
 * 返る関数は購読を解除する（React の `useEffect` の後始末）。
 */
export function installMacroRunRequests(
  store: MacroSurfaceStore,
  navigate: (destination: string) => void,
  destination: string,
): () => void {
  let cancelled = false;
  let unlisten: (() => void) | null = null;

  void (async () => {
    try {
      const stop = await listen(MACRO_RUN_REQUESTED_EVENT, () => {
        // **先に面の状態を動かしてから遷移する**（遷移先の面がマウント時に現在の状態を読むので、
        // 順序が逆だと「選ばせる段」でない一瞬が描かれる。診断の導線と同じ理由である）。
        store.dispatch({ type: "request" });
        navigate(destination);
      });
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    } catch (error: unknown) {
      console.warn("マクロの実行の要求を購読できなかった", error);
    }
  })();

  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/**
 * 文書の差し替え・破棄の通知を購読し、**一覧を取り直す**（要件 1.3）。面のマウント時に呼ぶ。
 *
 * 返る関数は購読を解除する。
 */
export function installMacroListRefresh(store: MacroSurfaceStore): () => void {
  let cancelled = false;
  let unlisten: (() => void) | null = null;

  void (async () => {
    try {
      const stop = await listen(DOCUMENT_SESSION_CHANGED_EVENT, () => {
        store.dispatch({ type: "refresh" });
      });
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    } catch (error: unknown) {
      console.warn("文書の差し替えの通知を購読できなかった", error);
    }
  })();

  return () => {
    cancelled = true;
    unlisten?.();
  };
}

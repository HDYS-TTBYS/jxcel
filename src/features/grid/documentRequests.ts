/**
 * セッションの変化（文書の差し替え・破棄）を、画面の入口へ引き渡す（tasks.md 10.7。data-grid
 * 要件 1.7）。
 *
 * # 何をするか（**器は知らせ、画面は取り直す**）
 *
 * メニューの「開く…」「新規」「保存」は Rust 側で完結するため、画面は結果を知らない。適応層は
 * 状態を変えた操作のあとに、対象ウィンドウへ [`DOCUMENT_SESSION_CHANGED_EVENT`] を 1 回送る
 * （`src/ipc/documentSession.ts` の module doc）。本 module はその通知を購読し、**本文を読まずに**
 * 入口を呼ぶだけである。
 *
 * | 段 | どこ |
 * |---|---|
 * | 状態を変えた操作のあとに対象ウィンドウへ送る（`emit_to`） | `src-tauri`（イベント名は [`DOCUMENT_SESSION_CHANGED_EVENT`]） |
 * | 購読して入口を呼ぶ | **本 module** |
 * | `document_state` を取り直し、提示を組み直す | `./GridScreen`（[`DocumentChangeEntry`] の中身） |
 *
 * # 本文を読まない（**状態の源は 1 つ**）
 *
 * 通知は「変わった」ことだけを運び、状態そのものを運ばない。したがって購読側の仕事は
 * 「`document_state` を取り直す」ことに閉じる — 本文を解釈しないことは最も強い防御であり
 * （本文がどんな値でも取り直しは壊れない）、状態の源が 2 つに割れる余地も残さない。
 * `installDocumentSessionChanged`（`src/ipc/documentSession.ts`）と同じ判断である。
 *
 * **本 module は取り直しそのものを行わない。** 取り直した結果をどう突き合わせ、どう提示を
 * 差し替えるかは画面の状態機械（`./GridScreen` の `gridScreenSessionChanged`）が持ち、本 module は
 * 「通知が届いた」ことだけを渡す。取り直しをここへ入れると、1 つのウィンドウの中に**状態の源を
 * 読む場所が 2 つ**できる（`./gridClient` を通す口が 2 本になる）。
 *
 * # 入口は投げない
 *
 * [`DocumentChangeEntry`] は**イベントハンドラから直接呼ばれる**。`ScreenBoundary` はイベント
 * ハンドラの例外を捕まえないので、入口は自分で投げない（`./GridScreen` 側で握り、失敗は
 * 画面内の告知にする）。本 module も購読の設置の失敗（IPC が無い素のブラウザ等）を外へ出さない
 * — 画面は文書の差し替え無しでも開ける（`installGridCopyRequests` と同じ 2 段である）。
 */
import { listen } from "@tauri-apps/api/event";

import { DOCUMENT_SESSION_CHANGED_EVENT } from "../../ipc/bindings";

/**
 * セッションの変化を受けて、**画面が状態を取り直す**入口。引数を取らないのは、通知が状態を
 * 運ばないためである（状態の源は `document_state` ただ 1 つ）。
 */
export type DocumentChangeEntry = () => void;

/**
 * 文書の差し替え・破棄の通知を購読する。**画面のマウント時に 1 回だけ呼ぶ。**
 *
 * 返る関数は購読を解除する（React の `useEffect` の後始末）。1 回の通知につき [`DocumentChangeEntry`]
 * を 1 回呼ぶ（間引かない — 器が送った回数をそのまま写す方が、記録と画面の突き合わせが狂わない）。
 * 設置は非同期なので、解除が先に来た場合は登録の完了を待ってから解除する（`cancelled` の守り）。
 */
export function installDocumentChangeRequests(onChange: DocumentChangeEntry): () => void {
  let cancelled = false;
  let unlisten: (() => void) | null = null;

  void (async () => {
    try {
      const stop = await listen<unknown>(DOCUMENT_SESSION_CHANGED_EVENT, () => {
        onChange();
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

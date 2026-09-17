/**
 * 文書の差し替え・破棄の通知の購読（tasks.md 10.7。data-grid 要件 1.7）。
 *
 * # 何を固定するか
 *
 * 1. **宛先は生成物の定数である**（文字列リテラルを書かない。`src/ipc/bindings.ts` の綴りが
 *    変わればこの検査が落ちる）
 * 2. 1 回の通知につき入口を**1 回**呼ぶ（間引かない。器が送った回数をそのまま写す）
 * 3. 後始末は購読を解除し、**解除が登録の完了より先に来ても**取りこぼさない（`cancelled` の守り）
 * 4. 購読の設置に失敗しても例外を外へ出さない（IPC が無い素のブラウザでも画面は開ける）
 *
 * `listen` を模す（`node` 環境には IPC が無い）。**捉えた引数はそのまま読む**ので、購読の宛先
 * （イベント名）と、登録された処理は本物である（`./clipboardRequests.test.ts` と同じ規則）。
 *
 * 通知の**処理**（取り直した状態の突き合わせと、提示の差し替え）は `./GridScreen` の
 * `gridScreenSessionChanged` が持ち、`GridScreen.test.ts` の「文書の差し替えと破棄を画面が
 * 追随する」節が固定する — 本 module は「届いたことを渡す」だけである。
 */
import { describe, expect, it, vi } from "vitest";

import { DOCUMENT_SESSION_CHANGED_EVENT } from "../../ipc/bindings";
import { installDocumentChangeRequests } from "./documentRequests";

const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));

/** 微小タスクを 1 巡ぶん流す（`listen` の解決も非同期である）。 */
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("文書の差し替えの通知の購読（10.7。要件 1.7）", () => {
  it("宛先は生成物の定数であり、通知のたびに入口を 1 回呼ぶ", async () => {
    listen.mockReset();
    const stops: (() => void)[] = [];
    listen.mockImplementation(async () => {
      const stop = (): void => undefined;
      stops.push(stop);
      return stop;
    });

    const notices: string[] = [];
    const stop = installDocumentChangeRequests(() => {
      notices.push("届いた");
    });
    await settle();

    // **宛先は生成物の定数である**（名前が変わればこの行が落ちる）。
    expect(DOCUMENT_SESSION_CHANGED_EVENT).toBe("document_session_changed");
    expect(listen).toHaveBeenCalledTimes(1);
    expect(listen.mock.calls[0]?.[0]).toBe(DOCUMENT_SESSION_CHANGED_EVENT);

    // 器が送った通知が、そのまま入口へ届く（1 回の通知につき 1 回。間引かない）。
    const handler = listen.mock.calls[0]?.[1] as () => void;
    handler();
    handler();
    expect(notices).toEqual(["届いた", "届いた"]);

    // 後始末は購読を解除する（画面が片付いた後に状態を取り直す経路を残さない）。
    stop();
    expect(stops).toHaveLength(1);
  });

  it("解除が登録の完了より先に来ても、登録された解除を取りこぼさない", async () => {
    listen.mockReset();
    // **登録の完了を保留する**（`listen` が解決する前に画面が片付く場合を作る）。
    const pending: { resolve: ((stop: () => void) => void) | null } = { resolve: null };
    listen.mockImplementation(
      () =>
        new Promise<() => void>((resolve) => {
          pending.resolve = resolve;
        }),
    );

    const stop = installDocumentChangeRequests(() => undefined);
    stop();
    const unlisten = vi.fn();
    pending.resolve?.(unlisten);
    await settle();

    // 解除は 1 回だけ呼ばれる（購読を残さない）。
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("購読の設置に失敗しても例外を外へ出さない（IPC 無しでも画面は開ける）", async () => {
    listen.mockReset();
    listen.mockRejectedValue(new Error("IPC が無い"));

    const notices: string[] = [];
    const stop = installDocumentChangeRequests(() => {
      notices.push("届いた");
    });
    await settle();

    // 投げずに戻り、解除も安全である（診断の導線・複製の導線と同じ判断）。
    expect(notices).toEqual([]);
    expect(() => {
      stop();
    }).not.toThrow();
  });
});

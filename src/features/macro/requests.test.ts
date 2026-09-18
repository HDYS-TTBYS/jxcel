/**
 * 実行の面の 2 つの購読（tasks.md 4.4。要件 1.3、2.1）。
 *
 * # 何を固定するか
 *
 * 1. **宛先は生成物の定数である**（文字列リテラルを書かない。`src/ipc/bindings.ts` の綴りが
 *    変わればこの検査が落ちる）
 * 2. **メニューの要求は面へ渡り、遷移は器の入口へ委ねられる**（面の状態が「選ばせる段」に
 *    入ってから、器が渡した識別子へ移る — 遷移の仕組みを 2 つ作らない）
 * 3. **文書の差し替えの通知は一覧を取り直す**（要件 1.3。本文を読まない == 状態の源を増やさない）
 * 4. 後始末は購読を解除し、**解除が登録の完了より先に来ても**取りこぼさない
 * 5. 購読の設置に失敗しても例外を外へ出さない（IPC が無い素のブラウザでも画面は開ける）
 *
 * `listen` を模す（`node` 環境には IPC が無い）。**捉えた引数はそのまま読む**ので、購読の宛先と
 * 登録された処理は本物である（`src/features/grid/documentRequests.test.ts` と同じ規則）。
 */
import { describe, expect, it, vi } from "vitest";

import { DOCUMENT_SESSION_CHANGED_EVENT, MACRO_RUN_REQUESTED_EVENT } from "../../ipc/bindings";
import type { IpcClientResult } from "../../ipc/client";
import type { DocumentStateResponse, MacroListResponse } from "../../ipc/bindings";
import type { MacroClient } from "./macroClient";
import { installMacroListRefresh, installMacroRunRequests } from "./requests";
import { createMacroSurfaceStore } from "./store";

const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));

/** 微小タスクを 1 巡ぶん流す（`listen` の解決も非同期である）。 */
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

/** 偽の境界（一覧だけを数える）。**文書は開いている**（一覧を求める側の経路を通す）。 */
function countingClient(calls: string[]): MacroClient {
  const empty: IpcClientResult<MacroListResponse> = {
    status: "ok",
    data: { context: { window: "doc-1" }, macros: [] },
  };
  const opened: IpcClientResult<DocumentStateResponse> = {
    status: "ok",
    data: {
      context: { window: "doc-1" },
      status: {
        state: "Open",
        name: "棚卸し.jxcel",
        origin: "file",
        unsaved: false,
        revision: 1,
        sheets: [],
      },
    },
  };
  return {
    readDocumentState: () => {
      calls.push("document_state");
      return Promise.resolve(opened);
    },
    list: () => {
      calls.push("macro_list");
      return Promise.resolve(empty);
    },
    run: (name) => {
      calls.push(`macro_run:${name}`);
      return Promise.resolve(empty as never);
    },
  };
}

/** `listen` を「登録が即座に完了する」形へ据える（解除の関数を集める）。 */
function installImmediateListen(): (() => void)[] {
  const stops: (() => void)[] = [];
  listen.mockImplementation(async () => {
    const stop = (): void => undefined;
    stops.push(stop);
    return stop;
  });
  return stops;
}

describe("メニューからの実行の要求の購読（要件 2.1）", () => {
  it("要求は面へ渡り、遷移は器の入口へ委ねられる", async () => {
    listen.mockReset();
    installImmediateListen();
    const calls: string[] = [];
    const store = createMacroSurfaceStore(countingClient(calls));
    const destinations: string[] = [];

    const stop = installMacroRunRequests(store, (destination) => {
      destinations.push(destination);
    }, "grid");
    await settle();

    // **宛先は生成物の定数である**（名前が変わればこの行が落ちる）。
    expect(MACRO_RUN_REQUESTED_EVENT).toBe("macro_run_requested");
    expect(listen).toHaveBeenCalledTimes(1);
    expect(listen.mock.calls[0]?.[0]).toBe(MACRO_RUN_REQUESTED_EVENT);
    // 要求の荷は無い（Rust 側は要求の本文を送らない）ので、本文を読まない。
    expect(listen.mock.calls[0]?.[2]).toBeUndefined();

    const handler = listen.mock.calls[0]?.[1] as () => void;
    handler();
    await settle();

    // **面は「選ばせる段」に入り、一覧を取り直している**（遷移先で一覧が出る）。
    expect(store.getState().picking).toBe(true);
    expect(calls).toEqual(["document_state", "macro_list"]);
    // 遷移は器が決める（識別子をこの module は知らない）。
    expect(destinations).toEqual(["grid"]);

    stop();
  });

  it("解除は購読を外し、登録の完了より先に来ても取りこぼさない", async () => {
    listen.mockReset();
    const pending: { resolve: ((stop: () => void) => void) | null } = { resolve: null };
    listen.mockImplementation(
      () =>
        new Promise<() => void>((resolve) => {
          pending.resolve = resolve;
        }),
    );

    const store = createMacroSurfaceStore(countingClient([]));
    const stop = installMacroRunRequests(store, () => undefined, "grid");
    stop();
    const unlisten = vi.fn();
    pending.resolve?.(unlisten);
    await settle();

    // **登録が済んだ後に解除される**（購読が残らない）。
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("購読の設置に失敗しても例外を外へ出さない", async () => {
    listen.mockReset();
    listen.mockImplementation(() => Promise.reject(new Error("IPC が無い")));
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);

    const store = createMacroSurfaceStore(countingClient([]));
    const stop = installMacroRunRequests(store, () => undefined, "grid");
    await settle();

    expect(warn).toHaveBeenCalled();
    // 解除は安全である（登録されていない購読を解除しても何も起きない）。
    stop();
    warn.mockRestore();
  });
});

describe("文書の差し替えの購読（要件 1.3）", () => {
  it("通知のたびに一覧を取り直す（本文は読まない）", async () => {
    listen.mockReset();
    installImmediateListen();
    const calls: string[] = [];
    const store = createMacroSurfaceStore(countingClient(calls));

    const stop = installMacroListRefresh(store);
    await settle();

    expect(DOCUMENT_SESSION_CHANGED_EVENT).toBe("document_session_changed");
    expect(listen).toHaveBeenCalledTimes(1);
    expect(listen.mock.calls[0]?.[0]).toBe(DOCUMENT_SESSION_CHANGED_EVENT);

    const handler = listen.mock.calls[0]?.[1] as (event: { payload: unknown }) => void;
    handler({ payload: { 何か: "想定外の本文" } });
    await settle();
    // **本文を解釈しない**（どんな本文でも取り直しは同じである）。
    expect(calls).toEqual(["document_state", "macro_list"]);

    handler({ payload: null });
    await settle();
    expect(calls).toEqual([
      "document_state",
      "macro_list",
      "document_state",
      "macro_list",
    ]);

    stop();
  });

  it("購読の設置に失敗しても例外を外へ出さない", async () => {
    listen.mockReset();
    listen.mockImplementation(() => Promise.reject(new Error("IPC が無い")));
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);

    const stop = installMacroListRefresh(createMacroSurfaceStore(countingClient([])));
    await settle();

    expect(warn).toHaveBeenCalled();
    stop();
    warn.mockRestore();
  });
});

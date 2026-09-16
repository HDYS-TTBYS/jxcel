/**
 * メニューの活性化による複製（tasks.md 8.7。data-grid 要件 7.8）。
 *
 * # ここで固定するもの（**この 2 つがレビューの指摘への答えである**）
 *
 * 1. **イベント名は生成物から来る。**画面は `src/ipc/bindings.ts` の
 *    [`GRID_COPY_REQUESTED_EVENT`] だけを参照し、文字列リテラルを書かない。名前が食い違えば
 *    **購読が無言で成立しなくなる**（活性化が届かない。エラーにもならない）ので、ここで
 *    購読の宛先そのものを固定する。
 * 2. **打鍵とメニューは同じ入口へ着く。**どちらも移植口の `RendererHandle.copySelection` を
 *    呼び、そこから `RendererSpec.onCopy`（同じ範囲）とクリップボードへの書き込みへ至る。
 *    検査は**書き込まれた文字列**と**移植口へ出た呼び出し**の両方を突き合わせる — 入口が
 *    2 つに分かれれば、範囲の決め方が食い違いうる（`./clipboardRequests` の doc）。
 *
 * # 何を模し、何を模さないか
 *
 * - `@tauri-apps/api/event` の `listen` を**模す**（`node` 環境には IPC が無い）。模すのは
 *   購読の設置だけであり、**イベント名と、登録された処理は本物である**（`vi.mock` が捉えた
 *   引数をそのまま読む）
 * - 器（`HTMLElement`）を**模す**（`addEventListener` を覚えるだけの代役）。`copy` の聴取は
 *   本物の `attachCopyKeystroke` が行う
 * - 移植口は**本物の配線**（`createGlideWiring`）を使い、クリップボードへ渡す口だけを差し替える
 *   （`node` 環境には `document` が無い。`glideAdapter.tsx` の `ClipboardWriter`）
 *
 * **模さないもの**: 実際のメニュー（`src-tauri` の `MenuRegistry` への登録・`emit_to`）と、
 * 実機の打鍵である。段は配布物を要する（`scripts/check-menu-shortcut.sh`。単体テストでは
 * 決着しない）。
 */
import { describe, expect, it, vi } from "vitest";

import { GRID_COPY_REQUESTED_EVENT } from "../../ipc/bindings";
import { createGridCopyEntry } from "./GridScreen";
import { installGridCopyRequests } from "./clipboardRequests";
import { attachCopyKeystroke, createGlideWiring } from "./renderer/glideAdapter";
import {
  arrayRowSource,
  createRendererSpec,
  type PortCall,
  type RecordedCall,
} from "./renderer/interactionDriver";
import type { RendererHandle, RendererSelection } from "./renderer/port";

// `listen` を模す（`node` 環境には IPC が無い）。**捉えた引数はそのまま読む**ので、購読の宛先
// （イベント名）と、登録された処理は本物である。
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));

/** 標本の選択（2×2）。**表示の位置であって、文書の位置ではない**（複製は表示の並びを保つ）。 */
const SELECTION: RendererSelection = {
  current: { row: 1, column: 1 },
  range: { start: { row: 0, column: 0 }, end: { row: 1, column: 1 } },
};

/**
 * クリップボードへ渡す口を差し替えた配線と、移植口へ出た呼び出しの記録を作る。
 *
 * **`mount` が返す取っ手と同じものを返す** — 画面が `handleRef.current` として持つのがこれで
 * あり、メニューの入口（`createGridCopyEntry`）も打鍵の結線（`attachCopyKeystroke`）も、
 * この 1 つを指す。
 */
function wiringWithRecordedClipboard(): {
  readonly handle: RendererHandle;
  readonly written: readonly string[];
  readonly calls: readonly RecordedCall[];
} {
  const written: string[] = [];
  const calls: RecordedCall[] = [];
  const record = (call: PortCall, args: readonly unknown[]): void => {
    calls.push({ call, args });
  };
  const spec = createRendererSpec({
    source: arrayRowSource(),
    record,
    selection: SELECTION,
  });
  return { handle: createGlideWiring(spec, async (text) => void written.push(text)).handle, written, calls };
}

/**
 * 器の代役。**`copy` の聴取者を覚えるだけ**であり、属性は 1 つも持たない（`GlideSurface` が
 * 器へ渡すのは `addEventListener` と `removeEventListener` の 2 つだけである）。
 */
function listeningNode(): {
  readonly node: Pick<HTMLElement, "addEventListener" | "removeEventListener">;
  readonly dispatch: (type: string) => void;
  readonly listeners: readonly string[];
} {
  const handlers = new Map<string, (event: Event) => void>();
  return {
    node: {
      addEventListener: (type: string, listener: EventListenerOrEventListenerObject) => {
        handlers.set(type, listener as (event: Event) => void);
      },
      removeEventListener: (type: string) => {
        handlers.delete(type);
      },
    } as Pick<HTMLElement, "addEventListener" | "removeEventListener">,
    dispatch: (type) => {
      const handler = handlers.get(type);
      if (handler === undefined) {
        throw new Error(`器に ${type} の聴取者が居ない`);
      }
      handler({ preventDefault: () => undefined } as unknown as Event);
    },
    get listeners() {
      return [...handlers.keys()];
    },
  };
}

/** 微小タスクを 1 巡ぶん流す（`copySelection` も `listen` の解決も非同期である）。 */
async function settle(): Promise<void> {
  for (let round = 0; round < 4; round += 1) {
    await Promise.resolve();
  }
}

/** 指定した種類の呼び出しだけを取り出す。 */
function callsNamed(calls: readonly RecordedCall[], call: PortCall): readonly RecordedCall[] {
  return calls.filter((recorded) => recorded.call === call);
}

describe("イベント名は生成物から来る（文字列リテラルを書かない）", () => {
  it("購読は生成物の定数を宛先にし、活性化のたびに入口を 1 回呼ぶ", async () => {
    listen.mockReset();
    const stops: (() => void)[] = [];
    listen.mockImplementation(async () => {
      const stop = (): void => undefined;
      stops.push(stop);
      return stop;
    });

    const entries: string[] = [];
    const stop = installGridCopyRequests(() => {
      entries.push("呼ばれた");
    });
    await settle();

    // **宛先は生成物の定数である**（`src/ipc/bindings.ts`。名前が変わればこの行が落ちる）。
    expect(GRID_COPY_REQUESTED_EVENT).toBe("grid_copy_requested");
    expect(listen).toHaveBeenCalledTimes(1);
    expect(listen.mock.calls[0]?.[0]).toBe(GRID_COPY_REQUESTED_EVENT);

    // 器が送った活性化が、そのまま入口へ届く（1 回の活性化につき 1 回）。
    const handler = listen.mock.calls[0]?.[1] as () => void;
    handler();
    expect(entries).toEqual(["呼ばれた"]);

    // 後始末は購読を解除する（画面が片付いた後に入口を叩く経路を残さない）。
    stop();
    expect(stops).toHaveLength(1);
  });

  it("購読の設置に失敗しても例外を外へ出さない（メニュー無しでも画面は開ける）", async () => {
    listen.mockReset();
    listen.mockRejectedValue(new Error("IPC が無い"));

    const entries: string[] = [];
    const stop = installGridCopyRequests(() => {
      entries.push("呼ばれた");
    });
    await settle();

    // 投げずに戻り、解除も安全である（診断の導線と同じ判断）。
    expect(entries).toEqual([]);
    expect(() => {
      stop();
    }).not.toThrow();
  });
});

describe("打鍵とメニューの活性化は、同じ入口へ着く（要件 7.8）", () => {
  it("どちらも同じ範囲を同じテキストにして、クリップボードへ 1 回ずつ渡す", async () => {
    listen.mockReset();
    listen.mockImplementation(async () => () => undefined);
    const { handle, written, calls } = wiringWithRecordedClipboard();

    // 打鍵の経路（器の DOM の `copy` → 本物の結線 → 入口）。
    const node = listeningNode();
    attachCopyKeystroke(node.node, handle);
    expect(node.listeners).toEqual(["copy"]);
    node.dispatch("copy");
    await settle();

    // メニューの経路（器のイベント → 購読 → **画面が組む入口** → 同じ取っ手）。
    const stop = installGridCopyRequests(createGridCopyEntry(() => handle));
    await settle();
    (listen.mock.calls[0]?.[1] as () => void)();
    await settle();
    stop();

    // **同じ文字列が 2 回**（同じ選択 → 同じ範囲 → 同じ表形式テキスト）。
    expect(written).toEqual(["0:0\t0:1\n1:0\t1:1", "0:0\t0:1\n1:0\t1:1"]);
    // **同じ範囲が 2 回**移植口の `onCopy` へ出る（入口が 2 つに分かれていないこと）。
    expect(callsNamed(calls, "onCopy")).toEqual([
      { call: "onCopy", args: [SELECTION.range] },
      { call: "onCopy", args: [SELECTION.range] },
    ]);
  });

  it("器がまだ無いときのメニューの活性化は、何もしない（投げない）", async () => {
    listen.mockReset();
    listen.mockImplementation(async () => () => undefined);
    const entry = createGridCopyEntry(() => null);

    expect(() => {
      entry();
    }).not.toThrow();
  });
});

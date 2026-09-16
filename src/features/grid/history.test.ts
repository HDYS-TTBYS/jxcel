/**
 * 取り消しとやり直しの 1 往復（tasks.md 8.9。data-grid 要件 9.2、9.3、9.8、9.9）。
 *
 * # ここで固定するもの
 *
 * 1. **向きはそのまま境界へ行く。**`grid_history` の要求は生成物の閉じた列挙
 *    （`GridHistoryDirection`）を運ぶ — 画面は「取り消し」と「やり直し」を**種別で分けない**
 *    （分けると、経路が 2 つになり片方だけが後始末を行う日が来る）。
 * 2. **適用のあとの作り直しは 8.3 / 8.6 / 8.7 と同じ 1 つである**（要件 1.7）。影響を受けた行が
 *    あれば `WindowCache.clear(row_count)` を**応答の行数で**呼ぶ（`invalidate` では、行数が
 *    変わったときに後ろの窓が別の行を指したまま残る）。
 * 3. **移動先の解決は捨てる前に済ませる**（要件 9.8）。`invalidate` / `clear` は影響を受けた
 *    行の窓そのものを捨てるので、**後に引けば答えは必ず `null`** になる。ここが本 module で
 *    最も壊れやすい順序であり、偽の記憶がその性質を写している（`droppingCache`）。
 * 4. **進める履歴が無いことは失敗ではない**（生成物の `GridEditResponse.outcome` の doc）。
 *    何も動かさない（作り直しも移動もしない）。
 * 5. **メニューの 2 つの項目は同じ 1 つの入口へ着く。**イベント名は生成物の定数であり、
 *    荷は向きだけである（解釈できない荷は無言で捨てる — 投げない）。
 *
 * # 何を模し、何を模さないか
 *
 * - `@tauri-apps/api/event` の `listen` を**模す**（`node` 環境には IPC が無い）。模すのは購読の
 *   設置だけであり、**イベント名と、登録された処理は本物である**（`vi.mock` が捉えた引数を
 *   そのまま読む。`clipboardRequests.test.ts` と同じ形）
 * - 境界の口と窓の記憶は**偽物**である（要る口だけを持つ）。**模さないもの**: 実機の打鍵
 *   （アクセラレータの配信）と、実際のメニューの活性化 — 段は配布物を要する
 *   （`scripts/check-menu-shortcut.sh`。単体テストでは決着しない）
 */
import { describe, expect, it, vi } from "vitest";

import { GRID_HISTORY_REQUESTED_EVENT } from "../../ipc/bindings";
import type {
  GridEditOutcome,
  GridEditResponse,
  GridHistoryDirection,
  IpcResult,
} from "../../ipc/bindings";
import type { IpcClientError } from "../../ipc/client";
import type { GridClient } from "./gridClient";
import { applyHistory, installGridHistoryRequests, parseHistoryDirection } from "./history";
import { initialSelection, selectionForKey } from "./selection";
import type { CellPosition } from "./renderer/port";

// `listen` を模す（`node` 環境には IPC が無い）。**捉えた引数はそのまま読む**ので、購読の宛先
// （イベント名）と、登録された処理は本物である。
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));

/** 窓の文脈（`WindowContext`。値そのものは検査に効かない）。 */
const CONTEXT = { window: "main" } as const;

/** 影響を受けた行の識別子（正準の 26 文字）。 */
const EDITED_ROW = "01ARZ3NDEKTSV4RRFFQ69G5FB0";
const RESTORED_ROW = "01ARZ3NDEKTSV4RRFFQ69G5FB1";

/** 適用の結果（指定した欄だけを変えて組む）。 */
function outcomeOf(overrides: Partial<GridEditOutcome>): GridEditOutcome {
  return {
    affected: [],
    coercions: [],
    violation_total: 0,
    violations: [],
    revalidated_columns: [],
    row_count: 4,
    ...overrides,
  };
}

/**
 * 成功の封筒（**世代は応答が運ぶ**。タスク 10.1。既定は「1 つ進んだ後」に当たる値である）。
 */
function ok(
  outcome: GridEditOutcome | null,
  generation = "2",
): IpcResult<GridEditResponse, IpcClientError> {
  return { status: "ok", data: { context: CONTEXT, outcome, generation } };
}

/** 失敗の封筒。 */
function failure(): IpcResult<GridEditResponse, IpcClientError> {
  return {
    status: "error",
    error: { kind: "Document", detail: { message: "経路が不達である" } },
  };
}

/**
 * 偽の境界。**使われてはならない口は例外を投げ、`readHistory` は向きを記録してから応答する。**
 *
 * 取り消しとやり直しが**同じ 1 つの口**を通ることは、この形で初めて固定できる（口を 2 つに
 * 分ければ、どちらかが「呼ばれない」ことで落ちる）。
 */
function fakeClient(answer: () => IpcResult<GridEditResponse, IpcClientError>): GridClient & {
  readonly directions: readonly GridHistoryDirection[];
} {
  const directions: GridHistoryDirection[] = [];
  const unused = (name: string) => (): never => {
    throw new Error(`履歴の経路は ${name} を呼んではならない`);
  };
  return {
    directions,
    readDocumentState: async () => unused("document_state")(),
    openSheet: async () => unused("grid_open_sheet")(),
    setView: async () => unused("grid_set_view")(),
    readWindow: async () => unused("grid_rows_window")(),
    applyEdit: async () => unused("grid_apply_edit")(),
    findViolation: async () => unused("grid_find_violation")(),
    readHistory: async (direction) => {
      directions.push(direction);
      return answer();
    },
  };
}

/**
 * 偽の窓の記憶。**捨てると行の対応が消える**（`invalidate` / `clear` が影響を受けた行の窓を
 * 捨てるという事実を写したもの）。
 *
 * この性質があるので、「捨てる前に移動先を引く」という順序が**落とせる** — 順序を入れ替えれば
 * `applyHistory` の `affectedRow` は `null` になる（`windowCache.test.ts` の `ordinalOf` の
 * 検査と対になる。あちらは本物の記憶が答えることを見る）。
 */
function droppingCache(options: {
  /** 行の識別子 → 表示の序数（**いま記憶が保っている行**）。 */
  readonly held: ReadonlyMap<string, number>;
}): {
  readonly cache: {
    clear: (rowCount?: number) => void;
    ordinalOf: (rowId: string) => number | null;
  };
  readonly cleared: readonly (number | undefined)[];
  readonly asked: readonly string[];
} {
  const cleared: (number | undefined)[] = [];
  const asked: string[] = [];
  const held = new Map(options.held);
  return {
    cleared,
    asked,
    cache: {
      clear: (rowCount?: number) => {
        cleared.push(rowCount);
        // **捨てる。**以後はどの行も引けない（行数が変わる編集では序数と行の対応そのものが
        // 変わるので、記憶は全部を捨てる）。
        held.clear();
      },
      ordinalOf: (rowId: string) => {
        asked.push(rowId);
        for (const [key, ordinal] of held) {
          if (key.toUpperCase() === rowId.toUpperCase()) {
            return ordinal;
          }
        }
        return null;
      },
    },
  };
}

// ===========================================================================
// 1. 1 往復（要件 9.2、9.3）
// ===========================================================================

describe("履歴を 1 つ進める（要件 9.2、9.3）", () => {
  it("向きをそのまま境界へ渡す（取り消しとやり直しは同じ 1 つの口を通る）", async () => {
    const client = fakeClient(() => ok(outcomeOf({})));
    const memory = droppingCache({ held: new Map() });

    expect((await applyHistory({ client, cache: memory.cache, direction: "undo" })).status).toBe(
      "applied",
    );
    expect((await applyHistory({ client, cache: memory.cache, direction: "redo" })).status).toBe(
      "applied",
    );

    // **2 つの指示が 1 つの口へ来る**（口を分ければ、どちらかが記録に現れない）。
    expect(client.directions).toEqual(["undo", "redo"]);
  });

  it("適用のあとは、応答の行数で記憶を作り直す（要件 1.7）", async () => {
    // **行数が変わる取り消し**（行の追加の取り消し ＝ 4 行へ戻る）。
    const client = fakeClient(() =>
      ok(outcomeOf({ affected: [RESTORED_ROW], row_count: 4 })),
    );
    const memory = droppingCache({ held: new Map([[RESTORED_ROW, 6]]) });

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "undo" });

    expect(settlement.status).toBe("applied");
    // **応答が運んだ数である**（画面は数え直さない。`clear` へ渡さなければ、増えた行は永久に
    // 読み込み中のままになり、減った先は古い窓のまま配られる）。
    expect(memory.cleared).toEqual([4]);
  });

  it("影響を受けた行が無ければ記憶を触らない（何も変わっていない）", async () => {
    const client = fakeClient(() => ok(outcomeOf({})));
    const memory = droppingCache({ held: new Map([[EDITED_ROW, 2]]) });

    await applyHistory({ client, cache: memory.cache, direction: "undo" });

    expect(memory.cleared).toEqual([]);
  });

  it("進める履歴が無いことは失敗ではない（何も動かさない）", async () => {
    const client = fakeClient(() => ok(null));
    const memory = droppingCache({ held: new Map([[EDITED_ROW, 2]]) });

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "undo" });

    expect(settlement).toEqual({ status: "empty" });
    // 文書が変わっていないので、作り直す理由も移動する理由も無い。
    expect(memory.cleared).toEqual([]);
    expect(memory.asked).toEqual([]);
  });

  it("経路が失敗したら、理由を 1 行にして記憶を触らない", async () => {
    const client = fakeClient(failure);
    const memory = droppingCache({ held: new Map([[EDITED_ROW, 2]]) });

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "redo" });

    expect(settlement.status).toBe("failed");
    expect(memory.cleared).toEqual([]);
  });
});

// ===========================================================================
// 2. 移動先の解決（要件 9.8）
// ===========================================================================

describe("対象となった範囲へ現在位置を移す（要件 9.8）", () => {
  it("影響を受けた行の表示の序数を、**捨てる前に**引く", async () => {
    const client = fakeClient(() =>
      ok(outcomeOf({ affected: [EDITED_ROW], row_count: 12 })),
    );
    const memory = droppingCache({ held: new Map([[EDITED_ROW, 7]]) });

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "undo" });

    // **順序が本質である**: 作り直し（`clear`）の後に引けば、記憶はもうその行を持たない。
    expect(settlement).toEqual({
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 12 }),
      affectedRow: 7,
      // **世代も応答が運ぶ**（タスク 10.1。画面は数え直さない）。
      generation: "2",
    });
    expect(memory.cleared).toEqual([12]);
  });

  it("見つかった最初の行を採る（影響を受けた並びの順である）", async () => {
    const client = fakeClient(() =>
      ok(outcomeOf({ affected: [RESTORED_ROW, EDITED_ROW] })),
    );
    // 2 つ目だけが記憶にある（1 つ目は削除された行である）。
    const memory = droppingCache({ held: new Map([[EDITED_ROW, 3]]) });

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "undo" });

    expect(settlement.status === "applied" && settlement.affectedRow).toBe(3);
  });

  it("序数が引けない行へは動かさない（推測しない）", async () => {
    // 削除の取り消しがこれに当たる: 戻ってくる行の識別子は、削除の時点の `clear` で記憶から
    // 消えている（**境界は行の識別子しか運ばない**ので、序数を求める手段が無い）。
    const client = fakeClient(() =>
      ok(outcomeOf({ affected: [RESTORED_ROW], row_count: 6 })),
    );
    const memory = droppingCache({ held: new Map([[EDITED_ROW, 3]]) });

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "undo" });

    expect(settlement.status === "applied" && settlement.affectedRow).toBeNull();
    // 作り直しは行う（値は戻っている。位置だけが動かない）。
    expect(memory.cleared).toEqual([6]);
  });
});

// ===========================================================================
// 3. メニューの活性化（要件 9.9）
// ===========================================================================

describe("メニューの活性化を 1 つの入口へ渡す（要件 9.9）", () => {
  /** 購読を設置し、捉えた処理へ荷を注ぐ道具。 */
  function installed(): {
    readonly entry: readonly GridHistoryDirection[];
    readonly fire: (payload: unknown) => void;
    readonly stop: () => void;
    readonly eventName: () => unknown;
  } {
    const entry: GridHistoryDirection[] = [];
    let handler: ((event: { payload: unknown }) => void) | null = null;
    const stop = vi.fn();
    listen.mockClear();
    listen.mockImplementation((_name: string, callback: (event: { payload: unknown }) => void) => {
      handler = callback;
      return Promise.resolve(stop);
    });
    const unsubscribe = installGridHistoryRequests((direction) => {
      entry.push(direction);
    });
    return {
      entry,
      fire: (payload) => {
        handler?.({ payload });
      },
      stop: unsubscribe,
      eventName: () => listen.mock.calls[0]?.[0],
    };
  }

  it("イベント名は生成物の定数である（文字列リテラルを書かない）", async () => {
    const tool = installed();
    await Promise.resolve();
    expect(tool.eventName()).toBe(GRID_HISTORY_REQUESTED_EVENT);
    expect(GRID_HISTORY_REQUESTED_EVENT).toBe("grid_history_requested");
    tool.stop();
  });

  it("2 つの項目（取り消し・やり直し）が**同じ 1 つの入口**へ着く", async () => {
    const tool = installed();
    await Promise.resolve();

    tool.fire({ direction: "undo" });
    tool.fire({ direction: "redo" });

    expect(tool.entry).toEqual(["undo", "redo"]);
    tool.stop();
  });

  it("解釈できない荷は捨てる（投げない）", async () => {
    const tool = installed();
    await Promise.resolve();

    // 荷が無い・形が違う・閉じた列挙の外である（`forward` は違反の探索の向きである）。
    tool.fire(undefined);
    tool.fire(null);
    tool.fire({});
    tool.fire({ direction: "forward" });
    tool.fire("undo");

    expect(tool.entry).toEqual([]);
    tool.stop();
  });

  it("解除は購読の取り消しを呼ぶ（登録の完了を待ってからでも）", async () => {
    const tool = installed();
    tool.stop();
    await Promise.resolve();
    await Promise.resolve();
    expect(listen).toHaveBeenCalledTimes(1);
    tool.stop();
  });

  it("閉じた列挙の外は解釈しない（`parseHistoryDirection` の全体）", () => {
    expect(parseHistoryDirection({ direction: "undo" })).toBe("undo");
    expect(parseHistoryDirection({ direction: "redo" })).toBe("redo");
    expect(parseHistoryDirection({ direction: "Undo" })).toBeNull();
    expect(parseHistoryDirection({ direction: 1 })).toBeNull();
    expect(parseHistoryDirection([])).toBeNull();
    expect(parseHistoryDirection(null)).toBeNull();
  });
});

// ===========================================================================
// 4. 位置の型が表示の位置であること（**取り違えの固定**）
// ===========================================================================
describe("解決した序数は表示の位置である", () => {
  it("序数（可視行の添字）をそのまま返す（文書の位置ではない）", async () => {
    const client = fakeClient(() =>
      ok(outcomeOf({ affected: [EDITED_ROW], row_count: 20 })),
    );
    const memory = droppingCache({ held: new Map([[EDITED_ROW, 0]]) });

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "undo" });

    // 0 は**先頭の可視行**である（1 行目）。画面はこの値をそのまま現在位置へ入れる
    // （`GridScreen.test.ts` の「対象となった範囲へ現在位置を移す」がその使い方を見る）。
    expect(settlement.status === "applied" && settlement.affectedRow).toBe(0);
    const position: CellPosition = { row: 0, column: 0 };
    expect(position.row).toBe(0);
  });
});

// ===========================================================================
// 5. アクセラレータが奪う経路が無いこと（要件 9.9）
// ===========================================================================

/**
 * **キーボードの経路はアクセラレータであり、画面はその打鍵を扱わない。**
 *
 * 8.7 は貼り付けの項目を**登録しなかった**（`Ctrl+V` を登録すると、基盤のメニューが打鍵を先に
 * 受け取り、動いている DOM の `paste` が届かなくなるためである）。取り消しとやり直しは
 * **逆向きの事情**にある — 打鍵を扱う経路が画面に 1 つも無いので、アクセラレータは何も
 * 奪わない。この検査はその前提を**振る舞いで**固定する（源の走査では、打鍵の表が別の名前に
 * 移ったときに追随できない）。
 */
describe("アクセラレータが奪う経路が無いこと（要件 9.9）", () => {
  it("画面の打鍵の引き受けは、取り消し・やり直しの打鍵を 1 つも飲まない", () => {
    const bounds = { rowCount: 20, columnCount: 3 };
    const selection = initialSelection();
    // 押されうる 4 通り（非 macOS の `Ctrl+Z` / `Ctrl+Shift+Z` と、macOS の `Cmd+Z` /
    // `Cmd+Shift+Z`）。
    for (const stroke of [
      { key: "z", ctrlKey: true, shiftKey: false, altKey: false, metaKey: false },
      { key: "z", ctrlKey: true, shiftKey: true, altKey: false, metaKey: false },
      { key: "z", ctrlKey: false, shiftKey: false, altKey: false, metaKey: true },
      { key: "z", ctrlKey: false, shiftKey: true, altKey: false, metaKey: true },
    ]) {
      expect(selectionForKey(stroke, selection, bounds)).toBeNull();
    }
  });
});

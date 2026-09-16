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
 * 3. **移動先は窓の記憶から引かない**（要件 9.8。10.5 が境界へ移した）。序数は応答が運び
 *    （`GridEditOutcome.affected_ordinals`）、現在位置を移すのは表の遷移である
 *    （`./GridScreen` の `appliedRowOperation`）。したがって本 module の依存は `clear` だけで
 *    あり、偽の記憶は `ordinalOf` を**持たない**（持てば型が通り、呼べば実行時に落ちる）。
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

// `listen` を模す（`node` 環境には IPC が無い）。**捉えた引数はそのまま読む**ので、購読の宛先
// （イベント名）と、登録された処理は本物である。
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));

/** 窓の文脈（`WindowContext`。値そのものは検査に効かない）。 */
const CONTEXT = { window: "main" } as const;

/** 影響を受けた行の識別子（正準の 26 文字）。**窓の記憶が保っていない行**である（10.5）。 */
const RESTORED_ROW = "01ARZ3NDEKTSV4RRFFQ69G5FB1";

/** 2 つ目の影響を受けた行（**並びが 2 件以上のとき、本 module はどれも取り出さない**）。 */
const EDITED_ROW = "01ARZ3NDEKTSV4RRFFQ69G5FB2";

/** 適用の結果（指定した欄だけを変えて組む）。 */
function outcomeOf(overrides: Partial<GridEditOutcome>): GridEditOutcome {
  return {
    affected: [],
    affected_ordinals: [],
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
    readReferenceRows: unused("readReferenceRows"),
    readHistory: async (direction) => {
      directions.push(direction);
      return answer();
    },
  };
}

/**
 * 偽の窓の記憶。**行数の作り直し（`clear`）だけを受ける。**
 *
 * `ordinalOf` を持たないことが本検査の 1 つである — 本 module は移動先を窓の記憶から**引かない**
 * （序数は応答が運ぶ。10.5）。あれば型が通ってしまい、経路が戻ってきたことに気づけない。
 */
function clearingCache(): {
  readonly cache: { clear: (rowCount?: number) => void };
  readonly cleared: readonly (number | undefined)[];
} {
  const cleared: (number | undefined)[] = [];
  return {
    cleared,
    cache: {
      clear: (rowCount?: number) => {
        cleared.push(rowCount);
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
    const memory = clearingCache();

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
    const memory = clearingCache();

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "undo" });

    expect(settlement.status).toBe("applied");
    // **応答が運んだ数である**（画面は数え直さない。`clear` へ渡さなければ、増えた行は永久に
    // 読み込み中のままになり、減った先は古い窓のまま配られる）。
    expect(memory.cleared).toEqual([4]);
  });

  it("影響を受けた行が無ければ記憶を触らない（何も変わっていない）", async () => {
    const client = fakeClient(() => ok(outcomeOf({})));
    const memory = clearingCache();

    await applyHistory({ client, cache: memory.cache, direction: "undo" });

    expect(memory.cleared).toEqual([]);
  });

  it("進める履歴が無いことは失敗ではない（何も動かさない）", async () => {
    const client = fakeClient(() => ok(null));
    const memory = clearingCache();

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "undo" });

    expect(settlement).toEqual({ status: "empty" });
    // 文書が変わっていないので、作り直す理由も移動する理由も無い。
    expect(memory.cleared).toEqual([]);
  });

  it("経路が失敗したら、理由を 1 行にして記憶を触らない", async () => {
    const client = fakeClient(failure);
    const memory = clearingCache();

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "redo" });

    expect(settlement.status).toBe("failed");
    expect(memory.cleared).toEqual([]);
  });
});

// ===========================================================================
// 2. 移動先は窓の記憶から引かない（要件 9.8。10.5）
// ===========================================================================

/**
 * **移動先の序数は応答が運ぶ**（`GridEditOutcome.affected_ordinals`。10.5 が境界へ移した）。
 * したがって本 module は序数を 1 つも解決せず、窓の記憶に問い合わせもしない — 10.5 より前は
 * `WindowCache.ordinalOf` で `affected` の行を引いており、**記憶が保っていない行**（行の追加の
 * やり直しで戻ってくる行）では答えが `null` になっていた（8.9 のレビューが実測した最小の再現）。
 *
 * ここで固定できるのは**依存の形**である（偽の記憶は `ordinalOf` を持たず、本 module の `cache`
 * の型も `clear` だけを要求する）。**現在位置が実際に移ること**は、表を描く状態の遷移が
 * 応答の序数を使うことそのものであり、`GridScreen.test.ts` の
 * 「行の追加のやり直しでも、現在位置が対象の行へ移り、追随が走る（要件 9.8）」が
 * `applyHistory` → `gridScreenHistorySettled` → `followSelection` の本物の経路で観測する。
 */
describe("移動先の序数は応答が運ぶ（要件 9.8）", () => {
  it("記憶の対応が無くても往復は成立し、序数は応答のまま画面へ渡る", async () => {
    // **`affected_ordinals` に載っている行は、記憶が 1 つも保っていない行である**
    // （行の追加のやり直しがこれに当たる）。
    const outcome = outcomeOf({
      affected: [RESTORED_ROW, EDITED_ROW],
      affected_ordinals: [6, 2],
      row_count: 6,
    });
    const client = fakeClient(() => ok(outcome));
    const memory = clearingCache();

    const settlement = await applyHistory({ client, cache: memory.cache, direction: "redo" });

    // 記憶から引く経路が無いので、順序（捨てる前に引く）も依存も無い — 作り直しだけを行う。
    expect(settlement.status).toBe("applied");
    if (settlement.status !== "applied") {
      throw new Error("適用の腕でなければならない");
    }
    // **影響を受けた並びは 1 つも落ちず、順も変わらない** — 本 module は序数を 1 つも取り出さない
    // （先頭を採るのは表の遷移である。10.5）。1 件だけを見る検査では、ここで `[0]` を取り出す
    // 実装も、昇順に並べ直す実装も通ってしまう。
    expect(settlement.outcome.affected_ordinals).toEqual([6, 2]);
    expect(settlement.outcome.affected).toEqual([RESTORED_ROW, EDITED_ROW]);
    expect(settlement.generation).toBe("2");
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
// 4. アクセラレータが奪う経路が無いこと（要件 9.9）
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

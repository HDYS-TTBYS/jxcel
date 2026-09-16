/**
 * 参照先の行を頁ごとに読むこと（タスク 10.3。data-grid 要件 3.8、11 の目的）。
 *
 * # 何を固定するか
 *
 * 偽物は**境界へ届く要求を控える台本の口**だけであり、読み込みの経路は本物である
 * （`./referenceRows`）。固定するのは次の 3 点である。
 *
 * 1. **1 回の要求が頁の大きさを超えない**（参照先が 1 万行でも一度に全部を読まない）
 * 2. **続きは読んだ行の数だけ進める**（頁が重ならない）／`hasMore` が偽になったら止まる
 * 3. **失敗は空の一覧と混同しない**（読めなかったことを状態として持つ）
 *
 * 境界の側の上限（要求を切る）は `src-tauri` の検査が固定する — ここで固定するのは**画面が
 * 大きすぎる要求を送らないこと**である。
 */
import { describe, expect, it } from "vitest";

import type { GridReferenceResponse } from "../../ipc/bindings";
import type { IpcClientResult } from "../../ipc/client";
import type { GridClient } from "./gridClient";
import { REFERENCE_PAGE_SIZE, loadReferenceRows, type ReferenceRows } from "./referenceRows";

/** 要求を控えるだけの口（**本物の頁の切り出しは境界が持つ**）。 */
function scripted(answer: (request: { start: number; count: number }) => IpcClientResult<GridReferenceResponse>): {
  readonly client: GridClient;
  readonly requests: readonly { readonly start: number; readonly count: number }[];
} {
  const requests: { start: number; count: number }[] = [];
  const unused = (name: string) => (): never => {
    throw new Error(`参照先の読み込みは ${name} を呼んではならない`);
  };
  const client: GridClient = {
    readDocumentState: unused("readDocumentState"),
    openSheet: unused("openSheet"),
    setView: unused("setView"),
    readWindow: unused("readWindow"),
    applyEdit: unused("applyEdit"),
    findViolation: unused("findViolation"),
    readHistory: unused("readHistory"),
    readReferenceRows: (request) => {
      requests.push({ start: request.start, count: request.count });
      return Promise.resolve(answer({ start: request.start, count: request.count }));
    },
  };

  return { client, requests };
}

/** 1 万行の参照先のうち `start` から `count` 行を返す（境界の応答の形そのものである）。 */
function pageOf(start: number, count: number, total = 10_000): IpcClientResult<GridReferenceResponse> {
  const rows = Array.from({ length: Math.max(0, Math.min(count, total - start)) }, (_, offset) => ({
    id: `01K4ANRRG004HMASW9NF6YY${String(start + offset).padStart(3, "0")}`,
    label: `仕入先${String(start + offset)}`,
  }));

  return {
    status: "ok",
    data: { context: { window: "main" }, rows, total, has_more: start + rows.length < total },
  };
}

/** 読了の状態から行の数を読む（型の上で `loaded` に閉じる）。 */
function loadedRows(state: ReferenceRows): number {
  return state.state === "loaded" ? state.rows.length : -1;
}

describe("参照先の行を頁ごとに読む（タスク 10.3。要件 3.8）", () => {
  it("1 回の要求は頁の大きさを超えず、続きは読んだ行の数だけ進める", async () => {
    const { client, requests } = scripted(({ start, count }) => pageOf(start, count));

    const first = await loadReferenceRows(client, 2, { state: "loading" });
    expect(loadedRows(first)).toBe(REFERENCE_PAGE_SIZE);
    expect(first.state === "loaded" && first.total).toBe(10_000);
    expect(first.state === "loaded" && first.hasMore).toBe(true);

    const second = await loadReferenceRows(client, 2, first);
    expect(loadedRows(second), "前の頁の後ろへ繋がる").toBe(REFERENCE_PAGE_SIZE * 2);

    // **大きすぎる要求を送っていない**（1 万行を一度に読む要求が 1 つも無い）。
    expect(requests).toEqual([
      { start: 0, count: REFERENCE_PAGE_SIZE },
      { start: REFERENCE_PAGE_SIZE, count: REFERENCE_PAGE_SIZE },
    ]);
    // 頁は重ならず、先頭からの並びである。
    const rows = second.state === "loaded" ? second.rows : [];
    expect(rows[0]?.label).toBe("仕入先0");
    expect(rows[REFERENCE_PAGE_SIZE]?.label).toBe(`仕入先${String(REFERENCE_PAGE_SIZE)}`);
  });

  it("末尾に達すると `hasMore` が偽になり、それ以上読まない", async () => {
    const { client, requests } = scripted(({ start, count }) => pageOf(start, count, 150));

    const first = await loadReferenceRows(client, 0, { state: "loading" });
    expect(first.state === "loaded" && first.hasMore).toBe(true);
    const second = await loadReferenceRows(client, 0, first);

    expect(loadedRows(second)).toBe(150);
    expect(second.state === "loaded" && second.hasMore).toBe(false);
    expect(requests).toHaveLength(2);
    expect(requests[1]).toEqual({ start: REFERENCE_PAGE_SIZE, count: REFERENCE_PAGE_SIZE });
  });

  it("読めなかったことは、空の一覧と混同しない", async () => {
    const { client } = scripted(() => ({
      status: "error",
      error: { kind: "Document", detail: { message: "シート 仕入先 が文書に無い" } },
    }));

    const failed = await loadReferenceRows(client, 0, { state: "loading" });

    expect(failed.state).toBe("failed");
    expect(failed.state === "failed" && failed.message).toContain("が文書に無い");
  });

  it("読了の状態は、行の識別子と表示の名を運ぶ（境界の形の写しである）", async () => {
    const { client } = scripted(({ start, count }) =>
      pageOf(start, count, 2),
    );

    const loaded = await loadReferenceRows(client, 1, { state: "loading" });
    expect(loaded.state === "loaded" && loaded.rows).toEqual([
      { id: "01K4ANRRG004HMASW9NF6YY000", label: "仕入先0" },
      { id: "01K4ANRRG004HMASW9NF6YY001", label: "仕入先1" },
    ]);
  });
});

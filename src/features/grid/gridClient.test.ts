/**
 * グリッド画面の境界の口（tasks.md 8.1。`./gridClient`）の**引数の形**を固定する。
 *
 * # なぜこれが要るのか（**起動の観測で見つかった実物の誤り**）
 *
 * Tauri が縛る payload の鍵は**コマンドの引数の名前**である。`grid_open_sheet` の実体は
 * `fn grid_open_sheet(app, window, request: GridOpenRequest)`（`src-tauri/src/commands/grid.rs`）
 * なので、要求は `{ request: { sheet } }` で渡さなければならない。`{ sheet }` を直接渡すと
 * **コマンドへ届く前に復号が失敗し**、封筒の失敗（`status: "error"`）ではなく `invoke` の拒否と
 * して現れる — 画面はそれを「シートを開けない」として見せるので、**症状から原因が読めない**
 * （8.1 の起動観測で実際にこれが出た。生バイト経路の引数を包む誤りも同じ性質である:
 * 包むと Tauri が数値の配列へ変換し、**例外を投げずに**空の窓を返す）。
 *
 * したがって本 file は、**`invoke` へ渡される綴りと引数の形そのもの**を固定する。偽の境界を
 * 渡す `GridScreen.test.ts` はこの層を飛ばすので、この誤りはここでしか捕まらない。
 *
 * 環境は `node` であり、`@tauri-apps/api/core` の `invoke` を差し替える（実物の IPC は無い）。
 */
import { describe, expect, it, vi } from "vitest";

/** 差し替える `invoke`（**`vi.mock` より先に作る**ので `vi.hoisted` を使う）。 */
const invoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { EMPTY_GRID_VIEW, createGridClient } from "./gridClient";
import type { GridOpenResponse } from "../../ipc/bindings";

/** シートを開いた応答（値そのものは検査に効かない）。 */
const OPEN_RESPONSE: { status: "ok"; data: GridOpenResponse } = {
  status: "ok",
  data: {
    context: { window: "main" },
    // 世代（タスク 10.1。開いた直後は `Generation::FIRST`）。
    generation: "0",
    sheet: {
      columns: [
        { column: 0, path: [], name: "名前", kind: "Text", element_count: null, expandability: "leaf" },
      ],
      row_count: 0,
    },
  },
};

describe("境界の口の引数の形", () => {
  it("シートを開く要求は `request` という名前の引数で包む", async () => {
    invoke.mockResolvedValue(OPEN_RESPONSE);
    const client = createGridClient();

    const result = await client.openSheet("01ARZ3NDEKTSV4RRFFQ69G5FB0");

    expect(invoke).toHaveBeenCalledWith("grid_open_sheet", {
      request: { sheet: "01ARZ3NDEKTSV4RRFFQ69G5FB0" },
    });
    expect(result).toEqual(OPEN_RESPONSE);
  });

  it("表示の指定も `request` という名前の引数で包む", async () => {
    invoke.mockResolvedValue({
      status: "ok",
      data: {
        context: { window: "main" },
        visible_rows: 0,
        hidden_rows: 0,
        violation_total: 0,
        columns: [],
      },
    });
    const client = createGridClient();

    await client.setView(EMPTY_GRID_VIEW);

    // 空の指定（絞り込み無し・並べ替え無し・展開無し）をそのまま渡す。
    expect(invoke).toHaveBeenCalledWith("grid_set_view", {
      request: { view: { sort: [], filters: [], expansion: [] } },
    });
  });

  it("生バイトの窓の要求は、バッファを包まずにそのまま渡す", async () => {
    const buffer = new Uint8Array([1, 2, 3]);
    invoke.mockResolvedValue(new ArrayBuffer(0));
    const client = createGridClient();

    await client.readWindow(buffer);

    // **入れ子にしない**（`{ argument: buffer }` にすると経路の意味が失われる。7.3 の
    // `defaultTransport` と同じ規律である）。
    expect(invoke).toHaveBeenCalledWith("grid_rows_window", buffer);
  });

  it("セッションの状態は引数なしで問い合わせる", async () => {
    invoke.mockResolvedValue({ status: "ok", data: { context: { window: "main" }, status: { state: "Absent" } } });
    const client = createGridClient();

    await client.readDocumentState();

    expect(invoke).toHaveBeenCalledWith("document_state", undefined);
  });

  it("編集命令も `request` という名前の引数で包む（1 セルの `SetCells`）", async () => {
    invoke.mockResolvedValue({
      status: "ok",
      data: {
        context: { window: "main" },
        outcome: {
          affected: [],
          coercions: [],
          violation_total: 0,
          violations: [],
          revalidated_columns: [],
          row_count: 0,
        },
      },
    });
    const client = createGridClient();

    await client.applyEdit({
      command: "SetCells",
      cells: [{ cell: { row: "01ARZ3NDEKTSV4RRFFQ69G5FB0", column: 2 }, text: "12.50" }],
    });

    // **宛先は文書の位置である**（行の識別子と列の添字。可視行の序数ではない）。
    // 包みを落とすと、コマンドへ届く前に復号が失敗し、封筒ではなく拒否として現れる。
    expect(invoke).toHaveBeenCalledWith("grid_apply_edit", {
      request: {
        command: {
          command: "SetCells",
          cells: [{ cell: { row: "01ARZ3NDEKTSV4RRFFQ69G5FB0", column: 2 }, text: "12.50" }],
        },
      },
    });
  });

  it("違反の探索も `request` という名前の引数で包む（起点と向き）", async () => {
    invoke.mockResolvedValue({
      status: "ok",
      data: { context: { window: "main" }, violation: null },
    });
    const client = createGridClient();

    const result = await client.findViolation({ from: 3, direction: "forward" });

    // 起点は**可視行の序数**であり、向きは生成物の綴り（`"forward"` / `"backward"`）である。
    expect(invoke).toHaveBeenCalledWith("grid_find_violation", {
      request: { from: 3, direction: "forward" },
    });
    expect(result).toEqual({
      status: "ok",
      data: { context: { window: "main" }, violation: null },
    });
  });

  it("履歴も `request` という名前の引数で包む（向きは生成物の閉じた列挙）", async () => {
    invoke.mockResolvedValue({
      status: "ok",
      data: {
        context: { window: "main" },
        outcome: {
          affected: [],
          coercions: [],
          violation_total: 0,
          violations: [],
          revalidated_columns: [],
          row_count: 0,
        },
      },
    });
    const client = createGridClient();

    await client.readHistory("undo");

    // **取り消しとやり直しは同じ 1 つのコマンドである**（向きは要求が運ぶ）。
    expect(invoke).toHaveBeenCalledWith("grid_history", { request: { direction: "undo" } });
    await client.readHistory("redo");
    expect(invoke).toHaveBeenLastCalledWith("grid_history", {
      request: { direction: "redo" },
    });
  });

  it("`invoke` の拒否は封筒の失敗として返る（画面はそれを失敗として扱える）", async () => {
    invoke.mockRejectedValue(new Error("コマンドが許可されていない"));
    const client = createGridClient();

    const result = await client.openSheet("s1");

    expect(result.status).toBe("error");
    if (result.status !== "error") {
      throw new Error("封筒の失敗にならなかった");
    }
    expect(result.error.kind).toBe("Frontend");
  });

  it("生バイト経路の拒否はそのまま伝わる（窓の記憶が未取得のまま再試行できる）", async () => {
    invoke.mockRejectedValue(new Error("経路が不達である"));
    const client = createGridClient();

    // **握らない。** 7.3 の窓の記憶は拒否を受けて未取得のまま残し、次の引きで再試行する。
    await expect(client.readWindow(new Uint8Array([0]))).rejects.toThrow("経路が不達である");
  });
});

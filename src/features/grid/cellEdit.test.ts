/**
 * セル編集の 1 往復（tasks.md 8.3。data-grid 要件 3.3、3.4、3.5、3.6、3.7、1.7）。
 *
 * # ここで固定するもの
 *
 * 1. **確定が境界へ送るもの** — 現在位置のセルと打たれた文字を `SetCells` の 1 件として送ること
 *    （要件 3.3）。宛先は**文書の行の識別子と列の添字**である（可視行の序数ではない）
 * 2. **取消が何も送らないこと**（要件 3.6）。偽の境界は**使われてはならない口が例外を投げ**、
 *    `applyEdit` は記録だけして結果を返す（呼ばれたことは `edits` の空/非空で分かる）ので、
 *    1 つでも呼べばこの検査は落ちる
 * 3. **値なしは空の文字列として送ること**（要件 3.7。`edited_value` がそれを `Null` へ写す）
 * 4. **影響を受けた行の窓を捨てること**（要件 1.7。`EditOutcome.affected` → 7.3 の `invalidate`）
 * 5. **適合しない値も、打たれた文字のまま送ること**（要件 3.5。捨てるかどうかを決めるのは
 *    判定であり本 module ではない）
 *
 * # ここで固定しないもの（**正直に書く**）
 *
 * - **値が文書に残ること**は Rust 側の契約である（`src-tauri/src/commands/grid.rs` の
 *   `answer_apply_edit` の docs「適合しない値も破棄せず違反として返す」。`WriteOrigin::Edit` は
 *   決して拒否しない）— 本 module は**送った文字が書き換わらないこと**までしか主張しない
 * - **入力手段が実際に現れ、打鍵が届くこと**は `node` の環境（DOM なし）では観測できない。
 *   実起動（9.2）と 8.1 の起動観測の領分である
 */
import { describe, expect, it } from "vitest";

import type {
  GridEditCommand,
  GridEditOutcome,
  GridEditResponse,
  IpcResult,
} from "../../ipc/bindings";
import type { IpcClientError } from "../../ipc/client";
import type { GridClient } from "./gridClient";
import { settleCellEdit } from "./cellEdit";
import type { CellPosition } from "./renderer/port";

// ===========================================================================
// 道具（偽の境界と、偽の窓の記憶）
// ===========================================================================

/** 窓の文脈（`WindowContext`。値そのものは検査に効かない）。 */
const CONTEXT = { window: "main" } as const;

/** 編集した行の識別子（正準の 26 文字。`rowKeyText` が作る綴りと同じ長さである）。 */
const ROW_ID = "01ARZ3NDEKTSV4RRFFQ69G5FB0";

/** 型強制の起きた応答（変換の前後が食い違う。提示が前の値を出すことは `GridScreen.test.ts`）。 */
function outcomeOf(overrides: Partial<GridEditOutcome>): GridEditOutcome {
  return {
    affected: [],
    coercions: [],
    violation_total: 0,
    violations: [],
    revalidated_columns: [],
    row_count: 3,
    ...overrides,
  };
}

/** 成功の封筒（応答の `outcome` は適用ではつねに `Some` である — 生成物の docs）。 */
function applied(outcome: GridEditOutcome): IpcResult<GridEditResponse, IpcClientError> {
  return { status: "ok", data: { context: CONTEXT, outcome } };
}

/** 失敗の封筒（経路の不達）。 */
const FAILURE: IpcResult<GridEditResponse, IpcClientError> = {
  status: "error",
  error: { kind: "Document", detail: { message: "経路が不達である" } },
};

/**
 * 偽の境界。**使われてはならない口は例外を投げ、`applyEdit` は記録してから応答する。**
 *
 * 取消（要件 3.6）と、行の識別子が未取得のときの確定が**本当に何も送らない**ことは、この形で
 * 初めて固定できる — 数えるだけでは「呼ばれたが数え忘れた」経路を見逃す。
 */
interface FakeClient extends GridClient {
  readonly edits: readonly GridEditCommand[];
}

function fakeClient(answer: IpcResult<GridEditResponse, IpcClientError>): FakeClient {
  const edits: GridEditCommand[] = [];
  const unused = (name: string) => (): never => {
    throw new Error(`セル編集の経路は ${name} を呼んではならない`);
  };
  return {
    edits,
    readDocumentState: async () => unused("document_state")(),
    openSheet: async () => unused("grid_open_sheet")(),
    setView: async () => unused("grid_set_view")(),
    readWindow: async () => unused("grid_rows_window")(),
    applyEdit: async (command) => {
      edits.push(command);
      return answer;
    },
  };
}

/** 偽の窓の記憶（要る 2 つの口だけ。**識別子が引けない場合は `null` を返す**）。 */
function fakeCache(rowId: string | null): {
  readonly rowId: (position: CellPosition) => string | null;
  readonly invalidate: (affected: readonly string[]) => void;
  readonly invalidated: readonly (readonly string[])[];
  readonly asked: readonly CellPosition[];
} {
  const invalidated: (readonly string[])[] = [];
  const asked: CellPosition[] = [];
  return {
    invalidated,
    asked,
    rowId: (position) => {
      asked.push(position);
      return rowId;
    },
    invalidate: (affected) => {
      invalidated.push([...affected]);
    },
  };
}

/** 第 5 行・第 3 列（0 起点）のセル。 */
const POSITION: CellPosition = { row: 4, column: 2 };

// ===========================================================================
// 1. 確定（要件 3.3、3.7）
// ===========================================================================

describe("確定が境界へ送るもの（要件 3.3）", () => {
  it("現在位置のセルと打たれた文字を、SetCells の 1 件として送る", async () => {
    const client = fakeClient(applied(outcomeOf({ affected: [ROW_ID], revalidated_columns: [2] })));
    const cache = fakeCache(ROW_ID);

    const settlement = await settleCellEdit({
      client,
      cache,
      position: POSITION,
      intent: { kind: "commit", text: "12.50" },
    });

    // **宛先は文書の位置である**（行の識別子と列の添字。可視行の序数ではない）。
    expect(client.edits).toEqual([
      { command: "SetCells", cells: [{ cell: { row: ROW_ID, column: 2 }, text: "12.50" }] },
    ]);
    expect(settlement.status).toBe("applied");
    // 打たれた文字は 1 文字も変わらない（解釈するのは `schema-engine` である）。
    expect(cache.asked).toEqual([POSITION]);
  });

  it("値なし（空の文字列）も、そのままの文字として送る（要件 3.7）", async () => {
    const client = fakeClient(applied(outcomeOf({ affected: [ROW_ID] })));
    const cache = fakeCache(ROW_ID);

    await settleCellEdit({
      client,
      cache,
      position: POSITION,
      intent: { kind: "commit", text: "" },
    });

    // **空の文字列が値なしである**（`crates/data-grid/src/edit/mod.rs` の `edited_value` が
    // `Null` へ写す）。本 module は「値なし」という別の表現を作らない。
    expect(client.edits).toEqual([
      { command: "SetCells", cells: [{ cell: { row: ROW_ID, column: 2 }, text: "" }] },
    ]);
  });

  it("適合しない値を打っても、捨てずに打たれた文字のまま送る（要件 3.5）", async () => {
    // 判定は Rust 側が行う（`WriteOrigin::Edit` は決して拒否しない）。本 module が決めるのは
    // 「送るか送らないか」だけであり、**適合しないという理由で送らない経路を作らない**。
    const client = fakeClient(
      applied(
        outcomeOf({
          affected: [ROW_ID],
          violation_total: 1,
          violations: [{ row: ROW_ID, column: 2, path: [] }],
          revalidated_columns: [2],
        }),
      ),
    );
    const cache = fakeCache(ROW_ID);

    const settlement = await settleCellEdit({
      client,
      cache,
      position: POSITION,
      intent: { kind: "commit", text: "存在しない名前" },
    });

    expect(client.edits).toEqual([
      {
        command: "SetCells",
        cells: [{ cell: { row: ROW_ID, column: 2 }, text: "存在しない名前" }],
      },
    ]);
    expect(settlement.status).toBe("applied");
  });
});

// ===========================================================================
// 2. 取消（要件 3.6）
// ===========================================================================

describe("取消（要件 3.6）", () => {
  it("取消では、境界へ何も送らない", async () => {
    // 偽の境界は**使われてはならない口が例外を投げ**、`applyEdit` は記録する。取消が 1 つでも
    // 呼べば（送る口でも送らない口でも）この検査は落ちる
    // （数えるだけの検査では「呼ばれたが数え忘れた」経路を見逃す）。
    const client = fakeClient(applied(outcomeOf({})));
    const cache = fakeCache(ROW_ID);

    const settlement = await settleCellEdit({
      client,
      cache,
      position: POSITION,
      intent: { kind: "cancel" },
    });

    expect(settlement).toEqual({ status: "cancelled" });
    expect(client.edits).toEqual([]);
    // **行の識別子も引かない**（文書を触らないので宛先が要らない）。
    expect(cache.asked).toEqual([]);
    expect(cache.invalidated).toEqual([]);
  });
});

// ===========================================================================
// 3. 適用のあとの表示の作り直し（要件 1.7）
// ===========================================================================

describe("適用のあとの表示の作り直し（要件 1.7）", () => {
  it("影響を受けた行の窓を捨てる（`EditOutcome.affected` をそのまま渡す）", async () => {
    const second = "01ARZ3NDEKTSV4RRFFQ69G5FB1";
    const client = fakeClient(applied(outcomeOf({ affected: [ROW_ID, second] })));
    const cache = fakeCache(ROW_ID);

    await settleCellEdit({
      client,
      cache,
      position: POSITION,
      intent: { kind: "commit", text: "1" },
    });

    // 窓を捨てれば 7.3 の記憶が取り直し、到着の通知が `RendererHandle.invalidate` を呼ぶ
    // （8.1 が結線した `onArrival`）。**順序もそのまま**である（重複を畳んだ命令の順）。
    expect(cache.invalidated).toEqual([[ROW_ID, second]]);
  });

  it("適用できなかったときは、何も捨てない（表示は変わっていない）", async () => {
    const client = fakeClient(FAILURE);
    const cache = fakeCache(ROW_ID);

    const settlement = await settleCellEdit({
      client,
      cache,
      position: POSITION,
      intent: { kind: "commit", text: "1" },
    });

    // 失敗の理由は 1 行へ写る（**見せ方を決めるのは画面である**。design.md「Error Handling」）。
    expect(settlement).toEqual({
      status: "failed",
      message: "ドキュメントの失敗: 経路が不達である",
    });
    expect(cache.invalidated).toEqual([]);
  });

  it("宛先の行の識別子がまだ届いていないときは、送らずに理由を返す", async () => {
    const client = fakeClient(applied(outcomeOf({})));
    const cache = fakeCache(null);

    const settlement = await settleCellEdit({
      client,
      cache,
      position: POSITION,
      intent: { kind: "commit", text: "1" },
    });

    // 窓がまだ届いていない行（読み込み中）は、**文書の位置を名指しできない**。推測で別の行へ
    // 書くより、送らない方が正しい（要件 8.6 の「取り違えると別の行を編集する」）。
    expect(settlement.status).toBe("failed");
    if (settlement.status !== "failed") {
      throw new Error("失敗として返らなかった");
    }
    expect(settlement.message).toContain("識別子");
    expect(client.edits).toEqual([]);
    expect(cache.invalidated).toEqual([]);
  });
});

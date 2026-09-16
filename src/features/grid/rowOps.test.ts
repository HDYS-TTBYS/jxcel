/**
 * 行の追加・削除・複製の 1 往復（tasks.md 8.6。data-grid 要件 6.1、6.2、6.3、6.5、1.7）。
 *
 * # ここで固定するもの
 *
 * 1. **送る命令の形**（要件 6.1、6.2、6.3）。追加は `InsertRows`（**錨と数だけを運び、値を
 *    運ばない** — 既定値は宣言が供給する）、削除は `RemoveRows`、複製は `DuplicateRows`（どちらも
 *    **可視の序数の区間**で対象を決める）
 * 2. **座標空間は可視の序数である**（タスク 10.4。要件 8.6 の取り違え）。削除・複製の対象は
 *    `{ target: "Ordinals", from, count }` であり、挿入の位置は `{ anchor: "Before", ordinal }`
 *    （末尾は `{ anchor: "End" }`）である。**文書の位置も行の識別子も送らない** — 写すのは
 *    ドメイン（可視の並びを持つ側）であり、画面が写しを持てば 2 つの写しが食い違う
 * 3. **削除の確認**（要件 6.5）。閾値は**いま 1 画面に見えている行数**である。それを超えるときは
 *    削除する行数を示して確認を求める。**確認への取り消しは境界へ何も送らない** — 計画の腕と、
 *    腕を振り分ける関数（`runRowOperationPlan`）の両方で固定する
 * 4. **行数が変わったあとの記憶の作り直し**（要件 1.7）。`WindowCache.clear(row_count)` を
 *    **両方向**で固定する — 増えた行は読め、減った先は配られず要求もされない
 * 5. 適用できなかったときは**記憶を捨てない**（何も変わっていないので、取り直す理由が無い）
 *
 * # ここで固定しないもの（**正直に書く**）
 *
 * - **既定値の中身**は宣言（`CompiledSchema::default_row`）が決める（`crates/data-grid` の検査）。
 *   本 module は**値を作らないこと**までしか主張しない
 * - **実際に文書へ行が足され・消えること**は Rust 側の契約であり、実物の起動（9.2）で観測する
 *   （本 module は「何を送ったか」までしか主張しない）
 * - **確認の面が実際に現れ、押下が届くこと**は `node` の環境（DOM なし）では観測できない
 *   （9.2 と `smoke-port-probe` の領分である）
 */
import { describe, expect, it } from "vitest";

import type {
  ColumnDescriptor,
  GridEditCommand,
  GridEditOutcome,
  GridEditResponse,
  IpcResult,
} from "../../ipc/bindings";
import type { IpcClientError } from "../../ipc/client";
import type { GridClient } from "./gridClient";
import { createColumnSpace } from "./columnSpace";
import { createWindowCache, type WindowCache } from "./windowCache";
import {
  applyRowOperation,
  deleteNeedsConfirmation,
  planRowOperation,
  rowTargets,
  runRowOperationPlan,
  type RowOperationContext,
  type RowOperationPlan,
  type RowSendIntent,
} from "./rowOps";
import type { RendererSelection } from "./renderer/port";

// ===========================================================================
// 道具（偽の境界・偽の窓の記憶・偽の窓のサーバ）
// ===========================================================================

/** 窓の文脈（`WindowContext`。値そのものは検査に効かない）。 */
const CONTEXT = { window: "main" } as const;

/** 境界の失敗（経路の不達）。 */
const FAILURE: IpcClientError = { kind: "Document", detail: { message: "経路が不達である" } };

/**
 * 行の識別子（**可視の序数からは作れない綴りである**）。
 *
 * 序数から機械的に作れる綴りを使うと、「識別子で対象を決めた」ことと「序数で決めた」ことが
 * 同じ結果になり、取り違えを捕まえられない。先頭の 5 件は固定ファイルと同じ綴りである
 * （`crates/data-grid/tests/fixtures/window_protocol.txt` のヘッダが名指す 26 文字）。
 */
const ROW_IDS: readonly string[] = [
  "01ARZ3NDEKTSV4RRFFQ69G5FAW",
  "01ARZ3NDEKTSV4RRFFQ69G5FAX",
  "01ARZ3NDEKTSV4RRFFQ69G5FAY",
  "01ARZ3NDEKTSV4RRFFQ69G5FAZ",
  "01ARZ3NDEKTSV4RRFFQ69G5FB0",
];

/** 計画の文脈（既定は「可視行 20・1 画面 10 行」）。 */
function contextOf(overrides: Partial<RowOperationContext> = {}): RowOperationContext {
  return {
    visibleRows: 20,
    viewportRows: 10,
    ...overrides,
  };
}

/** 適用の結果（指定した欄だけを変えて組む）。 */
function outcomeOf(overrides: Partial<GridEditOutcome>): GridEditOutcome {
  return {
    affected: [],
    affected_ordinals: [],
    coercions: [],
    violation_total: 0,
    violations: [],
    revalidated_columns: [],
    row_count: 20,
    ...overrides,
  };
}

/**
 * 成功の封筒。**世代も応答が運ぶ**（タスク 10.1。既定は「1 つ進んだ後」に当たる値である）。
 */
function applied(
  outcome: GridEditOutcome,
  generation = "2",
): IpcResult<GridEditResponse, IpcClientError> {
  return { status: "ok", data: { context: CONTEXT, outcome, generation } };
}

/**
 * 偽の境界。**使われてはならない口は例外を投げ、`applyEdit` は記録してから応答する。**
 *
 * 送らない経路（取り消し・識別子が引けない・位置を写せない）が**本当に何も送らない**ことは、
 * この形で初めて固定できる — 数えるだけでは「呼ばれたが数え忘れた」経路を見逃す。
 */
interface FakeClient extends GridClient {
  readonly edits: readonly GridEditCommand[];
}

function fakeClient(answer: IpcResult<GridEditResponse, IpcClientError>): FakeClient {
  const edits: GridEditCommand[] = [];
  const unused = (name: string) => (): never => {
    throw new Error(`行の操作の経路は ${name} を呼んではならない`);
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
    findViolation: async () => unused("grid_find_violation")(),
    readReferenceRows: unused("readReferenceRows"),
    readHistory: async () => unused("grid_history")(),
  };
}

/** 偽の記憶（`clear` だけ。**渡された行数を記録する**）。 */
function fakeClear(): {
  readonly cache: Pick<WindowCache, "clear">;
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

/** 1 つの計画を、記録するだけの受け口へ振り分ける（**どの腕へ届いたか**を読む）。 */
function planOf(plan: RowOperationPlan): {
  readonly reached: readonly string[];
  readonly sent: readonly RowSendIntent[];
} {
  const reached: string[] = [];
  const sent: RowSendIntent[] = [];
  runRowOperationPlan(plan, {
    send: (intent) => {
      sent.push(intent);
    },
    confirm: (confirmation) => {
      reached.push(`confirm:${String(confirmation.count)}`);
    },
    cancel: () => {
      reached.push("cancel");
    },
  });
  return { reached, sent };
}

/** 計画の送る腕（1 つだけのはずである。無ければ投げて、検査の意図を読めるようにする）。 */
function sentOf(plan: RowOperationPlan): RowSendIntent {
  const intent = planOf(plan).sent[0];
  if (intent === undefined) {
    throw new Error("送る腕が組まれなかった");
  }
  return intent;
}

// ---------------------------------------------------------------------------
// 窓のサーバ（7.3 の記憶を**実物のまま**組み、`clear` の両方向を見るための偽物）
// ---------------------------------------------------------------------------

/** wire の値を位置ごとに書く（`crates/data-grid/src/transport/mod.rs` の表が唯一の源である）。 */
function u64(value: number): Uint8Array {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(value), true);
  return bytes;
}

/** 行の鍵の生バイト（最終バイトを序数で進める。26 文字の綴りを作るのは記憶である）。 */
function keyFor(row: number): Uint8Array {
  const key = new Uint8Array(16);
  key.set([
    0x01, 0x56, 0x3e, 0x3a, 0xb5, 0xd3, 0xd6, 0x76, 0x4c, 0x61, 0xef, 0xb9, 0x93, 0x02, 0xbd, 0x5c,
  ]);
  key[15] = (key[15] ?? 0) + row;
  return key;
}

/**
 * 偽のサーバが返す窓のバイト列（**形式の真ではない** — 形式は `windowCache.test.ts` の
 * 1・2 が本物の符号化器のバイト列で固定している。ここは記憶の作り直しを読むための相手である）。
 */
function windowBytes(start: number, count: number, generation: number): ArrayBuffer {
  const encoder = new TextEncoder();
  const body: Uint8Array[] = [];
  for (let row = start; row < start + count; row += 1) {
    const text = encoder.encode(`${String(row)}:0`);
    body.push(keyFor(row));
    body.push(new Uint8Array([5, 0]));
    body.push(u64(text.byteLength), text);
  }
  const bytes = new Uint8Array(33 + body.reduce((total, part) => total + part.byteLength, 0));
  const view = new DataView(bytes.buffer);
  bytes[0] = 1;
  view.setBigUint64(1, BigInt(generation), true);
  view.setBigUint64(9, BigInt(start), true);
  view.setBigUint64(17, BigInt(count), true);
  view.setBigUint64(25, BigInt(1), true);
  let at = 33;
  for (const part of body) {
    bytes.set(part, at);
    at += part.byteLength;
  }
  return bytes.buffer;
}

/** 要求の頭から読んだ欄（**検査の側の独立な読み手**）。 */
interface RecordedRequest {
  readonly generation: number;
  readonly start: number;
  readonly count: number;
}

/** 移送の記録と、要求どおりの行を返す偽のサーバ。 */
function windowServer(): {
  readonly calls: readonly RecordedRequest[];
  readonly readWindow: (argument: Uint8Array) => Promise<ArrayBuffer>;
} {
  const calls: RecordedRequest[] = [];
  return {
    calls,
    readWindow: (argument) => {
      const view = new DataView(argument.buffer, argument.byteOffset, argument.byteLength);
      const request: RecordedRequest = {
        generation: Number(view.getBigUint64(1, true)),
        start: Number(view.getBigUint64(9, true)),
        count: Number(view.getBigUint64(17, true)),
      };
      calls.push(request);
      return Promise.resolve(windowBytes(request.start, request.count, request.generation));
    },
  };
}

/** 移送の約束を記憶へ反映させる（**実時間ではなく微小タスクを進める**）。 */
async function settle(): Promise<void> {
  for (let tick = 0; tick < 4; tick += 1) {
    await Promise.resolve();
  }
}

/** 列 1 本の構成（記憶の検査では列そのものが主題ではない）。 */
const ONE_COLUMN: readonly ColumnDescriptor[] = [
  { column: 0, path: [], name: "名前", kind: "Text", element_count: null, expandability: "leaf", nullable: true, choices: [], reference_sheet: null, custom_type_id: null, members: [] },
];

// ===========================================================================
// 1. 挿入は位置と数だけを送る（要件 6.1）
// ===========================================================================

describe("挿入は位置と数だけを送る（要件 6.1）", () => {
  it("追加は `InsertRows` を 1 つ送る（位置と数の 2 つだけであり、**値は運ばない**）", async () => {
    const outcome = outcomeOf({ affected: [ROW_IDS[2] ?? ""], row_count: 21 });
    const client = fakeClient(applied(outcome));
    const memory = fakeClear();

    const settlement = await applyRowOperation({
      client,
      cache: memory.cache,
      intent: sentOf(planRowOperation({ kind: "insert", at: 3 }, contextOf())),
    });

    expect(client.edits).toEqual([
      { command: "InsertRows", at: { anchor: "Before", ordinal: 3 }, count: 1 },
    ]);
    // **運ぶ欄はこの 3 つだけである**（値の欄が増えればここが落ちる。既定値は宣言が供給する
    // — `CompiledSchema::default_row`）。
    expect(Object.keys(client.edits[0] ?? {}).sort()).toEqual(["at", "command", "count"]);
    // 適用のあとは記憶を**新しい行数で**作り直す（既定値は取り直した窓が運ぶ）。
    expect(memory.cleared).toEqual([21]);
    // **世代も応答が運ぶ**（タスク 10.1。画面は数え直さない）。
    expect(settlement).toEqual({ status: "applied", outcome, generation: "2" });
  });

  it("挿入の位置は**可視の序数**で送る（文書の位置へ写さない。要件 8.6）", () => {
    // 起点は**可視の序数 3** である。その行の文書の位置が 3 である保証は無い（並べ替え・
    // 絞り込みが効いていれば食い違う）が、**写すのはドメイン**なので、画面は序数をそのまま
    // 送る。文書の位置を送れば、利用者が指したのとは別の行の隣に入る。
    expect(sentOf(planRowOperation({ kind: "insert", at: 3 }, contextOf()))).toEqual({
      kind: "insert",
      anchor: { kind: "before", ordinal: 3 },
    });
  });

  it("挿入の位置は可視行の数までに収める（末尾への追加は妥当である）", () => {
    const at = (row: number, visibleRows: number): RowSendIntent =>
      sentOf(planRowOperation({ kind: "insert", at: row }, contextOf({ visibleRows })));
    // 末尾の次の位置（`at == 可視行数`）への追加は**文書の末尾**への追加である
    // （`edit` 層の事前検査と同じ。可視の最後の行の直後とは限らない）。
    expect(at(20, 20)).toEqual({ kind: "insert", anchor: { kind: "end" } });
    // 表の外を指す指定は**末尾へ寄せる**（負の値は先頭へ）。
    expect(at(24, 20)).toEqual({ kind: "insert", anchor: { kind: "end" } });
    expect(at(-1, 20)).toEqual({ kind: "insert", anchor: { kind: "before", ordinal: 0 } });
  });
});

// ===========================================================================
// 2. 削除と複製は行の識別子で対象を決める（要件 6.2、6.3）
// ===========================================================================

describe("削除と複製は可視の序数の区間で対象を決める（要件 6.2、6.3）", () => {
  it("削除は選択の可視の序数の区間を送る（文書の位置でも識別子でもない）", async () => {
    const client = fakeClient(applied(outcomeOf({ affected: [ROW_IDS[1] ?? ""], row_count: 19 })));
    const memory = fakeClear();

    await applyRowOperation({
      client,
      cache: memory.cache,
      intent: sentOf(
        planRowOperation({ kind: "delete", targets: { first: 1, last: 3, count: 3 } }, contextOf()),
      ),
    });

    // **可視の序数の区間そのものである。**文書の位置へ解くのはドメインであり、画面は
    // 識別子を 1 つも引き集めない（窓の記憶に依らない）。
    expect(client.edits).toEqual([
      { command: "RemoveRows", target: { target: "Ordinals", from: 1, count: 3 } },
    ]);
    expect(memory.cleared).toEqual([19]);
  });

  it("複製は同じ可視の序数の区間を送る（値はドメインが写す）", async () => {
    const client = fakeClient(applied(outcomeOf({ affected: [ROW_IDS[2] ?? ""], row_count: 21 })));
    const memory = fakeClear();

    await applyRowOperation({
      client,
      cache: memory.cache,
      intent: sentOf(
        planRowOperation(
          { kind: "duplicate", targets: { first: 2, last: 2, count: 1 } },
          contextOf(),
        ),
      ),
    });

    expect(client.edits).toEqual([
      { command: "DuplicateRows", target: { target: "Ordinals", from: 2, count: 1 } },
    ]);
    expect(memory.cleared).toEqual([21]);
  });

  it("窓の記憶が保っていない行を含む選択でも、そのまま送る（識別子を引き集めない）", () => {
    // 以前は窓の記憶から識別子を引き集めていたため、**保っていない行が 1 つでもあれば操作
    // そのものができなかった**（10 万行で 1 画面に収まらない範囲を選ぶと起きる）。
    // 序数で指せば、保っているかどうかに関わりなく選択の全体が送られる。
    const plan = planOf(
      planRowOperation(
        { kind: "delete", targets: { first: 0, last: 99_999, count: 100_000 } },
        contextOf({ visibleRows: 100_000, viewportRows: 100_000 }),
      ),
    );
    expect(plan.reached).toEqual([]);
    expect(plan.sent).toEqual([
      { kind: "delete", range: { from: 0, count: 100_000 } },
    ]);
  });

  it("選択の行が表の外へ出ていれば、その分は対象にしない", () => {
    // 行 18..21 の選択（表は 20 行）。**在るのは 18・19 だけである。**
    const beyond: RendererSelection = {
      current: { row: 21, column: 0 },
      range: { start: { row: 18, column: 0 }, end: { row: 21, column: 0 } },
    };
    expect(rowTargets(beyond, 20)).toEqual({ first: 18, last: 19, count: 2 });
    // 行が 1 つも無ければ対象も無い（消す行が無いので、確認も送信も起こさない）。
    expect(rowTargets(beyond, 0)).toBeNull();
    // 現在位置が右下でも、範囲は対角で読む。
    const dragged: RendererSelection = {
      current: { row: 4, column: 0 },
      range: { start: { row: 6, column: 0 }, end: { row: 4, column: 0 } },
    };
    expect(rowTargets(dragged, 20)).toEqual({ first: 4, last: 6, count: 3 });
  });

  it("対象が無ければ、送る腕へも確認の腕へも載らない", () => {
    expect(
      planOf(planRowOperation({ kind: "delete", targets: { first: 0, last: 0, count: 0 } }, contextOf())),
    ).toEqual({ reached: [], sent: [] });
  });
});

// ===========================================================================
// 3. 削除の確認（要件 6.5）
// ===========================================================================

describe("削除の確認は、1 画面に見えている行数を閾値にする（要件 6.5）", () => {
  it("閾値は「いま 1 画面に見えている行数」である", () => {
    // 1 画面に 10 行見えているとき、10 行までは 1 画面に収まる。
    expect(deleteNeedsConfirmation(10, 10)).toBe(false);
    expect(deleteNeedsConfirmation(11, 10)).toBe(true);
    // **見えている行数を知らないうちは確認を求める**（収まると言えないためである。押下が 1 回
    // 増えるだけであり、尋ねずに消した行は戻せない — 取り違えの向きが逆である）。
    expect(deleteNeedsConfirmation(1, null)).toBe(true);
  });

  it("1 画面に収まらない数のときは、削除する行数を示して確認を求める（送らない）", () => {
    const plan = planOf(
      planRowOperation(
        { kind: "delete", targets: { first: 0, last: 11, count: 12 } },
        contextOf({ viewportRows: 10 }),
      ),
    );
    // **送る腕へは載らない**（確認が答えるまでは何も起きない）。
    expect(plan.sent).toEqual([]);
    expect(plan.reached).toEqual(["confirm:12"]);
  });

  it("1 画面に収まる数なら確認を求めず、そのまま送る", () => {
    const plan = planOf(
      planRowOperation(
        { kind: "delete", targets: { first: 0, last: 4, count: 5 } },
        contextOf({ viewportRows: 10 }),
      ),
    );
    expect(plan.reached).toEqual([]);
    expect(plan.sent).toEqual([{ kind: "delete", range: { from: 0, count: 5 } }]);
  });

  it("画面に見えている行数を知らないうちは、確認を求める", () => {
    const plan = planOf(
      planRowOperation(
        { kind: "delete", targets: { first: 4, last: 4, count: 1 } },
        contextOf({ viewportRows: null }),
      ),
    );
    expect(plan.reached).toEqual(["confirm:1"]);
    expect(plan.sent).toEqual([]);
  });

  it("確認の答えとして削除するときは、閾値をもう一度見ない（尋ねるのは 1 度だけである）", () => {
    // 確認の答えが新しい確認を生むと、答えた利用者は同じ問いを繰り返し見ることになる。
    const plan = planOf(
      planRowOperation(
        { kind: "confirmDelete", targets: { first: 0, last: 11, count: 12 } },
        contextOf({ viewportRows: 10 }),
      ),
    );
    expect(plan.reached).toEqual([]);
    expect(plan.sent).toHaveLength(1);
    expect(plan.sent[0]?.kind).toBe("delete");
  });

  it("確認への取り消しは、境界へ何も送らない", () => {
    // **計画そのものが送る腕を返さない**（取消は対象も位置も持たない）。
    const plan = planRowOperation({ kind: "cancel" }, contextOf());
    expect(plan).toEqual({ kind: "cancelled" });

    // 振り分けでも**送る腕へは載らない**（送る腕は例外を投げる — 取消が載れば落ちる）。
    const reached: string[] = [];
    runRowOperationPlan(plan, {
      send: () => {
        throw new Error("取消は送ってはならない");
      },
      confirm: () => {
        reached.push("confirm");
      },
      cancel: () => {
        reached.push("cancel");
      },
    });
    expect(reached).toEqual(["cancel"]);
  });
});

// ===========================================================================
// 4. 行数が変わったあとの記憶（要件 1.7）
// ===========================================================================

describe("行数が変わったあとの記憶（要件 1.7）", () => {
  /** 記憶を組む（窓 2 行。行数の変化が窓の区切りに見える大きさである）。 */
  function cacheWith(
    server: { readonly readWindow: (argument: Uint8Array) => Promise<ArrayBuffer> },
    rowCount: number,
  ): WindowCache {
    return createWindowCache({
      sheet: "標本",
      columns: createColumnSpace(ONE_COLUMN),
      rowCount,
      windowRows: 2,
      transport: server.readWindow,
    });
  }

  it("行が増えたとき、増えた行を要求して読める（行数を渡さなければ永久に読み込み中である）", async () => {
    const server = windowServer();
    const cache = cacheWith(server, 8);
    const client = fakeClient(applied(outcomeOf({ affected: [ROW_IDS[0] ?? ""], row_count: 20 })));

    cache.getCell({ row: 0, column: 0 });
    await settle();
    // **元の行数より先は範囲の外である**（取得もしない）。
    expect(cache.getCell({ row: 12, column: 0 }).loading).toBe(true);
    const beforeGrowth = server.calls.length;

    // 行の追加（`row_count` は 8 → 20）。
    await applyRowOperation({
      client,
      cache,
      intent: sentOf(planRowOperation({ kind: "insert", at: 0 }, contextOf({ visibleRows: 8 }))),
    });

    // **増えた行は要求され、届いたあとは読める。** 行数を持ち越すと（`clear` に引数を渡さないと）
    // ここは永久に読み込み中のままである — 要求が 1 本も出ない。
    expect(cache.getCell({ row: 12, column: 0 }).loading).toBe(true);
    expect(
      server.calls.slice(beforeGrowth).map((call) => ({ start: call.start, count: call.count })),
    ).toEqual([{ start: 12, count: 2 }]);
    await settle();
    expect(cache.getCell({ row: 12, column: 0 })).toMatchObject({ text: "12:0", loading: false });
  });

  it("行が減ったとき、減った先は読まず要求もしない（古い窓を配らない）", async () => {
    const server = windowServer();
    const cache = cacheWith(server, 8);
    const client = fakeClient(applied(outcomeOf({ affected: [ROW_IDS[0] ?? ""], row_count: 3 })));

    cache.getCell({ row: 6, column: 0 });
    await settle();
    expect(cache.getCell({ row: 6, column: 0 })).toMatchObject({ text: "6:0", loading: false });
    const beforeShrink = server.calls.length;

    // 行の削除（`row_count` は 8 → 3）。
    await applyRowOperation({
      client,
      cache,
      intent: sentOf(
        planRowOperation(
          { kind: "delete", targets: { first: 3, last: 7, count: 5 } },
          contextOf({ visibleRows: 8, viewportRows: 10 }),
        ),
      ),
    });

    // **捨てた窓の内容を配らない**: 範囲の外として読み込み中を返し、要求もしない。
    expect(cache.getCell({ row: 6, column: 0 }).loading).toBe(true);
    expect(server.calls).toHaveLength(beforeShrink);
    // 残った行は読み直せる（窓は新しい行数へ切り詰められる）。
    expect(cache.getCell({ row: 2, column: 0 }).loading).toBe(true);
    expect(server.calls.map((call) => ({ start: call.start, count: call.count }))).toEqual([
      { start: 6, count: 2 },
      { start: 2, count: 1 },
    ]);
    await settle();
    expect(cache.getCell({ row: 2, column: 0 })).toMatchObject({ text: "2:0", loading: false });
  });

  it("適用できなかったときは、記憶を捨てない（何も変わっていない）", async () => {
    const client = fakeClient({ status: "error", error: FAILURE });
    const memory = fakeClear();

    const settlement = await applyRowOperation({
      client,
      cache: memory.cache,
      intent: { kind: "delete", range: { from: 0, count: 1 } },
    });

    expect(settlement.status).toBe("failed");
    // **捨てない。**適用されていないので取り直す理由が無い（取り直せば、届いている窓をすべて
    // 捨てて読み込み中に見せることになる）。
    expect(memory.cleared).toEqual([]);
    expect(client.edits).toHaveLength(1);
  });

  it("何も変わらなかった適用（影響を受けた行が無い）でも記憶を捨てない", async () => {
    // `affected` が空であるのは「何も書かなかった」である（生成物の doc）。行数も動いていない
    // ので、記憶の対応は正しいままである。
    const client = fakeClient(applied(outcomeOf({ affected: [] })));
    const memory = fakeClear();

    await applyRowOperation({
      client,
      cache: memory.cache,
      intent: { kind: "delete", range: { from: 0, count: 0 } },
    });

    expect(memory.cleared).toEqual([]);
  });
});

// ===========================================================================
// 5. 閾値の源は移植口の知らせである（**先読みの幅ではない**）
// ===========================================================================

describe("閾値の源", () => {
  it("計画は文脈が渡した行数をそのまま閾値にする（画面の高さを推定しない）", () => {
    // この 2 つを取り違えると、**1 画面に収まらない削除**（要件 6.5）が確認を求めなくなる。
    // 計画が読むのは文脈の `viewportRows` だけである（移植口の `onVisibleSpanChange` が
    // その源であり、先読みの窓の幅 `WINDOW_ROWS` は 1 画面の行数ではない）。
    const targets = { first: 0, last: 39, count: 40 };
    expect(
      planOf(
        planRowOperation({ kind: "delete", targets }, contextOf({ viewportRows: 30 })),
      ).reached,
    ).toEqual(["confirm:40"]);
    expect(
      planOf(
        planRowOperation({ kind: "delete", targets }, contextOf({ viewportRows: 200 })),
      ).reached,
    ).toEqual([]);
  });
});

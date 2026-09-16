/**
 * 違反の提示と巡回の論理（tasks.md 8.4。data-grid 要件 4.1、4.2、4.3、4.4、4.6。
 * `./violations`）。
 *
 * # 何を固定するか
 *
 * 1. **窓の印の読み取り**（要件 4.1）。3 つの状態（違反・違反でない・未取得）を区別し、
 *    **未取得を「違反していない」と取り違えない**（取り違えると、窓が届く前のセルの理由を
 *    出さないままにする — 出してはいけない理由を出すよりは害が小さいが、どちらも誤りである）
 * 2. **いまの行の違反の理由**（要件 4.2）。境界の答えが**いまの行のものか**を確かめてから
 *    出し、確かめられなければ出さない。文言は境界が組み立てたものをそのまま載せる
 *    （**画面は 2 つ目の文言を作らない**）
 * 3. **次の違反への移動**（要件 4.4）。境界が返すのは**行の識別子**であり、可視行の序数では
 *    ない。序数は「その序数以降で最初の違反の行」を問い合わせる**二分探索**で解く
 *    （下の「序数の解決」）。表示範囲の外の違反にも到達し、費用は行数の対数で収まる
 * 4. **尽きたこと**は**失敗ではない**（要件 4.4 の正常な結果）
 *
 * # 序数の解決（**境界が序数を運ばないことへの答え**）
 *
 * `GridViolationResponse` が運ぶのは行の識別子（26 文字の ULID）と列の添字であり、
 * **可視行の序数は運ばない**。したがって画面は、返った行の序数を自分で求めるほかない。
 * 使える問い合わせは `grid_find_violation` の 1 つだけであり、その意味は
 * 「起点の序数**以降**で最初の違反の行」である（`from` は両方向で含まれる）。
 *
 * ここから、次の 2 つが成り立つ（対象の違反の序数を `t`、起点を `c` とし、`t` は `c` 以降で
 * 最初の違反である）:
 *
 *   - `x ∈ [c, t]` の問い合わせは、つねに**同じ行**を返す（`t` より前に違反が無いため）
 *   - `x > t` の問い合わせは、つねに**別の行**を返す（`x` 以降で最初の違反は `t` より後ろにある）
 *
 * すなわち「返った行が対象と一致する」は `x <= t` と同値であり、**単調な述語**である。
 * 序数は 0..可視行数-1 の整数なので、二分探索で `t` を確定できる（10 万行で 17 回）。
 * **この段数の対数性そのものを検査で固定する**（線形の走査に戻すと落ちる）。
 *
 * # 何を観測しないか（実物の起動で観測する）
 *
 * 本 file は境界を偽の実装で置き換える。したがって「実際の `grid_find_violation` が
 * 表示範囲の外の違反へ到達すること」（Rust 側の索引の性質。`crates/data-grid` の
 * `view/violations.rs` の `find` とその検査が担う）と、「印の色が実際に描かれること」は
 * ここでは観測しない。
 */
import { describe, expect, it } from "vitest";

import type { GridViolationResponse } from "../../ipc/bindings";
import type { IpcClientError, IpcClientResult } from "../../ipc/client";
import type { GridClient } from "./gridClient";
import type { RenderCell } from "./renderer/port";
import { nextViolation, reasonInRow, violationMark } from "./violations";

/** 境界の窓の文脈（値そのものは検査に効かない）。 */
const CONTEXT = { window: "main" } as const;

/** 境界の失敗（経路の不達）。 */
const FAILURE: IpcClientError = { kind: "Document", detail: { message: "経路が不達である" } };

/** 成功の封筒。 */
function ok<T>(data: T): IpcClientResult<T> {
  return { status: "ok", data };
}

/** 失敗の封筒。 */
function err<T>(): IpcClientResult<T> {
  return { status: "error", error: FAILURE };
}

/** 窓の印を読むためのセル（`RenderCell` の 4 つの欄をすべて埋める）。 */
function cell(overrides: Partial<RenderCell>): RenderCell {
  return { text: "", variant: "Text", violated: false, loading: false, ...overrides };
}

/** 行の識別子（正準の 26 文字）。 */
const ROW_A = "01ARZ3NDEKTSV4RRFFQ69G5FB0";
const ROW_B = "01ARZ3NDEKTSV4RRFFQ69G5FB1";
const ROW_C = "01ARZ3NDEKTSV4RRFFQ69G5FB2";

/** 偽の索引が持つ違反 1 件（**可視行の序数**つき。境界の応答は序数を運ばない）。 */
interface FakeViolation {
  readonly ordinal: number;
  readonly row: string;
  readonly column: number;
  readonly reason: string;
}

/** 偽の境界。**どの起点で問い合わせたかを順に数える**。 */
interface FakeSearch extends Pick<GridClient, "findViolation"> {
  readonly calls: readonly number[];
}

/**
 * 偽の索引（**境界の意味を写したもの**）。`from` 以降で最初の違反の行を返す
 * （`from` は含む。`crates/data-grid/src/view/violations.rs` の `find` と同じ）。該当が
 * 無ければ `null`（「これ以上違反が無い」）。
 *
 * `failOnCall` は**何回目の問い合わせを失敗させるか**である（回数で指定するのは、序数の解決の
 * 問い合わせる点が実装の性質であり、特定の序数を名指しすると検査が実装を写すことになるため）。
 * `rowlessAt` は挙げた序数の違反を行を持たないものとして返す。
 */
function searchOf(
  all: readonly FakeViolation[],
  options: { readonly failOnCall?: number; readonly rowlessAt?: readonly number[] } = {},
): FakeSearch {
  const calls: number[] = [];
  return {
    calls,
    findViolation: async (request) => {
      calls.push(request.from);
      if (calls.length === options.failOnCall) {
        return err<GridViolationResponse>();
      }
      const found = all.find((violation) => violation.ordinal >= request.from);
      if (found === undefined) {
        return ok<GridViolationResponse>({ context: CONTEXT, violation: null });
      }
      // **行を持たない違反**（列そのものの問題）は移動先にならない（`find_violation` の doc）。
      const row = options.rowlessAt?.includes(found.ordinal) === true ? null : found.row;
      return ok<GridViolationResponse>({
        context: CONTEXT,
        violation: {
          location: { row, column: found.column, path: [] },
          reason: found.reason,
        },
      });
    },
  };
}

/** 標本の索引（要件 4.4 の検査で使う 3 件）。 */
const SAMPLE: readonly FakeViolation[] = [
  { ordinal: 2, row: ROW_A, column: 0, reason: "値が空である" },
  { ordinal: 5, row: ROW_B, column: 1, reason: "値が 0 以上 100 以下の外の値である" },
  { ordinal: 9, row: ROW_C, column: 2, reason: "参照先の行が無い" },
];

// ===========================================================================
// 1. 窓の印（要件 4.1）
// ===========================================================================

describe("窓の印の読み取り（要件 4.1）", () => {
  it("違反しているセル・違反していないセル・未取得のセルを区別する", () => {
    expect(violationMark(cell({ violated: true }))).toBe("violated");
    expect(violationMark(cell({ violated: false }))).toBe("clear");
    // **未取得を「違反していない」と読まない。** 未取得のセルは `violated` が偽で運ばれる
    // （空白として描くため）ので、`loading` を見ないと取り違える。
    expect(violationMark(cell({ loading: true }))).toBe("unknown");
    expect(violationMark(cell({ loading: true, violated: true }))).toBe("unknown");
  });
});

// ===========================================================================
// 2. いまの行の違反の理由（要件 4.2）
// ===========================================================================

describe("いまの行の違反の理由（要件 4.2）", () => {
  it("起点は現在の行であり、返った理由をその位置とともに出す", async () => {
    const search = searchOf(SAMPLE);

    const reading = await reasonInRow({ client: search, current: { row: 5, column: 1 }, rowId: ROW_B });

    expect(reading).toEqual({
      kind: "reason",
      position: { row: 5, column: 1 },
      reason: "値が 0 以上 100 以下の外の値である",
    });
    // 起点は**現在の行**である（列は問い合わせに使わない）。
    expect(search.calls).toEqual([5]);
  });

  it("行の最小の違反列が現在の列と違っても、その列を名乗る（指定したセルと取り違えない）", async () => {
    // 5 行目の違反は列 1 にあり、利用者は列 3 を指している。**返るのは列 1 の理由**である
    // （索引は行ごとに最小の列しか返さない）。位置を名乗るので、どのセルの理由かは読める。
    const search = searchOf(SAMPLE);

    const reading = await reasonInRow({ client: search, current: { row: 5, column: 3 }, rowId: ROW_B });

    expect(reading).toEqual({
      kind: "reason",
      position: { row: 5, column: 1 },
      reason: "値が 0 以上 100 以下の外の値である",
    });
  });

  it("答えの行がいまの行と違えば、理由を出さない（後ろの行の違反を現在の行へ貼らない）", async () => {
    const search = searchOf(SAMPLE);

    // いまの行は ROW_A（序数 2）だが、起点 2 の答えは…序数 2 の違反である。ここでは
    // **答えの行が食い違う**場面を作る（別の行の識別子を渡す）。
    const reading = await reasonInRow({ client: search, current: { row: 3, column: 0 }, rowId: ROW_A });

    // 起点 3 の答えは序数 5（ROW_B）であり、いまの行（ROW_A）の違反ではない。
    expect(reading).toEqual({ kind: "cleared" });
  });

  it("行の識別子が無ければ問い合わせない（答えが現在の行のものかを確かめられない）", async () => {
    const search = searchOf(SAMPLE);

    const reading = await reasonInRow({ client: search, current: { row: 5, column: 1 }, rowId: null });

    expect(reading).toEqual({ kind: "cleared" });
    // **推測で出さない。** 窓が届いていない行では、返った答えが別の行のものでありうる。
    expect(search.calls).toEqual([]);
  });

  it("違反が 1 件も無ければ取り下げる", async () => {
    const search = searchOf([]);

    const reading = await reasonInRow({ client: search, current: { row: 5, column: 1 }, rowId: ROW_B });

    expect(reading).toEqual({ kind: "cleared" });
    expect(search.calls).toEqual([5]);
  });

  it("行の識別子の綴りの大小は問わない（同じ行を別の綴りで名指ししても同じ答えになる）", async () => {
    const search = searchOf(SAMPLE);

    const reading = await reasonInRow({ client: search, current: { row: 5, column: 1 }, rowId: ROW_B.toLowerCase() });

    expect(reading.kind).toBe("reason");
  });

  it("問い合わせが失敗したら、その理由を返す（握り潰さない）", async () => {
    const search = searchOf(SAMPLE, { failOnCall: 1 });

    const reading = await reasonInRow({ client: search, current: { row: 5, column: 1 }, rowId: ROW_B });

    expect(reading).toEqual({
      kind: "failed",
      message: "ドキュメントの失敗: 経路が不達である",
    });
  });
});

// ===========================================================================
// 3. 次の違反への移動（要件 4.4）
// ===========================================================================

describe("次の違反への移動（要件 4.4）", () => {
  it("次の違反の位置を、可視行の序数まで解決して返す", async () => {
    const search = searchOf(SAMPLE);

    // いまの行は 2（ROW_A の違反の行）である。次は序数 5 の ROW_B。
    // いまの行は 2（ROW_A の違反の行）である。起点はその次（3）である。
    const reading = await nextViolation({ client: search, current: { row: 2, column: 0 }, rowCount: 100 });

    expect(reading).toEqual({
      kind: "reason",
      // **返るのは行の識別子ではなく可視行の序数である**（そのまま現在位置にできる）。
      position: { row: 5, column: 1 },
      reason: "値が 0 以上 100 以下の外の値である",
    });
    // 起点がそのまま 1 度目の問い合わせである（`from` は含まれる）。
    expect(search.calls[0]).toBe(3);
  });

  it("表示範囲の外にある違反へも到達する（10 万行のうち 4 万行目）", async () => {
    const far: FakeViolation = {
      ordinal: 40_000,
      row: ROW_C,
      column: 2,
      reason: "参照先の行が無い",
    };
    const search = searchOf([SAMPLE[0] as FakeViolation, far]);

    const reading = await nextViolation({
      client: search,
      current: { row: 9, column: 0 },
      rowCount: 100_000,
    });

    expect(reading).toEqual({
      kind: "reason",
      position: { row: 40_000, column: 2 },
      reason: "参照先の行が無い",
    });
    // **行数を走査しない。** 序数の解決は行数の対数で収まる（線形の走査に戻すと落ちる）。
    expect(search.calls.length).toBeLessThanOrEqual(20);
  });

  it("いまの行の違反は探索の起点の外である（「次の」違反を返す）", async () => {
    const search = searchOf(SAMPLE);

    // いまの行が 2 であるとき、答えは序数 5 である（**いまの行の違反（序数 2）を返さない**）。
    // いまの行は 2（ROW_A の違反の行）である。起点はその次（3）である。
    const reading = await nextViolation({ client: search, current: { row: 2, column: 0 }, rowCount: 100 });
    if (reading.kind !== "reason") {
      throw new Error("次の違反が見つからなかった");
    }
    expect(reading.position.row).toBe(5);
  });

  it("これ以上違反が無ければ「尽きた」を返す（失敗ではない）", async () => {
    const search = searchOf(SAMPLE);

    // 最後の違反（序数 9）より後ろから探す。
    // 最後の違反（序数 9）が、いまの行そのものである。
    const reading = await nextViolation({ client: search, current: { row: 9, column: 0 }, rowCount: 100 });

    // **正常な結果である**（封筒の失敗でも、告知でもない）。
    expect(reading).toEqual({ kind: "exhausted" });
    expect(search.calls).toEqual([10]);
  });

  it("違反が 1 件も無いシートでも「尽きた」を返す", async () => {
    const search = searchOf([]);

    const reading = await nextViolation({ client: search, current: { row: 0, column: 0 }, rowCount: 100 });

    expect(reading).toEqual({ kind: "exhausted" });
  });

  it("行を持たない違反は移動先にしない（位置を特定できない）", async () => {
    // 行を持たない違反（列そのものの問題）は総数には数えるが、移動先にはならない
    // （`find_violation` の doc）。ここへ来る経路は無いが、**推測した行へ動かさない**ことを
    // 固定する（生成物の型は `row: null` を許す）。
    const search = searchOf(SAMPLE, { rowlessAt: [5] });

    // いまの行は 2（ROW_A の違反の行）である。起点はその次（3）である。
    const reading = await nextViolation({ client: search, current: { row: 2, column: 0 }, rowCount: 100 });

    expect(reading).toEqual({
      kind: "failed",
      message: "違反の行を特定できませんでした",
    });
  });

  it("序数の解決の途中で失敗したら、失敗を返す（途中の答えで位置を決めない）", async () => {
    const far: FakeViolation = { ordinal: 40_000, row: ROW_C, column: 2, reason: "参照先の行が無い" };
    const search = searchOf([far], { failOnCall: 2 });

    const reading = await nextViolation({
      client: search,
      current: { row: 9, column: 0 },
      rowCount: 100_000,
    });

    expect(reading).toEqual({
      kind: "failed",
      message: "ドキュメントの失敗: 経路が不達である",
    });
    // 途中で止まっている（最後まで問い合わせない）。
    expect(search.calls.length).toBeLessThan(20);
  });
});

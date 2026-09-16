/**
 * 現在位置と選択の規則（tasks.md 8.2。data-grid 要件 2.1、2.2、2.3、2.4、2.5）。
 *
 * # 何を固定するか
 *
 * 1. **移動の隣接性と端の扱い**（要件 2.2）。4 方向の移動、および行・列の端とシートの端で
 *    **止まる**こと（巻き戻さない）。端の扱いは本 module の決定であり、ここで逐語で固定する。
 * 2. **3 つの選択の形**（要件 2.3）。矩形（広げ）、行の全体、列の全体。
 * 3. **数え上げ**（要件 2.5）。行数・列数・セル数。1×1、行の全体、列の全体、0 件の列/行。
 * 4. **追随の判断**（要件 2.4）。現在位置が可視の区間の外へ出たときだけ `scrollTo` の宛先を返す。
 * 5. **打鍵の割り当て**。どの打鍵をこの module が引き受け、どれを他の束縛へ譲るか。
 * 6. **不変条件**: どの操作の後でも `current` は `range` の中にある（要件 2.1 が「現在位置は
 *    1 つ」と言える根拠である）。
 *
 * # 何を固定しないか（本 file は証明しない）
 *
 * **見え方**である。現在位置がグリッドの上で他のセルと区別されて描かれること（焦点の環）、
 * 追随が実際にスクロールを起こすことは、canvas と実物の起動を要する — 本 file の環境は
 * `node` であり DOM を持たない（`vitest.config.ts`）。そこは実物の起動で観測する
 * （`src/features/smoke/portProbe*` と 9.2 / 9.3 の観測。8.1 の起動観測と同じ規律である）。
 */
import { describe, expect, it } from "vitest";

import {
  clampSelection,
  extendSelection,
  followTarget,
  initialSelection,
  moveCurrent,
  selectWholeColumn,
  selectWholeRow,
  selectionAt,
  selectionCounts,
  selectionForKey,
  type KeyStroke,
  type SelectionBounds,
} from "./selection";
import type { CellPosition, CellRange, RendererSelection, VisibleSpan } from "./renderer/port";

// ===========================================================================
// 検査の道具
// ===========================================================================

/** 標本の大きさ（行 100 件・列 5 本）。 */
const BOUNDS: SelectionBounds = { rowCount: 100, columnCount: 5 };

/** 打鍵（修飾キーは指定したものだけ真）。 */
function key(keyName: string, modifiers: Partial<KeyStroke> = {}): KeyStroke {
  return { key: keyName, shiftKey: false, altKey: false, ctrlKey: false, metaKey: false, ...modifiers };
}

/** その位置 1 つだけの選択。 */
function at(position: CellPosition): RendererSelection {
  return { current: position, range: { start: position, end: position } };
}

/** 2 つの角から矩形の選択を組む（正規化する）。 */
function rect(from: CellPosition, to: CellPosition): RendererSelection {
  const range: CellRange = {
    start: {
      row: Math.min(from.row, to.row),
      column: Math.min(from.column, to.column),
    },
    end: {
      row: Math.max(from.row, to.row),
      column: Math.max(from.column, to.column),
    },
  };
  return { current: from, range };
}

/** 選択が不変条件（現在位置が範囲の中）を満たすか。 */
function holdsInvariant(selection: RendererSelection): boolean {
  const { current, range } = selection;
  return (
    current.row >= range.start.row &&
    current.row <= range.end.row &&
    current.column >= range.start.column &&
    current.column <= range.end.column
  );
}

/** 可視の区間（行 10 行・列 3 本が見えている）。 */
const VISIBLE: VisibleSpan = { rows: { start: 10, count: 10 }, columns: { start: 2, count: 3 } };

// ===========================================================================
// 1. 移動（要件 2.2）
// ===========================================================================

describe("方向の指示で隣接するセルへ移動する（要件 2.2）", () => {
  it("4 方向のそれぞれで、隣のセルへ 1 つだけ動く", () => {
    const from = at({ row: 5, column: 2 });

    expect(moveCurrent(from, "up", BOUNDS).current).toEqual({ row: 4, column: 2 });
    expect(moveCurrent(from, "down", BOUNDS).current).toEqual({ row: 6, column: 2 });
    expect(moveCurrent(from, "left", BOUNDS).current).toEqual({ row: 5, column: 1 });
    expect(moveCurrent(from, "right", BOUNDS).current).toEqual({ row: 5, column: 3 });
  });

  it("移動した後は、範囲が現在位置 1 つへ畳まれる", () => {
    const wide = rect({ row: 5, column: 2 }, { row: 8, column: 4 });

    const moved = moveCurrent(wide, "down", BOUNDS);

    expect(moved.range).toEqual({ start: { row: 6, column: 2 }, end: { row: 6, column: 2 } });
    expect(moved.current).toEqual({ row: 6, column: 2 });
  });

  it("行の端では止まる（前の行の末尾へは回り込まない）", () => {
    // 列 0 で左、最終列で右。**どちらも動かない。**
    expect(moveCurrent(at({ row: 3, column: 0 }), "left", BOUNDS).current).toEqual({
      row: 3,
      column: 0,
    });
    expect(moveCurrent(at({ row: 3, column: 4 }), "right", BOUNDS).current).toEqual({
      row: 3,
      column: 4,
    });
  });

  it("シートの端では止まる（最終行の次へは出ない）", () => {
    expect(moveCurrent(at({ row: 0, column: 2 }), "up", BOUNDS).current).toEqual({
      row: 0,
      column: 2,
    });
    expect(moveCurrent(at({ row: 99, column: 2 }), "down", BOUNDS).current).toEqual({
      row: 99,
      column: 2,
    });
  });

  it("端でも範囲は現在位置へ畳まれる（止まった先が 1 つの現在位置である）", () => {
    const wide = rect({ row: 0, column: 0 }, { row: 4, column: 4 });

    const moved = moveCurrent(wide, "up", BOUNDS);

    expect(moved.range).toEqual({ start: { row: 0, column: 0 }, end: { row: 0, column: 0 } });
  });
});

// ===========================================================================
// 2. 矩形の範囲の選択（要件 2.3 の 1 つ目）
// ===========================================================================

describe("矩形の範囲を広げる（要件 2.3）", () => {
  it("錨から現在位置までの矩形になる（広げた方向へ 1 つずつ）", () => {
    const start = at({ row: 2, column: 1 });

    const right = extendSelection(start, "right", BOUNDS);
    expect(right.current).toEqual({ row: 2, column: 2 });
    expect(right.range).toEqual({ start: { row: 2, column: 1 }, end: { row: 2, column: 2 } });

    const down = extendSelection(right, "down", BOUNDS);
    expect(down.current).toEqual({ row: 3, column: 2 });
    expect(down.range).toEqual({ start: { row: 2, column: 1 }, end: { row: 3, column: 2 } });
  });

  it("錨は動かない（広げてから戻すと元の 1 セルへ戻る）", () => {
    const start = at({ row: 2, column: 1 });

    const grown = extendSelection(extendSelection(start, "down", BOUNDS), "down", BOUNDS);
    expect(grown.range).toEqual({ start: { row: 2, column: 1 }, end: { row: 4, column: 1 } });

    const shrunk = extendSelection(grown, "up", BOUNDS);
    expect(shrunk.current).toEqual({ row: 3, column: 1 });
    expect(shrunk.range).toEqual({ start: { row: 2, column: 1 }, end: { row: 3, column: 1 } });
  });

  it("逆向きへ広げると向きが反転する（現在位置が反対の角へ移る）", () => {
    // 右下から左上へ引く操作に当たる。**現在位置は左上とは限らない**（移植口が
    // `RendererSelection`（現在位置と矩形）を運ぶ理由である）。
    const from = at({ row: 5, column: 3 });

    const left = extendSelection(from, "left", BOUNDS);
    expect(left.current).toEqual({ row: 5, column: 2 });
    expect(left.range).toEqual({ start: { row: 5, column: 2 }, end: { row: 5, column: 3 } });

    const up = extendSelection(left, "up", BOUNDS);
    expect(up.current).toEqual({ row: 4, column: 2 });
    expect(up.range).toEqual({ start: { row: 4, column: 2 }, end: { row: 5, column: 3 } });
  });

  it("端では広がらない（現在位置が端に留まり、範囲もそれ以上広がらない）", () => {
    // 錨を左に、現在位置を最終列に置いた選択（右へ引いた状態）。
    const atRightEdge: RendererSelection = {
      current: { row: 1, column: 4 },
      range: { start: { row: 1, column: 2 }, end: { row: 1, column: 4 } },
    };

    const grown = extendSelection(atRightEdge, "right", BOUNDS);

    expect(grown.current).toEqual({ row: 1, column: 4 });
    expect(grown.range).toEqual({ start: { row: 1, column: 2 }, end: { row: 1, column: 4 } });
  });

  it("錨へ向けて広げると、範囲はその角へ畳まれる（表計算と同じ振る舞い）", () => {
    // 錨が (1,4)、現在位置が (1,3) のとき右へ広げると、現在位置は錨へ着いて範囲は 1 セルになる。
    const towardAnchor: RendererSelection = {
      current: { row: 1, column: 3 },
      range: { start: { row: 1, column: 3 }, end: { row: 1, column: 4 } },
    };

    const grown = extendSelection(towardAnchor, "right", BOUNDS);

    expect(grown.current).toEqual({ row: 1, column: 4 });
    expect(grown.range).toEqual({ start: { row: 1, column: 4 }, end: { row: 1, column: 4 } });
  });
});

// ===========================================================================
// 3. 行の全体・列の全体（要件 2.3 の 2 つ目・3 つ目）
// ===========================================================================

describe("行の全体・列の全体を選ぶ（要件 2.3）", () => {
  it("行の全体は、現在位置の行の全列になる（現在位置は動かない）", () => {
    const selection = at({ row: 3, column: 2 });

    const whole = selectWholeRow(selection, BOUNDS);

    expect(whole.range).toEqual({ start: { row: 3, column: 0 }, end: { row: 3, column: 4 } });
    // **現在位置は利用者が居た所のままである**（行の全体を選んでも現在位置は 1 つ）。
    expect(whole.current).toEqual({ row: 3, column: 2 });
    expect(holdsInvariant(whole)).toBe(true);
  });

  it("列の全体は、現在位置の列の全行になる（現在位置は動かない）", () => {
    const selection = at({ row: 3, column: 2 });

    const whole = selectWholeColumn(selection, BOUNDS);

    expect(whole.range).toEqual({ start: { row: 0, column: 2 }, end: { row: 99, column: 2 } });
    expect(whole.current).toEqual({ row: 3, column: 2 });
    expect(holdsInvariant(whole)).toBe(true);
  });

  it("範囲の選択からでも行の全体・列の全体になる（現在位置の行・列を取る）", () => {
    const wide = rect({ row: 2, column: 1 }, { row: 6, column: 3 });

    expect(selectWholeRow(wide, BOUNDS).range).toEqual({
      start: { row: 2, column: 0 },
      end: { row: 2, column: 4 },
    });
    expect(selectWholeColumn(wide, BOUNDS).range).toEqual({
      start: { row: 0, column: 1 },
      end: { row: 99, column: 1 },
    });
  });

  it("列が 1 本も無い・行が 1 件も無いときは何もしない（範囲の外の選択を作らない）", () => {
    const selection = at({ row: 0, column: 0 });

    expect(selectWholeRow(selection, { rowCount: 0, columnCount: 0 })).toEqual(selection);
    expect(selectWholeColumn(selection, { rowCount: 0, columnCount: 0 })).toEqual(selection);
    expect(selectWholeRow(selection, { rowCount: 10, columnCount: 0 })).toEqual(selection);
    expect(selectWholeColumn(selection, { rowCount: 0, columnCount: 10 })).toEqual(selection);
  });
});

// ===========================================================================
// 4. 数え上げ（要件 2.5）
// ===========================================================================

describe("選択の行数・列数・セル数（要件 2.5）", () => {
  it("1 つのセルは 1 行 × 1 列 = 1 セルである（退化した場合）", () => {
    const counts = selectionCounts(initialSelection());

    expect(counts).toEqual({ rows: 1, columns: 1, cells: 1 });
  });

  it("矩形は行数 × 列数であり、セル数はその積である", () => {
    const counts = selectionCounts(rect({ row: 1, column: 0 }, { row: 3, column: 1 }));

    expect(counts).toEqual({ rows: 3, columns: 2, cells: 6 });
  });

  it("行の全体は 1 行 × 全列であり、セル数は列数に等しい", () => {
    const counts = selectionCounts(selectWholeRow(at({ row: 3, column: 2 }), BOUNDS));

    expect(counts).toEqual({ rows: 1, columns: 5, cells: 5 });
  });

  it("列の全体は全行 × 1 列であり、セル数は行数に等しい", () => {
    const counts = selectionCounts(selectWholeColumn(at({ row: 3, column: 2 }), BOUNDS));

    expect(counts).toEqual({ rows: 100, columns: 1, cells: 100 });
  });
});

// ===========================================================================
// 5. 追随（要件 2.4）
// ===========================================================================

describe("現在位置が表示範囲の外へ出たときの追随（要件 2.4）", () => {
  it("見えている位置では動かさない", () => {
    // 可視は行 10〜19・列 2〜4 である。**両端は見えている**（半開区間の端の扱い）。
    expect(followTarget(VISIBLE, at({ row: 10, column: 2 }))).toBeNull();
    expect(followTarget(VISIBLE, at({ row: 19, column: 4 }))).toBeNull();
    expect(followTarget(VISIBLE, at({ row: 15, column: 3 }))).toBeNull();
  });

  it("行が上・下へ外れたときは、その位置へ追随する", () => {
    expect(followTarget(VISIBLE, at({ row: 9, column: 3 }))).toEqual({ row: 9, column: 3 });
    expect(followTarget(VISIBLE, at({ row: 20, column: 3 }))).toEqual({ row: 20, column: 3 });
  });

  it("列が左・右へ外れたときも追随する（行だけでは足りない）", () => {
    expect(followTarget(VISIBLE, at({ row: 15, column: 1 }))).toEqual({ row: 15, column: 1 });
    expect(followTarget(VISIBLE, at({ row: 15, column: 5 }))).toEqual({ row: 15, column: 5 });
  });

  it("両軸とも外れたときは 1 回の追随でその位置へ動く", () => {
    expect(followTarget(VISIBLE, at({ row: 30, column: 9 }))).toEqual({ row: 30, column: 9 });
  });

  it("可視の区間を知らないうちは動かさない（まだ何も見えていない）", () => {
    expect(followTarget(null, at({ row: 0, column: 0 }))).toBeNull();
    expect(
      followTarget(
        { rows: { start: 10, count: 0 }, columns: { start: 2, count: 3 } },
        at({ row: 0, column: 0 }),
      ),
    ).toBeNull();
    expect(
      followTarget(
        { rows: { start: 10, count: 10 }, columns: { start: 2, count: 0 } },
        at({ row: 0, column: 0 }),
      ),
    ).toBeNull();
  });
});

// ===========================================================================
// 6. 打鍵の割り当て
// ===========================================================================

describe("打鍵の割り当て（どの指示をこの module が引き受けるか）", () => {
  const selection = at({ row: 5, column: 2 });

  it("矢印は移動である", () => {
    expect(selectionForKey(key("ArrowUp"), selection, BOUNDS)?.current).toEqual({
      row: 4,
      column: 2,
    });
    expect(selectionForKey(key("ArrowDown"), selection, BOUNDS)?.current).toEqual({
      row: 6,
      column: 2,
    });
    expect(selectionForKey(key("ArrowLeft"), selection, BOUNDS)?.current).toEqual({
      row: 5,
      column: 1,
    });
    expect(selectionForKey(key("ArrowRight"), selection, BOUNDS)?.current).toEqual({
      row: 5,
      column: 3,
    });
  });

  it("shift + 矢印は範囲を広げる", () => {
    const grown = selectionForKey(key("ArrowDown", { shiftKey: true }), selection, BOUNDS);

    expect(grown?.current).toEqual({ row: 6, column: 2 });
    expect(grown?.range).toEqual({ start: { row: 5, column: 2 }, end: { row: 6, column: 2 } });
  });

  it("shift + 空白は行の全体、ctrl（または meta）+ 空白は列の全体である", () => {
    expect(selectionForKey(key(" ", { shiftKey: true }), selection, BOUNDS)?.range).toEqual({
      start: { row: 5, column: 0 },
      end: { row: 5, column: 4 },
    });
    expect(selectionForKey(key(" ", { ctrlKey: true }), selection, BOUNDS)?.range).toEqual({
      start: { row: 0, column: 2 },
      end: { row: 99, column: 2 },
    });
    // macOS の修飾キーでも同じ（器は 3 つの OS で動く。要件 12.4）。
    expect(selectionForKey(key(" ", { metaKey: true }), selection, BOUNDS)?.range).toEqual({
      start: { row: 0, column: 2 },
      end: { row: 99, column: 2 },
    });
  });

  it("修飾キーの付いた矢印は引き受けない（他の束縛へ譲る）", () => {
    // primary + 矢印は列の端・行の端への移動であり、本 module の担当ではない（Glide の既定が
    // そのまま働く）。譲ることを `null` で表す — 引き受けて黙って飲むと、その束縛が死ぬ。
    for (const modifiers of [{ altKey: true }, { ctrlKey: true }, { metaKey: true }]) {
      for (const arrow of ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]) {
        expect(selectionForKey(key(arrow, modifiers), selection, BOUNDS)).toBeNull();
      }
    }
  });

  it("関係の無い打鍵は引き受けない", () => {
    for (const name of ["Enter", "Tab", "Escape", "a", "Home", "PageDown"]) {
      expect(selectionForKey(key(name), selection, BOUNDS)).toBeNull();
    }
    // 何も押さない空白も、修飾キーが無ければ行の全体ではない（選択の解除などに使われる）。
    expect(selectionForKey(key(" "), selection, BOUNDS)).toBeNull();
    expect(selectionForKey(key(" ", { shiftKey: true, ctrlKey: true }), selection, BOUNDS)).toBeNull();
  });
});

// ===========================================================================
// 7. 不変条件（要件 2.1 が「現在位置は 1 つ」と言える根拠）
// ===========================================================================

describe("現在位置はつねに選択の範囲の中にある（要件 2.1）", () => {
  it("どの操作の後でも不変条件が保たれる", () => {
    const bounds: SelectionBounds = { rowCount: 4, columnCount: 3 };
    const starts: CellPosition[] = [];
    for (let row = 0; row < bounds.rowCount; row += 1) {
      for (let column = 0; column < bounds.columnCount; column += 1) {
        starts.push({ row, column });
      }
    }

    const operations = [
      () => initialSelection(),
      (selection: RendererSelection) => moveCurrent(selection, "right", bounds),
      (selection: RendererSelection) => moveCurrent(selection, "up", bounds),
      (selection: RendererSelection) => extendSelection(selection, "down", bounds),
      (selection: RendererSelection) => extendSelection(selection, "left", bounds),
      (selection: RendererSelection) => selectWholeRow(selection, bounds),
      (selection: RendererSelection) => selectWholeColumn(selection, bounds),
    ];

    let checked = 0;
    for (const start of starts) {
      for (const operation of operations) {
        const next = operation(at(start));
        expect(holdsInvariant(next)).toBe(true);
        checked += 1;
      }
    }
    // **空の走査で緑になっていないこと**（この数が 0 なら上の主張は何も見ていない）。
    expect(checked).toBe(starts.length * operations.length);
    expect(checked).toBe(4 * 3 * 7);
  });
});

describe("構成が縮んだときの寄せ（要件 2.1、5.2）", () => {
  it("列が消えたら、現在位置と範囲は表の中へ寄る", () => {
    // 境界の修復のレビューが実測した欠陥: 入れ子を折りたたむと列数が減り、最後の列にあった
    // 現在位置が**描かれる表の外**へ残る（数え上げの行は「5 列」と名乗るのに描かれるのは 4 本）。
    const clamped = clampSelection(selectionAt({ row: 2, column: 4 }), {
      rowCount: 10,
      columnCount: 4,
    });

    expect(clamped.current).toEqual({ row: 2, column: 3 });
    expect(clamped.range.end).toEqual({ row: 2, column: 3 });
  });

  it("行が減ったときも同じく寄る", () => {
    const clamped = clampSelection(selectionAt({ row: 9, column: 1 }), {
      rowCount: 3,
      columnCount: 4,
    });

    expect(clamped.current).toEqual({ row: 2, column: 1 });
  });

  it("範囲の中にあるときは、同じ値を返す（据え置きを参照で決められる）", () => {
    const before = selectionAt({ row: 1, column: 1 });
    const after = clampSelection(before, { rowCount: 10, columnCount: 10 });

    expect(after).toBe(before);
  });

  it("表が空（列 0 本）でも、範囲の外の選択を作らない", () => {
    const clamped = clampSelection(selectionAt({ row: 3, column: 3 }), {
      rowCount: 0,
      columnCount: 0,
    });

    expect(clamped.current).toEqual({ row: 0, column: 0 });
  });
});

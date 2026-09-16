/**
 * 表示の操作（列幅・表示上の列順・並べ替え・絞り込み）の判断（tasks.md 8.8。data-grid 要件
 * 8.1、8.2、8.3、8.4、8.5、8.7。`./viewOps`）。
 *
 * # 何を固定するか
 *
 * 1. **並べ替えの基準列の巡回**（要件 8.3）。押された列は「基準でない → 昇順 → 降順 → 基準で
 *    ない」と回り、**他の基準列は残る**（複数の基準を持てる）。
 * 2. **絞り込みは列ごとに 1 件**（要件 8.4）。同じ列の条件は置き換わり、**他の列の条件は
 *    残る**（境界の `GridViewSpec` が積として働くことは、その形が保証する）。
 * 3. **どの操作も指定は完全な記述のままである**（`GridViewSpec` の doc）。とくに
 *    **入れ子の展開の状態は失われない**（要件 5.3。並べ替えと絞り込みは列の話ではない）。
 * 4. **列幅と表示上の列順は境界へ渡る指定に現れない**（要件 8.1、8.2、8.5）。**これは
 *    表示の操作の中で最も壊れやすい規則である** — `DisplayState` を指定へ混ぜても型検査は
 *    通り、画面は動き、**保存される列の順序だけが静かに変わる**。ここでは指定の鍵の集合を
 *    そのまま表明する（`[sort, filters, expansion]` の 3 つ以外は 1 つも無い）。
 * 5. **表示の並びの合成**（要件 8.2）。表示の操作（幅・並び）は `RendererSpec.columns` と
 *    窓の記憶の列の写像の**同じ 1 つの並び**を作る — 片方だけが追随すると、描かれる値と
 *    編集の宛先が別の列を指す（要件 8.6 と同じ危険が列にもある）。
 * 6. **隠れた行の数の提示**（要件 8.7）。数は**応答が運んだ数そのもの**であり、絞り込みが
 *    効いていないのに「隠れている」と名乗らない。
 *
 * # 検査の環境（`node`）
 *
 * 本 module は純粋な関数と `DisplayState`（7.5）だけで組める（React も DOM も要らない）。
 * 表の器の組み立てと、列幅のドラッグそのものは実物の起動で観測する（module doc の
 * 「単体テストが観測しないもの」）。
 */
import { describe, expect, it } from "vitest";

import type { ColumnDescriptor, GridViewSpec } from "../../ipc/bindings";
import { createDisplayState } from "./displayState";
import { EMPTY_GRID_VIEW } from "./gridClient";
import {
  applyViewOperation,
  distinctColumns,
  drawnColumns,
  filterModeOf,
  filterOf,
  filterSpecOf,
  hasRowRestriction,
  hiddenRowsNotice,
  layoutKeyOf,
  rowOrderKeyOf,
  sortStateOf,
  type ViewOperation,
} from "./viewOps";

// ===========================================================================
// 検査の道具
// ===========================================================================

/** 列 1 本ぶんの記述（**宣言の順**に入る。`column` は文書の列である）。 */
function descriptor(column: number, name: string): ColumnDescriptor {
  return { column, path: [], name, kind: "Text", element_count: null, expandability: "leaf" };
}

/** 展開した列の内側の位置の記述（親と同じ文書の列を指す。要件 5.1）。 */
function inner(column: number, name: string, field: string): ColumnDescriptor {
  return {
    column,
    path: [{ segment: "Field", name: field }],
    name,
    kind: "Text",
    element_count: null,
    expandability: "leaf",
  };
}

/** 宣言の列 3 本（列 0..2）。 */
const LAYOUT: readonly ColumnDescriptor[] = [
  descriptor(0, "名前"),
  descriptor(1, "数量"),
  descriptor(2, "備考"),
];

/** 展開した構成（**表示の位置と文書の列が離れる**唯一の場面である。8.5）。 */
const EXPANDED: readonly ColumnDescriptor[] = [
  inner(0, "名前.姓", "姓"),
  inner(0, "名前.名", "名"),
  descriptor(1, "数量"),
];

/** 表示の操作を適用する（`EMPTY_GRID_VIEW` から 1 手進める補助である）。 */
function applied(operation: ViewOperation, from: GridViewSpec = EMPTY_GRID_VIEW): GridViewSpec {
  return applyViewOperation(from, operation);
}

// ===========================================================================
// 1. 並べ替えの基準列の巡回（要件 8.3）
// ===========================================================================

describe("並べ替えの基準列（要件 8.3）", () => {
  it("押された列は、基準でない → 昇順 → 降順 → 基準でない と回る", () => {
    const ascending = applied({ kind: "sortCycle", column: 1 });
    expect(ascending.sort).toEqual([{ column: 1, descending: false }]);
    expect(sortStateOf(ascending, 1)).toBe(false);

    const descending = applied({ kind: "sortCycle", column: 1 }, ascending);
    expect(descending.sort).toEqual([{ column: 1, descending: true }]);
    expect(sortStateOf(descending, 1)).toBe(true);

    const cleared = applied({ kind: "sortCycle", column: 1 }, descending);
    expect(cleared.sort).toEqual([]);
    // 基準でない列の状態は `null` である（「昇順」と「基準でない」を潰さない）。
    expect(sortStateOf(cleared, 1)).toBeNull();
  });

  it("新しい列は第一の基準になり、既にある基準列は残る（複数の基準）", () => {
    const first = applied({ kind: "sortCycle", column: 1 });
    const second = applied({ kind: "sortCycle", column: 2 }, first);

    // **先頭が第一の基準である**（`GridViewSpec.sort` の doc）。
    expect(second.sort).toEqual([
      { column: 2, descending: false },
      { column: 1, descending: false },
    ]);
    // 第一の基準の向きを回しても、第二の基準は残る。
    const cycled = applied({ kind: "sortCycle", column: 2 }, second);
    expect(cycled.sort).toEqual([
      { column: 2, descending: true },
      { column: 1, descending: false },
    ]);
  });

  it("向きを直接指定でき、解除は 1 つも基準を残さない", () => {
    const descending = applied({ kind: "sort", column: 0, descending: true });
    expect(descending.sort).toEqual([{ column: 0, descending: true }]);

    expect(applied({ kind: "sortNone" }, descending).sort).toEqual([]);
    // 向きを指定し直しても、同じ列が 2 度現れない（基準は列ごとに 1 つである）。
    const again = applied({ kind: "sort", column: 0, descending: false }, descending);
    expect(again.sort).toEqual([{ column: 0, descending: false }]);
  });

  it("絞り込みと展開は並べ替えの操作で失われない", () => {
    const base = applied({ kind: "filter", column: 1, filter: filterSpecOf("equals", 1, "3") });
    const sorted = applied({ kind: "sortCycle", column: 2 }, base);

    expect(sorted.filters).toEqual(base.filters);
    expect(sorted.expansion).toEqual([]);
  });
});

// ===========================================================================
// 2. 絞り込み（要件 8.4）
// ===========================================================================

describe("絞り込みの条件（要件 8.4）", () => {
  it("5 つの条件が、境界の型そのものへ写る", () => {
    expect(filterSpecOf("equals", 1, "3")).toEqual({ filter: "Equals", column: 1, text: "3" });
    expect(filterSpecOf("contains", 1, "あ")).toEqual({ filter: "Contains", column: 1, text: "あ" });
    expect(filterSpecOf("empty", 1, "無視される")).toEqual({ filter: "IsEmpty", column: 1 });
    expect(filterSpecOf("notEmpty", 1, "")).toEqual({ filter: "IsNotEmpty", column: 1 });
    expect(filterSpecOf("violating", 1, "")).toEqual({ filter: "HasViolation", column: 1 });
    // 解除は条件そのものを持たない（列だけを指して外す）。
    expect(filterSpecOf("none", 1, "3")).toBeNull();
  });

  it("同じ列の条件は置き換わり、他の列の条件は残る（積である）", () => {
    const first = applied({ kind: "filter", column: 1, filter: filterSpecOf("equals", 1, "3") });
    const second = applied({ kind: "filter", column: 2, filter: filterSpecOf("contains", 2, "x") }, first);
    expect(second.filters).toEqual([
      { filter: "Equals", column: 1, text: "3" },
      { filter: "Contains", column: 2, text: "x" },
    ]);

    // 同じ列を別の条件へ置き換える（**2 件にならない**）。
    const replaced = applied({ kind: "filter", column: 1, filter: filterSpecOf("empty", 1) }, second);
    expect(replaced.filters).toEqual([
      { filter: "IsEmpty", column: 1 },
      { filter: "Contains", column: 2, text: "x" },
    ]);

    // 1 列だけ外す。
    const removed = applied({ kind: "filter", column: 1, filter: null }, replaced);
    expect(removed.filters).toEqual([{ filter: "Contains", column: 2, text: "x" }]);
    expect(filterOf(removed, 1)).toBeNull();

    // すべて外す。
    expect(applied({ kind: "filtersNone" }, removed).filters).toEqual([]);
  });

  it("いまの条件は、列ごとに読み出せる（画面が選択肢の現在値を出すためである）", () => {
    const view = applied({ kind: "filter", column: 1, filter: filterSpecOf("empty", 1) });
    expect(filterOf(view, 1)).toEqual({ filter: "IsEmpty", column: 1 });
    expect(filterOf(view, 2)).toBeNull();

    // 選択肢の現在値と、そこから組み立て直す条件が往復する（画面の表示と送る値がずれない）。
    for (const mode of ["contains", "equals", "empty", "notEmpty", "violating", "none"] as const) {
      const spec = filterSpecOf(mode, 1, "値");
      expect(filterModeOf(spec)).toBe(mode);
    }
  });

  it("並べ替えと展開は絞り込みの操作で失われない", () => {
    const base = applied({ kind: "sortCycle", column: 0 });
    const filtered = applied({ kind: "filter", column: 1, filter: filterSpecOf("empty", 1) }, base);

    expect(filtered.sort).toEqual(base.sort);
    expect(filtered.expansion).toEqual([]);
  });

  it("絞り込み・並べ替えの有無が読める（挿入の位置を写せるかの判断に使う。要件 8.6）", () => {
    expect(hasRowRestriction(EMPTY_GRID_VIEW)).toBe(false);
    expect(hasRowRestriction(applied({ kind: "sortCycle", column: 0 }))).toBe(true);
    expect(
      hasRowRestriction(applied({ kind: "filter", column: 0, filter: filterSpecOf("empty", 0) })),
    ).toBe(true);
  });
});

// ===========================================================================
// 3. 指定は完全な記述であり、列幅・列順を含まない（要件 8.1、8.2、8.5）
// ===========================================================================

describe("境界へ渡る指定（要件 8.1、8.2、8.5）", () => {
  const OPERATIONS: readonly ViewOperation[] = [
    { kind: "sortCycle", column: 1 },
    { kind: "sort", column: 1, descending: true },
    { kind: "sortNone" },
    { kind: "filter", column: 1, filter: filterSpecOf("contains", 1, "x") },
    { kind: "filter", column: 1, filter: null },
    { kind: "filtersNone" },
  ];

  it("どの操作の結果も、指定の欄は 3 つだけである（列幅と列順は現れない）", () => {
    for (const operation of OPERATIONS) {
      const view = applied(operation);
      expect(Object.keys(view).sort()).toEqual(["expansion", "filters", "sort"]);
      // **値にも現れない**（鍵を別名で持つ・入れ子へ混ぜる、のどちらの漏れも捕まえる）。
      const serialized = JSON.stringify(view);
      expect(serialized).not.toContain("width");
      expect(serialized).not.toContain("order");
    }
  });

  it("展開の操作も同じ 3 つの欄だけを返す（要件 5.1、5.2、5.3）", () => {
    const view = applied({ kind: "expansion", state: { column: 0, expanded: true, depth: 1 } });

    expect(Object.keys(view).sort()).toEqual(["expansion", "filters", "sort"]);
    expect(view.expansion).toEqual([{ column: 0, expanded: true, depth: 1 }]);
    // 空の指定へ足す形は 8.5 の `withExpansion` と同じである（同じ列の指定は置き換わる）。
    const folded = applied({ kind: "expansion", state: { column: 0, expanded: false, depth: 0 } }, view);
    expect(folded.expansion).toEqual([{ column: 0, expanded: false, depth: 0 }]);
  });

  it("行の集合を変えうる指定の同一性が読める（再マウントの合図である）", () => {
    const empty = rowOrderKeyOf(EMPTY_GRID_VIEW);
    const sorted = rowOrderKeyOf(applied({ kind: "sortCycle", column: 1 }));
    const cycled = rowOrderKeyOf(applied({ kind: "sortCycle", column: 1 }, applied({ kind: "sortCycle", column: 1 })));
    const filtered = rowOrderKeyOf(applied({ kind: "filter", column: 1, filter: filterSpecOf("empty", 1) }));

    // **同じ指定からは同じ鍵が出る**（送り直した指定で面を組み直さない）。
    expect(rowOrderKeyOf(applied({ kind: "sortCycle", column: 1 }))).toBe(sorted);
    expect(cycled).not.toBe(sorted);
    expect(filtered).not.toBe(empty);
    expect(empty).not.toBe(sorted);
    // **展開は行の集合を変えない**（列の構成の話である。要件 5.3）ので、鍵は動かない。
    expect(
      rowOrderKeyOf(applied({ kind: "expansion", state: { column: 0, expanded: true, depth: 1 } })),
    ).toBe(empty);
  });
});

// ===========================================================================
// 4. 表示の並びと幅（要件 8.1、8.2。窓の記憶の列の写像と同じ並びである）
// ===========================================================================

describe("表示の並びと幅（要件 8.1、8.2）", () => {
  it("並びを変えていなければ、構成の並びそのものである", () => {
    const display = createDisplayState({ columnCount: LAYOUT.length });

    expect(drawnColumns(LAYOUT, display).map((column) => column.name)).toEqual([
      "名前",
      "数量",
      "備考",
    ]);
  });

  it("表示位置どうしの移動で、描かれる列の並びが入れ替わる", () => {
    const display = createDisplayState({ columnCount: LAYOUT.length });
    display.moveColumn(2, 0);

    expect(drawnColumns(LAYOUT, display).map((column) => column.name)).toEqual([
      "備考",
      "名前",
      "数量",
    ]);
    // **幅は列そのものに付いている**（並べ替えても動かない。7.5 の規則である）。
    display.setColumnWidth(0, 200);
    expect(display.widthAt(0)).toBe(200);
    display.moveColumn(0, 2);
    expect(drawnColumns(LAYOUT, display).map((column) => column.name)).toEqual([
      "名前",
      "数量",
      "備考",
    ]);
    // 「いま位置 2 に居る列」＝ 幅 200 の列である（幅の鍵は列であり、位置ではない）。
    expect(display.widthAt(2)).toBe(200);
  });

  it("展開した構成でも、内側の位置の記述が同じ並びで運ばれる（要件 5.1）", () => {
    const display = createDisplayState({ columnCount: EXPANDED.length });

    expect(drawnColumns(EXPANDED, display).map((column) => column.name)).toEqual([
      "名前.姓",
      "名前.名",
      "数量",
    ]);
    // **文書の列はそのまま運ばれる**（表示の位置ではない。窓の記憶の写像がこれを読む）。
    expect(drawnColumns(EXPANDED, display).map((column) => column.column)).toEqual([0, 0, 1]);
  });

  it("並べ替え・絞り込みの対象は、文書の列ごとに 1 件である（展開した内側を数えない）", () => {
    expect(distinctColumns(LAYOUT).map((column) => column.column)).toEqual([0, 1, 2]);
    // 1 つの文書の列が複数の表示の位置へ展開されていても、対象は 1 件である（最初の出現を残す）。
    expect(distinctColumns(EXPANDED).map((column) => column.column)).toEqual([0, 1]);
    expect(distinctColumns(EXPANDED).map((column) => column.name)).toEqual(["名前.姓", "数量"]);
  });

  it("面を組み直す合図は、幅と並びの内容だけで決まる", () => {
    const display = createDisplayState({ columnCount: LAYOUT.length });
    const opening = layoutKeyOf(display);

    // **変わらない操作**（範囲の外・恒等の移動）では合図も動かない（無駄に組み直さない）。
    display.setColumnWidth(9, 300);
    expect(layoutKeyOf(display)).toBe(opening);
    display.moveColumn(1, 1);
    expect(layoutKeyOf(display)).toBe(opening);

    // 幅を変えると動く。**同じ値へ戻せば元の合図へ戻る**（内容の関数である）。
    display.setColumnWidth(0, 200);
    const widened = layoutKeyOf(display);
    expect(widened).not.toBe(opening);
    display.setColumnWidth(0, 120);
    expect(layoutKeyOf(display)).toBe(opening);

    // 並びを変えると動く。
    display.moveColumn(0, 2);
    expect(layoutKeyOf(display)).not.toBe(opening);
  });
});

// ===========================================================================
// 5. 隠れた行の数の提示（要件 8.7）
// ===========================================================================

describe("隠れた行の数の提示（要件 8.7）", () => {
  it("応答が運んだ数をそのまま名乗る", () => {
    const notice = hiddenRowsNotice({ hiddenRows: 12, filtersActive: true });

    expect(notice).toContain("12");
    // **数の意味を名乗る**（「表示していない行」であり、シートの行数でも窓の行数でもない）。
    expect(notice).toContain("表示していない");
  });

  it("絞り込みが効いていなければ、隠れている行を名乗らない（0 行でも名乗らない）", () => {
    expect(hiddenRowsNotice({ hiddenRows: 0, filtersActive: false })).toBeNull();
  });

  it("絞り込みが効いていれば、隠れが 0 でも名乗る（条件が効いていることが読める）", () => {
    const notice = hiddenRowsNotice({ hiddenRows: 0, filtersActive: true });

    expect(notice).toContain("0");
  });
});

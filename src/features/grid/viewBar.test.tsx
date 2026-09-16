/**
 * 表示の操作の行（tasks.md 8.8。data-grid 要件 8.1、8.2、8.3、8.4、8.7。`./viewBar`）。
 *
 * # 何を固定するか
 *
 * 1. **4 つの操作が画面に出る**（要件 8.1〜8.4）。列ごとに幅の入力と表示位置の移動の 2 つ、
 *    列ごとに並べ替えの操作と絞り込みの条件の選択があり、行の数の提示（要件 8.7）も同じ行に
 *    出る。
 * 2. **列は表示順に並ぶ**（要件 8.2）。行の操作の対象を名指すのは**文書の列**であり、
 *    表示の位置ではない（境界の型がそう定めている）ので、両方を属性に出す — 取り違えは
 *    利用者から見えない（同じ数だからである）。
 * 3. **状態の提示が値と一致する**: 幅は「その位置に描かれる列」の幅、並べ替えは 3 つの状態、
 *    絞り込みは選択肢の現在値、隠れた行の数は応答が運んだ数そのものである。
 * 4. **要素の受け口が、どの操作を組み立てるか**（`node` の環境でも読める。DOM の打鍵そのものは
 *    実物の起動で観測する）。
 *
 * # 環境（`node`）
 *
 * 本成分は**状態を持たない**（`useState` を使わない）ので、`renderToStaticMarkup` に加えて
 * **関数としてそのまま呼び**、返る要素の木の受け口を読める。`jsdom` を足さずに「押したときに
 * 何が起きるか」まで固定できるのはこのためである（`vitest.config.ts` の判断を変えない）。
 */
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import type { ColumnDescriptor, GridViewSpec } from "../../ipc/bindings";
import { createDisplayState, type DisplayState } from "./displayState";
import { EMPTY_GRID_VIEW } from "./gridClient";
import { filterSpecOf, type ViewOperation } from "./viewOps";
import { ViewBar, type ViewBarProps } from "./viewBar";

// ===========================================================================
// 検査の道具
// ===========================================================================

/** 列 1 本ぶんの記述（宣言の順。`column` が**文書の列**である）。 */
function descriptor(column: number, name: string): ColumnDescriptor {
  return { column, path: [], name, kind: "Text", element_count: null, expandability: "leaf" };
}

/** 宣言の列 3 本（列 0..2）。 */
const COLUMNS: readonly ColumnDescriptor[] = [
  descriptor(0, "名前"),
  descriptor(1, "数量"),
  descriptor(2, "備考"),
];

/** 表示位置 0 に文書の列 2 が来る並び（**表示の位置と文書の列を離す**）。 */
const DRAWN: readonly ColumnDescriptor[] = [
  descriptor(2, "備考"),
  descriptor(0, "名前"),
  descriptor(1, "数量"),
];

/** 要素の木を読むための、要素が持ちうる欄（**読む側の最小の形**である）。 */
interface Probe {
  readonly children?: ReactNode;
  readonly "data-testid"?: string;
  /** 属性は**数のまま**要素へ渡る（`renderToStaticMarkup` の側で文字になる）ので、
   *  読み手は [`attribute`] で文字列へ均す。 */
  readonly "data-display-position"?: string | number;
  readonly "data-document-column"?: string | number;
  readonly "data-sort-direction"?: string;
  readonly "data-filter-mode"?: string;
  readonly disabled?: boolean;
  readonly value?: string;
  readonly onClick?: () => void;
  readonly onChange?: (event: { readonly target: { readonly value: string } }) => void;
}

/** 要素の属性を文字列として読む（数で渡った属性も同じ綴りで読める）。 */
function attribute(element: ReactElement, name: keyof Probe): string | undefined {
  const value = (element.props as Probe)[name];
  return value === undefined ? undefined : String(value);
}

/** 要素の木を深さ優先で歩き、`data-testid` が一致する要素を順に集める。 */
function byTestId(root: ReactElement, testId: string): ReactElement[] {
  const found: ReactElement[] = [];
  const visit = (node: ReactNode): void => {
    if (Array.isArray(node)) {
      for (const child of node) {
        visit(child as ReactNode);
      }
      return;
    }
    if (typeof node !== "object" || node === null || !("props" in node)) {
      return;
    }
    const element = node as ReactElement;
    const props = element.props as Probe;
    if (props["data-testid"] === testId) {
      found.push(element);
    }
    visit(props.children);
  };
  visit(root);
  return found;
}

/** その列の面（表示位置で選ぶ）。 */
function columnAt(element: ReactElement, position: number): ReactElement {
  const found = byTestId(element, "jxcel-grid-view-column").find(
    (candidate) => attribute(candidate, "data-display-position") === String(position),
  );
  if (found === undefined) {
    throw new Error(`表示位置 ${String(position)} の列の面が無い`);
  }
  return found;
}

/** その列の中の 1 つの要素（テスト用の識別子で選ぶ）。 */
function inside(column: ReactElement, testId: string): ReactElement {
  const found = byTestId(column, testId)[0];
  if (found === undefined) {
    throw new Error(`列の面に ${testId} が無い`);
  }
  return found;
}

/** 面の要素の木（**関数として呼ぶ**。状態を持たない成分である）。 */
function treeOf(options: {
  readonly columns?: readonly ColumnDescriptor[];
  readonly display?: DisplayState;
  readonly view?: GridViewSpec;
  readonly visibleRows?: number;
  readonly hiddenRows?: number;
  readonly onColumnWidth?: ViewBarProps["onColumnWidth"];
  readonly onColumnMove?: ViewBarProps["onColumnMove"];
  readonly onView?: (operation: ViewOperation) => void;
}): ReactElement {
  const display = options.display ?? createDisplayState({ columnCount: COLUMNS.length });
  return ViewBar({
    columns: options.columns ?? COLUMNS,
    display,
    view: options.view ?? EMPTY_GRID_VIEW,
    visibleRows: options.visibleRows ?? 20,
    hiddenRows: options.hiddenRows ?? 0,
    onColumnWidth: options.onColumnWidth ?? (() => undefined),
    onColumnMove: options.onColumnMove ?? (() => undefined),
    onView: options.onView ?? (() => undefined),
  });
}

/** 面のマーク付け（**画面が実際に DOM へ出すもの**を読む）。 */
function markOf(options: Parameters<typeof treeOf>[0]): string {
  return renderToStaticMarkup(treeOf(options));
}

// ===========================================================================
// 1. 4 つの操作が画面に出る（要件 8.1〜8.4、8.7）
// ===========================================================================

describe("表示の操作の行（要件 8.1〜8.4、8.7）", () => {
  it("列ごとに、幅と表示位置の操作、並べ替え、絞り込みが出る", () => {
    const markup = markOf({});

    expect(markup).toContain("jxcel-grid-view-bar");
    for (const testId of [
      "jxcel-grid-column-width",
      "jxcel-grid-column-left",
      "jxcel-grid-column-right",
      "jxcel-grid-column-sort",
      "jxcel-grid-column-filter",
    ]) {
      expect(markup).toContain(testId);
    }
    // 列の面は 3 本（宣言の列の数である）。
    expect(byTestId(treeOf({}), "jxcel-grid-view-column")).toHaveLength(3);
  });

  it("列は表示順に並び、対象の文書の列が読める（取り違えは見えないので属性に出す）", () => {
    const display = createDisplayState({ columnCount: COLUMNS.length });
    display.moveColumn(2, 0);
    // **画面は表示順の並びを渡す**（`./viewOps` の `drawnColumns` が組む）。本成分は並べ替えを
    // しない — 表示の位置の順がそのまま描かれる順である。
    const tree = treeOf({ columns: DRAWN, display });

    const faces = byTestId(tree, "jxcel-grid-view-column");
    expect(faces.map((face) => attribute(face, "data-document-column"))).toEqual(["2", "0", "1"]);
    expect(faces.map((face) => attribute(face, "data-display-position"))).toEqual(["0", "1", "2"]);
  });

  it("幅は、その位置に描かれる列の幅である（未設定は既定）", () => {
    const display = createDisplayState({ columnCount: COLUMNS.length });
    display.setColumnWidth(1, 200);
    const tree = treeOf({ display });

    const widthOf = (position: number): string | undefined =>
      (inside(columnAt(tree, position), "jxcel-grid-column-width").props as Probe).value;
    expect(widthOf(0)).toBe("120");
    expect(widthOf(1)).toBe("200");
  });

  it("並べ替えの 3 つの状態が、文言と属性に出る（潰さない）", () => {
    const ascending = markOf({ view: { sort: [{ column: 1, descending: false }], filters: [], expansion: [] } });
    expect(ascending).toContain("昇順");
    expect(ascending).toContain('data-sort-direction="ascending"');

    const descending = markOf({ view: { sort: [{ column: 1, descending: true }], filters: [], expansion: [] } });
    expect(descending).toContain("降順");
    expect(descending).toContain('data-sort-direction="descending"');

    const none = markOf({});
    expect(none).toContain('data-sort-direction="none"');
  });

  it("絞り込みの条件が選択肢の現在値になる（往復が崩れない）", () => {
    for (const [mode, text] of [
      ["contains", "あ"],
      ["equals", "3"],
      ["empty", ""],
      ["notEmpty", ""],
      ["violating", ""],
    ] as const) {
      const view: GridViewSpec = {
        sort: [],
        filters: [filterSpecOf(mode, 1, text) ?? { filter: "IsEmpty", column: 1 }],
        expansion: [],
      };
      const filter = inside(columnAt(treeOf({ view }), 1), "jxcel-grid-column-filter");

      expect((filter.props as Probe)["data-filter-mode"]).toBe(mode);
    }
    // 条件の無い列は「なし」である。
    expect(
      (inside(columnAt(treeOf({}), 2), "jxcel-grid-column-filter").props as Probe)[
        "data-filter-mode"
      ],
    ).toBe("none");
  });

  it("絞り込みの文字列は、条件が持つ文字列である（画面が別の値を持たない）", () => {
    const view: GridViewSpec = {
      sort: [],
      filters: [filterSpecOf("contains", 1, "あ") ?? { filter: "IsEmpty", column: 1 }],
      expansion: [],
    };

    expect(
      (inside(columnAt(treeOf({ view }), 1), "jxcel-grid-column-filter-text").props as Probe).value,
    ).toBe("あ");
  });
});

// ===========================================================================
// 2. 隠れた行の数の提示（要件 8.7）
// ===========================================================================

describe("隠れた行の数の提示（要件 8.7）", () => {
  it("応答が運んだ数を、そのまま出す", () => {
    const markup = markOf({
      view: { sort: [], filters: [filterSpecOf("empty", 0) ?? { filter: "IsEmpty", column: 0 }], expansion: [] },
      visibleRows: 4,
      hiddenRows: 7,
    });

    expect(markup).toContain("jxcel-grid-row-visibility");
    expect(markup).toContain('data-hidden-rows="7"');
    expect(markup).toContain('data-visible-rows="4"');
    expect(markup).toContain("7");
    expect(markup).toContain("表示していない");
  });

  it("絞り込みが効いていなければ、隠れた行を名乗らない", () => {
    const tree = treeOf({ visibleRows: 20, hiddenRows: 0 });

    expect(byTestId(tree, "jxcel-grid-hidden-rows")).toHaveLength(0);
    expect(markOf({ visibleRows: 20, hiddenRows: 0 })).not.toContain("表示していない");
  });
});

// ===========================================================================
// 3. 要素の受け口（どの操作を組み立てるか）
// ===========================================================================

describe("要素の受け口（要件 8.1〜8.4）", () => {
  it("幅の入力は、正の数だけを列の幅として送る", () => {
    const onColumnWidth = vi.fn();
    const tree = treeOf({
      display: createDisplayState({ columnCount: COLUMNS.length }),
      onColumnWidth,
    });
    const input = inside(columnAt(tree, 0), "jxcel-grid-column-width");

    // 幅の入力は**表示位置**で送る（画面の遷移が同じ空間で受ける）。
    (input.props as Probe).onChange?.({ target: { value: "240" } });
    expect(onColumnWidth).toHaveBeenCalledWith(0, 240);

    onColumnWidth.mockClear();
    for (const value of ["", "abc", "-5", "0"]) {
      (input.props as Probe).onChange?.({ target: { value } });
    }
    expect(onColumnWidth).not.toHaveBeenCalled();
  });

  it("移動の操作は、隣の表示位置と入れ替える（端では何もしない）", () => {
    const onColumnMove = vi.fn();
    const tree = treeOf({ onColumnMove });

    (inside(columnAt(tree, 0), "jxcel-grid-column-right").props as Probe).onClick?.();
    expect(onColumnMove).toHaveBeenCalledWith(0, 1);

    onColumnMove.mockClear();
    (inside(columnAt(tree, 2), "jxcel-grid-column-left").props as Probe).onClick?.();
    expect(onColumnMove).toHaveBeenCalledWith(2, 1);

    // **端では押しても何も起きない**（範囲の外の移動は状態を変えない。7.5 の規律）。
    onColumnMove.mockClear();
    (inside(columnAt(tree, 0), "jxcel-grid-column-left").props as Probe).onClick?.();
    (inside(columnAt(tree, 2), "jxcel-grid-column-right").props as Probe).onClick?.();
    expect(onColumnMove).not.toHaveBeenCalled();
  });

  it("並べ替えの操作は、押された列を**文書の列**で名指す（表示の位置ではない）", () => {
    const onView = vi.fn();
    // 表示位置 0 に文書の列 2 が描かれている（取り違えれば 0 が送られる）。
    const display = createDisplayState({ columnCount: COLUMNS.length });
    display.moveColumn(2, 0);
    const tree = treeOf({ columns: DRAWN, display, onView });

    (inside(columnAt(tree, 0), "jxcel-grid-column-sort").props as Probe).onClick?.();

    expect(onView).toHaveBeenCalledWith({ kind: "sortCycle", column: 2 });
  });

  it("絞り込みの選択は、条件を設定し・外す（文字列は入力欄の値を使う）", () => {
    const onView = vi.fn();
    const tree = treeOf({ onView });
    const select = inside(columnAt(tree, 1), "jxcel-grid-column-filter");

    (select.props as Probe).onChange?.({ target: { value: "contains" } });
    expect(onView).toHaveBeenCalledWith({
      kind: "filter",
      column: 1,
      filter: { filter: "Contains", column: 1, text: "" },
    });

    onView.mockClear();
    (select.props as Probe).onChange?.({ target: { value: "none" } });
    expect(onView).toHaveBeenCalledWith({ kind: "filter", column: 1, filter: null });

    onView.mockClear();
    (select.props as Probe).onChange?.({ target: { value: "violating" } });
    expect(onView).toHaveBeenCalledWith({
      kind: "filter",
      column: 1,
      filter: { filter: "HasViolation", column: 1 },
    });
  });

  it("絞り込みの文字列の入力は、いまの種類の条件を文字列だけ替えて送る", () => {
    const onView = vi.fn();
    const view: GridViewSpec = {
      sort: [],
      filters: [filterSpecOf("equals", 1, "3") ?? { filter: "IsEmpty", column: 1 }],
      expansion: [],
    };
    const tree = treeOf({ view, onView });

    (inside(columnAt(tree, 1), "jxcel-grid-column-filter-text").props as Probe).onChange?.({
      target: { value: "4" },
    });

    expect(onView).toHaveBeenCalledWith({
      kind: "filter",
      column: 1,
      filter: { filter: "Equals", column: 1, text: "4" },
    });
  });
});

// ===========================================================================
// 4. 配色（器のカスタムプロパティだけを参照する）
// ===========================================================================

describe("配色（画面の契約）", () => {
  it("色の値そのものを 1 つも書かない", () => {
    const markup = markOf({});

    expect(markup).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(markup).not.toContain("rgb(");
    expect(markup).not.toContain("hsl(");
  });
});

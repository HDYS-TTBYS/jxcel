/**
 * 入れ子の列の操作と詳細表示（tasks.md 8.5。data-grid 要件 4.5、5.1〜5.7）。
 *
 * # ここで固定するもの
 *
 * 1. **展開の状態は列ごとに 1 つであり、指定は完全な記述である**（要件 5.1、5.2、5.3）:
 *    展開は `GridViewSpec.expansion` に載り、**別の列を展開しても前の展開が残る**（要求から
 *    消えた展開はドメインが折りたたみへ戻す — `answer_set_view` の規約）
 * 2. **列ごとの操作が記述から導かれる**（要件 5.1、5.2、5.4、5.6）: 展開できる列には
 *    展開・折りたたみ、段数の上限に達した列には**詳細表示へ**、同一の型の並びには**要素数**。
 *    **画面は型の札で分岐しない** — 記述の印（`expandability` / `element_count`）だけを見る
 * 3. **詳細表示は違反している内側の位置を特定できる形で示す**（要件 4.5）: `a.b` / `tags[2]` /
 *    「セル直下」を書き分け、**未取得（`null`）を「違反なし」と混同しない**
 * 4. **要素数の宣言と、そのセルの要約を混同しない**（要件 5.6）: 宣言（`element_count`）は
 *    範囲と要素の型であり、セルの値は窓が運ぶ要約である
 * 5. **編集の面は登録簿から引き、運び手もそこから来る**（要件 5.5、5.7、10.3）: 画面にも本
 *    module にも型ごとの分岐は無い
 *
 * # ここで固定しないもの（**正直に書く**）
 *
 * - **値そのものの構造は境界に読む経路が無い**（`design.md` の「8.5 が記録した申し送り」）。
 *    したがって「構造の全体を各フィールドの型とともに示す」（要件 5.5）は、**構成が晒す範囲の
 *    宣言**までである。詳細表示はその事実を利用者にも示す（値を空として見せない）
 * - `commit` の先（判定・違反の保持・取り直し）は `./cellEdit` の検査である（同じ 1 つの関数を
 *    通る。要件 5.7 の「同じ規律」は経路が 1 つであることで満たす）
 */
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type {
  ColumnDescriptor,
  GridExpansionState,
  GridPathSegment,
  GridViewSpec,
  TypeKindTag,
} from "../../ipc/bindings";
import { EMPTY_GRID_VIEW } from "./gridClient";
import type { NestedSegment } from "./windowCache";
import {
  NestedInspector,
  declaredInnerPositions,
  expandedColumnsIn,
  innerPathText,
  isColumnExpanded,
  nestedColumnControls,
  withExpansion,
} from "./nestedInspector";
import type { CellPosition } from "./renderer/port";

// ===========================================================================
// 道具
// ===========================================================================

/** 内側の位置の 1 段（フィールド名）。 */
function field(name: string): GridPathSegment {
  return { segment: "Field", name };
}

/** 構成の 1 列（必須の欄をすべて埋める）。 */
function descriptor(
  column: number,
  path: readonly GridPathSegment[],
  name: string,
  kind: TypeKindTag | null,
  options: {
    readonly expandability?: ColumnDescriptor["expandability"];
    readonly elementCount?: ColumnDescriptor["element_count"];
  } = {},
): ColumnDescriptor {
  return {
    column,
    path: [...path],
    name,
    kind,
    element_count: options.elementCount ?? null,
    expandability: options.expandability ?? "leaf",
  };
}

/** 窓の違反の札（内側の位置）。 */
function mark(...segments: readonly NestedSegment[]): readonly NestedSegment[] {
  return segments;
}

/** 詳細表示の既定の入力（主題ごとに上書きする）。 */
function inspectorProps(overrides: {
  readonly column?: ColumnDescriptor | null;
  readonly declared?: readonly ColumnDescriptor[];
  readonly summary?: string;
  readonly loading?: boolean;
  readonly innerViolations?: readonly (readonly NestedSegment[])[] | null;
  readonly committed?: (text: string, carrier: string) => void;
  readonly cancelled?: () => void;
  readonly closed?: () => void;
}): Parameters<typeof NestedInspector>[0] {
  const position: CellPosition = { row: 4, column: 1 };
  return {
    position,
    column:
      overrides.column === undefined
        ? descriptor(1, [], "提供元", "Object", { expandability: "available" })
        : overrides.column,
    declared: overrides.declared ?? [],
    // **`null` も指定である**（未取得）。`??` で既定へ畳むと、未取得の検査が「違反なし」を
    // 見ることになる（この検査がまさに固定しようとしている混同である）。
    summary: overrides.summary === undefined ? "2項目" : overrides.summary,
    loading: overrides.loading === undefined ? false : overrides.loading,
    innerViolations:
      overrides.innerViolations === undefined ? [] : overrides.innerViolations,
    editKey: 0,
    onCommit: (text, carrier) => {
      overrides.committed?.(text, carrier);
    },
    onCancel: () => {
      overrides.cancelled?.();
    },
    onClose: () => {
      overrides.closed?.();
    },
  };
}

/** 詳細表示を描いたマーク付け。 */
function renderInspector(overrides: Parameters<typeof inspectorProps>[0] = {}): string {
  return renderToStaticMarkup(createElement(NestedInspector, inspectorProps(overrides)));
}

/** `data-inner-violation` の値を順に読む。 */
function innerViolationsIn(markup: string): readonly string[] {
  return [...markup.matchAll(/data-inner-violation="([^"]*)"/g)].map((match) => match[1] ?? "");
}

/** `data-declared-path` の値を順に読む。 */
function declaredIn(markup: string): readonly string[] {
  return [...markup.matchAll(/data-declared-path="([^"]*)"/g)].map((match) => match[1] ?? "");
}

// ===========================================================================
// 1. 展開の状態（要件 5.1、5.2、5.3）
// ===========================================================================

describe("展開の状態（要件 5.1、5.2、5.3）", () => {
  /** 展開の指定（1 段）。 */
  function expanded(column: number): GridExpansionState {
    return { column, expanded: true, depth: 1 };
  }

  it("展開は列ごとに 1 件であり、折りたたみは指定として残る", () => {
    const opened = withExpansion(EMPTY_GRID_VIEW, expanded(2));

    // **展開は指定であって「指定が無い」ではない**（要件 5.3 は走査で失われないことを求める）。
    expect(opened.expansion).toEqual([{ column: 2, expanded: true, depth: 1 }]);
    expect(isColumnExpanded(opened, 2)).toBe(true);

    // 折りたたみも**指定として残す**（消すと、列ごとの状態が「指定が無い」と同じ形になり、
    // 「折りたたまれている」と「まだ触っていない」の区別が画面から失われる）。
    const closed = withExpansion(opened, { column: 2, expanded: false, depth: 0 });
    expect(closed.expansion).toEqual([{ column: 2, expanded: false, depth: 0 }]);
    expect(isColumnExpanded(closed, 2)).toBe(false);
  });

  it("別の列を展開しても、前の列の展開が残る（指定は完全な記述である）", () => {
    // ドメインは**要求に現れない展開を折りたたみへ戻す**（`answer_set_view` の規約）。
    // したがって画面は、押された列だけを載せた指定を送ってはならない — 送ると、前に展開した
    // 列が黙って折りたたまれる（要件 5.3「列ごとに保持し、走査によって失われない」）。
    const both = withExpansion(withExpansion(EMPTY_GRID_VIEW, expanded(0)), expanded(3));

    expect(both.expansion).toEqual([
      { column: 0, expanded: true, depth: 1 },
      { column: 3, expanded: true, depth: 1 },
    ]);
    expect(isColumnExpanded(both, 0)).toBe(true);
    expect(isColumnExpanded(both, 3)).toBe(true);
  });

  it("同じ列の指定は置き換わる（後ろが勝つ規則に頼らない）", () => {
    const first = withExpansion(EMPTY_GRID_VIEW, expanded(1));
    const deeper = withExpansion(first, { column: 1, expanded: true, depth: 2 });
    const closed = withExpansion(deeper, { column: 1, expanded: false, depth: 0 });

    expect(deeper.expansion).toEqual([{ column: 1, expanded: true, depth: 2 }]);
    expect(closed.expansion).toEqual([{ column: 1, expanded: false, depth: 0 }]);
  });

  it("並べ替えと絞り込みの指定を保つ（展開だけを変える）", () => {
    const view: GridViewSpec = {
      sort: [{ column: 0, descending: true }],
      filters: [{ filter: "IsNotEmpty", column: 1 }],
      expansion: [],
    };

    const toggled = withExpansion(view, expanded(2));

    expect(toggled.sort).toEqual(view.sort);
    expect(toggled.filters).toEqual(view.filters);
  });

  it("展開している列を列ごとに数え上げる（走査で失われないことの観測）", () => {
    const view = withExpansion(withExpansion(EMPTY_GRID_VIEW, expanded(0)), expanded(5));

    expect(expandedColumnsIn(view)).toEqual([0, 5]);
    // 折りたたんだ列は数えない（`expanded: false` は「展開している」ではない）。
    expect(expandedColumnsIn(withExpansion(view, { column: 0, expanded: false, depth: 0 }))).toEqual([
      5,
    ]);
  });
});

// ===========================================================================
// 2. 列ごとの操作（要件 5.1、5.2、5.4、5.6）
// ===========================================================================

describe("列ごとの操作（要件 5.1、5.2、5.4、5.6）", () => {
  /**
   * 4 列の構成である。
   *
   * | 表示の位置 | 記述 | 操作 |
   * |---|---|---|
   * | 0 | 葉（`Text`） | 無し |
   * | 1 | 展開できる（`available`） | 展開 / 折りたたむ |
   * | 2 | 段数の上限に達している（`capped`） | **詳細表示へ** |
   * | 3 | 同一の型の並び（`element_count`） | 詳細表示（要素数を示す） |
   */
  const COLUMNS: readonly ColumnDescriptor[] = [
    descriptor(0, [], "名前", "Text"),
    descriptor(1, [], "提供元", "Object", { expandability: "available" }),
    descriptor(2, [], "深い入れ子", "Object", { expandability: "capped" }),
    descriptor(3, [], "明細", "Array", {
      elementCount: { items: "Int", min: 1, max: 8 },
    }),
  ];

  it("展開できる列には、展開と折りたたみの操作が出る", () => {
    const controls = nestedColumnControls(COLUMNS, EMPTY_GRID_VIEW);

    expect(controls.map((entry) => entry.display)).toEqual([0, 1, 2, 3]);
    expect(controls[0]?.expansion).toBeNull();
    expect(controls[1]?.expansion).toEqual({ column: 1, expanded: true, depth: 1 });
    expect(controls[1]?.expansionLabel).toBe("展開");
    // 展開したあとは「折りたたむ」になる（同じ操作の 2 つの向きである）。
    const opened = withExpansion(EMPTY_GRID_VIEW, { column: 1, expanded: true, depth: 1 });
    expect(nestedColumnControls(COLUMNS, opened)[1]?.expansion).toEqual({
      column: 1,
      expanded: false,
      depth: 0,
    });
    expect(nestedColumnControls(COLUMNS, opened)[1]?.expansionLabel).toBe("折りたたむ");
  });

  it("段数の上限に達した列は、詳細表示へ誘導する（要件 5.4）", () => {
    const controls = nestedColumnControls(COLUMNS, EMPTY_GRID_VIEW);

    // **展開の操作は出さない**（それ以上降りられないので、押しても何も起きない操作を作らない）。
    expect(controls[2]?.expansion).toBeNull();
    expect(controls[2]?.expansionLabel).toBeNull();
    expect(controls[2]?.detail).toBe("詳細表示へ");
    // 展開できる列にも詳細表示の入口を出す（値の構造を見る道は展開とは別である）。
    expect(controls[1]?.detail).toBe("詳細表示");
    // 葉の列には出さない（内側が無いので見せる構造が無い）。
    expect(controls[0]?.expansion).toBeNull();
    expect(controls[0]?.detail).toBeNull();
  });

  it("内側の位置（展開された列）の展開は、段数を 1 つ深くする", () => {
    // 展開すると構成の並びは内側の位置になる。**位置が空でないことは「葉である」を意味しない**
    // — 内側の位置がまた入れ子であれば、さらに降りられる（要件 5.1）。ドメインの `depth` は
    // **内側へ降りる段数**である（`view` 層の `ExpansionState`）ので、2 段目を開く指定は
    // `depth: 2` である。
    const columns = [
      descriptor(0, [field("city")], "place.city", "Object", { expandability: "available" }),
    ];
    const opened = withExpansion(EMPTY_GRID_VIEW, { column: 0, expanded: true, depth: 1 });

    const control = nestedColumnControls(columns, opened)[0];

    expect(control?.expansion).toEqual({ column: 0, expanded: true, depth: 2 });
    expect(control?.expansionLabel).toBe("さらに展開");
    // 深さ 1 の位置は**親の列（文書の列 0）**の展開である（`ExpansionState.column` は最上位の列）。
    expect(control?.descriptor.column).toBe(0);
  });

  it("同一の型の並びには、要素数の宣言を出す（要件 5.6）", () => {
    const controls = nestedColumnControls(COLUMNS, EMPTY_GRID_VIEW);

    expect(controls[3]?.elementCount).toBe("要素数: 1..=8（要素の型: Int）");
    // 並びでない列には出さない。
    expect(controls[0]?.elementCount).toBeNull();
    // 上限・下限の宣言が無い並びも、並びであることは示す。
    expect(
      nestedColumnControls(
        [descriptor(0, [], "自由な並び", "Array", { elementCount: { items: "Text", min: null, max: null } })],
        EMPTY_GRID_VIEW,
      )[0]?.elementCount,
    ).toBe("要素数: 宣言なし（要素の型: Text）");
    expect(
      nestedColumnControls(
        [descriptor(0, [], "下限だけ", "Array", { elementCount: { items: "Any", min: 2, max: null } })],
        EMPTY_GRID_VIEW,
      )[0]?.elementCount,
    ).toBe("要素数: 2 以上（要素の型: Any）");
  });

  it("内側の位置にも、要素数の規則を当てる", () => {
    const columns = [
      descriptor(0, [field("tags")], "place.tags", "Array", {
        elementCount: { items: "Text", min: null, max: null },
      }),
    ];

    expect(nestedColumnControls(columns, EMPTY_GRID_VIEW)[0]?.elementCount).toBe(
      "要素数: 宣言なし（要素の型: Text）",
    );
  });
});

// ===========================================================================
// 3. 違反している内側の位置（要件 4.5）
// ===========================================================================

describe("違反している内側の位置（要件 4.5）", () => {
  it("フィールドと並びの位置を、人が読める形で書き分ける", () => {
    expect(innerPathText(mark({ kind: "field", name: "a" }))).toBe("a");
    expect(
      innerPathText(mark({ kind: "field", name: "a" }, { kind: "field", name: "b" })),
    ).toBe("a.b");
    expect(
      innerPathText(mark({ kind: "field", name: "tags" }, { kind: "index", index: 2 })),
    ).toBe("tags[2]");
    expect(innerPathText(mark({ kind: "index", index: 0 }))).toBe("[0]");
    // 位置が空であることは**セル直下**の違反である（生成物の `GridViolationLocation.path` と
    // 同じ規約）。
    expect(innerPathText(mark())).toBe("");
  });

  it("違反している位置を 1 件 1 行で示す", () => {
    const markup = renderInspector({
      innerViolations: [
        mark({ kind: "field", name: "tags" }, { kind: "index", index: 2 }),
        mark({ kind: "field", name: "a" }, { kind: "field", name: "b" }),
      ],
    });

    expect(markup).toContain("jxcel-grid-nested-violations");
    expect(innerViolationsIn(markup)).toEqual(["tags[2]", "a.b"]);
    expect(markup).toContain("違反している内側の位置");
  });

  it("セル直下の違反は「セル直下」と書く（空文字を並べない）", () => {
    const markup = renderInspector({ innerViolations: [mark()] });

    // 位置が空の札は 1 件ある（**「違反なし」ではない**）が、並べる文字は「セル直下」である。
    expect(innerViolationsIn(markup)).toEqual([""]);
    expect(markup).toContain("セル直下");
  });

  it("未取得と「違反なし」を混同しない", () => {
    // **未取得は「違反していない」ではない**（8.4 が窓の印を門番にしたのと同じ規律）。
    const unknown = renderInspector({ innerViolations: null });
    const none = renderInspector({ innerViolations: [] });

    expect(unknown).toContain("まだ届いていない");
    expect(unknown).not.toContain("違反はありません");
    expect(none).toContain("違反はありません");
  });
});

// ===========================================================================
// 4. 値の要約と、宣言から分かる構造（要件 5.5、5.6）
// ===========================================================================

describe("値の要約と、宣言から分かる構造（要件 5.5、5.6）", () => {
  it("値の要約は窓が運んだ文字をそのまま示す（値なし・読み込み中と区別する）", () => {
    expect(renderInspector({ summary: "3項目" })).toContain("3項目");
    // 値なし（空文字）は**空として示す**（読み込み中と同じ見た目にしない）。
    const none = renderInspector({ summary: "" });
    expect(none).toContain("値なし");
    expect(renderInspector({ summary: "", loading: true })).toContain("読み込み中");
  });

  it("構成が晒している内側の位置を、型とともに示す", () => {
    const markup = renderInspector({
      declared: [
        descriptor(1, [field("city")], "place.city", "Text"),
        descriptor(1, [field("count")], "place.count", "Int"),
      ],
    });

    expect(markup).toContain("jxcel-grid-nested-declared");
    expect(declaredIn(markup)).toEqual(["place.city", "place.count"]);
    expect(markup).toContain("Text");
    expect(markup).toContain("Int");
  });

  it("内側の宣言が構成に無いときは、その事実を示す（黙って空にしない）", () => {
    // **値そのものの構造は境界に読む経路が無い**（本 module の申し送り）。空の一覧を出すだけだと
    // 「内側が無い値」と読めてしまうので、読めないことを書く。
    const markup = renderInspector({ declared: [] });

    expect(markup).toContain("宣言が読めません");
    expect(markup).not.toContain("jxcel-grid-nested-declared");
  });

  it("内側の宣言は、その文書の列の位置だけから集める", () => {
    // 構成の並びには**別の文書の列の位置**も混ざっている（展開した列の隣には、折りたたまれた
    // 列が並ぶ）。同じ文書の列の位置だけを集める（そうしないと、隣の列のフィールドを自分の
    // 構造として見せることになる）。
    const columns = [
      descriptor(1, [field("city")], "place.city", "Text"),
      descriptor(2, [], "名前", "Text"),
      descriptor(1, [field("zip")], "place.zip", "Text"),
    ];

    const declared = declaredInnerPositions(columns, 1);

    expect(declared.map((column) => column.name)).toEqual(["place.city", "place.zip"]);
    // 折りたたまれた列そのもの（位置が空）は内側の位置ではない。
    expect(declaredInnerPositions(columns, 2)).toEqual([]);
  });

  it("同一の型の並びの列では、要素数の宣言を示す（要件 5.6）", () => {
    const markup = renderInspector({
      column: descriptor(1, [], "明細", "Array", {
        elementCount: { items: "Int", min: 1, max: 8 },
      }),
    });

    expect(markup).toContain("jxcel-grid-nested-element-count");
    expect(markup).toContain("要素数: 1..=8（要素の型: Int）");
  });
});

// ===========================================================================
// 5. 詳細表示の中の編集（要件 5.5、5.7、10.3）
// ===========================================================================

describe("詳細表示の中の編集（要件 5.5、5.7、10.3）", () => {
  it("列の札に対応する面を、登録簿から引いて出す", () => {
    // **本 module は面を名指ししない**（要件 10.3）。`Object` の札は登録簿の入れ子の面であり、
    // 位置ごとの面を持たない（`members` が境界に無い）ので既定の文字の面へ落ちる。
    const markup = renderInspector({
      column: descriptor(1, [], "提供元", "Object", { expandability: "available" }),
    });

    expect(markup).toContain("jxcel-grid-nested-editor");
    expect(markup).toContain('data-detail-kind="Object"');
    // **運び手は登録が宣言したものである**（入れ子の構造表現は `SetNested` へ載る）。
    expect(markup).toContain('data-detail-carrier="structure"');
  });

  it("文字の列では、打たれた文字の運び手になる", () => {
    const markup = renderInspector({ column: descriptor(1, [], "名前", "Text") });

    expect(markup).toContain('data-detail-kind="Text"');
    expect(markup).toContain('data-detail-carrier="text"');
  });

  it("札が読めない列は Any として登録簿へ来る（面は必ず出る）", () => {
    const markup = renderInspector({ column: descriptor(1, [], "壊れた列", null) });

    expect(markup).toContain('data-detail-kind="Any"');
    expect(markup).toContain("<textarea");
  });

  it("値の全体を置き換えることを、編集の前に書く", () => {
    // 構造表現は**セルの値そのもの**である（部分的な更新の口は境界に無い）。これを書かないと、
    // 利用者は内側の 1 つを直すつもりで値の全体を置き換える。
    const markup = renderInspector({});

    expect(markup).toContain("値の全体");
    expect(markup).toContain("構造表現");
  });

  it("見出しは、どの位置の入れ子かを 1 起点で名乗る", () => {
    const markup = renderInspector({});

    expect(markup).toContain("jxcel-grid-nested-inspector");
    expect(markup).toContain('data-detail-row="4"');
    expect(markup).toContain('data-detail-column="1"');
    // **利用者に見える数は 1 起点である**（内部の序数は 0 起点）。
    expect(markup).toContain("5 行 2 列");
    expect(markup).toContain("jxcel-grid-nested-close");
  });
});

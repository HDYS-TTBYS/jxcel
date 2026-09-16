/**
 * 列の 2 つの空間の写像（tasks.md 8.5。data-grid 要件 5.1、8.6）。
 *
 * # ここで固定するもの
 *
 * 1. **展開が無い構成では、表示の位置がそのまま文書の列である**（恒等。8.1〜8.4 が依拠して
 *    きた前提そのもの）
 * 2. **展開した構成では、表示の位置が文書の列から離れる**（`crates/data-grid/src/view/mod.rs`
 *    の `push_column` は、展開したオブジェクトの**内側の位置**を同じ文書の列の下へ並べる）
 * 3. **範囲の外・整数でない表示の位置では答えない**（`null`。**推測で列を答えると、別の列を
 *    読む／書く** — 要件 8.6 の取り違えである。`WindowCache.rowId` が `null` で「まだ無い」を
 *    表すのと同じ規律）
 * 4. **札は表示の位置の順であり、内側の位置はその位置の葉の型を持つ**（描き手と入力手段が
 *    読む札。`kind` が `null` の列は `"Any"` へ落ちる — 7.4 の既定）
 *
 * # ここで固定しないもの
 *
 * - **窓がどの列を運ぶか**は `crates/data-grid/src/transport/mod.rs` の契約である（宣言の列数。
 *   本 module は「表示の位置はどの文書の列を指すか」だけを答える）
 */
import { describe, expect, it } from "vitest";

import type { ColumnDescriptor, GridPathSegment, TypeKindTag } from "../../ipc/bindings";
import { createColumnSpace } from "./columnSpace";

// ===========================================================================
// 道具
// ===========================================================================

/** 内側の位置の 1 段（フィールド名）。 */
function field(name: string): GridPathSegment {
  return { segment: "Field", name };
}

/** 内側の位置の 1 段（並びの添字）。 */
function index(position: number): GridPathSegment {
  return { segment: "Index", position };
}

/** 構成の 1 列（`ColumnDescriptor` の必須の欄をすべて埋める）。 */
function descriptor(
  column: number,
  path: readonly GridPathSegment[],
  name: string,
  kind: TypeKindTag | null,
  expandability: ColumnDescriptor["expandability"] = "leaf",
): ColumnDescriptor {
  return { column, path: [...path], name, kind, element_count: null, expandability };
}

// ===========================================================================
// 1. 恒等（展開が無い構成）
// ===========================================================================

describe("展開が無い構成（恒等）", () => {
  it("表示の位置がそのまま文書の列である", () => {
    const space = createColumnSpace([
      descriptor(0, [], "名前", "Text"),
      descriptor(1, [], "数量", "Int"),
      descriptor(2, [], "提供元", "Text"),
    ]);

    expect(space.documentColumn(0)).toBe(0);
    expect(space.documentColumn(1)).toBe(1);
    expect(space.documentColumn(2)).toBe(2);
    expect(space.variants).toEqual(["Text", "Int", "Text"]);
  });
});

// ===========================================================================
// 2. 展開（2 つの空間が離れる）
// ===========================================================================

describe("展開した構成（2 つの空間が離れる）", () => {
  /**
   * 宣言は 3 列（`place` / `name` / `total`）であり、列 0 の `place` を展開した構成である。
   * 構成の並びは `place.city` / `place.zip` / `name` / `total` であり、**表示の位置 1 と 2 が
   * 同じ文書の列（0）を指す** — この構成で恒等を仮定すると、`name` を読むつもりで `place` の
   * セルを読む（要件 8.6 の取り違え）。
   */
  const EXPANDED: readonly ColumnDescriptor[] = [
    descriptor(0, [field("city")], "place.city", "Text"),
    descriptor(0, [index(0)], "place.tags[0]", "Text"),
    descriptor(1, [], "name", "Text"),
    descriptor(2, [], "total", "Int"),
  ];

  it("内側の位置は、複数あっても同じ文書の列を指す", () => {
    const space = createColumnSpace(EXPANDED);

    expect(space.documentColumn(0)).toBe(0);
    expect(space.documentColumn(1)).toBe(0);
    expect(space.documentColumn(2)).toBe(1);
    expect(space.documentColumn(3)).toBe(2);
  });

  it("札は表示の位置の順であり、内側の位置はその位置の葉の型を持つ", () => {
    const space = createColumnSpace(EXPANDED);

    // 内側の位置は**葉の型**を持つ（`place` 全体の型ではない）— 要件 5.1 の「内側のフィールドを
    // 列として展開する」の帰結であり、入力手段はこの札で選ばれる（7.4）。
    expect(space.variants).toEqual(["Text", "Text", "Text", "Int"]);
  });

  it("札が読めない列は Any へ落ちる（7.4 の既定）", () => {
    const space = createColumnSpace([descriptor(0, [], "壊れた列", null)]);

    expect(space.variants).toEqual(["Any"]);
  });
});

// ===========================================================================
// 3. 範囲の外（推測しない）
// ===========================================================================

describe("答えられない表示の位置", () => {
  const space = createColumnSpace([descriptor(0, [], "名前", "Text")]);

  it("列の数の外・負・整数でない位置では答えない", () => {
    // **推測した列は、別の列を読む／書く経路である**（要件 8.6）。
    expect(space.documentColumn(1)).toBeNull();
    expect(space.documentColumn(-1)).toBeNull();
    expect(space.documentColumn(0.5)).toBeNull();
    expect(space.documentColumn(Number.NaN)).toBeNull();
  });

  it("列が 1 本も無い構成でも投げない（列 0 本のシートは表を描かないが、構成は届く）", () => {
    const empty = createColumnSpace([]);

    expect(empty.documentColumn(0)).toBeNull();
    expect(empty.variants).toEqual([]);
  });
});

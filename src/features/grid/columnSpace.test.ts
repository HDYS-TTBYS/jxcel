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

// ===========================================================================
// 4. 逆向き（文書の列 + 内側の位置 → 表示の位置。8.4 の提示が使う）
// ===========================================================================

describe("文書の列から表示の位置へ（逆向き）", () => {
  /**
   * 実測に使った構成である（表示の位置 1・2 が同じ文書の列 1 を指す）。境界の修復（入れ子の
   * 展開）が描かれる並びを変えたため、**逆行きは関数ではない** — 内側の位置が無ければ
   * 答えられない。
   */
  const EXPANDED: readonly ColumnDescriptor[] = [
    descriptor(0, [], "名前", "Text"),
    descriptor(1, [field("name")], "提供元.name", "Text"),
    descriptor(1, [field("code")], "提供元.code", "Text"),
    descriptor(2, [], "深い入れ子", "Text"),
    descriptor(3, [], "明細", "Text"),
  ];

  it("展開が無ければ、文書の列がそのまま表示の位置である", () => {
    const space = createColumnSpace([
      descriptor(0, [], "名前", "Text"),
      descriptor(1, [], "数量", "Int"),
      descriptor(2, [], "提供元", "Text"),
    ]);

    expect(space.displayPosition(0, [])).toBe(0);
    expect(space.displayPosition(2, [])).toBe(2);
  });

  it("内側の位置で解く（同じ文書の列を指す 2 つの位置を選び分ける）", () => {
    const space = createColumnSpace(EXPANDED);

    expect(space.displayPosition(1, [field("name")])).toBe(1);
    expect(space.displayPosition(1, [field("code")])).toBe(2);
    // 4 番目に描かれる列（深い入れ子）は文書の列 2 である。
    expect(space.displayPosition(2, [])).toBe(3);
    expect(space.displayPosition(3, [])).toBe(4);
  });

  it("内側の位置が無ければ、展開された列では答えない（推測しない）", () => {
    const space = createColumnSpace(EXPANDED);

    // 表示の位置 1 と 2 のどちらも文書の列 1 を表示している。**最初の候補を返さない** —
    // 返すと、違反していないセルを違反として名乗る／そこへ現在位置を動かす経路になる。
    expect(space.displayPosition(1)).toBeNull();
    expect(space.displayPosition(1, [])).toBeNull();
  });

  it("折りたたんだ列は、内側の位置の違反も表示している（空の位置が祖先である）", () => {
    const space = createColumnSpace([
      descriptor(0, [], "名前", "Text"),
      descriptor(1, [], "提供元", "Object"),
    ]);

    // 展開が無ければ内側の位置は描かれないが、その値は 1 本の列に要約として載っている。
    expect(space.displayPosition(1, [field("inner")])).toBe(1);
    expect(space.displayPosition(1, [field("inner"), index(0), field("deep")])).toBe(1);
  });

  it("描かれた列より深い違反は、その値を表示している列へ落ちる", () => {
    const space = createColumnSpace(EXPANDED);

    // 表示の位置 1 は「提供元.name」であり、その内側（さらに深い位置）の違反もそこで見える。
    expect(space.displayPosition(1, [field("name"), field("first")])).toBe(1);
    // 段の種類まで見る（同じ綴りでも添字は別の段である）。
    expect(space.displayPosition(1, [field("name"), index(0)])).toBe(1);
  });

  it("段が 1 つでも食い違えば祖先ではない（別の枝の違反へ答えない）", () => {
    const space = createColumnSpace(EXPANDED);

    // 提供元（文書の列 1）の内側は name と code だけである。`zip` は構成に無い枝であり、
    // **どの列も表示していない**（名乗れる位置が無い）。
    expect(space.displayPosition(1, [field("zip")])).toBeNull();
    // 段が食い違えば祖先ではない（`code` は `zip` の祖先ではない）。
    expect(space.displayPosition(1, [field("zip"), field("code")])).toBeNull();
    expect(space.displayPosition(1, [field("name"), field("zip")])).toBe(1);
  });

  it("構成に無い文書の列では答えない（負・非整数も同じ）", () => {
    const space = createColumnSpace(EXPANDED);

    expect(space.displayPosition(4, [])).toBeNull();
    expect(space.displayPosition(-1)).toBeNull();
    expect(space.displayPosition(1.5)).toBeNull();
    expect(space.displayPosition(Number.NaN, [])).toBeNull();
    // 列が 1 本も無い構成でも投げない。
    expect(createColumnSpace([]).displayPosition(0, [])).toBeNull();
  });
});

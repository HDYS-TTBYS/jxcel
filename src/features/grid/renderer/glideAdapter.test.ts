/**
 * 描画層の移植口の Glide 実装（`./glideAdapter`）の**配線**の検査（tasks.md 7.2）。
 *
 * # 何を固定し、何を固定しないか（**この線引きが本ファイルの要である**）
 *
 * 固定するのは**目に見えない配線**だけである。
 *
 * 1. **仕様 → Glide の props**（`RendererSpec` の列・行数・セルが Glide の props へどう写るか。
 *    セルの引きが位置を取り違えず、読み込み中と違反の印が落ちないこと）。
 * 2. **Glide の通知 → 移植口の callback**（選択・起動・列幅・列の移動・複製・貼り付けの 6 つが、
 *    移植口の callback として外へ出ること）。
 * 3. **呼び出し側の口**（`scrollTo` / `invalidate` が Glide の口へどう写るか）。
 *
 * **固定しないもの（本ファイルは証明しない）**:
 *   - 「10 万行を走査できる」「選択した範囲が視覚的に区別できる」「列幅と列の位置が操作できる」
 *     という**見え方の主張**。canvas に本当に描かれるかは本ファイルでは分からない
 *     （`environment: "node"` であり、canvas も DOM も無い）。**実物を起動して観測する**
 *     （`tech.md`「GUI・配布物・プラットフォーム差を含む主張は、実物を起動して観測した結果で
 *     裏付けること。単体テストは回帰の網であって受入の証明ではない」）。観測の実体は
 *     `src/features/smoke/portProbe*` と `scripts/check-port-interaction.sh` である。
 *   - Glide の React の部品（`GlideSurface`）そのもの。本ファイルが触るのは
 *     **DOM を持たない配線**（`createGlideWiring`）だけである — だから `jsdom` も `happy-dom` も
 *     要らない（足していない。canvas を模した DOM では見え方の主張を何も裏付けられない）。
 *
 * # 呼び出しの並びの比較（7.1 の申し送り）は `port.test.ts` が持つ
 *
 * 実物の配線を 7.1 の比較表へ足した 1 行は `port.test.ts` にある（本ファイルは 1 つ 1 つの
 * 写像を名指しで固定し、あちらは**決められた操作の並び**が実物でも変わらないことを固定する）。
 * 役割を分けてあるので、どちらが落ちたかで原因の当たりが付く。
 */
import {
  CompactSelection,
  GridCellKind,
  type DataEditorRef,
  type GridCell,
  type GridSelection,
} from "@glideapps/glide-data-grid";
import { describe, expect, it, vi } from "vitest";

import {
  GLIDE_ADAPTER_HEADER_HEIGHT,
  GLIDE_ADAPTER_ROW_HEIGHT,
  createGlideWiring,
  type GlideWiring,
} from "./glideAdapter";
import {
  DRIVE_COLUMNS,
  DRIVE_ROW_COUNT,
  arrayRowSource,
  createRendererSpec,
  lazyRowSource,
  type PortCall,
  type RecordedCall,
} from "./interactionDriver";
import type { CellPosition, CellRange, RendererSpec } from "./port";

/** 記録の控え。**移植口の callback が受け取った引数をそのまま**溜める。 */
function recorder(): {
  readonly calls: RecordedCall[];
  readonly record: (call: PortCall, args: readonly unknown[]) => void;
} {
  const calls: RecordedCall[] = [];
  return {
    calls,
    record: (call, args) => {
      calls.push({ call, args });
    },
  };
}

/** 記録された呼び出しのうち、名指ししたものだけを返す（並びの比較は `port.test.ts` が持つ）。 */
function callsNamed(calls: readonly RecordedCall[], call: PortCall): readonly RecordedCall[] {
  return calls.filter((entry) => entry.call === call);
}

/** 矩形の選択（Glide の形）。選択は**配線が所有する**ので、通知は Glide の形で注ぐ。 */
function rectangleSelection(range: CellRange): GridSelection {
  return {
    columns: CompactSelection.empty(),
    rows: CompactSelection.empty(),
    current: {
      cell: [range.start.column, range.start.row],
      range: {
        x: range.start.column,
        y: range.start.row,
        width: range.end.column - range.start.column + 1,
        height: range.end.row - range.start.row + 1,
      },
      rangeStack: [],
    },
  };
}

/** 列の全体の選択（見出しの操作。要件 2.3）。 */
function columnSelection(columns: readonly number[]): GridSelection {
  return {
    columns: CompactSelection.fromArray(columns),
    rows: CompactSelection.empty(),
  };
}

/** 行の全体の選択（行見出しの操作。要件 2.3）。 */
function rowSelection(rows: readonly number[]): GridSelection {
  return {
    columns: CompactSelection.empty(),
    rows: CompactSelection.fromArray(rows),
  };
}

/** 選択の解除（Glide は空の選択を渡してくる）。 */
const NO_SELECTION: GridSelection = {
  columns: CompactSelection.empty(),
  rows: CompactSelection.empty(),
};

/** 標本の仕様から組んだ配線と、移植口が受け取った引数の記録。 */
function wiringFor(source = arrayRowSource()): {
  readonly wiring: GlideWiring;
  readonly calls: RecordedCall[];
} {
  const log = recorder();
  const spec: RendererSpec = createRendererSpec({ source, record: log.record });
  return { wiring: createGlideWiring(spec), calls: log.calls };
}

describe("仕様 → Glide の props（描き手が引く形へ写す）", () => {
  it("列は見出しと幅のまま渡り、行数と行の高さ・見出しの高さが定まる", () => {
    const { wiring } = wiringFor();

    // **幅を落とさない**（要件 8.1: 列幅を変更できるとは、変更された幅で描けることである）。
    // `RenderColumn` は `title` と `width` だけを持つので、写しはそのままである。
    expect(wiring.props.columns).toEqual([
      { title: "名前", width: 160 },
      { title: "数量", width: 96 },
      { title: "提供元", width: 200 },
    ]);
    expect(wiring.props.rows).toBe(DRIVE_ROW_COUNT);

    // 高さは移植口に運ぶ欄が無い（`RendererSpec` は高さを持たない）ので、実装の既定である。
    // **その既定を名指しで固定する** — 画面（8.1）が高さを決められるようにするには
    // 移植口の面を広げる判断が要る、という申し送りでもある。
    expect(wiring.props.rowHeight).toBe(GLIDE_ADAPTER_ROW_HEIGHT);
    expect(wiring.props.headerHeight).toBe(GLIDE_ADAPTER_HEADER_HEIGHT);
  });

  it("セルの引きは位置を取り違えず、値の文字列をそのまま運ぶ", () => {
    const source = arrayRowSource();
    const { wiring } = wiringFor(source);

    // **Glide の `Item` は `[列, 行]` である**（移植口の `CellPosition` は `{row, column}`）。
    // 取り違えると別のセルを描くので、ここは名指しで固定する。
    const cell = wiring.props.getCellContent([1, 2]);

    expect(source.asked.at(-1)).toEqual({ row: 2, column: 1 });
    expect(cell.kind).toBe(GridCellKind.Text);
    if (cell.kind !== GridCellKind.Text) throw new Error("文字のセルでない");
    // 値そのものは**表示文字列のまま**である（数値へ解釈しない。design.md「境界に数値を出さない」）。
    expect(cell.displayData).toBe("2:1");
    expect(cell.data).toBe("2:1");
    // Glide 自身の編集器を開かせない（移植口は編集の意味論を知らない。入力手段は 7.4 の登録簿）。
    expect(cell.readonly).toBe(true);
    expect(cell.allowOverlay).toBe(false);
  });

  it("数値の札は右寄せにだけ使い、値は解釈しない", () => {
    const { wiring } = wiringFor();

    const text = wiring.props.getCellContent([0, 1]);
    const number = wiring.props.getCellContent([1, 1]);

    // 列 0 は `Text`、列 1 は `Int`（標本の札。`interactionDriver.ts` の `DRIVE_KINDS`）。
    expect(text.contentAlign).toBe("left");
    expect(number.contentAlign).toBe("right");
    // **右寄せは見た目だけである。**`Int` のセルも文字のまま運ばれる（`NumberCell` へ落とすと
    // `data` に JS の数値を要求され、64 ビット整数が壊れる。design.md「境界に数値を出さない」）。
    expect(number.kind).toBe(GridCellKind.Text);
  });

  it("未取得の行は読み込み中として描かれ、空白とは区別される", () => {
    // 取得済みを 2 行に限る。3 行目は未取得である（`lazyRowSource` の契約）。
    const { wiring } = wiringFor(lazyRowSource(2));

    const fetched = wiring.props.getCellContent([0, 2]);
    const loading = wiring.props.getCellContent([0, 3]);

    expect(fetched.kind).toBe(GridCellKind.Text);
    // **空白のセルにしない。**空白は「値なし」と区別がつかない（要件 1.4。移植口の不変条件）。
    expect(loading.kind).toBe(GridCellKind.Loading);
    if (loading.kind !== GridCellKind.Loading) throw new Error("読み込み中のセルでない");
    // 骨組みの棒の幅は列の幅から決める（0 だと Glide は何も塗らない — `loadingCellRenderer` は
    // `skeletonWidth` が未定義・0 のとき何も描かない）。**「読み込み中に見える」ことの条件**である。
    expect(loading.skeletonWidth).toBeGreaterThan(0);
    expect(loading.skeletonWidth).toBeLessThanOrEqual(DRIVE_COLUMNS[0]?.width ?? 0);
  });

  it("違反の印は地色の上書きとして現れる（落とさない）", () => {
    const { wiring } = wiringFor(lazyRowSource());

    // 標本の違反は 7 行ごとの 3 行目である（`interactionDriver.ts`）。
    const violated = wiring.props.getCellContent([0, 3]);
    const clean = wiring.props.getCellContent([0, 4]);

    expect(violated.themeOverride?.bgCell).toBeDefined();
    expect(clean.themeOverride?.bgCell).toBeUndefined();
  });

  it("範囲の外を引かれても例外を投げない", () => {
    const { wiring } = wiringFor(lazyRowSource(2));

    // 移植口の不変条件（`getCell` は例外を投げない）は仕様の側の契約である。写しはそれに乗る —
    // ここで例外が出れば、1 つの範囲外のセルが画面全体の描画を落とすことになる。
    for (const item of [
      [-1, -1],
      [DRIVE_COLUMNS.length, 0],
      [0, DRIVE_ROW_COUNT + 100],
      [1.5, 0],
    ] as const) {
      expect(wiring.props.getCellContent([item[0], item[1]]).kind).toBe(GridCellKind.Loading);
    }
  });

  it("複製・切り取り・貼り付けのキー割り当ては止めてある（クリップボードの経路を 1 本にする）", () => {
    const { wiring } = wiringFor();

    // Glide 自身の複製・貼り付けは**移植口を通らない**（Glide が表示文字列から勝手に表形式を
    // 組み立て、貼り付けでは中身を解釈する）。移植口は「呼び出し側が作った文字列をクリップボードへ
    // 渡すだけ」「貼り付けの文字列を解釈しない」と定めているので、Glide の側の経路は止め、
    // **この写しが DOM の `copy` / `paste` を自分で受ける**（下の `clipboard` の節）。
    expect(wiring.props.keybindings).toEqual({ copy: false, cut: false, paste: false });
  });
});

describe("Glide の通知 → 移植口の callback（6 つが外へ出る）", () => {
  it("列幅の変更は列の添字と変更後の幅を渡す", () => {
    const { wiring, calls } = wiringFor();

    // Glide の `onColumnResize` は `(column, newSize, colIndex, newSizeWithGrow)` である。
    // 移植口へは `(列, 幅)` だけを渡す（**位置は添字から取る** — 画面の表示順は添字が表す）。
    wiring.props.onColumnResize?.(
      { title: "数量", width: 120 },
      144,
      1,
      144,
    );

    expect(callsNamed(calls, "onColumnResize")).toEqual([
      { call: "onColumnResize", args: [1, 144] },
    ]);
  });

  it("列の移動は表示順の位置どうしで渡る", () => {
    const { wiring, calls } = wiringFor();

    wiring.props.onColumnMoved?.(2, 0);

    expect(callsNamed(calls, "onColumnMove")).toEqual([
      { call: "onColumnMove", args: [2, 0] },
    ]);
  });

  it("編集の起動は Glide の `Item`（列, 行）から位置へ写る", () => {
    const { wiring, calls } = wiringFor();

    wiring.props.onCellActivated?.([1, 2]);

    expect(callsNamed(calls, "onActivateEditor")).toEqual([
      { call: "onActivateEditor", args: [{ row: 2, column: 1 }] },
    ]);
  });

  it("複製は選択の範囲を移植口へ渡し、返った文字列をそのまま返す", async () => {
    const { wiring, calls } = wiringFor();
    const range: CellRange = { start: { row: 1, column: 0 }, end: { row: 2, column: 1 } };

    const text = await wiring.copyRange(range);

    expect(callsNamed(calls, "onCopy")).toEqual([{ call: "onCopy", args: [range] }]);
    // **移植口は文字列を作らない**（作るのは呼び出し側である）。ここは素通しである。
    expect(text).toBe("1:0\t1:1\n2:0\t2:1");
  });

  it("貼り付けは錨と文字列をそのまま渡す（中身を解釈しない）", async () => {
    const { wiring, calls } = wiringFor();
    const anchor: CellPosition = { row: 4, column: 0 };

    await wiring.pasteAt(anchor, "1:0\t1:1\n2:0\t2:1");

    // 文字列は 1 バイトも変えずに渡る（解釈は Rust 側の `PasteCodec` と適用層の仕事である）。
    expect(callsNamed(calls, "onPaste")).toEqual([
      { call: "onPaste", args: [anchor, "1:0\t1:1\n2:0\t2:1"] },
    ]);
  });
});

describe("選択（移植口に下ろす欄が無いので、実装が持ち、外へ報せる一方通行）", () => {
  it("矩形の選択は正規化された範囲として外へ出る", () => {
    const { wiring, calls } = wiringFor();
    const range: CellRange = { start: { row: 1, column: 0 }, end: { row: 3, column: 1 } };

    wiring.selectionChanged(rectangleSelection(range));

    expect(callsNamed(calls, "onSelectionChange")).toEqual([
      { call: "onSelectionChange", args: [range] },
    ]);
    // 配線が持つ唯一の写しも同じ範囲を指す（制御選択の props へそのまま渡る）。
    expect(wiring.currentRange()).toEqual(range);
  });

  it("列の全体と行の全体も同じ 1 つの矩形として外へ出る", () => {
    // 要件 2.3 の 3 つの選択（矩形・行の全体・列の全体）は、移植口では**同じ形**（矩形）で
    // 表される（`port.ts` の `CellRange` の docs）。数えるのは写す側である。
    const columns = wiringFor();
    columns.wiring.selectionChanged(columnSelection([1, 2]));

    expect(columns.wiring.currentRange()).toEqual({
      start: { row: 0, column: 1 },
      end: { row: DRIVE_ROW_COUNT - 1, column: 2 },
    });

    const rows = wiringFor();
    rows.wiring.selectionChanged(rowSelection([3, 4]));

    expect(rows.wiring.currentRange()).toEqual({
      start: { row: 3, column: 0 },
      end: { row: 4, column: DRIVE_COLUMNS.length - 1 },
    });
  });

  it("同じ選択の再通知は外へ出さない（知らせは変化のときだけである）", () => {
    const { wiring, calls } = wiringFor();
    const range: CellRange = { start: { row: 1, column: 0 }, end: { row: 3, column: 1 } };

    wiring.selectionChanged(rectangleSelection(range));
    wiring.selectionChanged(rectangleSelection(range));

    expect(callsNamed(calls, "onSelectionChange")).toHaveLength(1);
  });

  it("選択の解除は null として外へ出る", () => {
    const { wiring, calls } = wiringFor();
    const range: CellRange = { start: { row: 1, column: 0 }, end: { row: 1, column: 0 } };

    wiring.selectionChanged(rectangleSelection(range));
    wiring.selectionChanged(NO_SELECTION);

    expect(callsNamed(calls, "onSelectionChange")).toEqual([
      { call: "onSelectionChange", args: [range] },
      { call: "onSelectionChange", args: [null] },
    ]);
    expect(wiring.currentRange()).toBeNull();
  });

  it("選択の変化は購読者へ届く（React の面が描き直せる）", () => {
    const { wiring } = wiringFor();
    const listener = vi.fn();
    const unsubscribe = wiring.subscribeSelection(listener);
    const range: CellRange = { start: { row: 2, column: 1 }, end: { row: 2, column: 1 } };

    wiring.selectionChanged(rectangleSelection(range));
    expect(listener).toHaveBeenCalledTimes(1);

    unsubscribe();
    wiring.selectionChanged(NO_SELECTION);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe("貼り付けの錨（Glide の規則の写し）", () => {
  // Glide 自身の貼り付け（`onPasteInternal`）は「現在のセル → 列が 1 つだけならその列の先頭行 →
  // 行が 1 つだけならその行の先頭列 → それ以外は貼り付けない」という順で宛先を決める。
  // **同じ規則を写す** — さもないと、利用者が見ている宛先と移植口が受け取る錨が食い違う。
  it("現在のセルがあればその位置である", () => {
    const { wiring } = wiringFor();
    wiring.selectionChanged(
      rectangleSelection({ start: { row: 4, column: 0 }, end: { row: 6, column: 2 } }),
    );

    expect(wiring.currentAnchor()).toEqual({ row: 4, column: 0 });
  });

  it("列が 1 つだけの選択ではその列の先頭行である", () => {
    const { wiring } = wiringFor();
    wiring.selectionChanged(columnSelection([2]));

    expect(wiring.currentAnchor()).toEqual({ row: 0, column: 2 });
  });

  it("行が 1 つだけの選択ではその行の先頭列である", () => {
    const { wiring } = wiringFor();
    wiring.selectionChanged(rowSelection([5]));

    expect(wiring.currentAnchor()).toEqual({ row: 5, column: 0 });
  });

  it("選択が無ければ錨は無い（貼り付けない）", () => {
    const { wiring } = wiringFor();

    expect(wiring.currentAnchor()).toBeNull();
  });
});

describe("呼び出し側の口（Glide の口へ写す）", () => {
  /** Glide の部品の代役。**配線が DOM を知らない**ので、口の写しだけをここで固定できる。 */
  function stubSurface(): DataEditorRef {
    return { scrollTo: vi.fn(), updateCells: vi.fn() } as unknown as DataEditorRef;
  }

  it("`scrollTo` はその位置が見えるところまで動かす（両軸）", () => {
    const { wiring } = wiringFor();
    const surface = stubSurface();
    // `surfaceRef` は React の面が埋める口である（本物の `DataEditorRef`）。
    wiring.surfaceRef.current = surface;

    wiring.handle.scrollTo({ row: 40, column: 2 });

    // Glide の `scrollTo` は `(列, 行, 方向)` である。**列と行を取り違えない。**
    expect(surface.scrollTo).toHaveBeenCalledWith(2, 40, "both");
  });

  it("`invalidate` は区間の行を全列ぶん描き直させる", () => {
    const { wiring } = wiringFor();
    const surface = stubSurface();
    wiring.surfaceRef.current = surface;

    wiring.handle.invalidate({ start: 8, count: 4 });

    // Glide の `damage` は**セル単位**であり、区間を受けない。したがって区間の行 × 全列へ
    // 展開する（窓は数十行 × 数十列なので、行数に比例はしない。要件 11.4 と同じ規律）。
    const updates = vi.mocked(surface.updateCells).mock.calls[0]?.[0] as readonly {
      cell: readonly [number, number];
    }[];
    expect(updates).toHaveLength(4 * DRIVE_COLUMNS.length);
    expect(updates[0]).toEqual({ cell: [0, 8] });
    expect(updates.at(-1)).toEqual({ cell: [DRIVE_COLUMNS.length - 1, 11] });
  });
});

describe("破棄の後は移植口を使えない", () => {
  it("知らせの経路は投げる（画面が消えているのに呼び出し側を叩かない）", async () => {
    const { wiring, calls } = wiringFor();

    wiring.handle.destroy();

    // 7.1 の偽の実装と同じ規律である（黙って隠さない）。
    expect(() => {
      wiring.selectionChanged(NO_SELECTION);
    }).toThrow();
    await expect(wiring.copyRange({ start: { row: 0, column: 0 }, end: { row: 0, column: 0 } })).rejects.toThrow();
    await expect(wiring.pasteAt({ row: 0, column: 0 }, "x")).rejects.toThrow();
    expect(calls).toEqual([]);
  });

  it("呼び出し側の口は黙って何もしない（描くものが既に無い）", () => {
    const { wiring } = wiringFor();
    wiring.handle.destroy();

    expect(() => {
      wiring.handle.scrollTo({ row: 1, column: 1 });
      wiring.handle.invalidate({ start: 0, count: 1 });
    }).not.toThrow();
  });
});

describe("セルの取得の契約が写しでも保たれる", () => {
  it("同期であり、`Promise` を返さない", () => {
    const { wiring } = wiringFor(lazyRowSource(2));

    const cell: GridCell = wiring.props.getCellContent([0, 0]);
    expect(cell).not.toBeInstanceOf(Promise);
  });

  it("未取得の行の引きでも例外を投げない（描画の引きに例外を持ち込まない）", () => {
    const { wiring } = wiringFor(lazyRowSource(0));

    // 0 行目だけが取得済みである。未取得の行・範囲の外の行を引いても例外を投げず、
    // 読み込み中の札で答える（Glide は見えている範囲を引くので、未取得の行に必ず当たる）。
    expect(wiring.props.getCellContent([0, 0]).kind).toBe(GridCellKind.Text);
    for (const item of [
      [1, DRIVE_ROW_COUNT - 1],
      [2, 1],
      [0, DRIVE_ROW_COUNT + 5],
    ] as const) {
      expect(wiring.props.getCellContent([item[0], item[1]]).kind).toBe(GridCellKind.Loading);
    }
  });
});

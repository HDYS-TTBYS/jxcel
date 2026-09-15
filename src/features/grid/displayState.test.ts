/**
 * 表示状態の契約（tasks.md 7.5。data-grid 要件 8.1、8.2。割り方の根拠は要件 8.5）。
 *
 * # 何を固定するか
 *
 * 1. **列ごとの幅**（要件 8.1）と、**描画のときにだけ効く列の並び順**（要件 8.2）
 * 2. **2 つの欄が別の添字の空間に住むこと** — `columnOrder` は「表示位置 → 文書の列」、
 *    `columnWidths` は「文書の列 → 幅」である。design.md の interface は両方を `number` で
 *    書いており、型からは向きが読み取れない（`port.ts` が `RowOrdinal` / `ColumnIndex` を
 *    別名に留めたのと同じ事情）。**取り違えが最も起きやすい所**なので、巡回する並び（`[2,0,1]`）
 *    で固定する — 巡回では「表示位置 → 文書の列」とその逆写像が別の並びになる
 * 3. **窓の中身を変えないこと**（tasks.md 7.5 の受け入れ）。7.3 の記録の道具と同じ形の偽の
 *    サーバを使い、**表示の列順を変えても窓の要求の生バイトが 1 バイトも変わらない**ことを
 *    示す。並び順が要求へ漏れれば、並べ替えが取り直しを起こせば、あるいは引く列の翻訳が
 *    狂えば落ちる
 * 4. **境界にもドキュメントにも届かないこと**（要件 8.5。design.md「表示状態」）。源と
 *    取り込みの閉包を走査して固定する。**走査そのものの生存**も見本で確かめる
 *
 * 環境は `node` である（`vitest.config.ts`）。本 module は DOM を触らず、この file も
 * node の API（`node:fs`）を使わない — 源は `import.meta.glob` の `?raw` で取り込む
 * （`windowCache.test.ts` と `editorRegistry.test.ts` の方針に合わせる）。
 */
import { describe, expect, it } from "vitest";

import * as displayStateModule from "./displayState";
import { DEFAULT_COLUMN_WIDTH, createDisplayState } from "./displayState";
// 契約は名前のある型で受ける（`ReturnType<typeof …>` で実装へ結び付けない）。
import type { DisplayStateStore } from "./displayState";
// 7.3 の窓の記憶（**本 file は読むだけである**）。tasks.md 7.5 の受け入れは「窓の要求の内容が
// 変わらないこと」であり、その記録の道具（偽のサーバと要求の読み手）をここでも使う。
import {
  ROW_KEY_LEN,
  WINDOW_FORMAT_VERSION,
  WINDOW_HEADER_LEN,
  WINDOW_REQUEST_HEADER_LEN,
  createWindowCache,
  type WindowCache,
  type WindowTransport,
} from "./windowCache";

/** 4 列ぶんの見出し（**文書の列の添字で引く**。画面が持つ `ColumnDescriptor.name` の代役）。 */
const TITLES: readonly string[] = ["名前", "数量", "単価", "備考"];

/** 描く並びの見出し（`renderColumns` の順を読むための写し）。 */
const titlesOf = (display: DisplayStateStore): readonly string[] =>
  display.renderColumns(TITLES).map((column) => column.title);

// ===========================================================================
// 1. 列ごとの幅（要件 8.1）
// ===========================================================================

describe("列ごとの幅（要件 8.1）", () => {
  it("設定した幅は、その列の幅として返る", () => {
    const display = createDisplayState({ columnCount: 4 });

    display.setColumnWidth(2, 240);

    expect(display.columnWidths.get(2)).toBe(240);
    expect(display.renderColumns(TITLES)).toEqual([
      { title: "名前", width: DEFAULT_COLUMN_WIDTH },
      { title: "数量", width: DEFAULT_COLUMN_WIDTH },
      { title: "単価", width: 240 },
      { title: "備考", width: DEFAULT_COLUMN_WIDTH },
    ]);
  });

  it("幅を設定していない列は既定の幅で描かれ、columnWidths には現れない", () => {
    const display = createDisplayState({ columnCount: 3, defaultWidth: 96 });

    display.setColumnWidth(1, 200);

    // 設定の有無そのものが読める（設定の無い列の幅を 96 として焼き付けない）。
    expect([...display.columnWidths.entries()]).toEqual([[1, 200]]);
    expect(display.renderColumns(["A", "B", "C"]).map((column) => column.width)).toEqual([96, 200, 96]);
  });

  it("幅の値は判定せず、そのまま覚えて返す", () => {
    const display = createDisplayState({ columnCount: 2 });

    // 画素は描画層の単位である。丸めも下限の切り上げもしない（値の正しさを決める根拠が
    // 本 module に無い）。
    display.setColumnWidth(1, 144.5);

    expect(display.renderColumns(["A", "B"]).map((column) => column.width)).toEqual([
      DEFAULT_COLUMN_WIDTH,
      144.5,
    ]);
  });

  it("存在しない列への設定は無視され、状態を変えない", () => {
    const display = createDisplayState({ columnCount: 2 });

    // 描画層の知らせは命令ではない。宣言の外の位置（配線の誤り、列数が変わった後の古い知らせ）
    // は状態へ入れない。**投げない** — 知らせは描画の途中に届く。
    for (const position of [2, -1, 1.5, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(() => display.setColumnWidth(position, 300)).not.toThrow();
    }

    expect([...display.columnWidths.entries()]).toEqual([]);
    expect(display.columnOrder).toEqual([0, 1]);
  });

  it("列数の指定は 0 以上の整数へ均す（列が 0 本のシートも正当である）", () => {
    expect(createDisplayState({ columnCount: 2.7 }).columnCount).toBe(2);
    expect(createDisplayState({ columnCount: -3 }).columnCount).toBe(0);
    expect(createDisplayState({ columnCount: Number.NaN }).columnCount).toBe(0);

    const empty = createDisplayState({ columnCount: 0 });
    empty.setColumnWidth(0, 240);
    empty.moveColumn(0, 1);

    expect(empty.columnOrder).toEqual([]);
    expect(empty.renderColumns(TITLES)).toEqual([]);
    expect([...empty.columnWidths.entries()]).toEqual([]);
  });
});

// ===========================================================================
// 2. 表示の列順（要件 8.2）
// ===========================================================================

describe("表示の列順（要件 8.2）", () => {
  it("初期の並びは文書の並びそのものである", () => {
    const display = createDisplayState({ columnCount: 4 });

    expect(display.columnOrder).toEqual([0, 1, 2, 3]);
    expect(titlesOf(display)).toEqual(TITLES);
  });

  it("運んだ列が行き先に来て、間の列は詰める", () => {
    const display = createDisplayState({ columnCount: 4 });

    // 描画層の知らせと同じ意味である（Glide の `onColumnMoved(startIndex, endIndex)`。
    // `research.md` の観測: 列 2 を位置 0 へ運ぶと描かれる見出しは `列2,列0,列1,列3` になる）。
    display.moveColumn(2, 0);
    expect(display.columnOrder).toEqual([2, 0, 1, 3]);
    expect(titlesOf(display)).toEqual(["単価", "名前", "数量", "備考"]);

    // 逆向きの運搬は元へ戻す（置換の群の中の操作であること）。
    display.moveColumn(0, 2);
    expect(display.columnOrder).toEqual([0, 1, 2, 3]);
  });

  it("columnOrder は「表示位置 → 文書の列」である（逆写像ではない）", () => {
    const display = createDisplayState({ columnCount: 3 });

    // 巡回（3 周期）を使う。**巡回では 2 つの向きが別の並びになる** — ここが `[2, 0, 1]` なら、
    // 実装は「文書の列 → 表示位置」を返している（逆写像）。
    display.moveColumn(0, 2);

    expect(display.columnOrder).toEqual([1, 2, 0]);
    expect(display.renderColumns(["A", "B", "C"])).toEqual([
      { title: "B", width: DEFAULT_COLUMN_WIDTH },
      { title: "C", width: DEFAULT_COLUMN_WIDTH },
      { title: "A", width: DEFAULT_COLUMN_WIDTH },
    ]);
  });

  it("恒等の運搬は並びを変えない", () => {
    const display = createDisplayState({ columnCount: 3 });
    display.moveColumn(1, 0);

    display.moveColumn(2, 2);

    expect(display.columnOrder).toEqual([1, 0, 2]);
  });

  it("範囲の外の運搬は無視され、並びは置換のままである", () => {
    const display = createDisplayState({ columnCount: 4 });

    for (const [from, to] of [
      [4, 0],
      [0, 4],
      [-1, 0],
      [0, -1],
      [1.5, 0],
      [Number.NaN, 0],
    ] as const) {
      expect(() => display.moveColumn(from, to)).not.toThrow();
    }

    expect(display.columnOrder).toEqual([0, 1, 2, 3]);
  });

  it("どの操作の後でも、並びは 0..列数-1 の置換であり、長さが変わらない", () => {
    const display = createDisplayState({ columnCount: 5 });

    // 範囲の外を混ぜた乱暴な並び（**長さが足りない並びは作れない**ことの確かめ）。
    for (const [from, to] of [
      [4, 0],
      [0, 4],
      [2, 2],
      [1, 3],
      [5, 1],
      [3, 0],
      [0, 2],
    ] as const) {
      display.moveColumn(from, to);
    }

    const order = display.columnOrder;
    expect(order).toHaveLength(5);
    expect([...order].sort((left, right) => left - right)).toEqual([0, 1, 2, 3, 4]);
  });
});

// ===========================================================================
// 3. 幅と並びは別の添字の空間に住む（取り違えの固定）
// ===========================================================================

describe("幅と並びは別の添字の空間に住む", () => {
  it("並びを変えても、幅は列に付いたまま運ばれる", () => {
    const display = createDisplayState({ columnCount: 3 });
    display.setColumnWidth(2, 200);

    display.moveColumn(2, 0);

    // 幅の鍵は文書の列の添字である（表示位置ではない）。列 2 は左へ運ばれても幅 200 のまま描かれる。
    expect(display.columnWidths.get(2)).toBe(200);
    expect(display.renderColumns(["A", "B", "C"])).toEqual([
      { title: "C", width: 200 },
      { title: "A", width: DEFAULT_COLUMN_WIDTH },
      { title: "B", width: DEFAULT_COLUMN_WIDTH },
    ]);
  });

  it("幅の設定は「そのとき位置にいる列」を指す", () => {
    const display = createDisplayState({ columnCount: 3 });
    display.moveColumn(2, 0); // 並びは [2, 0, 1]

    display.setColumnWidth(0, 90);

    // 位置 0 にいるのは列 2 である（列 0 ではない）。**同じ位置への設定が、並びを変える前と
    // 後では別の列を指す。**
    expect([...display.columnWidths.entries()]).toEqual([[2, 90]]);
    expect(display.renderColumns(["A", "B", "C"]).map((column) => column.width)).toEqual([90, 120, 120]);
  });
});

// ===========================================================================
// 4. 窓の中身を変えない（tasks.md 7.5 の受け入れ。要件 8.2）
//
// 偽のサーバは 7.3 の検査と同じ形である（要求の頭を検査の側で独立に読み、生バイトも控える）。
// 窓のバイト列の組み立ては `windowCache.test.ts` の写しであり、**形式の真ではない**
// （形式の真は Rust 側の符号化器と固定ファイルである）。
// ===========================================================================

/** 偽のサーバが返す行（**文書の列の並び**のセルを持つ）。 */
interface FakeRow {
  readonly key: Uint8Array;
  readonly cells: readonly { readonly tag: number; readonly text: string }[];
}

/** 偽のサーバが返す窓。 */
interface FakeWindow {
  readonly generation: number;
  readonly start: number;
  readonly rows: readonly FakeRow[];
  readonly columns: number;
}

/** 窓のバイト列を組み立てる（u64 はすべてリトルエンディアン）。 */
function windowBytes(window: FakeWindow): ArrayBuffer {
  const encoder = new TextEncoder();
  const bodies: Uint8Array[] = [];
  let length = WINDOW_HEADER_LEN;
  for (const row of window.rows) {
    bodies.push(row.key);
    length += row.key.byteLength;
    for (const cell of row.cells) {
      const text = encoder.encode(cell.text);
      const head = new Uint8Array(2 + 8);
      head[0] = cell.tag;
      head[1] = 0; // 違反なし（本検査の主題ではない）。
      new DataView(head.buffer).setBigUint64(2, BigInt(text.byteLength), true);
      bodies.push(head, text);
      length += head.byteLength + text.byteLength;
    }
  }

  const bytes = new Uint8Array(length);
  const view = new DataView(bytes.buffer);
  bytes[0] = WINDOW_FORMAT_VERSION;
  view.setBigUint64(1, BigInt(window.generation), true);
  view.setBigUint64(9, BigInt(window.start), true);
  view.setBigUint64(17, BigInt(window.rows.length), true);
  view.setBigUint64(25, BigInt(window.columns), true);
  let at = WINDOW_HEADER_LEN;
  for (const body of bodies) {
    bytes.set(body, at);
    at += body.byteLength;
  }
  return bytes.buffer;
}

/** 行の識別子の生バイト（7.3 の検査と同じ鍵の並びである）。 */
function keyFor(row: number): Uint8Array {
  const key = new Uint8Array(ROW_KEY_LEN);
  key.set([
    0x01, 0x56, 0x3e, 0x3a, 0xb5, 0xd3, 0xd6, 0x76, 0x4c, 0x61, 0xef, 0xb9, 0x93, 0x02, 0xbd, 0x5c,
  ]);
  key[ROW_KEY_LEN - 1] = (key[ROW_KEY_LEN - 1] ?? 0) + row;
  return key;
}

/** 1 行（`columns` 本のセル。文字は `行:列` であり、**どの列を読んだかが文字に出る**）。 */
function rowFor(row: number, columns: number): FakeRow {
  const cells = Array.from({ length: columns }, (_unused, column) => ({
    tag: 5,
    text: `${row}:${column}`,
  }));
  return { key: keyFor(row), cells };
}

/** 偽のサーバの窓（開始序数から `count` 行）。 */
function windowFor(start: number, count: number, generation: number, columns: number): FakeWindow {
  const rows: FakeRow[] = [];
  for (let row = start; row < start + count; row += 1) {
    rows.push(rowFor(row, columns));
  }
  return { generation, start, rows, columns };
}

/** 要求の頭から読んだ欄（**検査の側の独立な読み手**。位置とエンディアンを別に書く）。 */
interface RecordedRequest {
  readonly start: number;
  readonly count: number;
  readonly sheet: string;
}

/** 移送の記録と偽のサーバ。 */
interface RequestHarness {
  /** 到着順の要求（欄として読んだもの）。 */
  readonly calls: RecordedRequest[];
  /** 到着順の要求の**生バイト**（1 バイトも変わらないことを見るため）。 */
  readonly arguments: Uint8Array[];
  readonly transport: WindowTransport;
}

/** 要求の頭を独立に読む。 */
function readRequest(argument: Uint8Array): RecordedRequest {
  const view = new DataView(argument.buffer, argument.byteOffset, argument.byteLength);
  return {
    start: Number(view.getBigUint64(9, true)),
    count: Number(view.getBigUint64(17, true)),
    sheet: new TextDecoder().decode(argument.subarray(WINDOW_REQUEST_HEADER_LEN)),
  };
}

/** その場で解決する移送を組み立てる（既定は「要求どおりの行を要求の世代で返す」）。 */
function harnessOf(columns: number): RequestHarness {
  const calls: RecordedRequest[] = [];
  const argumentLog: Uint8Array[] = [];
  const transport: WindowTransport = (argument) => {
    calls.push(readRequest(argument));
    argumentLog.push(argument);
    return Promise.resolve(
      windowBytes(windowFor(calls[calls.length - 1]?.start ?? 0, calls[calls.length - 1]?.count ?? 0, 0, columns)),
    );
  };
  return { calls, arguments: argumentLog, transport };
}

/** 移送の約束を記憶へ反映させる（**実時間ではなく微小タスクを進める**）。 */
async function settle(): Promise<void> {
  for (let tick = 0; tick < 4; tick += 1) {
    await Promise.resolve();
  }
}

/** 記憶を組み立てる（4 列・4 行 1 窓・12 行）。 */
function cacheWith(harness: RequestHarness): WindowCache {
  return createWindowCache({
    sheet: "発注明細",
    variants: ["Text", "Int", "Text", "Int"],
    rowCount: 12,
    windowRows: 4,
    transport: harness.transport,
  });
}

/**
 * 生バイトの 16 進（**欄ではなくバイト列で比べる**。欄に現れない差も捕まえるためである）。
 * 綴りが非自明なので名前を与えてある。
 */
function hexOf(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

describe("窓の中身を変えない（tasks.md 7.5 の受け入れ）", () => {
  it("表示の列順を変えても、窓の要求の内容は変わらない", async () => {
    const harness = harnessOf(4);
    const cache = cacheWith(harness);
    const display = createDisplayState({ columnCount: 4 });

    // 画面（8.8）が行うことの最小の写し: 表示順に並んだ列を描き、見えている行の各**表示位置**の
    // セルを引く。**引く列は文書の列の添字である**（`columnOrder[表示位置]`）。
    const paint = (): readonly string[] => {
      const texts: string[] = [];
      for (let row = 0; row < 6; row += 1) {
        // **表示順の左から順に**引く。各表示位置が指す文書の列は `columnOrder` が決める。
        for (const column of display.columnOrder) {
          texts.push(cache.getCell({ row, column }).text);
        }
      }
      return texts;
    };

    paint();
    await settle();
    const before = paint();
    await settle();
    const requestsBefore = harness.arguments.map(hexOf);

    expect(before.slice(0, 4)).toEqual(["0:0", "0:1", "0:2", "0:3"]);

    // 列 2 を左端へ運ぶ。描かれる並びは実際に変わる。
    display.moveColumn(2, 0);
    expect(titlesOf(display)).toEqual(["単価", "名前", "数量", "備考"]);

    const after = paint();
    await settle();

    // (1) 要求は増えない（並べ替えは取り直しを起こさない）。**生バイトも 1 バイトも変わらない。**
    expect(harness.arguments.map(hexOf)).toEqual(requestsBefore);
    expect(harness.calls).toHaveLength(requestsBefore.length);

    // (2) 窓が運ぶ中身は同じである — 変わるのは、どの表示位置がどの列を指すかだけである。
    //     **期待は状態から導かず、ここに硬く書く**（状態が壊れていても緑になる書き方をしない）。
    expect(display.columnOrder).toEqual([2, 0, 1, 3]);
    expect(after.slice(0, 4)).toEqual(["0:2", "0:0", "0:1", "0:3"]);
    expect(after.slice(4, 8)).toEqual(["1:2", "1:0", "1:1", "1:3"]);
    expect([...after].sort()).toEqual([...before].sort());

    // (3) 窓そのものも同じである（文書の列で引けば、並べ替えの前後で同じ文字が返る）。
    expect(cache.getCell({ row: 1, column: 2 }).text).toBe("1:2");
    expect(cache.getCell({ row: 5, column: 1 }).text).toBe("5:1");
  });
});

// ===========================================================================
// 5. 境界にもドキュメントにも届かない（要件 8.5 の構造）
// ===========================================================================

describe("境界にもドキュメントにも届かない", () => {
  it("実行時の輸出に、境界を呼ぶ名前が 1 つも無い", () => {
    // 型（`DisplayState` など）は実行時には現れない。値として出るのは器と既定の幅だけである。
    expect(Object.keys(displayStateModule).sort()).toEqual(["DEFAULT_COLUMN_WIDTH", "createDisplayState"]);
  });

  it("値として取り込む module が 1 つも無い（型だけである）", () => {
    expect(valueImportsOf(SOURCES[DISPLAY_STATE_SOURCE] ?? "")).toEqual([]);
  });

  it("取り込みの閉包は境界へ届かない", () => {
    expect(reachableModules(DISPLAY_STATE_SOURCE).filter((key) => BOUNDARY_KEY.test(key))).toEqual([]);
  });

  it("走査そのものが生きている（窓の記憶からは境界へ届く）", () => {
    // 同じ走査を 7.3 の窓の記憶に当てると、境界（`../../ipc/client`）へ届く。走査が黙って
    // 何も見なくなったら、ここが空になって落ちる。
    expect(reachableModules("/src/features/grid/windowCache.ts").filter((key) => BOUNDARY_KEY.test(key))).toEqual([
      "/src/features/grid/windowCache.ts",
      "/src/ipc/client.ts",
    ]);

    // 綴りの読み分けそのものの見本（`import type` は取り込みではない）。
    expect(valueImportsOf('import { createWindowCache } from "./windowCache";')).toEqual(["./windowCache"]);
    expect(valueImportsOf('import type { RenderColumn } from "./renderer/port";')).toEqual([]);
    expect(valueImportsOf('import "副作用";')).toEqual(["副作用"]);
    expect(valueImportsOf('// import { invokeRaw } from "../../ipc/client";').length).toBe(0);

    // **空白の無い綴りと動的な綴りも取り込みとして読む。** 7.5 のレビューが実測した穴であり、
    // これらを落とすと「綴りを 1 つ足せば落ちる」という約束が偽になる。
    expect(valueImportsOf('import*as ns from "../../ipc/client";')).toEqual(["../../ipc/client"]);
    expect(valueImportsOf('export*from "../../ipc/client";')).toEqual(["../../ipc/client"]);
    expect(valueImportsOf('void import("../../ipc/client")')).toEqual(["../../ipc/client"]);
    expect(valueImportsOf('const m = require("../../ipc/client");')).toEqual(["../../ipc/client"]);
  });
});

// ===========================================================================
// 源の走査（本 file だけが使う道具）
//
// `import.meta.glob` は Vite が変換時に解決するので、検査の環境を node の API（`node:fs`）へ
// 結び付けない（`windowCache.test.ts` / `editorRegistry.test.ts` と同じ方針）。綴りは
// リポジトリの根からの道である（`../../ipc/client` のような格子の外への取り込みも辿るため、
// `./**` ではなく `src` の全体を取る）。
// ===========================================================================

/** 源の生のテキスト（鍵は `/src/…` の形）。 */
const SOURCES = import.meta.glob("/src/**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** 表示状態の源（本 file の主役）。 */
const DISPLAY_STATE_SOURCE = "/src/features/grid/displayState.ts";

/**
 * 境界の鍵: IPC の呼び口と、**境界を持つ module**（窓の記憶・シェルの呼び出し口・画面）。
 * 綴りは源の鍵（`/src/…`）に当てる。
 */
const BOUNDARY_KEY = /\/(?:ipc|windowCache|gridClient|GridScreen)[./]/;

/** `import … from "…"` / `export … from "…"` / `import "…"` の綴りを引く（素朴な走査である）。 */
const FROM_CLAUSE = /(?:^|[\n;])\s*(import|export)\s*([^;]*?)\s*from\s*["']([^"']+)["']/g;
const BARE_IMPORT = /(?:^|[\n;])\s*import\s*["']([^"']+)["']/g;
/**
 * 動的な取り込み（`import("…")`）と CommonJS の `require("…")`。
 *
 * **この 2 つを落とすと、走査は「綴りを 1 つ足せば落ちる」という約束を果たさない。**
 * 7.5 のレビューが実測した: `import * as ns from` / `export * from` / `import(…)` は当時の
 * 走査を素通りし、**18 件すべて緑のまま実際の module の辺を作った**（束ねた結果に境界の名前が
 * 現れ、見本の副作用も走った）。`\s+` は空白を要求するので空白なしの綴りも落としていた。
 */
const DYNAMIC_IMPORT = /(?:^|[^\w$.])import\s*\(\s*["']([^"']+)["']\s*\)/g;
const REQUIRE_CALL = /(?:^|[^\w$.])require\s*\(\s*["']([^"']+)["']\s*\)/g;

/**
 * 注釈を落とす（`/* … *\/` と `// …`）。**素朴な走査であることを認める** — 落としすぎれば
 * 見落とす側に倒れる（誤って緑にはならない）。行の注釈は `://` を壊さないよう、直前が `:` で
 * ない `/` の対だけを落とす（`editorRegistry.test.ts` と同じ綴りである）。
 *
 * **値として**取り込む綴りだけを返す（`import type` は含めない。型は実行時に消える）。
 * 綴りの種類は 3 つである: `import type …`（型だけ）・`import …`（値）・`import "…"`
 * （副作用）。`export … from "…"` も値の依存である（再輸出は読み込みを起こす）。
 */
function valueImportsOf(source: string): readonly string[] {
  const code = source.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/(^|[^:])\/\/[^\n]*/g, "$1");
  const specifiers: string[] = [];
  for (const match of code.matchAll(FROM_CLAUSE)) {
    const clause = match[2] ?? "";
    const specifier = match[3];
    if (specifier !== undefined && !clause.trimStart().startsWith("type")) {
      specifiers.push(specifier);
    }
  }
  for (const match of code.matchAll(BARE_IMPORT)) {
    const specifier = match[1];
    if (specifier !== undefined) {
      specifiers.push(specifier);
    }
  }
  // 動的な取り込みと `require`。**綴りを 1 つ足せば落ちる**という約束を果たすためである。
  for (const pattern of [DYNAMIC_IMPORT, REQUIRE_CALL]) {
    for (const match of code.matchAll(pattern)) {
      const specifier = match[1];
      if (specifier !== undefined) {
        specifiers.push(specifier);
      }
    }
  }
  return specifiers;
}

/** 取り込みの綴りを源の鍵（`/src/…`）へ直す（格子の外・外部のパッケージは `null`）。 */
function resolvedKeyOf(from: string, specifier: string): string | null {
  if (!specifier.startsWith(".")) {
    return null;
  }
  const parts = from.split("/");
  parts.pop();
  for (const segment of specifier.split("/")) {
    if (segment === "" || segment === ".") {
      continue;
    }
    if (segment === "..") {
      parts.pop();
      continue;
    }
    parts.push(segment);
  }
  const joined = parts.join("/");
  for (const candidate of [joined, `${joined}.ts`, `${joined}.tsx`, `${joined}/index.ts`, `${joined}/index.tsx`]) {
    if (SOURCES[candidate] !== undefined) {
      return candidate;
    }
  }
  return null;
}

/**
 * `entry` から**値として**取り込む module を辿った閉包（`entry` を含む。順序は鍵の昇順）。
 *
 * 型だけの取り込みは辿らない — 実行時の経路を作らないためである。**辿れない綴り（外部の
 * パッケージ）は閉包に入らない**ので、境界の判定は「境界の module が閉包に現れるか」で行う。
 */
function reachableModules(entry: string): readonly string[] {
  const seen = new Set<string>([entry]);
  const queue: string[] = [entry];
  while (queue.length > 0) {
    const key = queue.pop();
    if (key === undefined) {
      continue;
    }
    for (const specifier of valueImportsOf(SOURCES[key] ?? "")) {
      const resolved = resolvedKeyOf(key, specifier);
      if (resolved === null || seen.has(resolved)) {
        continue;
      }
      seen.add(resolved);
      queue.push(resolved);
    }
  }
  return [...seen].sort();
}

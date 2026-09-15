/**
 * 窓の記憶と先読み、および生バイト経路の符号化と復号（tasks.md 7.3。data-grid 要件 1.4, 1.7,
 * 11.1, 11.2, 11.6。design.md「WindowCache」）。
 *
 * 固定するのは次のものである。
 *
 * 1. **要求の頭のバイト配置**（`src-tauri/src/commands/grid.rs` の「生バイト経路」節が唯一の源）。
 *    版 1 バイト ＋ 世代 / 開始序数 / 行数 / シート長の u64 リトルエンディアン ＋ シートの
 *    UTF-8 で、全体の長さは `33 + シートの UTF-8 のバイト数` である。**位置ごとに表明する** —
 *    往復（符号化 → 復号）だけでは、両方が同じ誤り（たとえばビッグエンディアン）を共有して
 *    いても緑になる。
 * 2. **窓の復号は本物の Rust の符号化器が出したバイト列を読む**（言語をまたぐ固定。
 *    `crates/data-grid/tests/fixtures/window_protocol.txt` と、そのファイルを表明する
 *    `crates/data-grid/tests/window_protocol_fixture.rs`）。TS 側だけで往復させると、
 *    **両方が同じ誤りを共有していれば緑になる**ため、真のバイト列を読むことが要る。
 * 3. **`getCell` の同期契約**（`./renderer/port.ts`）: 記憶にあれば直ちに返り、無ければ
 *    `loading: true` を返して取得を始める。**決して投げない**（範囲の外も含む）。
 * 4. **同じ区間への要求は 1 本にまとめる**（呼び出し回数で示す）。
 * 5. **世代が変わった応答は捨てる**（記憶に入れない）。
 * 6. **走査の向きを見て前後の窓を先読みする**（どちらを先に要求するかで示す）。
 * 7. **`EditOutcome.affected` の行を含む窓だけを捨てる**（他の窓は残ること）。
 * 8. **空の窓は記憶に入れず、再試行できる**（空白として描かない）。
 * 9. **記憶の大きさは行数に比例しない**（要件 11.6）。
 * 10. **未取得のセルは移植口を通って読み込み中として描かれる**（`glideAdapter` の配線を通し、
 *    `GridCellKind.Loading` になることを見る）。
 * 11. **行数が変わる編集のあとは、画面が新しい行数を `clear` へ渡す**（渡さなければ、元の
 *    行数より先は**永久に読み込み中**のままである。減った先は範囲の外として扱い、要求も
 *    配りもしない）。
 *
 * # 検査の側の偽のサーバについて
 *
 * 窓を組み立てる `windowBytes` は**検査の側の偽のサーバ**であり、形式の真ではない（形式の真は
 * 1・2 の固定である）。記憶の検査はこの偽のサーバを相手にするので、こちらが誤っていても
 * **記憶の検査は緑になりうる** — 形式そのものは 1・2 が受け持つ、という分担である。
 *
 * # 待ち方を時間にしない
 *
 * 移送の偽物は**その場で解決する約束**を返し、検査は微小タスクを進めて応答の反映を待つ
 * （`settle`）。実時間の待ちを入れないので、走らせる環境の速さに結果が依らない。応答が
 * **あとから**届く場面（世代違い）は、移送の答えを要求の時点で確定させることで作る —
 * `getCell` の同期の本体が移送を呼び、そのあと検査が世代を進め、そのあとに微小タスクが
 * 進む、という順序がそのまま「古い応答が後から届く」になる。
 */
import { GridCellKind } from "@glideapps/glide-data-grid";
import { describe, expect, it, vi } from "vitest";

// 固定ファイルは**生のテキストとして**取り込む（Vite の `?raw`。型は `vite/client` が与える）。
// ファイルを実行時に開く口（`node:fs`）を使わないのは、検査の環境を node の API へ結び付けない
// ためである（`vitest.config.ts` のヘッダ「環境が node であること」）。
import fixtureText from "../../../crates/data-grid/tests/fixtures/window_protocol.txt?raw";
import type { TypeKindTag } from "../../ipc/bindings";
import { createGlideWiring } from "./renderer/glideAdapter";
import type { CellPosition, RendererSpec, RowSpan } from "./renderer/port";
import {
  MAX_WINDOWS,
  ROW_KEY_LEN,
  WINDOW_FORMAT_VERSION,
  WINDOW_HEADER_LEN,
  WINDOW_REQUEST_HEADER_LEN,
  WINDOW_REQUEST_VERSION,
  WINDOW_ROWS,
  createWindowCache,
  decodeWindow,
  describeWindowDecodeFailure,
  encodeWindowRequest,
  isViolated,
  rowKeyText,
  type WindowCache,
  type WindowTransport,
} from "./windowCache";

// ---------------------------------------------------------------------------
// 言語をまたぐ固定（本物の Rust の符号化器が出したバイト列）
// ---------------------------------------------------------------------------

/** 固定ファイル（`key = value` の行のみ。`#` は注釈）。**値は TS 側で組み立てない。** */
const FIXTURE: Readonly<Record<string, string>> = readFixture();

function readFixture(): Readonly<Record<string, string>> {
  const fields: Record<string, string> = {};
  for (const line of fixtureText.split("\n")) {
    const trimmed = line.trim();
    if (trimmed === "" || trimmed.startsWith("#")) {
      continue;
    }
    const separator = trimmed.indexOf("=");
    if (separator <= 0) {
      throw new Error(`固定ファイルの行が key = value ではない: ${trimmed}`);
    }
    fields[trimmed.slice(0, separator).trim()] = trimmed.slice(separator + 1).trim();
  }
  return fields;
}

/** 固定ファイルの 16 進をバイト列へ戻す。 */
function fixtureBytes(key: string): Uint8Array {
  const hex = FIXTURE[key];
  if (hex === undefined) {
    throw new Error(`固定ファイルに ${key} が無い`);
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let at = 0; at < bytes.length; at += 1) {
    bytes[at] = Number.parseInt(hex.slice(at * 2, at * 2 + 2), 16);
  }
  return bytes;
}

/** 固定ファイルの窓が運ぶ行の識別子（正準の 26 文字。固定ファイルのヘッダが名指す）。 */
const FIXTURE_ROW_IDS = [
  "01ARZ3NDEKTSV4RRFFQ69G5FAW",
  "01ARZ3NDEKTSV4RRFFQ69G5FAX",
  "01ARZ3NDEKTSV4RRFFQ69G5FAY",
] as const;

/** 固定ファイルの窓の内容（表示文字列・変種の札・違反の有無。固定ファイルのヘッダと同じ表）。 */
const FIXTURE_CELLS: readonly (readonly (readonly [string, number, boolean])[])[] = [
  [
    ["日本語", 5, false],
    ["3", 2, true],
    ["true", 1, false],
    ["", 0, false],
  ],
  [
    ["x", 5, false],
    ["12", 2, false],
    ["false", 1, false],
    ["備考2", 5, false],
  ],
  [
    ["", 5, false],
    ["5", 2, false],
    ["true", 1, false],
    ["", 0, false],
  ],
];

// ---------------------------------------------------------------------------
// 検査の側の偽のサーバ（形式の真ではない。ヘッダの注釈を参照）
// ---------------------------------------------------------------------------

/** 偽のサーバが返す行。 */
interface FakeRow {
  readonly key: Uint8Array;
  /** セル（列順）。`tag` は wire の変種の札（0 = Null / 1 = Bool / 2 = Int / 5 = Text）。 */
  readonly cells: readonly {
    readonly tag: number;
    readonly text: string;
    readonly violated?: boolean;
  }[];
}

/** 偽のサーバが返す窓の指定。 */
interface FakeWindow {
  readonly generation: number;
  readonly start: number;
  readonly rows: readonly FakeRow[];
  readonly columns: number;
}

/** 窓のバイト列を組み立てる（**偽のサーバ**。u64 はすべてリトルエンディアン）。 */
function windowBytes(window: FakeWindow): ArrayBuffer {
  const encoder = new TextEncoder();
  const bodies: Uint8Array[] = [];
  let length = WINDOW_HEADER_LEN;
  for (const row of window.rows) {
    bodies.push(row.key);
    length += row.key.byteLength;
    for (const cell of row.cells) {
      const text = encoder.encode(cell.text);
      const violated = cell.violated === true;
      const head = new Uint8Array(violated ? 2 + 8 + 8 + 8 : 2 + 8);
      head[0] = cell.tag;
      head[1] = violated ? 1 : 0;
      const view = new DataView(head.buffer);
      if (violated) {
        // 札 1 つ・段 0（セル直下の違反）。
        view.setBigUint64(2, 1n, true);
        view.setBigUint64(10, 0n, true);
        view.setBigUint64(18, BigInt(text.byteLength), true);
      } else {
        view.setBigUint64(2, BigInt(text.byteLength), true);
      }
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

/**
 * 行の識別子の生バイト（固定ファイルの行 0..2 と同じ鍵の並びを続ける）。
 *
 * 固定ファイルの 3 件（`…FAW` / `…FAX` / `…FAY`）は**本物の Rust の符号化器が出した鍵**で
 * あり、その 26 文字は固定ファイルのヘッダが名指す。4 件目以降は同じ鍵の最終バイトを進めた
 * ものである。
 */
function keyFor(row: number): Uint8Array {
  const key = new Uint8Array(ROW_KEY_LEN);
  key.set([
    0x01, 0x56, 0x3e, 0x3a, 0xb5, 0xd3, 0xd6, 0x76, 0x4c, 0x61, 0xef, 0xb9, 0x93, 0x02, 0xbd, 0x5c,
  ]);
  key[ROW_KEY_LEN - 1] = (key[ROW_KEY_LEN - 1] ?? 0) + row;
  return key;
}

/** 1 列 1 セルの行（記憶の検査ではセルの中身そのものが主題ではない）。 */
function rowFor(row: number): FakeRow {
  return { key: keyFor(row), cells: [{ tag: 5, text: `${row}:0` }] };
}

/** 偽のサーバの窓（開始序数から `count` 行）。 */
function windowFor(start: number, count: number, generation: number): FakeWindow {
  const rows: FakeRow[] = [];
  for (let row = start; row < start + count; row += 1) {
    rows.push(rowFor(row));
  }
  return { generation, start, rows, columns: 1 };
}

/** 要求の頭から読んだ欄（**検査の側の独立な読み手**。位置とエンディアンを別に書く）。 */
interface RecordedRequest {
  readonly version: number;
  readonly generation: number;
  readonly start: number;
  readonly count: number;
  readonly sheet: string;
}

/** 移送の記録と偽のサーバ。 */
interface WindowHarness {
  /** 到着順の要求（**復号は検査の側で独立に書く**）。 */
  readonly calls: RecordedRequest[];
  readonly transport: WindowTransport;
}

/** 要求の頭を独立に読む。 */
function readRequest(argument: Uint8Array): RecordedRequest {
  const view = new DataView(argument.buffer, argument.byteOffset, argument.byteLength);
  return {
    version: argument[0] ?? -1,
    generation: Number(view.getBigUint64(1, true)),
    start: Number(view.getBigUint64(9, true)),
    count: Number(view.getBigUint64(17, true)),
    sheet: new TextDecoder().decode(argument.subarray(WINDOW_REQUEST_HEADER_LEN)),
  };
}

/**
 * その場で解決する移送を組み立てる。
 *
 * `answer` は要求を読んで返す窓を決める（`null` は**空の窓** ＝ 失敗と世代違いの表現）。
 * 既定は「要求どおりの行を要求の世代で返す」。答えは**要求の時点で確定する**ので、
 * 検査が同期のうちに世代を進めれば「古い応答が後から届く」場面になる。
 */
function harnessOf(
  answer: (request: RecordedRequest) => ArrayBuffer | null = (request) =>
    windowBytes(windowFor(request.start, request.count, request.generation)),
): WindowHarness {
  const calls: RecordedRequest[] = [];
  const transport: WindowTransport = (argument) => {
    const request = readRequest(argument);
    calls.push(request);
    const bytes = answer(request);
    return Promise.resolve(bytes ?? new ArrayBuffer(0));
  };
  return { calls, transport };
}

/** 移送の約束を記憶へ反映させる（**実時間ではなく微小タスクを進める**）。 */
async function settle(): Promise<void> {
  for (let tick = 0; tick < 4; tick += 1) {
    await Promise.resolve();
  }
}

/** 記憶を組み立てる（検査ごとの既定: 4 行 1 窓・12 行・列 1 本）。 */
function cacheWith(options: {
  readonly transport: WindowTransport;
  readonly rowCount?: number;
  readonly windowRows?: number;
  readonly maxWindows?: number;
  readonly variants?: readonly TypeKindTag[];
  readonly sheet?: string;
  readonly generation?: number;
  readonly onArrival?: (span: RowSpan) => void;
}): WindowCache {
  return createWindowCache({
    sheet: options.sheet ?? "発注明細",
    variants: options.variants ?? ["Text"],
    rowCount: options.rowCount ?? 12,
    windowRows: options.windowRows ?? 4,
    transport: options.transport,
    ...(options.maxWindows === undefined ? {} : { maxWindows: options.maxWindows }),
    ...(options.generation === undefined ? {} : { generation: options.generation }),
    ...(options.onArrival === undefined ? {} : { onArrival: options.onArrival }),
  });
}

// ---------------------------------------------------------------------------
// 1. 要求の符号化（位置とエンディアンを位置ごとに表明する）
// ---------------------------------------------------------------------------

describe("要求の頭", () => {
  it("版は 0 バイト目にあり、数の欄は位置とリトルエンディアンで読める", () => {
    const bytes = encodeWindowRequest({
      generation: 0x0102030405060708n,
      start: 0x1112131415161718n,
      count: 0x2122232425262728n,
      sheet: "s",
    });

    expect(WINDOW_REQUEST_HEADER_LEN).toBe(33);
    expect(bytes[0]).toBe(WINDOW_REQUEST_VERSION);
    expect(bytes[0]).toBe(1);
    // リトルエンディアンであること（ビッグエンディアンならこの並びは逆になる）。
    expect(Array.from(bytes.slice(1, 9))).toEqual([8, 7, 6, 5, 4, 3, 2, 1]);
    expect(Array.from(bytes.slice(9, 17))).toEqual([0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11]);
    expect(Array.from(bytes.slice(17, 25))).toEqual([0x28, 0x27, 0x26, 0x25, 0x24, 0x23, 0x22, 0x21]);
    // シート長は「s」の 1 バイト。
    expect(Array.from(bytes.slice(25, 33))).toEqual([1, 0, 0, 0, 0, 0, 0, 0]);
    expect(bytes[33]).toBe("s".charCodeAt(0));
  });

  it("全体の長さは 33 + シートの UTF-8 のバイト数であり、シートは末尾までを占める", () => {
    const sheet = "発注明細";
    const bytes = encodeWindowRequest({ generation: 1n, start: 2n, count: 3n, sheet });
    const encoded = new TextEncoder().encode(sheet);

    // **文字数ではない**（`sheet.length` を使うと 4 バイトずれる）。
    expect(encoded.byteLength).toBe(12);
    expect(sheet.length).toBe(4);
    expect(bytes.byteLength).toBe(WINDOW_REQUEST_HEADER_LEN + encoded.byteLength);
    expect(Array.from(bytes.slice(25, 33))).toEqual([12, 0, 0, 0, 0, 0, 0, 0]);
    expect(Array.from(bytes.slice(WINDOW_REQUEST_HEADER_LEN))).toEqual(Array.from(encoded));
  });

  it("既定の移送は invokeRaw へ**バッファそのもの**を引数全体として渡す", async () => {
    // **入れ子にしない**（`{ argument: buffer }` にすると Tauri が数値の配列へ変換して JSON と
    // して送り、受け手は生バイトとして読めない。`bulk` のモジュール doc「経路の性質」）。
    const invocations: { command: string; args: unknown }[] = [];
    vi.stubGlobal("window", {
      __TAURI_INTERNALS__: {
        invoke: (command: string, args: unknown) => {
          invocations.push({ command, args });
          return Promise.resolve(
            windowBytes({ generation: 0, start: 0, rows: [rowFor(0)], columns: 1 }),
          );
        },
      },
    });
    try {
      const cache = createWindowCache({
        sheet: "発注明細",
        variants: ["Text"],
        rowCount: 4,
        windowRows: 4,
      });
      expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
      await settle();

      expect(invocations).toHaveLength(1);
      expect(invocations[0]?.command).toBe("grid_rows_window");
      const argument = invocations[0]?.args;
      expect(argument, "引数が入れ子になっている").toBeInstanceOf(Uint8Array);
      if (!(argument instanceof Uint8Array)) {
        throw new Error("引数が生バイトではない");
      }
      expect(readRequest(argument)).toEqual({
        version: 1,
        generation: 0,
        start: 0,
        count: 4,
        sheet: "発注明細",
      });
      // 窓が届いたので、次の引きは記憶から答える。
      expect(cache.getCell({ row: 0, column: 0 })).toMatchObject({ text: "0:0", loading: false });
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

// ---------------------------------------------------------------------------
// 2. 窓の復号（本物の Rust の符号化器が出したバイト列）
// ---------------------------------------------------------------------------

describe("窓の復号", () => {
  it("固定ファイルの窓を、表のとおりの欄として読む", () => {
    const result = decodeWindow(fixtureBytes("window"));
    expect(result.ok).toBe(true);
    if (!result.ok) {
      throw new Error(describeWindowDecodeFailure(result.failure));
    }
    const window = result.window;
    expect(window.version).toBe(WINDOW_FORMAT_VERSION);
    expect(window.version).toBe(1);
    expect(window.generation).toBe(7);
    expect(window.start).toBe(0);
    expect(window.rowCount).toBe(3);
    expect(window.columnCount).toBe(4);
    expect(window.rows).toHaveLength(3);

    window.rows.forEach((row, index) => {
      // 行の識別子は生の 16 バイト（不透明な鍵）であり、26 文字の正準形へ写せる。
      expect(row.key).toHaveLength(ROW_KEY_LEN);
      expect(rowKeyText(row.key)).toBe(FIXTURE_ROW_IDS[index]);
      expect(row.cells).toHaveLength(4);
      row.cells.forEach((cell, column) => {
        const expected = FIXTURE_CELLS[index]?.[column];
        expect(expected, "固定ファイルの内容の表が足りない").toBeDefined();
        expect(cell.text).toBe(expected?.[0]);
        expect(cell.variant).toBe(expected?.[1]);
        expect(isViolated(cell)).toBe(expected?.[2]);
      });
    });

    // 行 0 の Int の違反は**段 0 の札 1 つ**である（違反なしと区別できる形）。
    expect(window.rows[0]?.cells[1]?.marks).toEqual([[]]);
    // 違反でないセルには札が無い。
    expect(window.rows[1]?.cells[1]?.marks).toEqual([]);
  });

  it("行 0 の窓（頭だけの 33 バイト）は空の窓ではなく、行 0 件の窓として読める", () => {
    const bytes = fixtureBytes("window_rows_0");
    expect(bytes.byteLength).toBe(WINDOW_HEADER_LEN);
    const result = decodeWindow(bytes);
    expect(result.ok).toBe(true);
    if (!result.ok) {
      throw new Error(describeWindowDecodeFailure(result.failure));
    }
    expect(result.window.start).toBe(3);
    expect(result.window.rowCount).toBe(0);
    expect(result.window.rows).toEqual([]);
  });

  it("空の窓（長さ 0）は失敗として読める（行 0 の窓とは別の理由である）", () => {
    const result = decodeWindow(new Uint8Array(0));
    expect(result.ok).toBe(false);
    if (result.ok) {
      throw new Error("空の窓が復号できてしまった");
    }
    expect(result.failure.kind).toBe("empty");
  });

  it("壊れた窓は投げずに失敗として読める（知らない版・切り詰め・余分なバイト）", () => {
    const window = fixtureBytes("window");
    const unknownVersion = Uint8Array.from(window);
    unknownVersion[0] = WINDOW_FORMAT_VERSION + 1;
    expect(decodeWindow(unknownVersion)).toMatchObject({
      ok: false,
      failure: { kind: "unknownVersion" },
    });

    // 完全な窓の**すべての真の接頭辞**が拒まれる（Rust 側の検査と同じ規律）。
    for (let length = 1; length < window.byteLength; length += 1) {
      expect(decodeWindow(window.subarray(0, length)).ok, `接頭辞 ${length} バイトが復号できた`).toBe(
        false,
      );
    }

    const trailing = new Uint8Array(window.byteLength + 1);
    trailing.set(window);
    expect(decodeWindow(trailing)).toMatchObject({ ok: false, failure: { kind: "trailingBytes" } });

    const unknownTag = Uint8Array.from(window);
    unknownTag[WINDOW_HEADER_LEN + ROW_KEY_LEN] = 9;
    expect(decodeWindow(unknownTag)).toMatchObject({ ok: false, failure: { kind: "unknownTag" } });
  });

  it("行の識別子の 26 文字は、本物の Rust の符号化器が出した鍵と一致する", () => {
    // 固定ファイルの 3 件の鍵は本物の `Ulid::to_bytes` の出力である。26 文字への写しが
    // 食い違えば、`EditOutcome.affected` との突き合わせ（窓の破棄）が静かに外れる。
    const first = fixtureBytes("window").subarray(WINDOW_HEADER_LEN, WINDOW_HEADER_LEN + ROW_KEY_LEN);
    expect(rowKeyText(first)).toBe(FIXTURE_ROW_IDS[0]);
    expect(rowKeyText(keyFor(1))).toBe(FIXTURE_ROW_IDS[1]);
    expect(rowKeyText(keyFor(2))).toBe(FIXTURE_ROW_IDS[2]);
  });
});

// ---------------------------------------------------------------------------
// 3. 同期のセル取得（移植口の契約）
// ---------------------------------------------------------------------------

describe("同期のセル取得", () => {
  it("記憶にあるセルは直ちに返り、無いセルは読み込み中として返って取得が始まる", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport });

    const before = cache.getCell({ row: 1, column: 0 });
    expect(before.loading).toBe(true);
    expect(before.text).toBe("");
    expect(before.violated).toBe(false);
    expect(cache.pendingCount).toBe(1);
    expect(harness.calls.map((call) => call.start)).toEqual([0]);

    await settle();

    const after = cache.getCell({ row: 1, column: 0 });
    expect(after).toMatchObject({ text: "1:0", loading: false, violated: false, variant: "Text" });
    expect(cache.pendingCount).toBe(0);
    // 記憶から答えたので移送は増えない。
    expect(harness.calls).toHaveLength(1);
  });

  it("範囲の外・整数でない位置でも決して投げず、移送もしない", () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport });
    const positions: CellPosition[] = [
      { row: -1, column: 0 },
      { row: 12, column: 0 },
      { row: 1.5, column: 0 },
      { row: Number.NaN, column: 0 },
      { row: 0, column: 9 },
      { row: 0, column: -1 },
    ];
    for (const position of positions) {
      const cell = cache.getCell(position);
      expect(cell.loading).toBe(true);
      expect(cell.text).toBe("");
      // 列が範囲の外であるときも何か 1 つの札を返す（移植口の契約）。
      expect(typeof cell.variant).toBe("string");
    }
    // 範囲の外は取得しない（存在しない序数を要求しない）。
    expect(harness.calls).toEqual([]);
    expect(cache.windowCount).toBe(0);
  });

  it("窓が運ばない列は、行が取得済みでも空の値として返る", async () => {
    // 宣言が 2 列であるのに窓が 1 列しか運ばない場合（表示の指定と文書が食い違っている）。
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, variants: ["Text", "Text"] });
    cache.getCell({ row: 0, column: 0 });
    await settle();

    // 列 1 は窓（1 列）に無い。行は取得済みなので読み込み中ではない（読み込み中のまま
    // 止まるより、値なしとして描くほうが表示の指定に忠実である）。
    expect(cache.getCell({ row: 0, column: 1 })).toMatchObject({ text: "", loading: false });
    expect(harness.calls).toHaveLength(1);
  });

  it("移送の失敗（拒否）でも投げず、未取得のまま残って再試行できる", async () => {
    const calls: RecordedRequest[] = [];
    const transport: WindowTransport = (argument) => {
      calls.push(readRequest(argument));
      return Promise.reject(new Error("IPC が不達"));
    };
    const cache = cacheWith({ transport });

    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
    await settle();

    expect(cache.windowCount).toBe(0);
    expect(cache.pendingCount).toBe(0);
    // 再試行できる（同じ区間をもう一度要求する）。
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
    expect(calls.map((call) => call.start)).toEqual([0, 0]);
  });
});

// ---------------------------------------------------------------------------
// 4. 同じ区間への要求は 1 本にまとめる
// ---------------------------------------------------------------------------

describe("同じ区間への要求", () => {
  it("同じ窓を覆う複数のセルの引きは 1 回の移送にまとまる", async () => {
    // 行数が窓 1 つぶんしかないので、先読みの窓が混ざらない（まとめだけを見る）。
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 4 });

    // 同じ窓（0..4）を覆う 4 つのセルを引く。**移送は 1 回だけである。**
    for (const row of [0, 1, 2, 3]) {
      expect(cache.getCell({ row, column: 0 }).loading).toBe(true);
    }
    expect(harness.calls).toHaveLength(1);
    expect(cache.pendingCount).toBe(1);

    // 可視範囲の通知も同じ区間を要求するが、進行中の要求があるので増えない。
    cache.setVisibleSpan({ start: 1, count: 2 });
    expect(harness.calls).toHaveLength(1);

    await settle();

    expect(cache.windowCount).toBe(1);
    // 到着後の引きは記憶から答える（移送は増えない）。
    expect(cache.getCell({ row: 3, column: 0 }).loading).toBe(false);
    expect(harness.calls).toHaveLength(1);
  });

  it("区間が違えば別の要求になる（まとめが過剰でないこと）", () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport });

    cache.getCell({ row: 0, column: 0 });
    cache.getCell({ row: 4, column: 0 });
    expect(harness.calls.map((call) => call.start)).toEqual([0, 4]);
    expect(cache.pendingCount).toBe(2);
  });
});

// ---------------------------------------------------------------------------
// 5. 世代が変わった応答は捨てる
// ---------------------------------------------------------------------------

describe("世代", () => {
  it("世代が変わったあとに届いた応答は記憶に入れない", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, generation: 3 });

    // 移送は要求の時点で答えを確定する（世代 3 の窓）。
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
    expect(harness.calls[0]?.generation).toBe(3);

    // 表示が変わった（編集の適用など）ので、応答が届く**前に**世代を進める。
    cache.setGeneration(4);
    await settle();

    expect(cache.generation).toBe(4);
    expect(cache.windowCount).toBe(0);
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
    // 新しい世代で要求し直す。
    expect(harness.calls.map((call) => ({ generation: call.generation, start: call.start }))).toEqual([
      { generation: 3, start: 0 },
      { generation: 4, start: 0 },
    ]);
  });

  it("世代を進めても、影響を受けていない窓の記憶は残る", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, generation: 1 });
    cache.getCell({ row: 0, column: 0 });
    await settle();

    cache.setGeneration(2);
    // 窓の内容は変わっていない（破棄の通知は届いていない）ので、記憶はそのまま使える。
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(false);
    expect(harness.calls).toHaveLength(1);
  });

  it("窓が名乗る世代がいまの世代と違えば捨てる（応答そのものの札を見る）", async () => {
    const harness = harnessOf((request) => windowBytes(windowFor(request.start, request.count, 4)));
    const cache = cacheWith({ transport: harness.transport, generation: 5 });
    cache.getCell({ row: 0, column: 0 });
    await settle();

    expect(cache.windowCount).toBe(0);
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// 6. 走査の向きを見た先読み
// ---------------------------------------------------------------------------

describe("先読み", () => {
  it("下向きの走査では、可視の次に**向きの先**（下）の窓を要求する", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 24 });

    cache.setVisibleSpan({ start: 12, count: 2 });
    // 可視（12..16）→ 向きの先（16..20）→ 反対側（8..12）の順である。
    expect(harness.calls.map((call) => call.start)).toEqual([12, 16, 8]);
    await settle();
    expect(cache.pendingCount).toBe(0);
  });

  it("上へ戻る走査では、可視の窓も先読みも**向きの先**（上）から要求する", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 24 });

    // 下向き（最初の向き）: 可視（16..18）→ 向きの先（20..24）→ 反対側（12..16）の順である。
    cache.setVisibleSpan({ start: 16, count: 2 });
    expect(harness.calls.map((call) => call.start)).toEqual([16, 20, 12]);
    await settle();

    // 上へ戻る（6 < 16）。可視の 3 行は **2 つの窓（4..8 と 8..12）に掛かり、どちらも記憶に
    // 無い** — 可視の要求の並び**そのもの**が向きを映す（向きを見ない実装は 8 を先に要求する。
    // 可視の窓が 1 つだけ、あるいは片方が記憶にある並びでは、向きを見ない実装でも同じ並びに
    // なるため、この検査は向きを固定しない）。続く先読みも**向きの先（上 0..4）が先**である。
    cache.setVisibleSpan({ start: 6, count: 3 });
    expect(harness.calls.map((call) => call.start)).toEqual([16, 20, 12, 4, 8, 0]);
    expect(cache.pendingCount).toBe(3);
  });

  it("先読みの**並び**も向きを映す（反対側の先読み先が記憶に無い状態で逆走する）", async () => {
    // 上の検査は「可視の窓の並び」で向きを映すが、**先読みの並び**は映さない — 逆走した時点で
    // 反対側の先読み先（12..16）が既に記憶にあると、そこへの要求が出ないためである
    // （7.3 のレビューが実測: 先読みの並びだけを常に下向きへ固定する変異が 37/37 緑のまま通った）。
    // ここでは**両側の先読み先を未取得**にしてから逆走させ、先読みの並びそのものを固定する。
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 24 });

    // 下向きに 20..22 を見る。可視（20..24）→ 反対側（16..20）。下側の先読み先（24..28）は
    // 行数 24 の外なので要求されない。
    cache.setVisibleSpan({ start: 20, count: 2 });
    expect(harness.calls.map((call) => call.start)).toEqual([20, 16]);
    await settle();

    // 上へ戻る（6 < 20）。可視の 3 行は 2 つの窓（4..8 / 8..12）に掛かり、どちらも記憶に無い。
    // 続く先読みも**向きの先（上 0..4）が先**、反対側（12..16）が後である — ここで 12 が先に
    // 出たら、先読みの並びが向きを見ていない。
    cache.setVisibleSpan({ start: 6, count: 3 });
    expect(harness.calls.map((call) => call.start)).toEqual([20, 16, 4, 8, 0, 12]);
    // 可視の 2 窓 + 両側の先読み 2 窓 = 4 本が未着である。
    expect(cache.pendingCount).toBe(4);
  });

  it("記憶にある窓は要求しない（先読みが重ならない）", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 24 });

    cache.setVisibleSpan({ start: 8, count: 2 });
    await settle();
    const before = harness.calls.length;

    // 同じ可視範囲をもう一度通知しても、可視も先読みも記憶にある。
    cache.setVisibleSpan({ start: 8, count: 2 });
    expect(harness.calls).toHaveLength(before);
  });

  it("可視範囲が窓の境界を越えると、可視の窓をすべて要求する", () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 24 });

    // 可視 3 行が 2 つの窓（4..8 と 8..12）に掛かる。向きは最初は下とみなすので、
    // **向きの先（下）の窓から**要求する。
    cache.setVisibleSpan({ start: 6, count: 3 });
    expect(harness.calls.slice(0, 2).map((call) => call.start)).toEqual([8, 4]);
    // 続けて先読み（下 12..16、上 0..4）が並ぶ。
    expect(harness.calls.map((call) => call.start)).toEqual([8, 4, 12, 0]);
  });

  it("可視の終端が行数を越えても、行数の外を要求しない", () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 10 });

    cache.setVisibleSpan({ start: 8, count: 5 });
    // 可視の窓は 8..10 の 1 つだけであり、切り詰められて 2 行になる。
    expect(harness.calls[0]).toMatchObject({ start: 8, count: 2 });
    // 先読みも 10 行を越えない（12..16 の窓は要求しない）。
    for (const call of harness.calls) {
      expect(call.start + call.count, "行数の外を要求した").toBeLessThanOrEqual(10);
    }
    expect(harness.calls.map((call) => call.start)).toEqual([8, 4]);
  });
});

// ---------------------------------------------------------------------------
// 7. 影響を受けた行の窓を捨てる
// ---------------------------------------------------------------------------

describe("影響を受けた行の窓の破棄", () => {
  it("その行を含む窓だけを捨て、他の窓は残る", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 12 });

    // 2 つの窓（0..4 と 8..12）を取得する。
    cache.getCell({ row: 0, column: 0 });
    cache.getCell({ row: 8, column: 0 });
    await settle();
    expect(cache.windowCount).toBe(2);
    expect(cache.getCell({ row: 8, column: 0 }).text).toBe("8:0");

    // 行 0（固定ファイルの鍵の 1 つ目）が影響を受けた。
    cache.invalidate([FIXTURE_ROW_IDS[0]]);

    expect(cache.windowCount).toBe(1);
    // 影響を受けた行は未取得に戻り、再要求される。
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
    expect(harness.calls.map((call) => call.start)).toEqual([0, 8, 0]);
    // 影響を受けていない窓のセルは**そのまま読める**（移送も増えない）。
    expect(cache.getCell({ row: 8, column: 0 })).toMatchObject({ text: "8:0", loading: false });
    expect(harness.calls).toHaveLength(3);
  });

  it("可視の窓が落ちたら、その場で取得し直す（次の走査を待たない）", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 12 });
    cache.setVisibleSpan({ start: 0, count: 2 });
    await settle();
    expect(cache.windowCount).toBeGreaterThan(0);
    const before = harness.calls.length;

    cache.invalidate([FIXTURE_ROW_IDS[0]]);
    expect(harness.calls.length).toBe(before + 1);
    expect(harness.calls.at(-1)?.start).toBe(0);
  });

  it("知らない行の通知では何も捨てない", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 12 });
    cache.getCell({ row: 0, column: 0 });
    await settle();

    cache.invalidate([]);
    cache.invalidate(["01ARZ3NDEKTSV4RRFFQ69G5FB0"]);
    expect(cache.windowCount).toBe(1);
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(false);
  });

  it("大文字小文字の違う通知でも同じ行として捨てる", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 12 });
    cache.getCell({ row: 0, column: 0 });
    await settle();

    cache.invalidate([FIXTURE_ROW_IDS[0].toLowerCase()]);
    expect(cache.windowCount).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// 8. 空の窓（失敗と世代違いの表現）
// ---------------------------------------------------------------------------

describe("空の窓", () => {
  it("記憶に入れず、空白として描かず、再試行できる", async () => {
    const harness = harnessOf(() => null);
    const cache = cacheWith({ transport: harness.transport });

    const cell = cache.getCell({ row: 0, column: 0 });
    // **空白で代用しない**（値なしと区別がつかない）。読み込み中のままである。
    expect(cell).toMatchObject({ text: "", loading: true });
    await settle();

    expect(cache.windowCount).toBe(0);
    expect(cache.pendingCount).toBe(0);
    // 再試行できる（同じ区間をもう一度要求する）。
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
    expect(harness.calls.map((call) => call.start)).toEqual([0, 0]);
  });

  it("壊れた窓（復号できないバイト列）も記憶に入れない", async () => {
    const harness = harnessOf(() => {
      const broken = new Uint8Array(8);
      broken[0] = WINDOW_FORMAT_VERSION;
      return broken.buffer;
    });
    const cache = cacheWith({ transport: harness.transport });

    cache.getCell({ row: 0, column: 0 });
    await settle();

    expect(cache.windowCount).toBe(0);
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
  });

  it("窓が要求より短ければ、可視行の末尾に達したことを覚えて再要求しない", async () => {
    // 可視行が 6 行しかないシート（画面が持つ `rowCount` が古い場合）。
    const harness = harnessOf((request) =>
      windowBytes(windowFor(request.start, Math.max(0, Math.min(request.count, 6 - request.start)), 0)),
    );
    const cache = cacheWith({ transport: harness.transport, rowCount: 12 });

    cache.getCell({ row: 0, column: 0 });
    await settle();
    expect(cache.windowCount).toBe(1);

    // 端を越える序数は範囲の外として扱い、**再要求しない**（要求の嵐を作らない）。
    expect(cache.getCell({ row: 8, column: 0 }).loading).toBe(true);
    expect(cache.getCell({ row: 8, column: 0 }).loading).toBe(true);
    expect(harness.calls.map((call) => call.start)).toEqual([0, 8]);
  });
});

// ---------------------------------------------------------------------------
// 8.5 記憶そのものを捨てる（序数と行の対応が変わったとき）
// ---------------------------------------------------------------------------

describe("記憶の破棄", () => {
  it("clear は記憶と進行中の応答を捨て、取り直す", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 12 });
    cache.getCell({ row: 0, column: 0 });
    cache.getCell({ row: 8, column: 0 });
    await settle();
    expect(cache.windowCount).toBe(2);

    // 進行中の要求を作ってから捨てる（並べ替え・絞り込みで対応が変わった場面）。
    expect(cache.getCell({ row: 4, column: 0 }).loading).toBe(true);
    expect(cache.pendingCount).toBe(1);
    cache.clear();
    expect(cache.windowCount).toBe(0);
    expect(cache.pendingCount).toBe(0);

    // **進行中だった応答は入らない**（古い対応の窓を記憶へ戻さない）。
    await settle();
    expect(cache.windowCount).toBe(0);
    // 取り直せる。
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
    expect(harness.calls.map((call) => call.start)).toEqual([0, 8, 4, 0]);
  });

  it("行数が増えた編集のあと、増えた行を要求して読める（画面は新しい行数を渡す）", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 8 });

    cache.getCell({ row: 0, column: 0 });
    await settle();
    // **元の行数より先は範囲の外である**（取得もしない）。
    expect(cache.getCell({ row: 12, column: 0 }).loading).toBe(true);
    const beforeGrowth = harness.calls.length;

    // 貼り付けの補充で行が増えた（`GridEditResponse` の `row_count` が 8 → 20 になった）。
    cache.clear(20);

    // **増えた行は要求され、届いたあとは読める。** 行数を持ち越すと（`clear` が引数を
    // 無視すると）ここは永久に読み込み中のままである — 要求が 1 本も出ない。
    expect(cache.getCell({ row: 12, column: 0 }).loading).toBe(true);
    expect(
      harness.calls.slice(beforeGrowth).map((call) => ({ start: call.start, count: call.count })),
    ).toEqual([{ start: 12, count: 4 }]);
    await settle();
    expect(cache.getCell({ row: 12, column: 0 })).toMatchObject({ text: "12:0", loading: false });
  });

  it("行数が減った編集のあと、減った先は読まず要求もしない（古い窓を配らない）", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 12 });
    cache.getCell({ row: 8, column: 0 });
    await settle();
    expect(cache.getCell({ row: 8, column: 0 })).toMatchObject({ text: "8:0", loading: false });
    const beforeShrink = harness.calls.length;

    // 行の削除で 12 → 6 行になった。
    cache.clear(6);

    // **捨てた窓の内容を配らない**: 範囲の外として読み込み中を返し、要求もしない
    // （窓の側の終端は下がる一方であり、上がるのは `clear` の引数だけである）。
    expect(cache.getCell({ row: 8, column: 0 }).loading).toBe(true);
    expect(harness.calls).toHaveLength(beforeShrink);
    // 残った行は読み直せる（窓は新しい行数へ切り詰められる）。
    expect(cache.getCell({ row: 4, column: 0 }).loading).toBe(true);
    expect(
      harness.calls.map((call) => ({ start: call.start, count: call.count })),
    ).toEqual([
      { start: 8, count: 4 },
      { start: 4, count: 2 },
    ]);
    await settle();
    expect(cache.getCell({ row: 4, column: 0 })).toMatchObject({ text: "4:0", loading: false });
  });

  it("行数を渡さなければ、開いたときの行数へ戻る（引数の無い呼び出しは従来どおり）", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 8 });
    cache.clear(20);
    expect(cache.getCell({ row: 12, column: 0 }).loading).toBe(true);
    expect(harness.calls.map((call) => call.start)).toEqual([12]);

    // 引数を渡さない `clear` は**開いたときの行数**（8）へ戻す。
    cache.clear();
    expect(cache.getCell({ row: 12, column: 0 }).loading).toBe(true);
    expect(harness.calls.map((call) => call.start)).toEqual([12]);
    expect(cache.getCell({ row: 4, column: 0 }).loading).toBe(true);
    expect(harness.calls.map((call) => call.start)).toEqual([12, 4]);
  });

  it("dispose のあとは取得も記憶もしない（画面の片付け）", async () => {
    const harness = harnessOf();
    const cache = cacheWith({ transport: harness.transport, rowCount: 12 });
    cache.getCell({ row: 0, column: 0 });
    await settle();
    expect(cache.windowCount).toBe(1);

    cache.dispose();
    expect(cache.windowCount).toBe(0);
    expect(cache.getCell({ row: 0, column: 0 }).loading).toBe(true);
    // 片付けたあとは移送しない。
    expect(harness.calls).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// 9. 記憶の大きさ（要件 11.6）
// ---------------------------------------------------------------------------

describe("記憶の大きさ", () => {
  it("行数が 100 倍でも窓の数は増えない（上限に収まる）", async () => {
    const counts: number[] = [];
    const readable: boolean[] = [];
    for (const rowCount of [10_000, 100_000, 1_000_000]) {
      const harness = harnessOf();
      const cache = cacheWith({ transport: harness.transport, rowCount });
      // 窓を毎回新しくしながら先へ走査する（10 万行のシートを走る状況）。
      let last = 0;
      for (let start = 0; start < 3600; start += 300) {
        cache.setVisibleSpan({ start, count: 40 });
        cache.getCell({ row: start, column: 0 });
        await settle();
        last = start;
      }
      counts.push(cache.windowCount);
      readable.push(!cache.getCell({ row: last, column: 0 }).loading);
    }
    // 窓の数は上限で頭打ちになる（行数に比例しない）。可視の窓は記憶に残っている。
    expect(counts).toEqual([MAX_WINDOWS, MAX_WINDOWS, MAX_WINDOWS]);
    expect(readable).toEqual([true, true, true]);
    expect(MAX_WINDOWS).toBeGreaterThanOrEqual(4);
  });

  it("窓の行数の既定は可視の数画面ぶんである", () => {
    // 幅の根拠は `windowCache.ts` の定数の doc と design.md / research.md にある。
    expect(WINDOW_ROWS).toBeGreaterThanOrEqual(128);
    expect(WINDOW_ROWS).toBeLessThanOrEqual(512);
  });
});

// ---------------------------------------------------------------------------
// 10. 移植口を通した読み込み中
// ---------------------------------------------------------------------------

/** 移植口の仕様（セルの出所だけが検査の主題であり、知らせは何もしない）。 */
function specFor(cache: WindowCache, rowCount: number): RendererSpec {
  return {
    columns: [{ title: "名前", width: 120 }],
    rowCount,
    getCell: cache.getCell,
    onSelectionChange: () => undefined,
    onActivateEditor: () => undefined,
    onColumnResize: () => undefined,
    onColumnMove: () => undefined,
    onCopy: () => Promise.resolve(""),
    onPaste: () => Promise.resolve(),
  };
}

describe("移植口を通した読み込み中", () => {
  it("未取得のセルは読み込み中として描かれ、到着すると文字になる", async () => {
    const harness = harnessOf();
    const arrivals: string[] = [];
    const cache = cacheWith({
      transport: harness.transport,
      rowCount: 4,
      onArrival: (span) => arrivals.push(`${span.start}:${span.count}`),
    });
    const wiring = createGlideWiring(specFor(cache, 4));

    // 未取得: 骨組み（`Loading`）であり、**空の文字列ではない**。
    expect(wiring.props.getCellContent([0, 0]).kind).toBe(GridCellKind.Loading);
    await settle();

    const loaded = wiring.props.getCellContent([0, 0]);
    expect(loaded.kind).toBe(GridCellKind.Text);
    if (loaded.kind === GridCellKind.Text) {
      expect(loaded.displayData).toBe("0:0");
    }
    // 到着は呼び出し側へ報せる（画面が `RendererHandle.invalidate` を呼べるように）。
    // 区間は**実際に記憶した範囲**（要求した窓そのもの）である。
    expect(arrivals).toEqual(["0:4"]);
  });
});

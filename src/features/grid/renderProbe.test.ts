/**
 * 描画成立の検査と、走査中のフレーム時間の標本の契約（tasks.md 7.6。data-grid 要件 12.2、12.3）。
 *
 * # 何を固定するか
 *
 * 1. **塗って読み戻す検査**（要件 12.2）— 既知の図形を塗り 1 画素を読み戻す。とくに
 *    **意図的に描画を成立させない条件で不成立を返すこと**（tasks.md 7.6 の明文）を固定する
 * 2. **走査中のフレーム時間の中央値**（要件 12.3）— 偶数・奇数・標本が無い場合の規約
 * 3. **描画基盤の素性を問う手段を使わないこと**（design.md「RenderProbe」の Implementation
 *    Notes）と**起動時の描画経路の切り替えに踏み込まないこと**（同 Risks）— 源の走査で固定する
 * 4. 設計の 2 つの signature（`probePaint` / `sampleFrameTimes`）と、設計の型
 *    `RenderProbeResult` の関係 — 標本が無いことを `NaN` で表し、`null` へ移す場所を 1 つにする
 *
 * # 面（canvas）は渡される。本 file は面の代役を置く
 *
 * 走らせる環境は `node` である（`vitest.config.ts`）。本 module は**面を作らない**（1.6 の
 * 使い捨ての計測器は `document.createElement` で作っていたが、design.md の signature は
 * `probePaint(canvas)` である — どの面を問うかは呼び出し側が決める）。したがって検査に要るのは
 * `width` / `height` / `getContext("2d")` だけの代役であり、それで「塗れる面」と
 * **「塗られない面」の両方**を作れる — **後者が要件 12.2 の不成立の条件である。**
 *
 * # 1.6 の実測が本 file の数値の根拠である
 *
 * `research.md`「実測: 10 万行 × 30 列の走査中のフレーム時間（タスク 1.6）」が記録した:
 * 既知の色 `rgb(17,205,238)` を塗ると **`17,205,238,255`** が読み戻る。塗られた面の色数は
 * **52〜59** であり、**一様な面は 1 色**（1.6 は「1 なら何も塗られていない」と読んでいた。
 * 起動して観測した列も `色数=10` であり、いずれも 1 より大きい）。走査中のフレーム間隔の
 * **中央値は 17.00 ms**（1 ms 刻みの時計では 60 Hz の 16.67 ms が 17 として現れる。1.6 は
 * p90 と最大も記録している）。本 file の期待値はこの記録に合わせてある。
 *
 * # 単体では示せないこと（**正直に分けておく**）
 *
 * **「実物の WebKitGTK の面が実際に塗れる」ことは、本 file では示せない。** 本 file が示すのは
 * 「与えられた面の読み戻しが期待どおりなら成立と判定する」という**論理**である。実物の面の
 * 主張は**起動して観測する**しかない — 1.6 は `scripts/check-render-traversal.sh` と検証専用の
 * 画面（`src/features/smoke/glideProbe*`）でそれを測り、観測の行
 * （`塗り=ok 画素=17,205,238,255 色数=10`）をアクセシビリティの木から読んでいる。
 * **その恒久の観測は 9.2 / 9.3 が担う**（1.6 の段は一時的であり、その時点で取り除かれる）。
 */
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  FRAME_BACKSTOP_MARGIN_MS,
  countDistinctColors,
  probePaint,
  sampleFrameTimes,
  toRenderProbeResult,
} from "./renderProbe";
// 設計の型は名前で受ける（`ReturnType<typeof …>` で実装へ結び付けない）。**存在することの検査は
// `npm run typecheck` が担う**（`renderer/port.test.ts` と同じ方針）。
import type { RenderProbeResult } from "./renderProbe";

// ===========================================================================
// 面（canvas）の代役
// ===========================================================================

/** `getImageData` が返す 1 画素（`r,g,b,a`）。 */
type Pixel = readonly [number, number, number, number];

/** 面の代役の振る舞い。**「塗られない面」は既定ではなく、明示的に作る。** */
interface SurfaceOptions {
  /** 面の大きさ（既定は 8×8）。 */
  readonly size?: readonly [number, number];
  /** 面が描かれた色を実際に書くか。`false` は「何も塗られない」面（既定は `true`）。 */
  readonly paints?: boolean;
  /** `getContext("2d")` が返すもの。`null` なら 2D の文脈を作れない面（既定は文脈の代役）。 */
  readonly context?: "2d" | null;
  /** 位置ごとの画素。省略時は「自分が塗った色」を返す**正直な面**になる。 */
  readonly readbackAt?: (x: number, y: number) => Pixel;
  /** `getImageData` が投げるか（汚染された面）。 */
  readonly readbackThrows?: boolean;
}

/** 面の代役と、その面が**塗りを求められた記録**。 */
interface StandInCanvas {
  readonly canvas: HTMLCanvasElement;
  /** `fillRect` に渡された `fillStyle` の並び（塗るように求められた色。実際に書かれたかは別）。 */
  readonly fills: readonly string[];
}

/** `fillStyle` に渡された色を画素へ直す（面の代役が「塗った色」を覚えるため）。 */
function parseColor(color: string): Pixel {
  const text = color.trim();
  const hex = /^#([0-9a-f]{6})$/i.exec(text);
  if (hex?.[1] !== undefined) {
    const value = Number.parseInt(hex[1], 16);
    return [(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff, 255];
  }
  // `rgb(r, g, b)` / `rgb(r g b)` / `rgba(r, g, b, a)` を受ける。**綴りを 1 つに固定しない** —
  // 固定すると、同じ画素を塗る別の綴りへ書き換えただけで検査が落ちる（振る舞いは同じである）。
  const match = /^rgba?\(\s*(\d+)\s*[, ]\s*(\d+)\s*[, ]\s*(\d+)\s*(?:[,/]\s*([\d.]+)\s*)?\)$/i.exec(text);
  if (match === null) {
    // 解釈できない色は「何も塗られない」に倒す（誤って緑にしない）。
    return [0, 0, 0, 0];
  }
  const channel = (value: string | undefined, fallback: number): number =>
    value === undefined ? fallback : Number(value);
  const alpha = match[4] === undefined ? 255 : Math.round(Number(match[4]) * 255);
  return [channel(match[1], 0), channel(match[2], 0), channel(match[3], 0), alpha];
}

/** 面の代役を作る。 */
function createCanvas(options: SurfaceOptions = {}): StandInCanvas {
  const [width, height] = options.size ?? [8, 8];
  const fills: string[] = [];
  /** 面の裏側の 1 画素（**塗られない面では最後まで初期値のままである**）。 */
  let painted: Pixel = [0, 0, 0, 0];
  const context = {
    fillStyle: "#000000",
    fillRect: (): void => {
      // 記録は**求められた塗り**である（面が従うかどうかとは別）。
      fills.push(context.fillStyle);
      if (options.paints === false) {
        return;
      }
      if (options.readbackAt === undefined) {
        painted = parseColor(context.fillStyle);
      }
    },
    getImageData: (x: number, y: number): { data: Uint8ClampedArray } => {
      if (options.readbackThrows === true) {
        throw new Error("汚染された面");
      }
      return { data: Uint8ClampedArray.from(options.readbackAt?.(x, y) ?? painted) };
    },
  };
  const canvas = {
    width,
    height,
    getContext: (): unknown => (options.context === null ? null : context),
  };
  return { canvas: canvas as unknown as HTMLCanvasElement, fills };
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

// ===========================================================================
// 1. 塗って読み戻す検査（要件 12.2）
// ===========================================================================

describe("塗って読み戻す検査（要件 12.2）", () => {
  it("塗った色がそのまま読み戻せれば成立する（1.6 の `17,205,238,255` と同じ経路）", () => {
    const surface = createCanvas();

    expect(probePaint(surface.canvas)).toBe(true);
    // **渡された面に塗っている**（新しい面を作っていない）。塗らずに読むだけの検査なら、
    // 「DOM はあるが何も塗られない」を判定できない。
    expect(surface.fills.length).toBeGreaterThan(0);
  });

  it("意図的に描画を成立させない面では不成立を返す（何も塗られない）", () => {
    // 描き込みを捨てる面。**tasks.md 7.6 が名指しした不成立の条件である。**
    const surface = createCanvas({ paints: false });

    expect(probePaint(surface.canvas)).toBe(false);
    // 塗るように求めてはいる（面がそれに従わなかっただけである）。求めずに読んでいたなら、
    // この検査は「何も塗られない面」ではなく「何も塗らない検査」を見ていることになる。
    expect(surface.fills.length).toBeGreaterThan(0);
  });

  it("面が別の色を塗るなら不成立を返す（既知の色と一致することを求めている）", () => {
    // 「自分の内容を勝手に塗る面」 — 期待と違う色が読み戻る。
    const surface = createCanvas({ readbackAt: () => [255, 255, 255, 255] });

    expect(probePaint(surface.canvas)).toBe(false);
  });

  it("塗ったのに alpha が 0 なら不成立を返す（表示へ出ていない）", () => {
    const surface = createCanvas({ readbackAt: () => [17, 205, 238, 0] });

    expect(probePaint(surface.canvas)).toBe(false);
  });

  it("2D の文脈を作れない面では不成立を返す", () => {
    const surface = createCanvas({ context: null });

    expect(probePaint(surface.canvas)).toBe(false);
    expect(surface.fills).toHaveLength(0);
  });

  it("読み戻しが投げる面では不成立を返す（汚染された面）", () => {
    const surface = createCanvas({ readbackThrows: true });

    expect(probePaint(surface.canvas)).toBe(false);
  });

  it("大きさが 0 の面では不成立を返す（塗る場所が無い）", () => {
    const surface = createCanvas({ size: [0, 0] });

    expect(probePaint(surface.canvas)).toBe(false);
  });
});

// ===========================================================================
// 2. 面の色数（要件 12.2 の「無内容の領域を提示したまま留まらない」）
// ===========================================================================

describe("面の色数を数える（要件 12.2）", () => {
  it("一様な面は 1（何も塗られていない側の基準。1.6 の読み方と同じ）", () => {
    const surface = createCanvas({ readbackAt: () => [255, 255, 255, 255] });

    expect(countDistinctColors(surface.canvas)).toBe(1);
  });

  it("何か塗られている面は 2 以上（1.6 の実測は 52〜59、起動観測は 10）", () => {
    // 位置ごとに色が変わる面（表の文字と罫線の代役）。
    const surface = createCanvas({
      size: [80, 32],
      readbackAt: (x) => [x % 3, 0, 0, 255],
    });

    expect(countDistinctColors(surface.canvas)).toBe(3);
    // 判定は「1 より大きいか」である（数を固定しない — 表の内容で変わる）。
    expect(countDistinctColors(surface.canvas)).toBeGreaterThan(1);
  });

  it("読み戻せない面は 0（呼び出し側が測定不能と読む）", () => {
    const surface = createCanvas({ readbackThrows: true });

    expect(countDistinctColors(surface.canvas)).toBe(0);
  });

  it("大きさが 2 に満たない面は 0", () => {
    expect(countDistinctColors(createCanvas({ size: [1, 1] }).canvas)).toBe(0);
    expect(countDistinctColors(createCanvas({ context: null }).canvas)).toBe(0);
  });
});

// ===========================================================================
// 3. 走査中のフレーム時間の標本（要件 12.3）
// ===========================================================================

/**
 * `performance.now()` の代役。**渡した並びの順に値を返す**（尽きたら最後の値）。
 *
 * 実装は「開始時に 1 回、あとはフレームのコールバックごとに 1 回」時計を読む。したがって
 * 並びの 1 番目が開始時刻、2 番目以降がコールバックの時刻である。
 */
function installClock(times: readonly number[]): void {
  let index = 0;
  vi.spyOn(performance, "now").mockImplementation((): number => {
    const value = times[index] ?? times[times.length - 1] ?? 0;
    index += 1;
    return value;
  });
}

/** `requestAnimationFrame` の代役（**同期的に呼ぶ**。標本の本数を絞った検査なので再帰は浅い）。 */
function installAnimationFrames(): void {
  vi.stubGlobal("requestAnimationFrame", (callback: (time: number) => void): number => {
    callback(0);
    return 0;
  });
}

/** 10 ms / 20 ms / 30 ms / 40 ms の間隔を作る時刻（開始 = 0。1 本目は標本にしない）。 */
const EVEN_FRAMES: readonly number[] = [0, 100, 110, 130, 160, 200];
/** 10 ms / 20 ms / 30 ms の間隔を作る時刻。 */
const ODD_FRAMES: readonly number[] = [0, 100, 110, 130, 160];

describe("走査中のフレーム時間の標本（要件 12.3）", () => {
  it("標本が偶数個のときは、中央の 2 つの平均を返す", async () => {
    installClock(EVEN_FRAMES);
    installAnimationFrames();

    // 標本は [10, 20, 30, 40] であり、中央の 2 つ（20 と 30）の平均は 25 である。
    // 「中央のどちらか一方を返す」流儀なら 20 か 30 になるので、この値が規約を固定する。
    await expect(sampleFrameTimes(190)).resolves.toBe(25);
  });

  it("標本が奇数個のときは、中央の標本そのものを返す", async () => {
    installClock(ODD_FRAMES);
    installAnimationFrames();

    // 標本は [10, 20, 30] である。
    await expect(sampleFrameTimes(150)).resolves.toBe(20);
  });

  it("走査を始めるまでの間隔は標本にしない（1.6 の規律）", async () => {
    // 1 本目までに 1000 ms かかっている。混ぜれば中央値が 505 になる。
    installClock([0, 1000, 1010, 1020, 1030]);
    installAnimationFrames();

    await expect(sampleFrameTimes(1025)).resolves.toBe(10);
  });

  it("標本が 1 本も取れなければ NaN を返す（0 で埋めない）", async () => {
    // 期限が最初のコールバックと同時に来る条件（標本は 0 本）。
    installClock([0, 0]);
    installAnimationFrames();

    expect(Number.isNaN(await sampleFrameTimes(0))).toBe(true);
  });

  it("requestAnimationFrame が無い環境では NaN を返す", async () => {
    vi.stubGlobal("requestAnimationFrame", undefined);

    // **0 や Infinity で埋めない。** 0 は「速い」と読めてしまう（1.6 が同じ理由で null を選んだ）。
    expect(Number.isNaN(await sampleFrameTimes(100))).toBe(true);
  });

  it("フレームが 1 度も来なくても、期限を過ぎたら NaN で答える（約束を永久に保留しない）", async () => {
    vi.useFakeTimers();
    // 発火しない面（隠れた面で `requestAnimationFrame` が止まる症状。1.6 が実測している）。
    vi.stubGlobal("requestAnimationFrame", (): number => 0);

    const pending = sampleFrameTimes(50);
    await vi.advanceTimersByTimeAsync(50 + FRAME_BACKSTOP_MARGIN_MS);

    expect(Number.isNaN(await pending)).toBe(true);
  });
});

// ===========================================================================
// 4. 設計の `RenderProbeResult` を埋める
// ===========================================================================

describe("設計の RenderProbeResult を埋める", () => {
  it("標本が無いときは medianFrameMs が null になる（設計の型の意味）", () => {
    // **型の注釈が効き手である**（`npm run typecheck`）。`NaN` のまま運ぶと、`median > 予算` の
    // 比較が偽になり「測定不能」が「予算内」と読めてしまう。
    const result: RenderProbeResult = toRenderProbeResult(true, Number.NaN);

    expect(result).toEqual({ painted: true, medianFrameMs: null });
  });

  it("塗りの判定と中央値をそのまま運ぶ", () => {
    const result: RenderProbeResult = toRenderProbeResult(false, 17);

    expect(result).toEqual({ painted: false, medianFrameMs: 17 });
  });
});

// ===========================================================================
// 5. 禁じ手の走査（素性を問わない・起動時の経路を切り替えない）
// ===========================================================================
// 源は `import.meta.glob` の `?raw` で取り込む（`displayState.test.ts` と同じ方針。node の
// ファイル API を使わない）。綴りはリポジトリの根からの道である。
// ===========================================================================

const SOURCES = import.meta.glob("/src/**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** 本 file の主役。 */
const PROBE_SOURCE = "/src/features/grid/renderProbe.ts";
/** 1.6 の使い捨ての計測器。**UA を読む**（走査の生存を確かめる対照である）。 */
const THROWAWAY_SOURCE = "/src/features/smoke/glideProbeMeasure.ts";

/**
 * 注釈（`/* … *\/` と `// …`）を落とす（`displayState.test.ts` と同じ綴り）。**落とすのは、
 * 本 module の注釈が「なぜその手段を使わないか」を書くためである** — 素性の名前が注釈に現れる
 * ことは禁じ手ではない。逆に、落としすぎれば見落とす側に倒れる（誤って緑にはならない）。
 */
function codeOf(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/(^|[^:])\/\/[^\n]*/g, "$1");
}

/**
 * 禁じ手（design.md「RenderProbe」の Implementation Notes と Risks）。
 *
 * **素性を問う手段を使わない**（WebKit が指紋対策でレンダラ文字列を伏せるため、本製品の Linux と
 * macOS では機能しない）。**起動時の描画経路の切り替えに踏み込まない**（`app-shell` の要件 10.3
 * が既に所有する）。**面は渡される**（本 module は作らない）。
 */
const FORBIDDEN: readonly { readonly what: string; readonly pattern: RegExp }[] = [
  { what: "WEBGL_debug_renderer_info（伏せられる拡張）", pattern: /WEBGL_debug_renderer_info/ },
  { what: "UNMASKED_RENDERER / UNMASKED_VENDOR", pattern: /UNMASKED_(?:RENDERER|VENDOR)/ },
  { what: "getParameter（GL の素性を引く）", pattern: /\.\s*getParameter\s*\(/ },
  { what: "getExtension（素性を引く拡張）", pattern: /\.\s*getExtension\s*\(/ },
  { what: "navigator（素性の入口）", pattern: /\bnavigator\b/ },
  { what: "UA 文字列", pattern: /\buserAgent\b/ },
  { what: "ベンダー / レンダラの文字列", pattern: /\bvendor\b|\brendererString\b/i },
  { what: "面を自分で作る（渡された面を見るのが本 module の役である）", pattern: /createElement\s*\(/ },
  {
    what: "起動時の経路の切り替え",
    pattern: /__JXCEL_VERIFICATION__|location\s*\.\s*reload|process\s*\.\s*env/,
  },
  // **大域そのものの名前。** 上の綴りの一覧は文字列を組み立てれば潜れる（7.6 のレビューが実測:
  // `(globalThis as never)["navig" + "ator"]["user" + "Agent"]` が 23 件すべて緑のまま通った）。
  // しかし**本 module は取り込みが 0 件**（下の検査）なので、大域へ届く道は**大域の名前を
  // 直接書くこと**しかない — 取り込んだ束縛も引数も無いからである。したがって大域の名前を
  // 禁じれば、綴りを組み立てる潜り方も構造的に塞がる（`globalThis` / `self` / `Reflect` /
  // `Function` を経由する道も同時に塞がる）。
  { what: "大域そのもの（取り込み 0 件なので、これが唯一の入口である）", pattern: /\bglobalThis\b/ },
  { what: "大域そのもの（`self`）", pattern: /\bself\b/ },
  { what: "大域そのもの（`window` / `document` / `location` / `process`）", pattern: /\b(?:window|document|location|process)\b/ },
  { what: "大域を組み立てる道具（`Reflect` / `Function`）", pattern: /\b(?:Reflect|Function)\b/ },
];

/** 禁じ手に当たった札（空なら禁じ手はコードに現れていない）。 */
function violationsOf(code: string): readonly string[] {
  return FORBIDDEN.filter((entry) => entry.pattern.test(code)).map((entry) => entry.what);
}

/** `import … from "…"` / `export … from "…"` / `import "…"` の綴りを引く（素朴な走査である）。 */
const FROM_CLAUSE = /(?:^|[\n;])\s*(import|export)\s*([^;]*?)\s*from\s*["']([^"']+)["']/g;
/** 副作用だけの取り込み（`import "…"`）と、動的な取り込み・`require`。 */
const BARE_IMPORT = /(?:^|[\n;])\s*import\s*["']([^"']+)["']/g;
const DYNAMIC_IMPORT = /(?:^|[^\w$.])import\s*\(\s*["']([^"']+)["']\s*\)/g;
const REQUIRE_CALL = /(?:^|[^\w$.])require\s*\(\s*["']([^"']+)["']\s*\)/g;

/** 源が取り込む綴り（種類は問わない）。 */
function importsOf(code: string): readonly string[] {
  const specifiers: string[] = [];
  for (const match of code.matchAll(FROM_CLAUSE)) {
    const specifier = match[3];
    if (specifier !== undefined) {
      specifiers.push(specifier);
    }
  }
  for (const pattern of [BARE_IMPORT, DYNAMIC_IMPORT, REQUIRE_CALL]) {
    for (const match of code.matchAll(pattern)) {
      const specifier = match[1];
      if (specifier !== undefined) {
        specifiers.push(specifier);
      }
    }
  }
  return specifiers;
}

describe("禁じ手の走査（素性を問わない・起動時の経路を切り替えない）", () => {
  it("本 module は取り込みを 1 件も持たない（源の走査が実行時の閉包そのものである）", () => {
    // 取り込みが 1 件も無いので、**源の走査がそのまま実行時の閉包の走査になる**（別の module を
    // 経由して禁じ手へ届く経路が存在しない）。走査の生存は下の 2 件の対照が示す。
    expect(importsOf(codeOf(SOURCES[PROBE_SOURCE] ?? ""))).toEqual([]);
  });

  it("描画基盤の素性を問う手段と、起動時の経路の切り替えがコードに現れない", () => {
    expect(violationsOf(codeOf(SOURCES[PROBE_SOURCE] ?? ""))).toEqual([]);
  });

  it("対照: 禁じ手を 1 つ足した源は落ちる（走査が空回りしていない）", () => {
    // **変異の対照。** 実際の源に禁じ手を 1 つ足して、同じ走査がそれを捕まえることを示す。
    // これが無いと、走査は「何も見ていない」だけでも緑になる。
    const mutated = `${codeOf(SOURCES[PROBE_SOURCE] ?? "")}\nconst 素性 = navigator.userAgent;\n`;

    expect(violationsOf(mutated)).toEqual(["navigator（素性の入口）", "UA 文字列"]);
  });

  it("対照: 1.6 の使い捨ての計測器は UA を読み面を自分で作るので落ちる（別の源でも同じ走査が働く）", () => {
    // 1.6 の段では版の記録が必要だった（`research.md` が版に固有の欠陥を扱う）。また 1.6 は
    // 検査の面を自分で作っていた。**本機能の検査はそのどちらも持たない** — この対照は、走査が
    // 「たまたま何も見つけない」のではないことを、実在する別の源で示す。
    expect(violationsOf(codeOf(SOURCES[THROWAWAY_SOURCE] ?? ""))).toEqual([
      "navigator（素性の入口）",
      "UA 文字列",
      "面を自分で作る（渡された面を見るのが本 module の役である）",
      "大域そのもの（`window` / `document` / `location` / `process`）",
    ]);
  });

  it("潜ろうとした綴りも落ちる（大域の名前を禁じた理由）", () => {
    // 7.6 のレビューが実測した潜り方: 大域の名前を直接書けば、組み立てた綴りでは禁じ手の一覧を
    // 素通りできる（当時は 23 件すべて緑のまま通った）。**本 module は取り込みが 0 件**なので、
    // 大域へ届く道は大域の名前を書くことだけであり、そこを禁じればこの潜り方は塞がる。
    const slipped = [
      'const a = (globalThis as never)["navig" + "ator"]["user" + "Agent"];',
      'const b = self["navig" + "ator"];',
      'const c = Reflect.get(globalThis, "vend" + "or");',
      'const d = Function("return navig" + "ator")();',
    ];
    for (const source of slipped) {
      expect(violationsOf(codeOf(source)).length, source).toBeGreaterThan(0);
    }
  });
});

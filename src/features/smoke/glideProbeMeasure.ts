/**
 * 検証専用: 描画層の採否を決めるための**計測の実体**（tasks.md 1.6 / 要件 11.1、12.2、12.3）。
 *
 * 所有: 使い捨ての画面 [`GlideProbe`](./glideProbe.tsx) が読み込む**描画側の面**
 * [`GlideProbeGrid`](./glideProbeGrid.tsx)（このモジュールはその計測だけを担う）。
 *
 * # 何を測るか（tasks.md 1.6 の文言との対応）
 *
 *   1. **10 万行 × 30 列の走査中のフレーム時間の中央値** — 末尾へ向かう走査を行いながら
 *      `requestAnimationFrame` の間隔を標本に取り、**中央値**を求める（要件 11.1 の
 *      「毎秒 60 回の描画更新」を判定できる形）。
 *   2. **塗って読み戻す検査** — 既知の図形を canvas へ塗り、`getImageData` で 1 画素読み
 *      戻す（要件 12.2）。**「DOM はあるが何も塗られない」症状を捕まえる唯一の手段**である
 *      （WebKit は `WEBGL_debug_renderer_info` を伏せるため、レンダラの文字列では判定
 *      できない。research.md「描画成立の検出可能性」）。
 *
 * # 標本が取れなかったときは「取れなかった」と言う（重要）
 *
 * `requestAnimationFrame` は**隠れた面・間引かれた面では発火しない**（1.5 が実測した
 * 「隠れたタブ」の症状）。そのときに標本を空のまま 0 ms や `NaN` として扱うと、
 * **「測定不能」が「速い」と読めてしまう**。したがって標本が閾値に満たなければ
 * `status: "unmeasurable"` と理由を返し、`medianMs` は `null` にする（0 で埋めない）。
 *
 * # 標本の基準を結果に必ず載せる
 *
 * 中央値だけでは**その数が何に基づくか分からない**。したがって次の 3 つを必ず運ぶ:
 *
 *   - `frames`（標本にしたフレーム間隔の数）
 *   - `lastVisibleRow`（走査が末尾まで届いたか。届かなければ全件の走査ではない）
 *   - `rows` / `columns`（対象の宣言）
 *
 * # 出荷物に到達経路を作らない
 *
 * 本モジュールは [`glideProbeGrid`](./glideProbeGrid.tsx) だけが読み、その面は
 * `src/shell/Layout.tsx` の `__JXCEL_VERIFICATION__` の分岐の中だけで参照される
 * （`scripts/check-shipping-bundle.sh` が配布物の `dist/` を機械検査する）。
 */

/** 走査の計測の結果（`glideProbeGrid.tsx` の境界型と同じ形。**あちらが正本**）。 */
export interface GlideProbeMeasurement {
  /** 測定できたか。`"unmeasurable"` のときは `medianMs` は `null`。 */
  readonly status: "measured" | "unmeasurable";
  /** 測定不能の理由（`"measured"` のときは空文字）。 */
  readonly reason: string;
  /** 標本にしたフレーム間隔の数。 */
  readonly frames: number;
  /** フレーム時間の中央値（ミリ秒）。標本が無ければ `null`。 */
  readonly medianMs: number | null;
  /** 走査の間に実際に見えた最大の行番号（0 起点）。 */
  readonly lastVisibleRow: number;
  /** 宣言した行数・列数。 */
  readonly rows: number;
  readonly columns: number;
  /** 塗って読み戻す検査の結果。 */
  readonly paintOk: boolean;
  /** 読み戻した 1 画素（`r,g,b,a`。失敗したときは理由）。 */
  readonly paintPixel: string;
  /** 標本の面の canvas から数えた色数（1 なら一様＝何も塗られていない）。 */
  readonly gridColors: number;
  /** 実行環境の WebKitGTK の版（取れなければ `(不明)`）。 */
  readonly webkit: string;
  /**
   * `performance.now()` の刻み（ミリ秒。観測できなければ 0）。**中央値の読み方の前提である** —
   * 刻みが 1 ms なら 60 Hz の 16.67 ms は 17 ms として現れる（[`readTimerResolutionMs`]）。
   */
  readonly tickMs: number;
  /**
   * 標本の 90 パーセンタイルと最大（ミリ秒。標本が無ければ `null`）。**中央値だけでは
   * 滑らかさを語れない** — 落ち込むフレームの割合を示すために一緒に運ぶ（[`percentileOf`]）。
   */
  readonly p90Ms: number | null;
  readonly maxMs: number | null;
}

/**
 * 中央値を求める**純粋関数**。**入力を書き換えない**（複製して並べ替える）。
 *
 * 偶数個の標本では**中央の 2 つの平均**を返す（標本の数を 2 で割った位置の値だけを返す
 * 流儀もあるが、ここでは「フレーム時間の中央値」を素直に定義する）。空の配列では `null`
 * を返す — **0 で埋めない**（呼び出し側が「測定不能」と区別できなくなるため）。
 */
export function medianOf(samples: readonly number[]): number | null {
  if (samples.length === 0) {
    return null;
  }
  const sorted = [...samples].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) {
    return sorted[middle] ?? null;
  }
  const lower = sorted[middle - 1];
  const upper = sorted[middle];
  if (lower === undefined || upper === undefined) {
    return null;
  }
  return (lower + upper) / 2;
}

/**
 * 標本の合格に要する**最小のフレーム数**。**中央値を主張するのに足りる数**として置く
 * （1 標本の中央値はその 1 標本そのものであり、走査中の代表値にならない）。
 *
 * 走査は 10 万行を数十フレームで通り抜ける。12 本あれば走査の全区間に散り、かつ
 * `rAF` が間引かれた環境（面が隠れている）では到底届かない数である。
 */
export const MIN_FRAMES_FOR_MEDIAN = 12;

/**
 * 走査を駆動する**バッチあたりの行数**。**1 フレームで飛ばす行数**である。
 *
 * 小さすぎると走査が終わらないうちに標本の上限へ当たり、大きすぎると仮想化の効きが
 * 見えなくなる。可視は 15 行前後（520 px ÷ 34 px）なので、その 20 倍を 1 フレームで
 * 送る（10 万行は 300 フレーム強で通り抜ける）。
 */
export const ROWS_PER_FRAME = 300;

/**
 * 標本に取るフレーム間隔の上限。**上限を置くのは走査を必ず終わらせるためである**
 * （末尾へ届く前に打ち切った場合は `lastVisibleRow` が末尾でなくなり、全件の走査で
 * ないことが結果から分かる）。
 */
export const MAX_FRAME_SAMPLES = 2400;

/**
 * 実行環境の WebKitGTK の版を `navigator.userAgent` から読む。**取れなければ `(不明)`**（空文字に
 * しない — 空は「版が分からない」ではなく「版が無い」と読めてしまう）。
 *
 * 版を記録するのは、research.md が**特定の版（2.42 以降、実測は 2.52.x）に固有の欠陥**を
 * 扱っているためである（どの版で測ったかが分からない計測は比較できない）。
 */
export function readWebkitVersion(): string {
  const agent = typeof navigator === "undefined" ? "" : navigator.userAgent;
  const match = /AppleWebKit\/([0-9.]+)/.exec(agent);
  return match?.[1] === undefined ? "(不明)" : `AppleWebKit/${match[1]}`;
}

/**
 * 標本の**分位点**を求める（`[0,1]` の割合。標本が空なら `null`）。中央値と同じ規律で
 * **入力を書き換えない**。
 *
 * **中央値だけでは走査の滑らかさを語れない。** 例えば「中央値 17 ms」は、すべてのフレームが
 * 17 ms だった場合と、半分が 17 ms で半分が 33 ms だった場合の**どちらでも成り立つ**。
 * 要件 11.1 は「毎秒 60 回の描画更新を維持する」であり、**落ち込むフレームの割合**が
 * 判定の材料になる。したがって 90 パーセンタイルと最大も中央値と一緒に運ぶ
 * （[`medianOf`] は分位点の 0.5 の場合として書けるが、`medianOf` は 2 つの中央の平均という
 * 別の規約を持つので、名前と実装を分けてある）。
 */
export function percentileOf(
  samples: readonly number[],
  ratio: number,
): number | null {
  if (samples.length === 0) {
    return null;
  }
  const sorted = [...samples].sort((left, right) => left - right);
  // 最近傍の順位（補間しない）。標本が少数でも「実際に観測した値」だけを返すためである。
  const index = Math.min(
    sorted.length - 1,
    Math.max(0, Math.ceil(ratio * sorted.length) - 1),
  );
  return sorted[index] ?? null;
}

/**
 * `performance.now()` の**刻み**（ミリ秒）を実測する。**中央値をどう読むかの前提である。**
 *
 * WebKit は指紋対策として時刻の精度を粗くする（research.md「描画成立の検出可能性」が述べて
 * いるのと同じ理由で、レンダラの素性も伏せられる）。刻みが 1 ms であれば、60 Hz の表示周期
 * **16.67 ms** は**17 ms として読める** — つまり 16.67 をわずかに超える中央値は、要件を外した
 * 証拠ではなく**刻みの粗さの反映**でありうる。**その区別は記録された刻みが無ければ付けられない**
 * ので、中央値と同じ計測の中で刻みも取る（刻みを別の機会に測ると、同じ実行の中央値を説明
 * できない）。
 *
 * 取り方は「`performance.now()` を短い間だけ連続で読み、**最小の正の差**を取る」である。
 * 呼び出しは数ミリ秒で終わる（時計を 40 ms まで見るが、刻みが 1 ms なら 1 本目で決まる）。
 * 差が出なければ `0`（＝刻みを観測できなかった。呼び出し側は 0 を「粗さの主張の根拠に
 * 使わない」と読む）。
 */
export function readTimerResolutionMs(): number {
  if (typeof performance === "undefined") {
    return 0;
  }
  const deadline = performance.now() + 40;
  let smallest = 0;
  let previous = performance.now();
  for (;;) {
    const now = performance.now();
    const delta = now - previous;
    if (delta > 0 && (smallest === 0 || delta < smallest)) {
      smallest = delta;
    }
    previous = now;
    if (now >= deadline) {
      return smallest;
    }
  }
}

/**
 * 塗って読み戻す検査（要件 12.2）。**既知の図形を塗り、1 画素を読んで期待と比べる。**
 *
 * 検査に使う canvas は**画面に貼らない**（`document.createElement` して描き、読み、捨てる）。
 * 貼らない canvas でも 2D 文脈は同じ基盤を通るため、**「塗れない」環境ではここでも塗れない**
 * （これがこの検査の目的である）。加えて、**呼び出し元は標本の面の canvas も別に読む**
 * （[`countDistinctColors`]）— 貼った図形が読めるだけでは、グリッドが描かれていることの
 * 証拠にならないため、両方を見る。
 *
 * 戻り値の `pixel` は**読み戻せたときだけ** `r,g,b,a` の形になり、失敗したときは理由になる
 * （**成功と失敗が文字列の形で区別できる**ようにしてある）。
 */
export function probePaint(): { readonly ok: boolean; readonly pixel: string } {
  const expected = { r: 17, g: 205, b: 238 };
  try {
    const canvas = document.createElement("canvas");
    canvas.width = 8;
    canvas.height = 8;
    const context = canvas.getContext("2d");
    if (context === null) {
      return { ok: false, pixel: "2D 文脈を作れなかった" };
    }
    context.fillStyle = `rgb(${String(expected.r)}, ${String(expected.g)}, ${String(expected.b)})`;
    context.fillRect(0, 0, canvas.width, canvas.height);
    const data = context.getImageData(0, 0, 1, 1).data;
    const got = {
      r: data[0] ?? -1,
      g: data[1] ?? -1,
      b: data[2] ?? -1,
      a: data[3] ?? -1,
    };
    const pixel = `${String(got.r)},${String(got.g)},${String(got.b)},${String(got.a)}`;
    // 塗った色がそのまま読めることを要求する（**透明度も見る** — 塗ったのに alpha が 0 なら
    // 表示へ出ていない）。
    const ok =
      got.r === expected.r &&
      got.g === expected.g &&
      got.b === expected.b &&
      got.a === 255;
    return { ok, pixel };
  } catch (error: unknown) {
    return {
      ok: false,
      pixel: `読み戻せなかった: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}

/**
 * canvas に**実際に何色塗られているか**を数える（1 なら一様＝何も塗られていない）。
 *
 * 幾何の間引き（16 px ごと）で数える。**画素を全部読まない**のは、この検査が走査の直後に
 * 走るためである（読み出しの費用は描画の費用に混ざらないが、待たせる必要もない）。
 * 数えるのは**色の種類**であり、特定の色ではない — セルの文字と罫線が描かれていれば 2 以上に
 * なる、という**緩いが症状を捕まえる**判定である（要件 12.2 の「無内容の領域を提示したまま
 * 留まらない」）。
 */
export function countDistinctColors(canvas: HTMLCanvasElement): number {
  try {
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (context === null) {
      return 0;
    }
    const width = canvas.width;
    const height = canvas.height;
    if (width < 2 || height < 2) {
      return 0;
    }
    const step = 16;
    const colors = new Set<number>();
    for (let y = 0; y < height; y += step) {
      for (let x = 0; x < width; x += step) {
        const data = context.getImageData(x, y, 1, 1).data;
        colors.add(
          ((data[0] ?? 0) << 24) |
            ((data[1] ?? 0) << 16) |
            ((data[2] ?? 0) << 8) |
            (data[3] ?? 0),
        );
        if (colors.size > 64) {
          return colors.size;
        }
      }
    }
    return colors.size;
  } catch {
    // 読めない canvas（汚染されている等）は「色数 0」として返す（呼び出し側が測定不能にする）。
    return 0;
  }
}

/** 標本を集めるための入力（面が自分で用意する）。 */
export interface TraversalSamplingOptions {
  /** 1 フレームあたりに進める行数。 */
  readonly rowsPerFrame: number;
  /** 標本の上限。 */
  readonly maxSamples: number;
  /** 宣言した行数（末尾へ届いたかの判定に使う）。 */
  readonly rows: number;
  /** いま見えている末尾の行（0 起点）を読む。走査の到達点の観測に使う。 */
  readonly readLastVisibleRow: () => number;
  /** 1 フレームごとに呼ぶ（行を進める）。 */
  readonly advance: (row: number) => void;
}

/**
 * 走査の標本（フレーム間隔と到達点）。 */
export interface TraversalSamples {
  /** フレーム間隔（ミリ秒）。**最初の 1 本は含めない**（走査の開始前の間隔のため）。 */
  readonly frameTimes: readonly number[];
  /** 走査の間に観測した最大の可視行（0 起点）。 */
  readonly lastVisibleRow: number;
}

/**
 * 末尾へ着いてから**見え方の確定を待つフレーム数**。
 *
 * **これが要る理由は実測である。** `scrollTo` は「その行を見せろ」という**意図**を出し、
 * 実際に見えている行の報告（`onVisibleRegionChanged` → React の状態 → 参照）は**遅れて**
 * 届く。したがって末尾へ着いた瞬間に終わると、**意図は末尾なのに観測は 1 フレーム前**という
 * 状態で結果を作ってしまう（実測: 回避の環境変数を入れた条件で `到達行=99902 / 期待 99999`
 * となり、走査が末尾へ届かなかったのか、報告が遅れただけなのかを区別できなかった）。
 *
 * 10 フレーム（60 Hz で約 167 ms）待つ。**この間のフレーム間隔は標本に加えない** — 走査は
 * 既に終わっており、加えると表示周期そのものを標本へ混ぜて費用を過小に見せる。
 */
export const SETTLE_FRAMES = 10;

/**
 * 10 万行を走査しながらフレーム時間を標本に取る（tasks.md 1.6 / 要件 11.1）。
 *
 * **`requestAnimationFrame` の間隔をそのまま標本にする。** 描画が 1 フレームに収まっていれば
 * 間隔は画面のリフレッシュ周期（60 Hz なら 16.67 ms）に張り付き、収まらなければ伸びる。
 * したがって**間隔の中央値がそのまま「毎秒何回の描画更新を維持できたか」の尺度**である
 * （要件 11.1 の判定に使う閾値は 16.67 ms）。
 *
 * **標本が上限に達したら打ち切る。** その場合でも [`TraversalSamples.lastVisibleRow`] が
 * 返るので、呼び出し側は「末尾まで届いたか」を判定できる（打ち切りを黙って全件の走査として
 * 扱わない）。
 *
 * **末尾へ着いた後も [`SETTLE_FRAMES`] だけ待ってから終わる**（意図ではなく観測で到達点を
 * 決めるため。上記の doc を参照）。
 *
 * `requestAnimationFrame` が無い環境では**何も標本にせずに**戻る（呼び出し側が測定不能と
 * する）。
 */
export function sampleTraversal(
  options: TraversalSamplingOptions,
): Promise<TraversalSamples> {
  const { rowsPerFrame, maxSamples, rows, readLastVisibleRow, advance } = options;
  return new Promise<TraversalSamples>((resolve) => {
    if (typeof requestAnimationFrame !== "function") {
      resolve({ frameTimes: [], lastVisibleRow: readLastVisibleRow() });
      return;
    }
    const frameTimes: number[] = [];
    let previous = performance.now();
    let lastSeen = readLastVisibleRow();
    let row = 0;
    // **最初のコールバックの間隔は標本にしない**（それは「走査を始めるまで」の待ちであり、
    // 走査の費用ではない）。以降のコールバックが実際に走査しながら描いた間隔である。
    let first = true;
    let settle = 0;

    const step = (): void => {
      const now = performance.now();
      const walking = settle === 0;
      if (first) {
        first = false;
      } else if (walking) {
        frameTimes.push(now - previous);
      }
      previous = now;

      if (walking) {
        row = Math.min(row + rowsPerFrame, Math.max(rows - 1, 0));
        advance(row);
      }
      lastSeen = Math.max(lastSeen, readLastVisibleRow());

      const reachedEnd = row >= rows - 1;
      if (settle > 0 || reachedEnd) {
        // 末尾へ着いた後は、見え方の報告が追いつくまで**進めずに**待つ。
        settle += 1;
        if (settle > SETTLE_FRAMES) {
          resolve({ frameTimes, lastVisibleRow: lastSeen });
          return;
        }
        requestAnimationFrame(step);
        return;
      }
      if (frameTimes.length >= maxSamples) {
        resolve({ frameTimes, lastVisibleRow: lastSeen });
        return;
      }
      requestAnimationFrame(step);
    };

    requestAnimationFrame(step);
  });
}

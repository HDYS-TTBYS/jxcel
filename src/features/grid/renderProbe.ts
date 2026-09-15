/**
 * グリッド自身が実際に塗れたかの検査と、走査中のフレーム時間の標本（tasks.md 7.6。design.md
 * 「RenderProbe」。data-grid 要件 12.2、12.3）。
 *
 * 所有: グリッドの画面（8.1 / 8.8）と、劣化の記録（9.3）。本 module は**判定の論理だけ**を持ち、
 * 面（canvas）を作らず、起動時の描画経路を切り替えず、画面の状態を 1 つも持たない。
 *
 * # 要件との対応（何がどこで満たされるか）
 *
 * | 要件 | 何を求めるか | 本 module の役 |
 * |---|---|---|
 * | 12.2 | 描画が成立しないとき、識別できる情報を提示する | [`probePaint`] が成立 / 不成立を判定する。提示そのものは画面（8.1）が行う |
 * | 12.3 | 走査の滑らかさが要件を満たさなくなったとき、診断へ記録する | [`sampleFrameTimes`] が標本を取り中央値を返す。記録そのものは 9.3 が行う |
 *
 * **起動時の描画経路の切り替えには踏み込まない**（design.md「RenderProbe」の Risks）。
 * それは `app-shell` の要件 10.3 が既に所有しており、本 module は**グリッドの描画結果だけを見る。**
 *
 * # 素性を問わない（design.md「RenderProbe」の Implementation Notes）
 *
 * **WebGL の素性を問う手段を使わない。** WebKit は指紋対策としてレンダラ文字列を伏せるため、
 * 本製品の Linux と macOS では機能しない（`research.md`「描画成立の検出可能性」）。同じ理由で
 * **`navigator` を 1 度も読まない** — 版の記録は 1.6 の使い捨ての計測器が行ったことであり
 * （`readWebkitVersion`）、本 module の signature は真偽値しか返さないので、UA を読む場所が無い。
 * 実際に効く手段は「既知の図形を塗って 1 画素を読み戻す」ことだけである（要件 12.2）。
 *
 * # 標本が無いことをどう表すか（**設計の 2 つの signature と型の繋ぎ方**）
 *
 * design.md は 2 つの数の signature（`sampleFrameTimes(durationMs): Promise<number>`）と、
 * `medianFrameMs: number | null` を持つ [`RenderProbeResult`] の**両方**を与えている。数の型では
 * 「標本が 1 本も無い」を表せないので、[`sampleFrameTimes`] はそれを **`NaN`** で表し、
 * [`toRenderProbeResult`] が `NaN` を `null` へ移す**唯一の場所**である（9.3 が診断へ記録するのは
 * [`RenderProbeResult`] のほうである）。
 *
 * **生の中央値を直接比べてはならない。** `NaN > 予算` は偽であるため、`Number.isNaN` を置かずに
 * 比べると**「測定不能」が「予算内」と読めてしまう**（1.6 が「0 で埋めない」と決めたのと同じ
 * 罠である）。生の数を扱う呼び出しは、まず `Number.isNaN` を見ること。
 *
 * # 1.6 の実測が本 module の数値の根拠である
 *
 * `research.md`「実測: 10 万行 × 30 列の走査中のフレーム時間（タスク 1.6）」:
 *
 * - 既知の色 `rgb(17,205,238)` を塗ると **`17,205,238,255`** が読み戻る（本 module が塗る色は
 *   これである。透明度まで見るのは、塗ったのに alpha が 0 なら表示へ出ていないためである）
 * - 塗られた面の色数は **52〜59** であり、**一様な面は 1 色**（起動して観測したグリッドも
 *   `色数=10`）。よって「1 なら何も塗られていない、2 以上なら何か塗られている」と読む
 * - 走査中のフレーム間隔の**中央値は 17.00 ms**（p90 17.00、最大 18〜19。333 フレームで末尾まで
 *   到達）。**1 ms 刻みの時計では 60 Hz の 16.67 ms が 17 として現れる** — 中央値だけでは
 *   判定できないため、1.6 は分位点と最大も記録した。本 module は中央値だけを返し、
 *   分位点を要する読み方は 9.3 の記録の側の課題である
 *
 * # 単体で示せること / 起動でしか示せないこと（**正直な分界**）
 *
 * 本 module の単体検査（`renderProbe.test.ts`）が示すのは「与えられた面の読み戻しが期待どおりなら
 * 成立と判定する」という**論理**、中央値の規約、そして禁じ手がコードに現れないことである。
 * **「実物の WebKitGTK のグリッドの面が実際に塗れる」ことは単体では示せない** — それは
 * **アプリを起動して観測する**しかない（`tech.md`。1.6 は
 * `scripts/check-render-traversal.sh` と検証専用の画面 `src/features/smoke/glideProbe*` で測り、
 * `塗り=ok 画素=17,205,238,255 色数=…` の行をアクセシビリティの木から読んでいる）。
 * **その恒久の観測は 9.2 / 9.3 が担い、1.6 の一時的な段はその時点で取り除かれる。**
 */

/**
 * 描画成立の検査の結果（design.md「RenderProbe」の型そのまま）。
 *
 * `medianFrameMs` が `null` であることは「標本が 1 本も無く、中央値を主張できない」を意味する
 * （0 でも `NaN` でもない。[`toRenderProbeResult`] の doc を参照）。
 */
export interface RenderProbeResult {
  /** 既知の図形を塗って 1 画素を読み戻せたか（要件 12.2）。 */
  readonly painted: boolean;
  /** 走査中のフレーム間隔の中央値（ミリ秒）。標本が無ければ `null`（要件 12.3）。 */
  readonly medianFrameMs: number | null;
}

/**
 * 検査に塗る既知の色。**1.6 の実測と同じ色である** — `rgb(17,205,238)` を塗ると
 * `17,205,238,255` が読み戻る（`research.md`。起動時の観測の行に現れる `画素=17,205,238,255` と
 * 同じ値であり、段の側の読み手と突き合わせられる）。
 */
const PROBE_COLOR = { red: 17, green: 205, blue: 238 } as const;

/**
 * 検査に塗る図形の大きさ（画素）。**読み戻すのは (0,0) の 1 画素だけである。**
 *
 * 1.6 は貼らない 8×8 の面を自分で作り、その全体を塗っていた。本 module は**渡された面の左上**を
 * 2×2 画素だけ塗る — グリッドの canvas を渡された場合に覆う面積を最小にするためである
 * （2 画素にするのは、面の端の丸めに当たらない大きさとして選んだ）。
 */
const PROBE_TILE_PX = 2;

/**
 * 色数を数えるときに読む画素の間隔（画素）。
 *
 * **面の画素を全部は読まない。** この検査は走査の直後に走るので、読み出しで待たせない
 * （1.6 の計測器と同じ間隔であり、その実測値と比べられる）。
 */
const COLOR_COUNT_STEP_PX = 16;

/**
 * 色数を数えるのを止める数。**上限を置くのは、この判定が「1 か 1 より大きいか」しか見ないため
 * である**（表の内容によって色数は変わる。1.6 の実測でも 10 と 52〜59 の両方が現れている）。
 */
const COLOR_COUNT_LIMIT = 64;

/**
 * フレームが 1 度も来ないときに、期限からさらに待つ余白（ミリ秒）。
 *
 * **`requestAnimationFrame` は隠れた面では発火しない**（1.6 が実測した症状である）。この余白が
 * 無いと、期限を過ぎてもコールバックが来ない面で**約束が永久に解決しない** — 標本が無いことは
 * 「測定不能」という結果であって、答えが来ないことではない。250 ms は 60 Hz の 15 フレームぶん
 * であり、走査の滑らかさを語る窓（数百 ms の桁）に対して十分に小さい。
 */
export const FRAME_BACKSTOP_MARGIN_MS = 250;

/**
 * 標本の中央値。**入力を書き換えない**（複製して並べ替える）。
 *
 * 偶数個では**中央の 2 つの平均**を返す（1.6 の計測器と同じ規約であり、17.00 ms という記録も
 * この規約で読める）。**標本が無ければ `NaN`** を返す — 0 で埋めない（[`sampleFrameTimes`] の
 * doc を参照）。
 */
function medianOf(samples: readonly number[]): number {
  if (samples.length === 0) {
    return Number.NaN;
  }
  const sorted = [...samples].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) {
    return sorted[middle] ?? Number.NaN;
  }
  const lower = sorted[middle - 1];
  const upper = sorted[middle];
  if (lower === undefined || upper === undefined) {
    return Number.NaN;
  }
  return (lower + upper) / 2;
}

/**
 * 既知の図形を塗って 1 画素を読み戻し、描画が成立したかを判定する（要件 12.2）。
 *
 * **渡された面に塗る。** 面は呼び出し側が選ぶ（グリッドの canvas を渡せば「その面に塗れるか」を、
 * 使い捨ての面を渡せば「この環境で 2D の描画が通るか」を問う）。本 module は面を作らない
 * （1.6 の計測器は `document.createElement` で作っていたが、design.md の signature は面を受け取る
 * ので、その選択は呼び出し側の決定である）。
 *
 * **これが「DOM はあるが何も塗られない」症状を捕まえる唯一の実用的な手段である** — WebKit は
 * 指紋対策でレンダラ文字列を伏せるため、素性では判定できない（design.md の Implementation Notes）。
 *
 * **ただし、これだけでは「グリッドが描かれている」ことの証拠にならない。** 本関数は自分が塗った
 * 画素を読むので、面が自分の内容を 1 つも描いていなくても成立しうる。呼び出し側は
 * [`countDistinctColors`] も併せて見ること（1.6 が両方を見ていた理由である）。
 *
 * 成立しない場合は `false` を返す。**投げない**（原因は 2D の文脈が取れない・読み戻せない・
 * 既知の色と違う・alpha が 0、のいずれかであり、いずれも「成立しなかった」である）。
 */
export function probePaint(canvas: HTMLCanvasElement): boolean {
  if (canvas.width < 1 || canvas.height < 1) {
    return false;
  }
  try {
    const context = canvas.getContext("2d");
    if (context === null) {
      return false;
    }
    context.fillStyle = `rgb(${String(PROBE_COLOR.red)}, ${String(PROBE_COLOR.green)}, ${String(PROBE_COLOR.blue)})`;
    context.fillRect(0, 0, PROBE_TILE_PX, PROBE_TILE_PX);
    const pixel = context.getImageData(0, 0, 1, 1).data;
    // **透明度まで見る。** 塗ったのに alpha が 0 なら、読み戻せても表示へは出ていない。
    return (
      (pixel[0] ?? -1) === PROBE_COLOR.red &&
      (pixel[1] ?? -1) === PROBE_COLOR.green &&
      (pixel[2] ?? -1) === PROBE_COLOR.blue &&
      (pixel[3] ?? -1) === 255
    );
  } catch {
    // 汚染された面などでは `getImageData` が投げる。**どれも「成立しなかった」である。**
    return false;
  }
}

/**
 * 面に**実際に何色塗られているか**を数える。**1 なら一様＝何も塗られていない。**
 *
 * 要件 12.2 の症状（「DOM はあるが何も塗られない」＝要件の文言では「無内容の領域を提示したまま
 * 留まる」）は、[`probePaint`] の 1 画素の読み戻しだけでは捕まらない — あちらは自分が塗った画素を
 * 読むためである。**面そのものの内容を見るのがこの関数である。** 1.6 はこの 2 つを組で記録し、
 * 塗られた面で **52〜59 色**、一様な面で **1 色**を得ている（起動して観測したグリッドは 10 色）。
 * したがって呼び出し側の判定は **2 以上なら何か塗られている**である。
 *
 * 読めない面（汚染されている等）と、大きさが 2 に満たない面は **0** を返す（呼び出し側が
 * 測定不能として扱えるようにする。0 は「一様である」とは別の状態である）。**投げない。**
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
    const colors = new Set<number>();
    for (let y = 0; y < height; y += COLOR_COUNT_STEP_PX) {
      for (let x = 0; x < width; x += COLOR_COUNT_STEP_PX) {
        const pixel = context.getImageData(x, y, 1, 1).data;
        colors.add(
          ((pixel[0] ?? 0) << 24) |
            ((pixel[1] ?? 0) << 16) |
            ((pixel[2] ?? 0) << 8) |
            (pixel[3] ?? 0),
        );
        if (colors.size > COLOR_COUNT_LIMIT) {
          return colors.size;
        }
      }
    }
    return colors.size;
  } catch {
    return 0;
  }
}

/**
 * 走査の間のフレーム間隔を標本に取り、**中央値**を返す（要件 12.3）。
 *
 * **`requestAnimationFrame` の間隔をそのまま標本にする。** 描画が 1 フレームに収まっていれば
 * 間隔は表示の周期（60 Hz なら 16.67 ms）に張り付き、収まらなければ伸びる。したがって間隔の
 * 中央値がそのまま「毎秒何回の描画更新を維持できたか」の尺度であり、設計の閾値（16.67 ms。
 * 1.6 の実測は 17.00 ms）と比べられる。
 *
 * **最初の 1 本は標本にしない。** それは「走査を始めるまでの待ち」であり、走査の費用ではない
 * （1.6 の計測器と同じ規律である）。
 *
 * 標本を取るのは `durationMs` のあいだであり、**期限を過ぎた最初のフレームで終わる**。標本が
 * 1 本も無ければ **`NaN`** を返す — 0 でも `Infinity` でもない（0 は「速い」と読め、`Infinity` は
 * 測っていない遅さを主張する）。**`requestAnimationFrame` が無い環境でも `NaN` である。**
 *
 * **フレームが 1 度も来ない面でも答える。** 隠れた面では `requestAnimationFrame` が発火しない
 * （1.6 の実測）ので、期限に [`FRAME_BACKSTOP_MARGIN_MS`] を足した時計を置き、そこまでに
 * 1 本も来なければ「測定不能」として `NaN` を返す（約束を永久に保留しない）。一部だけ来た場合は
 * そこまでの標本の中央値を返す。
 *
 * **`NaN` をそのまま比べないこと**（module の doc を参照）。[`toRenderProbeResult`] を通すと
 * `null` になる。
 */
export function sampleFrameTimes(durationMs: number): Promise<number> {
  if (typeof requestAnimationFrame !== "function") {
    return Promise.resolve(Number.NaN);
  }
  return new Promise<number>((resolve) => {
    const started = performance.now();
    const samples: number[] = [];
    let previous = started;
    let first = true;
    let settled = false;
    const finish = (): void => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(backstop);
      resolve(medianOf(samples));
    };
    const backstop = setTimeout(finish, durationMs + FRAME_BACKSTOP_MARGIN_MS);
    const step = (): void => {
      const now = performance.now();
      if (first) {
        first = false;
      } else {
        samples.push(now - previous);
      }
      previous = now;
      if (now - started >= durationMs) {
        finish();
        return;
      }
      requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  });
}

/**
 * 2 つの signature の結果を、設計の型 [`RenderProbeResult`] へまとめる（要件 12.2、12.3）。
 *
 * 9.3 が診断へ記録するのはこの型であり、**標本が無いこと（`NaN`）を `null` へ移す唯一の場所**
 * である。生の数をこの関数を通さずに比べると、`NaN` との比較が常に偽になるため「測定不能」が
 * 「予算内」と読めてしまう（module の doc を参照）。
 */
export function toRenderProbeResult(
  painted: boolean,
  medianFrameMs: number,
): RenderProbeResult {
  // 数として意味を持たない値（標本が無いことを表す `NaN` など）は「報告できる中央値が無い」
  // として `null` にする。
  return { painted, medianFrameMs: Number.isFinite(medianFrameMs) ? medianFrameMs : null };
}

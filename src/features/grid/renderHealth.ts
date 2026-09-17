/**
 * 描画成立の検査と、走査の劣化の検出を**画面の振る舞いへ結線する**（tasks.md 9.3。data-grid
 * 要件 12.2、12.3）。
 *
 * 所有: グリッドの画面（`GridScreen.tsx` の `GridSurface`）。本 module は**判定の結線だけ**を
 * 持ち、判定の論理は 7.6 の [`./renderProbe`] にある（塗って読み戻す・色数を数える・フレーム
 * 時間の標本と中央値）。**画面は 1 つも持たない**（React を読み込まない）ので、`node` の環境の
 * 検査からそのまま組み立てられる。
 *
 * # 要件との対応（何がどこで満たされるか）
 *
 * | 要件 | 何を求めるか | 本 module の役 |
 * |---|---|---|
 * | 12.2 | 描画が成立しないとき、無内容の領域のまま留まらず、識別できる情報を提示する | [`createGridRenderHealth`] の `checkPaint` が表の面を検査し（**成立するまで 1 フレームずつ**。下の「面は非同期に現れる」）、**成立しなければ告知の文言を返す**（提示そのものは画面の既存の告知 1 行の腕が出す） |
 * | 12.3 | 走査の滑らかさが要件を満たさなくなったとき、その事実を診断情報に記録する | `scanned` が走査のたびにフレーム時間の標本を取り、**中央値が記録の閾値（要件値 + 計測の刻みの許容）を跨いだときに 1 回だけ**記録する |
 *
 * # 面は非同期に現れる（**1 フレーム目で結論しない**）
 *
 * 移植口の `mount` は React の根を作って描き始めるだけであり（`createRoot(container).render(…)`。
 * 7.2 の実装）、**面（canvas）もその中身も `mount` が返った時点には無い**。したがって検査は
 * **成立するまで 1 フレームずつ観測する**（上限は [`PAINT_PROBE_MS`] の経過時間）。1 回だけ
 * 見て結論する実装は、健全な環境でも「表の描画が成立しませんでした」を出してしまう。
 *
 * **何も描かれていない面には塗らない。**塗って読み戻す検査は自分の画素を読むので、空の面へ
 * 塗ると次の観測でその画素が「内容」に見える — 待つ実装と組にすると、**空の面が 2 フレーム目に
 * 成立してしまう**。この 2 つは必ず組で使う。
 *
 * # 記録の閾値は要件値 + 計測の刻みの許容である（**要件の合否は 9.2 が要件値で判定する**）
 *
 * **要件値（[`FRAME_BUDGET_MS`] = 16.67 ms）で記録の有無を決めてはならない。**1.6 の実画面の
 * 実測では、健全な走査の中央値が **17.00 ms** である（1 ms 刻みの時計。`research.md` の
 * 「中央値 17.00 ms の読み方」）ため、要件値で判定すると**健全な走査が `scan_below_budget`
 * として診断へ載り**、記録を読む側（9.2 の台本と人）に 1.6 が否定した結論を事実として渡す。
 *
 * したがって記録の閾値は [`FRAME_BUDGET_TOLERANCE_MS`]（1 ms）を足した **17.67 ms** である。
 * **要件の合否（11.1 の毎秒 60 回）は動かしていない** — それは 9.2 の実画面の観測が要件値で
 * 判定する。ここで緩めているのは**記録の閾値だけ**であり、記録は診断の材料である。
 * 検査（`GridScreen.test.ts`）は**健全な 17.00 で記録が出ず、劣化の 24.00 で出る**ことを
 * 陽性と陰性の対照で固定する。
 *
 * # 提示は告知 1 行に出る。**内容の領域を置き換えない**
 *
 * 12.2 の「識別できる情報」は、**画面が既に持っている告知の腕**（`GridScreenModel.notice`）へ
 * 出す。表そのものを差し替える（内容の領域を描画の説明に置き換える）経路は作らない — 1 つの
 * 失敗で表示中の表を失うと、利用者が見ていたものを失う（8.1 の `notice` の規律である）。
 *
 * したがって本 module は**文言を返すだけ**であり、状態遷移は画面の
 * `gridScreenFailed`（既存の 1 つ）が担う。
 *
 * # 記録は器の診断へ 1 件ずつ渡す（**札と数値だけを運ぶ**）
 *
 * 12.3 の記録の宛先は器（`app-shell` の診断の記録）である。本 module は
 * [`RenderHealthSink`]（記録の口）を**注入で受け取り**、その既定は
 * [`recordGridRenderHealth`]（境界の `diagnostics_record_render` を 1 回呼ぶ口）である。
 *
 * **自由な文字列を境界へ流さない。**運ぶのは**閉じた札**（[`RenderHealthReport.fact`] /
 * [`.failure`]）と**数値**（色数・フレーム時間・予算）だけであり、記録の 1 行を組み立てるのは
 * 器の側である（記録の注入面を広げない。10.8 がクリップボードの文字を記録へ出さなかったのと
 * 同じ規律である）。
 *
 * # 予算はマイクロ秒の整数で運ぶ
 *
 * 要件 11.1 の予算は **16.67 ms** であり、境界は**文字列と 32 ビット以下の整数**だけで構成する
 * （`ipc-contract.md`）。浮動小数を境界へ出さないため、ミリ秒は**マイクロ秒の整数**
 * （[`FRAME_BUDGET_US`] = 16670）へ直して運び、記録の 1 行を組み立てる側が `ms` へ戻す。
 *
 * # 標本が無いことを予算内と読まない（**7.6 の 1 箇所を再利用する**）
 *
 * [`FrameSampler`] の signature は `Promise<number>` であり、「標本が 1 本も無い」を `NaN` で
 * 表す（7.6 の [`./renderProbe`] の規約）。`NaN <= 予算` は**偽**であるため、生の数をそのまま
 * 比べると「測定不能」が「予算内」とも「予算超過」とも読めてしまう。本 module は
 * [`toRenderProbeResult`] を**必ず通して** `null`（報告できる中央値が無い）へ写し、**状態を
 * 1 つも動かさない** — 測定不能は「予算を満たしている」でも「満たしていない」でもない。
 *
 * **この規律が無いと記録が重複する。**測定不能で状態を「予算内」へ戻すと、その次に測った
 * 1 本が再び「跨いだ」と読め、**同じ劣化が走査のたびに記録され続ける**（検査
 * `GridScreen.test.ts` の「予算を跨いだときだけ 1 回記録する」が、**測定不能の標本を予算内の
 * 状態で先頭に置いて**固定する）。
 *
 * # 標本は 1 本ずつしか走らせない
 *
 * 走査は 1 秒に何十回も可視範囲を変えるので、変化のたびに標本を始めると**標本が積み上がる**。
 * 飛行中の標本があるときの走査は**1 本にまとめて**記憶し（`pending`）、いまの標本が終わった
 * ときに**1 回だけ**取り直す — 走査が続いている間は標本が途切れず、走査が止まれば取り直しも
 * 止まる。
 *
 * # 起動直後の 1 本は「走査」ではない（**記録の文言と測っている対象を合わせる**）
 *
 * 標本を始める引き金は**可視区間の知らせ**（`RendererSpec.onVisibleSpanChange`）である。移植口は
 * 可視区間を、利用者が走査したときだけでなく**取り付けの直後にも 1 回**知らせる（Glide が本当の
 * 区間を報せる瞬間である）。したがって**最初の標本が測るのは走査ではなく起動直後の読み込み**
 * でありうる。
 *
 * 本 module は**文言から「走査の」を落とす**側を選んだ（記録する事実は「1 秒ぶんのフレーム
 * 時間の中央値が予算を超えた」であり、利用者の操作を名乗らない）。**走査の標本が 1 本も無い間は
 * 記録しない、という側は選ばなかった** — 起動直後に描画が劣化している環境（まさに 12.3 が
 * 捕まえたい環境である）の記録を落とすことになるためである。境界の札
 * （`scan_below_budget`）は既存のままとし、**記録の 1 行を組み立てる器の側の文言で対象を
 * 名乗らない**（`diagnostics_cmds.rs`）。
 *
 * # 単体で示せること / 実画面でしか示せないこと（**正直な分界**）
 *
 * 本 module の検査（`GridScreen.test.ts` の 9.3 の節）が示すのは、**結線の論理**である —
 * 成立しない面で告知と記録が出ること、成立する面で何も出ないこと、記録の閾値を跨いだときだけ
 * 1 回記録されること（健全な 17.00 と劣化の 24.00 の対照）、測定不能が状態を動かさないこと、
 * 標本が重ならないこと、**画面が渡す 3 つの口（告知・可視区間の知らせ・記録）へ
 * [`installGridRenderHealth`] が繋がっていること**。**実物の WebKitGTK のグリッドの面が実際に
 * 塗れること**、**記録が実際に診断のファイルへ載ること**、**10 万行の走査で毎秒 60 回の更新が
 * 保たれること（11.1）**、**最初の画面が 1 秒以内に出ること（11.2）**、**編集の反映が
 * 100 ミリ秒以内であること（11.3）**は起動して観測するしかない（tech.md）— その恒久の観測は
 * 9.2 の台本が担い、判定は要件値で行う。
 */
import { invokeCommand, type CommandName } from "../../ipc/client";
import type { RenderHealthRecordRequest } from "../../ipc/bindings";
import {
  countDistinctColors,
  probePaint,
  sampleFrameTimes,
  toRenderProbeResult,
} from "./renderProbe";
import type { VisibleSpan } from "./renderer/port";

/**
 * 要件 11.1 の予算（毎秒 60 回の描画更新）。**要件値そのものであり、記録の閾値ではない。**
 *
 * 1 ms 刻みの時計では 60 Hz の周期 16.67 ms が 17 として現れるため、**この値で記録の有無を
 * 決めると、健全な走査（1.6 の実画面の実測で中央値 17.00 ms）が劣化として診断へ載る**
 * （`research.md` の「中央値 17.00 ms の読み方」）。記録の閾値は [`FRAME_BUDGET_TOLERANCE_MS`]
 * を足したものである。**要件の合否はこの値で判定する** — その判定は 9.2 の実画面の観測が担い、
 * 本 module は**記録の閾値だけ**を持つ（下の「記録の閾値」）。
 */
export const FRAME_BUDGET_MS = 16.67;

/** [`FRAME_BUDGET_MS`] を**マイクロ秒の整数**で表したもの（境界は整数しか運べない）。 */
export const FRAME_BUDGET_US = Math.round(FRAME_BUDGET_MS * 1000);

/**
 * 記録の閾値に足す**計測の刻みの許容**（ミリ秒）。
 *
 * 根拠は 1.6 の実画面の実測 2 つである（`research.md`）:
 *
 * - **健全な走査は中央値 17.00 ms**（`performance.now()` の刻みが 1.00 ms であり、60 Hz の
 *   16.67 ms はその刻みでは 17 として現れる。時刻の刻みに依らないテレスコープ平均は 16.68 ms、
 *   表示面は 59.97 Hz）
 * - **劣化した走査（`WEBKIT_DISABLE_DMABUF_RENDERER=1`）は中央値 24.00 ms**
 *
 * 1 ms（＝時計の 1 刻み）を足した **17.67 ms** を境にすると、健全な 17.00 では記録が出ず、
 * 劣化の 24.00 では出る。**要件を緩めているのではない** — 11.1 の合否は 9.2 の実画面の観測が
 * [`FRAME_BUDGET_MS`] で判定する。ここで緩めているのは**記録の閾値だけ**である。記録は診断の
 * 材料であり、時計の刻みの内側の値を「要件を外した」と記録すると、**健全な走査が劣化として
 * 診断へ載り、9.2 の台本が読む記録が嘘になる**。
 */
export const FRAME_BUDGET_TOLERANCE_MS = 1.0;

/**
 * 走査のフレーム時間を標本する長さ（ミリ秒）。
 *
 * 60 Hz の面では約 60 本の標本が取れる（1.6 は「中央値を主張するのに 12 本要る」として
 * `MIN_FRAMES_FOR_MEDIAN` を置いていた）。1 秒は走査の滑らかさを語る窓として十分であり、
 * 標本が終われば走査が続いている限り取り直す（1 本ずつしか走らせない）。
 */
export const SCAN_SAMPLE_MS = 1000;

/**
 * 面（canvas）とその中身が現れるのを待つ上限（**経過時間**。ミリ秒）。
 *
 * **フレーム数ではなく時間で数える。**1.6 の独立レビューが訂正したとおり、Xvfb の面は
 * **フレーム間隔 6〜7 ms** で走ることがあり（`research.md`）、フレーム数を固定すると**待つ長さが
 * 面の速さで変わる**（30 フレームは 60 Hz なら約 0.5 秒、6.7 ms の間隔なら約 0.2 秒である）。
 * 移植口の `mount` は面をその場で作らない（React の描画はそのあとのフレームで起きる）ため、
 * 待つべきは**最初の描画が現れるまでの時間**であり、その上限は経過時間で表すのが正しい。
 *
 * # 上限は「最初の窓が届くまで」を覆わなければならない（9.2 の実起動の実測）
 *
 * 9.2 の観測（10 万行の実物のシートを開く起動）で、**面が空のまま 500 ms を過ぎる**ことを
 * 実測した — `GridSurface` が mount する時点では**最初の窓がまだ届いていない**（窓は
 * `grid_rows_window` の往復であり、100 万セル級のシートでは 1 秒前後かかる）。当初の 500 ms は
 * **健全な起動でも `paint_failed` を記録していた**（実測: `diagnostics_record_render: 表の描画が
 * 成立しなかった（理由 = 面に何も塗られていない）` が通常の起動で 1 行出た）。
 *
 * したがって上限は**最初の窓の到着を含む**長さにする。3 秒は 1.6 と本観測の実測（1 秒前後）に
 * 対して十分な余裕であり、**不成立のときに告知が出るまで 3 秒かかる**ことを受け入れる
 * （無内容の領域のまま留まるより遅れて出る方がよい）。
 */
export const PAINT_PROBE_MS = 3_000;

/**
 * 上の上限に添える**フレーム数の歯止め**。
 *
 * 経過時間だけを条件にすると、**時計が進まない環境**（検査の代役の `nextFrame` が即座に返る
 * 並び）で取り直しが終わらない。2 ms 間隔（500 Hz）より速い面ではこの歯止めが先に来るが、
 * そのような面は現実に無い — **主の上限は経過時間であり、これは回り続けないための守りである。**
 */
const PAINT_PROBE_GUARD_FRAMES = Math.ceil(PAINT_PROBE_MS / 2);


/**
 * 表の描画が成立しなかった理由の種別（要件 12.2 の「識別できる情報」）。
 *
 * **3 つは排他である**: 面が無い（`no_canvas`）／面はあるが塗って読み戻せない
 * （`unpaintable`）／面は塗れるが何も描かれていない（`blank`）。1.6 が「塗り」と「色数」を
 * 組で記録したのは、この 2 つが**別の失敗**だからである（`probePaint` は自分が塗った画素を
 * 読むので、それだけでは「グリッドが描かれている」ことの証拠にならない）。
 */
export type PaintFailureKind = "no_canvas" | "unpaintable" | "blank";

/**
 * 診断へ渡す記録 1 件。**札と数値だけ**であり、自由な文字列を持たない。
 *
 * **2 つの腕は互いに素である**（境界の合併型 `RenderHealthReport` と同じ形である）。同じ事実を
 * 平たい欄の集まりで表すと「走査の劣化なのに理由の種別が載っている」ような状態が作れてしまい、
 * 記録の読み手が「埋まっている欄だけを見る」規律に頼ることになる。
 *
 * `medianUs` / `budgetUs` が走査の腕にだけ載るのは、**予算がその判定にしか使われない**ためで
 * ある（12.2 の検査は「塗れたか」しか見ない）。
 */
export type RenderHealthReport =
  | {
      /** 表の描画が成立しなかった（要件 12.2）。 */
      readonly fact: "paint_failed";
      /** 成立しなかった理由の種別。 */
      readonly failure: PaintFailureKind;
      /** 数えた色数。面が読めなかったときは `null`（0 は「一様である」とは別の状態である）。 */
      readonly colors: number | null;
    }
  | {
      /**
       * フレーム時間の中央値が**記録の閾値**（要件値 + 計測の刻みの許容）を超えた（要件 12.3）。
       */
      readonly fact: "scan_below_budget";
      /** 測定したフレーム時間の中央値（**マイクロ秒**）。 */
      readonly medianUs: number;
      /** **要件値**の予算（**マイクロ秒**。16670）。記録の閾値（17.67 ms）はここへ載らない。 */
      readonly budgetUs: number;
    };

/** 記録の口（器の診断の記録）。**既定は [`recordGridRenderHealth`]。検査は偽の口を渡す。** */
export type RenderHealthSink = (report: RenderHealthReport) => void;

/**
 * フレーム時間の標本の取り方（`(durationMs) => Promise<medianMs>`）。
 *
 * **これは 7.6 の [`sampleFrameTimes`] の signature そのものである。**標本が 1 本も無いことは
 * `NaN` で表される（0 でも `Infinity` でもない）ので、生の数を比べてはならない
 * （module doc の「標本が無いことを予算内と読まない」）。
 */
export type FrameSampler = (durationMs: number) => Promise<number>;

/**
 * 表の面を探す器。**`HTMLElement` の全部を要求しない** — 表を組み立てた効果が読むのは面
 * （canvas）1 つだけであり、検査は器の代役を渡せる（`GridScreen.test.ts`）。
 */
export interface PaintProbeContainer {
  readonly querySelector: (selectors: string) => Element | null;
}

/** 表の面の検査の結果（要件 12.2）。 */
export interface PaintProbeOutcome {
  /** 塗って読み戻せて、かつ面が無内容でないか。 */
  readonly painted: boolean;
  /** 成立しなかった理由の種別（成立したなら `null`）。 */
  readonly failure: PaintFailureKind | null;
  /** 数えた色数。**面が 1 つも無いときだけ `null`**（0 は「一様である」とは別の状態である）。 */
  readonly colors: number | null;
}

/**
 * 表の面を 1 回検査する（要件 12.2）。**面の読み出しだけを行い、状態を持たない。**
 *
 * 順序が意味を持つ: **色数を先に数え、そのあとで塗る。** 逆にすると、自分が塗った既知の色が
 * 色数へ加わり、**一様な（何も描かれていない）面が 2 色に見える** — 12.2 の症状
 * （無内容の領域のまま留まる）をちょうど見落とす。
 *
 * 面が取れないとき（`querySelector` が `null` を返す、あるいは canvas でない要素が返る）は
 * `no_canvas` である。**これは実際の症状である**: 移植口が描き始めていなければ面が無い。
 * 移植口の面が canvas でない場合も同じ扱いにする（読み戻す手段が無いため、成立を主張できない）。
 */
export function probeGridSurface(
  container: PaintProbeContainer | null,
): PaintProbeOutcome {
  // 面は**綴りで見る**（`node` の環境に `HTMLCanvasElement` が無いので `instanceof` を使わない）。
  const element = container?.querySelector("canvas") ?? null;
  const canvas =
    typeof (element as { getContext?: unknown } | null)?.getContext === "function"
      ? (element as HTMLCanvasElement)
      : null;
  if (canvas === null) {
    return { painted: false, failure: "no_canvas", colors: null };
  }
  // **先に数える。**順序が逆だと、自分が塗った既知の色が色数へ加わり、一様な面が 2 色に見える。
  const colors = countDistinctColors(canvas);
  if (colors === 0) {
    // 面を読めない（2D の文脈が取れない・読み戻しが失敗する・大きさが足りない）。
    return { painted: false, failure: "unpaintable", colors };
  }
  if (colors < 2) {
    // **何も描かれていない面には塗らない。**塗れば、次の観測で自分の画素が「内容」に見え、
    // 無内容な面を成立と読むことになる（[`createGridRenderHealth`] は成立するまで何度か
    // 観測するので、この規律が無いと**空の面が 2 フレーム目で成立する**）。
    return { painted: false, failure: "blank", colors };
  }
  // 面に内容があると分かってから塗って読み戻す。**塗った色が読み戻せないとき**だけ
  // `unpaintable` である（1.6 が実測した「塗られた面は 52〜59 色」の側の検査である）。
  return probePaint(canvas)
    ? { painted: true, failure: null, colors }
    : { painted: false, failure: "unpaintable", colors };
}

/**
 * 成立しなかったことを識別できる文言（要件 12.2）。**告知 1 行へそのまま出す。**
 *
 * 文言は「表の描画が成立しませんでした」＋**理由の種別**である。色数が読めているときは数を
 * 添える（利用者が診断の記録と突き合わせられるようにする）。
 */
export function paintFailureNotice(
  failure: PaintFailureKind,
  colors: number | null,
): string {
  switch (failure) {
    case "no_canvas":
      return "表の描画が成立しませんでした: 表の面が見つかりません";
    case "unpaintable":
      return "表の描画が成立しませんでした: 面に塗って読み戻せません";
    case "blank":
      return `表の描画が成立しませんでした: 面に何も塗られていません（色数=${String(colors ?? 0)}）`;
  }
}

/** [`createGridRenderHealth`] の入力。 */
export interface GridRenderHealthOptions {
  /** 記録の口。省略すると器の診断へ記録する（[`recordGridRenderHealth`]）。 */
  readonly record?: RenderHealthSink;
  /** 標本の取り方。省略すると 7.6 の [`sampleFrameTimes`]。**検査が差し替える。** */
  readonly sample?: FrameSampler;
  /** 標本を取る長さ（ミリ秒）。省略すると [`SCAN_SAMPLE_MS`]。 */
  readonly durationMs?: number;
  /**
   * 次のフレームまで待つ口。省略すると `requestAnimationFrame`（無い環境では即座に返る）。
   * **検査が差し替える**（面が現れるのを待つ順序を、フレームを数えて固定できるようにする）。
   */
  readonly nextFrame?: () => Promise<void>;
  /**
   * 経過を測る時計（ミリ秒）。省略すると `performance.now`（無い環境では `Date.now`）。
   * **検査が差し替える** — 待ちの上限（[`PAINT_PROBE_MS`]）がフレーム数ではなく経過時間で
   * あることを、時計を進めて固定できるようにする。
   */
  readonly now?: () => number;
  /** 予算（ミリ秒）。省略すると [`FRAME_BUDGET_MS`]。 */
  readonly budgetMs?: number;
}

/**
 * 画面が持つ描画の健全性の口。**`GridSurface` の組み立ての効果が作り、後始末で捨てる。**
 */
export interface GridRenderHealth {
  /**
   * 表を組み立てた**後**に 1 回呼ぶ（要件 12.2）。**成立しなければ記録し、告知の文言を返す。**
   *
   * **面とその中身が現れるのを待つ**（[`PAINT_PROBE_MS`] まで、1 フレームずつ観測する）。
   * 移植口の `mount` は面をその場で作らないため、1 フレーム目で結論すると健全な環境でも
   * 不成立と読めてしまう（定数の doc）。**まだ読めない面（色数 0）は取り直す** — 移植口は
   * 面を組み立てる途中であり、9.2 の実起動の実測でこれが**健全な起動の誤検知**を生んでいた
   * （実装の doc を参照）。**色数を読めたうえで塗って読み戻せないことだけは取り直さない。**
   *
   * 表を組み立て直す（列の構成や行の集合が変わる）たびに 1 回走る — 表はそのつど新しく描かれる
   * ので、**新しく無内容になったのなら、それは新しい事実である**。毎フレーム検査し続ける経路は
   * 無い（費用も意味も無い）。
   *
   * **捨てた後（[`GridRenderHealth.dispose`]）は告知を返さない**（`null` を返す）。遅れて
   * 届いた結果で、もう無い表について告知しない。
   */
  readonly checkPaint: (container: PaintProbeContainer | null) => Promise<string | null>;
  /**
   * 可視範囲が変わった（走査）。標本を取り、**記録の閾値を跨いだときに 1 回だけ**記録する
   * （要件 12.3）。**走査のたびに呼んでよい**（飛行中の標本は 1 本にまとめる）。
   */
  readonly scanned: () => Promise<void>;
  /** 後始末。**以後の標本の結果は記録しない**（捨てた面について記録しない）。 */
  readonly dispose: () => void;
}

/**
 * 描画の健全性の**判断**を作る（要件 12.2、12.3）。**表の面の検査と、走査の標本の本体である。**
 *
 * 効果は走らせないと観測できない（`node` の環境には DOM が無く、`renderToStaticMarkup` は効果を
 * 実行しない）。したがって判断はこの関数へ、**効果の形（画面の口との結線）は
 * [`installGridRenderHealth`] へ**集めてある — `GridScreen.tsx` の効果に残るのは「作る・2 つの
 * 位置へ渡す・捨てる」だけである（`followSelection` / `createGridCopyEntry` と同じ切り出し方で
 * ある）。
 *
 * **状態は 3 つだけ**である: いま記録の閾値を満たしているか（`underBudget`）と、飛行中の標本が
 * あるか（`inFlight` / `pending`）と、捨てたか（`settled`）。記録は**向きの変化**にだけ反応する。
 */
export function createGridRenderHealth(
  options: GridRenderHealthOptions = {},
): GridRenderHealth {
  const record = options.record ?? recordGridRenderHealth;
  const sample = options.sample ?? sampleFrameTimes;
  const durationMs = options.durationMs ?? SCAN_SAMPLE_MS;
  const nextFrame =
    options.nextFrame ??
    ((): Promise<void> =>
      new Promise<void>((resolve) => {
        // **`requestAnimationFrame` が無い環境では即座に返る**（`node` の検査である。待つ先が
        // 無いだけで、判定の順序は変わらない）。
        if (typeof requestAnimationFrame !== "function") {
          resolve();
          return;
        }
        requestAnimationFrame(() => {
          resolve();
        });
      }));
  const budgetMs = options.budgetMs ?? FRAME_BUDGET_MS;
  const budgetUs = Math.round(budgetMs * 1000);
  // **`performance` が無い環境では `Date.now` に落とす**（待ちの上限だけに使う時計であり、
  // 細かさは判定に効かない）。
  const now =
    options.now ??
    (typeof performance === "undefined"
      ? (): number => Date.now()
      : (): number => performance.now());
  /**
   * 記録の閾値（要件値 + 計測の刻みの許容）。**要件の判定値ではない**
   * （[`FRAME_BUDGET_TOLERANCE_MS`] の doc）。
   */
  const recordThresholdMs = budgetMs + FRAME_BUDGET_TOLERANCE_MS;

  /** 捨てたか（捨てた面について記録しない）。 */
  let settled = false;
  /** 飛行中の標本。 */
  let inFlight: Promise<void> | null = null;
  /** 飛行中に来た走査（**1 本にまとめる**。走査が続いている間は標本が途切れない）。 */
  let pending = false;
  /**
   * いま記録の閾値を満たしているか。**初期値は「満たしている」である** — 起動直後の面は通常の
   * 描画経路であり、はじめて測った 1 本が閾値を超えていればそれが 1 回目の記録になる。
   */
  let underBudget = true;

  const observe = (rawMedianMs: number): void => {
    // **`NaN` を閾値の内側と読ませない唯一の道**（7.6 の 1 箇所を再利用する。module doc）。
    const median = toRenderProbeResult(true, rawMedianMs).medianFrameMs;
    if (median === null) {
      // 測定不能（標本が 1 本も無い）。**どちらの状態でもない** — 状態を動かさない。
      return;
    }
    if (median <= recordThresholdMs) {
      // 閾値の内側である。**1.6 の健全な実測（17.00 ms）はここへ落ちる**
      // （[`FRAME_BUDGET_TOLERANCE_MS`] の doc）。
      underBudget = true;
      return;
    }
    if (!underBudget) {
      // 同じ状態が続いている（**跨いでいない**）。記録を増やさない。
      return;
    }
    underBudget = false;
    record({
      fact: "scan_below_budget",
      medianUs: Math.round(median * 1000),
      budgetUs,
    });
  };

  const start = (): Promise<void> => {
    const run = (async (): Promise<void> => {
      try {
        const median = await sample(durationMs);
        if (!settled) {
          observe(median);
        }
      } catch (error: unknown) {
        // **投げない。**標本が取れないことは、描画の劣化とは別の事実である（器の記録へは
        // 出さず、開発者向けの記録に留める）。
        console.warn("走査のフレーム時間の標本を取れなかった", error);
      } finally {
        inFlight = null;
      }
      if (!settled && pending) {
        pending = false;
        await start();
      }
    })();
    inFlight = run;
    return run;
  };

  return {
    checkPaint: async (container: PaintProbeContainer | null): Promise<string | null> => {
      let outcome = probeGridSurface(container);
      // **面とその中身は非同期に現れる**（[`PAINT_PROBE_MS`] の doc）。まだ判定できない状態に
      // 限って、次のフレームで取り直す:
      //
      // - 面が無い（`no_canvas`）
      // - 面はあるが**何も読めない**（`unpaintable` かつ色数 0）— 2D の文脈がまだ無い、または
      //   大きさが 2 画素未満（移植口が面を用意する途中）。**9.2 の実起動の実測**: 健全な
      //   起動でも、組み立ての直後に走るこの検査が色数 0 を読み、**誤って「塗って読み戻せない」と
      //   記録していた**（表は数百ミリ秒後に 57 色で塗られていた）。色数 0 は「塗れない」では
      //   なく「**まだ読めない**」である。
      // - 面は読めるが一様（`blank`）
      //
      // **色数を読めたうえで塗って読み戻せないこと（`unpaintable` かつ色数 >= 1）は取り直さない**
      // — 面に内容があるのに自分が塗った色が返らないのは、待っても変わらない状態である。
      //
      // 待ちの上限は**経過時間**である（フレーム数ではない。面の速さで待つ長さが変わらない
      // ようにするため）。`PAINT_PROBE_GUARD_FRAMES` は、時計が進まない環境（検査の代役が
      // 即座に返る並び）で回り続けないための歯止めである。
      const deadline = now() + PAINT_PROBE_MS;
      for (
        let attempt = 1;
        attempt < PAINT_PROBE_GUARD_FRAMES &&
        now() < deadline &&
        !settled &&
        (outcome.failure === "no_canvas" ||
          outcome.failure === "blank" ||
          (outcome.failure === "unpaintable" && (outcome.colors ?? 0) === 0));
        attempt += 1
      ) {
        await nextFrame();
        outcome = probeGridSurface(container);
      }
      const failure = outcome.failure;
      if (settled || failure === null) {
        return null;
      }
      record({
        fact: "paint_failed",
        failure,
        colors: outcome.colors,
      });
      return paintFailureNotice(failure, outcome.colors);
    },
    scanned: (): Promise<void> => {
      if (settled) {
        return Promise.resolve();
      }
      if (inFlight !== null) {
        // **飛行中の 1 本にまとめる**（新しい標本を始めない）。
        pending = true;
        return inFlight;
      }
      return start();
    },
    dispose: (): void => {
      settled = true;
    },
  };
}

/**
 * 記録のコマンドの名前（`crates/app-shell/src/ipc/command_names.rs` の
 * [`DIAGNOSTICS_RECORD_RENDER`] と同一）。
 *
 * **文字列を直接 `invoke` へ渡さない。**型注釈（`CommandName`）は生成物の `COMMAND_NAMES` から
 * 導かれた合併型であるため、名前が源から消えると**この行で型検査が落ちる**。
 */
const DIAGNOSTICS_RECORD_RENDER: CommandName = "diagnostics_record_render";

/**
 * 器の診断の記録へ 1 件渡す（**境界の `diagnostics_record_render` を 1 回呼ぶ口**）。
 *
 * これは [`RenderHealthSink`] の**既定の実装**であり、画面はこれを注入する（検査は偽の口を
 * 渡す）。境界の型（`RenderHealthRecordRequest`）は[札](#RenderHealthReport)と数値だけを持つ
 * ので、**任意の文字列が記録へ流れる経路は無い**。
 *
 * **投げない。**記録できないこと（経路がまだ立ち上がっていない等）は描画の失敗ではないので、
 * 開発者向けの 1 行に留める（`renderHeartbeat.ts` の通知と同じ規律である）。**画面の
 * 提示（12.2 の告知）はこの往復の結果に依存しない** — 記録が失敗しても、利用者には
 * 描画が成立しなかったことが見える。
 */
export function recordGridRenderHealth(report: RenderHealthReport): void {
  // 綴りが変わるのはここだけである（画面の型は camelCase、境界は snake_case）。
  const request: RenderHealthRecordRequest = {
    report:
      report.fact === "paint_failed"
        ? {
            fact: "paint_failed",
            failure: report.failure,
            colors: report.colors,
          }
        : {
            fact: "scan_below_budget",
            median_us: report.medianUs,
            budget_us: report.budgetUs,
          },
  };
  void (async () => {
    try {
      const result = await invokeCommand<unknown>(DIAGNOSTICS_RECORD_RENDER, {
        request,
      });
      if (result.status === "error") {
        console.warn(
          `描画の健全性の記録を受け取ってもらえなかった: ${result.error.kind}`,
        );
      }
    } catch (error: unknown) {
      console.warn("描画の健全性の記録を送れなかった", error);
    }
  })();
}

/** [`installGridRenderHealth`] が画面から受け取る口。 */
export interface GridRenderHealthPorts {
  /**
   * **表の描画が成立しなかったことを画面へ上げる口**（要件 12.2。`GridSurface.onPaintFailed`）。
   * 告知 1 行へ出る。
   */
  readonly onPaintFailed: (notice: string) => void;
  /**
   * **画面自身の可視区間の知らせ**（`GridSurface` が窓の先読みと記憶へ渡す処理）。
   * **結線がこれを飲み込まない** — 走査の標本はこの処理のあとに始める。
   */
  readonly onVisibleSpanChange: (span: VisibleSpan) => void;
  /** 記録の口。省略すると器の診断へ記録する（[`recordGridRenderHealth`]）。 */
  readonly record?: RenderHealthSink;
  /** 標本の取り方。省略すると 7.6 の [`sampleFrameTimes`]。**検査が差し替える。** */
  readonly sample?: FrameSampler;
  /** 標本を取る長さ（ミリ秒）。省略すると [`SCAN_SAMPLE_MS`]。 */
  readonly durationMs?: number;
  /** 次のフレームまで待つ口。省略すると `requestAnimationFrame`。**検査が差し替える。** */
  readonly nextFrame?: () => Promise<void>;
  /** 経過を測る時計。省略すると `performance.now`。**検査が差し替える。** */
  readonly now?: () => number;
  /** 予算（ミリ秒）。省略すると [`FRAME_BUDGET_MS`]。 */
  readonly budgetMs?: number;
}

/**
 * 画面が移植口と組み立ての効果へ渡す結線（[`installGridRenderHealth`] の結果）。
 *
 * **2 つの戻りは移植口の 2 つの位置へそのまま渡る**（`RendererSpec.onVisibleSpanChange` と、
 * 表を組み立てた後の 1 回）。**約束を返すのは、検査が「告知へ渡るまで」と「標本が記録される
 * まで」を待てるようにするためである** — 移植口は 2 つとも戻りを無視する（`void` を期待する
 * 位置に、値を返す関数を渡してよい）。
 */
export interface GridRenderHealthConnection {
  /** `RendererSpec.onVisibleSpanChange` へそのまま渡す（走査のたびに引かれる）。 */
  readonly onVisibleSpanChange: (span: VisibleSpan) => Promise<void>;
  /** **表を組み立てた後に 1 回呼ぶ。**成立しなければ告知の口へ文言が渡り、記録が 1 件残る。 */
  readonly checkPaint: (container: PaintProbeContainer | null) => Promise<void>;
  /** 後始末（[`GridRenderHealth.dispose`]）。 */
  readonly dispose: () => void;
}

/**
 * 描画の健全性を**画面の組み立ての効果の形**へ結線する（要件 12.2、12.3。tasks.md 9.3）。
 *
 * # なぜ切り出すか（**効果は走らせないと観測できない**）
 *
 * 正体は `GridSurface` の組み立ての効果である。効果は React の外から駆動できず、本 module は
 * `node` 環境の検査から組み立てられる（React を読み込まない）ので、**判断と結線の全部をこの
 * 関数へ寄せてある** — 効果の中に残るのは「作る・2 つの位置へ渡す・捨てる」だけである。
 * `createGridCopyEntry` / `createGridPasteEntry`（8.7）と `./documentRequests`（10.7）が同じ形で
 * 切り出されている。**この形が無いと、2 つの結線（可視区間の知らせと組み立て後の検査）を
 * 丸ごと削る変異がどの検査にも掛からない**（9.3 のレビューが実測した）。
 *
 * # 結線の中身
 *
 * 1. **可視区間の知らせは、画面の処理を通してから走査として標本を取る**（要件 12.3。標本の
 *    開始が画面の先読みを遅らせないよう、順序は画面の処理が先である）
 * 2. **組み立ての後の検査は、成立しなかったときだけ告知の口へ文言を渡す**（要件 12.2。
 *    記録そのものは [`createGridRenderHealth`] が行う）
 *
 * **移植口は戻りを待たない**（`onVisibleSpanChange` は同期の知らせである。約束を返すのは検査の
 * ためである）。
 */
export function installGridRenderHealth(
  ports: GridRenderHealthPorts,
): GridRenderHealthConnection {
  const health = createGridRenderHealth({
    ...(ports.record === undefined ? {} : { record: ports.record }),
    ...(ports.sample === undefined ? {} : { sample: ports.sample }),
    ...(ports.durationMs === undefined ? {} : { durationMs: ports.durationMs }),
    ...(ports.nextFrame === undefined ? {} : { nextFrame: ports.nextFrame }),
    ...(ports.now === undefined ? {} : { now: ports.now }),
    ...(ports.budgetMs === undefined ? {} : { budgetMs: ports.budgetMs }),
  });

  return {
    onVisibleSpanChange: (span: VisibleSpan): Promise<void> => {
      ports.onVisibleSpanChange(span);
      // **走査である**（要件 12.3）。フレーム時間の標本を取り、閾値を跨いだら 1 回だけ記録する
      // （跨いだかどうかの判定と重複の防止は [`createGridRenderHealth`] が持つ。ここは走査の
      // たびに引くだけでよい — 飛行中の標本は 1 本にまとめられる）。
      return health.scanned();
    },
    checkPaint: (container: PaintProbeContainer | null): Promise<void> =>
      health.checkPaint(container).then((notice) => {
        if (notice !== null) {
          ports.onPaintFailed(notice);
        }
      }),
    dispose: (): void => {
      health.dispose();
    },
  };
}


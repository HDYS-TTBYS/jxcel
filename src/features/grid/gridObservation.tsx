/**
 * 検証専用: **実用のグリッド画面（`./GridScreen`）を実際に起動して観測する画面**（tasks.md 9.2、
 * 要件 11.1 / 11.2 / 11.3 / 12.1 / 12.2 / 12.3 / 12.4）。
 *
 * 所有: 検証専用の初期画面の経路（`src/shell/verificationScreen.ts` と
 * `src/shell/Layout.tsx` のレジストリの `__JXCEL_VERIFICATION__` の分岐）。
 *
 * # この画面が要る理由
 *
 * 9.2 の受入は「**10 万行のシートを開き、末尾へ移動し、セルを編集し、取り消して戻すまでを
 * 実際に起動して観測する**」ことである。**単体テストはここを代替できない** — 効果が走らず、
 * 実物の WebKitGTK の面が塗られるかも分からない（9.3 が同じ理由を記録している）。したがって
 * **製品の画面（`GridScreen`）そのもの**を検証用の初期画面として起動し、外から観測できる信号
 * （DOM の属性と時計）だけを読む。**自前の表を組まない**（組むと製品と別のものを測る）。
 *
 * # 出荷物に到達経路を作らない（1.6 / 7.2 と同じ二重の括り）
 *
 * 1. `src/shell/Layout.tsx` のレジストリへの登録を Vite の `define`
 *    （`vite.config.ts` の `__JXCEL_VERIFICATION__`）で括る。既定のビルド（`false`）では登録が
 *    定数畳み込みで消え、**本モジュールは参照されなくなる**。
 * 2. Rust 側の引き金（`JXCEL_VERIFICATION_GRID_OBSERVATION`、
 *    `src-tauri/src/window/mod.rs`）は `verification-triggers` feature の下にのみあり、
 *    **既定のビルドには環境変数の読み取りが入らない**。
 *
 * `scripts/check-shipping-bundle.sh` が配布物の `dist/` を機械検査する。
 *
 * # 観測の行（AT-SPI から読む唯一の証拠）
 *
 * 15 秒後（または全項目が終わった時点）に 1 行を `aria-label` へ書く:
 *
 * ```text
 * [検証] グリッドの観測: 最初の画面ms=<n> 走査中央値ms=<n|測定不能> 到達行=<n> 行数=<n>
 *   編集反映ms=<n|未観測> 取消=ok|ng:<理由> 描画=<成立|不成立> 項目=<key=value,...>
 * ```
 *
 * **観測できなかった項目は理由を書く**（無言で飛ばさない）。行は**部分でも書く** — 走査が
 * 途中で失敗しても、それまでの実測が読めるようにする（9.2 の段は失敗の内容を記録から読む）。
 *
 * # 描画不成立の条件（要件 12.2 の陽性の観測）
 *
 * `JXCEL_VERIFICATION_GRID_PAINT_FAILURE=1` のとき、**面の canvas を検査の窓の間だけ空にする**
 * （`canvas.width = canvas.width` は canvas を消す）— これは WebKitGTK の「DOM はあるが何も
 * 塗られない」症状（tauri-apps/tauri#15936）の**症状そのもの**であり、製品の描画成立の検査
 * （7.6）と告知・記録（9.3）の経路が不成立を観測する。**製品のコードは 1 行も変えない。**
 */
import { useEffect, useState } from "react";
import type { ReactElement } from "react";

import { GridScreen } from "./GridScreen";
import { createGridClient } from "./gridClient";
import { invokeCommand, type CommandName } from "../../ipc/client";
import type {
  ObservationItem,
  ObservationItemOutcome,
  ObservationItemReason,
  RenderHealthRecordRequest,
} from "../../ipc/bindings";
import { documentDiscard, documentNew } from "../../ipc/documentSession";
import { countDistinctColors, sampleFrameTimes, toRenderProbeResult } from "./renderProbe";
import { FRAME_BUDGET_MS } from "./renderHealth";

/**
 * 記録のコマンドの名前（`renderHealth.ts` の同名の定数と同じ綴りである）。
 *
 * **文字列を直接 `invoke` へ渡さない。**型注釈（`CommandName`）は生成物の `COMMAND_NAMES` から
 * 導かれた合併型であるため、名前が源から消えるとこの行で型検査が落ちる。
 */
const DIAGNOSTICS_RECORD_RENDER: CommandName = "diagnostics_record_render";

/**
 * 観測の実測を**診断の記録へ 1 行**残す（tasks.md 9.2 の「3 OS で観測が成功すること」）。
 *
 * # なぜ記録なのか（AT-SPI では 3 OS を閉じられない）
 *
 * 検証用の観測画面は結果を `aria-label` にも書くが、**それを読めるのは Linux だけである** —
 * AT-SPI は D-Bus 上の仕組みであり、macOS の WKWebView と Windows の WebView2 は別の
 * アクセシビリティ API を使う（しかも CI のランナーにはその権限が無い）。**3 OS の検査器が
 * 同じ形で読める唯一の場所が診断の記録である**（5.2 / 5.3 の段と同じ理由。
 * `.kiro/steering/verification.md`「ログと記録を一次証拠にする」）。
 *
 * # 運ぶのは数値と閉じた札だけである
 *
 * 境界の型（`RenderHealthRecordRequest`）は**任意の文字列を運べない**（記録の注入面を広げない
 * 規律。`renderHealth.ts` の doc）。記録の 1 行を組み立てるのは器の側である。
 *
 * **投げない。**記録できないことは観測の失敗ではない（開発者向けの 1 行に留める）。
 */
function recordObservation(observation: Observation): void {
  const request: RenderHealthRecordRequest = {
    report: {
      fact: "observation",
      first_screen_ms:
        observation.firstScreenMs === null ? null : Math.round(observation.firstScreenMs),
      scan_median_us:
        observation.traversal.medianMs === null
          ? null
          : Math.round(observation.traversal.medianMs * 1000),
      reached_row: observation.traversal.reachedRow,
      row_count: observation.traversal.rowCount,
      edit_ms: observation.edit.appliedMs === null ? null : Math.round(observation.edit.appliedMs),
      undo: observation.edit.undone === null
        ? "not_observed"
        : observation.edit.undone.startsWith("ok")
          ? "ok"
          : "ng",
      paint_failed: observation.paintFailed,
      colors: observation.surfaceColors,
      // **国勢調査は観測が面を読んだその瞬間のものである**（`surfaceCensus` の doc。ここで
      // 読み直すと、観測が終わって表が消えた後を写す — 実測でそうなった）。
      surface: observation.surface,
      items: observation.items.map((result) => ({
        item: result.item,
        outcome: result.outcome,
        reason: result.reason ?? null,
      })),
    },
  };
  void (async () => {
    try {
      const result = await invokeCommand<unknown>(DIAGNOSTICS_RECORD_RENDER, { request });
      if (result.status === "error") {
        console.warn(
          `グリッドの観測の記録を受け取ってもらえなかった: ${result.error.kind}`,
        );
      }
    } catch (error: unknown) {
      console.warn("グリッドの観測の記録を送れなかった", error);
    }
  })();
}

/**
 * **項目が待ちに入ったことを記録へ出す**（`RenderHealthReport::ItemWaiting`。tasks.md 9.2）。
 *
 * 段（検査器）は記録の追記を読んで「今が活性化の機会である」と知る。**投げない**（記録できない
 * ことは観測の失敗ではない）。
 */
export function recordItemWaiting(item: ObservationItem): void {
  const request: RenderHealthRecordRequest = { report: { fact: "item_waiting", item } };
  void (async () => {
    try {
      await invokeCommand<unknown>(DIAGNOSTICS_RECORD_RENDER, { request });
    } catch (error: unknown) {
      console.warn("観測の項目の待ちを記録へ送れなかった", error);
    }
  })();
}

/** 検証専用の初期画面の識別子（`JXCEL_VERIFICATION_INITIAL_SCREEN=grid-observation`）。 */
export const GRID_OBSERVATION_SCREEN_ID = "grid-observation";

/** 観測の行を書くまでの上限（この時間で打ち切って、それまでの実測を書く）。 */
const OBSERVATION_DEADLINE_MS = 260_000;

/** 表が組み立てられるのを待つ上限（要件 11.2 の 1 秒より十分に長く取る）。 */
const TABLE_WAIT_MS = 20_000;

/** 走査の標本を取る時間（`renderProbe.sampleFrameTimes` へ渡す）。 */
const TRAVERSAL_SAMPLE_MS = 1_000;


/**
 * 走査の 1 歩の行数。**1.6 の実測と同じ歩幅である**（10 万行 ÷ 300 行 ≈ 333 フレーム）。
 *
 * これは**連続した走査**の近似であり、1 歩ごとに窓の記憶が 1〜2 個の窓を取り直す（窓は
 * 256 行。跳躍を大きくすると 1 フレームで十数の窓を取り直すことになり、1.6 の実測とも
 * 比べられない）。
 */
const TRAVERSAL_ROWS_PER_STEP = 300;

/**
 * 1 フレームの待ち（`requestAnimationFrame` の 1 周）。
 *
 * **`Promise.withResolvers` は使えない** — `tsconfig.json` の `lib` が ES2024 を含まないため、
 * 型検査が `TS2550` で落ちる（実行環境の Chromium には在るが、プロジェクトの目標水準に合わせる）。
 */
function nextFrame(): Promise<void> {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      resolve();
    });
  });
}

/** 条件が満たされるまで 1 フレームずつ待つ（見つからなければ `null`）。 */
async function waitFor<T>(
  lookup: () => T | null,
  deadlineMs: number,
): Promise<T | null> {
  const deadline = performance.now() + deadlineMs;
  for (;;) {
    const found = lookup();
    if (found !== null) {
      return found;
    }
    if (performance.now() >= deadline) {
      return null;
    }
    await nextFrame();
  }
}

/** 文書の保持を待った結果（**保持された**か、**読み込めなかった**か、待ち切ったか）。 */
type HeldOutcome =
  | { readonly kind: "held" }
  | { readonly kind: "unavailable"; readonly reason: string }
  | { readonly kind: "timeout" };

/**
 * 文書が**保持される**（表示を指示できる）まで待つ。**読み込めなかったならその場で止める。**
 *
 * 画面は `document_state` を同じ境界から読むので、ここも同じ口を読んで**その瞬間を観測する**。
 * **読み込めなかった状態（`Unavailable`）は終端である** — 待ち続けても変わらない（9.2 の観測で、
 * 読み込みに失敗した文書を 15 秒待ち続け、期限の行だけを残して実測を失った）。呼び出し元が
 * そのまま理由を報告できるように、理由を返す。
 *
 * 間隔は**フレームより粗く取る**（`POLL_INTERVAL_MS`）— 画面の状態を観測するための問い合わせで
 * あり、毎フレーム往復させると**アプリ自身の往復と競合する**（実測: 1 秒間に十数件の
 * `document_state` が記録に出た）。
 */
async function waitForHeld(deadlineMs: number): Promise<HeldOutcome> {
  const client = createGridClient();
  const deadline = performance.now() + deadlineMs;
  for (;;) {
    const answer = await client.readDocumentState();
    if (answer.status === "ok") {
      if (answer.data.status.state === "Open") {
        return { kind: "held" };
      }
      if (answer.data.status.state === "Unavailable") {
        return { kind: "unavailable", reason: answer.data.status.reason };
      }
    }
    if (performance.now() >= deadline) {
      return { kind: "timeout" };
    }
    await new Promise<void>((resolve) => {
      window.setTimeout(resolve, POLL_INTERVAL_MS);
    });
  }
}

/** 文書の状態を問い合わせる間隔（ミリ秒）。**毎フレームは粗すぎる**（上の doc）。 */
const POLL_INTERVAL_MS = 100;

/** 検証専用のグローバルの綴り（`src-tauri/src/window/mod.rs` の対の契約）。 */
const PAINT_FAILURE_GLOBAL = "__JXCEL_VERIFICATION_GRID_PAINT_FAILURE__" as const;

/** 貼り付けの往復の筋書き（10.8）を要求するグローバルの綴り（同じ対の契約）。 */
const PASTE_GLOBAL = "__JXCEL_VERIFICATION_GRID_PASTE__" as const;

declare global {
  interface Window {
    readonly __JXCEL_VERIFICATION_GRID_PAINT_FAILURE__?: boolean;
    readonly __JXCEL_VERIFICATION_GRID_PASTE__?: boolean;
  }
}

/**
 * 貼り付けの往復を筋書きに含めるか（既定は偽）。
 *
 * **活性化する段だけが要求する。**`data-grid.paste` にはアクセラレータが無く、活性化できるのは
 * ネイティブのメニューを操作できる段（Linux の AT-SPI）だけである。要求されていない起動で
 * 待ちに入ると、macOS / Windows の段が必ず失敗する。
 */
function pasteRequested(): boolean {
  return __JXCEL_VERIFICATION__ && window[PASTE_GLOBAL] === true;
}

/** 塗られない条件が要求されているか（既定のビルドでは常に偽）。 */
function paintFailureRequested(): boolean {
  return (
    __JXCEL_VERIFICATION__ && window[PAINT_FAILURE_GLOBAL] === true
  );
}

/**
 * 描画不成立の条件を作る（検査の窓の間だけ canvas を空にする）。
 *
 * **面が現れてから**始め、`probeGridSurface` の待ちの上限（`PAINT_PROBE_MS`）より長く続ける。
 * 検査が canvas を見つけた時点で空であれば `blank` として観測される（色数 1）。
 */
function blankTheCanvasFor(ms: number): void {
  const deadline = performance.now() + ms;
  const clear = (): void => {
    const canvas = document.querySelector("canvas");
    if (canvas !== null) {
      // **空の canvas と入れ替える**（新しい面は何も塗られていない）。**描き手は元の面を
      // 持ち続けるので、そこへ描いても画面には出ない** — WebKitGTK の「DOM はあるが何も
      // 塗られない」症状（tauri-apps/tauri#15936）と同じ状態を、製品のコードを変えずに作る。
      // `width` の代入（自己代入になる）は使わない — `no-self-assign` に触れるうえ、
      // 描き手が再描画すれば内容が戻る。
      const blank = canvas.cloneNode(false);
      if (blank instanceof HTMLCanvasElement) {
        blank.width = canvas.width;
        blank.height = canvas.height;
        canvas.replaceWith(blank);
      }
    }
    if (performance.now() < deadline) {
      requestAnimationFrame(clear);
    }
  };
  requestAnimationFrame(clear);
}

/**
 * 表の面の色数を読む（**製品と同じ数え方**。`./renderProbe`）。**面が無ければ `null`。**
 *
 * 9.2 の初回の実測で、健全な起動でも検査（9.3）が色数 0 を読み、**誤って「塗って読み戻せない」と
 * 記録していた**（検査は面の組み立ての直後に走るため、移植口が面を用意する前に読んでいた）。
 * 本観測は**表が現れた後の色数**を実測として残し、その誤検知を検査器から見えるようにする。
 */
/**
 * **面（canvas）の国勢調査**（`ObservationSurface`。契約の型の doc を参照）。
 *
 * **要件の合否には使わない。**12.2 の判定は「面に内容が描かれているか」であり、その実測は
 * `colors` である。本関数が返すのは、その数値が**なぜその値になったか**を 3 OS の段で
 * 切り分けるための材料である — CI の実測で macOS と Windows のランナーが `色数=0` を記録し、
 * Linux が 2 以上を記録したとき、**面が小さいのか・塗られていないのか・読む面を間違えている
 * のか**を切り分ける材料が無く、CI の往復（1 回 30 分）が要った。
 *
 * **引数は読んだその瞬間の面である**（記録を書く時点で読み直してはならない。実測: 記録の直前に
 * 読むと、観測が終わって表が消えた後を写し、`面の数=0 器=0x0` になった — 何も分からない）。
 *
 * **画素比は 1000 倍の整数で運ぶ**（境界は 32 ビット以下の整数だけ。`window.devicePixelRatio` は
 * 1.25 / 2 のような小数を取り得る）。**この 1 箇所だけが `devicePixelRatio` を読む** — 製品の
 * 検査（`./renderProbe`）は素性を問わない規律を持ち、ここは**検証専用の観測**である。
 */
function surfaceCensus(
  table: HTMLElement | null,
  first: Element | null,
  colors: number | null,
): SurfaceCensus {
  const container: HTMLElement | null = table?.parentElement ?? table ?? null;
  const canvases: HTMLCanvasElement[] =
    table === null
      ? []
      : Array.from(table.querySelectorAll("canvas")).filter(
          (element): element is HTMLCanvasElement => element instanceof HTMLCanvasElement,
        );
  const firstCanvas = first instanceof HTMLCanvasElement ? first : canvases[0] ?? null;
  // 最大は本来塗られているべき面である（先頭と同じなら色数は既に読んだ値である）。
  let largest: HTMLCanvasElement | null = null;
  for (const canvas of canvases) {
    if (largest === null || canvas.width * canvas.height > largest.width * largest.height) {
      largest = canvas;
    }
  }
  return {
    container_width: Math.round(container?.clientWidth ?? 0),
    container_height: Math.round(container?.clientHeight ?? 0),
    pixel_ratio_milli: Math.round((window.devicePixelRatio ?? 0) * 1000),
    canvas_count: canvases.length,
    first_width: firstCanvas?.width ?? 0,
    first_height: firstCanvas?.height ?? 0,
    first_colors: firstCanvas === null ? null : colors,
    largest_width: largest?.width ?? 0,
    largest_height: largest?.height ?? 0,
    largest_colors:
      largest === null
        ? null
        : largest === firstCanvas
          ? colors
          : countDistinctColors(largest),
  };
}

/** 面を 1 回読み、**記録の 2 つの欄へそのまま写せる形**で返す。 */
function surfaceFields(): { surfaceColors: number | null; surface: SurfaceCensus } {
  const reading = surfaceNow();
  return { surfaceColors: reading.colors, surface: reading.census };
}

/** 面を 1 回読んだ結果（色数と、その瞬間の国勢調査）。 */
interface SurfaceReading {
  readonly colors: number | null;
  readonly census: SurfaceCensus;
}

async function waitForSurfaceColors(deadlineMs: number): Promise<SurfaceReading> {
  const deadline = performance.now() + deadlineMs;
  let last: SurfaceReading = surfaceNow();
  // **製品の検査（9.3）と同じく、面は非同期に現れる。**1 回読んで 0 だったことを「描かれていない」
  // と読むと、健全な起動を不成立と読む（実測: この待ちを入れないと 306 ms の時点で 0 であった）。
  // 2 色以上になった時点で確定し、ならなければ期限まで読み続けた最後の値を返す。
  while ((last.colors ?? 0) < 2 && performance.now() < deadline) {
    await nextFrame();
    last = surfaceNow();
  }
  return last;
}

function surfaceNow(): SurfaceReading {
  // **表の器の中を先に見る**（描画成立の検査が読むのと同じ面である。`./GridScreen` の
  // `health.checkPaint(container)`）。器の外の面は別の面であり得る。
  const table = tableOf();
  const root: ParentNode = table ?? document;
  const canvas = root.querySelector("canvas");
  const colors =
    canvas instanceof HTMLCanvasElement ? countDistinctColors(canvas) : null;
  return { colors, census: surfaceCensus(table, canvas, colors) };
}

/** 描画の不成立の告知（12.2）が画面に出ているか。**表の面の失敗ではなく、告知 1 行**である。 */
function paintNoticeShown(): boolean {
  const notice = document.querySelector('[data-testid="jxcel-grid-notice"]');
  return notice !== null && notice.textContent.includes("描画");
}

/** 走査の計測と、末尾への到達（要件 11.1 / 1.4）。 */
interface TraversalOutcome {
  readonly medianMs: number | null;
  readonly reachedRow: number | null;
  readonly rowCount: number | null;
  readonly reason: string | null;
}

/** `.dvn-scroller`（Glide の仮想スクロールの要素）を引く。 */
function scrollerOf(): HTMLElement | null {
  return document.querySelector<HTMLElement>(".dvn-scroller");
}

/** 表の外枠（canvas を含む）を引く。 */
function tableOf(): HTMLElement | null {
  return document.querySelector<HTMLElement>('[data-testid="jxcel-grid-table"]');
}

/** 行数を DOM から読む（画面が名乗る `data-row-count`。**数え直さない**）。 */
function rowCountOf(): number | null {
  const element = document.querySelector<HTMLElement>("[data-row-count]");
  const raw = element?.dataset["rowCount"];
  const parsed = raw === undefined ? Number.NaN : Number.parseInt(raw, 10);
  return Number.isSafeInteger(parsed) ? parsed : null;
}

/**
 * 末尾まで走査し、フレーム時間の中央値と到達した行を測る（要件 11.1）。
 *
 * **スクロールの位置を直接動かす**（打鍵やポインタの合成に頼らない）— 面は Glide であり、
 * その仮想スクロールは `scrollTop` の変化に反応して行を描く。標本は走査の**最中**に取る。
 */
async function traverse(): Promise<TraversalOutcome> {
  const scroller = scrollerOf();
  if (scroller === null) {
    return { medianMs: null, reachedRow: null, rowCount: rowCountOf(), reason: "面のスクロール要素が無い" };
  }
  const rowCount = rowCountOf();
  const before = scroller.scrollTop;
  const top = scroller.scrollHeight - scroller.clientHeight;
  // 1 歩 = `TRAVERSAL_ROWS_PER_STEP` 行。**画素の量は行の高さから出す**（面の見た目の
  // 高さに依らせない）: `平均の行の高さ = 内容の高さ ÷ 行数`。
  const rows = rowCountOf();
  const rowsPerPixel = scroller.scrollHeight / (rows ?? 1);
  const stepPixels = Math.max(1, Math.round(TRAVERSAL_ROWS_PER_STEP * rowsPerPixel));
  const steps = Math.max(1, Math.ceil(top / stepPixels));
  const sampling = sampleFrameTimes(TRAVERSAL_SAMPLE_MS * Math.min(steps, 4));
  for (let step = 1; step <= steps; step += 1) {
    scroller.scrollTop = Math.min(top, step * stepPixels);
    await nextFrame();
  }
  const samples = await sampling;
  const probe = toRenderProbeResult(true, samples);
  const atEnd = scroller.scrollTop >= top - 1;
  if (!atEnd && top > 0) {
    return { medianMs: null, reachedRow: null, rowCount, reason: `末尾へ届かなかった（${String(before)} → ${String(scroller.scrollTop)} / 上限 ${String(top)}）` };
  }
  // 到達した行は**陽に数える**: 末尾の位置で見えている最後の行は `行数 - 1` である。
  const reachedRow = rowCount === null ? null : rowCount - 1;
  return {
    medianMs: probe.medianFrameMs,
    reachedRow,
    rowCount,
    reason: probe.medianFrameMs === null ? "フレームの標本が 1 本も取れなかった" : null,
  };
}

/** セルの編集と取り消し（要件 11.3 / 9.2）。 */
interface EditOutcome {
  readonly appliedMs: number | null;
  readonly undone: string | null;
}

/** 面（canvas）の中でセルを 1 つ選んで編集を起動する（ダブルクリック = Glide の起動の仕方）。 */
async function openEditor(): Promise<HTMLInputElement | null> {
  const table = tableOf();
  if (table === null) {
    return null;
  }
  const canvas = table.querySelector("canvas");
  if (canvas === null) {
    return null;
  }
  const rect = canvas.getBoundingClientRect();
  const point = { x: rect.left + 200, y: rect.top + 60 };
  const panelOf = (): HTMLElement | null =>
    document.querySelector<HTMLElement>('[data-testid="jxcel-grid-editor"]');
  // 1 度目の当たり方を試し、開かなければ別の当たり方を試す（**面が開くまで諦めない** —
  // 開かなければ要件 11.3 を判定できず、観測の行にその理由が残る）。
  const attempts: (() => void)[] = [
    () => {
      const options = { bubbles: true, detail: 2, button: 0, clientX: point.x, clientY: point.y };
      canvas.dispatchEvent(new MouseEvent("mousedown", { ...options, detail: 1 }));
      canvas.dispatchEvent(new MouseEvent("mouseup", { ...options, detail: 1 }));
      canvas.dispatchEvent(new MouseEvent("click", { ...options, detail: 1 }));
      canvas.dispatchEvent(new MouseEvent("mousedown", options));
      canvas.dispatchEvent(new MouseEvent("mouseup", options));
      canvas.dispatchEvent(new MouseEvent("click", options));
      canvas.dispatchEvent(new MouseEvent("dblclick", options));
    },
    () => {
      canvas.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    },
    () => {
      canvas.dispatchEvent(new KeyboardEvent("keydown", { key: "F2", bubbles: true }));
    },
  ];
  for (const attempt of attempts) {
    attempt();
    const panel = await waitFor(panelOf, 2_000);
    if (panel !== null) {
      return panel.querySelector<HTMLInputElement>("input[type=text]") ?? null;
    }
  }
  return null;
}

/**
 * セルを編集し、確定が表示へ反映されるまでを測り、取り消す（要件 11.3 / 9.2）。
 *
 * **反映の信号**は「編集の面が閉じ、失敗の告知が出ていないこと」である — 面が閉じるのは
 * 適用が成功して状態が確定した後である（`./cellEdit` の遷移。失敗のときは開いたままにする）。
 */
async function editAndUndo(): Promise<EditOutcome> {
  const input = await openEditor();
  if (input === null) {
    return { appliedMs: null, undone: "ng:編集の面が開かなかった" };
  }
  // 打つ文字は**面の型に依らず適合しうる**ものを選ぶ: 今の値をそのまま置き直しても適用の
  // 経路は通るが「編集」にはならないので、1 文字足す（違反になっても保持される — 要件 3.5）。
  input.value = `${input.defaultValue}x`;
  const started = performance.now();
  // 面は Enter で確定する（`editors/text.tsx` の `handleInputKeys`）。
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  const closed = await waitFor(
    () => (document.querySelector('[data-testid="jxcel-grid-editor"]') === null ? true : null),
    5_000,
  );
  const appliedMs = closed === null ? null : performance.now() - started;
  const undo = document.querySelector<HTMLElement>('[data-testid="jxcel-grid-undo"]');
  if (undo === null) {
    return { appliedMs, undone: "ng:取り消しの入口が無い" };
  }
  undo.click();
  // 取り消しの後に**失敗の告知が出ていない**ことを確かめる（告知は内容の領域を置き換えない）。
  await nextFrame();
  const failure = document.querySelector('[data-testid="jxcel-grid-failure"]');
  return { appliedMs, undone: failure === null ? "ok" : "ng:取り消しのあとに失敗の告知が出た" };
}

/**
 * 筋書きの 1 項目の結果（tasks.md 9.2 の「群 10 が閉じた経路」）。
 *
 * **記録へ運ぶ値は生成物の閉じた型**（`ObservationItem` / `ObservationItemOutcome`）である —
 * 綴りを 2 つ持たない（`./../ipc/bindings`）。
 */
/** 筋書きの 1 項目の判定（型の結果・**止まった場所**・人が読む理由）。 */
interface ItemOutcome {
  readonly outcome: ObservationItemOutcome;
  readonly note: string;
  /** 成立しなかったときの止まった場所（成立したときは `undefined`）。 */
  readonly reason: ObservationItemReason | undefined;
}

/** 成立した（理由は要らない）。 */
const OK: ItemOutcome = { outcome: "ok", note: "", reason: undefined };

/**
 * 成立しなかった。**止まった場所（閉じた札）と、人が読む理由の両方**を持つ。
 *
 * 札は記録へ出る（3 OS の検査器が読む唯一の場所。CI の実測: Windows で「どの項目が ng か」
 * までしか分からず、原因の切り分けに往復を要した）。文言は人が読む行にだけ載る。
 */
function NG(reason: ObservationItemReason, note: string): ItemOutcome {
  return { outcome: "ng", note, reason };
}

interface ItemResult {
  readonly item: ObservationItem;
  readonly outcome: ObservationItemOutcome;
  /**
   * 成立しなかったときの**止まった場所**（閉じた札）。記録へ出る（3 OS の検査器が読む唯一の
   * 場所）。成立したときは `undefined` である。
   */
  readonly reason: ObservationItemReason | undefined;
  /**
   * 成立しなかった理由（**人が読む行にだけ載る**。記録は閉じた型だけを運ぶ — 任意の文字列を
   * 記録へ流さない規律）。**空文字は「理由が無い」ではなく「成立した」**である。
   */
  readonly note: string;
}

/** 筋書きの 1 項目が IPC の往復を待つ上限（ミリ秒）。 */
const ITEM_WAIT_MS = 10_000;

/**
 * **隣のセルへ移ったあと、違反の理由の面が更新されるまでの待ち**（ミリ秒）。
 *
 * 1 フレームでは足りない環境がある（CI の Linux ランナーでこの取りこぼしが実測された）。
 * 速い環境では最初のフレームで返るので、費用は増えない。
 */
const VIOLATION_STEP_WAIT_MS = 2_000;

/**
 * 貼り付けが届くのを待つ上限（ミリ秒）。
 *
 * **活性化するのは段である** — 段はアクセシビリティの木を 1 節ずつ `busctl` でたどって準備の印を
 * 探すので、10 万行の表を描いている最中は 1 巡に時間がかかる（実測: 30 秒では足りず、印が消えた
 * あとに段が探し続けた）。**この待ちの間、画面は静かである**（他の項目は走らない）。
 */
const PASTE_WAIT_MS = 150_000;

/**
 * **到着の増加が続くことを確かめる長さ**（ミリ秒）。
 *
 * 前の項目の描き直しの残りは過渡であり、`PASTE_WAIT_MS` の窓の中で消える。本当の貼り付けは
 * 表の内容と行数を変えるので、増加は残る。実測: この確かめを入れないと、健全な起動でも
 * 15 秒で「貼り付けが届いた」と結論していた（器は貼り付けの要求を 1 件も記録していない）。
 */
const PASTE_SUSTAIN_MS = 700;

/** 検証用の要素を 1 つ引く（`data-testid` の綴りは製品のものを使う）。 */
function elementOf(testid: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-testid="${testid}"]`);
}

/** 検証用の要素を押す（無ければ偽）。 */
function clickOf(testid: string): boolean {
  const element = elementOf(testid);
  if (element === null) {
    return false;
  }
  element.click();
  return true;
}

/** 現在位置（**0 起点の生の数**。画面が属性に出しているもの）。 */
function currentPositionOf(): { readonly row: number; readonly column: number } | null {
  const element = elementOf("jxcel-grid-selection-counts");
  const row = Number.parseInt(element?.dataset["currentRow"] ?? "", 10);
  const column = Number.parseInt(element?.dataset["currentColumn"] ?? "", 10);
  return Number.isSafeInteger(row) && Number.isSafeInteger(column) ? { row, column } : null;
}

/** 窓の到着の回数（**描き直しを起こした数**。画面が属性に出しているもの）。 */
function arrivalsOf(): number | null {
  const element = document.querySelector<HTMLElement>("[data-window-arrivals]");
  const value = Number.parseInt(element?.dataset["windowArrivals"] ?? "", 10);
  return Number.isSafeInteger(value) ? value : null;
}

/** 選択の行数（画面が属性に出しているもの）。 */
function selectionRowsOf(): number | null {
  const value = Number.parseInt(
    elementOf("jxcel-grid-selection-counts")?.dataset["selectionRows"] ?? "",
    10,
  );
  return Number.isSafeInteger(value) ? value : null;
}

/** 表へ打鍵を届ける（`onKeyDown` は表の器が受けている。既存の編集の起動と同じ経路）。 */
function pressOnTable(
  key: string,
  options: { readonly shift?: boolean; readonly count?: number } = {},
): void {
  const table = tableOf();
  if (table === null) {
    return;
  }
  const times = options.count ?? 1;
  for (let index = 0; index < times; index += 1) {
    table.dispatchEvent(
      new KeyboardEvent("keydown", { key, shiftKey: options.shift === true, bubbles: true }),
    );
  }
}

/** 行数が `expected` になるまで待つ（**画面が名乗る数**で待つ。数え直さない）。 */
async function waitForRowCount(expected: number, deadlineMs: number): Promise<boolean> {
  const reached = await waitFor(() => (rowCountOf() === expected ? true : null), deadlineMs);
  return reached !== null;
}

/** 違反の理由の面が名乗っている位置と文言（要件 4.2）。 */
function violationReasonOf(): {
  readonly row: number;
  readonly column: number;
  readonly text: string;
} | null {
  const element = elementOf("jxcel-grid-violation-reason");
  if (element === null) {
    return null;
  }
  const row = Number.parseInt(element.dataset["violationRow"] ?? "", 10);
  const column = Number.parseInt(element.dataset["violationColumn"] ?? "", 10);
  if (!Number.isSafeInteger(row) || !Number.isSafeInteger(column)) {
    return null;
  }
  return { row, column, text: element.textContent };
}

/**
 * 筋書き: **行を追加して取り消し・やり直しをし、現在位置が対象の行へ移る**（10.5。要件 9.9）。
 *
 * 「変更箇所が見える」は **現在位置が対象の行へ移ること**で観測する（画面は適用の応答が運ぶ
 * 影響の行へ現在位置を移し、そこへ追随する）。canvas の画素は変更箇所を名乗らないので、
 * DOM から読める信号だけで判定する。
 */
async function driveInsertRow(): Promise<ItemOutcome> {
  const before = rowCountOf();
  if (before === null) {
    return NG("row_count_unreadable", "行数を読めなかった");
  }
  if (!clickOf("jxcel-grid-insert-row")) {
    return NG("entry_missing", "行の追加の入口が無い");
  }
  if (!(await waitForRowCount(before + 1, ITEM_WAIT_MS))) {
    return NG("row_count_unchanged", "追加で行数が増えなかった");
  }
  const inserted = currentPositionOf();
  if (!clickOf("jxcel-grid-undo")) {
    return NG("entry_missing", "取り消しの入口が無い");
  }
  if (!(await waitForRowCount(before, ITEM_WAIT_MS))) {
    return NG("row_count_not_restored", "取り消しで行数が戻らなかった");
  }
  const afterUndo = currentPositionOf();
  if (!clickOf("jxcel-grid-redo")) {
    return NG("entry_missing", "やり直しの入口が無い");
  }
  if (!(await waitForRowCount(before + 1, ITEM_WAIT_MS))) {
    return NG("row_count_unchanged", "やり直しで行数が増えなかった");
  }
  // **後始末**: 追加を戻す（後続の項目が同じ前提で走れるように）。
  clickOf("jxcel-grid-undo");
  await waitForRowCount(before, ITEM_WAIT_MS);
  if (inserted === null || afterUndo === null) {
    return NG("position_unreadable", "現在位置を読めなかった");
  }
  return afterUndo.row === inserted.row
    ? OK
    : NG("position_not_moved", `現在位置が対象の行へ移らなかった（${String(inserted.row)} → ${String(afterUndo.row)}）`);
}

/**
 * 筋書き: **違反しているセルの理由を読み、同じ行の別の違反セルでも理由が読める**（10.6。要件 4.2）。
 *
 * 標本は 1000 行ごとに**すべての列へ**違反を仕込む（`crates/data-grid/tests/common/sample.rs` の
 * 列優先の等間隔）ので、同じ行の別の列へ移れば 2 つ目の理由が読める。
 */
async function driveViolationReason(): Promise<ItemOutcome> {
  if (!clickOf("jxcel-grid-next-violation")) {
    return NG("entry_missing", "違反の巡回の入口が無い");
  }
  const first = await waitFor(() => {
    const reason = violationReasonOf();
    return reason !== null && reason.text.includes("違反") ? reason : null;
  }, ITEM_WAIT_MS);
  if (first === null) {
    return NG("violation_reason_missing", "違反の理由が読めなかった");
  }
  for (let step = 0; step < 8; step += 1) {
    pressOnTable("ArrowRight");
    // **面の更新を待つ。**1 フレームでは足りない環境がある（実測: CI の Linux ランナーで
    // この項目だけが `violation_reason_not_repeated` で落ち、同じ検査器が次の実行では通った
    // ＝取りこぼしである）。待ちの上限は項目の上限に合わせる（速い環境では 1 フレームで返る）。
    const other = await waitFor(() => {
      const reason = violationReasonOf();
      if (
        reason !== null &&
        reason.row === first.row &&
        reason.column !== first.column &&
        reason.text.includes("違反")
      ) {
        return reason;
      }
      return null;
    }, VIOLATION_STEP_WAIT_MS);
    if (other !== null) {
      return OK;
    }
  }
  return NG("violation_reason_not_repeated", "同じ行の別の違反セルで理由が読めなかった");
}

/**
 * 筋書き: **参照の列の面が行を一覧する**（10.3。要件 3.8）。
 *
 * 参照の列は**名ではなく構造で見つける**（列の名は標本のものであり、入替えられうる）: 境界の
 * `grid_open_sheet` の応答が運ぶ列の一覧から `reference_sheet` を持つ列の**表示の序数**を取り、
 * 現在位置を打鍵でその列へ移してから編集の面を開く。**「続きを読む」は観測しない** — 標本の
 * 参照先は 64 行であり、頁（`REFERENCE_PAGE_SIZE` = 100）に満たないため続きが存在しない
 * （tasks.md の申し送り。続きを観測するには頁より多い参照先を持つ標本が要る）。
 */
async function driveReferenceRows(): Promise<ItemOutcome> {
  const state = await createGridClient().readDocumentState();
  if (state.status !== "ok") {
    return NG("document_state_unreadable", "文書の状態を読めなかった");
  }
  if (state.data.status.state !== "Open") {
    return NG("document_not_open", `文書が保持されていない（状態=${state.data.status.state}）`);
  }
  const sheet = state.data.status.sheets[0];
  if (sheet === undefined) {
    return NG("sheet_missing", "シートが無い");
  }
  const opened = await createGridClient().openSheet(sheet.id);
  if (opened.status !== "ok") {
    return NG("sheet_open_failed", `シートを開けなかった（${opened.error.detail.message}）`);
  }
  const column = opened.data.sheet.columns.findIndex((entry) => entry.reference_sheet !== null);
  if (column < 0) {
    return NG("reference_column_missing", "参照の列が無い");
  }
  // 現在位置をその列へ移す（画面は 1 行 1 列から始まらないので、差を打鍵で詰める）。
  for (let step = 0; step < 40; step += 1) {
    const position = currentPositionOf();
    if (position === null) {
      return NG("position_unreadable", "現在位置を読めなかった");
    }
    if (position.column === column) {
      break;
    }
    pressOnTable(position.column < column ? "ArrowRight" : "ArrowLeft");
    await nextFrame();
  }
  const position = currentPositionOf();
  if (position === null || position.column !== column) {
    return NG("position_not_moved", `参照の列へ移れなかった（${String(position?.column)} → ${String(column)}）`);
  }
  if (!pressEditorOpen()) {
    return NG("table_missing", "表が無い");
  }
  const listed = await waitFor(() => {
    const panel = elementOf("jxcel-grid-reference");
    if (panel === null) {
      return null;
    }
    const rows = Number.parseInt(panel.dataset["referenceRows"] ?? "", 10);
    return Number.isSafeInteger(rows) && rows > 0 ? true : null;
  }, ITEM_WAIT_MS);
  closeEditor();
  return listed === null ? NG("reference_not_listed", "参照の面が行を一覧しなかった") : OK;
}

/**
 * 筋書き: **入れ子の列を展開したあとに走査する**（10.1）。
 *
 * かつては展開で世代がずれ、**窓が永久に空**になっていた。観測するのは「展開のあとに窓が
 * 届き続け、面が塗られていること」である（空の窓なら面は一様になる）。
 */
async function driveNestedExpansion(): Promise<ItemOutcome> {
  const expand = document.querySelector<HTMLElement>('[data-testid^="jxcel-grid-expansion-"]');
  const scroller = scrollerOf();
  const before = arrivalsOf();
  if (expand === null) {
    return NG("entry_missing", "展開の入口が無い");
  }
  if (scroller === null || before === null) {
    return NG("surface_unreadable", "面か窓の到着の数を読めなかった");
  }
  expand.click();
  const arrived = await waitFor(() => {
    const now = arrivalsOf();
    return now !== null && now > before ? true : null;
  }, ITEM_WAIT_MS);
  if (arrived === null) {
    return NG("arrivals_unchanged", "展開のあとに窓が届かなかった");
  }
  // 展開のあとに走査する（窓の要求が空にならないこと。面が一様なら内容が無い）。
  const rowsPerPixel = scroller.scrollHeight / (rowCountOf() ?? 1);
  const stepPixels = Math.max(1, Math.round(TRAVERSAL_ROWS_PER_STEP * rowsPerPixel));
  const top = scroller.scrollHeight - scroller.clientHeight;
  for (let step = 1; step <= Math.min(10, Math.ceil(top / stepPixels)); step += 1) {
    scroller.scrollTop = Math.min(top, step * stepPixels);
    await nextFrame();
  }
  const colors = (await waitForSurfaceColors(ITEM_WAIT_MS)).colors;
  const after = arrivalsOf();
  if (colors === null || colors < 2) {
    return NG("surface_uniform", `展開のあとの面が一様である（色数=${String(colors)}）`);
  }
  return after !== null && after > before ? OK : NG("arrivals_unchanged", "展開のあとに窓の到着が増えなかった");
}

/**
 * 筋書き: **並べ替えた表示で位置を指定して行を追加し、範囲を選んで削除する**（10.4。6.1 / 6.2）。
 *
 * 並べ替えは表示の指定であり文書を変えないので、追加と削除だけを後始末する（取り消しで戻す）。
 */
async function driveSortThenDelete(): Promise<ItemOutcome> {
  const before = rowCountOf();
  const sort = document.querySelector<HTMLElement>('[data-testid="jxcel-grid-column-sort"]');
  if (before === null) {
    return NG("row_count_unreadable", "行数を読めなかった");
  }
  if (sort === null) {
    return NG("entry_missing", "並べ替えの入口が無い");
  }
  const directionBefore = sort.dataset["sortDirection"] ?? "";
  sort.click();
  const sorted = await waitFor(() => {
    const now = document.querySelector<HTMLElement>('[data-testid="jxcel-grid-column-sort"]');
    const direction = now?.dataset["sortDirection"] ?? "";
    return direction !== directionBefore ? true : null;
  }, ITEM_WAIT_MS);
  if (sorted === null) {
    return NG("sort_not_reflected", "並べ替えが表示へ反映されなかった");
  }
  // 位置を指定して追加する（現在位置を 2 行進めてから）。
  pressOnTable("ArrowDown", { count: 2 });
  await nextFrame();
  if (!clickOf("jxcel-grid-insert-row")) {
    return NG("entry_missing", "並べ替えた表示で行の追加の入口が無い");
  }
  if (!(await waitForRowCount(before + 1, ITEM_WAIT_MS))) {
    return NG("row_count_unchanged", "並べ替えた表示で行数が増えなかった");
  }
  // 範囲を選んで削除する（行全体の選択 = shift + 空白。範囲は shift + 矢印で広げる）。
  pressOnTable(" ", { shift: true });
  pressOnTable("ArrowDown", { shift: true });
  await nextFrame();
  const selected = selectionRowsOf();
  if (selected === null || selected < 1) {
    return NG("selection_empty", `範囲を選べなかった（選択=${String(selected)}）`);
  }
  if (!clickOf("jxcel-grid-delete-rows")) {
    return NG("entry_missing", "削除の入口が無い");
  }
  // **確認は「押した後」に現れる**（選んだ行数が見えている行数を超えるとき、または画面の高さが
  // 未知のときに出る。`./rowOps` の `deleteNeedsConfirmation`）。現れるのを待ってから押す —
  // 押した直後に読むと `null` であり、削除が 1 件も送られない（実測: これで落ちていた）。
  // **確認の面を待つ上限は項目の上限に合わせる**（遅い環境では 2 秒で現れず、確認を押さないまま
  // 削除が送られない — 実測: CI の Linux ランナーで `row_count_unchanged` になった）。
  const confirmation = await waitFor(
    () => elementOf("jxcel-grid-delete-confirm-yes"),
    ITEM_WAIT_MS,
  );
  confirmation?.click();
  const deleted = await waitForRowCount(before + 1 - selected, ITEM_WAIT_MS);
  // **後始末**: 削除と追加を戻す（取り消し 2 回）。
  clickOf("jxcel-grid-undo");
  await waitForRowCount(before + 1, ITEM_WAIT_MS);
  clickOf("jxcel-grid-undo");
  await waitForRowCount(before, ITEM_WAIT_MS);
  return deleted
    ? OK
    : NG(
        "row_count_unchanged",
        `削除で行数が減らなかった（選択=${String(selected)} 行 / 確認の面=${confirmation === null ? "現れなかった" : "押した"}）`,
      );
}

/**
 * 筋書き: **シートを切り替えても取り消しが効く**（10.2。要件 9.5）。
 *
 * **切り替えは境界から行う** — 本機能にシートを選ぶ UI は無い（`GridOpenRequest` の doc:
 * 選ぶ手段は本機能の外である）。取り消しは**画面のボタン**から行い、切替のあとに**前のシートへ
 * 加えた変更が戻る**ことを行数で見る（履歴が文書の単位であることの実地の証拠）。
 */
async function driveSheetSwitchUndo(): Promise<ItemOutcome> {
  const client = createGridClient();
  const state = await client.readDocumentState();
  if (state.status !== "ok" || state.data.status.state !== "Open") {
    return NG("document_state_unreadable", "文書の状態を読めなかった");
  }
  const first = state.data.status.sheets[0];
  const other = state.data.status.sheets[1];
  if (first === undefined || other === undefined) {
    return NG("sheet_missing", "切り替え先のシートが無い");
  }
  const before = rowCountOf();
  if (before === null) {
    return NG("row_count_unreadable", "行数を読めなかった");
  }
  if (!clickOf("jxcel-grid-insert-row")) {
    return NG("entry_missing", "行の追加の入口が無い");
  }
  if (!(await waitForRowCount(before + 1, ITEM_WAIT_MS))) {
    return NG("row_count_unchanged", "追加で行数が増えなかった");
  }
  // 別のシートへ切り替えてから、**元のシートへ戻る**（利用者の操作に相当する選択を、UI が
  // 無いので境界から行う）。**戻らずに取り消すと、応答は「触っていないシート」の行数を運ぶ**
  // （`src-tauri/src/commands/grid.rs` の `untouched_sheet_outcome`）ため、画面の行数は
  // 戻らない — 履歴が文書の単位であることは、**戻ってから取り消す**ことで観測できる
  // （Rust の既存の検査 `switching_sheets_keeps_the_history_of_the_document` と同じ順序）。
  const switched = await client.openSheet(other.id);
  if (switched.status !== "ok") {
    const described = state.data.status.sheets
      .map((entry) => `${entry.name}(列=${String(entry.columns)} 行=${String(entry.rows)})`)
      .join("/");
    return NG("sheet_open_failed", `別のシートを開けなかった（${switched.error.detail.message} / シート=${described}）`);
  }
  if ((await client.openSheet(first.id)).status !== "ok") {
    return NG("sheet_not_restored", "元のシートへ戻れなかった");
  }
  await nextFrame();
  // **戻った状態で**画面の取り消しを押す。履歴は文書の単位なので、切り替えをまたいでも戻る。
  if (!clickOf("jxcel-grid-undo")) {
    return NG("entry_missing", "取り消しの入口が無い");
  }
  const undone = await waitForRowCount(before, ITEM_WAIT_MS);
  return undone ? OK : NG("row_count_not_restored", "シートを切り替えたあとの取り消しで行数が戻らなかった");
}

/**
 * 筋書き: **文書を差し替えたとき、表が古い行を残さずに追随する**（10.7。要件 1.7）。
 *
 * 「新規」は窓の文書を**差し替える**（`document_new` は状態の変化を通知する）。100,000 行の
 * 表が**古い行を残さない**ことは、行数が新しい文書のものになり、面がそのまま塗られていることで
 * 見る。**この項目は最後に走る**（文書を置き換えるため、他の項目の前提を壊す）。
 */
async function driveReplaceDocument(): Promise<ItemOutcome> {
  const before = rowCountOf();
  if (before === null || before < 2) {
    return NG("row_count_unreadable", "差し替える前の行数を読めなかった");
  }
  // **未保存の変更があると「新規」は拒否される**（`DocumentNewOutcome::Refused`。封筒は成功の
  // まま返る）。筋書きの前の項目が文書へ加えた変更を**破棄してから**差し替える。
  const discarded = await documentDiscard();
  if (discarded.status !== "ok") {
    return NG("discard_failed", "未保存の変更を破棄できなかった");
  }
  const replaced = await documentNew();
  if (replaced.status !== "ok") {
    return NG("document_new_failed", "新規の文書を作れなかった");
  }
  if (replaced.data.outcome.outcome !== "Created") {
    return NG("document_new_failed", `新規の文書が拒否された（${replaced.data.outcome.reason}）`);
  }
  // **古い行を残さず追随したこと**を、**100,000 行の表そのものが消え、新しい文書の提示
  // （列が無い・行が無い）へ替わったこと**で見る。新しい文書は列 0 本・行 0 件であり、表の面
  // （canvas）も行数を名乗る要素も無くなる（`./GridScreen` の `no-schema` / `no-rows` の腕）。
  const followed = await waitFor(() => {
    const replacedByNewDocument =
      elementOf("jxcel-grid-schema-undefined") !== null ||
      elementOf("jxcel-grid-no-rows") !== null;
    return replacedByNewDocument && tableOf() === null ? true : null;
  }, ITEM_WAIT_MS);
  if (followed === null) {
    return NG("document_not_followed", "表が古い行を残したままである（新しい文書の提示へ替わらなかった）");
  }
  return OK;
}

/**
 * 筋書き: **貼り付けがシステムのクリップボードを経由して戻る**（10.8。要件 7.2）。
 *
 * **活性化は画面の外（ネイティブのメニュー）である** — `data-grid.paste` にはアクセラレータが
 * 無く、段がアクセシビリティの木から活性化する。したがって観測の画面ができるのは次の 3 つで
 * あり、往復そのものの判定は**検査器が記録から**行う（`グリッドの貼り付けの要求を送った` の行）:
 *
 * 1. 現在位置のセルを複製して**クリップボードへ書く**（画面の複製の経路。`Ctrl+C`）
 * 2. **準備ができたことをアクセシビリティの木へ名乗る**（表の器の `aria-label`）— 段はこれを
 *    見てからメニューの項目を活性化する
 * 3. 貼り付けが届いたことを**窓の到着の増加**で観測する（適用は表を描き直す）
 */
async function drivePasteThroughMenu(): Promise<ItemOutcome> {
  const table = tableOf();
  if (table === null) {
    return NG("table_missing", "表が無い");
  }
  // **静かになるまで待ってから基準を取る**（前の項目の描き直しが残っていると、貼り付けが
  // 届いていなくても到着の数が増え、偽の成立になる。実測: それが起きた）。
  let settled = arrivalsOf();
  for (let step = 0; step < 20; step += 1) {
    await nextFrame();
    const now = arrivalsOf();
    if (now !== null && now === settled) {
      break;
    }
    settled = now;
  }
  const before = arrivalsOf();
  // **複製は DOM の `copy` の経路である**（移植口が器の内側の面へ**捕獲の段**で結線している。
  // 器へ送ると子孫の聴取には届かない）— 人が `Ctrl+C` を打つと基盤が起こすイベントを、
  // 同じ形で起こす。`Ctrl+C` の keydown では**この経路は走らない**（実測）。
  // **面は組み直しの間だけ消えることがある**（表の表示を変える項目の直後）。1 回読んで
  // 無かったことを「面が無い」と読むと、健全な起動でも落ちる（実測: 手元の Linux で
  // この項目だけが落ちた）。面が現れるまで待つ。
  const canvas = await waitFor(
    () => table.querySelector("canvas"),
    ITEM_WAIT_MS,
  );
  if (canvas === null) {
    return NG("surface_unreadable", "表の面が無い");
  }
  canvas.dispatchEvent(new ClipboardEvent("copy", { bubbles: true, cancelable: true }));
  await nextFrame();
  // **印は記録へ出す**（「今が活性化の機会である」）。段が印を読む道は記録しか無い —
  // アクセシビリティの木を深く歩くと WebKit のアクセシビリティが答えなくなり（実測: 565 節の
  // 直後に 14 節へ落ち、以後 300 秒以上戻らなかった）、ウィンドウの題名も反映されない
  // （実測: `document.title` を変えても `frame` の名前は `jxcel` のまま）。
  recordItemWaiting("paste_through_menu");
  // **待ちは長く取る。**活性化するのは段であり、段はアクセシビリティの木を 1 節ずつ busctl で
  // たどって印を探す（10 万行の表を描いている最中は 1 巡に数十秒かかることがある。実測: 30 秒では
  // 足りず、印が消えたあとに段が探し続けて「見つからない」になった）。
  //
  // **増加が続くことを確かめてから結論する。**1 回の増加は貼り付けの証明ではない — 前の項目の
  // 描き直しの残りでも数は増える（実測: この項目が 15 秒で成立と結論した起動で、器は貼り付けの
  // 要求を 1 件も記録していなかった）。**過渡で結論すると印が早く消え、段が活性化の機会を失う**
  // — 印の寿命は「段が活性化するまで」でなければならない（段は活性化の直後に貼り付けが届くので、
  // 本当の貼り付けが届いた時点で印を外してよい）。
  const pasteDeadline = performance.now() + PASTE_WAIT_MS;
  let pasted: boolean | null = null;
  while (pasted === null && performance.now() < pasteDeadline) {
    await nextFrame();
    const now = arrivalsOf();
    if (now === null || before === null || now <= before) {
      continue;
    }
    await new Promise((resolve) => setTimeout(resolve, PASTE_SUSTAIN_MS));
    const again = arrivalsOf();
    pasted = again !== null && before !== null && again > before;
  }
  const arrived = arrivalsOf();
  if (pasted === null) {
    return NG("paste_not_delivered", `貼り付けが表へ届かなかった（到着 ${String(before)} → ${String(arrived)}）`);
  }
  return OK;
}

/**
 * 編集の面を開く（**面（canvas）への打鍵**）。
 *
 * **表の器へ送ってはならない** — 器の `onKeyDown` は選択の移動だけを扱い（`./GridScreen` の
 * `selectionForKey`）、編集の起動は移植口が**面（canvas）**に結線している（Glide の
 * `activateCell`。既定の打鍵は Enter）。器へ送った打鍵は子孫の面には届かない（実測: 参照の
 * 項目がこの理由で成立しなかった）。
 */
function pressEditorOpen(): boolean {
  const canvas = tableOf()?.querySelector("canvas") ?? null;
  if (canvas === null) {
    return false;
  }
  canvas.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  return true;
}

/** 編集の面を閉じる（**確定しない**。Escape は取り消しである）。 */
function closeEditor(): void {
  const panel = elementOf("jxcel-grid-editor");
  const input = panel?.querySelector<HTMLInputElement>("input[type=text]") ?? null;
  input?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
}

/** 観測の結果（行にする前の値）。 */
interface Observation {
  firstScreenMs: number | null;
  traversal: TraversalOutcome;
  edit: EditOutcome;
  paintFailed: boolean;
  /**
   * 表の面（canvas）から読めた色数（**製品と同じ数え方**。`./renderProbe` の
   * `countDistinctColors`）。`null` は面が無く、**0 は読めなかった**（2D の文脈が無い・
   * 大きさが足りない）である。**12.2 の成立側**（面に実際に内容が描かれていること）の実測。
   */
  surfaceColors: number | null;
  /**
   * **9.2 の筋書きの項目ごとの結果**（「群 10 が閉じた経路」）。**観測しなかった項目は載せない**
   * （段は自分が要求する項目を引数で名乗る。`--expect-items`）。
   */
  items: readonly ItemResult[];
  /**
   * 面の国勢調査（`surfaceCensus`）。**面を読んだその瞬間のものである** — 各腕が
   * `...surfaceFields()` / `waitForSurfaceColors` の結果から写す。
   */
  surface: SurfaceCensus;
  reason: string | null;
}

/** 面の国勢調査（境界の `ObservationSurface` と同じ形）。 */
interface SurfaceCensus {
  container_width: number;
  container_height: number;
  pixel_ratio_milli: number;
  canvas_count: number;
  first_width: number;
  first_height: number;
  first_colors: number | null;
  largest_width: number;
  largest_height: number;
  largest_colors: number | null;
}

/** 観測の結果を 1 行にする（**入力に対して純粋**。欠けた値は理由を書く）。 */
export function describeObservation(observation: Observation): string {
  const median =
    observation.traversal.medianMs === null
      ? `測定不能${observation.traversal.reason === null ? "" : `(${observation.traversal.reason})`}`
      : observation.traversal.medianMs.toFixed(2);
  const applied =
    observation.edit.appliedMs === null ? "未観測" : observation.edit.appliedMs.toFixed(1);
  return (
    `[検証] グリッドの観測: 最初の画面ms=${observation.firstScreenMs?.toFixed(1) ?? "未観測"}` +
    ` 走査中央値ms=${median} 予算ms=${FRAME_BUDGET_MS.toFixed(2)}` +
    ` 到達行=${observation.traversal.reachedRow ?? "未観測"}` +
    ` 行数=${observation.traversal.rowCount ?? "未観測"}` +
    ` 編集反映ms=${applied} 取消=${observation.edit.undone ?? "未観測"}` +
    ` 描画=${observation.paintFailed ? "不成立" : "成立"}` +
    ` 面の色数=${observation.surfaceColors ?? "面なし"}` +
    (observation.items.length === 0
      ? ""
      : ` 項目=${observation.items
          .map((entry) =>
            entry.note === "" ? `${entry.item}:${entry.outcome}` : `${entry.item}:${entry.outcome}(${entry.note})`,
          )
          .join(",")}`) +
    (observation.reason === null ? "" : ` 理由=${observation.reason}`)
  );
}

/**
 * 筋書きを順に走らせる（9.2 の「群 10 が閉じた経路」）。
 *
 * **1 項目が落ちても次を走らせる**（1 つの欠陥で残りの証拠を失わない）。例外も `ng` に写す —
 * 観測の行は「どの項目がどうだったか」を運ぶのが仕事である。
 *
 * **順序に意味がある**: 表示だけを変える項目（入れ子の展開・並べ替え）は後ろへ置き、
 * **文書を差し替える項目（`document_new`）は最後**に置く（他の項目の前提を壊すため）。
 * 貼り付けは**活性化を要求された起動でだけ**走らせる（ネイティブのメニューを操作できる段は
 * Linux だけである）。
 */
async function runScenario(): Promise<ItemResult[]> {
  const results: ItemResult[] = [];
  const run = async (
    item: ObservationItem,
    drive: () => Promise<ItemOutcome>,
  ): Promise<void> => {
    let result: ItemOutcome;
    try {
      result = await drive();
    } catch (error: unknown) {
      console.warn(`筋書きの項目が例外で止まった: ${item}`, error);
      result = NG("exception", `例外で止まった: ${String(error)}`);
    }
    results.push({
      item,
      outcome: result.outcome,
      note: result.note,
      reason: result.reason,
    });
  };
  await run("insert_row", driveInsertRow);
  await run("violation_reason", driveViolationReason);
  await run("reference_rows", driveReferenceRows);
  await run("nested_expansion", driveNestedExpansion);
  await run("sort_then_delete", driveSortThenDelete);
  await run("sheet_switch_undo", driveSheetSwitchUndo);
  // **貼り付けは文書を差し替える項目の直前に走らせる。**この項目だけは**外からの活性化を待つ**
  // ので、貼り付けが届く時刻を選べない（待っている間は他の項目が走らないので、**貼り付けが
  // 別の項目の最中に文書を変えることはない**）。後ろに残すのは**文書を差し替える項目だけ**に
  // する — あれは未保存の変更を破棄してから作り直すので、行が増えていても成立する
  // （実測: 貼り付けを中間に置くと `sheet_switch_undo` が行数の不一致で落ち、先頭に置くと
  // `violation_reason` が落ち、最後に置くと `replace_document` が先に表を消していた）。
  if (pasteRequested()) {
    await run("paste_through_menu", drivePasteThroughMenu);
  }
  await run("replace_document", driveReplaceDocument);
  return results;
}

/** 観測の本体（画面から 1 回だけ起動する）。 */
async function observe(): Promise<Observation> {
  let reason: string | null = null;
  const paintFailure = paintFailureRequested();
  if (paintFailure) {
    // **観測の間ずっと空にする**（途中で止めると、描き手が塗り直して「成立」に戻りうる）。
    blankTheCanvasFor(OBSERVATION_DEADLINE_MS);
  }
  // **要件 11.2 の時計は「表示が指示されたとき」から回す** — 文書の読み込み（20 MB の
  // コンテナ）は `document-session` の予算であり、本機能の予算ではない。**画面がドキュメントを
  // 持っていることを知った瞬間**（= 表示の指示を受けた瞬間）を 0 とする。画面自身は
  // `document_state` を開くときに 1 回読むので、ここでも同じ口を読んで**その瞬間を観測する**
  // （画面の内部状態を覗くのではなく、同じ境界の答えを待つ）。
  const held = await waitForHeld(TABLE_WAIT_MS);
  if (held.kind !== "held") {
    // **表示の指示が来ていない**（読み込めなかった・待ち切った）。3 つの実測はどれも意味を
    // 持たないので、**理由を書いて即座に返す**（期限まで待って実測を失うより、理由の方が要る）。
    return {
      firstScreenMs: null,
      traversal: {
        medianMs: null,
        reachedRow: null,
        rowCount: rowCountOf(),
        reason: "文書が保持されなかった",
      },
      edit: { appliedMs: null, undone: "ng:文書が保持されなかった" },
      paintFailed: false,
      ...surfaceFields(),
      items: [],
      reason:
        held.kind === "unavailable"
          ? `文書を読み込めなかった: ${held.reason}`
          : "文書が保持されるのを待っていたが、現れなかった",
    };
  }
  const displayAsked = performance.now();
  const table = await waitFor(tableOf, TABLE_WAIT_MS);
  // **この瞬間が要件 11.2 の実測である**（表が描かれた最初の瞬間）。走査と編集の後で
  // 測ってはいけない（それでは 3 つの実測が同じ時計を共有してしまう）。
  const firstScreenMs = performance.now() - displayAsked;
  if (table === null) {
    return {
      firstScreenMs: null,
      traversal: { medianMs: null, reachedRow: null, rowCount: rowCountOf(), reason: "表が現れなかった" },
      edit: { appliedMs: null, undone: "ng:表が現れなかった" },
      paintFailed: false,
      ...surfaceFields(),
      items: [],
      reason: "表が現れなかった（待ちの上限を超えた）",
    };
  }
  // 面が実際に描かれるまで待つ（DOM が在るだけの状態を「最初の画面」と数えない）。
  await waitFor(() => {
    const scroller = scrollerOf();
    return scroller !== null && scroller.scrollHeight > 1_000 ? true : null;
  }, TABLE_WAIT_MS);
  // **描かれた面の色数**（12.2 の成立側の実測）。**器が現れた瞬間ではなく、面が描かれてから
  // 読む** — 器（`jxcel-grid-table`）は React の最初の描画で現れるが、移植口の面（canvas）と
  // その中身はその後に組み立てられ、塗られる（実測: 器が現れた時点で読むと面が無い）。
  const surfaceReading: SurfaceReading = paintFailure
    ? surfaceNow()
    : await waitForSurfaceColors(TABLE_WAIT_MS);
  const surfaceColors = surfaceReading.colors;

  // 描画成立の検査（9.3）は面の組み立ての効果が走る。**告知 1 行が出ているかどうか**を読む
  // （`jxcel-grid-failure` はシートを開けなかったときの面であり、描画の不成立ではない）。
  // **塗られない条件では告知が遅れて出る**（検査は面が現れるのを上限まで待ってから結論する。
  // `./renderHealth` の `PAINT_PROBE_MS`）。通常の起動では告知が出ないことを確かめるだけなので
  // 短くてよい。
  await waitFor(() => (paintNoticeShown() ? true : null), paintFailure ? 6_000 : 1_000);
  const paintFailed = paintNoticeShown();
  if (paintFailed !== paintFailureRequested()) {
    reason = paintFailureRequested()
      ? "塗られない条件で告知が出なかった"
      : "通常の条件で告知が出た";
  }
  if (paintFailure) {
    // **塗られない条件では走査と編集を観測しない** — 面が空のままなので、走査の標本も
    // 編集の反映も意味を持たない（要件 12.2 の観測がこの起動の目的である）。
    await waitFor(() => (paintNoticeShown() ? true : null), 10_000);
    const failed = paintNoticeShown();
    return {
      firstScreenMs,
      traversal: {
        medianMs: null,
        reachedRow: null,
        rowCount: rowCountOf(),
        reason: "塗られない条件の起動では観測しない",
      },
      edit: { appliedMs: null, undone: "ok:塗られない条件の起動では観測しない" },
      paintFailed: failed,
      ...surfaceFields(),
      // **塗られない条件の起動では筋書きを走らせない**（面が空であり、走査の標本も編集の反映も
      // 意味を持たない。12.2 の陽性の観測がこの起動の目的である）。
      items: [],
      reason: failed ? null : "塗られない条件で告知が出なかった",
    };
  }
  const traversal = await traverse();
  if (traversal.reason !== null && reason === null) {
    reason = traversal.reason;
  }
  const edit = await editAndUndo();
  // **筋書き（群 10 が閉じた経路）を走らせる。**計測が済んだ後に走らせるのは、走査の標本が
  // 100,000 行の標本そのものを測るためである（表示を変えると前提が変わる）。
  const items = await runScenario();
  return {
    firstScreenMs,
    traversal,
    edit,
    paintFailed,
    surfaceColors,
    items,
    surface: surfaceReading.census,
    reason,
  };
}

/** 検証専用の観測の画面。**製品の画面をそのまま描く**（自前の表を組まない）。 */
export function GridObservation(): ReactElement {
  const [line, setLine] = useState(
    "[検証] グリッドの観測: 状態=未測定 理由=観測がまだ終わっていない",
  );
  useEffect(() => {
    let cancelled = false;
    const report = (observation: Observation): void => {
      // **記録へも残す**（3 OS の検査器が読む唯一の場所である）。捨てた後でも記録は行う —
      // 記録は画面の表示に依存しない。
      recordObservation(observation);
      if (!cancelled) {
        setLine(describeObservation(observation));
      }
    };
    void observe().then(report, (error: unknown) => {
      report({
        firstScreenMs: null,
        traversal: { medianMs: null, reachedRow: null, rowCount: null, reason: "観測が例外で止まった" },
        edit: { appliedMs: null, undone: "ng:観測が例外で止まった" },
        paintFailed: false,
        ...surfaceFields(),
        items: [],
        reason: `観測が例外で止まった: ${String(error)}`,
      });
    });
    // **期限を打って部分でも書く**（走査が返らない場合でも、それまでの実測が読める）。
    const timer = window.setTimeout(() => {
      report({
        firstScreenMs: null,
        traversal: { medianMs: null, reachedRow: null, rowCount: rowCountOf(), reason: "期限を超えた" },
        edit: { appliedMs: null, undone: "ng:期限を超えた" },
        paintFailed: false,
        ...surfaceFields(),
        items: [],
        reason: `観測が ${String(OBSERVATION_DEADLINE_MS)} ms で終わらなかった`,
      });
    }, OBSERVATION_DEADLINE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, []);
  return (
    <div>
      {/*
        **この `aria-label` が 3 OS の段の唯一の証拠である**（AT-SPI の名前に出る。
        `scripts/check-grid-observation.sh` が読む）。
      */}
      <div
        aria-label={line}
        data-testid="jxcel-grid-observation"
        // **`display: none` にしない** — 見えない要素はアクセシビリティの木から落ちうる
        // （この行が 3 OS の段の唯一の証拠である）。1 画素の面に置いて読み上げの対象に残す。
        style={{
          position: "absolute",
          width: "1px",
          height: "1px",
          overflow: "hidden",
          clipPath: "inset(50%)",
          whiteSpace: "nowrap",
        }}
      >
        {line}
      </div>
      {/*
        **貼り付けの準備の印を置く専用の要素である。**印を観測の行の要素へ置いてはならない —
        あちらの `aria-label` は画面が書く（観測が終わると書き換わり、**段が活性化の機会を
        失う**。実測: 段の走査が印を見つけられず、貼り付けの項目が成立しなかった）。ここは
        **React が `aria-label` を書かない要素**であり、印は観測の画面の側が置き、外す。
      */}
      <div
        data-testid="jxcel-grid-paste-ready"
        style={{
          position: "absolute",
          width: "1px",
          height: "1px",
          overflow: "hidden",
          clipPath: "inset(50%)",
          whiteSpace: "nowrap",
        }}
      />
      <GridScreen />
    </div>
  );
}

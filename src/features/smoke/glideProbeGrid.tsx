/**
 * 検証専用: グリッドライブラリ（`@glideapps/glide-data-grid`）を**実際に描く**標本の面
 * （tasks.md 1.5、要件 12.1）。
 *
 * 所有: [`GlideProbe`](./glideProbe.tsx)（検証専用の使い捨ての画面）。
 *
 * # なぜ本モジュールが分かれているか（重要）
 *
 * **本モジュールは `@glideapps/glide-data-grid` を静的に import し、その CSS も取り込む。**
 * したがってこれが**配布物のバンドルへ入ると、ライブラリ本体とスタイルが配布物へ入る**。
 * 呼び出し元の [`GlideProbe`](./glideProbe.tsx) を `SHELL_SCREEN_REGISTRY` へ直接足すだけでは
 * 足りない — Vite の `define`（`vite.config.ts` の `__JXCEL_VERIFICATION__`）で登録を括っても、
 * **静的 import の CSS は Rollup の定数畳み込みで落ちない**（実際に測った: 既定のビルドで
 * `dist/assets/index-*.css` が新たに 8.5 kB 生成され、`.gdg-*` / `.dvn-*` の規則が現れた）。
 * 配布物から確実に落ちるのは**到達不能な動的 import の塊**だけである（`src/main.tsx` の
 * `verificationBulk` が同じ理由で同じ形になっている）。
 *
 * そこで [`GlideProbe`](./glideProbe.tsx) は本モジュールを**動的 import** で読み、本モジュール
 * （とライブラリ本体・CSS）は独立した塊になる。既定のビルド（`__JXCEL_VERIFICATION__` が
 * `false`）では画面の登録ごと消えて本モジュールへの参照が無くなり、**塊の生成そのものが
 * 取りやめられる**。`scripts/check-shipping-bundle.sh` が配布物の `dist/` を機械検査する。
 *
 * # ライブラリの流儀に従う範囲
 *
 * 本モジュールは移植口（`src/features/grid/renderer/port.ts`）を**まだ持たない**。7.2 が
 * 移植口と `glideAdapter.tsx` を実装し、この面はそのときに捨てる（使い捨て）。
 */
import {
  DataEditor,
  GridCellKind,
  type DataEditorRef,
  type GridCell,
  type GridColumn,
  type Item,
  type Rectangle,
} from "@glideapps/glide-data-grid";
// ライブラリのスタイル。**スクロールの成立そのものがこの CSS に依る**（`.dvn-scroller` の
// `overflow` は linaria が展開したクラス規則が与える）ので、省くと広がりが観測できない。
import "@glideapps/glide-data-grid/dist/index.css";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  type ReactElement,
} from "react";

import { APPEARANCE_VARS } from "../../shell/theme";
import {
  MAX_FRAME_SAMPLES,
  MIN_FRAMES_FOR_MEDIAN,
  ROWS_PER_FRAME,
  countDistinctColors,
  medianOf,
  percentileOf,
  probePaint,
  readTimerResolutionMs,
  readWebkitVersion,
  sampleTraversal,
  type GlideProbeMeasurement,
} from "./glideProbeMeasure";

/**
 * 実測した広がり（DOM から読む値）。**呼び出し元が `data-probe-*` として出す**ので、
 * ここでは数えるだけで、判定はしない。
 */
export interface GlideProbeExtent {
  /** スクロール要素（`.dvn-scroller`）の内容の高さ（`scrollHeight`）。 */
  readonly scrollHeight: number;
  /** スクロール要素の見えている高さ（`clientHeight`）。 */
  readonly clientHeight: number;
  /** スクロール要素の現在の縦位置（`scrollTop`）。末尾へ移動した後は最大値になる。 */
  readonly scrollTop: number;
  /** いま見えている先頭の行（0 起点）。 */
  readonly firstVisibleRow: number;
  /** いま見えている末尾の行（0 起点・両端を含む）。 */
  readonly lastVisibleRow: number;
}

/**
 * 標本の面が受け取るもの。**数は画面側が決める**（この面は形だけを知る）。
 */
export interface GlideProbeGridProps {
  readonly rows: number;
  readonly columns: number;
  readonly rowHeight: number;
  readonly headerHeight: number;
  readonly heightPx: number;
  readonly onExtent: (extent: GlideProbeExtent) => void;
  /** 走査の計測の結果（1.6）。**1 回だけ**届く（マウント後に自動で 1 回走らせる）。 */
  readonly onMeasured: (measurement: GlideProbeMeasurement) => void;
}

/**
 * 走査の計測の結果（tasks.md 1.6 / 要件 11.1、12.2）。**定義の正本は
 * [`glideProbeMeasure`](./glideProbeMeasure.ts)** である（計測を行う側が型を持つ。
 * ここは面の境界として再輸出するだけである）。
 */
export type { GlideProbeMeasurement } from "./glideProbeMeasure";

/** グリッドを入れる枠の見た目。**ここが横にはみ出さないようにする**（縦はグリッドが持つ）。 */
const FRAME_STYLE = {
  width: "100%",
  border: `1px solid var(${APPEARANCE_VARS.screenMuted})`,
  borderRadius: "0.375rem",
  overflow: "hidden",
} as const;

/** 末尾へ移動する操作の見た目。 */
const BUTTON_STYLE = {
  alignSelf: "flex-start",
  font: "inherit",
  fontSize: "0.8125rem",
  padding: "0.2rem 0.6rem",
  borderRadius: "0.25rem",
  cursor: "pointer",
  color: `var(${APPEARANCE_VARS.screenText})`,
  backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/**
 * セルの内容。**行番号と列番号だけの決定的な関数**である（同じ添字なら常に同じ内容になる。
 * 乱数も時刻も使わない）。1.4 の標本の生成器は **Rust 側にあり、フロントエンドから到達
 * できない**ため（IPC も足さない）、標本はこの面が自分で組み立てる。
 *
 * 呼ばれるのは**見えているセルだけ**である（canvas の仮想化。10 万行でも数千回に収まる）。
 */
function specimenCell(column: number, row: number): GridCell {
  const text =
    column === 0
      ? `行 ${String(row + 1)}`
      : // 決定的な擬似値。桁を揃えるのは、列幅の見え方を安定させるためである。
        String((row * 31 + column * 7) % 100_007).padStart(6, "0");
  return {
    kind: GridCellKind.Text,
    allowOverlay: false,
    readonly: true,
    displayData: text,
    data: text,
  };
}

/**
 * ライブラリへ渡すセル取得関数。**モジュール定数**である（状態を閉じ込めないので、
 * 描画の間ずっと同じ関数でよい）。
 */
function getCellContent(cell: Item): GridCell {
  const [column, row] = cell;
  return specimenCell(column, row);
}

/**
 * 標本を描く面。**行数と大きさだけを props で受け取り、状態は持たない**（数えるのは
 * 呼び出し元の責務であり、この面は実測値を報告するだけである）。
 */
export function GlideProbeGrid({
  rows,
  columns,
  rowHeight,
  headerHeight,
  heightPx,
  onExtent,
  onMeasured,
}: GlideProbeGridProps): ReactElement {
  const gridRef = useRef<DataEditorRef | null>(null);
  const frameRef = useRef<HTMLDivElement | null>(null);
  const visibleRef = useRef<{ first: number; last: number }>({
    first: 0,
    last: 0,
  });
  // 計測は**1 回だけ**走らせる（描き直しのたびに走らせると、走査が繰り返されて結果が
  // 上書きされる。しかも 2 回目以降は既に末尾に居るので走査にならない）。
  const measuredRef = useRef(false);
  // 実測値を描き直しの入力にするのは**この面の外**である（呼び出し元が `data-probe-*` へ
  // 出す）。ここでは「読んで報告する」だけなので、報告を再描画のきっかけにしない。
  const report = useCallback(() => {
    const frame = frameRef.current;
    if (frame === null) {
      return;
    }
    const scroller = frame.querySelector(".dvn-scroller");
    const measured = scroller instanceof HTMLElement ? scroller : null;
    onExtent({
      scrollHeight: measured?.scrollHeight ?? 0,
      clientHeight: measured?.clientHeight ?? 0,
      scrollTop: measured?.scrollTop ?? 0,
      firstVisibleRow: visibleRef.current.first,
      lastVisibleRow: visibleRef.current.last,
    });
  }, [onExtent]);

  // グリッドは自分の寸法を**レイアウトの後に**測る（`useResizeDetector`）ので、マウント直後の
  // 1 フレームでは広がりがまだ無い。2 フレーム待ってから読む。
  useEffect(() => {
    const first = requestAnimationFrame(() => {
      const second = requestAnimationFrame(report);
      return second;
    });
    return () => {
      cancelAnimationFrame(first);
    };
  }, [report]);

  // 走査の計測（tasks.md 1.6）。**面の寸法が確定した後**に 1 回だけ走らせる（寸法が 0 の
  // うちに走査すると、仮想化が何も描かずフレーム時間が「速く」出る）。計測は走査・塗りの
  // 読み戻し・canvas の色数の 3 つを行い、結果を 1 回だけ報告する。
  useEffect(() => {
    if (measuredRef.current) {
      return;
    }
    measuredRef.current = true;
    let cancelled = false;

    const run = async (): Promise<void> => {
      // 面が寸法を測るまで待つ（拡大の報告を 2 フレーム待つ既存の処理と同じ理由）。
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            resolve();
          });
        });
      });
      if (cancelled) {
        return;
      }

      const samples = await sampleTraversal({
        rowsPerFrame: ROWS_PER_FRAME,
        maxSamples: MAX_FRAME_SAMPLES,
        rows,
        readLastVisibleRow: () => visibleRef.current.last,
        advance: (row) => {
          gridRef.current?.scrollTo(0, row, "vertical");
        },
      });
      if (cancelled) {
        return;
      }

      // 走査の直後の状態で塗りの読み戻しと canvas の色数を取る（**走査の後**にするのは、
      // 描画が実際に進んだ面を見るためである）。
      const paint = probePaint();
      const canvas = frameRef.current?.querySelector("canvas");
      const gridColors =
        canvas instanceof HTMLCanvasElement ? countDistinctColors(canvas) : 0;

      const median = medianOf(samples.frameTimes);
      // **測定不能を「速い」と読ませない。** 標本が足りない・中央値が無い・走査が末尾へ
      // 届いていない・グリッドに何も塗られていない、のいずれかなら `unmeasurable` にして
      // 理由を残す（数を捏造しない）。
      const reasons: string[] = [];
      if (median === null) {
        reasons.push("フレームの標本が 1 本も取れなかった（requestAnimationFrame が発火していない）");
      } else if (samples.frameTimes.length < MIN_FRAMES_FOR_MEDIAN) {
        reasons.push(
          `フレームの標本が足りない（${String(samples.frameTimes.length)} < ${String(MIN_FRAMES_FOR_MEDIAN)}）`,
        );
      }
      if (samples.lastVisibleRow < rows - 1) {
        reasons.push(
          `走査が末尾へ届かなかった（最後に見えた行 = ${String(samples.lastVisibleRow)} / 期待 ${String(rows - 1)}）`,
        );
      }
      if (!paint.ok) {
        reasons.push(`塗りの読み戻しが失敗した（${paint.pixel}）`);
      }
      if (gridColors < 2) {
        reasons.push(
          `標本の面の canvas が一様である（色数 ${String(gridColors)}。DOM はあるが何も塗られていない）`,
        );
      }

      onMeasured({
        status: reasons.length === 0 ? "measured" : "unmeasurable",
        reason: reasons.join(" / "),
        frames: samples.frameTimes.length,
        medianMs: median,
        lastVisibleRow: samples.lastVisibleRow,
        rows,
        columns,
        paintOk: paint.ok,
        paintPixel: paint.pixel,
        gridColors,
        webkit: readWebkitVersion(),
        tickMs: readTimerResolutionMs(),
        p90Ms: percentileOf(samples.frameTimes, 0.9),
        maxMs: percentileOf(samples.frameTimes, 1),
      });
    };

    void run();
    return () => {
      cancelled = true;
    };
  }, [columns, onMeasured, rows]);

  /** 可視範囲の変化を受ける。**描画が進んだことの証拠**（行の範囲）をそのまま報告する。 */
  const onVisibleRegionChanged = useCallback(
    (range: Rectangle) => {
      const first = Math.max(0, Math.min(range.y, rows - 1));
      visibleRef.current = {
        first,
        last: Math.max(first, Math.min(range.y + Math.max(range.height, 1), rows) - 1),
      };
      report();
    },
    [report, rows],
  );

  /** 最末尾の行へ移動する。**10 万行の広がりが走査できることの確認**に使う。 */
  const scrollToLastRow = useCallback(() => {
    gridRef.current?.scrollTo(0, rows - 1, "vertical");
    // ライブラリはスクロールの反映と可視範囲の報告を次のフレームで行うので、実測もそれに
    // 合わせて 1 フレーム待つ。
    requestAnimationFrame(report);
  }, [report, rows]);

  // 列の定義。**`columns` が変わったときだけ作り直す**（描画のたびに作り直さない）。
  const columnDefs = useMemo<readonly GridColumn[]>(
    () =>
      Array.from({ length: columns }, (_, column) => ({
        title: column === 0 ? "行番号" : `列 ${column}`,
        width: column === 0 ? 96 : 120,
      })),
    [columns],
  );

  return (
    <>
      <div ref={frameRef} style={FRAME_STYLE}>
        <DataEditor
          ref={gridRef}
          columns={columnDefs}
          rows={rows}
          getCellContent={getCellContent}
          rowHeight={rowHeight}
          headerHeight={headerHeight}
          width="100%"
          height={heightPx}
          onVisibleRegionChanged={onVisibleRegionChanged}
        />
      </div>
      <button
        type="button"
        data-testid="jxcel-smoke-glide-probe-scroll-end"
        onClick={scrollToLastRow}
        style={BUTTON_STYLE}
      >
        末尾の行へ移動
      </button>
    </>
  );
}

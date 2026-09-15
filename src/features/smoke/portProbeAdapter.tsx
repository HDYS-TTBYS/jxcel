/**
 * 検証専用: 移植口の実装（`../grid/renderer/glideAdapter`）を**実物の DOM の上で**駆動する面
 * （tasks.md 7.2。要件 1.2, 1.3, 2.4, 7.1, 7.2）。
 *
 * # なぜこれが要るのか（単体テストでは足りない）
 *
 * 7.2 の受け入れは**見え方の主張**である — 「10 万行のシートを走査でき、選択した範囲が視覚的に
 * 区別でき、列幅と列の位置が操作できる」。canvas に本当に描かれるかは `vitest`（`node` 環境）では
 * 観測できない（`glideAdapter.test.ts` が固定するのは写像と配管だけである）。
 * `tech.md`「GUI・配布物・プラットフォーム差を含む主張は、実物を起動して観測した結果で
 * 裏付けること。単体テストは回帰の網であって受入の証明ではない」に従い、**実物を起動して
 * 観測する**のが本モジュールである（1.6 の `glideProbe*` と同じ形の使い捨ての段。
 * **9.2 / 9.3 が恒久の観測を入れた時点で、この面と段は取り除く**）。
 *
 * # ライブラリではなく**移植口**を駆動する（1.6 との違い）
 *
 * 1.6 の標本は Glide の部品を直に置いて走査した。本モジュールは**移植口を通す** —
 * `createGlideAdapter().mount(container, spec)` でマウントし、`handle.scrollTo` で末尾へ飛び、
 * **利用者の操作は DOM のイベントとして**（ポインタと `copy` / `paste`）注ぐ。したがって
 * 観測されるのは「移植口の実装が実物の上で何をするか」である。
 *
 * # 操作方法（**本物と同じ経路を通す**）
 *
 * | 主張 | 注ぐもの | 観測するもの |
 * |---|---|---|
 * | 走査 | `handle.scrollTo({row: 99999, column: 0})`（移植口の口） | `scrollTop` / 内容の高さ / 到達行 / 塗り / 色数 |
 * | 選択 | canvas への `pointerdown` → `pointermove` → `pointerup`（**本物のポインタ**） | 移植口の `onSelectionChange`、同じ画素の色の変化、選択として印されたセルの数 |
 * | 列幅 | 見出しの右端への `pointerdown` → `pointermove` → `pointerup` | 移植口の `onColumnResize`、**内容の幅の増分** |
 * | 列の移動 | 見出しへの `pointerdown` → `pointermove`（20 px 以上）→ `pointerup` | 移植口の `onColumnMove`、**描かれている見出しの並び** |
 * | 複製 | canvas への `copy` イベント（本物は Ctrl+C が出す） | 移植口の `onCopy`、クリップボードからの読み戻し |
 * | 貼り付け | canvas への `paste` イベント（文字列を載せた `DataTransfer`） | 移植口の `onPaste`、**送った文字列と受け取った文字列の一致** |
 * | 読み込み中 | （末尾は取得済みでない行である） | 骨組みの棒が引かれている画素の数（地色と同じなら 0） |
 *
 * **列幅と列の位置の反映は再マウントで行う。**`RendererHandle` に幅や順を押し込む口が無く、
 * 変化は次の `mount` の仕様に載る（`glideAdapter.tsx` の申し送り）— 本面は移植口の契約どおりに
 * 「新しい仕様でマウントし直す」ことで、変更が**描かれた幅と並び**に現れることを見る。
 *
 * # 値を捏造しない
 *
 * 途中で例外が出たら `状態=failed` と `理由=` を残して終わる。`ok` のときにだけ、3 つの主張に
 * 対応する値が埋まる。**画素の読みは「変わった / 変わらない」「地色と同じ / 違う」という
 * 事実だけを出し、判定は読む側（`scripts/check-port-interaction.sh`）が行う。**
 */
import { useEffect, useRef, type ReactElement } from "react";

import {
  GLIDE_ADAPTER_HEADER_HEIGHT,
  GLIDE_ADAPTER_ROW_HEIGHT,
  createGlideAdapter,
} from "../grid/renderer/glideAdapter";
import type {
  CellPosition,
  CellRange,
  RenderCell,
  RendererHandle,
  RendererSelection,
  RendererSpec,
} from "../grid/renderer/port";
import type { TypeKindTag } from "../../ipc/bindings";
import { countDistinctColors, probePaint, readWebkitVersion } from "./glideProbeMeasure";
import type { PortProbeFacts } from "./portProbeFacts";

/** 標本の行数（要件 1.1 の 10 万行）。 */
export const PORT_PROBE_ROWS = 100_000;

/** 標本の列数（要件 1.1 の 30 列）。 */
export const PORT_PROBE_COLUMNS = 30;

/** 標本の列の幅（ピクセル）。**変更の前後の増分を読めるよう、全列を同じ幅にする。** */
const PORT_PROBE_COLUMN_WIDTH = 120;

/** 取得済みとして答える行数。**これより先は読み込み中である**（窓の記憶（7.3）の代役）。 */
const PORT_PROBE_FETCHED_ROWS = 2_000;

/** 標本の葉の型の札（列ごとに巡回させる）。 */
const PORT_PROBE_KINDS: readonly TypeKindTag[] = ["Text", "Int", "Ref"];

/** 標本の違反の帯の周期（窓が運ぶ印の代役）。読む画素の行はこの帯の外に選んである。 */
const VIOLATION_PERIOD = 7;

/** グリッドの表示の高さ（ピクセル）。**器に確定した寸法を与える**（移植口は寸法を運ばない）。 */
export const PORT_PROBE_HEIGHT_PX = 420;

/** 見出しの並びを読む列数（先頭から）。**移動の結果は先頭 4 列で足りる。** */
const HEADER_SAMPLE = 4;

/** アクセシビリティの木が作られるまでの待ち（`useDebouncedMemo` は 200 ms である）。 */
const ACCESSIBLE_TREE_WAIT_MS = 400;

/** 貼り付けで送る文字列。**複製の結果に依存させない**（配管の往復だけを見る）。 */
const PASTE_TEXT = "P1\tP2\nQ1\tQ2";

/** 骨組みの棒を探す画素の縦の刻み（ピクセル）。 */
const STRIP_STEP_PX = 2;

/** 標本のセル。**行と列だけで決まる**（同じ位置なら常に同じ内容である）。 */
function probeCell(position: CellPosition): RenderCell {
  // 取得済みの範囲の外は読み込み中である（**空白で代用しない** — 値なしと区別がつかない）。
  const loading = position.row >= PORT_PROBE_FETCHED_ROWS;
  return {
    text: loading ? "" : `${String(position.row)}:${String(position.column)}`,
    variant: PORT_PROBE_KINDS[position.column % PORT_PROBE_KINDS.length] ?? "Text",
    violated: position.row % VIOLATION_PERIOD === 3,
    loading,
  };
}

/** 複製が返す表形式のテキスト。**本物では Rust 側の `PasteCodec` が作る**（ここはその代役）。 */
function tableTextOf(range: CellRange): string {
  const lines: string[] = [];
  for (let row = range.start.row; row <= range.end.row; row += 1) {
    const cells: string[] = [];
    for (let column = range.start.column; column <= range.end.column; column += 1) {
      cells.push(probeCell({ row, column }).text);
    }
    lines.push(cells.join("\t"));
  }
  return lines.join("\n");
}

/** 1 フレーム待つ。 */
function nextFrame(): Promise<void> {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      resolve();
    });
  });
}

/** フレームを数える待ち（描画は `requestAnimationFrame` の上で進む）。 */
async function frames(count: number): Promise<void> {
  for (let index = 0; index < count; index += 1) {
    await nextFrame();
  }
}

/** 状態の確定を待つ（アクセシビリティの木は 200 ms 遅れる）。 */
function delay(ms: number): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });
}

/** canvas の画素を 1 つ読む（`r,g,b,a` の形。**読み戻しが成立しない場合は投げる**）。 */
function readPixel(canvas: HTMLCanvasElement, x: number, y: number): string {
  const context = canvas.getContext("2d");
  if (context === null) {
    throw new Error("canvas の 2D 文脈が取れない");
  }
  const rect = canvas.getBoundingClientRect();
  const data = context.getImageData(
    Math.round(x * (canvas.width / rect.width)),
    Math.round(y * (canvas.height / rect.height)),
    1,
    1,
  ).data;
  return `${String(data[0] ?? 0)},${String(data[1] ?? 0)},${String(data[2] ?? 0)},${String(data[3] ?? 0)}`;
}

/**
 * ある x の縦の帯を走査し、**地色と違う画素の数**を数える。
 *
 * 画素そのものを 1 点だけ読むのではなく数を数えるのは、**行の高さに対する座標の誤差で
 * 結論が変わらないようにする**ためである（骨組みの棒は行の中央に高さ 18 px で引かれるので、
 * 帯の内側を 2 px 刻みで見れば必ず当たる）。
 */
function countPixelsDifferingFrom(
  canvas: HTMLCanvasElement,
  x: number,
  background: string,
  fromY: number,
  toY: number,
): number {
  let count = 0;
  for (let y = fromY; y < toY; y += STRIP_STEP_PX) {
    if (readPixel(canvas, x, y) !== background) {
      count += 1;
    }
  }
  return count;
}

/**
 * ポインタの操作を 1 つ注ぐ。**本物と同じ経路である**（canvas に届いた `pointerdown` などが
 * window へ伝わり、Glide の聴取が受ける。合成のイベントでも経路は同じである）。
 *
 * **1 つの操作の途中でフレームを跨ぐ**（[`dragPointer`]）のは本物と同じ時間の流れにするためで
 * あり、それ自体が要件である — Glide の選択・並べ替え・幅の変更は**制御された props の再描画**を
 * 挟んで進む（掴んだ列や選択の現在値は React の状態として次の描画で届く）。押し込み・移動・
 * 離しを同じタスクで続けて注ぐと、移動の時点で状態がまだ届いておらず、**操作が成立しない**
 * （実測: 3 つを同期で注ぐと選択は「列の全体の選択」になり、幅の変更と列の移動は無反応だった）。
 */
function dispatchPointer(
  canvas: HTMLCanvasElement,
  type: "pointerdown" | "pointermove" | "pointerup",
  x: number,
  y: number,
  buttons: number,
): void {
  const rect = canvas.getBoundingClientRect();
  const init = {
    pointerId: 1,
    pointerType: "mouse",
    isPrimary: true,
    button: 0,
    buttons,
    clientX: rect.left + x,
    clientY: rect.top + y,
    bubbles: true,
    cancelable: true,
    composed: true,
  };
  // `pointerType` は「マウスとして扱うか」の判定に使われる（Glide の `getMouseArgsForPosition`）。
  canvas.dispatchEvent(new PointerEvent(type, init));
}

/**
 * ポインタの 1 操作（押し込み → 移動 → 離し）を、**フレームを跨いで**注ぐ。
 *
 * `moves` は移動の中間点である（列の移動では 20 px 以上動かしてから目的の位置へ運ぶ —
 * Glide は 20 px を超えるまで並べ替えを始めない）。
 */
async function dragPointer(
  canvas: HTMLCanvasElement,
  from: { readonly x: number; readonly y: number },
  moves: readonly { readonly x: number; readonly y: number }[],
): Promise<void> {
  dispatchPointer(canvas, "pointerdown", from.x, from.y, 1);
  await frames(2);
  for (const move of moves) {
    dispatchPointer(canvas, "pointermove", move.x, move.y, 1);
    await frames(2);
  }
  const last = moves.at(-1) ?? from;
  dispatchPointer(canvas, "pointerup", last.x, last.y, 0);
  await frames(2);
}

/** 区間の先頭の x（canvas の論理座標）。**横にスクロールしていない前提**である。 */
function columnLeft(column: number, widths: readonly number[]): number {
  let left = 0;
  for (let index = 0; index < column; index += 1) {
    left += widths[index] ?? PORT_PROBE_COLUMN_WIDTH;
  }
  return left;
}

/** 可視行の中心の y（canvas の論理座標）。**縦にスクロールしていない前提**である。 */
function rowCenter(row: number): number {
  return (
    GLIDE_ADAPTER_HEADER_HEIGHT +
    row * GLIDE_ADAPTER_ROW_HEIGHT +
    GLIDE_ADAPTER_ROW_HEIGHT / 2
  );
}

/** アクセシビリティの木の行（`aria-rowindex` は 0 行目が 2 である）。 */
function accessibleRows(container: HTMLElement): readonly number[] {
  return [...container.querySelectorAll('tbody tr[role="row"]')].map((row) =>
    Number(row.getAttribute("aria-rowindex") ?? "0"),
  );
}

/** 選択として印されたセルの数（アクセシビリティの木）。 */
function accessibleSelectedCells(container: HTMLElement): number {
  return container.querySelectorAll('td[role="gridcell"][aria-selected="true"]').length;
}

/** 描かれている見出しの並び（表示順の先頭から）。 */
function headerTitles(container: HTMLElement): readonly string[] {
  return [...container.querySelectorAll('th[role="columnheader"]')]
    .slice(0, HEADER_SAMPLE)
    .map((header) => (header.textContent ?? "").trim());
}

/** 範囲を `開始-終了` の形にする（読む側が空白で切れるようにする）。 */
function describeRange(range: CellRange | null): string {
  if (range === null) return "なし";
  return (
    `${String(range.start.row)}:${String(range.start.column)}` +
    `-${String(range.end.row)}:${String(range.end.column)}`
  );
}

/** 位置を `行:列` の形にする。 */
function describePosition(position: CellPosition | null): string {
  if (position === null) return "なし";
  return `${String(position.row)}:${String(position.column)}`;
}

/** canvas を要求する（無ければ実装の誤りである）。 */
function requireCanvas(container: HTMLElement): HTMLCanvasElement {
  const canvas = container.querySelector("canvas");
  if (!(canvas instanceof HTMLCanvasElement)) {
    throw new Error("canvas が無い（描画が始まっていない）");
  }
  return canvas;
}

/** 仮想スクロールの要素を要求する（無ければライブラリの構造が変わっている）。 */
function requireScroller(container: HTMLElement): HTMLElement {
  const scroller = container.querySelector(".dvn-scroller");
  if (!(scroller instanceof HTMLElement)) {
    throw new Error(".dvn-scroller が無い（ライブラリの構造が変わった）");
  }
  return scroller;
}

/** 移植口が受け取ったもの（**面が記録するだけであり、状態を変えない**）。 */
interface Observed {
  selection: RendererSelection | null;
  resized: { readonly column: number; readonly width: number } | null;
  moved: { readonly from: number; readonly to: number } | null;
  copyRange: CellRange | null;
  copied: string;
  pasteAnchor: CellPosition | null;
  pasted: string;
}

/** 失敗を理由つきで返す（**数を捏造しない**）。 */
function failure(reason: string): PortProbeFacts {
  return {
    status: "failed",
    reason,
    rows: PORT_PROBE_ROWS,
    columns: PORT_PROBE_COLUMNS,
    reachedRow: 0,
    scrollTop: 0,
    scrollHeight: 0,
    clientHeight: 0,
    paintOk: false,
    paintPixel: "",
    colors: 0,
    selection: "なし",
    selectedCells: 0,
    selectionPixelChanged: false,
    selectionPixelBefore: "",
    selectionPixelAfter: "",
    resize: "なし",
    contentWidthBefore: 0,
    contentWidthAfter: 0,
    move: "なし",
    headerOrder: "",
    copyRange: "なし",
    copyChars: 0,
    clipboard: "不可",
    pasteAnchor: "なし",
    pasteRoundTrip: false,
    loadingPixels: 0,
    loadingStrip: 0,
    backgroundPixel: "",
    webkit: "",
  };
}

/**
 * 実物の上で 1 回だけ駆動し、観測した事実を返す。**逐次である**（各段は前の段の状態に依る:
 * 複製は選択の範囲を写し、貼り付けの錨は選択の左上である）。
 */
async function driveAdapterProbe(container: HTMLElement): Promise<PortProbeFacts> {
  const adapter = createGlideAdapter();
  const widths: number[] = Array.from(
    { length: PORT_PROBE_COLUMNS },
    () => PORT_PROBE_COLUMN_WIDTH,
  );
  /** 表示順（列の添字の並び）。列の移動はこの並びを入れ替える。 */
  let order: number[] = Array.from({ length: PORT_PROBE_COLUMNS }, (_unused, index) => index);
  const observed: Observed = {
    selection: null,
    resized: null,
    moved: null,
    copyRange: null,
    copied: "",
    pasteAnchor: null,
    pasted: "",
  };

  const buildSpec = (): RendererSpec => ({
    columns: order.map((column) => ({
      // **見出しに空白を入れない。** 観測の行は空白で区切って読まれる（`portProbeFacts.ts`）ので、
      // 見出しの並びを 1 つの値として読めるようにするためである。
      title: `列${String(column)}`,
      width: widths[column] ?? PORT_PROBE_COLUMN_WIDTH,
    })),
    rowCount: PORT_PROBE_ROWS,
    // 8.2 が広げた 3 つ。本面は**実装の通知を観測するだけ**であり、下ろす選択は持たない
    // （現在位置の移動と追随の観測は 9.2 / 9.3 の仕事である）。
    selection: { current: { row: 0, column: 0 }, range: { start: { row: 0, column: 0 }, end: { row: 0, column: 0 } } },
    rowMarkers: "clickable-number",
    getCell: probeCell,
    onVisibleSpanChange: () => undefined,
    onSelectionChange: (selection) => {
      observed.selection = selection;
    },
    onActivateEditor: () => {
      // 本面は編集の起動を観測しない（要件 2.2 の入力手段は 7.4 の登録簿が担う）。
    },
    onColumnResize: (column, width) => {
      observed.resized = { column, width };
    },
    onColumnMove: (from, to) => {
      observed.moved = { from, to };
    },
    onCopy: (range) => {
      observed.copyRange = range;
      observed.copied = tableTextOf(range);
      return Promise.resolve(observed.copied);
    },
    onPaste: (anchor, text) => {
      observed.pasteAnchor = anchor;
      observed.pasted = text;
      return Promise.resolve();
    },
  });

  let handle: RendererHandle = adapter.mount(container, buildSpec());
  // 列幅・列順は次の仕様でしか表示へ載らない（移植口の申し送り）。**作り直して描かせる。**
  const remount = (): void => {
    handle.destroy();
    container.replaceChildren();
    handle = adapter.mount(container, buildSpec());
  };

  await frames(3);
  await delay(ACCESSIBLE_TREE_WAIT_MS);

  const canvasAtTop = requireCanvas(container);
  // 地色の基準は**値のあるセル**の空いている部分から取る（列 4 は 3 文字しか持たないので、
  // 右寄りの x は地色である）。選択の範囲（列 0〜1）の外でもある。
  const backgroundPixel = readPixel(
    canvasAtTop,
    columnLeft(4, widths) + PORT_PROBE_COLUMN_WIDTH - 20,
    rowCenter(1) + 10,
  );

  // ---- 主張 2: 選択した範囲が視覚的に区別できる ----
  //
  // 読む画素はセル (2, 1) の右下寄りである（文字が届かず、既定では地色である）。
  const selectionProbeX = columnLeft(1, widths) + PORT_PROBE_COLUMN_WIDTH - 20;
  const selectionProbeY = rowCenter(2) + 10;
  const selectionPixelBefore = readPixel(canvasAtTop, selectionProbeX, selectionProbeY);

  await dragPointer(
    canvasAtTop,
    { x: columnLeft(0, widths) + 20, y: rowCenter(1) },
    [{ x: selectionProbeX, y: rowCenter(3) }],
  );
  await delay(ACCESSIBLE_TREE_WAIT_MS);

  const selectionPixelAfter = readPixel(requireCanvas(container), selectionProbeX, selectionProbeY);
  const selectedCells = accessibleSelectedCells(container);
  // **この時点の選択を控える。** このあとの段（列幅・列の移動）は見出しを掴むので、Glide 自身が
  // 「列の全体の選択」へ変えてしまう（見出しの操作としては正しい）。観測したいのは**選択の段で
  // 起きたこと**なので、ここで値を固定する。
  const selectionAfterDrag = observed.selection;

  // ---- クリップボードの配管（複製）----
  //
  // `copy` は本物では Ctrl+C が出すイベントである。同じイベントを canvas へ注ぐ（移植口の実装は
  // `GlideSurface` の捕獲の段で受けている）。
  requireCanvas(container).dispatchEvent(new Event("copy", { bubbles: true, cancelable: true }));
  await frames(2);
  await delay(150);

  let clipboard: string;
  try {
    const read = await navigator.clipboard.readText();
    clipboard = read === observed.copied && observed.copied !== "" ? "一致" : "不一致";
  } catch {
    // 読み取りの権限が無い環境である。**「読めなかった」を「一致した」と読み替えない。**
    clipboard = "不可";
  }

  // ---- クリップボードの配管（貼り付け）----
  const data = new DataTransfer();
  data.setData("text/plain", PASTE_TEXT);
  requireCanvas(container).dispatchEvent(
    new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
  );
  await frames(2);

  // ---- 主張 3: 列幅が操作できる ----
  //
  // 見出しの右端（列 1 の右端の 2 px 内側）を掴み、+120 px 動かす。Glide は掴んだ位置からの差を
  // 幅として報告するので、期待する幅は 元の幅 + 120 である。
  const resizeY = GLIDE_ADAPTER_HEADER_HEIGHT / 2;
  // **列の境界そのものを掴む。** Glide は幅を「ポインタの位置 - 列の左端」で決めるので、
  // 境界から 2 px 内側を掴むと 2 px 足りない幅になる（実測: 120 px 動かして 118 px 広がった）。
  const resizeEdgeX = columnLeft(2, widths);
  const contentWidthBefore = requireScroller(container).scrollWidth;
  await dragPointer(
    requireCanvas(container),
    { x: resizeEdgeX, y: resizeY },
    [{ x: resizeEdgeX + PORT_PROBE_COLUMN_WIDTH, y: resizeY }],
  );

  const resized = observed.resized;
  if (resized !== null) {
    widths[resized.column] = resized.width;
  }
  remount();
  await frames(3);
  await delay(ACCESSIBLE_TREE_WAIT_MS);
  const contentWidthAfter = requireScroller(container).scrollWidth;

  // ---- 主張 3: 列の位置が操作できる ----
  //
  // 列 2 の見出しを掴んで列 0 の上まで運ぶ（Glide は 20 px 以上の移動で並べ替えを始める）。
  const moveFrom = 2;
  const moveTo = 0;
  await dragPointer(
    requireCanvas(container),
    { x: columnLeft(moveFrom, widths) + 20, y: resizeY },
    [
      // 20 px 以上動かして並べ替えを始めさせ、そのうえで目的の列の上へ運ぶ。
      { x: columnLeft(moveFrom, widths) - 40, y: resizeY },
      { x: columnLeft(moveTo, widths) + 20, y: resizeY },
    ],
  );

  const moved = observed.moved;
  if (moved !== null) {
    const next = [...order];
    const [picked] = next.splice(moved.from, 1);
    if (picked !== undefined) {
      next.splice(moved.to, 0, picked);
      order = next;
    }
  }
  remount();
  await frames(3);
  await delay(ACCESSIBLE_TREE_WAIT_MS);
  const headerOrder = headerTitles(container).join(",");

  // ---- 主張 1: 10 万行の走査 ----
  //
  // **移植口の口（`handle.scrollTo`）で末尾へ飛ぶ**（要件 1.4 の「中間の行を順にたどらない」）。
  handle.scrollTo({ row: PORT_PROBE_ROWS - 1, column: 0 });
  await frames(3);
  await delay(ACCESSIBLE_TREE_WAIT_MS);

  const scroller = requireScroller(container);
  const canvas = requireCanvas(container);
  const rows = accessibleRows(container);
  const reachedRow = rows.length === 0 ? 0 : Math.max(...rows) - 2;
  const paint = probePaint();

  // ---- 読み込み中の描き方（末尾は取得済みでない行である）----
  //
  // 骨組みの棒はセルの中央に高さ 18 px で引かれ、幅は列の幅の半分（60 px）である。したがって
  // 列 4 の左端から 38 px の縦の帯は**棒の内側**であり、値の無い（読み込み中の）行では地色と
  // 違う画素が並ぶ。**数で示す**ので、行の高さに対する座標の誤差では結論が変わらない。
  const stripX = columnLeft(4, widths) + 38;
  const loadingStrip = countPixelsDifferingFrom(
    canvas,
    stripX,
    backgroundPixel,
    GLIDE_ADAPTER_HEADER_HEIGHT + 2,
    scroller.clientHeight - 2,
  );

  return {
    status: "ok",
    reason: "",
    rows: PORT_PROBE_ROWS,
    columns: PORT_PROBE_COLUMNS,
    reachedRow,
    scrollTop: scroller.scrollTop,
    scrollHeight: scroller.scrollHeight,
    clientHeight: scroller.clientHeight,
    paintOk: paint.ok,
    paintPixel: paint.pixel,
    colors: countDistinctColors(canvas),
    selection: describeRange(selectionAfterDrag?.range ?? null),
    selectedCells,
    selectionPixelChanged: selectionPixelBefore !== selectionPixelAfter,
    selectionPixelBefore,
    selectionPixelAfter,
    resize:
      observed.resized === null
        ? "なし"
        : `${String(observed.resized.column)}:${String(PORT_PROBE_COLUMN_WIDTH)}→${String(observed.resized.width)}`,
    contentWidthBefore,
    contentWidthAfter,
    move:
      observed.moved === null
        ? "なし"
        : `${String(observed.moved.from)}→${String(observed.moved.to)}`,
    headerOrder,
    copyRange: describeRange(observed.copyRange),
    copyChars: observed.copied.length,
    clipboard,
    pasteAnchor: describePosition(observed.pasteAnchor),
    pasteRoundTrip: observed.pasted === PASTE_TEXT,
    loadingPixels: loadingStrip,
    loadingStrip: Math.ceil((scroller.clientHeight - GLIDE_ADAPTER_HEADER_HEIGHT) / STRIP_STEP_PX),
    backgroundPixel,
    webkit: readWebkitVersion(),
  };
}

/** 面が受け取るもの。**観測が終わったら 1 回だけ報告する。** */
export interface PortProbeSurfaceProps {
  readonly onFacts: (facts: PortProbeFacts) => void;
}

/**
 * 標本の面。**マウントした器の中へ移植口をマウントし、1 回だけ駆動する**（使い捨てである）。
 *
 * 器は**確定した寸法を持つ**（移植口は寸法を運ばない。`height: 100%` は親の高さを要する）。
 */
export function PortProbeSurface({ onFacts }: PortProbeSurfaceProps): ReactElement {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const startedRef = useRef(false);

  useEffect(() => {
    const container = containerRef.current;
    if (container === null || startedRef.current) {
      return;
    }
    startedRef.current = true;
    let cancelled = false;

    const run = async (): Promise<void> => {
      const facts = await driveAdapterProbe(container).catch((error: unknown) =>
        failure(error instanceof Error ? error.message : String(error)),
      );
      if (!cancelled) {
        onFacts(facts);
      }
    };
    void run();

    return () => {
      cancelled = true;
    };
  }, [onFacts]);

  return (
    <div
      ref={containerRef}
      data-testid="jxcel-port-probe-surface"
      style={{ width: "100%", height: `${String(PORT_PROBE_HEIGHT_PX)}px`, minHeight: 0 }}
    />
  );
}

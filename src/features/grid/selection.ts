/**
 * 現在位置（現在のセル）と選択の規則（tasks.md 8.2。data-grid 要件 2.1、2.2、2.3、2.4、2.5）。
 *
 * # 何を担い、何を担わないか
 *
 * 担うのは 5 つである。
 *
 * 1. **現在位置の移動**（4 方向。要件 2.2）
 * 2. **範囲の選択**（矩形・行の全体・列の全体。要件 2.3）
 * 3. **数え上げ**（行数・列数・セル数。要件 2.5）
 * 4. **追随の判断**（現在位置が表示範囲の外へ出たか。要件 2.4）
 * 5. **打鍵から選択への対応**（どの指示をこの module が引き受けるか）
 *
 * 担わないのは**描くこと**である。現在位置を他のセルと区別して示すのは移植口の実装（Glide の
 * 制御選択と焦点の環）であり、スクロールを起こすのは [`RendererHandle.scrollTo`] である。
 * 本 module は**値だけを組み替える純粋な関数**であり、したがって canvas も DOM も要さず、
 * `vitest`（`environment: "node"`）がそのまま検査できる（`selection.test.ts`）。
 *
 * # 現在位置と選択はどちらが持つか（**8.2 の決定**）
 *
 * **画面が持ち、移植口へ下ろす。**7.1 は逆（実装が持ち、外へ報せる一方通行）と定めていたが、
 * 要件が 4 つ重なるとその向きでは成り立たない。
 *
 * | 何が要るか | 実装が持つ場合 | 画面が持つ場合 |
 * |---|---|---|
 * | 移動の意味論（要件 2.2 の隣接と端の扱い） | **ライブラリの打鍵の扱いが仕様になる**（規約として書き留められず、実装を差し替えると変わる） | 本 module が決める。移植口を差し替えても残る |
 * | 数え上げ（要件 2.5）と 2.6 の対象 | 正規化した矩形から現在位置を推し量ることになる（**右下から左上へ引いた選択では錨が失われる**） | 組（現在位置と矩形）をそのまま持つ |
 * | 追随（要件 2.4） | 追随は画面がスクロールを起こすので、画面が現在位置を知る必要がある | 同じ値を見る |
 * | 9.8 の移動（違反の位置へ現在位置を移す。4.4） | **外から現在位置を指定する口が無い** | `handle.setSelection` で下ろす |
 *
 * とくに 1 つ目が効く。移植口を挟んだ理由は**上流が止まっても差し替えられること**であり
 * （`glideAdapter.tsx` の module doc）、製品の要件（2.2 の端の扱い）がライブラリの打鍵処理の中に
 * あると、差し替えのたびに要件を写し直すことになる。そこで 8.2 は、方向の指示を**ライブラリから
 * 取り戻した** — `glideAdapter.tsx` が Glide の移動の束縛を切り、画面が下の規則で扱う。
 *
 * **食い違い（画面の写しと描かれたもののずれ）は構造で防ぐ。**選択の値は 1 つしか無く
 * （表を描く状態の `selection`）、その同じ値が数え上げの表示と `handle.setSelection` の両方へ
 * 渡る。実装は自分では選択を変えず、ポインタ由来の変化だけを報せる（`RendererSpec` の docs）。
 *
 * # 端の扱い（**要件 2.2 の「隣接するセル」をどう解釈したか**）
 *
 * 行の端・列の端・シートの端で**止まる**（巻き戻さない）。理由は 2 つある。
 *
 *   - 巻き戻すと、**隣接しないセルへ動く**。最終列で右を押したときに次の行の先頭へ移るのは
 *     要件 2.2 の「隣接するセル」ではない（表計算の矢印キーもそう振る舞わない）。
 *   - 巻き戻しは**端に着いたことが分からない**まま動き続ける。止まれば、利用者は端に居ることを
 *     位置の提示（要件 1.3）から読める。
 *
 * 先頭・末尾への直接の移動（要件 1.4）と 1 ページぶんの移動は**本 module の担当ではない**
 * （移植口の実装が持つ束縛がそのまま働く。`glideAdapter.tsx` の `keybindings`）。
 *
 * # 打鍵の割り当て（**どこで扱うか**）
 *
 * | 打鍵 | 何をするか | 誰が扱うか |
 * |---|---|---|
 * | ↑ ↓ ← → | 現在位置を隣へ移す（範囲は 1 つへ畳まれる） | 本 module（[`selectionForKey`]） |
 * | shift + ↑ ↓ ← → | 現在位置を隣へ移し、錨から現在位置までの矩形にする | 本 module |
 * | shift + 空白 | 現在位置の行の全体 | 本 module |
 * | ctrl（または meta）+ 空白 | 現在位置の列の全体 | 本 module |
 * | primary + ↑ ↓ ← → / Home / End / PageUp / PageDown | 端・ページへの移動 | **Glide の既定**（譲る） |
 * | Tab / shift + Tab | 右・左へ移す | **Glide の既定**（譲る。器の焦点の移動を壊さないため） |
 * | ctrl + A（行と列の全体） | 表の全体 | **Glide の既定**（譲る） |
 *
 * 扱う場所は**画面の表の枠（`GridSurface` の器）の打鍵の受け口**である。画面は器に自分の
 * 打鍵の受け口を付け、扱うと決めた打鍵だけを `preventDefault` する（ブラウザの走査を止める）。
 * 扱わない打鍵は**そのまま流す**ので、Glide の束縛が生き続ける。
 *
 * # 打鍵の失敗は画面内の状態である
 *
 * `ScreenBoundary` は**イベントハンドラの例外を捕まえない**（`src/shell/ScreenBoundary.tsx`）。
 * したがって本 module の関数は**全域である**: 範囲の外の入力でも例外を投げず、値を組み替えて
 * 返す（整数でない座標・負の数・範囲を越えた位置は、範囲へ寄せる）。
 */
import type { CellPosition, RendererSelection, VisibleSpan } from "./renderer/port";

/** 表の大きさ（現在位置を寄せる先）。**可視行の数と列の本数**である。 */
export interface SelectionBounds {
  readonly rowCount: number;
  readonly columnCount: number;
}

/** 方向の指示（要件 2.2）。 */
export type MoveDirection = "up" | "down" | "left" | "right";

/** 選択されている範囲の数え上げ（要件 2.5）。 */
export interface SelectionCounts {
  readonly rows: number;
  readonly columns: number;
  readonly cells: number;
}

/**
 * 打鍵のうち、本 module が判断に使う部分だけ。**DOM の `KeyboardEvent` をそのまま受け取れる**
 * （React の合成イベントもこの形を満たす）ので、画面は写しを作らずに渡せる。
 */
export interface KeyStroke {
  readonly key: string;
  readonly shiftKey: boolean;
  readonly altKey: boolean;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
}

/** 現在位置が居る所から 1 つ動く量。 */
const STEP: Readonly<Record<MoveDirection, CellPosition>> = {
  up: { row: -1, column: 0 },
  down: { row: 1, column: 0 },
  left: { row: 0, column: -1 },
  right: { row: 0, column: 1 },
};

/** 矢印の名前と方向の対応。 */
const ARROWS: Readonly<Record<string, MoveDirection>> = {
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
};

/** 行の全体・列の全体の打鍵が使う空白の名前（`KeyboardEvent.key` の綴りである）。 */
const SPACE_KEY = " ";

/**
 * 座標を 0 以上 `count` 未満へ寄せる。**整数でない値も負の値もここで均す**（全域にするため）。
 * `count` が 0 以下（そもそも表が無い）のときは 0 を返す — 範囲の外の選択を作らない。
 */
function within(value: number, count: number): number {
  if (count <= 0) {
    return 0;
  }
  if (!Number.isInteger(value)) {
    return 0;
  }
  return Math.min(Math.max(value, 0), count - 1);
}

/** その位置を 1 つ動かし、表の中へ寄せる（**端では止まる**。上の module doc）。 */
function step(position: CellPosition, direction: MoveDirection, bounds: SelectionBounds): CellPosition {
  const delta = STEP[direction];
  return {
    row: within(position.row + delta.row, bounds.rowCount),
    column: within(position.column + delta.column, bounds.columnCount),
  };
}

/**
 * 範囲の中で、現在位置の**対角の角**。範囲を広げるときの錨である。
 *
 * 現在位置が範囲の左上にあれば右下、右下にあれば左上が錨になる（現在位置が辺の途中にある場合は
 * 左下・右上のいずれかへ寄せる — **決定的であればよく、利用者には「反対側の角」に見える**）。
 */
function anchorOf(selection: RendererSelection): CellPosition {
  const { current, range } = selection;
  return {
    row: current.row === range.start.row ? range.end.row : range.start.row,
    column: current.column === range.start.column ? range.end.column : range.start.column,
  };
}

/**
 * その位置へ現在位置を移す（**範囲は 1 セルへ畳む**）。要件 4.4 の「違反の位置へ現在位置を
 * 移動する」と、表を描き始めるときの初期値（[`initialSelection`]）がこれを使う。
 *
 * 畳むのは、**位置を指定する指示に範囲が無い**ためである。直前の範囲を持ち越すと、画面に
 * 出ている数え上げ（要件 2.5）と、確定した選択を対象にする操作（要件 2.6）が「利用者が
 * 指したつもりのない範囲」を指すことになる。
 */
export function selectionAt(position: CellPosition): RendererSelection {
  return { current: position, range: { start: position, end: position } };
}

/**
 * 表を描き始めるときの選択: 先頭のセル 1 つ。**要件 2.1 の「現在位置となるセルを 1 つ持つ」の
 * 初期値である**（表を描く間、現在位置が 1 つも無い瞬間を作らない）。
 */
export function initialSelection(): RendererSelection {
  return selectionAt({ row: 0, column: 0 });
}

/**
 * 方向の指示で現在位置を隣接するセルへ移す（要件 2.2）。**範囲は現在位置 1 つへ畳まれる**
 * （利用者が 1 つのセルを指したのである）。端では止まる（module doc の「端の扱い」）。
 */
export function moveCurrent(
  selection: RendererSelection,
  direction: MoveDirection,
  bounds: SelectionBounds,
): RendererSelection {
  const current = step(selection.current, direction, bounds);
  return { current, range: { start: current, end: current } };
}

/**
 * 範囲を広げる（要件 2.3 の 1 つ目）。現在位置を 1 つ動かし、**錨（動かない角）から現在位置まで**を
 * 矩形にする。逆向きへ広げれば範囲は縮み、錨を越えれば向きが反転する（表計算と同じである）。
 */
export function extendSelection(
  selection: RendererSelection,
  direction: MoveDirection,
  bounds: SelectionBounds,
): RendererSelection {
  const current = step(selection.current, direction, bounds);
  const anchor = anchorOf(selection);
  return {
    current,
    range: {
      start: {
        row: Math.min(anchor.row, current.row),
        column: Math.min(anchor.column, current.column),
      },
      end: {
        row: Math.max(anchor.row, current.row),
        column: Math.max(anchor.column, current.column),
      },
    },
  };
}

/**
 * 現在位置の行の全体を選ぶ（要件 2.3 の 2 つ目）。**現在位置は動かさない** — 利用者が居た
 * セルは行の中にあり、要件 2.1 の「現在位置は 1 つ」もそのまま保たれる。
 */
export function selectWholeRow(
  selection: RendererSelection,
  bounds: SelectionBounds,
): RendererSelection {
  if (bounds.rowCount <= 0 || bounds.columnCount <= 0) {
    return selection;
  }
  const row = within(selection.current.row, bounds.rowCount);
  return {
    current: { row, column: within(selection.current.column, bounds.columnCount) },
    range: {
      start: { row, column: 0 },
      end: { row, column: bounds.columnCount - 1 },
    },
  };
}

/** 現在位置の列の全体を選ぶ（要件 2.3 の 3 つ目）。現在位置は動かさない（上の行版と同じ）。 */
export function selectWholeColumn(
  selection: RendererSelection,
  bounds: SelectionBounds,
): RendererSelection {
  if (bounds.rowCount <= 0 || bounds.columnCount <= 0) {
    return selection;
  }
  const column = within(selection.current.column, bounds.columnCount);
  return {
    current: { row: within(selection.current.row, bounds.rowCount), column },
    range: {
      start: { row: 0, column },
      end: { row: bounds.rowCount - 1, column },
    },
  };
}

/**
 * 選択の行数・列数・セル数（要件 2.5）。**両端を含む**（1 つのセルは 1 行 × 1 列 = 1 セル）。
 * セル数はその積である（穴の無い矩形なので、行数 × 列数が実際のセル数に一致する）。
 */
export function selectionCounts(selection: RendererSelection): SelectionCounts {
  const rows = Math.max(0, selection.range.end.row - selection.range.start.row + 1);
  const columns = Math.max(0, selection.range.end.column - selection.range.start.column + 1);
  return { rows, columns, cells: rows * columns };
}

/**
 * 追随の宛先（要件 2.4）: **現在位置が可視の区間の外へ出ていれば、その位置そのもの**。
 * 見えていれば `null`（表示範囲を動かさない）。
 *
 * 区間は**半開**である（[`RowSpan`] と同じ規約）ので、`start + count - 1` の行は見えている。
 * 可視の区間を知らないうち（`null`）と、区間が空のとき（まだ何も見えていない）は `null` を返す
 * — **何も見えていないのに走査を起こさない**。
 *
 * 軸は選ばない。`scrollTo` は与えられた位置が見えるところまで動かすので、宛先は現在位置である。
 */
export function followTarget(
  span: VisibleSpan | null,
  selection: RendererSelection,
): CellPosition | null {
  if (span === null) {
    return null;
  }
  const { rows, columns } = span;
  if (rows.count <= 0 || columns.count <= 0) {
    return null;
  }
  const { row, column } = selection.current;
  const visibleRow = row >= rows.start && row < rows.start + rows.count;
  const visibleColumn = column >= columns.start && column < columns.start + columns.count;
  return visibleRow && visibleColumn ? null : selection.current;
}

/**
 * 打鍵を選択へ写す。**引き受けない打鍵は `null`**（呼び出し側は何もせず、その打鍵を他の束縛へ
 * 流す）。どの打鍵を引き受けるかは module doc の表がすべてである。
 *
 * 修飾キーの付いた矢印を引き受けないのは、**primary + 矢印が端への移動（Glide の既定）だから
 * である**。ここで引き受けて飲むと、その束縛が黙って死ぬ。
 */
export function selectionForKey(
  stroke: KeyStroke,
  selection: RendererSelection,
  bounds: SelectionBounds,
): RendererSelection | null {
  if (stroke.key === SPACE_KEY) {
    if (stroke.shiftKey && !stroke.ctrlKey && !stroke.metaKey && !stroke.altKey) {
      return selectWholeRow(selection, bounds);
    }
    if ((stroke.ctrlKey || stroke.metaKey) && !stroke.shiftKey && !stroke.altKey) {
      return selectWholeColumn(selection, bounds);
    }
    return null;
  }

  const direction = ARROWS[stroke.key];
  if (direction === undefined) {
    return null;
  }
  if (stroke.altKey || stroke.ctrlKey || stroke.metaKey) {
    return null;
  }
  return stroke.shiftKey
    ? extendSelection(selection, direction, bounds)
    : moveCurrent(selection, direction, bounds);
}

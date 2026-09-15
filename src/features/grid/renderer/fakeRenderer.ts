/**
 * 偽の `GridRendererPort` の実装 2 つ（tasks.md 7.1）。
 *
 * **明らかに本物の描画器ではない。**「描く」というのは移植口の `getCell` を引くことであり、
 * 器（`HTMLElement`）にも canvas にも触れず、塗りも測りもしない。引いた結果は使わない
 * （使わないことは、`getCell` が例外を投げないことの観測には十分である — 投げればその場で落ちる）。
 * 残る仕事は 2 つだけで、どちらも移植口の契約に属する。
 *
 *   1. **描画の引き方**（いつ・どれだけ `getCell` を引くか）
 *   2. **知らせの出し方**（利用者の操作を `RendererSpec` の callback へ、いつ・どの順で渡すか）
 *
 * この 2 つを 2 つの実装で**わざと違えて**ある。そのうえで外へ出る呼び出しの並びが同じである
 * ことを、`port.test.ts` が確かめる（タスクの文言の「偽の実装に差し替えても呼び出しの並びが
 * 変わらないことを示す」）。
 *
 * | | 窓をまとめて引く実装 | 必要になった行だけ引く実装 |
 * |---|---|---|
 * | マウントの時の引き | 可視の窓（24 行 × 全列）を引く | 何も引かない |
 * | 操作の時の引き | 動いた先の行・区間を引く | 選択の範囲・起動した行・動いた先の行・区間を引く |
 * | 知らせの時機 | その場で渡す | 次の微小タスクまで遅らせる |
 *
 * **どちらの実装も、`RendererHandle` の 3 つの口（`scrollTo` / `invalidate` / `destroy`）を
 * 持つ。**`destroy` の後は移植口を使えない（`mountedSpec` が投げる）— 実装の誤りをテストの側で
 * 気づけるようにするためである。
 *
 * # 7.2（Glide の写し）への申し送り
 *
 * 本物の実装はこの file の 2 つを置き換えるのではなく**並ぶ**。`port.test.ts` の比較表に 1 行
 * 足せば、実物が同じ並びを保つことの実測になる。そのとき必要になるのは、実物の通知
 * （選択・起動・列幅・列の移動・複製・貼り付け）へ `RendererEventSource` をかぶせた薄い層であり、
 * `interactionDriver.ts` のヘッダにその旨を書いてある。
 */
import type { DrivableRenderer } from "./interactionDriver";
import type { RendererSpec, RowOrdinal } from "./port";

/** 窓をまとめて引く実装が、マウントの時に一度に引く可視の行数。 */
const EAGER_WINDOW_ROWS = 24;

/**
 * マウントされていない（または破棄された）移植口を使ったときに投げる。**実装の誤りを
 * 黙って隠さないためである** — 破棄の後に知らせを出し続ける実装は、画面が消えているのに
 * 呼び出し側を叩くことになる。
 */
function mountedSpec(spec: RendererSpec | null, operation: string): RendererSpec {
  if (spec === null) {
    throw new Error(`移植口がマウントされていない（または破棄された）のに ${operation} が起きた`);
  }
  return spec;
}

/**
 * **窓をまとめて引く**偽の実装。マウントの時に可視の窓を丸ごと引いてから、知らせはその場で渡す。
 */
export function createEagerFakeRenderer(): DrivableRenderer {
  let spec: RendererSpec | null = null;
  let clipboard: string | null = null;

  const draw = (start: RowOrdinal, count: number): void => {
    if (spec === null) return;
    for (let row = start; row < start + count; row += 1) {
      for (let column = 0; column < spec.columns.length; column += 1) {
        spec.getCell({ row, column });
      }
    }
  };

  return {
    mount(_container, initial) {
      // 器は使わない（本物の描画器ではないので描画面を持たない）。**触れないことは前提ではなく
      // 確かめられる** — 駆動器はどの属性を読んでも投げる代役を渡す（`interactionDriver.ts`）。
      spec = initial;
      draw(0, EAGER_WINDOW_ROWS);
      return {
        scrollTo(position) {
          draw(position.row, 1);
        },
        invalidate(span) {
          draw(span.start, span.count);
        },
        destroy() {
          spec = null;
        },
      };
    },
    emitSelectionChange(range) {
      mountedSpec(spec, "emitSelectionChange").onSelectionChange(range);
      return Promise.resolve();
    },
    emitActivateEditor(position) {
      mountedSpec(spec, "emitActivateEditor").onActivateEditor(position);
      return Promise.resolve();
    },
    emitColumnResize(column, width) {
      mountedSpec(spec, "emitColumnResize").onColumnResize(column, width);
      return Promise.resolve();
    },
    emitColumnMove(from, to) {
      mountedSpec(spec, "emitColumnMove").onColumnMove(from, to);
      return Promise.resolve();
    },
    async emitCopy(range) {
      clipboard = await mountedSpec(spec, "emitCopy").onCopy(range);
    },
    async emitPaste(anchor, text) {
      await mountedSpec(spec, "emitPaste").onPaste(anchor, text);
    },
    get clipboard() {
      return clipboard;
    },
  };
}

/**
 * **必要になった行だけ引く**偽の実装。マウントの時は何も引かず、操作に応じてその範囲だけを引き、
 * 知らせは `await Promise.resolve()`（次の微小タスク）まで遅らせる。**時機は実装の自由であり、
 * 契約ではない** — この実装でも並びが変わらないことが `port.test.ts` の比較で確かめられる。
 */
export function createLazyFakeRenderer(): DrivableRenderer {
  let spec: RendererSpec | null = null;
  let clipboard: string | null = null;

  const draw = (start: RowOrdinal, count: number): void => {
    if (spec === null) return;
    for (let row = start; row < start + count; row += 1) {
      for (let column = 0; column < spec.columns.length; column += 1) {
        spec.getCell({ row, column });
      }
    }
  };

  return {
    mount(_container, initial) {
      // 器は使わない（理由は窓をまとめて引く実装と同じ）。
      spec = initial;
      return {
        scrollTo(position) {
          draw(position.row, 1);
        },
        invalidate(span) {
          draw(span.start, span.count);
        },
        destroy() {
          spec = null;
        },
      };
    },
    async emitSelectionChange(range) {
      await Promise.resolve();
      if (range !== null) draw(range.start.row, range.end.row - range.start.row + 1);
      mountedSpec(spec, "emitSelectionChange").onSelectionChange(range);
    },
    async emitActivateEditor(position) {
      await Promise.resolve();
      draw(position.row, 1);
      mountedSpec(spec, "emitActivateEditor").onActivateEditor(position);
    },
    async emitColumnResize(column, width) {
      await Promise.resolve();
      mountedSpec(spec, "emitColumnResize").onColumnResize(column, width);
    },
    async emitColumnMove(from, to) {
      await Promise.resolve();
      mountedSpec(spec, "emitColumnMove").onColumnMove(from, to);
    },
    async emitCopy(range) {
      await Promise.resolve();
      clipboard = await mountedSpec(spec, "emitCopy").onCopy(range);
    },
    async emitPaste(anchor, text) {
      await Promise.resolve();
      await mountedSpec(spec, "emitPaste").onPaste(anchor, text);
    },
    get clipboard() {
      return clipboard;
    },
  };
}

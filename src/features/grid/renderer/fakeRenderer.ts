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
 * **どちらの実装も、`RendererHandle` の 5 つの口（`setSelection` / `scrollTo` / `invalidate` /
 * `copySelection` / `destroy`）を持つ。**`copySelection` は 8.7 が足した**打鍵とメニューの唯一の
 * 入口**であり（要件 7.8）、どちらの実装も「範囲は選択から決め、テキストは仕様に作らせる」と
 * いう同じ意味論で答える。`destroy` の後は移植口を使えない（`mountedSpec` が投げる）— 実装の
 * 誤りをテストの側で気づけるようにするためである。2 つの実装は `setSelection` の扱いでも
 * わざと違う（窓をまとめて引く側はその場で描き、必要になった行だけ引く側は引かない —
 * **複製の対象を知るために選択を覚えるのはどちらも同じである**）。それでも外へ出る
 * 呼び出しの並びは同じである。
 *
 * # 7.2（Glide の写し）への申し送り
 *
 * 本物の実装はこの file の 2 つを置き換えるのではなく**並ぶ**。`port.test.ts` の比較表に 1 行
 * 足せば、実物が同じ並びを保つことの実測になる。そのとき必要になるのは、実物の通知
 * （選択・起動・列幅・列の移動・複製・貼り付け）へ `RendererEventSource` をかぶせた薄い層であり、
 * `interactionDriver.ts` のヘッダにその旨を書いてある。
 */
import type { DrivableRenderer } from "./interactionDriver";
import type { RendererSelection, RendererSpec, RowOrdinal } from "./port";

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
  // **いまの選択**（`copySelection` が範囲を決めるのに要る。要件 7.8）。マウントの時点の値から
  // 始まり、知らせ（利用者の操作）と指示（`setSelection`）の両方で更新する — 選択の所有者は
  // 呼び出し側だが、**複製の対象を決めるのは移植口**である（`./port` の
  // `RendererHandle.copySelection` の docs）。
  let selection: RendererSelection | null = null;

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
      selection = initial.selection;
      draw(0, EAGER_WINDOW_ROWS);
      return {
        setSelection(next) {
          // 下ろされた選択を覚える（複製の対象になる。上の `selection` の doc）。
          selection = next;
          // **与えられた選択を描く**（下ろされた指示が描くものになる）。範囲の行を引くだけで
          // あり、外へ報せ返さない（報せ返せば `port.test.ts` の並びの比較に余計な 1 つが載る）。
          if (next !== null) {
            draw(next.range.start.row, next.range.end.row - next.range.start.row + 1);
          }
        },
        scrollTo(position) {
          draw(position.row, 1);
        },
        invalidate(span) {
          draw(span.start, span.count);
        },
        async copySelection() {
          // **打鍵とメニューの唯一の入口**（本物と同じ意味論: 範囲は選択から決め、テキストは
          // 仕様が作り、クリップボードへ渡す）。
          const current = selection;
          if (current === null) {
            return;
          }
          clipboard = await mountedSpec(spec, "copySelection").onCopy(current.range);
        },
        destroy() {
          spec = null;
        },
      };
    },
    emitSelectionChange(selection) {
      mountedSpec(spec, "emitSelectionChange").onSelectionChange(selection);
      return Promise.resolve();
    },
    emitVisibleSpanChange(span) {
      mountedSpec(spec, "emitVisibleSpanChange").onVisibleSpanChange(span);
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
  /**
   * **いまの選択**（`copySelection` が範囲を決めるのに要る。要件 7.8）。**この実装は描かないが、
   * 覚えるのは要る** — 複製の入口は範囲を引数に取らないので、対象を決められるのは移植口だけ
   * である（`./port` の `RendererHandle.copySelection` の docs）。**覚えることと描くことの
   * 違いが本実装の「必要になった行だけ引く」という性格であり、そこは変えていない。**
   */
  let selection: RendererSelection | null = null;

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
      selection = initial.selection;
      return {
        setSelection(next) {
          // **この実装は引かない**（必要になった行だけを引く作りである）。選択は覚えるが
          // （複製の対象）、報せ返しはしない — 選択の所有者は呼び出し側である。
          selection = next;
        },
        scrollTo(position) {
          draw(position.row, 1);
        },
        invalidate(span) {
          draw(span.start, span.count);
        },
        async copySelection() {
          await Promise.resolve();
          // **打鍵とメニューの唯一の入口**（意味論は窓をまとめて引く実装と同じである）。
          const current = selection;
          if (current === null) {
            return;
          }
          clipboard = await mountedSpec(spec, "copySelection").onCopy(current.range);
        },
        destroy() {
          spec = null;
        },
      };
    },
    async emitSelectionChange(selection) {
      await Promise.resolve();
      if (selection !== null) {
        draw(selection.range.start.row, selection.range.end.row - selection.range.start.row + 1);
      }
      mountedSpec(spec, "emitSelectionChange").onSelectionChange(selection);
    },
    async emitVisibleSpanChange(span) {
      await Promise.resolve();
      mountedSpec(spec, "emitVisibleSpanChange").onVisibleSpanChange(span);
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
    async emitPaste(anchor, text) {
      await Promise.resolve();
      await mountedSpec(spec, "emitPaste").onPaste(anchor, text);
    },
    get clipboard() {
      return clipboard;
    },
  };
}

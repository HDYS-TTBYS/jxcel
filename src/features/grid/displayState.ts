/**
 * 列幅と表示上の列順だけを持つ画面側の状態（tasks.md 7.5。design.md「表示状態（ドキュメントに
 * 保存されない）」。data-grid 要件 8.1、8.2。割り方の根拠と要件 8.5 は下の「2 つの欄」と
 * 「層の鎖」）。
 *
 * # 何を担い、何を担わないか
 *
 * 担うのは 2 つだけである。
 *
 * 1. **列ごとの幅**（要件 8.1）
 * 2. **描画のときにだけ効く列の並び順**（要件 8.2）
 *
 * **窓の中身を 1 つも変えない。**したがって本 module は境界へもドキュメントへも届かない
 * （下の「層の鎖」）。並べ替え・絞り込み・展開が Rust 側にあるのは、それらが**窓が運ぶ行と
 * 列を変える**ためであり、列幅と表示上の列順は窓の内容を一切変えない — 境界を越える理由が
 * 無い（design.md「割り方の根拠」）。同じ状態を 2 か所が持たないための線引きである。
 * `ViewState` からも `DisplayState` からも `Document` へ到達する経路は存在せず、要件 8.5 は
 * この構造で満たされる（design.md の同節）。
 *
 * # 2 つの欄は別の添字の空間に住む（**取り違えが最も起きやすい所である**）
 *
 * design.md の `DisplayState` は 2 つの欄を持ち、**どちらも `number` である**。型検査では
 * 区別できない（`port.ts` が `RowOrdinal` / `ColumnIndex` を別名に留めたのと同じ事情）ので、
 * ここに書く。
 *
 * | 欄 | 添字の空間 | 意味 |
 * |---|---|---|
 * | `columnOrder` | **表示位置 → 文書の列の添字** | `columnOrder[表示位置]` が、その位置に描く列 |
 * | `columnWidths` | **文書の列の添字 → 幅** | 鍵は列そのものであり、位置ではない |
 *
 * すなわち:
 *
 * - `columnOrder` は**左から順に**並んだ列の並びである。`RendererSpec.columns` と同じ向きで
 *   あり（`port.ts`「`RendererSpec.columns` の順がそのまま画面の左からの順」）、
 *   **逆写像（その列はどの位置に描かれるか）ではない。**並びが巡回のとき両者は別の並びになる
 *   — 例: `[2, 0, 1]` の逆は `[1, 2, 0]` であり、向きを取り違えると**別の列を描く**。
 * - `columnWidths` の鍵は**文書の列の添字**である。したがって**列の並びを変えても幅は動かない**
 *   — 幅は列そのものに付いており、列が左へ運ばれても同じ幅で描かれる。逆に「表示位置 1 に幅を
 *   設定する」は、**そのとき位置 1 にいる列**の幅を変えるので、並びを変えた後では別の列を指す。
 *
 * # 操作の口は描画層の知らせと同じ空間を取る
 *
 * `RendererSpec.onColumnResize` / `onColumnMove` は**表示位置**で報告する（`port.ts`。
 * `glideAdapter.tsx` は Glide の添字をそのまま位置として渡す）。本 module の口も表示位置を取り、
 * **文書の添字への翻訳は本 module の中で 1 回だけ**行う — 画面が毎回 `columnOrder` を引くと、
 * 同じ翻訳が 2 か所に現れ、向きの取り違えが片方だけに起きる。
 *
 * # 宣言された列数はセッションの間、変わらない
 *
 * 列数は**作るときに 1 度だけ**受け取る（[`createDisplayState`]）。列の集合と並び順はスキーマが
 * 供給し（requirements.md Boundary Context）、**列の追加・削除・型の変更はスキーマ編集を所有する
 * 機能の仕事である**（本機能の外）。したがって `columnOrder` は**常に `0..列数-1` の置換**であり、
 * 長さが列数に足りない状態は作れない — [`DisplayStateStore.moveColumn`] は置換を置換へ写すだけで、
 * **配列の長さも要素の集合も変えない**。列数が変わったら状態を作り直す（`windowCache.clear` の
 * 事情「列の構成が変わったとき」と同じ扱いである）。
 *
 * # 範囲の外の入力は無視する（状態を変えない。投げない）
 *
 * `on*` は**知らせであって命令ではない**（`port.ts`）。描画層が宣言の外の位置を報せてきたとき
 * （配線の誤り、あるいは列数が変わった後の古い知らせ）に本 module ができる正しいことは、
 * **その知らせを状態へ入れない**ことである。投げない — 知らせは描画の途中に届くので、例外は
 * 画面を巻き込む（`port.ts` が `getCell` に「決して投げない」を課したのと同じ理由である）。
 *
 * **幅の値そのものは判定しない。**画素は描画層の単位であり、どの値が正しいかを決める根拠が
 * 本 module に無い（要件 8.1 が求めているのは変更できることである）。与えられた数をそのまま
 * 覚え、そのまま返す。
 *
 * # 層の鎖（どこまでが本 module か）
 *
 * `renderer/port.ts` の**型だけ**を取り込む（`verbatimModuleSyntax` により `import type` が要る。
 * 型はバンドル時に消える）。**値を 1 つも取り込まない**ので、本 module から実行時の経路が生える
 * 余地が無い — `windowCache.ts` も `ipc/client.ts` も `Document` も届かない。この非到達は
 * `displayState.test.ts` が源と取り込みの閉包を走査して固定する（綴りを 1 つ足せば落ちる）。
 */
// **型だけの取り込みである**（実行時の依存は 0 件。上の「層の鎖」）。
import type { RenderColumn } from "./renderer/port";

/**
 * 列幅と表示上の列順（design.md「表示状態」の interface そのまま。要件 8.1、8.2）。
 *
 * **画面の状態であり、ドキュメントにも境界にも保存されない**（保存される列の順序を変えないこと
 * が要件 8.5 である）。2 つの欄の添字の空間は module docs の表を参照 — とくに `columnOrder` の
 * 向き（表示位置 → 文書の列の添字）は、この型だけからは読み取れない。
 */
export interface DisplayState {
  /** 要件 8.1。**鍵は文書の列の添字**であり、表示位置ではない（module docs の表）。 */
  readonly columnWidths: ReadonlyMap<number, number>;
  /** 要件 8.2。**表示位置 → 文書の列の添字**（左から順）。描画時の並べ替えのみに効く。 */
  readonly columnOrder: readonly number[];
}

/**
 * 幅を設定していない列の幅（ピクセル）。
 *
 * 描き手は**すべての列に幅を要る**（`RenderColumn.width` は必須である）ため、幅を設定されて
 * いない列にも 1 つの数を与えなければならない。スキーマが列の幅を宣言する日が来たら、その値は
 * [`DisplayStateOptions.defaultWidth`] を通して入る（本 module はスキーマを持たない）。
 */
export const DEFAULT_COLUMN_WIDTH = 120;

/** 表示状態を組み立てる指定。 */
export interface DisplayStateOptions {
  /**
   * 宣言された列数（生成物の `ColumnDescriptor` の数。**スキーマが供給する**）。
   *
   * 0 以上の整数でなければ 0 として扱う（`windowCache` の「整数でない指定は 0 として扱う」と
   * 同じ規律。列が 0 本のシートも正当である）。
   */
  readonly columnCount: number;
  /** 幅を設定していない列の幅（既定は [`DEFAULT_COLUMN_WIDTH`]）。 */
  readonly defaultWidth?: number;
}

/**
 * 表示状態と、それを変える口（tasks.md 7.5）。
 *
 * [`DisplayState`] を**そのまま満たす**（2 つの欄を読み取り専用の欄として持つ）ので、状態だけを
 * 要る側（描画層へ渡す列を組む側、要件 8.2 の並びを見る側）へはこの値をそのまま渡せる。
 */
export interface DisplayStateStore extends DisplayState {
  /** 宣言された列数（[`DisplayState.columnOrder`] は常にこの長さを持つ）。 */
  readonly columnCount: number;
  /**
   * 列の幅を変更する（要件 8.1。`RendererSpec.onColumnResize` の知らせを受ける口）。
   *
   * `displayPosition` は**表示位置**である（描画層の知らせと同じ空間）。幅は**そのときその位置に
   * いる列**（文書の列の添字 `columnOrder[displayPosition]`）に付く。範囲の外の位置は無視し、
   * 状態を変えない（module docs「範囲の外の入力」）。値は判定せずそのまま覚える。
   */
  setColumnWidth(displayPosition: number, width: number): void;
  /**
   * 列を表示の並びの中で運ぶ（要件 8.2。`RendererSpec.onColumnMove` の知らせを受ける口）。
   *
   * `from` と `to` は**表示順の位置**どうしである（描画層の知らせと同じ空間。Glide の
   * `onColumnMoved(startIndex, endIndex)` と同じ意味である）。位置 `from` に描かれていた列が、
   * 位置 `to` へ運ばれ、間の列は詰める。恒等（`from === to`）と範囲の外は無視する。
   * **幅は動かない** — 幅の鍵は文書の列の添字であり、位置ではない（module docs の表）。
   */
  moveColumn(from: number, to: number): void;
  /**
   * 描画層へ渡す列の並びを組む（表示順。左から順）。
   *
   * `titles` は**文書の列の添字で引く見出し**である（生成物の `ColumnDescriptor.name`。画面が
   * 持つ）。本 module が列の名を知らないのは、見出しがスキーマのものであり、ここに写しを持つと
   * 名前の源が 2 つになるためである。同じ理由で、幅の源も 1 つである（[`DisplayState.columnWidths`]。
   * 未設定の列は [`DisplayStateOptions.defaultWidth`] で描く）。
   *
   * 見出しを与えられていない列は空の見出しで返す（描画の途中に例外を出さない。`port.ts` が
   * `getCell` に課したのと同じ理由である）。
   */
  renderColumns(titles: readonly string[]): readonly RenderColumn[];
}

/**
 * 表示状態を組み立てる（tasks.md 7.5）。
 *
 * 初期の状態は**文書の並びそのもの**（`columnOrder[i] === i`）であり、幅は 1 つも設定されて
 * いない — 画面が開いた直後は、宣言された列がスキーマの順にそのまま現れる
 * （`ViewSpec` の空の指定が「宣言の列がそのまま現れる」のと同じ規約である）。
 *
 * 変化のたびに**新しい配列と新しい `Map` を作る**（`readonly` の欄は読み手の側の写しであり、
 * 手元の値を書き換えると、前に読んだ値が読み手の下で変わる）。変更は利用者の操作のたびに
 * 1 回であり、引き（描画のたび）と違って頻度が低い。
 */
export function createDisplayState(options: DisplayStateOptions): DisplayStateStore {
  // 列数は 0 以上の整数へ均す（列が 0 本のシートも正当であり、整数でない指定は 0 として扱う
  // — `windowCache` の「整数でない指定は 0 として扱う」と同じ規律である）。
  const columnCount = Number.isFinite(options.columnCount) ? Math.max(0, Math.floor(options.columnCount)) : 0;
  const defaultWidth = options.defaultWidth ?? DEFAULT_COLUMN_WIDTH;
  /** 表示位置 → 文書の列の添字（左から順）。 */
  let order: number[] = Array.from({ length: columnCount }, (_unused, index) => index);
  /** 文書の列の添字 → 幅（**設定された列だけ**を持つ。未設定は既定の幅である）。 */
  let widths = new Map<number, number>();

  /** その位置が表示の並びの中にあるか（整数でなければ無い）。 */
  const inRange = (position: number): boolean =>
    Number.isInteger(position) && position >= 0 && position < columnCount;

  return {
    columnCount,
    get columnWidths(): ReadonlyMap<number, number> {
      return widths;
    },
    get columnOrder(): readonly number[] {
      return order;
    },
    setColumnWidth(displayPosition, width) {
      if (!inRange(displayPosition)) {
        // 宣言の外の列は幅を持てない。知らせを状態へ入れない（module docs「範囲の外の入力」）。
        return;
      }
      const column = order[displayPosition];
      if (column === undefined) {
        return;
      }
      const next = new Map(widths);
      next.set(column, width);
      widths = next;
    },
    moveColumn(from, to) {
      if (!inRange(from) || !inRange(to) || from === to) {
        // 恒等は並びを変えない（新しい配列も作らない）。範囲の外も同じである。
        return;
      }
      const next = order.slice();
      const [moved] = next.splice(from, 1);
      if (moved === undefined) {
        return;
      }
      next.splice(to, 0, moved);
      order = next;
    },
    renderColumns(titles) {
      return order.map((column) => ({
        title: titles[column] ?? "",
        width: widths.get(column) ?? defaultWidth,
      }));
    },
  };
}

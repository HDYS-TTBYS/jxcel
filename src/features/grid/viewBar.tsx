/**
 * 表示の操作の行（tasks.md 8.8。data-grid 要件 8.1、8.2、8.3、8.4、8.7）。
 *
 * **純粋な描画である**（状態も効果も持たない）。状態を持つのは `./GridScreen` の状態機械であり、
 * 判断は `./viewOps` が担う。したがって検査は状態を組んでこの成分を呼び、「何が DOM へ出るか」
 * と「要素の受け口がどの操作を組み立てるか」を読める（`viewBar.test.tsx`）。
 *
 * # 4 つの操作を 1 行に出す（**8.6 の行の操作と同じ形である**）
 *
 * | 操作 | 要件 | 出すもの |
 * |---|---|---|
 * | 列幅の変更 | 8.1 | 列ごとの**数の入力**（その位置に描かれる列の幅） |
 * | 表示上の列順の変更 | 8.2 | 列ごとの**左右への移動**（端では押しても動かない） |
 * | 並べ替え | 8.3 | 列ごとの**巡回の操作**（基準でない → 昇順 → 降順 → 基準でない） |
 * | 絞り込み | 8.4 | 列ごとの**条件の選択**と、条件が文字列を持つときの入力 |
 *
 * 加えて、**絞り込みで表示されていない行の数**（要件 8.7）を同じ行に出す。数は応答が運んだ
 * 数そのものであり、画面は数え直さない。
 *
 * # 名指すのは**文書の列**である（表示の位置ではない。要件 8.6 の危険）
 *
 * 並べ替えと絞り込みの対象は境界の型が**文書の列**と定めている（`GridSortKey.column` は
 * 行の値の並びに対する添字である）。表示上の列順を変えると 2 つは離れるので、本成分は
 * **表示順に並べる**（利用者が見る順である）が、送る値は `ColumnDescriptor.column` から作る。
 * 取り違えは利用者から見えない（どちらも同じ数だからである）ので、両方を属性に出す。
 *
 * # 幅は「いま位置に居る列」の幅である
 *
 * 幅の読み書きは `DisplayState` の口（`widthAt` / `setColumnWidth`）を通し、**表示位置**で
 * 行う — 幅の鍵は列そのものであり（7.5 の規則）、画面が `columnWidths` を直接引くと
 * 位置で引く誤りが起きる。
 *
 * # 配色
 *
 * 器が与えるカスタムプロパティ（`APPEARANCE_VARS`）だけを参照する（`GridScreen.tsx` と同じ
 * 契約。源の走査は**本 module にも当たる** — `GridScreen.test.ts` の走査が源の一覧に本 module を
 * 並べる）。
 */
import type { ChangeEvent, ReactElement } from "react";

import { APPEARANCE_VARS } from "../../shell/theme";
import type { ColumnDescriptor, GridViewSpec } from "../../ipc/bindings";
import type { DisplayState } from "./displayState";
import {
  filterModeOf,
  filterOf,
  filterSpecOf,
  hiddenRowsNotice,
  sortStateOf,
  type FilterMode,
  type ViewOperation,
} from "./viewOps";

/**
 * 表示の操作の行へ渡すもの。**値と、3 つの受け口だけである。**
 *
 * 判断（どの操作になるか）は本成分が `./viewOps` の関数で組み立て、**適用するのは画面**である
 * （8.6 の行の操作と同じ分担である — 部品は境界も状態も持たない）。
 */
export interface ViewBarProps {
  /**
   * **表示順**の構成（`./viewOps` の `drawnColumns` が組む）。
   *
   * 添字が表示位置であり、各要素の `column` が文書の列である。宣言の順ではない —
   * 表示上の列順を変えたときに、並べ替えと絞り込みの対象が**描かれている列**を指すためである。
   */
  readonly columns: readonly ColumnDescriptor[];
  /** 幅と並び（**読み取りだけである。変更は 2 つの受け口を通す**）。 */
  readonly display: DisplayState;
  /** いまの表示の指定（並べ替えと絞り込みの現在値）。 */
  readonly view: GridViewSpec;
  /** いま表示している行の数（応答が運んだ数）。 */
  readonly visibleRows: number;
  /** 絞り込みによって表示されていない行の数（要件 8.7。応答が運んだ数）。 */
  readonly hiddenRows: number;
  /** その**表示位置**の列の幅を変える（要件 8.1）。 */
  readonly onColumnWidth: (displayPosition: number, width: number) => void;
  /** その**表示位置**の列を、並びの中で隣へ運ぶ（要件 8.2）。 */
  readonly onColumnMove: (from: number, to: number) => void;
  /** 表示の操作を 1 つ適用する（要件 8.3、8.4）。 */
  readonly onView: (operation: ViewOperation) => void;
}

/** 行の枠（**表の上に出す**。8.4 のバーと 8.6 の行の操作と同じ位置である）。 */
const BAR_STYLE = {
  display: "flex",
  flexWrap: "wrap",
  alignItems: "center",
  gap: "0.5rem",
  padding: "0.4rem 0.75rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 列 1 本ぶんの操作（**縦に積む**。列の名前と操作が 1 つのまとまりになる）。 */
const COLUMN_STYLE = {
  display: "flex",
  alignItems: "center",
  gap: "0.25rem",
} as const;

/** 補助的な文字（行の数・列の名前）。 */
const TEXT_STYLE = { margin: 0, color: `var(${APPEARANCE_VARS.screenMuted})` } as const;

/** 操作（移動・並べ替え）。**枠線と文字に器の配色を使う。** */
const BUTTON_STYLE = {
  font: "inherit",
  fontSize: "0.8125rem",
  padding: "0.15rem 0.45rem",
  borderRadius: "0.25rem",
  cursor: "pointer",
  color: `var(${APPEARANCE_VARS.controlActiveText})`,
  backgroundColor: `var(${APPEARANCE_VARS.controlActiveBackground})`,
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/** 数と条件の入力（**幅は画素である**。ブラウザの数の入力を使う）。 */
const INPUT_STYLE = {
  font: "inherit",
  fontSize: "0.8125rem",
  width: "4.5rem",
  padding: "0.1rem 0.25rem",
} as const;

/** 絞り込みの種類の選択。 */
const SELECT_STYLE = { font: "inherit", fontSize: "0.8125rem", padding: "0.1rem 0.25rem" } as const;

/**
 * 絞り込みの選択肢（**綴りは境界の条件の 5 つ + 解除である**）。
 *
 * 並びは `./viewOps` の `FilterMode` の並びと揃えてある（選択肢を足すときは、条件を組む関数と
 * 往復の検査も同じ変更で足すこと）。
 */
const FILTER_MODES: readonly { readonly mode: FilterMode; readonly label: string }[] = [
  { mode: "none", label: "なし" },
  { mode: "contains", label: "含む" },
  { mode: "equals", label: "一致" },
  { mode: "empty", label: "値なし" },
  { mode: "notEmpty", label: "値あり" },
  { mode: "violating", label: "違反あり" },
];

/** 並べ替えの操作の文言（3 つの状態を潰さない）。 */
const SORT_LABELS: Readonly<Record<"none" | "ascending" | "descending", string>> = {
  none: "並べ替え: なし",
  ascending: "並べ替え: 昇順",
  descending: "並べ替え: 降順",
};

/**
 * 数の入力を、正の数へ読む（**読めなければ `null`**）。
 *
 * 空欄・非数・0 以下は幅として意味を持たない（描けない列の幅である）。**画面の状態は値を
 * 判定しない**（7.5 の規律）ので、判定するのは入力の入口であるここ 1 箇所である。
 */
function readWidth(text: string): number | null {
  const value = Number(text);
  return text.trim() !== "" && Number.isFinite(value) && value > 0 ? value : null;
}

/** 表示の操作の行。**4 つの操作と、隠れている行の数を出す。** */
export function ViewBar({
  columns,
  display,
  view,
  visibleRows,
  hiddenRows,
  onColumnWidth,
  onColumnMove,
  onView,
}: ViewBarProps): ReactElement {
  const hiddenNotice = hiddenRowsNotice({
    hiddenRows,
    filtersActive: view.filters.length > 0,
  });
  return (
    <div data-testid="jxcel-grid-view-bar" style={BAR_STYLE}>
      {/*
        表示している行の数と、絞り込みで隠れている行の数（要件 8.7）。**隠れている数は応答が
        運んだ数そのもの**である（画面は数え直さない）。絞り込みが効いていなければ隠れている行を
        名乗らない — 「0 行」と名乗ると、条件が効いているように読める。
      */}
      <span
        data-testid="jxcel-grid-row-visibility"
        data-visible-rows={visibleRows}
        data-hidden-rows={hiddenRows}
        style={TEXT_STYLE}
      >
        {`表示 ${String(visibleRows)} 行`}
      </span>
      {hiddenNotice === null ? null : (
        <span data-testid="jxcel-grid-hidden-rows" data-hidden-rows={hiddenRows} style={TEXT_STYLE}>
          {hiddenNotice}
        </span>
      )}
      {columns.map((column, position) => {
        const width = display.widthAt(position);
        const sorted = sortStateOf(view, column.column);
        const filter = filterOf(view, column.column);
        const mode = filterModeOf(filter);
        const filterText = filter !== null && "text" in filter ? filter.text : "";
        const direction = sorted === null ? "none" : sorted ? "descending" : "ascending";
        return (
          <div
            key={`${String(column.column)}:${column.path.map((segment) => (segment.segment === "Field" ? segment.name : `[${String(segment.position)}]`)).join(".")}`}
            data-testid="jxcel-grid-view-column"
            data-display-position={position}
            data-document-column={column.column}
            style={COLUMN_STYLE}
          >
            <span style={TEXT_STYLE}>{column.name}</span>
            {/*
              表示上の列順の変更（要件 8.2）。**端では押しても何も起きない**（範囲の外の移動は
              状態を変えない — 7.5 の規律であり、押下を無効として示す）。
            */}
            <button
              type="button"
              data-testid="jxcel-grid-column-left"
              disabled={position === 0}
              onClick={() => {
                if (position > 0) {
                  onColumnMove(position, position - 1);
                }
              }}
              style={BUTTON_STYLE}
            >
              ◀
            </button>
            <button
              type="button"
              data-testid="jxcel-grid-column-right"
              disabled={position === columns.length - 1}
              onClick={() => {
                if (position < columns.length - 1) {
                  onColumnMove(position, position + 1);
                }
              }}
              style={BUTTON_STYLE}
            >
              ▶
            </button>
            {/*
              列幅の変更（要件 8.1）。**その位置に描かれる列の幅**を出し、変更も**表示位置**で
              送る（幅の鍵は列であるが、知らせの空間は位置である。7.5 の表）。
            */}
            <label style={TEXT_STYLE}>
              幅
              <input
                type="number"
                min={1}
                data-testid="jxcel-grid-column-width"
                value={width === null ? "" : String(width)}
                onChange={(event: ChangeEvent<HTMLInputElement>) => {
                  const next = readWidth(event.target.value);
                  if (next !== null) {
                    onColumnWidth(position, next);
                  }
                }}
                style={INPUT_STYLE}
              />
            </label>
            {/*
              並べ替え（要件 8.3）。押すと巡回する（基準でない → 昇順 → 降順 → 基準でない）。
              **対象は文書の列**である（`column.column`）。
            */}
            <button
              type="button"
              data-testid="jxcel-grid-column-sort"
              data-sort-direction={direction}
              onClick={() => {
                onView({ kind: "sortCycle", column: column.column });
              }}
              style={BUTTON_STYLE}
            >
              {SORT_LABELS[direction]}
            </button>
            {/*
              絞り込み（要件 8.4）。条件の選択と、文字列を持つ条件の入力である。**条件は列ごとに
              1 件**であり、選択を「なし」へ戻すとその列の条件が外れる（他の列の条件は残る）。
            */}
            <select
              data-testid="jxcel-grid-column-filter"
              data-filter-mode={mode}
              value={mode}
              onChange={(event: ChangeEvent<HTMLSelectElement>) => {
                const chosen = event.target.value as FilterMode;
                onView({
                  kind: "filter",
                  column: column.column,
                  filter: filterSpecOf(chosen, column.column, filterText),
                });
              }}
              style={SELECT_STYLE}
            >
              {FILTER_MODES.map((option) => (
                <option key={option.mode} value={option.mode}>
                  {option.label}
                </option>
              ))}
            </select>
            {mode === "contains" || mode === "equals" ? (
              <input
                type="text"
                data-testid="jxcel-grid-column-filter-text"
                value={filterText}
                onChange={(event: ChangeEvent<HTMLInputElement>) => {
                  onView({
                    kind: "filter",
                    column: column.column,
                    filter: filterSpecOf(mode, column.column, event.target.value),
                  });
                }}
                style={INPUT_STYLE}
              />
            ) : null}
          </div>
        );
      })}
    </div>
  );
}

/**
 * 表示の操作（列幅・表示上の列順・並べ替え・絞り込み）の判断（tasks.md 8.8。data-grid 要件
 * 8.1、8.2、8.3、8.4、8.5、8.6、8.7）。
 *
 * 所有: `applyViewOperation` ほか（design.md「Components and Interfaces → Frontend Layer」の
 * GridScreen が使う、表示の操作の判断）。
 *
 * # 4 つの操作と、その行き先（**2 つの行き先がある**）
 *
 * 要件 8 の 4 つの操作は、**同じ型の値を作らない**。行き先が違うからである。
 *
 * | 操作 | 要件 | 何が変わるか | 行き先 |
 * |---|---|---|---|
 * | 列幅の変更 | 8.1 | 描かれる列の幅だけ | [`DisplayState`]（画面の状態。境界へは渡らない） |
 * | 表示上の列順の変更 | 8.2 | 描かれる列の並びだけ | 同上 |
 * | 並べ替え | 8.3 | **窓が運ぶ行とその順序** | `grid_set_view`（`GridViewSpec.sort`） |
 * | 絞り込み | 8.4 | **窓が運ぶ行の集合**（隠れる行が生まれる） | `grid_set_view`（`GridViewSpec.filters`） |
 *
 * **この線引きが要件 8.5 の根拠である。**列幅と表示上の列順は窓の中身を 1 つも変えないので、
 * 境界を越える理由が無い（design.md「表示状態」の割り方の根拠）— 越えれば、文書へ保存される
 * 列の順序が変わる経路が生まれる。本 module は**前者を `GridViewSpec` へ混ぜない**ことを
 * 機械検査で固定する（`viewOps.test.ts` が、どの操作の結果も欄が 3 つ
 * （`sort` / `filters` / `expansion`）だけであることを表明する。欄を 1 つ足せば落ちる）。
 *
 * # 指定は**完全な記述**である（前の指定へ足すのではなく、常に全体を組む）
 *
 * `GridViewSpec` の doc のとおり、空の指定は「絞り込み無し・並べ替え無し・展開無し」を意味し、
 * **要求に現れない指定は適用されない**。したがって本 module の関数はすべて
 * **1 つの指定から 1 つの指定への写像**であり、部分的な指定を組み立てない
 * （部分を送ると、ドメインは要求に現れない展開を折りたたみへ戻す）。
 *
 * **展開の状態は本 module も持つ**（[`ViewOperation`] の `expansion` の腕）。並べ替えや
 * 絞り込みの操作で展開が失われないことは、この形（`{ ...view, sort }`）から出る —
 * 3 つの欄を別々に持つと、片方の操作がもう片方を落とす日が来る。
 *
 * # 表示の並びは 1 つである（描かれる列と、窓の記憶の列の写像）
 *
 * 表示上の列順を変えると、**表示の位置から文書の列への写像そのものが変わる**
 * （`./columnSpace` の module doc の表）。したがって [`drawnColumns`] が返す**表示順の構成**が、
 * 2 つの用途の**唯一の源**でなければならない:
 *
 * 1. 描かれる列（`RendererSpec.columns`。`DisplayState.renderColumns` が組む）
 * 2. 窓の記憶の列の写像（`createColumnSpace`）と、移植口の知らせ（`getCell` の位置、
 *    編集の宛先、違反の位置）
 *
 * どちらか片方だけが並びを反映すると、**描かれている値と編集の宛先が別の列を指す**
 * （要件 8.6 が行について名指しした危険と同じものが列にもある）。8.5 が入れ子の展開で
 * 同じ結論（写像は 1 つ）に達しているので、本 module はその 1 つを表示の並びから組む。
 *
 * # 幅は列そのものに付く（表示位置ではない。7.5 の規則）
 *
 * 幅の読み書きは [`DisplayState`] の口（`widthAt` / `setColumnWidth`）を通す — 鍵が
 * **列**である以上、画面が `columnWidths` を直接引くと「位置で引く」誤りが起きる
 * （7.5 の module doc の表）。並べ替えても幅が動かないことは、その口を通して検査してある。
 *
 * # 単体テストが観測しないもの（**正直に書く**）
 *
 * ① **列幅のドラッグと列のドラッグそのもの**（マウスの操作が移植口の知らせになること）は
 * 実物の起動で観測する（`glideAdapter.tsx` の `onColumnResize` / `onColumnMoved` の配線。
 * 7.2 の申し送り）— 単体テストが固定するのは、**その知らせが来たときに画面が何をするか**である。
 * ② **絞り込みの入力欄への打鍵と選択**は `node` の環境（DOM なし）では観測できない
 * （`viewBar.test.tsx` は「何が DOM へ出るか」と、要素が持つ受け口がどの操作を組み立てるかを
 * 読む）。③ **実際に面が組み直ること**（幅・並びの変更が次の `mount` に載ること）は、
 * 8.5 と同じく実起動で観測する — 単体テストが固定するのは、**次のマウントに載る値**
 * （[`layoutKeyOf`] と [`drawnColumns`]）である。
 */
import type {
  ColumnDescriptor,
  GridExpansionState,
  GridFilterSpec,
  GridViewSpec,
} from "../../ipc/bindings";
import { assertNever } from "../../ipc/client";
import type { DisplayState } from "./displayState";
import { withExpansion } from "./nestedInspector";

/**
 * 絞り込みの種類（**画面の語**である。境界の 5 条件と 1 対 1 である）。
 *
 * `none` は「この列の条件を外す」であり、条件そのものではない（`null` へ写る）。
 */
export type FilterMode = "none" | "contains" | "equals" | "empty" | "notEmpty" | "violating";

/**
 * 画面の語から境界の条件を組む（要件 8.4）。`none` は `null`（**条件を外す**）である。
 *
 * 値なし・値あり・違反ありは `text` を使わない（`equals` / `contains` だけが比較の文字列を
 * 持つ）。文字列を運ばない条件へ空文字を足さないのは、境界の型にその欄が無いからである。
 */
export function filterSpecOf(
  mode: FilterMode,
  column: number,
  text = "",
): GridFilterSpec | null {
  switch (mode) {
    case "none":
      return null;
    case "equals":
      return { filter: "Equals", column, text };
    case "contains":
      return { filter: "Contains", column, text };
    case "empty":
      return { filter: "IsEmpty", column };
    case "notEmpty":
      return { filter: "IsNotEmpty", column };
    case "violating":
      // **列を指定した「違反あり」**である（列を問わない指定は別の要求である。
      // `GridFilterSpec` の doc）。画面が列ごとに出す口からは、この形だけが作れる。
      return { filter: "HasViolation", column };
    default:
      return assertNever(mode, "絞り込みの種類の分岐が網羅されていない");
  }
}

/**
 * 境界の条件を画面の語へ戻す（**選択肢の現在値**である。要件 8.4）。
 *
 * 往復が成り立つこと（`filterModeOf(filterSpecOf(mode, ...)) === mode`）は
 * `viewOps.test.ts` が固定する — 成り立たないと、条件を当てた列の選択肢が別のものを指し、
 * 打鍵のたびに条件が変わる。
 */
export function filterModeOf(filter: GridFilterSpec | null): FilterMode {
  if (filter === null) {
    return "none";
  }
  switch (filter.filter) {
    case "Equals":
      return "equals";
    case "Contains":
      return "contains";
    case "IsEmpty":
      return "empty";
    case "IsNotEmpty":
      return "notEmpty";
    case "HasViolation":
      return "violating";
    default:
      return assertNever(filter, "絞り込みの条件の分岐が網羅されていない");
  }
}

/**
 * 画面から来る表示の操作（**表示の位置ではなく文書の列で指す**）。
 *
 * 並べ替えと絞り込みの対象が**文書の列**であるのは、境界の型がそう定めているためである
 * （`GridSortKey.column` / `GridFilterSpec.column` は行の値の並びに対する添字であり、
 * **表示の位置ではない**）。展開した構成では両者が離れるので、画面は表示位置から
 * `ColumnDescriptor.column` を引いてからここへ渡す。
 */
export type ViewOperation =
  /** その列を巡回させる（基準でない → 昇順 → 降順 → 基準でない）。要件 8.3。 */
  | { readonly kind: "sortCycle"; readonly column: number }
  /** その列を指定の向きの基準にする（**第一の基準になる**）。 */
  | { readonly kind: "sort"; readonly column: number; readonly descending: boolean }
  /** 並べ替えを解除する（基準を 1 つも残さない）。 */
  | { readonly kind: "sortNone" }
  /** その列の絞り込みを設定する（`null` なら外す）。**他の列の条件は残る。** */
  | { readonly kind: "filter"; readonly column: number; readonly filter: GridFilterSpec | null }
  /** 絞り込みをすべて解除する。 */
  | { readonly kind: "filtersNone" }
  /** 入れ子の展開の指定（要件 5.1、5.2）。**他の 2 つを落とさないためにここを通す。** */
  | { readonly kind: "expansion"; readonly state: GridExpansionState };

/**
 * 表示の操作を、**完全な**表示の指定へ写す（全域であり、投げない）。
 *
 * 3 つの欄はどれも**元の指定から引き継ぐ**（操作が触らない欄はそのまま残る）— これが
 * 要件 5.3（展開は走査や並べ替えで失われない）と、絞り込みと並べ替えの共存の根拠である。
 */
export function applyViewOperation(view: GridViewSpec, operation: ViewOperation): GridViewSpec {
  switch (operation.kind) {
    case "sortCycle": {
      const existing = view.sort.find((key) => key.column === operation.column);
      if (existing === undefined) {
        // **新しい列は第一の基準になる**（`sort` の先頭が第一の基準である）。末尾へ足すと、
        // 既に基準があるとき押した列が効かない（同値の行が多く、第一の基準で決まることが多い）。
        return {
          ...view,
          sort: [{ column: operation.column, descending: false }, ...view.sort],
        };
      }
      if (!existing.descending) {
        return {
          ...view,
          sort: view.sort.map((key) =>
            key.column === operation.column ? { column: key.column, descending: true } : key,
          ),
        };
      }
      // 降順の次は**基準から外す**（3 つ目の状態を作らない）。
      return { ...view, sort: view.sort.filter((key) => key.column !== operation.column) };
    }
    case "sort": {
      // 同じ列を 2 度持たない（基準は列ごとに 1 つである）。
      const others = view.sort.filter((key) => key.column !== operation.column);
      return {
        ...view,
        sort: [{ column: operation.column, descending: operation.descending }, ...others],
      };
    }
    case "sortNone":
      return { ...view, sort: [] };
    case "filter": {
      const next: GridFilterSpec[] = [];
      let replaced = false;
      for (const filter of view.filters) {
        if (filter.column !== operation.column) {
          next.push(filter);
          continue;
        }
        // **同じ列の条件はその位置で置き換わる**（末尾へ足すと、条件の並びが操作の手順に
        // 依るようになり、同じ見た目の指定が 2 通りになる）。
        replaced = true;
        if (operation.filter !== null) {
          next.push(operation.filter);
        }
      }
      if (operation.filter !== null && !replaced) {
        next.push(operation.filter);
      }
      return { ...view, filters: next };
    }
    case "filtersNone":
      return { ...view, filters: [] };
    case "expansion":
      // **同じ列の既存の指定を置き換える**規則は `./nestedInspector` が持つ（写しを作らない）。
      return withExpansion(view, operation.state);
    default:
      return assertNever(operation, "表示の操作の分岐が網羅されていない");
  }
}

/**
 * その列の並べ替えの状態（`null` は基準でない）。**向きを潰さない** — 昇順と「基準でない」を
 * 同じ偽にすると、押下の巡回が読めない（画面の文言が実際の指定と食い違う）。
 */
export function sortStateOf(view: GridViewSpec, documentColumn: number): boolean | null {
  const key = view.sort.find((candidate) => candidate.column === documentColumn);
  return key === undefined ? null : key.descending;
}

/** その列の絞り込みの条件（無ければ `null`）。**列ごとに高々 1 件である。** */
export function filterOf(view: GridViewSpec, documentColumn: number): GridFilterSpec | null {
  return view.filters.find((filter) => filter.column === documentColumn) ?? null;
}

/**
 * 行の集合を変えうる指定が効いているか（**並べ替えまたは絞り込み**）。
 *
 * 入れ子の展開は列の話であり、行の集合を変えない（要件 5.3）。**行の増減のあとに表示の指定を
 * 当て直すか**（要件 8.7 の数の取り直し）の判断がこれを読む（8.6 の挿入の位置は可視の序数で
 * 送るため、この条件を見ない。tasks.md 10.4）。
 */
export function hasRowRestriction(view: GridViewSpec): boolean {
  return view.sort.length > 0 || view.filters.length > 0;
}

/**
 * **行の集合の同一性**（並べ替えと絞り込みだけから決まる文字列）。
 *
 * 表の面（器と窓の記憶）は、これが変わったときに組み直す — 並べ替えと絞り込みは「何番目の行が
 * どの行か」を変えるので、**取得済みの窓は別の行を指す**（7.3 の `clear` の doc）。
 * 展開は含めない（列の構成の話であり、面は列の側の合図で組み直る）。
 *
 * 行の数（可視・隠れ）は含めない — それは応答が運ぶ数であり、`GridScreenState.ready` の側が
 * 持つ（数を混ぜると、数を直すたびに面が組み直る）。
 */
export function rowOrderKeyOf(view: GridViewSpec): string {
  return JSON.stringify({ sort: view.sort, filters: view.filters });
}

/**
 * **描かれる列の内容の同一性**（表示の並びと、設定された幅だけから決まる文字列）。
 *
 * 列幅と表示上の列順は、移植口へ**次の `mount` の仕様として**届く（`RendererHandle` に幅や
 * 順を押し込む口が無い。design.md「列幅・列順の反映」）。したがって画面は、この鍵が変わった
 * ときに**器を組み直す**（鍵は「次の仕様が前の仕様と違う」ことの判断そのものである）。
 *
 * **描かれる幅**（設定された幅と既定の幅を解いた結果）を、**表示位置の順**に並べる。設定値では
 * なく結果を見るのは、**同じ絵を描く 2 つの状態が同じ鍵になる**ようにするためである —
 * 既定と同じ幅を明示しても絵は変わらないので、組み直す理由が無い。表示位置の順に並べるのは、
 * 幅が列そのものに付く（7.5）のに対し、**仕様へ載る幅は位置ごと**だからである（並びが変われば
 * 載る幅の並びも変わる）。
 */
export function layoutKeyOf(display: DisplayState): string {
  const widths: (number | null)[] = [];
  for (let position = 0; position < display.columnCount; position += 1) {
    widths.push(display.widthAt(position));
  }
  return JSON.stringify([display.columnOrder, widths]);
}

/**
 * 表示順の構成（**表示の位置の順に並んだ `ColumnDescriptor`**。要件 8.2）。
 *
 * **これが表示の位置の空間そのものである。**返る並びの添字が `RendererSpec.columns` の位置
 * （＝ `CellPosition.column`）であり、各要素の `column` が**文書の列**である — 窓の記憶の写像
 * （`createColumnSpace`）も編集の宛先も、この 1 つの並びから引く（module doc「表示の並びは
 * 1 つである」）。
 *
 * 記述を持たない位置（表示状態が古い場合）は**落とす** — 空の見出しの列を描くと、数え上げの
 * 行（要件 2.5）が名乗る列数と、実際に描かれる列の数が食い違う。
 */
export function drawnColumns(
  layout: readonly ColumnDescriptor[],
  display: DisplayState,
): readonly ColumnDescriptor[] {
  const drawn: ColumnDescriptor[] = [];
  for (let position = 0; position < display.columnCount; position += 1) {
    const layoutPosition = display.layoutColumnAt(position);
    const column = layoutPosition === null ? undefined : layout[layoutPosition];
    if (column !== undefined) {
      drawn.push(column);
    }
  }
  return drawn;
}

/**
 * **文書の列ごとに 1 件**の記述（表示順。最初の出現を残す）。
 *
 * 入れ子の展開は 1 つの文書の列を複数の表示の位置へ写すが（要件 5.1）、並べ替えと絞り込みの
 * 対象は**文書の列**である（境界の型がそう定める）。展開した内側の位置をそのまま対象にすると、
 * **同じ列に 2 件の条件**を出すことになり、押した列と効く条件が食い違う。
 */
export function distinctColumns(
  columns: readonly ColumnDescriptor[],
): readonly ColumnDescriptor[] {
  const seen = new Set<number>();
  const distinct: ColumnDescriptor[] = [];
  for (const column of columns) {
    if (seen.has(column.column)) {
      continue;
    }
    seen.add(column.column);
    distinct.push(column);
  }
  return distinct;
}

/**
 * 絞り込みによって表示されていない行の数の提示（要件 8.7）。**出すものが無ければ `null`。**
 *
 * 数は**応答が運んだ数そのもの**である（画面は数え直さない。`GridViewResponse.hidden_rows` の
 * doc「行が在って隠れている数である」）。絞り込みが効いていなければ隠れている行は無いので
 * 名乗らない（0 行と名乗ると、条件が効いているように読める）。
 */
export function hiddenRowsNotice(options: {
  readonly hiddenRows: number;
  readonly filtersActive: boolean;
}): string | null {
  if (!options.filtersActive && options.hiddenRows <= 0) {
    return null;
  }
  return `絞り込みにより表示していない行: ${String(options.hiddenRows)} 行`;
}

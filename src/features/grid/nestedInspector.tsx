/**
 * 入れ子の列の操作と、入れ子の値の詳細表示（tasks.md 8.5。data-grid 要件 4.5、5.1〜5.7）。
 *
 * 所有: `NestedInspector` / `NestedColumnControls`（design.md「Components and Interfaces」の
 * NestedInspector）。**入れ子に閉じた決定をこの 1 つの module が持つ**:
 *
 * | 何を | どこが | どの要件か |
 * |---|---|---|
 * | 展開の指定（列ごとに 1 つ。`GridViewSpec.expansion`） | [`withExpansion`] / [`isColumnExpanded`] | 5.1、5.2、5.3 |
 * | 列ごとの操作（展開・折りたたみ・詳細表示への誘導・要素数） | [`nestedColumnControls`] / [`NestedColumnControls`] | 5.1、5.2、5.4、5.6 |
 * | 違反している内側の位置の提示 | [`innerPathText`] / [`NestedInspector`] | 4.5 |
 * | 詳細表示の中の編集の入口 | `NestedInspector`（面は登録簿から引く） | 5.5、5.7、10.3 |
 *
 * # 画面は型の札で分岐しない（要件 10.3）
 *
 * 列ごとの操作は**記述が運ぶ 2 つの印**から導く — `expandability`（展開できるか、段数の上限に
 * 達したか）と `element_count`（同一の型の並びの要素数の宣言）。`kind`（型の札）は**見ない**。
 * 「内側があるか」を型の名前で判定すると、ユーザー定義型の列が同じ扱いから外れる。
 *
 * 編集の面も同じである — どの面を出すかも、その文字をどの命令へ載せるか（`EditCarrier`）も、
 * **登録簿が答える**（[`columnEditor`]）。本 module は入力手段の成分を名指ししない。
 *
 * # 展開の状態は「指定」であって「指定が無い」ではない（要件 5.3）
 *
 * `GridViewSpec` は**完全な記述**である（生成物の doc）— ドメインは要求に現れない展開を
 * 折りたたみへ戻す（`src-tauri/src/commands/grid.rs` の `answer_set_view`）。したがって画面は
 * **いまの指定に 1 件を足したもの**を送る（[`withExpansion`]）。押された列だけを載せて送ると、
 * 前に展開した列が黙って折りたたまれる — 要件 5.3「展開の状態を列ごとに保持し、走査によって
 * 失われない」は、この 1 つの関数の形で満たされる。
 *
 * # 詳細表示が示せるもの・示せないもの（**ごまかさない**）
 *
 * 窓が運ぶ入れ子のセルは**要約**（「3項目」）と**違反している内側の位置**だけである
 * （design.md「Data Models / 窓の二進形式」。`transport` の `push_cell`）。値の構造そのものを
 * 読む経路は境界に無い（下の「申し送り」）。したがって詳細表示が示すのは:
 *
 * 1. **窓が運んだ要約**（要件 5.6 の「そのセルの値」）
 * 2. **違反している内側の位置**（要件 4.5。`WindowCache.nestedMarks`）
 * 3. **同一の型の並びの要素数の宣言**（要件 5.6。`ColumnDescriptor.element_count`）
 * 4. **構成が晒している内側の位置とその型**（要件 5.5 の材料。展開された列がそれである）
 *
 * **4 が「構造の全体」に届かない**ことを画面も書く（[`NestedInspector`] の「宣言が読めません」）。
 * 値を空として見せると、利用者は値が空だと読む。
 *
 * ## 申し送り（境界に足りないもの。8.5 が実測した）
 *
 * | # | 何が足りないか | どの要件か | どこへ足すか |
 * |---|---|---|---|
 * | 1 | **値の構造そのもの**（`design.md` は「構造そのものは詳細表示の要求時に JSON として別途取得する」と定めているが、6.1 の 6 本のコマンドに読む口が無い） | 5.5 | 境界に読み口を 1 本足す（`grid_nested_json` など）。**それまでは編集の初期値も空である**（値を捨てないために、確定は構造表現の打ち込みに限る） |
 * | 2 | **展開の結果の列の構成**（`grid_set_view` の応答は可視行数・隠れた行数・違反の総数だけで、導出後の構成を運ばない。`grid_open_sheet` はセッションを作り直すので、開き直しても展開後の構成は得られない） | 5.1、5.2、5.4 | `GridViewResponse` に導出後の列の構成を足す（または `grid_open_sheet` が既存のセッションの表示の指定を保つ）。**それまでは、展開を指定しても描かれる列は変わらない**（ドメインは展開を保持するが、画面はその結果を読めない） |
 * | 3 | **内側の位置ごとの宣言**（7.4 の申し送り 6。`members`） | 5.1、5.5 | 境界用の型、または位置の一覧を返す経路。**それまでは入れ子の面が既定の文字の面へ落ちる**（位置ごとの面を出さない） |
 */
import type { ReactElement } from "react";

import type {
  ColumnDescriptor,
  GridExpansionState,
  GridViewSpec,
} from "../../ipc/bindings";
import { APPEARANCE_VARS } from "../../shell/theme";
import { columnEditor } from "./editors";
import type { EditCarrier } from "./editorRegistry";
import type { CellPosition } from "./renderer/port";
import type { NestedSegment } from "./windowCache";

// ===========================================================================
// 1. 展開の指定（要件 5.1、5.2、5.3）
// ===========================================================================

/**
 * いまの表示の指定へ、1 列ぶんの展開の指定を**足した**ものを返す（要件 5.1、5.2、5.3）。
 *
 * 同じ列の指定は**置き換える**（並びの位置は保つ）。ドメインの導出は同じ列に複数の指定があると
 * **後ろを勝たせる**が、それに頼らない — 画面が持つ指定は「列ごとに 1 つ」であり、そう見える
 * 方が読める（同じ列が 2 度現れる指定は、画面の状態としては重複である）。
 *
 * **並べ替えと絞り込みには触れない。** 本関数が変えるのは `expansion` だけである（8.8 が
 * 並べ替えと絞り込みを足しても、ここを直す必要は無い）。
 */
export function withExpansion(view: GridViewSpec, state: GridExpansionState): GridViewSpec {
  const at = view.expansion.findIndex((entry) => entry.column === state.column);
  const expansion =
    at < 0
      ? [...view.expansion, state]
      : view.expansion.map((entry, index) => (index === at ? state : entry));

  return { sort: view.sort, filters: view.filters, expansion };
}

/** その**文書の列**を展開しているか（要件 5.1）。折りたたみの指定と「指定が無い」はどちらも偽である。 */
export function isColumnExpanded(view: GridViewSpec, documentColumn: number): boolean {
  const entry = view.expansion.find((state) => state.column === documentColumn);
  return entry !== undefined && entry.expanded;
}

/** 展開している列（文書の列の添字。指定の順）。**走査で失われないことの観測口である**（要件 5.3）。 */
export function expandedColumnsIn(view: GridViewSpec): readonly number[] {
  return view.expansion.filter((state) => state.expanded).map((state) => state.column);
}

// ===========================================================================
// 2. 列ごとの操作（要件 5.1、5.2、5.4、5.6）
// ===========================================================================

/**
 * 構成の 1 列ぶんの操作（**描く前の値である**。`NestedColumnControls` がこれを描く）。
 *
 * 操作の有無と種類は**記述の印だけ**から決まる（本 module の module doc）。
 */
export interface NestedColumnControl {
  /** 表示の位置（`ColumnDescriptor` の並びの位置）。 */
  readonly display: number;
  readonly descriptor: ColumnDescriptor;
  /**
   * 押したときに送る展開の指定（`null` なら展開の操作を出さない）。
   *
   * **段数まで決めておく**（内側の位置の「さらに展開」は段数を 1 つ深くする）。
   */
  readonly expansion: GridExpansionState | null;
  /** 展開の操作のラベル（`null` なら操作を出さない）。 */
  readonly expansionLabel: string | null;
  /**
   * 詳細表示の入口のラベル（`null` なら出さない）。
   *
   * 段数の上限に達した列は**「詳細表示へ」**である（要件 5.4 が求める誘導）。それ以外の内側を
   * 持つ列は「詳細表示」である — 構造を見る道は展開とは別であり、**展開できない列にも要る**
   * （要件 5.5）。
   */
  readonly detail: string | null;
  /** 同一の型の並びの要素数の宣言（`null` なら並びではない。要件 5.6）。 */
  readonly elementCount: string | null;
}

/**
 * 構成の列ごとの操作を導く（要件 5.1、5.2、5.4、5.6）。
 *
 * | 記述の印 | 操作 |
 * |---|---|
 * | `available`（内側を持ち、上限に達していない） | 展開 / 折りたたむ（内側の位置では「さらに展開」） |
 * | `capped`（内側を持つが上限に達している） | **詳細表示へ**（要件 5.4） |
 * | `leaf` かつ `element_count` あり（同一の型の並び） | 詳細表示（要素数を示す。要件 5.6） |
 * | `leaf` かつ `element_count` なし | 無し |
 */
export function nestedColumnControls(
  columns: readonly ColumnDescriptor[],
  view: GridViewSpec,
): readonly NestedColumnControl[] {
  return columns.map((descriptor, display) => {
    // 内側の位置の段数（0 はセル直下）。**この位置が構成に現れている**ことは、親がそこまで
    // 展開されていることを意味する（`view` 層の `derive_layout`）。
    const nesting = descriptor.path.length;
    const expanded = isColumnExpanded(view, descriptor.column);

    let expansion: GridExpansionState | null = null;
    let expansionLabel: string | null = null;
    if (descriptor.expandability === "available") {
      if (nesting === 0) {
        // 最上位の列: 展開は 1 段（内側のフィールドまで。要件 5.1）、折りたたみは指定として残す。
        expansion = expanded
          ? { column: descriptor.column, expanded: false, depth: 0 }
          : { column: descriptor.column, expanded: true, depth: 1 };
        expansionLabel = expanded ? "折りたたむ" : "展開";
      } else {
        // 内側の位置: すでに親が `nesting` 段まで展開されているので、1 つ深くする。
        expansion = { column: descriptor.column, expanded: true, depth: nesting + 1 };
        expansionLabel = "さらに展開";
      }
    }

    const hasInner = descriptor.expandability !== "leaf" || descriptor.element_count !== null;
    const detail = descriptor.expandability === "capped" ? "詳細表示へ" : hasInner ? "詳細表示" : null;

    return {
      display,
      descriptor,
      expansion,
      expansionLabel,
      detail,
      elementCount: elementCountText(descriptor),
    };
  });
}

/**
 * 同一の型の並びの要素数の宣言を 1 行にする（要件 5.6）。並びでなければ `null`。
 *
 * **宣言である**（`minItems` / `maxItems`）— そのセルの実際の要素数は窓が運ぶ要約（「3要素」）
 * であり、詳細表示が値として別に示す。開いた端点は「以上」「以下」と書く（`None` は**開いた
 * 端点**であり、0 とは違う。`ColumnElementCount` の doc）。
 */
export function elementCountText(descriptor: ColumnDescriptor): string | null {
  const count = descriptor.element_count;
  if (count === null) {
    return null;
  }
  const range =
    count.min === null && count.max === null
      ? "宣言なし"
      : count.min === null
        ? `${String(count.max)} 以下`
        : count.max === null
          ? `${String(count.min)} 以上`
          : `${String(count.min)}..=${String(count.max)}`;

  return `要素数: ${range}（要素の型: ${count.items}）`;
}

// ===========================================================================
// 3. 違反している内側の位置（要件 4.5）
// ===========================================================================

/**
 * 内側の位置を人が読める 1 つの文字列にする（要件 4.5）。
 *
 * フィールドは `.` で繋ぎ、並びの位置は `[i]` で書く（`a.b` / `tags[2]` / `[0]`）。**空の並びは
 * セル直下**である（生成物の `GridViolationLocation.path` と `GridPathSegment` の同じ規約）ので、
 * 空文字を返す — 呼び出し側が「セル直下」と書き分ける。
 *
 * 段を潰さないのは、`a.b` と `a[1]` を書き分けられなくなるためである（生成物の
 * `GridPathSegment` が `Field` と `Index` を分けている理由と同じ）。
 */
export function innerPathText(segments: readonly NestedSegment[]): string {
  let text = "";
  for (const segment of segments) {
    if (segment.kind === "field") {
      text = text === "" ? segment.name : `${text}.${segment.name}`;
    } else {
      text = `${text}[${String(segment.index)}]`;
    }
  }
  return text;
}

/**
 * その文書の列の**内側の位置**（構成が晒しているもの）を、構成の順に集める（要件 5.5 の材料）。
 *
 * 構成の並びには別の文書の列の位置も混ざる（展開した列の隣には、折りたたまれた列が並ぶ）ので、
 * 同じ文書の列の位置のうち**位置が空でないもの**だけを集める。位置が空の 1 件（その列そのもの）は
 * 内側ではない。
 */
export function declaredInnerPositions(
  columns: readonly ColumnDescriptor[],
  documentColumn: number,
): readonly ColumnDescriptor[] {
  return columns.filter(
    (column) => column.column === documentColumn && column.path.length > 0,
  );
}

// ===========================================================================
// 4. 列ごとの操作の面（要件 5.1、5.2、5.4、5.6）
// ===========================================================================

/** 列ごとの操作の面へ渡すもの。 */
export interface NestedColumnControlsProps {
  readonly columns: readonly ColumnDescriptor[];
  /** いまの表示の指定（展開の状態をここから読む）。 */
  readonly view: GridViewSpec;
  /** 詳細表示へ誘導するときの行（現在位置の行）。 */
  readonly currentRow: number;
  /** 展開の操作（送るのは**いまの指定に足した完全な記述**である）。 */
  readonly onExpansion: (state: GridExpansionState) => void;
  /** 詳細表示の入口（**表示の位置**を渡す。列の identity はそこで引く）。 */
  readonly onDetail: (position: CellPosition) => void;
}

/**
 * 列ごとの操作（要件 5.1、5.2、5.4、5.6）。**表の見出しではなく、表の上の 1 行に出す。**
 *
 * 移植口に「見出しの操作を受け取る口」は無い（`RendererSpec` の `onColumnResize` /
 * `onColumnMove` は幅と並びだけであり、8.1 の表が同じ理由で見出しの操作を持たない）ので、
 * 列の操作はここに並べる。**どの列の操作かが読める**ことが要件である（列の名前を出す）。
 */
export function NestedColumnControls({
  columns,
  view,
  currentRow,
  onExpansion,
  onDetail,
}: NestedColumnControlsProps): ReactElement {
  const controls = nestedColumnControls(columns, view);

  return (
    <ul data-testid="jxcel-grid-column-controls" style={CONTROLS_STYLE}>
      {controls.map((control) => {
        // 押された列の指定をそのまま親へ渡す（親が `withExpansion` で**いまの指定に足した
        // 完全な記述**を組む。押された 1 件だけの指定を送ると、前の展開が消える）。
        const expansion = control.expansion;
        const expansionLabel = control.expansionLabel;
        const detail = control.detail;
        const elementCount = control.elementCount;

        return (
          <li
            key={`${String(control.descriptor.column)}:${String(control.display)}`}
            data-column-control={control.display}
            data-column-kind={control.descriptor.kind ?? "Any"}
            style={CONTROL_STYLE}
          >
            <span>{control.descriptor.name}</span>
            {elementCount === null ? null : (
              <span data-element-count={elementCount} style={CONTROL_NOTE_STYLE}>
                {elementCount}
              </span>
            )}
            {expansion === null || expansionLabel === null ? null : (
              <button
                type="button"
                data-testid={`jxcel-grid-expansion-${String(control.display)}`}
                onClick={() => {
                  onExpansion(expansion);
                }}
                style={BUTTON_STYLE}
              >
                {expansionLabel}
              </button>
            )}
            {detail === null ? null : (
              <button
                type="button"
                data-testid={`jxcel-grid-detail-${String(control.display)}`}
                onClick={() => {
                  // **開くのは現在位置の行のその列である**（利用者が居る行の値を詳しく見る）。
                  onDetail({ row: currentRow, column: control.display });
                }}
                style={BUTTON_STYLE}
              >
                {detail}
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}

// ===========================================================================
// 5. 詳細表示（要件 4.5、5.5、5.6、5.7）
// ===========================================================================

/** 詳細表示へ渡すもの（**源ごとに分けてある**。まとめると、どこから来た値かが読めなくなる）。 */
export interface NestedInspectorProps {
  /** 見せる位置（**表示の位置**。見出しに 1 起点で出す）。 */
  readonly position: CellPosition;
  /** その位置の列の記述（構成）。 */
  readonly column: ColumnDescriptor | null;
  /** 同じ文書の列の内側の位置（構成が晒している範囲。要件 5.5 の材料）。 */
  readonly declared: readonly ColumnDescriptor[];
  /** 窓が運んだ要約（`WindowCache.getCell` の文字）。 */
  readonly summary: string;
  /** 窓がまだ届いていないか（**空文字と混同しない**。要件 1.4）。 */
  readonly loading: boolean;
  /**
   * 違反している内側の位置（`WindowCache.nestedMarks`）。`null` は**未取得**である。
   *
   * 空の並びは「違反なし」、空の並び 1 つ（`[[]]`）は**セル直下**の違反である。
   */
  readonly innerViolations: readonly (readonly NestedSegment[])[] | null;
  /**
   * 編集の面を初期状態へ戻す鍵（確定・取消のたびに親が進める）。
   *
   * 面は打たれた文字を自分の状態に持つので、鍵を変えて作り直す — **確定したのに打ちかけの
   * 文字が残る**（取消したのに文字が残る）という食い違いを作らない。
   */
  readonly editKey: number;
  /** 確定（**運び手つきで上がる**。`./cellEdit` がそれで命令を選ぶ）。 */
  readonly onCommit: (text: string, carrier: EditCarrier) => void;
  /** 取消（**境界へ何も送らない**。要件 3.6）。 */
  readonly onCancel: () => void;
  /** 詳細表示を閉じる（**値も文書も動かない**）。 */
  readonly onClose: () => void;
}

/**
 * 入れ子の値の詳細表示（要件 4.5、5.5、5.6、5.7）。**表の面の中に出す**（8.3 の編集の面と同じ）。
 *
 * 示すものは 4 つであり、それぞれ源が別である（module doc の表）。**値そのものの構造は
 * 境界から読めない**ので、読める範囲を示し、読めないことを書く（`declaredCount === 0` の枝）。
 *
 * 編集の面は**登録簿から引く**（要件 10.3）。運び手も同じ 1 件の登録から来るので、画面に
 * 型ごとの分岐は 1 つも無い。
 */
export function NestedInspector({
  position,
  column,
  declared,
  summary,
  loading,
  innerViolations,
  editKey,
  onCommit,
  onCancel,
  onClose,
}: NestedInspectorProps): ReactElement {
  const resolved = columnEditor(column);
  const Editor = resolved.component;
  const elementCount = elementCountTextOf(column);
  // 見出しの位置の提示（**利用者に見える数は 1 起点である**）。
  const place = `${String(position.row + 1)} 行 ${String(position.column + 1)} 列`;
  const value = loading ? "読み込み中" : summary === "" ? "値なし" : summary;

  return (
    <div
      data-testid="jxcel-grid-nested-inspector"
      data-detail-row={position.row}
      data-detail-column={position.column}
      data-detail-kind={column?.kind ?? "Any"}
      style={INSPECTOR_STYLE}
    >
      <div style={HEADER_STYLE}>
        <span style={MESSAGE_STYLE}>
          {`${place}の入れ子の詳細（${column?.name ?? "構成に無い列"}）`}
        </span>
        <button
          type="button"
          data-testid="jxcel-grid-nested-close"
          onClick={onClose}
          style={BUTTON_STYLE}
        >
          閉じる
        </button>
      </div>

      {/*
        1. 窓が運んだ要約（要件 5.6）。**読み込み中と値なしを混同しない** — 空文字は「値なし」
        という値であり、未取得は「まだ分からない」である（`RenderCell` の doc）。
      */}
      <p data-testid="jxcel-grid-nested-summary" style={MESSAGE_STYLE}>
        {`いまの値: ${value}`}
      </p>

      {/*
        2. 同一の型の並びの要素数の宣言（要件 5.6）。**宣言である**（そのセルの実際の要素数は
        上の要約が運ぶ）。並びでない列では出さない。
      */}
      {elementCount === null ? null : (
        <p data-testid="jxcel-grid-nested-element-count" style={MESSAGE_STYLE}>
          {elementCount}
        </p>
      )}

      {/*
        3. 構成が晒している内側の位置と、その型（要件 5.5 の材料）。**値の構造そのものでは
        ない** — 読めないときはその事実を書く（空の一覧を出さない）。
      */}
      {declared.length === 0 ? (
        <p data-testid="jxcel-grid-nested-unreadable" style={MESSAGE_STYLE}>
          内側のフィールドの宣言が読めません。値そのものの構造を読む経路が境界に無いため、ここに
          示せるのは、展開された列として現れている位置と、下に並ぶ違反の位置だけです。
        </p>
      ) : (
        <ul data-testid="jxcel-grid-nested-declared" style={LIST_STYLE}>
          {declared.map((inner) => (
            <li
              key={`${String(inner.column)}:${inner.name}`}
              data-declared-path={inner.name}
              data-declared-kind={inner.kind ?? "Any"}
            >
              {`${inner.name}: ${inner.kind ?? "Any"}`}
            </li>
          ))}
        </ul>
      )}

      {/*
        4. 違反している内側の位置（要件 4.5）。**未取得を「違反なし」と混同しない** — 印は窓が
        運ぶのであり、届いていなければ分からない（8.4 が窓の印を門番にしたのと同じ規律）。
      */}
      <div data-testid="jxcel-grid-nested-violations" style={INNER_STYLE}>
        <span style={MESSAGE_STYLE}>違反している内側の位置:</span>
        {innerViolations === null ? (
          <span style={MESSAGE_STYLE}>まだ届いていない</span>
        ) : innerViolations.length === 0 ? (
          <span style={MESSAGE_STYLE}>違反はありません</span>
        ) : (
          <ul style={LIST_STYLE}>
            {innerViolations.map((segments, index) => (
              <li key={`${String(index)}:${innerPathText(segments)}`} data-inner-violation={innerPathText(segments)}>
                {innerPathText(segments) === "" ? "セル直下" : innerPathText(segments)}
              </li>
            ))}
          </ul>
        )}
      </div>

      {/*
        5. 詳細表示の中の編集（要件 5.5、5.7）。**確定は同じ規律で扱われる** — 運び手は登録が
        宣言したものであり、`./cellEdit` がそれで `SetCells` / `SetNested` を選ぶ（判定・違反の
        保持・取消の経路はセルの編集と同一である）。
      */}
      <div data-testid="jxcel-grid-nested-editor" data-detail-carrier={resolved.carrier} style={EDITOR_STYLE}>
        <span style={MESSAGE_STYLE}>
          構造表現（JSON）で書き換える。**打ち込んだ構造が値の全体を置き換える**（いまの値の構造は
          読めないので、初期値は空である）。
        </span>
        {/*
          **鍵で作り直す**。打たれた文字は面の状態であるため、確定・取消のたびに初期状態へ戻す
          （`editKey` は親が進める）。
        */}
        <Editor
          key={editKey}
          initialText=""
          constraints={{ kind: column?.kind ?? "Any", nullable: true }}
          commit={(text: string) => {
            onCommit(text, resolved.carrier);
          }}
          cancel={onCancel}
        />
      </div>
    </div>
  );
}

/** その列の要素数の宣言（`null` の列では `null`）。 */
function elementCountTextOf(column: ColumnDescriptor | null): string | null {
  return column === null ? null : elementCountText(column);
}

// ===========================================================================
// 6. 見た目（**配色は器のカスタムプロパティだけを参照する**）
// ===========================================================================

/** 列ごとの操作の並び（**縦に積む**。列の名前と操作が 1 行になる）。 */
const CONTROLS_STYLE = {
  display: "flex",
  flexWrap: "wrap",
  gap: "0.5rem",
  margin: 0,
  padding: 0,
  listStyle: "none",
  fontSize: "0.8125rem",
} as const;

/** 列 1 本ぶんの操作。 */
const CONTROL_STYLE = {
  display: "flex",
  alignItems: "center",
  gap: "0.375rem",
  padding: "0.15rem 0.5rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 要素数の宣言（補助的な文字色）。 */
const CONTROL_NOTE_STYLE = { color: `var(${APPEARANCE_VARS.screenMuted})` } as const;

/** 補助的な文字（説明・要約・状態）。 */
const MESSAGE_STYLE = { margin: 0, color: `var(${APPEARANCE_VARS.screenMuted})` } as const;

/** 操作（展開・折りたたむ・詳細表示へ・閉じる）。**枠線と文字に器の配色を使う。** */
const BUTTON_STYLE = {
  font: "inherit",
  fontSize: "0.8125rem",
  padding: "0.15rem 0.5rem",
  borderRadius: "0.25rem",
  cursor: "pointer",
  color: `var(${APPEARANCE_VARS.controlActiveText})`,
  backgroundColor: `var(${APPEARANCE_VARS.controlActiveBackground})`,
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/** 詳細表示の面（**表の面の中、編集の面と同じ位置に出す**）。 */
const INSPECTOR_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.375rem",
  padding: "0.5rem 0.75rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlActiveBackground})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 詳細表示の見出しの行（位置の提示と、閉じる操作）。 */
const HEADER_STYLE = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: "0.75rem",
} as const;

/** 内側の位置の並び（**1 件 1 行**）。 */
const LIST_STYLE = {
  margin: 0,
  padding: 0,
  listStyle: "none",
  fontSize: "0.8125rem",
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 違反している内側の位置の区画。 */
const INNER_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.125rem",
} as const;

/** 編集の面（**どの位置をどう書き換えるかを名乗る**）。 */
const EDITOR_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.25rem",
  padding: "0.375rem 0.5rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/**
 * グリッド画面の契約（tasks.md 8.1。data-grid 要件 1.5、1.6）。
 *
 * # 何を固定するか
 *
 * 1. **2 つの空の状態**（要件 1.5、1.6）。列が 1 本も宣言されていないときは**表を描かず**
 *    スキーマが未定義であることを示し、列はあるが行が 1 件も無いときは**列の構成を示した
 *    うえで**行が無いことを示す。**2 つの提示が入れ替わらないこと**（片方だけ出る）まで見る
 * 2. **画面内の失敗の経路**（器に届かない失敗）。開く呼び出しが失敗したとき、その旨と
 *    **再試行の操作**が出ること、再試行が読み込みの状態へ戻ること
 * 3. **画面の契約**: 受け取るのは器が渡す `ScreenProps` だけであり、余分な props を要求
 *    しないこと。加えて**登録簿にちょうど 1 件**増え、**初期画面が変わっていない**こと
 * 4. **自前の配色を持たない**こと。配色は器が与えるカスタムプロパティ（`APPEARANCE_VARS` の
 *    10 本）のみを参照する。源の走査で固定する（**走査そのものの生存も見本で確かめる**）
 *
 * # 環境（`node`。器を持たない）
 *
 * `vitest.config.ts` の環境は `node` であり、DOM を持たない（7.2 の結論。`jsdom` を足さない）。
 * したがって:
 *
 *   - 描画は `react-dom/server` の `renderToStaticMarkup` で読む（7.4 の入力手段の検査と同じ）
 *   - **効果（マウント後の読み込み）は走らない**ので、読み込みの流れは純粋な非同期関数
 *     （`loadGridScreenState`）として、状態機械は純粋な遷移として検査する
 *   - 器へ移植口を実際にマウントする経路（見え方）は、実物を起動して観測する
 *     （`tech.md`。単体テストは回帰の網であって受入の証明ではない）
 *
 * # 配色の走査の限界（**正直に書く**）
 *
 * 色の値の走査（`#` / `rgb(` / `hsl(`）は**この画面の源 1 つ**に当てる。取り込んだ module
 * （たとえば移植口の実装）が持つ色は対象外である — 移植口には色を運ぶ欄が無く、移植口の
 * 実装が既定を 1 つ持つ（`glideAdapter.tsx` の `VIOLATION_THEME`）。この穴は design.md の
 * Implementation Notes に記録してある。
 */
import { describe, expect, it, vi } from "vitest";
import { createElement, type ComponentType } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { APPEARANCE_VARS } from "../../shell/theme";
import type { ScreenProps } from "../../shell/router";
import type {
  ColumnDescriptor,
  DocumentSheet,
  DocumentStateResponse,
  GridEditCommand,
  GridEditOutcome,
  GridEditResponse,
  GridOpenResponse,
  GridSheetSummary,
  GridViewResponse,
  IpcResult,
  TypeKindTag,
} from "../../ipc/bindings";
import type { IpcClientError } from "../../ipc/client";
import { EDITOR_SMOKE_SCREEN_ID } from "../smoke/EditorSmoke";
import { TABLE_SMOKE_SCREEN_ID } from "../smoke/TableSmoke";
import { EMPTY_WINDOW_SCREEN_ID } from "../empty/EmptyWindowScreen";
import { DIAGNOSTICS_SCREEN_ID } from "../diagnostics/requests";
import {
  GRID_SCREEN_ID,
  GridScreen,
  GridScreenView,
  createGridRendererSpec,
  gridScreenEditReportDismissed,
  gridScreenEditSettled,
  gridScreenEditStarted,
  gridScreenFailed,
  gridScreenLoaded,
  gridScreenNoticeDismissed,
  gridScreenRetried,
  gridScreenSelectionChanged,
  initialGridScreenModel,
  loadGridScreenState,
  type GridScreenModel,
  type GridScreenState,
} from "./GridScreen";
import { initialSelection } from "./selection";
import type { GridClient } from "./gridClient";
import type { CellPosition, RendererSelection, VisibleSpan } from "./renderer/port";

// ===========================================================================
// 検査の道具（偽の境界と、状態からの描画）
// ===========================================================================

/** 境界の窓の文脈（`WindowContext`。値そのものは検査に効かない）。 */
const CONTEXT = { window: "main" } as const;

/** 境界の失敗（経路の不達。`IpcError` の 1 つの腕）。 */
const FAILURE: IpcClientError = { kind: "Document", detail: { message: "経路が不達である" } };

/** 列 1 本ぶんの記述（`ColumnDescriptor` の必須の欄をすべて埋める）。 */
function descriptor(column: number, name: string): ColumnDescriptor {
  return { column, path: [], name, kind: "Text", element_count: null, expandability: "leaf" };
}

/** `document_state` が運ぶシートの 1 件。 */
function sheetOf(id: string, name: string, columns: number, rows: number): DocumentSheet {
  return { id, name, columns, rows };
}

/** 開いたドキュメントの状態（`DocumentSessionStatus` の `Open` の腕）。 */
function openDocument(sheets: readonly DocumentSheet[]): DocumentStateResponse {
  return {
    context: CONTEXT,
    status: { state: "Open", name: "標本", origin: "new", unsaved: false, sheets: [...sheets] },
  };
}

/** シートを開いた応答。 */
function openedSheet(summary: GridSheetSummary): GridOpenResponse {
  return { context: CONTEXT, sheet: summary };
}

/** 表示の指定を適用した応答（可視行数だけが検査に効く）。 */
function derivedView(visibleRows: number): GridViewResponse {
  return { context: CONTEXT, visible_rows: visibleRows, hidden_rows: 0, violation_total: 0 };
}

/** 成功の封筒。 */
function ok<T>(data: T): IpcResult<T, IpcClientError> {
  return { status: "ok", data };
}

/** 失敗の封筒。 */
function err<T>(): IpcResult<T, IpcClientError> {
  return { status: "error", error: FAILURE };
}

/** 偽の境界。**どの口が呼ばれたかを順に数える**（表を描かないとき開く呼び出しが起きないこと）。 */
interface FakeClient extends GridClient {
  readonly calls: readonly string[];
  readonly edits: readonly GridEditCommand[];
}

function fakeClient(answers: {
  readonly state: IpcResult<DocumentStateResponse, IpcClientError>;
  readonly open?: IpcResult<GridOpenResponse, IpcClientError>;
  readonly view?: IpcResult<GridViewResponse, IpcClientError>;
  readonly edit?: IpcResult<GridEditResponse, IpcClientError>;
}): FakeClient {
  const calls: string[] = [];
  const edits: GridEditCommand[] = [];
  return {
    calls,
    edits,
    readDocumentState: async () => {
      calls.push("document_state");
      return answers.state;
    },
    openSheet: async (sheet: string) => {
      calls.push(`grid_open_sheet:${sheet}`);
      return answers.open ?? err<GridOpenResponse>();
    },
    setView: async (view) => {
      // **空の指定であることまで数える**（操作ではない。並べ替え・絞り込みは 8.8 の担当）。
      calls.push(`grid_set_view:${view.sort.length}${view.filters.length}${view.expansion.length}`);
      return answers.view ?? err<GridViewResponse>();
    },
    readWindow: async () => {
      calls.push("grid_rows_window");
      // 8.1 の検査は窓を引かない（引くのは描き手である）。空の窓を返して再試行を起こさない。
      return new ArrayBuffer(0);
    },
    applyEdit: async (command) => {
      calls.push("grid_apply_edit");
      edits.push(command);
      return answers.edit ?? err<GridEditResponse>();
    },
  };
}

/** 状態を描いたマーク付け（画面が実際に DOM へ出すものを読む）。 */
function markOf(model: GridScreenModel): string {
  return renderToStaticMarkup(
    createElement(GridScreenView, {
      model,
      // 効果は走らないので、この口が呼ばれることはない（描かれるものだけを読む）。
      client: fakeClient({ state: err<DocumentStateResponse>() }),
      onRetry: () => undefined,
      onDismissNotice: () => undefined,
      onDismissEditReport: () => undefined,
      onUnavailable: () => undefined,
      onSelectionChange: () => undefined,
      onEditStarted: () => undefined,
      onEditSettled: () => undefined,
    }),
  );
}

/** 状態 1 つぶんのマーク付け（読み込み済みの状態機械を組む）。 */
function markOfState(state: GridScreenState): string {
  return markOf(gridScreenLoaded(initialGridScreenModel(), state));
}

/** 列の構成として出た見出し（**宣言の順**に読む）。 */
function columnNamesIn(markup: string): readonly string[] {
  return [...markup.matchAll(/data-grid-column="[^"]*"[^>]*>([^<]*)</g)].map((match) => match[1] ?? "");
}

// ===========================================================================
// 1. 2 つの空の状態（要件 1.5、1.6）
// ===========================================================================

describe("2 つの空の状態（要件 1.5、1.6）", () => {
  it("列が 1 本も宣言されていないとき、表を描かずスキーマが未定義であることを示す", async () => {
    const client = fakeClient({ state: ok(openDocument([sheetOf("s1", "空のシート", 0, 0)])) });

    const state = await loadGridScreenState(client);

    expect(state).toEqual({ status: "no-schema", sheetName: "空のシート" });
    // **開く呼び出しをしない。** 列 0 本の計画は Rust 側が `SchemaUnusable` として拒むので
    // （`GridSession::open`）、「表を描かない」という答えは境界の答えを待たずに決まる。
    expect(client.calls).toEqual(["document_state"]);

    const markup = markOfState(state);
    expect(markup).toContain("jxcel-grid-schema-undefined");
    expect(markup).toContain("スキーマが未定義です");
    // 表も、行が無いことの提示も出さない（2 つの提示が入れ替わらないこと）。
    expect(markup).not.toContain("jxcel-grid-table");
    expect(markup).not.toContain("jxcel-grid-no-rows");
  });

  it("列はあるが行が 1 件も無いとき、列の構成を示したうえで行が無いことを示す", async () => {
    const columns = [descriptor(0, "名前"), descriptor(1, "数量")];
    const client = fakeClient({
      state: ok(openDocument([sheetOf("s1", "標本シート", 2, 0)])),
      open: ok(openedSheet({ columns, row_count: 0 })),
    });

    const state = await loadGridScreenState(client);

    expect(state.status).toBe("no-rows");
    expect(client.calls).toEqual(["document_state", "grid_open_sheet:s1"]);

    const markup = markOfState(state);
    expect(markup).toContain("jxcel-grid-no-rows");
    expect(markup).toContain("行がありません");
    expect(markup).toContain("jxcel-grid-columns");
    // 列の構成は**宣言の順**に、宣言された名前で出す。
    expect(columnNamesIn(markup)).toEqual(["名前", "数量"]);
    // 表は描かない（行が無いので描くものが無い）が、**列の構成は示す**。
    expect(markup).not.toContain("jxcel-grid-table");
    expect(markup).not.toContain("jxcel-grid-schema-undefined");
  });

  it("行があるときは表を描く（2 つの状態の判定が行数だけでは決まらないこと）", async () => {
    const columns = [descriptor(0, "名前"), descriptor(1, "数量")];
    const client = fakeClient({
      state: ok(openDocument([sheetOf("s1", "標本シート", 2, 3)])),
      open: ok(openedSheet({ columns, row_count: 3 })),
      view: ok(derivedView(3)),
    });

    const state = await loadGridScreenState(client);

    // **可視行の順序を導出させる呼び出しまで行う**（これが無いと窓はつねに行 0 件で返る）。
    expect(client.calls).toEqual(["document_state", "grid_open_sheet:s1", "grid_set_view:000"]);
    expect(state.status).toBe("ready");
    if (state.status !== "ready") {
      throw new Error("表を描く状態にならなかった");
    }
    // **窓が覆うのは可視行である**（シートの行数ではない）。
    expect(state.visibleRows).toBe(3);
    const markup = markOfState(state);
    expect(markup).toContain("jxcel-grid-table");
    expect(markup).not.toContain("jxcel-grid-no-rows");
    expect(markup).not.toContain("jxcel-grid-schema-undefined");
    expect(markup).not.toContain("jxcel-grid-failure");
  });

  it("ドキュメントを開いていないときは、空の状態ではなく失敗として示す（再試行つき）", async () => {
    const client = fakeClient({
      state: ok({ context: CONTEXT, status: { state: "Absent" } }),
    });

    const state = await loadGridScreenState(client);

    expect(state.status).toBe("failed");
    expect(client.calls).toEqual(["document_state"]);
    const markup = markOfState(state);
    expect(markup).toContain("jxcel-grid-failure");
    expect(markup).not.toContain("jxcel-grid-schema-undefined");
    expect(markup).not.toContain("jxcel-grid-no-rows");
  });
});

// ===========================================================================
// 2. 画面内の失敗の経路（器に届かない失敗）
// ===========================================================================

describe("画面内の失敗の経路（器に届かない失敗）", () => {
  it("開く呼び出しが失敗したとき、その旨と再試行の操作を出す", async () => {
    const client = fakeClient({
      state: ok(openDocument([sheetOf("s1", "標本シート", 2, 10)])),
      open: err<GridOpenResponse>(),
    });

    const state = await loadGridScreenState(client);

    expect(state.status).toBe("failed");
    if (state.status !== "failed") {
      throw new Error("失敗の状態にならなかった");
    }
    expect(state.canRetry).toBe(true);
    expect(state.message).toContain("経路が不達である");

    const markup = markOfState(state);
    expect(markup).toContain("jxcel-grid-failure");
    expect(markup).toContain("jxcel-grid-retry");
    expect(markup).toContain("再試行");
    // 失敗の提示は内容の領域に出す（表も空の状態の提示も出さない）。
    expect(markup).not.toContain("jxcel-grid-table");
    expect(markup).not.toContain("jxcel-grid-no-rows");
  });

  it("可視行の順序の導出が失敗したときも、その旨と再試行の操作を出す", async () => {
    const client = fakeClient({
      state: ok(openDocument([sheetOf("s1", "標本シート", 2, 10)])),
      open: ok(openedSheet({ columns: [descriptor(0, "名前")], row_count: 10 })),
      view: err<GridViewResponse>(),
    });

    const state = await loadGridScreenState(client);

    expect(state.status).toBe("failed");
    expect(client.calls).toEqual(["document_state", "grid_open_sheet:s1", "grid_set_view:000"]);
    const markup = markOfState(state);
    expect(markup).toContain("jxcel-grid-failure");
    expect(markup).toContain("jxcel-grid-retry");
  });

  it("再試行は読み込みの状態へ戻り、試行の番号が進む（読み込みが走り直す）", () => {
    const failed = gridScreenLoaded(initialGridScreenModel(), {
      status: "failed",
      message: "経路が不達である",
      canRetry: true,
    });

    const retried = gridScreenRetried(failed);

    // 読み込みの効果は試行の番号を依存に持つので、番号が進むことが「走り直す」の実体である。
    expect(retried.attempt).toBe(failed.attempt + 1);
    expect(retried.state).toEqual({ status: "loading" });
    expect(gridScreenRetried(retried).attempt).toBe(failed.attempt + 2);
  });

  it("イベントハンドラ・非同期の失敗は、内容を消さずに 1 行として出す", () => {
    const ready = gridScreenLoaded(initialGridScreenModel(), {
      status: "ready",
      sheet: "s1",
      summary: { columns: [descriptor(0, "名前")], row_count: 3 },
      visibleRows: 3,
      selection: initialSelection(),
      editing: null,
    });

    const withNotice = gridScreenFailed(ready, "この操作はまだ結線されていない: 列の幅");

    expect(withNotice.state).toEqual(ready.state);
    const markup = markOf(withNotice);
    expect(markup).toContain("jxcel-grid-notice");
    expect(markup).toContain("列の幅");
    // 内容（表）は消さない — 選ばれた 1 つの操作の失敗で画面の内容を失わない。
    expect(markup).toContain("jxcel-grid-table");
    // 告知には再試行が無い（開き直しても直らない失敗である）。
    expect(markup).not.toContain("jxcel-grid-retry");

    const dismissed = gridScreenNoticeDismissed(withNotice);
    expect(dismissed.state).toEqual(withNotice.state);
    expect(markOf(dismissed)).not.toContain("jxcel-grid-notice");
  });

  it("試行の番号は、遅れて届いた結果の適用でも保たれる（遷移の側の性質）", async () => {
    // **この検査は「古い応答が新しい表示を上書きしない」ことを観測しない。**その守りは
    // `GridScreen` の効果の後始末（`cancelled`）にあり、効果の生き死には描画器を要するため
    // ここでは測れない（正しさ自体は 8.1 のレビューが使い捨ての駆動器で実測しており、
    // `cancelled` を消すと古い失敗が勝つことを確認している）。ここで固定するのは**遷移の側の
    // 性質**である — 遅れて届いた結果を適用しても試行の番号が潰れないこと（番号を潰す遷移を
    // 足すとこの検査が落ちる）。恒久の観測は 9.3（失敗と劣化を画面へ結線する側）が行う。
    const stale = gridScreenLoaded(initialGridScreenModel(), {
      status: "loading",
    });
    expect(stale.attempt).toBe(0);

    const retried = gridScreenRetried(
      gridScreenLoaded(stale, { status: "failed", message: "古い失敗", canRetry: true }),
    );
    const arrivedLate = gridScreenLoaded(retried, {
      status: "failed",
      message: "古い失敗",
      canRetry: true,
    });

    expect(arrivedLate.attempt).toBe(1);
    expect(arrivedLate.state.status).toBe("failed");
  });
});

describe("移植口の操作（8.3〜8.9 が結線する）", () => {
  it("編集の起動は結線され、まだ結線していない操作は黙って捨てずに画面内の告知へ流す", async () => {
    const unavailable: string[] = [];
    const activated: [CellPosition, string][] = [];
    const spec = createGridRendererSpec({
      columns: [{ title: "名前", width: 120 }],
      rowCount: 3,
      selection: initialSelection(),
      rowMarkers: "clickable-number",
      // 引く口は移植口へそのまま渡る（表を描く経路である）。
      getCell: () => ({ text: "標本", variant: "Text", violated: false, loading: false }),
      onSelectionChange: () => undefined,
      onVisibleSpanChange: () => undefined,
      onActivateEditor: (position, initialText) => {
        activated.push([position, initialText]);
      },
      onUnavailable: (operation) => {
        unavailable.push(operation);
      },
    });

    expect(spec.rowCount).toBe(3);
    expect(spec.columns).toEqual([{ title: "名前", width: 120 }]);
    expect(spec.getCell({ row: 0, column: 0 })).toEqual({
      text: "標本",
      variant: "Text",
      violated: false,
      loading: false,
    });

    // **編集の起動（要件 3.1）はもう「まだ使えない操作」ではない。**位置と、いま描かれている
    // 値が画面へ上がる。
    spec.onActivateEditor({ row: 0, column: 0 });
    expect(activated).toEqual([[{ row: 0, column: 0 }, "標本"]]);
    expect(unavailable).toEqual([]);

    spec.onColumnResize(0, 200);
    spec.onColumnMove(0, 1);
    // 値を持つ 2 つは**拒否**である（空文字を返せばクリップボードが空になり、黙って解決すれば
    // 貼り付けが消える。移植口の実装は拒否を記録するだけで、描画を止めない）。
    await expect(
      spec.onCopy({ start: { row: 0, column: 0 }, end: { row: 0, column: 0 } }),
    ).rejects.toThrow("選択の範囲の複製");
    await expect(spec.onPaste({ row: 0, column: 0 }, "1\t2")).rejects.toThrow(
      "表形式のテキストの貼り付け",
    );

    expect(unavailable).toEqual([
      "列の幅の変更",
      "列の位置の変更",
      "選択の範囲の複製",
      "表形式のテキストの貼り付け",
    ]);

    // 選択の知らせは**操作ではない**（8.2 が消費する。告知へは流さない）。
    spec.onSelectionChange(null);
    expect(unavailable).toHaveLength(4);
  });
});

// ===========================================================================
// 2.5 現在位置と選択（要件 2.1、2.2、2.3、2.5、2.6。8.2）
// ===========================================================================

/** 標本の列 3 本（数え上げの検査に要る。行数は 20 件）。 */
const SAMPLE_COLUMNS: readonly ColumnDescriptor[] = [
  descriptor(0, "名前"),
  descriptor(1, "数量"),
  descriptor(2, "提供元"),
];
const SAMPLE_ROWS = 20;

/** 表を描いている状態（選択を指定して組む）。 */
function readyModel(selection: RendererSelection): GridScreenModel {
  return gridScreenLoaded(initialGridScreenModel(), {
    status: "ready",
    sheet: "s1",
    summary: { columns: [...SAMPLE_COLUMNS], row_count: SAMPLE_ROWS },
    visibleRows: SAMPLE_ROWS,
    selection,
    editing: null,
  });
}

/** マーク付けから数え上げを読む（**文字ではなく属性の数を読む**）。 */
function countsIn(markup: string): {
  readonly rows: number;
  readonly columns: number;
  readonly cells: number;
  readonly currentRow: number;
  readonly currentColumn: number;
} {
  const attribute = (name: string): number => {
    const match = new RegExp(`${name}="(-?[0-9]+)"`).exec(markup);
    if (match === null) {
      throw new Error(`数え上げの属性 ${name} が画面に出ていない`);
    }
    return Number(match[1]);
  };
  return {
    rows: attribute("data-selection-rows"),
    columns: attribute("data-selection-columns"),
    cells: attribute("data-selection-cells"),
    currentRow: attribute("data-current-row"),
    currentColumn: attribute("data-current-column"),
  };
}

describe("現在位置と選択（8.2。要件 2.1、2.3、2.5）", () => {
  it("表を描く状態は、現在位置を 1 つ持って始まる（先頭のセル）", async () => {
    const client = fakeClient({
      state: ok(openDocument([sheetOf("s1", "標本シート", 3, SAMPLE_ROWS)])),
      open: ok(openedSheet({ columns: [...SAMPLE_COLUMNS], row_count: SAMPLE_ROWS })),
      view: ok(derivedView(SAMPLE_ROWS)),
    });

    const state = await loadGridScreenState(client);

    // **現在位置は開いた時点で 1 つある**（要件 2.1）。表を描かない状態は選択を持たない
    // （型がそれを表している — `ready` の腕だけが `selection` を持つ）。
    expect(state.status).toBe("ready");
    if (state.status !== "ready") {
      throw new Error("表を描く状態にならなかった");
    }
    expect(state.selection).toEqual(initialSelection());
    expect(state.selection.current).toEqual({ row: 0, column: 0 });
  });

  it("1 つのセルの選択は 1 行 × 1 列 = 1 セルとして画面に出る（退化した場合）", () => {
    const markup = markOf(
      readyModel({ current: { row: 0, column: 0 }, range: { start: { row: 0, column: 0 }, end: { row: 0, column: 0 } } }),
    );

    expect(countsIn(markup)).toEqual({
      rows: 1,
      columns: 1,
      cells: 1,
      currentRow: 0,
      currentColumn: 0,
    });
    expect(markup).toContain("jxcel-grid-table");
    expect(markup).toContain("jxcel-grid-selection-counts");
  });

  it("矩形の選択は行数 × 列数 = セル数として画面に出る", () => {
    const markup = markOf(
      readyModel({
        current: { row: 1, column: 0 },
        range: { start: { row: 1, column: 0 }, end: { row: 3, column: 1 } },
      }),
    );

    expect(countsIn(markup)).toEqual({
      rows: 3,
      columns: 2,
      cells: 6,
      currentRow: 1,
      currentColumn: 0,
    });
  });

  it("行の全体と列の全体でも、行数・列数・セル数が画面に出る", () => {
    // 行の全体（列 3 本のシート）: 1 行 × 3 列 = 3 セル。
    const wholeRow = markOf(
      readyModel({
        current: { row: 5, column: 1 },
        range: { start: { row: 5, column: 0 }, end: { row: 5, column: 2 } },
      }),
    );
    expect(countsIn(wholeRow)).toEqual({
      rows: 1,
      columns: 3,
      cells: 3,
      currentRow: 5,
      currentColumn: 1,
    });

    // 列の全体（可視行 20 件）: 20 行 × 1 列 = 20 セル。
    const wholeColumn = markOf(
      readyModel({
        current: { row: 5, column: 1 },
        range: { start: { row: 0, column: 1 }, end: { row: SAMPLE_ROWS - 1, column: 1 } },
      }),
    );
    expect(countsIn(wholeColumn)).toEqual({
      rows: SAMPLE_ROWS,
      columns: 1,
      cells: SAMPLE_ROWS,
      currentRow: 5,
      currentColumn: 1,
    });
  });

  it("利用者に見える現在位置は 1 起点である（内部の序数は 0 起点である）", () => {
    const markup = markOf(
      readyModel({
        current: { row: 4, column: 2 },
        range: { start: { row: 4, column: 2 }, end: { row: 4, column: 2 } },
      }),
    );

    // 属性は**内部の序数**（0 起点）、画面の文字は**利用者に見える数**（1 起点）である。
    expect(countsIn(markup).currentRow).toBe(4);
    expect(markup).toContain("現在位置 5 行 3 列");
  });

  it("移植口へ渡す仕様に、選択・行見出し・知らせの口が載る（欄の素通し）", () => {
    const selection: RendererSelection = {
      current: { row: 2, column: 1 },
      range: { start: { row: 1, column: 0 }, end: { row: 2, column: 1 } },
    };
    const observed: (RendererSelection | null)[] = [];
    const spans: VisibleSpan[] = [];
    const spec = createGridRendererSpec({
      columns: [{ title: "名前", width: 120 }],
      rowCount: SAMPLE_ROWS,
      selection,
      rowMarkers: "clickable-number",
      getCell: () => ({ text: "", variant: "Text", violated: false, loading: false }),
      onSelectionChange: (next) => {
        observed.push(next);
      },
      onVisibleSpanChange: (span) => {
        spans.push(span);
      },
      onActivateEditor: () => undefined,
      onUnavailable: () => undefined,
    });

    // マウントの時点の選択がそのまま渡る（要件 2.1）。
    expect(spec.selection).toEqual(selection);
    // **行見出しを出す**（行の全体をポインタで選ぶ操作はこれで成立する。要件 2.3）。
    expect(spec.rowMarkers).toBe("clickable-number");
    // 実装の知らせは画面の口へそのまま流れる（**現在位置を含む**）。
    const reported: RendererSelection = {
      current: { row: 7, column: 2 },
      range: { start: { row: 5, column: 0 }, end: { row: 7, column: 2 } },
    };
    spec.onSelectionChange(reported);
    spec.onSelectionChange(null);
    expect(observed).toEqual([reported, null]);
    // 見えている区間も同じである（追随の判断と窓の先読みの材料。要件 2.4）。
    const span = { rows: { start: 0, count: 24 }, columns: { start: 0, count: 3 } };
    spec.onVisibleSpanChange(span);
    expect(spans).toEqual([span]);
  });
});

describe("選択の遷移（8.2。要件 2.1）", () => {
  it("表を描いていないときは選択を入れ替えない（描いていない表に現在位置は無い）", () => {
    const loading = initialGridScreenModel();

    expect(gridScreenSelectionChanged(loading, initialSelection())).toBe(loading);

    const failed = gridScreenLoaded(loading, { status: "failed", message: "だめ", canRetry: true });
    expect(gridScreenSelectionChanged(failed, initialSelection())).toBe(failed);
  });

  it("選択を入れ替えても、内容の領域と告知は動かない", () => {
    const before = readyModel(initialSelection());
    const next: RendererSelection = {
      current: { row: 9, column: 2 },
      range: { start: { row: 9, column: 2 }, end: { row: 9, column: 2 } },
    };

    const after = gridScreenSelectionChanged(before, next);

    expect(after.state).toEqual({ ...before.state, selection: next });
    expect(after.attempt).toBe(before.attempt);
    expect(after.notice).toBe(before.notice);
  });

  it("解除の知らせでは、いまの選択を置き直す（描かれている選択と写しがずれない）", () => {
    // Glide の Escape は選択の解除を報せてくる。**それを受け取って取り下げると、器に解除が
    // 描かれたまま画面の写しだけが残る**（現在位置が 0 つになる。要件 2.1 に反する）。
    const before = readyModel({
      current: { row: 3, column: 1 },
      range: { start: { row: 3, column: 1 }, end: { row: 3, column: 1 } },
    });

    const after = gridScreenSelectionChanged(before, null);

    if (after.state.status !== "ready" || before.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // 値は同じであり、**同一の値ではない**（新しい値なので、移植口へもう一度下ろされる）。
    expect(after.state.selection).toEqual(before.state.selection);
    expect(after.state.selection).not.toBe(before.state.selection);
    // 画面の数え上げも変わらない（写しは 1 つである）。
    expect(countsIn(markOf(after))).toEqual(countsIn(markOf(before)));
  });
});

// ===========================================================================
// 2.6 セルの編集（tasks.md 8.3。要件 3.1、3.3、3.4、3.5、3.6、3.7）
// ===========================================================================

/** 編集した行の識別子（正準の 26 文字）。 */
const EDITED_ROW = "01ARZ3NDEKTSV4RRFFQ69G5FB0";

/** もう 1 つの行（違反の位置が複数になる場合に使う）。 */
const OTHER_ROW = "01ARZ3NDEKTSV4RRFFQ69G5FB1";

/** 札を選べる列の記述（既存の `descriptor` は `Text` 固定である）。 */
function descriptorOfKind(column: number, name: string, kind: TypeKindTag | null): ColumnDescriptor {
  return { column, path: [], name, kind, element_count: null, expandability: "leaf" };
}

/** 適用の結果（指定した欄だけを変えて組む）。 */
function outcomeOf(overrides: Partial<GridEditOutcome>): GridEditOutcome {
  return {
    affected: [],
    coercions: [],
    violation_total: 0,
    violations: [],
    revalidated_columns: [],
    row_count: SAMPLE_ROWS,
    ...overrides,
  };
}

/** 表を描いていて、1 つのセルを編集中の状態（列の札を選べる）。 */
function editingModel(
  columns: readonly ColumnDescriptor[],
  position: CellPosition,
  initialText: string,
): GridScreenModel {
  return gridScreenEditStarted(
    gridScreenLoaded(initialGridScreenModel(), {
      status: "ready",
      sheet: "s1",
      summary: { columns: [...columns], row_count: SAMPLE_ROWS },
      visibleRows: SAMPLE_ROWS,
      selection: initialSelection(),
      editing: null,
    }),
    position,
    initialText,
  );
}

/** 確定の結果を画面へ反映する（画面が `settleCellEdit` の結果に対して行う遷移そのものである）。 */
function settled(model: GridScreenModel, outcome: GridEditOutcome | null): GridScreenModel {
  return gridScreenEditSettled(model, { status: "applied", outcome });
}

/** `checked` の付いた欄が持つ値の並び（初期値がどの選択肢かを読む）。 */
function checkedValues(markup: string): readonly string[] {
  return (markup.match(/<input\b[^>]*>/g) ?? [])
    .filter((tag) => tag.includes("checked"))
    .map((tag) => /value="([^"]*)"/.exec(tag)?.[1] ?? "");
}

describe("編集の起動と入力手段の解決（8.3。要件 3.1、10.3、10.4）", () => {
  it("編集中の面は、現在位置を名乗り、その列の札の入力手段を登録簿から出す", () => {
    const markup = markOf(
      editingModel(
        [descriptorOfKind(0, "在庫", "Bool"), descriptorOfKind(1, "名前", "Text")],
        { row: 4, column: 0 },
        "true",
      ),
    );

    // **どのセルを編集しているかが読める**（面は表の器の外に出る。覆われたセルを探させない）。
    expect(markup).toContain("jxcel-grid-editor");
    expect(markup).toContain('data-editor-row="4"');
    expect(markup).toContain('data-editor-column="0"');
    expect(markup).toContain("5 行 1 列を編集中");
    // 札に対応する面が出る（`Bool` の面 = 二値の切り替え）。
    expect(markup).toContain('data-editor-kind="Bool"');
    expect(markup).toContain('aria-label="真偽"');
    expect(markup).toContain('type="radio"');
    // 初期値は**開いた時点の表示文字列**である（`true` の側が選ばれている）。
    expect(checkedValues(markup)).toEqual(["true"]);
  });

  it("別の札の列では、別の面が出る（画面に型ごとの分岐が無いこと）", () => {
    const columns = [descriptorOfKind(0, "在庫", "Bool"), descriptorOfKind(1, "名前", "Text")];

    const text = markOf(editingModel(columns, { row: 4, column: 1 }, "標本"));

    // 文字の札は文字の欄であり、**真偽の面は出ない**（解決は登録簿が行う）。
    expect(text).toContain('data-editor-kind="Text"');
    expect(text).toContain('type="text"');
    expect(text).toContain('value="標本"');
    expect(text).not.toContain('aria-label="真偽"');
  });

  it("登録の無い札は、登録簿の既定（値をそのまま扱う面）へ落ちる", () => {
    // `Attachment` は組込が登録しない札である（`editors/index.ts` の表。実体の選択は本スペックの
    // 対象外）。**面を出さないのではなく、既定へ落ちる**（要件 10.4 の事後条件）。
    const markup = markOf(
      editingModel([descriptorOfKind(0, "添付", "Attachment")], { row: 0, column: 0 }, "a.png"),
    );

    expect(markup).toContain('data-editor-kind="Attachment"');
    expect(markup).toContain('type="text"');
    expect(markup).toContain('value="a.png"');
  });

  it("札が読めない列は `Any` として登録簿へ来る（値をそのまま扱う面）", () => {
    // 宣言が壊れている列は `kind` が `null` で届く（生成物の doc）。7.4 の約束どおり `Any` と
    // して解決され、**面は必ず出る**（編集できない列は作らない）。
    const markup = markOf(
      editingModel([descriptorOfKind(0, "壊れた列", null)], { row: 0, column: 0 }, "{}"),
    );

    expect(markup).toContain('data-editor-kind="Any"');
    expect(markup).toContain("<textarea");
  });

  it("値なしへ戻す道が出る（要件 3.7。**境界に nullable の欄が無いので、つねに出す**）", () => {
    // キーだけで取り消せない面（二値）は、値なしを許す列でだけ `値なし` を出す。**境界の
    // `ColumnDescriptor` に nullable の欄が無い**ため、いまはつねに出す（材料が来たら写す。
    // 下の「申し送り」）。道を閉じると、値なしを許す列で値なしへ戻せない。
    const markup = markOf(
      editingModel([descriptorOfKind(0, "在庫", "Bool")], { row: 0, column: 0 }, "true"),
    );

    expect(markup).toContain("値なし");
    expect(markup).toContain("取消");
  });

  it("表を描いていないときは、編集を開かない（描かれていないセルは編集できない）", () => {
    const loading = initialGridScreenModel();

    expect(gridScreenEditStarted(loading, { row: 0, column: 0 }, "標本")).toBe(loading);
  });

  it("編集を開くと、現在位置と初期値が状態に入る（選択も内容も動かない）", () => {
    const before = readyModel(initialSelection());

    const after = gridScreenEditStarted(before, { row: 4, column: 2 }, "標本");

    if (after.state.status !== "ready" || before.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.editing).toEqual({
      position: { row: 4, column: 2 },
      initialText: "標本",
    });
    expect(after.state.selection).toEqual(before.state.selection);
    expect(after.state.summary).toEqual(before.state.summary);
    expect(after.attempt).toBe(before.attempt);
  });
});

describe("確定と取消（8.3。要件 3.3、3.6）", () => {
  it("確定の結果を反映すると、入力手段が閉じ、報告が出る（送った値の反映は境界が担う）", () => {
    const before = editingModel([descriptorOfKind(0, "名前", "Text")], { row: 4, column: 0 }, "標本");

    const after = settled(
      before,
      outcomeOf({ affected: [EDITED_ROW], revalidated_columns: [0] }),
    );

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.editing).toBeNull();
    expect(markOf(after)).not.toContain("jxcel-grid-editor");
    // 表はそのまま描かれている（表を失わない）。
    expect(markOf(after)).toContain("jxcel-grid-table");
  });

  it("取り消すと、入力手段が閉じ、報告も告知も動かない（境界へは何も送らない）", () => {
    // 前の確定で報告が付いている状態を作り、そのうえで開いて取り消す。
    const reported = settled(
      readyModel(initialSelection()),
      outcomeOf({
        affected: [EDITED_ROW],
        coercions: [{ cell: { row: EDITED_ROW, column: 0 }, before: "12.50", after: "12.5" }],
        revalidated_columns: [0],
      }),
    );
    const withNotice = gridScreenFailed(reported, "この操作はまだ使えません: 列の幅の変更");
    const opened = gridScreenEditStarted(withNotice, { row: 4, column: 0 }, "12.50");

    const after = gridScreenEditSettled(opened, { status: "cancelled" });

    if (after.state.status !== "ready" || opened.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.editing).toBeNull();
    // **文書を触っていないので、直近の確定の報告はまだ「直近」である。**
    expect(after.editReport).toEqual(opened.editReport);
    expect(after.editReport).not.toBeNull();
    expect(after.notice).toBe(opened.notice);
    // 内容の領域（選択・要約・可視行）も動かない。
    expect(after.state).toEqual({ ...opened.state, editing: null });
    expect(markOf(after)).toContain("jxcel-grid-coercions");
  });

  it("適用できなかったときは、入力手段を開いたままにして、理由を告知として出す", () => {
    // **適用されていないので、打たれている値を閉じて捨てる理由が無い。**
    const before = editingModel([descriptorOfKind(0, "名前", "Text")], { row: 4, column: 0 }, "打ちかけ");

    const after = gridScreenEditSettled(before, {
      status: "failed",
      message: "ドキュメントの失敗: 経路が不達である",
    });

    expect(after.state).toEqual(before.state);
    expect(after.notice).toBe("編集を適用できませんでした: ドキュメントの失敗: 経路が不達である");
    const markup = markOf(after);
    expect(markup).toContain("jxcel-grid-editor");
    expect(markup).toContain('value="打ちかけ"');
    expect(markup).toContain("jxcel-grid-notice");
  });
});

describe("型強制の提示（8.3。要件 3.4）", () => {
  it("型強制が起きたとき、変換が起きたことと、**変換前**の値を出す", () => {
    const after = settled(
      editingModel([descriptorOfKind(0, "単価", "Decimal")], { row: 4, column: 0 }, "12.50"),
      outcomeOf({
        affected: [EDITED_ROW],
        coercions: [{ cell: { row: EDITED_ROW, column: 0 }, before: "12.50", after: "12.5" }],
        revalidated_columns: [0],
      }),
    );

    const markup = markOf(after);
    expect(markup).toContain("jxcel-grid-edit-report");
    expect(markup).toContain("jxcel-grid-coercions");
    // 変換が起きたことが読める。
    expect(markup).toContain("型強制");
    // **前後の値はどちらも境界が運んだものであり、取り違えない。**
    expect(markup).toContain("変換前「12.50」");
    expect(markup).toContain("変換後「12.5」");
    expect(markup).not.toContain("変換前「12.5」");
    // 位置も読める（人が読む行と、機械が読む属性の両方）。
    expect(markup).toContain(`行「${EDITED_ROW}」の 1 列目`);
    expect(markup).toContain(`data-coercion-row="${EDITED_ROW}"`);
    expect(markup).toContain('data-coercion-column="0"');
    expect(markup).toContain('data-coercion-before="12.50"');
    expect(markup).toContain('data-coercion-after="12.5"');
  });

  it("型強制が起きなければ、型強制の提示は出ない（空の枠を出さない）", () => {
    const after = settled(
      editingModel([descriptorOfKind(0, "名前", "Text")], { row: 4, column: 0 }, "標本"),
      outcomeOf({ affected: [EDITED_ROW], revalidated_columns: [0] }),
    );

    const markup = markOf(after);
    expect(markup).not.toContain("jxcel-grid-coercions");
    expect(markup).not.toContain("jxcel-grid-violations");
    // 出すものが 1 つも無いので、報告の枠そのものを出さない。
    expect(markup).not.toContain("jxcel-grid-edit-report");
  });

  it("次の確定は、前の報告を置き換える（前の変換を残さない）", () => {
    const reported = settled(
      editingModel([descriptorOfKind(0, "単価", "Decimal")], { row: 4, column: 0 }, "12.50"),
      outcomeOf({
        affected: [EDITED_ROW],
        coercions: [{ cell: { row: EDITED_ROW, column: 0 }, before: "12.50", after: "12.5" }],
      }),
    );

    const after = settled(reported, outcomeOf({ affected: [EDITED_ROW] }));

    expect(after.editReport).toBeNull();
    expect(markOf(after)).not.toContain("jxcel-grid-coercions");
  });

  it("適用の結果が無い（`None`）ときも、報告は出ない（生成物の型がその腕を許す）", () => {
    // `GridEditResponse.outcome` が `None` であるのは「進める履歴が無かった」場合であり、適用では
    // つねに `Some` である（生成物の doc）。それでも型はその腕を許すので、**壊れない**ことを固定する。
    const after = settled(
      editingModel([descriptorOfKind(0, "名前", "Text")], { row: 4, column: 0 }, "標本"),
      null,
    );

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.editReport).toBeNull();
    expect(after.state.editing).toBeNull();
    expect(markOf(after)).not.toContain("jxcel-grid-edit-report");
  });

  it("報告は閉じられる（閉じても内容の領域も告知も動かない）", () => {
    const reported = settled(
      editingModel([descriptorOfKind(0, "単価", "Decimal")], { row: 4, column: 0 }, "12.50"),
      outcomeOf({
        affected: [EDITED_ROW],
        coercions: [{ cell: { row: EDITED_ROW, column: 0 }, before: "12.50", after: "12.5" }],
      }),
    );

    const dismissed = gridScreenEditReportDismissed(reported);

    expect(dismissed.editReport).toBeNull();
    expect(dismissed.state).toEqual(reported.state);
    expect(dismissed.notice).toBe(reported.notice);
    expect(markOf(dismissed)).not.toContain("jxcel-grid-edit-report");
  });
});

describe("違反の提示（8.3。要件 3.5。**提示の本体は 8.4**）", () => {
  it("適合しない値の確定は、違反として示され、入力手段は閉じる（値の保持は境界が担う）", () => {
    // **値が文書に残ることは Rust 側の契約である** — 判定する側は `WriteOrigin::Edit` を決して
    // 拒否せず、`grid_apply_edit` は適合しない値も破棄せずに返す（`src-tauri/src/commands/
    // grid.rs` の同コマンドの docs）。したがって画面がするのは「送る」「違反として示す」であり、
    // **値を捨てる経路を作らない**（送る中身は `cellEdit.test.ts` が固定する）。
    const after = settled(
      editingModel([descriptorOfKind(0, "数量", "Int")], { row: 4, column: 0 }, "存在しない名前"),
      outcomeOf({
        affected: [EDITED_ROW],
        violation_total: 2,
        violations: [
          { row: EDITED_ROW, column: 0, path: [] },
          { row: OTHER_ROW, column: 0, path: [] },
        ],
        revalidated_columns: [0],
      }),
    );

    const markup = markOf(after);
    // **失敗ではない**（適用は成功しており、値は文書にある）。告知も出さない。
    expect(markup).not.toContain("jxcel-grid-notice");
    expect(markup).not.toContain("jxcel-grid-failure");
    expect(markup).toContain("jxcel-grid-table");
    // 違反として示される（位置と、**シート全体**の総数）。
    expect(markup).toContain("jxcel-grid-violations");
    expect(markup).toContain('data-violation-total="2"');
    expect(markup).toContain('data-violation-count="2"');
    expect(markup).toContain("違反 2 件（シート全体の総数）");
    expect(markup).toContain(`行「${EDITED_ROW}」の 1 列目`);
    expect(markup).toContain(`行「${OTHER_ROW}」の 1 列目`);
    // 入力手段は閉じている（確定したので、面は用済みである）。
    expect(markup).not.toContain("jxcel-grid-editor");
  });

  it("位置を持たない違反でも、総数があればそれを出す（シート全体の数と偽らない）", () => {
    const after = settled(
      editingModel([descriptorOfKind(0, "数量", "Int")], { row: 4, column: 0 }, "9"),
      outcomeOf({ affected: [EDITED_ROW], violation_total: 1, revalidated_columns: [0] }),
    );

    const markup = markOf(after);
    expect(markup).toContain('data-violation-total="1"');
    expect(markup).toContain('data-violation-count="0"');
    // **シート全体**の数であることを言う（適応層が `GridSession::violation_total()` から写す）。
    expect(markup).toContain("違反 1 件（シート全体の総数）");
  });

  it("違反が残っていなければ、違反の提示は出ない", () => {
    const after = settled(
      editingModel([descriptorOfKind(0, "数量", "Int")], { row: 4, column: 0 }, "9"),
      outcomeOf({ affected: [EDITED_ROW], revalidated_columns: [0] }),
    );

    expect(markOf(after)).not.toContain("jxcel-grid-violations");
  });
});

// ===========================================================================
// 3. 画面の契約と登録簿（tasks.md 8.1 の受け入れ）
// ===========================================================================
describe("画面の契約（受け取るのは器が渡す引数だけ）", () => {
  it("`ScreenProps` だけで描ける（それ以外の props を要求しない）", () => {
    // 型の水準の証明: 登録簿が要求する形（`ComponentType<ScreenProps>`）へそのまま代入できる。
    // 余分な props を必須にすれば、この代入が型検査で落ちる（`npm run typecheck`）。
    const registered: ComponentType<ScreenProps> = GridScreen;

    const markup = renderToStaticMarkup(
      createElement(registered, { screenId: GRID_SCREEN_ID, navigate: () => undefined }),
    );

    // 読み込みの状態が描かれる（効果は走らないので、マウント直後の姿である）。
    expect(markup).toContain("jxcel-grid-screen");
    expect(markup).toContain("jxcel-grid-loading");
  });

  it("画面の実体は引数を 1 つも要求しない（余分な props を持たない）", () => {
    const source = SCREEN_SOURCE;
    expect(source).not.toBe("");
    expect(source).toContain("export function GridScreen(): ReactElement {");
  });
});

describe("画面登録簿", () => {
  it("グリッド画面がちょうど 1 件増え、初期画面は変わっていない", async () => {
    // `src/shell/Layout.tsx` は Vite の `define`（`__JXCEL_VERIFICATION__`）を参照する。
    // vitest は `vite.config.ts` を読まない（`vitest.config.ts`）ので、その識別子を大域へ
    // 置いてから読み込む（既定のビルドでは `false` に畳まれる値である）。
    vi.stubGlobal("__JXCEL_VERIFICATION__", false);
    const layout = await import("../../shell/Layout");
    const registry = layout.SHELL_SCREEN_REGISTRY;

    // 初期画面は 9.6 の空ウィンドウの画面のままである（8.1 は `initial` を動かさない）。
    expect(registry.initial).toBe(EMPTY_WINDOW_SCREEN_ID);

    // 増えたのはグリッド画面 1 件だけであり、並びは前の 5 件の後ろである。
    expect(registry.screens.map((screen) => screen.id)).toEqual([
      EMPTY_WINDOW_SCREEN_ID,
      layout.INITIAL_SCREEN_ID,
      DIAGNOSTICS_SCREEN_ID,
      TABLE_SMOKE_SCREEN_ID,
      EDITOR_SMOKE_SCREEN_ID,
      GRID_SCREEN_ID,
    ]);

    const mine = registry.screens.filter((screen) => screen.id === GRID_SCREEN_ID);
    expect(mine).toHaveLength(1);
    expect(mine[0]?.component).toBe(GridScreen);
    expect(mine[0]?.title).toBe("グリッド");

    // 検証専用の画面は登録簿に居ない（既定のビルドでも登録されない。8.1 が
    // `SHELL_SCREEN_REGISTRY` へ足してよいのはグリッド画面だけである）。
    const ids: readonly string[] = registry.screens.map((screen) => screen.id);
    expect(ids).not.toContain("smoke-glide-probe");
    expect(ids).not.toContain("smoke-port-probe");
  });
});

// ===========================================================================
// 4. 自前の配色を持たない
// ===========================================================================

describe("自前の配色を持たない（器が与える変数のみを参照する）", () => {
  /** 色の値そのもの（16 進・rgb・hsl）。**この画面の源では 1 つも現れてはならない。** */
  const COLOUR_LITERAL = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/;

  /**
   * 器の配色の参照（2 つの綴り）。本 module は **`APPEARANCE_VARS` の成員**（
   * `Layout.tsx` と同じ綴り）か、**生のカスタムプロパティ名**のどちらかしか書かない。
   */
  const MEMBER_REFERENCE = /var\(\$\{APPEARANCE_VARS\.([A-Za-z0-9_]+)\}\)/g;
  const RAW_REFERENCE = /var\((--jxcel-[a-z-]+)\)/g;

  it("色の値そのものを 1 つも書かない", () => {
    expect(SCREEN_SOURCE).not.toBe("");

    // 走査そのものが生きている（見本で確かめる）。**この 3 行が無いと、綴りを間違えた走査が
    // 何も見ないまま緑になる。**
    expect(COLOUR_LITERAL.test("backgroundColor: #c2185b")).toBe(true);
    expect(COLOUR_LITERAL.test("color: rgb(17, 205, 238)")).toBe(true);
    expect(COLOUR_LITERAL.test("border: 1px solid hsl(200, 50%, 50%)")).toBe(true);
    expect(COLOUR_LITERAL.test("backgroundColor: var(--jxcel-screen-panel)")).toBe(false);

    expect(COLOUR_LITERAL.test(SCREEN_SOURCE)).toBe(false);
  });

  it("参照するカスタムプロパティは器の 10 本だけである", () => {
    const allowed = new Set<string>(Object.values(APPEARANCE_VARS));
    const members = [...SCREEN_SOURCE.matchAll(MEMBER_REFERENCE)].map((match) => match[1] ?? "");
    const raws = [...SCREEN_SOURCE.matchAll(RAW_REFERENCE)].map((match) => match[1] ?? "");

    // **参照が 1 つも無い画面は「10 本だけを参照する」を自明に満たしてしまう。** 器の配色を
    // 実際に使っていることを先に確かめる。
    expect(members.length + raws.length).toBeGreaterThanOrEqual(4);
    expect(new Set([...members, ...raws]).size).toBeGreaterThanOrEqual(4);

    // 成員の綴りは `APPEARANCE_VARS` の鍵であり（鍵でなければ型検査も落ちる）、生の綴りは
    // 「10 本」の中にある。
    const keys = new Set(Object.keys(APPEARANCE_VARS));
    expect(members.every((name) => keys.has(name))).toBe(true);
    expect(raws.every((name) => allowed.has(name))).toBe(true);

    // **`var(` の出現がすべて上のどちらかであること**（別の配色の出所を混ぜていない）。
    // この 1 行が無いと、`var(--jxcel-…)` でも `APPEARANCE_VARS` の成員でもない参照
    // （たとえばライブラリのカスタムプロパティ）を黙って見落とす。
    expect(SCREEN_SOURCE.match(/\bvar\(/g)?.length ?? 0).toBe(members.length + raws.length);
  });
});

// ===========================================================================
// 源の走査（本 file だけが使う道具）
// ===========================================================================

/**
 * 画面の源の生のテキスト。`import.meta.glob` は Vite が変換時に解決するので、検査の環境を
 * node の API（`node:fs`）へ結び付けない（`displayState.test.ts` / `windowCache.test.ts` と
 * 同じ方針）。
 */
const SOURCES = import.meta.glob("/src/features/grid/GridScreen.tsx", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** グリッド画面の源（本 file の主役）。 */
const SCREEN_SOURCE = SOURCES["/src/features/grid/GridScreen.tsx"] ?? "";

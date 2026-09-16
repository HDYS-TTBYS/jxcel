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
  GridHistoryDirection,
  GridOpenResponse,
  GridSheetSummary,
  GridViolationResponse,
  GridViewResponse,
  GridViewSpec,
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
  applyGridView,
  createGridRendererSpec,
  createGridSurfaceCache,
  followSelection,
  gridScreenDeleteCancelled,
  gridScreenDeleteRequested,
  gridScreenDetailClosed,
  gridScreenDetailEditSettled,
  gridScreenDetailOpened,
  gridScreenEditReportDismissed,
  gridScreenColumnMoved,
  gridScreenColumnResized,
  gridScreenNextViolation,
  needsViewRefresh,
  gridScreenViolationReason,
  gridScreenEditSettled,
  gridScreenEditStarted,
  gridScreenFailed,
  gridScreenHistorySettled,
  gridScreenLoaded,
  gridScreenNoticeDismissed,
  gridScreenPasteSettled,
  gridScreenRetried,
  gridScreenRowOperationSettled,
  gridScreenSelectionChanged,
  gridScreenViewSettled,
  initialGridScreenModel,
  loadGridScreenState,
  type CellDetail,
  type GridScreenModel,
  type GridScreenState,
} from "./GridScreen";
import { EMPTY_GRID_VIEW } from "./gridClient";
import { createColumnSpace } from "./columnSpace";
import { DEFAULT_COLUMN_WIDTH, createDisplayState } from "./displayState";
import { applyViewOperation, drawnColumns, layoutKeyOf, rowOrderKeyOf } from "./viewOps";
import { withExpansion } from "./nestedInspector";
import { followTarget, initialSelection, selectionAt } from "./selection";
import type { GridClient } from "./gridClient";
import { applyRowOperation, type DeleteConfirmation } from "./rowOps";
import type { PastePayload } from "./clipboard";
import { applyPaste, createClipboardSurface } from "./clipboard";
import { applyHistory } from "./history";
import { settleCellEdit } from "./cellEdit";
import type { CellPosition, RendererSelection, VisibleSpan } from "./renderer/port";
import { nextViolation, reasonInRow, type ViolationPresentation } from "./violations";

// 窓の二進形式の言語をまたぐ固定（7.3）。**生のテキストとして**取り込む（`windowCache.test.ts`
// と同じ経路。ファイルを実行時に開く口は使わない — 検査の環境を node の API へ結び付けない）。
import windowFixtureText from "../../../crates/data-grid/tests/fixtures/window_protocol.txt?raw";

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

/**
 * 表示の指定を適用した応答。**導出後の列の構成・可視行数・シート全体の違反の総数**が検査に効く
 * （構成は要件 5.1 / 5.2 の見える結果の唯一の源であり、総数は要件 4.3 の提示の唯一の源である）。
 *
 * `columns` の既定は空である — **構成を渡さない検査は、展開を 1 つも含まない指定**（開いた直後の
 * 呼び出しなど、構成が変わらない場合）だけである。
 */
function derivedView(
  visibleRows: number,
  violationTotal = 0,
  columns: readonly ColumnDescriptor[] = [],
): GridViewResponse {
  return {
    context: CONTEXT,
    visible_rows: visibleRows,
    hidden_rows: 0,
    violation_total: violationTotal,
    columns: [...columns],
  };
}

/** 成功の封筒。 */
function ok<T>(data: T): IpcResult<T, IpcClientError> {
  return { status: "ok", data };
}

/** 失敗の封筒。 */
function err<T>(): IpcResult<T, IpcClientError> {
  return { status: "error", error: FAILURE };
}

/**
 * 偽の境界。**どの口が呼ばれたかを順に数える**（表を描かないとき開く呼び出しが起きないこと）。
 *
 * 違反の探索（`grid_find_violation`）は**起点の序数を順に記録する** — 4.4 の巡回が
 * 「いまの行の次」から探し、序数の解決で何度も問い合わせることの観測である。
 */
interface FakeClient extends GridClient {
  readonly calls: readonly string[];
  readonly edits: readonly GridEditCommand[];
  readonly searches: readonly number[];
  /** 履歴へ渡した向き（8.9。`grid_history` の引数である）。 */
  readonly directions: readonly GridHistoryDirection[];
}

function fakeClient(answers: {
  readonly state: IpcResult<DocumentStateResponse, IpcClientError>;
  readonly open?: IpcResult<GridOpenResponse, IpcClientError>;
  readonly view?: IpcResult<GridViewResponse, IpcClientError>;
  readonly edit?: IpcResult<GridEditResponse, IpcClientError>;
  /** 違反の探索の答え（既定は「見つからない」）。 */
  readonly search?: (from: number) => IpcResult<GridViolationResponse, IpcClientError>;
  /** 履歴の答え（既定は封筒の失敗である。**8.9 の検査だけが与える**）。 */
  readonly history?: (direction: GridHistoryDirection) => IpcResult<GridEditResponse, IpcClientError>;
}): FakeClient {
  const calls: string[] = [];
  const edits: GridEditCommand[] = [];
  const searches: number[] = [];
  const directions: GridHistoryDirection[] = [];
  return {
    calls,
    edits,
    searches,
    directions,
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
    findViolation: async (request) => {
      calls.push(`grid_find_violation:${request.direction}`);
      searches.push(request.from);
      return answers.search?.(request.from) ?? ok({ context: CONTEXT, violation: null });
    },
    readHistory: async (direction) => {
      calls.push(`grid_history:${direction}`);
      directions.push(direction);
      return answers.history?.(direction) ?? err<GridEditResponse>();
    },
  };
}

/**
 * 偽の索引（**境界の意味を写したもの**。`./violations.test.ts` の `searchOf` と同じ規則である）。
 * `from` 以降で最初の違反を返す（`from` は含む）。
 */
function indexOf(
  all: readonly { readonly ordinal: number; readonly row: string; readonly column: number; readonly reason: string }[],
): (from: number) => IpcResult<GridViolationResponse, IpcClientError> {
  return (from) => {
    const found = all.find((violation) => violation.ordinal >= from);
    if (found === undefined) {
      return ok<GridViolationResponse>({ context: CONTEXT, violation: null });
    }
    return ok<GridViolationResponse>({
      context: CONTEXT,
      violation: {
        location: { row: found.row, column: found.column, path: [] },
        reason: found.reason,
      },
    });
  };
}

/** 探索そのものが失敗する偽の索引（経路の不達）。 */
function failingIndex(): (from: number) => IpcResult<GridViolationResponse, IpcClientError> {
  return () => err<GridViolationResponse>();
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
      onColumnWidth: () => undefined,
      onColumnMove: () => undefined,
      onView: () => undefined,
      onSelectionChange: () => undefined,
      onEditStarted: () => undefined,
      onEditSettled: () => undefined,
      onNextViolation: () => undefined,
      onViolationRead: () => undefined,
      onExpansion: () => undefined,
      onDetailOpened: () => undefined,
      onDetailEditSettled: () => undefined,
      onDetailClosed: () => undefined,
      onDeleteRequested: () => undefined,
      onDeleteCancelled: () => undefined,
      onRowOperationSettled: () => undefined,
      onPasteSettled: () => undefined,
      onHistorySettled: () => undefined,
      onRefused: () => undefined,
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

/**
 * 列ごとの操作の並びに出た列の名前（**表示の順**。要件 5.1、5.2 の見える結果）。
 *
 * 表そのもの（移植口）は `node` の環境では走らない（効果が無い）ので、**描かれる列の並び**は
 * 状態から描かれるこの 1 行で読む（`./nestedInspector` が構成の列ごとに 1 件を出す）。
 * 8.5 と 8.8 の節が同じ行を読む（**同じ規則を 2 つ書かない**）。
 */
function controlNamesIn(markup: string): readonly string[] {
  return [...markup.matchAll(/data-column-control="\d+"[^>]*><span>([^<]*)<\/span>/g)].map(
    (match) => match[1] ?? "",
  );
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
      hiddenRows: 0,
      display: createDisplayState({ columnCount: 1 }),
      layoutKey: layoutKeyOf(createDisplayState({ columnCount: 1 })),
      rowOrderKey: rowOrderKeyOf(EMPTY_GRID_VIEW),
      selection: initialSelection(),
      editing: null,
      violationTotal: 0,
      violation: null,
      // 表示の指定（8.5）と世代（適用・表示の指定の変更で進む）。開いた直後はどちらも初期値である。
      view: EMPTY_GRID_VIEW,
      generation: 1,
      detail: null,
      pendingDelete: null,
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
  it("6 つの知らせがすべて結線され、列幅と列の移動は画面の遷移へそのまま渡る", async () => {
    const resized: [number, number][] = [];
    const moved: [number, number][] = [];
    const activated: [CellPosition, string][] = [];
    const refused: string[] = [];
    const pasted: PastePayload[] = [];
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
      // 複製と貼り付け（8.7）。**判断は `./clipboard` が持ち、ここはその 2 つの口を移植口へ
      // 渡すだけである**（8.6 が `./rowOps` の判断を表の面から呼んだのと同じ分担）。
      copyRange: (range) => ({ kind: "text", text: `複製:${String(range.start.row)}` }),
      pasteAt: (anchor, text) => ({
        kind: "send",
        payload: { anchor: { row: `id:${String(anchor.row)}`, column: anchor.column }, rows: [], text },
      }),
      sendPaste: async (payload) => {
        pasted.push(payload);
      },
      onRefused: (message) => {
        refused.push(message);
      },
      // 列幅と列の移動（8.8。要件 8.1、8.2）。**知らせは表示位置で来る**ので、そのまま渡る。
      onColumnResize: (displayPosition, width) => {
        resized.push([displayPosition, width]);
      },
      onColumnMove: (from, to) => {
        moved.push([from, to]);
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

    // **列幅と列の移動（要件 8.1、8.2）も結線された。**2 つの知らせは表示位置のまま画面の
    // 遷移へ渡る（移植口の実装が Glide の添字をそのまま位置として渡すので、写し直さない）。
    spec.onColumnResize(0, 200);
    spec.onColumnMove(1, 0);
    expect(resized).toEqual([[0, 200]]);
    expect(moved).toEqual([[1, 0]]);

    // **複製と貼り付け（要件 7.1、7.2）ももう「まだ使えない操作」ではない。**複製は文字列を
    // 返し（器がクリップボードへ書く）、貼り付けは 1 往復を起こす。
    await expect(
      spec.onCopy({ start: { row: 0, column: 0 }, end: { row: 0, column: 0 } }),
    ).resolves.toBe("複製:0");
    await expect(spec.onPaste({ row: 0, column: 0 }, "1\t2")).resolves.toBeUndefined();
    expect(pasted).toEqual([
      { anchor: { row: "id:0", column: 0 }, rows: [], text: "1\t2" },
    ]);
    expect(refused).toEqual([]);

    // 選択の知らせは**操作ではない**（8.2 が消費する。告知へは流さない）。
    spec.onSelectionChange(null);
    expect(resized).toEqual([[0, 200]]);
    expect(refused).toEqual([]);
  });

  it("送れない複製・貼り付けは、拒否して理由を告知へ流す（空文字を返さない）", async () => {
    const refused: string[] = [];
    const pasted: PastePayload[] = [];
    const spec = createGridRendererSpec({
      columns: [{ title: "名前", width: 120 }],
      rowCount: 3,
      selection: initialSelection(),
      rowMarkers: "clickable-number",
      getCell: () => ({ text: "標本", variant: "Text", violated: false, loading: false }),
      onSelectionChange: () => undefined,
      onVisibleSpanChange: () => undefined,
      onActivateEditor: () => undefined,
      copyRange: () => ({ kind: "refused", message: "選択の範囲のセルがまだ届いていないため、複製できません" }),
      pasteAt: () => ({ kind: "refused", message: "貼り付けの宛先の行の識別子がまだ届いていないため、貼り付けできません" }),
      sendPaste: async (payload) => {
        pasted.push(payload);
      },
      onRefused: (message) => {
        refused.push(message);
      },
      onColumnResize: () => undefined,
      onColumnMove: () => undefined,
    });

    // **空文字を返さない**（返せばクリップボードが空になり、利用者には「複製できた」と見える）。
    await expect(
      spec.onCopy({ start: { row: 0, column: 0 }, end: { row: 0, column: 0 } }),
    ).rejects.toThrow("複製できません");
    // **黙って捨てない**（捨てれば貼り付けが消える）。
    await expect(spec.onPaste({ row: 0, column: 0 }, "1\t2")).rejects.toThrow("貼り付けできません");
    expect(pasted).toEqual([]);
    expect(refused).toEqual([
      "選択の範囲のセルがまだ届いていないため、複製できません",
      "貼り付けの宛先の行の識別子がまだ届いていないため、貼り付けできません",
    ]);
  });

  it("貼り付けるものが無い（空のテキスト）ときは、往復を起こさずに成功する", async () => {
    const pasted: PastePayload[] = [];
    const spec = createGridRendererSpec({
      columns: [{ title: "名前", width: 120 }],
      rowCount: 3,
      selection: initialSelection(),
      rowMarkers: "clickable-number",
      getCell: () => ({ text: "標本", variant: "Text", violated: false, loading: false }),
      onSelectionChange: () => undefined,
      onVisibleSpanChange: () => undefined,
      onActivateEditor: () => undefined,
      copyRange: () => ({ kind: "text", text: "" }),
      pasteAt: () => ({ kind: "nothing" }),
      sendPaste: async (payload) => {
        pasted.push(payload);
      },
      onRefused: () => undefined,
      onColumnResize: () => undefined,
      onColumnMove: () => undefined,
    });

    await expect(spec.onPaste({ row: 0, column: 0 }, "")).resolves.toBeUndefined();
    expect(pasted).toEqual([]);
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

/**
 * 標本の構成の写像。**展開を含まないので恒等である**（表示の位置がそのまま文書の列）。
 * 違反の読み取りへ渡す写像はこれであり、8.5 の節だけが展開した構成の写像を使う。
 */
const SAMPLE_SPACE = createColumnSpace(SAMPLE_COLUMNS);

/**
 * 表を描いている状態（選択を指定して組む）。違反の総数と、いま出している違反の提示も指定できる
 * （既定はどちらも「無い」）。
 */
function readyModel(
  selection: RendererSelection,
  options: {
    readonly visibleRows?: number;
    readonly violationTotal?: number;
    readonly violation?: ViolationPresentation | null;
    /** 表示の指定（8.5。展開の状態をここへ入れる）。 */
    readonly view?: GridViewSpec;
    /** 世代（8.5。適用と表示の指定の変更で進む）。 */
    readonly generation?: number;
    /** 開いている詳細表示（8.5）。 */
    readonly detail?: CellDetail | null;
    /** 絞り込みで隠れている行の数（8.8。要件 8.7）。 */
    readonly hiddenRows?: number;
  } = {},
): GridScreenModel {
  const visibleRows = options.visibleRows ?? SAMPLE_ROWS;
  return gridScreenLoaded(initialGridScreenModel(), {
    status: "ready",
    sheet: "s1",
    summary: { columns: [...SAMPLE_COLUMNS], row_count: visibleRows },
    visibleRows,
    // 絞り込みで隠れている行（要件 8.7）。既定は 0 である（絞り込みが効いていない）。
    hiddenRows: options.hiddenRows ?? 0,
    ...displayFields(SAMPLE_COLUMNS.length),
    selection,
    editing: null,
    violationTotal: options.violationTotal ?? 0,
    violation: options.violation ?? null,
    // 8.5 の欄（表示の指定・世代・詳細表示）。既定は「開いた直後」である。
    view: options.view ?? EMPTY_GRID_VIEW,
    generation: options.generation ?? 1,
    detail: options.detail ?? null,
    // 8.6 の欄（削除の確認）。既定は「尋ねていない」である。
    pendingDelete: null,
  });
}

/**
 * 表を描く状態の**表示状態の 3 つの欄**（8.8。要件 8.1、8.2）。
 *
 * 状態を手で組む検査が使う（開く流れを通す検査は `loadGridScreenState` が組む）。**開いた直後と
 * 同じ値**である — 幅は 1 つも設定されておらず、並びは構成そのものである。
 */
function displayFields(columnCount: number): {
  readonly display: ReturnType<typeof createDisplayState>;
  readonly layoutKey: string;
  readonly rowOrderKey: string;
} {
  const display = createDisplayState({ columnCount });
  return { display, layoutKey: layoutKeyOf(display), rowOrderKey: rowOrderKeyOf(EMPTY_GRID_VIEW) };
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
      // 複製と貼り付け（8.7）は本検査の主題ではない（結線は下の 8.7 の検査が固定する）。
      copyRange: () => ({ kind: "text", text: "" }),
      pasteAt: () => ({ kind: "nothing" }),
      sendPaste: async () => undefined,
      onRefused: () => undefined,
      onColumnResize: () => undefined,
      onColumnMove: () => undefined,
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
      hiddenRows: 0,
      ...displayFields(columns.length),
      selection: initialSelection(),
      editing: null,
      violationTotal: 0,
      violation: null,
      view: EMPTY_GRID_VIEW,
      generation: 1,
      detail: null,
      pendingDelete: null,
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
// ===========================================================================
// 2.7 違反のバーと巡回（tasks.md 8.4。要件 4.1、4.2、4.3、4.4、4.6）
// ===========================================================================

/**
 * 違反の表示のうち、**画面が担う部分**を固定する。
 *
 *   - **4.1（区別できる形で示す）**は窓の印（`RenderCell.violated`）と移植口の実装の色であり、
 *     本 file は窓の印を移植口へ渡す仕様（`createGridRendererSpec`）までを見る。**色そのものは
 *     実物の起動で観測する**（下の「単体テストが観測しないもの」）
 *   - **4.2（理由）**、**4.4（次の違反への移動）**は境界への問い合わせの結果を
 *     [`gridScreenViolationReason`] / [`gridScreenNextViolation`] へ通して読む（画面が行う合成
 *     そのものである。8.3 の `settled` と同じ規律）
 *   - **4.3（総数）**は `grid_set_view` の応答が運ぶ**シート全体**の数を、表を描く状態が持ち、
 *     バーが常に出す
 *   - **4.6（解消の反映）**は、適用の結果が運ぶ新しい総数と、提示の取り下げである。**窓の印
 *     そのものの取り直しは 7.3 の記憶の検査が担う**（下の「単体テストが観測しないもの」）
 */
describe("違反のバーと巡回（8.4。要件 4.1〜4.4、4.6）", () => {
  it("開いたとき、シート全体の違反の総数がバーに出る（要件 4.3）", async () => {
    const client = fakeClient({
      state: ok(openDocument([sheetOf("s1", "標本シート", 3, SAMPLE_ROWS)])),
      open: ok(openedSheet({ columns: [...SAMPLE_COLUMNS], row_count: SAMPLE_ROWS })),
      view: ok(derivedView(SAMPLE_ROWS, 5)),
    });

    const state = await loadGridScreenState(client);

    if (state.status !== "ready") {
      throw new Error("表を描く状態にならなかった");
    }
    // 総数は `grid_set_view` の応答が運ぶ（**それ以外にシート全体の数の源は無い**）。
    expect(state.violationTotal).toBe(5);
    // 開いた直後は出すべき理由が 1 つも無い。
    expect(state.violation).toBeNull();
    // **総数を読むために探索を 1 度も呼ばない**（総数は検証の結果が既に持っている）。
    expect(client.searches).toEqual([]);

    const markup = markOfState(state);
    expect(markup).toContain("jxcel-grid-violation-bar");
    expect(markup).toContain('data-violation-total="5"');
    expect(markup).toContain("違反 5 件");
    // 表はそのまま描かれている。
    expect(markup).toContain("jxcel-grid-table");
  });

  it("表を描いていないときは、バーも出ない（総数の源が無い）", () => {
    const markup = markOfState({
      status: "no-rows",
      sheetName: "空のシート",
      columns: [descriptor(0, "名前")],
    });

    expect(markup).not.toContain("jxcel-grid-violation-bar");
    expect(markup).not.toContain("jxcel-grid-next-violation");
  });

  it("いまの行の違反の理由を、位置とともに出す（要件 4.2）", async () => {
    const model = readyModel(selectionAt({ row: 5, column: 1 }), { violationTotal: 3 });
    const client = fakeClient({
      state: err<DocumentStateResponse>(),
      search: indexOf([
        {
          ordinal: 5,
          row: OTHER_ROW,
          column: 1,
          reason: "値が 0 以上 100 以下の外の値である",
        },
      ]),
    });

    // 画面が行う合成そのものである（現在位置の行を起点に、いまの行の識別子で確かめる）。
    const reading = await reasonInRow({
      client,
      current: { row: 5, column: 1 },
      rowId: OTHER_ROW,
      space: SAMPLE_SPACE,
    });
    const after = gridScreenViolationReason(model, reading);

    if (after.state.status !== "ready" || model.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.violation).toEqual({
      kind: "reason",
      position: { row: 5, column: 1 },
      reason: "値が 0 以上 100 以下の外の値である",
    });
    // 起点は現在の行である。
    expect(client.searches).toEqual([5]);
    // **現在位置は動かない**（理由を読むことは移動ではない）。
    expect(after.state.selection).toEqual(model.state.selection);

    const markup = markOf(after);
    expect(markup).toContain("jxcel-grid-violation-reason");
    // 文言は境界が組み立てたものをそのまま出す（画面は 2 つ目の文言を作らない）。
    expect(markup).toContain("値が 0 以上 100 以下の外の値である");
    expect(markup).toContain("6 行 2 列目");
  });

  it("答えが後ろの行の違反なら、理由を出さない（いまの行の違反として貼らない）", async () => {
    const model = readyModel(selectionAt({ row: 2, column: 0 }), { violationTotal: 3 });
    const client = fakeClient({
      state: err<DocumentStateResponse>(),
      search: indexOf([
        { ordinal: 5, row: OTHER_ROW, column: 1, reason: "値が範囲の外である" },
      ]),
    });

    // 起点 2 の答えは序数 5 の違反であり、いまの行（`EDITED_ROW`）のものではない。
    const reading = await reasonInRow({
      client,
      current: { row: 2, column: 0 },
      rowId: EDITED_ROW,
      space: SAMPLE_SPACE,
    });
    const after = gridScreenViolationReason(model, reading);

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.violation).toBeNull();
    expect(markOf(after)).not.toContain("jxcel-grid-violation-reason");
  });

  it("理由の問い合わせが失敗したら、告知として出す（理由は出さない）", async () => {
    const model = readyModel(selectionAt({ row: 5, column: 1 }));
    const client = fakeClient({ state: err<DocumentStateResponse>(), search: failingIndex() });

    const reading = await reasonInRow({
      client,
      current: { row: 5, column: 1 },
      rowId: OTHER_ROW,
      space: SAMPLE_SPACE,
    });
    const after = gridScreenViolationReason(model, reading);

    expect(after.notice).toBe(
      "違反の理由を取得できませんでした: ドキュメントの失敗: 経路が不達である",
    );
    const markup = markOf(after);
    expect(markup).toContain("jxcel-grid-notice");
    expect(markup).not.toContain("jxcel-grid-violation-reason");
    // **表は失わない**（選ばれた 1 つの操作の失敗で内容を消さない。8.1 の表のまま）。
    expect(markup).toContain("jxcel-grid-table");
  });

  it("次の違反への移動は、表示範囲の外の違反へ現在位置を移す（要件 4.4）", async () => {
    // 10 万行のシートで、違反は 4 万行目に 1 件だけある。表示範囲は先頭の数十行である。
    const model = readyModel(selectionAt({ row: 0, column: 0 }), { visibleRows: 100_000 });
    const client = fakeClient({
      state: err<DocumentStateResponse>(),
      search: indexOf([
        { ordinal: 40_000, row: OTHER_ROW, column: 2, reason: "参照先の行が無い" },
      ]),
    });

    // いまの行は 0 である（起点は `./violations` がいまの行の次として決める）。
    const reading = await nextViolation({
      client,
      current: { row: 0, column: 0 },
      rowCount: 100_000,
      space: SAMPLE_SPACE,
    });
    const after = gridScreenNextViolation(model, reading);

    if (after.state.status !== "ready" || model.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(client.searches[0]).toBe(1);
    // **現在位置は違反の位置へ移る**（行の識別子ではなく、可視行の序数である）。
    expect(after.state.selection.current).toEqual({ row: 40_000, column: 2 });
    // 範囲は畳まれる（利用者が指していた範囲を持ち越さない）。
    expect(after.state.selection.range).toEqual({
      start: { row: 40_000, column: 2 },
      end: { row: 40_000, column: 2 },
    });
    // 移った先の違反の理由をそのまま出す。
    expect(after.state.violation).toEqual({
      kind: "reason",
      position: { row: 40_000, column: 2 },
      reason: "参照先の行が無い",
    });
    expect(after.notice).toBeNull();

    // **追随が `scrollTo` へ渡す位置は、移った現在位置そのものである。**表示範囲（先頭 40 行）
    // の外なので、追随は「動かす」と決める（要件 2.4 の判断は 8.2 の `followTarget` である）。
    const scrolled: CellPosition[] = [];
    const lowered: (RendererSelection | null)[] = [];
    followSelection(
      {
        setSelection: (selection) => {
          lowered.push(selection);
        },
        scrollTo: (position) => {
          scrolled.push(position);
        },
      },
      { rows: { start: 0, count: 40 }, columns: { start: 0, count: 3 } },
      after.state.selection,
    );
    expect(lowered).toEqual([after.state.selection]);
    expect(scrolled).toEqual([{ row: 40_000, column: 2 }]);
  });

  it("これ以上違反が無ければ、現在位置は動かず、その旨を出す（要件 4.4 の正常な結果）", async () => {
    const model = readyModel(selectionAt({ row: 3, column: 1 }), { violationTotal: 2 });
    const client = fakeClient({ state: err<DocumentStateResponse>() });

    const reading = await nextViolation({
      client,
      current: { row: 3, column: 1 },
      rowCount: SAMPLE_ROWS,
      space: SAMPLE_SPACE,
    });
    const after = gridScreenNextViolation(model, reading);

    if (after.state.status !== "ready" || model.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // **失敗ではない**（告知を出さない。内容も選択も動かさない）。
    expect(after.notice).toBeNull();
    expect(after.state.selection).toEqual(model.state.selection);
    expect(after.state.violationTotal).toBe(2);
    expect(after.state.violation).toEqual({ kind: "exhausted" });

    const markup = markOf(after);
    expect(markup).toContain("jxcel-grid-violation-exhausted");
    expect(markup).toContain("これ以上違反はありません");
    // **総数が 2 のままでも「尽きた」と言える**（総数は行を持たない違反も数えるが、探索は
    // それを移動先にしない。`violationBar.tsx` の module doc）。
    expect(markup).toContain('data-violation-total="2"');
  });

  it("巡回の問い合わせが失敗したら、現在位置を動かさず、理由を告知へ出す", async () => {
    const model = readyModel(selectionAt({ row: 3, column: 1 }));
    const client = fakeClient({ state: err<DocumentStateResponse>(), search: failingIndex() });

    const reading = await nextViolation({
      client,
      current: { row: 3, column: 1 },
      rowCount: SAMPLE_ROWS,
      space: SAMPLE_SPACE,
    });
    const after = gridScreenNextViolation(model, reading);

    expect(after.notice).toBe("次の違反を取得できませんでした: ドキュメントの失敗: 経路が不達である");
    if (after.state.status !== "ready" || model.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.selection).toEqual(model.state.selection);
    expect(markOf(after)).toContain("jxcel-grid-notice");
  });

  it("選択が動けば、その理由は取り下げられる（古い理由を新しい位置へ貼らない）", () => {
    const before = readyModel(selectionAt({ row: 4, column: 1 }), {
      violation: { kind: "reason", position: { row: 4, column: 1 }, reason: "値が範囲の外である" },
    });

    const moved = gridScreenSelectionChanged(before, selectionAt({ row: 5, column: 1 }));

    if (moved.state.status !== "ready" || before.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(moved.state.violation).toBeNull();
    expect(markOf(moved)).not.toContain("jxcel-grid-violation-reason");

    // 同じ位置を置き直すだけなら、理由は残る（動いていないので取り下げる理由が無い）。
    const same = gridScreenSelectionChanged(before, selectionAt({ row: 4, column: 1 }));
    if (same.state.status !== "ready" || before.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(same.state.violation).toEqual(before.state.violation);
  });

  it("総数は差し引きの推測ではなく、適用の結果が運ぶ数である（4.6）", () => {
    // 4.6 の「総数が減る」を**画面が 1 引く**実装でも満たせてしまうため、**減り方が -1 でない**
    // 適用で固定する（8.4 のレビューが実測: `violationTotal - 1` の変異が 302 件すべてを緑の
    // まま通った）。1 セルの編集で複数の違反が解消することはありうる（一意性の違反が 1 行で
    // 3 件あった場合など）ので、画面は**境界が運んだ数をそのまま**置かなければならない。
    const before = readyModel(selectionAt({ row: 4, column: 0 }), { violationTotal: 7 });
    const after = settled(
      before,
      outcomeOf({ affected: [EDITED_ROW], violation_total: 4, revalidated_columns: [0] }),
    );

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // 7 - 1 = 6 でも、据え置きの 7 でもない。
    expect(after.state.violationTotal).toBe(4);
  });

  it("編集で違反が解消されたとき、総数が減り、そのセルの提示が取り下げられる（要件 4.6）", () => {
    const before = readyModel(selectionAt({ row: 4, column: 0 }), {
      violationTotal: 3,
      violation: { kind: "reason", position: { row: 4, column: 0 }, reason: "値が範囲の外である" },
    });
    expect(markOf(before)).toContain("jxcel-grid-violation-reason");

    // 解消した適用（再検証した列に違反が残っていない）である。総数は 3 → 2 へ減る。
    const after = settled(
      before,
      outcomeOf({ affected: [EDITED_ROW], violation_total: 2, revalidated_columns: [0] }),
    );

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // **総数は適用の結果が運ぶシート全体の数である**（索引が差分で最新に保っている）。
    expect(after.state.violationTotal).toBe(2);
    // **そのセルの提示は取り下げられる。**
    expect(after.state.violation).toBeNull();

    const markup = markOf(after);
    expect(markup).toContain("jxcel-grid-violation-bar");
    expect(markup).toContain('data-violation-total="2"');
    expect(markup).not.toContain("jxcel-grid-violation-reason");
    // 表はそのまま描かれている。
    expect(markup).toContain("jxcel-grid-table");
  });

  it("適用の結果が無い（`None`）ときは、総数を動かさない（動かす根拠が無い）", () => {
    const before = readyModel(initialSelection(), { violationTotal: 1 });

    const after = settled(before, null);

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.violationTotal).toBe(1);
  });

  it("窓の印は移植口へそのまま渡る（要件 4.1 の結線）", () => {
    // 印を描くのは移植口の実装であり、画面がするのは**窓の印を渡すこと**である
    // （色そのものは実物の起動で観測する）。
    const spec = createGridRendererSpec({
      columns: [{ title: "名前", width: 120 }],
      rowCount: 1,
      selection: initialSelection(),
      rowMarkers: "clickable-number",
      getCell: () => ({ text: "標本", variant: "Text", violated: true, loading: false }),
      onSelectionChange: () => undefined,
      onVisibleSpanChange: () => undefined,
      onActivateEditor: () => undefined,
      copyRange: () => ({ kind: "text", text: "" }),
      pasteAt: () => ({ kind: "nothing" }),
      sendPaste: async () => undefined,
      onRefused: () => undefined,
      onColumnResize: () => undefined,
      onColumnMove: () => undefined,
    });

    expect(spec.getCell({ row: 0, column: 0 }).violated).toBe(true);
    // **移植口に色を運ぶ欄は無い**（8.4 の決定。`design.md`「8.4 が確定させたもの」）。
    expect(Object.keys(spec)).not.toContain("violationTheme");
  });
});

// ===========================================================================
// 2.6 入れ子の展開と詳細表示（8.5。要件 4.5、5.1〜5.7）
// ===========================================================================

/** 入れ子の列（`kind` は `Object` で、展開の可否を指定する）。 */
function nestedDescriptor(
  column: number,
  name: string,
  expandability: ColumnDescriptor["expandability"],
): ColumnDescriptor {
  return { column, path: [], name, kind: "Object", element_count: null, expandability };
}

/** 同一の型の並びの列（要素数の宣言つき。要件 5.6）。 */
function listDescriptor(column: number, name: string): ColumnDescriptor {
  return {
    column,
    path: [],
    name,
    kind: "Array",
    element_count: { items: "Int", min: 1, max: 8 },
    expandability: "leaf",
  };
}

/**
 * 入れ子を持つ標本の構成（4 列）。
 *
 * | 表示の位置 | 記述 | 期待する操作 |
 * |---|---|---|
 * | 0 | 葉（`Text`） | 無し |
 * | 1 | 展開できる | 展開 |
 * | 2 | 段数の上限に達している | **詳細表示へ** |
 * | 3 | 同一の型の並び | 詳細表示（要素数を示す） |
 */
const NESTED_COLUMNS: readonly ColumnDescriptor[] = [
  descriptor(0, "名前"),
  nestedDescriptor(1, "提供元", "available"),
  nestedDescriptor(2, "深い入れ子", "capped"),
  listDescriptor(3, "明細"),
];

/** 入れ子の**内側の位置**の列（親と同じ文書の列を指す。要件 5.1）。 */
function innerDescriptor(column: number, field: string, name: string): ColumnDescriptor {
  return {
    column,
    path: [{ segment: "Field", name: field }],
    name,
    kind: "Text",
    element_count: null,
    expandability: "leaf",
  };
}

/**
 * [`NESTED_COLUMNS`] の列 1（提供元）を 1 段展開したときの**導出後の構成**（境界が返すもの）。
 *
 * 親の列そのものは残らず（`view` 層の `push_column` は内側のフィールドへ置き換える）、内側の
 * 位置は**親と同じ文書の列**を指す — したがって表示の位置 1・2 はどちらも文書の列 1 であり、
 * **恒等ではない**（要件 8.6 の写像が要る理由）。
 */
const EXPANDED_NESTED_COLUMNS: readonly ColumnDescriptor[] = [
  descriptor(0, "名前"),
  innerDescriptor(1, "name", "提供元.name"),
  innerDescriptor(1, "code", "提供元.code"),
  nestedDescriptor(2, "深い入れ子", "capped"),
  listDescriptor(3, "明細"),
];

/** 入れ子を持つ標本を開いた応答。 */
function nestedOpened(): GridOpenResponse {
  return openedSheet({ columns: [...NESTED_COLUMNS], row_count: SAMPLE_ROWS });
}

/** 入れ子を持つ標本の表を描いている状態（選択と表示の指定を指定できる）。 */
function nestedModel(
  options: { readonly selection?: RendererSelection; readonly view?: GridViewSpec } = {},
): GridScreenModel {
  const visibleRows = SAMPLE_ROWS;
  return gridScreenLoaded(initialGridScreenModel(), {
    status: "ready",
    sheet: "s1",
    summary: { columns: [...NESTED_COLUMNS], row_count: visibleRows },
    visibleRows,
    hiddenRows: 0,
    ...displayFields(NESTED_COLUMNS.length),
    selection: options.selection ?? initialSelection(),
    editing: null,
    violationTotal: 0,
    violation: null,
    view: options.view ?? EMPTY_GRID_VIEW,
    generation: 1,
    detail: null,
    pendingDelete: null,
  });
}

describe("列ごとの操作（8.5。要件 5.1、5.2、5.4、5.6）", () => {
  it("展開できる列には展開の操作が出て、押された列の展開を指定として送る", async () => {
    const client = fakeClient({
      state: ok(openDocument([sheetOf("s1", "標本シート", 4, SAMPLE_ROWS)])),
      open: ok(nestedOpened()),
      // **境界は導出後の構成を返す**（提供元を展開した並び。要件 5.1）。
      view: ok(derivedView(SAMPLE_ROWS, 0, EXPANDED_NESTED_COLUMNS)),
    });

    const state = await loadGridScreenState(client);
    expect(state.status).toBe("ready");

    const markup = markOfState(state);
    expect(markup).toContain("jxcel-grid-column-controls");
    expect(markup).toContain("jxcel-grid-expansion-1");
    expect(markup).toContain(">展開<");
    // 段数の上限の列には展開の操作を出さない（それ以上降りられない）。
    expect(markup).not.toContain("jxcel-grid-expansion-2");

    // **送るのは完全な記述である**（並べ替え・絞り込みも載る）。開いた直後の 1 回のあとに
    // もう 1 度呼ばれる（8.1 の「開いた直後の 1 回」に足して 2 回目である）。
    const settlement = await applyGridView(
      client,
      withExpansion(EMPTY_GRID_VIEW, { column: 1, expanded: true, depth: 1 }),
    );

    expect(client.calls).toEqual([
      "document_state",
      "grid_open_sheet:s1",
      "grid_set_view:000",
      "grid_set_view:001",
    ]);
    expect(settlement).toEqual({
      status: "applied",
      view: withExpansion(EMPTY_GRID_VIEW, { column: 1, expanded: true, depth: 1 }),
      // **境界が運んだ導出後の構成をそのまま持ち帰る**（要件 5.1）。
      columns: EXPANDED_NESTED_COLUMNS,
      visibleRows: SAMPLE_ROWS,
      hiddenRows: 0,
      violationTotal: 0,
    });
  });

  it("表示の指定を適用した結果を状態へ反映する（構成と世代が変わる）", async () => {
    const before = readyModel(initialSelection(), { generation: 1 });
    const view = withExpansion(EMPTY_GRID_VIEW, { column: 1, expanded: true, depth: 1 });

    const after = gridScreenViewSettled(before, {
      status: "applied",
      view,
      // 導出前の 3 列（`SAMPLE_COLUMNS`）を受け取った場合。
      columns: SAMPLE_COLUMNS,
      visibleRows: SAMPLE_ROWS,
      hiddenRows: 0,
      violationTotal: 3,
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.view).toEqual(view);
    // **境界は世代を運ばない**（7.3 の申し送り）ので画面が数える。`set_view` はつねに進める
    // （`api.rs` の `set_view`）— 数えないと、以後の窓の要求が古い世代を名乗り、空の窓が返る。
    expect(after.state.generation).toBe(2);
    // 応答が運ぶ可視行数と違反の総数も反映する（絞り込みが効けば可視行数は変わる）。
    expect(after.state.visibleRows).toBe(SAMPLE_ROWS);
    expect(after.state.violationTotal).toBe(3);
    // 構成は**応答が運んだもの**になる（同じ並びなので値は変わらない）。
    expect(after.state.summary.columns).toEqual(SAMPLE_COLUMNS);
  });

  it("表示の指定を適用できなかったときは、告知を出して前の指定と世代を残す", async () => {
    const before = readyModel(initialSelection(), { generation: 1 });
    const view = withExpansion(EMPTY_GRID_VIEW, { column: 1, expanded: true, depth: 1 });
    const pending = gridScreenViewSettled(before, {
      status: "applied",
      view,
      columns: SAMPLE_COLUMNS,
      visibleRows: SAMPLE_ROWS,
      hiddenRows: 0,
      violationTotal: 0,
    });

    // **失敗は `applyGridView` が作る**（文言もそこが組み立てる）。
    const failure = await applyGridView(
      fakeClient({ state: err<DocumentStateResponse>() }),
      view,
    );
    const after = gridScreenViewSettled(pending, failure);

    if (after.state.status !== "ready" || pending.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // **適用されていないので、状態は 1 つも動かさない**（世代も進めない — 進めると、窓の要求が
    // 存在しない世代を名乗る。進んだのは直前の適用の 1 つだけである）。
    expect(after.state.view).toEqual(pending.state.view);
    expect(after.state.generation).toBe(pending.state.generation);
    expect(after.notice).toContain("表示の指定を適用できませんでした");
  });

  it("展開の状態は走査で失われない（列ごとに保たれ、指定は完全な記述である）", () => {
    // 開いた直後の状態から、2 列を展開し、そのあと**走査に相当する遷移**（現在位置の移動・
    // 確定の反映・違反の提示の取り下げ）を通す。
    const view = withExpansion(
      withExpansion(EMPTY_GRID_VIEW, { column: 1, expanded: true, depth: 1 }),
      { column: 3, expanded: true, depth: 1 },
    );
    const opened = gridScreenViewSettled(nestedModel(), {
      status: "applied",
      view,
      // 構成そのものはこの検査の対象ではない（見るのは展開の状態の保持である）。
      columns: EXPANDED_NESTED_COLUMNS,
      visibleRows: SAMPLE_ROWS,
      hiddenRows: 0,
      violationTotal: 0,
    });

    let traversed = gridScreenSelectionChanged(opened, selectionAt({ row: 12, column: 3 }));
    traversed = gridScreenSelectionChanged(traversed, selectionAt({ row: 0, column: 0 }));
    traversed = gridScreenEditSettled(traversed, {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], revalidated_columns: [0] }),
    });
    traversed = gridScreenViolationReason(traversed, { kind: "cleared" });

    if (traversed.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(traversed.state.view.expansion).toEqual([
      { column: 1, expanded: true, depth: 1 },
      { column: 3, expanded: true, depth: 1 },
    ]);
    // もう 1 列を展開すると、**前の 2 つを載せたまま** 3 つ目が足される（押された 1 件だけを
    // 送ると、ドメインは要求に現れない展開を折りたたみへ戻す）。
    const three = withExpansion(traversed.state.view, { column: 0, expanded: true, depth: 1 });
    expect(three.expansion).toHaveLength(3);
    expect(three.expansion.map((entry) => entry.column)).toEqual([1, 3, 0]);
  });

  it("段数の上限に達した列は、詳細表示へ誘導する（要件 5.4）", () => {
    const markup = markOf(nestedModel());

    // 記述の印（`expandability` が `capped`）から「詳細表示へ」が出る。
    expect(markup).toContain("jxcel-grid-detail-2");
    expect(markup).toContain("詳細表示へ");
    // 展開できる列の入口は「詳細表示」である（誘導ではない）。
    expect(markup).toContain(">詳細表示<");
    // 葉の列（`element_count` も無い）には詳細表示の入口を出さない。
    expect(markup).not.toContain("jxcel-grid-detail-0");
  });

  it("同一の型の並びの列では、要素数が示される（要件 5.6）", () => {
    const markup = markOf(nestedModel());

    expect(markup).toContain("要素数: 1..=8（要素の型: Int）");
  });
});

describe("詳細表示（8.5。要件 4.5、5.5、5.7）", () => {
  it("開くと、その位置の入れ子の詳細が出る（閉じると消える）", () => {
    const before = nestedModel();
    const position: CellPosition = { row: 4, column: 2 };

    const opened = gridScreenDetailOpened(before, position);

    if (opened.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // **どの位置を詳しく見ているかが状態に載る**（描かれるものは表の面の中であり、窓の記憶を
    // 持つ側が描く — 状態に載らないと、開いていることを検査から観測できない）。
    expect(opened.state.detail).toEqual({ position, edit: 0 });
    const markup = markOf(opened);
    expect(markup).toContain("jxcel-grid-nested-inspector");
    expect(markup).toContain('data-detail-row="4"');
    expect(markup).toContain('data-detail-column="2"');

    const closed = gridScreenDetailClosed(opened);
    if (closed.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(closed.state.detail).toBeNull();
    expect(markOf(closed)).not.toContain("jxcel-grid-nested-inspector");
  });

  it("詳細表示の中の編集は、確定・取消・失敗のいずれもセルの編集と同じ規律で扱われる", () => {
    // **同じ遷移（`gridScreenEditSettled`）を通る** — 違うのは、確定・取消で編集の面を作り直す
    // 鍵が進むことだけである（面は打たれた文字を自分の状態に持つので、作り直さないと確定した
    // のに打ちかけの文字が残る）。
    const detail: CellDetail = { position: { row: 4, column: 2 }, edit: 0 };
    const opened = gridScreenDetailOpened(nestedModel(), detail.position);

    const cancelled = gridScreenDetailEditSettled(opened, { status: "cancelled" });
    const applied = gridScreenDetailEditSettled(opened, {
      status: "applied",
      // 変換も違反も無い確定では報告を出さない（`reportOf` の規則）ので、違反を 1 つ残した
      // 結果を渡す（**提示が出ることを見る**）。
      outcome: outcomeOf({
        affected: [EDITED_ROW],
        violation_total: 1,
        violations: [{ row: EDITED_ROW, column: 2, path: [] }],
        revalidated_columns: [2],
      }),
    });
    const failed = gridScreenDetailEditSettled(opened, {
      status: "failed",
      message: "ドキュメントの失敗: 経路が不達である",
    });

    if (
      cancelled.state.status !== "ready" ||
      applied.state.status !== "ready" ||
      failed.state.status !== "ready"
    ) {
      throw new Error("表を描く状態でなくなった");
    }
    // 取消: 文書は 1 バイトも変わらず、面は初期状態へ戻る（告知も報告も動かない）。
    expect(cancelled.state.detail).toEqual({ position: detail.position, edit: 1 });
    expect(cancelled.notice).toBeNull();
    expect(cancelled.editReport).toBeNull();
    // 確定: 報告が出て、面は初期状態へ戻る（要件 3.4、3.5 の提示はセルの編集と同じである）。
    expect(applied.state.detail).toEqual({ position: detail.position, edit: 1 });
    expect(applied.editReport).not.toBeNull();
    // 適用のあとは世代が進む（以後の窓の要求が古い世代を名乗らない）。
    expect(applied.state.generation).toBe(2);
    // 失敗: **面は開いたままである**（適用されていないので、打たれている値を捨てる理由が無い）。
    expect(failed.state.detail).toEqual({ position: detail.position, edit: 0 });
    expect(failed.notice).toContain("編集を適用できませんでした");
  });
});

describe("表の窓と列の空間（8.5。要件 5.1、8.6）", () => {
  it("窓の記憶は、構成が定める列の写像で組まれる（恒等を仮定しない）", () => {
    // 表の面が組む窓の記憶を**同じ関数で**組み、その写像を読む（効果の中にしか無いと、
    // 「画面が恒等で組んでいないこと」を検査から観測できない）。
    const summary: GridSheetSummary = {
      columns: [
        { column: 0, path: [{ segment: "Field", name: "city" }], name: "place.city", kind: "Text", element_count: null, expandability: "leaf" },
        { column: 0, path: [{ segment: "Field", name: "zip" }], name: "place.zip", kind: "Text", element_count: null, expandability: "leaf" },
        { column: 1, path: [], name: "name", kind: "Text", element_count: null, expandability: "leaf" },
      ],
      row_count: SAMPLE_ROWS,
    };
    const client = fakeClient({ state: err<DocumentStateResponse>() });

    const cache = createGridSurfaceCache({
      sheet: "s1",
      // **描かれる列の並び**（表示順）を渡す（8.8。表示上の列順が写像を決める）。
      columns: summary.columns,
      visibleRows: SAMPLE_ROWS,
      generation: 1,
      client,
    });

    // **表示の位置 2 は文書の列 1 である**（恒等なら 2 になる）。読みも書きもこの 1 つの写像を使う
    // （`WindowCache.getCell` と `./cellEdit` の宛先）。
    expect(cache.documentColumn({ row: 0, column: 0 })).toBe(0);
    expect(cache.documentColumn({ row: 0, column: 1 })).toBe(0);
    expect(cache.documentColumn({ row: 0, column: 2 })).toBe(1);
    // 恒等を仮定していないことは、同じ構成から作った写像そのもので確かめる（`./columnSpace`）。
    expect(createColumnSpace(summary.columns).documentColumn(2)).toBe(1);
  });

  it("展開を指定すると描かれる列が入れ子の内側へ広がり、折りたたむと元へ戻る（要件 5.1、5.2）", async () => {
    // **境界の写し**: `view` の展開に応じて**導出後**の構成を返す（列 1 = 提供元 が展開されて
    // いれば内側の位置が現れ、展開が無ければ宣言の列がそのまま並ぶ。`view` 層の `derive_layout`
    // と同じ 2 通りである）。
    const scripted: GridClient = {
      ...fakeClient({
        state: ok(openDocument([sheetOf("s1", "標本シート", 4, SAMPLE_ROWS)])),
        open: ok(nestedOpened()),
      }),
      setView: async (view) =>
        ok(
          derivedView(
            SAMPLE_ROWS,
            0,
            view.expansion.some((state) => state.column === 1 && state.expanded)
              ? EXPANDED_NESTED_COLUMNS
              : NESTED_COLUMNS,
          ),
        ),
    };

    const state = await loadGridScreenState(scripted);
    if (state.status !== "ready") {
      throw new Error("表を描く状態にならなかった");
    }
    const before = gridScreenLoaded(initialGridScreenModel(), state);
    // 導出前: 描かれる列は宣言の 4 本であり、内側の位置は 1 つも現れない。
    expect(controlNamesIn(markOf(before))).toEqual(["名前", "提供元", "深い入れ子", "明細"]);

    const view = withExpansion(EMPTY_GRID_VIEW, { column: 1, expanded: true, depth: 1 });
    const after = gridScreenViewSettled(before, await applyGridView(scripted, view));
    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }

    // **展開した列の内側の位置が、描かれる列として現れる**（要件 5.1）。親の列そのものは残らない
    // （`view` 層の `push_column` が内側のフィールドへ置き換える）。
    expect(controlNamesIn(markOf(after))).toEqual([
      "名前",
      "提供元.name",
      "提供元.code",
      "深い入れ子",
      "明細",
    ]);

    // **写像も新しい構成で組まれる**（窓の読みと編集の宛先の唯一の源。要件 8.6）。表示の位置 1・2
    // はどちらも文書の列 1 を指す（恒等なら 2 になる）。
    const cache = createGridSurfaceCache({
      sheet: "s1",
      columns: after.state.summary.columns,
      visibleRows: after.state.visibleRows,
      generation: after.state.generation,
      client: scripted,
    });
    expect(cache.documentColumn({ row: 0, column: 1 })).toBe(1);
    expect(cache.documentColumn({ row: 0, column: 2 })).toBe(1);
    expect(cache.documentColumn({ row: 0, column: 3 })).toBe(2);
    expect(createColumnSpace(after.state.summary.columns).documentColumn(2)).toBe(1);

    // 折りたたみ: 境界が導出前の並びを返せば、**描かれる列も元の 1 本へ戻る**（要件 5.2）。
    const collapsed = gridScreenViewSettled(
      after,
      await applyGridView(scripted, EMPTY_GRID_VIEW),
    );
    if (collapsed.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(controlNamesIn(markOf(collapsed))).toEqual(["名前", "提供元", "深い入れ子", "明細"]);
    // 写像も元へ戻る（表示の位置 2 は文書の列 2 である）。
    expect(createColumnSpace(collapsed.state.summary.columns).documentColumn(2)).toBe(2);

    // **縮んだ構成へ現在位置が寄る**（要件 2.1、5.2）。展開して**最後の列**へ移ってから
    // 折りたたむと、列は 5 → 4 へ減るので、寄せが無ければ現在位置は**描かれる表の外**に残り、
    // 数え上げの行の文言と描かれている列が食い違う（境界の修復のレビューが実測）。
    const moved = gridScreenSelectionChanged(after, selectionAt({ row: 0, column: 4 }));
    const shrunk = gridScreenViewSettled(
      moved,
      await applyGridView(scripted, EMPTY_GRID_VIEW),
    );
    if (shrunk.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // 最後の列（表示の位置 3）へ寄っている。
    expect(shrunk.state.selection.current.column).toBe(3);
    // 文言の側も実際の列数と一致する（食い違わない）。
    expect(markOf(shrunk)).toContain("現在位置 1 行 4 列");
  });
});

/**
 * **展開した構成の違反の位置**（8.4 と 8.5 の合わせ。要件 4.2、4.4）。
 *
 * 境界の `GridViolationLocation.column` は**文書の列**であり、提示が名乗る列（バーの「M 列目」）
 * と巡回の着地点は**表示の位置**である（`./violations` の module doc）。8.5 の展開が描かれる
 * 並びを変えたため、写像を通さないと**描かれている列と違う列**を名乗る — 実測に使った並び
 * （[`EXPANDED_NESTED_COLUMNS`]。表示の位置 1・2 が同じ文書の列 1 を指す）で固定する。
 */
describe("展開した構成の違反の位置（要件 4.2、4.4。8.5 との合わせ）", () => {
  /** 4 番目に描かれる列（深い入れ子 ＝ 文書の列 2）に違反がある標本。 */
  const VIOLATED = [
    { ordinal: 5, row: OTHER_ROW, column: 2, reason: "参照先の行が無い" },
  ] as const;

  /**
   * 提供元を 1 段展開した表を描いている状態（**境界が返す導出後の構成**を採用した状態である）。
   * 画面の効果（[`GridSurface`] のマップ）と、巡回の経路が組む写像は、どちらも
   * `summary.columns` から引く — 検査も同じ 1 本から引く。
   */
  function expandedModel(): GridScreenModel {
    return gridScreenViewSettled(nestedModel(), {
      status: "applied",
      view: withExpansion(EMPTY_GRID_VIEW, { column: 1, expanded: true, depth: 1 }),
      columns: EXPANDED_NESTED_COLUMNS,
      visibleRows: SAMPLE_ROWS,
      hiddenRows: 0,
      violationTotal: 1,
    });
  }

  it("巡回は違反しているセル（4 番目に描かれる列）へ着く", async () => {
    const opened = expandedModel();
    if (opened.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    const client = fakeClient({ state: err<DocumentStateResponse>(), search: indexOf(VIOLATED) });

    const reading = await nextViolation({
      client,
      current: opened.state.selection.current,
      rowCount: opened.state.visibleRows,
      space: createColumnSpace(opened.state.summary.columns),
    });
    const after = gridScreenNextViolation(opened, reading);

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // **着地点は表示の位置 3 である**（文書の列 2 ではない。文書の列のままだと、3 番目に
    // 描かれる列（提供元.code）＝違反していないセルへ移る）。
    expect(after.state.selection.current).toEqual({ row: 5, column: 3 });
    expect(after.state.violation).toEqual({
      kind: "reason",
      position: { row: 5, column: 3 },
      reason: "参照先の行が無い",
    });

    // バーが名乗るのも**4 番目**である（`./violationBar` は表示の位置へ 1 を足して書く）。
    const markup = markOf(after);
    expect(markup).toContain('data-violation-column="3"');
    expect(markup).toContain("6 行 4 列目");

    // 追随（要件 2.4）の宛先も**表示の位置 3** である — 可視の区間の外なら、8.2 の
    // `followTarget` がこの位置を `scrollTo` へ渡す（文書の列のままだと 1 本左へ走査する）。
    const span: VisibleSpan = { rows: { start: 0, count: 10 }, columns: { start: 0, count: 3 } };
    expect(followTarget(span, after.state.selection)).toEqual({ row: 5, column: 3 });
  });

  it("理由の提示も、4 番目に描かれる列を名乗る", async () => {
    const opened = expandedModel();
    if (opened.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    const client = fakeClient({ state: err<DocumentStateResponse>(), search: indexOf(VIOLATED) });

    // 画面の効果（`GridSurface` の `refreshViolation`）が行う合成そのものである。
    const reading = await reasonInRow({
      client,
      current: { row: 5, column: 3 },
      rowId: OTHER_ROW,
      space: createColumnSpace(opened.state.summary.columns),
    });
    const after = gridScreenViolationReason(opened, reading);

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    const markup = markOf(after);
    expect(markup).toContain("jxcel-grid-violation-reason");
    expect(markup).toContain("6 行 4 列目");
    expect(markup).not.toContain("6 行 3 列目");
  });
});

// ===========================================================================
// 8.6 行の追加・削除・複製（要件 6.1、6.2、6.3、6.5）
// ===========================================================================

/**
 * **行数の提示**（要件 6.2。
 * 画面の位置の提示が直ちに更新されること）。
 *
 * 表そのもの（移植口）は `node` の環境では走らないので、**提示は 2 つに分けて読む** —
 * 表の面が出す行数（この検査）と、器が描く行の番号（実起動。9.2）。前者が据え置かれると、
 * 後者は何も変わらない。
 */
describe("行の増減の反映（8.6。要件 6.2、6.5）", () => {
  /** 3 つの操作が画面に出ている（要件 6.1、6.2、6.3 の入口である）。 */
  function rowOperationIdsIn(markup: string): readonly string[] {
    return [
      "jxcel-grid-insert-row",
      "jxcel-grid-delete-rows",
      "jxcel-grid-duplicate-rows",
    ].filter((id) => markup.includes(id));
  }

  it("行の操作と、提示する行数が画面に出る（要件 6.1、6.2、6.3）", () => {
    const markup = markOf(readyModel(initialSelection()));

    expect(markup).toContain("jxcel-grid-row-ops");
    expect(rowOperationIdsIn(markup)).toHaveLength(3);
    // **行数は開いたときの要約である**（`grid_open_sheet` の `GridSheetSummary.row_count`）。
    expect(markup).toContain('data-row-count="20"');
    expect(markup).toContain("行数 20");
  });

  it("適用のあと、提示する行数と窓が覆う行数が新しい数になる", () => {
    const before = readyModel(initialSelection());

    const after = gridScreenRowOperationSettled(before, {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 7, violation_total: 2 }),
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // **窓が覆う行数**（移植口へ渡す行数）と**提示する行数**（要約）の両方が動く。片方だけでは
    // 足りない — 前者だけなら提示が古い数のままになり、後者だけなら増えた行を永久に読めない。
    expect(after.state.visibleRows).toBe(7);
    expect(after.state.summary.row_count).toBe(7);
    // 違反の総数は適用の応答が運ぶ数で置き換わり、いまの提示は取り下げる（要件 4.6）。
    expect(after.state.violationTotal).toBe(2);
    expect(after.state.violation).toBeNull();
    // 世代は進む（以後の窓の要求が古い世代を名乗らない。`api.rs` の `apply` と同じ規則）。
    expect(after.state.generation).toBe(2);

    const markup = markOf(after);
    expect(markup).toContain('data-row-count="7"');
    expect(markup).toContain("行数 7");
  });

  it("行数が減ったとき、現在位置と選択が新しい表の範囲へ寄る", () => {
    // 行 7..9 の選択（現在位置は 9）。行が 3 件になれば、**描かれる表の外**である。
    const before = readyModel({
      current: { row: 9, column: 0 },
      range: { start: { row: 7, column: 0 }, end: { row: 9, column: 0 } },
    });

    const after = gridScreenRowOperationSettled(before, {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 3 }),
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // 寄せが無ければ、数え上げの行は「現在位置 10 行」と名乗るのに表は 3 行しか無い — 利用者に
    // 見える食い違いである（8.5 の列の側で実測したのと同じ形の欠陥）。
    expect(after.state.selection.current).toEqual({ row: 2, column: 0 });
    expect(after.state.selection.range).toEqual({
      start: { row: 2, column: 0 },
      end: { row: 2, column: 0 },
    });
    const markup = markOf(after);
    expect(markup).toContain("現在位置 3 行 1 列");
    expect(markup).toContain('data-current-row="2"');
  });

  it("行が消えたら、その行に開いていた面は閉じる", () => {
    // 行 9 を編集中で、同じ行の詳細表示も開いている（消えるのはその行である）。
    const editing = gridScreenEditStarted(
      gridScreenDetailOpened(readyModel(initialSelection()), { row: 9, column: 0 }),
      { row: 9, column: 0 },
      "打ちかけ",
    );

    const after = gridScreenRowOperationSettled(editing, {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 3 }),
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // **描かれていないセルを編集・表示しない**（面は位置を持つので、消えた行を指したまま残ると
    // 確定が行の識別子を引けず、「まだ届いていない」という誤った理由になる）。
    expect(after.state.editing).toBeNull();
    expect(after.state.detail).toBeNull();
  });

  it("行が 1 件も無いときは、位置の提示が行数を偽らない", () => {
    // すべての行を消した（要件 6.2 の行き着く先である）。**表を描く状態のままにする** — ここで
    // `no-rows` へ移ると、入り口（開いたときの提示）は同じでも、**行を足す手段が無くなる**
    // （要件 6 の目的は記録を足し続けられることである）。
    const emptied = gridScreenRowOperationSettled(readyModel(initialSelection()), {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 0 }),
    });

    if (emptied.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(emptied.state.visibleRows).toBe(0);
    const markup = markOf(emptied);
    expect(markup).toContain('data-row-count="0"');
    // **「現在位置 1 行 1 列」と名乗らない**（描かれている行は 0 件である）。行を足す操作は
    // 残る（`at == 行数 == 0` への追加は妥当である）。
    expect(markup).toContain("行がありません");
    expect(markup).not.toContain("現在位置");
    expect(rowOperationIdsIn(markup)).toHaveLength(3);
  });
});

describe("削除の確認（8.6。要件 6.5）", () => {
  /** 削除する行数を示す確認（**閾値を超えたときだけ出る**。閾値の判断は `./rowOps`）。 */
  const CONFIRMATION: DeleteConfirmation = { first: 0, last: 11, count: 12 };

  it("確認を求めると、削除する行数を示す確認が出る", () => {
    const asked = gridScreenDeleteRequested(readyModel(initialSelection()), CONFIRMATION);

    if (asked.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(asked.state.pendingDelete).toEqual(CONFIRMATION);

    const markup = markOf(asked);
    expect(markup).toContain("jxcel-grid-delete-confirm");
    // **削除する行数を示す**（要件 6.5 の本体である）。
    expect(markup).toContain("12 行");
    expect(markup).toContain("jxcel-grid-delete-confirm-yes");
    expect(markup).toContain("jxcel-grid-delete-confirm-cancel");
  });

  it("確認は出ていないときは、削除する行数を名乗らない", () => {
    const markup = markOf(readyModel(initialSelection()));
    expect(markup).not.toContain("jxcel-grid-delete-confirm");
  });

  it("取り消すと、確認を取り下げるだけである（文書も表示も動かない）", () => {
    // 確定の報告と告知が付いている状態から始める（**取り消しがそれらを動かさない**ことまで見る）。
    const reported = settled(
      readyModel(initialSelection()),
      outcomeOf({
        affected: [EDITED_ROW],
        coercions: [{ cell: { row: EDITED_ROW, column: 0 }, before: "12.50", after: "12.5" }],
        revalidated_columns: [0],
      }),
    );
    const withNotice = gridScreenFailed(reported, "この操作はまだ使えません: 列の幅の変更");
    const asked = gridScreenDeleteRequested(withNotice, CONFIRMATION);

    const after = gridScreenDeleteCancelled(asked);

    if (after.state.status !== "ready" || asked.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.pendingDelete).toBeNull();
    // **文書を触っていないので、報告も告知もそのままである**（送る経路に載っていない）。
    expect(after.editReport).toEqual(asked.editReport);
    expect(after.notice).toBe(asked.notice);
    expect(after.state).toEqual({ ...asked.state, pendingDelete: null });
    expect(markOf(after)).not.toContain("jxcel-grid-delete-confirm");
  });

  it("適用できなかったときは、確認を開いたままにして、理由を告知へ出す", () => {
    // **適用されていないので、確認を取り下げる理由が無い**（セルの編集が入力手段を開いたままに
    // するのと同じ規律である）。
    const asked = gridScreenDeleteRequested(readyModel(initialSelection()), CONFIRMATION);

    const after = gridScreenRowOperationSettled(asked, {
      status: "failed",
      message: "ドキュメントの失敗: 経路が不達である",
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.pendingDelete).toEqual(CONFIRMATION);
    expect(after.notice).toBe("行の操作を適用できませんでした: ドキュメントの失敗: 経路が不達である");
    expect(markOf(after)).toContain("jxcel-grid-delete-confirm");
  });

  it("選択が動けば確認を取り下げる（確認は選択についてのものである）", () => {
    const asked = gridScreenDeleteRequested(readyModel(initialSelection()), CONFIRMATION);
    if (asked.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    const askedSelection = asked.state.selection;

    // 別のセルへ移った（**数が変わりうる**ので、古い数を掲げたままにしない）。
    const moved = gridScreenSelectionChanged(asked, selectionAt({ row: 4, column: 0 }));

    if (moved.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(moved.state.pendingDelete).toBeNull();
    // 選択が同じ値のまま報せられたときは取り下げない（据え置きの判定と同じ規律である）。
    const again = gridScreenSelectionChanged(asked, askedSelection);
    if (again.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(again.state.pendingDelete).toEqual(CONFIRMATION);
  });

  it("行数が変われば確認を取り下げる（適用された対象はもう無い）", () => {
    const asked = gridScreenDeleteRequested(readyModel(initialSelection()), CONFIRMATION);

    const after = gridScreenRowOperationSettled(asked, {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 3 }),
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.pendingDelete).toBeNull();
  });
});

describe("貼り付けの反映（8.7。要件 7.3、7.4、1.7）", () => {
  it("行が増えたとき、提示する行数と窓が覆う行数が新しい数になり、選択は動かない", () => {
    // 要件 7.4 の行の補充である（矩形が表の末尾を越えると行が足される）。**行数が変わりうる
    // 操作は、反映の形が 8.6 と同じ 1 つである**（`clear(row_count)` ＋ 寄せ）。
    const before = readyModel(initialSelection());

    const after = gridScreenPasteSettled(before, {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 22, violation_total: 1 }),
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.visibleRows).toBe(22);
    expect(after.state.summary.row_count).toBe(22);
    expect(after.state.violationTotal).toBe(1);
    expect(after.state.generation).toBe(2);
    // 行が増えたので、現在位置（先頭のセル）は寄せられない。
    expect(after.state.selection).toEqual(before.state.status === "ready" ? before.state.selection : null);
    expect(markOf(after)).toContain('data-row-count="22"');
  });

  it("行数が減ったとき、現在位置と選択が新しい表の範囲へ寄る（消えた行の面は閉じる）", () => {
    // 貼り付けは行を減らさない（`PasteRange` の逆命令が減らすのは取り消しの側である）が、
    // **反映の形は行数が変わる操作に共通**である — 寄せが無ければ「現在位置 10 行」と名乗り
    // ながら表は 3 行になる（8.6 が閉じたのと同じ欠陥）。
    const editing = gridScreenEditStarted(readyModel(initialSelection()), { row: 9, column: 0 }, "打ちかけ");

    const after = gridScreenPasteSettled(editing, {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 3 }),
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.summary.row_count).toBe(3);
    expect(after.state.editing).toBeNull();
  });

  it("適用できなかったときは、内容を消さずに理由を告知へ出す（表も状態も動かない）", () => {
    const before = readyModel(initialSelection());

    const after = gridScreenPasteSettled(before, {
      status: "failed",
      message: "ドキュメントの失敗: 経路が不達である",
    });

    expect(after.state).toEqual(before.state);
    expect(after.notice).toBe("貼り付けを適用できませんでした: ドキュメントの失敗: 経路が不達である");
    expect(markOf(after)).toContain("jxcel-grid-table");
  });
});

// ===========================================================================
// 2.9 取り消しとやり直し（tasks.md 8.9。要件 9.2、9.3、9.8、9.9）
// ===========================================================================

/**
 * 取り消しとやり直しの**反映**（要件 9.2、9.3、9.8）を、画面の結線の側から固定する。
 *
 * 3 つを見る:
 *
 * 1. **反映の形は 8.6 / 8.7 と同じ 1 つである**（`appliedRowOperation` を通る）。取り消しは
 *    行数を変えうるので、提示する行数・窓が覆う行数・現在位置と選択の寄せ・消えた行の面を
 *    閉じることが要る — 形を 2 つに割れば、片方だけが寄せを持つ日が来る
 * 2. **対象となった範囲へ現在位置が移る**（要件 9.8）。移動先は**表示の序数**であり（行の
 *    識別子ではない）、引けなければ**動かさない**（推測しない）
 * 3. **3 種の操作が同じ 1 つの履歴に乗っている**（8.9 の受け入れ）。セルの編集・行の操作・
 *    貼り付けはどれも `grid_apply_edit` へ行き、取り消しとやり直しはどれも `grid_history` へ
 *    行く — 画面は**操作の種別を 1 つも持たない**（種別を持つと、5 つ目の操作が来た日に
 *    分岐が増える）
 */

describe("取り消しとやり直しの反映（8.9。要件 9.2、9.3、9.8）", () => {
  it("取り消しとやり直しの操作が画面に出る（要件 9.9 の入口）", () => {
    const markup = markOf(readyModel(initialSelection()));

    // **画面の中の入口である**（メニューと打鍵だけにしない — どちらも使えない環境では
    // 操作へ届く道が無くなる）。表示名はメニューの項目と同じ綴りである。
    expect(markup).toContain("jxcel-grid-history");
    expect(markup).toContain("jxcel-grid-undo");
    expect(markup).toContain("jxcel-grid-redo");
    expect(markup).toContain("元に戻す");
    expect(markup).toContain("やり直し");
  });

  it("適用のあと、提示する行数と窓が覆う行数が応答の数になる（構造の取り消し）", () => {
    const before = readyModel(initialSelection());

    const after = gridScreenHistorySettled(before, {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 3, violation_total: 4 }),
      affectedRow: null,
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // **行数を戻すのは取り消しの本体の一部である**（行の追加の逆命令は行を取り除く）。
    // `invalidate` だけでは、削除された行より後ろの窓が別の行を指したまま残る。
    expect(after.state.visibleRows).toBe(3);
    expect(after.state.summary.row_count).toBe(3);
    expect(after.state.violationTotal).toBe(4);
    expect(after.state.generation).toBe(2);
    expect(markOf(after)).toContain('data-row-count="3"');
  });

  it("対象となった範囲へ現在位置を移す（要件 9.8）", () => {
    // 現在位置は先頭、影響を受けた行は表示の序数 7 である（行の識別子ではない）。
    const before = readyModel(initialSelection());

    const after = gridScreenHistorySettled(before, {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 20 }),
      affectedRow: 7,
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    // 選択は**その 1 セルへ畳む**（列は現在位置のものが残る — 変わったのは行だけである）。
    expect(after.state.selection).toEqual(selectionAt({ row: 7, column: 0 }));
    const markup = markOf(after);
    expect(markup).toContain('data-current-row="7"');
    expect(markup).toContain("現在位置 8 行 1 列");
  });

  it("移した先が表示範囲の外なら、追随がスクロールを起こす（変更された箇所が見える）", () => {
    // **可視の区間は先頭 5 行である**（要件 9.8 の「見える状態」は追随が担う。8.4 の巡回と
    // 同じ道であり、`scrollTo` へ何を渡すかは 1 箇所に閉じている）。
    const before = readyModel(initialSelection());
    const after = gridScreenHistorySettled(before, {
      status: "applied",
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 20 }),
      affectedRow: 12,
    });
    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }

    const visible: VisibleSpan = {
      rows: { start: 0, count: 5 },
      columns: { start: 0, count: SAMPLE_COLUMNS.length },
    };
    expect(followTarget(visible, after.state.selection)).toEqual({ row: 12, column: 0 });

    // 移植口へ下ろす 2 つ（選択と、追随のスクロール）を偽の取っ手で読む。
    const calls: string[] = [];
    followSelection(
      {
        setSelection: (selection) => {
          // **下ろすのは選択そのものであり、`null` は「選択が無い」である**（表を描いていない
          // ときだけであり、この検査では起きない）。
          calls.push(`set:${selection === null ? "none" : String(selection.current.row)}`);
        },
        scrollTo: (position) => {
          calls.push(`scroll:${String(position.row)}`);
        },
      },
      visible,
      after.state.selection,
    );
    expect(calls).toEqual(["set:12", "scroll:12"]);
  });

  it("移動先が引けなければ現在位置を動かさない（推測しない）", () => {
    const before = readyModel({ current: { row: 4, column: 2 }, range: { start: { row: 4, column: 2 }, end: { row: 4, column: 2 } } });

    const after = gridScreenHistorySettled(before, {
      status: "applied",
      // 削除の取り消しがこれに当たる（戻ってくる行の識別子は記憶に無い）。
      outcome: outcomeOf({ affected: [EDITED_ROW], row_count: 20 }),
      affectedRow: null,
    });

    if (after.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(after.state.selection.current).toEqual({ row: 4, column: 2 });
  });

  it("進める履歴が無いときは何も動かさない（失敗として扱わない）", () => {
    const before = readyModel(initialSelection());

    const after = gridScreenHistorySettled(before, { status: "empty" });

    // **告知も出さない**: 何も起きていないので、失敗の枠を出す理由が無い（要件 9.2、9.3 の
    // 「対象が無い」は正常な結果である）。
    expect(after.state).toEqual(before.state);
    expect(after.notice).toBe(before.notice);
  });

  it("経路が失敗したら、理由を告知へ出す（表も状態も動かない）", () => {
    const before = readyModel(initialSelection());

    const after = gridScreenHistorySettled(before, {
      status: "failed",
      message: "ドキュメントの失敗: 経路が不達である",
    });

    expect(after.state).toEqual(before.state);
    expect(after.notice).toBe(
      "取り消し・やり直しを実行できませんでした: ドキュメントの失敗: 経路が不達である",
    );
    expect(markOf(after)).toContain("jxcel-grid-table");
  });
});

/**
 * 3 種の操作が同じ 1 つの履歴に乗っている（8.9 の受け入れ）ことを、**本物の往復と本物の遷移**で
 * 確かめる節である。
 *
 * # 何が偽物で、何が本物か（**正直に書く**）
 *
 * 偽物は**境界と窓の記憶だけ**である（`node` の環境には IPC も DOM も無い）。履歴そのものは
 * **ドメインの `UndoStack` 1 つ**であり（4.1 / 4.2）、本物の往復をするには Rust を要する —
 * `crates/data-grid` の検査がその本体を持つ。ここで観測できるのは**画面の側の主張**である:
 *
 * - 3 種の操作（セルの編集・行の操作・貼り付け）は**どれも同じ 1 つの口**（`grid_apply_edit`）へ
 *   行き、画面は**種別を 1 つも持たない**（種別ごとの分岐も、種別ごとの履歴も無い）
 * - 取り消しとやり直しは**どれも同じ 1 つの口**（`grid_history`）へ行き、**反映も同じ 1 つの
 *   遷移**を通る — 3 種の往復が同じ形で画面の状態を戻す
 *
 * したがって「同じ履歴に乗っている」ことの**画面の側の証拠**はこの 2 つであり、履歴が実際に
 * 逆命令を戻すこと（要件 9.2、9.3 の本体）は `crates/data-grid` の検査が持つ（`design.md`
 * 「同じ履歴であること」）。
 */

describe("3 種の操作が同じ 1 つの履歴に乗っている（8.9 の受け入れ）", () => {
  /**
   * 標本の道具。**取り消しとやり直しの応答は台本である**（本物の履歴は Rust 側にしか無い）が、
   * 画面が通る口は本物である — 台本を返す口を 2 つに分ければ、どちらかが「呼ばれない」ことで
   * この検査が落ちる。
   */
  function instrumented(): {
    readonly client: GridClient;
    readonly cache: {
      clear: (rowCount?: number) => void;
      ordinalOf: (rowId: string) => number | null;
      invalidate: (affected: readonly string[]) => void;
      rowId: (position: CellPosition) => string | null;
      documentColumn: (position: CellPosition) => number | null;
    };
    /** 境界へ届いた編集命令（`grid_apply_edit` の引数）。 */
    readonly edits: readonly GridEditCommand[];
    /** 境界へ届いた向き（`grid_history` の引数）。 */
    readonly directions: readonly GridHistoryDirection[];
    /** 適用の応答を 1 つずつ消費する台本（`applyEdit` がこの順に答える）。 */
    readonly scriptEdits: (outcomes: readonly GridEditOutcome[]) => void;
    /** 履歴の応答を 1 つずつ消費する台本（`readHistory` がこの順に答える。尽きたら `null`）。 */
    readonly scriptHistory: (outcomes: readonly GridEditOutcome[]) => void;
  } {
    const edits: GridEditCommand[] = [];
    const directions: GridHistoryDirection[] = [];
    let editAt = 0;
    let editScript: readonly GridEditOutcome[] = [];
    let historyAt = 0;
    let historyScript: readonly GridEditOutcome[] = [];
    const rows = new Map<string, number>([
      [EDITED_ROW, 1],
      [OTHER_ROW, 2],
    ]);
    const unused = (name: string) => (): never => {
      throw new Error(`履歴の往復は ${name} を呼んではならない`);
    };
    return {
      edits,
      directions,
      scriptEdits: (outcomes) => {
        editScript = outcomes;
        editAt = 0;
      },
      scriptHistory: (outcomes) => {
        historyScript = outcomes;
        historyAt = 0;
      },
      client: {
        readDocumentState: async () => unused("document_state")(),
        openSheet: async () => unused("grid_open_sheet")(),
        setView: async () => unused("grid_set_view")(),
        readWindow: async () => unused("grid_rows_window")(),
        findViolation: async () => unused("grid_find_violation")(),
        applyEdit: async (command) => {
          edits.push(command);
          const outcome = editScript[editAt];
          editAt += 1;
          if (outcome === undefined) {
            throw new Error("適用の応答の台本が尽きた");
          }
          return { status: "ok", data: { context: CONTEXT, outcome } };
        },
        readHistory: async (direction) => {
          directions.push(direction);
          const outcome = historyScript[historyAt];
          historyAt += 1;
          // **尽きたら「進める履歴が無い」である**（失敗ではない。要件 9.2、9.3）。
          return { status: "ok", data: { context: CONTEXT, outcome: outcome ?? null } };
        },
      },
      cache: {
        // **捨てると行の対応が消える**（本物の記憶と同じ性質である。8.9 の移動先の解決が
        // 作り直しの前に済んでいることは `history.test.ts` が固定する）。
        clear: () => {
          rows.clear();
        },
        ordinalOf: (rowId: string) => rows.get(rowId.toUpperCase()) ?? null,
        invalidate: () => undefined,
        rowId: (position) => (position.row === 1 ? EDITED_ROW : null),
        documentColumn: (position) => position.column,
      },
    };
  }

  it("編集 → 行の操作 → 貼り付け → 取り消し 3 回 → やり直し 3 回で、同じ履歴を往復する", async () => {
    const tool = instrumented();
    // 3 種の操作が積む結果。① セルの編集（4 行のまま）② 行の追加（4 → 5 行）③ 貼り付け（5 行の
    // まま、2 行へ書く）。
    const editOutcome = outcomeOf({ affected: [EDITED_ROW], row_count: 4 });
    const insertOutcome = outcomeOf({ affected: [OTHER_ROW], row_count: 5 });
    const pasteOutcome = outcomeOf({ affected: [OTHER_ROW, EDITED_ROW], row_count: 5 });
    // **取り消しの応答は、逆命令を適用したあとの数である**（行の追加の取り消しは行を取り除くの
    // で、応答の行数は 4 である）。だから取り消しとやり直しで同じ欄が違う値になる。
    const insertUndone = outcomeOf({ affected: [OTHER_ROW], row_count: 4 });
    const editUndone = outcomeOf({ affected: [EDITED_ROW], row_count: 4 });
    tool.scriptEdits([editOutcome, insertOutcome, pasteOutcome]);
    // 取り消しは**新しい操作から戻る**（貼り付け → 行の追加 → セルの編集）ので、行数は
    // 5 → 4 → 4 と動く。やり直しは**同じ履歴を前へ戻る**ので 4 → 5 → 5 である。
    tool.scriptHistory([
      pasteOutcome,
      insertUndone,
      editUndone,
      editOutcome,
      insertOutcome,
      pasteOutcome,
    ]);

    // ① セルの編集（`SetCells`）
    const edit = await settleCellEdit({
      client: tool.client,
      cache: tool.cache,
      position: { row: 1, column: 0 },
      intent: { kind: "commit", text: "12.50" },
      carrier: "text",
    });
    expect(edit.status).toBe("applied");
    let model = gridScreenEditSettled(readyModel(initialSelection()), edit);

    // ② 行の操作（`InsertRows`）
    const inserted = await applyRowOperation({
      client: tool.client,
      cache: tool.cache,
      intent: { kind: "insert", at: 2 },
    });
    expect(inserted.status).toBe("applied");
    model = gridScreenRowOperationSettled(model, inserted);
    if (model.state.status !== "ready") {
      throw new Error("表を描く状態でなくなった");
    }
    expect(model.state.summary.row_count).toBe(5);

    // ③ 貼り付け（`PasteRange`）
    const pasted = await applyPaste({
      client: tool.client,
      cache: tool.cache,
      payload: { anchor: { row: EDITED_ROW, column: 0 }, rows: [EDITED_ROW], text: "12.50" },
    });
    expect(pasted.status).toBe("applied");
    model = gridScreenPasteSettled(model, pasted);

    // **3 種とも同じ 1 つの口（`grid_apply_edit`）へ行った**（種別ごとの経路を作っていない）。
    expect(tool.edits.map((command) => command.command)).toEqual([
      "SetCells",
      "InsertRows",
      "PasteRange",
    ]);

    // ④ 取り消し 3 回（貼り付け → 行の操作 → セルの編集）。**反映は同じ 1 つの遷移である。**
    const rowCounts: number[] = [];
    for (let step = 0; step < 3; step += 1) {
      const undone = await applyHistory({ client: tool.client, cache: tool.cache, direction: "undo" });
      expect(undone.status).toBe("applied");
      model = gridScreenHistorySettled(model, undone);
      if (model.state.status !== "ready") {
        throw new Error("表を描く状態でなくなった");
      }
      rowCounts.push(model.state.summary.row_count);
    }
    // 貼り付けの取り消し（5 行）→ 行の追加の取り消し（4 行へ戻る）→ セルの編集の取り消し（4 行）。
    expect(rowCounts).toEqual([5, 4, 4]);

    // ⑤ やり直し 3 回（同じ履歴を前へ戻る）。行数は 4 → 5 → 5 と進む。
    const redoneCounts: number[] = [];
    for (let step = 0; step < 3; step += 1) {
      const redone = await applyHistory({ client: tool.client, cache: tool.cache, direction: "redo" });
      expect(redone.status).toBe("applied");
      model = gridScreenHistorySettled(model, redone);
      if (model.state.status !== "ready") {
        throw new Error("表を描く状態でなくなった");
      }
      redoneCounts.push(model.state.summary.row_count);
    }
    expect(redoneCounts).toEqual([4, 5, 5]);

    // **取り消しとやり直しは同じ 1 つの口を通った**（6 回とも `grid_history` である）。
    expect(tool.directions).toEqual(["undo", "undo", "undo", "redo", "redo", "redo"]);

    // ⑥ 履歴が尽きたら何も動かない（要件 9.2、9.3 の正常な結果である）。
    const exhausted = await applyHistory({ client: tool.client, cache: tool.cache, direction: "undo" });
    expect(exhausted).toEqual({ status: "empty" });
    const unchanged = gridScreenHistorySettled(model, exhausted);
    expect(unchanged.state).toEqual(model.state);
  });
});



/**
 * 列幅・表示上の列順（要件 8.1、8.2）、並べ替え・絞り込み（8.3、8.4）、隠れた行の提示（8.7）を、
 * **画面の結線の側から**固定する。判断そのもの（`./viewOps`）と表示状態（`./displayState`）は
 * それぞれの検査が持つので、ここが見るのは次の 4 つである:
 *
 * 1. **列幅と列順は境界へ 1 つも渡らない**（要件 8.5 の線引きが結線でも守られていること）。
 *    送る指定は `sort` / `filters` / `expansion` の 3 つの欄だけであり、2 つの操作は
 *    **1 つの往復も起こさない** — 越える経路が無いことを、送った値そのものを見て確かめる
 * 2. **描かれる列の並びは 1 つである**（要件 8.2、8.6）。列ごとの操作の行と、窓の記憶の写像が
 *    **同じ並び**から組まれる（片方だけが追随すると、描かれている値と編集の宛先が食い違う）
 * 3. **隠れた行の数は応答が運んだ数そのものである**（要件 8.7。画面は数え直さない）
 * 4. **並べ替えや絞り込みの下の編集・貼り付けが、その行そのものへ届く**（要件 8.6、8.8、8.9）。
 *    窓は `window_protocol.txt` の**本物の Rust の符号化器が出したバイト列**を使う
 *    （偽のサーバを書かない。形式の真は 7.3 の固定が持つ）
 *
 * # 単体テストが観測しないもの（**正直に書く**）
 *
 * - **実際のドラッグと絞り込みの入力**（マウスの操作が移植口の知らせになること、選択肢が
 *   切り替わること）は `node` の環境では観測できない。台本 `smoke/gridProbe` の実起動と 9.2 が
 *   観測する（`viewOps.ts` の module doc「単体テストが観測しないもの」と同じ分担である）
 * - **幅・並びの変化が次の `mount` に載ること**は、ここでは組み直しの合図（`layoutKey`）までを
 *   見る。器へ実際に載ることは実起動の観測である
 */

/**
 * 窓の二進形式の言語をまたぐ固定（`windowCache.test.ts` と同じファイル）。
 *
 * **検査の側で符号化器を書かない** — 本物の Rust の符号化器が出した列を使う（形式の真は
 * 7.3 の固定である）。
 */
const WINDOW_FIXTURE: ArrayBuffer = windowFixtureBytes("window");

function windowFixtureBytes(key: string): ArrayBuffer {
  const line = windowFixtureText
    .split("\n")
    .map((found) => found.trim())
    .find((found) => found.startsWith(`${key} = `));
  if (line === undefined) {
    throw new Error(`固定ファイルに ${key} が無い`);
  }
  const hex = line.slice(key.length + 3).trim();
  const bytes = new Uint8Array(hex.length / 2);
  for (let at = 0; at < bytes.length; at += 1) {
    bytes[at] = Number.parseInt(hex.slice(at * 2, at * 2 + 2), 16);
  }
  return bytes.buffer as ArrayBuffer;
}

/** 窓の固定が運ぶ行の識別子（`window_protocol.txt` の表。**検査の側で組み立てない**）。 */
const WINDOW_FIXTURE_ROWS = [
  "01ARZ3NDEKTSV4RRFFQ69G5FAW",
  "01ARZ3NDEKTSV4RRFFQ69G5FAX",
  "01ARZ3NDEKTSV4RRFFQ69G5FAY",
] as const;

/** 偽の移送の約束を反映させる（時間に依らない待ち方。`windowCache.test.ts` と同じ）。 */
async function settleTasks(): Promise<void> {
  for (let tick = 0; tick < 4; tick += 1) {
    await Promise.resolve();
  }
}

/** 表を描いている状態を取り出す（描いていなければ検査の誤りである）。 */
function readyStateOf(model: GridScreenModel): Extract<GridScreenState, { status: "ready" }> {
  if (model.state.status !== "ready") {
    throw new Error("表を描く状態でなかった");
  }
  return model.state;
}

/** 境界へ渡された表示の指定を**丸ごと**控える偽の境界（欄の漏れを読むためである）。 */
interface ViewRecordingClient extends FakeClient {
  readonly views: readonly GridViewSpec[];
}

function viewRecordingClient(
  answers: Parameters<typeof fakeClient>[0],
  answer: (view: GridViewSpec) => IpcResult<GridViewResponse, IpcClientError>,
): ViewRecordingClient {
  const client = fakeClient(answers);
  const views: GridViewSpec[] = [];
  return {
    ...client,
    views,
    setView: async (view: GridViewSpec) => {
      // **境界へ届いた値そのもの**を控える（送る側の写しではなく、渡された値の深い写しである）。
      views.push(JSON.parse(JSON.stringify(view)) as GridViewSpec);
      return answer(view);
    },
  };
}

/** 表示の操作の行に出た列（**表示の順**。属性から文書の列と名前を読む）。 */
function viewBarColumnsIn(markup: string): readonly { readonly column: number; readonly name: string }[] {
  return [...markup.matchAll(/data-document-column="(\d+)"[^>]*><span[^>]*>([^<]*)</g)].map(
    (match) => ({ column: Number(match[1]), name: match[2] ?? "" }),
  );
}

describe("表示の操作（8.8。要件 8.1〜8.7）", () => {
  it("列幅と表示上の列順は境界へ 1 つも渡らず、渡る指定は 3 つの欄だけである（要件 8.1、8.2、8.5）", async () => {
    const client = viewRecordingClient(
      {
        state: ok(openDocument([sheetOf("s1", "標本シート", 3, SAMPLE_ROWS)])),
        open: ok(openedSheet({ columns: [...SAMPLE_COLUMNS], row_count: SAMPLE_ROWS })),
      },
      () => ok(derivedView(12, 0, SAMPLE_COLUMNS)),
    );

    const state = await loadGridScreenState(client);
    if (state.status !== "ready") {
      throw new Error("表を描く状態にならなかった");
    }
    let model = gridScreenLoaded(initialGridScreenModel(), state);

    // 開く流れは**空の指定を 1 度だけ**渡す（操作ではない。8.1）。
    expect(client.views).toHaveLength(1);
    expect(client.views[0]).toEqual(EMPTY_GRID_VIEW);

    // 並べ替えと絞り込みを、**画面の操作から指定を組む経路そのもの**（`./viewOps`）で通す。
    const sorted = applyViewOperation(EMPTY_GRID_VIEW, { kind: "sortCycle", column: 1 });
    model = gridScreenViewSettled(model, await applyGridView(client, sorted));
    const filteredView = applyViewOperation(sorted, {
      kind: "filter",
      column: 0,
      filter: { filter: "Contains", column: 0, text: "名" },
    });
    model = gridScreenViewSettled(model, await applyGridView(client, filteredView));

    // **境界へ渡ったのは指定の 3 つの欄だけである**（列幅も列順も 1 つも現れない）。
    // 欄の集合を**そのまま**比べるのは、入れ子の漏れ（`sort` の下に幅を混ぜる形）も
    // 捕まえるためである（3 つの欄の外に何かあれば落ちる）。
    for (const payload of client.views) {
      expect(Object.keys(payload).sort()).toEqual(["expansion", "filters", "sort"]);
      expect(JSON.stringify(payload)).not.toMatch(/width|order/i);
    }
    // 送った中身も操作の結果そのものである（並べ替えと絞り込みが両方載っている）。
    expect(client.views.at(-1)?.sort).toEqual(sorted.sort);
    expect(client.views.at(-1)?.filters).toHaveLength(1);

    // ここからが本題である。列幅の変更と列の移動。
    const sent = client.views.length;
    const filteredKey = readyStateOf(model).layoutKey;
    const resized = gridScreenColumnResized(model, 0, 240);
    // **幅は列に付く**（表示位置ではない。7.5 の規則）— 位置 0 にいる列（文書の列 0）の幅である。
    expect(readyStateOf(resized).display.widthAt(0)).toBe(240);
    const resizedKey = readyStateOf(resized).layoutKey;
    const moved = gridScreenColumnMoved(resized, 2, 0);

    // **1 つの往復も起こさない**（列幅と表示上の列順は窓の中身を 1 つも変えないので、境界を
    // 越える理由が無い。越えれば、保存される列の順序が変わる経路が生まれる。要件 8.5）。
    expect(client.views).toHaveLength(sent);

    // 画面の側では 2 つとも効いている（組み直しの合図が動く。幅と並びの変化は次の `mount` に
    // 載る — 器へ押し込む口が無い。7.1 の申し送り）。
    expect(resizedKey).not.toBe(filteredKey);
    expect(readyStateOf(moved).layoutKey).not.toBe(resizedKey);
    // 列を運ぶと、幅は**同じ列に残る**（位置 1 へ運ばれた文書の列 0 が 240 のままである）。
    expect(readyStateOf(moved).display.widthAt(1)).toBe(240);
    // 位置 0 へ運ばれた文書の列 2 は幅を設定されていないので、既定の幅である。
    expect(readyStateOf(moved).display.widthAt(0)).toBe(DEFAULT_COLUMN_WIDTH);

    // そのあとの操作も、指定の 3 つの欄だけを送る（列幅と並びは 1 つも混ざらない）。
    const afterMove = gridScreenViewSettled(
      moved,
      await applyGridView(client, applyViewOperation(readyStateOf(moved).view, { kind: "sortNone" })),
    );
    expect(afterMove.state.status).toBe("ready");
    expect(client.views.at(-1)?.filters).toHaveLength(1);
    expect(Object.keys(client.views.at(-1) ?? {}).sort()).toEqual(["expansion", "filters", "sort"]);
    expect(client.views).toHaveLength(sent + 1);
  });

  it("描かれる列の並びは 1 つであり、列ごとの操作と窓の写像が同じ並びに従う（要件 8.2、8.6）", () => {
    const before = readyModel(initialSelection());
    // 表示位置 2（提供元）を先頭へ運ぶ。
    const moved = gridScreenColumnMoved(before, 2, 0);
    const state = readyStateOf(moved);

    // **描かれる列の並び**（表示順。これが表示の位置の空間そのものである）。
    const drawn = drawnColumns(state.summary.columns, state.display);
    expect(drawn.map((column) => column.column)).toEqual([2, 0, 1]);

    // 列ごとの操作（8.5）も、表示の操作の行（8.8）も**同じ並び**で並ぶ。
    expect(controlNamesIn(markOf(moved))).toEqual(["提供元", "名前", "数量"]);
    expect(viewBarColumnsIn(markOf(moved))).toEqual([
      { column: 2, name: "提供元" },
      { column: 0, name: "名前" },
      { column: 1, name: "数量" },
    ]);

    // **窓の写像も描かれる並びから組まれる**（表の面が渡す値そのものである）。恒等で組むと、
    // 描かれている値と編集の宛先が別の列を指す（要件 8.6 の列版）。
    const cache = createGridSurfaceCache({
      sheet: "s1",
      columns: drawn,
      visibleRows: state.visibleRows,
      generation: state.generation,
      client: fakeClient({ state: err<DocumentStateResponse>() }),
    });
    expect(cache.documentColumn({ row: 0, column: 0 })).toBe(2);
    expect(cache.documentColumn({ row: 0, column: 2 })).toBe(1);
    // 対照: 構成の順（恒等）で組むと別の答えになる（この差が取り違えの正体である）。
    expect(createColumnSpace(state.summary.columns).documentColumn(0)).toBe(0);
  });

  it("絞り込みで隠れている行の数は、応答が運んだ数そのものである（要件 8.7）", async () => {
    let answered = 0;
    const client = viewRecordingClient(
      {
        state: ok(openDocument([sheetOf("s1", "標本シート", 3, SAMPLE_ROWS)])),
        open: ok(openedSheet({ columns: [...SAMPLE_COLUMNS], row_count: SAMPLE_ROWS })),
      },
      () => {
        answered += 1;
        // 開く流れ（空の指定）は絞り込みが 1 つも無いので隠れは 0 である。
        if (answered === 1) {
          return ok<GridViewResponse>({
            context: CONTEXT,
            visible_rows: SAMPLE_ROWS,
            hidden_rows: 0,
            violation_total: 0,
            columns: [...SAMPLE_COLUMNS],
          });
        }
        // **この応答は、画面が数え直すと食い違う数を運ぶ**（可視 13 ＋ 隠れ 10 = 23 であり、
        // 開いたときの要約が運んだシートの行数 20 と一致しない — 別の窓が行を足した後の状態で
        // ある）。数え直す画面は「20 − 13 = 7」と名乗り、**応答が運んだ数を写す画面**だけが
        // 10 と名乗る。数の唯一の源は応答である（`GridViewResponse.hidden_rows`）。
        return ok<GridViewResponse>({
          context: CONTEXT,
          visible_rows: 13,
          hidden_rows: 10,
          violation_total: 0,
          columns: [...SAMPLE_COLUMNS],
        });
      },
    );

    const state = await loadGridScreenState(client);
    if (state.status !== "ready") {
      throw new Error("表を描く状態にならなかった");
    }
    const loaded = gridScreenLoaded(initialGridScreenModel(), state);
    expect(readyStateOf(loaded).hiddenRows).toBe(0);

    const filtered = gridScreenViewSettled(
      loaded,
      await applyGridView(
        client,
        applyViewOperation(EMPTY_GRID_VIEW, {
          kind: "filter",
          column: 0,
          filter: { filter: "Contains", column: 0, text: "名" },
        }),
      ),
    );
    const filteredState = readyStateOf(filtered);
    expect(filteredState.hiddenRows).toBe(10);
    expect(filteredState.visibleRows).toBe(13);

    // 提示は**応答が運んだ数そのもの**である（利用者が読む文字と、状態が持つ数が一致する）。
    const markup = markOf(filtered);
    expect(markup).toContain('data-hidden-rows="10"');
    expect(markup).toContain("絞り込みにより表示していない行: 10 行");
    expect(markup).toContain("表示 13 行");
    // 数え直した数（20 − 13）ではない。
    expect(markup).not.toContain("絞り込みにより表示していない行: 7 行");
  });

  it("並べ替えや絞り込みが効いている間の値だけの編集では、表示の指定を送り直さない（要件 8.8）", () => {
    const sorted: GridViewSpec = applyViewOperation(EMPTY_GRID_VIEW, { kind: "sortCycle", column: 1 });
    const filteredView: GridViewSpec = applyViewOperation(EMPTY_GRID_VIEW, {
      kind: "filter",
      column: 0,
      filter: { filter: "Contains", column: 0, text: "名" },
    });

    // 基準列の値だけを書く編集（行数は変わらない）。**送り直さない** — 送り直せばドメインが
    // 順序を導出し直し、確定と同時に行の表示位置が動く（要件 8.8 の本体である）。
    expect(
      needsViewRefresh({
        view: sorted,
        outcome: outcomeOf({ affected: [EDITED_ROW], row_count: SAMPLE_ROWS }),
        sheetRowsBefore: SAMPLE_ROWS,
      }),
    ).toBe(false);

    // **絞り込みの条件に合う値へ書き換えた行も、画面から消えない**（数を取り直すと、条件に
    // 合わなくなった行が確定と同時に消える。8.8 が名指しした「編集した行を見失う」経路）。
    expect(
      needsViewRefresh({
        view: filteredView,
        outcome: outcomeOf({ affected: [EDITED_ROW], row_count: SAMPLE_ROWS }),
        sheetRowsBefore: SAMPLE_ROWS,
      }),
    ).toBe(false);

    // 行を増減したときだけ取り直す（可視行数を知る唯一の源は表示の指定の応答である）。
    expect(
      needsViewRefresh({
        view: sorted,
        outcome: outcomeOf({ affected: [EDITED_ROW], row_count: SAMPLE_ROWS + 1 }),
        sheetRowsBefore: SAMPLE_ROWS,
      }),
    ).toBe(true);
    // 指定が 1 つも無ければ、応答が運ぶ行数がそのまま可視行数である（取り直す理由が無い）。
    expect(
      needsViewRefresh({
        view: EMPTY_GRID_VIEW,
        outcome: outcomeOf({ affected: [EDITED_ROW], row_count: SAMPLE_ROWS + 1 }),
        sheetRowsBefore: SAMPLE_ROWS,
      }),
    ).toBe(false);
  });

  it("並べ替えや絞り込みの下の編集と貼り付けは、表示の位置ではなく記憶が答える行と文書の列へ届く（要件 8.6、8.8、8.9）", async () => {
    const edited = outcomeOf({ affected: [WINDOW_FIXTURE_ROWS[2]] });
    const base = fakeClient({
      state: err<DocumentStateResponse>(),
      edit: ok<GridEditResponse>({ context: CONTEXT, outcome: edited }),
    });
    // 窓は**本物の符号化器が出した固定**である（3 行 4 列・世代 7）。窓が運ぶ行の並びは
    // **ドメインの可視の並び**そのものであり、画面はそれを並べ替えない。
    const client: FakeClient = { ...base, readWindow: async () => WINDOW_FIXTURE };

    // 表示上の列順を変える（表示の位置 0 の列を末尾へ運ぶ）。**表示の位置 3 が文書の列 0**である。
    const display = createDisplayState({ columnCount: 4 });
    display.moveColumn(0, 3);
    const columns = [descriptor(0, "A"), descriptor(1, "B"), descriptor(2, "C"), descriptor(3, "D")];
    const drawn = drawnColumns(columns, display);
    expect(drawn.map((column) => column.column)).toEqual([1, 2, 3, 0]);

    // 画面が組むのと同じ引数で窓の記憶を組む（表の面と同じ 1 つの経路）。
    const cache = createGridSurfaceCache({
      sheet: "s1",
      columns: drawn,
      visibleRows: 3,
      generation: 7,
      client,
    });
    // 取得を始める（同期の契約: 未取得は読み込み中を返して要求を始める）。
    expect(cache.getCell({ row: 0, column: 3 }).loading).toBe(true);
    await settleTasks();

    // **読みも、表示の位置ではなく文書の列で引く**（表示の位置 3 の値は文書の列 0 の値 =
    // 「日本語」である。恒等なら空文字（文書の列 3 は `Null`）になる）。
    const read = cache.getCell({ row: 0, column: 3 });
    expect(read.text).toBe("日本語");
    expect(read.loading).toBe(false);

    // **行の身元は窓が答える**（表示の序数 2 の行は、窓の 3 番目の行 = `…FAY` である）。
    const row = cache.rowId({ row: 2, column: 3 });
    expect(row).toBe(WINDOW_FIXTURE_ROWS[2]);

    // 貼り付けの錨も同じ 1 つの写像から組まれ、**歩く並びは表示されている行**である
    // （表示の位置 1 の行 = 窓の 2 番目の行。矩形の 2 行ぶんだけを渡す）。
    const clipboard = createClipboardSurface({
      client,
      cache: () => cache,
      visibleRows: 3,
      onSettled: () => undefined,
      onApplied: () => undefined,
    });
    expect(clipboard.pasteAt({ row: 1, column: 2 }, "a\nb")).toEqual({
      kind: "send",
      payload: {
        anchor: { row: WINDOW_FIXTURE_ROWS[1], column: 3 },
        rows: [WINDOW_FIXTURE_ROWS[1], WINDOW_FIXTURE_ROWS[2]],
        text: "a\nb",
      },
    });

    // **表示されている行の外は渡せない**（表示されている行にのみ及ぶ。要件 8.9）— 錨が可視行
    // （絞り込みが効いていればシートの行数より少ない）の外へ出ると、画面は行を足さずに拒否する
    // （行の補充はドメインの仕事である。要件 7.4）。
    expect(clipboard.pasteAt({ row: 3, column: 0 }, "a")).toEqual({
      kind: "refused",
      message: "貼り付けの起点が表の外にあるため、貼り付けできません",
    });

    // 編集の宛先は**行の識別子と文書の列**である（表示の序数でも表示の列でもない）。
    // **編集は適用の後で記憶を捨てる**（`EditOutcome.affected` の行の窓。7.3・8.3）ので、
    // 貼り付けの検査を先に置いてある — 順序を入れ替えると、影響を受けた行の識別子が
    // 引けなくなる（それが正しい振る舞いである）。
    const settlement = await settleCellEdit({
      client,
      cache,
      position: { row: 2, column: 3 },
      carrier: "text",
      intent: { kind: "commit", text: "訂正" },
    });
    expect(settlement.status).toBe("applied");
    expect(client.edits).toEqual([
      { command: "SetCells", cells: [{ cell: { row, column: 0 }, text: "訂正" }] },
    ]);
    // 影響を受けた行の窓は捨てられる（取り直すまで、その行の識別子は引けない）。
    expect(cache.rowId({ row: 2, column: 3 })).toBeNull();
  });
});

describe("画面の契約（受け取るのは器が渡す引数だけ）", () => {
  it("`ScreenProps` だけで描ける（それ以外の props を要求しない）", () => {
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
 *
 * **画面が持つ源は 1 つではない**（8.4 がバーを `violationBar.tsx` へ、8.8 が表示の操作の行を
 * `viewBar.tsx` へ分けた）。走査を画面本体だけに当てると、**分けた側へ色の値を書いても緑の
 * まま**になる — 確かめられるよう、源の一覧をここに並べる。
 */
const SOURCE_PATHS = [
  "/src/features/grid/GridScreen.tsx",
  "/src/features/grid/violationBar.tsx",
  "/src/features/grid/nestedInspector.tsx",
  "/src/features/grid/viewBar.tsx",
] as const;

const SOURCES = import.meta.glob("/src/features/grid/{GridScreen,violationBar,nestedInspector,viewBar}.tsx", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** グリッド画面の源（本 file の主役と、その表示の一部）。 */
const SCREEN_SOURCE = SOURCE_PATHS.map((path) => SOURCES[path] ?? "").join("\n");

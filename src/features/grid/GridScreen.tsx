/**
 * グリッド画面 — シートを表として見せる画面（tasks.md 8.1。要件 1.1、1.4、1.5、1.6）。
 *
 * 所有: `GridScreen`（design.md「Components and Interfaces → Frontend Layer」の GridScreen、
 * 「File Structure Plan」の `src/features/grid/GridScreen.tsx`）。**本 module は群 8 の骨格で
 * あり、8.2〜8.9 がここへ機能を足す**（下の「8.2〜8.9 への申し送り」）。
 *
 * # 画面の契約（器に差し込まれる側が守ること）
 *
 * 1. **受け取るのは `ScreenProps` だけである。** 本 module の画面は引数を 1 つも宣言しない
 *    （器が渡す 2 つをこの画面は使わない）。`SHELL_SCREEN_REGISTRY`（`src/shell/Layout.tsx`）へ
 *    **1 件**登録され、`ScreenDefinition.component` の型（`ComponentType<ScreenProps>`）が
 *    「余分な props を要求しない」ことを型で強制する。
 * 2. **自前のレイアウト・遷移を持たない。** ウィンドウ全体の面とヘッダは器が持つ。本 module が
 *    描くのは領域の内側だけであり、遷移の入口（`navigate`）を使わない — 表示対象のシートを
 *    選ぶ画面遷移は本機能の外である（design.md「Out of Boundary」）。
 * 3. **自前の配色を持たない。** 色は [`APPEARANCE_VARS`]（`src/shell/theme.ts`）の 10 本の
 *    カスタムプロパティだけを参照する（`GridScreen.test.ts` が源の走査で固定する）。
 *
 * # 失敗の隔離（**どこまでが器で、どこからが画面か**）
 *
 * 器（`src/shell/ScreenBoundary.tsx`）が隔離するのは**描画・コミットと、効果の同期的な例外**
 * だけである。React のエラー境界は**イベントハンドラと非同期の失敗（`Promise` の拒否）を
 * 捕まえない**ため、それらは画面内の状態として扱うほかない。本 module の扱いは次のとおりである。
 *
 * | 事象 | 扱い |
 * |---|---|
 * | 状態の問い合わせ（`document_state`）の失敗 | 内容の領域に失敗を出し、**再試行**を出す |
 * | シートを開く呼び出し（`grid_open_sheet`）の失敗 | 同上 |
 * | 開けるシートが無い・ドキュメントが無い | 同上（**空の状態ではない** — 利用者に打つ手がある） |
 * | 移植口の操作（8.2〜8.9 が結線する）が届いた | **告知の 1 行**を出す。内容は消さず、再試行も出さない（開き直しても直らない失敗である） |
 * | 窓の移送の失敗（生バイト経路の不達） | **画面は何もしない。** 7.3 の窓の記憶が未取得のまま残し、次の引きが再試行する（設計の誤り表「経路の失敗」。読み込み中のままである） |
 * | 描画・コミット・効果の同期的な例外 | 器の `ScreenBoundary` が隔離する（本 module は捕まえない） |
 *
 * 画面内の状態は [`GridScreenState`]（内容の領域）と、告知 1 行（[`GridScreenModel.notice`]）の
 * 2 つである。**告知で内容を置き換えない**のは、選ばれた 1 つの操作の失敗で表示中の表を失うと、
 * 利用者が見ていたものを失うためである。
 *
 * # 2 つの空の状態（要件 1.5、1.6。**判定する欄を明記する**）
 *
 * | 状態 | 判定する欄 | 提示 |
 * |---|---|---|
 * | 列が 1 本も宣言されていない（要件 1.6） | `document_state` の `DocumentSheet.columns === 0` | **表を描かず**、スキーマが未定義であることを示す |
 * | 列はあるが行が 1 件も無い（要件 1.5） | `grid_open_sheet` の応答 `GridSheetSummary.row_count === 0`（かつ `columns` が非空） | **列の構成を示したうえで**、行が無いことを示す |
 * | 行がある | `row_count > 0` | 表を描く |
 *
 * **2 つを区別するのは列の数である**（行数ではない — どちらの状態でも 0 件でありうる）。この
 * 規則は 6.1 の `GridSheetSummary` の doc が持っており、**画面は自前の規則を書かない**。
 *
 * **要件 1.6 は開く呼び出しの応答では届かない。** 列 0 本の計画を [`GridSession::open`] は
 * `SchemaUnusable` として拒む（`crates/data-grid/src/api.rs` の `open` の docs「その提示は画面が
 * セッション無しに行う」）ので、**開く前に**セッションの状態（`DocumentSheet.columns`）で判定する。
 * 防御として、開いた応答の `columns` が空である場合も同じ提示にする（`GridSheetSummary` の型は
 * それを許す）。
 *
 * [`GridSession::open`]: ../../ipc/bindings.ts
 *
 * # データの経路（何をどの順に呼ぶか）
 *
 * 1. `document_state` でセッションの状態を読み、**先頭のシート**を表示対象にする（シートを選ぶ
 *    手段は本機能の外であり、窓が運ぶのは 1 つのシートぶんだけである）
 * 2. 列が 1 本も無ければここで止める（上の表。開く呼び出しはしない）
 * 3. `grid_open_sheet` でシートを開き、`GridSheetSummary`（列の構成と行数）を受ける
 * 4. 行があれば `grid_set_view` に**空の指定**（絞り込み無し・並べ替え無し・展開無し）を渡し、
 *    **可視行の順序を導出させる**。**これを呼ばないと窓はつねに行 0 件で返り、表は読み込み中の
 *    ままになる**（`GridSession::set_view` が順序の導出そのものである。8.1 の起動観測で実測した）。
 *    操作（並べ替え・絞り込み）は 8.8 がここへ足す
 * 5. 表を描く場合、**窓の記憶（7.3）を組み立て、移植口（7.1 / 7.2）へ仕様を渡す**。
 *    `RendererSpec.getCell` は窓の記憶のものをそのまま渡し、窓が届いたら `invalidate` で
 *    その区間を描き直させる（移植口は知らせが無ければ描き直さない）
 *
 * 手順 4 と 5 のあいだに**世代の整合**がある。境界の型は世代を運ばないので、画面が
 * `GridSession` と同じ規則で数える（開いた直後が 0、`grid_set_view` の成功ごとに +1）。
 * 記憶へ渡す行数は**可視行数**（手順 4 の応答）である — 窓の区間は可視行の序数で表される。
 *
 * 境界の口は [`GridClient`]（`./gridClient`）1 つを通す。**本 module は `invoke` もコマンド名も
 * 知らない。**
 *
 * # 8.2 が足したもの（現在位置・選択・追従）
 *
 * **選択と現在位置は画面が持つ。**表を描く状態（`ready` の腕）が `selection` を持ち、その 1 つの
 * 値だけが、数え上げの表示（要件 2.5）と移植口へ下ろす選択（`handle.setSelection`）の両方へ渡る
 * — **写しを 2 つ持たない**ので、画面に出ている数と描かれている選択はずれようがない。
 *
 * | 要件 | どこが担うか |
 * |---|---|
 * | 2.1 現在位置を 1 つ持ち、区別して示す | `ready.selection`（型が「表を描くときは選択がある」を表す）。区別して示すのは移植口の実装（Glide の焦点の環） |
 * | 2.2 方向の指示で隣接するセルへ移る | `./selection` の移動の規則（打鍵は `GridSurface` の器が受ける） |
 * | 2.3 矩形・行の全体・列の全体 | 同じ module の 3 つの規則。ポインタの操作（行見出し・見出し）は移植口の通知として届く |
 * | 2.4 表示範囲の追随 | `followTarget` が宛先を決め、`handle.scrollTo` が動かす。可視の区間は `RendererSpec.onVisibleSpanChange` が知らせる（**8.2 が移植口へ足した口である**） |
 * | 2.5 行数・列数・セル数 | `selectionCounts` を表の上の 1 行に出す |
 * | 2.6 確定した選択を複製・貼り付け・削除・取り消しの対象にする | **`ready.selection` がその口である**（8.6 / 8.7 / 8.9 が読む）。本 module は操作そのものを実装しない |
 *
 * # 8.3〜8.9 への申し送り（本 module が足す予定の場所）
 *
 * - **8.3〜8.9**: 移植口の 5 つの**操作**（`onActivateEditor` / `onColumnResize` /
 *   `onColumnMove` / `onCopy` / `onPaste`）を実装で置き換える。8.1 はそれらを `onUnavailable`
 *   へ流すだけである（**黙って何もしない実装にしない** — `onCopy` が空文字を返せばクリップボード
 *   が空になり、`onPaste` が黙って捨てれば貼り付けが消える。無反応より悪い）
 * - **8.4（違反の提示）**: 違反の印の色は移植口の実装が既定を 1 つ持つ（`RendererSpec` に色を
 *   運ぶ欄が無い）。画面から配色を決めるには移植口の面を広げる判断が要る（本 module は決めて
 *   いない）。**4.4 の「違反の位置へ現在位置を移す」は `ready.selection` を入れ替えれば足りる**
 *   （移動の口は 8.2 が用意した）
 * - **8.6（行の増減）**: 行数が変わったら `WindowCache.clear(rowCount)`（7.3 の申し送り）
 * - **8.8（列幅・列順）**: 列幅と表示上の列順は `createDisplayState`（7.5）が持つ。変化は
 *   **次の `mount` の仕様**に載せる（`RendererHandle` に幅や順を押し込む口は無い。7.2 の申し送り）
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactElement,
} from "react";

import { APPEARANCE_VARS } from "../../shell/theme";
import { assertNever, describeIpcError } from "../../ipc/client";
import type { ColumnDescriptor, GridSheetSummary } from "../../ipc/bindings";
import { createDisplayState } from "./displayState";
import { WINDOW_ROWS, createWindowCache } from "./windowCache";
import { createGlideAdapter } from "./renderer/glideAdapter";
import {
  followTarget,
  initialSelection,
  selectionCounts,
  selectionForKey,
} from "./selection";
import type {
  CellPosition,
  GridRendererPort,
  RenderCell,
  RenderColumn,
  RendererHandle,
  RendererSelection,
  RendererSpec,
  RowMarkerMode,
  VisibleSpan,
} from "./renderer/port";
import { EMPTY_GRID_VIEW, createGridClient, type GridClient } from "./gridClient";

/**
 * この画面の識別子。`src/shell/Layout.tsx` の登録簿が同じ綴りを使うための単一の定義である
 * （検証専用の初期画面の指定 `JXCEL_VERIFICATION_INITIAL_SCREEN=grid` もこの綴りである）。
 */
export const GRID_SCREEN_ID = "grid";

/**
 * 既定の境界の口。**モジュール定数である** — 画面は `ScreenProps` 以外の props を持てないので、
 * 差し替えの口は検査側（`GridScreen.test.ts` が [`loadGridScreenState`] へ渡す偽の実装）にある。
 */
const DEFAULT_CLIENT: GridClient = createGridClient();

/**
 * 移植口の実装。**モジュール定数である**（`mount` だけを持つ状態の無い値であり、面ごとに
 * 作り直す理由が無い）。実物の起動で観測されるのはこの実装である。
 */
const GRID_RENDERER_PORT: GridRendererPort = createGlideAdapter();

// ===========================================================================
// 1. 画面の状態
// ===========================================================================

/**
 * 内容の領域に出す状態。**判別可能な合併型である**（`status` で網羅的に分岐する）。
 */
export type GridScreenState =
  /** 読み込みの途中（器を描く前。効果は [`GridScreen`] が持つ）。 */
  | { readonly status: "loading" }
  /**
   * 開けなかった。`canRetry` は**再試行に意味があるか**である（開く流れの失敗は真、開き直しても
   * 直らない失敗は偽）。
   */
  | { readonly status: "failed"; readonly message: string; readonly canRetry: boolean }
  /** 列が 1 本も宣言されていない（要件 1.6）。**表を描かない。** */
  | { readonly status: "no-schema"; readonly sheetName: string }
  /** 列はあるが行が 1 件も無い（要件 1.5）。**列の構成を示し、行が無いことを示す。** */
  | {
      readonly status: "no-rows";
      readonly sheetName: string;
      readonly columns: readonly ColumnDescriptor[];
    }
  /** 表を描く（行がある）。 */
  | {
      readonly status: "ready";
      readonly sheet: string;
      readonly summary: GridSheetSummary;
      /**
       * 可視行の数（`grid_set_view` の応答）。**窓の区間は可視行の序数である**ため、記憶へ
       * 渡す行数はこれである（シートの行数ではない。絞り込みが効けば両者は食い違う）。
       */
      readonly visibleRows: number;
      /**
       * 選択（**現在位置と矩形**。要件 2.1、2.2、2.3）。
       *
       * **この腕が持つことが「表を描いている間は現在位置が 1 つある」を型で表している**
       * （他の腕は持たない — 要件 2.1 は表があるときの性質である）。単体テストはこの値を
       * 描いて数え上げを読み、8.6 / 8.7 / 8.9 はこの値を操作の対象として読む（要件 2.6）。
       */
      readonly selection: RendererSelection;
    };

/**
 * 画面の状態機械。**読み込みは試行の番号で駆動する**（読み込みの効果は番号を依存に持ち、
 * 再試行は番号を進める）。
 */
export interface GridScreenModel {
  /** 読み込みの試行の番号（0 が最初。再試行のたびに 1 つ進む）。 */
  readonly attempt: number;
  /** 内容の領域に出す状態。 */
  readonly state: GridScreenState;
  /**
   * 器が捕まえない失敗（イベントハンドラ・非同期）の 1 行。`null` なら無い。
   *
   * **内容の領域とは別に持つ**（選ばれた 1 つの操作の失敗で、表示中の表を失わないためである）。
   */
  readonly notice: string | null;
}

/** 画面の初期状態（読み込みの前）。 */
export function initialGridScreenModel(): GridScreenModel {
  return { attempt: 0, state: { status: "loading" }, notice: null };
}

/** 読み込みの結果を入れる。**試行の番号は動かさない**（番号が動くと読み込みが走り直す）。 */
export function gridScreenLoaded(
  model: GridScreenModel,
  state: GridScreenState,
): GridScreenModel {
  // 開き直しの結果なので、前の告知は落とす（古い失敗を新しい表示に重ねない）。
  return { attempt: model.attempt, state, notice: null };
}

/** 器が捕まえない失敗を告知として積む（**内容の領域は変えない**）。 */
export function gridScreenFailed(model: GridScreenModel, message: string): GridScreenModel {
  return { attempt: model.attempt, state: model.state, notice: message };
}

/**
 * 選択（現在位置と矩形）を入れ替える（要件 2.1、2.2、2.3）。**表を描いていないときは何もしない**
 * — 描いていない表に現在位置は無い（`ready` の腕だけが選択を持つ。上の型）。
 *
 * **解除（`null`）は受け取らない。**要件 2.1 は「現在位置となるセルを 1 つ持つ」と言っており、
 * 表を描いている間はつねに 1 つでなければならない。実装が解除を報せてきたとき（Glide の Escape
 * など）は、**いまの選択を新しい値として置き直す** — 器は選択を制御されているので、置き直さない
 * と「解除された選択が描かれたまま、画面の写しは残る」というずれになる（8.1 のレビューが
 * 名指しした危険である）。
 */
export function gridScreenSelectionChanged(
  model: GridScreenModel,
  selection: RendererSelection | null,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  const next = selection ?? { ...model.state.selection };
  return { attempt: model.attempt, state: { ...model.state, selection: next }, notice: model.notice };
}

/** 告知を閉じる。 */
export function gridScreenNoticeDismissed(model: GridScreenModel): GridScreenModel {
  return { attempt: model.attempt, state: model.state, notice: null };
}

/** 再試行する（**番号を進め、読み込みの状態へ戻す**）。 */
export function gridScreenRetried(model: GridScreenModel): GridScreenModel {
  return { attempt: model.attempt + 1, state: { status: "loading" }, notice: null };
}

// ===========================================================================
// 2. 開く流れ（純粋な非同期関数。効果はこれを呼ぶだけである）
// ===========================================================================

/**
 * セッションの状態を読み、表示するシートを開き、**内容の領域に出す状態**を決める。
 *
 * 例外を投げない（`GridClient` の口は封筒の失敗を値で返す）。宣言された列が 1 本も無いときは
 * **開く呼び出しをしない**（モジュール doc の表）。
 */
export async function loadGridScreenState(client: GridClient): Promise<GridScreenState> {
  const answer = await client.readDocumentState();
  if (answer.status === "error") {
    return { status: "failed", message: describeIpcError(answer.error), canRetry: true };
  }

  const session = answer.data.status;
  if (session.state === "Unavailable") {
    return {
      status: "failed",
      message: `ドキュメントを読み込めませんでした: ${session.reason}`,
      canRetry: true,
    };
  }
  if (session.state === "Absent") {
    // 空の状態ではない（表の対象そのものが無い）。利用者には開く手立てがあるので再試行を出す。
    return {
      status: "failed",
      message: "このウィンドウにはドキュメントがありません",
      canRetry: true,
    };
  }

  const sheet = session.sheets[0];
  if (sheet === undefined) {
    return { status: "failed", message: "このドキュメントにはシートがありません", canRetry: true };
  }
  if (sheet.columns === 0) {
    // 要件 1.6。**開く呼び出しをしない**（モジュール doc「2 つの空の状態」）。
    return { status: "no-schema", sheetName: sheet.name };
  }

  const opened = await client.openSheet(sheet.id);
  if (opened.status === "error") {
    return { status: "failed", message: describeIpcError(opened.error), canRetry: true };
  }
  const summary = opened.data.sheet;
  if (summary.columns.length === 0) {
    // 防御: `GridSheetSummary` の型は列 0 本を許す（現在の Rust 側は開く前に拒む）。
    return { status: "no-schema", sheetName: sheet.name };
  }
  if (summary.row_count === 0) {
    // 要件 1.5。**列の構成を示したうえで**行が無いことを示す（表を描かないので順序も要らない）。
    return { status: "no-rows", sheetName: sheet.name, columns: summary.columns };
  }

  // 可視行の順序は `grid_set_view` が導出する（`GridSession::set_view` が `recompute_order`
  // を行う）。**これを呼ばないと窓はつねに行 0 件で返り、表は読み込み中のままになる**
  // （8.1 の起動観測で実測した）。渡すのは「絞り込み無し・並べ替え無し・展開無し」であり、
  // 操作ではない（並べ替え・絞り込みの操作は 8.8 の担当である）。
  const derived = await client.setView(EMPTY_GRID_VIEW);
  if (derived.status === "error") {
    return { status: "failed", message: describeIpcError(derived.error), canRetry: true };
  }
  return {
    status: "ready",
    sheet: sheet.id,
    summary,
    // **窓が覆うのは可視行である**（窓の区間は可視行の序数で表される。`RowSpan` の doc）ので、
    // 記憶へ渡す行数はシートの行数ではなく応答の可視行数である。
    visibleRows: derived.data.visible_rows,
    // **表を描き始める時点から現在位置が 1 つある**（要件 2.1）。先頭のセルである（開いた直後に
    // 見えているのは先頭の窓なので、追随も要らない）。
    selection: initialSelection(),
  };
}

// ===========================================================================
// 3. 移植口へ渡す仕様（**キャンバスを要さない純粋な部分**）
// ===========================================================================

/**
 * まだ結線していない操作の名前（**利用者に見える語**である）。8.2〜8.9 がそれぞれ実装したら、
 * その名前はここから落ちる。
 */
const OPERATION_NAMES = {
  activateEditor: "セルの編集の起動",
  columnResize: "列の幅の変更",
  columnMove: "列の位置の変更",
  copy: "選択の範囲の複製",
  paste: "表形式のテキストの貼り付け",
} as const;

/**
 * 移植口へ渡す仕様を組む。**引く口（`getCell`）・列・行数と、選択に関わる 3 つである。**
 *
 * 選択に関わる 3 つは 8.2 が結線した（8.1 は選択を使わなかった）:
 *
 *   - `selection`: マウントの時点の選択（**表を描くときは現在位置が 1 つある**。要件 2.1）
 *   - `onSelectionChange`: **実装が起こした**変化（ポインタ・行見出し・Glide の側に残した束縛）
 *     を画面へ上げる。画面はそれをそのまま自分の選択として取り込む
 *   - `onVisibleSpanChange`: 見えている区間（追随の判断と窓の先読みの材料。要件 2.4）
 *
 * 残る 5 つは**操作**であり（8.3〜8.9 の担当）、8.2 はそれらを [`onUnavailable`] へ流す。
 * **黙って何もしない実装にしない**理由は 2 つある: `onCopy` が空文字を返せばクリップボードが
 * 空になり、`onPaste` が黙って捨てれば貼り付けが消える（どちらも無反応より悪い）。拒否（`Promise`
 * の失敗）にしておくのは、移植口の実装が**クリップボードへ書かず・適用もしない**ためである
 * （`glideAdapter.tsx` の `GlideSurface` は拒否を記録して描画を止めない）。
 */
export function createGridRendererSpec(options: {
  readonly getCell: (position: CellPosition) => RenderCell;
  readonly columns: readonly RenderColumn[];
  readonly rowCount: number;
  readonly selection: RendererSelection | null;
  readonly rowMarkers: RowMarkerMode;
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
  readonly onVisibleSpanChange: (span: VisibleSpan) => void;
  readonly onUnavailable: (operation: string) => void;
}): RendererSpec {
  const refuse = (operation: string): Error => {
    options.onUnavailable(operation);
    return new Error(operation);
  };

  return {
    columns: options.columns,
    rowCount: options.rowCount,
    selection: options.selection,
    rowMarkers: options.rowMarkers,
    getCell: options.getCell,
    onSelectionChange: options.onSelectionChange,
    onVisibleSpanChange: options.onVisibleSpanChange,
    onActivateEditor: () => {
      refuse(OPERATION_NAMES.activateEditor);
    },
    onColumnResize: () => {
      refuse(OPERATION_NAMES.columnResize);
    },
    onColumnMove: () => {
      refuse(OPERATION_NAMES.columnMove);
    },
    onCopy: () => Promise.reject(refuse(OPERATION_NAMES.copy)),
    onPaste: () => Promise.reject(refuse(OPERATION_NAMES.paste)),
  };
}

// ===========================================================================
// 4. 表（窓の記憶と移植口を組み立てる場所）
// ===========================================================================

/**
 * 行見出し列の出し方。**行の全体をポインタで選べるようにする**（要件 2.3 の 2 つ目）。
 *
 * Glide は行見出しの分の添字を内部で補正する（`getCellContent` / 列幅 / 選択の正規化 /
 * 見えている区間 / `scrollTo` のいずれも）ので、画面はこの値を渡すだけでよい（design.md
 * 「7.2 が決めたこと」の行見出し列の行）。数字は Glide が 1 起点で描く。
 */
const ROW_MARKERS: RowMarkerMode = "clickable-number";

/** 表を入れる枠。**ここが移植口の器である**（確定した寸法が要る。`createGlideAdapter` の doc）。 */
const TABLE_STYLE = {
  flex: "1 1 auto",
  minHeight: 0,
  width: "100%",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
  borderRadius: "0.25rem",
  overflow: "hidden",
} as const;

/** 表の面（数え上げの行と、移植口の器）。**縦に伸びる**（器は確定した寸法を要する）。 */
const SURFACE_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.25rem",
  flex: "1 1 auto",
  minHeight: 0,
} as const;

/** 選択の数え上げの行（要件 2.5）。**表の上に出す**。 */
const SELECTION_STYLE = {
  margin: 0,
  fontSize: "0.8125rem",
  color: `var(${APPEARANCE_VARS.screenMuted})`,
} as const;

/** 表を組み立てる指定。 */
interface GridSurfaceProps {
  /** 開いたシートの識別子（窓の要求が名乗る。`grid_open_sheet` に渡した文字列と同一である）。 */
  readonly sheet: string;
  /** 開いた応答の要約（列の構成と行数）。 */
  readonly summary: GridSheetSummary;
  /** 可視行の数（窓が覆う行数）。 */
  readonly visibleRows: number;
  /** 選択（現在位置と矩形）。**画面の状態が持つ唯一の値である**（写しをここに作らない）。 */
  readonly selection: RendererSelection;
  /** 選択が変わった（打鍵・ポインタのどちらでも）ことを画面へ上げる口。 */
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
  /** 器が捕まえない失敗を画面内へ流す口。 */
  readonly onUnavailable: (operation: string) => void;
}

/**
 * 開いた直後に 1 度だけ `grid_set_view` を呼んだあとの世代。
 *
 * 画面は `GridSession` と同じ規則で世代を数える（**境界の型は世代を運ばない**ため。design.md
 * 「世代を進めるのは画面である」）: 開いた直後が 0（`Generation::FIRST`）であり、
 * `grid_set_view` の成功ごとに 1 つ進む。**食い違うと窓はつねに空になる**（Rust 側は一致
 * しない世代へ空の窓を返すため、画面は読み込み中のまま再試行を続ける）。
 */
const GENERATION_AFTER_OPEN = 1;

/**
 * 表そのもの。**開いたシート 1 つぶんの窓の記憶（7.3）と移植口（7.1 / 7.2）を組み立て、
 * 現在位置と選択（8.2）を結線する。**
 *
 * 組み立てはマウントの効果 1 つで行い、後始末で移植口を片付けて記憶を手放す（`dispose`。以後の
 * 応答は捨てられる）。**列の並びは表示状態（7.5）から組む**ので、8.8 が列幅・列順を変えたときは
 * 新しい仕様でマウントし直すことになる（`RendererHandle` に幅や順を押し込む口が無い）。
 *
 * 効果は 3 つである: ① 器の組み立て（依存はシートと列の構成と可視行数だけ — **選択を依存に
 * 入れない**。入れると打鍵のたびに器を組み直し、React の根と Glide の部品を作り直して走査の
 * 位置を失う）、② 選択を移植口へ下ろし、必要なら追随させる（依存は選択だけ）、③ なし —
 * 見えている区間は ref に置く（描き直しが要らないためである）。
 */
function GridSurface({
  sheet,
  summary,
  visibleRows,
  selection,
  onSelectionChange,
  onUnavailable,
}: GridSurfaceProps): ReactElement {
  const containerRef = useRef<HTMLDivElement | null>(null);
  // 移植口の取っ手。**窓の到着（非同期）と選択の効果が使う**ので、効果の外に置く。
  const handleRef = useRef<RendererHandle | null>(null);
  /**
   * いま見えている区間（要件 2.4 の追随の判断と、7.3 の先読みの材料）。
   *
   * **状態にしない**（描き直しが要らない）。走査のたびに再描画すると、窓の到着のたびに表を
   * 組み直すことになる。値は移植口の知らせ（`onVisibleSpanChange`）が書き換える。
   */
  const visibleRef = useRef<VisibleSpan | null>(null);

  // 表の大きさ（現在位置を寄せる先。要件 2.2 の端の扱いと、行・列の全体の選択に要る）。
  const bounds = { rowCount: visibleRows, columnCount: summary.columns.length };

  /**
   * 表の器が打鍵を受ける口。**方向の指示だけを引き受け、残りは流す**（`./selection` の
   * module doc の表）。ここで例外を投げない — `ScreenBoundary` は**イベントハンドラの例外を
   * 捕まえない**（`src/shell/ScreenBoundary.tsx`）ので、投げれば画面が壊れる。
   */
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>): void => {
    const next = selectionForKey(event, selection, bounds);
    if (next === null) {
      return;
    }
    // 引き受けた打鍵はブラウザの走査を止める（矢印は器をスクロールさせてしまう）。
    event.preventDefault();
    onSelectionChange(next);
  };

  // 表そのものの組み立て。**選択が変わっても組み直さない** — 器を作り直すと React の根と
  // Glide の部品が作り直され、走査の位置（表示範囲）も失われる。仕様へ渡す選択はマウントの
  // 時点の値であり（`RendererSpec.selection` の doc）、以後の更新は下の効果が
  // `handle.setSelection` で下ろす。**したがってこの効果の依存に選択を入れない**
  // （入れると打鍵のたびに組み直すことになる）。効果の閉包は依存が変わった回の描画の値なので、
  // 器を組み直すとき（シートが変わったとき）はそのときの選択が初期値になる。
  useEffect(() => {
    const container = containerRef.current;
    if (container === null) {
      return undefined;
    }

    // 表示状態（7.5）。8.1 は初期の並び（宣言の順・既定の幅）だけを組む。
    const display = createDisplayState({ columnCount: summary.columns.length });
    // 開いた直後に見えている区間の見当。**実装が知らせてくるまでの値である**（Glide は
    // マウントの直後に本当の区間を知らせる）。
    const openingSpan: VisibleSpan = {
      rows: { start: 0, count: Math.min(visibleRows, WINDOW_ROWS) },
      columns: { start: 0, count: summary.columns.length },
    };
    visibleRef.current = openingSpan;

    const cache = createWindowCache({
      sheet,
      // 葉の型の札は**列の宣言から取る**（窓は値の変種しか運ばない。7.3 の指定の doc）。
      variants: summary.columns.map((column) => column.kind ?? "Any"),
      // **窓が覆うのは可視行である**（絞り込みが効いていればシートの行数と食い違う）。
      rowCount: visibleRows,
      // 開いた直後に `grid_set_view` を 1 度呼んだ後の世代（上の定数）。
      generation: GENERATION_AFTER_OPEN,
      transport: (argument) => DEFAULT_CLIENT.readWindow(argument),
      // 窓が届いたら、その区間を描き直させる（移植口は知らせが無ければ描き直さない）。
      onArrival: (span) => {
        handleRef.current?.invalidate(span);
      },
    });

    const handle = GRID_RENDERER_PORT.mount(
      container,
      createGridRendererSpec({
        columns: display.renderColumns(summary.columns.map((column) => column.name)),
        rowCount: visibleRows,
        // **マウントの時点で現在位置が 1 つある**（要件 2.1）。
        selection,
        rowMarkers: ROW_MARKERS,
        getCell: (position) => cache.getCell(position),
        onSelectionChange,
        // 見えている区間の知らせ（8.2 が移植口へ足した口である）。**追随の判断の材料**であり、
        // 同時に窓の先読みの材料でもある（この行が 8.1 の申し送りの答えである）。
        onVisibleSpanChange: (span) => {
          visibleRef.current = span;
          cache.setVisibleSpan(span.rows);
        },
        onUnavailable,
      }),
    );
    handleRef.current = handle;
    // 開いた直後の先読み（要件 1.4）。**上の見当を渡す** — 本物の区間は実装が知らせてくる。
    cache.setVisibleSpan(openingSpan.rows);

    return () => {
      handle.destroy();
      cache.dispose();
      handleRef.current = null;
    };
  }, [sheet, summary, visibleRows, onUnavailable]);

  /**
   * 選択を移植口へ下ろし、必要なら表示範囲を追随させる（要件 2.2、2.4）。
   *
   * **下ろす値と数え上げの値は同じ 1 つである**（`selection`）— 画面に出ている数と、描かれて
   * いる選択がずれる余地を作らない（design.md「8.2 が広げた面」の判断）。
   */
  useEffect(() => {
    const handle = handleRef.current;
    if (handle === null) {
      return;
    }
    handle.setSelection(selection);
    const target = followTarget(visibleRef.current, selection);
    if (target !== null) {
      handle.scrollTo(target);
    }
  }, [selection]);

  const counts = selectionCounts(selection);

  return (
    <div style={SURFACE_STYLE}>
      {/*
        選択の数え上げ（要件 2.5）。**利用者に見える数は 1 起点である**（内部の序数は 0 起点）。
        読み手（検査）のために、生の数を属性にも出しておく。
      */}
      <p
        data-testid="jxcel-grid-selection-counts"
        data-selection-rows={counts.rows}
        data-selection-columns={counts.columns}
        data-selection-cells={counts.cells}
        data-current-row={selection.current.row}
        data-current-column={selection.current.column}
        style={SELECTION_STYLE}
      >
        {`現在位置 ${String(selection.current.row + 1)} 行 ${String(selection.current.column + 1)} 列 ／ 選択 ${String(counts.rows)} 行 × ${String(counts.columns)} 列 = ${String(counts.cells)} セル`}
      </p>
      <div
        ref={containerRef}
        onKeyDown={onKeyDown}
        data-testid="jxcel-grid-table"
        style={TABLE_STYLE}
      />
    </div>
  );
}

// ===========================================================================
// 5. 見た目（**配色は器のカスタムプロパティだけを参照する**）
// ===========================================================================

/** 画面の枠。**領域いっぱいに広がる**（領域は中央寄せなので、自前で伸ばさないと縦に潰れる）。 */
const ROOT_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.75rem",
  width: "100%",
  height: "100%",
  minHeight: 0,
  boxSizing: "border-box",
  padding: "1rem",
  borderRadius: "0.5rem",
  backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 見出し。 */
const HEADING_STYLE = { margin: 0, fontSize: "1.125rem" } as const;

/** 説明・失敗の理由・告知の文字（補助的な文字色）。 */
const MESSAGE_STYLE = { margin: 0, color: `var(${APPEARANCE_VARS.screenMuted})` } as const;

/** 内容の区画（読み込み中・失敗・空の状態）。 */
const PANEL_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.5rem",
  alignItems: "flex-start",
} as const;

/** 操作（再試行・告知を閉じる）。**枠線と文字に器の配色を使う。** */
const BUTTON_STYLE = {
  font: "inherit",
  fontSize: "0.875rem",
  padding: "0.25rem 0.75rem",
  borderRadius: "0.25rem",
  cursor: "pointer",
  color: `var(${APPEARANCE_VARS.controlActiveText})`,
  backgroundColor: `var(${APPEARANCE_VARS.controlActiveBackground})`,
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/** 告知の 1 行（**内容の領域の上に出す**）。 */
const NOTICE_STYLE = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: "0.75rem",
  padding: "0.5rem 0.75rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/** 列の構成（要件 1.5 の提示）。**宣言の順に並べる。** */
const COLUMNS_STYLE = {
  display: "flex",
  flexWrap: "wrap",
  gap: "0.5rem",
  margin: 0,
  padding: 0,
  listStyle: "none",
} as const;

/** 列 1 本ぶんの見出し。 */
const COLUMN_STYLE = {
  padding: "0.15rem 0.5rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/**
 * 列の同一性の綴り（`ColumnDescriptor` の doc「列の同一性は（`column`, `path`）の対である」）。
 *
 * 表示名は人が読むためのものであり、同一性ではない（同じ名前の列が 2 本ありうる）。並びの鍵に
 * 名前を使うと、名前が重複したスキーマで鍵が衝突する。
 */
function columnKey(column: ColumnDescriptor): string {
  const path = column.path
    .map((segment) => (segment.segment === "Field" ? segment.name : `[${segment.position}]`))
    .join(".");
  return `${column.column}:${path}`;
}

/** 内容の領域。**状態を網羅的に分岐する**（新しい変種を足すと型検査がここで落ちる）。 */
function GridScreenBody({
  model,
  onRetry,
  onUnavailable,
  onSelectionChange,
}: {
  readonly model: GridScreenModel;
  readonly onRetry: () => void;
  readonly onUnavailable: (operation: string) => void;
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
}): ReactElement {
  const state = model.state;
  switch (state.status) {
    case "loading":
      return (
        <div data-testid="jxcel-grid-loading" style={PANEL_STYLE}>
          <h2 style={HEADING_STYLE}>シートを読み込んでいます</h2>
        </div>
      );
    case "failed":
      return (
        <div data-testid="jxcel-grid-failure" style={PANEL_STYLE}>
          <h2 style={HEADING_STYLE}>シートを開けませんでした</h2>
          <p style={MESSAGE_STYLE}>{state.message}</p>
          {state.canRetry ? (
            <button type="button" data-testid="jxcel-grid-retry" onClick={onRetry} style={BUTTON_STYLE}>
              再試行
            </button>
          ) : null}
        </div>
      );
    case "no-schema":
      // 要件 1.6: **表を描かない。**
      return (
        <div data-testid="jxcel-grid-schema-undefined" style={PANEL_STYLE}>
          <h2 style={HEADING_STYLE}>スキーマが未定義です</h2>
          <p style={MESSAGE_STYLE}>
            シート「{state.sheetName}」には列が 1 本も宣言されていません。表は描きません。
          </p>
        </div>
      );
    case "no-rows":
      // 要件 1.5: **列の構成を示したうえで**行が無いことを示す。
      return (
        <div data-testid="jxcel-grid-no-rows" style={PANEL_STYLE}>
          <h2 style={HEADING_STYLE}>行がありません</h2>
          <p style={MESSAGE_STYLE}>
            シート「{state.sheetName}」には行が 1 件もありません。列の構成は次の {state.columns.length}
            列です。
          </p>
          <ol data-testid="jxcel-grid-columns" style={COLUMNS_STYLE}>
            {state.columns.map((column) => (
              <li key={columnKey(column)} data-grid-column={columnKey(column)} style={COLUMN_STYLE}>
                {column.name}
              </li>
            ))}
          </ol>
        </div>
      );
    case "ready":
      return (
        <GridSurface
          sheet={state.sheet}
          summary={state.summary}
          visibleRows={state.visibleRows}
          selection={state.selection}
          onSelectionChange={onSelectionChange}
          onUnavailable={onUnavailable}
        />
      );
    default:
      return assertNever(state);
  }
}

/** 見た目へ渡すもの。**状態と操作だけである**（読み込みは [`GridScreen`] が持つ）。 */
export interface GridScreenViewProps {
  readonly model: GridScreenModel;
  /** 開く流れをやり直す（失敗の提示の「再試行」）。 */
  readonly onRetry: () => void;
  /** 告知を閉じる。 */
  readonly onDismissNotice: () => void;
  /** 移植口の操作がまだ結線されていないことを知らせる。 */
  readonly onUnavailable: (operation: string) => void;
  /**
   * 選択が変わった（打鍵・ポインタのどちらでも）。要件 2.1、2.2、2.3。
   *
   * `null` は**実装が選択を解除した**こと（Glide の Escape など）である。画面はそれを
   * 取り下げず、いまの選択を置き直す（[`gridScreenSelectionChanged`] の doc）。
   */
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
}

/**
 * 画面の見た目。**状態だけを受け取る純粋な描画である**ので、検査は状態ごとにこれを呼んで
 * 「何が DOM へ出るか」を読める（`GridScreen.test.ts`）。
 */
export function GridScreenView({
  model,
  onRetry,
  onDismissNotice,
  onUnavailable,
  onSelectionChange,
}: GridScreenViewProps): ReactElement {
  return (
    <section data-testid="jxcel-grid-screen" aria-label="グリッド" style={ROOT_STYLE}>
      {model.notice === null ? null : (
        <div data-testid="jxcel-grid-notice" role="status" style={NOTICE_STYLE}>
          <span style={MESSAGE_STYLE}>{model.notice}</span>
          <button
            type="button"
            data-testid="jxcel-grid-notice-dismiss"
            onClick={onDismissNotice}
            style={BUTTON_STYLE}
          >
            閉じる
          </button>
        </div>
      )}
      <GridScreenBody
        model={model}
        onRetry={onRetry}
        onUnavailable={onUnavailable}
        onSelectionChange={onSelectionChange}
      />
    </section>
  );
}

// ===========================================================================
// 6. 画面の実体（登録簿へ差し込まれる側）
// ===========================================================================

/**
 * グリッド画面。**`ScreenProps` だけを受け取り、それ以外の props を持たない**（画面の契約 1）。
 *
 * 器が渡す 2 つ（`screenId` と `navigate`）をこの画面は使わないので、引数を 1 つも宣言しない
 * （宣言しなければ、余分な props を要求する経路が生まれない）。
 *
 * 読み込みは**試行の番号を依存に持つ効果** 1 つで行う。後始末で「古い結果を捨てる」印を立てる
 * ので、再試行の途中に届いた前の応答が新しい表示を上書きしない。
 */
export function GridScreen(): ReactElement {
  const [model, setModel] = useState<GridScreenModel>(initialGridScreenModel);

  useEffect(() => {
    let cancelled = false;
    void loadGridScreenState(DEFAULT_CLIENT).then((state) => {
      if (cancelled) {
        return;
      }
      setModel((current) => gridScreenLoaded(current, state));
    });
    return () => {
      cancelled = true;
    };
  }, [model.attempt]);

  const retry = useCallback(() => {
    setModel(gridScreenRetried);
  }, []);
  const dismissNotice = useCallback(() => {
    setModel(gridScreenNoticeDismissed);
  }, []);
  const select = useCallback((selection: RendererSelection | null) => {
    // **器に届かない失敗と同じ側である**（イベントハンドラ）。ここは状態の遷移だけであり、
    // 表を描いていないときは遷移が自分で何もしない（`gridScreenSelectionChanged`）。
    setModel((current) => gridScreenSelectionChanged(current, selection));
  }, []);
  const noteUnavailable = useCallback((operation: string) => {
    // **器に届かない失敗である**（イベントハンドラ。`ScreenBoundary` は捕まえない）。内容の
    // 領域は変えず、告知として 1 行出す。
    setModel((current) => gridScreenFailed(current, `この操作はまだ使えません: ${operation}`));
  }, []);

  return (
    <GridScreenView
      model={model}
      onRetry={retry}
      onDismissNotice={dismissNotice}
      onUnavailable={noteUnavailable}
      onSelectionChange={select}
    />
  );
}

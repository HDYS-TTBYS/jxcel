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
 * 画面内の状態は [`GridScreenState`]（内容の領域）と、内容の領域の上に出る 2 つ — 告知 1 行
 * （[`GridScreenModel.notice`]）と、直近の確定の報告（[`GridScreenModel.editReport`]。8.3）で
 * ある。**告知で内容を置き換えない**のは、選ばれた 1 つの操作の失敗で表示中の表を失うと、
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
 * # 8.3 が足したもの（セルの編集と、確定の報告）
 *
 * **編集の起動・確定・取消の 3 つは `./cellEdit` の 1 つの関数（`settleCellEdit`）へ集まる。**
 * 入力手段（7.4）が出す口は `commit(text)` と `cancel()` の 2 つだけであり、そのどちらもが同じ
 * 経路を通る。**取消が境界へ何も送らないこと**はこの形から出る（取消の腕は `GridClient` を
 * 1 つも触らない。`cellEdit.test.ts` が「偽の境界のどの口も例外を投げる」形で固定する）。
 *
 * | 論点 | 決定 | どこが担うか |
 * |---|---|---|
 * | 入力手段の選択（要件 3.1、10.3） | **登録簿（`editorRegistry.resolve`）だけが解決する。** 画面に型ごとの分岐は 1 つも無い | `CellEditorPanel`（本 module の内側の成分） |
 * | 編集するセル | **現在位置の 1 セルだけである**（範囲の編集は 8.7 の貼り付けの担当） | `ready.editing` が位置を持ち、`SetCells` の 1 件になる |
 * | 編集の面をどこに描くか | **表の面（`GridSurface`）の中、数え上げの行と表の器の間**である。移植口に「セルの上へ DOM を重ねる」口は無く（`RendererSpec` にそんな欄は無い）、開いているセルを**位置の提示つきで**出す方が、覆われたセルを探すより読める | `GridSurface` の編集の面 |
 * | 確定の結果（要件 3.4、3.5） | **表の上に 1 つの報告**として出す（型強制の一覧と、残った違反）。**窓の印とは別である** — 移植口のセルが運ぶのは違反の有無だけで（`RenderCell.violated`）、型強制は**起きた出来事**であってセルの状態ではない | `model.editReport` |
 * | 取消の値の復帰（要件 3.6） | 何もしない。文書は適用の前にあり、**値を戻すのは「編集を閉じる」ことそのもの**である（窓の記憶も捨てない） | `gridScreenEditSettled` の `cancelled` の腕 |
 * | 適用のあとの作り直し（要件 1.7） | 影響を受けた行の窓を捨てる（`./cellEdit`）。**描き直させるのは 7.3 の記憶と移植口の既存の結線**（8.1 の `onArrival` → `RendererHandle.invalidate`） | `./cellEdit` と `GridSurface` |
 *
 * ## 編集の面は表の上に出る（**現在位置の追跡**）
 *
 * 編集中のセルは `ready.editing`（`CellEdit`）が持つ — 位置と、**開いた時点の表示文字列**である。
 * 位置だけを持たないのは、入力手段の初期値が要るためであり（`CellEditorProps.initialText`）、
 * **開いた時点で写す**のは、窓の到着で描き直されても入力中の値が動かないようにするためである
 * （要件 3.5 の「値を捨てない」は、打っている最中に足元が変わることでも壊れる）。
 *
 * 表を描いていない状態（`loading` / `failed` / 空の 2 つ）は `editing` を持たない — **描かれて
 * いないセルは編集できない**（型がそれを表す）。
 *
 * ## 確定の報告（要件 3.4、3.5。**提示の本体は 8.4**）
 *
 * 報告は 2 つを持つ:
 *
 *   - **型強制**（要件 3.4）— 変換が起きたことと、**変換前の値**（`GridCoercionNotice.before`）。
 *     変換の前後はどちらも境界が表示文字列で運ぶ（`GridCoercionNotice` の doc）ので、画面は
 *     解釈せずに両方を並べる。**前の値を落とさない**ことは検査が固定する
 *   - **残った違反**（要件 3.5）— 適用のあとに再検証した列の違反の位置（`GridEditOutcome.violations`）
 *     と、**シート全体**の違反の総数（`violation_total`。適応層が `GridSession::violation_total()`
 *     から写す — **再検証した列に閉じない**。閉じているのは位置の一覧のほうである）で
 *     あり、シート全体の数ではない（生成物の doc。シート全体を保つのは 6.2 の適応層である）。
 *     **値は文書に残っている** — 判定する側（`schema-engine`）は `WriteOrigin::Edit` を決して
 *     拒否せず、`grid_apply_edit` は適合しない値も破棄せずに返す（`src-tauri/src/commands/grid.rs`
 *     の同コマンドの docs）。したがって 3.5 の「保持」は**画面が値を捨てないこと**で満たす
 *
 * **8.4 が引き取るもの**: 違反の印の色（移植口が既定を 1 つ持つ）、違反のバーと巡回
 * （`grid_find_violation` と、違反の位置への現在位置の移動）。**本 module が持つのは、直近の
 * 確定で生じた違反を 1 行使に出して閉じられるようにするところまで**である（位置の一覧は
 * `GridEditOutcome.violations` が運ぶが、**理由の文言はこの経路に無い** — `GridViolation` の
 * `reason` は `grid_find_violation` が組み立てる）。
 *
 * # 8.3〜8.9 への申し送り（本 module が足す予定の場所）
 *
 * - **8.4〜8.9**: 移植口の 4 つの**操作**（`onColumnResize` / `onColumnMove` / `onCopy` /
 *   `onPaste`）を実装で置き換える。8.1 はそれらを `onUnavailable` へ流すだけである
 *   （**黙って何もしない実装にしない** — `onCopy` が空文字を返せばクリップボード
 *   が空になり、`onPaste` が黙って捨てれば貼り付けが消える。無反応より悪い）
 * - **8.4（違反の提示）**: 上の「8.4 が引き取るもの」。**違反の総数をシート全体へ広げる**のも
 *   8.4 である（本 module が受け取る `violation_total` は**シート全体**の数である）
 * - **8.6（行の増減）**: 行数が変わったら `WindowCache.clear(rowCount)`（7.3 の申し送り）。
 *   `SetCells` は行数を変えないので、本 module の経路では要らない
 * - **8.8（列幅・列順）**: 列幅と表示上の列順は `createDisplayState`（7.5）が持つ。変化は
 *   **次の `mount` の仕様**に載せる（`RendererHandle` に幅や順を押し込む口は無い。7.2 の申し送り）。
 * - **列の添字の恒等が崩れるのは 2 つである（8.8 と 8.5）**: いまの並びは恒等であり、入力手段の
 *   選択（`summary.columns` の添字）・編集の宛先（`./cellEdit`）・窓の記憶の列の添字は同じ前提に
 *   立っている。**崩すのは ① 8.8 の列順 ② 8.5 の入れ子の展開**（展開すると `ColumnDescriptor`
 *   の並びが文書の列の添字と一致しなくなる — `view/mod.rs` の `push_column`）。どちらが入る
 *   ときも、**この 3 つを同じ 1 箇所で揃えること**（揃わないと、描かれている値と編集の宛先が
 *   別の列を指す）
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
import type {
  ColumnDescriptor,
  GridCoercionNotice,
  GridEditOutcome,
  GridSheetSummary,
  GridViolationLocation,
} from "../../ipc/bindings";
import { settleCellEdit, type CellEditIntent, type CellEditSettlement } from "./cellEdit";
import { createDisplayState } from "./displayState";
import { editorRegistry } from "./editors";
import type { ColumnConstraints } from "./editorRegistry";
import { WINDOW_ROWS, createWindowCache, type WindowCache } from "./windowCache";
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
      /**
       * 編集中のセル（要件 3.1）。`null` なら編集していない。
       *
       * **この腕が持つことが「描かれている表のセルしか編集できない」を型で表している**
       * （他の腕は持たない）。位置だけでなく**開いた時点の表示文字列**を持つ理由は、
       * 入力手段の初期値（`CellEditorProps.initialText`）が要るためであり、開いた時点で写す
       * 理由は、窓の到着で表が描き直されても**入力中の値が足元で変わらない**ためである。
       */
      readonly editing: CellEdit | null;
    };

/**
 * 編集中の 1 セル（要件 3.1）。
 *
 * `position` は**表示の位置**である（`CellPosition`。移植口の座標と同じ空間）。文書の位置
 * （`GridCellAddress`）へ写すのは確定のときであり、その写像を持つのは窓の記憶である
 * （`./cellEdit` の module doc「宛先は文書の位置である」）。
 */
export interface CellEdit {
  /** 編集しているセル（可視行の序数と列の添字）。 */
  readonly position: CellPosition;
  /** 開いた時点の表示文字列（`WindowCache.getCell` の写し）。 */
  readonly initialText: string;
}

/**
 * 直近の確定の報告（要件 3.4、3.5。**提示の本体は 8.4**）。
 *
 * 2 つを持つのは、この 2 つが**別の要件**であり、片方だけが起きることがあるためである —
 * 変換は起きたが違反は無い（型強制が正常に働いた場合）ことも、その逆もある。1 つの文に
 * 畳むと、どちらの要件の提示なのかが読めなくなる。
 */
export interface CellEditReport {
  /** 型強制の記録（要件 3.4）。**変換の前後をそのまま運ぶ**（解釈は画面に無い）。 */
  readonly coercions: readonly GridCoercionNotice[];
  /** 確定のあとに残っている違反の位置（要件 3.5）。 */
  readonly violations: readonly GridViolationLocation[];
  /**
   * 違反の総数。**シート全体**の数である（適応層が `GridSession::violation_total()` から写す）
   * （生成物の `GridEditOutcome.violation_total` の doc。広げるのは 8.4 である）。
   */
  readonly violationTotal: number;
}

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
  /**
   * 直近の確定の報告（要件 3.4、3.5）。`null` なら出すものが無い（提示する型強制も違反も無い）。
   *
   * **確定のたびに置き換わる**（古い変換の記録を新しい確定に重ねない）。**取消と失敗では
   * 動かない** — どちらも文書を変えていないので、前の報告はまだ「直近の確定」のままである。
   */
  readonly editReport: CellEditReport | null;
}

/** 画面の初期状態（読み込みの前）。 */
export function initialGridScreenModel(): GridScreenModel {
  return { attempt: 0, state: { status: "loading" }, notice: null, editReport: null };
}

/** 読み込みの結果を入れる。**試行の番号は動かさない**（番号が動くと読み込みが走り直す）。 */
export function gridScreenLoaded(
  model: GridScreenModel,
  state: GridScreenState,
): GridScreenModel {
  // 開き直しの結果なので、前の告知と前の確定の報告は落とす（古い失敗・古い変換を新しい表示へ
  // 重ねない。開き直せば表の中身そのものが変わりうる）。
  return { attempt: model.attempt, state, notice: null, editReport: null };
}

/** 器が捕まえない失敗を告知として積む（**内容の領域と報告は変えない**）。 */
export function gridScreenFailed(model: GridScreenModel, message: string): GridScreenModel {
  return {
    attempt: model.attempt,
    state: model.state,
    notice: message,
    editReport: model.editReport,
  };
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
  return {
    attempt: model.attempt,
    state: { ...model.state, selection: next },
    notice: model.notice,
    editReport: model.editReport,
  };
}

/** 告知を閉じる。 */
export function gridScreenNoticeDismissed(model: GridScreenModel): GridScreenModel {
  return {
    attempt: model.attempt,
    state: model.state,
    notice: null,
    editReport: model.editReport,
  };
}

/** 再試行する（**番号を進め、読み込みの状態へ戻す**）。 */
export function gridScreenRetried(model: GridScreenModel): GridScreenModel {
  return { attempt: model.attempt + 1, state: { status: "loading" }, notice: null, editReport: null };
}

/**
 * セルの編集を開く（要件 3.1）。**表を描いていないときは何もしない** — 描かれていないセルは
 * 編集できない（`ready` の腕だけが `editing` を持つ。上の型）。
 *
 * 初期値は**呼び出し側が渡す**（開いた時点の表示文字列）。画面の状態に入れるのは、窓の到着で
 * 表が描き直されても入力中の値が動かないようにするためである（`CellEdit` の doc）。
 */
export function gridScreenEditStarted(
  model: GridScreenModel,
  position: CellPosition,
  initialText: string,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  return {
    attempt: model.attempt,
    state: { ...model.state, editing: { position, initialText } },
    notice: model.notice,
    editReport: model.editReport,
  };
}

/**
 * 確定の 1 往復（`./cellEdit`）の結果を画面へ反映する（要件 3.3、3.4、3.5、3.6）。
 *
 * | 結果 | 何が起きるか |
 * |---|---|
 * | 取消 | **入力手段を閉じるだけである。**文書も表示も報告も動かない（要件 3.6 — 適用の前なので、値はまだ変わっていない） |
 * | 適用 | 入力手段を閉じ、**報告を置き換える**（型強制と、残った違反。要件 3.4、3.5） |
 * | 適用できなかった | **入力手段は開いたままにする。**適用されていないので、打たれている値を閉じて捨てる理由が無い（理由は 8.1 の告知として出す） |
 *
 * 報告が `null` になるのは、**提示するものが 1 つも無い**ときである（変換も違反も無い確定）—
 * そのときは古い報告も落ちる（報告は「直近の確定」を指す。前の変換を新しい確定に重ねない）。
 */
export function gridScreenEditSettled(
  model: GridScreenModel,
  settlement: CellEditSettlement,
): GridScreenModel {
  switch (settlement.status) {
    case "cancelled":
      return gridScreenEditClosed(model);
    case "failed":
      // **適用されていない。**打たれている値を閉じて捨てる理由が無いので、開いたままにする。
      return gridScreenFailed(model, `編集を適用できませんでした: ${settlement.message}`);
    case "applied": {
      const closed = gridScreenEditClosed(model);
      return { ...closed, editReport: reportOf(settlement.outcome) };
    }
    default:
      return assertNever(settlement, "確定の結果の分岐が網羅されていない");
  }
}

/** 確定の報告を閉じる（**文書も値も動かない** — 提示を消すだけである）。 */
export function gridScreenEditReportDismissed(model: GridScreenModel): GridScreenModel {
  return {
    attempt: model.attempt,
    state: model.state,
    notice: model.notice,
    editReport: null,
  };
}

/** 入力手段を閉じる（**表を描いていないときは何もしない**）。 */
function gridScreenEditClosed(model: GridScreenModel): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  return {
    attempt: model.attempt,
    state: { ...model.state, editing: null },
    notice: model.notice,
    editReport: model.editReport,
  };
}

/**
 * 適用の結果を報告へ写す（要件 3.4、3.5）。
 *
 * **解釈を 1 つも足さない** — 変換の前後の表示文字列も、違反の位置も、総数も、境界が運んだ値を
 * そのまま置く。出すものが 1 つも無ければ `null` を返す（空の報告を出しても、利用者には
 * 「何かあった」と読める枠が残るだけである）。
 */
function reportOf(outcome: GridEditOutcome | null): CellEditReport | null {
  if (outcome === null) {
    // 適用では起こらない（生成物の doc）。`grid_history` の腕のための守りである。
    return null;
  }
  const nothing =
    outcome.coercions.length === 0 &&
    outcome.violations.length === 0 &&
    outcome.violation_total === 0;
  if (nothing) {
    return null;
  }
  return {
    coercions: outcome.coercions,
    violations: outcome.violations,
    violationTotal: outcome.violation_total,
  };
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
    // 開いた直後は編集していない（要件 3.1。編集は利用者の起動で始まる）。
    editing: null,
  };
}

// ===========================================================================
// 3. 移植口へ渡す仕様（**キャンバスを要さない純粋な部分**）
// ===========================================================================

/**
 * まだ結線していない操作の名前（**利用者に見える語**である）。8.4〜8.9 がそれぞれ実装したら、
 * その名前はここから落ちる（8.3 が「セルの編集の起動」を実装したので、それはもう無い）。
 */
const OPERATION_NAMES = {
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
 * 編集の起動は 8.3 が結線した（要件 3.1）。**初期値をここで引く**（`getCell`）のは、入力手段に
 * 見せるべき値が**いま描かれている値そのもの**だからである — 画面が別の経路で値を持ち寄ると、
 * 描かれている値と打ち直しの初期値が食い違いうる。
 *
 * 残る 4 つは**操作**であり（8.4〜8.9 の担当）、本 module はそれらを [`onUnavailable`] へ流す。
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
  /** 編集の起動（要件 3.1）。**位置と、いま描かれている値**を渡す。 */
  readonly onActivateEditor: (position: CellPosition, initialText: string) => void;
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
    onActivateEditor: (position) => {
      // **描かれている値が初期値である**（要件 3.1）。引く口は同期であり投げないので、
      // ここで例外が画面を巻き込むことはない（`RendererSpec.getCell` の不変条件）。
      options.onActivateEditor(position, options.getCell(position).text);
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
  /** 編集中のセル（要件 3.1）。`null` なら編集していない。 */
  readonly editing: CellEdit | null;
  /**
   * 境界の口。**編集の 1 往復（`./cellEdit`）が使う** — カードの記憶（下）と組にするのは、
   * 確定が宛先（窓の行の識別子）と、適用のあとの作り直し（影響を受けた行の窓を捨てる）の
   * 両方を要するためである。
   */
  readonly client: GridClient;
  /** 選択が変わった（打鍵・ポインタのどちらでも）ことを画面へ上げる口。 */
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
  /** 編集が起動された（要件 3.1。位置と、いま描かれている値）ことを画面へ上げる口。 */
  readonly onEditStarted: (position: CellPosition, initialText: string) => void;
  /** 確定の 1 往復の結果を画面へ上げる口（要件 3.3、3.4、3.5、3.6）。 */
  readonly onEditSettled: (settlement: CellEditSettlement) => void;
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
  editing,
  client,
  onSelectionChange,
  onEditStarted,
  onEditSettled,
  onUnavailable,
}: GridSurfaceProps): ReactElement {
  const containerRef = useRef<HTMLDivElement | null>(null);
  // 移植口の取っ手。**窓の到着（非同期）と選択の効果が使う**ので、効果の外に置く。
  const handleRef = useRef<RendererHandle | null>(null);
  /**
   * 窓の記憶。**描画（編集の面）からも使う**ので ref に置く — 組み立てはマウントの効果の中で
   * 行われるが、確定は利用者の操作（描画の外の出来事）から来る。
   */
  const cacheRef = useRef<WindowCache | null>(null);
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
        // 編集の起動（要件 3.1）。**現在位置と、いま描かれている値**を画面へ上げる。
        onActivateEditor: onEditStarted,
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
    cacheRef.current = cache;
    // 開いた直後の先読み（要件 1.4）。**上の見当を渡す** — 本物の区間は実装が知らせてくる。
    cache.setVisibleSpan(openingSpan.rows);

    return () => {
      handle.destroy();
      cache.dispose();
      handleRef.current = null;
      cacheRef.current = null;
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

  /**
   * 入力手段の 2 つの口を、境界への 1 往復へ渡す（要件 3.3、3.6）。
   *
   * **画面の状態を変えるのは結果を受け取った側である**（`onEditSettled`）。ここが担うのは
   * 往復そのものであり、窓の記憶を要する（宛先の行の識別子と、適用のあとの作り直し）。
   * 例外を投げない — `settleCellEdit` が投げないので、この非同期の経路も投げない。
   */
  const settle = (position: CellPosition, intent: CellEditIntent): void => {
    const cache = cacheRef.current;
    if (cache === null) {
      // 器がまだ無い（描かれていない）。編集も開いていないので、ここへは来ない。
      return;
    }
    void settleCellEdit({ client, cache, position, intent }).then(onEditSettled);
  };

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
      {editing === null ? null : (
        <CellEditorPanel
          edit={editing}
          column={summary.columns[editing.position.column] ?? null}
          onCommit={(text) => {
            settle(editing.position, { kind: "commit", text });
          }}
          onCancel={() => {
            settle(editing.position, { kind: "cancel" });
          }}
        />
      )}
      <div
        ref={containerRef}
        onKeyDown={onKeyDown}
        data-testid="jxcel-grid-table"
        style={TABLE_STYLE}
      />
    </div>
  );
}

/**
 * 編集中の 1 セルの面（要件 3.1）。**入力手段を登録簿から引く唯一の場所である。**
 *
 * 型ごとの分岐はここに 1 つも無い — 解決するのは `CellEditorRegistry.resolve` であり、未登録の
 * 札は登録簿が既定（値をそのまま扱う面）へ落とす（要件 10.3、10.4）。**本 module は入力手段の
 * 成分を名指ししない**（`editorRegistry.test.ts` が源の走査で固定する）。
 *
 * 面は**表の器の外**に出る。移植口に「セルの上へ DOM を重ねる」口は無く（`RendererSpec` に
 * そんな欄は無い）、覆われたセルを探させるより、**どのセルを編集しているかを名乗る**方が読める。
 */
function CellEditorPanel({
  edit,
  column,
  onCommit,
  onCancel,
}: {
  readonly edit: CellEdit;
  /** その列の宣言（表示位置の列）。使用不能な列は `null`（札が読めない）。 */
  readonly column: ColumnDescriptor | null;
  readonly onCommit: (text: string) => void;
  readonly onCancel: () => void;
}): ReactElement {
  // 札が読めない列は `Any`（値をそのまま扱う面）として登録簿へ来る（7.4 の module doc）。
  const kind = column?.kind ?? "Any";
  const Editor = editorRegistry.resolve(kind);
  /**
   * 入力手段が読む宣言（要件 3.1、3.7）。
   *
   * `kind` だけが境界から来る。`nullable` は**境界に欄が無い**（`ColumnDescriptor` は
   * `column / path / name / kind / element_count / expandability` しか運ばない）ので、
   * **つねに「値なしの道を出す」**を渡す — 道を閉じると、値なしを許す列で値なしへ戻せなくなる
   * （要件 3.7）。値なしを許さない列では、判定がそれを違反として返し、**値は保持される**
   * （要件 3.5）。`choices` / `reference` / `members` も同じく材料が無い（下の申し送り）。
   */
  const constraints: ColumnConstraints = { kind, nullable: true };

  return (
    <div
      data-testid="jxcel-grid-editor"
      data-editor-row={edit.position.row}
      data-editor-column={edit.position.column}
      data-editor-kind={kind}
      style={EDITOR_STYLE}
    >
      <span style={MESSAGE_STYLE}>
        {`${String(edit.position.row + 1)} 行 ${String(edit.position.column + 1)} 列を編集中`}
      </span>
      <Editor
        initialText={edit.initialText}
        constraints={constraints}
        commit={onCommit}
        cancel={onCancel}
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

/** 編集中のセルの面（**表の器の外に出す**。どのセルを編集しているかを名乗る）。 */
const EDITOR_STYLE = {
  display: "flex",
  flexWrap: "wrap",
  alignItems: "center",
  gap: "0.5rem",
  padding: "0.5rem 0.75rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlActiveBackground})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 確定の報告の中身（型強制の一覧と、残った違反）。 */
const REPORT_BODY_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.25rem",
} as const;

/** 型強制の一覧（要件 3.4）。**1 件 1 行である。** */
const REPORT_LIST_STYLE = {
  margin: 0,
  padding: 0,
  listStyle: "none",
  fontSize: "0.8125rem",
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
  client,
  onRetry,
  onUnavailable,
  onSelectionChange,
  onEditStarted,
  onEditSettled,
}: {
  readonly model: GridScreenModel;
  /** 境界の口（表を描く腕が、編集の 1 往復に使う）。 */
  readonly client: GridClient;
  readonly onRetry: () => void;
  readonly onUnavailable: (operation: string) => void;
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
  readonly onEditStarted: (position: CellPosition, initialText: string) => void;
  readonly onEditSettled: (settlement: CellEditSettlement) => void;
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
          editing={state.editing}
          client={client}
          onSelectionChange={onSelectionChange}
          onEditStarted={onEditStarted}
          onEditSettled={onEditSettled}
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
  /**
   * 境界の口。**表を描く腕が編集の 1 往復に使う**（入力手段の 2 つの口 → `./cellEdit`）。
   * 差し替えの口である（検査は偽の実装を渡せる。8.1 の `loadGridScreenState` と同じ規律）。
   */
  readonly client: GridClient;
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
  /** 編集が起動された（要件 3.1。位置と、いま描かれている値）。 */
  readonly onEditStarted: (position: CellPosition, initialText: string) => void;
  /** 確定の 1 往復の結果（要件 3.3、3.4、3.5、3.6）。 */
  readonly onEditSettled: (settlement: CellEditSettlement) => void;
  /** 直近の確定の報告を閉じる（**文書も値も動かない**）。 */
  readonly onDismissEditReport: () => void;
}

/**
 * 画面の見た目。**状態だけを受け取る純粋な描画である**ので、検査は状態ごとにこれを呼んで
 * 「何が DOM へ出るか」を読める（`GridScreen.test.ts`）。
 */
export function GridScreenView({
  model,
  client,
  onRetry,
  onDismissNotice,
  onDismissEditReport,
  onUnavailable,
  onSelectionChange,
  onEditStarted,
  onEditSettled,
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
      {model.editReport === null ? null : (
        <div data-testid="jxcel-grid-edit-report" role="status" style={NOTICE_STYLE}>
          <div style={REPORT_BODY_STYLE}>
            {/*
              型強制（要件 3.4）。**変換が起きたこと**と、**変換前の値**を出す。前後の表示文字列は
              どちらも境界が運んだものである（`GridCoercionNotice` の doc）— 画面は解釈しない。
            */}
            {model.editReport.coercions.length === 0 ? null : (
              <ul data-testid="jxcel-grid-coercions" style={REPORT_LIST_STYLE}>
                {model.editReport.coercions.map((coercion) => (
                  <li
                    key={`${coercion.cell.row}:${String(coercion.cell.column)}`}
                    data-coercion-row={coercion.cell.row}
                    data-coercion-column={coercion.cell.column}
                    data-coercion-before={coercion.before}
                    data-coercion-after={coercion.after}
                  >
                    {`型強制: 行「${coercion.cell.row}」の ${String(coercion.cell.column + 1)} 列目 — 変換前「${coercion.before}」／変換後「${coercion.after}」`}
                  </li>
                ))}
              </ul>
            )}
            {/*
              残った違反（要件 3.5。**提示の本体は 8.4**）。**値は文書に残っている** — 判定する
              側は編集を決して拒否せず、適合しない値も破棄せずに返す（`grid_apply_edit` の doc）。
              総数は**シート全体**の数である（再検証した列に閉じているのは、下に並ぶ位置のほうである）。
            */}
            {model.editReport.violationTotal === 0 ? null : (
              <p
                data-testid="jxcel-grid-violations"
                data-violation-total={model.editReport.violationTotal}
                data-violation-count={model.editReport.violations.length}
                style={MESSAGE_STYLE}
              >
                {`違反 ${String(model.editReport.violationTotal)} 件（シート全体の総数）${
                  model.editReport.violations.length === 0
                    ? ""
                    : `: ${model.editReport.violations
                        .map(
                          (violation) =>
                            `行「${violation.row ?? "（行なし）"}」の ${String(violation.column + 1)} 列目`,
                        )
                        .join("、")}`
                }`}
              </p>
            )}
          </div>
          <button
            type="button"
            data-testid="jxcel-grid-edit-report-dismiss"
            onClick={onDismissEditReport}
            style={BUTTON_STYLE}
          >
            閉じる
          </button>
        </div>
      )}
      <GridScreenBody
        model={model}
        client={client}
        onRetry={onRetry}
        onUnavailable={onUnavailable}
        onSelectionChange={onSelectionChange}
        onEditStarted={onEditStarted}
        onEditSettled={onEditSettled}
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
  const startEdit = useCallback((position: CellPosition, initialText: string) => {
    setModel((current) => gridScreenEditStarted(current, position, initialText));
  }, []);
  const settleEdit = useCallback((settlement: CellEditSettlement) => {
    // **非同期の結果である**（`ScreenBoundary` は効果の同期の例外しか捕まえない）。遷移は
    // 全域であり、投げない（`gridScreenEditSettled`）。
    setModel((current) => gridScreenEditSettled(current, settlement));
  }, []);
  const dismissEditReport = useCallback(() => {
    setModel(gridScreenEditReportDismissed);
  }, []);

  return (
    <GridScreenView
      model={model}
      client={DEFAULT_CLIENT}
      onRetry={retry}
      onDismissNotice={dismissNotice}
      onDismissEditReport={dismissEditReport}
      onUnavailable={noteUnavailable}
      onSelectionChange={select}
      onEditStarted={startEdit}
      onEditSettled={settleEdit}
    />
  );
}

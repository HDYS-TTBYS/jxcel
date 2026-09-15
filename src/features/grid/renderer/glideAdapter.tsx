/**
 * 描画層の移植口（`./port`）の実装 — グリッドライブラリ `@glideapps/glide-data-grid`
 * `6.0.4-alpha24` を**移植口の背後に置く**写し（tasks.md 7.2。要件 1.2, 1.3, 2.4, 7.1, 7.2）。
 *
 * 所有: `GlideAdapter`（design.md「RendererPort と GlideAdapter」）。移植口そのものは 7.1 が
 * 定義した（`./port`）— **本ファイルはその面を実装するだけであり、面を広げない。**
 *
 * # なぜ移植口を挟むのか（そして、それを実測で確かめる場所）
 *
 * 上流が止まっている（stable は React 19 を受け付けず beta に固定している。research.md
 * 「グリッドライブラリの選定」）。退路は「同じ移植口の背後で自前 canvas へ切り替える」ことで
 * あり、そのためには**移植口を通る呼び出しの並びが実装に依存しない**ことが要る。7.1 の
 * `port.test.ts` が 2 つの偽の実装で並びを固定し、**7.2 の本実装をその比較へ 1 行足した**
 * （`port.test.ts` の「実装の差し替え」の節。実物が同じ並びを保つことの実測である）。
 *
 * 1.6 の実測（10 万行 × 30 列の走査でフレーム時間の中央値 17.00 ms）は**この実装のまま進む**
 * 判断である（research.md「採否の判断（7.2）」）。自前 canvas への切り替えは行わない。
 *
 * # この実装が使うライブラリの面（**わざと狭く保つ**）
 *
 * 移植口が求めるのは「描画・当たり判定・文字計測・クリップボードの配管」だけである。したがって
 * 本ファイルがライブラリへ渡すのは `DataEditor` の 1 つの部品と、その props だけであり、
 * 並べ替え・絞り込み・行モデル・判定（適合するか）には一切触れない — それらは Rust 側にある
 * （design.md「窓単位で運ぶ」）。**上流が止まっても差し替えられる面積を最小に保つ**のが、
 * 移植口を挟んだ理由そのものである。
 *
 * # 層の分け方（**この分け方が本ファイルの骨格である**）
 *
 * | | 何を持つか | どこで動くか |
 * |---|---|---|
 * | [`createGlideWiring`] | 仕様 → Glide の props、Glide の通知 → 移植口の callback、選択の所有 | **DOM を持たない**（`node` 環境で検査できる） |
 * | [`GlideSurface`] | `DataEditor` を描き、DOM の `copy` / `paste` を受け、クリップボードへ書く | 実物の canvas を要する |
 * | [`createGlideAdapter`] | 上の 2 つを繋ぎ、`GridRendererPort` として出す | 実物の器を要する |
 *
 * 分けてあるのは**検査の都合ではなく責務の違い**である。移植口の契約（写像と知らせ）は
 * 純粋な論理であり、canvas を要求しない。実物の canvas を要求するのは「描く」ことであり、
 * それは `GlideSurface` の側だけである。したがって:
 *
 *   - **配線は `vitest`（`environment: "node"`）が検査する**（`glideAdapter.test.ts`）。
 *     ここで固定できるのは写像と callback の配管であり、**見え方ではない。**
 *   - **見え方（10 万行の走査・選択の区別・列幅と列の位置）は実物を起動して観測する。**
 *     `tech.md`「GUI・配布物・プラットフォーム差を含む主張は、実物を起動して観測した結果で
 *     裏付けること。単体テストは回帰の網であって受入の証明ではない」に従い、観測の実体を
 *     `src/features/smoke/portProbe*` と `scripts/check-port-interaction.sh` に置いた
 *     （1.6 の使い捨ての段と同じ形。**新しい系統は作らない**）。
 *
 * # 選択は画面が持ち、この写しは指示されたとおりに描く（8.2 が向きを決めた）
 *
 * 7.1 と 7.2 は「選択は実装が持ち、外へ報せる一方通行」としていた（`RendererSpec` に下ろす欄が
 * 無かったためである）。**8.2 が向きを変えた**: 選択と現在位置は画面が持ち、仕様
 * （[`RendererSpec.selection`]。マウントの時点の値）と [`RendererHandle.setSelection`]（以後の
 * 更新）で下ろす。理由は `selection.ts` の module doc にある（移動の意味論が製品の要件であり、
 * ライブラリの中に置けない）。
 *
 * それでも**写しは 1 つ**である。[`GlideWiring`] の `selection` は「いま描いている選択」そのもの
 * であり、`GlideSurface` はそれを `useSyncExternalStore` で購読して `DataEditor` の制御選択の
 * props へ渡す。外へ報せる値（`onSelectionChange`）は**その同じ 1 つの値から導く** — 画面の写しと
 * 描かれているものがずれる余地を作らない。
 *
 * 下ろされた選択は**報せ返さない**（報せ返すと往復し、ずれの種になる）。報せるのは**実装が
 * 起こした**変化だけである: 利用者のポインタ、行見出しの操作、Glide の側に残した打鍵の束縛
 * （端への移動など）。
 *
 * # 打鍵の束縛のうち、どれをこの写しが止めるか
 *
 * 止めるのは [`GLIDE_KEYBINDINGS`] にある 3 種類（クリップボード・移動と範囲の広げ・行/列の
 * 全体）である。**移動を止めるのは、要件 2.2 の「隣接するセルへ移る」と端の扱いが製品の要件
 * だからである** — ライブラリの打鍵処理に置くと、移植口を差し替えたときに要件が消える
 * （`selection.ts` の module doc）。止めないもの（端への移動・ページ・表の全体・Tab・編集の起動）
 * は画面が引き受けないので、そのまま働かせる。
 *
 * 行見出し列（`rowMarkers`）は**画面が決める**（`RendererSpec.rowMarkers`）。Glide は行見出しの
 * ぶんの添字を内部で補正する（`getCellContent` / `onColumnResize` / `onSelectionChange` の
 * 正規化 / `onVisibleRegionChanged` / `scrollTo` のいずれも）ので、写しは値を渡すだけでよい。
 *
 * # 列幅・列順は次の `mount` でしか表示へ反映できない
 *
 * `RendererHandle` が選択のための口を 1 つ持つ（`setSelection`）以外は変わらない。列幅・列順の
 * 変化は**次に渡される仕様**に載るほかない。本実装は「マウントのたびに仕様を受け取り、そのとおりに
 * 描く」だけであり、`onColumnResize` / `onColumnMove` は外向きの知らせに徹する（移植口を
 * 使う側が、新しい仕様で `mount` し直すかどうかを決める）。**8.8 が列幅・列順の操作を結線する
 * ときに効いてくる**（design.md の同じ申し送り）。
 *
 * # `0.4` の 3 つ（**判断をここに書く**）
 *
 * 1. **`getCell` は同期であり例外を投げない。** Glide の `getCellContent` は引きに来る形
 *    であり、移植口の契約とそのまま噛み合う。本実装は例外を**捕まえない** — 捕まえると
 *    仕様の側の契約違反（例外を投げる `getCell`）が見えなくなる。移植口の不変条件を守るのは
 *    仕様の側の責任である（`./port` の `RendererSpec.getCell` の docs）。
 * 2. **`loading: true` は Glide の読み込み中のセル（骨組みの棒）として描く。**
 *    `GridCellKind.Loading` は `skeletonWidth` が未定義・0 のとき**何も塗らない**ので、列の幅から
 *    決めた幅を与える（「読み込み中に見える」ことの条件）。**空白のセルで代用しない** — 空白は
 *    「値なし」と区別がつかない（要件 1.4）。
 * 3. **`violated: true` は地色の上書きで示す。** 移植口は判定を下さず、窓が運んできた印を
 *    そのまま渡すだけである（`./port` のヘッダ）。色を運ぶ欄が移植口に無いので、写しが既定を
 *    1 つ持つ（下の [`VIOLATION_THEME`]）。
 *
 * # クリップボードは移植口を唯一の経路にする
 *
 * Glide 自身も複製と貼り付けを持つが、**その経路は移植口を通らない**（Glide は表示文字列から
 * 独自に表形式を組み立て、貼り付けでは中身を解釈する）。移植口の契約は逆である —
 * 「複製の文字列を作るのは呼び出し側（Rust 側の `PasteCodec` と適用層）」であり、「貼り付けの
 * 文字列を移植口は解釈しない」。したがって:
 *
 *   - Glide の複製・切り取り・貼り付けは `keybindings` で**止める**（`copy` / `cut` / `paste`）。
 *   - DOM の `copy` と `paste` は `GlideSurface` が自分で受ける（`preventDefault` する）。
 *     経路は `RendererSpec.onCopy` / `onPaste` の 1 本だけになる。
 *
 * これは要件 7.1 / 7.2 の「行と列の配置を保った形」「一般的な表形式」を**移植口の側の 1 箇所**で
 * 決めるための措置である（表形式の規則を 2 箇所に持たない）。
 *
 * # この実装が知らないこと（**知らないことが契約である**）
 *
 * 編集の意味論・型の判定・取り消しとやり直し・並べ替え・絞り込み・行の順序。移植口が運ぶのは
 * 座標・表示の内容・列の見出しだけであり、本ファイルもそれ以上を運ばない。とくに:
 *
 *   - **セルは `readonly` として描く。** 移植口は編集の意味論を知らないので、Glide 自身の
 *     編集器を開かせない（入力手段の選択は `EditorRegistry`（7.4）が列の位置から行う）。
 *     編集の起動は `onActivateEditor` として外へ報せるだけである。
 *   - **値は常に文字のセルである。** `Int` や `Decimal` は表示文字列として運ばれ（design.md
 *     「境界に数値を出さない」）、写しは数値へ解釈しない（64 ビット整数が壊れる経路を作らない）。
 *     型の札は**右寄せの判断にだけ**使う。
 */
import {
  CompactSelection,
  DataEditor,
  GridCellKind,
  emptyGridSelection,
  type DataEditorRef,
  type GridCell,
  type GridColumn,
  type GridSelection,
  type Item,
  type Rectangle,
  type Theme,
} from "@glideapps/glide-data-grid";
// ライブラリのスタイル。**スクロールの成立そのものがこの CSS に依る**（`.dvn-scroller` の
// `overflow` は linaria が展開したクラス規則が与える）ので、省くと広がりが観測できない。
//
// **これは配布物にも入る**（本モジュールは移植口の実装であり、検証専用ではない）。1.5 の
// 使い捨ての面（`src/features/smoke/glideProbeGrid.tsx`）が「既定のビルドへ入れない」ために
// 動的 import で括られていたのとは事情が逆である — 移植口の実装は配布物で動くので、
// そのスタイルも配布物に要る（`scripts/check-shipping-bundle.sh` が禁じているのは**検証専用の
// 識別子**だけであり、ライブラリ本体とスタイルは配布物に入ってよい）。
import "@glideapps/glide-data-grid/dist/index.css";
import {
  useCallback,
  useEffect,
  useRef,
  useSyncExternalStore,
  type CSSProperties,
  type ReactElement,
} from "react";
import { createRoot } from "react-dom/client";

// **型だけの取り込みである**（移植口と同じ規律。`./port` のヘッダを参照）。
import type { TypeKindTag } from "../../../ipc/bindings";
import type {
  CellPosition,
  CellRange,
  GridRendererPort,
  RenderCell,
  RendererHandle,
  RendererSelection,
  RendererSpec,
  RowMarkerMode,
  RowSpan,
} from "./port";

/**
 * 行の高さ（ピクセル）。**移植口に高さを運ぶ欄が無い**（`RendererSpec` は行の高さも見出しの
 * 高さも持たない）ので、実装が既定を決める。8.1 が高さを決められるようにするには移植口の面を
 * 広げる判断が要る（面は 7.1 が固定した）。
 */
export const GLIDE_ADAPTER_ROW_HEIGHT = 34;

/** 列見出しの高さ（ピクセル）。上の理由で実装が決める。 */
export const GLIDE_ADAPTER_HEADER_HEIGHT = 36;

/** 読み込み中の骨組みの棒の幅を列の幅から決める割合。 */
const LOADING_SKELETON_RATIO = 0.5;

/** 骨組みの棒の最小の幅（ピクセル）。**0 にしない**（0 のとき Glide は何も塗らない）。 */
const LOADING_SKELETON_MIN_WIDTH = 16;

/**
 * 違反のセルの地色。**移植口に色を運ぶ欄が無い**ので、写しが既定を 1 つ持つ。
 *
 * 移植口は判定を下さない（印は窓が運んでくる）。色は「区別できる形で提示する」（要件 4.1）の
 * 実装であり、配色の決定そのものではない。8.1 が配色を決めるときは、`RendererSpec` に欄を
 * 足すか、この写しへ外から渡す口を作る判断になる（面の改訂は 7.1 の契約に触る）。
 */
const VIOLATION_THEME: Partial<Theme> = { bgCell: "#ffe0e0" };

/** 右寄せで描く葉の型（数値として読まれるもの）。**値は解釈しない。見た目だけである。** */
const RIGHT_ALIGNED_KINDS: Partial<Record<TypeKindTag, true>> = {
  Int: true,
  Float: true,
  Decimal: true,
};

/**
 * Glide へ渡す打鍵の束縛のうち、**この写しが止めるもの**（8.2）。
 *
 * 止めるのは 3 種類である。
 *
 *   1. **複製・切り取り・貼り付け**（7.2）。Glide の経路は移植口を通らない（モジュール doc）。
 *   2. **移動と範囲の広げ**（`go*Cell` / `selectGrow*` / `*RetainSelection`）。要件 2.2 の
 *      「隣接するセルへ移る」と端の扱いは**製品の要件であり、ライブラリの打鍵処理の中に
 *      置けない**（移植口を挟んだ理由は差し替え可能性である。`selection.ts` の module doc）。
 *      画面（`selection.ts`）が扱い、この写しは Glide の側の経路を閉じる。
 *   3. **行の全体・列の全体**（`selectRow` / `selectColumn`）。同じ理由で画面が扱う。
 *
 * **止めないもの（そのまま働かせる）**: 先頭・末尾への移動（`goToFirst*` / `goToLast*`。
 * 要件 1.3、1.4）、ページの移動（`goToNextPage` / `goToPreviousPage`）、表の全体（`selectAll`）、
 * 端までの範囲の選択（`selectToFirst*` / `selectToLast*`）、編集の起動（`activateCell`）。
 * これらは**画面が引き受けない**ので、止めれば機能が黙って消える。
 *
 * `goLeftCell` / `goRightCell` を `false` にせず Tab だけを残してあるのは、**Tab が器の焦点の
 * 移動でもある**ためである（止めると Tab がグリッドの外へ逃げ、見えている表を離れる）。
 */
const GLIDE_KEYBINDINGS = {
  copy: false,
  cut: false,
  paste: false,
  goUpCell: false,
  goDownCell: false,
  goLeftCell: "shift+Tab",
  goRightCell: "Tab",
  goUpCellRetainSelection: false,
  goDownCellRetainSelection: false,
  goLeftCellRetainSelection: false,
  goRightCellRetainSelection: false,
  selectGrowUp: false,
  selectGrowDown: false,
  selectGrowLeft: false,
  selectGrowRight: false,
  selectRow: false,
  selectColumn: false,
} as const;

/** 上の束縛の型（配線の props が名指しする）。**値は Glide が解釈する綴りである。** */
export type GlideKeybindings = typeof GLIDE_KEYBINDINGS;

/**
 * `DataEditor` へ渡すもの。**Glide の props の signature そのまま**である（写像を薄く保つ —
 * 上流が止まっても差し替えられる面積を最小にするのが移植口を挟んだ理由である）。
 *
 * 選択の 2 つ（`gridSelection` / `onGridSelectionChange`）のうち、**`gridSelection` はここに
 * 無い**。選択は配線が所有し、面が [`GlideWiring.selection`] を読んで渡す（制御選択）。
 */
export interface GlideWiringProps {
  /** 左から順に描く列。表示順であり、幅を含む。 */
  readonly columns: readonly GridColumn[];
  /** 可視行の総数。 */
  readonly rows: number;
  /** 行見出し列の出し方（`RendererSpec.rowMarkers` の写し）。要件 2.3。 */
  readonly rowMarkers: RowMarkerMode;
  /** セルを引く。**同期であり、例外を投げない**（移植口の不変条件に乗る）。 */
  readonly getCellContent: (item: Item) => GridCell;
  readonly rowHeight: number;
  readonly headerHeight: number;
  /**
   * Glide 自身のクリップボードの操作を止める。**この写しが DOM の `copy` / `paste` を受ける**
   * （モジュール doc「クリップボードは移植口を唯一の経路にする」）。8.2 はこれに加えて
   * **移動・範囲の広げ・行/列の全体の打鍵の束縛も止める**（[`GLIDE_KEYBINDINGS`]）。
   */
  readonly keybindings: GlideKeybindings;
  /**
   * 見えている区間が変わった。**Glide の `Rectangle` はデータの座標である**（行見出しのぶんは
   * Glide が内部で補正する）。移植口へは行と列の区間として渡す（要件 2.4）。
   */
  readonly onVisibleRegionChanged: (
    range: Rectangle,
    tx: number,
    ty: number,
    extras: { readonly selected?: Item; readonly freezeRegion?: Rectangle },
  ) => void;
  /** 編集の起動（打鍵または二度打ち）。**編集の中身は運ばない。** */
  readonly onCellActivated: (item: Item) => void;
  /**
   * 列の幅が変更された。Glide の引数は `(列, 変更後の幅, 列の添字, grow 込みの幅)` である
   * （**移植口へ渡すのは添字と変更後の幅だけ**であり、grow 込みの幅は使わない）。
   */
  readonly onColumnResize: (
    column: GridColumn,
    newSize: number,
    colIndex: number,
    newSizeWithGrow: number,
  ) => void;
  /** 列の位置が変更された（表示順の位置どうし）。 */
  readonly onColumnMoved: (startIndex: number, endIndex: number) => void;
  /** 選択が変わった。**これと `selection` を渡すと Glide の選択は制御になる。** */
  readonly onGridSelectionChange: (selection: GridSelection) => void;
}

/**
 * Glide の部品（`DataEditor`）への参照。**React の面が埋め、配線が使う**（`scrollTo` と
 * `invalidate` の写し先）。名前を付けてあるのは、これが層の境界だからである。
 */
export interface GlideSurfaceRef {
  current: DataEditorRef | null;
}

/**
 * 移植口の実装のうち **DOM を持たない部分**（仕様 → Glide の props、Glide の通知 → 移植口の
 * callback、選択の所有、呼び出し側の口の写し）。
 *
 * **これが移植口の意味論の全体である。** `GlideSurface` はこれを `DataEditor` に繋ぐだけであり、
 * 検査（`glideAdapter.test.ts`）はこの面を直接叩く — DOM も canvas も要らない。
 */
export interface GlideWiring {
  /** `DataEditor` へ渡す props（選択の 2 つを除く。上の docs）。 */
  readonly props: GlideWiringProps;
  /** いまの選択。**配線が保持する唯一の写しである**（面はこれを描くだけである）。 */
  readonly selection: GridSelection;
  /** 選択の変化の購読（React の面が `useSyncExternalStore` で使う）。解除の関数を返す。 */
  subscribeSelection(listener: () => void): () => void;
  /** 選択が変わった（`DataEditor` の `onGridSelectionChange` から）。 */
  selectionChanged(selection: GridSelection): void;
  /** いまの選択の範囲（正規化済み。選択が無ければ `null`）。 */
  currentRange(): CellRange | null;
  /** 貼り付けの錨（Glide の規則の写し。選択が無ければ `null` = 貼り付けない）。 */
  currentAnchor(): CellPosition | null;
  /** 複製する。**文字列を作るのは呼び出し側である**（移植口は素通しする）。 */
  copyRange(range: CellRange): Promise<string>;
  /** 貼り付ける。**文字列を解釈しない**（解釈は Rust 側の `PasteCodec` の仕事である）。 */
  pasteAt(anchor: CellPosition, text: string): Promise<void>;
  /** React の面が埋める参照（`scrollTo` と `invalidate` の宛先）。 */
  readonly surfaceRef: GlideSurfaceRef;
  /** 呼び出し側が使う口。**破棄の後は知らせの経路が投げる**（7.1 の偽の実装と同じ規律）。 */
  readonly handle: RendererHandle;
}

/** 選択が無いことを表す値（Glide が定義する 1 つの定数。写しを作らない）。 */
const NO_SELECTION = emptyGridSelection;

/**
 * Glide の選択を移植口の範囲へ正規化する。**移植口の矩形は 1 つである**（`CellRange`）ので、
 * 行の全体・列の全体も同じ形（矩形）に写す。数えるのは写す側である。
 *
 *   - `current`（矩形）があればそれを使う。
 *   - 列の全体の選択（見出しの操作。要件 2.3）は**全行にまたがる矩形**にする。
 *   - 行の全体の選択（行見出しの操作。要件 2.3）は**全列にまたがる矩形**にする。
 *
 * **飛び飛びの選択は最小から最大までの 1 つの矩形へ潰れる**（移植口の面が矩形 1 つしか持たない
 * ためである）。連続した矩形（要件 2.3 の 1 つ目）はそのまま写る。
 */
function rangeOfSelection(
  selection: GridSelection,
  rowCount: number,
  columnCount: number,
): CellRange | null {
  const current = selection.current;
  if (current !== undefined) {
    const { x, y, width, height } = current.range;
    return {
      start: { row: y, column: x },
      end: { row: y + height - 1, column: x + width - 1 },
    };
  }
  const firstColumn = selection.columns.first();
  const lastColumn = selection.columns.last();
  if (firstColumn !== undefined && lastColumn !== undefined) {
    return { start: { row: 0, column: firstColumn }, end: { row: rowCount - 1, column: lastColumn } };
  }
  const firstRow = selection.rows.first();
  const lastRow = selection.rows.last();
  if (firstRow !== undefined && lastRow !== undefined) {
    return { start: { row: firstRow, column: 0 }, end: { row: lastRow, column: columnCount - 1 } };
  }
  return null;
}

/**
 * 貼り付けの錨。**Glide 自身の規則の写しである**（`onPasteInternal` が宛先を決める順序）:
 * 現在のセル → 列が 1 つだけならその列の先頭行 → 行が 1 つだけならその行の先頭列 → 無し。
 *
 * 写しである理由: 利用者が画面で見ている宛先と、移植口が受け取る錨を一致させるためである。
 * 順序を変えると**別の位置へ貼り付く**（要件 8.6・8.9 が問題にする「表示の位置と文書の位置」の
 * 取り違えと同じ型の誤りである）。
 */
function anchorOfSelection(selection: GridSelection): CellPosition | null {
  const current = selection.current;
  if (current !== undefined) {
    return { row: current.range.y, column: current.range.x };
  }
  const column = selection.columns.first();
  if (column !== undefined && selection.columns.length === 1) {
    return { row: 0, column };
  }
  const row = selection.rows.first();
  if (row !== undefined && selection.rows.length === 1) {
    return { row, column: 0 };
  }
  return null;
}

/**
 * Glide の選択を移植口の選択（**現在位置と矩形**）へ写す。要件 2.1 の「現在位置を他のセルと
 * 区別して提示する」は、描き手にとっては `current` の有無である。
 *
 * **現在位置は `current.cell` から取る**（矩形の左上ではない）。右下から左上へ引いた選択では
 * 錨が右下にあり、左上へ潰すと利用者が動かしていたセルが別のセルになる。`current` が無い選択
 * （行見出しで行の全体を選んだときなど）に限り矩形の始点を現在位置にする — 画面は「表を描くときは
 * 現在位置が 1 つある」を保つので、`null` を返すのは選択が解除されたときだけである。
 */
function selectionOf(
  selection: GridSelection,
  rowCount: number,
  columnCount: number,
): RendererSelection | null {
  const range = rangeOfSelection(selection, rowCount, columnCount);
  if (range === null) {
    return null;
  }
  const cell = selection.current?.cell;
  return {
    current: cell === undefined ? range.start : { row: cell[1], column: cell[0] },
    range,
  };
}

/**
 * 移植口の選択を Glide の選択へ写す（**下ろす向き**。要件 2.1、2.2、2.3）。
 *
 * 行の全体・列の全体も矩形 + 現在位置として渡すので、Glide は同じ 1 つの形として描く
 * （数えるのは画面の側である — `selection.ts`）。行と列の集合（`rows` / `columns`）は使わない:
 * **矩形 1 つと現在位置 1 つで表せる**ためである。
 */
function glideSelectionFor(selection: RendererSelection | null): GridSelection {
  if (selection === null) {
    return NO_SELECTION;
  }
  const { current, range } = selection;
  return {
    columns: CompactSelection.empty(),
    rows: CompactSelection.empty(),
    current: {
      cell: [current.column, current.row],
      range: {
        x: range.start.column,
        y: range.start.row,
        width: range.end.column - range.start.column + 1,
        height: range.end.row - range.start.row + 1,
      },
      rangeStack: [],
    },
  };
}

/** 2 つの選択が同じかを比べる（知らせは**変化のときだけ**出す。下の `selectionChanged`）。 */
function sameSelection(a: RendererSelection | null, b: RendererSelection | null): boolean {
  if (a === null || b === null) {
    return a === b;
  }
  return (
    a.current.row === b.current.row &&
    a.current.column === b.current.column &&
    a.range.start.row === b.range.start.row &&
    a.range.start.column === b.range.start.column &&
    a.range.end.row === b.range.end.row &&
    a.range.end.column === b.range.end.column
  );
}

/**
 * `RenderCell` を Glide のセルへ写す。**値の意味は変えない**（文字列のまま運ぶ）。
 *
 *   - `loading`: Glide の読み込み中のセルにする。骨組みの棒の幅は列の幅から決める。
 *   - `violated`: 地色の上書きを載せる（落とさない）。
 *   - `variant`: **右寄せの判断にだけ**使う。
 *   - 常に `readonly` であり、Glide 自身の編集器は開かない（編集の起動は外へ報せるだけである）。
 */
function glideCellFor(cell: RenderCell, columnWidth: number): GridCell {
  if (cell.loading) {
    return {
      kind: GridCellKind.Loading,
      allowOverlay: false,
      skeletonWidth: Math.max(
        LOADING_SKELETON_MIN_WIDTH,
        Math.round(columnWidth * LOADING_SKELETON_RATIO),
      ),
      ...(cell.violated ? { themeOverride: VIOLATION_THEME } : {}),
    };
  }
  return {
    kind: GridCellKind.Text,
    allowOverlay: false,
    readonly: true,
    displayData: cell.text,
    // `data` は表示文字列のままである。**数値へ解釈しない**（64 ビット整数が壊れる経路を
    // 作らない。design.md「境界に数値を出さない」）。
    data: cell.text,
    contentAlign: RIGHT_ALIGNED_KINDS[cell.variant] === true ? "right" : "left",
    ...(cell.violated ? { themeOverride: VIOLATION_THEME } : {}),
  };
}

/**
 * 区間の行を全列ぶんのセルへ展開する。**Glide の `damage` はセル単位であり、区間を受けない**
 * ためである。窓は数十行 × 数十列なので、**行数に比例しない**（要件 11.4 と同じ規律:
 * 1 つの操作のためにシート全件を触らない）。
 */
function damageForSpan(span: RowSpan, columnCount: number): readonly { readonly cell: Item }[] {
  const damaged: { cell: Item }[] = [];
  for (let row = span.start; row < span.start + span.count; row += 1) {
    for (let column = 0; column < columnCount; column += 1) {
      damaged.push({ cell: [column, row] });
    }
  }
  return damaged;
}

/**
 * 移植口の実装の DOM を持たない部分を組み立てる（上の [`GlideWiring`]）。**仕様はマウントの
 * たびに 1 つ受け取る**（変化は次の `mount` の仕様に載る。モジュール doc を参照）。
 *
 * **選択だけは仕様の外からも動く**（8.2）: 仕様の [`RendererSpec.selection`] はマウントの時点の
 * 値であり、以後は呼び出し側が [`RendererHandle.setSelection`] で下ろす。下ろされた選択は
 * **外へ報せ返さない**（報せ返すと、画面の状態と実装の状態が往復して食い違いの種になる）。
 */
export function createGlideWiring(spec: RendererSpec): GlideWiring {
  // 列は**見出しと幅だけ**を写す（`RenderColumn` の面そのままである）。
  const columns: readonly GridColumn[] = spec.columns.map(({ title, width }) => ({ title, width }));
  // 幅は骨組みの棒のために引く。**`GridColumn` から読まない** — Glide の `GridColumn` は
  // 幅を持たない変種（自動幅）を含む合併型であり、写しが決めた幅は仕様の側にある。
  const widths: readonly number[] = spec.columns.map(({ width }) => width);
  const surfaceRef: GlideSurfaceRef = { current: null };
  const listeners = new Set<() => void>();

  // マウントの時点の選択は**仕様が持っている**（要件 2.1: 表を描くときは現在位置が 1 つある）。
  let selection: GridSelection = glideSelectionFor(spec.selection);
  // 直近に**外へ報せた**（または画面が最初から知っている）選択。同じ選択の再通知を外へ出さない
  // ために持つ（知らせは変化のときだけである。Glide は同じ選択を何度も通知しうる）。
  let reported: RendererSelection | null = spec.selection;
  let released = false;

  /**
   * 破棄の後に移植口の callback を呼ぼうとしたら投げる。**黙って隠さない** — 画面が消えている
   * のに呼び出し側を叩く実装は、呼び出し側の状態を壊す（7.1 の偽の実装と同じ規律である。
   * `fakeRenderer.ts` の `mountedSpec`）。
   */
  const liveSpec = (operation: string): RendererSpec => {
    if (released) {
      throw new Error(`移植口が破棄された後に ${operation} が起きた`);
    }
    return spec;
  };

  /** 描くものを差し替え、面を描き直させる（React の面が `useSyncExternalStore` で読む）。 */
  const drawSelection = (next: GridSelection): void => {
    selection = next;
    for (const listener of listeners) {
      listener();
    }
  };

  /**
   * **実装が起こした**選択の変化（利用者のポインタ・行見出し・Glide の打鍵の束縛）を外へ報せる。
   * 画面が下ろした選択（[`RendererHandle.setSelection`]）はここを通らない — 通すと往復する。
   */
  const selectionChanged = (next: GridSelection): void => {
    // 破棄の後は知らせの経路を閉じる（同じ選択かどうかを見る前に閉じる — 破棄そのものが
    // 画面の消滅であり、以後の通知はすべて実装の誤りである）。
    liveSpec("onSelectionChange");
    drawSelection(next);
    const observed = selectionOf(next, spec.rowCount, columns.length);
    if (sameSelection(observed, reported)) {
      return;
    }
    reported = observed;
    liveSpec("onSelectionChange").onSelectionChange(observed);
  };

  return {
    props: {
      columns,
      rows: spec.rowCount,
      rowMarkers: spec.rowMarkers,
      keybindings: GLIDE_KEYBINDINGS,
      // **列と行を取り違えない**（Glide の `Item` は `[列, 行]` である）。幅は骨組みの棒の
      // ためだけに使う（列が範囲の外である場合も `RenderCell` の契約どおり読み込み中が返る）。
      getCellContent: (item) =>
        glideCellFor(spec.getCell({ row: item[1], column: item[0] }), widths[item[0]] ?? 0),
      rowHeight: GLIDE_ADAPTER_ROW_HEIGHT,
      headerHeight: GLIDE_ADAPTER_HEADER_HEIGHT,
      onVisibleRegionChanged: (range) => {
        // Glide の `Rectangle` は**データの座標である**（行見出しのぶんは Glide が内部で補正
        // する）。行と列の区間としてそのまま移植口へ渡す（要件 2.4）。
        liveSpec("onVisibleSpanChange").onVisibleSpanChange({
          rows: { start: range.y, count: range.height },
          columns: { start: range.x, count: range.width },
        });
      },
      onCellActivated: (item) => {
        liveSpec("onActivateEditor").onActivateEditor({ row: item[1], column: item[0] });
      },
      onColumnResize: (_column, newSize, colIndex) => {
        // **位置は添字から取る**（画面の表示順は添字が表す）。幅は `newSize` を使う
        // （`newSizeWithGrow` は Glide の伸長ぶんを含む別の量である）。
        liveSpec("onColumnResize").onColumnResize(colIndex, newSize);
      },
      onColumnMoved: (startIndex, endIndex) => {
        liveSpec("onColumnMove").onColumnMove(startIndex, endIndex);
      },
      onGridSelectionChange: selectionChanged,
    },
    get selection(): GridSelection {
      return selection;
    },
    subscribeSelection(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    selectionChanged,
    currentRange: () => rangeOfSelection(selection, spec.rowCount, columns.length),
    currentAnchor: () => anchorOfSelection(selection),
    copyRange: async (range) => liveSpec("onCopy").onCopy(range),
    pasteAt: async (anchor, text) => {
      await liveSpec("onPaste").onPaste(anchor, text);
    },
    surfaceRef,
    handle: {
      setSelection(next) {
        liveSpec("setSelection");
        drawSelection(glideSelectionFor(next));
        // **下ろした値も「実装が最後に知っている選択」として覚える。**覚えないと、利用者が
        // その値と一致する位置をポインタで指したとき `selectionChanged` の同一判定で飲み込まれ、
        // **描かれている選択と画面の写しが食い違う**（8.2 のレビューが実測: 画面が (0,1) を
        // 下ろした後に利用者が (0,0) をクリックすると、外へ報せず写しは (0,1) のまま残る）。
        // **報せ返さない**こと自体は変わらない（画面は自分が決めた選択を知っている）。
        reported = next;
      },
      scrollTo(position) {
        // Glide の `scrollTo` は `(列, 行, 方向)` である。**列と行を取り違えない。**
        // 両軸を動かす（移植口の契約は「その位置が見えるところまで」であり、軸を選ばない）。
        surfaceRef.current?.scrollTo(position.column, position.row, "both");
      },
      invalidate(span) {
        const surface = surfaceRef.current;
        if (surface === null) {
          // まだ部品が繋がっていない（React の描画はマウントの直後から始まる）。**黙って何もしない**
          // — Glide は要求を溜める口を持たず、呼び出し側は描画の開始前に描き直しを要求しない。
          return;
        }
        surface.updateCells(damageForSpan(span, columns.length));
      },
      destroy() {
        released = true;
        // 破棄の後に知らせを出し続けない（購読者も切る）。**部品の参照は React が外す**
        // （`DataEditor` の unmount で `ref.current` が `null` になる）。
        listeners.clear();
        selection = NO_SELECTION;
        reported = null;
      },
    },
  };
}

/** 部品を包む面。**器いっぱいに広げる**（移植口は寸法を持たない。寸法は器が決める）。 */
const SURFACE_STYLE: CSSProperties = { width: "100%", height: "100%" };

/**
 * クリップボードへ文字列を渡す。**移植口の外の失敗であり、描画を止めない**（例外は呼び出し元が
 * 記録する）。
 *
 * 予備の経路（`document.execCommand("copy")`）を残してあるのは、`navigator.clipboard` が
 * **安全な文脈でしか現れない**ためである（現れない環境では複製が黙って無反応になる。
 * 要件 7.1 は「他のアプリケーションへ渡せる」ことなので、無反応は要件を外す）。
 */
async function writeClipboardText(text: string): Promise<void> {
  const clipboard: Clipboard | undefined = navigator.clipboard;
  if (clipboard !== undefined && typeof clipboard.writeText === "function") {
    await clipboard.writeText(text);
    return;
  }
  const area = document.createElement("textarea");
  area.value = text;
  // 見えないが選択できる必要がある（`execCommand("copy")` は選択範囲を写す）。
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.top = "-1000px";
  area.style.opacity = "0";
  document.body.append(area);
  try {
    area.select();
    if (!document.execCommand("copy")) {
      throw new Error("execCommand(\"copy\") が false を返した");
    }
  } finally {
    area.remove();
  }
}

/**
 * 選択の範囲を複製してクリップボードへ渡す。**文字列を作るのは呼び出し側である**
 * （`RendererSpec.onCopy`。Rust 側の `PasteCodec` が表形式を組み立てる）。
 *
 * 選択が無いときは移植口を呼ばない（複製する範囲が無い）。
 */
async function copySelection(wiring: GlideWiring): Promise<void> {
  const range = wiring.currentRange();
  if (range === null) {
    return;
  }
  await writeClipboardText(await wiring.copyRange(range));
}

/**
 * `DataEditor` を描く面。**移植口の意味論を持たない**（それは [`createGlideWiring`] の側である）。
 * ここが持つのは DOM にしか無いものだけである:
 *
 *   - `DataEditor` を器の中に描くこと、
 *   - DOM の `copy` / `paste` を受けて配線へ渡すこと、
 *   - クリップボードへ書くこと、
 *   - 選択（配線が持つ）を購読して制御選択の props へ渡すこと。
 *
 * `copy` / `paste` は**捕獲の段で受ける**（`addEventListener(..., true)`）。Glide 自身の
 * 聴取は `window` のバブルの段にあるので、捕獲の段の方が先に走る — 止め損ねると 2 本の経路が
 * 同じ操作を処理することになる（`keybindings` で止めてあるが、これはその二重の錠前である）。
 */
function GlideSurface({ wiring }: { readonly wiring: GlideWiring }): ReactElement {
  const containerRef = useRef<HTMLDivElement | null>(null);
  // 選択は配線が持つ。**写しを React の状態として持たない**（所有者を 1 つにする）。
  const selection = useSyncExternalStore(wiring.subscribeSelection, () => wiring.selection);

  const onCopy = useCallback(
    (event: Event) => {
      event.preventDefault();
      void copySelection(wiring).catch((error: unknown) => {
        console.error("選択の範囲をクリップボードへ渡せなかった", error);
      });
    },
    [wiring],
  );

  const onPaste = useCallback(
    (event: Event) => {
      // **文字列を解釈しない。**クリップボードの生の表形式をそのまま移植口へ渡す
      // （何行何列かも、どの列の型に掛けるかも移植口は知らない）。
      const text =
        event instanceof ClipboardEvent ? (event.clipboardData?.getData("text/plain") ?? "") : "";
      const anchor = wiring.currentAnchor();
      if (anchor === null) {
        return;
      }
      event.preventDefault();
      void wiring.pasteAt(anchor, text).catch((error: unknown) => {
        console.error("貼り付けを移植口へ渡せなかった", error);
      });
    },
    [wiring],
  );

  useEffect(() => {
    const node = containerRef.current;
    if (node === null) {
      return;
    }
    node.addEventListener("copy", onCopy, true);
    node.addEventListener("paste", onPaste, true);
    return () => {
      node.removeEventListener("copy", onCopy, true);
      node.removeEventListener("paste", onPaste, true);
    };
  }, [onCopy, onPaste]);

  return (
    <div ref={containerRef} style={SURFACE_STYLE}>
      <DataEditor
        {...wiring.props}
        ref={wiring.surfaceRef}
        // 制御選択である（`onGridSelectionChange` を渡すと制御になる）。選択の持ち主は配線である。
        gridSelection={selection}
        width="100%"
        height="100%"
      />
    </div>
  );
}

/**
 * 移植口の実装。**器の中へ描き始める唯一の口**である（`GridRendererPort.mount`）。
 *
 * 器は実装が自分の描画面をぶら下げる場所である（移植口は器の中身を読み書きしない）。
 * **器には確定した寸法が要る** — 移植口は寸法を運ばないので、器の大きさがそのまま
 * グリッドの大きさになる（`height: 100%` は親の確定した高さを要する）。
 */
export function createGlideAdapter(): GridRendererPort {
  return {
    mount(container, spec) {
      const wiring = createGlideWiring(spec);
      const root = createRoot(container);
      root.render(<GlideSurface wiring={wiring} />);
      return {
        // 写しは配線のものをそのまま使う（意味論は配線の側にある）。
        setSelection: (selection) => {
          wiring.handle.setSelection(selection);
        },
        scrollTo: (position) => {
          wiring.handle.scrollTo(position);
        },
        invalidate: (span) => {
          wiring.handle.invalidate(span);
        },
        destroy: () => {
          // 先に配線を破棄する（以後の知らせは投げる）。そのうえで React の根を片付ける。
          wiring.handle.destroy();
          root.unmount();
        },
      };
    },
  };
}

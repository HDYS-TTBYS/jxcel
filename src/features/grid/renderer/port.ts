/**
 * 描画層の移植口（tasks.md 7.1 / 8.2。design.md「RendererPort と GlideAdapter」）。
 *
 * 所有: `RendererPort`（同「Components and Interfaces」の表）。要件: 1.1, 1.2, 1.3, 1.4,
 * 2.1, 2.2, 2.3, 2.4, 7.1, 7.2。**本ファイルはタスク 7.1 が定義し、8.2 が 4 点だけ広げた**
 * （design.md「8.2 が広げた面（移植口の改訂）」）。
 *
 * # 8.2 が広げた 4 点（**7.1 の契約を変えた唯一の箇所である**）
 *
 * | 足したもの | 向き | 何のためか |
 * |---|---|---|
 * | [`RendererSpec.selection`] | 画面 → 実装 | マウントの時点で現在位置が 1 つあること（要件 2.1） |
 * | [`RendererHandle.setSelection`] | 画面 → 実装 | 打鍵のたびに選択を下ろす（要件 2.2、2.3、2.4。マウントし直さない） |
 * | [`RendererSpec.onVisibleSpanChange`] | 実装 → 画面 | 追随の判断（要件 2.4）と、窓の先読み（7.3） |
 * | [`RendererSpec.rowMarkers`] | 画面 → 実装 | 行の全体の選択のポインタの操作（要件 2.3） |
 *
 * 加えて [`RendererSpec.onSelectionChange`] の引数を矩形から [`RendererSelection`]（現在位置と
 * 矩形）へ広げた — **現在位置は矩形の左上とは限らない**（右下から左上へ引いた選択では錨が右下に
 * ある）ためである。7.1 は「選択は実装が持ち、外へ報せる一方通行」と定めていたが、8.2 は
 * **画面が持ち、実装は指示されたとおりに描く**向きへ変えた（理由と、食い違いが起きない仕組みは
 * design.md の同節）。
 *
 * # 何を運び、何を運ばないか
 *
 * 運ぶのは**描画・当たり判定・文字計測・クリップボードの配管だけ**である。編集の意味論・
 * 判定（適合するか）・履歴（取り消しとやり直し）は**知らない**。したがって本 module は
 * それらの型・関数・定数を取り込まないし、宣言もしない。ここを通る値は次の 3 種類だけである。
 *
 *   1. **座標** — 表示の位置（[`CellPosition`] / [`CellRange`] / [`RowSpan`]）
 *   2. **表示の内容** — 文字列と、型の札と、違反の印と、読み込み中の印（[`RenderCell`]）
 *   3. **列の見出し** — 見出しの文字列と幅（[`RenderColumn`]）
 *
 * `violated` は**判定ではない**。窓の二進形式（design.md「窓の二進形式」）がセルごとに
 * **違反の有無を 1 バイト**で運んでおり、それをそのまま札として描き手へ渡すだけである。
 * 「なぜ違反なのか」「どう直すのか」を決めるのは `schema-engine` と Rust 側の適用層であり、
 * 理由の文字列は本移植口を経由しない。**移植口は判定を下さず、解釈もしない。**
 *
 * # 2 つの空間を混ぜない（表示の序数と文書の位置）
 *
 * [`CellPosition`] の行は**可視行の序数**である（並べ替えと絞り込みを適用した**あとの**並びで
 * 何番目か）。文書の行（`RowId`）ではない。両者は絞り込みの下では一致せず、取り違えると
 * **別の行を編集する**（要件 8.6）。文書の位置を運ぶ型は本 module に無い — 画面が操作の宛先を
 * 決めるときだけ生成物の `GridCellAddress`（`row` は `RowId` の文字列表現）を使い、表示の序数から
 * そちらへの写像は行の順序（`view` 層の `RowOrder`）を通す。**本 module にその写像は置かない**
 * （`crates/data-grid/src/types/mod.rs` が同じ規律を持っている）。
 *
 * # セルの取得の不変条件（本移植口で最も重要な契約）
 *
 * `RendererSpec.getCell` は**同期であり、例外を投げない**。未取得の行は `loading: true` を返す
 * （design.md の不変条件）。描き手は描画の途中でセルを引くため、`Promise` を返す口にすると
 * 描画の引きに載せられず、例外を投げる口にすると**1 つの未取得のセルが画面全体の描画を落とす**。
 * 空白で答えるのも間違いである — 空白は「値なし」と区別がつかない（要件 1.4、タスク 7.3）。
 *
 * # 型の札は生成物から取り込む
 *
 * `TypeKindTag` は**境界用の型の生成物**（`src/ipc/bindings.ts`。`cargo run -p app-shell --bin
 * generate-bindings` が作る）から取り込む。**本ファイルで定義し直さない** — 写しを 2 つ持つと、
 * `schema-engine` の `TypeKind` に変種が 1 つ増えたときに片方だけが古くなり、気づけない
 * （tasks.md の Implementation Notes「境界の型は `app-shell` が単一の源」。design.md
 *「EditorRegistry」の Risks も同じ理由を書いている）。取り込みは**型だけ**（`import type`）なので
 * 実行時には消える — 本 module は実行時の値を 1 つも持たない（`port.test.ts` が機械検査する）。
 *
 * # 画面そのものはここではない
 *
 * 画面（`GridScreen`）は 8.1 が作る。本 module が持つのは**面だけ**であり、実装（7.2 の
 * Glide の写し）も窓の記憶（7.3）も含まない。呼び出しの並びが実装に依存しないことは
 * `port.test.ts` が 2 つの偽の実装で固定している。
 */
// **型だけの取り込みである**（`verbatimModuleSyntax` により `import type` が要る）。
// 型はバンドル時に消えるので、生成物への依存が実行時の依存になることはない。
import type { TypeKindTag } from "../../../ipc/bindings";

/**
 * 可視行の序数（0 起点）。並べ替えと絞り込みを適用した**あとの**並びで、行が何番目か。
 *
 * **これは文書の行の位置ではない。**同じ序数が、絞り込みの指定ひとつで別の行を指す
 * （`crates/data-grid/src/types/mod.rs` の `RowOrdinal` と同じ意味である）。
 *
 * 別名（`number`）であって新しい型ではない。**型検査では混同を防げない** — 防いでいるのは
 * 名前である（`row` と `column`、`start` と `count`）。番号に札を付ける（branded type）と、
 * 8.x が表示状態や行の順序から受け取った値をそのつど変換することになり、`onColumnResize` /
 * `onColumnMove`（design.md が素の `number` で書いている）と形が食い違う。
 */
export type RowOrdinal = number;

/** 列の添字（0 起点）。上の [`RowOrdinal`] と同じ理由で別名に留める。 */
export type ColumnIndex = number;

/**
 * 表示の空間のセルの位置: 可視行の序数と列の添字。
 *
 * 選択と現在位置は**画面に見えている位置**で指定される（要件 2.1、2.3）ため、こちらは序数を
 * 持つ。列に序数が別に無いのは、並べ替えと絞り込みが**行だけを並べ替え、列の集合と並び順は
 * スキーマが供給する**ためである（列の表示順の変更は窓の中身を変えない）。
 *
 * 2 つの成分を名指しする（組や並びにしない）。位置を `[3, 1]` と書くと読み手に行と列の
 * 区別が伝わらない — 生成物が `GridCellEdit` で位置と文字を名前のある欄に分けているのと同じ
 * 判断である（`src/ipc/bindings.ts`）。
 */
export interface CellPosition {
  readonly row: RowOrdinal;
  readonly column: ColumnIndex;
}

/**
 * 選択された矩形の範囲。**両端を含む**（`start` と `end` の両方のセルが範囲に入る）。
 *
 * 角は表示の位置（[`CellPosition`]）であり、行の全体も列の全体もこの形で表せる — 行の全体は
 * 列 0 から最終列までの矩形、列の全体は行 0 から最終行までの矩形である（要件 2.3 の 3 つの
 * 選択は、行数と列数を数える側（画面）が同じ形から導く）。**正規化（左上と右下へ揃える）は
 * 範囲を作る側の責任である** — 描き手は `start <= end` を両軸で前提にしてよい（Rust 側の
 * `CellRange::new` が軸ごとに正規化しており、写す側で同じ前提を保つ）。
 */
export interface CellRange {
  readonly start: CellPosition;
  readonly end: CellPosition;
}

/**
 * 画面が持つ選択: **現在位置（現在のセル）と、選択されている矩形**。
 *
 * 要件 2.1 が求める「現在位置となるセルを 1 つ持ち、区別できる形で提示する」は、描き手にとっては
 * この 2 つの組である — Glide の制御選択も同じ組（`current` と矩形）を持ち、現在位置を矩形の
 * 枠とは別の印（焦点の環）で描く。**矩形 1 つでは現在位置を表せない**: 右下から左上へ引いた選択の
 * 錨は右下にあり、正規化した矩形の左上とは別のセルである。
 *
 * **不変条件: `current` はつねに `range` の中にある。**選択を作る側（`selection.ts`）が保つ。
 * 行の全体・列の全体の選択でも現在位置は 1 つである（利用者が居た位置をそのまま残す）。
 */
export interface RendererSelection {
  readonly current: CellPosition;
  readonly range: CellRange;
}

/**
 * 行見出し列の出し方。**画面が決める**（実装は与えられたとおりに描く）。
 *
 *   - `none`: 行見出しを描かない（行をポインタで選ぶ操作が無い）。
 *   - `clickable-number`: 行の番号を描き、**クリックでその行の全体を選べる**ようにする
 *     （要件 2.3 の 2 つ目）。番号は 1 起点である（Glide の行見出しの既定）。
 *
 * Glide の `number` は**クリックできない**（`rowMarkers === "number"` のとき行見出しの操作は
 * 何もしない — 7.2 の実装を読んで確認した）。使える値だけを並べる。
 */
export type RowMarkerMode = "none" | "clickable-number";

/**
 * 可視行の区間: 連続する可視行の並び。窓の要求と、影響を受けた行の通知に使う。
 *
 * **半開区間である**: `start` を含み、`start + count` を含まない（可視 1 行目が `0` であって
 * 長さが `0` の区間も矛盾なく表せ、隣り合う区間が重ならない）。座標は**可視行の序数**であって
 * 文書の行の位置ではない（design.md の Implementation Notes「`RowSpan` は可視行の序数で表す」。
 * `crates/data-grid/src/types/mod.rs` の `RowSpan` と同じ規約である）。
 */
export interface RowSpan {
  readonly start: RowOrdinal;
  readonly count: number;
}

/** 可視列の区間: [`ColumnSpan`] の列版であり、同じ半開区間の規約である。 */
export interface ColumnSpan {
  readonly start: ColumnIndex;
  readonly count: number;
}

/**
 * 表示範囲に見えている区間（**行と列の両方**）。要件 2.4 の追随の判断がこれで決まる。
 *
 * 移植口は画面へ「どこが見えているか」を知らせる口を持たなかった（7.1 は開いたままにした）。
 * 8.2 が [`RendererSpec.onVisibleSpanChange`] として足した — 追随（現在位置がこの区間の外へ
 * 出たら `scrollTo` する）と、窓の先読み（7.3 の `setVisibleSpan`）がどちらもこれを使う。
 *
 * **列も要る。**行だけでは、右へ動いて見えなくなった現在位置に追随できない（追随の判断が
 * 片軸だけになる）。画面は行の区間を窓の記憶へ渡し、両軸を追随の判断に使う。
 */
export interface VisibleSpan {
  readonly rows: RowSpan;
  readonly columns: ColumnSpan;
}

/**
 * 描き手へ渡す列。**描画に要るものだけ**を持つ。
 *
 * 生成物の `ColumnDescriptor`（`src/ipc/bindings.ts`）は 6.1 の境界型であり、位置・内側の経路・
 * 表示名・葉の型の札・要素数の能力・展開の可否を持つ。**本移植口はその全部を要らない** —
 * 見出しを描き、列幅を反映するだけである。よって写さずに絞る。
 *
 *   - `title`: 見出しに描く文字列（`ColumnDescriptor.name` の写し。位置に沿ったフィールド名を
 *     `.` で連結したもの）。
 *   - `width`: 描く幅（ピクセル）。**要件 8.1 がこれを要求している** — 列幅を変更できるという
 *     ことは、変更された幅で描けるということである。`RendererSpec.onColumnResize` は外向きの
 *     知らせであり、移植口自身は幅を変えない。変わった幅は次に渡される `columns` に載る
 *     （列を描くのに幅の入力が他に無い）。
 *
 * **型の札（`kind`）は載せない。**描き手は見出しの描画に型を要さず、入力手段の選択（要件 3.1、
 * 10.1）は `EditorRegistry`（7.4）が列の位置から行う。ここに載せると、移植口が「どの型にどの
 * 入力を割り当てるか」を知る経路が生まれる（本移植口が知らないと決めたことである）。
 *
 * **列の並びは表示順である。**`RendererSpec.columns` の順がそのまま画面の左からの順であり、
 * `onColumnMove(from, to)` はこの並びの位置で報告する（design.md「表示状態」の `columnOrder` は
 * 画面が持ち、描き手は与えられた順に描くだけである）。
 */
export interface RenderColumn {
  readonly title: string;
  readonly width: number;
}

/**
 * 1 つのセルを描くのに要るもの。**値そのものではない**（数値としての `Int` や `Decimal` は
 * 表示文字列として運ばれる。design.md「窓の二進形式」）。
 */
export interface RenderCell {
  /** 表示する文字列。未取得・値なしは空文字である（区別は `loading` が担う）。 */
  readonly text: string;
  /** 葉の型の札（生成物の `TypeKindTag`）。列が範囲の外である場合も何か 1 つを返す（下記）。 */
  readonly variant: TypeKindTag;
  /**
   * 窓が運んできた違反の印（`schema-engine` の判定の結果であり、移植口は作らない）。
   */
  readonly violated: boolean;
  /**
   * まだ取得していない行であることの印。**空白文字列で代用しない** — 空白は「値なし」と
   * 区別がつかない（要件 1.4、タスク 7.3 の「読み込み中として描く」）。
   */
  readonly loading: boolean;
}

/**
 * 移植口へ渡す仕様。**描き手が引く口（`getCell`）と、描き手が出す知らせ（`on*`）だけ**である。
 *
 * `on*` は**知らせであって命令ではない**。移植口は受け取ったことを覚えないし、自分の状態を
 * 変えない。何をどう変えるか（選択の確定、編集の起動、列幅の記憶、列順の記憶、複製の内容、
 * 貼り付けの解釈）は呼び出し側（画面。8.x）の仕事である。
 *
 * **選択と現在位置だけは、呼び出し側が持ち、仕様と [`RendererHandle.setSelection`] で下ろす**
 * （8.2。上の module doc）。列幅・列順のように「次の `mount` の仕様でしか反映できない」ものでは
 * ない — 選択は打鍵のたびに変わるので、そのたびにマウントし直すわけにいかない。
 */
export interface RendererSpec {
  /** 左から順に描く列。表示順であり、幅を含む。 */
  readonly columns: readonly RenderColumn[];
  /** 可視行の総数（絞り込みを適用したあとの数）。 */
  readonly rowCount: number;
  /**
   * マウントの時点の選択。**現在位置が 1 つも無い状態で描き始めない**（要件 2.1）。
   *
   * `null` は「選択が無い」であり、許されるのは表を描かない場合だけである（画面は表を描くとき
   * つねに現在位置を 1 つ渡す）。以後の変化は [`RendererHandle.setSelection`] で下ろす。
   */
  readonly selection: RendererSelection | null;
  /** 行見出し列の出し方。**ポインタで行の全体を選ぶ操作の有無を決める**（要件 2.3）。 */
  readonly rowMarkers: RowMarkerMode;
  /**
   * セルを引く。**同期であり、例外を投げない**（本移植口の不変条件）。未取得の行は
   * `loading: true` を返す。範囲の外の位置（負・行数以上・列数以上・整数でない）も
   * 例外を投げてはならない — 描き手は描画の途中で引くため、1 つの例外が画面全体を落とす。
   */
  readonly getCell: (position: CellPosition) => RenderCell;
  /**
   * **実装が起こした**選択の変化（利用者がポインタで選んだ、行見出しを押した、Glide 自身の
   * 打鍵の束縛が動かした）。解除は `null`。要件 2.1、2.3。
   *
   * 呼び出し側が `setSelection` で下ろした選択は報せない（**自分が決めたことを自分へ報せない**。
   * 報せると、画面の状態と実装の状態が往復して食い違いの種になる）。したがってこの知らせは
   * つねに「画面が知らない変化」であり、画面はそのまま自分の選択として取り込む。
   */
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
  /**
   * 見えている区間が変わった（走査・マウント・追随の後）。要件 2.4 の追随の判断と、7.3 の窓の
   * 先読みに使う。**これは知らせであって命令ではない** — 実装は画面の状態を変えない。
   */
  readonly onVisibleSpanChange: (span: VisibleSpan) => void;
  /** 編集の起動が指示された（打鍵または二度打ち）。**編集の中身は運ばない**。要件 2.2、3.x。 */
  readonly onActivateEditor: (position: CellPosition) => void;
  /** 列の幅が変更された（変更後の幅。ピクセル）。要件 8.1。 */
  readonly onColumnResize: (column: number, width: number) => void;
  /** 列の位置が変更された（表示順の位置どうし）。要件 8.2。 */
  readonly onColumnMove: (from: number, to: number) => void;
  /**
   * 選択の範囲の複製が指示された。**表形式のテキストを返す**（行の区切りと列の区切りを持つ。
   * 要件 7.1、7.2）。作るのは呼び出し側であり、移植口は受け取った文字列をクリップボードへ
   * 渡すだけである。
   */
  readonly onCopy: (range: CellRange) => Promise<string>;
  /**
   * 表形式のテキストが貼り付けられた。**中身を解釈しない** — 何行何列かも、どの列の型に
   * 掛けるかも、移植口は知らない（解釈は Rust 側の `PasteCodec` と適用層の仕事である）。
   */
  readonly onPaste: (anchor: CellPosition, text: string) => Promise<void>;
}

/**
 * 描き終えた移植口を扱う口。**呼び出し側（画面）が使う。**
 *
 * 表示状態（列幅・列順・選択）を**移植口から書き換える口は無い** — 移植口は画面の状態を
 * 勝手に変えられない。状態が変われば、次の `mount` に渡す仕様が変わる。**唯一の例外が選択で
 * ある**（8.2）: 選択は打鍵のたびに変わるので、次の `mount` を待っていられない
 * （マウントし直すと React の根と Glide の部品を作り直すことになり、走査の位置も失われる）。
 */
export interface RendererHandle {
  /**
   * 選択を下ろす（解除は `null`）。要件 2.1、2.2、2.3。
   *
   * **これが選択の唯一の更新の口である**（マウントの時点の値は [`RendererSpec.selection`]）。
   * 実装は与えられた組をそのまま描き、**外へ報せ返さない**（自分が決めたことを自分へ報せない。
   * `RendererSpec.onSelectionChange` の docs）。
   */
  readonly setSelection: (selection: RendererSelection | null) => void;
  /** その位置が見えるところまで表示範囲を動かす（要件 2.4 の追従、9.8 の移動）。 */
  readonly scrollTo: (position: CellPosition) => void;

  /** その区間を描き直させる（窓の内容が変わったとき。要件 1.7）。 */
  readonly invalidate: (span: RowSpan) => void;
  /**
   * **選択の範囲を複製する**（要件 7.1、7.2、7.8）。表形式のテキストを作るのは呼び出し側
   * （[`RendererSpec.onCopy`]）であり、この口は範囲を決め（**移植口が持っている選択**から）
   * クリップボードへ渡すところまでを担う。
   *
   * **打鍵（DOM の `copy`）とメニューの活性化の唯一の入口である。**範囲を引数に取らないのは、
   * 取ると呼ぶ側が範囲を計算することになり、**範囲の決定が 2 箇所へ分かれる**ためである
   * （打鍵は移植口の選択、メニューは画面の写し、という 2 つになる）。移植口は選択を両方向に
   * 同期している（[`RendererSpec.selection`] / [`RendererHandle.setSelection`] /
   * `onSelectionChange`）ので、決めるのは移植口 1 つに閉じられる。
   *
   * 選択が無いときは**何もしない**（`onCopy` を呼ばない）。複製できないとき（選択の範囲に
   * 窓が届いていないセルがある）は [`RendererSpec.onCopy`] の契約どおり失敗し、その理由は
   * 画面の告知へ出る（**空文字を渡さない** — クリップボードが空になり、利用者には
   * 「複製できた」と見える）。
   */
  readonly copySelection: () => Promise<void>;
  /** 描き手を片付ける。**以後その `RendererHandle` を使ってはならない。** */
  readonly destroy: () => void;
}

/**
 * 描画層の移植口。**面はこれだけである**（実装は 7.2、偽の実装と駆動器は
 * `fakeRenderer.ts` / `interactionDriver.ts`）。
 *
 * 実装を差し替えても呼び出しの並びが変わらないことを、`port.test.ts` が 2 つの偽の実装で
 * 固定している。**画面そのもの（8.1）はこの面の向こう側の話ではない** — 画面はこの面を
 * 使う側である。
 */
export interface GridRendererPort {
  /**
   * 器の中へ描き始める。器（`HTMLElement`）は実装が自分の描画面をぶら下げる場所であり、
   * **移植口は器の中身を読み書きしない**（テストの偽の実装は器に触れない。`interactionDriver.ts`
   * の `standInContainer` を参照）。
   */
  mount(container: HTMLElement, spec: RendererSpec): RendererHandle;
}

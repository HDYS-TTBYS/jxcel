/**
 * 描画層の移植口の契約（tasks.md 7.1。要件 2.1, 2.2, 2.3）。
 *
 * 固定するのは 4 つである。
 *
 * 1. **セルの取得**（`RendererSpec.getCell`）: 同期であり例外を投げない。未取得の行は
 *    `loading: true` で返る（design.md「RendererPort と GlideAdapter」の不変条件）。
 * 2. **外向きの知らせ**: 選択の変化・編集の起動・列幅の変更・列の移動・複製・貼り付けの
 *    6 つが、移植口を通って外へ出る（タスクの文言の「移植口の呼び出しとして外へ出す」）。
 *    呼び出し側が使う 3 つの口（`scrollTo` / `invalidate` / `destroy`）も同じ並びに載る。
 * 3. **実装を差し替えても呼び出しの並びが変わらないこと**: 内部の作りが違う 2 つの偽の実装と、
 *    行の出所が違う 2 つの模型を掛け合わせた 4 通りの駆動で、記録された並びが一致する。
 * 4. **移植口が編集の意味論・判定・履歴を知らないこと**: 面の型（`npm run typecheck` が
 *    効き手）と、実行時の輸出（値が 1 つも無いこと）の両方で固定する。
 *
 * # 期待する並びは駆動器の定数から導かない
 *
 * `CANONICAL_SEQUENCE` は下に**逐語で**書いてある。駆動器（`interactionDriver.ts`）の定数を
 * 読み込んで組み立てると、駆動器ごと書き換えたときに期待値も一緒に動き、並びの変化に気づけない。
 * ここは「何がどの順に外へ出るか」を固定する唯一の場所である。
 *
 * # 7.2（Glide の実装）への申し送り
 *
 * 駆動器は移植口に加えて「利用者の操作を注ぐ面」（`RendererEventSource`）を要求する — 移植口
 * そのものには注ぐ口が無い（`GridRendererPort` が持つのは `mount` だけで、外向きの知らせは
 * `RendererSpec` の callback である）。7.2 は Glide の通知（選択・セルの起動・列幅・列の移動・
 * 複製・貼り付け）に同じ面をかぶせ、`driveCanonicalSequence` をそのまま通して
 * `CANONICAL_SEQUENCE` と突き合わせること。**実物の実装がこの並びを保つことが、移植口を
 * 挟んだ理由（上流が止まっても差し替えられる）の実測になる。**
 */
import { describe, expect, it } from "vitest";

import { createEagerFakeRenderer, createLazyFakeRenderer } from "./fakeRenderer";
import {
  DRIVE_COLUMNS,
  DRIVE_ROW_COUNT,
  arrayRowSource,
  createRendererSpec,
  driveCanonicalSequence,
  lazyRowSource,
} from "./interactionDriver";
import type { DrivableRenderer, DrivenRun, PortCall, RecordedCall, RowSource } from "./interactionDriver";
// **値としての取り込みである**（型だけの取り込みと分けて書く。`src/ipc/client.ts` と同じ書き方）。
// 移植口の module が実行時に何も輸出しないことを、読み込んだ名前空間そのもので確かめるために要る。
import * as portModule from "./port";
import type {
  CellPosition,
  CellRange,
  GridRendererPort,
  RenderCell,
  RenderColumn,
  RendererHandle,
  RendererSpec,
  RowSpan,
} from "./port";

/**
 * 決められた順に外へ出る呼び出しの並び。**駆動器の定数を使わずに書く**（上のヘッダを参照）。
 * `mount` は呼び出し側が移植口へ入る唯一の口である。`getCell` は載らない — セルをいつ何回
 * 引くかは実装の作りであり、並びの比較に持ち込むと「実装に依存しない」という主張が崩れる
 * （駆動器の `driveCanonicalSequence` の docs を参照）。
 */
const CANONICAL_SEQUENCE: readonly RecordedCall[] = [
  { call: "mount", args: [] },
  { call: "onSelectionChange", args: [{ start: { row: 1, column: 0 }, end: { row: 3, column: 1 } }] },
  { call: "onActivateEditor", args: [{ row: 2, column: 1 }] },
  { call: "onColumnResize", args: [1, 144] },
  { call: "onColumnMove", args: [2, 0] },
  { call: "onCopy", args: [{ start: { row: 1, column: 0 }, end: { row: 2, column: 1 } }] },
  { call: "onPaste", args: [{ row: 4, column: 0 }, "1:0\t1:1\n2:0\t2:1"] },
  { call: "scrollTo", args: [{ row: 40, column: 2 }] },
  { call: "invalidate", args: [{ start: 8, count: 4 }] },
  { call: "destroy", args: [] },
];

/** 六つの知らせ（タスクの文言が並べているもの）。 */
const CALLBACKS: readonly PortCall[] = [
  "onSelectionChange",
  "onActivateEditor",
  "onColumnResize",
  "onColumnMove",
  "onCopy",
  "onPaste",
];

describe("セルの取得の契約（同期・例外を投げない・未取得は読み込み中）", () => {
  it("同期であり、範囲の外でも例外を投げず、読み込み中の札で答える", () => {
    const spec = createRendererSpec({ source: lazyRowSource(2) });

    const outOfRange: readonly CellPosition[] = [
      { row: -1, column: 0 },
      { row: DRIVE_ROW_COUNT, column: 0 },
      { row: 1.5, column: 0 },
    ];
    const expectedInColumn: RenderCell = {
      text: "",
      variant: "Text",
      violated: false,
      loading: true,
    };

    for (const position of outOfRange) {
      // 同期である: 返るのは `Promise` ではない（`await` を要する値は描画の引きに載せられない）。
      expect(spec.getCell(position)).not.toBeInstanceOf(Promise);
      // 例外を投げない: ここまで来ていること自体が証拠である。返る札も固定する。
      expect(spec.getCell(position)).toEqual(expectedInColumn);
    }

    // 列が範囲の外である場合、列の札も決まらない（生成物の `ColumnDescriptor.kind` が `null` に
    // なりうるのと同じ状況である）。既定の入力へ落ちる札（`Any`）を使い、値は読み込み中のままにする。
    const expectedNoColumn: RenderCell = {
      text: "",
      variant: "Any",
      violated: false,
      loading: true,
    };
    expect(spec.getCell({ row: 0, column: -1 })).toEqual(expectedNoColumn);
    expect(spec.getCell({ row: 0, column: DRIVE_COLUMNS.length })).toEqual(expectedNoColumn);
  });

  it("未取得の行は読み込み中として返り、取得の到着で値へ変わる", () => {
    const source = lazyRowSource(2);
    const spec = createRendererSpec({ source });

    const fetched: RenderCell = { text: "2:0", variant: "Text", violated: false, loading: false };
    const notFetched: RenderCell = { text: "", variant: "Text", violated: false, loading: true };

    // 取得済み（0..2 行）。
    expect(spec.getCell({ row: 2, column: 0 })).toEqual(fetched);
    // 未取得（3 行目）。**空白ではなく読み込み中である** — 空白は「値なし」と区別がつかない。
    expect(spec.getCell({ row: 3, column: 0 })).toEqual(notFetched);

    // 窓の到着の模型。取得済みの区間が広がると、同じ位置が値になる（3 行目は標本の違反の行でもある）。
    source.fetch({ start: 3, count: 1 });
    expect(spec.getCell({ row: 3, column: 0 })).toEqual({
      text: "3:0",
      variant: "Text",
      violated: true,
      loading: false,
    });
  });

  it("違反の印は窓が運んでくる札であり、そのまま描き手へ渡る", () => {
    // 標本の違反は 7 行ごとの 3 行目である（窓の二進形式は違反の有無を 1 バイトで運ぶ。
    // design.md「窓の二進形式」）。移植口はこの札を**作らず、解釈もしない**。
    const spec = createRendererSpec({ source: lazyRowSource() });
    const violated: RenderCell = { text: "3:0", variant: "Text", violated: true, loading: false };
    const clean: RenderCell = { text: "4:0", variant: "Text", violated: false, loading: false };
    expect(spec.getCell({ row: 3, column: 0 })).toEqual(violated);
    expect(spec.getCell({ row: 4, column: 0 })).toEqual(clean);
  });
});

describe("外向きの知らせと呼び出し側の口の並び", () => {
  it("決められた順に観測される", async () => {
    const run = await driveCanonicalSequence(createEagerFakeRenderer(), { source: arrayRowSource() });

    expect(run.calls).toEqual(CANONICAL_SEQUENCE);

    // 六つの知らせが 1 つも欠けていないことを、名前でも言っておく（並びの比較が空同士で
    // 一致したのではないことを読み手に示す）。
    const observed = run.calls.map(({ call }) => call);
    for (const callback of CALLBACKS) expect(observed).toContain(callback);
  });

  it("複製が返した文字列がそのまま貼り付けの入力になる（移植口は中身を解釈しない）", async () => {
    const renderer = createEagerFakeRenderer();
    const run = await driveCanonicalSequence(renderer, { source: arrayRowSource() });

    const paste = run.calls.find(({ call }) => call === "onPaste");
    const pastedText = paste?.args[1];

    // 表形式（行の区切りは改行、列の区切りはタブ）であること。
    expect(pastedText).toBe("1:0\t1:1\n2:0\t2:1");
    // 複製の約束の値が描き手（の代役）の側まで届いていること。**移植口は文字列を素通しするだけで、
    // 何行何列かも、どの列の型に掛けるかも知らない**（解釈は Rust 側の PasteCodec の仕事である）。
    expect(renderer.clipboard).toBe(pastedText);
  });
});

describe("実装の差し替え（呼び出しの並びが変わらない）", () => {
  /**
   * 4 通りを駆動し、記録された並びと、描画のために引いたセルの数を返す。
   *
   * 2 つの軸を掛け合わせる:
   *   - **移植口の実装**: マウントの時に可視の窓をまとめて引く実装と、操作に応じて必要な行だけを
   *     引き、知らせを次の微小タスクまで遅らせる実装（`fakeRenderer.ts`）。
   *   - **行の出所**: 行を配列として持つ模型と、序数からその場で作る模型（`interactionDriver.ts`）。
   *
   * 「差し替えても呼び出しの並びが変わらない」の実測はこの 4 通りの一致である。**実物の実装
   * （7.2 の Glide）を足すのは、この一致に 1 行を加えるだけでよい**（ヘッダの申し送りを参照）。
   */
  const drive = async (
    renderer: DrivableRenderer,
    source: RowSource,
  ): Promise<{ readonly run: DrivenRun; readonly asked: number }> => {
    const run = await driveCanonicalSequence(renderer, { source });
    return { run, asked: source.asked.length };
  };

  it("内部の作りも行の出所も違う 4 通りで、同じ並びが観測される", async () => {
    const eagerWithArray = await drive(createEagerFakeRenderer(), arrayRowSource());
    const eagerWithComputed = await drive(createEagerFakeRenderer(), lazyRowSource());
    const lazyWithArray = await drive(createLazyFakeRenderer(), arrayRowSource());
    const lazyWithComputed = await drive(createLazyFakeRenderer(), lazyRowSource());

    for (const { run } of [eagerWithArray, eagerWithComputed, lazyWithArray, lazyWithComputed]) {
      expect(run.calls).toEqual(CANONICAL_SEQUENCE);
    }

    // **これが無いと上の一致は弱い**: 同じ実装を 4 回走らせて同じ並びになっただけでも緑になる。
    // 実装が違う 2 つは、描画のために引くセルの数が実際に違う（窓をまとめて引く側の方が多い）。
    // 作りが違い、行の読み方が違い、知らせの時機も違うのに、外へ出る並びは同じである。
    expect(eagerWithArray.asked).toBeGreaterThan(lazyWithArray.asked);
    expect(lazyWithArray.asked).toBeGreaterThan(0);
  });

  it("描画の引きは未取得の行にも及び、そこで例外を投げない", async () => {
    // 取得済みを 2 行に限った出所を渡す。まとめて引く実装はマウントの時に可視の窓
    // （24 行）を引くので、未取得の行にも必ず当たる。**そこで例外が起こればこの駆動は落ちる。**
    const source = lazyRowSource(2);
    const run = await driveCanonicalSequence(createEagerFakeRenderer(), { source });

    expect(source.asked.some(({ row }) => row > 2)).toBe(true);
    expect(run.calls).toEqual(CANONICAL_SEQUENCE);
  });
});

/**
 * 型が「その型そのもの」であることの検査。互いに代入可能であること（部分型）では足りない —
 * 面に欄が 1 つ増えると部分型の関係は保たれたままなので、**双方向の包含**を要求する。
 */
type Exactly<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;

/**
 * 移植口の面。**ここに欄が増えれば下の `it` が型検査で落ちる**（`npm run typecheck` の段。
 * vitest の実行では型の誤りは見えないため、package.json の `//devDependencies` にも書いてある）。
 *
 * これが「移植口が編集の意味論・判定・履歴を知らない」ことの機械検査である。運び手を足すには
 * 面の欄を増やすしかない（この module は実行時の輸出を 1 つも持たない。下の `it` を参照）。
 */
const renderCellSurface: Exactly<keyof RenderCell, "text" | "variant" | "violated" | "loading"> = true;
const rendererSpecSurface: Exactly<
  keyof RendererSpec,
  | "columns"
  | "rowCount"
  | "getCell"
  | "onSelectionChange"
  | "onActivateEditor"
  | "onColumnResize"
  | "onColumnMove"
  | "onCopy"
  | "onPaste"
> = true;
const rendererHandleSurface: Exactly<keyof RendererHandle, "scrollTo" | "invalidate" | "destroy"> = true;
const renderColumnSurface: Exactly<keyof RenderColumn, "title" | "width"> = true;
const cellPositionSurface: Exactly<keyof CellPosition, "row" | "column"> = true;
const cellRangeSurface: Exactly<keyof CellRange, "start" | "end"> = true;
const rowSpanSurface: Exactly<keyof RowSpan, "start" | "count"> = true;
const gridRendererPortSurface: Exactly<keyof GridRendererPort, "mount"> = true;

describe("移植口は編集の意味論・判定・履歴を知らない", () => {
  it("面の型はこれ以上広がらない", () => {
    expect([
      renderCellSurface,
      rendererSpecSurface,
      rendererHandleSurface,
      renderColumnSurface,
      cellPositionSurface,
      cellRangeSurface,
      rowSpanSurface,
      gridRendererPortSurface,
    ]).toEqual([true, true, true, true, true, true, true, true]);
  });

  it("実行時の輸出が 1 つも無い（値を運ぶ経路が存在しない）", () => {
    // 移植口の module は実行時の値を 1 つも輸出しない。宣言だけの面であるため、判定・履歴・
    // 命令の運び手（関数であれ定数であれ）がここに現れる余地が無い。`TypeKindTag` は生成物からの
    // **型だけの取り込み**であり、バンドルから消える（`../../ipc/bindings` の取り込みが
    // 型だけであることは `port.ts` の import の形が示している）。
    expect(Object.getOwnPropertyNames(portModule)).toEqual([]);
  });
});

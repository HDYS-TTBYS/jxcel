/**
 * 描画層の移植口（`./port`）の駆動器（tasks.md 7.1。要件 2.1, 2.2, 2.3）。
 *
 * **テスト専用の道具である**（配布物へ入る経路は無い。`src/` のどの入口からも取り込まれない）。
 * 役割は 3 つある。
 *
 *   1. **決められた操作の並び**（マウント → 選択の変化 → 編集の起動 → 列幅の変更 → 列の移動 →
 *      複製 → 貼り付け → `scrollTo` → `invalidate` → `destroy`）を、どの実装にも同じように
 *      注ぎ込む（[`driveCanonicalSequence`]）。
 *   2. 移植口が**外へ出した呼び出しを順に記録する**（[`RecordedCall`]）。記録は移植口の
 *      callback の引数をそのままの形で持つので、余計な材料が載れば比較で見える。
 *   3. セルの出所（窓の記憶の模型。[`RowSource`]）を 2 通り用意し、行の持ち方が違っても
 *      呼び出しの並びが変わらないことを示せるようにする。
 *   4. **実物の配線へ面をかぶせる薄い層**（[`glideDrivableRenderer`]。タスク 7.2 が足した）を
 *      置き、7.1 の申し送り（実物でも同じ並びが観測されること）を 1 つの場所で満たす。
 *
 * # 移植口そのものには「利用者の操作」を注ぐ口が無い
 *
 * `GridRendererPort` が持つのは `mount` だけで、外向きの知らせは `RendererSpec` の callback で
 * ある。したがって駆動器は、移植口に**操作を起こす面**（[`RendererEventSource`]）を足した
 * [`DrivableRenderer`] を要求する。**この面は移植口の一部ではない** — 実物の実装（7.2 の Glide の
 * 写し）が実装すべきものではなく、テストの側が実物の通知（選択・起動・列幅・列の移動・複製・
 * 貼り付け）に 1 枚かぶせる薄い層である。実物に対するその層は
 * [`glideDrivableRenderer`] にある。
 *
 * # 何を記録し、何を記録しないか
 *
 * 記録するのは**移植口の契約に属する呼び出し**である: 呼び出し側が入る唯一の口（`mount`）、
 * 外向きの知らせ 6 つ、呼び出し側が使う口 3 つ（`scrollTo` / `invalidate` / `destroy`）。
 *
 * **`getCell` は記録しない。**セルをいつ何回引くかは実装の作りそのもの（窓をまとめて引くか、
 * 必要になった行だけ引くか）であり、並びの比較に持ち込むと「実装を差し替えても並びが変わらない」
 * という主張が崩れる（実装ごとに違ってよいものを比較対象に入れてしまう）。**この線引きが
 * `port.test.ts` の比較を意味のあるものにしている。**
 *
 * `mount` の引数も記録しない（`[]` とする）。器は実装ごとに別の物であり、並びの比較に持ち込むと
 * 比較そのものが成立しない。
 *
 * # 器（`HTMLElement`）をどう扱うか
 *
 * 移植口の signature は `mount(container: HTMLElement, …)` なので器を渡さねばならないが、
 * 本課題の面は純粋な論理であり DOM を要しない。そこで [`standInContainer`] が**どの属性を読んでも
 * 投げる**代役を返す。**「偽の実装は器に触れない」という前提を、確かめられる性質に変える**のが
 * 狙いである（触れば落ちる）。したがって `jsdom` / `happy-dom` を足す必要が無い。
 *
 * **7.2 の実物もこの規律の内側にある。**移植口が運ぶのは座標・文字・見出しだけであり、
 * Glide の**配線**（`./glideAdapter` の `createGlideWiring`）も DOM に触れない。DOM を要するのは
 * 配線を `DataEditor` へ繋ぐ面（`GlideSurface`）だけであり、**そちらは実物を起動して観測する**
 * （`src/features/smoke/portProbe*`。canvas を模した DOM では見え方の主張を何も裏付けられない）。
 * したがって `vitest.config.ts` の環境は `node` のままである（`jsdom` を足していない）。
 */
// **型だけの取り込みである**（`verbatimModuleSyntax` により `import type` が要る）。
import type { TypeKindTag } from "../../../ipc/bindings";
// 実物の配線へ面をかぶせる薄い層（下の `glideDrivableRenderer`）が使う。**値として要るのは
// `CompactSelection` と `emptyGridSelection` だけ**であり、Glide の部品そのものは読み込まない。
import { CompactSelection, emptyGridSelection, type GridSelection, type Item } from "@glideapps/glide-data-grid";
import type { GlideWiring } from "./glideAdapter";
import type {
  CellPosition,
  CellRange,
  GridRendererPort,
  RenderCell,
  RenderColumn,
  RendererHandle,
  RendererSpec,
  RowOrdinal,
  RowSpan,
} from "./port";

/** 標本の可視行数。 */
export const DRIVE_ROW_COUNT = 64;

/** 標本の列（描き手へ渡す形: 見出しと幅だけ）。 */
export const DRIVE_COLUMNS: readonly RenderColumn[] = [
  { title: "名前", width: 160 },
  { title: "数量", width: 96 },
  { title: "提供元", width: 200 },
];

/**
 * 標本の列の葉の型の札。**セルの札はここから来る**（生成物の `ColumnDescriptor.kind` の写しに
 * あたる。列の見出しには載せない — `RenderColumn` の docs を参照）。
 */
const DRIVE_KINDS: readonly TypeKindTag[] = ["Text", "Int", "Ref"];

/** 標本の違反の印: 7 行ごとの 3 行目。窓は違反の有無を 1 バイトで運ぶ（その 1 つをそのまま札にする）。 */
const VIOLATION_PERIOD = 7;
const VIOLATION_OFFSET = 3;

/** 移植口の呼び出しの名前。記録の並びを比較する単位である。 */
export type PortCall =
  | "mount"
  | "onSelectionChange"
  | "onActivateEditor"
  | "onColumnResize"
  | "onColumnMove"
  | "onCopy"
  | "onPaste"
  | "scrollTo"
  | "invalidate"
  | "destroy";

/** 記録された 1 回の呼び出し。`args` は**移植口が受け取った引数そのもの**である。 */
export interface RecordedCall {
  readonly call: PortCall;
  readonly args: readonly unknown[];
}

/**
 * 利用者の操作を移植口へ注ぐ面。**テスト専用であり、移植口の一部ではない**（ヘッダを参照）。
 *
 * すべて `Promise` を返すのは、**知らせの時機を実装の自由にするためである** — 同期に通知する
 * 実装と、次の微小タスクまで遅らせる実装のどちらも、駆動器が 1 つずつ待って進められる。
 * 並びが時機に依存しないことが、`port.test.ts` の比較で確かめられる。
 */
export interface RendererEventSource {
  /** 選択が変わった（解除は `null`）。 */
  emitSelectionChange(range: CellRange | null): Promise<void>;
  /** 編集の起動が指示された。 */
  emitActivateEditor(position: CellPosition): Promise<void>;
  /** 列の幅が変更された。 */
  emitColumnResize(column: number, width: number): Promise<void>;
  /** 列の位置が変更された。 */
  emitColumnMove(from: number, to: number): Promise<void>;
  /** 複製が指示された。 */
  emitCopy(range: CellRange): Promise<void>;
  /** 貼り付けが指示された。 */
  emitPaste(anchor: CellPosition, text: string): Promise<void>;
  /**
   * 直近に**クリップボードへ渡した**文字列（実装の側の控え）。複製の約束の値が移植口を通って
   * 描き手の側まで届いたことを観測するために使う。まだ複製していなければ `null`。
   */
  readonly clipboard: string | null;
}

/** 駆動器が要求するもの: 移植口と、利用者の操作を注ぐ面。 */
export type DrivableRenderer = GridRendererPort & RendererEventSource;

/**
 * セルの出所（窓の記憶の模型）。**呼び出し側（画面）の側にある** — 移植口がセルを引くと、
 * 画面がここから答える（design.md の「Glide の `getCellContent` は引きに来る形」と同じ向き）。
 *
 * `cell` は**全域である**: 範囲の外の位置でも例外を投げず、読み込み中の札を返す（移植口の
 * 不変条件。`./port` の `RendererSpec.getCell` の docs）。
 */
export interface RowSource {
  /** その位置のセル。**同期であり例外を投げない。**未取得の行は `loading: true`。 */
  cell(position: CellPosition): RenderCell;
  /** `cell` が引かれた位置の記録（順に積む）。描画の引き方の観測に使う。 */
  readonly asked: readonly CellPosition[];
  /** 窓の到着の模型: その区間までを取得済みにする。 */
  fetch(span: RowSpan): void;
}

/** 標本の表示文字列。行と列が読み取れる形にする（表の格子の写しである）。 */
const SAMPLE_SEPARATOR = ":";

/**
 * 出所の共通の答え方: 列の札を決め、取得済みの区間の外と範囲の外を読み込み中に落とし、
 * 取得済みなら文字と違反の印を載せる。**どこでも例外を投げない**のが要点である。
 *
 * `fetchedThrough` は取得済みの終端の可視行の序数である（窓の到着で広がる）。
 */
function answer(fetchedThrough: RowOrdinal, position: CellPosition, text: string): RenderCell {
  const variant = DRIVE_KINDS[position.column];
  const inRange =
    Number.isInteger(position.row) && position.row >= 0 && position.row <= fetchedThrough;
  if (variant === undefined || !inRange) {
    // 未取得・範囲の外を表す札である。**空白で代用しない**（値なしと区別がつかない）。
    // 列が範囲の外であるときは列の札も決まらないので、既定の入力へ落ちる札（`Any`）を使う —
    // 生成物の `ColumnDescriptor.kind` が `null` になりうるのと同じ状況である。
    return { text: "", variant: variant ?? "Any", violated: false, loading: true };
  }
  return {
    text,
    variant,
    violated: position.row % VIOLATION_PERIOD === VIOLATION_OFFSET,
    loading: false,
  };
}

/**
 * 行を**配列として持つ**出所（表の写しを丸ごと抱える作り）。`fetchedThrough` より先の行は
 * 取得済みでないものとして読み込み中を返す。
 */
export function arrayRowSource(fetchedThrough: RowOrdinal = DRIVE_ROW_COUNT - 1): RowSource {
  const rows: readonly (readonly string[])[] = Array.from({ length: DRIVE_ROW_COUNT }, (_, row) =>
    Array.from(
      { length: DRIVE_COLUMNS.length },
      (_unused, column) => `${row}${SAMPLE_SEPARATOR}${column}`,
    ),
  );
  let fetched = fetchedThrough;
  const asked: CellPosition[] = [];
  return {
    cell(position) {
      asked.push(position);
      const row = rows[position.row];
      return answer(fetched, position, row?.[position.column] ?? "");
    },
    asked,
    fetch(span) {
      fetched = Math.max(fetched, span.start + span.count - 1);
    },
  };
}

/**
 * 行を**一切持たず、序数からその場で作る**出所（引かれるまで値が存在しない作り）。
 * `arrayRowSource` と答えは同じであり、**持ち方だけが違う**。
 */
export function lazyRowSource(fetchedThrough: RowOrdinal = DRIVE_ROW_COUNT - 1): RowSource {
  let fetched = fetchedThrough;
  const asked: CellPosition[] = [];
  return {
    cell(position) {
      asked.push(position);
      return answer(fetched, position, `${position.row}${SAMPLE_SEPARATOR}${position.column}`);
    },
    asked,
    fetch(span) {
      fetched = Math.max(fetched, span.start + span.count - 1);
    },
  };
}

/** [`createRendererSpec`] の入力。 */
export interface SpecOptions {
  /** セルの出所。 */
  readonly source: RowSource;
  /** 記録の宛先。省くと何も記録しない（セルの取得だけを試すときに使う）。 */
  readonly record?: (call: PortCall, args: readonly unknown[]) => void;
  /**
   * 複製が返す文字列を作る関数。省くと出所から組み立てる（{@link clipboardTextFor}）。
   * 駆動器はこれを使って、**貼り付けへ渡す文字列を複製の戻り値そのものにする**（作り直さない）。
   */
  readonly copyText?: (range: CellRange) => string;
}

/**
 * 複製が返す表形式のテキスト（行の区切りは改行、列の区切りはタブ）。要件 7.1、7.2。
 *
 * **本物では Rust 側の `PasteCodec` が作る**。ここはその代役であり、範囲のセルを出所から引いて
 * 並べるだけである（この引きも `asked` に載る）。
 */
function clipboardTextFor(source: RowSource, range: CellRange): string {
  const lines: string[] = [];
  for (let row = range.start.row; row <= range.end.row; row += 1) {
    const cells: string[] = [];
    for (let column = range.start.column; column <= range.end.column; column += 1) {
      cells.push(source.cell({ row, column }).text);
    }
    lines.push(cells.join("\t"));
  }
  return lines.join("\n");
}

/**
 * 画面（8.x）の側の代役として仕様を組み立てる。**外向きの知らせは記録するだけ**であり、
 * 状態を変えない（何をどう変えるかは画面の仕事である）。
 */
export function createRendererSpec(options: SpecOptions): RendererSpec {
  return {
    columns: DRIVE_COLUMNS,
    rowCount: DRIVE_ROW_COUNT,
    getCell: (position) => options.source.cell(position),
    onSelectionChange: (range) => {
      options.record?.("onSelectionChange", [range]);
    },
    onActivateEditor: (position) => {
      options.record?.("onActivateEditor", [position]);
    },
    onColumnResize: (column, width) => {
      options.record?.("onColumnResize", [column, width]);
    },
    onColumnMove: (from, to) => {
      options.record?.("onColumnMove", [from, to]);
    },
    onCopy: async (range) => {
      const text = options.copyText?.(range) ?? clipboardTextFor(options.source, range);
      options.record?.("onCopy", [range]);
      return text;
    },
    onPaste: async (anchor, text) => {
      options.record?.("onPaste", [anchor, text]);
    },
  };
}

/**
 * 器の代役。**どの属性を読んでも投げる**（`Proxy`）。
 *
 * 偽の実装は器に触れない — 触れないことを「前提」ではなく「確かめられる性質」にするために、
 * 触れたらその場で落ちるようにしてある。DOM の実装（`jsdom`）を足さないで済む根拠がこれである。
 * 本物の実装（7.2）は実物の器を要するので、そこで環境を見直すこと（`vitest.config.ts`）。
 */
export function standInContainer(): HTMLElement {
  const refuse = (property: string): never => {
    throw new Error(`偽の実装が器へ触れた（${property}）。器は本課題の面ではない`);
  };
  return new Proxy({}, { get: (_target, property) => refuse(String(property)) }) as unknown as HTMLElement;
}

/** [`driveCanonicalSequence`] の入力。 */
export interface DriveOptions {
  /** セルの出所。 */
  readonly source: RowSource;
  /** 器。省くと [`standInContainer`] の代役を使う（DOM を持たない走らせ方）。 */
  readonly container?: HTMLElement;
}

/** 1 回の駆動の結果。**観測された呼び出しの並び**が主である。 */
export interface DrivenRun {
  readonly renderer: DrivableRenderer;
  readonly container: HTMLElement;
  readonly spec: RendererSpec;
  readonly handle: RendererHandle;
  readonly calls: readonly RecordedCall[];
}

// 決められた操作の並び（駆動器が持つ台本）。**この値そのものが契約であり、期待する並びを
// 確かめる側（`port.test.ts`）はここから読まずに逐語で書く**（台本ごと書き換えたときに
// 気づけるようにするためである）。
const SELECTION: CellRange = { start: { row: 1, column: 0 }, end: { row: 3, column: 1 } };
const ACTIVATION: CellPosition = { row: 2, column: 1 };
const RESIZE_COLUMN = 1;
const RESIZE_WIDTH = 144;
const MOVE_FROM = 2;
const MOVE_TO = 0;
const COPY_RANGE: CellRange = { start: { row: 1, column: 0 }, end: { row: 2, column: 1 } };
const PASTE_ANCHOR: CellPosition = { row: 4, column: 0 };
const SCROLL_TO: CellPosition = { row: 40, column: 2 };
const INVALIDATE: RowSpan = { start: 8, count: 4 };

/**
 * 決められた操作の並びを 1 回注ぎ、観測された呼び出しの並びを返す。
 *
 * 順は マウント → 選択の変化 → 編集の起動 → 列幅の変更 → 列の移動 → 複製 → 貼り付け →
 * `scrollTo` → `invalidate` → `destroy` である。複製で返った文字列は**そのまま貼り付けの入力に
 * 使う** — 移植口が中身を解釈しないこと（素通しであること）を、同じバイトが往復することで示す。
 *
 * 知らせは 1 つずつ待ってから次へ進む。したがって**同期に通知する実装と、遅らせて通知する
 * 実装のどちらでも並びは同じ**になる（時機は実装の自由であり、契約ではない）。
 */
export async function driveCanonicalSequence(
  renderer: DrivableRenderer,
  options: DriveOptions,
): Promise<DrivenRun> {
  const calls: RecordedCall[] = [];
  const record = (call: PortCall, args: readonly unknown[]): void => {
    calls.push({ call, args });
  };
  // 複製が返した文字列の控え。**貼り付けへはこれをそのまま渡す** — 作り直さないので、
  // 「複製の戻り値が素通しで貼り付けの入力になる」ことが 1 つの値で示される。
  let copiedText = "";
  const spec = createRendererSpec({
    source: options.source,
    record,
    copyText: (range) => {
      copiedText = clipboardTextFor(options.source, range);
      return copiedText;
    },
  });
  const container = options.container ?? standInContainer();

  record("mount", []);
  const handle = renderer.mount(container, spec);

  await renderer.emitSelectionChange(SELECTION);
  await renderer.emitActivateEditor(ACTIVATION);
  await renderer.emitColumnResize(RESIZE_COLUMN, RESIZE_WIDTH);
  await renderer.emitColumnMove(MOVE_FROM, MOVE_TO);
  await renderer.emitCopy(COPY_RANGE);
  await renderer.emitPaste(PASTE_ANCHOR, copiedText);

  record("scrollTo", [SCROLL_TO]);
  handle.scrollTo(SCROLL_TO);
  record("invalidate", [INVALIDATE]);
  handle.invalidate(INVALIDATE);
  record("destroy", []);
  handle.destroy();

  return { renderer, container, spec, handle, calls };
}

/** 移植口の範囲（矩形）を Glide の選択へ写す。**錨は左上**である（貼り付けの宛先になる）。 */
function selectionOfRange(range: CellRange | null): GridSelection {
  if (range === null) {
    return emptyGridSelection;
  }
  return {
    columns: CompactSelection.empty(),
    rows: CompactSelection.empty(),
    current: {
      cell: [range.start.column, range.start.row],
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

/**
 * **実物の配線**（`./glideAdapter` の [`GlideWiring`]）へ [`RendererEventSource`] をかぶせた
 * 薄い層（tasks.md 7.2。`fakeRenderer.ts` のヘッダの申し送り）。
 *
 * 7.1 が残した課題はこれである: 移植口には「利用者の操作」を注ぐ口が無いので、**実物の通知**
 * （選択・起動・列幅・列の移動・複製・貼り付け）にテスト専用の面を 1 枚かぶせ、`port.test.ts` の
 * 比較表と同じ並びが実物でも観測されることを示す。**この層は移植口の一部ではない** —
 * `GlideWiring` が実装すべきものではなく、Glide の引数の形と移植口の引数の形の**間の写し**を
 * テストの側が持つだけである。
 *
 * # 何を写し、何を写さないか
 *
 * | 注入する操作 | Glide の通知（`GlideWiringProps`） | 移植口 |
 * |---|---|---|
 * | `emitSelectionChange` | `onGridSelectionChange`（`GridSelection`） | `onSelectionChange`（矩形） |
 * | `emitActivateEditor` | `onCellActivated`（`Item` = 列, 行） | `onActivateEditor`（位置） |
 * | `emitColumnResize` | `onColumnResize`（列, 幅, 添字, grow 込み） | `onColumnResize`（添字, 幅） |
 * | `emitColumnMove` | `onColumnMoved`（from, to） | `onColumnMove`（from, to） |
 * | `emitCopy` | DOM の `copy`（本物は `GlideSurface` が受ける） | `onCopy`（範囲） |
 * | `emitPaste` | DOM の `paste`（同じ） | `onPaste`（錨, 文字列） |
 *
 * **複製と貼り付けだけは Glide の props を通らない。** 本物の経路は DOM の `copy` / `paste` を
 * `GlideSurface` が自分で受けて配線へ渡す（Glide 自身のクリップボードの経路は移植口を通らない
 * ので止めてある。`glideAdapter.tsx` のモジュール doc）。したがってこの層も `copyRange` /
 * `pasteAt` を直接叩く — **錨を選択から決める部分（`currentAnchor`）は本物と同じ関数である**
 * が、ここでは呼ばない（選択を動かすと並びに余計な `onSelectionChange` が載るためである。
 * 錨の決定は `glideAdapter.test.ts` が名指しで固定している）。
 *
 * # 器は使わない
 *
 * 配線は DOM を持たないので、この層は `mount` へ渡された器に触れない（`fakeRenderer.ts` の
 * 2 つの実装と同じである — 触れないことは駆動器が確かめる）。
 */
export function glideDrivableRenderer(
  createWiring: (spec: RendererSpec) => GlideWiring,
): DrivableRenderer {
  let mounted: { readonly spec: RendererSpec; readonly wiring: GlideWiring } | null = null;
  let clipboard: string | null = null;

  const wiringOf = (operation: string): GlideWiring => {
    if (mounted === null) {
      throw new Error(`移植口がマウントされていない（または破棄された）のに ${operation} が起きた`);
    }
    return mounted.wiring;
  };

  return {
    mount(_container, initial) {
      mounted = { spec: initial, wiring: createWiring(initial) };
      return mounted.wiring.handle;
    },
    async emitSelectionChange(range) {
      wiringOf("emitSelectionChange").props.onGridSelectionChange(selectionOfRange(range));
    },
    async emitActivateEditor(position) {
      const item: Item = [position.column, position.row];
      wiringOf("emitActivateEditor").props.onCellActivated(item);
    },
    async emitColumnResize(column, width) {
      const wiring = wiringOf("emitColumnResize");
      const target = wiring.props.columns[column] ?? { title: "", width };
      // Glide の第 2 引数は変更後の幅、第 3 引数は列の添字、第 4 引数は grow 込みの幅である
      // （写しは第 4 引数を使わない — 渡しているのは signature を合わせるためである）。
      wiring.props.onColumnResize(target, width, column, width);
    },
    async emitColumnMove(from, to) {
      wiringOf("emitColumnMove").props.onColumnMoved(from, to);
    },
    async emitCopy(range) {
      clipboard = await wiringOf("emitCopy").copyRange(range);
    },
    async emitPaste(anchor, text) {
      await wiringOf("emitPaste").pasteAt(anchor, text);
    },
    get clipboard() {
      return clipboard;
    },
  };
}

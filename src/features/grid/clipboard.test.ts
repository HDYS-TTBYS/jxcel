/**
 * 範囲の複製と貼り付け（tasks.md 8.7。data-grid 要件 7.1、7.2、7.3、7.4、8.6、8.9、1.7）。
 *
 * # ここで固定するもの
 *
 * 1. **複製のテキスト**（要件 7.1、7.2）。選択の範囲の**行と列の配置を保った**表形式テキストで
 *    あり、行の区切りは LF、列の区切りは TAB、区切りと `"` を含む値は囲む（囲みの中の `"` は
 *    `""` へ倍にする）。**往復**（書き出したテキストを読むと元のセルへ戻ること）も同じ検査で
 *    固定する — 規則の正典は `crates/data-grid/src/edit/paste.rs`（3.4）であり、ここはその写し
 *    である（module doc「境界の項目」）。
 * 2. **複製が送らない条件**（8.6 と同じ規律）。窓がまだ届いていないセルがあれば、**空文字で
 *    埋めずに**理由を返す（空文字を返せばクリップボードが空になり、利用者には「複製できた」と
 *    見える）。
 * 3. **貼り付けの宛先の座標空間**（要件 8.6、8.9）。錨は**物理の行（`RowId`）と文書の列**であり、
 *    表示の序数でも表示の列でもない。歩く順序（`rows`）は**表示されている行の並び**であり、
 *    絞り込み・並べ替えの下で文書の順序と食い違う。
 * 4. **行の補充の境目**（要件 7.4）。矩形が可視行の末尾を越えるときは、渡せるだけの行を渡し、
 *    残りはドメインが末尾へ足す（**可視行数を越えて存在しない行の識別子を作らない**）。
 * 5. **行数が変わったあとの記憶の作り直し**（要件 1.7）。`WindowCache.clear(row_count)` を
 *    両方向で固定し、適用されなかったときは捨てないこと（8.6 と同じ規律）も固定する。
 * 6. **画面の面**（[`createClipboardSurface`]）。移植口へ渡す 3 つの口（`copyRange` /
 *    `pasteAt` / `sendPaste`）を組む唯一の場所であり、**表示の列を文書の列として送らない
 *    こと**・**送る腕が境界へ行くこと**・**往復の結果が画面へ届くこと**をここで固定する
 *    （この 3 つは画面の関数本体に在った頃、どの検査でも捕まらなかった — 実測は
 *    `clipboardRequests.test.ts` の module doc と design.md「8.7 が切り出した画面の面」）。
 *
 * # ここで固定しないもの（**正直に書く**）
 *
 * - **文書へ実際に値が書かれること**と、**補充される行の既定値**は Rust 側の契約である
 *   （`crates/data-grid` の検査）。本 module は「何を送ったか」までしか主張しない
 * - **クリップボードとの実際の往復**（他の表計算アプリケーションとの間の往復）は、実物の起動と
 *   実機のクリップボードを要する（要件 7.2 の後半）。`node` の環境には DOM が無く、7.2 の
 *   レビューが記録したとおり**システムのクリップボードの読み戻しはこの環境では観測できない**
 *   （module doc「単体テストが観測しないもの」）
 * - **メニューからの実行**（要件 7.8）のうち、**複製は `clipboardRequests.test.ts` が固定する**
 *   （イベント名が生成物から来ること、打鍵とメニューが同じ入口へ着くこと）。**貼り付けの項目は
 *   器が登録していない**（クリップボードの読み口が無いため。module doc「境界の項目」）
 */
import { describe, expect, it } from "vitest";

import type {
  GridEditCommand,
  GridEditOutcome,
  GridEditResponse,
  IpcResult,
} from "../../ipc/bindings";
import type { IpcClientError } from "../../ipc/client";
import type { GridClient } from "./gridClient";
import type { WindowCache } from "./windowCache";
import {
  applyPaste,
  createClipboardSurface,
  planCopy,
  planPaste,
  parseTableText,
  runPastePlan,
  tableText,
  tableTextRowCount,
  type PasteContext,
  type PastePayload,
  type PastePlan,
} from "./clipboard";
import type { CellPosition, CellRange, RenderCell } from "./renderer/port";

// ===========================================================================
// 道具（偽の境界・偽の記憶・識別子の綴り）
// ===========================================================================

/** 境界の窓の文脈（`WindowContext`。値そのものは検査に効かない）。 */
const CONTEXT = { window: "main" } as const;

/** 境界の失敗（経路の不達）。 */
const FAILURE: IpcClientError = { kind: "Document", detail: { message: "経路が不達である" } };

/**
 * 行の識別子（**可視の序数からは作れない綴りである**）。
 *
 * 序数から機械的に作れる綴りを使うと、「識別子を送った」ことと「序数を送った」ことが同じ
 * 結果になり、取り違えを捕まえられない（8.6 の検査と同じ規律。先頭の 5 件は固定ファイル
 * `crates/data-grid/tests/fixtures/window_protocol.txt` のヘッダが名指す 26 文字である）。
 */
const ROW_IDS: readonly string[] = [
  "01ARZ3NDEKTSV4RRFFQ69G5FAW",
  "01ARZ3NDEKTSV4RRFFQ69G5FAX",
  "01ARZ3NDEKTSV4RRFFQ69G5FAY",
  "01ARZ3NDEKTSV4RRFFQ69G5FAZ",
  "01ARZ3NDEKTSV4RRFFQ69G5FB0",
];

/** 可視の序数 `row` の識別子（6 件目以降は同じ形で綴りを進める）。 */
function rowIdOf(row: number): string {
  return ROW_IDS[row] ?? `01ARZ3NDEKTSV4RRFFQ69G5G${String(row).padStart(4, "0")}`;
}

/** 序数 `first`..`last` の識別子を答える口（範囲の外は `null` ＝ 窓がまだ届いていない）。 */
function rowIdsFor(first: number, last: number): (position: CellPosition) => string | null {
  return (position) =>
    position.row >= first && position.row <= last ? rowIdOf(position.row) : null;
}

/** 文書の列を答える口（既定は表示の列の恒等。展開の下では食い違う — 要件 5.1、8.6）。 */
function columnsShifted(offset: number): (position: CellPosition) => number | null {
  return (position) => position.column + offset;
}

/** 計画の文脈（既定は「可視行 20・識別子は可視行の数だけ・列は恒等」）。 */
function pasteContext(overrides: Partial<PasteContext> = {}): PasteContext {
  return {
    visibleRows: 20,
    rowId: rowIdsFor(0, 19),
    documentColumn: columnsShifted(0),
    ...overrides,
  };
}

/** 描かれているセル（値の一覧から引く口を組む。届いていない行は読み込み中である）。 */
function cellSource(rows: readonly (readonly string[])[]): (position: CellPosition) => RenderCell {
  return (position) => {
    const value = rows[position.row]?.[position.column];
    return value === undefined
      ? { text: "", variant: "Text", violated: false, loading: true }
      : { text: value, variant: "Text", violated: false, loading: false };
  };
}

/** 適用の結果（指定した欄だけを変えて組む）。 */
function outcomeOf(overrides: Partial<GridEditOutcome>): GridEditOutcome {
  return {
    affected: [],
    coercions: [],
    violation_total: 0,
    violations: [],
    revalidated_columns: [],
    row_count: 20,
    ...overrides,
  };
}

/**
 * 成功の封筒。**世代も応答が運ぶ**（タスク 10.1。既定は「1 つ進んだ後」に当たる値である）。
 */
function applied(
  outcome: GridEditOutcome,
  generation = "2",
): IpcResult<GridEditResponse, IpcClientError> {
  return { status: "ok", data: { context: CONTEXT, outcome, generation } };
}

/**
 * 偽の境界。**使われてはならない口は例外を投げ、`applyEdit` は記録してから応答する**
 * （送らない経路が本当に何も送らないことは、この形で初めて固定できる。8.6 と同じ）。
 */
interface FakeClient extends GridClient {
  readonly edits: readonly GridEditCommand[];
}

function fakeClient(answer: IpcResult<GridEditResponse, IpcClientError>): FakeClient {
  const edits: GridEditCommand[] = [];
  const unused = (name: string) => (): never => {
    throw new Error(`貼り付けの経路は ${name} を呼んではならない`);
  };
  return {
    edits,
    readDocumentState: async () => unused("document_state")(),
    openSheet: async () => unused("grid_open_sheet")(),
    setView: async () => unused("grid_set_view")(),
    readWindow: async () => unused("grid_rows_window")(),
    applyEdit: async (command) => {
      edits.push(command);
      return answer;
    },
    findViolation: async () => unused("grid_find_violation")(),
    readHistory: async () => unused("grid_history")(),
  };
}

/** 偽の記憶（`clear` だけ。**渡された行数を記録する**）。 */
function fakeClear(): {
  readonly cache: Pick<WindowCache, "clear">;
  readonly cleared: readonly (number | undefined)[];
} {
  const cleared: (number | undefined)[] = [];
  return {
    cleared,
    cache: {
      clear: (rowCount?: number) => {
        cleared.push(rowCount);
      },
    },
  };
}

/** 計画の送る腕（送らなかった計画では投げる — 検査の意図を読めるようにする）。 */
function payloadOf(plan: PastePlan): PastePayload {
  if (plan.kind !== "send") {
    throw new Error(`送る腕ではなかった（${plan.kind}）`);
  }
  return plan.payload;
}

/** 矩形（2 つの角から組む）。 */
function rangeOf(start: CellPosition, end: CellPosition): CellRange {
  return { start, end };
}

// ===========================================================================
// 1. 表形式テキストの書き出し（要件 7.1、7.2）
// ===========================================================================

describe("複製のテキストは行と列の配置を保つ（要件 7.1、7.2）", () => {
  it("セルの矩形を、行は LF・列は TAB で繋ぐ", () => {
    // 行の区切りは LF、列の区切りは TAB である（`PasteCodec` の規則表と同じ）。
    expect(tableText([["名前", "数量"], ["りんご", "3"]])).toBe("名前\t数量\nりんご\t3");
    // **配置は行と列で決まる**（値の個数ではなく、渡された矩形そのものである）。
    expect(tableText([["a", "b", "c"]])).toBe("a\tb\tc");
    expect(tableText([["a"], ["b"], ["c"]])).toBe("a\nb\nc");
  });

  it("区切りと `\"` を含む値は囲み、囲みの中の `\"` を `\"\"` へ倍にする", () => {
    expect(tableText([["a\tb"]])).toBe('"a\tb"');
    expect(tableText([["a\nb"]])).toBe('"a\nb"');
    // `\r` を含む値も囲む（囲まないと、値の末尾の `\r` と次の行の区切りの `\n` が繋がって
    // **行が 1 つ増える**。`paste.rs` のモジュール docs「往復」）。
    expect(tableText([["a\rb"]])).toBe('"a\rb"');
    expect(tableText([["say \"hi\""]])).toBe('"say ""hi"""');
    // 値の途中の `"` も囲みの対象である（`PasteCodec::write` は `"` を含む値を囲む）—
    // 囲まない綴りも**読み直せば**同じ値になるが、正典（`paste.rs`）と 1 バイト違わないことを
    // ここで固定する。
    expect(tableText([["a\"b"]])).toBe('"a""b"');
  });

  it("空の値は空の綴りで書く（一般の表として通常の綴りである）", () => {
    expect(tableText([["", "x"]])).toBe("\tx");
    expect(tableText([["", ""], ["", ""]])).toBe("\t\n\t");
  });

  it("空の矩形は空のテキストである（読み直すと行 0 件）", () => {
    expect(tableText([])).toBe("");
  });

  it("書き出したテキストを読み直すと、同じセルへ戻る（往復）", () => {
    // **規則の正典は `crates/data-grid/src/edit/paste.rs`（3.4）である。**本 module の書き出しは
    // その写しであり、読む側（`parseTableText`）も同じ規則の写しである — ここで固定するのは
    // 「この 2 つが同じ規則を実装していること」であり、**Rust の読む側が同じ結果を返すことは
    // 実測で確かめてある**（research.md「複製のテキストの往復」）。
    const rectangles: readonly (readonly (readonly string[])[])[] = [
      [["名前", "数量"], ["りんご", "3"]],
      [["a\tb", "c"], ["d", "e\nf"]],
      [["say \"hi\"", ""], ["", "\"quoted\""]],
      [["cr\rhere", "tab\there"]],
      [["", ""], ["", ""]],
      [["1"]],
      [["a\"b", "c\"d"]],
    ];
    for (const cells of rectangles) {
      expect(parseTableText(tableText(cells))).toEqual(cells.map((row) => [...row]));
    }
  });

  it("書き出しは Rust の `PasteCodec::write` と 1 バイトも違わない（実測の突き合わせ）", () => {
    // 規則の正典は `crates/data-grid/src/edit/paste.rs`（3.4）である。本 module の写しが正典と
    // 同じ綴りを出すことを、**使い捨ての駆動器（リポジトリの外）で `PasteCodec::write` を呼んで
    // 実測した**（research.md「複製のテキストの往復」。標本は本検査と同じものである）。
    // 下の文字列はその実測の出力そのままである（**推測で書いた綴りではない**）。
    const goldens: readonly (readonly [readonly (readonly string[])[], string])[] = [
      [[["名前", "数量"], ["りんご", "3"]], "名前\t数量\nりんご\t3"],
      [[["a\tb", "c"], ["d", "e\nf"]], '"a\tb"\tc\nd\t"e\nf"'],
      [[["say \"hi\"", ""], ["", "\"quoted\""]], '"say ""hi"""\t\n\t"""quoted"""'],
      [[["cr\rhere", "tab\there"]], '"cr\rhere"\t"tab\there"'],
      [[["", ""], ["", ""]], "\t\n\t"],
      [[["1"]], "1"],
      [[["a\"b", "c\"d"]], '"a""b"\t"c""d"'],
    ];
    for (const [cells, golden] of goldens) {
      expect(tableText(cells)).toBe(golden);
      // **読む側の行数も正典と同じである**（貼り付けの宛先を決めるのに使う値である）。
      expect(tableTextRowCount(golden)).toBe(cells.length);
    }
    // 行数の数え方のうち、囲みと末尾の区切りに当たる 4 件（同じ実測の出力である）。
    expect(tableTextRowCount("a\nb\nc")).toBe(3);
    expect(tableTextRowCount('"a\nb"')).toBe(1);
    expect(tableTextRowCount("a\n")).toBe(1);
    expect(tableTextRowCount("a\n\n")).toBe(2);
  });

  it("既知の限界: 1 行 1 列でその値が空の矩形は、空のテキストになる", () => {
    // `paste.rs` のモジュール docs「往復」が記録している 1 件である（空の値と空のテキストは
    // 表示文字列の上で区別できない）。**本 module もこの 1 件を救わない** — `""` として書くと、
    // 一般の表では空のセルがすべて `""` になり、他のアプリケーションへ渡す綴りとして通常と
    // 違うものになる（要件 7.2）。
    expect(tableText([[""]])).toBe("");
    expect(parseTableText("")).toEqual([]);
  });
});

// ===========================================================================
// 2. 複製の計画（要件 7.1。窓の記憶からテキストを組む）
// ===========================================================================

describe("複製は選択の範囲を表示の位置の順に読む（要件 7.1）", () => {
  it("選択の矩形のセルを、左上から右下へ読んで書く", () => {
    const plan = planCopy({
      range: rangeOf({ row: 0, column: 0 }, { row: 1, column: 1 }),
      cell: cellSource([
        ["r0c0", "r0c1", "r0c2"],
        ["r1c0", "r1c1", "r1c2"],
      ]),
    });

    expect(plan).toEqual({ kind: "text", text: "r0c0\tr0c1\nr1c0\tr1c1" });
  });

  it("右下から左上へ引いた選択でも、錨の位置に依らず同じ配置になる", () => {
    // 移植口の `CellRange` は正規化されている前提である（`port.ts` の doc）。**前提が崩れても
    // 取り違えない**（本 module が自分で正規化する）。
    const plan = planCopy({
      range: rangeOf({ row: 1, column: 1 }, { row: 0, column: 0 }),
      cell: cellSource([
        ["r0c0", "r0c1"],
        ["r1c0", "r1c1"],
      ]),
    });

    expect(plan).toEqual({ kind: "text", text: "r0c0\tr0c1\nr1c0\tr1c1" });
  });

  it("窓が届いていないセルがあれば、送らずに理由を返す（空文字で埋めない）", () => {
    const plan = planCopy({
      range: rangeOf({ row: 0, column: 0 }, { row: 2, column: 0 }),
      cell: cellSource([["r0"], ["r1"]]),
    });

    expect(plan.kind).toBe("refused");
    if (plan.kind !== "refused") {
      throw new Error("拒否の腕ではなかった");
    }
    // **利用者に見える文言が事実と食い違わない**（何が足りないかを名乗る）。
    expect(plan.message).toContain("複製");
    expect(plan.message).toContain("届いていない");
  });

  it("選択が空の矩形（1 セル）でも、そのセル 1 つを書く", () => {
    expect(
      planCopy({
        range: rangeOf({ row: 1, column: 0 }, { row: 1, column: 0 }),
        cell: cellSource([["a"], ["b"]]),
      }),
    ).toEqual({ kind: "text", text: "b" });
  });
});

// ===========================================================================
// 3. 貼り付けの計画（要件 7.3、7.4、8.6、8.9）
// ===========================================================================

describe("貼り付けの宛先は物理の行と文書の列である（要件 8.6、8.9）", () => {
  it("錨は行の識別子と文書の列であり、表示の序数でも表示の列でもない", () => {
    // 表示の列 1 が文書の列 3 である世界（入れ子の展開が効いている。要件 5.1）。
    const payload = payloadOf(
      planPaste({ row: 2, column: 1 }, "a\tb", pasteContext({ documentColumn: columnsShifted(2) })),
    );

    expect(payload.anchor).toEqual({ row: rowIdOf(2), column: 3 });
    // 表示の序数（2）は送らない（送れば別の行へ書く。8.6 の取り違え）。
    expect(payload.anchor.row).not.toBe("2");
  });

  it("歩く順序は表示されている行の並びである（絞り込み・並べ替えの下でも成り立つ）", () => {
    // 並べ替えの下では、可視の序数 1 の行が文書の先頭の行であるとは限らない。**送るのは
    // 表示の並びそのもの**（識別子は窓にしか無く、序数からは作れない）。
    const payload = payloadOf(
      planPaste({ row: 1, column: 0 }, "a\nb\nc", pasteContext({ rowId: rowIdsFor(0, 19) })),
    );

    expect(payload.rows).toEqual([rowIdOf(1), rowIdOf(2), rowIdOf(3)]);
  });

  it("矩形が可視行の末尾を越えるときは、渡せる行だけを渡す（残りはドメインが足す）", () => {
    // 要件 7.4 の行の補充である。**存在しない行の識別子を作らない**（作ればドメインは
    // `UnknownRow` で失敗するか、別の行へ書く）。
    const payload = payloadOf(
      planPaste({ row: 19, column: 0 }, "a\nb\nc", pasteContext({ visibleRows: 20 })),
    );

    expect(payload.rows).toEqual([rowIdOf(19)]);
  });

  it("**3 × 1 の矩形は 3 行ぶんの並びを渡す**（足す行数をドメインが決められるようにする）", () => {
    // 行数を渡し間違えると、ドメインは「並びが尽きた」と見て行を足す（既存の行へ書かない）。
    const payload = payloadOf(planPaste({ row: 5, column: 0 }, "a\nb\nc", pasteContext()));
    expect(payload.rows).toHaveLength(3);
  });

  it("囲みの中の改行は行の区切りではない（行数を数え違えない）", () => {
    // `"a\nb"` は 1 行 1 列である（`PasteCodec` の囲みの規則）。素朴に `\n` を数えると 2 行と
    // なり、1 つ余分な行の識別子を求めて拒否する（あるいは余分な行へ書く）。
    const payload = payloadOf(planPaste({ row: 0, column: 0 }, '"a\nb"', pasteContext()));
    expect(payload.rows).toEqual([rowIdOf(0)]);
    expect(payload.text).toBe('"a\nb"');
  });

  it("テキストは 1 バイトも変えずに渡す（解釈はドメインが行う）", () => {
    const text = "a\tb\r\nc\n";
    expect(payloadOf(planPaste({ row: 0, column: 0 }, text, pasteContext())).text).toBe(text);
  });

  it("識別子が届いていない行があれば、送らずに理由を返す（部分的な対象を送らない）", () => {
    // 1 つでも引けなければ送らない（8.6 と同じ規律）。部分的な並びを送ると、ドメインは
    // 残りを「補充」として末尾へ足し、**利用者が見ている位置とは別のところへ書く**。
    const plan = planPaste({ row: 2, column: 0 }, "a\nb\nc", pasteContext({ rowId: rowIdsFor(0, 3) }));

    expect(plan.kind).toBe("refused");
    if (plan.kind !== "refused") {
      throw new Error("拒否の腕ではなかった");
    }
    expect(plan.message).toContain("識別子");
  });

  it("錨そのものの識別子が引けなければ、送らない（推測で書かない）", () => {
    const plan = planPaste({ row: 40, column: 0 }, "a", pasteContext());

    expect(plan.kind).toBe("refused");
  });

  it("錨の列が引けなければ、送らない", () => {
    const plan = planPaste(
      { row: 0, column: 0 },
      "a",
      pasteContext({ documentColumn: () => null }),
    );

    expect(plan.kind).toBe("refused");
  });

  it("空のテキストでは、境界へ 1 つも送らない（書くものが無い）", () => {
    // ドメインも空のテキストでは何も書かない（`PasteCodec::parse` が行 0 件を返す）。**往復を
    // 1 つも起こさない** — 起こせば、書くものが無いのに未保存の印が立ちうる。
    expect(planPaste({ row: 0, column: 0 }, "", pasteContext())).toEqual({ kind: "nothing" });
  });

  it("表の外を指す錨では、送らない", () => {
    expect(planPaste({ row: 20, column: 0 }, "a", pasteContext({ visibleRows: 20 }))).toEqual({
      kind: "refused",
      message: expect.stringContaining("表"),
    });
  });
});

// ===========================================================================
// 4. 適用と、行数が変わったあとの記憶（要件 7.3、7.4、1.7）
// ===========================================================================

describe("貼り付けの 1 往復（要件 7.3、7.4、1.7）", () => {
  const payload: PastePayload = {
    anchor: { row: rowIdOf(2), column: 1 },
    rows: [rowIdOf(2)],
    text: "a",
  };

  it("`PasteRange` を 1 つ送る（形は生成物の `GridEditCommand` そのものである）", async () => {
    const client = fakeClient(applied(outcomeOf({ affected: [rowIdOf(2)] })));
    const memory = fakeClear();

    await applyPaste({ client, cache: memory.cache, payload });

    const expected: GridEditCommand = {
      command: "PasteRange",
      anchor: { row: rowIdOf(2), column: 1 },
      rows: [rowIdOf(2)],
      text: "a",
    };
    expect(client.edits).toEqual([expected]);
  });

  it("行が増えたとき、増えた行を要求して読める（行数を渡さなければ永久に読み込み中である）", async () => {
    const client = fakeClient(
      applied(outcomeOf({ affected: [rowIdOf(0)], row_count: 22 })),
    );
    const memory = fakeClear();

    const settlement = await applyPaste({ client, cache: memory.cache, payload });

    expect(memory.cleared).toEqual([22]);
    expect(settlement).toEqual({
      status: "applied",
      outcome: expect.objectContaining({ row_count: 22 }) as GridEditOutcome,
      // **世代も応答が運ぶ**（タスク 10.1。画面は数え直さない）。
      generation: "2",
    });
  });

  it("何も変わらなかった適用（影響を受けた行が無い）では、記憶を捨てない", async () => {
    const client = fakeClient(applied(outcomeOf({ affected: [] })));
    const memory = fakeClear();

    await applyPaste({ client, cache: memory.cache, payload });

    expect(memory.cleared).toEqual([]);
  });

  it("適用できなかったときは、記憶を捨てずに理由を 1 行へ写す", async () => {
    const client = fakeClient({ status: "error", error: FAILURE });
    const memory = fakeClear();

    const settlement = await applyPaste({ client, cache: memory.cache, payload });

    expect(memory.cleared).toEqual([]);
    expect(settlement.status).toBe("failed");
    if (settlement.status !== "failed") {
      throw new Error("失敗の腕ではなかった");
    }
    expect(settlement.message).toContain("経路が不達");
  });
});

// ===========================================================================
// 5. 計画の腕の振り分け（**送る腕だけが境界へ行く**）
// ===========================================================================

describe("計画の腕は 1 箇所で振り分けられる（`runPastePlan`）", () => {
  /** 3 つの腕を、記録するだけの受け口へ渡す（**どの腕へ届いたか**を読む）。 */
  function dispatch(plan: PastePlan): Promise<{ reached: readonly string[] }> {
    const reached: string[] = [];
    return runPastePlan(plan, {
      send: async (payload) => {
        reached.push(`send:${payload.anchor.row}`);
      },
      refuse: (message) => {
        reached.push(`refuse:${message}`);
        return Promise.reject(new Error(message));
      },
    }).then(() => ({ reached }));
  }

  it("送る腕は境界へ行き、拒否の腕は理由を上げてから拒む", async () => {
    const payload = payloadOf(planPaste({ row: 0, column: 0 }, "a", pasteContext()));
    expect(await dispatch({ kind: "send", payload })).toEqual({
      reached: [`send:${rowIdOf(0)}`],
    });

    // **拒否の腕は送らない**（理由が告知へ上がり、`Promise` は拒む）。
    await expect(
      dispatch({ kind: "refused", message: "貼り付けの宛先の行の識別子がまだ届いていない" }),
    ).rejects.toThrow("識別子");
  });

  it("書くものが無い腕は、境界へ 1 つも送らない（成功として返る）", async () => {
    expect(await dispatch({ kind: "nothing" })).toEqual({ reached: [] });
  });
});

// ===========================================================================
// 6. 画面の面（移植口へ渡す 3 つの口。要件 7.1、7.3、7.4、8.6、8.9、1.7）
// ===========================================================================

/**
 * 偽の窓の記憶（4 つの能力だけ。**表示の列と文書の列を食い違わせられる**）。
 *
 * 既定は「値は `行:列`・識別子は可視の序数から・文書の列は表示の列の恒等・`clear` は記録する」
 * である。`documentColumn` をずらせるようにしてあるのは、**表示の位置と文書の位置の取り違え**
 * （要件 8.6、8.9 が名指しする誤り）を恒等な写像の下でも捕まえるためである。
 */
function fakeSurfaceCache(
  overrides: {
    readonly cells?: (position: CellPosition) => RenderCell;
    readonly documentColumn?: (position: CellPosition) => number | null;
  } = {},
): {
  readonly cache: Pick<WindowCache, "getCell" | "rowId" | "documentColumn" | "clear">;
  readonly cleared: readonly (number | undefined)[];
} {
  const cleared: (number | undefined)[] = [];
  return {
    cleared,
    cache: {
      getCell:
        overrides.cells ??
        ((position) => ({
          text: `${String(position.row)}:${String(position.column)}`,
          variant: "Text",
          violated: false,
          loading: false,
        })),
      rowId: rowIdsFor(0, 19),
      documentColumn: overrides.documentColumn ?? columnsShifted(0),
      clear: (rowCount?: number) => {
        cleared.push(rowCount);
      },
    },
  };
}

describe("画面の面は材料と行き先を繋ぐ（移植口へ渡す 3 つの口）", () => {
  it("貼り付けの宛先は表示の列ではなく文書の列である（要件 8.6、8.9）", () => {
    // 表示の列 0 が文書の列 1 である構成（入れ子の展開と同じ食い違い。要件 5.1）。
    // **恒等な写像で検査すると、この取り違えは緑のまま通る。**
    const { cache } = fakeSurfaceCache({ documentColumn: columnsShifted(1) });
    const surface = createClipboardSurface({
      client: fakeClient(applied(outcomeOf({}))),
      cache: () => cache,
      visibleRows: 20,
      onSettled: () => undefined,
      onApplied: () => undefined,
    });

    const payload = payloadOf(
      surface.pasteAt({ row: 2, column: 0 }, "1\t2\n3\t4"),
    );

    // 錨の列は**文書の列**である（表示の列 0 ではない）。
    expect(payload.anchor).toEqual({ row: rowIdOf(2), column: 1 });
    // 行は表示の並びの識別子である（矩形の行数ぶんだけ渡す）。
    expect(payload.rows).toEqual([rowIdOf(2), rowIdOf(3)]);
    // テキストは 1 バイトも変えない。
    expect(payload.text).toBe("1\t2\n3\t4");
  });

  it("送る腕は境界へ PasteRange を 1 つ送る（何もしない `sendPaste` にしない）", async () => {
    const { cache, cleared } = fakeSurfaceCache();
    const client = fakeClient(applied(outcomeOf({ affected: [rowIdOf(1)], row_count: 21 })));
    const surface = createClipboardSurface({
      client,
      cache: () => cache,
      visibleRows: 20,
      onSettled: () => undefined,
      onApplied: () => undefined,
    });

    await surface.sendPaste(payloadOf(surface.pasteAt({ row: 1, column: 2 }, "標本")));

    // **境界が受けた命令そのものを見る。**送らなければ空であり、この検査が落ちる。
    expect(client.edits).toEqual([
      {
        command: "PasteRange",
        anchor: { row: rowIdOf(1), column: 2 },
        rows: [rowIdOf(1)],
        text: "標本",
      },
    ]);
    // 行数が変わったので記憶を作り直す（要件 1.7。応答の行数をそのまま渡す）。
    expect(cleared).toEqual([21]);
  });

  it("往復の結果を画面へ渡し、適用されたときだけ後始末を呼ぶ（要件 7.3、4.6）", async () => {
    const settlements: string[] = [];
    let appliedCount = 0;
    const outcome = outcomeOf({ affected: [], row_count: 20 });
    const { cache } = fakeSurfaceCache();
    const surface = createClipboardSurface({
      client: fakeClient(applied(outcome)),
      cache: () => cache,
      visibleRows: 20,
      onSettled: (settlement) => {
        settlements.push(settlement.status);
      },
      onApplied: () => {
        appliedCount += 1;
      },
    });

    await surface.sendPaste(payloadOf(surface.pasteAt({ row: 0, column: 0 }, "a")));

    // **結果が画面へ届く**（届かなければ告知も提示も動かない）。
    expect(settlements).toEqual(["applied"]);
    // 適用されたので違反を引き直す（要件 4.6）。
    expect(appliedCount).toBe(1);

    // **失敗は理由を運び、後始末は呼ばない**（適用されていないので引き直す先が無い）。
    const failing = createClipboardSurface({
      client: fakeClient({ status: "error", error: FAILURE }),
      cache: () => cache,
      visibleRows: 20,
      onSettled: (settlement) => {
        settlements.push(settlement.status);
      },
      onApplied: () => {
        appliedCount += 1;
      },
    });
    await failing.sendPaste(payloadOf(failing.pasteAt({ row: 0, column: 0 }, "a")));

    expect(settlements).toEqual(["applied", "failed"]);
    expect(appliedCount).toBe(1);
  });

  it("器がまだ無いときは拒否の計画を返し、複製はそのまま窓の記憶を読む（要件 7.1）", () => {
    const empty = createClipboardSurface({
      client: fakeClient(applied(outcomeOf({}))),
      cache: () => null,
      visibleRows: 20,
      onSettled: () => undefined,
      onApplied: () => undefined,
    });

    // **投げない**（告知へ流すのは呼ぶ側である）。
    expect(empty.copyRange(rangeOf({ row: 0, column: 0 }, { row: 0, column: 0 })).kind).toBe(
      "refused",
    );
    expect(empty.pasteAt({ row: 0, column: 0 }, "a").kind).toBe("refused");

    // 値は**表示の並びのまま**読む（複製は行と列の配置を保つ。要件 7.1、7.2）。
    const { cache } = fakeSurfaceCache();
    const surface = createClipboardSurface({
      client: fakeClient(applied(outcomeOf({}))),
      cache: () => cache,
      visibleRows: 20,
      onSettled: () => undefined,
      onApplied: () => undefined,
    });
    const plan = surface.copyRange(rangeOf({ row: 0, column: 0 }, { row: 1, column: 1 }));
    expect(plan).toEqual({ kind: "text", text: "0:0\t0:1\n1:0\t1:1" });
  });
});

/**
 * マクロの実行の面の状態と提示（tasks.md 4.4。要件 1.3、1.4、2.3、2.4、2.5、2.7、6.1、6.2、
 * 8.2、9.1、9.2、9.3）。
 *
 * # 何を固定するか
 *
 * 1. **一覧は解釈できなかった 1 件も理由つきで残る**（要件 1.4）— 種別と名前は載り、
 *    `failure` が層・理由・フレームを運ぶ
 * 2. **実行できる 1 件は解釈できたものだけ**であり、**1 つも無ければ実行の導線を出さない**
 *    （要件 2.7）
 * 3. **能力の提示へ移れるのは実行できる 1 件だけ**である（要件 8.2。解釈できない 1 件を
 *    選ぶ操作は作らない）
 * 4. **失敗の 3 値（成功・失敗・打ち切り）と経路の失敗が、それぞれ固有の提示になる** —
 *    失敗は層と理由とフレーム、打ち切りは**種類**と理由とフレーム（要件 2.4、6.1、6.2、9.1、
 *    9.2、9.3）
 * 5. **変更の件数は種別ごとに出す**（要件 2.5）。0 件は「変更はありません」と言い切る
 * 6. **実行中は次の実行の導線を出さない**（要件 2.7 と同じ判断。2 つ目の実行は Rust 側も
 *    「実行中である」として断る）
 *
 * ここは**値の層**である（`Promise` も DOM も持たない）。4 つの流れの**結線**は
 * `./store.test.ts` と `./MacroPanel.test.tsx` が固定する。
 */
import { describe, expect, it } from "vitest";

import type {
  MacroChangeCounts,
  MacroFailureReport,
  MacroOutputLine,
  MacroRunOutcome,
  MacroSummary,
} from "../../ipc/bindings";
import {
  canPresentRun,
  changeTotal,
  chosenSummary,
  describeAbortLimit,
  describeChanges,
  describeFailureLayer,
  describeFrame,
  describeKind,
  describeOutputLine,
  failurePresentation,
  initialMacroSurfaceState,
  macroSurfaceChosen,
  macroSurfaceLoadFailed,
  macroSurfaceLoaded,
  macroSurfacePickRequested,
  macroSurfaceRunSettled,
  macroSurfaceRunStarted,
  resultPresentation,
  runnableMacros,
  type MacroSurfaceState,
} from "./surface";

/** 変更の件数（既定はすべて 0 件）。 */
function counts(overrides: Partial<MacroChangeCounts> = {}): MacroChangeCounts {
  return { set_cells: 0, inserted_rows: 0, removed_rows: 0, duplicated_rows: 0, ...overrides };
}

/** 解釈できた 1 件。 */
function summary(overrides: Partial<MacroSummary> = {}): MacroSummary {
  return { name: "棚卸し", kind: "typescript", capabilities: [], failure: null, ...overrides };
}

/** 解釈できなかった 1 件（能力宣言の綴りが誤っている、等）。 */
function broken(name = "壊れたマクロ"): MacroSummary {
  return {
    name,
    kind: "javascript",
    capabilities: [],
    failure: {
      kind: { kind: "source" },
      reason: "能力の宣言を解釈できない: `file.raed`",
      frames: [],
    },
  };
}

/** 一覧が読めた状態。 */
function loaded(macros: readonly MacroSummary[]): MacroSurfaceState {
  return macroSurfaceLoaded(initialMacroSurfaceState(), macros);
}

/** 実行が成功した結果を作る。 */
function ran(
  value: string,
  changes: MacroChangeCounts = counts(),
  output: MacroOutputLine[] = [],
): MacroRunOutcome {
  // **出力の並びは可変の配列である**（境界の型 `MacroOutputLine[]` のまま渡す）。
  return { outcome: "Ran", value, output, changes, elapsed_ms: 12 };
}

describe("一覧の提示（要件 1.3、1.4）", () => {
  it("解釈できなかった 1 件も一覧に残り、理由とフレームを運ぶ（要件 1.4）", () => {
    const state = loaded([
      summary({ name: "集計", kind: "typescript", capabilities: ["file.read"] }),
      {
        ...broken(),
        failure: {
          kind: { kind: "transpile" },
          reason: "予期しないトークン",
          frames: [{ macro_name: "壊れたマクロ", function: "", line: 3, column: 7 }],
        },
      },
    ]);

    // **どちらも一覧に載る**（実行できるかは `failure` の有無で分かる）。
    expect(state.list.status).toBe("ready");
    if (state.list.status !== "ready") {
      throw new Error("一覧が読めた状態にならなかった");
    }
    expect(state.list.macros.map((macro) => macro.name)).toEqual(["集計", "壊れたマクロ"]);
    expect(runnableMacros(state.list.macros).map((macro) => macro.name)).toEqual(["集計"]);

    // 種別の見出しは境界の札から組む（綴りを画面に書かない）。
    expect(describeKind("typescript")).toBe("TypeScript");
    expect(describeKind("javascript")).toBe("JavaScript");
  });

  it("一覧が空なら実行の導線を出さない（要件 2.7）", () => {
    expect(canPresentRun(loaded([]))).toBe(false);
    // **解釈できる 1 件が 1 つも無い場合も同じである**（実行できるものが無い）。
    expect(canPresentRun(loaded([broken()]))).toBe(false);
    expect(canPresentRun(loaded([summary()]))).toBe(true);
  });

  it("読めなかった一覧では実行の導線を出さない", () => {
    const state = macroSurfaceLoadFailed(initialMacroSurfaceState(), "文書がありません");
    expect(state.list.status).toBe("failed");
    expect(canPresentRun(state)).toBe(false);
    // 読み込みの途中でも同じである（**押しても何も起きない状態を作らない**。要件 2.7）。
    expect(canPresentRun(initialMacroSurfaceState())).toBe(false);
  });
});

describe("能力の提示（要件 8.2）", () => {
  it("実行できる 1 件だけを選べる（解釈できない 1 件は選べない）", () => {
    const state = macroSurfacePickRequested(loaded([summary({ name: "集計" }), broken()]));

    // 選べるのは解釈できた 1 件だけである。
    expect(macroSurfaceChosen(state, "壊れたマクロ").chosen).toBeNull();
    const chosen = macroSurfaceChosen(state, "集計");
    expect(chosen.chosen).toBe("集計");
    // **一覧から引く**（宣言は一覧の値そのものである。写しを作らない）。
    expect(chosenSummary(chosen)?.name).toBe("集計");
    // 選ぶと「選ばせる段」から降りる（要求に対する選択が済んだ）。
    expect(chosen.picking).toBe(false);
  });

  it("知らない名前は選べない（一覧が取り直された直後の押下）", () => {
    const state = macroSurfacePickRequested(loaded([summary({ name: "集計" })]));
    expect(macroSurfaceChosen(state, "別のマクロ").chosen).toBeNull();
  });

  it("宣言している能力は、選ばれた 1 件の値そのものである", () => {
    const state = macroSurfaceChosen(
      loaded([summary({ capabilities: ["file.read", "net"] })]),
      "棚卸し",
    );
    expect(chosenSummary(state)?.capabilities).toEqual(["file.read", "net"]);
  });

  it("実行中は次の実行の導線を出さない（要件 2.7 と同じ判断）", () => {
    const state = macroSurfaceChosen(loaded([summary()]), "棚卸し");
    expect(canPresentRun(state)).toBe(true);
    const running = macroSurfaceRunStarted(state, "棚卸し");
    expect(running.running).toEqual({ name: "棚卸し" });
    // **選ばれていた 1 件は落ちる**（能力の提示は実行の前の段である）。
    expect(running.chosen).toBeNull();
    expect(canPresentRun(running)).toBe(false);
  });
});

describe("失敗の提示（要件 2.4、9.1、9.2、9.3）", () => {
  it("失敗は層・理由・フレームを運び、内側から外側へ並ぶ（要件 9.3）", () => {
    const state = macroSurfaceRunSettled(
      macroSurfaceRunStarted(loaded([summary()]), "棚卸し"),
      "棚卸し",
      resultPresentation({
        outcome: "Failed",
        failure: {
          kind: { kind: "host_rejected", api: "host.readRows" },
          reason: "能力 `file.read` を宣言していない",
          frames: [
            { macro_name: "棚卸し", function: "集計", line: 12, column: 5 },
            { macro_name: "棚卸し", function: "", line: 3, column: 1 },
          ],
        },
      }),
    );

    expect(state.running).toBeNull();
    expect(state.result?.name).toBe("棚卸し");
    const result = state.result?.result;
    if (result?.kind !== "failed") {
      throw new Error("失敗の提示にならなかった");
    }
    // **拒んだ API の名前を落とさない**（要件 9.2）。
    expect(result.failure.layer).toEqual({ kind: "host_rejected", api: "host.readRows" });
    expect(describeFailureLayer(result.failure.layer)).toBe(
      "ホスト API が拒否した（host.readRows）",
    );
    expect(result.failure.reason).toBe("能力 `file.read` を宣言していない");
    // 並びは境界のまま（内側から外側へ）。行と列は**1 起点の原位置**である（要件 9.1）。
    expect(result.failure.frames.map(describeFrame)).toEqual([
      "マクロ「棚卸し」の 12 行 5 列目（集計）",
      "マクロ「棚卸し」の 3 行 1 列目",
    ]);
  });

  it("ホスト API の拒否でない失敗は API の名前を持たない", () => {
    const failure: MacroFailureReport = {
      kind: { kind: "execution" },
      reason: "TypeError: x is not a function",
      frames: [],
    };
    expect(failurePresentation(failure).layer).toEqual({ kind: "execution" });
    expect(describeFailureLayer(failurePresentation(failure).layer)).toBe(
      "実行の途中で失敗した",
    );
  });
});

describe("打ち切りの提示（要件 6.1、6.2）", () => {
  it("打ち切りは失敗と別の値であり、種類を落とさない", () => {
    const time = resultPresentation({
      outcome: "Aborted",
      limit: "time",
      elapsed_ms: 30000,
      failure: { kind: { kind: "execution" }, reason: "実行が終わらない", frames: [] },
    });
    const memory = resultPresentation({
      outcome: "Aborted",
      limit: "memory",
      elapsed_ms: 4200,
      failure: { kind: { kind: "execution" }, reason: "メモリの上限に達した", frames: [] },
    });

    // **別の値である**（種類が判別子として載る）。
    expect(time.kind).toBe("aborted");
    expect(memory.kind).toBe("aborted");
    if (time.kind !== "aborted" || memory.kind !== "aborted") {
      throw new Error("打ち切りの提示にならなかった");
    }
    expect(time.limit).toBe("time");
    expect(memory.limit).toBe("memory");
    // 種類は**提示の文言でも区別できる**（どちらの上限に当たったかを言う）。
    expect(describeAbortLimit(time.limit)).toBe("時間の上限");
    expect(describeAbortLimit(memory.limit)).toBe("メモリの上限");
    expect(time.elapsedMs).toBe(30000);
    expect(time.failure.reason).toBe("実行が終わらない");
  });
});

describe("成功の提示（要件 2.3、2.5）", () => {
  it("戻り値・出力・変更の件数・所要を運び、変更の有無を数える", () => {
    const result = resultPresentation(
      ran("42", counts({ set_cells: 3, inserted_rows: 1 }), [
        { level: "log", text: "開始" },
        { level: "warn", text: "空の行を飛ばした" },
      ]),
    );

    if (result.kind !== "ran") {
      throw new Error("成功の提示にならなかった");
    }
    expect(result.value).toBe("42");
    // 出力は**順序を保ち、種別を落とさない**（要件 2.3）。
    expect(result.output.map(describeOutputLine)).toEqual([
      "[log] 開始",
      "[warn] 空の行を飛ばした",
    ]);
    expect(changeTotal(result.changes)).toBe(4);
    expect(result.changed).toBe(true);
    expect(describeChanges(result.changes)).toBe(
      "変更 4 件（セル 3 / 追加 1 / 削除 0 / 複製 0）",
    );
    expect(result.elapsedMs).toBe(12);
  });

  it("変更 0 件は「変更はありません」であり、開き直しの対象にしない", () => {
    const result = resultPresentation(ran("undefined"));
    if (result.kind !== "ran") {
      throw new Error("成功の提示にならなかった");
    }
    expect(result.changed).toBe(false);
    expect(describeChanges(result.changes)).toBe("変更はありません");
  });
});

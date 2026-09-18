/**
 * 検証専用のマクロの観測の 1 行（`macro-runtime` スペックの tasks.md 5.1）の検査。
 *
 * # 何を固定するか
 *
 * [`observationOf`] が組む 1 行は、**5.2 の検査器が読む契約**である。検査器は
 * `scripts/check-macro-observation.sh` として後から置かれ、診断の記録の
 * `マクロの観測: {…}` を解釈する。したがってここで固定するのは**値の層**である —
 * 結果の 3 値（`ran` / `failed` / `aborted`）と経路の失敗（`rejected`）が、
 * **それぞれ固有の欄**（変更の件数・失敗の層と理由とフレーム・打ち切りの種類）を持つこと。
 *
 * **通しの駆動（グリッドの出現待ち・選択・実行・記録への送信）は実起動で観測する** —
 * `node` の環境には DOM もタイマーの進行も無く、5.1 の受け入れそのものが実起動の観測である
 * （`src/features/grid/gridObservation.tsx` が 9.2 で同じ分担を記録している）。
 */
import { describe, expect, it } from "vitest";

import type { MacroFailureReport, MacroSummary } from "../ipc/bindings";
import { observationOf } from "./verificationMacroRun";

/** 標本のマクロ 1 件（標本の文書はこれを 1 件だけ持つ）。 */
const SUMMARY: MacroSummary = {
  name: "標本の記入",
  kind: "typescript",
  capabilities: [],
  failure: null,
};

/** 一覧（保存順）。 */
const LIST: readonly MacroSummary[] = [SUMMARY];

/** 失敗の 1 件（実行の例外。フレームが 1 段ある）。 */
const FAILURE: MacroFailureReport = {
  kind: { kind: "execution" },
  reason: "TypeError: 未定義の値を読んだ",
  frames: [{ macro_name: "標本の記入", function: "main", line: 4, column: 5 }],
};

describe("マクロの観測の 1 行（5.1）", () => {
  it("成功は変更の件数と所要を持ち、失敗の欄はすべて空である", () => {
    const observation = observationOf("標本の記入", LIST, SUMMARY, {
      kind: "ran",
      value: '"りんご"',
      output: [],
      changes: { set_cells: 1, inserted_rows: 0, removed_rows: 0, duplicated_rows: 0 },
      changed: true,
      elapsedMs: 12,
    });
    expect(observation.outcome).toBe("ran");
    expect(observation.changes).toEqual({
      set_cells: 1,
      inserted_rows: 0,
      removed_rows: 0,
      duplicated_rows: 0,
    });
    expect(observation.elapsedMs).toBe(12);
    // **「無い」ことも書く**（検査器が「欠けている」と「空である」を区別できるようにする）。
    expect(observation.limit).toBeNull();
    expect(observation.layer).toBeNull();
    expect(observation.reason).toBeNull();
    expect(observation.frames).toEqual([]);
    // 一覧に現れた事実（仕込みの材料）と、選択・能力も同じ行に載る。
    expect(observation.listed).toBe(1);
    expect(observation.names).toEqual(["標本の記入"]);
    expect(observation.chosen).toBe("標本の記入");
    expect(observation.capabilities).toEqual([]);
  });

  it("失敗は層・理由・フレームを持ち、変更の件数を持たない", () => {
    const observation = observationOf("標本の記入", LIST, SUMMARY, {
      kind: "failed",
      failure: { layer: { kind: "execution" }, reason: FAILURE.reason, frames: FAILURE.frames },
    });
    expect(observation.outcome).toBe("failed");
    expect(observation.layer).toBe("execution");
    expect(observation.reason).toBe(FAILURE.reason);
    expect(observation.frames).toEqual(FAILURE.frames);
    expect(observation.changes).toBeNull();
    expect(observation.elapsedMs).toBeNull();
  });

  it("能力の拒否は拒んだ API の名前を層に載せる（要件 9.2）", () => {
    const observation = observationOf("標本の記入", LIST, SUMMARY, {
      kind: "failed",
      failure: {
        layer: { kind: "host_rejected", api: "host.netFetch" },
        reason: "能力 net を宣言していない",
        frames: [],
      },
    });
    expect(observation.layer).toBe("host_rejected:host.netFetch");
  });

  it("打ち切りは種類と所要を持ち、失敗と同じ欄に理由とフレームが入る", () => {
    const observation = observationOf("標本の記入", LIST, SUMMARY, {
      kind: "aborted",
      limit: "time",
      elapsedMs: 30_001,
      failure: { layer: { kind: "execution" }, reason: "実行を打ち切った", frames: [] },
    });
    expect(observation.outcome).toBe("aborted");
    expect(observation.limit).toBe("time");
    expect(observation.elapsedMs).toBe(30_001);
    expect(observation.reason).toBe("実行を打ち切った");
    // **変更は 1 件も適用されていない**（要件 6.3）。件数の欄は空のままである。
    expect(observation.changes).toBeNull();
  });

  it("実行そのものが始まらなかった経路の失敗は理由だけを運ぶ", () => {
    const observation = observationOf("標本の記入", LIST, SUMMARY, {
      kind: "rejected",
      message: "グリッドがまだ開かれていない（変更の適用先のシートを決められない）",
    });
    expect(observation.outcome).toBe("rejected");
    expect(observation.reason).toBe(
      "グリッドがまだ開かれていない（変更の適用先のシートを決められない）",
    );
    expect(observation.layer).toBeNull();
    expect(observation.frames).toEqual([]);
  });
});

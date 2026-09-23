/**
 * 実行の面のパネルの見た目（tasks.md 4.4。要件 1.3、1.4、2.1、2.3、2.4、2.5、2.7、8.2、9.1、
 * 9.2、9.3）。
 *
 * # 何を固定するか（**画面が実際に DOM へ出すもの**を読む）
 *
 * 1. **一覧は開いた時点で出る**（要件 1.3）— 名前・種別・能力。**解釈できなかった 1 件も残り**、
 *    層・理由・フレームが出る（要件 1.4）
 * 2. **能力の提示**（要件 8.2）— 選ばれた 1 件の宣言を実行の前に並べ、実行の操作を出す
 * 3. **結果は 1 つの面に 4 つの固有の形で出る**（要件 2.3、2.4、2.5、6.1、6.2）— 成功・失敗・
 *    打ち切り・経路の失敗がそれぞれ別の区画であり、失敗と打ち切りは**区別できる**
 * 4. **実行中も表と一覧は止まらない**（要件 2.2）— 実行中に足されるのは「実行中」の 1 行だけで
 *    あり、覆いも対話の窓も出ない（`aria-modal` も `role="dialog"` も無い）。一覧はそのまま
 *    残り、操作できる
 * 5. **実行できる 1 件が 1 つも無ければ導線を出さない**（要件 2.7）— 空の一覧でも、解釈できない
 *    1 件だけでも、選ぶ操作も実行の操作も出ない
 *
 * 描画は `renderToStaticMarkup` である（`node` 環境に DOM は無い。`src/features/grid` の
 * 画面の検査と同じ規律）。**効果は走らない**ので、ここに来るのは状態だけである —
 * 状態を作る結線は `./store.test.ts`、状態と境界の写しは `./surface.test.ts` が固定する。
 */
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { MacroPanelView, type MacroPanelViewProps } from "./MacroPanel";
import {
  failurePresentation,
  initialMacroSurfaceState,
  macroSurfaceChosen,
  macroSurfaceLoadFailed,
  macroSurfaceLoaded,
  macroSurfacePickRequested,
  macroSurfaceResultDismissed,
  macroSurfaceRunSettled,
  macroSurfaceRunStarted,
  type MacroResultPresentation,
  type MacroSurfaceState,
} from "./surface";
import type { MacroFailureReport, MacroSummary } from "../../ipc/bindings";

/** 解釈できた 1 件。 */
function summary(overrides: Partial<MacroSummary> = {}): MacroSummary {
  return { name: "棚卸し", kind: "typescript", capabilities: [], failure: null, ...overrides };
}

/** 解釈できなかった 1 件。 */
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

/** 面の状態 1 つぶんのマーク付け（**画面が実際に DOM へ出すもの**を読む）。 */
function markOf(state: MacroSurfaceState): string {
  const props: MacroPanelViewProps = {
    state,
    dispatch: () => undefined,
  };
  return renderToStaticMarkup(createElement(MacroPanelView, props));
}

/** 一覧が読めた状態。 */
function loaded(macros: readonly MacroSummary[]): MacroSurfaceState {
  return macroSurfaceLoaded(initialMacroSurfaceState(), macros);
}

/** 選ばれた 1 件の能力を提示している状態（**遷移そのものを使って組む**）。 */
function chosen(macro: MacroSummary): MacroSurfaceState {
  return macroSurfaceChosen(
    macroSurfaceLoaded(initialMacroSurfaceState(), [macro]),
    macro.name,
  );
}

/** 実行中の状態。 */
function running(macro: MacroSummary): MacroSurfaceState {
  return macroSurfaceRunStarted(chosen(macro), macro.name);
}

/** 実行が終わった状態。 */
function settled(macro: MacroSummary, result: MacroResultPresentation): MacroSurfaceState {
  return macroSurfaceRunSettled(running(macro), macro.name, result);
}

describe("一覧の提示（要件 1.3、1.4）", () => {
  it("開いた時点で名前・種別・能力を出し、解釈できない 1 件も理由つきで残す", () => {
    const markup = markOf(
      loaded([
        summary({ name: "集計", kind: "typescript", capabilities: ["file.read", "net"] }),
        {
          ...broken(),
          failure: {
            kind: { kind: "transpile" },
            reason: "予期しないトークン",
            frames: [{ macro_name: "壊れたマクロ", function: "main", line: 3, column: 7 }],
          },
        },
      ]),
    );

    // 解釈できた 1 件。
    expect(markup).toContain('data-macro-name="集計"');
    expect(markup).toContain('data-macro-kind="typescript"');
    expect(markup).toContain("TypeScript");
    expect(markup).toContain('data-macro-runnable="true"');
    expect(markup).toContain('data-macro-capabilities="file.read net"');
    expect(markup).toContain("能力: file.read, net");

    // **解釈できなかった 1 件も一覧に残る**（要件 1.4）。
    expect(markup).toContain('data-macro-name="壊れたマクロ"');
    expect(markup).toContain('data-macro-kind="javascript"');
    expect(markup).toContain('data-macro-runnable="false"');
    expect(markup).toContain("jxcel-macro-row-uninterpretable");
    expect(markup).toContain('data-macro-failure-layer="transpile"');
    expect(markup).toContain("変換できない");
    expect(markup).toContain("予期しないトークン");
    // フレームは**1 起点の原位置**で名乗る（要件 9.1）。
    expect(markup).toContain('data-macro-frame-line="3"');
    expect(markup).toContain('data-macro-frame-column="7"');
    expect(markup).toContain("マクロ「壊れたマクロ」の 3 行 7 列目（main）");
  });

  it("読み込みの途中・読めなかった・1 件も無い、をそれぞれ固有に出す", () => {
    expect(markOf(initialMacroSurfaceState())).toContain("jxcel-macro-list-loading");
    expect(markOf(initialMacroSurfaceState())).toContain('data-macro-list="loading"');
    // 読み込みの途中では**実行の導線も選択も出ない**（押しても何も起きない状態を作らない）。
    expect(markOf(initialMacroSurfaceState())).not.toContain('data-testid="jxcel-macro-run"');

    const failed = markOf(macroSurfaceLoadFailed(initialMacroSurfaceState(), "文書がありません"));
    expect(failed).toContain("jxcel-macro-list-failure");
    expect(failed).toContain("マクロの一覧を読み込めませんでした");
    expect(failed).toContain("文書がありません");
    expect(failed).toContain('data-testid="jxcel-macro-list-retry"');

    const empty = markOf(loaded([]));
    expect(empty).toContain("jxcel-macro-empty");
    expect(empty).toContain("この文書にはマクロがありません");
    expect(empty).not.toContain("jxcel-macro-list");
  });

  it("能力の宣言が無い 1 件は「宣言なし」と出る（実行できる）", () => {
    const markup = markOf(loaded([summary()]));
    expect(markup).toContain("能力の宣言なし");
    expect(markup).toContain('data-macro-capabilities=""');
    expect(markup).toContain('data-macro-runnable="true"');
  });
});

describe("能力の提示（要件 8.2）", () => {
  it("選ばれた 1 件の宣言している能力を、実行の前に並べる", () => {
    const markup = markOf(chosen(summary({ capabilities: ["file.read", "file.write", "net"] })));

    expect(markup).toContain("jxcel-macro-capabilities");
    expect(markup).toContain('data-macro-name="棚卸し"');
    expect(markup).toContain("実行する前に、宣言している能力を確認してください");
    // **綴りは境界の値そのままである**（面は言い換えを作らない）。
    expect(markup).toContain('data-macro-capability="file.read"');
    expect(markup).toContain('data-macro-capability="file.write"');
    expect(markup).toContain('data-macro-capability="net"');
    // 実行の操作と取り消しの操作が出る。
    expect(markup).toContain('data-testid="jxcel-macro-run"');
    expect(markup).toContain("jxcel-macro-cancel");
    // **まだ実行していない**（実行中ではない）。
    expect(markup).toContain('data-macro-running="false"');
  });

  it("宣言が無い 1 件は、触らないことを明示して実行を出す", () => {
    const markup = markOf(chosen(summary()));
    expect(markup).toContain("jxcel-macro-no-capability");
    expect(markup).toContain("宣言している能力はありません");
    expect(markup).not.toContain("jxcel-macro-capability-list");
    expect(markup).toContain('data-testid="jxcel-macro-run"');
  });

  it("実行の入口はメニューの要求であり、選ばせる段でだけ「選ぶ」が出る（要件 2.1）", () => {
    const macros = [summary({ name: "集計" }), broken()];
    // 要求の前は一覧だけである（**選ぶ操作は出ない**）。
    const idle = markOf(loaded(macros));
    expect(idle).not.toContain('data-testid="jxcel-macro-choose"');

    const picking = markOf(macroSurfacePickRequested(loaded(macros)));
    expect(picking).toContain('data-testid="jxcel-macro-choose"');
    // **選べるのは実行できる 1 件だけである**（解釈できない 1 件には出ない）。
    const choosable = [...picking.matchAll(/data-testid="jxcel-macro-choose" data-macro-name="([^"]*)"/g)].map(
      (match) => match[1],
    );
    expect(choosable).toEqual(["集計"]);
  });

  it("実行できる 1 件が 1 つも無ければ、要求が来ても導線を出さない（要件 2.7）", () => {
    // 空の一覧。
    const empty = markOf(macroSurfacePickRequested(loaded([])));
    expect(empty).not.toContain('data-testid="jxcel-macro-choose"');
    expect(empty).not.toContain('data-testid="jxcel-macro-run"');
    expect(empty).toContain("jxcel-macro-empty");

    // 解釈できない 1 件だけ（**一覧には理由つきで残る**が、実行の導線は出ない）。
    const noneRunnable = markOf(macroSurfacePickRequested(loaded([broken()])));
    expect(noneRunnable).toContain("jxcel-macro-none-runnable");
    expect(noneRunnable).toContain("実行できるマクロがありません");
    expect(noneRunnable).toContain("jxcel-macro-row-uninterpretable");
    expect(noneRunnable).not.toContain('data-testid="jxcel-macro-choose"');
    expect(noneRunnable).not.toContain('data-testid="jxcel-macro-run"');
  });
});

describe("実行中（要件 2.2）", () => {
  it("実行中を示し、覆いも対話の窓も出さない（表と一覧は止まらない）", () => {
    const markup = markOf(running(summary({ name: "棚卸し" })));

    // **実行中であることが外から読める**。
    expect(markup).toContain('data-macro-running="true"');
    expect(markup).toContain("jxcel-macro-running");
    expect(markup).toContain("実行中: 棚卸し");
    // 実行中は**次の実行の導線を出さない**（2 つ目の実行は Rust 側も断る）。
    expect(markup).not.toContain('data-testid="jxcel-macro-run"');
    // **一覧はそのまま残る**（実行が一覧や表を置き換えない）。
    expect(markup).toContain("jxcel-macro-list");
    expect(markup).toContain('data-macro-name="棚卸し"');
    // **覆いも対話の窓も無い**（`src/features/grid` の表と同じ画面に並ぶバーである）。
    expect(markup).not.toContain("aria-modal");
    expect(markup).not.toContain('role="dialog"');
    expect(markup).not.toContain("backdrop");
    // 実行中でも一覧の操作は無効化されない（`disabled` を出さない）。
    expect(markup).not.toContain("disabled");
  });

  it("実行中に選び直しても、次の実行の導線は出ない（一覧は読める）", () => {
    const state = running(summary({ name: "棚卸し" }));
    const swapped = macroSurfaceChosen(macroSurfacePickRequested(state), "棚卸し");
    const markup = markOf(swapped);

    expect(markup).toContain("jxcel-macro-running");
    // 能力の提示は出る（**一覧を読める**）が、実行の操作は出ず、その理由が出る。
    expect(markup).toContain("jxcel-macro-capabilities");
    expect(markup).toContain("jxcel-macro-run-blocked");
    expect(markup).toContain("実行中です。終わってから実行できます。");
    expect(markup).not.toContain('data-testid="jxcel-macro-run"');
  });
});

describe("結果の提示（要件 2.3、2.4、2.5、6.1、6.2、9.1、9.2、9.3）", () => {
  it("成功は戻り値・出力・変更の件数・所要を 1 つの区画に出す（要件 2.3、2.5）", () => {
    const markup = markOf(
      settled(summary(), {
        kind: "ran",
        value: "42",
        output: [
          { level: "log", text: "開始" },
          { level: "error", text: "空の行を飛ばした" },
        ],
        changes: { set_cells: 3, inserted_rows: 1, removed_rows: 0, duplicated_rows: 0 },
        changed: true,
        elapsedMs: 12,
      }),
    );

    expect(markup).toContain("jxcel-macro-result-ran");
    expect(markup).toContain('data-macro-changed="true"');
    expect(markup).toContain("実行が終わりました: 棚卸し");
    expect(markup).toContain("戻り値: 42");
    // 出力は**順序を保ち、種別を落とさない**。
    expect(markup).toContain('data-macro-output-level="log"');
    expect(markup).toContain("[log] 開始");
    expect(markup).toContain('data-macro-output-level="error"');
    expect(markup).toContain("[error] 空の行を飛ばした");
    expect(markup).toContain('data-macro-changes-total="4"');
    expect(markup).toContain("変更 4 件（セル 3 / 追加 1 / 削除 0 / 複製 0）");
    expect(markup).toContain("所要 12 ms");
    expect(markup).toContain('data-testid="jxcel-macro-result-dismiss"');
    // 成功の腕に失敗や打ち切りの提示は出ない。
    expect(markup).not.toContain("jxcel-macro-result-failed");
    expect(markup).not.toContain("jxcel-macro-result-aborted");
  });

  it("失敗は層・理由・フレームを出す（要件 2.4、9.1、9.2、9.3）", () => {
    const failure: MacroFailureReport = {
      kind: { kind: "host_rejected", api: "host.readRows" },
      reason: "能力 `file.read` を宣言していない",
      frames: [
        { macro_name: "棚卸し", function: "集計", line: 12, column: 5 },
        { macro_name: "棚卸し", function: "", line: 3, column: 1 },
      ],
    };
    const markup = markOf(
      settled(summary(), { kind: "failed", failure: failurePresentation(failure) }),
    );

    expect(markup).toContain("jxcel-macro-result-failed");
    expect(markup).toContain("実行が失敗しました: 棚卸し");
    // **拒んだ API の名前を落とさない**（要件 9.2）。
    expect(markup).toContain('data-macro-failure-layer="host_rejected"');
    expect(markup).toContain('data-macro-failure-api="host.readRows"');
    expect(markup).toContain("ホスト API が拒否した（host.readRows）");
    expect(markup).toContain("能力 `file.read` を宣言していない");
    // 呼び出しの並びは**内側から外側へ**（要件 9.3）。
    const frames = [...markup.matchAll(/data-macro-frame-line="(\d+)"/g)].map((match) => match[1]);
    expect(frames).toEqual(["12", "3"]);
    expect(markup).toContain('data-macro-frame-function=""');
    // **打ち切りの提示は出ない**（別の値である）。
    expect(markup).not.toContain("jxcel-macro-result-aborted");
    expect(markup).not.toContain("jxcel-macro-abort-limit");
  });

  it("打ち切りは種類・理由・フレームを出し、失敗とは区別できる（要件 6.1、6.2）", () => {
    const time = markOf(
      settled(summary(), {
        kind: "aborted",
        limit: "time",
        elapsedMs: 30000,
        failure: failurePresentation({
          kind: { kind: "execution" },
          reason: "実行が終わらない",
          frames: [{ macro_name: "棚卸し", function: "", line: 8, column: 1 }],
        }),
      }),
    );

    expect(time).toContain("jxcel-macro-result-aborted");
    expect(time).toContain('data-macro-abort-limit="time"');
    expect(time).toContain("実行を打ち切りました: 棚卸し（時間の上限）");
    expect(time).toContain("打ち切りの種類: 時間の上限");
    expect(time).toContain("打ち切りまでの所要 30000 ms");
    expect(time).toContain("実行が終わらない");
    expect(time).toContain('data-macro-frame-line="8"');
    // **失敗の提示（`execution` の層）とは別の区画である**。
    expect(time).not.toContain("jxcel-macro-result-failed");

    const memory = markOf(
      settled(summary(), {
        kind: "aborted",
        limit: "memory",
        elapsedMs: 900,
        failure: failurePresentation({
          kind: { kind: "execution" },
          reason: "メモリの上限に達した",
          frames: [],
        }),
      }),
    );
    // 種類が違えば提示も違う（どちらの上限に当たったかが読める）。
    expect(memory).toContain('data-macro-abort-limit="memory"');
    expect(memory).toContain("メモリの上限");
    expect(memory).not.toContain("時間の上限");
  });

  it("経路の失敗は「実行できなかった」として別に出す（実行の失敗と混ぜない）", () => {
    const markup = markOf(
      settled(summary(), { kind: "rejected", message: "ドキュメントの失敗: 実行基盤が要求を受け取らない" }),
    );

    expect(markup).toContain("jxcel-macro-result-rejected");
    expect(markup).toContain("実行できませんでした: 棚卸し");
    expect(markup).toContain("実行基盤が要求を受け取らない");
    // **失敗の層もフレームも無い**（実行そのものが始まっていない）。
    expect(markup).not.toContain("jxcel-macro-failure-layer");
    expect(markup).not.toContain("jxcel-macro-result-failed");
    expect(markup).not.toContain("jxcel-macro-result-aborted");
  });

  it("結果を閉じると、その区画は出ない（文書も値も動かない）", () => {
    const state = settled(summary(), {
      kind: "ran",
      value: "42",
      output: [],
      changes: { set_cells: 0, inserted_rows: 0, removed_rows: 0, duplicated_rows: 0 },
      changed: false,
      elapsedMs: 3,
    });
    expect(markOf(state)).toContain("jxcel-macro-result-ran");
    expect(markOf(macroSurfaceResultDismissed(state))).not.toContain("jxcel-macro-result-ran");
    // **一覧はそのままである**。
    expect(markOf(macroSurfaceResultDismissed(state))).toContain("jxcel-macro-list");
  });
});


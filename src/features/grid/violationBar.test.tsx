/**
 * 違反のバーの提示（tasks.md 8.4。data-grid 要件 4.2、4.3、4.4。`./violationBar`）。
 *
 * # 何を固定するか
 *
 * 1. **シート全体の違反の総数**（要件 4.3）を常に出す。数は属性にも出す（検査が文字を
 *    読まなくても読めるように）
 * 2. **いまの違反の理由**（要件 4.2）を、**その位置とともに**出す。位置を名乗るのは、
 *    索引が返すのが「その行の最小の違反列」であり、利用者が指した列と違うことがあるため
 *    である（位置を名乗らなければ、別のセルの理由を指したセルの理由として見せることになる）
 * 3. **これ以上違反が無い**（要件 4.4 の正常な結果）ことを、失敗とは違う形で出す
 * 4. 総数が 0 でも**巡回の操作を残す**。総数は行を持たない違反（列そのものの問題）も数える
 *    一方、探索はそのような違反を移動先にしない（`find_violation` の doc）— つまり
 *    **総数が 0 でないのに尽きることはあるが、総数が 0 でも「尽きた」とは限らない**。
 *    ボタンを数で無効にすると、その差が利用者から見えなくなる
 */
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ViolationBar } from "./violationBar";
import type { ViolationPresentation } from "./violations";

/** バー 1 つぶんのマーク付け（**画面が実際に DOM へ出すもの**を読む）。 */
function markOfBar(total: number, presentation: ViolationPresentation | null): string {
  return renderToStaticMarkup(
    createElement(ViolationBar, { total, presentation, onNext: () => undefined }),
  );
}

describe("違反のバー（要件 4.2、4.3、4.4）", () => {
  it("シート全体の違反の総数を常に出す（要件 4.3）", () => {
    const markup = markOfBar(7, null);

    expect(markup).toContain("jxcel-grid-violation-bar");
    expect(markup).toContain('data-violation-total="7"');
    expect(markup).toContain("違反 7 件");
    // 総数は**シート全体**の数である（表示中の窓の数ではない）。
    expect(markup).toContain("シート全体");
  });

  it("総数が 0 のときも、その数を出す（バーを消さない）", () => {
    const markup = markOfBar(0, null);

    expect(markup).toContain('data-violation-total="0"');
    expect(markup).toContain("違反 0 件");
    // 巡回の操作は残す（総数が 0 でも探索が尽きているとは限らない。上の module doc）。
    expect(markup).toContain("jxcel-grid-next-violation");
  });

  it("いまの違反の理由を、その位置とともに出す（要件 4.2）", () => {
    const markup = markOfBar(3, {
      kind: "reason",
      position: { row: 4, column: 1 },
      reason: "値が 0 以上 100 以下の外の値である",
    });

    expect(markup).toContain("jxcel-grid-violation-reason");
    // **文言は境界（適応層）が組み立てたものをそのまま出す**（画面は 2 つ目の文言を作らない）。
    expect(markup).toContain("値が 0 以上 100 以下の外の値である");
    // 位置は**利用者に見える数**（1 起点）で名乗る。機械が読む数（0 起点）は属性に出す。
    expect(markup).toContain("5 行 2 列目");
    expect(markup).toContain('data-violation-row="4"');
    expect(markup).toContain('data-violation-column="1"');
  });

  it("これ以上違反が無いことを出す（要件 4.4 の正常な結果）", () => {
    const markup = markOfBar(3, { kind: "exhausted" });

    expect(markup).toContain("jxcel-grid-violation-exhausted");
    expect(markup).toContain("これ以上違反はありません");
    // **失敗として出さない**（再試行も、失敗の枠も出さない）。
    expect(markup).not.toContain("jxcel-grid-violation-reason");
  });

  it("出すものが無ければ、理由も尽きたことも出さない（空の枠を出さない）", () => {
    const markup = markOfBar(3, null);

    expect(markup).not.toContain("jxcel-grid-violation-reason");
    expect(markup).not.toContain("jxcel-grid-violation-exhausted");
    expect(markup).toContain("jxcel-grid-violation-bar");
  });

  it("次の違反への移動の操作を持つ（要件 4.4）", () => {
    const markup = markOfBar(3, null);

    expect(markup).toContain("jxcel-grid-next-violation");
    expect(markup).toContain("次の違反へ");
    // 操作は**押せる**（`disabled` を出さない）。
    expect(markup).not.toContain("disabled");
  });
});

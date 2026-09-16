/**
 * 違反のバー（tasks.md 8.4。data-grid 要件 4.2、4.3、4.4。design.md「GridScreen /
 * NestedInspector / ViolationBar（要約）」の ViolationBar）。
 *
 * **純粋な描画である**（状態も効果も持たない）。状態を持つのは `./GridScreen` の状態機械で
 * あり、境界への問い合わせは `./violations` が担う。したがって検査は状態を組んで
 * `renderToStaticMarkup` でこの成分を呼び、「何が DOM へ出るか」を読める。
 *
 * # バーと、確定の報告（8.3）の関係（**どちらも違反の数を出すが、別のものである**）
 *
 * | | 確定の報告（`CellEditReport`。8.3） | バー（本 module） |
 * |---|---|---|
 * | いつ出るか | **確定のたびに出て、閉じられる**（直近の確定の記録） | **表を描いている間つねに出る**（表示中のシートの状態） |
 * | 何を出すか | 変換（型強制）の前後と、そのときに残った違反の位置 | シート全体の総数と、いまの違反の理由 |
 * | 数はどこから | `GridEditOutcome.violation_total`（適用の応答） | 開いた応答（`grid_set_view`）と、そのあとの適用の応答 |
 *
 * **2 つは同じ数を出す**（どちらもシート全体の数である）。同じ数であることは、どちらも
 * 適応層が `GridSession::violation_total()` から写した値をそのまま置くことで保たれる —
 * 画面が数え直す経路が 1 つも無いので、**食い違いようがない**。
 *
 * # 総数が 0 でも巡回の操作を残す理由
 *
 * 総数は**行を持たない違反**（列そのものの問題。`GridViolationLocation.row` が `null`）も
 * 数えるが、探索（`find_violation`）はそのような違反を移動先にしない。つまり総数と探索の
 * 対象は同じ集合ではない。ボタンを総数で無効にすると、この差が利用者から見えなくなる
 * （0 件なのに巡回が残る、という形の方がまだ読める）。
 *
 * # 配色
 *
 * 器が与えるカスタムプロパティ（`APPEARANCE_VARS`）だけを参照する（`GridScreen.tsx` と
 * 同じ契約。源の走査は**本 module にも当たる** — `GridScreen.test.ts` の走査が 2 つの源を
 * 並べて見る）。
 */
import type { ReactElement } from "react";

import { APPEARANCE_VARS } from "../../shell/theme";
import type { ViolationPresentation } from "./violations";

/** バーへ渡すもの。**数と、いま出している提示と、巡回の操作だけである。** */
export interface ViolationBarProps {
  /** シート全体の違反の総数（要件 4.3）。 */
  readonly total: number;
  /** いま出している違反の提示。`null` なら出すものが無い。 */
  readonly presentation: ViolationPresentation | null;
  /** 次の違反へ現在位置を移す（要件 4.4）。 */
  readonly onNext: () => void;
}

/** バーの枠（**表の上に出す**）。 */
const BAR_STYLE = {
  display: "flex",
  flexWrap: "wrap",
  alignItems: "center",
  gap: "0.5rem",
  padding: "0.4rem 0.75rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 数と理由の文字（補助的な文字色）。 */
const TEXT_STYLE = { margin: 0, color: `var(${APPEARANCE_VARS.screenMuted})` } as const;

/** 巡回の操作。**器の配色を使う**（8.1 の「再試行」と同じ綴りである）。 */
const BUTTON_STYLE = {
  font: "inherit",
  fontSize: "0.875rem",
  padding: "0.2rem 0.6rem",
  borderRadius: "0.25rem",
  cursor: "pointer",
  color: `var(${APPEARANCE_VARS.controlActiveText})`,
  backgroundColor: `var(${APPEARANCE_VARS.controlActiveBackground})`,
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/** 違反のバー。**総数を常に出し、いまの違反の理由と巡回の操作を添える。** */
export function ViolationBar({ total, presentation, onNext }: ViolationBarProps): ReactElement {
  return (
    <div data-testid="jxcel-grid-violation-bar" data-violation-total={total} style={BAR_STYLE}>
      <span style={TEXT_STYLE}>{`違反 ${String(total)} 件（シート全体）`}</span>
      <button
        type="button"
        data-testid="jxcel-grid-next-violation"
        onClick={onNext}
        style={BUTTON_STYLE}
      >
        次の違反へ
      </button>
      {presentation === null ? null : presentation.kind === "reason" ? (
        /*
          理由（要件 4.2）。**その理由が属するセルを名乗る** — 索引は行ごとに最小の違反列しか
          返さないので、利用者が指した列と違うことがある（`./violations` の `reasonInRow`）。
          位置を名乗れば、どのセルの理由かが読み手に伝わる（名乗らなければ、別のセルの理由を
          指したセルの理由として見せることになる）。

          行と列は**利用者に見える数**（1 起点）で書く。機械が読む生の数（0 起点）は属性に出す。
        */
        <span
          data-testid="jxcel-grid-violation-reason"
          data-violation-row={presentation.position.row}
          data-violation-column={presentation.position.column}
          style={TEXT_STYLE}
        >
          {`${String(presentation.position.row + 1)} 行 ${String(presentation.position.column + 1)} 列目の違反: ${presentation.reason}`}
        </span>
      ) : (
        /*
          「これ以上違反が無い」（要件 4.4 の**正常な結果**）。失敗の告知（`jxcel-grid-notice`）
          とは別のものとして出す — 探索が尽きたことは、利用者の操作が失敗したことではない。
        */
        <span data-testid="jxcel-grid-violation-exhausted" style={TEXT_STYLE}>
          これ以上違反はありません
        </span>
      )}
    </div>
  );
}

/**
 * 数値の入力の面（tasks.md 7.4。data-grid 要件 3.1, 3.2）。
 *
 * # 2 つの札が 1 つの面を共有する（要件 10.2）
 *
 * `Int` と `Float` の入力手段は同じ面である。**違いは刻みだけ**であり、それも別の面を作るほど
 * ではない（整数の欄に小数を打つ道が無いことと、浮動小数の欄が刻みを縛らないことは、
 * `step` の 1 つで表せる）。振る舞いを変える材料は宣言（[`ColumnConstraints.kind`]）から来る —
 * **画面の側に「この札ならこの面」と書かない**（要件 10.3）。
 *
 * # 数値として解釈しない（1 箇所だけ例外がある）
 *
 * 確定するのは**欄の値の文字列**であり、本 module は `Number(...)` を通さない
 * （研究の記録「境界に数値を出さず、表示文字列と変種の札で運ぶ」— 64 ビット整数は JavaScript の
 * 数値で表せない）。ただし欄の**種類**は `number` である — 数値の面が数値の面であるために
 * 要るのは、数値の打鍵を受け取り、刻みを示し、数値として読めない打鍵を欄へ入れないことであり、
 * それはブラウザの数値の欄がそのまま行う（打った文字は `value` から**文字列として**読むので、
 * 途中で数値へ通ることはない）。
 *
 * **限界（正直な記録）**: `type="number"` の欄は、数値として読めない打鍵を値にしない
 * （`abc` と打つと欄の値は空になる）。これは「数値の列に文字を打つ」場面の話であり、
 * 要件 3.5 が保持を求める「型に適合しない**値**」（範囲の外の数・刻みに合わない数）は
 * 打ったまま残る。この区別は `research.md` の「7.4 が記録した限界」に書いてある。
 */
import type { ReactElement } from "react";

import type { CellEditorProps } from "../editorRegistry";
import { handleInputKeys } from "./text";

/**
 * 数値の欄。`Int` は 1 刻み、`Float` は刻みを縛らない。
 *
 * 刻みは**見せるため**のものでもある — 整数の列では上下の矢印が 1 ずつ動き、浮動小数の列では
 * 値の精度を偽らない（`step="any"` は「刻みの主張をしない」の意味である）。
 */
export function NumberEditor({ initialText, constraints, commit, cancel }: CellEditorProps): ReactElement {
  const step = constraints.kind === "Int" ? "1" : "any";

  return (
    <input
      type="number"
      step={step}
      defaultValue={initialText}
      autoFocus
      onKeyDown={handleInputKeys(commit, cancel)}
    />
  );
}

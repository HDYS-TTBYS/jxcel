/**
 * 十進の入力の面（tasks.md 7.4。data-grid 要件 3.1, 3.2）。
 *
 * # なぜ数値の面と分けるのか（`editors/number.tsx` との違い）
 *
 * `Decimal` の値は**十進であり、IEEE 754 の倍精度ではない**。倍精度は 10 進の小数を正確に
 * 表せない（`0.1` は 2 進で循環する）ため、`type="number"` の欄 — 値をブラウザが数値として
 * 解釈する面 — に十進の値を置くと、**打った桁が面の中で動く**。「1.50」と打った値の末尾の 0 が
 * 消えるかどうかは環境の実装に依存し、それは表示文字列として値を運ぶ本アプリの規律
 * （研究の記録「境界に数値を出さず、表示文字列と変種の札で運ぶ」）と噛み合わない。
 *
 * したがってこの面は**文字の欄**であり、`inputMode="decimal"` で「小数を含む数の入力」である
 * ことだけを端末へ伝える（仮想鍵盤を持つ環境では、小数点を含む鍵盤が出る）。打った文字は
 * **そのまま**確定する — 桁を削るのも丸めるのも、判定（`schema-engine` の十進の規則）の仕事で
 * ある（要件 3.3、3.5）。
 *
 * # 数値の面を共有しない理由を、共有で済ませないこと
 *
 * `Float` と `Decimal` はどちらも「小数を含みうる数」であり、面としては近い。それでも module を
 * 分けてあるのは、**この 2 つを取り違えたときにデータが壊れる**ためである — 倍精度で読んだ
 * 十進の値は、元の桁を復元できない。面の実装が同じ形になることより、どちらの札にどちらの面が
 * 付くかを読み手が 1 対 1 で確かめられることを優先する（対応は `editors/index.ts` の表が持つ）。
 */
import type { ReactElement } from "react";

import type { CellEditorProps } from "../editorRegistry";
import { handleInputKeys } from "./text";

/** 十進の欄。**値を JavaScript の数値へ通さない**（桁は打たれたまま運ばれる）。 */
export function DecimalEditor({ initialText, commit, cancel }: CellEditorProps): ReactElement {
  return (
    <input
      type="text"
      inputMode="decimal"
      defaultValue={initialText}
      autoFocus
      onKeyDown={handleInputKeys(commit, cancel)}
    />
  );
}

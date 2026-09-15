/**
 * シート間参照の入力の面（tasks.md 7.4。data-grid 要件 3.8, 3.7, 10.4）。
 *
 * # 参照先の行から選ぶ（要件 3.8）
 *
 * 渡された参照先のシートの行を並べ、**今の参照先を印す**。確定するのは行の**識別子**である —
 * 参照の値は「どの行か」を指すのであって、その行の表示文字列ではない（表示は並べ替えや
 * 絞り込みで変わる。要件 8.5、8.6）。
 *
 * **打たせない。** 参照は宛先の行そのものであり、手で打った識別子は存在しない行を指しうる。
 * 一覧から選ぶことが、この型の入力手段である（要件 3.8 の「参照先のシートに存在する行から選ぶ」）。
 *
 * # 参照先が渡されていなければ、既定へ落ちる（要件 10.4）
 *
 * **境界は参照先をまだ運ばない** — 参照先のシートの名も、そのシートの行を一覧する経路も無い
 * （6.1 のコマンドは 6 本であり、その中に無い）。行が 1 つも無い場合も同じである
 * （選ぶ対象が無い一覧は、値を編集できない面になる）。どちらの場合も**値をそのまま扱う面**
 * （[`TextEditor`]）へ落ちるので、いまある参照は打ち直せる。
 *
 * 材料が揃った日に、この面は書き換えずに一覧になる — 必要な追加は `research.md` の
 * 「7.4 が記録した隙間」に書いてある。
 *
 * # シートの名を見せる
 *
 * 行の見出しだけでは、その行がどのシートのものかが分からない。参照先のシートの名は
 * 宣言（[`ReferenceSource.sheet`]）から来るので、面はそれを出す。
 */
import type { ReactElement } from "react";

import type { CellEditorProps } from "../editorRegistry";
import { CancelButton, NoValueButton, TextEditor } from "./text";

/** 参照先の行からの選択（`Ref`）。 */
export function RefEditor({ initialText, constraints, commit, cancel }: CellEditorProps): ReactElement {
  const reference = constraints.reference;
  if (reference === undefined || reference.rows.length === 0) {
    return <TextEditor initialText={initialText} constraints={constraints} commit={commit} cancel={cancel} />;
  }

  return (
    <div>
      <p>{`参照先: ${reference.sheet}`}</p>
      <ul>
        {reference.rows.map((row) => (
          <li key={row.id}>
            <button
              type="button"
              value={row.id}
              aria-current={row.id === initialText ? "true" : undefined}
              onClick={(event) => commit(event.currentTarget.value)}
            >
              {row.label}
            </button>
          </li>
        ))}
      </ul>
      {constraints.nullable ? <NoValueButton commit={commit} /> : null}
      <CancelButton cancel={cancel} />
    </div>
  );
}

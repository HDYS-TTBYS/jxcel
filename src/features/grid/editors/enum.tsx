/**
 * 選択肢を持つ型の入力の面（tasks.md 7.4。data-grid 要件 3.2, 3.7, 10.4）。
 *
 * # 選択肢の一覧（要件 3.2）
 *
 * 渡された選択肢を**一覧として並べ**、その 1 つを選んだ状態で出す。確定するのは**値**
 * （{@link EnumChoice.value}）であり、並ぶのは名である。
 *
 * # 選択肢が渡されていなければ、既定へ落ちる（要件 10.4）
 *
 * **境界は選択肢をまだ運ばない**（`editorRegistry.ts` の申し送りと `research.md` の
 * 「7.4 が記録した隙間」）。だからこの面は、`constraints.choices` が渡されたときにだけ一覧に
 * なり、渡されなければ**値をそのまま扱う面**（[`TextEditor`]）になる。
 *
 * これは**ごまかしではない**。選択肢の一覧が無い状態で「それらしい一覧」を出す道は 3 つあり、
 * どれも取れない:
 *
 * - 空の選択肢を並べる → 何も選べない面になり、値の編集ができなくなる（要件 3.1 に反する）
 * - 選択肢を勝手に作る → 宣言に無い値を面が提案することになり、**必ず違反を生む**
 * - 値を打たせない → 既にある値の打ち直しができなくなる
 *
 * 既定へ落ちれば、値は**そのまま**編集できる（適合の判定は `schema-engine` が行う）。材料が
 * 揃った日に、この面は書き換えずに一覧になる。
 *
 * # 表の側（`select`）にしない理由
 *
 * 一覧を `<select>` に畳むと、開くまで何が選べるか見えない。要件 3.2 が求めるのは
 * 「選択肢の一覧を提示する」ことであり、選択肢の数は宣言が決める（数十件の列では、開く操作の
 * 後ろに隠す理由が無い）。並べた選択肢は**そのまま読み上げの対象**でもある。
 */
import { useId } from "react";
import type { ReactElement } from "react";

import type { CellEditorProps } from "../editorRegistry";
import { CancelButton, NoValueButton, TextEditor } from "./text";

/** 選択肢の一覧（`Enum`）。選択肢が渡されていなければ、値をそのまま扱う面である。 */
export function EnumEditor({ initialText, constraints, commit, cancel }: CellEditorProps): ReactElement {
  // **面ごとに一意な群名**（`useId`）。定数にすると、同じ型の面が 1 画面に 2 つ現れたとき
  // （入れ子の `Object` に `Enum` の位置が 2 つ、など）ブラウザが 1 つの群として扱い、
  // 一方を選ぶともう一方の印が外れる — **印が値と食い違い、書かれる値と表示がずれる**。
  const group = useId();
  const choices = constraints.choices ?? [];
  if (choices.length === 0) {
    return <TextEditor initialText={initialText} constraints={constraints} commit={commit} cancel={cancel} />;
  }

  return (
    <div>
      <ul>
        {choices.map((choice) => (
          <li key={choice.value}>
            <label>
              <input
                type="radio"
                name={group}
                value={choice.value}
                defaultChecked={choice.value === initialText}
                onChange={(event) => commit(event.currentTarget.value)}
              />
              {choice.label}
            </label>
          </li>
        ))}
      </ul>
      {constraints.nullable ? <NoValueButton commit={commit} /> : null}
      <CancelButton cancel={cancel} />
    </div>
  );
}

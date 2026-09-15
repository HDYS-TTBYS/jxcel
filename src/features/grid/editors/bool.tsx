/**
 * 真偽の入力の面（tasks.md 7.4。data-grid 要件 3.2, 3.7）。
 *
 * # 二値の切り替え（要件 3.2）
 *
 * 真と偽の 2 つを並べ、初期値の側を選んだ状態で出す。**文字を打たせない** — 真偽の列に
 * `t` / `Y` / `はい` を打つ余地を残すと、綴りの揺れがそのまま違反になる
 * （`schema-engine` は真偽の綴りを 1 つに定めている）。
 *
 * **入り切りの 1 つの箱（checkbox）にしない理由**: 未入りの箱は「偽」と「値なし」の
 * どちらとも読める。値なしを許す列（`constraints.nullable`）では**その 2 つは別の状態**であり
 * （要件 3.7 と、真偽の値そのもの）、面の上で見分けが付かない表現は使えない。2 つの選択肢なら
 * 「どちらも選ばれていない」が値なしであり、`値なし` の道を別に持てる。
 *
 * # 確定する文字
 *
 * `"true"` / `"false"` である（生成物の窓の表示文字列と同じ綴り。`crates/data-grid` の窓の
 * 実例に `Bool` の `true` がある）。面は値を作らず、**要素の `value` をそのまま渡す**。
 */
import { useId } from "react";
import type { ReactElement } from "react";

import type { CellEditorProps } from "../editorRegistry";
import { CancelButton, NoValueButton } from "./text";

/** 二値と、その表示名。**綴りは表示文字列の側に合わせる**（面が独自の綴りを作らない）。 */
const BOOL_OPTIONS: readonly { readonly value: string; readonly label: string }[] = [
  { value: "true", label: "真" },
  { value: "false", label: "偽" },
];

/**
 * 真偽の切り替え。
 *
 * `name` は 1 つに固定してある — 面は 1 度に 1 つしか出ないため、束ねる名前を呼び出し元から
 * 受け取る必要が無い（受け取ると、`CellEditorProps` を design が固定している以上、
 * 名前を渡す道が「宣言」に紛れ込む）。
 */
export function BoolEditor({ initialText, constraints, commit, cancel }: CellEditorProps): ReactElement {
  // **面ごとに一意な群名**（`useId`）。定数にすると、同じ型の面が 1 画面に 2 つ現れたとき
  // （入れ子の `Object` に `Bool` の位置が 2 つ、など）ブラウザが 1 つの群として扱い、
  // 一方を選ぶともう一方の印が外れる — **印が値と食い違い、書かれる値と表示がずれる**。
  const group = useId();
  return (
    <div role="group" aria-label="真偽">
      {BOOL_OPTIONS.map((option) => (
        <label key={option.value}>
          <input
            type="radio"
            name={group}
            value={option.value}
            defaultChecked={initialText === option.value}
            onChange={(event) => commit(event.currentTarget.value)}
          />
          {option.label}
        </label>
      ))}
      {constraints.nullable ? <NoValueButton commit={commit} /> : null}
      <CancelButton cancel={cancel} />
    </div>
  );
}

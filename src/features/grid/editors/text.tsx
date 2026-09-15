/**
 * 文字の入力の面（tasks.md 7.4。data-grid 要件 3.1, 3.7, 10.2, 10.4）。
 *
 * # この面が既定である
 *
 * **登録簿の既定はこの面である**（要件 10.4「登録されていない型のセルが編集されたとき、値を
 * そのまま扱える既定の入力手段を提示する」）。`Text` の札の入力手段でもある（要件 10.2）—
 * 既定と `Text` が同じ面であることは重複ではない。**「値をそのまま扱う」ことが `Text` の
 * 意味そのもの**だからである。
 *
 * # 打たれた文字は解釈しない
 *
 * 面が出すのは**打たれた文字**だけである（要件 3.3「入力された値を型システムの判定に掛けた
 * うえで、その結果とともにドキュメントへ反映する」）。数値へ直すのも、日付として読むのも、
 * 適合しない値を違反として残すのも `schema-engine` とその上の適用層の仕事である
 * （要件 3.5「その値を破棄せずに保持したうえで、違反として提示する」）。**面が先に解釈すると、
 * 打った文字が解釈の都合で書き換わり、違反として提示すべき値が消える。**
 *
 * # 取り消しと値なし（要件 3.6、3.7）
 *
 * - 文字を打てる面は `Escape` で取り消す（[`handleInputKeys`]）。その面は**欄を空にする**ことで
 *   値なしへ戻せるので、`値なし` のボタンを重ねて持たない
 * - **キーだけで取り消せない面**（暦・一覧・二値・参照）は、[`CancelButton`] と、値なしを許す
 *   列（`constraints.nullable`）での [`NoValueButton`] を借りる
 * - **値なしの表現は空の文字列である**。境界の宛先は「打たれた文字」であり
 *   （生成物の `GridCellEdit.text`）、空の文字列は値なしへ写る
 *   （`crates/data-grid/src/edit/mod.rs` の `edited_value`）。**面はその規約を写すだけであり、
 *   自分で「値なし」という別の表現を作らない**
 */
import type { KeyboardEventHandler, ReactElement } from "react";

import type { CellEditorProps } from "../editorRegistry";

/**
 * 打たれた文字をそのまま確定する欄。**登録簿の既定であり**（要件 10.4）、`Text` の札の入力手段
 * でもある（要件 10.2）。
 *
 * `defaultValue`（制御しない欄）である理由: 打鍵のたびに React の状態へ写すと、変換の途中の
 * 文字（日本語入力の未確定の文字）まで状態になり、確定の瞬間に何を渡すかが状態の更新の順序に
 * 依存する。**確定のときに欄の値（`currentTarget.value`）を読む**方が、打たれた文字そのものを
 * 渡せる（要件 3.5 の「破棄せずに保持」はこの経路で満たす）。
 */
export function TextEditor({ initialText, commit, cancel }: CellEditorProps): ReactElement {
  return (
    <input type="text" defaultValue={initialText} autoFocus onKeyDown={handleInputKeys(commit, cancel)} />
  );
}

/**
 * 文字を打てる面の鍵の規律: `Enter` で打たれた文字を確定し、`Escape` で取り消す。
 *
 * 3 つの面（文字・数値・十進）が同じ規律を共有する。**`any` は共有しない** — 複数行を打てる面で
 * は `Enter` が改行であり、確定は別の鍵である（`editors/any.tsx`）。
 */
export function handleInputKeys(
  commit: (text: string) => void,
  cancel: () => void,
): KeyboardEventHandler<HTMLInputElement> {
  return (event) => {
    if (event.key === "Enter") {
      commit(event.currentTarget.value);
    } else if (event.key === "Escape") {
      cancel();
    }
  };
}

/** キーだけで取り消せない面が持つ「取消」（要件 3.6。何が起きるかは画面が決める）。 */
export function CancelButton({ cancel }: { readonly cancel: () => void }): ReactElement {
  return (
    <button type="button" onClick={cancel}>
      取消
    </button>
  );
}

/**
 * 値なしへ戻す道（要件 3.7）。
 *
 * 確定する文字は**要素の `value`（空の文字列）**であり、押下はそれをそのまま渡す — 他の面と
 * 同じ経路である（どの押下がどの文字を確定するかが面の構造から読める）。
 */
export function NoValueButton({ commit }: { readonly commit: (text: string) => void }): ReactElement {
  return (
    <button type="button" value="" onClick={(event) => commit(event.currentTarget.value)}>
      値なし
    </button>
  );
}

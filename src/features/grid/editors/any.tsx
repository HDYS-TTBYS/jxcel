/**
 * 任意の値の入力の面（tasks.md 7.4。data-grid 要件 3.1、要件 10.4 の既定と同じ規律）。
 *
 * # `Any` とは何か
 *
 * `Any` の札は「その列の値の型が定まっていない」ことを表す（`schema-engine` の `TypeKind`）。
 * したがって面ができることは 1 つしかない — **打たれた文字をそのまま扱う**。既定の面
 * （`editors/text.tsx`）と同じ規律であり、そこへ**複数行**の余地を足したものである。
 *
 * # なぜ複数行なのか
 *
 * `Any` の値は**構造**でありうる（オブジェクト・並び）。その表示文字列は構造表現であり、
 * 改行を含むことがある（1 行の欄では、既にある値を打ち直すことができない — 打ち直せない面は、
 * 値を読むだけの面と区別が付かない）。確定は `Ctrl`（または `Cmd`）+ `Enter` である —
 * 複数行の面では `Enter` は改行であり、取り消しは `Escape` のままである。
 *
 * # 既定の面と別の module である理由
 *
 * 面の実装は近いが、**`Any` の札に登録された面が現れること**（要件 10.2）と、`Any` の値が
 * 複数行でありうること（上の節）は別の事実である。同じ module に寄せると、`Text` の面を
 * 直したときに `Any` の面が黙って変わる。
 */
import type { ReactElement } from "react";

import type { CellEditorProps } from "../editorRegistry";

/** 値をそのまま扱う複数行の面。 */
export function AnyEditor({ initialText, commit, cancel }: CellEditorProps): ReactElement {
  return (
    <textarea
      defaultValue={initialText}
      autoFocus
      onKeyDown={(event) => {
        // 複数行を打てる面なので `Enter` は改行である（`editors/text.tsx` の鍵の規律とは別）。
        if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
          commit(event.currentTarget.value);
        } else if (event.key === "Escape") {
          cancel();
        }
      }}
    />
  );
}

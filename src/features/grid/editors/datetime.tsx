/**
 * 日時の入力の面（tasks.md 7.4。data-grid 要件 3.2, 3.7）。
 *
 * # 暦と時刻（要件 3.2）
 *
 * 日付の面（`editors/date.tsx`）と同じ暦を使い、そこへ**時刻の欄**を足す。確定する文字は
 * 正準表記である（`crates/schema-engine/src/types/datetime.rs` の文法）:
 *
 * ```text
 * civil 日時: YYYY-MM-DDTHH:MM:SS[.<小数>]
 * 瞬時:       YYYY-MM-DDTHH:MM:SS[.<小数>](Z|±HH:MM)
 * ```
 *
 * # 打った桁とオフセットを保つ（本 module で最も重要な決定）
 *
 * 十進の面と同じ理由で、**値を通す経路を持たない**。とくに:
 *
 * - **小数部**（`.5` のような 9 桁までの小数）は初期値のものを**そのまま保つ** — 打ち直していない
 *   桁を面が削ると、値が黙って変わる
 * - **オフセット**（`Z` / `±HH:MM`）も同じである。オフセットを要求する宣言かどうかは境界から
 *   来ない（`ColumnDescriptor` は宣言の細部を運ばない）ので、**面は初期値が持っていた末尾を保つ**。
 *   初期値がオフセットを持たない列では、組み立てる値も持たない — 要否の判断は判定層の領分である
 *   （要件 3.3）
 *
 * # 時だけを変えるときも、日を押して確定する
 *
 * 押下の経路は「この日時へ変える」という指示であり、時刻の欄の値はその指示に含まれる。つまり
 * **時刻を打ち直してから、暦のその日を押す**。時刻の欄で `Enter` を押した場合も確定する
 * （すでに表示されている日をそのまま使う）。初期値が無いセルでは日が定まらないため、
 * `Enter` は何もしない — 日の選択が先である。
 */
import { useState, type ReactElement } from "react";

import type { CellEditorProps } from "../editorRegistry";
import { Calendar, monthOf, parseDateText, shiftMonth, type Month } from "./date";
import { CancelButton, NoValueButton } from "./text";

/** 正準表記の日時を、暦の日・時刻・末尾（小数とオフセット）へ分けたもの。 */
export interface DateTimeParts {
  /** `YYYY-MM-DD`。 */
  readonly date: string;
  /** `HH:MM:SS`。 */
  readonly time: string;
  /** 秒の直後の残り（`.<小数>` と `Z` / `±HH:MM`。無ければ空の文字列）。 */
  readonly suffix: string;
}

const CANONICAL_DATETIME = /^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}:\d{2})((?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})?)$/;

/**
 * 正準表記を読む（読めなければ `null`）。
 *
 * **日付の部分は `editors/date.tsx` の読み手に掛ける** — 2 つの面が同じ綴りを受け入れることを、
 * 綴りの規則を 2 箇所に書くことで保つより、同じ読み手を通すことで保つ。
 */
export function parseDateTimeText(text: string): DateTimeParts | null {
  const match = CANONICAL_DATETIME.exec(text);
  if (match === null) {
    return null;
  }
  const date = match[1] ?? "";
  const time = match[2] ?? "";
  const suffix = match[3] ?? "";
  if (parseDateText(date) === null) {
    return null;
  }

  return { date, time, suffix };
}

/** 打たれた時刻と、暦の日と、保つ末尾から正準表記を組み立てる。 */
export function composeDateTime(date: string, time: string, suffix: string): string {
  return `${date}T${time}${suffix}`;
}

/** 日時の面（暦と時刻の欄、値なしと取消の道）。 */
export function DateTimeEditor({ initialText, constraints, commit, cancel }: CellEditorProps): ReactElement {
  const parsed = parseDateTimeText(initialText);
  const [view, setView] = useState<Month>(() => monthOf(parsed === null ? null : parsed.date));
  const [time, setTime] = useState<string>(parsed === null ? "00:00:00" : parsed.time);
  const suffix = parsed === null ? "" : parsed.suffix;

  return (
    <div>
      <Calendar
        year={view.year}
        month={view.month}
        selected={parsed === null ? "" : parsed.date}
        onShiftMonth={(delta) => setView(shiftMonth(view, delta))}
        onPick={(date) => commit(composeDateTime(date, time, suffix))}
      />
      <input
        type="text"
        inputMode="numeric"
        aria-label="時刻"
        value={time}
        onChange={(event) => setTime(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && parsed !== null) {
            commit(composeDateTime(parsed.date, time, suffix));
          } else if (event.key === "Escape") {
            cancel();
          }
        }}
      />
      {constraints.nullable ? <NoValueButton commit={commit} /> : null}
      <CancelButton cancel={cancel} />
    </div>
  );
}

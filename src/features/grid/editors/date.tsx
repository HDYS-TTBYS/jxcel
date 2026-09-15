/**
 * 日付の入力の面（tasks.md 7.4。data-grid 要件 3.2, 3.7）。
 *
 * # 暦による選択（要件 3.2）
 *
 * 日付を打たせず、**暦から選ばせる**。月の升は日曜始まりの週に切り、前後の月へ送れる。
 * 確定する文字は**正準表記**（`YYYY-MM-DD`）である — `schema-engine` が受理する綴りは
 * 正準表記そのものであり（`crates/schema-engine/src/types/datetime.rs` の文法）、
 * 面が別の綴り（`2026/9/5`、和暦、曜日名）を作れば**必ず違反になる**。
 *
 * # 暦は自前で描く（ブラウザの日付の欄に委ねない）
 *
 * `<input type="date">` の暦はブラウザの実装に依存する（3 OS で同じものが出る保証が無く、
 * 出ない環境では欄がただの文字入力になる）。本アプリは 3 OS での描画の成立を要件にしているため
 * （要件 12）、暦の見え方を自前で持つ。依存は足さない（design.md の Allowed Dependencies に
 * 暦の部品は無い）。
 *
 * # 日付だけを見る。値の正しさは判定の領分である
 *
 * [`parseDateText`] が読むのは**見出しの月と、選んだ日の印**を決めるためだけであり、
 * 「その日が存在するか」の判定ではない（`2026-02-31` は月としては読めるが、その日の升は無い
 * ので印が付かない）。値が適合するかを決めるのは `schema-engine` である（要件 3.3）。
 */
import { useState, type ReactElement } from "react";

import type { CellEditorProps } from "../editorRegistry";
import { CancelButton, NoValueButton } from "./text";

/** 月（年と、1 起点の月）。 */
export interface Month {
  readonly year: number;
  readonly month: number;
}

/** 正準表記の日付を分解したもの。 */
export interface CivilDate {
  readonly year: number;
  readonly month: number;
  readonly day: number;
}

/** 正準表記（`crates/schema-engine/src/types/datetime.rs` の文法: 4 桁-2 桁-2 桁）。 */
const CANONICAL_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;

/** 週の見出し（日曜始まり。`monthGrid` の並びと同じ順である）。 */
const WEEKDAY_LABELS: readonly string[] = ["日", "月", "火", "水", "木", "金", "土"];

/**
 * 正準表記を読む（読めなければ `null`）。**値の正否の判定ではない**（上の module doc）。
 *
 * 月の範囲だけは見る — 1 起点の月は[`monthGrid`]と[`shiftMonth`]の前提であり、13 月を受け取ると
 * 暦の見出しが壊れる。日の上限は 31 で止める（実際の日数は暦が知っており、その日の升が無い
 * ことで現れる）。
 */
export function parseDateText(text: string): CivilDate | null {
  const match = CANONICAL_DATE.exec(text);
  if (match === null) {
    return null;
  }
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  if (month < 1 || month > 12 || day < 1 || day > 31) {
    return null;
  }
  return { year, month, day };
}

/** 正準表記へ組み立てる（月と日は 2 桁へ揃える）。 */
export function composeDate(year: number, month: number, day: number): string {
  const paddedMonth = month < 10 ? `0${month}` : `${month}`;
  const paddedDay = day < 10 ? `0${day}` : `${day}`;

  return `${year}-${paddedMonth}-${paddedDay}`;
}

/**
 * 月の升の並び。日曜始まりの週に切る。**月の外は 0 で埋める**（日は 1 以上なので衝突しない）。
 *
 * **UTC で計算する。**地方時を使うと、時差のある環境で月初の曜日が 1 日ずれ、同じ日付でも
 * 暦の見え方が実行環境で変わる（3 OS で同じものを出すという要件 12 に反する）。
 */
export function monthGrid(year: number, month: number): readonly (readonly number[])[] {
  const leading = new Date(Date.UTC(year, month - 1, 1)).getUTCDay();
  const length = new Date(Date.UTC(year, month, 0)).getUTCDate();

  const cells: number[] = [];
  for (let blank = 0; blank < leading; blank += 1) {
    cells.push(0);
  }
  for (let day = 1; day <= length; day += 1) {
    cells.push(day);
  }
  while (cells.length % 7 !== 0) {
    cells.push(0);
  }

  const weeks: (readonly number[])[] = [];
  for (let start = 0; start < cells.length; start += 7) {
    weeks.push(cells.slice(start, start + 7));
  }

  return weeks;
}

/** 月を送る（年をまたいで正の範囲へ正規化する）。 */
export function shiftMonth(month: Month, delta: number): Month {
  const total = month.year * 12 + (month.month - 1) + delta;

  return { year: Math.floor(total / 12), month: ((total % 12) + 12) % 12 + 1 };
}

/**
 * 見出しの月を、初期値の正準表記から決める（読めなければ今月）。
 *
 * **今月は UTC で取る**（[`monthGrid`] と同じ理由）。初期値が無いセル（値なし）では「今日の月」が
 * 最も近い見当であり、そこから送れる。
 */
export function monthOf(dateText: string | null): Month {
  const parsed = dateText === null ? null : parseDateText(dateText);
  if (parsed !== null) {
    return { year: parsed.year, month: parsed.month };
  }

  const now = new Date();

  return { year: now.getUTCFullYear(), month: now.getUTCMonth() + 1 };
}

/** 暦の見え方（月と、選ばれている日）。 */
export interface CalendarProps {
  readonly year: number;
  readonly month: number;
  /** 選ばれている日（正準表記。無ければ空の文字列）。 */
  readonly selected: string;
  readonly onShiftMonth: (delta: number) => void;
  readonly onPick: (date: string) => void;
}

/**
 * 暦（月の升）。**押された升が確定する文字を持つ** — 升は `value` に正準表記を持つので、
 * 押下の経路はそれをそのまま渡す（どの升がどの文字を確定するかが面の構造から読める）。
 *
 * 日時の面（`editors/datetime.tsx`）もこの部品を使う（暦の見え方は 1 つである）。
 */
export function Calendar({ year, month, selected, onShiftMonth, onPick }: CalendarProps): ReactElement {
  return (
    <div>
      <div>
        <button type="button" aria-label="前の月" onClick={() => onShiftMonth(-1)}>
          ‹
        </button>
        <span>{`${year}年${month}月`}</span>
        <button type="button" aria-label="次の月" onClick={() => onShiftMonth(1)}>
          ›
        </button>
      </div>
      <table>
        <thead>
          <tr>
            {WEEKDAY_LABELS.map((label) => (
              <th key={label} scope="col">
                {label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {monthGrid(year, month).map((week, weekIndex) => (
            <tr key={weekIndex}>
              {week.map((day, dayIndex) => (
                <td key={dayIndex}>
                  {day === 0 ? null : (
                    <CalendarDay
                      date={composeDate(year, month, day)}
                      label={day}
                      selected={selected}
                      onPick={onPick}
                    />
                  )}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

interface CalendarDayProps {
  /** この升が確定する正準表記。 */
  readonly date: string;
  readonly label: number;
  /** 選ばれている日（正準表記）。 */
  readonly selected: string;
  readonly onPick: (date: string) => void;
}

/** 1 つの升。 */
function CalendarDay({ date, label, selected, onPick }: CalendarDayProps): ReactElement {
  return (
    <button
      type="button"
      value={date}
      aria-current={date === selected ? "date" : undefined}
      onClick={(event) => onPick(event.currentTarget.value)}
    >
      {label}
    </button>
  );
}

/** 日付の面（暦と、値なし・取消の道）。 */
export function DateEditor({ initialText, constraints, commit, cancel }: CellEditorProps): ReactElement {
  const parsed = parseDateText(initialText);
  const [view, setView] = useState<Month>(() => monthOf(initialText));

  return (
    <div>
      <Calendar
        year={view.year}
        month={view.month}
        selected={parsed === null ? "" : composeDate(parsed.year, parsed.month, parsed.day)}
        onShiftMonth={(delta) => setView(shiftMonth(view, delta))}
        onPick={commit}
      />
      {constraints.nullable ? <NoValueButton commit={commit} /> : null}
      <CancelButton cancel={cancel} />
    </div>
  );
}

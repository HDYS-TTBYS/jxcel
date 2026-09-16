/**
 * 組込の入力手段が何を描くか（tasks.md 7.4。data-grid 要件 3.1, 3.2, 3.8, 10.1〜10.5）。
 *
 * # 何を表明し、何を表明しないか
 *
 * ここで固定するのは**描かれた面の構造**（どの要素が、どの文字を値として持ち、初期値のどれが
 * 選ばれているか）と、**DOM を要さない計算**（暦の並び・正準表記の組み立てと分解・入れ子の
 * 読み書き）である。実物の押下・打鍵・フォーカスの移動は**ここでは表明しない** — `node` の
 * 環境に DOM が無いためであり（`vitest.config.ts` のヘッダ「環境が node であること」）、
 * それらは 8.3 の画面と 9.2 の実起動で観測する（`tech.md` の「GUI の主張は実起動で確かめる」）。
 *
 * 例外は「押したときに何が確定されるか」である。組込の面はどれも、**確定する文字を
 * 要素の `value` として持つ**（押下の側は `event.currentTarget.value` をそのまま渡す）。
 * したがって「どの押下がどの文字を確定するか」は面の構造から読み取れる —
 * 「15 日の升は `2026-09-15` を確定する」はそれで固定できる。
 *
 * # 面の共通の規律（`editors/text.tsx` の module docs が唯一の源）
 *
 * - 打たれた文字は**そのまま**確定する（解釈は `schema-engine` が行う。要件 3.3）
 * - **空の文字列が値なしである**（`crates/data-grid/src/edit/mod.rs` の `edited_value`）
 * - 文字を打てる面は `Escape` で取り消す。**キーだけで取り消せない面**（暦・一覧・二値・参照）は
 *   `取消` のボタンを持ち、`値なし` を許す列では `値なし` のボタンも持つ
 */
import { createElement } from "react";
import type { ComponentType } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { TypeKindTag } from "../../../ipc/bindings";
import type { CellEditorProps, ColumnConstraints } from "../editorRegistry";
import { createEditorRegistry } from "../editorRegistry";
import { createBuiltinEditorRegistry } from "./index";
import { AnyEditor } from "./any";
import { BoolEditor } from "./bool";
import { Calendar, DateEditor, composeDate, monthGrid, parseDateText, shiftMonth } from "./date";
import { DateTimeEditor, composeDateTime, parseDateTimeText } from "./datetime";
import { DecimalEditor } from "./decimal";
import { EnumEditor } from "./enum";
import { assembleNestedText, createNestedEditor, parseNestedText } from "./nested";
import { NumberEditor } from "./number";
import { RefEditor } from "./ref";
import { TextEditor } from "./text";

// ===========================================================================
// 道具
// ===========================================================================

/**
 * 値なしを許さない列の宣言（値の無いことは列の宣言が決めるため、既定は「許さない」）。
 * **`Record<TypeKindTag, …>` なので、生成物の型に変種が増えれば鍵が足りずに型検査が落ちる。**
 */
const PLAIN: Record<TypeKindTag, ColumnConstraints> = {
  Int: { kind: "Int", nullable: false },
  Float: { kind: "Float", nullable: false },
  Decimal: { kind: "Decimal", nullable: false },
  Text: { kind: "Text", nullable: false },
  Bool: { kind: "Bool", nullable: false },
  Date: { kind: "Date", nullable: false },
  DateTime: { kind: "DateTime", nullable: false },
  Enum: { kind: "Enum", nullable: false },
  Ref: { kind: "Ref", nullable: false },
  Attachment: { kind: "Attachment", nullable: false },
  Object: { kind: "Object", nullable: false },
  Array: { kind: "Array", nullable: false },
  Any: { kind: "Any", nullable: false },
  Custom: { kind: "Custom", nullable: false },
};

/** 面を描いて、その文字列（HTML）を返す。押下と打鍵は起こさない（上のヘッダを参照）。 */
function render(
  component: ComponentType<CellEditorProps>,
  initialText: string,
  constraints: ColumnConstraints,
): string {
  const props: CellEditorProps = { initialText, constraints, commit: () => {}, cancel: () => {} };
  return renderToStaticMarkup(createElement(component, props));
}

/**
 * 同じ型の面を**1 つの木の中に 2 つ**描く（入れ子の `Object` に同じ型の位置が 2 つ現れる形）。
 *
 * 別々の木として描くと `useId` は同じ id を返すため（木の中の位置で決まる）、**群名の一意性は
 * 別々の木では測れない**。測るべきは「1 つの木の中の 2 つの面が別の群を持つこと」である。
 */
function renderTwo(component: ComponentType<CellEditorProps>, constraints: ColumnConstraints): string {
  const props = (initialText: string): CellEditorProps => ({
    initialText,
    constraints,
    commit: () => {},
    cancel: () => {},
  });
  return renderToStaticMarkup(
    createElement("div", null, createElement(component, props("true")), createElement(component, props("false"))),
  );
}

/** 描かれた順の群名（`name="…"`）。 */
function radioGroupNames(markup: string): readonly string[] {
  // 捕獲群がある正規表現なので `match[1]` はつねにあるが、型の上では `undefined` を含む。
  return [...markup.matchAll(/name="([^"]*)"/g)].flatMap((match) =>
    match[1] === undefined ? [] : [match[1]],
  );
}

/** 描かれた `<input …>` の並び。 */
function inputTags(markup: string): readonly string[] {
  return markup.match(/<input\b[^>]*>/g) ?? [];
}

/** 描かれた `<button …>` の並び。 */
function buttonTags(markup: string): readonly string[] {
  return markup.match(/<button\b[^>]*>/g) ?? [];
}

/** `checked` の付いた欄が持つ値の並び。 */
function checkedValues(markup: string): readonly string[] {
  return inputTags(markup)
    .filter((tag) => tag.includes("checked"))
    .map((tag) => /value="([^"]*)"/.exec(tag)?.[1] ?? "");
}

/** 組込の登録簿に結び付いた入れ子の面（本 file の大半の検査が使う）。 */
function nestedEditor(): ComponentType<CellEditorProps> {
  return createNestedEditor(createBuiltinEditorRegistry());
}

/** 指定の値を運ぶ `<button …>`（無ければ `undefined`）。 */
function buttonWithValue(markup: string, value: string): string | undefined {
  return buttonTags(markup).find((tag) => tag.includes(`value="${value}"`));
}

// ===========================================================================
// 文字の面（既定。要件 10.4）
// ===========================================================================

describe("文字の面", () => {
  it("打たれた文字をそのまま初期値として持ち、解釈しない", () => {
    const markup = render(TextEditor, " 3.50 と 文字 ", PLAIN.Text);

    expect(markup).toContain('type="text"');
    expect(markup).toContain('value=" 3.50 と 文字 "');
  });

  it("空の欄をそのまま確定できるので、値なしのボタンを重ねて持たない（要件 3.7）", () => {
    const markup = render(TextEditor, "x", { kind: "Text", nullable: true });

    expect(markup).not.toContain(">値なし<");
  });
});

// ===========================================================================
// 数値の面（Int / Float）
// ===========================================================================

describe("数値の面", () => {
  it("整数の列は刻み 1 の数値の欄を出す", () => {
    const markup = render(NumberEditor, "42", PLAIN.Int);

    expect(markup).toContain('type="number"');
    expect(markup).toContain('step="1"');
    expect(markup).toContain('value="42"');
  });

  it("浮動小数の列は刻みを縛らない", () => {
    const markup = render(NumberEditor, "3.5", PLAIN.Float);

    expect(markup).toContain('type="number"');
    expect(markup).toContain('step="any"');
    expect(markup).toContain('value="3.5"');
  });
});

// ===========================================================================
// 十進の面（Decimal）
// ===========================================================================

describe("十進の面", () => {
  it("値を倍精度へ通さない面であり、打った桁が残る", () => {
    const markup = render(DecimalEditor, "1.50", PLAIN.Decimal);

    expect(markup).toContain('type="text"');
    // 属性の綴りは React の出力（`inputMode`）に従う。HTML の属性名は大文字小文字を区別しない。
    expect(markup).toMatch(/inputmode="decimal"/i);
    // `type="number"` は値を IEEE 754 の倍精度として扱う面であり、十進の桁を保証しない
    // （研究の記録: 「境界に数値を出さず、表示文字列と変種の札で運ぶ」）。
    expect(markup).not.toContain('type="number"');
    expect(markup).toContain('value="1.50"');
  });
});

// ===========================================================================
// 二値の切り替え（要件 3.2）
// ===========================================================================

describe("二値の切り替え", () => {
  it("真偽の二値が並び、初期値の側が選ばれている", () => {
    const truthy = render(BoolEditor, "true", PLAIN.Bool);
    expect(truthy).toContain(">真<");
    expect(truthy).toContain(">偽<");
    expect(checkedValues(truthy)).toEqual(["true"]);

    expect(checkedValues(render(BoolEditor, "false", PLAIN.Bool))).toEqual(["false"]);
  });

  it("値なしを持つ列では、値なしへ戻す道が並ぶ", () => {
    expect(buttonWithValue(render(BoolEditor, "true", { kind: "Bool", nullable: true }), "")).toBeDefined();
    expect(buttonWithValue(render(BoolEditor, "true", PLAIN.Bool), "")).toBeUndefined();
  });

  it("キーだけでは取り消せない面なので、取消のボタンを持つ", () => {
    expect(render(BoolEditor, "true", PLAIN.Bool)).toContain(">取消<");
  });

  it("同じ型の面が 1 画面に 2 つ現れても、群名が食い違わない（要件 5.5）", () => {
    // 入れ子の `Object` に `Bool` の位置が 2 つ現れると、群名が定数である面は**ブラウザに 1 つの
    // 群**として扱われ、一方を選ぶともう一方の印が外れる — 印が値と食い違い、**書かれる値と
    // 表示がずれる**（7.4 のレビューが実測）。群名は面ごとに一意でなければならない。
    const names = radioGroupNames(renderTwo(BoolEditor, PLAIN.Bool));
    expect(names.length).toBe(4);
    // 前半の 2 つ（1 つ目の面）は同じ群、後半の 2 つ（2 つ目の面）も同じ群、**両者は別**である。
    expect(names[0]).toBe(names[1]);
    expect(names[2]).toBe(names[3]);
    expect(names[0]).not.toBe(names[2]);
  });
});

// ===========================================================================
// 暦（要件 3.2）
// ===========================================================================

describe("暦の並び（DOM を要さない計算）", () => {
  it("月の升は日曜始まりの週に切られ、月の外は 0 で埋まる", () => {
    const weeks = monthGrid(2026, 9);

    // 2026 年 9 月 1 日は火曜である（日曜始まりなので前に 2 升）。
    expect(weeks[0]).toEqual([0, 0, 1, 2, 3, 4, 5]);
    expect(weeks[weeks.length - 1]).toEqual([27, 28, 29, 30, 0, 0, 0]);
    for (const week of weeks) {
      expect(week).toHaveLength(7);
    }
  });

  it("月の日数は暦に従う（2 月と閏年）", () => {
    const days = (year: number, month: number): number =>
      monthGrid(year, month).flat().filter((day) => day !== 0).length;

    expect(days(2026, 9)).toBe(30);
    expect(days(2026, 2)).toBe(28);
    expect(days(2028, 2)).toBe(29);
    expect(days(2026, 12)).toBe(31);
  });

  it("月の送りは年をまたいで正の範囲へ正規化される", () => {
    expect(shiftMonth({ year: 2026, month: 12 }, 1)).toEqual({ year: 2027, month: 1 });
    expect(shiftMonth({ year: 2026, month: 1 }, -1)).toEqual({ year: 2025, month: 12 });
  });

  it("正準表記だけを読み、それ以外は読まない", () => {
    expect(composeDate(2026, 9, 5)).toBe("2026-09-05");
    expect(parseDateText("2026-09-05")).toEqual({ year: 2026, month: 9, day: 5 });
    expect(parseDateText("2026-9-5")).toBeNull();
    expect(parseDateText("2026-09-05T00:00:00")).toBeNull();
    expect(parseDateText("")).toBeNull();
    expect(parseDateText("2026-13-05")).toBeNull();
  });
});

describe("暦の面", () => {
  it("初期値の月を開き、その日を選んだ印を付ける", () => {
    const markup = render(DateEditor, "2026-09-15", PLAIN.Date);

    expect(markup).toContain("2026年9月");
    // 月の升は 30 あり、それぞれが**確定する正準表記を `value` として持つ**。
    expect(buttonTags(markup).filter((tag) => tag.includes('value="2026-09-'))).toHaveLength(30);
    expect(buttonWithValue(markup, "2026-09-15")).toContain('aria-current="date"');
    expect(buttonWithValue(markup, "2026-09-01")).not.toContain("aria-current");
  });

  it("前後の月へ送る道と、日曜始まりの見出しを持つ", () => {
    const markup = render(DateEditor, "2026-09-15", PLAIN.Date);

    expect(markup).toContain('aria-label="前の月"');
    expect(markup).toContain('aria-label="次の月"');
    expect(markup).toContain("<th");
  });

  it("値なしを持つ列では、値なしへ戻す道が並ぶ", () => {
    expect(buttonWithValue(render(DateEditor, "2026-09-15", { kind: "Date", nullable: true }), "")).toBeDefined();
    expect(buttonWithValue(render(DateEditor, "2026-09-15", PLAIN.Date), "")).toBeUndefined();
  });

  it("初期値が正準表記でなければ、選んだ日の印は付かない", () => {
    expect(render(DateEditor, "", PLAIN.Date)).not.toContain("aria-current");
  });
});

// ===========================================================================
// 暦と時刻（要件 3.2）
// ===========================================================================

describe("暦と時刻", () => {
  it("正準表記の日付・時刻・末尾（小数とオフセット）へ分解する", () => {
    expect(parseDateTimeText("2026-09-15T10:30:00")).toEqual({
      date: "2026-09-15",
      time: "10:30:00",
      suffix: "",
    });
    expect(parseDateTimeText("2026-09-15T10:30:00.5+09:00")).toEqual({
      date: "2026-09-15",
      time: "10:30:00",
      suffix: ".5+09:00",
    });
    expect(parseDateTimeText("2026-09-15T10:30:00Z")).toEqual({
      date: "2026-09-15",
      time: "10:30:00",
      suffix: "Z",
    });
  });

  it("日付だけの値や別の綴りは読まない", () => {
    expect(parseDateTimeText("2026-09-15")).toBeNull();
    expect(parseDateTimeText("2026-09-15 10:30:00")).toBeNull();
    expect(parseDateTimeText("2026-09-15T10:30")).toBeNull();
    expect(parseDateTimeText("")).toBeNull();
  });

  it("暦の日と打たれた時刻を組み立て直す（末尾は初期値のものを保つ）", () => {
    expect(composeDateTime("2026-09-16", "11:00:00", "")).toBe("2026-09-16T11:00:00");
    expect(composeDateTime("2026-09-16", "11:00:00", ".5+09:00")).toBe("2026-09-16T11:00:00.5+09:00");
  });

  it("初期値の日を選んだ印を付け、時刻を欄に持つ", () => {
    const markup = render(DateTimeEditor, "2026-09-15T10:30:00+09:00", PLAIN.DateTime);

    expect(buttonWithValue(markup, "2026-09-15")).toContain('aria-current="date"');
    expect(inputTags(markup)).toContainEqual(expect.stringContaining('value="10:30:00"'));
  });
});

// ===========================================================================
// 一覧（要件 3.2）
// ===========================================================================

const ENUM_CONSTRAINTS: ColumnConstraints = {
  kind: "Enum",
  nullable: false,
  choices: [
    { value: "ringo", label: "りんご" },
    { value: "mikan", label: "みかん" },
  ],
};

describe("選択肢の一覧", () => {
  it("同じ型の面が 1 画面に 2 つ現れても、群名が食い違わない（要件 5.5）", () => {
    // `Bool` と同じ理由である（群名が定数だと、入れ子の `Object` に `Enum` の位置が 2 つ
    // 現れたときにブラウザが 1 つの群として扱い、印が値と食い違う）。
    const names = radioGroupNames(renderTwo(EnumEditor, ENUM_CONSTRAINTS));
    expect(names.length).toBe(4);
    expect(names[0]).toBe(names[1]);
    expect(names[2]).toBe(names[3]);
    expect(names[0]).not.toBe(names[2]);
  });

  it("渡された選択肢がそのまま並び、その 1 つが選ばれている", () => {
    const markup = render(EnumEditor, "mikan", ENUM_CONSTRAINTS);

    expect(markup).toContain("りんご");
    expect(markup).toContain("みかん");
    expect(checkedValues(markup)).toEqual(["mikan"]);
    // 確定するのは値（人が読むのは名）である。
    expect(inputTags(markup)).toContainEqual(expect.stringContaining('value="ringo"'));
  });

  it("選択肢が渡されていなければ、値をそのまま扱う面へ落ちる（要件 10.4）", () => {
    expect(render(EnumEditor, "そのまま", PLAIN.Enum))
      .toBe(render(TextEditor, "そのまま", PLAIN.Enum));
  });
});

// ===========================================================================
// 参照先の行からの選択（要件 3.8）
// ===========================================================================

const REF_CONSTRAINTS: ColumnConstraints = {
  kind: "Ref",
  nullable: false,
  reference: {
    sheet: "発注明細",
    rows: [
      { id: "01ARZ3NDEKTSV4RRFFQ69G5FAW", label: "発注 1" },
      { id: "01ARZ3NDEKTSV4RRFFQ69G5FAX", label: "発注 2" },
    ],
  },
};

describe("シート間参照", () => {
  it("参照先のシートの行が並び、今の参照先が印される", () => {
    const markup = render(RefEditor, "01ARZ3NDEKTSV4RRFFQ69G5FAX", REF_CONSTRAINTS);

    expect(markup).toContain("発注明細");
    expect(markup).toContain("発注 1");
    expect(markup).toContain("発注 2");
    // 確定するのは行の識別子である（人が読むのは見出しの文字）。
    expect(buttonWithValue(markup, "01ARZ3NDEKTSV4RRFFQ69G5FAX")).toContain('aria-current="true"');
    expect(buttonWithValue(markup, "01ARZ3NDEKTSV4RRFFQ69G5FAW")).not.toContain("aria-current");
  });

  it("参照先が渡されていなければ、値をそのまま扱う面へ落ちる（要件 10.4）", () => {
    expect(render(RefEditor, "01ARZ3NDEKTSV4RRFFQ69G5FAW", PLAIN.Ref))
      .toBe(render(TextEditor, "01ARZ3NDEKTSV4RRFFQ69G5FAW", PLAIN.Ref));
  });

  it("参照先の行が 1 つも無いときも、値をそのまま扱う面へ落ちる（操作の対象が無い）", () => {
    const empty: ColumnConstraints = { kind: "Ref", nullable: false, reference: { sheet: "発注明細", rows: [] } };

    expect(render(RefEditor, "x", empty)).toBe(render(TextEditor, "x", empty));
  });
});

// ===========================================================================
// 入れ子（要件 5.1、5.5）
// ===========================================================================

const NESTED_MEMBERS = [
  { name: "数量", constraints: { kind: "Int", nullable: false } },
  { name: "有効", constraints: { kind: "Bool", nullable: false } },
] as const;

const NESTED_CONSTRAINTS: ColumnConstraints = {
  kind: "Object",
  nullable: false,
  members: NESTED_MEMBERS,
};

describe("入れ子の面", () => {
  it("位置ごとの文字を構造表現から取り出し、組み立て直せる", () => {
    expect(parseNestedText('{"数量":3,"有効":true}', NESTED_MEMBERS)).toEqual(["3", "true"]);
    expect(assembleNestedText(NESTED_MEMBERS, ["3", "true"])).toBe('{"数量":3,"有効":true}');
    // 文字の位置は引用符を付けない（打たれた文字がそのまま値になる）。
    expect(assembleNestedText([{ name: "備考", constraints: { kind: "Text", nullable: false } }], ["そのまま"]))
      .toBe('{"備考":"そのまま"}');
    // 値なし（空の文字列）は「空から立ち上げる」である。
    expect(parseNestedText("", NESTED_MEMBERS)).toEqual(["", ""]);
    // 読めない、または構造でない初期値は `null` — 呼び出し元は既定の面へ落ちる（値を捨てない）。
    expect(parseNestedText("読めない", NESTED_MEMBERS)).toBeNull();
    expect(parseNestedText("42", NESTED_MEMBERS)).toBeNull();
  });

  it("位置ごとの入力手段を**登録簿を通して**選ぶ（要件 10.3）", () => {
    const markup = render(nestedEditor(), '{"数量":3,"有効":true}', NESTED_CONSTRAINTS);

    expect(markup).toContain("数量");
    expect(markup).toContain("有効");
    // 位置の型の札に対応する面が現れる — 数値の面と二値の面である。
    expect(markup).toContain('type="number"');
    expect(markup).toContain(">真<");
    expect(inputTags(markup)).toContainEqual(expect.stringContaining('value="3"'));
  });

  it("位置の宣言が渡されていなければ、値をそのまま扱う面へ落ちる（要件 10.4）", () => {
    expect(render(nestedEditor(), "そのまま", PLAIN.Object))
      .toBe(render(TextEditor, "そのまま", PLAIN.Object));
  });

  it("位置ごとに読めない初期値では、位置ごとの面を出さない（既存の値を捨てない）", () => {
    // 構造表現として読めない値を位置ごとの面で開くと、確定したときに**空の構造で上書き**して
    // しまう。読めないときは値をそのまま扱う面へ落ち、既にある文字を見せる。
    expect(render(nestedEditor(), "そのまま", NESTED_CONSTRAINTS))
      .toBe(render(TextEditor, "そのまま", NESTED_CONSTRAINTS));
  });
});

// ===========================================================================
// 任意の値（Any）
// ===========================================================================

describe("任意の値の面", () => {
  it("複数行をそのまま扱える面である", () => {
    const markup = render(AnyEditor, "1 行目\n2 行目", PLAIN.Any);

    expect(markup).toContain("<textarea");
    expect(markup).toContain("1 行目\n2 行目");
  });
});

// ===========================================================================
// 拡張が届く範囲（要件 10.1、10.3）
// ===========================================================================

describe("拡張の届く範囲", () => {
  it("登録だけで、新しい型の面が入れ子の内側にも現れる（要件 10.1）", () => {
    const CustomEditor: ComponentType<CellEditorProps> = ({ initialText }) =>
      createElement("mark", { "data-custom": "温度" }, initialText);
    // **拡張を混ぜない隔離**（既定の登録簿を汚さない）。入れ子の面は渡された登録簿を使うので、
    // この隔離がそのまま効く — どの登録簿の拡張が内側へ届くかは登録簿を作った側が決める。
    const registry = createBuiltinEditorRegistry();
    registry.register({ kind: "Custom", customTypeId: "com.example.温度", component: CustomEditor, carrier: "text" });

    expect(registry.resolve("Custom", "com.example.温度")).toBe(CustomEditor);

    const members = [
      { name: "気温", customTypeId: "com.example.温度", constraints: { kind: "Custom", nullable: false } },
    ] as const;
    const markup = render(
      createNestedEditor(registry),
      '{"気温":"21"}',
      { kind: "Object", nullable: false, members },
    );

    // 入れ子の内側の面も登録簿から引かれる（型ごとの分岐を書き足さずに拡張が届く）。
    expect(markup).toContain('data-custom="温度"');
    expect(markup).toContain("21");
  });

  it("位置の面は、渡された登録簿のものだけを引く（隔離が漏れない）", () => {
    // 組込の入っていない器を渡せば、位置の面は既定（値をそのまま扱う）になる。
    const markup = render(createNestedEditor(createEditorRegistry()), '{"数量":3,"有効":true}', NESTED_CONSTRAINTS);

    expect(markup).not.toContain('type="number"');
    expect(markup).toContain('value="3"');
  });
});

// `Calendar` は `DateEditor` と `DateTimeEditor` が共有する（どちらの面にも現れる）。
describe("暦の部品", () => {
  it("単独でも描ける（見出しの行を持つ）", () => {
    const markup = renderToStaticMarkup(
      createElement(Calendar, {
        year: 2026,
        month: 9,
        selected: "",
        onShiftMonth: () => {},
        onPick: () => {},
      }),
    );

    expect(markup).toContain("2026年9月");
    expect(markup).toContain("<th");
  });
});

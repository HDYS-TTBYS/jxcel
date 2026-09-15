/**
 * 入れ子の入力の面（tasks.md 7.4。data-grid 要件 5.1, 5.5, 10.3, 10.4）。
 *
 * # 位置ごとの面を、登録簿を通して選ぶ（要件 10.3）
 *
 * `Object` と `Array` の札に対応する面である。位置（[`ColumnMember`]）ごとに**その位置の型の
 * 面を登録簿から引く** — 本 module に「この札ならこの部品」という分岐は無い。型ごとの対応を
 * 知っている唯一の場所は登録簿であり、したがって**ユーザー定義型の位置にも、登録だけで面が
 * 現れる**（要件 10.1。`editors/builtinEditors.test.ts` の最後の検査がそれを実測する）。
 *
 * 本 module が輸出するのは**面を作る口**（[`createNestedEditor`]）である — 位置の面を引く登録簿は
 * **引数で受け取る**。入れ子の内側にどの登録簿の拡張が届くかは、登録簿を作った側が決める
 * （組込の登録簿は `editors/index.ts` が作る）。
 *
 * # 確定する文字は構造表現（JSON）であり、運び手は `SetCells` ではない（**申し送り**）
 *
 * 位置の面が確定すると、本 module は**構造表現**を組み立てて `commit` へ渡す。
 *
 * **この文字は `SetCells` の「打たれた文字」ではない。** `Text` → `object` / `array` の変換の行は
 * 変換の表に無く（`crates/schema-engine/src/coerce/mod.rs` の `_` の腕）、`object` の列が受理する
 * のは `Object` の値だけである（`crates/schema-engine/src/types/mod.rs` の `accepted_variants`）。
 * すなわち入れ子の列では、`SetCells` に文字を載せると**必ず違反になる**。
 *
 * 正しい運び手は `SetNested`（構造表現を運ぶ命令。生成物の `GridEditCommand`）である。ところが
 * `CellEditorProps.commit(text)` は**経路を 1 本しか持たない**（design.md が逐語で固定している）。
 * したがって**文字をどちらの命令へ載せるかを決めるのは画面（8.3）であり、その判断は列の札に
 * よる** — これは要件 10.3「グリッド側に型ごとの分岐を書かない」と真正面から衝突する。
 *
 * 7.4 は設計を勝手に変えられない（`CellEditorRegistration` も `CellEditorProps` も逐語で固定
 * されている）ため、**この衝突を記録して申し送る**: 登録に「確定の文字をどこへ載せるか」の札を
 * 足す設計変更（design.md の改訂）が要る。記録は `research.md` の「7.4 が記録した隙間」にある。
 * それまでは、本 module が渡す文字は**構造表現**であるという事実だけを固定する。
 *
 * # 読めない初期値では、位置ごとの面を出さない（値を捨てない）
 *
 * 位置ごとの面が空から始まると、確定したときに**既存の値が捨てられる**（要件 3.5 の精神に
 * 反する）。したがって [`parseNestedText`] が読めなかったときは、値をそのまま扱う面
 * （[`TextEditor`]）へ落ちる — 既にある文字をそのまま見せて打ち直せる。値なし（空の文字列）は
 * 「読めない」ではなく「空から立ち上げる」であり、位置ごとの面を出す。
 *
 * # 取り消しは位置の面に委ねる
 *
 * 本 module は `取消` のボタンを持たない（位置の面がそれぞれ持つ。キーだけで取り消せる面は
 * ボタンを持たないという `editors/text.tsx` の規律がそのまま効く）。
 */
import { createElement, useState, type ComponentType, type ReactElement } from "react";

import type { CellEditorProps, CellEditorRegistry, ColumnMember } from "../editorRegistry";
import { TextEditor } from "./text";

/**
 * 構造表現から位置ごとの文字を取り出す。
 *
 * - 空の文字列（値なし）は**空から立ち上げる**: 位置ごとに空の文字
 * - 読めて、かつ構造（オブジェクト／並び）であれば、位置ごとの文字（無い位置は空の文字）
 * - 読めない、または構造でない（数・真偽・文字）なら `null` — **呼び出し元は既定の面へ落ちる**
 *   （既存の値を捨てないため）
 *
 * 並び（`Array`）の位置の名は添字の文字列であり、オブジェクトと同じ読み方で足りる
 * （JavaScript の並びは文字列の添字で読める）。
 */
export function parseNestedText(text: string, members: readonly ColumnMember[]): readonly string[] | null {
  if (text === "") {
    return members.map(() => "");
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) {
    return null;
  }

  const positions = parsed as Record<string, unknown>;

  return members.map((member) => valueText(positions[member.name]));
}

/**
 * 位置ごとの文字から構造表現を組み立てる。
 *
 * 位置の型が `Text` なら打たれた文字がそのまま値であり、それ以外は**構造表現として読む**
 * （読めなければ打たれた文字をそのまま入れる — 適合しない値も捨てない。読み直すのは判定層の
 * 領分である。要件 3.5）。
 *
 * 位置の名がすべて 0 起点の添字であるときは並びを組み立てる（`Array` の位置は添字である）。
 */
export function assembleNestedText(members: readonly ColumnMember[], texts: readonly string[]): string {
  const entries = members.map(
    (member, index) => [member.name, memberValue(member, texts[index] ?? "")] as const,
  );
  const isArray = members.every((member, index) => member.name === `${index}`);

  return JSON.stringify(isArray ? entries.map(([, value]) => value) : Object.fromEntries(entries));
}

/**
 * 入れ子の面を作る。**位置の面を引く登録簿を受け取る**（この面だけが登録簿を要する）。
 *
 * 登録簿を module の側から掴まない理由: 組込が入った既定の登録簿は `editors/index.ts` が作り、
 * その module が本 module を取り込む（表に載せる面を作るため）。ここで既定の登録簿を取り込むと
 * **相互の取り込み**になり、どちらを先に評価したかで束縛が未初期化になる（実際に、既定の
 * 登録簿を直に取り込む形で組んだところ、組込が入る前の器を掴んで**すべての位置が既定の面に
 * 落ちた**）。渡された登録簿を使えば、その登録簿に登録された拡張がそのまま位置の面にも現れる
 * （要件 10.1）— 隔離した登録簿を作る検査も、その隔離がそのまま効く。
 */
export function createNestedEditor(registry: CellEditorRegistry): ComponentType<CellEditorProps> {
  return function NestedEditor({ initialText, constraints, commit, cancel }: CellEditorProps): ReactElement {
    const members = constraints.members ?? [];
    const [texts, setTexts] = useState<readonly string[] | null>(() => parseNestedText(initialText, members));

    if (members.length === 0 || texts === null) {
      return <TextEditor initialText={initialText} constraints={constraints} commit={commit} cancel={cancel} />;
    }
    const currentTexts: readonly string[] = texts;

    return (
      <div>
        {members.map((member, index) => (
          <div key={member.name}>
            <span>{member.name}</span>
            {createElement(registry.resolve(member.constraints.kind, member.customTypeId), {
              initialText: currentTexts[index] ?? "",
              constraints: member.constraints,
              commit: (text: string) => {
                const next = currentTexts.map((current, at) => (at === index ? text : current));
                setTexts(next);
                commit(assembleNestedText(members, next));
              },
              cancel,
            })}
          </div>
        ))}
      </div>
    );
  };
}

/** 構造表現の中の 1 つの値を、打ち直せる文字へ写す。 */
function valueText(value: unknown): string {
  if (value === undefined || value === null) {
    return "";
  }
  if (typeof value === "string") {
    return value;
  }

  return JSON.stringify(value);
}

/** 打たれた文字を、位置の型に応じた値へ写す（`Text` は文字のまま、それ以外は構造表現として読む）。 */
function memberValue(member: ColumnMember, text: string): unknown {
  if (member.constraints.kind === "Text") {
    return text;
  }
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

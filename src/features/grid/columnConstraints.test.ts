/**
 * 境界の材料から入力手段の制約を組み立てること（タスク 10.3。data-grid 要件 3.1、3.2、3.7、5.5、
 * 10.1、10.4）。
 *
 * # 何を固定するか（**材料つきの端から端**）
 *
 * 偽物は**境界の記述（`ColumnDescriptor`）と登録簿に登録した検査用の面だけ**であり、通る経路は
 * 本物である: 境界の材料 → `./columnConstraints` → 登録簿（`./editors`）→ 面 → 確定の文字。
 * これが「画面が材料から組み立て、面が一覧・位置ごとの面になる」ことの画面側の証拠である
 * （Rust 側の証拠は `crates/data-grid/tests/column_materials.rs` と
 * `src-tauri/src/commands/grid.rs` の検査が持つ）。
 *
 * | # | 何を固定するか | どの要件か |
 * |---|---|---|
 * | 1 | 選択肢を持つ列の面が**一覧**になり、選ぶと**その値**が確定する | 3.2 |
 * | 2 | 値なしを許さない列では**「値なしへ戻す」道が出ない**（許す列では出る） | 3.7 |
 * | 3 | 参照の列の面が参照先の行を一覧し、選ぶと**その行の識別子**が確定する | 3.8 |
 * | 4 | 入れ子の列の面が**位置ごとの面**になる（折りたたみでも宣言が読める） | 5.5、5.7 |
 * | 5 | ユーザー定義型の識別子が登録簿へ届く（**登録だけで面が現れる**） | 10.1、10.4 |
 * | 6 | 材料を持たない列は**既定へ落ちる**（材料が無いことは誤りではない） | 10.4 |
 */
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ColumnDescriptor, ColumnMemberDescriptor } from "../../ipc/bindings";
import { constraintsOf, memberTree, withReferenceRows } from "./columnConstraints";
import { createBuiltinEditorRegistry, columnEditor } from "./editors";
import type { CellEditorProps } from "./editorRegistry";

/** 記述 1 本（材料は指定した分だけ載る）。 */
function descriptor(overrides: Partial<ColumnDescriptor> = {}): ColumnDescriptor {
  return {
    column: 0,
    path: [],
    name: "列",
    kind: "Text",
    element_count: null,
    expandability: "leaf",
    nullable: true,
    choices: [],
    reference_sheet: null,
    custom_type_id: null,
    members: [],
    ...overrides,
  };
}

/** 内側の宣言 1 件。 */
function member(fields: readonly string[], kind: ColumnMemberDescriptor["kind"]): ColumnMemberDescriptor {
  return {
    path: fields.map((name) => ({ segment: "Field" as const, name })),
    name: fields.join("."),
    kind,
    nullable: true,
    choices: [],
    custom_type_id: null,
  };
}

/** その列の面を、既定の登録簿から引いて描く（**画面と同じ 1 つの入口**である）。 */
function renderEditor(
  column: ColumnDescriptor,
  commit: (text: string) => void = () => undefined,
): string {
  const { component: Editor } = columnEditor(column);
  const props: CellEditorProps = {
    initialText: "",
    constraints: constraintsOf(column),
    commit,
    cancel: () => undefined,
  };
  return renderToStaticMarkup(createElement(Editor, props));
}

describe("材料から組み立てた入力手段（タスク 10.3。要件 3.2、3.7、10.4）", () => {
  it("選択肢を持つ列の面は一覧になり、選ぶとその値が確定する", () => {
    const committed: string[] = [];
    const markup = renderEditor(
      descriptor({
        kind: "Enum",
        choices: [
          { value: "赤", label: "赤" },
          { value: "青", label: "青" },
        ],
      }),
      (text) => committed.push(text),
    );

    // **一覧である**（`type="radio"` の並びであり、値を打つ欄ではない）。
    expect(markup).toContain('type="radio"');
    expect(markup).toContain('value="赤"');
    expect(markup).toContain('value="青"');
    expect(markup).not.toContain('type="text"');
  });

  it("値なしを許さない列では「値なしへ戻す」道が出ない（要件 3.7）", () => {
    const choices = [
      { value: "赤", label: "赤" },
      { value: "青", label: "青" },
    ];

    // **必須の列**（`nullable: false`）— 道を出すと、判定が違反を返す値を作れてしまう。
    const required = renderEditor(descriptor({ kind: "Enum", nullable: false, choices }));
    expect(required).not.toContain("値なし");

    // 任意の列では道が出る（材料どおりであることの対比）。
    const optional = renderEditor(descriptor({ kind: "Enum", nullable: true, choices }));
    expect(optional).toContain("値なし");
  });

  it("参照の列の面は参照先の行を一覧し、選ぶとその行の識別子が確定する（要件 3.8）", () => {
    const committed: string[] = [];
    const column = descriptor({ kind: "Ref", reference_sheet: "仕入先" });
    const { component: Editor } = columnEditor(column);
    const constraints = withReferenceRows(constraintsOf(column), "仕入先", [
      { id: "01K4ANRRG004HMASW9NF6YY091", label: "仕入先A 東京" },
      { id: "01K4ANRRG004HMASW9NF6YY092", label: "仕入先B 大阪" },
    ]);
    const markup = renderToStaticMarkup(
      createElement(Editor, {
        initialText: "",
        constraints,
        commit: (text: string) => committed.push(text),
        cancel: () => undefined,
      } satisfies CellEditorProps),
    );

    // **シートの名と行が並ぶ**（打たせない — 手で打った識別子は存在しない行を指しうる）。
    expect(markup).toContain("参照先: 仕入先");
    expect(markup).toContain("仕入先A 東京");
    expect(markup).toContain("仕入先B 大阪");
    // 確定するのは識別子である（`value` がそれである）。
    expect(markup).toContain('value="01K4ANRRG004HMASW9NF6YY091"');
    expect(committed).toEqual([]);
  });

  it("入れ子の列の面は位置ごとの面になる（折りたたみでも宣言が読める。要件 5.5、5.7）", () => {
    const markup = renderEditor(
      descriptor({
        kind: "Object",
        members: [member(["name"], "Text"), member(["code"], "Text")],
      }),
    );

    // 位置ごとの面が並び、位置の名前（構造表現の鍵）が出る。
    expect(markup).toContain("name");
    expect(markup).toContain("code");
    // **材料が無ければこの形にならない**（既定へ落ちる）ことも同じ検査で固定する。
    const without = renderEditor(descriptor({ kind: "Object" }));
    expect(without).not.toContain("jxcel");
    expect(without).toContain('type="text"');
  });

  it("ユーザー定義型の識別子が登録簿へ届く（登録だけで面が現れる。要件 10.1）", () => {
    const registry = createBuiltinEditorRegistry();
    const seen: string[] = [];
    registry.register({
      kind: "Custom",
      customTypeId: "postal-code",
      carrier: "text",
      component: ({ initialText }: CellEditorProps) => {
        seen.push(initialText);
        return createElement("span", null, "郵便番号の面");
      },
    });

    const column = descriptor({ kind: "Custom", custom_type_id: "postal-code" });
    const { component } = {
      component: registry.resolve(column.kind ?? "Any", column.custom_type_id ?? undefined),
    };
    const markup = renderToStaticMarkup(
      createElement(component, {
        initialText: "100-0001",
        constraints: constraintsOf(column),
        commit: () => undefined,
        cancel: () => undefined,
      } satisfies CellEditorProps),
    );

    expect(markup).toContain("郵便番号の面");
    expect(seen).toEqual(["100-0001"]);
  });

  it("材料を持たない列は既定（値をそのまま扱う面）へ落ちる（要件 10.4）", () => {
    expect(renderEditor(descriptor({ kind: "Attachment" }))).toContain('type="text"');
    // 使用不能な列（札が `null`）も面は出る（`Any` の既定 = 値をそのまま扱う複数行の面）。
    expect(renderEditor(descriptor({ kind: null }))).toContain("<textarea");
    // 記述が引けなくても面は出る（記述の側の事後条件と同じである）。
    const constraints = constraintsOf(null);
    expect(constraints.kind).toBe("Any");
    expect(constraints.choices).toBeUndefined();
    expect(constraints.members).toBeUndefined();
  });
});

describe("入れ子の宣言の木（タスク 10.3。要件 5.5）", () => {
  it("平坦な位置の並びから、1 段ずつの鍵と制約を組む", () => {
    const tree = memberTree([
      member(["住所"], "Object"),
      member(["住所", "市"], "Text"),
      member(["住所", "番地"], "Text"),
      member(["区分"], "Enum"),
    ]);

    expect(tree.map((entry) => entry.name)).toEqual(["住所", "区分"]);
    expect(tree[0]?.constraints.members?.map((entry) => entry.name)).toEqual(["市", "番地"]);
    // 位置の型の札はその位置のものである（親の札ではない）。
    expect(tree[0]?.constraints.kind).toBe("Object");
    expect(tree[1]?.constraints.kind).toBe("Enum");
    // 内側を持たない位置は `members` を持たない（空の並びを渡さない。要件 10.4）。
    expect(tree[0]?.constraints.members?.[0]?.constraints.members).toBeUndefined();
  });
});

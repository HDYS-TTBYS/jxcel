/**
 * セル入力手段の登録簿の契約（tasks.md 7.4。data-grid 要件 3.1, 3.2, 3.8, 10.1〜10.6。
 * design.md「EditorRegistry（拡張点の所有者）」）。
 *
 * 固定するのは 5 つである。
 *
 * 1. **組込の 10 種**が登録簿を通じて引ける（要件 10.2）。どの札がどの入力手段に対応するかを
 *    **成分の同一性**（`resolve("Text") === TextEditor`）で表明する — 「何か関数が返る」では
 *    対応表が入れ替わっても緑になる
 * 2. **全性**（要件 10.4 の事後条件）: 生成物の型の 14 種すべてについて `resolve` が成分を返す。
 *    未登録の札（`Attachment` と、登録の無い `Custom`）は**既定の文字入力へ落ちる**
 * 3. **重複登録の検出**（要件 10.6）: 同じ鍵への 2 度目の登録は `EditorRegistrationError` で
 *    登録側へ報告される。鍵は `(kind, customTypeId)` の対であり、`Custom` だけが
 *    `customTypeId` を伴う（前提条件）
 * 4. **型の札は生成物から取り込む**: 本 module と `editors/` の側に `TypeKindTag` の写しが
 *    無いことを、源の走査と実行時の輸出の両方で確かめる（片方の写しだけが古くなる事故を塞ぐ）
 * 5. **グリッド側に型ごとの分岐が無い**（要件 10.3、`structure.md`「拡張点は所有者と実装者を
 *    分ける」）: 入力手段の成分を名指しする module は登録簿と `editors/` の側だけである
 *
 * # 生成物の型との結び付きは型検査が担う
 *
 * [`TAG_COVERAGE`] は `Record<TypeKindTag, true>` である。`src/ipc/bindings.ts`（6.1 の生成物）
 * に変種が 1 つ増えれば鍵が足りず、**`npm run typecheck` が落ちる** — design.md の Risks
 * 「片方が増えたときに気づけるよう、生成された境界用の型から導く」の実装がこれである。
 * 実行時の並べ上げ（14 種）はこの表の鍵から作るので、件数を書き写す場所は 1 つになる。
 *
 * # 描けない主張は描かない
 *
 * 本 file が表明するのは**登録簿の意味論**（引けるか・落ちるか・報告されるか）だけである。
 * それぞれの入力手段が何を描くかは `editors/builtinEditors.test.ts`、実物の面での操作
 * （打鍵・押下・暦の巡回）は 8.3 の画面と 9.2 の実起動で観測する
 * （`tech.md` の「GUI の主張は実起動で確かめる」）。
 */
import { createElement } from "react";
import type { ComponentType } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { TypeKindTag } from "../../ipc/bindings";
import { EditorRegistrationError, createEditorRegistry } from "./editorRegistry";
import type { CellEditorProps, ColumnConstraints } from "./editorRegistry";
import { createBuiltinEditorRegistry, editorRegistry } from "./editors";
import { AnyEditor } from "./editors/any";
import { BoolEditor } from "./editors/bool";
import { DateEditor } from "./editors/date";
import { DateTimeEditor } from "./editors/datetime";
import { DecimalEditor } from "./editors/decimal";
import { EnumEditor } from "./editors/enum";
import { NumberEditor } from "./editors/number";
import { RefEditor } from "./editors/ref";
import { TextEditor } from "./editors/text";
// **値としての取り込みである**（型だけの取り込みと分けて書く。`src/ipc/client.ts` と同じ書き方）。
// 実行時の輸出に `TypeKindTag` が現れないこと（型は消えること）を、名前空間そのもので確かめる。
import * as registryModule from "./editorRegistry";

/**
 * 生成物の型の並べ上げ。**鍵の過不足は型検査が落とす**（`Record<TypeKindTag, true>`）。
 * 実行時の巡りはこの表の鍵を使う（件数を書き写す場所を 2 つにしない）。
 */
const TAG_COVERAGE: Record<TypeKindTag, true> = {
  Int: true,
  Float: true,
  Decimal: true,
  Text: true,
  Bool: true,
  Date: true,
  DateTime: true,
  Enum: true,
  Ref: true,
  Attachment: true,
  Object: true,
  Array: true,
  Any: true,
  Custom: true,
};

/** 生成物の型の 14 種（宣言の順に近い順で並べたもの）。 */
const ALL_TYPE_KIND_TAGS: readonly TypeKindTag[] = Object.keys(TAG_COVERAGE) as readonly TypeKindTag[];

/**
 * 組込の対応表（要件 10.2）。**登録簿の側の表から導かず、期待をここに逐語で書く** —
 * 表を書き換えたときに期待値も一緒に動くと、対応が入れ替わったことに気づけない。
 */
const BUILTIN_MECHANISMS: readonly (readonly [TypeKindTag, unknown])[] = [
  ["Int", NumberEditor],
  ["Float", NumberEditor],
  ["Decimal", DecimalEditor],
  ["Text", TextEditor],
  ["Bool", BoolEditor],
  ["Date", DateEditor],
  ["DateTime", DateTimeEditor],
  ["Enum", EnumEditor],
  ["Ref", RefEditor],
  ["Any", AnyEditor],
];

/**
 * 入れ子の面は**登録簿に結び付いて作られる**ので、成分そのものをここへ書けない
 * （`editors/index.ts` が登録簿ごとに作る）。対応の期待は「`Object` と `Array` が同じ面を
 * 共有し、それが他の 9 種のどれでもない」ことである（`editors/builtinEditors.test.ts` が
 * その面の振る舞い — 位置の面を登録簿から引くこと — を実測する）。
 */
const NESTED_TAGS: readonly TypeKindTag[] = ["Object", "Array"];

function editorProps(initialText: string, constraints: ColumnConstraints): CellEditorProps {
  return { initialText, constraints, commit: () => {}, cancel: () => {} };
}

const TEXT_CONSTRAINTS: ColumnConstraints = { kind: "Text", nullable: false };

describe("登録簿の意味論（要件 10.1、10.2、10.5）", () => {
  it("組込の札は、対応する入力手段を登録簿を通じて引ける", () => {
    for (const [kind, component] of BUILTIN_MECHANISMS) {
      expect(editorRegistry.resolve(kind)).toBe(component);
    }
  });

  it("既定の登録簿は組込の入った器である（取り込み口を間違えると黙って既定へ落ちる）", () => {
    // `editorRegistry.ts` の器（組込を入れない）と、`editors/index.ts` の登録簿（入っている）は
    // 別のものである。**画面が取り込むのは後者である。**
    const empty = createEditorRegistry();

    expect(editorRegistry.resolve("Int")).toBe(NumberEditor);
    expect(empty.resolve("Int")).toBe(TextEditor);
  });

  it("組込の入力手段は 10 種であり、12 種の札を覆う", () => {
    // **公開面から測る。**14 種の札を引いた結果の**異なり数**が面の数であり、どの札がどの面を
    // 共有するかは成分の同一性である（`Int`/`Float` と `Object`/`Array` がそれぞれ共有する）。
    const resolved = ALL_TYPE_KIND_TAGS.map((kind) => editorRegistry.resolve(kind));
    const mechanisms = new Set(resolved);

    // 14 種のうち 12 種が組込へ、2 種（`Attachment` と `Custom`）が既定へ落ちる。既定は `Text` の
    // 面と同じ成分なので、異なり数はその 2 種を足しても増えない。
    expect(mechanisms.size).toBe(10);

    // 組込の 12 種はそれぞれ違う鍵である（`Object` と `Array` だけが「同じ成分を共有する別の鍵」）。
    expect(resolved.length).toBe(14);
    expect(new Set(NESTED_TAGS.map((kind) => editorRegistry.resolve(kind))).size).toBe(1);
  });

  it("組込の登録は 14 種のうち 12 種であり、残る 2 種は空いている", () => {
    // **登録の有無を重複の報告で測る。**登録簿の公開面（`register` / `resolve`）だけを使うので、
    // 「登録されている」の判定に内部の表を覗く必要がない（design.md の Service Interface に
    // 一覧の口は無い）。
    const registry = createBuiltinEditorRegistry();
    const registeredTags = [...BUILTIN_MECHANISMS.map(([kind]) => kind), ...NESTED_TAGS];
    expect(registeredTags).toHaveLength(12);

    for (const kind of registeredTags) {
      expect(() => registry.register({ kind, component: TextEditor })).toThrow(EditorRegistrationError);
    }
    // 届かない 2 種は空いている（登録が通る）。`Attachment` の実体の選択は本スペックの対象外
    // （requirements.md の Boundary Context）であり、`Custom` は拡張する側が登録する（要件 10.1）。
    expect(() => registry.register({ kind: "Attachment", component: TextEditor })).not.toThrow();
    expect(() => registry.register({ kind: "Custom", customTypeId: "com.example.述語", component: TextEditor }))
      .not.toThrow();
  });

  it("登録した成分が resolve から返る", () => {
    const registry = createEditorRegistry();
    const replacement: ComponentType<CellEditorProps> = () => null;
    registry.register({ kind: "Enum", component: replacement });

    expect(registry.resolve("Enum")).toBe(replacement);
  });
});

describe("全性と既定（要件 3.1、10.2、10.4）", () => {
  it("14 種の札すべてについて、resolve が成分を返す", () => {
    expect(ALL_TYPE_KIND_TAGS).toHaveLength(14);

    for (const kind of ALL_TYPE_KIND_TAGS) {
      const component = editorRegistry.resolve(kind);
      expect(typeof component).toBe("function");
    }
  });

  it("登録の無い札は、値をそのまま扱える既定の入力手段へ落ちる", () => {
    expect(editorRegistry.resolve("Attachment")).toBe(TextEditor);
    expect(editorRegistry.resolve("Custom")).toBe(TextEditor);
    expect(editorRegistry.resolve("Custom", "com.example.温度")).toBe(TextEditor);
  });

  it("既定の入力手段は、打たれた文字をそのまま初期値として持つ", () => {
    const markup = renderToStaticMarkup(
      createElement(editorRegistry.resolve("Attachment"), editorProps("そのまま の 文字", TEXT_CONSTRAINTS)),
    );

    expect(markup).toContain('value="そのまま の 文字"');
  });

  it("resolve は投げない（事後条件）", () => {
    const registry = createEditorRegistry();

    // 空の登録簿でも全性は成り立つ（既定へ落ちる）。
    for (const kind of ALL_TYPE_KIND_TAGS) {
      expect(typeof registry.resolve(kind, "未登録の型")).toBe("function");
    }
    expect(registry.resolve("Text", "余分な customTypeId")).toBe(TextEditor);
  });

  it("Custom 以外の札では customTypeId は鍵にならない", () => {
    expect(editorRegistry.resolve("Bool", "com.example.温度")).toBe(BoolEditor);
  });
});

describe("重複登録の検出（要件 10.6）", () => {
  it("同じ札への 2 度目の登録は、登録側へ報告される", () => {
    const registry = createEditorRegistry();
    registry.register({ kind: "Text", component: TextEditor });

    let reported: unknown = null;
    try {
      registry.register({ kind: "Text", component: AnyEditor });
    } catch (error) {
      reported = error;
    }

    expect(reported).toBeInstanceOf(EditorRegistrationError);
    const failure = reported as EditorRegistrationError;
    expect(failure.reason).toBe("duplicate");
    expect(failure.kind).toBe("Text");
    expect(failure.message).toContain("Text");
    // 1 度目の登録は残る（2 度目が上書きしない）。
    expect(registry.resolve("Text")).toBe(TextEditor);
  });

  it("Custom の鍵は customTypeId ごとに別である", () => {
    const registry = createEditorRegistry();
    registry.register({ kind: "Custom", customTypeId: "com.example.温度", component: NumberEditor });
    registry.register({ kind: "Custom", customTypeId: "com.example.色", component: TextEditor });

    expect(registry.resolve("Custom", "com.example.温度")).toBe(NumberEditor);
    expect(registry.resolve("Custom", "com.example.色")).toBe(TextEditor);
    // 登録の無い Custom は既定へ落ちる（上の 2 件に引きずられない）。
    expect(registry.resolve("Custom", "com.example.重さ")).toBe(TextEditor);
  });

  it("同じ (Custom, customTypeId) の重複は報告される", () => {
    const registry = createEditorRegistry();
    registry.register({ kind: "Custom", customTypeId: "com.example.温度", component: NumberEditor });

    expect(() => registry.register({ kind: "Custom", customTypeId: "com.example.温度", component: TextEditor }))
      .toThrow(EditorRegistrationError);
    expect(registry.resolve("Custom", "com.example.温度")).toBe(NumberEditor);
  });

  it("Custom 以外と Custom の札は別の鍵である", () => {
    const registry = createEditorRegistry();
    registry.register({ kind: "Custom", customTypeId: "Text", component: NumberEditor });
    registry.register({ kind: "Text", component: TextEditor });

    expect(registry.resolve("Text")).toBe(TextEditor);
    expect(registry.resolve("Custom", "Text")).toBe(NumberEditor);
  });
});

describe("前提条件（kind が Custom のときのみ customTypeId を伴う）", () => {
  it("Custom に customTypeId が無ければ報告される", () => {
    const registry = createEditorRegistry();

    let reported: unknown = null;
    try {
      registry.register({ kind: "Custom", component: TextEditor });
    } catch (error) {
      reported = error;
    }

    expect(reported).toBeInstanceOf(EditorRegistrationError);
    expect((reported as EditorRegistrationError).reason).toBe("customTypeIdRequired");
    // 報告は登録を行った側の誤りであり、Custom の鍵は空いたままである。
    expect(registry.resolve("Custom", "com.example.温度")).toBe(TextEditor);
  });

  it("Custom 以外に customTypeId を付けることはできない", () => {
    const registry = createEditorRegistry();

    let reported: unknown = null;
    try {
      registry.register({ kind: "Text", customTypeId: "com.example.温度", component: TextEditor });
    } catch (error) {
      reported = error;
    }

    expect(reported).toBeInstanceOf(EditorRegistrationError);
    expect((reported as EditorRegistrationError).reason).toBe("customTypeIdForbidden");
    expect(registry.resolve("Text")).toBe(TextEditor);
  });
});

describe("型の札は生成物から取り込む", () => {
  it("実行時の輸出に型は現れない（型だけの取り込みである）", () => {
    expect(Object.keys(registryModule)).not.toContain("TypeKindTag");
  });

  it("グリッドの下に TypeKindTag の写しが無い", () => {
    expect(TYPE_KIND_TAG_DECLARATIONS).toEqual([]);
  });

  it("走査そのものが生きている（見本を捕まえる）", () => {
    // 走査が黙って何も見なくなっていないことの確かめ。捕まえるべき見本を流す。
    expect(/(?:export\s+)?type\s+TypeKindTag\b/.test('export type TypeKindTag = "Int" | "Float";')).toBe(true);
    expect(/(?:export\s+)?type\s+TypeKindTag\b/.test('import type { TypeKindTag } from "../../ipc/bindings";'))
      .toBe(false);
  });
});

describe("グリッド側に型ごとの分岐を書かない（要件 10.3）", () => {
  it("入力手段の成分を名指しするのは、登録簿と editors の側だけである", () => {
    // 名指ししてよいのは、既定を持つ所有者（`editorRegistry.ts`）と、組込の実装
    // （`editors/index.ts` と 10 の module）だけである。**画面・窓・描画の側が入力手段の成分を
    // 名指ししたら落ちる** — そのときは `CellEditorRegistry.resolve` を通すこと（要件 10.3）。
    expect(COMPONENT_NAMING_FILES).toEqual([
      "./editorRegistry.ts",
      "./editors/any.tsx",
      "./editors/bool.tsx",
      "./editors/date.tsx",
      "./editors/datetime.tsx",
      "./editors/decimal.tsx",
      "./editors/enum.tsx",
      "./editors/index.ts",
      "./editors/nested.tsx",
      "./editors/number.tsx",
      "./editors/ref.tsx",
      "./editors/text.tsx",
    ]);
  });

  it("走査そのものが生きている（成分の名前を捕まえる）", () => {
    const namesMechanism = /(?<![A-Za-z_$])(?:Text|Number|Decimal|Bool|Date|DateTime|Enum|Ref|Any)Editor\b/;

    expect(namesMechanism.test("const editor = NumberEditor;")).toBe(true);
    expect(namesMechanism.test("const editor = DateTimeEditor;")).toBe(true);
    // 描画層の面（Glide の `DataEditor`）は入力手段ではない。移植口が隠しているものである。
    expect(namesMechanism.test("const surface = DataEditor;")).toBe(false);
    expect(namesMechanism.test("onActivateEditor(position);")).toBe(false);
    // 呼び出しの側の名（`resolve`）は捕まえない — 画面が通すべき道そのものである。
    expect(namesMechanism.test("registry.resolve(kind, customTypeId);")).toBe(false);
  });
});

// ===========================================================================
// 源の走査（本 file だけが使う道具）
//
// `import.meta.glob` は Vite が変換時に解決するので、検査の環境を node の API（`node:fs`）へ
// 結び付けない（`windowCache.test.ts` の「環境が node であること」と同じ方針）。
// ===========================================================================

/** グリッドの面の源（生のテキスト）。鍵は本 file からの相対の道である。 */
const GRID_SOURCES = import.meta.glob("./**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/**
 * 検査の対象（他の検査の file は数えない）。**注釈を落としてある** — 注釈の中の名前は
 * 名指しではない（`renderer/port.ts` は `TypeKindTag` の由来を長い注釈で説明している）。
 */
const GRID_CODE: readonly (readonly [key: string, code: string])[] = Object.entries(GRID_SOURCES)
  .filter(([key]) => !key.includes(".test."))
  .map(([key, source]) => [key, stripComments(source)] as const);

/** `TypeKindTag` を**定義**している源の道（取り込みは定義ではない）。 */
const TYPE_KIND_TAG_DECLARATIONS: readonly string[] = GRID_CODE
  .filter(([, code]) => /(?:export\s+)?type\s+TypeKindTag\b/.test(code))
  .map(([key]) => key);

/**
 * **入力手段の成分**の名前を名指ししている源の道。
 *
 * 綴りを「`*Editor` で終わる識別子」にしない理由: 描画層は Glide Data Grid の面
 * （`DataEditor`）を使う。あれは**入力手段ではなく、canvas の描画面**である（`renderer/port.ts`
 * の移植口が隠しているもの）。したがって綴りは、組込の 10 種の名前そのものに限る。
 */
const COMPONENT_NAMING_FILES: readonly string[] = GRID_CODE
  .filter(([, code]) => /(?<![A-Za-z_$])(?:Text|Number|Decimal|Bool|Date|DateTime|Enum|Ref|Any)Editor\b/.test(code))
  .map(([key]) => key);

/**
 * 注釈を落とす（`/* … *\/` と `// …`）。正規表現の綴りが非自明であり、3 箇所から使うため
 * 名前を与えてある。**素朴な走査であることを認める** — 落としすぎれば見落とす側に倒れる
 * （誤って緑にはならない）。行の注釈は `://` を壊さないよう、直前が `:` でない `/` の対だけを落とす。
 */
function stripComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/(^|[^:])\/\/[^\n]*/g, "$1");
}

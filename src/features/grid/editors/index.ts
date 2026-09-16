/**
 * 組込の入力手段の登録と、**既定の登録簿**（tasks.md 7.4。data-grid 要件 3.1, 3.2, 3.8, 10.1,
 * 10.2, 10.3）。
 *
 * # 型ごとの対応は**この 1 箇所**にある（要件 10.3）
 *
 * 登録簿（`editorRegistry.ts`）は拡張点の所有者であり、**どの札にどの面を付けるかは知らない**。
 * 対応を知っているのは本 module だけである（`structure.md`「拡張点は所有者と実装者を分ける」）。
 * 画面・窓・描画の側は対応を写さない — `CellEditorRegistry.resolve` を通す
 * （`editorRegistry.test.ts` が源の走査で固定する）。
 *
 * # 14 種の札と、12 の登録・10 の面
 *
 * 生成物の `TypeKindTag` は 14 種であり、**すべてを並べ上げる**（漏れを作らない）。
 *
 * | 札 | 面 | 備考 |
 * |---|---|---|
 * | `Int` / `Float` | `number` | 刻みだけが違う（`editors/number.tsx`） |
 * | `Decimal` | `decimal` | 十進であり倍精度へ通さない（`editors/decimal.tsx`） |
 * | `Text` | `text` | 登録簿の**既定でもある**（要件 10.4） |
 * | `Bool` | `bool` | 二値の切り替え（要件 3.2） |
 * | `Date` | `date` | 暦（要件 3.2） |
 * | `DateTime` | `datetime` | 暦と時刻（要件 3.2） |
 * | `Enum` | `enum` | 選択肢の一覧（要件 3.2） |
 * | `Ref` | `ref` | 参照先の行からの選択（要件 3.8） |
 * | `Object` / `Array` | `nested` | 位置ごとの面を登録簿から引く（要件 10.3） |
 * | `Any` | `any` | 値をそのまま扱う複数行の面 |
 * | `Attachment` | **登録しない** | 実体の選択は本スペックの対象外（requirements.md の Boundary Context）。面は既定（値をそのまま扱う）へ落ちる（要件 10.4） |
 * | `Custom` | **登録しない** | 拡張する側（`custom-types`）が `customTypeId` ごとに登録する（要件 10.1） |
 *
 * したがって**登録は 12 件・面は 10 種**である（`Int`/`Float` と `Object`/`Array` が面を共有
 * する）。`editorRegistry.test.ts` はこの数を**公開面（`register` / `resolve`）から測る** —
 * 表の書き写しを期待値にしない（表を書き換えたときに期待値も一緒に動くと、対応が入れ替わった
 * ことに気づけない）。
 *
 * # 既定の登録簿は本 module が持つ（取り込み口は 1 つ）
 *
 * 画面（8.3）と拡張する側（`custom-types`）は、**本 module の `editorRegistry` を共有する**。
 * 別々の登録簿を作ると拡張が片方にしか届かず、また「組込の入っていない器」を使うと
 * **すべての列が既定の面になる**（落ちも警告も出ないので気づけない）。したがって本 module は
 * 組込の入った登録簿だけを輸出し、`editorRegistry.ts` の器は直に輸出しない
 * （器が要るのは、隔離した組を作る検査と、組込を持たない土台を作る将来の用途だけである）。
 *
 * # 入れ子の面だけは、登録簿に結び付けて作る
 *
 * `Object` / `Array` の面は**位置ごとの面を登録簿から引く**ので、登録簿を要する。本 module が
 * 登録簿を作り、その登録簿に結び付いた入れ子の面を作って表へ載せる（`createNestedEditor`）—
 * こうすると、**拡張が入れ子の内側にも届く**（要件 10.1）。
 */
import type { ComponentType } from "react";

import type { ColumnDescriptor } from "../../../ipc/bindings";
import type {
  CellEditorProps,
  CellEditorRegistration,
  CellEditorRegistry,
  EditCarrier,
} from "../editorRegistry";
import { createEditorRegistry } from "../editorRegistry";
import { AnyEditor } from "./any";
import { BoolEditor } from "./bool";
import { DateEditor } from "./date";
import { DateTimeEditor } from "./datetime";
import { DecimalEditor } from "./decimal";
import { EnumEditor } from "./enum";
import { createNestedEditor } from "./nested";
import { NumberEditor } from "./number";
import { RefEditor } from "./ref";
import { TextEditor } from "./text";

/**
 * 組込の登録（上の表そのもの。要件 10.2）。
 *
 * 入れ子の面だけは**登録簿を受け取って作る**ので、引数で受け取る（`nested`）。
 *
 * **運び手（`carrier`）は面ごとに宣言する**（8.5 が足した設計の改訂。`editorRegistry.ts` の
 * [`EditCarrier`]）。`Object` / `Array` の面が確定する文字は**構造表現（JSON）**であり、
 * `SetCells` の「打たれた文字」ではない — `Text` → `object` / `array` の変換の行が
 * `schema-engine` の変換の表に無いため、`SetCells` に載せると**必ず違反になる**（7.4 が実測）。
 * 残る 10 種の面が渡すのは打たれた文字そのものであり、解釈は判定層が行う（要件 3.3）。
 *
 * この 1 箇所が**型と運び手の対応の唯一の源**である（画面も入れ子の面も、自分の運び手を
 * 自分では決めない — 登録簿へ問い合わせる）。
 */
function builtinRegistrations(nested: ComponentType<CellEditorProps>): readonly CellEditorRegistration[] {
  return [
    { kind: "Int", component: NumberEditor, carrier: "text" },
    { kind: "Float", component: NumberEditor, carrier: "text" },
    { kind: "Decimal", component: DecimalEditor, carrier: "text" },
    { kind: "Text", component: TextEditor, carrier: "text" },
    { kind: "Bool", component: BoolEditor, carrier: "text" },
    { kind: "Date", component: DateEditor, carrier: "text" },
    { kind: "DateTime", component: DateTimeEditor, carrier: "text" },
    { kind: "Enum", component: EnumEditor, carrier: "text" },
    { kind: "Ref", component: RefEditor, carrier: "text" },
    { kind: "Object", component: nested, carrier: "structure" },
    { kind: "Array", component: nested, carrier: "structure" },
    { kind: "Any", component: AnyEditor, carrier: "text" },
  ];
}

/**
 * 組込の入った登録簿を新しく作る。
 *
 * 用途は**拡張を混ぜない隔離**である（検査が既定の登録簿へ登録を足すとき）。画面は既定の
 * 登録簿（下の `editorRegistry`）を使う。
 */
export function createBuiltinEditorRegistry(): CellEditorRegistry {
  const registry = createEditorRegistry();
  for (const registration of builtinRegistrations(createNestedEditor(registry))) {
    registry.register(registration);
  }

  return registry;
}

/**
 * 既定の登録簿（組込が入っている）。**画面（8.3）と拡張する側の取り込み口は本 module である。**
 * 拡張する側はここへ登録し、画面はここから引く — 同じ器を共有する（要件 10.1、10.2）。
 */
export const editorRegistry: CellEditorRegistry = createBuiltinEditorRegistry();

/**
 * **列の記述から、その列の入力手段と運び手を 1 度に引く**（8.5 が足した）。
 *
 * この関数が要るのは、画面と詳細表示の**両方**が同じ問いをするためである:「この列を編集する面は
 * 何であり、その面の文字はどの命令へ載せるのか」。2 箇所で別々に引くと、**成分と運び手が別の
 * 登録を指す**経路ができる（面は入れ子の面なのに、文字は `SetCells` へ載る — 必ず違反になる）。
 *
 * 札が読めない列（宣言が壊れている）は `"Any"` として引く（7.4 の事後条件。**面は必ず出る**）。
 * `members`（位置ごとの宣言）は境界に材料が無いので、入れ子の面は位置ごとの面を出さず、値を
 * そのまま扱う面へ落ちる（7.4 の申し送り 6）。
 */
export function columnEditor(column: ColumnDescriptor | null): {
  readonly component: ComponentType<CellEditorProps>;
  readonly carrier: EditCarrier;
} {
  const kind = column?.kind ?? "Any";
  return {
    component: editorRegistry.resolve(kind),
    carrier: editorRegistry.resolveCarrier(kind),
  };
}

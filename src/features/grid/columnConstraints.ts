/**
 * 列の**宣言の材料**から、入力手段が読む制約を組み立てる（タスク 10.3。data-grid 要件 3.1、3.2、
 * 3.7、3.8、5.5、10.1、10.4）。
 *
 * # 材料は境界から来る（7.4・8.3 の申し送りを閉じる）
 *
 * 7.4 は `ColumnConstraints` の 5 つの欄（`kind` / `nullable` / `choices` / `reference` /
 * `members`）を定めたが、境界（`ColumnDescriptor`）が運んでいたのは `kind` だけだった
 * （`editorRegistry.ts` の申し送り 1〜3・5・6）。**本 module がその写しである** — 境界が運ぶ
 * 材料を、面が読む形へ**そのまま**組み替える。
 *
 * | 境界の欄 | 面が読む欄 | どの要件か |
 * |---|---|---|
 * | `kind` | `kind` | 3.1、3.2 |
 * | `nullable` | `nullable` | 3.7 |
 * | `choices` | `choices`（値と名） | 3.2 |
 * | `custom_type_id` | `resolve` / `resolveCarrier` の第 2 引数 | 10.1、10.4 |
 * | `members` | `members`（位置ごとの宣言） | 5.5、5.7 |
 * | `reference_sheet` + 読んだ頁 | `reference` | 3.8 |
 *
 * # 画面は型で分岐しない（要件 10.3）
 *
 * 本 module は**型の札を見て何かを選ぶことをしない** — 材料があるかどうかだけを見る。
 * 「選択肢の一覧を出すのは `Enum` の面である」という対応は登録簿（`editors/index.ts`）が
 * 持ち、ここは材料を渡すだけである（要件 10.3 が「グリッド側に型ごとの分岐を書かない」と
 * 定めるのはこの分担である）。
 *
 * # 材料が無いことは誤りではない（要件 10.4）
 *
 * 空の欄は**渡さない**（`undefined` のままにする）。面はそのとき値をそのまま扱う既定へ落ちる
 * — 既定を消すと、材料の無い列（使用不能な列・拡張の未登録な型）を編集できなくなる。
 */
import type { ColumnDescriptor, ColumnMemberDescriptor } from "../../ipc/bindings";
import type {
  ColumnConstraints,
  ColumnMember,
  ReferenceRow,
  ReferenceSource,
} from "./editorRegistry";

/**
 * 列の宣言から、面が読む制約を組み立てる（要件 3.1、3.2、3.7、5.5、10.1、10.4）。
 *
 * `column` が `null`（記述が引けない）ときは `Any` の既定を返す — 面は必ず出る
 * （`CellEditorRegistry.resolve` の事後条件と同じ規律である）。
 *
 * **参照先の行（`reference`）はここでは入らない。** 行は境界から頁ごとに読むものであり、
 * 材料（`reference_sheet`）だけでは一覧が作れない — 読んだ頁を載せるのは
 * [`withReferenceRows`] である。
 */
export function constraintsOf(column: ColumnDescriptor | null): ColumnConstraints {
  if (column === null) {
    return { kind: "Any", nullable: true };
  }

  // 選択肢は値と名の対で運ばれる（宣言は 1 つの文字列しか持たないため、いまは同じ文字列である。
  // 欄を 2 つにしてあるので、面は値と名を別々に描ける。境界の `ColumnChoice` の doc）。
  const choices = column.choices.map((choice) => ({ value: choice.value, label: choice.label }));
  const members = memberTree(column.members);

  return {
    kind: column.kind ?? "Any",
    // **値なしを許すかは宣言から来る**（定数にしない。要件 3.7）。
    nullable: column.nullable,
    ...(choices.length > 0 ? { choices } : {}),
    ...(members.length > 0 ? { members } : {}),
  };
}

/**
 * 参照先の行を載せた制約を返す（要件 3.8）。
 *
 * `sheet` は境界が運ぶ参照先のシートの名であり（`ColumnDescriptor.reference_sheet`）、
 * `rows` は頁ごとに読んだ行である。**行が 1 つも無いときも載せる** — 「参照先は在るが行が
 * 無い」ことと「参照していない」ことは別であり、面は前者で既定へ落ちる
 * （`editors/ref.tsx` の doc）。
 */
export function withReferenceRows(
  constraints: ColumnConstraints,
  sheet: string,
  rows: readonly ReferenceRow[],
): ColumnConstraints {
  const reference: ReferenceSource = { sheet, rows: [...rows] };
  return { ...constraints, reference };
}

/**
 * 内側の宣言の**平坦な並び**から、位置ごとの木を組む（要件 5.5、5.7）。
 *
 * 境界は内側の位置を**セル直下からの絶対の位置**（`path`）で運ぶ（1 段目も 2 段目も同じ並びに
 * 入る）。面（`editors/nested.tsx`）が要るのは**1 段ずつの鍵と、その位置の制約**であるため、
 * ここで入れ子の形へ組み直す。
 *
 * **位置の並びは親が子より先である**（ドメインが宣言順に積む）。したがってある位置の子は、
 * その位置を接頭辞に持つ位置である — 親を探すために並びを組み替えない。
 *
 * 段が `Index`（並びの添字）である位置は**現れない**（宣言が要素の位置を持たないためである。
 * `data-grid` の `ColumnDeclaration::members` の doc）。現れた場合の鍵は `[i]` である
 * （構造表現の鍵の規約は `editors/nested.tsx` と同じ）。
 */
export function memberTree(members: readonly ColumnMemberDescriptor[]): readonly ColumnMember[] {
  /** `depth` 段目の位置を、その子孫とともに組む。 */
  const childrenOf = (
    subset: readonly ColumnMemberDescriptor[],
    depth: number,
  ): readonly ColumnMember[] =>
    subset
      .filter((member) => member.path.length === depth + 1)
      .map((member) => {
        // ある位置の子孫は、その位置を接頭辞に持ち、より深い位置である。
        const descendants = subset.filter(
          (candidate) =>
            candidate.path.length > member.path.length &&
            member.path.every((segment, index) => {
              const other = candidate.path[index];
              if (other === undefined || other.segment !== segment.segment) {
                return false;
              }
              return segment.segment === "Field"
                ? other.segment === "Field" && other.name === segment.name
                : other.segment === "Index" && other.position === segment.position;
            }),
        );
        const nested = descendants.length > 0 ? childrenOf(descendants, depth + 1) : [];
        const choices = member.choices.map((choice) => ({
          value: choice.value,
          label: choice.label,
        }));
        // **鍵はその位置の段の名前である**（構造表現の鍵。表示名ではない — 表示名は位置を
        // `.` で連結したものであり、親の名前を含む）。
        const last = member.path[member.path.length - 1];
        const name =
          last === undefined || last.segment === "Field" ? (last?.name ?? "") : `[${String(last.position)}]`;
        const constraints: ColumnConstraints = {
          kind: member.kind,
          nullable: member.nullable,
          ...(choices.length > 0 ? { choices } : {}),
          ...(nested.length > 0 ? { members: nested } : {}),
        };

        return {
          name,
          ...(member.custom_type_id === null ? {} : { customTypeId: member.custom_type_id }),
          constraints,
        };
      });

  return childrenOf(members, 0);
}

/**
 * 列の 2 つの空間の写像（tasks.md 8.5。data-grid 要件 5.1、5.2、8.6）。
 *
 * 所有: `ColumnSpace`（design.md「8.5 が確定させたもの」の表。**表示の位置と文書の列を
 * 取り違えないための唯一の場所**である）。
 *
 * # 2 つの空間はなぜ離れるのか（**8.3 の申し送りの決着**）
 *
 * 列には 2 つの添字がある。**同じ数だが、別のものを数える。**
 *
 * | 空間 | 何を数えるか | どこから来るか |
 * |---|---|---|
 * | **表示の位置** | 左から何番目に描かれるか | 生成物の `ColumnDescriptor` の並びの位置（= `RendererSpec.columns` の位置 = `CellPosition.column`） |
 * | **文書の列** | `Row::values()` に対する位置 | 各 `ColumnDescriptor.column`（**窓が運ぶ列そのもの**） |
 *
 * **入れ子の展開が 2 つを離す。**`crates/data-grid/src/view/mod.rs` の `push_column` は、
 * 展開したオブジェクトの**内側の位置**を、**親と同じ文書の列**の下へ並べる（親の列そのものは
 * 積まない）。したがって展開が 1 つでもあると、表示の位置は文書の列と一致しない。
 *
 * **この写像を 1 箇所に閉じる理由**: 恒等を仮定した写像が複数あると、展開が入った日に
 * **片方だけが直り、描かれている値と編集の宛先が別の列を指す**（要件 8.6「取り違えると別の行
 * を編集する」と同じ危険が列にもある）。8.5 は、窓の記憶の列の添字（読み）と編集の宛先
 * （書き）の**両方**をこの 1 つの値から引く（`./windowCache` の `documentColumn`）。
 *
 * **窓が運ぶのは文書の列だけである。**`crates/data-grid/src/transport/mod.rs` の
 * `WindowCodec::encode` は**宣言の列数**ぶんのセルを運び、入れ子の展開は窓の列を変えない
 * （セルが運ぶのは要約であり、構造そのものではない）— したがって `WindowCache.getCell` は
 * 表示の位置を受け取り、この写像で文書の列へ落としてからセルを引く。
 *
 * # 別名ではなく値である理由
 *
 * 表示の位置も文書の列も素の数であるため、型検査は取り違えを防げない（`renderer/port.ts` の
 * `RowOrdinal` / `ColumnIndex` を別名に留めた判断と同じ）。防ぐのは**名前と、写像が 1 箇所で
 * あること**である。
 */
import type { ColumnDescriptor, TypeKindTag } from "../../ipc/bindings";

/**
 * 構成（`ColumnDescriptor` の並び）が定める、表示の位置から文書の列への写像。
 *
 * **構成そのもの（左から右への並び）が表示の位置の空間である** — したがって本型は構成を
 * 別に持たない（持つと、2 つの並びが食い違いうる）。
 */
export interface ColumnSpace {
  /**
   * 表示の位置ごとの**葉の型の札**（表示の位置の順。長さが表示の列の数である）。
   *
   * 描き手へ渡す札（`RenderCell.variant`。数値の列を右寄せにする材料）と、窓の記憶が返す札の
   * 源である。**内側の位置はその位置の札を持つ**ので、展開された入れ子の列でも入力手段が
   * 正しく選ばれる（7.4。`push_column` の `kind_of` が内側の位置も通る）。
   *
   * 使用できない列（`kind` が `null`）は `"Any"` へ落とす — 登録簿の既定（値をそのまま扱う面）
   * へ落ちる札であり、7.4 の事後条件と同じである。
   */
  readonly variants: readonly TypeKindTag[];
  /**
   * その表示の位置が指す**文書の列の添字**。答えられない位置では `null`。
   *
   * `null` は「その位置に列が無い」であり、**推測で答えてはならない**（`WindowCache.rowId` が
   * 未取得の行に `null` を返すのと同じ規律。推測した列は、別の列の値を描く／別の列へ書く）。
   * 整数でない位置・負の位置・列の数の外はすべて `null` である。
   */
  documentColumn(display: number): number | null;
}

/**
 * 構成から写像を作る（**同じ入力からは常に同じ答えが出る**。状態を持たない）。
 *
 * 構成の並びがそのまま表示の位置の空間である（`ColumnDescriptor` の doc「左から右への表示順。
 * 入れ子の展開を含む」）。したがって本関数が行うのは、各位置の `column` を引く口を閉じることと、
 * 札の並びを作ることだけである。
 */
export function createColumnSpace(columns: readonly ColumnDescriptor[]): ColumnSpace {
  const variants: TypeKindTag[] = columns.map((column) => column.kind ?? "Any");

  return {
    variants,

    documentColumn(display: number): number | null {
      // 整数でない位置は引かない（`columns[0.5]` は `undefined` であり、`undefined` を
      // 「無い」として扱うと、範囲の検査を書き忘れた呼び出し側が気づけない）。
      if (!Number.isInteger(display) || display < 0 || display >= columns.length) {
        return null;
      }
      const column = columns[display];
      return column === undefined ? null : column.column;
    },
  };
}

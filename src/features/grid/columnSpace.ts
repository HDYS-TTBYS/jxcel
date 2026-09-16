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
 * # 逆向き（**文書の列 + 内側の位置 → 表示の位置**）もここが持つ
 *
 * 8.4 の提示（バーの「M 列目」と、巡回が移る現在位置）は**表示の位置**でなければならない
 * （`./violations` の module doc）。他方、境界の `GridViolationLocation` が運ぶ列は
 * **文書の列**である — 展開が 1 つでもあると 2 つは一致せず、文書の列を表示の位置として読むと
 * **描かれている列と違う列を名乗り、違うセルへ着く**（境界の修復が露わにした取り違えである）。
 * したがって逆向きも [`ColumnSpace::displayPosition`] として**ここ 1 箇所**に置く。
 *
 * **逆向きは関数ではない**（展開したオブジェクトは複数の表示の位置を 1 つの文書の列へ写す）。
 * 曖昧さを解くのは**内側の位置**であり、規則と、答えられないときの扱いは
 * [`ColumnSpace::displayPosition`] の doc が唯一の源である。
 *
 * # 別名ではなく値である理由
 *
 * 表示の位置も文書の列も素の数であるため、型検査は取り違えを防げない（`renderer/port.ts` の
 * `RowOrdinal` / `ColumnIndex` を別名に留めた判断と同じ）。防ぐのは**名前と、写像が 1 箇所で
 * あること**である。
 */
import type { ColumnDescriptor, GridPathSegment, TypeKindTag } from "../../ipc/bindings";

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
  /**
   * その**文書の列**と**内側の位置**を表示している**表示の位置**。答えられないときは `null`。
   *
   * # この向きだけが曖昧である
   *
   * 表示の位置 → 文書の列は関数である（各位置は `column` を 1 つ持つ）。**逆向きは関数では
   * ない** — 展開したオブジェクトは**複数の表示の位置を同じ 1 つの文書の列へ**写すためである。
   * したがって本関数は両方を要求し、**内側の位置で曖昧さを解く**（8.5 の `sameLayout` が列の
   * 同一性を（`column`, `path`）の対で見るのと同じ考えである）。
   *
   * # 規則（**推測しない**）
   *
   * 1. `documentColumn` が整数でなければ `null`（[`documentColumn`] と同じ入口の検査である）
   * 2. `path` が与えられていれば、**その位置の祖先を表示している列**だけを候補にする — 描かれた
   *    列は、その位置を含む値を表示している。祖先の判定は段ごとの一致であり、**空の位置は
   *    すべての位置の祖先である**（折りたたんだ列は、どの内側の位置の違反も表示している）
   * 3. 候補が**ちょうど 1 つ**なら、それを返す
   * 4. 候補が **0 個**（その文書の列が構成に無い・違反が描かれた列より**上**の値にある）、または
   *    **2 個以上**（`path` が無く、同じ文書の列が複数の位置へ展開されている）なら **`null`**
   *
   * 4 で最初の候補を返さないのは、**それが別のセルを指しうる**ためである（`WindowCache.rowId`
   * が未取得の行に `null` を返すのと同じ規律。推測した位置は、違反していないセルを違反として
   * 名乗り、そこへ現在位置を動かす）。
   *
   * 構成の並びは同じ文書の列の中で**反鎖**である（`crates/data-grid/src/view/mod.rs` の
   * `push_column` は展開した列の内側だけを積み、親の列そのものは積まない）ため、`path` を
   * 与えたときに候補が 2 つ以上になることは無い — それでも 4 を書くのは、**構成の作り方が
   * 変わった日に黙って 1 つを選ばない**ためである。
   */
  displayPosition(documentColumn: number, path?: readonly GridPathSegment[]): number | null;
}

/**
 * 構成から写像を作る（**同じ入力からは常に同じ答えが出る**。状態を持たない）。
 *
 * 構成の並びがそのまま表示の位置の空間である（`ColumnDescriptor` の doc「左から右への表示順。
 * 入れ子の展開を含む」）。したがって本関数が行うのは、2 つの向きの口を閉じること
 * （[`ColumnSpace::documentColumn`] と [`ColumnSpace::displayPosition`]）と、札の並びを作ること
 * だけである。
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

    displayPosition(documentColumn: number, path?: readonly GridPathSegment[]): number | null {
      // 入口の検査は `documentColumn` と同じである（文書の列は 0 起点の整数である）。
      if (!Number.isInteger(documentColumn) || documentColumn < 0) {
        return null;
      }
      // 候補を**数えながら**走る（最初の 1 つを覚えておき、2 つ目が現れた時点で答えない）。
      // 先に配列を作らないのは、この問い合わせが**打鍵と巡回のたび**に来るためである。
      let found: number | null = null;
      for (let display = 0; display < columns.length; display += 1) {
        const column = columns[display];
        if (column === undefined || column.column !== documentColumn) {
          continue;
        }
        if (path !== undefined && !isAncestorPath(column.path, path)) {
          continue;
        }
        if (found !== null) {
          // 2 つ以上が該当する（`path` の無い要求で、展開された列を指した場合である）。
          return null;
        }
        found = display;
      }
      return found;
    },
  };
}

/**
 * `ancestor` が `descendant` の祖先か（**空の位置はすべての位置の祖先である**）。
 *
 * 段の数が祖先の側で多ければ祖先ではない。段ごとに一致を見る（`ancestor` は
 * `descendant` の先頭から続いていなければならない）。段の比較は種類ごとである — 名前と添字は
 * 別のものであり、綴りが同じでも同じ段ではない。
 */
function isAncestorPath(
  ancestor: readonly GridPathSegment[],
  descendant: readonly GridPathSegment[],
): boolean {
  if (ancestor.length > descendant.length) {
    return false;
  }
  for (let step = 0; step < ancestor.length; step += 1) {
    const here = ancestor[step];
    const there = descendant[step];
    if (here === undefined || there === undefined) {
      return false;
    }
    const same =
      here.segment === "Field"
        ? there.segment === "Field" && here.name === there.name
        : there.segment === "Index" && here.position === there.position;
    if (!same) {
      return false;
    }
  }
  return true;
}

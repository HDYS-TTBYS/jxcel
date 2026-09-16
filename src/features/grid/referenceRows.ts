/**
 * 参照先の行を**頁ごとに**読む（タスク 10.3。data-grid 要件 3.8、11 の目的）。
 *
 * # 一度に全部を読まない
 *
 * 参照先は**別のシート**であり、その行数は表示中のシートと無関係である（1 万行の参照先は
 * 普通にありうる）。したがって画面は**頁**を要求する — 1 回の要求が運ぶ件数は
 * [`REFERENCE_PAGE_SIZE`] であり、続きは読んだ行の数だけ `start` を進めて読む
 * （重ならない。`hasMore` が偽になったら止める）。
 *
 * **境界はさらに上限を持つ**（`GRID_REFERENCE_PAGE_LIMIT` = 200）。本 module の頁の大きさは
 * それを超えない（超えても境界が切るが、切られたことに画面が気づかないまま「読んだ」と
 * 数えると頁が重なる — 要求の件数を上限の内側に保つのは画面の責任である）。
 *
 * # 何を読むかは**材料**が決める（要件 10.3）
 *
 * 読む列は `ColumnDescriptor.reference_sheet` を持つ列である（型の札ではない。画面は型で
 * 分岐しない）。参照先のシートは要求に載らない — 要求は**文書の列の添字**だけを運び、
 * シートは宣言から決まる（`grid_reference_rows` の doc）。
 *
 * # 検索の文字を送らない
 *
 * 境界の要求は検索の文字を運ぶ（`GridReferenceRequest.search`）が、**本画面は検索を持たない** —
 * 面（`editors/ref.tsx`）にあるのは行の一覧だけであり、絞り込みの操作は要件 3.8 が求めていない。
 * 送るのは空文字であり、境界とドメインは絞り込み無しとして扱う（規則の実体は
 * `data-grid` の `view::reference` にあり、本 module はそれを使わない）。
 *
 * # 失敗は状態として持つ
 *
 * 頁が読めなかったこと（経路の失敗）を**空の一覧と混同しない**。空の並びは「参照先に行が
 * 無い」であり、失敗は「読めなかった」である。面はどちらも値をそのまま扱う既定へ落ちる
 * （要件 10.4）が、画面は読めなかったことを名乗れる。
 */
import type { GridReferenceResponse } from "../../ipc/bindings";
import type { ReferenceRow } from "./editorRegistry";
import type { GridClient } from "./gridClient";

/**
 * 1 回の要求で読む行数。
 *
 * 境界の上限（`GRID_REFERENCE_PAGE_LIMIT` = 200）の内側である。**上限そのものを使わない**
 * のは、上限が境界の都合（応答の大きさ）で動きうるためである — 画面の頁の大きさは画面が
 * 決め、境界は要求が大きすぎたときに切る（最後の砦である）。
 */
export const REFERENCE_PAGE_SIZE = 100;

/** 参照先の行の読み込みの状態（**進行中・失敗・読了を区別する**）。 */
export type ReferenceRows =
  | { readonly state: "loading" }
  | { readonly state: "failed"; readonly message: string }
  | {
      readonly state: "loaded";
      readonly rows: readonly ReferenceRow[];
      readonly total: number;
      readonly hasMore: boolean;
    };

/**
 * 頁を 1 つ読む（要件 3.8）。**最初の頁と続きの頁で同じ 1 つの入口である。**
 *
 * `previous` が `loaded` なら**その続き**（読んだ行の数だけ `start` を進める）を読み、
 * 行をそのまま繋ぐ。`loading` または `failed` なら最初から読む（`start` は 0）。
 *
 * 失敗（封筒の失敗腕・境界からの拒否）は**同じ 1 つの失敗の状態**へ落とす — 画面が要るのは
 * 「読めたか、読めなかったか、何行あるか」だけである（文言はそのまま出す）。
 */
export async function loadReferenceRows(
  client: GridClient,
  column: number,
  previous: ReferenceRows,
): Promise<ReferenceRows> {
  const loaded = previous.state === "loaded" ? previous : null;
  const answer = await client.readReferenceRows({
    column,
    search: "",
    // **続きは読んだ行の数だけ進める**（読んだ行は頁の先頭からの並びである）。
    start: loaded === null ? 0 : loaded.rows.length,
    count: REFERENCE_PAGE_SIZE,
  });
  if (answer.status === "error") {
    return { state: "failed", message: answer.error.detail.message };
  }

  return {
    state: "loaded",
    rows: [
      ...(loaded === null ? [] : loaded.rows),
      ...rowsOf(answer.data),
    ],
    total: answer.data.total,
    hasMore: answer.data.has_more,
  };
}

/** 応答の行を、面が読む形（識別子と表示の名）へ写す。 */
function rowsOf(page: GridReferenceResponse): readonly ReferenceRow[] {
  return page.rows.map((row) => ({ id: row.id, label: row.label }));
}

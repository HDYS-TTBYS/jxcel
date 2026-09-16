/**
 * 違反の提示と巡回（tasks.md 8.4。data-grid 要件 4.1、4.2、4.3、4.4、4.6）。
 *
 * 所有: バーが出す提示（[`ViolationPresentation`]）と、境界の `grid_find_violation` から
 * それを導く 3 つの関数（[`violationMark`] / [`reasonInRow`] / [`nextViolation`]）。
 * 表示は `./violationBar`、状態への反映は `./GridScreen` の遷移が担う。
 *
 * # 何を担い、何を担わないか
 *
 * | 要件 | 何が源か | 本 module の役割 |
 * |---|---|---|
 * | 4.1 違反しているセルを区別する | 窓の違反の印（`RenderCell.violated`）と、移植口の実装の色 | **印を 3 つの状態へ読むだけ**（[`violationMark`]）。色は決めない |
 * | 4.2 指定したセルの違反の理由 | `GridViolationResponse.reason`（組み立てるのは適応層） | 答えが**いまの行のものか**を確かめてから返す（[`reasonInRow`]） |
 * | 4.3 シート全体の違反の総数 | `grid_set_view` / `grid_apply_edit` の応答 | **触らない**（画面がそのまま持つ。数え直す経路を作らない） |
 * | 4.4 次の違反への移動 | `grid_find_violation`（前向き） | 違反の**可視行の序数**を解決して返す（[`nextViolation`]） |
 * | 4.5 入れ子の内側の違反の位置 | 窓の内側の位置の札と `NestedInspector`（8.5） | **触らない**（`GridViolationLocation.path` は運ぶが、その提示は 8.5 である） |
 * | 4.6 編集で解消されたことの反映 | 編集の結果の総数と、窓の印の取り直し | 総数と提示の取り下げは画面の遷移、**印そのものは窓の記憶（7.3）** である |
 *
 * # 文言は組み立てない
 *
 * [`ViolationPresentation`] が運ぶ `reason` は**適応層が `ViolationReason` / `Expected` から
 * 組み立てた文**である（生成物の `GridViolation` の doc）。本 module は**写すだけ**であり、
 * 2 つ目の文言を作らない（作ると、同じ違反が経路によって別の言い方になる）。
 * 失敗の 1 行（`failed`）だけは `src/ipc/client.ts` の [`describeIpcError`] の写しであり、
 * 画面の告知へ載る。
 *
 * # 位置は**表示の位置**である（境界は文書の列しか運ばない）
 *
 * `GridViolationLocation.column` は**文書の列**（`Row::values()` に対する位置）であり、
 * `crates/data-grid` の索引もその添字で引かれている。他方、返る
 * [`ViolationPresentation.position`] の列は**表示の位置**でなければならない — そのまま
 * `./selection` の `selectionAt` へ渡り（巡回の着地点）、バーが「M 列目」として名乗る
 * （`./violationBar`）。
 *
 * **入れ子の展開が 2 つを離す**（`crates/data-grid/src/view/mod.rs` の `push_column` は内側の
 * 位置を**親と同じ文書の列**の下へ並べる。`./columnSpace` の module doc）。したがって 2 つの
 * 関数は `ColumnSpace.displayPosition`（文書の列 + 内側の位置 → 表示の位置）を通してから返す。
 * **落とせないときは名乗らない** — [`reasonInRow`] は取り下げ（`cleared`）、[`nextViolation`] は
 * **現在位置を動かさずに**失敗を返す（推測した列へ着かない。規則と、答えられない場合の定義は
 * `ColumnSpace.displayPosition` の doc が唯一の源である）。
 *
 * **行は写像しない。**可視行の序数は 1 つの空間しか持たない（行の識別子から序数への解決は
 * 下の「序数の解決」であり、それ以外の行の空間は無い）。
 *
 * # 序数の解決（**境界が可視行の序数を運ばないことへの答え**）
 *
 * `grid_find_violation` の応答が運ぶのは**行の識別子**（26 文字の ULID）と列の添字であり、
 * 可視行の序数は運ばない。他方 [`CellPosition`] の行は可視行の序数であり、その 2 つは
 * **絞り込みと並べ替えの下では一致しない**（`./renderer/port` の module doc）。したがって
 * 違反へ現在位置を移すには、行の識別子から序数への写像が要る。
 *
 * 序数を持つのは順序（`data-grid` の `RowOrder`）だけであり、境界はそれを外へ出さない。
 * 使える問い合わせは `grid_find_violation` の 1 つだけであり、その意味は
 * **「起点の序数以降で最初の違反の行」**である（`from` は両方向で含まれる）。ここから:
 *
 *   - 対象の違反の序数を `t`、起点を `c` とすると（`t` は `c` 以降で最初の違反である）、
 *     `x ∈ [c, t]` の問い合わせはつねに**同じ行**を返す（`t` より前に違反が無いため）
 *   - `x > t` の問い合わせはつねに**別の行**を返す（`x` 以降で最初の違反は `t` より後ろにある）
 *
 * すなわち「返った行が対象と一致する」は `x <= t` と**同値**であり、序数の軸で単調である。
 * [`nextViolation`] はこれを二分探索する（序数は 0..可視行数-1 の整数なので、10 万行でも
 * 17 回で確定する。**行数を走査する経路を作らない**）。
 *
 * **境界が序数を運べばこの探索は 1 回で済む。** 序数は Rust 側の `ViolationIndex::find` が
 * 内部で持っており（`ordinals` の鍵である）、落ちているのは応答の欄だけである — 申し送りと
 * して `design.md`「8.4 が確定させたもの」に記録してある（境界は本タスクの外である）。
 *
 * # 巡回は前向きだけである
 *
 * 要件 4.4 は「次の違反への移動」だけを求める（逆向きは本機能の要件に無い）。逆向きへ広げる
 * ときは、上の単調性が成り立たない（後ろ向きの問い合わせは「起点以下で最後の違反」を返す
 * ため、一致する区間が別の形になる）ので、**探索をそのまま反転してはならない**。
 */
import { describeIpcError } from "../../ipc/client";
import type { ColumnSpace } from "./columnSpace";
import type { GridClient } from "./gridClient";
import type { CellPosition, RenderCell } from "./renderer/port";

/**
 * 窓の違反の印を読んだ結果（要件 4.1）。
 *
 * `unknown` が要るのは、**未取得のセルも `violated: false` で運ばれる**ためである（空白と
 * して描く。`./windowCache` の `getCell`）。真偽の 2 つに畳むと、窓が届く前のセルを
 * 「違反していない」と読むことになり、そのセルの理由を引く機会を失う。
 */
export type ViolationMark = "violated" | "clear" | "unknown";

/** 1 つのセルの印を読む（要件 4.1）。 */
export function violationMark(cell: RenderCell): ViolationMark {
  if (cell.loading) {
    return "unknown";
  }
  return cell.violated ? "violated" : "clear";
}

/**
 * バーが出すもの（要件 4.2、4.3、4.4）。
 *
 * `exhausted` は**失敗ではない** — 「これ以上違反が無い」は探索の正常な結果である
 * （生成物の `GridViolationResponse` の doc。封筒の失敗腕には載せない）。
 */
export type ViolationPresentation =
  | {
      /**
       * 違反の理由（要件 4.2）。`position` は**その理由が属するセル**であり、**表示の位置**で
       * ある（境界が運ぶ文書の列ではない。module doc「位置は表示の位置である」）。
       */
      readonly kind: "reason";
      readonly position: CellPosition;
      readonly reason: string;
    }
  | { readonly kind: "exhausted" };

/**
 * 境界への問い合わせ 1 つぶんの結果。`cleared` は**取り下げる**（出すものが無い）であり、
 * `exhausted`（尽きた）とは意味が違う — 前者は「いまの位置に出す理由が無い」、後者は
 * 「その向きに違反がもう無い」である。
 */
export type ViolationReading =
  | ViolationPresentation
  | { readonly kind: "cleared" }
  | { readonly kind: "failed"; readonly message: string };

/**
 * 巡回が使う境界の口。**狭く取る**（`GridClient` の全体を要求しない — 検査が偽の実装を
 * 置きやすくなる。`./cellEdit` の `Pick` と同じ規律である）。
 */
type ViolationSearcher = Pick<GridClient, "findViolation">;

/**
 * 行の識別子が同じ行を指すか。**綴りの大小は問わない** — 窓から読む識別子と、境界が
 * `Display` で綴る識別子は同じ ULID の 26 文字であるが、7.3 の `invalidate` が同じ理由で
 * 大小を問わない扱いをしており（`windowCache.test.ts` が固定する）、ここだけ厳しくすると
 * **同じ綴りの食い違いが片方だけを壊す**ことになる。
 */
function sameRow(left: string, right: string): boolean {
  return left.toUpperCase() === right.toUpperCase();
}

/**
 * **いまの行**の違反の理由を引く（要件 4.2）。現在位置の行を起点に前向きへ問い合わせ、
 * 返った答えが**いまの行のものか**を確かめてから返す。
 *
 * # なぜ確かめるのか
 *
 * `grid_find_violation` は「起点**以降**で最初の違反」を返すため、いまの行に違反が無ければ
 * **後ろの行の違反**が返る。それをいまの行の理由として出すと、利用者が指したセルと関係の
 * 無い理由を、指したセルの理由として見せることになる（要件 4.2 の提示が事実と食い違う）。
 * 確かめる手段は窓から読む**行の識別子**である（`WindowCache.rowId`）。
 *
 * # 返る位置は「その行の最小の違反列」である
 *
 * 索引は行ごとに**最小の列**の違反しか返さない（`crates/data-grid/src/view/violations.rs`
 * の `find`）。利用者が指した列と違うことがあるので、返る [`ViolationPresentation.position`]
 * は**その理由が属するセル**を名乗る（表示は位置を名乗る。`./violationBar`）。
 * 行の同じセルを指していれば、それがそのまま要件 4.2 の答えである。**名乗る列は表示の位置で
 * あり、境界の文書の列ではない**（module doc「位置は表示の位置である」）。
 *
 * 名乗れる位置が無ければ（展開された親の値そのものの違反など。規則は
 * [`ColumnSpace::displayPosition`] の doc）**取り下げる** — 名乗れない理由を、推測した列の
 * 理由として見せるより、出さない方が事実に合う。
 *
 * 行の識別子が無ければ**問い合わせない**（確かめる手段が無いため。上の理由）。
 */
export async function reasonInRow(options: {
  readonly client: ViolationSearcher;
  /** 現在位置（可視行の序数と列の添字）。 */
  readonly current: CellPosition;
  /** いまの行の識別子（窓から読む。無ければ `null`）。 */
  readonly rowId: string | null;
  /** いまの構成の写像（**境界の列を表示の位置へ落とす唯一の口**）。 */
  readonly space: Pick<ColumnSpace, "displayPosition">;
}): Promise<ViolationReading> {
  if (options.rowId === null) {
    return { kind: "cleared" };
  }

  const answer = await options.client.findViolation({
    from: options.current.row,
    direction: "forward",
  });
  if (answer.status === "error") {
    return { kind: "failed", message: describeIpcError(answer.error) };
  }

  const found = answer.data.violation;
  // 行を持たない違反（列そのものの問題）はどの行のものでもない（`find_violation` の doc）。
  if (found === null || found.location.row === null || !sameRow(found.location.row, options.rowId)) {
    return { kind: "cleared" };
  }
  const column = options.space.displayPosition(found.location.column, found.location.path);
  if (column === null) {
    // **名乗れる位置が無い**（展開された親の値そのものの違反など）。推測した列を名乗ると、
    // バーが**描かれている違反のセルと違うセル**を指す（取り下げておけば、間違った場所を
    // 指すことはない）。
    return { kind: "cleared" };
  }
  return {
    kind: "reason",
    position: { row: options.current.row, column },
    reason: found.reason,
  };
}

/**
 * **次の違反**の位置を求める（要件 4.4）。表示範囲の外にある違反にも到達する
 * （窓を読まない問い合わせだからである — `find_violation` の doc）。
 *
 * 起点（**いまの行の次**）を決めるのは本関数である。いまの行自身を起点にすると、違反の上で
 * 押したときに**同じ違反が返り、現在位置が動かない**（「次の違反へ」が壊れて見える）。
 * 向きは前向きに固定である（module doc「巡回は前向きだけである」）。
 *
 * 返る [`ViolationPresentation.position`] の行は**可視行の序数**であり、そのまま現在位置に
 * できる（行の識別子ではない）。序数の解決は module doc「序数の解決」の二分探索である。
 *
 * **列は表示の位置である**（module doc「位置は表示の位置である」）。名乗れる位置が無ければ
 * **現在位置を動かさずに**失敗を返す — 行を持たない違反を移動先にしないのと同じ規律であり、
 * 推測した列へ動かすと、**違反していないセル**へ現在位置が移り、そこが違反として提示される。
 */
export async function nextViolation(options: {
  readonly client: ViolationSearcher;
  /** 現在位置（可視行の序数と列の添字）。起点はいまの行の**次**である。 */
  readonly current: CellPosition;
  /** 可視行の総数（序数の上限。窓が覆う行数である）。 */
  readonly rowCount: number;
  /** いまの構成の写像（**境界の列を表示の位置へ落とす唯一の口**）。 */
  readonly space: Pick<ColumnSpace, "displayPosition">;
}): Promise<ViolationReading> {
  // 起点は**いまの行の次**である（いまの行の違反は「次の違反」ではない）。
  const from = options.current.row + 1;
  const first = await options.client.findViolation({ from, direction: "forward" });
  if (first.status === "error") {
    return { kind: "failed", message: describeIpcError(first.error) };
  }
  const found = first.data.violation;
  if (found === null) {
    // **正常な結果である**（その向きに違反がもう無い）。
    return { kind: "exhausted" };
  }
  const rowId = found.location.row;
  if (rowId === null) {
    // 行を持たない違反は移動先にならない（総数には数える — `violation_total` の doc）。
    // ここへ来る経路は無い（索引は行に属する違反しか返さない）が、生成物の型は `null` を
    // 許すので、**推測した行へ動かす**道を作らない。
    return { kind: "failed", message: "違反の行を特定できませんでした" };
  }

  // 序数の解決（module doc「序数の解決」）。問い合わせは「起点の序数以降で最初の違反」であり、
  // その行が対象と一致するのは起点が対象の序数以下であるときに限る — 単調な述語を二分探索する。
  // `lo` は**最初の問い合わせの起点**であり、そこでは必ず一致する（不変条件）。
  let lo = from;
  let hi = Math.max(lo, options.rowCount - 1);
  while (lo < hi) {
    const mid = lo + Math.ceil((hi - lo) / 2);
    const answer = await options.client.findViolation({ from: mid, direction: "forward" });
    if (answer.status === "error") {
      // 途中で失敗したら**位置を決めない**（部分的な答えから推測しない）。
      return { kind: "failed", message: describeIpcError(answer.error) };
    }
    const probed = answer.data.violation;
    if (probed !== null && probed.location.row !== null && sameRow(probed.location.row, rowId)) {
      lo = mid;
    } else {
      hi = mid - 1;
    }
  }

  const column = options.space.displayPosition(found.location.column, found.location.path);
  if (column === null) {
    // **名乗れる位置が無い違反へは動かさない**（行を持たない違反を移動先にしないのと同じ規律。
    // 動かすと、違反していないセルへ現在位置が移り、そこが違反として提示される）。
    return { kind: "failed", message: "違反の列を特定できませんでした" };
  }

  return {
    kind: "reason",
    position: { row: lo, column },
    reason: found.reason,
  };
}

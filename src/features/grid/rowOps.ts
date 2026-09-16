/**
 * 行の追加・削除・複製の 1 往復（tasks.md 8.6。data-grid 要件 6.1、6.2、6.3、6.5、1.7、8.6）。
 *
 * 所有: `planRowOperation` / `applyRowOperation`（design.md「Components and Interfaces →
 * Frontend Layer」の GridScreen が使う、行の操作の判断と、境界の間の 1 枚）。
 *
 * # 何を担うか（**判断と往復を 1 つの関数に集める**）
 *
 * | 指示 | 送る命令 | どの要件か |
 * |---|---|---|
 * | 追加（位置を指定した 1 行） | `InsertRows`（**可視の序数と数だけ**） | 6.1 |
 * | 削除（選択した行すべて） | `RemoveRows`（**可視の序数の区間**） | 6.2 |
 * | 複製（選択した行と同じ値） | `DuplicateRows`（**可視の序数の区間**） | 6.3 |
 * | 1 画面に収まらない削除 | 送る前に**削除する行数を示して確認を求める** | 6.5 |
 * | 確認への取り消し | **何も送らない** | 6.5 |
 *
 * 判断（[`planRowOperation`]）と往復（[`applyRowOperation`]）を分けてあるのは、**画面の状態を
 * 決めるのに要るものが境界への往復を待たない**ためである — 確認を求めるかどうか、どの位置へ
 * 足すか、どの行を消すかは、可視行の数と表示の位置だけで決まる（`./cellEdit` が判定と反映を
 * 分けているのと同じ形である）。**判断は純粋であり、境界も窓の記憶も触らない**ので、
 * 座標空間（下の「座標空間」）と閾値（下の「削除の確認」）は `node` の環境でそのまま検査できる。
 *
 * # 座標空間は**可視の序数**である（タスク 10.4。要件 8.6）
 *
 * 生成物の `GridEditCommand` は、行の対象（`RemoveRows` / `DuplicateRows`）と挿入の位置
 * （`InsertRows`）を**2 つの空間**で指せる（`GridRowTarget` / `GridRowAnchor`）。本 module は
 * **可視の序数の腕だけ**を使う:
 *
 * | 指示 | 送る腕 | なぜか |
 * |---|---|---|
 * | 追加（現在位置の行の位置へ 1 行） | `{ anchor: "Before", ordinal }`（末尾は `{ anchor: "End" }`） | 画面が指せるのは**表示の位置**だけである |
 * | 削除・複製（選択した行） | `{ target: "Ordinals", from, count }` | 同上。範囲は `RendererSelection` そのものである |
 *
 * **写像は本 module に無い。**可視の序数を文書の識別子・位置へ解くのは**ドメイン**（適用の
 * 直前。`RowOrder` を持つ `data-grid`）ただ 1 つである。画面が写しを持てば、並べ替え・絞り込みの
 * 下で 2 つの写しが食い違い、**利用者が指したのとは別の行を消す**（要件 8.6 の取り違え）。
 *
 * したがって、**窓の記憶が保っている範囲に依らない** — 10 万行のうち 1 画面に収まらない範囲を
 * 選んでも、選択の全体がそのまま送られる（以前は識別子を窓から引き集めていたため、窓が
 * 保っていない行が 1 つでもあれば操作そのものができなかった。tasks.md 10.4）。
 *
 * **確認に出す数と、実際に消える数は一致する**: 範囲は可視の序数であり、ドメインはその区間の
 * 可視行をそのまま解く（[`rowTargets`] が表の外の分を落とすので、範囲はつねに可視行の中にある）。
 *
 * # 足す位置は「現在位置の行の位置」である（**その行の上に入る**）
 *
 * 画面の入口は 1 つである: **現在位置の行の位置へ 1 行**（`InsertRows { at, count: 1 }`）。
 * 表計算の「上に行を挿入」と同じであり、要件 6.1 の「指定された位置」を画面が指せる唯一の
 * 位置が現在位置だからである（行の全体を選んでいても、位置を指すのは現在位置のセルである）。
 * **数はつねに 1** である — まとめて足すのは貼り付けの補充であり、`PasteRange`（8.7 / 8.9）が
 * 担う（ここで数を増やすと、画面が「何行足すか」を決める口を持つことになる）。
 *
 * # 追加した行の既定値の源は宣言である（**画面は値を 1 つも作らない**）
 *
 * 要件 6.1 の「値が与えられていない列には既定値を適用する」の源は**宣言ただ 1 つ**である —
 * `crates/data-grid/src/edit` の `CompiledSchema::default_row()` であり、`InsertRows` は値を
 * 運ばない（生成物の型が位置と数しか持たないことがそれを表している）。画面が既定値を組み立てる
 * 経路は無い（組み立てれば、宣言が既定値を 1 つ持つという 1 つの事実が 2 箇所に現れる）。
 *
 * したがって本 module の責務は**足した行の値が画面に届くようにすること**である: 窓は行の
 * **序数**を鍵にしているので（[`WindowCache`] の module doc「区間の量子化」）、行が増えた後に
 * 古い窓を配り続けると、**増えた行の値は永久に届かない**（読み込み中のままである）。適用の
 * あとに行数を渡して記憶を捨てるのがその答えであり、既定値は取り直した窓が運ぶ。
 *
 * # 削除の確認の閾値は「いま 1 画面に見えている行数」である（要件 6.5）
 *
 * 要件 6.5 は「削除の対象が 1 つの画面に収まらない行数であるとき、削除する行数を提示したうえで
 * 確認を求める」と定める。**仮想化されたグリッドに「1 画面」は、描かれている面の高さである** —
 * 表が全体を描くことは無いので、固定の行数を閾値にすると、その数と実際に見えている行が
 * 食い違う（窓の大きさは**先読みの幅**であり、画面の高さではない。
 * `WINDOW_ROWS` は 256 行である）。
 *
 * 閾値の源は**移植口が報せた可視の区間**（`RendererSpec.onVisibleSpanChange` の行数。
 * design.md「8.2 が広げた面」）ただ 1 つである（Glide の `onVisibleRegionChanged` が渡す
 * 矩形の高さがそのまま行数である）。**画面の見えている行数を知らないうちは確認を求める** —
 * 「収まると言えない」ときに尋ねないと、尋ねずに消した行は戻せない（押下が 1 回増えることとの
 * 釣り合いが逆である）。
 *
 * # 確認への取り消しは境界へ何も送らない
 *
 * 取り消しは**計画の腕**（[`RowOperationPlan`] の `cancelled`）であり、**送る腕を持たない** —
 * 送る腕（`send`）に載る命令だけが境界へ行く（[`runRowOperationPlan`] の振り分けがそれである）。
 * `./cellEdit` が取消の腕で `GridClient` を 1 つも触らないのと同じ規律であり、同じ形で固定する
 * （送る腕を呼べば落ちる偽の実装を渡す。`rowOps.test.ts`）。
 *
 * # 行数が変わったら記憶を作り直す（要件 1.7。7.3 の申し送り）
 *
 * 適用が影響を受けた行を持てば、`WindowCache.clear(row_count)` を**その応答の行数で**呼ぶ
 * （画面は数え直さない。design.md「行数が変わる編集は画面が `clear` を呼ぶ」）。`clear` が
 * 要るのは、窓が可視行の**序数**を鍵にしているためである — `invalidate`（影響を受けた行を捨てる）
 * では、**削除された行より後ろの窓が別の行を指したまま残る**。
 *
 * 逆に、**何も変わらなかったときは捨てない**（失敗したとき、影響を受けた行が無いとき）—
 * 捨てれば、届いている窓をすべて失って読み込み中に見えることになる。
 *
 * # 単体テストが観測しないもの（**正直に書く**）
 *
 * ① **文書へ実際に行が足され・消えること**と、**既定値の中身**（`CompiledSchema::default_row`
 * が何を書くか）は Rust 側の契約である（`crates/data-grid` の検査が担う）。本 module は
 * 「何を送ったか」までしか主張しない。② **確認の面が現れ、押下が届くこと**は `node` の環境
 * （DOM なし）では観測できない — 実起動（9.2）と `smoke-port-probe` の領分である。
 */
import type { GridEditCommand, GridEditOutcome, GridRowAnchor } from "../../ipc/bindings";
import { assertNever, describeIpcError } from "../../ipc/client";
import type { GridClient } from "./gridClient";
import type { RendererSelection } from "./renderer/port";
import type { WindowCache } from "./windowCache";

/**
 * 消す（複製する）行の範囲。**表示の位置である**（文書の位置ではない）。
 *
 * `count` を別に持つのは、**確認が数を名乗る**ためである（要件 6.5）。範囲から数え直せるが、
 * 確認を求めた時点の数をそのまま掲げる方が、尋ねた数と消える数が食い違う余地を作らない。
 */
export interface RowTargets {
  /** 対象の先頭の可視行（含む）。 */
  readonly first: number;
  /** 対象の最後の可視行（含む）。 */
  readonly last: number;
  /** 対象の行数（`last - first + 1`）。 */
  readonly count: number;
}

/**
 * 確認を求める対象（**画面の状態が持つ値である**）。
 *
 * 対象そのもの（[`RowTargets`]）と別の名を持つのは、**状態の欄が「確認待ちの対象」である**ことを
 * 読めるようにするためである（`GridScreenState` の `pendingDelete`）。
 */
export type DeleteConfirmation = RowTargets;

/**
 * 画面が指した対象。**表示の位置である**（文書の位置でも行の識別子でもない）。
 *
 * `cancel` がここに在るのは、**確認への取り消しが同じ入口から来る**ためである（確認の面の
 * 「取り消す」は、行の操作の 1 つとして扱う — 別の経路にすると、取消が送る経路へ載っていない
 * ことを検査する場所が無くなる）。
 */
export type RowOperationTarget =
  /** 可視の序数 `at` の行の位置へ 1 行足す（現在位置の行である）。 */
  | { readonly kind: "insert"; readonly at: number }
  /** その範囲の行を消す（**閾値を超えていれば確認を求める**）。 */
  | { readonly kind: "delete"; readonly targets: RowTargets }
  /**
   * 確認の答えとして、その範囲の行を消す。
   *
   * **閾値を見ない**（尋ねるのは 1 度だけである — 確認の答えが新しい確認を生むと、答えた
   * 利用者は同じ問いを繰り返し見ることになる）。それ以外は `delete` と同じ道を通る（対象の
   * 解決と、識別子が引けないときの拒否も同じである）。
   */
  | { readonly kind: "confirmDelete"; readonly targets: RowTargets }
  /** その範囲の行と同じ値の行を足す。 */
  | { readonly kind: "duplicate"; readonly targets: RowTargets }
  /** 確認への取り消し（**境界へは何も送らない**）。 */
  | { readonly kind: "cancel" };

/**
 * 判断の材料。**境界も窓の記憶も持たない**（数だけである）。
 *
 * **表示の指定も識別子の口も要らない。**序数で指すため、送る形は材料から直ちに決まる
 * （2 つの空間を写す口が消えたことが、この型の小ささに現れている。タスク 10.4）。
 */
export interface RowOperationContext {
  /** 可視行の数（表示の順序の行数。挿入の位置の上限である）。 */
  readonly visibleRows: number;
  /**
   * **いま 1 画面に見えている行数**（移植口の知らせ。要件 6.5 の閾値）。
   *
   * `null` は「まだ知らない」である（マウントの直後）。**先読みの幅で代用しない** — それは
   * 画面の高さではなく、代用すると 1 画面に収まらない削除が確認を求めなくなる。
   */
  readonly viewportRows: number | null;
}

/**
 * 可視の序数の半開区間（境界の `GridRowTarget` の `Ordinals` そのものである）。
 *
 * **`from` も `count` も可視の序数・可視行の数である**（文書の位置ではない。要件 8.6）。
 */
export interface RowRange {
  /** 対象の先頭の可視行（含む）。 */
  readonly from: number;
  /** 対象の行数。 */
  readonly count: number;
}

/**
 * 挿入の位置（境界の `GridRowAnchor` のうち、画面が使う 2 つ）。
 *
 * `before` は**可視の序数 `ordinal` の行の直前**であり、`end` は**文書の末尾**である
 * （可視の最後の行の直後ではない。末尾へ足せるのは、現在位置が表の最後の行より後ろを指した
 * ときだけである）。
 */
export type RowSendAnchor =
  | { readonly kind: "before"; readonly ordinal: number }
  | { readonly kind: "end" };

/**
 * 境界へ送る対象。**座標空間は可視の序数である**（文書の位置も行の識別子も現れない）。
 *
 * **取り消しの腕を持たない**ことが「取り消しは送れない」の型の水準の表現である — 取り消しは
 * 計画（[`RowOperationPlan`]）の腕であり、往復へは載らない。
 */
export type RowSendIntent =
  | { readonly kind: "insert"; readonly anchor: RowSendAnchor }
  | { readonly kind: "delete"; readonly range: RowRange }
  | { readonly kind: "duplicate"; readonly range: RowRange };

/**
 * 判断の結果。**3 つの行き先と、何もしない腕である。**
 *
 * | 腕 | 画面は何をするか |
 * |---|---|
 * | `send` | 境界へ 1 命令を送る（[`applyRowOperation`]） |
 * | `confirm` | **送らずに**確認を求める（数を示す。要件 6.5） |
 * | `cancelled` | 確認を取り下げる（**送らない**） |
 * | `nothing` | 何もしない（対象が無い） |
 *
 * **「送らずに理由を提示する」腕は無い**（タスク 10.4 が消した）。画面が指せる対象はつねに
 * 可視の序数であり、それを解けるのはドメインであるため、**写せないことを理由に断る場面が
 * 無くなった**（断れば利用者は操作そのものをできない。それこそが 10.4 の指摘である）。
 */
export type RowOperationPlan =
  | { readonly kind: "send"; readonly intent: RowSendIntent }
  | { readonly kind: "confirm"; readonly confirmation: DeleteConfirmation }
  | { readonly kind: "cancelled" }
  | { readonly kind: "nothing" };

/** 計画の 3 つの行き先（**送る腕だけが境界へ行く**）。 */
export interface RowOperationSinks {
  /** 境界へ送る（**この腕に載った命令だけが文書を変える**）。 */
  readonly send: (intent: RowSendIntent) => void;
  /** 確認を求める（まだ送らない）。 */
  readonly confirm: (confirmation: DeleteConfirmation) => void;
  /** 確認を取り下げる（**送らない**）。 */
  readonly cancel: () => void;
}

/** 選択の行の範囲。**表の外へ出ている分は落とす**（`null` は対象が無い）。 */
export function rowTargets(selection: RendererSelection, visibleRows: number): RowTargets | null {
  // 現在位置は範囲の左上とは限らない（右下から左上へ引いた選択では錨が右下にある。要件 2.3）。
  const first = Math.max(
    0,
    Math.min(selection.range.start.row, selection.range.end.row),
  );
  const last = Math.min(
    Math.max(selection.range.start.row, selection.range.end.row),
    visibleRows - 1,
  );
  if (last < first) {
    // 表が空であるか、選択が表の外にある（寄せが効いていれば起きないが、全域にしておく）。
    return null;
  }
  return { first, last, count: last - first + 1 };
}

/**
 * 削除の確認を求めるか（要件 6.5）。閾値は**いま 1 画面に見えている行数**である。
 *
 * 見えている行数を知らない（`null`）ときは**求める** — 「収まると言えない」ときに尋ねないと、
 * 尋ねずに消した行は戻せない（押下が 1 回増えることとの釣り合いが逆である）。
 */
export function deleteNeedsConfirmation(count: number, viewportRows: number | null): boolean {
  return viewportRows === null || count > viewportRows;
}

/** 計画の材料から、対象を境界へ送る形（または「送らない」）を決める。**全域であり、投げない。** */
export function planRowOperation(
  target: RowOperationTarget,
  context: RowOperationContext,
): RowOperationPlan {
  switch (target.kind) {
    case "cancel":
      // **境界も窓の記憶も触らない**（取り消す対象は既に決まっている）。
      return { kind: "cancelled" };
    case "insert": {
      // 表の外を指す指定は端へ寄せる（末尾の次の位置＝ `at == 行数` は末尾への追加である）。
      const at = Math.max(0, Math.min(target.at, context.visibleRows));
      // **可視の序数で指す**（文書の位置へ写すのはドメインである。要件 8.6）。並べ替えや
      // 絞り込みが効いていても、そのまま送れる。
      const anchor: RowSendAnchor =
        at < context.visibleRows ? { kind: "before", ordinal: at } : { kind: "end" };
      return { kind: "send", intent: { kind: "insert", anchor } };
    }
    case "delete":
    case "confirmDelete":
    case "duplicate": {
      if (target.targets.count <= 0) {
        return { kind: "nothing" };
      }
      if (target.kind === "delete" && deleteNeedsConfirmation(target.targets.count, context.viewportRows)) {
        // **送らずに数を示して尋ねる**（要件 6.5）。尋ねた数は対象から数え直さず、そのまま置く
        // （対象は可視の序数の範囲であり、`rowTargets` が表の外を落としているため、尋ねた数と
        // 実際に解かれる行の数は一致する）。
        return { kind: "confirm", confirmation: target.targets };
      }
      // **尋ねるのは 1 度だけである**（`confirmDelete` は確認の答えであり、閾値を見ない）。
      const range: RowRange = { from: target.targets.first, count: target.targets.count };
      return target.kind === "duplicate"
        ? { kind: "send", intent: { kind: "duplicate", range } }
        : { kind: "send", intent: { kind: "delete", range } };
    }
    default:
      return assertNever(target, "行の操作の対象の分岐が網羅されていない");
  }
}

/**
 * 計画を 3 つの行き先へ振り分ける（**画面の内側では、これが唯一の振り分けである**）。
 *
 * 取り消しが**送る腕へ載らない**ことは、この関数の形が表している（`cancelled` の腕は
 * [`RowOperationSinks.cancel`] を呼び、`send` を呼ばない）。検査は偽の受け口を渡して
 * 「どの腕へ届いたか」を読む（`rowOps.test.ts`）。
 */
export function runRowOperationPlan(plan: RowOperationPlan, sinks: RowOperationSinks): void {
  switch (plan.kind) {
    case "send":
      sinks.send(plan.intent);
      return;
    case "confirm":
      sinks.confirm(plan.confirmation);
      return;
    case "cancelled":
      sinks.cancel();
      return;
    case "nothing":
      return;
    default:
      assertNever(plan, "行の操作の計画の分岐が網羅されていない");
  }
}

/** 往復の結果（**画面が状態を決めるのに要るものだけ**）。 */
export type RowOperationSettlement =
  | {
      readonly status: "applied";
      readonly outcome: GridEditOutcome | null;
      /** **応答を組み立てた時点の世代**（10 進の文字列。`GridEditResponse.generation` そのもの）。 */
      readonly generation: string;
    }
  | { readonly status: "failed"; readonly message: string };

/**
 * 1 命令を境界へ送り、**行数が変わったなら記憶を作り直す**（要件 6.1、6.2、6.3、1.7）。
 *
 * **例外を投げない**（`GridClient` の口は封筒の失敗を値で返し、本 module はそれを 1 行へ写す）。
 * 画面の側は `ScreenBoundary` が捕まえない経路（イベントハンドラと非同期）に居るので、投げない
 * ことがそのまま画面の壊れなさになる。
 *
 * 依存を**狭く取る**: 窓の記憶から要るのは `clear` 1 つだけである（宛先の識別子を引くのは
 * 判断の側であり、判断は記憶を持たない）。
 *
 * `clear` を呼ぶのは**適用が影響を受けた行を持ったとき**だけである（`affected` が空であるのは
 * 「何も書かなかった」であり、行数の対応は正しいままである）。**世代の進み方は本 module が
 * 決めない** — 応答が運ぶ値をそのまま返し、画面が採用する（タスク 10.1。規則を 2 つに割ると、
 * 片方だけが古い記憶を残す日が来る）。
 */
export async function applyRowOperation(options: {
  readonly client: GridClient;
  /** 行数の変化のあとに記憶を捨てる口（要件 1.7）。**適用されなかったときは触らない。** */
  readonly cache: Pick<WindowCache, "clear">;
  readonly intent: RowSendIntent;
}): Promise<RowOperationSettlement> {
  const answer = await options.client.applyEdit(commandOf(options.intent));
  if (answer.status === "error") {
    return { status: "failed", message: describeIpcError(answer.error) };
  }
  const outcome = answer.data.outcome;
  if (outcome !== null && outcome.affected.length > 0) {
    // **行数を渡す。**渡さないと、増えた行は永久に読み込み中のままになり（記憶の行数は
    // 組み立て時にしか決まらない）、減った先は古い窓のまま配られる。
    options.cache.clear(outcome.row_count);
  }
  return { status: "applied", outcome, generation: answer.data.generation };
}

/** 挿入の位置を、生成物の錨へ写す（可視の序数と末尾の 2 つだけである）。 */
function anchorOf(anchor: RowSendAnchor): GridRowAnchor {
  switch (anchor.kind) {
    case "before":
      return { anchor: "Before", ordinal: anchor.ordinal };
    case "end":
      return { anchor: "End" };
    default:
      return assertNever(anchor, "挿入の位置の分岐が網羅されていない");
  }
}

/** 対象を、生成物の編集命令へ写す（**値を運ばない 3 つの命令だけである**）。 */
function commandOf(intent: RowSendIntent): GridEditCommand {
  switch (intent.kind) {
    case "insert":
      // 足す行は**つねに 1 行**である（画面の入口が「現在位置の行の位置へ 1 行」である）。
      // まとめて足す口が要るのは貼り付けの補充であり、そちらは `PasteRange` が担う（8.7、8.9）。
      return { command: "InsertRows", at: anchorOf(intent.anchor), count: 1 };
    case "delete":
      // **可視の序数の区間である。**解くのはドメインであり、本 module は写像を持たない。
      return {
        command: "RemoveRows",
        target: { target: "Ordinals", from: intent.range.from, count: intent.range.count },
      };
    case "duplicate":
      return {
        command: "DuplicateRows",
        target: { target: "Ordinals", from: intent.range.from, count: intent.range.count },
      };
    default:
      return assertNever(intent, "行の操作の命令の分岐が網羅されていない");
  }
}

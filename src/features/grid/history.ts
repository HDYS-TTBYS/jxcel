/**
 * 取り消しとやり直しの 1 往復（tasks.md 8.9。data-grid 要件 9.2、9.3、9.8、9.9）。
 *
 * 所有: [`applyHistory`]（design.md「Components and Interfaces → Frontend Layer」の GridScreen が
 * 使う、境界の `grid_history` と画面の間の 1 枚）。
 *
 * # 取り消しとやり直しは 1 つの経路である
 *
 * 利用者にとっては**別々の指示**である（メニューの 2 つの項目、2 つのアクセラレータ）が、
 * 経路は 1 つである — 境界の `grid_history` が向きを運び（生成物の `GridHistoryRequest`）、
 * 本 module の口も 1 つ（[`applyHistory`]）である。**口を 2 つに割らない**理由は、適用のあとの
 * 後始末（行数の作り直しと移動先の解決）が片方だけに載る余地を作らないためである。
 *
 * | 何が起きるか | どの要件か |
 * |---|---|
 * | 直前の操作の前の状態へ戻す（`undo`） | 9.2 |
 * | 取り消した操作を再び適用する（`redo`） | 9.3 |
 * | 対象となった範囲へ現在位置を移す（**表示の序数**で） | 9.8 |
 * | メニューの活性化を 1 つの入口へ引き渡す | 9.9 |
 *
 * # 適用のあとの作り直し（要件 1.7。8.3 / 8.6 / 8.7 と同じ 1 つである）
 *
 * 影響を受けた行があれば **`WindowCache.clear(row_count)`** を**応答の行数で**呼ぶ。取り消しは
 * **行数を変えうる**（行の追加・削除・複製・貼り付けの補充の逆命令がそれである）ので、
 * `invalidate`（影響を受けた行の窓を捨てる）では足りない — 捨てても、削除された行より後ろの窓は
 * **別の行を指したまま残る**（`WindowCache.clear` の doc）。画面は数を数え直さない（応答が運ぶ
 * `GridEditOutcome.row_count` がそのまま新しい行数である）。
 *
 * # 移動先は**応答が運ぶ**（要件 9.8。10.5 が窓の記憶から移した）
 *
 * 「取り消しまたはやり直しの後、対象となった範囲へ現在位置が移り、変更された箇所が見える」ため
 * には、**影響を受けた行の表示の序数**が要る。序数は**応答が運ぶ**（`GridEditOutcome.affected_ordinals`。
 * 写すのは `RowOrder` を持つ適応層であり、画面は写像を持たない）。本 module は序数を**1 つも
 * 解決しない** — 現在位置を移すのは表を描く状態の遷移（`./GridScreen` の `appliedRowOperation`）
 * であり、そこが応答の序数の先頭を使う（移す先が無ければ**動かさない**）。
 *
 * **窓の記憶に依らない。**10.5 より前は `WindowCache.ordinalOf` で `affected` の行を順に引いて
 * いたが、この口は**保っている窓を順に見るだけ**であり、記憶がその行を持っていなければ答えられ
 * なかった（8.9 のレビューが実測した最小の再現は**行の追加のやり直し**である — 行数が変わる
 * 編集の後に記憶は全部を捨てるので、戻ってくる行は 1 つも保たれていない）。序数が応答に載った
 * いま、**「捨てる前に引く」という順序も、`ordinalOf` への依存も無い** — 本 module が記憶から
 * 要るのは行数の作り直し（`clear`）だけである。
 *
 * # 移動した先が見えること（要件 9.8 の後半）
 *
 * **本 module は何もしない。** 移した選択は画面の状態に入り、既存の追随（要件 2.4）が
 * `RendererHandle.setSelection` / `scrollTo` へ渡す（8.4 の「次の違反へ」と同じ道である）。
 * ここでスクロールを起こす経路を別に作ると、`scrollTo` へ何を渡すかが 2 箇所に現れる。
 *
 * # 進める履歴が無いことは失敗ではない（要件 9.2、9.3）
 *
 * 生成物の `GridEditResponse.outcome` の doc が「進める履歴が無かった」を**正常な結果**と
 * 定めている。したがって `null` は失敗の腕に載せず、[`HistorySettlement`] の `empty` にする —
 * 「直前の操作が無い」ことは利用者の操作が失敗したことではない。**何も動かさない**（作り直す
 * 理由も、移動する理由も、告知を出す理由も無い）。
 *
 * # メニューの活性化（要件 9.9）
 *
 * | 段 | どこ |
 * |---|---|
 * | 項目の登録（`編集 > 元に戻す` / `編集 > やり直し`）とアクセラレータ | `src-tauri/src/commands/grid.rs` の `install` |
 * | 活性化を対象ウィンドウへ送る（`emit_to`） | 同（イベント名は [`GRID_HISTORY_REQUESTED_EVENT`]） |
 * | 購読して入口を呼ぶ | **本 module**（[`installGridHistoryRequests`]） |
 * | 往復（`grid_history`）と後始末 | [`applyHistory`] |
 *
 * **2 つの項目が 1 つのイベントを送る。**どちらの項目かは荷（`GridHistoryRequestedEvent.direction`）
 * が運び、本 module はそれを**生成物の閉じた列挙へ解釈**する（解釈できない荷は捨てる — 綴りを
 * 画面が持たない）。したがって**メニューの 2 つの経路と、画面の中の 2 つの操作が、同じ 1 つの
 * 入口へ着く**。
 *
 * **キーボードの経路はアクセラレータである。**画面の側に `Ctrl+Z` を扱う経路は無く（`./selection`
 * の `selectionForKey` は空白と矢印だけを引き受け、`./renderer` の打鍵の聴取は `copy` / `paste`
 * の 2 つだけである）、プラットフォームが活性化として配る。したがって本 module は打鍵を
 * 聴かない — 聴けば、動いている半分を二重に実行する経路を作ることになる（8.7 が貼り付けの
 * 項目を登録しなかった判断と同じ規律である）。
 *
 * # 単体テストが観測しないもの（**正直に書く**）
 *
 * ① **実機の打鍵がメニューの項目を起こすこと**（プラットフォームがアクセラレータを配信する
 * こと）と、② **活性化が実画面へ届き**、`scrollTo` が実際にスクロールを起こすことは、GUI を
 * 要する。観測の場所は 9.2 の台本（3 OS の実起動）であり、`scripts/check-menu-shortcut.sh` の
 * 段は**グリッド画面を開かない**ので静的な期待（項目・荷・綴り）までしか確かめない
 * （`design.md` の「単体テストが観測しないもの（8.9）」）。③ **文書が実際に元へ戻ること**
 * （要件 9.2、9.3 の本体）は Rust 側の契約であり、`crates/data-grid` の検査が担う。
 */
import { listen } from "@tauri-apps/api/event";

import { GRID_HISTORY_REQUESTED_EVENT } from "../../ipc/bindings";
import type { GridEditOutcome, GridHistoryDirection } from "../../ipc/bindings";
import { describeIpcError } from "../../ipc/client";
import type { GridClient } from "./gridClient";
import type { WindowCache } from "./windowCache";

/**
 * 1 往復の結果。**画面が状態を決めるのに要るものだけ**を持つ。
 *
 * `applied` は応答の `outcome`（適用の要約）と世代だけを持つ。**現在位置を移す先はここで
 * 決めない** — 決めるのは表を描く状態の遷移（`./GridScreen` の `appliedRowOperation`）であり、
 * 応答が運ぶ `affected_ordinals` の先頭を使う（要件 9.8。窓の記憶に依らない）。
 */
export type HistorySettlement =
  | { readonly status: "empty" }
  | {
      readonly status: "applied";
      readonly outcome: GridEditOutcome;
      /** **応答を組み立てた時点の世代**（10 進の文字列。`GridEditResponse.generation` そのもの）。 */
      readonly generation: string;
    }
  | { readonly status: "failed"; readonly message: string };

/**
 * 履歴を 1 つ進める（要件 9.2、9.3、9.8、1.7）。
 *
 * **例外を投げない**（`GridClient` の口は封筒の失敗を値で返し、本 module はそれを 1 行へ写す）。
 * 画面の側は `ScreenBoundary` が捕まえない経路（メニューの購読と非同期）に居るので、投げない
 * ことがそのまま画面の壊れなさになる。
 *
 * 依存を**狭く取る**: 窓の記憶から要るのは `clear`（行数の作り直し）**だけ**である
 * （`./cellEdit` の `Pick` と同じ規律）。10.5 より前は移動先の解決に `ordinalOf` も要ったが、
 * 序数は応答が運ぶようになった（module doc「移動先は応答が運ぶ」）。
 */
export async function applyHistory(options: {
  readonly client: GridClient;
  readonly cache: Pick<WindowCache, "clear">;
  readonly direction: GridHistoryDirection;
}): Promise<HistorySettlement> {
  const answer = await options.client.readHistory(options.direction);
  if (answer.status === "error") {
    return { status: "failed", message: describeIpcError(answer.error) };
  }
  const outcome = answer.data.outcome;
  if (outcome === null) {
    // **進める履歴が無い**（要件 9.2、9.3 の正常な結果である）。文書も表示も動いていないので、
    // 作り直す理由も移動する理由も無い。
    return { status: "empty" };
  }
  if (outcome.affected.length > 0) {
    // 行数を渡す（渡さなければ、増えた行は永久に読み込み中のままになり、減った先は古い窓の
    // まま配られる。`WindowCache.clear` の doc）。
    options.cache.clear(outcome.row_count);
  }
  return { status: "applied", outcome, generation: answer.data.generation };
}

/**
 * 境界から届いた荷を向きへ解釈する。解釈できない値は `null`。
 *
 * **閉じた列挙である**（生成物の `GridHistoryDirection` は `"undo"` / `"redo"` の 2 つだけである）。
 * 綴りをここに写すのは、境界の型が**実行時の検証を持たない**ためである（TS の型は消える）—
 * 写しが 1 箇所に閉じていることが、`history.test.ts` の「解釈できない荷は捨てる」で固定される。
 */
export function parseHistoryDirection(payload: unknown): GridHistoryDirection | null {
  if (typeof payload !== "object" || payload === null) {
    return null;
  }
  const value = (payload as { direction?: unknown }).direction;
  return value === "undo" || value === "redo" ? value : null;
}

/**
 * メニューの活性化を受けて履歴を進める入口。**画面の中の 2 つの操作と同じ口**である。
 *
 * 引数は向き（どちらの項目が選ばれたか）であり、本 module はそれを**そのまま**渡す。
 */
export type HistoryEntry = (direction: GridHistoryDirection) => void;

/**
 * メニューからの履歴の要求を購読する。**表の面（`GridSurface`）のマウント時に 1 回だけ呼ぶ。**
 *
 * 返る関数は購読を解除する（React の `useEffect` の後始末）。`installGridCopyRequests` と同じ
 * 2 段（`cancelled` / `unlisten`）であり、解除が先に来た場合は登録完了を待ってから解除する。
 * **失敗を外へ出さない** — 購読の登録に失敗しても（IPC が無い素のブラウザ等）画面は開ける
 * （利用者は画面の中の操作をそのまま使える）。
 *
 * 1 回の活性化につき [`HistoryEntry`] を 1 回呼ぶ（間引かない — 器が送った回数をそのまま写す。
 * 複製の購読と同じ規律である）。
 */
export function installGridHistoryRequests(entry: HistoryEntry): () => void {
  let cancelled = false;
  let unlisten: (() => void) | null = null;

  void (async () => {
    try {
      const stop = await listen(GRID_HISTORY_REQUESTED_EVENT, (event) => {
        const direction = parseHistoryDirection(event.payload);
        if (direction === null) {
          // **投げない**（購読の処理は `ScreenBoundary` の外に居る）。綴りが食い違えば、
          // メニューの活性化が届いたのに何も起きない、という形で現れる。
          console.warn(`履歴の要求を解釈できないため無視する: ${JSON.stringify(event.payload)}`);
          return;
        }
        entry(direction);
      });
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    } catch (error: unknown) {
      console.warn("メニューからの履歴の要求を購読できなかった", error);
    }
  })();

  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/**
 * 実行の面の状態の保持（tasks.md 4.4。要件 2.1、2.2、2.5）。
 *
 * # なぜ React の状態ではなく module の保持なのか
 *
 * **実行は秒単位かかりうる**（既定の時間の上限は 30 秒。要件 6.1）。面の状態を
 * `useState` に置くと、実行の途中で利用者が別の画面へ移った瞬間に（`src/shell/Layout.tsx`
 * は一度に 1 つの画面しか差し込まない）状態が消え、**結果の提示も、変更を適用したあとの
 * 表示の作り直しも失われる**。要件 2.2 は「実行中も表の表示と操作を止めない」ことを求めて
 * おり、それは「実行中に別の画面へ移れる」ことを含む。
 *
 * したがって状態は module の保持（[`createMacroSurfaceStore`]）に置き、面は
 * `useSyncExternalStore` でそれを読む（`src/features/diagnostics/requests.ts` が区画の現在値で
 * 同じ形を採っている）。**実行そのものも中断しない** — 実行の主体は Rust 側の専用スレッドで
 * あり（`crates/macro-runtime/src/engine/actor.rs`）、面は結果を待つだけである。
 *
 * # 実行のあとの表示の作り直し（要件 2.5）
 *
 * 適用のあと、**開いているシートの表示は古い**（`src-tauri/src/commands/grid.rs` の
 * `with_displayed` の doc — 画面が `grid_open_sheet` を呼び直して作り直す）。変更を適用した
 * ことを知っているのは本 module だけなので、**変更があったときに購読者へ 1 回通知する**
 * （[`MacroSurfaceStore.subscribeApplied`]）。作り直す側（グリッド画面）が表示中のシートを
 * 知っているので、**本 module はシートを名指ししない**。
 *
 * # 投げない
 *
 * 入口（[`MacroSurfaceStore.refresh`] / [`MacroSurfaceStore.run`]）は効果とイベントハンドラから
 * 直接呼ばれる。`src/shell/ScreenBoundary.tsx` はイベントハンドラと非同期の失敗を捕まえないので、
 * ここから例外を出さない — 境界の口（[`MacroClient`]）は封筒を返し（`src/ipc/client.ts` の
 * `invokeCommand` は拒否を封筒へ写す）、失敗は面の状態として現れる。
 */
import { describeIpcError } from "../../ipc/client";
import type { MacroClient } from "./macroClient";
import {
  initialMacroSurfaceState,
  macroSurfaceChoiceCancelled,
  macroSurfaceChosen,
  macroSurfaceLoadFailed,
  macroSurfaceLoaded,
  macroSurfacePickRequested,
  macroSurfaceReloadStarted,
  macroSurfaceResultDismissed,
  macroSurfaceRunSettled,
  macroSurfaceRunStarted,
  resultPresentation,
  type MacroSurfaceState,
} from "./surface";

/**
 * 面の状態の保持。**面（[`./MacroPanel`]）はこれだけを通して状態を読む。**
 */
export interface MacroSurfaceStore {
  /** いまの状態（`useSyncExternalStore` の取得関数。変化が無ければ同じ値を返す）。 */
  getState: () => MacroSurfaceState;
  /** 状態の変化の購読（`useSyncExternalStore` の購読関数）。 */
  subscribe: (listener: () => void) => () => void;
  /**
   * 一覧を取り直す（要件 1.3、1.4）。**画面のマウント時と、文書が差し替わったとき**に呼ぶ。
   *
   * 前の答えより後に届いた答えだけを採用する（文書を続けて開いたとき、古い文書の一覧で
   * 新しい文書の面が上書きされないようにする）。
   */
  refresh: () => void;
  /**
   * メニューからの要求を受けた（要件 2.1）。**一覧を取り直し、選ばせる段へ入る。**
   *
   * 要求の本文は読まない（`MACRO_RUN_REQUESTED_EVENT` の荷は無い。`src-tauri/src/commands/macro.rs`
   * の `MenuSelection` の扱い）。
   */
  request: () => void;
  /** 一覧から 1 件を選ぶ（要件 8.2 の能力の提示へ移る）。実行できない 1 件は選べない。 */
  choose: (name: string) => void;
  /** 選択を取り消す（**何も送らない**）。 */
  cancelChoice: () => void;
  /**
   * 選ばれている 1 件を実行する（要件 2.1、2.5）。**結果を待たない**（面と表は実行中も使える）。
   */
  run: () => void;
  /** 直近の結果を閉じる（**文書も値も動かない**）。 */
  dismissResult: () => void;
  /**
   * **変更が適用された**ことの購読（要件 2.5）。購読者は**変更が入ったときだけ**呼ばれる。
   *
   * 呼ぶのは、実行が `Ran` で終わり、かつ変更の件数が 1 件以上であるときだけである —
   * 失敗・打ち切り・変更 0 件では文書が動いておらず、作り直す理由が無い（要件 6.3、7.3）。
   */
  subscribeApplied: (listener: () => void) => () => void;
}

/**
 * 保持を作る。**境界の口は差し替えられる**（検査は偽の実装を渡す。
 * `src/features/grid/GridScreen.tsx` の `loadGridScreenState` と同じ規律）。
 */
export function createMacroSurfaceStore(client: MacroClient): MacroSurfaceStore {
  let state = initialMacroSurfaceState();
  const listeners = new Set<() => void>();
  const applied = new Set<() => void>();
  /** 一覧の読みの世代。**遅れて届いた答えを捨てる**（文書を続けて開いたときの取り違え）。 */
  let listToken = 0;

  const publish = (next: MacroSurfaceState): void => {
    state = next;
    for (const listener of [...listeners]) {
      listener();
    }
  };

  const refresh = (): void => {
    const token = (listToken += 1);
    publish(macroSurfaceReloadStarted(state));
    void client.list().then((answer) => {
      if (token !== listToken) {
        return;
      }
      publish(
        answer.status === "ok"
          ? macroSurfaceLoaded(state, answer.data.macros)
          : macroSurfaceLoadFailed(state, describeIpcError(answer.error)),
      );
    });
  };

  return {
    getState: () => state,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    refresh,
    request: () => {
      refresh();
      publish(macroSurfacePickRequested(state));
    },
    choose: (name) => {
      publish(macroSurfaceChosen(state, name));
    },
    cancelChoice: () => {
      publish(macroSurfaceChoiceCancelled(state));
    },
    run: () => {
      // **実行は 1 つずつである**（要件 2.2 の裏返し。2 つ目は Rust 側も「実行中である」として
      // 断る）。導線は実行中に 1 つも出ないので、ここへ来るのは競合した押下だけである。
      if (state.running !== null) {
        return;
      }
      const name = state.chosen;
      if (name === null) {
        return;
      }
      publish(macroSurfaceRunStarted(state, name));
      void client.run(name).then((answer) => {
        if (answer.status === "error") {
          // **実行そのものが始まらなかった**（経路の失敗。文書は変わっていない）。
          publish(
            macroSurfaceRunSettled(state, name, {
              kind: "rejected",
              message: describeIpcError(answer.error),
            }),
          );
          return;
        }
        const result = resultPresentation(answer.data.outcome);
        publish(macroSurfaceRunSettled(state, name, result));
        if (result.kind === "ran" && result.changed) {
          for (const listener of [...applied]) {
            listener();
          }
        }
      });
    },
    dismissResult: () => {
      publish(macroSurfaceResultDismissed(state));
    },
    subscribeApplied: (listener) => {
      applied.add(listener);
      return () => {
        applied.delete(listener);
      };
    },
  };
}

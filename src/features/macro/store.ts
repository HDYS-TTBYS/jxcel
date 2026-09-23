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
 *
 * # 一覧を求める前に、文書が付いているかを見る（**起動直後の一過性の失敗を出さない**）
 *
 * 一覧は**そのウィンドウの文書の中身**である。したがって文書がまだ関連付いていないウィンドウ
 * （起動直後。パネルは文書の読み込みより先にマウントされる）で `macro_list` を求めると、
 * 呼び出しは経路の失敗として返り、**器はそれを記録に 1 行の失敗として書き**（`log::error!`）、
 * 面は「読み込めませんでした」を一瞬出す。実害は薄い（文書が付いた時点の通知で取り直して
 * 成功する）が、記録の雑音であり、利用者にも失敗が見える。
 *
 * そこで [`MacroSurfaceStore.refresh`] は `macro_list` の前に `document_state` を読む:
 *
 * | 状態 | すること |
 * |------|---------|
 * | `Open` | 一覧を求める（**呼ぶのはこの腕だけである**） |
 * | `Absent` | **何もしない**（一覧は「読み込み中」のまま。文書が付いた時点の通知が取り直す） |
 * | `Unavailable` | 一覧は失敗であり、理由は**文書が読めなかった理由**をそのまま出す |
 *
 * `Absent` を失敗にしないのは、それが**失敗ではなく順序**（文書はこれから付く）だからである。
 * 待つのを解くのは [`./requests`] の [`installMacroListRefresh`]（`DOCUMENT_SESSION_CHANGED_EVENT`
 * の購読）であり、面は要求を足さない。この読みは冪等であり、**文書の解決そのもの**でもある
 * （`src-tauri/src/session/commands.rs` の `document_state` は生成要求の位置を最初のアクセスで
 * 解決する。要件 1.2）ので、起動直後の読みはそのまま `Open` を返す。
 */
import { assertNever, describeIpcError } from "../../ipc/client";
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
   * **文書がまだ関連付いていないときは一覧を求めない**（module doc「一覧を求める前に、文書が
   * 付いているかを見る」）。`document_state` を先に読み、`Open` のときだけ `macro_list` を
   * 呼ぶ — `Absent` は失敗ではなく順序であり、一覧は「読み込み中」のまま待つ。
   *
   * 前の答えより後に届いた答えだけを採用する（文書を続けて開いたとき、古い文書の一覧で
   * 新しい文書の面が上書きされないようにする）。
   */
  refresh: () => void;
  request: () => void;
  choose: (name: string) => void;
  cancelChoice: () => void;
  run: () => void;
  dismissResult: () => void;
  dispatch: (event: MacroSurfaceEvent) => void;
  subscribeApplied: (listener: () => void) => () => void;
}

export type MacroSurfaceEvent =
  | { readonly type: "refresh" }
  | { readonly type: "request" }
  | { readonly type: "choose"; readonly name: string }
  | { readonly type: "cancel-choice" }
  | { readonly type: "run" }
  | { readonly type: "dismiss-result" };

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
    // **一覧の前に文書の状態を読む**（module doc「一覧を求める前に、文書が付いているかを見る」）。
    void client.readDocumentState().then((answer) => {
      if (token !== listToken) {
        return;
      }
      if (answer.status === "error") {
        publish(macroSurfaceLoadFailed(state, describeIpcError(answer.error)));
        return;
      }
      const session = answer.data.status;
      switch (session.state) {
        // **まだ文書が付いていない**（起動直後）。失敗ではないので、一覧は読み込み中のまま
        // 据え置く — 文書が付いた時点の通知（`./requests` の購読）が取り直す。
        case "Absent":
          return;
        // 文書は在るが読めなかった。一覧は求めず（求めても同じ理由で失敗する）、**理由を
        // そのまま**失敗として出す（文言は組み立てない。`./surface` の規律）。
        case "Unavailable":
          publish(macroSurfaceLoadFailed(state, session.reason));
          return;
        case "Open":
          break;
        default:
          return assertNever(session, "文書の状態の分岐が網羅されていない");
      }
      void client.list().then((listed) => {
        if (token !== listToken) {
          return;
        }
        publish(
          listed.status === "ok"
            ? macroSurfaceLoaded(state, listed.data.macros)
            : macroSurfaceLoadFailed(state, describeIpcError(listed.error)),
        );
      });
    });
  };
  const choose = (name: string): void => {
    publish(macroSurfaceChosen(state, name));
  };
  const cancelChoice = (): void => {
    publish(macroSurfaceChoiceCancelled(state));
  };
  const run = (): void => {
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
  };
  const dismissResult = (): void => {
    publish(macroSurfaceResultDismissed(state));
  };
  const dispatch = (event: MacroSurfaceEvent): void => {
    switch (event.type) {
      case "refresh": refresh(); return;
      case "request":
        refresh();
        publish(macroSurfacePickRequested(state));
        return;
      case "choose": choose(event.name); return;
      case "cancel-choice": cancelChoice(); return;
      case "run": run(); return;
      case "dismiss-result": dismissResult(); return;
      default: return assertNever(event, "マクロ操作イベントの分岐が網羅されていない");
    }
  };

  return {
    getState: () => state,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    refresh: () => dispatch({ type: "refresh" }),
    request: () => dispatch({ type: "request" }),
    choose: (name) => dispatch({ type: "choose", name }),
    cancelChoice: () => dispatch({ type: "cancel-choice" }),
    run: () => dispatch({ type: "run" }),
    dismissResult: () => dispatch({ type: "dismiss-result" }),
    dispatch,
    subscribeApplied: (listener) => {
      applied.add(listener);
      return () => {
        applied.delete(listener);
      };
    },
  };
}

/**
 * 診断の画面と、メニューからの要求の引き渡し。
 *
 * 所有: `ShellLayout` へ差し込まれる診断の画面（design.md「Components and Interfaces →
 * Frontend Layer」の画面の契約）。要件: 8.1, 8.6, 8.7（タスク 9.5）。
 *
 * # 何をここに置くか
 *
 * 1. **画面の識別子**（[`DIAGNOSTICS_SCREEN_ID`]）。`src/shell/Layout.tsx` のレジストリが
 *    この識別子で画面を登録する。
 * 2. **メニューからの要求の受け口**（[`installDiagnosticsRequests`]）。Rust 側の 3 つの
 *    メニュー項目は、選択されると活性化の対象ウィンドウへ
 *    `DIAGNOSTICS_REQUESTED_EVENT` を送る（7.4 / 7.5 の経路）。**イベント名は生成物
 *    （`src/ipc/bindings.ts`）の定数だけを参照し、文字列リテラルを書かない。**
 *    受け取った要求は「どの区画を示すか」としてこのモジュールが持ち、遷移は呼び出し側
 *    （シェル）へ委ねる — **遷移の仕組みは `src/shell/router.tsx` の 1 つだけであり、
 *    ここに 2 つ目の遷移機構を作らない**（要件 9.2）。
 * 3. **区画の現在値の配布**（[`useRequestedSection`]）。画面はこれを見て、メニューから
 *    選ばれた区画を示す。
 *
 * # なぜシェルの外（feature 側）に置くか
 *
 * 要求の購読は「画面が表示されていない間もメニューの選択を落とさない」必要がある。したがって
 * 購読の設置は画面のマウントより早い（`src/shell/Layout.tsx` のマウント時）必要があり、
 * 設置の関数と状態はシェルではなく**この機能が所有する**。シェルが受け持つのは
 * 「設置を 1 回呼ぶこと」と「遷移」だけである。
 */
import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";

import { DIAGNOSTICS_REQUESTED_EVENT } from "../../ipc/bindings";
import type {
  DiagnosticsRequestedEvent,
  DiagnosticsSection,
} from "../../ipc/bindings";

/**
 * 診断の画面の識別子。`src/shell/Layout.tsx` のレジストリと、この機能の画面が同じ綴りを
 * 使うための単一の定義である。
 */
export const DIAGNOSTICS_SCREEN_ID = "diagnostics";

/** 要求がまだ無いときに示す区画（画面を直接開いたときの既定）。 */
const DEFAULT_SECTION: DiagnosticsSection = "location";

/** 現在の要求（メニューが最後に選んだ区画）。 */
let requestedSection: DiagnosticsSection = DEFAULT_SECTION;

/** 要求の変化の購読者。 */
const listeners = new Set<() => void>();

/** 現在の要求（`useSyncExternalStore` の取得関数。変化が無ければ同じ値を返す）。 */
export function getRequestedSection(): DiagnosticsSection {
  return requestedSection;
}

/** 要求の変化の購読（`useSyncExternalStore` の購読関数）。 */
export function subscribeRequestedSection(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * 示す区画を要求する。**遷移は行わない**（遷移はシェルの 1 つの入口だけが行う）。
 *
 * 同じ区画を続けて要求しても購読者へは通知しない（画面の再描画を無駄に起こさない）。
 */
export function requestSection(next: DiagnosticsSection): void {
  if (next === requestedSection) {
    return;
  }
  requestedSection = next;
  for (const listener of [...listeners]) {
    listener();
  }
}

/**
 * 画面が示すべき区画。`src/features/diagnostics/DiagnosticsScreen.tsx` が使う。
 */
export function useRequestedSection(): DiagnosticsSection {
  return useSyncExternalStore(
    subscribeRequestedSection,
    getRequestedSection,
    getRequestedSection,
  );
}

/**
 * 境界から届いた要求（`unknown`）を区画へ解釈する。解釈できない値は `null`。
 *
 * **型は実行時の保証ではない**（Rust 側の型が正しくても、購読するイベントの本文は
 * 実行時に何でもありうる）。外観の設定値（`src/shell/theme.ts` の `parseChoice`）と同じ方針で、
 * 解釈できない値は警告して無視する — 画面を誤った区画で開くより、何もしない方が正しい。
 */
function parseSection(payload: unknown): DiagnosticsSection | null {
  if (typeof payload !== "object" || payload === null) {
    return null;
  }
  const value = (payload as { section?: unknown }).section;
  return value === "location" || value === "export" || value === "verbosity"
    ? value
    : null;
}

/**
 * メニューからの要求の購読を設置する。**`src/shell/Layout.tsx` のマウント時に 1 回だけ呼ぶ。**
 *
 * 返る関数は購読を解除する（React の `useEffect` の後始末）。購読の登録は非同期なので、
 * 解除が先に来た場合は登録完了を待ってから解除する。
 *
 * 購読に失敗しても（IPC が無い素のブラウザ等）例外を外へ出さない — 画面はメニュー無しでも
 * 開ける（利用者は画面の中の操作を直接使える）。
 */
export function installDiagnosticsRequests(
  navigate: (destination: string) => void,
): () => void {
  let cancelled = false;
  let unlisten: (() => void) | null = null;

  void (async () => {
    try {
      const stop = await listen<DiagnosticsRequestedEvent>(
        DIAGNOSTICS_REQUESTED_EVENT,
        (event) => {
          const section = parseSection(event.payload);
          if (section === null) {
            console.warn(
              `診断の導線の要求を解釈できないため無視する: ${JSON.stringify(event.payload)}`,
            );
            return;
          }
          // **先に区画を確定させてから遷移する**（画面はマウント時に現在の要求を読むので、
          // 順序が逆だと既定の区画で一瞬描かれる）。
          requestSection(section);
          navigate(DIAGNOSTICS_SCREEN_ID);
        },
      );
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    } catch (error: unknown) {
      console.warn("診断の導線の要求を購読できなかった", error);
    }
  })();

  return () => {
    cancelled = true;
    unlisten?.();
  };
}

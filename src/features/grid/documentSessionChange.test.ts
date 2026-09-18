/**
 * 文書の差し替え・破棄の通知の契約（tasks.md 10.7。data-grid 要件 1.7）。
 *
 * # 何を固定するか
 *
 * 1. **購読は 1 つである**（2026-09-18 の親の裁定）。グリッドの側は購読の写しを持たず、画面
 *    （`./GridScreen.tsx`）は `src/ipc/documentSession.ts` の `installDocumentSessionChanged` を
 *    設置する
 * 2. **宛先は生成物の定数である**（文字列リテラルを書かない。`src/ipc/bindings.ts` の綴りが
 *    変わればこの検査が落ちる）
 * 3. **1 回の通知につき `document_state` を 1 回取り直し、その封筒を入口へ渡す**（間引かない。
 *    取り直しは購読の側の仕事であり、入口は**状態を運ぶ封筒**を受け取る — イベントの本文は
 *    読まない）
 * 4. 後始末は購読を解除し、**解除が登録の完了より先に来ても**取りこぼさない。解除の後に解決した
 *    取り直しの結果は捨てる（アンマウント済みの画面を更新しない）
 * 5. 購読の設置に失敗しても例外を外へ出さない（IPC が無い素のブラウザでも画面は開ける）
 *
 * `listen` と `invoke` を模す（`node` 環境には IPC が無い）。**捉えた引数はそのまま読む**ので、
 * 購読の宛先（イベント名）と登録された処理は本物である（`src/features/macro/requests.test.ts` と
 * 同じ規則）。この検査は、削除した `./documentRequests.test.ts`（購読の写しの module 契約）を
 * **寄せた先の契約を確かめる形へ置き換えたもの**である。
 *
 * # なぜこの file がグリッドの側にあるのか
 *
 * 固定するのは**この画面が依存する契約**（10.7 の購読）であり、検査は元々グリッドの側にあった。
 * `src/ipc` の側に検査基盤を置かない判断（`src/ipc/documentSession.ts` の「完了状態の証明」節。
 * 完了状態は負の型検査で示す）はそのままである。
 *
 * # 限界（正直に書く）
 *
 * **画面の効果そのものは走らない。** `vitest` の環境は `node` であり（`vitest.config.ts`）、
 * `GridScreen` の実体をマウントしない（`renderToStaticMarkup` は効果を実行しない）。したがって
 * 「画面がこの関数を設置している」ことは**源の走査**で固定する — `GridScreen.test.ts` の配色の
 * 走査と同じ規律であり、効果の 1 行は実起動の観測（9.2 の筋書き）が受け取る。
 */
import { describe, expect, it, vi } from "vitest";

import { DOCUMENT_SESSION_CHANGED_EVENT } from "../../ipc/bindings";
import type { DocumentStateResponse } from "../../ipc/bindings";
import type { IpcClientResult } from "../../ipc/client";
import { installDocumentSessionChanged } from "../../ipc/documentSession";

const listen = vi.hoisted(() => vi.fn());
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

/** 微小タスクを 1 巡ぶん流す（`listen` の解決と、取り直しの往復も非同期である）。 */
async function settle(): Promise<void> {
  for (let turn = 0; turn < 10; turn += 1) {
    await Promise.resolve();
  }
}

/** 文書を保持している状態の答え（`document_state` が運ぶ封筒）。 */
function openedState(): IpcClientResult<DocumentStateResponse> {
  return {
    status: "ok",
    data: {
      context: { window: "main" },
      status: {
        state: "Open",
        name: "標本",
        origin: "new",
        unsaved: false,
        revision: 1,
        sheets: [{ id: "s1", name: "標本シート", columns: 1, rows: 3 }],
      },
    },
  };
}

describe("購読の契約（10.7。要件 1.7）", () => {
  it("宛先は生成物の定数であり、通知のたびに `document_state` を取り直して封筒を入口へ渡す", async () => {
    listen.mockReset();
    invoke.mockReset();
    const unlisten = vi.fn();
    listen.mockImplementation(async () => unlisten);
    const opened = openedState();
    invoke.mockResolvedValue(opened);

    const answers: IpcClientResult<DocumentStateResponse>[] = [];
    const stop = installDocumentSessionChanged((result) => {
      answers.push(result);
    });
    await settle();

    // **宛先は生成物の定数である**（名前が変わればこの行が落ちる）。
    expect(DOCUMENT_SESSION_CHANGED_EVENT).toBe("document_session_changed");
    expect(listen).toHaveBeenCalledTimes(1);
    expect(listen.mock.calls[0]?.[0]).toBe(DOCUMENT_SESSION_CHANGED_EVENT);

    // 器が送った通知が、そのたびに**取り直し**へ写る（1 回の通知につき 1 回。間引かない）。
    // **本文は読まない** — 本文がどんな値でも取り直しは同じである。
    const handler = listen.mock.calls[0]?.[1] as (event: unknown) => void;
    handler({ payload: { 何か: "想定外の本文" } });
    await settle();
    handler(undefined);
    await settle();

    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke.mock.calls[0]?.[0]).toBe("document_state");
    // 入口が受け取るのは**問い合わせ直した封筒**である（イベントの本文ではない）。
    expect(answers).toEqual([opened, opened]);

    // 後始末は購読を解除する（画面が片付いた後に状態を取り直す経路を残さない）。
    stop();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("解除が登録の完了より先に来ても、登録された解除を取りこぼさない", async () => {
    listen.mockReset();
    invoke.mockReset();
    // **`Promise.withResolvers` は使えない**（tsconfig.json の `lib` は ES2022 であり、
    // `withResolvers` は ES2024 である）。解決を外へ取り出すために executor の形を取る。
    let resolveListen: ((stop: () => void) => void) | undefined;
    listen.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveListen = resolve;
        }),
    );

    const stop = installDocumentSessionChanged(() => undefined);
    // **登録の完了より先に解除が来る**（画面がすぐ片付いた場合である）。
    stop();
    const unlisten = vi.fn();
    resolveListen?.(unlisten);
    await settle();

    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("解除の後に解決した取り直しの結果は捨てる（アンマウント済みの画面を更新しない）", async () => {
    listen.mockReset();
    invoke.mockReset();
    listen.mockImplementation(async () => () => undefined);
    let resolveInvoke: ((value: unknown) => void) | undefined;
    invoke.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveInvoke = resolve;
        }),
    );

    const answers: IpcClientResult<DocumentStateResponse>[] = [];
    const stop = installDocumentSessionChanged((result) => {
      answers.push(result);
    });
    await settle();

    const handler = listen.mock.calls[0]?.[1] as () => void;
    handler();
    await settle();
    // **取り直しが解決する前に画面が片付く。**
    stop();
    resolveInvoke?.(openedState());
    await settle();

    expect(answers).toEqual([]);
  });

  it("購読の設置に失敗しても例外を外へ出さない（IPC 無しでも画面は開ける）", async () => {
    listen.mockReset();
    invoke.mockReset();
    listen.mockRejectedValue(new Error("IPC が無い"));
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);

    expect(() => installDocumentSessionChanged(() => undefined)).not.toThrow();
    await settle();

    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });
});

// ===========================================================================
// 購読は 1 つである（源の走査。本 file だけが使う道具）
// ===========================================================================

/**
 * グリッドの源の生のテキスト。`import.meta.glob` は Vite が変換時に解決するので、検査の環境を
 * node の API（`node:fs`）へ結び付けない（`GridScreen.test.ts` の配色の走査と同じ方針）。
 */
const GRID_SOURCES = import.meta.glob("/src/features/grid/**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** **生産の源**（検査自身は除く — 本 file は裁定の経緯として削除した module を名指しする）。 */
const PRODUCTION_SOURCES = Object.entries(GRID_SOURCES).filter(
  ([path]) => !/\.test\.tsx?$/.test(path),
);

/** 画面の源（設置の 1 行を読む）。 */
const SCREEN_SOURCE = GRID_SOURCES["/src/features/grid/GridScreen.tsx"] ?? "";

describe("購読は 1 つである（10.7 の裁定。要件 1.7）", () => {
  it("画面は `installDocumentSessionChanged` を設置する", () => {
    expect(SCREEN_SOURCE).not.toBe("");

    // **呼び出しの形**（`(` を伴う）と、その出所（`src/ipc/documentSession.ts`）を読む。
    // 呼び出しを消して別の購読へ戻せばこの 2 行が落ちる（`import` だけを残しても落ちる）。
    expect(SCREEN_SOURCE).toContain("installDocumentSessionChanged(");
    expect(SCREEN_SOURCE).toContain('"../../ipc/documentSession"');
  });

  it("グリッドの側に購読の写しを 1 つも残していない", () => {
    // **走査そのものが生きている**（見本で確かめる）。この 2 行が無いと、綴りを間違えた走査が
    // 何も見ないまま緑になる。
    expect(Object.keys(GRID_SOURCES).length).toBeGreaterThan(30);
    expect(PRODUCTION_SOURCES.some(([path]) => path.endsWith("/GridScreen.tsx"))).toBe(true);

    // **裁定**: 同じ「購読 → 1 回の `document_state` の取り直し」を写す module はグリッドの側に
    // 置かない（`documentRequests.ts` とその検査を削り、`installDocumentSessionChanged` へ
    // 寄せた）。写しを戻せばこの 2 行が落ちる。
    expect(Object.keys(GRID_SOURCES).filter((path) => path.includes("documentRequests"))).toEqual(
      [],
    );
    expect(
      PRODUCTION_SOURCES.filter(([, source]) => source.includes("documentRequests")).map(
        ([path]) => path,
      ),
    ).toEqual([]);
  });
});

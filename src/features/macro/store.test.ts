/**
 * 実行の面の保持（tasks.md 4.4。要件 1.3、2.1、2.2、2.5）。
 *
 * # 何を固定するか
 *
 * 1. **4 つの流れが結線されている** — 一覧（`macro_list`）→ 選択（能力の提示）→ 実行
 *    （`macro_run`）→ 結果（3 値の提示）
 * 2. **実行の入口から結果が返るまで、面は実行中を示し、ほかの呼び出しを止めない**（要件 2.2。
 *    実行は境界の非同期呼び出しであり、面は `Promise` を待って状態機械を止めない）
 * 3. **変更が入ったときだけ「適用の通知」を出す**（要件 2.5）— 失敗・打ち切り・変更 0 件では
 *    文書が動いておらず、表示を作り直す理由が無い（要件 6.3、7.3）
 * 4. **経路の失敗（封筒の失敗腕）は実行の失敗と別の値になる**（実行そのものが始まらなかった）
 * 5. **遅れて届いた一覧の答えは捨てる**（文書を続けて開いても、古い文書の一覧で新しい面を
 *    上書きしない）
 * 6. **文書がまだ付いていないときは一覧を求めない**（起動直後の一過性の失敗を出さない。
 *    一覧は読み込み中のまま待ち、文書が読めなかったときは理由がそのまま失敗になる）
 *
 * 境界は偽の実装である（`node` 環境には IPC が無い）。**本物のコマンド名は
 * `macroClient.ts` が持つ**ので、ここでは「どの名前で何回呼ばれたか」だけを読む。
 */
import { describe, expect, it } from "vitest";

import type { IpcClientResult } from "../../ipc/client";
import type {
  DocumentStateResponse,
  MacroListResponse,
  MacroRunOutcome,
  MacroRunResponse,
  MacroSummary,
} from "../../ipc/bindings";
import type { MacroClient } from "./macroClient";
import { createMacroSurfaceStore } from "./store";

/** 解釈できた 1 件。 */
function summary(name: string, capabilities: MacroSummary["capabilities"] = []): MacroSummary {
  return { name, kind: "typescript", capabilities, failure: null };
}

/** 一覧の成功の応答。 */
function listed(macros: readonly MacroSummary[]): IpcClientResult<MacroListResponse> {
  return { status: "ok", data: { context: { window: "doc-1" }, macros: [...macros] } };
}

/** 実行の成功の応答（結果は 3 値のいずれか）。 */
function ran(outcome: MacroRunOutcome): IpcClientResult<MacroRunResponse> {
  return { status: "ok", data: { context: { window: "doc-1" }, outcome } };
}

/** 変更の件数（既定はすべて 0 件）。 */
function changes(setCells: number): MacroRunOutcome {
  return {
    outcome: "Ran",
    value: "42",
    output: [],
    changes: {
      set_cells: setCells,
      inserted_rows: 0,
      removed_rows: 0,
      duplicated_rows: 0,
    },
    elapsed_ms: 5,
  };
}

/**
 * 文書が開いている（`Open`）答の応答。**既定である** — 文書が付いている普通の場合の経路を
 * そのまま通す。
 */
function opened(): IpcClientResult<DocumentStateResponse> {
  return {
    status: "ok",
    data: {
      context: { window: "doc-1" },
      status: {
        state: "Open",
        name: "棚卸し.jxcel",
        origin: "file",
        unsaved: false,
        revision: 1,
        sheets: [],
      },
    },
  };
}

/** 文書がまだ関連付いていない（`Absent`）答の応答（**起動直後**）。 */
function absent(): IpcClientResult<DocumentStateResponse> {
  return { status: "ok", data: { context: { window: "doc-1" }, status: { state: "Absent" } } };
}

/** 文書は在るが読めなかった（`Unavailable`）答の応答。 */
function unavailable(reason: string): IpcClientResult<DocumentStateResponse> {
  return {
    status: "ok",
    data: { context: { window: "doc-1" }, status: { state: "Unavailable", reason } },
  };
}

/**
 * 偽の境界。**実行と文書の状態の答えを外から解決できる**（実行中の状態と、起動直後の
 * 「文書がまだ無い」状態を作るために要る）。
 *
 * 保留の約束は `new Promise` で組む — **`Promise.withResolvers` は使えない**（`tsconfig.json`
 * の `lib` が ES2024 を含まない。`src/shell/verificationMacroRun.ts` と同じ理由である）。
 */
function fakeClient(): {
  readonly client: MacroClient;
  readonly calls: string[];
  /** 次に返す一覧の答え（積むと、読む順に 1 つずつ返る。空なら空の一覧）。 */
  readonly listAnswers: IpcClientResult<MacroListResponse>[];
  /** 次に返す文書の状態の答え（積むと、読む順に 1 つずつ返る。空なら開いている）。 */
  readonly sessionAnswers: IpcClientResult<DocumentStateResponse>[];
  /** 保留中の実行の答えを解決する（**実行中の状態を外から作る**）。 */
  readonly settleRun: (answer: IpcClientResult<MacroRunResponse>) => void;
} {
  const calls: string[] = [];
  const listAnswers: IpcClientResult<MacroListResponse>[] = [];
  const sessionAnswers: IpcClientResult<DocumentStateResponse>[] = [];
  const pending: ((answer: IpcClientResult<MacroRunResponse>) => void)[] = [];
  const client: MacroClient = {
    readDocumentState: () => {
      calls.push("document_state");
      return Promise.resolve(sessionAnswers.shift() ?? opened());
    },
    list: () => {
      calls.push("macro_list");
      return Promise.resolve(listAnswers.shift() ?? listed([]));
    },
    run: (name) => {
      calls.push(`macro_run:${name}`);
      return new Promise((resolve) => {
        pending.push(resolve);
      });
    },
  };
  return {
    client,
    calls,
    listAnswers,
    sessionAnswers,
    settleRun: (answer) => {
      const resolve = pending.shift();
      if (resolve === undefined) {
        throw new Error("保留中の実行が無い");
      }
      resolve(answer);
    },
  };
}

/** 微小タスクを数巡ぶん流す（境界の約束の解決も非同期である）。 */
async function settle(): Promise<void> {
  for (let round = 0; round < 4; round += 1) {
    await Promise.resolve();
  }
}

describe("一覧の流れ（要件 1.3、1.4）", () => {
  it("取り直すと文書の状態を読んでから macro_list を 1 回呼び、一覧を入れる", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);
    expect(store.getState().list.status).toBe("loading");

    fake.listAnswers.push(listed([summary("棚卸し", ["file.read"])]));
    store.refresh();
    await settle();

    // **一覧の前に文書の状態を読む**（起動直後の一過性の失敗を出さないための順序である）。
    expect(fake.calls).toEqual(["document_state", "macro_list"]);
    const list = store.getState().list;
    expect(list.status).toBe("ready");
    if (list.status !== "ready") {
      throw new Error("一覧が読めた状態にならなかった");
    }
    expect(list.macros.map((macro) => macro.name)).toEqual(["棚卸し"]);
  });

  it("読めなかった一覧は失敗として現れる（**実行の導線は出ない**）", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);

    fake.listAnswers.push({
      status: "error",
      error: { kind: "Document", detail: { message: "このウィンドウにはドキュメントがありません" } },
    });
    store.refresh();
    await settle();

    const list = store.getState().list;
    expect(list.status).toBe("failed");
    if (list.status !== "failed") {
      throw new Error("失敗の状態にならなかった");
    }
    // **理由の文言は境界が組み立てたものをそのまま運ぶ**（面は 2 つ目の文言を作らない）が、
    // どの層の失敗か（`describeIpcError` の前置き）も落とさない。
    expect(list.message).toContain("このウィンドウにはドキュメントがありません");
    expect(list.message).toContain("ドキュメント");
  });

  it("文書がまだ付いていないときは一覧を求めず、読み込み中のまま待つ（起動直後の一過性の失敗を出さない）", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);

    // 起動直後: 文書はまだ関連付いていない（`document_state` は `Absent`）。
    fake.sessionAnswers.push(absent());
    store.refresh();
    await settle();

    // **一覧を求めない** — 求めた呼び出しは経路の失敗になり、記録に 1 行残る。
    expect(fake.calls).toEqual(["document_state"]);
    expect(store.getState().list.status).toBe("loading");

    // 文書が付いた時点の通知（`./requests` の購読）が取り直す。そのときは一覧が出る。
    fake.listAnswers.push(listed([summary("棚卸し")]));
    store.refresh();
    await settle();

    const list = store.getState().list;
    expect(list.status).toBe("ready");
    if (list.status !== "ready") {
      throw new Error("一覧が読めた状態にならなかった");
    }
    expect(list.macros.map((macro) => macro.name)).toEqual(["棚卸し"]);
  });

  it("文書が読めなかったときは、その理由がそのまま失敗になる（一覧は求めない）", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);

    const reason = "位置 /tmp/消えた.jxcel のドキュメントを読めない";
    fake.sessionAnswers.push(unavailable(reason));
    store.refresh();
    await settle();

    // 求めても同じ理由で失敗するので、一覧は求めない。
    expect(fake.calls).toEqual(["document_state"]);
    const list = store.getState().list;
    expect(list.status).toBe("failed");
    if (list.status !== "failed") {
      throw new Error("失敗の状態にならなかった");
    }
    // **文言は組み立てない**（理由は境界が組んだものをそのまま出す）。
    expect(list.message).toBe(reason);
  });

  it("遅れて届いた一覧の答えは捨てる（古い文書の一覧で上書きしない）", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);

    // 1 回目と 2 回目を続けて要求し、**2 回目を先に解決する**（1 回目は後から届く）。
    const answers: (() => void)[] = [];
    const slow: MacroClient = {
      ...fake.client,
      list: () =>
        new Promise((resolve) => {
          answers.push(() => {
            resolve(listed([summary("古い文書のマクロ")]));
          });
        }),
    };
    const store2 = createMacroSurfaceStore(slow);
    store2.refresh();
    await settle();
    store2.refresh();
    await settle();

    // 2 回目（後の要求）の答えが先に届いた、という状況を作る。
    expect(answers).toHaveLength(2);
    answers[1]?.();
    await settle();
    // ここで 1 回目（古いほう）が遅れて届いても、**採用しない**。
    answers[0]?.();
    await settle();

    const list = store2.getState().list;
    if (list.status !== "ready") {
      throw new Error("一覧が読めた状態にならなかった");
    }
    expect(list.macros.map((macro) => macro.name)).toEqual(["古い文書のマクロ"]);

    // 未使用の保持（上の 1 本目）も動いていないことを確かめる（取り違えの検査ではない）。
    expect(store.getState().list.status).toBe("loading");
  });
});

describe("要求から実行までの 4 つの流れ（要件 2.1、8.2、2.5）", () => {
  it("要求 → 選択 → 実行 → 結果の順に進み、実行は名前を 1 つだけ運ぶ", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);

    // 1. 一覧（開いた時点で出る）。
    fake.listAnswers.push(listed([summary("棚卸し", ["file.read", "net"])]));
    store.refresh();
    await settle();

    // 2. メニューからの要求（一覧から選ばせる段へ入り、一覧を取り直す）。
    fake.listAnswers.push(listed([summary("棚卸し", ["file.read", "net"])]));
    store.request();
    expect(store.getState().picking).toBe(true);
    await settle();
    expect(fake.calls).toEqual([
      "document_state",
      "macro_list",
      "document_state",
      "macro_list",
    ]);

    // 3. 選択（能力の提示）。**まだ実行していない。**
    store.choose("棚卸し");
    expect(store.getState().chosen).toBe("棚卸し");
    expect(store.getState().picking).toBe(false);
    expect(fake.calls).not.toContain("macro_run:棚卸し");

    // 4. 実行（**実行中を示す**）。要求は名前だけである（上限もソースも運ばない）。
    const applied: string[] = [];
    store.subscribeApplied(() => {
      applied.push("適用された");
    });
    store.run();
    expect(store.getState().running).toEqual({ name: "棚卸し" });
    expect(store.getState().chosen).toBeNull();
    expect(fake.calls).toEqual([
      "document_state",
      "macro_list",
      "document_state",
      "macro_list",
      "macro_run:棚卸し",
    ]);

    // 5. 結果（変更が入った → 表示を作り直す通知が 1 回）。
    fake.settleRun(ran(changes(2)));
    await settle();

    expect(store.getState().running).toBeNull();
    expect(store.getState().result?.name).toBe("棚卸し");
    expect(store.getState().result?.result.kind).toBe("ran");
    expect(applied).toEqual(["適用された"]);
  });

  it("要求のあとに面が開いて取り直しても、「選ばせる段」が消えない（実起動の観測が実測した順序）", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);

    // 実起動では 2 つが**この順序**で起きる: 器が要求を受けて面の状態を動かし（`request`）、
    // そのあと遷移先の面が**マウント時に一覧を取り直す**（`refresh`）。ここで段を落とすと、
    // 一覧は出るのに「選ぶ」が 1 つも出ない画面になる（実起動で観測した取り違えである）。
    fake.listAnswers.push(listed([summary("棚卸し", ["file.read"])]));
    store.request();
    expect(store.getState().picking).toBe(true);

    fake.listAnswers.push(listed([summary("棚卸し", ["file.read"])]));
    store.refresh();
    expect(store.getState().picking).toBe(true);
    await settle();

    expect(store.getState().picking).toBe(true);
    const list = store.getState().list;
    if (list.status !== "ready") {
      throw new Error("一覧が読めた状態にならなかった");
    }
    // **選択を提示できる**（要求が生きている）。
    store.choose("棚卸し");
    expect(store.getState().chosen).toBe("棚卸し");
    expect(store.getState().picking).toBe(false);
  });

  it("返り値を待たない（実行中も状態機械は動く。要件 2.2）", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);
    fake.listAnswers.push(listed([summary("棚卸し")]));
    store.refresh();
    await settle();
    store.choose("棚卸し");
    store.run();
    expect(store.getState().running).toEqual({ name: "棚卸し" });

    // **実行の答えが返る前に、別の一覧の取り直しが完了する**
    // （実行のために面が止まっていないことの証拠である）。
    fake.listAnswers.push(listed([summary("棚卸し"), summary("別のマクロ")]));
    store.refresh();
    await settle();
    const list = store.getState().list;
    if (list.status !== "ready") {
      throw new Error("一覧が読めた状態にならなかった");
    }
    expect(list.macros.map((macro) => macro.name)).toEqual(["棚卸し", "別のマクロ"]);
    // **実行中はそのままである**（取り直しは実行の状態を解除しない）。
    expect(store.getState().running).toEqual({ name: "棚卸し" });

    // 実行中に選ぼうとしても、次の実行は始まらない（導線を出していないのと同じ判断）。
    store.choose("別のマクロ");
    store.run();
    expect(fake.calls.filter((call) => call.startsWith("macro_run:"))).toEqual([
      "macro_run:棚卸し",
    ]);
  });

  it("失敗と打ち切りでは表示を作り直さない（要件 6.3、7.3）", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);
    fake.listAnswers.push(listed([summary("棚卸し")]));
    store.refresh();
    await settle();
    const applied: string[] = [];
    store.subscribeApplied(() => {
      applied.push("適用された");
    });

    store.choose("棚卸し");
    store.run();
    fake.settleRun(
      ran({
        outcome: "Failed",
        failure: { kind: { kind: "execution" }, reason: "落ちた", frames: [] },
      }),
    );
    await settle();
    expect(store.getState().result?.result.kind).toBe("failed");
    // **文書は変わっていない**（作り直す理由が無い）。
    expect(applied).toEqual([]);

    // 打ち切りも同じである（**実行中は解除され、次の実行ができる**）。
    store.choose("棚卸し");
    store.run();
    fake.settleRun(
      ran({
        outcome: "Aborted",
        limit: "memory",
        elapsed_ms: 900,
        failure: { kind: { kind: "execution" }, reason: "メモリの上限", frames: [] },
      }),
    );
    await settle();
    const result = store.getState().result?.result;
    expect(result?.kind).toBe("aborted");
    if (result?.kind !== "aborted") {
      throw new Error("打ち切りの提示にならなかった");
    }
    expect(result.limit).toBe("memory");
    expect(applied).toEqual([]);
    expect(store.getState().running).toBeNull();
  });

  it("変更 0 件の成功では表示を作り直さない（要件 2.5）", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);
    fake.listAnswers.push(listed([summary("棚卸し")]));
    store.refresh();
    await settle();
    const applied: string[] = [];
    store.subscribeApplied(() => {
      applied.push("適用された");
    });

    store.choose("棚卸し");
    store.run();
    fake.settleRun(ran(changes(0)));
    await settle();

    expect(store.getState().result?.result.kind).toBe("ran");
    expect(applied).toEqual([]);
  });

  it("経路の失敗（実行そのものが始まらなかった）は実行の失敗と別の値になる", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);
    fake.listAnswers.push(listed([summary("棚卸し")]));
    store.refresh();
    await settle();
    store.choose("棚卸し");
    store.run();

    fake.settleRun({
      status: "error",
      error: { kind: "Document", detail: { message: "実行基盤が要求を受け取らない" } },
    });
    await settle();

    const result = store.getState().result?.result;
    expect(result?.kind).toBe("rejected");
    if (result?.kind !== "rejected") {
      throw new Error("経路の失敗の提示にならなかった");
    }
    // 理由も、どの層の失敗かも落とさない（`src/ipc/client.ts` の `describeIpcError`）。
    expect(result.message).toContain("実行基盤が要求を受け取らない");
    expect(result.message).toContain("ドキュメント");
    expect(store.getState().running).toBeNull();
  });

  it("結果は閉じられる（文書も値も動かない）", async () => {
    const fake = fakeClient();
    const store = createMacroSurfaceStore(fake.client);
    fake.listAnswers.push(listed([summary("棚卸し")]));
    store.refresh();
    await settle();
    store.choose("棚卸し");
    store.run();
    fake.settleRun(ran(changes(1)));
    await settle();
    expect(store.getState().result).not.toBeNull();

    store.dismissResult();
    expect(store.getState().result).toBeNull();
    // **一覧はそのままである**（閉じるのは提示だけである）。
    expect(store.getState().list.status).toBe("ready");
    // 閉じたあとに適用の通知が遅れて届くことはない（通知は実行の終わりに 1 回だけである）。
    expect(fake.calls.filter((call) => call.startsWith("macro_run:"))).toEqual([
      "macro_run:棚卸し",
    ]);
  });
});

/**
 * 検証専用: **起動時にマクロを「一覧 → 選択 → 実行」まで順に駆動する**（`macro-runtime`
 * スペックの tasks.md 5.1 / 5.2）。
 *
 * 所有: 検証専用の起動時の駆動（`src/main.tsx` の `__JXCEL_VERIFICATION__` の分岐）と、
 * 実行の面の保持（`src/features/macro/`）。
 *
 * # なぜ押下ではなく仕込みなのか（4.4 の申し送り）
 *
 * 4.4 は「マクロ > 実行…」から「選ぶ → 実行する」までの押下を実起動で観測しようとしたが、
 * **AT-SPI では再現できない** — この機械の WebKitGTK は DOM をアクセシビリティの木へ露出しない
 * ため、画面の押下は外から起こせない（4.4 の申し送り）。したがって 5.1 の引き金は
 * **起動時に「一覧 → 選択 → 実行」までを仕込む形**であり、その仕込みが本モジュールである。
 * **製品の実行経路をそのまま通す** — 使うのは製品の面の保持（`MACRO_SURFACE_STORE`）だけであり、
 * 検証専用の実行経路も検証専用のコマンドも持たない（`session/verification.rs` と同じ規律）。
 *
 * # なぜ 1 件ではなく並びなのか（5.2）
 *
 * 要件 6.4（打ち切りの後も操作できる）は、**打ち切りが起きたのと同じ起動の中で続けて別の
 * マクロが走ること**でしか観測できない（別の起動で見ると、確かめているのは「次の起動が
 * できること」になる）。したがって 5.2 の引き金は `,` 区切りの**並び**を運び、本モジュールは
 * **並びの順に 1 件ずつ**駆動して、**1 件につき観測の行を 1 行**送る。
 *
 * # 経路（Rust とフロントエンドの対の契約）
 *
 * 1. 検証ビルド（`--features verification-triggers` かつ `JXCEL_VERIFICATION_BUILD=1`）を、
 *    **標本の文書を起動の引数に渡し**、環境変数
 *    `JXCEL_VERIFICATION_MACRO_RUN=<マクロ名>[,<マクロ名>…]` と
 *    `JXCEL_VERIFICATION_INITIAL_SCREEN=grid` を付けて起動する（標本は
 *    `cargo run -p macro-runtime --example make-macro-document --features verification-samples -- <出力先>`
 *    が書き出す。マクロの名前は `標本の記入` / `標本の往復` / `標本の拒否` / `標本の失敗` /
 *    `標本の打ち切り`）。
 * 2. Rust（`src-tauri/src/window/mod.rs` の `macro_run_script`）がその値を**ウィンドウの初期化
 *    スクリプト**として書き、`window.__JXCEL_VERIFICATION_MACRO_RUN__` に**文字列の配列**として
 *    載せる（**Webview はプロセスの環境変数を読めない**。`src/shell/verificationScreen.ts` と
 *    同じ理由）。配列だけを受け付け、文字列は受け付けない（`macro_run_script` の doc）。
 * 3. 本モジュールがマウント時にそのグローバルを読み、**空でない文字列の並びであるときだけ**
 *    駆動する。
 * 4. 結果は**イベントで Rust へ送り**、`src-tauri/src/lifecycle.rs` の
 *    `register_macro_observation_listener` が診断の記録へ 1 行（`マクロの観測: {…}`）で写す。
 *
 * **グローバルの綴りとイベントの名前は Rust 側と対であり、片方だけ変えてはならない。**
 *
 * # なぜ記録へ送るのか（製品の記録だけでは足りない）
 *
 * 製品の記録（`macro_run` の 1 行）は**実行**の事実（名前・種別・結果・打ち切り・変更の件数・
 * 所要）を持ち、**失敗の理由とフレームを持たない**（要件 8.3 の「ソースと値は出さない」の
 * 帰結であり、4.3 の設計である）。5.2 は「失敗の理由とフレーム」
 * 「能力の拒否」をも**診断の記録**から判定するので、検証専用の観測の行が要る。本モジュールが
 * 運ぶのは**閉じた事実だけ**である — 一覧の件数と名前・選んだ名前・宣言されている能力・結果の
 * 3 値・変更の件数・打ち切りの種類・失敗の層と理由とフレーム。**戻り値と `console` の出力は
 * 運ばない**（値が記録へ流れる経路を作らない）。
 *
 * # 待つもの（起動の順序）
 *
 * 起動の引数の文書は非同期に読み込まれ、グリッドはそのあとにシートを開く。したがって
 *
 * 1. **グリッドが表を描くまで待つ**（製品の画面が外から読める印 `data-testid="jxcel-grid-table"`
 *    を出す瞬間。`src/features/grid/gridObservation.tsx` が 9.2 で使ったのと同じ読み方である）。
 *    **マクロの変更を適用するにはグリッドがシートを開いていることが要る**ためであり、これを
 *    待たずに実行すると適用だけが拒まれる（`macro_run` は**記録を書いた後**に適用するので、
 *    記録には実行の行が残るのに文書は変わらない — 黙って誤った観測になる）。
 * 2. 一覧を読む（面の保持を通すので、パネルに出るのと同じ一覧である）。
 * 3. 選び、実行し、**結果が届くまで待つ**（打ち切りは既定 30 秒かかりうる）。
 *
 * **例外を外へ出さない。** 駆動の失敗（一覧が読めない・時間切れ）も 1 行として記録へ残し、
 * 検査器がそれを見て落ちる（`src/shell/verificationBulk.ts` と同じ判断）。
 *
 * # 既定のビルドと既定の起動は変わらない
 *
 * グローバルは `verification-triggers` feature の下でしか設定されない（`src-tauri/Cargo.toml`）。
 * したがって既定のビルドでは本モジュールは常に何もせずに戻り、**配布物のバンドルには本モジュール
 * が 1 バイトも入らない**（`src/main.tsx` の動的 import が `__JXCEL_VERIFICATION__` で括られて
 * おり、`scripts/check-shipping-bundle.sh` が機械検査する）。
 */
import { emit } from "@tauri-apps/api/event";

import { MACRO_SURFACE_STORE } from "../features/macro/MacroPanel";
import type { MacroSurfaceStore } from "../features/macro/store";
import {
  failurePresentation,
  type MacroFailurePresentation,
  type MacroResultPresentation,
} from "../features/macro/surface";
import type {
  MacroAbortKind,
  MacroChangeCounts,
  MacroFrame,
  MacroSummary,
} from "../ipc/bindings";

/**
 * 実行するマクロの名前の**並び**を載せるグローバルの名前。
 *
 * **`src-tauri/src/window/mod.rs` の `VERIFY_MACRO_RUN_GLOBAL` と同じ綴りでなければならない**
 * （既定のビルドには Rust 側の定義が存在しないため、共有できる定数を持てない検証専用の対の
 * 契約である。`src/shell/verificationBulk.ts` と同じ形）。載る値は**文字列の配列**である。
 */
const VERIFICATION_MACRO_RUN_GLOBAL = "__JXCEL_VERIFICATION_MACRO_RUN__" as const;

/**
 * 観測の結果を Rust へ通知するイベントの名前。
 *
 * **`src-tauri/src/lifecycle.rs` の `VERIFY_MACRO_OBSERVATION_EVENT` と同じ綴りでなければ
 * ならない**（上と同じ理由の対の契約）。`emit` は `core:event:default` の許可（`core:default` に
 * 含まれる）の範囲であり、**新しい権限もコマンドも要さない**。
 */
const MACRO_OBSERVATION_EVENT = "jxcel-verification-macro-observation";

/**
 * 製品の画面が「グリッドの表を描いている」ことを外から読む印。
 *
 * `src/features/grid/GridScreen.tsx` が表を描くときだけ出す `data-testid` であり、
 * `src/features/grid/gridObservation.tsx`（9.2）も同じ印を読んでいる。**列や行の値ではなく、
 * 「シートが開いて表が描かれた」ことだけを読む。**
 */
const GRID_TABLE_SELECTOR = '[data-testid="jxcel-grid-table"]';

/**
 * 一覧が読めるまでの上限（起動の引数の文書の読み込みと、グリッドのシートを開く往復を含む）。
 *
 * 4.4 の実測では `macro_list` は起動の数秒後に成立している。余裕を持って 30 秒とする
 * （**時間切れは失敗として記録に残る**ので、上限は「遅い環境を緑にしない」側へ倒す）。
 */
const START_DEADLINE_MS = 30_000;

/**
 * 実行が終わるまでの上限。
 *
 * **時間の上限の既定は 30 秒**（要件 6.4。`macro-runtime` の `Limits::DEFAULT_TIME_MS`）であり、
 * 打ち切りの観測（5.2）はその満了を待つ。したがって 30 秒より十分に長く取る。
 */
const RUN_DEADLINE_MS = 180_000;

/** 状態を見に行く間隔（グリッドの出現・一覧・結果の待ち）。 */
const POLL_INTERVAL_MS = 100;

/**
 * 一覧の取り直しの間隔。**文書がまだ関連付いていない間の失敗を捨てて取り直す**ために置く
 * （毎回の取り直しは記録に `macro_list` の行を 1 行残すので、間隔は空けておく）。
 */
const LIST_RETRY_MS = 1_000;

declare global {
  interface Window {
    /**
     * 検証専用: 検証ビルドの初期化スクリプトが載せるマクロの名前の並び。**既定のビルドでは
     * 決して設定されない**（`undefined`）。値は**文字列の配列**であることを実行時に検査する。
     */
    readonly __JXCEL_VERIFICATION_MACRO_RUN__?: unknown;
  }
}

/**
 * 観測の 1 行（Rust が診断の記録へ写す。**キーは 5.2 の検査器が読む契約である**）。
 *
 * `changes` / `limit` / `layer` / `reason` / `frames` は、その結果の種別に無いとき `null`
 * （または空の並び）である — **無いことを書かないのではなく、無いと書く**（検査器が
 * 「欠けている」と「空である」を区別できるようにする）。
 */
export interface MacroObservation {
  /** 仕込みが要求した名前（`JXCEL_VERIFICATION_MACRO_RUN` の値）。 */
  readonly requested: string;
  /** 一覧の件数（読めなかったときは `null`）。 */
  readonly listed: number | null;
  /** 一覧の名前（**順序は保存順**。仕込まれたマクロが一覧に現れたことの材料）。 */
  readonly names: readonly string[];
  /** 選んで実行した名前（選べなかったときは `null`）。 */
  readonly chosen: string | null;
  /** 選んだ 1 件が宣言している能力（要件 8.2 の提示。`file.read` / `file.write` / `net`）。 */
  readonly capabilities: readonly string[];
  /**
   * 結果の種別。製品の 3 値（`ran` / `failed` / `aborted`）に、経路の失敗（`rejected`）と
   * 仕込みが進まなかった理由（`no-grid` / `not-listed` / `not-runnable` / `timed-out`）を足す。
   */
  readonly outcome: string;
  /** 変更の件数（`ran` のときだけ。種別ごと）。 */
  readonly changes: MacroChangeCounts | null;
  /** 実行の所要（ミリ秒。`ran` / `aborted` のとき）。 */
  readonly elapsedMs: number | null;
  /** 打ち切りの種類（`aborted` のときだけ。要件 6.1、6.2）。 */
  readonly limit: MacroAbortKind | null;
  /** 失敗の層（`source` / `transpile` / `execution` / `host_rejected:<API 名>`）。 */
  readonly layer: string | null;
  /** 失敗の理由（例外のメッセージ・拒否の理由・構文の診断）。 */
  readonly reason: string | null;
  /** 失敗に至る呼び出しの並び（内側から外側へ）。 */
  readonly frames: readonly MacroFrame[];
}

/**
 * 起動時のマクロの実行を仕掛ける。**`src/main.tsx` から 1 回だけ呼ぶ。**
 *
 * グローバルが無い（既定のビルド・通常の起動）ときは何もしない。あるときは駆動を始める
 * （初回描画のフレームを妨げないよう、**2 番目の `requestAnimationFrame`** から始める —
 * `src/shell/verificationBulk.ts` と同じ理由である）。
 */
export function installVerificationMacroRun(): void {
  // グローバルは**空でない文字列の並びであるときだけ**受け付ける（配列でない値・空の要素を
  // 黙って受け付けない — 受け付ければ「一覧に無い名前」の観測が 1 行増えるだけで、仕込みの
  // 誤りが見えなくなる。Rust 側（`macro_run_script`）は同じ規則で弾いている）。
  const requested: unknown = window[VERIFICATION_MACRO_RUN_GLOBAL];
  if (!Array.isArray(requested) || requested.length === 0) {
    return;
  }
  const names: string[] = [];
  for (const entry of requested) {
    if (typeof entry !== "string" || entry.trim() === "") {
      return;
    }
    names.push(entry);
  }
  if (typeof requestAnimationFrame !== "function") {
    // 描画フレームを持たない環境では駆動しない — 検証は実アプリで行う。
    return;
  }
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void drive(names, MACRO_SURFACE_STORE);
    });
  });
}

/**
 * 待つ（タイマーだけであり、投げない）。
 *
 * **`Promise.withResolvers` は使えない** — `tsconfig.json` の `lib` が ES2024 を含まないため
 * である（`src/features/grid/gridObservation.tsx` が同じ理由を記録している）。
 */
function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

/**
 * 仕込みの本体。**並びの 1 件につき観測の行を 1 行必ず送る**（途中で失敗してもそこまでの事実を
 * 送る）。**順に実行する** — 前の 1 件が終わってから次を選ぶ（実行は 1 つずつであり、
 * 面の保持も実行中の 2 つ目を断る。要件 2.2 の裏返し）。
 */
async function drive(
  requested: readonly string[],
  store: MacroSurfaceStore,
): Promise<void> {
  // 1. **グリッドが表を描くまで待つ**（doc「待つもの」）。ここを待たないと、実行の記録は
  //    残るのに変更の適用だけが拒まれる。
  const gridReady = await waitUntil(
    () => document.querySelector(GRID_TABLE_SELECTOR) !== null,
    START_DEADLINE_MS,
  );
  if (!gridReady) {
    for (const name of requested) {
      await report(observationFailure(name, "no-grid", null, []));
    }
    return;
  }

  // 2. 一覧を読む（面の保持を通す。パネルに出るのと同じ一覧である）。**1 回だけ読む** —
  //    仕込みの間に文書が差し替わることはなく、毎回読むと `macro_list` の行が増えるだけである。
  const macros = await waitForList(store);
  if (macros === null) {
    for (const name of requested) {
      await report(observationFailure(name, "not-listed", null, []));
    }
    return;
  }

  for (const name of requested) {
    const summary = macros.find((macro) => macro.name === name) ?? null;
    if (summary === null) {
      await report(observationFailure(name, "not-listed", null, macros));
      continue;
    }
    if (summary.failure !== null) {
      // **解釈できなかった 1 件は実行しない**（要件 1.4。理由を記録へ残す）。面の提示と同じ写像を
      // 通す（境界の `kind` と提示の `layer` の 2 つの形を本モジュールに持ち込まない）。
      await report({
        ...observationFailure(name, "not-runnable", summary, macros),
        ...flattenFailure(failurePresentation(summary.failure)),
      });
      continue;
    }

    // 3. 選ぶ（要件 8.2 の能力の提示へ入る）→ 実行する（要件 2.1）。
    store.choose(name);
    store.run();
    const settled = await waitForRun(store, name);
    if (settled === null) {
      await report(observationFailure(name, "timed-out", summary, macros));
      continue;
    }
    await report(observationOf(name, macros, summary, settled));
  }
}

/** 条件が成り立つまで待つ（上限を過ぎたら `false`）。 */
async function waitUntil(condition: () => boolean, deadlineMs: number): Promise<boolean> {
  const deadline = Date.now() + deadlineMs;
  while (Date.now() < deadline) {
    if (condition()) {
      return true;
    }
    await sleep(POLL_INTERVAL_MS);
  }
  return condition();
}

/**
 * 一覧が読めるまで待つ。**読めなかったときは `null`**（上限を過ぎた場合）。
 *
 * 文書がまだ関連付いていない間の `macro_list` は失敗するので、失敗の状態のときは
 * [`LIST_RETRY_MS`] を空けて取り直す（**取り直しの回数を記録に残さない**ための間隔である）。
 */
async function waitForList(store: MacroSurfaceStore): Promise<MacroSummary[] | null> {
  const deadline = Date.now() + START_DEADLINE_MS;
  let nextAttempt = 0;
  while (Date.now() < deadline) {
    const state = store.getState();
    if (state.list.status === "ready") {
      return [...state.list.macros];
    }
    if (Date.now() >= nextAttempt) {
      nextAttempt = Date.now() + LIST_RETRY_MS;
      store.refresh();
    }
    await sleep(POLL_INTERVAL_MS);
  }
  const state = store.getState();
  return state.list.status === "ready" ? [...state.list.macros] : null;
}

/**
 * 実行の結果が届くまで待つ。**届かなければ `null`**（上限を過ぎた場合）。
 *
 * 結果は**名前で確かめる** — 面の結果は 1 つであり、別の実行の結果と取り違えない
 * （`src/features/macro/surface.ts` の `SettledMacroRun` の理由と同じ）。
 */
async function waitForRun(
  store: MacroSurfaceStore,
  name: string,
): Promise<MacroResultPresentation | null> {
  const deadline = Date.now() + RUN_DEADLINE_MS;
  while (Date.now() < deadline) {
    const state = store.getState();
    if (state.running === null && state.result !== null && state.result.name === name) {
      return state.result.result;
    }
    await sleep(POLL_INTERVAL_MS);
  }
  return null;
}

/** 結果の種別に応じた 1 行を組み立てる（**純粋関数**。検査はここを直接呼ぶ）。 */
export function observationOf(
  requested: string,
  macros: readonly MacroSummary[],
  summary: MacroSummary,
  result: MacroResultPresentation,
): MacroObservation {
  const base: Omit<MacroObservation, "outcome"> = {
    requested,
    listed: macros.length,
    names: macros.map((macro) => macro.name),
    chosen: requested,
    capabilities: [...summary.capabilities],
    changes: null,
    elapsedMs: null,
    limit: null,
    layer: null,
    reason: null,
    frames: [],
  };
  switch (result.kind) {
    case "ran":
      return {
        ...base,
        outcome: "ran",
        changes: result.changes,
        elapsedMs: result.elapsedMs,
      };
    case "failed":
      return { ...base, outcome: "failed", ...flattenFailure(result.failure) };
    case "aborted":
      return {
        ...base,
        outcome: "aborted",
        elapsedMs: result.elapsedMs,
        limit: result.limit,
        ...flattenFailure(result.failure),
      };
    case "rejected":
      // **実行そのものが始まらなかった**（封筒の失敗腕。文書は変わっていない）。
      return { ...base, outcome: "rejected", reason: result.message };
  }
}

/** 仕込みが実行まで進まなかったときの 1 行（`chosen` は選べたときだけ入る）。 */
function observationFailure(
  requested: string,
  outcome: string,
  summary: MacroSummary | null,
  macros: readonly MacroSummary[] | null,
): MacroObservation {
  return {
    requested,
    listed: macros === null ? null : macros.length,
    names: macros === null ? [] : macros.map((macro) => macro.name),
    chosen: summary === null ? null : requested,
    capabilities: summary === null ? [] : [...summary.capabilities],
    outcome,
    changes: null,
    elapsedMs: null,
    limit: null,
    layer: null,
    reason: null,
    frames: [],
  };
}

/**
 * 失敗の理由・層・フレームを 1 行の形へ写す（**解釈せずそのまま運ぶ**）。
 *
 * 層は 1 つの文字列へ畳む — ホスト API の拒否だけが**拒んだ API の名前**を持つ（要件 9.2）ので、
 * `host_rejected:host.netFetch` の形にする（検査器が「能力の拒否」を文字列一致で読めるように
 * するため）。
 */
function flattenFailure(
  failure: MacroFailurePresentation,
): Pick<MacroObservation, "layer" | "reason" | "frames"> {
  return {
    layer:
      failure.layer.kind === "host_rejected"
        ? `host_rejected:${failure.layer.api}`
        : failure.layer.kind,
    reason: failure.reason,
    frames: [...failure.frames],
  };
}

/** 観測の 1 行を Rust（診断の記録）へ送る。**投げない**（送れないことは観測の失敗ではない）。 */
async function report(observation: MacroObservation): Promise<void> {
  try {
    await emit(MACRO_OBSERVATION_EVENT, observation);
  } catch (error: unknown) {
    console.warn("マクロの観測を記録へ送れなかった", error);
  }
}

/**
 * マクロの実行の面の状態と提示（tasks.md 4.4。要件 1.3、1.4、2.1、2.3、2.4、2.5、2.7、8.2、
 * 9.1、9.2、9.3）。
 *
 * # 何をここに置くか（**表示の文言を組む唯一の場所**）
 *
 * 1. **面の状態**（[`MacroSurfaceState`]）と、その上の**全域な遷移**（`macroSurface*`）。
 *    遷移はどれも投げない — 面の入口はイベントハンドラと非同期の結果から呼ばれるので、
 *    `src/shell/ScreenBoundary.tsx` はその例外を捕まえない（`src/features/grid/GridScreen.tsx`
 *    と同じ規律）。
 * 2. **境界の値を利用者へ見せる形への写し**（`describe*`）。境界の型は判別可能な合併型であり
 *    （`src/ipc/bindings.ts` の `MacroFailureTag` / `MacroRunOutcome`）、**どの値も握り潰さない**
 *    — 網羅的に分岐し、新しい変種が増えたら `src/ipc/client.ts` の `assertNever` が型検査を
 *    落とす。
 *
 * **境界の値そのものは改変しない。** 文言を運んでくるもの（失敗の理由、出力の本文、戻り値の
 * 提示）はそのまま出す — 面が 2 つ目の言い換えを作ると、記録（`src-tauri` の診断の記録）と
 * 画面が食い違う。
 *
 * # 何をここに置かないか
 *
 * - **実行の起動と結果の保持**（[`./store`]）。本 module は値だけを扱い、`Promise` を持たない
 * - **描画**（[`./MacroPanel`]）
 *
 * # 3 つの提示はそれぞれ固有である（要件 2.4、6.1、6.2、9.1）
 *
 * | 値 | 何が起きたか | 提示 |
 * |---|---|---|
 * | `Ran` | 実行が終わった（成功） | 戻り値・出力・**変更の件数**・所要 |
 * | `Failed` | 実行したが失敗した | 失敗の層（ソース／変換／実行／ホスト API の拒否）・理由・**フレーム** |
 * | `Aborted` | 上限で打ち切った | **打ち切りの種類**（時間／メモリ）・理由・フレーム・所要 |
 * | `rejected` | **実行そのものが始まらなかった**（経路の失敗） | 理由だけ（失敗とも打ち切りとも混ぜない） |
 *
 * 打ち切りを失敗と**別の値**にしたのはこの表のためである（打ち切りの提示は「どこで失敗したか」
 * ではなく「どちらの上限に当たったか」を言う。要件 6.1、6.2）。
 */
import { assertNever } from "../../ipc/client";
import type {
  MacroAbortKind,
  MacroChangeCounts,
  MacroFailureReport,
  MacroFailureTag,
  MacroFrame,
  MacroKindTag,
  MacroOutputLine,
  MacroRunOutcome,
  MacroSummary,
} from "../../ipc/bindings";

// ===========================================================================
// 1. 面の状態
// ===========================================================================

/**
 * 一覧の読み込みの状態。**判別可能な合併型である**（`status` で網羅的に分岐する）。
 *
 * `ready` の空の並びは「マクロが 1 件も無い」であり、`failed` と混ぜない — 前者は正常な結果で
 * あり（要件 2.7 の「実行できるマクロが 1 つも無い」の一つの場合である）、後者は読めなかった
 * ことである。
 */
export type MacroListState =
  | { readonly status: "loading" }
  | { readonly status: "failed"; readonly message: string }
  | { readonly status: "ready"; readonly macros: readonly MacroSummary[] };

/**
 * 実行の提示（[`./surface`] の module doc の表）。**境界の 3 値に、経路の失敗を足した 4 値**
 * である。
 */
export type MacroResultPresentation =
  | {
      readonly kind: "ran";
      /** 戻り値の提示用の表現（**境界が組んだ文字列そのもの**）。 */
      readonly value: string;
      /** `console` の出力（順序を保つ。要件 2.3）。 */
      readonly output: readonly MacroOutputLine[];
      /** 変更の件数（種別ごと。要件 2.5）。 */
      readonly changes: MacroChangeCounts;
      /** 変更が 1 件でもあったか（表示中のシートを開き直す判断に使う）。 */
      readonly changed: boolean;
      /** 実行の所要（ミリ秒）。 */
      readonly elapsedMs: number;
    }
  | { readonly kind: "failed"; readonly failure: MacroFailurePresentation }
  | {
      readonly kind: "aborted";
      /** どちらの上限に当たったか（要件 6.1、6.2）。 */
      readonly limit: MacroAbortKind;
      readonly elapsedMs: number;
      readonly failure: MacroFailurePresentation;
    }
  /** **実行そのものが始まらなかった**（封筒の失敗腕）。実行の失敗（`failed`）と混ぜない。 */
  | { readonly kind: "rejected"; readonly message: string };

/**
 * 失敗の提示（要件 2.4、9.1、9.2、9.3）。**理由とフレームをそのまま運ぶ。**
 */
export interface MacroFailurePresentation {
  /**
   * どの層で失敗したか（`MacroFailureTag` の 4 値に、拒んだ API の名前を足したもの）。
   * ホスト API の拒否だけが名前を持つ（要件 9.2）。
   */
  readonly layer: MacroFailureLayer;
  /** 失敗の理由（例外のメッセージ・拒否の理由・構文の診断。**境界が組んだ文言そのもの**）。 */
  readonly reason: string;
  /** 失敗に至る呼び出しの並び（内側から外側へ。空でありうる）。 */
  readonly frames: readonly MacroFrame[];
}

/** 失敗の層（提示に使う形。`MacroFailureTag` と同じ判別子を持つ）。 */
export type MacroFailureLayer =
  | { readonly kind: "source" }
  | { readonly kind: "transpile" }
  | { readonly kind: "execution" }
  | { readonly kind: "host_rejected"; readonly api: string };

/**
 * 直近の実行の提示（要件 2.3、2.4、2.5、6.1、6.2）。
 *
 * **どのマクロの実行だったかを伴う** — 結果の面は 1 つであり（要件 2.5）、その 1 つが
 * 「実行中」の表示から置き換わるので、名前を伴わないと別の 1 件の結果と読める。
 */
export interface SettledMacroRun {
  readonly name: string;
  readonly result: MacroResultPresentation;
}

/**
 * 面の状態。**実行の面が持つものの全体**である。
 *
 * `chosen` が**名前**を持つのは、能力と種別を一覧から引くためである（写しを作らない —
 * 一覧を取り直したときに古い宣言が残らない）。
 */
export interface MacroSurfaceState {
  /** 一覧（要件 1.3、1.4）。 */
  readonly list: MacroListState;
  /**
   * メニューからの要求で**一覧から選ばせている段**に居るか（要件 2.1）。
   *
   * 真のときだけ、実行できる 1 件に「選ぶ」の操作が出る（**実行の入口はメニューの 1 項目**で
   * あり、面はその要求を受けて選択を提示する）。
   */
  readonly picking: boolean;
  /** 選ばれて**能力を提示している**マクロの名前（実行の前。要件 8.2）。 */
  readonly chosen: string | null;
  /** 実行中（要求を送ってから結果が返るまで）。 */
  readonly running: { readonly name: string } | null;
  /** 直近の実行の提示（要件 2.3、2.4、2.5、6.1、6.2）。 */
  readonly result: SettledMacroRun | null;
}

/** 面の初期状態（一覧を読む前）。 */
export function initialMacroSurfaceState(): MacroSurfaceState {
  return { list: { status: "loading" }, picking: false, chosen: null, running: null, result: null };
}

// ===========================================================================
// 2. 遷移（**すべて全域であり、投げない**）
// ===========================================================================

/**
 * 一覧を取り直し始める（画面のマウント・文書の変化・メニューからの要求）。
 *
 * **選ばれていた 1 件は落とす** — 文書が差し替われば同じ名前が別のソースであるかもしれず、
 * 古い宣言を掲げたまま実行させるのは要件 8.2 の提示として誤りである。**直近の結果は残す**
 * （終わった実行の事実は、そのあと文書が差し替わっても消えない。消すのは利用者の操作である）。
 *
 * **「選ばせる段」（`picking`）は動かさない。** これは一覧の中身ではなく**面の段**であり、
 * メニューからの要求が定めたものである。実起動の観測が実測した取り違え: 要求は遷移の**前**に
 * 面の状態を動かし（`src/shell/Layout.tsx` の購読）、遷移先の面は**マウント時に取り直す**ので、
 * ここで `picking` を落とすと**要求が消える** — 一覧は出るのに「選ぶ」が 1 つも出ない画面に
 * なる（`./store.test.ts` の「要求のあとの取り直し」がこの順序を固定する）。
 */
export function macroSurfaceReloadStarted(state: MacroSurfaceState): MacroSurfaceState {
  return {
    list: { status: "loading" },
    picking: state.picking,
    chosen: null,
    running: state.running,
    result: state.result,
  };
}

/** 一覧を入れる（要件 1.3、1.4）。 */
export function macroSurfaceLoaded(
  state: MacroSurfaceState,
  macros: readonly MacroSummary[],
): MacroSurfaceState {
  return { ...state, list: { status: "ready", macros } };
}

/** 一覧を読めなかった（**実行の導線は出さない**）。 */
export function macroSurfaceLoadFailed(
  state: MacroSurfaceState,
  message: string,
): MacroSurfaceState {
  return { ...state, list: { status: "failed", message } };
}

/**
 * メニューからの要求を受けた（要件 2.1）。
 *
 * **一覧から選ばせる段へ入る。** 前の結果は落とす（1 つの面に 2 つの実行の提示を重ねない）。
 */
export function macroSurfacePickRequested(state: MacroSurfaceState): MacroSurfaceState {
  return { ...state, picking: true, chosen: null, result: null };
}

/**
 * 一覧から 1 件を選んだ（要件 8.2 の**能力の提示**へ移る）。
 *
 * **実行できない 1 件は選べない。** 解釈できなかったマクロ（`failure` を持つもの）を選ぶ操作は
 * 面が作らない（要件 1.4 の理由を読ませるだけであり、走らせる対象にしない）。知らない名前も
 * 同じ（一覧が取り直された直後の押下である）。
 */
export function macroSurfaceChosen(state: MacroSurfaceState, name: string): MacroSurfaceState {
  const summary = summaryOf(state, name);
  if (summary === null || summary.failure !== null) {
    return state;
  }
  return { ...state, picking: false, chosen: name };
}

/** 選択を取り消す（**何も送らない** — 実行の要求はまだ出ていない）。 */
export function macroSurfaceChoiceCancelled(state: MacroSurfaceState): MacroSurfaceState {
  return { ...state, picking: false, chosen: null };
}

/**
 * 実行を始めた（要件 2.1、2.2）。
 *
 * **選ばれていた 1 件は落とす**（能力の提示は実行の前の段であり、実行中は「実行中」を示す）。
 * 前の結果も落とす（この実行の結果が同じ場所に入る）。
 */
export function macroSurfaceRunStarted(
  state: MacroSurfaceState,
  name: string,
): MacroSurfaceState {
  return { ...state, picking: false, chosen: null, running: { name }, result: null };
}

/** 実行が終わった（要件 2.3、2.4、2.5、6.1、6.2）。**実行中は解除する。** */
export function macroSurfaceRunSettled(
  state: MacroSurfaceState,
  name: string,
  result: MacroResultPresentation,
): MacroSurfaceState {
  return { ...state, running: null, result: { name, result } };
}

/** 結果を閉じる（**文書も値も動かない** — 提示を消すだけである）。 */
export function macroSurfaceResultDismissed(state: MacroSurfaceState): MacroSurfaceState {
  return { ...state, result: null };
}

// ===========================================================================
// 3. 選択（**状態からの引き方**）
// ===========================================================================

/** 一覧にある 1 件を名前で引く。無ければ `null`（**取り直しの直後に起きうる**）。 */
export function summaryOf(state: MacroSurfaceState, name: string): MacroSummary | null {
  if (state.list.status !== "ready") {
    return null;
  }
  return state.list.macros.find((macro) => macro.name === name) ?? null;
}

/** 実行できる 1 件（**解釈できたものだけ**。要件 1.4、2.7）。 */
export function runnableMacros(macros: readonly MacroSummary[]): readonly MacroSummary[] {
  return macros.filter((macro) => macro.failure === null);
}

/**
 * いま**実行の導線を出せるか**（要件 2.7）。
 *
 * 出すのは 2 つが同時に成り立つときだけである:
 *
 * 1. **実行できる 1 件がある**（解釈できたマクロが 1 件以上ある）
 * 2. **実行中でない**（実行は 1 つずつである。実行中に 2 つ目の導線を出すと、押しても
 *    断られる操作を提示することになる — `src-tauri` は実行中の要求を「実行中である」として
 *    断る）
 */
export function canPresentRun(state: MacroSurfaceState): boolean {
  if (state.list.status !== "ready" || state.running !== null) {
    return false;
  }
  return runnableMacros(state.list.macros).length > 0;
}

/** いま能力を提示している 1 件（要件 8.2）。無ければ `null`。 */
export function chosenSummary(state: MacroSurfaceState): MacroSummary | null {
  return state.chosen === null ? null : summaryOf(state, state.chosen);
}

// ===========================================================================
// 4. 境界の値の提示（**文言を組む唯一の場所**）
// ===========================================================================

/** 種別の見出し（要件 1.3）。 */
export function describeKind(kind: MacroKindTag): string {
  switch (kind) {
    case "typescript":
      return "TypeScript";
    case "javascript":
      return "JavaScript";
    default:
      return assertNever(kind, "種別の分岐が網羅されていない");
  }
}

/** 失敗の層の見出し（要件 9.1、9.2）。 */
export function describeFailureLayer(layer: MacroFailureLayer): string {
  switch (layer.kind) {
    case "source":
      return "ソースを解釈できない";
    case "transpile":
      return "変換できない";
    case "execution":
      return "実行の途中で失敗した";
    case "host_rejected":
      return `ホスト API が拒否した（${layer.api}）`;
    default:
      return assertNever(layer, "失敗の層の分岐が網羅されていない");
  }
}

/**
 * 呼び出しの 1 段の見出し（要件 9.1、9.3）。
 *
 * 行と列は**1 起点のまま**出す（原位置である。要件 9.1 の「マクロのソースの行と列」）。
 * `function` は無名の位置で空文字である（境界の型の doc）ので、そのときは名前を出さない。
 */
export function describeFrame(frame: MacroFrame): string {
  const position = `マクロ「${frame.macro_name}」の ${String(frame.line)} 行 ${String(frame.column)} 列目`;
  return frame.function === "" ? position : `${position}（${frame.function}）`;
}

/** 打ち切りの種類の見出し（要件 6.1、6.2）。**上限の値は言わない**（設定で変わる）。 */
export function describeAbortLimit(limit: MacroAbortKind): string {
  switch (limit) {
    case "time":
      return "時間の上限";
    case "memory":
      return "メモリの上限";
    default:
      return assertNever(limit, "打ち切りの種類の分岐が網羅されていない");
  }
}

/** 変更の件数の合計（要件 2.5）。 */
export function changeTotal(changes: MacroChangeCounts): number {
  return changes.set_cells + changes.inserted_rows + changes.removed_rows + changes.duplicated_rows;
}

/**
 * 変更の件数の 1 行（要件 2.5、2.6）。
 *
 * **種別ごとの数を全部出す**（「何件変わったか」だけでは、行が増えたのか値が変わったのかが
 * 分からない）。0 件のときは「変更はありません」と言い切る（0 件の内訳を並べない）。
 */
export function describeChanges(changes: MacroChangeCounts): string {
  if (changeTotal(changes) === 0) {
    return "変更はありません";
  }
  return `変更 ${String(changeTotal(changes))} 件（セル ${String(changes.set_cells)} / 追加 ${String(
    changes.inserted_rows,
  )} / 削除 ${String(changes.removed_rows)} / 複製 ${String(changes.duplicated_rows)}）`;
}

/** `console` の出力の 1 行（要件 2.3）。**種別を落とさない**（`error` は警告として読める）。 */
export function describeOutputLine(line: MacroOutputLine): string {
  return `[${line.level}] ${line.text}`;
}

/** 失敗の写像（境界の `MacroFailureReport` → 提示）。 */
export function failurePresentation(failure: MacroFailureReport): MacroFailurePresentation {
  return {
    layer: failureLayer(failure.kind),
    reason: failure.reason,
    frames: failure.frames,
  };
}

/** 失敗の種別の写像（要件 9.2 の拒んだ API の名前を落とさない）。 */
export function failureLayer(kind: MacroFailureTag): MacroFailureLayer {
  switch (kind.kind) {
    case "source":
      return { kind: "source" };
    case "transpile":
      return { kind: "transpile" };
    case "execution":
      return { kind: "execution" };
    case "host_rejected":
      return { kind: "host_rejected", api: kind.api };
    default:
      return assertNever(kind, "失敗の種別の分岐が網羅されていない");
  }
}

/**
 * 実行の結果の写像（要件 2.3、2.4、2.5、6.1、6.2）。
 *
 * `changed` を**ここで 1 度だけ**数える（表示中のシートを開き直す判断と、提示の「変更は
 * ありません」が同じ数を見る）。
 */
export function resultPresentation(outcome: MacroRunOutcome): MacroResultPresentation {
  switch (outcome.outcome) {
    case "Ran":
      return {
        kind: "ran",
        value: outcome.value,
        output: outcome.output,
        changes: outcome.changes,
        changed: changeTotal(outcome.changes) > 0,
        elapsedMs: outcome.elapsed_ms,
      };
    case "Failed":
      return { kind: "failed", failure: failurePresentation(outcome.failure) };
    case "Aborted":
      return {
        kind: "aborted",
        limit: outcome.limit,
        elapsedMs: outcome.elapsed_ms,
        failure: failurePresentation(outcome.failure),
      };
    default:
      return assertNever(outcome, "実行の結果の分岐が網羅されていない");
  }
}

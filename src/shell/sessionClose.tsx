/**
 * 終了前の問い — 終了拒否の理由と 3 択（保存して閉じる / 変更を破棄して閉じる /
 * 閉じるのをやめる）を、シェルのクロームに提示する。
 *
 * 所有: `SessionClosePrompt`（design.md「Components and Interfaces → Frontend Layer」の
 * `src/shell/SessionClosePrompt.tsx`、「System Flows → 終了前の問い」のシーケンス図）。
 * 要件: 6.1（拒否と理由）, 6.2（3 択の提示）, 6.3（保存の成功でのみ閉じ直す）, 6.4（失敗・
 * 取り消しでは閉じずに理由を示す）, 6.5（破棄の印）, 6.6（やめる）, 6.7（答え直す）。タスク 4.2。
 *
 * # 何を担い、何を担わないか
 *
 * 担うのは 3 つである:
 *
 * 1. **拒否の受け取り** — `./closeVeto` の差し替え口（`installCloseDenialHandler`）に
 *    ハンドラを 1 つ入れる（[`installSessionClosePrompt`]。`src/main.tsx` が 1 回だけ呼ぶ）。
 * 2. **モジュール局所のストア** — 受け取った拒否を置き、購読関数 + 取得関数を
 *    `useSyncExternalStore` に渡す。**2 つ目の React ルートは切らない**（例外隔離と配色の
 *    契約から外れるため。design.md の同節）。形は `./theme` の
 *    `subscribeAppearance` / `getAppearanceState` と `../features/diagnostics/requests.ts` の
 *    `subscribeRequestedSection` / `getRequestedSection` を写した。
 * 3. **3 択の提示と実行** — 保存・破棄・やめるの 1 つだけを行う（下の「3 択が何をするか」）。
 *
 * **担わないもの**: 閉じてよいかの判定（`./closeVeto` と Rust の `can_close_window`）、
 * ウィンドウの破棄（許可されたときに `./closeVeto` が `destroy()` する）、画面の描画
 * （`Layout` のクロームに 1 点だけ置く）。**自前のダイアログ機構を作らない** — ファイル選択の
 * ような別ウィンドウは要らず、提示はシェルのクロームの一部である（tasks.md 4.2 の本文）。
 *
 * # 3 択が何をするか（要件 6.3〜6.6）
 *
 * | 選択 | 呼ぶもの | 成功したとき | 失敗・取り消しのとき |
 * |---|---|---|---|
 * | 保存して閉じる | `documentSave`（4.1 の薄いラッパ） | **`Saved` のときだけ**閉じ直す | 提示を残し、理由を示す（6.4） |
 * | 変更を破棄して閉じる | `documentDiscard` | 閉じ直す | 提示を残し、理由を示す |
 * | 閉じるのをやめる | 何も呼ばない | 提示だけを消す（文書と未保存は不変。6.6） | — |
 *
 * **閉じ直すとは、閉じる要求をもう一度通すことである**（[`SessionCloseDenial.retry`] を
 * 呼ぶ）。判定をここで作り直さない — 決めるのは既存の経路（`./closeVeto` の往復）であり、
 * **その時点の状態に基づいて答え直される**（6.7）。したがってこのモジュールは判定を
 * キャッシュしない: 拒否は「提示するための材料」としてだけ持ち、次に閉じるときは必ず問い直す。
 * 保存が成功した場合は未保存の印が落ちているので、閉じ直しは `Allow` に到達して既存の経路が
 * `destroy()` する。**ウィンドウは利用者の選択が終わるまで生かしておく**（6.3 / 6.4。閉じる
 * 操作自体は `./closeVeto` が常に拒否しているので、提示が出ている間にウィンドウが消えることは
 * ない）。
 *
 * # 取消（`Cancelled`）を失敗として見せない
 *
 * 保存先の選択の取り消しは**失敗ではない**（要件 5.3。`DocumentSaveOutcome` の doc と同じ判断）。
 * したがって「保存できなかった」ではなく「選ばなかったので閉じられない」と述べる。どちらの
 * 場合も未保存は保たれ、提示は残る（6.4）。
 *
 * # 観測（3 OS の段とローカルの実測）
 *
 * 提示の器は `data-testid="jxcel-session-close-prompt"` を持ち、次の属性を出す:
 *
 * - `data-close-state` — `idle`（提示なし）/ `asking`（3 択を提示中）/ `saving` / `discarding`
 *   （実行中）/ `kept`（拒否を保ったまま理由を示している）
 * - `data-close-action` — 最後に選ばれた選択（`save` / `discard` / `cancel`。未選択は空）。
 *   **提示が消えたあとも読める**ので、「やめるが選ばれたこと」も外から観測できる
 * - `data-close-window` / `data-close-reason` — 拒否したウィンドウのラベルと理由
 * - `data-close-message` — 失敗・取り消しの理由（`kept` のときだけ）
 *
 * 3 つのボタンもそれぞれ `data-testid` を持つ（下の [`SessionClosePrompt`]）。
 *
 * # 失敗の扱い
 *
 * ラッパ（4.1）は例外を外へ出さない（封筒の `status` で分岐する）ため、ここに `try`/`catch` は
 * 要らない。IPC が無い環境（配信先中立の画面・素のブラウザ）では `status: "error"` が返るので、
 * 提示はそのまま残り理由が出る（**操作が無反応にならない**）。
 */
import { useSyncExternalStore, type ReactElement } from "react";

import { describeIpcError } from "../ipc/client";
import { documentDiscard, documentSave } from "../ipc/documentSession";
import { installCloseDenialHandler } from "./closeVeto";
import { APPEARANCE_VARS } from "./theme";

/**
 * 提示中の拒否 1 件。**取得関数が同じ参照を返せるよう、不変の値として作る**
 * （`./theme` の `AppearanceState` と同じ規律）。
 */
export interface SessionCloseDenial {
  /** 拒否ごとの識別子。応答が別の拒否のものになったときに古い結果を捨てるために使う。 */
  readonly id: number;
  /** 拒否した判定を返したウィンドウのラベル（`data-close-window` に出る）。 */
  readonly window: string;
  /** 委譲先が示した拒否の理由（`data-close-reason` に出る）。 */
  readonly reason: string;
  /** 閉じる要求をもう一度通す（**判定はここで作り直さない**。モジュール doc を参照）。 */
  readonly retry: () => void;
}

/** 利用者が選びうる 3 択。 */
export type SessionCloseAction = "save" | "discard" | "cancel";

/** 提示の段階（`data-close-state` に出る）。 */
export type SessionClosePhase = "idle" | "asking" | "saving" | "discarding" | "kept";

/** 提示の現在の姿。React へは [`useSessionClosePrompt`] が `useSyncExternalStore` で配る。 */
export interface SessionCloseState {
  /** 提示の段階。 */
  readonly phase: SessionClosePhase;
  /** 提示している拒否（無ければ `null`）。 */
  readonly denial: SessionCloseDenial | null;
  /** 最後に選ばれた選択（未選択は `null`）。**提示が消えたあとも保つ**（観測のため）。 */
  readonly lastAction: SessionCloseAction | null;
  /** 失敗・取り消しの理由（`kept` のときだけ `null` でない）。 */
  readonly message: string | null;
}

/**
 * 提示が無く、まだ何も選ばれていない初期の姿。
 *
 * **`lastAction` を保ったまま `phase` と `denial` だけを戻す**ため、状態の生成は
 * [`setState`] の部分更新に閉じる（呼び出し側が 4 つのフィールドを毎回書かない）。
 */
const INITIAL_STATE: SessionCloseState = {
  phase: "idle",
  denial: null,
  lastAction: null,
  message: null,
};

let state: SessionCloseState = INITIAL_STATE;
/** 拒否ごとの識別子。**単調増加**であり、値そのものに意味は無い（同一性の判定だけに使う）。 */
let nextDenialId = 1;
const listeners = new Set<() => void>();

/** 状態を差し替え、変わっていれば購読者へ知らせる（`./theme` の `setState` と同じ形）。 */
function setState(next: SessionCloseState): void {
  const changed =
    next.phase !== state.phase ||
    next.denial !== state.denial ||
    next.lastAction !== state.lastAction ||
    next.message !== state.message;
  state = next;
  if (!changed) {
    return;
  }
  for (const listener of [...listeners]) {
    listener();
  }
}

/**
 * 提示中の拒否（`useSyncExternalStore` の取得関数。変化が無ければ同じ参照を返す）。
 *
 * ストアの形は `./theme` / `../features/diagnostics/requests.ts` と同じである（購読関数 +
 * 取得関数の対）。**2 つ目の React ルートを切らない**ため、配布はこの 1 対だけを通る。
 */
export function getSessionCloseState(): SessionCloseState {
  return state;
}

/** 提示の変化の購読（`useSyncExternalStore` の購読関数）。 */
export function subscribeSessionCloseState(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * 終了拒否の理由を提示へつなぐ。**`src/main.tsx` が起動時に 1 回だけ呼ぶ。**
 *
 * `./closeVeto` の差し替え口は 1 つだけであり、ここで入れるハンドラが**既定の記録
 * （`console.warn`）を置き換える**。拒否は提示へ置かれ、`Layout` のクロームが描く。加えて
 * [`SessionCloseDenial.retry`] を預かるので、3 択の「閉じ直す」は既存の閉じる経路へ戻る。
 *
 * **2 回呼んでも安全**（差し替え口は後から入れたものを覚える）。起動時の 1 回だけを想定して
 * いるが、例外を投げないので起動を止めない。
 */
export function installSessionClosePrompt(): void {
  installCloseDenialHandler((denial) => {
    const id = nextDenialId;
    nextDenialId += 1;
    // 拒否が来た＝閉じる操作が拒否された。**前の失敗の文言は捨てる**（新しい問いである）。
    setState({
      phase: "asking",
      denial: { id, window: denial.window, reason: denial.reason, retry: denial.retry },
      lastAction: state.lastAction,
      message: null,
    });
  });
}

/**
 * 提示を消してから、閉じる要求をもう一度通す（3 択のうち閉じる 2 つが成功したとき）。
 *
 * **判定をここで作らない** — `retry` は既存の閉じる経路（`./closeVeto` の往復）へ戻るだけ
 * であり、その時点の状態に基づいて答え直される（要件 6.7）。
 */
function dismissAndRetry(denial: SessionCloseDenial, action: SessionCloseAction): void {
  setState({ phase: "idle", denial: null, lastAction: action, message: null });
  denial.retry();
}

/** 「保存して閉じる」（要件 6.3、6.4）。 */
async function chooseSave(denial: SessionCloseDenial): Promise<void> {
  setState({ phase: "saving", denial, lastAction: "save", message: null });
  const result = await documentSave();
  if (state.denial?.id !== denial.id) {
    return;
  }
  if (result.status === "error") {
    setState({
      phase: "kept",
      denial,
      lastAction: "save",
      message: `保存できなかった。閉じるのをやめるか、もう一度試すこと: ${describeIpcError(result.error)}`,
    });
    return;
  }
  switch (result.data.outcome.outcome) {
    case "Saved":
      // **成功したときだけ閉じ直す**（6.3）。保存で未保存が落ちているので、閉じ直しは
      // 既存の経路で許可される。
      dismissAndRetry(denial, "save");
      return;
    case "Cancelled":
      // 取り消しは失敗ではない（要件 5.3）。未保存は保たれ、提示は残る（6.4）。
      setState({
        phase: "kept",
        denial,
        lastAction: "save",
        message: "保存先が選ばれなかったため、未保存の変更はそのままである",
      });
      return;
    case "Failed":
      setState({
        phase: "kept",
        denial,
        lastAction: "save",
        message: `書き出せなかった: ${result.data.outcome.reason}`,
      });
      return;
  }
}

/** 「変更を破棄して閉じる」（要件 6.5）。 */
async function chooseDiscard(denial: SessionCloseDenial): Promise<void> {
  setState({ phase: "discarding", denial, lastAction: "discard", message: null });
  const result = await documentDiscard();
  if (state.denial?.id !== denial.id) {
    return;
  }
  if (result.status === "error") {
    setState({
      phase: "kept",
      denial,
      lastAction: "discard",
      message: `変更を破棄できなかったため、未保存の変更はそのままである: ${describeIpcError(result.error)}`,
    });
    return;
  }
  // 破棄の印が付いた（または保持していないウィンドウでは何も変わらなかった）。**閉じ直す** —
  // 印が付けば未保存が落ちており、既存の経路が許可する。付けられなかった場合は同じ拒否が
  // もう一度届き、提示が理由とともに戻る（判定をここで作らない。6.7）。
  dismissAndRetry(denial, "discard");
}

/**
 * 提示を読むフック。`src/shell/Layout.tsx` のクロームが使う。
 *
 * 3 つの操作を渡す。**文書と未保存を変えるのは保存と破棄だけで、「やめる」は何も変えない**
 * （要件 6.6）。
 */
export interface SessionClosePromptController {
  /** 提示の現在の姿（`data-close-*` 属性の材料）。 */
  readonly state: SessionCloseState;
  /** 「保存して閉じる」。 */
  readonly save: () => void;
  /** 「変更を破棄して閉じる」。 */
  readonly discard: () => void;
  /** 「閉じるのをやめる」。文書と未保存の状態は変わらない。 */
  readonly cancel: () => void;
}

/** 提示の現在の姿を React から読む。 */
export function useSessionClosePrompt(): SessionClosePromptController {
  const snapshot = useSyncExternalStore(
    subscribeSessionCloseState,
    getSessionCloseState,
    getSessionCloseState,
  );
  return {
    state: snapshot,
    save: () => {
      const denial = state.denial;
      if (denial === null) {
        return;
      }
      void chooseSave(denial);
    },
    discard: () => {
      const denial = state.denial;
      if (denial === null) {
        return;
      }
      void chooseDiscard(denial);
    },
    cancel: () => {
      // **何も呼ばない。** 提示を消すだけであり、文書も未保存の状態も変えない（6.6）。
      setState({ phase: "idle", denial: null, lastAction: "cancel", message: null });
    },
  };
}

/** 提示の枠。**配色は器が与えるカスタムプロパティだけを参照する**（画面の契約 4）。 */
const PROMPT_STYLE = {
  display: "flex",
  alignItems: "flex-start",
  justifyContent: "space-between",
  gap: "1rem",
  padding: "0.75rem 1.5rem",
  backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
  borderTop: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/**
 * 提示が無いときの見た目。**`display: none` を明示する** — 属性（`hidden`）が与える
 * `display: none` は `PROMPT_STYLE` の `display: flex` に負けるためである
 * （[`SessionClosePrompt`] の doc）。属性は DOM の外からも観測できるよう残す。
 */
const HIDDEN_STYLE = { display: "none" } as const;

/** 理由・失敗の行の見た目（器の与える補助的な文字色を使う）。 */
const MUTED_STYLE = {
  margin: 0,
  fontSize: "0.8125rem",
  color: `var(${APPEARANCE_VARS.screenMuted})`,
} as const;

/** 3 択のボタンの見た目（外観を選ぶ操作と同じ変数を使う。自前の色を持たない）。 */
const BUTTON_STYLE = {
  font: "inherit",
  fontSize: "0.8125rem",
  lineHeight: 1.4,
  padding: "0.25rem 0.75rem",
  borderRadius: "0.25rem",
  cursor: "pointer",
  color: `var(${APPEARANCE_VARS.controlActiveText})`,
  backgroundColor: `var(${APPEARANCE_VARS.controlActiveBackground})`,
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/**
 * 終了前の問い。**シェルのクロームに 1 点だけ置く**（`ShellRegion` の中には置かない）。
 *
 * 提示が無いときは器だけを残して隠す。**器を残す理由**は観測である — `data-close-action` は
 * 提示が消えたあとも読めるので、「やめる」や「破棄して閉じる」が選ばれたことを 3 OS の段が
 * 確認できる（モジュール doc「観測」）。隠れた要素はアクセシビリティの木にも現れないので、
 * 提示が無いときに読み上げへ混ざることもない。
 *
 * **`display: none` を style で明示する**（`hidden` 属性だけに頼らない）。この要素は
 * `PROMPT_STYLE` の `display: flex` を持つため、`hidden` 属性が与える `display: none` は
 * インライン style に負けて効かない — 属性だけでは**提示が無いときに行が 1 本残る**
 * （枠線と背景だけの帯が chrome と領域の間に見える）。
 */
export function SessionClosePrompt(): ReactElement {
  const { state: close, save, discard, cancel } = useSessionClosePrompt();
  const denial = close.denial;
  const busy = close.phase === "saving" || close.phase === "discarding";
  return (
    <section
      data-testid="jxcel-session-close-prompt"
      data-close-state={close.phase}
      data-close-action={close.lastAction ?? ""}
      data-close-window={denial?.window ?? ""}
      data-close-reason={denial?.reason ?? ""}
      aria-label="終了前の確認"
      hidden={denial === null}
      style={denial === null ? HIDDEN_STYLE : PROMPT_STYLE}
    >
      {denial === null ? null : (
        <>
          <div style={{ display: "flex", flexDirection: "column", gap: "0.25rem" }}>
            <p style={{ margin: 0 }}>
              未保存の変更があるため、このウィンドウはまだ閉じられない。
            </p>
            <p data-testid="jxcel-session-close-reason" style={MUTED_STYLE}>
              {denial.reason}
            </p>
            {close.message === null ? null : (
              <p data-testid="jxcel-session-close-message" style={MUTED_STYLE}>
                {close.message}
              </p>
            )}
          </div>
          <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
            <button
              type="button"
              data-testid="jxcel-session-close-save"
              disabled={busy}
              onClick={save}
              style={BUTTON_STYLE}
            >
              保存して閉じる
            </button>
            <button
              type="button"
              data-testid="jxcel-session-close-discard"
              disabled={busy}
              onClick={discard}
              style={BUTTON_STYLE}
            >
              変更を破棄して閉じる
            </button>
            <button
              type="button"
              data-testid="jxcel-session-close-cancel"
              disabled={busy}
              onClick={cancel}
              style={BUTTON_STYLE}
            >
              閉じるのをやめる
            </button>
          </div>
        </>
      )}
    </section>
  );
}

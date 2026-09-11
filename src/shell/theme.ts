/**
 * 明色・暗色の外観と OS の外観設定への追随。
 *
 * 所有: `ShellLayout` の外観部分（design.md「Components and Interfaces → Frontend Layer」、
 * 「Directory Structure」の `src/shell/theme.ts`）。
 * 要件: 9.3（明色と暗色の提供と既定での OS 追随）, 9.4（明示選択の優先と再起動後の維持）。
 *
 * タスク 1.4 が置いた骨組み（`export {}`）をタスク 9.2 が実体にした。**遷移の仕組み
 * （`./router`）には触れない**（要件 9.2 の契約を動かさない）。
 *
 * # 解決後の外観がどこに載るか（**単一の源**）
 *
 * 適用先は `document.documentElement`（`<html>`）ただ 1 つであり、次の 3 つを同じ 1 回の
 * 適用で書く:
 *
 * 1. **`data-appearance`** — 解決後の外観（`"light"` / `"dark"`）。外部の検査はこの 1 属性を
 *    読めばよい（例: `document.documentElement.getAttribute("data-appearance")`）。
 * 2. **`data-appearance-choice`** — 利用者の選択（`"system"` / `"light"` / `"dark"`）。
 *    `system` のときは OS の値が 1 に反映される。
 * 3. **CSS カスタムプロパティ**（[`APPEARANCE_VARS`]）と **`color-scheme`**。
 *
 * カスタムプロパティは `<html>` から継承するため、**シェル自身のクローム
 * （`src/shell/Layout.tsx` の `<main data-testid="jxcel-shell">` と
 * `data-testid="jxcel-shell-chrome"`）も、その中に差し込まれる個別機能の画面も、同じ 1 組の
 * 配色に従う**。画面は `var(--jxcel-…)` を参照するだけでよく、自前の配色を持たない
 * （`Layout.tsx` の「画面の契約」4）。値の定義は [`PALETTES`] の 1 箇所だけである。
 *
 * `color-scheme` を併せて書くのは、スクロールバーやフォーム部品など**ブラウザが描く部分**を
 * 外観に合わせるためである（カスタムプロパティでは届かない）。
 *
 * # 既定は OS 追随、明示選択は優先
 *
 * - OS の値は `prefers-color-scheme: dark` から読む。`matchMedia` の `change` を購読するので、
 *   **起動中に OS の外観が切り替わっても追随する**（選択が `system` のときだけ）。
 * - 利用者の選択は設定 `appearance.theme`（`crates/app-shell/src/settings/mod.rs` の
 *   `SettingsKey::AppearanceTheme`。design.md「Logical Data Model」の
 *   `appearance.theme` = `system` / `light` / `dark`）に、**既存の設定コマンド**
 *   `settings_get` / `settings_set` 経由で保存する。新しい鍵もコマンドも作らない。
 *
 * # 初回描画で誤った外観を見せない工夫
 *
 * `src/main.tsx` は [`bootstrapAppearance`] の解決を待ってから React をマウントする。
 * したがって**シェルの最初の描画は解決済みの外観で行われる**（画面の中身が既定の外観で
 * 一瞬描かれてから切り替わる、という往復は起きない）。加えて `src/index.html` の下地は
 * `prefers-color-scheme` の媒体クエリで OS に追随させてあり、**スクリプトが動く前の空白も
 * 既定（OS 追随）の色**になる。明示選択が OS と食い違う場合に限り、IPC の往復（数ミリ秒）
 * の間だけ空白が OS の色になりうるが、シェルの内容が誤った外観で描かれることはない。
 * 解決は `APPEARANCE_READ_TIMEOUT_MS` で打ち切るため、**読み取りが固まってもシェルは必ず
 * 描かれる**。
 *
 * # 失敗したときの振る舞い（**空白・壊れたシェルを作らない**）
 *
 * | 事象 | 振る舞い |
 * |---|---|
 * | 設定が未設定（値が `null`） | 既定の `system`（OS 追随）。失敗ではない |
 * | 保存値が解釈できない（文字列でない・未知の文字列） | 警告を 1 行残し、`system`（OS 追随）へ落とす。**ファイルは書き換えない** |
 * | 読み取りが失敗（IPC 不在・封筒の失敗・期限内に応答なし） | 警告を 1 行残し、`system`（OS 追随）で起動する。シェルは通常どおり描かれる |
 * | 書き込みが失敗（IPC 不在・封筒の失敗） | **その起動の間は選択をメモリに保持**して見た目へ反映する（利用者の操作が無反応にならない）。警告を 1 行残し、次回起動時は保存済みの値（何も無ければ `system`）から始まる |
 * | 変更通知の購読に失敗（IPC 不在） | 警告を 1 行残して諦める。自ウィンドウの追随・選択は影響を受けない |
 *
 * どの経路でも例外を外へ出さない。呼び出し側（`src/main.tsx` / `Layout`）は外観の初期化で
 * 起動を止めなくてよい。
 */

import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";

import { SETTINGS_CHANGED_EVENT } from "../ipc/bindings";
import type { SettingsChangedEvent, SettingsResponse } from "../ipc/bindings";
import { invokeCommand, type CommandName } from "../ipc/client";

/**
 * 設定コマンドの名前。**文字列を直接 `invoke` へ渡さない。** 型注釈（[`CommandName`]）は
 * 生成物 `src/ipc/bindings.ts` の `COMMAND_NAMES` から導かれた合併型であるため、
 * `crates/app-shell/src/ipc/command_names.rs` からこの名前が消えるとこの行で型検査が落ちる
 * （tasks.md 2.2 / 2.4 の拡張規則）。
 */
const SETTINGS_GET_COMMAND: CommandName = "settings_get";
const SETTINGS_SET_COMMAND: CommandName = "settings_set";

/**
 * 外観の選択を保存する設定キーの名前。
 *
 * `crates/app-shell/src/settings/mod.rs` の `SettingsKey::AppearanceTheme` の
 * `as_str()` と一致する。**カタログは閉じており、ここで新しい鍵を作らない。**
 */
export const APPEARANCE_THEME_SETTINGS_KEY = "appearance.theme";

/** 解決後の外観（実際に描かれる明暗）。 */
export type Appearance = "light" | "dark";

/** 利用者の選択。`system` は「OS に追随する」を意味する。 */
export type AppearanceChoice = "system" | Appearance;

/**
 * シェルと画面が参照する CSS カスタムプロパティの名前。**値の定義は [`PALETTES`] の 1 箇所。**
 *
 * `Layout.tsx` と個別機能の画面は `var(--jxcel-…)` を参照する。名前をここへ集めておくのは、
 * 参照側と定義側の綴りがずれないようにするためである。
 */
export const APPEARANCE_VARS = {
  /** シェル全体（`<main data-testid="jxcel-shell">`）の面の色。 */
  shellSurface: "--jxcel-shell-surface",
  /** ヘッダ帯（`data-testid="jxcel-shell-chrome"`）の背景。**両外観でアクセント色**。 */
  shellChrome: "--jxcel-shell-chrome",
  /** ヘッダ帯の文字色。 */
  shellChromeText: "--jxcel-shell-chrome-text",
  /** 画面の外（シェル）の文字色。 */
  shellText: "--jxcel-shell-text",
  /** 画面の面の色。 */
  screenPanel: "--jxcel-screen-panel",
  /** 画面の文字色。 */
  screenText: "--jxcel-screen-text",
  /** 画面の補助的な文字色。 */
  screenMuted: "--jxcel-screen-muted",
  /** 外観を選ぶ操作の枠線の色（ヘッダ帯の上に載る）。 */
  controlBorder: "--jxcel-control-border",
  /** 選ばれている操作の背景色。 */
  controlActiveBackground: "--jxcel-control-active-background",
  /** 選ばれている操作の文字色。 */
  controlActiveText: "--jxcel-control-active-text",
} as const;

/**
 * 初期画面の識別色。**白背景・暗背景のどちらとも一致しない**ことを 10.4 の画素検査の根拠に
 * する（1.4 の Implementation Notes の目印）。両方の外観でヘッダ帯の背景に使うので、外観に
 * よらず常に画面のどこかに現れる。`Layout.tsx` は後方互換のためこの名前を再輸出する。
 */
export const INITIAL_SCREEN_ACCENT_COLOR = "#c2185b";

/**
 * 外観ごとの配色。**値の唯一の定義。** 鍵は [`APPEARANCE_VARS`] のカスタムプロパティ名である。
 *
 * アクセント色（`INITIAL_SCREEN_ACCENT_COLOR`）は**両方の外観でヘッダ帯に残す**。明暗の差は
 * シェルの面・画面の面・文字色で付ける（10.4 の画素検査の目印を外観で消さない）。
 */
const PALETTES: Readonly<Record<Appearance, Readonly<Record<string, string>>>> = {
  light: {
    [APPEARANCE_VARS.shellSurface]: "#f4f4f6",
    [APPEARANCE_VARS.shellChrome]: INITIAL_SCREEN_ACCENT_COLOR,
    [APPEARANCE_VARS.shellChromeText]: "#ffffff",
    [APPEARANCE_VARS.shellText]: "#212121",
    [APPEARANCE_VARS.screenPanel]: "#ffffff",
    [APPEARANCE_VARS.screenText]: "#212121",
    [APPEARANCE_VARS.screenMuted]: "#5f6368",
    [APPEARANCE_VARS.controlBorder]: "rgba(255, 255, 255, 0.7)",
    [APPEARANCE_VARS.controlActiveBackground]: "#ffffff",
    [APPEARANCE_VARS.controlActiveText]: INITIAL_SCREEN_ACCENT_COLOR,
  },
  dark: {
    [APPEARANCE_VARS.shellSurface]: "#121216",
    [APPEARANCE_VARS.shellChrome]: INITIAL_SCREEN_ACCENT_COLOR,
    [APPEARANCE_VARS.shellChromeText]: "#ffffff",
    [APPEARANCE_VARS.shellText]: "#e6e6ea",
    [APPEARANCE_VARS.screenPanel]: "#1d1d24",
    [APPEARANCE_VARS.screenText]: "#e6e6ea",
    [APPEARANCE_VARS.screenMuted]: "#a2a2ad",
    [APPEARANCE_VARS.controlBorder]: "rgba(255, 255, 255, 0.7)",
    [APPEARANCE_VARS.controlActiveBackground]: "#ffffff",
    [APPEARANCE_VARS.controlActiveText]: INITIAL_SCREEN_ACCENT_COLOR,
  },
};

/** OS の外観を問い合わせる媒体クエリ。 */
const OS_DARK_QUERY = "(prefers-color-scheme: dark)";

/**
 * 設定の読み取りを打ち切るまでの時間。**初回描画をこの時間より長く止めない**（描画監視の
 * 期限は 3 秒であり、それより十分短く取る。タスク 8.2 の `FIRST_PAINT_DEADLINE`）。
 */
const APPEARANCE_READ_TIMEOUT_MS = 1500;

/** 現在の外観。React へは [`useAppearance`] が `useSyncExternalStore` で配る。 */
export interface AppearanceState {
  /** 利用者の選択（`system` / `light` / `dark`）。 */
  readonly choice: AppearanceChoice;
  /** 実際に描かれる外観（`light` / `dark`）。 */
  readonly resolved: Appearance;
}

let state: AppearanceState = { choice: "system", resolved: "light" };
const listeners = new Set<() => void>();
let osQuery: MediaQueryList | null = null;
let settingsSubscriptionStarted = false;

/** 選択と OS の値から、実際に描く外観を決める（純粋な関数）。 */
function resolveAppearance(choice: AppearanceChoice, osPrefersDark: boolean): Appearance {
  if (choice === "system") {
    return osPrefersDark ? "dark" : "light";
  }
  return choice;
}

/**
 * 解決後の外観を **`<html>` の 1 箇所**へ書く（モジュール doc「解決後の外観がどこに載るか」）。
 *
 * CSS カスタムプロパティと `color-scheme` は `style.setProperty` / `style.colorScheme` で
 * 書く。**`setAttribute("style", …)` は使わない** — アプリの CSP は `style-src 'self'` であり、
 * インラインの style 属性の解析は遮断される（tasks.md 8.2 の実測）。CSSOM の直接操作は
 * 遮断されない。
 */
function applyAppearance(choice: AppearanceChoice, resolved: Appearance): void {
  const root = document.documentElement;
  root.setAttribute("data-appearance", resolved);
  root.setAttribute("data-appearance-choice", choice);
  root.style.colorScheme = resolved;
  for (const [name, value] of Object.entries(PALETTES[resolved])) {
    root.style.setProperty(name, value);
  }
}

/** 状態を差し替え、変わっていれば購読者へ知らせる。適用は毎回行う（冪等）。 */
function setState(next: AppearanceState): void {
  applyAppearance(next.choice, next.resolved);
  const changed = next.choice !== state.choice || next.resolved !== state.resolved;
  state = next;
  if (!changed) {
    return;
  }
  for (const listener of [...listeners]) {
    listener();
  }
}

/** 選択を適用する（`system` のときは `matchMedia` から OS の現在の値を読む）。 */
function applyChoice(choice: AppearanceChoice): void {
  const prefersDark =
    typeof window.matchMedia === "function" &&
    window.matchMedia(OS_DARK_QUERY).matches;
  setState({ choice, resolved: resolveAppearance(choice, prefersDark) });
}

/** OS の外観の変化を購読する。**選択が `system` のときだけ追随する。** */
function installOsListener(): void {
  if (osQuery !== null || typeof window.matchMedia !== "function") {
    return;
  }
  const query = window.matchMedia(OS_DARK_QUERY);
  osQuery = query;
  query.addEventListener("change", (event: MediaQueryListEvent) => {
    if (state.choice !== "system") {
      return;
    }
    setState({ choice: "system", resolved: event.matches ? "dark" : "light" });
  });
}

/** 設定値（`unknown`）を選択へ解釈する。解釈できない値は `null`。 */
function parseChoice(value: unknown): AppearanceChoice | null {
  return value === "system" || value === "light" || value === "dark" ? value : null;
}

/**
 * 保存された選択を読む。**失敗はすべて `system`（OS 追随）へ落とす**（モジュール doc の表）。
 *
 * 期限 [`APPEARANCE_READ_TIMEOUT_MS`] を超えたら打ち切り、`system` を返す。これにより
 * 設定経路がどれだけ壊れていてもシェルは描かれる。
 */
async function readStoredChoice(): Promise<AppearanceChoice> {
  const timeout = new Promise<null>((resolve) => {
    setTimeout(() => {
      resolve(null);
    }, APPEARANCE_READ_TIMEOUT_MS);
  });
  const read = invokeCommand<SettingsResponse>(SETTINGS_GET_COMMAND, {
    request: { key: APPEARANCE_THEME_SETTINGS_KEY },
  }).catch(() => null);

  const result = await Promise.race([read, timeout]);

  if (result === null) {
    console.warn(
      "外観の設定を読み取れなかったため、OS の外観設定に追随する（既定）",
    );
    return "system";
  }
  if (result.status === "error") {
    console.warn(
      `外観の設定を読み取れなかったため、OS の外観設定に追随する（既定）: ${result.error.kind}`,
    );
    return "system";
  }

  const value = result.data.value;
  if (value === null) {
    // 未設定は既定（OS 追随）であり、失敗ではない。
    return "system";
  }
  const choice = parseChoice(value);
  if (choice === null) {
    console.warn(
      `外観の設定値が解釈できないため、OS の外観設定に追随する（既定）: ${JSON.stringify(value)}`,
    );
    return "system";
  }
  return choice;
}

/** 他のウィンドウ・外部からの変更を購読する（設定は全ウィンドウで共有される。要件 7.3）。 */
function installSettingsListener(): void {
  if (settingsSubscriptionStarted) {
    return;
  }
  settingsSubscriptionStarted = true;
  void (async () => {
    try {
      await listen<SettingsChangedEvent>(SETTINGS_CHANGED_EVENT, (event) => {
        const payload = event.payload;
        if (payload.key !== APPEARANCE_THEME_SETTINGS_KEY) {
          return;
        }
        const choice = parseChoice(payload.value);
        if (choice === null) {
          console.warn(
            `外観の設定値が解釈できないため、OS の外観設定に追随する（既定）: ${JSON.stringify(
              payload.value,
            )}`,
          );
          applyChoice("system");
          return;
        }
        applyChoice(choice);
      });
    } catch (error: unknown) {
      // IPC が無い環境（配信先中立の画面・素のブラウザ）では購読できない。自ウィンドウの
      // 追随と選択は影響を受けないので、警告 1 行に留める。
      console.warn("設定変更の購読を登録できなかった", error);
    }
  })();
}

/**
 * 外観を初期化する。**`src/main.tsx` が React のマウント前に 1 回だけ呼ぶ。**
 *
 * 順序は次のとおりである:
 *
 * 1. OS の変化の購読を張る（同期）。
 * 2. 既定（`system` = OS 追随）を**先に適用**する。これだけで「設定が読めないときも
 *    動くシェル」が成立する。
 * 3. 保存された選択を読み、あれば適用する。**シェルの最初の描画はこの後に起きる**ので、
 *    誤った外観が描かれてから直ることはない。
 * 4. 設定変更の購読を張る（失敗しても起動を止めない）。
 *
 * **例外を外へ出さない。** 途中で何が起きても 2 の外観で起動できる。
 */
export async function bootstrapAppearance(): Promise<AppearanceState> {
  installOsListener();
  applyChoice("system");

  const stored = await readStoredChoice();
  if (stored !== "system") {
    applyChoice(stored);
  }

  installSettingsListener();
  return state;
}

/**
 * 利用者の選択を適用し、設定へ保存する（要件 9.4）。
 *
 * **先に適用し、後から保存する。** 保存が失敗しても選択はその起動の間だけメモリに残るので、
 * 利用者の操作が無反応にならない（モジュール doc の表）。保存に失敗した事実は警告 1 行に
 * 残す。**例外を外へ出さない。**
 */
export async function setAppearanceChoice(choice: AppearanceChoice): Promise<void> {
  applyChoice(choice);
  const result = await invokeCommand<SettingsResponse>(SETTINGS_SET_COMMAND, {
    request: { key: APPEARANCE_THEME_SETTINGS_KEY, value: choice },
  });
  if (result.status === "error") {
    console.warn(
      `外観の選択を保存できなかった（${result.error.kind}）。この起動の間だけ保持する`,
    );
  }
}

/** 現在の外観。`useSyncExternalStore` の取得関数（変化が無ければ同じ参照を返す）。 */
export function getAppearanceState(): AppearanceState {
  return state;
}

/** 外観の変化の購読。`useSyncExternalStore` の購読関数。 */
export function subscribeAppearance(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * 外観を読むフック。`src/shell/Layout.tsx` のクローム（および外観を選ぶ操作）が使う。
 *
 * 配色そのものは CSS カスタムプロパティが担うため、フックが必要とするのは**選択の表示**と
 * **選択の入口**だけである。個別機能の画面はフックを呼ばず、`var(--jxcel-…)` を参照する
 * （画面の契約 4。画面は `ScreenProps` 以外を受け取らない）。
 */
export interface AppearanceController extends AppearanceState {
  /** 利用者の選択を適用して保存する（保存に失敗してもこの起動の間は保持される）。 */
  readonly setChoice: (choice: AppearanceChoice) => void;
}

/** 外観の現在の姿を React から読む。 */
export function useAppearance(): AppearanceController {
  const snapshot = useSyncExternalStore(
    subscribeAppearance,
    getAppearanceState,
    getAppearanceState,
  );
  return {
    choice: snapshot.choice,
    resolved: snapshot.resolved,
    setChoice: (choice: AppearanceChoice) => {
      void setAppearanceChoice(choice);
    },
  };
}

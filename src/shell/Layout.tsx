/**
 * シェルのレイアウト — 個別機能の画面が差し込まれる領域（`ShellRegion`）と、その外側の
 * シェル自身のクロームを定義する器。
 *
 * 所有: `ShellLayout`（design.md「Components and Interfaces → Frontend Layer」、
 * 「Directory Structure」の `src/shell/Layout.tsx`）。
 * 要件: 1.1, 1.2（3 OS で初期画面が描画されること）, 9.1（画面が差し込まれる領域の定義）,
 * 9.3, 9.4（明暗の外観と、明示選択の優先・永続化。タスク 9.2）。
 *
 * # 画面の契約（**個別機能が守る側**）
 *
 * 画面は [`ScreenDefinition`]（`./router`）として登録され、[`ShellRegion`] の中だけで描画される。
 * したがって画面は次を守る。ここに書かれた以外の前提を持ってはならない。
 *
 * 1. **受け取るのは `ScreenProps` だけ** (`screenId` と `navigate`)。それ以外の props は無い。
 * 2. **自前のレイアウトを作らない。** ウィンドウ全体を覆う背景・ヘッダ・フッタ・固定配置
 *    （`position: fixed`）はシェルのクロームが持つ。画面は領域の内側の内容だけを描く
 *    （領域は `flex: 1 1 auto` と `min-height: 0` を持つので、内容はそこでスクロールする）。
 * 3. **自前の遷移を持たない。** `history` / `location` / ハッシュ / 独自のルーターを参照せず、
 *    遷移は受け取った `navigate` だけを呼ぶ（要件 9.2。機構は `./router` の 1 つだけである）。
 * 4. **自前で配色を決めない。** 明暗の外観はシェルが持ち（タスク 9.2 の `./theme`）、
 *    画面は `var(--jxcel-…)` を参照するだけである（下の「外観」を参照）。
 *
 * # 外観（要件 9.3、9.4。タスク 9.2）
 *
 * 明暗の解決・適用・永続化は `./theme` が単独で持つ。**解決後の外観は `<html>` の
 * `data-appearance` と、同じ要素に書かれる CSS カスタムプロパティ（`APPEARANCE_VARS`）の
 * 1 組に載る。** カスタムプロパティは継承するので、このファイルの `<main>` とヘッダ帯、および
 * 領域へ差し込まれる個別機能の画面は、**同じ 1 組の配色**に従う。このファイルは色の値を持たず、
 * `var(--jxcel-…)` を参照するだけである（画面の契約 4）。
 *
 * ヘッダ帯には外観を選ぶ最小の操作（`data-testid="jxcel-appearance-control"`）を置く。これは
 * **シェルのクロームの一部であり、領域（`ShellRegion`）の中には置かない** — 9.1 の領域の識別
 * （`data-shell-screen`）・遷移の契約と、画面へ渡す props の契約に触れないためである。
 *
 * # 現在画面の識別（外部から観測できる形・再掲）
 *
 * `ShellRegion` は表示中の画面の識別子を **`data-shell-screen` 属性**に書き出す。検査は
 * `data-testid="jxcel-shell-region"` の要素 1 つを見れば足りる:
 *
 * ```js
 * document.querySelector('[data-testid="jxcel-shell-region"]').getAttribute("data-shell-screen")
 * ```
 *
 * 題名（利用者に見える名前）は `data-testid="jxcel-shell-screen-title"` の要素に文字として出る。
 *
 * # 3 OS の描画確認（10.4）が目印にするもの
 *
 * **アクセント色 `INITIAL_SCREEN_ACCENT_COLOR`（`#c2185b` = `rgb(194, 24, 91)`）は両方の外観で
 * ヘッダ帯（`data-testid="jxcel-shell-chrome"`）の背景に使う。** 帯はアプリ名 `jxcel` を白字で
 * 出し、高さは約 0.75rem + 文字なので上端 10 px 付近は必ずアクセント色である。画面が領域を
 * 自前の色で覆っても、このヘッダ帯は常に残る。したがって 10.4 は **画像中に
 * `rgb(194, 24, 91)` が存在すること**、またはヘッダ帯の画素で確認できる（1.4 の Implementation
 * Notes の目印をそのまま保っている）。明暗の差はシェルの面と画面の面・文字色で付けるので、
 * **外観によって目印が消えることはない**。
 *
 * # 後続タスクがここへ差し込む場所
 *
 * - **9.3（画面単位のエラー隔離）— 実装済み**: `ShellRegion` の中の `<Screen … />` 1 式を
 *   `./ScreenBoundary` で包んだ。**境界の粒度は画面 1 つ**（`screen.id` を `key` と
 *   `screenId` に与える）。シェルのクロームと領域そのものは包まない。エラー時は同じ領域に
 *   提示（`[data-testid="jxcel-screen-error"]`）が出て、境界は `key` の変化で作り直される
 *   （詳細は `./ScreenBoundary` のモジュール doc）。
 * - **9.7（3 OS 描画確認の最小画面）— 実装済み**: `src/features/smoke/` の 2 画面
 *   （`smoke-table` / `smoke-editor`）を `ScreenDefinition` として `SHELL_SCREEN_REGISTRY` に
 *   足した。**遷移機構には触れていない**（足すだけで表示できる）。描画の通知
 *   （`./renderHeartbeat`）も 8.2 の 1 本をそのまま再利用し、**2 つ目の発信側を足していない**
 *   （通知はウィンドウごと・起動ごとに 1 回であり、そのウィンドウが最初に表示した画面の描画を
 *   証拠にする）。**ユーザー向けの導線は持たない**（9.6 の空ウィンドウの画面にスモークの項目を
 *   足していない）。10.4 は検証専用の経路（`./verificationScreen` の
 *   `JXCEL_VERIFICATION_INITIAL_SCREEN`）で初期画面として選び、**1 画面につき 1 回起動する**。
 * - **9.5（診断の導線）— 実装済み**: `src/features/diagnostics/` の画面を `ScreenDefinition`
 *   として `SHELL_SCREEN_REGISTRY` に足し（`initial` は変えない）、メニューの選択を画面へ
 *   引き渡す購読を `installDiagnosticsRequests` で 1 回だけ張る（購読が持つのは**区画の
 *   選択だけ**で、遷移は [`useShellRouter`] の `navigate` を通る）。**3 つの導線の提示と
 *   操作は画面の中だけで完結する**ので、メニューが無い環境（素のブラウザ）でも使える。
 * - **9.4 / 9.6**: このファイルに領域を増やさない。配信先中立の資産は `src/shared/` に、
 *   空ウィンドウの操作は画面として登録する。
 * - **9.6（空ウィンドウの操作導線）— 実装済み**: `src/features/empty/` の画面を
 *   `ScreenDefinition` として登録し、**`initial` をその画面にする**。理由は、この画面が
 *   唯一、ウィンドウの状態（要件 2.2）に応じて提示を変える画面だからである —
 *   レジストリの `initial` は 1 つの識別子であり、ウィンドウごとに初期画面を選ぶ仕組みは
 *   9.1 に無い。**判定の材料はセッションの状態（`document_state`）であり、生成要求の記録
 *   （`window_document_state`）ではない** — 開いているドキュメントの真実はセッションだから
 *   である（document-session の design.md「既存の関連付け（`window_document_state`）との
 *   関係」。判定の移行はそちらのタスク 4.3 が行った）。**ドキュメントの画面は下流スペックの
 *   持ち物であり、ここでは装わない**（保持しているウィンドウには、名前・未保存・シートの
 *   要約だけを提示する）。1.4 の初期画面は `screens` に残る（消さない。9.7 のスモーク画面も
 *   `screens` へ足すだけで、`initial` の選び方はこの画面が引き続き持つ）。
 *
 * # 遷移機構の確認手順（9.1 で実際に用いた再現手順）
 *
 * 画面が 1 つしか無いコミット状態では遷移は起きない。次の一時的な差し込みで、そのまま
 * 遷移を確認できる（`npm run dev` → `http://localhost:1420`。Tauri のランタイムは不要である）:
 *
 * 1. `ScreenProps` だけを受け取る画面を 2 つ作り、片方の `onClick` で
 *    `navigate("<もう片方の id>")` を呼ぶ。
 * 2. `SHELL_SCREEN_REGISTRY` の `screens` にその 2 つを足し、`initial` を片方の id にする。
 * 3. 遷移の前後で `data-testid="jxcel-shell-region"` の `data-shell-screen` を読む。
 *    値と領域の中身の両方が変わることを確認する。
 * 4. 確認が済んだら 1 と 2 を元に戻す（コミットツリーに検証用の画面を残さない）。
 *    タスク 9.7 のスモーク画面は同じ経路（`screens` に足すだけ）を通る。
 *
 * # 1.4 からの引き継ぎ
 *
 * 初期画面はタスク 1.4 が置いた実体をそのまま画面として登録した（`InitialScreen`）。**まだ
 * 実用画面ではない**（実用水準へ育てるのは下流のスペックであり、10.4 用の画面は 9.7 が足す）。
 * 配色はタスク 9.2 が `./theme` のカスタムプロパティへ移した。
 */
import { useEffect, useMemo, type ReactElement } from "react";

import {
  useShellRouter,
  type ScreenDefinition,
  type ScreenProps,
  type ShellScreenRegistry,
} from "./router";
import { ScreenBoundary } from "./ScreenBoundary";
import {
  APPEARANCE_VARS,
  useAppearance,
  type AppearanceChoice,
} from "./theme";
import { DiagnosticsScreen } from "../features/diagnostics/DiagnosticsScreen";
import {
  DIAGNOSTICS_SCREEN_ID,
  installDiagnosticsRequests,
} from "../features/diagnostics/requests";
import {
  EmptyWindowScreen,
  EMPTY_WINDOW_SCREEN_ID,
} from "../features/empty/EmptyWindowScreen";
import {
  EDITOR_SMOKE_SCREEN_ID,
  EditorSmoke,
} from "../features/smoke/EditorSmoke";
import {
  TABLE_SMOKE_SCREEN_ID,
  TableSmoke,
} from "../features/smoke/TableSmoke";
import { GLIDE_PROBE_SCREEN_ID, GlideProbe } from "../features/smoke/glideProbe";
import { resolveVerificationInitialScreen } from "./verificationScreen";
import { SessionClosePrompt } from "./sessionClose";

/**
 * 1.4 が置いた初期画面（`InitialScreen`）の識別子。**9.6 以降は既定で表示される画面ではない**
 * （既定は `EMPTY_WINDOW_SCREEN_ID` の画面）。それでも登録を残すのは、消すと 1.4 の画面
 * （3 OS の描画確認の最小内容）が到達不能になるためである。
 */
export const INITIAL_SCREEN_ID = "shell.initial";

/**
 * アクセント色（`#c2185b` = `rgb(194, 24, 91)`）。10.4 の画素検査の目印であり、定義は
 * `./theme` にある。**10.4 が同じ名前で参照できるよう、ここからも再輸出する。**
 */
export { INITIAL_SCREEN_ACCENT_COLOR } from "./theme";

/**
 * 1.4 の初期画面。**シェルのクロームを除いた領域の中身**だけを描く（画面の契約 2）。
 *
 * 実用画面ではない。3 OS の描画確認（10.4）が「無内容の白い窓ではない」ことを示せる最小の
 * 内容を持つ。9.7 が 3 OS 描画確認用の画面を足すまで、既定の表示はこれである。配色は
 * シェルが与えるカスタムプロパティ（`./theme`）に従う。
 */
function InitialScreen(): ReactElement {
  return (
    <section
      data-testid="jxcel-initial-screen"
      style={{
        padding: "2rem 3rem",
        borderRadius: "0.5rem",
        backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
        color: `var(${APPEARANCE_VARS.screenText})`,
        textAlign: "center",
      }}
    >
      <h1 style={{ margin: 0, fontSize: "2.5rem" }}>jxcel</h1>
      <p style={{ margin: "0.75rem 0 0", color: `var(${APPEARANCE_VARS.screenMuted})` }}>
        jxcel の初期画面（3 OS の描画確認用）
      </p>
    </section>
  );
}

/**
 * シェルが差し込める画面の一覧。**画面を足すとは、この配列に 1 つ足すことに他ならない。**
 *
 * タスク 9.5 が診断の導線（`src/features/diagnostics/`）の画面を足し、9.6 が空ウィンドウの
 * 操作導線（`src/features/empty/`）の画面を足して `initial` をそこへ移した。**タスク 9.7 は
 * 3 OS 描画確認用の 2 画面（`src/features/smoke/`）をここへ足した** — 実用画面ではなく、
 * 育てるのは `data-grid` と `macro-editor-lsp` である。
 *
 * **`initial` が 1 つの識別子であることは 9.1 の契約である**（ウィンドウごとに初期画面を
 * 選ぶ仕組みは無い）。**ウィンドウの状態に応じて提示を変えるのは 9.6 の画面自身の責務**であり、
 * この配列は「最初にどの画面を領域へ差し込むか」だけを決める。その状態をどこから読むかは
 * 画面が決める — 9.6 の画面は当初 `window_document_state`（生成要求の記録）で関連付けを
 * 判定していたが、**今は `document_state`（セッションの状態）だけを見る**
 * （document-session の design.md「既存の関連付け（`window_document_state`）との関係」。判定の
 * 移行はそちらのタスク 4.3）。
 *
 * **既定の初期画面は 9.6 の空ウィンドウの画面である。**9.7 の 2 画面はユーザー向けの導線を
 * 持たず、**検証用の形**（`JXCEL_VERIFICATION_BUILD=1` のフロントエンドと `verification-triggers`
 * の Rust。`JXCEL_VERIFICATION_INITIAL_SCREEN`）だけが `./verificationScreen` 経由で初期画面
 * として選ぶ。**配布物には `./verificationScreen` が入らない**（`vite.config.ts` の
 * `__JXCEL_VERIFICATION__`）。
 */
export const SHELL_SCREEN_REGISTRY: ShellScreenRegistry = {
  initial: EMPTY_WINDOW_SCREEN_ID,
  screens: [
    {
      id: EMPTY_WINDOW_SCREEN_ID,
      title: "ドキュメント",
      component: EmptyWindowScreen,
    },
    {
      id: INITIAL_SCREEN_ID,
      title: "初期画面",
      component: InitialScreen,
    },
    {
      id: DIAGNOSTICS_SCREEN_ID,
      title: "診断",
      component: DiagnosticsScreen,
    },
    {
      id: TABLE_SMOKE_SCREEN_ID,
      title: "描画確認: 表",
      component: TableSmoke,
    },
    {
      id: EDITOR_SMOKE_SCREEN_ID,
      title: "描画確認: 文字編集",
      component: EditorSmoke,
    },
  ],
};

/**
 * 検証専用: 標本を描く**使い捨ての画面**（`src/features/smoke/glideProbe.tsx`。tasks.md 1.5、
 * 要件 12.1）。
 *
 * **`SHELL_SCREEN_REGISTRY` へは直接足さない。**9.7 のスモーク画面は 10.4 の方式 A（配布物と
 * 検証用の形が**同じ**コードで描く）のために配布物の登録簿にも要るが、こちらは逆であり、
 * **グリッドライブラリを配布物のバンドルへ 1 バイトも入れない**ことが要件である。したがって
 * 本定義は `Layout` の `__JXCEL_VERIFICATION__` の分岐の中だけで参照し、既定のビルド（`false`）
 * では定数畳み込みで参照ごと消える（`scripts/check-shipping-bundle.sh` が機械検査する）。
 */
const GLIDE_PROBE_SCREEN_DEFINITION: ScreenDefinition = {
  id: GLIDE_PROBE_SCREEN_ID,
  title: "描画確認: グリッド（10 万行）",
  component: GlideProbe,
};

/** 外観を選ぶ操作の 1 項目。 */
interface AppearanceOption {
  readonly choice: AppearanceChoice;
  readonly label: string;
  readonly description: string;
  readonly testId: string;
}

/**
 * 外観を選ぶ操作の項目。`system` は「OS の外観設定に追随する」、`light` / `dark` は
 * 「OS の設定より優先する明示選択」である（要件 9.3、9.4）。
 */
const APPEARANCE_OPTIONS: readonly AppearanceOption[] = [
  {
    choice: "system",
    label: "OS",
    description: "OS の外観設定に追随する",
    testId: "jxcel-appearance-system",
  },
  {
    choice: "light",
    label: "明",
    description: "常に明色にする（OS の設定より優先）",
    testId: "jxcel-appearance-light",
  },
  {
    choice: "dark",
    label: "暗",
    description: "常に暗色にする（OS の設定より優先）",
    testId: "jxcel-appearance-dark",
  },
];

/**
 * 外観を選ぶ最小の操作。**シェルのクロームの中に置く**（領域の中へは置かない）。
 *
 * 選択は `./theme` が先に反映し、その後で設定 `appearance.theme` へ保存する。保存に失敗しても
 * この起動の間は選択が保たれるので、操作が無反応になることはない（`./theme` のモジュール doc）。
 */
function AppearanceControl({
  choice,
  setChoice,
}: {
  readonly choice: AppearanceChoice;
  readonly setChoice: (choice: AppearanceChoice) => void;
}): ReactElement {
  return (
    <nav
      data-testid="jxcel-appearance-control"
      aria-label="外観"
      style={{ display: "flex", gap: "0.25rem" }}
    >
      {APPEARANCE_OPTIONS.map((option) => {
        const active = option.choice === choice;
        return (
          <button
            key={option.choice}
            type="button"
            data-testid={option.testId}
            aria-pressed={active}
            title={option.description}
            onClick={() => {
              setChoice(option.choice);
            }}
            style={{
              font: "inherit",
              fontSize: "0.75rem",
              lineHeight: 1.4,
              padding: "0.15rem 0.5rem",
              borderRadius: "0.25rem",
              cursor: "pointer",
              color: active
                ? `var(${APPEARANCE_VARS.controlActiveText})`
                : `var(${APPEARANCE_VARS.shellChromeText})`,
              backgroundColor: active
                ? `var(${APPEARANCE_VARS.controlActiveBackground})`
                : "transparent",
              border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
            }}
          >
            {option.label}
          </button>
        );
      })}
    </nav>
  );
}

/**
 * 個別機能の画面が差し込まれる領域。
 *
 * **画面の描画はここ 1 箇所だけで起きる。** シェル自身のクローム（`Layout` 側）はこの外側に
 * あり、画面はここへ入る。領域が持つ style は「画面に与える枠」であり、画面が自前で作っては
 * ならないレイアウトの一部である。
 */
export interface ShellRegionProps {
  /** 表示する画面（`useShellRouter` が返す現在の定義）。 */
  readonly screen: ScreenDefinition;
  /**
   * エラー時に提示する他の画面（自分自身を除く）。`ScreenBoundary` の回復導線が使う。
   *
   * 失敗した画面自身は遷移の操作を持てない（描画が失敗している）ため、**領域から出る導線は
   * シェル側のエラー提示が持つ**（要件 9.5。`./ScreenBoundary` のモジュール doc「回復」）。
   */
  readonly alternatives: readonly ScreenDefinition[];
  /** 遷移を要求する唯一の入口（`useShellRouter` が返すもの）。 */
  readonly navigate: ScreenProps["navigate"];
}

/**
 * 選択された画面を DOM へ差し込む領域。
 *
 * `flex: 1 1 auto` と `min-height: 0` は、画面の内容が増えてもウィンドウを押し広げず、
 * **領域の中でスクロールさせる**ためのものである（3 OS 描画確認の表形式の画面はこれに依存する）。
 */
export function ShellRegion({
  screen,
  alternatives,
  navigate,
}: ShellRegionProps): ReactElement {
  const Screen = screen.component;
  return (
    <section
      data-testid="jxcel-shell-region"
      // 現在表示されている画面の識別子。**外部の検査はこの属性を見る**（モジュール doc を参照）。
      data-shell-screen={screen.id}
      aria-label={`画面: ${screen.title}`}
      style={{
        flex: "1 1 auto",
        minHeight: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "1.5rem",
        overflow: "auto",
      }}
    >
      {/*
        エラー隔離の境界は**この 1 式だけ**を包む（クロームや領域そのものを包むとアプリ全体が
        止まる。要件 9.5、`./ScreenBoundary` のモジュール doc「配置」）。`key` に画面の
        識別子を与えるのは、**別の画面へ遷移したときに境界を作り直し、失敗状態を持ち越さない**
        ためである（同 doc「回復」）。
      */}
      <ScreenBoundary
        key={screen.id}
        screenId={screen.id}
        title={screen.title}
        alternatives={alternatives}
        navigate={navigate}
      >
        <Screen screenId={screen.id} navigate={navigate} />
      </ScreenBoundary>
    </section>
  );
}

/**
 * シェルの器。クローム（アクセント色のヘッダ帯と外観の操作）と、選択された画面が入る
 * [`ShellRegion`] を組み立てる。
 *
 * 遷移機構は `./router` の `useShellRouter` 1 つだけであり、本コンポーネントはその結果を
 * 領域へ渡すだけである。外観は `./theme` のカスタムプロパティに従い、本コンポーネントが
 * 持つのは選択の表示と入口（[`AppearanceControl`]）だけである。
 */
export function Layout(): ReactElement {
  // 検証専用の初期画面の選択（要件 10.4。`./verificationScreen`）。**`__JXCEL_VERIFICATION__`
  // は Vite の `define` が埋め込むビルド時定数であり、既定のビルド（配布物）では `false` で
  // ある。したがってこの条件式は定数畳み込みで `null` になり、参照されなくなった
  // `./verificationScreen` は配布物のバンドルから落ちる**（`vite.config.ts` のヘッダ。
  // `scripts/check-shipping-bundle.sh` が配布物の `dist/` を機械検査する）。
  //
  // **静的な分岐で書く理由**: この解決は描画時に同期で必要であり、動的 import では
  // 間に合わない。`./verificationScreen` は副作用を持たない（関数と型だけ）ので、Rollup は
  // 未使用になったモジュールを落とせる。
  //
  // 検証用の形でも、指定が無ければ `null` を返すので、この解決は登録簿の内容を変えない —
  // 既定の初期画面は 9.6 の空ウィンドウの画面のままである。マウント時に 1 回だけ読む（値は
  // 文書の他のスクリプトより前に走る初期化スクリプトが載せるので、この時点で確定している）。
  // **検証専用の画面の登録（tasks.md 1.5）もこの分岐の中だけで行う。**`SHELL_SCREEN_REGISTRY`
  // へ直接足さないのは、足すと**グリッドライブラリが配布物のバンドルへ入る**ためである
  // （9.7 のスモーク画面は配布物にも要るので直接足してある。`scripts/check-shipping-bundle.sh`
  // が「`smoke-table` / `smoke-editor` は在ること」「検証専用の識別子は無いこと」の両方を検査する）。
  const registry = useMemo<ShellScreenRegistry>(() => {
    const base: ShellScreenRegistry = __JXCEL_VERIFICATION__
      ? {
          ...SHELL_SCREEN_REGISTRY,
          screens: [
            ...SHELL_SCREEN_REGISTRY.screens,
            GLIDE_PROBE_SCREEN_DEFINITION,
          ],
        }
      : SHELL_SCREEN_REGISTRY;
    const verification = __JXCEL_VERIFICATION__
      ? resolveVerificationInitialScreen(base)
      : null;
    return verification === null
      ? base
      : { ...base, initial: verification };
  }, []);
  const router = useShellRouter(registry);
  const appearance = useAppearance();

  // メニューからの診断の導線（要件 8.1、8.6、8.7。タスク 9.5）。**登録するのは購読だけで
  // あり、遷移は唯一の入口（`router.navigate`）へ委ねる** — 2 つ目の遷移機構を作らない
  // （要件 9.2）。購読はマウント時に 1 回だけ張り、画面が表示されていない間に選ばれた
  // メニュー項目も落とさない（選ばれた区画を示した状態でこの画面へ遷移する）。
  useEffect(
    () => installDiagnosticsRequests(router.navigate),
    [router.navigate],
  );

  // エラー提示の回復導線に出す「他の画面」。**表示中の画面自身は除く**（同じ画面へは
  // 「再試行」で戻る）。画面の集合はモジュール定数なので、現在の識別子だけが入力である。
  const alternatives = useMemo(
    () => registry.screens.filter((screen) => screen.id !== router.current.id),
    [registry, router.current.id],
  );

  return (
    <main
      data-testid="jxcel-shell"
      style={{
        minHeight: "100vh",
        display: "flex",
        flexDirection: "column",
        backgroundColor: `var(${APPEARANCE_VARS.shellSurface})`,
        fontFamily: "system-ui, sans-serif",
        color: `var(${APPEARANCE_VARS.shellText})`,
      }}
    >
      {/*
        シェル自身のクローム。**アプリ名とアクセント色を両方の外観で常に描く**ので、画面が
        何であっても 3 OS の画素検査（10.4）の目印が消えない。外観を選ぶ操作もここに置く
        （領域の中へは置かない。モジュール doc「外観」を参照）。
      */}
      <header
        data-testid="jxcel-shell-chrome"
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          gap: "1rem",
          padding: "0.75rem 1.5rem",
          backgroundColor: `var(${APPEARANCE_VARS.shellChrome})`,
          color: `var(${APPEARANCE_VARS.shellChromeText})`,
        }}
      >
        <span style={{ fontSize: "1.25rem", fontWeight: 700 }}>jxcel</span>
        <span
          style={{
            display: "flex",
            alignItems: "center",
            gap: "1rem",
          }}
        >
          <span
            data-testid="jxcel-shell-screen-title"
            style={{ fontSize: "0.875rem", opacity: 0.85 }}
          >
            {router.current.title}
          </span>
          <AppearanceControl
            choice={appearance.choice}
            setChoice={appearance.setChoice}
          />
        </span>
      </header>

      {/*
        終了前の問い（要件 6.2、タスク 4.2）。**シェルのクロームに 1 点だけ載せる** —
        `ShellRegion` の中には置かない。画面の契約（`ScreenProps` だけを受け、自前の
        レイアウトを持たない）に触れないためであり、提示はどの画面が表示されていても同じ
        位置に出る必要があるためである（design.md「SessionClosePrompt」）。

        提示の中身と配色は `./sessionClose` が持ち、本ファイルは置き場所だけを決める。
        配色は `var(--jxcel-…)`（`./theme`）に従う。
      */}
      <SessionClosePrompt />

      <ShellRegion
        screen={router.current}
        alternatives={alternatives}
        navigate={router.navigate}
      />
    </main>
  );
}

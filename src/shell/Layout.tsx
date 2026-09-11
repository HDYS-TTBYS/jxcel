/**
 * シェルのレイアウト — 個別機能の画面が差し込まれる領域（`ShellRegion`）と、その外側の
 * シェル自身のクロームを定義する器。
 *
 * 所有: `ShellLayout`（design.md「Components and Interfaces → Frontend Layer」、
 * 「Directory Structure」の `src/shell/Layout.tsx`）。
 * 要件: 1.1, 1.2（3 OS で初期画面が描画されること）, 9.1（画面が差し込まれる領域の定義）。
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
 * 4. **自前で配色を決めない。** 明暗の外観はシェルが持つ（タスク 9.2）。9.2 はこのファイルの
 *    style を theme の解決結果へ差し替え、`InitialScreen` の `INITIAL_SCREEN_PANEL_COLOR` も
 *    theme 側へ移す。画面はシェルが与える配色に従う。
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
 * **アクセント色 `INITIAL_SCREEN_ACCENT_COLOR`（`#c2185b` = `rgb(194, 24, 91)`）はシェルの
 * クロームの一部として常に描かれる。** 具体的には (1) `<main data-testid="jxcel-shell">` の背景、
 * (2) `data-testid="jxcel-shell-chrome"` のヘッダ帯（アプリ名 `jxcel` を白字で出す。帯の高さは
 * 約 0.75rem + 文字なので上端 10 px 付近は必ずアクセント色である）。画面が領域を自前の色で
 * 覆っても、このヘッダ帯と領域の余白にアクセント色が残る。したがって 10.4 は
 * **画像中に `rgb(194, 24, 91)` が存在すること**、またはヘッダ帯の画素で確認できる
 * （1.4 の Implementation Notes の目印をそのまま保っている）。
 *
 * # 後続タスクがここへ差し込む場所
 *
 * - **9.2（外観）**: このファイルの `<main>` / ヘッダの style の色を `./theme` の解決結果へ
 *   差し替える。`INITIAL_SCREEN_PANEL_COLOR` も theme 側へ移す。画面は 1 で述べたとおり、
 *   自前の配色を持たない。
 * - **9.3（画面単位のエラー隔離）**: `ShellRegion` の中の `<Screen … />` 1 式を
 *   `./ScreenBoundary` で包む。**境界の粒度は画面 1 つ**（`screen.id` を鍵に与える）。
 *   シェルのクロームと他の領域は包まない。
 * - **9.7（3 OS 描画確認の最小画面）**: `src/features/smoke/` の 2 画面を `ScreenDefinition`
 *   として `SHELL_SCREEN_REGISTRY` に足す。**遷移機構には触れない**（足すだけで表示できる）。
 *   描画の通知（`./renderHeartbeat`）も再利用し、2 つ目の発信側を足さないこと。
 * - **9.4 / 9.5 / 9.6**: このファイルに領域を増やさない。配信先中立の資産は `src/shared/` に、
 *   診断の導線と空ウィンドウの操作は画面として登録する。
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
 */
import type { ReactElement } from "react";

import {
  useShellRouter,
  type ScreenDefinition,
  type ScreenProps,
  type ShellScreenRegistry,
} from "./router";

/** 初期画面の識別色。白背景・暗背景のどちらとも一致しないことを画素検査の根拠にする。 */
export const INITIAL_SCREEN_ACCENT_COLOR = "#c2185b";

/** 中央の面の背景色。アクセント色と対比させる。 */
export const INITIAL_SCREEN_PANEL_COLOR = "#ffffff";

/** 既定で表示される画面（1.4 の初期画面）の識別子。 */
export const INITIAL_SCREEN_ID = "shell.initial";

/**
 * 1.4 の初期画面。**シェルのクロームを除いた領域の中身**だけを描く（画面の契約 2）。
 *
 * 実用画面ではない。3 OS の描画確認（10.4）が「無内容の白い窓ではない」ことを示せる最小の
 * 内容を持つ。9.7 が 3 OS 描画確認用の画面を足すまで、既定の表示はこれである。
 */
function InitialScreen(): ReactElement {
  return (
    <section
      data-testid="jxcel-initial-screen"
      style={{
        padding: "2rem 3rem",
        borderRadius: "0.5rem",
        backgroundColor: INITIAL_SCREEN_PANEL_COLOR,
        color: "#212121",
        textAlign: "center",
      }}
    >
      <h1 style={{ margin: 0, fontSize: "2.5rem" }}>jxcel</h1>
      <p style={{ margin: "0.75rem 0 0" }}>jxcel の初期画面（3 OS の描画確認用）</p>
    </section>
  );
}

/**
 * シェルが差し込める画面の一覧。**画面を足すとは、この配列に 1 つ足すことに他ならない。**
 *
 * タスク 9.7 が `src/features/smoke/` の 2 画面をここへ足す（`initial` は変えない）。
 */
export const SHELL_SCREEN_REGISTRY: ShellScreenRegistry = {
  initial: INITIAL_SCREEN_ID,
  screens: [
    {
      id: INITIAL_SCREEN_ID,
      title: "初期画面",
      component: InitialScreen,
    },
  ],
};

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
  /** 遷移を要求する唯一の入口（`useShellRouter` が返すもの）。 */
  readonly navigate: ScreenProps["navigate"];
}

/**
 * 選択された画面を DOM へ差し込む領域。
 *
 * `flex: 1 1 auto` と `min-height: 0` は、画面の内容が増えてもウィンドウを押し広げず、
 * **領域の中でスクロールさせる**ためのものである（3 OS 描画確認の表形式の画面はこれに依存する）。
 */
export function ShellRegion({ screen, navigate }: ShellRegionProps): ReactElement {
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
        タスク 9.3 はこの 1 式を `<ScreenBoundary screenId={screen.id}>` で包む。境界を領域の
        外へ出さないこと（クロームまで巻き込むとアプリ全体が止まる。要件 9.5）。
      */}
      <Screen screenId={screen.id} navigate={navigate} />
    </section>
  );
}

/**
 * シェルの器。クローム（アクセント色の背景とヘッダ帯）と、選択された画面が入る
 * [`ShellRegion`] を組み立てる。
 *
 * 遷移機構は `./router` の `useShellRouter` 1 つだけであり、本コンポーネントはその結果を
 * 領域へ渡すだけである。
 */
export function Layout(): ReactElement {
  const router = useShellRouter(SHELL_SCREEN_REGISTRY);

  return (
    <main
      data-testid="jxcel-shell"
      style={{
        minHeight: "100vh",
        display: "flex",
        flexDirection: "column",
        backgroundColor: INITIAL_SCREEN_ACCENT_COLOR,
        fontFamily: "system-ui, sans-serif",
        color: "#212121",
      }}
    >
      {/*
        シェル自身のクローム。**アプリ名とアクセント色を常に描く**ので、画面が何であっても
        3 OS の画素検査（10.4）の目印が消えない。タスク 9.2 はここの色を theme の解決結果へ
        差し替える。
      */}
      <header
        data-testid="jxcel-shell-chrome"
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          gap: "1rem",
          padding: "0.75rem 1.5rem",
          backgroundColor: INITIAL_SCREEN_ACCENT_COLOR,
          color: "#ffffff",
        }}
      >
        <span style={{ fontSize: "1.25rem", fontWeight: 700 }}>jxcel</span>
        <span
          data-testid="jxcel-shell-screen-title"
          style={{ fontSize: "0.875rem", opacity: 0.85 }}
        >
          {router.current.title}
        </span>
      </header>

      <ShellRegion screen={router.current} navigate={router.navigate} />
    </main>
  );
}

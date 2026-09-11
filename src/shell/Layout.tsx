/**
 * シェルのレイアウト — 個別機能の画面が差し込まれる領域を定義する器。
 *
 * 所有: `ShellLayout`（design.md「Components and Interfaces → Frontend Layer」）。
 * 要件: 1.1, 1.2（3 OS で初期画面が描画されること）, 9.1（画面が差し込まれる領域の定義）。
 *
 * 本ファイルはタスク 1.4 が置いた最小の実体である。**初期画面が実際に描画されることを
 * 確かめられる範囲**に留め、実用画面を持たない（要件 10.4 のスモーク画面はタスク 9.7 が
 * `features/smoke/` に置く）。次のタスクがここを育てる:
 *
 * - タスク 9.1: 画面の差し込み口（`ShellRegion`）と遷移の結線。実際の画面はここへ入る。
 * - タスク 9.2: 外観（明色・暗色と OS 追随）の適用。現在は色を直接指定している。
 * - タスク 9.3: 画面単位のエラー隔離（`ScreenBoundary`）の配置。
 *
 * 描画確認のための約束: 初期画面は背景色 `INITIAL_SCREEN_ACCENT_COLOR` でウィンドウを
 * 覆い、中央に白い面でアプリ名を出す。既定の WebKit / GTK の白背景・暗背景と衝突しない
 * 色であることを、画素による確認（`xshot`）と DOM の計算済みスタイルの確認の両方で使う。
 */
import type { ReactElement } from "react";

/** 初期画面の識別色。白背景・暗背景のどちらとも一致しないことを画素検査の根拠にする。 */
export const INITIAL_SCREEN_ACCENT_COLOR = "#c2185b";

/** 中央の面の背景色。アクセント色と対比させる。 */
export const INITIAL_SCREEN_PANEL_COLOR = "#ffffff";

/**
 * シェルの器。現時点では初期画面のみを描画する。
 *
 * タスク 9.1 以降、`children` あるいは画面レジストリから選ばれた画面を差し込む形へ育つ。
 */
export function Layout(): ReactElement {
  return (
    <main
      data-testid="jxcel-shell"
      style={{
        minHeight: "100vh",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        backgroundColor: INITIAL_SCREEN_ACCENT_COLOR,
        fontFamily: "system-ui, sans-serif",
      }}
    >
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
        <p style={{ margin: "0.75rem 0 0" }}>
          jxcel の初期画面（3 OS の描画確認用）
        </p>
      </section>
    </main>
  );
}

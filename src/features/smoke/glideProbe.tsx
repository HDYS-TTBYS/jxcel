/**
 * 検証専用: グリッドライブラリ（`@glideapps/glide-data-grid`）が **React 19 上で描画される**
 * ことと、**10 万行分の縦の広がり**を持つことの確認に使う**使い捨ての画面**（tasks.md 1.5、
 * 要件 12.1）。
 *
 * 所有: 検証専用の初期画面の経路（`src/shell/verificationScreen.ts` と
 * `src/shell/Layout.tsx` のレジストリ）。
 *
 * # 実用の画面ではない（使い捨てである）
 *
 * これは 7.2 が移植口（`src/features/grid/renderer/port.ts`）の背後へ置く描画層の**下見**で
 * あり、育てる対象ではない。**本ファイルと `src/shell/Layout.tsx` の登録は、1.6 の実測が
 * 済んだ時点で捨てる前提である**（7.2 は移植口と `glideAdapter.tsx` を実装し、この画面を
 * 引き継がない）。したがって次の形にしてある。
 *
 *   - 標本は**ライブラリ側の面（`./glideProbeGrid`）が自分で組み立てる**（行番号と列番号だけの
 *     決定的な関数）。1.4 の標本の生成器は **Rust 側にあり、フロントエンドから到達できない**
 *     （IPC も足さない）。
 *   - 配色は**ライブラリの既定のテーマ**を使う。器の 10 本のカスタムプロパティを canvas の
 *     描画色へ写すのは 8.1・7.2 の仕事であり、ここへ先取りしない（本ファイルが
 *     `APPEARANCE_VARS` を参照するのは、**この画面自身の見出しと実測値の表示**だけである）。
 *   - 分類・選択・編集・IPC を持たない。**描画されることと広がりを見せることだけ**を持つ。
 *
 * # 出荷物に到達経路を作らない（この画面の一番の勘所）
 *
 * 本モジュール**自身はグリッドライブラリを import しない**（重いのは `./glideProbeGrid` 側）。
 * その上で二重に括ってある。
 *
 *   1. `src/shell/Layout.tsx` のレジストリへの登録を Vite の `define`
 *      （`vite.config.ts` の `__JXCEL_VERIFICATION__`）で括る。既定のビルド（`false`）では
 *      登録が定数畳み込みで消え、**本モジュールは参照されなくなる**。
 *   2. ライブラリ側の面を**動的 import** で読み、しかもその式自体を `__JXCEL_VERIFICATION__`
 *      で括る（下の [`loadGridSurface`]）。
 *
 * 2 が要るのは、**1 だけでは足りないと実測で分かった**ためである。登録を括っただけの形では、
 * 本モジュールが静的に import していた CSS（`@glideapps/glide-data-grid/dist/index.css`。
 * 副作用だけの import なので木から落ちない）が配布物へ残り、**既定のビルドの `dist/` に
 * 8.5 kB の `.gdg-*` / `.dvn-*` の規則を持つ CSS 資産が現れた**。動的 import の式ごと
 * 到達不能にすると、Rollup は**その塊の生成そのものを取りやめる**（`src/main.tsx` の
 * `verificationBulk` が同じ理由で同じ形になっている。`vite.config.ts` のヘッダも参照）。
 *
 * `scripts/check-shipping-bundle.sh` が配布物の `dist/` を機械検査する。検証用の形は
 * `JXCEL_VERIFICATION_BUILD=1` でビルドする。
 *
 * # 経路
 *
 * 検証ビルドを `JXCEL_VERIFICATION_INITIAL_SCREEN=smoke-glide-probe` 付きで起動すると、Rust が
 * ウィンドウの初期化スクリプトでグローバルを載せ、`resolveVerificationInitialScreen` が
 * **登録済みの識別子と一致したときだけ**この画面を初期画面にする（詳細は
 * `src/shell/verificationScreen.ts` のモジュール doc）。
 *
 * # 10 万行分の縦の広がりの確かめ方
 *
 * DOM に出る実測値を読めるようにしてある（数え方の推測を要さない）。
 *
 *   1. **仮想スクロールの内容の高さ** — ライブラリのスクロール要素（`.dvn-scroller`）の
 *      `scrollHeight`。期待値は `見出しの高さ + 行数 × 行の高さ` であり、
 *      `data-probe-expected-scroll-height` に併記する（10 万行 × 34 + 36 = 3,400,036）。
 *      **描画が成立していなければ 0 のままである。**
 *   2. **最末尾の行までの到達** — 「末尾の行へ移動」で `scrollTop` が上端 0 から
 *      `scrollHeight - clientHeight` まで動き、`data-probe-scroll-top` と可視の行の範囲
 *      （`data-probe-first-visible-row` / `data-probe-last-visible-row`）が追随する。
 *      広がりが絵だけでなく**走査可能**であることの証拠である。
 *
 * 補足: ライブラリはアクセシビリティの表（`table[role="grid"]`）も描くが、こちらは
 * `useDebouncedMemo`（200 ms）越しに作られるため、**この画面の実測値には使わない**
 * （描画直後に読むと間に合わない）。広がりの証拠は上の 2 つで足りる。
 */
import {
  useCallback,
  useEffect,
  useState,
  type ComponentType,
  type ReactElement,
} from "react";

import { APPEARANCE_VARS } from "../../shell/theme";
// **型だけの import である**（`verbatimModuleSyntax` により `import type` が要る）。型は
// バンドル時に消えるので、ライブラリ側の面への依存をここへ作らない。
import type { GlideProbeExtent, GlideProbeGridProps } from "./glideProbeGrid";

/**
 * 画面の識別子。`src/shell/Layout.tsx` のレジストリと、検証専用の初期画面の指定
 * （`src/shell/verificationScreen.ts`）が同じ綴りを使うための単一の定義である。
 */
export const GLIDE_PROBE_SCREEN_ID = "smoke-glide-probe";

/** 標本の行数。**10 万行分の縦の広がりを作る値そのもの**である（tasks.md 1.5）。 */
export const GLIDE_PROBE_ROWS = 100_000;

/** 標本の列数。1.6 が走査する形（10 万行 × 30 列）に合わせてある。 */
export const GLIDE_PROBE_COLUMNS = 30;

/** 行の高さ（ピクセル）。**期待する広がりを計算できるよう、既定値に任せず明示する。** */
export const GLIDE_PROBE_ROW_HEIGHT = 34;

/** 列見出しの高さ（ピクセル）。こちらも明示する（期待値の計算に要る）。 */
export const GLIDE_PROBE_HEADER_HEIGHT = 36;

/**
 * 期待する仮想スクロールの内容の高さ（ピクセル）。**実測値と突き合わせる目標値**であり、
 * 描画の判断には使わない（`10 万行 × 34 + 36 = 3,400,036`）。
 */
export const GLIDE_PROBE_EXPECTED_SCROLL_HEIGHT =
  GLIDE_PROBE_HEADER_HEIGHT + GLIDE_PROBE_ROWS * GLIDE_PROBE_ROW_HEIGHT;

/**
 * グリッドの表示の高さ（ピクセル）。**領域の高さに依存させない**（`100%` は親の確定した高さを
 * 要し、解決に失敗すると 0 になって何も描かれない）。ここだけは数値を与えて確実に描く。
 */
const GRID_HEIGHT_PX = 520;

/**
 * ライブラリ側の面を読み込む式。**`__JXCEL_VERIFICATION__` で括るのが要点である**
 * （モジュール doc「出荷物に到達経路を作らない」の 2）。既定のビルドでは `false` に置換され、
 * この定数は `null` になる — 動的 import の式ごと到達不能になり、**塊の生成そのものが
 * 取りやめられる**。
 *
 * **静的 import では要件を満たせない**（本式が動的 import である理由）。静的 import は
 * 到達不能な分岐の中にあってもモジュールグラフへ引き込まれ、`./glideProbeGrid` が持つ
 * 副作用（`@glideapps/glide-data-grid/dist/index.css` の取り込み）は木から落ちない。
 * 実際に測った: 静的 import の形では既定のビルドの `dist/` に 8.5 kB の `.gdg-*` / `.dvn-*`
 * の規則を持つ CSS 資産が現れた。配布物から確実に落ちるのは**到達不能な動的 import の塊**
 * だけである（`src/main.tsx` の `verificationBulk` も同じ理由で同じ形になっている）。
 */
const loadGridSurface: (() => Promise<ComponentType<GlideProbeGridProps>>) | null =
  __JXCEL_VERIFICATION__
    ? async () => (await import("./glideProbeGrid")).GlideProbeGrid
    : null;

/** 画面の枠。**領域いっぱいに広がる**（領域は中央寄せなので、自前で伸ばさないと縦に潰れる）。 */
const ROOT_STYLE = {
  alignSelf: "stretch",
  width: "100%",
  minHeight: 0,
  display: "flex",
  flexDirection: "column",
  gap: "0.75rem",
} as const;

/** 見出しの見た目。 */
const HEADING_STYLE = { margin: 0, fontSize: "1.125rem" } as const;

/** 補足の見た目。 */
const NOTE_STYLE = {
  margin: 0,
  fontSize: "0.8125rem",
  color: `var(${APPEARANCE_VARS.screenMuted})`,
} as const;

/** 実測値の表示。**等幅**にして、値の変化を読み取りやすくする。 */
const STATUS_STYLE = {
  margin: 0,
  fontFamily: "monospace",
  fontSize: "0.8125rem",
  whiteSpace: "pre-wrap",
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** まだ面が読めていないときの表示。 */
const WAITING = "(未報告)";

/**
 * 実測値を 1 つの文字列にする。**入力に対して純粋**であり、描画の判断には使わない。
 */
function describeExtent(extent: GlideProbeExtent | null): string {
  const visible =
    extent === null
      ? WAITING
      : `${String(extent.firstVisibleRow)} 〜 ${String(extent.lastVisibleRow)}`;
  return (
    `行数=${String(GLIDE_PROBE_ROWS)} 列数=${String(GLIDE_PROBE_COLUMNS)}\n` +
    `仮想スクロールの内容の高さ=${String(extent?.scrollHeight ?? 0)} ピクセル` +
    `（期待値 ${String(GLIDE_PROBE_EXPECTED_SCROLL_HEIGHT)}` +
    ` = 見出し ${String(GLIDE_PROBE_HEADER_HEIGHT)}` +
    ` + ${String(GLIDE_PROBE_ROWS)} 行 × ${String(GLIDE_PROBE_ROW_HEIGHT)}）\n` +
    `見えている高さ=${String(extent?.clientHeight ?? 0)}` +
    ` 縦位置=${String(extent?.scrollTop ?? 0)} ピクセル` +
    `（下端=${String(extent?.scrollTop ?? 0)}+${String(extent?.clientHeight ?? 0)}）\n` +
    `可視の行=${visible}`
  );
}

/**
 * 検証専用のグリッド確認画面。**`ScreenProps` 以外の props を受け取らない**（画面の契約。
 * `src/shell/Layout.tsx`）。`screenId` と `navigate` はこの画面では使わない（画面を切り替える
 * 導線を持たない）。
 */
export function GlideProbe(): ReactElement {
  const [surface, setSurface] = useState<ComponentType<GlideProbeGridProps> | null>(
    null,
  );
  const [extent, setExtent] = useState<GlideProbeExtent | null>(null);

  // ライブラリ側の面を 1 回だけ読む。**マウント後に読むのは、動的 import を最初の描画の
  // 経路から外すためである**（レジストリの登録と初期画面の解決は同期のままにする。
  // `src/shell/renderHeartbeat.ts` の通知は領域の `data-shell-screen` を読むので、
  // 面の到着を待たない）。
  useEffect(() => {
    if (loadGridSurface === null) {
      return;
    }
    let cancelled = false;
    void loadGridSurface()
      .then((loaded) => {
        if (!cancelled) {
          // **関数を値として渡す**ので、更新関数の形にする（`setState` は関数を更新関数として
          // 解釈する）。
          setSurface(() => loaded);
        }
      })
      .catch((error: unknown) => {
        console.error("グリッドの面を読み込めなかった", error);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const onExtent = useCallback((measured: GlideProbeExtent) => {
    setExtent(measured);
  }, []);

  // **大文字の別名へ移す**（小文字のまま JSX で使うと、HTML の要素名として解釈される）。
  const Surface = surface;

  return (
    <section
      data-testid="jxcel-smoke-glide-probe"
      // 宣言した形。**実 DOM と突き合わせる**ためのもので、描画の判断には使わない。
      data-probe-rows={String(GLIDE_PROBE_ROWS)}
      data-probe-columns={String(GLIDE_PROBE_COLUMNS)}
      data-probe-row-height={String(GLIDE_PROBE_ROW_HEIGHT)}
      data-probe-header-height={String(GLIDE_PROBE_HEADER_HEIGHT)}
      data-probe-expected-scroll-height={String(
        GLIDE_PROBE_EXPECTED_SCROLL_HEIGHT,
      )}
      // 実測値。**描画が成立していなければ 0 のままである。**
      data-probe-scroll-height={String(extent?.scrollHeight ?? 0)}
      data-probe-client-height={String(extent?.clientHeight ?? 0)}
      data-probe-scroll-top={String(extent?.scrollTop ?? 0)}
      data-probe-first-visible-row={
        extent === null ? "" : String(extent.firstVisibleRow)
      }
      data-probe-last-visible-row={
        extent === null ? "" : String(extent.lastVisibleRow)
      }
      style={ROOT_STYLE}
    >
      <header>
        <h2 data-testid="jxcel-smoke-glide-probe-heading" style={HEADING_STYLE}>
          描画確認: グリッド（{GLIDE_PROBE_ROWS.toLocaleString("en-US")} 行 ×{" "}
          {GLIDE_PROBE_COLUMNS} 列）
        </h2>
        <p style={NOTE_STYLE}>
          Glide Data Grid が React 19
          上で描画されることと、10万行分の縦の広がりを持つことを確かめるための使い捨ての画面です。実用の画面ではありません。
        </p>
      </header>
      {Surface === null ? (
        <p data-testid="jxcel-smoke-glide-probe-waiting" style={NOTE_STYLE}>
          グリッドの面を読み込み中です。
        </p>
      ) : (
        <Surface
          rows={GLIDE_PROBE_ROWS}
          columns={GLIDE_PROBE_COLUMNS}
          rowHeight={GLIDE_PROBE_ROW_HEIGHT}
          headerHeight={GLIDE_PROBE_HEADER_HEIGHT}
          heightPx={GRID_HEIGHT_PX}
          onExtent={onExtent}
        />
      )}
      <p data-testid="jxcel-smoke-glide-probe-status" style={STATUS_STYLE}>
        {describeExtent(extent)}
      </p>
    </section>
  );
}

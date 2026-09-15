/**
 * 検証専用: 描画層の移植口の実装（`GlideAdapter`）を**実物の上で駆動した結果**を表示する
 * 使い捨ての画面（tasks.md 7.2。要件 1.2, 1.3, 2.4, 7.1, 7.2）。
 *
 * 所有: 検証専用の初期画面の経路（`src/shell/verificationScreen.ts` と `src/shell/Layout.tsx` の
 * レジストリの `__JXCEL_VERIFICATION__` の分岐）。
 *
 * # 実用の画面ではない（使い捨てである）
 *
 * これは 7.2 の受け入れ（10 万行の走査・選択の区別・列幅と列の位置の操作）が**実物の上で**
 * 成立することを観測するための面であり、育てる対象ではない。**9.2 / 9.3 が恒久の観測を入れた
 * 時点で、この画面と段（`scripts/check-port-interaction.sh`）は取り除く** — 1.6 の
 * `glideProbe` と `scripts/check-render-traversal.sh` と同じ性質の一時的な段である。
 *
 * # 出荷物に到達経路を作らない（1.6 の画面と同じ勘所）
 *
 * 本モジュール**自身は移植口の実装を import しない**（`./portProbeAdapter` が
 * `../grid/renderer/glideAdapter` を静的に読み、その先でライブラリと CSS が入る）。そのうえで
 * 二重に括ってある。
 *
 *   1. `src/shell/Layout.tsx` のレジストリへの登録を Vite の `define`
 *      （`vite.config.ts` の `__JXCEL_VERIFICATION__`）で括る。既定のビルド（`false`）では登録が
 *      定数畳み込みで消え、**本モジュールは参照されなくなる**。
 *   2. 面を**動的 import** で読み、しかもその式自体を `__JXCEL_VERIFICATION__` で括る。
 *      1.5 の実測（静的 import の CSS は木から落ちない）どおり、**到達不能な動的 import の塊**
 *      だけが配布物から確実に落ちる。
 *
 * なお移植口の実装（`glideAdapter.tsx`）は**配布物に入ってよい**（それが製品の描画層である）。
 * `scripts/check-shipping-bundle.sh` が禁じているのは検証専用の識別子だけである。
 *
 * # 観測の行の読み方
 *
 * 駆動の結果は**1 行の `[検証]` 行**として `aria-label` に置く（1.6 と同じ経路: アプリの診断
 * 記録へはフロントエンドから書けないので、アクセシビリティの木から読む）。読む側は
 * `scripts/check-port-interaction.sh` である。
 */
import { useCallback, useEffect, useState, type ComponentType, type ReactElement } from "react";

import { APPEARANCE_VARS } from "../../shell/theme";
// **型だけの import である**（`verbatimModuleSyntax` により `import type` が要る）。
import type { PortProbeSurfaceProps } from "./portProbeAdapter";
import { describePortProbe, pendingPortProbeLine, type PortProbeFacts } from "./portProbeFacts";

/**
 * 画面の識別子。`src/shell/Layout.tsx` のレジストリと、検証専用の初期画面の指定
 * （`src/shell/verificationScreen.ts`）が同じ綴りを使うための単一の定義である。
 */
export const PORT_PROBE_SCREEN_ID = "smoke-port-probe";

/**
 * 面を読み込む式。**`__JXCEL_VERIFICATION__` で括るのが要点である**（モジュール doc）。
 * 既定のビルドでは `false` に置換され、この定数は `null` になる — 動的 import の式ごと
 * 到達不能になり、**塊の生成そのものが取りやめられる**。
 */
const loadProbeSurface: (() => Promise<ComponentType<PortProbeSurfaceProps>>) | null =
  __JXCEL_VERIFICATION__
    ? async () => (await import("./portProbeAdapter")).PortProbeSurface
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

/** 観測の行の見た目。**等幅**にして、読む側と画面が同じ文字列を見るようにする。 */
const STATUS_STYLE = {
  margin: 0,
  fontFamily: "monospace",
  fontSize: "0.8125rem",
  whiteSpace: "pre-wrap",
  wordBreak: "break-all",
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/**
 * 検証専用の移植口の確認画面。**`ScreenProps` 以外の props を受け取らない**（画面の契約。
 * `src/shell/Layout.tsx`）。`screenId` と `navigate` はこの画面では使わない。
 */
export function PortProbe(): ReactElement {
  const [surface, setSurface] = useState<ComponentType<PortProbeSurfaceProps> | null>(null);
  const [facts, setFacts] = useState<PortProbeFacts | null>(null);

  // 面を 1 回だけ読む。**マウント後に読むのは、動的 import を最初の描画の経路から外すためで
  // ある**（登録と初期画面の解決は同期のままにする。1.6 の画面と同じ判断）。
  useEffect(() => {
    if (loadProbeSurface === null) {
      return;
    }
    let cancelled = false;
    void loadProbeSurface()
      .then((loaded) => {
        if (!cancelled) {
          // **関数を値として渡す**ので、更新関数の形にする。
          setSurface(() => loaded);
        }
      })
      .catch((error: unknown) => {
        console.error("移植口の確認の面を読み込めなかった", error);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const onFacts = useCallback((observed: PortProbeFacts) => {
    setFacts(observed);
  }, []);

  const line = facts === null ? pendingPortProbeLine() : describePortProbe(facts);
  // **大文字の別名へ移す**（小文字のまま JSX で使うと、HTML の要素名として解釈される）。
  const Surface = surface;

  return (
    <section
      data-testid="jxcel-smoke-port-probe"
      // 宣言した形。**実 DOM と突き合わせる**ためのもので、判定には使わない。
      data-probe-rows={String(100_000)}
      data-probe-columns={String(30)}
      style={ROOT_STYLE}
    >
      <header>
        <h2 data-testid="jxcel-smoke-port-probe-heading" style={HEADING_STYLE}>
          描画確認: 移植口の操作（Glide の写し）
        </h2>
        <p style={NOTE_STYLE}>
          移植口（RendererPort）の Glide の実装を実際に駆動し、10万行の走査・選択の区別・列幅と
          列の位置の操作が成立することを確かめるための使い捨ての画面です。実用の画面ではありません。
        </p>
      </header>
      {Surface === null ? (
        <p data-testid="jxcel-smoke-port-probe-waiting" style={NOTE_STYLE}>
          移植口の面を読み込み中です。
        </p>
      ) : (
        <Surface onFacts={onFacts} />
      )}
      {/*
        観測の行。**表示だけでなくアクセシビリティの名前としても出す**のが要点である
        （フロントエンドからアプリの診断記録へは書けない。モジュール doc を参照）。
        `aria-label` は AT-SPI の名前として読めるので、`scripts/check-port-interaction.sh` が
        この 1 行を読んで判定する。**表示は同じ文字列のままにする**（読み手と画面が食い違わない）。
      */}
      <p
        data-testid="jxcel-smoke-port-probe-report"
        aria-label={line}
        style={STATUS_STYLE}
      >
        {line}
      </p>
    </section>
  );
}

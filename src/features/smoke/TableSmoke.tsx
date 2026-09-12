/**
 * 3 OS の描画確認に使う、多数の要素を持つ表形式の**最小画面**。
 *
 * 所有: `SmokeScreens`（design.md「Components and Interfaces → Frontend Layer」）。
 * 要件: 10.4（多数の要素を持つ表形式の描画と、文字編集を伴う描画のそれぞれについて、
 * 3 つの OS で成立することを確認できる最小の画面を提供すること）。
 *
 * **実用画面ではない。**実用水準へ育てるのは `data-grid` スペックであり、本ファイルは
 * 3 OS の描画確認（10.4）が成立するのに必要な最小の内容だけを持つ。したがって並べ替え・
 * 絞り込み・選択・セル編集のような操作は**意図的に持たない**（偽の機能を作らない。
 * 9.6 がドキュメント所有機能を作らなかったのと同じ判断）。
 *
 * # なぜ 200 行 × 8 列 = 1,600 セルなのか
 *
 * この画面の目的は「描画が成立したこと」を外から識別できるようにすることである。したがって
 * 要素数は**無内容のウィンドウと桁で違う**必要がある。内訳は `td` 1,600（データセル）＋
 * 行見出し `th` 200 ＋ 列見出し `th` 8 ＋ 左上の見出し `th` 1 ＋ `tr` 201 ＋
 * 表の構造 3（`table` / `thead` / `tbody`）＝ **表だけで 2,013 要素**（実測。領域全体では
 * 2,018 要素）であり、シェルのクローム（20 要素未満）や空ウィンドウの画面（5 要素。実測）とは
 * 要素数で区別できる。加えて 200 行はウィンドウの既定の高さ（800 論理ピクセル）より縦に長い
 * （実測: 表の高さ 5,226 ピクセル）ので、**1 画面に収まらない行の連続した描画**が確かめられる
 * — 1 行だけの表では「描けたがほぼ空」を識別できない。表は画面の中でスクロールする
 * （下の `VIEWPORT_STYLE` の上限）ので、窓を押し広げずに最初の画面の描画が測定できる。
 *
 * # 描画成立の通知（10.1、10.2）はこのファイルから送らない
 *
 * 通知の送信側は **8.2 の `src/shell/renderHeartbeat.ts` 1 本だけ**であり、`src/main.tsx` が
 * React のマウント直前に 1 回だけ仕掛ける。**この画面は 2 本目を足さない** — 同じウィンドウから
 * 2 つの通知が届くと、先着だけが判定を確定し、どちらが先かは環境依存になる
 * （`renderHeartbeat.ts` のモジュール doc「9.7 との分担」）。
 *
 * 通知は**画面ごとではなくウィンドウごと・起動ごとに 1 回**、そのウィンドウが最初に表示した
 * 画面の描画フレームから出る。10.4 はこの画面を**初期画面として**起動する
 * （`JXCEL_VERIFICATION_INITIAL_SCREEN=smoke-table`。経路は `src/shell/verificationScreen.ts`）
 * ので、この画面の描画そのものが通知の対象になる。
 *
 * # 配色
 *
 * シェルが与える `var(--jxcel-*)` だけを参照する（画面の契約 4。`src/shell/Layout.tsx` の
 * 「画面の契約」）。本ファイルは色の値を持たないので、明暗の外観に自動的に追随する。
 */
import type { ReactElement } from "react";

import { APPEARANCE_VARS } from "../../shell/theme";

/**
 * 画面の識別子。`src/shell/Layout.tsx` のレジストリと、検証専用の初期画面の指定
 * （`src/shell/verificationScreen.ts`）が同じ綴りを使うための単一の定義である。
 */
export const TABLE_SMOKE_SCREEN_ID = "smoke-table";

/** データ行の数。 */
export const SMOKE_TABLE_ROWS = 200;

/** データ列の数（先頭の行見出し列は数えない）。 */
export const SMOKE_TABLE_COLUMNS = 8;

/**
 * データセル（`td`）の総数。**宣言した数と実際の DOM を突き合わせるために公開する**
 * （10.4 とレビューが `document.querySelectorAll` で数えられる）。
 */
export const SMOKE_TABLE_CELLS = SMOKE_TABLE_ROWS * SMOKE_TABLE_COLUMNS;

/** 行番号（0 起点）。モジュール定数なので描画のたびに作り直さない。 */
const ROW_INDICES: readonly number[] = Array.from(
  { length: SMOKE_TABLE_ROWS },
  (_, row) => row,
);

/** 列番号（0 起点）。 */
const COLUMN_INDICES: readonly number[] = Array.from(
  { length: SMOKE_TABLE_COLUMNS },
  (_, column) => column,
);

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

/**
 * 表を入れる枠。**ここがスクロールする。**
 *
 * `maxHeight` を与えるのは、表（実測 5,226 ピクセル）がウィンドウより縦に長く、上限が無いと
 * シェル全体（`min-height: 100vh` の `<main>`）が内容に合わせて伸びてしまうためである
 * （伸びると「画面 1 つ分の描画」を測定できず、10.4 の画素検査の対象もぼやける）。
 * `vh` は窓に対する相対なので、3 OS のどの窓の大きさでも画面の中に収まる。
 */
const VIEWPORT_STYLE = {
  flex: "1 1 auto",
  minHeight: 0,
  maxHeight: "60vh",
  overflow: "auto",
  border: `1px solid var(${APPEARANCE_VARS.screenMuted})`,
  borderRadius: "0.375rem",
  backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
} as const;

/** 表そのもの。**列幅を均等に固定する**ので、内容による幅の揺れで横へはみ出さない。 */
const TABLE_STYLE = {
  width: "100%",
  borderCollapse: "collapse",
  tableLayout: "fixed",
  fontSize: "0.75rem",
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 見出しセル（列見出しと行見出し）。 */
const HEADER_CELL_STYLE = {
  position: "sticky",
  top: 0,
  padding: "0.25rem 0.5rem",
  textAlign: "left",
  borderBottom: `1px solid var(${APPEARANCE_VARS.screenMuted})`,
  backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
  color: `var(${APPEARANCE_VARS.screenMuted})`,
  fontWeight: 600,
} as const;

/** 行見出しセル。列見出しの粘着と重ならないよう、上端の粘着は与えない。 */
const ROW_HEADER_STYLE = {
  padding: "0.25rem 0.5rem",
  textAlign: "right",
  borderRight: `1px solid var(${APPEARANCE_VARS.screenMuted})`,
  color: `var(${APPEARANCE_VARS.screenMuted})`,
  fontWeight: 600,
} as const;

/** データセル。 */
const CELL_STYLE = {
  padding: "0.25rem 0.5rem",
  borderBottom: `1px solid var(${APPEARANCE_VARS.screenMuted})`,
  borderRight: `1px solid var(${APPEARANCE_VARS.screenMuted})`,
  textAlign: "right",
} as const;

/**
 * 表形式の最小画面。**`ScreenProps` 以外の props を受け取らない**（画面の契約。
 * `src/shell/Layout.tsx`）。
 *
 * 状態を持たない。描画は入力に対して純粋であり、**最初の描画で表の全体が DOM に載る**
 * （通知が対象にする描画フレームを、あとから届くデータで遅らせない）。
 */
export function TableSmoke(): ReactElement {
  return (
    <section
      data-testid="jxcel-smoke-table"
      // 宣言した形。**実 DOM の要素数と突き合わせる**ためのもので、描画の判断には使わない。
      data-smoke-rows={String(SMOKE_TABLE_ROWS)}
      data-smoke-columns={String(SMOKE_TABLE_COLUMNS)}
      data-smoke-cells={String(SMOKE_TABLE_CELLS)}
      style={ROOT_STYLE}
    >
      <header>
        <h2 data-testid="jxcel-smoke-table-heading" style={HEADING_STYLE}>
          描画確認: 表形式（{SMOKE_TABLE_ROWS} 行 × {SMOKE_TABLE_COLUMNS} 列 /{" "}
          {SMOKE_TABLE_CELLS} セル）
        </h2>
        <p style={NOTE_STYLE}>
          多数の要素を持つ表が 3 OS で描画されることを確認するための最小画面です。実用の表ではありません。
        </p>
      </header>
      <div style={VIEWPORT_STYLE}>
        <table data-testid="jxcel-smoke-table-grid" style={TABLE_STYLE}>
          <thead>
            <tr>
              <th scope="col" style={{ ...HEADER_CELL_STYLE, textAlign: "right" }}>
                行
              </th>
              {COLUMN_INDICES.map((column) => (
                <th key={column} scope="col" style={HEADER_CELL_STYLE}>
                  列 {column + 1}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {ROW_INDICES.map((row) => (
              <tr key={row}>
                <th scope="row" style={ROW_HEADER_STYLE}>
                  {row + 1}
                </th>
                {COLUMN_INDICES.map((column) => (
                  <td key={column} style={CELL_STYLE}>
                    {row * SMOKE_TABLE_COLUMNS + column + 1}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

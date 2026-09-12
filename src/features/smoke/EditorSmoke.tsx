/**
 * 3 OS の描画確認に使う、文字編集を伴う描画の**最小画面**。
 *
 * 所有: `SmokeScreens`（design.md「Components and Interfaces → Frontend Layer」）。
 * 要件: 10.4（多数の要素を持つ表形式の描画と、文字編集を伴う描画のそれぞれについて、
 * 3 つの OS で成立することを確認できる最小の画面を提供すること）。
 *
 * **実用画面ではない。**実用水準へ育てるのは `macro-editor-lsp` スペックであり、本ファイルは
 * 3 OS の描画確認（10.4）が成立するのに必要な最小の内容だけを持つ。したがって編集支援・
 * 保存・取り消し・言語機能のような**実用の機能は意図的に持たない**（偽の機能を作らない）。
 *
 * # 編集の面は何か
 *
 * **素の `<textarea>` である。**エディタの依存は足さない（design.md「Allowed Dependencies」に
 * エディタの実装は無く、育てるのは `macro-editor-lsp`）。`textarea` はネイティブの文字入力の
 * 面であり、**テキストの組版・カーソル・選択・スクロール**という「文字編集を伴う描画」の
 * 要素を 3 OS それぞれの実装で通す。内容は **64 行**（日本語と ASCII の混在）で、行が
 * 面の高さを超えるので**面の中のスクロールと組版**も同時に確かめられる — 1 行では
 * 「描けたがほぼ空」を識別できない。
 *
 * 編集が起きたことは面の下の状態表示に現れる（`data-edited` と文字数・行数）。10.4 は
 * 実アプリで面を選んで文字を入力し、この表示が変わることで**入力と再描画の両方**を確認できる。
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
 * （`JXCEL_VERIFICATION_INITIAL_SCREEN=smoke-editor`。経路は `src/shell/verificationScreen.ts`）
 * ので、この画面の描画そのものが通知の対象になる。
 *
 * # 配色
 *
 * シェルが与える `var(--jxcel-*)` だけを参照する（画面の契約 4。`src/shell/Layout.tsx` の
 * 「画面の契約」）。本ファイルは色の値を持たないので、明暗の外観に自動的に追随する。
 */
import { useState, type ReactElement } from "react";

import { APPEARANCE_VARS } from "../../shell/theme";

/**
 * 画面の識別子。`src/shell/Layout.tsx` のレジストリと、検証専用の初期画面の指定
 * （`src/shell/verificationScreen.ts`）が同じ綴りを使うための単一の定義である。
 */
export const EDITOR_SMOKE_SCREEN_ID = "smoke-editor";

/** 初期の行数。 */
export const SMOKE_EDITOR_LINES = 64;

/**
 * 初期の内容。**決定的に組み立てる**（乱数も時刻も使わないので、3 OS で同じ文字列になる）。
 *
 * 各行に日本語と ASCII を混ぜてある。文字の組版（フォントの代替・行の高さ）が 3 OS で
 * 成立することは「文字編集を伴う描画」の確認そのものである。
 */
const INITIAL_TEXT: string = Array.from(
  { length: SMOKE_EDITOR_LINES },
  (_, index) =>
    `${String(index + 1).padStart(2, "0")}: jxcel の描画確認用テキスト — 文字編集が 3 OS で成立することを確かめる行です。`,
).join("\n");

/** 初期の文字数（編集されたかどうかの判断に使う）。 */
export const SMOKE_EDITOR_INITIAL_LENGTH = INITIAL_TEXT.length;

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

/** 編集の面。**ネイティブの文字入力を受ける唯一の要素**である。 */
const TEXTAREA_STYLE = {
  flex: "1 1 auto",
  minHeight: 0,
  width: "100%",
  resize: "none",
  padding: "0.75rem",
  border: `1px solid var(${APPEARANCE_VARS.screenMuted})`,
  borderRadius: "0.375rem",
  backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
  fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
  fontSize: "0.8125rem",
  lineHeight: 1.5,
  tabSize: 2,
} as const;

/** 状態表示の見た目。 */
const STATUS_STYLE = {
  margin: 0,
  fontSize: "0.8125rem",
  color: `var(${APPEARANCE_VARS.screenMuted})`,
} as const;

/**
 * 編集の状態を 1 行で述べる。**入力のたびに描き直される**ので、これが変わることが
 * 「入力を受け取り、描画し直した」ことの証拠になる（`data-edited` も同じ判断を運ぶ）。
 */
function describeEditing(text: string, edited: boolean): string {
  const lineCount = text === "" ? 0 : text.split("\n").length;
  const state = edited ? "編集済み" : "未編集";
  return `${state}: ${lineCount} 行 / ${text.length} 文字（初期 ${SMOKE_EDITOR_LINES} 行 / ${SMOKE_EDITOR_INITIAL_LENGTH} 文字）`;
}

/**
 * 文字編集を伴う描画の最小画面。**`ScreenProps` 以外の props を受け取らない**（画面の契約。
 * `src/shell/Layout.tsx`）。
 */
export function EditorSmoke(): ReactElement {
  const [text, setText] = useState(INITIAL_TEXT);
  const edited = text !== INITIAL_TEXT;

  return (
    <section data-testid="jxcel-smoke-editor-root" style={ROOT_STYLE}>
      <header>
        <h2 data-testid="jxcel-smoke-editor-heading" style={HEADING_STYLE}>
          描画確認: 文字編集（textarea / 初期 {SMOKE_EDITOR_LINES} 行）
        </h2>
        <p style={NOTE_STYLE}>
          文字編集を伴う描画が 3 OS で成立することを確認するための最小画面です。実用のエディタではありません。
        </p>
      </header>
      <textarea
        data-testid="jxcel-smoke-editor"
        // 編集が起きたかどうかを外から 1 式で読めるようにする（10.4 の実アプリ確認）。
        data-edited={edited ? "true" : "false"}
        aria-label="描画確認用のテキスト"
        spellCheck={false}
        value={text}
        onChange={(event) => {
          setText(event.target.value);
        }}
        style={TEXTAREA_STYLE}
      />
      <p data-testid="jxcel-smoke-editor-status" data-edited={edited ? "true" : "false"} style={STATUS_STYLE}>
        {describeEditing(text, edited)}
      </p>
    </section>
  );
}

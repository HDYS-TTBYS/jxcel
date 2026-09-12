/**
 * 検証専用: 起動時に表示する画面の選択 — 3 OS の描画確認（10.4）が、実用画面ではない
 * 最小画面（`src/features/smoke/`）を直接開くための**唯一の経路**。
 *
 * 所有: シェルの起動経路（`src/shell/Layout.tsx` のレジストリの初期画面の解決）。
 * 要件: 10.4（3 OS それぞれで最小画面が描画されること）。
 *
 * # 経路（Rust とフロントエンドの対の契約）
 *
 * 1. 検証ビルド（`--features verification-triggers`）を環境変数
 *    `JXCEL_VERIFICATION_INITIAL_SCREEN=<画面の識別子>` 付きで起動する。
 * 2. Rust（`src-tauri/src/window/mod.rs` の `initial_screen_script`）がその値を**ウィンドウの
 *    初期化スクリプト**として書き、`window.__JXCEL_VERIFICATION_INITIAL_SCREEN__` に載せる。
 *    **Webview はプロセスの環境変数を読めない**ため、この 1 手だけが環境変数を画面へ運ぶ方法で
 *    ある（コマンドを増やさない — 通信境界 `src/ipc/bindings.ts` は閉じている）。
 *    初期化スクリプトは「グローバルが作られた後・文書が解析される前・文書の他のスクリプトより
 *    前」に走るので、フロントエンドのバンドルが読む時点で値は必ず載っている。
 * 3. 本モジュールがマウント時にそのグローバルを読み、**登録済みの画面の識別子と一致したとき
 *    だけ**初期画面として使う。
 *
 * **グローバルの綴りと環境変数の綴りは Rust 側と対であり、片方だけ変えてはならない。**
 *
 * # 既定のビルドと既定の起動は変わらない
 *
 * `verification-triggers` は非既定の cargo feature であり、**既定のビルドには環境変数の読み取り
 * も初期化スクリプトも入らない**（配布物に検証専用の入口を残さない。`src-tauri/Cargo.toml`）。
 * したがって既定のビルドでは本モジュールは常に `null` を返し、初期画面は
 * `SHELL_SCREEN_REGISTRY.initial`（9.6 の空ウィンドウの画面）のままである。値が未知の識別子の
 * ときも同じ（警告して既定へ落ちる）。**9.6 の受け入れ（ドキュメントを関連付けていない
 * ウィンドウの導線）は退行しない。**
 *
 * # 通知との関係（10.1、10.2）
 *
 * 描画成立の通知の送信側は `src/shell/renderHeartbeat.ts` 1 本であり、**画面ごとではなく
 * ウィンドウごと・起動ごとに 1 回**（マウント直後の描画フレーム）送られる。通知は「その
 * ウィンドウが最初に表示した画面が描画された」ことの証拠であり、画面を切り替えても 2 回目は
 * 送られない（送ると 2 本目の発信側になる）。したがって「**両方の**最小画面が描画された」ことは
 * **1 画面につき 1 回の起動**（合わせて 2 回の起動）で確認する。10.4 はその 2 回をこの経路で
 * 行う。
 */
import type { ScreenId, ShellScreenRegistry } from "./router";

/**
 * 検証専用の初期画面を載せるグローバルの名前。
 *
 * **`src-tauri/src/window/mod.rs` の `VERIFY_INITIAL_SCREEN_GLOBAL` と同じ綴りでなければ
 * ならない。**名前の定義を両側に 1 つずつ置くのは、これがビルド時にも実行時にも共有できない
 * **検証専用の対の契約**だからである（既定のビルドには Rust 側の定義が存在しない）。
 */
const VERIFICATION_INITIAL_SCREEN_GLOBAL =
  "__JXCEL_VERIFICATION_INITIAL_SCREEN__" as const;

declare global {
  interface Window {
    /**
     * 検証専用: 検証ビルドの初期化スクリプトが載せる画面の識別子。**既定のビルドでは
     * 決して設定されない**（`undefined`）。値は文字列であることを実行時に検査する。
     */
    readonly __JXCEL_VERIFICATION_INITIAL_SCREEN__?: unknown;
  }
}

/**
 * 検証専用に指定された初期画面を解決する。**指定が無い・未知の識別子・文字列でない**の
 * いずれでも `null` を返し、呼び出し元は既定の初期画面を使う。
 *
 * 登録済みの識別子と一致することを確かめるのは、**グローバルが何であっても画面の集合の外へは
 * 出られない**ようにするためである（シェルのレジストリが唯一の正本であり、ここに 2 つ目の
 * 一覧を作らない）。未知の値は黙って捨てず、1 行だけ警告する（10.4 の取り違えを気付けるように）。
 */
export function resolveVerificationInitialScreen(
  registry: ShellScreenRegistry,
): ScreenId | null {
  const requested = window[VERIFICATION_INITIAL_SCREEN_GLOBAL];
  if (requested === undefined || requested === null) {
    return null;
  }
  if (typeof requested !== "string" || requested === "") {
    console.warn(
      `${VERIFICATION_INITIAL_SCREEN_GLOBAL} が画面の識別子ではない。既定の初期画面で起動する`,
      requested,
    );
    return null;
  }
  if (!registry.screens.some((screen) => screen.id === requested)) {
    console.warn(
      `${VERIFICATION_INITIAL_SCREEN_GLOBAL} の画面が登録されていない。既定の初期画面で起動する: "${requested}"`,
    );
    return null;
  }
  return requested;
}

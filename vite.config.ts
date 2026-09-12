// Vite の構成。ルートを `src/` に置き、配布物の出力先だけをリポジトリ直下の `dist/` へ出す。
// `build.outDir` は `root` 相対であるため `../dist` と書く（`src/dist` にしない）。
// この `dist/` を `src-tauri/tauri.conf.json` の `build.frontendDist` が指す。
//
// # 検証専用コードを配布物から除外する（tasks.md 5.4 / 8.2 の決定のフロントエンド側）
//
// tasks.md は「配布物（既定のビルド）に検証専用のコードを入れない」と決めている（5.4 / 8.2）。
// Rust 側はそれを非既定の cargo feature `verification-triggers` で果たしているが、**cargo の
// feature は TypeScript を括れない**。したがって `src/shell/verificationScreen.ts` と
// `src/shell/verificationBulk.ts`（およびその呼び出し元）を、**ビルド時の定数**で括る。
//
//   - 既定のビルド（`npm run build` / `npx tauri build`）: `__JXCEL_VERIFICATION__` は `false`。
//     `src/main.tsx` と `src/shell/Layout.tsx` の分岐が定数畳み込みで消え、参照されなくなった
//     両モジュールはバンドルから落ちる。**配布物の `dist/`（および Tauri が埋め込む資産）には
//     `__JXCEL_VERIFICATION_*` のような検証専用の識別子・グローバル名が 1 つも残らない。**
//   - 検証用の形（10.4 / 10.5 / 10.7 / 10.8 が使う）: ビルド時の環境変数
//     `JXCEL_VERIFICATION_BUILD=1` で有効にする。書き方は
//     `JXCEL_VERIFICATION_BUILD=1 npx tauri build --no-bundle --features verification-triggers`。
//     **tauri CLI のコマンド自体は変えずに済む** — `beforeBuildCommand`（`npm run build`）は
//     環境変数を継承するので、`vite build` がこの値をそのまま読む。
//
// **この環境変数は本ファイル（Node 側）だけが読む。** `src/` のソースには現れないので、
// 配布物のバンドルへ文字列として入る経路が無い（検査器 `scripts/check-shipping-bundle.sh` が
// 配布物の `dist/` に検証専用の識別子が無いことを機械検査する）。
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Vite の構成は Node 上で読まれるため `process` が実在するが、tsconfig.json の `types` は
// `vite/client` に限られており `@types/node` を導入していない（新しい依存を足さない）。
// 本ファイルが必要とする範囲だけをここで宣言する。
declare const process: { env: Record<string, string | undefined> };

/** 検証用の形を要求するビルド時の環境変数（`1` のときだけ有効）。 */
const VERIFICATION_BUILD_ENV = "JXCEL_VERIFICATION_BUILD";

export default defineConfig(({ mode }) => {
  // `mode`（`vite build --mode …`）も受け付ける。CI の検証用の段は環境変数で駆動するが、
  // ローカルで `npx vite build --mode verification` を使う経路も同じ結果になるようにしておく。
  const verificationBuild =
    process.env[VERIFICATION_BUILD_ENV] === "1" || mode === "verification";

  return {
    root: "src",
    plugins: [react()],
    // Tauri の開発時に Vite が端末を消さないようにする（既定の true のままだと CLI の出力が消える）。
    clearScreen: false,
    // フロントエンドのソースが参照するビルド時のスイッチ。`define` は識別子を**リテラルへ
    // 置換する**ので、`false` のときの分岐は Rollup の定数畳み込みで消える（上のヘッダを参照）。
    define: {
      __JXCEL_VERIFICATION__: JSON.stringify(verificationBuild),
    },
    server: {
      // Tauri の `build.devUrl` と一致させる。`strictPort` により、別のプロセスが使用中なら
      // 黙って別ポートへ移らずに失敗する（`devUrl` が指す先と食い違わないようにするため）。
      port: 1420,
      strictPort: true,
    },
    build: {
      outDir: "../dist",
      emptyOutDir: true,
    },
  };
});

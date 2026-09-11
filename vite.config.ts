// Vite の構成。ルートを `src/` に置き、配布物の出力先だけをリポジトリ直下の `dist/` へ出す。
// `build.outDir` は `root` 相対であるため `../dist` と書く（`src/dist` にしない）。
// この `dist/` を `src-tauri/tauri.conf.json` の `build.frontendDist` が指す。
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  root: "src",
  plugins: [react()],
  // Tauri の開発時に Vite が端末を消さないようにする（既定の true のままだと CLI の出力が消える）。
  clearScreen: false,
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
});

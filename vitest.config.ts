// フロントエンドのテストの構成（tasks.md 7.1。導入の理由と版の選び方は package.json の
// `//devDependencies` にある）。
//
// # なぜ独立の構成ファイルなのか（`vite.config.ts` を読み込まない理由）
//
// `vitest.config.ts` があると vitest は `vite.config.ts` を読まない。本課題の面
// （`src/features/grid/renderer/`）は**純粋な論理**であり、アプリ側の構成が持つ 3 つ
// （`root: "src"`・react プラグイン・`__JXCEL_VERIFICATION__` の `define`）のいずれも要らない。
// `root` を持ち込むと探索の起点が `src/` に移って下の `include` と二重になり、`define` を
// 持ち込むと検証専用の分岐がテストの側へ漏れる。よって合流（`mergeConfig`）はしない。
//
// **申し送り（7.2 / 群 8 が読むこと）**: 検証用のスイッチ（`__JXCEL_VERIFICATION__`）を参照する
// モジュールや React の部品（`.tsx`）をテストが読み込むようになったら、この判断を見直すこと。
// そのときは `vite.config.ts` を合流させる（react プラグインが要る）。**黙って環境が変わらない**
// よう、ここに明記しておく（`scripts/check-shipping-bundle.sh` が配布物から検証専用の
// 識別子を締め出しているので、テストの側の構成が配布物へ混ざる経路は無い）。
//
// **7.2 の結論: 環境は `node` のままである（`jsdom` を足していない）。** 7.2 の実物
// （`src/features/grid/renderer/glideAdapter.tsx`）は 2 層に分かれている。移植口の意味論
// （仕様 → Glide の props、Glide の通知 → 移植口の callback、選択の所有）は **DOM を持たない
// 配線**（`createGlideWiring`）であり、`node` 環境でそのまま検査できる（`glideAdapter.test.ts`。
// `.tsx` を読み込むが、react プラグインを要する JSX はテストの側に無い — esbuild が tsconfig の
// `jsx: "react-jsx"` で `.tsx` を変換し、CSS の取り込みは vitest が空の module にする）。
// 実物の canvas を要するのは `DataEditor` を描く面（`GlideSurface`）だけで、**そちらは実物を
// 起動して観測する**（`src/features/smoke/portProbe*` と `scripts/check-port-interaction.sh`）。
// **`jsdom` は足さない** — 模した DOM には canvas が無く、見え方の主張（10 万行の走査・選択の
// 区別・列幅と列の位置の操作）を何も裏付けられないためである。ライブラリ自体の取り込みは
// `node` でも成功する（実測: `import("@glideapps/glide-data-grid")` が 58 の輸出を返す）。
//
// # 環境が `node` であること
//
// 描画層の移植口（`./port`）が扱うのは座標・文字・列の見出しだけであり、DOM を触らない。
// 移植口が受け取る器（`HTMLElement`）は、偽の実装が触れないことを**確かめる**代役
// （`interactionDriver.ts` の `standInContainer`）として渡す。したがって `jsdom` も
// `happy-dom` も要らない（新しい依存を足さない）。
// **7.2 の Glide の実装は実物の canvas を要する**ので、環境はそこで選び直す。
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // 器（DOM）を持たない走らせ方。理由は上のヘッダ。
    environment: "node",
    // 探索範囲を `src/` に限る。既定の `**/*.{test,spec}.*` はリポジトリ全体を走査するため、
    // 将来 `scripts/` や仕様の側に同名のファイルが現れると黙って巻き込む。
    //
    // **`tsx` を落とさないこと。** `.ts` だけを挙げると、**画面のテストが黙って飛ばされる** —
    // vitest は発見できなかったファイルを「失敗」ではなく「対象外」として扱うため、群 8 の
    // 画面（`.tsx`）のテストを足しても走査の対象数が増えず、緑のまま何も検査しない状態になる
    // （7.1 のレビューが実測: `.test.tsx` に意図的な失敗を置いても 9 passed のままだった）。
    include: ["src/**/*.test.{ts,tsx}"],
  },
});

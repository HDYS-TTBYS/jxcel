// ESLint の flat 構成。
//
// 機械検査としての目的は 1 つに絞る: **明示的な `any` を許さないこと**（タスク 1.4、要件 4.2）。
// ts-rs の生成物 src/ipc/bindings.ts も対象から外さない。生成物に `any` は現れないため、
// 除外すると検査の意味が失われる（design.md「Technology Stack」）。
//
// 骨組みのファイル（doc コメントのみ）は合法である。未使用の変数・引数は
// tsconfig.json の noUnusedLocals / noUnusedParameters と本ファイルの規則の両方で塞ぐが、
// 宣言が無いファイルはどちらにも触れない。
import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    // フロントエンド以外は ESLint の検査対象にしない（Rust・仕様・検証環境・ビルド生成物）。
    // `eslint .` がリポジトリ全体を走査しても、対象は src/ と設定ファイルだけになる。
    ignores: [
      "dist/**",
      "node_modules/**",
      "src-tauri/**",
      ".devsys/**",
      ".kiro/**",
      "crates/**",
      "scripts/**",
      "target/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    rules: {
      // `any` の禁止は規約ではなく機械検査である。エラーとして `npm run lint` を落とす。
      "@typescript-eslint/no-explicit-any": "error",
    },
  },
);

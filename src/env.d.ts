/**
 * Vite の `define` が埋め込むビルド時定数の宣言（`vite.config.ts`）。
 *
 * `__JXCEL_VERIFICATION__` は **検証専用の形（`JXCEL_VERIFICATION_BUILD=1` のビルド）でだけ
 * `true`** になる。既定のビルド（配布物）では `false` であり、この定数を条件にした分岐は
 * Rollup の定数畳み込みで消える — それにより `src/shell/verificationScreen.ts` と
 * `src/shell/verificationBulk.ts` が配布物のバンドルから落ちる（tasks.md 5.4 / 8.2 の決定を
 * フロントエンドでも成立させる。`vite.config.ts` のヘッダを参照）。
 *
 * **この定数はビルド時に置換されるので、実行時に参照されることはない**（バンドル後の
 * ソースに `__JXCEL_VERIFICATION__` という識別子は残らない）。
 */
declare const __JXCEL_VERIFICATION__: boolean;

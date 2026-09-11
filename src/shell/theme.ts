/**
 * 明色・暗色の外観と OS の外観設定への追随。
 *
 * 所有: `ShellLayout` の外観部分（design.md「Components and Interfaces → Frontend Layer」）。
 * 要件: 9.3（明色と暗色の提供と既定での OS 追随）, 9.4（明示選択の優先と再起動後の維持）。
 *
 * 本ファイルはタスク 1.4 が置いた骨組みである。**振る舞いを持たない。**実体を埋めるのは
 * タスク 9.2 であり、そのとき `prefers-color-scheme` の参照、利用者の明示選択、設定ストア
 * （IPC 境界の向こうにある）への永続化をここへ実装する。
 *
 * 実装上の制約（design.md より）: 既定は OS に追随し、**明示選択があればそれを優先**する。
 * 適用先は `Layout.tsx` のシェル自身のクローム（`<main data-testid="jxcel-shell">` の背景と
 * `data-testid="jxcel-shell-chrome"` のヘッダ帯。現在は `INITIAL_SCREEN_ACCENT_COLOR` を
 * 直接指定している）。個別機能の画面は自前の配色を持たず、シェルが与える配色に従う。
 */
export {};

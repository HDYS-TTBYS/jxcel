# `src/shared/` — 配信先中立なフロントエンド資産

このディレクトリには、**Tauri の通信境界に依存してはならない**コードを置く。

## 何を置く場所か

フォームレンダラのように、**デスクトップアプリの中と LAN 配信の Web ページの両方**で
動く必要がある資産の置き場である（design.md「Directory Structure」の `SharedAssets`、
structure.md「配信先中立なフロントエンド資産」）。逆に、デスクトップ専用の資産
（`src/shell/`・`src/ipc/`・画面）はここへ置かない。ここは**どこへ配信しても動く**ことが
要件であり、それが `SharedAssets` がレイヤー図でどこからも依存されない孤立した位置に
ある理由である（design.md「Layering」）。

現時点の中身は**この README だけ**である。フォーム配信そのものは後続スペック
（`form-web-server`）が担うため、app-shell の段では置き場と検査だけを用意する
（design.md の木も `README.md` のみを示す）。置き場の内側に共通モジュールを足すのは、
実際に配信される資産を持つスペックの責務である。

## 制約

ここに置くコードは、次を参照してはならない（要件 9.6。design.md「SharedAssets」の
「`IpcClient` を含め、Tauri の通信境界に依存してはならない」）。

- `@tauri-apps/api`（および `@tauri-apps/plugin-*` などの同パッケージ群）— サブパス・
  型のみの import・`require`・動的 `import()` を含む
- アプリ自身の IPC モジュール `src/ipc/bindings.ts` / `src/ipc/client.ts`（および
  そのディレクトリ形 `src/ipc/`）
- Tauri が注入する実行時グローバル `window.__TAURI__` / `window.__TAURI_INTERNALS__` /
  `window.__TAURI_IPC__` と、`@tauri-apps/api` が内部で使う `transformCallback`

いずれも**指定子や名前を変数・文字列に保持した場合も同じ**である。たとえば
`const spec = "../ipc/client"; import(spec);` は、import 文に直接書いた場合と同じ逸脱として
検出される（検査は値ではなく構文木の literal を照合する）。

`src/shared/` 配下の TypeScript / JavaScript は、`tsconfig.json` の
`include: ["src"]` と ESLint の対象範囲（`eslint.config.js` の `ignores` は
`scripts/**` などを除くだけで `src/shared/**` を含む）に**そのまま入る**。
したがって置き場のコードは `npm run typecheck` / `npm run lint`（`any` の禁止を含む）を
自動的に通り、別の設定を足す必要は無い。

## なぜ破ってはならないか

この制約が破られると `form-web-server` スペックが成立しない（design.md「SharedAssets」）。
LAN 配信される画面には Tauri のランタイムが存在せず、通信境界への参照は読み込み時点で
失敗するか、無言で機能しなくなる。「デスクトップでは動くが配信すると壊れる」という形の
破損は、配信機能を実装するまで表面化しない。だから目視ではなく機械検査で止める。

## 機械検査

`scripts/check-shared-assets.sh`（tasks.md 9.4）が置き場を走査する。使い方:

```
sh scripts/check-shared-assets.sh          # 既定の置き場 src/shared を検査
sh scripts/check-shared-assets.sh <パス>   # 置き場を明示する
```

リポジトリルートで実行する（構文解析に devDependency の `typescript` を使うため
`npm ci` 済みであることが前提）。

- 終了コード: **0 = 依存なし / 1 = 依存を検出（ファイル・行・列を列挙）/ 2 = 入力が使えない**
  （置き場の不在・node 不在・typescript 不在・読み込み失敗・構文解析の失敗）。
- 検査対象は `.ts .tsx .mts .cts .js .jsx .mjs .cjs` のみ。**Markdown を含めない**ので、
  この README が制約の語（`@tauri-apps` など）を述べても逸脱にならない。
- 生のテキストを正規表現で走査せず、**TypeScript の構文木**として解釈したうえで、
  **構文木に現れるすべての文字列・テンプレート literal と識別子**を照合する
  （7.3 の `check-capabilities.sh` と同じ方針）。`import` 文などの**指定子も literal の
  一種にすぎず、位置で規則を分けない**ので、指定子を変数や文字列に保持した迂回も
  同じ照合を受ける（例: `const spec = "../ipc/client"; import(spec)` は逸脱）。コードの
  コメント内の言及は構文木に現れないため逸脱にならない。
- 相対指定子（`./` / `../` で始まる literal）は、変数に保持されていてもその literal の
  位置から実際に解決し、置き場の兄弟 `../ipc/{client,bindings}` または IPC モジュール群の
  ディレクトリ `../ipc` を指す場合だけ逸脱とする。置き場の内側の同名モジュール
  （例 `src/shared/ipc/client.ts`）は誤検出しない。
- **検出できない形**: どの literal にも境界の形が単体で現れない断片からの組み立て
  （例 `import(base + "/ipc/" + "client")`。検査は**定数伝播をしない**）、
  `eval` / `new Function` が文字列から組み立てる指定子・グローバル名、実行時に連結した
  グローバル名（`globalThis["__TAURI" + "_INTERNALS__"]`）。詳細と限界はスクリプトの
  ヘッダに明記してある。

CI への組み込みはタスク 10.1 が行う。エントリは
`sh scripts/check-shared-assets.sh`（npm スクリプト `npm run check:shared-assets`）である。

# Roadmap

## Overview
JSON をファイル実体とする、データベースとして運用可能なスプレッドシート型 GUI アプリケーション。File → Sheet → データスキーマ（それぞれ 1 対 N）という構造を持ち、ネスト可能なスキーマと DB 的なデータ型（ANY を含む）で型付けされたデータを扱う。ファイルの実体は zip 圧縮された JSON テキストであり、これにより git による意味のあるバージョン管理が既定で機能する — xlsx がバイナリであるがゆえに失われていた「いつ・誰が・どのセルを変えたか」を取り戻すことが本プロジェクトの中心的な価値になる。

設計上の最大の判断は「関数はマクロに統合する」ことにある。閉じた式言語を持たず、すべての計算を JS/TS マクロとして表現し、その上に高機能な標準マクロライブラリと LSP 付きのエディタを載せる。これにより、Excel 関数の限界に突き当たった時点で別言語へ逃げる必要がなくなる。さらに、コンポーネントを配置して作った入力フォームをアプリ内蔵の HTTP サーバで LAN 配信し、スキーマを知らない人からも型の正しいデータを集められる。

## Approach Decision
- **Chosen**: Rust + Tauri v2。Rust バックエンドに `deno_core`（V8）をマクロランタイムとして埋め込み、`git2-rs`（libgit2）でバージョン管理を行い、`axum` でフォーム配信する。UI は webview 上の Web フロントエンドとし、グリッドは canvas ベースの仮想化グリッド、マクロエディタは Monaco + LSP。Windows / macOS は OS 標準 WebView に乗る真の単一 exe、Linux は WebKitGTK を同梱した AppImage で自己完結させる（WebKitGTK だけで約 76MB、言語サーバを含めると 150〜200MB の見込み）。
- **Why**: 要件のうち「LSP 付き高機能マクロエディタ」と「10 万行級グリッド」の 2 つは本質的に Web 技術の領域にある。この 2 つを要件に置いた時点で、ネイティブ GUI 案はエディタとグリッドの自作コストが他の全機能を圧迫する。Tauri v2 は単一実行ファイル要件を満たしつつ Monaco と canvas グリッドをそのまま使える唯一の現実解であり、Rust エコシステムは git（libgit2）と xlsx の両方で成熟した選択肢を持つ。
- **Rejected alternatives**:
  - **Rust ネイティブ GUI（egui / GPUI / Slint）**: 5〜15MB の完全静的バイナリという唯一の優位を持つが、Monaco も CodeMirror も動かないため tree-sitter ベースのエディタと LSP UI の自作が必要になる。しかも LSP サーバはどのみち別プロセスになるため「完全静的」の利点は部分的にしか得られない。
  - **Go + Wails**: ビルドとクロスコンパイルは最も楽だが、`go-git` の 3-way マージが libgit2 に明確に劣り（バージョン管理が中心価値である本プロジェクトでは致命的）、`goja` は TypeScript 非対応。
  - **Electron / CEF**: 単一実行ファイル要件を満たさない（ディレクトリ構成、100〜200MB）。
  - **Qt**: 静的リンクした proprietary 単一バイナリには商用ライセンスが必須。技術的には QTableView が最も成熟した仮想化グリッドだが、ライセンスが要件と直接衝突する。
  - **Servo / Ultralight**: Servo は embeddable API が 2026-04 公開でまだ若すぎる。Ultralight はクローズドソース部分と収益制限を持つ。

## Scope
- **In**: zip + JSON のドキュメント形式、ネスト可能な型システム（ANY と JS によるユーザー定義型を含む）、10 万行級の仮想化グリッド編集、スキーマ編集 UI、JS/TS マクロランタイムと標準マクロライブラリ、マクロに統合された数式と自動再計算、LSP 付きマクロエディタ、内蔵 git によるバージョン管理、xlsx / docx テンプレートへの連続書き出し、入力フォームのビルダーと LAN 内蔵サーバ配信、Windows / macOS / Linux の単一実行ファイル配布
- **Out**: 10 万行を超える規模のための遅延ロード・ページング・インデックス（全件オンメモリ前提）、複数人の同時編集、git のリモート連携と 3-way マージ（初版）、インターネットへのフォーム公開とクラウドホスティング、認証基盤とユーザー管理、PDF 出力、自動更新と署名配布

## Constraints
- **データ規模**: 1 ファイルあたり約 10 万行、全件オンメモリ。この単純化が storage 層のスペックを大幅に軽くしている
- **対象ユーザー**: 個人・パワーユーザー。マクロのサンドボックスは「敵対的コードに対する硬い境界」ではなく「事故防止」の水準
- **配布**: 配布物は 1 ファイル。ただし LSP サーバがサブプロセスになるため「実行時プロセスも 1 つ」は達成しない（2026 年時点で技術的に不可能 — tsserver は C-ABI を持たない JS プログラムであり、ライブラリとして埋め込めない）
- **Linux が最高リスクのターゲット**: WebKitGTK の描画問題（白画面、ソフトウェアラスタライズへの無言のフォールバック、描画劣化）と、AppImage バンドルがサイドカーバイナリを破壊する未解決問題を抱える。**個別の issue 番号は tech.md の Known Risks が実測付きで保持する**（2026-09 時点で当初挙げていたものの多くはクローズ済み、正典は別の issue）
- **依存の下限**: `git2` ≥ 0.21.0（2026 年に unsoundness advisory 3 件）、`zip` ≥ 2.3.0（RUSTSEC-2025-0168）。CI に `cargo audit` を必須とする
- **ライセンス**: 全依存が proprietary 配布と両立する。libgit2 は GPL-2.0 + リンク例外のため安全（libgit2 自体を改変した場合のみ改変版ソースの公開義務）

## Boundary Strategy
- **Why this split**: 「エンジン（意味論）」と「UI（操作）」を一貫して分離している — schema-engine / schema-editor、macro-runtime / macro-editor-lsp、form-builder / form-web-server がその対。エンジン側は Tauri を知らない純粋な Rust ライブラリとして実装・テストでき、UI 側は独立に進められる。また、拡張点を持つスペック（schema-engine の型拡張、data-grid のエディタレジストリ）と、その拡張点を埋めるスペック（custom-types）を分けることで、拡張インターフェースの設計が後付けにならないようにしている。標準マクロライブラリ（macro-stdlib）はランタイム本体とは別物の規模を持つため独立させた。
- **Shared seams to watch**:
  - **document-format ⇔ version-control**: 決定的な JSON 出力レイアウトが差分の品質を直接決める。最も重要な継ぎ目
  - **schema-engine ⇔ custom-types**: 拡張インターフェースの所有権は schema-engine 側。実装は custom-types 側。**schema-engine 側の一括検証経路は結線済み**（拡張型の列を第 1 段から外し、列ごとに 1 回だけ一括判定を呼ぶ。`validate_sheet` と列指定の再検証が同じ経路を通る）。**custom-types 側は `validate_batch` を上書きし、既定実装と同じ結果を返すことをテストで示すこと**。「列ごとに 1 回」は呼び出し回数を数える観測で固定する（`structure.md`「拡張点は所有者と実装者を分ける」）
  - **macro-runtime ⇔ macro-editor-lsp**: ホスト API の .d.ts 生成責任の所在。補完の質はここで決まる
  - **data-grid ⇔ formula-engine ⇔ macro-runtime**: undo / redo スタックを 3 者で共有する。data-grid が最初から共有可能な形で設計すること
  - **form-builder ⇔ form-web-server**: フォームレンダラを共有する。レンダラは Tauri IPC に依存してはならない
  - **app-shell ⇔ macro-editor-lsp**: サイドカー基盤の所有権は app-shell 側。LSP はその利用者

## Progress
チェックボックスは**実装完了**を表す。仕様だけが先行している状態はここに書く。

- `document-format` — 実装完了（38 サブタスク）
- `app-shell` — 実装完了（56 サブタスク）。4 次元の feature 検証（全スイート＋起動の実測 / 要件被覆 / 設計整合と境界 / 横断統合）で一度 **NO-GO** となり、欠けていた検証成果物・出荷物に混入した検証コード・強制検査の欠落・設計の記述のずれを是正して **GO**（2026-09-12）。**ローカルで閉じられない残り（macOS / Windows の実行時、コード署名、3 OS の配布物）は CI の実行で確認する**
- `schema-engine` — **実装完了（31 サブタスク、2026-09-13）**。設計で外部依存を `jiff` と `regex` の 2 本に絞り、10 進数クレートは**採らない**と決めた（tech.md 参照）。feature 検証は **GO**（全スイート green、要件 11 節 66 基準すべてを実装とテストの双方で確認、依存の鎖の逆向き参照 0、境界違反 0）。実測: 10 万行 × 30 列の全件検証 **255 ms**（予算 1 秒）、一意制約を持つ列 1 本の再検証 **31 ms**。**残るスペックがこの実装から写すべき規約は `structure.md`「ドメインクレートの内部構造」と `verification.md`「証拠の取り方」に記録済み**
- 他 11 本 — `brief.md` のみ

## Specs (dependency order)
- [x] document-format -- zip + JSON のドキュメント形式と File/Sheet/Schema/Row のドキュメントモデル。Dependencies: none
- [x] app-shell -- Tauri v2 の器、IPC 境界、サイドカー基盤、3 OS ビルドパイプライン。Dependencies: none
- [x] schema-engine -- ネスト可能な型システム、ANY、検証と型強制、スキーマ移行。Dependencies: document-format
- [ ] data-grid -- 10 万行の仮想化グリッド、型別セルエディタ、共有 undo スタック。Dependencies: app-shell, schema-engine
- [ ] schema-editor -- スキーマのツリー編集 UI と変更の影響プレビュー。Dependencies: app-shell, schema-engine
- [ ] macro-runtime -- deno_core の埋め込み、TS トランスパイル経路、ホスト API、実行の隔離。Dependencies: document-format, schema-engine, app-shell
- [ ] version-control -- git2-rs による自動バージョン管理と構造的差分。Dependencies: document-format, app-shell
- [ ] export-templates -- xlsx / docx テンプレートへの連続書き出し。Dependencies: document-format, schema-engine
- [ ] form-builder -- 配信先中立なフォーム定義、配置エディタ、レンダラ。Dependencies: app-shell, schema-engine
- [ ] custom-types -- JS によるユーザー定義型と型レジストリ。Dependencies: schema-engine, macro-runtime, data-grid
- [ ] macro-stdlib -- 標準マクロライブラリと Excel 互換レイヤー。Dependencies: macro-runtime, schema-engine
- [ ] macro-editor-lsp -- Monaco + LSP のマクロエディタ、言語サーバの同梱と起動。Dependencies: app-shell, macro-runtime
- [ ] formula-engine -- 依存グラフと差分再計算、数式バー。Dependencies: data-grid, macro-runtime, macro-stdlib
- [ ] form-web-server -- axum による LAN 配信、送信の検証と書き戻し。Dependencies: document-format, schema-engine, form-builder, app-shell

## Waves (parallel execution order)
- **Wave 1**: document-format, app-shell
- **Wave 2**: schema-engine
- **Wave 3**: data-grid, schema-editor, macro-runtime, version-control, export-templates, form-builder
- **Wave 4**: custom-types, macro-stdlib, macro-editor-lsp, formula-engine, form-web-server

**Wave は目安であり、実際に着手できるかは各スペックの Dependencies が決める**。2026-09-13 時点で
依存が満たされているのは **6 本**である: `version-control`（document-format / app-shell のみ）、
`data-grid`・`schema-editor`・`export-templates`・`form-builder`（app-shell / schema-engine）、
`macro-runtime`（document-format / schema-engine / app-shell）。
**`schema-engine` の実装完了で着手可能になったのは後ろの 5 本**であり、これを待っていたスペックが一斉に開く。
残り 5 本（`custom-types` / `macro-stdlib` / `macro-editor-lsp` / `formula-engine` / `form-web-server`）は
いずれも `macro-runtime` か `data-grid` を待つ。

**MVP**: Wave 1 + Wave 2 + data-grid + schema-editor。**Wave 1・2 は実装完了済みなので、残りは `data-grid` と `schema-editor` の 2 本**である。この時点で「開いて・型を定義して・編集して・保存できる型付きスプレッドシート」が成立する。

## Prototype-First Risks
以下は spec の design フェーズを待たず、早期にプロトタイプで成立性を確認すべき項目。いずれも失敗した場合にアーキテクチャ全体を変更しうる。
1. **deno_core を Tauri バイナリ内に埋め込む**（macro-runtime）— V8 isolate の current-thread 制約と Tauri の multi-thread ランタイムの共存。この組み合わせに既知の前例がない
2. **AppImage + サイドカーバイナリ**（app-shell / macro-editor-lsp）— Tauri の AppImage バンドルが大きな ELF バイナリを破壊しうる未解決 issue
3. **WebKitGTK 上の canvas グリッドと Monaco**（data-grid / macro-editor-lsp）— Linux での描画問題
4. **docx テンプレート差し込み**（export-templates）— Rust に成熟したテンプレータが存在せず、zip + OOXML の自前実装になる。Word がプレースホルダを複数の run に分割する問題への対処が必要

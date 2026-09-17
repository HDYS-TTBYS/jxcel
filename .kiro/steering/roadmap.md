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
  - **document-session ⇔ data-grid / schema-editor / macro-runtime**: ドキュメントへの変更の適用経路を共有する。所有権は `document-session` 側。**`data-grid` は取り消し履歴を所有するが、変更そのものはこの経路を通す**
  - **form-builder ⇔ form-web-server**: フォームレンダラを共有する。レンダラは Tauri IPC に依存してはならない
  - **app-shell ⇔ macro-editor-lsp**: サイドカー基盤の所有権は app-shell 側。LSP はその利用者

## Progress
チェックボックスは**実装完了**を表す。仕様だけが先行している状態はここに書く。

- `document-format` — 実装完了（38 サブタスク）
- `app-shell` — 実装完了（56 サブタスク）。4 次元の feature 検証（全スイート＋起動の実測 / 要件被覆 / 設計整合と境界 / 横断統合）で一度 **NO-GO** となり、欠けていた検証成果物・出荷物に混入した検証コード・強制検査の欠落・設計の記述のずれを是正して **GO**（2026-09-12）。**ローカルで閉じられない残り（macOS / Windows の実行時、コード署名、3 OS の配布物）は CI の実行で確認する**
- `schema-engine` — **実装完了（31 サブタスク、2026-09-13）**。設計で外部依存を `jiff` と `regex` の 2 本に絞り、10 進数クレートは**採らない**と決めた（tech.md 参照）。feature 検証は **GO**（全スイート green、要件 11 節 66 基準すべてを実装とテストの双方で確認、依存の鎖の逆向き参照 0、境界違反 0）。実測: 10 万行 × 30 列の全件検証 **255 ms**（予算 1 秒）、一意制約を持つ列 1 本の再検証 **31 ms**。**残るスペックがこの実装から写すべき規約は `structure.md`「ドメインクレートの内部構造」と `verification.md`「証拠の取り方」に記録済み**
- `data-grid` — **実装完了（47 サブタスク、2026-09-17。数え方は他のスペックと同じ「`tasks.md` の番号付き小見出しの件数」）**。feature レベルの検証（`/kiro-validate-impl`）は一度 **NO-GO** となり、確定した欠陥 8 件を修復（群 10: 世代を境界が運ぶ・履歴の所有者をウィンドウの保持へ・境界に列の材料を足す・行の対象を序数でも指せるようにする・影響行の表示の序数を運ぶ・違反の理由・文書の差し替えへの追随・貼り付けのメニュー項目）して **GO**。実測（Linux / WebKitGTK、10 万行 × 30 列）: **走査のフレーム時間の中央値 17.00 ms**（要件 11.1 の 16.67 ms。1.6 の実測と同じ歩幅）、**最初の画面 300〜318 ms**（11.2 の 1 秒）、**編集の確定から反映まで 59〜63 ms**（11.3 の 100 ms）、走査の到達 99999 / 100000 行、描画を成立させない条件で告知と `paint_failed` の記録が残る（12.2 / 12.3）。**残るもの**: macOS / Windows の数値は CI の実機でのみ確かめる（開発機では走らせられない）。7.2 の一時的な段（`verify-port-interaction.*`）の去就はレビューの判断として tasks.md に記録した。`document_state` へ版を載せる件（10.7 が閉じられなかった部分）は **`document-session` 側の改修**（design.md の Revalidation Triggers に記録済み）
- `document-session` — **実装完了（22 サブタスク、2026-09-14。数え方は他のスペックと同じ「`tasks.md` の番号付き小見出しの件数」であり、6 件のタスク群の見出しは数えない。**タスク群の見出しのチェックも他のスペックと同じく完了した群には付ける**）**。**チェックボックスは 22 件すべて確定した**（4.3 は 3b14ae8 で取り込み済み、6.1 は 3 巡のレビューを経て 2026-09-14 に完了）。本スペックの正典は「開いたドキュメントの寿命」と「変更の唯一の経路」の 2 つで、**公開面（`crates/document-session/src/lib.rs` の `DocumentSessionsApi` 11 メソッド）と境界の型（`crates/app-shell/src/ipc/document.rs` の 4 コマンドぶん）が確定した**。検証は**タスク単位では GO**（**振る舞いを持つタスクは**実装担当とレビュー担当の 2 者を通り、実測つきの Implementation Notes を残した。足場だけの 2 件（1.1 / 1.4）は Implementation Notes を持たず、証拠は `Cargo.toml` の依存方針コメントと `research.md` のリスク記録にある — 弟スペックも同じ扱いである）。**フィーチャー横断の 4 次元検証（全スイート / 要件被覆 / 設計整合と境界 / 横断統合）は未実施**であり、これを残る作業として明示する。実測（計測は 2026-09-13 20:30 の commit bad41ce の作業時。このホスト、criterion の mean 点推定値。2026-09-14 に `scripts/check-bench-budget.sh` で再判定し 6 本すべて OK）: 10 万行 × 30 列の **open+hold 0.785 s**（予算 3 秒）/ **save 0.637 s**（予算 2 秒）/ **一括の適用 0.117 s**（予算 1 秒 = 全 300 万セルの置き換えを 1 回の `edit` で）。`scripts/check-bench-budget.sh` は 6 本の判定行すべて OK（exit 0）。**出荷物の検査は緑** — `npm run build` で `dist/` を作り直した上で（4.2・4.3 のフロントエンドが入った状態。50 モジュール / 272.09 kB）、`scripts/check-shipping-bundle.sh` が「配布物の資産に検証専用の識別子はありません（走査 1 資産 / `JXCEL_VERIFICATION`=0 `verificationBulk`=0 `verificationScreen`=0 `verification-triggers`=0 / 必須 `smoke-table`,`smoke-editor` は存在）」で exit 0。**実装が先に在るタスクの拘束力は 4 箇所の変異で実測した**（未保存の印 / `set_cells` の索引構築 / `resolve` の冪等の分岐 / 宿主の連鎖の順序。いずれも対応するテストが落ち、`md5` と空の `git diff` で復元を証明）。**下流が写すべき規約は `structure.md`「セッションの所有の規約」と「共有される継ぎ目」（変更の適用の経路）、および `verification.md`「引き金の語彙」と「検査器の規約」の横断の規則（メニュー項目を足すスペックは 2 つの検査器の項目数と一覧を同時に更新する）に記録済み**。**`data-grid` の再検証は要る**（`data-grid/design.md` の Revalidation Trigger「`document-session` の design 確定」）— 同ファイルの `Allowed Dependencies` の `document-session` の項を、確定した公開面（`edit` の閉包の形・`set_cells`・境界の型）に合わせて更新した（再検証の結果、設計の変更は不要であり、`EditApply` の再入禁止は本記録と同じ作業の中で `data-grid/design.md` の `Allowed Dependencies` の `document-session` の項へ記録した）
- `macro-runtime` — **仕様化済み（requirements / design / tasks。2026-09-17）**。**実装の準備が整っている**（`spec.json` は `phase: tasks-generated` / `ready_for_implementation: true`。11 要件 / 54 受入基準、5 群 21 サブタスク）。下調べの要点は `research.md` と `tech.md`「Known Risks」1（実行基盤の成立性は 2026-09-17 の使い捨てスパイクで実測済み）。**設計の要**: 実行ごとに isolate を作る（`deno_core` 0.412 の realm 作成は `pub(crate)` で、使い回すとグローバルが実行間に残る）／打ち切りは V8 の 1 経路に合流（時間は別スレッドの `terminate_execution`、メモリは `heap_limits` + near-heap-limit callback）／変更は実行の間に集約して成功時だけ 1 回で適用（`UndoLabel::MacroRun` の 1 対）／エンジンは `data-grid` と `document-session` を依存に持たない／マクロの**形**は `document-format`（新しいパート 1 形）、**意味**は `macro-runtime`
- 他 9 本 — `brief.md` のみ

## Specs (dependency order)
- [x] document-format -- zip + JSON のドキュメント形式と File/Sheet/Schema/Row のドキュメントモデル。Dependencies: none
- [x] app-shell -- Tauri v2 の器、IPC 境界、サイドカー基盤、3 OS ビルドパイプライン。Dependencies: none
- [x] schema-engine -- ネスト可能な型システム、ANY、検証と型強制、スキーマ移行。Dependencies: document-format
- [x] document-session -- ウィンドウ単位のドキュメント保持、変更の適用経路、未保存の追跡と保存。Dependencies: document-format, app-shell
- [x] data-grid -- 10 万行の仮想化グリッド、型別セルエディタ、共有 undo スタック。Dependencies: app-shell, schema-engine, document-session
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
- **Wave 3**: document-session, schema-editor, macro-runtime, version-control, export-templates, form-builder
- **Wave 3.5**: data-grid（`document-session` を待っていた — **2026-09-14 に充足**）
- **Wave 4**: custom-types, macro-stdlib, macro-editor-lsp, formula-engine, form-web-server

**Wave は目安であり、実際に着手できるかは各スペックの Dependencies が決める**。2026-09-13 時点で
依存が満たされているのは **6 本**である: `version-control`（document-format / app-shell のみ）、
`data-grid`・`schema-editor`・`export-templates`・`form-builder`（app-shell / schema-engine）、
`macro-runtime`（document-format / schema-engine / app-shell）。
**`schema-engine` の実装完了で着手可能になったのは後ろの 5 本**であり、これを待っていたスペックが一斉に開く。
残り 5 本（`custom-types` / `macro-stdlib` / `macro-editor-lsp` / `formula-engine` / `form-web-server`）は
いずれも `macro-runtime` か `data-grid` を待つ。

**2026-09-14 の更新（`document-session` の実装完了を受けて）**: 上の 2026-09-13 の数え方には**ずれがあった** — `data-grid` の依存は `app-shell` / `schema-engine` だけでなく **`document-session` も含む**（`Specs` の行と Wave 3.5 がそう書いている）が、当時 `document-session` は仕様しか無く、**依存は満たされていなかった**。`document-session` が実装完了したことで、**`document-session` を依存に持つ唯一のスペックである `data-grid` の依存も満たされた**（`data-grid` を待っていた `custom-types` / `formula-engine` はここから開く）。**依存が満たされているスペックは 6 本のままである** — `document-session` の完了で新たに開いたのは `data-grid` の 1 本だけであり、上の 5 本（`schema-editor` / `macro-runtime` / `version-control` / `export-templates` / `form-builder`）は 2026-09-13 から変わらない。

**2026-09-17 の更新（`data-grid` の実装完了と `macro-runtime` の仕様化を受けて）**: `data-grid` が完了したことで**依存が満たされているスペックは 5 本**になった（`schema-editor` / `macro-runtime` / `version-control` / `export-templates` / `form-builder`）。**`macro-runtime` はその 5 本のうち唯一「仕様化済み」であり、実装を開始できる**（`custom-types` / `macro-stdlib` / `macro-editor-lsp` / `formula-engine` の 4 本はここを待っている）。**Prototype-First Risks の未着手は #4（docx テンプレート差し込み = `export-templates`）だけ**であり、次に着手するスペックを選ぶときは #4 を抱える `export-templates` を、着手の前にプロトタイプで確かめる対象として扱うこと。

**MVP**: Wave 1 + Wave 2 + document-session + data-grid + schema-editor。**2026-09-17 時点で残るのは `schema-editor` の 1 本**である（Wave 1・2 は実装完了済み、`document-session` は 2026-09-14、`data-grid` は 2026-09-17 に完了）。（当初は 2 本としていたが、MVP の文言にある「**開いて**…**保存できる**」を担う持ち主が存在しないことが `data-grid` の設計中に判明したため 1 本増え、いま `document-session` の完了で元へ戻った。）この時点で「開いて・型を定義して・編集して・保存できる型付きスプレッドシート」が成立する。

## Prototype-First Risks
以下は spec の design フェーズを待たず、早期にプロトタイプで成立性を確認すべき項目。いずれも失敗した場合にアーキテクチャ全体を変更しうる。**「確定」とあるものは実装または設計で解消済みであり、残るリスクは 4（docx テンプレート差し込み）だけである**（1 は 2026-09-17 のスパイクで成立、2 は設計と実測、3 はグリッドの側が `data-grid` の 2026-09-17 の実測で閉じ、Monaco の側だけが `macro-editor-lsp` に残る）。**次に着手するスペックを選ぶときは、1（`macro-runtime`）を最優先の候補として扱うこと** — 失敗すればアーキテクチャ全体が変わり、かつ `custom-types` / `macro-stdlib` / `macro-editor-lsp` / `formula-engine` の 4 本がここを待っている
1. **deno_core を Tauri バイナリ内に埋め込む**（macro-runtime）— V8 isolate の current-thread 制約と Tauri の multi-thread ランタイムの共存。この組み合わせに既知の前例がない。**2026-09-17 の使い捨てスパイクで成立した**（専用スレッドが isolate を所有し、multi-thread 側からチャネル越しに呼ぶ形。同期・**非同期のホスト呼び出し**・同時呼び出し・例外と panic の隔離・後始末まで実測。isolate の生成 5.5 ms、常駐 +24 MB、実行ファイル +68 MB）。**残るのは実際の Tauri コマンド層への結線**であり、確定した形と踏んだ穴は `tech.md`「Known Risks」1 に記録した
2. **AppImage + サイドカーバイナリ**（app-shell / macro-editor-lsp）— Tauri の AppImage バンドルが大きな ELF バイナリを破壊しうる未解決 issue。**回避配置は `app-shell` で確定・実測済み**（`usr/share/` へ置き走査対象の外に出す。`tech.md`「Known Risks」2 を参照）
3. **WebKitGTK 上の canvas グリッドと Monaco**（data-grid / macro-editor-lsp）— Linux での描画問題。**検出の仕組みは実装済み**（起動時は `app-shell` の `RenderWatchdog`、グリッドは `data-grid` の `RenderProbe`）。**グリッドライブラリの選定も `data-grid` の design で確定**（Glide 6.0.4-alpha24 + 移植口）。**グリッドの側は実測済みである**（`data-grid` が 2026-09-17 に完了。Linux / WebKitGTK の実画面で走査の中央値 17.00 ms = 毎秒 60 回を維持、最初の画面 300〜318 ms、描画不成立の検出と告知も実起動で観測。**Monaco の側は `macro-editor-lsp` の実装時に同じ段で確かめる**）
4. **docx テンプレート差し込み**（export-templates）— Rust に成熟したテンプレータが存在せず、zip + OOXML の自前実装になる。Word がプレースホルダを複数の run に分割する問題への対処が必要。**未着手**

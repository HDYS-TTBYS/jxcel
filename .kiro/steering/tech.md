# Technology Stack

## Architecture

**Rust ドメインコア + Tauri v2 シェル + Web フロントエンド** の 3 層。

一貫した原則は **「エンジン（意味論）と UI（操作）の分離」** である。ドメインロジックは Tauri を知らない純粋な Rust ライブラリとして実装され、Tauri コマンド層は薄いアダプタに徹する。これにより、ドメインは GUI を起動せずにテストでき、UI は独立して進められる。

この分離はスペックの分割にもそのまま現れている（`schema-engine` / `schema-editor`、`macro-runtime` / `macro-editor-lsp`、`form-builder` / `form-web-server`）。

## Core Technologies

- **Language**: Rust（バックエンド / ドメイン）、TypeScript（フロントエンド / マクロ）
- **Shell**: Tauri v2 — Windows は WebView2、macOS は WKWebView、Linux は WebKitGTK
- **配布**: 単一実行ファイル。Windows / macOS は OS 標準 WebView に乗る真の単一 exe、Linux は WebKitGTK 同梱の AppImage。**実測（`app-shell`）: Linux の AppImage は約 81MB、deb は約 4.7MB**（WebKitGTK を含む）。言語サーバとその実行環境を加えると 150〜200MB を見込む

## Key Libraries

パターンを規定するものだけを挙げる。網羅ではない。

| 用途 | 選定 | 理由 |
|---|---|---|
| マクロランタイム | `deno_core`（V8） | TS フルサポート、default-deny サンドボックス |
| TS トランスパイル | `deno_ast` + `swc` | `deno_core` は素では TS を実行できないため必須 |
| バージョン管理 | `git2-rs`（libgit2） | merge / conflict まで完備。`gitoxide` は push/merge/rebase が未完成 |
| グリッド | `@glideapps/glide-data-grid` **6.0.4-alpha24**（`data-grid` が選定。**実装はこれから**。stable 6.0.3 は React 19 の peer を受け付けない） | 10 万行で定常メモリ。**stable は 2024-02 で止まり alpha が続いている**ため、「移植口（`RendererPort`）の背後の実装」という形で採り、タスク最初期の実測で毎秒 60 回に届かなければ同じ口の背後に自前 canvas を置く（投機的な抽象ではなく退路）。MIT |
| マクロエディタ | Monaco + `monaco-languageclient` | LSP 統合の既製経路 |
| xlsx | `umya-spreadsheet`（テンプレート差し込み） / `rust_xlsxwriter`（新規作成） | 前者は既存ブックを開いて書き換えられる |
| HTTP | `axum` | フォームの LAN 配信 |
| ZIP コンテナ | `zip` 8.x（`deflate-flate2` のみ有効化） | 既定 feature 束は zlib-rs / zopfli を誘発する。**圧縮バックエンドを `miniz_oxide` に固定し、C バインディングを依存ツリーに入れない** |
| 内容アドレス | `blake3` | 添付の同一性判定とパートのダイジェスト |
| 識別子 | `ulid` | 行・シート・型定義の識別子。時刻順序を持ち、並べ替えで不変 |
| Rust⇔TS 型共有 | `ts-rs` | `tauri-specta` を却下（JSON を通さない生バイト経路を型付けできず、生成物に `any` が入り、2023 年から RC のまま） |
| フロントエンド | React + Vite（SPA） | `monaco-languageclient` の一次ラッパが React のみ。canvas グリッドの実績も React 前提。Tauri は SSR 非対応 |
| ネイティブダイアログ | `gtk` 0.18（Linux） / `rfd` 0.17（Windows / macOS） | **`tauri-plugin-dialog` は使わない** — 全版で `tauri-plugin-fs` を非オプション依存に持ち（下記「禁止依存」を壊す）、Linux の実装は親ウィンドウを渡せない。直接使えばどちらも回避でき、親ウィンドウの指定が確実になる |
| 日時 | `jiff` 0.2（tzdb を同梱する feature は無効） | 曖昧な入力・オフセットの矛盾・日付への `Z` 付与を**既定で拒否**する。地域設定に依存する解析経路を持たない。`Zoned`（IANA 注釈付き）を使うと tzdb が実行ファイルに埋まるため使わない。`chrono` は解析が寛容、`time` は利用者入力の解析でスタック枯渇の前例（RUSTSEC-2026-0009） |
| 書式（正規表現） | `regex` 1.13 | 利用者が書いたパターンを実行するため、**線形時間が構成上保証される**ものに限る（後方参照・先読みは非対応でコンパイル時に落ちる）。`fancy-regex` は後戻り式で ReDoS の余地がある。パターン長・コンパイル後の大きさ・入れ子の深さに上限を課す |
| ログ | `tauri-plugin-log` 2.9.1 | 追記型のローテーション・保持世代・1 ファイル上限を設定で与えられる。**`Builder::level` は基盤の水準を構築時に固定する** ため、詳細度を実行中に**上げる**には「受入上限を最大にし、実効フィルタを全体の上限 1 つに寄せる」形が要る（片方だけでは下げる変更しか効かない） |

## Development Standards

### Dependency Floors（セキュリティ）
以下は下限を明示的に固定し、古いバージョンへピン留めしない。

- `git2` **≥ 0.21.0** — 2026 年に unsoundness advisory が 3 件（RUSTSEC-2026-0008 / 0183 / 0184）
- `zip` **≥ 2.3.0** — RUSTSEC-2025-0168（展開時のシンボリックリンク経由の任意ファイル書き込み）。`zip-extract` / `zip_next` はメンテ終了フォークのため使用禁止。現在の解決版は 8.x
- `tauri` **≥ 2.11.3** — 2.11.1 に security fix 2 件（`AppManifest` 未設定時に自前コマンドの ACL が迂回される、Windows の `.localhost` サフィックスによる origin 混同）。2.11.3 で起動性能が改善
- `tauri-plugin-single-instance` **≥ 2.4.3** — macOS のスレッドブロック不具合の修正版

**逐語で往復する値をライブラリに通さない。**`Decimal` のように「同じ内容は同じバイト列」を契約に持つ値は、10 進数クレート（`rust_decimal` / `bigdecimal` / `fastnum`）を通した時点で先頭の 0・符号・指数形のいずれかが正規化され、往復が壊れる。桁数の検査や比較のような**読み取りだけの処理は自前で書く**（1 パスの文字走査で足りる）。演算が本当に必要になった時点で初めてクレートを検討する。この判断は `document-format` の決定的出力に依存するすべてのクレートに効く。

**禁止依存（下限ではなく「不在」を固定する）**: `tauri-plugin-fs` / `-shell` / `-store` / `-dialog` を依存ツリーに入れない。**フロントエンドから任意のファイルとプロセスへ到達する経路を作らないための第一の制御**であり、`scripts/check-forbidden-plugins.sh` が機械的に守る。capability を絞るのは第二の制御で、**両方要る**（権限は「与え忘れ」で壊れるが、依存は入れた時点で経路が存在する）。ネイティブダイアログが上表で `gtk` / `rfd` を直接使うのはこの制約のためである。

CI に `cargo audit` を必須とする。本プロジェクトは advisory 履歴を持つ crate に依存しているため、これは形式的な要件ではない。

### Type Safety
- マクロ向けに公開するすべての API は TypeScript の型が完全に付くこと。型が付かない API は補完体験を壊すため許容しない
- ホスト API の `.d.ts` は生成可能な形で管理する（LSP の補完の入力になる）

### Testing
- ドメインコアは GUI を起動せずにテストできること。Tauri への依存がテストを妨げるなら、それは層の分離が壊れている兆候
- 性能要件を持つ機能はベンチマークを伴うこと（例: 10 万行で開く 3 秒 / 保存 2 秒。`schema-engine` は 10 万行 × 30 列の全件検証 1 秒 / 1 セルの判定 16 ミリ秒。**後者は要件値であり、CI のゲートには載せない** — 予算に対して桁違いに小さく、ゲートにする価値がないためテスト内の計測に留める）
- **性能予算は CI のゲートにすること。**計測して記録するだけでは回帰は止まらない。判定器は**計測が無いときに `0` を返してはならない**（fail-closed。`structure.md`「計測が存在しない状態で予算ゲートだけ先に結線しない」と対になる）。**計測が無い状態でゲートだけ先に結線すると CI が赤のままになるため、結線は計測を入れるタスクが行う**。現在の対象と予算: `document-format` の open 3 秒 / save 2 秒、`schema-engine` の全件検証 1 秒、`document-session` の open+hold 3 秒 / save 2 秒 / **一括の適用 1 秒**（10 万行 × 30 列の全 300 万セルを 1 回の `edit` で置き換える）。予算の既定値は判定器（`scripts/check-bench-budget.sh`）が持ち、**CI は引数なしで呼ぶ**（閾値を CI から渡さない）
- **不変条件は検査スクリプトにすること。**目視確認で守る規則は、いずれ守られなくなる（`scripts/` に置き CI から呼ぶ。structure.md 参照）
- **GUI・配布物・プラットフォーム差を含む主張は、実物を起動して観測した結果で裏付けること。**単体テストは回帰の網であって受入の証明ではない
- **観測できないことは未確認として書くこと。**「CI で確認する」は確認済みではない。残るリスクを明示する
- 検証専用のコードは非既定の feature（Rust）／ビルド時の定数（フロントエンド）で切り、**出荷物に残さない**。残っていないことを検査で固定する
- GUI と 3 OS の検証の具体的な規約は `.kiro/steering/verification.md` に置く

## Development Environment

Cargo ワークスペース（`crates/*` + `src-tauri`）とフロントエンド（`src/`）、3 OS の CI が稼働している。**配布物は必ず Tauri CLI 経由で作る** — 素の `cargo build` は開発用の形になり、フロントエンドを埋め込まない（白いウィンドウになる）。

| 目的 | コマンド |
|---|---|
| ビルド | `cargo build --workspace --all-targets` |
| テスト | `cargo test --workspace --no-fail-fast` |
| 決定性の検証（3 OS でバイト一致） | `cargo test -p document-format --test determinism` |
| 生成物のドリフト | `cargo test -p app-shell --test bindings_drift` |
| 生成物の再生成 | `cargo run -p app-shell --bin generate-bindings` |
| フロントエンドの型検査 / lint / ビルド | `npm run typecheck` / `npm run lint` / `npm run build` |
| ベンチマーク | `cargo bench -p document-format -p schema-engine -p document-session -- --save-baseline=main` |
| 性能予算の判定 | `bash scripts/check-bench-budget.sh` |
| 起動時間予算の判定 | `bash scripts/check-startup-budget.sh` |
| 依存下限の検査 | `bash scripts/check-zip-floor.sh Cargo.lock` |
| 脆弱性検査 | `cargo audit` |
| 配布物の生成 | `npx tauri build --bundles <形式>` |
| 検証用の形の生成 | `JXCEL_VERIFICATION_BUILD=1 npx tauri build --no-bundle --features verification-triggers` |

**不変条件の検査は `scripts/check-*.sh` に揃っている**（依存下限・性能予算・起動時間・生成物のドリフト・権限の逸脱・配信先中立な資産の依存・コアクレートの tauri 非依存・禁止プラグイン・コマンドと権限の一致・出荷物への検証コード混入・配布物の補助プロセス整合性・3 OS の実画面検証・**文書のセッションの 3 OS 観測**）。**規約と一覧の考え方は `.kiro/steering/verification.md` に置く。**

`Cargo.lock` は追跡する。jxcel はライブラリではなくアプリケーションであり、下記の依存下限を固定する方針は lockfile が追跡されていて初めて意味を持つ。

## Key Technical Decisions

### なぜ Tauri v2 か
要件のうち「LSP 付き高機能マクロエディタ」と「10 万行級グリッド」の 2 つは本質的に Web 技術の領域にある。この 2 つを要件に置いた時点で、ネイティブ GUI 案はエディタとグリッドの自作コストが他の全機能を圧迫する。

**却下した代替案**:
- **Rust ネイティブ GUI（egui / GPUI / Slint）** — 完全静的バイナリという唯一の優位を持つが、Monaco も CodeMirror も動かず、LSP サーバはどのみち別プロセスになるためその優位は部分的にしか得られない
- **Go + Wails** — ビルドは最も楽だが、`go-git` の 3-way マージが libgit2 に明確に劣る。バージョン管理が中心価値である本プロジェクトでは致命的。`goja` は TypeScript 非対応
- **Electron / CEF** — 単一実行ファイル要件を満たさない（100〜200MB、ディレクトリ構成）
- **Qt** — 静的リンクした proprietary 単一バイナリには商用ライセンスが必須。要件と直接衝突する
- **Servo / Ultralight** — Servo は embeddable API が 2026-04 公開で若すぎる。Ultralight はクローズドソース部分と収益制限を持つ

### 「配布物 1 ファイル」と「実行時プロセス 1 つ」は別物
tsserver は C-ABI を持たない JS プログラムであり、ライブラリとして埋め込めない。したがって LSP サーバは必ず別プロセスになる。**配布物が 1 ファイルであることは守るが、実行時にプロセスが 1 つであることは 2026 年時点で技術的に達成不可能**である。この区別を設計上の前提として全スペックで共有する。

### ライセンス
全依存が proprietary 配布と両立する。libgit2 は GPL-2.0 + リンク例外のため安全（libgit2 自体を改変した場合のみ改変版ソースの公開義務が生じる）。Monaco・deno_core・Glide Data Grid・axum はいずれも MIT。

## Known Risks (prototype-first)

以下は design を待たず早期にプロトタイプで成立性を確認する。いずれも失敗した場合にアーキテクチャ全体を変更しうる。

1. **`deno_core` を Tauri バイナリ内に埋め込む** — V8 isolate は `Send` でなく current-thread ランタイムを要求するが、Tauri の既定は multi-thread。専用 OS スレッド + チャネル橋渡しが必要。**この組み合わせに既知の前例が見つかっていない**（既存の tauri + deno プロジェクトはすべて Deno を別プロセスに逃がしている）
   **2026-09-17 の使い捨てスパイクで成立した**（`target/spike-macro-runtime`。**使い捨てであり追跡しない** — 記録はここに残す）。Tauri のコマンド層と同じ形（**multi-thread の tokio ランタイム + チャネル**）で専用スレッドの isolate を呼び、release での実測は: isolate の生成 **5.5 ms** / 後始末 0.3 ms / 常駐 **+24 MB** / 100,000 件の畳み込み 4.1 ms / **8 本の同時呼び出しが 0.1 ms**（直列化される）/ JS の例外は `Err` として返る / **op の panic を捕捉してもランタイムは生き続ける**（panic の後の評価も通る）/ 静的リンク後の実行ファイル **68 MB**。
   **確定した形**（動かなかった形も実測であり、記録として残す）:
   - 専用 OS スレッドが `JsRuntime` を所有し、その上で **current-thread の tokio ランタイムを 1 つ**回す。**`LocalSet` を挟まない**（挟んだ形では deferred op の完了が届かなかった）。多スレッド側からは **mpsc + oneshot** 越しに呼ぶ（isolate は境界を越えない）
   - **`JsRuntime::resolve` を使わない** — 実測で、イベントループが回り切った後でも返らなかった。正しい形は「`execute_script` → `run_event_loop(Default::default()).await` → **Promise の状態を読む**（`Local::try_cast::<Promise>()` の `state()` / `result()`）」
   - 非同期 op は **`#[op2]` を `async fn` に付ける**（`deno_core` 自身の検査 `runtime/tests/ops.rs` と同じ形）。`(lazy)` / `(deferred)` は完了が Promise へ届かなかった
   - op の状態は**型として受け取る**（非同期は `Rc<RefCell<OpState>>`、同期は `&mut OpState`）。`#[state]` 属性はこの版に無い
   - op は JS から **`Deno.core.ops.<name>()`** で呼ぶ。**`op_panic` は `deno_core` が持つ組込**であり、同じ名前の op を登録すると `Found ops with duplicate names` で isolate の生成に失敗する
   - **残る未確認は実際のアプリへの結線だけ**（`src-tauri` のコマンド層から await する形と capability）。本スパイクが確かめたのは Tauri と同じ形のランタイム共存であり、結線は `macro-runtime` の設計で確かめる
   - 副次的な費用: 実行ファイルが **68 MB** 増える（V8 + `deno_core` の静的リンク。`deno_ast` + `swc` の TS トランスパイルを足すとさらに増える）。**起動時に isolate を作る必要は無い** — 生成は 5.5 ms であり、初回のマクロ実行まで遅らせれば常駐 +24 MB も後ろへ送れる（単一実行ファイルの要求は静的な埋め込みで満たされる）
2. **AppImage + サイドカーバイナリ** — AppImage のバンドル処理が `usr/bin` 配下の ELF に無条件で rpath を書き込み、追記型ペイロードを持つ実行ファイル（Node SEA / pkg / Bun compile / PyInstaller / Nuitka）を破壊する。正典は `tauri-apps/tauri#5189`（2022 年から open、2026-01 に Tauri v2 で再現報告）であり、`#11898` は 2024-12 以降停止している。除外設定は存在せず `NO_STRIP=1` も効かない。**回避配置は `app-shell` の design で確定済み**（`usr/share/` へ置き走査対象の外に出す）
3. **WebKitGTK 上の canvas グリッドと Monaco** — 白画面、ソフトウェアラスタライズへの無言のフォールバック、描画劣化。**Linux を最高リスクのターゲットとして扱う**。2026-09 の実測で現在 open なのは `#5143`（白画面）、`#15936`（ソフトウェア GL 下の白いウィンドウ。**検出手段がないこと自体が主題**）、`#14721`（NVIDIA 環境の SIGSEGV）、`#10702` / `#14924`（Wayland Error 71）。当初挙げていた `#5761` / `#7021` / `#13157` はいずれもクローズ済み（ただし `#13157` は NOT_PLANNED であり未修正。WebKitGTK 2.48.0 で発現）。**基盤側に描画失敗を検出する API は無く、Tauri も wry も回避策の環境変数を自動設定しない** — したがって検出は**アプリ側が自分で持つ**。実装済みの分: 起動の初回描画は `app-shell` の `RenderWatchdog`（`crates/app-shell/src/render.rs`）が**三値**（`Painted` / `SoftwareRaster` / `NoPaint`）で判定し、**通知が期限内に届かないことだけを不成立の根拠**にする（描画を検出する API が無いため）。不成立のときは設定に印（`render.fallback`）を残し、次の起動が代替経路を適用する。**グリッド自身が塗れたかは `data-grid` の `RenderProbe`（`probePaint` / `sampleFrameTimes`）が受け持つ**（要件 12.2 / 12.3）。**起動時の判定とグリッドの判定は別物である** — 前者は「シェルが描けたか」、後者は「canvas が実際に塗れたか」であり、両方を要する
4. **docx テンプレート差し込み** — Rust に成熟したテンプレータが存在せず ZIP + OOXML の自前実装になる。Word がプレースホルダを複数の run に分割する問題への対処が必要

---
_Document standards and patterns, not every dependency_

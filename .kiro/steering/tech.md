# Technology Stack

## Architecture

**Rust ドメインコア + Tauri v2 シェル + Web フロントエンド** の 3 層。

一貫した原則は **「エンジン（意味論）と UI（操作）の分離」** である。ドメインロジックは Tauri を知らない純粋な Rust ライブラリとして実装され、Tauri コマンド層は薄いアダプタに徹する。これにより、ドメインは GUI を起動せずにテストでき、UI は独立して進められる。

この分離はスペックの分割にもそのまま現れている（`schema-engine` / `schema-editor`、`macro-runtime` / `macro-editor-lsp`、`form-builder` / `form-web-server`）。

## Core Technologies

- **Language**: Rust（バックエンド / ドメイン）、TypeScript（フロントエンド / マクロ）
- **Shell**: Tauri v2 — Windows は WebView2、macOS は WKWebView、Linux は WebKitGTK
- **配布**: 単一実行ファイル。Windows / macOS は OS 標準 WebView に乗る真の単一 exe、Linux は WebKitGTK 同梱の AppImage（約 76MB）

## Key Libraries

パターンを規定するものだけを挙げる。網羅ではない。

| 用途 | 選定 | 理由 |
|---|---|---|
| マクロランタイム | `deno_core`（V8） | TS フルサポート、default-deny サンドボックス |
| TS トランスパイル | `deno_ast` + `swc` | `deno_core` は素では TS を実行できないため必須 |
| バージョン管理 | `git2-rs`（libgit2） | merge / conflict まで完備。`gitoxide` は push/merge/rebase が未完成 |
| グリッド | canvas ベースの仮想化グリッド（未確定） | 10 万行で定常メモリ。第一候補 Glide Data Grid だがメンテ停滞のため design で再評価 |
| マクロエディタ | Monaco + `monaco-languageclient` | LSP 統合の既製経路 |
| xlsx | `umya-spreadsheet`（テンプレート差し込み） / `rust_xlsxwriter`（新規作成） | 前者は既存ブックを開いて書き換えられる |
| HTTP | `axum` | フォームの LAN 配信 |

## Development Standards

### Dependency Floors（セキュリティ）
以下は下限を明示的に固定し、古いバージョンへピン留めしない。

- `git2` **≥ 0.21.0** — 2026 年に unsoundness advisory が 3 件（RUSTSEC-2026-0008 / 0183 / 0184）
- `zip` **≥ 2.3.0** — RUSTSEC-2025-0168（展開時のシンボリックリンク経由の任意ファイル書き込み）。`zip-extract` / `zip_next` はメンテ終了フォークのため使用禁止

CI に `cargo audit` を必須とする。本プロジェクトは advisory 履歴を持つ crate に依存しているため、これは形式的な要件ではない。

### Type Safety
- マクロ向けに公開するすべての API は TypeScript の型が完全に付くこと。型が付かない API は補完体験を壊すため許容しない
- ホスト API の `.d.ts` は生成可能な形で管理する（LSP の補完の入力になる）

### Testing
- ドメインコアは GUI を起動せずにテストできること。Tauri への依存がテストを妨げるなら、それは層の分離が壊れている兆候
- 性能要件を持つ機能はベンチマークを伴うこと（例: 10 万行で開く 3 秒 / 保存 2 秒）

## Development Environment

ワークスペースは未スキャフォールド。ビルド・テスト・実行のコマンドは `app-shell` スペックで確定し、その時点で本節を更新する。

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
2. **AppImage + サイドカーバイナリ** — Tauri の AppImage バンドルが大きな ELF バイナリを再リンクで破壊しうる（`tauri-apps/tauri#11898`、未解決）
3. **WebKitGTK 上の canvas グリッドと Monaco** — 白画面、ソフトウェアラスタライズへの無言のフォールバック、描画劣化の既知 issue が複数（`#5761`、`#13157`、`#7021`）。**Linux を最高リスクのターゲットとして扱う**
4. **docx テンプレート差し込み** — Rust に成熟したテンプレータが存在せず ZIP + OOXML の自前実装になる。Word がプレースホルダを複数の run に分割する問題への対処が必要

---
_Document standards and patterns, not every dependency_

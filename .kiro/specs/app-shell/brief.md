# Brief: app-shell

## Problem
「単一実行ファイル・マルチプラットフォーム」という要件を満たす器がなければ、他のどの機能もユーザーに届かない。また、Rust 側のドメインロジックと Web フロントエンドの間の通信境界を最初に決めておかないと、後続スペックがそれぞれ勝手な IPC を生やして収拾がつかなくなる。

## Current State
グリーンフィールド。技術調査により Rust + Tauri v2 を採用済み。Windows / macOS は OS 標準 WebView に乗るため真の単一 exe、Linux は WebKitGTK を同梱した AppImage（約 76MB）で自己完結させる方針。

## Desired Outcome
3 つの OS で単一実行ファイルとして起動し、ウィンドウが開き、フロントエンドから Rust コアの機能を型安全に呼べる。CI で 3 プラットフォーム分のバイナリが出力される。LSP サーバのようなサイドカープロセスを起動する基盤が整っている。

## Approach
Tauri v2 でプロジェクトを構成する。Rust 側は「ドメインコア（Tauri 非依存）」と「Tauri コマンド層（薄いアダプタ）」に分離し、ドメインロジックが Tauri に汚染されないようにする。フロントエンドとの型共有はコード生成で担保する。サイドカーバイナリの埋め込みと初回起動時展開の仕組みをここで用意し、macro-editor-lsp が利用する。

## Scope
- **In**: Tauri v2 プロジェクト構成、ウィンドウ / メニュー / ショートカット、IPC コマンド境界と Rust⇔TS の型共有、サイドカーバイナリの埋め込み・展開・起動基盤、3 OS ビルドパイプライン（CI）、アプリ設定の永続化、ロギングとクラッシュレポート、フロントエンドのシェル（レイアウト・ルーティング・テーマ）
- **Out**: 業務ロジック全般、個別画面の中身、ドキュメントモデル

## Boundary Candidates
- Rust ドメインコア（Tauri 非依存のライブラリ）と Tauri コマンド層の分離 — これにより各機能スペックは Tauri を知らずに実装・テストできる
- フロントエンドのシェル（レイアウト・ナビゲーション）と 個別画面コンポーネントの分離
- サイドカー管理を独立したモジュールにし、LSP 以外の将来のサブプロセスにも使えるようにする

## Out of Boundary
- グリッド、マクロエディタ、フォームビルダーなど個別画面の実装
- ドキュメントの読み書き（document-format が所有）
- 自動更新 / 署名配布（個人利用向けのため当面の非目標）

## Upstream / Downstream
- **Upstream**: なし（document-format と並行着手可能）
- **Downstream**: 全スペックがこの上に乗る

## Existing Spec Touchpoints
- **Extends**: なし（新規プロジェクト）
- **Adjacent**: document-format と並行着手可能

## Constraints
- Tauri v2（2.11.5 / 2026-07 時点で活発）。Windows は WebView2、macOS は WKWebView、Linux は WebKitGTK
- Linux 配布は AppImage（約 76MB、WebKitGTK 同梱で自己完結）
- **AppImage バンドル + サイドカーは未解決の問題領域**。Tauri の AppImage バンドル処理が非自明な ELF バイナリを再リンクで破壊しうる（`tauri-apps/tauri#11898`）。加えてサイドカーの「file not found」「permission denied (OS error 13)」は頻出の初期トラップ（`#9981`、`#7460`。ターゲットトリプル接尾辞と `shell:allow-execute` / `shell:allow-spawn` の capability 指定が必要）。**サイドカー基盤は最初期にプロトタイプで成立性を確認すること**
- CI に `cargo audit` を組み込み、依存の advisory を継続監視すること（本プロジェクトは git2・zip とも 2025〜2026 に advisory が出た crate に依存する）
- Linux/WebKitGTK を最高リスクのターゲットとして扱い、描画問題の回避策（`WEBKIT_DISABLE_DMABUF_RENDERER` 等）の適用方針を決めること
- 「配布物は 1 ファイル」は満たすが「実行時プロセスも 1 つ」は満たさない（LSP がサブプロセスになる）ことを設計上の前提として明記する

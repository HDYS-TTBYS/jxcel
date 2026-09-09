# Brief: macro-runtime

## Problem
本アプリの中心的な設計判断は「関数はマクロに統合する」ことにある。つまり Excel の組込関数のような閉じた式言語を持たず、すべての計算を JS/TS マクロとして表現する。そのためには、ドキュメントを安全かつ高速に読み書きできる埋め込み JS ランタイムが必要になる。これがなければ数式も標準ライブラリもユーザー定義型も成立しない。

## Current State
document-format がデータを、schema-engine が型を提供する。プログラマブルに触る手段が存在しない。

## Desired Outcome
ユーザーが書いた TypeScript / JavaScript が、アプリ内でドキュメントを読み書きしながら実行される。ホスト API が型付きで公開され、実行の失敗がアプリ全体を巻き込まない。同じランタイムが数式・標準ライブラリ・ユーザー定義型・エクスポート処理から共通に使われる。

## Approach
Rust バックエンドに `deno_core` を埋め込む（V8、default-deny のサンドボックス）。ただし技術検証により 2 つの追加作業が判明している。(1) `deno_core` は純粋な JS エンジンであり **TypeScript を素では実行できない** ため、`deno_ast` + `swc` によるトランスパイル module loader を自前で用意する必要がある。(2) V8 isolate は `Send` ではなく current-thread の tokio ランタイムを要求するが、Tauri の既定は multi-thread のため、**専用 OS スレッド + チャネル橋渡しのアーキテクチャが必須**である。実行はワーカースレッドに隔離し、無限ループやメモリ暴走をタイムアウト・上限で打ち切れるようにする。個人利用向けのため、サンドボックスは「敵対的コードに対する硬い境界」ではなく「事故防止」の水準に設定する。

## Scope
- **In**: deno_core の埋め込みと Tauri の非同期ランタイムとの共存、TypeScript のトランスパイル経路、ホスト API 設計（ドキュメント / シート / 行 / セル / スキーマへのアクセス）、実行モデル（同期 / 非同期、トランザクション境界、undo スタックとの統合）、実行の隔離とタイムアウト・リソース上限、エラーとスタックトレースの表現、マクロの保存場所とドキュメントとの関係、権限モデル（ファイル / ネットワークアクセスの明示的な許可）
- **Out**: 標準マクロライブラリの中身（macro-stdlib）、エディタと LSP（macro-editor-lsp）、数式としての呼び出しと再計算（formula-engine）、型定義への応用（custom-types）

## Boundary Candidates
- JS エンジンの埋め込み層と ホスト API 層の分離
- ホスト API の「能力（capability）」定義を独立させ、権限モデルと標準ライブラリの両方がそれを参照する構造にする
- 実行の隔離・スケジューリングを独立させ、数式の一括再計算とユーザー起動のマクロが同じ実行基盤に乗るようにする

## Out of Boundary
- Excel 関数相当のライブラリ実装（macro-stdlib が所有）
- 補完・診断・エディタ UI（macro-editor-lsp が所有）
- セルへの数式入力と依存グラフ（formula-engine が所有）
- 敵対的コードに対する強固なセキュリティ境界（個人利用前提のため明示的な非目標）

## Upstream / Downstream
- **Upstream**: document-format, schema-engine, app-shell
- **Downstream**: macro-stdlib, custom-types, formula-engine, macro-editor-lsp, export-templates（スクリプト駆動のエクスポート）

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: macro-editor-lsp（ホスト API の型定義を共有する。LSP の補完はここで公開した API の型から生成される）

## Constraints
- `deno_core` は current-thread tokio ランタイムを要求し、V8 isolate はスレッド間を移動できない。Tauri の既定 multi-thread ランタイムとの共存方式（専用スレッド + チャネル）を design で確定させること
- **この組み合わせ（deno_core を Tauri バイナリ内にライブラリとして埋め込む）に既知の前例が見つかっていない**。既存の tauri + deno プロジェクトはいずれも Deno を別プロセスとして逃がしている。tasks の最初期にプロトタイプで成立性を確認すること
- TypeScript のトランスパイル経路（`deno_ast` + `swc` による custom ModuleLoader）を自前実装する必要がある。`deno_core` の TS 対応を前提にしないこと
- deno_core の GitHub リポジトリは 2026-04-02 に archive され `denoland/deno` monorepo に統合された。crate 自体は活発（0.411.0 / 2026-08）だが、issue の追跡先が変わっている
- ホスト API の型定義（.d.ts）は macro-editor-lsp の補完の入力になるため、生成可能な形で管理する
- ANY 型の値が JS 側にどう現れるかを schema-engine と整合させること
- 10 万行に対する一括処理が実用時間で終わること。行ごとに IPC を跨がない API 形状にする

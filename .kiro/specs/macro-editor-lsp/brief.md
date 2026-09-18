# Brief: macro-editor-lsp

## Problem
「LSP あり高機能マクロエディター」は本アプリの差別化要因のひとつ。マクロがアプリの中心である以上、それを書く体験が貧弱なら設計全体が破綻する。一方で技術調査により、tsserver はライブラリとして埋め込めない（C-ABI を持たない JS プログラムである）ことが判明しており、単一実行ファイル要件との両立に固有の工夫が要る。

## Current State
app-shell がサイドカープロセスの起動基盤を持ち、macro-runtime がホスト API とその型定義を公開している。エディタが存在しない。

## Desired Outcome
アプリ内のエディタで TypeScript マクロを書くと、ホスト API・標準ライブラリ・ユーザー定義スキーマに対する補完と型診断が効く。定義ジャンプ、リネーム、フォーマットが動く。単一実行ファイルとしての配布は維持されている。

## Approach
webview 内で Monaco を動かし、`monaco-languageclient` + `vscode-ws-jsonrpc` で言語サーバに接続する。ドキュメントのスキーマから .d.ts を動的生成し、ユーザーのシート構造がそのまま補完に現れるようにする。

**言語サーバの選定は design フェーズの主要な判断であり、未確定とする。** 当初想定していた `deno lsp` の同梱は、技術検証により不適と判明した — Deno の LSP は `deno.json` の存在を前提に Deno 固有のモジュール解決を適用する設計であり、プロジェクト外の素の JS/TS ファイルでは診断・補完が劣化する。マクロ実行に `deno_core` を使うことと、LSP に Deno を使うことの間に技術的な必然性はない。以下 3 案を design で比較評価する。
- **案1**: `typescript-language-server` を最小 Node ランタイムに載せて同梱（TS 本来の言語サービスがそのまま使える）
- **案2**: TypeScript の language service を組込 JS ランタイム上でインプロセス実行（サブプロセス不要だが統合が重い）
- **案3**: Deno バイナリ同梱（約40MB）+ マクロごとに `deno.json` を合成する回避策

## Scope
- **In**: Monaco の組み込みと設定、LSP クライアントの接続とライフサイクル管理、言語サーバの埋め込み・初回展開・起動・再起動、ホスト API と標準ライブラリの .d.ts 提供、ドキュメントスキーマからの .d.ts 動的生成、補完 / 診断 / 定義ジャンプ / リネーム / フォーマット、マクロファイルの管理 UI（一覧・作成・削除）、LSP が使えない環境での縮退動作
- **Out**: マクロの実行（macro-runtime）、標準ライブラリの中身（macro-stdlib）、数式バー（formula-engine）

## Boundary Candidates
- エディタ UI（Monaco のラッパー）と LSP クライアント / トランスポート層の分離
- 言語サーバのプロセス管理（埋め込み・展開・起動・監視）を独立したモジュールにする
- 静的な .d.ts（ホスト API・標準ライブラリ）と 動的生成される .d.ts（ユーザースキーマ）を別経路で供給する

## Out of Boundary
- マクロの実行とサンドボックス（macro-runtime が所有）
- 標準ライブラリの API 設計（macro-stdlib が所有。本スペックはその型定義を消費するだけ）
- セル上の数式入力（formula-engine が所有）

## Upstream / Downstream
- **Upstream**: app-shell（サイドカー基盤）, macro-runtime（ホスト API 型定義）
- **Downstream**: なし（体験の終端）

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: macro-runtime（.d.ts の生成責任の所在を明確にすること）、app-shell（サイドカー基盤の所有権は app-shell 側）

## Constraints
- tsserver はライブラリとして埋め込めない（C-ABI を持たない JS プログラム）。案1・案3 では言語サーバは必ず別プロセスになる。「配布物は 1 ファイル」は守るが「実行時プロセス 1 つ」は達成しない
- `deno lsp` は `deno.json` を前提とし Deno 固有のモジュール解決を強制するため、素の JS/TS マクロには不向き（`denoland/deno` discussion #26221）。採用する場合は回避策のコストを明示すること
- Deno バイナリの実測サイズは約40MB / プラットフォーム（当初見積の130MBは過大）。それでも deno_core の V8 をインプロセスに持つ本体と合わせると単一バイナリとしては重い
- **Tauri v2 の AppImage バンドルは、非自明なサイドカーバイナリを ELF 再リンクで破壊しうる**（`tauri-apps/tauri#11898`、未解決）。V8/ICU を抱えた言語サーババイナリは同じリスク階級。サイドカー同梱を選ぶ場合、AppImage での動作を最優先でプロトタイプ検証すること
- `monaco-languageclient` は WebSocket / インプロセス transport、言語サーバは stdio を話すため、**stdio ⇔ webview のブリッジ（プロキシ）が別途必要**
- `monaco-languageclient` は 10.7.0 / 2026-02 以降リリースが止まっている（約7か月）。採用時にメンテナンス状況を再確認すること
- Monaco は WebKitGTK 上で動作するが、Tauri に描画劣化の既知 issue が複数ある（`#7021` 描画の鈍化、`#13157` 描画の乱れ、`#14286` フォント太さ）。Linux を高リスク対象として早期検証すること

## Incoming Handovers（実装済みスペックからの申し送り。2026-09-18 の棚卸し）

- **ホスト API の `.d.ts` は生成物**: `types/macro-host.d.ts` は手書きしない。生成元は `crates/macro-runtime/src/types.rs` のカタログで、生成器は `cargo run -p macro-runtime --bin generate-macro-types`、ドリフトは `cargo test -p macro-runtime --test macro_host_dts_drift`（出典: `.kiro/specs/macro-runtime/design.md:63`、`crates/macro-runtime/src/types.rs:86-89`）
- **宣言表を変えると補完の入力が変わる**: `HOST_APIS`（`crates/macro-runtime/src/surface/declaration.rs`）の変更は `.d.ts` とドリフト検査に影響する（出典: `.kiro/specs/macro-runtime/design.md:63`）
- **サイドカーを増やすときの義務**: `scripts/stage-sidecars.sh` の macOS 事前署名と同じ扱いを用意し、`SidecarKind` を増やすなら `build.rs` の `KINDS` / `as_str` / `parse` / `ALL` を同時に更新する（忘れると `Unregistered` で起動中止側に倒れる。出典: `.kiro/specs/app-shell/design.md:66`、`.kiro/specs/app-shell/tasks.md:809`）
- **実行基盤の依存を上げたら実測をやり直す**: `deno_core` / `deno_ast` を更新したら打ち切り・上限・変換・値の写像を再実測する（出典: `.kiro/specs/macro-runtime/design.md:64`）

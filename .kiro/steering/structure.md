# Project Structure

> コードはまだ存在しない。本ファイルは既存構造の記述ではなく、**これから書くコードが従うべきパターンの規定**である。`app-shell` の実装で実体が生まれた時点で、逸脱があれば本ファイルを更新する。

## Organization Philosophy

**エンジンと UI の分離を、ディレクトリ構造そのもので強制する。**

Rust ドメインクレートは Tauri に依存してはならない。この規則は「行儀の良さ」ではなく、テスト容易性とスペック分割の前提である。ドメインのテストに GUI 起動が必要になったら、層が壊れている。

依存の向きは常に一方向: **フロントエンド → Tauri コマンド層 → ドメインクレート**。逆流は許容しない。

## Directory Patterns

### Rust ドメインクレート
**Location**: `crates/<domain>/`
**Purpose**: 業務ロジックの実体。Tauri を知らない純粋なライブラリクレート
**規則**: `Cargo.toml` に `tauri` が現れたら誤り。スペック 1 つがおおむねクレート 1 つに対応する
**Example**: `crates/document-format/`、`crates/schema-engine/`、`crates/macro-runtime/`

### Tauri アプリケーション
**Location**: `src-tauri/`
**Purpose**: ウィンドウ、IPC コマンド定義、サイドカー管理、ビルド設定
**規則**: ここに業務ロジックを書かない。コマンド関数はドメインクレートを呼ぶだけの薄いアダプタに留める

### フロントエンド
**Location**: `src/`
**Purpose**: UI。シェル（レイアウト・ルーティング・テーマ）と個別画面を分ける
**規則**: `src/shell/` はアプリ全体の器、`src/features/<feature>/` は画面単位。features 間の直接 import は避ける

### 配信先中立なフロントエンド資産
**Location**: `src/shared/`
**Purpose**: Tauri IPC に依存してはならないコード
**規則**: フォームレンダラのように、デスクトップ内と LAN 配信の Web ページの**両方**で動く必要があるものはここに置く。`window.__TAURI__` への参照が現れたら誤り

### マクロ向け型定義
**Location**: `types/`
**Purpose**: ホスト API と標準マクロライブラリの `.d.ts`
**規則**: 手書きしない。ドメインクレートから生成する。LSP の補完品質はここに直結する

## Naming Conventions

- **Rust クレート / ディレクトリ**: kebab-case（`document-format`）— スペック名と一致させる
- **Rust 型 / トレイト**: PascalCase、**関数 / モジュール**: snake_case
- **TypeScript コンポーネント**: PascalCase（`SchemaTree.tsx`）
- **TypeScript その他**: camelCase
- **スペック名 = クレート名 = feature ディレクトリ名** を一致させる。追跡可能性のため例外を作らない

## Code Organization Principles

### 拡張点は所有者と実装者を分ける
拡張インターフェースを定義するクレートと、それを埋めるクレートは別にする。これは後付けの拡張点が歪むのを防ぐための規則である。

- `schema-engine` が型の拡張インターフェースを**定義**し、`custom-types` が**実装**する
- `data-grid` がセルエディタのレジストリを**定義**し、`custom-types` が**登録**する

### 共有される継ぎ目
以下は複数スペックが触るため、変更時に必ず両側を確認する。

- **決定的シリアライズ**（`document-format` → `version-control`）— 差分の品質はここに全面依存する
- **undo / redo スタック**（`data-grid` ← `formula-engine`、`macro-runtime`）— 最初から共有可能な形で設計する
- **ホスト API の `.d.ts`**（`macro-runtime` → `macro-editor-lsp`）— 生成責任の所在を曖昧にしない
- **フォームレンダラ**（`form-builder` → `form-web-server`）— IPC 非依存を壊さない
- **サイドカー基盤**（`app-shell` → `macro-editor-lsp`）— 所有権は `app-shell` 側

### 性能はドメイン側で守る
10 万行を跨ぐ処理で、行ごとに IPC 境界や JS 呼び出しを越えてはならない。バッチ経路をドメインクレートに用意し、UI 側は結果だけを受け取る。この規則は `schema-engine` の検証、`custom-types` の型チェック、`formula-engine` の再計算のすべてに適用される。

### スペック横断の作業ディレクトリ
仕様・設計・タスクは `.kiro/specs/<feature>/` に置く。プロジェクト方針は `.kiro/steering/` に置く。実装コードから仕様を参照するときは相対リンクを使う。

---
_Document patterns, not file trees. New files following patterns shouldn't require updates_

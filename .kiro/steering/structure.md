# Project Structure

> 本ファイルは規定と記述の両方である。`document-format` は実装済みであり、その構造は以下のパターンに従っている。`src-tauri/` と `src/` はまだ存在せず、`app-shell` の実装で実体が生まれる。実体が規定から逸脱したら、逸脱を直すか本ファイルを更新するかをその場で決める。

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

### Tauri を必要とするスペックは 2 つに割る
Tauri の機能を使うスペックでも、**GUI なしでテストできる部分は必ずドメインクレート側に置く**。`app-shell` がこの形の基準例である。

- `crates/app-shell/` — プロセス監督、整合性検査、設定の原子的書き込み、ショートカット競合検査。Tauri 非依存
- `src-tauri/` — 上記を Tauri のウィンドウ・コマンド・イベントへ接続するアダプタ

判定基準は「その振る舞いを確かめるのに画面が要るか」である。要らないならドメイン側に置く。

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
- **スペック名 = クレート名 = feature ディレクトリ名** を一致させる。追跡可能性のためスペックに対応するクレートで例外を作らない
- **スペックに対応しない補助クレートは存在してよい**（例: 検証専用の最小実行ファイル）。その場合はスペック名と衝突しない名前を付け、`.kiro/specs/` に対応物がないことが名前から分かるようにする

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
- **コマンド登録の根**（`app-shell` → 全 UI スペック）— 登録一覧はコンパイル時に集中して列挙する必要があり、完全な動的登録はできない。**各機能は自分のモジュールに関数を持ち、根は列挙だけを行う**。ここに業務ロジックが漏れ出したら誤り
- **メニュー項目の登録口**（`app-shell` → 全 UI スペック）— 項目そのものは各機能が所有し、`app-shell` は登録口と競合検査だけを持つ

### 性能はドメイン側で守る
10 万行を跨ぐ処理で、行ごとに IPC 境界や JS 呼び出しを越えてはならない。バッチ経路をドメインクレートに用意し、UI 側は結果だけを受け取る。この規則は `schema-engine` の検証、`custom-types` の型チェック、`formula-engine` の再計算のすべてに適用される。

### 生成物は追跡するが手で編集しない
Rust の型定義から生成される TypeScript のように、**生成物をリポジトリに追跡する**場合がある。追跡する理由はドリフト検査の比較対象にするためであり、フロントエンドのビルドを Rust ツールチェーンから独立させるためでもある。生成物には手を入れない。直すのは生成元である。

### 機械検査は `scripts/` に置き CI から呼ぶ
「守られているか目視で確認する」で終わる規則は、いずれ守られなくなる。**不変条件は検査スクリプトとして書き、CI のゲートにする。**

- 依存バージョンの下限、性能予算、生成物のドリフト、権限の逸脱、配信先中立な資産の依存、コアクレートの tauri 非依存
- スクリプトは POSIX sh とし、3 OS のランナーで同じものが走ること

### スペック横断の作業ディレクトリ
仕様・設計・タスクは `.kiro/specs/<feature>/` に置く。プロジェクト方針は `.kiro/steering/` に置く。実装コードから仕様を参照するときは相対リンクを使う。

---
_Document patterns, not file trees. New files following patterns shouldn't require updates_

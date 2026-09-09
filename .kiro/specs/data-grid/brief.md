# Brief: data-grid

## Problem
10 万行の型付きデータを実用的な速度で閲覧・編集できなければ、どれだけ型システムが優れていても道具として使えない。加えて、ネスト可能なスキーマを 2 次元のグリッドにどう写すかという固有の難問がある。

## Current State
document-format がデータを保持し、schema-engine が型と検証を提供する。表示・編集する手段が存在しない。

## Desired Outcome
10 万行のシートを引っかかりなくスクロールでき、セルを直接編集でき、型に応じた適切なエディタ（日付ピッカー、列挙のドロップダウンなど）が出る。検証エラーがその場に表示される。ネストしたスキーマが破綻せず表現される。

## Approach
canvas ベースの仮想化グリッドを採用し、行数によらず定常メモリで動作させる。型 → セルエディタのマッピングを schema-engine の型カタログから導出し、型が増えたときにグリッド側の分岐が増えない構造にする。ネスト構造は列の展開・折りたたみとして扱い、深いネストは専用のインスペクタに逃がす。

**グリッドライブラリの選定は design フェーズで確定する。** 第一候補は Glide Data Grid（MIT、canvas、100万行でも定常メモリ）だが、技術検証により stable 版が 6.0.3 / 2025-02 以降 19 か月更新されておらず、次版は 2024-12 から alpha のまま、最終コミットが 2026-01 であることが判明した。リポジトリは archive されておらずライセンスも MIT で問題はないが、グリッド層全体を賭ける前に AG Grid・canvas 自前実装と並べて評価すること。

## Scope
- **In**: 仮想化グリッドの組み込み、セル選択・範囲選択・コピー / ペースト、型別セルエディタ、ネスト列の展開表現とネスト値のインスペクタ、検証エラーのインライン表示、列幅・並び替え・フィルタ・ソート、編集の undo / redo、大量貼り付け時の型強制
- **Out**: スキーマの編集（schema-editor）、数式の入力と再計算（formula-engine）、シート間の参照ナビゲーション

## Boundary Candidates
- グリッド描画層（Glide Data Grid のラッパー）と セル編集のセマンティクス層の分離
- 型 → エディタのマッピングを独立したレジストリにし、custom-types が後からエディタを登録できるようにする
- undo / redo のコマンドスタックを独立させ、後続スペック（数式・マクロ実行）も同じスタックに乗せられるようにする

## Out of Boundary
- スキーマ定義を変更する UI（schema-editor が所有）
- 数式バーと再計算（formula-engine が所有）
- マクロの実行トリガ（macro-runtime が所有）

## Upstream / Downstream
- **Upstream**: app-shell, schema-engine
- **Downstream**: formula-engine（数式バーと再計算結果の表示をここに載せる）

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: schema-editor（同じシート画面上に同居するため、画面レイアウトの所有権を明確に分けること）

## Constraints
- グリッドライブラリは未確定。Glide Data Grid を第一候補としつつ、メンテナンス停滞（stable 19 か月未更新）を理由に代替と比較評価すること
- **canvas ベースのグリッドは Tauri の Linux/WebKitGTK 描画問題に最も晒されるワークロード**。起動後の白画面、ソフトウェアラスタライズへの無言のフォールバック、DMABUF フレームバッファエラーが既知（`tauri-apps/tauri#5761`、`#13157`）。`WEBKIT_DISABLE_DMABUF_RENDERER=1` 等の回避策込みで早期検証すること
- 10 万行でスクロールが 60fps を維持すること
- undo / redo のスタックは formula-engine と macro-runtime が後から相乗りするため、最初から共有可能な設計にする

# Brief: document-session

## Problem
`document-format` はファイルを開き・保存する能力を持ち、`app-shell` はウィンドウとファイル選択を持つ。しかし**開いた `Document` をメモリ上で保持する持ち主が存在しない**。`app-shell` は設計上「他のドメインクレートに依存しない」と `Cargo.toml` に明記されており、`src-tauri` はウィンドウごとに**ファイルのパス**を持つだけで `Document` を開かない。その結果、`pick_document_file` のコメントが言う「ドキュメント所有者」が宙に浮いている。

この穴は `data-grid` の設計中に発見された。グリッドは「与えられた 1 枚のシート」を表示すると宣言したが、それを与える者がいない。

## Current State
- `document-format`: 開く・保存する・`Document` を組み立てる能力を持つ（実装完了）
- `app-shell`: ウィンドウ、`pick_document_file`、`can_close_window`（終了拒否の問い合わせ）、IPC 境界を持つ（実装完了）。**`document-format` に依存しない**
- `src-tauri/src/window/mod.rs`: `WindowRequest::Document(PathBuf)` — パスだけを持つ
- **欠落**: 開いた `Document` をウィンドウ単位で保持し、変更を受け取り、保存し、未保存を追跡するもの

## Desired Outcome
ウィンドウ 1 つに対して開いているドキュメントが 1 つ対応し、そのドキュメントへの読み取りと変更が単一の経路を通る。未保存の変更があるかが分かり、ウィンドウを閉じる前に問われる。保存はファイル形式の側へ委ねられ、本機能はいつ・何を保存するかだけを決める。

## Approach
ウィンドウラベルをキーとするセッションの表を持ち、各セッションが `Document` の所有権と、その出所（パス）と、未保存の状態を持つ。変更は本機能を通してのみ適用され、変更のたびに未保存が立つ。`app-shell` が既に持つ `can_close_window` の問い合わせに答える側になる。

**保存の形式・決定的出力・履歴には触れない。**開く・保持する・変更を受ける・保存を指示する・閉じてよいか答える、の 5 つに閉じる。

## Scope
- **In**: ウィンドウ単位のドキュメントセッションの生成と破棄、パスからの読み込み、メモリ上の `Document` の所有、変更の適用経路、未保存の追跡、保存の指示、`can_close_window` への応答、新規ドキュメントの作成
- **Out**: ファイル形式そのもの（`document-format`）、型と検証（`schema-engine`）、表示と編集の操作（`data-grid`）、スキーマ編集（`schema-editor`）、変更履歴と自動コミット（`version-control`）、ウィンドウの生成と破棄（`app-shell`）

## Boundary Candidates
- セッションの表（ウィンドウ ↔ ドキュメント）と、セッション 1 つの状態機械の分離
- 変更の適用経路を独立させ、`data-grid` / `schema-editor` / `macro-runtime` が同じ口を通るようにする
- 保存の方針（いつ保存するか）と保存の実行（どう書くか）の分離

## Out of Boundary
- 決定的な出力とファイルの構造（`document-format` が所有）
- 自動コミットと履歴（`version-control` が所有。本機能の保存に相乗りする）
- 取り消し履歴（`data-grid` が所有）

## Upstream / Downstream
- **Upstream**: `document-format`, `app-shell`
- **Downstream**: `data-grid`, `schema-editor`, `macro-runtime`, `version-control`

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: `app-shell`（`pick_document_file` が引き渡す先、`can_close_window` が問う先。どちらも既に実装済みで相手を待っている）

## Constraints
- `app-shell` の「他のドメインクレートに依存しない」制約を壊さないこと。本機能は `app-shell` の**下流**であり、逆流させない
- 10 万行・全件オンメモリが前提。セッションは 1 ウィンドウ 1 ドキュメント
- 変更の適用経路は 10 万行を跨ぐ一括操作を受けられる形にすること（`structure.md`「性能はドメイン側で守る」）
- IPC コマンドを足す場合は `ipc-contract.md` の規約（名前の単一の源・権限の付与・生成物の再生成）に従う

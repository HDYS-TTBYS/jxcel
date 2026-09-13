# Brief: data-grid

## Problem
10 万行の型付きデータを実用的な速度で閲覧・編集できなければ、どれだけ型システムが優れていても道具として使えない。加えて、ネスト可能なスキーマを 2 次元のグリッドにどう写すかという固有の難問がある。

## Current State
上流 3 本はいずれも実装完了しており、本スペックは**既に存在する実体の上に載る**。

- **`document-format`**: File / Sheet / Schema / Row のドキュメントモデルとデータ保持（実装完了）
- **`schema-engine`**: 型と検証（実装完了、2026-09-13）。公開面は `CompiledSchema` / `ColumnIndex` / `TypeKind` / `ValueVariant` / `TypeRegistry` と、`validate_sheet`・`validate_columns`・`validate_write`（`WriteOrigin` / `EditVerdict` / `CollectVerdict`）。実測は 10 万行 × 30 列の全件検証 **255 ms**、一意制約を持つ列 1 本の再検証 **31 ms**
- **`app-shell`**: Tauri v2 の器、IPC 境界、画面登録簿（実装完了）。フロントエンドは React 19 + Vite 7 + TypeScript で、追加の UI ライブラリを**まだ 1 つも持たない**
- **`src/features/smoke/TableSmoke.tsx`**: 200 行 × 8 列の表。3 OS の描画確認のための**最小画面であり実用画面ではない**ことがファイル自身に明記されている。「実用水準へ育てるのは data-grid スペック」と名指しされている

つまり、表示・編集する実用の手段が存在しない。

## Desired Outcome
10 万行のシートを引っかかりなくスクロールでき、セルを直接編集でき、型に応じた適切なエディタ（日付ピッカー、列挙のドロップダウンなど）が出る。検証エラーがその場に表示される。ネストしたスキーマが破綻せず表現される。

## Approach
canvas ベースの仮想化グリッドを採用し、行数によらず定常メモリで動作させる。型 → セルエディタのマッピングを `schema-engine` の `TypeKind` / `TypeRegistry` から導出し、型が増えたときにグリッド側の分岐が増えない構造にする。ネスト構造は列の展開・折りたたみとして扱い、深いネストは専用のインスペクタに逃がす。

検証は**自前で書かない**。1 セルの編集は `validate_write`、貼り付けなどの一括投入は `validate_columns` に委ね、グリッド側は結果の表示に徹する（`structure.md`「性能はドメイン側で守る」）。

**グリッドライブラリの選定は design フェーズで確定する。**`tech.md` の Key Libraries は **Glide Data Grid を第一候補から外している**（stable が 2024-02 で止まり alpha が 2.5 年続いているため）。ライセンス（MIT）と定常メモリの特性に問題はないが、グリッド層全体を賭ける前に AG Grid・canvas 自前実装と並べて評価すること。**評価軸に WebKitGTK 上での実描画を必ず含める**（下記 Constraints）。

## Scope
- **In**: 仮想化グリッドの組み込み、セル選択・範囲選択・コピー / ペースト、型別セルエディタ、ネスト列の展開表現とネスト値のインスペクタ、検証エラーのインライン表示、列幅・並び替え・フィルタ・ソート、編集の undo / redo、大量貼り付け時の型強制、**10 万行をフロントエンドへ運ぶ転送方式**
- **Out**: スキーマの編集（schema-editor）、数式の入力と再計算（formula-engine）、シート間の参照ナビゲーション、検証規則そのものの実装（schema-engine が所有済み）

## Boundary Candidates
- グリッド描画層（選定したライブラリのラッパー）と セル編集のセマンティクス層の分離
- 型 → エディタのマッピングを独立したレジストリにし、`custom-types` が後からエディタを登録できるようにする（`schema-engine` の `TypeRegistry` と対になる UI 側の拡張点）
- undo / redo のコマンドスタックを独立させ、後続スペック（`formula-engine`・`macro-runtime`）も同じスタックに乗せられるようにする
- 行データの転送層（IPC の生バイト経路）を描画層から分離し、グリッドライブラリの差し替えが転送方式に波及しないようにする

## Out of Boundary
- スキーマ定義を変更する UI（`schema-editor` が所有）
- 数式バーと再計算（`formula-engine` が所有）
- マクロの実行トリガ（`macro-runtime` が所有）
- 検証の意味論（`schema-engine` が所有。グリッドは呼ぶだけで判定を持たない）

## Upstream / Downstream
- **Upstream**: `app-shell`（画面登録簿・IPC 境界）、`schema-engine`（型カタログと検証）、`document-format`（行データ）
- **Downstream**: `formula-engine`（数式バーと再計算結果の表示をここに載せる）、`custom-types`（エディタレジストリへ型別エディタを登録する）

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**:
  - `schema-editor`（同じシート画面上に同居するため、画面レイアウトの所有権を明確に分けること。**本スペックが先行するので、境界の定義責任は本スペック側にある**）
  - `app-shell`（`TableSmoke.tsx` は本スペックが実用画面へ育てる対象。ただし当該ファイルは検証専用の経路にあり、**出荷物への到達経路を作らない**規律は維持する）

## Constraints
### 上流が既に固定している規約（`ipc-contract.md` / `structure.md`）
- **境界に 64 ビット整数を出してはならない**（TS の数値精度で壊れる）。**10 万行の行インデックスを扱う本スペックに直接効く制約**であり、行 ID・オフセットの型を design で確定させること
- **大量データは生バイト経路で運ぶ**。JSON を経由すると劣化するため、`app-shell` が `bulk_echo`（要件 4.5）で確立した「引数を全体のバッファとして受け取り、応答を生バイトで返す」形をとる。**行ごとに境界を越える API 形状にしない**
- **コマンドを足したら権限も足す**。ACL が有効なため、どの capability も許可していないコマンドは**ビルド時に静かに削除される**（`src-tauri/permissions/app.toml` と `scripts/check-command-acl.sh`）
- **コマンド名は `crates/app-shell/src/ipc/command_names.rs` の定数**。文字列リテラルで書かない。`src/ipc/bindings.ts` は生成物であり手で編集しない
- **画面の契約 4 つ**（`structure.md`「個別画面の契約」）: `ScreenProps { screenId, navigate }` だけを受け取る／配色は `var(--jxcel-*)` のみ／エラー隔離はシェルの仕事／検証専用画面は出荷物に到達経路を作らない

### 本スペック固有
- グリッドライブラリは未確定。`tech.md` が Glide Data Grid を第一候補から外しているため、AG Grid・canvas 自前実装を含めて選定し直すこと。**フロントエンドは現在 React / Vite 以外の UI 依存を持たないため、ここが最初の重い依存追加になる**
- **canvas ベースのグリッドは Tauri の Linux/WebKitGTK 描画問題に最も晒されるワークロード**（Prototype-First Risk #3）。起動後の白画面、ソフトウェアラスタライズへの無言のフォールバック、DMABUF フレームバッファエラーが既知。`WEBKIT_DISABLE_DMABUF_RENDERER=1` 等の回避策込みで、**ライブラリ選定と同じ段で**検証すること
- 10 万行でスクロールが 60fps を維持すること。予算は要件値で判定し、CI ランナー（2 vCPU）の遅さを理由に緩めない（`verification.md`「ランナーの扱い」）
- undo / redo のスタックは `formula-engine` と `macro-runtime` が後から相乗りするため、最初から共有可能な設計にする。**拡張点の所有者は本スペック**であり、利用者が後から形を変えられない形で定義すること（`structure.md`「拡張点は所有者と実装者を分ける」）
- 検証の往復予算は `schema-engine` の実測（全件 255 ms / 1 列 31 ms）を前提にできる。**グリッド側の予算はこれに上乗せする分として設計すること**

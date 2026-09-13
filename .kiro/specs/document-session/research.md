# Research & Design Decisions: document-session

## Summary

- **Feature**: `document-session`
- **Discovery Scope**: Extension（既存 3 クレート＝`document-format` / `app-shell` / `schema-engine` が実装完了したうえでの新規ドメインクレート。上流の実装事実と継ぎ目を実測で確定した）
- **Key Findings**:
  1. **開いた `Document` の持ち主が存在しない。** `crates/document-format` の被依存は `schema-engine` だけで、`src-tauri` はウィンドウごとに `Option<PathBuf>` を持つだけ（`src-tauri/src/window/mod.rs:608-613`）。`pick_document_file` の引き渡し先は `DocumentHostPort` の既定実装で、何もしない（`src-tauri/src/ports.rs:174-183`）
  2. **`can_close_window` の答えは常に `Allow`。** 拒否を返す実装は `verification-triggers` の検証用宿主にしかない（`src-tauri/src/ports.rs:319-366`）。窓を閉じる前の問いを成立させる相手が本スペックである
  3. **`document-format` の公開面に一括のセル書き換えが無い。** 変更は `&mut Document` のメソッド経由のみで、`set_row_values` は対象行を線形探索するため行数分繰り返すと O(n²)（`crates/document-format/src/parts/rows_codec.rs:679-682` がその旨を明記）。**10 万行を跨ぐ一括の変更は現在の公開面では予算に載らない**
  4. **行の削除と位置指定の挿入は `data-grid` が足す**と既に決まっている（`.kiro/specs/data-grid/design.md:88-100` が「本機能が上流へ加える唯一の変更」と宣言）。本スペックはそこへ手を出さず、**値の書き込みだけ**を足す
  5. **`DocumentHostPort` は 2 メソッドの同期契約**（`may_close` はブロック禁止。`src-tauri/src/ports.rs:142-160`）。セッションの表が数秒のロックを持つなら、未保存はロックの外の原子値で持たねばならない
  6. `Document` は `Clone` も `Default` 以外の複製口も持たず（`crates/document-format/src/model/mod.rs:159-166`）、`tauri` にも `unsafe impl` にも依存しない。**1 実体を保持して貸す**形が自然に成立する

## Research Log

### `document-format` の公開面（上流の実測）

- **Context**: 変更の適用経路と一括の受け口をどこに置くかを決めるため、実装済みの公開面を逐語で確認した
- **Sources Consulted**: `crates/document-format/src/lib.rs`、`src/model/mod.rs`、`src/model/sheet.rs`、`src/parts/rows_codec.rs`、`src/parts/validate.rs`、`benches/large_document.rs`
- **Findings**:
  - ファイル入出力は `DocumentFormatApi` の 4 メソッドのみ: `open`（`lib.rs:240`）/ `save`（`:261`）/ `to_parts`（`:280`）/ `from_parts`（`:293`）。いずれも同期で、失敗は `DocumentError`
  - 変更は集約ルート `Document` の `&mut self` メソッドのみ: `set_sheet_columns` / `add_sheet` / `remove_sheet` / `rename_sheet` / `set_root_schema` / `add_row` / `reorder_rows` / `add_attachment` / `set_row_values`（`model/mod.rs:319-467`）。**変更を表す型（パッチ・差分・操作）は存在しない**
  - `add_row` は末尾追加 O(1)。`set_row_values` は対象行を `iter_mut().find()` で探すため 1 回 O(行数)（`model/sheet.rs`）。**一括の値書き換えの公開口は無い**（`Sheet::extend_rows` は `pub(crate)` で、`parts::from_entries` + `from_parts` が唯一の公開口だが、これは全件の直列化往復を伴う）
  - 行の値数と列数の一致は**保存時の門**（`RowsCodec::encode` が `ValueCountMismatch` を返す。`parts/rows_codec.rs:191-197`）。モデル自身は値を解釈しない
  - 誤り型は 3 層に分かれる: `DocumentError`（形式・構造・入出力。`error.rs:92-169`）、`UnknownSheet` / `UnknownRow` / `ReorderError`（モデル局所。`model/mod.rs:87-129`）、`IdParseError`。**いずれも表示用の文言を持たず、診断に必要な文脈だけを持つ**
  - 予算は開く 3 秒 / 保存 2 秒（`.kiro/specs/document-format/requirements.md:100-101`）。`benches/large_document.rs` が group `large_document` として計測し、`scripts/check-bench-budget.sh:137-138` が判定する
- **Implications**: 一括の値書き換えを 1 つ足す。保存の門（値数＝列数）は触らない。予算の判定器へ本スペックの計測を足す

### `app-shell` の継ぎ目（ウィンドウ・IPC・終了拒否）

- **Context**: 本スペックが実装する相手（`DocumentHost`）と、ウィンドウの生成要求の読み方を確定するため
- **Sources Consulted**: `src-tauri/src/ports.rs`、`src-tauri/src/dialog.rs`、`src-tauri/src/window/{mod.rs,association.rs,close.rs}`、`src-tauri/src/lifecycle.rs`、`crates/app-shell/src/ipc/{mod.rs,command_names.rs,error.rs}`、`src/shell/closeVeto.ts`
- **Findings**:
  - `DocumentHost` は `may_close(&WindowLabel) -> CloseVerdict` と `attach(&WindowLabel, &Path) -> Result<(), AttachError>` の 2 つ。**`may_close` はブロック禁止**（基盤が `prevent_close()` を非ブロッキングに読むため。`ports.rs:142-149`）。差し替えは `DocumentHostPort::install` または構築時の置換（`ports.rs:53-56`）。**現在の宿主は `host()` で取り出せる**（`ports.rs:236-238`）
  - 検証ビルドでは `VerificationDocumentHost` が入り、`JXCEL_VERIFICATION_DENY_CLOSE` のラベルを拒否し、`attach` の `(ウィンドウ, 位置)` を記録する（`ports.rs:268-366`、`lifecycle.rs:687-700`）。**宿主を素朴に置き換えると app-shell の検証（拒否の実測と引き渡しの記録）が壊れる**
  - ウィンドウの生成要求は `WindowRequest::{Empty, Document(PathBuf)}`（`window/mod.rs:97-102`）としてレジストリに保持され、`WindowRegistry::document_of(label)`（`mod.rs:775`）で読める。**中身を読む者はおらず、関連付けの有無としてのみ使われる**（`window/association.rs`）。生成の完了を通知する拡張点は無い
  - `attach` は記録された関連付けを書き換えない（`association.rs:14-26`）
  - コマンドを足す手順は 5 点セット（名前の定数と `COMMAND_NAMES`、自分のモジュールのハンドラ、`command_root!` へ 1 行、`permissions/app.toml` の権限と集合、`bindings.ts` の再生成）。**capability は `app-shell` 集合を参照しているだけ**なので、集合へ足せば届く（`src-tauri/capabilities/default.json`、`scripts/check-command-acl.sh`）
  - 境界型に `ts-rs` の derive を付けてよいのは `crates/app-shell/src/ipc/` の下だけ。`i64` / `u64` を境界へ出さない（`ipc/mod.rs:6-7,62-68`）
  - `IpcError` は隣接タグ付けの判別可能な合併型（`error.rs:30-31`）で、変種は `Settings` / `Sidecar` / `Window` / `Diagnostics`
  - 終了拒否のフロント側は `src/shell/closeVeto.ts` の 1 ファイル。**拒否のときは `console.warn` のみで、利用者への提示が存在しない**（`closeVeto.ts:90-97`）。許可のときだけ `destroy()`
  - **メニューのファイル系項目は「開く…」だけ**（`dialog.rs:122-133`）。保存・新規の項目は存在しない
  - **`request_exit` は掛け金を立てて `app.exit(0)` を呼び、二度と拒否しない**（`lifecycle.rs:2117-2121`）。アプリ全体の終了は窓の `CloseRequested` を通らないため、**未保存のまま終了操作をすると変更は失われる**
- **Implications**: 宿主は**連鎖**（現在の宿主を内側に持つ）で設置する。ウィンドウの生成要求は `WindowRegistry` から**読む側**（本スペック）が解決する。終了前の提示のために `closeVeto.ts` に差し替え口を 1 つ足す。アプリ全体の終了は境界外（残るリスクとして記録）

### 下流の期待（`data-grid` / `version-control`）

- **Context**: 本スペックが公開すべき面と、API の形をどこまで決めてよいかを確定するため
- **Sources Consulted**: `.kiro/specs/data-grid/{design.md,requirements.md,tasks.md,research.md}`、`.kiro/steering/roadmap.md`、`.kiro/specs/version-control/brief.md`
- **Findings**:
  - `data-grid` は「`document-session`: ウィンドウに対応する `Document` への参照と、変更を書き戻す手段。**能力の水準でのみ依存し、API の形を本設計で先に決めない**」と明記し、**本設計の確定を Revalidation Trigger に挙げている**（`design.md:53,67`）
  - `GridSession` の署名は `&Document` / `&mut Document` を直接受ける（`design.md:388-391`）。**「呼び出しごとに参照または可変参照を受け取る」ことが唯一の要求**である
  - 取り消し履歴は**ドキュメント単位**（`design.md:496`）であり、同じ `Document` 実体が保持され続けることを前提にしている
  - `data-grid` の要件 1.7（外部経路の変更を表示へ反映）は `data-grid` 側の `WindowCache.invalidate` に閉じており、**本スペックへ通知 API を要求していない**
  - `version-control` は保存時の自動コミットを Scope に持つが、Upstream に `document-session` を挙げていない（`brief.md:29`）。接点は `document-format` の `to_parts` / `from_parts`（`document-format/design.md:397`）
- **Implications**: 変更の口は `&mut Document` を**閉包で貸す**形にすれば、`data-grid` の署名を変更せずに要件を満たせる。保存を 1 経路に閉じれば `version-control` の相乗り点になる

### 終了前の問いと、まだ無い提示

- **Context**: 「ウィンドウを閉じる前に問われる」という成果をどう成立させるか
- **Findings**: 拒否の理由は境界を通ってフロントエンドへ届く（`CanCloseWindowResponse.verdict`）が、**提示の実装が無く、`console.warn` で捨てられている**。かつ、未保存のままでは利用者が窓を閉じる手段を持たない（破棄の指示が存在しない）
- **Implications**: 提示（3 択）と、選択を反映する操作（保存・破棄）を本スペックが与える。破棄は「未保存の印を落とす」操作であり、以降の答えが変わる

### 検証の実測手段（3 OS）

- **Context**: 「実際に起動して観測した」証拠をどう取るか
- **Sources Consulted**: `scripts/check-bulk-transfer.sh`、`scripts/ci/linux/verify-bulk-transfer.sh`、`.github/workflows/ci.yml`、`.kiro/steering/verification.md`
- **Findings**:
  - 引き金は環境変数 1 系統（`JXCEL_VERIFICATION_*`）。検証用の形は `npx tauri build --no-bundle --features verification-triggers`。**配布物は引き金を読まないことを負の対照として段に持たせる**
  - 検査器は `scripts/*.sh`（POSIX、3 OS 共用）で、記録（`jxcel.log`）の**開始行数より後ろの行だけ**を数える。起動の形跡と引き金の要求行を判定の前提に置く（空振りで緑にしない）
  - 段の実体は `scripts/ci/<os>/` に置き、ワークフローの段は 1 行で呼ぶ
- **Implications**: 読み込み（起動引数）→ 一括の変更（1 回）→ 保存（バイトの変化と決定性）→ 閉じてよいかの答え、を記録行で観測する段を作る。**提示の見え方は Linux の実測に限られ、macOS / Windows では未確認として明示する**

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| 変更の語彙を本スペックが定義 | `Change` 列挙体を定義し `apply(change)` で受ける | 何が変わったかを本スペックが知る | `document-format` の公開面の写しになる。下流の語彙（`data-grid` の `EditCommand` 等）と二重定義。一括のたびに詰め替えが要る | 却下（下流の署名 `&mut Document` とも衝突する） |
| **`&mut Document` を閉包で貸す** | `edit(window, f: FnOnce(&mut Document) -> R)` | 語彙を持たず、下流の署名を変えない。一括は 1 回の閉包で運べる。借用が外へ漏れないので「単一の経路」が構造で成立する | 値が同じでも適用の要求で未保存の印が立つ（no-op の適用でも立つ） | **採用**。印は保守側に倒す。読み取りだけの経路は印を立てない |
| 可変参照を返すガード | `guard(window) -> MutexGuard<Document>` 相当 | 呼び出し側が好きな時に変更できる | 借用が呼び出しを跨いで保持でき、変更を経路に閉じられない。未保存の印を立てる時機が無い | 却下 |
| ウィンドウごとに `Document` を複製して渡す | 呼び出しごとに独立の実体 | ロックが要らない | `Document` は複製口を持たず、10 万行の複製は予算に載らない。1 ウィンドウ 1 ドキュメントの同一性（取り消し履歴の単位）が壊れる | 却下 |

## Design Decisions

### Decision: 変更の適用は借用の閉包で受ける

- **Context**: 変更を単一の経路に閉じつつ、下流（`data-grid` の `&mut Document` を受ける署名、`macro-runtime` の一括書き込み）をそのまま受け止める必要がある
- **Alternatives Considered**:
  1. `Change` 列挙体を本スペックが定義して `apply` で受ける — 語彙が二重定義になり、一括のたびに詰め替えが生じる
  2. `Stack::edit(window, f: &mut dyn FnMut(&mut Document) -> R)` — 借用が閉じるまで外へ出ない（採用）
- **Selected Approach**: `edit` がセッションのロックの内側で閉包へ `&mut Document` を渡し、閉包の戻り値をそのまま返す。適用のあとに**変更の版を 1 進め、未保存の印を立てる**
- **Rationale**: 変更の意味論（何をどう変えるか）は要求側が所有するという境界を保ったまま、単一の経路が成立する。`data-grid` の `GridSession::apply(&mut Document, EditCommand)` はそのまま呼べる
- **Trade-offs**: 値が同じでも適用の要求で未保存の印が立つ（取り消しが空振りした場合など）。印を立てない経路は読み取りだけであり、**取りこぼし（変更したのに印が立たない）は起きない**
- **Follow-up**: 実装では、閉包の間にロックを保持するため**再入を禁止する仕様**を doc に明記する。`may_close` は印を原子値から読むので影響を受けない

### Decision: 一括の値書き換えを `document-format` に 1 つ足す

- **Context**: 要件 3.5 / 3.7（10 万行の一括適用を 1 秒以内、行数に対して二乗で増やさない）は既存の公開面では満たせない
- **Alternatives Considered**:
  1. `set_row_values` を繰り返す — 1 回ごとに対象行を線形探索するため O(行数 × 変更数)
  2. 行の値列ごと差し替える一括（`set_rows_values`）— 変更しない列まで呼び出し側が複製する
  3. **セル単位の一括**（`set_cells(&[(RowId, usize, CellValue)])`）— 索引を 1 度だけ作り、変更しない列に触れない（採用）
- **Selected Approach**: `Document::set_cells` を追加する。行の索引を 1 度構築し、列の添字は `Sheet::columns` の並びに対する位置として検査する。**事前検査を 1 パスで済ませ、どのセルも変更しないか全部変更するかのどちらか**（部分適用なし）。行の現在の値数より後ろへの書き込みは間を値なしで埋める
- **Rationale**: 単一セルの編集（最も頻度が高い）で行全体を複製せずに済み、一括でも O(行数 + 変更数) になる。`document-format` の決定性（書き出しのバイト列）には触れない（変わるのは行の値だけ）
- **Trade-offs**: 公開面が 1 つ増える。`CellWriteError` を 1 つ増やす
- **Follow-up**: `data-grid` が足す `remove_rows` / `insert_row_at` と衝突しないことを実装時に確認する（本スペックは行の構造を変える入口を足さない）

### Decision: セッションは遅延で解決し、宿主は連鎖で設置する

- **Context**: `app-shell` のウィンドウ生成は拡張点を持たず、`DocumentHost` は 2 メソッドしかない。素朴に宿主を置き換えると app-shell の検証用宿主が消える
- **Selected Approach**:
  - ウィンドウの生成要求にあった位置は、**最初のアクセス時に呼び出し元が `WindowRegistry` から読んで渡す**（`resolve(window, requested)`）。セッションは冪等に確定する
  - 設置は `DocumentHostPort::host()` で現在の宿主を取り、`SessionDocumentHost { sessions, inner }` を入れる。`may_close` は未保存なら拒否、そうでなければ `inner` に委ねる。`attach` はセッションへ読み込んだうえで `inner` にも渡す
- **Rationale**: `app-shell` の契約（2 メソッド・同期・非ブロッキング）を変えずに接続できる。検証用の拒否と引き渡しの記録が保存される
- **Trade-offs**: 起動時に指定されたドキュメントは、画面が状態を問い合わせた時点で開く（開いた「こと」の観測は画面の問い合わせが担う）
- **Follow-up**: `attach` の連鎖順序（セッション → inner）を実装のテストで固定する

### Decision: 未保存と変更の版はロックの外の原子値で持つ

- **Context**: `may_close` はブロックしてはならないが、読み込みと一括適用は秒単位でロックを保持する
- **Selected Approach**: セッションごとに `AtomicBool`（未保存）と `AtomicU64`（変更の版）を文書のロックの外に置く。`may_close` は未保存の原子値のみを読む
- **Rationale**: 閉じる要求はイベントループ側から来るため、数秒待たせると UI が固まる。**変更の版**は、一括の適用が 1 回で起きたことを呼び出し回数ではなく版の進み方で示す検証の材料にもなる（`verification.md`「速度を証拠にしない。呼び出しの形で取る」）
- **Trade-offs**: 状態の取得（シート一覧を含む）は文書のロックを要するため、適用中は待つ。**閉じてよいかの答えだけが待たない**

### Decision: 終了前の提示はセッションの操作で解ける形にする

- **Context**: 拒否の理由は届くが提示が無く、未保存のままでは窓を閉じる手段が無い
- **Selected Approach**: `closeVeto.ts` に**拒否の提示の差し替え口**を 1 つ足す（既定は現行の記録のみ＝app-shell の振る舞いを変えない）。本スペックが 3 択（保存して閉じる / 破棄して閉じる / やめる）を提示し、保存と破棄を**コマンド**として与える。破棄は未保存の印を落とす操作であり、以後の答えが変わる
- **Rationale**: 拒否を答えながら閉じる手段が無い状態を解消する。app-shell の検証（拒否の実測）と既定の振る舞いを壊さない
- **Trade-offs**: `src/shell/closeVeto.ts` は app-shell のファイルであり、変更はスペック横断の変更になる（Revalidation Trigger に記載）

### Decision: 保存先の選択は Rust 側で完結させる

- **Context**: 出所を持たないドキュメントを保存するには位置の選択が要る。`ipc-contract.md` は位置を境界へ出さない
- **Selected Approach**: `src-tauri/src/dialog.rs` に保存用の選択を 1 つ足す（Linux は `gtk` の `FileChooserAction::Save`、Windows / macOS は `rfd` の `save_file`。既存の「開く」と同じ親ウィンドウ指定の規律）。選択は保存コマンドの内側（`spawn_blocking`）で行い、**位置はコマンドの応答に含めない**
- **Rationale**: フロントエンドにファイルシステムの経路を与えずに、新規ドキュメントを保存可能にする。既存の `.bak` 退避（形式側の初回保存の規約）をそのまま使える
- **Trade-offs**: 3 OS 分のプラットフォームコードが 1 つ増える（Windows / macOS の実行時は CI でしか確かめられない）
- **Follow-up**: `tauri-plugin-dialog` を依存に入れない（`scripts/check-forbidden-plugins.sh` が機械的に守る）

## Risks & Mitigations

- **アプリ全体の終了では未保存が問われない**（`request_exit` が無条件に終了する。`lifecycle.rs:2117-2121`）— 本スペックの境界外とし、`design.md` の Revalidation Trigger と残るリスクに明記する。解消するには `app-shell` の終了経路を拒否可能にする設計変更が要る（本スペックの 5 つの責務の外）
- **提示の見え方は macOS / Windows で未確認** — Linux で実測し、他の 2 つは未確認として明示する（要件 8.3、`verification.md`）
- **一括のセル書き換えが `document-format` の決定性を壊さないこと** — 変わるのは行の値だけで、書き出しのバイト列は同一内容なら同一であることを既存の決定性テストと本スペックの統合テストの双方で示す
- **`data-grid` の `remove_rows` / `insert_row_at` との衝突** — 本スペックは行の集合を変える入口を足さない。`data-grid` の design 確定時に `set_cells` の形を再確認する（Revalidation Trigger）
- **標本の生成器がクレートごとに写される**（`document-format` / `schema-engine` に続き 3 つ目、`data-grid` でも 4 つ目になる予定）— 補助クレートへ集約する案はスペック横断の変更になるため本スペックでは採らず、リスクとして記録する
- **`may_close` の非ブロッキング性が実装で崩れる** — 未保存を原子値で持つ設計とし、**適用中に問い合わせても待たない**ことを実測（統合テスト＋起動時の検証）で固定する

## References

- `crates/document-format/src/lib.rs` — `DocumentFormatApi`（open / save / to_parts / from_parts）と予算の出所
- `crates/document-format/src/model/mod.rs` — 集約ルート `Document` と変更の公開面
- `crates/document-format/src/parts/rows_codec.rs` — 行の値数と列数の門、O(n²) の注記
- `src-tauri/src/ports.rs` — `DocumentHost` / `DocumentHostPort` / 既定実装 / 検証用宿主
- `src-tauri/src/window/mod.rs` — `WindowRequest` / `WindowRegistry` / 生成と破棄
- `crates/app-shell/src/ipc/{mod.rs,command_names.rs,error.rs}` — 境界型の単一の源・コマンド名・封筒
- `.kiro/steering/ipc-contract.md` — コマンド追加の規約（64 ビット整数を境界へ出さない・権限の付与・生成物）
- `.kiro/steering/verification.md` — 証拠の取り方、引き金の語彙、検査器と CI 段の規約
- `.kiro/specs/data-grid/{design.md,tasks.md}` — 下流の期待と Revalidation Trigger

---

# ギャップ分析（kiro-validate-gap / 2026-09-13）

> **位置づけ**: `document-session` は既に requirements → design → tasks まで生成済みである。したがって本分析は「これから設計する」ための調査ではなく、**設計が前提にしている資産と統合点を実コードで検証し、差分（設計へ持ち戻すべき修正）を返す**ことを目的とする。設計判断そのものは `design.md` にあり、ここでは事実と差分だけを示す。

## 1. 調査範囲と方法

- 読み取り専用。対象は `crates/{document-format,schema-engine,app-shell}`、`src-tauri/`、`src/`、`scripts/`、`.github/workflows/`、`.kiro/steering/`、`.kiro/specs/data-grid/`。
- 検証した主張は 3 種: (a) 上流の公開面が設計の前提どおり存在するか、(b) 拡張点（差し替え口・登録口・権限）が設計の手順どおりに使えるか、(c) **フロントエンドの統合点**（生成物の型・例外分岐・非 React から React へのチャネル・マウント点）。

## 2. 再利用できる資産（実測）

| 資産 | 場所 | 本スペックでの役割 |
|---|---|---|
| ファイル入出力（同期） | `crates/document-format/src/lib.rs:230-296`（`DocumentFormatApi` open/save/to_parts/from_parts） | そのまま呼ぶ。形式を作らない |
| 文書の変更 API（`&mut`、一括口なし） | `crates/document-format/src/model/mod.rs:319-467` | `set_cells` を 1 つ足すだけで足りる |
| 行の値数＝列数の門 | `crates/document-format/src/parts/rows_codec.rs:191-197` | 保存時の整合はこの門が担う（本スペックは判定しない） |
| モデル局所の誤り型の規律 | `src/model/mod.rs:87-129`（`UnknownSheet` / `UnknownRow` / `ReorderError`） | `CellWriteError` を同じ形で足す |
| 宿主の差し替え点 | `src-tauri/src/ports.rs:229-238`（`install` / `host`） | **連鎖**設置（現在の宿主を内側に保存） |
| ウィンドウの生成要求 | `src-tauri/src/window/mod.rs:97-102`, `:775`（`WindowRequest` / `document_of`） | 起動時に指定された位置の解決元 |
| 削除の通知 | `src-tauri/src/window/mod.rs:265-286`（`on_window_event`） | 触らない。隣にウィンドウ単位の購読を足す |
| メニューの登録口 | `src-tauri/src/menu.rs`（`enroll` / `register` / `install`） | 「新規」「保存」を 1 機能として登録 |
| コマンドの 5 点セット | `src-tauri/src/commands/mod.rs:107-119`（`command_root!` に 11 行） | 4 行を足す。名前は `command_names.rs` の定数 |
| 権限の与え方 | `src-tauri/permissions/app.toml`（`[[permission]]` + `[[set]] app-shell`）・`capabilities/default.json`（集合参照） | 権限 4 件と集合への所属 |
| 境界型の置き場 | `crates/app-shell/src/ipc/*`（`ts-rs` derive はここだけ） | `ipc/document.rs` を新設 + `render_bindings` の宣言 |
| 非 React → React のチャネル | `src/shell/theme.ts:378-409`、`src/features/diagnostics/requests.ts:52-88`（**モジュール局所のストア + `useSyncExternalStore`**） | 終了前の提示とセッション状態の配布に**そのまま使える既存の型** |
| 終了拒否の購読 | `src/shell/closeVeto.ts`、`src/main.tsx:34`（`installCloseVeto()`） | 差し替え口を 1 つ足す |
| 検証の型 | `scripts/check-bulk-transfer.sh`（引き金・記録行・開始行数・負の対照）、`scripts/ci/<os>/`、`bench.yml` | 3 OS 観測と予算ゲートの雛形 |
| 検証専用の切り方 | `verification-triggers` feature（`JXCEL_VERIFICATION_*` の一族）・`__JXCEL_VERIFICATION__` | 引き金の語彙に 1 系統を足す |

## 3. 要件 → 資産の対応（ギャップの印: **Missing** / **Unknown** / **Constraint**）

| 要件 | 既存で足りる部分 | ギャップ |
|---|---|---|
| 1.1〜1.5 ウィンドウ↔文書 | ウィンドウの識別子（`WindowLabel`）とレジストリ、`DocumentHostPort` | **Missing**: セッションの表そのもの（保持者不在）。**Constraint**: `DocumentHost` は 2 メソッドの同期契約、`app-shell` は逆依存できない |
| 1.5 破棄 | `WindowRegistry::remove` は app-shell 側に既存 | **Missing**: 本スペックが破棄を知る経路（`WebviewWindow::on_window_event` を実測で確認済み。`tauri-2.11.5/src/webview/webview_window.rs:1524`） |
| 1.6〜1.7 状態の読み出し | 境界型の規律と生成物 | **Missing**: 本スペックの状態を運ぶ境界型（`ipc/document.rs`）。**Constraint**: 位置を境界へ出さない・件数は `u32` |
| 2.1〜2.5 読み込み | `DocumentFormatApi::open`、`pick_document_file` の引き渡し、`AttachError` | **Missing**: 呼び出し元の位置解決（遅延解決）。**Unknown**: 読み込み中の同時要求の粒度（`Busy` を返す設計は `design.md` にあるが実測根拠は無い） |
| 3.1〜3.7 変更の経路 | `Document` の `&mut` メソッド群 | **Missing**: 一括のセル書き換え（`set_cells`）。**Constraint**: 行の削除と位置指定の挿入は `data-grid` が足す（`data-grid/design.md:88-100`） |
| 4.1〜4.6 未保存 | なし（`Document` に dirty は無い） | **Missing**: 未保存と変更の版の保持。**Constraint**: `may_close` はブロック禁止（`ports.rs:142-149`）→ ロック外の原子値 |
| 5.1〜5.8 保存 | `DocumentFormatApi::save`（原子的書き込み・`.bak` 退避・決定性） | **Missing**: 保存先の選択（保存用のダイアログは存在しない。「開く」のみ）。**Constraint**: `tauri-plugin-dialog` / `-fs` を依存に入れない |
| 6.1〜6.7 閉じる前の問い | `can_close_window` の往復と `CloseVerdictDeny` の写像 | **Missing**: 提示（現在は `console.warn` のみ）・破棄の指示・**マウント点** |
| 7.1〜7.4 新規 | `Document::add_sheet` / `set_sheet_columns` / `set_root_schema`（空文書の組み立ては可能） | **Missing**: 新規作成の入口とメニュー項目 |
| 8.1〜8.3 予算と 3 OS | `check-bench-budget.sh`（引数 4 つ）、`bench.yml`（`paths` と `-p`）、`scripts/ci/` | **Missing**: 本スペックのベンチ・判定行・検査器・段。**Constraint**: 計測が無い状態でゲートを先に結線しない |

## 4. 統合の難所（実測に基づく）

1. **一括のセル書き換えが存在しない**（`set_row_values` は対象行を線形探索するため繰り返すと O(行数 × 変更数)。`parts/rows_codec.rs:679-682` がその旨を明記）。10 万行の一括は現状の公開面では予算に載らない。
2. **宿主の連鎖と破棄購読**。`host()` で現在の宿主を取り出せること、`WebviewWindow::on_window_event` がウィンドウ単位で使えることは実測で確認した（`ports.rs:236-238`、`webview_window.rs:1524`）。`on_window_event` は**戻り値を持たない**ため、取得と登録の間の破棄に備えた掃除の経路が要る（`design.md` の WindowDestroyWatch に反映済み）。
3. **未保存・セッション状態を UI へ配るチャネルが無い（新発見）**。メニューからの「開く…」「新規」「保存」は **Rust 側で完結**するため、フロントエンドは結果を知らない。今の設計にはこの通知が無く、画面の「未保存」表示が古いまま残る。既存の `SETTINGS_CHANGED_EVENT` と同じ形（生成物のイベント名 + emit + `useSyncExternalStore`）が使える。
4. **`IpcError` に種別を足すとフロントエンドの網羅分岐が壊れる（新発見）**。`src/ipc/client.ts:102-117` の `describeIpcError` は `kind` を網羅し `assertNever` で新しい種別をコンパイルエラーにする設計であり、`IpcError::Document` を足すと `npm run typecheck` が落ちる。**分岐の追加が必須**（この挙動は「新しい種別に気づける」という既存設計の意図どおり）。
5. **提示のマウント点が無い（新発見）**。`main.tsx` は `<Layout />` を 1 つの React ルートとして描き、シェルは「クロームは領域の外に置く」規律を持つ（`Layout.tsx:32-34`）。提示はシェルのクロームとして `Layout` に載せる必要があり、**`Layout.tsx` は「記述のみの更新」では足りない**（構造の 1 点追加が要る）。2 つ目の React ルートを切る案は、例外隔離と配色の契約から外れるため採らない。

## 5. 実装方針の選択肢

### Option A: 既存クレートへ相乗りする（`app-shell` の中に置く）
- **内容**: `crates/app-shell/` にセッションの表を置き、`document-format` に依存させる
- **長所**: 新しいクレートを作らない。型（`WindowLabel`）が既に同じクレート内にある
- **短所**: `app-shell` は「他のドメインクレートに依存しない」と `Cargo.toml:17-18` に明記しており、`document-format` への依存はこの方針の破棄にあたる。`DocumentHostPort` の所有者と利用者が同一になり、破棄の購読も app-shell のライフサイクルへ混ざる
- **判定**: 方針に反する

### Option B: 新しいクレート（+ 新規のフロントエンド機能）に全部置く
- **内容**: `crates/document-session/` と `src-tauri/src/session/` を新設し、提示も `src/features/document-session/` に独立させる
- **長所**: 責務が明確。Tauri 非依存を機械検査で固定できる（`check-core-deps.sh` が引数なしで `crates/*` を全列挙）
- **短所**: 提示は**シェルクローム**（領域の外）である必要があり、`features/` に置くと器の契約から外れる。`closeVeto.ts` の差し替え口はどのみち要る
- **判定**: コアは B、提示はシェル側という切り分けが必要

### Option C: ハイブリッド（新設 2 つ + シェルへの最小の追加）— **`design.md` が採っている形**
- **内容**: `crates/document-session/`（セッション表・状態機械・変更の経路）+ `src-tauri/src/session/`（宿主の連鎖・4 コマンド・メニュー・破棄購読）+ `src/shell/` への提示（差し替え口・ストア・クローム 1 点）+ `document-format` への 1 メソッド + `app-shell/src/ipc/` への境界型
- **長所**: 依存の向きを保ったまま、実測済みの差し替え点（`DocumentHostPort::host`/`install`、`MenuRegistry`、`token` の 5 点セット）だけを使う
- **短所**: 触るファイル数が 20 前後になる（境界型の生成物を含む）
- **判定**: 妥当。ただし本分析の G1〜G3 を設計へ戻す必要がある

## 6. 工数とリスク（実装単位）

| 単位 | 工数 | リスク | 根拠 |
|---|---|---|---|
| クレートの足場 + セッション表 + 状態機械 | M | Low | `schema-engine` の内部構造と `Cargo.toml` をそのまま写せる |
| 一括のセル書き換え（`document-format`） | S | Low | 1 ファイル。誤り型の規律も既存 |
| 宿主の連鎖 + 破棄購読 | S〜M | Medium | Tauri のリスナ経路とライフタイム。掃除の経路が要る（`on_window_event` は戻り値なし） |
| 4 コマンド + 権限 + 生成物 | M | Low | 5 点セットが確立。ACL 検査が受け皿 |
| 保存先の選択（3 OS） | M | Medium | Linux は `gtk`、他は `rfd`。**Windows / macOS はローカルで閉じられない** |
| 終了前の提示（差し替え口 + ストア + クローム） | M | Medium | **`Layout.tsx` の構造変更が必要**（G1）。非 React → React のチャネルは既存の型を写せる |
| セッション状態の通知（**不足**） | S | Low | `settings_changed` と同じ形（G2） |
| 検証（引き金 + 検査器 + 3 OS 段） | L | Medium | 記録行の語彙設計と 3 OS の実測。負の対照が要る |
| **合計** | **L** | **Medium** | 新しい技術は無い。難所は GUI のチャネルと 3 OS の実測 |

## 7. 設計へ持ち戻すべき差分（ギャップ）

- **G1（必須）**: `src/shell/Layout.tsx` は**記述のみの更新では足りない**。提示をシェルのクロームとして載せる 1 点の構造追加が要る（`design.md` の Modified Files と タスク 4.2 を修正）。
- **G2（必須）**: **セッション状態の変化をフロントエンドへ伝える経路が無い**。メニュー経由の「開く…」「新規」「保存」は Rust で完結するため、イベント名を生成物に足して emit し、フロントエンドが購読して再問い合わせする形が要る（`design.md` の境界型・タスク 3.1/3.4/4.1/4.3 を修正）。
- **G3（必須）**: `IpcError` に種別を足すため、`src/ipc/client.ts` の `describeIpcError` に分岐を足す（足さないと `npm run typecheck` が落ちる）。Modified Files に `src/ipc/client.ts` を追加。
- **G4（確認済み・差分なし）**: 宿主の連鎖・破棄購読・5 点セット・権限の集合・遅延解決・`check-core-deps.sh` の自動対象化は、設計の記述どおりで成立する（実測）。
- **G5（順序）**: `document-format` の 3 ファイルを `data-grid` と共有するため、**本スペックの 1.2 を先に実装する**（`design.md` とタスクの前提に明記済み）。

## 8. Research Needed（設計・実装へ持ち越す未知）

1. **保存先の選択の Windows / macOS の実行時**（`rfd` の保存）。ローカルで閉じられないため、CI の実行と未確認の明示で扱う（`verification.md` の規約どおり）。
2. **提示（3 択）の実画面の見え方**（macOS / Windows）。Linux で実測し、他は未確認として明示する。
3. **`Busy`（読み込み中の同時要求）の粒度**が実利用で問題にならないか。`try_lock` 相当の粒度は実測根拠が無く、実装時の観測で確かめる。
4. **イベントの粒度**（状態変化のたびに 1 回か、種別ごとか）。`settings_changed` は「1 つ」の粒度であり、同じ形で足りる見込みだが、連続操作での再問い合わせ回数は実装時に数えて確かめる（速度ではなく**呼び出しの形**で確認する。`verification.md`）。
5. **`document-format` の標本生成器の重複**（本スペックで 3 つ目、`data-grid` で 4 つ目）。補助クレートへの集約はスペック横断の変更になるため、本スペックでは行わずリスクとして保持する。

## 9. 結論

要件 → 資産の対応は **Missing が 4 群（セッションの保持者・一括のセル書き換え・保存先の選択・提示）**、**Constraint が 5 件（逆依存の禁止・ブロック禁止・位置を境界へ出さない・行の構造は data-grid・検証専用は feature で切る）**であり、いずれも既存のパターンの組み合わせで埋まる（新しい技術は無い）。`design.md` が採った **Option C（ハイブリッド）** は妥当だが、**G1〜G3 の 3 件は設計へ戻して修正しないと実装が行き詰まる**（提示のマウント点・状態の通知・`IpcError` の網羅分岐）。総合の工数は **L**、リスクは **Medium**（GUI のチャネルと 3 OS の実測が支配的）。

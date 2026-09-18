# Research & Design Decisions

## Summary

- **Feature**: `macro-runtime` — ユーザーが書いた TypeScript / JavaScript をドキュメントに対して実行する埋め込みランタイム
- **Discovery Scope**: New Feature（新規ドメインクレート + 適応層 + フロントエンドの実行面。上流 4 クレートは実装済み）
- **Key Findings**:
  1. **実行基盤は成立済み**。V8 isolate を専用 OS スレッドに置き、multi-thread 側（Tauri のコマンド層と同じ形）から mpsc/oneshot 越しに呼ぶ形が実測で動く（`tech.md`「Known Risks」1。スパイクの値: 生成 5.5 ms / 常駐 +24 MB / 実行ファイル +68 MB / op の panic は捕捉しても生存）。**製品での実測（2026-09-18。タスク 5.4）**: isolate の生成を含む実行 1 回 **4〜8 ms** / 常駐 **26〜29 MB**（`VmHWM` の増分） / 実行ファイル **+83,816,136 B（約 79.9 MiB）** / 打ち切りは時間の上限 30,000 ms に対して **30,006〜30,007 ms**、メモリの上限 512 MB に対して **210〜220 ms**
  2. **打ち切りとメモリ上限は V8 の API で足りる**。`terminate_execution`（別スレッドからは `thread_safe_handle`）＋`cancel_terminate_execution` で再利用可。メモリは `create_params` の `heap_limits` ＋ `add_near_heap_limit_callback` の中で terminate に合流させ、**OOM を捕捉可能な例外へ変換**する（callback を付けない既定はプロセスの abort）
  3. **変更の取り消しは既存の口が用意してある**。`data-grid` の `UndoStack::push` が唯一の登録口であり、`UndoLabel::MacroRun` は**このスペックのために予約済み**、`HistoryCommand::Composite` が複数部分の逆命令を 1 対にまとめる
  4. **マクロの保存先は存在しない**。`document-format` の `EntryName` は 6 形（Marker / Manifest / Document / Schema / Rows / Attachment）であり、マクロのパートを足すのは**上流（document-format）の変更**
  5. **TS 変換は `deno_ast` で足り、`deno_core` と衝突しない**（`deno_core` 0.412 は swc に依存しない。スパイクの `Cargo.lock` に swc は 0 件）
  6. **`.d.ts` は ts-rs で型を、Rust 側の宣言表で関数面を生成する**（op のメタデータは型情報を持たない）

## Research Log

### 実行基盤の成立性（Roadmap「Prototype-First Risks」1）

- **Context**: `deno_core` を Tauri バイナリへ埋め込む組み合わせに前例が無く、失敗すればアーキテクチャ全体が変わる。design の前に確かめる必要があった
- **Sources Consulted**: 使い捨てスパイク `target/spike-macro-runtime/`（実測）、`deno_core` 0.412 の vendored ソース
- **Findings**:
  - 専用 OS スレッドが `JsRuntime` を所有し、その上で current-thread の tokio ランタイムを 1 つ回す形が動く（`LocalSet` を挟むと deferred op の完了が届かない）
  - `JsRuntime::resolve` は**使えない**（イベントループが回り切った後でも返らない）。`execute_script` → `run_event_loop(Default::default())` → Promise の状態を読む、が正しい
  - 非同期 op は `#[op2]` を `async fn` に付ける形（`(lazy)` / `(deferred)` は完了が届かない）。状態は型として受け取る
  - op は JS から `Deno.core.ops.<name>()` で呼ぶ。`op_panic` は組込であり、同名の登録は isolate の生成に失敗する
  - 実測（release）: 生成 5.5 ms / 後始末 0.3 ms / 常駐 +24 MB / 10 万件の畳み込み 4.1 ms / 8 本の同時呼び出し 0.1 ms / 実行ファイル +68 MB
- **Implications**: `crates/macro-runtime` は「専用スレッドの actor」を唯一の実行の実体として持つ。isolate は**実行ごとに作る**（下の決定 1）

### 実行の打ち切りとメモリ上限

- **Context**: 要件 6 は時間 30 秒・メモリ 512 MB の既定で打ち切り、打ち切り後もアプリが使えることを求める
- **Sources Consulted**: `deno_core-0.412.0/runtime/tests/misc.rs`（357-386 / 566-590 / 1493-1532）、`runtime/jsruntime.rs`（601 `create_params`・2174 `add_near_heap_limit_callback`）
- **Findings**:
  - 別スレッドからの打ち切りは `v8_isolate().thread_safe_handle()` の clone をタイマースレッドへ渡し `terminate_execution()`。例外は `CoreError`（`"Uncaught Error: execution terminated"`）
  - **打ち切った後も `cancel_terminate_execution()` で同じ isolate を使い続けられる**（テストが固定している）
  - `heap_limits` は `RuntimeOptions::create_params` から V8 へそのまま渡る。上限超過は**例外ではなく near-heap-limit コールバック**として現れ、callback 内で `terminate_execution()` を呼ぶと例外になって戻る。callback が無いと V8 がプロセスを abort させる
- **Implications**: 時間とメモリの 2 つの打ち切りは**同じ 1 つの terminate 経路に合流**する（`limits.rs` の 1 箇所）。打ち切りの種類（時間 / メモリ）は callback 側で記録して outcome に載せる

### 変更の適用と取り消しの継ぎ目

- **Context**: 要件 5（画面と同じ経路）と要件 7（実行 1 回 = 取り消し 1 回）を既存の構造のどこへ繋ぐか
- **Sources Consulted**: `crates/document-session/src/lib.rs`（`DocumentSessionsApi::edit`）、`src-tauri/src/commands/grid.rs`（`SheetEntry` が `UndoStack` を所有）、`crates/data-grid/src/history/mod.rs`（55-61, 146-161, 232）、`crates/data-grid/src/edit/mod.rs`（92-96, 489-493）
- **Findings**:
  - ドキュメントの変更の唯一の経路は `document-session` の `edit`（閉包へ `&mut Document` を貸し、同じ Guard の内側で未保存と版を記録する）
  - 取り消し履歴は**ドメインクレートではなくウィンドウの保持**（`src-tauri` の `SheetEntry`）が `data-grid::history::UndoStack` として所有する（10.2 が所有者を降ろした）
  - `UndoLabel::MacroRun` は**本スペックが積むために予約済み**。`HistoryCommand::Composite` は「1 つの操作が複数の書き込みを連ねる」ための既存の形（貼り付けの逆命令が同じ形を使う）
- **Implications**: エンジンは**変更の集約（ChangeSet）を返すだけ**にし、適用と履歴の 1 対化は適応層が行う（エンジンが `data-grid` を依存に持たない）。詳細は決定 2 と 3

### マクロの保存先（document-format の拡張点）

- **Context**: 要件 1 は「ドキュメントの一部として保存する」と定めた（利用者の決定）。上流のどこを広げるか
- **Sources Consulted**: `crates/document-format/src/entry_name.rs`（102-124 の 6 形と許可リスト）、`crates/document-format/src/parts/document_parts.rs`、`crates/document-format/src/model/schema_part.rs`
- **Findings**:
  - エントリ名は**閉じた 6 形**であり、新しいパートは `EntryName` の 1 形を足すことになる（`parse` の文法は `LAYOUT_FORMS` が決める）
  - パートの追加は**符号化の決定性**（同一内容 → 同一バイト列）とマニフェスト索引に影響する
- **Implications**: `document-format` に**形（名前と種別とソースの並び）だけ**を足し、**意味（ソースの解釈・能力宣言）は `macro-runtime` が持つ**（決定 4）。上流の再検証トリガーになる

### TypeScript の変換経路と型定義の生成

- **Context**: 要件 3（TS を実行、構文誤りは位置つきで提示）と要件 10（型定義の公開）
- **Sources Consulted**: `deno_ast` 0.53.3（`transpiling` feature / `ParseParams` / `ParsedSource::transpile`）、`deno_core` の `ModuleLoader`（`modules/loaders.rs:61`）と `SourceMapper`（`source_map.rs:214`）、`crates/app-shell/src/bin/generate-bindings.rs` と `crates/app-shell/src/ipc/mod.rs`（`render_bindings`）
- **Findings**:
  - 変換は `deno_ast` の transpile で足りる（型注釈の除去に加え enum / namespace / デコレータを swc の TS 変換が扱う。**型検査は対象外**であり要件 3.2 と一致する）
  - 変換は **`ModuleLoader::load` の中で行う**のが正しい（`RuntimeOptions::extension_transpiler` は拡張専用）。変換結果へ `sourceMappingURL` を付けると、`deno_core` の `SourceMapper` が**例外のフレームを原位置へ写す**
  - `deno_core` は swc に依存しないため、`deno_ast` の swc と同居しても `swc_common` の二重化が起きない（実測）
  - `.d.ts` は **ts-rs で型を、Rust 側の宣言表で関数面**を生成する（op のメタデータは型情報を持たない）。`src/ipc/bindings.ts` の生成・ドリフト検査と同じ運用にできる
  - 値の往復は `serde_v8` の `to_v8` / `from_v8`。10 万行は `#[buffer]` / `#[arraybuffer]` の借用でコピーを 1 回に抑えられる
- **Implications**: 変換はエンジンの `transpile.rs` に閉じ、型定義は `types/` へ生成する（`structure.md`「マクロ向け型定義」に従う）

### ホスト API の公開面と能力

- **Context**: 要件 4（読み）・5（書き）・8（能力）・10（型）を 1 つの整合した面にする
- **Sources Consulted**: スパイクの op 実装、`crates/app-shell/src/ipc/command_names.rs`（名前の単一の源という規約）、`structure.md`「拡張点は所有者と実装者を分ける」
- **Findings**: 3 つの用途（ops の登録・能力の門・型定義の生成）が同じ API の一覧を必要とする。別々に持つと必ずずれる（既存の `COMMAND_NAMES` が同じ問題を 1 つの源で解いている）
- **Implications**: **宣言表（`surface/declaration.rs`）を唯一の源**にし、3 つがそれだけを読む（決定 5）

### 検証の置き場

- **Context**: 要件の多くは「実起動でしか確かめられない」（GUI の中の操作、失敗の提示、打ち切り）
- **Sources Consulted**: `scripts/check-grid-observation.sh`（診断の記録を 3 OS 共通の読み口にした前例）、`verification.md`、`structure.md`「検証コードの置き場」、`.github/workflows/ci.yml`（test ジョブ 1 つ・3 OS マトリクス）
- **Findings**: 検証専用の入口は環境変数 1 系統（`JXCEL_VERIFICATION_*`）＋ 非既定 feature ＋ Vite の `define` の 3 点で閉じる。判定は POSIX の検査器が行い、CI は既存ジョブへ段を 1 行足す
- **Implications**: 実行の観測は**診断の記録**（`macro_run` の 1 行）を 3 OS の読み口にする（`data-grid` の 9.2 と同じ形）

### 実装後の実測 — 実行基盤と実行ファイルの大きさ（tasks.md 5.4。2026-09-18）

- **Context**: スパイクの値（生成 5.5 ms / 常駐 +24 MB / 実行ファイル +68 MB）は**使い捨ての実行ファイル**の値であり、製品（`deno_core` 0.412 + `deno_ast` 0.53.3 + `ts-rs` + `src-tauri` のアダプタ）の値ではない。5.4 は製品の実測で置き換える
- **Sources Consulted**: 検証専用の例 `crates/macro-runtime/examples/measure-runtime.rs`（5.4 が追加。**製品の公開面だけ**を使う — 計器を `src/` へ足すと「計器を入れた製品」を測ることになる。`examples/` はアプリのバイナリにも配布物にも入らない）、Linux の `/proc/self/status`（`VmRSS` = その時点の常駐 / `VmHWM` = ピーク）、`target/release/jxcel`
- **計測の条件**: この開発機（AMD Ryzen AI MAX+ 395 / 32 コア / Linux / **release**）。上限は**製品の既定**（`engine/outcome.rs` の `Limits::default` = 30 秒 / 512 MB）である。打ち切りのマクロはホスト API を 1 つも呼ばない（`HostPort` の実装は呼ばれたら理由つきで拒む — op の panic はプロセスを abort させるため）
- **コマンド**: `cargo build -p macro-runtime --example measure-runtime --release` の後、`./target/release/examples/measure-runtime <isolate|time|memory>`（本機の `cargo` は podman のシムであり、ビルドは容器の中で走る。**実行はホストで行った**）
- **Findings（実行基盤。`isolate` の出力。2 回の実行の要約）**:
  - `計測: 実行基盤の起動（isolate は作らない） = 0 ms`（`MacroRuntime::new`。**isolate は最初の実行まで作らない** = 決定 1 が実装でもそのままである）
  - `計測: 実行 1 回目（isolate の生成を含む） = 7 ms`（1 回目の別の試行では 8 ms）/ `計測: 実行 2 回目 … = 4 ms` / `計測: 実行 3 回目 … = 4 ms` — **毎回同じ桁であり、JIT の温まりを持ち越していない**（実行ごとに isolate を作る）
  - `計測: 実行の前後の VmRSS = 6708 kB（約 6 MB） → 36184 kB（約 35 MB）` / `計測: VmHWM …（増分 29476 kB（約 28 MB））` — 別の試行では増分 **26,404 kB（約 25.8 MB）**。**常駐は 26〜29 MB** である。実行の後に `VmRSS` が下限へ戻らないのは**確保したページがプロセス（アロケータ）に残る**ためであり、isolate が生きていることを意味しない（isolate は実行ごとに所有スレッドの上で落ちる）
  - `計測: 実行基盤の後始末 = 0 ms`（専用スレッドの終了。スパイクの 0.3 ms と同じ桁）
- **Findings（実行ファイルの大きさ）**: 3 つのビルドを**同じソース**から作り、`stat -c %s` で測った（`cargo build -p jxcel --release`。既定 feature）

| ビルド | 大きさ | 何を除いたか |
|---|---|---|
| 結線前 | **23,233,312 B** | リポジトリの写しから **macro の結線そのもの**を除いた（`main.rs` の `mod macro_apply` / `mod macro_host`、`commands/mod.rs` の `macro_commands` と登録 4 本、`lifecycle.rs` の `commands::macro_install`、`commands/macro.rs` / `macro_apply.rs` / `macro_host.rs`） |
| 結線あり・実行基盤なし | **27,385,736 B** | 結線はそのままに、`crates/macro-runtime/src/engine/` の 4 ファイル（`actor` / `isolate` / `limits` / `transpile`）を同じ公開面の空の実装に置き換えた（V8 / `deno_core` / `deno_ast` が実行ファイルへ入らない） |
| 結線後（製品） | **107,049,448 B** | — |

  - **増分 = 83,816,136 B（約 79.9 MiB / 83.8 MB）**（= 107,049,448 − 23,233,312）
  - **内訳**: **実行基盤（V8 + `deno_core` + `deno_ast`）が 79,663,712 B**（= 107,049,448 − 27,385,736）、**結線（アダプタ + `macro-runtime` の Rust コード + `ts-rs`）が 4,152,424 B**（= 27,385,736 − 23,233,312）
  - 結線前・結線あり・結線後の 3 つは**同じソース**から作った（写しの作成は 2026-09-18 11:57、`src-tauri/src` と `crates/macro-runtime/src` の最終更新は 11:47 であり、その後に変わっていないことを `find -newermt` で確認した）。結線前・結線ありのビルドは**共有の `target/` を使った**ため、レジストリの依存は再コンパイルしていない
  - **実行基盤が入っていないことの確認**: 結線前のバイナリは `strings -a` で `Deno.core` が 0 件、`nm -C` で `v8::` のシンボルが 0 件。結線後のバイナリは `v8::` のシンボルが 66,123 件（**V8 は静的に入っている**）
  - **スパイクの +68 MB より大きいのは正しい** — 製品は `deno_ast`（swc の TS 変換）と `ts-rs`、アダプタを足で持つ。**単一実行ファイルの要求は満たされる**（静的に含む）
- **Implications**: 起動時に isolate を作らない判断（決定 1）は製品でもそのままである。常駐はマクロを 1 度も実行しなければ 0 であり、実行の間だけ 26〜29 MB 増える。実行ファイルの増分は「V8 + `deno_core` + `deno_ast`」がほぼ全部であり、**アプリ側のコードは 5%** である（層を増やしても配布物の大きさには効かない）

### 実装後の実測 — 予算（tasks.md 5.3 / 5.4。**判定器を読み口にする**）

- **Context**: 5.3 がベンチ（`crates/macro-runtime/benches/bulk.rs`）と判定器（`scripts/check-bench-budget.sh`）を置いており、予算の判定はそこが実体である。5.4 は**同じ値を二重に書かない** — 記録は判定器の出力を引く
- **Sources Consulted**: `cargo bench -p macro-runtime --bench bulk`（criterion の平均の点推定値）、`sh scripts/check-bench-budget.sh`（要件値との比較。`tech.md`「ベンチマーク」「性能予算の判定」）
- **Findings**（2026-09-18 の計測。criterion の出力そのまま）:
  - `bulk/read_100k_rows_x_30_columns   time: [1.9022 s 1.9166 s 1.9343 s]`（10 サンプル・外れ値 1 件）
  - `bulk/rewrite_10k_rows   time: [348.69 ms 350.31 ms 352.21 ms]`
- **判定器の出力**（この 2 行が判定の実体であり、記録はこれを読む）:
  - `check-bench-budget: OK  read (10万行×30列, マクロの一括の読み+集計) 1.917s <= 10.000s`
  - `check-bench-budget: OK  rewrite (1万行×30列, マクロの書き換え) 0.351s <= 5.000s`
  - 判定器は 8 本すべてを読み、この実行では 8 本とも OK で exit 0 だった（他の 6 本は他スペックの計測であり、本作業では走らせていない）
- **Implications**: 要件 11.1（10 秒）と 11.2（5 秒）に対して 5 倍以上の余裕がある。**計測をワークフローから落とす変更は判定器が exit 2 で検出する**（fail-closed。5.3 の受け入れ）

### 実装後の実測 — 打ち切り（時間の精度とメモリ。tasks.md 5.4）

- **Context**: 5.1 / 5.2 の実起動の観測は**時間の上限**までしか見ていない（標本の `標本の打ち切り` は終わらない繰り返しであり、メモリの上限には当たらない）。**メモリの打ち切りは製品の既定（512 MB）で観測されていなかった** — 5.4 が測る（時間の上限の実測は 5.2 の実起動の観測にもある）
- **Findings（時間。`./target/release/examples/measure-runtime time` を 2 回）**: 上限 **30,000 ms** に対して実測 **30,007 ms / 30,006 ms**（**差 +6〜+7 ms**）。種類は `time`、理由は `Error: execution terminated`（V8 の打ち切りは位置を持たない）、打ち切りの後の実行は同じ実行基盤で **4 ms** で成功する（要件 6.4 の復帰）。実行中の `VmRSS` の山は 41〜42 MB
  - `計測: 打ち切り（時間） = 30007 ms / aborted(time) / 打ち切りまでの所要 = 30007 ms / Error: execution terminated`
  - `計測: 上限と実測の差 = 7 ms（上限 30000 ms / 実測 30007 ms）`
- **Findings（メモリ。`memory` を 2 回）**: 上限 **536,870,912 B（512 MB）** に対して **210 ms / 220 ms** で打ち切られ、種類は `memory` である。実行中の `VmRSS` の山は **502,528 kB / 495,968 kB（約 484〜490 MB）**、`VmHWM` の増分は **494,324 kB / 496,172 kB** — **上限の値が実際の確保として現れている**（`heap_limits` の上限が効いていることの裏づけ）。打ち切りの後の実行は 4 ms で成功する
  - メモリの確保は**文字列の連結ではなく数（倍精度配列）の保持**にした — 文字列には V8 の最大文字列長（約 512 MB）があり、上限の種類を取り違える危険があるためである
  - **標本（`make-macro-document.rs`）へマクロを足さなかった理由**: 標本のマクロの並びは 5.2 の検査器が**閉じた一覧**として要求しており（`scripts/check-macro-observation.sh` の `EXPECTED_NAMES` と `EXPECTED_LISTED=5`）、足すと同じ作業で検査器を直すことになる。加えてメモリの打ち切りは**実起動の観測**（同じ起動で続けて別のマクロを走らせる形）を要さない — 計測は専用の例が持つ
  - `計測: メモリの上限 = 536870912 B（既定 = 512 MB）`
  - `計測: 打ち切り（メモリ） = 210 ms / aborted(memory) / 打ち切りまでの所要 = 210 ms / Error: execution terminated`
  - `計測: 打ち切り（メモリ） の実行中の VmRSS の山 = 502528 kB（約 490 MB）`
  - `計測: VmHWM（確保したピーク） = 6848 kB（約 6 MB） → 501172 kB（約 489 MB）（増分 494324 kB（約 482 MB））`
- **精度の読み方**: 時間の打ち切りは**上限 + 1 桁 ms** で止まる（タイマースレッドが期限で `terminate_execution` を呼び、実行中の V8 が次の検査点で止まる）。これは**実行の所要に対する上限の保証**である。上限が 0 の端（実行が始まる前か後かという競合）は別の話であり `engine/limits.rs` の doc にある
- **Implications**: 要件 6.1 / 6.2 の打ち切りは**製品の既定で実際に起きる**。種類（時間 / メモリ）は結果から読め（`LimitKind`。提示は文言ではなくこの種類を使う）、どちらの後も次の実行が通る（同じ actor で 4 ms）。メモリの打ち切りは**プロセスの abort にならない**（near-heap-limit callback が terminate へ合流する）ことを実測で確かめた

### 実装で確定したこと（設計の決定 1〜6 の帰結。tasks.md 5.4）

| 決定 | 実装での所在 | 実装で確定した細部 |
|---|---|---|
| 1. 実行ごとに isolate を作る | `engine/actor.rs`（専用 OS スレッド + current-thread の tokio ランタイム。要求は mpsc + oneshot で直列化）と `engine/isolate.rs`（生成と破棄） | `MacroRuntime::new` は isolate を作らない（遅延）。生成は 1 箇所であり、isolate はスレッドを越えない。実行の入口は**同期**であり、非同期の実行文脈からは `spawn_blocking` の上で呼ぶ（アダプタ）。実測: 実行 1 回 4〜8 ms / 常駐 26〜29 MB |
| 2. 変更は実行の間に集約し、終わってから 1 回で適用する | `host/changes.rs`（適用しない `ChangeSet` / 適用の順序 追加 → 値 → 削除 / 種別ごとの件数）、`host/overlay.rs`（自分の書き込みを読む）、`src-tauri/src/macro_apply.rs`（`apply_macro_changes`） | 適用は `document-session::edit` の**閉包 1 回**の中で行い、履歴へは `UndoLabel::MacroRun` の**1 対**を積む（`macro_apply.rs:412`）。適用の前にシートを照合し、食い違えば何も書かない。実行が失敗・打ち切りなら**何も適用しない**ので巻き戻しの機構を持たない |
| 3. エンジンは `data-grid` に依存しない | `crates/macro-runtime/Cargo.toml` の依存方針 + 1.1 の `cargo tree` 検査 | 変更の型は `document-format` の値だけで 4 種（セルの書き込み・行の追加・削除・複製）。エンジンは適用しない — 適用と履歴はアダプタの仕事である |
| 4. マクロの形は document-format、意味は macro-runtime | `document-format` の `EntryName` に `macros.json` を 1 形（1.2）＋ `crates/document-format/src/parts/` | **形式バージョンは進めない**（`entry_name.rs` の審査の記録）。上流は名前・種別・ソースの並びと往復・決定性だけを知り、解釈（宣言の解析・構文・能力）は `source/capability.rs` / `engine/transpile.rs` / `surface/gate.rs` が持つ |
| 5. 宣言表を唯一の源にする | `surface/declaration.rs` の `HOST_APIS` | 門（`surface/gate.rs`）・型定義の生成（`types.rs`。`.d.ts`）・op の登録が**同じ表**を読む。宣言と実装の乖離は検査で落ちる（2.1 / 3.3） |
| 6. 能力はソース先頭の宣言で決める | `source/capability.rs`（解析）と `surface/gate.rs`（呼び出しの前の門） | 綴りの揺れは正規化し、未知の能力は**名前と位置つき**で拒む。値が無ければ要する API は呼べない（門が先に拒む）。提示は実行の前（4.4） |

- **設計から動かした 2 点（`host/mod.rs` の doc）**: `HostPort` に `overlay()` を置かない（重ね合わせは `read_rows` の内側の道具であり、`&self` で変更を集める以上 `&ChangeSet` を外へ返せない）、`emit()` を置かない（出力の型は `engine` にあり層の鎖を越える。出力は `RunOutcome::Ran.output` で戻す）。**どちらも実装で確定した形である**（設計の Service Interface の綴りから動いた理由がコードの側に書いてある）

### 実装中に実測した落とし穴（tasks.md 5.4。4.1 / 4.2 / 5.3 の記録から）

- **op の中で `reqwest::blocking` の `Client` を実行文脈で落とすと、プロセスごと abort する**（4.1 の実測。**最初の実装で `host.netFetch` がプロセスを落とした**）。`reqwest::blocking` は内部に専用スレッドと tokio のランタイムを作って落とすため、エンジンの実行文脈（tokio の `block_on`）の中で落とすと tokio が panic し、**V8 の op は `extern "C"` の境界を越えるので巻き戻せず SIGABRT** になる（`tech.md`「Known Risks」1 の「op は panic してはならない」と同じ経路）。**回避**: 取得を専用スレッドへ出し、作って落とす場所を実行文脈の外へ置く（`src-tauri/src/macro_host.rs` の `net_fetch`。取得のスレッドが panic しても理由つきの失敗として実行へ戻る）
- **TLS は rustls + ring を選んだ**（`src-tauri/Cargo.toml` の依存方針）。`reqwest` の既定 `rustls` feature は **aws-lc-rs** を引き、**CMake と nasm** を要する（ビルドの前提を増やさない）。`native-tls` は 3 OS の既定 TLS に委ねる分だけ前提は軽いが、**Linux のビルドと配布物に OpenSSL の開発ファイルと共有ライブラリを要求する**（本アプリは追加のライブラリを要さずに起動することを要件にしている）。`rustls-no-provider` は暗号器を同梱しないので、`net.fetch` の入口で `rustls::crypto::ring` を 1 度だけ据える（`install_crypto_provider`）
- **上流の性能問題（所有は `data-grid` / `document-format`。製品から見える）**: 10 万行の変更の**取り消しが 35.2 秒**（同じ条件の適用は 259.9 ms）。原因は `data-grid` の `write_material_rows` が行ごとに `Document::set_row_values` を呼び、`Sheet::set_row_values` が行を**線形探索**すること（費用は 行数 × 書いた行数）。`data-grid` の命令語彙には行の値を一括で置換する口が `RestoreValues` しかなく、写像側（`src-tauri/src/macro_apply.rs`）で回避する道が無い。**したがってマクロの変更を大きく取り消すと 35 秒待たされる**。修正は上流で行う（`macro-runtime` の側では直さない）
- **棄却した設計の副産物（製品の主張として測ったものではない）**: 5.3 が最初に書いたロックの規律の検査（マクロが `host.sheets()` を密に回して行数の変化を待つ形）では、実行中の `documents.edit` が **5 秒以上待たされた** — 読みの連打が文書のロックを占有するためである。これは**棄却した設計**の性質であり、製品の値として測ったものではない（記録には残すが、**製品の主張には使わない**。使うなら別途計測が要る）
- **`MacroRuntime::is_running()` は要求が受け取られた時点（isolate の生成より前）に真になる**（5.3 の実測）。「実行が始まったのを見てから文書を変える」形の合図では、マクロの最初の読みより後になることがある — 待ち合わせを観測に基づけるときの前提である
- **`cargo bench` の落とし穴（5.3 の実測）**: `[[bin]] generate-macro-types` に `bench = false` を付けないと `cargo bench -p macro-runtime` が bin の libtest ハーネスで `Unrecognized option: save-baseline` で落ちる（`[[bench]] harness = false` と対で要る）
- **測れなかったこと（未確認として残す）**: macOS / Windows の**実行ファイルの大きさと常駐**（`/proc/self/status` は Linux のみ）、3 OS での打ち切りの精度、配布物（AppImage / deb）の大きさの増分。3 OS の CI の段が観測するのは**実行の観測**（5.2）までであり、大きさと常駐は測っていない。上流の取り消し（35.2 秒）の修正後の値も本スペックの範囲外である

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|---|---|---|---|---|
| 専用スレッドの actor（採用） | isolate を専用 OS スレッドが所有し、要求を mpsc/oneshot で直列化 | V8 の current-thread 制約を満たす。失敗の隔離がスレッド境界そのもの。打ち切りを外から入れられる | 呼び出しごとに往復の費用（実測 0.1〜0.2 ms/件、直列化） | スパイクで実測済み |
| 別プロセス（Deno をサイドカーに） | 既存の tauri+deno プロジェクトの形 | 隔離が最も強い | 単一実行ファイルの要件に反する。IPC の往復が増える。既存のサイドカー基盤を流用しても配布物が 2 つになる | 却下（`product.md`「配布物は 1 ファイル」） |
| 埋め込み（isolate を都度作る / 使い回す） | 同一スレッド・同一 isolate を共有 | 起動費用が最小 | V8 のスレッド親和性に反する。打ち切りの影響が UI に及ぶ | 却下（スパイクの前提） |

## Design Decisions

### Decision 1: 実行ごとに isolate を作る（使い回さない）

- **Context**: 1 つの isolate を使い回すと `globalThis` が実行間で残る。`deno_core` 0.412 の realm 作成 API は `pub(crate)` であり、公開された口が無い
- **Alternatives Considered**:
  1. isolate を使い回し、実行ごとに新しい realm を作る — 公開 API が無い（`runtime/jsrealm.rs:271` が `pub(crate)`）
  2. isolate を使い回してグローバルの共有を許容する — マクロ A が置いたグローバルがマクロ B に見える（説明できない振る舞い）
  3. 実行ごとに isolate を作る — 生成 5.5 ms・常駐 +24 MB を毎回払う
- **Selected Approach**: 実行ごとに isolate を作り、終わったら**所有スレッドの上で落とす**
- **Rationale**: 5.5 ms は打ち切りの既定 30 秒に対して無視できる。グローバルを共有しないという説明可能な意味論が得られ、要件 2 の「失敗が他へ漏れない」も isolate の破棄で確実になる
- **Trade-offs**: JIT の温まりを持ち越せない（10 万行の集計で 4.1 ms の実測があり、予算 10 秒に対して十分）。実行ごとに 24 MB の常駐が増減する
- **Follow-up**: 実装時に「連続 2 回の実行でグローバルが共有されないこと」をテストで固定する

### Decision 2: 変更は実行の間に集約し、終わってから 1 回で適用する

- **Context**: 要件 6.3（打ち切り時は実行前のまま）と要件 7（実行 1 回 = 取り消し 1 回）を最小の機構で満たす必要がある
- **Alternatives Considered**:
  1. 実行の間に逐次適用し、失敗したら取り消す — 失敗が画面に見えうる（一瞬書き換わって戻る）。巻き戻しの機構を別に持つ
  2. 変更を集約し、成功したときだけ 1 回で適用する（採用） — 失敗・打ち切りでは**何も適用しない**ので巻き戻しが要らない
  3. 適用を呼び出し側（マクロの作者）に委ねる — 要件 5 の「画面と同じ経路」を保証できない
- **Selected Approach**: マクロの書き込みは**未適用の変更集合**（`ChangeSet`）へ積む。実行が成功で終わったときだけ、適応層が `document-session::edit` の 1 回の閉包の中で適用し、`UndoLabel::MacroRun` の 1 対を積む
- **Rationale**: トランザクション境界が「実行の終わり」という 1 点になり、要件 6.3 / 7.1 / 7.3 が同じ 1 つの性質から出る
- **Trade-offs**: マクロが**自分の書き込みを読む**には重ね合わせが要る（下の不変条件）。行の追加は**適用まで識別子を持たない**ため、追加した行を後から読むことはできない（`insert` は値を同時に受け取る形にして、この制約を実用上問題にしない）
- **Follow-up**: 「同じセルへ 2 回書いたら最後の値が残る」「書いたセルを読むと書いた値が返る」をテストで固定する

### Decision 3: エンジンは `data-grid` に依存しない（適用は適応層の仕事）

- **Context**: 変更の適用先（`document-session`）と履歴の所有者（`src-tauri` の保持 + `data-grid::history`）が、エンジンから見て別の層にある
- **Alternatives Considered**:
  1. エンジンが `data-grid` の `EditCommand` を組み立てて `document-session` へ渡す — エンジンが画面のクレートに依存する（層の向きが崩れる）
  2. エンジンは**自前の変更の型**を返し、適応層が `EditCommand` へ写して適用する（採用） — 依存の向きが保たれる
- **Selected Approach**: `macro-runtime` の `ChangeSet` / `Change` は `document-format` の値の型だけを使う。`src-tauri` の適応層が `EditCommand` へ写し、1 回の `edit` と 1 つの `UndoEntry` にまとめる
- **Rationale**: `structure.md`「ドメインクレートは Tauri を知らない」「エンジンと UI の分離」。同じ理由で打ち切り・能力・出力の扱いもエンジン側に閉じる
- **Trade-offs**: 写しの 1 段が増える（型の数は限られる: セルの書き込み・行の追加・削除・複製の 4 種）
- **Follow-up**: 写しの網羅性を「変更の全種を 1 回ずつ適用する」テストで固定する

### Decision 4: マクロの形は document-format、意味は macro-runtime

- **Context**: 要件 1 の保存は上流（document-format）の変更を要する
- **Alternatives Considered**:
  1. `document-format` がマクロの意味（種別・能力）まで知る — 上流が下流の語彙を持つ
  2. `document-format` は**名前・種別・ソースの並び**という形だけを持ち、解釈は `macro-runtime` が行う（採用）
  3. 添付（`attachments/`）にテキストを入れる — 添付は内容 адрес付けされたバイナリであり、名前や種別を持てない
- **Selected Approach**: `EntryName` に 1 形（`macros.json`）を足し、中身はマクロの記録の並び（名前・種別・ソース）とする。解釈（ソースの検証・能力宣言の読み取り・一覧の提示）はエンジンが行う
- **Rationale**: 上流は形を、下流は意味を持つ（`schema-engine` ⇔ `document-types` の分け方と同じ）
- **Trade-offs**: 上流の決定性・フィクスチャの検査を追随させる必要がある（下の Revalidation Triggers）
- **Follow-up**: 保存 → 開き直しで**ソースがバイト単位で一致する**ことを `document-format` と `macro-runtime` の両側で固定する

### Decision 5: ホスト API の宣言表を唯一の源にする

- **Context**: 要件 4・5・8・10 は同じ API の一覧を 3 つの用途（ops の登録・能力の門・型定義）で必要とする
- **Alternatives Considered**:
  1. 用途ごとに一覧を持つ — ずれる（`COMMAND_NAMES` が同じ問題を 1 つの源で解いた前例がある）
  2. 宣言表 1 つを全員が読む（採用）
- **Selected Approach**: `surface/declaration.rs` に「名前・引数・戻り値の型・必要とする能力」を並べ、ops の登録・能力の判定・`.d.ts` の生成がそこだけを読む
- **Rationale**: `structure.md`「拡張点は所有者と実装者を分ける」と同じ規律。**宣言と実装の乖離をテストで固定できる**（宣言の全項目に実装があることを機械検査する）
- **Trade-offs**: 宣言の追加が実装の追加を強制する（意図した性質である）
- **Follow-up**: 「宣言にある API はすべて呼べる」「宣言に無い API は呼べない」の両方向をテストで固定する

### Decision 6: 能力はマクロのソース先頭の宣言で決める

- **Context**: 要件 8 は「マクロごとの宣言 + 実行時の提示」を求める（利用者の決定）
- **Alternatives Considered**:
  1. 実行のたびに確認の面 — 繰り返し実行のたびに手が止まる
  2. アプリ設定で一括 — マクロごとの差が見えない
  3. ソース先頭の宣言行（`// @grant file.read` 等）＋実行前の提示（採用）
- **Selected Approach**: 宣言が無ければ**その能力を使う API は呼べない**（門は宣言表の能力と突き合わせる）。宣言は一覧として利用者へ提示する
- **Rationale**: ソースと一緒にドキュメントへ保存されるので、渡した相手にも同じ宣言が付いてくる（要件 1 と整合する）
- **Trade-offs**: 宣言行の綴りを覚える必要がある（エディタスペックが補完で支える）
- **Follow-up**: 宣言の綴りの揺れ（大文字小文字・空白）の扱いをテストで固定する

## Risks & Mitigations

- **上流（document-format）の変更が本スペックに先行される** — マクロのパートが無いと要件 1 が満たせない。対策: タスクの最初期に「パートを足す」を置き、`document-format` の決定性検査とフィクスチャを同じ作業で追随させる（Revalidation Triggers に明記）
- **実行ファイルが 83.8 MB 増える（製品の実測。2026-09-18。スパイクの値は 68 MB）**（V8 + `deno_core` + `deno_ast` + アダプタ。内訳は上の「実装後の実測 — 実行基盤と実行ファイルの大きさ」） — 対策: 実測を `tech.md`「Known Risks」1 に記録し、配布物の検証段で大きさを観測する。単一実行ファイルの要求は満たされる（静的に含む）
- **10 万行の往復でコピーが増える** — 対策: 範囲の読みは `#[buffer]` の借用で渡し、予算の判定を実起動の観測で行う（要件 11。**実測は判定器を読む**: 読み + 集計 1.917 s ≤ 10 s / 1 万行の書き換え 0.351 s ≤ 5 s）
- **打ち切りの取りこぼし**（マクロがホスト API の中で待ち続ける場合） — 対策: terminate はエンジンのループと op の待ちの双方に効くことをテストで固定する（`misc.rs:1493-1532` が前例）
- **能力の門が「付け忘れ」で素通りする** — 対策: 宣言表の全 API に能力の欄を必須とし、**能力を持たない API を明示的な `None` として書く**（既定を「能力不要」にしない）

## References

- `.kiro/steering/tech.md`「Known Risks」1 — 実行基盤の確定形と実測（2026-09-17 のスパイクと 2026-09-18 の製品での実測）
- `crates/macro-runtime/examples/measure-runtime.rs` — 実行基盤と上限の実測（tasks.md 5.4。**製品の公開面だけ**を使う検証専用の例。出荷物には入らない）
- `target/release/jxcel` の大きさ — `cargo build -p jxcel --release` の成果物（結線前の 23,233,312 B と、実行基盤だけを外した 27,385,736 B は、同じソースの写しで測った比較である。手順は「実装後の実測 — 実行基盤と実行ファイルの大きさ」）
- `scripts/check-bench-budget.sh` — 予算の読み口（`bulk/read_100k_rows_x_30_columns` / `bulk/rewrite_10k_rows`）
- `src-tauri/src/macro_host.rs`（`net_fetch` の専用スレッドと `install_crypto_provider`）/ `src-tauri/Cargo.toml`（TLS と依存方針）— 実装で踏んだ落とし穴の回避
- `target/spike-macro-runtime/` — 使い捨てスパイク（専用スレッド + current-thread ランタイム + op の実例）
- `deno_core` 0.412（vendored）: `runtime/jsruntime.rs`（601 `create_params`・1420 `v8_isolate`・2174 `add_near_heap_limit_callback`）、`runtime/tests/misc.rs`（357-386・566-590）、`modules/loaders.rs`（61）、`error.rs`（495 `JsError`・608 `JsStackFrame`）
- `deno_ast` 0.53.3（`transpiling` feature）
- `crates/data-grid/src/history/mod.rs`（55-61・146-161・232）— `UndoLabel::MacroRun` と `push` の唯一の口
- `crates/document-format/src/entry_name.rs`（102-124）— エントリ名の閉じた 6 形
- `crates/document-session/src/change.rs` — 変更の唯一の経路
- `.kiro/steering/structure.md`「ドメインクレートの内部構造」「セッションの所有の規約」「マクロ向け型定義」

# Research & Design Decisions

## Summary

- **Feature**: `schema-engine`
- **Discovery Scope**: New Feature（既存コードベース上の新規ドメインクレート。上流の `document-format` は実装完了済み）
- **Key Findings**:
  - 行は**位置づけされた値の列**（`Row::values()` が `Sheet::columns()` と同じ順序）。検証はハッシュ探索なしの添字走査で回せ、これが 1 秒予算の前提になる
  - `Decimal` は文字列のまま逐語で往復する契約であるため、**10 進数のライブラリを検証経路に入れてはならない**（どれも出力時に何かを正規化する）
  - 検証器は閉じた列挙体の直接マッチにする。`Box<dyn Fn>` はこの形（配列を舐めながらディスパッチする）で実測 3〜5 倍遅い
  - 予算 330 ナノ秒／セルに対して単一スレッドで 5 倍程度の余裕がある見込みであり、初版で `rayon` を入れる必要はない
  - `jiff` は曖昧な日時入力を**既定で拒否**する。要件 7.3（環境で解釈が変わる入力を変換しない）と道具の既定が一致している

## Research Log

### 既存クレートの規約（`document-format` / `app-shell`）

- **Context**: 新しいドメインクレートが従うべき規約を、推測ではなく実物から取る必要があった
- **Sources Consulted**: `crates/*/Cargo.toml`、`crates/document-format/src/{error,ids,value,model,parts}`、`crates/document-format/tests/`、`benches/`、`scripts/check-*.sh`、`.github/workflows/{ci,bench}.yml`、`.kiro/steering/{structure,verification,ipc-contract}.md`
- **Findings**:
  - `[workspace.dependencies]` は存在せず、依存は**クレートごとに版を直書き**する。各 `Cargo.toml` は冒頭に `# 依存方針` のコメント塊を置き、`tauri` を推移的にも入れないこと・兄弟クレートの可否・feature 固定の理由を書く
  - 既にワークスペースにある: `serde` 1.0、`serde_json` 1.0（`float_roundtrip`, `raw_value`）、`thiserror` 2.0、`ulid` 3.0、`blake3` 1.8、`tempfile` 3.27、`criterion` 0.8（dev）
  - 誤り型は `thiserror` の構造体変種で、**文脈のみを持ち表示文を持たない**。`Display` は診断用であり、利用者向けの文言は呼び出し元が組み立てる
  - 識別子は `macro_rules!` で生成する ULID newtype。型エイリアスではなく別の構造体であり、取り違えはコンパイル誤りになる
  - モジュールの `//!` は長く規範的で、要件 ID・タスク ID・design の節名を引用し、**却下した代替案の理由まで書く**。層ごとの一方向依存の鎖を各 `mod.rs` の冒頭に再掲する
  - テストは**モジュール内 `#[cfg(test)]` が主**（`document-format` で 233 対 155）。`tests/common/mod.rs` を共有し、`proptest` / `insta` は使わない。決定性はコミット済みのゴールデンバイトで確かめる。テスト名は英語の文
  - ベンチは `criterion` + `harness = false` + `[lib] bench = false`。予算判定は `scripts/check-bench-budget.sh` が `estimates.json` を読む。終了コードは `0` 適合 / `1` 逸脱 / `2` 入力が使えない
  - **CI に `cargo clippy` も `cargo fmt --check` も無い**。整形と lint はローカルの `qlty` 規約に依存している
- **Implications**:
  - `scripts/check-bench-budget.sh` の相対パスは `large_document/...` に固定されており、`.github/workflows/bench.yml` も `paths` と `-p document-format` が固定である。**新しい予算を CI のゲートにするには両方を拡張する必要がある**（実装タスクに明示する）
  - 汎用の `scripts/check-core-deps.sh <pkg>` は `app-shell` にしか掛かっていない。`schema-engine` の段を足さないと `tauri` 非依存が検査されない
  - ワークスペース根の `Cargo.toml` の注記は「ドメインクレートは他のドメインクレートに依存しない」と読めるが、`crates/document-format/Cargo.toml` が「自身は依存グラフの根であり下流（`schema-engine` を名指し）は依存してよい」と書いている。**根の注記を是正する**
  - `ipc-contract.md` により `ts_rs` の導出は `crates/app-shell/src/ipc/` 配下に限られる。本クレートの型は `TS` を導出しない。境界を越える DTO は app-shell 側が定義する。境界で 64 ビット整数を使わない規約もあるため、`i64` を持つ型はそのまま渡せない

### 10 進数の表現

- **Context**: 要件 2.3 が有効桁数と小数点以下の桁数の宣言を求める一方、`document-format` は `Decimal` を文字列のまま逐語で往復させる契約を持つ
- **Sources Consulted**: `rust_decimal` 1.43.0、`bigdecimal` 0.4.10、`fastnum` 0.7.5、`dec` 0.4.11 の各ドキュメントと 10 進クレート比較記事
- **Findings**:
  - いずれのクレートも出力時に何かを正規化する（先頭の 0、`+` 符号、`1e3` → `1000`、指数形）。値を通した時点で逐語往復が壊れる
  - `rust_decimal` は 96 ビット仮数で 28〜29 有効桁が上限。`NUMERIC(38,10)` を表現できない
  - `dec` は libdecnumber（C）へのバインディングであり、C を依存ツリーに入れない方針と衝突する
  - 桁数の検査は符号・整数部・小数点・小数部・指数を 1 パスで走査するだけで足りる
- **Implications**: 検証経路にクレートを入れない。順序比較は宣言された `scale` に揃えた正準形の上で行う。任意精度演算が必要になった時点で `rust_decimal` を検討する（precision 28 桁までに制限することが条件）

### 日時の解釈

- **Context**: 要件 7.3 が「地域設定または実行環境によって解釈が変わりうる入力は変換しない」と定める
- **Sources Consulted**: `jiff` 0.2.35 の設計文書と比較文書、`chrono` 0.4.45、`time` 0.3.55、RUSTSEC-2020-0159、RUSTSEC-2026-0009
- **Findings**:
  - `jiff` の型は宣言にそのまま対応する: `civil::Date`（日付のみ）、`civil::DateTime`（civil）、`Timestamp`（オフセット付き）、`Zoned`（IANA 注釈付き）。`civil::Date` は `Z` 付き文字列を**拒否**する
  - 厳密さが既定で有効: オフセットと注釈が矛盾すれば推測せず誤りにし、曖昧な現地時刻は明示設定で扱い、`2024-06-16T10.5` のような小数成分を拒否する。地域設定に依存する解析経路が存在しない
  - `Zoned` を使うと Windows で `jiff-tzdb-platform` がタイムゾーンデータベースを実行ファイルに埋め込む
  - `chrono` の `localtime_r` 不健全性（RUSTSEC-2020-0159）は 0.4.20 以降で解決済み。ただし解析が寛容で `NaiveDateTime` の意味論が事故りやすい
  - `time` は RUSTSEC-2026-0009（RFC 2822 解析でのスタック枯渇、CVE-2026-25727）を 0.3.47 で修正済み。利用者の入力を解析する本用途では無視できない前例
- **Implications**: `jiff` を採る。tzdb を同梱する feature は有効にせず、`Zoned` は初版で扱わない。宣言の `offset` は `forbidden` / `required` の 2 値とする。3 値目を足す余地は文字列の列挙として残す

### 高スループットな検証

- **Context**: 10 万行 × 30 列＝300 万セルを 1 秒以内。1 セルあたり約 330 ナノ秒
- **Sources Consulted**: `jsonschema` 0.56.0、`boon` 0.6.1、`valico` 4.0.0、`rayon` 1.12.0、Rust のディスパッチ比較記事
- **Findings**:
  - 「一度組み立てて何度も使う」形は `jsonschema` が実装しており正しい方向だが、本機能は JSON Schema を採用しないため語彙が合わず、`fancy-regex`（後方参照あり）を含む 20 個近い依存を連れてくる。`boon` は半休止、`valico` は無保守
  - 配列を舐めながらディスパッチする形では、`Box<dyn Fn>` は列挙体の直接マッチに対して実測 3〜5 倍遅い。列挙体はデータをインラインに持て、分岐予測が効く
  - 既に解析済みの値の列挙体に対する判定は 1 セルあたり 1 桁〜数十ナノ秒。費用は 10 進数と日時の文字列解析、書式の照合に集中する。平均 50 ナノ秒前後なら単一スレッドで 0.2 秒前後
  - `rayon` 1.12.0 は純 Rust で、C も copyleft も持ち込まない
- **Implications**: 自前のコンパイル済み検証器を書く。列挙体の直接マッチにする。初版で `rayon` を入れない。入れる場合は `par_chunks` で行の塊に分け、**Tauri の webview と競合しないよう専用のスレッドプールを作る**

### 書式（パターン）制約

- **Context**: 要件 4.5 が文字列の書式制約を求める。パターンは利用者が書く
- **Sources Consulted**: `regex` 1.13.1 のドキュメントと既定値、RUSTSEC-2022-0013
- **Findings**:
  - `regex` は有限オートマトンで後戻りを行わず、**構成上 O(m·n) が保証される**。後方参照と先読みは非対応で、使うとコンパイル時に落ちる
  - 既定の上限: `size_limit` 10 MiB、`dfa_size_limit` 2 MiB、`nest_limit` 250
  - `fancy-regex` は後戻り式であり ReDoS の余地がある
- **Implications**: `regex` を採る。利用者のパターンには生文字列長の上限を課し、`size_limit` を 64〜256 KiB、`nest_limit` を 32 程度へ引き下げる。コンパイルはスキーマのコンパイル時に一度だけ行う

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| 宣言 → コンパイル → 実行（採用） | 宣言を一度だけ計画へ落とし、実行は計画の上で回す | 10 万行の走査から宣言のたどり直しが消える。拡張点を計画の 1 変種に閉じ込められる | 計画とスキーマの同期を呼び出し元が保つ必要がある | `jsonschema` の `Validator::build` と同じ形 |
| 宣言を毎回たどる解釈実行 | コンパイル段を持たず、値ごとに宣言を辿る | 実装が単純。同期の問題が無い | 予算に入らない。入れ子と `$ref` の解決が毎セル走る | 却下 |
| 導出マクロ式の検証（`garde` / `validator`） | Rust の構造体に属性を付ける | 宣言的で読みやすい | **実行時に宣言されるスキーマを表現できない**。形が根本的に違う | 却下 |
| JSON Schema を採用 | 語彙ごと既存規格に乗る | 規格の恩恵、既存実装 | 要件の型カタログ（10 進数の桁、シート間参照、添付、ANY）が表現できない。依存が重い | 却下 |

## Design Decisions

### Decision: 違反を保持せず、常に導出する

- **Context**: 要件 6 は編集経路で違反する値を保持できることを求める。どこに「不正である」という情報を置くか
- **Alternatives Considered**:
  1. ドキュメントに不正フラグや検証結果を書き込む
  2. 違反は保持せず、宣言と値から毎回導出する
- **Selected Approach**: 2。`CellValue` の 8 変種は任意の入力を表現でき、`document-format` はそれを逐語で往復させる。数値列に打たれた `"abc"` は `Text("abc")` として保存・復元され、検証のたびに違反として報告される
- **Rationale**: 保存を妨げない（6.5）・位置をいつでも取得できる（6.4）・入力どおりに保たれる（6.6）が、すべて同じ仕組みで成立する。ドキュメント形式に新しい格納先を足さずに済み、上流のスペックを変更しない
- **Trade-offs**: ファイルを開いたあとに全件検証が必要になる。だからこそ 1 秒の予算が要る
- **Follow-up**: 開いた直後の検証の引き金は呼び出し元（`data-grid` / app-shell）が持つ。その結線を下流スペックで確認する

### Decision: 行を跨ぐ性質は一括経路にだけ置く

- **Context**: 一意性（4.7）と参照の実在（9.2）は 1 セルだけを見ても判定できない。1 セルの判定は 16 ミリ秒以内（10.2）
- **Alternatives Considered**:
  1. 呼び出し元が持ち回る増分索引を公開し、書き込みのたびに更新する
  2. 跨る性質を一括経路にだけ置き、書き込み判定は値に閉じた性質だけを見る
- **Selected Approach**: 2。編集直後に重複を知りたい呼び出し元は、その列だけを `validate_columns` で舐め直す（1 列 10 万件で 10 ミリ秒程度）
- **Rationale**: 書き込みのたびに 10 万行分の索引を持ち回る必要がなくなり、16 ミリ秒の予算が素直に満たせる。索引の一貫性を保つ責任も生まれない
- **Trade-offs**: 重複の検出が 1 セルの判定より 1 拍遅れる
- **Follow-up**: `data-grid` が 16 ミリ秒未満の重複提示を要求するなら、増分索引を後から足す。公開インターフェースは変えずに入れられる

### Decision: 解決できない型を列単位の使用不能に落とす

- **Context**: 要件 11.7 は未登録の拡張型でスキーマを破棄しないことを求める。一方 3.5 / 3.6 は実在しない `$ref` と値が存在しえない循環を宣言ごと拒否することを求める
- **Alternatives Considered**:
  1. 解決できないものはすべて宣言ごと拒否する
  2. すべて列単位に落とす
  3. 構造の壊れと解決の失敗を分ける
- **Selected Approach**: 3。実在しない `$ref` と循環は**宣言ごと拒否**（宣言が構造的に壊れており、`document-format` も読み込み時に同じ判定をする）。未知の `kind` と未登録の拡張型は**その列だけ使用不能**（登録や将来の版で解決しうる）
- **Rationale**: 「壊れている」と「まだ解決できない」は別の事態である。前者を通すと壊れた宣言が保存され、後者を拒否すると古い版で開けないファイルができる
- **Trade-offs**: 2 つの扱いを実装と文書で区別し続ける必要がある
- **Follow-up**: 使用不能な列の値をどう画面に出すかは `data-grid` と `schema-editor` の判断

### Decision: 拡張型のトレイトに一括メソッドを置く

- **Context**: `roadmap.md` が `schema-engine ⇔ custom-types` について「10 万行のバッチ検証経路を両者で整合させること」と名指ししている。拡張型の実体は JS である
- **Alternatives Considered**:
  1. 1 件用のメソッドだけを定義し、繰り返しは呼ぶ側の責任にする
  2. 一括メソッドを必須にする
  3. 一括メソッドを置き、既定実装を 1 件用の繰り返しにする
- **Selected Approach**: 3。境界を越える実装は上書きして列ごとに 1 回にまとめられ、単純な拡張型は 1 件用だけ書けばよい
- **Rationale**: 10 万行でセルごとに JS 境界を越えると予算に収まらない。一方で拡張点の敷居を上げると、単純な型（郵便番号の書式など）を書くのが重くなる
- **Trade-offs**: 既定実装と上書き実装が同じ結果を返すことを、実装側が保つ必要がある
- **Follow-up**: `custom-types` の design で上書きを必須とする。本スペックのベンチに拡張型を含む列を入れる

### Decision: スキーマ変更の可謬な処理をすべて計画側に寄せる

- **Context**: 要件 8.6 は途中失敗で半端に適用された状態を残さないことを求める
- **Alternatives Considered**:
  1. 適用中に失敗したら巻き戻す
  2. 可謬な処理を計画側に寄せ、適用を失敗しえないものにする
- **Selected Approach**: 2。`plan_change` が新しい列名の配列と全行分の新しい値を計算しきり、`apply_change` は書き戻すだけにする
- **Rationale**: 巻き戻しの経路は書かれた回数が少なく、壊れていても気づきにくい。**半端な状態を構造上作れなくする**ほうが確実である
- **Trade-offs**: 計画が全行分の新しい値を保持するため記憶域を使う（10 万行分）
- **Follow-up**: 計画は作成時のシートの行識別子の並びと列名のダイジェストを持ち、適用時に照合する（陳腐化した計画は `StalePlan`）

### Decision: `evolution` という名前を使う

- **Context**: `document-format` の `migration` は**形式バージョン**（`major.minor`）の移行であり、本機能の「スキーマ変更時のデータ移行」とは別物である
- **Selected Approach**: 本機能の層を `evolution` と呼ぶ
- **Rationale**: 同じワークスペースに `migration` が 2 つあると、実装でもレビューでも取り違える。要件の作成時点でこの衝突を記録してある
- **Trade-offs**: 日本語では両方「移行」と訳せてしまうため、文書側でも区別を明記し続ける必要がある

### Decision: 宣言テキストの正準形は本機能が責任を持つ

- **Context**: 要件 1.4 は同一内容が同一テキストになることを求める
- **Findings**: `document-format` はスキーマ・ペイロードを `RawJson` として**逐語のバイト列で保持し、再シリアライズしない**。エンベロープのキー整列規則はペイロードの内側に及ばない
- **Selected Approach**: 本機能が出力するテキストはキーを宣言順に固定し（`name` → `type` → `required` → `unique` → `default` → `description`）、余分な空白を含めない
- **Rationale**: 正準形の責任がどこにもない状態を放置すると、同じスキーマが別のバイト列になって git 差分が濁る。ペイロードは本機能のものなので、差分の読みやすさを優先した順序を選べる
- **Trade-offs**: エンベロープ側の整列規則と意図的に異なる。両方の規則を文書に残す

## Risks & Mitigations

- 拡張型が JS 境界を越えて予算を超える — トレイトの一括メソッドを最初から置き、ベンチに拡張型を含む列を入れる
- 宣言の文法が後から変わり既存ファイルが読めなくなる — 未知の `kind` を列単位の使用不能に落とす仕組みを最初から入れる
- `Decimal` の順序比較の自作を誤る — 比較は宣言された `scale` に揃えた正準形の上でのみ行い、境界値の単体テストを持つ
- ベンチの予算ゲートが `document-format` 固定で、新しい予算が CI で守られない — `scripts/check-bench-budget.sh` と `.github/workflows/bench.yml` の拡張を実装タスクに含める
- `check-core-deps.sh` が `app-shell` にしか掛かっておらず `tauri` 非依存が検査されない — CI に `schema-engine` の段を足す

## References

- [jiff design](https://docs.rs/jiff/latest/jiff/_documentation/design/index.html) — 厳密な既定と型の対応
- [jiff comparison](https://docs.rs/jiff/latest/jiff/_documentation/comparison/index.html) — `chrono` / `time` との比較
- [RUSTSEC-2020-0159](https://rustsec.org/advisories/RUSTSEC-2020-0159) — `chrono` の `localtime_r` 不健全性（解決済み）
- [jsonschema](https://github.com/Stranger6667/jsonschema) — コンパイル済み検証器の形
- [bigdecimal](https://docs.rs/bigdecimal) / [fastnum](https://github.com/neogenie/fastnum) — 10 進数クレートの比較対象

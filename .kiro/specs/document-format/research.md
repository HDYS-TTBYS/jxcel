# Research & Design Decisions: document-format

## Summary
- **Feature**: `document-format`
- **Discovery Scope**: New Feature（グリーンフィールド、フル調査）
- **Key Findings**:
  - **git は ZIP をデルタ圧縮できない。** DEFLATE は 1 バイトの入力変化で圧縮ストリーム全体がずれ、`Stored` にしてもエントリサイズ変動で中央ディレクトリのオフセットがずれる。ZIP をそのまま git に渡すとリポジトリ増加量は O(コミット数 × ファイルサイズ) になる。実務の定説は「git には ZIP ではなく展開済みエントリを渡す」
  - **決定的 ZIP 出力には明示的な固定が複数必要。** `FileOptions::default()` は壁時計時刻を書き込む（`FileOptions::DEFAULT` は 1980-01-01 固定）。加えて version-made-by のホスト OS バイト、unix permissions、エントリ順序、データディスクリプタが非決定性の源になりうる
  - **キー順序を実際に保証するのは `#[derive(Serialize)]` のフィールド順のみ。** `BTreeMap` は決定的だが辞書順に並べ替えるため差分の可読性を損なう。`IndexMap` + `preserve_order` は挿入順を保つだけで、順序の一貫性は呼び出し側の規律に依存する
  - **NaN / Infinity は serde_json でシリアライズエラーになる。** JSON に表現が無く `Number::from_f64()` が `None` を返す。数式が Inf を生む以上、書き出し境界での遮断が必須
  - **RFC 8785 (JCS) は不適。** ハッシュ・署名のための正規化仕様であり、数値を ECMAScript 形式に強制する。行粒度の整形についても何も規定しない

## Research Log

### 決定的 ZIP 出力（Rust `zip` crate）
- **Context**: 要件 3.1 / 3.2 が「同一内容 → バイト単位で同一、OS をまたいでも同一」を要求する
- **Sources Consulted**: `zip-rs/zip2` リポジトリおよび crate ドキュメント、`flate2` / `miniz_oxide` のバックエンド仕様
- **Findings**:
  - 現行メジャーは `zip` 8.x（8.6.0 / 2026-07）。プロジェクトは `zip-rs/zip2` に移行しており、依存監査で旧 `zip-rs/zip` と混同されやすい
  - `FileOptions<'k, T>` は `T: FileOptionExtension` でジェネリック。素の用途では `FileOptions<'static, ()>`
  - **`FileOptions::DEFAULT`（定数）は 1980-01-01 固定だが、`FileOptions::default()`（`time` feature 有効時）は現在時刻を使う。** 1 文字違いで決定性が壊れる
  - 他の非決定性の源: version-made-by バイト（ビルドプラットフォームの Unix / DOS を反映）、`unix_permissions()` の既定 `0o644`（Windows にはネイティブな概念が無い）、エントリ書き込み順（呼び出し側制御。ファイルシステムの readdir 順ではなく論理エントリのソート済みリストを反復すれば構成的に決定的）、zip64 マーカー（サイズ依存であってホスト依存ではないがテストで踏むべき）
  - データディスクリプタ（general-purpose flag bit 3）はサイズ未知のストリーミング書き込みで使われる。全件オンメモリならサイズは既知なので使わずに済み、決定性と互換性の両方で有利
- **Implications**: ZIP 書き出しは「既定値に頼らず全フィールドを明示的に固定する」方針を設計に明記する。決定性はテストで守る（ゴールデンファイルのバイト比較）

### 圧縮のバイト安定性
- **Context**: DEFLATE 出力が決定的でなければ要件 3.1 / 3.2 が成立しない
- **Findings**:
  - `flate2` のバックエンド（`miniz_oxide` / `zlib` / `zlib-ng`）は同じ入力・同じレベルでも異なるバイト列を出す。マッチ探索のヒューリスティックが異なるため
  - **`miniz_oxide` 自身のバージョン間でのバイト安定性は公開情報からは確認できなかった。** 検索で解決できる種類の問題ではなく、バージョンをピン留めしたうえでの回帰テストが必要
  - `zstd` crate は `libzstd` への FFI ラッパであり、ベンダリング / ピン留めをしない限りクロスプラットフォームビルドで異なる libzstd にリンクしうる。純 Rust の `miniz_oxide` より決定性の負債は大きい
- **Implications**: バックエンドを `miniz_oxide` に固定し `zlib` / `zlib-ng` feature を無効化する。crate 更新は決定性を壊す変更として扱い、ゴールデンファイルテストで検出する

### 行粒度の差分に適した JSON 整形
- **Context**: 要件 3.4 / 3.5 が「1 行のデータ = 出力テキスト上の 1 行」「1 行変更 → 対応する 1 テキスト行のみ変化」を要求する
- **Findings**:
  - `serde_json` の `PrettyFormatter` は全展開か全圧縮かのどちらかで、「配列要素ごとに 1 行、要素内は圧縮」を出せない。実現するにはカスタム `Formatter`（`begin_array_value` / `end_array_value` を上書き）を書く必要がある
  - **NDJSON（JSON Lines）の方が構造的に単純かつ堅牢。** 各行が独立して妥当な JSON であり、末尾カンマの管理が不要で、挿入・追加の差分がより明快になる。代償はそのエントリが単一の妥当な JSON ドキュメントでなくなること
  - NDJSON は行指向 JSON の事実上の標準（ログパイプライン、学習データセット等）
  - キー順序: `#[derive(Serialize)]` のフィールド順は serde が宣言順を保証し実行時コストがゼロ。`BTreeMap` は決定的だが辞書順で人間可読性を損なう。`IndexMap` + `preserve_order` は挿入順の保持のみで、一貫性は呼び出し側の責任
- **Implications**: 行データは NDJSON。行は自由形式マップではなく型付き構造体としてモデル化し、フィールド順を serde に守らせる。スキーマ由来の動的な列集合のみ順序を明示的に固定する

### 数値の決定性と round-trip
- **Findings**:
  - `serde_json` は `ryu` で浮動小数点を出力する。f64 → JSON → f64 は最短表現でロスレスに往復する
  - **NaN / Infinity は JSON に表現が無く、`Number::from_f64()` が `None` を返し、シリアライズは無効な JSON ではなくエラーになる**
  - 2^53 を超える整数は f64 で静かに精度を失う。`-0.0` は ryu で `-0.0` として往復するが、正規化する経路としない経路が混在すると「値は同じなのにバイトが違う」差分の源になる
  - `serde_json::Value` を経由すると `5` は `PosInt`、`5.0` は `Float` として復元され、論理値が変わらないまま表現が変わる型ドリフトが起きる
- **Implications**: 書き出し境界で NaN / Inf を型付きエラーとして遮断する。`-0.0` は `0` に正規化する規則を持つ。`serde_json::Value` を内部表現に使わず、閉じた列挙型で受ける

### 100k 行の性能
- **Findings**:
  - `serde_json` のスループットは構造体形状のデータで数百 MB/s 〜 1 GB/s 程度（ワークロード依存）。`sonic-rs` は 2–4 GB/s、`simd-json` の 1.5–2 倍とされる
  - 100k 行 × 20–50 列（数十 MB の JSON）なら、素の `serde_json` でも解析・直列化そのものは数百 ms 程度に収まる見込み
  - **ボトルネックは JSON 解析より (a) 圧縮・展開、(b) 10 万個の中間 `String` / `Value` 割り当て、(c) 解析後のアプリ側処理になりやすい**
- **Implications**: 素の `serde_json` で実装し、プロファイルで JSON 解析がボトルネックだと示されるまで SIMD 系に手を出さない。割り当て削減を先に検討する

### 原子的保存
- **Findings**:
  - `tempfile::NamedTempFile::persist()` は Unix では rename(2) により原子的だが、**全プラットフォームでの原子性は保証されないと明記されている**
  - Windows 固有の失敗: アンチウイルスの実時間スキャナ、検索インデクサ、自アプリの second instance などが宛先のハンドルを保持していると共有違反で rename が失敗する。POSIX のように黙って置き換えるのではなく拒否する
  - 正しい手順: 同一ディレクトリ（同一ファイルシステム）に一時ファイルを書く → `sync_all()` → rename → **Linux / macOS では親ディレクトリの fsync**（rename 単体ではディレクトリエントリの更新が電源断に対して耐久化されない）
- **Implications**: Windows では共有違反に対するバックオフ付きリトライを設計に含める。これは任意の最適化ではなく、実際の AV 製品で起きる

### 完全性検証
- **Findings**: ZIP の CRC32 は検出専用で弱い。BLAKE3 は単一スレッドで 2 GB/s 超、SHA-256（ハードウェア拡張なし）の 0.5–1 GB/s を大きく上回る。数十 MB のハッシュは数ミリ秒で、2 秒の保存予算に対して無視できる
- **Implications**: エントリごとの BLAKE3 ダイジェストをマニフェストに持つ。ZIP の CRC32 は安価な二次チェックとして併用する

### zip-slip と悪意あるアーカイブ
- **Findings**:
  - CVE-2025-29787（CVSS 7.3）は `zip` 1.3.0–2.2.x に影響。アーカイブ内の先行するシンボリックリンクエントリで後続エントリを展開先の外へ誘導できた。**2.3.0 で修正**（パス正規化・検証モジュールを追加）
  - `ZipFile::enclosed_name()` は NUL で切り詰め、先頭 `/` を除去し、`..` 成分を落とす。**ただし名前文字列のサニタイズのみ**でシンボリックリンク経由の誘導は扱わず、Unicode 類似文字や Windows 由来のバックスラッシュ区切りの曖昧さも正規化しない
  - **一次形式（自分が書いた形式）を読む場合の最善策は、アーカイブのディレクトリリストを信用せず、期待するエントリ名の許可リストと照合して不明なエントリを拒否すること**
- **Implications**: サニタイズではなく許可リストによる拒否を採用する

### ZIP コンテナ形式の先行事例
- **Sources Consulted**: OASIS ODF v1.2/1.3 Part 3 (Packages)、ECMA-376 / ISO 29500-3 (MCE)、Microsoft Learn OOXML ドキュメント
- **Findings**:
  - **ODF**: `mimetype` を ZIP の先頭エントリに無圧縮（STORED、extra field 無し）で置くことで、固定バイトオフセットでの型判定を可能にしている。`META-INF/manifest.xml` が全パートの唯一の権威あるインデックス
  - **OOXML**: `[Content_Types].xml`（型レジストリ）と `_rels/*.rels`（関係グラフ）という**2 つの独立した帳簿**を持ち、両者の同期が必要。実際の失敗形態は (a) 関係から参照されない孤立パート（自動清掃されない）、(b) 削除・改名されたパートを指す宙吊りの関係（「コンテンツに問題があります」の修復プロンプトや読み込み失敗）
  - ODF の単一マニフェストの方が明確に優れたパターン。「参照されている全ファイルが存在するか」「全ファイルがマニフェストに載っているか」を 1 箇所で検査できる
- **Implications**: ODF 方式（先頭に無圧縮のマーカー + 単一マニフェスト）を採る。OOXML の二重帳簿は採らない

### git フレンドリーを狙った形式の実例
- **Findings**:
  - **Jupyter `.ipynb`**: base64 の出力と `execution_count` のような揮発的メタデータを本文に埋め込む JSON。差分が破綻する典型例。生態系の対処は 2 系統に分かれた — **nbdime**（セル・出力の意味を理解する構造的 diff/merge を git ドライバとして差し込む）と **Jupytext**（JSON を差分しない。`.py` / `.md` の平文表現と対にして後者を差分する）。**どちらも形式自体は直していない。迂回している**
  - **Godot `.tscn`**: 平文。Godot 4 でリソース ID を連番整数から文字列 UID に変更した。理由は、独立したブランチが同時にリソースを追加したときの衝突を減らすため。それでも衝突は起き、`gdmerge` という第三者の意味的マージツールが生まれた
  - **Unity YAML シーン**: 平文化（Force Text）しても `fileID` 参照が絡むため行ベースのマージが参照を壊す。`.gitattributes` 経由で専用の 3-way マージドライバ UnityYAMLMerge が必須
  - **Tiled `.tmx`**: タイル層のエンコーディングとして raw XML / base64+圧縮 / **CSV** を選べ、git と人間に優しい選択肢として CSV が明示的に推奨されている
  - **Grist**: スプレッドシートアプリだがネイティブ保存形式が SQLite。安定した行 ID を持つため、バイナリでも 3-way diff が扱いやすいという設計
  - 教訓: **平文であることは必要条件だが十分条件ではない。ID 方式が構文と同じくらい効く**
- **Implications**: 安定した行 ID を最初から持つ。将来 `version-control` が構造的 diff を実装できる土台を、本スペックの ID 設計で用意する

### git のデルタ圧縮と ZIP（設計上の中心的緊張）
- **Context**: 製品価値の中心が「既定で有効なバージョン管理」であるため、リポジトリの増え方は無視できない
- **Findings**:
  - 根本原因: DEFLATE は不安定。1 バイトの入力変化で圧縮ストリーム全体がずれ、git は前バージョンとのデルタを取れない。**リポジトリ増加量は変更量ではなくファイルサイズに比例する**
  - 実務での対処、採用実績の多い順:
    1. **展開したディレクトリを git に持たせる**（ZIP はエクスポート / オープン時のみ）。追加のツール不要、git が各パートをネイティブに扱う。代償はアプリがディレクトリを読み書きすること
    2. **単一 ZIP のまま `Stored`（無圧縮）で書く、および / または clean/smudge フィルタ**（`ReZip`、`ReZipDoc` 等）でチェックイン時に展開しチェックアウト時に再圧縮する。実測例として 280 個のバイナリ `.slx` / 3000 コミットのリポジトリが 281MB → 156MB（約 55% 減）。代償はフィルタのインストールがユーザー任せになること、smudge/clean がバイト単位で往復しないと「差分は無いのにファイルが変わった」不具合になること
    3. **`textconv` / カスタム diff ドライバ**: 保存されるものは変わらず（サイズ問題は未解決）、`git diff` の**表示**のみを変換する。表示だけが問題なら最も安価
    4. **フィルタ無しの `Stored` ZIP**: 部分的な改善に留まる。DEFLATE の不安定性は消えるが、エントリサイズが変わると中央ディレクトリのオフセットがずれるため完全にはデルタが効かない
- **Implications**: **本スペックは単一 ZIP を「ユーザーに見えるドキュメント」として保証しつつ、展開済みエントリ集合を公開契約として `version-control` に渡す。** git に何を保存するかは `version-control` の決定だが、その選択肢を持たせるのは本スペックの責務

### レコード ID 方式
- **Findings**:
  - **連番整数**: テキスト上の footprint は最小だが、途中挿入や削除後の再採番が後続全行に波及する。位置を同一性に使ってはならない
  - **UUIDv4**: 位置から独立して安定だが 36 文字が全行に乗り、順序と無相関。差分の可読性は最悪
  - **UUIDv7**: v4 と同じ 128 bit / 36 文字だが 48 bit のミリ秒タイムスタンプを埋め込むため時系列にソートされ、新規行が末尾に固まって差分の局所性が上がる
  - **ULID**: UUIDv7 と同設計（48 bit ms + ランダム）だが Crockford base32 で 26 文字。ハイフン無し、大文字小文字非依存。人間が読む場所に ID が現れる用途で明確に有利
  - **content-hash ID**: 内容が変われば ID も変わる。行の同一性には逆効果だが、添付・ブロブの命名には最適
- **Implications**: 行・シート・ネスト型定義は ULID。添付は content-hash（BLAKE3）

### 分割戦略
- **Findings**:
  - **シートごとの分離**は全形式で一致した合意（OOXML の `xl/worksheets/sheetN.xml`、Tiled の層ごと）。あるシートの編集が他シートのエントリに触れてはならない
  - **スキーマとデータの分離**も強く支持される。OOXML が `styles.xml` / shared strings をシートデータから分けているのと同じ理由 — スキーマ変更は稀で大きく、データ変更は頻繁で局所的
  - **行の N 件ごとのチャンク分割は先行事例に無い。** 各形式は構造単位ごとに 1 つのデータブロブを保ち、その内部で JSONL / CSV の「1 レコード 1 行」性に差分の局所性を任せている。常時オンメモリなら多数の小エントリのオーバーヘッド（ZIP のエントリごとのローカルヘッダ、git の多数の小ブロブ処理）に見合う利得が無い
  - **揮発的メタデータの分離**: OOXML の `docProps/core.xml`（作者、タイムスタンプ）が本文と分かれているのと同じ理由
- **Implications**: シートごと、かつスキーマとデータを分離。行のチャンク分割はしない。揮発的メタデータについては、要件 3.6 が保存時刻依存の値の出力自体を禁じているため、分離ではなく**排除**で解決する

### バージョニングと移行
- **Findings**:
  - **nbformat**: `nbformat`（major）+ `nbformat_minor`。minor は後方互換（省略可能キーの追加のみ、古い読み手は未知キーを無視し往復で保存する）。major は破壊的変更。**既知の実際の齟齬**: 参照実装の upgrade 経路は major ステップしか扱っておらず、minor の更新はツールが実際には適用していない。文書化された方針と出荷されたコードが乖離した
  - **OOXML MCE**: `mc:Ignorable` で新しい生成器が拡張要素を「古い消費者は安全に読み飛ばしてよい」と標示できる。`AlternateContent` / `Choice` / `Fallback` で新機能表現と旧互換表現を同一ファイルに同梱し、開いた処理系が解決する
  - **ODF**: 適合クラス（conforming / extended conforming）+ バージョン属性。適合する消費者は**理解できない要素・属性を往復で破棄せず保持することを要求される**。この「理解できないものを保持する」規則こそが前方互換の実務上の核心であり、バージョン番号そのものより重要
  - 段階的移行チェーン（v1→v2→v3 を順に適用）は確かに標準的な手法。**繰り返される失敗形態は nbformat が踏んだものそのもの** — 実ファイルの大半は最新への 1 段だけを必要とするため、中間ステップの移行コードが未保守・未テストのまま腐り、古いファイルが現れて初めて露見する
- **Implications**: major.minor を採る。minor は追加のみ、未知フィールドは往復で保持する。major は読み込み時にゲートする。**移行チェーンの腐敗に対しては、過去バージョンごとのゴールデン fixture を CI に置くことで対処する**

### 添付と参照整合性
- **Findings**:
  - OOXML はバイナリを `xl/media/imageN.png` に置き、`_rels` の関係 ID で参照し `[Content_Types].xml` と突き合わせる。**組み込みの整合性チェッカは存在しない**。実務の修復手順は「信頼できるパートを取り出して再圧縮する」
  - 先行形式はいずれも添付に content-hash 名を使っておらず連番・位置ベースの命名。content-addressed 命名なら重複排除が無料で得られ、「このバイナリは実際に変わったのか」が名前の一致判定になる。代償はファイル名が意味を持たなくなること（人間可読名は参照表側に持たせる）
  - **git にコミットされるアーカイブ内にバイナリが同居する場合の固有の問題**: ドキュメントのどの部分を編集しても再圧縮が起き、変更されていない添付のバイトまで毎回コミットし直される
- **Implications**: 添付は content-hash 命名で、スキーマ・行データとは独立したエントリとして持つ。人間可読名は参照表側に置く

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| **階層化パイプライン（採用）** | Container ↔ Parts ↔ Model の 3 層。読み書きは同一の Parts 集合に対する逆操作 | 各層が単独でテスト可能。Parts を公開契約にすることで `version-control` に ZIP 以外の選択肢を渡せる | 層が 1 つ増える | 依存方向を左向き一方向に固定できる |
| 単層（Model ↔ ZIP 直結） | モデルから直接 ZIP を読み書きする | 実装が短い | 決定性・完全性・移行・zip-slip の関心が 1 箇所に混ざる。`version-control` にエントリ集合を渡す手段が無い | 却下 |
| ディレクトリを正典形式にする | ZIP をやめ、展開済みディレクトリを保存形式とする | git との相性が最良 | 「持ち運べる 1 ファイル」という製品要件と正面から衝突する | 却下（product.md の中核価値） |
| SQLite をコンテナにする（Grist 方式） | 保存形式を SQLite にする | 安定行 ID、部分更新、トランザクション | 要件が ZIP + JSON テキストを明示。バイナリになり「中身を自分で確認できる」価値を失う | 却下（要件 2.1） |

## Design Decisions

### Decision: 展開済みエントリ集合を公開契約とする
- **Context**: git は ZIP をデルタ圧縮できず、リポジトリはコミットごとにファイルサイズ分ずつ増える。一方で「持ち運べる単一ファイル」は製品の中核価値であり放棄できない
- **Alternatives Considered**:
  1. ZIP をそのまま git に渡す — リポジトリが線形に肥大する
  2. 保存形式をディレクトリにする — 単一ファイル要件と衝突する
  3. `version-control` 側で ZIP を展開する — 形式の内部構造の知識が 2 スペックに重複し、片方の変更が黙ってもう片方を壊す
- **Selected Approach**: 本スペックは `DocumentParts`（論理エントリ集合: 名前 → バイト列 + ダイジェスト）を公開型として定義する。ユーザーに見えるドキュメントは単一 ZIP のままだが、git に何を保存するかは `version-control` が `DocumentParts` を受け取って決められる
- **Rationale**: 形式の内部構造の知識が本スペックに閉じる。`version-control` は「ZIP をコミットする」「エントリを個別にコミットする」「clean/smudge を使う」のいずれも選べる。ZIP 生成は `DocumentParts` からの純粋な関数になり、テストしやすい
- **Trade-offs**: 公開 API が 1 つ増え、`DocumentParts` の形が下流の再検証トリガになる
- **Follow-up**: `version-control` の design で、実際に何を git に置くかを決める。本スペックはその判断をしない

### Decision: 行データは NDJSON、コンテナは ODF 方式のレイアウト
- **Context**: 要件 3.4 / 3.5 が行粒度の差分を要求する
- **Alternatives Considered**:
  1. JSON 配列 + カスタム `Formatter` — 単一の妥当な JSON を保てるが、末尾カンマの管理が要り、挿入時の差分が 1 行に収まらない
  2. NDJSON — 各行が独立して妥当。挿入・追加の差分が最も明快
- **Selected Approach**: 行データエントリは NDJSON（`sheets/<sheet-id>.jsonl`）。コンテナは ODF に倣い、先頭に無圧縮の型マーカー、単一の `manifest.json` を権威あるインデックスとする。OOXML の二重帳簿（content-types + rels）は採らない
- **Rationale**: NDJSON は行指向 JSON の事実上の標準。ODF の単一マニフェストは OOXML の二重帳簿より整合性検査が単純で、OOXML の既知の失敗形態（孤立パート・宙吊り参照）を構造的に避けられる
- **Trade-offs**: 行データエントリは単一の妥当な JSON ドキュメントではなくなる。汎用 JSON ツールでそのエントリだけを開くと失敗する
- **Follow-up**: エクスポート機能が行エントリを直接読む場合、NDJSON 対応が必要であることを `export-templates` に伝える

### Decision: ID は ULID（行・シート・型定義）、添付は content-hash
- **Context**: 差分の可読性と、並び替えに対する安定性の両立
- **Alternatives Considered**: 連番整数（挿入で全後続行が動く）、UUIDv4（36 文字・順序と無相関）、UUIDv7（36 文字・時系列ソート可）
- **Selected Approach**: 行・シート・ネスト型定義は ULID（26 文字、Crockford base32、時系列ソート可）。添付は BLAKE3 ダイジェストによる content-addressed 命名
- **Rationale**: ULID は UUIDv7 と同じ保証を 10 文字短く提供し、差分テキストに毎行現れる用途で有利。単一ユーザー前提なので完全ランダムの衝突回避より可読性が効く。添付は内容が同一性そのものであり content-hash が自然で、重複排除も無料で得られる
- **Trade-offs**: 添付のファイル名が人間には意味を持たない。人間可読名は参照表側に持たせる
- **Follow-up**: 添付の内容が変わると ID が変わるため、参照側の更新が必要になる。これは意図した意味論である

### Decision: `Stored` と `Deflate` の使い分け、および決定性の守り方
- **Context**: 要件 3.1 / 3.2（バイト同一）と、単一ファイルとしての可搬性（サイズ）の両立
- **Alternatives Considered**: 全エントリ `Stored`（決定性は自明、サイズが数倍）、全エントリ `Deflate`（サイズ最小、バックエンド依存の非決定性リスク）
- **Selected Approach**: 型マーカーは `Stored`（ODF 方式、固定オフセットでの型判定のため）。他は `Deflate` で `flate2` のバックエンドを `miniz_oxide` に固定し `zlib` / `zlib-ng` feature を無効化する。決定性はゴールデンファイルのバイト比較テストで守り、crate 更新は決定性を壊す変更として扱う
- **Rationale**: git に渡すのは `DocumentParts`（非圧縮のエントリ）であるため、ZIP の圧縮方式は差分品質に影響しない。したがって可搬性（サイズ）を優先できる。決定性はテストで担保する
- **Trade-offs**: `miniz_oxide` のバージョン間バイト安定性は未検証であり、crate 更新のたびにゴールデンテストが落ちる可能性がある。落ちた場合は形式のマイナーバージョンを上げるか、バージョンを固定し続けるかの判断が要る
- **Follow-up**: ゴールデンテストが `miniz_oxide` 更新で落ちた場合の運用手順を実装時に決める。落ちる頻度が高ければ全エントリ `Stored` へ退避する

### Decision: 未知フィールドは往復で保持する（ODF の前方互換規則）
- **Context**: 要件 6 がバージョン間の互換を要求する
- **Selected Approach**: major.minor を持つ。minor の増加は省略可能フィールドの追加のみに限り、読み込み側は未知のフィールドを**破棄せずに保持して書き戻す**。major の不一致は読み込み時にゲートする
- **Rationale**: ODF が明示的に要求している規則であり、前方互換の実務上の核心。バージョン番号そのものより効く。これが無いと、新しいバージョンで作ったファイルを古いバージョンで一度開いて保存しただけでデータが静かに失われる
- **Trade-offs**: 未知フィールドを保持する領域をデータモデルに持つ必要がある
- **Follow-up**: 移行チェーンの中間ステップが腐る既知の失敗形態（nbformat が踏んだもの）に対し、過去バージョンごとのゴールデン fixture を CI に置く

### Decision: zip-slip はサニタイズではなく許可リストで防ぐ
- **Context**: 要件 2.5 が、ルート外を指すエントリの拒否を要求する
- **Selected Approach**: `enclosed_name()` によるサニタイズに依存せず、期待するエントリ名の文法（許可リスト）と照合し、一致しないエントリを持つコンテナを不正として拒否する
- **Rationale**: 本形式は一次形式であり、正当なファイルのエントリ名は完全に既知である。サニタイズは「危険な名前を安全な名前に変える」が、許可リストは「想定外のものを拒否する」。後者は Unicode 類似文字や Windows 由来の区切り文字の曖昧さといった、サニタイズが扱わない攻撃面も同時に閉じる
- **Trade-offs**: 形式の拡張時に許可リストの更新を忘れると正当なファイルを拒否する。許可リストを形式バージョンと同じ場所で管理して緩和する

### Decision: 数値は閉じた列挙型で受け、NaN / Inf を境界で遮断する
- **Context**: 要件 3.1 / 3.2 の決定性と、セル値のロスレスな往復
- **Selected Approach**: セル値を閉じた列挙型（`CellValue`）として定義し、`serde_json::Value` を内部表現に使わない。NaN / Infinity は書き出し境界で型付きエラーとして拒否する。`-0.0` は `0` に正規化する。厳密な十進表現が必要な列は文字列としてエンコードされた十進値の変種を使う
- **Rationale**: `serde_json::Value` を経由すると `5` と `5.0` の型ドリフトが起き、論理値が変わらないままバイトが変わる。NaN / Inf は JSON に表現が無く、遮断しなければ保存が失敗する
- **Trade-offs**: どの列が十進表現を使うかの判断は型の意味論であり `schema-engine` の責務。本スペックは wire 表現の集合を定義するに留める
- **Follow-up**: `schema-engine` に、数値型と `CellValue` 変種の対応を決める責務があることを伝える

### Decision: 素の `serde_json` で実装し、SIMD 系は証拠が出るまで採らない
- **Context**: 要件 8.1 / 8.2（開く 3 秒 / 保存 2 秒）
- **Selected Approach**: `serde_json` で実装し、Criterion による 10 万行のベンチマークを CI に置く。予算を超えた場合にのみ、プロファイルの結果に従って対処する
- **Rationale**: 数十 MB の JSON なら解析・直列化そのものは数百 ms に収まる見込みで、予算の大半は他の要因（圧縮、割り当て、後段処理）に消える。生態系の成熟度を捨てて SIMD 系に移る根拠が現時点で無い
- **Trade-offs**: 予算を超えた場合に手戻りが発生しうる。ベンチマークを最初から置くことで早期に検出する

## Synthesis Outcomes

### Generalization
- 読み込みと書き出しは `DocumentParts` を挟んだ対称な逆操作である。片方向ずつ別々に設計せず、`Model ↔ Parts ↔ Container` の 2 段の双方向変換として一般化する。これにより、将来のエクスポート・インポート経路も同じ `Parts` を再利用できる
- 完全性検証・許可リスト検証・参照整合性検証はすべて「`Parts` に対する検証」として同じ層に置ける。読み込み経路に散在させない

### Build vs. Adopt
- **採用**: `zip` 8.x（コンテナ）、`serde_json`（直列化）、`tempfile`（原子的保存）、`blake3`（完全性）、`ulid`（ID）
- **不採用**: RFC 8785 (JCS) — ハッシュ用の正規化仕様であり用途が違う。数値を ECMAScript 形式に強制する点が有害
- **不採用**: `simd-json` / `sonic-rs` — 現時点で性能上の必要性の証拠が無い
- **自作**: NDJSON の読み書きと決定的 ZIP 書き出しの薄い層。どちらも既存 crate の組み合わせで、独立した crate を採る規模ではない

### Simplification
- **揮発的メタデータ用のパートを作らない。** 先行事例（OOXML の `docProps/core.xml`）は揮発値を分離するが、要件 3.6 が保存時刻依存の値の出力自体を禁じているため、分離ではなく排除で解決できる。パートが 1 つ減る
- **行のチャンク分割をしない。** 先行事例に無く、常時オンメモリでは多数の小エントリのオーバーヘッドに見合う利得が無い
- **OOXML の二重帳簿を採らない。** ODF の単一マニフェストで足りる
- **移行チェーンは空の状態から始める。** 初版は v1 のみであり、移行ステップの実装は存在しない。枠組みだけを置き、最初の移行が必要になった時点で実装する

## Risks & Mitigations
- **`miniz_oxide` のバージョン間バイト安定性が未検証** — ゴールデンファイルのバイト比較テストを CI に置き、crate 更新を決定性を壊す変更として扱う。頻繁に落ちるなら全エントリ `Stored` へ退避する
- **`FileOptions::default()` と `FileOptions::DEFAULT` の取り違え** — ZIP 書き出しを 1 箇所に閉じ込め、そこで全フィールドを明示的に固定する。決定性テストが取り違えを検出する
- **Windows での rename 失敗（AV のハンドル保持）** — バックオフ付きリトライを実装に含める。ライブラリ単体では解決しない既知の問題
- **移行チェーンの中間ステップの腐敗** — 過去バージョンごとのゴールデン fixture を CI に置く。nbformat が実際に踏んだ失敗形態
- **NDJSON が単一の妥当な JSON でないことによる下流の齟齬** — `export-templates` および将来のインポート経路に対し、行エントリが NDJSON であることを契約として明示する
- **10 万行の性能予算の超過** — Criterion ベンチマークを最初から CI に置き、予算超過を機能追加と同時に検出する

## References
- [OASIS ODF v1.2 Part 3: Packages](https://docs.oasis-open.org/office/v1.2/os/OpenDocument-v1.2-os-part3.html) — 先頭 `mimetype` エントリと単一マニフェストの規定
- [ECMA-376 / ISO 29500-3 Markup Compatibility and Extensibility](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/) — `mc:Ignorable` と `AlternateContent` による前方互換
- [nbformat: The Notebook file format](https://nbformat.readthedocs.io/) — major/minor バージョニングと移行チェーンの腐敗事例
- [JSON Lines](https://jsonlines.org/) — 行指向 JSON の事実上の標準
- [zip-rs/zip2](https://github.com/zip-rs/zip2) — 現行の `zip` crate（8.x）
- [RUSTSEC-2025-0168 / CVE-2025-29787](https://rustsec.org/advisories/RUSTSEC-2025-0168.html) — シンボリックリンク経由の展開先脱出、2.3.0 で修正
- [Tiled TMX Map Format](https://doc.mapeditor.org/en/stable/reference/tmx-map-format/) — CSV エンコーディングを git / 人間向けに推奨
- [Godot: TSCN file format](https://docs.godotengine.org/en/stable/contributing/development/file_formats/tscn.html) — 連番整数 ID から文字列 UID への移行
- [ULID Specification](https://github.com/ulid/spec) — Crockford base32、時系列ソート可能な 26 文字 ID
- [BLAKE3](https://github.com/BLAKE3-team/BLAKE3) — 完全性検証のダイジェスト

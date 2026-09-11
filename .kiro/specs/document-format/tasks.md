# Implementation Plan

- [x] 1. Foundation: リポジトリ基盤と横断型
- [x] 1.1 ワークスペースとクレートをスキャフォールドする
  - リポジトリルートに Cargo ワークスペースを作成する（本クレートがリポジトリ最初のクレートである）
  - `crates/document-format/` をライブラリクレートとしてワークスペースに登録する
  - `zip` 8.x、`serde` / `serde_json`、`tempfile`、`blake3`、`ulid`、`thiserror` を依存に追加する
  - ベンチマーク用に `criterion` を dev-dependency として追加し、ベンチターゲットを宣言する
  - `flate2` のバックエンドを `miniz_oxide` に固定し `zlib` / `zlib-ng` feature を無効化する
  - `tauri` および他の jxcel ドメインクレートへの依存を持たないことを Cargo.toml のコメントで明示する
  - `cargo test` と `cargo bench --no-run` が成功し、`cargo tree` に `tauri` が現れない
  - _Requirements: 3.1, 3.2_

- [x] 1.2 CI パイプラインを構築する
  - Linux / macOS / Windows の 3 OS マトリクスでビルドとテストを実行する
  - `cargo audit` をゲートとして組み込み、`zip` ≥ 2.3.0 の下限を満たすことを確認する
  - ベンチマークを実行する経路を用意する（計測環境として SSD 搭載・4 コア以上のランナーを選ぶ）
  - 3 OS すべてでテストが緑になり、`cargo audit` の失敗がパイプラインを落とす
  - _Requirements: 3.2, 8.3_

- [x] 1.3 識別子の型と発行を実装する
  - シート / 行 / ネスト型定義に ULID ベースの識別子新型を定義し、相互に取り違えられないようにする
  - 添付には BLAKE3 ダイジェストによる content-addressed 識別子を定義する
  - 同一内容の添付が同一の識別子を得ることをテストで示す
  - 連続発行した ULID が時系列に昇順ソートされることをテストで示す
  - _Requirements: 1.4, 7.2_

- [x] 1.4 エラー型を定義する
  - 設計のエラー表で定めた全変種を判別可能な列挙型として定義する
  - 各変種が診断に必要な文脈（エントリ名、識別子、参照元と参照先、バージョン）を保持する
  - エラーは文言を持たず、呼び出し元が提示方法を決められる形になっている
  - すべての変種が生成・照合できることをテストで示す
  - _Requirements: 4.5, 5.3, 6.5_

- [x] 1.5 セル値の wire 表現を実装する
  - Null / Bool / Int / Float / Decimal / Text / Nested / Attachment の閉じた列挙型を定義する
  - 汎用 JSON 値型を内部表現に使わず、整数と浮動小数点の型ドリフトが起きないことをテストで示す
  - NaN と Infinity が型付きエラーとして拒否され、無効な JSON が書かれないことをテストで示す
  - `-0.0` が `0` として出力されることをテストで示す
  - 行から添付への参照が値として表現できる
  - _Requirements: 7.3_

- [x] 1.6 エントリ名の文法と許可リストを実装する
  - 型マーカー / マニフェスト / ドキュメント / スキーマ / 行データ / 添付の 6 形のみを受理する型を定義する
  - サニタイズではなく拒否とし、名前の正規化に依存しない
  - 絶対パス、`..` 成分、ドライブレター、バックスラッシュ区切り、NUL を含む名前がすべて拒否されることをテストで示す
  - 許可リストを形式バージョンと同じ場所で管理する
  - この型はパート層とコンテナ層の双方が使う共有プリミティブであり、コンテナ層ではなく最下層に置く
  - _Requirements: 2.2, 2.3, 2.5_
  - _Boundary: EntryName_

- [x] 2. Core: ドキュメントモデル
- [x] 2.1 集約ルートと構造的不変条件を実装する
  - Document / Sheet / Row の階層と、シートの順序・行の順序を保持する構造を定義する
  - 行の並び替えとシートの改名が識別子を変更しないことをテストで示す
  - 10 万行を保持できることを確認する
  - _Requirements: 1.1, 1.2, 1.5, 1.6, 8.4_

- [x] 2.2 (P) スキーマパートを不透明ペイロードとして保持する
  - ルートスキーマとネスト型定義を、内部を解釈せずに保持する構造を定義する
  - 型定義の識別子と参照構造のみを本クレートが扱い、型の意味論には触れないことをテストで示す
  - 1 シートがルートスキーマをちょうど 1 つ持つ不変条件を検証する
  - _Requirements: 1.2, 1.3_
  - _Boundary: SchemaPart_
  - _Depends: 1.3_

- [x] 2.3 (P) 添付レジストリと参照集計を実装する
  - 任意のバイト列を添付として保持し、解釈・変換・再圧縮を行わない
  - 添付のバイト列が保持と取り出しで 1 バイトも変わらないことをテストで示す
  - どの行からも参照されていない添付を削除せず、未参照として一覧できる
  - _Requirements: 7.1, 7.5, 7.6_
  - _Boundary: AttachmentRegistry_
  - _Depends: 1.3_

- [x] 3. Core: JSON 決定性層
- [x] 3.1 決定的な JSON 出力を実装する
  - UTF-8 で出力し、キー順序を構造体のフィールド宣言順に固定する
  - 汎用 JSON 値型およびハッシュマップを経由する経路を持たない
  - スキーマ由来の動的な列集合を、スキーマが定める列順序で明示的に整列する
  - 同一の入力に対して常に同一のバイト列が出ることをテストで示す
  - _Requirements: 2.4, 3.3, 3.6_

- [x] 3.2 未知フィールドの保持を実装する
  - 読み込み時に未知のフィールドを破棄せず保持し、書き戻し時に元の位置へ復元する
  - 未知フィールドを含む JSON を読んで書き戻すと、バイト単位で元に戻ることをテストで示す
  - 移行チェーンがこの機構に依存するため、バージョニングの実装より先に確定させる
  - _Requirements: 6.2, 6.3_

- [x] 3.3 NDJSON コーデックを実装する
  - 1 行 1 オブジェクトで読み書きし、行末を固定して OS の改行規約に依存しない
  - 行の出力順がシートの保持する行順序に従うことをテストで示す
  - 1 行のセルを変更して書き出したとき、出力テキストの差分が 1 行のみであることをテストで示す
  - _Requirements: 3.4, 3.5_

- [x] 4. Core: パート層
- [x] 4.1 完全性検証を実装する
  - パートごとに BLAKE3 ダイジェストを算出し、照合できるようにする
  - ダイジェストが一致しない場合に該当エントリ名を含むエラーを返すことをテストで示す
  - _Requirements: 5.1, 5.2, 5.3_

- [x] 4.2 マニフェストパートを実装する
  - 形式バージョン、パート索引、パートごとのダイジェストを保持する唯一の権威ある索引とする
  - マニフェストが存在しない場合に不足パート名を含むエラーで中止することをテストで示す
  - _Requirements: 4.5, 6.1_
  - _Depends: 4.1_

- [x] 4.3 (P) ドキュメントパートを実装する
  - ドキュメント識別子、シート順序、シートのメタデータを保持する
  - 保存時刻のような揮発値を一切含まないことをテストで示す
  - _Requirements: 2.2, 3.6_
  - _Boundary: DocumentPart_
  - _Depends: 1.6, 3.1_

- [x] 4.4 (P) スキーマパートの符号化を実装する
  - シートごとに独立したエントリとしてスキーマを符号化・復号する
  - 不透明ペイロードが往復で完全に同一であることをテストで示す
  - _Requirements: 1.3, 2.2_
  - _Boundary: SchemaCodec_
  - _Depends: 1.6, 2.2, 3.1_

- [x] 4.5 (P) 行データパートの符号化を実装する
  - シートごとに独立した NDJSON エントリとして行データを符号化・復号する
  - 10 万行の往復でモデルが完全に一致することをテストで示す
  - _Requirements: 2.3, 3.4_
  - _Boundary: RowsCodec_
  - _Depends: 1.6, 3.3_

- [x] 4.6 識別子の一意性とスキーマ存在の検証を実装する
  - シート / 行 / 型定義 / 添付の識別子が種別ごとに一意であることを検証する
  - シートデータに対応するスキーマが存在することを検証する
  - 重複した識別子と出現箇所、およびスキーマ欠落シートの識別子を含むエラーが返ることをテストで示す
  - _Requirements: 4.3, 4.4_

- [x] 4.7 参照整合性の検証を実装する
  - 型定義参照が同一シートの型定義集合に実在することを検証する
  - 添付参照が添付レジストリに実在することを検証する
  - 宙吊りの参照が、参照元と参照先を含むエラーとして返ることをテストで示す
  - _Requirements: 1.7, 4.2, 7.4_

- [x] 4.8 論理エントリ集合の組み立てと分解を実装する
  - モデルからパート集合を構築し、パート集合からモデルを復元する双方向変換を実装する
  - エントリ名の昇順で決定的に反復し、ファイルシステムの列挙順に依存しないことをテストで示す
  - ZIP の知識を一切持たず、圧縮にも触れないことを型で保証する
  - _Requirements: 2.2, 2.3_
  - _Depends: 4.2, 4.3, 4.4, 4.5, 4.6, 4.7_

- [x] 5. Core: コンテナ層と原子的保存
- [x] 5.1 (P) 原子的保存を実装する
  - 対象と同一ディレクトリに一時ファイルを作り、同期してから置換する
  - Unix では置換後に親ディレクトリを同期する
  - Windows では共有違反に対してバックオフ付きでリトライする
  - 置換前にプロセスを落としたとき、元のファイルが保存前の内容のまま残ることをテストで示す
  - _Requirements: 5.6_
  - _Boundary: AtomicWriter_

- [x] 5.2 決定的な ZIP 書き出しを実装する
  - 更新日時、パーミッション、ホスト OS バイト、エントリ順序をすべて明示的に固定し、crate の既定値に依存しない
  - データディスクリプタを使わず、型マーカーを無圧縮で先頭に置く
  - 同一のパート集合に対して常に同一のバイト列を返すことをテストで示す
  - 期待バイト列のゴールデンファイルを追加し、圧縮バックエンドの更新で決定性が壊れたことを検出できるようにする
  - _Requirements: 2.1, 3.1, 3.2, 3.6_
  - _Depends: 1.6, 4.8_

- [x] 5.3 ZIP 読み込みと不正コンテナの拒否を実装する
  - 許可リストに一致しないエントリ名を持つコンテナを拒否する
  - 同一パスの重複エントリを持つコンテナを拒否する
  - 展開前にマニフェストの宣言サイズと実際の展開量を照合し、極端に膨張するアーカイブを拒否する
  - 各拒否シナリオが該当エントリ名を含むエラーを返すことをテストで示す
  - _Requirements: 2.5, 2.6_
  - _Depends: 1.6_

- [x] 6. Core: バージョニングと移行
- [x] 6.1 形式バージョンの表現と読み込み時ゲートを実装する
  - major.minor を表現し、マニフェストに記録する
  - 現行より新しい major のドキュメントを、要求バージョンを含むエラーで拒否することをテストで示す
  - _Requirements: 6.1, 6.5_
  - _Depends: 4.2_

- [x] 6.2 段階的移行チェーンの枠組みを実装する
  - 古いバージョンを 1 段ずつ現行へ変換する適用機構を実装する（初版は v1 のみのため実際のステップは空）
  - 複数バージョンをまたぐ変換が順に適用される経路をテストで示す
  - 未知フィールドが変換を経ても保持されることをテストで示す
  - 過去バージョンごとのゴールデン fixture を置くディレクトリを用意する
  - _Requirements: 6.2, 6.3_
  - _Depends: 3.2, 6.1_

- [x] 7. Integration: 公開 API の結線
- [x] 7.1 ドキュメントを開く経路を結線する
  - ZIP 読み込み、許可リスト照合、バージョンゲート、移行、ダイジェスト照合、構造検証、モデル構築をこの順に接続する
  - すべての検証がモデル構築の前に完了し、失敗時に部分的なモデルが返らないことをテストで示す
  - 読み込み経路がいかなる書き込みも行わないことをテストで示す
  - 10 万行を超えるドキュメントを拒否せず、保証対象外である旨を通知する
  - _Depends: 4.1, 4.6, 4.7, 4.8, 5.3, 6.2_
  - _Requirements: 4.1, 5.2, 5.4, 5.5, 8.5_

- [x] 7.2 ドキュメントを保存する経路を結線する
  - 不変条件の検証、パート構築、ダイジェスト算出、決定的符号化、原子的書き込みをこの順に接続する
  - いずれかの段階で失敗した場合、既存ファイルが一切変更されないことをテストで示す
  - _Depends: 4.1, 4.2, 4.8, 5.1, 5.2_
  - _Requirements: 8.2_

- [x] 7.3 論理エントリ集合の公開契約を結線する
  - モデルとパート集合を ZIP を経由せずに相互変換する公開経路を提供する
  - 開く経路と同一の検証が適用されることを、検証ロジックを 2 箇所に持たない形で保証する
  - `version-control` がこの経路のみでドキュメントの中身に到達できることをテストで示す
  - _Depends: 4.1, 4.6, 4.7, 4.8, 6.2_
  - _Requirements: 2.2, 2.3_

- [x] 7.4 変換後の初回保存における退避を実装する
  - 読み込み時に形式変換が適用された場合、その後の初回保存で変換前のファイルを退避として残す
  - 変換が発生しなかった場合に退避が作られないことをテストで示す
  - _Depends: 6.2, 7.2_
  - _Requirements: 6.4_

- [x] 8. Validation: 横断的な検証
- [x] 8.1 全経路の往復同一性を検証する
  - モデルからパート集合、コンテナ、再びパート集合、モデルへと一巡して完全に同一のモデルが復元されることを検証する
  - 保存したドキュメントを開き直すと元のモデルと一致することを検証する
  - ZIP を経由する経路と経由しない経路が同じモデルを与えることを検証する
  - _Depends: 7.1, 7.2, 7.3_
  - _Requirements: 2.2, 2.3, 4.1_

- [x] 8.2 決定性を検証する
  - 同一ドキュメントを 2 回保存してバイト単位で一致することを検証する
  - CI マトリクスで Linux / macOS / Windows の出力が一致することを検証する
  - _Depends: 1.2, 7.2_
  - _Requirements: 3.1, 3.2_

- [x] 8.3 (P) 行粒度の差分を検証する
  - 10 万行のうち 1 行のセルを変更して保存し、出力テキストの差分が 1 行のみであることを検証する
  - _Depends: 7.2_
  - _Requirements: 3.5_
  - _Boundary: RowsCodec_

- [x] 8.4 (P) 破損の検出を検証する
  - ダイジェスト改竄、識別子重複、宙吊りの型定義参照と添付参照、スキーマ欠落、マニフェスト欠落の各シナリオを検証する
  - すべてのシナリオで対応するエラー変種が返り、モデルが返らないことを検証する
  - _Depends: 7.1_
  - _Requirements: 1.7, 4.2, 4.3, 4.4, 4.5, 5.3, 5.4, 7.4_
  - _Boundary: StructuralValidator, IntegrityVerifier_

- [x] 8.5 (P) 不正アーカイブの拒否を検証する
  - 許可リスト外のエントリ名、ルート外を指すパス、同一パスの重複エントリを持つ ZIP が拒否されることを検証する
  - 極端に膨張するアーカイブが拒否されることを検証する
  - _Depends: 7.1_
  - _Requirements: 2.5, 2.6_
  - _Boundary: EntryLayout, ContainerCodec_

- [x] 8.6 (P) 原子的保存の中断耐性を検証する
  - 一時ファイル書き込み後・置換前にプロセスを落とし、元ファイルが保存前の内容のまま残ることを検証する
  - _Depends: 7.2_
  - _Requirements: 5.6_
  - _Boundary: AtomicWriter_

- [x] 8.7 (P) 移行のゴールデン fixture を追加する
  - 形式バージョン v1 のゴールデン fixture を追加し、現行として読めることを検証する
  - 変換が発生した場合に初回保存で退避が残ることを検証する
  - 以降のバージョン追加時に fixture の追加が必要であることを、テストの構造で強制する
  - _Depends: 7.4_
  - _Requirements: 6.2, 6.3, 6.4_
  - _Boundary: MigrationChain_

- [x] 8.8 (P) 添付の往復と参照整合性を検証する
  - 添付のバイト列が往復で 1 バイトも変わらないことを検証する
  - どの行からも参照されない添付が削除されず一覧に現れることを検証する
  - 行から添付への参照が往復で保たれることを検証する
  - _Depends: 7.1, 7.2_
  - _Requirements: 7.1, 7.3, 7.5, 7.6_
  - _Boundary: AttachmentRegistry_

- [x] 8.9 性能ベンチマークを追加する
  - 10 万行 × 30 列のドキュメントで、開く操作が 3 秒以内、保存が 2 秒以内であることを計測する
  - SSD を搭載した 4 コア以上の環境を計測条件として記録する
  - 10 万行を超えるドキュメントが拒否されず、保証対象外の通知が立つことを検証する
  - ベンチマークを CI に組み込み、予算超過を機能追加と同時に検出できるようにする
  - _Depends: 1.2, 7.1, 7.2_
  - _Requirements: 8.1, 8.2, 8.3, 8.5_

## Implementation Notes
- 実行環境: ホストに C ツールチェーンが無い（gcc なし・sudo なし）。`~/.local/bin/cargo` は podman イメージ `localhost/rustdev:1.98`（rust 1.98 + gcc）内で cargo を実行するシム。すべての cargo コマンドはリポジトリルートから実行すること（シムが git toplevel を同一絶対パスにバインドマウントする）。コンテナ内のファイルは rootless podman によりホストユーザー所有で作られる
- zip 8.6.0 の素の `deflate` feature は `[deflate-zopfli, deflate-flate2-zlib-rs]` のバンドルで zlib-rs を誘発する。`deflate-flate2` のみを使うこと（`deflate-zopfli` / `deflate-flate2-zlib-rs` を有効化する feature 変更は miniz_oxide 固定とゴールデンテストを壊す）
- criterion 0.8: `criterion_main!` は群名必須、`criterion::black_box` は非推奨で `std::hint::black_box` を使う
- ulid は 3.0 に解決済み（design は major 未固定）。ulid 3 の API で実装すること
- CellValue wire 規約（task 1.5、value.rs）: 文字列の判定制御は 64 文字英小文字 hex → Attachment、十進文法 → Decimal、その他 Text。誤解釈される Text と十進文法外 Decimal は `{"$t":"text"|"decimal","v":...}` エスケープ。Nested の `$` 始まりキーは `$$...` にエスケープ（書き手が生の `$` キーを出力しないことが全エスケープの前提）。i64 範囲外整数リテラルは prescan で `InvalidContainer`（serde_json が範囲外整数を黙って f64 に落とすため）
- `serde_json` は `float_roundtrip` feature が必須（既定のパーサは約 30% の f64 で 1 ULP ドリフト。レビューの probe で確認）。外すと読み込み時に値が壊れる
- `[lib] bench = false` が必須（crates/document-format/Cargo.toml）: libtest 自動ベンチが `cargo bench --workspace -- --save-baseline=...` のファンアウトで criterion フラグを弾き、ベンチ経路がexit 101 になる（task 1.2 で発覚）。決定的ベンチは `[[bench]]` ターゲットのみで運用する
- `serde_json` に `raw_value` feature を追加済み（task 2.2）: `serde_json::value::RawValue` は非既定 feature であり、不透明なスキーマ・ペイロードのバイト verbatim 保持に必須。**`preserve_order` は絶対に有効化しない**（キー順序の決定性 = 要件 3.3 が壊れる）。`Value` は引き続き禁止
- task 2.2 が `schemas/<sheet-ulid>.json` のエンベロープ形 `{"root": <opaque>, "types": [{"id": <ULID-26>, "definition": <opaque>}]}` を確定させた（design は形状未規定）。task 4.4 の SchemaCodec はこの定義を引き継ぎ、最終的な決定的符号化とバイト再構成を担当する
- `SchemaPart::type_ref_targets()` は参照**先**の生テキストのみを返し参照元情報を持たない。task 4.7 で `DanglingTypeRef { from, to }`（design エラー表）を組むには、`root()` / `TypeDef::definition()` の `RawJson` を再走査して参照元を復元する必要がある
- スキーマ層で解析失敗を表現する既定規約（task 1.5 の `value.rs` と同一）: 型付き変種を持たないコンテンツ解析失敗は `DocumentError::InvalidContainer` の `entry` に「失敗箇所のラベル + 理由」を文字列で載せる。診断は `schemas entry: root` / `schemas entry: type <ULID>` の形
- task 2.3 でモデルに**セル値の設定経路**が入った: `Document::set_row_values(sheet, row, Vec<CellValue>) -> Result<(), UnknownRow>`（`Sheet::set_row_values` / `Row::set_values` は `pub(crate)`）。要件 7.6「未参照添付の一覧」を公開 API 経由で観測可能にするために親が承認した越境で、task 4.5（行データの復号）はこの経路で行を組み立てる。**未知シートも `UnknownRow { row }` として報告される**（戻り型を単一化した帰結。呼び出し元はシート誤りと行誤りを区別できない）
- 添付レジストリの反復順は `AttachmentId` の昇順（`BTreeMap` による構造保証）。登録順・行順に依存しないため、task 4.8 の `attachments/<hex64>.bin` エントリ順（エントリ名昇順）と自然に一致する
- **添付の削除・prune・gc 経路は存在しない**（要件 7.5 / 7.6 の「自動的に削除しない」を API 不在で表現）。後続タスクで添付の整理機能を足す場合は本要件との衝突を先に解消すること
- `Attachment::bytes()` は `&[u8]` を返す（`Vec<u8>` のコピーを作らない）。添付は非 UTF-8・NUL 込み・既圧縮バイト列をそのまま保持し、内容を検査・変換・再圧縮するコードを一切持たない
- レビュー教訓（task 2.3 で実際に検出された欠陥）: 「全シート走査」のような横断集計のテストは、**後続要素のみが参照するデータ**を含めないと `take(1)` 変異を検出できない（初回提出は 87 テスト全緑のまま変異が通過し REJECTED）。同種の検証（task 8.1〜8.8）では、対象を 2 つ以上に分散させたうえで**変異を入れて落ちることを実測**してから完了とすること
- task 3.1 で `src/json/` が入った: 構造体の書き出し口（宣言順がそのまま出力順）と、**順序付き列のオブジェクト書き出し口**（呼び出し元が与えた列順序を一切並べ替えない。本クレートはスキーマを解釈しないため列順序の供給はスキーマ側の責務）。`serde_json` の使用は `to_writer` / `to_vec` のみで、`serde_json::Value` / `HashMap` は経路に存在しない
- **`write_json<T: Serialize>` に生の `f64` フィールドを置いてはならない**: `serde_json` の `serialize_f64` は NaN / ±Infinity をエラーにせず `null` として書き（`{"n":null}`、書き出しは成功する）、生 `f64` の `-0.0` は符号付きのまま出る。数値は必ず `CellValue` として運び `NonRepresentableNumber` で拒否させること。パート層（4.1 / 4.3 / 4.6）の出力構造体を設計するときに生 `f64` フィールドを作らないこと
- `-0.0` の `0` 正規化と NaN / Inf の遮断は `value::to_json_bytes` が単一の源。`json` 層はこれを再利用し再実装しない（第二の規約を作らない）
- テスト作法: 環境変数で分岐して自分自身のテストバイナリを子プロセス再実行するテストは、**変数の値の厳密一致**で分岐すること（存在のみで分岐すると外部環境に同名変数がある場合に assertion が空振りして緑になる。task 3.1 のレビューで実測再現された）
- task 3.2 で未知フィールド保持のプリミティブが `src/json/determinism.rs` に入った（`PreservedFields` / `PreservedField` / `PreservingObjectWriter`。値は `RawValue` の verbatim バイト列、位置は**既知キー前方カウント** `preceding_known_fields`。`json` 経由で公開、doctest 付き）
- **パート構造体（4.2 / 4.3 / 4.4 / 6.2）がこのプリミティブを使うときの必須条件**（task 3.2 レビュー由来。`determinism.rs` の doc にも記載）: (1) `record_known_field` を呼ぶキー列と `write_known` を呼ぶキー列を 1 対 1・同順に保つ（件数がずれると差し戻し位置がずれる。フィールド自体は失われない）、(2) 条件付きで省略する既知フィールドは読み書きで扱いを揃える、(3) `PreservedFields` は 1 読み込みにつき 1 個（読み込みをまたいで再利用しない）、(4) `write_known` は宣言順に呼ぶ（順序を変えると要件 3.3 に反する）、(5) 失敗時に 1 バイトも書かない規律を継承（一時バッファ + `finish` 一括書き出し）
- バイト単位往復の成立条件は「**コンパクト入力 + 既知フィールドが宣言順**」。キーと値の間の空白は値の一部ではないため消え、キーはデコード済みテキストとして保持されるためキー側の `\uXXXX` 表記は正規化され得る（**値だけが表記まで verbatim**）。ゴールデン fixture は必ず本クレートの書き出し形式で作ること
- 未知フィールド保持の機構は **task 4.4 で一本化済み**: 旧 `SchemaPart` の `RawField` / `unknown_fields`（エンベロープ・トップレベル限定・位置情報なし。task 2.2）は削除され、`src/json/determinism.rs` の `PreservedFields` / `PreservingObjectWriter`（位置付き）が唯一の機構である。**スキーマ・エンベロープも全階層（トップレベル + 各型定義要素）で未知キーを位置ごと保持し書き戻す**（型定義要素の未知キーを拒否していた 2.2 の挙動は要件 6.2 / 6.3 に忠実な側へ変更済み）。新しいバージョン付き構造を作るタスク（6.1 / 6.2）も同じ機構だけを使うこと
- task 3.3 で NDJSON コーデックが `src/json/ndjson.rs` に入った（`json::write_ndjson` / `json::read_ndjson`。行末は常に LF で**最終行も終端**、順序を並べ替えない、失敗時は 1 バイトも書かない、行番号 1 始まり付き `InvalidContainer`）。`\r` は正規化しない（出力に `\r` は出さず、読みは行内 `\r` を JSON 空白として通す。doc とテストで固定）
- **task 4.5（行データパートの符号化）が満たすべき必須条件**（task 3.3 レビュー由来）:
  - **i64 範囲外整数の門が必須**: `json::read_ndjson` は汎用コーデックなので整数リテラルの範囲検査をせず、`{"label":-9223372036854775809}` → `Float(-9.223372036854776e18)`、`99999999999999999999999999` → `Float(1e26)` と**黙って f64 に落ちる**（`9223372036854775808` だけは `visit_u64` が拒否する）。task 1.5 の `value::from_json_bytes` は 3 例とも `InvalidContainer` で拒否する。4.5 はセルの原文を `value::from_json_bytes` に通す門を必ず設けること（`check_integer_literals` は `value.rs` 非公開のため `RawValue` 等で原文を捕捉する必要がある）
  - `Sheet::rows()` の保持順（`reorder_rows` 後を含む）を**そのまま** `write_ndjson` に渡し、ULID 昇順・挿入順でソートしない。検証は「ULID 昇順とも追加順とも異なる並び替え後」で行い、行順一致と「並び替えが行 ID を変えずテキスト行の移動として現れる」ことを実測する（要件 1.5 / 3.5）
  - 10 万行で復元順 = `Sheet` の行順。行への値の設定は `Document::set_row_values` 経由（未知シートも `UnknownRow` に単一化される既知の制約を踏む）
  - 行ファイル末尾に**自前の `\n` を足さない**（`write_ndjson` が LF 終端済み。二重改行は空行 = 読みで `InvalidContainer`）
  - `location` に `sheets/<sheet-id>.jsonl` を渡し、行番号付き `InvalidContainer` を保つ
  - 全 `null` 行・空列を間引かない（1 行変更 → 1 行差分が崩れる）
- task 4.2 で `parts/` 層が新設され、`ManifestPart`（`manifest.json` の唯一の権威ある索引 = 形式バージョン + エントリ名→BLAKE3 ダイジェスト）が入った。`entry_name.rs` に `pub const MANIFEST_ENTRY` を追加済み（**"manifest.json" の文字列リテラルを実装経路に散在させないこと**。索引はエントリ名昇順、`manifest.json` 自身の索引掲載と索引内の重複エントリ名は拒否、`ManifestPart` 自身のダイジェストは自己記録しない）
- **未知フィールドの保持範囲（task 4.2 の裁定）**: design の `DeterministicJson` の責務にスコープ限定が無いため、**トップレベルだけでなく索引要素（`{"name":..., "blake3":...}`）の中の未知キーも位置ごと保持し書き戻す**。新しく「バージョン付きの JSON オブジェクト構造」を作るタスク（4.3 / 4.4 / 6.1 / 6.2）も、**その構造の全階層**で同じ保持を適用すること（要素ごとに `PreservedFields` を 1 つ持たせる）。対象外は (a) 未知キーの `\uXXXX` 等のエスケープ**表記**、(b) 値・区切りの外側の空白、の 2 点のみ
- **`PreservedFields` を保持する型に `#[derive(PartialEq)]` を後付けしないこと**（task 4.2 の申し送り）: `PreservedFields` は内部カーソル `known_seen` 込みで `PartialEq` を導出しているため、保持フィールドが空同士でも `record_known_field` の呼び出し回数が違うと**不等**になる（`src/json/determinism.rs`）。task 4.8（モデル ⇄ パート集合）と task 8.1（往復同一性）でパート集合の等価比較が必要になったら、`entries()` の写像か符号化バイト列で比較すること
- task 4.2 の `ManifestPart` / `ManifestEntry` は `PartialEq` を実装しない（上記カーソル事情。`ManifestEntry` は `Vec` を保持するため `Copy` も無い）。比較は `version()` / `entries()` / 符号化バイト列で行う
- task 4.3 で `parts/document_part.rs` と `DocumentId`（`ids.rs`。`IdFactory::new_document_id()` は既存 3 種と同じ単調カウンタを共有）が入った。`document.json` の確定形は **`{"document_id":"<26 文字 ULID>","sheets":[{"sheet_id":"<ULID>","name":"<文字列>"}]}`（compact・末尾改行なし）**（空 `sheets` 配列形も許容）
- **順序規則は 2 つで逆なので混同しないこと**: `document.json` は**与えられたシート順序をそのまま**保持（ソートしない。シート順序がそれ自体データ = 要件 1.1）、`manifest.json` は**エントリ名昇順**にソート（索引だから）。この区別は両ファイルの doc と `parts/mod.rs` に明記済み
- **task 4.8（モデル ⇄ パート集合）が着手前に解消すべき制約**（task 4.3 レビュー由来。実測に基づく）:
  - `model::Document` は**ドキュメント識別子を持たない**（`ids: IdFactory` と `sheets` のみでアクセサも無い）。`DocumentPart::new` は `DocumentId` 必須なので、4.8 は (a) model に識別子フィールドと `document_id()` を足す、(b) `IdFactory` を公開する、(c) parts 層で発行する、のいずれかを選ぶ必要がある。**(a) 以外は `Document → Parts → Document → Parts` の往復で識別子が再発行され、要件 3.1（同一内容 → 同一バイト）を壊すため (a) が唯一整合する**
  - `model::Document` / `Sheet` に**未知フィールド保持の領域が無い**（`unknown_fields` を持つのは `SchemaPart` のみ）。`document.json` はトップレベルとシート要素の両方で未知フィールドを保持する契約なので、4.8 は Model を経由する往復で前方互換データが落ちないよう保持領域を model 側に用意する必要がある
  - `Document::sheets()` の反復順を 1 対 1 で `SheetMeta` 列へ写すこと（`add_sheet` の末尾追加・`remove_sheet` の順序保存と整合）。`Sheet::name()` は無変換で写せる
  - `DocumentPart::new` / `from_json_bytes` は `Result` を返す（重複シート識別子で `InvalidContainer`）ため、**4.8 の `to_parts` は infallible にできない**
  - `document.json` 内の重複検出は同一ファイル内の自己矛盾に限る。行・型定義・添付をまたぐ全体一意性は task 4.6 の担当
- task 4.4 で `parts/schema_codec.rs` の `SchemaCodec`（`SheetId` + `SchemaPart` ⇄ `schemas/<sheet-ulid>.json` エンベロープ）が入った。`EntryName::Schema` は既存だったため `entry_name.rs` は無変更。エンベロープの確定形は `{"root": <opaque>, "types": [{"id": <ULID-26>, "definition": <opaque>}]}` のまま
- **パート層の公開面は `parts::` 配下に統一する**（`parts::ManifestPart` / `parts::DocumentPart` / `parts::SchemaCodec`）。クレート根には出さない（モデル層とエラーは根、パート層は `parts` 配下）。新しいパート（task 4.5 の `RowsCodec`）も `parts::` 経由で公開すること
- **`SchemaPart` / `TypeDef` は `PartialEq` を持たない**（保持カーソルが等値比較に混じるため。task 4.1〜4.4 の一貫した方針）。等価比較は「`root()` のバイト列 + 各型定義の `id()` / `definition()` の個別比較」または「符号化したバイト列の比較」で行う。**`type_defs()` 同士を直接 `==` では比較できない**（コンパイルエラーになる）
- 往復バイト一致の成立範囲は task 3.2 / 4.2 / 4.3 と同じ（コンパクト入力 + 既知キーが宣言順）。スキーマ・エンベロープの既知キー宣言順は `root` → `types`、型定義要素は `id` → `definition`。`id` は正準大文字 ULID で書くため、小文字表記の入力は表記のみ正規化される（受理規則は不変）
- **行データエントリの wire 形式（task 4.5 が確定。`sheets/<sheet-ulid>.jsonl`）**: 1 行 = フラットな JSON オブジェクト `{"$id":"<ULID-26>", <列名>: <値>, ...}`。`$id` が先頭、続いて呼び出し元が与えたスキーマ列順序。`$` 始まりの列名は先頭 `$` を 1 つ足して書く（`$foo` → `$$foo`）、`$id` は完全一致のみ予約キー。**1 シート内の全行が同一のキー列**でなければならず、逸脱は行番号付き `InvalidContainer` で拒否（穴埋め・並べ替え・黙落としをしない）。復号の列順序は**ファイル自身のキー順序**から決める（本クレートはスキーマを解釈しないため）。復号は**書き手の像に含まれないキー**を拒否するので、`encode(decode(W)) == W` は本クレートが書いた形式の入力に対して全域
- 行データの公開面は `parts::{RowsCodec, SheetRows, RowsEncodeError}`。`RowsCodec::encode(SheetId, &[String] /*列名*/, &[Row])` / `decode(&EntryName, &[u8])`。**0 行のエントリは列順を復元できない**（`columns()` が空。情報が無いことによる不可避の損失）
- **行データの wire 形式は `export-templates` の再検証トリガ**（design Revalidation Triggers「行データエントリの形式（NDJSON）の変更」）。下流スペックに伝えること
- **task 4.8 / 7.1 が踏む性能の罠（task 4.5 レビューで実測）**: `Document::set_row_values` / `Sheet::set_row_values` は行ごとに線形探索するため **O(n²)**（10 万行の構築に実測 34.5〜36.45s。要件 8.1 の「10 万行を 3 秒以内に開く」を単独で破る）。`Document::add_row` は識別子を再発行するため復号済み行の投入に使えない（要件 1.4 / 3.1 違反）。**クレート可視の `Row::new` + `Row::set_values` + `Sheet::push_row`（O(1)）で投入し、シート探索は行ループの外へ出すこと**。`RowsCodec::decode` は値入りの `Vec<Row>` を返すのでそのまま積める
- 行の列の同一性は**列名**で決まる（位置ではない）。本クレートはスキーマを解釈しないので、列名の供給（書き込み時）と列順の解釈（読み込み時）はこの wire 形式とファイル自身のキー順序で完結する
- task 4.6 で `parts/validate.rs` の `StructuralValidator` / `PartInventory` / `IdDeclaration` が入った（**識別子の一意性**と**スキーマ存在**の検証のみ。参照整合性は task 4.7 が同じファイルに追加する）。入力は復号済みパート群の目録（種別 + 識別子テキスト + 出現箇所、要求シート、スキーマ保有シート集合）で、**モデルに依存しない**（検証はモデル構築の前に完了する）。報告順は doc に確定形として明記（重複 = `IdKind` 宣言順 → 識別子テキスト昇順 → 出現箇所は目録順、スキーマ欠落 = 文書順）
- **スキーマ存在の裁定（task 4.6）**: 行データを持たないシートでもスキーマを要求する（要件 1.2 / design 不変条件「各シートはちょうど 1 つのルートスキーマを持つ」の帰結。要件 4.4 の文言より強い側）。`AttachmentId` の重複検査は実運用では到達しない防御的検査（content-addressed + コンテナ層とマニフェストが同一エントリ名を拒否済み）
- **task 4.7 への申し送り**: 型定義参照の「同一シートの型定義集合に実在」判定には**宣言の所属シートが要る**ため、`IdDeclaration` に所属シートを **additive に追加**すること。また `SchemaPart::type_ref_targets()` は参照**先**の生テキストのみを返し参照元情報を持たないので、`DanglingTypeRef { from, to }` を組むには参照元を復元する必要がある（既存の `RefScan` を**拡張**し、走査器を二重に持たないこと）
- task 4.7 で `parts/validate.rs` に参照整合性が additive に入った: (1) 型定義参照は**同一シートの型定義集合**に実在（別シートのみに在る型定義を指す参照も宙吊り）、(2) 添付参照は `Nested` を再帰的にたどって実在を確認、**実在するが未参照の添付は破れではない**、(3) 未知のシートを指すパートは `InvalidContainer`（design に専用変種が無いため新変種を足さない）。報告順は「一意性（種別 → 識別子テキスト昇順）→ スキーマ存在（文書順）→ 参照（型定義 → 添付 → シート）」。`validate.rs` は `crate::model` に依存しない（**参照の目録を入力で受ける**）
- **識別子参照の解決規約（task 4.7 の裁定）**: 型定義参照の実在判定は、宣言側の id と参照先（`$ref`）のテキストを**双方** `TypeDefId` の解決（大小文字不問）で正準形に写してから比較する。解決できないテキストは原文のまま比較（実在しない扱い。新変種は足さない）。**`DanglingTypeRef { to }` にはファイルの原文を載せる**（診断の忠実性）。添付識別子は小文字 hex 固定で正規化不要
- `model::SchemaPart::type_refs() -> &[TypeRef]`（`TypeRef` は `from()` / `to()` を持つ名前付き型。`PartialEq` は derive しない）。**同じ 1 回の `RefScan` 走査が `type_ref_targets()` と `type_refs()` の両ビューを埋める**（走査器を二重化しない）。`type_ref_targets()` の意味・順序・重複保持は不変
- **task 4.8 への申し送り（task 4.7 レビュー由来）**: (1) 型定義宣言に `IdDeclaration::with_sheet` を与え忘れると、そのシートの参照が**宙吊りとして誤報**される（fail-loud な設計。誤りの向きが「妥当な文書の拒否」なので、`DocumentParts` からの流し込みで `Ok(())` になる end-to-end テストを 4.8 の受入に含めること）、(2) `validate.rs` は `model` 非依存を保つため、`SchemaPart::type_refs()` 等からの**参照抽出は model / parts 側で行い、テキストの目録として検証器へ渡す**こと
- **列順序の永続化（task 4.8 で解消する format 決定。親の裁定）**: 行データは列名をキーとするため（task 4.5 の wire 形式）、**書き出し時に行の列名一覧が必要**であり、design の `DocumentFormatApi::to_parts(document)` / `save(document, path)` は列名を外から受け取らない。したがって**列名（順序付き）は `Document` 側が保持**しなければならない。さらに 0 行のシートは行エントリから列順を復元できない（task 4.5）ため、**`document.json` のシート要素（`SheetMeta`）に順序付き列名を永続化**する（design の「`document.json` = ドキュメント ID、シート順序、シートのメタデータ」に含まれる）。よって 4.8 は (a) `Sheet`（model）と `SheetMeta`（parts）に順序付き列名を additive に追加し、(b) `document.json` の確定形を更新（4.3 の確定形テストを追随）し、(c) 行エントリの列順序が `document.json` の列名一覧と一致することを検証する（不一致は `InvalidContainer` の `entry` にエントリ名と理由。新しい変種は足さない）。**本クレートは列名の中身を解釈しない**（単に運ぶだけ。schema-engine が決める）
- 本クレートはスキーマを解釈しないため、**列名の供給（書き込み時）と列順の解釈（読み込み時）は `document.json` と行エントリのキー順序で完結**する
- task 4.8 でパート層の公開契約が確定した: `parts/document_parts.rs` の `DocumentParts` / `Part { name, bytes, digest }` / `to_parts` / `from_parts`。不変条件は「エントリ名昇順の決定的反復」「全 `Part` の `digest` が `bytes` と一致（`integrity::digest_part` 経由）」。`ZIP` の知識も `std::fs` の列挙も持たない
- `from_parts` の処理順は **manifest 解決 → 完全性照合（`IntegrityMismatch` / `MissingPart`）→ 復号 → 構造検証（`StructuralValidator`。型定義宣言に `with_sheet` 必須）→ 行の列順序と `document.json` の列名一覧の一致検証 → モデル構築**。**すべての検証がモデル構築の前に完了**し、失敗時に部分的モデルを返さない（要件 5.4）。行の投入は `SheetRows::into_rows()`（所有権移動）+ `Sheet::extend_rows()` の一括経路で、`Document::set_row_values` を行ごとに呼ばない（O(n) を実測: 2 万行 351ms）
- **`document.json` の確定形が変わった（task 4.8）**: シート要素は `{"sheet_id": <ULID>, "name": <文字列>, "columns": [<列名>...]}`。**`columns` は必須キー**（0 列でも `"columns":[]` を書く）。キー順は `sheet_id` → `name` → `columns`。0 行シートの列名はこの経路でのみ往復する（行エントリからは復元不能）
- **`jxcel`（型マーカー）は `DocumentParts` に含めない**（構築時に `InvalidContainer` で拒否）。コンテナ層が `Stored`・先頭エントリとして自分で書き、復号時に集合から外す。**`manifest.json` の索引にも `jxcel` を含めない**
- **task 5.2 / 5.3 が従うべき契約（task 4.8 レビュー由来）**: (i) 符号化はマーカーを `Stored`・先頭に自身で書き、その後に `DocumentParts::iter()` の昇順で各パートを `Deflate` で書く、(ii) 復号はマーカーを外してから残りを `(EntryName, 展開後バイト列)` として `DocumentParts::from_entries` に渡す（マーカーを渡すと拒否される = 意図どおり）、(iii) **ダイジェストは非圧縮（展開後）バイト列から算出**されるので照合も展開後のバイト列で行う、(iv) `decode(encode(p)) == p` は「マーカー除去 + 展開後バイト列」で成立する（`DocumentParts` に `PartialEq` は無いので等値確認は `iter()` の名前・バイト列比較で行う）、(v) 許可リスト（要件 2.5）と同一パスの重複拒否（2.6）はコンテナ層で先に適用する、(vi) 「実体 → 索引」の監査（索引に無い実体パートの拒否）を足すかは 5.3 の判断（現状は受理される。design の「manifest が唯一の権威ある索引」を厳格に読むなら 5.3 で追加し、受理範囲が狭まることを記録する）
- **task 6.1 / 6.2 への申し送り**: `DocumentParts::format_version()` は索引の記録値をそのまま返す（ゲートは無い。現行 1.0 以外も `from_parts` は読もうとする）。バージョンゲート（新しい major は `UnsupportedVersion`、古い形式は移行チェーン）を `from_parts` の前段に置くか内部に挿すかは 6.1 で決めること
- **task 7.x への申し送り**: `open` は `from_parts` と同じ検証経路を通す（検証を二重化しない）。公開面は `parts::` 配下の `to_parts` / `from_parts` / `DocumentParts` / `Part` を再輸出するだけで足りる
- **task 8.9 の計測対象**: 添付バイト列は `from_parts(&DocumentParts)` の借用契約により 1 回複製される（巨大添付でピークメモリが二重）。`DocumentParts` は全パート分の `Vec<u8>` を保持する
- task 5.1 で `container/atomic_save.rs` の `AtomicWriter::commit(target: &Path, bytes: &[u8])` が入った。一時ファイルは**対象と同一ディレクトリ**（prefix `.jxcel-tmp-`、対象名から作らない）、順序は `write_all` → `sync_all` → `rename` → 親ディレクトリ fsync（Unix）。Windows は再試行コード `[5, 32, 33]`・`RENAME_ATTEMPTS = 10`・バックオフ 1ms から倍々・上限 200ms。`retried = true` は**再試行予算を使い切った場合のみ**（それ以外の失敗では `false`）
- **不変条件「`Err` を返すなら対象は保存前のまま」を守ること（task 5.1 の裁定）**: design の保存フロー事後条件（`Err` ⇒ `path` は保存前の内容）と design エラー表の `Io` 応答「既存ファイルは無変更（5.6）」を守るため、**`rename` 成立後は `Err` を返さない**。親ディレクトリの同期は試行するが**失敗は握り潰して `Ok(())`**（失われうるのは置換の永続化だけ。**部分的に書かれたファイルにはならない**）。この判断と caveat は `atomic_save.rs` の doc に明記済みで、旧挙動（置換後の同期失敗を `Err` にする）は変異としてテストが検出する。**後続タスク（5.3 / 7.1 / 7.2）も「`Err` ⇒ 対象不変」を前提にしてよい**
- 一時ファイルを使う保存を書くときの検証限界（task 5.1 レビューの実測）: 「ファイルの `sync_all` を外す」「親ディレクトリの fsync を外す」の 2 変異は**テストでは検出できない**（電源断を模擬しない限り耐久性は観測不能）。順序は `strace` 等の外部トレースで確認するほかない。テストで殺せるのは構造（同一ディレクトリ性・`rename` によるエントリ差し替え・失敗時の対象不変・一時ファイル非残留）までである
- `AtomicWriter` は**モード（パーミッション）を継承しない**（`fs::write` 相当の既定）。シンボリックリンクはリンク自体が置換される。これらは doc に明記済み
- task 5.2 で `container/writer.rs` に `ContainerCodec::encode(parts: &DocumentParts) -> Result<Vec<u8>, DocumentError>` が入った（無状態の inherent 型 + 関連関数。design の trait 形ではない）。**task 5.3 は同じ型に `decode` を追加する**（型を分けない）。決定性パラメータは `FileOptions::DEFAULT`（const。**`default()` は使わない**）から `MARKER_OPTIONS` / `PART_OPTIONS` を組み立てて明示固定: 更新日時 `DateTime::DEFAULT`（1980-01-01、MS-DOS 生値 `0x0000` / `0x0021`）、unix permissions `0o644`、`System::Unix`（ホスト OS バイト 3）、圧縮レベル 6。マーカーは `Stored`・先頭、他は `Deflate`、出力は `Cursor<Vec<u8>>`（**データディスクリプタ不使用**）。失敗は `DocumentError::Io { source, retried: false }`
- **型マーカーの内容は `jxcel\n<major>.<minor>\n` で、バージョンは `parts.format_version()`（= 索引 `manifest.json` の記録値）から導出**する（`marker_bytes(version: FormatVersion)`）。**第二のリテラルを持たない**。マーカーは固定オフセットでの早期判定用の写しであり、**権威は常に索引**（正式なゲートは 6.1）。両者の一致は `the_type_marker_carries_the_manifest_format_version` が `zip::ZipArchive` 経由で値比較して固定する
- **ゴールデン `tests/fixtures/bytes/golden_container.zip`（2125 バイト、sha256 `479b506b…`）が決定性の最終防衛線**。実文書（`DocumentPart` / `SchemaCodec` / `RowsCodec` / `ManifestPart` を公開 API から構築、6 エントリ）を符号化したバイト列で、テストはファイルのバイト列を期待値として読む（**実装の出力から期待値を組み立てない**）。`flate2` / `miniz_oxide` を更新したときに差分がここで顕在化するので、**再生成が必要になった場合は一時的な `#[ignore]` テストで作り直し、生成コードを残さない**（手順はテスト doc に記載済み）
- **検証限界（task 5.2 レビューの実測）**: 明示固定を外しても**この環境では crate 既定値が同じ値になるため生存する変異**が 4 つある（`.last_modified_time` / `.unix_permissions` / `.system` / `.compression_level` の明示を外す）。値そのものは統合テストの**生ヘッダのリテラル期待値**とゴールデンが固定しており、`System` の明示は Windows でのみ差が出る（`time` feature 有効時は既定が現在時刻になる）。**「今は同じ値だから」を理由に明示指定を削除しないこと**（design は「crate の既定値に依存しない」と要求している）。テストが殺せるのは構造（順序・マーカーの位置と圧縮方式・日時/permissions の値・`encode` の再現性・標本間の一致）までである
- **変異検査の手順（task 5.2 で 1 人目のレビュアーが変異を適用したまま中断し、親が復元する事故が起きた）**: 変異は必ず「バックアップ → ハッシュ記録 → 変異 → 観測 → **バイト単位で復元 → ハッシュ一致確認**」の順で 1 つずつ行い、**終了前に `git status --porcelain --untracked-files=all` が期待どおりであること**を確認する。変異観測は `cargo test -p document-format ...` に限定してよい（`--workspace` を毎回回すと時間とコンテキストを浪費する）
- task 5.3 で `container/reader.rs` に `ContainerCodec::decode(bytes: &[u8]) -> Result<DocumentParts, DocumentError>`、`container/layout.rs` に復号時の許可リスト適用（`admit`。**`zip` 非依存**）が入った。判定順は ① ZIP を開く（失敗は `InvalidContainer { entry: "<archive>: <理由>" }`）→ ② 許可リスト照合（`EntryName::parse` が唯一の権威。**正規化も `enclosed_name()` も使わない**）→ ③ 同一パスの重複検出（**バイト単位完全一致**、名前の列だけで判定）→ ④ 型マーカー（実在・固定形・索引の記録値との一致。**位置と圧縮方式に依存しない**）→ ⑤ 宣言サイズを上限とした有界読みと一致要求 → ⑥ `DocumentParts::from_entries`（`jxcel` は渡さない）。**展開（解凍）は ②③ の後にしか始まらない**
- **`zip` 8.6 の `ZipArchive` は同一パスのエントリを `IndexMap` で黙って畳む**ため、公開 API からは重複を観測できない。重複検出だけは `central_directory_start()` からの生バイト走査で行っている（これが唯一の手書き ZIP 解析。`zip` の視界の上位集合を同一整列で読むため**重複の見逃しは起きない**）
- 既知の制限（task 5.3 レビューの実測。**task 8.5 への申し送り**）: 生バイト走査は EOCD が宣言する `cd_size` / `cd_count` で有界化していない（`zip` 8.6 が両者を公開しないため）。中央ディレクトリと EOCD の間のアーカイブコメント等に `PK\x01\x02` の並びが現れると、**存在しないエントリを読んで過剰に拒否する**（可用性のみの劣化で、不正な受理にはつながらない）。8.5 で扱うこと
- **宣言サイズの出所は ZIP ヘッダの非圧縮サイズ**（task 5.3 の裁定）: `ManifestEntry` は `name` / `digest` / `preserved` のみでサイズを持たず、マニフェストにサイズ欄を足すのはワイヤ形式の変更でゴールデンを無効化するため行わない。**絶対的なサイズ上限は設けない**（要件 8.5 が「10 万行を超えても拒否せず、性能保証の対象外である旨を通知」と定めるため。DEFLATE の理論圧縮比上限により比判定も実効性がない）。「極端に膨張するアーカイブ」への防御は (a) 許可リスト・重複の判定が展開に先行、(b) `take(宣言 + 1)` の有界読み、(c) 宣言と実展開量の一致要求、(d) 後段のダイジェスト照合（4.1）の多層防御である。この判断は `reader.rs` の module doc に記録済み
- サイズ不一致の拒否は**自分の照合が発火する**（宣言を実より小さく書いた場合は `zip` の CRC 検証まで到達せず、報告される展開長が **宣言 + 1** で止まる。宣言を実より大きく書いた場合は CRC を通過してから自分の照合が拒否する）。`zip` の CRC 不一致が拒否するのは**無圧縮エントリを破壊した場合**のみで、その I/O エラーは `InvalidContainer { entry: "<名前>: <理由>" }` に写像される。破損エントリのテストは Deflated の破損だと自分のサイズ照合を叩いてしまうため、**`Stored` の CRC 経路**で書くこと
- task 6.1 で `FormatVersion` の定義を `error.rs` から `migration/mod.rs` へ移管し、`CURRENT_FORMAT_VERSION`（= 1.0）も `migration` の**1 箇所**に集約した（`parts/document_parts.rs` の非公開定数は廃止）。クレート根の `document_format::FormatVersion` は維持（既存テストがこれを使う）。`error.rs` ⇄ `migration` は相互参照になる（`error` は `UnsupportedVersion` の文脈型として `FormatVersion` を、`migration` は中止値として `DocumentError` を参照する）が、**design の依存グラフに `error.rs` は現れない共有の葉**でありコンポーネント間の依存方向は増えない。両モジュールの docs に明記済み
- task 6.1 のゲートは `migration::MigrationChain::{gate, admit}` と `VersionVerdict { Openable, NeedsMigration { from }, Unsupported { found, supported } }`。**major のみで判定**（同一 major は minor の新旧を問わず受理 = 省略可能フィールドの追加のみ）。呼び出しは `parts/document_parts.rs::from_parts` の**索引解決直後・ダイジェスト照合の前**に `MigrationChain::admit(parts.format_version())?;` の **1 回だけ**（design の読み込みフロー順。7.3 の「検証を 2 箇所に持たない」を満たす）。`from_entries`（構築側）はゲートせず、**`format_version()` は記録値をそのまま返す契約**（5.2 の型マーカーがこれに依存）
- **task 6.2 への申し送り（task 6.1 レビューで校正済み）**: 差し替えるのは `MigrationChain::admit` の `NeedsMigration` 分岐 **1 箇所**（`from_parts` の挿入行はそのまま）。ただし変換の適用は**ダイジェスト照合より前**（移行後のバイト列と索引のダイジェストの整合をどう取るかは 6.2 が決める。design の読み込みフローは「バージョンゲート → 移行 → ダイジェスト照合」）。古い major の**拒否**を固定していて更新が必要なテストは 3 本: `migration::tests::admit_opens_only_the_current_major`（0.9 行）/ `parts::document_parts::tests::from_parts_gates_on_the_major_boundary`（0.9 行）/ `tests/migration.rs::an_older_major_is_rejected_until_the_migration_chain_lands`（`migration::tests::an_older_major_needs_migration` は `NeedsMigration` の verdict のみを固定するので 6.2 でも妥当）
- task 6.2 で `migration/steps.rs`（`MigrationStep { from, to, rewrite }` の宣言。**実表 `STEPS` は空** = v1 のみ）と `migration/mod.rs` の適用機構（`apply`（公開）/ `apply_with`（`pub(crate)`）/ `plan` / `migrate`）が入った。**1 段ずつ適用**し、`from` 完全一致で鎖を組み、**連続性を検証**（隙間・後退・停滞・現行 major 飛び越し・同一 `from` の二重宣言は `InvalidContainer { entry: "migration: …" }`、移行先が無い版は `UnsupportedVersion { found, supported }`）。各段適用後に `manifest.json` の版を段の `to` へ更新し、**索引を実ダイジェストから再構築**する（`DocumentParts::reindex` → `ManifestPart::reindexed`（`pub(crate)`）。**索引トップレベルと要素内の未知フィールドを引き継ぐ**）。`MigrationChain::admit` は**廃止**（`gate` は不変）。段を追加する手順は `steps.rs` の module docs に記載
- **移行とダイジェスト照合の順序（task 6.2 の裁定。design の記述を精密化する）**: `from_parts` は「索引解決 → ゲート →（**移行する場合のみ**）**記録されたままの集合**を索引と照合 → 移行適用 → 移行後の索引で照合・構造検証」。design の「ゲート → 移行 → 照合」の順では移行が索引を無条件に作り直すため、**破損した古いファイルが黙って「修復」される**（要件 5.2 は古い形式にも適用。要件 5.5 の「自動修復・上書きの禁止」でも同じ結論）。この裁定と理由は `migration/mod.rs` と `parts/document_parts.rs` の両 module docs に明記済み。**移行が必要なときだけ**集合を所有して組み直し、**現行版の通常経路は複製も再ハッシュもしない**（`apply_with` が `Ok(None)` を返す）
- **task 7.1 への申し送り**: 開く経路は `MigrationChain::apply` を直接呼ばず、**`from_parts` を通す**こと（破損検出の先行照合は `from_parts` 側にあり、`apply` 直呼びでは迂回される）。**task 7.4 への申し送り**: `MigrationChain::gate`（`const fn`・副作用なし）で「移行が必要か」は事前判定できるが、「移行が成功するか（鎖が現行 major へ届くか）」は `apply` を呼ぶまで確定しない（空表の初版では `NeedsMigration` でも中止）。**task 8.7 への申し送り**: 実 fixture を `tests/fixtures/golden/v<major>/` に `<内容>.jxcel` の命名で追加し、過去版 fixture を現行版へ運ぶ往復比較を実装する（配置・生成・更新の規約は `tests/migration.rs::the_golden_fixture_directory_for_the_current_version_exists` の doc に記載済み）
- task 7.1 で公開 API 層が `src/lib.rs` に入った: `SUPPORTED_ROW_LIMIT`（= 100_000。要件 8.4 / 8.5）、`OpenOutcome { document, migrated_from, beyond_supported_scale }`、**トレイト `DocumentFormatApi`**（本タスクは `open` のみ。`save` / `to_parts` / `from_parts` は 7.2 / 7.3 が**同じトレイトへ加点**する。スタブは置かない）、具象型 `DocumentFormat`（`const fn new()`）。`open` の順序は ① `std::fs::read`（失敗は `Io { retried: false }`）→ ② `ContainerCodec::decode` → ③ `MigrationChain::gate`（`migrated_from` の事前判定）→ ④ `parts::from_parts`（ゲート・移行・ダイジェスト照合・構造検証・モデル構築の 1 経路）→ ⑤ 規模の通知。**検証ロジックをこの層に再実装しない**（各層の 1 経路を呼ぶだけ）
- **`migrated_from` は `MigrationChain::gate` の判定から導く**（`from_parts` のシグネチャを変えないため。`gate` は純粋なので 2 回呼んでよい）。写像は**クレート内部の純粋関数 `migrated_from_verdict`** に切り出し、3 分岐（`NeedsMigration → Some` / `Openable`・`Unsupported → None`）を単体テストで固定している（**初版は移行チェーンが空で `Ok` になる古い版が存在しないため、`migrated_from = Some` の成功経路は統合テストから観測不能**。これが理由でこの写像だけは単体テストで守る）。**task 7.4 / 8.7 への申し送り**: 段付きチェーン（または過去版 fixture）が入った時点で `Some` の成功経路の統合テストを必須にすること
- task 7.1 の公開 API テストは `tests/api.rs`（design のテストファイル一覧に Api 層の項目が無いための新設）。**非書き込み（要件 5.5）の検証は「対象ファイルのバイト列 + 作業ディレクトリのエントリ一覧」の両方を前後で比較**する形にすること（一時ファイルを作って消す実装でもディレクトリ mtime が変わるため、一覧だけでなく mtime まで見るのが望ましい）。規模境界（要件 8.5）は 1 シート 100_000 行 → `false` / 100_001 行 → `true` を**リテラルで**固定し、`SUPPORTED_ROW_LIMIT == 100_000` は別テストで固定する（**しきい値を期待値に使う自己参照テストにしない**）。行は O(1) の `Sheet::push_row` 系で積むこと（`set_row_values` は O(n²)）
- task 7.2 で公開 `DocumentFormatApi::save(document, path)` が入った。順序は design の保存フローどおり **① `parts::validate_document(document)?` → ② `parts::to_parts(document)?` → ③ `ContainerCodec::encode(&parts)?` → ④ `AtomicWriter::commit(path, &bytes)`**。**書き込みは ④ の 1 箇所だけ**で、①〜③ の失敗は書き込み前に起きる（＝既存ファイルはバイト列・inode・mtime とも不変）。`save` は `parts` / `container` の 1 経路を呼ぶだけで、検証・構築・符号化を Api 層に再実装しない
- **【事実の訂正。重要】モデルの不変条件は構築 API では強制されない**（当初「構築で強制されるので保存時に再検証しない」と裁定したが誤りだった。レビューで公開 API から違反状態を作れることを実測）: `SchemaPart::parse` は `$ref` の**実在を見ない**、`CellValue::Attachment` は**レジストリ登録を強制しない**、`SchemaPart` のペイロードは**同一 `TypeDefId` の重複宣言を排除しない**。したがって **`save` の第 1 段の不変条件検証は必須**であり、外すと「自分自身の `open` が拒否するファイルを正常終了で書き出す」状態に戻る（変異で検出できる）。構築で強制されるのは「各シートちょうど 1 つのルートスキーマ」「改名・並べ替えで識別子が不変」「`SheetId` / `RowId` の発行経路」程度である
- `parts::validate_document(document)` は **`from_parts` と同一の `inventory_of`（借用ベースの共通関数）でモデルから目録を組み、`StructuralValidator::validate` を 1 回呼ぶだけ**の薄い経路である（**規則の実装は 4.7 の 1 箇所のまま**。規則を書き写さないこと）。出現箇所テキストも `inventory_of` の 1 箇所で組み立てるため、**同じ違反は読み込み経路と同一の文言で報告される**（`save` と `from_parts(&to_parts(..))` のエラーが一致するパリティテストで固定）。ワイヤ形の整合（行の値の個数と列数の一致、列名の重複）は `to_parts` 側の検査のまま
- **`AtomicWriter` を使っていることの代理観測**: `tests/api.rs` に `#[cfg(unix)]` のテストを置き、**上書き保存で対象の inode が変わる**ことを確認している（rename による置換の代理観測。`commit` を `std::fs::write` に差し替える変異をこれが殺す。クラッシュ耐性そのものは観測不能）。inode の再利用は「一時ファイルが対象 inode を保持したまま生成される」ため起きず、偽陽性/偽陰性は生じない
- **task 7.3 への申し送り**: 公開 `to_parts` は（`save` と違い）`validate_document` を呼ばない。公開契約として `to_parts` が構造検証を担うのか、`from_parts` 側の検証に委ねるのかを明示して決めること。**task 8.x への申し送り**: ゴールデン fixture の schema は型参照を持たないため、**型参照の誤検出防止の主たる防衛線は往復テスト**である（8.x で補強する余地）
- task 7.3 で design の Service Interface の 4 メソッドが揃った（`open` / `save` / `to_parts` / `from_parts`）。`to_parts(document)` は **`parts::validate_document(document)?` → `parts::to_parts(document)`** の順で、**書き出し側の不変条件検証を含む**（`version-control` が「読み込み経路が拒否する集合」を手に入れられない。task 7.2 の裁定の帰結）。`from_parts(parts)` は `parts::from_parts(parts)` をそのまま呼ぶ（`open` と同じ 1 経路）。**Api 層に検証ロジックを持ち込まない**。`save` は `self.to_parts(document)?` → `encode` → `commit` に寄せてあり、**検証は `to_parts` 経由の 1 回だけ**
- **`open` と `from_parts` の同一検証は実測で固定**（`tests/api.rs` のパリティテスト。ダイジェスト改竄 / 未来 major / 宙吊り型参照 / 未登録添付参照 / `TypeDefId` 重複宣言の 5 種で、変種と出現箇所テキストが `Debug` 表記で一致）。コンテナ層にしかない検査（許可リスト・重複パス・マーカー・CRC）は ZIP 固有であり、`DocumentParts` の正準化（マーカー拒否）と `EntryName` の閉じた型により `from_parts` 経路へは構造的に到達不能
- **ZIP 非経由の実証は `tests/parts_contract.rs`**（新設。`document_format::container` を import せず、ファイル I/O もしない）。往復・エントリ名と中身の列挙・期待集合（`document.json` / シートごとの `schemas`・`sheets` / `attachments/<hex64>.bin` / `manifest.json`、`jxcel` を含まない）・各行エントリが自シートの行のみ（`RowsCodec::decode` で復号して実比較）・2 回の `to_parts` のバイト一致を検証する。**`open` / `save` のテストは `tests/api.rs`、ZIP 非経由の契約は `tests/parts_contract.rs`** と置き場を分けている（前者は `ContainerCodec` を import するため）
- **task 7.4 への申し送り**: `save(document, path)` は `Document` しか受け取らず「読み込み時に変換が適用された」ことを運ばないため、**退避分岐の判定源**（モデル側に保持するか、`open` の `OpenOutcome` を保存側へ渡す新しい口を作るか）を 7.4 で決める必要がある。`MigrationChain::gate` は副作用なしで「移行が必要か」を判定できるが、「移行が成功したか」は `from_parts` が `Ok` を返したことで分かる
- task 7.4 で**退避分岐**が入り、Public API 層が完成した。判定源は **`Document` が運ぶ真偽 1 つ**（`was_converted_from_an_older_format()`。設定は `parts::from_parts_with` が `MigrationChain::apply_with` の `Ok(Some(..))` を得た **1 箇所だけ**。`Ok(None)`（現行版）では立てない）。**この標識は wire 形式に含めない**（`to_parts` の出力は標識の真偽に依存せずバイト単位で同一。保存 → `open` し直すと `false` に戻る = 保存されない）。`OpenOutcome::migrated_from`（報告用の版情報）と `Document` の標識（`save` 判断用の「読み込み時の事実」）で役割を分けている
- `save` の順序は **`to_parts` → `encode` → 退避（必要なときだけ）→ `commit`**（退避は符号化の後なので、符号化失敗では退避を作らない）。**退避の 3 分岐**: ① 対象が存在しない → 作らない ② **退避先に既存の退避ファイルがある → 上書きしない**（原本を保持し続けることで「初回保存で残す」を満たし、2 回目以降の保存でも原本が失われない）③ それ以外 → 対象の現在のバイト列を読み、**`AtomicWriter::commit` で退避先へ書いてから**保存。退避先は**対象と同一ディレクトリの `<ファイル名>.bak`**
- **退避の失敗は保存の中止**（`Io { retried: false }`）で、対象は保存前のまま（要件 6.4 を守れない状態で上書きしない）。退避は `fs::copy` ではなく `AtomicWriter::commit` を使うため**内容のコピー**であり、**ファイルモード/メタデータは引き継がない**（`atomic_save.rs` の規約。lib.rs の退避節に明記済み）。**保持期間の方針**は「作るだけで削除も上書きもしない」（design の Risks「保持期間の方針は実装時に決める」への回答。削除は呼び出し元の責務）
- **検証限界（task 7.4 の実測）**: 実 `STEPS` が空のため、**実経路で「変換が適用された文書」はまだ存在しない**。退避分岐の観測は `src/lib.rs` の crate 内部テストが合成ステップ表（`migration::steps::synthetic` の 0.0→0.1→1.0 の実段）と `from_parts_with` で行っている。**非変換時の否定側は `tests/api.rs`**（実経路）。**task 8.7 への申し送り**: 実経路で退避を観測するには「過去版のゴールデン fixture + 実 `MigrationStep`（例 v0→v1）」を追加し、fixture を `open` → `save` して `<名前>.bak` が fixture のバイト列と一致することを統合テストで固定する必要がある
- **task 8.7 への申し送り（増分）**: 移行のゴールデン fixture を足すときは、**「fixture を開いて保存したときの退避」と「移行後のバイト列が決定的（同一 fixture から同一バイト列）」**の 2 点も固定すること（退避・決定性は 6.4 / 3.1 の実経路の証明になる）
- task 8.1 で検証用の共通ヘルパ **`tests/common/mod.rs`** ができた（`Scratch`（リポジトリ内・`Drop` で削除）/ `snapshot` / `entry_names` / **`document_view` と `assert_same_document`（完全比較）** / 標本 5 種（最小・複数シート＋0 行・値の網羅・未参照添付・未知フィールド）/ 違反文書ビルダ）。`tests/api.rs` / `tests/parts_contract.rs` / `tests/roundtrip.rs` が `mod common;` で共有する。**`tests/common/mod.rs` は `document_format::container` を import しない**（ZIP 非経由の契約を import 一覧で示すため、コンテナ依存のヘルパは `tests/api.rs` 側に置く）。`dead_code` は複数バイナリが部分集合を使うため理由コメント付きで許容
- **8.x の比較は `common::document_view` / `assert_same_document` を再利用すること**（第二の比較規約を作らない）。比較は ① `document_id` ② シート（**文書順**・ID・名前・**列順**・保持フィールド）③ 行（**行順**・`RowId`・`CellValue`）④ スキーマ（`root` と型定義 `definition` の**verbatim バイト列**・型参照）⑤ 添付（ID＋バイト列）⑥ 未参照添付 ⑦ **wire 射影（`to_parts` のエントリ名とバイト列）** の 7 観測点。7 は保持フィールドの公開アクセサが `pub(crate)` のため（`src/` 無変更で済ませるための射影）であり、`to_parts` は決定性があるので有効
- 8.1 の往復は 3 経路（フルサーキット / `save`→`open` / ZIP 経由と非経由の 3 者一致）を**それぞれ独立テスト**で、**標本をループして**検証し、コミット済みゴールデン fixture を実ファイル経路でも通す。**未参照添付が往復で落ちないこと**も押さえている（要件 7.6 の前提。本格検証は 8.8）。`roundtrip.rs` の `the_comparison_view_separates_every_observable_aspect` が「比較の強度」の自己検査（各観測点を 1 箇所ずつ変えて検出）を担う。**`assert_same_document` の個別アサートを 1 つ抜く変異は `parts` 射影の比較に隠れて生存する**（冗長観測のため能力欠落ではないが、`parts` 比較を外す変更を入れるときは注意）
- **【決定性の意味。重要】** 「同じ内容が常に同じバイト列」（要件 3.1）の「同じ内容」は**同じ識別子を持つ文書**を意味する: `DocumentId` / `SheetId` / `RowId` / 添付 ID は内容の一部であり、`Document::new` / `add_sheet` / `add_row` は ULID を発行するため、**新規作成した文書のバイト列がプロセスごとに違うのは違反ではない**（実測: `save(sample())` が同一コマンド 2 回で 2011 / 2019 バイト）。したがって**期待バイト列をコミットして比較するアンカーは、識別子まで固定された入力でなければならない**。既成の実体は `tests/fixtures/bytes/golden_container.zip`（5.2 が手で組んだ固定 ID の集合から生成）であり、**task 8.2 はこれを唯一のアンカーにした**（新規 fixture は作らない）。この解釈は `tests/determinism.rs` の module doc に明記済み
- task 8.2 で `tests/determinism.rs`（4 本）と **`.github/workflows/ci.yml` への 1 ステップ追加**（`cargo test -p document-format --test determinism`）が入った。CI の 3 OS マトリクス（ubuntu / macos / windows）で**同一のコミット済み期待バイト列**と比較するため、これが要件 3.2（OS 間のバイト一致）の実測機構になる。検証内容は ① アンカー文書の `save` 出力 == fixture バイト列（かつ `encode(to_parts(..))` とも一致、1.1 秒待って別ディレクトリへ 2 回目保存でも一致）② `common` の 5 標本で別パス・別ディレクトリへの 2 回保存がバイト一致 ③ `save` 出力 == `encode(to_parts(..))` ④ **ファイルに落ちたバイト列**の生ヘッダで固定係数（日時 `(0x0000, 0x0021)` / `0o100644` / host system `3` / 順序 / マーカー `Stored` 先頭 / 他 `Deflate` / データディスクリプタ不在）を確認
- **検証限界（task 8.2 の実測）**: ① 圧縮レベル 6→9 は **miniz_oxide が同一バイトを出すため等価**（差が出るのは低レベル側。6→1 は検出される）② `.last_modified_time` / `.system` / `.unix_permissions` の**明示固定を外す**変異は、この環境では crate 既定値が固定値と一致するため生存する（`.system` は Windows で差が出るので **CI の Windows レグだけが観測点**。これも CI マトリクスが必要な理由）③ 時刻依存は「現在時刻由来の値を**明示的に注入**する」変異（固定を外すのではなく）で検出できる ④ 新しい flag の `sleep(1.1s)` は秒境界を確実に跨ぐが、時刻依存自体はアンカー比較と生ヘッダ検査も捉えるため**防御的冗長**（CI の実行時間を 1 秒強使う）
- task 8.3 で `tests/row_granular_diff.rs`（要件 3.4 / 3.5）が入った。**差分の範囲の定義（この解釈を再利用すること）**: 1 行 1 セルを変更したとき、**バイト列が変化するパートは「変更行を持つ `sheets/<ulid>.jsonl`」と `manifest.json` の 2 つだけ**（`manifest.json` はそのシートのダイジェストを記録しているため変化する。**欠陥と誤解しないこと**）。他のシートの行エントリ / `schemas/*.json` / `document.json` / `attachments/*` はバイト単位で不変。行エントリは**テキスト行数が不変**で**異なる行がちょうど 1 行**、その**位置が変更行の位置と一致**する。生の ZIP バイトは比較しない（中央ディレクトリのオフセットが動くため）。比較は**露出させたパート集合**で行う
- 要件 3.4 の実測（8.3）: 値に LF / CRLF / 単独 CR / タブ / `"` / 日本語 / 絵文字 / U+2028 を含んでも**テキスト行数 == 行数**で、生の LF は区切り以外に現れない（`\n` / `\r\n` / `\r` はエスケープされる）。**行数が増える値は無い**
- **検証タスクの規模と時間の裁定（task 8.3）**: 10 万行 × 30 列のフル経路は debug で `save` が約 7.7 秒かかる（CI は 3 OS で debug 実行）。したがって **10 万行では 1 位置（中間行）**、**位置追従の 3 位置ループは 1000 行の小標本**（比較口は共通）で検証する。`tests/row_granular_diff.rs` は約 23 秒。**CI の実行時間への影響を意識すること**（`cargo test --workspace` 全体は現在 1 分強）
- **`Row::new` / `Row::set_values` / `Sheet::push_row` / `Sheet::extend_rows` は `pub(crate)`** で統合テストからは到達できない。統合テストで大量の行を持つ文書を作るときは、**`DocumentParts::from_entries` + `from_parts`（内部で `extend_rows` の一括経路を使う）** を使うこと（`set_row_values` の反復は O(n²)）。変更は `set_row_values` を**1 回だけ**呼ぶ（1 回なら O(n) の走査で済む）
- **【重要な事実】`zip` は統合テストからも名前で参照できる**（`[dependencies]` の crate は `tests/` のターゲットからも使える。`tests/corruption.rs` が `zip::write::ZipWriter` を実際に使ってコンパイル・実行できている）。5.2 が `tests/container_writer.rs` に書いた「統合テストからは参照できない」というコメントは**事実に反していた**ため、タスク 8.4 で修正した。**したがって ZIP の生組み立て（索引の無いコンテナ等）も統合テストでできる**
- task 8.4 で `tests/corruption.rs`（8 本）が入った。**`open`（実ファイル経路）で変種と「`Err` でありモデルが返らないこと」（要件 5.4）とファイル不変（要件 5.5）をシナリオごとに固定**している: ダイジェスト改竄 → `IntegrityMismatch { entry }` / `TypeDefId` 重複 → `DuplicateId { kind: TypeDef, id, occurrences }` / `RowId` 重複 → `DuplicateId { kind: Row, … }` / 宙吊り型定義参照 → `DanglingTypeRef` / 宙吊り添付参照 → `DanglingAttachmentRef` / スキーマ欠落 → `MissingSchema { sheet }` / `document.json` 欠落・`manifest.json` 欠落 → `MissingPart { name }`。`tests/validate.rs`（規則の単体）の写しは作らず、**実ファイル経路の観測**に絞っている
- **【検証タスクの作法】** 壊し方は「**パート集合のレベルで対象エントリを書き換えてから `ContainerCodec::encode` で組み直す**」こと（**ZIP の生バイトをパッチしない**。CRC で `zip` が先に落ちて検証したい層のエラーにならない）。例外は `manifest.json` 欠落で、これは `DocumentParts::from_entries` が索引を必須にするため**公開 API では索引なし集合を組めない** → `zip` の `ZipWriter` で直接組み立てる（**索引を残した対照**を併置して、失敗原因が索引の欠落だけであることを示すこと）
- **`RowId` の重複は同一エントリ内では到達不能**: `RowsCodec::decode` が `$id` の重複を `InvalidContainer`（`duplicate row identifier`）で先に拒否するため、`DuplicateId { kind: Row }` に到達する経路は**シートをまたぐ重複**だけである（テストは 2 シート 1 行ずつの標本で構成）
- 生存変異（8.4）: `validate_schema_presence` が `MissingSchema` を報告しなくても、後段の `take_schema` が**同じ変種・同じシート**を返すため等価 / `read_format_version` の索引要求を緩めても `manifest_part_of` → `resolve_manifest` が同じ `MissingPart { name: "manifest.json" }` を返すため等価（**冗長な二重防衛**であり欠陥ではない）
- **宿題（8.5 で実施。親の裁定）**: `zip` が統合テストから使える以上、`tests/container_writer.rs` と `tests/determinism.rs` に重複している**生ヘッダ解析（`local_headers` / `central_headers`、約 90 行）は `tests/common/mod.rs` へ寄せる**（バイト列だけを解析するヘルパなので `common` が `container` を import しない方針を守れる）。**第二の重複を作らないこと**
- **【8.5 で実施済み】** 生ヘッダ解析（`local_headers` / `central_headers`）は `tests/common/mod.rs` の**1 箇所**に移設した（`container_writer.rs` / `determinism.rs` は機械的に移行。テスト本数 5 / 4 は不変）。`CentralHeader.header_start` を追加。**新しいヘッダ解析を書く前に `common` を見ること**
- task 8.5 で `tests/malicious_archive.rs`（9 本）が入った。**許可リスト外の名前は原文のまま `InvalidContainer { entry }` で拒否**され、`entry` は入力文字列と**バイト単位で完全一致**する（サニタイズ・正規化なし。20 ケースで実測）。同一パス重複は `InvalidContainer`（`entry` に該当パスを含む）。**`./manifest.json` は `manifest.json` と畳み込まれず、許可リスト違反（要件 2.5）として報告される**（design が「重複 = バイト単位完全一致・正規化なし」と定めているため妥当）。**判定順**（許可リスト照合と重複検出が展開に先行）も許可リスト違反 + サイズ偽装の同時持ちで実測し、コンテナ経路でも固定するテストを置いた
- **「極端に膨張するアーカイブ」の検証内容（8.5 の裁定の実測）**: **絶対サイズ上限は実装しない**（要件 8.5 が 10 万行超の拒否を禁じるため意図的な非実装。module doc に明記）。防御は **有界読み（`take(宣言 + 1)`）と展開長の一致要求**で、実 64 KiB に 100 宣言 → **報告展開長 101 = 宣言 + 1**（無制限読みなら 65536）、1 MiB 宣言 → `expanded size 65536` として**両方向で拒否**されることを実測。**対照（同じ組み立て方の正しいアーカイブは成功）**も併置
- **既知の制限の現挙動を固定（8.5）**: 中央ディレクトリと EOCD の間の `PK\x01\x02` 並びを存在しないエントリとして読むため**過剰拒否**する（可用性のみの劣化で不正の受理にはならない）。テストで現挙動を固定し、doc に「将来 `cd_size` で有界化されたらこの期待を更新してよい」と記載した
- task 8.6 で `tests/atomic_save.rs` が入った。**`AtomicWriter::commit` に注入点が無く `src/` も変更できないため、子プロセスを外から落とす**方式: テストバイナリを `std::env::current_exe()` の**絶対パス**で `--ignored --exact --test-threads=1 crash_worker` として再実行し、親が**`.jxcel-tmp-` の出現/サイズをポーリングして `Child::kill()`（SIGKILL）**する。**落とす係のワーカーは `#[ignore]`**（通常の `cargo test` で実行されない）。**新規依存は追加しない**（`libc` 不要）。子への受け渡しは環境変数（`Command::env` で明示。`set_var` を使わない）で、分岐は**値の厳密一致**
- 8.6 の観測（4 MiB の非圧縮性添付で窓を広げる）: 「出現直後（一時 0 バイト）」「**書き切り後（一時サイズ == 期待サイズ）**」「出現 + 20ms（置換完了）」を再現。中断試行では (a) 対象バイト列が保存前と一致しかつ `open` 可、(b) **`.jxcel-tmp-` の残骸が存在**、(c) inode / mtime / 長さ不変。**対照**（落とさなければ置換完了・残骸なし）を同じ経路・同じ標本で置く。**期待サイズは子が `to_parts` → `encode` の長さを書いて親に渡す**（`save` と同一経路なので自己参照は無害。位相の検出にのみ使う）
- **8.6 の検証限界（5.1 の申し送りと一致）**: `sync_all` の耐久性と置換後の親ディレクトリ fsync は**このテストでも埋まらない**（`sync_all` 除去変異は位相要件で確率的に RED になるだけで、耐久性の検出ではない。親 fsync 除去は生存）。固定できるのは構造（同一ディレクトリ性・rename 差し替え・中断時の対象不変・残骸の有無）まで
- **【既知の穴（8.6 レビューの判定 = 非阻害）】`tests/atomic_save.rs` は `#![cfg(unix)]`** のため **Windows では 0 テスト**になる（macOS は `unix` なので実行される）。判定理由: `Child::kill()` は Windows でも `TerminateProcess` で機構は移植可能 / 「rename まで対象に触れない」保証は OS 非依存の共有経路にあり、Windows 固有差分（共有違反リトライの分類）は Windows CI 上の**単体**テストで検証済み / タスク受入と design は全 OS を要求していない。**拡張するなら「inode 比較を `#[cfg(unix)]` 化し、Windows はバイト列 + 長さ + mtime で代替、`signal()` を `code()` へ分岐」**。ただし **Windows 実機での検証手段が手元に無い状態で cfg 分岐を足すと未検証コードが増える**ため、実機で走らせられる人が行うのが安全
- **8.6 のフレーキー耐性の条件**: 位相の成立は「4 MiB の `sync_all` が親のポーリングで観測できる長さ」に依存する。fsync が実質ゼロ時間の FS（tmpfs 等）や高負荷な runner では「書き切り後」を捕捉できない可能性がある（緩和はペイロード拡大）
- task 8.7 で移行のゴールデン fixture が入った: **`tests/fixtures/golden/v1/anchored.jxcel`（コミット済み。`bytes/golden_container.zip` と同一バイト列 = 識別子まで固定した現行版コンテナ。真偽の源は `tests/container_writer.rs` の `fixed_parts` 生成手順）**。`golden/v1/.gitkeep` は fixture 実体が入ったため削除。統合テスト 2 本（`tests/migration.rs` が計 10 本）+ crate 内部 1 本（`src/lib.rs` の `#[cfg(test)]` のみ、**プロダクション差分 0**）
- **【移行 fixture の作法】** 古い版の入力は**テスト時に合成**する: コミット済み v1 fixture を `decode` → **記録バージョンだけ**を古い値へ書き換え（`manifest.json` は自身を索引しないのでダイジェスト整合は壊れない）→ `encode` → **ディスクへ書いて読み直してから**移行に渡す。**「v0 という形式は歴史上存在しない」＝合成入力であることを doc に明記**し、新しい移行ステップを `src/` に足さないこと（design「初版は v1 のみ」）。移行の適用は `#[cfg(test)]` の合成表 + `parts::from_parts_with`（`pub(crate)`）で行う
- **【構造による強制（8.7 の中核）】** テストが **`STEPS` の各 `from` と `CURRENT_FORMAT_VERSION` から「必要な fixture の集合」を導出**し、各版の `golden/v<major>/` の存在・**fixture の記録バージョンがディレクトリ名と一致**・コンテナとして復号可能であることを検証する。**新しい版を足してステップを書くと、fixture を足すまでテストが落ちる**（`STEPS` にダミー段を足す変異で実測済み）。**fixture の選択は「記録バージョン一致」で行う**（「現行 major ディレクトリの唯一の `.jxcel`」という形は **同じ major の新しい minor の fixture を足した瞬間に panic する**ため避ける）
- **8.7 の検証限界**: 現行版 fixture は**バイト往復**まで検証しているが、将来版の fixture は構造テストが**存在・復号・記録バージョン一致**までしか見ない（バイト往復は現行版のみ）。将来版を足すときは**その版の往復検証も足す**ことが望ましい（規約 doc に手順を書くこと）
- task 8.8 で `tests/attachment_registry.rs` が拡張された（計 6 本 = 既存 2 + 新規 4。差分は追加のみ + doc/import 更新）。検証内容: **添付のバイト列が `save`→`open` で 1 バイトも変わらない**（標本: 空 / 1 バイト / **NUL と高ビットを含む非 UTF-8** / **JSON 風 `{"a":1}`（7 バイトのまま = 解釈・正規化されない）** / 非 ASCII UTF-8 / 改行入り / 1 MiB / 同一内容の重複）/ **未参照添付が削除されず `unreferenced_attachments()` に現れ、参照済みは現れない**（混合状態の区別も）/ **参照の保持**（直接セルと入れ子配列内の参照。ZIP 経由と非経由の両方。参照先バイト列を `Document::attachment(id)` で取得可）/ **エントリ名が `attachments/<hex64>.bin` で `<hex64>` が内容ハッシュと一致**（同一内容は 1 件に畳まれる）
- **「再圧縮しない」（要件 7.5）の解釈（親の裁定。8.8 で doc に明記）**: 本形式はコンテナ全体を ZIP として書くため添付も `Deflate` で格納される（5.2 の確定形: `Stored` は型マーカーだけ）。したがって要件 7.5 の意味は「**本モジュールが添付の内容を解釈・変換しない**」であり、**ZIP の圧縮は透過で可逆**（展開後に元のバイト列が得られる）。**「圧縮しない＝`Stored` で書く」と取り違えないこと**
- **【運用上の注意】レビュー probe の残骸**: task 8.6 のレビュアーがリポジトリ直下に `.probe-atomic86/`（4 MiB の一時ファイルを含む）を残し、親が削除した。**レビューの probe は `CARGO_MANIFEST_DIR` 配下などに置き、終了前に必ず削除すること**（`git status --porcelain --untracked-files=all` で確認）
- task 8.9 で `benches/large_document.rs`（**10 万行 × 30 列**の `open` / `save` を criterion で計測）/ `scripts/check-bench-budget.sh`（POSIX sh の予算判定。0 = 予算内 / 1 = 超過 / 2 = 計測値なし・解釈不能 = **fail-closed**）/ `Cargo.toml` の `[[bench]] large_document`（**追加のみ**）/ `.github/workflows/bench.yml` の予算判定ステップ（**追加のみ**）が入った。**実測: open 約 0.62 秒（予算 3 秒、余裕 約 4.8 倍）/ save 約 1.46 秒（予算 2 秒、余裕 約 1.37 倍）**、`cargo bench -p document-format --bench large_document` の所要 約 42 秒（`sample_size=10` / `warm_up=3s` / `measurement=10s`。save は criterion が自動延長して実効約 14.6 秒）
- **性能予算の判定は criterion の `estimates.json` の `mean.point_estimate`（ns）に対して行う**（予算値は**要件由来のリテラル** `3000000000` / `2000000000`。計測値から予算を導出する自己参照にしない）。**判定不能時は exit 2 で明示的に失敗**させる（黙って通すとベンチが走らなくなっても気付けない）。`--save-baseline=main` でも `new/estimates.json` が生成されるため CI の現行コマンドで機能する
- **【検証限界（8.9）】** ① **CI 実行そのものはこの環境では検証できない**（Linux のみ。`bench.yml` の YAML とコマンド・判定はローカルで再現実測）② **性能予算は「速い偽計測」（計測値を定数に差し替える等）を原理的に検出できない**（規模・経路の assertion が別層で担保）③ `save` の余裕は約 1.37 倍しかなく、**CI runner の速度差で 2 秒を超えるスプリアス失敗のリスク**がある（閾値は要件値のまま維持し、真の超過時は design の対処順序＝割り当て削減 → `Stored` → SIMD を検討する）④ `bench.yml` は既存の起動条件（`workflow_dispatch` / 週次 `schedule` / `push` の `branches:[main]` かつ `paths:[crates/**]`）のため、**PR では走らない**（「機能追加と同時に検出」は main マージ時点の意味）
- **8.9 の規模 assertion の教訓**: ベンチ内の「10 万行 × 30 列」の検証は、**定数同士の比較（恒真）にしないこと**。当初 `COLUMNS` 定数自身と比較していたため `COLUMNS=20` 変異が生存し、**要件リテラル 30 との比較**に直して検出できるようにした。行数は `SUPPORTED_ROW_LIMIT` と比較している

## 最終検証（`/kiro-validate-impl`）の記録

全 39 サブタスクの完了後、フィーチャー横断の検証を 4 つの観点（テスト実行＋スモーク / 要件カバレッジ / 設計整合 / タスク間統合・境界監査）で実施した。**初回は NO-GO**（下表の R1・R2 と衛生上の残骸）、**修正後に再検証して GO**。

- **R1【要件 4.3 の未達。修正済み】** `document.json` の**重複シート識別子**は `DocumentPart` の復号が `StructuralValidator` より先に拒否するため、`DuplicateId { occurrences }` に到達せず、`InvalidContainer` のメッセージにも**出現箇所が載っていなかった**（要件 4.3 は「重複した識別子と**その出現箇所**を含むエラー」を要求）。→ **分類は `InvalidContainer` のまま**（`document_part.rs` の裁定。コンテナ不正と同じ分類）で、**識別子と全出現位置**（`document.json sheets[i]`、0 始まり、`inventory_of` と同一の綴り）を `entry` に載せるよう修正。重複が 3 件以上でも全位置を載せる。テストは `tests/corruption.rs`（実ファイル `open` 経路）。**`DuplicateId` へ寄せる変更はしない**
- **R2【重複実装。修正済み】** 添付参照の再帰走査が `model/attachment.rs` と `parts/validate.rs` に**別々に実装**されていた（`NestedValue` に変種が増えると片方だけ直して黙って漏れる）。→ **`src/value.rs` の `visit_attachment_references(value, &mut dyn FnMut(AttachmentId))` 1 箇所**へ統合（**ビジターで `Vec` を確保させない**）。両層がこれを呼ぶ。`value.rs` は `model` と `parts` の双方が依存する最下層であることが選定理由
- **R3〜R5【衛生。修正済み】** `tests/container_writer.rs` のローカル `fixture_path`（8.5 移行の取り残し）と `tests/migration.rs` のローカル `api()` を削除して `tests/common` の唯一の定義に寄せた。死コード `IdDeclaration::sheet()`（参照 0 件）を削除
- **記録のみ（変更しない）**:
  - **`model → json` の辺**（`PreservedFields`）は design の依存鎖「各層は左方向にのみ依存する」に対する**例外**であり、タスク 4.8 の親裁定として `model/sheet.rs` の doc に記載済み（`json → model` の逆向きは無い）。**design 本文は「逆流は誤り」のままなので、design の記載と実装の例外を突き合わせるのは仕様オーナーの作業**（本スペックの実装は裁定に従っている）
  - `tests/document_parts.rs` の同名ヘルパ（`sheet_metadata` / `rows` / `schemas` / `attachments`）は `common` と**戻り型が異なる**ため完全なコピーではなく、統合しない
  - `MigrationChain::apply`（design が名付けた公開入口）と `PartInventory::declare_attachment_ref`（テストが使う builder）は**孤児ではない**（削除しない）
- **検証の判定材料（再検証時点の実測）**: `cargo test --workspace` = **391 passed / 1 ignored / 0 failed**（既存テストの消失ゼロ。新規は `value.rs` の走査器テストと `tests/corruption.rs` の R1 テストの 2 本のみ）／`cargo build --workspace --all-targets` 警告 0／`cargo bench --workspace --no-run` 成功／要件カバレッジは **46/46 基準**に対応あり／層の逆流は上記の 1 件のみ（裁定済み）／`zip` の参照は `container/` の 2 ファイルのみ／エラー表の 10 変種が実在し新変種なし
- **未解決の検証限界（受け入れ済み）**: ① 移行（6.2 / 6.3 / 6.4）の**実経路**は v1 のみの形式のため合成チェーンでのみ検証（実 `STEPS` は空）② OS 間バイト一致（3.2）は **CI の 3 OS マトリクス**でのみ検証（本環境は Linux のみ）③ 計測環境の規定（8.3）は**文書依存**（実行時検査なし）④ 原子性の**耐久性**（`sync_all` / 親 fsync）は電源断を模擬しない限り観測不能で、`tests/atomic_save.rs` は `#![cfg(unix)]`（**Windows では 0 テスト**）⑤ 性能予算は「速い偽計測」を原理的に検出できない（規模・経路の assertion が別層で担保）

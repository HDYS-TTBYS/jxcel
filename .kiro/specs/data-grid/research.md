# Research & Design Decisions: data-grid

## Summary
- **Feature**: `data-grid`
- **Discovery Scope**: New Feature + Complex Integration（実装済みの 3 スペックの上に載り、拡張点を 2 つ新設する）
- **Key Findings**:
  - **`document-format` に行の削除と位置指定の挿入が存在しない。**変更 API は `add_row`（末尾追加）・`reorder_rows`・`set_row_values` のみで、要件 6.2 は現状の公開面では実現できない
  - **開いた `Document` をメモリ上で保持する持ち主がどこにも存在しない。**`document-format` に依存するクレートは `schema-engine` だけであり、`app-shell` は「他のドメインクレートに依存しない」と明記している。この欠落に対して新スペック `document-session` を roadmap へ追加した
  - **グリッドライブラリの現実解は 1 つに絞られる。**範囲選択とクリップボードが無償で使えて canvas で描く MIT のライブラリは Glide Data Grid だけであり、その stable は React 19 を受け付けない
  - **描画の失敗を web ページ側から確実に検出する手段は存在しない。**WebKit は WebGL のレンダラ文字列を伏せるため、要件 12.2 は「塗って読み戻す」検査で満たすほかない

## Research Log

### グリッドライブラリの選定
- **Context**: `brief.md` は選定を design で行うと定めた。`tech.md` は Glide Data Grid を「stable が止まっている」ことを理由に第一候補から外している。要件 11.1（毎秒 60 回の描画更新）と要件 7（範囲のコピーと貼り付け）を無償の許諾で満たす必要がある
- **Sources Consulted**: npm registry / GitHub API（2026-09-13 取得）、各プロジェクトの LICENSE と価格表、1771 Technologies の 2026-06 のベンチマーク
- **Findings**:

| 候補 | 版 / 日付 | 許諾 | 描画 | 範囲選択・貼り付け | React 19 | 判定 |
|---|---|---|---|---|---|---|
| Glide Data Grid | stable 6.0.3 / 2024-02-03、beta 6.0.4-alpha24 / 2025-10-08、最終コミット 2026-01-21 | MIT | canvas | あり（`getCellsForSelection` / `onPaste`） | **stable は不可**（peer が 18.x まで。issue #1189 が open）。beta のみ可 | **採用（beta に固定）** |
| AG Grid Community | 36.1.0 / 2026-08-05 | MIT（Community） | DOM | **Enterprise 限定**（$999/開発者） | 公式対応 | 却下（無償で要件 7 を満たせない） |
| TanStack Table v9 + Virtual | 9.2.4 / 2026-08-28 | MIT | 自前 | セル範囲選択あり / 貼り付けは自前 | 公式対応 | 却下（行モデルが全件の実体化を前提とし、窓単位の転送と噛み合わない） |
| RevoGrid | 4.27.10 / 2026-09-09 | MIT（中核） | DOM | あり（貼り付けは未確認） | **公式の表明なし**（devDeps は React 18 固定） | 次点 |
| Handsontable | 18.1.0 / 2026-09-01 | 商用のみ | DOM | あり | 対応 | **失格**（商用利用に有償契約が必要。かつ競合製品の開発を禁ずる条項があり、表計算である本製品に抵触しうる） |
| 自前 canvas | — | — | canvas | 自前 | — | 退路として保持 |

- **Implications**:
  - DOM 系（AG Grid / RevoGrid / TanStack）は 2026-06 の実測で 10 万行規模で毎秒 27〜38 回まで落ちており、要件 11.1 に届かない見込みが強い。**30 列 × 可視 40 行で毎フレーム 1,200 要素**を扱うことになる
  - 本設計は並べ替え・絞り込み・行モデルを Rust 側に置くため、**ライブラリに求めるのは描画・当たり判定・文字計測・クリップボードの配管だけ**になる。Glide の `getCellContent` は引きに来る形であり、窓単位の転送とそのまま噛み合う
  - Glide は並べ替えと絞り込みを**持たない**（公式に「データ源の側で実装せよ」）。本設計にとってはこれは欠点ではなく一致である

### Linux / WebKitGTK 上の canvas
- **Context**: roadmap の Prototype-First Risk #3。`tech.md` は「描画失敗を検出する API は存在せず、Tauri も wry も回避策を自動設定しない」と記録している
- **Findings**:
  - 症状は白画面・リサイズ時のちらつき・`Failed to create GBM buffer` で、原因は WebKitGTK の DMA-BUF レンダラ。**2.42 以降**が該当し、NVIDIA のプロプライエタリドライバとソフトウェア GL で再現する。WebKit 側の bug 262607 は **WONTFIX** で解決済みであり、上流の修正は来ない
  - 最新の再現報告は WebKitGTK 2.52.3（`tauri-apps/tauri#15936`、2026-08-29）。**DOM は検査器に見えているのに何も塗られず、ログも出ない**
  - 回避は環境変数のみ。順に `__NV_DISABLE_EXPLICIT_SYNC=1` → `WEBKIT_DISABLE_DMABUF_RENDERER=1` → `WEBKIT_DISABLE_COMPOSITING_MODE=1`
  - 一方で canvas の描画自体は改善している。2.46 が Cairo から Skia へ、2.50 が Skia のスレッド化、2.52 が 2D canvas の動的 MSAA と描画命令の一括再生を入れた。Ubuntu 22.04 は 2.50.4、24.04 は 2.52.6 を配っている
- **Implications**: canvas を選ぶこと自体は 2026 年時点で不利ではない。危険なのは描画基盤の初期化であり、**起動直後に描画が成立したことを自分で確かめる**設計が要る

### 描画成立の検出可能性
- **Findings**:
  - `WEBGL_debug_renderer_info` の `UNMASKED_RENDERER_WEBGL` は一般的な手段だが、**WebKit は指紋対策のためこの文字列を伏せる**。Tauri 自身の文書がそう明記している。本製品の Linux と macOS はいずれも WebKit であり、この手段は使えない
  - WebGL2 のコンテキスト生成はソフトウェアラスタライズでも成功し、信号が出ない。**Canvas 2D に至っては GPU 支援の有無を問う API が存在しない**
  - 実用的に効くのは 3 つ: (1) 既知の図形を塗って `getImageData` で 1 画素読み戻す、(2) 起動時に合成的な走査を行いフレーム時間の中央値を測る、(3) Rust 側から環境変数と GL の素性を読む
- **Implications**: 要件 12.2 は (1)、要件 12.3 は (2) で満たす。(3) は `app-shell` の要件 10.3 が既に所有しているため**本機能は重複して持たない**

### 上流の公開面の実地確認
- **Context**: 要件 3・6・7 が `document-format` の変更 API に依存する
- **Findings**（`crates/document-format/src/model/mod.rs` を直接確認）:
  - 存在する: `add_sheet` / `remove_sheet` / `rename_sheet` / `set_sheet_columns` / `set_root_schema` / `add_row` / `reorder_rows` / `set_row_values` / `add_attachment`
  - **存在しない**: 行の削除、位置を指定した挿入、セル単位の設定、変更通知、未保存の追跡
  - 書き込みの粒度は**行まるごと**（`set_row_values` が `Vec<CellValue>` を置き換える）。1 セルの編集は行を読んで写して書き戻す形になるが、列数は 30 程度なので費用は無視できる
  - `Row` は `Serialize` も `Clone` も持たず、`Violation`（`crates/schema-engine`）も `Serialize` を持たない。`CellValue` は `Serialize` / `Deserialize` を持つ（`crates/document-format/src/value.rs`）。それでも境界を越えられないのは 64 ビット整数（`CellValue::Int`）と識別子を出せず、ts-rs の derive が `crates/app-shell/src/ipc/` の下だけに許されるためであり、直列化できないからではない
- **Implications**: `document-format` に足すのは行の削除・位置指定の挿入・取り除いた行の差し戻しの 3 つだけである（design.md「上流への最小の追加」）。それ以外は既存の公開面で足りる

## Architecture Pattern Evaluation

| 選択肢 | 内容 | 強み | 危険 / 限界 | 判定 |
|---|---|---|---|---|
| 全件をフロントエンドへ運ぶ | 10 万行を一度に webview へ送り、JS 側で並べ替え・絞り込み・描画 | 走査中の往復が皆無で、描画が滑らか | ドキュメントが Rust と webview に二重に載る。要件 11.6 に反する。`structure.md`「性能はドメイン側で守る」に反する | 却下 |
| **窓単位で運ぶ（採用）** | 並べ替え・絞り込み・違反集計を Rust 側に置き、可視範囲の窓だけを生バイトで送る | 資源が行数に比例しない。判定の源が 1 つに保たれる | 走査中に往復が入るため、先読みを誤ると引っかかる | **採用** |
| 行ごとのコマンド | セルや行を個別に取得する | 実装が単純 | `bulk.rs` の module doc が「この経路に行指向の API は無い」と明示的に禁じている | 却下 |

## Design Decisions

### Decision: 描画層を移植口の背後に置き、Glide Data Grid を beta 版に固定して採用する
- **Context**: 要件 11.1 を満たす描画と、要件 7 の範囲コピー・貼り付けを、無償かつ proprietary 配布と両立する許諾で得る必要がある
- **Alternatives Considered**: 1) Glide を直接使う 2) 自前 canvas を最初から書く 3) DOM 系ライブラリ
- **Selected Approach**: 描画・当たり判定・クリップボードの配管だけを担う**移植口**を本機能が定義し、その実装として Glide Data Grid `6.0.4-alpha24` を使う
- **Rationale**: 本設計はデータ・並べ替え・絞り込み・検証をすべて Rust 側に置くため、ライブラリに委ねる面積が小さい。移植口を挟む費用は薄く、上流が止まっている危険に対する退路になる
- **Trade-offs**: alpha 版への固定を受け入れる。stable は React 19 を拒否するため選択肢がない。MIT なので取り込み（vendoring）は合法であり、規模も現実的
- **Follow-up**: **タスクの最初期に、WebKitGTK 上で 10 万行 × 30 列の走査を実測する。**ここで毎秒 60 回に届かない場合、同じ移植口の背後に自前 canvas を置く判断へ切り替える

### Decision: 境界に数値を出さず、表示文字列と変種の札で運ぶ
- **Context**: `ipc-contract.md` が 64 ビット整数の越境を禁じている。`CellValue::Int(i64)` と `RowId`（ULID/u128）はいずれも JS の数値に収まらない
- **Selected Approach**: 窓の応答は「行識別子（生の 16 バイト）・セルの表示文字列・値の変種の札・違反の有無」の並びとする。**数値としての値は一切越えない**
- **Rationale**: グリッドは値を表示し、利用者が打った文字を返すだけでよい。型解釈は `schema-engine` が所有しており、フロントエンドが数値として解釈する理由がない。これにより 64 ビット問題が構造的に消える
- **Trade-offs**: 桁区切りや小数点の表示規則が Rust 側に寄る。フロントエンドでの並べ替えができなくなるが、並べ替えはもともと Rust 側に置く
- **Follow-up**: 入れ子の値だけは構造を持つため、`document-format` の `to_json_bytes` / `from_json_bytes` を使って JSON 文字列として往復させる

### Decision: 取り消し履歴を Rust 側に置く
- **Context**: 要件 9.7 は、数式の再計算とマクロの実行が同じ履歴に加わることを求める
- **Selected Approach**: 履歴は `crates/data-grid` が所有し、命令と逆命令の対を積む
- **Rationale**: `formula-engine` と `macro-runtime` はいずれも Rust 側で動く。履歴をフロントエンドに置くと、後から乗る 2 者が境界を越えて履歴を操作することになり、拡張点として成立しない
- **Trade-offs**: 取り消しのたびに境界を 1 往復する。1 操作あたり 1 回なので要件 11.3 の 100 ミリ秒に収まる

### Decision: 上流の欠落は 3 メソッドの追加に閉じる
- **Context**: 行の削除と位置指定の挿入が `document-format` に無い
- **Selected Approach**: `remove_rows(sheet, &[RowId]) -> Result<Vec<Row>, _>` と `insert_row_at(sheet, index) -> Result<RowId, _>` に加え、取り除いた行をそのまま差し戻す `insert_rows_at(sheet, index, Vec<Row>) -> Result<(), _>` を追加する。`remove_rows` が取り除いた行を返すのは、取り消しに値と識別子の両方が要るためである（`Row` は `Clone` を持たず、既存行の識別子・値を書き換える口も無い。空の行しか作れない `insert_row_at` では取り消しを満たせない。詳細は design.md「上流への最小の追加」）
- **Rationale**: 一括で受けるのは、範囲削除が 1 操作であり、行ごとに呼ぶと並びの作り直しが繰り返されるため。決定的出力の契約には触れない
- **Trade-offs**: 実装済みのクレートへ手を入れる。影響は行の集合と並びに閉じており、往復の契約は変わらない

### Decision: ドキュメントの保持を新スペックへ切り出す
- **Context**: 開いた `Document` の持ち主が存在しない
- **Alternatives Considered**: 1) `data-grid` が吸収する 2) `app-shell` を拡張する 3) 新スペック
- **Selected Approach**: `document-session` を roadmap へ新設し、`data-grid` はその下流になる
- **Rationale**: `app-shell` は「他のドメインクレートに依存しない」制約を持つため 2) は制約を壊す。1) は「表示と操作」と「ファイルの寿命管理」という異質な責務を同居させ、`version-control` が保存に相乗りする時点で切り直しが必要になる
- **Trade-offs**: MVP の本数が 2 本から 3 本に増える。`data-grid` の実装は `document-session` の完了を待つ

## Risks & Mitigations
- **Glide Data Grid が WebKitGTK で毎秒 60 回に届かない** — 移植口の背後で自前 canvas へ切り替える。判断はタスク最初期の実測で行い、設計全体には波及させない
- **alpha 版への固定** — 版を固定し、`Cargo.lock` と同じ規律で `package-lock.json` を追跡する。取り込みが必要になった場合に備え、移植口が触る面を最小に保つ
- **窓の先読みが外れて走査が引っかかる** — 先読みの幅と窓の大きさを計測で決める。フレーム時間の標本を要件 12.3 の検出と共用する
- **`document-session` の契約が未確定のまま設計する** — 本設計は「ウィンドウに対応する `Document` への参照と、変更を書き戻す手段」という能力の水準でのみ依存し、API の形を先に決めない。`document-session` の design 確定時に本設計を再検証する（Revalidation Trigger に記載）

## References
- [Glide Data Grid](https://github.com/glideapps/glide-data-grid) — MIT、canvas、`getCellContent` が引きに来る形
- [glide-data-grid issue #1189](https://github.com/glideapps/glide-data-grid/issues/1189) — stable が React 19 を受け付けない（2026-05-22 提出、open）
- [AG Grid の価格表](https://www.ag-grid.com/license-pricing/) — 範囲選択とクリップボードが Enterprise 限定である根拠
- [Handsontable の許諾](https://handsontable.com/docs/react-data-grid/software-license/) — 商用利用に有償契約が必要、かつ競合製品の開発を禁ずる条項
- [TanStack Table v9 の告知](https://tanstack.com/blog/announcing-tanstack-table-v9) — React 19 対応と行モデルの改善
- [Tauri の Linux 描画の手引き](https://v2.tauri.app/develop/debug/linux-graphics/) — 回避の環境変数の順序と、WebKit がレンダラ文字列を伏せる旨
- [WebKit bug 262607](https://bugs.webkit.org/show_bug.cgi?id=262607) — DMA-BUF の無効化要求が WONTFIX で終了
- [tauri-apps/tauri#15936](https://github.com/tauri-apps/tauri/issues/15936) — WebKitGTK 2.52.3 で「DOM はあるが何も塗られない」再現報告（2026-08-29）

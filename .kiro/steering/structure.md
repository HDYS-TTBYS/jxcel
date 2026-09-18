# Project Structure

> 本ファイルは規定と記述の両方である。`document-format` / `app-shell` / `schema-engine` / `document-session` は実装済みであり、その構造は以下のパターンに従っている。`src-tauri/` と `src/` の実体は `app-shell` が作り、`document-session` が文書の寿命と変更の唯一の経路を足した（下の「セッションの所有の規約」）。**残るスペックは GUI 側の構造をここから写す**（`app-shell` が確立した画面の契約・検証コードの置き場・機械検査の書き方は後述）。**エンジン側のスペックは「ドメインクレートの内部構造」から写す**（`document-format` と `schema-engine` が確立）。実体が規定から逸脱したら、逸脱を直すか本ファイルを更新するかをその場で決める。

## Organization Philosophy

**エンジンと UI の分離を、ディレクトリ構造そのもので強制する。**

Rust ドメインクレートは Tauri に依存してはならない。この規則は「行儀の良さ」ではなく、テスト容易性とスペック分割の前提である。ドメインのテストに GUI 起動が必要になったら、層が壊れている。

依存の向きは常に一方向: **フロントエンド → Tauri コマンド層 → ドメインクレート**。逆流は許容しない。

## Directory Patterns

### Rust ドメインクレート
**Location**: `crates/<domain>/`
**Purpose**: 業務ロジックの実体。Tauri を知らない純粋なライブラリクレート
**規則**: `Cargo.toml` に `tauri` が現れたら誤り。スペック 1 つがおおむねクレート 1 つに対応する
**Example**: `crates/document-format/`、`crates/schema-engine/`、`crates/app-shell/`、`crates/document-session/`、`crates/macro-runtime/`

### ドメインクレートの内部構造（`document-format` / `schema-engine` が確立）
新しいドメインクレートもこの 4 つに従う。

- **依存の鎖を各層の `mod.rs` 冒頭に書く。** 層を一方向に並べ（例 `Ids / Value → Model → Json → Parts → Container → Api`、`error / types → declaration → registry → compile → { coerce, validate } → write → evolution → api`、**`error / state → session → change → table → api`**、`error / types → view → edit → history → transport → api`（`data-grid`）、**`error / source → surface → host → engine → types → api`**（`macro-runtime`。`engine → types` の逆向き参照が実装中に実際に生じ、綴りの源を `host/value.rs` へ寄せて解消した）、**左の層だけを参照する**。鎖の文言そのものを冒頭の doc に置くのは、越境が「読めば分かる」状態を保つためである。**例外は、理由を層の doc に書き、裁定の所在を design にも書いたものだけ認める**（現時点の唯一の例外は `document-format` の `model → json` — 未知フィールドの保持 `PreservedFields` を型として共有するため。**逆向きの辺は無い**。`crates/document-format/src/model/sheet.rs` の doc と `document-format/design.md` の「依存方向」に記録されている）
- **公開面は根の再輸出に集める。** 下位モジュールは `pub mod` のままでよく、根に `pub use` を並べる（`document-format` / `app-shell` と同じ形）。**下流は根の名前だけを使う。** これは**コンパイラ強制ではない規約**である（`pub mod` 経由で下位に到達できる）。強制したくなったら 3 クレート同時の設計変更として扱う
- **兄弟のドメインクレートへの依存は一方向に限る。** `document-format` が依存グラフの根であり、**下流はこれに依存してよい**（例 `schema-engine → document-format`）。逆流は不可。兄弟同士が循環する形は作らない
- **誤り型は判別可能な列挙体とし、診断に必要な文脈だけを持ち、表示用の文言を持たない**（表示は呼び出し元が組み立てる。`DocumentError` / `SchemaError` が同じ規約）。**「宣言・入力が壊れている」と「値が合わない」は別の型にする** — 前者は処理を止め、後者は止めない（1 つの型にすると「1 件の不正な値で全体が開けない」という振る舞いが型として表現できてしまう）

### セッションの所有の規約（`document-session` が確立。2026-09-14 の実測）

**開いた `Document` をメモリ上に保持する持ち主**（`crates/document-session`）が確立した規約である。**変更の経路を直接使う利用者は `data-grid` / `schema-editor` / `macro-runtime`** である（下の「共有される継ぎ目」）。`formula-engine` は `data-grid` と `macro-runtime` を依存に持ち、再計算の結果を `data-grid` の変更として同じ経路へ流す（`document-session` を直接の依存には持たない）。`version-control` は**保存の時機を観測する相乗り**であり、変更の経路の利用者ではない。

- **ウィンドウ 1 つにつきセッション 1 つ。** 表（`table::Sessions`）は `WindowLabel → Arc<Slot>` の対応を持ち、同じ窓には**同じ実体**を返す（`Arc` の共有がその手段であり、挿入は書きロックの下で二重に確認する）。表のロックは**参照と挿入・除去のためだけ**に取り、`Slot` の処理の間は保持しない — 10 万行の適用が他のウィンドウを待たせないためである（要件 3.6）
- **読み取りと変更の経路を 1 つに閉じる。** 変更は `change::edit`（`Slot::lock_for_change` で Guard を取り、閉包へ `&mut Document` を貸し、**同じ Guard の生存範囲の内側で**未保存と版を記録する）だけである。**「同じ Guard であること」は型では強制されない** — Guard を取り直す変異はコンパイルが通る。担保は**コードの形状**であり、doc にその旨を明記する。記録しない可変の貸出口（`Slot::with_document_mut`）は `#[cfg(test)]` に閉じ、production の可変経路を `change::edit` ただ 1 つに固定する
- **未保存（`AtomicBool`）と版（`AtomicU64`）は文書のロックの外に置く。** `DocumentHost::may_close` は**ブロック禁止**（基盤が `prevent_close()` を非ブロッキングに読む）ため、`may_close` が文書のロックを待つとデッドロックする。したがって判定に要る 2 つの値は原子値であり、**未保存の判定と、差し替え・印・版の更新は文書のロックを保持したまま行う**（判定と更新を同じ臨界区間に入れないと、差し替えで変更が黙って失われる）。**版は文書が入れ替わるか変更が適用されたときに 1 進む**（適用回数ではない — 入れ替えで据え置くと下流の窓が古い内容を表示し続ける）。この非ブロッキングは**デッドロック検出のための時間切れ**であり、速度の証拠としての閾値ではない（`verification.md`）旨をコメントに明記する
- **破棄の購読は適応層の入口 1 つに閉じる。** セッションを作る入口は `resolve` / `attach` / `create` の 3 つだけで、`src-tauri` 側では `session/watch.rs` の `WindowDestroyWatch` が「**ラベルでウィンドウを引き、購読を登録してから、表へ挿入する**」を行う（登録を先に行うので、挿入だけが済んで購読が無い状態は作られない）。**破棄の通知は `WebviewWindow::on_window_event` をテストから駆動できない**（`tauri::test` の `MockWindowDispatcher::on_window_event` は渡された閉包を保持しない。`mock_runtime.rs:711`）ため、`WindowDestroyEvents` の**縫い目 1 つ**に閉じ、本番は `TauriWindowEvents` が担う（`window/geometry.rs` の `GeometryRead` と同じ形）。**「ウィンドウ 1 つにつき 1 回」を保証するのは適応層側の登録済みラベルの集合**である（コアは購読の存在を知らない）。**ただし登録済みかどうかの検査と集合への挿入は別々のロック取得であり、原子的ではない** — 同じラベルへの同時の初回入口が 2 つの購読を作りうる（入口は IPC と起動経路に限られるため実際には影響しないが、絶対の保証ではない。`document-session/tasks.md` の 3.3 の Implementation Notes に記録がある）。`WebviewWindow::on_window_event` は戻り値を持たないので登録の失敗は検出できず、**取得と登録の間に破棄された場合の受け皿**（`forget_unresolvable`）を併せて持つ

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
**内部の分け方**（`app-shell` が確立）: `shell/` = 器（レイアウト・遷移・外観・画面単位のエラー隔離・終了拒否・描画通知）、`features/<feature>/` = 画面、`ipc/` = 生成された型と薄い呼び出しラッパ、`shared/` = 配信先中立の資産

### 個別画面の契約（`app-shell` が確立。全 UI スペックが従う）
画面は `src/shell/Layout.tsx` の**画面登録簿に 1 件登録する**だけで差し込まれる。守ることは 5 つ。

- **受け取るのは `ScreenProps { screenId, navigate }` だけ**（`src/shell/router.tsx`）。画面が自前のレイアウト・遷移・履歴を持ってはならない。遷移の仕組みはシェルに 1 つだけある
- **配色は持たない**。シェルが `<html>` に与える `var(--jxcel-*)` を参照する（明暗の追随が画面ごとの分岐なしに成立する理由）
- **エラー隔離はシェルの仕事**。境界は領域の中の画面 1 式だけを包むので、画面が投げても器は生き残る。画面側で例外を握り潰さない
- **ユーザー向けでない画面（3 OS の描画確認用など）は検証専用の経路に置く**。出荷物に到達経路を作らない（後述の検証コードの節）
- **別の画面のデータを操作する機能は、独立した画面にせず、その画面の中のパネルとして置く**（例 `macro-runtime` の実行の面はグリッド画面の中にある）。独立した画面にすると**操作している間に対象の表が見えず**、要件（実行の間も画面が使える等）を満たせない。設計が独立した画面を前提にしていても要件と食い違うなら**面の側を要件に合わせ、食い違いを design に記録する**

現在の領域の識別は `data-shell-screen` で外から読める。**後続スペックの画面もこの契約に従えば `Layout.tsx` の登録簿に 1 行足すだけで入る。**

### 配信先中立なフロントエンド資産
**Location**: `src/shared/`
**Purpose**: Tauri IPC に依存してはならないコード
**規則**: フォームレンダラのように、デスクトップ内と LAN 配信の Web ページの**両方**で動く必要があるものはここに置く。`window.__TAURI__` への参照が現れたら誤り

### マクロ向け型定義
**Location**: `types/`
**Purpose**: ホスト API と標準マクロライブラリの `.d.ts`
**規則**: 手書きしない。ドメインクレートから生成する。LSP の補完品質はここに直結する。**生成元は型を公開する側のクレートが持ち**（`cargo run -p macro-runtime --bin generate-macro-types` → `types/macro-host.d.ts`）、**生成物のパスと再生成コマンドはクレート内の定数**（`GENERATED_PATH` / `REGENERATE_COMMAND`）が唯一の源である。**ドリフトは結合テストで固定する**（`tests/macro_host_dts_drift.rs`。`src/ipc/bindings.ts` の `bindings_drift` と同じ形）。**上流クレート（`document-format` / `schema-engine`）に導出を足さない** — 境界の型の導出は `crates/app-shell/src/ipc/` の下に限る（`ipc-contract.md`）。カタログを写経せず、列挙体の全件走査（`TypeKind::ALL`）から作る

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

**10 万行を跨ぐ拡張点には一括メソッドを置き、既定実装を 1 件用の繰り返しにする。**拡張の実体は JS であることが多く、セルごとに境界を越えると性能予算に入らない。既定実装があるので単純な拡張は 1 件用だけ書けばよく、境界を越える実装だけが一括版を上書きする。既定実装と上書き実装が同じ結果を返すことは、拡張側のテストで固定する。

**一括メソッドを置くだけでは足りない。本番の一括経路からそれが呼ばれていることを示すこと。** `schema-engine` の実装で実際に踏んだ罠: トレイトに `validate_batch` があり、doc も単体テストも揃っていたのに、**検証の本番経路が呼んでいなかった**（拡張型がセルごとに境界を越え、要件が静かに未達のまま、テストは緑）。担うのは 2 者である。

- **一括経路を持つ側が結線する**。`schema-engine` は拡張型の列を第 1 段から外し、列ごとに 1 回だけ一括判定を呼ぶ（`validate_sheet` と列指定の再検証の双方が同じ経路を通る）
- **拡張を実装する側が上書きする**（`custom-types`）。**「列ごとに 1 回」は、呼び出し回数を数える観測で固定する** — 「速くなった」は証拠にならない（`verification.md`）

### 共有される継ぎ目
以下は複数スペックが触るため、変更時に必ず両側を確認する。

- **決定的シリアライズ**（`document-format` → `version-control`）— 差分の品質はここに全面依存する
- **undo / redo スタック**（`data-grid` ← `formula-engine`、`macro-runtime`）— 最初から共有可能な形で設計する
- **変更の適用の経路**（`document-session` ← `data-grid`、`schema-editor`、`macro-runtime`）— **所有権は `document-session` 側**（開いた `Document` を保持する唯一の持ち主であり、読み取りと可変の貸出の口を持つ）。下流は変更を `DocumentSessionsApi::edit` の閉包の内側で適用し、`document-format` の一括書き換え（`set_cells`）を直接呼ばない。**閉包の内側から同じセッションを呼び返してはならない**（ロックを保持したまま呼ぶので再入はデッドロックする）。適用の記録（未保存・版）は `document-session` が閉包と同じ臨界区間で行うので、下流は印を立てない
- **ホスト API の `.d.ts`**（`macro-runtime` → `macro-editor-lsp`）— 生成責任の所在を曖昧にしない
- **フォームレンダラ**（`form-builder` → `form-web-server`）— IPC 非依存を壊さない
- **サイドカー基盤**（`app-shell` → `macro-editor-lsp`）— 所有権は `app-shell` 側
- **コマンド登録の根**（`app-shell` → 全 UI スペック）— 登録一覧はコンパイル時に集中して列挙する必要があり、完全な動的登録はできない。**各機能は自分のモジュールに関数を持ち、根は列挙だけを行う**。ここに業務ロジックが漏れ出したら誤り
- **メニュー項目の登録口**（`app-shell` → 全 UI スペック）— 項目そのものは各機能が所有し、`app-shell` は登録口と競合検査だけを持つ。**項目を足すスペックは、項目数と一覧を主張している検査器（`scripts/check-menu-shortcut.sh` と `scripts/ci/macos/verify-menu-shortcuts.sh`）を同じ作業の中で更新すること** — 更新しないと Linux と macOS の CI 段が落ちる（`verification.md`「検査器の規約」の横断の規則）
- **IPC の境界**（`app-shell` → 全 UI スペック）— コマンド名の単一の源・生成される TypeScript・封筒の形・呼び出し元ウィンドウの取り方・権限の付与。**規約は `.kiro/steering/ipc-contract.md` に置く**（ここに業務ロジックが漏れ出したら誤り）

### 性能はドメイン側で守る
10 万行を跨ぐ処理で、行ごとに IPC 境界や JS 呼び出しを越えてはならない。バッチ経路をドメインクレートに用意し、UI 側は結果だけを受け取る。この規則は `schema-engine` の検証、`custom-types` の型チェック、`formula-engine` の再計算のすべてに適用される。

**そのバッチ経路の内側では動的ディスパッチを使わない。**列や行ごとの処理を `Box<dyn Fn>` の配列で回す形は、閉じた列挙体の直接マッチに対して実測で数倍遅い。判定の種類は有限なので列挙体で表し、パラメータは変種にインラインで持たせる。拡張点だけは実装を保持する必要があるため 1 変種に閉じ込め、上の一括メソッドで呼び出し回数を列ごとに 1 回へ落とす。

### 生成物は追跡するが手で編集しない
Rust の型定義から生成される TypeScript のように、**生成物をリポジトリに追跡する**場合がある。追跡する理由はドリフト検査の比較対象にするためであり、フロントエンドのビルドを Rust ツールチェーンから独立させるためでもある。生成物には手を入れない。直すのは生成元である。

### 機械検査は `scripts/` に置き CI から呼ぶ
「守られているか目視で確認する」で終わる規則は、いずれ守られなくなる。**不変条件は検査スクリプトとして書き、CI のゲートにする。**

- 依存バージョンの下限、性能予算、生成物のドリフト、権限の逸脱、配信先中立な資産の依存、コアクレートの tauri 非依存
- スクリプトは POSIX sh とし、3 OS のランナーで同じものが走ること
- **終了コードは 3 値で統一する**: `0` = 適合 / `1` = 逸脱 / `2` = 入力が使えない（生成物が無い・道具が無い）。**入力が無いときに 0 を返してはならない** — 静かに通るゲートはゲートではない。**対象が空であることが正常な場合**（まだ資産が無い置き場など）は 0 でよいが、**走査した件数を出力して、空であることが見えるようにする**
- **検査の対象を「無い」ことにした検査は、必ず負の対照を持つこと**（出荷物を渡したら落ちる、等）。無いことを確かめる検査は、対象を間違えても緑になる
- **共有する観測・後始末は `scripts/lib/` に置き、検査器はそれを読む**。ウィンドウ観測のように複数の検査器が同じ処理を要る場合、写すと片方だけ直る
- 逸脱の検出は**文字列ではなく構造**（JSON を解析する・構文木を走る）で行う。散文やコメントを誤検出する検査は、やがて無効化される
- **検査器は対象を引数で受け取る形にし、クレートを足したら CI の段も足す。**検査器が汎用でも、CI が呼んでいなければ不変条件は守られていない。**この規則が実際に破れていた例**: 2026-09-12 まで `check-core-deps.sh` は `app-shell` にしか掛かっておらず、性能予算の判定は `document-format` のパスに固定されていた。`schema-engine` の実装が両方を拡張して解消した（性能予算は `check-bench-budget.sh` が `large_document/*` と `large_sheet/*` の計測を 1 回の実行で判定する形になった）。**集合が「ディレクトリ配下の全部」である検査は、引数なしで全部を列挙する形にする** — `check-core-deps.sh` は引数が無いとき `crates/*/Cargo.toml` を列挙して全ドメインクレートを回し、CI もその形で呼ぶ。クレートを足したときに段への追記を忘れる余地がなくなる（`document-format` と `sidecar-smoke` が抜けていた取りこぼしは、この形で構造的に解消した）。**クレートを足すタスクには、検査器の段を足すところまでを含める**
- **CI の段そのものの実体は `scripts/ci/` に置き、ワークフローの段はそれを 1 行で呼ぶだけにする**（`scripts/ci/linux|macos|windows/` は OS ごと、`scripts/ci/common/` は 3 OS 共通。macOS の 3 段が使う Swift のスパイクは同じディレクトリの `*.swift`）。理由: 段の中身（1 段で数百行になる）を YAML のブロックへ埋めると、**差分のレビュー・ローカルでの実行・静的解析**（qlty の shellcheck は `*.sh` を見るが YAML の中は対象外）がいずれも効かない。**検査器（`check-*.sh`）と違い、ここはランナー固有の手順**なので POSIX sh に縛らず bash / PowerShell / Swift を使う。**振る舞いは抽出前と同一に保つ**: Actions が台本へ与えていた `-e`（`shell: bash` なら `-o pipefail`）と `$ErrorActionPreference = 'Stop'` は、各スクリプトの冒頭が自分で持つ（自分で `set -…` を書いている段はそのまま）
- **計測が存在しない状態で予算ゲートだけ先に結線しない。**判定器は計測が無いとき `2` を返すため、計測が入るまで CI が赤のままになる。ゲートの結線は計測を入れるタスクが行う

### 検証専用のコードは出荷物に入れない
検証のための到達経路（引き金・画面・記録）は必要だが、**配布物に載ってはならない**。Rust 側とフロントエンド側で手段が違う。

- **Rust**: 非既定の cargo feature `verification-triggers` の下に置く。既定ビルドには識別子すら残らない（`strings` で 0 件）
- **フロントエンド**: cargo feature は TypeScript を括れない。**ビルド時の定数で切り、参照を残さない**（`vite.config.ts` の `__JXCEL_VERIFICATION__`。検証用の形は `JXCEL_VERIFICATION_BUILD=1` で作る）。静的 import は消えないことがあるため、**動的 import ＋ 到達しない分岐**にしてバンドラに落とさせる
- 出荷物に無いことは検査で固定する（`scripts/check-shipping-bundle.sh`）。**注意: Tauri は資産を圧縮して埋め込むため、バイナリへの `strings` では JS の中身を見られない** — ビルド済み `dist` を見る

詳細な規約は `.kiro/steering/verification.md` に置く。

### スペック横断の作業ディレクトリ
仕様・設計・タスクは `.kiro/specs/<feature>/` に置く。プロジェクト方針は `.kiro/steering/` に置く。実装コードから仕様を参照するときは相対リンクを使う。

---
_Document patterns, not file trees. New files following patterns shouldn't require updates_

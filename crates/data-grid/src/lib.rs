//! jxcel データグリッド: 型付きのシートを人が読み書きするための表示状態・編集・履歴・窓の符号化。
//!
//! # 層の鎖
//!
//! 本クレートのモジュールは次の鎖の順に並ぶ。**各層は自分の左にある層だけを参照する**
//! （右の層から左を呼ぶことはなく、逆向きの参照も無い。design.md「内部の依存の向き」・
//! structure.md「ドメインクレートの内部構造」）。
//!
//! ```text
//! error / types → view → edit → history → transport → api
//! ```
//!
//! - `error` / `types` — 誤り型（宣言の誤りと値の不適合を混ぜない）と、座標（セルの位置、
//!   可視行の序数、行の区間、セルの範囲）。**鎖の最も左**であり、本クレートの他のどの層にも
//!   依存しない
//! - `view` — 並べ替え・絞り込みの適用結果としての行の順序と、可視行の序数に対する違反の
//!   索引。**ドキュメントを変更しない**（左の `types` だけを参照する）
//! - `edit` — 編集命令の定義と適用。`schema-engine` の判定を呼び、`document-format` を変更する
//!   （左の `types` / `view` を参照する）
//! - `history` — 取り消し履歴。命令と逆命令の対を積む（左の `types` / `edit` を参照する）
//! - `transport` — 窓の行データと違反の符号化。**64 ビット整数と 10 進数を数値として出さない**
//!   （左の `types` / `view` を参照する）
//! - `api` — 公開面の再輸出と、画面 1 枚ぶんの操作口 `GridSession`。左のすべての層を参照する
//!
//! この鎖の文言は各層の冒頭にも置く（design.md「内部の依存の向き」が要求する形）。
//!
//! # 依存方針の要約
//!
//! - **`tauri` に依存しない**（推移的依存も含む）。本クレートの意味論は GUI を起動せずに
//!   テストできなければならない。機械検査は `scripts/check-core-deps.sh` が引数なしで
//!   `crates/*/Cargo.toml` を全列挙して行う
//! - 依存してよい兄弟は `document-format`（文書モデル）と `schema-engine`（判定の唯一の源）の
//!   2 つだけである。**`app-shell` には依存しない**（境界用の型の組み立ては `src-tauri` の
//!   適応層が行う）
//! - **境界用の型を本クレートに置かない**。`ts-rs` の derive を許されるのは
//!   `crates/app-shell/src/ipc/` の下だけであり、ドメイン型（`CellValue::Int`・行の識別子等）は
//!   そのまま境界を越えられない
//!
//! 具体的な宣言とその理由は `Cargo.toml` の冒頭コメントを参照。
//!
//! # 現状
//!
//! タスク 1.1 がクレートの足場を作り、タスク 1.3 が鎖の最も左の 2 層
//! （[`error`] と [`types`]）を足した。ワークスペースの `members` に登録され、
//! `document-format` と `schema-engine` への path 依存だけを持ち、`tauri` 非依存の検査
//! （`scripts/check-core-deps.sh`）の対象に入り、criterion のベンチターゲット
//! （`benches/large_grid.rs`。実ベンチは後続のタスクが実装する）を宣言している。
//!
//! タスク 2.1 が [`view`] 層の入口（[`RowOrder`] と [`ViewSpec`]）を足した。並べ替えの
//! 比較は値の変種ごとの順序で行い、同値の行は [`RowId`](document_format::RowId) の順に
//! 並ぶ。**順序の導出は `&Document` しか受け取らない**（要件 8.5 を型の上で示す形。
//! `view` のモジュール docs 参照）。
//!
//! タスク 2.2 が [`view`] 層へ絞り込みを足した（[`FilterSpec`] の 5 変種と
//! [`ViewSpec::filters`]、可視行数と隠された行数を返す [`ViewSummary`]）。複数の絞り込みは
//! **積**であり、並べ替えは**絞り込んだ集合に対して**定まる。表示文字列の写し
//! （[`DisplayText`] / [`display_text`]）は本層が 1 つだけ持ち（**タスク 5.1 の
//! `WindowCodec` はこれを再利用する**）、違反ありの絞り込みは据え付けられた
//! [`ViolationPresence`] だけを読む（**判定も索引も持たない**。索引を作って据え付けるのは
//! 2.4 と 5.2 である。`view` のモジュール docs「違反ありの絞り込みは据え付けられた情報だけを
//! 見る」）。
//!
//! タスク 2.3 が [`view`] 層へ入れ子の展開を足した（[`ViewState`] / [`ExpansionState`]、
//! [`MAX_EXPANSION_DEPTH`]、そこから導かれる平坦な列の構成 [`ColumnLayout`]）。展開は
//! **窓が運ぶ列の数を変える**ためフロントエンドではなく本クレート側にあり、段数の上限が
//! 構成の発散を止める（上限に達した位置は [`Expandability::Capped`] で詳細表示へ委ねる。
//! 要件 5.1, 5.2, 5.3, 5.4, 5.6）。順序の再計算は [`ViewState::recompute_order`] が `&self`
//! で行うため、展開の状態を失う経路が型の上に無い。
//!
//! タスク 2.4 が [`view`] 層へ**可視行の序数に対する違反の索引**を足した
//! （[`ViolationIndex`]。design.md の File Structure Plan の `view/violations.rs`）。検証の
//! 結果（`SheetReport`）と、いまの表示の順序（[`RowOrder`]）から組み立て、**シートに存在する
//! 違反の総数**（要件 4.3）と**指定した位置から最も近い違反セル**（要件 4.4）を答える。
//! 索引は**可視行の序数**を鍵とし、描画の窓を読まないため表示範囲の外の違反にも到達する。
//! 入れ子の違反は**内側の位置**（[`NestedPath`]）を保ったまま載り（要件 4.5）、順序や絞り込みが
//! 変わったときは [`ViolationIndex::rekey`] が鍵を張り直す。同層の [`ViolationPresence`] を
//! 組み立てて [`RowOrder`] へ据え付ける経路（[`ViolationIndex::install`]）が 2.2 の
//! 「違反あり」の絞り込みへ渡す（依存は `ViolationIndex → RowOrder` の一方向である）。
//!
//! タスク 3.1 が [`edit`] 層の入口を足した（[`EditApply`] と [`EditCommand`]）。
//! **打たれた文字を受け取り、型システムの書き込み判定を編集経路として呼ぶ唯一の経路**で
//! あり、判定が返した値・変換・違反をそのまま写す（本層は判定の分岐を持たない。要件 3.3,
//! 3.4, 3.5）。1 セルの編集では再検証を**当該列に限定して**呼び、シート全件の検証を
//! 呼ばない（要件 11.4。観測は `EditSchemaQuery` の縫い目で呼び出しの形を数えて行う）。
//!
//! タスク 3.2 が [`EditCommand`] へ行の構造を変える 3 つの命令
//! （`InsertRows` / `RemoveRows` / `DuplicateRows`）を足した（要件 6.1, 6.2, 6.3, 6.4）。
//! 挿入位置は**文書の位置**として読む（可視の序数ではない。画面の位置に挿入したい呼び出し側が
//! [`RowOrder`] で写す）。挿入する行の値は宣言が供給し（`CompiledSchema::default_row`）、
//! 複製する行の値はドキュメントから写す — **どちらも打たれた文字ではない**ため、この経路は
//! 判定を呼ばず、**すべての列**を指定した再検証を 1 回だけ呼ぶ（要件 11.4 が禁じるのは
//! 1 セルの編集についての全件検証であり、行の集合を変える命令はそれに当たらない。全列を
//! 明示するのは「どの列を見たか」を呼び出しの形に残すためである）。一意制約の重複は
//! 中止せず違反として報告され、行数の変化は `EditOutcome` の `row_count` と `affected` に載る。
//! 入れ子の編集（3.3）と貼り付け（3.4）は変種を足して進む。
//!
//! 残りの層（`history` は群 4、`transport` / `api` は群 5）は設計の File Structure Plan に
//! 挙げられた順に後続のタスクが足す。**実体の無いモジュールを先に宣言しない**（錆びた宣言は、
//! 層の鎖が実際に守られているかを検査できなくする）。

pub mod edit;
pub mod error;
pub mod types;
pub mod view;

// 公開面は根の再輸出に集める（`structure.md`「ドメインクレートの内部構造」。下流は根の
// 名前だけを使う）。`error` / `types` / `view` の各層の名前をここへ並べる。
pub use error::GridError;
// 列の添字は上流 `schema-engine` の型そのものである（定義し直さない理由は `types` の
// モジュール doc を参照）。同じ型がここからも見えるようにする。
pub use types::{CellAddress, CellPosition, CellRange, ColumnIndex, NestedPath, NestedPathSegment};
pub use types::{RowOrdinal, RowSpan, SearchDirection};
// `view` 層の状態と指定: 表示の指定（並べ替えと絞り込み）、行の順序、要約、違反の有無の
// 据え付け、そして表示文字列の写し（5.1 が写しを作らずに使う唯一の源）。
pub use view::ViolationPresence;
pub use view::{display_text, DisplayText, FilterSpec, RowOrder, SortKey, ViewSpec, ViewSummary};
// `view` 層の入れ子の展開: 列ごとの展開の状態、段数の上限、そこから導かれる平坦な列の構成、
// 要素数の能力、詳細表示へ委ねる印。
pub use view::{
    derive_layout, ColumnLayout, ElementCount, Expandability, ExpansionState, LayoutColumn,
    ViewState, MAX_EXPANSION_DEPTH,
};
// `view` 層の違反の索引（タスク 2.4）: 可視行の序数を鍵とする索引、その総数と探索、内側の
// 位置の保持、鍵の張り直し、2.2 への据え付け。`GridSession::violation_total` /
// `find_violation`（タスク 5.2）がこの層を読む。
pub use view::{CellViolations, ColumnViolations, RowViolations, ViolationIndex};
// `edit` 層の適用（タスク 3.1）: 編集命令とその結果、変換の記録、そして `schema-engine` へ
// 問い合わせる縫い目。縫い目を根へ出すのは、要件 11.4 の**観測が本番の実装を包んで**
// 呼び出しの形（回数・渡った値・列の集合）を数えるためであり、包む対象が公開されていないと
// 「本番が呼んでいない模擬」しか数えられない（`structure.md`「一括メソッドを置くだけでは
// 足りない」）。本番の具象（`SchemaEngineQuery`）も同じ理由で公開する。
pub use edit::{
    CoercionNotice, EditApply, EditCommand, EditOutcome, EditSchemaQuery, SchemaEngineQuery,
};

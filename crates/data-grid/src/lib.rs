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
//! 入れ子の編集と貼り付け（3.4）は変種を足して進む。
//!
//! タスク 3.3 が [`EditCommand`] へ入れ子の値を書く `SetNested` を足した（要件 5.5, 5.7）。
//! 入れ子のセルを**構造を保った表現**として受け取り、構造を保ったまま書き戻す — 表現の形も
//! 相互変換も上流 `document-format` の `to_json_bytes` / `from_json_bytes` が唯一の源であり、
//! **本層は解析器を持たない**。解釈できない入力は `value` の不適合（停止しない）ではなく
//! **入力の破損**として [`GridError::NestedDecode`] で止まり、1 つのセルも書かない。解釈できた
//! 値が入れ子であるかは本層が判定せず、そのまま判定へ渡す（受理を決めるのは `schema-engine`
//! の規則表だけである）。入れ子の編集も **1 セルの編集**であり、呼び出しの形は 3.1 と同じで
//! ある（要件 11.4）。内側の違反の位置は判定と再検証の違反が保ち、[`NestedPath`] が写す
//! （観測の経路は `edit` のモジュール docs「入れ子の内側の違反の位置はどこで観測できるか」）。
//!
//! タスク 3.4 が [`EditCommand`] へ表形式テキストを貼り付ける `PasteRange` を足し、
//! `edit` 層へ [`PasteCodec`]（`edit/paste.rs`。design.md「File Structure Plan」の
//! 「表形式テキストとセル値の相互変換」）を足した（要件 7.3, 7.4, 7.5, 7.7, 8.9）。
//! テキストの規則（列の区切りは TAB、行の区切りは LF と CRLF、値は `"` で囲み、中の `""` は
//! `"` 1 つ）は同モジュールが唯一の源であり、同じ写しが選択範囲の複製（要件 7.2）にも使える。
//! 貼り付けの宛先は**錨（物理の行と列）**と**表示されている行の並び**の 2 つの成分で決まり、
//! 後者を呼び出し側（5.2 の `GridSession`）が `RowOrder` から渡す — 本層は表示の並びを持たず、
//! 隠れている行へ値が届く経路を型の上で残さない（要件 8.9）。矩形が既存の行数を超えるときは
//! 不足する行を宣言の既定値で**末尾へ補充**し（要件 7.4）、貼り付けは**行ごとの判定を呼ばず**
//! 一括の書き込みと再検証 1 回で完了する（要件 7.7, 11.5。**1 万行でも同じ呼び出しの形**で
//! あることをテストが数えて固定する）。
//!
//! タスク 4.1 が [`history`] 層の入口を足した（[`UndoStack`] / [`UndoEntry`] / [`UndoLabel`]）。
//! 命令と逆命令の対を積む**拡張点の所有者**であり、`formula-engine` と `macro-runtime` が
//! 後から**同じ 1 つの登録口**（[`UndoStack::push`]）で乗る（要件 9.1, 9.7。区分
//! [`UndoLabel::Recalculation`] / [`UndoLabel::MacroRun`] を先に用意してある）。履歴は
//! **ドキュメント単位**でありシートごとではない（要件 9.5）— マクロの実行が複数シートに
//! 跨る 1 つの操作になりうるためである。
//!
//! **逆命令は適用の時にしか作れない**（適用の後には変更前の値も、取り除かれた行も、その位置も
//! 存在しない。要件 9.1 / 6.6）。したがって `edit` 層へ [`EditApply::apply_with_inverse`] を
//! 足し、適用と**同じ本体**で対（[`HistoryPair`]）を組み立てて返す（[`EditApply::apply`] は
//! それを捨てるだけの委譲であり、3.1〜3.4 の呼び出しの形は 1 つも変わらない）。材料の型
//! （[`HistoryCommand`] / [`RestoredRow`]）は **`edit` 層に置く** — 適用の経路がその型を
//! 名指すためであり、`history` 層に置くと層の鎖（`error / types → view → edit → history`）が
//! 閉じない。**公開の [`EditCommand`] は 1 変種も増えない**。
//!
//! 材料は**表示文字列を経由しない**。セルの編集の逆命令は**変更前の値そのもの**を保持する —
//! 表示文字列へ写して書き戻す経路は、添付の列（hex のテキストになる）と入れ子の列
//! （要素数の要約になる）で値の変種を変えてしまい、元の状態を復元できない。行の削除の逆命令
//! （design.md の「復元用の内部命令」）は**値・識別子・位置**を保持し、適用は `Row` を
//! 復号の正規の入口（行データの wire 形式）から組み立てて差し戻す（`Row` は `Clone` を
//! 持たず、本クレートの外で組み立てられない）。1 回の貼り付けは**1 つの操作**として積まれ、
//! 1 回の適用で適用前の状態へ戻る（要件 7.6）。
//!
//! タスク 4.2 が [`history`] 層へ**上限による追い出し**（要件 9.6）と、取り消し・やり直しを
//! ドキュメントへ適用する口（[`UndoRedo`]。要件 9.2, 9.3）を足した。上限を超えたら
//! **古い側から**捨て、上限 **0 は「1 件も保持しない」** である（「無制限」に 0 を負わせない
//! 理由は [`history`] のモジュール docs）。取り消しの後に新しい操作を積むと**やり直しの対象は
//! 破棄**され（要件 9.4。これは 4.1 の `push` が既に行っていた — 4.2 はそれを破棄の帰結まで
//! 含めて固定する）、取り消し・やり直しは履歴へ積まれない（積むと「取り消しの取り消し」に
//! なる）。結果は [`EditOutcome`] であり、**影響を受けた行**を運ぶ。
//!
//! タスク 5.1 が [`transport`] 層の**窓の二進符号化**を足した（design.md の `WindowCodec`。
//! 要件 1.1, 1.2, 4.5, 11.2, 11.6）。窓は**行の識別子を生の 16 バイトのまま**運び、セルは
//! **表示文字列・変種の札・違反の札**で表す — **64 ビット整数と 10 進数を数値として出さない**
//! （design.md「Data Models / 窓の二進形式」）。入れ子のセルは**要約（要素数）だけ**を運び、
//! 構造そのものは運ばない（内側のどの位置が違反しているかは**札**として運ぶ。要件 4.5）。
//! 表示文字列の規則は `view` 層の [`DisplayText`] / [`display_text`] が唯一の源であり、
//! 本層は**写しを作らない**（利用者が画面で見た文字列と、窓が運ぶ文字列が食い違わない）。
//! 版（[`WINDOW_FORMAT_VERSION`]）と世代（[`Generation`]）を頭に持ち、同じ世代・同じ区間の
//! 要求は常に同じバイト列になり、**世代が一致しない要求は空の窓**（[`EMPTY_WINDOW`]）になる。
//! 符号化は [`Document`](document_format::Document) を**一切受け取らない**ため、10 万行を
//! 走査する経路が型の上に存在しない（要件 11.2 の費用は呼び出しの形で示す）。
//!
//! タスク 5.2 が [`api`] 層の**画面 1 枚ぶんの操作口**を足した（[`GridSession`]。design.md
//! の File Structure Plan の `api.rs`。要件 4.3, 4.6, 11.4）。表示状態（順序・違反の索引）、
//! 入れ子の展開、窓の世代を 1 つの型が所有し、**開く・列を返す・表示の指定を
//! 変える・窓を符号化する・編集を適用する・履歴を進める・次の違反を探す**を 1 つの口として
//! 公開する。**取り消し履歴は所有しない** — 履歴はドキュメント単位であり（要件 9.5）、
//! 適用・取り消し・やり直しが**呼び出しごとに可変参照で受け取る**（10.2 がウィンドウの保持へ
//! 移した。design.md「UndoStack」の所有者）。**文書は所有しない** — [`GridSession::open`] は
//! シートと計画だけを受け取り、以後の文書を触る呼び出しはすべて参照または可変参照を受け取る
//! （design.md「GridSession」の Responsibility）。
//!
//! 本タスクの核心は**違反の総数を全件検証の再実行なしで最新に保つこと**である
//! （design.md の Invariants「判定が返した違反との差分で索引を更新する」）。そのため
//! `edit` 層の [`EditOutcome`] が違反そのものと再検証した列を運ぶようになり
//! （**適用の経路が既に得ている報告の写しであり、追加の検証は 1 回も呼ばない**）、
//! `view` 層の [`ViolationIndex`] がその差分を載せる口
//! （`ViolationIndex::apply_report_delta`）を持つ。総数をどう閉じるかは [`api`] の
//! モジュール docs「違反の総数をどう閉じるか」が正典である（覆いが全列なら報告がシート
//! 全体であり、列に閉じているなら覆った列の旧違反数を索引から引いて足す）。
//! 観測は 3.1〜3.4 と同じ縫い目（[`EditSchemaQuery`]）で行い、編集・取り消し・やり直しの
//! 経路で `validate_sheet` が **0 回**であることを `tests/grid_session.rs` が数えて固定する。
//!
//! **列の構成は `view` 層の [`LayoutColumn`] をそのまま返す**（[`GridSession::columns`]）。
//! 境界の型（6.1 の `ColumnDescriptor`）を本クレートに足さないのは、`LayoutColumn` が
//! 6.1 の写しに要るものを全部持ち、写しを 2 つ持つと必ず食い違うためである（判断の根拠は
//! [`api`] のモジュール docs「design.md が開いたままにした点」）。
//!
//! 残りの層は設計の File Structure Plan に挙げられた順に後続のタスクが足す。
//! **実体の無いモジュールを先に宣言しない**（錆びた宣言は、層の鎖が実際に守られているかを
//! 検査できなくする）。
pub mod api;
pub mod edit;
pub mod error;
pub mod history;
pub mod transport;
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
pub use view::{DisplayText, FilterSpec, RowOrder, SortKey, ViewSpec, ViewSummary, display_text};
// `view` 層の入れ子の展開: 列ごとの展開の状態、段数の上限、そこから導かれる平坦な列の構成、
// 要素数の能力、詳細表示へ委ねる印。
pub use view::{
    ColumnDeclaration, ColumnLayout, ElementCount, Expandability, ExpansionState, LayoutColumn,
    LayoutMember, MAX_EXPANSION_DEPTH, ViewState, derive_layout,
};
// `view` 層の参照先の頁（タスク 10.3）: 参照先のシートの行を頁ごとに読む。要件 3.8 の入力手段
// （参照先の行からの選択）が要る材料であり、**件数の上限は境界が強制する**（本層は与えられた
// 件数を使う）。
pub use view::{ReferencePage, ReferenceRow, reference_page};
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
// `edit` 層の表形式テキストの相互変換（タスク 3.4）: 表形式テキストとセル値の矩形の相互変換。
// 5.2 の `GridSession` が選択範囲の複製（要件 7.2）と貼り付けの受理（要件 7.3）の双方に
// **この 1 つの写し**を使う（貼り付けの適用そのものは `EditCommand::PasteRange` が担う）。
pub use edit::paste::PasteCodec;
// `edit` 層の履歴の材料（タスク 4.1）: 履歴が積む**復元用の内部命令**と、適用の時に組んだ
// 命令と逆命令の対。材料の型を `edit` 層に置くのは、適用の経路（`EditApply`）がその型を
// 名指すためである — `history` 層に置くと層の鎖が閉じない（層の向きの説明は `history` の
// モジュール docs）。**公開の `EditCommand` は 1 変種も増えない**。
pub use edit::{HistoryCommand, HistoryPair, RestoredRow};
// `history` 層の取り消し履歴（タスク 4.1 と 4.2）: 命令と逆命令の対を積む**拡張点の所有者**と、
// 履歴と適用の経路を束ねて取り消し・やり直しをドキュメントへ適用する口。`formula-engine` /
// `macro-runtime` が同じ 1 つの登録口（`UndoStack::push`）で乗り、5.2 の `GridSession` が
// ドキュメントと一緒に保持する。
pub use history::{UndoEntry, UndoLabel, UndoRedo, UndoStack};
// `transport` 層の窓の二進符号化（タスク 5.1）: 可視範囲の行を境界の向こうへ運ぶ形。窓は
// **行の識別子を生のまま**運び、セルは**表示文字列・変種の札・違反の札**で表す —
// 64 ビット整数と 10 進数は**数値として出さない**（design.md「Data Models / 窓の二進形式」）。
// 表示文字列の規則は `view` 層の 1 つを再利用し（写しを作らない）、入れ子のセルは
// 要約（要素数）だけを運ぶ。6.3 の生バイト経路が `encode` を呼び、7.3 の窓の記憶が
// `decode_window` を呼ぶ。
pub use transport::{
    DecodedCell, DecodedRow, DecodedWindow, EMPTY_WINDOW, Generation, HEADER_LEN, ROW_KEY_LEN,
    VariantTag, WINDOW_FORMAT_VERSION, WindowCodec, WindowDecodeError, WindowRequest,
    WindowRowSource, decode_window, variant_tag,
};
// `api` 層の**画面 1 枚ぶんの操作口**（タスク 5.2）: 表示状態（順序・違反の索引・入れ子の
// 展開）と窓の世代を 1 つの型が所有する。**取り消し履歴は所有しない** — 履歴は
// ドキュメント単位であり（要件 9.5）、適用・取り消し・やり直しが呼び出しごとに可変参照で
// 受け取る（10.2 がウィンドウの保持へ移した）。**文書は所有しない** — 文書を触る
// 呼び出しはすべて参照または可変参照を受け取る（design.md「GridSession」の
// Responsibility）。`src-tauri` の適応層がこの 1 つをウィンドウごとに保持する。
pub use api::{DEFAULT_UNDO_LIMIT, GridSession};

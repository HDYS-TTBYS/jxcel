//! 画面 1 枚ぶんの操作口: [`GridSession`]（data-grid のタスク 5.2。要件 4.3, 4.6, 11.4）。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本層は鎖の最も右にあり、
//! 左のすべての層を参照する**（design.md「内部の依存の向き」。層の鎖の文言を各層の冒頭に
//! 置く規約は `structure.md`「ドメインクレートの内部構造」）。逆向きの参照は無い —
//! 左の層は本型を知らず、本型が左の層を組み合わせる。
//!
//! | 本層が組み合わせるもの | 層 | 本層での役割 |
//! |---|---|---|
//! | [`ViewState`] / [`RowOrder`] | `view` | 表示の指定と、そこから導いた可視行の順序 |
//! | [`ViolationIndex`] | `view` | 可視行の序数に対する違反の索引（総数と探索） |
//! | [`EditApply`] | `edit` | 編集命令の適用（判定は呼ばず、[`EditSchemaQuery`] へ委ねる） |
//! | [`UndoStack`] / [`UndoRedo`] | `history` | 命令と逆命令の対を積む履歴と、その適用 |
//! | [`WindowCodec`] | `transport` | 可視範囲の窓の二進符号化 |
//!
//! # 1 つの口にまとめるもの（タスク 5.2）
//!
//! 画面 1 枚が行う操作は 7 つである。それぞれがどの層へ降りるか、本層が何を足すかを
//! 先に示す。
//!
//! | 操作 | 要件 | 降りる先 | 本層が足すもの |
//! |---|---|---|---|
//! | 開く（[`GridSession::open`]） | 1.3, 1.4 | — | 前提の検査（列 0 本の計画を拒む）と、列の構成の導出 |
//! | 列を返す（[`GridSession::columns`]） | 5.1〜5.4 | `view`（[`derive_layout`]） | 展開の状態を保った構成の保持 |
//! | 表示の指定を変える（[`GridSession::set_view`]） | 8.3, 8.4, 8.7 | `view` | **索引の鍵の張り直し**と据え付けの入れ替え |
//! | 窓を符号化する（[`GridSession::encode_window`]） | 1.1, 1.2 | `transport` | いまの世代と、行の値を引く口 |
//! | 編集を適用する（[`GridSession::apply`]） | 3.3, 6.1, 7.3 | `edit` / `history` | **違反の差分での索引の更新**と、行の集合が変わったときの順序の導出
//! | 履歴を進める（[`GridSession::undo`] / [`GridSession::redo`]） | 9.2, 9.3 | `history` | 同上（編集と**同じ 1 つの道**を通る）
//! | 次の違反を探す（[`GridSession::find_violation`]） | 4.4 | — | 索引への委譲だけである |
//!
//! # `Document` と履歴を所有しない（design.md の Responsibility）
//!
//! [**本型は `Document` の欄を持たない。**] 文書の所有者は `document-session` であり
//! （design.md「GridSession」の Responsibilities & Constraints）、本型は呼び出しごとに
//! 参照（[`GridSession::set_view`] / [`GridSession::encode_window`]）または可変参照
//! （[`GridSession::apply`] / [`GridSession::undo`] / [`GridSession::redo`]）を受け取る。
//! [`GridSession::open`] は**文書を 1 つも受け取らない** — シートの識別子と計画だけで開く
//! （`tests/grid_session.rs` の `the_session_never_owns_the_document` が、セッションを
//! 落とした後も文書がそのまま使えることを表明する）。
//!
//! 所有しないことは**型の上に現れている**（欄の一覧に `Document` が無い）。文書を所有する
//! 実装を後から足そうとすれば欄が増え、[`GridSession::open`] の signature が変わる —
//! この 2 つの事実が同時に壊れるので、黙って所有者になることはできない。
//!
//! [**同じことが取り消し履歴（[`UndoStack`]）にも言える**]（要件 9.5、9.6。design.md
//! 「UndoStack（拡張点の所有者）」）。本型は履歴の欄を持たず、
//! [`GridSession::apply`] / [`GridSession::undo`] / [`GridSession::redo`] が
//! `&mut UndoStack` を**受け取る**。[`GridSession::set_view`] と
//! [`GridSession::encode_window`] は `&Document` だけを受け取るため、**履歴に触れる経路が
//! 型の上に無い**（表示の指定と窓の符号化は履歴を進めない）。
//!
//! **所有者は適応層のウィンドウの保持である**（`src-tauri/src/commands/grid.rs` の
//! `SheetEntry`）。所有者をこう置くのは、履歴が**ドキュメント単位**（要件 9.5）であり、
//! 本型が**シートごとに作り直される**（計画は開いたシートの列の宣言から落とす）ためである
//! — 履歴を本型が持つと、**シートを切り替えた瞬間に文書の履歴が消える**（9.5 の違反）。
//! 保持の側の規律（同じ文書ならシートの切り替えを越えて引き継ぎ、**保持しているシートが
//! 文書に無い**ときは捨てる）は適応層の docs が正典である。
//!
//! 取り違えても**古い履歴が別の文書へ届かない**ことは、本型が保証する: 文書を触る 3 つの
//! 経路はどれも `GridSession::sheet_of` を最初に通るため、履歴の材料が名乗るシートが
//! 文書に無ければ [`GridError::SchemaUnusable`] で止まる（命令は 1 つも適用されない）。
//!
//! # 違反の総数をどう閉じるか（本タスクの核心。要件 4.3, 4.6, 11.4）
//!
//! [`ViolationIndex::violation_total`]（design.md の `violation_total`）は**シート全体の
//! 違反の総数**である（要件 4.3 の「表示中のシートに存在する違反の総数」。絞り込みに依らない
//! ことは `view/violations.rs` のモジュール docs が定める）。design.md の Invariants は
//! これが `apply` / `undo` / `redo` の直後につねに最新であること、そしてその更新が
//! **全件検証の再実行ではなく、判定が返した違反との差分**であることを求める（要件 11.4 が
//! 1 セルの編集にシート全件の検証を禁じているため）。
//!
//! ## 何が差分として届くか
//!
//! [`EditOutcome`] は適用が既に得ている報告を運ぶ — `revalidated_columns`（適用が再検証
//! した列）、`violations`（その列に閉じた違反の一覧）、`violation_total`（その列に閉じた
//! 総数）である（`edit` のモジュール docs「違反は結果が運ぶ」の表）。
//!
//! ## 覆いが全列か、列に閉じているかで閉じ方が違う
//!
//! | `revalidated_columns` | 誰が作るか | 新しいシート総数 |
//! |---|---|---|
//! | **全列**（長さが宣言の列数と一致） | 行の追加・削除・複製、復元、合成、補充を伴う貼り付け | `outcome.violation_total` を**そのまま**使う（報告が既にシート全体である） |
//! | **一部の列** | 1 セルの編集（`SetCells` / `SetNested`）、補充を伴わない貼り付け | `旧総数 − 覆った列の旧違反数 + outcome.violation_total` |
//!
//! 「長さが宣言の列数と一致すれば全列である」理由: `revalidated_columns` は**昇順・重複なし**で、
//! 各要素は宣言の列数より小さい（`edit` が作る 3 つの経路のいずれもそうである）。したがって
//! 長さが列数に等しいとき、その集合は `0..列数` に他ならない。
//!
//! ## 列に閉じているときの差分がなぜ正しいか
//!
//! 覆った列の旧違反数（[`ViolationIndex::violations_in_columns`]）は、**索引がその列について
//! 保持している違反の数**である。索引は `set_view` で `ValidationOptions::unlimited` の
//! 全件検証から組み立てられるため、次の 2 つが成り立つ:
//!
//! 1. **保持は切られていない** — `indexed_violations() == violation_total()`（詳細な報告から
//!    組み立てたので、載せた件数が報告の総数に一致する）。したがって「索引がその列に持つ
//!    違反の数」は「編集前のシートのその列の違反の数」そのものである
//! 2. **報告はその列の全体を覆う** — `revalidate_columns` の契約は「指定した列に閉じた報告」で
//!    あり、行を跨ぐ性質（一意性・参照の実在）も指定した列に閉じて判定される
//!    （`schema-engine` の `validate_columns`）。したがって `outcome.violation_total` は
//!    **編集後のシートのその列の違反の数**そのものである
//!
//! 触っていない列の違反は編集で変わらない（編集が書くのは覆った列だけであり、行を跨ぐ性質も
//! 指定した列に閉じて判定される）ため、
//! `新しいシート総数 = 旧総数 − (覆った列の旧違反) + (覆った列の新違反)` が**厳密に**成り立つ。
//! これは全件検証を 1 回も呼ばない（要件 11.4）。覆った列は「編集が触れた列」であり、ふつうは
//! 1 本である — 費用は列の数に比例し、シートの全セルには比例しない。
//!
//! 索引が無いセッション（[`GridSession::set_view`] をまだ呼んでいない）では、この差分を
//! 据える先が無い。そのときは**据え置く**（[`GridSession::violation_total`] は 0 のままであり、
//! 「索引はまだ組み立てられていない」という `open` 直後の状態と同じである）。最初の
//! [`GridSession::set_view`] がシート全体から索引を組み立てるので、以後はつねに最新になる。
//!
//! ## 履歴の 1 歩が別のシートへ落ちたときは据え直さない
//!
//! 履歴はドキュメント単位であるため（要件 9.5）、[`GridSession::undo`] / [`GridSession::redo`]
//! が進める 1 歩は**表示しているシートとは別のシート**を指しうる。適用先は結果が名乗る
//! （[`EditOutcome::sheet`]）ため本層はそれを見分けられ、**別のシートのときは索引にも順序にも
//! 1 つも触れない** — 表示中のシートの中身は 1 つも変わっておらず、`set_view` が組み立てた
//! 総数・据え付け・鍵がそのまま最新だからである。据え直せば**表示中のシートの違反が黙って
//! 消える**（別のシートの報告で置き換えるためである）。規則の本体と限界は
//! [`GridSession::settle`] の docs「履歴の 1 歩が別のシートへ落ちたとき」にある。
//!
//! ## 据え付け（2.2 への引き渡し）も同じ差分で動く
//!
//! [`FilterSpec::HasViolation`] が読む据え付けは索引が組み立てる（[`ViolationIndex::presence`]）。
//! 差分の更新は索引の保持から据え付けを作り直し（[`ViolationIndex::apply_report_delta`]）、
//! 本層がそれを順序へ据え付ける（[`ViolationIndex::install`]）。据え付けを据え直さないと、
//! 違反が 1 件も無くなった行に列の印が残り、「違反あり」の絞り込みが古い答えを返す。
//!
//! # 表示の指定を変える（要件 8.3〜8.5, 8.7 と 5.3）
//!
//! [`GridSession::set_view`] は 4 つのことを 1 つの順序で行う。
//!
//! 1. **順序の導出**（`view` の [`RowOrder::recompute`]）。可視行数と隠された行数が決まる
//! 2. **据え付けの入れ替え**（[`ViolationIndex::install`]） — 絞り込みの**後**に見えるように、
//!    順序を導出する**前**に据え付ける
//! 3. **索引の鍵の張り直し**（[`ViolationIndex::rekey`]） — 並べ替えと絞り込みは
//!    「どの行が何番目に見えるか」を変えるため、違反の**位置**（要件 4.4 が答える序数）を
//!    新しい順序へ写し直す。行を鍵とする保持（違反そのもの）は張り直さない
//! 4. **代数（[`Generation`]）を 1 つ進める** — 古い世代の窓を捨てさせるため
//!
//! **文書は 1 つも変わらない。** signature が `&Document` であることがこれを型の上で示す
//! （要件 8.5。`tests/grid_session.rs` の `set_view_rekeys_the_index_and_keeps_the_expansion`
//! が文書全体の写しを前後で比べる）。**入れ子の展開の状態も失われない** — 順序の導出は
//! [`ViewState::recompute_order`]（`&self`）が行うため、展開の状態を書き換える経路が型の上に
//! 無い（要件 5.3。`view` のモジュール docs「展開の状態は順序の再計算で失われない」）。
//!
//! # 行の集合が変わったときだけ順序を導出し直す（要件 8.8, 1.7）
//!
//! 編集の後、**行の集合が変わったとき**（行の増減を伴う命令と、貼り付けの補充）だけ順序を
//! 導出し直す。変えるのは値だけの編集では順序を導出し直さない — 並べ替えの基準列の値を編集しても
//! 行の表示位置が動かないことが要件 8.8 の本体である。
//!
//! 変わったかどうかは**シートの行数**で見る（`edit` の [`EditOutcome::row_count`]）。
//! `affected` だけでは決まらない（貼り付けは行を書くことと補充することが同時に起きうる）ため、
//! 本層は行の数を適用の前後で比べ、加えて行を増減する命令の種類を手掛かりにする。
//! どちらかが真なら順序を導出し直す — 取りこぼすと、消えた行を窓が引く経路が残る（要件 1.7）。
//!
//! # 世代（[`Generation`]）をいつ進めるか
//!
//! 世代は窓の記憶（7.3）を捨てさせるための札である（`transport` のモジュール docs「世代」）。
//! 本層は**画面に見えているものが変わりうる操作**の後で進める:
//!
//! | 操作 | 進めるか | 理由 |
//! |---|---|---|
//! | [`GridSession::set_view`] | つねに | 可視の行の並びが変わる（並べ替え・絞り込み） |
//! | [`GridSession::set_expansion`] | つねに | 列の構成が変わる（窓の**セル**は宣言の列数のままである。後述「窓が運ぶ列」） |
//! | [`GridSession::apply`] / [`undo`][GridSession::undo] / [`redo`][GridSession::redo] | 適用が何かを書いたとき（`affected` が空でない） | 値が変わり、違反の札も変わりうる |
//! | 空の命令（何も書かない） | 進めない | 窓の内容が変わりようが無い |
//!
//! **進めることだけが契約である**（古い世代へは戻さない。`WindowCodec::set_generation` の docs）。
//!
//! # 窓が運ぶ列（5.1 の契約との境目）
//!
//! [`GridSession::encode_window`] が [`WindowCodec::encode`] へ渡す列の数は**宣言の列数**で
//! ある。入れ子の展開（[`GridSession::columns`] が返す構成）は窓の列を変えない — 5.1 の
//! `encode` の docs がこれを明示しており、「入れ子の展開が窓の列を変えるなら signature を
//! 見直す合図」と書いてある。本層はその見直しを行わない（**本タスクの範囲外**である）ため、
//! 窓は宣言の列をそのまま運び、展開は画面側が [`GridSession::columns`] を読んで描く。
//! 窓のセルの違反の札は**宣言の列の添字**で引かれ、内側の位置は [`NestedPath`] の並びとして
//! 運ばれる（要件 4.5）。
//!
//! # 貼り付けの宛先（要件 8.9）— セッションが表示の並びを埋める
//!
//! [`EditCommand::PasteRange`] は「表示されている行の並び」を要求する（`edit` の同変種の
//! docs）。**表示の並びを持つのは本層だけである**（[`RowOrder`] の所有者は本型である）ため、
//! 呼び出し側が空の並びを渡したときは**本層がいまの可視の並びで埋めてから**適用へ渡す。
//! 埋めないと、絞り込みが効いている間に隠れた行へ値が届く経路ができる（要件 8.9 の本体）。
//!
//! 呼び出し側が非空の並びを渡した場合は**そのまま渡す**（表示の並びを自前で決めたい
//! 呼び出し側の指定を上書きしない。`edit` 層の契約は「呼び出し側が表示の並びを渡す」であり、
//! 本層はその既定を提供するだけである）。
//!
//! # design.md が開いたままにした点（本層が決めたもの）
//!
//! | 開いた点 | 本層の決定 | 根拠 |
//! |---|---|---|
//! | `columns` の戻り値の型（`ColumnDescriptor` は 6.1 の境界型） | **`&[LayoutColumn]`**（2.3 の列の構成）を返す | 6.1 が境界型へ写すのに要るもの（列の添字・内側の位置・表示名・型の札・要素数の能力・展開の可否）は `LayoutColumn` が既に全部持つ。**並行する記述子を本クレートに足さない**（写しを 2 つ持つと必ず食い違う。design.md の File Structure Plan も境界型を 6.1 に割り当てている） |
//! | `GridSession` の構築（design.md は `open` だけを示す） | [`GridSession::open`] と、縫い目を差し替える [`GridSession::with_query`] の 2 つ | 要件 11.4 の観測（**全件検証を呼ばない**ことの証拠）は、本番の実装を包んだ縫い目を差し込んで**呼び出しの形を数える**ことでしか取れない（`structure.md`「一括メソッドを置くだけでは足りない」）。`open` は本番の縫い目（[`SchemaEngineQuery`]）で開く薄い入口である |
//! | 履歴の上限（design.md は「上限を持つ」とだけ定める） | [`DEFAULT_UNDO_LIMIT`]（**履歴の所有者**が履歴を作るときの既定） | 10 万行を扱う道具で、有界でない記憶の伸びる経路を既定で開かない（`history` のモジュール docs の同じ判断） |
//! | 展開の指定の口（design.md は `ViewState.expansion` を持つとだけ定める） | [`GridSession::set_expansion`] / [`GridSession::expansion`] | 展開は表示状態の一部であり（2.3）、順序の再計算では失われない（要件 5.3）。境界（6.1）が展開の指定を運ぶには、セッションに指定口と読み口が要る |
//! | 違反の総数の閉じ方（design.md は「差分で更新する」とだけ定める） | 前節「違反の総数をどう閉じるか」 | 要件 11.4 の下で厳密に成り立つ唯一の式である（覆った列の旧違反数を索引から引き、報告の総数を足す） |
//! | 編集が返す違反の運搬（design.md の `EditOutcome` は総数だけ） | `EditOutcome` が違反そのものを運ぶ（`edit` 層の変更） | 差分を組むには「どの違反が消えて生じたか」が要る。情報は適用の経路に既にあり（判定と報告）、捨てられていた。**追加の検証は 1 回も呼ばない**（`edit` のモジュール docs「違反は結果が運ぶ」） |
//!
//! # 決定性
//!
//! 同じ文書・同じ計画・同じ指定からは同じ観測結果が出る。本型は `HashMap` を持たず
//! （[`RowOrder`] / [`ViolationIndex`] / [`ViolationPresence`] がすべて [`BTreeMap`] を持つ。
//! `view` 層の規律である）、乱数も時刻も読まない。`tests/grid_session.rs` の
//! `the_session_is_deterministic` が、同じ入力から開いた 2 つのセッションが同じ数・同じ位置・
//! 同じ窓を答えることを表明する（生の識別子は実行ごとに変わるため比較しない —
//! `tests/common/sample.rs` の決定性の規則）。
//!
//! # 本モジュールが持たないもの
//!
//! - **文書の所有**（前述）。[`Document`] は呼び出しごとに借りるだけである
//! - **判定**。値が列の型に適合するかを決めるのは `schema-engine` であり、本層は
//!   `edit` 層の縫い目（[`EditSchemaQuery`]）をそのまま通す。本層が「この列は int だから」と
//!   いう分岐を書く箇所は 1 つも無い
//! - **編集の意味論**。命令の適用・逆命令の組み立て・違反の判定は `edit` 層と `history` 層の
//!   仕事である（本層は順序と索引の整合を取る）
//! - **境界の型**（`ts-rs` の derive を許される型）。本層はドメインの型をそのまま返し、
//!   `app-shell` の型への写しは `src-tauri` の適応層が行う（`lib.rs` の依存方針）
//! - **窓の記憶**（7.3）。どの窓を要求したかを覚えるのは画面側であり、本層はつねに
//!   いまの世代の窓を作る
//!
//! [`BTreeMap`]: std::collections::BTreeMap
//! [`derive_layout`]: crate::view::derive_layout
//! [`FilterSpec::HasViolation`]: crate::view::FilterSpec::HasViolation
//! [`NestedPath`]: crate::types::NestedPath
//! [`ViolationPresence`]: crate::view::ViolationPresence

use std::sync::Arc;

use document_format::{CellValue, Document, Row, RowId, Sheet, SheetId};
use schema_engine::{ColumnIndex, CompiledSchema, EditVerdict, SheetReport, ValidationOptions};

use crate::edit::{EditApply, EditCommand, EditOutcome, EditSchemaQuery, SchemaEngineQuery};
use crate::error::GridError;
use crate::history::{UndoEntry, UndoLabel, UndoRedo, UndoStack};
use crate::transport::{Generation, WindowCodec, WindowRequest, WindowRowSource};
use crate::types::{CellAddress, RowOrdinal, RowSpan, SearchDirection};
use crate::view::{
    ColumnLayout, ExpansionState, LayoutColumn, RowOrder, ViewSpec, ViewState, ViewSummary,
    ViolationIndex,
};

/// 取り消し履歴の上限の既定（要件 9.6）。
///
/// **履歴を作る側（所有者）が使う値である** — 本層の [`GridSession`] は履歴を所有しない
/// （モジュール docs「`Document` と履歴を所有しない」）ため、本定数は適応層のウィンドウの
/// 保持（`src-tauri/src/commands/grid.rs` の `SheetEntry`）が履歴を作るときの既定である。
///
/// [`UndoStack`] は上限を超えた対を**古い側から捨てる**（要件 9.6）。上限 **0 は「1 件も
/// 保持しない」** である（`history` のモジュール docs）。本定数は 0 を既定にしない —
/// 取り消しが 1 回も効かない保持を黙って作らないためである。
///
/// 値そのものは本層の決定である（design.md は「上限を持ち、超えたら古い側から捨てる」と
/// だけ定める）。1,000 件は、1 セルずつ打った 1,000 操作を取り消せることを意味する。
pub const DEFAULT_UNDO_LIMIT: usize = 1_000;

/// 画面 1 枚ぶんの操作口（design.md「GridSession」の Service Interface。要件 4.3, 4.6,
/// 11.4）。
///
/// 表示状態（[`ViewState`] / [`RowOrder`] / [`ViolationIndex`]）を所有し、編集の適用
/// （[`EditApply`]）と窓の符号化（[`WindowCodec`]）を束ねる。**文書と取り消し履歴は
/// 所有しない** — 文書を触る呼び出しは参照または可変参照を受け取り（モジュール docs
/// 「`Document` を所有しない」）、履歴は [`GridSession::apply`] / [`GridSession::undo`] /
/// [`GridSession::redo`] が `&mut UndoStack` として受け取る（同「履歴を所有しない」）。
/// 履歴の所有者は適応層のウィンドウの保持であり、**同じ文書のシートを切り替えても保たれる**
/// （要件 9.5）。
///
/// # スキーマは開いた時点で固定する
///
/// 本型は [`CompiledSchema`] を 1 つ保持し、列の添字はその計画に対する位置である。**スキーマが
/// 変わったらセッションを作り直す**（design.md「GridSession」の Risks）。列の添字がずれたまま
/// 編集すると、別の列へ値が届く。
///
/// # 何を答えるか
///
/// | 問い | メソッド | 要件 |
/// |---|---|---|
/// | 列の構成（入れ子の展開を含む） | [`GridSession::columns`] | 5.1〜5.4, 8.5 |
/// | 可視行数・隠れた行数 | [`GridSession::visible_row_count`] / [`GridSession::hidden_row_count`] | 8.7 |
/// | シートの違反の総数 | [`GridSession::violation_total`] | 4.3 |
/// | 表示範囲の外にも届く違反の探索 | [`GridSession::find_violation`] | 4.4 |
/// | いまの世代 | [`GridSession::generation`] | 1.1, 7.3 |
///
/// # 決定性
///
/// 内部に `HashMap` を持たない（モジュール docs「決定性」）。同じ入力からは同じ観測結果が出る。
pub struct GridSession {
    /// 画面が表示しているシートの識別子（本型が対象とする唯一のシート）。
    sheet: SheetId,
    /// 開いた時点の計画（列の添字はこの計画に対する位置である）。
    schema: CompiledSchema,
    /// 表示状態: 表示の指定と、入れ子の展開の状態（要件 5.3, 8.3, 8.4）。
    view: ViewState,
    /// 可視行の順序（[`GridSession::set_view`] と、行の集合が変わった編集の後に導出する）。
    order: RowOrder,
    /// 編集の適用の経路（判定は縫い目へ委ねる。要件 11.4）。
    apply: EditApply,
    /// 可視行の序数に対する違反の索引（要件 4.1, 4.3, 4.4, 4.5）。
    index: ViolationIndex,
    /// いまの展開の状態から導いた列の構成（[`GridSession::columns`] が返す）。
    ///
    /// **保持する理由**: [`ViewState::layout`] は構成を**値として**返すため、その借用を
    /// 呼び出し側へ渡せない（一時の値の内側を指す参照は返せない）。展開が変わったときだけ
    /// 導出し直す（[`GridSession::set_expansion`]）。
    layout: ColumnLayout,
    /// 窓の符号化（いまの世代を所有する。[`GridSession::encode_window`]）。
    codec: WindowCodec,
    /// 判定の縫い目（要件 11.4 の観測の口）。
    ///
    /// [`GridSession::set_view`] の索引の組み立てと、[`EditApply`] が持つ縫い目は**同じ
    /// 実装**を指す（[`GridSession::with_query`] が 1 つの [`Arc`] を両方へ配る）ため、
    /// 数える側は 1 か所で両方の経路の呼び出しを観測できる。
    query: Arc<dyn EditSchemaQuery>,
    /// 索引を組み立てたか（[`GridSession::set_view`] が最初に立てる）。
    ///
    /// **偽の間は違反の総数を据え置く** — シート全体を知る道が無く、かつ据える先の索引も
    /// 無いためである（モジュール docs「違反の総数をどう閉じるか」）。
    indexed: bool,
}

impl GridSession {
    /// シートと計画を指定してセッションを開く（design.md の Service Interface）。
    ///
    /// **文書は受け取らない** — 表示の順序も索引も、文書が渡される
    /// （[`GridSession::set_view`]）まで導出しない。開いた直後の状態は次のとおりである:
    ///
    /// | 問い | 開いた直後 |
    /// |---|---|
    /// | [`GridSession::columns`] | 展開が 1 つも無い構成（宣言の列そのもの） |
    /// | [`GridSession::visible_row_count`] / [`GridSession::hidden_row_count`] | 0（順序をまだ導出していない） |
    /// | [`GridSession::violation_total`] | 0（索引をまだ組み立てていない） |
    /// | [`GridSession::generation`] | [`Generation::FIRST`] |
    /// | [`GridSession::find_violation`] | `None`（索引が空である） |
    ///
    /// # 誤り
    ///
    /// 列を 1 本も宣言していない計画は [`GridError::SchemaUnusable`] である。要件 1.6 は
    /// **列 0 本のシートを正当とする**（表を描かない）ため、これは壊れた宣言ではなく
    /// 「編集も表示もできないスキーマ」である — その提示は画面がセッション無しに行う
    /// （`edit` 層の `usable_columns` と同じ判定である）。
    pub fn open(sheet: SheetId, schema: CompiledSchema) -> Result<Self, GridError> {
        Self::with_query(sheet, schema, Arc::new(SchemaEngineQuery))
    }

    /// 判定の縫い目を差し替えてセッションを開く（要件 11.4 の観測の口）。
    ///
    /// 本番の縫い目（[`SchemaEngineQuery`]）を包んだ実装を差し込むと、**索引の組み立てと
    /// 編集の適用の双方**を通った呼び出しを 1 か所で数えられる（`tests/grid_session.rs` の
    /// `CountingQuery` がこれを行う）。差し込まれた縫い目の結果は本番と同一であり、
    /// 数える側は「本番が実際に何を呼んだか」を観測する。
    ///
    /// [`GridSession::open`] は本番の縫い目で開く薄い入口である（既定の具象を名前で
    /// 指定できるようにしてある）。
    pub fn with_query<Q>(
        sheet: SheetId,
        schema: CompiledSchema,
        query: Arc<Q>,
    ) -> Result<Self, GridError>
    where
        Q: EditSchemaQuery + 'static,
    {
        if schema.column_count() == 0 {
            return Err(GridError::SchemaUnusable { sheet });
        }
        let query: Arc<dyn EditSchemaQuery> = query;
        let view = ViewState::new();
        let layout = view.layout(&schema);
        // 適用の経路へも**同じ実装**を渡す（`Arc` を包む薄い適合器を通す）。
        let apply = EditApply::with_query(
            sheet,
            schema.clone(),
            Box::new(SharedQuery(Arc::clone(&query))),
        );
        Ok(Self {
            sheet,
            schema,
            view,
            order: RowOrder::default(),
            apply,
            index: ViolationIndex::default(),
            layout,
            codec: WindowCodec::new(Generation::FIRST),
            query,
            indexed: false,
        })
    }

    /// いまの列の構成（左から右への表示順。入れ子の展開を含む。要件 5.1〜5.4）。
    ///
    /// 返るのは `view` 層の [`LayoutColumn`] の並びである —
    /// 展開の状態から導いた平坦な列であり、**要素の数が窓の列の数ではない**（窓は宣言の列数を
    /// 運ぶ。モジュール docs「窓が運ぶ列」）。境界の型（6.1 の `ColumnDescriptor`）への写しは
    /// 本クレートの外で行う（モジュール docs「design.md が開いたままにした点」）。
    ///
    /// 各要素は列の添字・内側の位置・表示名・葉の型の札・要素数の能力・展開の可否を持つ。
    pub fn columns(&self) -> &[LayoutColumn] {
        self.layout.columns()
    }

    /// いまの表示の順序で可視の行数（要件 8.7。design.md の `visible_row_count`）。
    ///
    /// [`GridSession::set_view`] を呼ぶまでは 0 である（順序をまだ導出していない）。
    #[inline]
    #[must_use]
    pub fn visible_row_count(&self) -> usize {
        self.order.len()
    }

    /// 絞り込みによって隠れている行数（要件 8.7。design.md の `hidden_row_count`）。
    ///
    /// [`GridSession::set_view`] を呼ぶまでは 0 である。可視行数との和は**つねにシートの
    /// 行数**に一致する（`RowOrder::recompute` が差として導出する。`view` のモジュール docs）。
    #[inline]
    #[must_use]
    pub fn hidden_row_count(&self) -> usize {
        self.order.hidden()
    }

    /// シートに存在する違反の総数（要件 4.3。design.md の `violation_total`）。
    ///
    /// **絞り込みに依らない** — 隠れている行の違反も、行に属さない列そのものの問題も数える
    /// （`view/violations.rs` のモジュール docs「違反の総数」）。
    ///
    /// [`GridSession::set_view`] をまだ呼ぶまでは 0 である（索引をまだ組み立てていない）。
    /// 以後は [`GridSession::apply`] / [`GridSession::undo`] / [`GridSession::redo`] の直後に
    /// つねに最新である（design.md の Invariants）。更新は**全件検証の再実行ではなく、
    /// 判定が返した違反との差分**で行う（モジュール docs「違反の総数をどう閉じるか」）。
    ///
    /// **適用先が表示中のシートであるとき**の話である — [`GridSession::undo`] /
    /// [`GridSession::redo`] が進める 1 歩は別のシートへ落ちうる（履歴はドキュメント単位で
    /// ある。要件 9.5）。そのときは索引に触れないので、この数は**表示中のシートの違反の
    /// 総数のまま**である（規則は [`GridSession::settle`] の docs）。
    #[inline]
    #[must_use]
    pub fn violation_total(&self) -> usize {
        self.index.violation_total()
    }

    /// いまの世代（[`GridSession::encode_window`] が窓へ書く札。要件 1.1, 7.3）。
    ///
    /// 進むのは表示が変わりうる操作の後だけである（表はモジュール docs「世代をいつ進めるか」）。
    #[inline]
    #[must_use]
    pub fn generation(&self) -> Generation {
        self.codec.generation()
    }

    /// 入れ子の列の展開を指定する（要件 5.1〜5.4）。
    ///
    /// 同じ列の既存の指定を置き換える（[`ViewState::set_expansion`] と同じ規則）。列の構成が
    /// 変わるため導出し直し（[`GridSession::columns`]）、世代を 1 つ進める（古い列の構成で
    /// 描かれた画面を捨てさせるため）。
    ///
    /// **順序は変わらない** — 展開は窓の**列**を変えるものであり、可視の行の集合には触れない
    /// （要件 5.3 は展開の状態が走査で失われないことを求める。順序の再計算は
    /// [`ViewState::recompute_order`] が `&self` であり、展開を書き換えられない）。
    pub fn set_expansion(&mut self, state: ExpansionState) {
        self.view.set_expansion(state);
        self.layout = self.view.layout(&self.schema);
        self.advance_generation();
    }

    /// いまの展開の指定（読み口。`view` 層の [`ExpansionState`] の並び）。
    ///
    /// 列ごとに 1 つであり、列の添字の昇順に並ぶ。**順序の再計算や表示の指定の変更で失われない**
    /// （要件 5.3。`tests/grid_session.rs` の `set_view_rekeys_the_index_and_keeps_the_expansion`
    /// が、この並びから導いた構成が [`GridSession::columns`] と一致することを表明する）。
    #[inline]
    #[must_use]
    pub fn expansion(&self) -> &[ExpansionState] {
        &self.view.expansion
    }

    /// 表示の指定（並べ替えと絞り込み）を変え、順序を導出し直す（要件 8.3, 8.4, 8.7）。
    ///
    /// 最初の呼び出しが**シート全体の索引**を組み立てる（このときだけ 1 回の全件検証を
    /// 縫い目へ通す。モジュール docs「違反の総数をどう閉じるか」）。2 回目以降は索引を
    /// 組み立て直さず、**鍵だけを張り直す**（並べ替えと絞り込みは違反そのものを変えない）。
    ///
    /// # 誤り
    ///
    /// 名指されたシートが文書に無い、または計画の列数と食い違う場合は
    /// [`GridError::SchemaUnusable`] である（`edit` 層の `target_sheet` と同じ判定）。
    ///
    /// # 文書を変更しない
    ///
    /// signature が `&Document` である（要件 8.5。design.md の Invariants）
    /// 。入れ子の展開の状態も失われない（モジュール docs「表示の指定を変える」）。
    pub fn set_view(&mut self, doc: &Document, spec: ViewSpec) -> Result<ViewSummary, GridError> {
        self.sheet_of(doc)?;
        self.view.spec = spec;

        if !self.indexed {
            // 索引の組み立ては**この経路だけ**が行う（全件検証 1 回）。序数の写像は
            // この直後に順序を導出してから張り直すため、組み立ての時点では作らない
            // （`build` の契約は「いまの順序」だが、本経路は同じ呼び出しの中で
            // `recompute_order` と `rekey` を行う）。
            let report = self.query.validate_sheet(
                doc,
                self.sheet,
                &self.schema,
                &ValidationOptions::unlimited(),
            );
            self.index = ViolationIndex::build(&report, &RowOrder::default());
            self.indexed = true;
        }

        // 据え付けを**順序の導出より先に**入れ替える（`HasViolation` の絞り込みが据え付けを
        // 読むためである）。
        self.index.install(&mut self.order);
        let summary = self.view.recompute_order(&mut self.order, doc, self.sheet);
        // 鍵を新しい順序へ張り直す（並べ替えと絞り込みは「どの行が何番目か」を変える）。
        self.index.rekey(&self.order);
        self.advance_generation();
        Ok(summary)
    }

    /// 可視範囲の窓を二進形式へ符号化する（要件 1.1, 1.2。design.md の `encode_window`）。
    ///
    /// 窓の行は**いまの可視の順序**（[`RowOrder::span`] の切り落としを含む）であり、セルは
    /// 表示文字列・変種の札・違反の札で表す（64 ビット整数と 10 進数を数値として出さない —
    /// 5.1 の契約）。列の数は**宣言の列数**である（モジュール docs「窓が運ぶ列」）。
    ///
    /// 世代は本型が所有し、[`WindowRequest`] へいまの世代を載せる（呼び出し側は世代を
    /// 渡さない — 渡す形にすると、画面が古い世代の窓を要求して空の窓を受け取る経路が
    /// 型の上に残る）。
    ///
    /// # 誤り
    ///
    /// | 誤り | いつ |
    /// |---|---|
    /// | [`GridError::SchemaUnusable`] | 名指されたシートが文書に無い、または列数が食い違う |
    /// | [`GridError::SpanOutOfRange`] | 開始序数が可視行数より後ろ（5.1 の契約） |
    /// | [`GridError::UnknownRow`] | 順序が指す行を文書が持たない |
    ///
    /// **文書を変更しない**（`&Document`。design.md の Invariants）。
    pub fn encode_window(&self, doc: &Document, span: RowSpan) -> Result<Vec<u8>, GridError> {
        let source = SessionRowSource {
            sheet: self.sheet_of(doc)?,
        };
        let request = WindowRequest::new(self.codec.generation(), span);
        self.codec.encode(
            &self.order,
            &self.index,
            self.schema.column_count(),
            &source,
            &request,
        )
    }

    /// 編集命令を適用する（要件 3.3, 6.1, 7.3。design.md の `apply`）。
    ///
    /// 適用そのものは `edit` 層（[`EditApply::apply_with_inverse`]）が行い、本層は 4 つを足す:
    ///
    /// 1. **表示の並びの補完** — 貼り付けの宛先（要件 8.9。モジュール docs「貼り付けの宛先」）
    /// 2. **可視の序数の解決** — 行の対象と挿入の位置が可視の序数で指されていれば、**適用の
    ///    直前に**いまの [`RowOrder`]（`&self.order`）で識別子・文書の位置へ解く（タスク 10.4。
    ///    `RowOrder` を持つのは本層だけであり、画面も境界も写像を持たない。要件 8.6）
    /// 3. **履歴への積み込み** — 適用が組んだ対（逆命令とやり直しの命令）を、**渡された履歴**
    ///    （`history`）へ積む（要件 9.1。状態を変えない適用は対を持たないため積まない）。
    ///    **積まれるのは識別子であり、可視の序数は 1 つも残らない**（`edit` のモジュール docs
    ///    「序数は逆命令に残さない」）
    /// 4. **索引と順序の整合** — 判定が返した違反との差分で索引を更新し（要件 4.6, 11.4）、
    ///    行の集合が変わったときだけ順序を導出し直す（要件 8.8, 1.7）
    ///
    /// # 履歴は受け取る（所有しない）
    ///
    /// `history` は**呼び出し側（適応層のウィンドウの保持）が所有する**履歴であり、本型は
    /// 借りて 1 件積むだけである（要件 9.5 の「ドキュメント単位」を、シートごとに作り直される
    /// 本型の上で保つ唯一の形。モジュール docs「履歴を所有しない」）。**不可分な単位で借りる**
    /// ので、「積む先がその呼び出しの間だけ別の履歴へ差し替わる」経路は無い。
    ///
    /// # 誤り
    ///
    /// 適用の失敗（`edit` 層の 5 つの誤り）はそのまま返る。**失敗したときは 1 つのセルも
    /// 書かず、索引も履歴も動かさない**（適用が失敗すれば対が返らず、本層は何もしない）。
    pub fn apply(
        &mut self,
        doc: &mut Document,
        history: &mut UndoStack,
        command: EditCommand,
    ) -> Result<EditOutcome, GridError> {
        let rows_before = self.sheet_of(doc)?.rows().len();
        // 表示の並びを先に埋める（区分（履歴の札）と、行の集合の手掛かりも同じ命令から取る）。
        let command = self.fill_paste(command);
        let label = UndoLabel::of_edit(&command);
        let structural = is_structural(&command);

        // **可視の序数の解決は適用の直前で 1 回だけ**（タスク 10.4）。`RowOrder` を持つのは
        // 本層だけであるため、いまの並びを `edit` 層へ渡す — 行の対象（削除・複製）と挿入の
        // 位置が可視の序数で指されていても、適用の道は 1 つしか無い。
        let (outcome, pair) = self.apply.apply_with_inverse(doc, command, &self.order)?;
        if let Some(pair) = pair {
            history.push(UndoEntry {
                label,
                inverse: pair.inverse,
                redo: pair.redo,
            });
        }
        self.settle(doc, &outcome, rows_before, structural)?;
        if !outcome.affected.is_empty() {
            self.advance_generation();
        }
        Ok(outcome)
    }

    /// 直前の操作の**前**の状態へ戻す（要件 9.2。design.md の `undo`）。
    ///
    /// 戻る操作が無ければ `Ok(None)` である（失敗ではない — 履歴の先頭である）。適用の失敗は
    /// そのまま返り、**履歴の位置も文書も動かない**（[`UndoRedo::undo`] の契約）。
    ///
    /// **履歴は所有せず受け取る**（[`GridSession::apply`] と同じ。要件 9.5）。
    ///
    /// 適用が成功したときは [`GridSession::apply`] と**同じ道**（`GridSession::settle`）を
    /// 通って索引と順序を整合させる — 取り消しもやり直しも「文書が変わった」という点で
    /// 編集と変わらない。
    ///
    /// **ただし、進めた 1 歩が表示しているシートと別のシートへ落ちたときは何も変わらない**
    /// — 履歴はドキュメント単位であるため（要件 9.5）、シートを切り替えた後の取り消しは
    /// 前のシートの操作を指す。そのとき `settle` は索引にも順序にも触れず、違反の総数と
    /// 探索の答えは表示中のシートのままである（規則は [`GridSession::settle`] の docs
    /// 「履歴の 1 歩が別のシートへ落ちたとき」）。
    pub fn undo(
        &mut self,
        doc: &mut Document,
        history: &mut UndoStack,
    ) -> Result<Option<EditOutcome>, GridError> {
        let rows_before = self.sheet_of(doc)?.rows().len();
        // 借用はこの 1 文で切れる（履歴と適用の経路を同時に借りるため、束ねた型を通す）。
        let outcome = UndoRedo::new(history, &mut self.apply).undo(doc, &self.order)?;
        match outcome {
            Some(outcome) => {
                // 戻る操作の種類は本層に届かないため、行の集合の変化は行数だけで見る。
                self.settle(doc, &outcome, rows_before, false)?;
                if !outcome.affected.is_empty() {
                    self.advance_generation();
                }
                Ok(Some(outcome))
            }
            None => Ok(None),
        }
    }

    /// 取り消した操作を**再び適用**する（要件 9.3。design.md の `redo`）。
    ///
    /// やり直す操作が無ければ `Ok(None)` である。取り消しの後に新しい操作が積まれていれば、
    /// やり直しの対象は破棄されている（要件 9.4）。誤りの扱いと履歴の受け取り方は
    /// [`GridSession::undo`] と同じである（要件 9.5）— **別のシートへ落ちた 1 歩で表示中の
    /// シートの索引に触れないこと**も同じである（[`GridSession::settle`] の docs）。
    pub fn redo(
        &mut self,
        doc: &mut Document,
        history: &mut UndoStack,
    ) -> Result<Option<EditOutcome>, GridError> {
        let rows_before = self.sheet_of(doc)?.rows().len();
        let outcome = UndoRedo::new(history, &mut self.apply).redo(doc, &self.order)?;
        match outcome {
            Some(outcome) => {
                self.settle(doc, &outcome, rows_before, false)?;
                if !outcome.affected.is_empty() {
                    self.advance_generation();
                }
                Ok(Some(outcome))
            }
            None => Ok(None),
        }
    }

    /// 指定した位置から、その向きで最も近い違反セル（要件 4.4。design.md の
    /// `find_violation`）。
    ///
    /// `from` は**両方向で含まれる**（`from` 自身が違反していればそのセルを返す）。返るのは
    /// **セルの位置**（物理の同一性。行と列の対）である — 表示の位置（序数）ではないため、
    /// 返った位置をそのまま編集の宛先に使うことも、[`RowOrder`] を通して表示の位置へ写すことも
    /// できる（要件 8.6）。
    ///
    /// **描画の窓を読まない** — 表示範囲の外の違反にも届く（要件 4.4）。行に属さない違反
    /// （列そのものの問題）は行を持たないため移動先にならない（総数には数える。
    /// [`GridSession::violation_total`]）。
    ///
    /// 委譲先は [`ViolationIndex::find`] だけであり、本層は解釈を足さない。
    #[inline]
    #[must_use]
    pub fn find_violation(
        &self,
        from: RowOrdinal,
        direction: SearchDirection,
    ) -> Option<CellAddress> {
        self.index.find(from, direction)
    }

    /// 適用の前後で索引と順序を整合させる（[`GridSession::apply`] / `undo` / `redo` の共通の後段）。
    ///
    /// # 順序
    ///
    /// 1. 行の集合が変わったかを見る（行を増減する命令であるか、あるいは行数が変わったか）
    /// 2. **違反の差分を索引へ据える** — 据えるのは `close_total` が閉じたシート総数である
    /// 3. 行の集合が変わったなら順序を導出し直し、索引の鍵を張り直す（据え付けを**導出より
    ///    先に**入れ替える — [`FilterSpec::HasViolation`] の絞り込みが据え付けを読むため）
    /// 4. 値だけの編集でも据え付けは入れ替える（索引の保持が変わったためである）
    ///
    /// # 値だけの編集では順序を導出し直さない（要件 8.8）
    ///
    /// 並べ替えの基準列の値を編集しても行の表示位置は動かない — これが要件 8.8 の本体であり、
    /// 本層は**行の集合が変わったときだけ**順序を導出し直す。`HasViolation` の絞り込みが
    /// 効いているときも同じ規律を通す（据え付けは入れ替わるが、可視の集合を導出し直すのは
    /// 次の [`GridSession::set_view`] と、行の集合が変わったときだけである）— 編集した行が
    /// 編集の確定と同時に画面から消えると、利用者は自分が直したセルを見失う。
    ///
    /// # 索引がまだ無いとき
    ///
    /// [`GridSession::set_view`] をまだ呼んでいないセッション（表示の指定が 1 つも無い）では
    /// 何もしない — 据える先の索引が無く、総数も据え置きである（モジュール docs「違反の総数を
    /// どう閉じるか」）。最初の [`GridSession::set_view`] がシート全体から索引を組み立てる。
    ///
    /// # 履歴の 1 歩が別のシートへ落ちたとき（本層は何もしない）
    ///
    /// 履歴はドキュメント単位であるため（要件 9.5）、[`GridSession::undo`] /
    /// [`GridSession::redo`] が進める 1 歩は、**表示しているシートとは別のシート**を指しうる。
    /// その適用の結果が記述するのは**別のシート**であり（[`EditOutcome::sheet`]）、その違反も
    /// 行数も影響を受けた行も別のシートのものである。したがって本層は**索引にも順序にも
    /// 1 つも触れない**:
    ///
    /// - **索引はそのままで正しい** — 表示中のシートの中身は 1 つも変わっていないため、
    ///   `set_view` が組み立てた違反の総数・据え付け・鍵はそのまま最新である
    /// - 触れば**表示中のシートの違反が黙って消える**（別のシートの報告で据え直すためである。
    ///   10.2 のレビューが実測した欠陥）
    /// - 行数の構造判定（`rows_before` / `rows_after`）も見ない — それは**適用先のシート**の
    ///   行数であり、表示中のシートの順序を導出し直す理由にはならない
    ///
    /// **世代はここでは扱わない** — `undo` / `redo` が「適用が何かを書いた」ときに進める
    /// （モジュール docs「世代をいつ進めるか」）。進んでも表示中のシートの窓の内容は変わらない
    /// ため、画面は同じ内容を取り直すだけである。
    ///
    /// # 限界（申し送り）
    ///
    /// **別のシートを参照する列**（`Expected::RowsOf`）を持つときは、参照先のシートの行が
    /// 変わることで表示中のシートの違反が変わりうる。本層はその経路を知らない（索引は
    /// `set_view` の 1 回の全件検証から組み立て、以後は差分で保つ）ため、この場合は表示中の
    /// シートの違反が据え置きになる。**本ラウンドは「表示中のシートの索引を触らない」を
    /// 優先する**（誤った材料で据え直すより、参照先の変化を反映しないほうが害が小さい）。
    ///
    /// # 誤り
    ///
    /// 名指されたシートが文書に無い場合（[`GridError::SchemaUnusable`]）。適用が成功した後に
    /// これが起きることは無い（同じ検査を適用の入口でも通している）が、行数を数える経路が
    /// 失敗しうるため signature は `Result` である。
    fn settle(
        &mut self,
        doc: &Document,
        outcome: &EditOutcome,
        rows_before: usize,
        structural_hint: bool,
    ) -> Result<(), GridError> {
        if !self.indexed || outcome.sheet != self.sheet {
            return Ok(());
        }
        let rows_after = self.sheet_of(doc)?.rows().len();
        let structural = structural_hint || rows_after != rows_before;

        if !outcome.revalidated_columns.is_empty() {
            // 総数は**差分の前に**読む（差分は索引の保持を載せ替えるため、旧違反数は
            // その前にしか読めない）。
            let total = self.close_total(outcome);
            self.index.apply_report_delta(
                &self.order,
                &outcome.revalidated_columns,
                &outcome.violations,
                total,
            );
            // 据え付けは保持から作られる（差分の後に据え直す。モジュール docs「据え付けも
            // 同じ差分で動く」）。
            self.index.install(&mut self.order);
        }
        if structural {
            // 順序を導出し直し（据え付けは上で入れ替えてある）、鍵を張り直す。
            self.view.recompute_order(&mut self.order, doc, self.sheet);
            self.index.rekey(&self.order);
        }
        Ok(())
    }

    /// 編集の後に索引へ据える**シート全体の違反の総数**を閉じる（モジュール docs
    /// 「違反の総数をどう閉じるか」）。
    ///
    /// 覆いが全列なら報告の総数をそのまま返し、列に閉じているなら
    /// `旧総数 − 覆った列の旧違反数 + 報告の総数` を返す。**どちらの経路も検証を呼ばない。**
    ///
    /// # 「覆いが全列である」の判定
    ///
    /// [`EditOutcome::revalidated_columns`] は**昇順・重複なし**であり、その各要素は宣言の
    /// 列数より小さい（`edit` が作る経路はどれもそうである — 編集が書いた列を昇順に畳んだ
    /// ものか、貼り付けの矩形の列の範囲である）。したがって**長さが列数に等しいとき、その
    /// 集合は `0..列数` に他ならない**。
    ///
    /// # 前提（索引が満たしている不変条件）
    ///
    /// - `indexed_violations() == violation_total()` — 索引は詳細な報告（`unlimited`）から
    ///   組み立てられ、差分も同じ等式を保つ。したがって「索引が覆った列に持つ違反の数」は
    ///   「編集前のシートのその列の違反の数」に等しく、引き算が負になることはない
    ///   （覆った列の違反は索引が載せている分の一部である）
    /// - 覆った列以外の違反は編集で変わらない（編集が書くのは覆った列だけである）
    fn close_total(&self, outcome: &EditOutcome) -> usize {
        if outcome.revalidated_columns.len() == self.schema.column_count() {
            // 覆いが全列である（報告がシート全体である）。
            return outcome.violation_total;
        }
        let previously = self
            .index
            .violations_in_columns(&outcome.revalidated_columns);
        self.index.violation_total() - previously + outcome.violation_total
    }

    /// 貼り付けの宛先（表示されている行の並び）を、いまの可視の順序で埋める（要件 8.9）。
    ///
    /// 呼び出し側が空の並びを渡したときだけ埋める（非空の指定は上書きしない。モジュール docs
    /// 「貼り付けの宛先」）。他の命令はそのまま返す。
    fn fill_paste(&self, command: EditCommand) -> EditCommand {
        match command {
            EditCommand::PasteRange { anchor, rows, text } if rows.is_empty() => {
                let rows: Vec<RowId> = (0..self.order.len())
                    .filter_map(|pointer| self.order.row_at(RowOrdinal::new(pointer)))
                    .collect();
                EditCommand::PasteRange { anchor, rows, text }
            }
            other => other,
        }
    }

    /// 名指されたシートを引く（本型の前提の検査。`edit` 層の `target_sheet` と同じ判定）。
    ///
    /// 文書に無い場合と、計画の列数と食い違う場合は [`GridError::SchemaUnusable`] である —
    /// どちらも「セッションが前提とするシートと計画が使えない状態」であり、列の添字が
    /// 食い違ったまま読み書きするより止まるほうが回復可能である。
    fn sheet_of<'d>(&self, doc: &'d Document) -> Result<&'d Sheet, GridError> {
        let sheet = doc
            .sheet_by_id(self.sheet)
            .ok_or(GridError::SchemaUnusable { sheet: self.sheet })?;
        if sheet.columns().len() != self.schema.column_count() {
            return Err(GridError::SchemaUnusable { sheet: self.sheet });
        }
        Ok(sheet)
    }

    /// 世代を 1 つ進める（進めることだけが契約である。`transport` のモジュール docs）。
    fn advance_generation(&mut self) {
        self.codec.set_generation(self.codec.generation().next());
    }
}

/// [`EditSchemaQuery`] を [`Arc`] 越しに呼ぶ薄い適合器（[`GridSession::with_query`] の下請け）。
///
/// [`EditApply::with_query`] は縫い目を [`Box`] で受け取り、本型は同じ縫い目を [`Arc`] で
/// 保持する（[`GridSession::set_view`] の索引の組み立てが直接呼ぶため）。2 つの入口を
/// **同じ実装**へ向けるためにこの適合器を通す — 別々の実装を渡すと、数える側が観測する
/// 呼び出しが経路によって食い違う（要件 11.4 の観測が成り立たなくなる）。
///
/// 判定も検証も行わない（呼び出しをそのまま渡すだけである）。
struct SharedQuery(Arc<dyn EditSchemaQuery>);

impl EditSchemaQuery for SharedQuery {
    fn judge_write(&self, schema: &CompiledSchema, values: Vec<CellValue>) -> EditVerdict {
        self.0.judge_write(schema, values)
    }

    fn revalidate_columns(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        columns: &[ColumnIndex],
        options: &ValidationOptions,
    ) -> SheetReport {
        self.0
            .revalidate_columns(doc, sheet, schema, columns, options)
    }

    fn validate_sheet(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        options: &ValidationOptions,
    ) -> SheetReport {
        self.0.validate_sheet(doc, sheet, schema, options)
    }
}

/// 窓の符号化が行の値を引く口（[`WindowRowSource`] の実装）。
///
/// 文書のシートを 1 回の符号化の間だけ借り、行の識別子で線形に探す。**索引を持たない**のは、
/// 索引を作る費用（10 万行の写像）が 1 回の窓の符号化に見合わないためである — 窓が引く行は
/// 表示範囲ぶん（ふつうは画面 1 枚ぶん）だけであり、[`WindowCodec::encode`] は 1 行につき
/// 1 回しか問い合わせない（5.1 の契約）。
struct SessionRowSource<'a> {
    /// 行を引くシート（`&Document` から借りる。本型は文書を所有しない）。
    sheet: &'a Sheet,
}

impl WindowRowSource for SessionRowSource<'_> {
    fn values(&self, row: RowId) -> Option<&[CellValue]> {
        self.sheet
            .rows()
            .iter()
            .find(|found| found.id() == row)
            .map(Row::values)
    }
}

/// 命令が**行の集合を変えうる**か（[`GridSession::settle`] へ渡す手掛かり）。
///
/// 行を追加・削除・複製する 3 つの命令である。**これだけでは足りない** — 状態を変えない
/// 命令（件数 0 の追加、空の削除）は対を持たず、行の数も変えないため、本層は行数の比較も
/// 併せて見る（モジュール docs「行の集合が変わったときだけ順序を導出し直す」）。
fn is_structural(command: &EditCommand) -> bool {
    matches!(
        command,
        EditCommand::InsertRows { .. }
            | EditCommand::RemoveRows { .. }
            | EditCommand::DuplicateRows { .. }
    )
}

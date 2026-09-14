//! 編集命令の定義と適用: [`EditApply`]（data-grid のタスク 3.1, 3.2。要件 3.3, 3.4, 3.5, 3.7,
//! 6.1, 6.2, 6.3, 6.4, 11.4）。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本層は左の `types` /
//! `view` と、上流の `schema-engine` / `document-format` を参照する**（design.md「内部の
//! 依存の向き」。層の鎖の文言を各層の冒頭に置く規約は `structure.md`
//! 「ドメインクレートの内部構造」）。
//!
//! # 判定の分岐を持たない（本層の主題）
//!
//! 値が列の型に適合するか、どの値へ変換されるかは `schema-engine` が決める。本層は
//! **判定を依頼し、返った [`EditVerdict`] を写すだけである**:
//!
//! | 本層が決めること | 本層が決めないこと |
//! |---|---|
//! | どの行のどの列へ書くか（[`CellAddress`]） | 打たれた文字が列の型に適合するか |
//! | 打たれた文字を値の**在否**へ写すこと（空文字は値なし。理由は `edited_value`） | 変換の規則（`schema-engine` の規則表が唯一の源） |
//! | 判定の結果をどの順でドキュメントへ書くか | 違反があるときに拒否するか（**編集経路は拒否しない**） |
//!
//! **`WriteOrigin::Edit` は決して拒否しない**（`schema-engine` 要件 6.1）。したがって本層に
//! 「編集が失敗して値が戻る」分岐は存在せず、書き込み値はつねに判定が返した値そのものである
//! （design.md「System Flows / 編集の適用と判定」）。型に適合しない値も**破棄されず**、
//! ドキュメントに残ったうえで違反として報告される（要件 3.5）。
//!
//! 本層が列の型を見る箇所は 1 つも無い。`schema-engine` の検証器
//! （`CompiledSchema::validator`）を引く経路も持たない — 引けば「この列は int だから…」と
//! いう分岐を書く誘因が生まれ、規則の二重化（遅かれ早かれ食い違う）が始まる。
//!
//! # 1 セルの編集の費用の形（要件 11.4）
//!
//! 1 セルの編集が呼ぶのは次の 2 つだけである:
//!
//! 1. **書き込みの判定 1 回**（[`EditSchemaQuery::judge_write`]）。`validate_write` は
//!    **1 行分の値**を受け取り、値の添字を列の添字として読む（`schema-engine` の不変条件）。
//!    したがって「1 セルの編集」は **行を読む → その列の値を打たれた文字へ置き換える →
//!    判定を呼ぶ**であり、渡すのは当該の 1 セルだけではない（1 セル分の値を単独で判定する
//!    口は上流に無い）。
//! 2. **当該列に限定した再検証 1 回**（[`EditSchemaQuery::revalidate_columns`]）。
//!
//! **シート全件の検証（[`EditSchemaQuery::validate_sheet`]）は呼ばない。**これが要件 11.4 の
//! 内容そのものである。仮にここで全件検証を呼ぶと、10 万行 × 30 列で 255 ミリ秒（上流の
//! 実測）が編集のたびに掛かり、要件 11.3 の 100 ミリ秒に入らない（design.md
//! 「Performance & Scalability」）。
//!
//! 費用の形は「行数に対する 1 パス」＋「編集した列 1 本の走査」である。前者は行の位置の索引
//! （[`EditApply::apply`] 参照。上流の `Document::set_cells` が同じ索引を 1 度作るのと同じ
//! 規律）であり、後者は [`EditSchemaQuery::revalidate_columns`] が**指定した列だけ**を舐める
//! ことによる（`schema-engine` の契約）。**編集したセルの数にも、まして列数にも比例しない。**
//!
//! # 観測の縫い目（[`EditSchemaQuery`]）
//!
//! 「全件検証を呼んでいない」ことは**速度では示せない**（`verification.md`
//! 「速度を証拠にしない。証拠は『呼び出しの形』で取る」）。示せるのは**呼び出しを数える**
//! ことだけであり、そのためには数えられる縫い目が要る。本層は `schema-engine` へ
//! **必ずこの縫い目を通して**問い合わせる:
//!
//! - 本番の実装は [`SchemaEngineQuery`] であり、`schema-engine` の公開面をそのまま呼ぶ。
//!   [`EditApply::new`] はこれを使う（**本番の経路が縫い目を通る**）。
//! - 検査は [`EditApply::with_query`] で**数える実装**を差し込み、呼び出しの形
//!   （回数・渡った値・列の集合）を観測する（`tests/edit_apply.rs`）。
//!
//! 縫い目が公開面にあるのは、数える側が**本番の実装を包めるようにする**ためである
//! （数える対象が本番の経路であることを、委譲先を写しではなく本番の実装にすることで保証する。
//! 「本番が呼んでいない模擬」を作らない、という `structure.md`
//! 「一括メソッドを置くだけでは足りない。本番の一括経路からそれが呼ばれていることを示すこと」
//! の規律そのものである）。
//!
//! 3 つ目の [`EditSchemaQuery::validate_sheet`] は本層が**呼ばない**。縫い目が
//! `schema-engine` の 3 つの入口を数えられる形で並べているのは、**呼ばないことを数えて
//! 示すため**である（口を置かなければ「呼ばない」ことも数えられない）。本番の実装はこれを
//! `validate_sheet` へ委ねる。後続（5.2 の `GridSession` の開設）が全件検証を要するときも、
//! 同じ縫い目を通れば呼び出しの形が観測できる。
//!
//! 縫い目は `Send + Sync` を要求する。操作口（5.2 の `GridSession`）はウィンドウごとの文脈
//! から呼ばれるため、保持する値はスレッドを跨げなければならない（`src-tauri` の
//! `manage`（`Send + Sync + 'static` を要求する）と同じ理由）。実装の状態は
//! **引数と返り値に現れない**ため、数を数える側は内部可変性（`Mutex`）で記録する
//! （`tests/edit_apply.rs` の `CountingQuery`）。
//!
//! # ドキュメントへの書き込みは 1 回であり、部分適用が無い
//!
//! 判定をすべて済ませてから、[`Document::set_cells`] を**1 回**呼ぶ（1 セルの編集でも
//! 複数セルの編集でも同じである）。事前検査（対象シートの存在・列数の一致・列の範囲・行の
//! 存在）は書き込みの前に済ませるため、1 つでも不正なら**判定も呼ばず、1 つのセルも書かない**。
//! 判定が返した値のうちドキュメントへ書くのは**編集した列の値**だけであり、行の他の列の値は
//! 判定が返したもの（＝入力そのもの）と一致するため触れない。
//!
//! ## 同じセルを 2 度書く命令
//!
//! 同じセルが複数回現れた命令は、**後ろのものを残す 1 回の書き込み**として扱う
//! （上流の一括経路（`Document::set_cells`）の契約と同じ last-wins）。判定に渡る値は
//! **実際に書かれる値**そのものである — 重複を畳んでから判定するため、「判定した値」と
//! 「書いた値」が食い違う経路は無い。
//!
//! # 行の構造を変える命令（要件 6.1, 6.2, 6.3, 6.4）
//!
//! [`EditCommand::InsertRows`] / [`EditCommand::RemoveRows`] /
//! [`EditCommand::DuplicateRows`] は行の集合を変える。**値の判定を要さない**という点で
//! `SetCells` と性格が異なる（下の「行の構造を変える命令の再検証」）。
//!
//! ## 挿入位置は**文書の位置**であり、可視の序数ではない（要件 8.6 の帰結）
//!
//! `at` は [`RowOrdinal`] として運ぶが、**本層はそれを文書の行順に対する位置として読む**
//! （上流の [`Document::insert_row_at`] が同じ空間の添字を取る）。可視の序数ではない。
//!
//! 理由は 3 つある。第 1 に、挿入される行はまだ存在しないため**行の識別子で指せない** —
//! 編集の宛先（要件 8.6）を識別子で表す規律が、この命令だけは適用できない。第 2 に、
//! 可視の序数が指すのは**導出された表示の並び**であり、それを与える `RowOrder` は
//! `view` 層が `&Document` から導く（要件 8.5。順序の導出はドキュメントを変更しない）。
//! 本層は適用の間 `&mut Document` を握るため、序数を解く手段を持たない — 持てば表示の
//! 都合（並べ替え・絞り込み）がドキュメントへ書き込む位置を決めることになり、
//! design.md「表示状態（ドキュメントに保存されない）」に反する。第 3 に、文書の位置は
//! **まさにこれから変えようとしている構造そのもの**の座標であり、同じ命令を 2 度適用しても
//! 同じ場所を指す（可視の序数は並べ替えの再計算で動く）。
//!
//! したがって**画面の位置に挿入したい呼び出し側が写す** — `RowOrder::row_at` で可視の
//! 序数から行そのものを得て、その行の文書の位置を渡す。この分担は 3.4 の貼り付けの
//! 錨（[`CellAddress`] が行の識別子を運ぶ）と同じ規律である: 表示の座標を物理の座標へ
//! 写すのは表示を持っている側の仕事であり、本層は物理の座標しか受け取らない
//! （`tests/edit_rows.rs` の
//! `an_insert_position_is_a_document_position_and_the_caller_translates_the_visible_ordinal`
//! が、2 つの空間が食い違う並べ替えの下で両方の渡し方を固定する）。
//!
//! ## 既定値の適用（要件 6.1）
//!
//! 上流の [`Document::insert_row_at`] は**値を持たない行**を作る（モデルは列の型を知らない）。
//! 既定値の源は宣言ただ 1 つ（[`CompiledSchema::default_row`]）であり、本層は
//! **それをそのまま書く** — 値を発明する分岐を持たず、既定値が値なしである列（宣言が既定値を
//! 持たない列）には値なしが入る。書くのは [`Document::set_row_values`] の 1 回であり、
//! 列ごとの書き込みに分けない（行 1 行分の値の置換は上流の 1 口である）。
//!
//! ## 複製は末尾へ足し、値をそのまま写す（要件 6.3）
//!
//! 複製は**末尾**（その時点の行数の位置）へ足す。元の行の**すぐ後ろではない** — 元の行の
//! 後ろへ差し込むと、複製した行が「選択された行と同じ値を持つ行」であることに加えて
//! 既存の行の位置まで動かし、`affected` が指す行の意味（増えた行そのもの）が曖昧になる。
//! 末尾へ足せば、**元の行は 1 つも動かない**。
//!
//! 値は**元の行の値の並びをそのまま写す**（[`Document::set_row_values`] に行の値の写しを渡す）。
//! 列数に満たない行も列数まで埋めない — 値を持たない列は上流でも値なしとして扱われ、
//! 埋めれば元の行と複製の値の並びが食い違い、保存（行データの符号化）の門で列数の不一致と
//! して現れる（`tests/edit_rows.rs` の `duplicating_a_short_row_copies_exactly_the_values_it_has`）。
//! 判定を通さないのも同じ理由である（通せば変換で値が変わりうる。要件 6.3 は「同じ値を持つ
//! 行」を求める）。**適合しない値もそのまま複製される** — 本層は違反を持つ行を特別扱いしない。
//!
//! 同じ行が要求に 2 度現れる場合は 1 回へ畳み、要求の並びではなく**シート順**に複製する
//! （上流の [`Document::remove_rows`] が取り除いた行をシート順で返すのと同じ規律:
//! 同じ行集合の要求は、引数の並びに依らず常に同じ結果になる）。
//!
//! ## 削除は 1 回の操作である（要件 6.2）
//!
//! 選択されたすべての行は [`Document::remove_rows`] の**1 回の呼び出し**で取り除く
//! （行ごとに呼ぶと、1 行ごとに並びの作り直しが走り、10 万行の範囲削除が要件 11 の予算を
//! 割る）。上流は事前検査を 1 パスで行い、**1 つでも不正なら 1 行も取り除かない**ため、
//! 「妥当な行を先に取り除いてから失敗する」経路が存在しない。`affected` は取り除かれた行を
//! **シート順**に持つ（要求の並びに依らない）。
//!
//! # 行の構造を変える命令の再検証（要件 11.4 との関係）
//!
//! 行の構造を変える命令は、**すべての列を指定した再検証を 1 回だけ**呼び、
//! [`EditSchemaQuery::judge_write`] と [`EditSchemaQuery::validate_sheet`] は**呼ばない**。
//!
//! **判定を呼ばない理由**は、この経路へ届く値が**打たれた文字ではない**ことである。
//! 挿入する行の値は宣言が供給し（[`CompiledSchema::default_row`]、適合はコンパイル時に
//! 検査済み）、複製する行の値はドキュメントに既にある値である。判定（`validate_write`）は
//! 打たれた文字を型へ変換する門であり、通せば値が変わりうる — 複製に通せば「同じ値を持つ行」
//! （要件 6.3）が崩れる。
//!
//! **列を絞らない理由**は、行の集合が変わると**すべての列**の違反が変わりうるためである。
//! 挿入した行はあらゆる列で値なし／既定値になり、削除した行を参照していた他の行の違反が
//! 消える。とくに一意性と参照の実在は**行を跨ぐ**性質であり、複製で生まれ、削除で解消する
//! （`CompiledSchema::unique_columns` を列の型ごとの走査からは導けない）。絞れば静かに
//! 過少報告になる列が生まれる。
//!
//! 要件 11.4 が禁じるのは**1 セルの編集についての全件検証**である（`validate_sheet` は
//! 10 万行 × 30 列で 255 ミリ秒（上流の実測）が編集のたびに掛かり、要件 11.3 の 100 ミリ秒に
//! 入らない）。行の構造を変える命令は**その 1 セルの編集ではない** — 1 回の命令で行の集合が
//! 変わり、応答の予算は操作単位（要件 11.5 の貼り付けが 1 万行で 3 秒であるのと同じ扱い）で
//! ある。したがって**全列を明示して 1 回**呼ぶ（`validate_sheet` を呼ぶのではない: 全列の
//! 指定は「どの列を見たか」を呼び出しの形に残す。数える側はこれを見る）。
//!
//! # 誤りの経路と部分適用の不在
//!
//! 行の操作の誤りは 3 つであり、いずれも**事前検査**（変更の前）が判別する:
//!
//! | 誤り | 返る変種 |
//! |---|---|
//! | 要求された行が対象シートに属さない | [`GridError::UnknownRow`] |
//! | 挿入位置が行数を超える | [`GridError::SpanOutOfRange`]（`span` は要求された位置と件数、`visible` は適用前の行数） |
//! | 計画が列を持たない・対象シートと食い違う | [`GridError::SchemaUnusable`] |
//!
//! どちらの経路でも**1 行も増減せず、縫い目も 1 回も呼ばれない**（再検証は構造を変えた後に
//! しか呼ばれない）。`at == 行数` は末尾への追加として妥当であり、`at > 行数` だけを拒む
//! （上流の [`Document::insert_row_at`] と同じ境界）。
//!
//! # 空の命令
//!
//! 行 0 件の削除・複製と件数 0 の追加は**成功し、何も変えず、縫い目を 1 回も呼ばない**
//! （3.1 の「空の `SetCells` は何も変えない」と同じ規則。状態を変えない命令を履歴に積むと、
//! 取り消しが何も戻さない操作になる）。件数 0 の追加は**位置を見ない**（何も挿入しないため、
//! 位置の妥当性も問わない）。ただし**セッションの前提は空の命令でも検査する**（列 0 本の
//! シートでは空の命令も [`GridError::SchemaUnusable`] になる）— 前提は命令の中身に依らない。
//!
//! # 変換の記録は表示文字列で運ぶ（要件 3.4）
//!
//! [`CoercionNotice`] の `before` / `after` は [`String`] である。境界（6.1）は
//! `CellValue` をそのまま運べず（`CellValue::Int` の 64 ビット整数と識別子を出せない。
//! design.md「Existing Architecture Analysis」）、`Document` の行も `Clone` を持たない。
//! したがって**表示文字列へ写してから**運ぶ。写しの規則は `view` 層の [`display_text`] が
//! 唯一の源であり（2.2 が定め、5.1 の `WindowCodec` も同じものを再利用する）、本層は
//! **2 つ目の写しを作らない**。画面に見えている文字列と、変換の前後として提示する文字列が
//! 食い違わないことは、この 1 つの源が保証する。
//!
//! # 違反の総数の源（[`EditOutcome::violation_total`]）
//!
//! 本タスクが `violation_total` に入れるのは、**再検証のために呼んだ列に閉じた総数**
//! （[`SheetReport::total_violations`]）である。要件 4.3 が求める「表示中のシートに存在する
//! 違反の総数」は**シート全体**の数であり、それを答えるのは 5.2 の `GridSession` である
//! （design.md の Invariants「`violation_total` は `apply` / `undo` / `redo` の直後につねに
//! 最新である。全件検証の再実行ではなく、判定が返した違反との差分で索引を更新する」）。
//! 群 3 の残りのタスク（3.2 の行の追加・削除・複製、3.4 の貼り付け）は再検証する列の集合を
//! 広げることでこの数を広げる。
//!
//! 本層が再検証の報告から読むのは**総数だけ**である。したがって保持の上限 0
//! （[`ValidationOptions::capped`]）で呼ぶ — 10 万行 × 1 列の違反一覧を保持する理由が無く、
//! 総数と違反を持つ行の一覧は上限に関わらず保たれる（`schema-engine` の契約）。違反の
//! **一覧**（どの行のどの位置か）が要るのは 5.2 であり、そちらは判定が返した違反を使う
//! （design.md の Invariants）。
//!
//! 1 行分の判定（[`EditVerdict`]）も違反を持つが、本層はそれを `violation_total` に写さない。
//! 書き込みの後に当該列を再検証した報告が**同じ位置・同じ理由**を持つためである（どちらも
//! 同じ値と同じ計画から導かれ、連続する違反の有無を除けば報告のほうが広い — 報告は行を跨ぐ
//! 性質（一意性と参照の実在）も含むが、1 行分の判定は含まない）。2 つを足すと同じ違反を
//! 二重に数える。
//!
//! # 履歴（4.x）との境目
//!
//! 本層は履歴を積まない（`UndoStack` は 4.1 が足す）。**空の `SetCells`（セル 0 個）は
//! 何も変えないため、適用の結果を「影響を受けた行なし・違反の総数 0」として返し、判定も
//! 再検証も呼ばない。** 4.1 はこれを履歴に積まないこと — 状態を変えない命令を積むと、
//! 取り消しが何も戻さない操作になる。
//!
//! # 群 3 の残りのタスクへの拡張
//!
//! [`EditCommand`] は `SetCells`（3.1）と行の構造を変える 3 つ（3.2）を持つ。3.3
//! （`SetNested`）と 3.4（`PasteRange`）は**変種を足す**ことで進み、本モジュールの構造
//! （事前検査 → 判定 → 1 回の書き込み → 再検証 → 写し）を作り直さない。行を増減する命令は
//! `row_count` が変わり、`affected` に増減した行が加わる。`SetNested` は打たれた文字が
//! JSON になるが、判定を呼ぶ形は変わらない（design.md「EditApply」の Implementation Notes）。
//!
//! # 履歴（4.x）が逆命令を組み立てるのに要るもの
//!
//! design.md「編集命令と逆命令の対応」は `InsertRows` / `DuplicateRows` の逆命令を
//! `RemoveRows`（追加された `RowId` を保持する）とし、`RemoveRows` の逆命令を「復元用の
//! 内部命令」（取り除いた `Row` の値・`RowId`・位置を保持する）とする。
//!
//! 本層は履歴を積まないが、**その材料を `EditOutcome` から取り出せる形にしてある**:
//! 追加された行の識別子は `affected` そのものであり、取り除かれた行の識別子も `affected` で
//! ある。取り除かれた**値**は本層に残らない（[`Row`](document_format::Row) は `Clone` を
//! 持たず、[`Document::remove_rows`] が返した行は本層の外へ出せない）ため、4.1 は
//! **適用の前に**対象の行の値を読んでおく（その読み口は `&Document` から既にある）か、
//! 本層へ返させるときに `affected` を広げる（3.1 の seam の形は変えない）。
//!
//! 位置まで要るのは `RemoveRows` の逆命令だけであり、位置は**適用前の文書の位置**である
//! （適用後には行が消えているため、後からは導けない）。

use std::collections::{HashMap, HashSet};

use document_format::{
    CellValue, CellWriteError, Document, RowId, RowInsertionError, RowRemovalError, Sheet, SheetId,
};
use schema_engine::{
    validate_columns, validate_sheet, validate_write, Coercion, ColumnIndex, CompiledSchema,
    EditVerdict, SheetReport, ValidationOptions, WriteOrigin, WriteVerdict,
};

use crate::error::GridError;
use crate::types::{CellAddress, RowOrdinal, RowSpan};
use crate::view::display_text;

/// 編集命令（design.md「EditApply」の Service Interface。要件 3.3, 6.1, 6.2, 6.3）。
///
/// 本タスクが持つのは `SetCells`（3.1）と、行の構造を変える 3 つ（3.2）である。
/// `SetNested`（3.3）と `PasteRange`（3.4）は後続のタスクが**変種として足す** — 既存の
/// 変種の形（セルの位置と打たれた文字の対、行の識別子の並び、挿入位置と件数）を変えないため、
/// 適用の経路（事前検査 → 判定 → 書き込み → 再検証）も作り直しにならない。
///
/// `SetCells` の値は**打たれた文字**として運ぶ。数値・真偽・日付として解釈するのは
/// `schema-engine` であり、本層もフロントエンドも値を型として扱わない（design.md 同節の
/// Implementation Notes）。**行の構造を変える 3 つは値を運ばない** — 挿入する行の値は宣言が
/// 供給し、複製する行の値はドキュメントから写す（モジュール docs「行の構造を変える命令」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditCommand {
    /// 指定したセルへ打たれた文字を書く。
    ///
    /// 同じ行の同じ列が複数回現れた場合、**後ろのものが残る**（適用は
    /// [`Document::set_cells`] の一括経路であり、その契約が last-wins である）。判定へ
    /// 渡る値も**実際に書かれる値**である（モジュール docs「同じセルを 2 度書く命令」）。
    SetCells {
        /// 書くセルと、そこへ打たれた文字。
        cells: Vec<(CellAddress, String)>,
    },
    /// 指定した**文書の位置**へ、値を持たない行を `count` 行足し、**宣言の既定値**を書く
    /// （要件 6.1）。
    ///
    /// `at` は**可視の序数ではなく、文書の行順に対する位置**である（`at == 行数` は末尾への
    /// 追加。モジュール docs「挿入位置は文書の位置であり、可視の序数ではない」）。画面の位置に
    /// 挿入したい呼び出し側は `RowOrder` で行そのものへ写してからその行の文書の位置を渡す。
    /// 追加された行の値は [`CompiledSchema::default_row`] そのものであり、**値を運ばない**
    /// （打たれた文字ではないため判定を通さない）。
    InsertRows {
        /// 挿入する**文書の位置**（適用前の行順に対する添字。行数までの値が妥当）。
        at: RowOrdinal,
        /// 挿入する行数。`0` は何も変えない。
        count: usize,
    },
    /// 選択された複数の行を**1 回の操作**として取り除く（要件 6.2）。
    ///
    /// 同じ行が 2 度現れる要求は 1 回へ畳む。`affected` は取り除かれた行を**シート順**に持つ
    /// （要求の並びに依らない）。対象シートに属さない行が 1 つでもあれば
    /// [`GridError::UnknownRow`] を返し、**1 行も取り除かない**。
    RemoveRows {
        /// 取り除く行。空なら何も変えない。
        rows: Vec<RowId>,
    },
    /// 選択された行と**同じ値を持つ行**を末尾へ足す（要件 6.3, 6.4）。
    ///
    /// 値は元の行の値の並びをそのまま写す（列数に満たない行も埋めない。モジュール docs
    /// 「複製は末尾へ足し、値をそのまま写す」）。同じ行が 2 度現れる要求は 1 回へ畳む。
    /// 一意制約に重複が生じても**中止しない** — 行は増え、重複は再検証の報告に現れる
    /// （要件 6.4）。
    DuplicateRows {
        /// 複製する元の行。空なら何も変えない。
        rows: Vec<RowId>,
    },
}

/// 編集を適用した結果（design.md「EditApply」の Service Interface。要件 3.4, 6.2）。
///
/// **判定が返したものを写しただけ**であり、本層が足す解釈は無い。`affected` と `row_count` は
/// 画面側が窓の記憶を捨てる（要件 1.7）ためと、行数の変化を提示する（要件 6.2）ための要約で
/// ある。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOutcome {
    /// 影響を受けた行の識別子（重複を畳み、命令に現れた順）。
    ///
    /// `SetCells` では**編集したセルの行**である（行の増減は無い）。行の構造を変える命令では
    /// **増減した行そのもの**である（`InsertRows` は挿入された行を文書の順に、`RemoveRows` は
    /// 取り除かれた行をシート順に、`DuplicateRows` は複製をシート順に）。design.md の
    /// Postconditions は `apply` がこれを必ず含むことを求める。
    ///
    /// design.md「編集命令と逆命令の対応」が `InsertRows` / `DuplicateRows` の逆命令
    /// （`RemoveRows`）へ渡す「追加された `RowId`」はこの欄である。
    pub affected: Vec<RowId>,
    /// 型強制によって値が変換されたセル（要件 3.4）。変換が起きなければ空である。
    ///
    /// 行の構造を変える命令では**つねに空**である — この経路へ届く値は打たれた文字ではなく、
    /// 判定を通さないため変換も起きない（モジュール docs「行の構造を変える命令の再検証」）。
    pub coercions: Vec<CoercionNotice>,
    /// 違反の総数（本タスクでは**再検証した列に閉じた総数**。モジュール docs
    /// 「違反の総数の源」）。行の構造を変える命令は**すべての列**を再検証するため、適用後の
    /// シートの違反の総数と一致する（同「行の構造を変える命令の再検証」）。
    pub violation_total: usize,
    /// 適用の**後**のシートの行数。`SetCells` は行を増減しないため、適用の前後で変わらない
    /// （行を増減する命令（3.2）がこの欄に変化を載せる）。
    pub row_count: usize,
}

/// 型強制によって値が変換されたことの記録（design.md「EditApply」の Service Interface。
/// 要件 3.4）。
///
/// 変換の**前と後**の双方を表示文字列として持つ。「変換が起きたこと」と「変換前の値」を
/// 人が確認できる形にするためである（要件 3.4）。表示文字列の写しは `view` 層の
/// [`display_text`] が唯一の源であり（モジュール docs「変換の記録は表示文字列で運ぶ」）、
/// 本層は 2 つ目の写しを持たない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoercionNotice {
    /// 変換が起きたセルの位置。
    pub cell: CellAddress,
    /// 変換**前**の値の表示文字列（打たれた文字そのもの）。
    pub before: String,
    /// 変換**後**の値の表示文字列（ドキュメントへ書かれた値）。
    pub after: String,
}

/// 編集経路が `schema-engine` へ問い合わせる口（要件 11.4 の観測の縫い目）。
///
/// 本層が `schema-engine` を呼ぶ経路はこの 3 つだけであり、[`EditApply`] は必ずこの縫い目を
/// 通る（モジュール docs「観測の縫い目」）。メソッドの意味は上流の同名の関数と同じである
/// （本層は判定も検証も持たず、依頼と写ししかしない）。
///
/// # 実装の契約
///
/// - [`EditSchemaQuery::judge_write`] は `WriteOrigin::Edit` の判定を返す。**`Rejected` を
///   持たない**（[`EditVerdict`] は 2 変種しか持たない）。返る `values` の長さは入力と同じで
///   あり、`coercions` はその並びと同じ長さで値ごとに 1 件が対応する（`schema-engine` の
///   契約。本条項は **本層が列の位置で値を読む**根拠である）。
/// - [`EditSchemaQuery::revalidate_columns`] は指定した列に閉じた報告を返す。列の並びは
///   結果に影響しない（上流が列添字の昇順へ正規化する）。
/// - [`EditSchemaQuery::validate_sheet`] は全件の報告を返す。**編集経路はこれを呼ばない。**
pub trait EditSchemaQuery: Send + Sync {
    /// 1 行分の書き込み判定（編集経路）。強制も含む（要件 3.3, 3.4, 3.5）。
    fn judge_write(&self, schema: &CompiledSchema, values: Vec<CellValue>) -> EditVerdict;

    /// 指定した列だけの再検証（要件 11.4）。
    fn revalidate_columns(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        columns: &[ColumnIndex],
        options: &ValidationOptions,
    ) -> SheetReport;

    /// シート全件の検証。**編集経路は呼ばない**（要件 11.4。数えて示すための口である）。
    fn validate_sheet(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        options: &ValidationOptions,
    ) -> SheetReport;
}

/// 本番の縫い目: `schema-engine` の公開面をそのまま呼ぶ（モジュール docs「観測の縫い目」）。
///
/// 状態を持たない。公開しているのは、数を数える検査が**この実装を包んで**呼び出しの形を
/// 観測できるようにするためである（数える対象が本番の経路であることを、委譲先を写しではなく
/// 本番の実装にすることで保証する）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SchemaEngineQuery;

impl EditSchemaQuery for SchemaEngineQuery {
    fn judge_write(&self, schema: &CompiledSchema, values: Vec<CellValue>) -> EditVerdict {
        match validate_write(WriteOrigin::Edit, schema, values) {
            WriteVerdict::Edit(verdict) => verdict,
            // `validate_write` は渡された経路の腕をそのまま返す（`write` 層の
            // `match origin`）ため、`WriteOrigin::Edit` を渡した本経路で収集側の腕は
            // 起こりえない。ここへ来たなら上流の写像が変わっている（判定の分岐ではなく
            // **不変条件の表明**である）。
            WriteVerdict::Collect(_) => {
                unreachable!("WriteOrigin::Edit の判定が収集経路の判定になった")
            }
        }
    }

    fn revalidate_columns(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        columns: &[ColumnIndex],
        options: &ValidationOptions,
    ) -> SheetReport {
        validate_columns(doc, sheet, schema, columns, options)
    }

    fn validate_sheet(
        &self,
        doc: &Document,
        sheet: SheetId,
        schema: &CompiledSchema,
        options: &ValidationOptions,
    ) -> SheetReport {
        validate_sheet(doc, sheet, schema, options)
    }
}

/// 編集命令を適用し、判定を `schema-engine` に委ねる唯一の経路（design.md「EditApply」）。
///
/// 対象シートと計画（[`CompiledSchema`]）を開いたときに 1 度だけ受け取り、以後の適用で
/// 使い回す。**計画は不変であり、スキーマが変われば作り直す**（design.md「GridSession」の
/// State Management「スキーマが変わったらセッションを作り直す」）— 作り直しを忘れると列の
/// 添字がずれる。本層は計画と対象シートの**列数の一致**を適用のたびに確かめるため、ずれた
/// 計画は静かに書き込まずに [`GridError::SchemaUnusable`] として止まる（列の添字が
/// ドキュメントの列名と食い違ったまま書くより、止まるほうが回復可能である）。
///
/// 一連の適用（[`EditApply::apply`]）は対象シートの行数を 1 度走査して行の位置の索引を作り
/// （費用は行数に対する 1 パスであり、セル数には依らない）、判定を呼び、ドキュメントへの
/// 書き込みを 1 回だけ行い、編集した列に限定した再検証を 1 回呼ぶ。
pub struct EditApply {
    /// 対象のシート。
    sheet: SheetId,
    /// 開いたときに解決した計画。
    schema: CompiledSchema,
    /// 判定の問い合わせ先（本番は [`SchemaEngineQuery`]）。
    query: Box<dyn EditSchemaQuery>,
}

impl EditApply {
    /// 本番の縫い目（[`SchemaEngineQuery`]）で適用の経路を作る。
    pub fn new(sheet: SheetId, schema: CompiledSchema) -> Self {
        Self::with_query(sheet, schema, Box::new(SchemaEngineQuery))
    }

    /// 問い合わせ先を差し替えて適用の経路を作る。
    ///
    /// 判定の**依頼の形**（何回・何を渡して呼んだか）を観測するための入口である
    /// （モジュール docs「観測の縫い目」。`tests/edit_apply.rs` が数える実装を差し込む）。
    pub fn with_query(
        sheet: SheetId,
        schema: CompiledSchema,
        query: Box<dyn EditSchemaQuery>,
    ) -> Self {
        Self {
            sheet,
            schema,
            query,
        }
    }

    /// 編集命令を適用し、判定が返した値・変換・違反をそのまま写した結果を返す
    /// （design.md「EditApply」の Service Interface。要件 3.3, 3.4, 3.5, 6.1, 6.2, 6.3, 6.4）。
    ///
    /// 失敗するのは**宣言・宛先が壊れている**場合だけである
    /// （[`GridError::SchemaUnusable`] / [`GridError::UnknownRow`] /
    /// [`GridError::ColumnOutOfRange`] / [`GridError::SpanOutOfRange`]）。値が型に適合しない
    /// ことは失敗ではない — 値はドキュメントに残り、違反として報告される（要件 3.5）。
    ///
    /// 失敗したときは**1 つのセルも書かず、1 行も増減しない**（事前検査を書き込みの前に
    /// 済ませる。モジュール docs「ドキュメントへの書き込みは 1 回であり、部分適用が無い」
    /// 「行の構造を変える命令」）。**縫い目も 1 回も呼ばれない** — 再検証は構造を変えた後に
    /// しか呼ばれない。
    pub fn apply(
        &mut self,
        doc: &mut Document,
        command: EditCommand,
    ) -> Result<EditOutcome, GridError> {
        match command {
            EditCommand::SetCells { cells } => self.set_cells(doc, cells),
            EditCommand::InsertRows { at, count } => self.insert_rows(doc, at, count),
            EditCommand::RemoveRows { rows } => self.remove_rows(doc, rows),
            EditCommand::DuplicateRows { rows } => self.duplicate_rows(doc, rows),
        }
    }

    /// セッションの前提を検査し、**この適用で使える列数**を返す（命令の中身に依らない）。
    ///
    /// 空の命令もこの検査を通る（前提は命令の中身に依らない。モジュール docs「空の命令」）。
    ///
    /// # セッションの前提
    ///
    /// 1. 計画が列を 1 本も持たない場合、書き込む宛先が存在しない。要件 1.6 は列 0 本の
    ///    シートを正当とする（表を描かない）ため、これは「編集できないスキーマ」であって
    ///    壊れた宣言ではない。
    /// 2. 計画の列数と対象シートの列数が食い違う場合、列の添字がドキュメントの列名と
    ///    対応しない（design.md の事前条件「`schema` は同じ `sheet` から `compile` した
    ///    ものであること」が破れている）。
    fn usable_columns(&self, doc: &Document) -> Result<usize, GridError> {
        let columns = self.schema.column_count();
        if columns == 0 {
            return Err(GridError::SchemaUnusable { sheet: self.sheet });
        }
        self.target_sheet(doc)?;
        Ok(columns)
    }

    /// 何も変えなかった適用の結果（空の命令）。
    ///
    /// `affected` は空、`coercions` は空、`violation_total` は 0、`row_count` は**適用後の**
    /// 行数（変わっていない）。**縫い目を 1 回も呼ばない** — 状態を変えない命令の違反の総数は
    /// 変わりようがなく、引き直せば要件 11.4 の費用を理由もなく払う
    /// （モジュール docs「空の命令」）。
    fn unchanged(&self, doc: &Document) -> Result<EditOutcome, GridError> {
        Ok(EditOutcome {
            affected: Vec::new(),
            coercions: Vec::new(),
            violation_total: 0,
            row_count: self.target_sheet(doc)?.rows().len(),
        })
    }

    /// 行の構造を変えた適用の結果: **すべての列**を 1 回だけ再検証し、増減した行と適用後の
    /// 行数を載せる（モジュール docs「行の構造を変える命令の再検証」）。
    ///
    /// 変換の記録は空である（この経路へ届く値は打たれた文字ではないため判定を通らない）。
    fn changed_rows(&self, doc: &Document, affected: Vec<RowId>) -> Result<EditOutcome, GridError> {
        Ok(EditOutcome {
            affected,
            coercions: Vec::new(),
            violation_total: self.revalidate_every_column(doc),
            row_count: self.target_sheet(doc)?.rows().len(),
        })
    }

    /// すべての列を指定した再検証を**1 回**呼び、その総数を返す。
    ///
    /// 列の集合は計画の列の添字を昇順に並べたものである（上流は列の並びを正規化するため
    /// 結果に影響しないが、数える側が「全列を指定した」ことを読める形にする）。
    /// [`EditSchemaQuery::validate_sheet`] を呼ばない理由はモジュール docs
    /// 「行の構造を変える命令の再検証」にある（全列の**指定**が呼び出しの形に残る）。
    fn revalidate_every_column(&self, doc: &Document) -> usize {
        let columns: Vec<ColumnIndex> = (0..self.schema.column_count())
            .map(ColumnIndex::new)
            .collect();
        self.query
            .revalidate_columns(
                doc,
                self.sheet,
                &self.schema,
                &columns,
                &ValidationOptions::capped(0),
            )
            .total_violations()
    }

    /// `InsertRows` の適用（[`EditApply::apply`] の本体。要件 6.1）。
    ///
    /// 空の命令（`count == 0`）は**位置を見ない** — 何も挿入しないため、位置の妥当性も
    /// 問わない（モジュール docs「空の命令」）。
    fn insert_rows(
        &mut self,
        doc: &mut Document,
        at: RowOrdinal,
        count: usize,
    ) -> Result<EditOutcome, GridError> {
        self.usable_columns(doc)?;
        if count == 0 {
            return self.unchanged(doc);
        }
        // 挿入位置の事前検査（`at == 行数` は末尾への追加として妥当）。
        let rows_before = self.target_sheet(doc)?.rows().len();
        if at.get() > rows_before {
            return Err(GridError::SpanOutOfRange {
                span: RowSpan::new(at, count),
                visible: rows_before,
            });
        }

        // 既定値の源は計画ただ 1 つ（宣言）。**行ごとに写す**（`defaults` を 1 つ作って
        // clone するだけであり、列ごとに値を組み立て直さない）。
        let defaults = self.schema.default_row();
        let mut inserted: Vec<RowId> = Vec::with_capacity(count);
        for offset in 0..count {
            // 位置は挿入のたびに 1 つずつ後ろへずれる（`at + offset` は挿入前の行順に対する
            // 位置であり、直前までの挿入で空けた分だけ後ろにある）。
            let index = at.get() + offset;
            let row = doc
                .insert_row_at(self.sheet, index)
                .map_err(|error| row_insertion_error(error, at, count))?;
            // 上流の挿入は**値を持たない行**を作る（モデルは列の型を知らない）。既定値を
            // 書くのは本層である（モジュール docs「既定値の適用」）。
            doc.set_row_values(self.sheet, row, defaults.clone())
                .map_err(|error| GridError::UnknownRow { row: error.row })?;
            inserted.push(row);
        }

        self.changed_rows(doc, inserted)
    }

    /// `RemoveRows` の適用（[`EditApply::apply`] の本体。要件 6.2）。
    fn remove_rows(
        &mut self,
        doc: &mut Document,
        rows: Vec<RowId>,
    ) -> Result<EditOutcome, GridError> {
        self.usable_columns(doc)?;
        if rows.is_empty() {
            return self.unchanged(doc);
        }
        // **1 回の呼び出し**で取り除く（上流が 1 パスで事前検査し、1 つでも不正なら 1 行も
        // 取り除かない。モジュール docs「削除は 1 回の操作である」）。返る行は**シート順**で
        // あり、`affected` はその識別子である（要求の並びに依らない。取り除かれた**値**は
        // 4.1 の逆命令が要る — モジュール docs「履歴（4.x）が逆命令を組み立てるのに要るもの」）。
        let removed = doc
            .remove_rows(self.sheet, &rows)
            .map_err(row_removal_error)?;
        let affected: Vec<RowId> = removed.iter().map(|row| row.id()).collect();

        self.changed_rows(doc, affected)
    }

    /// `DuplicateRows` の適用（[`EditApply::apply`] の本体。要件 6.3, 6.4）。
    ///
    /// 元の行の値を**そのまま写して**末尾へ足す。判定を通さないため、一意制約に重複が生じても
    /// 中止しない — 行は増え、重複は再検証の報告に現れる（要件 6.4）。
    fn duplicate_rows(
        &mut self,
        doc: &mut Document,
        rows: Vec<RowId>,
    ) -> Result<EditOutcome, GridError> {
        self.usable_columns(doc)?;
        if rows.is_empty() {
            return self.unchanged(doc);
        }

        // 事前検査（読み）: 要求された行を**文書の位置**へ写し、同じ行の 2 度の要求を畳んで
        // **シート順**に並べ、複製する値を読む。**1 つでも未知なら 1 行も足さない**
        // （モジュール docs「誤りの経路と部分適用の不在」）。
        let sources: Vec<Vec<CellValue>> = {
            let sheet = self.target_sheet(doc)?;
            let positions: HashMap<RowId, usize> = sheet
                .rows()
                .iter()
                .enumerate()
                .map(|(position, row)| (row.id(), position))
                .collect();
            let mut wanted: Vec<usize> = Vec::with_capacity(rows.len());
            let mut seen: HashSet<RowId> = HashSet::with_capacity(rows.len());
            for row in rows {
                let Some(position) = positions.get(&row).copied() else {
                    return Err(GridError::UnknownRow { row });
                };
                if seen.insert(row) {
                    wanted.push(position);
                }
            }
            wanted.sort_unstable();
            // 行の値の並びを**そのまま**写す（列数に満たない行も埋めない。モジュール docs
            // 「複製は末尾へ足し、値をそのまま写す」）。
            wanted
                .into_iter()
                .map(|position| sheet.rows()[position].values().to_vec())
                .collect()
        };

        let mut copies: Vec<RowId> = Vec::with_capacity(sources.len());
        // 末尾へ足す（`at == 行数`）。元の行は 1 つも動かず、`index` は足すたびに伸びる。
        let mut index = self.target_sheet(doc)?.rows().len();
        for values in sources {
            let copy = doc
                .insert_row_at(self.sheet, index)
                .map_err(|error| row_insertion_error(error, RowOrdinal::new(index), 1))?;
            doc.set_row_values(self.sheet, copy, values)
                .map_err(|error| GridError::UnknownRow { row: error.row })?;
            copies.push(copy);
            index += 1;
        }

        self.changed_rows(doc, copies)
    }

    /// `SetCells` の適用（[`EditApply::apply`] の本体）。
    fn set_cells(
        &mut self,
        doc: &mut Document,
        cells: Vec<(CellAddress, String)>,
    ) -> Result<EditOutcome, GridError> {
        // セッションの前提を先に検査する（命令の中身に依らない。2 つの前提の理由は
        // `usable_columns` の docs「セッションの前提」）。
        let columns = self.usable_columns(doc)?;
        // 空の命令は何も変えない（判定も再検証も呼ばない。モジュール docs「履歴（4.x）との
        // 境目」）。ただしセッションの前提は空の命令でも検査する（前提は命令に依らない）。
        if cells.is_empty() {
            return self.unchanged(doc);
        }

        // 事前検査（読み）。行の位置の索引を 1 度だけ作り、列の範囲と行の存在を確かめながら、
        // **編集の対象になった行ごとに**その行の値（打たれた文字を当該の列へ置いたもの）を
        // 組み立てる。**1 つでも不正なら判定も呼ばず、1 つのセルも書かない。**
        //
        // 同じセルが 2 度現れた命令はここで**後ろのものを残す 1 つの編集**へ畳む（モジュール
        // docs「同じセルを 2 度書く命令」）。畳んでから判定するため、判定に渡る値と実際に
        // 書かれる値が食い違う経路が無い。
        //
        // 行ごとにまとめるのは、上流の判定（`validate_write`）が**1 行分の値**を受け取る
        // ためである。1 つの行の複数のセルの編集は 1 回の判定で足りる（判定の回数は命令の
        // セル数ではなく**編集の対象になった行数**に等しい。要件 11.4 の費用の形）。
        let mut affected: Vec<RowId> = Vec::new();
        let mut rows: Vec<RowEdit> = Vec::new();
        {
            let sheet = self.target_sheet(doc)?;
            let positions: HashMap<RowId, usize> = sheet
                .rows()
                .iter()
                .enumerate()
                .map(|(position, row)| (row.id(), position))
                .collect();
            let mut seen: HashMap<RowId, usize> = HashMap::with_capacity(cells.len());
            for (address, text) in cells {
                let column = address.column();
                if column.index() >= columns {
                    return Err(GridError::ColumnOutOfRange {
                        column,
                        count: columns,
                    });
                }
                let Some(position) = positions.get(&address.row()).copied() else {
                    return Err(GridError::UnknownRow { row: address.row() });
                };
                match seen.get(&address.row()).copied() {
                    Some(index) => rows[index].edit(column, text),
                    None => {
                        seen.insert(address.row(), rows.len());
                        affected.push(address.row());
                        rows.push(RowEdit::new(
                            address.row(),
                            sheet.rows()[position].values().to_vec(),
                            column,
                            text,
                        ));
                    }
                }
            }
        }

        // 判定（型システム）。編集の対象になった行ごとに、その行の値を（当該の列を打たれた
        // 文字へ置き換えて）渡し、返った値と変換の記録をそのまま写す。列の型を見て受理を
        // 決める分岐はここに無い。
        let mut writes: Vec<(RowId, usize, CellValue)> = Vec::new();
        let mut coercions: Vec<CoercionNotice> = Vec::new();
        for row in rows {
            let (decided, recorded) = match self.query.judge_write(&self.schema, row.edited_values())
            {
                // どちらの腕でも、返った値と変換の記録をそのまま受け取る（**判定の分岐を
                // 書かない**）。判定が運ぶ違反は、書き込みの後の再検証が同じ位置・同じ理由で
                // 持つ（モジュール docs「違反の総数の源」）。
                EditVerdict::Accepted { values, coercions } => (values, coercions),
                EditVerdict::AcceptedWithViolations {
                    values, coercions, ..
                } => (values, coercions),
            };
            for (column, _) in &row.edits {
                // `decided` の長さは渡した値の並びと同じであり、`coercions` は値ごとに 1 件が
                // 対応する（`schema-engine` の契約。縫い目の docs 参照）。
                let written = decided[column.index()].clone();
                if let Some(Coercion::Converted { from }) = recorded.get(column.index()) {
                    coercions.push(CoercionNotice {
                        cell: CellAddress::new(row.row, *column),
                        before: display_text(from).into_owned(),
                        after: display_text(&written).into_owned(),
                    });
                }
                writes.push((row.row, column.index(), written));
            }
        }

        // 書き込みは 1 回（置換であって追加ではない。行の集合・並び・識別子は変わらない）。
        doc.set_cells(self.sheet, &writes).map_err(write_error)?;

        // 当該列に限定した再検証（**全件検証は呼ばない**。要件 11.4）。列の集合は編集した列を
        // 昇順に畳んだものであり、1 セルの編集では 1 列だけである。
        let mut revalidated: Vec<ColumnIndex> = writes
            .iter()
            .map(|(_, column, _)| ColumnIndex::new(*column))
            .collect();
        revalidated.sort_unstable();
        revalidated.dedup();
        let report = self.query.revalidate_columns(
            doc,
            self.sheet,
            &self.schema,
            &revalidated,
            &ValidationOptions::capped(0),
        );

        Ok(EditOutcome {
            affected,
            coercions,
            violation_total: report.total_violations(),
            // `SetCells` は行を増減しないため、適用の前後で同じ数になる。適用の後の行数を
            // 改めて読む（行数を変える命令（3.2）がこの経路をそのまま使えるようにする）。
            row_count: self.target_sheet(doc)?.rows().len(),
        })
    }

    /// 対象シートを引く。文書に無い場合と、計画の列数と食い違う場合は使用不能として返す。
    ///
    /// 文書にシートが無い状態・列数の食い違う計画は、セッションが前提とするシート（とその
    /// 計画）が使えない状態である（[`GridError`] の 5 変種にこれ以上適切な変種は無い。
    /// `error.rs` の docs「宣言・指定が壊れている」）。
    fn target_sheet<'d>(&self, doc: &'d Document) -> Result<&'d Sheet, GridError> {
        let sheet = doc
            .sheet_by_id(self.sheet)
            .ok_or(GridError::SchemaUnusable { sheet: self.sheet })?;
        if sheet.columns().len() != self.schema.column_count() {
            return Err(GridError::SchemaUnusable { sheet: self.sheet });
        }
        Ok(sheet)
    }
}

/// 打たれた文字を、判定へ渡すセル値へ写す。
///
/// **本層が行う唯一の解釈である。**空の文字列は**値なし**（[`CellValue::Null`]）とする —
/// 要件 3.7 の「値を消して値なしへ戻す」を、境界が運ぶ文字列だけで表せるようにするためで
/// ある。空でない文字列は [`CellValue::Text`] としてそのまま渡す（数値や日付として読むのは
/// `schema-engine` の規則表である）。
///
/// **列の型は見ない。**列ごとに変わるのは「適合するか」であり、それは `schema-engine` が
/// 決める。本層が写すのは値の**在否**だけであり、この規則はどの列にも同じく適用される。
///
/// 値なしと空のテキストは**画面で区別できない**（`view` 層の [`display_text`] は両者を空
/// 文字列に写し、「違反あり」以外の絞り込みも両者を同じ行として選ぶ）。したがってこの写しが
/// 画面から見える情報を失うことはなく、境界に「区別を運ぶ手段」も無い（design.md
/// 「EditApply」の Implementation Notes は値を文字列で運ぶと定めている）。
fn edited_value(text: &str) -> CellValue {
    if text.is_empty() {
        CellValue::Null
    } else {
        CellValue::Text(text.to_owned())
    }
}

/// 編集の対象になった **1 行分**の編集（判定へ渡す値と、書き込むセルの並び）。
///
/// 上流の判定は 1 行分の値を受け取るため、編集を**行ごとにまとめてから**判定する。行ごとに
/// まとめると、同じ行の複数のセルを編集しても判定は 1 回で済み、かつ**同じセルが 2 度現れた
/// 命令**を後ろのものを残す 1 つの編集へ畳める（モジュール docs「同じセルを 2 度書く命令」）。
struct RowEdit {
    /// 対象の行。
    row: RowId,
    /// 行の**適用前**の値（列の添字で並ぶ）。
    values: Vec<CellValue>,
    /// 書くセル（列の添字）と、そこへ打たれた文字（命令に現れた順）。
    edits: Vec<(ColumnIndex, String)>,
}

impl RowEdit {
    /// 行の値を読み取り、1 つ目の編集を載せて作る。
    fn new(row: RowId, mut values: Vec<CellValue>, column: ColumnIndex, text: String) -> Self {
        // 行が値を持たない列への書き込みは、値なしを挟んで位置を合わせる（`validate_write` は
        // 値の添字を列の添字として読む。値なしの列は上流でも値なしとして扱われる）。
        values.resize(values.len().max(column.index() + 1), CellValue::Null);
        Self {
            row,
            values,
            edits: vec![(column, text)],
        }
    }

    /// 同じ行の別のセル（または同じセルの 2 度目）の編集を載せる。
    ///
    /// 同じセルが既に載っている場合は**後ろのものを残す**（上流の一括経路の last-wins と
    /// 同じ規則。モジュール docs「同じセルを 2 度書く命令」）。
    fn edit(&mut self, column: ColumnIndex, text: String) {
        self.values
            .resize(self.values.len().max(column.index() + 1), CellValue::Null);
        match self
            .edits
            .iter_mut()
            .find(|(existing, _)| *existing == column)
        {
            Some(found) => found.1 = text,
            None => self.edits.push((column, text)),
        }
    }

    /// 判定へ渡す 1 行分の値（打たれた文字を当該の列へ置いたもの）。
    fn edited_values(&self) -> Vec<CellValue> {
        let mut values = self.values.clone();
        for (column, text) in &self.edits {
            values[column.index()] = edited_value(text);
        }
        values
    }
}

/// `document-format` の書き込みの誤りを本クレートの誤り型へ写す。
///
/// 本層の事前検査（列の範囲と行の存在）が同じ判定を先に通しているため、ここへ来るのは
/// **事前検査の後にドキュメントが変わった**場合だけである。それでも写しを置くのは、上流の
/// 誤りを捨てる（握り潰す）経路を作らないためである。文書にシートが無い場合は
/// [`GridError::SchemaUnusable`] へ写す（[`EditApply::target_sheet`] と同じ理由）。
fn write_error(error: CellWriteError) -> GridError {
    match error {
        CellWriteError::UnknownSheet { sheet } => GridError::SchemaUnusable { sheet },
        CellWriteError::UnknownRow { row } => GridError::UnknownRow { row },
        CellWriteError::UnknownColumn { column, columns } => GridError::ColumnOutOfRange {
            column: ColumnIndex::new(column),
            count: columns,
        },
    }
}

/// `document-format` の行の挿入の誤りを本クレートの誤り型へ写す。
///
/// 行の挿入の失敗は 3 つであり、本層の経路では 1 つしか起こりえない:
///
/// - **挿入位置が範囲外**（[`RowInsertionError::IndexOutOfRange`]）— 本層の事前検査
///   （`at > 行数`）が同じ判定を先に通しているため、ここへ来るのは事前検査の後に
///   ドキュメントが変わった場合だけである。それでも写しを置くのは、上流の誤りを捨てる
///   （握り潰す）経路を作らないためである。診断に載せる位置と件数は**呼び出し元が要求した
///   もの**（`at` / `count`）である — 画面へ返すのは利用者が指定した位置であり、
///   複数行の挿入では上流が見る添字（1 行ずつ後ろへずれる）と一致しない。
/// - **識別子が既にある**（[`RowInsertionError::DuplicateRow`]）— [`Document::insert_row_at`]
///   は識別子を発行元から受け取るため、この腕は起こりえない（上流の docs「発行直後の
///   識別子は文書のどのシートにも無く、一意性は構築で保証される」）。
/// - **シートが無い**（[`RowInsertionError::UnknownSheet`]）— [`EditApply::target_sheet`] が
///   先に同じ判定を通している。
///
/// 起こりえない 2 つを黙って捨てないのは、上流の写像が変わったときに**静かに挿入を続ける**
/// より、止まるほうが回復可能だからである（`SchemaEngineQuery::judge_write` の
/// `unreachable!` と同じ規律）。
fn row_insertion_error(error: RowInsertionError, at: RowOrdinal, count: usize) -> GridError {
    match error {
        RowInsertionError::IndexOutOfRange { rows, .. } => GridError::SpanOutOfRange {
            span: RowSpan::new(at, count),
            visible: rows,
        },
        RowInsertionError::UnknownSheet { sheet } => GridError::SchemaUnusable { sheet },
        RowInsertionError::DuplicateRow { row } => {
            unreachable!("insert_row_at は識別子を発行するため、重複した識別子は起こりえない: {row}")
        }
    }
}

/// `document-format` の行の削除の誤りを本クレートの誤り型へ写す。
///
/// 2 変種とも本層の事前検査（対象シートの存在と、要求された行の所属）が先に判別する。
/// それでも写しを置く理由は [`write_error`] と同じである（上流の誤りを捨てる経路を作らない）。
fn row_removal_error(error: RowRemovalError) -> GridError {
    match error {
        RowRemovalError::UnknownSheet { sheet } => GridError::SchemaUnusable { sheet },
        RowRemovalError::UnknownRow { row } => GridError::UnknownRow { row },
    }
}

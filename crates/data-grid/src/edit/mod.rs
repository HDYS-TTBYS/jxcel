//! 編集命令の定義と適用: [`EditApply`]（data-grid のタスク 3.1, 3.2, 3.3, 3.4。要件 3.3, 3.4,
//! 3.5, 3.7, 5.5, 5.7, 6.1, 6.2, 6.3, 6.4, 7.3, 7.4, 7.5, 7.7, 8.9, 11.4）。
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
//! いう分岐を書く誘因が生まれ、規則の二重化（遅かれ早かれ食い違う）が始まる。貼り付け
//! （3.4）が値を書く前に引く [`coerce`] も**規則表そのもの**であり、本層が型ごとの場合分けを
//! 書く箇所は無い（モジュール docs「貼り付け」）。
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
//! # 履歴（4.1）との境目
//!
//! 本層は履歴を**積まない**（積むのは `history` 層の `UndoStack`。design.md
//! 「UndoStack（拡張点の所有者）」）が、逆命令の**材料**は適用の時に本層が読んで返す
//! （[`EditApply::apply_with_inverse`]。適用の後には材料が存在しないため。要件 9.1）。
//!
//! **空の `SetCells`（セル 0 個）は何も変えないため、適用の結果を「影響を受けた行なし・
//! 違反の総数 0」として返し、判定も再検証も呼ばず、対も返さない（`None`）。** 対を返さないのは、
//! 取り消しが何も戻さない操作を履歴へ入れないためである（`tests/undo_stack.rs` の
//! `an_empty_command_changes_nothing_and_leaves_no_pair`）。
//!
//! # 群 3 の残りのタスクへの拡張
//!
//! [`EditCommand`] は `SetCells`（3.1）と行の構造を変える 3 つ（3.2）を持ち、3.3 が
//! `SetNested` を足し、3.4 が `PasteRange` を足した。3.1〜3.3 は**判定を呼ぶ形**を
//! 共有し、3.4 はそれを**呼ばない**（モジュール docs「貼り付け」）。行を増減する命令は
//! `row_count` が変わり、`affected` に増減した行が加わる。`SetNested` は打たれた文字が
//! JSON になるが、判定を呼ぶ形は変わらない（design.md「EditApply」の Implementation Notes）。
//!
//! # 入れ子の値の編集（要件 5.5, 5.7）
//!
//! [`EditCommand::SetNested`] は、入れ子のセルを**構造を保った表現**として受け取り、
//! 構造を保ったまま書き戻す。表現の形も、表現とセル値の相互変換も**上流が持っている**:
//! `document-format` の [`to_json_bytes`] /
//! [`from_json_bytes`] がセル値の JSON 表現の唯一の源であり
//! （同クレートの行データの符号化もこの 2 つを通る）、**本層は解析器を 1 つも持たない**。
//! 本層がやるのは `from_json_bytes` に渡すことと、返った [`CellValue`] を判定へ渡すことだけ
//! である。とくに、キー順を保つ
//! [`NestedValue::Object`](document_format::NestedValue::Object) の並び、
//! `Text` / `Decimal` / 添付の
//! 判別（同クレートの規則 1・2 の脱出口）は**すべて上流の 1 箇所**が決めるため、本層が
//! 表現を組み立て直す経路は存在しない。
//!
//! ## 解釈できない入力は処理を止める（要件 5.5 の「構造表現として解釈できない入力」）
//!
//! `from_json_bytes` が失敗した入力（JSON として不正・`i64` の範囲外の整数リテラル・
//! 非有限になる数値リテラルなど）は [`GridError::NestedDecode`] として返す。これは
//! **値の不適合とは別の型**である（`error` 層「宣言の誤りと値の不適合を混ぜない」）:
//! 解釈できない入力はセル値として存在しません、つまり「入力が壊れている」であり、編集経路が
//! 決して拒否しない「値が型に適合しない」とは性格が違う。したがって**縫い目を 1 回も呼ばず、
//! 1 つのセルも書かない**（判定を呼べる値が無い）。
//!
//! ## 解釈できた値が入れ子でない場合の取り決め
//!
//! `from_json_bytes` は**任意の** JSON 値を [`CellValue`] へ戻す（`CellValue::Nested` は
//! オブジェクトと配列の腕にすぎない）。したがって `"42"` や `"null"` も解釈は成功する。
//! 本層の取り決めは「**解釈できた値はそのまま通常の書き込み経路へ渡す**」である — 入れ子で
//! なければならないと本層が判定しない。理由は 2 つある。第 1 に、受理の判定は `schema-engine`
//! の規則表だけが行う（本層が「入れ子の列だから入れ子でなければならない」という分岐を書けば、
//! 列の型を見ることになり規則の二重化になる）。第 2 に、値は破棄されない規律（要件 3.5）が
//! そのまま効く — 入れ子の列へ `Int(42)` を書けば**型の不一致として報告され、値は
//! ドキュメントに残る**（`Int(42)` を選ぶか `Nested` を要求するかを決めるのは
//! `schema-engine` の宣言の側である）。同じ入力が `int` の列では適合する。この取り決めは
//! `tests/edit_nested.rs` の `json_that_is_not_nested_is_decided_by_the_type_system` が固定する。
//!
//! ## 入れ子の内側の違反の位置はどこで観測できるか（要件 4.5、編集経路は 5.5 / 5.7）
//!
//! 要件 4.5 は「入れ子のどの位置が違反しているかを特定できる形で提示する」ことを求める
//! （提示そのものは 8.5 が担う）。編集経路の 5.5 / 5.7 は、その位置が**失われない**ことを
//! 本層に求める。
//! **位置は本層の [`EditOutcome`] には載らない** — design.md「EditApply」の Service Interface
//! が定めるのは `violation_total`（総数）と `coercions` だけであり、本層が再検証の報告から
//! 読むのも総数だけである（モジュール docs「違反の総数の源」）。3.1 も同じ形であり、
//! 入れ子だけを特別扱いしない。
//!
//! 位置を運ぶのは `schema-engine` の [`Violation::path`](schema_engine::Violation) である。
//! 本層は判定へ渡す値を平坦化しない（[`CellValue::Nested`] の木をそのまま渡す）ため、
//! 判定と再検証の違反は**内側の位置を保ったまま**生成される。その位置は本クレートの
//! `types` 層の [`NestedPath`](crate::types::NestedPath) が写し（`From<&ValuePath>` が唯一の入口）、
//! 2.4 の違反の索引がそれを載せる。
//!
//! したがって観測の経路は 2 つある。**表示の側は 2.4 の索引**（`CellViolations::paths`）であり、
//! `tests/violation_index.rs` が入れ子の位置を固定している。**適用の側は、編集した列に
//! 限定した [`SheetReport`]**（`violations()[..].path()`）であり、`tests/edit_nested.rs` の
//! `a_violation_inside_an_array_reports_the_element_position` と
//! `a_violation_inside_an_object_reports_the_field_position` が、編集で生じた違反の位置を
//! [`NestedPath`](crate::types::NestedPath) の段（配列の添字とフィールド名）として突き合わせる。5.2 の `GridSession` は
//! 判定が返した違反から索引を更新するため（design.md の Invariants）、適用の直後から画面まで
//! 位置が落ちる箇所は無い。
//!
//! ## 1 セルの編集であること（要件 11.4）
//!
//! 入れ子の編集は**入れ子の列の 1 セルを書く命令**であり、3.1 の `SetCells` とまったく同じ
//! 呼び出しの形をとる（判定 1 回・編集した列に限定した再検証 1 回・全件検証 0 回）。入れ子の
//! 木が大きいことは費用の形を変えない — 判定も再検証も**1 つのセル値**として木を扱い、
//! 木の内側を本層が歩く経路は 1 つも無い（歩くのは `schema-engine` の検証器である）。
//! `tests/edit_nested.rs` の `a_nested_edit_is_a_single_cell_edit_for_the_call_shape` が
//! 記録の全体を 1 つの表明で固定する。
//!
//! ## `SetNested` の逆命令が要る「変更前の値」
//!
//! design.md「編集命令と逆命令の対応」は `SetNested` の逆命令を「変更前の値」とする。
//! **本層は履歴を積まない**（積むのは `history` 層の `UndoStack`。4.1）が、材料は
//! [`EditApply::apply_with_inverse`] が**適用と同じ本体の中で**読んで返す — 変更前の値は
//! 適用の後には存在しないためである（要件 9.1）。`SetNested` は `SetCells` と同じく
//! **値そのもの**を材料にする（表示文字列や JSON ではない。モジュール docs「履歴（4.1）が
//! 逆命令を組み立てるのに要るもの」）。往復が値と表現の双方で可逆であることは
//! `editing_one_inner_field_leaves_every_other_field_and_the_rest_of_the_document_unchanged`
//! が固定する。
//!
//! # 貼り付け（要件 7.3, 7.4, 7.5, 7.7, 8.9）
//!
//! [`EditCommand::PasteRange`] は表形式テキストを矩形として書き込む。テキストの解釈
//! （区切りの規則・囲み・往復）は [`PasteCodec`]（本層の `paste` モジュール）が唯一の源で
//! あり、本層はその矩形を**セル値へ写して書く**だけである。値の列は錨の列を起点とする
//! 相対位置であり、打ち込まれた文字を値へ写す規則は 1 セルの編集と**同じ 1 つ**を使う。
//!
//! ## 誰が表示の座標を物理の座標へ写すか（要件 8.6, 8.9）
//!
//! 本層は**表示の並びを持たない** — `EditApply` は適用の間 `&mut Document` を握るだけで
//! あり、可視の順序は `view` 層の [`RowOrder`](crate::view::RowOrder) が `&Document` から
//! 導く（3.1 の裁定）。したがって**呼び出し側（5.2 の `GridSession`）が両方を渡す**:
//! `anchor` が起点の**物理の行**（要件 8.6）を、`rows` が**表示されている行の並び**
//! （要件 8.9）を運ぶ。貼り付けは `rows` の中の錨の行の位置から歩き、矩形の行 *i* を
//! `rows[錨の位置 + i]` へ書く。**`rows` に現れない行は 1 つも書かれない** — 8.9 の
//! 「表示されている行にのみ値を反映する」はこの形で満たされる（錨が `rows` に現れなければ
//! 何も書かない。`tests/edit_paste.rs` が両方を固定する）。
//!
//! ## 行ごとの判定を呼ばない（要件 7.7, 11.5）
//!
//! 1 セル・1 行の編集は[`EditSchemaQuery::judge_write`] を呼ぶ（上流の判定は 1 行分の値を
//! 受け取る口しか無い）。貼り付けは**これを 1 回も呼ばない** — 1 万行なら 1 万回になり、
//! 要件 7.7 の「行ごとの確認を求めることなく完了する」と 11.5 の 3 秒に反する。
//! 代わりに貼り付けは次の 2 つの形をとる:
//!
//! 1. **一括の書き込み 1 回**。矩形の全文をセル値へ写し、行ごとに分けずに
//!    [`Document::set_cells`] の 1 回の呼び出しで書く（上流の一括経路は行の索引を 1 度だけ
//!    作り、費用は O(行数 + 変更数) である）。
//! 2. **再検証 1 回**。書き込みの後に [`EditSchemaQuery::revalidate_columns`] を**1 回**呼び、
//!    その報告の総数を `violation_total` に写す（要件 7.5 の違反の件数）。**全件検証は
//!    呼ばない**（要件 11.4）。指定する列は**貼り付けた列**（矩形が覆う列の合併）であり、
//!    行を補充した場合だけ**全列**にする — 行の集合が変われば行を跨ぐ性質（一意性と参照の
//!    実在）が変わりうるためであり、3.2 の行の操作が全列を指定するのと同じ理由である。
//!
//! 値の**強制**は [`coerce`]（規則表）でセルごとに引く。これは判定を呼ぶことではない —
//! 縫い目が数えるのは `schema-engine` の 3 つの入口（判定・再検証・全件検証）であり、
//! 規則表は列の型に応じた写しを返すだけである。強制したうえで書くため、貼り付けのセルの
//! 意味論は 1 セルの編集（3.1）と一致する（変換の記録も同じ形で載る）。
//! 適合しない値は**破棄されず**、書かれたまま再検証の違反として報告される（要件 7.5）。
//! 違反の**位置**は報告が持つ（本層は `EditOutcome` に総数しか載せない。モジュール docs
//! 「違反の総数の源」）。
//!
//! ## 不足する行の補充（要件 7.4）
//!
//! 矩形の行数が `rows` の錨から先に残る行数を超えるとき、**不足する行を文書の末尾へ足して**
//! 貼り付けを完了する（[`EditCommand::InsertRows`] と同じく位置 `== 行数` への追加であり、
//! 既存の行は 1 つも動かない）。補充した行の値の源は宣言ただ 1 つ
//! （[`CompiledSchema::default_row`]）であり、矩形が覆わない列はその既定値のまま残る。
//! 足した行は `rows` の続きとして書かれる（表示されていない行を値の宛先にしない）。
//!
//! ## 空の貼り付けと、書くセルが無い場合
//!
//! 行 0 件のテキスト（空のテキスト）、`rows` が空（表示されている行が 1 つも無い）、
//! 錨の行が `rows` に現れない（表示されていない行を錨にした）場合は、**成功し、何も変えず、
//! 縫い目を 1 回も呼ばない**（3.1 の空の命令と同じ規則）。
//!
//! # 履歴（4.1）が逆命令を組み立てるのに要るもの
//!
//! design.md「編集命令と逆命令の対応」は `InsertRows` / `DuplicateRows` の逆命令を
//! `RemoveRows`（追加された `RowId` を保持する）とし、`RemoveRows` の逆命令を「復元用の
//! 内部命令」（取り除いた `Row` の値・`RowId`・位置を保持する）とする。
//!
//! **逆命令は適用時にしか作れない**（適用の後には変更前の値も、取り除かれた行も存在しない。
//! tasks.md 4.1）。したがって本層は [`EditApply::apply_with_inverse`] を持ち、適用と
//! **同じ本体**で対（[`HistoryPair`]）を組み立てて返す — [`EditApply::apply`] はそれを
//! 捨てるだけの委譲である（3.1〜3.4 の呼び出しの形は 1 つも変わらない）。
//!
//! ## 逆命令は [`EditCommand`] では表せない（[`HistoryCommand`] を持つ理由）
//!
//! `RemoveRows` の逆命令（取り除いた行を**同じ識別子・同じ値・同じ位置**へ戻す）は、
//! [`EditCommand`] のどの変種でも表せない。表すには [`Row`] を保持する
//! 必要があり、`Row` は `Clone` を実装せず（識別子発行の単発行者保証を黙って壊さないため）、
//! 本クレートの外で組み立てる公開の口も無い（[`Document::insert_rows_at`] は `Vec<Row>` を
//! 取るが、`Row` を得る公開経路は [`Document::remove_rows`] と
//! [`RowsCodec::decode`] + `SheetRows::into_rows` だけである）。
//!
//! そこで本層は**履歴専用の命令型** [`HistoryCommand`] を置く。`EditCommand` で表せる
//! 方向は [`HistoryCommand::Edit`] に包んで運び、値の復元は
//! [`HistoryCommand::RestoreValues`]、行の差し戻しは [`HistoryCommand::RestoreRows`] が
//! 運ぶ。**公開の [`EditCommand`] は 1 変種も増えない**（履歴の材料は履歴の側の型である）。
//!
//! ## 適用時に読む材料
//!
//! | 命令 | 逆命令 | 材料を読む時点 |
//! |---|---|---|
//! | `SetCells` / `SetNested` | [`HistoryCommand::RestoreValues`] | 書き込みの**前**（事前検査で作る行の位置の索引を使い、触れる行の値の並びをそのまま読む） |
//! | `InsertRows` | `Edit(RemoveRows)` | 挿入の**後**（挿入した識別子そのもの。`affected` と同一） |
//! | `RemoveRows` | [`HistoryCommand::RestoreRows`] | 取り除く**前**（[`Document::remove_rows`] は**シート順**の `Vec<Row>` を返すが `Row` は外へ出せないため、位置と値は事前に読み、返った行から識別子を取る） |
//! | `DuplicateRows` | `Edit(RemoveRows)` | 複製の後（複製の識別子そのもの） |
//! | `PasteRange` | [`HistoryCommand::Composite`] | 書き込みの**前**（矩形が覆う行の値）と後（補充した行の識別子） |
//!
//! **セルの編集の材料は値そのものであり、表示文字列ではない。**表示文字列へ写して書き戻す
//! 経路は、添付の列（hex のテキストになる）と入れ子の列（要素数の要約になる）で値の変種を
//! 変えてしまい、元の状態を復元できない（`tests/undo_stack.rs` がこの 2 列で復元を固定する）。
//!
//! ## 復元の経路（本層の中で完結させる）
//!
//! [`HistoryCommand`] の適用（[`EditApply::apply_history`]）は [`EditApply::apply`] と
//! **同じ層の中で**行う。`history` 層は本層を参照してよい（層の鎖は
//! `error / types → view → edit → history`）が、**逆向きの参照は無い** — 履歴の内部命令の
//! 型を `history` に置くと、本層が適用のためにその型を名指すことになり層の鎖が閉じない。
//! したがって材料の型は本層が持ち、履歴の層（`history`）がそれを積む。
//!
//! [`HistoryCommand::RestoreRows`] の適用は、[`RowsCodec`] の復号経路（行データの
//! wire 形式）を通して [`Row`] を組み立て、[`Document::insert_rows_at`] へ渡す。**判定は
//! 呼ばない** — 差し戻す値はドキュメントに既にあった値そのものであり、判定（書き込みの門）を
//! 通せば値が変わりうる（複製が判定を通らないのと同じ理由。モジュール docs
//! 「行の構造を変える命令の再検証」）。再検証は差し戻しの後に**すべての列**を 1 回だけ呼ぶ
//! （行の集合が変わるため。同じ節）。

use std::collections::{HashMap, HashSet};

use document_format::parts::RowsCodec;
use document_format::{
    from_json_bytes, to_json_bytes, CellValue, CellWriteError, Document, EntryName, Row, RowId,
    RowInsertionError, RowRemovalError, Sheet, SheetId,
};
use schema_engine::{
    validate_columns, validate_sheet, validate_write, Coercion, ColumnIndex, CompiledSchema,
    EditVerdict, SheetReport, ValidationOptions, WriteOrigin, WriteVerdict,
};
// 貼り付けは**判定を呼ばず**、強制の規則表だけをセルごとに引く（モジュール docs「貼り付け」）。
// `coerce` は縫い目の 3 つの口（判定・再検証・全件検証）のいずれでもない — 縫い目が数えるのは
// **判定の呼び出しの形**であり、規則表は判定の分岐を持たない写しである。
use schema_engine::coerce::coerce;

use crate::error::GridError;
use crate::types::{CellAddress, RowOrdinal, RowSpan};
use crate::view::display_text;

/// 行データの wire 形式（NDJSON）の予約キー（行識別子を 26 文字 ULID テキストで持つ）。
///
/// 復元用の内部経路（[`EditApply::rows_from_materials`]）が行を組み立てるときに使う。
/// `document-format` の `parts::rows_codec` が同じ名前を私的に持つ（本クレートは復号の
/// 側だけを使うため、名前を写して**書く側**を組み立てる）。
const ROW_ID_KEY: &str = "$id";

pub mod paste;

use self::paste::PasteCodec;

/// 編集命令（design.md「EditApply」の Service Interface。要件 3.3, 5.5, 6.1, 6.2, 6.3, 7.3,
/// 7.4, 8.9）。
///
/// 本タスクが持つのは `SetCells`（3.1）と、行の構造を変える 3 つ（3.2）、入れ子の値を
/// 構造表現で書く `SetNested`（3.3）、表形式テキストを貼り付ける `PasteRange`（3.4）である。
/// `PasteRange` は design.md の Service Interface の `PasteRange { anchor, text }` へ
/// **`rows`（表示されている行の並び）を足した形**をとる — 理由は本層が表示の並びを持たない
/// ことにあり、同変種の docs が唯一の説明である（要件 8.9）。
///
/// `SetCells` の値は**打たれた文字**として運ぶ。数値・真偽・日付として解釈するのは
/// `schema-engine` であり、本層もフロントエンドも値を型として扱わない（design.md 同節の
/// Implementation Notes）。`SetNested` だけは文字列が**セル値の構造表現（JSON）**になり、
/// `PasteRange` は文字列が**表形式テキスト**になる — それでも判定を呼ぶ形は変わらない
/// （モジュール docs「入れ子の値の編集」「貼り付け」）。**行の構造を変える 3 つは値を運ばない**
/// — 挿入する行の値は宣言が供給し、複製する行の値はドキュメントから写す（モジュール docs
/// 「行の構造を変える命令」）。
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
    /// 入れ子のセルへ、**構造を保った表現**（`document-format` の JSON 表現）を書く
    /// （要件 5.5, 5.7）。
    ///
    /// `json` の解釈は上流の `document_format::from_json_bytes` に委ねる
    /// （本層は解析器を持たない）。解釈できない入力は
    /// [`GridError::NestedDecode`] として返り、**縫い目も呼ばず、1 つのセルも書かない**
    /// （モジュール docs「解釈できない入力は処理を止める」）。解釈できた値は**そのまま
    /// 判定へ渡す** — 入れ子であること自体は本層が要求しない（同「解釈できた値が入れ子で
    /// ない場合の取り決め」）。
    ///
    /// 入れ子の値を編集する呼び出し側は、**編集の前の値を同じ表現で読んでおく**こと —
    /// design.md「編集命令と逆命令の対応」の `SetNested` の逆命令が保持する「変更前の JSON」
    /// であり、履歴の材料は適用の後には作れない（同「`SetNested` の逆命令が要る
    /// 「変更前の値」」）。
    SetNested {
        /// 書くセル（行の識別子と列の添字）。`SetCells` と同じく**物理のセル**である。
        cell: CellAddress,
        /// そのセルへ書く値の**構造表現**。
        json: String,
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

    /// 表形式テキストを、錨のセルから始まる矩形として貼り付ける（要件 7.3, 7.4, 7.5, 7.7,
    /// 8.9）。
    ///
    /// `text` の解釈は [`PasteCodec`] が唯一の源であり（列の区切りは TAB、行の区切りは LF と
    /// CRLF、値は `"` で囲み、中の `""` は `"` 1 つ）、**値は打ち込まれた文字として書き、
    /// 型の解釈は `schema-engine` に委ねる**（design.md「EditApply」の Implementation Notes:
    /// 「打たれた文字は `String` として受け取り、型解釈は `schema-engine` に委ねる」）。
    /// 矩形の列は `anchor` の列を起点とする相対位置であり、打ち込まれた文字を値へ写す規則は
    /// 1 セルの編集と**同じ 1 つ**を使う。
    ///
    /// # 2 つの成分が宛先を決める（要件 8.6, 8.9）
    ///
    /// 本層は**表示の並びを持たない**（`EditApply` は適用の間 `&mut Document` を握るだけで
    /// あり、可視の順序は `view` 層の [`RowOrder`](crate::view::RowOrder) が `&Document` から
    /// 導く。3.1 の裁定）。したがって**表示の座標を物理の座標へ写すのは表示を持っている側の
    /// 仕事**であり、呼び出し側（5.2 の `GridSession`）が次を渡す:
    ///
    /// - `anchor` — 貼り付けの起点（列と、**物理の行**）。要件 8.6 の「画面上の位置ではなく
    ///   対象の行そのものへ届く」は、錨が [`RowId`] を運ぶことで満たされる。
    /// - `rows` — **表示されている行の並び**（呼び出し側が `RowOrder` から取り出したもの）。
    ///   貼り付けはこの並びの中の**錨の行の位置**から歩き、矩形の行 *i* を `rows[錨の位置 + i]`
    ///   へ書く。
    ///
    /// **`rows` は絞り込みの結果そのものである**（要件 8.9）。隠れている行は並びに現れない
    /// ため、**1 つのセルも書かれない** — 文書順に歩く実装へ静かに落ちる余地は無い
    /// （`tests/edit_paste.rs` の `a_filtered_paste_touches_only_the_displayed_rows` が、
    /// 隠れた行の値が変わらないことと、可視の行が矩形の**対応する行**を受けることを固定する）。
    /// 錨の行が `rows` に現れない場合（表示されていない行を錨にした場合）は**何も書かない**。
    /// これも「表示されていない行へ値が届く」経路を残さないためである。
    ///
    /// **`rows` が空の場合は何も書かない**（表示されている行が 1 つも無い。0 件の命令と
    /// 同じ扱いであり、絞り込みで全行が隠れているときに値を書く宛先は無い）。
    ///
    /// # 行の補充（要件 7.4）
    ///
    /// 矩形の行数が `rows` に残る行数を超えるとき、**不足する行を文書の末尾へ足してから**
    /// 貼り付けを完了する（[`EditCommand::InsertRows`] と同じく文書の位置 `== 行数` への追加で
    /// あり、既存の行は 1 つも動かない）。補充した行の値の源は宣言ただ 1 つ
    /// （[`CompiledSchema::default_row`]）であり、矩形が覆わない列はその既定値のまま残る。
    /// **足りない行が 1 つでもあれば、貼り付けは 1 セルも書かずに止まる** — 補充してから
    /// 途中で誤りを見つける経路を作らない（モジュール docs「ドキュメントへの書き込みは 1 回で
    /// あり、部分適用が無い」）。
    ///
    /// 補充する行の数は**矩形の行数から `rows` の残りを引いた数**である。絞り込みが効いて
    /// いるときに「隠れた行を補充で作る」ことはない — 補充は**表示されていない行を値の
    /// 宛先にしない**という 8.9 の要請と両立する（足した行は文書の末尾にでき、`rows` の
    /// 続きとして書かれる）。
    ///
    /// # 矩形の行が短い場合
    ///
    /// 矩形の行が覆わない列は**そのまま残す**（値なしで埋めない）。理由は 2 つある。第 1 に、
    /// 他のアプリケーションのコピーは行ごとに値の個数が同じとは限らず、短い行の残りを消すと
    /// **貼り付けていないセルの値が失われる**。第 2 に、値なしで埋めると、貼り付けの前後で
    /// 変わったセルの集合が「矩形の内側」より広くなり、取り消し（4.1）が保持すべき範囲が
    /// 矩形から導けなくなる。
    ///
    /// # 判定（要件 7.3, 7.5, 7.7）
    ///
    /// 貼り付けは**行ごとの判定を 1 回も呼ばない**（[`EditSchemaQuery::judge_write`] は
    /// 1 セル・1 行の編集の経路である）。矩形の全文をセル値へ写してから**1 回の一括書き込み**を
    /// 行い、その後に**貼り付けた列に限定した再検証を 1 回**呼ぶ（要件 11.4。全件検証は
    /// 呼ばない）。違反の総数はその報告から写す（モジュール docs「貼り付け」）。
    PasteRange {
        /// 貼り付けの起点（**物理の行**と列。要件 8.6）。
        anchor: CellAddress,
        /// **表示されている行の並び**（呼び出し側が `RowOrder` から取り出したもの）。
        ///
        /// 貼り付けは錨の行がこの並びに現れる位置から歩く。空なら何も書かない。
        rows: Vec<RowId>,
        /// 貼り付ける表形式テキスト（行の区切りと列の区切りを持つ）。
        text: String,
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
    /// 取り除かれた行をシート順に、`DuplicateRows` は複製をシート順に）。`PasteRange` では
    /// **矩形の行を書いた順**であり、補充した行がその末尾に加わる（行は増える）。design.md の
    /// Postconditions は `apply` がこれを必ず含むことを求める。
    ///
    /// design.md「編集命令と逆命令の対応」が `InsertRows` / `DuplicateRows` の逆命令
    /// （`RemoveRows`）へ渡す「追加された `RowId`」はこの欄である。`PasteRange` の逆命令が
    /// 要る「補充された行の `RowId`」も同じ欄である（モジュール docs「`PasteRange` の逆命令
    /// （`SetCells` + `RemoveRows`）が要るもの」）。
    pub affected: Vec<RowId>,
    /// 型強制によって値が変換されたセル（要件 3.4）。変換が起きなければ空である。
    ///
    /// 行の構造を変える命令では**つねに空**である — この経路へ届く値は打たれた文字ではなく、
    /// 判定を通さないため変換も起きない（モジュール docs「行の構造を変える命令の再検証」）。
    /// `PasteRange` では**矩形の順**（行ごと、行の中は列の順）に載る — 値は判定を経ずに規則表
    /// （[`coerce`]）で変換されるため、1 セルの編集と同じ形で変換が記録される。
    pub coercions: Vec<CoercionNotice>,
    /// 違反の総数（本タスクでは**再検証した列に閉じた総数**。モジュール docs
    /// 「違反の総数の源」）。行の構造を変える命令は**すべての列**を再検証するため、適用後の
    /// シートの違反の総数と一致する（同「行の構造を変える命令の再検証」）。`PasteRange` は
    /// 貼り付けた列だけを再検証するため（行を補充したときを除く）、**貼り付けた列に閉じた**
    /// 総数である（要件 7.5 の違反の件数。モジュール docs「貼り付け」）。
    pub violation_total: usize,
    /// 適用の**後**のシートの行数。`SetCells` と `PasteRange` は行を増減しない限り前後で
    /// 変わらない（`PasteRange` は矩形が既存の行数を超えるときに増える。要件 7.4。行を増減する
    /// 命令（3.2）がこの欄に変化を載せる）。
    pub row_count: usize,
}

/// 取り消し履歴が運ぶ命令（履歴の**復元用の内部命令**。要件 6.6, 9.1, 9.2）。
///
/// [`EditCommand`] は「利用者が起こした編集」であり、そのままでは**逆方向**を表せない。
/// とくに `RemoveRows` の逆命令（取り除いた行を同じ識別子・同じ値・同じ位置へ戻す）は、
/// [`Row`] を保持する必要があるが、`Row` は `Clone` を実装せず
/// （識別子発行の単発行者保証を黙って壊さないため）、本クレートの外で組み立てる公開の口も
/// 無い（[`Document::insert_rows_at`] は `Vec<Row>` を取るが、`Row` を得る公開経路は
/// [`Document::remove_rows`] と [`RowsCodec::decode`] + `SheetRows::into_rows` だけである）。
///
/// したがって履歴は**この型**を積む。本層が持つのは、適用の経路
/// （[`EditApply::apply_history`]）がこの型を名指すためである（層の鎖
/// `error / types → view → edit → history` の向きを閉じたままにする）。
/// **公開の [`EditCommand`] は 1 変種も増えない。**
///
/// 等値と複製を持つのは、履歴の形を検査（`tests/undo_stack.rs`）が突き合わせられるように
/// するためである。[`CellValue`] が `Eq` を持たない（浮動小数を持つ）ため `Eq` は導出しない。
#[derive(Debug, Clone, PartialEq)]
pub enum HistoryCommand {
    /// [`EditCommand`] で表せる方向（逆方向も `EditCommand` で表せるもの）。
    ///
    /// `SetCells` / `SetNested` の**やり直し**、`InsertRows` / `RemoveRows` / `DuplicateRows` の
    /// **逆命令**（いずれも `RemoveRows`）、および `PasteRange` のやり直しがこれに載る。
    Edit(EditCommand),
    /// セルの編集を**適用前の値そのもの**へ戻す（`SetCells` / `SetNested` の逆命令、および
    /// 貼り付けが覆ったセルの復元）。
    ///
    /// 保持するのは**行の値の並びそのもの**であり、表示文字列でも JSON でもない。
    /// 表示文字列へ写して書き戻す経路は、添付の列（hex のテキストになる）と入れ子の列
    /// （要素数の要約になる）で値の変種を変えてしまい、元の状態を復元できない
    /// （`tests/undo_stack.rs` の `a_cell_edit_is_undone_to_the_values_read_at_apply_time`）。
    RestoreValues {
        /// 復元する行が属するシート。
        sheet: SheetId,
        /// 復元する行（識別子・位置・適用前の値の並び）。**位置は適用前の文書の位置**である。
        rows: Vec<RestoredRow>,
    },
    /// 取り除いた行を**同じ識別子・同じ値・同じ位置**へ差し戻す（`RemoveRows` の逆命令、
    /// および行を補充した操作のやり直し）。要件 6.6 / 9.2 の本体である。
    ///
    /// 適用は [`RowsCodec`] の復号経路で [`Row`] を組み立て、
    /// [`Document::insert_rows_at`] へ**位置の昇順に連続する区間ごとに 1 回**渡す
    /// （質量削除の差し戻しが 1 回の呼び出しで済むようにする。連続する区間へ分けるのは、
    /// 挿入位置が挿入前の行順に対する添字であるためである）。
    RestoreRows {
        /// 復元する行が属するシート。
        sheet: SheetId,
        /// 差し戻す行（識別子・位置・値の並び）。
        rows: Vec<RestoredRow>,
    },
    /// 複数の部分から成る**1 つの操作**（貼り付け。要件 7.6）。
    ///
    /// 部分は**この並びの順**に適用される。貼り付けの逆命令は
    /// 「覆ったセルの値を戻す → 補充した行を取り除く」であり、この順でなければならない
    /// （先に補充した行を取り除くと、その行に書かれた値の復元先が消える）。
    /// 履歴には**1 つの対として**積まれ、取り消しは 1 回で元の状態へ戻る。
    Composite(Vec<HistoryCommand>),
}

/// 復元の材料としての 1 行（識別子・位置・値の並び）。
///
/// [`Row`] は `Clone` を持たず外へ出す口も無いため、履歴が保持する形へ
/// **写し取る**。位置を持つのは、行の削除の逆命令が**同じ位置**へ戻さなければならないため
/// である（適用後には行が消えており、後からは導けない）。
#[derive(Debug, Clone, PartialEq)]
pub struct RestoredRow {
    /// 行の識別子（**同じ識別子へ戻す**。要件 6.6）。
    pub id: RowId,
    /// 適用前の文書の位置（0 起点。行の挿入はこの位置に対して行う）。
    pub position: usize,
    /// 行の値の並び（列の添字で並ぶ。値なしの列も含めて**そのまま**）。
    pub values: Vec<CellValue>,
}

/// 1 回の適用が生んだ**命令と逆命令の対**（[`EditApply::apply_with_inverse`] の戻り値）。
///
/// `inverse` を適用すると適用前の状態へ戻り、`redo` を適用すると適用後の状態へ戻る。
/// どちらも**適用の時点**で組み立てられる（適用の後には材料が存在しない。tasks.md 4.1）。
///
/// 4.1 の履歴（`history` 層の `UndoStack`）はこの対をそのまま積む。**本層は履歴を積まない**
/// （積むのは操作口の仕事である）。
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryPair {
    /// 適用前の状態へ戻す命令。
    pub inverse: HistoryCommand,
    /// 適用後の状態へ進める命令。
    pub redo: HistoryCommand,
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
    /// [`GridError::ColumnOutOfRange`] / [`GridError::SpanOutOfRange`] /
    /// [`GridError::NestedDecode`]）。値が型に適合しない
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
        // 本体は 1 つである（[`EditApply::apply_with_inverse`]）。本経路は履歴へ積まないため、
        // 適用時に組んだ対を捨てるだけである — 対を組む作業は「材料を読む」ことであり、
        // 読む場所は適用の本体の中（書き込みの前）にしか無い。
        self.apply_with_inverse(doc, command)
            .map(|(outcome, _)| outcome)
    }

    /// 編集命令を適用し、**適用時に組んだ命令と逆命令の対**も返す（tasks.md 4.1。要件 6.6,
    /// 9.1, 9.2）。
    ///
    /// # 逆命令は適用の時にしか作れない
    ///
    /// 適用の後には、変更前の値も、取り除かれた行も、その位置も存在しない（要件 9.1）。
    /// したがって材料は適用の**本体の中で**（書き込みの前に）読み、ここで対へまとめる。
    /// 材料の型と、命令ごとに何をいつ読むかはモジュール docs
    /// 「履歴（4.1）が逆命令を組み立てるのに要るもの」にある。
    ///
    /// **状態を変えない適用（空の命令）は `None` を返す** — 何も戻さない操作を履歴に積むと、
    /// 取り消しが何もしない操作になる（モジュール docs「空の命令」）。
    pub fn apply_with_inverse(
        &mut self,
        doc: &mut Document,
        command: EditCommand,
    ) -> Result<(EditOutcome, Option<HistoryPair>), GridError> {
        match command {
            EditCommand::SetCells { cells } => self.set_cells_with_inverse(doc, cells),
            EditCommand::SetNested { cell, json } => self.set_nested_with_inverse(doc, cell, json),
            EditCommand::InsertRows { at, count } => self.insert_rows_with_inverse(doc, at, count),
            EditCommand::RemoveRows { rows } => self.remove_rows_with_inverse(doc, rows),
            EditCommand::DuplicateRows { rows } => self.duplicate_rows_with_inverse(doc, rows),
            EditCommand::PasteRange {
                anchor,
                rows,
                text,
            } => self.paste_range_with_inverse(doc, anchor, rows, text),
        }
    }

    /// 履歴の命令（**[`HistoryCommand`]**。逆命令とやり直しの命令）を適用する（要件 6.6, 9.2）。
    ///
    /// `EditCommand` で表せる方向は [`EditApply::apply`] へそのまま委ねる（判定を呼ぶ形も
    /// 再検証の形も編集経路のものと**同一**である）。復元用の内部命令
    /// （[`HistoryCommand::RestoreValues`] / [`HistoryCommand::RestoreRows`]）と合成
    /// （[`HistoryCommand::Composite`]）は本層のこの経路が担う。
    ///
    /// # 復元の材料が名乗るシート
    ///
    /// [`HistoryCommand::Edit`] はシートを運ばないため**この適用の対象シート**
    /// （`self.sheet`）へ書く。復元の 2 つは材料が**シートを名乗る** — 履歴がドキュメント単位
    /// であるため（要件 9.5）、取り消しは別のシートの操作を指しうる。適用はその名乗った
    /// シートへ行い、**計画の列数と食い違えば [`GridError::SchemaUnusable`] で止まる**
    /// （列の添字が食い違ったまま書くより、止まるほうが回復可能である。編集経路と同じ規律）。
    ///
    /// # 判定を呼ばない
    ///
    /// 復元の材料はドキュメントに**既にあった値**そのものであり、判定（打たれた文字を型へ
    /// 変換する門）を通せば値が変わりうる。判定を呼ばないのは複製・挿入と同じ理由である
    /// （モジュール docs「行の構造を変える命令の再検証」）。再検証は材料が触れる行の集合に
    /// ついて**全列を 1 回**だけ呼ぶ。
    pub fn apply_history(
        &mut self,
        doc: &mut Document,
        command: &HistoryCommand,
    ) -> Result<EditOutcome, GridError> {
        match command {
            HistoryCommand::Edit(edit) => self.apply(doc, edit.clone()),
            HistoryCommand::RestoreValues { sheet, rows } => self.restore_values(doc, *sheet, rows),
            HistoryCommand::RestoreRows { sheet, rows } => self.restore_rows(doc, *sheet, rows),
            HistoryCommand::Composite(parts) => self.apply_parts(doc, parts),
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

    /// `InsertRows` の適用（[`EditApply::apply_with_inverse`] の本体。要件 6.1）。
    ///
    /// 空の命令（`count == 0`）は**位置を見ない** — 何も挿入しないため、位置の妥当性も
    /// 問わない（モジュール docs「空の命令」）。
    ///
    /// 逆命令は**挿入した行の削除**（`RemoveRows`）である — 追加された識別子は挿入が発行する
    /// ものであり、`affected` と同一である。やり直しは**発行済みの行の差し戻し**であって
    /// 「もう一度挿入する」ではない（挿入し直すと識別子が変わり、逆命令が指す行が消える）。
    /// 差し戻す位置と値も適用の時点で揃う。
    fn insert_rows_with_inverse(
        &mut self,
        doc: &mut Document,
        at: RowOrdinal,
        count: usize,
    ) -> Result<(EditOutcome, Option<HistoryPair>), GridError> {
        self.usable_columns(doc)?;
        if count == 0 {
            return Ok((self.unchanged(doc)?, None));
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
        let mut restored: Vec<RestoredRow> = Vec::with_capacity(count);
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
            // やり直しの材料は「いま書いた値」であり、`defaults` がその唯一の源である。
            restored.push(RestoredRow {
                id: row,
                position: index,
                values: defaults.clone(),
            });
            inserted.push(row);
        }

        let outcome = self.changed_rows(doc, inserted.clone())?;
        Ok((
            outcome,
            Some(HistoryPair {
                inverse: HistoryCommand::Edit(EditCommand::RemoveRows { rows: inserted }),
                redo: HistoryCommand::RestoreRows {
                    sheet: self.sheet,
                    rows: restored,
                },
            }),
        ))
    }

    /// `RemoveRows` の適用（[`EditApply::apply_with_inverse`] の本体。要件 6.2）。
    ///
    /// 逆命令は**取り除いた行の差し戻し**（[`HistoryCommand::RestoreRows`]）であり、
    /// **同じ識別子・同じ値・同じ位置**を保持する（要件 6.6 / 9.2 の本体）。材料は
    /// 取り除く**前**にしか読めない: [`Document::remove_rows`] は取り除いた行を返すが、
    /// [`Row`] は `Clone` を持たず外へも出せないため、**位置と値は事前に**読み、
    /// 返った行からは**識別子だけ**を取る（返る順はシート順である。モジュール docs
    /// 「削除は 1 回の操作である」）。
    ///
    /// やり直しは**同じ識別子をもう一度取り除く** — 差し戻しで識別子が元へ戻っているため、
    /// そのまま使える。
    fn remove_rows_with_inverse(
        &mut self,
        doc: &mut Document,
        rows: Vec<RowId>,
    ) -> Result<(EditOutcome, Option<HistoryPair>), GridError> {
        self.usable_columns(doc)?;
        if rows.is_empty() {
            return Ok((self.unchanged(doc)?, None));
        }
        // 事前検査（読み）: 取り除く行の**位置と値**を、要求の並びに依らず**シート順**で読む
        // （上流が返す行の順と対になる材料を、同じ規律で組んでおく）。
        let materials: Vec<RestoredRow> = {
            let sheet = self.target_sheet(doc)?;
            let positions: HashMap<RowId, usize> = sheet
                .rows()
                .iter()
                .enumerate()
                .map(|(position, row)| (row.id(), position))
                .collect();
            let mut wanted: Vec<usize> = Vec::with_capacity(rows.len());
            let mut seen: HashSet<RowId> = HashSet::with_capacity(rows.len());
            for row in &rows {
                let Some(position) = positions.get(row).copied() else {
                    return Err(GridError::UnknownRow { row: *row });
                };
                if seen.insert(*row) {
                    wanted.push(position);
                }
            }
            wanted.sort_unstable();
            wanted
                .into_iter()
                .map(|position| {
                    let row = &sheet.rows()[position];
                    RestoredRow {
                        id: row.id(),
                        position,
                        values: row.values().to_vec(),
                    }
                })
                .collect()
        };
        // **1 回の呼び出し**で取り除く（上流が 1 パスで事前検査し、1 つでも不正なら 1 行も
        // 取り除かない。モジュール docs「削除は 1 回の操作である」）。返る行は**シート順**で
        // あり、`affected` はその識別子である（要求の並びに依らない）。
        let removed = doc
            .remove_rows(self.sheet, &rows)
            .map_err(row_removal_error)?;
        let affected: Vec<RowId> = removed.iter().map(|row| row.id()).collect();

        let outcome = self.changed_rows(doc, affected.clone())?;
        Ok((
            outcome,
            Some(HistoryPair {
                inverse: HistoryCommand::RestoreRows {
                    sheet: self.sheet,
                    rows: materials,
                },
                redo: HistoryCommand::Edit(EditCommand::RemoveRows { rows: affected }),
            }),
        ))
    }

    /// `DuplicateRows` の適用（[`EditApply::apply_with_inverse`] の本体。要件 6.3, 6.4）。
    ///
    /// 元の行の値を**そのまま写して**末尾へ足す。判定を通さないため、一意制約に重複が生じても
    /// 中止しない — 行は増え、重複は再検証の報告に現れる（要件 6.4）。
    ///
    /// 逆命令は**複製の削除**である。やり直しは、複製の値（元の行と同じ値）を発行済みの
    /// 識別子のまま差し戻す（複製し直すと識別子が変わるため使えない）。
    fn duplicate_rows_with_inverse(
        &mut self,
        doc: &mut Document,
        rows: Vec<RowId>,
    ) -> Result<(EditOutcome, Option<HistoryPair>), GridError> {
        self.usable_columns(doc)?;
        if rows.is_empty() {
            return Ok((self.unchanged(doc)?, None));
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
        let mut restored: Vec<RestoredRow> = Vec::with_capacity(sources.len());
        // 末尾へ足す（`at == 行数`）。元の行は 1 つも動かず、`index` は足すたびに伸びる。
        let mut index = self.target_sheet(doc)?.rows().len();
        for values in sources {
            let copy = doc
                .insert_row_at(self.sheet, index)
                .map_err(|error| row_insertion_error(error, RowOrdinal::new(index), 1))?;
            doc.set_row_values(self.sheet, copy, values.clone())
                .map_err(|error| GridError::UnknownRow { row: error.row })?;
            restored.push(RestoredRow {
                id: copy,
                position: index,
                values,
            });
            copies.push(copy);
            index += 1;
        }

        let outcome = self.changed_rows(doc, copies.clone())?;
        Ok((
            outcome,
            Some(HistoryPair {
                inverse: HistoryCommand::Edit(EditCommand::RemoveRows { rows: copies }),
                redo: HistoryCommand::RestoreRows {
                    sheet: self.sheet,
                    rows: restored,
                },
            }),
        ))
    }

    /// `PasteRange` の適用（[`EditApply::apply_with_inverse`] の本体。要件 7.3, 7.4, 7.5, 7.7,
    /// 8.9）。
    ///
    /// 段の順は 3.1 の経路と同じ「事前検査 → 書き込み → 再検証」であり、**判定（1 行分の
    /// 書き込み判定）を挟まない**。理由はモジュール docs「貼り付け」にある — 上流の判定は
    /// 1 行分の値を受け取る口であり、1 万行の貼り付けで行数だけ呼ぶと要件 7.7 / 11.5 の
    /// 費用の形に反する。代わりに**一括の書き込み**と**再検証 1 回**で完了する（違反の件数は
    /// その報告が持つ。要件 7.5）。再検証の列は貼り付けた列であり、行を補充したときだけ全列に
    /// 広がる（[`EditApply::pasted_columns`]）。
    ///
    /// 値の強制は [`coerce`]（規則表）でセルごとに引く。これは**判定を呼ぶことではない** —
    /// 縫い目が数えるのは `schema-engine` の 3 つの入口（判定・再検証・全件検証）であり、
    /// 規則表は列の型に応じた写しを返すだけである。強制したうえで書くため、貼り付けの
    /// セルの意味論は 1 セルの編集（3.1）と一致する。
    ///
    /// # 1 回の貼り付けは 1 つの操作である（要件 7.6）
    ///
    /// 逆命令は 2 つの部分から成りうる — **覆ったセルの値を戻す**（
    /// [`HistoryCommand::RestoreValues`]。材料は覆った行の**変更前の値**であり、書き込みの
    /// 前に読む）と、**補充した行を取り除く**（`RemoveRows`。識別子は補充が発行する）。
    /// 履歴には**1 つの対**として積まれる（[`HistoryCommand::Composite`]）ため、取り消しは
    /// 1 回で適用前の状態へ戻る。部分の順は「値を戻す → 行を取り除く」である — 逆にすると、
    /// 取り除いた行に書かれていた値の復元先が消える。
    ///
    /// 補充が起きなかった貼り付けの逆命令は**値の復元だけ**である（部分 1 つの合成にしない。
    /// 履歴の形が操作の形を写す）。
    ///
    /// やり直しは、補充した行を**同じ識別子のまま差し戻してから**、貼り付けを**もう一度**
    /// 適用する。差し戻す行を貼り付けの宛先へ明示的に加えるのは、そのためである — 元の
    /// 表示の並びだけを渡すと、やり直しの貼り付けは残りの行が足りないと見て**新しい行を
    /// もう一度補充し**、行数が増えてしまう（識別子も変わり、積んだ対が指す行が消える）。
    fn paste_range_with_inverse(
        &mut self,
        doc: &mut Document,
        anchor: CellAddress,
        rows: Vec<RowId>,
        text: String,
    ) -> Result<(EditOutcome, Option<HistoryPair>), GridError> {
        let columns = self.usable_columns(doc)?;
        let rectangle = PasteCodec::parse(&text);
        // 書くセルが 1 つも無い場合（行 0 件のテキスト、または表示されている行が 1 つも無い）は、
        // 空の命令と同じく**成功し、何も変えず、縫い目を 1 回も呼ばない**（モジュール docs
        // 「空の貼り付けと、書くセルが無い場合」）。
        let column = anchor.column();
        if rectangle.is_empty() || rows.is_empty() {
            return Ok((self.unchanged(doc)?, None));
        }
        // 錨の列は範囲内でなければならない（矩形の列はここを起点とする相対位置である）。
        if column.index() >= columns {
            return Err(GridError::ColumnOutOfRange {
                column,
                count: columns,
            });
        }

        // 事前検査（読み）。**1 つのセルも書く前に**、錨の行・`rows` の並び・矩形の列の範囲・
        // 補充する行数を決める。1 つでも不正なら止まる（モジュール docs「誤りの経路と部分適用の
        // 不在」）。
        let start = {
            let sheet = self.target_sheet(doc)?;
            let positions: HashMap<RowId, usize> = sheet
                .rows()
                .iter()
                .enumerate()
                .map(|(position, row)| (row.id(), position))
                .collect();
            // 錨の行は**文書に属していなければならない**（物理の行である。要件 8.6）。
            if !positions.contains_key(&anchor.row()) {
                return Err(GridError::UnknownRow { row: anchor.row() });
            }
            // `rows` は表示されている行の並びである（要件 8.9）。すべての行が対象シートに
            // 属することを先に確かめる — 属さない行があれば 1 つのセルも書かない。
            let mut seen: HashSet<RowId> = HashSet::with_capacity(rows.len());
            let mut displayed: Vec<RowId> = Vec::with_capacity(rows.len());
            for row in &rows {
                if !positions.contains_key(row) {
                    return Err(GridError::UnknownRow { row: *row });
                }
                // 同じ行が 2 度現れる並びは、後ろを残す 1 つの指定へ畳む（`SetCells` の
                // 「同じセルを 2 度書く命令」と同じ規則）。表示の並びは重複を含まないが、
                // 畳んでも意味が変わらないことを型の上で保証しておく。
                if seen.insert(*row) {
                    displayed.push(*row);
                }
            }
            // 錨の行が表示の並びに現れる位置から歩く（要件 8.6: 錨は物理の行であり、歩く
            // 順序は表示の並びである）。錨が表示されていなければ**何も書かない**（表示されて
            // いない行へ値を届ける経路を残さない）。
            displayed
                .iter()
                .position(|row| *row == anchor.row())
                .map(|position| (displayed, position))
        };
        let Some((displayed, start)) = start else {
            return Ok((self.unchanged(doc)?, None));
        };

        // 矩形が覆う行と列の範囲を確かめる。列は錨の列から右へ、行は表示の並びの錨の位置から
        // 下へ進む。**はみ出す列が 1 つでもあれば 1 つのセルも書かない**（最初に外れる列は
        // つねに列数の位置である。[`check_paste_columns`]）。
        let mut widest = 0usize;
        for row in &rectangle {
            check_paste_columns(column, row.len(), columns)?;
            widest = widest.max(row.len());
        }
        // 補充する行数は「矩形の行数から、表示の並びの錨から先に残る行数を引いたもの」である
        // （要件 7.4）。`rows` の残りで足りるなら 0 である。
        let available = displayed.len() - start;
        let appended = rectangle.len().saturating_sub(available);

        // 矩形が**書き込む行**（表示の並びの錨から先の、矩形の行数ぶん）を先に決める。
        // これは `destination` の先頭部分そのものであり、**材料の読み口と書き込みの宛先が
        // 同一の 1 つの並びから出る**ようにするための取り出しである（下の `destination` は
        // これに補充した行を継ぎ足すだけである）。
        //
        // 文書の位置で引いてはならない — 錨の位置は**表示の並び**に対する位置であり、
        // 並べ替えや絞り込みの下では文書の位置と食い違う（表示の並びを決めるのは `view` 層の
        // `RowOrder` であり、本層は表示の座標を受け取る。要件 8.6, 8.9）。食い違えば材料が
        // **別の行**を指し、取り消しが実際に書いた行ではなく他の行を戻してしまう。
        let covered: Vec<RowId> = displayed[start..]
            .iter()
            .copied()
            .take(rectangle.len())
            .collect();

        // **変更前の値**（逆命令の最初の部分）を書き込みの前に読む。覆った行の値の並びの
        // 全体を写す（矩形の外の列の値も写すが、それは元の値と同じであり、復元の結果を
        // 変えない — 材料の形が操作の形に依らず一様になる）。
        let restore_values: Vec<RestoredRow> = {
            let sheet = self.target_sheet(doc)?;
            // 文書の位置の索引はこの 1 回だけ作る（位置は材料が要る — 行の集合を変える命令の
            // 材料と同じく、位置は適用の後には導けない）。
            let positions: HashMap<RowId, usize> = sheet
                .rows()
                .iter()
                .enumerate()
                .map(|(position, row)| (row.id(), position))
                .collect();
            covered
                .iter()
                .map(|row| {
                    let position = positions[row];
                    RestoredRow {
                        id: *row,
                        position,
                        values: sheet.rows()[position].values().to_vec(),
                    }
                })
                .collect()
        };

        // 不足する行を文書の**末尾**へ足す（`InsertRows` と同じ位置であり、既存の行は 1 つも
        // 動かない。モジュール docs「複製は末尾へ足し」と同じ規律）。既定値の源は宣言ただ 1 つ。
        let defaults = self.schema.default_row();
        let mut targets: Vec<RowId> = Vec::with_capacity(rectangle.len());
        if appended > 0 {
            let mut index = self.target_sheet(doc)?.rows().len();
            for _ in 0..appended {
                let row = doc
                    .insert_row_at(self.sheet, index)
                    .map_err(|error| row_insertion_error(error, RowOrdinal::new(index), 1))?;
                doc.set_row_values(self.sheet, row, defaults.clone())
                    .map_err(|error| GridError::UnknownRow { row: error.row })?;
                index += 1;
                targets.push(row);
            }
        }
        // 宛先は「覆った行」＋「補充した行」である（矩形の行 *i* はこの並びの *i* 番目へ
        // 書かれる）。補充した行を末尾へ足したため、順序は「表示の並びの錨から先」＋
        // 「足した行」である。矩形が表示の並びより短ければ、残りの行は**書かれない**
        // （上の `take` がその上限そのものであり、覆わない行の値は変わらない）。
        let mut destination = covered;
        destination.extend(targets.iter().copied());

        // 全セルの値と宛先を 1 回の走査で組み立てる（強制もここで一度きりである）。
        let mut writes: Vec<(RowId, usize, CellValue)> = Vec::new();
        let mut coercions: Vec<CoercionNotice> = Vec::new();
        for (row_index, values) in rectangle.iter().enumerate() {
            let row = destination[row_index];
            for (offset, text) in values.iter().enumerate() {
                // 矩形の列は錨の列を起点とする相対位置である。
                let target = ColumnIndex::new(column.index() + offset);
                let written = coerce(&self.schema, target, edited_value(text));
                if let Coercion::Converted { from } = &written.coercion {
                    coercions.push(CoercionNotice {
                        cell: CellAddress::new(row, target),
                        before: display_text(from).into_owned(),
                        after: display_text(&written.value).into_owned(),
                    });
                }
                writes.push((row, target.index(), written.value));
            }
        }
        // `affected` は**書いた行**（補充した行を含む）であり、矩形の行ごとに 1 つである。
        // `destination` は重複しない（表示の並びは畳んであり、補充した行は新しい）ため、
        // そのまま使える。「矩形の行が短い」場合も行そのものは値の在否が変わる（覆わない列が
        // 0 個になる行は無い — 矩形の行は必ず値 1 つ以上を持つ）。
        let affected = destination;

        // 書き込みは 1 回（`Document::set_cells` の一括経路。行の集合・並びは変わらない）。
        doc.set_cells(self.sheet, &writes).map_err(write_error)?;

        // 再検証は**1 回**だけ呼ぶ（要件 11.4。全件検証は呼ばない）。
        let revalidated = self.pasted_columns(column, widest, appended);
        let report = self.query.revalidate_columns(
            doc,
            self.sheet,
            &self.schema,
            &revalidated,
            &ValidationOptions::capped(0),
        );

        let outcome = EditOutcome {
            affected,
            coercions,
            violation_total: report.total_violations(),
            row_count: self.target_sheet(doc)?.rows().len(),
        };
        let pair = self.paste_pair(doc, anchor, displayed, restore_values, &targets, text)?;
        Ok((outcome, pair))
    }

    /// 貼り付けの対（1 回の貼り付け = 1 つの操作。要件 7.6）を組む。
    ///
    /// `displayed` は**適用の前**の表示の並びであり、`appended_rows` は補充した行の識別子である。
    /// 逆命令の部分は最大 2 つ（値の復元と、補充した行の削除）であり、順は
    /// 「値を戻す → 行を取り除く」に固定する。補充が無ければ**値の復元だけ**を返す
    /// （部分 1 つの合成にしない — 履歴の形が操作の形を写す）。
    ///
    /// やり直しは、補充した行を**同じ識別子のまま**差し戻してから、元の貼り付けを
    /// **補充した行を宛先に加えた形**で適用する（加えなければ、やり直しの貼り付けが
    /// 行数の不足を見て新しい行をもう一度補充し、識別子が変わる）。
    fn paste_pair(
        &self,
        doc: &Document,
        anchor: CellAddress,
        displayed: Vec<RowId>,
        restore_values: Vec<RestoredRow>,
        appended_rows: &[RowId],
        text: String,
    ) -> Result<Option<HistoryPair>, GridError> {
        // 逆命令: 覆ったセルの値を戻す（材料は適用の前に読んだ値そのもの）。
        let mut parts: Vec<HistoryCommand> = Vec::with_capacity(2);
        if !restore_values.is_empty() {
            parts.push(HistoryCommand::RestoreValues {
                sheet: self.sheet,
                rows: restore_values,
            });
        }
        if !appended_rows.is_empty() {
            parts.push(HistoryCommand::Edit(EditCommand::RemoveRows {
                rows: appended_rows.to_vec(),
            }));
        }
        let inverse = match parts.len() {
            0 => return Ok(None),
            1 => parts.pop().expect("長さを確かめた"),
            _ => HistoryCommand::Composite(parts),
        };

        // やり直し: 補充した行（**適用の後に読む**。値は貼り付けが書いたもの）を差し戻し、
        // そのうえで貼り付けをもう一度適用する。
        let redo = if appended_rows.is_empty() {
            HistoryCommand::Edit(EditCommand::PasteRange {
                anchor,
                rows: displayed,
                text,
            })
        } else {
            let sheet = self.target_sheet(doc)?;
            let positions: HashMap<RowId, usize> = sheet
                .rows()
                .iter()
                .enumerate()
                .map(|(position, row)| (row.id(), position))
                .collect();
            let mut restored: Vec<RestoredRow> = Vec::with_capacity(appended_rows.len());
            for row in sheet.rows() {
                if appended_rows.contains(&row.id()) {
                    restored.push(RestoredRow {
                        id: row.id(),
                        position: positions[&row.id()],
                        values: row.values().to_vec(),
                    });
                }
            }
            // 宛先に補充した行を加えた並び。貼り付けは錨の位置から矩形の行数ぶんを歩くため、
            // 行数が足り、**もう補充しない**（発行済みの識別子のまま書く）。
            let mut destinations = displayed;
            destinations.extend(appended_rows.iter().copied());
            HistoryCommand::Composite(vec![
                HistoryCommand::RestoreRows {
                    sheet: self.sheet,
                    rows: restored,
                },
                HistoryCommand::Edit(EditCommand::PasteRange {
                    anchor,
                    rows: destinations,
                    text,
                }),
            ])
        };
        Ok(Some(HistoryPair { inverse, redo }))
    }

    /// 貼り付けの後に再検証する列の集合（貼り付けの列、または行を補充した場合は全列）。
    ///
    /// `widest` は矩形が実際に覆う最大の列数である（行ごとに値の個数が違ってよいため、
    /// **覆った列の合併**をそのまま指定する）。
    ///
    /// # 行を補充したときだけ列を絞らない理由
    ///
    /// 補充は**行の集合を変える**。挿入された行はあらゆる列で値なし／既定値になり、行を跨ぐ
    /// 性質（一意性と参照の実在）は行が増えるだけで変わりうる（[`CompiledSchema::unique_columns`]
    /// を列の型ごとの走査からは導けない）。貼り付けた列だけを見れば静かに過少報告になる列が
    /// 生まれるため、この経路は [`EditApply::revalidate_every_column`] と同じ**全列の指定**を
    /// 使う — 行の構造を変える命令（3.2）が全列を指定するのと**同じ理由**である。
    ///
    /// 行を補充しなかった貼り付けでは、**貼り付けた列の外の値は 1 つも変わらない**。したがって
    /// 貼り付けた列の合併だけで足り、その外の列の違反は適用の前後で同じである（絞ることは
    /// 過少報告にならない）。この 2 つの形は `tests/edit_paste.rs` が**それぞれ別の検査**で
    /// 固定する（`a_paste_beyond_the_last_row_appends_the_missing_rows` と
    /// `a_filtered_paste_touches_only_the_displayed_rows`）。
    fn pasted_columns(
        &self,
        start: ColumnIndex,
        widest: usize,
        appended: usize,
    ) -> Vec<ColumnIndex> {
        if appended > 0 {
            return (0..self.schema.column_count())
                .map(ColumnIndex::new)
                .collect();
        }
        (0..widest)
            .map(|offset| ColumnIndex::new(start.index() + offset))
            .collect()
    }

    /// `SetCells` の適用（[`EditApply::apply_with_inverse`] の本体）。
    fn set_cells_with_inverse(
        &mut self,
        doc: &mut Document,
        cells: Vec<(CellAddress, String)>,
    ) -> Result<(EditOutcome, Option<HistoryPair>), GridError> {
        // セッションの前提を先に検査する（命令の中身に依らない。2 つの前提の理由は
        // `usable_columns` の docs「セッションの前提」）。
        let columns = self.usable_columns(doc)?;
        // 空の命令は何も変えない（判定も再検証も呼ばない。モジュール docs「履歴（4.1）との
        // 境目」）。ただしセッションの前提は空の命令でも検査する（前提は命令に依らない）。
        if cells.is_empty() {
            return Ok((self.unchanged(doc)?, None));
        }
        // やり直しの命令は**適用した命令そのもの**である（編集は打たれた文字を運ぶため、
        // 適用の後に値を写し直す必要が無い）。
        let redo = EditCommand::SetCells {
            cells: cells.clone(),
        };

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
        //
        // 逆命令の材料（変更前の値）も**同じ 1 度の走査**で読む — 位置の索引からその行を引き、
        // 値の並びをそのまま写す（表示文字列を経由しない。モジュール docs「履歴（4.1）が逆命令を
        // 組み立てるのに要るもの」）。
        let mut affected: Vec<RowId> = Vec::new();
        let mut rows: Vec<RowEdit> = Vec::new();
        let mut restore: Vec<RestoredRow> = Vec::new();
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
                        let values = sheet.rows()[position].values().to_vec();
                        restore.push(RestoredRow {
                            id: address.row(),
                            position,
                            values: values.clone(),
                        });
                        rows.push(RowEdit::from_text(
                            address.row(),
                            values,
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
        let outcome = self.write_judged_rows(doc, affected, rows)?;
        Ok((outcome, self.cell_pair(restore, redo)))
    }

    /// `SetNested` の適用（[`EditApply::apply`] の本体。要件 5.5, 5.7）。
    ///
    /// 入れ子のセルを**構造を保った表現**として受け取り、構造を保ったまま書き戻す。表現の
    /// 解釈は上流の [`from_json_bytes`] が唯一の源であり、**本層は解析器を持たない**
    /// （モジュール docs「入れ子の値の編集」）。
    ///
    /// 検査の順は `SetCells` と同じ 2 段である — セッションの前提
    /// （[`EditApply::usable_columns`]）は命令の中身に依らないため先に検査し、そのあと
    /// **表現の解釈 → 宛先（列の範囲と行の存在）**の順に検査する。表現の解釈を先に置くのは、
    /// それが**ドキュメントを 1 度も読まない**操作だからである — 壊れた入力はシートの走査を
    /// 起こさずに止まる（順序はどちらでも「部分適用が無く縫い目を呼ばない」を満たすため、
    /// **本層の決定**としてここに書き、`tests/edit_nested.rs` が固定する）。**どの誤りでも
    /// 縫い目を 1 回も呼ばず、1 つのセルも書かない。**
    ///
    /// 解釈できた値が入れ子であるかは本層が判定しない — そのまま 1 セルの書き込みとして
    /// 判定へ渡す（同「解釈できた値が入れ子でない場合の取り決め」）。したがって判定を呼ぶ形は
    /// 1 セルの編集そのものである（要件 11.4）。
    fn set_nested_with_inverse(
        &mut self,
        doc: &mut Document,
        cell: CellAddress,
        json: String,
    ) -> Result<(EditOutcome, Option<HistoryPair>), GridError> {
        let columns = self.usable_columns(doc)?;
        // 解釈できない入力は**値の不適合ではなく入力の破損**である。`CellValue` が 1 つも
        // 得られないため判定を呼ぶ値が無く、処理を止める（`error` 層の 2 分法。
        // モジュール docs「解釈できない入力は処理を止める」）。
        let value = from_json_bytes(json.as_bytes()).map_err(|_| GridError::NestedDecode { cell })?;
        // やり直しの命令は**適用した命令そのもの**である（打たれた表現をそのまま運ぶ）。
        let redo = EditCommand::SetNested {
            cell,
            json: json.clone(),
        };

        // 事前検査（読み）。`SetCells` と同じく**書き込みの前に**宛先を検査するため、1 つでも
        // 不正なら判定も呼ばず、1 つのセルも書かない。
        let column = cell.column();
        if column.index() >= columns {
            return Err(GridError::ColumnOutOfRange {
                column,
                count: columns,
            });
        }
        let (values, position) = {
            let sheet = self.target_sheet(doc)?;
            let Some((position, row)) = sheet
                .rows()
                .iter()
                .enumerate()
                .find(|(_, found)| found.id() == cell.row())
            else {
                return Err(GridError::UnknownRow { row: cell.row() });
            };
            (row.values().to_vec(), position)
        };

        // 逆命令の材料は**書き込みの前に**読む（変更前の値そのもの）。
        let restore = vec![RestoredRow {
            id: cell.row(),
            position,
            values: values.clone(),
        }];
        let outcome = self.write_judged_rows(
            doc,
            vec![cell.row()],
            vec![RowEdit::from_value(cell.row(), values, column, value)],
        )?;
        Ok((outcome, self.cell_pair(restore, redo)))
    }

    /// セルの編集の対（逆命令は**適用前の値の復元**、やり直しは適用した編集そのもの）を組む。
    ///
    /// `SetCells` と `SetNested` は値を書く形が違うだけで、対の形は同一である —
    /// 逆命令は「触れた行の、適用前の値の並びへ戻す」であり、やり直しは適用した命令そのもの。
    /// どちらの材料（逆命令の値と、やり直しの命令）も**適用の前に**揃う（要件 9.1）。
    ///
    /// 触れた行が 0 個の場合は `None` を返す — そのような命令は上の段で
    /// [`EditApply::unchanged`] へ分岐しているため、ここへは来ない（来たなら**対を持たない
    /// 適用**を作らないための門である）。
    fn cell_pair(&self, restore: Vec<RestoredRow>, redo: EditCommand) -> Option<HistoryPair> {
        if restore.is_empty() {
            return None;
        }
        Some(HistoryPair {
            inverse: HistoryCommand::RestoreValues {
                sheet: self.sheet,
                rows: restore,
            },
            redo: HistoryCommand::Edit(redo),
        })
    }

    /// 判定へかける編集の一覧を受け取り、**判定 → 1 回の書き込み → 編集した列に限定した
    /// 再検証**を行って結果を組み立てる（`SetCells` と `SetNested` が共有する後半）。
    ///
    /// 判定の呼び出しは**編集の対象になった行ごとに 1 回**であり、渡すのはその行の 1 行分の
    /// 値である（上流の `validate_write` の契約）。返った値と変換の記録はそのまま写し、
    /// 列の型を見て受理を決める分岐はここに無い。違反が内側の位置を持つ場合もそのまま
    /// 報告へ載る（本層は値を平坦化しない。モジュール docs「入れ子の内側の違反の位置」）。
    fn write_judged_rows(
        &mut self,
        doc: &mut Document,
        affected: Vec<RowId>,
        rows: Vec<RowEdit>,
    ) -> Result<EditOutcome, GridError> {
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
            // セルを書く命令は行を増減しないため、適用の前後で同じ数になる。適用の後の行数を
            // 改めて読む（行数を変える命令（3.2）がこの経路をそのまま使えるようにする）。
            row_count: self.target_sheet(doc)?.rows().len(),
        })
    }

    /// セルの編集の逆命令を適用する（[`HistoryCommand::RestoreValues`]。要件 9.2）。
    ///
    /// 材料は**適用前の値そのもの**である。したがって判定（打たれた文字を型へ変換する門）を
    /// 通さず、**行の値の並びごと**書く（[`EditApply::write_material_rows`]）— 通せば値が
    /// 変わりうる（複製・挿入の経路が判定を呼ばないのと同じ理由）。
    ///
    /// **行の幅も材料の幅へ戻る。**セル単位の書き込み（[`Document::set_cells`]）は行を
    /// **伸ばすことしかできない**（`Row::set_cell` は `resize(column + 1, Null)` であり、
    /// 決して縮めない）ため、編集が行を広げていた場合（短い行の先の列へ書いた場合）に
    /// 幅が材料より大きいまま残る — しかし材料こそが適用前の状態である（要件 9.2）。
    ///
    /// 行の集合・並び・識別子は変えない。再検証は差し戻しの後に
    /// **すべての列**を 1 回だけ呼ぶ（行を跨ぐ違反（一意性）は差し戻しで変わりうる。全列を
    /// 明示するのは「どの列を見たか」を呼び出しの形に残すためである）。
    ///
    /// # 名乗られたシート
    ///
    /// 材料が名乗るシートへ書く（[`EditApply::apply_history`] の docs「復元の材料が名乗る
    /// シート」）。計画の列数と食い違えば [`GridError::SchemaUnusable`] で止まる。
    fn restore_values(
        &mut self,
        doc: &mut Document,
        sheet: SheetId,
        rows: &[RestoredRow],
    ) -> Result<EditOutcome, GridError> {
        self.usable_columns_of(doc, sheet)?;
        if rows.is_empty() {
            return self.unchanged_of(doc, sheet);
        }
        Self::write_material_rows(doc, sheet, rows)?;
        Ok(EditOutcome {
            affected: rows.iter().map(|row| row.id).collect(),
            coercions: Vec::new(),
            violation_total: self.revalidate_every_column_of(doc, sheet),
            row_count: self.sheet_of(doc, sheet)?.rows().len(),
        })
    }

    /// 取り除いた行の逆命令を適用する（[`HistoryCommand::RestoreRows`]。要件 6.6 / 9.2 の本体）。
    ///
    /// 差し戻す行は**同じ識別子・同じ値・同じ位置**へ戻る。材料は
    /// [`EditApply::rows_from_materials`] が **wire 形式（行データの NDJSON）** を組み立て、
    /// 上流の復号経路（[`RowsCodec::decode`]）を通して [`Row`] にする — `Row` は `Clone` を
    /// 持たず本クレートの外で組み立てられないため、**本クレートが唯一の正規の入口**
    /// （復号）を使う。判定は呼ばない（材料はドキュメントに既にあった値である。
    /// [`EditApply::restore_values`] と同じ理由）。
    ///
    /// 挿入は**位置の昇順に連続する区間ごとに 1 回**呼ぶ（質量削除の差し戻しが 1 回の
    /// 呼び出しで済む）。[`Document::insert_rows_at`] の位置は**挿入前の行順**に対する添字で
    /// あり、昇順に差し戻せば元の位置がそのまま復元される（前の区間を差し込んだ分だけ後ろの
    /// 位置がずれるが、升順の挿入ではこのずれが「挿入前の位置」と一致する）。
    ///
    /// # 既に在る行（古い履歴の対）
    ///
    /// 同じ対を 2 度適用すると、差し戻す行が既に文書にある。`insert_rows_at` はこれを
    /// [`RowInsertionError::DuplicateRow`] として返すが、これは**履歴の対が古い**ことを意味する
    /// （適用の対象が変わった）ため [`GridError::UnknownRow`] へ写す — 「戻そうとした行は既に
    /// そこにある」という意味であり、この型の既存の 5 変種で最も近い（**変種は増やさない**）。
    fn restore_rows(
        &mut self,
        doc: &mut Document,
        sheet: SheetId,
        rows: &[RestoredRow],
    ) -> Result<EditOutcome, GridError> {
        self.usable_columns_of(doc, sheet)?;
        if rows.is_empty() {
            return self.unchanged_of(doc, sheet);
        }
        // 復号済みの行を、材料の**位置の昇順**（材料の並びに依らない）で受け取る。
        let mut rest = Self::rows_from_materials(sheet, self.schema.column_count(), rows)?;
        // 連続した位置の区間ごとに 1 回だけ差し込む（質量削除の差し戻しが 1 回で済む）。
        // 位置は昇順に並んでいるため、先頭から区間を切り出しては差し込むことを繰り返す
        // （差し込む位置は**挿入前の行順**に対する添字であり、昇順に差し込めば元の位置が
        // そのまま戻る）。
        // [`Row`] は `Clone` を持たないため、区間は `drain` で所有権ごと渡す。
        while !rest.is_empty() {
            let start = rest[0].0;
            let mut end = 1;
            while end < rest.len() && rest[end].0 == rest[end - 1].0 + 1 {
                end += 1;
            }
            let batch: Vec<(usize, Row)> = rest.drain(..end).collect();
            let inserted: Vec<Row> = batch.into_iter().map(|(_, row)| row).collect();
            doc.insert_rows_at(sheet, start, inserted).map_err(|error| {
                // 差し戻そうとした行が既にある（古い対）か、名乗られたシートが無い。位置の
                // 範囲外は材料が壊れている場合だけであり、先頭の行を載せて返す。
                match error {
                    RowInsertionError::UnknownSheet { sheet } => GridError::SchemaUnusable { sheet },
                    RowInsertionError::DuplicateRow { row } => GridError::UnknownRow { row },
                    RowInsertionError::IndexOutOfRange { .. } => GridError::UnknownRow {
                        row: rows[0].id,
                    },
                }
            })?;
        }
        // **行の幅を材料の幅へ戻す**。wire 形式（復号の正規の入口）は列数ぶんのキーを書くため、
        // 復号された行の幅は**つねに列数**である — 決して材料の幅ではない（材料が短い行でも、
        // 幅 0 の行でも同じ）。行の値の並びが列数に満たないことは正当であり（モジュール docs
        // 「複製は末尾へ足し、値をそのまま写す」）、差し戻した行が材料と違う幅になるのは復元に
        // ならない — 複製のやり直しが埋まった幅のまま残ってしまう（要件 9.2 の往復）。
        //
        // 書き手は [`EditApply::restore_values`] と同じ 1 つを使う（材料の値の並びを置換で
        // 書く）。**幅 0 の行もここで戻る** — 空の並びの置換は「値なし」ではなく「値なしを
        // 1 つも持たない」であり、[`Row::set_values`] が並びを丸ごと差し替えるためである。
        Self::write_material_rows(doc, sheet, rows)?;
        Ok(EditOutcome {
            affected: rows.iter().map(|row| row.id).collect(),
            coercions: Vec::new(),
            violation_total: self.revalidate_every_column_of(doc, sheet),
            row_count: self.sheet_of(doc, sheet)?.rows().len(),
        })
    }

    /// 復元の材料を**行の値の並びごと**書く（`RestoreValues` と `RestoreRows` が共有する
    /// 唯一の書き手。要件 9.2 の往復）。
    ///
    /// [`Document::set_row_values`] は行の値の並びを**置換**する（上流の `Row::set_values` が
    /// `self.values = values` である）。これは復元に要る 3 つの性質を同時に満たす唯一の口で
    /// ある:
    ///
    /// 1. **短い行をそのまま書ける**（材料の並びの長さがそのまま行の幅になる。列数まで埋めない）
    /// 2. **広げられた行を縮められる** — セル単位の書き込み（[`Document::set_cells`]）は
    ///    `Row::set_cell` の `resize(column + 1, Null)` を通るため**伸びるだけで決して縮まない**。
    ///    編集が短い行の先の列へ書いていた場合、セル単位の逆命令では幅が材料より大きいまま
    ///    残る（`tests/undo_stack.rs` の
    ///    `an_undo_of_an_edit_that_widened_a_short_row_restores_its_original_width`）
    /// 3. **幅 0 の行を書ける** — 空の並びの置換は「値なし 1 件」ではなく「1 件も持たない」
    ///    である（`tests/undo_stack.rs` の
    ///    `a_removed_zero_width_row_is_restored_with_zero_width`）。wire 形式（復号の経路）は
    ///    キーを 1 つも持てないため幅 0 の行を運べないが、差し戻しの後にここで書けば戻る
    ///
    /// 行の識別子・集合・並びには触れない（置換であって追加でも移動でもない）。
    fn write_material_rows(
        doc: &mut Document,
        sheet: SheetId,
        rows: &[RestoredRow],
    ) -> Result<(), GridError> {
        for row in rows {
            doc.set_row_values(sheet, row.id, row.values.clone())
                .map_err(|error| GridError::UnknownRow { row: error.row })?;
        }
        Ok(())
    }

    /// 復元の材料から [`Row`] を組み立てる（**復元用の内部経路**）。
    ///
    /// # なぜこの経路が要るか
    ///
    /// [`Row`] は `Clone` を実装せず（識別子発行の単発行者保証を黙って壊さないため）、
    /// 本クレートの外で組み立てる公開の口も無い。本クレートが `Row` を得る正規の入口は
    /// **復号**（[`RowsCodec::decode`] → `SheetRows::into_rows`）である。したがって材料
    /// （識別子・値の並び）を、行データの **wire 形式**（`$id` + 列キーのオブジェクトの
    /// NDJSON）へ組み立ててから復号する。
    ///
    /// **公開の [`EditCommand`] は 1 変種も増えない** — この経路は履歴の復元のためだけの
    /// 内部経路である（design.md「編集命令と逆命令の対応」の「復元用の内部命令」）。
    ///
    /// # 組み立ての規則
    ///
    /// - 列のキーは `c0`, `c1`, … `cN-1`（`N` はこの適用の列数）を使う。キー名は行データの
    ///   復号が受理する形（`$` 始まりでない）であり、**この 1 行のキー列がそのまま値の位置を
    ///   定める**（`RowsCodec::decode` の契約）。シートの列名に依存しないのは、材料が列名を
    ///   持たないためである（値の並びの位置が列の添字である）。
    /// - セルは [`to_json_bytes`]（value 層の単一の源）で写す — 非有限の浮動小数はここで
    ///   拒否され、不正な JSON を 1 バイトも書かない。
    /// - 値の並びは列数まで [`CellValue::Null`] で埋める。行の値の並びは列数に満たないことが
    ///   あり（値を持たない列は上流でも値なしとして扱われる。モジュール docs「複製は末尾へ
    ///   足し、値をそのまま写す」）、wire 形式は**全行が同じキー列**を要求するためである。
    ///   埋めた位置は値なしであり、ドキュメント上も同じ値なしになる。
    /// - 位置の昇順に並べ替える（材料の並びに依らない。同じ行集合の材料は常に同じ結果になる）。
    fn rows_from_materials(
        sheet: SheetId,
        columns: usize,
        rows: &[RestoredRow],
    ) -> Result<Vec<(usize, Row)>, GridError> {
        let entry = EntryName::Rows { sheet };
        let location = entry.to_string();
        let mut sorted: Vec<&RestoredRow> = rows.iter().collect();
        sorted.sort_by_key(|row| row.position);
        let mut bytes: Vec<u8> = Vec::new();
        for row in &sorted {
            bytes.extend_from_slice(b"{\"");
            bytes.extend_from_slice(ROW_ID_KEY.as_bytes());
            bytes.extend_from_slice(b"\":\"");
            bytes.extend_from_slice(row.id.to_string().as_bytes());
            bytes.push(b'"');
            for column in 0..columns {
                bytes.push(b',');
                bytes.extend_from_slice(format!("\"c{column}\":").as_bytes());
                let value = row.values.get(column).unwrap_or(&CellValue::Null);
                // 非有限の浮動小数はここで拒否される（value 層が単一の源）。材料は
                // ドキュメントにあった値であるため、通常は起こらない — 起こったなら
                // その行の値を書き戻せないという意味であり、行の識別子を載せて返す
                // （`GridError` の 5 変種にこれ以上適切な変種は無い。**変種は増やさない**）。
                let encoded = to_json_bytes(value, &location)
                    .map_err(|_| GridError::UnknownRow { row: row.id })?;
                bytes.extend_from_slice(&encoded);
            }
            bytes.extend_from_slice(b"}\n");
        }
        let decoded = RowsCodec::decode(&entry, &bytes)
            .map_err(|_| GridError::UnknownRow { row: sorted[0].id })?;
        // 復号は列数ぶんのキーを書いた形を返す（行の幅はつねに列数である）。材料の幅は
        // 差し戻しの**後**に [`EditApply::write_material_rows`] が書く。
        Ok(sorted
            .iter()
            .zip(decoded.into_rows())
            .map(|(material, row)| (material.position, row))
            .collect())
    }

    /// 履歴の合成（**1 つの操作**）を順に適用する（要件 7.6）。
    ///
    /// 部分は**並びの順**に適用する（貼り付けの逆命令は「覆ったセルの値を戻す → 補充した行を
    /// 取り除く」であり、逆にすると値の復元先が消える）。結果は 1 つにまとめる — 影響を
    /// 受けた行は部分の和、違反の総数は**最後に 1 回**全列を再検証したもの
    /// （部分が触れた列は重なりうるため、部分の総数を足すと同じ違反を二重に数える）、
    /// 行数は適用の後の実数である。
    fn apply_parts(
        &mut self,
        doc: &mut Document,
        parts: &[HistoryCommand],
    ) -> Result<EditOutcome, GridError> {
        let sheet = self.sheet;
        let mut affected: Vec<RowId> = Vec::new();
        for part in parts {
            let outcome = self.apply_history(doc, part)?;
            affected.extend(outcome.affected);
        }
        // 同じ行が 2 度現れる合成（値の復元と行の削除が同じ行を指す等）は 1 回へ畳む
        // （`affected` の既存の規律）。
        let mut seen: HashSet<RowId> = HashSet::with_capacity(affected.len());
        affected.retain(|row| seen.insert(*row));
        Ok(EditOutcome {
            affected,
            coercions: Vec::new(),
            violation_total: self.revalidate_every_column_of(doc, sheet),
            row_count: self.sheet_of(doc, sheet)?.rows().len(),
        })
    }

    /// 名指されたシートの前提を検査し、**この適用で使える列数**を返す
    /// （[`EditApply::usable_columns`] のシート指定版）。
    fn usable_columns_of(&self, doc: &Document, sheet: SheetId) -> Result<usize, GridError> {
        let columns = self.schema.column_count();
        if columns == 0 {
            return Err(GridError::SchemaUnusable { sheet });
        }
        self.sheet_of(doc, sheet)?;
        Ok(columns)
    }

    /// 名指されたシートを引く（計画の列数の一致も確かめる）。
    fn sheet_of<'d>(&self, doc: &'d Document, sheet: SheetId) -> Result<&'d Sheet, GridError> {
        let found = doc
            .sheet_by_id(sheet)
            .ok_or(GridError::SchemaUnusable { sheet })?;
        if found.columns().len() != self.schema.column_count() {
            return Err(GridError::SchemaUnusable { sheet });
        }
        Ok(found)
    }

    /// 名指されたシートについて、何も変えなかった適用の結果を返す。
    fn unchanged_of(&self, doc: &Document, sheet: SheetId) -> Result<EditOutcome, GridError> {
        Ok(EditOutcome {
            affected: Vec::new(),
            coercions: Vec::new(),
            violation_total: 0,
            row_count: self.sheet_of(doc, sheet)?.rows().len(),
        })
    }

    /// 名指されたシートの**すべての列**を 1 回だけ再検証し、その総数を返す
    /// （[`EditApply::revalidate_every_column`] のシート指定版）。
    fn revalidate_every_column_of(&self, doc: &Document, sheet: SheetId) -> usize {
        let columns: Vec<ColumnIndex> = (0..self.schema.column_count())
            .map(ColumnIndex::new)
            .collect();
        self.query
            .revalidate_columns(doc, sheet, &self.schema, &columns, &ValidationOptions::capped(0))
            .total_violations()
    }

    /// 対象シートを引く。文書に無い場合と、計画の列数と食い違う場合は使用不能として返す。
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
///
/// 編集は**セル値**として載せる。`SetCells` は打たれた文字を [`edited_value`] で値へ写して
/// から載せ、`SetNested` は上流の解釈が返した値をそのまま載せる — この 2 経路の違いは
/// **値を作るところだけ**であり、判定を呼ぶ形も書き込みの形も変わらない。
struct RowEdit {
    /// 対象の行。
    row: RowId,
    /// 行の**適用前**の値（列の添字で並ぶ）。
    values: Vec<CellValue>,
    /// 書くセル（列の添字）と、そこへ書く値（命令に現れた順）。
    edits: Vec<(ColumnIndex, CellValue)>,
}

impl RowEdit {
    /// 打たれた文字の編集を 1 つ載せて作る（`SetCells`）。
    fn from_text(row: RowId, values: Vec<CellValue>, column: ColumnIndex, text: String) -> Self {
        Self::from_value(row, values, column, edited_value(&text))
    }

    /// 判定へ渡すセル値の編集を 1 つ載せて作る（`SetNested` が解釈した値は既にセル値である）。
    fn from_value(
        row: RowId,
        mut values: Vec<CellValue>,
        column: ColumnIndex,
        value: CellValue,
    ) -> Self {
        // 行が値を持たない列への書き込みは、値なしを挟んで位置を合わせる（`validate_write` は
        // 値の添字を列の添字として読む。値なしの列は上流でも値なしとして扱われる）。
        values.resize(values.len().max(column.index() + 1), CellValue::Null);
        Self {
            row,
            values,
            edits: vec![(column, value)],
        }
    }

    /// 同じ行の別のセル（または同じセルの 2 度目）の編集を載せる。
    ///
    /// 同じセルが既に載っている場合は**後ろのものを残す**（上流の一括経路の last-wins と
    /// 同じ規則。モジュール docs「同じセルを 2 度書く命令」）。
    fn edit(&mut self, column: ColumnIndex, text: String) {
        self.put(column, edited_value(&text));
    }

    /// 指定した列の値を編集として載せる（同じ列が既にあれば置き換える）。
    fn put(&mut self, column: ColumnIndex, value: CellValue) {
        self.values
            .resize(self.values.len().max(column.index() + 1), CellValue::Null);
        match self
            .edits
            .iter_mut()
            .find(|(existing, _)| *existing == column)
        {
            Some(found) => found.1 = value,
            None => self.edits.push((column, value)),
        }
    }

    /// 判定へ渡す 1 行分の値（編集した列の値を置き換えたもの）。
    fn edited_values(&self) -> Vec<CellValue> {
        let mut values = self.values.clone();
        for (column, value) in &self.edits {
            values[column.index()] = value.clone();
        }
        values
    }
}

/// 貼り付けの 1 行が覆う列がシートの範囲に収まることを確かめる。
///
/// 矩形の列は錨の列を起点とする相対位置であり、覆う列は `start .. start + count` である。
/// 呼び出し元が錨の列そのものを先に検査しているため、はみ出す場合の**最初の範囲外の列は
/// つねに列数そのもの**である（錨が範囲内なら、その先で最初に外れるのは列数の位置である）。
fn check_paste_columns(start: ColumnIndex, count: usize, columns: usize) -> Result<(), GridError> {
    if start.index() + count > columns {
        return Err(GridError::ColumnOutOfRange {
            column: ColumnIndex::new(columns),
            count: columns,
        });
    }
    Ok(())
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

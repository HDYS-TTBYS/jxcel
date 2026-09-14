//! 並べ替えの順序の導出: 可視行の並び [`RowOrder`] と、その指定 [`ViewSpec`]。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本層は左の `types` だけを
//! 参照する**（design.md「内部の依存の向き」）。層の鎖の文言を各層の冒頭に置く規約は
//! `structure.md`「ドメインクレートの内部構造」。
//!
//! 本モジュールが上流に求めるのは `document-format` の [`Document`] / [`Row`] / [`RowId`] と、
//! `schema-engine` の [`ColumnIndex`]・10 進数の正準形（`schema_engine::types::decimal`）だけ
//! である。**判定は呼ばない**（値が型に適合するかを決めるのは `schema-engine` であり、
//! 本モジュールは値の大小だけを決める）。
//!
//! # 何を所有するか
//!
//! [`RowOrder`] は **`Vec<RowId>`（可視行の順）、隠された行数、そして違反の有無の据え付け
//! （[`ViolationPresence`]）を持つ**（design.md「RowOrder」の Responsibilities & Constraints。
//! 据え付けが 3 つ目に要る理由は「違反ありの絞り込みは据え付けられた情報だけを見る」）。
//! 座標の型（[`RowOrdinal`] / [`RowSpan`]）は鎖の最も左の `types` 層にあり、本モジュールは
//! それを使う側である。[`RowOrder`] / [`ViewSpec`] / [`SortKey`] / [`FilterSpec`] /
//! [`ViewSummary`] / [`ViolationPresence`] は**本層の状態と指定**であり、design.md の
//! File Structure Plan が `view/mod.rs` に置いている（`types` 層は「どの層にも依存しない
//! 横断する型」の置き場であり、行の順序という状態をそこへ移す理由は無い。`SortKey` と
//! `FilterSpec` が列の添字として `types` の再輸出である [`ColumnIndex`] を使うのは、
//! 列の添字を 2 つに増やさないためである。`types` のモジュール docs 参照）。
//!
//! # 並べ替えは表示に閉じる（要件 8.5）
//!
//! 本製品の中心価値は「いつ・誰が・どのセルを変えたか」が追えることであり、並べ替えが
//! 保存される順序を書き換えると、並べ替え 1 回で全行が変更されたように見える（要件 8 の
//! 方針の根拠）。したがって [`RowOrder::recompute`] は [`Document`] を**共有参照でしか
//! 受け取らない**。この 1 点が「ドキュメントの行の並びを書き換える経路を持たない」ことを
//! 型の上で示す実体である（要件 8.5。`&mut Document` を取る形にすれば書き換える経路が
//! 生まれ、呼び出し側は共有借用を持ったまま呼べなくなる。`tests/sort_order.rs` の
//! `the_order_derivation_only_borrows_the_document` がこの形を検査する）。
//!
//! ドキュメントに触れない帰結として、**保存された値の文字列も変えない**。とくに
//! 10 進数は逐語で往復する契約であり（`document-format` の `value.rs`）、比較のための
//! 正準形は**比較のためだけに作り**、`CellValue::Decimal` の中身へ書き戻さない。
//! 絞り込みも同じである: 絞り込みは何も書き換えず、可視の集合を選ぶだけであり、
//! [`RowOrder::recompute`] の signature は絞り込みを足しても `&Document` のままである
//! （`tests/filter_order.rs` の `filtering_only_borrows_the_document_and_leaves_it_unchanged`
//! が、ドキュメントの共有借用を生かしたまま導出し、行の並びと値が変わらないことを見る）。
//!
//! # 絞り込みは積であり、順序は絞り込んだ集合に定まる（要件 8.4, 8.7）
//!
//! [`RowOrder::recompute`] は **まず絞り込みで可視の集合を選び、その集合を並べ替える**。
//! 絞り込みの判定は**文書に現れる行**に対して行う（`Sheet::rows()` の並びを 1 度だけ走査し、
//! その部分列を並べ替える）。**順序は絞り込んだ集合に対して定まる**（除外された行は順序の
//! 計算に入らず、可視の並びに現れない。要件 8.4, 8.7）。
//!
//! 絞り込みを先に行うのは**費用**の理由である: 捨てる行を並べ替えの比較に引き込まない。
//! 基準列の比較は 10 万行で最も重い処理であり（要件 11 の予算はそこに掛かる）、
//! 絞り込みが 1 割を通すなら 9 割の比較を捨てることになる。並べ替えてから絞る形でも
//! **結果は同じ**である — 2.1 の比較器は全順序であり（モジュール docs「決着と決定性」）、
//! 全順序による並べ替えの部分列は、部分列を並べ替えたものと一致するためである。
//! つまりこれは意味論の違いではなく同じ意味論の安い実装であり、どちらの経路も
//! `tests/filter_order.rs` の `the_order_is_derived_over_the_filtered_set` が同じ期待で
//! 固定する（除外された行が現れないことと、含まれる行が値の順に並ぶことの双方向）。
//!
//! 複数の絞り込みは**積（AND）**である: 並びのすべてに一致する行だけが可視になる。
//! 一致する行が 1 つも無ければ可視 0 であり、これは「違反ありの情報が無い」場合を含む
//! （後述）。絞り込みの並び順は集合を変えない（積は可換であり、これはテストで固定する）。
//!
//! 可視行数と隠された行数は [`ViewSummary`] で返る（要件 8.7）。隠された行数は
//! **シートの行数と可視行数の差**として導出する（[`RowOrder::hidden`]）。したがって
//! `visible + hidden == シートの行数` が常に成り立ち、これは「それぞれの数」ではなく
//! **恒等式として** `tests/filter_order.rs` が検査する（一方だけを直す取り違えが落ちる）。
//!
//! ## 絞り込みの入口と段取り
//!
//! | 変種 | 可視になる行 |
//! |---|---|
//! | [`FilterSpec::Equals`] | 表示文字列が `text` と**完全に一致**する行 |
//! | [`FilterSpec::Contains`] | 表示文字列が `text` を**含む**行 |
//! | [`FilterSpec::IsEmpty`] | 表示文字列が**空**の行（値なし・空のテキスト・届かない値） |
//! | [`FilterSpec::IsNotEmpty`] | 表示文字列が**空でない**行 |
//! | [`FilterSpec::HasViolation`] | 据え付けられた違反の情報がその行（と列）を違反とする行 |
//!
//! 列の添字が行の値の数に届かない場合は、2.1 と同じく**値なし**として扱う
//! （`value_of`。モジュール docs「値を持たない行」）。したがって
//! `IsEmpty` は「その列の値を持たない行」も選び、`Equals { text: "" }` と同じ行を選ぶ
//! （両者が食い違わないことをテストが固定する）。
//!
//! # 表示文字列の写しは本層が 1 つだけ持つ（5.1 への引き渡し）
//!
//! [`FilterSpec::Equals`] / [`FilterSpec::Contains`] は、型付きの値と利用者が打ったテキストを
//! 突き合わせる。このとき比較するのは**値そのものではなく表示文字列**（[`display_text`]）で
//! ある。値を比較しない理由は、利用者が打てるのがテキストだけであり、`Int(10)` と
//! `Decimal("1e1")` と `Float(10.0)` を数値として等しいと見なす規則を本層が持つと
//! **判定の分岐を持つ**ことになるためである（本クレートは判定を持たず `schema-engine` に
//! 委ねる。クレート docs「依存方針の要約」）。したがって絞り込みは
//! **「画面に見えている文字列」の一致**であり、表示が一致する値は絞り込みでも一致する。
//!
//! ## 変種ごとの規則（正典）
//!
//! | 変種 | 表示文字列 |
//! |---|---|
//! | `Null` | 空文字（値なしは何も見えない） |
//! | `Bool` | `true` / `false` |
//! | `Int` | 十進の数字列（負号をつける。`i64` の全域で桁落ちしない） |
//! | `Float` | 最短の往復可能な十進表記（Rust の [`format!`] の `{}`）。**仮数と指数を分けず指数表記を使わない**（`1e21` ではなく `1000000000000000000000`）。`-0.0` は `0` へ畳む。非有限（`NaN` / `inf`）はそのまま `NaN` / `inf`（保存の門が拒むため本来は現れない） |
//! | `Decimal` | **保持された文字列そのまま**（正規化しない。`"007.50"` は `"007.50"`） |
//! | `Text` | そのもの |
//! | `Nested` | 最上位の要素数の要約（オブジェクトは `N項目`、配列は `N要素`。要件 5.6 の要約であり、中身は含めない） |
//! | `Attachment` | 正準の小文字 hex（64 文字。`AttachmentId` の `to_hex`（`document-format`）） |
//!
//! **指数表記を使わない**のは、10 進数の逐語の契約と揃えるためである。`Decimal` を
//! `format!("{}", f64)` で書くと `1e21` のような表記が現れうるが、`Decimal` の中身には
//! 一切触れない（そのまま出す）ので、指数表記が現れるのは `Float` だけであり、
//! それは `{}` の規則（指数を使わない最短表記）に従う。
//!
//! ## 大文字小文字は区別する
//!
//! `Equals` / `Contains` はいずれも**バイト列の一致**であり、大文字小文字を畳まない
//! （`"Apple"` と `"apple"` は別の表示文字列であり、`contains("PP")` は `"Apple"` に
//! 一致しない）。並べ替えが地域の並び替え規則を使わない（モジュール docs「変種の順位」の
//! `Text`）のと同じ理由であり、畳む規則を入れると**表示が区別しているものを絞り込みが
//! 区別しなくなる**（画面に `Apple` と `apple` の 2 行が見えているのに、`equals("apple")` が
//! 両方を返す状態になる）。
//!
//! ## 値なしの一致（`IsEmpty` と `Equals { text: "" }`）
//!
//! [`FilterSpec::IsEmpty`] は**表示文字列が空であること**と定める。`Equals { column, text: "" }`
//! は「表示文字列が空文字と完全に一致すること」であり、同じ条件になる — 両者は常に同じ行を
//! 選ぶ（規則が 2 つに分かれて食い違うことをテストが禁じる）。ここで `Null` と
//! **空のテキスト**（`Text("")`）は同じ表示文字列になるため、`IsEmpty` は値なしの行と
//! 空文字を打たれた行の双方を選ぶ。これは表示の一致としては正しく、両者を区別する絞り込みは
//! 本層の範囲に無い（区別が要るなら [`FilterSpec`] に変種の札を加えるのが拡張点である）。
//!
//! ## 5.1 への引き渡し
//!
//! `WindowCodec`（タスク 5.1）は窓の二進形式で**セルの表示文字列**を運ぶ（design.md
//! 「Data Models / 窓の二進形式」）。その写しを 5.1 がもう 1 つ書くと、**同じ値が絞り込みと
//! 画面で違う文字列になる**（利用者が画面で見た文字列で絞り込んだのに一致しない）。
//! したがって表示文字列の規則は [`DisplayText`]（[`core::fmt::Display`] 実装）が**唯一の源**
//! であり、[`display_text`] はそこから借用で取る薄い入口である。5.1 はこれを再利用する
//! （`Decimal` をそのまま出す・`Int` を十進で出す・入れ子を要約する、の 3 点が 5.1 の
//! 要件とそのまま重なる）。本層は `transport` より左にあるため、依存の向きも正しい。
//!
//! # 比較は値の変種ごとの順序で行う（要件 8.3）
//!
//! 表示文字列（`CellValue` を人が見る形へ写した文字列）で比較すると、`9` と `10` が
//! `"10" < "9"` になり、真偽が `"false" < "true"`、値なしが `""` として先頭に来る。
//! したがって本モジュールは [`CellValue`] の**変種ごとに**順序を定め、変種をまたぐときは
//! 順位で決める。これが並べ替えの核心である（タスク 2.1）。
//!
//! ## 変種の順位（正典）
//!
//! ```text
//! Null < Bool < Int < Float < Decimal < Text < Nested < Attachment
//! ```
//!
//! この並びが**正典**である（design.md「Implementation Notes」は「セルの表示文字列ではなく
//! 値の変種ごとの順序で行う」ことだけを定めており、順位そのものは 2.1 が決めた）。
//! [`CellValue`] は `PartialEq` だけを持ち `Ord` を持たないため、この順位と以下の規則が
//! 順序の唯一の源である。決め方は「値なし → 真偽 → 数値 → 文字列 → 構造 → 添付」であり、
//! 並べ替えの第一の読み方（小さい値・偽・空が先、構造は後）に合わせてある。
//!
//! | 変種 | 同じ変種の中の順序 |
//! |---|---|
//! | `Null` | すべて同値（値なしに大小は無い） |
//! | `Bool` | `false < true` |
//! | `Int` | 数値として（`i64` の `Ord`） |
//! | `Float` | 数値として（下記「浮動小数」） |
//! | `Decimal` | **数値として**（下記「10 進数」） |
//! | `Text` | UTF-8 のバイト列の辞書式順序（`str` の `Ord` そのもの。地域の並び替え規則は使わない） |
//! | `Nested` | オブジェクト < 配列。オブジェクトはキー→値の順の辞書式、配列は要素の辞書式（下記） |
//! | `Attachment` | 内容アドレスの識別子（BLAKE3 ダイジェスト）のバイト列順 |
//!
//! **浮動小数**: まず `-0.0` を `0.0` へ畳み（`CellValue::float` の正規化と `CellValue` の
//! `PartialEq` が `-0.0 == 0.0` であることに合わせる。畳まないと、等しいと見なされる 2 つの
//! 値が同値にならず決着が効かない）、`f64::total_cmp` で比べる。非数（`NaN`）は比較の相手が
//! 何であっても順序が定まる位置に来る（正の非数はすべての数より後、負の非数はすべての数より
//! 前）。`NaN` を書けるのは復号の門を迂回した値だけであり（`to_json_bytes` は非有限を
//! 拒否する）、**比較器は全順序でなければならない**（`sort_by` は全順序でない比較器に対して
//! 並びを保証せず、実装によっては失敗する）。
//!
//! **入れ子**: オブジェクトを配列より前に置く。オブジェクトは**キー順をそのまま保つ**
//! `Vec<(String, CellValue)>` であり（`HashMap` は反復順が実行ごとに変わるため
//! `document-format` が禁じている）、先頭から突き合わせてキー、次に値、を比べる。どちらかが
//! 他方の前置なら短い側が先（`Vec` の辞書式の規約と同じ）。配列も同じ規約で要素を比べる。
//!
//! **添付**: 添付の識別子は内容から決まる（content-addressed）ため、同じ内容は常に同じ
//! 位置に並ぶ。順序は 32 バイトのダイジェストのバイト列順であり、これは正準の小文字 hex
//! （固定幅）の辞書順と一致する。
//!
//! # 10 進数は数値として比較する
//!
//! `CellValue::Decimal` の中身は**文字列であり、文法の外の中身も持ちうる**。逐語で往復する
//! 契約（`document-format`。文法に一致しない中身は脱出口 `{"$t":"decimal",...}` で書かれる）
//! のため、文法を検査せずに保持される。
//!
//! 桁数の比較のために**新しい依存は足さない**。10 進数のライブラリ（`rust_decimal` 等）は
//! 出力時に正規化を行うため、逐語の契約を壊す（`schema-engine` の `types/decimal.rs` の
//! モジュール docs「なぜ 10 進数のライブラリを入れないか」）。比較のための正準形は上流に
//! 既にある — `schema_engine::types::decimal::canonicalize` が、文法に一致する文字列を
//! 値そのもの（符号・先頭と末尾の 0 を除いた数字列・10 の指数）へ畳む。本モジュールはそれを
//! **呼ぶだけ**であり、10 進数の文法も桁勘定も書き直さない（文法が 2 つに分かれると、
//! 上流が保存した `Decimal` を本クレートが別の値として並べる状態になる）。
//!
//! 規則は次のとおりである。
//!
//! - 双方が正準形を作れるなら、**正準形の `Ord`** で比べる。正準形の順序は値の順序に一致し、
//!   `-1000 < -2 < -1.5 < 0 < 0.0001 < 0.5 < 1 < 1.5 < 2 < 10 < 100 < 1e3` のようになる
//!   （`"9"` と `"10"` は `9 < 10` であり、文字列の辞書順とは逆である）。
//!   指数表記・末尾の 0・先頭の 0・明示の正符号は同じ値に畳まれるため、`"1.5"` と `"1.50"` と
//!   `"1e1"`/`"10"` は**同値**であり、決着（後述）で `RowId` の順に並ぶ。極端な指数でも
//!   表現が衝突しない（正準形は指数を展開しない）。
//! - **文法に一致しない中身**（決定的な規則が必要であり、値としての大小が存在しない）:
//!   文法に一致する値**より後ろ**に置き、文法外どうしは**バイト列の辞書順**で比べる。
//!   文法外の値は上流でも違反として報告される値であり（`DecimalDigits::accepts` が
//!   `Violating` を返す）、数として読めないものを数の列に混ぜない。規則は全順序であり、
//!   同じ入力からは常に同じ順序が出る（順序を「不定」にしない理由は、要件 8.3 が同一の
//!   入力から同一の順序を求めるためである）。
//!
//! # 変種をまたぐ数値の比較はしない
//!
//! `Int` / `Float` / `Decimal` は**別の変種**であり、順位が違う。したがって `Int(5)` と
//! `Float(5.0)` と `Decimal("5")` は**同値ではなく**、`Int(5) < Float(5.0) < Decimal("5")`
//! の順に並ぶ（数値として同値なら同じ位置に来る、という扱いはしない）。
//!
//! これは「値が型に適合するかの判断を本クレートが持たない」ことの帰結である。列の型が
//! 決まっていれば現れない値の組み合わせ（`document-format` の `CellValue` は変種の閉じた
//! 集合であり、1 つの列に複数の変種が並びうる）でも順序が定まることを優先し、変種の同一性を
//! 値の等値より先に見る。**安定性への帰結**: 数値として等しい値でも変種が違えば同値では
//! ないため、その 2 行の前後は決着（`RowId` の順）には委ねられず、順位が決める。
//!
//! # 決着と決定性（design.md の Invariants）
//!
//! design.md は「同一の `Document` と `ViewSpec` からは常に同一の順序が出る（並べ替えは
//! 安定であり、同値の行は `RowId` の順で並ぶ）」を不変条件とする。本モジュールは
//! **比較器そのものを全順序にする** — 基準列がすべて同値なら、最後の鍵として `RowId` の
//! 昇順を比較する。したがって並べ替えの結果は一意であり、**`sort_by` の安定性には依存しない**
//! （安定でない並べ替えでも同じ結果になる。安定であることは結果の性質として従う）。
//! `RowId` は ULID であり、その `Ord` は値の数値順 = 正準テキスト形の辞書順である
//! （`document-format` の `ids.rs`）。行の識別子は行ごとに一意であるため、決着は必ず付く。
//!
//! 実装は `sort_by`（安定な並べ替え）を使う。`sort_unstable_by` でも結果は同じ（比較器が
//! 全順序だから）であり、**どちらでも正しいが `sort_by` を選ぶ**: 決着の規則を将来
//! 取り違えても（たとえば `RowId` の比較を落としてしまっても）安定性が最後の防波堤として
//! 残るためである。つまり「正しさは全順序の比較器が担い、安定性は保険である」。設計が
//! 「安定な並べ替え」を明示している以上、素直に安定な側を使う。
//!
//! **降順はその基準列の比較だけを反転し、決着は反転しない。**反転させると昇順と降順で
//! 同値の行の並びが変わり、「決着は `RowId` の順」という不変条件が指定に依ってしまう。
//! 反転は各基準列の比較の直後に行い、`RowId` の比較はその外側で最後に 1 度だけ行う。
//!
//! 再計算は [`RowOrder::recompute`] の呼び出しごとに**入力だけから**順序を組み立てる
//! （前回の順序を引き継がない）。したがって「別の指定で上書きした後に戻しても同じ順序」に
//! なり、呼び出しの履歴に依らない。
//!
//! # 基準列が 0 本のときは文書の行順を保つ
//!
//! 決着（`RowId` の昇順）は**基準列があって、そのすべてが同値のとき**にだけ働く。基準列が
//! 0 本のときは比較そのものを行わないため、**文書の行順（`Sheet::rows()` の並び）がそのまま
//! 可視の順**になる。この区別は要件 6.1（位置を指定した行の挿入）と要件 8.6（行の増減が
//! 提示へ直ちに反映される）が要求する — 挿入された行の `RowId` は必ず最も新しい（ULID は
//! 発行時刻を先頭に持つ）ため、基準列が無いときに `RowId` の順へ並べ替えると、挿入した行が
//! **指定した位置ではなく末尾に現れる**。文書の行順は「行が実際に並んでいる順」であり、
//! 基準列が無いときの可視の順はまさにそれである。`Document::reorder_rows`（要件 1.5）が
//! 保存される順序を変えた後も、この規則は一貫して文書の並びを写す。
//!
//! # 値を持たない行
//!
//! 行の値の数が列の添字に届かない場合（列の追加前に作られた行、長さの短い行）は
//! **値なし（`CellValue::Null`）として扱う**。列を追加した直後の行が並べ替えで
//! 落ちたり、比較や絞り込みが失敗したりしないためである（`Row::values()` の外を読まない）。
//! 並べ替えと絞り込みは同じ扱いを共有する（`value_of`。届かない値は表示文字列が空に
//! なるため、`IsEmpty` はその行も選ぶ）。
//!
//! # 違反ありの絞り込みは据え付けられた情報だけを見る（2.4 / 5.2 への引き渡し）
//!
//! [`FilterSpec::HasViolation`] は**違反の有無の情報**を要するが、本層は違反を判定しない。
//! 違反を判定するのは `schema-engine` であり、可視行の序数に対する違反の索引を持つのは
//! タスク 2.4 の `view/violations.rs` である（design.md の層の鎖
//! `error / types → view → edit → history → transport → api` と、Component 表の
//! `ViolationIndex` が `RowOrder` に依存する向き）。**本層が `validate_sheet` を呼ぶ形には
//! しない** — 導出のたびにシート全体の検証を走らせることになり、要件 11.4 の根拠
//! （全件検証を編集のたびに走らせない）が禁じている費用を、絞り込みのたびに払うことになる。
//! また 2.4 の索引を持つ向きと依存が閉じなくなる（`RowOrder` が `ViolationIndex` に依存し、
//! `ViolationIndex` が `RowOrder` に依存する循環）。
//!
//! したがって違反の有無は**呼び出し側が据え付ける**（[`RowOrder::set_violation_presence`]）。
//! 据え付けの形は [`ViolationPresence`] であり、`RowId` ごとに「どの列が違反か」を持つ
//! （[`ViolationPresence::mark_row`] は列を問わない違反、[`ViolationPresence::mark_column`]
//! は特定の列の違反）。**既に索引を持っている側が 1 度だけ据え付ける**ので、絞り込みの
//! たびに違反を引き直す費用は生じない。
//!
//! **既定は空であり、そのとき `HasViolation` はどの行にも一致しない。**これは stub では
//! なく正しい答えである — 空の据え付けは「違反が無い」ではなく**「違反の情報がまだ
//! 与えられていない」**を意味し、情報が無い状態で「違反がある行」を答えることはできない。
//! 据え付けを行うのは次の 2 つである:
//!
//! - **タスク 2.4**（`view/violations.rs`）— `validate_sheet` の `SheetReport` から
//!   `ViolationPresence` を組み立て、順序と絞り込みが変わるたびに据え付ける
//! - **タスク 5.2**（`GridSession` の組み立て）— セッションが保持する索引から据え付ける
//!
//! 据え付けは**順序の状態**であり、導出のたびに消えない（[`RowOrder::recompute`] は
//! 据え付けに触れない）。[`RowOrder::set_violation_presence`] の呼び出しは据え付けを
//! **丸ごと置き換える**（差分を積まない）ため、据え付ける側は毎回その時点の全体を渡す。
//!
//! # 文書に無いシート
//!
//! `SheetId` が文書に無い場合は**行が 1 件も無いシート**として扱い、可視行 0 の順序を返す
//! （`Document::sheet_by_id` は `Option` を返し、`recompute` の signature は `Result` を
//! 持たない。design.md の Service Interface）。シートの妥当性を先に確かめるのは呼び出し側
//! （`GridSession::set_view`。要件 8.3）の責務である。
//!
//! # 本層が持たないもの
//!
//! - **基準列の値の編集で行が動かないこと**（要件 8.8）。順序と絞り込みの再計算はこの入口の
//!   呼び出しでだけ起き、編集の経路（群 3）はここを呼ばない
//! - **可視行の序数に対する違反の索引**（要件 4.1, 4.3, 4.4, 4.5）。2.4 が `view/violations.rs`
//!   に置く。本モジュールは違反を**判定せず**、据え付けられた [`ViolationPresence`] を
//!   絞り込みの条件として読むだけである（モジュール docs「違反ありの絞り込みは据え付けられた
//!   情報だけを見る」）
//! - **入れ子の展開から導かれる列の構成**（要件 5.1 等）。2.3 が `ViewState` として足す。
//!   展開された入れ子の内側の列は絞り込みの対象にならない（[`FilterSpec`] の列の添字は
//!   `Row::values()` に対する位置であり、展開された列ではない）
//! - **表示文字列の境界の符号化**。5.1 の `WindowCodec` は [`DisplayText`] / [`display_text`]
//!   を**再利用する**（写しを作らない。モジュール docs「表示文字列の写しは本層が 1 つだけ持つ」）
//! - 誤り型。順序と絞り込みの導出は失敗しない（`GridError` を返す経路を持たない）

use core::cmp::Ordering;
use core::fmt::Write as _;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use document_format::{CellValue, Document, NestedValue, Row, RowId, SheetId};
use schema_engine::types::decimal;

use crate::types::{ColumnIndex, RowOrdinal, RowSpan};

/// 並べ替えの基準列 1 本: 列の添字と、降順かどうか。
///
/// 列の添字は `schema-engine` の [`ColumnIndex`] そのものである（本クレートが独自の列添字を
/// 定義すると、見た目が同じ 2 つの型が生まれて列が静かにずれる。`types` のモジュール docs）。
/// 添字は `Row::values()` に対する位置であり、シートの列名の並び（`Sheet::columns`）と
/// `CompiledSchema::columns` の並びは同じものである。
///
/// `descending` は**その基準列の比較だけ**を反転する。同値の行の決着（`RowId` の昇順）は
/// 反転しない（モジュール docs「決着と決定性」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SortKey {
    /// 基準となる列の添字（0 起点。`Row::values()` に対する位置）。
    pub column: ColumnIndex,
    /// この基準列を降順で並べるか。
    pub descending: bool,
}

/// 絞り込みの条件 1 本: どの列を、どの条件で選ぶか。
///
/// 変種は design.md の Service Interface が固定している 5 つである（要件 8.4 の
/// 一致・部分一致・値なし・値あり・違反ありがこの 5 つに対応する）。複数与えられた場合は
/// **積**として働く（[`ViewSpec::filters`]）。列の添字は [`SortKey::column`] と同じく
/// `Row::values()` に対する位置である。
///
/// `Equals` / `Contains` が比較するのは**値ではなく表示文字列**（[`display_text`]）である。
/// 規則と、なぜ値を比較しないかはモジュール docs「表示文字列の写しは本層が 1 つだけ持つ」。
/// 大文字小文字は区別し、`Decimal` は保持された文字列そのまで比較する。
///
/// `IsEmpty` は表示文字列が空であること（= `Equals { text: "" }`）であり、値の数が列の添字に
/// 届かない行も選ぶ（モジュール docs「値を持たない行」）。
///
/// `HasViolation` は**据え付けられた情報**（[`ViolationPresence`]）だけを見る。据え付けが
/// 空のときはどの行にも一致しない（モジュール docs「違反ありの絞り込みは据え付けられた情報
/// だけを見る」）。
///
/// `text` は利用者が打った文字列をそのまま持つ（本層は入力の正規化をしない。表示文字列との
/// 比較が唯一の解釈であり、大文字小文字の畳み込みや空白の除去を行わない）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FilterSpec {
    /// 列の表示文字列が `text` と完全に一致する行を選ぶ。
    Equals {
        /// 対象の列の添字（0 起点。`Row::values()` に対する位置）。
        column: ColumnIndex,
        /// 一致させる表示文字列（そのまま比較する）。
        text: String,
    },
    /// 列の表示文字列が `text` を含む行を選ぶ。
    Contains {
        /// 対象の列の添字（0 起点。`Row::values()` に対する位置）。
        column: ColumnIndex,
        /// 含まれることを求める表示文字列（そのまま比較する。空文字は全行に一致する）。
        text: String,
    },
    /// 列の表示文字列が空である行を選ぶ（値なし・空のテキスト・届かない値）。
    IsEmpty {
        /// 対象の列の添字（0 起点。`Row::values()` に対する位置）。
        column: ColumnIndex,
    },
    /// 列の表示文字列が空でない行を選ぶ。
    IsNotEmpty {
        /// 対象の列の添字（0 起点。`Row::values()` に対する位置）。
        column: ColumnIndex,
    },
    /// 違反を持つ行を選ぶ（据え付けられた [`ViolationPresence`] を読むだけである）。
    HasViolation {
        /// 対象の列。`None` は**列を問わない**（その行に違反が 1 つでもあれば選ぶ）。
        column: Option<ColumnIndex>,
    },
}

/// 行ごとの違反の有無: [`FilterSpec::HasViolation`] の判定に使う据え付け。
///
/// **本層は違反を判定しない。**違反を判定するのは `schema-engine` であり、可視行の序数に
/// 対する索引を作るのはタスク 2.4 である。本型はその結果を**受け取る形**であり、
/// `RowId` ごとに「どの列が違反か」を持つ（モジュール docs「違反ありの絞り込みは据え付けられた
/// 情報だけを見る」に、据え付けを行う側と、`RowOrder` が `validate_sheet` を呼ばない理由）。
///
/// 行ごとの違反は 2 つの形を取る:
///
/// - **行の印**（[`ViolationPresence::mark_row`]）— その行に違反があるが、**どの列かは
///   与えられていない**。`HasViolation { column: None }` に一致し、
///   `HasViolation { column: Some(..) }` には**一致しない**
/// - **列の印**（[`ViolationPresence::mark_column`]）— その行のその列に違反がある。
///   どちらの `HasViolation` にも一致する（列を問わない側は「1 つでも違反があれば」だから）
///
/// 同じ行に列の印を複数つけられる（1 行の複数の列が違反している場合）。行の印と列の印は
/// 独立であり、両方を同じ行につけることもできる。
///
/// # 決定性
///
/// 内部は `RowId` を鍵とする [`BTreeMap`] であり、**反復の順が値から決まる**。絞り込みは
/// 鍵の有無しか読まないため順序は結果に影響しないが、`HashMap` を使わないのは
/// `document-format` が `NestedValue::Object` で `HashMap` を禁じているのと同じ理由である
/// （反復順が実行ごとに変わると、順序に依る実装を後から足したときに決定性が壊れる）。
///
/// [`BTreeMap`]: std::collections::BTreeMap
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViolationPresence {
    /// 行ごとの違反（列の集合が空なら「どの列かは与えられていない」）。
    rows: BTreeMap<RowId, BTreeSet<ColumnIndex>>,
}

impl ViolationPresence {
    /// 違反の情報が 1 つも無い据え付けを作る。
    ///
    /// 空は「違反が無い」ではなく**「違反の情報がまだ与えられていない」**である
    /// （[`FilterSpec::HasViolation`] はこのときどの行にも一致しない）。
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// 行 `row` に、列を問わない違反があるとして印をつける。
    ///
    /// この印は `HasViolation { column: None }` にだけ一致する（どの列かが与えられていない
    /// ため、列を指定した絞り込みには一致しない。型の docs 参照）。
    #[inline]
    pub fn mark_row(&mut self, row: RowId) {
        self.rows.entry(row).or_default();
    }

    /// 行 `row` の列 `column` に違反があるとして印をつける。
    ///
    /// この印は `HasViolation { column: None }` と `HasViolation { column: Some(column) }` の
    /// **双方**に一致する。同じ行に複数の列の印をつけられる。
    #[inline]
    pub fn mark_column(&mut self, row: RowId, column: ColumnIndex) {
        self.rows.entry(row).or_default().insert(column);
    }

    /// 据え付けが 1 つも無いか（[`ViolationPresence::new`] の状態か）。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// 違反を持つと印をつけた行の数（診断のための数であり、絞り込みの判定には使わない）。
    #[inline]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// 行 `row` に**何らかの違反**があるか（列を問わない）。
    ///
    /// `HasViolation { column: None }` の判定そのものである。
    #[inline]
    pub fn has_any(&self, row: RowId) -> bool {
        self.rows.contains_key(&row)
    }

    /// 行 `row` の**列 `column`** に違反があるか。
    ///
    /// `HasViolation { column: Some(column) }` の判定そのものである。行の印（どの列かが
    /// 与えられていない違反）はここでは一致しない。
    #[inline]
    pub fn has_column(&self, row: RowId, column: ColumnIndex) -> bool {
        self.rows
            .get(&row)
            .is_some_and(|columns| columns.contains(&column))
    }
}

/// 表示の指定: 行の並びをどう導出するか。
///
/// 絞り込み（[`ViewSpec::filters`]）で**可視の集合**を選び、並べ替え（[`ViewSpec::sort`]）で
/// その集合の**順序**を定める（モジュール docs「絞り込みは積であり、順序は絞り込んだ集合に
/// 定まる」）。design.md の Service Interface が固定する 2 つの欄そのものである。
///
/// `filters` が空の並びなら絞り込みは**恒等**であり、`sort` が空の並びなら並べ替えは
/// **行わない**（文書の行順がそのまま可視の順になる。モジュール docs「基準列が 0 本のときは
/// 文書の行順を保つ」）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewSpec {
    /// 並べ替えの基準列。先頭が第一の基準であり、同値のときだけ次の基準が効く。
    pub sort: Vec<SortKey>,
    /// 絞り込みの条件。**すべてに一致する行だけが可視になる（積）**。
    pub filters: Vec<FilterSpec>,
}

/// 表示の指定を適用した結果の要約: 可視行数と、隠された行数。
///
/// 「隠された行数」を提示するのは要件 8.7 である（絞り込みによって表示されていない行の数）。
/// 隠された行数は**シートの行数と可視行数の差**として導出するので、`visible + hidden` は
/// **常に**シートの行数に一致する（[`RowOrder::recompute`]）。この恒等式は「それぞれの数を
/// 別々に計算して食い違う」実装を落とすために `tests/filter_order.rs` が検査する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewSummary {
    /// 可視行の数（[`RowOrder::len`] と同じ値）。
    pub visible: usize,
    /// 隠された行の数（絞り込みによって可視でない行の数。[`RowOrder::hidden`] と同じ値）。
    pub hidden: usize,
}

/// 可視行の順序: 絞り込みと並べ替えを適用した**あとの**行の並び。
///
/// **`Vec<RowId>`（可視行の順）、隠された行数、違反の有無の据え付けだけを持つ**
/// （design.md「RowOrder」。据え付けを加えた理由はモジュール docs「違反ありの絞り込みは
/// 据え付けられた情報だけを見る」）。物理の行の位置や値は持たない（値は [`Document`] が
/// 所有し、順序はその写像だけを持つ）。
///
/// 本型が [`Document`] を変更しないことは、[`RowOrder::recompute`] が `&Document` しか
/// 受け取らないことで型の上に現れている（要件 8.5。モジュール docs「並べ替えは表示に
/// 閉じる」）。この型のメソッドに `&mut Document` を取るものは無く、順序を物理の行へ写す
/// 読み出し（[`RowOrder::row_at`]）だけを持つ。
///
/// # 表示空間と文書空間
///
/// [`RowOrdinal`] は**可視行の序数**（この順序の何番目か）であり、[`RowId`] は行そのもので
/// ある。両者を取り違えると、絞り込みや並べ替えが効いている間に**別の行を編集する**
/// （要件 8.6）。写像は本型の [`RowOrder::row_at`] と [`RowOrder::ordinal_of`] の 1 対だけを
/// 通り、逆写像（可視の範囲から `Vec<RowId>` を作る補助）は `types` 層にもここにも置かない
/// （`types` のモジュール docs「2 つの空間を混ぜない」）。
#[derive(Debug, Clone, Default)]
pub struct RowOrder {
    /// 可視行の順（先頭が可視の 1 行目）。
    rows: Vec<RowId>,
    /// 隠された行の数（直近の [`RowOrder::recompute`] が導出した、シートの行数と可視行数の差）。
    hidden: usize,
    /// 違反の有無の据え付け（[`FilterSpec::HasViolation`] が読む。既定は空）。
    presence: ViolationPresence,
}

impl RowOrder {
    /// `doc` の `sheet` の行のうち `spec` の絞り込みに一致するものを選び、それを `spec` の
    /// 基準列で並べ替えて、可視行の順を組み立てる。
    ///
    /// **`doc` は共有参照である。**これが要件 8.5（「ドキュメントの行の並びを書き換える経路を
    /// 持たない」ことを可変参照を受け取らない形で示す）の実体であり、本メソッドが `Document` に
    /// できることは読み出しだけである。呼び出し側は呼び出しの間も呼び出しの後も
    /// ドキュメントを共有借用したままでよく、`&mut Document` を取る形なら成立しない。
    /// 絞り込みを足してもこの signature は変わらない（絞り込みは可視の集合を選ぶだけで、
    /// 何も書き換えない）。
    ///
    /// # 段取り（絞り込んでから並べ替える）
    ///
    /// 1. シートの行を**文書の並びのまま** 1 度だけ走査し、`spec.filters` の**すべて**に
    ///    一致する行だけを残す（積。要件 8.4）。絞り込みが 0 件なら全行が残る（恒等）
    /// 2. 残った行を `spec.sort` の基準列で並べ替える（要件 8.3）。**基準列が 0 本なら比較を
    ///    行わない**ため、絞り込んだ集合は文書の行順のまま残る
    ///
    /// 逆の順（並べ替えてから絞る）でも結果は同じである（比較器が全順序のため）が、本メソッド
    /// は**絞り込みを先に行う** — 捨てる行を並べ替えの比較に引き込まないためである
    /// （モジュール docs「絞り込みは積であり、順序は絞り込んだ集合に定まる」）。
    ///
    /// # 決定性
    ///
    /// 行の並びは**入力と据え付けだけから**決まる（前回の順序を引き継がない）。基準列が
    /// すべて同値の行は `RowId` の昇順で並ぶ（比較器そのものが全順序であり、`sort_by` の
    /// 安定性には依存しない。モジュール docs「決着と決定性」）。絞り込みの判定は据え付けを
    /// 読むだけであり、据え付けが同じなら結果は同じである。
    ///
    /// 値を持たない行は値なしとして扱い（比較も絞り込みも同じ規約。`value_of`）、
    /// `SheetId` が文書に無い場合は可視行 0 の順序になる。
    ///
    /// 可視行数と隠された行数は [`ViewSummary`] として返り、隠された行数は
    /// **シートの行数と可視行数の差**である（要件 8.7。したがって `visible + hidden` は
    /// 常にシートの行数に一致する）。
    pub fn recompute(&mut self, doc: &Document, sheet: SheetId, spec: &ViewSpec) -> ViewSummary {
        let rows: &[Row] = match doc.sheet_by_id(sheet) {
            Some(found) => found.rows(),
            // 文書に無いシートは行が 1 件も無いものとして扱う（モジュール docs）。
            None => &[],
        };

        self.rows.clear();
        if spec.filters.is_empty() {
            // 絞り込みが 0 件なら恒等である。行の借用を新たに取らずに全行を通す。
            if spec.sort.is_empty() {
                // 基準列が 0 本のときは**比較そのものを行わない**（比較が無いので行の同値・
                // 非同値も定まらず、決着も起こらない）。したがって文書の行順がそのまま可視の
                // 順になる。これは要件 6.1（位置を指定した挿入）と 8.6（行の増減が直ちに提示へ
                // 反映される）が要求する振る舞いでもある: 挿入された行の `RowId` は必ず最も
                // 新しい（ULID）ため、`RowId` の順に並べると指定された位置ではなく末尾に現れる。
                self.rows.extend(rows.iter().map(Row::id));
            } else {
                // 行そのものを借りたまま並べ替える（比較のたびに行を引き直さない）。
                let mut visible: Vec<&Row> = rows.iter().collect();
                visible.sort_by(|left, right| compare_rows(left, right, &spec.sort));
                self.rows.extend(visible.into_iter().map(Row::id));
            }
        } else {
            // 絞り込みを先に行う。**文書の並びのまま 1 度だけ走査**し、すべての条件に一致する
            // 行だけを残す（積。要件 8.4）。可視の順は文書の並びの部分列である。
            // 表示文字列の緩衝は 1 本だけ用意して使い回す（行ごとに確保しない）。
            let mut buffer = String::new();
            let mut visible: Vec<&Row> = Vec::new();
            for row in rows {
                if spec
                    .filters
                    .iter()
                    .all(|filter| self.filter_matches(filter, row, &mut buffer))
                {
                    visible.push(row);
                }
            }
            if !spec.sort.is_empty() {
                // 順序は**絞り込んだ集合に対して**定まる（要件 8.4, 8.7）。
                visible.sort_by(|left, right| compare_rows(left, right, &spec.sort));
            }
            self.rows.extend(visible.into_iter().map(Row::id));
        }
        // 隠された行数は**シートの行数と可視行数の差**である（可視の数を別に数えない —
        // 2 つの数が食い違う余地を残さない。要件 8.7）。
        self.hidden = rows.len() - self.rows.len();

        ViewSummary {
            visible: self.rows.len(),
            hidden: self.hidden,
        }
    }

    /// 1 本の絞り込みが行 `row` に一致するか（[`RowOrder::recompute`] の下請け）。
    ///
    /// 判定の規則は [`FilterSpec`] の型の docs と、モジュール docs「表示文字列の写しは本層が
    /// 1 つだけ持つ」が唯一の源である。`HasViolation` は**据え付けられた情報だけ**を読む。
    ///
    /// 比較のための表示文字列は `buffer`（呼び出し側が 1 本だけ用意する）へ書く。行ごとに
    /// `String` を確保すると 10 万行の絞り込みで 10 万回の確保になる（要件 11.1・11.6 の費用は
    /// 本層の走査に掛かる）。`buffer` は呼び出しのたびに空へ戻す。
    fn filter_matches(&self, filter: &FilterSpec, row: &Row, buffer: &mut String) -> bool {
        match filter {
            // 一致も部分一致も、比較するのは値ではなく**表示文字列**である（大文字小文字は
            // 区別し、10 進数は保持された文字列そのままで比較する）。
            FilterSpec::Equals { column, text } => {
                displayed(value_of(row, *column), buffer) == text.as_str()
            }
            FilterSpec::Contains { column, text } => {
                displayed(value_of(row, *column), buffer).contains(text.as_str())
            }
            // 値なしは表示文字列が空であること（= `Equals { text: "" }`）。値の数が列の添字に
            // 届かない行も、値なしとして空の表示文字列になる（`value_of`）。
            FilterSpec::IsEmpty { column } => displayed(value_of(row, *column), buffer).is_empty(),
            FilterSpec::IsNotEmpty { column } => {
                !displayed(value_of(row, *column), buffer).is_empty()
            }
            // 違反の有無は据え付けだけを見る（本層は違反を判定しない）。据え付けが空なら
            // `has_any` も `has_column` も常に偽であり、どの行にも一致しない。
            FilterSpec::HasViolation { column: None } => self.presence.has_any(row.id()),
            FilterSpec::HasViolation {
                column: Some(column),
            } => self.presence.has_column(row.id(), *column),
        }
    }

    /// 違反の有無の据え付けを置き換える（[`FilterSpec::HasViolation`] が読む情報）。
    ///
    /// **本層は違反を判定しない**（モジュール docs「違反ありの絞り込みは据え付けられた情報
    /// だけを見る」）。据え付けるのは、違反の索引を持つ側 — タスク 2.4 の `view/violations.rs`
    /// と、その結果をセッションに持たせるタスク 5.2 — である。据え付けは**丸ごと置き換わり**、
    /// 差分は積まれない（渡す側がその時点の全体を渡す）。
    ///
    /// 据え付けは順序の状態であり、[`RowOrder::recompute`] はこれを消さない（導出のたびに
    /// 据え付け直す必要はない）。既定は空であり、そのときは
    /// [`FilterSpec::HasViolation`] がどの行にも一致しない。
    #[inline]
    pub fn set_violation_presence(&mut self, presence: ViolationPresence) {
        self.presence = presence;
    }

    /// 据え付けられた違反の有無（[`RowOrder::set_violation_presence`] が入れたもの）。
    ///
    /// 診断のための読み出しであり、絞り込みは [`RowOrder::recompute`] の内側でこれを読む。
    #[inline]
    pub fn violation_presence(&self) -> &ViolationPresence {
        &self.presence
    }

    /// 可視の `ordinal` 番目の行。可視行数の外なら `None`。
    ///
    /// 表示空間から文書空間への唯一の写像の 1 つである（要件 8.6, 8.9。編集の経路は
    /// 選択された表示の位置をこのメソッドで行そのものへ写してから宛先を組み立てる）。
    #[inline]
    pub fn row_at(&self, ordinal: RowOrdinal) -> Option<RowId> {
        self.rows.get(ordinal.get()).copied()
    }

    /// 行 `row` が可視の何番目か。可視でなければ `None`。
    ///
    /// [`RowOrder::row_at`] の逆写像である（往復は `row_at(ordinal_of(row)) == row`、
    /// `ordinal_of(row_at(ordinal)) == ordinal`）。可視行の並びを先頭から走査する
    /// （design.md は序数の索引を要求しておらず、行数は 10 万行の規模である）。
    #[inline]
    pub fn ordinal_of(&self, row: RowId) -> Option<RowOrdinal> {
        self.rows
            .iter()
            .position(|candidate| *candidate == row)
            .map(RowOrdinal::new)
    }

    /// `span` が指す可視行の並び。半開区間として切り出す。
    ///
    /// 区間の座標は**可視行の序数**である（[`RowSpan`]）。可視行数の外へ出る要求は
    /// **切り落とす**（`start` が可視行数を超えるなら空、`start + count` が行数を超えるなら
    /// 末尾まで）。窓の要求は表示範囲の端で必ず短くなるため、切り落としは呼び出し側の
    /// 事前検査ではなく本メソッドの規約である（窓の符号化 5.1 と窓の記憶 7.3 が依存する）。
    #[inline]
    pub fn span(&self, span: RowSpan) -> &[RowId] {
        let start = span.start().get().min(self.rows.len());
        let end = start.saturating_add(span.count()).min(self.rows.len());
        &self.rows[start..end]
    }

    /// 可視行の数（[`ViewSummary::visible`] と同じ値）。
    #[inline]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// 可視行が 1 件も無いか。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// 隠された行の数（[`ViewSummary::hidden`] と同じ値）。
    ///
    /// **直近の [`RowOrder::recompute`] が導出した、シートの行数と可視行数の差**である
    /// （要件 8.7）。絞り込みを 1 つも指定しなければ 0 になるが、絞り込んだ行がある限り
    /// その数だけ隠れる — 「常に 0」ではない。導出の前に読むと初期値の 0 が返る。
    #[inline]
    pub fn hidden(&self) -> usize {
        self.hidden
    }
}

/// 2 つの行を `keys` の順に比べ、すべて同値なら `RowId` の昇順で決着する。
///
/// **この比較器は全順序である**（決着が必ず付く）。降順の反転は各基準列の比較の直後に行い、
/// 決着は反転しない（モジュール docs「決着と決定性」）。
///
/// 呼び出し元（[`RowOrder::recompute`]）は `keys` が 1 本以上のときだけ本関数を使う。基準列が
/// 0 本のときは比較そのものを行わない（文書の行順を保つ。モジュール docs「基準列が 0 本の
/// ときは文書の行順を保つ」）。
fn compare_rows(left: &Row, right: &Row, keys: &[SortKey]) -> Ordering {
    for key in keys {
        let ordering = compare_values(value_of(left, key.column), value_of(right, key.column));
        let ordering = match key.descending {
            true => ordering.reverse(),
            false => ordering,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    // 決着: 同値の行は `RowId` の順（design.md の Invariants）。
    left.id().cmp(&right.id())
}

/// 行の `column` 番目の値。値の数が `column` に届かない行は値なしとして扱う。
///
/// 番人の値なし 1 つを共有する（行ごと・比較ごとに複製しない。10 万行 × 複数の基準列の
/// 比較では、この 1 つを返すかどうかが比較のたびの確保に効く）。**並べ替えと絞り込みの双方が
/// この 1 つを通る**ため、「値を持たない行」の扱いが 2 つに分かれる余地が無い
/// （モジュール docs「値を持たない行」）。
fn value_of(row: &Row, column: ColumnIndex) -> &CellValue {
    /// 値を持たない行に返す値なし（すべての行・すべての比較・すべての絞り込みで共有する）。
    static ABSENT: CellValue = CellValue::Null;
    row.values().get(column.index()).unwrap_or(&ABSENT)
}

/// セル値の**表示文字列**を書くための薄い包み（[`core::fmt::Display`] 実装が規則そのもの）。
///
/// **これが表示文字列の規則の唯一の源である。**絞り込み（[`FilterSpec::Equals`] /
/// [`FilterSpec::Contains`] / [`FilterSpec::IsEmpty`]）が比較するのはこの文字列であり、
/// タスク 5.1 の `WindowCodec` も**これを使う**（写しを作らない。モジュール docs
/// 「表示文字列の写しは本層が 1 つだけ持つ」）。
///
/// [`core::fmt::Display`] にしてあるのは、**呼び出し側が用意した緩衝へ直接書ける**ためである
/// — `WindowCodec` は窓の二進形式の `Vec<u8>` へ、絞り込みは 1 本の使い回しの `String` へ
/// （[`RowOrder::recompute`]）、それぞれ `write!` 1 回で書ける。行ごとに `String` を確保すると
/// 10 万行の絞り込みで 10 万回の確保になる（要件 11.1・11.6 の費用は本層の走査に掛かる）。
/// 借用した文字列をそのまま返したい呼び出し側は [`display_text`] を使う。
///
/// # 規則（正典）
///
/// | 変種 | 表示文字列 |
/// |---|---|
/// | [`CellValue::Null`] | 空文字 |
/// | [`CellValue::Bool`] | `true` / `false` |
/// | [`CellValue::Int`] | 十進の数字列（負号をつける。`i64` の全域で桁落ちしない） |
/// | [`CellValue::Float`] | 最短の往復可能な十進表記。**指数表記を使わない**。`-0.0` は `0` |
/// | [`CellValue::Decimal`] | **保持された文字列そのまま**（正規化しない） |
/// | [`CellValue::Text`] | そのもの |
/// | [`CellValue::Nested`] | 最上位の要素数の要約（オブジェクトは `N項目`、配列は `N要素`） |
/// | [`CellValue::Attachment`] | 正準の小文字 hex（64 文字） |
///
/// 浮動小数は [`format!`] の `{}`（最短の往復可能な十進表記）で書く。`{:?}` は `1e21` の
/// ような指数表記を出すため使わない（モジュール docs「指数表記を使わない」の理由）。
/// **非数と無限**（`NaN` / `inf` / `-inf`）は `{}` がそのまま `NaN` / `inf` / `-inf` と書く
/// （保存の門が非有限を拒否するため、本来は現れない。復号の門を迂回した値でも表示が
/// 破綻しないことを優先し、ここで別の表記へ写さない — 写すと、表示と絞り込みの一致という
/// 契約が、どの値に対しても成り立つかを場合分けで確かめる必要が生じる）。
/// `-0.0` は `CellValue::float` と同じく `0` へ畳む（復号の門を迂回した値が負のゼロを
/// 持ちうるため。`CellValue` の `PartialEq` も `-0.0 == 0.0` である）。
///
/// 入れ子の要約は**最上位の要素数だけ**であり、中身を含めない（要件 5.6。構造そのものは
/// 詳細表示が別途取りに行く。design.md「Data Models / 窓の二進形式」）。オブジェクトを
/// `N項目`、配列を `N要素` と書き分けるのは、`{}` の入れ子と `[]` の入れ子を人が区別できる
/// ようにするためである。
///
/// 新しい変種が `CellValue` に足された場合、この `match` が網羅でなくなるため**コンパイルが
/// 止まる**（表示文字列の規則を書かずに素通りすることはない）。
#[derive(Debug, Clone, Copy)]
pub struct DisplayText<'a>(pub &'a CellValue);

impl core::fmt::Display for DisplayText<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            // 値なしは何も見えない。
            CellValue::Null => Ok(()),
            CellValue::Bool(value) => f.write_str(if *value { "true" } else { "false" }),
            // 整数は `i64` の全域で桁落ちしない（`Display` が十進で書く）。
            CellValue::Int(value) => write!(f, "{value}"),
            CellValue::Float(value) => {
                // `-0.0` を `0.0` へ畳んでから書く（`CellValue::float` と `PartialEq` に合わせる）。
                let folded = if *value == 0.0 { 0.0 } else { *value };
                write!(f, "{folded}")
            }
            // 10 進数は**保持された文字列そのまま**（正規化すると逐語で往復する契約が壊れる）。
            CellValue::Decimal(value) => f.write_str(value),
            CellValue::Text(value) => f.write_str(value),
            CellValue::Nested(NestedValue::Object(entries)) => write!(f, "{}項目", entries.len()),
            CellValue::Nested(NestedValue::Array(items)) => write!(f, "{}要素", items.len()),
            // 添付は正準の小文字 hex（64 文字。内容から決まる識別子そのもの）。
            CellValue::Attachment(value) => write!(f, "{value}"),
        }
    }
}

/// セル値の**表示文字列**を取り出す（規則は [`DisplayText`] が唯一の源）。
///
/// 保持している文字列をそのまま出せる変種（`Text` / `Decimal`）は**借用で返し**、それ以外は
/// [`DisplayText`] で書式化した文字列を所有で返す。確保が要らない変種で確保を起こさないためで
/// あり、**規則そのものは [`DisplayText`] の 1 箇所にしかない**（この関数はどの変種を借用で
/// 返すかを選ぶだけである。`Text` / `Decimal` の規則も [`DisplayText`] の腕が定める値と
/// 同じであり、両者が食い違わないことはテストが固定する）。
///
/// 呼び出し側が自分の緩衝へ直接書けるなら [`DisplayText`] を使う方が安い（行ごとに
/// `String` を確保しない。タスク 5.1 の `WindowCodec` と [`RowOrder::recompute`] はそうする）。
#[inline]
pub fn display_text(value: &CellValue) -> Cow<'_, str> {
    match value {
        CellValue::Text(text) => Cow::Borrowed(text),
        CellValue::Decimal(text) => Cow::Borrowed(text),
        other => Cow::Owned(DisplayText(other).to_string()),
    }
}

/// [`display_text`] と違い、書式化が要る変種だけを `buffer` へ書き、`Text` / `Decimal` は
/// **借用したまま返す**（10 万行の走査で行ごとの `String` 確保を起こさない）。
///
/// **規則は [`DisplayText`] の 1 箇所にしかない**（ここは書く先を選ぶだけである）。
#[inline]
fn displayed<'buf>(value: &'buf CellValue, buffer: &'buf mut String) -> Cow<'buf, str> {
    match value {
        CellValue::Text(text) => Cow::Borrowed(text),
        CellValue::Decimal(text) => Cow::Borrowed(text),
        other => {
            buffer.clear();
            // `fmt::Write` は `String` への書き込みで失敗しない。
            let _ = write!(buffer, "{}", DisplayText(other));
            Cow::Borrowed(buffer.as_str())
        }
    }
}

/// 2 つのセル値を、変種ごとの順序で比べる（変種をまたぐときは順位で決める）。
///
/// 同じ変種の対はその変種の規則（モジュール docs「変種の順位（正典）」の表）で比べ、変種が
/// 違う対は順位（[`VariantRank`]）で比べる。**変種をまたいで数値として比較しない**
/// （`Int(5)` と `Float(5.0)` は順位で決まる。モジュール docs「変種をまたぐ数値の比較は
/// しない」）。
///
/// 新しい変種が `CellValue` に足された場合、[`variant_rank`] の `match` が網羅でなくなるため
/// **コンパイルが止まる**（比較の規則を書かずに素通りすることはない）。
fn compare_values(left: &CellValue, right: &CellValue) -> Ordering {
    match (left, right) {
        (CellValue::Null, CellValue::Null) => Ordering::Equal,
        (CellValue::Bool(left), CellValue::Bool(right)) => left.cmp(right),
        (CellValue::Int(left), CellValue::Int(right)) => left.cmp(right),
        (CellValue::Float(left), CellValue::Float(right)) => compare_floats(*left, *right),
        (CellValue::Decimal(left), CellValue::Decimal(right)) => compare_decimals(left, right),
        (CellValue::Text(left), CellValue::Text(right)) => left.cmp(right),
        (CellValue::Nested(left), CellValue::Nested(right)) => compare_nested(left, right),
        (CellValue::Attachment(left), CellValue::Attachment(right)) => left.cmp(right),
        // 変種が違う: 順位で決める（モジュール docs「変種の順位（正典）」）。
        _ => variant_rank(left).cmp(&variant_rank(right)),
    }
}

/// 変種の順位（モジュール docs「変種の順位（正典）」が唯一の源）。
///
/// **宣言の順がそのまま順位である**（導出した `Ord` が宣言順を写す）。数を直に書かないのは、
/// 順位の表とコードが食い違わないようにするためである（表に行を足してここを直し忘れる、と
/// いう食い違いが起こりえない）。新しい変種が `CellValue` に足されれば、[`variant_rank`] の
/// `match` が網羅でなくなるためコンパイルが止まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum VariantRank {
    /// 値なし（最も小さい。値なしに大小は無い）。
    Null,
    /// 真偽（`false < true`）。
    Bool,
    /// 64 ビット整数。
    Int,
    /// 浮動小数。
    Float,
    /// 10 進数（値として比較する）。
    Decimal,
    /// テキスト。
    Text,
    /// 入れ子（オブジェクト / 配列）。
    Nested,
    /// 添付参照（最も大きい）。
    Attachment,
}

/// 値の変種の順位。
const fn variant_rank(value: &CellValue) -> VariantRank {
    match value {
        CellValue::Null => VariantRank::Null,
        CellValue::Bool(_) => VariantRank::Bool,
        CellValue::Int(_) => VariantRank::Int,
        CellValue::Float(_) => VariantRank::Float,
        CellValue::Decimal(_) => VariantRank::Decimal,
        CellValue::Text(_) => VariantRank::Text,
        CellValue::Nested(_) => VariantRank::Nested,
        CellValue::Attachment(_) => VariantRank::Attachment,
    }
}

/// 浮動小数を数値として比べる（`-0.0` を `0.0` へ畳んでから `f64::total_cmp`）。
///
/// 畳む理由と非数の扱いはモジュール docs「変種の順位（正典）」の「浮動小数」を参照。
fn compare_floats(left: f64, right: f64) -> Ordering {
    /// `-0.0` を `0.0` へ畳む（`PartialEq` が `-0.0 == 0.0` であることに合わせる）。
    fn fold_zero(value: f64) -> f64 {
        if value == 0.0 {
            0.0
        } else {
            value
        }
    }
    fold_zero(left).total_cmp(&fold_zero(right))
}

/// 10 進数を値として比べる（モジュール docs「10 進数は数値として比較する」）。
///
/// 文法に一致する中身は上流の正準形（`schema_engine::types::decimal::canonicalize`）へ畳んで
/// から比べる（**文法も桁勘定も本モジュールで書き直さない**）。文法に一致しない中身は
/// 文法に一致する値の後ろに置き、文法外どうしはバイト列の辞書順で比べる。
fn compare_decimals(left: &str, right: &str) -> Ordering {
    match (decimal::canonicalize(left), decimal::canonicalize(right)) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.cmp(right),
    }
}

/// 入れ子の値を構造で比べる（オブジェクト < 配列。規則はモジュール docs）。
fn compare_nested(left: &NestedValue, right: &NestedValue) -> Ordering {
    match (left, right) {
        (NestedValue::Object(left), NestedValue::Object(right)) => compare_entries(left, right),
        (NestedValue::Array(left), NestedValue::Array(right)) => compare_items(left, right),
        (NestedValue::Object(_), NestedValue::Array(_)) => Ordering::Less,
        (NestedValue::Array(_), NestedValue::Object(_)) => Ordering::Greater,
    }
}

/// オブジェクトのエントリ列を、キー→値の順の辞書式で比べる（前置は短い側が先）。
fn compare_entries(left: &[(String, CellValue)], right: &[(String, CellValue)]) -> Ordering {
    let common = left.len().min(right.len());
    for index in 0..common {
        let (left_key, left_value) = &left[index];
        let (right_key, right_value) = &right[index];
        let ordering = left_key
            .cmp(right_key)
            .then_with(|| compare_values(left_value, right_value));
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

/// 配列の要素列を辞書式で比べる（前置は短い側が先）。
fn compare_items(left: &[CellValue], right: &[CellValue]) -> Ordering {
    let common = left.len().min(right.len());
    for index in 0..common {
        let ordering = compare_values(&left[index], &right[index]);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

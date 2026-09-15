//! 窓の二進形式: 可視範囲の行を、境界を越えられる形へ符号化する [`WindowCodec`]
//! （タスク 5.1。design.md の Component 表の `WindowCodec`。要件 1.1, 1.2, 4.5, 11.2, 11.6）。
//!
//! # 層の鎖
//!
//! `error / types → view → edit → history → transport → api`。**本層は左の `types` と `view`
//! だけを参照する**（design.md「内部の依存の向き」。層の鎖の文言を各層の冒頭に置く規約は
//! `structure.md`「ドメインクレートの内部構造」）。上流には `document-format` の
//! [`CellValue`] / [`RowId`] と、`schema-engine` の経路の型（[`ValuePath`] /
//! [`ValuePathSegment`]）を求めるだけであり、**判定は呼ばない**（値の正否は
//! `schema-engine` の所有である）。
//!
//! # 何を運び、何を運ばないか
//!
//! design.md「Data Models / 窓の二進形式」が定めるのは次の 3 点である。
//!
//! | 運ぶ | 運ばない |
//! |---|---|
//! | 行の識別子（生の 16 バイト） | 64 ビット整数と 10 進数を**数値として**（表示文字列として運ぶ） |
//! | セルの表示文字列・変種の札・違反の札 | 入れ子の**構造そのもの**（要約と、違反している内側の位置だけを運ぶ） |
//! | 版・世代・開始序数・行数・列数 | 違反の**理由**（4.2 は `GridViolationResponse.reason` が運ぶ） |
//!
//! **行の値の一部は運ばれない**: 入れ子のセルは要約（要素数）だけを運ぶ。構造そのものは
//! 詳細表示（5.5 / 8.5）が別途取りに行く — `NestedInspector`（7.3 の経路）の要求であり、
//! 本層の範囲ではない。
//!
//! **行に属さない違反（列そのものの問題）は窓に載らない**。窓は行の区間に対する窓であり、
//! 載せる行が無い違反を置く場所が無い（`view/violations.rs` の
//! [`ViolationIndex::column_violations`]）。4.3 の総数は索引が別に答える（5.2 が読む）。
//!
//! # バイト配置（版 1。**エンディアンはすべてリトルエンディアン**）
//!
//! ```text
//! 窓 = 頭 || 行 × 行数
//!
//! 頭（33 バイト。すべて u64 リトルエンディアン）:
//!   0       版        u8      = WINDOW_FORMAT_VERSION
//!   1..9    世代      u64
//!   9..17   開始序数  u64     可視行の序数（文書の位置ではない）
//!   17..25  行数      u64     この窓が運ぶ行の数
//!   25..33  列数      u64     1 行が運ぶセルの数（宣言の列数）
//!
//! 行（行数ぶん）:
//!   RowId の生 16 バイト（ULID。u128 のビッグエンディアン = `Ulid::to_bytes` そのもの）
//!   || セル × 列数
//!
//! セル:
//!   変種の札   u8      0..=7（下の表）
//!   違反の有無 u8      0 = 違反なし / 1 = 違反あり
//!   [違反の有無が 1 のときだけ続く 違反の札]
//!       札の数     u64
//!       札ごと:   段の数 u64 || 段 × 段の数
//!       段:       種類 u8（0 = Field / 1 = Index）
//!                 || Field なら 名前の長さ u64 || UTF-8 の名前
//!                 || Index なら 添字 u64
//!   表示文字列の長さ u64      UTF-8 のバイト長
//!   表示文字列の本体 UTF-8
//! ```
//!
//! ## なぜ数の欄がすべて 64 ビットなのか
//!
//! 符号化の側に**「収まらない」という経路を作らない**ためである。`usize` は 64 ビット以下で
//! あり、`u64` への変換は常に正確である — 幅を 32 ビットにすると「4 GiB を超える表示文字列」
//! のような**値の側から決まる**場面で切り詰めが起こりうる。切り詰めは静かな破損
//! （窓が壊れているのに長さだけが合う）になるため、幅を広げる方を選ぶ。復号の側だけが
//! [`WindowDecodeError::TooLarge`] を持ち、32 ビットのホストで 64 ビットの窓を読む場合に
//! それを返す（設計が禁じているのは**値**を数値として出すことであり、長さの欄は値ではない）。
//!
//! ## 違反の札の位置（design.md の表からの拡張）
//!
//! design.md の表は「変種の札（1 バイト）・違反の有無（1 バイト）・表示文字列の長さ・UTF-8 の
//! 本体」までを定め、**入れ子の内側の位置をどう運ぶかは開いたまま**である（タスク 5.1 の
//! 「内側のどの位置が違反しているかは札として運ぶ」がそれを要求する）。本層は
//! **違反の有無のバイトの直後**に札の塊を置く（design の表の相対順 — 札 → 有無 → 長さ →
//! 本体 — はそのまま保たれる）。有無のバイトだけでは読めない情報を、そのバイトが
//! **続きの有無**を決める位置に置くことで、前方 1 回の走査が閉じる。
//!
//! 札は**畳まない**（索引が保つ件数と、窓が運ぶ件数が食い違わない）。費用は窓のセルの
//! 違反の件数に比例するが、その件数は索引が報告から受けた保持の分であり
//! （`schema-engine` の `ValidationOptions` が上限を与える）、本層は取捨を行わない —
//! **要件 4.5 が名指す情報を黙って落とさない**。
//!
//! # 変種の札（wire ABI。**表を動かさない**）
//!
//! | 値 | 変種 | 値 | 変種 |
//! |---|---|---|---|
//! | 0 | [`CellValue::Null`] | 4 | [`CellValue::Decimal`] |
//! | 1 | [`CellValue::Bool`] | 5 | [`CellValue::Text`] |
//! | 2 | [`CellValue::Int`] | 6 | [`CellValue::Nested`] |
//! | 3 | [`CellValue::Float`] | 7 | [`CellValue::Attachment`] |
//!
//! フロントエンドはこの札で入力手段を選び（7.4 が 6.1 の生成物を通して読む型の札と対応する）、
//! 6.1 は型の札をこの並びから作る。したがって**この表は wire ABI であり、並べ替えない**
//! （並べ替えは、フロントエンドが別の入力手段を開く変更である）。値を足すときは末尾へ足す。
//! 未知の値は復号が [`WindowDecodeError::UnknownTag`] で拒む。
//!
//! [`variant_tag`] の `match` は**網羅であり、wildcard を持たない** — `CellValue` に変種が
//! 足されればコンパイルが止まる（札を書かずに素通りすることはない）。
//!
//! # 表示文字列は `view` 層の 1 つを再利用する
//!
//! セルの表示文字列は [`display_text`] / [`DisplayText`] が**唯一の源**である（2.2 が定め、その
//! モジュール docs が「タスク 5.1 の `WindowCodec` もこれを使う」と名指す）。本層は
//! **2 つ目の写しを作らない** — 写しを作ると、利用者が画面で見た文字列で絞り込んだのに
//! 窓が別の文字列を運ぶ状態になる。
//!
//! とくに次の 3 点は `view` の規則がそのまま本層の要件である:
//!
//! - `Int` は十進の数字列（[`i64`] の全域で桁落ちしない）
//! - `Decimal` は**保持された文字列そのまま**（正規化しない）
//! - `Nested` は**最上位の要素数の要約**（オブジェクトは `N項目`、配列は `N要素`）—
//!   これが要件 5.6 の要約であり、入れ子の構造が窓に載らないことの実体である
//!
//! 書式化は**呼び出し側の緩衝へ直接行う**（[`DisplayText`] を
//! `write!` で窓の [`Vec<u8>`] へ書き、長さの欄は書いた後に埋める）。`Text` / `Decimal` の
//! ように借用で済む変種でも、整形が要る変種でも**セルごとの [`String`] を確保しない**
//! （窓は走査のたびに作られる。要件 11.1）。
//!
//! # 世代（design.md の Idempotency 句）
//!
//! **世代**は、窓を組み立てた文書・表示の状態を識別する単調増加の数である。同じ世代・
//! 同じ区間の要求は**常に同じ結果**を返し（冪等）、**世代が一致しない要求は空の窓**を返す。
//!
//! | 誰が | 何をするか |
//! |---|---|
//! | 5.2 の `GridSession` | 表示が変わったときに世代を進める（[`WindowCodec::set_generation`]） |
//! | 6.3 のコマンド | 要求の頭から世代を読み、[`WindowCodec::is_stale`] を見て空の窓を失敗と同じに扱う |
//! | 7.3 の窓の記憶 | 古い世代の応答を捨てる |
//!
//! **どちらが世代を比べるかを本層は決めない**（design.md は「呼び出し側が再要求する」と
//! だけ定める）。本層は比較の口（[`WindowCodec::is_stale`]）と、空の窓を返す規則を与える。
//! 注意: **一致しない世代はすべて「古い」として扱う**（現在より新しい世代を名乗る要求も
//! 窓を作らない）。窓はつねに**現在の世代のもの**であり、違う世代を名乗る要求へ窓を返すと、
//! フロントエンドが別の世代の窓として受け取る — design の文面（「世代が**古い**要求」）を、
//! 安全側の一般化として「一致しない要求」へ広げた（本層が決めた点。モジュール docs
//! 「design.md が開いたままにした点」）。
//!
//! # 空の窓の表現
//!
//! **空の窓は長さ 0 のバイト列**である（[`EMPTY_WINDOW`]）。design.md の Batch 契約が
//! 「失敗は空の窓で表す」（6.3。生バイト経路は封筒を運べない）と定めるため、**失敗と
//! 世代違いが同じ表現**になる。
//!
//! これは**行 0 の窓**（頭だけを持つ [`HEADER_LEN`] バイト。開始序数が可視行数に等しい
//! 要求、または可視行が 0 のシート）と**区別できる** — フロントエンドは「端に達した」と
//! 「要求が通らなかった（読み込み中のまま再試行する）」を別に扱えなければならない
//! （design.md「Error Categories and Responses」）。したがって本層は、
//! **失敗・世代違い以外では空の窓を返さない**。
//!
//! # 範囲の検査
//!
//! design.md の誤り表は「範囲外の窓の要求」を `GridError` として封筒の失敗腕で返し、
//! **画面は再要求する**とする。本層は次の 3 つを区別する。
//!
//! | 要求 | 結果 | 根拠 |
//! |---|---|---|
//! | 開始序数が可視行数**より後ろ** | [`GridError::SpanOutOfRange`] | 表示と要求が食い違っている（その序数は存在しない） |
//! | 開始序数が可視行数**に等しい**（末尾の直後） | **行 0 の窓** | 端に達しただけで誤りではない（切り落としの帰結） |
//! | 終端が可視行を越える | **末尾まで切り落とす** | [`RowOrder::span`] の規約（窓の要求は表示範囲の端で必ず短くなる） |
//!
//! 世代の不一致は**範囲の検査より先**に見る（古い世代の要求は、その世代の表示では妥当だった
//! 範囲を運びうる。範囲の誤りとして返すと、呼び出し側が「再要求」ではなく「誤り」として
//! 扱う）。**誤りを返す経路は文書に触れない** — 符号化はそもそも [`Document`] を
//! 受け取らない（費用の形は後述）。
//!
//! # 前方 1 回の走査で復号できる（不変条件）
//!
//! **不変条件: 窓の各欄は、それより前のバイトだけを読んで解釈できる。**後方参照は無く、
//! 長さの欄は運ばれる本体の**直前**にある。したがって [`decode_window`] は 1 本の前方向の
//! カーソルで 1 回だけ走査する。
//!
//! この不変条件の裏側を復号が検査する:
//!
//! - **途中で切れた入力は必ず拒まれる** — 頭が宣言する行数を読み終える前に尽きれば長さが
//!   足りない。したがって**完全な窓のすべての真の接頭辞**は [`WindowDecodeError`] になる
//!   （`tests/window_codec.rs` が接頭辞のすべてを走査して固定する）
//! - **余分なバイトも拒まれる** — 行数を読み終えた位置が入力の終端でなければ
//!   [`WindowDecodeError::TrailingBytes`]（余りを黙って捨てると、壊れた窓が正常に見える）
//! - **宣言された数は信用しない** — 行・札・段の確保は**読み進めながら**行う
//!   （`Vec::with_capacity(宣言された数)` をすると、壊れた窓 1 つで巨大な確保が走る）
//!
//! ## 1 回の走査の何が検査で固定され、何が固定されないか（正直な限界）
//!
//! **接頭辞の検査だけでは固定されない。**接頭辞の拒否は「後ろを読む欄が**無い**」ことの
//! 証拠にはならない — 接頭辞の長さが `>= 8` の位置を指す後方参照は、その接頭辞の中で
//! 完結しうる。実測: 末尾のバイトを読んで段の種類を決める変異（接頭辞の中で完結する形）を
//! 入れると、`an_unknown_version_and_a_truncated_window_are_rejected_without_panicking`
//! **だけ**を走らせた場合は通る（その 1 件は接頭辞の拒否だけを見ているためである）。
//!
//! **しかし他の検査と組にすると落ちる。**同じ変異は `tests/window_codec.rs` の**4 件**を
//! 落とす（`an_unknown_tag_flag_or_segment_kind_is_rejected`・
//! `the_documented_byte_layout_decodes_into_the_documented_fields`・
//! `a_violated_nested_cell_carries_the_inner_position_mark`・
//! `a_window_round_trips_forward_in_a_single_pass`。**この 4 件はこの変異の形に対する実測**で
//! あり、一般的な後方参照が落とす集合ではない — どの検査が落ちるかは変異の形に依る）。
//! 落ちる理由は、欄の解釈が**その欄の接頭辞の外**のバイトに依るため、符号化した位置と、
//! 検査が独立に導く値・位置との対応が崩れることである（とくに手で組み立てた配置の検査と、
//! 索引から導く札の検査が、独立な期待値を持っているため効く）。
//!
//! したがって本層が言えるのは次である: **不変条件そのものは実装の形が担い**
//! （カーソルが唯一の読み取り口であり、後戻りする経路を持たない）、**その帰結は検査が
//! 捉える**（接頭辞の拒否と、位置・値の一致を独立な期待値で見る複数の検査の組）。
//! 「時間で測る」ことはしない（`verification.md`）。
//!
//! # 入れ子の違反の位置は、上流の経路を 1 つの写しで往復する
//!
//! 内側の位置は [`NestedPath`]（`types` 層）である。復号は生のバイトから
//! [`ValuePath`] を組み立ててから **`NestedPath` の正規の変換（`From<&ValuePath>`）** を通す
//! — 経路の表現の写しは `types` 層の 1 つに閉じており、本層は解析器も再導出も持たない。
//!
//! # 費用の形（要件 11.2。**速度ではなく呼び出しの形で示す**）
//!
//! `verification.md`「速度を証拠にしない。証拠は『呼び出しの形』で取る」に従い、本層の費用は
//! 構造で示す。
//!
//! - [`WindowCodec::encode`] は **[`Document`] を一切受け取らない** — 10 万行を走査する経路が
//!   型の上に存在しない。受け取るのは表示の順序（[`RowOrder`]）・違反の索引
//!   （[`ViolationIndex`]）・列数・[`WindowRowSource`]・要求だけである
//! - 行は [`RowOrder::span`] が返す**切片**から取る。切片の長さは高々要求の行数である
//! - 行の値は [`WindowRowSource`] へ**1 行につき 1 回**だけ問い合わせる
//!   （`tests/window_codec.rs` が回数を数えて固定する）
//!
//! **したがって費用は窓の行数と、窓のセルの違反の件数に比例**し、シートの行数に依らない。
//! 行の値の引き方は呼び出し側が決める（5.2 の `GridSession` は 1 回の呼び出しの間だけ
//! 行の索引を保つ側であり、本層はその索引の形を定めない）。
//!
//! ## この主張の何が検査で固定され、何が固定されないか（正直な限界）
//!
//! **固定されるもの**: ① 符号化が [`Document`] を型の上で受け取らないこと（コンパイル時に
//! 決まる）、② [`WindowRowSource`] への問い合わせが**ちょうど窓の行数**であり、**窓の外の
//! 行は 1 度も引かれない**こと（`tests/window_codec.rs` が、窓の外を引くと panic する
//! source を差し込んで固定する）、③ 窓の行数が [`RowOrder::span`] の切片の長さであり、
//! 要求の行数を超えないこと。
//!
//! **固定されないもの**: [`RowOrder::span`] の**内部の費用**である。観測の縫い目は
//! [`WindowRowSource`] にしか無く、`RowOrder` に縫い目を足すのは本タスクの境界の外である
//! （足せば「本番が呼んでいない模擬」を数える危険も生じる）。したがって
//! **可視の並びを丸ごと走査してから窓の行だけを拾う実装は、この検査では区別できない**
//! （値の問い合わせは窓の行だけになるためである）。区別できるのは「費用がシートの行数に
//! **比例しない**」という構造の主張までであり、それを支えるのは上の①②③ — とくに
//! ①（型の上に走査の経路が無い）と②（窓の外を引かない）である。②が意味を持つのは、
//! 行の値の取得が**シートの走査を伴いうる**ためである（本番の 5.2 は索引を持つが、
//! 本テストの `SheetSource` は線形に走査する）。
//!
//! # design.md が開いたままにした点（本層が決めたもの）
//!
//! | 開いていた点 | 本層の決定 |
//! |---|---|
//! | 入れ子の内側の位置の encoding | 違反の有無の直後に「札の数 + 段 +（種類と本体）」を置く。畳まない |
//! | 空の窓の表現 | 長さ 0 のバイト列。行 0 の窓（頭だけ）と区別する |
//! | 世代の比較を誰がするか | **呼び出し側**。本層は [`WindowCodec::is_stale`] と空の窓を返す規則を与える |
//! | 一致しない世代の扱い | 一致しない世代は**すべて**空の窓（「古い」を安全側に一般化） |
//! | 範囲外の要求 | 開始序数が可視行数より後ろなら [`GridError::SpanOutOfRange`]、端に掛かる要求は切り落とす |
//! | 数の欄の幅 | すべて `u64`（符号化に「収まらない」経路を作らない） |
//! | 列数より長い行 | 宣言の列数までを運び、余り（表示に列が無い値）は運ばない |
//! | 版の数値と配置 | [`WINDOW_FORMAT_VERSION`] = 1、頭の幅は [`HEADER_LEN`] = 33 バイト |
//!
//! # 本モジュールが持たないもの
//!
//! - **要求の頭の復号**（生バイト経路の引数。シート・開始序数・行数・世代の読み取り）は
//!   6.3 の適応層が行う。本層は [`WindowRequest`] を**組み立て済みの値**として受け取る
//! - **窓の記憶と先読み**（7.3）、**世代を進める側**（5.2）、**フロントエンドの復号**（7.3）
//! - **違反の理由**（4.2 は封筒つきの `GridViolationResponse.reason` が運ぶ）
//! - 行に属さない違反（列そのものの問題）と違反の総数（4.3 は
//!   [`ViolationIndex::violation_total`] が答える）
//!
//! モジュール docs が参照するが、本モジュールの `use` に無い名前の宛先（`view/violations.rs`
//! と同じ流儀 — 名前を `use` すると、docs だけの利用が未使用の import になる）。
//!
//! [`Document`]: document_format::Document
//! [`ValuePathSegment`]: schema_engine::ValuePathSegment
//! [`ColumnLayout`]: crate::view::ColumnLayout
//! [`display_text`]: crate::view::display_text

use core::fmt;
use std::io::Write as _;

use document_format::{CellValue, RowId};
use schema_engine::ValuePath;

use crate::error::GridError;
use crate::types::{ColumnIndex, NestedPath, NestedPathSegment, RowOrdinal, RowSpan};
use crate::view::{DisplayText, RowOrder, ViolationIndex};

/// 窓の二進形式の版（頭の 1 バイト目）。
///
/// **この数値は wire ABI であり、動かさない。**フロントエンドは版を見て、知らない版の窓を
/// 拒むか適応する。版を上げるのは配置の意味が変わる変更であり、互換の要否は 6.3 と 7.3 の
/// 判断である（本層は版を書くだけで、古い版を読む経路を持たない）。
pub const WINDOW_FORMAT_VERSION: u8 = 1;

/// 頭の幅（バイト）: 版 1 + 世代 8 + 開始序数 8 + 行数 8 + 列数 8。
///
/// [`decode_window`] はこの幅を先に読み、それより短い入力を行 0 の窓として扱わない
/// （[`WindowDecodeError::Truncated`]）。
pub const HEADER_LEN: usize = 33;

/// 行の識別子の生バイト長（ULID の 128 ビット）。
///
/// 窓は [`RowId`] を**この長さの生バイトのまま**運ぶ（design.md「Data Models / 窓の二進形式」）。
/// フロントエンドは**不透明な鍵**として扱い、数値へ変換しない。
pub const ROW_KEY_LEN: usize = 16;

/// 空の窓の表現（**長さ 0 のバイト列**）。
///
/// design.md の Batch 契約は「世代が古い要求は空の窓を返し、呼び出し側が再要求する」と定め、
/// 6.3 は「失敗と世代違いを空の窓で表す」を要件にする（生バイト経路は封筒を運べない）。
/// したがって空の窓は**失敗の表現でもある**。行 0 の窓（頭だけを持つ [`HEADER_LEN`] バイト）と
/// 区別できることが要点である（モジュール docs「空の窓の表現」）。
pub const EMPTY_WINDOW: &[u8] = &[];

/// 違反の有無のバイト: 違反なし。
const VIOLATED_NO: u8 = 0;

/// 違反の有無のバイト: 違反あり（直後に違反の札が続く）。
const VIOLATED_YES: u8 = 1;

/// 内側の位置の段の種類: オブジェクトのフィールド名。
const SEGMENT_FIELD: u8 = 0;

/// 内側の位置の段の種類: 配列の 0 起点の添字。
const SEGMENT_INDEX: u8 = 1;

/// 窓を組み立てた**文書と表示の状態**を識別する単調増加の数（design.md の Idempotency 句）。
///
/// 世代が同じである限り、同じ区間の要求は同じ窓になる。表示が変わったら 5.2 が
/// [`Generation::next`]（または [`WindowCodec::set_generation`]）で進める。
///
/// **単調増加であることだけが契約である**（連番であること・飛ばさないことは要求しない。
/// 5.2 が編集のたびに進めても、表示の指定が変わったときにだけ進めてもよい）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(u64);

impl Generation {
    /// 世代の最初の値（新しい `WindowCodec` が持つ値）。
    pub const FIRST: Self = Self(0);

    /// 世代を包む。
    ///
    /// 値の意味は呼び出し側が決める。**本層は一致するかどうかだけを見る**
    /// （[`WindowCodec::is_stale`] は「要求の世代が現在の世代と異なる」を古いとみなす —
    /// 大小では見ない。窓はつねに現在の世代のものであり、**現在より新しい世代を名乗る要求**も
    /// 同じく空の窓で答えるのが安全側である）。
    #[inline]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 世代の数値。
    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// 次の世代（`u64` の飽和。10 万行の編集を続けても尽きない）。
    #[inline]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for Generation {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// セルの変種の札（**1 バイト。wire ABI**。表はモジュール docs が唯一の源）。
///
/// フロントエンドはこの札で型を判別し、入力手段を選ぶ（7.4 が 6.1 の生成物を通して読む）。
/// したがって**並べ替えも詰め直しもしない** — 値を足すときは末尾へ足す。
///
/// 未知の値（将来の版が運びうる値、壊れた入力）は [`VariantTag::from_byte`] が `None` を
/// 返し、[`decode_window`] が拒む。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariantTag(u8);

impl VariantTag {
    /// 値なし（[`CellValue::Null`]。表示文字列は空）。
    pub const NULL: Self = Self(0);
    /// 真偽（[`CellValue::Bool`]）。
    pub const BOOL: Self = Self(1);
    /// 64 ビット整数（[`CellValue::Int`]。**数値ではなく十進の文字列として運ぶ**）。
    pub const INT: Self = Self(2);
    /// 浮動小数（[`CellValue::Float`]）。
    pub const FLOAT: Self = Self(3);
    /// 10 進数（[`CellValue::Decimal`]。**桁の文字列そのままを運ぶ**）。
    pub const DECIMAL: Self = Self(4);
    /// テキスト（[`CellValue::Text`]）。
    pub const TEXT: Self = Self(5);
    /// 入れ子（[`CellValue::Nested`]。表示文字列は要素数の要約であり、構造は運ばない）。
    pub const NESTED: Self = Self(6);
    /// 添付参照（[`CellValue::Attachment`]。表示文字列は正準の小文字 hex）。
    pub const ATTACHMENT: Self = Self(7);

    /// 札の並び（宣言の順 = 値の順。表の検査が写しを作らずに走査できるようにする）。
    pub const ALL: [Self; 8] = [
        Self::NULL,
        Self::BOOL,
        Self::INT,
        Self::FLOAT,
        Self::DECIMAL,
        Self::TEXT,
        Self::NESTED,
        Self::ATTACHMENT,
    ];

    /// バイトから札へ戻す（表に無い値は `None`）。
    ///
    /// **表を 1 箇所に閉じる**ための入口である。復号はこれを通すので、未知の値が
    /// 「表に無い変種」として素通りすることはない。
    #[inline]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::NULL),
            1 => Some(Self::BOOL),
            2 => Some(Self::INT),
            3 => Some(Self::FLOAT),
            4 => Some(Self::DECIMAL),
            5 => Some(Self::TEXT),
            6 => Some(Self::NESTED),
            7 => Some(Self::ATTACHMENT),
            _ => None,
        }
    }

    /// 札のバイト（wire に出る値そのもの）。
    #[inline]
    pub const fn byte(self) -> u8 {
        self.0
    }
}

/// セル値の変種の札（[`VariantTag::ALL`] の表が唯一の源）。
///
/// **この `match` は網羅であり、wildcard を持たない** — [`CellValue`] に変種が足されれば
/// コンパイルが止まる（札の表を直さずに素通りすることはない）。`view` 層の
/// [`DisplayText`] と `variant_rank` が同じ規律で書かれている。
#[inline]
#[must_use]
pub const fn variant_tag(value: &CellValue) -> VariantTag {
    match value {
        CellValue::Null => VariantTag::NULL,
        CellValue::Bool(_) => VariantTag::BOOL,
        CellValue::Int(_) => VariantTag::INT,
        CellValue::Float(_) => VariantTag::FLOAT,
        CellValue::Decimal(_) => VariantTag::DECIMAL,
        CellValue::Text(_) => VariantTag::TEXT,
        CellValue::Nested(_) => VariantTag::NESTED,
        CellValue::Attachment(_) => VariantTag::ATTACHMENT,
    }
}

/// 窓の要求: 組み立て済みの**世代**と**可視行の区間**（design.md の Input「要求の頭」）。
///
/// シートの識別子は含まない — 呼び出し側（5.2 の `GridSession`）は 1 枚のシートに閉じた
/// 操作口であり、シートはどの窓を見ているかで決まる（design.md の Service Interface の
/// `encode_window(&self, doc: &Document, span: RowSpan)` がシートを引数に持たないのと
/// 同じ理由）。生バイト経路の引数からこの値を組み立てるのは 6.3 である。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowRequest {
    /// この要求が対象とする世代。
    generation: Generation,
    /// 可視行の序数の区間（**文書の位置ではない**）。
    span: RowSpan,
}

impl WindowRequest {
    /// 世代と区間から要求を作る。
    #[inline]
    pub const fn new(generation: Generation, span: RowSpan) -> Self {
        Self { generation, span }
    }

    /// この要求が名乗る世代。
    #[inline]
    pub const fn generation(self) -> Generation {
        self.generation
    }

    /// 要求された区間（可視行の序数）。
    #[inline]
    pub const fn span(self) -> RowSpan {
        self.span
    }
}

/// 行の値を引く口（呼び出し側が行の索引の形を決める）。
///
/// [`WindowCodec::encode`] は**行の値の在処を知らない**。知っているのは表示の順序
/// （[`RowOrder`]）であり、そこから得た [`RowId`] の値をこの口で引く。こうすると
/// 符号化が [`Document`](document_format::Document) を受け取る経路が型の上に存在しなくなり、
/// **10 万行を走査する経路も存在しない**（モジュール docs「費用の形」）。
///
/// 実装は 1 回の呼び出しの間だけ有効な参照を返す（5.2 の `GridSession` は 1 回の呼び出しの
/// 間だけ行の索引を保つ）。`None` は「その行が文書に無い」であり、
/// [`WindowCodec::encode`] は [`GridError::UnknownRow`] で止まる（表示の状態と文書が
/// 食い違っている場合である）。
///
/// # 返す値の意味
///
/// 返る切片の**列の添字は [`Sheet::columns`](document_format::Sheet::columns) に対する位置**
/// である（窓の列数と行の値の数の関係は [`WindowCodec::encode`] の docs）。
pub trait WindowRowSource {
    /// 行の値（列順）。その行が文書に無ければ `None`。
    fn values(&self, row: RowId) -> Option<&[CellValue]>;
}

/// 可視範囲の窓を二進形式へ符号化する（design.md の Batch 契約）。
///
/// 型の全体像と配置はモジュール docs が正典である。要点だけをここに置く:
///
/// - 同じ世代・同じ区間の要求は常に**同じバイト列**になる（冪等）
/// - 世代が一致しない要求は**空の窓**を返す（[`WindowCodec::is_stale`]）
/// - 開始序数が可視行数より後ろの要求は [`GridError::SpanOutOfRange`] であり、
///   端に掛かる要求は**切り落とす**（[`RowOrder::span`] の規約）
/// - 行の値は [`WindowRowSource`] に**1 行につき 1 回**だけ問い合わせる
///
/// 本型は**世代だけを持つ**（要求ごとの状態を溜めない）。したがって 2 回の同じ要求が
/// 同じ結果になることは、状態ではなく規則から従う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowCodec {
    /// いまの世代（窓はつねにこの世代のものとして書かれる）。
    generation: Generation,
}

impl WindowCodec {
    /// 世代を指定して符号化器を作る。
    #[inline]
    pub const fn new(generation: Generation) -> Self {
        Self { generation }
    }

    /// いまの世代。
    #[inline]
    pub const fn generation(self) -> Generation {
        self.generation
    }

    /// いまの世代を置き換える（5.2 が表示を変えたときに呼ぶ）。
    ///
    /// **進めることだけが契約である**（古い世代へ戻すと、フロントエンドが既に捨てた世代の
    /// 窓を受け取る経路ができる）。
    #[inline]
    pub fn set_generation(&mut self, generation: Generation) {
        self.generation = generation;
    }

    /// 要求の世代が**いまの世代と一致しない**か。
    ///
    /// 一致しない要求には窓を作らない（[`WindowCodec::encode`] が空の窓を返す）。規則を
    /// 「古い」ではなく「一致しない」にしてある理由はモジュール docs「世代」を参照 —
    /// 窓はつねに現在の世代のものであり、違う世代を名乗る要求へ窓を返すと、フロントエンドが
    /// 別の世代の窓として受け取る。
    #[inline]
    #[must_use]
    pub const fn is_stale(self, request: &WindowRequest) -> bool {
        request.generation.0 != self.generation.0
    }

    /// 可視行の区間 `request.span()` を窓へ符号化する。
    ///
    /// `order` は表示の順序（可視行の並び）、`index` は違反の索引（窓の違反の札の源）、
    /// `columns` は**窓が運ぶ列の数**（宣言の列数。タスク 5.1 の依存が 2.1・2.2・2.4 で
    /// あり、`view` 層の [`ColumnLayout`](crate::view::ColumnLayout)（2.3 の入れ子の展開）は
    /// 含まない — 入れ子の展開が窓の列を変えることは、展開の実装が入るときに本 signature を
    /// 見直す合図である）、`source` は行の値を引く口である。
    ///
    /// # 規則（評価の順）
    ///
    /// 1. **世代が一致しなければ空の窓**（[`EMPTY_WINDOW`]）。範囲は見ない
    /// 2. **開始序数が可視行数より後ろなら** [`GridError::SpanOutOfRange`]。この経路は
    ///    `source` を 1 回も呼ばない
    /// 3. 行は [`RowOrder::span`] の切片から取り、**端に掛かる区間は切り落とす**。
    ///    切り落とした行数が頭の行数になる（可視行が 0 のシートは頭だけの窓になる）
    /// 4. 行の値を [`WindowRowSource::values`] で引く。`None` なら [`GridError::UnknownRow`]
    ///
    /// # 列の数と行の値の数の関係
    ///
    /// 窓は**宣言の列数**（`columns`）ぶんのセルをつねに運ぶ。行の値の数がそれに足りなければ
    /// 足りない列は**値なし**（[`CellValue::Null`]）として運び、多い場合は余り
    /// （表示に列が無い値）を運ばない。**列の添字は [`Row::values`](document_format::Row::values)
    /// に対する位置**であり、この規約は `view` 層の `value_of`（値を持たない行の扱い）と
    /// 同じである。
    ///
    /// # 誤り
    ///
    /// | 誤り | いつ |
    /// |---|---|
    /// | [`GridError::SpanOutOfRange`] | 開始序数が可視行数より後ろ（モジュール docs「範囲の検査」） |
    /// | [`GridError::UnknownRow`] | 表示の順序が指す行を `source` が知らない |
    ///
    /// どちらも**何も書かない**（値は戻り値の [`Vec`] にしか現れない。本メソッドは
    /// `&self` であり、文書への可変参照を持たない）。
    pub fn encode(
        &self,
        order: &RowOrder,
        index: &ViolationIndex,
        columns: usize,
        source: &dyn WindowRowSource,
        request: &WindowRequest,
    ) -> Result<Vec<u8>, GridError> {
        let span = request.span();
        // 1. 世代。古い（一致しない）要求は空の窓であり、失敗ではない。
        if self.is_stale(request) {
            return Ok(EMPTY_WINDOW.to_vec());
        }
        // 2. 範囲。開始序数が可視行数を超える要求は、この世代の表示と整合しない。
        if span.start().get() > order.len() {
            return Err(GridError::SpanOutOfRange {
                span,
                visible: order.len(),
            });
        }

        // 3. 切り落とし。`order.span` の規約がそのまま窓の行数になる。
        let rows = order.span(span);
        let mut out = Vec::with_capacity(
            HEADER_LEN + rows.len() * (ROW_KEY_LEN + columns * MIN_ENCODED_CELL_LEN),
        );
        out.push(WINDOW_FORMAT_VERSION);
        out.extend_from_slice(&self.generation.0.to_le_bytes());
        out.extend_from_slice(&(span.start().get() as u64).to_le_bytes());
        out.extend_from_slice(&(rows.len() as u64).to_le_bytes());
        out.extend_from_slice(&(columns as u64).to_le_bytes());

        for (offset, row) in rows.iter().enumerate() {
            let values = source
                .values(*row)
                .ok_or(GridError::UnknownRow { row: *row })?;
            out.extend_from_slice(&row.ulid().to_bytes());
            // 違反は可視行の序数で引く（窓は表示の窓である。索引の鍵は序数である）。
            let ordinal = RowOrdinal::new(span.start().get() + offset);
            let violations = index.row_violations(ordinal);
            for column in 0..columns {
                let value = values.get(column).unwrap_or(&CellValue::Null);
                let marks: &[NestedPath] = violations
                    .and_then(|entry| entry.cell(ColumnIndex::new(column)))
                    .map(|cell| cell.paths())
                    .unwrap_or(&[]);
                push_cell(&mut out, value, marks);
            }
        }
        Ok(out)
    }
}

/// セル 1 つが最低限占めるバイト数（変種の札 1 + 違反の有無 1 + 長さ 8）。
///
/// 確保の見積もりにだけ使う（確保は**上限**であり、実際の窓は表示文字列のぶんだけ長い）。
/// 見積もりを下回ることはないため、`Vec` の伸長は表示文字列のぶんだけしか起きない。
const MIN_ENCODED_CELL_LEN: usize = 10;

/// セル 1 つを窓へ書く（[`WindowCodec::encode`] の下請け）。
///
/// 配置はモジュール docs の表が唯一の源である。`marks` が空なら違反の有無のバイトは
/// [`VIOLATED_NO`] であり、**札の塊は書かれない** — 空の経路（セル直下の違反）は
/// 「札が 1 つあり、その段の数が 0」として現れる（違反なしとは区別される）。
fn push_cell(out: &mut Vec<u8>, value: &CellValue, marks: &[NestedPath]) {
    out.push(variant_tag(value).byte());
    match marks.is_empty() {
        true => out.push(VIOLATED_NO),
        false => {
            out.push(VIOLATED_YES);
            push_u64(out, marks.len());
            for mark in marks {
                push_u64(out, mark.segments().len());
                for segment in mark.segments() {
                    match segment {
                        NestedPathSegment::Field(name) => {
                            out.push(SEGMENT_FIELD);
                            push_u64(out, name.len());
                            out.extend_from_slice(name.as_bytes());
                        }
                        NestedPathSegment::Index(index) => {
                            out.push(SEGMENT_INDEX);
                            push_u64(out, *index);
                        }
                    }
                }
            }
        }
    }
    push_text(out, value);
}

/// 表示文字列を書く（長さの欄を先に置き、書いた後に埋める）。
///
/// 表示文字列の規則は `view` 層の [`DisplayText`] が唯一の源であり、本関数はその
/// **書き先を用意するだけ**である（セルごとの [`String`] を確保しない。モジュール docs
/// 「表示文字列は `view` 層の 1 つを再利用する」）。
///
/// 長さを後から埋めるのは、`Display` の書式化が書き込みながら長さを決めるためである。
/// **後方参照ではない** — 長さの欄は本体の直前という固定の位置にあり、復号は前から読む。
fn push_text(out: &mut Vec<u8>, value: &CellValue) {
    let length_at = out.len();
    out.extend_from_slice(&[0; 8]);
    let body_at = out.len();
    write!(out, "{}", DisplayText(value)).expect("Vec への書き込みは失敗しない");
    let length = (out.len() - body_at) as u64;
    out[length_at..length_at + 8].copy_from_slice(&length.to_le_bytes());
}

/// `usize` を `u64` のリトルエンディアンで書く。
///
/// `usize` は 64 ビット以下であるため、この変換は常に正確である（モジュール docs
/// 「なぜ数の欄がすべて 64 ビットなのか」）。
#[inline]
fn push_u64(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&(value as u64).to_le_bytes());
}

/// 窓の復号が失敗する理由（判別可能な列挙体。診断に要る文脈だけを持ち、提示の文言を持たない）。
///
/// `GridError` と混ぜないのは、**この誤りが経路の内側で閉じる**ためである — 復号の失敗は
/// 6.3 の生バイト経路が空の窓へ写す（design.md「Error Categories and Responses」の
/// 「経路の失敗」）。`GridError` は「宣言・指定が壊れている」を表す型であり、窓のバイト列の
/// 破損はそこに属さない。
///
/// **型付きの誤りは速度ではなく形で示す**（`verification.md`）ため、復号は未知の版も
/// 切り詰めも panic せず、この型で返す（`tests/window_codec.rs` が接頭辞のすべてを走査する）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowDecodeError {
    /// 空の窓（[`EMPTY_WINDOW`]）。
    ///
    /// これは**失敗の表現**である（design.md の Batch 契約が「失敗は空の窓で表す」と定める）。
    /// 行 0 の窓（頭だけ）ではない — 行 0 の窓は [`HEADER_LEN`] バイトである。
    Empty,
    /// 頭が宣言する行数を読み終える前に尽きた（**すべての真の接頭辞**がここへ来る）。
    Truncated,
    /// 知らない版（[`WINDOW_FORMAT_VERSION`] 以外）。
    UnknownVersion(u8),
    /// 知らない変種の札（[`VariantTag::ALL`] の表に無い値）。
    UnknownTag(u8),
    /// 知らない違反の有無のバイト（`0` = 違反なし / `1` = 違反あり のどちらでもない）。
    UnknownViolationFlag(u8),
    /// 知らない段の種類（`0` = フィールド / `1` = 添字 のどちらでもない）。
    UnknownSegmentKind(u8),
    /// 行数を読み終えた後にバイトが残っている（余りを黙って捨てない）。
    TrailingBytes,
    /// 数の欄がこのホストの `usize` に収まらない（32 ビットのホストで 64 ビットの窓を読む）。
    TooLarge,
    /// 表示文字列の本体が UTF-8 ではない（窓は UTF-8 だけを運ぶ）。
    NotUtf8,
}

impl fmt::Display for WindowDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty window"),
            Self::Truncated => f.write_str("truncated window"),
            Self::UnknownVersion(version) => write!(f, "unknown window version {version}"),
            Self::UnknownTag(tag) => write!(f, "unknown cell variant tag {tag}"),
            Self::UnknownViolationFlag(flag) => write!(f, "unknown violation flag {flag}"),
            Self::UnknownSegmentKind(kind) => write!(f, "unknown nested path segment kind {kind}"),
            Self::TrailingBytes => f.write_str("trailing bytes after the declared rows"),
            Self::TooLarge => f.write_str("a length field does not fit this host"),
            Self::NotUtf8 => f.write_str("a display string is not UTF-8"),
        }
    }
}

impl std::error::Error for WindowDecodeError {}

/// 復号された窓（[`decode_window`] が返す値）。
///
/// 入力のバイト列を**借用**しており、表示文字列はその一部を指す（コピーしない）。したがって
/// 復号は 1 回の走査で済み、窓の大きさに比例する確保は**札の分だけ**である。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedWindow<'a> {
    /// 版（[`WINDOW_FORMAT_VERSION`]）。
    version: u8,
    /// 世代。
    generation: Generation,
    /// 開始序数（可視行の序数）。
    start: RowOrdinal,
    /// 列数（1 行が運ぶセルの数）。
    columns: usize,
    /// 行（表示の順）。
    rows: Vec<DecodedRow<'a>>,
}

impl<'a> DecodedWindow<'a> {
    /// 版（設計は [`WINDOW_FORMAT_VERSION`] 以外を拒むため、復号できた窓では常にその値）。
    #[inline]
    pub const fn version(&self) -> u8 {
        self.version
    }

    /// 世代（この窓が属する文書・表示の状態）。
    #[inline]
    pub const fn generation(&self) -> Generation {
        self.generation
    }

    /// 開始序数（**可視行の序数**であり、文書の位置ではない）。
    #[inline]
    pub const fn start(&self) -> RowOrdinal {
        self.start
    }

    /// この窓が運ぶ行の数。
    #[inline]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// 1 行が運ぶセルの数（宣言の列数）。
    #[inline]
    pub const fn columns(&self) -> usize {
        self.columns
    }

    /// 行（表示の順）。空になりうる（開始序数が可視行数に等しい要求、可視行 0 のシート）。
    #[inline]
    pub fn rows(&self) -> &[DecodedRow<'a>] {
        &self.rows
    }
}

/// 復号された 1 行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedRow<'a> {
    /// 行の識別子の生 16 バイト（[`ROW_KEY_LEN`]）。**不透明な鍵**として扱う。
    key: [u8; ROW_KEY_LEN],
    /// セル（列順）。
    cells: Vec<DecodedCell<'a>>,
}

impl<'a> DecodedRow<'a> {
    /// 行の識別子の生バイト。
    ///
    /// フロントエンドはこれを**不透明な鍵**として使う（数値へ変換しない。design.md
    /// 「Data Models / 窓の二進形式」）。Rust の側で [`RowId`] へ戻す経路は本層に無い —
    /// 窓は識別子を「行そのもの」として運び、写すのは呼び出し側である。
    #[inline]
    pub const fn key(&self) -> [u8; ROW_KEY_LEN] {
        self.key
    }

    /// セル（列順。長さは [`DecodedWindow::columns`]）。
    #[inline]
    pub fn cells(&self) -> &[DecodedCell<'a>] {
        &self.cells
    }
}

/// 復号された 1 セル。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedCell<'a> {
    /// 変種の札。
    tag: VariantTag,
    /// 違反している内側の位置（報告の順。**空の並びなら違反なし**であり、セル直下の違反は
    /// 空の位置 1 つとして現れる）。
    marks: Vec<NestedPath>,
    /// 表示文字列（UTF-8。値なしは空文字）。
    text: &'a str,
}

impl<'a> DecodedCell<'a> {
    /// 変種の札（7.4 が入力手段を選ぶのに使う）。
    #[inline]
    pub const fn tag(&self) -> VariantTag {
        self.tag
    }

    /// このセルが違反しているか（違反の札が 1 つ以上あるかと同じ）。
    ///
    /// 要件 4.1 の提示（違反しているセルを区別する）がこれを読む。理由（4.2）は窓に無い —
    /// 理由は封筒つきの `GridViolationResponse.reason` が運ぶ。
    #[inline]
    pub fn violated(&self) -> bool {
        !self.marks.is_empty()
    }

    /// 違反している**内側の位置**（要件 4.5。報告の順、畳まない）。
    ///
    /// 空の並びは違反なしを意味する。セル直下の違反は [`NestedPath::root`]（段 0）として
    /// 現れる — 「入れ子の内側のどこか」と「セルそのもの」を区別できる形で運ぶためである。
    #[inline]
    pub fn marks(&self) -> &[NestedPath] {
        &self.marks
    }

    /// 表示文字列（`view` 層の規則が唯一の源。値なしは空文字）。
    #[inline]
    pub fn text(&self) -> &'a str {
        self.text
    }
}

/// 窓のバイト列を復号する（**前方 1 回の走査**。モジュール docs の不変条件）。
///
/// 空の窓（[`EMPTY_WINDOW`]）は [`WindowDecodeError::Empty`] である — 空の窓は**失敗の
/// 表現**であり、行 0 の窓（頭だけ）ではない。
///
/// # 検査
///
/// - 知らない版（[`WindowDecodeError::UnknownVersion`]）と知らない札
///   （[`WindowDecodeError::UnknownTag`] ほか）を拒む
/// - **完全な窓のすべての真の接頭辞**を拒む（長さの欄が本体の直前にあるため、尽きた時点で
///   分かる。不変条件の裏側）
/// - 余分なバイトを拒む（[`WindowDecodeError::TrailingBytes`]）
/// - 宣言された数を信用せず、確保は読み進めながら行う（壊れた窓で巨大な確保をしない）
///
/// **panic しない。**この経路は webview から届くバイト列を扱うため、壊れた入力は誤りとして
/// 返さなければならない（6.3 がそれを空の窓へ写す）。
pub fn decode_window(bytes: &[u8]) -> Result<DecodedWindow<'_>, WindowDecodeError> {
    // 空の窓は**失敗の表現**であり、切り詰めとは別の理由である（呼び出し側が
    // 「要求が通らなかった」と「バイト列が壊れている」を区別できるようにする）。
    if bytes.is_empty() {
        return Err(WindowDecodeError::Empty);
    }
    let mut cursor = Cursor::new(bytes);
    let version = cursor.u8()?;
    if version != WINDOW_FORMAT_VERSION {
        return Err(WindowDecodeError::UnknownVersion(version));
    }
    let generation = Generation::new(cursor.u64()?);
    let start = RowOrdinal::new(cursor.usize()?);
    let row_count = cursor.usize()?;
    let columns = cursor.usize()?;

    let mut rows = Vec::new();
    for _ in 0..row_count {
        let key: [u8; ROW_KEY_LEN] = cursor
            .take(ROW_KEY_LEN)?
            .try_into()
            .expect("`take` は要求した長さを返す");
        let mut cells = Vec::new();
        for _ in 0..columns {
            cells.push(decode_cell(&mut cursor)?);
        }
        rows.push(DecodedRow { key, cells });
    }
    if !cursor.is_at_end() {
        return Err(WindowDecodeError::TrailingBytes);
    }
    Ok(DecodedWindow {
        version,
        generation,
        start,
        columns,
        rows,
    })
}

/// セル 1 つを復号する（[`decode_window`] の下請け）。
fn decode_cell<'a>(cursor: &mut Cursor<'a>) -> Result<DecodedCell<'a>, WindowDecodeError> {
    let tag = VariantTag::from_byte(cursor.u8()?)
        .ok_or_else(|| WindowDecodeError::UnknownTag(cursor.last_byte()))?;
    let marks = match cursor.u8()? {
        VIOLATED_NO => Vec::new(),
        VIOLATED_YES => {
            let count = cursor.usize()?;
            // 宣言された数を信用しない（確保は読み進めながら）。
            let mut marks = Vec::new();
            for _ in 0..count {
                marks.push(decode_mark(cursor)?);
            }
            marks
        }
        other => return Err(WindowDecodeError::UnknownViolationFlag(other)),
    };
    let length = cursor.usize()?;
    let text = cursor.utf8(length)?;
    Ok(DecodedCell { tag, marks, text })
}

/// 内側の位置の札 1 つを復号する。
///
/// 上流の [`ValuePath`] を組み立ててから、`types` 層の正規の変換（`From<&ValuePath>`）を
/// 通す — 経路の表現の写しを本層に作らない（モジュール docs）。
fn decode_mark(cursor: &mut Cursor<'_>) -> Result<NestedPath, WindowDecodeError> {
    let segments = cursor.usize()?;
    let mut path = ValuePath::root();
    for _ in 0..segments {
        match cursor.u8()? {
            SEGMENT_FIELD => {
                let length = cursor.usize()?;
                let name = cursor.utf8(length)?;
                path.push_field(name);
            }
            SEGMENT_INDEX => {
                let index = cursor.usize()?;
                path.push_index(index);
            }
            other => return Err(WindowDecodeError::UnknownSegmentKind(other)),
        }
    }
    Ok(NestedPath::from(&path))
}

/// 前方向のカーソル（[`decode_window`] の唯一の読み取り口）。
///
/// **本型が「前方 1 回の走査」の実体である** — 位置は前へしか進まず、後戻りする経路を
/// 持たない（モジュール docs の不変条件）。
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    /// 入力の先頭から読む。
    #[inline]
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    /// 入力を使い切ったか（余りの検査に使う）。
    #[inline]
    const fn is_at_end(&self) -> bool {
        self.at == self.bytes.len()
    }

    /// 直前に読んだバイト（知らない値を誤りへ載せるため）。
    #[inline]
    fn last_byte(&self) -> u8 {
        self.bytes[self.at - 1]
    }

    /// `count` バイトを読み進めて返す。尽きていれば [`WindowDecodeError::Truncated`]。
    #[inline]
    fn take(&mut self, count: usize) -> Result<&'a [u8], WindowDecodeError> {
        let end = self
            .at
            .checked_add(count)
            .ok_or(WindowDecodeError::Truncated)?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or(WindowDecodeError::Truncated)?;
        self.at = end;
        Ok(slice)
    }

    /// 1 バイト。
    #[inline]
    fn u8(&mut self) -> Result<u8, WindowDecodeError> {
        Ok(self.take(1)?[0])
    }

    /// リトルエンディアンの `u64`。
    #[inline]
    fn u64(&mut self) -> Result<u64, WindowDecodeError> {
        let raw = self.take(8)?;
        Ok(u64::from_le_bytes(
            raw.try_into().expect("`take` は 8 バイトを返す"),
        ))
    }

    /// `usize` に収まる数の欄。収まらなければ [`WindowDecodeError::TooLarge`]。
    #[inline]
    fn usize(&mut self) -> Result<usize, WindowDecodeError> {
        usize::try_from(self.u64()?).map_err(|_| WindowDecodeError::TooLarge)
    }

    /// `length` バイトの UTF-8 の本体。
    #[inline]
    fn utf8(&mut self, length: usize) -> Result<&'a str, WindowDecodeError> {
        let raw = self.take(length)?;
        core::str::from_utf8(raw).map_err(|_| WindowDecodeError::NotUtf8)
    }
}

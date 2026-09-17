//! テストとベンチが共有する標本の生成器（タスク 1.4。要件 1.1, 11.7）。
//!
//! `tests/` 配下の各 `.rs` は独立したテストバイナリであり、あるテストクレートの関数を
//! 別のテストクレートから使う機構が無い。さらに `benches/` から `tests/` のモジュールを
//! そのまま取り込むこともできない。したがって共有する道具はここへ置き、テストは
//! `mod common;`、ベンチは `#[path = "../tests/common/mod.rs"] mod common;` で取り込む
//! （tasks.md 1.4「ベンチからは相対パス指定でこの共有モジュールを取り込む」）。
//!
//! # 1 つの入口で「行数・列数・違反の割合・種」を決める（tasks.md 1.4）
//!
//! 入口は [`sample`] 1 つであり、変わるのは [`SampleOptions`] の 4 つだけである:
//! 行数・列数・違反の割合（`0.0..=1.0`）・種（擬似乱数の種。値の中身だけを決める）。
//! 列数は 1..=[`SAMPLE_COLUMNS`]（30）であり、**列の宣言の並びの先頭からの前置**を取る。
//! 先頭 [`COVERAGE_COLUMNS`]（13）列までで、タスク 1.4 が挙げる組込の各型
//! （`Int` / `Float` / `Decimal` / `Text` / `Bool` / `Date` / `DateTime` / `Enum` / `Ref` /
//! `Attachment` / `Object` / `Array` / `Any`）が 1 列ずつ揃う — 列数を減らしても網羅が
//! 先頭で保たれるのは、この並びが保証であるためである。
//!
//! 拡張型（`custom`）は標本に**含めない**。拡張型は実装の登録（`TypeRegistry`）を要し、
//! その標本は拡張型の隔離を見る側（`schema-engine` の `tests/common/schema.rs`）が持つ。
//! 本生成器が返す [`Sample::compiled`] は空の台帳でコンパイルする（標本は組込型だけで
//! 閉じる）。
//!
//! # 現実に寄せた列構成（tasks.md 1.4）
//!
//! 列の並びは「発注明細」という 1 シートの現実の形に寄せてある。`Text` の品番が
//! **唯一の一意制約つきの列**であり、`required` の列は 5 本（品番・数量・仕入先・
//! 届け先・機械可読値）である。入れ子は 3 列あり、性格がそれぞれ違う:
//!
//! | 列 | 形 | 入れ子の内側 |
//! |----|----|--------------|
//! | `届け先` | オブジェクト（名前つき型定義を使わず**インラインで宣言**する） | 書式つき文字列・選択肢・長さつき文字列・既定値つきの任意フィールド・**参照** |
//! | `明細` | 配列（要素はインラインのオブジェクト。要素数の範囲つき） | 書式つき文字列・範囲つき整数・桁つき 10 進数 |
//! | `改訂履歴` | 配列（要素はインラインのオブジェクト。上限つき） | 日付・範囲つき整数・長さつき文字列 |
//!
//! 宣言は上流の不透明なペイロード（[`SchemaPart`]）として組み立てる。[`Sample::schema_part`]
//! がそれを返し、宣言テキストの**正準出力は `schema-engine` の `declaration::codec` に
//! 任せる**（本モジュールは宣言テキストの文法を再実装しない）。テストは
//! `schema_engine::parse_schema` でそれを読み直し、列の宣言を直接突き合わせる。
//!
//! # 適合する値は違反を 1 件も生まず、違反する値はちょうど 1 件を生む（tasks.md 1.4）
//!
//! [`SampleColumn`] が列ごとに 2 つの規則（`conforming` / `violating`）を持ち、[`sample`]
//! が違反の位置にだけ後者を置く。**違反する値は 1 セルにつき 1 件**でなければならない —
//! 入れ子の列には形の違う値を置き（内側へ降りずに 1 件で済むのではなく、内側の**1 つの
//! フィールドだけ**を外して 1 件にする）、複数の制約を持つ列にはそのうち 1 つだけを外す
//! 値（例: 長さは合うが書式だけを外す）を置く。この不変条件が「違反の件数と位置が仕込んだ
//! とおりであること」の土台である。
//!
//! 違反の位置は**列優先**の平坦添字（`column * 行数 + row`）で等間隔に選ぶ。行優先
//! （`row * 列数 + column`）の添字でそのまま選ぶと、30 列の標本では間隔（10 万行 × 30 列で
//! 3,000 件なら 1,000）が列数の倍数になり（`gcd(1000, 30) = 10`）、違反が一部の列にしか
//! 現れない — 残りの列の違反の規則が死ぬ。列優先にすると各列が同じ割合を受け取る。
//!
//! # 一意制約の重複は数え方が違う（[`SampleOptions::with_unique_collisions`]）
//!
//! 重複の違反は**値ごとに 1 件**であり（重複するすべての行を `rows` に載せる）、仕込んだ
//! セルと 1 対 1 にならない。したがって既定では重複を混ぜず、混ぜたときの件数と位置は
//! [`Sample::duplicate_first_rows`] が**標本自身が使ったのと同じ算術から**導く（手で書いた
//! 定数ではない。テストが検証の結果と突き合わせる）。
//!
//! # 決定性 — 何が同一で、何が同一でないか（tasks.md 1.4 の核心）
//!
//! **同じ引数の 2 回の [`sample`] は、バイト列としては一致しない。** `document-format` の
//! 行識別子とシート識別子は発行時刻を含む ULID（[`IdFactory`]）であり、組み立てのたびに
//! 変わる。標本はその識別子を 2 箇所に載せる:
//!
//! 1. **シート間参照の値** — 参照列（`仕入先`・`承認者`）と、入れ子の内側の参照
//!    （`届け先.最寄り倉庫`）の値は参照先シートの行識別子である。
//! 2. **`ref` の宣言が運ぶ参照先シートの識別子**（`Constraints::sheet`）。
//!
//! どちらも本モジュールでは固定できない（公開経路に「識別子を指定して行を作る」口が無く、
//! シート識別子も `Document::add_sheet` が発行する）。したがって**同一なのは次の 4 つ**で
//! あり、比較してよいのはこの 4 つだけである:
//!
//! - **形** — 列数・行数・列名の並び。
//! - **宣言** — 列の宣言（名前・型・制約・必須・一意・既定値）と入れ子のフィールドの宣言。
//!   参照先シートの識別子だけが実行ごとに変わる（伏せれば一致する）。
//! - **値** — 各セルの値。参照先の行識別子を運ぶ値だけが実行ごとに変わる（参照先シートの
//!   行位置へ畳めば一致する）。添付の識別子は内容から決まる（content-addressed）ため
//!   安定であり、伏せる必要が無い。
//! - **違反の集合** — 行添字・列添字・入れ子の位置（[`ValuePath`](schema_engine::ValuePath)）
//!   と、理由の**種別**。位置は行添字・列添字の算術だけから決まり、識別子に依らない。
//!   理由が運ぶ実際の値（違反した値・重複する行の識別子）は実行ごとに変わる。
//!
//! **生の値・生の宣言テキスト・文書のバイト列は比較してはならない**（必ず食い違う）。
//! `tests/sample.rs` の決定性の検査は、この 4 つだけを突き合わせる。
//!
//! 値の**中身**は種（[`SampleOptions::with_seed`]）から決まる。種は擬似乱数（本モジュールが
//! 持つ小さな xorshift）の初期状態であり、`HashMap` の反復順のような実行ごとに変わる
//! ものには依らない（`document-format` が `NestedValue::Object` で `HashMap` を禁じている
//! のと同じ理由）。同じ種・同じ他の引数なら値は常に同じであり、種を変えれば**値だけ**が
//! 変わる（違反の位置は種に依らない — 位置を選ぶのは添字の算術だけである）。
//!
//! # 依存（tasks.md 1.4 の境界）
//!
//! 本モジュールは `document-format`（文書・セル値・識別子）と `schema-engine`（宣言の型と
//! 正準出力・コンパイル）だけに依存する。**本番モジュール（`src/**`）には何も足さない** —
//! 標本はテストとベンチのための道具であり、出荷物の依存を 1 つも増やさない
//! （`crates/data-grid` は `tauri` に依存せず、依存してよい兄弟は `document-format` と
//! `schema-engine` の 2 つだけである。`Cargo.toml` の冒頭コメント）。

use std::str::FromStr;

use document_format::parts::{to_parts, DocumentParts, ManifestEntry, ManifestPart};
use document_format::{
    to_json_bytes, AttachmentId, CellValue, Document, DocumentFormat, DocumentFormatApi, EntryName,
    FormatVersion, IdFactory, NestedValue, RowId, SchemaPart, SheetId,
};
use schema_engine::{
    schema_to_text, ColumnDecl, CompiledSchema, Constraints, DecimalDigits, DeclaredKind,
    FieldDecl, OffsetPolicy, Schema, SchemaEngine, SchemaEngineApi, TypeDecl, TypeKind,
    TypeRegistry,
};

/// 標本の既定の行数（要件 1.1「1 つのシートにつき 10 万行かつ 30 列」の行数の側）。
pub const SAMPLE_ROWS: usize = 100_000;

/// 標本の列数（要件 1.1 の列数の側）。列数の上限でもある。
pub const SAMPLE_COLUMNS: usize = 30;

/// 先頭からこの列数までで、タスク 1.4 が挙げる組込の各型（拡張型を除く 13 種別）が
/// 1 列ずつ揃う（モジュール docs「1 つの入口で…」の並びの保証）。
pub const COVERAGE_COLUMNS: usize = 13;

/// 標本の既定の違反の割合（全セルの 0.1%。10 万行 × 30 列で 3,000 件）。
pub const SAMPLE_VIOLATION_RATIO: f64 = 0.001;

/// 標本の既定の種（擬似乱数の初期状態。値の中身だけを決める固定値）。
pub const SAMPLE_SEED: u64 = 0x5EED_2026;

/// 参照先シート（`取引先`）の行数。参照列の適合する値はこの行を指す。
pub const REFERENCE_ROWS: usize = 64;

/// 一意制約つきの列（品番）の適合する値が巡回する値の個数。
///
/// [`SampleOptions::with_unique_collisions`] を立てると値がこの個数を巡回するため、
/// 行数がこの個数を超える分だけ重複が生じる（10 万行なら 1 値あたり 100 行）。
pub const UNIQUE_GROUPS: usize = 1_000;

/// 標本のデータシート（30 列を持つ側）の名前。
const SHEET_NAME: &str = "発注明細";
/// 標本の参照先シートの名前。
const REFERENCE_SHEET_NAME: &str = "取引先";
/// 参照先シートの列名。
const REFERENCE_FIELDS: [&str; 2] = ["名称", "部門"];
/// 添付の内容。識別子は内容から決まる（content-addressed なので実行を跨いで安定する）。
const ATTACHMENT_BYTES: &[u8] =
    b"1.4 \xe3\x81\xae\xe6\xa8\x99\xe6\x9c\xac\xe3\x81\xae\xe6\xb7\xbb\xe4\xbb\x98";
/// 入れ子のオブジェクト（`届け先`）のフィールド名。宣言と値の両方がこの並びを使う
/// （名前が食い違うと、宣言されたフィールドが値に無いものとして扱われる）。
const DESTINATION_FIELDS: [&str; 5] = ["郵便番号", "都道府県", "住所", "建物", "最寄り倉庫"];
/// 入れ子の配列（`明細`）の要素のフィールド名。
const LINE_FIELDS: [&str; 3] = ["商品コード", "数量", "単価"];
/// 入れ子の配列（`改訂履歴`）の要素のフィールド名。
const REVISION_FIELDS: [&str; 3] = ["改訂日", "版", "理由"];
/// 選択肢（`都道府県`）。
const PREFECTURES: [&str; 4] = ["東京都", "大阪府", "愛知県", "福岡県"];
/// 選択肢（`状態`）。
const STATES: [&str; 4] = ["未着手", "進行中", "完了", "取消"];
/// 選択肢（`単位`）。
const UNITS: [&str; 5] = ["個", "箱", "kg", "m", "式"];
/// 選択肢（`通貨`）。
const CURRENCIES: [&str; 3] = ["JPY", "USD", "EUR"];
/// 選択肢（`カテゴリ`）。
const CATEGORIES: [&str; 4] = ["資材", "部品", "設備", "消耗品"];
/// 選択肢（`予備区分`）。
const SPARE_GRADES: [&str; 3] = ["A", "B", "C"];
/// 一意制約つきの列（品番）の適合する値の接頭辞。
const UNIQUE_PREFIX: char = 'P';

/// 一意制約つきの列の添字（`品番`。標本で唯一の `unique` 列）。
const UNIQUE_COLUMN: usize = 0;

/// 標本の作り方（tasks.md 1.4）。
#[derive(Debug, Clone, Copy)]
pub struct SampleOptions {
    rows: usize,
    columns: usize,
    ratio: f64,
    seed: u64,
    unique_collisions: bool,
}

impl SampleOptions {
    /// 行数と列数を指定し、違反の割合・種・一意制約の重複は既定にする。
    ///
    /// `columns` は 1..=[`SAMPLE_COLUMNS`] であり、宣言の並びの**先頭からの前置**を取る
    /// （モジュール docs「1 つの入口で…」）。
    pub const fn new(rows: usize, columns: usize) -> Self {
        Self {
            rows,
            columns,
            ratio: SAMPLE_VIOLATION_RATIO,
            seed: SAMPLE_SEED,
            unique_collisions: false,
        }
    }

    /// 違反として仕込むセルの割合を変える（`0.0..=1.0` へ丸める。有限でない値は 0 とする）。
    pub const fn with_ratio(mut self, ratio: f64) -> Self {
        self.ratio = ratio;
        self
    }

    /// 擬似乱数の種を変える（値の中身だけが変わり、違反の位置は変わらない）。
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// 一意制約つきの列（品番）の値を [`UNIQUE_GROUPS`] 個で巡回させ、重複を混ぜる。
    pub const fn with_unique_collisions(mut self, collisions: bool) -> Self {
        self.unique_collisions = collisions;
        self
    }
}

impl Default for SampleOptions {
    /// 要件 1.1 の規模そのもの（10 万行 × 30 列、違反 0.1%、重複なし、既定の種）。
    fn default() -> Self {
        Self::new(SAMPLE_ROWS, SAMPLE_COLUMNS)
    }
}

/// 組み立てられた標本（tasks.md 1.4）。
///
/// 文書（データシートと参照先シート）・対象シートの識別子・行識別子・仕込んだ違反の内訳を
/// 持ち、宣言（[`Sample::schema_part`]）と計画（[`Sample::compiled`]）をその場で取り出せる。
/// 消費側は文書と計画をそのまま検証の入口へ渡せる。
pub struct Sample {
    document: Document,
    sheet: SheetId,
    reference: SheetId,
    row_ids: Vec<RowId>,
    columns: Vec<String>,
    plan: ViolationPlan,
    duplicates: Vec<usize>,
}

impl Sample {
    /// 標本の文書（データシートと参照先シート）。
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// 標本を、編集の適用に要る部品へ分解する（タスク 3.1 が足した）。
    ///
    /// 編集は文書を**変更する**ため、`&Document` しか返さない [`Sample::document`] のままでは
    /// 適用できない。所有権ごと渡すことで、消費側は文書を書き換えつつ、標本が申告する行の
    /// 並び・列名・シート識別子を使える（`SetCells` は行を増減しないため、行識別子の申告は
    /// 適用の後も有効である）。宣言と計画が要る場合は**分解の前に**
    /// [`Sample::compiled`] / [`Sample::schema_part`] で取り出す。
    ///
    /// **この標本に対する [`Sample::document`] 系の申告（行の並び・違反の位置）は、分解の
    /// 後は消費側の編集に依る** — 標本自身は文書を持たないため、編集の後の状態について何も
    /// 申告しない。期待値を編集の前後で比べる検査は、適用の前に読み取った値を使う。
    pub fn into_edit_parts(self) -> SampleEditParts {
        SampleEditParts {
            document: self.document,
            sheet: self.sheet,
            reference: self.reference,
            row_ids: self.row_ids,
            columns: self.columns,
        }
    }

    /// 30 列を持つデータシートの識別子。
    pub fn sheet(&self) -> SheetId {
        self.sheet
    }

    /// 参照先シートの識別子（参照列の適合する値が指す先）。
    pub fn reference_sheet(&self) -> SheetId {
        self.reference
    }

    /// 行識別子（行順）。違反の行を突き合わせるために使う。
    pub fn row_ids(&self) -> &[RowId] {
        &self.row_ids
    }

    /// 参照先シートの行識別子のテキストを、その行位置へ写す（決定性の比較用）。
    ///
    /// 参照の値は参照先シートの行の ULID であり実行ごとに変わるため、意味が同じかを
    /// 比較するには行位置へ畳む（モジュール docs「決定性 — 何が同一で、何が同一でないか」）。
    /// 参照先の行でないテキストには `None` を返す。
    pub fn reference_position(&self, text: &str) -> Option<usize> {
        let id = RowId::from_str(text).ok()?;
        self.document
            .sheet_by_id(self.reference)
            .expect("標本の参照先シートは文書にある")
            .rows()
            .iter()
            .position(|row| row.id() == id)
    }

    /// 行数。
    pub fn rows(&self) -> usize {
        self.row_ids.len()
    }

    /// 列名（宣言の並びそのもの。行データのキー順になる）。
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// 列数。
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// データシートのルートスキーマ（上流が不透明なペイロードとして保持する宣言）。
    pub fn schema_part(&self) -> &SchemaPart {
        self.document
            .sheet_by_id(self.sheet)
            .expect("標本のシートは文書にある")
            .root_schema()
    }

    /// 標本の宣言から計画を組み立てる（`schema-engine` の公開面だけを使う）。
    ///
    /// 空の台帳でコンパイルする（標本は組込型だけで閉じる。モジュール docs）。
    pub fn compiled(&self) -> CompiledSchema {
        let sheet = self
            .document
            .sheet_by_id(self.sheet)
            .expect("標本のシートは文書にある");
        SchemaEngine::new()
            .compile(sheet, &TypeRegistry::new())
            .expect("標本の宣言はコンパイルできる")
    }

    /// 行優先のセル値を複製して返す（決定性の比較用）。
    pub fn row_values(&self) -> Vec<Vec<CellValue>> {
        self.document
            .sheet_by_id(self.sheet)
            .expect("標本のシートは文書にある")
            .rows()
            .iter()
            .map(|row| row.values().to_vec())
            .collect()
    }

    /// 行添字と列添字のセルを違反として仕込んだか。
    pub fn violation_at(&self, row: usize, column: usize) -> bool {
        self.plan.is_violation(flat_index(row, column, self.rows()))
    }

    /// 違反として仕込んだセルの数（1 セルにつき違反 1 件）。
    pub fn injected_violations(&self) -> usize {
        self.plan.count()
    }

    /// 一意制約の重複によって生じる違反の数（既定は 0）。
    pub fn unique_violations(&self) -> usize {
        self.duplicates.len()
    }

    /// 重複の違反が報告される行の添字（重複する値が最初に現れた行。昇順）。
    pub fn duplicate_first_rows(&self) -> Vec<usize> {
        self.duplicates.clone()
    }

    /// この標本が生む違反の総数（値の違反 + 一意制約の違反）。
    pub fn expected_violations(&self) -> usize {
        self.injected_violations() + self.unique_violations()
    }
}

/// [`Sample::into_edit_parts`] が返す部品（タスク 3.1）。
///
/// 編集の適用（`EditApply::apply`）は文書の可変参照を要するため、標本を**所有権ごと**
/// 分解する。分解の後に残る申告は、編集が行を増減しない限り（`SetCells`）有効である。
pub struct SampleEditParts {
    /// 標本の文書（データシートと参照先シート）。
    pub document: Document,
    /// 30 列を持つデータシートの識別子。
    pub sheet: SheetId,
    /// 参照先シートの識別子。
    pub reference: SheetId,
    /// 行識別子（編集の前の行順）。
    pub row_ids: Vec<RowId>,
    /// 列名（宣言の並びそのもの）。
    pub columns: Vec<String>,
}

/// 指定した行数・列数・違反の割合・種で標本を組み立てる（tasks.md 1.4。要件 1.1, 11.7）。
///
/// 同じ引数なら**形**（列数・行数・列名の並び・仕込む違反の数）と**値**と**違反の集合**が
/// 常に同じになる。ただし参照先シートの行識別子とシートの識別子は発行のたびに変わるため、
/// バイト列は一致しない（モジュール docs「決定性 — 何が同一で、何が同一でないか」）。
pub fn sample(options: &SampleOptions) -> Sample {
    assert!(
        options.columns >= 1 && options.columns <= SAMPLE_COLUMNS,
        "標本の列数は 1..={SAMPLE_COLUMNS}（先頭からの前置を取る）"
    );

    let mut skeleton = Document::new();
    let sheet = skeleton.add_sheet(SHEET_NAME);
    let reference = skeleton.add_sheet(REFERENCE_SHEET_NAME);

    // 参照先シートを先に符号化して行識別子を得る（参照列の適合する値がこれを要る）。
    let reference_names: Vec<String> = REFERENCE_FIELDS
        .iter()
        .map(|name| name.to_string())
        .collect();
    skeleton
        .set_sheet_columns(reference, reference_names.clone())
        .expect("標本の参照先シートは骨格にある");
    // **参照先のシートにも宣言を置く**（1.4 の要求は「シートと、それに適合するスキーマ宣言」で
    // ある）。宣言が無いシートは**表として開けない**（`GridSession::open` は列 0 本の計画を
    // `SchemaUnusable` として拒む）ので、9.2 の「シートを切り替えても取り消しが効く」の観測が
    // 切り替え先を持てなくなる（実測: 参照の列の一覧は読めるのに、参照先のシートは開けなかった）。
    skeleton
        .set_root_schema(
            reference,
            SchemaPart::parse(&envelope_text(&reference_names)).expect("参照先の宣言は妥当"),
        )
        .expect("標本の参照先シートは骨格にある");
    let (reference_entry, reference_bytes, reference_rows) = encode_rows(
        reference,
        &reference_names,
        REFERENCE_ROWS,
        |row, column| match column {
            0 => CellValue::Text(format!("取引先{:02}", row % 10)),
            _ => CellValue::Text(["購買", "資材", "経理", "物流"][row % 4].to_owned()),
        },
    );

    let mut context = SampleContext {
        reference_rows: reference_rows.clone(),
        attachment: AttachmentId::from_bytes(ATTACHMENT_BYTES),
        unique_collisions: options.unique_collisions,
        rng: Rng::new(options.seed),
    };
    // 参照列の宣言は参照先シートの識別子を運ぶ（`ref` の `sheet`）。参照先シートの識別子は
    // 上の `add_sheet` が発行したものを使う。
    let catalog = sample_columns(reference);
    let columns: &[SampleColumn] = &catalog[..options.columns];
    let names: Vec<String> = columns
        .iter()
        .map(|column| column.decl.name.to_string())
        .collect();

    skeleton
        .set_sheet_columns(sheet, names.clone())
        .expect("標本のシートは骨格にある");
    skeleton
        .set_root_schema(
            sheet,
            SchemaPart::parse(&envelope(columns)).expect("標本の宣言は妥当"),
        )
        .expect("標本のシートは骨格にある");

    // 行の値は列ごとの 2 つの規則から決まる（適合する値と、違反を 1 つだけ生む値）。
    let rows = options.rows;
    let column_count = columns.len();
    let plan = ViolationPlan::ratio(rows * column_count, options.ratio);
    let (rows_entry, rows_bytes, row_ids) = encode_rows(sheet, &names, rows, |row, column| {
        if plan.is_violation(flat_index(row, column, rows)) {
            (columns[column].violating)(&mut context, row)
        } else {
            (columns[column].conforming)(&mut context, row)
        }
    });

    // 骨格の行エントリを差し替えて 1 回で復元する。添付の実体も足す（参照の実在は
    // `attachments/<hex>.bin` の存在であり、内容は識別子と一致する）。
    let parts = to_parts(&skeleton).expect("骨格はパート集合へ取り出せる");
    let version = parts.format_version();
    let mut entries: Vec<(EntryName, Vec<u8>)> = parts
        .iter()
        .map(|part| (part.name, part.bytes.clone()))
        .collect();
    replace_entry(&mut entries, reference_entry, reference_bytes);
    replace_entry(&mut entries, rows_entry, rows_bytes);
    entries.push((
        EntryName::Attachment {
            attachment: context.attachment,
        },
        ATTACHMENT_BYTES.to_vec(),
    ));
    let entries = with_rebuilt_manifest(version, entries);
    let parts = DocumentParts::from_entries(entries).expect("行を差し込んだ集合は妥当");
    let document = DocumentFormat::new()
        .from_parts(&parts)
        .expect("行を差し込んだ集合は復元できる");

    let duplicates = if options.unique_collisions {
        duplicate_first_rows(rows, &plan)
    } else {
        Vec::new()
    };

    Sample {
        document,
        sheet,
        reference,
        row_ids,
        columns: names,
        plan,
        duplicates,
    }
}

/// セルの**列優先**の平坦添字（モジュール docs「適合する値は違反を 1 件も生まず…」）。
///
/// 行優先の添字をそのまま違反の選定へ渡すと、30 列の標本では等間隔の選定が列数の倍数に
/// なり、違反が一部の列に集中する。列優先にすると各列が同じ割合を受け取る。
fn flat_index(row: usize, column: usize, rows: usize) -> usize {
    column * rows + row
}

/// 重複の違反が報告される行の添字（重複する値が最初に現れた行。昇順）。
///
/// 品番（[`UNIQUE_COLUMN`]）の適合する値は [`UNIQUE_GROUPS`] 個を巡回し、違反として仕込んだ
/// 行だけがその巡回から外れる。2 件以上残った値の個数がそのまま違反の件数であり、最初に
/// 現れた行が報告される行である（`validate::unique` が重複の違反に行の並び順で最初の行を
/// 載せる）。仕込む位置は [`flat_index`] の選定で実行前に決まるため、この数も実行前に
/// 決まる — 手で書いた定数ではなく、標本自身が**同じ計画から**導く。
fn duplicate_first_rows(rows: usize, plan: &ViolationPlan) -> Vec<usize> {
    let mut members = vec![0usize; UNIQUE_GROUPS];
    let mut first = vec![usize::MAX; UNIQUE_GROUPS];
    for row in 0..rows {
        if plan.is_violation(flat_index(row, UNIQUE_COLUMN, rows)) {
            continue;
        }
        let group = row % UNIQUE_GROUPS;
        members[group] += 1;
        if first[group] == usize::MAX {
            first[group] = row;
        }
    }
    let mut out: Vec<usize> = (0..UNIQUE_GROUPS)
        .filter(|group| members[*group] >= 2)
        .map(|group| first[group])
        .collect();
    out.sort_unstable();
    out
}

/// 全セルのうちどの位置を違反として仕込むかの決定的な規則。
///
/// 総セル数 `total` のうち `ratio` の割合（**切り捨て**）を、列優先の平坦添字で等間隔に
/// 選ぶ。件数は厳密に `count` であり、位置は実行ごとに変わらない。
#[derive(Debug, Clone, Copy)]
struct ViolationPlan {
    total: usize,
    count: usize,
}

impl ViolationPlan {
    /// 総セル数 `total` の `ratio` の割合（切り捨て）を違反にする規則。
    ///
    /// `ratio` は `0.0` 以上 `1.0` 以下へ丸める。有限でない値は 0 として扱う（panic しない）。
    fn ratio(total: usize, ratio: f64) -> Self {
        let bounded = if ratio.is_finite() {
            ratio.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self {
            total,
            count: ((total as f64) * bounded) as usize,
        }
    }

    /// 仕込む違反の件数。
    const fn count(&self) -> usize {
        self.count
    }

    /// 平坦添字 `index` を違反として仕込むか。
    ///
    /// 添字 `i` の選定は `floor((i+1)*count/total) > floor(i*count/total)` であり、
    /// `0..total` を走査するとちょうど `count` 件が選ばれる（等間隔）。
    fn is_violation(&self, index: usize) -> bool {
        if self.count == 0 || self.total == 0 || index >= self.total {
            return false;
        }
        (index + 1) * self.count / self.total > index * self.count / self.total
    }
}

/// 標本の列が値を組み立てるのに要る文脈。
struct SampleContext {
    /// 参照先シートの行識別子（参照列の適合する値）。
    reference_rows: Vec<RowId>,
    /// 添付の識別子（添付の列の適合する値）。
    attachment: AttachmentId,
    /// 一意制約の列に重複を混ぜるか。
    unique_collisions: bool,
    /// 値の中身を決める擬似乱数（列優先ではなく**行優先**に 1 本だけ進む）。
    rng: Rng,
}

/// 標本の 1 列。
///
/// 宣言と、その列に入れる 2 つの値の規則を組にする。**適合する値は違反を 1 件も生まず、
/// 違反する値はちょうど 1 件を生む**（モジュール docs）。規則は値を持たない関数であり、
/// 列ごとに確保しない（標本は 3,000,000 セルを組み立てるため、規則の側で確保を増やさない）。
struct SampleColumn {
    /// 列の宣言（名前・型・必須・一意・既定値）。
    decl: ColumnDecl,
    /// 適合する値。
    conforming: fn(&mut SampleContext, usize) -> CellValue,
    /// 違反する値（その列で 1 つだけ制約を外した値）。
    violating: fn(&mut SampleContext, usize) -> CellValue,
}

/// 標本の 30 列（宣言の並びそのものが列の並び順であり、列数はこの先頭からの前置である）。
///
/// 先頭 [`COVERAGE_COLUMNS`] 列で組込の各型（拡張型を除く 13 種別）が 1 列ずつ揃う
/// （モジュール docs「1 つの入口で…」）。`reference_sheet` は参照列（`ref`）の宣言が運ぶ
/// 参照先シートの識別子である。
fn sample_columns(reference_sheet: SheetId) -> Vec<SampleColumn> {
    vec![
        // 0 品番: 必須かつ一意のキー。標本で唯一の `unique` 列である。
        SampleColumn {
            decl: column(
                "品番",
                typed(
                    TypeKind::Text,
                    Constraints {
                        min_length: Some(8),
                        max_length: Some(16),
                        pattern: Some("^[A-Z0-9-]+$".into()),
                        ..Constraints::default()
                    },
                ),
                true,
                true,
                None,
            ),
            conforming: |context, row| {
                if context.unique_collisions {
                    // 値が UNIQUE_GROUPS 個を巡回する（重複が生じる）。
                    CellValue::Text(format!("{UNIQUE_PREFIX}{:07}", row % UNIQUE_GROUPS))
                } else {
                    CellValue::Text(format!("{UNIQUE_PREFIX}{row:07}"))
                }
            },
            // 長さは範囲に収まり、書式だけを外す（`!` は `^[A-Z0-9-]+$` に無い）。
            violating: |_, row| CellValue::Text(format!("!{row:07}")),
        },
        // 1 数量: 範囲つきの整数（必須・既定値あり）。
        SampleColumn {
            decl: column(
                "数量",
                typed(
                    TypeKind::Int,
                    Constraints {
                        min: Some(CellValue::Int(1)),
                        max: Some(CellValue::Int(100_000)),
                        ..Constraints::default()
                    },
                ),
                true,
                false,
                Some(CellValue::Int(1)),
            ),
            conforming: |context, _| CellValue::Int(context.rng.below(1_000) as i64 + 1),
            violating: |_, _| CellValue::Int(-1),
        },
        // 2 単価: 桁と範囲つきの 10 進数。
        SampleColumn {
            decl: column(
                "単価",
                typed(
                    TypeKind::Decimal,
                    Constraints {
                        digits: decimal_digits(12, 2),
                        min: Some(CellValue::Decimal("0.00".to_owned())),
                        max: Some(CellValue::Decimal("99999999.99".to_owned())),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| decimal(context.rng.below(10_000), context.rng.below(100)),
            // 桁だけを外す（文法にも範囲にも合うが、宣言された scale を超える）。
            violating: |_, _| CellValue::Decimal("1.23456".to_owned()),
        },
        // 3 予備率: 範囲つきの小数。
        SampleColumn {
            decl: column(
                "予備率",
                typed(
                    TypeKind::Float,
                    Constraints {
                        min: Some(CellValue::Float(0.0)),
                        max: Some(CellValue::Float(100.0)),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| CellValue::float(context.rng.below(200) as f64 / 2.0),
            violating: |_, _| CellValue::Float(-1.0),
        },
        // 4 検査済み: 真偽。
        SampleColumn {
            decl: column(
                "検査済み",
                typed(TypeKind::Bool, Constraints::default()),
                false,
                false,
                None,
            ),
            conforming: |context, _| CellValue::Bool(context.rng.next() % 2 == 0),
            violating: |_, _| CellValue::Text("yes".to_owned()),
        },
        // 5 発注日: 範囲つきの日付。
        SampleColumn {
            decl: column(
                "発注日",
                typed(
                    TypeKind::Date,
                    Constraints {
                        min: Some(CellValue::Text("2026-01-01".to_owned())),
                        max: Some(CellValue::Text("2026-12-31".to_owned())),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| date(&mut context.rng),
            // 正準表記に一致しない（`/` 区切り）。
            violating: |_, _| CellValue::Text("2026/09/12".to_owned()),
        },
        // 6 確定日時: オフセットを要求する日時。
        SampleColumn {
            decl: column(
                "確定日時",
                typed(
                    TypeKind::DateTime,
                    Constraints {
                        offset: Some(OffsetPolicy::Required),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| {
                CellValue::Text(format!(
                    "2026-09-12T{:02}:{:02}:00Z",
                    context.rng.below(24),
                    context.rng.below(60)
                ))
            },
            // オフセットの表記も `T` の区切りも無い（正準表記に一致しない）。
            violating: |_, _| CellValue::Text("2026-09-12 10:30:00".to_owned()),
        },
        // 7 状態: 選択肢。
        SampleColumn {
            decl: column(
                "状態",
                typed(TypeKind::Enum, choices(&STATES)),
                false,
                false,
                None,
            ),
            conforming: |context, _| choice(&STATES, &mut context.rng),
            violating: |_, _| CellValue::Text("該当なし".to_owned()),
        },
        // 8 仕入先: 参照先シートを指す参照（必須）。
        SampleColumn {
            decl: column(
                "仕入先",
                typed(
                    TypeKind::Ref,
                    Constraints {
                        sheet: Some(reference_sheet),
                        ..Constraints::default()
                    },
                ),
                true,
                false,
                None,
            ),
            conforming: |context, _| reference(&context.reference_rows, &mut context.rng),
            // 実在しない行識別子（書式としては正しい ULID）。
            violating: |_, _| CellValue::Text(UNKNOWN_REFERENCE.to_owned()),
        },
        // 9 添付: 添付参照（実体は `attachments/<hex>.bin`）。
        SampleColumn {
            decl: column(
                "添付",
                typed(TypeKind::Attachment, Constraints::default()),
                false,
                false,
                None,
            ),
            conforming: |context, _| CellValue::Attachment(context.attachment),
            violating: |_, _| CellValue::Text("!".to_owned()),
        },
        // 10 届け先: 入れ子のオブジェクト（必須）。内側に書式つき文字列・選択肢・
        // 長さつき文字列・既定値つきの任意フィールド・参照を持つ。
        SampleColumn {
            decl: column(
                "届け先",
                destination_type(reference_sheet),
                true,
                false,
                None,
            ),
            conforming: |context, row| destination(context, row),
            // 内側の 1 つのフィールド（郵便番号）だけを外す（違反は 1 件）。
            violating: |context, _| destination_with_bad_postal_code(context),
        },
        // 11 明細: 入れ子の配列（要素はインラインのオブジェクト。要素数の範囲つき）。
        SampleColumn {
            decl: column("明細", line_items_type(), false, false, None),
            conforming: |context, row| line_items(context, row),
            // 要素 0 の数量だけを外す（違反は 1 件。位置は `[0].数量`）。
            violating: |context, _| line_items_with_bad_quantity(context),
        },
        // 12 機械可読値: ANY（必須なので値なしが違反になる）。
        SampleColumn {
            decl: column(
                "機械可読値",
                typed(TypeKind::Any, Constraints::default()),
                true,
                false,
                None,
            ),
            conforming: |context, _| CellValue::Text(format!("k={}", context.rng.below(100))),
            violating: |_, _| CellValue::Null,
        },
        // 13 単位: 選択肢（既定値あり）。
        SampleColumn {
            decl: column(
                "単位",
                typed(TypeKind::Enum, choices(&UNITS)),
                false,
                false,
                Some(CellValue::Text("個".to_owned())),
            ),
            conforming: |context, _| choice(&UNITS, &mut context.rng),
            violating: |_, _| CellValue::Text("不明".to_owned()),
        },
        // 14 通貨: 選択肢（既定値あり）。
        SampleColumn {
            decl: column(
                "通貨",
                typed(TypeKind::Enum, choices(&CURRENCIES)),
                false,
                false,
                Some(CellValue::Text("JPY".to_owned())),
            ),
            conforming: |context, _| choice(&CURRENCIES, &mut context.rng),
            // 選択肢に無い通貨コード。
            violating: |_, _| CellValue::Text("円".to_owned()),
        },
        // 15 納品予定日: 範囲を宣言しない日付。
        SampleColumn {
            decl: column(
                "納品予定日",
                typed(TypeKind::Date, Constraints::default()),
                false,
                false,
                None,
            ),
            conforming: |context, _| date(&mut context.rng),
            // 存在しない暦日である（正準表記に一致しない）。
            violating: |_, _| CellValue::Text("2026-02-30".to_owned()),
        },
        // 16 登録日時: オフセットを禁ずる日時（civil）。
        SampleColumn {
            decl: column(
                "登録日時",
                typed(
                    TypeKind::DateTime,
                    Constraints {
                        offset: Some(OffsetPolicy::Forbidden),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| {
                CellValue::Text(format!(
                    "2026-09-12T{:02}:{:02}:00",
                    context.rng.below(24),
                    context.rng.below(60)
                ))
            },
            // 禁じられたオフセットを持つ。
            violating: |_, _| CellValue::Text("2026-09-12T10:30:00+09:00".to_owned()),
        },
        // 17 金額: 桁だけを宣言した 10 進数（範囲は宣言しない）。
        SampleColumn {
            decl: column(
                "金額",
                typed(
                    TypeKind::Decimal,
                    Constraints {
                        digits: decimal_digits(16, 2),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| decimal(context.rng.below(1_000_000), context.rng.below(100)),
            violating: |_, _| CellValue::Decimal("1.23456".to_owned()),
        },
        // 18 カテゴリ: 選択肢。
        SampleColumn {
            decl: column(
                "カテゴリ",
                typed(TypeKind::Enum, choices(&CATEGORIES)),
                false,
                false,
                None,
            ),
            conforming: |context, _| choice(&CATEGORIES, &mut context.rng),
            violating: |_, _| CellValue::Text("該当なし".to_owned()),
        },
        // 19 備考: 最大長だけを宣言した文字列。
        SampleColumn {
            decl: column(
                "備考",
                typed(
                    TypeKind::Text,
                    Constraints {
                        max_length: Some(200),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| CellValue::Text(format!("備考{}", context.rng.below(1_000))),
            violating: |_, _| CellValue::Text("x".repeat(201)),
        },
        // 20 社内コード: 長さと書式の両方を持つ文字列（違反は書式だけを外す）。
        SampleColumn {
            decl: column(
                "社内コード",
                typed(
                    TypeKind::Text,
                    Constraints {
                        min_length: Some(7),
                        max_length: Some(7),
                        pattern: Some("^[0-9]{4}-[0-9]{2}$".into()),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| {
                CellValue::Text(format!(
                    "{:04}-{:02}",
                    context.rng.below(10_000),
                    context.rng.below(100)
                ))
            },
            // 長さは合う（7 文字）が書式だけを外す。
            violating: |_, _| CellValue::Text("ABCDEFG".to_owned()),
        },
        // 21 ロット番号: 書式だけを持つ文字列。
        SampleColumn {
            decl: column(
                "ロット番号",
                typed(
                    TypeKind::Text,
                    Constraints {
                        pattern: Some("^LOT-[0-9]{6}$".into()),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| {
                CellValue::Text(format!("LOT-{:06}", context.rng.below(1_000_000)))
            },
            violating: |_, _| CellValue::Text("LOT-12345".to_owned()),
        },
        // 22 検査日時: オフセットを要求し、範囲も持つ日時。
        SampleColumn {
            decl: column(
                "検査日時",
                typed(
                    TypeKind::DateTime,
                    Constraints {
                        offset: Some(OffsetPolicy::Required),
                        min: Some(CellValue::Text("2026-01-01T00:00:00+09:00".to_owned())),
                        max: Some(CellValue::Text("2026-12-31T23:59:59+09:00".to_owned())),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| {
                let month = context.rng.below(12) + 1;
                let day = context.rng.below(28) + 1;
                let hour = context.rng.below(24);
                CellValue::Text(format!("2026-{month:02}-{day:02}T{hour:02}:00:00+09:00"))
            },
            // オフセットの表記が正準でない（`+0900`）。
            violating: |_, _| CellValue::Text("2026-09-12T10:30:00+0900".to_owned()),
        },
        // 23 予備数量: 範囲つきの整数（下限は 0）。
        SampleColumn {
            decl: column(
                "予備数量",
                typed(
                    TypeKind::Int,
                    Constraints {
                        min: Some(CellValue::Int(0)),
                        max: Some(CellValue::Int(1_000)),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| CellValue::Int(context.rng.below(1_001) as i64),
            violating: |_, _| CellValue::Int(-1),
        },
        // 24 予備単価: scale 4 の 10 進数。
        SampleColumn {
            decl: column(
                "予備単価",
                typed(
                    TypeKind::Decimal,
                    Constraints {
                        digits: decimal_digits(10, 4),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| {
                CellValue::Decimal(format!(
                    "{}.{:04}",
                    context.rng.below(100),
                    context.rng.below(10_000)
                ))
            },
            violating: |_, _| CellValue::Decimal("1.23456".to_owned()),
        },
        // 25 送料: 下限だけを宣言した整数。
        SampleColumn {
            decl: column(
                "送料",
                typed(
                    TypeKind::Int,
                    Constraints {
                        min: Some(CellValue::Int(0)),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| CellValue::Int(context.rng.below(500) as i64),
            violating: |_, _| CellValue::Int(-1),
        },
        // 26 承認者: 同じ参照先を指す 2 本目の参照（必須ではない）。
        SampleColumn {
            decl: column(
                "承認者",
                typed(
                    TypeKind::Ref,
                    Constraints {
                        sheet: Some(reference_sheet),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| reference(&context.reference_rows, &mut context.rng),
            violating: |_, _| CellValue::Text(UNKNOWN_REFERENCE.to_owned()),
        },
        // 27 改訂履歴: 入れ子の配列（要素はインラインのオブジェクト。日付を持つ）。
        SampleColumn {
            decl: column("改訂履歴", revisions_type(), false, false, None),
            conforming: |context, row| revisions(context, row),
            // 要素 0 の版だけを外す（違反は 1 件。位置は `[0].版`）。
            violating: |context, _| revisions_with_bad_version(context),
        },
        // 28 予備区分: 選択肢。
        SampleColumn {
            decl: column(
                "予備区分",
                typed(TypeKind::Enum, choices(&SPARE_GRADES)),
                false,
                false,
                None,
            ),
            conforming: |context, _| choice(&SPARE_GRADES, &mut context.rng),
            violating: |_, _| CellValue::Text("Z".to_owned()),
        },
        // 29 補助コード: 書式つきの文字列（入れ子の商品コードと別の書式）。
        SampleColumn {
            decl: column(
                "補助コード",
                typed(
                    TypeKind::Text,
                    Constraints {
                        pattern: Some("^[A-Z]{2}[0-9]{4}$".into()),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            conforming: |context, _| {
                CellValue::Text(format!(
                    "{}{:04}",
                    ["AB", "CD", "EF", "GH"][context.rng.below(4)],
                    context.rng.below(10_000)
                ))
            },
            violating: |_, _| CellValue::Text("!!".to_owned()),
        },
    ]
}

/// 実在しない行識別子（書式としては正しい ULID。参照の違反を 1 件だけ生む）。
const UNKNOWN_REFERENCE: &str = "99999999999999999999999999";

/// `届け先` の型（入れ子のオブジェクト。書式・選択肢・長さ・既定値・参照を内側に持つ）。
fn destination_type(reference_sheet: SheetId) -> TypeDecl {
    typed(
        TypeKind::Object,
        Constraints {
            fields: vec![
                field(
                    DESTINATION_FIELDS[0],
                    typed(
                        TypeKind::Text,
                        Constraints {
                            pattern: Some("^[0-9]{3}-[0-9]{4}$".into()),
                            ..Constraints::default()
                        },
                    ),
                    true,
                    None,
                ),
                field(
                    DESTINATION_FIELDS[1],
                    typed(TypeKind::Enum, choices(&PREFECTURES)),
                    true,
                    None,
                ),
                field(
                    DESTINATION_FIELDS[2],
                    typed(
                        TypeKind::Text,
                        Constraints {
                            max_length: Some(64),
                            ..Constraints::default()
                        },
                    ),
                    true,
                    None,
                ),
                field(
                    DESTINATION_FIELDS[3],
                    typed(
                        TypeKind::Text,
                        Constraints {
                            max_length: Some(8),
                            ..Constraints::default()
                        },
                    ),
                    false,
                    Some(CellValue::Text("本館".to_owned())),
                ),
                // 入れ子の内側の参照（検証は内側へ降りて判定する）。
                field(
                    DESTINATION_FIELDS[4],
                    typed(
                        TypeKind::Ref,
                        Constraints {
                            sheet: Some(reference_sheet),
                            ..Constraints::default()
                        },
                    ),
                    false,
                    None,
                ),
            ],
            ..Constraints::default()
        },
    )
}

/// `明細` の型（入れ子の配列。要素はインラインのオブジェクト）。
fn line_items_type() -> TypeDecl {
    typed(
        TypeKind::Array,
        Constraints {
            items: Some(Box::new(typed(
                TypeKind::Object,
                Constraints {
                    fields: vec![
                        field(
                            LINE_FIELDS[0],
                            typed(
                                TypeKind::Text,
                                Constraints {
                                    pattern: Some("^ITEM-[0-9]{4}$".into()),
                                    ..Constraints::default()
                                },
                            ),
                            true,
                            None,
                        ),
                        field(
                            LINE_FIELDS[1],
                            typed(
                                TypeKind::Int,
                                Constraints {
                                    min: Some(CellValue::Int(1)),
                                    max: Some(CellValue::Int(100)),
                                    ..Constraints::default()
                                },
                            ),
                            true,
                            None,
                        ),
                        field(
                            LINE_FIELDS[2],
                            typed(
                                TypeKind::Decimal,
                                Constraints {
                                    digits: decimal_digits(12, 2),
                                    ..Constraints::default()
                                },
                            ),
                            true,
                            None,
                        ),
                    ],
                    ..Constraints::default()
                },
            ))),
            min_items: Some(1),
            max_items: Some(8),
            ..Constraints::default()
        },
    )
}

/// `改訂履歴` の型（入れ子の配列。要素は日付を持つインラインのオブジェクト）。
fn revisions_type() -> TypeDecl {
    typed(
        TypeKind::Array,
        Constraints {
            items: Some(Box::new(typed(
                TypeKind::Object,
                Constraints {
                    fields: vec![
                        field(
                            REVISION_FIELDS[0],
                            typed(TypeKind::Date, Constraints::default()),
                            true,
                            None,
                        ),
                        field(
                            REVISION_FIELDS[1],
                            typed(
                                TypeKind::Int,
                                Constraints {
                                    min: Some(CellValue::Int(1)),
                                    ..Constraints::default()
                                },
                            ),
                            true,
                            None,
                        ),
                        field(
                            REVISION_FIELDS[2],
                            typed(
                                TypeKind::Text,
                                Constraints {
                                    max_length: Some(32),
                                    ..Constraints::default()
                                },
                            ),
                            false,
                            None,
                        ),
                    ],
                    ..Constraints::default()
                },
            ))),
            max_items: Some(4),
            ..Constraints::default()
        },
    )
}

/// 上流が不透明なペイロードとして保持するエンベロープ（`{ "root": …, "types": [] }`）。
///
/// ルート宣言の正準出力は `schema-engine` の `declaration::codec` が行う（本モジュールは
/// 宣言テキストの文法を再実装しない）。名前つき型定義を使わない（入れ子はすべてインラインで
/// 宣言する）ため、型定義の配列は空である。
fn envelope(columns: &[SampleColumn]) -> String {
    let schema = Schema {
        columns: columns.iter().map(|column| column.decl.clone()).collect(),
    };
    let root = schema_to_text(&schema).expect("標本の宣言は正準出力できる");
    format!(r#"{{"root":{root},"types":[]}}"#)
}

/// 列名だけから宣言を組む（**参照先のシート**のように、種別が `Text` だけのシートに使う）。
fn envelope_text(names: &[String]) -> String {
    let schema = Schema {
        columns: names
            .iter()
            .map(|name| {
                column(
                    name,
                    typed(TypeKind::Text, Constraints::default()),
                    true,
                    false,
                    None,
                )
            })
            .collect(),
    };
    let root = schema_to_text(&schema).expect("標本の宣言は正準出力できる");
    format!(r#"{{"root":{root},"types":[]}}"#)
}

/// 種別による型の宣言。
fn typed(kind: TypeKind, constraints: Constraints) -> TypeDecl {
    TypeDecl::Kind {
        kind: DeclaredKind::Known(kind),
        constraints,
    }
}

/// 列の宣言（説明は標本では省く）。
fn column(
    name: &str,
    ty: TypeDecl,
    required: bool,
    unique: bool,
    default: Option<CellValue>,
) -> ColumnDecl {
    ColumnDecl {
        name: name.into(),
        ty,
        required,
        unique,
        default,
        description: None,
    }
}

/// 入れ子のフィールドの宣言。
fn field(name: &str, ty: TypeDecl, required: bool, default: Option<CellValue>) -> FieldDecl {
    FieldDecl {
        name: name.into(),
        ty,
        required,
        default,
        description: None,
    }
}

/// 選択肢の制約。
fn choices(values: &[&str]) -> Constraints {
    Constraints {
        choices: values.iter().map(|value| Box::from(*value)).collect(),
        ..Constraints::default()
    }
}

/// 10 進数の桁の制約（設計の `precision` / `scale`）。
fn decimal_digits(precision: u32, scale: u32) -> Option<DecimalDigits> {
    Some(DecimalDigits::new(precision, scale).expect("標本の桁の宣言は妥当"))
}

/// 選択肢から 1 つを擬似乱数で選ぶ。
fn choice(values: &[&str], rng: &mut Rng) -> CellValue {
    CellValue::Text(values[rng.below(values.len())].to_owned())
}

/// `{整数}.{小数 2 桁}` の 10 進数の値。
fn decimal(integer: usize, fraction: usize) -> CellValue {
    CellValue::Decimal(format!("{integer}.{fraction:02}"))
}

/// 2026 年の実在する日付（日は 28 日以下に限るため、どの月でも正しい暦日になる）。
fn date(rng: &mut Rng) -> CellValue {
    CellValue::Text(format!(
        "2026-{:02}-{:02}",
        rng.below(12) + 1,
        rng.below(28) + 1
    ))
}

/// 参照先シートの行識別子（擬似乱数で行を選ぶ）。
fn reference(rows: &[RowId], rng: &mut Rng) -> CellValue {
    CellValue::Text(rows[rng.below(rows.len())].to_string())
}

/// `届け先`（入れ子のオブジェクト）に適合する値。
fn destination(context: &mut SampleContext, _row: usize) -> CellValue {
    CellValue::Nested(NestedValue::Object(vec![
        (
            DESTINATION_FIELDS[0].to_owned(),
            CellValue::Text(format!(
                "{:03}-{:04}",
                context.rng.below(1_000),
                context.rng.below(10_000)
            )),
        ),
        (
            DESTINATION_FIELDS[1].to_owned(),
            choice(&PREFECTURES, &mut context.rng),
        ),
        (
            DESTINATION_FIELDS[2].to_owned(),
            CellValue::Text(format!(
                "架空市{}丁目{}番{}号",
                context.rng.below(100),
                context.rng.below(50),
                context.rng.below(20)
            )),
        ),
        (
            DESTINATION_FIELDS[3].to_owned(),
            CellValue::Text(format!("{}号館", context.rng.below(9) + 1)),
        ),
        (
            DESTINATION_FIELDS[4].to_owned(),
            reference(&context.reference_rows, &mut context.rng),
        ),
    ]))
}

/// `届け先` の違反する値（郵便番号だけを外す。違反は `[郵便番号]` の 1 件）。
fn destination_with_bad_postal_code(context: &mut SampleContext) -> CellValue {
    CellValue::Nested(NestedValue::Object(vec![
        (
            DESTINATION_FIELDS[0].to_owned(),
            // ハイフンの無い 7 桁（書式だけを外す）。
            CellValue::Text("1000001".to_owned()),
        ),
        (
            DESTINATION_FIELDS[1].to_owned(),
            CellValue::Text(PREFECTURES[0].to_owned()),
        ),
        (
            DESTINATION_FIELDS[2].to_owned(),
            CellValue::Text("架空市1丁目1番1号".to_owned()),
        ),
        (
            DESTINATION_FIELDS[3].to_owned(),
            CellValue::Text("1号館".to_owned()),
        ),
        (
            DESTINATION_FIELDS[4].to_owned(),
            reference(&context.reference_rows, &mut context.rng),
        ),
    ]))
}

/// `明細` の要素（インラインのオブジェクト）に適合する値。
fn line_item(rng: &mut Rng, index: usize) -> CellValue {
    CellValue::Nested(NestedValue::Object(vec![
        (
            LINE_FIELDS[0].to_owned(),
            CellValue::Text(format!("ITEM-{:04}", rng.below(10_000))),
        ),
        (
            LINE_FIELDS[1].to_owned(),
            CellValue::Int(rng.below(100) as i64 + 1),
        ),
        (
            LINE_FIELDS[2].to_owned(),
            // 要素ごとに整数部をずらして、同じ行の要素が同じ値にならないようにする。
            decimal(rng.below(10_000), (index * 7) % 100),
        ),
    ]))
}

/// `明細` に適合する値（要素 2 個。位置の検査が固定できる形）。
fn line_items(context: &mut SampleContext, _row: usize) -> CellValue {
    CellValue::Nested(NestedValue::Array(vec![
        line_item(&mut context.rng, 0),
        line_item(&mut context.rng, 1),
    ]))
}

/// `明細` の違反する値（要素 0 の数量だけを外す。違反は `[0].数量` の 1 件）。
fn line_items_with_bad_quantity(_context: &mut SampleContext) -> CellValue {
    CellValue::Nested(NestedValue::Array(vec![
        CellValue::Nested(NestedValue::Object(vec![
            (
                LINE_FIELDS[0].to_owned(),
                CellValue::Text("ITEM-0000".to_owned()),
            ),
            // 下限 1 を外す。
            (LINE_FIELDS[1].to_owned(), CellValue::Int(0)),
            (LINE_FIELDS[2].to_owned(), decimal(1, 0)),
        ])),
        CellValue::Nested(NestedValue::Object(vec![
            (
                LINE_FIELDS[0].to_owned(),
                CellValue::Text("ITEM-0001".to_owned()),
            ),
            (LINE_FIELDS[1].to_owned(), CellValue::Int(1)),
            (LINE_FIELDS[2].to_owned(), decimal(1, 0)),
        ])),
    ]))
}

/// `改訂履歴` の要素（インラインのオブジェクト）に適合する値。
fn revision(rng: &mut Rng, version: usize) -> CellValue {
    CellValue::Nested(NestedValue::Object(vec![
        (REVISION_FIELDS[0].to_owned(), date(rng)),
        (
            REVISION_FIELDS[1].to_owned(),
            CellValue::Int(version as i64),
        ),
        (
            REVISION_FIELDS[2].to_owned(),
            CellValue::Text(format!("改訂{}", rng.below(100))),
        ),
    ]))
}

/// `改訂履歴` に適合する値（要素 2 個。位置の検査が固定できる形）。
fn revisions(context: &mut SampleContext, _row: usize) -> CellValue {
    CellValue::Nested(NestedValue::Array(vec![
        revision(&mut context.rng, 1),
        revision(&mut context.rng, 2),
    ]))
}

/// `改訂履歴` の違反する値（要素 0 の版だけを外す。違反は `[0].版` の 1 件）。
fn revisions_with_bad_version(_context: &mut SampleContext) -> CellValue {
    CellValue::Nested(NestedValue::Array(vec![
        CellValue::Nested(NestedValue::Object(vec![
            (
                REVISION_FIELDS[0].to_owned(),
                CellValue::Text("2026-01-01".to_owned()),
            ),
            // 下限 1 を外す。
            (REVISION_FIELDS[1].to_owned(), CellValue::Int(0)),
            (
                REVISION_FIELDS[2].to_owned(),
                CellValue::Text("改訂0".to_owned()),
            ),
        ])),
        CellValue::Nested(NestedValue::Object(vec![
            (
                REVISION_FIELDS[0].to_owned(),
                CellValue::Text("2026-01-02".to_owned()),
            ),
            (REVISION_FIELDS[1].to_owned(), CellValue::Int(2)),
            (
                REVISION_FIELDS[2].to_owned(),
                CellValue::Text("改訂1".to_owned()),
            ),
        ])),
    ]))
}

/// 値の中身を決める小さな擬似乱数（xorshift64 と 64 ビットの乗算）。
///
/// 依存を増やさないため本モジュールで完結させる（`rand` クレートを持ち込まない）。
/// **同じ種からは常に同じ列**を返し、状態は 0 にならない（0 を渡されたときは既定の
/// 非 0 の定数へ置き換える）— xorshift は状態 0 で不動点になるためである。
struct Rng {
    state: u64,
}

impl Rng {
    /// 種から作る。
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// 次の 64 ビットを取り出す（xorshift64* の 1 段）。
    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// `0..bound` の値を 1 つ取り出す（`bound == 0` は 0 を返す。panic しない）。
    fn below(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            return 0;
        }
        (self.next() % bound as u64) as usize
    }
}

/// 1 シート分の行データを `sheets/<sheet-ulid>.jsonl` のバイト列へ符号化し、発行した
/// 行識別子を返す。
///
/// [`Document`] は公開 API では行を 1 件ずつしか足せない（`add_row` + `set_row_values`）。
/// `set_row_values` は行を線形探索するため、10 万行では O(n²) になり分単位かかる。本生成器は
/// 行エントリのバイト列だけを組み立てて `from_parts` の一括経路（`Sheet::extend_rows`）で
/// 復元する。これにより行数に対して線形である（`document-format` と `schema-engine` の
/// 大量標本が同じ理由で同じ迂回路を使う）。
///
/// セル値の符号化は [`to_json_bytes`]（value 層の単一の源）に任せる。列名は利用者が書く
/// 任意のテキストでありうるため、[`push_json_string`] で正しく逃がす。
fn encode_rows<F>(
    sheet: SheetId,
    columns: &[String],
    rows: usize,
    mut value_at: F,
) -> (EntryName, Vec<u8>, Vec<RowId>)
where
    F: FnMut(usize, usize) -> CellValue,
{
    let entry = EntryName::Rows { sheet };
    let location = entry.to_string();
    let mut ids = IdFactory::new();
    let mut row_ids = Vec::with_capacity(rows);
    let mut out = Vec::with_capacity(rows * (40 + columns.len() * 12));
    for row in 0..rows {
        let id = ids.new_row_id();
        row_ids.push(id);
        out.extend_from_slice(b"{\"$id\":");
        push_json_string(&mut out, &id.to_string());
        for (column, name) in columns.iter().enumerate() {
            out.push(b',');
            push_json_string(&mut out, name);
            out.push(b':');
            let bytes = to_json_bytes(&value_at(row, column), &location)
                .expect("標本のセル値は符号化できる");
            out.extend_from_slice(&bytes);
        }
        out.extend_from_slice(b"}\n");
    }
    (entry, out, row_ids)
}

/// JSON 文字列リテラル（前後の `"` を含む）を `out` へ書く。
///
/// 依存を増やさないため本モジュールで完結させる。`"`・`\`・制御文字を逃がし、それ以外は
/// UTF-8 のまま書く。
fn push_json_string(out: &mut Vec<u8>, text: &str) {
    out.push(b'"');
    for character in text.chars() {
        match character {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\r' => out.extend_from_slice(b"\\r"),
            '\t' => out.extend_from_slice(b"\\t"),
            '\u{08}' => out.extend_from_slice(b"\\b"),
            '\u{0c}' => out.extend_from_slice(b"\\f"),
            control if (control as u32) < 0x20 => {
                out.extend_from_slice(format!("\\u{:04x}", control as u32).as_bytes());
            }
            other => {
                let mut buffer = [0u8; 4];
                out.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    out.push(b'"');
}

/// 骨格のエントリの 1 つを差し替える（行データ）。
fn replace_entry(entries: &mut [(EntryName, Vec<u8>)], name: EntryName, bytes: Vec<u8>) {
    let slot = entries
        .iter()
        .position(|(known, _)| *known == name)
        .expect("骨格は差し替えるエントリを持つ");
    entries[slot].1 = bytes;
}

/// 索引（`manifest.json`）を実体から組み直したエントリ集合を返す。
///
/// 行エントリのバイト列を差し替えると索引のダイジェストが古くなるため、復元の前に組み直す
/// （`from_parts` はダイジェストを照合する）。`document-format` と `schema-engine` の
/// 大量標本にある同名の補助と同じ手順である。
fn with_rebuilt_manifest(
    version: FormatVersion,
    mut entries: Vec<(EntryName, Vec<u8>)>,
) -> Vec<(EntryName, Vec<u8>)> {
    entries.retain(|(name, _)| *name != EntryName::Manifest);
    let index: Vec<ManifestEntry> = entries
        .iter()
        .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
        .collect();
    let manifest = ManifestPart::new(version, index)
        .expect("標本の索引は妥当")
        .to_json_bytes()
        .expect("索引は符号化できる");
    entries.push((EntryName::Manifest, manifest));
    entries
}

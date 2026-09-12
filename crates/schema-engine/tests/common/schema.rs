//! 現実に寄せた 30 列の標本（tasks.md 9.1。要件 10.6）。
//!
//! 親モジュール [`super`]（タスク 1.4）の生成器は上流のドキュメントモデルとセル値だけを
//! 知り、宣言を必要とする標本を組み立てない。本モジュールはその生成器の上に**型つきの
//! 30 列**を載せる。群 9 の 3 つの検証 — 9.2 の予算の計測（`benches/large_sheet.rs`）・
//! 9.3 の違反の順序の決定性・9.4 の拡張型の隔離と一括経路 — は**この同じ標本**を共有する
//! （tasks.md 9.1）。入口は [`schema_sample`] 1 つであり、行数・違反の割合・一意制約の
//! 重複の有無・一括判定の失敗の有無だけが [`SchemaSampleOptions`] で変わる。
//!
//! # 何を標本に含めるか（tasks.md 9.1 の列挙）
//!
//! 10 進数（桁つき・範囲つき）・日時（オフセットを要求／禁ずる・範囲つき）・書式つき
//! 文字列・シート間参照（最上位と入れ子の内側）・入れ子（名前つき型定義への参照と、
//! 配列の要素）・拡張型（**一括判定を上書きした実装**と**既定実装のままの実装**の両方）を
//! 30 列の中に置く。列の並びは「発注明細」という 1 シートの現実の形に寄せてあり、
//! 品番（必須かつ一意）・数量・単価・金額・日付・参照・状態・備考のような列が並ぶ。
//!
//! 標本の拡張型の実装は 2 つあり、それぞれ別の列が使う（[`BATCH_CUSTOM_COLUMN`] と
//! [`SIMPLE_CUSTOM_COLUMN`]）。**一括判定が `Err` を返す経路を踏む列**は
//! [`SchemaSampleOptions::with_batch_failure`] で選ぶ — 既定は既定実装のままの列であり、
//! 9.4 はこの切り替えで**一括判定を上書きした実装**の失敗隔離を見る。
//!
//! # 適合する値は 1 件も違反を生まず、違反する値はちょうど 1 件を生む
//!
//! [`SampleColumn`] が列ごとに 2 つの規則（`conforming` / `violating`）を持ち、
//! [`schema_sample`] が違反の位置にだけ後者を置く。この不変条件が、9.1 の検証
//! （違反の件数と位置が仕込んだとおりであること）と 9.3 の決定性（違反の種類が混ざる
//! こと）の土台である。**違反する値は 1 セルにつき 1 件**でなければならない —
//! 入れ子の列には形の違う値を置き（内側へ降りないので 1 件）、選択肢・桁・書式のような
//! 複数の制約を持つ列には、そのうち 1 つだけを外す値を置く。2 つの拡張型の値も同じ規則に
//! 従う（実装の拒否も判定の失敗も 1 セルにつき 1 件である）。
//!
//! # 違反の位置は列優先の平坦添字で選ぶ
//!
//! 1.4 の [`ViolationPlan`] は全セルの平坦添字を等間隔に選ぶ。**行優先**
//! （`row * 列数 + column`）の添字でそのまま選ぶと、30 列の標本では間隔（10 万行 × 30 列で
//! 3,000 件なら 1,000）が列数の倍数になり（`gcd(1000, 30) = 10`）、違反が添字の
//! 列数の剰余で同じ位置に落ちる 3 列にしか現れない — 残り 27 列の違反の規則が死ぬ。
//! そこで**列優先**の添字（`column * 行数 + row`）で選び、各列が同じ割合を受け取るように
//! する（[`flat_index`]）。
//!
//! # 2 つのシートを 1 回の復元で組む
//!
//! `ref` 列の適合する値は参照先シートの行識別子であり、その識別子は参照先の行を
//! 符号化した時点で初めて決まる。本モジュールは骨格の文書に 2 つのシート（発注明細と
//! 取引先）を置き、両方の行エントリを差し替えてから 1 回で復元する。行の符号化と索引の
//! 組み直しは親モジュールの [`encode_rows`] / [`with_rebuilt_manifest`] をそのまま使う
//! （10 万行を O(n) で組む迂回路を写し取らない。タスク 1.4 の docs）。
//!
//! # 決定性 — 何が同一で、何が同一でないか
//!
//! **同じ引数の 2 回の [`schema_sample`] は、バイト列としては一致しない。** セル値にも
//! 宣言にも、`document-format` が発行時刻を含めて発行する識別子が現れるためである —
//! 参照列の適合する値は参照先シートの行の ULID であり、標本の型定義の識別子も組み立ての
//! たびに発行される。どちらも標本側で固定できない（公開経路に「識別子を指定して行を作る」
//! 口が無い）。
//!
//! 同一なのは**形**と**違反の集合**である:
//!
//! - **形**（列数・行数・違反として仕込むセルの数・宣言の構造）は同じ引数で常に同じである。
//! - **違反の集合**は、**行添字・列添字・入れ子の位置と、その位置の理由の種別**で見れば
//!   常に同じである。違反の位置は [`ViolationPlan`] の仕込み位置と列の規則だけから決まり、
//!   識別子の値に依らない。
//! - 一方、`ViolationReason` が運ぶ実際の値と、参照先シートの識別子・重複する行の識別子は、
//!   参照列と型定義 id を運ぶ列では実行ごとに異なる。
//!
//! **9.3 が依拠してよい性質はこれである**: 違反の位置と理由の種別の並びは、実行とプロセスを
//! 跨いで同一であり、比較してよい。参照列と型定義 id を運ぶ**値そのもの**は実行ごとに異なる
//! ため、比較してはならない（生の値を突き合わせると必ず食い違う）。

use std::sync::Arc;

use document_format::parts::{to_parts, DocumentParts};
use document_format::{
    AttachmentId, CellValue, Document, DocumentFormat, DocumentFormatApi, EntryName, IdFactory,
    NestedValue, RowId, SchemaPart, SheetId, TypeDefId,
};
use schema_engine::{
    schema_to_text, type_definition_to_text, ColumnDecl, CompiledSchema, Constraints, CustomType,
    CustomTypeFailure, CustomTypeId, CustomVerdict, DecimalDigits, DeclaredKind, FieldDecl,
    OffsetPolicy, Schema, SchemaEngine, SchemaEngineApi, TypeDecl, TypeKind, TypeRegistry,
};

use super::{encode_rows, with_rebuilt_manifest, SampleSpec, ViolationPlan};

/// 標本の既定の行数（要件 10.6「1 シートにつき合計 10 万行」）。
pub const SAMPLE_ROWS: usize = 100_000;

/// 標本の列数（design.md「Performance」の 10 万行 × 30 列）。
pub const SAMPLE_COLUMNS: usize = 30;

/// 標本の既定の違反の割合（全セルの 0.1%。10 万行 × 30 列で 3,000 件）。
pub const SAMPLE_VIOLATION_RATIO: f64 = 0.001;

/// 参照先シート（`取引先`）の行数。`ref` 列の値はこの行を指す。
pub const REFERENCE_ROWS: usize = 64;

/// 一意制約の重複を混ぜるときの値の種類（[`SchemaSampleOptions::with_unique_collisions`]）。
///
/// 品番の値がこの個数を巡回するため、重複した値がこの個数だけ生じる
/// （10 万行なら 1 値あたり 100 行）。
pub const UNIQUE_GROUPS: usize = 1_000;

/// 標本のデータシート（30 列を持つ側）。
const SHEET_NAME: &str = "発注明細";
/// 標本の参照先シート。
const REFERENCE_SHEET_NAME: &str = "取引先";
/// 参照先シートの列名。
const REFERENCE_FIELDS: [&str; 2] = ["名称", "部門"];
/// 添付の内容。識別子は内容から決まる（content-addressed）。
const ATTACHMENT_BYTES: &[u8] =
    b"9.1 \xe3\x81\xae\xe6\xa8\x99\xe6\x9c\xac\xe3\x81\xae\xe6\xb7\xbb\xe4\xbb\x98";

/// 入れ子の型定義（`届け先`）のフィールド名。宣言と値の両方がこの並びを使う
/// （名前が食い違うと、宣言されたフィールドが値に無いものとして扱われる）。
const DESTINATION_FIELDS: [&str; 5] = ["郵便番号", "都道府県", "住所", "建物", "最寄り倉庫"];

/// 入れ子の配列（`明細`）の要素のフィールド名。
const LINE_FIELDS: [&str; 3] = ["商品コード", "数量", "単価"];

/// 入れ子の配列（`改訂履歴`）の要素のフィールド名。
const REVISION_FIELDS: [&str; 3] = ["改訂日", "版", "理由"];

/// 標本用の拡張型の識別子（**一括判定を上書きした**実装。要件 11.6 の代役）。
pub const BATCH_CUSTOM_ID: &str = "sample-postal-code-batch";

/// 標本用の拡張型の識別子（**既定実装のまま**の実装）。
pub const SIMPLE_CUSTOM_ID: &str = "sample-postal-code-simple";

/// 一括判定を上書きした実装（[`BATCH_CUSTOM_ID`]）を型に持つ列の添字（`届け先郵便番号`）。
///
/// 9.4 が失敗の位置を仕込み位置と突き合わせるのに使う（[`schema_sample`] が宣言と一致する
/// ことを表明している）。
pub const BATCH_CUSTOM_COLUMN: usize = 27;

/// 既定実装のままの実装（[`SIMPLE_CUSTOM_ID`]）を型に持つ列の添字（`請求先郵便番号`）。
///
/// 入れ子の `届け先.郵便番号` も同じ実装を使うが、こちらは最上位の列である。
pub const SIMPLE_CUSTOM_COLUMN: usize = 28;

/// 一意制約を持つ列の添字（`品番`。標本で唯一の `unique` 列）。
const UNIQUE_COLUMN: usize = 0;

/// 標本の作り方（tasks.md 9.1）。
#[derive(Debug, Clone, Copy)]
pub struct SchemaSampleOptions {
    /// 行数。
    pub rows: usize,
    /// 違反として仕込むセルの割合（[`ViolationPlan::ratio`] に渡す）。
    pub ratio: f64,
    /// 一意制約の列に重複を混ぜるか（9.3 が値の違反と一意制約の違反の両方を出すのに使う）。
    pub unique_collisions: bool,
    /// 一括判定を上書きした実装の列で**判定の失敗（`Err`）**を起こすか
    /// （9.4 が一括実装の失敗隔離を見るのに使う）。
    pub batch_failure: bool,
}

impl SchemaSampleOptions {
    /// 行数を指定し、違反の割合と一意制約の重複は既定（0.1%、重複なし）にする。
    pub const fn new(rows: usize) -> Self {
        Self {
            rows,
            ratio: SAMPLE_VIOLATION_RATIO,
            unique_collisions: false,
            batch_failure: false,
        }
    }

    /// 違反として仕込むセルの割合を変える。
    pub const fn with_ratio(mut self, ratio: f64) -> Self {
        self.ratio = ratio;
        self
    }

    /// 一意制約の列に重複を混ぜる（9.3 が跨る経路の決定性を検査するのに使う）。
    pub const fn with_unique_collisions(mut self, collisions: bool) -> Self {
        self.unique_collisions = collisions;
        self
    }

    /// 一括判定を上書きした実装（[`BATCH_CUSTOM_ID`]）の列に、**判定の失敗を返す値**を
    /// 違反として仕込む（9.4 が一括実装の失敗隔離を見るのに使う）。
    ///
    /// 既定（`false`）は既定実装のままの実装（[`SIMPLE_CUSTOM_ID`]）の列で失敗を起こす。
    /// どちらの設定でも標本は `CustomRejected` と `CustomFailed` の両方を含む — 失敗を
    /// 起こす列が入れ替わるだけで、2 つの値の規則（1 セルにつき違反 1 件）は変わらない。
    pub const fn with_batch_failure(mut self, failure: bool) -> Self {
        self.batch_failure = failure;
        self
    }
}

impl Default for SchemaSampleOptions {
    /// 設計と要件が定める標本そのもの（10 万行 × 30 列、違反 0.1%、重複なし）。
    fn default() -> Self {
        Self::new(SAMPLE_ROWS)
    }
}

/// 組み立てられた 30 列の標本（tasks.md 9.1。要件 10.6）。
///
/// 文書（2 シート）・拡張型を登録した台帳・行識別子・仕込んだ違反の内訳を持つ。
/// 消費側は文書と台帳をそのまま検証の入口へ渡せる（[`SchemaSample::compiled`] が
/// 計画まで作る。9.2 / 9.3 / 9.4）。
pub struct SchemaSample {
    document: Document,
    sheet: SheetId,
    registry: TypeRegistry,
    row_ids: Vec<RowId>,
    columns: Vec<String>,
    plan: ViolationPlan,
    duplicates: usize,
}

impl SchemaSample {
    /// 標本の文書（データシートと参照先シート）。
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// 30 列を持つデータシートの識別子。
    pub fn sheet(&self) -> SheetId {
        self.sheet
    }

    /// 標本の拡張型を登録した台帳（9.4 が一括経路と失敗の隔離を見るのに使う）。
    pub fn registry(&self) -> &TypeRegistry {
        &self.registry
    }

    /// 行識別子（行順）。
    pub fn row_ids(&self) -> &[RowId] {
        &self.row_ids
    }

    /// 行数。
    pub fn rows(&self) -> usize {
        self.row_ids.len()
    }

    /// 列名（宣言の並びそのもの。行データのキー順になる）。
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// 行添字と列添字のセルを違反として仕込んだか（[`flat_index`] の選定規則）。
    pub fn violation_at(&self, row: usize, column: usize) -> bool {
        self.plan.is_violation(flat_index(row, column, self.rows()))
    }

    /// 違反として仕込んだセルの数（1 セルにつき違反 1 件）。
    pub fn injected_violations(&self) -> usize {
        self.plan.count()
    }

    /// 一意制約の重複によって生じる違反の数（既定は 0）。
    pub fn unique_violations(&self) -> usize {
        self.duplicates
    }

    /// この標本が生む違反の総数（値の違反 + 一意制約の違反）。
    pub fn expected_violations(&self) -> usize {
        self.injected_violations() + self.unique_violations()
    }

    /// 標本の宣言から計画を組み立てる（`schema-engine` の公開面だけを使う）。
    pub fn compiled(&self) -> CompiledSchema {
        let sheet = self
            .document
            .sheet_by_id(self.sheet)
            .expect("標本のシートは文書にある");
        SchemaEngine::new()
            .compile(sheet, &self.registry)
            .expect("標本の宣言はコンパイルできる")
    }
}

/// 30 列の標本を組み立てる（tasks.md 9.1。要件 10.6）。
///
/// 同じ引数なら**形**（列数・行数・仕込む違反の数・宣言の構造）と**違反の集合**（行添字・
/// 列添字・入れ子の位置と理由の種別）が常に同じになる。バイト列は一致しない —
/// 参照列の適合する値は参照先シートの行の ULID であり、型定義の識別子も組み立てのたびに
/// 発行されるためである（モジュール docs「決定性 — 何が同一で、何が同一でないか」）。
pub fn schema_sample(options: &SchemaSampleOptions) -> SchemaSample {
    let mut skeleton = Document::new();
    let sheet = skeleton.add_sheet(SHEET_NAME);
    let reference = skeleton.add_sheet(REFERENCE_SHEET_NAME);

    // 参照先シートを先に符号化して行識別子を得る（`ref` 列の適合する値がこれを要る）。
    let reference_spec = SampleSpec::new(REFERENCE_ROWS, REFERENCE_FIELDS.len())
        .with_sheet_name(REFERENCE_SHEET_NAME)
        .with_column_names(
            REFERENCE_FIELDS
                .iter()
                .map(|name| name.to_string())
                .collect(),
        );
    skeleton
        .set_sheet_columns(reference, reference_spec.columns().to_vec())
        .expect("標本の参照先シートは骨格にある");
    let (reference_entry, reference_bytes, reference_rows) =
        encode_rows(reference, &reference_spec, |row, column| match column {
            0 => CellValue::Text(format!("取引先{:02}", row % 10)),
            _ => CellValue::Text(["購買", "資材", "経理", "物流"][row % 4].to_owned()),
        });

    let context = SampleContext {
        reference,
        reference_rows,
        attachment: AttachmentId::from_bytes(ATTACHMENT_BYTES),
        definition: IdFactory::new().new_type_def_id(),
        unique_collisions: options.unique_collisions,
        batch_failure: options.batch_failure,
    };
    let columns = sample_columns(&context);
    let names: Vec<String> = columns
        .iter()
        .map(|column| column.decl.name.to_string())
        .collect();
    assert_eq!(SAMPLE_COLUMNS, names.len(), "標本の列数が 30 でない");
    assert!(
        columns[UNIQUE_COLUMN].decl.unique,
        "一意制約を持つ列の添字が標本と食い違っている"
    );
    assert_eq!(
        Some(BATCH_CUSTOM_ID),
        custom_type_of(&columns[BATCH_CUSTOM_COLUMN].decl.ty),
        "一括判定を上書きした拡張型の列の添字が標本と食い違っている"
    );
    assert_eq!(
        Some(SIMPLE_CUSTOM_ID),
        custom_type_of(&columns[SIMPLE_CUSTOM_COLUMN].decl.ty),
        "既定実装のままの拡張型の列の添字が標本と食い違っている"
    );

    skeleton
        .set_sheet_columns(sheet, names.clone())
        .expect("標本のシートは骨格にある");
    skeleton
        .set_root_schema(
            sheet,
            SchemaPart::parse(&envelope(&context, &columns)).expect("標本のエンベロープは妥当"),
        )
        .expect("標本のシートは骨格にある");

    // 行の値は列ごとの 2 つの規則から決まる（適合する値と、違反を 1 件だけ生む値）。
    let spec = SampleSpec::new(options.rows, names.len()).with_column_names(names.clone());
    let plan = ViolationPlan::ratio(spec.cell_count(), options.ratio);
    let rows = options.rows;
    let (rows_entry, rows_bytes, row_ids) = encode_rows(sheet, &spec, |row, column| {
        if plan.is_violation(flat_index(row, column, rows)) {
            (columns[column].violating)(row, &context)
        } else {
            (columns[column].conforming)(row, &context)
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

    SchemaSample {
        document,
        sheet,
        registry: sample_registry(),
        row_ids,
        columns: names,
        plan,
        duplicates: if options.unique_collisions {
            duplicate_groups(rows, &plan)
        } else {
            0
        },
    }
}

/// セルの**列優先**の平坦添字（モジュール docs「違反の位置は列優先の平坦添字で選ぶ」）。
///
/// 行優先の添字をそのまま [`ViolationPlan`] へ渡すと、30 列では等間隔の選定が列数の
/// 倍数になり、違反が一部の列に集中する。列優先にすると各列が同じ割合を受け取る。
fn flat_index(row: usize, column: usize, rows: usize) -> usize {
    column * rows + row
}

/// 一意制約の重複によって生じる違反の数（標本の品番の値の巡回から決まる）。
///
/// 品番（添字 0 の列）の適合する値は [`UNIQUE_GROUPS`] 個を巡回し、違反として仕込んだ行
/// だけがその巡回から外れる。重複の違反は**値ごとに 1 件**であり（重複するすべての行を
/// `rows` に載せる。要件 4.7）、2 件以上残った値の個数がそのまま違反の件数になる。
/// 仕込む位置は [`flat_index`] の選定で実行前に決まるため、この数も実行前に決まる —
/// 手で書いた定数ではなく、標本自身が**同じ計画から**導く（テストが検証の結果と突き合わせる）。
fn duplicate_groups(rows: usize, plan: &ViolationPlan) -> usize {
    let mut members = vec![0usize; UNIQUE_GROUPS];
    for row in 0..rows {
        if plan.is_violation(flat_index(row, UNIQUE_COLUMN, rows)) {
            continue;
        }
        members[row % UNIQUE_GROUPS] += 1;
    }
    members.iter().filter(|count| **count >= 2).count()
}

/// 骨格のエントリの 1 つを差し替える（行データ）。
fn replace_entry(entries: &mut [(EntryName, Vec<u8>)], name: EntryName, bytes: Vec<u8>) {
    let slot = entries
        .iter()
        .position(|(known, _)| *known == name)
        .expect("骨格は差し替えるエントリを持つ");
    entries[slot].1 = bytes;
}

/// 標本の列が値を組み立てるのに要る文脈。
struct SampleContext {
    /// 参照先シート。
    reference: SheetId,
    /// 参照先シートの行識別子（`ref` 列の適合する値）。
    reference_rows: Vec<RowId>,
    /// 添付の識別子（添付の列の適合する値）。
    attachment: AttachmentId,
    /// 名前つき型定義（`届け先`）の識別子。
    definition: TypeDefId,
    /// 一意制約の列に重複を混ぜるか（品番の値を [`UNIQUE_GROUPS`] 個で巡回させる）。
    unique_collisions: bool,
    /// 一括判定を上書きした実装の列で判定の失敗を起こすか
    /// （[`SchemaSampleOptions::with_batch_failure`]）。
    batch_failure: bool,
}

/// 標本の 1 列（tasks.md 9.1）。
///
/// 宣言と、その列に入れる 2 つの値の規則を組にする。**適合する値は違反を 1 件も生まず、
/// 違反する値はちょうど 1 件を生む**（モジュール docs）。規則は値を持たない関数であり、
/// 列ごとに確保しない（標本は 3,000,000 セルを組み立てるため、規則の側で確保を増やさない）。
struct SampleColumn {
    /// 列の宣言（名前・型・必須・一意・既定値）。
    decl: ColumnDecl,
    /// 適合する値。
    conforming: fn(usize, &SampleContext) -> CellValue,
    /// 違反する値（その列で 1 つだけ制約を外した値）。
    violating: fn(usize, &SampleContext) -> CellValue,
}

impl SampleColumn {
    /// 列の宣言と 2 つの値の規則を組にする。
    fn new(
        decl: ColumnDecl,
        conforming: fn(usize, &SampleContext) -> CellValue,
        violating: fn(usize, &SampleContext) -> CellValue,
    ) -> Self {
        Self {
            decl,
            conforming,
            violating,
        }
    }
}

/// 標本の 30 列（宣言順そのものが列の並び順になる）。
fn sample_columns(context: &SampleContext) -> Vec<SampleColumn> {
    vec![
        // 品番: 必須かつ一意のキー。唯一の `unique` 列である（9.3 が重複を混ぜる列）。
        SampleColumn::new(
            column(
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
            |row, context| {
                if context.unique_collisions {
                    // 重複を混ぜる（9.3 が値の違反と一意制約の違反の両方を見るため）。
                    CellValue::Text(format!("P{:07}", row % UNIQUE_GROUPS))
                } else {
                    CellValue::Text(format!("P{row:07}"))
                }
            },
            |row, _| CellValue::Text(format!("!{row:07}")),
        ),
        // 数量: 範囲つきの整数（既定値あり）。
        SampleColumn::new(
            column(
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
            |row, _| CellValue::Int((row % 1_000 + 1) as i64),
            |_, _| CellValue::Int(-1),
        ),
        // 単位: 選択肢（既定値あり）。
        SampleColumn::new(
            column(
                "単位",
                typed(TypeKind::Enum, choices(&["個", "箱", "kg", "m", "式"])),
                true,
                false,
                Some(CellValue::Text("個".to_owned())),
            ),
            |row, _| CellValue::Text(["個", "箱", "kg", "m", "式"][row % 5].to_owned()),
            |_, _| CellValue::Text("不明".to_owned()),
        ),
        // 単価: 桁と範囲つきの 10 進数。
        SampleColumn::new(
            column(
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
                true,
                false,
                None,
            ),
            |row, _| decimal(row % 10_000, row % 100),
            // 桁だけを外す（文法にも範囲にも合うが、宣言された scale を超える）。
            |_, _| CellValue::Decimal("1.23456".to_owned()),
        ),
        // 金額: 桁だけを宣言した 10 進数（範囲は宣言しない）。
        SampleColumn::new(
            column(
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
            |row, _| decimal(row % 1_000_000, row % 100),
            |_, _| CellValue::Decimal("1.23456".to_owned()),
        ),
        // 通貨: 選択肢（既定値あり）。
        SampleColumn::new(
            column(
                "通貨",
                typed(TypeKind::Enum, choices(&["JPY", "USD", "EUR"])),
                false,
                false,
                Some(CellValue::Text("JPY".to_owned())),
            ),
            |row, _| CellValue::Text(["JPY", "USD", "EUR"][row % 3].to_owned()),
            |_, _| CellValue::Text("XXX".to_owned()),
        ),
        // 発注日: 範囲つきの日付。
        SampleColumn::new(
            column(
                "発注日",
                typed(
                    TypeKind::Date,
                    Constraints {
                        min: Some(CellValue::Text("2026-01-01".to_owned())),
                        max: Some(CellValue::Text("2026-12-31".to_owned())),
                        ..Constraints::default()
                    },
                ),
                true,
                false,
                None,
            ),
            |row, _| date_at(row),
            |_, _| CellValue::Text("2026/09/12".to_owned()),
        ),
        // 納品予定日: 範囲を宣言しない日付。
        SampleColumn::new(
            column(
                "納品予定日",
                typed(TypeKind::Date, Constraints::default()),
                false,
                false,
                None,
            ),
            |row, _| date_at(row + 1),
            // 存在しない暦日である（正準表記に一致しない）。
            |_, _| CellValue::Text("2026-02-30".to_owned()),
        ),
        // 確定日時: オフセットを要求する日時。
        SampleColumn::new(
            column(
                "確定日時",
                typed(
                    TypeKind::DateTime,
                    Constraints {
                        offset: Some(OffsetPolicy::Required),
                        ..Constraints::default()
                    },
                ),
                true,
                false,
                None,
            ),
            |row, _| CellValue::Text(format!("2026-09-12T{:02}:{:02}:00Z", row % 24, row % 60)),
            |_, _| CellValue::Text("2026-09-12 10:30:00".to_owned()),
        ),
        // 登録日時: オフセットを禁ずる日時（civil）。
        SampleColumn::new(
            column(
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
            |row, _| CellValue::Text(format!("2026-09-12T{:02}:{:02}:00", row % 24, row % 60)),
            |_, _| CellValue::Text("2026-09-12T10:30:00+09:00".to_owned()),
        ),
        // 仕入先: 参照先シートを指す参照（必須）。
        SampleColumn::new(
            column(
                "仕入先",
                typed(
                    TypeKind::Ref,
                    Constraints {
                        sheet: Some(context.reference),
                        ..Constraints::default()
                    },
                ),
                true,
                false,
                None,
            ),
            |row, context| reference_at(context, row),
            // 実在しない行識別子（書式としては正しい ULID）。
            |_, _| CellValue::Text("99999999999999999999999999".to_owned()),
        ),
        // 担当者: 同じ参照先を指す 2 本目の参照。
        SampleColumn::new(
            column(
                "担当者",
                typed(
                    TypeKind::Ref,
                    Constraints {
                        sheet: Some(context.reference),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            |row, context| reference_at(context, row),
            |_, _| CellValue::Text("99999999999999999999999999".to_owned()),
        ),
        // カテゴリ: 選択肢。
        SampleColumn::new(
            column(
                "カテゴリ",
                typed(TypeKind::Enum, choices(&["資材", "部品", "設備", "消耗品"])),
                false,
                false,
                None,
            ),
            |row, _| CellValue::Text(["資材", "部品", "設備", "消耗品"][row % 4].to_owned()),
            |_, _| CellValue::Text("該当なし".to_owned()),
        ),
        // 状態: 選択肢。
        SampleColumn::new(
            column(
                "状態",
                typed(
                    TypeKind::Enum,
                    choices(&["未着手", "進行中", "完了", "取消"]),
                ),
                false,
                false,
                None,
            ),
            |row, _| CellValue::Text(["未着手", "進行中", "完了", "取消"][row % 4].to_owned()),
            |_, _| CellValue::Text("該当なし".to_owned()),
        ),
        // 備考: 最大長だけを宣言した文字列。
        SampleColumn::new(
            column(
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
            |row, _| CellValue::Text(format!("備考{}", row % 1_000)),
            |_, _| CellValue::Text("x".repeat(201)),
        ),
        // 社内コード: 長さと書式の両方を持つ文字列（違反は書式だけを外す）。
        SampleColumn::new(
            column(
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
            |row, _| CellValue::Text(format!("{:04}-{:02}", row % 10_000, row % 100)),
            // 長さは合うが書式だけを外す（`ABCDEFG` は 7 文字）。
            |_, _| CellValue::Text("ABCDEFG".to_owned()),
        ),
        // ロット番号: 書式だけを持つ文字列。
        SampleColumn::new(
            column(
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
            |row, _| CellValue::Text(format!("LOT-{:06}", row % 1_000_000)),
            |_, _| CellValue::Text("LOT-12345".to_owned()),
        ),
        // 検査日時: オフセットを要求し、範囲も持つ日時。
        SampleColumn::new(
            column(
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
            |row, _| {
                CellValue::Text(format!(
                    "2026-{:02}-{:02}T{:02}:00:00+09:00",
                    row % 12 + 1,
                    row % 28 + 1,
                    row % 24
                ))
            },
            // オフセットの表記が正準でない（`+0900`）。
            |_, _| CellValue::Text("2026-09-12T10:30:00+0900".to_owned()),
        ),
        // 検査済み: 真偽。
        SampleColumn::new(
            column(
                "検査済み",
                typed(TypeKind::Bool, Constraints::default()),
                false,
                false,
                None,
            ),
            |row, _| CellValue::Bool(row % 2 == 0),
            |_, _| CellValue::Text("yes".to_owned()),
        ),
        // 予備数量: 範囲つきの整数（下限は 0）。
        SampleColumn::new(
            column(
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
            |row, _| CellValue::Int((row % 1_000) as i64),
            |_, _| CellValue::Int(-1),
        ),
        // 予備単価: scale 4 の 10 進数。
        SampleColumn::new(
            column(
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
            |row, _| CellValue::Decimal(format!("{}.{:04}", row % 100, row % 10_000)),
            |_, _| CellValue::Decimal("1.23456".to_owned()),
        ),
        // 予備率: 範囲つきの小数。
        SampleColumn::new(
            column(
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
            |row, _| CellValue::float((row % 200) as f64 / 2.0),
            |_, _| CellValue::Float(-1.0),
        ),
        // 添付: 添付参照（実体は `attachments/<hex>.bin`）。
        SampleColumn::new(
            column(
                "添付",
                typed(TypeKind::Attachment, Constraints::default()),
                false,
                false,
                None,
            ),
            |_, context| CellValue::Attachment(context.attachment),
            |_, _| CellValue::Text("!".to_owned()),
        ),
        // 機械可読値: ANY（必須なので値なしが違反になる）。
        SampleColumn::new(
            column(
                "機械可読値",
                typed(TypeKind::Any, Constraints::default()),
                true,
                false,
                None,
            ),
            |row, _| CellValue::Text(format!("k={}", row % 100)),
            |_, _| CellValue::Null,
        ),
        // 届け先: 名前つき型定義への参照（入れ子のオブジェクト）。
        SampleColumn::new(
            column(
                "届け先",
                TypeDecl::Ref(context.definition),
                true,
                false,
                None,
            ),
            |row, context| destination(row, context),
            |_, _| CellValue::Text("!".to_owned()),
        ),
        // 明細: 入れ子の配列（要素はインラインのオブジェクト）。
        SampleColumn::new(
            column(
                "明細",
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
                ),
                false,
                false,
                None,
            ),
            |row, _| {
                CellValue::Nested(NestedValue::Array(vec![
                    line_item(row, 0),
                    line_item(row, 1),
                ]))
            },
            |_, _| CellValue::Text("!".to_owned()),
        ),
        // 改訂履歴: 入れ子の配列（要素はインラインのオブジェクト。日付を持つ）。
        SampleColumn::new(
            column(
                "改訂履歴",
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
                ),
                false,
                false,
                None,
            ),
            |row, _| {
                CellValue::Nested(NestedValue::Array(vec![revision(row, 1), revision(row, 2)]))
            },
            |_, _| CellValue::Text("!".to_owned()),
        ),
        // 届け先郵便番号: 拡張型（一括判定を上書きした実装）。
        SampleColumn::new(
            column(
                "届け先郵便番号",
                typed(
                    TypeKind::Custom,
                    Constraints {
                        custom_type: Some(BATCH_CUSTOM_ID.into()),
                        ..Constraints::default()
                    },
                ),
                true,
                false,
                None,
            ),
            |row, _| postal_code(row),
            // 既定は実装の拒否を、`batch_failure` では一括判定の失敗を仕込む（9.4）。
            |_, context| {
                if context.batch_failure {
                    undecidable_postal_code()
                } else {
                    malformed_postal_code()
                }
            },
        ),
        // 請求先郵便番号: 拡張型（既定実装のままの実装。9.4 が両者を突き合わせる）。
        SampleColumn::new(
            column(
                "請求先郵便番号",
                typed(
                    TypeKind::Custom,
                    Constraints {
                        custom_type: Some(SIMPLE_CUSTOM_ID.into()),
                        ..Constraints::default()
                    },
                ),
                false,
                false,
                None,
            ),
            |row, _| postal_code(row + 1),
            // `batch_failure` では実装の拒否を、既定では既定実装の失敗を仕込む（9.4）。
            |_, context| {
                if context.batch_failure {
                    malformed_postal_code()
                } else {
                    undecidable_postal_code()
                }
            },
        ),
        // 補助コード: 書式つきの文字列（入れ子の商品コードと別の書式）。
        SampleColumn::new(
            column(
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
            |row, _| CellValue::Text(format!("AB{:04}", row % 10_000)),
            |_, _| CellValue::Text("!!".to_owned()),
        ),
    ]
}

/// `届け先` の名前つき型定義（入れ子のオブジェクト。拡張型と参照を内側に持つ）。
fn destination_definition(context: &SampleContext) -> TypeDecl {
    typed(
        TypeKind::Object,
        Constraints {
            fields: vec![
                // 入れ子のフィールドにも拡張型を指定できる（要件 11.2）。こちらは
                // 既定実装のままの実装を使う。
                field(
                    DESTINATION_FIELDS[0],
                    typed(
                        TypeKind::Custom,
                        Constraints {
                            custom_type: Some(SIMPLE_CUSTOM_ID.into()),
                            ..Constraints::default()
                        },
                    ),
                    true,
                    None,
                ),
                field(
                    DESTINATION_FIELDS[1],
                    typed(
                        TypeKind::Enum,
                        choices(&["東京都", "大阪府", "愛知県", "福岡県"]),
                    ),
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
                // 入れ子の内側の参照（5.3 が降りて判定する）。
                field(
                    DESTINATION_FIELDS[4],
                    typed(
                        TypeKind::Ref,
                        Constraints {
                            sheet: Some(context.reference),
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

/// 上流が不透明なペイロードとして保持するエンベロープ（`{ "root": …, "types": [ … ] }`）。
///
/// ルートスキーマと型定義の正準出力は `schema-engine` の `declaration::codec` が行う
/// （本モジュールは宣言テキストの文法を再実装しない）。
fn envelope(context: &SampleContext, columns: &[SampleColumn]) -> String {
    let schema = Schema {
        columns: columns.iter().map(|column| column.decl.clone()).collect(),
    };
    let root = schema_to_text(&schema).expect("標本の宣言は正準出力できる");
    let definition = type_definition_to_text(&destination_definition(context))
        .expect("標本の型定義は正準出力できる");
    format!(
        r#"{{"root":{root},"types":[{{"id":"{}","definition":{definition}}}]}}"#,
        context.definition
    )
}

/// 種別による型の宣言。
fn typed(kind: TypeKind, constraints: Constraints) -> TypeDecl {
    TypeDecl::Kind {
        kind: DeclaredKind::Known(kind),
        constraints,
    }
}

/// 型の宣言が `custom` 拡張型を指すなら、その識別子を返す。
///
/// [`BATCH_CUSTOM_COLUMN`] / [`SIMPLE_CUSTOM_COLUMN`] の添字が宣言と食い違っていないことを
/// [`schema_sample`] が表明するために使う（添字を手で保つ代わりに、食い違えば落ちる）。
fn custom_type_of(ty: &TypeDecl) -> Option<&str> {
    match ty {
        TypeDecl::Kind { constraints, .. } => constraints.custom_type.as_deref(),
        TypeDecl::Ref(_) => None,
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

/// `{整数}.{小数 2 桁}` の 10 進数の値。
fn decimal(integer: usize, fraction: usize) -> CellValue {
    CellValue::Decimal(format!("{integer}.{fraction:02}"))
}

/// 2026 年の実在する日付（日は 28 日以下に限るため、どの月でも正しい暦日になる）。
fn date_at(row: usize) -> CellValue {
    CellValue::Text(format!("2026-{:02}-{:02}", row % 12 + 1, row % 28 + 1))
}

/// 参照先シートの行識別子（行ごとに決定的に巡回する）。
fn reference_at(context: &SampleContext, row: usize) -> CellValue {
    let rows = &context.reference_rows;
    CellValue::Text(rows[row % rows.len()].to_string())
}

/// 郵便番号の形の値（`123-4567`）。
fn postal_code(row: usize) -> CellValue {
    CellValue::Text(format!("{:03}-{:04}", row % 1_000, row % 10_000))
}

/// 拡張型の実装が**判定できない**値（`?` で始まる。実装は `Err` を返す）。
///
/// 外部の規則エンジンへ渡せない値の代役であり、判定の失敗（`CustomFailed`）を 1 セルに
/// つき 1 件だけ生む（要件 11.5 の隔離がこれを見る）。
fn undecidable_postal_code() -> CellValue {
    CellValue::Text("?判定不能".to_owned())
}

/// 拡張型の実装が**拒否する**値（形が違う。実装は違反の判定を返す）。
fn malformed_postal_code() -> CellValue {
    CellValue::Text("1000001".to_owned())
}

/// `届け先`（名前つき型定義）に適合する入れ子のオブジェクト。
fn destination(row: usize, context: &SampleContext) -> CellValue {
    CellValue::Nested(NestedValue::Object(vec![
        (
            DESTINATION_FIELDS[0].to_owned(),
            // 入れ子のフィールドは拡張型（既定実装）。形は最上位の列と同じである。
            postal_code(row + 2),
        ),
        (
            DESTINATION_FIELDS[1].to_owned(),
            CellValue::Text(["東京都", "大阪府", "愛知県", "福岡県"][row % 4].to_owned()),
        ),
        (
            DESTINATION_FIELDS[2].to_owned(),
            CellValue::Text(format!(
                "架空市{}丁目{}番{}号",
                row % 100,
                row % 50,
                row % 20
            )),
        ),
        (
            DESTINATION_FIELDS[3].to_owned(),
            CellValue::Text(format!("{}号館", row % 9 + 1)),
        ),
        (
            DESTINATION_FIELDS[4].to_owned(),
            reference_at(context, row + 3),
        ),
    ]))
}

/// `明細` の要素（インラインのオブジェクト）に適合する値。
fn line_item(row: usize, index: usize) -> CellValue {
    let seed = row * 2 + index;
    CellValue::Nested(NestedValue::Object(vec![
        (
            LINE_FIELDS[0].to_owned(),
            CellValue::Text(format!("ITEM-{:04}", seed % 10_000)),
        ),
        (
            LINE_FIELDS[1].to_owned(),
            CellValue::Int((seed % 100 + 1) as i64),
        ),
        (
            LINE_FIELDS[2].to_owned(),
            decimal(seed % 10_000, seed % 100),
        ),
    ]))
}

/// `改訂履歴` の要素（インラインのオブジェクト）に適合する値。
fn revision(row: usize, version: usize) -> CellValue {
    CellValue::Nested(NestedValue::Object(vec![
        (REVISION_FIELDS[0].to_owned(), date_at(row + version)),
        (
            REVISION_FIELDS[1].to_owned(),
            CellValue::Int(version as i64),
        ),
        (
            REVISION_FIELDS[2].to_owned(),
            CellValue::Text(format!("改訂{}", row % 100)),
        ),
    ]))
}

/// 標本用の拡張型の規則（tasks.md 9.1。両方の実装が同じ規則を共有する）。
///
/// - `123-4567` の形（3 桁・ハイフン・4 桁）だけを適合とする。
/// - `?` で始まる値は判定できないものとして失敗を返す（外部の規則エンジンへ渡せない値の
///   代役。要件 11.5 の隔離がこれを見る）。
/// - その他の値は拒否する。
fn judge_postal_code(value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
    match value {
        CellValue::Text(text) if is_postal_code(text) => Ok(CustomVerdict::Accepted),
        CellValue::Text(text) if text.starts_with('?') => {
            Err(CustomTypeFailure::new("判定できない文字を含む"))
        }
        _ => Ok(CustomVerdict::rejected("郵便番号の形でない")),
    }
}

/// `123-4567` の形か（`\d{3}-\d{4}`）。
fn is_postal_code(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 8
        && bytes[..3].iter().all(u8::is_ascii_digit)
        && bytes[3] == b'-'
        && bytes[4..].iter().all(u8::is_ascii_digit)
}

/// **既定実装のまま**の拡張型（`validate_batch` を上書きしない）。
///
/// 判定は 1 件用の [`CustomType::validate`] だけに書き、一括判定と正準化はトレイトの
/// 既定実装に委ねる（design.md「Registry Layer / TypeRegistry」）。
struct PostalCode {
    id: CustomTypeId,
}

impl PostalCode {
    /// 識別子を与えて作る。
    fn new(id: &str) -> Self {
        Self {
            id: CustomTypeId::new(id),
        }
    }
}

impl CustomType for PostalCode {
    fn id(&self) -> &CustomTypeId {
        &self.id
    }

    fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
        judge_postal_code(value)
    }
}

/// **一括判定を上書きした**拡張型（境界を越える実装の代役。design.md の Risks）。
///
/// 規則は [`PostalCode`] と同じであり、違うのは境界を越える回数を列ごとに 1 回へ
/// まとめる点だけである（要件 11.6）。`Err` を返すときは**失敗より前の判定を先に
/// `out` へ渡してから**返す（4.1 の裁定。既定実装と同じ結果になるために要る）。
struct PostalCodeBatch {
    id: CustomTypeId,
}

impl PostalCodeBatch {
    /// 識別子を与えて作る。
    fn new(id: &str) -> Self {
        Self {
            id: CustomTypeId::new(id),
        }
    }
}

impl CustomType for PostalCodeBatch {
    fn id(&self) -> &CustomTypeId {
        &self.id
    }

    fn validate(&self, value: &CellValue) -> Result<CustomVerdict, CustomTypeFailure> {
        judge_postal_code(value)
    }

    fn validate_batch(
        &self,
        values: &[CellValue],
        out: &mut dyn FnMut(usize, CustomVerdict),
    ) -> Result<(), CustomTypeFailure> {
        let mut verdicts = Vec::with_capacity(values.len());
        for (index, value) in values.iter().enumerate() {
            match judge_postal_code(value) {
                Ok(verdict) => verdicts.push((index, verdict)),
                Err(failure) => {
                    for (index, verdict) in verdicts.drain(..) {
                        out(index, verdict);
                    }
                    return Err(failure);
                }
            }
        }
        for (index, verdict) in verdicts {
            out(index, verdict);
        }
        Ok(())
    }
}

/// 標本の拡張型を登録した台帳（tasks.md 9.1。要件 11.1, 11.2）。
///
/// 同じ規則の 2 つの実装（[`PostalCodeBatch`] と [`PostalCode`]）を別々の識別子で登録する。
/// 9.4 はこの 2 つが同じ結果を返すことと、一方が `Err` を返してもシート全体の検証が
/// 完走することを確かめる。
pub fn sample_registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    registry
        .register(Arc::new(PostalCodeBatch::new(BATCH_CUSTOM_ID)))
        .expect("標本の拡張型の識別子は重複しない");
    registry
        .register(Arc::new(PostalCode::new(SIMPLE_CUSTOM_ID)))
        .expect("標本の拡張型の識別子は重複しない");
    registry
}

//! 一括処理の予算のベンチ（tasks.md 5.3。要件 11.1, 11.2, 11.3）。
//!
//! # 計測する対象（design.md「Performance / Load」の 1・2。いずれも予算の判定に載せる）
//!
//! | bench id | 計測 | 予算 |
//! |---|---|---|
//! | `bulk/read_100k_rows_x_30_columns` | 10 万行 × 30 列の**全行読み + 集計**を 1 本のマクロとして実行 | **10 秒**（要件 11.1） |
//! | `bulk/rewrite_10k_rows` | **1 万行 × 30 列**の書き換えを 1 本のマクロとして実行 | **5 秒**（要件 11.2） |
//!
//! どちらも**実行 1 回の全体**（isolate の生成・変換・実行・値の往復・変更の集約）を測る。
//! 実行ごとに isolate を作るのは製品の形であり（design.md 決定 1）、**JIT の温まりを
//! 持ち越さない代償**（design.md「Performance & Scalability」）を予算が吸収することの確認が
//! 本計測の目的である。測定条件は要件値そのもの（10 万行 × 30 列 / 1 万行）である。
//!
//! # 標本は `data-grid` の生成器を取り込む（tasks.md 5.3。写しを作らない）
//!
//! 10 万行 × 30 列の標本（文書・列の宣言・値・仕込んだ違反）の唯一の源は
//! `crates/data-grid/tests/common/sample.rs` であり、本ベンチはその入口
//! （[`common::sample::sample`]）だけを使う。`benches/` から `tests/` のモジュールをそのまま
//! 取り込めないため、**相対パスで取り込む**（`crates/data-grid/benches/large_grid.rs` と
//! 同じ形。本ベンチは**クレートを越えて**取り込むが、標本の生成器は `document-format` と
//! `schema-engine` だけに依存するため、`macro-runtime` の依存の範囲でそのままコンパイル
//! できる。**写しを作らない**という要求はクレートの境界では緩まない）。
//!
//! 標本の規模は**要件値のリテラル**（10 万行・30 列・1 万行）で組み立て、組み上がった標本の
//! 形を**マクロの戻り値**で表明する — 標本の行数を黙って縮める変更は、計測の外で 1 度
//! 実行する検査（下記）で失敗する。
//!
//! # ホスト（縫い目）は本ベンチが持つ
//!
//! エンジンは文書を知らない（design.md 決定 2 / 3）ため、読み書きは縫い目（[`HostPort`]）
//! 越しである。製品の実装はアダプタ（`src-tauri/src/macro_host.rs`）が持ち、**本クレートは
//! それに依存できない**（Tauri に依存するクレートをコアへ引き込まない。`Cargo.toml` の
//! 依存方針）。したがって本ベンチが**標本の文書 1 つに対する最小の実装**を持つ。
//!
//! 実装は**製品より軽くしない**ことを規律とする（軽くすると予算の判定が甘くなる）:
//!
//! * 列の宣言（`columns`）は**呼び出しごとに**宣言テキストから組み立てる（製品の
//!   `DocumentHost::columns` と同じ。キャッシュしない）
//! * 行の実在の検査（`stage`）は**呼び出しごとに**文書の行を集合へ写して引く（製品の
//!   `refuse_missing` と同じ）
//! * 読みは文書の行を写し、**重ね合わせ**（[`Overlay`]）を通す（製品と同じ経路）
//!
//! 能力を要する口（`file_read` / `file_write` / `net_fetch`）は**拒む**。本ベンチのマクロは
//! どれも呼ばない（呼べば門が先に拒むが、縫い目側も fail-closed にしておく）。
//!
//! # 実行時間（既定設定では長すぎる）
//!
//! criterion の既定（100 サンプル・3 秒目標）は実行 1 回が秒単位になる本計測では長すぎる
//! ため、`sample_size` / `warm_up_time` / `measurement_time` を明示的に絞る（同じ形は
//! `crates/data-grid/benches/large_grid.rs` にある）。

#[path = "../../data-grid/tests/common/mod.rs"]
mod common;

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use document_format::{Document, RowId, SheetId};

use common::sample::{sample, SampleOptions};
use macro_runtime::host::changes::{Change, ChangeSet};
use macro_runtime::host::overlay::{ColumnTypeInfo, Overlay, ReadRow, RowPage, RowSpan, SheetInfo};
use macro_runtime::host::{HostError, HostPort};
use macro_runtime::{
    Limits, MacroKind, MacroName, MacroRecord, MacroRuntime, MacroRuntimeApi, RunOutcome,
    RunRequest, WindowLabel,
};
use schema_engine::compile::resolve::{resolve, Resolver};
use schema_engine::{
    parse_schema, parse_type_definition, DeclaredKind, SchemaError, TypeDecl, TypeDefinition,
    TypeKind,
};

/// 標本の行数（要件 11.1 が定める計測条件そのもの。10 万行 × 30 列）。
const ROWS: usize = 100_000;

/// 標本の列数（要件 11.1 の計測条件の列数の側）。
const COLUMNS: usize = 30;

/// 書き換える行数（要件 11.2 の規模そのもの。1 万行）。
///
/// **列は 30 列すべてを書く**（1 万行の「書き換え」を、画面から 1 万行を貼り付ける予算と
/// 同じ桁に収めるという要件 11.2 の趣旨に合わせる。1 行につき 1 セルだけ書くと、同じ
/// 「1 万行」でも書き込みの量が 30 分の 1 になり、比較の意味を失う）。
const REWRITE_ROWS: usize = 10_000;

/// criterion のサンプル数（既定 100 は実行 1 回が秒単位の本計測では長すぎる）。
const SAMPLE_SIZE: usize = 10;

/// 目標の計測時間（実行 1 回が秒単位であるため、10 サンプルでこの長さを目安にする）。
const MEASUREMENT: Duration = Duration::from_secs(30);

/// 全行読み + 集計のマクロ（要件 11.1）。
///
/// 10 万行を**1 回の呼び出し**で読み（要件 4.4 / 11.3）、**すべてのセルを集計に使う**
/// （要件 11.1 の「その全部を使う集計」）。戻り値は `行数/列数/合計` の文字列であり、
/// 標本の形の表明を兼ねる（計測の外で 1 度だけ実行して検査する）。
const READ_SOURCE: &str = r#"
const sheet = (await host.sheets())[0];
const page = await host.readRange(sheet.id, { from: 0, to: sheet.row_count - 1 });
let total = 0;
for (const row of page.rows) {
  for (let column = 0; column < row.cells.length; column++) {
    const value = row.cells[column];
    if (typeof value === "number") total += value;
    else if (typeof value === "string") total += value.length;
    else if (typeof value === "boolean") total += value ? 1 : 0;
  }
}
export default `${page.rows.length}/${page.rows[0].cells.length}/${total}`;
"#;

/// 1 万行 × 30 列の書き換えのマクロ（要件 11.2）。
///
/// 先頭の 1 万行を 1 回の呼び出しで読み（書き込みは行の識別子で指すため。要件 4.4 /
/// 11.3）、**読んだ行の全列を書き戻す**。数値は 1 を足し、それ以外はそのまま書く
/// （書き換えの内容そのものは予算に効かないが、値の写像と変更の集約を実際に通すために
/// 読んだ値から組み立てる）。戻り値は `行数/書き込み数` であり、書き込みが 30 列ぶん
/// 届いたことの表明を兼ねる。
const REWRITE_SOURCE: &str = r#"
const sheet = (await host.sheets())[0];
const page = await host.readRange(sheet.id, { from: 0, to: 9999 });
const writes = [];
for (const row of page.rows) {
  for (let column = 0; column < row.cells.length; column++) {
    const value = row.cells[column];
    writes.push({
      row: row.id,
      column,
      value: typeof value === "number" ? value + 1 : value,
    });
  }
}
await host.setCells(sheet.id, writes);
export default `${page.rows.length}/${writes.length}`;
"#;

/// 10 万行 × 30 列の標本を組み立て、形を要件値と突き合わせる。
fn large_sample() -> common::sample::Sample {
    let sample = sample(&SampleOptions::new(ROWS, COLUMNS));
    assert_eq!(
        sample.rows(),
        ROWS,
        "標本の行数が要件 11.1 の計測条件と一致しない"
    );
    assert_eq!(
        sample.column_count(),
        COLUMNS,
        "標本の列数が要件 11.1 の計測条件と一致しない"
    );
    sample
}

/// マクロを 1 回実行し、結果を返す（計測の内側と外側が共有する唯一の入口）。
fn run(runtime: &MacroRuntime, host: Arc<dyn HostPort>, name: &str, source: &str) -> RunOutcome {
    let record = MacroRecord::new(MacroName::new(name), MacroKind::TypeScript, source);
    let request = RunRequest::new(record, Limits::default(), WindowLabel::from("bench"));
    runtime
        .run(request, host)
        .expect("実行の要求は受け取られる（同じ実行基盤で直列に呼ぶ）")
}

/// 成功した実行の戻り値と変更の件数を取り出す（失敗と打ち切りは理由つきで panic する）。
fn ran<'a>(outcome: &'a RunOutcome, what: &str) -> (&'a str, usize) {
    match outcome {
        RunOutcome::Ran {
            value, changes, ..
        } => (value.as_str(), changes.total()),
        other => panic!("{what}: 成功を期待したが {other:?} を返した"),
    }
}

/// 戻り値の提示から引用符を外す（要件 2.3 の提示は JSON であり、文字列は引用符つきで返る）。
///
/// [`RunOutcome::Ran`] の `value` は `JSON.stringify` の結果である（`engine/isolate.rs` の
/// `presentation`）。本ベンチのマクロは `行数/列数/…` の文字列を返すため、形の表明の前に
/// 引用符を外す。
fn text(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

/// 標本の文書 1 つに読み書きを供給する縫い目（本ベンチ専用。モジュール docs を参照）。
struct SampleHost {
    /// 読みの元であり、行・列の実在を引く先でもある文書。
    ///
    /// **`Arc` で持つ**（縫い目は `Arc<dyn HostPort>` として実行へ渡り、`'static` を要する。
    /// 借用にすると、標本の文書がベンチ関数の寿命を越えられない）。
    document: Arc<Document>,
    /// この実行で集めた変更（**適用しない**。design.md 決定 2 / 3）。
    changes: Mutex<ChangeSet>,
}

impl SampleHost {
    fn new(document: Arc<Document>) -> Self {
        Self {
            document,
            changes: Mutex::new(ChangeSet::new()),
        }
    }

    /// この実行で集めた変更を借用のまま読ませる。
    fn with_locked_changes(&self, read: &mut dyn FnMut(&ChangeSet)) {
        // 毒された場合も読む（変更集合は他のスレッドの panic で壊れるデータを持たない）。
        read(&self.changes.lock().unwrap_or_else(PoisonError::into_inner));
    }

    /// 文書に無いシート・行・範囲外の列を理由つきで拒む（要件 5.4）。
    ///
    /// **製品（`DocumentHost::refuse_missing`）と同じ形**である — 行の実在は集合で引く
    /// （1 件ずつ文書を走査して件数の積にしない）。本ベンチのマクロはセルの書き換えしか
    /// 呼ばないため、他の腕は fail-closed に拒む。
    fn refuse_missing(&self, change: &Change) -> Result<(), HostError> {
        let Change::SetCells { sheet, writes } = change else {
            return Err(HostError::new(
                "このベンチのマクロはセルの書き換え以外の変更を呼ばない",
            ));
        };
        if writes.is_empty() {
            return Ok(());
        }
        let Some(found) = self.document.sheet_by_id(*sheet) else {
            return Err(HostError::new(format!("シート {sheet} はこの文書に無い")));
        };
        let column_count = found.columns().len();
        let present: HashSet<RowId> = found.rows().iter().map(|row| row.id()).collect();
        if let Some(write) = writes.iter().find(|write| !present.contains(&write.row)) {
            return Err(HostError::new(format!(
                "行 {} はシート {sheet} に無い",
                write.row
            )));
        }
        if let Some(write) = writes
            .iter()
            .find(|write| write.column.index() >= column_count)
        {
            return Err(HostError::new(format!(
                "列 {} はシート {sheet} の範囲外である（列数 {column_count}）",
                write.column.index()
            )));
        }
        Ok(())
    }
}

impl HostPort for SampleHost {
    fn sheets(&self) -> Result<Vec<SheetInfo>, HostError> {
        Ok(self
            .document
            .sheets()
            .iter()
            .map(|sheet| SheetInfo {
                id: sheet.id(),
                name: sheet.name().to_owned(),
                row_count: sheet.rows().len(),
            })
            .collect())
    }

    fn columns(&self, sheet: SheetId) -> Result<Vec<ColumnTypeInfo>, HostError> {
        // **呼び出しごとに組み立てる**（製品と同じ。モジュール docs の規律）。
        column_types(&self.document, sheet)
    }

    fn read_rows(&self, sheet: SheetId, span: RowSpan) -> Result<RowPage, HostError> {
        let Some(found) = self.document.sheet_by_id(sheet) else {
            return Err(HostError::new(format!("シート {sheet} はこの文書に無い")));
        };
        let rows = found.rows();
        let Some(range) = span.resolve(rows.len()) else {
            return Ok(RowPage { rows: Vec::new() });
        };
        let base = rows[range]
            .iter()
            .map(|row| ReadRow {
                id: row.id(),
                cells: row.values().to_vec(),
            })
            .collect();
        // 重ね合わせは文書の読みの後で作る（製品と同じ経路。要件 5.1 の裏面）。
        let changes = self.changes.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(Overlay::new(&changes).read_range(sheet, base))
    }

    fn stage(&self, change: Change) -> Result<(), HostError> {
        self.refuse_missing(&change)?;
        self.changes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stage(change)
            .map_err(|error| HostError::new(error.to_string()))
    }

    fn with_changes(&self, read: &mut dyn FnMut(&ChangeSet)) {
        self.with_locked_changes(read);
    }

    fn file_read(&self, _path: &str) -> Result<String, HostError> {
        Err(HostError::new(
            "このベンチのマクロは能力を要する口を呼ばない（file_read）",
        ))
    }

    fn file_write(&self, _path: &str, _text: &str) -> Result<(), HostError> {
        Err(HostError::new(
            "このベンチのマクロは能力を要する口を呼ばない（file_write）",
        ))
    }

    fn net_fetch(&self, _url: &str) -> Result<String, HostError> {
        Err(HostError::new(
            "このベンチのマクロは能力を要する口を呼ばない（net_fetch）",
        ))
    }
}

/// シートの列の宣言を種別つきで組む（製品の `DocumentHost::columns` と同じ手順）。
///
/// 宣言テキストの解釈は `schema-engine` に任せる（本ベンチは文法を持たない）。`$ref` の
/// 連鎖は [`Resolver`] が辿り、解決できない型は [`TypeKind::Any`] として渡す（製品と同じ。
/// どの変種も失われない読み方であり、判定は適用のときに `schema-engine` が行う）。
fn column_types(document: &Document, sheet: SheetId) -> Result<Vec<ColumnTypeInfo>, HostError> {
    let Some(found) = document.sheet_by_id(sheet) else {
        return Err(HostError::new(format!("シート {sheet} はこの文書に無い")));
    };
    let declaration = found.root_schema();
    let schema = parse_schema(declaration.root().as_str())
        .map_err(|error| HostError::new(format!("シート {sheet} の宣言を解釈できない: {error}")))?;
    let definitions = declaration
        .type_defs()
        .iter()
        .map(|definition| {
            Ok(TypeDefinition {
                id: definition.id(),
                definition: parse_type_definition(definition.definition().as_str())?,
            })
        })
        .collect::<Result<Vec<TypeDefinition>, SchemaError>>()
        .map_err(|error| HostError::new(format!("シート {sheet} の型定義を解釈できない: {error}")))?;
    let resolver = resolve(&schema, &definitions)
        .map_err(|error| HostError::new(format!("シート {sheet} の型の参照を解決できない: {error}")))?;
    Ok(schema
        .columns
        .iter()
        .map(|column| ColumnTypeInfo {
            name: column.name.to_string(),
            kind: declared_kind(&resolver, &column.ty),
            required: column.required,
            unique: column.unique,
        })
        .collect())
}

/// 宣言された型を種別へ落とす（`$ref` の連鎖は [`Resolver`] が辿る）。
fn declared_kind(resolver: &Resolver, ty: &TypeDecl) -> TypeKind {
    match resolver.follow(ty) {
        Some(TypeDecl::Kind {
            kind: DeclaredKind::Known(kind),
            ..
        }) => *kind,
        // 未知の種別トークン（未来の版の宣言）と、防御の枝（`resolve` を通った宣言では
        // 到達しない）。**Any はどの変種も失わない**ため、読みの側で嘘をつかない唯一の選択である。
        Some(TypeDecl::Kind {
            kind: DeclaredKind::Unknown(_),
            ..
        })
        | Some(TypeDecl::Ref(_))
        | None => TypeKind::Any,
    }
}

/// 10 万行 × 30 列の全行読み + 集計を計測する（予算: 10 秒。要件 11.1）。
///
/// # 測定条件
///
/// * 標本は 10 万行 × 30 列（要件 11.1 の規模そのもの）。
/// * マクロは**1 回の呼び出し**で全行を読み（要件 4.4 / 11.3）、**すべてのセルを集計に使う**。
/// * **計測の外で 1 度実行し**、`Ran` であることと、戻り値の `行数/列数` が標本の形
///   （10 万行 × 30 列）と一致することを確かめる（標本を黙って縮める変更を検出する。
///   集計の値そのものは検算しない — 予算の判定に要るのは「全部を使った」ことである）。
fn read_100k_rows_x_30_columns(c: &mut Criterion) {
    let sample = large_sample();
    let parts = sample.into_edit_parts();
    let document = Arc::new(parts.document);
    let runtime = MacroRuntime::new().expect("実行基盤を起こせる");

    let outcome = run(
        &runtime,
        Arc::new(SampleHost::new(Arc::clone(&document))),
        "一括の読み",
        READ_SOURCE,
    );
    let (value, _) = ran(&outcome, "全行の読み + 集計");
    assert!(
        text(value).starts_with(&format!("{ROWS}/{COLUMNS}/")),
        "標本の形が要件 11.1 の計測条件と一致しない（戻り値 {value}）"
    );

    let mut group = c.benchmark_group("bulk");
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(MEASUREMENT);

    group.bench_function("read_100k_rows_x_30_columns", |b| {
        b.iter(|| {
            let host = Arc::new(SampleHost::new(Arc::clone(&document)));
            std::hint::black_box(run(&runtime, host, "一括の読み", READ_SOURCE))
        })
    });

    group.finish();
}

/// 1 万行 × 30 列の書き換えを計測する（予算: 5 秒。要件 11.2）。
///
/// # 測定条件
///
/// * 標本は 10 万行 × 30 列であり、**先頭の 1 万行の全 30 列**を書き換える（モジュール docs
///   の「列は 30 列すべてを書く」）。
/// * **計測の外で 1 度実行し**、`Ran` であることと、集約された変更が **1 万行 × 30 列 =
///   30 万セル**であることを確かめる（書き込みが縫い目へ届かない変更・列を落とす変更を
///   検出する）。変更は**適用しない**（適用はアダプタの仕事であり、要件 11.2 が測るのは
///   マクロが終わるまでの時間である）。
fn rewrite_10k_rows(c: &mut Criterion) {
    let parts = large_sample().into_edit_parts();
    let document = Arc::new(parts.document);
    let runtime = MacroRuntime::new().expect("実行基盤を起こせる");

    let outcome = run(
        &runtime,
        Arc::new(SampleHost::new(Arc::clone(&document))),
        "一括の書き換え",
        REWRITE_SOURCE,
    );
    let (value, total) = ran(&outcome, "1 万行の書き換え");
    assert!(
        text(value).starts_with(&format!("{REWRITE_ROWS}/{}", REWRITE_ROWS * COLUMNS)),
        "書き換えた行数と書き込み数が計測条件と一致しない（戻り値 {value}）"
    );
    assert_eq!(
        total,
        REWRITE_ROWS * COLUMNS,
        "集約された変更の件数が 1 万行 × 30 列でない"
    );

    let mut group = c.benchmark_group("bulk");
    group.sample_size(SAMPLE_SIZE);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(MEASUREMENT);

    group.bench_function("rewrite_10k_rows", |b| {
        b.iter(|| {
            let host = Arc::new(SampleHost::new(Arc::clone(&document)));
            std::hint::black_box(run(&runtime, host, "一括の書き換え", REWRITE_SOURCE))
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    read_100k_rows_x_30_columns,
    rewrite_10k_rows
);
criterion_main!(benches);

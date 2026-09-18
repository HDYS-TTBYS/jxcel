//! 検証専用: **マクロを 1 件持つ標本の文書**を書き出す例（tasks.md 5.1）。
//!
//! # なぜ例なのか（出荷物へ入れない）
//!
//! 5.2 は「検証用のビルドを起動すると、仕込まれたマクロが一覧に現れ、実行まで進むこと」を
//! **実起動で**観測する。そのためには**実在する `.jxcel`**（実行できるマクロを 1 件持ち、
//! マクロが書き込む先のシートと行を持つもの）が要る。**標本は生成器で作る** — 手で作った
//! 文書をリポジトリへ置かない（`crates/data-grid/examples/make-large-sheet.rs` と同じ作法で
//! あり、`scripts/ci/*/verify-grid-observation.*` がその生成器を走らせて標本を作る）。
//!
//! **本ファイルは出荷物の一部ではない。** `examples/` はアプリのバイナリにも配布物にも入らず、
//! さらに非既定の feature（`verification-samples`）を要求する（`Cargo.toml` の `[[example]]`）
//! ので、既定のビルドでは**コンパイルすらされない**（`crates/data-grid/Cargo.toml` の同じ節と
//! `crates/document-format/Cargo.toml` の fixture の規約）。
//!
//! # 何を書くか（上流の生成器を使う）
//!
//! 文書は**上流（`document-format`）の公開面だけ**で組み立てる — 生の JSON も ZIP も手で
//! 書かない（形式の知識を本ファイルへ写し取らない）。シート・列・行・マクロの 4 つを上流の
//! API で積み、保存は `DocumentFormatApi::save` に任せる。**書き出したあと開き直して確かめる**
//! （マクロのソースがバイト単位で一致すること。要件 1.5 の往復）。
//!
//! 標本の中身:
//!
//! | 何 | 値 |
//! |---|---|
//! | シート | `在庫` 1 枚 |
//! | 列 | `品名` / `数量` |
//! | 行 | 3 行（りんご 3 / みかん 5 / ぶどう 8） |
//! | マクロ | `標本の記入` 1 件（TypeScript。先頭行を読み、`数量` に 100 を書いて戻り値を返す） |
//!
//! マクロは**能力を宣言しない**（セルの読みと書きは能力を要さない。要件 8.1）ので、能力の門に
//! 阻まれず実行まで進む。**書き込みを 1 セルだけ行う**のは、実行の記録の変更の件数を 5.2 が
//! 要件値（1 セル）で判定できるようにするためである（`crates/macro-runtime/src/host/changes.rs`
//! の「空の並びは記録しない」と同じ規律で、書き込みは 1 件だけ置く）。
//!
//! # 決定性 — 何が同じで、何が同じでないか
//!
//! **2 回の生成はバイト列として一致しない。** 文書識別子・シート識別子・行識別子は発行時刻を
//! 含む ULID である（`crates/data-grid/tests/common/sample.rs` の「決定性」節と同じ理由であり、
//! 「識別子を指定して行を作る」公開の口が無い）。同じなのは**中身**である: マクロの
//! （名前・種別・ソース）・列の並び・行数・セルの値。本ファイルは**書き出したものを開き直して
//! 1 行（`標本の内容:`）に写すので、2 回の実行の写しを比べれば「同じ内容が生成できる」ことを
//! 実測できる（5.1 の受け入れの観測はこの行と、マクロのソースのバイト一致である）。
//!
//! # 使い方
//!
//! ```text
//! cargo run -p macro-runtime --example make-macro-document --features verification-samples -- <出力先>
//! ```
//!
//! # 終了コード
//!
//! `0` = 書き出した／`2` = 入力が使えない（引数の不足。検査器の 3 値の使い方に合わせる）／
//! `1` = 書き出し・開き直しの失敗（要求は成立したが操作が失敗した）。

use std::path::PathBuf;
use std::process::ExitCode;

use document_format::{
    CellValue, Document, DocumentFormat, DocumentFormatApi, MacroKind, MacroRecord, SchemaPart,
};
use schema_engine::{
    schema_to_text, ColumnDecl, Constraints, DeclaredKind, Schema, TypeDecl, TypeKind,
};

/// 標本のシート名。
const SPECIMEN_SHEET: &str = "在庫";

/// 標本の列（順序がそのまま `host.columns` の並びである）。
const SPECIMEN_COLUMNS: [&str; 2] = ["品名", "数量"];

/// 標本の行（品名, 数量）。
const SPECIMEN_ROWS: [(&str, i64); 3] = [("りんご", 3), ("みかん", 5), ("ぶどう", 8)];

/// 標本のマクロの名前。**5.1 の引き金（`JXCEL_VERIFICATION_MACRO_RUN`）はこの名前を指す。**
const SPECIMEN_MACRO_NAME: &str = "標本の記入";

/// 標本のマクロのソース（TypeScript）。
///
/// **型注釈をわざと書いてある** — 実行の経路が TypeScript の変換（型注釈の除去）を通ることまで
/// 観測できる（1 行目のコメントではなく注釈そのものが、変換が走ったことの材料になる）。
/// セルの読みと書きは能力を要さないので宣言（`// @grant …`）は無い。
///
/// **書くのは 1 セルだけである**（変更の件数を 5.2 が要件値で読む）。
const SPECIMEN_MACRO_SOURCE: &str = "// 検証専用の標本のマクロ（tasks.md 5.1）。能力の宣言は要らない。\n\
     const sheets: SheetInfo[] = host.sheets();\n\
     const page: RowPage = host.readRange(sheets[0].id, { from: 0, to: 0 });\n\
     const row: ReadRow = page.rows[0];\n\
     host.setCells(sheets[0].id, [{ row: row.id, column: 1, value: 100 }]);\n\
     export default row.cells[0];\n";

/// 標本の列の宣言を 1 本組み立てる（種別は組み込む列に合わせる）。
fn column(name: &str, kind: TypeKind) -> ColumnDecl {
    ColumnDecl {
        name: name.into(),
        ty: TypeDecl::Kind {
            kind: DeclaredKind::Known(kind),
            constraints: Constraints::default(),
        },
        required: false,
        unique: false,
        default: None,
        description: None,
    }
}

/// 標本のルートスキーマを組み立てる（**上流の正準出力を使う**）。
///
/// **`SchemaPart::empty()`（`{"root":null}`）では駄目である。** 表を開く経路は宣言から列の
/// 計画を組むので、宣言が空だと列 0 本の計画になり、`GridSession::open` がそれを拒む
/// （`crates/data-grid/tests/grid_session.rs` の「列を 1 本も宣言していない計画では開けない」）。
/// 文書の列の並び（[`SPECIMEN_COLUMNS`]）と**同じ名前・同じ順序**で宣言する — 表が読む列と
/// 行データの鍵が食い違うと、標本は開けても中身が空になる。
fn specimen_schema() -> SchemaPart {
    let schema = Schema {
        columns: vec![
            column("品名", TypeKind::Text),
            column("数量", TypeKind::Int),
        ],
    };
    let root = schema_to_text(&schema).expect("標本の宣言は正準出力できる");
    SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#)).expect("標本の宣言は解析できる")
}

/// 標本の文書を組み立てる（**上流の公開面だけを使う**）。
fn specimen() -> Document {
    let mut document = Document::new();
    let sheet = document.add_sheet(SPECIMEN_SHEET);
    document
        .set_sheet_columns(
            sheet,
            SPECIMEN_COLUMNS.iter().map(|name| (*name).to_owned()).collect(),
        )
        .expect("いま追加したシートは実在する");
    document
        .set_root_schema(sheet, specimen_schema())
        .expect("いま追加したシートは実在する");
    for (name, quantity) in SPECIMEN_ROWS {
        let row = document.add_row(sheet).expect("いま追加したシートは実在する");
        document
            .set_row_values(
                sheet,
                row,
                vec![
                    CellValue::Text(name.to_owned()),
                    CellValue::Int(quantity),
                ],
            )
            .expect("いま追加した行は実在する");
    }
    document.set_macros(vec![MacroRecord::new(
        SPECIMEN_MACRO_NAME,
        MacroKind::TypeScript,
        SPECIMEN_MACRO_SOURCE,
    )]);
    document
}

/// 開き直した文書の**中身**を 1 行に写す（決定性の比較の材料。冒頭の「決定性」節）。
///
/// **識別子とパスは出さない** — それらは実行ごとに変わり（ULID）、比較の対象ではない。
fn describe_content(document: &Document, source_bytes: usize) -> String {
    let columns = document
        .sheets()
        .iter()
        .map(|sheet| sheet.columns().join(", "))
        .collect::<Vec<_>>()
        .join(" / ");
    let rows = document
        .sheets()
        .iter()
        .map(|sheet| sheet.rows().len().to_string())
        .collect::<Vec<_>>()
        .join(" / ");
    let cells: usize = document
        .sheets()
        .iter()
        .map(|sheet| {
            sheet
                .rows()
                .iter()
                .map(|row| row.values().len())
                .sum::<usize>()
        })
        .sum();
    let macro_line = document
        .macros()
        .iter()
        .map(|record| {
            // 種別の綴りは上流の `Debug` をそのまま使う（本ファイルが綴りを持つと、上流が
            // 変種を足したときに写しが黙って古くなる。表示用の 1 行にすぎない）。
            format!(
                "{} / {:?} / ソース = {} B",
                record.name(),
                record.kind(),
                record.source().len()
            )
        })
        .collect::<Vec<_>>()
        .join(" / ");
    format!(
        "標本の内容: マクロ = {} 件（{macro_line}）/ シート = {} 件 / シート名 = {} / 列 = {columns} / \
         行 = {rows} / セル = {cells} / ソースの読み込み = {source_bytes} B",
        document.macros().len(),
        document.sheets().len(),
        document
            .sheets()
            .iter()
            .map(|sheet| sheet.name().to_owned())
            .collect::<Vec<_>>()
            .join(" / "),
    )
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        eprintln!("使い方: make-macro-document <出力先>（例: target/observation/macro-sample.jxcel）");
        return ExitCode::from(2);
    }
    let path = PathBuf::from(&args[0]);

    let api = DocumentFormat::new();
    if let Err(error) = api.save(&specimen(), &path) {
        eprintln!("NG: 標本の書き出しに失敗した: {error}");
        return ExitCode::from(1);
    }

    // **書けたことで終わらない。** 開き直し、標本が要求どおりの中身を持つことをここで確かめる
    // （5.1 の標本は「マクロを 1 件持ち、実行が変更を 1 セル生む」ことが要件である）。
    let reopened = match api.open(&path) {
        Ok(opened) => opened.document,
        Err(error) => {
            eprintln!("NG: 書き出した標本を開き直せなかった: {error}");
            return ExitCode::from(1);
        }
    };
    if reopened.macros().len() != 1 {
        eprintln!(
            "NG: 標本のマクロが 1 件ではない（{} 件）",
            reopened.macros().len()
        );
        return ExitCode::from(1);
    }
    let record = &reopened.macros()[0];
    if record.name() != SPECIMEN_MACRO_NAME
        || record.kind() != MacroKind::TypeScript
        || record.source().as_bytes() != SPECIMEN_MACRO_SOURCE.as_bytes()
    {
        eprintln!("NG: 保存と読み込みの間でマクロが変わった（名前・種別・ソース）");
        return ExitCode::from(1);
    }
    let bytes = match std::fs::metadata(&path) {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            eprintln!("NG: 書き出した標本の大きさを読めなかった: {error}");
            return ExitCode::from(1);
        }
    };
    println!("{}", describe_content(&reopened, record.source().len()));
    println!(
        "標本を書き出した: パス={} バイト数={bytes}",
        path.display()
    );
    ExitCode::SUCCESS
}

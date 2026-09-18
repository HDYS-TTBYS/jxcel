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
//! | マクロ | 5 件（すべて TypeScript。並びは保存順である） |
//!
//! マクロの 5 件は **5.2 の検査器が駆動するシナリオ**である（1 件ずつが 1 つの要件の観測を
//! 生む。どれが何を観測させるかは [`SPECIMEN_MACRO_SOURCES`] の表を見ること）:
//!
//! | マクロ | 何を観測させるか |
//! |---|---|
//! | `標本の記入` | 実行の成功と変更の件数（1 セル。要件 2.1, 5.1, 5.5） |
//! | `標本の往復` | 保存と開き直しの往復（引き金のセッションの経路が保存した文書を開き直したときだけ 1 セル書く。要件 1.2, 1.3, 1.5） |
//! | `標本の拒否` | 宣言の無い能力の拒否（要件 8.3） |
//! | `標本の失敗` | 失敗の理由とフレーム（3 行目で投げる。要件 9.1–9.3） |
//! | `標本の打ち切り` | 時間の上限による打ち切り（終わらない繰り返し。要件 6.1, 6.4） |
//!
//! マクロは**どれも能力を宣言しない**（セルの読みと書きは能力を要さない。要件 8.1）ので、
//! 能力の門に阻まれず実行まで進む。**書き込みを 1 セルだけ行う**のは、実行の記録の変更の件数を
//! 5.2 が要件値（1 セル）で判定できるようにするためである（`crates/macro-runtime/src/host/changes.rs`
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

/// 標本のマクロの名前（**この並びが保存順であり、一覧の並びである**）。
///
/// **5.1 の引き金（`JXCEL_VERIFICATION_MACRO_RUN`）はこの名前を指す。** 5.2 の検査器は
/// **この並びを閉じた一覧として要求する** — 標本へマクロを足す／名前を変えるときは
/// `scripts/check-macro-observation.sh` の `EXPECTED_NAMES` を同じ作業で直す（並びも含めて
/// 一致しなければ検査器は落ちる。片方だけ直すと落ちるので、写しが黙って古くならない）。
const SPECIMEN_MACRO_NAMES: [&str; 5] = [
    "標本の記入",
    "標本の往復",
    "標本の拒否",
    "標本の失敗",
    "標本の打ち切り",
];

/// 標本のマクロのソース（TypeScript）。**並びは [`SPECIMEN_MACRO_NAMES`] と 1 対 1 である。**
///
/// 5.2 は実起動の観測を**シナリオごとのマクロ**で駆動する（1 つのマクロで 5 つの要件を
/// 見ようとすると、観測の行のどの欄がどの要件の材料かが言えなくなる）。各マクロの冒頭に
/// **どの要件のための標本か**を書いてある:
///
/// | マクロ | 何を観測させるか |
/// |---|---|
/// | `標本の記入` | 実行の成功と変更の件数（要件 2.1, 5.1, 5.5）。**書くのは 1 セルだけ**である |
/// | `標本の往復` | 保存と開き直しの往復（要件 1.2, 1.3, 1.5）。引き金のセッションの経路が 2 行を回転させて保存するので、開き直すと先頭行の数量は 5 になる |
/// | `標本の拒否` | 宣言の無い能力の拒否（要件 8.3）。`@grant` を書かないので門が呼び出しの前に拒む |
/// | `標本の失敗` | 失敗の理由とフレーム（要件 9.1–9.3）。関数の入れ子の内側で例外を投げる |
/// | `標本の打ち切り` | 時間の上限による打ち切り（要件 6.1）。終わらない繰り返しに入る |
///
/// **どれも能力を宣言しない**（セルの読みと書きは能力を要さない。要件 8.1）ので、能力の門に
/// 阻まれず実行まで進む。**型注釈をわざと書いてある** — 実行の経路が TypeScript の変換
/// （型注釈の除去）を通ることまで観測できる（コメントではなく注釈そのものが、変換が走った
/// ことの材料になる）。
const SPECIMEN_MACRO_SOURCES: [&str; 5] = [
    // 1. 標本の記入（5.1 と同じ。**書くのは 1 セルだけ**である — 変更の件数を 5.2 が
    //    要件値（1 セル）で読む）。
    "// 検証専用: 実行の成功と変更の件数（tasks.md 5.2。要件 2.1, 5.1, 5.5）。\n\
     const sheets: SheetInfo[] = host.sheets();\n\
     const page: RowPage = host.readRange(sheets[0].id, { from: 0, to: 0 });\n\
     const row: ReadRow = page.rows[0];\n\
     host.setCells(sheets[0].id, [{ row: row.id, column: 1, value: 100 }]);\n\
     export default row.cells[0];\n",
    // 2. 標本の往復（保存と開き直し。要件 1.2, 1.3, 1.5）。**条件つきの書き込み**である —
    //    引き金のセッションの経路（`open,edit,2,save`）が 2 行を回転させて保存するので、
    //    開き直した文書の先頭行の数量は 5 である（初期値は 3）。3 のままなら（＝保存された
    //    文書を開き直していないなら）**1 件も書かない**ので、変更の件数が 0 になり、検査器は
    //    「往復していない」と判定できる。
    "// 検証専用: 保存と開き直しの往復（tasks.md 5.2。要件 1.2, 1.3, 1.5）。\n\
     const sheets: SheetInfo[] = host.sheets();\n\
     const page: RowPage = host.readRange(sheets[0].id, { from: 0, to: 2 });\n\
     const 先頭: CellValue = page.rows[0].cells[1];\n\
     if (先頭 === 5) {\n\
     \x20 host.setCells(sheets[0].id, [{ row: page.rows[2].id, column: 1, value: 200 }]);\n\
     }\n\
     export default 先頭;\n",
    // 3. 標本の拒否（要件 8.3）。**`@grant file.read` を書かない**ので、門が呼び出しの前に
    //    拒む（ファイルは 1 バイトも読まれない）。
    "// 検証専用: 宣言の無い能力の拒否（tasks.md 5.2。要件 8.3）。\n\
     const 本文: string = host.fileRead(\"/etc/hostname\");\n\
     export default 本文;\n",
    // 4. 標本の失敗（要件 9.1–9.3）。**投げる位置を 3 行目に固定する** — 検査器は
    //    フレームが保存されたソースの原位置（3 行目）を指すことを要件値として要求する。
    "// 検証専用: 失敗の理由とフレーム（tasks.md 5.2。要件 9.1–9.3）。\n\
     function 内側(): number {\n\
     \x20 throw new Error(\"検証用の失敗\");\n\
     }\n\
     function 外側(): number {\n\
     \x20 return 内側();\n\
     }\n\
     export default 外側();\n",
    // 5. 標本の打ち切り（要件 6.1）。**終わらない繰り返し**に入り、時間の上限（既定 30 秒）で
    //    打ち切られる。検査器は同じ起動の続けて別のマクロを走らせ、**打ち切りの後も操作できる**
    //    こと（要件 6.4）まで観測する。
    "// 検証専用: 時間の上限による打ち切り（tasks.md 5.2。要件 6.1, 6.4）。\n\
     let 合計 = 0;\n\
     while (true) {\n\
     \x20 合計 += 1;\n\
     }\n\
     export default 合計;\n",
];

/// 標本のマクロを上流の記録の型へ組む（並びは [`SPECIMEN_MACRO_NAMES`] と同じ）。
fn specimen_macros() -> Vec<MacroRecord> {
    SPECIMEN_MACRO_NAMES
        .iter()
        .zip(SPECIMEN_MACRO_SOURCES)
        .map(|(name, source)| MacroRecord::new(*name, MacroKind::TypeScript, source))
        .collect()
}

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
            SPECIMEN_COLUMNS
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        )
        .expect("いま追加したシートは実在する");
    document
        .set_root_schema(sheet, specimen_schema())
        .expect("いま追加したシートは実在する");
    for (name, quantity) in SPECIMEN_ROWS {
        let row = document
            .add_row(sheet)
            .expect("いま追加したシートは実在する");
        document
            .set_row_values(
                sheet,
                row,
                vec![CellValue::Text(name.to_owned()), CellValue::Int(quantity)],
            )
            .expect("いま追加した行は実在する");
    }
    document.set_macros(specimen_macros());
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
        eprintln!(
            "使い方: make-macro-document <出力先>（例: target/observation/macro-sample.jxcel）"
        );
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
    if reopened.macros().len() != SPECIMEN_MACRO_NAMES.len() {
        eprintln!(
            "NG: 標本のマクロが {} 件ではない（{} 件）",
            SPECIMEN_MACRO_NAMES.len(),
            reopened.macros().len()
        );
        return ExitCode::from(1);
    }
    // **1 件ずつ、名前・種別・ソースのバイト一致を確かめる**（要件 1.2, 1.5）。並びも
    // 保存順のままであることを見る（5.2 の検査器が一覧の並びを要件値として読む）。
    for (index, (expected_name, expected_source)) in SPECIMEN_MACRO_NAMES
        .iter()
        .zip(SPECIMEN_MACRO_SOURCES)
        .enumerate()
    {
        let Some(record) = reopened.macros().get(index) else {
            eprintln!("NG: 標本のマクロ {index} 番目を開き直せなかった");
            return ExitCode::from(1);
        };
        if record.name() != *expected_name
            || record.kind() != MacroKind::TypeScript
            || record.source().as_bytes() != expected_source.as_bytes()
        {
            eprintln!(
                "NG: 保存と読み込みの間でマクロが変わった（{index} 番目: 名前 = {} / 種別 = {:?} / \
                 ソース = {} B）",
                record.name(),
                record.kind(),
                record.source().len()
            );
            return ExitCode::from(1);
        }
    }
    let bytes = match std::fs::metadata(&path) {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            eprintln!("NG: 書き出した標本の大きさを読めなかった: {error}");
            return ExitCode::from(1);
        }
    };
    let source_bytes: usize = reopened
        .macros()
        .iter()
        .map(|record| record.source().len())
        .sum();
    println!("{}", describe_content(&reopened, source_bytes));
    println!("標本を書き出した: パス={} バイト数={bytes}", path.display());
    ExitCode::SUCCESS
}

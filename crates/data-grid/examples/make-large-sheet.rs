//! 検証専用: 3 OS の観測（tasks.md 9.2）が起動の引数へ渡す**標本の文書**を書き出す例。
//!
//! # なぜ例なのか（出荷物へ入れない）
//!
//! 9.2 は「10 万行のシートを開き、末尾へ移動し、セルを編集し、取り消して戻すまでを**実際に
//! 起動して**観測する」ことを要求する。1.6 と 7.2 の使い捨ての面は 10 万行を**フロントエンドで
//! 合成**していたが、9.2 は**実物のアプリが実物の文書を開く**経路を観測するので、10 万行 ×
//! 30 列の**実在する `.jxcel`** が要る。**標本の生成器は 1 つでなければならない**（tasks.md
//! 1.4）ので、本ファイルは `tests/common/sample.rs` の `sample()` をそのまま使う — 写しを
//! 作らない（ベンチと同じ `#[path]` の取り込みである）。
//!
//! **本ファイルは出荷物の一部ではない。** `examples/` はアプリのバイナリにも配布物にも入らず、
//! さらに非既定の feature（`verification-samples`）を要求する（`Cargo.toml` の `[[example]]`）
//! ので、既定のビルドでは**コンパイルすらされない**（`verification.md`「検証専用のコードの
//! 置き場」の Rust 側の規約と同じ形である — ただし対象は `src-tauri` ではなく本クレートで
//! あり、本クレートは出荷物を作らない）。
//!
//! # 何を書くか
//!
//! `sample(&SampleOptions::new(rows, columns))` が組み立てた文書を、上流の保存経路
//! （`DocumentFormatApi::save`）でそのまま書く。**本ファイルは文書の形を一切変えない** — 列の
//! 種類・入れ子・違反の位置は 1.4 の生成器が決める（9.2 の筋書きはそれらに依る: 入れ子の展開は
//! `届け先` / `明細` / `改訂履歴` を使い、参照の面は参照列を使う）。
//!
//! # 使い方
//!
//! ```text
//! cargo run -p data-grid --example make-large-sheet --features verification-samples -- \
//!   <行数> <列数> <出力先>
//! ```
//!
//! 出力の 1 行（`標本を書き出した: …`）は、段が**要求した規模で書けたこと**を確かめるために
//! 出す（黙って小さい標本へ縮退しない）。
//!
//! # 終了コード
//!
//! `0` = 書き出した / `2` = 入力が使えない（引数の不足・解釈できない数・範囲外。検査器の規約
//! と同じ 3 値の使い方である — 書き出しの失敗は `1`）。

use std::path::PathBuf;
use std::process::ExitCode;

use common::sample::{sample, SampleOptions, SAMPLE_COLUMNS};
use document_format::{DocumentFormat, DocumentFormatApi};

#[path = "../tests/common/mod.rs"]
mod common;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        eprintln!(
            "使い方: make-large-sheet <行数> <列数> <出力先>（例: 100000 30 /tmp/sample.jxcel）"
        );
        return ExitCode::from(2);
    }
    let rows: usize = match args[0].parse() {
        Ok(rows) if rows >= 1 => rows,
        _ => {
            eprintln!("NG: 行数は 1 以上の整数であること: {:?}", args[0]);
            return ExitCode::from(2);
        }
    };
    let columns: usize = match args[1].parse() {
        Ok(columns) if columns >= 1 && columns <= SAMPLE_COLUMNS => columns,
        _ => {
            eprintln!(
                "NG: 列数は 1..={SAMPLE_COLUMNS} の整数であること: {:?}",
                args[1]
            );
            return ExitCode::from(2);
        }
    };
    let path = PathBuf::from(&args[2]);

    // **1.4 の生成器をそのまま使う**（標本の定義を 2 つに割らない）。
    let built = sample(&SampleOptions::new(rows, columns));
    let document = built.document();
    let api = DocumentFormat::new();
    if let Err(error) = api.save(document, &path) {
        eprintln!("NG: 標本の書き出しに失敗した: {error}");
        return ExitCode::from(1);
    }
    let bytes = match std::fs::metadata(&path) {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            eprintln!("NG: 書き出した標本の大きさを読めなかった: {error}");
            return ExitCode::from(1);
        }
    };
    // 要求した規模と書けた規模を同じ行に出す（段はこの行で「縮退していない」ことを確かめる）。
    println!(
        "標本を書き出した: 行数={rows} 列数={columns} バイト数={bytes} パス={}",
        path.display()
    );
    ExitCode::SUCCESS
}

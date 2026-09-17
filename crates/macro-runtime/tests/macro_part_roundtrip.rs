//! マクロの並びが**文書の保存と読み込みを跨いで変わらない**こと（tasks.md 1.6 の受け入れ。
//! 要件 1.2, 1.5）。
//!
//! 上流の `document-format` のパート（`macros.json`。1.2 が形だけを足した）を通した**実ファイル
//! の往復**であり、本クレートの側の規約（ソースは保存されたまま保持する。整形しない）と、
//! 能力宣言の解釈（1.6）が同じ 1 本のマクロに対して両立することを確かめる。
//!
//! **1.2 のテスト（`document-format` の `roundtrip.rs`）との分界**: あちらは**形式の側**の
//! 往復（決定性・未知フィールドの差し戻し・形式版のゲート）を固定する。こちらは**マクロの
//! 意味の側**（ソースのバイト一致と、宣言の解析が同じソースから取れること）を固定する。

use std::path::PathBuf;

use document_format::{Document, DocumentFormat, DocumentFormatApi};
use macro_runtime::{parse_capabilities, Capability, MacroKind, MacroName, MacroRecord};

/// 使い捨ての保存先（`std::env::temp_dir` の下。`tempfile` を足さない）。
fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    path.push(format!("jxcel-macro-runtime-{name}-{unique}.jxcel"));
    path
}

#[test]
fn the_source_and_its_declaration_survive_save_and_reopen() {
    // 宣言つきのソースを 1 件持つ文書（**ソースは整形しない** — 空白とコメントのまま）。
    let source =
        "// @grant file.read, net\n\nconst rows = await host.readRange(0, 10);\nrows.length\n";
    let record = MacroRecord::new(
        MacroName::new("棚卸し"),
        MacroKind::TypeScript,
        source.to_owned(),
    );
    let expected = parse_capabilities(source).expect("宣言が読める");
    assert_eq!(
        expected.iter().collect::<Vec<_>>(),
        vec![Capability::FileRead, Capability::Net]
    );

    // **上流の記録へ写す**（`document-format` の側の記録は形だけを持つ。design 決定 4）。
    // 製品の経路ではこの写像をアダプタ（群 4）が担う。
    let upstream = document_format::MacroRecord::new(
        record.name.as_str(),
        match record.kind {
            MacroKind::TypeScript => document_format::MacroKind::TypeScript,
            MacroKind::JavaScript => document_format::MacroKind::JavaScript,
        },
        record.source.clone(),
    );
    let mut document = Document::new();
    document.set_macros(vec![upstream]);

    let path = scratch("roundtrip");
    DocumentFormat::new()
        .save(&document, &path)
        .expect("保存できる");
    let reopened = DocumentFormat::new()
        .open(&path)
        .expect("開き直せる")
        .document;
    let _ = std::fs::remove_file(&path);

    let macros = reopened.macros();
    assert_eq!(macros.len(), 1, "マクロの件数が変わった");
    assert_eq!(macros[0].name(), "棚卸し");
    assert_eq!(
        macros[0].source().as_bytes(),
        source.as_bytes(),
        "保存と読み込みの間でソースが変わった（整形してはならない。要件 1.5）"
    );
    // **同じソースから同じ宣言が取れる**（往復の後もマクロは同じ意味を持つ）。
    assert_eq!(
        parse_capabilities(macros[0].source())
            .expect("宣言が読める")
            .iter()
            .collect::<Vec<_>>(),
        expected.iter().collect::<Vec<_>>()
    );
}

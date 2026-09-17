//! 生成物のドリフト検査（tasks.md 3.3。要件 10.2, 10.3）。
//!
//! `types/macro-host.d.ts` はツールチェーンの生成物であり、正本は
//! `crates/macro-runtime/src/surface/declaration.rs` の `HOST_APIS` と
//! `crates/macro-runtime/src/types.rs` の型カタログである。正本を変えて生成をやり直さないと、
//! 補完を使う側（後続の `macro-editor-lsp` と標準マクロライブラリ）は古い型のまま動き続ける。
//!
//! 本検査は 2 つを固定する:
//!
//! 1. 追跡済みファイルと [`generate`] の出力の**バイト一致**（「Rust 側が変わったのに生成物が
//!    古い」を捕まえる）
//! 2. 追跡済みファイルと宣言表の一致（[`check`]）— 宣言にある API が `.d.ts` に無い／
//!    `.d.ts` にある API が宣言に無い／宣言の無い型が現れる、を**名前つきで**報告する
//!    （要件 10.1, 10.3）
//!
//! 比較器は [`first_difference`] と [`report`] として切り出し、単体テストからも直接叩ける
//! ようにしてある。差分が出たときのメッセージに何を再生成すべきか（コマンドと位置）を含める
//! ことが本検査の主眼であり、その内容は下のテストで固定する。

use std::path::PathBuf;

use macro_runtime::types::{check, generate, GENERATED_PATH, REGENERATE_COMMAND};

/// 差分メッセージに載せる前後文脈の幅（バイト）。
const EXCERPT_WINDOW: usize = 60;

/// リポジトリルートを `CARGO_MANIFEST_DIR` から解決する。
///
/// `CARGO_MANIFEST_DIR` は `crates/macro-runtime/` を指すため、2 つ上がリポジトリルートである。
/// カレントディレクトリには依存させない（`cargo test` をどこから実行しても同じ結果になる）。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/macro-runtime はリポジトリ直下の crates/ 配下に置かれている")
        .to_path_buf()
}

/// 追跡済み生成物の絶対位置。
fn committed_path() -> PathBuf {
    repo_root().join(GENERATED_PATH)
}

/// 2 つのバイト列のうち、最初に食い違う位置を返す。完全一致なら `None`。
///
/// 一方が他方の接頭辞である場合（長さだけが違う場合）は、短い方の末尾を食い違い位置と
/// する。末尾への追記・削除を見逃さないための境界条件である。
fn first_difference(committed: &[u8], generated: &[u8]) -> Option<usize> {
    let shared = committed.len().min(generated.len());
    if let Some(offset) = committed[..shared]
        .iter()
        .zip(&generated[..shared])
        .position(|(a, b)| a != b)
    {
        return Some(offset);
    }
    if committed.len() != generated.len() {
        return Some(shared);
    }
    None
}

/// 差分の報告（**再生成のコマンドと、食い違った位置の前後**を含む）。
fn report(committed: &[u8], generated: &[u8], offset: usize) -> String {
    let excerpt = |bytes: &[u8]| {
        let start = offset.saturating_sub(EXCERPT_WINDOW);
        let end = (offset + EXCERPT_WINDOW).min(bytes.len());
        String::from_utf8_lossy(&bytes[start..end]).replace('\n', "\\n")
    };
    format!(
        "{path} が生成器の出力と食い違う（最初の違いは {offset} バイト目）。\n\
         追跡済み: {committed}\n\
         生成器  : {generated}\n\
         再生成（リポジトリルートで実行する）: {REGENERATE_COMMAND}\n\
         生成物は手で編集しない — 直すのは生成元（crates/macro-runtime/src/types.rs と \
         surface/declaration.rs）である。",
        path = GENERATED_PATH,
        committed = excerpt(committed),
        generated = excerpt(generated),
    )
}

/// 追跡済みの `.d.ts` が生成器の出力と**バイト一致**する（要件 10.3）。
#[test]
fn the_committed_file_matches_the_generator_byte_for_byte() {
    let generated = generate().expect("宣言表の型はすべて宣言を持つ");
    let path = committed_path();
    let committed = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "追跡済みの生成物を読めない（{}）: {error}\n再生成: {REGENERATE_COMMAND}",
            path.display()
        )
    });
    if let Some(offset) = first_difference(&committed, generated.as_bytes()) {
        panic!("{}", report(&committed, generated.as_bytes(), offset));
    }
}

/// 追跡済みの `.d.ts` が宣言表と一致する（宣言にある API が無い／宣言に無い API がある／
/// 宣言の無い型が現れる、のいずれでも落ちる。要件 10.1, 10.3）。
#[test]
fn the_committed_file_agrees_with_the_declaration_table() {
    check().unwrap_or_else(|divergence| {
        panic!("{divergence}\n再生成: {REGENERATE_COMMAND}");
    });
}

/// 比較器は末尾の追記・削除も食い違いとして扱う（長さだけが違う場合を見逃さない）。
#[test]
fn the_comparator_detects_appends_and_truncations() {
    assert_eq!(None, first_difference(b"abc", b"abc"));
    assert_eq!(Some(1), first_difference(b"abc", b"axc"));
    assert_eq!(Some(3), first_difference(b"abc", b"abcd"));
    assert_eq!(Some(3), first_difference(b"abcd", b"abc"));
}

/// 差分の報告は再生成のコマンドと食い違いの位置を含む（人がそのまま直せる）。
#[test]
fn the_report_names_the_regenerate_command_and_the_offset() {
    let message = report(b"abc", b"axc", 1);
    assert!(message.contains(REGENERATE_COMMAND), "{message}");
    assert!(message.contains("1 バイト目"), "{message}");
    assert!(message.contains(GENERATED_PATH), "{message}");
}

//! 生成物のドリフト検査（要件 4.3 の前半、tasks.md 2.3）。
//!
//! `src/ipc/bindings.ts` はツールチェーンの生成物であり、正本は
//! `crates/app-shell/src/ipc/` の境界の型と `command_names.rs` の `COMMAND_NAMES` である。
//! 正本を変えて生成をやり直さないと、フロントエンドは古い型のまま動き続ける。本検査は
//! [`render_bindings()`] の出力と追跡済みファイルを**バイト単位で**比較し、差があれば
//! `cargo test` を失敗させる。
//!
//! 要件 4.3 は 2 段で成立する（design.md「IpcContract」の Implementation Notes）。
//! ここは前半の「Rust 側が変わったのに生成物を更新していない」を捕まえる段であり、
//! 後半の「生成物は新しいがフロント側が追随していない」は `tsc --noEmit` が担う（10.1）。
//!
//! 比較器は [`compare`] として切り出し、単体テストからも直接叩けるようにしてある。
//! 差分が出たときのメッセージに何を再生成すべきか（コマンドと位置）を含めることが
//! 本タスクの主眼であり、その内容は [`compare`] の単体テストで固定する。

use std::path::{Path, PathBuf};

use app_shell::ipc::{render_bindings, REGENERATE_BINDINGS_COMMAND};

/// 追跡済み生成物の、リポジトリルートからの相対位置。
const BINDINGS_RELATIVE: &str = "src/ipc/bindings.ts";

/// 差分メッセージに載せる前後文脈の幅（バイト）。
const EXCERPT_WINDOW: usize = 40;

/// リポジトリルートを `CARGO_MANIFEST_DIR` から解決する。
///
/// `CARGO_MANIFEST_DIR` は `crates/app-shell/` を指すため、2 つ上がリポジトリルートである。
/// カレントディレクトリには依存させない（`cargo test` をどこから実行しても同じ結果になる）。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/app-shell はリポジトリ直下の crates/ 配下に置かれている")
        .to_path_buf()
}

/// 追跡済み生成物の絶対位置。
fn committed_bindings_path() -> PathBuf {
    repo_root().join(BINDINGS_RELATIVE)
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

/// `bytes` の `offset` が何行目にあたるかを 1 始まりで返す。
fn line_number(bytes: &[u8], offset: usize) -> usize {
    1 + bytes[..offset.min(bytes.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// `bytes` の `offset` 付近を、食い違い位置を `⟪⟫` で挟んで 1 行に収めた抜粋として返す。
///
/// 改行・復帰・タブはエスケープし、非 UTF-8 のバイトは置換文字として落とす。抜粋は人間が
/// 差分の当たりを付けるためのものであり、一致判定そのものはバイト列で行う。
fn excerpt(bytes: &[u8], offset: usize) -> String {
    let start = offset.saturating_sub(EXCERPT_WINDOW);
    let end = bytes.len().min(offset.saturating_add(EXCERPT_WINDOW));
    let window = &bytes[start..end];

    // 通常は有効な UTF-8 であるため、文字単位で挟んで日本語のコメントでも読めるようにする。
    if offset < end {
        if let Ok(text) = std::str::from_utf8(window) {
            let rel = offset - start;
            if text.is_char_boundary(rel) {
                let mut out = String::new();
                for (at, ch) in text.char_indices() {
                    if at == rel {
                        out.push('⟪');
                    }
                    push_escaped_char(&mut out, ch);
                    if at == rel {
                        out.push('⟫');
                    }
                }
                return out;
            }
        }
    }

    // 非 UTF-8、または食い違い位置が文字の途中である場合のフォールバック。
    let mut out = String::new();
    for (i, &byte) in window.iter().enumerate() {
        let at = start + i;
        if at == offset {
            out.push('⟪');
        }
        push_escaped_byte(&mut out, byte);
        if at == offset {
            out.push('⟫');
        }
    }
    if offset >= end {
        out.push_str("⟪⟫");
    }
    out
}

fn push_escaped_char(out: &mut String, ch: char) {
    match ch {
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        _ => out.push(ch),
    }
}

fn push_escaped_byte(out: &mut String, byte: u8) {
    match byte {
        b'\n' => out.push_str("\\n"),
        b'\r' => out.push_str("\\r"),
        b'\t' => out.push_str("\\t"),
        _ => out.push_str(&String::from_utf8_lossy(&[byte])),
    }
}

/// 生成結果と追跡済みのバイト列を比較する。一致すれば `Ok(())`、差があれば何を再生成すべきか
/// とどこが食い違うかを記したメッセージを返す。
///
/// `path` はメッセージに載せるためだけに使う（読み書きしない）。そのため単体テストから
/// 存在しない合成パスを渡せる。
fn compare(path: &Path, committed: &[u8], generated: &[u8]) -> Result<(), String> {
    let Some(offset) = first_difference(committed, generated) else {
        return Ok(());
    };

    let committed_line = line_number(committed, offset);
    let generated_line = line_number(generated, offset);

    let mut message = String::new();
    message
        .push_str("生成物のドリフトを検出した: 追跡済みの TypeScript が生成結果と一致しない。\n");
    message.push('\n');
    message.push_str(&format!("  追跡ファイル: {}\n", path.display()));
    message.push_str(&format!(
        "  リポジトリルートからの相対: {BINDINGS_RELATIVE}\n"
    ));
    message.push_str(&format!(
        "  再生成コマンド: {REGENERATE_BINDINGS_COMMAND}\n"
    ));
    message.push_str(
        "  （リポジトリルートで実行し、書き換わった内容をそのまま追跡対象としてコミットする）\n",
    );
    message.push('\n');
    message.push_str(&format!(
        "  追跡ファイルの長さ: {} バイト\n",
        committed.len()
    ));
    message.push_str(&format!("  生成結果の長さ: {} バイト\n", generated.len()));
    message.push_str(&format!(
        "  最初に食い違う位置: バイトオフセット {offset}（追跡側 {committed_line} 行目 / 生成側 {generated_line} 行目）\n"
    ));
    message.push('\n');
    message.push_str(&format!("  追跡側: {}\n", excerpt(committed, offset)));
    message.push_str(&format!("  生成側: {}\n", excerpt(generated, offset)));

    Err(message)
}

// ---------------------------------------------------------------------------
// 検査本体: 追跡済みファイルと生成結果のバイト比較
// ---------------------------------------------------------------------------

/// 木を変更していない状態では必ず通る。ここが落ちたときは、報告された再生成コマンドを
/// リポジトリルートで実行し、書き換わった `src/ipc/bindings.ts` をコミットする。
#[test]
fn committed_bindings_match_the_generator() {
    let path = committed_bindings_path();
    let committed = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("追跡済みの生成物を読めない: {} ({error})", path.display()));
    let generated = render_bindings().expect("境界の型から TypeScript を生成できなければならない");

    if let Err(message) = compare(&path, &committed, generated.as_bytes()) {
        panic!("{message}");
    }
}

// ---------------------------------------------------------------------------
// 比較器の単体テスト: 検査が載っていることを固定する
// ---------------------------------------------------------------------------

/// 対照。比較器が常に失敗するのではないことを固定する（これが無いと「常に落ちる検査」でも
/// 上のテストは通ってしまう）。
#[test]
fn comparison_accepts_identical_bytes() {
    let bytes = b"export type WindowLabel = string;\n";
    let path = Path::new("src/ipc/bindings.ts");
    assert!(
        compare(path, bytes, bytes).is_ok(),
        "同一のバイト列を不一致として報告してはならない"
    );
}

/// 差分を検出し、そのメッセージに「何を再生成すべきか」と「どこが食い違うか」が載ることを
/// 固定する。
#[test]
fn comparison_reports_mismatch_with_the_regeneration_command_and_location() {
    let path = Path::new("src/ipc/bindings.ts");
    let committed =
        b"export type WindowLabel = string;\nexport type WindowContext = { window: string, };\n";
    let generated =
        b"export type WindowLabel = string;\nexport type WindowContext = { window: string, label: string, };\n";

    let message = compare(path, committed, generated)
        .expect_err("食い違うバイト列を一致として扱ってはならない");

    assert!(
        message.contains(REGENERATE_BINDINGS_COMMAND),
        "メッセージに再生成コマンドが無い:\n{message}"
    );
    assert!(
        message.contains(BINDINGS_RELATIVE),
        "メッセージに追跡ファイルの名前が無い:\n{message}"
    );
    assert!(
        message.contains("最初に食い違う位置: バイトオフセット 80"),
        "メッセージに食い違い位置（オフセット）が無い:\n{message}"
    );
    assert!(
        message.contains("追跡側: ") && message.contains("生成側: "),
        "メッセージに両側の抜粋が無い:\n{message}"
    );
    // 抜粋が実際に食い違い箇所を指していること（`WindowContext` の直後の差）。
    assert!(
        message.contains("⟪") && message.contains("⟫"),
        "抜粋が食い違い位置を指していない:\n{message}"
    );
    assert!(
        message.contains(&format!("{} バイト", committed.len()))
            && message.contains(&format!("{} バイト", generated.len())),
        "メッセージに両側の長さが無い:\n{message}"
    );
}

/// 末尾の改行だけが違う場合でも検出することを固定する。生成物は末尾に改行を持つため、
/// 「改行を落としただけ」の差分もドリフトである。
#[test]
fn comparison_detects_a_trailing_newline_only_difference() {
    let path = Path::new("src/ipc/bindings.ts");
    let with_newline = b"export type WindowLabel = string;\n";
    let without_newline = b"export type WindowLabel = string;";

    let message = compare(path, without_newline, with_newline)
        .expect_err("末尾の改行の差を看過してはならない");
    assert!(
        message.contains("最初に食い違う位置: バイトオフセット 33"),
        "末尾の食い違いを短い方の末尾として報告していない:\n{message}"
    );
}

/// 片方が他方の接頭辞である場合（末尾の追記・削除）も、長さの違いとして検出することを
/// 固定する。
#[test]
fn comparison_detects_a_length_only_difference() {
    let path = Path::new("src/ipc/bindings.ts");
    let shorter = b"export type WindowLabel = string;\n";
    let longer = b"export type WindowLabel = string;\nexport const COMMAND_NAMES = [];\n";

    let message = compare(path, shorter, longer).expect_err("末尾への追記を看過してはならない");
    assert!(
        message.contains("最初に食い違う位置: バイトオフセット 34"),
        "接頭辞の一致を長さの違いとして報告していない:\n{message}"
    );
    assert!(
        message.contains("34 バイト"),
        "短い方の長さを報告していない:\n{message}"
    );
}

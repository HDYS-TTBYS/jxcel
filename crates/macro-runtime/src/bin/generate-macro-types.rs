//! TypeScript の生成物（`types/macro-host.d.ts`）を書き出す、唯一の文書化された入口
//! （tasks.md 3.3。design.md「Components and Interfaces」の `types.rs`）。
//!
//! # 実行方法（リポジトリルートで実行する）
//!
//! ```text
//! cargo run -p macro-runtime --bin generate-macro-types
//! ```
//!
//! 出力は [`macro_runtime::types::generate`] が返す文字列そのものであり、同一の入力から常に
//! 同一のバイト列になる。生成物は**追跡対象**であり、手で編集しない。差が出たらこのコマンドで
//! 再生成する。ドリフト検査（`crates/macro-runtime/tests/macro_host_dts_drift.rs`）が本コマンドの
//! 出力とコミット済みのファイルをバイト比較する。
//!
//! **出力の前に検査する。** 宣言の無い型が 1 つでもあれば、生成物は書かずに理由を名前つきで
//! 出して非 0 で終わる（型の無い API を公開しない。要件 10.1）。黙って不完全な `.d.ts` を
//! 書くと、補完の効かない API が公開されたことに誰も気づけない。
//!
//! 出力先は `CARGO_MANIFEST_DIR` から導くため、カレントディレクトリに依存しない。bin ターゲット
//! であるため `cargo build --workspace --all-targets` がビルド対象に含め、CI でも腐らない。

use std::path::PathBuf;

fn main() {
    // `crates/macro-runtime/` の 2 つ上がリポジトリルートである。設計時に決まる位置であり、
    // 実行時のカレントディレクトリには依存させない。
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/macro-runtime はリポジトリ直下の crates/ 配下に置かれている")
        .to_path_buf();
    let output = repo_root.join(macro_runtime::types::GENERATED_PATH);

    if let Err(divergence) = macro_runtime::types::check() {
        eprintln!("生成できない（宣言の無い型がある）: {divergence}");
        eprintln!(
            "宣言を足すのは {} である。",
            "crates/macro-runtime/src/types.rs の catalog()"
        );
        std::process::exit(1);
    }
    let generated =
        macro_runtime::types::generate().expect("`check` が通ったため、生成は失敗しない");

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).expect("生成物の出力先ディレクトリを作れない");
    }
    std::fs::write(&output, generated.as_bytes()).expect("生成物を書き出せない");

    println!("生成した: {}", output.display());
    println!(
        "再生成コマンド: {}",
        macro_runtime::types::REGENERATE_COMMAND
    );
}

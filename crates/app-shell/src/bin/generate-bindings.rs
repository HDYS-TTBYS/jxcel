//! TypeScript の生成物（`src/ipc/bindings.ts`）を書き出す、唯一の文書化された入口
//! （tasks.md 2.2。design.md「IpcContract」と「File Structure Plan」）。
//!
//! # 実行方法（リポジトリルートで実行する）
//!
//! ```text
//! cargo run -p app-shell --bin generate-bindings
//! ```
//!
//! 出力は [`app_shell::ipc::render_bindings`] が返す文字列そのものであり、同一の入力から常に
//! 同一のバイト列になる。生成物は**追跡対象**であり、手で編集しない。差が出たらこのコマンドで
//! 再生成する。ドリフト検査（タスク 2.3、`crates/app-shell/tests/bindings_drift.rs`）が
//! 本コマンドの出力とコミット済みのファイルをバイト比較する。
//!
//! 出力先は `CARGO_MANIFEST_DIR` から導くため、カレントディレクトリに依存しない。bin ターゲット
//! であるため `cargo build --workspace --all-targets` がビルド対象に含め、CI でも腐らない。

use std::path::PathBuf;

fn main() {
    // `crates/app-shell/` の 2 つ上がリポジトリルートである。設計時に決まる位置であり、
    // 実行時のカレントディレクトリには依存させない。
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/app-shell はリポジトリ直下の crates/ 配下に置かれている")
        .to_path_buf();
    let output = repo_root.join("src").join("ipc").join("bindings.ts");

    let bindings = app_shell::ipc::render_bindings()
        .expect("境界の型から TypeScript を生成できなければならない");

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).expect("生成物の出力先ディレクトリを作れない");
    }
    std::fs::write(&output, bindings.as_bytes()).expect("生成物を書き出せない");

    println!("生成した: {}", output.display());
    println!("再生成コマンド: {}", app_shell::ipc::REGENERATE_BINDINGS_COMMAND);
}

//! ビルド時に補助プロセスの原本を取り扱うための入口。
//!
//! design.md「SidecarIntegrity」は、同梱する補助プロセスの SHA-256 を `build.rs` が計算し、
//! コンパイル時定数（`EXPECTED_DIGESTS`）として埋め込むことを要求する。原本を書き換えたのに
//! 定数が古いままだと、実行時の整合性検査（要件 5.3、6.5）が常に不一致を返すか、逆に
//! 改変を見逃す。したがって定数の発行はタスク 3.1 が、原本の配置はタスク 1.7 が担う。
//!
//! 本タスク（1.2）ではその土台だけを置く。原本の置き場をビルドの再実行対象として登録し、
//! 原本が現れた時点でビルドがやり直されるようにする。ダイジェストの算出そのものは行わない
//! （存在しないファイルのダイジェストを埋め込んでも意味を持たない）。
//!
//! 原本の置き場（リポジトリ直下の `sidecars/`）はタスク 1.7 が用意する。不在でも本スクリプトは
//! 失敗しない — 配置規約が「原本が不在のときの扱い」を定めるまでは、走査対象の登録に留める。

use std::path::Path;

fn main() {
    // `crates/app-shell/` の 2 つ上がリポジトリルートである（design.md「File Structure Plan」
    // の `sidecars/`）。正規化されていない `..` を含むパスを `rerun-if-changed` に渡さない。
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/app-shell はリポジトリ直下の crates/ 配下に置かれている");

    // `cargo:rerun-if-changed` は存在しないパスに対しても登録でき、後に現れた時点で
    // 再ビルドが走る。原本の配置（タスク 1.7）をビルドに反映させるための登録である。
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("sidecars").display()
    );
}

//! ビルド時に補助プロセスの原本のダイジェストを発行する。
//!
//! design.md「SidecarIntegrity」は、同梱する補助プロセスの SHA-256 を `build.rs` が計算し、
//! コンパイル時定数（`EXPECTED_DIGESTS`）として埋め込むことを要求する。原本を書き換えたのに
//! 定数が古いままだと、実行時の整合性検査（要件 5.3、6.5）が常に不一致を返すか、逆に
//! 改変を見逃す。したがって定数の発行はタスク 3.1 が、原本の配置はタスク 1.7 が担う。
//!
//! ## どの原本を対象にするか
//!
//! Cargo が渡す `TARGET`（このビルドのターゲットトリプル）と同じ接尾辞を持つ原本だけを
//! 対象にする。配置規約（tasks.md 1.7）は `sidecars/<語幹>-<ターゲットトリプル>[.exe]` で、
//! ステムは `SidecarKind::as_str()` が返す値である。`build.rs` はコアクレートに依存できない
//! （依存が循環する）ため、この対応表だけを [`KINDS`] として写す。
//!
//! ## 原本が未配置のとき（不在時の方針。tasks.md 1.7）
//!
//! 対象トリプルの原本が無ければ、その種類のダイジェストを**発行しない**。パニックしない。
//! これはクローン直後の `cargo build --workspace --all-targets` と、ホスト以外の
//! ターゲットに対する `cargo check --target ...`（原本はホストトリプル分しか無い）を
//! 壊さないためである。実行時の `verify` は「期待値が無い」ことを `Unregistered` として
//! 返し、沈黙して通すことはない。
//!
//! ## 出力
//!
//! `$OUT_DIR/sidecar_digests.rs` を必ず生成する（原本が 1 つも無くても空の定数を書く）。
//! `crates/app-shell/src/sidecar/integrity.rs` が `include!` して `EXPECTED_DIGESTS` と
//! `BUILD_TARGET_TRIPLE` を公開する。原本の変化でこのファイルが作り直されるよう、
//! 置き場のディレクトリを `cargo:rerun-if-changed` に登録する。

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// 同梱する補助プロセスの語幹と、`SidecarKind` の変種名の対応。
///
/// 語幹は `crates/app-shell/src/sidecar/mod.rs` の `SidecarKind::as_str()` が唯一の契約
/// （tasks.md 1.2）。`build.rs` はコアクレートに依存できないため、ここに写して発行する。
/// 登録されるのは常に `SidecarKind::<変種>` なので、語幹の対応が崩れれば実行時照合は
/// 別ファイルを指すことになる。変種を追加するスペックは `SidecarKind` と本表を同時に更新する。
const KINDS: &[(&str, &str)] = &[("sidecar-smoke", "Smoke")];

fn main() {
    // `crates/app-shell/` の 2 つ上がリポジトリルートである（design.md「File Structure Plan」
    // の `sidecars/`）。正規化されていない `..` を含むパスを `rerun-if-changed` に渡さない。
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/app-shell はリポジトリ直下の crates/ 配下に置かれている");
    let sidecars_dir = repo_root.join("sidecars");

    // 置き場そのものを再ビルド対象にする。ディレクトリが存在すれば、その中での作成・
    // 置換（新しい原本の配置）が mtime の変化として拾われる。
    println!("cargo:rerun-if-changed={}", sidecars_dir.display());

    // `TARGET` は Cargo がビルドスクリプトに渡すこのビルドのターゲットトリプルである。
    let target = env::var("TARGET").expect("Cargo はビルドスクリプトに TARGET を渡す");
    // Windows のターゲットトリプルだけ `.exe` を持つ（tasks.md 1.7 の命名規約）。
    let exe_suffix = if target.contains("windows") { ".exe" } else { "" };

    let mut entries = String::new();
    for (stem, variant) in KINDS {
        let original: PathBuf = sidecars_dir.join(format!("{stem}-{target}{exe_suffix}"));
        // 存在しなくても登録する。後から現れた時点で再ビルドが走る（不在時の方針）。
        println!("cargo:rerun-if-changed={}", original.display());
        if !original.is_file() {
            continue;
        }
        let digest = sha256(&original);
        entries.push_str(&format!(
            "    (crate::sidecar::SidecarKind::{variant}, [{}]),\n",
            digest
                .iter()
                .map(|byte| format!("0x{byte:02x}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let out_dir = env::var("OUT_DIR").expect("Cargo はビルドスクリプトに OUT_DIR を渡す");
    let out_path = Path::new(&out_dir).join("sidecar_digests.rs");
    let mut file = fs::File::create(&out_path).expect("$OUT_DIR へ生成ファイルを作成できる");
    write!(
        file,
        "// このファイルは crates/app-shell/build.rs が生成する。手で編集しない。\n\
         //\n\
         // 同梱原本 sidecars/<語幹>-<ターゲットトリプル>[.exe] の SHA-256 を、\n\
         // コンパイル時定数として埋め込む（design.md「SidecarIntegrity」）。\n\
         // 原本が未配置のトリプルでは EXPECTED_DIGESTS が空になる（不在時の方針、\n\
         // tasks.md 1.7）。そのとき実行時の verify は Unregistered を返す。\n\
         \n\
         /// このダイジェスト集合が対象とするターゲットトリプル。\n\
         /// 実行時の解決（tasks.md 8.1）と、原本の想定パスの組み立てが共有する。\n\
         pub const BUILD_TARGET_TRIPLE: &str = \"{target}\";\n\
         \n\
         /// build.rs が発行する同梱時ハッシュ。種類ごとに高々 1 件である。\n\
         pub const EXPECTED_DIGESTS: &[(crate::sidecar::SidecarKind, [u8; 32])] = &[\n\
         {entries}];\n"
    )
    .expect("生成ファイルへ書き込める");
}

/// ファイルの SHA-256 を計算する。読み取りに失敗した場合はパニックする — これは
/// ビルド入力が `is_file()` を通過した後に消えた場合に限られ、黙って誤ったダイジェストを
/// 埋め込むよりはビルドを失敗させる方が安全である。
fn sha256(path: &Path) -> [u8; 32] {
    let bytes = fs::read(path).unwrap_or_else(|error| {
        panic!("原本 {} を読み取れない: {error}", path.display());
    });
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&Sha256::digest(&bytes));
    digest
}

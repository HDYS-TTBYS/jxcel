//! 配布物中の補助プロセスが同梱時と同一であることの照合（要件 5.1、5.3、6.5、tasks.md 3.1）。
//!
//! `build.rs` が埋め込む同梱時ダイジェストと、実行時に解決したファイルのダイジェストを
//! 照合する [`verify`] の振る舞いを固定する。とくに完了状態（1 バイトの改変で不一致を返し、
//! 期待値と実測値の**両方**をエラーに含む）を、ステージング済み原本の有無に依存せず
//! 決定的に検証するため、照合表を明示的に渡せる [`verify_with`] を併用する。
//!
//! 原本を書き換えることはない。改変は必ず一時ディレクトリへ複製したファイルに対して行う
//! （`sidecars/` の原本は配布物と同梱時ダイジェストの基準であるため、破壊してはならない）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use app_shell::sidecar::integrity::{
    verify, verify_with, IntegrityError, BUILD_TARGET_TRIPLE, EXPECTED_DIGESTS,
};
use app_shell::sidecar::SidecarKind;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// 補助関数
// ---------------------------------------------------------------------------

/// リポジトリルートを `CARGO_MANIFEST_DIR` から解決する。カレントディレクトリに依存しない。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/app-shell はリポジトリ直下の crates/ 配下にある")
        .to_path_buf()
}

/// このビルドで `build.rs` が期待ダイジェストを発行した対象トリプル向けの、原本の想定パス。
///
/// 配置規約（tasks.md 1.7）: `sidecars/sidecar-smoke-<ターゲットトリプル>[.exe]`。
/// ステムは `SidecarKind::Smoke::as_str()`、トリプルは `build.rs` が埋め込んだ値を共有する。
fn staged_original_path() -> PathBuf {
    let exe = if BUILD_TARGET_TRIPLE.contains("windows") {
        ".exe"
    } else {
        ""
    };
    repo_root()
        .join("sidecars")
        .join(format!("{}-{BUILD_TARGET_TRIPLE}{exe}", SidecarKind::Smoke.as_str()))
}

/// バイト列の SHA-256（独立した実装で期待値を作る）。
fn digest32(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Sha256::digest(bytes));
    out
}

/// ダイジェストを 16 進小文字へ。`verify` の報告文字列と突き合わせるためだけに使う。
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// テスト中だけ使う一時ディレクトリ。`Drop` でベストエフォート削除する。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が UNIX エポックより前")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "jxcel-sidecar-integrity-{tag}-{}-{nanos}-{seq}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("一時ディレクトリを作成できる");
        TempDir { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, bytes).expect("一時ファイルへ書き込める");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// 完了状態: 1 バイトの改変で不一致を返し、期待値と実測値の両方を含む
// ---------------------------------------------------------------------------

/// 1 バイトだけ改変したファイルに対して不一致を返し、そのエラー本文に**期待値と実測値の
/// 両方**が現れることを固定する（tasks.md 3.1 の完了状態）。
///
/// 照合表はテストが独立に計算した値を渡す。ステージングの有無に依存せず、比較器そのものを
/// 検証する（原本は書き換えない — 改変は複製に対して行う）。
#[test]
fn one_byte_modification_is_reported_with_both_digests() {
    let dir = TempDir::new("mismatch");
    let original = b"sidecar-smoke: integrity-check fixture (original)".to_vec();
    let expected = digest32(&original);
    let _original_path = dir.write("original", &original);

    let mut modified = original.clone();
    modified[0] ^= 0x01; // ちょうど 1 バイトだけを変える
    let modified_path = dir.write("modified", &modified);

    let expected_hex = hex(&expected);
    let actual_hex = hex(&digest32(&modified));
    assert_ne!(expected_hex, actual_hex, "1 バイト改変でダイジェストは変わる");

    let error = verify_with(&[(SidecarKind::Smoke, expected)], SidecarKind::Smoke, &modified_path)
        .expect_err("1 バイト改変は不一致になるはずである");

    // 報告本文に期待値と実測値の両方が含まれること（起動中止の報告として読める）。
    let text = error.to_string();
    assert!(
        text.contains(&expected_hex),
        "不一致の報告に期待値 {expected_hex} が含まれない: {text}"
    );
    assert!(
        text.contains(&actual_hex),
        "不一致の報告に実測値 {actual_hex} が含まれない: {text}"
    );

    // 型でも両方のダイジェストを区別して取り出せること。
    match error {
        IntegrityError::Mismatch {
            kind,
            expected: got_expected,
            actual: got_actual,
        } => {
            assert_eq!(kind, SidecarKind::Smoke);
            assert_eq!(got_expected, expected_hex);
            assert_eq!(got_actual, actual_hex);
        }
        other => panic!("Mismatch を期待したが {other:?} だった"),
    }
}

// ---------------------------------------------------------------------------
// 一致: 改変していない複製は Ok
// ---------------------------------------------------------------------------

#[test]
fn unmodified_copy_verifies_ok() {
    let dir = TempDir::new("match");
    let bytes = b"sidecar-smoke: integrity-check fixture".to_vec();
    let expected = digest32(&bytes);
    let path = dir.write("copy", &bytes);

    verify_with(&[(SidecarKind::Smoke, expected)], SidecarKind::Smoke, &path)
        .expect("改変していない複製は一致するはずである");
}

// ---------------------------------------------------------------------------
// 読み取り不能: Mismatch とも Unregistered とも区別できる第 2 の結果
// ---------------------------------------------------------------------------

/// 存在しないパスは panic せず、区別できる `Unreadable` になることを固定する。
/// ダイジェストを登録済みの状態で読み取りだけが失敗する状況を作る（登録が無ければ
/// `Unregistered` が先に返るため、ここでは明示的な照合表を渡す）。
#[test]
fn missing_file_is_reported_as_unreadable_not_a_panic() {
    let dir = TempDir::new("unreadable");
    let missing = dir.path().join("does-not-exist");

    let error = verify_with(
        &[(SidecarKind::Smoke, [0u8; 32])],
        SidecarKind::Smoke,
        &missing,
    )
    .expect_err("存在しないファイルは読み取れないはずである");

    // 報告（Display）が理由と対象パスを含むこと。起動中止の報告はこの文字列を運ぶ。
    let text = error.to_string();
    assert!(text.contains("読み取れない"), "報告に理由が含まれない: {text}");
    assert!(
        text.contains(&missing.display().to_string()),
        "報告に対象パスが含まれない: {text}"
    );

    assert!(
        matches!(error, IntegrityError::Unreadable { .. }),
        "Unreadable を期待したが {error:?} だった"
    );
}

// ---------------------------------------------------------------------------
// 未登録: 期待値が無いことを Mismatch / Unreadable と区別して返す（沈黙して通さない）
// ---------------------------------------------------------------------------

/// 種類に対する期待ダイジェストが 1 つも登録されていないとき、`verify_with` は
/// `Unregistered` を返す。これはステージングの有無に依存せず常に検証できる。
#[test]
fn kind_without_a_registered_digest_is_distinguishable() {
    let dir = TempDir::new("unregistered");
    let path = dir.write("sidecar-smoke", b"readable file, but no expected digest");

    let error = verify_with(&[], SidecarKind::Smoke, &path)
        .expect_err("期待値が無いのに成功してはならない");

    // 報告が「期待値が登録されていない」ことと種類を含むこと。
    let text = error.to_string();
    assert!(
        text.contains("登録されていない"),
        "報告に理由が含まれない: {text}"
    );
    assert!(text.contains("Smoke"), "報告に種類が含まれない: {text}");

    assert!(
        matches!(
            error,
            IntegrityError::Unregistered {
                kind: SidecarKind::Smoke,
                ..
            }
        ),
        "Unregistered を期待したが {error:?} だった"
    );
}

/// クローン直後（`sidecars/` に `.gitkeep` しか無い）はビルド時定数が空であり、
/// `verify` も沈黙して通らないことを固定する。原本が配置済みの環境では、この分岐に
/// 代えて [`staged_original_verifies_and_is_covered_by_the_registered_digest`] が実経路を走る。
#[test]
fn fresh_clone_without_a_staged_original_reports_unregistered() {
    if !EXPECTED_DIGESTS.is_empty() {
        eprintln!(
            "原本が配置済みのためスキップします（登録数: {}）。実経路は \
             staged_original_verifies_and_is_covered_by_the_registered_digest が検証します。",
            EXPECTED_DIGESTS.len()
        );
        return;
    }

    let dir = TempDir::new("fresh-clone");
    let path = dir.write("sidecar-smoke", b"readable file, but nothing was registered");
    let error = verify(SidecarKind::Smoke, &path).expect_err("期待値が無いのに成功してはならない");

    assert!(
        matches!(
            error,
            IntegrityError::Unregistered {
                kind: SidecarKind::Smoke,
                ..
            }
        ),
        "Unregistered を期待したが {error:?} だった"
    );
}

// ---------------------------------------------------------------------------
// 実経路: 原本が配置されているときだけ走る（配置済みダイジェストとの一致と改変検出）
// ---------------------------------------------------------------------------

/// `build.rs` が発行したダイジェストが、ステージング済み原本の SHA-256 と一致し、
/// その原本に対する `verify` が `Ok` を返し、**複製を 1 バイト変えると不一致になる**
/// ことを固定する。原本が未配置のクローンでは理由を出してスキップする。
///
/// 期待値の正しさを `sha256sum` と同値の独立計算（`sha2` をテスト側で直接使う）で
/// 確かめるため、ビルド時ダイジェストが古い場合もここで落ちる。
#[test]
fn staged_original_verifies_and_is_covered_by_the_registered_digest() {
    let expected = EXPECTED_DIGESTS
        .iter()
        .find(|(kind, _)| *kind == SidecarKind::Smoke)
        .map(|(_, digest)| *digest);

    let Some(expected) = expected else {
        eprintln!(
            "原本が未配置のためスキップします（期待: {}）。\
             `bash scripts/stage-sidecars.sh` で配置すると実経路が走ります。",
            staged_original_path().display()
        );
        return;
    };

    let staged = staged_original_path();
    assert!(
        staged.is_file(),
        "ダイジェストが登録済みなのに原本が存在しない: {}",
        staged.display()
    );

    // 登録済みダイジェスト == 原本の SHA-256（`sha256sum` と同値）。
    let original = fs::read(&staged).expect("ステージング済み原本を読み取れる");
    assert_eq!(
        hex(&expected),
        hex(&digest32(&original)),
        "登録済みダイジェストが原本の SHA-256 と一致しない（build.rs の再実行が必要）"
    );

    // 原本そのもの（未改変）は Ok。
    verify(SidecarKind::Smoke, &staged).expect("原本は登録済みダイジェストと一致するはずである");

    // 複製を 1 バイトだけ変える。原本は書き換えない。
    let dir = TempDir::new("staged-mismatch");
    let mut modified = original.clone();
    modified[0] ^= 0x01;
    let modified_path = dir.write("sidecar-smoke", &modified);

    let error = verify(SidecarKind::Smoke, &modified_path)
        .expect_err("1 バイト改変は実行時の照合でも検出されるはずである");
    let expected_hex = hex(&expected);
    let actual_hex = hex(&digest32(&modified));
    match error {
        IntegrityError::Mismatch {
            expected: got_expected,
            actual: got_actual,
            ..
        } => {
            assert_eq!(got_expected, expected_hex);
            assert_eq!(got_actual, actual_hex);
        }
        other => panic!("Mismatch を期待したが {other:?} だった"),
    }
}

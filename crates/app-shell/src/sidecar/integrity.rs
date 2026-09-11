//! 配布物中の補助プロセスが同梱時と同一であることの照合（要件 5.1、5.3、6.5）。
//!
//! `build.rs` が発行した同梱時ハッシュ（[`EXPECTED_DIGESTS`]、[`SidecarKind`](super::SidecarKind) を
//! キーとする）と、実行時に解決したファイルのダイジェストを照合する。照合は起動の**前**に行う。
//! 不一致は修復に接続しない — AppImage は読み取り専用の squashfs でありその場で置き換えられない
//! ため、検出は起動の中止と報告に接続する（design.md「SidecarIntegrity」、research.md 決定 4）。
//! したがって本モジュールは書き換え・複製・再配置の経路を一切持たない。
//!
//! 結果は 3 つの失敗を区別する:
//! - [`IntegrityError::Mismatch`] — 期待値があるのに内容が違う（改変・破損）。期待値と実測値を
//!   どちらも報告に含む
//! - [`IntegrityError::Unreadable`] — 期待値はあるが実行時に読み取れない（不在・権限など）
//! - [`IntegrityError::Unregistered`] — その種類の期待値がそもそも埋め込まれていない（原本が
//!   未配置のビルド）。design.md の列挙は 2 変種だが、この第 3 の結果は tasks.md 1.7 の
//!   申し送りが要求するものである: 未配置と「内容不一致」「読み取り不能」を混同すると、
//!   期待値の無いビルドが沈黙して通るか、原因を誤って報告する。したがって追加している。

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::SidecarKind;

// build.rs が `$OUT_DIR` へ生成する。`BUILD_TARGET_TRIPLE` と `EXPECTED_DIGESTS` を含む。
include!(concat!(env!("OUT_DIR"), "/sidecar_digests.rs"));

/// 同梱物の照合に失敗した理由。3 つの結果は互いに区別できる。
#[derive(Debug, thiserror::Error)]
pub enum IntegrityError {
    /// 同梱時のダイジェストと実測値が異なる。期待値と実測値をどちらも載せ、報告が
    /// 何と何が食い違ったのかを特定できるようにする（要件 5.3）。
    #[error(
        "補助プロセスの内容が同梱時と一致しない（種類: {kind:?}）: 期待値 {expected} / 実測値 {actual}"
    )]
    Mismatch {
        kind: SidecarKind,
        expected: String,
        actual: String,
    },

    /// 解決したパスを読み取れない（不在、権限、ディレクトリなど）。照合以前の失敗であり、
    /// 内容不一致とは区別する。
    #[error("補助プロセスを読み取れない: {path}")]
    Unreadable { path: PathBuf },

    /// その種類の期待ダイジェストがビルド時に登録されていない（原本が未配置だった）。
    /// 照合そのものが成立しないことを表す。**沈黙して通してはならない**（要件 5.3）ため、
    /// 成功ではなく失敗として返す。
    #[error("補助プロセスの期待ダイジェストが登録されていない（種類: {kind:?}、対象: {path}）")]
    Unregistered { kind: SidecarKind, path: PathBuf },
}

/// 同梱時ダイジェストと、実行時に解決したファイルの内容を照合する（要件 5.1、5.3）。
///
/// 起動が補助プロセスのパスを解決した直後に呼ぶ。`Ok(())` は「同梱時と同一」を意味し、
/// それ以外は起動を中止して報告する材料になる。本関数はファイルを変更しない。
pub fn verify(kind: SidecarKind, path: &Path) -> Result<(), IntegrityError> {
    verify_with(EXPECTED_DIGESTS, kind, path)
}

/// 照合表を明示的に受け取る `verify` の実体。
///
/// 公開しているのは、テストが「原本が未配置のビルド」の結果（空の照合表 → `Unregistered`）を
/// ステージングの有無に依存せず決定的に検証できるようにするためである。`verify` は
/// ビルド時定数 [`EXPECTED_DIGESTS`] を渡すだけであり、利用側は通常こちらを呼ばない。
pub fn verify_with(
    registered: &[(SidecarKind, [u8; 32])],
    kind: SidecarKind,
    path: &Path,
) -> Result<(), IntegrityError> {
    // 期待値の有無を先に確かめる。期待値が無ければ照合は成立せず、実行時の読み取り結果に
    // かかわらず `Unregistered` を返す（未配置のビルドを「読み取り不能」と誤報告しない）。
    let Some((_, expected)) = registered.iter().find(|(candidate, _)| *candidate == kind) else {
        return Err(IntegrityError::Unregistered {
            kind,
            path: path.to_path_buf(),
        });
    };

    let actual = digest_of(path).map_err(|_| IntegrityError::Unreadable {
        path: path.to_path_buf(),
    })?;

    if &actual == expected {
        Ok(())
    } else {
        Err(IntegrityError::Mismatch {
            kind,
            expected: hex(expected),
            actual: hex(&actual),
        })
    }
}

/// ファイル全体の SHA-256 を計算する。
fn digest_of(path: &Path) -> std::io::Result<[u8; 32]> {
    let bytes = std::fs::read(path)?;
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&Sha256::digest(&bytes));
    Ok(digest)
}

/// ダイジェストを 16 進小文字の文字列へ。報告（`Mismatch`）に載せる表現である。
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

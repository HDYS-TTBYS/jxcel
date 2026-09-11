//! 補助プロセスの監督（要件 5.4〜5.9、とくに 5.5・5.6）。
//!
//! 本モジュールは、配布物に同梱した補助プロセスを種類ごとに高々 1 つ起動し、ウィンドウ間で共有し、
//! アプリケーションの終了とクラッシュの双方で確実に終了させる機構を所有する。Tauri の
//! `tauri-plugin-shell` には委ねない — 同プラグインの終了時クリーンアップは JS→IPC 経路で
//! 起動した子だけを対象とし、Rust から起動した子は対象外である（research.md 決定 5）。監督の
//! 実体は [`supervisor`]、整合性検査は [`integrity`]、終了保証は環境別の `group_unix` /
//! `job_windows`、残留の掃除は [`orphan_sweep`] が持つ。
//!
//! 本モジュールは、複数のモジュールが参照する共有の列挙 [`SidecarKind`] を定義する。これは
//! `SidecarSpec.kind`、`SidecarIntegrity::EXPECTED_DIGESTS` のキー、`SidecarSupervisor::ensure` /
//! `get` の引数、`orphan_sweep` の実行ファイル名照合が共有する唯一の識別子である。
//!
//! 本タスク（1.2）では骨組みと共有の列挙のみを置く。実体は tasks.md 3.1（整合性検査）・
//! 3.2（起動と共有）・3.3（終了保証）・3.5（残留の掃除）が追加する。

#[cfg(unix)]
pub mod group_unix;
pub mod integrity;
#[cfg(windows)]
pub mod job_windows;
pub mod orphan_sweep;
pub mod supervisor;

/// 補助プロセスの種類。どの実行ファイルを、どの名前で同梱し、どのプロセスを監督するかを
/// 一意に決める識別子である。
///
/// 変種を検証用の 1 つに限る理由: 本スペックの実装時点で同梱すべき実用的な補助プロセスは
/// 存在しない。本機能が所有するのは機構検証専用の `crates/sidecar-smoke` だけで、実物
/// （言語サーバ）の選定と登録は下流の `macro-editor-lsp` が行う。したがって「将来 N 種類の
/// 補助プロセスが載る」ことを見越した抽象をここに置かない（research.md 決定 9、design.md
/// 「Out of Boundary」）。2 つ目の実体が必要になった時点で、この列挙に変種を足す。
///
/// 導出している性質は用途が決めている: `Copy` は `ensure` / `get` の引数とイベントに
/// 複製して渡すため、`Eq` + `Hash` と `Ord` は起動中のプロセスを種類で引く写像のキーに
/// するため、`Debug` はエラーと記録に載せるためである。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SidecarKind {
    /// 機構検証専用の最小の補助プロセス（`crates/sidecar-smoke`）。
    Smoke,
}

impl SidecarKind {
    /// 定義済みのすべての種類。網羅の検査と、実行ファイル名からの照合に使う。
    pub const ALL: &'static [SidecarKind] = &[SidecarKind::Smoke];

    /// 同梱する実行ファイルの語幹（バンドル時のターゲットトリプル接尾辞を除いた名前）と、
    /// 残留プロセスの実行ファイル名の照合に使う安定した文字列表現。
    ///
    /// 値は `crates/sidecar-smoke` のパッケージ名 = バイナリ名と一致させる。同梱するファイルの
    /// 配置名、Job Object / プロセスグループが対象とするプロセス、`build.rs` が発行する
    /// ダイジェスト定数のキーが、すべてこの 1 つの表現を共有する。
    pub const fn as_str(self) -> &'static str {
        match self {
            SidecarKind::Smoke => "sidecar-smoke",
        }
    }

    /// 実行ファイルの語幹から種類へ戻す。未知の名前は `None` を返す。残留プロセスの照合
    /// （要件 5.6、タスク 3.5）がこれを使う。拡張子やターゲットトリプル接尾辞を含む名前は
    /// 語幹ではないため解釈しない — 照合が緩むと無関係なプロセスを誤って対象にしうる。
    pub fn parse(name: &str) -> Option<SidecarKind> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::SidecarKind;
    use std::collections::HashSet;

    /// 種類の識別子は往復し、互いに重複しない。この 1 つの文字列が同梱する実行ファイルの名前・
    /// 残留プロセスの照合・ビルド時のダイジェスト発行で共有されるため、ここが崩れると別の
    /// 実行ファイルを指すことになる。
    #[test]
    fn kind_identifiers_are_distinct_and_round_trip() {
        for &kind in SidecarKind::ALL {
            let name = kind.as_str();
            assert_eq!(SidecarKind::parse(name), Some(kind), "{name} が往復しない");
        }

        let names: HashSet<&str> = SidecarKind::ALL.iter().map(|kind| kind.as_str()).collect();
        assert_eq!(
            names.len(),
            SidecarKind::ALL.len(),
            "識別子が重複している"
        );
    }

    /// 識別子は同梱する実行ファイルの語幹と一致する。`crates/sidecar-smoke` が生成する実行ファイルの
    /// 名前（ターゲットトリプル接尾辞を除いた語幹）であり、バンドル配置と残留プロセスの照合が
    /// この値に依存する。未知の名前を種類として解釈しないことも合わせて固定する — 照合が緩むと
    /// 無関係なプロセスを補助プロセスと誤認しうる。
    #[test]
    fn identifier_matches_the_bundled_executable_stem() {
        assert_eq!(SidecarKind::Smoke.as_str(), "sidecar-smoke");
        assert_eq!(SidecarKind::parse("sidecar-smoke.exe"), None);
        assert_eq!(SidecarKind::parse(""), None);
    }
}

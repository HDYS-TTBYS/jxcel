//! 形式バージョンと読み込み時ゲート（タスク 6.1。要件 6.1, 6.5。design「MigrationChain」）。
//!
//! 本モジュールは形式バージョンの型 [`FormatVersion`] と、その**単一定義**である現行版
//! [`CURRENT_FORMAT_VERSION`]、そして記録値を判定するゲート [`MigrationChain`] を持つ。
//! 本クレートで「今の形式は何か」「その記録値は読めるか」を決める唯一の場所である。
//!
//! # 記録と判定（要件 6.1 / 6.5）
//!
//! 保存経路（[`crate::parts::to_parts`]）は [`CURRENT_FORMAT_VERSION`] を `manifest.json` へ
//! 記録する（要件 6.1。wire 形 `{"major":..,"minor":..}` の所有者は
//! [`crate::parts::manifest`] であり、[`FormatVersion`] 自身に `Serialize` を後付けしない）。
//! 読み込み経路は索引の記録値（[`crate::parts::DocumentParts::format_version`]）を
//! [`MigrationChain::gate`] に掛け、**ダイジェスト照合より前**に版の可否を決める
//! （design「読み込みフロー」の「形式バージョン」→「移行」→「ダイジェスト照合」）。
//!
//! 判定は major だけで決まる（design「MigrationChain / Responsibilities」）:
//!
//! | 記録された major | 判定 | 読み込み経路の応答 |
//! |------------------|------|--------------------|
//! | 現行より新しい | [`VersionVerdict::Unsupported`] | 中止（要件 6.5。**要求バージョンを含める**） |
//! | 現行と同じ | [`VersionVerdict::Openable`] | そのまま読む |
//! | 現行より古い | [`VersionVerdict::NeedsMigration`] | 移行チェーンの適用が必要（タスク 6.2） |
//!
//! **minor の増加は省略可能フィールドの追加のみ**に限る（design 同節）。したがって同一 major の
//! より新しい minor は未対応のキーを持つだけであり、本クレートは未知フィールドを破棄せず
//! 保持して書き戻すため（要件 6.2 / 6.3）、読めない理由が無い。**判定に minor を混ぜない**
//! ことがこの判定の意味である。
//!
//! # 移行チェーンの適用はタスク 6.2
//!
//! [`MigrationChain`] は design のコンポーネント名であり、段階的適用（v1→v2→…）は
//! タスク 6.2 がここへ足す。**タスク 6.1 は判定まで**である: 適用先が無い古い major は
//! [`VersionVerdict::NeedsMigration`] と判定し、読み込み経路（[`MigrationChain::admit`]）が
//! **同じ中止**（[`crate::error::DocumentError::UnsupportedVersion`]）として報告する。
//! 判定型が [`VersionVerdict::NeedsMigration`] と [`VersionVerdict::Unsupported`] を
//! 区別しているのは、6.2 が前者だけを実チェーン適用へ置き換えられるようにするためである。
//!
//! # [`FormatVersion`] の所有と [`crate::error`] の相互参照
//!
//! design の File Structure は [`FormatVersion`] を本モジュールに置く（タスク 6.1 で
//! `error.rs` から移管した）。一方この型は
//! [`crate::error::DocumentError::UnsupportedVersion`] の文脈型（`found` / `supported`）
//! でもあるため、`error.rs` は本モジュールを `use` し、本モジュールはゲートの失敗として
//! [`crate::error::DocumentError`] を `use` する。**同一クレート内の相互参照は合法であり、
//! コンポーネント間の依存方向を増やさない**: `error.rs` は design の依存グラフに現れない
//! 共有の葉であり、どちらの参照もクレート内の型参照にすぎない（タスク 6.1 の親の裁定）。
//!
//! # 依存方向
//!
//! 本モジュールは最下層に近い判定だけを持ち、[`crate::parts`] にも [`crate::container`] にも
//! 依存しない（呼ばれる側である）。`std::fs` も `zip` も参照しない。判定は記録値に対する
//! **純粋な関数**であり、ファイル I/O はタスク 7.1 / 7.2 の責務である。

use core::fmt;

use crate::error::DocumentError;

/// 本実装が書き出す現行の形式バージョン（design「Container Entry Layout」の 1.0）。
///
/// **現行バージョンの単一定義**である: 保存経路（[`crate::parts::to_parts`]）はこの値を
/// `manifest.json` へ記録し、ゲート（[`MigrationChain::gate`]）はこの値を基準に判定する。
/// クレート内に第二の定義を置かないこと（食い違うと保存した版を自分で拒否する）。
pub const CURRENT_FORMAT_VERSION: FormatVersion = FormatVersion::new(1, 0);

/// ドキュメント形式のバージョン（design「MigrationChain」）。
///
/// 比較は major 優先の辞書順（[`Ord`]）であり、「現行より新しい形式」の判定
/// （[`MigrationChain::gate`]。要件 6.5）と [`DocumentError::UnsupportedVersion`] の
/// 文脈（`found` / `supported`）に使える。テキスト形は [`fmt::Display`] の `major.minor`
/// だけであり、コンテナ内の型マーカー `jxcel` の内容がこれに一致する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormatVersion {
    /// major バージョン（非後方互換の上がり方）。
    pub major: u32,
    /// minor バージョン（後方互換のある追加）。
    pub minor: u32,
}

impl FormatVersion {
    /// 組み立てる。
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for FormatVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// 記録された形式バージョンに対するゲートの判定（[`MigrationChain::gate`] の戻り値）。
///
/// 「そのまま読める」と「新しすぎて読めない」と「古くて移行が必要」を**区別する**ことが
/// この型の存在理由である（タスク 6.2 は [`Self::NeedsMigration`] だけを実チェーン適用へ
/// 置き換える。モジュール docs「移行チェーンの適用はタスク 6.2」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionVerdict {
    /// 現行と同じ major である。**minor が現行より新しくても**そのまま読める。
    Openable,
    /// 現行より古い major である。移行チェーンの適用が必要（タスク 6.2）。
    ///
    /// `from` はファイルに記録されていたバージョン（移行の出発点）である。
    NeedsMigration {
        /// ファイルに記録されていたバージョン。
        from: FormatVersion,
    },
    /// 現行より新しい major である。読み込めない（要件 6.5）。
    Unsupported {
        /// ファイルに記録されていたバージョン（要求バージョン）。
        found: FormatVersion,
        /// 本実装が読み込めるバージョン（[`CURRENT_FORMAT_VERSION`]）。
        supported: FormatVersion,
    },
}

/// 形式バージョンのゲートと、段階的移行チェーンの置き場（design「MigrationChain」）。
///
/// タスク 6.1 では**ゲートだけ**を持つ。タスク 6.2 が古い形式を 1 段ずつ現行へ変換する
/// 適用機構（`apply`）をここへ足す。無状態であり、構築も保持もしない。
pub struct MigrationChain;

impl MigrationChain {
    /// 記録された形式バージョンへ判定を下す（要件 6.5。design「MigrationChain」）。
    ///
    /// 判定は major の比較だけで決まる。minor は見ない
    /// （`1.0` と `1.99` はどちらも [`VersionVerdict::Openable`]。モジュール docs 参照）。
    /// この関数はエラーを返さず、応答（中止 / 移行 / そのまま）は呼び出し側が決める。
    pub const fn gate(recorded: FormatVersion) -> VersionVerdict {
        if recorded.major > CURRENT_FORMAT_VERSION.major {
            VersionVerdict::Unsupported { found: recorded, supported: CURRENT_FORMAT_VERSION }
        } else if recorded.major == CURRENT_FORMAT_VERSION.major {
            VersionVerdict::Openable
        } else {
            VersionVerdict::NeedsMigration { from: recorded }
        }
    }

    /// 読み込み経路向けのゲート: そのまま読める版なら `Ok(())`、それ以外は中止のエラー。
    ///
    /// [`Self::gate`] の判定を、design のエラー表にある単一の応答
    /// [`DocumentError::UnsupportedVersion`] へ写す。**新しすぎる版**は要求バージョン
    /// （`found`）と対応バージョン（`supported`）をそのまま報告する。
    ///
    /// **古い major も現在は同じ中止で報告する（タスク 6.2 が置き換える分岐）**:
    /// 移行チェーンの適用機構は 6.2 の実装であり、本タスクには移行先が無い。移行先が無い版は
    /// 開けないため、`NeedsMigration` も「読めない版」として同じ変種で中止する。
    /// 6.2 はこの分岐を「`parts` を実チェーンへ掛けてから読む」処理に置き換える。
    pub fn admit(recorded: FormatVersion) -> Result<(), DocumentError> {
        match Self::gate(recorded) {
            VersionVerdict::Openable => Ok(()),
            VersionVerdict::NeedsMigration { from } => Err(DocumentError::UnsupportedVersion {
                found: from,
                supported: CURRENT_FORMAT_VERSION,
            }),
            VersionVerdict::Unsupported { found, supported } => {
                Err(DocumentError::UnsupportedVersion { found, supported })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 現行版は 1.0 である（golden fixture と既存テストの期待値の源）。
    #[test]
    fn the_current_version_is_one_zero() {
        assert_eq!(FormatVersion::new(1, 0), CURRENT_FORMAT_VERSION, "現行バージョンが 1.0 でない");
    }

    /// 同一 major は minor の新旧を問わず受理する（受理条件は major の等値だけである。
    /// minor の増加は省略可能フィールドの追加のみ、design「MigrationChain」）。
    #[test]
    fn the_same_major_is_openable_regardless_of_minor() {
        for minor in [0u32, 1, 99, u32::MAX] {
            let recorded = FormatVersion::new(CURRENT_FORMAT_VERSION.major, minor);
            assert_eq!(
                VersionVerdict::Openable,
                MigrationChain::gate(recorded),
                "同一 major の {recorded} が受理されない"
            );
        }
    }

    /// 現行より新しい major は要求バージョンつきで拒否される（要件 6.5）。
    #[test]
    fn a_newer_major_is_unsupported() {
        for (major, minor) in [(2u32, 0u32), (2, 7), (u32::MAX, u32::MAX)] {
            let recorded = FormatVersion::new(major, minor);
            assert_eq!(
                VersionVerdict::Unsupported {
                    found: recorded,
                    supported: CURRENT_FORMAT_VERSION,
                },
                MigrationChain::gate(recorded),
                "新しい major {recorded} の判定が違う"
            );
        }
    }

    /// 現行より古い major は「移行が必要」と判定される（適用はタスク 6.2）。
    #[test]
    fn an_older_major_needs_migration() {
        for (major, minor) in [(0u32, 0u32), (0, 9)] {
            let recorded = FormatVersion::new(major, minor);
            assert_eq!(
                VersionVerdict::NeedsMigration { from: recorded },
                MigrationChain::gate(recorded),
                "古い major {recorded} の判定が違う"
            );
        }
    }

    /// 読み込み経路向けのゲート: 開ける版だけ `Ok`、それ以外は要求バージョンつきの中止。
    #[test]
    fn admit_opens_only_the_current_major() {
        assert!(MigrationChain::admit(FormatVersion::new(1, 99)).is_ok(), "同一 major が閉じられた");
        assert!(MigrationChain::admit(CURRENT_FORMAT_VERSION).is_ok(), "現行版が閉じられた");

        for recorded in [FormatVersion::new(2, 7), FormatVersion::new(0, 9)] {
            match MigrationChain::admit(recorded) {
                Err(DocumentError::UnsupportedVersion { found, supported }) => {
                    assert_eq!(recorded, found, "要求バージョンが報告されていない");
                    assert_eq!(CURRENT_FORMAT_VERSION, supported, "対応バージョンが報告されていない");
                }
                other => panic!("{recorded} が中止されない: {other:?}"),
            }
        }
    }

    /// `Display` は `major.minor` の正準テキスト形である（型マーカー `jxcel` と golden が依存）。
    #[test]
    fn display_is_the_canonical_major_minor_text() {
        assert_eq!("1.0", FormatVersion::new(1, 0).to_string());
        assert_eq!("2.7", FormatVersion::new(2, 7).to_string());
        assert_eq!("12.345", FormatVersion::new(12, 345).to_string());
    }

    /// `Ord` は major 優先の辞書順である（`gate` の比較の意味を単独で固定する。
    /// minor だけを比較する実装はここで落ちる）。
    #[test]
    fn ordering_is_major_first() {
        assert!(
            FormatVersion::new(1, 99) < FormatVersion::new(2, 0),
            "major が minor より優先されない"
        );
        assert!(FormatVersion::new(0, u32::MAX) < FormatVersion::new(1, 0));
        assert!(
            FormatVersion::new(1, 0) < FormatVersion::new(1, 1),
            "同一 major の minor 比較が壊れた"
        );
    }
}

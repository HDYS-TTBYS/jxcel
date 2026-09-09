//! クレート共通エラー型（タスク 1.4。要件 4.3, 4.5, 5.3, 6.5 ほか、design
//! 「Error Handling / Error Categories and Responses」表の全 10 変種）。
//!
//! # 判別可能性と提示
//!
//! すべてのエラーは [`DocumentError`] という単一の判別可能な列挙型として返す
//! （design「エラー戦略」: 文言と提示方法は呼び出し元が決める。本スペックは UI を
//! 持たない）。変種は診断に必要な**文脈**（エントリ名、識別子、参照元と参照先、
//! バージョン）だけを保持し、提示用の文言を持たない: [`std::fmt::Display`] は
//! ログ・デバッグ用の最小限の技術的診断であり、呼び出し元は `match` で変種と
//! 文脈を取り出して任意の提示（ロケール化メッセージを含む）を組み立てられる。
//! `thiserror` は design 採択ライブラリ（design「型付きエラー: 判別可能な列挙型と
//! して公開」）であり、`Error` / `Display` の機械的な実装だけをここに発生させる。
//!
//! 読み込みエラーは常に読み込み全体の中止であり、部分的な結果を返さない
//! （design エラー戦略・自動修復の禁止）。この型自体は中止/報告の区別を持たない:
//! どの変種で中止かを決定するのは呼び出し側の経路である（design の応答列が対応する）。
//!
//! # 文脈フィールドの型について
//!
//! - エントリ名フィールド（`entry` / `name`）は当面 [`String`]。エントリ名新型
//!   `EntryName` はタスク 1.6 の導入であり、その際に本ファイルの対応フィールドも
//!   新型へ migrate する（後続タスクで型を締める前提の暫定 [`String`]）。
//! - 識別子フィールドは 1.3 の新型ではなくテキスト形 [`String`] で保持する。
//!   エラー文脈は読み込み途中で失敗箇所を**文字どおり**記録することが目的であり、
//!   妥当性済みの新型を要求すると不正入力を保持できないため。
//! - [`DocumentError::DuplicateId`] の `occurrences` は design 表の変種签名
//!   `{ kind, id }` に対する要件 4.3 由来の拡張である（「識別子と出現箇所を
//!   含める」）。出現箇所は各重複の発生位置を示す説明文字列（パート名 + 位置など）の列。
//! - [`DocumentError::Io`] は多フィールド変種（`retried` を伴う）のため
//!   `From<std::io::Error>` を定義しない。呼び出し元がリトライ状態を明示的に
//!   渡して構築する。フィールド名 `source` により `Error::source()` は結線される。
//!
//! # [`FormatVersion`] の暫定所有
//!
//! design の File Structure では `FormatVersion` は `migration` 側の型だが、
//! 移行の実装（タスク 6.1）より先にエラー表の `UnsupportedVersion { found,
//! supported }` が必要になる。重複定義を避け単一定義とするため暫く本ファイルの
//! 唯一の定義として置き、タスク 6.1 で `migration` へ移管する（移管時は本ファイルが
//! `use` を追従させ、本節の暫定注記を消す）。

use core::fmt;
use thiserror::Error;

/// ドキュメント形式のバージョン（**暫定定義** — 所有権はモジュール docs の
/// 「[`FormatVersion`] の暫定所有」参照。タスク 6.1 で `migration` へ移管予定）。
///
/// `UnsupportedVersion { found, supported }` の文脈型。比較は辞書順
/// （major 優先）であり、「現行より新しい形式」判定（`found > supported` の
/// major 比較、要件 6.5）に使える。
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

/// [`DocumentError::DuplicateId`] が指す識別子の種別。
///
/// design のエラー表は `DuplicateId { kind, id }` の `kind` に型を与えていない
/// ため、ID 体系（タスク 1.3: シート / 行 / 型定義 / 添付の 4 体系）から
/// 導出した最小の列挙型としてここに定義する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdKind {
    /// [`crate::ids::SheetId`]。
    Sheet,
    /// [`crate::ids::RowId`]。
    Row,
    /// [`crate::ids::TypeDefId`]。
    TypeDef,
    /// [`crate::ids::AttachmentId`]。
    Attachment,
}

impl IdKind {
    /// 技術的診断用の安定トークン（`Display` と同一。ロケール依存なし）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Sheet => "sheet",
            Self::Row => "row",
            Self::TypeDef => "type_def",
            Self::Attachment => "attachment",
        }
    }
}

impl fmt::Display for IdKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// クレート共通エラー。design「Error Categories and Responses」表の全 10 変種。
///
/// 変種は診断文脈のみを保持し、提示用の文言を持たない（モジュール docs 参照）。
/// `Display` はログ用の技術的診断であり、ユーザー向け表示は呼び出し元が
/// `match` で変種と文脈から組み立てる。
#[derive(Debug, Error)]
pub enum DocumentError {
    /// コンテナ不正: 許可リスト外のエントリ名、重複パス（design 表、要件 2.5, 2.6）。
    /// 応答は中止。`entry` は該当エントリ名（タスク 1.6 で `EntryName` 型へ）。
    #[error("invalid container: entry `{entry}`")]
    InvalidContainer { entry: String },

    /// パート欠落: manifest / document パートの不在（要件 4.5）。応答は中止。
    #[error("missing part: {name}")]
    MissingPart { name: String },

    /// 完全性: BLAKE3 ダイジェストの不一致（要件 5.3）。応答は中止。
    #[error("integrity mismatch at entry: {entry}")]
    IntegrityMismatch { entry: String },

    /// 構造: 識別子の重複（要件 4.3）。応答は中止。
    /// `occurrences` は各重複の出現箇所を示す説明（パート名 + 位置など）。
    #[error("duplicate {kind} id {id}; occurrences: {}", occurrences.join(", "))]
    DuplicateId {
        /// 重複した識別子の種別。
        kind: IdKind,
        /// 重複した識別子。
        id: String,
        /// 出現箇所（要件 4.3 が要求する診断情報。design 表 `{ kind, id }` の拡張）。
        occurrences: Vec<String>,
    },

    /// 構造: シートデータに対応するスキーマがない（要件 4.4）。応答は中止。
    #[error("no schema part for sheet {sheet}")]
    MissingSchema { sheet: String },

    /// 構造: 実在しない型定義への参照（要件 1.7）。応答は報告。
    #[error("dangling type definition ref: {from} -> {to}")]
    DanglingTypeRef {
        /// 参照元（行 / セルの位置）。
        from: String,
        /// 実在しない参照先の型定義識別子。
        to: String,
    },

    /// 構造: 実在しない添付への参照（要件 7.4）。応答は報告。
    #[error("dangling attachment ref: {from} -> {id}")]
    DanglingAttachmentRef {
        /// 参照元（行 / セルの位置）。
        from: String,
        /// 実在しない添付識別子。
        id: String,
    },

    /// バージョン: 現行より新しい形式（要件 6.5）。応答は中止。
    #[error("unsupported version: found {found}, supported {supported}")]
    UnsupportedVersion {
        /// ファイルに記録されていたバージョン。
        found: FormatVersion,
        /// 本実装が読み込めるバージョン。
        supported: FormatVersion,
    },

    /// 値: NaN / Infinity の書き出し試行（要件 3.3 系）。保存を中止。
    #[error("non-representable number at {location}")]
    NonRepresentableNumber {
        /// 該当するセル等の位置。
        location: String,
    },

    /// 入出力: 読み書きの失敗、rename のリトライ枯渇（要件 5.6）。
    /// 応答は中止（既存ファイルは無変更）。
    ///
    /// `retried` は rename リトライを枯渇させて返すときに `true`。
    /// `source` フィールドのため `Error::source()` は結線されるが、多フィールド
    /// 変種なので `From<std::io::Error>` は定義しない（呼び出し元が構築する）。
    #[error("i/o failure (retried: {retried}): {source}")]
    Io {
        /// 基となった I/O エラー。
        source: std::io::Error,
        /// rename リトライを枯渇させたかどうか。
        retried: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 変種を（変種名, 保持する診断文脈）へ分解する。
    ///
    /// `match` はワイルドカードなしで全変種を網羅する: 変種が増減すれば
    /// ここでコンパイルが壊れ、判別可能性を機械的に保証する。
    fn discriminate(err: &DocumentError) -> (&'static str, Vec<String>) {
        match err {
            DocumentError::InvalidContainer { entry } => {
                ("InvalidContainer", vec![entry.clone()])
            }
            DocumentError::MissingPart { name } => ("MissingPart", vec![name.clone()]),
            DocumentError::IntegrityMismatch { entry } => {
                ("IntegrityMismatch", vec![entry.clone()])
            }
            DocumentError::DuplicateId { kind, id, occurrences } => {
                let mut ctx = vec![kind.to_string(), id.clone()];
                ctx.extend(occurrences.iter().cloned());
                ("DuplicateId", ctx)
            }
            DocumentError::MissingSchema { sheet } => ("MissingSchema", vec![sheet.clone()]),
            DocumentError::DanglingTypeRef { from, to } => {
                ("DanglingTypeRef", vec![from.clone(), to.clone()])
            }
            DocumentError::DanglingAttachmentRef { from, id } => {
                ("DanglingAttachmentRef", vec![from.clone(), id.clone()])
            }
            DocumentError::UnsupportedVersion { found, supported } => {
                ("UnsupportedVersion", vec![found.to_string(), supported.to_string()])
            }
            DocumentError::NonRepresentableNumber { location } => {
                ("NonRepresentableNumber", vec![location.clone()])
            }
            DocumentError::Io { retried, .. } => ("Io", vec![format!("retried={retried}")]),
        }
    }

    /// design エラー表の全 10 変種を、診断としてあり得る文脈で 1 個ずつ構築する。
    fn all_variants() -> Vec<DocumentError> {
        let version = |major, minor| FormatVersion { major, minor };
        vec![
            DocumentError::InvalidContainer { entry: "outside/../outside.json".into() },
            DocumentError::MissingPart { name: "manifest.json".into() },
            DocumentError::IntegrityMismatch {
                entry: "parts/01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ.sheet.json".into(),
            },
            DocumentError::DuplicateId {
                kind: IdKind::Row,
                id: "01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ".into(),
                occurrences: vec![
                    "parts/01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ.sheet.json#rows[3]".into(),
                    "parts/01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ.sheet.json#rows[9]".into(),
                ],
            },
            DocumentError::MissingSchema { sheet: "01JSCHEMAMISSING000000000".into() },
            DocumentError::DanglingTypeRef {
                from: "rows[42]/cells[spec_type]".into(),
                to: "01JTYPEDEFDELETED000000000".into(),
            },
            DocumentError::DanglingAttachmentRef {
                from: "rows[7]/cells[photo]".into(),
                id: "a3dd2f9c4e1b58c7d0a6f47c2b1e5d90e0c1b7a6f5e4d3c2b2a9d8c7e6f5d412".into(),
            },
            DocumentError::UnsupportedVersion { found: version(2, 0), supported: version(1, 0) },
            DocumentError::NonRepresentableNumber { location: "rows[9]/cells[score]".into() },
            DocumentError::Io {
                source: std::io::Error::other("rename retry exhausted"),
                retried: true,
            },
        ]
    }

    /// 全変種が構築可能であり、ワイルドカードなしの `match` で自分自身の変種に
    /// 判別され、文脈フィールドが取り出せる（タスク 1.4 受け入れ: construct + match）。
    #[test]
    fn every_variant_is_constructible_and_discriminable() {
        let errors = all_variants();
        let labels: Vec<&'static str> = errors.iter().map(|e| discriminate(e).0).collect();
        assert_eq!(
            vec![
                "InvalidContainer",
                "MissingPart",
                "IntegrityMismatch",
                "DuplicateId",
                "MissingSchema",
                "DanglingTypeRef",
                "DanglingAttachmentRef",
                "UnsupportedVersion",
                "NonRepresentableNumber",
                "Io",
            ],
            labels,
            "design エラー表の 10 変種が判別可能でない"
        );

        // 各変種は最低 1 つの診断文脈を露出する（空文脈の変種は診断不能）。
        for (label, ctx) in errors.iter().map(discriminate) {
            assert!(!ctx.is_empty(), "{label} は診断文脈を保持していない");
        }

        // 多フィールド変種で抽出値・順が正しいこと。
        let (label, ctx) = discriminate(&errors[3]);
        assert_eq!("DuplicateId", label);
        assert_eq!(
            vec![
                "row",
                "01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ",
                "parts/01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ.sheet.json#rows[3]",
                "parts/01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ.sheet.json#rows[9]",
            ],
            ctx,
            "DuplicateId は kind・id・occurrences（要件 4.3 の出現箇所）を保持する"
        );

        let (label, ctx) = discriminate(&errors[7]);
        assert_eq!("UnsupportedVersion", label);
        assert_eq!(vec!["2.0", "1.0"], ctx, "UnsupportedVersion は found/supported を保持する");

        let (label, ctx) = discriminate(&errors[9]);
        assert_eq!("Io", label);
        assert_eq!(vec!["retried=true"], ctx);
    }

    /// Display は提示文言ではなく技術的診断であり、診断文脈の値をそのまま含む。
    /// source 連鎖は [`DocumentError::Io`] のみが結線する（他変種は源を持たない）。
    #[test]
    fn display_carries_context_and_only_io_has_a_source() {
        use std::error::Error as _;

        let errors = all_variants();
        let needles: [&[&str]; 10] = [
            &["outside/../outside.json"],
            &["manifest.json"],
            &["parts/01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ.sheet.json"],
            &[
                "row",
                "01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ",
                "parts/01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ.sheet.json#rows[3]",
                "parts/01JQ0ZK6Y7W2H3NQ8RTVXBGMAZ.sheet.json#rows[9]",
            ],
            &["01JSCHEMAMISSING000000000"],
            &["rows[42]/cells[spec_type]", "01JTYPEDEFDELETED000000000"],
            &[
                "rows[7]/cells[photo]",
                "a3dd2f9c4e1b58c7d0a6f47c2b1e5d90e0c1b7a6f5e4d3c2b2a9d8c7e6f5d412",
            ],
            &["2.0", "1.0"],
            &["rows[9]/cells[score]"],
            &["rename retry exhausted", "retried: true"],
        ];
        for (err, ctx) in errors.iter().zip(needles) {
            let text = err.to_string();
            for needle in ctx {
                assert!(text.contains(needle), "Display {text:?} に診断文脈 {needle:?} が含まれない");
            }
        }

        // Io は標準の source 連鎖で基の io::Error を復元できる。
        let src = errors[9].source().expect("Io は source を保持する");
        assert!(src.downcast_ref::<std::io::Error>().is_some());
        for err in &errors[..9] {
            assert!(err.source().is_none(), "Io 以外の変種は源を持たない");
        }
    }
}

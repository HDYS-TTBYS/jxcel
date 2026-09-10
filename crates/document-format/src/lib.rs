//! jxcel ドキュメント形式の実装。
//! ZIP コンテナに格納された JSON テキストとメモリ上のドキュメントモデルを
//! 双方向に変換する。同一内容は常に同一バイト列になる（決定的出力）。
//!
//! # 公開 API 層（design「Public API Layer / DocumentFormatApi」）
//!
//! 本クレートの対外的な入口が [`DocumentFormatApi`] とその具象実装 [`DocumentFormat`] である
//! （`Ids / Value / EntryName → Model → Json → Parts → Container → Api` の最右。
//! design「Architecture Integration」）。タスク 7.1 は**読み込み**（[`DocumentFormatApi::open`]）
//! を結線する: ファイル I/O はここだけが担い、下位層（[`crate::container`] /
//! [`crate::parts`] / [`crate::migration`]）の 1 経路を順に呼ぶ。検証ロジックをこの層へ
//! 持ち込まない（design「Validation: `open` と `from_parts` は同一の検証経路を通る。
//! 検証ロジックを 2 箇所に持たない」）。
//!
//! `save` / `to_parts` / `from_parts` の公開結線は後続タスク（7.2 / 7.3）が**同じトレイトへ
//! 加点的に**足す。本クレートに未実装メソッドのスタブは置かない。
//!
//! ## 読み込みの順序（design「読み込みフロー」。要件 4.1, 5.2, 5.4, 5.5）
//!
//! [`DocumentFormatApi::open`] は次の順に既存の層を接続する:
//!
//! 1. **ファイルの読み込み**: [`std::fs::read`]。失敗は [`DocumentError::Io`]
//!    （`retried` は保存経路の rename リトライ枯渇だけが `true`。読み込みは `false`）。
//! 2. **コンテナの復号**: [`ContainerCodec::decode`]（タスク 5.3）。ZIP の知識
//!    （許可リスト照合・重複検出・サイズ照合・型マーカー）はこの 1 経路に閉じる。
//! 3. **移行の事前判定**: [`MigrationChain::gate`]（`const fn`・副作用なし）。記録値が
//!    [`VersionVerdict::NeedsMigration`] ならその `from` を [`OpenOutcome::migrated_from`]
//!    の候補にする。
//! 4. **検証とモデル構築**: [`crate::parts::from_parts`]（タスク 4.8 + 6.1 + 6.2 の 1 経路）。
//!    バージョンゲート・段階的移行・ダイジェスト照合・構造検証・モデル構築をこの 1 経路が
//!    運ぶ。**移行の適用（`MigrationChain::apply`）を直接呼ばない**: 移行前の破損検出
//!    （記録どおりの照合）は `from_parts` 側にあり、直呼びはそれを迂回する
//!    （タスク 6.2 の申し送り）。
//! 5. **規模の通知**: 全シートの行数の合計が [`SUPPORTED_ROW_LIMIT`] を超えたときだけ
//!    [`OpenOutcome::beyond_supported_scale`] を `true` にする（要件 8.5）。
//!
//! ## `migrated_from` を `gate` の事前判定から得る理由
//!
//! [`MigrationChain::gate`] は**純粋な判定**（`const fn`・状態を持たず・集合に触れない）で
//! あり、この判定を `open` が先に知っても検証は 1 箇所のままである。移行が実際に成功したか
//! どうかは [`crate::parts::from_parts`] が決める: 記録値に一致する段が無い／現行 major へ
//! 届かない場合は [`DocumentError::UnsupportedVersion`] で中止するため、
//! `from_parts` が `Ok` を返した時点で「[`VersionVerdict::NeedsMigration`] だった」ことが
//! 「移行が適用された」ことを意味する（`from_parts` のシグネチャは変えない）。
//! したがって `gate` を 2 回（`open` と `from_parts`）呼んでよい。
//!
//! ## 読み込み経路は書き込みを行わない（要件 5.5）
//!
//! 本層の読み込み経路が使うのは [`std::fs::read`] だけである: `std::fs::write` /
//! `File::create` / `OpenOptions` / `tempfile` を持たない（保存はタスク 7.2 が
//! [`crate::container::atomic_save`] 経由で足す）。破損を検出しても自動修復・上書きを
//! しない。`Err` のとき `path` のファイルは変更されない（要件 5.6 の前提）。
//!
//! ## 部分的なモデルを返さない（要件 5.4）
//!
//! すべての検証は [`crate::parts::from_parts`] の中でモデル構築の前に完了し、
//! [`DocumentFormatApi::open`] はその戻り値だけを [`OpenOutcome`] へ載せる。
//! `Ok` のとき返る [`Document`] は全検証を通過したものだけであり、中途半端に構築された
//! モデルが外へ出る経路は構造的に無い。

pub mod container;
pub mod entry_name;
pub mod error;
pub mod ids;
pub mod integrity;
pub mod json;
pub mod migration;
pub mod model;
pub mod parts;
pub mod value;

pub use entry_name::EntryName;
pub use error::{DocumentError, IdKind};
pub use ids::{
    AttachmentId, Blake3Digest, DocumentId, IdFactory, IdParseError, RowId, SheetId, TypeDefId,
};
pub use migration::{CURRENT_FORMAT_VERSION, FormatVersion, MigrationChain, VersionVerdict};
pub use model::{
    Attachment, AttachmentRegistry, Document, RawJson, ReorderError, Row, SchemaPart, Sheet,
    TypeDef, UnknownRow, UnknownSheet,
};
pub use value::{CellValue, NestedValue, from_json_bytes, to_json_bytes};

use std::path::Path;

use crate::container::ContainerCodec;
use crate::parts::from_parts;

/// 性能保証の対象となる 1 ドキュメントあたりの行数（要件 8.4）。
///
/// 1 つのドキュメントにつき合計 10 万行を保証対象とする。この数を超えるドキュメントは
/// **拒否しない**: 読み込みは成功し、[`OpenOutcome::beyond_supported_scale`] が性能保証の
/// 対象外であることを通知する（要件 8.5）。
pub const SUPPORTED_ROW_LIMIT: usize = 100_000;

/// ドキュメントを開いた結果（design「Public API Layer / DocumentFormatApi」の Service Interface）。
///
/// 派生は [`Document`] が実際に実装するトレイトだけに限る（[`Document`] は保持フィールドを
/// 持つため `PartialEq` を持たない）。
#[derive(Debug)]
pub struct OpenOutcome {
    /// 全検証を通過したドキュメントモデル。
    pub document: Document,
    /// 読み込み時に形式変換が適用された場合、変換前のバージョン（要件 6.2, 6.3）。
    ///
    /// 移行が不要だった場合（現行 major）は `None`。古い major でも移行先が無ければ読み込み
    /// 自体が中止されるため、`Ok` と `None` 以外の組は現れない（型の docs「`migrated_from` を
    /// `gate` の事前判定から得る理由」）。
    pub migrated_from: Option<FormatVersion>,
    /// 行数が保証対象の 10 万行を超えていた場合に `true`（要件 8.5）。
    ///
    /// 判定は全シートの行数の合計で行い、しきい値は [`SUPPORTED_ROW_LIMIT`]。
    pub beyond_supported_scale: bool,
}

/// ドキュメントの公開面（design「Public API Layer / DocumentFormatApi」）。
///
/// 本トレイトは後続タスクが**加点的に**育てる: タスク 7.1 は [`Self::open`] を定義し、
/// 7.2 が `save`、7.3 が `to_parts` / `from_parts` を加える（未実装メソッドのスタブは
/// 置かない）。
pub trait DocumentFormatApi {
    /// ZIP コンテナを読み、検証し、ドキュメントモデルを構築する。
    ///
    /// 事前条件: `path` が読み取り可能なファイルであること。
    /// 事後条件: `Ok` の場合、返る [`OpenOutcome::document`] は全検証を通過している。
    /// `Err` の場合、部分的なモデルは返らず、`path` のファイルは変更されない（要件 5.4, 5.5）。
    ///
    /// 処理順はモジュール docs「読み込みの順序」にある。ファイル I/O は本メソッドだけが担い、
    /// コンテナの復号・バージョンゲート・移行・ダイジェスト照合・構造検証・モデル構築は
    /// 下位層の 1 経路を呼ぶ。
    fn open(&self, path: &Path) -> Result<OpenOutcome, DocumentError>;
}

/// 公開面の無状態の具象実装（design はトレイトのみを指定するため、利用側が値を持てるように
/// 本型を添える）。
///
/// ```text
/// DocumentFormat::new().open(path)
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct DocumentFormat;

impl DocumentFormat {
    /// 無状態の実装を作る。
    #[inline]
    pub const fn new() -> Self {
        Self
    }
}

impl DocumentFormatApi for DocumentFormat {
    fn open(&self, path: &Path) -> Result<OpenOutcome, DocumentError> {
        // 1. ファイルの読み込み。読み込み経路が使う I/O は `fs::read` だけである
        //    （要件 5.5）。`retried` は保存経路の rename リトライ枯渇のための標識であり、
        //    読み込みでは常に `false`。
        let bytes = std::fs::read(path)
            .map_err(|source| DocumentError::Io { source, retried: false })?;

        // 2. コンテナの復号と、許可リスト照合・重複検出・サイズ照合・型マーカー検査
        //    （タスク 5.3。ZIP の知識はこの 1 経路に閉じる）。
        let parts = ContainerCodec::decode(&bytes)?;

        // 3. 移行の事前判定（純粋な判定。モジュール docs 参照）。
        //    実際に移行が成功したかは 4 の `from_parts` が決める。
        let migrated_from =
            migrated_from_verdict(MigrationChain::gate(parts.format_version()));

        // 4. バージョンゲート・段階的移行・ダイジェスト照合・構造検証・モデル構築
        //    （タスク 4.8 + 6.1 + 6.2 の 1 経路）。`MigrationChain::apply` を直接呼ばない
        //    （移行前の破損検出は `from_parts` 側にある。タスク 6.2 の申し送り）。
        let document = from_parts(&parts)?;

        // 5. 規模の通知（要件 8.4, 8.5）。行は走査せず、シートごとの `len()` の合計だけを
        //    見る（通常経路の性能に影響させない。要件 8.1）。
        Ok(OpenOutcome {
            beyond_supported_scale: exceeds_supported_scale(&document),
            migrated_from,
            document,
        })
    }
}

/// 全シートの行数の合計が保証対象（[`SUPPORTED_ROW_LIMIT`]）を超えるか（要件 8.4, 8.5）。
///
/// `Sheet::rows()` の長さだけを見る（行を 1 つずつ数えない）ため、費用はシート数に比例する。
/// 合計がしきい値を超えた時点で打ち切る。
fn exceeds_supported_scale(document: &Document) -> bool {
    let mut total = 0usize;
    for sheet in document.sheets() {
        total = total.saturating_add(sheet.rows().len());
        if total > SUPPORTED_ROW_LIMIT {
            return true;
        }
    }
    false
}

/// バージョンゲートの判定を [`OpenOutcome::migrated_from`] の値へ写す（要件 6.2, 6.3）。
///
/// - [`VersionVerdict::NeedsMigration`] は**移行の出発点**（ファイルに記録されていた版）を
///   返す。`open` がこの値を持つのは、続く [`crate::parts::from_parts`] が移行を適用して
///   `Ok` を返した場合だけである（移行先が無ければ `from_parts` が中止する）。
/// - [`VersionVerdict::Openable`] は移行しないため `None`。
/// - [`VersionVerdict::Unsupported`] も `None` を返す。読めない版は
///   [`crate::parts::from_parts`] が [`DocumentError::UnsupportedVersion`] で中止するため
///   `Ok` の [`OpenOutcome`] には載らないが、写像としては「移行していない」= `None` が
///   一貫している（`open` を `Err` で抜けるため観測されない分岐である）。
///
/// 判定（[`MigrationChain::gate`]）は純粋であり、この写像も同じく純粋である: 状態を
/// 持たず、集合にも触れない。したがって単体テストで両分岐を固定できる（空の移行チェーン
/// では `Ok` になる古い版が存在しないため、統合テストからは `Some` を観測できない）。
fn migrated_from_verdict(verdict: VersionVerdict) -> Option<FormatVersion> {
    match verdict {
        VersionVerdict::NeedsMigration { from } => Some(from),
        VersionVerdict::Openable | VersionVerdict::Unsupported { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 判定から移行元への写像を両分岐とも固定する（要件 6.2, 6.3）。
    ///
    /// 統合テストは空の移行チェーンゆえに `NeedsMigration` の `Ok` 経路を観測できないため、
    /// 写像そのものをここで固定する。
    #[test]
    fn version_verdict_maps_to_the_recorded_migration_origin() {
        assert_eq!(
            Some(FormatVersion::new(1, 0)),
            migrated_from_verdict(VersionVerdict::NeedsMigration {
                from: FormatVersion::new(1, 0),
            }),
            "移行が必要な判定が移行元を残さない"
        );
        assert_eq!(
            None,
            migrated_from_verdict(VersionVerdict::Openable),
            "現行版（移行不要）で移行元が記録された"
        );
        assert_eq!(
            None,
            migrated_from_verdict(VersionVerdict::Unsupported {
                found: FormatVersion::new(2, 0),
                supported: FormatVersion::new(1, 0),
            }),
            "読めない版で移行元が記録された"
        );
    }
}

//! 移行ステップの宣言（タスク 6.2。要件 6.2, 6.3。design「MigrationChain」/ File Structure Plan）。
//!
//! 古い形式のドキュメントを現行版へ運ぶには、**隣接する版の間の変換**（v1→v2→v3…）を
//! 1 段ずつ適用する必要がある。本モジュールはその 1 段を**データ**として宣言する場所であり、
//! 適用の機構（鎖の組み立て・連続性の検証・版と索引の記帳）は [`super::MigrationChain`] が
//! 持つ。**初版は v1 のみのため実表 [`STEPS`] は空である**（design「MigrationChain /
//! Implementation Notes: 初版は v1 のみのため移行ステップの実装は存在しない。枠組みのみを
//! 置く」）。
//!
//! # ステップを足すときの作法
//!
//! 1. 変換関数（[`RewriteFn`]）を書き、[`MigrationStep::new`] で [`STEPS`] へ足す。変換関数は
//!    「その版の表現を次の版の表現へ書き換える」責務だけを持ち、[`DocumentParts`] を
//!    受け取って書き換えた集合を返す。
//! 2. **索引（`manifest.json`）の版とダイジェストの記帳は書かない。** 適用機構
//!    （[`super::MigrationChain::apply`]）が各段の適用後に
//!    [`crate::parts::DocumentParts::reindex`] で行う（ステップが触るのは表現だけである）。
//! 3. **未知フィールドを落とす再構築をしない。** 表現の書き換えは各パートのコーデックの
//!    復号 → 再符号化を通すこと。既知フィールドだけを組み立て直すと、将来の minor が足した
//!    省略可能フィールドが黙って消える（design「MigrationChain / Responsibilities:
//!    未知フィールドは破棄せず保持して書き戻す」）。
//! 4. **過去バージョンのゴールデン fixture を `tests/fixtures/golden/vN/` に足す。**
//!    移行チェーンの中間ステップが未保守のまま腐る失敗形態は先行事例（nbformat）で実際に
//!    起きており、fixture が唯一の防御である（design 同節）。配置・命名・生成・更新の規約は
//!    統合テスト `tests/migration.rs` の
//!    `the_golden_fixture_directory_for_the_current_version_exists` の doc にある。
//!
//! # 版の対応は完全一致で引く
//!
//! 適用機構は `from` が記録値と**完全に一致する**（minor も一致する）ステップを選ぶ。minor の
//! 増加は省略可能フィールドの追加のみであり通常は major 単位のステップで足りるが、過去に
//! minor を持つ版を書いたことがある場合はその版を `from` に持つステップを明示的に足すこと
//! （記録値に一致する `from` を持たない版は「移行先が無い」として中止される）。
//!
//! # 依存方向
//!
//! 本モジュールは [`crate::parts`] の集合型に依存する（ステップは集合の表現を書き換える）。
//! [`crate::parts::from_parts`] は [`super::MigrationChain`] に依存するため `migration` ⇄
//! `parts` の相互参照になるが、**同一クレート内の型参照**であり、コンポーネント間の依存
//! 方向を増やさない（`error` ⇄ `migration` と同じ整理。モジュール docs「依存方向」）。
//! 本モジュールは `std::fs` も `zip` も参照しない（移行はメモリ上の変換である）。

use crate::error::DocumentError;
use crate::parts::DocumentParts;

use super::FormatVersion;

/// 1 段分の変換関数: その版の表現を次の版の表現へ書き換える。
///
/// 引数は書き換え前の集合（`from` の版の表現）、戻り値は書き換え後の集合である。索引の版と
/// ダイジェストの記帳は適用機構の責務なので、実装は表現だけを書き換えればよい
/// （モジュール docs の作法 2）。
pub type RewriteFn = fn(&DocumentParts) -> Result<DocumentParts, DocumentError>;

/// 移行チェーンの 1 段（design「MigrationChain / Responsibilities: 変換は v1→v2→v3 の順に
/// 1 段ずつ適用する」）。
///
/// `from` / `to` は**隣接する版**である。表 [`STEPS`] の中で `from` は一意でなければならず
/// （鎖が一意に定まらなくなる）、`to` は `from` より前進していなければならない。どちらも
/// 適用機構が検証し、破れていれば中止する（表の欠陥は呼び出し元の programming error である）。
pub struct MigrationStep {
    /// この段が前提とする版（ファイルに記録されている値と完全一致で引かれる）。
    from: FormatVersion,
    /// この段が出力する版（次の段の `from` と一致していなければならない）。
    to: FormatVersion,
    /// 表現の書き換え。
    rewrite: RewriteFn,
}

impl MigrationStep {
    /// 1 段を宣言する（[`STEPS`] の要素として使う）。
    pub const fn new(from: FormatVersion, to: FormatVersion, rewrite: RewriteFn) -> Self {
        Self { from, to, rewrite }
    }

    /// 前提とする版。
    pub const fn from(&self) -> FormatVersion {
        self.from
    }

    /// 出力する版。
    pub const fn to(&self) -> FormatVersion {
        self.to
    }

    /// 表現を書き換える（`self` は書き換え前の集合を借用する）。
    ///
    /// 索引と版の記帳は行わない（適用機構が [`DocumentParts::reindex`] で行う）。
    pub fn apply(&self, parts: &DocumentParts) -> Result<DocumentParts, DocumentError> {
        (self.rewrite)(parts)
    }
}

/// 本実装が持つ移行ステップの表（宣言順）。
///
/// **初版は v1 のみのため空である**（design「Implementation Notes」）。空の表でも読み込みの
/// 経路は成立する: 現行版の集合はゲートが [`super::VersionVerdict::Openable`] と判定して
/// そのまま読み、現行版以外の major はステップが無いため「移行先が無い」として中止する
/// （[`super::MigrationChain::apply`]）。
///
/// ステップを足す手順はモジュール docs「ステップを足すときの作法」。
pub const STEPS: &[MigrationStep] = &[];

#[cfg(test)]
pub(crate) mod synthetic {
    //! 段階適用の経路を試すための合成ステップ群（テスト専用。公開面には出さない）。
    //!
    //! 実表 [`STEPS`] は v1 のみで空であるため、**多段の適用・適用順序・連続性の検証・
    //! 記帳・未知フィールドの保持**は実表では観測できない。本モジュールは `from`/`to` を
    //! 過去の版（0.x）に置いた合成の表を提供し、[`super::MigrationChain::apply_with`]
    //! （クレート可視）と `crate::parts::document_parts` のモジュール内入口へ注入する。
    //!
    //! 各ステップは `document.json` のシート名の末尾へ自分の版の印を足し、2 段目は
    //! **前段の印を要求する**。したがって適用順序が壊れれば印が積まれず、段が飛べば最終形に
    //! ならない（順序を内容の上で観測できる）。書き換えは `DocumentPart` の復号 → 再符号化で
    //! 行うため、未知フィールドを落とす経路を通らない（`steps.rs` の作法 3）。

    use crate::entry_name::{EntryName, MANIFEST_ENTRY};
    use crate::error::DocumentError;
    use crate::parts::document_part::{DocumentPart, SheetMeta};
    use crate::parts::manifest::{ManifestEntry, ManifestPart};
    use crate::parts::DocumentParts;

    use super::{FormatVersion, MigrationStep};

    /// 印を積む前の版（この版から 2 段で現行版 1.0 へ届く）。
    pub(crate) const OLDEST: FormatVersion = FormatVersion::new(0, 0);

    /// 印を積んだ中間の版。
    pub(crate) const MIDDLE: FormatVersion = FormatVersion::new(0, 1);

    /// 0.0 → 0.1 の段がシート名へ足す印。
    pub(crate) const MARK_V0_1: &str = "|v0.1";

    /// 0.1 → 1.0 の段がシート名へ足す印。
    pub(crate) const MARK_V1_0: &str = "|v1.0";

    /// 多段の鎖（0.0 → 0.1 → 1.0）。適用順序を内容で観測する標準の表。
    pub(crate) const MULTI_STEP: &[MigrationStep] = &[
        MigrationStep::new(OLDEST, MIDDLE, stamp_v0_1),
        MigrationStep::new(MIDDLE, FormatVersion::new(1, 0), stamp_v1_0),
    ];

    /// [`MULTI_STEP`] と同じ 2 段を**宣言順だけ逆**に並べた表。段の適用順は `from` の一致で
    /// 引いた鎖の順であり、表の並びではないことを固定する。
    pub(crate) const REVERSED: &[MigrationStep] = &[
        MigrationStep::new(MIDDLE, FormatVersion::new(1, 0), stamp_v1_0),
        MigrationStep::new(OLDEST, MIDDLE, stamp_v0_1),
    ];

    /// 記録値（0.0）から出る段が無い表（隙間）。
    pub(crate) const GAP: &[MigrationStep] =
        &[MigrationStep::new(MIDDLE, FormatVersion::new(1, 0), stamp_v1_0)];

    /// 現行 major に届かない表（0.1 で止まる）。
    pub(crate) const SHORT: &[MigrationStep] = &[MigrationStep::new(OLDEST, MIDDLE, stamp_v0_1)];

    /// 前進しない段（0.0 → 0.0）を含む表。
    pub(crate) const STALLED: &[MigrationStep] = &[MigrationStep::new(OLDEST, OLDEST, stamp_v0_1)];

    /// 現行 major（1）を飛び越す段（0.0 → 2.0）を含む表。
    pub(crate) const OVERSHOOT: &[MigrationStep] =
        &[MigrationStep::new(OLDEST, FormatVersion::new(2, 0), stamp_v1_0)];

    /// 同じ版から 2 段出る表（鎖が一意に定まらない）。
    pub(crate) const AMBIGUOUS: &[MigrationStep] = &[
        MigrationStep::new(OLDEST, FormatVersion::new(1, 0), stamp_v1_0),
        MigrationStep::new(OLDEST, MIDDLE, stamp_v0_1),
    ];

    /// 0.0 → 0.1 の段: シート名の末尾へ `|v0.1` を足す。
    pub(crate) fn stamp_v0_1(parts: &DocumentParts) -> Result<DocumentParts, DocumentError> {
        append_mark(parts, MARK_V0_1, None)
    }

    /// 0.1 → 1.0 の段: 前段の印を確かめてから `|v1.0` を足す。
    pub(crate) fn stamp_v1_0(parts: &DocumentParts) -> Result<DocumentParts, DocumentError> {
        append_mark(parts, MARK_V1_0, Some(MARK_V0_1))
    }

    /// 集合を指定バージョンの索引で組み直す（ダイジェストは実体に一致させたまま、記録値だけを
    /// 差し替える）。古い版の集合を合成表へ通す入力を作る。
    pub(crate) fn recorded_at(version: FormatVersion, parts: &DocumentParts) -> DocumentParts {
        let mut entries: Vec<(EntryName, Vec<u8>)> = data_entries(parts);
        let index: Vec<ManifestEntry> = entries
            .iter()
            .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
            .collect();
        let manifest = ManifestPart::new(version, index)
            .expect("標本の索引は妥当")
            .to_json_bytes()
            .expect("符号化");
        entries.push((MANIFEST_ENTRY, manifest));
        DocumentParts::from_entries(entries).expect("標本の集合は妥当")
    }

    /// 標本の集合へ未知フィールドを足したもの（`document.json` のトップレベルとシート要素、
    /// `manifest.json` のトップレベルと索引要素。要件 6.2 / 6.3 の観測用）。
    ///
    /// 索引は書き換え後の実体に合わせて組み直す（未知フィールドの差し込みでダイジェストを
    /// 変えるため）。記録値は `version` にする。
    pub(crate) fn with_unknown_fields(
        version: FormatVersion,
        parts: &DocumentParts,
    ) -> DocumentParts {
        let mut entries = data_entries(parts);

        // document.json: トップレベルの `document_id` の手前と、要素の `sheet_id` の手前。
        let document = entries
            .iter_mut()
            .find(|(name, _)| *name == EntryName::Document)
            .expect("標本は document.json を持つ");
        let text = String::from_utf8(std::mem::take(&mut document.1)).expect("確定形は UTF-8");
        let text = text.replacen('{', r#"{"future_top":{"unit":"mm"},"#, 1);
        let text = text.replacen(r#"{"sheet_id""#, r#"{"element_note":7,"sheet_id""#, 1);
        assert_eq!(1, text.matches("future_top").count(), "未知キーの差し込みに失敗した");
        assert_eq!(1, text.matches("element_note").count(), "未知キーの差し込みに失敗した");
        document.1 = text.into_bytes();

        // manifest.json: 索引を実体に合わせて組み直し、トップレベルと 1 要素へ未知キーを挿す。
        let index: Vec<ManifestEntry> = entries
            .iter()
            .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
            .collect();
        let manifest = ManifestPart::new(version, index)
            .expect("標本の索引は妥当")
            .to_json_bytes()
            .expect("符号化");
        let text = String::from_utf8(manifest).expect("確定形は UTF-8");
        let text = text.replacen('{', r#"{"future_manifest":true,"#, 1);
        let text = text.replacen(
            r#"{"name":"document.json""#,
            r#"{"entry_note":9,"name":"document.json""#,
            1,
        );
        assert_eq!(1, text.matches("future_manifest").count(), "未知キーの差し込みに失敗した");
        assert_eq!(1, text.matches("entry_note").count(), "未知キーの差し込みに失敗した");
        entries.push((MANIFEST_ENTRY, text.into_bytes()));

        DocumentParts::from_entries(entries).expect("標本の集合は妥当")
    }

    /// 標本の集合から索引を除いた（エントリ名, バイト列）の列。
    fn data_entries(parts: &DocumentParts) -> Vec<(EntryName, Vec<u8>)> {
        parts
            .iter()
            .filter(|part| part.name != MANIFEST_ENTRY)
            .map(|part| (part.name, part.bytes.clone()))
            .collect()
    }

    /// `document.json` の全シート名の末尾へ `mark` を足した集合を返す。
    ///
    /// `required` を与えた場合、全シート名がその接尾辞で終わっていなければ中止する
    /// （= 前段のステップが先に走っていなければならない、という前提の表現）。
    /// 書き換えは `DocumentPart` の復号 → 再符号化で行う: 未知フィールドは `PreservedFields`
    /// の経路でそのまま書き戻る（`steps.rs` の作法 3）。
    fn append_mark(
        parts: &DocumentParts,
        mark: &str,
        required: Option<&str>,
    ) -> Result<DocumentParts, DocumentError> {
        let entry =
            parts.get(&EntryName::Document).ok_or_else(|| DocumentError::MissingPart {
                name: EntryName::Document.to_string(),
            })?;
        let decoded = DocumentPart::from_json_bytes(&entry.bytes)?;
        let mut metas = Vec::with_capacity(decoded.sheets().len());
        for meta in decoded.sheets() {
            if let Some(required) = required {
                if !meta.name().ends_with(required) {
                    return Err(DocumentError::InvalidContainer {
                        entry: format!(
                            "{}: the previous migration step did not run first",
                            EntryName::Document
                        ),
                    });
                }
            }
            metas.push(
                SheetMeta::new(meta.sheet_id(), format!("{}{mark}", meta.name()))
                    .with_columns(meta.columns().to_vec())
                    .with_preserved(meta.preserved_fields().clone()),
            );
        }
        let rewritten = DocumentPart::new(decoded.document_id(), metas)?
            .with_preserved(decoded.preserved_fields().clone())
            .to_json_bytes()?;
        replace_entry(parts, EntryName::Document, rewritten)
    }

    /// 集合の 1 パートだけを差し替えた集合を組み立て直す（他のパートはそのまま運ぶ）。
    ///
    /// 索引は組み直さない: 版とダイジェストの記帳は適用機構の責務である
    /// （`migration/mod.rs` のモジュール docs）。
    fn replace_entry(
        parts: &DocumentParts,
        name: EntryName,
        bytes: Vec<u8>,
    ) -> Result<DocumentParts, DocumentError> {
        let entries = parts
            .iter()
            .map(|part| {
                if part.name == name {
                    (name, bytes.clone())
                } else {
                    (part.name, part.bytes.clone())
                }
            })
            .collect();
        DocumentParts::from_entries(entries)
    }
}

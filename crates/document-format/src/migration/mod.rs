//! 形式バージョン、読み込み時ゲート、段階的移行チェーン（タスク 6.1 / 6.2。要件 6.1, 6.2,
//! 6.3, 6.5。design「MigrationChain」）。
//!
//! 本モジュールは本クレートで「今の形式は何か」「その記録値は読めるか」「どう現行版へ運ぶか」
//! を決める唯一の場所である:
//!
//! | 項目 | 実体 |
//! |------|------|
//! | 形式バージョンの型 | [`FormatVersion`]（`major.minor`） |
//! | 現行バージョンの単一定義 | [`CURRENT_FORMAT_VERSION`]（初版は 1.0） |
//! | 記録値の判定 | [`MigrationChain::gate`]（[`VersionVerdict`]） |
//! | 古い形式の段階的な適用 | [`MigrationChain::apply`]（子モジュール [`steps`] の表を適用する） |
//!
//! # 記録と判定（要件 6.1 / 6.5）
//!
//! 保存経路（[`crate::parts::to_parts`]）は [`CURRENT_FORMAT_VERSION`] を `manifest.json` へ
//! 記録する（要件 6.1。wire 形 `{"major":..,"minor":..}` の所有者は
//! [`crate::parts::manifest`] であり、[`FormatVersion`] 自身に `Serialize` を後付けしない）。
//! 読み込み経路は索引の記録値（[`crate::parts::DocumentParts::format_version`]）を
//! [`MigrationChain::gate`] に掛け、**ダイジェスト照合より前**に版の可否を決める。
//!
//! 判定は major だけで決まる（design「MigrationChain / Responsibilities」）:
//!
//! | 記録された major | 判定 | 読み込み経路の応答 |
//! |------------------|------|--------------------|
//! | 現行より新しい | [`VersionVerdict::Unsupported`] | 中止（要件 6.5。**要求バージョンを含める**） |
//! | 現行と同じ | [`VersionVerdict::Openable`] | そのまま読む（移行しない） |
//! | 現行より古い | [`VersionVerdict::NeedsMigration`] | 移行チェーンを 1 段ずつ適用する（要件 6.2 / 6.3） |
//!
//! **minor の増加は省略可能フィールドの追加のみ**に限る（design 同節）。したがって同一 major の
//! より新しい minor は未対応のキーを持つだけであり、本クレートは未知フィールドを破棄せず
//! 保持して書き戻すため（要件 6.2 / 6.3）、読めない理由が無い。**判定に minor を混ぜない**
//! ことがこの判定の意味である。
//!
//! # 段階的移行チェーン（要件 6.2 / 6.3）
//!
//! 移行は**隣接する版の間の変換**（v1→v2→v3…）を 1 段ずつ適用する（design
//! 「MigrationChain / Responsibilities: 変換は v1→v2→v3 の順に 1 段ずつ適用する」）。1 段は
//! `from` / `to` / 書き換え関数を持つ**データ**として [`steps::STEPS`] に宣言され、適用機構
//! である [`MigrationChain::apply`] が次の規律で運ぶ:
//!
//! 1. **段の選択**: 記録値と `from` が**完全に一致する**（minor も一致する）段を選ぶ。選んだ段の
//!    `to` が次の段の `from` になる。表の中で `from` は一意でなければならない。
//! 2. **連続性の検証**: 段が無い（隙間）・同じ版から 2 段出る（鎖が一意に定まらない）・`to` が
//!    前進しない（後退・停滞）・現行 major を飛び越す・現行 major に届かない、のいずれも中止
//!    する。検証は**適用の前**に計画（段の列）を組み立てる段で行うため、欠陥のある表では 1 段も
//!    適用されない。
//! 3. **終端**: 計画の終端が**現行と同じ major** に到達していることを検証する。minor の一致は
//!    要求しない（minor の増加は省略可能フィールドの追加のみであり、[`MigrationChain::gate`] は
//!    同一 major を現行版として読む。現行 minor が上がるたびに段を書き換える規則は表を
//!    minor 更新のたびに腐らせる）。
//! 4. **版と索引の記帳**: 各段を適用した後、`manifest.json` の形式バージョンを**その段の `to`**
//!    に更新し、索引のダイジェストを集合の実内容に合わせて組み直す
//!    （[`crate::parts::DocumentParts::reindex`]）。段は**表現**だけを書き換えればよく、記帳を
//!    各段の作者に任せない。移行後の集合は現行版として一貫している（要件 5.1 / 6.2）。
//! 5. **未知フィールドの保持**: 段の書き換えは各パートのコーデックの復号 → 再符号化を通す。
//!    既知フィールドだけを組み立て直すと、将来の minor が足した省略可能フィールドが黙って
//!    消えてしまう（design「MigrationChain / Responsibilities: 未知フィールドは破棄せず保持して
//!    書き戻す」）。索引（`manifest.json`）の未知フィールドも記帳で引き継がれる。
//! 6. **現行版は複製しない**: 移行が不要な集合（[`VersionVerdict::Openable`]）に対して
//!    [`MigrationChain::apply`] は `Ok(None)` を返し、集合を組み立て直さない（要件 8.1 / 8.2 の
//!    予算に移行の複製と再ハッシュを持ち込まない）。
//!
//! **無限ループしない**: 段の選択は各反復で版を厳密に前進させるので、有限の表では同じ段が
//! 2 度選ばれない（高々 `表の長さ + 1` 回で終わる）。
//!
//! # 移行とダイジェスト照合の順序（タスク 6.2 の親の裁定）
//!
//! design「読み込みフロー」は「バージョンゲート → 移行 → ダイジェスト照合」と書いているが、
//! **この順序のままでは古い形式のファイルの破損を検出できない**: 移行が索引を無条件に
//! 作り直すため、壊れた古いファイルが黙って「修復」されて読まれてしまう。要件 5.2（記録された
//! 完全性検証値と実際の内容の照合）は古い形式のファイルにも適用される。
//!
//! したがって読み込み経路（[`crate::parts::from_parts`]）は次の順で運ぶ:
//!
//! 1. 形式バージョンのゲート（[`MigrationChain::gate`]）
//! 2. **移行が必要な場合だけ**、記録されたままの集合を索引と照合する（要件 5.2 / 5.3）
//! 3. 移行チェーンの適用（[`MigrationChain::apply`]）
//! 4. 移行後の集合の照合と構造検証（`from_parts` の既存の 1 経路。検証を 2 箇所に持たない）
//!
//! 移行が不要な集合では 2 を省く（通常経路の照合は 4 の 1 回だけである）。
//!
//! # v1 のみの帰結と、ステップを足したときに起きること
//!
//! 初版は v1（[`CURRENT_FORMAT_VERSION`] = 1.0）のみであり、実表 [`steps::STEPS`] は空である
//! （design「Implementation Notes: 初版は v1 のみのため移行ステップの実装は存在しない。枠組み
//! のみを置く」）。したがって**現行版以外は移行先が無い**:
//!
//! - 記録値が現行と同じ major: [`VersionVerdict::Openable`]。移行しない。
//! - 記録値が現行より新しい major: [`VersionVerdict::Unsupported`]。中止。
//! - 記録値が現行より古い major: [`VersionVerdict::NeedsMigration`] と判定したうえで移行を
//!   試み、記録値と一致する `from` を持つ段が無いため
//!   [`DocumentError::UnsupportedVersion`]（`found` = **記録値**、`supported` = 現行）で中止
//!   する。**6.1 と同じ観測結果である**が、経路は「ゲートで即拒否」から「移行を試みたが
//!   移行先が無い」に変わっている。
//!
//! 段を足すと、記録値から現行 major まで鎖が繋がる版だけが `Ok(Some(移行後の集合))` になる。
//! 例えば `0.0 -> 0.1 -> 1.0` の 2 段を足すと、記録値 `0.0` のファイルは現行版へ移行されるが、
//! 記録値 `0.9` のファイルは `0.9` から出る段が無いため依然として中止される（記録値は `from` と
//! **完全一致**で引く。過去に minor を持つ版を書いたことがある場合はその版を `from` に持つ段を
//! 明示的に足すこと）。手順は [`steps`] のモジュール docs「ステップを足すときの作法」。
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
//! 本モジュールは [`crate::parts::DocumentParts`] に依存する（移行は集合の表現を書き換える。
//! design の Component Traceability も `MigrationChain` の依存先を `ManifestPart`（Parts 層）と
//! している）。[`crate::parts::from_parts`] が本モジュールのゲートを呼ぶため `migration` ⇄
//! `parts` の相互参照になるが、同一クレート内の型参照であり依存方向を増やさない（上記と同じ
//! 整理）。`std::fs` も `zip` も参照しない: ゲートは記録値に対する**純粋な関数**であり、移行は
//! **メモリ上**の変換である（ファイル I/O はタスク 7.1 / 7.2 の責務）。
//!
//! # エラー対応
//!
//! 移行の失敗はすべて読み込み全体の中止であり、部分的なモデルを返さない（要件 5.4）。
//! 新しい [`DocumentError`] 変種は足さない（design のエラー表は閉じている）。
//!
//! | 失敗 | 返す変種と文脈 |
//! |------|----------------|
//! | 現行より新しい major | [`DocumentError::UnsupportedVersion`]（`found` = 記録値、`supported` = 現行。要件 6.5） |
//! | 移行先が無い（記録値に一致する `from` の段が無い / 現行 major に届かない） | 同上（`found` = **記録値**） |
//! | ステップ表の欠陥（同じ版から 2 段 / 前進しない段 / 現行 major を飛び越す段） | [`DocumentError::InvalidContainer`]（`entry` = `migration: <理由>`。呼び出し元の programming error） |
//!
//! 移行後の集合の照合・検証の失敗（[`DocumentError::IntegrityMismatch`] / 構造検証の各変種）は
//! 読み込み経路が報告する（本モジュールは表現の書き換えと記帳だけを行う）。

use core::fmt;

use crate::error::DocumentError;
use crate::parts::DocumentParts;

use steps::MigrationStep;

pub mod steps;

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
/// この型の存在理由である（[`Self::Openable`] は移行せず、[`Self::NeedsMigration`] だけが
/// 移行チェーンの適用へ進む。モジュール docs「段階的移行チェーン」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionVerdict {
    /// 現行と同じ major である。**minor が現行より新しくても**そのまま読める。
    Openable,
    /// 現行より古い major である。移行チェーンの適用が必要（要件 6.2 / 6.3）。
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

/// 形式バージョンのゲートと、段階的移行チェーンの適用（design「MigrationChain」）。
///
/// 記録値の判定（[`Self::gate`]）と、古い形式を現行 major まで 1 段ずつ運ぶ適用
/// （[`Self::apply`]）を持つ。無状態であり、構築も保持もしない（ステップの宣言は
/// 子モジュール [`steps`] にある）。
pub struct MigrationChain;

impl MigrationChain {
    /// 記録された形式バージョンへ判定を下す（要件 6.5。design「MigrationChain」）。
    ///
    /// 判定は major の比較だけで決まる。minor は見ない
    /// （`1.0` と `1.99` はどちらも [`VersionVerdict::Openable`]。モジュール docs 参照）。
    /// この関数はエラーを返さず、応答（中止 / 移行 / そのまま）は呼び出し側が決める。
    /// **副作用を持たない**ので、開く経路が「移行が適用されるか」を事前に知るためにも使える。
    pub const fn gate(recorded: FormatVersion) -> VersionVerdict {
        if recorded.major > CURRENT_FORMAT_VERSION.major {
            VersionVerdict::Unsupported { found: recorded, supported: CURRENT_FORMAT_VERSION }
        } else if recorded.major == CURRENT_FORMAT_VERSION.major {
            VersionVerdict::Openable
        } else {
            VersionVerdict::NeedsMigration { from: recorded }
        }
    }

    /// 記録された形式バージョンに応じて、段階的移行チェーンを適用する（要件 6.2, 6.3）。
    ///
    /// 戻り値は「移行した集合」である:
    ///
    /// - 現行と同じ major（[`VersionVerdict::Openable`]）: **`Ok(None)`**。移行は不要であり、
    ///   集合は複製もしない（通常経路の性能に影響を与えない。要件 8.1）。
    /// - 現行より新しい major（[`VersionVerdict::Unsupported`]）: 中止
    ///   （[`DocumentError::UnsupportedVersion`]。要件 6.5）。
    /// - 現行より古い major（[`VersionVerdict::NeedsMigration`]）: 記録値から現行 major まで
    ///   1 段ずつ適用した集合を `Ok(Some(..))` で返す。記録値と一致する `from` を持つ段が無い
    ///   場合（実表が空である初版を含む）や現行 major に届かない場合は、移行先が無いため中止
    ///   する（[`DocumentError::UnsupportedVersion`] の `found` は**記録値**、`supported` は現行）。
    ///
    /// 適用の規律（段の選択・連続性の検証・終端・記帳）はモジュール docs
    /// 「段階的移行チェーン」。集合は移行する場合だけ組み立て直す（[`Self::apply_with`] は
    /// [`steps::STEPS`] を使う）。
    pub fn apply(parts: &DocumentParts) -> Result<Option<DocumentParts>, DocumentError> {
        Self::apply_with(steps::STEPS, parts)
    }

    /// ステップ表を明示して [`Self::apply`] と同じ判定・適用を行う（クレート可視）。
    ///
    /// 表を差し替えて段階適用の経路を試すための入口である。**公開面には出さない**: 表は
    /// [`steps::STEPS`] が唯一であり、呼び出し元が任意の表を注入できる経路をクレート外へ
    /// 作らない（[`crate::parts::from_parts`] は [`steps::STEPS`] を渡す）。
    pub(crate) fn apply_with(
        steps: &[MigrationStep],
        parts: &DocumentParts,
    ) -> Result<Option<DocumentParts>, DocumentError> {
        match Self::gate(parts.format_version()) {
            VersionVerdict::Openable => Ok(None),
            VersionVerdict::Unsupported { found, supported } => {
                Err(DocumentError::UnsupportedVersion { found, supported })
            }
            VersionVerdict::NeedsMigration { from } => {
                Ok(Some(Self::migrate(steps, parts, from)?))
            }
        }
    }

    /// `recorded` から現行 major まで、隣接する版の間の段を 1 段ずつ適用する。
    ///
    /// 1 段目だけは借用元（読み込み経路が持つ集合）から複製を作り、以降は前段の結果を
    /// 引き継ぐ。各段の適用後、索引の版とダイジェストを記帳する
    /// （[`DocumentParts::reindex`]。モジュール docs「段階的移行チェーン」の 4）。
    fn migrate(
        steps: &[MigrationStep],
        parts: &DocumentParts,
        recorded: FormatVersion,
    ) -> Result<DocumentParts, DocumentError> {
        let plan = Self::plan(steps, recorded)?;
        // `gate` が `NeedsMigration`（現行より古い major）と判定した集合は必ず 1 段以上を持つ
        // （`plan` は現行 major に到達しない計画を空のまま返さない）。到達しない経路だが
        // `panic` を置かず、「移行先が無い」と同じ中止で報告する。
        let (first, rest) = plan.split_first().ok_or(DocumentError::UnsupportedVersion {
            found: recorded,
            supported: CURRENT_FORMAT_VERSION,
        })?;
        let mut migrated = first.apply(parts)?.reindex(first.to())?;
        for step in rest {
            migrated = step.apply(&migrated)?.reindex(step.to())?;
        }
        Ok(migrated)
    }

    /// `recorded` から現行 major までの段の列（計画）を組み立てる（連続性の検証つき）。
    ///
    /// 検証は 5 つである: (1) 各版から出る段が**ちょうど 1 つ**あること（無ければ移行先が
    /// 無い＝中止、2 つ以上なら鎖が一意に定まらない＝表の欠陥）、(2) 各段が版を**前進**させる
    /// こと、(3) 隣接していること（直前の `to` が次の `from` に完全一致する。`cursor` との
    /// 一致で段を選ぶため構造的に保証される）、(4) 段が**現行 major を飛び越さない**こと
    /// （飛び越した先の版を記録した集合は、ゲートが読めない版になる）、(5) 終端が**現行と同じ
    /// major** に到達すること（minor の一致は要求しない。理由はモジュール docs
    /// 「段階的移行チェーン」の 3）。
    ///
    /// 検証はすべて適用の前に行うため、欠陥のある表では 1 段も適用されない。
    ///
    /// **無限ループしない**: 各反復で `cursor` は厳密に前進するので（(2) の検証は段を選んだ
    /// 直後に行う）、表が有限である限り同じ段が 2 度選ばれることはない。このループは高々
    /// `steps.len() + 1` 回で終わる。
    fn plan(
        steps: &[MigrationStep],
        recorded: FormatVersion,
    ) -> Result<Vec<&MigrationStep>, DocumentError> {
        let mut plan = Vec::new();
        let mut cursor = recorded;
        while cursor.major != CURRENT_FORMAT_VERSION.major {
            let mut candidates = steps.iter().filter(|step| step.from() == cursor);
            let step = candidates.next().ok_or(DocumentError::UnsupportedVersion {
                found: recorded,
                supported: CURRENT_FORMAT_VERSION,
            })?;
            if candidates.next().is_some() {
                return Err(malformed(format!(
                    "two migration steps start at format version {cursor}"
                )));
            }
            if step.to() <= step.from() {
                return Err(malformed(format!(
                    "the migration step {cursor} -> {} does not advance the format version",
                    step.to()
                )));
            }
            if step.to().major > CURRENT_FORMAT_VERSION.major {
                return Err(malformed(format!(
                    "the migration step {cursor} -> {} lands beyond the current major",
                    step.to()
                )));
            }
            plan.push(step);
            cursor = step.to();
        }
        Ok(plan)
    }
}

/// ステップ表の欠陥（呼び出し元の programming error）を報告する。
///
/// design のエラー表に固有の変種が無いため、既存規約どおり
/// [`DocumentError::InvalidContainer`] の `entry` に「失敗箇所のラベル + 理由」を載せる
/// （[`crate::parts::document_parts`] の行符号化エラーと同じ扱い）。
fn malformed(reason: impl fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("migration: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry_name::{EntryName, MANIFEST_ENTRY};
    use crate::integrity::verify_part;
    use crate::model::Document;
    use crate::parts::document_part::DocumentPart;
    use crate::parts::manifest::ManifestPart;
    use crate::parts::to_parts;

    use steps::synthetic;

    /// 標本の集合（0 行 2 シート。移行の観測はシート名へ積まれる印で行う）。
    fn sample_parts() -> DocumentParts {
        let mut document = Document::new();
        document.add_sheet("在庫");
        document.add_sheet("空");
        to_parts(&document).expect("保存経路")
    }

    /// 集合の `document.json` のシート名（適用順序の観測点）。
    fn sheet_names(parts: &DocumentParts) -> Vec<String> {
        let entry = parts.get(&EntryName::Document).expect("集合は document.json を持つ");
        let decoded = DocumentPart::from_json_bytes(&entry.bytes).expect("標本は復号できる");
        decoded.sheets().iter().map(|meta| meta.name().to_owned()).collect()
    }

    /// 集合の 1 パートの実バイト列（観測用）。
    fn part_bytes(parts: &DocumentParts, name: EntryName) -> Vec<u8> {
        parts.get(&name).expect("標本のパートは実在する").bytes.clone()
    }

    /// 集合の索引を復号する（観測用）。
    fn manifest_of(parts: &DocumentParts) -> ManifestPart {
        ManifestPart::from_json_bytes(&part_bytes(parts, MANIFEST_ENTRY))
            .expect("標本の索引は復号できる")
    }

    /// 合成表へ通した結果が「移行先が無い」中止であることを確かめる。
    fn assert_no_target(
        result: Result<Option<DocumentParts>, DocumentError>,
        recorded: FormatVersion,
    ) {
        match result {
            Err(DocumentError::UnsupportedVersion { found, supported }) => {
                assert_eq!(recorded, found, "移行前の版が報告されていない");
                assert_eq!(CURRENT_FORMAT_VERSION, supported, "対応バージョンが報告されていない");
            }
            other => panic!("移行先が無い集合が中止されない: {other:?}"),
        }
    }

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

    /// 現行より古い major は「移行が必要」と判定される（適用は [`MigrationChain::apply`]）。
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

    /// 読み込み経路向けの入口: 現行と同じ major は移行せず（`None`）、新しすぎる major と
    /// 移行先の無い古い major は**要求バージョンつき**で中止する。
    ///
    /// 6.1 は古い major も「移行先が無い版」として同じ中止で報告していた（`admit`）。
    /// **6.2 はその分岐を実チェーン適用に置き換えた**（実表が空である現時点の観測結果は同じ）。
    #[test]
    fn apply_migrates_only_what_the_chain_can_reach() {
        let parts = sample_parts();
        assert!(
            MigrationChain::apply_with(synthetic::MULTI_STEP, &parts)
                .expect("現行版は中止しない")
                .is_none(),
            "現行版の集合が移行された"
        );
        let same_major = synthetic::recorded_at(FormatVersion::new(1, 99), &parts);
        assert!(
            MigrationChain::apply(&same_major).expect("同一 major は中止しない").is_none(),
            "同一 major の集合が移行された"
        );

        // 実表（空）では古い major に移行先が無い（`found` は記録値である）。
        for recorded in [FormatVersion::new(2, 7), FormatVersion::new(0, 9)] {
            let parts = synthetic::recorded_at(recorded, &sample_parts());
            match MigrationChain::apply(&parts) {
                Err(DocumentError::UnsupportedVersion { found, supported }) => {
                    assert_eq!(recorded, found, "要求バージョンが報告されていない");
                    assert_eq!(CURRENT_FORMAT_VERSION, supported, "対応バージョンが報告されていない");
                }
                other => panic!("{recorded} が中止されない: {other:?}"),
            }
        }
    }

    /// 実表は空である（初版は v1 のみ。design「MigrationChain / Implementation Notes」）。
    /// ステップを足す作法は [`steps`] のモジュール docs にある。
    #[test]
    fn the_step_table_is_empty_at_v1() {
        assert!(
            steps::STEPS.is_empty(),
            "v1 のみのはずがステップが登録されている: {} 段",
            steps::STEPS.len()
        );
    }

    /// 要件 6.2 / 6.3: 現行より **2 段前**の版から現行版へ、隣接する段が**順に**適用される。
    /// 各段は前の段の結果を受け取る（2 段目は 1 段目の印を前提にする）ので、印は
    /// 段の順に積まれる。1 段しか適用しない実装は最終形にならず、版も現行版にならない。
    #[test]
    fn a_multi_step_chain_is_applied_in_order() {
        let parts = synthetic::recorded_at(synthetic::OLDEST, &sample_parts());
        let migrated = MigrationChain::apply_with(synthetic::MULTI_STEP, &parts)
            .expect("合成表は妥当")
            .expect("古い版は移行される");

        assert_eq!(CURRENT_FORMAT_VERSION, migrated.format_version(), "移行後の版が現行版でない");
        assert_eq!(
            vec!["在庫|v0.1|v1.0".to_owned(), "空|v0.1|v1.0".to_owned()],
            sheet_names(&migrated),
            "段の適用順序または適用段数が違う（印の積み方で観測する）"
        );
    }

    /// 段の適用順は**鎖の順**（`from` の一致で引いた順）であり、表の宣言順ではない。
    /// 宣言順が逆の表でも同じ結果になる（表の並びを実行順と取り違える実装を落とす）。
    #[test]
    fn the_application_order_follows_the_chain_not_the_table_order() {
        let parts = synthetic::recorded_at(synthetic::OLDEST, &sample_parts());
        let migrated = MigrationChain::apply_with(synthetic::REVERSED, &parts)
            .expect("合成表は妥当")
            .expect("古い版は移行される");
        assert_eq!(
            vec!["在庫|v0.1|v1.0".to_owned(), "空|v0.1|v1.0".to_owned()],
            sheet_names(&migrated),
            "表の宣言順で適用されている（鎖の順で適用されていない）"
        );
    }

    /// 移行の記帳: 各段の適用後に `manifest.json` の版がその段の `to` になり、索引の
    /// ダイジェストが実体に一致する（移行後の集合は現行版として一貫している。要件 5.1 / 6.2）。
    #[test]
    fn the_migrated_set_is_consistent_with_its_index_and_version() {
        let parts = synthetic::recorded_at(synthetic::OLDEST, &sample_parts());
        let migrated = MigrationChain::apply_with(synthetic::MULTI_STEP, &parts)
            .expect("合成表は妥当")
            .expect("古い版は移行される");

        assert_eq!(CURRENT_FORMAT_VERSION, migrated.format_version(), "移行後の版が現行版でない");
        let manifest = manifest_of(&migrated);
        assert_eq!(CURRENT_FORMAT_VERSION, manifest.version(), "索引の記録値が現行版でない");
        assert_eq!(
            migrated.iter().count() - 1,
            manifest.entries().len(),
            "索引が集合の内容と一致しない"
        );
        for entry in manifest.entries() {
            let part = migrated.get(&entry.name()).expect("索引のエントリは実在する");
            verify_part(&entry.name(), &part.bytes, entry.digest()).unwrap_or_else(|error| {
                panic!("{} のダイジェストが実体と違う: {error}", entry.name())
            });
        }

        // 旧版の索引は書き換え前の `document.json` のダイジェストを記録していた。移行後の
        // 索引がその値のままなら再計算されていない。
        let before = manifest_of(&parts);
        assert_ne!(
            before.digest_of(&EntryName::Document),
            manifest.digest_of(&EntryName::Document),
            "書き換えたパートのダイジェストが再計算されていない"
        );
    }

    /// 連続性の検証: 隙間のある表・現行 major に届かない表・前進しない段・同じ版から 2 段出る
    /// 表を中止する。**無限ループしない**（このテストが返ってくること自体が、前進の検証と
    /// 有限の表による停止の実測である）。
    #[test]
    fn a_broken_step_table_is_rejected() {
        let parts = synthetic::recorded_at(synthetic::OLDEST, &sample_parts());
        // 隙間: 記録値（0.0）から出る段が無い。
        assert_no_target(MigrationChain::apply_with(synthetic::GAP, &parts), synthetic::OLDEST);
        // 現行 major に届かない: 0.1 で止まる（そこから先の段が無い）。
        assert_no_target(MigrationChain::apply_with(synthetic::SHORT, &parts), synthetic::OLDEST);
        // 前進しない段 / 現行 major を飛び越す段 / 同じ版から 2 段: 表の欠陥
        // （呼び出し元の programming error）。
        for table in [synthetic::STALLED, synthetic::OVERSHOOT, synthetic::AMBIGUOUS] {
            match MigrationChain::apply_with(table, &parts) {
                Err(DocumentError::InvalidContainer { entry }) => {
                    assert!(entry.starts_with("migration:"), "entry が違う: {entry}");
                }
                other => panic!("欠陥のある表が拒否されない: {other:?}"),
            }
        }
    }

    /// 計画の終端は**現行と同じ major** であればよい（minor の一致は要求しない）。
    /// 現行 minor が上がるたびに段を書き換える規則を採らないことの固定である。
    #[test]
    fn the_plan_ends_at_the_current_major_not_the_exact_version() {
        let ahead = [MigrationStep::new(
            synthetic::OLDEST,
            FormatVersion::new(1, 9),
            synthetic::stamp_v0_1,
        )];
        let plan = MigrationChain::plan(&ahead, synthetic::OLDEST).expect("現行 major に到達する");
        assert_eq!(1, plan.len(), "計画の段数が違う");
        assert_eq!(FormatVersion::new(1, 9), plan[0].to(), "終端の版が違う");
    }

    /// 現行版の集合は移行されない: `apply` は `None` を返し（複製を作らない）、ステップ表を
    /// **参照もしない**（表が壊れていても現行版は読める）。要件 8.1 の予算に移行の複製と
    /// 再ハッシュを持ち込まないことの構造的な根拠である。
    #[test]
    fn the_current_version_is_neither_migrated_nor_copied() {
        let parts = sample_parts();
        assert_eq!(CURRENT_FORMAT_VERSION, parts.format_version(), "標本が現行版でない");
        assert!(
            MigrationChain::apply_with(synthetic::MULTI_STEP, &parts)
                .expect("現行版は中止しない")
                .is_none(),
            "現行版の集合が移行（複製）された"
        );
        // 表の欠陥は現行版の読み込みに影響しない（判定は gate が行い、表を参照しない）。
        assert!(
            MigrationChain::apply_with(synthetic::STALLED, &parts)
                .expect("現行版は中止しない")
                .is_none(),
            "現行版が移行された"
        );
    }

    /// 要件 6.2 / 6.3: 未知フィールドは移行チェーンを通っても保持される。`document.json` は
    /// **表現を書き換えた段**を通り、`manifest.json` は**記帳**を通る（どちらの経路も
    /// 未知フィールドを落としてはならない）。
    #[test]
    fn unknown_fields_survive_the_migration_chain() {
        let parts = synthetic::with_unknown_fields(synthetic::OLDEST, &sample_parts());
        let migrated = MigrationChain::apply_with(synthetic::MULTI_STEP, &parts)
            .expect("合成表は妥当")
            .expect("古い版は移行される");

        let document =
            String::from_utf8(part_bytes(&migrated, EntryName::Document)).expect("確定形は UTF-8");
        assert!(
            document.contains(r#""future_top":{"unit":"mm"}"#),
            "document.json のトップレベルの未知フィールドが移行で消えた: {document}"
        );
        assert!(
            document.contains(r#""element_note":7"#),
            "シート要素の未知フィールドが移行で消えた: {document}"
        );

        let manifest =
            String::from_utf8(part_bytes(&migrated, MANIFEST_ENTRY)).expect("確定形は UTF-8");
        assert!(
            manifest.contains(r#""future_manifest":true"#),
            "索引のトップレベルの未知フィールドが記帳で消えた: {manifest}"
        );
        assert!(
            manifest.contains(r#""entry_note":9"#),
            "索引要素の未知フィールドが記帳で消えた: {manifest}"
        );

        // 未知フィールドが既知の値を隠していない（移行後の集合は現行版として一貫している）。
        assert_eq!(CURRENT_FORMAT_VERSION, migrated.format_version(), "移行後の版が現行版でない");
        assert_eq!(
            vec!["在庫|v0.1|v1.0".to_owned(), "空|v0.1|v1.0".to_owned()],
            sheet_names(&migrated),
            "未知フィールドの保持で表現の書き換えが壊れている"
        );
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

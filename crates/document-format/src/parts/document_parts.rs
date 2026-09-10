//! 論理エントリ集合（`DocumentParts`）の組み立てと分解（タスク 4.8。要件 2.2, 2.3, 3.1,
//! 5.1〜5.4。design「Parts Layer / DocumentParts」の Service 契約）。
//!
//! 本モジュールが本スペックの**対外的な境界**である: ドキュメントを論理エントリ（パート）
//! の集合として取り出し、その集合からドキュメントモデルを復元する双方向変換を持つ。
//! `version-control` が git へ置く単位はこのパート集合であり（design
//! `DocumentFormatApi` の Implementation Notes「`to_parts` / `from_parts` が
//! `version-control` との唯一の接点である」）、ZIP コンテナ（タスク 5.x）はこの集合の
//! 符号化先にすぎない。
//!
//! | 方向 | 入力 | 出力 |
//! |------|------|------|
//! | [`to_parts`] | [`Document`]（モデル） | [`DocumentParts`]（エントリ名の昇順の決定的な集合） |
//! | [`from_parts`] | [`DocumentParts`] | [`Document`]（全検証を通過したものだけ） |
//!
//! # エントリ構成（design「Container Entry Layout」）
//!
//! | エントリ名 | 内容 | 担当 |
//! |------------|------|------|
//! | `manifest.json` | 形式バージョン + パート索引（エントリ名 → BLAKE3 ダイジェスト） | [`ManifestPart`]（タスク 4.2） |
//! | `document.json` | ドキュメント識別子・シート順序・シートのメタデータ（列名を含む） | [`DocumentPart`]（タスク 4.3 / 4.8） |
//! | `schemas/<sheet-ulid>.json` | ルートスキーマ + ネスト型定義（不透明ペイロード） | [`SchemaCodec`]（タスク 4.4） |
//! | `sheets/<sheet-ulid>.jsonl` | 行データ（1 行 1 オブジェクトの NDJSON） | [`RowsCodec`]（タスク 4.5） |
//! | `attachments/<hex64>.bin` | 添付（content-addressed 命名、内容は不透明） | 本モジュール（バイト列をそのまま運ぶ） |
//!
//! **全シートが schema エントリと rows エントリを持つ**（0 行のシートも空の rows エントリを
//! 持つ）。エントリの形を一様にすることで、索引（`manifest.json`）がシートの有無に
//! 依存しない完全な目録になり、読み側の分岐も減る。0 行のシートの列名は rows エントリから
//! 復元できない（列順を書く行が無い）ため、`document.json` の `columns` が唯一の永続先である
//! （[`SheetMeta`] の docs）。
//!
//! ## 型マーカー `jxcel` はこの集合に含めない
//!
//! `jxcel` は**コンテナ層のエントリ**である（`Stored`・先頭固定オフセットでの型判定用
//! マーカー。design「Container Entry Layout」/「ContainerCodec」）。`DocumentParts` は
//! 文書の論理的な内容（メタデータ・スキーマ・行・添付）だけを持ち、マーカーを持たない:
//! 索引順に並べた集合へ `jxcel` を混ぜると、design が要求する「先頭に置く」と
//! 「`DocumentParts` のソート済み順序で書く」が両立しなくなる。したがって
//! [`DocumentParts`] の構築は `jxcel` を拒否し（受理して黙って落とすと
//! `ContainerCodec::decode(encode(p)) == p` が崩れる）、コンテナ層（タスク 5.2 / 5.3）が
//! マーカーを自分で先頭へ置き、復号時に集合から外す。**タスク 5.2 / 5.3 への申し送り。**
//!
//! # 順序（決定的反復）
//!
//! [`DocumentParts`] はエントリ名の**昇順**（[`EntryName`] の `Ord` = 表示テキストの
//! 辞書順）で保持し、[`DocumentParts::iter`] は常にその順で反復する。順序は集合の内容
//! だけで決まり、**入力の並び・構築順・ファイルシステムの列挙順に依存しない**
//! （要件 2.2, 3.1, 3.6）。本モジュールは `std::fs` も `zip` も参照しない（コンテナ層の
//! 責務は 5.x）。
//!
//! 索引である `manifest.json` の並び（昇順）と、**データであるシート順序**
//! （`document.json` の配列順。要件 1.1）は規則が逆であることに注意（`parts/mod.rs` の
//! モジュール docs「パートごとの順序規則」）。
//!
//! # 決定性（要件 3.1, 3.6）
//!
//! `to_parts` は入力の**純関数**である: 同じモデルからは常に同じパート集合（同じ名前・
//! 同じバイト列）になる。保存時刻・実行環境・内部処理順序に由来する値は一切含めない
//! （添付の反復は content-addressed 識別子の昇順に決まる。最終的な集合の並びは
//! エントリ名の昇順である）。
//!
//! # ダイジェストは integrity 層の 1 経路で算出する
//!
//! 各パートのダイジェストは [`crate::integrity::digest_part`]（タスク 4.1）で算出し、
//! BLAKE3 を直接呼ばない。design の `IntegrityVerifier` が持つ**集合版**
//! （`digest_parts` / `verify_parts`）は本モジュールが担う: [`to_parts`] が索引へ記録する
//! 値を算出し、[`from_parts`] が [`crate::integrity::verify_part`] で照合する
//! （`integrity.rs` は単一パートのプリミティブだけを持ち、集合型を先取りしない）。
//!
//! # 未知フィールドの往復（前方互換。要件 6.2 / 6.3）
//!
//! `document.json` のトップレベルとシート要素で保持した未知キーは、復号 → モデル →
//! 再符号化の経路でも失われない。**モデルが運ぶ**ためである: [`Document`] が
//! トップレベルの保持内容を、`Sheet` がシート要素ごとの保持内容を持つ（タスク 4.8 の
//! 親の裁定）。`from_parts` が復号済みの保持内容をモデルへ移し、`to_parts` が
//! `document.json` の同じ位置へ差し戻す。`manifest.json` / `schemas/*` の未知フィールドは
//! それぞれのパート型（[`ManifestPart`] / [`SchemaPart`]）が保持しており、本層はそれらを
//! そのまま往復させる。
//!
//! # 列名の検証
//!
//! 行データの wire 形式は列名をキーとするため、行エントリ自身が列順を持つ。`document.json`
//! の `columns` は**書き出し時の権威**であり、read 側は両者の一致を検証する:
//! 行を持つエントリでは列名の並び（順序も集合も）が一致しなければ
//! [`DocumentError::InvalidContainer`] で中止する。0 行のエントリは列順を表現できないため
//! 検査しない（`document.json` が唯一の源である）。
//!
//! **列名の重複**は本層の検査対象ではない（列名は識別子ではなく、本クレートは中身を
//! 解釈しない）。行を持つエントリでは行データの wire 形式が同名キーを拒否し
//! （[`RowsCodec::decode`]）、保存側は [`RowsCodec::encode`] が全シートについて拒否する
//! （列名が重複した 0 行のシートは保存できない）。読み込みがそれを受け入れる余地は
//! 「保存できないモデルを渡す」という非対称として残る（列名の意味論を持つ
//! `schema-engine` 側で解消するのが筋であり、本クレートへ第二の列名規則を持ち込まない）。
//!
//! 本クレートは**列名の中身を解釈しない**（どの列がどの型かは `schema-engine` が決める。
//! design「スキーマ・ペイロードの不透明性」）。
//!
//! # 読み込みの処理順（要件 5.4: 部分的なモデルを返さない）
//!
//! [`from_parts`] は design「読み込みフロー」の順で処理し、**すべての検証をモデル構築の
//! 前に完了させる**:
//!
//! 1. 索引の解決（[`resolve_manifest`]。不在は [`DocumentError::MissingPart`]）
//! 2. 形式バージョンのゲートと、古い形式への段階的移行（要件 6.2, 6.3, 6.5）:
//!    索引が記録した版を [`MigrationChain::gate`] で判定し、新しすぎる major は
//!    [`DocumentError::UnsupportedVersion`] で中止する。古い major は移行チェーン
//!    （[`MigrationChain::apply`]）を適用する。**移行する場合だけ**集合を所有して組み立て直し、
//!    移行先が無ければ同じ [`DocumentError::UnsupportedVersion`] で中止する
//! 3. 完全性の照合（要件 5.2 / 5.3）: 索引の全エントリについて、実体の存在
//!    （無ければ [`DocumentError::MissingPart`]）とダイジェストの一致
//!    （不一致は [`DocumentError::IntegrityMismatch`]）を確かめる。**移行する場合は、移行の
//!    前に記録されたままの集合を照合し、移行後にもう一度この照合を通す**（下記
//!    「移行とダイジェスト照合の順序」）
//! 4. 各パートの復号（[`DocumentPart`] / [`SchemaCodec`] / [`RowsCodec`]。添付は
//!    バイト列のまま。content-addressed の再計算照合もここで行う）
//! 5. 構造検証（[`StructuralValidator`]。識別子の一意性・スキーマの存在・参照の実在性）
//! 6. 行エントリの列順序が `document.json` の列名一覧と一致することの検証
//! 7. モデル構築（[`Document`] の識別子・シート順序・名前・列名・ルートスキーマ・行・添付）
//!
//! 1〜6 のいずれかで失敗した場合、モデルは 1 つも構築されず [`Err`] が返る。7 の構築は
//! 検証済みの内容だけを移す（行は行ごとの探索をしない一括経路で入れる。要件 8.1）。
//!
//! ## 移行とダイジェスト照合の順序（タスク 6.2 の親の裁定）
//!
//! design「読み込みフロー」は「バージョンゲート → 移行 → ダイジェスト照合」と書いているが、
//! **この順序のままでは古い形式のファイルの破損を検出できない**: 移行が索引を無条件に
//! 作り直すため、壊れた古いファイルが黙って「修復」されて読まれてしまう（要件 5.2 は古い
//! 形式のファイルにも適用される）。したがって本経路は
//! **「ゲート →（移行する場合だけ）記録どおりの照合 → 移行 → 移行後の照合・構造検証」**
//! の順で運ぶ。移行後の集合は 3 以降の既存の 1 経路で検証される（検証ロジックを 2 箇所に
//! 持たない）。同じ裁定と理由は [`crate::migration`] のモジュール docs
//! 「移行とダイジェスト照合の順序」にも書いてある。
//!
//! 移行は**メモリ上**で完結する: 本モジュールは `std::fs` に触れず、書き込みも読み込みも
//! しない（ファイル I/O はタスク 7.1 / 7.2 の責務）。
//!
//! ## 保存側の不変条件検証（[`validate_document`]）
//!
//! 保存経路（[`crate::DocumentFormatApi::save`]）は書き込みの前に [`validate_document`] を
//! 呼ぶ。これは **`from_parts` の処理順 5 と同じ検証器（[`StructuralValidator`]）を、モデル
//! から組み立てた目録へ 1 回掛けるだけ**であり、規則も報告文面も二重化しない
//! （design「`open` と `from_parts` は同一の検証経路を通る」）。モデル API が構築で強制する
//! 不変条件（各シートのルートスキーマ、改名・並べ替えでの識別子の安定）はここで検査せず、
//! **構築では強制されない**不変条件（スキーマのペイロード内の `TypeDefId` の重複宣言、
//! 型定義参照の実在、添付参照の実在）を遮断する。これにより `save` は自分自身の `open` が
//! 拒否するファイルを書き出さない。
//!
//! ## 索引の向きと、検査しないこと
//!
//! 照合の向きは**索引 → 実体**である（索引に載った各パートが実在し、内容が記録どおりか）。
//! 実体があって索引に載っていないパートは本層では拒否しない: 索引の完全性
//! （実体 → 索引）は保存経路 [`to_parts`] が構造的に保証しており（全パートを索引へ載せる）、
//! 許可リストの適用はコンテナ層（要件 2.5。タスク 5.3）の責務である。索引の監査を
//! 読み込み経路へ足す場合は本判断を見直すこと（**タスク 5.3 / 6.x への申し送り**）。
//!
//! # 形式バージョン
//!
//! `to_parts` は [`CURRENT_FORMAT_VERSION`]（現行 1.0。定義の唯一の所有者は
//! [`crate::migration`]）を `manifest.json` へ記録する（要件 6.1）。
//! [`DocumentParts::format_version`] はパート集合が記録しているバージョンを**そのまま**返す
//! （ゲートを掛けない。報告と読み込みの可否は別の関心事であり、コンテナ層の型マーカーは
//! この値を写す）。`from_parts` は読む前段でこの値を [`MigrationChain::gate`] に掛け、
//! 読めない版を中止し、古い版は移行チェーン（[`MigrationChain::apply`]）で現行版へ運ぶ
//! （要件 6.2, 6.3, 6.5。処理順 2）。
//!
//! # 依存方向
//!
//! `Ids / Value / EntryName → Model → Json → Parts → Container → Api` の一方向
//! （design「Architecture Integration」）。本モジュールは `parts` 層の各パート符号化と
//! [`crate::integrity`] / [`crate::model`] に依存し、`container` には依存しない
//! （ZIP を一切知らない）。`serde_json` の汎用値型・`HashMap` / `HashSet` も使わない
//! （決定性の根拠。`crate::json` と同じ禁止理由）。
//!
//! # エラー対応
//!
//! 読み込みの失敗はすべて読み込み全体の中止であり、部分的なモデルを返さない（要件 5.4）。
//! 新しい [`DocumentError`] 変種は足さない（design のエラー表は閉じている）。型付き変種が
//! 無い構造的矛盾は [`DocumentError::InvalidContainer`] の `entry` に「失敗箇所のエントリ名
//! + 理由」を載せる（各パートモジュールと同じ規約）。
//!
//! | 失敗 | 返す変種と文脈 |
//! |------|----------------|
//! | 索引（`manifest.json`）が無い | [`DocumentError::MissingPart`]（`name` = `manifest.json`） |
//! | 索引が復号できない | [`DocumentError::InvalidContainer`]（`entry` = `manifest.json: <理由>`） |
//! | 記録された形式バージョンが読めない | [`DocumentError::UnsupportedVersion`]（`found` / `supported`。新しすぎる major と、移行先が無い古い major。要件 6.2, 6.3, 6.5） |
//! | 移行のステップ表が欠陥を持つ（同じ版から 2 段 / 前進しない段 / 現行 major を飛び越す段） | [`DocumentError::InvalidContainer`]（`entry` = `migration: <理由>`。呼び出し元の programming error） |
//! | 索引に載ったパートが実在しない | [`DocumentError::MissingPart`]（`name` = そのエントリ名） |
//! | 実体のダイジェストが索引の記録と一致しない | [`DocumentError::IntegrityMismatch`]（`entry` = そのエントリ名） |
//! | メタデータ（`document.json`）が無い | [`DocumentError::MissingPart`]（`name` = `document.json`） |
//! | パートが復号できない（破損・形違い） | [`DocumentError::InvalidContainer`]（`entry` = そのエントリ名 + 理由） |
//! | 添付の実バイト列が content-addressed 識別子と一致しない | 同上（`entry` = そのエントリ名 + 理由） |
//! | 識別子の重複 / スキーマ欠落 / 宙吊り参照 / 未知シート参照 | 構造検証の各変種（[`StructuralValidator`]）。シート参照破れは [`DocumentError::InvalidContainer`] |
//! | 行エントリの列順が `document.json` の列名一覧と違う | [`DocumentError::InvalidContainer`]（`entry` = その行エントリ名 + 理由） |
//! | 形式マーカー `jxcel` が集合に混ざっている | 同上（`entry` = `jxcel` + 理由） |
//! | 保存側: 行の列数と値の個数が合わない / 列名が重複している | [`DocumentError::InvalidContainer`]（`entry` = その行エントリ名 + 理由。呼び出し元の programming error） |

use std::fmt;

use crate::entry_name::{EntryName, MANIFEST_ENTRY};
use crate::error::{DocumentError, IdKind};
use crate::ids::{AttachmentId, Blake3Digest, SheetId};
use crate::integrity::{digest_part, verify_part};
use crate::migration::steps::{MigrationStep, STEPS};
use crate::migration::{FormatVersion, MigrationChain, VersionVerdict, CURRENT_FORMAT_VERSION};
use crate::model::{Document, Row, SchemaPart, Sheet};
use crate::parts::document_part::{DocumentPart, SheetMeta};
use crate::parts::manifest::{resolve_manifest, ManifestEntry, ManifestPart};
use crate::parts::rows_codec::{RowsCodec, RowsEncodeError, SheetRows};
use crate::parts::schema_codec::SchemaCodec;
use crate::parts::validate::{
    IdDeclaration, PartInventory, SheetRefDeclaration, StructuralValidator, TypeRefDeclaration,
};

/// 論理エントリ 1 件（design「DocumentParts」の Service Interface）。
///
/// エントリ名・実バイト列・ダイジェストの 3 つを持つ。`digest` は `bytes` から
/// [`crate::integrity::digest_part`] が算出した値そのものであり、この不変条件は
/// [`DocumentParts`] の構築経路だけが `Part` を組み立てることで保たれる
/// （外部から与えられたダイジェストを信用する経路は無い）。
///
/// `Clone` は提供しない: パート集合は借用して読む（[`DocumentParts::iter`] /
/// [`DocumentParts::get`]）か、必要なら [`crate::parts::to_parts`] で組み立て直す。
#[derive(Debug)]
pub struct Part {
    /// エントリ名（許可リストの 6 形のいずれか。コンテナ内の相対パスそのもの）。
    pub name: EntryName,
    /// エントリの実バイト列（**展開後・非圧縮**の内容。ZIP の知識を持たない）。
    pub bytes: Vec<u8>,
    /// `bytes` から算出した BLAKE3 ダイジェスト（索引へ記録する値と同じ）。
    pub digest: Blake3Digest,
}

/// エントリ名でソートされた決定的な集合（design「DocumentParts」の Service Interface）。
///
/// フィールドは非公開であり、構築は [`DocumentParts::from_entries`]（実バイト列の列から。
/// コンテナ復号経路が使う）と [`to_parts`]（モデルから）の 2 経路だけである。どちらも
/// **エントリ名の昇順への整列**・**重複エントリ名の拒否**・**形式マーカー `jxcel` の拒否**
/// という同じ正準化を通り（[`DocumentParts::canonicalize`]）、さらに `manifest.json` を
/// 必ず含む（[`DocumentParts::from_entries`] が索引を解決して形式バージョンを読む。
/// したがって [`DocumentParts::format_version`] は失敗しない）。
///
/// ```text
/// to_parts(document) -> DocumentParts -> from_parts -> Document -> to_parts -> 同一の集合
/// ```
///
/// `Clone` は提供しない（[`Part`] と同じ理由。集合は借用して読む）。
#[derive(Debug)]
pub struct DocumentParts {
    /// エントリ名の昇順・重複なし（構築時の正準化が保証する）。
    parts: Vec<Part>,
    /// 索引が記録していた形式バージョン（[`DocumentParts::format_version`]）。
    version: FormatVersion,
}

impl DocumentParts {
    /// 実バイト列の列から集合を組み立てる（エントリ名は重複していてはならない）。
    ///
    /// 各パートのダイジェストは [`crate::integrity::digest_part`] が算出する（呼び出し元の
    /// 自己申告値を受け取らない）。並びはエントリ名の昇順へ整列し、`manifest.json` を
    /// 含まない集合・`jxcel` を含む集合・重複したエントリ名を持つ集合は
    /// [`DocumentError`] として拒否する。形式バージョンは索引から読む
    /// （索引が復号できなければ拒否する）。
    pub fn from_entries(entries: Vec<(EntryName, Vec<u8>)>) -> Result<Self, DocumentError> {
        let parts = entries
            .into_iter()
            .map(|(name, bytes)| {
                let digest = digest_part(&bytes);
                Part { name, bytes, digest }
            })
            .collect();
        let parts = Self::canonicalize(parts)?;
        let version = read_format_version(&parts)?;
        Ok(Self { parts, version })
    }

    /// エントリ名の昇順で反復する（順序は常に同一。要件 2.2, 3.1）。
    pub fn iter(&self) -> impl Iterator<Item = &Part> {
        self.parts.iter()
    }

    /// エントリ名で引く。集合に無ければ `None`。
    ///
    /// 集合はエントリ名の昇順（構築時の不変条件）なので二分探索で引く。
    pub fn get(&self, name: &EntryName) -> Option<&Part> {
        self.parts
            .binary_search_by(|part| part.name.cmp(name))
            .ok()
            .map(|index| &self.parts[index])
    }

    /// パート集合が記録している形式バージョン（要件 6.1。design の Service Interface）。
    ///
    /// 値は索引（`manifest.json`）が持つバージョンであり、構築時に解決されるため失敗しない。
    /// **報告するだけでゲートは掛けない**: 読み込みの可否は [`from_parts`] が
    /// [`MigrationChain::gate`] で判定する（値そのものを観測したい呼び出し元と、読めるか
    /// 否かを知りたい呼び出し元は別である。モジュール docs「形式バージョン」）。
    pub const fn format_version(&self) -> FormatVersion {
        self.version
    }

    /// 形式バージョンと索引を、この集合の実際の内容に合わせて組み直す（移行の記帳。タスク 6.2）。
    ///
    /// 移行チェーンが各段を適用した後に呼ぶ。段は**表現**だけを書き換えればよく、版の記帳と
    /// 索引の整合（索引に載る全パートのダイジェストを実体に一致させること）はここが担う
    /// （[`crate::migration`] のモジュール docs「段階的移行チェーン」の 4）。索引要素と索引
    /// トップレベルで保持した未知フィールドは引き継ぐ（要件 6.2 / 6.3）。
    ///
    /// 集合を**消費する**（所有権を取る）: 移行は稀な経路であり、段が組み立て直した集合を
    /// そのまま使うため、複製も再ハッシュもしない（各パートのダイジェストは構築時に実体から
    /// 算出済みである。書き換わるのは索引の記録値だけである）。クレート可視である
    /// （読み込み経路の外に任意の集合の記帳を許す経路を作らない）。
    pub(crate) fn reindex(mut self, version: FormatVersion) -> Result<Self, DocumentError> {
        let index = self
            .parts
            .binary_search_by(|part| part.name.cmp(&MANIFEST_ENTRY))
            .map_err(|_| DocumentError::MissingPart { name: MANIFEST_ENTRY.to_string() })?;
        let recorded = ManifestPart::from_json_bytes(&self.parts[index].bytes)?;
        let digests = self
            .parts
            .iter()
            .filter(|part| part.name != MANIFEST_ENTRY)
            .map(|part| (part.name, part.digest));
        let bytes = recorded.reindexed(version, digests)?.to_json_bytes()?;
        let digest = digest_part(&bytes);
        self.parts[index] = Part { name: MANIFEST_ENTRY, bytes, digest };
        self.version = version;
        Ok(self)
    }

    /// 集合の正準化（昇順への整列・重複拒否・形式マーカーの拒否）。両構築経路の共通の門。
    fn canonicalize(mut parts: Vec<Part>) -> Result<Vec<Part>, DocumentError> {
        // 形式マーカーはコンテナ層のエントリであり、論理エントリ集合の一部ではない
        // （モジュール docs「型マーカー `jxcel` はこの集合に含めない」）。黙って落とすと
        // 復号 → 符号化で集合が変わってしまうため、明示的に拒否する。
        if let Some(marker) = parts.iter().find(|part| part.name == EntryName::Marker) {
            return Err(invalid(
                &marker.name,
                "the type marker is a container entry, not a document part",
            ));
        }

        // 入力順は捨て、エントリ名の昇順へ固定する（design「DocumentParts」の決定的順序）。
        parts.sort_by(|left, right| left.name.cmp(&right.name));

        if let Some(pair) = parts.windows(2).find(|pair| pair[0].name == pair[1].name) {
            return Err(invalid(&pair[1].name, "duplicate part entry"));
        }

        Ok(parts)
    }
}

/// モデルからパート集合を構築する（design `DocumentFormatApi::to_parts` の実体）。
///
/// 構築されるエントリは `document.json` / `schemas/<sheet-ulid>.json` /
/// `sheets/<sheet-ulid>.jsonl`（**全シート分**。0 行のシートも空の行エントリを持つ）/
/// `attachments/<hex64>.bin`（登録済みの全添付。参照の有無を問わない。要件 7.6）と、
/// それら全部を索引として載せた `manifest.json` である（モジュール docs「エントリ構成」）。
///
/// 行エントリの列名はシートが保持する列名（[`Sheet::columns`]）をそのまま使う
/// （本クレートはスキーマを解釈しないため、列順の供給はモデル側の責務である）。
/// 添付のバイト列は解釈・再圧縮せずそのまま運ぶ（要件 7.5）。
///
/// 失敗は保存の中止であり、部分的な集合を返さない。行の値の個数と列数の不一致、
/// 列名の重複のような**呼び出し元の programming error** は、design のエラー表に変種が
/// 無いため [`DocumentError::InvalidContainer`] の `entry` にエントリ名と理由を載せる。
pub fn to_parts(document: &Document) -> Result<DocumentParts, DocumentError> {
    // 索引に載せるパート（manifest.json 以外のすべて）。
    let mut entries: Vec<(EntryName, Vec<u8>)> = Vec::new();

    // document.json: シート順序そのまま（並べ替えない。要件 1.1）＋ 列名 ＋ 保持フィールド。
    let mut metas = Vec::with_capacity(document.sheets().len());
    for sheet in document.sheets() {
        metas.push(
            SheetMeta::new(sheet.id(), sheet.name().to_owned())
                .with_columns(sheet.columns().to_vec())
                .with_preserved(sheet.preserved_fields().clone()),
        );
    }
    let document_part = DocumentPart::new(document.document_id(), metas)?
        .with_preserved(document.preserved_fields().clone());
    entries.push((EntryName::Document, document_part.to_json_bytes()?));

    // 全シートが schema エントリと rows エントリを持つ（一様な形）。
    for sheet in document.sheets() {
        entries.push(SchemaCodec::encode(sheet.id(), sheet.root_schema())?);
        let rows = RowsCodec::encode(sheet.id(), sheet.columns(), sheet.rows())
            .map_err(|error| rows_encode_error(sheet.id(), error))?;
        entries.push(rows);
    }

    // 添付は登録順ではなく識別子の昇順に反復される（content-addressed。要件 3.6）。
    for attachment in document.attachments().iter() {
        entries.push((
            EntryName::Attachment { attachment: attachment.id() },
            attachment.bytes().to_vec(),
        ));
    }

    // manifest.json: 他の**全**パートの索引（自分自身は載せられない。タスク 4.2）。
    let mut index = Vec::with_capacity(entries.len());
    let mut parts = Vec::with_capacity(entries.len() + 1);
    for (name, bytes) in entries {
        let digest = digest_part(&bytes);
        index.push(ManifestEntry::new(name, digest));
        parts.push(Part { name, bytes, digest });
    }
    let manifest = ManifestPart::new(CURRENT_FORMAT_VERSION, index)?.to_json_bytes()?;
    let digest = digest_part(&manifest);
    parts.push(Part { name: MANIFEST_ENTRY, bytes: manifest, digest });

    let parts = DocumentParts::canonicalize(parts)?;
    Ok(DocumentParts { parts, version: CURRENT_FORMAT_VERSION })
}

/// パート集合からドキュメントモデルを復元する（design `DocumentFormatApi::from_parts` の実体）。
///
/// 処理順と検証の範囲はモジュール docs「読み込みの処理順」にある。検証はすべてモデル構築の
/// 前に完了し、失敗した場合は部分的なモデルを返さない（要件 5.4）。`open`（タスク 7.1）も
/// この経路と同じ検証を通る（検証ロジックを 2 箇所に持たない）。実表 [`STEPS`] を使う。
pub fn from_parts(parts: &DocumentParts) -> Result<Document, DocumentError> {
    from_parts_with(STEPS, parts)
}

/// 移行のステップ表を明示して [`from_parts`] と同じ経路を運ぶ（モジュール内の入口）。
///
/// 表を差し替えて段階適用の経路を試すための入口である。**公開面には出さない**（読み込みの
/// 経路は [`STEPS`] を使い、モジュールのテストだけが合成表を渡す）。
fn from_parts_with(
    steps: &[MigrationStep],
    parts: &DocumentParts,
) -> Result<Document, DocumentError> {
    // 1. 索引の解決（design 読み込みフローの「manifest が存在」）。
    let recorded_manifest = decode_manifest(parts)?;

    // 2. 形式バージョンのゲートと、古い形式への段階的移行（要件 6.2, 6.3, 6.5）。
    //
    //    移行する場合は、**移行の前に**記録されたままの集合を索引と照合する（要件 5.2）:
    //    移行が索引を無条件に作り直すと、破損した古いファイルが黙って「修復」されてしまう
    //    （モジュール docs「移行とダイジェスト照合の順序」）。移行が不要な集合では照合を
    //    増やさない（通常経路の照合は下の 3 の 1 回だけである）。
    if let VersionVerdict::NeedsMigration { .. } = MigrationChain::gate(parts.format_version()) {
        verify_indexed_parts(parts, &recorded_manifest)?;
    }
    // 移行が必要な場合だけ集合を所有して組み立て直す（現行版の集合は複製しない。要件 8.1）。
    let migrated = MigrationChain::apply_with(steps, parts)?;
    let parts = migrated.as_ref().unwrap_or(parts);
    // 移行した場合は索引を組み直してから照合する（移行後の集合も同じ 1 経路で検証される）。
    let refreshed = match &migrated {
        Some(migrated) => Some(decode_manifest(migrated)?),
        None => None,
    };
    let manifest = refreshed.as_ref().unwrap_or(&recorded_manifest);

    // 3. 完全性の照合（要件 5.2 / 5.3）: 索引に載った各パートが実在し、内容が記録どおりか。
    verify_indexed_parts(parts, manifest)?;

    // 4. 各パートの復号（検証はまだ行わない: すべての復号結果が揃ってから検証する）。
    let mut document_part: Option<DocumentPart> = None;
    let mut schemas: Vec<(EntryName, SheetId, SchemaPart)> = Vec::new();
    let mut row_sets: Vec<(EntryName, SheetRows)> = Vec::new();
    let mut attachments: Vec<(EntryName, Vec<u8>)> = Vec::new();
    for part in parts.iter() {
        match part.name {
            EntryName::Manifest => {}
            EntryName::Document => {
                document_part = Some(DocumentPart::from_json_bytes(&part.bytes)?);
            }
            EntryName::Schema { .. } => {
                let (sheet, schema) = SchemaCodec::decode(&part.name, &part.bytes)?;
                schemas.push((part.name, sheet, schema));
            }
            EntryName::Rows { .. } => {
                row_sets.push((part.name, RowsCodec::decode(&part.name, &part.bytes)?));
            }
            EntryName::Attachment { attachment } => {
                // 添付は content-addressed である: 実バイト列から識別子を再計算しても
                // エントリ名と一致しなければならない（要件 7.2。不一致は中止）。
                if AttachmentId::from_bytes(&part.bytes) != attachment {
                    return Err(invalid(
                        &part.name,
                        "the attachment bytes do not hash to this entry name",
                    ));
                }
                attachments.push((part.name, part.bytes.clone()));
            }
            EntryName::Marker => {
                // `DocumentParts` は構築時にマーカーを拒否するため到達しないが、
                // 網羅性のために明示する（黙って無視する経路を作らない）。
                return Err(invalid(
                    &part.name,
                    "the type marker is a container entry, not a document part",
                ));
            }
        }
    }
    let document_part = document_part.ok_or_else(|| DocumentError::MissingPart {
        name: EntryName::Document.to_string(),
    })?;

    // 5. 構造検証（要件 4.2, 4.3, 4.4, 1.7, 7.4）。目録は復号済みパート群から組み立てる
    //    （保存経路 `validate_document` と同じ 1 経路。検証ロジックも報告文面も二重化しない）。
    let inventory = inventory_of(
        document_part.sheets().iter().map(|meta| meta.sheet_id()),
        schemas.iter().map(|(_, sheet, schema)| (*sheet, schema)),
        row_sets.iter().map(|(_, rows)| (rows.sheet(), rows.rows())),
        attachments.iter().filter_map(|(entry, _)| match entry {
            EntryName::Attachment { attachment } => Some(*attachment),
            _ => None,
        }),
    );
    StructuralValidator::validate(&inventory)?;

    // 6. 行エントリの列順序が `document.json` の列名一覧と一致すること（要件 2.3）。
    verify_row_columns(&document_part, &row_sets)?;

    // 7. モデル構築（検証済みの内容だけを移す）。
    let mut document = Document::with_document_id(document_part.document_id());
    let mut sheets = Vec::with_capacity(document_part.sheets().len());
    for meta in document_part.sheets() {
        let schema = take_schema(&mut schemas, meta.sheet_id())?;
        let rows = take_rows(&mut row_sets, meta.sheet_id());
        let mut sheet = Sheet::new(meta.sheet_id(), meta.name().to_owned());
        sheet.set_columns(meta.columns().to_vec());
        sheet.set_preserved_fields(meta.preserved_fields().clone());
        sheet.set_root_schema(schema);
        sheet.extend_rows(rows);
        sheets.push(sheet);
    }
    document.restore_sheets(sheets);
    for (_, bytes) in attachments {
        // content-addressed で冪等（識別子の再計算は上の照合で確認済み）。
        document.add_attachment(bytes);
    }
    document.set_preserved_fields(document_part.preserved_fields().clone());
    Ok(document)
}

/// 索引（`manifest.json`）のパートを取り出す。無ければ [`DocumentError::MissingPart`]。
///
/// 集合の構築時に索引の存在は確かめられている（[`DocumentParts::from_entries`]）が、
/// 読み込み経路も同じ入口を通るため、ここでも同じ報告をする（`panic` しない）。
fn manifest_part_of(parts: &DocumentParts) -> Result<&Part, DocumentError> {
    resolve_manifest(parts.iter().map(|part| &part.name))?;
    parts.get(&MANIFEST_ENTRY).ok_or_else(|| DocumentError::MissingPart {
        name: MANIFEST_ENTRY.to_string(),
    })
}

/// 集合の索引（`manifest.json`）を復号する。
///
/// 索引は信頼の根であり（design「Container Entry Layout」）、復号できない索引を持つ集合は
/// 読み込めない（[`DocumentError::InvalidContainer`]）。
fn decode_manifest(parts: &DocumentParts) -> Result<ManifestPart, DocumentError> {
    let part = manifest_part_of(parts)?;
    ManifestPart::from_json_bytes(&part.bytes)
}

/// 索引に載った各パートが実在し、内容が記録どおりかを照合する（要件 5.2 / 5.3）。
///
/// 向きは**索引 → 実体**である（索引に載った各パートが実在し、内容が記録どおりか）。
/// 移行の前後で同じ照合を使う（移行が必要な集合でも、記録されたままの集合を先に照合し、
/// 移行後の集合をもう一度通す。モジュール docs「移行とダイジェスト照合の順序」）。
fn verify_indexed_parts(
    parts: &DocumentParts,
    manifest: &ManifestPart,
) -> Result<(), DocumentError> {
    for entry in manifest.entries() {
        let part = parts.get(&entry.name()).ok_or_else(|| DocumentError::MissingPart {
            name: entry.name().to_string(),
        })?;
        verify_part(&entry.name(), &part.bytes, entry.digest())?;
    }
    Ok(())
}

/// 索引からパート集合の形式バージョンを読む（構築時に 1 回だけ通る）。
fn read_format_version(parts: &[Part]) -> Result<FormatVersion, DocumentError> {
    resolve_manifest(parts.iter().map(|part| &part.name))?;
    let manifest = parts
        .iter()
        .find(|part| part.name == MANIFEST_ENTRY)
        .ok_or_else(|| DocumentError::MissingPart { name: MANIFEST_ENTRY.to_string() })?;
    Ok(ManifestPart::from_json_bytes(&manifest.bytes)?.version())
}

/// 復号済みパート群またはモデルから構造検証の目録（[`PartInventory`]）を組み立てる。
///
/// 出現箇所テキストの綴りは [`StructuralValidator`] の推奨慣例に合わせる
/// （`document.json sheets[i]` / `schemas/<ulid>.json types[i]` /
/// `sheets/<ulid>.jsonl line n` / `attachments/<hex>.bin`）。エントリ名はシート識別子から
/// 正準形（[`EntryName`] の表示テキスト）を組み直すため、読み込み経路（復号済みパート）と
/// 保存経路（モデル）で**同じ違反が同じ文言になる**（検証ロジックも報告文面も 2 箇所に
/// 持たない。design「`open` と `from_parts` は同一の検証経路を通る」）。
///
/// 入力は**借用**で受ける: 行は `&[Row]` のまま渡し、複製しない（10 万行の保存経路で
/// 行の複製を作らない）。読み込み経路は復号済みの `SheetRows` から、保存経路は
/// [`Document`] から直接、同じ目録を組み立てる。
///
/// **型定義の宣言には所属シートを与える**（`IdDeclaration::with_sheet`）。与え忘れると
/// 同一シート規則（要件 1.7）がどのシートの参照も満たせず、妥当な文書を宙吊りとして
/// 誤報する（タスク 4.7 の申し送り）。
fn inventory_of<'a>(
    sheets: impl Iterator<Item = SheetId>,
    schemas: impl Iterator<Item = (SheetId, &'a SchemaPart)>,
    row_sets: impl Iterator<Item = (SheetId, &'a [Row])>,
    attachments: impl Iterator<Item = AttachmentId>,
) -> PartInventory {
    let mut inventory = PartInventory::new();

    // document.json: シート識別子の宣言と、スキーマの要求（全シート。要件 1.2, 4.4）。
    for (index, sheet) in sheets.enumerate() {
        let sheet = sheet.to_string();
        inventory.declare(IdDeclaration::new(
            IdKind::Sheet,
            sheet.clone(),
            format!("{} sheets[{index}]", EntryName::Document),
        ));
        inventory.require_schema(sheet);
    }

    // schemas/*: スキーマの提供、型定義の宣言（所属シートつき）、型定義参照の出現。
    for (sheet, schema) in schemas {
        let entry = EntryName::Schema { sheet };
        let sheet_text = sheet.to_string();
        inventory.provide_schema(sheet_text.clone());
        inventory.declare_sheet_ref(SheetRefDeclaration::new(
            entry.to_string(),
            sheet_text.clone(),
        ));
        for (index, def) in schema.type_defs().iter().enumerate() {
            inventory.declare(
                IdDeclaration::new(
                    IdKind::TypeDef,
                    def.id().to_string(),
                    format!("{entry} types[{index}]"),
                )
                .with_sheet(sheet_text.clone()),
            );
        }
        for reference in schema.type_refs() {
            inventory.declare_type_ref(TypeRefDeclaration::new(
                format!("{entry} {}", reference.from()),
                reference.to(),
                sheet_text.clone(),
            ));
        }
    }

    // sheets/*: 行識別子の宣言、セル値からの添付参照、エントリ名が指すシートの参照。
    for (sheet, rows) in row_sets {
        let entry = EntryName::Rows { sheet };
        inventory.declare_sheet_ref(SheetRefDeclaration::new(entry.to_string(), sheet.to_string()));
        for (index, row) in rows.iter().enumerate() {
            let from = format!("{entry} line {}", index + 1);
            inventory.declare(IdDeclaration::new(IdKind::Row, row.id().to_string(), from.clone()));
            for value in row.values() {
                inventory.declare_attachment_refs(from.clone(), value);
            }
        }
    }

    // attachments/*: エントリ名そのものが content-addressed の宣言である。
    // 出現箇所はエントリ名を使う（推奨慣例）。
    for attachment in attachments {
        inventory.declare(IdDeclaration::new(
            IdKind::Attachment,
            attachment.to_string(),
            EntryName::Attachment { attachment }.to_string(),
        ));
    }

    inventory
}

/// モデルが構造的不変条件を満たすか検証する（design「保存フロー」の最初の段。タスク 7.2）。
///
/// 保存経路が**書き込みの前に**呼ぶ公開経路である（`save` 以外の後続タスク 7.3 / 7.4 からも
/// 同じ 1 経路を使える）。実装は「[`inventory_of`] でモデルから目録を組み立て、
/// [`StructuralValidator::validate`] を 1 回呼ぶ」だけであり、**規則を再実装しない**:
///
/// - 同一種別の識別子の一意性（とくに `TypeDefId` の重複宣言）は要件 4.3
/// - 型定義参照の実在（要件 1.7。`SchemaPart::parse` は `$ref` の実在を見ない）
/// - 添付参照の実在（要件 7.4。`CellValue::Attachment` は登録を強制しない）
/// - スキーマの存在（要件 4.4）
///
/// モデル API が**構築で強制しない**のはこの 4 つである（識別子の発行は `SheetId` /
/// `RowId` / 文書識別子については `IdFactory` が担うが、`TypeDefId` はスキーマの
/// ペイロードの中にあり、参照と添付の実在はモデルが解釈しない）。構築が強制する不変条件
/// （各シートちょうど 1 つのルートスキーマ、改名・並べ替えで識別子が変わらない）は
/// ここでは検査しない。
///
/// 行の値の個数と列数の一致・列名の重複は**ワイヤ形**の問題であり、この関数ではなく
/// [`to_parts`] が遮断する（呼び出し元の programming error として
/// [`DocumentError::InvalidContainer`]）。
pub fn validate_document(document: &Document) -> Result<(), DocumentError> {
    let inventory = inventory_of(
        document.sheets().iter().map(Sheet::id),
        document.sheets().iter().map(|sheet| (sheet.id(), sheet.root_schema())),
        document.sheets().iter().map(|sheet| (sheet.id(), sheet.rows())),
        document.attachments().iter().map(|attachment| attachment.id()),
    );
    StructuralValidator::validate(&inventory)
}

/// 行エントリの列順序が `document.json` の列名一覧と一致することを検証する（要件 2.3）。
///
/// 0 行のエントリは列順を表現できない（列を書く行が無い）ため検査しない。`document.json` が
/// 唯一の永続先である。行を持つエントリでは列名の並び（順序も集合も）が完全一致しなければ
/// ならない: ここで食い違いを見逃すと、行の値が別の列に割り当てられたモデルを返すことになる。
fn verify_row_columns(
    document_part: &DocumentPart,
    row_sets: &[(EntryName, SheetRows)],
) -> Result<(), DocumentError> {
    for (entry, rows) in row_sets {
        if rows.rows().is_empty() {
            continue;
        }
        let Some(meta) = document_part.sheets().iter().find(|meta| meta.sheet_id() == rows.sheet())
        else {
            // 未知シートは構造検証（シート参照）が先に報告するため到達しない。
            return Err(invalid(entry, "no metadata for this sheet in document.json"));
        };
        if meta.columns() != rows.columns() {
            return Err(invalid(
                entry,
                format!(
                    "row keys do not match the columns in document.json: expected {:?}, found {:?}",
                    meta.columns(),
                    rows.columns(),
                ),
            ));
        }
    }
    Ok(())
}

/// 復号済みのスキーマを 1 枚分取り出す（所有権を移す）。
///
/// スキーマの存在は構造検証（要件 4.4）が要求済みであり、欠落は到達しない経路だが、
/// `panic` を置かず同じ意味の変種で報告する。
fn take_schema(
    schemas: &mut Vec<(EntryName, SheetId, SchemaPart)>,
    sheet: SheetId,
) -> Result<SchemaPart, DocumentError> {
    match schemas.iter().position(|(_, id, _)| *id == sheet) {
        Some(index) => Ok(schemas.swap_remove(index).2),
        None => Err(DocumentError::MissingSchema { sheet: sheet.to_string() }),
    }
}

/// 復号済みの行を 1 枚分取り出す（所有権を移す）。行エントリを持たないシートは 0 行である。
fn take_rows(row_sets: &mut Vec<(EntryName, SheetRows)>, sheet: SheetId) -> Vec<Row> {
    match row_sets.iter().position(|(_, rows)| rows.sheet() == sheet) {
        Some(index) => row_sets.swap_remove(index).1.into_rows(),
        None => Vec::new(),
    }
}

/// 行エントリの符号化失敗（呼び出し元の programming error）を保存のエラーへ写す。
///
/// design のエラー表に固有の変種が無いため、失敗箇所のエントリ名と理由を
/// [`DocumentError::InvalidContainer`] の `entry` へ載せる（各パートモジュールと同じ規約）。
fn rows_encode_error(sheet: SheetId, error: RowsEncodeError) -> DocumentError {
    match error {
        RowsEncodeError::Document(error) => error,
        other => invalid(&EntryName::Rows { sheet }, other),
    }
}

/// エントリ名を文脈にした失敗（失敗箇所のラベル + 理由）。
fn invalid(entry: &EntryName, reason: impl fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer { entry: format!("{entry}: {reason}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::IdFactory;
    use crate::migration::steps::synthetic;
    use crate::value::{CellValue, NestedValue};

    /// 標本の型定義識別子（正準 Crockford base32 大文字 26 文字）。
    const TYPE_DEF: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    /// 列名の列を作る（列順は与えた順のまま）。
    fn columns(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    /// 標本のルートスキーマ（型定義 `TYPE_DEF` への参照を 1 つ持つ）。
    fn schema_with_ref() -> SchemaPart {
        SchemaPart::parse(&format!(
            r#"{{"root":{{"$ref":"{TYPE_DEF}"}},"types":[{{"id":"{TYPE_DEF}","definition":{{"kind":"string"}}}}]}}"#
        ))
        .expect("標本は妥当なエンベロープ")
    }

    /// 標本の文書: 2 シート（行 2 行 + 0 行）・型定義参照・添付 1 件。
    fn sample_document() -> Document {
        let mut document = Document::new();
        let stocked = document.add_sheet("在庫");
        document
            .set_sheet_columns(stocked, columns(&["name", "$id", "量"]))
            .expect("標本のシートは実在する");
        document.set_root_schema(stocked, schema_with_ref()).expect("標本のシートは実在する");
        let attachment = document.add_attachment(vec![0x00, 0xff, b'x']);
        let first = document.add_row(stocked).expect("標本の行は実在する");
        document
            .set_row_values(
                stocked,
                first,
                vec![
                    CellValue::Text("りんご".to_owned()),
                    CellValue::Attachment(attachment),
                    CellValue::Nested(NestedValue::Array(vec![CellValue::Int(1)])),
                ],
            )
            .expect("標本の行は実在する");
        let second = document.add_row(stocked).expect("標本の行は実在する");
        document
            .set_row_values(
                stocked,
                second,
                vec![CellValue::Null, CellValue::Decimal("0.5".to_owned()), CellValue::Bool(true)],
            )
            .expect("標本の行は実在する");

        let empty = document.add_sheet("空");
        document.set_sheet_columns(empty, columns(&["x"])).expect("標本のシートは実在する");
        document.set_root_schema(empty, SchemaPart::empty()).expect("標本のシートは実在する");
        document
    }

    /// 集合を（エントリ名, バイト列）の列へ写す。
    fn entries(parts: &DocumentParts) -> Vec<(EntryName, Vec<u8>)> {
        parts.iter().map(|part| (part.name, part.bytes.clone())).collect()
    }

    /// エントリ名の表示テキスト（反復順そのまま）。
    fn names(parts: &DocumentParts) -> Vec<String> {
        parts.iter().map(|part| part.name.to_string()).collect()
    }

    /// 標本の集合から 1 件を差し替える。
    fn replace(entries: &mut [(EntryName, Vec<u8>)], name: EntryName, bytes: Vec<u8>) {
        let slot = entries
            .iter_mut()
            .find(|(entry, _)| *entry == name)
            .unwrap_or_else(|| panic!("標本に {name} が無い"));
        slot.1 = bytes;
    }

    /// 集合を指定バージョンの索引で組み直す（各パートのダイジェストは実体に一致させたまま、
    /// 記録値だけを差し替える。ゲートの入力を作る唯一の補助）。
    fn rebuilt(version: FormatVersion, mut entries: Vec<(EntryName, Vec<u8>)>) -> DocumentParts {
        entries.retain(|(name, _)| *name != MANIFEST_ENTRY);
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

    /// 構築の正準化: 入力順に関わらずエントリ名の昇順になり、重複は拒否される
    /// （要件 2.2。ファイルシステムの列挙順に依存しないことの構造的な根拠）。
    #[test]
    fn from_entries_sorts_ascending_and_rejects_duplicates() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");
        let forward = entries(&parts);
        assert!(forward.len() >= 6, "標本のパートが少なすぎる: {}", forward.len());

        // 逆順・入れ替え順で組み立てても、反復順も内容も同一である。
        let ascending = names(&parts);
        for order in [forward.iter().rev().cloned().collect::<Vec<_>>(), {
            let mut shuffled = forward.clone();
            shuffled.swap(1, 3);
            shuffled
        }] {
            let rebuilt = DocumentParts::from_entries(order).expect("標本の集合は妥当");
            assert_eq!(ascending, names(&rebuilt), "入力順が反復順へ漏れている");
            assert_eq!(forward, entries(&rebuilt), "入力順が内容を変えている");
        }

        // 同一エントリ名が 2 回現れる集合は不正である（コンテナの重複パスと同じ分類）。
        let mut duplicated = forward.clone();
        let repeated = duplicated[0].0.to_string();
        duplicated.push(duplicated[0].clone());
        match DocumentParts::from_entries(duplicated) {
            Err(DocumentError::InvalidContainer { entry }) => {
                assert!(entry.starts_with(&repeated), "entry が違う: {entry}");
            }
            other => panic!("重複エントリ名が拒否されない: {:?}", other.map(|parts| names(&parts))),
        }
    }

    /// 形式マーカー `jxcel` はコンテナ層のエントリであり、論理エントリ集合には含めない
    /// （モジュール docs「型マーカー `jxcel` はこの集合に含めない」）。
    #[test]
    fn the_type_marker_is_rejected_from_the_set() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");
        let mut with_marker = entries(&parts);
        with_marker.push((EntryName::Marker, b"jxcel".to_vec()));

        match DocumentParts::from_entries(with_marker) {
            Err(DocumentError::InvalidContainer { entry }) => {
                assert!(entry.starts_with("jxcel:"), "entry が違う: {entry}");
            }
            other => panic!("形式マーカーが拒否されない: {:?}", other.map(|parts| names(&parts))),
        }
    }

    /// 集合は必ず索引（`manifest.json`）を持ち、索引は復号できなければならない。
    /// 形式バージョンは索引から読む（要件 6.1）。
    #[test]
    fn from_entries_requires_a_readable_manifest() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");
        let forward = entries(&parts);
        let version = parts.format_version();
        assert_eq!(CURRENT_FORMAT_VERSION, version, "現行バージョンが 1.0 でない");

        // 索引が無い集合は構築できない。
        let without = forward
            .iter()
            .filter(|(name, _)| *name != MANIFEST_ENTRY)
            .cloned()
            .collect::<Vec<_>>();
        match DocumentParts::from_entries(without) {
            Err(DocumentError::MissingPart { name }) => assert_eq!(MANIFEST_ENTRY.to_string(), name),
            other => panic!("索引の無い集合が拒否されない: {:?}", other.map(|parts| names(&parts))),
        }

        // 索引が復号できなければ拒否する（索引は信頼の根である）。
        let mut corrupt = forward.clone();
        replace(&mut corrupt, MANIFEST_ENTRY, b"{".to_vec());
        match DocumentParts::from_entries(corrupt) {
            Err(DocumentError::InvalidContainer { entry }) => {
                assert!(entry.starts_with("manifest.json:"), "entry が違う: {entry}");
            }
            other => panic!("壊れた索引が拒否されない: {:?}", other.map(|parts| names(&parts))),
        }

        // 索引が記録したバージョンはそのまま報告される（構築経路 `from_entries` はゲートを
        // 掛けない。読めるか否かは読み込み経路 `from_parts` が判定する）。
        let index: Vec<ManifestEntry> = forward
            .iter()
            .filter(|(name, _)| *name != MANIFEST_ENTRY)
            .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
            .collect();
        let future = ManifestPart::new(FormatVersion::new(2, 7), index)
            .expect("標本の索引は妥当")
            .to_json_bytes()
            .expect("符号化");
        let mut with_future = forward.clone();
        replace(&mut with_future, MANIFEST_ENTRY, future);
        let rebuilt = DocumentParts::from_entries(with_future).expect("標本の集合は妥当");
        assert_eq!(FormatVersion::new(2, 7), rebuilt.format_version(), "索引のバージョンが読めない");
    }

    /// 読み込み経路の形式バージョンゲート（要件 6.5。design 読み込みフロー「形式バージョン」）:
    /// 現行より新しい major の集合は、**要求バージョンと対応バージョンを持つ**
    /// [`DocumentError::UnsupportedVersion`] として拒否され、部分的なモデルを返さない（要件 5.4）。
    #[test]
    fn from_parts_rejects_a_major_newer_than_the_current_one() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");
        let forked = rebuilt(FormatVersion::new(2, 7), entries(&parts));

        match from_parts(&forked) {
            Err(DocumentError::UnsupportedVersion { found, supported }) => {
                assert_eq!(FormatVersion::new(2, 7), found, "要求バージョンが報告されていない");
                assert_eq!(CURRENT_FORMAT_VERSION, supported, "対応バージョンが報告されていない");
            }
            Ok(restored) => {
                panic!("拒否されずモデルが返った（{} シート）", restored.sheets().len())
            }
            Err(other) => panic!("変種が違う: {other}"),
        }
    }

    /// ゲートの major 境界: 同一 major は minor の新旧を問わず受理し、より新しい major は拒否する。
    ///
    /// **古い major は移行チェーンを試みるが、実表（v1 のみ・空）に移行先が無いため同じ中止に
    /// なる**。タスク 6.2 が 6.1 の「ゲートで即拒否」の分岐を実チェーン適用へ置き換えた
    /// （観測結果は同じ。モジュール docs「移行とダイジェスト照合の順序」）。
    #[test]
    fn from_parts_gates_on_the_major_boundary() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");

        for accepted in [FormatVersion::new(1, 0), FormatVersion::new(1, 1), FormatVersion::new(1, 99)]
        {
            let same_major = rebuilt(accepted, entries(&parts));
            let restored = from_parts(&same_major)
                .unwrap_or_else(|error| panic!("同一 major の {accepted} が拒否された: {error}"));
            assert_eq!(document.document_id(), restored.document_id(), "{accepted} で内容が変わった");
        }

        for rejected in [FormatVersion::new(2, 0), FormatVersion::new(0, 9)] {
            let other_major = rebuilt(rejected, entries(&parts));
            match from_parts(&other_major) {
                Err(DocumentError::UnsupportedVersion { found, supported }) => {
                    assert_eq!(rejected, found, "{rejected} の要求バージョンが報告されていない");
                    assert_eq!(CURRENT_FORMAT_VERSION, supported);
                }
                Ok(restored) => panic!(
                    "{rejected} が拒否されずモデルが返った（{} シート）",
                    restored.sheets().len()
                ),
                Err(other) => panic!("{rejected} の変種が違う: {other}"),
            }
        }
    }

    /// ゲートは完全性の照合より前に走る（design 読み込みフローの「形式バージョン」→
    /// 「ダイジェスト照合」）: 索引が実体の無いパートを載せていても、版が読めなければ
    /// [`DocumentError::UnsupportedVersion`] が返る（[`DocumentError::MissingPart`] ではない）。
    ///
    /// **この順序はタスク 6.2 が移行を挿した位置の前提である**: 移行する集合は、まず記録
    /// されたままの照合を通る（モジュール docs「移行とダイジェスト照合の順序」）。
    #[test]
    fn the_version_gate_runs_before_the_integrity_check() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");
        let absent = EntryName::Rows { sheet: IdFactory::new().new_sheet_id() };

        let mut forward = entries(&parts);
        forward.retain(|(name, _)| *name != MANIFEST_ENTRY);
        let mut index: Vec<ManifestEntry> = forward
            .iter()
            .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
            .collect();
        index.push(ManifestEntry::of_bytes(absent, b"no such part in the set"));
        let manifest = ManifestPart::new(FormatVersion::new(2, 7), index)
            .expect("標本の索引は妥当")
            .to_json_bytes()
            .expect("符号化");
        forward.push((MANIFEST_ENTRY, manifest));
        let broken = DocumentParts::from_entries(forward).expect("標本の集合は妥当");

        match from_parts(&broken) {
            Err(DocumentError::UnsupportedVersion { found, .. }) => {
                assert_eq!(FormatVersion::new(2, 7), found);
            }
            Ok(restored) => {
                panic!("拒否されずモデルが返った（{} シート）", restored.sheets().len())
            }
            Err(other) => panic!("ゲートが完全性照合より後にある: {other}"),
        }
    }

    /// `get` はエントリ名で引き（無ければ `None`）、`Part` のダイジェストは実バイト列と
    /// 一致する（design「DocumentParts」の不変条件。要件 5.1）。
    #[test]
    fn get_looks_up_by_name_and_part_digests_match_their_bytes() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");

        for part in parts.iter() {
            assert_eq!(digest_part(&part.bytes), part.digest, "ダイジェストが実バイト列と違う");
            let looked_up = parts.get(&part.name).expect("集合にある名前は引ける");
            assert_eq!(part.bytes, looked_up.bytes);
        }

        // 集合に無いエントリ名（標本が持たない別シートの行エントリ）は引けない。
        let absent = EntryName::Rows { sheet: IdFactory::new().new_sheet_id() };
        assert!(parts.get(&absent).is_none(), "集合に無い名前が引けた");
    }

    /// 0 シートの文書でも往復し、パート集合は `document.json` と `manifest.json` の 2 件に
    /// なる（要件 1.1。端の状態を構造で確かめる）。
    #[test]
    fn a_document_without_sheets_round_trips() {
        let document = Document::new();
        let parts = to_parts(&document).expect("保存経路");

        assert_eq!(
            vec![EntryName::Document.to_string(), MANIFEST_ENTRY.to_string()],
            names(&parts),
            "0 シートの集合の構成が違う"
        );

        let restored = from_parts(&parts).expect("読み込み経路");
        assert_eq!(document.document_id(), restored.document_id());
        assert!(restored.sheets().is_empty(), "シートが 0 枚でない");
        assert_eq!(entries(&parts), entries(&to_parts(&restored).expect("再保存経路")));
    }

    /// 保存側の programming error（値の個数と列数の不一致・列名の重複）は、対象の行エントリ名を
    /// 含む [`DocumentError::InvalidContainer`] として報告され、部分的な集合を残さない。
    #[test]
    fn row_encoding_errors_are_reported_with_the_rows_entry_name() {
        // 値の個数が列数と合わない行。
        let mut document = Document::new();
        let sheet = document.add_sheet("不一致");
        document.set_sheet_columns(sheet, columns(&["only"])).expect("標本のシートは実在する");
        let row = document.add_row(sheet).expect("標本の行は実在する");
        document
            .set_row_values(sheet, row, vec![CellValue::Int(1), CellValue::Int(2)])
            .expect("標本の行は実在する");
        match to_parts(&document) {
            Err(DocumentError::InvalidContainer { entry }) => {
                assert!(
                    entry.starts_with(&EntryName::Rows { sheet }.to_string()),
                    "entry が行エントリ名で始まらない: {entry}"
                );
            }
            other => panic!("値の個数の不一致が報告されない: {other:?}"),
        }

        // 列名が重複しているシート。
        let mut document = Document::new();
        let sheet = document.add_sheet("重複列");
        document.set_sheet_columns(sheet, columns(&["a", "a"])).expect("標本のシートは実在する");
        let row = document.add_row(sheet).expect("標本の行は実在する");
        document
            .set_row_values(sheet, row, vec![CellValue::Int(1), CellValue::Int(2)])
            .expect("標本の行は実在する");
        match to_parts(&document) {
            Err(DocumentError::InvalidContainer { entry }) => {
                assert!(
                    entry.starts_with(&EntryName::Rows { sheet }.to_string()),
                    "entry が行エントリ名で始まらない: {entry}"
                );
            }
            other => panic!("列名の重複が報告されない: {other:?}"),
        }
    }

    /// 添付の実バイト列が content-addressed 識別子と一致しないエントリは拒否される
    /// （要件 7.2。索引のダイジェストを実体に合わせて組み直しても検出できる）。
    #[test]
    fn an_attachment_that_does_not_hash_to_its_entry_name_is_rejected() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");
        let version = parts.format_version();
        let mut forward = entries(&parts);

        let attachment_entry = forward
            .iter()
            .map(|(name, _)| *name)
            .find(|name| matches!(name, EntryName::Attachment { .. }))
            .expect("標本は添付を持つ");
        // 実バイト列を差し替え、索引のダイジェストは実体に合わせて組み直す
        // （完全性照合ではなく content-addressed の照合で落ちることを確かめる）。
        replace(&mut forward, attachment_entry, b"different bytes".to_vec());
        // 索引を組み直す（添付のエントリ名は元の識別子のまま）。
        let index: Vec<ManifestEntry> = forward
            .iter()
            .filter(|(name, _)| *name != MANIFEST_ENTRY)
            .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
            .collect();
        let manifest = ManifestPart::new(version, index)
            .expect("標本の索引は妥当")
            .to_json_bytes()
            .expect("符号化");
        forward.retain(|(name, _)| *name != MANIFEST_ENTRY);
        forward.push((MANIFEST_ENTRY, manifest));

        match from_parts(&DocumentParts::from_entries(forward).expect("標本の集合は妥当")) {
            Err(DocumentError::InvalidContainer { entry }) => {
                assert!(
                    entry.starts_with(&attachment_entry.to_string()),
                    "entry が添付エントリ名で始まらない: {entry}"
                );
            }
            other => panic!("content-addressed の不一致が報告されない: {other:?}"),
        }
    }

    /// 端から端まで: 古い版の集合が合成の多段表を通って**モデルまで到達**し、移行後の
    /// `manifest.json` の版が現行版になる（要件 6.2, 6.3）。
    ///
    /// 合成表（[`synthetic`]）は多段の経路を実表（v1 のみ・空）と独立に試すための
    /// クレート内部の道具である（`migration::tests::the_step_table_is_empty_at_v1` が実表の
    /// 空を固定する）。
    #[test]
    fn from_parts_migrates_an_older_version_through_the_step_table() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");
        let recorded = synthetic::recorded_at(synthetic::OLDEST, &parts);
        assert_eq!(
            FormatVersion::new(0, 0),
            recorded.format_version(),
            "標本の記録値が古い版になっていない"
        );

        let restored = from_parts_with(synthetic::MULTI_STEP, &recorded).expect("移行して読める");
        // 段の適用順序は、段がシート名へ積む印で観測する（逆順・一段のみでは最終形にならない）。
        let names: Vec<&str> = restored.sheets().iter().map(|sheet| sheet.name()).collect();
        assert_eq!(vec!["在庫|v0.1|v1.0", "空|v0.1|v1.0"], names, "段の適用順序が違う");
        // 移行が書き換えるのは `document.json` だけである: 識別子・列名・行・添付はそのまま復元される。
        assert_eq!(document.document_id(), restored.document_id(), "移行で識別子が変わった");
        assert_eq!(
            vec![columns(&["name", "$id", "量"]), columns(&["x"])],
            restored.sheets().iter().map(|sheet| sheet.columns().to_vec()).collect::<Vec<_>>(),
            "移行で列名が変わった"
        );
        assert_eq!(
            vec![2, 0],
            restored.sheets().iter().map(|sheet| sheet.rows().len()).collect::<Vec<_>>(),
            "移行で行が変わった"
        );
        assert_eq!(
            document.attachments().iter().count(),
            restored.attachments().iter().count(),
            "移行で添付が変わった"
        );

        // 移行後の `manifest.json` の版は現行版である（移行の記帳）。
        let migrated = MigrationChain::apply_with(synthetic::MULTI_STEP, &recorded)
            .expect("合成表は妥当")
            .expect("古い版は移行される");
        assert_eq!(CURRENT_FORMAT_VERSION, migrated.format_version(), "移行後の版が現行版でない");
    }

    /// 現行版の集合は移行されない: 合成の多段表を渡しても段は 1 つも走らず（印が付かず）、
    /// 集合も複製されない（[`MigrationChain::apply`] は `None` を返す。要件 8.1 の予算に
    /// 移行の複製と再ハッシュを持ち込まない）。
    #[test]
    fn the_current_version_is_neither_migrated_nor_copied() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");
        assert_eq!(CURRENT_FORMAT_VERSION, parts.format_version(), "標本が現行版でない");

        let restored = from_parts_with(synthetic::MULTI_STEP, &parts).expect("現行版は読める");
        let names: Vec<&str> = restored.sheets().iter().map(|sheet| sheet.name()).collect();
        assert_eq!(vec!["在庫", "空"], names, "現行版の集合に段が適用された");
        assert!(
            MigrationChain::apply_with(synthetic::MULTI_STEP, &parts)
                .expect("現行版は中止しない")
                .is_none(),
            "現行版の集合が移行（複製）された"
        );
    }

    /// 移行が必要な版の集合でも、**移行の前に**記録されたままの照合を行う（要件 5.2 は古い
    /// 形式のファイルにも適用される）。移行が索引を無条件に作り直すと、壊れた古いファイルが
    /// 黙って「修復」されて読まれてしまう（タスク 6.2 の親の裁定。モジュール docs
    /// 「移行とダイジェスト照合の順序」）。
    #[test]
    fn a_corrupt_older_version_is_rejected_before_the_migration_runs() {
        let document = sample_document();
        let parts = to_parts(&document).expect("保存経路");
        // 記録値だけを古い版へ差し替える（索引のダイジェストは元の実体のまま）。
        let recorded = rebuilt(FormatVersion::new(0, 0), entries(&parts));
        // 索引が指す実体（`document.json`）を 1 箇所だけ書き換える。**復号できる**壊し方に
        // するのが要点である: 移行が先に走れば、新しい索引で上書きされて「修復」されてしまう。
        let mut forward = entries(&recorded);
        let original = forward
            .iter()
            .find(|(name, _)| *name == EntryName::Document)
            .expect("標本はメタデータを持つ")
            .1
            .clone();
        let text = String::from_utf8(original).expect("メタデータは UTF-8");
        let tampered = text.replacen('{', r#"{"unrecorded":1,"#, 1).into_bytes();
        replace(&mut forward, EntryName::Document, tampered);
        let broken = DocumentParts::from_entries(forward).expect("標本の集合は妥当");

        match from_parts_with(synthetic::MULTI_STEP, &broken) {
            Err(DocumentError::IntegrityMismatch { entry }) => {
                assert_eq!(EntryName::Document.to_string(), entry, "不一致のエントリが違う");
            }
            Ok(restored) => panic!(
                "壊れた旧版が移行で「修復」されて読まれた（{} シート）",
                restored.sheets().len()
            ),
            Err(other) => panic!("移行が照合より先に走っている: {other}"),
        }
    }

    /// 未知フィールドは**移行の経路でも**保持される: 段が `document.json` を書き換えても
    /// （トップレベルとシート要素の）未知キーは残り、モデルを経由して書き戻しても失われない
    /// （要件 6.2, 6.3）。
    #[test]
    fn unknown_fields_survive_migration_and_reach_the_model() {
        let parts = to_parts(&sample_document()).expect("保存経路");
        let recorded = synthetic::with_unknown_fields(FormatVersion::new(0, 0), &parts);

        let restored = from_parts_with(synthetic::MULTI_STEP, &recorded).expect("移行して読める");
        let rewritten = to_parts(&restored).expect("再保存経路");
        let document = rewritten.get(&EntryName::Document).expect("メタデータは常に存在する");
        let text = String::from_utf8(document.bytes.clone()).expect("メタデータは UTF-8");
        assert!(
            text.contains(r#""future_top":{"unit":"mm"}"#),
            "トップレベルの未知フィールドが移行とモデル経由で消えた: {text}"
        );
        assert!(
            text.contains(r#""element_note":7"#),
            "シート要素の未知フィールドが移行とモデル経由で消えた: {text}"
        );
    }
}

//! jxcel ドキュメント形式の実装。
//! ZIP コンテナに格納された JSON テキストとメモリ上のドキュメントモデルを
//! 双方向に変換する。同一内容は常に同一バイト列になる（決定的出力）。
//!
//! # 公開 API 層（design「Public API Layer / DocumentFormatApi」）
//!
//! 本クレートの対外的な入口が [`DocumentFormatApi`] とその具象実装 [`DocumentFormat`] である
//! （`Ids / Value / EntryName → Model → Json → Parts → Container → Api` の最右。
//! design「Architecture Integration」）。design の Service Interface が定める 4 メソッドを
//! 備える: タスク 7.1 が**読み込み**（[`DocumentFormatApi::open`]）を、7.2 が**保存**
//! （[`DocumentFormatApi::save`]）を、7.3 が**論理エントリ集合の公開契約**
//! （[`DocumentFormatApi::to_parts`] / [`DocumentFormatApi::from_parts`]）を結線した。
//! ファイル I/O は `open` / `save` だけが担い、`to_parts` / `from_parts` は一切のファイル
//! I/O をせず（ZIP も開かない。[`crate::container`] の型を参照しない）、下位層
//! （[`crate::parts`] / [`crate::migration`]）の 1 経路を呼ぶだけである。検証ロジックを
//! この層へ持ち込まない（design「Validation: `open` と `from_parts` は同一の検証経路を
//! 通る。検証ロジックを 2 箇所に持たない」）。
//!
//! ## 論理エントリ集合の公開契約（design `DocumentFormatApi`。要件 2.2, 2.3）
//!
//! [`DocumentFormatApi::to_parts`] / [`DocumentFormatApi::from_parts`] が
//! `version-control` との**唯一の接点**である（design の Implementation Notes。
//! `version-control` が [`ContainerCodec`] を直接呼ぶことは境界違反）。したがって両者は
//! ZIP もファイル I/O も経由せず、その実体は [`crate::parts`] の 1 経路を呼ぶだけである:
//!
//! 1. **[`DocumentFormatApi::from_parts`]**: [`crate::parts::from_parts`] をそのまま呼ぶ。
//!    `open` が使うのと同じ 1 経路であり（バージョンゲート・段階的移行・ダイジェスト照合・
//!    構造検証・モデル構築）、検証が構造的に同一である。
//! 2. **[`DocumentFormatApi::to_parts`]**: まず [`crate::parts::validate_document`] で
//!    不変条件を検証し、次に [`crate::parts::to_parts`] でパートを構築する。検証を先に
//!    通す理由は、そうしないと `version-control` が**読み込み経路の拒否する集合**
//!    （宙吊り型参照・未登録添付参照・`TypeDefId` の重複宣言）を公開経路から手に入れて
//!    しまうためである（モジュール docs「保存の順序」の第 1 段と同じ検証器。タスク 7.2 の
//!    裁定）。ワイヤ形の整合（行の値の個数と列数、列名の重複）は
//!    [`crate::parts::to_parts`] 側の既存検査のままである。
//!
//! [`DocumentFormatApi::save`] も同じ [`DocumentFormatApi::to_parts`] を使って検証と構築の
//! 経路を 1 本にしている（保存 = `to_parts` → 符号化 → 原子的書き込み）。検証を 2 箇所に
//! 持たない。
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
//! ## 保存の順序（design「System Flows / 保存フロー」。要件 8.2、5.6）
//!
//! [`DocumentFormatApi::save`] は次の順に既存の層を接続する（タスク 7.2、7.3）。**検証・
//! パート構築・符号化・書き込みのロジックをこの層に再実装しない**（各層の 1 経路を呼ぶ）。
//! 検証とパート構築は [`DocumentFormatApi::to_parts`] の 1 経路に集約されている:
//!
//! 1. **不変条件の検証とパート構築**（[`DocumentFormatApi::to_parts`] = 第 1 段 + 第 2 段）:
//!    - **不変条件の検証**: [`crate::parts::validate_document`] が、モデルから
//!      組み立てた目録に [`crate::parts::StructuralValidator`] を 1 回掛ける（design の保存
//!      フローが最初の段に置く「構造的不変条件を検証」。`Model-->>Api: ok または
//!      StructuralError`）。読み込み経路（`from_parts`）と**同じ検証器・同じ出現箇所
//!      テキスト**である（検証ロジックを 2 箇所に持たない）。
//!      - **構築で強制される不変条件**（この段では検査しない）: 各シートがちょうど 1 つの
//!        ルートスキーマを持つこと（[`Document::set_root_schema`] の置換だけが変える）、
//!        改名（[`Document::rename_sheet`]）・並べ替え（[`Document::reorder_rows`]）で
//!        識別子が変わらないこと、`SheetId` / `RowId` が [`IdFactory`] の発行経路だけから
//!        得られること。
//!      - **構築では強制されない不変条件**（この段が遮断する）: 同一種別の識別子の一意性
//!        （とくにスキーマのペイロード内の `TypeDefId` の重複宣言）、型定義参照の実在
//!        （[`crate::model::SchemaPart::parse`] は `$ref` の実在を見ない）、添付参照の実在
//!        （[`CellValue::Attachment`] はレジストリ登録を強制しない）。したがって保存は
//!        **自分自身の `open` が拒否するファイルを書き出さない**。
//!    - **パート構築とダイジェスト算出**: [`crate::parts::to_parts`]（タスク 4.8）。索引
//!      （`manifest.json`）のダイジェスト算出もこの 1 経路が運ぶ。**ワイヤ形に必要な整合**
//!      （行の値の個数と列数の一致、列名の重複）はここが遮断する。非有限値（NaN / Infinity）の
//!      遮断もここで起きる（行エントリの符号化が [`crate::value::to_json_bytes`] の事前走査を
//!      通る）。
//! 2. **決定的符号化**: [`ContainerCodec::encode`]（タスク 5.2。決定性パラメータは
//!    固定済み。要件 3.1, 3.2, 3.6）。
//! 3. **原子的書き込み**: [`AtomicWriter::commit`]（タスク 5.1）。**書き込みはこの 1 箇所
//!    だけ**であり、`std::fs::write` などを直接使わない。
//!
//! 1〜2 のいずれかで失敗した場合、3 に到達しないため `path` のファイルは一切変更されない。
//! 3 が `Err` を返す場合も対象は保存前のままである（[`AtomicWriter::commit`] の不変条件。
//! タスク 5.1 の裁定）。成功した場合だけ `path` が新しい内容になる。したがって `save` の
//! `Err` は常に「`path` は保存前の内容のまま」を意味する（要件 5.6。design の `save`
//! 事後条件）。
//!
//! **変換後の初回保存の退避（要件 6.4）は後続タスク 7.4 が足す**: 本経路は退避の分岐を
//! 持たず、design の Service Interface のシグネチャも変えない。
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
//! `File::create` / `OpenOptions` / `tempfile` を持たない。書き込みは保存経路
//! （[`DocumentFormatApi::save`]）だけが [`AtomicWriter`] 経由で行う。破損を検出しても
//! 自動修復・上書きをしない。`Err` のとき `path` のファイルは変更されない
//! （要件 5.6 の前提）。
//!
//! [`DocumentFormatApi::to_parts`] / [`DocumentFormatApi::from_parts`] は**ファイル I/O を
//! 一切しない**（`std::fs` も [`crate::container`] も参照しない）: メモリ上のモデルと
//! パート集合の間を往復するだけであり、`version-control` はこの経路だけで中身に到達する。
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

use crate::container::{AtomicWriter, ContainerCodec};
// パート層の自由関数はトレイトの `to_parts` / `from_parts` と同じ名前である。どちらの層の
// 呼び出しかを実装で取り違えないよう、下位層の 1 経路は別名で参照する。
use crate::parts::{
    from_parts as parts_from_parts, to_parts as parts_to_parts, validate_document, DocumentParts,
};

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
/// design の Service Interface が定める 4 メソッドを備える: タスク 7.1 が [`Self::open`] を、
/// 7.2 が [`Self::save`] を、7.3 が [`Self::to_parts`] / [`Self::from_parts`] を定義した
/// （未実装メソッドのスタブは無い）。
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

    /// ドキュメントを決定的な ZIP コンテナとして `path` へ原子的に書き出す。
    ///
    /// 事前条件: `document` がモデル API で構築されていること。
    /// 事後条件: `Ok` の場合、`path` は新しい内容を持つ。`Err` の場合、`path` は保存前の
    /// 内容のまま残る（要件 5.6）。
    /// 不変条件: 同一内容の `document` は、書き込み先のパスによらず常に同一のバイト列になる
    /// （要件 3.1, 3.2, 3.6）。
    ///
    /// 構造的不変条件（同一種別の識別子の一意性、型定義参照・添付参照の実在、
    /// スキーマの存在）は書き込みの前に検証され、違反は診断文脈つきの `Err` で返る
    /// （読み込み経路と同じ検証器。モジュール docs「保存の順序」）。
    ///
    /// 処理順はモジュール docs「保存の順序」にある。ファイル I/O は本メソッドだけが担い、
    /// 不変条件の検証・パート構築・ダイジェスト算出・決定的符号化・原子的書き込みは
    /// 下位層の 1 経路を呼ぶ。変換後の初回保存の退避（要件 6.4）は後続タスク 7.4 が足す。
    fn save(&self, document: &Document, path: &Path) -> Result<(), DocumentError>;

    /// ドキュメントを論理エントリ集合として取り出す（`version-control` 向け。要件 2.2, 2.3）。
    ///
    /// **ZIP を経由しない**: ファイル I/O も [`crate::container`] の型も参照せず、メモリ上の
    /// モデルから [`DocumentParts`] を組み立てるだけである。`version-control` がこの経路
    /// （と [`Self::from_parts`]）だけでドキュメントの中身に到達できる（design の
    /// Implementation Notes「`to_parts` / `from_parts` が `version-control` との唯一の接点」。
    /// [`ContainerCodec`] の直接呼び出しは境界違反）。
    ///
    /// 事前条件: `document` がモデル API で構築されていること。
    /// 事後条件: `Ok` の場合、返る集合は `version-control` がそのまま保管・比較できる
    /// エントリ名の昇順の集合である。`Err` の場合、部分的な集合は返らない。
    ///
    /// 処理順はモジュール docs「論理エントリ集合の公開契約」にある: ①
    /// [`crate::parts::validate_document`] で構造的不変条件を検証し（読み込み経路と同じ
    /// 検証器。これにより**読み込み経路の拒否する集合**を外へ出さない）、②
    /// [`crate::parts::to_parts`] でパートと索引のダイジェストを構築する。
    /// [`Self::save`] も同じ本メソッドを通る（検証と構築の経路を 1 本に保つ）。
    fn to_parts(&self, document: &Document) -> Result<DocumentParts, DocumentError>;

    /// 論理エントリ集合からドキュメントを復元する（要件 2.2, 2.3）。
    ///
    /// [`Self::to_parts`] の逆操作であり、**[`Self::open`] と同じ検証を適用する**:
    /// 実体は [`crate::parts::from_parts`] の 1 経路であり、バージョンゲート・段階的移行・
    /// ダイジェスト照合・構造検証・モデル構築をこの 1 経路が運ぶ（検証ロジックを 2 箇所に
    /// 持たない。design「Validation: `open` と `from_parts` は同一の検証経路を通る」）。
    /// ファイル I/O も ZIP も経由しない。
    ///
    /// 事前条件: `parts` が [`Self::to_parts`] またはコンテナ復号の経路で構築されていること。
    /// 事後条件: `Ok` の場合、返る [`Document`] は全検証を通過している。`Err` の場合、
    /// 部分的なモデルは返らない（要件 5.4）。
    fn from_parts(&self, parts: &DocumentParts) -> Result<Document, DocumentError>;
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
        //    `DocumentFormatApi::from_parts` も同じこの経路を呼ぶ。
        let document = parts_from_parts(&parts)?;

        // 5. 規模の通知（要件 8.4, 8.5）。行は走査せず、シートごとの `len()` の合計だけを
        //    見る（通常経路の性能に影響させない。要件 8.1）。
        Ok(OpenOutcome {
            beyond_supported_scale: exceeds_supported_scale(&document),
            migrated_from,
            document,
        })
    }

    fn save(&self, document: &Document, path: &Path) -> Result<(), DocumentError> {
        // 1. 不変条件の検証とパート構築（`to_parts` の 1 経路。design「保存フロー」の
        //    最初の 2 段）。検証は読み込み経路と同じ検証器をモデルから組み立てた目録へ
        //    1 回掛けるだけであり、規則をこの層に再実装しない。構築では強制されない違反
        //    （TypeDefId の重複宣言・型定義参照の実在・添付参照の実在）はここで遮断され、
        //    書き込みは 1 度も走らない。行の値の個数と列数の不一致・列名の重複・非有限値
        //    （NaN / Infinity）の遮断はパート構築の段で `Err` になる。
        let parts = self.to_parts(document)?;

        // 2. 決定的符号化（タスク 5.2）。同じパート集合からは常に同じバイト列になる。
        let bytes = ContainerCodec::encode(&parts)?;

        // 3. 原子的書き込み（タスク 5.1 の 1 箇所）。`Err` は「対象が置換されていない」
        //    ことを意味する（`commit` の不変条件）。
        AtomicWriter::commit(path, &bytes)
    }

    fn to_parts(&self, document: &Document) -> Result<DocumentParts, DocumentError> {
        // 1. 不変条件の検証（書き出し側の検証）。読み込み経路と同じ検証器をモデルから
        //    組み立てた目録へ 1 回掛けるだけであり、規則をこの層に再実装しない。先に
        //    通すのは、`version-control` が**読み込み経路の拒否する集合**（宙吊り型参照・
        //    未登録添付参照・TypeDefId の重複宣言）を公開経路から手に入れないためである
        //    （タスク 7.2 の裁定）。
        validate_document(document)?;

        // 2. パート構築と索引のダイジェスト算出（タスク 4.8 の 1 経路）。ワイヤ形の整合は
        //    この経路が遮断する。
        parts_to_parts(document)
    }

    fn from_parts(&self, parts: &DocumentParts) -> Result<Document, DocumentError> {
        // 読み込みの 1 経路をそのまま呼ぶ（`open` と同じ検証。検証ロジックを 2 箇所に
        // 持たない）。ゲート・移行・ダイジェスト照合・構造検証・モデル構築はすべて
        // この経路の中にある。
        parts_from_parts(parts)
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

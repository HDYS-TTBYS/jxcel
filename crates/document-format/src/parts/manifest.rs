//! マニフェストパート（タスク 4.2。要件 4.5, 6.1。design「Container Entry Layout」）。
//!
//! `manifest.json` は**唯一の権威ある索引**である（ODF 方式）。OOXML の content-types と
//! rels のような二重帳簿は採らない（design 同節）。本モジュールが所有するのは、
//! その 1 パートが持つ 3 つである:
//!
//! - **形式バージョン** [`FormatVersion`]（要件 6.1）。design の File Structure に従い
//!   [`crate::migration`] が所有する（タスク 6.1 で `error.rs` から移管した）。本モジュールは
//!   その wire 形 `{"major":..,"minor":..}` と、索引への記録・読み出しだけを受け持つ。
//! - **パート索引**: エントリ名（[`EntryName`]）→ BLAKE3 ダイジェスト
//!   （[`Blake3Digest`]）の対応。**どのパートが存在するか**と**各パートの完全性検証値**
//!   （要件 5.1）の唯一の権威であり、別の索引を並置しない。
//! - **索引の順序**: エントリ名の昇順（[`EntryName`] の `Ord` = 表示テキストの辞書順）。
//!   design「DocumentParts」の決定的順序と同じ規則であり、ファイルシステムの列挙順や
//!   入力順に依存しない（要件 3.3 系）。
//!
//! # JSON 形式（本クレートが所有する確定形）
//!
//! コンパクトな UTF-8・末尾改行なしで、以下が確定形である（例は読みやすさのため
//! 改行と空白を入れている）:
//!
//! ```json
//! {
//!   "version": {"major": 1, "minor": 0},
//!   "parts": [
//!     {"name": "document.json", "blake3": "<64 桁小文字 hex>"},
//!     {"name": "sheets/<ulid>.jsonl", "blake3": "<64 桁小文字 hex>"}
//!   ]
//! }
//! ```
//!
//! - トップレベルのキーは `version` → `parts` の順に固定する（[`ManifestPart`] の
//!   宣言順。要件 3.3）。`parts` の要素はエントリ名の昇順。
//! - 形式バージョンは **2 フィールドのオブジェクト** `{"major":..,"minor":..}` として
//!   書く。文字列表現（`"1.0"`）は採らない。[`FormatVersion`] 自身に `Serialize` を
//!   後付けしない（`migration` の型に JSON の都合を持ち込まない）ための wire 専用の写しを
//!   本モジュールが持つ。
//! - 索引要素は `{"name": <エントリ名>, "blake3": <64 桁小文字 hex>}` である。
//!   ダイジェストのテキスト形は [`Blake3Digest`] の正準形（小文字 hex）に限り、大文字 hex は
//!   拒否する（[`EntryName::parse`] が ID 構成要素に求める正準形と同じ規則:
//!   エントリ名は本クレートが正準形で書く唯一の命名である）。
//! - **揮発値を一切含めない**（保存時刻・実行環境・ホスト名・内部処理順。要件 3.6）。
//!   同じ論理内容は、入力順や構築経路が違っても常に同じバイト列になる。
//! - `serde_json` の汎用 JSON 値型・マップ型・`HashMap` を経由しない
//!   （親モジュール [`crate::json`] の規則。キー順序の固定と決定性の根拠）。
//! - 索引の組み立ては要素ごとに差し戻し位置が決まるため、要素を
//!   [`PreservingObjectWriter`] で 1 件ずつ書き出して配列を組み、その配列を raw 値として
//!   トップレベルへ差し込む（[`RawValue`] が verbatim の唯一の経路）。`from_string` は
//!   本クレートが生成した妥当な JSON 配列を 1 回検証するだけで、値を再解釈も再整形もしない。
//!
//! # 未知フィールドの保持（前方互換。要件 6.2 / 6.3）
//!
//! 復号は**トップレベルと索引要素のそれぞれで** [`PreservedFields`] が未知キーを原文の
//! 位置ごと保持し、符号化は [`PreservingObjectWriter`] がその位置へ差し戻す（トップレベルに
//! 1 個、索引要素 1 件につき 1 個。タスク 3.2 の必須条件「1 オブジェクトにつき 1 個」）。
//! したがって将来の minor 版が文書全体のメタデータを足しても、索引要素へ省略可能
//! フィールド（例: 展開前サイズの宣言）を足しても、本実装を通した往復で失われない。
//! `manifest.json` は design が定めた**唯一の権威ある索引**であり、前方互換が破れると
//! 被害が最も大きい構造であるため、索引のどのオブジェクトでも保持を例外にしない。
//!
//! 保持の**対象外**は 2 つだけである: (1) 未知フィールドの**キー**は JSON 文字列として
//! デコードしたテキストで保持するため、原文が `\uXXXX` のようなエスケープ表記を使っていれば
//! その表記は保たれない（値はバイト単位で保たれる。[`PreservedField`] の docs）。
//! (2) 値と区切り・キーと値の間の空白は JSON の値の一部ではないため保たれない。
//! 往復のバイト一致が成立する条件は [`PreservedFields`] の docs と同じ（コンパクトな
//! 入力 + 既知フィールドが宣言順）であり、本クレート自身が書いたファイルはこの条件を満たす。
//!
//! 未知フィールドの差し戻し位置を保つため、既知キーの**重複**（同じ `"version"` が 2 回など）は
//! トップレベル・索引要素のどちらでも [`DocumentError::InvalidContainer`] として拒否する
//! （[`PreservedFields::record_known_field`] と [`PreservingObjectWriter::write_known`] の
//! 呼び出し件数を 1 対 1 に保つため。件数がずれると差し戻し位置がずれる）。
//!
//! # 索引の不変条件（構築時に強制）
//!
//! [`ManifestPart::new`] と [`ManifestPart::from_json_bytes`] は同じ正準化を通る:
//!
//! - **昇順ソート**: 入力順は捨て、エントリ名の昇順へ固定する（上記「索引の順序」）。
//! - **重複の拒否**: 同一エントリ名が 2 回現れる索引は不正である。これはコンテナの
//!   重複パス（要件 2.6）と同じ分類であり、`DuplicateId`（識別子体系の重複）ではない。
//! - **自己参照の拒否**: 索引が `manifest.json` 自身を含むことはできない。マニフェストの
//!   ダイジェストは自分の内容に依存するため記録できず（記録すれば自己参照）、
//!   **マニフェストが信頼の根である**（design「Container Entry Layout」の唯一の索引という
//!   決定の帰結）。他の 5 形は内容を持つパートであり索引に載せられる（形式マーカー
//!   `jxcel` も内容を持つので載せられる）。
//!
//! # ダイジェストは integrity 層の 1 経路で算出する
//!
//! [`ManifestEntry::of_bytes`] は [`crate::integrity::digest_part`]（タスク 4.1）へ委譲し、
//! BLAKE3 を直接呼ばない。算出の実装は `ids.rs` の [`Blake3Digest::of`] 1 箇所に集約されて
//! おり、添付の content-addressed 識別子（要件 7.2）と同一の値を与える。
//!
//! # エラー対応
//!
//! 読み込みの失敗はすべて読み込み全体の中止であり、部分的な結果を返さない（要件 5.4 系）。
//! design のエラー表に本モジュール固有の変種は無いため、コンテンツ解析の失敗は
//! コンテナ不正 [`DocumentError::InvalidContainer`] へ写す（[`crate::value`] /
//! [`crate::json::determinism`] と同じ規約）。
//!
//! | 失敗 | 返す変種と文脈 |
//! |------|----------------|
//! | マニフェストのエントリが無い（[`resolve_manifest`]） | [`DocumentError::MissingPart`]（`name` = `manifest.json`） |
//! | JSON として不正 / トップレベルがオブジェクトでない | [`DocumentError::InvalidContainer`]（`entry` = `manifest.json: <理由>`） |
//! | 必須キー欠落・型違い（`version` / `parts` / `name` / `blake3`） | 同上 |
//! | 形式バージョンの形が違う | 同上 |
//! | ダイジェストが 64 桁小文字 hex でない | 同上 |
//! | 索引内のエントリ名が許可リスト外 | [`DocumentError::InvalidContainer`]（[`EntryName::parse`] が返すもので、`entry` は該当エントリ名そのもの） |
//! | 索引内の重複エントリ名 / 自己参照 | [`DocumentError::InvalidContainer`]（`entry` = `manifest.json: <理由>`） |
//! | 既知キーの重複（トップレベル / 索引要素） | 同上 |
//! | 内部の組み立てが壊れた場合（本クレートが生成した索引配列の raw 化失敗） | 同上（起こり得ない経路だが `panic` しない） |

use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::entry_name::{EntryName, MANIFEST_ENTRY};
use crate::error::DocumentError;
use crate::ids::Blake3Digest;
use crate::json::{PreservedFields, PreservingObjectWriter};
use crate::migration::FormatVersion;

/// トップレベルの既知キー: 形式バージョン（宣言順の 1 番目）。
const VERSION_KEY: &str = "version";
/// トップレベルの既知キー: パート索引（宣言順の 2 番目）。
const PARTS_KEY: &str = "parts";
/// 索引要素の既知キー: エントリ名（宣言順の 1 番目）。
const NAME_KEY: &str = "name";
/// 索引要素の既知キー: BLAKE3 ダイジェスト（宣言順の 2 番目）。
const DIGEST_KEY: &str = "blake3";

/// パート索引の 1 件: エントリ名と、そのパートの内容から算出した BLAKE3 ダイジェスト。
///
/// エントリ名は [`EntryName`]（許可リストの 6 形のいずれか）であり、自由な文字列を
/// 持てない。ダイジェストは [`crate::integrity::digest_part`] が算出した値そのものである
/// （[`ManifestEntry::of_bytes`]）。
///
/// 要素の中の未知キーは 1 件ごとに [`PreservedFields`] が原文の位置ごと保持し、
/// [`ManifestEntry::to_json_bytes`] が差し戻す（[`ManifestPart`] のトップレベルと同じ機構。
/// モジュール docs「未知フィールドの保持」）。
///
/// `PartialEq` は提供しない（保持している未知フィールドの比較に読み込みカーソルが混じるため。
/// [`ManifestPart`] と同じ方針で、内容は [`ManifestEntry::name`] / [`ManifestEntry::digest`]
/// か符号化したバイト列で判定する）。
#[derive(Debug, Clone)]
pub struct ManifestEntry {
    name: EntryName,
    digest: Blake3Digest,
    /// 解釈しない要素内のフィールド（前方互換。要件 6.2 / 6.3）。
    preserved: PreservedFields,
}

impl ManifestEntry {
    /// エントリ名と既知のダイジェストから 1 件を組み立てる。
    pub const fn new(name: EntryName, digest: Blake3Digest) -> Self {
        Self {
            name,
            digest,
            preserved: PreservedFields::new(),
        }
    }

    /// パートの実バイト列からダイジェストを算出して 1 件を組み立てる（保存経路）。
    ///
    /// 算出は [`crate::integrity::digest_part`] へ委譲する（BLAKE3 を直接呼ばない。
    /// モジュール docs「ダイジェストは integrity 層の 1 経路で算出する」）。
    pub fn of_bytes(name: EntryName, bytes: &[u8]) -> Self {
        Self::new(name, crate::integrity::digest_part(bytes))
    }

    /// エントリ名。
    pub const fn name(&self) -> EntryName {
        self.name
    }

    /// 記録された完全性検証値。
    pub const fn digest(&self) -> Blake3Digest {
        self.digest
    }

    /// ダイジェストだけを差し替えた写しを返す（移行の記帳。タスク 6.2）。
    ///
    /// 名前と、要素で保持した未知フィールドはそのまま引き継ぐ（[`ManifestPart::reindexed`]）。
    fn with_digest(&self, digest: Blake3Digest) -> Self {
        Self {
            name: self.name,
            digest,
            preserved: self.preserved.clone(),
        }
    }

    /// 要素 1 件を確定形の JSON オブジェクトとして書き出す。
    ///
    /// キー順序は宣言順（`name` → `blake3`）で、未知キーは
    /// [`PreservedFields::record_known_field`] と対になる位置へ差し戻す。失敗したときは
    /// 1 バイトも書かない（[`PreservingObjectWriter`] の規律を継承）。
    fn to_json_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        let mut out = Vec::new();
        let mut writer = PreservingObjectWriter::new(&mut out, &self.preserved);
        writer.write_known(NAME_KEY, &NameWire(self.name))?;
        writer.write_known(DIGEST_KEY, &self.digest)?;
        writer.finish()?;
        Ok(out)
    }
}

/// マニフェストパート（`manifest.json`）: 形式バージョンと、エントリ名 → BLAKE3
/// ダイジェストの**唯一の権威ある索引**（design「Container Entry Layout」）。
///
/// 索引は常にエントリ名の昇順であり、重複したエントリ名と `manifest.json` 自身を
/// 含まない（モジュール docs「索引の不変条件」）。フィールドは非公開で、不変条件は
/// [`ManifestPart::new`] と [`ManifestPart::from_json_bytes`] の 2 つの構築経路だけが
/// 通る正準化が強制する。
///
/// `PartialEq` は提供しない。保持している未知フィールドの比較には読み込みカーソル
/// （[`PreservedFields`] の内部状態）が混じり、内容の等値を素直に表せないためである。
/// 2 つのパートが同じ内容かどうかは、[`ManifestPart::version`] /
/// [`ManifestPart::entries`] を見るか、符号化したバイト列を比べて判定する
/// （後者が本形式の契約: 同一内容は常に同一バイト列。要件 3.1）。
#[derive(Debug, Clone)]
pub struct ManifestPart {
    version: FormatVersion,
    /// エントリ名の昇順・重複なしの索引（不変条件は構築時に強制される）。
    parts: Vec<ManifestEntry>,
    /// 解釈しないトップレベルのフィールド（前方互換。要件 6.2 / 6.3）。
    preserved: PreservedFields,
}

impl ManifestPart {
    /// 形式バージョンと索引から組み立てる（保存経路）。
    ///
    /// 索引はエントリ名の昇順へ並べ替え、重複エントリ名と `manifest.json` 自身の記録を
    /// [`DocumentError::InvalidContainer`] として拒否する（モジュール docs「索引の不変条件」）。
    pub fn new(version: FormatVersion, entries: Vec<ManifestEntry>) -> Result<Self, DocumentError> {
        Ok(Self {
            version,
            parts: Self::canonicalize(entries)?,
            preserved: PreservedFields::new(),
        })
    }

    /// 記録された形式バージョン（要件 6.1）。
    pub const fn version(&self) -> FormatVersion {
        self.version
    }

    /// エントリ名の昇順の索引。
    pub fn entries(&self) -> &[ManifestEntry] {
        &self.parts
    }

    /// エントリ名に対応するダイジェストを引く。索引に無ければ `None`。
    ///
    /// 索引はエントリ名の昇順（構築時の不変条件）なので二分探索で引く。
    pub fn digest_of(&self, name: &EntryName) -> Option<Blake3Digest> {
        self.entry_of(name).map(|entry| entry.digest)
    }

    /// エントリ名に対応する索引要素を引く（索引に無ければ `None`）。
    ///
    /// 索引はエントリ名の昇順（構築時の不変条件）なので二分探索で引く。
    fn entry_of(&self, name: &EntryName) -> Option<&ManifestEntry> {
        self.parts
            .binary_search_by(|entry| entry.name.cmp(name))
            .ok()
            .map(|index| &self.parts[index])
    }

    /// 版と索引を差し替えた写しを返す（移行の記帳。タスク 6.2）。
    ///
    /// `digests` は集合の実内容から得た（エントリ名, ダイジェスト）の列である。名前が一致する
    /// 索引要素は**未知フィールドを引き継いで**ダイジェストだけを差し替え、一致しない名前
    /// （移行がパートを足した場合）は新しい要素になる。索引に載らない名前（移行がパートを
    /// 消した場合）は落ちる。トップレベルの未知フィールドも引き継ぎ、索引の並びは構築時の
    /// 正準化（エントリ名の昇順・重複なし・自己参照なし）を通す。
    ///
    /// 保持の継承がこの関数の存在理由である: 素の再構築（[`ManifestPart::new`]）は
    /// [`PreservedFields`] を空から始めるため、未知フィールドが黙って落ちる（要件 6.2 / 6.3）。
    /// クレート可視である（呼び出し元は移行チェーンの記帳だけであり、外部に任意の索引を
    /// 組み立てる経路を作らない）。
    pub(crate) fn reindexed(
        &self,
        version: FormatVersion,
        digests: impl IntoIterator<Item = (EntryName, Blake3Digest)>,
    ) -> Result<Self, DocumentError> {
        let entries = digests
            .into_iter()
            .map(|(name, digest)| match self.entry_of(&name) {
                Some(previous) => previous.with_digest(digest),
                None => ManifestEntry::new(name, digest),
            })
            .collect();
        Ok(Self {
            version,
            parts: Self::canonicalize(entries)?,
            preserved: self.preserved.clone(),
        })
    }

    /// 確定形の JSON バイト列へ符号化する（コンパクトな UTF-8・末尾改行なし）。
    ///
    /// キー順序・索引順序・ダイジェストのテキスト形はモジュール docs の確定形に従う。
    /// 未知フィールドはトップレベル・索引要素のそれぞれで原文の位置へ差し戻す。
    /// 失敗したときは `Err` を返し、出力先へ 1 バイトも書かない
    /// （[`PreservingObjectWriter`] の規律を継承）。
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        // 索引は要素ごとに差し戻し位置が決まるため、先に配列全体を組み立ててから
        // raw 値として差し込む（`RawValue` が verbatim の唯一の経路）。
        // `from_string` は本クレートが生成した妥当な JSON 配列を 1 回検証するだけで、
        // 値の再解釈はしない。
        let parts = self.parts_json_bytes()?;
        let parts_raw = RawValue::from_string(String::from_utf8(parts).map_err(invalid_manifest)?)
            .map_err(invalid_manifest)?;

        let mut out = Vec::new();
        {
            // 未知フィールドを原文の位置へ差し戻しながら、既知フィールドを宣言順に書く。
            // 書き出しは `finish` まで一時バッファに留まる（失敗時に 1 バイトも出さない）。
            let mut writer = PreservingObjectWriter::new(&mut out, &self.preserved);
            writer.write_known(VERSION_KEY, &VersionWire::from(self.version))?;
            writer.write_known(PARTS_KEY, parts_raw.as_ref())?;
            writer.finish()?;
        }
        Ok(out)
    }

    /// パート索引を決定的な JSON 配列（エントリ名の昇順）として組み立てる。
    ///
    /// 各要素は自分の未知キーを原文の位置へ差し戻す（[`ManifestEntry::to_json_bytes`]）。
    fn parts_json_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        let mut out = Vec::new();
        out.push(b'[');
        for (index, entry) in self.parts.iter().enumerate() {
            if index > 0 {
                out.push(b',');
            }
            out.extend_from_slice(&entry.to_json_bytes()?);
        }
        out.push(b']');
        Ok(out)
    }

    /// 確定形の JSON バイト列から復号する。
    ///
    /// 失敗は中止であり、部分的な索引を返さない（モジュール docs「エラー対応」の表）。
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, DocumentError> {
        let raw: RawManifest = serde_json::from_slice(bytes).map_err(invalid_manifest)?;
        Self::from_raw(raw)
    }

    /// 構文レベルの読み取り結果を検証し、索引の不変条件を強制して組み立てる。
    ///
    /// 構文・型の失敗（serde）は [`invalid_manifest`] がコンテナ不正へ写し、
    /// ドメイン検証（エントリ名の許可リスト・ダイジェストの正準形・重複・自己参照）は
    /// ここが型付きの文脈で返す。
    fn from_raw(raw: RawManifest) -> Result<Self, DocumentError> {
        if let Some(key) = raw.duplicate_known {
            return Err(invalid_manifest(format!("duplicate field `{key}`")));
        }
        let version = raw
            .version
            .ok_or_else(|| invalid_manifest(format!("missing field `{VERSION_KEY}`")))?;
        let raw_parts = raw
            .parts
            .ok_or_else(|| invalid_manifest(format!("missing field `{PARTS_KEY}`")))?;

        let mut entries = Vec::with_capacity(raw_parts.len());
        for raw_entry in raw_parts {
            let RawEntry {
                name: name_text,
                blake3: digest_text,
                duplicate_known,
                preserved,
            } = raw_entry;
            // 既知キーの重複は差し戻し位置の基準を壊すため拒否する（トップレベルと同じ規則）。
            if let Some(key) = duplicate_known {
                return Err(invalid_manifest(format!(
                    "duplicate field `{key}` in a part index element"
                )));
            }
            // 許可リスト外のエントリ名はエントリ名の分類そのもので拒否する
            // （`EntryName::parse` が返す `InvalidContainer` をそのまま通す）。
            let name = EntryName::parse(&name_text)?;
            let digest = parse_canonical_digest(&digest_text)?;
            entries.push(ManifestEntry {
                name,
                digest,
                preserved,
            });
        }

        Ok(Self {
            version: version.into(),
            parts: Self::canonicalize(entries)?,
            preserved: raw.preserved,
        })
    }

    /// 索引を正準形（昇順・重複なし・自己参照なし）へ整える。両構築経路の共通の門。
    fn canonicalize(mut parts: Vec<ManifestEntry>) -> Result<Vec<ManifestEntry>, DocumentError> {
        if parts.iter().any(|entry| entry.name == MANIFEST_ENTRY) {
            return Err(invalid_manifest(format!(
                "the manifest `{MANIFEST_ENTRY}` cannot index itself"
            )));
        }

        // 入力順は捨て、エントリ名の昇順へ固定する（モジュール docs「索引の順序」）。
        parts.sort_unstable_by(|left, right| left.name.cmp(&right.name));

        if let Some(pair) = parts.windows(2).find(|pair| pair[0].name == pair[1].name) {
            return Err(invalid_manifest(format!(
                "duplicate part entry `{}`",
                pair[0].name
            )));
        }

        Ok(parts)
    }
}

/// 与えられたエントリ名の列から、権威ある索引 `manifest.json` のエントリを解決する。
///
/// 読み込みフロー（design「読み込みフロー」）の「manifest が存在」の判定がこれである。
/// 存在すれば [`MANIFEST_ENTRY`] を返し、無ければ**不足しているエントリ名**を含む
/// [`DocumentError::MissingPart`] として中止する（要件 4.5）。
///
/// エントリ名の列は [`EntryName`] の参照で受けるため、呼び出し元は列を用意するだけでよい
/// （`DocumentParts`（タスク 4.8）は `entries.iter().map(|part| &part.name)` のように渡す。
/// 本関数は集合型を知らない）。
pub fn resolve_manifest<'a, I>(entries: I) -> Result<EntryName, DocumentError>
where
    I: IntoIterator<Item = &'a EntryName>,
{
    if entries.into_iter().any(|entry| *entry == MANIFEST_ENTRY) {
        return Ok(MANIFEST_ENTRY);
    }
    Err(DocumentError::MissingPart {
        name: MANIFEST_ENTRY.to_string(),
    })
}

/// 形式バージョンの wire 形（`{"major":..,"minor":..}`）。
///
/// [`FormatVersion`] 自身に `Serialize` / `Deserialize` を後付けしない（`migration` の型に
/// JSON の都合を持ち込まない）ための写しである。キー順序はフィールド宣言順
/// （`major` → `minor`）。
#[derive(Serialize, Deserialize)]
struct VersionWire {
    major: u32,
    minor: u32,
}

impl From<FormatVersion> for VersionWire {
    fn from(version: FormatVersion) -> Self {
        Self {
            major: version.major,
            minor: version.minor,
        }
    }
}

impl From<VersionWire> for FormatVersion {
    fn from(wire: VersionWire) -> Self {
        Self::new(wire.major, wire.minor)
    }
}

/// エントリ名の wire 形。表示テキストを文字列として書く（`Display` と解析の受理形は
/// [`EntryName::parse`] により 1 対 1 なので、往復で正準形が崩れない）。
struct NameWire(EntryName);

impl Serialize for NameWire {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

/// 索引要素の構文レベルの読み取り結果（検証前）。
///
/// 要素内の未知キーは [`PreservedFields`] が原文の位置ごと保持する（要素 1 件につき 1 個）。
struct RawEntry {
    name: String,
    blake3: String,
    /// 2 回以上現れた既知キー（診断のための記録。件数を 1 対 1 に保つため拒否する）。
    duplicate_known: Option<&'static str>,
    preserved: PreservedFields,
}

impl<'de> Deserialize<'de> for RawEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RawEntryVisitor)
    }
}

/// 索引要素（`{"name":..,"blake3":..}`）を読む訪問者。
struct RawEntryVisitor;

impl<'de> Visitor<'de> for RawEntryVisitor {
    type Value = RawEntry;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a manifest part index element")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawEntry, A::Error> {
        let mut name: Option<String> = None;
        let mut blake3: Option<String> = None;
        let mut duplicate_known: Option<&'static str> = None;
        let mut preserved = PreservedFields::new();

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                NAME_KEY => {
                    if name.is_some() {
                        duplicate_known = Some(NAME_KEY);
                    }
                    name = Some(map.next_value()?);
                    // 書き出し側の `write_known` と同じ順序で件数を進める（必須条件）。
                    preserved.record_known_field();
                }
                DIGEST_KEY => {
                    if blake3.is_some() {
                        duplicate_known = Some(DIGEST_KEY);
                    }
                    blake3 = Some(map.next_value()?);
                    preserved.record_known_field();
                }
                _ => preserved.capture(&key, &mut map)?,
            }
        }

        let Some(name) = name else {
            return Err(de::Error::missing_field(NAME_KEY));
        };
        let Some(blake3) = blake3 else {
            return Err(de::Error::missing_field(DIGEST_KEY));
        };

        Ok(RawEntry {
            name,
            blake3,
            duplicate_known,
            preserved,
        })
    }
}

/// マニフェスト全体の構文レベルの読み取り結果（検証前）。
///
/// 未知のトップレベルキーは [`PreservedFields`] が原文の位置ごと保持する。
struct RawManifest {
    version: Option<VersionWire>,
    parts: Option<Vec<RawEntry>>,
    /// 2 回以上現れた既知キー（診断のための記録。件数を 1 対 1 に保つため拒否する）。
    duplicate_known: Option<&'static str>,
    preserved: PreservedFields,
}

impl<'de> Deserialize<'de> for RawManifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RawManifestVisitor)
    }
}

/// `manifest.json` のトップレベルを読む訪問者（既知キー `version` / `parts` だけを
/// 解釈し、他は [`PreservedFields`] へ回す）。
struct RawManifestVisitor;

impl<'de> Visitor<'de> for RawManifestVisitor {
    type Value = RawManifest;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a manifest part object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawManifest, A::Error> {
        let mut version: Option<VersionWire> = None;
        let mut parts: Option<Vec<RawEntry>> = None;
        let mut duplicate_known: Option<&'static str> = None;
        let mut preserved = PreservedFields::new();

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                VERSION_KEY => {
                    if version.is_some() {
                        duplicate_known = Some(VERSION_KEY);
                    }
                    version = Some(map.next_value()?);
                    // 書き出し側の `write_known` と同じ順序で既知フィールドの件数を
                    // 進める（未知フィールドの差し戻し位置の基準。タスク 3.2 の必須条件）。
                    preserved.record_known_field();
                }
                PARTS_KEY => {
                    if parts.is_some() {
                        duplicate_known = Some(PARTS_KEY);
                    }
                    parts = Some(map.next_value()?);
                    preserved.record_known_field();
                }
                _ => preserved.capture(&key, &mut map)?,
            }
        }

        Ok(RawManifest {
            version,
            parts,
            duplicate_known,
            preserved,
        })
    }
}

/// 64 桁小文字 hex の正準形に限ってダイジェストを解析する。
///
/// [`Blake3Digest::from_hex`] は大文字 hex も通るため、先に正準形（[`Blake3Digest::LEN`]
/// × 2 文字の小文字 hex）でなければ拒否する（[`EntryName::parse`] が ID 構成要素に
/// 求める規則と同じ: エントリ名とダイジェストは本クレートが正準形で書く）。
fn parse_canonical_digest(text: &str) -> Result<Blake3Digest, DocumentError> {
    let canonical = text.len() == Blake3Digest::LEN * 2
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    if !canonical {
        return Err(invalid_manifest(format!("invalid BLAKE3 digest `{text}`")));
    }
    Blake3Digest::from_hex(text)
        .map_err(|_| invalid_manifest(format!("invalid BLAKE3 digest `{text}`")))
}

/// 読み込みのコンテンツ解析失敗を [`DocumentError::InvalidContainer`] へ写す。
///
/// design のエラー表に本モジュール固有の変種が無いための写像であり、`entry` に
/// `manifest.json: <理由>` を残す（[`crate::value`] / [`crate::json::determinism`] と
/// 同じ規約）。
fn invalid_manifest(reason: impl fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("{MANIFEST_ENTRY}: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::digest_part;

    /// シート識別子の標本（正準 Crockford base32 大文字 26 文字）。2 つ目は 1 つ目より
    /// 辞書順で後（入力順・ULID 昇順・辞書順を混ぜて索引順を実測するため）。
    const SHEET_A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const SHEET_B: &str = "01BX5ZZKBKACTAV9WEVGEMMVRY";

    /// 添付の標本（64 文字小文字 hex）。`HEX_A` は索引に載せない（引き当ての負例）。
    const HEX_A: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const HEX_B: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    /// パートの実バイト列の標本（パートごとに長さも内容も異なる）。
    const DOCUMENT_BYTES: &[u8] = b"{\"document_id\":\"01ARZ3NDEKTSV4RRFFQ69G5FAV\"}";
    const SCHEMA_BYTES: &[u8] = b"{\"root\":{}}";
    const ROWS_A_BYTES: &[u8] = b"{\"a\":1}\n{\"a\":2}\n";
    const ROWS_B_BYTES: &[u8] = b"{\"b\":1}\n";
    const ATTACHMENT_BYTES: &[u8] = &[0xff, 0x00, 0x80, 0x01];

    /// 6 形すべての標本（名前, 実バイト列）。索引に載せられる形を 1 形 1 個ずつ。
    fn sample_cases() -> Vec<(String, &'static [u8])> {
        vec![
            ("jxcel".to_owned(), b"jxcel"),
            ("document.json".to_owned(), DOCUMENT_BYTES),
            (format!("sheets/{SHEET_B}.jsonl"), ROWS_B_BYTES),
            (format!("schemas/{SHEET_A}.json"), SCHEMA_BYTES),
            (format!("sheets/{SHEET_A}.jsonl"), ROWS_A_BYTES),
            (format!("attachments/{HEX_B}.bin"), ATTACHMENT_BYTES),
        ]
    }

    /// 標本の索引。並びは**昇順ではない**（入力順が出力へ漏れないことを確かめるため）。
    fn samples() -> Vec<ManifestEntry> {
        sample_cases()
            .into_iter()
            .map(|(name, bytes)| entry(&name, bytes))
            .collect()
    }

    /// 実バイト列から索引の 1 件を組み立てる（保存経路と同じ入口）。
    fn entry(name: &str, bytes: &[u8]) -> ManifestEntry {
        ManifestEntry::of_bytes(EntryName::parse(name).expect("標本は許可リスト内"), bytes)
    }

    /// 標本 6 件の昇順（表示テキストの辞書順）。`samples()` の並びとは異なる。
    ///
    /// 期待値を [`EntryName`] の `Ord` で計算せず**文字列で固定する**のは、並びの定義
    /// （辞書順）そのものを回帰対象にするためである。
    fn expected_ascending_names() -> Vec<String> {
        vec![
            format!("attachments/{HEX_B}.bin"),
            "document.json".to_owned(),
            "jxcel".to_owned(),
            format!("schemas/{SHEET_A}.json"),
            format!("sheets/{SHEET_A}.jsonl"),
            format!("sheets/{SHEET_B}.jsonl"),
        ]
    }

    fn names(part: &ManifestPart) -> Vec<String> {
        part.entries()
            .iter()
            .map(|entry| entry.name().to_string())
            .collect()
    }

    /// 索引の内容（エントリ名とダイジェスト）を比較可能な形へ写す。
    ///
    /// [`ManifestEntry`] / [`ManifestPart`] は保持している未知フィールド（内部カーソルを
    /// 含む）を等値比較に持ち込まないため `PartialEq` を持たない。内容の比較はこの写像か
    /// 符号化したバイト列で行う。
    fn fingerprint(part: &ManifestPart) -> Vec<(EntryName, Blake3Digest)> {
        part.entries()
            .iter()
            .map(|entry| (entry.name(), entry.digest()))
            .collect()
    }

    fn manifest(version: FormatVersion, entries: Vec<ManifestEntry>) -> ManifestPart {
        ManifestPart::new(version, entries).expect("標本の索引は妥当")
    }

    fn rows_a() -> EntryName {
        EntryName::parse(&format!("sheets/{SHEET_A}.jsonl")).expect("標本は許可リスト内")
    }

    /// 往復同一: 符号化 → 復号が元と一致し、バイト列も変わらない。同じ入力の 2 回の
    /// 符号化はバイト単位で一致する（要件 3.1, 3.6）。
    #[test]
    fn encodes_and_decodes_byte_identically_and_deterministically() {
        let part = manifest(FormatVersion::new(1, 0), samples());
        let bytes = part.to_json_bytes().expect("符号化");

        assert_eq!(
            bytes,
            part.to_json_bytes().expect("符号化"),
            "同一内容の 2 回の符号化がバイト一致しない"
        );

        let decoded = ManifestPart::from_json_bytes(&bytes).expect("復号");
        assert_eq!(part.version(), decoded.version(), "復号で形式バージョンが変わった");
        assert_eq!(fingerprint(&part), fingerprint(&decoded), "復号で索引が変わった");
        assert_eq!(
            bytes,
            decoded.to_json_bytes().expect("再符号化"),
            "往復でバイト列が変わった"
        );
        assert_eq!(sample_cases().len(), decoded.entries().len());
    }

    /// 索引順序: 入力順を変えても、エントリ名の昇順（辞書順）で出る。標本は ULID 2 種と
    /// 5 形を混ぜてあり、ULID の昇順・入力順・辞書順が一致しない（要件 3.3 系）。
    #[test]
    fn index_is_in_entry_name_ascending_order_independent_of_input_order() {
        let ascending = expected_ascending_names();

        let reversed: Vec<ManifestEntry> = samples().into_iter().rev().collect();
        let rotated: Vec<ManifestEntry> = {
            let mut rotated = samples();
            rotated.rotate_left(3);
            rotated
        };

        let mut encodings = Vec::new();
        for entries in [samples(), reversed, rotated] {
            let input_names: Vec<String> =
                entries.iter().map(|entry| entry.name().to_string()).collect();
            assert_ne!(ascending, input_names, "標本の入力順が既に昇順で、順序の検証にならない");

            let part = manifest(FormatVersion::new(1, 0), entries);
            assert_eq!(ascending, names(&part), "索引がエントリ名の昇順でない");
            encodings.push(part.to_json_bytes().expect("符号化"));
        }

        assert_eq!(encodings[0], encodings[1], "入力順の違いがバイト列へ漏れている");
        assert_eq!(encodings[0], encodings[2], "入力順の違いがバイト列へ漏れている");
    }

    /// 確定形: キー名・キー順序・入れ子の形・ダイジェストのテキスト形を文字列で固定する
    /// （モジュール docs「JSON 形式」の確定形そのもの）。
    #[test]
    fn wire_form_is_the_documented_confirmed_shape() {
        let digest = digest_part(DOCUMENT_BYTES).to_hex();
        let part = manifest(
            FormatVersion::new(1, 0),
            vec![entry("document.json", DOCUMENT_BYTES)],
        );

        let text = String::from_utf8(part.to_json_bytes().expect("符号化")).expect("UTF-8");
        let expected = format!(
            r#"{{"version":{{"major":1,"minor":0}},"parts":[{{"name":"document.json","blake3":"{digest}"}}]}}"#
        );
        assert_eq!(expected, text, "確定形が変わっている");
    }

    /// 形式バージョンは往復で保存される（major / minor の組を 2 通り。要件 6.1）。
    #[test]
    fn format_version_survives_the_round_trip() {
        for version in [FormatVersion::new(1, 0), FormatVersion::new(2, 7)] {
            let part = manifest(version, samples());
            assert_eq!(version, part.version());

            let decoded = ManifestPart::from_json_bytes(&part.to_json_bytes().expect("符号化"))
                .expect("復号");
            assert_eq!(version, decoded.version(), "{version} が往復で変わった");
            assert_eq!(
                fingerprint(&part),
                fingerprint(&decoded),
                "{version} の往復で索引が変わった"
            );
        }
    }

    /// 未知フィールド保持: 未知キーがトップレベルと索引要素の両方に現れても、読み書きで
    /// **バイト単位**に元へ戻る（要件 6.2 / 6.3）。
    #[test]
    fn unknown_top_level_fields_round_trip_byte_for_byte() {
        let parts = format!(
            "[{},{}]",
            entry_json("document.json", DOCUMENT_BYTES),
            entry_json(&format!("sheets/{SHEET_A}.jsonl"), ROWS_A_BYTES)
        );
        let version = r#"{"major":2,"minor":7}"#;

        let cases = [
            // 前（既知フィールドの手前）
            format!(r#"{{"future_a":1,"version":{version},"parts":{parts}}}"#),
            // 間（version と parts のあいだ）
            format!(r#"{{"version":{version},"future_b":[1,2,{{"n":null}}],"parts":{parts}}}"#),
            // 後（既知フィールドの後ろ）
            format!(r#"{{"version":{version},"parts":{parts},"future_c":"x"}}"#),
            // 入れ子: トップレベルの未知キーと索引要素の未知キーが同時に現れる
            format!(
                r#"{{"version":{version},"future_meta":{{"unit":"mm"}},"parts":[{{"name":"document.json","future_unit":"mm","blake3":"{}"}}]}}"#,
                digest_part(DOCUMENT_BYTES).to_hex()
            ),
        ];

        for input in cases {
            let decoded = ManifestPart::from_json_bytes(input.as_bytes()).expect("復号");
            let written = decoded.to_json_bytes().expect("再符号化");
            assert_eq!(
                input.as_bytes(),
                written.as_slice(),
                "未知フィールドが往復で消えたか位置が変わった: {input}"
            );
        }
    }

    /// 要素内の未知フィールド保持: 索引要素の中の未知キーが「先頭・間・末尾」のどこにあっても、
    /// 読み→書き戻しが**バイト単位**に一致する（要件 6.2 / 6.3。将来の minor が索引要素へ
    /// 足した省略可能フィールドが消えない）。
    ///
    /// 未知キーを持つ要素 3 つと持たない要素 1 つを混ぜる（1 要素だけの検証では、要素ごとの
    /// 保持が壊れても検出できない）。
    #[test]
    fn unknown_keys_inside_index_elements_round_trip_byte_for_byte() {
        let attachments = entry_json(&format!("attachments/{HEX_B}.bin"), ATTACHMENT_BYTES);
        // 先頭（name の手前）と間（name と blake3 のあいだ）
        let document = format!(
            r#"{{"future_unit":"mm","name":"document.json","declared_size":7,"blake3":"{}"}}"#,
            digest_part(DOCUMENT_BYTES).to_hex()
        );
        // 間
        let schema = format!(
            r#"{{"name":"schemas/{SHEET_A}.json","compression":"deflate","blake3":"{}"}}"#,
            digest_part(SCHEMA_BYTES).to_hex()
        );
        // 末尾（blake3 の後ろ）
        let rows = format!(
            r#"{{"name":"sheets/{SHEET_A}.jsonl","blake3":"{}","version_added":2}}"#,
            digest_part(ROWS_A_BYTES).to_hex()
        );
        // 索引順（エントリ名の昇順）に並べた入力
        let input = format!(
            r#"{{"version":{{"major":1,"minor":0}},"parts":[{attachments},{document},{schema},{rows}]}}"#
        );

        let decoded = ManifestPart::from_json_bytes(input.as_bytes()).expect("復号");
        assert_eq!(4, decoded.entries().len());
        // 未知キーは既知フィールドを隠さない（索引として正しく読める）。
        assert_eq!(
            Some(digest_part(DOCUMENT_BYTES)),
            decoded.digest_of(&EntryName::Document)
        );

        let written = decoded.to_json_bytes().expect("再符号化");
        assert_eq!(
            input.as_bytes(),
            written.as_slice(),
            "要素内の未知フィールドが往復で消えたか位置が変わった: {input}"
        );

        // raw 値の差し込みを含む再符号化が安定している（復号 → 符号化を繰り返しても同じ）。
        let redecoded = ManifestPart::from_json_bytes(&written).expect("復号");
        assert_eq!(written, redecoded.to_json_bytes().expect("再符号化"));
        assert_eq!(fingerprint(&decoded), fingerprint(&redecoded));
    }

    /// 要素の JSON 1 件（確定形のつづりで組み立てる。符号化経路に依存しない）。
    fn entry_json(name: &str, bytes: &[u8]) -> String {
        format!(
            r#"{{"name":"{name}","blake3":"{}"}}"#,
            digest_part(bytes).to_hex()
        )
    }

    /// 要件 4.5: マニフェストが無い場合、**不足しているエントリ名を含む**
    /// [`DocumentError::MissingPart`] で中止する。ある場合はそのエントリ名が返る。
    #[test]
    fn resolve_manifest_returns_the_manifest_entry_or_reports_missing_part_by_name() {
        for entries in [
            vec![MANIFEST_ENTRY, EntryName::Document, rows_a()],
            vec![rows_a(), EntryName::Document, MANIFEST_ENTRY],
        ] {
            let resolved = resolve_manifest(entries.iter()).expect("manifest がある");
            assert_eq!(MANIFEST_ENTRY, resolved, "解決結果が manifest でない");
        }

        for entries in [
            Vec::<EntryName>::new(),
            vec![EntryName::Document],
            vec![EntryName::Document, rows_a()],
        ] {
            match resolve_manifest(entries.iter()) {
                Err(DocumentError::MissingPart { name }) => assert_eq!(
                    MANIFEST_ENTRY.to_string(),
                    name,
                    "不足パート名が manifest.json でない"
                ),
                other => panic!("MissingPart 以外が返った: {other:?}"),
            }
        }
    }

    /// ダイジェストは integrity 層の算出そのものであり、エントリ名で引ける
    /// （BLAKE3 を独自に呼んでいない）。載っていないエントリ名は `None`。
    #[test]
    fn digests_come_from_the_integrity_primitive_and_are_looked_up_by_name() {
        let part = manifest(FormatVersion::new(1, 0), samples());

        for (name_text, bytes) in sample_cases() {
            let name = EntryName::parse(&name_text).expect("標本は許可リスト内");
            assert_eq!(
                Some(digest_part(bytes)),
                part.digest_of(&name),
                "{name_text}: 記録されたダイジェストが integrity の算出と違う"
            );
        }

        assert_eq!(None, part.digest_of(&MANIFEST_ENTRY), "索引に自分自身が載っている");
        assert_eq!(
            None,
            part.digest_of(&EntryName::parse(&format!("attachments/{HEX_A}.bin")).unwrap()),
            "載っていないエントリ名が引けた"
        );
    }

    /// 不正入力はすべて [`DocumentError::InvalidContainer`] で拒否される
    /// （モジュール docs のエラー表）。
    #[test]
    fn invalid_manifest_inputs_are_rejected_as_invalid_container() {
        let digest = digest_part(DOCUMENT_BYTES).to_hex();
        let element = format!(r#"{{"name":"document.json","blake3":"{digest}"}}"#);
        let short = &digest[..62];
        let non_hex = format!("g{}", &digest[1..]);
        let upper = digest.to_uppercase();

        let cases: Vec<(&str, String)> = vec![
            ("JSON として不正", "{".to_owned()),
            ("トップレベルがオブジェクトでない", "[]".to_owned()),
            ("トップレベルが空", String::new()),
            (
                "version 欠落",
                format!(r#"{{"parts":[{element}]}}"#),
            ),
            ("parts 欠落", r#"{"version":{"major":1,"minor":0}}"#.to_owned()),
            ("version が数値", r#"{"version":1,"parts":[]}"#.to_owned()),
            ("version が文字列", r#"{"version":"1.0","parts":[]}"#.to_owned()),
            (
                "major が数値でない",
                r#"{"version":{"major":"1","minor":0},"parts":[]}"#.to_owned(),
            ),
            ("minor 欠落", r#"{"version":{"major":1},"parts":[]}"#.to_owned()),
            (
                "version が負",
                r#"{"version":{"major":-1,"minor":0},"parts":[]}"#.to_owned(),
            ),
            (
                "parts が配列でない",
                r#"{"version":{"major":1,"minor":0},"parts":{}}"#.to_owned(),
            ),
            (
                "parts が null",
                r#"{"version":{"major":1,"minor":0},"parts":null}"#.to_owned(),
            ),
            (
                "索引要素がオブジェクトでない",
                r#"{"version":{"major":1,"minor":0},"parts":["document.json"]}"#.to_owned(),
            ),
            (
                "索引要素に name が無い",
                format!(r#"{{"version":{{"major":1,"minor":0}},"parts":[{{"blake3":"{digest}"}}]}}"#),
            ),
            (
                "索引要素に blake3 が無い",
                r#"{"version":{"major":1,"minor":0},"parts":[{"name":"document.json"}]}"#
                    .to_owned(),
            ),
            (
                "blake3 が数値",
                r#"{"version":{"major":1,"minor":0},"parts":[{"name":"document.json","blake3":7}]}"#
                    .to_owned(),
            ),
            (
                "blake3 が短い",
                format!(r#"{{"version":{{"major":1,"minor":0}},"parts":[{{"name":"document.json","blake3":"{short}"}}]}}"#),
            ),
            (
                "blake3 が非 hex 文字を含む",
                format!(r#"{{"version":{{"major":1,"minor":0}},"parts":[{{"name":"document.json","blake3":"{non_hex}"}}]}}"#),
            ),
            (
                "blake3 が大文字 hex",
                format!(r#"{{"version":{{"major":1,"minor":0}},"parts":[{{"name":"document.json","blake3":"{upper}"}}]}}"#),
            ),
            (
                "エントリ名が許可リスト外",
                format!(r#"{{"version":{{"major":1,"minor":0}},"parts":[{{"name":"bogus.txt","blake3":"{digest}"}}]}}"#),
            ),
            (
                "エントリ名の大小が正準形でない",
                format!(r#"{{"version":{{"major":1,"minor":0}},"parts":[{{"name":"document.JSON","blake3":"{digest}"}}]}}"#),
            ),
            (
                "索引が自分自身を含む",
                format!(r#"{{"version":{{"major":1,"minor":0}},"parts":[{{"name":"manifest.json","blake3":"{digest}"}}]}}"#),
            ),
            (
                "索引内の重複エントリ名",
                format!(r#"{{"version":{{"major":1,"minor":0}},"parts":[{element},{element}]}}"#),
            ),
            (
                "既知キーの重複",
                format!(
                    r#"{{"version":{{"major":1,"minor":0}},"version":{{"major":1,"minor":0}},"parts":[{element}]}}"#
                ),
            ),
            (
                "索引要素の既知キーの重複",
                format!(
                    r#"{{"version":{{"major":1,"minor":0}},"parts":[{{"name":"document.json","name":"document.json","blake3":"{digest}"}}]}}"#
                ),
            ),
        ];

        for (why, input) in cases {
            match ManifestPart::from_json_bytes(input.as_bytes()) {
                Err(DocumentError::InvalidContainer { .. }) => {}
                other => panic!("{why}: InvalidContainer 以外が返った: {other:?}"),
            }
        }
    }

    /// エントリ名の拒否は該当エントリ名を `entry` に保持する（`entry_name.rs` と同じ分類。
    /// 許可リスト外のエントリ名はコンテナ層のエラー）。
    #[test]
    fn a_rejected_entry_name_is_kept_verbatim_in_the_error() {
        let digest = digest_part(DOCUMENT_BYTES).to_hex();
        let input = format!(
            r#"{{"version":{{"major":1,"minor":0}},"parts":[{{"name":"sheets/01arz3ndektsv4rrffq69g5fav.jsonl","blake3":"{digest}"}}]}}"#
        );

        match ManifestPart::from_json_bytes(input.as_bytes()) {
            Err(DocumentError::InvalidContainer { entry }) => {
                assert_eq!("sheets/01arz3ndektsv4rrffq69g5fav.jsonl", entry);
            }
            other => panic!("InvalidContainer 以外が返った: {other:?}"),
        }
    }

    /// 構築経路（[`ManifestPart::new`]）も復号と同じ不変条件を強制する: 重複エントリ名と
    /// 自己参照は索引として不正である。回帰は入力順に依らず検出される（重複は昇順へ
    /// 並べた後の隣接比較で見るため）。
    #[test]
    fn constructor_rejects_duplicates_and_self_reference() {
        let duplicate = vec![
            entry("document.json", DOCUMENT_BYTES),
            entry("document.json", SCHEMA_BYTES),
        ];
        assert!(
            matches!(
                ManifestPart::new(FormatVersion::new(1, 0), duplicate),
                Err(DocumentError::InvalidContainer { .. })
            ),
            "重複エントリ名が拒否されない"
        );

        let self_reference = vec![ManifestEntry::new(MANIFEST_ENTRY, digest_part(b"{}"))];
        assert!(
            matches!(
                ManifestPart::new(FormatVersion::new(1, 0), self_reference),
                Err(DocumentError::InvalidContainer { .. })
            ),
            "索引の自己参照が拒否されない"
        );
    }

    /// 出力に揮発値を含まない（要件 3.6）: 同じ論理内容を別の入力順で組み立てても
    /// バイト列は一致し、復号 → 再符号化でも変わらない。
    #[test]
    fn encoding_is_free_of_volatile_values() {
        let ascending_inputs = manifest(FormatVersion::new(2, 7), samples());
        let reversed_inputs = manifest(
            FormatVersion::new(2, 7),
            samples().into_iter().rev().collect(),
        );

        let bytes = ascending_inputs.to_json_bytes().expect("符号化");
        assert_eq!(
            bytes,
            reversed_inputs.to_json_bytes().expect("符号化"),
            "構築経路の違いが出力へ漏れている"
        );
        assert_eq!(
            bytes,
            ascending_inputs.to_json_bytes().expect("符号化"),
            "2 回目の符号化で出力が変わった"
        );

        let decoded = ManifestPart::from_json_bytes(&bytes).expect("復号");
        assert_eq!(FormatVersion::new(2, 7), decoded.version());
        assert_eq!(bytes, decoded.to_json_bytes().expect("再符号化"));
    }
}

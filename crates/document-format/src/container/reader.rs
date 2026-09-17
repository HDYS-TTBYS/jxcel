//! ZIP の読み込みと不正コンテナの拒否（タスク 5.3。要件 2.5, 2.6。design「Container Layer /
//! ContainerCodec」「Container Layer / EntryLayout」「Security Considerations / zip-slip」）。
//!
//! 本モジュールの責務は、ZIP アーカイブのバイト列を**検証しながら**読んで論理エントリ集合
//! （[`DocumentParts`]）へ戻すことだけである。書き出しは [`super::writer`] が担い、両者は
//! 同じ型 [`ContainerCodec`] の associated function である（型を分けない。タスク 5.2 の
//! 申し送り）。
//!
//! # 確定形
//!
//! `ContainerCodec::decode(bytes: &[u8]) -> Result<DocumentParts, DocumentError>`。
//! 受理した集合は [`DocumentParts`] の不変条件（エントリ名の昇順・重複なし・
//! `manifest.json` を含む）を満たし、`decode(encode(p))` は `p` と名前・バイト列が一致する。
//!
//! # 判定順（この順序が本質である）
//!
//! 1. **ZIP として開く**。開けなければ中止。アーカイブ全体の失敗には実在するエントリ名が
//!    無いため、実在しないことが自明な擬似名 `"<archive>"` を使い理由を添える。
//! 2. **許可リスト照合**（[`super::layout::admit`]）。[`EntryName::parse`] が唯一の権威で
//!    あり、`zip` の `enclosed_name()` のような正規化には依存しない（要件 2.5）。
//! 3. **同一パスの重複検出**（同上。要件 2.6）。バイト単位の完全一致で判定する。
//! 4. **型マーカー** `jxcel`。実在すること、内容が `jxcel\n<major>.<minor>\n` として解釈
//!    できること、運ぶバージョンが索引（`manifest.json`）の記録値と一致することを要求する。
//! 5. **展開とサイズ照合**。各エントリを宣言サイズ（下記）を上限に有界に読み、実際の展開長が
//!    宣言と一致しなければ中止する。
//! 6. **論理エントリ集合の構築**。取り出した `(EntryName, 展開後バイト列)` を
//!    [`DocumentParts::from_entries`] に渡す（型マーカーは渡さない）。集合の構造検証は
//!    ここでは行わない（各パートの復号と検証は [`crate::parts`] の責務）。
//!
//! **展開（5）は 2 と 3 が終わるまで始まらない。** 名前の列だけで決まる判定をすべて先に
//! 済ませることで、名前が不正なアーカイブは（中身が壊れていても、あるいは巨大に膨張しても）
//! 1 バイトも展開せずに拒否される。単体テスト
//! `decode_checks_the_allow_list_before_expanding` がこの先行を実測で固定する。
//!
//! # 重複検出は中央ディレクトリを生バイトで走査する
//!
//! `zip` crate の `ZipArchive` はエントリを**名前をキーにした `IndexMap`** で保持するため、
//! 同一名のエントリを複数持つアーカイブを開くと**黙って 1 件に畳む**（`len()` も
//! `by_index()` も畳んだ後の値しか見えない）。したがって重複の検出だけは公開 API からは
//! 行えない。本モジュールは `ZipArchive::central_directory_start()` が返す位置から
//! **中央ディレクトリの固定ヘッダを生バイトで走査**し、重複を含む生のエントリ名の列を作る
//! （`raw_entry_names`）。走査するのは名前だけであり、内容には触れない。
//!
//! 名前の列は ZIP のバイト列表現（UTF-8。7 形はすべて ASCII）として得る。UTF-8 として
//! 読めない生の名前は 7 形のいずれにも一致し得ないため、この段階で拒否する
//! （唯一 `EntryName::parse` を経由しない拒否であり、理由は「`parse` が `&str` を要求する」
//! ことに尽きる。`&str` でない名前はこの 7 形の外である）。
//!
//! # 宣言サイズの出所（マニフェストにサイズ欄を足さない）
//!
//! 照合に使う「宣言サイズ」は **ZIP ヘッダ（中央ディレクトリ）の非圧縮サイズ**である。
//! タスク記述は「マニフェストの宣言サイズ」と言うが、[`crate::parts::ManifestEntry`] は
//! `name` / `digest` / `preserved` だけを持ち**サイズ欄を持たない**。マニフェストにサイズ欄を
//! 足すのは**ワイヤ形式の変更**であり、`manifest.json` の確定形を固定している
//! ゴールデン（`tests/fixtures/bytes/golden_container.zip`）を無効化するため、本タスクでは
//! 行わない。展開前に判明する宣言サイズは ZIP ヘッダの値だけであり、それを用いる。
//!
//! # 絶対的なサイズ上限を設けない理由（要件 8.5 と多層防御）
//!
//! 本モジュールは**バイト数のしきい値による上限を持たない**。要件 8.5 が「10 万行を超えて
//! も読み込みを拒否せず、性能保証の対象外である旨を通知する」と定めており、正当に大きな
//! ドキュメントを拒否してはならないためである。加えて DEFLATE の圧縮比には理論上限があり
//! （最良でも約 1032:1）、比による判定は実効的な防御にならない。
//!
//! したがって「極端に膨張するアーカイブ」への防御は次の多層で構成する:
//!
//! - (a) 許可リストと重複の判定が展開に先行すること（不正な名前のアーカイブは展開しない）
//! - (b) 宣言サイズを上限とした**有界読み**（宣言 + 1 バイトだけ読む）
//! - (c) 実際の展開長が宣言と一致することの要求（不一致は中止）
//! - (d) 後段のダイジェスト照合（[`crate::integrity`]。索引が記録した BLAKE3 と実体の照合は
//!   [`crate::parts::from_parts`] / `open` 経路 = タスク 7.1 が行う）
//!
//! タスク 8.5（不正アーカイブの拒否）は本判断を前提にする。上限を導入する場合は
//! 要件 8.5 との衝突を先に解消すること（**申し送り**）。
//!
//! # 型マーカー（位置に依存しない）
//!
//! `jxcel` は**コンテナ層のエントリ**であり [`DocumentParts`] には含めない（タスク 4.8 の
//! 裁定）。復号は展開した内容を集合から外し、[`DocumentParts::from_entries`] へ渡さない
//! （渡すと `parts` 層が重複した規則で拒否する）。マーカーの内容は
//! `jxcel\n<major>.<minor>\n` の ASCII で、バージョンは正準 10 進表記（先頭ゼロなし）に限る。
//!
//! **マーカーの位置は要求しない**。要件 2.1 は汎用 ZIP ツールでの展開・再圧縮を想定して
//! おり、エントリの並び順はアーカイブ作成者に委ねられる。書き出し側（[`super::writer`]）は
//! `Stored`・先頭に置くが、復号側はそれを要求しない（`Deflate` でも、先頭以外でも読める）。
//!
//! マーカーが運ぶバージョンと索引の記録値が食い違う場合は**破損として中止する**
//! （タスク 5.2 の裁定: マーカーは固定オフセットでの早期判定のための**写し**であり、
//! 権威は常に索引。写しと索引の不一致はどちらかが壊れている）。恒久的なバージョンゲートは
//! 索引側、すなわち [`crate::migration::MigrationChain`] にあり、読み込み経路
//! （[`crate::parts::from_parts`]）が掛ける。本モジュールは版の可否を判定せず、
//! マーカーと索引の一致だけを見る。
//!
//! # エラー
//!
//! 新しい [`DocumentError`] 変種は足さない（design のエラー表は閉じている）。判別可能な
//! 変種が無い失敗は [`DocumentError::InvalidContainer`] の `entry` に
//! `<エントリ名>: <理由>`（名前が無い場合は `<archive>: <理由>`）を載せる。理由の文言は
//! ログ用の技術的診断であり、提示文言は呼び出し元が組み立てる（[`crate::error`] の規約）。
//!
//! | 失敗 | 返す変種と文脈 |
//! |------|----------------|
//! | ZIP として開けない | [`DocumentError::InvalidContainer`]（`entry` = `<archive>: <理由>`） |
//! | 中央ディレクトリが走査できない / 生の名前が UTF-8 でない | 同上（`entry` = `<archive>: <理由>`。後者は生の名前の損失つき表示） |
//! | 走査した名前が `zip` の読み取り結果に見つからない | 同上（`entry` = `<名前>: the entry is missing from the archive`） |
//! | 許可リスト外のエントリ名 | 同上（`entry` = 生のエントリ名そのもの。要件 2.5） |
//! | 同一パスの重複 | 同上（`entry` = `<エントリ名>: duplicate entry path`。要件 2.6） |
//! | 型マーカーが無い / 解釈できない / 索引の記録値と食い違う | 同上（`entry` = `jxcel: <理由>`） |
//! | 宣言サイズと実際の展開長が違う | 同上（`entry` = `<エントリ名>: declared size <N> does not match expanded size <M>`） |
//! | エントリの展開に失敗（破損した圧縮データ等） | 同上（`entry` = `<エントリ名>: <理由>`） |
//! | 索引が無い / パートの復号に失敗 | [`crate::parts`] の各経路が返すもの（本モジュールは素通しする） |
//!
//! # 依存
//!
//! `zip` crate を参照してよいのは `container/` 配下だけである（design「ContainerCodec /
//! Implementation Notes」）。本モジュールはコンテナ層の他のモジュール（[`super::writer`] /
//! [`super::layout`]）と [`crate::parts`] にのみ依存し、[`super::layout`] 自身は `zip` を
//! 参照しない（純粋な判定。単体テストは `zip` 無しで書ける）。

use std::fmt;
use std::io::{Cursor, Read};

use zip::result::ZipError;
use zip::ZipArchive;

use crate::entry_name::EntryName;
use crate::error::DocumentError;
use crate::migration::FormatVersion;
use crate::parts::DocumentParts;

use super::layout;
use super::writer::ContainerCodec;

/// 型マーカーの骨格（先頭側）: 形式名 `jxcel` と LF。
///
/// [`super::writer::marker_bytes`] が組み立てる確定形と対である。`writer.rs` の
/// `MARKER_PREFIX` / `MARKER_SUFFIX` は非公開であり、本タスクでは `writer.rs` を変更しない
/// ため、解析側の骨格リテラルは本モジュールが持つ。両者の一致は単体テスト
/// `parse_marker_accepts_every_marker_bytes`（往復）が固定する。
const MARKER_PREFIX: &[u8] = b"jxcel\n";

/// 型マーカーの骨格（末尾側）: LF。
const MARKER_SUFFIX: &[u8] = b"\n";

/// アーカイブ全体の失敗を報告するための擬似エントリ名。
///
/// 実在するエントリ名が無い失敗（ZIP として開けない等）に使う。許可リストの 7 形の
/// いずれとも一致しない綴りであり、実在のエントリ名と取り違えられない。
const ARCHIVE_ENTRY: &str = "<archive>";

/// 中央ディレクトリのシグネチャ（`PK\x01\x02`）。
const CENTRAL_SIGNATURE: &[u8; 4] = b"PK\x01\x02";

/// 中央ディレクトリの固定ヘッダ長（可変長の名前・拡張・コメントを除く）。
const CENTRAL_HEADER_LEN: usize = 46;

/// 中央ディレクトリ固定ヘッダ内のエントリ名長（16 bit LE）の位置。
const CENTRAL_NAME_LENGTH_OFFSET: usize = 28;

/// 中央ディレクトリ固定ヘッダ内の拡張フィールド長（16 bit LE）の位置。
const CENTRAL_EXTRA_LENGTH_OFFSET: usize = 30;

/// 中央ディレクトリ固定ヘッダ内のコメント長（16 bit LE）の位置。
const CENTRAL_COMMENT_LENGTH_OFFSET: usize = 32;

/// ZIP の読み込みと不正コンテナの拒否（design「ContainerCodec」の `decode`）。
///
/// 判定順・宣言サイズの出所・絶対上限を設けない理由はモジュール docs にある。
impl ContainerCodec {
    /// コンテナのバイト列を検証しながら読み、論理エントリ集合へ戻す。確定形:
    /// `ContainerCodec::decode(bytes: &[u8]) -> Result<DocumentParts, DocumentError>`。
    ///
    /// 許可リスト照合と重複検出（名前の列だけで決まる判定）を済ませてから展開する。
    /// 型マーカー `jxcel` は集合から外し、展開した全パートと索引の形式バージョンを
    /// [`DocumentParts::from_entries`] へ渡す。失敗はすべて読み込み全体の中止であり、
    /// 部分的な集合を返さない（要件 5.4 系）。
    pub fn decode(bytes: &[u8]) -> Result<DocumentParts, DocumentError> {
        // (1) ZIP として開く。開けなければ実エントリ名が無いので擬似名で報告する。
        let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(archive_error)?;

        // (2) 中央ディレクトリを生バイトで走査し、重複を含む名前列を作る。
        let raw_names = raw_entry_names(bytes, archive.central_directory_start())?;

        // (3) 許可リスト照合と重複検出（展開より先）。
        let names = layout::admit(&raw_names)?;

        // (4) 型マーカー: 実在・固定形・索引の記録値との一致。
        let marker_version = read_marker(&mut archive, &raw_names, &names)?;

        // (5) 展開とサイズ照合。マーカーは論理エントリ集合に含めない（タスク 4.8 の裁定）。
        let mut entries = Vec::with_capacity(names.len().saturating_sub(1));
        for (raw, name) in raw_names.iter().zip(&names) {
            if *name == EntryName::Marker {
                continue;
            }
            entries.push((*name, read_entry(&mut archive, raw)?));
        }

        // (6) 集合の構築と、マーカーの写しと索引の記録値の最終照合。
        let parts = DocumentParts::from_entries(entries)?;
        if parts.format_version() != marker_version {
            return Err(marker_error(format_args!(
                "the type marker carries version {marker_version} but manifest.json records {}",
                parts.format_version()
            )));
        }
        Ok(parts)
    }
}

/// 中央ディレクトリを走査して、生のエントリ名を**重複も含めて**入力順に返す。
///
/// `ZipArchive` は同一名のエントリを `IndexMap` で畳むため公開 API からは重複を観測できない
/// （モジュール docs「重複検出は中央ディレクトリを生バイトで走査する」）。ここでは
/// `directory_start` から固定ヘッダを順に読み、シグネチャが中央ディレクトリのものでなく
/// なった位置（EOCD 等）で止める。名前の長さ・拡張長・コメント長だけを辿り、名前以外の
/// フィールドも内容も解釈しない。
fn raw_entry_names(bytes: &[u8], directory_start: u64) -> Result<Vec<&str>, DocumentError> {
    let mut names = Vec::new();
    let mut offset = usize::try_from(directory_start)
        .map_err(|_| archive_message("the central directory offset is out of range"))?;
    loop {
        let end = offset
            .checked_add(CENTRAL_HEADER_LEN)
            .ok_or_else(|| archive_message("the central directory offset overflowed"))?;
        let Some(header) = bytes.get(offset..end) else {
            // 中央ディレクトリの終端に到達した（EOCD 等）。走査の正常な終了である。
            break;
        };
        if header[..CENTRAL_SIGNATURE.len()] != *CENTRAL_SIGNATURE {
            break;
        }
        let name_length = le_u16(header, CENTRAL_NAME_LENGTH_OFFSET);
        let extra_length = le_u16(header, CENTRAL_EXTRA_LENGTH_OFFSET);
        let comment_length = le_u16(header, CENTRAL_COMMENT_LENGTH_OFFSET);

        let name_end = end
            .checked_add(name_length)
            .ok_or_else(|| archive_message("the central directory name length overflowed"))?;
        let raw = bytes
            .get(end..name_end)
            .ok_or_else(|| archive_message("the central directory is truncated"))?;
        let name = core::str::from_utf8(raw).map_err(|_| {
            archive_message(format_args!(
                "the entry name {:?} is not valid UTF-8",
                String::from_utf8_lossy(raw)
            ))
        })?;
        names.push(name);

        offset = name_end
            .checked_add(extra_length)
            .and_then(|position| position.checked_add(comment_length))
            .ok_or_else(|| archive_message("the central directory length overflowed"))?;
    }
    Ok(names)
}

/// 型マーカー `jxcel` を展開して形式バージョンを読む（実在しなければ中止）。
///
/// 生の名前と解析済みの名前は [`layout::admit`] が 1 対 1 で返すため、同じ位置の組を
/// 走査すればよい（添字を使わず `zip` で組にする）。
fn read_marker(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    raw_names: &[&str],
    names: &[EntryName],
) -> Result<FormatVersion, DocumentError> {
    for (raw, name) in raw_names.iter().zip(names) {
        if *name == EntryName::Marker {
            return parse_marker(&read_entry(archive, raw)?);
        }
    }
    Err(marker_error("the type marker entry is missing"))
}

/// 1 エントリを展開する。宣言サイズ（中央ディレクトリの非圧縮サイズ）を上限に**有界に**読み、
/// 実際の展開長が宣言と一致しなければ中止する。
///
/// 上限は「宣言 + 1」バイトである。超過を検出できる最小の読みであり、これにより壊れた
/// アーカイブが宣言より多くのバイトを要求しても、読む量は宣言された値で頭打ちになる
/// （無制限な `read_to_end` をしない）。宣言より長い場合は報告する展開長が上限値
/// （宣言 + 1）になる（真の展開長は読まないため不明。検出と報告には十分である）。
fn read_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    raw: &str,
) -> Result<Vec<u8>, DocumentError> {
    let index = archive
        .index_for_name(raw)
        .ok_or_else(|| invalid(raw, "the entry is missing from the archive"))?;
    let mut entry = archive
        .by_index(index)
        .map_err(|error| invalid(raw, error))?;
    let declared = entry.size();
    let mut content = Vec::new();
    // 宣言サイズ + 1 バイトだけ読む（超過を検出できる最小の上限）。
    (&mut entry)
        .take(declared.saturating_add(1))
        .read_to_end(&mut content)
        .map_err(|error| invalid(raw, error))?;
    if content.len() as u64 != declared {
        return Err(invalid(
            raw,
            format_args!(
                "declared size {declared} does not match expanded size {}",
                content.len()
            ),
        ));
    }
    Ok(content)
}

/// 型マーカーの内容を `jxcel\n<major>.<minor>\n` として読み、形式バージョンを返す。
///
/// バージョンの各構成要素は正準 10 進表記（ASCII 数字のみ・先頭ゼロなし）に限る。
/// 書き出し側は [`FormatVersion`] の `Display`（= 正準形）で書くため、ここで綴りの揺れを
/// 許す理由が無い（マーカーは索引の写しであり、写しの綴りが違えば破損として扱う）。
fn parse_marker(bytes: &[u8]) -> Result<FormatVersion, DocumentError> {
    let malformed =
        || marker_error("the type marker does not have the form `jxcel\\n<major>.<minor>\\n`");
    let body = bytes
        .strip_prefix(MARKER_PREFIX)
        .and_then(|rest| rest.strip_suffix(MARKER_SUFFIX))
        .and_then(|body| core::str::from_utf8(body).ok())
        .ok_or_else(malformed)?;
    let (major, minor) = body.split_once('.').ok_or_else(malformed)?;
    let major = parse_version_component(major).ok_or_else(malformed)?;
    let minor = parse_version_component(minor).ok_or_else(malformed)?;
    Ok(FormatVersion::new(major, minor))
}

/// 形式バージョンの構成要素を正準 10 進表記として読む（ASCII 数字のみ・先頭ゼロなし）。
fn parse_version_component(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    text.parse().ok()
}

/// 中央ディレクトリの固定ヘッダから 16 bit LE フィールドを `usize` として読む。
///
/// 呼び出し元はヘッダ長を検査済みなので境界外アクセスは起こらない（`usize` への変換は
/// 16 bit の範囲で必ず成功する）。
fn le_u16(header: &[u8], offset: usize) -> usize {
    usize::from(u16::from_le_bytes([header[offset], header[offset + 1]]))
}

/// エントリ名を文脈にした失敗（`<エントリ名>: <理由>`）。
fn invalid(name: &str, reason: impl fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("{name}: {reason}"),
    }
}

/// 型マーカーを文脈にした失敗（`jxcel: <理由>`）。
fn marker_error(reason: impl fmt::Display) -> DocumentError {
    invalid(&EntryName::Marker.to_string(), reason)
}

/// 実在するエントリ名が無い失敗を擬似名 `"<archive>"` で報告する。
fn archive_message(reason: impl fmt::Display) -> DocumentError {
    invalid(ARCHIVE_ENTRY, reason)
}

/// `zip` のエラーを、アーカイブ全体の失敗として報告する。
fn archive_error(error: ZipError) -> DocumentError {
    archive_message(error)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::FileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;
    use crate::container::writer::marker_bytes;
    use crate::migration::FormatVersion;
    use crate::parts::{ManifestEntry, ManifestPart};

    /// 標本のシート識別子（正準 Crockford base32 大文字 26 文字）。
    const SHEET: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    /// 中央ディレクトリ固定ヘッダ内の非圧縮サイズ（32 bit LE）の位置。
    ///
    /// 本実装は宣言サイズを `zip` の `ZipFile::size()`（同じフィールド）から読むため、
    /// 生のオフセットはテストが宣言値を書き換えるためにだけ要る。
    const CENTRAL_DECLARED_SIZE_OFFSET: usize = 24;

    /// 標本のシート識別子。
    fn sheet() -> crate::ids::SheetId {
        SHEET.parse().expect("標本の識別子は正準形")
    }

    /// 本体パートに、そこから算出した索引を足した標本を組み立てる。
    fn parts_with(mut entries: Vec<(EntryName, Vec<u8>)>) -> DocumentParts {
        let index: Vec<ManifestEntry> = entries
            .iter()
            .map(|(name, bytes)| ManifestEntry::of_bytes(*name, bytes))
            .collect();
        let manifest = ManifestPart::new(FormatVersion::new(1, 0), index).expect("標本の索引");
        entries.push((
            EntryName::Manifest,
            manifest.to_json_bytes().expect("索引の符号化"),
        ));
        DocumentParts::from_entries(entries).expect("標本は妥当")
    }

    /// 標本（複数の本体パート）。
    fn sample_parts() -> DocumentParts {
        parts_with(vec![
            (
                EntryName::Document,
                br#"{"document_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","sheets":[]}"#.to_vec(),
            ),
            (
                EntryName::Schema { sheet: sheet() },
                br#"{"root":null,"types":[]}"#.to_vec(),
            ),
            (EntryName::Rows { sheet: sheet() }, b"".to_vec()),
        ])
    }

    /// パート集合を (名前, バイト列) の列にする（`DocumentParts` に `PartialEq` は無い）。
    fn named(parts: &DocumentParts) -> Vec<(String, Vec<u8>)> {
        parts
            .iter()
            .map(|part| (part.name.to_string(), part.bytes.clone()))
            .collect()
    }

    /// 与えられた (名前, 内容) を順に持つ ZIP を組み立てる（名前は許可リストを経由しない）。
    fn archive_bytes(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
        let options = FileOptions::DEFAULT.compression_method(method);
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            writer.start_file(*name, options).expect("エントリの開始");
            writer.write_all(content).expect("書き込み");
        }
        writer.finish().expect("ZIP の完成").into_inner()
    }

    /// 標本のパート集合を、与えられた型マーカーの内容・順序・圧縮方式で ZIP へ書き直す。
    ///
    /// パートの順序は呼び出し元が決める（`marker_last` が真なら型マーカーを最後に置く）。
    fn rebuild(
        parts: &DocumentParts,
        marker: &[u8],
        method: CompressionMethod,
        marker_last: bool,
    ) -> Vec<u8> {
        let options = FileOptions::DEFAULT.compression_method(method);
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let write_marker = |writer: &mut ZipWriter<Cursor<Vec<u8>>>| {
            writer.start_file("jxcel", options).expect("マーカーの開始");
            writer.write_all(marker).expect("マーカーの書き込み");
        };
        if !marker_last {
            write_marker(&mut writer);
        }
        for part in parts.iter() {
            writer
                .start_file(part.name.to_string(), options)
                .expect("エントリの開始");
            writer.write_all(&part.bytes).expect("書き込み");
        }
        if marker_last {
            write_marker(&mut writer);
        }
        writer.finish().expect("ZIP の完成").into_inner()
    }

    /// 与えられたバイト列を `decode` に通し、中止したエラーが `InvalidContainer` であること
    /// を確かめて `entry` を返す。
    fn rejection(bytes: &[u8]) -> String {
        match ContainerCodec::decode(bytes) {
            Err(DocumentError::InvalidContainer { entry }) => entry,
            other => panic!("不正なコンテナが拒否されない: {other:?}"),
        }
    }

    /// ZIP として開けないバイト列を拒否し、理由を添えた擬似名で報告する。
    #[test]
    fn decode_rejects_bytes_that_are_not_an_archive() {
        let entry = rejection(b"this is not a zip archive");
        assert!(
            entry.starts_with(ARCHIVE_ENTRY),
            "アーカイブ全体の失敗が擬似名 `{ARCHIVE_ENTRY}` で報告されない: {entry:?}"
        );
        assert!(
            entry.len() > ARCHIVE_ENTRY.len(),
            "理由が添えられていない: {entry:?}"
        );
    }

    /// 許可リスト外・ルート外を指す名前を持つアーカイブを拒否し、該当エントリ名をそのまま
    /// エラーに含める（要件 2.5）。
    #[test]
    fn decode_rejects_names_outside_the_allow_list() {
        let marker = marker_bytes(FormatVersion::new(1, 0));
        let cases = [
            "../evil.json",           // 親ディレクトリへ出る相対パス
            "sheets/../../evil.json", // 許可プレフィックスの後からの脱出
            "/abs.json",              // 絶対パス
            "C:/x.json",              // ドライブレター
            "a\\b.json",              // バックスラッシュ区切り
            "a\0b.json",              // NUL
            "sheets/",                // ディレクトリエントリ（末尾 `/`）
            "Manifest.json",          // 大文字小文字の変種
            "README",                 // 未知の名前
        ];
        for name in cases {
            let bytes = archive_bytes(
                &[("jxcel", &marker), (name, b"{}")],
                CompressionMethod::Deflated,
            );
            assert_eq!(
                name,
                rejection(&bytes),
                "該当エントリ名が原文のままエラーに含まれていない"
            );
        }
    }

    /// 同一パスのエントリを複数持つアーカイブを拒否し、該当パスをエラーに含める（要件 2.6）。
    ///
    /// `ZipWriter` は同名のエントリを拒否するため、異なる同名長のエントリを書いてから
    /// 中央ディレクトリ（とローカルヘッダ）の名前を書き換えて重複を作る。
    #[test]
    fn decode_rejects_a_duplicate_entry_path() {
        let mut bytes = archive_bytes(
            &[("manifest.json", b"{}"), ("document.json", b"{}")],
            CompressionMethod::Deflated,
        );
        replace_all(&mut bytes, b"document.json", b"manifest.json");

        let entry = rejection(&bytes);
        assert!(
            entry.contains("manifest.json") && entry.contains("duplicate"),
            "重複のエラーが該当パスと理由を含まない: {entry:?}"
        );
    }

    /// 許可リストの判定が展開に先行する（Contract「判定の先行」）。
    ///
    /// 許可リスト外の名前と、宣言サイズを実展開量と食い違わせたエントリを同時に持つ
    /// アーカイブを与える。展開が先ならサイズ不一致（`document.json`）が報告されるはずで
    /// あり、許可リスト違反（`../evil.json`）が報告されることが先行の実測である。
    /// サイズ不一致が実際に検出されることは `decode_rejects_a_size_that_exceeds_the_declaration`
    /// が同じ作り方で示す。
    #[test]
    fn decode_checks_the_allow_list_before_expanding() {
        let marker = marker_bytes(FormatVersion::new(1, 0));
        let expansive = repetitive(1 << 12);
        let mut bytes = archive_bytes(
            &[
                ("jxcel", &marker),
                ("manifest.json", b"{}"),
                ("../evil.json", b"{}"),
                ("document.json", &expansive),
            ],
            CompressionMethod::Deflated,
        );
        patch_declared_size(&mut bytes, "document.json", 8);

        assert_eq!(
            "../evil.json",
            rejection(&bytes),
            "許可リスト違反より展開（サイズ照合）が先に走っている"
        );
    }

    /// 型マーカーが無いアーカイブを拒否する。
    #[test]
    fn decode_requires_the_type_marker() {
        let bytes = archive_bytes(
            &[("manifest.json", b"{}"), ("document.json", b"{}")],
            CompressionMethod::Deflated,
        );
        let entry = rejection(&bytes);
        assert!(
            entry.starts_with("jxcel") && entry.contains("missing"),
            "型マーカーの不在が `jxcel` を文脈に報告されない: {entry:?}"
        );
    }

    /// 内容が固定形として解釈できない型マーカーを拒否する。
    #[test]
    fn decode_rejects_a_malformed_type_marker() {
        let cases: [&[u8]; 7] = [
            b"",
            b"nope",
            b"jxcel\n",        // バージョンが無い
            b"jxcel\n1.0",     // 末尾の LF が無い
            b"jxcel\n1\n",     // minor が無い
            b"jxcel\n01.0\n",  // 正準形でない（先頭ゼロ）
            b"jxcel\n1.0.2\n", // 構成要素が多い
        ];
        for content in cases {
            let bytes = archive_bytes(
                &[("jxcel", content), ("manifest.json", b"{}")],
                CompressionMethod::Deflated,
            );
            let entry = rejection(&bytes);
            assert!(
                entry.starts_with("jxcel"),
                "壊れた型マーカーが `jxcel` を文脈に拒否されない: {content:?} -> {entry:?}"
            );
        }
    }

    /// 型マーカーが運ぶバージョンと索引の記録値が食い違うアーカイブを拒否する。
    ///
    /// マーカーは索引の写しであり、権威は索引である（タスク 5.2 の裁定）。
    #[test]
    fn decode_rejects_a_marker_version_that_disagrees_with_the_index() {
        let parts = sample_parts();
        let bytes = rebuild(
            &parts,
            &marker_bytes(FormatVersion::new(9, 9)),
            CompressionMethod::Deflated,
            false,
        );
        let entry = rejection(&bytes);
        assert!(
            entry.starts_with("jxcel") && entry.contains("9.9") && entry.contains("1.0"),
            "写しと索引の不一致が両方のバージョンつきで報告されない: {entry:?}"
        );
    }

    /// 型マーカーの位置も圧縮方式も要求しない（要件 2.1 の汎用 ZIP ツール経由の再圧縮）。
    ///
    /// マーカーを最後に置き、`Deflate` で書いたアーカイブを受理する。書き出し側は `Stored`・
    /// 先頭に置くが、復号側はそれに依存しない。
    #[test]
    fn decode_does_not_require_the_marker_to_be_stored_or_first() {
        let parts = sample_parts();
        let bytes = rebuild(
            &parts,
            &marker_bytes(parts.format_version()),
            CompressionMethod::Deflated,
            true,
        );
        let decoded = ContainerCodec::decode(&bytes).expect("マーカーが先頭でなくても読める");
        assert_eq!(named(&parts), named(&decoded), "再圧縮で集合が変わった");
    }

    /// `decode(encode(p))` が `p` と一致する（design の Invariants）。
    ///
    /// 名前とバイト列で比較する（[`DocumentParts`] に `PartialEq` は無い）。型マーカーは
    /// 集合に含まれない。すべてのエントリが宣言サイズの照合を通るため、正常系で照合が
    /// 誤発火しないことも同時に示す。
    #[test]
    fn decode_inverts_encode() {
        let attachment = repetitive(1 << 16);
        let samples: [(&str, DocumentParts); 3] = [
            ("索引のみ", parts_with(Vec::new())),
            ("複数パート", sample_parts()),
            (
                "大きい添付",
                parts_with(vec![(
                    EntryName::Attachment {
                        attachment: crate::ids::AttachmentId::from_bytes(&attachment),
                    },
                    attachment,
                )]),
            ),
        ];
        for (label, parts) in samples {
            let bytes = ContainerCodec::encode(&parts).expect("符号化");
            let decoded = ContainerCodec::decode(&bytes).expect("復号");
            assert_eq!(
                named(&parts),
                named(&decoded),
                "{label}: 往復で集合が変わった"
            );
            assert_eq!(
                parts.format_version(),
                decoded.format_version(),
                "{label}: 往復で形式バージョンが変わった"
            );
        }
    }

    /// 実際の展開長が宣言サイズを超えるエントリを拒否する（有界読みの超過検出）。
    ///
    /// 宣言を 100 に書き換えたエントリは 4096 バイトへ展開する。有界読み（宣言 + 1）は
    /// **101 バイト**で止まるため、報告される展開長は 101 である。上限を外して
    /// `read_to_end` すると報告が 4096 になり、このテストが落ちる（読む量が宣言で
    /// 頭打ちになることの実測）。拒否するのは本層の照合であり、`zip` の CRC 検証ではない:
    /// 有界読みは圧縮ストリームの EOF に達しないため、CRC は照合されない。
    #[test]
    fn decode_rejects_a_size_that_exceeds_the_declaration() {
        let mut bytes = ContainerCodec::encode(&parts_with(vec![(
            EntryName::Document,
            repetitive(1 << 12),
        )]))
        .expect("符号化");
        patch_declared_size(&mut bytes, "document.json", 100);

        assert_eq!(
            "document.json: declared size 100 does not match expanded size 101",
            rejection(&bytes),
            "宣言より長い展開の検出（有界読み）が働いていない"
        );
    }

    /// 実際の展開長が宣言サイズに満たないエントリを拒否する（一致要求の逆方向）。
    ///
    /// 宣言を 100000 に書き換えたエントリは 4096 バイトで終わる。読む量の上限は
    /// 宣言 + 1 なので、報告される展開長は真の値 4096 である。
    #[test]
    fn decode_rejects_a_size_that_is_shorter_than_the_declaration() {
        let mut bytes = ContainerCodec::encode(&parts_with(vec![(
            EntryName::Document,
            repetitive(1 << 12),
        )]))
        .expect("符号化");
        patch_declared_size(&mut bytes, "document.json", 100_000);

        assert_eq!(
            "document.json: declared size 100000 does not match expanded size 4096",
            rejection(&bytes),
            "宣言より短い展開の検出（一致要求）が働いていない"
        );
    }

    /// 破損した内容を、該当エントリ名つきで報告する（中止は読み込み全体）。
    ///
    /// 無圧縮（`Stored`）のエントリを 1 バイト壊すと、ストリームの終端で `zip` の CRC 照合が
    /// 失敗する。その失敗が本層の写像で該当エントリ名つきの
    /// [`DocumentError::InvalidContainer`] になることを示す（サイズ照合の不一致としてでは
    /// ない: 読む量は宣言どおりで、失敗はストリームの終端で起きる）。圧縮エントリの伸長失敗も
    /// 同じ写像を通る。
    #[test]
    fn decode_reports_a_corrupt_entry_with_the_entry_name() {
        let parts = parts_with(vec![(EntryName::Document, repetitive(1 << 12))]);
        let mut bytes = rebuild(
            &parts,
            &marker_bytes(parts.format_version()),
            CompressionMethod::Stored,
            false,
        );
        corrupt_entry_payload(&mut bytes, "document.json");

        let entry = rejection(&bytes);
        assert!(
            entry.starts_with("document.json"),
            "展開の失敗が該当エントリ名つきで報告されない: {entry:?}"
        );
        assert!(
            !entry.contains("does not match expanded size"),
            "サイズ照合の不一致として報告されている（伸長の失敗を写像していない）: {entry:?}"
        );
    }

    /// UTF-8 として読めない生のエントリ名を拒否する（7 形はすべて ASCII）。
    #[test]
    fn decode_rejects_a_name_that_is_not_utf8() {
        let mut bytes = archive_bytes(
            &[("manifest.json", b"{}"), ("document.json", b"{}")],
            CompressionMethod::Deflated,
        );
        replace_all(&mut bytes, b"document.json", b"\xff\xffdocument.js");

        let entry = rejection(&bytes);
        assert!(
            entry.starts_with(ARCHIVE_ENTRY) && entry.contains("document.js"),
            "UTF-8 でない名前の拒否が生の名前つきで報告されない: {entry:?}"
        );
    }

    /// 型マーカーの解析が書き出し側の組み立てと一致する（骨格リテラルの二重管理の固定）。
    #[test]
    fn parse_marker_accepts_every_marker_bytes() {
        for version in [
            FormatVersion::new(0, 0),
            FormatVersion::new(1, 0),
            FormatVersion::new(12, 345),
        ] {
            assert_eq!(
                version,
                parse_marker(&marker_bytes(version)).expect("書き出し側のマーカーが解析できない"),
                "書き出し側のマーカーが別のバージョンとして読まれる"
            );
        }
    }

    /// 決定的な反復バイト列（Deflate が自明にならない程度の分量）。
    fn repetitive(len: usize) -> Vec<u8> {
        const PATTERN: &[u8] = b"jxcel document-format: deterministic deflate. ";
        PATTERN.iter().copied().cycle().take(len).collect()
    }

    /// 中央ディレクトリの指定エントリの非圧縮サイズ（宣言サイズ）を書き換える。
    fn patch_declared_size(bytes: &mut [u8], name: &str, size: u32) {
        let offset =
            central_name_offset(bytes, name) - CENTRAL_HEADER_LEN + CENTRAL_DECLARED_SIZE_OFFSET;
        bytes[offset..offset + 4].copy_from_slice(&size.to_le_bytes());
    }

    /// 中央ディレクトリ内の `name`（名前そのもの）の先頭位置。
    fn central_name_offset(bytes: &[u8], name: &str) -> usize {
        let start = ZipArchive::new(Cursor::new(bytes))
            .expect("標本は ZIP")
            .central_directory_start() as usize;
        let needle = name.as_bytes();
        bytes[start..]
            .windows(needle.len())
            .position(|window| window == needle)
            .map(|position| start + position)
            .expect("中央ディレクトリに名前がある")
    }

    /// `from` を `to` へ置換する（同一長。ローカルヘッダと中央ディレクトリの両方の名前を
    /// 書き換えるため全出現を置換する）。
    fn replace_all(bytes: &mut [u8], from: &[u8], to: &[u8]) {
        assert_eq!(from.len(), to.len(), "名前の長さを変える書き換えはしない");
        let mut replaced = 0;
        for position in 0..=bytes.len() - from.len() {
            if &bytes[position..position + from.len()] == from {
                bytes[position..position + from.len()].copy_from_slice(to);
                replaced += 1;
            }
        }
        assert!(replaced > 0, "置換対象が見つからない");
    }

    /// ローカルヘッダの圧縮データを 1 バイト壊す（展開を失敗させる）。
    fn corrupt_entry_payload(bytes: &mut [u8], name: &str) {
        let mut offset = 0;
        while bytes[offset..offset + 4] == *b"PK\x03\x04" {
            let compressed =
                u32::from_le_bytes(bytes[offset + 18..offset + 22].try_into().expect("固定長"))
                    as usize;
            let name_length =
                u16::from_le_bytes(bytes[offset + 26..offset + 28].try_into().expect("固定長"))
                    as usize;
            let extra_length =
                u16::from_le_bytes(bytes[offset + 28..offset + 30].try_into().expect("固定長"))
                    as usize;
            let data_start = offset + 30 + name_length + extra_length;
            if &bytes[offset + 30..offset + 30 + name_length] == name.as_bytes() {
                let target = data_start + compressed / 2;
                bytes[target] ^= 0x5a;
                return;
            }
            offset = data_start + compressed;
        }
        panic!("ローカルヘッダにエントリ {name} が無い");
    }
}

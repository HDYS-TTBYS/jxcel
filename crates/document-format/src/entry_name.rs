//! エントリ名の文法と許可リスト（タスク 1.6。要件 2.2, 2.3, 2.5）。
//!
//! # 6 形のみを受理する許可リスト
//!
//! コンテナのエントリ名は design「Container Entry Layout」が定める 6 形だけでなければならない:
//!
//! ```text
//! jxcel                          # 形式マーカー（固定オフセット・スニッフ用）
//! manifest.json                  # 権威あるパート索引
//! document.json                  # 安定メタデータ
//! schemas/<sheet-ulid>.json      # シート別スキーマ
//! sheets/<sheet-ulid>.jsonl      # シート別行データ（NDJSON）
//! attachments/<blake3-hex64>.bin # コンテンツアドレス指定添付
//! ```
//!
//! [`EntryName::parse`] は完全一致照合であり、サニタイズでも正規化でもない:
//! 任意の文字列は文法に照らして「そのまま受理」または「拒否」のどちらかになる。
//! 近似的な入力を正準形へ書き換える経路（小文字化・パーセントデコード・
//! `..` の除去・Unicode 正規化など）は存在せず、それらはすべて拒否
//! （[`crate::error::DocumentError::InvalidContainer`]、design エラー表
//! 「許可リスト外のエントリ名」）として扱われる。名前の正規化パイプラインへの
//! 依存はゼロ（tasks「サニタイズではなく拒否とし、名前の正規化に依存しない」）。
//!
//! # 大文字小文字と ID 構成要素の正準形
//!
//! - キーワード・ディレクトリ名（`jxcel` / `manifest.json` / `document.json` /
//!   `schemas/` / `sheets/` / `attachments/`）はバイト単位完全一致で、大文字小文字の
//!   変種は拒否する。エントリ名は本クレートが正準形で書く唯一の命名であり、
//!   別名を認めると同一のパートに 2 つの名前が生まれて重複検出（要件 2.6）と
//!   ソート決定性が壊れる。
//! - ULID 構成要素（`schemas/` と `sheets/` の ID 部分）: [`crate::ids::SheetId`] の
//!   解析（`ulid` クレートのデコーダは大小文字の別を受けつける）の結果に対し、
//!   正準テキスト形（26 文字 Crockford base32 大文字）との一致検査を行い、
//!   正準形でない入力（小文字など）は拒否する。
//!   これは「エントリ名は本クレートが正準形で書く」という決定に基づくものであり、
//!   ids.rs の `FromStr` が大小文字を問わないこととの意図的な差異である。
//! - 添付 ID 構成要素: [`crate::ids::AttachmentId`] の `Display`（正準形）に合わせ、
//!   64 文字の小文字 hex のみ受理し、大文字 hex は拒否する（同上の理由）。
//!
//! # 形式バージョンと同じ場所で管理する許可リスト
//!
//! 受理する 6 形の集合は形式バージョン [`crate::migration::FormatVersion`] と同じ審査で
//! 管理する（[`crate::migration`] が現行版とゲートを所有する）。[`LAYOUT_FORMS`] が
//! その単一審査可能集合であり、集合の変更は形式バージョンの変更として扱う。
//!
//! # 順序
//!
//! `Ord` は表示テキスト（[`std::fmt::Display`]）の辞書順と一致する。パート反復は
//! この昇順で行う（タスク 4.8 の決定的パート走順の根拠。golden テストが昇順の
//! 実体を固定している）。

use core::cmp::Ordering;
use core::fmt;
use core::str::FromStr;

use crate::error::DocumentError;
use crate::ids::{AttachmentId, SheetId};

/// 受理するエントリ形の集合（単一審査可能セット。モジュール docs
/// 「形式バージョンと同じ場所で管理する」参照）。変更は形式バージョンの変更として
/// 扱うこと。文法の実装は [`EntryName::parse`] であり、本定数との一致は
/// `layout_forms_is_the_reviewable_set` テストが壊れなく担保する。
pub const LAYOUT_FORMS: &[&str] = &[
    "jxcel",
    "manifest.json",
    "document.json",
    "schemas/",
    "sheets/",
    "attachments/",
];

/// キーワード形の正準リテラル（表示と解析は同じ定数を読む。正規化の余地なし）。
const MARKER_TEXT: &str = "jxcel";
const MANIFEST_TEXT: &str = "manifest.json";
const DOCUMENT_TEXT: &str = "document.json";

const SCHEMAS_PREFIX: &str = "schemas/";
const SCHEMAS_SUFFIX: &str = ".json";
const SHEETS_PREFIX: &str = "sheets/";
const SHEETS_SUFFIX: &str = ".jsonl";
const ATTACHMENTS_PREFIX: &str = "attachments/";
const ATTACHMENT_SUFFIX: &str = ".bin";

/// 添付 ID の正準 hex 長（[`crate::ids::Blake3Digest`] のテキスト形と一致）。
const HEX_LEN: usize = crate::ids::Blake3Digest::LEN * 2;

/// 表示テキスト長の上限: 最長の形 `attachments/<hex64>.bin`。
/// [`EntryName`] の表示・比較はヒープ確保なしの固定バッファで行う。
const MAX_DISPLAY_LEN: usize = ATTACHMENTS_PREFIX.len() + HEX_LEN + ".bin".len();

/// コンテナ エントリ名の許可リスト型（design「Container Entry Layout」の 6 形）。
///
/// 値は [`EntryName::parse`] による正準テキストからの解析（または同じ文法を
/// 満たす変種直接構築）でのみ得られる。自由な文字列からのサニタイズや正規化の
/// 経路は存在しない（モジュール docs 参照）。
///
/// `PartialEq` / `Eq` は構造的等値だが、表示テキストは変種ごとに単射で
/// （プレフィックスも互いに素）、`Ord` は表示テキストの辞書順を定義として実装する
/// （モジュール docs「順序」。全ペア一致のテストと golden ソート順テストが
/// 一致を固定する）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryName {
    /// 形式マーカー `jxcel`（Stored・固定オフセット・スニッフ用）。
    Marker,
    /// 権威あるパート索引 `manifest.json`。
    Manifest,
    /// 安定メタデータ `document.json`。
    Document,
    /// シート別スキーマ `schemas/<sheet-ulid>.json`。
    Schema {
        /// 対象シートの識別子（正準 Crockford base32 大文字）。
        sheet: SheetId,
    },
    /// シート別行データ `sheets/<sheet-ulid>.jsonl`。
    Rows {
        /// 行データの所属シート。
        sheet: SheetId,
    },
    /// 添付 `attachments/<blake3-hex64>.bin`（content-addressed）。
    Attachment {
        /// 添付識別子（正準小文字 hex）。
        attachment: AttachmentId,
    },
}

/// 権威あるパート索引 `manifest.json` のエントリ名。
///
/// パート欠落の報告（要件 4.5 の [`crate::error::DocumentError::MissingPart`]）や
/// マニフェストの符号化は、この定数だけを供給元にする（エントリ名の文字列リテラルを
/// 他所へ散在させない）。[`EntryName::parse`] の文法・許可リストは本定数の追加で
/// 一切変わらない（[`LAYOUT_FORMS`] の 1 形に名前を付けただけである）。
pub const MANIFEST_ENTRY: EntryName = EntryName::Manifest;

impl EntryName {
    /// エントリ名を許可リスト文法で解析する。受理は完全一致、それ以外は
    /// [`DocumentError::InvalidContainer`] として**拒否**（サニタイズ不可、要件 2.5）。
    /// エラーは該当エントリ名を原文のまま保持する。
    pub fn parse(text: &str) -> Result<Self, DocumentError> {
        match text {
            MARKER_TEXT => return Ok(EntryName::Marker),
            MANIFEST_TEXT => return Ok(EntryName::Manifest),
            DOCUMENT_TEXT => return Ok(EntryName::Document),
            _ => {}
        }
        if let Some(rest) = text.strip_prefix(SCHEMAS_PREFIX) {
            return match rest
                .strip_suffix(SCHEMAS_SUFFIX)
                .and_then(parse_canonical_ulid)
            {
                Some(sheet) => Ok(EntryName::Schema { sheet }),
                None => Err(invalid_container(text)),
            };
        }
        if let Some(rest) = text.strip_prefix(SHEETS_PREFIX) {
            if let Some(sheet) = rest
                .strip_suffix(SHEETS_SUFFIX)
                .and_then(parse_canonical_ulid)
            {
                return Ok(EntryName::Rows { sheet });
            }
            return Err(invalid_container(text));
        }
        if let Some(rest) = text.strip_prefix(ATTACHMENTS_PREFIX) {
            if let Some(attachment) = rest
                .strip_suffix(ATTACHMENT_SUFFIX)
                .and_then(parse_canonical_hex)
            {
                return Ok(EntryName::Attachment { attachment });
            }
            return Err(invalid_container(text));
        }
        Err(invalid_container(text))
    }

    /// 表示テキスト（コンテナ内の相対パスそのもの）を `buf` に書いて返す。
    /// [`Display`] と `Ord` が同じ 1 経路を共有する。
    ///
    /// [`Display`]: fmt::Display
    fn write_display<'buf>(&self, buf: &'buf mut [u8; MAX_DISPLAY_LEN]) -> &'buf str {
        match self {
            EntryName::Marker => fixed_copy(buf, MARKER_TEXT.as_bytes(), MARKER_TEXT.len()),
            EntryName::Manifest => fixed_copy(buf, MANIFEST_TEXT.as_bytes(), MANIFEST_TEXT.len()),
            EntryName::Document => fixed_copy(buf, DOCUMENT_TEXT.as_bytes(), DOCUMENT_TEXT.len()),
            EntryName::Schema { sheet } => {
                let mut id_buf = [0u8; ulid::ULID_LEN];
                let id = sheet.as_str(&mut id_buf);
                let mid = SCHEMAS_PREFIX.len() + ulid::ULID_LEN;
                buf[..SCHEMAS_PREFIX.len()].copy_from_slice(SCHEMAS_PREFIX.as_bytes());
                buf[SCHEMAS_PREFIX.len()..mid].copy_from_slice(id.as_bytes());
                buf[mid..mid + SCHEMAS_SUFFIX.len()].copy_from_slice(SCHEMAS_SUFFIX.as_bytes());
                ascii(&buf[..mid + SCHEMAS_SUFFIX.len()])
            }
            EntryName::Rows { sheet } => {
                let mut id_buf = [0u8; ulid::ULID_LEN];
                let id = sheet.as_str(&mut id_buf);
                let mid = SHEETS_PREFIX.len() + ulid::ULID_LEN;
                buf[..SHEETS_PREFIX.len()].copy_from_slice(SHEETS_PREFIX.as_bytes());
                buf[SHEETS_PREFIX.len()..mid].copy_from_slice(id.as_bytes());
                buf[mid..mid + SHEETS_SUFFIX.len()].copy_from_slice(SHEETS_SUFFIX.as_bytes());
                ascii(&buf[..mid + SHEETS_SUFFIX.len()])
            }
            EntryName::Attachment { attachment } => {
                let hex = attachment.to_hex();
                let mid = ATTACHMENTS_PREFIX.len() + HEX_LEN;
                buf[..ATTACHMENTS_PREFIX.len()].copy_from_slice(ATTACHMENTS_PREFIX.as_bytes());
                buf[ATTACHMENTS_PREFIX.len()..mid].copy_from_slice(hex.as_bytes());
                buf[mid..mid + ATTACHMENT_SUFFIX.len()]
                    .copy_from_slice(ATTACHMENT_SUFFIX.as_bytes());
                ascii(&buf[..mid + ATTACHMENT_SUFFIX.len()])
            }
        }
    }
}

/// 先頭 `len` バイトを `text` のバイト列で満たす表示ヘルパー（キーワード形用）。
fn fixed_copy<'buf>(buf: &'buf mut [u8; MAX_DISPLAY_LEN], text: &[u8], len: usize) -> &'buf str {
    buf[..len].copy_from_slice(text);
    ascii(&buf[..len])
}

/// 表示経路が書くのは常に ASCII（ASCII-only）なので、変換失敗は起こり得ない。
fn ascii(bytes: &[u8]) -> &str {
    core::str::from_utf8(bytes).expect("表示テキストは常に ASCII")
}

/// 拒否は design エラー表「許可リスト外のエントリ名」= コンテナ層のエラー。
/// 原文をそのまま `entry` に保持する（書き換え・正規化をしないことの担保でもある）。
fn invalid_container(text: &str) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: text.to_owned(),
    }
}

/// ULID 構成要素の解析: 正準テキスト形（26 文字 Crockford base32 大文字）のみ受理。
/// `ulid` クレートのデコーダは大小文字の別を通すため、解析結果の正準レンダリングが
/// 原文と一致する場合のみ受理する（モジュール docs「大文字小文字と ID 構成要素の
/// 正準形」の決定。換字・ハイフンはデコーダ自体が拒否する）。
fn parse_canonical_ulid(text: &str) -> Option<SheetId> {
    let id = SheetId::from_str(text).ok()?;
    let mut buf = [0u8; ulid::ULID_LEN];
    (id.as_str(&mut buf) == text).then_some(id)
}

/// 添付 ID 構成要素の解析: 64 文字の小文字 hex（[`crate::ids::AttachmentId`] の
/// `Display` 正準形）のみ受理。`AttachmentId::from_hex` は大文字も通すが、
/// 上の ULID と同じ決定により大文字 hex は一致検査の前に拒否する。
fn parse_canonical_hex(text: &str) -> Option<AttachmentId> {
    if text.len() != HEX_LEN || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    AttachmentId::from_hex(text).ok()
}

impl fmt::Display for EntryName {
    /// 表示はコンテナ内の相対パスそのもの（`schemas/01J….json` など。
    /// [`EntryName::parse`] の受理形と 1 対 1）。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = [0u8; MAX_DISPLAY_LEN];
        f.write_str(self.write_display(&mut buf))
    }
}

impl Ord for EntryName {
    /// 定義: 表示テキストの辞書順（モジュール docs「順序」。パート反復の
    /// 決定的昇順はこれ。golden テストが実順を固定している）。
    fn cmp(&self, other: &Self) -> Ordering {
        let mut own = [0u8; MAX_DISPLAY_LEN];
        let mut theirs = [0u8; MAX_DISPLAY_LEN];
        self.write_display(&mut own)
            .cmp(other.write_display(&mut theirs))
    }
}

impl PartialOrd for EntryName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DocumentError;
    use crate::ids::{AttachmentId, IdFactory, SheetId};
    use core::str::FromStr;

    const SHEET: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn sheet() -> SheetId {
        SheetId::from_str(SHEET).unwrap()
    }

    /// 正準形のサンプル（表示文字列, 期待値）を 1 形 1 個ずつ返す。
    /// 受理・両往復律・ゴールデン・LAYOUT_FORMS 連動の各テストが同じ元を使う。
    fn canonical_forms() -> Vec<(String, EntryName)> {
        vec![
            ("jxcel".to_owned(), EntryName::Marker),
            ("manifest.json".to_owned(), EntryName::Manifest),
            ("document.json".to_owned(), EntryName::Document),
            (
                format!("schemas/{SHEET}.json"),
                EntryName::Schema { sheet: sheet() },
            ),
            (
                format!("sheets/{SHEET}.jsonl"),
                EntryName::Rows { sheet: sheet() },
            ),
            (
                format!("attachments/{HEX}.bin"),
                EntryName::Attachment {
                    attachment: AttachmentId::from_hex(HEX).unwrap(),
                },
            ),
        ]
    }

    /// 拒否は「該当エントリ名を含むエラー」（要件 2.5）でなければならない。
    fn assert_rejected(name: &str) {
        match EntryName::parse(name) {
            Err(DocumentError::InvalidContainer { entry }) => {
                assert_eq!(entry, name, "拒否エラーが該当エントリ名を保持していない");
            }
            other => panic!("拒否されるはずのエントリ名が受理された: {name:?} -> {other:?}"),
        }
    }

    /// 受理: 6 形すべてが正準形で通り、期待する変種（ID 値込み）になる。
    #[test]
    fn accepts_the_six_canonical_forms() {
        for (text, expected) in canonical_forms() {
            assert_eq!(
                EntryName::parse(&text).unwrap(),
                expected,
                "正準形が受理されないか別変種になった: {text}"
            );
        }
        // 同一 ID でもディレクトリ形が違えば別変種（パート分離、要件 2.2, 2.3）
        assert_ne!(
            EntryName::Schema { sheet: sheet() },
            EntryName::Rows { sheet: sheet() },
            "同一 ID でも schemas/ と sheets/ は別変種でなければならない"
        );
        // 別 ID は別の変種値になる
        let other = IdFactory::new().new_sheet_id();
        assert_ne!(
            EntryName::Schema { sheet: sheet() },
            EntryName::Schema { sheet: other }
        );
        let other_attachment = AttachmentId::from_bytes(b"jxcel second attachment");
        assert_ne!(
            EntryName::Attachment {
                attachment: AttachmentId::from_hex(HEX).unwrap()
            },
            EntryName::Attachment {
                attachment: other_attachment
            }
        );
    }

    /// 拒否バッテリー: タスク 1.6 が名指しする全級（絶対パス / `..` 成分 /
    /// ドライブレター / バックスラッシュ / NUL / パーセントエンコード /
    /// ホモグリフ / 大文字小文字変種 / 誤った拡張・ディレクトリ / 余分な経路成分 /
    /// 空 / 空白 / 非正準 ID / 過長名）が、サニタイズではなく拒否されること。
    #[test]
    fn rejects_the_hostile_class_table() {
        let mut rejects: Vec<String> = [
            // 空・絶対パス・ルート相対
            "",
            "/",
            "//",
            "/manifest.json",
            "/jxcel",
            "/etc/passwd",
            "/schemas/01ARZ3NDEKTSV4RRFFQ69G5FAV.json",
            // `..` 成分（位置を問わない）と `.` 相対指定
            "..",
            "../",
            "../../x",
            "../manifest.json",
            "manifest.json/..",
            "manifest.json/.",
            "./manifest.json",
            "./jxcel",
            "schemas/../manifest.json",
            "sheets/..",
            "attachments/../../../../etc/passwd",
            "attachments/../../x.bin",
            "....//manifest.json",
            // ドライブレター（Windows 絶対指定。大文字小文字問わず）
            "C:\\manifest.json",
            "c:/manifest.json",
            "C:\\jxcel",
            "d:/x",
            "Z:\\manifest.json",
            // バックスラッシュ区切り
            "\\manifest.json",
            "manifest.json\\",
            "\\jxcel",
            "jxcel\\",
            "schemas\\01ARZ3NDEKTSV4RRFFQ69G5FAV.json",
            "sheets\\01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonl",
            "\\\\server\\share\\manifest.json",
            // NUL（終端・内側のいずれも）
            "\u{0}",
            "jxcel\u{0}",
            "manifest\u{0}.json",
            "schemas/\u{0}.json",
            "manifest.json\u{0}extra",
            // パーセントエンコードによる回避試行
            "%2e%2e/manifest.json",
            "%2E%2E%2fmanifest.json",
            "%2Fetc%2Fpasswd",
            "schemas/%2e%2e.json",
            "schemas/%301ARZ3NDEKTSV4RRFFQ69G5FAV.json",
            "%6a%78cel",
            // ホモグリフ（全角・キリル混交はバイト一致せず拒否。正規化しない）
            "\u{FF4D}anifest.json",
            "d\u{43E}cument.json",
            "jxc\u{435}l",
            // 大文字小文字変種（キーワード・ディレクトリ側は正準形のみ受理）
            "Manifest.json",
            "MANIFEST.json",
            "MANIFEST.JSON",
            "Document.json",
            "DOCUMENT.JSON",
            "Jxcel",
            "JXCEL",
            "jxCel",
            "Schemas/01ARZ3NDEKTSV4RRFFQ69G5FAV.json",
            "SCHEMAS/01ARZ3NDEKTSV4RRFFQ69G5FAV.json",
            "Sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonl",
            "Attachments/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.bin",
            "schemas/01ARZ3NDEKTSV4RRFFQ69G5FAV.JSON",
            "sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.JSONL",
            // ID 構成要素の非正準形（正準形のみ受理する決定の検証。モジュール docs 参照）
            "schemas/01arz3ndektsv4rrffq69g5fav.json",
            "attachments/0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF.bin",
            "schemas/01ARZ3NDEK-TSV4RRFFQ69G5FAV.json",
            "schemas/01ARZ3NDEK_TSV4RRFFQ69G5FAV.json",
            "schemas/0000000000000000000000000O.json",
            "schemas/01ARZ3NDEKTSV4RRFFQ69G5FAVX.json",
            "schemas/01ARZ3NDEKTSV4RRFFQ69G5FA.json",
            // 誤った拡張子・誤った（単数形など）名前
            "manifest",
            "manifest.jsonl",
            "manifest.json5",
            "document",
            "document.json5",
            "jxcel.json",
            "jxcel.bin",
            "schemas/01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonl",
            "sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.json",
            "attachment/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.bin",
            "schema/01ARZ3NDEKTSV4RRFFQ69G5FAV.json",
            "sheet/01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonl",
            "attachments/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.txt",
            "attachment.bin",
            // 余分な経路成分
            "manifest.json/extra",
            "document.json/extra",
            "jxcel/",
            "jxcel2",
            "jxcels",
            "schemas/01ARZ3NDEKTSV4RRFFQ69G5FAV.json/extra",
            "schemas/01ARZ3NDEKTSV4RRFFQ69G5FAV.json/",
            "schemas/a/b.json",
            "sheets/x/y.jsonl",
            "attachments/a/b.bin",
            "attachments//0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.bin",
            "schemas/.json",
            "sheets/.jsonl",
            "attachments/.bin",
            "schemas/...json",
            // ドットファイル・末尾の点・終端改行
            ".gitignore",
            ".manifest.json",
            "..manifest.json",
            "manifest.json.",
            "jxcel.",
            "manifest.json\n",
            "jxcel\n",
            // 空白（前後・構成要素内）
            " manifest.json",
            "manifest.json ",
            " jxcel",
            "jxcel ",
            "\tmanifest.json",
            "schemas/ 01ARZ3NDEKTSV4RRFFQ69G5FAV.json",
            "schemas/01ARZ3NDEKTSV4RRFFQ69G5FAV .json",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        // 過長名（キーワード連結・ID 部膨張・末尾膨張・前膨張）
        rejects.push(format!("jxcel{}", "x".repeat(4096)));
        rejects.push(format!("schemas/{SHEET}{}.json", "x".repeat(4096)));
        rejects.push(format!("attachments/{HEX}.bin/{}", "y".repeat(4096)));
        rejects.push(format!("{}manifest.json", "p".repeat(4096)));
        rejects.push("x".repeat(65_536));

        assert!(
            rejects.len() >= 115,
            "級を網羅できているか確認できるだけの件数を保つこと"
        );
        for name in &rejects {
            assert_rejected(name);
        }
    }

    /// 往復律 1: すべての形で `parse(display(v)) == v`。
    #[test]
    fn display_output_reparses_to_the_same_value() {
        for (text, expected) in canonical_forms() {
            let display = expected.to_string();
            assert_eq!(display, text, "golden の入力文字列と Display が一致しない");
            assert_eq!(
                EntryName::parse(&display).unwrap(),
                expected,
                "parse(display(v)) が v と一致しない: {display}"
            );
        }
    }

    /// 往復律 2: 受理された正準文字列に対して `display(parse(s)) == s`。
    /// （表示は容れ物の相対パスそのもの。書き出した名前をそのまま読み返せる。）
    #[test]
    fn canonical_text_displays_verbatim() {
        for (text, _) in canonical_forms() {
            let parsed = EntryName::parse(&text).unwrap();
            assert_eq!(
                parsed.to_string(),
                text,
                "display(parse(s)) が s と一致しない"
            );
        }
    }

    /// `Ord` は表示文字列の辞書順と明示的に一致する（全形ペアで検証）。
    /// parts 反復の昇順走査（タスク 4.8）はこの一致に依存する。
    #[test]
    fn ord_is_display_string_ord() {
        let all: Vec<EntryName> = canonical_forms().into_iter().map(|(_, v)| v).collect();
        for a in &all {
            for b in &all {
                assert_eq!(
                    a.cmp(b),
                    a.to_string().cmp(&b.to_string()),
                    "Ord が表示文字列の辞書順と一致しない: {a} / {b}"
                );
            }
        }
    }

    /// golden ソート順: 6 形を構築順（列挙順）からソートした結果の表示列。
    /// parts 反復の昇順（`attachments/` < `document.json` < `jxcel` <
    /// `manifest.json` < `schemas/` < `sheets/`）が仕様上の固定順であることを
    /// 文書化する golden。
    #[test]
    fn golden_sorted_iteration_order() {
        let mut names: Vec<EntryName> = [
            EntryName::Marker,
            EntryName::Manifest,
            EntryName::Document,
            EntryName::Schema { sheet: sheet() },
            EntryName::Rows { sheet: sheet() },
            EntryName::Attachment {
                attachment: AttachmentId::from_hex(HEX).unwrap(),
            },
        ]
        .into_iter()
        .collect();
        names.sort();
        let display: Vec<String> = names.iter().map(ToString::to_string).collect();
        assert_eq!(
            vec![
                format!("attachments/{HEX}.bin"),
                "document.json".to_owned(),
                "jxcel".to_owned(),
                "manifest.json".to_owned(),
                format!("schemas/{SHEET}.json"),
                format!("sheets/{SHEET}.jsonl"),
            ],
            display,
            "parts 反復 golden の昇順が一致しない"
        );
    }

    /// 許可リストとパーサの連動: LAYOUT_FORMS は 6 形そのものであり、
    /// 各形に対しパーサが受理する正準サンプルが 1 個以上存在する
    /// （パーサを足し込んでも定数との乖離はテストを壊す）。
    #[test]
    fn layout_forms_is_the_reviewable_set() {
        const EXPECTED: [&str; 6] = [
            "jxcel",
            "manifest.json",
            "document.json",
            "schemas/",
            "sheets/",
            "attachments/",
        ];
        assert_eq!(
            LAYOUT_FORMS,
            EXPECTED.as_slice(),
            "許可リストは 6 形そのもの"
        );
        for form in LAYOUT_FORMS {
            assert!(
                canonical_forms()
                    .iter()
                    .any(|(text, _)| text.starts_with(form)),
                "LAYOUT_FORMS の形 {form} に対する受理サンプルがない"
            );
        }
        for (text, _) in canonical_forms() {
            assert!(
                LAYOUT_FORMS.iter().any(|f| text.starts_with(f)),
                "受理サンプル {text} が LAYOUT_FORMS のどれにも属さない"
            );
        }
    }
}

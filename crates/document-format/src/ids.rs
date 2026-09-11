//! 識別子の型と発行（タスク 1.3。要件 1.4, 7.2）。
//!
//! # 識別子の体系
//!
//! - [`SheetId`] / [`RowId`] / [`TypeDefId`] / [`DocumentId`]: ULID ベース。26 文字の
//!   Crockford base32 テキスト形は辞書順 = 時系列順でソート可能。4 つは別個の新型
//!   （型エイリアスではない）であり、シート識別子を行識別子が求められる箇所へ渡すことは
//!   型エラーになる（design「各 ID は別個の新型として定義し、取り違えを型で防ぐ」）。
//! - [`DocumentId`]: ドキュメントそのものの識別子（design「Container Entry Layout」の
//!   「ドキュメント ID」であり、タスク 4.3 の `document.json` が永続化する）。
//!   **識別子体系の単一の源は本モジュールである**ため、発行・正準テキスト形・解析を
//!   ここへ集約する。[`SheetId`] とは別個の新型であり、シートの識別子をドキュメントの
//!   識別子が求められる箇所へ渡すことは型エラーになる（取り違えを型で防ぐ）。
//! - [`AttachmentId`]: BLAKE3 ダイジェストによる content-addressed 識別子（要件 7.2）。
//!   識別子は内容を指す: 同一バイト列は常に同一識別子、異なるバイト列は常に異なる識別子。
//!
//! ULID の発行は [`IdFactory`] が担う。[`AttachmentId`] は内容バイト列から決定的に
//! 決まる純粋関数であり、発行者を必要としない。
//!
//! テキストからの解析（`FromStr` / `from_hex`）は、当面ローカルな最小型
//! [`IdParseError`] を返す。クレート共通エラー列挙型はタスク 1.4 で導入され、その際に
//! そちらへ統合される。

use core::fmt;
use std::borrow::Cow;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// テキストからの識別子解析失敗。診断用に不正な入力を保持する。
///
/// 暫定的なローカル型: タスク 1.4 で定義されるクレート共通エラー列挙型の
/// 変種に置き換えられる暫定のものである。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdParseError {
    /// 不正だった入力（診断用の所有コピー）
    pub input: String,
}

impl fmt::Display for IdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid identifier syntax: {}", self.input)
    }
}

impl std::error::Error for IdParseError {}

/// ULID 系識別子の新型を生成する。
///
/// 内部表現は [`ulid::Ulid`]（u128。`Ord` は値の数値順であり、これは正準
/// Crockford base32 テキストの辞書順と一致する）。各呼び出しは別個の構造体を
/// 生成するため、`SheetId` を [`RowId`] が求められる箇所へ渡すことは型エラーになる。
macro_rules! ulid_id_newtype {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(ulid::Ulid);

        impl $name {
            /// 発行済みの ULID をラップする（[`IdFactory`] から呼ばれる）。
            #[inline]
            pub const fn from_ulid(id: ulid::Ulid) -> Self {
                Self(id)
            }

            /// 下位 [`ulid::Ulid`]（上位 48 bit は発行時刻のミリ秒）。
            #[inline]
            pub const fn ulid(&self) -> ulid::Ulid {
                self.0
            }

            /// ヒープ確保なしの借用テキスト形: 正準テキスト形（26 文字、
            /// Crockford base32 大文字）を `buf` に書き込んで返す。
            /// 所有する形でよい場合は `Display`（`to_string()`）をそのまま使うこと。
            #[inline]
            pub fn as_str<'buf>(&self, buf: &'buf mut [u8; ulid::ULID_LEN]) -> &'buf str {
                self.0.array_to_str(buf)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut buf = [0u8; ulid::ULID_LEN];
                f.write_str(self.0.array_to_str(&mut buf))
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                // ulid の正準デコーダ。入力は大小文字を問わない（出力は大文字固定）
                ulid::Ulid::from_string(text)
                    .map(Self)
                    .map_err(|_| IdParseError { input: text.to_owned() })
            }
        }

        // 素の文字列（正準 26 文字テキスト形）として入出力する。
        // `ulid` クレートの `serde` feature は有効化していない（Cargo.toml 固定）ため
        // 文字列形への変換は本モジュールで実装する。
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut buf = [0u8; ulid::ULID_LEN];
                serializer.serialize_str(self.0.array_to_str(&mut buf))
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = Cow::<str>::deserialize(deserializer)?;
                Self::from_str(&text).map_err(serde::de::Error::custom)
            }
        }
    };
}

ulid_id_newtype! {
    /// シート識別子（ULID）。[`RowId`] / [`TypeDefId`] とは別個の新型であり、
    /// 相互に取り違えられない。
    SheetId
}

ulid_id_newtype! {
    /// 行識別子（ULID）。[`SheetId`] / [`TypeDefId`] とは別個の新型であり、
    /// 相互に取り違えられない。
    RowId
}

ulid_id_newtype! {
    /// ネスト型定義識別子（ULID）。[`SheetId`] / [`RowId`] / [`DocumentId`] とは別個の
    /// 新型であり、相互に取り違えられない。
    TypeDefId
}

ulid_id_newtype! {
    /// ドキュメント識別子（ULID）。`document.json` が永続化する（タスク 4.3。design
    /// 「Container Entry Layout」の「ドキュメント ID」）。[`SheetId`] とは別個の新型で
    /// あり、シートの識別子と相互に取り違えられない。
    DocumentId
}

/// 16 進 1 桁の ASCII を数値に戻す。
fn hex_digit_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 32 バイトの BLAKE3 ダイジェスト。パート単位の完全性ダイジェスト（タスク 4.1 の
/// `Part.digest`）と [`AttachmentId`] の土台の双方で使う新型。
/// テキスト形は小文字 hex（64 文字）。serde も hex 文字列として入出力する。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Blake3Digest([u8; 32]);

impl Blake3Digest {
    /// ダイジェストのバイト長
    pub const LEN: usize = 32;

    /// バイト列の BLAKE3 ダイジェストを算出する。
    #[inline]
    pub fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    /// 算出済みの 32 バイト・ダイジェストをラップする。
    #[inline]
    pub const fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    /// 生の 32 バイト。
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; Self::LEN] {
        &self.0
    }

    /// 小文字 hex（64 文字）。
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(Self::LEN * 2);
        for byte in self.0 {
            out.push(HEX_DIGITS[usize::from(byte >> 4)] as char);
            out.push(HEX_DIGITS[usize::from(byte & 0x0f)] as char);
        }
        out
    }

    /// hex テキスト（64 文字）から解析する。大小文字はどちらも受理するが、
    /// 出力形（[`Self::to_hex`]）は常に小文字である。
    pub fn from_hex(text: &str) -> Result<Self, IdParseError> {
        let invalid = || IdParseError {
            input: text.to_owned(),
        };
        let bytes = text.as_bytes();
        if bytes.len() != Self::LEN * 2 {
            return Err(invalid());
        }
        let mut out = [0u8; Self::LEN];
        for (i, pair) in bytes.chunks_exact(2).enumerate() {
            let hi = hex_digit_value(pair[0]).ok_or_else(invalid)?;
            let lo = hex_digit_value(pair[1]).ok_or_else(invalid)?;
            out[i] = hi << 4 | lo;
        }
        Ok(Self(out))
    }
}

impl fmt::Display for Blake3Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Blake3Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Blake3Digest({})", self.to_hex())
    }
}

impl Serialize for Blake3Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Blake3Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = Cow::<str>::deserialize(deserializer)?;
        Self::from_hex(&text).map_err(serde::de::Error::custom)
    }
}

/// 添付の content-addressed 識別子（要件 7.2）: 内容バイト列の BLAKE3 ダイジェスト。
/// テキスト形は [`Blake3Digest`] と同じく小文字 hex（64 文字）であり、
/// `attachments/<hex>.bin` エントリ名はこの形から作られる（タスク 1.6）。
///
/// 識別子は内容を指す: 同一バイト列なら発行時期・プロセスを問わず常に同一識別子、
/// 内容が変われば識別子も変わる。参照が内容を指すことが意図された意味論である
/// （design IdFactory の Risks 参照）。ULID と違い発行主体を必要とせず、
/// 内容バイト列からの純関数として算出する。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttachmentId(Blake3Digest);

impl AttachmentId {
    /// 内容バイト列（BLAKE3）から content-addressed 識別子を算出する。
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(Blake3Digest::of(bytes))
    }

    /// 算出済みダイジェストをそのままラップする。
    #[inline]
    pub const fn from_digest(digest: Blake3Digest) -> Self {
        Self(digest)
    }

    /// 識別子の本体であるダイジェスト。
    #[inline]
    pub const fn digest(&self) -> Blake3Digest {
        self.0
    }

    /// 小文字 hex（64 文字）。
    #[inline]
    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }

    /// hex から解析する（大小文字いずれも受理）。
    pub fn from_hex(text: &str) -> Result<Self, IdParseError> {
        Blake3Digest::from_hex(text).map(Self)
    }
}

impl fmt::Display for AttachmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for AttachmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AttachmentId({})", self.to_hex())
    }
}

impl FromStr for AttachmentId {
    type Err = IdParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::from_hex(text)
    }
}

impl Serialize for AttachmentId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for AttachmentId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = Cow::<str>::deserialize(deserializer)?;
        Self::from_hex(&text).map_err(serde::de::Error::custom)
    }
}

/// シート / 行 / ネスト型定義 / ドキュメントの ULID 識別子を発行する（要件 1.4）。
///
/// * **連続発行は厳密昇順**: 同一ミリ秒内の再発行は下位 80 bit のランダム値を
///   再生成せずインクリメントで進める（`ulid::Generator` の単調増加方針）。
///   仮に同一ミリ秒内でランダムビットが枯渇しても翌ミリ秒へ繰り上げて厳密昇順を
///   保つ（`commit_overflow_increment`）。
/// * **ソート可能性**: ULID の正準テキスト形（26 文字 Crockford base32 大文字）は
///   辞書順 = 時系列順である。
/// * **時刻同期不要**: 単調性はプロセス内状態だけで保証される。プロセス間衝突は
///   ULID 自身の 48 bit ミリ秒時刻 + 80 bit ランダムが担保する
///   （前提: ドキュメントは一度に 1 プロセスが編集する）。
///
/// 4 つの発行口は 1 つの単調カウンタを共有する（実装上の選択）: 種別をまたいで
/// 発行が交互になっても全体で厳密昇順であり、種別内の一意性はそこから従って
/// 保証される。
#[derive(Debug, Clone)]
pub struct IdFactory {
    generator: ulid::Generator,
}

impl IdFactory {
    /// 新しいファクトリを作る。インスタンス間で状態は共有しない。
    #[inline]
    pub const fn new() -> Self {
        Self {
            generator: ulid::Generator::new(),
        }
    }

    #[inline]
    fn next_ulid(&mut self) -> ulid::Ulid {
        match self.generator.generate() {
            Ok(id) => id,
            // 同一ミリ秒内でのランダムビット枯渇（2^80 分の 1 の理論限界）は
            // 翌ミリ秒へ繰り上げて厳密昇順を保つ。
            Err(overflow) => overflow.commit_overflow_increment(),
        }
    }

    /// 新しいシート識別子を発行する。
    #[inline]
    pub fn new_sheet_id(&mut self) -> SheetId {
        SheetId::from_ulid(self.next_ulid())
    }

    /// 新しいドキュメント識別子を発行する。
    #[inline]
    pub fn new_document_id(&mut self) -> DocumentId {
        DocumentId::from_ulid(self.next_ulid())
    }

    /// 行識別子を発行する。
    #[inline]
    pub fn new_row_id(&mut self) -> RowId {
        RowId::from_ulid(self.next_ulid())
    }

    /// 新しいネスト型定義識別子を発行する。
    #[inline]
    pub fn new_type_def_id(&mut self) -> TypeDefId {
        TypeDefId::from_ulid(self.next_ulid())
    }
}

impl Default for IdFactory {
    fn default() -> Self {
        Self::new()
    }
}

// 型による取り違え防止（受け入れ基準 d）は構造的な保証である: `SheetId` / `RowId` /
// `TypeDefId` / `DocumentId` はマクロが生成する別個のユニット構造体であり、型别名ではない。
// `SheetId` を `RowId` が要求される箇所へ渡すとコンパイルエラーになるため、
// 実行時テストではなく型体系そのものが保証する（compile-fail テストは意図的に置かない）。

#[cfg(test)]
mod tests {
    use super::*;

    /// Crockford base32 の英字アルファベット（I・L・O・U を除外）。
    const CROCKFORD: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    /// 要件 7.2: 同一内容の添付は同一の識別子を得る。識別子は内容の BLAKE3 に一致し、
    /// 異なるバイト列は異なる識別子になる。
    #[test]
    fn attachment_id_is_content_addressed() {
        let bytes: &[u8] = b"jxcel attachment payload";
        let a = AttachmentId::from_bytes(bytes);
        let again = AttachmentId::from_bytes(bytes);
        assert_eq!(a, again, "同一内容の 2 回の発行で識別子が一致しない");

        // 識別子は内容の BLAKE3 ダイジェストそのもの（design Testing Strategy）
        assert_eq!(a.to_hex(), blake3::hash(bytes).to_hex().as_str());

        // 異なるバイト列 → 異なる識別子
        assert_ne!(a, AttachmentId::from_bytes(b"jxcel attachment payload!"));
        assert_ne!(a, AttachmentId::from_bytes(b""));

        // ダイジェストからの直接構築も同一の識別子になる
        assert_eq!(AttachmentId::from_digest(Blake3Digest::of(bytes)), a);
    }

    /// 要件 1.4: 連続発行した 1000 個の ULID が時系列に昇順であること。
    /// 短時間で 1000 個を発行するため同一ミリ秒内の単調増加経路が必ず踏まれる。
    #[test]
    fn consecutive_ulids_sort_ascending() {
        let mut factory = IdFactory::new();
        let ids: Vec<SheetId> = (0..1000).map(|_| factory.new_sheet_id()).collect();

        for pair in ids.windows(2) {
            assert!(pair[0] < pair[1], "連続発行が厳密昇順でない: {pair:?}");
        }
        // 同一ミリ秒内の発行（単調増加インクリメント）が実際に発生していること
        assert!(
            ids.windows(2)
                .any(|pair| pair[0].ulid().timestamp_ms() == pair[1].ulid().timestamp_ms()),
            "1000 個の連続発行で同一ミリ秒内の発行が発生せず、単調増加経路を検証できていない"
        );

        // text 形（Crockford base32）の辞書順でも時系列順が保たれること
        let text: Vec<String> = ids.iter().map(ToString::to_string).collect();
        let mut sorted = text.clone();
        sorted.sort_unstable();
        assert_eq!(text, sorted, "text 形のソートが発行順と一致しない");
    }

    /// 3 種の発行口が同一の単調シーケンスを共有すること（交差発行でも厳密昇順）。
    #[test]
    fn interleaved_kinds_share_the_monotonic_sequence() {
        let mut factory = IdFactory::new();
        let mut issued: Vec<ulid::Ulid> = Vec::new();
        for _ in 0..300 {
            issued.push(factory.new_sheet_id().ulid());
            issued.push(factory.new_row_id().ulid());
            issued.push(factory.new_type_def_id().ulid());
        }
        for pair in issued.windows(2) {
            assert!(pair[0] < pair[1], "種別交差の連続発行が厳密昇順でない");
        }
    }

    /// ドキュメント識別子（タスク 4.3。design「Container Entry Layout」の
    /// 「ドキュメント ID」）も、他の ULID 系識別子と同じ 1 つの単調列から発行される
    /// （要件 1.4）。連続発行の昇順性・種別交差での昇順性・正準テキスト形の往復を、
    /// 既存 3 種と同じ水準で確かめる。
    #[test]
    fn document_ids_are_issued_from_the_shared_monotonic_sequence() {
        let mut factory = IdFactory::new();
        let ids: Vec<DocumentId> = (0..1000).map(|_| factory.new_document_id()).collect();

        for pair in ids.windows(2) {
            assert!(pair[0] < pair[1], "連続発行が厳密昇順でない: {pair:?}");
        }
        // 同一ミリ秒内の発行（単調増加インクリメント）が実際に発生していること
        assert!(
            ids.windows(2)
                .any(|pair| pair[0].ulid().timestamp_ms() == pair[1].ulid().timestamp_ms()),
            "1000 個の連続発行で同一ミリ秒内の発行が発生せず、単調増加経路を検証できていない"
        );

        // シート識別子と交差して発行しても全体が厳密昇順（1 つのカウンタの共有）
        let mut factory = IdFactory::new();
        let mut issued: Vec<ulid::Ulid> = Vec::new();
        for _ in 0..300 {
            issued.push(factory.new_document_id().ulid());
            issued.push(factory.new_sheet_id().ulid());
        }
        for pair in issued.windows(2) {
            assert!(pair[0] < pair[1], "種別交差の連続発行が厳密昇順でない");
        }

        // 正準テキスト形（26 文字 Crockford base32 大文字）の往復
        let id = IdFactory::new().new_document_id();
        let text = id.to_string();
        assert_eq!(ulid::ULID_LEN, text.len());
        assert!(
            text.chars().all(|c| CROCKFORD.contains(c)),
            "正準 text 形が Crockford base32 大文字でない: {text}"
        );
        assert_eq!(
            id,
            text.parse::<DocumentId>().unwrap(),
            "text 形往復が一致しない"
        );
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(format!("\"{text}\""), json);
        assert_eq!(id, serde_json::from_str::<DocumentId>(&json).unwrap());
    }

    /// ULID 26 文字 Crockford base32 大テキスト形の往復と、serde の素の文字列表現。
    #[test]
    fn ulid_text_roundtrip() {
        let mut factory = IdFactory::new();
        check_roundtrip(factory.new_sheet_id());
        check_roundtrip(factory.new_row_id());
        check_roundtrip(factory.new_type_def_id());

        fn check_roundtrip<T>(id: T)
        where
            T: core::fmt::Display
                + core::str::FromStr<Err = IdParseError>
                + PartialEq
                + core::fmt::Debug
                + serde::Serialize
                + serde::de::DeserializeOwned,
        {
            let text = id.to_string();
            assert_eq!(text.len(), ulid::ULID_LEN);
            assert!(
                text.chars().all(|c| CROCKFORD.contains(c)),
                "正準 text 形が Crockford base32 大文字でない: {text}"
            );
            assert_eq!(text.parse::<T>().unwrap(), id, "text 形往復が一致しない");
            // 小文字入力は受理される（正準出力は大文字のまま）
            assert_eq!(
                text.to_lowercase().parse::<T>().unwrap(),
                id,
                "小文字入力での parse が正準値と一致しない"
            );
            // serde は素の文字列としてシリアライズされる
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<T>(&json).unwrap(), id);
        }

        // 不正な text は拒否される
        assert!("nope".parse::<RowId>().is_err());
        assert!("O".repeat(26).parse::<RowId>().is_err()); // 'O' は Crockford で無効
    }

    /// [`Blake3Digest`] の小文字 hex 往復。不正入力の拒否。
    #[test]
    fn blake3_digest_hex_roundtrip() {
        let digest = Blake3Digest::of(b"jxcel part bytes");
        let hex = digest.to_hex();
        assert_eq!(hex.len(), 64);
        assert!(
            hex.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "hex 表記が小文字 hex でない: {hex}"
        );
        assert_eq!(Blake3Digest::from_hex(&hex).unwrap(), digest);
        // 大文字入力は受理され、同じダイジェストに戻る
        assert_eq!(Blake3Digest::from_hex(&hex.to_uppercase()).unwrap(), digest);

        // 不正入力の拒否
        assert!(Blake3Digest::from_hex("deadbeef").is_err());
        assert!(Blake3Digest::from_hex(&"g".repeat(64)).is_err());
        let err = Blake3Digest::from_hex("zz").unwrap_err();
        assert_eq!(err.input, "zz");

        // serde は hex 文字列として往復する
        let json = serde_json::to_string(&digest).unwrap();
        assert_eq!(json, format!("\"{hex}\""));
        assert_eq!(serde_json::from_str::<Blake3Digest>(&json).unwrap(), digest);

        // AttachmentId の text 形はダイジェストの hex と同一
        assert_eq!(AttachmentId::from_digest(digest).to_hex(), hex);
    }

    /// ヒープ確保なしの借用 text 形 `as_str` が `Display` と一致する。
    #[test]
    fn ulid_borrowed_text_matches_display() {
        let id = IdFactory::new().new_type_def_id();
        let mut buf = [0u8; ulid::ULID_LEN];
        assert_eq!(id.as_str(&mut buf), id.to_string());
    }
}

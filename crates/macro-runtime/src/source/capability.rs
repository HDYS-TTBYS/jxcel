//! ソース先頭の**能力宣言**の解析（tasks.md 1.6。要件 8.1–8.5。design.md 決定 6）。
//!
//! マクロがファイルとネットワークのどこに触れるかは、**ソースの先頭に書いた宣言**で決まる
//! （実行のたびに確認の面を出さない。宣言はソースと一緒にドキュメントへ保存されるので、
//! 渡した相手にも同じ宣言が付いてくる）。宣言が無ければ、**その能力を使うホスト API は
//! 呼べない**（門は `surface/gate.rs` が担う。本モジュールは集合を作るところまでである）。
//!
//! # 綴り（決定 6 の Follow-up をここで固定する）
//!
//! ```text
//! // @grant file.read, file.write
//! // @grant net
//! const rows = await host.readRange(...)
//! ```
//!
//! - 宣言行は**ソースの先頭**（空行とコメント行だけを挟んでよい）に置く。**最初の
//!   コメントでない行**が現れた時点で、そこから後ろは見ない（本文のコメントを宣言と
//!   取り違えないためである）
//! - 行の形は `//`（または `#!` の後の `//`）＋ `@grant` ＋ 名前の並び。名前の区切りは
//!   **カンマと空白のどちらでもよい**（`file.read,file.write` も `file.read file.write` も
//!   同じ）
//! - **大文字小文字は問わない**（`FILE.READ` は `file.read`）。名前の前後の空白も無視する
//! - 同じ名前を 2 度書くことは**誤りではない**（集合なので 1 つに畳まれる）
//! - **知らない名前は誤りである**（黙って捨てない。名前と位置を返す — 要件 8.3）
//!
//! # 何をしないか
//!
//! ソースの**構文**も**型**も見ない（要件 3.2。それは変換の段とエディタの仕事である）。
//! ファイルの読み込みのような*実行時の*拒否（宣言に無い能力を使ったとき）は門の仕事である。

use core::fmt;

/// マクロが宣言できる能力（要件 8.4。**閉じた集合である**）。
///
/// ファイルの読み込み・書き込み・ネットワークは**それぞれ別の能力**として扱う
/// （まとめて 1 つにしない — 読むだけのマクロに書く権利を与えない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    /// ファイルの読み込み。
    FileRead,
    /// ファイルの書き込み。
    FileWrite,
    /// ネットワークの利用。
    Net,
}

impl Capability {
    /// 宣言に書く綴り（**正準の綴り**。表示と提示はこれをそのまま使う）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileRead => "file.read",
            Self::FileWrite => "file.write",
            Self::Net => "net",
        }
    }

    /// 綴りから能力を引く（**大文字小文字は問わない**。前後の空白は呼び出し元が落とす）。
    fn from_spelling(spelling: &str) -> Option<Self> {
        let folded = spelling.to_ascii_lowercase();
        match folded.as_str() {
            "file.read" => Some(Self::FileRead),
            "file.write" => Some(Self::FileWrite),
            "net" => Some(Self::Net),
            _ => None,
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 宣言された能力の集合。
///
/// **順序は綴りの辞書順で固定する**（提示と記録の行が、ソースの書き方に依らず決まる）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    capabilities: Vec<Capability>,
}

impl CapabilitySet {
    /// 何も宣言していない集合。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 能力を加える（重複は畳む。順序は辞書順のまま）。
    pub fn insert(&mut self, capability: Capability) {
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
            self.capabilities.sort_unstable();
        }
    }

    /// その能力が宣言されているか（**門がこれを見る**）。
    pub fn contains(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// 宣言されている能力（辞書順）。
    pub fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        self.capabilities.iter().copied()
    }

    /// 宣言の数。
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    /// 空であるか。
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    /// 提示用の 1 行（`file.read, net`。空なら空文字）。
    pub fn described(&self) -> String {
        self.capabilities
            .iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// 宣言の解析の誤り（**表示の文言は持たない**。組み立てるのは利用者へ見せる層である）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationError {
    /// 知らない能力の名前（要件 8.3）。**名前と、ソースの中の位置**を運ぶ。
    UnknownCapability {
        /// 書かれていた名前（書かれたままの綴り）。
        name: String,
        /// 1 起点の行。
        line: u32,
        /// 1 起点の列。
        column: u32,
    },
}

impl fmt::Display for DeclarationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCapability { name, line, column } => {
                write!(f, "知らない能力 {name}（{line}:{column}）")
            }
        }
    }
}

impl std::error::Error for DeclarationError {}

/// ソース先頭の宣言を解析する（tasks.md 1.6 の受け入れ）。
///
/// 宣言が 1 つも無ければ**空の集合**を返す（それが「何も宣言していない」であり、門はすべて
/// の能力を拒む）。知らない名前があれば [`DeclarationError::UnknownCapability`] を返す。
pub fn parse(source: &str) -> Result<CapabilitySet, DeclarationError> {
    let mut set = CapabilitySet::empty();
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        // **先頭の空行とコメント行だけを見る。**本文のコメントを宣言と取り違えない
        // （最初のコメントでない行が現れたら、そこで解析を止める）。
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with("//") {
            break;
        }
        let Some(rest) = grant_body(trimmed) else {
            // `@grant` を持たないコメント行は、宣言の前後にあってよい（著作権表示など）。
            continue;
        };
        // 名前の並びを切り分ける（カンマと空白のどちらでも区切れる）。**名前の位置**は
        // 利用者へ示すために要る（要件 8.3）ので、区切りの走査と一緒に数える。
        let base = trimmed.len() - rest.len();
        let mut position = 0;
        for segment in rest.split([',', ' ', '\t']) {
            let start = base + position;
            position += segment.len() + 1;
            let name = segment.trim();
            if name.is_empty() {
                continue;
            }
            let Some(capability) = Capability::from_spelling(name) else {
                return Err(DeclarationError::UnknownCapability {
                    name: name.to_owned(),
                    line: (index + 1) as u32,
                    column: (start + 1) as u32,
                });
            };
            set.insert(capability);
        }
    }
    Ok(set)
}

/// コメント行から `@grant` の後ろを取り出す（`@grant` を持たない行は `None`）。
///
/// **指令の綴りも大文字小文字を問わない**（`// @GRANT net` を認める）。名前の綴りだけを
/// 畳むと、指令を大文字で書いた利用者に「宣言が効いていない」という分かりにくい失敗を
/// 与えるためである。
fn grant_body(trimmed: &str) -> Option<&str> {
    let rest = trimmed.trim_start_matches('/').trim_start();
    let head = rest.get(.."@grant".len())?;
    if !head.eq_ignore_ascii_case("@grant") {
        return None;
    }
    Some(rest["@grant".len()..].trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declaration_is_read_from_the_head_and_folded_into_a_set() {
        let source = "// @grant file.read, net\n// @grant net\nconst x = 1;\n";
        let set = parse(source).expect("宣言が読める");
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            vec![Capability::FileRead, Capability::Net],
            "重複が畳まれ、順序が辞書順に固定されていない"
        );
        assert_eq!(set.described(), "file.read, net");
        assert!(set.contains(Capability::Net));
        assert!(
            !set.contains(Capability::FileWrite),
            "宣言が無い能力を許した"
        );
    }

    #[test]
    fn spelling_is_folded_but_unknown_names_are_rejected_with_their_name() {
        let folded = parse("// @GRANT File.Read\n").expect("大文字小文字は問わない");
        assert!(folded.contains(Capability::FileRead));

        let error = parse("// @grant file.read, teleport\n").expect_err("知らない名前は誤りである");
        assert_eq!(
            error,
            DeclarationError::UnknownCapability {
                name: "teleport".to_owned(),
                line: 1,
                column: 22,
            },
            "名前か位置が違う（要件 8.3 は名前を提示することを求める）"
        );
    }

    #[test]
    fn only_the_head_is_read_and_a_body_comment_is_not_a_declaration() {
        let source = "const x = 1;\n// @grant net\n";
        let set = parse(source).expect("本文のコメントは宣言ではない");
        assert!(set.is_empty(), "本文のコメントを宣言として読んだ");
    }

    #[test]
    fn an_empty_set_is_what_a_macro_without_a_declaration_gets() {
        let set = parse("// 補助の関数\n\nconst x = 1;\n").expect("宣言が無くても誤りではない");
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert_eq!(set.described(), "");
    }
}

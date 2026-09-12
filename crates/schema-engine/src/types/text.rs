//! 文字列の長さと書式の検査（design.md「コンポーネントとファイルの対応」の
//! `TextConstraints`。tasks.md 2.4 が実装する。要件 4.5）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは [`super`]（`TypeCatalog`）の下に置かれ、文字列の
//! 最小長・最大長と書式（パターン）の検査だけを所有する。値の違反
//! （[`Violation`](crate::validate::report::Violation)）は組み立てず、適合か違反かの
//! 2 値（[`Acceptance`]）だけを返す。違反の理由を作るのは `validate` 層（タスク 5.1）である。
//! 宣言の誤り（[`SchemaError`]）は、上限を超えるパターンを**コンパイル時に拒否する**ときだけ
//! 生成する（矛盾する長さの範囲は `Option` で表し、宣言層が位置と理由を付けて拒否する）。
//!
//! # 実行時間をパターンに依存させない（design.md「Compile Layer / SchemaCompiler」）
//!
//! 書式は `regex` 1.13 を使う。**有限オートマトンで線形時間**であり、後方参照と先読みは
//! 非対応で、使うとコンパイル時に落ちる（ReDoS が原理的に起きない）。利用者が書いた
//! パターンには生文字列長・コンパイル後の大きさ・入れ子の深さの上限を課し、超えるものは
//! 宣言の誤り（[`SchemaError::PatternLimitExceeded`]）として拒否する。`regex` は
//! **コンパイル時に一度だけ**構築し、走査中に構築しない（[`TextPattern`] が保持する）。
//!
//! # 長さは文字数で数える（裁定）
//!
//! design は長さの単位を定めていない。本モジュールは**文字数**（`str::chars`）で数える。
//! `str::len()` は UTF-8 のバイト数であり、日本語の列に「最大 32 文字」を宣言すると
//! 10 文字程度で違反になってしまう。利用者が数えるのは文字である。パターンの生文字列長
//! だけは例外で、バイト数で上限を課す（下記）。
//!
//! # パターンは標準の照合意味論に従う（裁定）
//!
//! design はパターンの照合範囲を定めていない。本モジュールは `regex` の標準の意味論を
//! そのまま使う（[`TextPattern::is_match`]）。すなわち**部分一致**であり、値の全体が
//! 書式に従うことを求める宣言は `^…$`（または `\A…\z`）で囲む。囲みを強制しないのは、
//! 利用者のパターンと 3 つの上限を一対一に対応させ、拒否の診断（どの上限をどれだけ超えたか）
//! をパターンそのものについて語れるようにするためである。
//!
//! # 上限と、その超過の報告（tasks.md 2.4）
//!
//! 上限は 3 つあり、それぞれ別の理由で置く（research.md「書式（パターン）制約」）。
//!
//! - [`SOURCE_LENGTH_LIMIT`]: 生のパターンの UTF-8 バイト長。`regex` 自身の推奨に従い、
//!   コンパイルの前に落とす（`regex` の既定の `size_limit` は 10 MiB と緩い）。
//! - [`COMPILED_SIZE_LIMIT`]: コンパイル後のプログラムの大きさ。`regex` の既定は 10 MiB
//!   であり、`\w` だけで実測 50044 バイトになる。64 KiB は `\w` や `\p{Han}` のような
//!   実用的なクラスを通しつつ、`[a-z]{1000}`（実測 72208 バイト）のような展開で膨らむ
//!   パターンを落とす（research.md の裁定は 64〜256 KiB）。
//! - [`NEST_DEPTH_LIMIT`]: 入れ子の深さ。`regex` の既定は 250 であり、`\w{1000}` の
//!   ような展開を許す。research.md の裁定は 32 程度。深さは `regex` の解析器の意味での
//!   入れ子（グループ・文字クラス・連接・選択・繰り返しの段数）である。
//!
//! [`SchemaError::PatternLimitExceeded`] は観測値（`observed`）と上限（`max`）を持つ。
//! **観測値は、分かる範囲で最も強い下限**を入れる。
//!
//! - 生文字列長は自分で数えるため実測値である。
//! - 入れ子の深さは、「コンパイルが通る最小の深さの上限」を二分探索して実測する
//!   （深さの上限だけを動かして、他の上限は固定したまま測る）。
//! - コンパイル後の大きさは `regex` が実測値を返さない（`Error::CompiledTooBig` が運ぶのは
//!   **設定した上限**である）。実測には、拒否したパターンを上限なしでコンパイルすることに
//!   なり、「実行時間が跳ねる余地を残さない」という本モジュールの目的に反する。したがって
//!   超えたことが確実な最小値（上限 + 1）を報告する。
//!
//! # 値なしは本モジュールの関心事ではない
//!
//! [`CellValue::Null`](document_format::CellValue::Null) の扱いは列の必須指定が決める
//! （design.md の型カタログ表）。[`TextConstraints::accepts`] は文字列を受け取るだけであり、
//! 値なしを判定しない。

use super::Acceptance;
use crate::error::{PatternLimit, SchemaError};
use regex::{Regex, RegexBuilder};

/// パターンの生文字列長の上限（UTF-8 バイト。tasks.md 2.4）。
pub const SOURCE_LENGTH_LIMIT: usize = 1024;

/// コンパイル後のプログラムの大きさの上限（バイト。tasks.md 2.4）。
pub const COMPILED_SIZE_LIMIT: usize = 64 * 1024;

/// パターンの入れ子の深さの上限（tasks.md 2.4）。
pub const NEST_DEPTH_LIMIT: u32 = 32;

/// コンパイル済みの書式（design.md「Compile Layer / ColumnValidator」の
/// 「コンパイル済みパターン」。tasks.md 2.4）。
///
/// `regex` は**コンパイル時に一度だけ**構築し、走査中に構築しない。上限の検査も
/// コンパイル時に行うため、照合（[`TextPattern::is_match`]）は上限の内側のパターンに
/// 対してだけ行われる（design.md「Compile Layer / SchemaCompiler」）。
///
/// 生文字列を保持するのは、違反の理由（`Expected::Pattern`。`src/validate/report.rs` の
/// タスク 1.3）が宣言されたパターンそのものを運ぶためである（[`TextPattern::as_str`]）。
#[derive(Debug, Clone)]
pub struct TextPattern(Regex);

impl TextPattern {
    /// パターンを上限付きでコンパイルする（tasks.md 2.4。要件 4.5）。
    ///
    /// `position` はパターンを宣言した位置であり、拒否したときの診断
    /// （[`SchemaError::PatternLimitExceeded`] / [`SchemaError::MalformedDeclaration`]）に
    /// そのまま載せる。本モジュールは位置の中身に関与しない。
    ///
    /// # 拒否するもの
    ///
    /// - 生文字列長が [`SOURCE_LENGTH_LIMIT`] を超える（コンパイルの前に落とす）。
    /// - コンパイル後の大きさが [`COMPILED_SIZE_LIMIT`] を超える。
    /// - 入れ子の深さが [`NEST_DEPTH_LIMIT`] を超える。
    /// - パターンが解釈できない（後方参照・先読み・後読みなど `regex` が対応しない構文を
    ///   含む場合。[`SchemaError::MalformedDeclaration`]）。
    pub fn compile(pattern: &str, position: &str) -> Result<Self, SchemaError> {
        if pattern.len() > SOURCE_LENGTH_LIMIT {
            return Err(SchemaError::PatternLimitExceeded {
                position: position.to_owned(),
                limit: PatternLimit::SourceLength,
                observed: pattern.len(),
                max: SOURCE_LENGTH_LIMIT,
            });
        }
        match build(pattern, NEST_DEPTH_LIMIT) {
            Ok(regex) => Ok(Self(regex)),
            Err(regex::Error::CompiledTooBig(limit)) => {
                Err(compiled_size_exceeded(position, limit))
            }
            Err(regex::Error::Syntax(_)) => Err(classify_syntax_error(pattern, position)),
            Err(other) => Err(SchemaError::MalformedDeclaration {
                position: position.to_owned(),
                reason: other.to_string(),
            }),
        }
    }

    /// 値が書式に一致するか（`regex` の標準の照合意味論。裁定はモジュール docs）。
    pub fn is_match(&self, text: &str) -> bool {
        self.0.is_match(text)
    }

    /// 宣言されたパターンの生文字列。違反の理由（`Expected::Pattern`）が運ぶ。
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// 文字列の型に宣言できる制約（design.md「組込型カタログと `CellValue` への写像」表の
/// `text` のパラメータ。tasks.md 2.4。要件 4.5）。
///
/// 長さは**文字数**で数える（裁定はモジュール docs）。開いている側は `None` である。
/// パターンは [`TextPattern`]（コンパイル済み）として保持し、宣言テキストの解析
/// （`declaration` 層。タスク 3.2）とコンパイル（`compile` 層。タスク 4.4）を経て
/// ここへ渡る。
#[derive(Debug, Clone)]
pub struct TextConstraints {
    min_length: Option<usize>,
    max_length: Option<usize>,
    pattern: Option<TextPattern>,
}

impl TextConstraints {
    /// 制約を組み立てる（要件 4.5）。
    ///
    /// `min_length > max_length` の宣言は**適合しうる値が存在しない**ため受け付けず `None` を
    /// 返す。宣言の誤りとして拒否するのは宣言層であり（`SchemaError` に長さの範囲専用の
    /// 変種は無い。[`SchemaError::MalformedDeclaration`] が位置と理由を運べる唯一の変種）、
    /// 本モジュールは矛盾した制約を作らせない。
    pub fn new(
        min_length: Option<usize>,
        max_length: Option<usize>,
        pattern: Option<TextPattern>,
    ) -> Option<Self> {
        if let (Some(min), Some(max)) = (min_length, max_length) {
            if min > max {
                return None;
            }
        }
        Some(Self {
            min_length,
            max_length,
            pattern,
        })
    }

    /// 宣言された最小長（文字数）。
    pub const fn min_length(&self) -> Option<usize> {
        self.min_length
    }

    /// 宣言された最大長（文字数）。
    pub const fn max_length(&self) -> Option<usize> {
        self.max_length
    }

    /// 宣言された書式（コンパイル済み）。
    pub const fn pattern(&self) -> Option<&TextPattern> {
        self.pattern.as_ref()
    }

    /// 文字列がすべての制約に適合するか（要件 4.5）。
    ///
    /// 公開するのは**適合**と**違反**の 2 値だけである（[`Acceptance`]。要件 2.7）。
    /// どの制約に外れたか（長さか書式か）は違反の理由であり、`validate` 層（タスク 5.1）が
    /// 期待と実際から組み立てる。長さの判定を先に行うのは、書式の照合より安いためである。
    pub fn accepts(&self, text: &str) -> Acceptance {
        if !self.length_fits(text) {
            return Acceptance::Violating;
        }
        match &self.pattern {
            Some(pattern) if !pattern.is_match(text) => Acceptance::Violating,
            _ => Acceptance::Conforming,
        }
    }

    /// 文字数が宣言された範囲に収まるか。
    ///
    /// 最大長が分かっているときは、その上限を超えた時点で数えるのをやめる
    /// （`max + 1` 文字まで数えれば比較には足りる。`min <= max` が不変条件である）。
    /// 長さだけを宣言した列の 10 万行の走査（要件 10.1）で、長い値を最後まで数えない。
    fn length_fits(&self, text: &str) -> bool {
        let cap = self
            .max_length
            .map_or(usize::MAX, |max| max.saturating_add(1));
        let count = text.chars().take(cap).count();
        self.min_length.is_none_or(|min| count >= min)
            && self.max_length.is_none_or(|max| count <= max)
    }
}

/// 上限付きでパターンをコンパイルする（深さの上限だけを呼び出し元が選ぶ）。
fn build(pattern: &str, nest_limit: u32) -> Result<Regex, regex::Error> {
    RegexBuilder::new(pattern)
        .nest_limit(nest_limit)
        .size_limit(COMPILED_SIZE_LIMIT)
        .build()
}

/// 構文の誤りか、入れ子の深さの超過かを切り分ける。
///
/// `regex::Error::Syntax` は両者を文字列でしか運ばない（後方参照・先読みの非対応も、
/// 深さの超過も同じ変種になる）。深さの上限だけを外して再挑戦し、通るなら深さが原因で
/// あり、通らなければ構文そのものの誤りである。大きさの上限で落ちる場合は、解析は通って
/// いる（大きさの検査は解析の後である）ため、これも深さが原因である。
fn classify_syntax_error(pattern: &str, position: &str) -> SchemaError {
    match build(pattern, u32::MAX) {
        Ok(_) | Err(regex::Error::CompiledTooBig(_)) => nesting_depth_exceeded(pattern, position),
        Err(error) => SchemaError::MalformedDeclaration {
            position: position.to_owned(),
            reason: error.to_string(),
        },
    }
}

/// コンパイル後の大きさが上限を超えたことを表す。
fn compiled_size_exceeded(position: &str, limit: usize) -> SchemaError {
    SchemaError::PatternLimitExceeded {
        position: position.to_owned(),
        limit: PatternLimit::CompiledSize,
        // `regex::Error::CompiledTooBig` が運ぶのは**設定した上限**であり、実測値ではない
        // （モジュール docs「上限と、その超過の報告」）。超えたことが確実な最小値を報告する。
        observed: limit.saturating_add(1),
        max: limit,
    }
}

/// 入れ子の深さが上限を超えたことを表す。
fn nesting_depth_exceeded(pattern: &str, position: &str) -> SchemaError {
    let max = usize::try_from(NEST_DEPTH_LIMIT).unwrap_or(usize::MAX);
    let observed = measure_nesting_depth(pattern)
        .and_then(|depth| usize::try_from(depth).ok())
        .map_or(max.saturating_add(1), |depth| {
            depth.max(max.saturating_add(1))
        });
    SchemaError::PatternLimitExceeded {
        position: position.to_owned(),
        limit: PatternLimit::NestDepth,
        observed,
        max,
    }
}

/// 入れ子の深さを実測する（「コンパイルが通る最小の深さの上限」）。
///
/// `regex` は解析器が観測した深さを公開せず、深さの上限は解析の成否を変えるだけである。
/// 深さ `d` のパターンは上限 `d` で初めて通る（上限を上げても解析結果は変わらない）ため、
/// 上限を二分探索すれば深さが実測できる。大きさの上限は固定したまま測るため、探索は有界で
/// ある。大きさの上限も超えているパターンではどの上限でも通らず実測できないため `None` を
/// 返し、呼び出し元が下限を報告する。
fn measure_nesting_depth(pattern: &str) -> Option<u32> {
    let compiles = |limit: u32| build(pattern, limit).is_ok();
    if !compiles(u32::MAX) {
        return None;
    }
    let (mut low, mut high) = (0u32, u32::MAX);
    while low < high {
        let middle = low + (high - low) / 2;
        if compiles(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Some(low)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テストが宣言位置として使う文字列（本モジュールは位置の中身に関与しない）。
    const POSITION: &str = "columns[0].type.pattern";

    /// テスト前提の妥当なパターンをコンパイルする。
    fn compile(pattern: &str) -> TextPattern {
        TextPattern::compile(pattern, POSITION).expect("テスト前提: 妥当なパターン")
    }

    /// テスト前提の矛盾しない制約を組み立てる。
    fn constraints(
        min_length: Option<usize>,
        max_length: Option<usize>,
        pattern: Option<TextPattern>,
    ) -> TextConstraints {
        TextConstraints::new(min_length, max_length, pattern)
            .expect("テスト前提: 矛盾しない長さの範囲")
    }

    /// 長さは文字数で数える（裁定）。`str::len()` のバイト数では日本語が 3 倍に数えられる。
    #[test]
    fn length_is_counted_in_characters() {
        let two = constraints(Some(2), Some(2), None);
        assert_eq!(Acceptance::Conforming, two.accepts("あい"));
        assert_eq!(Acceptance::Violating, two.accepts("a"));
        assert_eq!(Acceptance::Violating, two.accepts("あいう"));
        // バイト数で数えると「あい」は 6 バイトであり、この期待は逆になる。
        assert_eq!(6, "あい".len(), "テスト前提: 日本語は 1 文字 3 バイト");
    }

    /// 最小長・最大長の境界は両端を含む。
    #[test]
    fn length_boundaries_are_inclusive() {
        let ranged = constraints(Some(3), Some(5), None);
        assert_eq!(Acceptance::Violating, ranged.accepts("ab"));
        assert_eq!(Acceptance::Conforming, ranged.accepts("abc"));
        assert_eq!(Acceptance::Conforming, ranged.accepts("abcde"));
        assert_eq!(Acceptance::Violating, ranged.accepts("abcdef"));

        // 上限だけを宣言できる。空文字列も長さ 0 として判定される。
        let upper_only = constraints(None, Some(1), None);
        assert_eq!(Acceptance::Conforming, upper_only.accepts(""));
        assert_eq!(Acceptance::Conforming, upper_only.accepts("a"));
        assert_eq!(Acceptance::Violating, upper_only.accepts("ab"));

        // 下限だけを宣言できる。
        let lower_only = constraints(Some(1), None, None);
        assert_eq!(Acceptance::Conforming, lower_only.accepts("a"));
        assert_eq!(Acceptance::Violating, lower_only.accepts(""));
    }

    /// 最小長が最大長を超える宣言は組み立てられない（宣言層が位置と理由を付けて拒否する）。
    #[test]
    fn contradictory_length_bounds_are_rejected() {
        assert!(TextConstraints::new(Some(5), Some(3), None).is_none());
        assert!(TextConstraints::new(Some(3), Some(3), None).is_some());
        assert!(TextConstraints::new(None, None, None).is_some());
    }

    /// 書式は `regex` の標準の照合意味論に従う（裁定）。
    #[test]
    fn the_pattern_uses_the_standard_matching_semantics() {
        // 全体一致を求める宣言は `^…$` を書く。
        let anchored = constraints(None, None, Some(compile(r"^\d{3}-\d{4}$")));
        assert_eq!(Acceptance::Conforming, anchored.accepts("123-4567"));
        assert_eq!(Acceptance::Violating, anchored.accepts("123-456"));
        assert_eq!(Acceptance::Violating, anchored.accepts("123-45678"));
        assert_eq!(Acceptance::Violating, anchored.accepts("ab1-234567"));

        // 囲まなければ部分一致である（`regex` の標準の意味論をそのまま使う）。
        let unanchored = constraints(None, None, Some(compile(r"\d{3}")));
        assert_eq!(Acceptance::Conforming, unanchored.accepts("123"));
        assert_eq!(Acceptance::Conforming, unanchored.accepts("ab123cd"));
        assert_eq!(Acceptance::Violating, unanchored.accepts("12"));
    }

    /// 複数の制約はすべてを満たすときだけ適合する。
    #[test]
    fn every_constraint_must_hold() {
        let all = constraints(Some(3), Some(5), Some(compile(r"^\d+$")));
        assert_eq!(Acceptance::Conforming, all.accepts("1234"));
        // 書式は適合するが長さが足りない。
        assert_eq!(Acceptance::Violating, all.accepts("12"));
        // 書式は適合するが長すぎる。
        assert_eq!(Acceptance::Violating, all.accepts("123456"));
        // 長さは適合するが書式に合わない。
        assert_eq!(Acceptance::Violating, all.accepts("abcd"));
    }

    /// 制約を 1 つも宣言しなければ、どんな文字列も適合する。
    #[test]
    fn unconstrained_text_accepts_everything() {
        let free = constraints(None, None, None);
        assert_eq!(Acceptance::Conforming, free.accepts(""));
        assert_eq!(Acceptance::Conforming, free.accepts("任意の文字列\n"));
    }

    /// 生文字列長の上限を超えるパターンは、コンパイルの前に宣言の誤りとして拒否する。
    #[test]
    fn an_overlong_source_pattern_is_rejected() {
        let overlong = "a".repeat(SOURCE_LENGTH_LIMIT + 1);
        match TextPattern::compile(&overlong, POSITION) {
            Err(SchemaError::PatternLimitExceeded {
                position,
                limit: PatternLimit::SourceLength,
                observed,
                max,
            }) => {
                assert_eq!(POSITION, position);
                assert_eq!(
                    SOURCE_LENGTH_LIMIT + 1,
                    observed,
                    "生文字列長は実測値である"
                );
                assert_eq!(SOURCE_LENGTH_LIMIT, max);
            }
            other => panic!("生文字列長の上限を超えたパターンが拒否されない: {other:?}"),
        }

        // 長さは UTF-8 バイト数で数える（`regex` の推奨に従い、生のテキスト量を制限する）。
        let multibyte = "あ".repeat(SOURCE_LENGTH_LIMIT / 3 + 2);
        match TextPattern::compile(&multibyte, POSITION) {
            Err(SchemaError::PatternLimitExceeded {
                limit: PatternLimit::SourceLength,
                observed,
                ..
            }) => assert_eq!(multibyte.len(), observed),
            other => panic!("バイト長で上限を超えたパターンが拒否されない: {other:?}"),
        }

        // ちょうど上限はコンパイルできる（境界の内側を拒否しない）。
        let at_limit = "a".repeat(SOURCE_LENGTH_LIMIT);
        assert!(TextPattern::compile(&at_limit, POSITION).is_ok());
    }

    /// コンパイル後の大きさの上限を超えるパターンは宣言の誤りとして拒否する。
    #[test]
    fn an_overlarge_compiled_pattern_is_rejected() {
        // 生文字列は短いが、繰り返しの展開でコンパイル後が膨らむ（実測 72208 バイト）。
        let pattern = "[a-z]{1000}";
        assert!(
            pattern.len() < SOURCE_LENGTH_LIMIT,
            "テスト前提: 生文字列は短い"
        );
        match TextPattern::compile(pattern, POSITION) {
            Err(SchemaError::PatternLimitExceeded {
                position,
                limit: PatternLimit::CompiledSize,
                observed,
                max,
            }) => {
                assert_eq!(POSITION, position);
                assert_eq!(COMPILED_SIZE_LIMIT, max);
                // `regex` は実測値を返さないため、超えたことが確実な最小値を報告する。
                assert_eq!(COMPILED_SIZE_LIMIT + 1, observed);
            }
            other => panic!("コンパイル後の大きさの上限を超えたパターンが拒否されない: {other:?}"),
        }

        // 同じ形でも上限の内側ならコンパイルできる（実測 14608 バイト）。
        assert!(TextPattern::compile("[a-z]{200}", POSITION).is_ok());
    }

    /// 入れ子の深さの上限を超えるパターンは宣言の誤りとして拒否する。
    #[test]
    fn an_overly_nested_pattern_is_rejected() {
        let depth = 40usize;
        let nested = format!("{}a{}", "(".repeat(depth), ")".repeat(depth));
        match TextPattern::compile(&nested, POSITION) {
            Err(SchemaError::PatternLimitExceeded {
                position,
                limit: PatternLimit::NestDepth,
                observed,
                max,
            }) => {
                assert_eq!(POSITION, position);
                assert_eq!(usize::try_from(NEST_DEPTH_LIMIT).unwrap_or(usize::MAX), max);
                // 深さは二分探索で実測する（40 段の入れ子は深さ 40）。
                assert_eq!(depth, observed, "入れ子の深さが実測されていない");
            }
            other => panic!("入れ子の深さの上限を超えたパターンが拒否されない: {other:?}"),
        }

        // ちょうど上限の深さはコンパイルできる（境界の内側を拒否しない）。
        let at_limit = format!(
            "{}a{}",
            "(".repeat(usize::try_from(NEST_DEPTH_LIMIT).unwrap_or(usize::MAX)),
            ")".repeat(usize::try_from(NEST_DEPTH_LIMIT).unwrap_or(usize::MAX))
        );
        assert!(TextPattern::compile(&at_limit, POSITION).is_ok());
    }

    /// 後方参照と先読み・後読みはコンパイル時に拒否される（ReDoS の余地を残さない）。
    #[test]
    fn backreferences_and_lookaround_are_rejected_at_compile_time() {
        let unsupported = [
            r"(a)\1",   // 後方参照
            r"\1",      // 後方参照（グループを伴わない）
            r"a(?=b)",  // 先読み
            r"a(?!b)",  // 否定先読み
            r"(?<=a)b", // 後読み
            r"(?<!a)b", // 否定後読み
        ];
        for pattern in unsupported {
            match TextPattern::compile(pattern, POSITION) {
                Err(SchemaError::MalformedDeclaration { position, reason }) => {
                    assert_eq!(POSITION, position);
                    assert!(!reason.is_empty(), "{pattern} の診断理由が空である");
                }
                other => panic!("{pattern} がコンパイル時に拒否されない: {other:?}"),
            }
        }

        // 対応する構文（名前つきグループ・数の指定）は拒否されない。
        assert!(TextPattern::compile(r"^(?P<year>\d{4})-\d{2}$", POSITION).is_ok());
    }
}

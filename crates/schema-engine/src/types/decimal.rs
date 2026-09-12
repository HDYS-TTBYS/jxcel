//! 10 進数の文字列上の桁検査と正準化（design.md「コンポーネントとファイルの対応」の
//! `DecimalDigits`。tasks.md 2.2 が実装する。要件 2.3）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは [`super`]（`TypeCatalog`）の下に置かれ、
//! 10 進数の**桁勘定と比較のための正準形**だけを所有する。上流の `document-format` の
//! 値（`CellValue::Decimal` の文字列）以外に依存せず、宣言の誤り
//! （[`SchemaError`](crate::error::SchemaError)）も値の違反
//! （[`Violation`](crate::validate::report::Violation)）も生成しない。**どちらを組み立てるかは
//! 呼び出し元が決める**（宣言の解析は `declaration` 層、違反の理由は `validate` 層。
//! `TypeCatalog` のモジュール docs「誤り型を持たない」と同じ規約）。
//!
//! # なぜ 10 進数のライブラリを入れないか（research.md の裁定）
//!
//! `rust_decimal` / `bigdecimal` / `fastnum` はいずれも出力時に何かを正規化する（先頭の 0、
//! `+`、指数形）。`document-format` の `Decimal` は**文字列のまま逐語で往復する**契約であり、
//! 正規化はその契約を壊す。桁数の検査と、比較のための正準形は ASCII の 1 パス走査で足りる。
//! **保存される文字列は変えない**（正準形は比較のためだけに作る）。
//!
//! # 文法は上流の wire 規則と同じ
//!
//! 受理する形は `document-format` の `value.rs` が `Decimal` の判別に使う文法
//! `^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$` と同じである（ASCII の数字のみ。先頭の 0・
//! 明示的な正符号・指数形・末尾の点を受理し、`.` だけの表記は拒否する）。上流の判別関数は
//! 非公開であり、上流が知るのは「その文字列が `Decimal` と読まれるか」だけであるため、
//! **桁を数える本モジュールが同じ文法をもう一度書く**。食い違うと、上流が保存した
//! `Decimal` を本モジュールが解釈できない（またはその逆の）状態になる。テスト
//! `the_grammar_matches_the_wire_decimal_grammar` が、上流の復号（
//! [`document_format::from_json_bytes`]）と判別が一致することを標本ごとに検査する。
//!
//! # 桁勘定の定義（要件 2.3）
//!
//! 走査は符号・整数部・小数点・小数部・指数を 1 パスでたどり、次の 2 つを数える。
//!
//! - **整数部の桁数**: 値の小数点より上の桁数。**先頭の 0 は桁として数えない**
//!   （`000123.45` の整数部は 3 桁）。指数は小数点を動かすため、指数が正なら末尾に 0 が
//!   付いた桁として数え（`1.5e3` は 4 桁）、負なら桁が小数点の下へ移る。
//! - **小数点以下の桁数**: 値の小数点以下の桁数。指数が負ならその分だけ増え（`1e-3` は
//!   3 桁）、正なら減る（`1.5e1` は 1 桁）。
//!
//! 宣言された `precision`（有効桁数）と `scale`（小数点以下の桁数）に対する適合は、
//! **`scale` に揃えた形の桁数が `precision` 以内であること**と、**書かれた小数部が `scale`
//! 以内であること**の 2 つで決まる（`NUMERIC(p, s)` と同じ規則。`0.001` は `precision = 3,
//! scale = 3` で適合し、`12345` は `precision = 5, scale = 3` で適合しない。後者は `scale`
//! に揃えると整数部に 5 桁を要求するためである）。指数が `i64` で表せない入力は、宣言しうる
//! どの桁数にも収まらない（`scale` / `precision` は `u32`）ため走査の結果としない。
//!
//! # 正準形は値そのものを表す（桁を切り捨てない）
//!
//! 比較のための正準形（[`DecimalCanonical`]）は、宣言された `scale` に揃えるとは
//! **値として同一視すること**（`1.5` と `1.50` を同じ値と見ること）であり、`scale` を
//! 超える小数部を切り捨てること**ではない**。切り捨てると `1.501` と `1.502` が同じ値に
//! なり、一意制約（タスク 5.2）が誤った重複を報告する。同じ理由で、範囲制約（タスク 4.2）
//! が違反値の大小を判定するときも桁を落とさない。正準形は指数を展開しないため、
//! `1e100000` のような値でもその大きさの記憶域を使わない。

use super::Acceptance;
use core::cmp::Ordering;

/// 宣言された桁数（design.md「組込型カタログと `CellValue` への写像」表の `decimal` の
/// パラメータ `precision` / `scale`。要件 2.3）。
///
/// `precision` は有効桁数、`scale` は小数点以下の桁数である。**`precision >= 1` かつ
/// `scale <= precision`** を不変条件とし、これに反する宣言は [`DecimalDigits::new`] が
/// 受け付けない。`scale > precision` の宣言は、整数部に 1 桁も許さない一方で小数点以下に
/// `scale` 桁を要求することになり、適合しうる値が存在しない（`0.001` は `precision = 3,
/// scale = 3` に収まるが、`precision = 2, scale = 3` には収まらない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecimalDigits {
    precision: u32,
    scale: u32,
}

impl DecimalDigits {
    /// 宣言された桁数を作る（要件 2.3）。
    ///
    /// `precision == 0` または `scale > precision` なら `None`。0 桁の 10 進数は存在しえず、
    /// `scale > precision` に適合する値も存在しない（型の docs 参照）。
    pub const fn new(precision: u32, scale: u32) -> Option<Self> {
        if precision == 0 || scale > precision {
            return None;
        }
        Some(Self { precision, scale })
    }

    /// 有効桁数。
    pub const fn precision(self) -> u32 {
        self.precision
    }

    /// 小数点以下の桁数。
    pub const fn scale(self) -> u32 {
        self.scale
    }

    /// 値が文法に一致し、かつ宣言された桁数に収まるか（要件 2.3, 2.7）。
    ///
    /// 文法に一致しない文字列は**違反**を返す。`CellValue::Decimal` は逐語で往復する契約の
    /// ため文法外の中身（上流の脱出口 `{"$t":"decimal",...}` で書かれた値）も持ちうるが、
    /// それは 10 進数の型の値として解釈できない。
    pub fn accepts(self, text: &str) -> Acceptance {
        match scan(text) {
            Some(scanned) if scanned.fits(self) => Acceptance::Conforming,
            _ => Acceptance::Violating,
        }
    }
}

/// 文法に一致した 10 進数の桁勘定（tasks.md 2.2。要件 2.3）。
///
/// 数えるのは**書かれた形**（符号・指数を含む）ではなく**値の桁**である。モジュール docs
/// 「桁勘定の定義」が正典。**元の文字列は保持せず、変更もしない**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecimalScan {
    negative: bool,
    significant_digits: u64,
    fraction_digits: u64,
    exponent: i64,
}

impl DecimalScan {
    /// 値が負か。値 0 は符号を持たない（`-0` は `false`）。
    pub const fn negative(self) -> bool {
        self.negative
    }

    /// 仮数の桁数から先頭の 0 を除いた数（値 0 は 0）。
    pub const fn significant_digits(self) -> u64 {
        self.significant_digits
    }

    /// **書かれた**小数点以下の桁数（指数を適用する前。値 0 は 0）。
    pub const fn fraction_digits(self) -> u64 {
        self.fraction_digits
    }

    /// 書かれた指数。
    pub const fn exponent(self) -> i64 {
        self.exponent
    }

    /// 値の小数点より上の桁数（先頭の 0 を除く。値 0 は 0）。
    ///
    /// 値 = 仮数 × 10^(指数 - 書かれた小数部の桁数) であるため、仮数の桁数（先頭の 0 を
    /// 除く）に指数と書かれた小数部の差を足した数が、小数点より上の桁数になる。0 以下なら
    /// 整数部は 0 である（`0.0005` の整数部は 0 桁）。
    ///
    /// 計算は `i128` で行う。文法が受理する指数は `i64` の全範囲を取るため、`i64` では
    /// 桁数の比較が飽和して異なる値が同じ桁数になりうる。`u64` に収まらないときだけ
    /// 飽和させる（比較相手は `u32` の桁数であり、収まらないことが分かれば足りる）。
    pub fn digits_before_point(self) -> u64 {
        if self.significant_digits == 0 {
            // 値は 0 である。指数も書かれた小数部も桁を持たない（`0e5` は 0 桁）。
            return 0;
        }
        let before = i128::from(self.significant_digits) + i128::from(self.exponent)
            - i128::from(self.fraction_digits);
        if before <= 0 {
            0
        } else {
            u64::try_from(before).unwrap_or(u64::MAX)
        }
    }

    /// 値の小数点以下の桁数（値 0 は 0）。
    ///
    /// 書かれた小数部の桁数から指数を引いた数であり、指数が負なら増える（`1e-3` は 3 桁）。
    pub fn digits_after_point(self) -> u64 {
        if self.significant_digits == 0 {
            return 0;
        }
        let after = i128::from(self.fraction_digits) - i128::from(self.exponent);
        if after <= 0 {
            0
        } else {
            u64::try_from(after).unwrap_or(u64::MAX)
        }
    }

    /// 値が宣言された桁数に収まるか（要件 2.3）。
    ///
    /// `scale` に揃えた形の桁数（整数部 + `scale`）が `precision` 以内であることと、
    /// **書かれた**小数部が `scale` 以内であることの 2 つを要求する（モジュール docs
    /// 「桁勘定の定義」。`0.001` は `precision = 3, scale = 3` に収まり、`12345` は
    /// `precision = 5, scale = 3` に収まらない）。
    pub fn fits(self, declared: DecimalDigits) -> bool {
        let scale = u64::from(declared.scale);
        let precision = u64::from(declared.precision);
        self.digits_after_point() <= scale
            && self.digits_before_point().saturating_add(scale) <= precision
    }
}

/// 比較のための正準形（tasks.md 2.2。要件 2.3）。
///
/// **保存される文字列ではない。** 値そのものを符号・数字列・10 の指数で表し
/// （値 = `digits × 10^exponent`。`digits` は先頭と末尾の 0 を持たず、値 0 では空）、同じ値の
/// 異なる書き方（先頭の 0・明示的な正符号・指数形・末尾の 0）を 1 つに畳む。
/// この畳み方が「宣言された `scale` に揃える」の実体であり、**桁は切り捨てない**
/// （モジュール docs「正準形は値そのものを表す」）。
///
/// 値ごとに表現が一意であるため、等値（[`PartialEq`]）とハッシュ（[`Hash`]）は導出でき、
/// 一意制約の 1 パス判定（タスク 5.2）がそのまま使える。順序（[`Ord`]）は値の順序に一致する。
/// 指数は `i128` で厳密に持つため（[`DecimalCanonical::exponent`]）、文法が受理する
/// `i64` の全範囲の指数でも表現が衝突せず、等値と順序が食い違わない。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DecimalCanonical {
    negative: bool,
    digits: String,
    exponent: i128,
}

impl DecimalCanonical {
    /// 正準形の符号。値 0 は符号を持たない。
    pub fn negative(&self) -> bool {
        self.negative
    }

    /// 先頭と末尾の 0 を除いた数字列（値 0 では空）。
    pub fn digits(&self) -> &str {
        &self.digits
    }

    /// 10 の指数（値 = `digits × 10^exponent`）。
    ///
    /// `i64` ではなく `i128` で持つ。文法が受理する指数は `i64` の全範囲を取るうえ、小数点を
    /// 桁数分動かすため、`i64` では表せない値が生じて異なる値が同じ指数に飽和しうる。
    pub fn exponent(&self) -> i128 {
        self.exponent
    }

    /// 整数部の桁数（小数点より上の桁数。1 未満の値では 0 以下）。
    ///
    /// 値 = `digits × 10^exponent` であるため、`digits` の長さに指数を足した数が先頭の桁の
    /// 位置になる。大小はまずこの数で決まり、同じなら桁が同じ位置に揃っているので数字列を
    /// 短い側から 0 で埋めて比べれば足りる（[`compare_digits`]）。**値 0 は数字列が空**で
    /// あり、この数の比較は当てはまらない（[`Ord`] が先に扱う）。
    fn magnitude(&self) -> i128 {
        i128::try_from(self.digits.len())
            .unwrap_or(i128::MAX)
            .saturating_add(self.exponent)
    }
}

impl Ord for DecimalCanonical {
    /// 値の順序。等値（導出した [`PartialEq`]）と整合する。正準形は値ごとに一意であり、
    /// 大小は符号 → 整数部の桁数 → 数字列の順に決まる。
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => return Ordering::Greater,
            (true, false) => return Ordering::Less,
            _ => {}
        }
        // 値 0 は空の数字列で表す。この時点で双方の符号は同じ（どちらも非負か、どちらも負）
        // であるため、0 は非負側の最小・負側の最大であり、他の値との大小は空か否かで決まる
        // （`0` の整数部の桁数 0 を `0.0001` の -3 と比べると大小が逆になる）。
        let magnitude = match (self.digits.is_empty(), other.digits.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self
                .magnitude()
                .cmp(&other.magnitude())
                .then_with(|| compare_digits(&self.digits, &other.digits)),
        };
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}

impl PartialOrd for DecimalCanonical {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 10 進数の文法に一致するかを 1 パスで走査し、桁勘定を返す（要件 2.3）。
///
/// 文法に一致しない入力と、指数が `i64` で表せない入力（宣言しうるどの桁数にも収まらない）
/// は `None`。**元の文字列は変更しない。**
pub fn scan(text: &str) -> Option<DecimalScan> {
    let parsed = parse(text)?;
    let mantissa = parsed.integer.len() + parsed.fraction.len();
    let leading_zeros = parsed
        .integer
        .iter()
        .chain(parsed.fraction.iter())
        .take_while(|byte| **byte == b'0')
        .count();
    let significant_digits = mantissa - leading_zeros;
    Some(DecimalScan {
        // 値 0 は符号を持たない（`-0` は 0 である）。
        negative: parsed.negative && significant_digits > 0,
        significant_digits: u64::try_from(significant_digits).ok()?,
        fraction_digits: u64::try_from(parsed.fraction.len()).ok()?,
        exponent: parsed.exponent,
    })
}

/// 比較のための正準形を作る（[`DecimalCanonical`]。要件 2.3）。
///
/// 文法に一致しない入力は `None`（[`scan`] と同じ門）。
pub fn canonicalize(text: &str) -> Option<DecimalCanonical> {
    let parsed = parse(text)?;
    let first_significant = parsed
        .integer
        .iter()
        .chain(parsed.fraction.iter())
        .position(|byte| *byte != b'0');
    let Some(first_significant) = first_significant else {
        // 値は 0 である。符号も指数も持たず、桁も無い（`-0` と `0.00` を同じ値にする）。
        return Some(DecimalCanonical {
            negative: false,
            digits: String::new(),
            exponent: 0,
        });
    };
    let mut digits = String::with_capacity(parsed.integer.len() + parsed.fraction.len());
    if first_significant < parsed.integer.len() {
        digits.push_str(std::str::from_utf8(&parsed.integer[first_significant..]).ok()?);
        digits.push_str(std::str::from_utf8(parsed.fraction).ok()?);
    } else {
        // 整数部はすべて 0 である（先頭の 0 として落ちる）。
        let start = first_significant - parsed.integer.len();
        digits.push_str(std::str::from_utf8(&parsed.fraction[start..]).ok()?);
    }
    // 値 = 仮数 × 10^(指数 - 書かれた小数部の桁数) であり、末尾の 0 を除いた分だけ指数が
    // 上がる。桁は落とさない（モジュール docs「正準形は値そのものを表す」）。
    let trailing_zeros = digits.len() - digits.trim_end_matches('0').len();
    digits.truncate(digits.len() - trailing_zeros);
    let fraction = i128::from(i64::try_from(parsed.fraction.len()).ok()?);
    let trailing_zeros = i128::from(i64::try_from(trailing_zeros).ok()?);
    Some(DecimalCanonical {
        negative: parsed.negative,
        digits,
        // 指数は `i128` で厳密に計算する（`i64` の全範囲の指数と桁数が同時に来ても飽和しない）。
        exponent: i128::from(parsed.exponent) - fraction + trailing_zeros,
    })
}

/// 文法に一致した 10 進数の分解。数字は入力の一部を指す（この時点では確保しない）。
struct Parsed<'a> {
    negative: bool,
    integer: &'a [u8],
    fraction: &'a [u8],
    exponent: i64,
}

/// 文法 `^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$` に 1 パスで一致するか確かめ、分解する。
///
/// 上流の `document-format` の `value.rs` の判別と同じ文法である（モジュール docs
/// 「文法は上流の wire 規則と同じ」）。指数が `i64` で表せない入力は `None` とする。
/// そのような値は `u32` の `precision` / `scale` では表せず、適合しえないためである。
fn parse(text: &str) -> Option<Parsed<'_>> {
    let bytes = text.as_bytes();
    let mut cursor = 0usize;
    let negative = match byte_at(bytes, cursor) {
        Some(b'-') => {
            cursor += 1;
            true
        }
        Some(b'+') => {
            cursor += 1;
            false
        }
        _ => false,
    };
    let integer_start = cursor;
    while matches!(byte_at(bytes, cursor), Some(b'0'..=b'9')) {
        cursor += 1;
    }
    let integer = &bytes[integer_start..cursor];
    let mut fraction: &[u8] = &[];
    if byte_at(bytes, cursor) == Some(b'.') {
        cursor += 1;
        let fraction_start = cursor;
        while matches!(byte_at(bytes, cursor), Some(b'0'..=b'9')) {
            cursor += 1;
        }
        fraction = &bytes[fraction_start..cursor];
    }
    // 数字を 1 つも持たない表記（`+` や `.` だけ）は文法違反である。
    if integer.is_empty() && fraction.is_empty() {
        return None;
    }
    let mut exponent = 0i64;
    if matches!(byte_at(bytes, cursor), Some(b'e') | Some(b'E')) {
        cursor += 1;
        let exponent_negative = match byte_at(bytes, cursor) {
            Some(b'-') => {
                cursor += 1;
                true
            }
            Some(b'+') => {
                cursor += 1;
                false
            }
            _ => false,
        };
        let exponent_start = cursor;
        let mut magnitude = 0i64;
        while let Some(byte) = byte_at(bytes, cursor) {
            if !byte.is_ascii_digit() {
                break;
            }
            match magnitude
                .checked_mul(10)
                .and_then(|value| value.checked_add(i64::from(byte - b'0')))
            {
                Some(next) => magnitude = next,
                // 指数が `i64` の範囲を超えた。`u32` の桁数では表せない大きさである。
                None => return None,
            }
            cursor += 1;
        }
        if cursor == exponent_start {
            // `e` のあとに数字が無い（`1e` / `1e+`）。
            return None;
        }
        exponent = if exponent_negative {
            -magnitude
        } else {
            magnitude
        };
    }
    if cursor != bytes.len() {
        return None;
    }
    Some(Parsed {
        negative,
        integer,
        fraction,
        exponent,
    })
}

/// 範囲外を `None` と読む（ASCII 比較なので UTF-8 の継続バイトはすべて非該当）。
#[inline]
fn byte_at(bytes: &[u8], index: usize) -> Option<u8> {
    bytes.get(index).copied()
}

/// 正準形の数字列を、短い側を `0` で埋めて比較する（整数部の桁数が同じときの大小）。
fn compare_digits(left: &str, right: &str) -> Ordering {
    let width = left.len().max(right.len());
    for index in 0..width {
        let left_digit = left.as_bytes().get(index).copied().unwrap_or(b'0');
        let right_digit = right.as_bytes().get(index).copied().unwrap_or(b'0');
        match left_digit.cmp(&right_digit) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_format::{from_json_bytes, CellValue};
    use std::collections::HashSet;

    /// テスト前提の妥当な桁数を組み立てる。
    fn declared(precision: u32, scale: u32) -> DecimalDigits {
        DecimalDigits::new(precision, scale).expect("テスト前提: 妥当な桁数")
    }

    /// 文法に一致することを前提に桁勘定を取り出す。
    fn counted(text: &str) -> DecimalScan {
        scan(text).expect("テスト前提: 文法に一致する")
    }

    /// 先頭の 0 は桁として数えない（要件 2.3）。`000123.45` の整数部は 3 桁である。
    #[test]
    fn leading_zeros_are_not_digits() {
        let scanned = counted("000123.45");
        assert_eq!(5, scanned.significant_digits(), "仮数の桁数が 5 でない");
        assert_eq!(3, scanned.digits_before_point(), "先頭の 0 を桁に数えた");
        assert_eq!(2, scanned.digits_after_point());
        // 先頭の 0 を桁に数えると `precision - scale = 3` に収まらない。
        assert_eq!(Acceptance::Conforming, declared(5, 2).accepts("000123.45"));
        assert_eq!(Acceptance::Conforming, declared(5, 2).accepts("123.45"));
        assert_eq!(Acceptance::Violating, declared(4, 2).accepts("000123.45"));
        assert_eq!(Acceptance::Violating, declared(5, 1).accepts("000123.45"));
        // 値 0 の先頭の 0 も桁にしない。
        assert_eq!(0, counted("000.000").significant_digits());
        assert_eq!(0, counted("000.000").digits_before_point());
        assert_eq!(0, counted("000.000").digits_after_point());
    }

    /// 明示的な正符号は桁として数えない（要件 2.3）。`+1.50` は `1.50` と同じ桁勘定になる。
    #[test]
    fn an_explicit_plus_sign_is_not_a_digit() {
        let plus = counted("+1.50");
        let plain = counted("1.50");
        assert_eq!(plain.significant_digits(), plus.significant_digits());
        assert_eq!(plain.digits_before_point(), plus.digits_before_point());
        assert_eq!(plain.digits_after_point(), plus.digits_after_point());
        assert!(!plus.negative(), "`+` を負と読んだ");
        assert_eq!(Acceptance::Conforming, declared(3, 2).accepts("+1.50"));
        // 整数部 1 桁 + `scale` 2 桁 = 3 桁は `precision` 2 に収まらない。
        assert_eq!(Acceptance::Violating, declared(2, 2).accepts("+1.50"));
    }

    /// 指数は小数点を動かすだけで、桁そのものは変えない（要件 2.3）。
    #[test]
    fn an_exponent_moves_the_point_without_changing_the_digits() {
        let cases: [(&str, u64, u64); 5] = [
            ("1.5e3", 4, 0),
            ("15e-1", 1, 1),
            ("1e-3", 0, 3),
            ("0.5e2", 2, 0),
            ("0.0005e5", 2, 0),
        ];
        for (text, before, after) in cases {
            assert_eq!(
                before,
                counted(text).digits_before_point(),
                "{text} の整数部の桁数"
            );
            assert_eq!(
                after,
                counted(text).digits_after_point(),
                "{text} の小数点以下の桁数"
            );
        }
        assert_eq!(Acceptance::Conforming, declared(4, 0).accepts("1.5e3"));
        assert_eq!(Acceptance::Violating, declared(3, 0).accepts("1.5e3"));
        assert_eq!(Acceptance::Conforming, declared(3, 3).accepts("1e-3"));
        assert_eq!(Acceptance::Violating, declared(3, 2).accepts("1e-3"));
    }

    /// 空の小数部（末尾の点）は小数点以下の桁を持たない（要件 2.3）。
    #[test]
    fn an_empty_fraction_adds_no_digits() {
        let scanned = counted("+1.");
        assert_eq!(0, scanned.fraction_digits(), "空の小数部を 1 桁と数えた");
        assert_eq!(1, scanned.significant_digits());
        assert_eq!(1, scanned.digits_before_point());
        assert_eq!(0, scanned.digits_after_point());
        assert_eq!(Acceptance::Conforming, declared(1, 0).accepts("+1."));
        assert_eq!(scan("1"), scan("+1."), "符号と末尾の点が桁勘定に漏れた");
        // 点だけの表記（数字を含まない）は文法に一致しない。
        assert_eq!(None, scan("."));
        assert_eq!(None, scan("+."));
    }

    /// 文法は上流の wire 規則（`document-format` の `Decimal` の判別）と同じである（要件 2.3）。
    #[test]
    fn the_grammar_matches_the_wire_decimal_grammar() {
        let accepted = [
            "0", "1", "-1", "+1", "1.", "+1.", ".5", "-.5", "00.10", "1E+3", "1e-3", "007",
        ];
        let rejected = [
            "",
            ".",
            "+",
            "-",
            "1e",
            "1e+",
            "1.2.3",
            "1,5",
            "2026/09/12",
            " 1",
            "1 ",
            "1_000",
            "NaN",
            "１２３",
        ];
        for text in accepted {
            assert!(scan(text).is_some(), "{text} を文法違反とした");
        }
        for text in rejected {
            assert!(scan(text).is_none(), "{text} を文法に一致とした");
        }
        // 上流の復号と判別が一致すること（裸の文字列は `Decimal` に決まる。上流の規則 1）。
        for text in accepted.iter().chain(rejected.iter()) {
            let wire = format!("\"{text}\"");
            let decoded = from_json_bytes(wire.as_bytes()).expect("裸の文字列は読める");
            let upstream_decimal = matches!(decoded, CellValue::Decimal(_));
            assert_eq!(
                scan(text).is_some(),
                upstream_decimal,
                "{text} の判別が上流と食い違う",
            );
        }
    }

    /// 正準形は比較のためだけに作り、**保存される文字列を変えない**（tasks.md 2.2）。
    #[test]
    fn canonicalizing_never_changes_the_stored_string() {
        let stored = CellValue::Decimal("+001.50".to_owned());
        let before = stored.clone();
        let canonical = canonicalize("+001.50").expect("文法に一致する");
        assert_eq!(stored, before, "保存されたセル値が変わった");
        match &stored {
            CellValue::Decimal(text) => assert_eq!("+001.50", text, "逐語の文字列が変わった"),
            other => panic!("テスト前提: Decimal だが {other:?}"),
        }
        assert_eq!(
            canonicalize("1.5").expect("文法に一致する"),
            canonical,
            "同じ値の正準形が違う",
        );
        assert_eq!("15", canonical.digits());
        assert_eq!(-1, canonical.exponent());
        assert!(!canonical.negative());
    }

    /// 同じ値の異なる書き方は同じ正準形になる（要件 2.3。一意制約と範囲制約の前提）。
    #[test]
    fn equal_values_share_one_canonical_form() {
        let same = ["1.5", "+1.5", "1.50", "15e-1", "0.15e1", "001.500"];
        let first = canonicalize(same[0]).expect("文法に一致する");
        let mut seen = HashSet::new();
        for text in same {
            let canonical = canonicalize(text).expect("文法に一致する");
            assert_eq!(first, canonical, "{text} の正準形が違う");
            assert_eq!(first.clone(), canonical.clone());
            seen.insert(canonical);
        }
        assert_eq!(1, seen.len(), "同じ値が別の正準形になった");
        // 値 0 は書き方に依らず 1 つの正準形になる（符号も指数も持たない）。
        let zeros = ["0", "-0", "+0.00", "0e5", "0.000e-3"];
        let zero = canonicalize(zeros[0]).expect("文法に一致する");
        for text in zeros {
            let canonical = canonicalize(text).expect("文法に一致する");
            assert_eq!(zero, canonical, "{text} の正準形が 0 と違う");
            assert!(!canonical.negative(), "{text} が負になった");
            assert!(canonical.digits().is_empty(), "{text} が桁を持った");
            assert_eq!(0, canonical.exponent(), "{text} が指数を持った");
        }
    }

    /// 正準形の順序は値の順序に一致する（要件 2.3。範囲制約が使う）。
    #[test]
    fn canonical_ordering_follows_the_numeric_order() {
        let ascending = [
            "-1000", "-2", "-1.5", "-0.05", "-0.001", "0", "0.0001", "0.5", "1", "1.5", "2", "10",
            "99.999", "100", "1e3",
        ];
        let expected: Vec<DecimalCanonical> = ascending
            .iter()
            .map(|text| canonicalize(text).expect("文法に一致する"))
            .collect();
        let mut shuffled: Vec<DecimalCanonical> = expected.iter().rev().cloned().collect();
        shuffled.sort();
        assert_eq!(expected, shuffled, "正準形の順序が値の順序と食い違う");
    }

    /// 極端な指数でも値の同一性と順序が保たれる（桁数の比較を飽和させない。tasks.md 2.2）。
    #[test]
    fn extreme_exponents_keep_their_identity_and_order() {
        let high = canonicalize("1e9223372036854775807").expect("文法に一致する");
        let lower = canonicalize("1e9223372036854775806").expect("文法に一致する");
        assert_ne!(high, lower, "極端な指数で別の値が同一視された");
        assert!(high > lower, "極端な指数で順序が壊れた");
        let low = canonicalize("1e-9223372036854775807").expect("文法に一致する");
        let higher = canonicalize("1e-9223372036854775806").expect("文法に一致する");
        assert_ne!(low, higher, "極端な負の指数で別の値が同一視された");
        assert!(low < higher, "極端な負の指数で順序が壊れた");
    }

    /// 文法外の入力と、指数が `i64` で表せない入力は正準形を作れない（tasks.md 2.2）。
    #[test]
    fn out_of_range_inputs_have_no_canonical_form() {
        for text in ["", "abc", "1e", ".", "1.2.3", "1,5", "NaN"] {
            assert_eq!(None, canonicalize(text), "{text} の正準形が作れた");
        }
        // 指数が `i64` を超える値は、`u32` の桁数では表せない。
        let overflow = "1e99999999999999999999";
        assert_eq!(None, scan(overflow), "指数の飽和を検出していない");
        assert_eq!(None, canonicalize(overflow));
        assert_eq!(
            Acceptance::Violating,
            declared(u32::MAX, 0).accepts(overflow)
        );
    }

    /// 存在しえない桁数の宣言は受け付けない（要件 2.3）。
    #[test]
    fn impossible_digit_declarations_are_rejected() {
        assert_eq!(None, DecimalDigits::new(0, 0), "0 桁の 10 進数は存在しない");
        assert_eq!(None, DecimalDigits::new(0, 0));
        assert_eq!(None, DecimalDigits::new(2, 3), "scale > precision");
        let digits = DecimalDigits::new(3, 3).expect("妥当");
        assert_eq!(3, digits.precision());
        assert_eq!(3, digits.scale());
        assert!(
            DecimalDigits::new(1, 0).is_some(),
            "整数専用の宣言を拒否した"
        );
        assert!(DecimalDigits::new(38, 10).is_some());
    }

    /// 値 0 は書き方に依らず常に適合し、指数を桁に数えない（要件 2.3）。
    #[test]
    fn zero_fits_every_declared_digits() {
        for text in ["0", "-0", "+0.00", "0e5", "000.000", "0e-3"] {
            let scanned = counted(text);
            assert_eq!(0, scanned.digits_before_point(), "{text}");
            assert_eq!(0, scanned.digits_after_point(), "{text}");
            assert_eq!(
                Acceptance::Conforming,
                declared(1, 0).accepts(text),
                "{text}"
            );
            assert_eq!(
                Acceptance::Conforming,
                declared(1, 1).accepts(text),
                "{text}"
            );
        }
    }

    /// `CellValue::Decimal` は逐語で往復するため文法外の中身も持ちうる。それは違反になる。
    #[test]
    fn decimal_values_outside_the_grammar_violate() {
        for text in ["", "abc", "1.2.3", "1,5", " 1"] {
            assert_eq!(
                Acceptance::Violating,
                declared(38, 10).accepts(text),
                "{text} を適合とした",
            );
        }
    }

    /// 指数が大きくても正準形は桁を展開しない（比較のための表現。要件 10.1 の予算を守る）。
    #[test]
    fn a_large_exponent_does_not_expand_zeros() {
        let big = canonicalize("1e100000").expect("文法に一致する");
        assert_eq!("1", big.digits());
        assert_eq!(100_000, big.exponent());
        assert_eq!(big, canonicalize("10e99999").expect("文法に一致する"));
        assert!(
            canonicalize("1e100000").expect("文法に一致する")
                > canonicalize("1e99999").expect("文法に一致する"),
            "指数の大小が順序に現れない",
        );
    }
}

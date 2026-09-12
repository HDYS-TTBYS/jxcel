//! 日時の厳密な解釈と正準表記（design.md「コンポーネントとファイルの対応」の
//! `TemporalValue`。tasks.md 2.3 が実装する。要件 2.4, 7.3）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは [`super`]（`TypeCatalog`）の下に置かれ、日付・civil
//! 日時・オフセット付き瞬時の**厳密な解釈と正準表記**だけを所有する。上流の
//! `document-format` の値（`CellValue::Text` の文字列）以外に依存せず、宣言の誤り
//! （[`SchemaError`](crate::error::SchemaError)）も値の違反
//! （[`Violation`](crate::validate::report::Violation)）も生成しない。**どちらを組み立てるかは
//! 呼び出し元が決める**（[`DecimalDigits`](super::decimal::DecimalDigits) と同じ規約）。
//!
//! # 3 つの形（要件 2.4。tasks.md 2.3）
//!
//! [`TemporalForm`] は design.md「組込型カタログと `CellValue` への写像」表の `date` /
//! `datetime` と、`datetime` の `offset`（`forbidden` / `required`）の 2 値を写す。すなわち
//! **日付のみ**（[`TemporalForm::Date`]）・**時刻を含む（オフセットなし）**
//! （[`TemporalForm::DateTime`] かつ [`OffsetPolicy::Forbidden`]）・**オフセットを要求する**
//! （[`TemporalForm::DateTime`] かつ [`OffsetPolicy::Required`]）の 3 つである。
//!
//! # 受理する綴りは正準表記そのもの（要件 7.3）
//!
//! 文法は次の 3 つだけである。`YYYY` は 4 桁、`MM` `DD` `HH` `MM` `SS` は 2 桁の ASCII 数字
//! であり、区切りは `-` と `:`、`T` は大文字に限る。
//!
//! ```text
//! 日付のみ:   YYYY-MM-DD
//! civil 日時: YYYY-MM-DDTHH:MM:SS[.<小数>]
//! 瞬時:       YYYY-MM-DDTHH:MM:SS[.<小数>](Z|±HH:MM)
//! ```
//!
//! - **小数部**は 1〜9 桁で、末尾の桁は `0` でない（正準表記は末尾の 0 を持たない。
//!   `.5` と `.05` と `.123456789` は正準であり、`.0` と `.50` は正準でない）。
//! - **オフセット**は `Z` か `±HH:MM` である。オフセット 0 の唯一の綴りは `Z` であり、
//!   `+00:00` と `-00:00` は同じオフセットの別の綴りであるため正準でない。
//! - 前後の空白・桁区切り・別の区切り・書式の推測が要る綴り・曜日名・和暦・タイムゾーン名と
//!   その略号・タイムゾーン注釈つきの値は**すべて拒否**する（要件 7.3）。
//!
//! 受理は正準表記との厳密一致であるため、[`TemporalForm::accepts`] が適合を返した値は
//! [`TemporalValue::canonical`] がそのまま返す。すなわち **`accepts` は値を書き換えない**。
//! design.md「Coerce Layer」の変換の規則表にある `Text` → `date` / `datetime` の
//! 「正準表記に厳密一致すればそのまま」がこれであり、`Coercer`（タスク 6.1）は
//! 適合した `Text` を変換せず、適合しない `Text` を違反とするだけでよい。
//!
//! # タイムゾーン注釈を扱わない（design.md「組込型カタログと `CellValue` への写像」）
//!
//! IANA のタイムゾーン注釈（`[Asia/Tokyo]`）とタイムゾーン名・略号（`JST` / `UTC` / `GMT`）は
//! 解釈しない。扱うにはタイムゾーンデータベースを実行ファイルへ同梱することになり、
//! 単一実行ファイルのサイズに直接効くためである（`Cargo.toml` の依存方針 3）。オフセット付き
//! の値は**書かれたオフセットだけ**を持ち、注釈とオフセットの矛盾を判定する必要は生じない
//! （注釈つきの値は文法の段階で拒否される）。
//!
//! # 文法の走査は自前、暦・時計・オフセットの妥当性は jiff に委ねる
//!
//! `jiff` の既定の解析は**要件 7.3 と「正準表記に厳密一致」に合わない**。実測では
//! `20260912`（基本形式）・`2026-09-12T10:00:00`（日付だけの値を切る）・
//! `2026-09-12 10:30:00`（空白区切り）・`2026-09-12T10:30`（秒を省く）・`+0900` と `+09`
//! （区切りの省略）・`2026-09-12T10:30:00+09:00[Asia/Tokyo]`（注釈を黙って捨てる）・
//! `:60`（59 秒へ丸める）を受理する。したがって文法の走査は本モジュールが持ち、
//! **成分の妥当性**（2 月 30 日・13 月・24 時・60 分・60 秒・オフセットの範囲）だけを
//! `jiff` の `civil::Date::new` / `civil::Time::new` / `Offset::from_seconds` に委ねる。
//! うるう秒（`:60`）は扱わない（丸めを避ける）。
//!
//! # 値の同一性は瞬時で、正準表記は綴りを保つ
//!
//! オフセット付きの値はローカル日時とオフセットの対として解釈し、正準表記もその対を保つ。
//! したがって `2026-09-12T10:30:00+09:00` と `2026-09-12T01:30:00Z` は**同じ瞬時**であるが
//! **別の正準表記**を持つ（保存される文字列は逐語で往復するため、この 2 つは別の値である）。
//! 順序（[`Ord`]）と等値（[`PartialEq`]）は瞬時で判定する。範囲制約（タスク 4.2）は
//! オフセットの異なる上下限を正しく比べる必要があるためである。一意制約（タスク 5.2）が
//! 「同じ瞬時」と「同じ綴り」のどちらを使うかは、その比較の目的が決める
//! （本モジュールは両方の手段を提供する）。

use core::cmp::Ordering;

use jiff::civil;
use jiff::tz::{Offset, TimeZone};
use jiff::Timestamp;

use super::Acceptance;

/// 宣言されたオフセットの扱い（design.md「組込型カタログと `CellValue` への写像」表の
/// `datetime` のパラメータ `offset`。要件 2.4）。
///
/// 宣言の文法では `"forbidden"` / `"required"` の 2 値であり、`declaration` 層（タスク 3.1）が
/// その 2 値の宣言をこの型へ写す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OffsetPolicy {
    /// オフセットを持たない（civil 日時）。`"forbidden"`。
    Forbidden,
    /// オフセットを要求する（オフセット付きの瞬時）。`"required"`。
    Required,
}

/// 日時の型が要求する形（要件 2.4。tasks.md 2.3 の「3 つの形」）。
///
/// design.md の組込型カタログ表の `date` / `datetime` と、`datetime` の `offset` を写す
/// （モジュール docs「3 つの形」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemporalForm {
    /// `date` 種別。日付のみで、時刻もオフセットも持たない（`YYYY-MM-DD`）。
    Date,
    /// `datetime` 種別。時刻を含む。オフセットの扱いは [`OffsetPolicy`] が決める。
    DateTime {
        /// 宣言されたオフセットの扱い。
        offset: OffsetPolicy,
    },
}

impl TemporalForm {
    /// 値がこの形の正準表記に厳密一致するか（要件 2.4, 2.7, 7.3）。
    ///
    /// 適合を返した値は、そのままの綴りで保存してよい（[`TemporalValue::canonical`] が入力と
    /// 同じ文字列を返す）。
    pub fn accepts(self, text: &str) -> Acceptance {
        match self.parse(text) {
            Some(_) => Acceptance::Conforming,
            None => Acceptance::Violating,
        }
    }

    /// 正準表記に厳密一致する値を解釈する（要件 2.4, 7.3）。
    ///
    /// 文法に一致しない値・成分が成立しない値は `None`。`None` は「適合しない」であり、
    /// 誤りの種類（書式・暦・範囲）は本層では区別しない（呼び出し元が期待と実際から作る）。
    pub fn parse(self, text: &str) -> Option<TemporalValue> {
        build(self, scan(self, text)?)
    }
}

/// 文法に一致した日時の分解（確保しない）。
struct Parts {
    year: i16,
    month: i8,
    day: i8,
    /// 時・分・秒・ナノ秒。日付のみの形では `None`。
    time: Option<(i8, i8, i8, i32)>,
    /// オフセット（秒）。オフセットを持たない形では `None`。
    offset_seconds: Option<i32>,
}

/// 正準表記の文法を 1 パスで走査し、分解する（モジュール docs「受理する綴りは正準表記
/// そのもの」の文法が正典）。
///
/// 形が要求しない成分は受理しない（[`TemporalForm::Date`] は時刻もオフセットも持たず、
/// [`OffsetPolicy::Forbidden`] はオフセットを持たない）。**成分の妥当性は見ない**
/// （暦・時計・オフセットの範囲は [`build`] が `jiff` へ委ねる）。
fn scan(form: TemporalForm, text: &str) -> Option<Parts> {
    let bytes = text.as_bytes();
    let year = digits(bytes, 0, 4)?;
    if bytes.get(4) != Some(&b'-') {
        return None;
    }
    let month = digits(bytes, 5, 2)?;
    if bytes.get(7) != Some(&b'-') {
        return None;
    }
    let day = digits(bytes, 8, 2)?;
    let mut index = 10;
    let (time, offset_seconds) = match form {
        TemporalForm::Date => (None, None),
        TemporalForm::DateTime { offset } => {
            if bytes.get(index) != Some(&b'T') {
                return None;
            }
            index += 1;
            let hour = digits(bytes, index, 2)?;
            index += 2;
            if bytes.get(index) != Some(&b':') {
                return None;
            }
            index += 1;
            let minute = digits(bytes, index, 2)?;
            index += 2;
            if bytes.get(index) != Some(&b':') {
                return None;
            }
            index += 1;
            let second = digits(bytes, index, 2)?;
            index += 2;
            let mut nanos = 0;
            if bytes.get(index) == Some(&b'.') {
                index += 1;
                let start = index;
                while matches!(bytes.get(index), Some(b'0'..=b'9')) {
                    index += 1;
                }
                let written = &bytes[start..index];
                // 小数部は 1〜9 桁で、末尾の桁は 0 でない（正準表記は末尾の 0 を持たない）。
                if written.is_empty() || written.len() > 9 || written[written.len() - 1] == b'0' {
                    return None;
                }
                nanos = fraction_nanos(written);
            }
            let offset_seconds = match offset {
                OffsetPolicy::Forbidden => None,
                OffsetPolicy::Required => {
                    let (seconds, used) = scan_offset(&bytes[index..])?;
                    index += used;
                    Some(seconds)
                }
            };
            (
                Some((hour as i8, minute as i8, second as i8, nanos)),
                offset_seconds,
            )
        }
    };
    if index != bytes.len() {
        return None;
    }
    Some(Parts {
        year: year as i16,
        month: month as i8,
        day: day as i8,
        time,
        offset_seconds,
    })
}

/// オフセット（`Z` か `±HH:MM`）を読み、秒と読んだ長さを返す。
///
/// `Z` はオフセット 0 の唯一の綴りであり、`+00:00` と `-00:00` は同じオフセットの別の綴り
/// であるため正準でない（モジュール docs）。時・分の範囲は [`Offset::from_seconds`] が判定する。
fn scan_offset(bytes: &[u8]) -> Option<(i32, usize)> {
    match bytes.first()? {
        b'Z' => Some((0, 1)),
        sign @ (b'+' | b'-') => {
            let hours = digits(bytes, 1, 2)?;
            if bytes.get(3) != Some(&b':') {
                return None;
            }
            let minutes = digits(bytes, 4, 2)?;
            if minutes > 59 {
                return None;
            }
            let seconds = hours * 3600 + minutes * 60;
            if seconds == 0 {
                // オフセット 0 の唯一の綴りは `Z`。
                return None;
            }
            Some((if *sign == b'-' { -seconds } else { seconds }, 6))
        }
        _ => None,
    }
}

/// 文法の分解から値を作る。暦・時計・オフセットの妥当性は `jiff` の成分の検査に委ねる
/// （モジュール docs「文法の走査は自前、暦・時計・オフセットの妥当性は jiff に委ねる」）。
fn build(form: TemporalForm, parts: Parts) -> Option<TemporalValue> {
    let date = civil::Date::new(parts.year, parts.month, parts.day).ok()?;
    match form {
        TemporalForm::Date => Some(TemporalValue::Date(date)),
        TemporalForm::DateTime {
            offset: OffsetPolicy::Forbidden,
        } => {
            let (hour, minute, second, nanos) = parts.time?;
            let time = civil::Time::new(hour, minute, second, nanos).ok()?;
            Some(TemporalValue::Civil(civil::DateTime::from_parts(
                date, time,
            )))
        }
        TemporalForm::DateTime {
            offset: OffsetPolicy::Required,
        } => {
            let (hour, minute, second, nanos) = parts.time?;
            let time = civil::Time::new(hour, minute, second, nanos).ok()?;
            let offset = Offset::from_seconds(parts.offset_seconds?).ok()?;
            let local = civil::DateTime::from_parts(date, time);
            let at = local.to_zoned(TimeZone::fixed(offset)).ok()?.timestamp();
            Some(TemporalValue::Instant { local, offset, at })
        }
    }
}

/// ASCII 数字を `count` 桁読み、その値を返す。
///
/// 桁数が足りない場合と ASCII 数字でないバイトを含む場合は `None`（UTF-8 の継続バイトは
/// すべて非該当であり、ASCII 数字の位置で文字が切れることはない）。
fn digits(bytes: &[u8], start: usize, count: usize) -> Option<i32> {
    let slice = bytes.get(start..start.checked_add(count)?)?;
    let mut value = 0i32;
    for &byte in slice {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + i32::from(byte - b'0');
    }
    Some(value)
}

/// 小数部（1〜9 桁、末尾は 0 でない）をナノ秒へ直す。
fn fraction_nanos(digits: &[u8]) -> i32 {
    let mut nanos = 0i32;
    for &byte in digits {
        nanos = nanos * 10 + i32::from(byte - b'0');
    }
    for _ in digits.len()..9 {
        nanos *= 10;
    }
    nanos
}

/// 厳密に解釈された日時の値（tasks.md 2.3。要件 2.4, 7.3）。
///
/// 変種は [`TemporalForm`] の 3 つの形に対応する。いずれも**正準表記に厳密一致した値**
/// だけから作られ、[`TemporalValue::canonical`] は元の綴りを再現する。
#[derive(Debug, Clone, Copy)]
pub enum TemporalValue {
    /// 日付のみの値（[`TemporalForm::Date`]）。
    Date(civil::Date),
    /// オフセットを持たない civil 日時の値（[`TemporalForm::DateTime`] かつ
    /// [`OffsetPolicy::Forbidden`]）。
    Civil(civil::DateTime),
    /// オフセット付きの瞬時の値（[`TemporalForm::DateTime`] かつ [`OffsetPolicy::Required`]）。
    ///
    /// 値は `at`（瞬時）であり、`local` と `offset` は**書かれた綴り**を保つ。`at` は `local` と
    /// `offset` が表す瞬時であり、この 3 つを揃えて作るのは [`TemporalForm::parse`] だけである。
    Instant {
        /// 書かれたローカル日時。
        local: civil::DateTime,
        /// 書かれたオフセット。
        offset: Offset,
        /// ローカル日時とオフセットが表す瞬時。
        at: Timestamp,
    },
}

impl TemporalValue {
    /// この値の形（解釈に使った [`TemporalForm`]）。
    pub fn form(self) -> TemporalForm {
        match self {
            TemporalValue::Date(_) => TemporalForm::Date,
            TemporalValue::Civil(_) => TemporalForm::DateTime {
                offset: OffsetPolicy::Forbidden,
            },
            TemporalValue::Instant { .. } => TemporalForm::DateTime {
                offset: OffsetPolicy::Required,
            },
        }
    }

    /// 正準表記（要件 2.4）。[`TemporalForm::parse`] が受理した綴りをそのまま再現する。
    pub fn canonical(self) -> String {
        let mut out = String::new();
        self.write_canonical(&mut out);
        out
    }

    /// 正準表記を `out` へ書き足す（割り当てを避けたい呼び出し元向け）。
    pub fn write_canonical(self, out: &mut String) {
        match self {
            TemporalValue::Date(date) => write_date(out, date),
            TemporalValue::Civil(local) => write_local(out, local),
            TemporalValue::Instant { local, offset, .. } => {
                write_local(out, local);
                write_offset(out, offset);
            }
        }
    }

    /// 形の判別（異なる形の比較を全域にするためだけの内部値）。
    fn rank(self) -> u8 {
        match self {
            TemporalValue::Date(_) => 0,
            TemporalValue::Civil(_) => 1,
            TemporalValue::Instant { .. } => 2,
        }
    }
}

/// 日付を `YYYY-MM-DD` で書く。
fn write_date(out: &mut String, date: civil::Date) {
    use core::fmt::Write as _;

    let _ = write!(
        out,
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month(),
        date.day()
    );
}

/// ローカル日時を `YYYY-MM-DDTHH:MM:SS[.<小数>]` で書く。
///
/// 小数部は**末尾の 0 を書かない**（正準表記。受理した綴りが末尾の 0 を持たないため、
/// ここでも同じ桁数が再現される）。
fn write_local(out: &mut String, local: civil::DateTime) {
    use core::fmt::Write as _;

    write_date(out, local.date());
    let time = local.time();
    let _ = write!(
        out,
        "T{:02}:{:02}:{:02}",
        time.hour(),
        time.minute(),
        time.second()
    );
    let nanos = time.subsec_nanosecond();
    if nanos != 0 {
        let mut value = nanos;
        let mut width = 9;
        while value % 10 == 0 {
            value /= 10;
            width -= 1;
        }
        let _ = write!(out, ".{value:0width$}");
    }
}

/// オフセットを `Z` か `±HH:MM` で書く（オフセット 0 の唯一の綴りは `Z`）。
fn write_offset(out: &mut String, offset: Offset) {
    use core::fmt::Write as _;

    let seconds = offset.seconds();
    if seconds == 0 {
        out.push('Z');
        return;
    }
    let (sign, seconds) = if seconds < 0 {
        ('-', -seconds)
    } else {
        ('+', seconds)
    };
    let _ = write!(
        out,
        "{sign}{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60
    );
}

impl PartialEq for TemporalValue {
    /// 値（日付・civil 日時・瞬時）として等しいか。書かれたオフセットの違いは値の違いでは
    /// ない（モジュール docs「値の同一性は瞬時で、正準表記は綴りを保つ」）。
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for TemporalValue {}

impl PartialOrd for TemporalValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TemporalValue {
    /// 値の順序（要件 2.4。範囲制約が使う）。形が同じなら日付・civil 日時・瞬時の順に比べ、
    /// 形が異なれば形の判別で決める（同じ列の上下限は同じ形であり、この比較は起こらない）。
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (TemporalValue::Date(left), TemporalValue::Date(right)) => left.cmp(right),
            (TemporalValue::Civil(left), TemporalValue::Civil(right)) => left.cmp(right),
            (TemporalValue::Instant { at: left, .. }, TemporalValue::Instant { at: right, .. }) => {
                left.cmp(right)
            }
            _ => self.rank().cmp(&other.rank()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 日付のみの形。
    const DATE: TemporalForm = TemporalForm::Date;
    /// 時刻を含む（オフセットなし）の形。
    const CIVIL: TemporalForm = TemporalForm::DateTime {
        offset: OffsetPolicy::Forbidden,
    };
    /// オフセットを要求する形。
    const INSTANT: TemporalForm = TemporalForm::DateTime {
        offset: OffsetPolicy::Required,
    };
    /// tasks.md 2.3 が挙げる 3 つの形。
    const ALL_FORMS: [TemporalForm; 3] = [DATE, CIVIL, INSTANT];

    /// 適合する値は解釈でき、**受理された綴りがそのまま正準表記である**ことを確かめる
    /// （受理は正準表記との厳密一致。モジュール docs）。
    fn conforming(form: TemporalForm, text: &str) {
        assert_eq!(
            Acceptance::Conforming,
            form.accepts(text),
            "{form:?} が {text:?} を拒否した",
        );
        let value = form.parse(text).expect("適合した値は解釈できる");
        assert_eq!(form, value.form(), "{text:?} の形が宣言と食い違う");
        assert_eq!(text, value.canonical(), "{text:?} が自分の正準表記と違う");
    }

    /// 適合しない値は解釈できないことを確かめる。
    fn violating(form: TemporalForm, text: &str) {
        assert_eq!(
            Acceptance::Violating,
            form.accepts(text),
            "{form:?} が {text:?} を受理した",
        );
        assert_eq!(None, form.parse(text), "{form:?} が {text:?} を解釈した");
    }

    /// 3 つの形が、それぞれの正準表記を受理する（tasks.md 2.3「日付のみ・時刻を含む
    /// （オフセットなし）・オフセットを要求する」。要件 2.4）。
    #[test]
    fn each_form_accepts_its_canonical_spelling() {
        for text in ["2026-09-12", "0001-01-01", "2024-02-29", "9999-12-31"] {
            conforming(DATE, text);
        }
        for text in [
            "2026-09-12T10:30:00",
            "2026-09-12T00:00:00",
            "2026-09-12T23:59:59",
            "2026-09-12T10:30:00.5",
            "2026-09-12T10:30:00.05",
            "2026-09-12T10:30:00.123456789",
        ] {
            conforming(CIVIL, text);
        }
        for text in [
            "2026-09-12T10:30:00Z",
            "2026-09-12T10:30:00.5+09:00",
            "2026-09-12T10:30:00-05:30",
            "2026-09-12T23:59:59.999999999+14:00",
        ] {
            conforming(INSTANT, text);
        }
    }

    /// 形が違えば、他の形の正準表記は受理しない（tasks.md 2.3 の 3 つの形の区別）。
    #[test]
    fn a_form_rejects_the_other_forms_spellings() {
        // 日付のみの形は、時刻もオフセットも持たない。
        for text in ["2026-09-12T10:30:00", "2026-09-12T10:30:00Z", "2026-09-12Z"] {
            violating(DATE, text);
        }
        // オフセットなしの形は、日付のみもオフセット付きも持たない。
        for text in [
            "2026-09-12",
            "2026-09-12T10:30:00Z",
            "2026-09-12T10:30:00+09:00",
        ] {
            violating(CIVIL, text);
        }
        // オフセットを要求する形は、日付のみもオフセットなしも持たない。
        for text in ["2026-09-12", "2026-09-12T10:30:00", "2026-09-12T10:30:00.5"] {
            violating(INSTANT, text);
        }
    }

    /// tasks.md 2.3 が名指しする拒否対象を、3 つの形すべてで拒否する（要件 7.3）。
    ///
    /// 桁区切りのある数値・`2026/09/12` のような書式・曜日名・和暦・タイムゾーン名の略号・
    /// 日付への `Z` 付与・オフセットと注釈が矛盾する値。
    #[test]
    fn the_named_rejections_are_rejected_by_every_form() {
        let named = [
            // 桁区切りのある数値
            "2,026-09-12",
            "2026-09-12T10,30:00",
            // 書式の推測が要るもの
            "2026/09/12",
            "2026.09.12",
            "12-09-2026",
            // 曜日名
            "Sun, 12 Sep 2026 10:30:00 +0900",
            "2026-09-12T10:30:00 Sunday",
            // 和暦
            "令和8年9月12日",
            "2026年09月12日",
            // タイムゾーン名の略号
            "2026-09-12T10:30:00 JST",
            "2026-09-12T10:30:00 UTC",
            "2026-09-12T10:30:00 GMT",
            // 日付への Z 付与
            "2026-09-12Z",
            // オフセットと注釈が矛盾する値（注釈つきの値は扱わない）
            "2026-09-12T10:30:00+09:00[UTC]",
            "2026-09-12T10:30:00+09:00[Asia/Tokyo]",
            "2026-09-12T10:30:00Z[Asia/Tokyo]",
            "2026-09-12[Asia/Tokyo]",
        ];
        for text in named {
            for form in ALL_FORMS {
                violating(form, text);
            }
        }
    }

    /// 正準表記でない綴りは、値として解釈できるものでも拒否する（tasks.md 2.3）。
    ///
    /// 表記の揺れを受理すると、同じ値が複数の綴りで保存され、表記の推測が保存側へ入り込む。
    #[test]
    fn non_canonical_spellings_are_rejected() {
        for text in [
            "20260912",  // 基本形式（jiff は受理するが正準でない）
            "2026-9-12", // 桁数を省く
            "26-09-12",
            "2026-09-12 ",
            " 2026-09-12",
            "2026-09-12\n",
            "2026-09-12T00:00:00", // 日付のみの形に時刻は付けない
        ] {
            violating(DATE, text);
        }
        for text in [
            "2026-09-12 10:30:00",            // 区切りが空白
            "2026-09-12t10:30:00",            // T が小文字
            "2026-09-12T10:30",               // 秒を省く
            "2026-09-12T10:30:00.",           // 空の小数部
            "2026-09-12T10:30:00.0",          // 末尾が 0 の小数部
            "2026-09-12T10:30:00.50",         // 同上
            "2026-09-12T10:30:00.1234567890", // 10 桁
            "2026-09-12T10:30:00+0900",       // オフセットの区切りを省く
            "2026-09-12T10:30:00+09",
        ] {
            violating(CIVIL, text);
        }
        for text in [
            "2026-09-12T10:30:00z",      // Z が小文字
            "2026-09-12T10:30:00+00:00", // オフセット 0 の唯一の綴りは Z
            "2026-09-12T10:30:00-00:00",
            "2026-09-12T10:30:00+9:00",
            "2026-09-12T10:30:00+09:0",
            "2026-09-12T10:30:00.Z",
        ] {
            violating(INSTANT, text);
        }
    }

    /// 暦・時計・オフセットとして成立しない値は拒否する（成分の妥当性は `jiff` が判定する）。
    #[test]
    fn invalid_calendar_clock_and_offset_values_are_rejected() {
        for text in [
            "2026-02-30",
            "2027-02-29", // 平年の 2 月 29 日
            "2026-04-31",
            "2026-13-01",
            "2026-00-10",
            "2026-01-00",
        ] {
            violating(DATE, text);
        }
        for text in [
            "2026-02-30T10:00:00",
            "2026-09-12T24:00:00",
            "2026-09-12T10:60:00",
            "2026-09-12T10:30:60", // うるう秒は扱わない（丸めない）
        ] {
            violating(CIVIL, text);
        }
        for text in [
            "2026-09-12T10:30:00+26:00", // オフセットの範囲外
            "2026-09-12T10:30:00-26:00",
            "2026-09-12T10:30:00+09:60",
        ] {
            violating(INSTANT, text);
        }
    }

    /// 同じ瞬時を別のオフセットで書いた値は、値としては等しく、綴りとしては別である
    /// （モジュール docs「値の同一性は瞬時で、正準表記は綴りを保つ」）。
    #[test]
    fn the_same_instant_in_two_offsets_is_one_value_with_two_spellings() {
        let tokyo = INSTANT
            .parse("2026-09-12T10:30:00+09:00")
            .expect("適合する");
        let utc = INSTANT.parse("2026-09-12T01:30:00Z").expect("適合する");
        assert_eq!(Ordering::Equal, tokyo.cmp(&utc), "同じ瞬時が等しくない");
        assert_eq!(tokyo, utc);
        assert_eq!("2026-09-12T10:30:00+09:00", tokyo.canonical());
        assert_eq!("2026-09-12T01:30:00Z", utc.canonical());
    }

    /// 範囲制約が使う順序は値の順序に一致する（要件 2.4）。オフセットが違っても瞬時で比べる。
    #[test]
    fn ordering_follows_the_value() {
        let ascending = [
            "2026-09-11T23:00:00Z",
            "2026-09-12T10:30:00+09:00", // 01:30Z
            "2026-09-12T02:00:00Z",
            "2026-09-13T00:00:00+14:00", // 2026-09-12T10:00Z
            "2026-09-13T01:00:00Z",
        ];
        let expected: Vec<TemporalValue> = ascending
            .iter()
            .map(|text| INSTANT.parse(text).expect("適合する"))
            .collect();
        assert!(
            expected.windows(2).all(|pair| pair[0] < pair[1]),
            "昇順の標本が昇順でない",
        );
        let mut shuffled: Vec<TemporalValue> = expected.iter().rev().copied().collect();
        shuffled.sort();
        assert_eq!(expected, shuffled, "正準形の順序が値の順序と食い違う");

        let dates: Vec<TemporalValue> = ["2026-09-11", "2026-09-12", "2026-10-01", "2027-01-01"]
            .iter()
            .map(|text| DATE.parse(text).expect("適合する"))
            .collect();
        assert!(dates.windows(2).all(|pair| pair[0] < pair[1]), "日付の順序");

        let civils: Vec<TemporalValue> = [
            "2026-09-12T09:59:59",
            "2026-09-12T10:00:00",
            "2026-09-12T10:00:00.5",
            "2026-09-13T00:00:00",
        ]
        .iter()
        .map(|text| CIVIL.parse(text).expect("適合する"))
        .collect();
        assert!(
            civils.windows(2).all(|pair| pair[0] < pair[1]),
            "civil の順序"
        );
    }

    /// 形の異なる値の比較は形の判別で決まる（同じ列の上下限は同じ形であり、この比較は
    /// 起こらない。順序を全域にするための規則）。
    #[test]
    fn different_forms_are_ordered_by_form() {
        let date = DATE.parse("2026-09-12").expect("適合する");
        let civil = CIVIL.parse("2026-09-12T10:30:00").expect("適合する");
        let instant = INSTANT.parse("2026-09-12T10:30:00Z").expect("適合する");
        assert!(date < civil);
        assert!(civil < instant);
        assert!(date < instant);
    }
}

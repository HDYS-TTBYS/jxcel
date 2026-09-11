//! キーボードショートカットの一意性検査（要件 3.3、3.4。design.md「Core Layer /
//! AcceleratorRegistry」）。
//!
//! 同一の組み合わせが二重に登録されたとき、登録の時点で競合として返す。競合には既存の登録と
//! 新しい登録の**両方**を含め、片方を黙って捨てない（要件 3.4）。この検査を自前で持つ必要が
//! ある理由: 競合検出の仕組みは Tauri にも muda にも存在せず、Windows ではアクセラレータ表が
//! ハッシュマップの反復順から構築されるため競合時の勝者が実行ごとに変わりうる
//! （research.md「メニューとキーボードショートカット」）。
//!
//! さらに、Tauri 2.11.5 はメニュー項目のアクセラレータ文字列を `s.parse().ok()` で解釈する
//! （`tauri-2.11.5/src/menu/normal.rs`）。**綴りを間違えた文字列はエラーにならず、その項目だけが
//! ショートカットを失う。** したがって「どの組み合わせがどの登録のものか」を確かめ、誤った
//! 綴りを先に弾く場所はここしかない。
//!
//! # 無言で片方を捨てない
//!
//! [`AcceleratorRegistry::insert`] は、同じ組み合わせを別の登録が要求したとき、**勝者を選ばずに**
//! [`AcceleratorConflict`] を返す。既存の登録はそのまま残り、新しい登録は加わらない。呼び出し元は
//! 衝突した両方の登録（[`AcceleratorConflict::existing`] と [`AcceleratorConflict::incoming`]）を
//! 得るので、どちらの登録元に何を報告すべきか判断できる。既存の登録を上書きする入口は存在しない。
//!
//! # 正規化（等価な綴りを同じ組み合わせとして扱う）
//!
//! 一意性検査は「同じ組み合わせ」の判定が正しくなければ役に立たない。`Ctrl+S` と `ctrl+s`、あるいは
//! `Shift+Control+S` と `ctrl+shift+s` は同じ組み合わせであり、競合として検出しなければならない。
//! [`Accelerator::parse`] は入力を次の規則で**正準形**（canonical form）へ畳む:
//!
//! 1. **構文**: `修飾キー+修飾キー+...+キー`。区切りは `+`。修飾キーは 0 個以上、キーはちょうど
//!    1 個で最後に置く（modifiers first, key last。`Shift+Alt+KeyQ` は正しく、`Shift+KeyQ+Alt` は
//!    誤り）。文字列全体と各トークンの前後の空白は無視する。空のトークン（`Ctrl++S`）は誤り。
//! 2. **大文字小文字**: 修飾キー名とキー名の ASCII の大文字小文字は区別しない。
//! 3. **修飾キーの別名**: `ctrl` = `control`、`alt` = `option`、`shift`、`super` = `cmd` =
//!    `command` をそれぞれ同じ修飾キーへ畳む（`MODIFIER_ALIASES`）。
//! 4. **修飾キーの順序**: 正準形では固定順（`MODIFIER_ORDER`）
//!    （`ctrl` → `alt` → `shift` → `super`）に並べ替える。`alt+ctrl+s` と `ctrl+alt+s` は同じ。
//! 5. **重複する修飾キー**: 同じ修飾キーの繰り返しは 1 つに畳む（muda と同じ。`Ctrl+Ctrl+S` と
//!    `Ctrl+S` は同じ組み合わせ）。
//! 6. **キーの別名**: プラットフォーム層（muda 0.19.3 の `accelerator::parse_code`）と同じ閉じた
//!    別名表（`KEY_ALIASES`）で正準名へ畳む。`S` = `KeyS`、`Esc` = `Escape`、`Num4` =
//!    `Numpad4`、`,` = `Comma` はそれぞれ同じキーである。
//!
//! 正準形は、修飾キーを小文字の正準トークン、キーを `KeyS` / `ArrowUp` / `Digit0` のような
//! プラットフォームが受理する名前で `+` で連結した文字列である（例: `ctrl+shift+KeyS`、`F11`）。
//! [`Accelerator::as_str`] が返すのはこの形であり、**表示用ではない**。要件 3.3 のメニュー上の
//! 表示（`Ctrl+Shift+S` のようなプラットフォームの表記）は 7.5 が別に組む。
//!
//! # 構文の契約（7.4 / 7.5 との合意）
//!
//! 受理する構文は**プラットフォームのメニュー層から借りている**のであり、ここで発明したものでは
//! ない。Tauri 2.11.5 はアクセラレータ文字列を muda 0.19.3 の `Accelerator: FromStr` で解析する
//! ため、本モジュールはその別名表を写している（`KEY_ALIASES` / `MODIFIER_ALIASES`。
//! `Cargo.lock` の muda は 0.19.3）。**基盤の構文が変わればこの表も更新しなければならない。**
//! 7.4 / 7.5 は、メニューへ渡すのと同じ綴りをここへ渡すこと。
//!
//! 例外は `CmdOrCtrl`（`CmdOrControl` / `CommandOrCtrl` / `CommandOrControl`）だけであり、
//! **意図的に受理しない**（[`AcceleratorParseError::PlatformDependentModifier`]）。この別名は
//! プラットフォームによって `Ctrl`（非 macOS）にも `Cmd` = `Super`（macOS）にも畳まれる。
//! Core はプラットフォーム非依存でなければならないので、`CmdOrCtrl` を独立した組み合わせとして
//! 受理すると、Windows / Linux で `Ctrl+S` と `CmdOrCtrl+S` が同じキーになる競合を見落とす。
//! どちらか一方へ畳むと、もう一方のプラットフォームで存在しない競合を報告することになる。
//! したがって 7.4 / 7.5（アダプタ層）が**プラットフォーム解決済みの修飾キー**（非 macOS は
//! `Ctrl`、macOS は `Cmd` / `Super`）を渡す。解決してから渡す限り、同じ論理ショートカットは
//! どのプラットフォームでも同じ綴りになり、競合は必ず検出される。
//!
//! # 同じ登録の再登録（冪等）
//!
//! メニューは再構築される（7.5 はフォーカス移動のたびに項目の有効・無効を更新する）。同じ登録元が
//! 同じ項目を同じ組み合わせで再登録しても競合ではない。**登録の同一性は「登録元 + 項目」の組**で
//! あり、同じ組の再登録は状態を変えずに成功する。競合は**異なる登録**が同じ組み合わせを要求した
//! ときだけである。したがって、同じ登録元でも別の項目が同じ組み合わせを要求すれば競合であり
//! （要件 3.4 は「複数の項目」について述べている）、同じ項目が別の組み合わせで再登録された場合は
//! その項目の組み合わせの更新として扱う（他の登録と衝突するなら競合し、状態は変えない）。
//!
//! # 列挙順（メニュー描画の安定性）
//!
//! [`AcceleratorRegistry::registrations`] は**登録元の文字列昇順、同じ登録元の中では項目の文字列
//! 昇順**で返す（内部の [`BTreeMap`] の順序）。挿入順にもハッシュ順にも依存しないため、メニューを
//! 再構築しても描画順が変わらない。
//!
//! # 呼び出し元が登録を失わせる残りの経路
//!
//! 同じ組み合わせを二重登録する経路は [`AcceleratorRegistry::insert`] が塞いでいる。残るのは
//! 明示的な [`AcceleratorRegistry::remove`] だけであり、これは登録元自身が項目を畳む操作であって
//! 競合の解決手段ではない（`insert` が返した [`AcceleratorConflict`] を捨てて `remove` で片方を
//! 消す、という使い方をしても、消えた事実は呼び出し元が持っている）。`insert` の `Err` を無視
//! することもできるが、その場合も登録は「加わらなかった」だけで、「上書きされて片方が消える」
//! 経路は存在しない。

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// 正準化の表（プラットフォームの構文を写したもの）
// ---------------------------------------------------------------------------

/// 修飾キーの別名（(別名, 正準トークン)）。ASCII の大文字小文字は区別しない。
///
/// muda 0.19.3 の `accelerator::parse_modifier` と同じ集合である。
const MODIFIER_ALIASES: &[(&str, &str)] = &[
    ("ctrl", "ctrl"),
    ("control", "ctrl"),
    ("alt", "alt"),
    ("option", "alt"),
    ("shift", "shift"),
    ("super", "super"),
    ("cmd", "super"),
    ("command", "super"),
];

/// `CmdOrCtrl` 系の別名。**受理しない**（[`AcceleratorParseError::PlatformDependentModifier`]）。
///
/// プラットフォームによって `Ctrl` にも `Super` にも畳まれるため、Core では独立した組み合わせとして
/// 扱えない。認識だけして専用のエラーを返し、黙って別の修飾キーとして登録されることを防ぐ。
const PLATFORM_DEPENDENT_MODIFIER_ALIASES: &[&str] = &[
    "cmdorctrl",
    "cmdorcontrol",
    "commandorctrl",
    "commandorcontrol",
];

/// 正準形での修飾キーの並び順。この順に並べ替えるので、入力の順序は結果に現れない。
const MODIFIER_ORDER: &[&str] = &["ctrl", "alt", "shift", "super"];

/// キーの別名表（(正準名, 別名の並び)）。ASCII の大文字小文字は区別しない。
///
/// muda 0.19.3 の `accelerator::parse_code` と同じ閉じた集合である。正準名はそのまま
/// プラットフォームへ渡せる綴り（muda が受理する）である。**キーを追加・変更するときは
/// `Cargo.lock` の muda の版と突き合わせること**（module doc「構文の契約」）。
static KEY_ALIASES: &[(&str, &[&str])] = &[
    ("Backquote", &["Backquote", "`"]),
    ("Backslash", &["Backslash", "\\"]),
    ("BracketLeft", &["BracketLeft", "["]),
    ("BracketRight", &["BracketRight", "]"]),
    ("Comma", &["Comma", ","]),
    ("Digit0", &["Digit0", "0"]),
    ("Digit1", &["Digit1", "1"]),
    ("Digit2", &["Digit2", "2"]),
    ("Digit3", &["Digit3", "3"]),
    ("Digit4", &["Digit4", "4"]),
    ("Digit5", &["Digit5", "5"]),
    ("Digit6", &["Digit6", "6"]),
    ("Digit7", &["Digit7", "7"]),
    ("Digit8", &["Digit8", "8"]),
    ("Digit9", &["Digit9", "9"]),
    ("Equal", &["Equal", "="]),
    ("KeyA", &["KeyA", "A"]),
    ("KeyB", &["KeyB", "B"]),
    ("KeyC", &["KeyC", "C"]),
    ("KeyD", &["KeyD", "D"]),
    ("KeyE", &["KeyE", "E"]),
    ("KeyF", &["KeyF", "F"]),
    ("KeyG", &["KeyG", "G"]),
    ("KeyH", &["KeyH", "H"]),
    ("KeyI", &["KeyI", "I"]),
    ("KeyJ", &["KeyJ", "J"]),
    ("KeyK", &["KeyK", "K"]),
    ("KeyL", &["KeyL", "L"]),
    ("KeyM", &["KeyM", "M"]),
    ("KeyN", &["KeyN", "N"]),
    ("KeyO", &["KeyO", "O"]),
    ("KeyP", &["KeyP", "P"]),
    ("KeyQ", &["KeyQ", "Q"]),
    ("KeyR", &["KeyR", "R"]),
    ("KeyS", &["KeyS", "S"]),
    ("KeyT", &["KeyT", "T"]),
    ("KeyU", &["KeyU", "U"]),
    ("KeyV", &["KeyV", "V"]),
    ("KeyW", &["KeyW", "W"]),
    ("KeyX", &["KeyX", "X"]),
    ("KeyY", &["KeyY", "Y"]),
    ("KeyZ", &["KeyZ", "Z"]),
    ("Minus", &["Minus", "-"]),
    ("Period", &["Period", "."]),
    ("Quote", &["Quote", "'"]),
    ("Semicolon", &["Semicolon", ";"]),
    ("Slash", &["Slash", "/"]),
    ("Backspace", &["Backspace"]),
    ("CapsLock", &["CapsLock"]),
    ("Enter", &["Enter"]),
    ("Space", &["Space"]),
    ("Tab", &["Tab"]),
    ("Delete", &["Delete"]),
    ("End", &["End"]),
    ("Home", &["Home"]),
    ("Insert", &["Insert"]),
    ("PageDown", &["PageDown"]),
    ("PageUp", &["PageUp"]),
    ("PrintScreen", &["PrintScreen"]),
    ("ScrollLock", &["ScrollLock"]),
    ("ArrowDown", &["ArrowDown", "Down"]),
    ("ArrowLeft", &["ArrowLeft", "Left"]),
    ("ArrowRight", &["ArrowRight", "Right"]),
    ("ArrowUp", &["ArrowUp", "Up"]),
    ("NumLock", &["NumLock"]),
    ("Numpad0", &["Numpad0", "Num0"]),
    ("Numpad1", &["Numpad1", "Num1"]),
    ("Numpad2", &["Numpad2", "Num2"]),
    ("Numpad3", &["Numpad3", "Num3"]),
    ("Numpad4", &["Numpad4", "Num4"]),
    ("Numpad5", &["Numpad5", "Num5"]),
    ("Numpad6", &["Numpad6", "Num6"]),
    ("Numpad7", &["Numpad7", "Num7"]),
    ("Numpad8", &["Numpad8", "Num8"]),
    ("Numpad9", &["Numpad9", "Num9"]),
    ("NumpadAdd", &["NumpadAdd", "NumAdd", "NumpadPlus", "NumPlus"]),
    ("NumpadDecimal", &["NumpadDecimal", "NumDecimal"]),
    ("NumpadDivide", &["NumpadDivide", "NumDivide"]),
    ("NumpadEnter", &["NumpadEnter", "NumEnter"]),
    ("NumpadEqual", &["NumpadEqual", "NumEqual"]),
    ("NumpadMultiply", &["NumpadMultiply", "NumMultiply"]),
    ("NumpadSubtract", &["NumpadSubtract", "NumSubtract"]),
    ("Escape", &["Escape", "Esc"]),
    ("F1", &["F1"]),
    ("F2", &["F2"]),
    ("F3", &["F3"]),
    ("F4", &["F4"]),
    ("F5", &["F5"]),
    ("F6", &["F6"]),
    ("F7", &["F7"]),
    ("F8", &["F8"]),
    ("F9", &["F9"]),
    ("F10", &["F10"]),
    ("F11", &["F11"]),
    ("F12", &["F12"]),
    ("AudioVolumeDown", &["AudioVolumeDown", "VolumeDown"]),
    ("AudioVolumeUp", &["AudioVolumeUp", "VolumeUp"]),
    ("AudioVolumeMute", &["AudioVolumeMute", "VolumeMute"]),
    ("F13", &["F13"]),
    ("F14", &["F14"]),
    ("F15", &["F15"]),
    ("F16", &["F16"]),
    ("F17", &["F17"]),
    ("F18", &["F18"]),
    ("F19", &["F19"]),
    ("F20", &["F20"]),
    ("F21", &["F21"]),
    ("F22", &["F22"]),
    ("F23", &["F23"]),
    ("F24", &["F24"]),
];

// ---------------------------------------------------------------------------
// 組み合わせ（アクセラレータ）
// ---------------------------------------------------------------------------

/// 正規化済みのキーボードショートカット（キーの組み合わせ）。
///
/// 作る入口は [`Accelerator::parse`]（と [`FromStr`]）だけであり、正準形だけを保持する。したがって
/// **同じ組み合わせの等価な綴りは必ず同じ `Accelerator` になる**（[`PartialEq`] / [`Hash`] は正準形の
/// 比較そのものである）。正準形への規則は module doc「正規化」を参照。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Accelerator {
    canonical: String,
}

impl Accelerator {
    /// ショートカット文字列を正準形へ畳む。
    ///
    /// 受理する構文はプラットフォームのメニュー層（muda 0.19.3 の `Accelerator: FromStr`）を写した
    /// ものである（module doc「構文の契約」）。`CmdOrCtrl` 系だけは受理せず
    /// [`AcceleratorParseError::PlatformDependentModifier`] を返す。
    ///
    /// # Errors
    ///
    /// 空文字列（[`AcceleratorParseError::Empty`]）、空のトークン
    /// （[`AcceleratorParseError::EmptyToken`]）、修飾キー位置の未知のトークン
    /// （[`AcceleratorParseError::UnknownModifier`]）、キー位置の未知のトークン
    /// （[`AcceleratorParseError::UnsupportedKey`]）、`CmdOrCtrl` 系
    /// （[`AcceleratorParseError::PlatformDependentModifier`]）を返す。
    ///
    /// ```
    /// use app_shell::accelerator::Accelerator;
    ///
    /// assert_eq!(Accelerator::parse("Shift+Control+S").unwrap().as_str(), "ctrl+shift+KeyS");
    /// assert_eq!(Accelerator::parse("s").unwrap().as_str(), "KeyS");
    /// ```
    pub fn parse(input: &str) -> Result<Self, AcceleratorParseError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(AcceleratorParseError::Empty);
        }

        let (modifiers_part, key_token) = split_modifiers_and_key(trimmed);

        // 修飾キー。muda と同じく重複は 1 つに畳む（OR と同じ意味論）。
        let mut modifiers: Vec<&'static str> = Vec::new();
        if !modifiers_part.trim().is_empty() {
            for token in modifiers_part.split('+') {
                let token = token.trim();
                if token.is_empty() {
                    return Err(AcceleratorParseError::EmptyToken {
                        input: input.to_string(),
                    });
                }
                match recognize_modifier(token) {
                    ModifierToken::Canonical(name) => {
                        if !modifiers.contains(&name) {
                            modifiers.push(name);
                        }
                    }
                    ModifierToken::PlatformDependent => {
                        return Err(AcceleratorParseError::PlatformDependentModifier {
                            input: input.to_string(),
                            token: token.to_string(),
                        });
                    }
                    ModifierToken::Unknown => {
                        return Err(AcceleratorParseError::UnknownModifier {
                            input: input.to_string(),
                            token: token.to_string(),
                        });
                    }
                }
            }
        }

        let key = canonical_key(key_token).ok_or_else(|| AcceleratorParseError::UnsupportedKey {
            input: input.to_string(),
            token: key_token.to_string(),
        })?;

        // 正準形の並び順へ。すべての正準トークンは MODIFIER_ORDER にあるので position は必ず Some。
        modifiers.sort_unstable_by_key(|name| {
            MODIFIER_ORDER
                .iter()
                .position(|canonical| canonical == name)
                .unwrap_or(usize::MAX)
        });

        let mut canonical = String::with_capacity(input.len());
        for name in &modifiers {
            canonical.push_str(name);
            canonical.push('+');
        }
        canonical.push_str(key);

        Ok(Accelerator { canonical })
    }

    /// 正準形を返す。**表示用ではない**（module doc「正規化」）。
    pub fn as_str(&self) -> &str {
        &self.canonical
    }
}

impl FromStr for Accelerator {
    type Err = AcceleratorParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

impl fmt::Display for Accelerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

/// [`Accelerator::parse`] の失敗。原因を区別できる（要件 4.4 の「文字列だけのエラーにしない」に
/// 倣う。呼び出し元は「どのトークンが誤りか」をそのまま報告できる）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AcceleratorParseError {
    /// 空、または空白だけの入力。
    #[error("ショートカット文字列が空である")]
    Empty,

    /// `Ctrl++S` のように空のトークンが含まれる。
    #[error("ショートカット \"{input}\" に空のトークンが含まれる")]
    EmptyToken { input: String },

    /// 修飾キーの位置に、修飾キーとして認識できないトークンがある。キーを修飾キーより前に
    /// 置いた場合（`Shift+KeyQ+Alt`）もここに落ちる。
    #[error("ショートカット \"{input}\" の修飾キー \"{token}\" を認識できない")]
    UnknownModifier { input: String, token: String },

    /// キーの位置に、キーとして認識できないトークンがある。
    #[error("ショートカット \"{input}\" のキー \"{token}\" を認識できない")]
    UnsupportedKey { input: String, token: String },

    /// `CmdOrCtrl` 系の別名。プラットフォームによって意味が変わるため受理しない
    /// （module doc「構文の契約」）。プラットフォーム解決済みの修飾キーを渡すこと。
    #[error(
        "ショートカット \"{input}\" の修飾キー \"{token}\" はプラットフォーム依存である。\
         解決済みの修飾キー（非 macOS は Ctrl、macOS は Cmd / Super）を渡すこと"
    )]
    PlatformDependentModifier { input: String, token: String },
}

/// 修飾キー位置のトークンの判定結果。
enum ModifierToken {
    /// 正準トークン。
    Canonical(&'static str),
    /// `CmdOrCtrl` 系（受理しない。専用のエラーにするため区別する）。
    PlatformDependent,
    /// 未知。
    Unknown,
}

fn recognize_modifier(token: &str) -> ModifierToken {
    if PLATFORM_DEPENDENT_MODIFIER_ALIASES
        .iter()
        .any(|alias| token.eq_ignore_ascii_case(alias))
    {
        return ModifierToken::PlatformDependent;
    }
    match MODIFIER_ALIASES
        .iter()
        .find(|(alias, _)| token.eq_ignore_ascii_case(alias))
    {
        Some((_, canonical)) => ModifierToken::Canonical(canonical),
        None => ModifierToken::Unknown,
    }
}

/// キー位置のトークンを正準名へ畳む。別名表に無ければ `None`。
fn canonical_key(token: &str) -> Option<&'static str> {
    KEY_ALIASES.iter().find_map(|(canonical, aliases)| {
        aliases
            .iter()
            .any(|alias| token.eq_ignore_ascii_case(alias))
            .then_some(*canonical)
    })
}

/// 入力（前後の空白を除いたもの）を修飾キー部とキー部へ分ける。
///
/// 最後の `+` で分けるので、キーは必ず最後のトークンになる。最後の `+` の後が空の場合は
/// `+` 自体をキーとして扱う（muda の `split_key_and_modifiers` と同じ。ただし `+` は
/// [`KEY_ALIASES`] に無いため、結局 [`AcceleratorParseError::UnsupportedKey`] になる）。
fn split_modifiers_and_key(input: &str) -> (&str, &str) {
    match input.rfind('+') {
        Some(index) => {
            let raw_key = &input[index + 1..];
            if raw_key.trim().is_empty() {
                (input[..index].trim_end_matches('+'), "+")
            } else {
                (&input[..index], raw_key.trim())
            }
        }
        None => ("", input),
    }
}

// ---------------------------------------------------------------------------
// 登録元と項目の同一性
// ---------------------------------------------------------------------------

/// ショートカットを登録する側（機能・画面など）の識別子。
///
/// 中身は不透明な文字列である。メニューを再構築しても同じ値を渡すこと（同じ登録の再登録を冪等に
/// 判定する鍵になる。module doc「同じ登録の再登録」）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AcceleratorOwner(String);

impl AcceleratorOwner {
    /// 識別子から作る。
    pub fn new(identity: impl Into<String>) -> Self {
        Self(identity.into())
    }

    /// 識別子を返す。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for AcceleratorOwner {
    fn from(identity: &str) -> Self {
        Self::new(identity)
    }
}

impl From<String> for AcceleratorOwner {
    fn from(identity: String) -> Self {
        Self::new(identity)
    }
}

impl fmt::Display for AcceleratorOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// メニュー項目の識別子。ショートカットが押されたときに**どの項目へ振り向けるか**を表す
/// （7.4 / 7.5 がこの値で登録元の処理を引く）。
///
/// 表示文字列ではなく識別子である。表示の文言は項目側が持つ。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MenuItemId(String);

impl MenuItemId {
    /// 識別子から作る。
    pub fn new(identity: impl Into<String>) -> Self {
        Self(identity.into())
    }

    /// 識別子を返す。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MenuItemId {
    fn from(identity: &str) -> Self {
        Self::new(identity)
    }
}

impl From<String> for MenuItemId {
    fn from(identity: String) -> Self {
        Self::new(identity)
    }
}

impl fmt::Display for MenuItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// 登録
// ---------------------------------------------------------------------------

/// 1 件の登録。登録元と項目の組が登録の同一性であり、組み合わせが一意性検査の対象である。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorRegistration {
    /// 登録元。
    pub owner: AcceleratorOwner,
    /// メニュー項目。押されたショートカットの振り向け先。
    pub item: MenuItemId,
    /// 正規化済みの組み合わせ。
    pub chord: Accelerator,
}

impl fmt::Display for AcceleratorRegistration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{} ({})", self.owner, self.item, self.chord)
    }
}

/// 組み合わせが競合したという報告（design.md「AcceleratorRegistry」の
/// `AcceleratorConflict { chord, existing, incoming }`）。
///
/// **両方の登録を運ぶ。** 呼び出し元はこれを使って「どちらとどちらが衝突したか」を登録元へ報告
/// できる。片方を選んで無効化するための情報ではない（module doc「無言で片方を捨てない」）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "キーボードショートカット \"{chord}\" を既存の登録 {existing} と新しい登録 {incoming} が要求した"
)]
pub struct AcceleratorConflict {
    /// 競合した正規化済みの組み合わせ。
    pub chord: Accelerator,
    /// 先に登録されていた側。競合の後も登録されたままである。
    pub existing: AcceleratorRegistration,
    /// 拒否された側。登録されていない。
    pub incoming: AcceleratorRegistration,
}

// ---------------------------------------------------------------------------
// 登録簿
// ---------------------------------------------------------------------------

/// 登録の同一性（登録元 + 項目）。列挙順はこの組の辞書順である。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RegistrationKey {
    owner: AcceleratorOwner,
    item: MenuItemId,
}

/// ショートカットの登録簿。**同一の組み合わせを二重に登録させない**（要件 3.4。
/// design.md「Core Layer / AcceleratorRegistry」）。
///
/// 登録元（7.4 / 7.5）がシェル内で 1 つ共有し、メニューを構築するたびに
/// [`insert`](AcceleratorRegistry::insert) を呼ぶ。スレッド間の同期は持たない — 登録はメニューの
/// 構築時（メインスレッド）に行われるため、呼び出し元が直列化する。
///
/// # 不変条件
///
/// - `by_registration` の各登録は `by_chord` にちょうど 1 つの索引を持つ
/// - `by_chord` の索引は必ず `by_registration` にある登録を指す
#[derive(Debug, Default, Clone)]
pub struct AcceleratorRegistry {
    /// 登録の同一性 → 登録。列挙順（登録元 → 項目の辞書順）を与える。
    by_registration: BTreeMap<RegistrationKey, AcceleratorRegistration>,
    /// 正規化済みの組み合わせ → それを保持している登録の同一性。
    by_chord: HashMap<Accelerator, RegistrationKey>,
}

impl AcceleratorRegistry {
    /// 空の登録簿を作る。
    pub fn new() -> Self {
        Self::default()
    }

    /// ショートカットを登録する。
    ///
    /// 同じ組み合わせを持つ**別の登録**があれば [`AcceleratorConflict`] を返し、状態を変えない
    /// （既存の登録も新しい登録も失われない）。同じ組み合わせを持つ**同じ登録**（同じ登録元 +
    /// 同じ項目）の再登録は、メニューの再構築として成功し、状態を変えない。
    ///
    /// 同じ項目が別の組み合わせで再登録された場合は、その項目の組み合わせを更新する。ただし新しい
    /// 組み合わせが別の登録と競合する場合は、その更新も [`AcceleratorConflict`] として拒否し、
    /// 状態を変えない。
    ///
    /// # Errors
    ///
    /// 別の登録が同じ組み合わせを保持している場合（[`AcceleratorConflict`]）。
    ///
    /// ```
    /// use app_shell::accelerator::{Accelerator, AcceleratorOwner, AcceleratorRegistry, MenuItemId};
    ///
    /// let mut registry = AcceleratorRegistry::new();
    /// let chord = Accelerator::parse("Ctrl+S").unwrap();
    /// registry
    ///     .insert(AcceleratorOwner::new("document"), MenuItemId::new("save"), chord.clone())
    ///     .unwrap();
    ///
    /// let conflict = registry
    ///     .insert(AcceleratorOwner::new("macro"), MenuItemId::new("save-macro"), chord)
    ///     .unwrap_err();
    /// assert_eq!(conflict.existing.owner.as_str(), "document");
    /// assert_eq!(conflict.incoming.owner.as_str(), "macro");
    /// ```
    pub fn insert(
        &mut self,
        owner: AcceleratorOwner,
        item: MenuItemId,
        chord: Accelerator,
    ) -> Result<(), AcceleratorConflict> {
        let key = RegistrationKey {
            owner: owner.clone(),
            item: item.clone(),
        };

        if let Some(holder) = self.by_chord.get(&chord) {
            if *holder == key {
                // 同じ登録の再登録。メニューの再構築で何度も呼ばれる（冪等）。
                return Ok(());
            }
            let existing = self
                .by_registration
                .get(holder)
                .expect("by_chord の索引は by_registration の登録を指す（構造上の不変条件）")
                .clone();
            return Err(AcceleratorConflict {
                chord: chord.clone(),
                existing,
                incoming: AcceleratorRegistration {
                    owner: key.owner,
                    item: key.item,
                    chord,
                },
            });
        }

        // 同じ項目の組み合わせの更新。旧い組み合わせの索引だけを外す（他の登録には触れない）。
        if let Some(previous) = self.by_registration.remove(&key) {
            self.by_chord.remove(&previous.chord);
        }
        self.by_chord.insert(chord.clone(), key.clone());
        self.by_registration
            .insert(key, AcceleratorRegistration { owner, item, chord });
        Ok(())
    }

    /// 押された組み合わせを、それを保持している登録へ解決する（7.5 の振り向け）。
    ///
    /// 登録されていない組み合わせなら `None`。
    pub fn resolve(&self, chord: &Accelerator) -> Option<&AcceleratorRegistration> {
        self.by_chord
            .get(chord)
            .and_then(|key| self.by_registration.get(key))
    }

    /// 登録元と項目の組から登録を引く。
    pub fn registration(
        &self,
        owner: &AcceleratorOwner,
        item: &MenuItemId,
    ) -> Option<&AcceleratorRegistration> {
        self.by_registration.get(&RegistrationKey {
            owner: owner.clone(),
            item: item.clone(),
        })
    }

    /// 登録を明示的に解除する。メニューの再構築で項目を畳むときに使う。
    ///
    /// **競合の解決手段ではない。** 解除された登録は戻り値として呼び出し元が受け取る
    /// （module doc「呼び出し元が登録を失わせる残りの経路」）。
    pub fn remove(
        &mut self,
        owner: &AcceleratorOwner,
        item: &MenuItemId,
    ) -> Option<AcceleratorRegistration> {
        let key = RegistrationKey {
            owner: owner.clone(),
            item: item.clone(),
        };
        let removed = self.by_registration.remove(&key)?;
        self.by_chord.remove(&removed.chord);
        Some(removed)
    }

    /// 登録を**登録元の文字列昇順、同じ登録元の中では項目の文字列昇順**で列挙する
    /// （module doc「列挙順」）。
    pub fn registrations(&self) -> impl Iterator<Item = &AcceleratorRegistration> {
        self.by_registration.values()
    }

    /// 登録の件数。
    pub fn len(&self) -> usize {
        self.by_registration.len()
    }

    /// 登録が 1 件も無ければ `true`。
    pub fn is_empty(&self) -> bool {
        self.by_registration.is_empty()
    }
}

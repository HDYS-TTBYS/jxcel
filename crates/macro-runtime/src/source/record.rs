//! マクロの記録（design.md「Data Models」の `MacroRecord` / `MacroKind`。tasks.md 1.3 / 1.6）。
//!
//! マクロは**名前・種別・ソース**の 3 つ組の値オブジェクトである（要件 1.1, 1.2）。
//! ソースは**保存されたままのテキスト**であり、整形も正規化もしない（要件 1.5。design.md
//!「Logical Data Model」）。名前は一意であり、同じ名前の保存は置き換えである（要件 1.6）。
//!
//! # このファイルが持つもの / 持たないもの（tasks.md 1.3 と 1.6 の分界）
//!
//! tasks.md 1.3 は実行の要求（`engine::outcome::RunRequest`）が**マクロの記録を運ぶ**ことを
//! 定めるため、その**形**（新型と、それを組み立てる最小の口）をここに置いた。**規則は
//! ここには無い** — 名前の空・重複の判定と置き換えの規則、ソース先頭の能力宣言の解析
//! （綴りの揺れ・重複・未知の能力）、上流のパート（`document-format` の `macros.json`）との
//! 往復は**タスク 1.6** が同じモジュールへ足す。

use core::fmt;

/// マクロの名前（要件 1.6）。
///
/// 1 つのドキュメントの中で一意であり、同じ名前で保存することは**置き換え**である。
/// 空の名前や重複の扱いはタスク 1.6 が決める（本型は値を持つだけである）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MacroName(String);

impl MacroName {
    /// 名前を組み立てる。
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// 名前の文字列。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MacroName {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

impl From<String> for MacroName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl fmt::Display for MacroName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// マクロの種別（要件 3.1, 3.3）。
///
/// TypeScript は変換（型注釈の除去）を経て実行し、JavaScript はそのまま実行する
/// （design.md「Transpiler」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroKind {
    /// TypeScript。型注釈を除去してから実行する。
    TypeScript,
    /// JavaScript。変換しない。
    JavaScript,
}

impl MacroKind {
    /// 保存形式と診断の記録に使う安定トークン（`Display` と同一。ロケール依存なし）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
        }
    }
}

/// マクロの記録（design.md「Data Models」の `MacroRecord`。要件 1.1, 1.2, 1.5）。
///
/// **ソースは保存されたバイト列のまま**保持する。実行の直前の変換（タスク 3.1）も、
/// 上流のパートへの書き出し（タスク 1.6）も、この値を書き換えない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroRecord {
    /// マクロの名前（ドキュメントの中で一意）。
    pub name: MacroName,
    /// マクロの種別。
    pub kind: MacroKind,
    /// 保存されたままのソース。
    pub source: String,
}

impl MacroRecord {
    /// 記録を組み立てる。
    pub fn new(name: MacroName, kind: MacroKind, source: impl Into<String>) -> Self {
        Self {
            name,
            kind,
            source: source.into(),
        }
    }
}

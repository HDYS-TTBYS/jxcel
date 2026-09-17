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

// ---------------------------------------------------------------------------
// 名前で一意な並び（tasks.md 1.6。要件 1.6, 1.7）
// ---------------------------------------------------------------------------

/// 並びの中から名前で 1 件を引く（無ければ `None`）。
pub fn find<'a>(records: &'a [MacroRecord], name: &MacroName) -> Option<&'a MacroRecord> {
    records.iter().find(|record| &record.name == name)
}

/// **同じ名前の保存は置き換えである**（要件 1.6）。
///
/// **位置を保つ**（並びの順序は保存された順であり、一覧の提示順に使う。design.md
/// 「Logical Data Model」）。同じ名前が無ければ末尾へ足す。置き換えたときは `false` を返す
/// （新しく足したときは `true`）— 利用者へ「上書きした」と伝える材料である。
pub fn upsert(records: &mut Vec<MacroRecord>, record: MacroRecord) -> bool {
    match records
        .iter_mut()
        .find(|existing| existing.name == record.name)
    {
        Some(existing) => {
            *existing = record;
            false
        }
        None => {
            records.push(record);
            true
        }
    }
}

/// 名前で 1 件を取り除く（要件 1.7）。取り除いた記録を返す（無ければ `None`）。
pub fn remove(records: &mut Vec<MacroRecord>, name: &MacroName) -> Option<MacroRecord> {
    let position = records.iter().position(|record| &record.name == name)?;
    Some(records.remove(position))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(name: &str, source: &str) -> MacroRecord {
        MacroRecord::new(MacroName::new(name), MacroKind::TypeScript, source)
    }

    #[test]
    fn saving_the_same_name_replaces_in_place_and_keeps_the_saved_order() {
        let mut records = vec![record("棚卸し", "旧"), record("集計", "そのまま")];
        let added = upsert(&mut records, record("棚卸し", "新"));
        assert!(!added, "置き換えを「新しく足した」と報告した");
        assert_eq!(
            records
                .iter()
                .map(|record| (record.name.as_str(), record.source.as_str()))
                .collect::<Vec<_>>(),
            vec![("棚卸し", "新"), ("集計", "そのまま")],
            "置き換えで位置が動いた、または別の記録が変わった"
        );

        let added = upsert(&mut records, record("検算", "足した"));
        assert!(added, "新しい名前を「置き換えた」と報告した");
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn a_record_is_found_and_removed_by_name() {
        let mut records = vec![record("棚卸し", "x"), record("集計", "y")];
        assert_eq!(
            find(&records, &MacroName::new("集計")).map(|record| record.source.as_str()),
            Some("y")
        );
        let removed = remove(&mut records, &MacroName::new("棚卸し")).expect("取り除ける");
        assert_eq!(removed.source, "x");
        assert_eq!(records.len(), 1);
        assert!(remove(&mut records, &MacroName::new("無い")).is_none());
    }
}

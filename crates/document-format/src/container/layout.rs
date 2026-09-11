//! 復号時の許可リスト適用（タスク 5.3。要件 2.5, 2.6。design「Container Layer /
//! EntryLayout」）。
//!
//! 本モジュールは **`zip` にもアーカイブのバイト列にも依存しない純粋な判定**である:
//! 生のエントリ名の列を受け取り、どの名前を受理するか（[`EntryName::parse`] が許可リストの
//! 唯一の権威）と、同じパスのエントリが重複しているかを決める。アーカイブの走査と展開は
//! [`super::reader`] の責務であり、本モジュールの判定は `zip` を使わずに単体で検証できる
//! （この分割が本タスクのテスト戦略の要である）。
//!
//! # 判定順（`zip` を開いた後にここへ来る）
//!
//! 1. **許可リスト照合**: 名前列の全件を [`EntryName::parse`] に通す。1 件でも文法に
//!    一致しなければ、その名前を含む [`DocumentError::InvalidContainer`] で中止する
//!    （要件 2.5）。`zip` の `enclosed_name()` のような**名前の正規化に依存しない**:
//!    絶対パス・`..` 成分・ドライブレター・バックスラッシュ区切り・NUL・ディレクトリ
//!    エントリ（末尾 `/`）は、いずれも 6 形の完全一致に当たらないため自動的に拒否される。
//! 2. **重複検出**: 生の名前の**バイト単位完全一致**で同一パスの重複を探す（要件 2.6）。
//!    比較は名前だけで行い、内容（展開後バイト列）は見ない。したがって**展開より前に
//!    判定が終わる**。
//!
//! 許可リストを先に置くのは、展開も内容の解釈もせずに決められる最も強い条件だからである
//! （不正な名前を含むアーカイブは、中身がどれほど壊れていても名前の段階で拒否される。
//! [`super::reader`] の「判定の先行」テストがこの順序を固定する）。
//!
//! # 重複の定義（畳み込みをしない）
//!
//! 重複とは生のエントリ名がバイト単位で完全に等しいことである。大文字小文字の畳み込み・
//! Unicode 正規化・パーセントデコードの類はしない（`MANIFEST.json` は `manifest.json` の
//! 重複ではない。そもそも許可リストで拒否される）。ZIP は同一名のエントリを複数持てるため
//! （中央ディレクトリは名前の一意性を要求しない）、この検査だけが要件 2.6 を守る。
//!
//! # 返す順序
//!
//! [`admit`] は**入力順**（中央ディレクトリの順序）の [`EntryName`] 列を返す。並べ替えは
//! 論理エントリ集合の正準化（[`crate::parts::DocumentParts::from_entries`]）が行う
//! （本モジュールは集合型を知らない）。
//!
//! # エラー
//!
//! 新しい [`DocumentError`] 変種は足さない。許可リスト違反は [`EntryName::parse`] が返す
//! [`DocumentError::InvalidContainer`] をそのまま伝播し（`entry` は**生の名前そのもの**。
//! 要件 2.5 の「該当エントリ名を含む」）、重複は `entry` = `<名前>: duplicate entry path`
//! とする（要件 2.6 の「該当パスを含む」）。理由の文言はログ用の技術的診断であり、
//! 提示文言は呼び出し元が組み立てる（[`crate::error`] の規約）。

use std::collections::BTreeSet;

use crate::entry_name::EntryName;
use crate::error::DocumentError;

/// 生のエントリ名の列を許可リストと重複検査に通す（要件 2.5, 2.6）。
///
/// 受理した [`EntryName`] を**入力順**で返す。1 件でも許可リストの外にあれば
/// [`DocumentError::InvalidContainer`]（`entry` = 生の名前）で中止し、同一パスが複数あれば
/// 同じ変種（`entry` = `<名前>: duplicate entry path`）で中止する。判定は名前だけで完結し、
/// アーカイブの内容には触れない（展開より前に終わる）。
pub fn admit(names: &[&str]) -> Result<Vec<EntryName>, DocumentError> {
    let mut parsed = Vec::with_capacity(names.len());
    for name in names {
        parsed.push(EntryName::parse(name)?);
    }
    if let Some(path) = duplicate_path(names) {
        return Err(DocumentError::InvalidContainer {
            entry: format!("{path}: duplicate entry path"),
        });
    }
    Ok(parsed)
}

/// バイト単位で完全一致する重複エントリパスを返す（無ければ `None`）。
///
/// 線形の走査ではなく順序集合（[`BTreeSet`]）で既出を判定する（判定は入力順、比較は
/// `&str` のバイト順で、大文字小文字を畳み込まない）。`HashMap` / `HashSet` を使わないのは
/// 本クレート共通の決定性規則（[`crate::json`]）と同じ理由である: 反復順に依存する値を
/// 持ち込まない。
fn duplicate_path<'a>(names: &[&'a str]) -> Option<&'a str> {
    let mut seen: BTreeSet<&'a str> = BTreeSet::new();
    for name in names {
        if !seen.insert(name) {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 標本の ULID（正準 Crockford base32 大文字 26 文字）。
    const SHEET: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    /// 標本の添付ダイジェスト（正準小文字 hex 64 文字）。
    const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// 受理する 6 形（design「Container Entry Layout」）。
    fn allowed_names() -> Vec<(String, EntryName)> {
        vec![
            ("jxcel".to_owned(), EntryName::Marker),
            ("manifest.json".to_owned(), EntryName::Manifest),
            ("document.json".to_owned(), EntryName::Document),
            (
                format!("schemas/{SHEET}.json"),
                EntryName::Schema {
                    sheet: SHEET.parse().expect("標本の ULID"),
                },
            ),
            (
                format!("sheets/{SHEET}.jsonl"),
                EntryName::Rows {
                    sheet: SHEET.parse().expect("標本の ULID"),
                },
            ),
            (
                format!("attachments/{HEX}.bin"),
                EntryName::Attachment {
                    attachment: HEX.parse().expect("標本の hex"),
                },
            ),
        ]
    }

    /// 与えられた名前の `admit` が返すエラーの `entry`（中止しなければ panic）。
    fn rejection(name: &str) -> String {
        match admit(&[name]) {
            Err(DocumentError::InvalidContainer { entry }) => entry,
            other => panic!("{name:?} が拒否されない: {other:?}"),
        }
    }

    /// 受理する 6 形をすべて受け入れる（要件 2.5 の許可リスト）。
    #[test]
    fn admit_accepts_every_allowed_form() {
        let allowed = allowed_names();
        let names: Vec<&str> = allowed.iter().map(|(name, _)| name.as_str()).collect();
        let admitted = admit(&names).expect("6 形はすべて受理される");
        let expected: Vec<EntryName> = allowed.iter().map(|(_, name)| *name).collect();
        assert_eq!(
            expected, admitted,
            "受理したエントリ名が入力順で返らない（または形が違う）"
        );
    }

    /// 許可リスト外・ルート外を指す名前をすべて拒否し、**生の名前をそのまま**エラーに含める
    /// （要件 2.5）。
    #[test]
    fn admit_rejects_names_outside_the_allow_list() {
        let cases = [
            "../evil.json",           // 親ディレクトリへ出る相対パス
            "sheets/../../evil.json", // 許可プレフィックスの後からの脱出
            "/abs.json",              // 絶対パス
            "C:/x.json",              // ドライブレター
            "C:\\x.json",             // ドライブレター（バックスラッシュ）
            "a\\b.json",              // バックスラッシュ区切り
            "a\0b.json",              // NUL
            "sheets/",                // ディレクトリエントリ（末尾 `/`）
            "schemas/",
            "attachments/", // ディレクトリエントリ（許可プレフィックスのみ）
            "evil.json",    // 未知の名前
            "README",
            "Manifest.json", // 大文字小文字の変種
            "MANIFEST.JSON",
            "schemas/01arz3ndektsv4rrffq69g5fav.json", // ULID の小文字
            "attachments/0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef.bin",
            "schemas/01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonx", // 接尾辞の変種
            "sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.json",   // 別形の接尾辞
        ];
        for name in cases {
            let entry = rejection(name);
            assert_eq!(
                name, entry,
                "拒否したエラーが該当エントリ名を原文のまま含んでいない"
            );
        }
    }

    /// 許可リスト違反は重複より先に判定される（判定順）。
    ///
    /// 先頭に許可リスト外の名前、後ろに重複した正当な名前を置く。重複検査が先に走る実装なら
    /// エラーは重複した正当な名前を指すため、このテストで順序が固定される（要件 2.5 は
    /// 「そのエントリを展開せず」を要求するので、名前の段階で止まる必要がある）。
    #[test]
    fn admit_checks_the_allow_list_before_duplicates() {
        let entry = rejection_of(&["../evil.json", "manifest.json", "manifest.json"]);
        assert_eq!(
            "../evil.json", entry,
            "許可リスト違反より重複が先に報告されている"
        );
    }

    /// 同一パスの重複を拒否し、該当パスをエラーに含める（要件 2.6）。
    #[test]
    fn admit_rejects_a_duplicate_path() {
        for name in ["manifest.json", "jxcel", "document.json"] {
            let entry = rejection_of(&["jxcel", name, name]);
            assert!(
                entry.contains(name) && entry.contains("duplicate"),
                "重複のエラーが該当パスと理由を含まない: {entry:?}"
            );
        }
    }

    /// 異なる名前の並びは重複ではない（集合が同じでも、名前が違えば別のエントリ）。
    #[test]
    fn admit_accepts_distinct_paths() {
        let allowed = allowed_names();
        let names: Vec<&str> = allowed.iter().map(|(name, _)| name.as_str()).collect();
        assert!(
            duplicate_path(&names).is_none(),
            "異なる名前が重複と判定された"
        );
        assert_eq!(
            names.len(),
            admit(&names).expect("異なる名前は受理される").len()
        );
    }

    /// 重複判定はバイト単位の完全一致であり、大文字小文字を畳み込まない（要件 2.6）。
    ///
    /// 畳み込む実装（`eq_ignore_ascii_case` / 小文字化）なら `MANIFEST.json` が
    /// `manifest.json` の重複として報告される。許可リスト側が先に大文字小文字の変種を
    /// 拒否するため、重複判定そのものを直接呼んで固定する。
    #[test]
    fn duplicate_detection_is_byte_exact() {
        assert_eq!(
            Some("manifest.json"),
            duplicate_path(&["manifest.json", "MANIFEST.json", "manifest.json"]),
            "バイト単位で一致する最後の出現が報告されない"
        );
        assert!(
            duplicate_path(&["manifest.json", "MANIFEST.json"]).is_none(),
            "大文字小文字を畳み込んで重複と判定している"
        );
        assert!(duplicate_path(&["manifest.json", "manifest.json.json"]).is_none());
        assert!(duplicate_path(&[]).is_none());
    }

    /// 3 件以上の重複でも 1 件目と同一のパスを報告する（線形探索の `windows(2)` では
    /// 隣接しない重複を取り逃すため、非隣接を含む並びで固定する）。
    #[test]
    fn duplicate_detection_finds_non_adjacent_repeats() {
        assert_eq!(
            Some("document.json"),
            duplicate_path(&["document.json", "manifest.json", "document.json"])
        );
    }

    /// 単一の名前に対する `admit` のエラー `entry`（中止しなければ panic）。
    fn rejection_of(names: &[&str]) -> String {
        match admit(names) {
            Err(DocumentError::InvalidContainer { entry }) => entry,
            other => panic!("{names:?} が拒否されない: {other:?}"),
        }
    }
}

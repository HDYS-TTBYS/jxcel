//! マクロの記録(名前・種別・ソース)の**形**(タスク 1.2。要件 1.1, 1.2, 1.5)。
//!
//! 本モジュールが持つのは `macros.json` の中身の**形だけ**であり、**意味は持たない**
//! (design 決定 4「マクロの形は `document-format`、意味は `macro-runtime`」)。
//! 分界は次のとおりである:
//!
//! | 本クレート(形) | 下流 `macro-runtime`(意味) |
//! |----------------|------------------------------|
//! | 名前・種別・ソースの 3 つ組と、その並び | ソースの検証(構文・種別との整合) |
//! | 種別の閉じた集合([`MacroKind`]) | ソース先頭の能力宣言の読み取り |
//! | 並びの順序(保存順)をそのまま保つこと | 名前の一意性と置き換え(同じ名前の保存)の規則 |
//!
//! **本クレートが検査しないこと**(すべて意味であり、下流が所有する): 名前の重複、名前や
//! ソースが空であること、種別とソースの整合、ソースの構文。上流で第二の規則を持つと、
//! 同じ違反が 2 通りの文言・2 通りの可否判定で現れる。本クレートが担うのは
//! 「名前・種別・ソースがそのまま保存され、そのまま戻る」ことだけである(要件 1.5)。
//! ソースは**整形しない**: 与えられたテキストをバイト単位でそのまま運ぶ。
//!
//! # wire 形との分界
//!
//! 本モジュールの型は wire 形の知識を持たない([`MacroKind`] にテキスト写像を持たせない)。
//! `macros.json` の符号化・復号、キー名、種別のテキストは [`crate::parts::MacrosPart`] が
//! 所有する(先例: [`crate::migration::FormatVersion`] は wire 形 `{"major":..,"minor":..}`
//! を持たず、その写しを [`crate::parts::ManifestPart`] が持つ)。
//!
//! # 未知フィールドの保持(前方互換。要件 6.2 / 6.3)
//!
//! マクロ 1 件は [`MacroRecord::preserved_fields`] に、その要素の中で解釈しないキーを
//! 原文の位置ごと保持する([`crate::model::Sheet`] が `document.json` のシート要素について
//! 持つのと同じ扱い)。書き戻しは [`crate::parts::MacrosPart`] が行う。
//!
//! # 依存方向
//!
//! `Ids / Value / EntryName → Model → Json → Parts → Container → Api` の一方向のうち、
//! 本モジュールはモデル層の最下段にあり、[`crate::json`] にのみ依存する(パート層の型を
//! 参照しない: `macros.json` の符号化は本層より上にある)。

use crate::json::PreservedFields;

/// マクロのソースの種別(design「Logical Data Model」の `MacroRecord` の 1 欄)。
///
/// **閉じた集合**である: 2 変種以外の種別は形式として表現できず、`macros.json` の復号は
/// 未知の種別テキストを(受理して既定値へ丸めず)[`crate::error::DocumentError::InvalidContainer`]
/// として拒否する(推測による修復をしない)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroKind {
    /// TypeScript(実行の前に型注釈を落とす。変換は下流の仕事である)。
    TypeScript,
    /// JavaScript(そのまま実行する)。
    JavaScript,
}

/// マクロ 1 件の記録(design「Logical Data Model」の `MacroRecord` の**形**)。
///
/// フィールドは非公開で、構築は [`MacroRecord::new`]（保存経路）と
/// [`crate::parts::MacrosPart`] の復号(読み込み経路)の 2 経路だけである。どちらも
/// 内容を解釈しない(モジュール docs「本クレートが検査しないこと」)。
///
/// `PartialEq` は提供しない: 保持している未知フィールドの比較には読み込みカーソル
/// ([`PreservedFields`] の内部状態)が混じり、内容の等値を素直に表せないためである
/// ([`crate::model::Sheet`] / [`crate::parts::DocumentPart`] と同じ方針)。1 件の等価は
/// [`MacroRecord::name`] / [`MacroRecord::kind`] / [`MacroRecord::source`] を見るか、
/// `macros.json` の符号化バイト列を比べて判定する。
#[derive(Debug, Clone)]
pub struct MacroRecord {
    /// 名前(マクロの識別子。下流が一意性を保証する)。
    name: String,
    /// ソースの種別。
    kind: MacroKind,
    /// ソースそのもの(整形しない。バイト単位で往復する)。
    source: String,
    /// 解釈しない要素内のフィールド(前方互換。要件 6.2 / 6.3)。
    preserved: PreservedFields,
}

impl MacroRecord {
    /// 名前・種別・ソースから 1 件を組み立てる(保存経路)。
    ///
    /// **解釈も検証もしない**(モジュール docs「本クレートが検査しないこと」)。名前とソースは
    /// 正規化もサニタイズもせず、与えられた文字列をそのまま保持する(要件 1.5 の「ソースは
    /// テキストのまま保持する」)。
    #[inline]
    pub fn new(name: impl Into<String>, kind: MacroKind, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind,
            source: source.into(),
            preserved: PreservedFields::new(),
        }
    }

    /// 名前(原文のまま。下流が一意性と置き換えの規則を適用する)。
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// ソースの種別。
    #[inline]
    pub const fn kind(&self) -> MacroKind {
        self.kind
    }

    /// ソース(原文のまま。バイト単位で保存・復元される。要件 1.5)。
    #[inline]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// 解釈しない要素内のキー(読み込み経路が保持する。書き戻しは
    /// [`crate::parts::MacrosPart`] が行う)。
    #[inline]
    pub(crate) fn preserved_fields(&self) -> &PreservedFields {
        &self.preserved
    }

    /// 保持すべき未知フィールドを据える経路([`crate::parts::MacrosPart`] の復号が呼ぶ)。
    #[inline]
    pub(crate) fn with_preserved(mut self, preserved: PreservedFields) -> Self {
        self.preserved = preserved;
        self
    }
}

//! 構造検証（`StructuralValidator`。タスク 4.6 / 4.7。要件 1.4, 1.7, 4.2, 4.3, 4.4, 7.4。
//! design「Components and Interfaces」の StructuralValidator）。
//!
//! 復号済みパート群から取り出した**目録**（[`PartInventory`]）を受け取り、ドキュメント
//! 全体にわたる構造不変条件を検証する。持つ検証は次の 5 つである（前 2 つがタスク 4.6、
//! 残り 3 つがタスク 4.7 の追加）:
//!
//! | 検証 | 不変条件 | 違反時の変種 |
//! |------|----------|--------------|
//! | 識別子の一意性 | シート / 行 / 型定義 / 添付の識別子は**種別ごとに**ドキュメント内で一意（要件 1.4, 4.3。design「Domain Model 不変条件」） | [`DocumentError::DuplicateId`] |
//! | スキーマの存在 | 文書の全シートがルートスキーマをちょうど 1 つ持つ（要件 4.4。design 同不変条件） | [`DocumentError::MissingSchema`] |
//! | 型定義参照の実在性 | すべての型定義参照（`$ref` 構造）が**同一シート**の型定義集合に実在する（要件 1.7, 4.2。design 不変条件「すべての `TypeDefId` 参照は同一シートの型定義集合に実在する」） | [`DocumentError::DanglingTypeRef`] |
//! | 添付参照の実在性 | セル値の [`CellValue::Attachment`] が添付レジストリに実在する（要件 7.4, 4.2。design「参照整合性」） | [`DocumentError::DanglingAttachmentRef`] |
//! | シート参照の実在性 | パートのエントリ名が指すシートが `document.json` のシート集合に実在する（要件 4.2。**親の裁定**は下記） | [`DocumentError::InvalidContainer`]（`entry` にエントリ名と理由） |
//!
//! # 入力の形（タスク 4.8 が流し込む確定形）
//!
//! 検証器の入力は `DocumentParts`（タスク 4.8）ではなく**目録** [`PartInventory`] である。
//! 目録は復号済みパート群から取り出した識別子の宣言・参照の出現・スキーマの要求/提供の
//! コレクションから成る。呼び出し元は `DocumentParts` のエントリを復号しながら次のように
//! 写す（4.8 の `from_parts` がこの形で流し込む）:
//!
//! ```text
//! document.json       →  各シートについて declare(Sheet, sheet_id, "document.json sheets[i]")
//!                         かつ require_schema(sheet_id)（全シート。行データの有無を問わない）
//! schemas/<ulid>.json →  provide_schema(<ulid>) と、
//!                         各型定義について declare(TypeDef, type_id, "schemas/<ulid>.json types[i]")
//!                             .with_sheet(<ulid>)（所属シートは同一シート規則が使う）
//!                         かつ SchemaPart::type_refs() の各参照について
//!                             declare_type_ref("schemas/<ulid>.json <位置ラベル>", to, <ulid>)
//! sheets/<ulid>.jsonl →  各行について declare(Row, row_id, "sheets/<ulid>.jsonl line n")
//!                         かつ各行のセルについて
//!                             declare_attachment_refs("sheets/<ulid>.jsonl line n", セル値)
//!                         かつ declare_sheet_ref("sheets/<ulid>.jsonl", <ulid>)
//! attachments/<hex>.bin → declare(Attachment, attachment_id, "attachments/<hex>.bin")
//! ```
//!
//! | 宣言の種別 | [`IdKind`] | 取り出し元（タスク） | 出現箇所テキストの慣例 |
//! |------------|------------|----------------------|------------------------|
//! | シート識別子 | [`IdKind::Sheet`] | `document.json` の `sheets` 配列（4.3） | `document.json sheets[<0 始まりの添字>]` |
//! | 行識別子 | [`IdKind::Row`] | `sheets/<ulid>.jsonl` の各行（4.5） | `sheets/<ulid>.jsonl line <1 始まりの行番号>` |
//! | 型定義識別子 | [`IdKind::TypeDef`] | `schemas/<ulid>.json` の `types` 配列（4.4） | `schemas/<ulid>.json types[<0 始まりの添字>]` |
//! | 添付識別子 | [`IdKind::Attachment`] | `attachments/<hex64>.bin` のエントリ名（4.8） | `attachments/<hex64>.bin` |
//!
//! 出現箇所・参照元テキストの綴りは本モジュールにとって**不透明**である（診断用の文字列
//! としてそのまま [`DocumentError`] の文脈へ運ぶだけで、解析も再構成もしない）。上の慣例は
//! 呼び出し元が綴りを発明しなくて済むための推奨であり、強制ではない。
//!
//! ## 目録に入れるのは「宣言」と「参照」を別々に
//!
//! シート識別子は `document.json` の `sheets` 配列の要素が**宣言**であり、
//! `schemas/<ulid>.json` / `sheets/<ulid>.jsonl` のエントリ名に現れる同じテキストは
//! **参照**である。同じく添付識別子はエントリ名 `attachments/<hex64>.bin` 自体が宣言である。
//! 目録へ参照を宣言として入れてはならない: エントリ名を宣言として数えると、妥当な文書が
//! 常に重複として報告されてしまう。参照は参照のコレクション
//! （[`PartInventory::type_refs`] / [`PartInventory::attachment_refs`] /
//! [`PartInventory::sheet_refs`]）へ入れる。
//!
//! ## 識別子は正準テキスト形で与えられる
//!
//! 目録の識別子は [`String`] のテキスト形で受ける（[`DocumentError::DuplicateId`] の
//! `id` が診断用のテキスト形であるため。`error.rs` の「文脈フィールドの型について」）。
//! 復号層（[`crate::parts::document_part`] / [`crate::parts::schema_codec`] /
//! [`crate::parts::rows_codec`]）が正準形の識別子しか受理しないため、目録のテキストは
//! 正準形である。**本モジュールは正規化を一切しない**（第二の正規化規則を作らない）。
//! 表記が揺れた目録は別の識別子として扱われるが、それは呼び出し元の責務である。
//!
//! ## スキーマの要求は「行データを持つシート」に限らない（親の裁定）
//!
//! 要件 1.2 と design 不変条件「各シートはちょうど 1 つのルートスキーマを持つ」により、
//! **行データを持たないシートも**ルートスキーマを要求する（[`PartInventory::require_schema`]
//! は `document.json` に列挙された全シートについて呼ぶ）。要件 4.4 の文言（シートデータに
//! 対応するスキーマ）より強い側だが、空のシートでもスキーマ無しでは列を持てず、要件 1.2 の
//! 「0 個以上のシートを保持できる構造」がスキーマを欠いたシートを許す形にならないための裁定で
//! ある。
//!
//! 「ちょうど 1 つ」のうち**2 つ以上**が存在する状況は、コンテナ層（要件 2.6: 同一パスの
//! 重複エントリの拒否）と `manifest.json` の索引（エントリ名の重複の拒否）が既に閉じている。
//! 本検証は与えられた目録について「要求されたシートにスキーマが存在するか」だけを見る。
//!
//! ## シート参照の破れは `InvalidContainer` で報告する（タスク 4.7 の親の裁定）
//!
//! 要件 4.2 は「シート、行、ネスト型定義、添付のすべての識別子参照が実在する対象を指して
//! いること」を要求する。したがって `document.json` に列挙されていないシートを指すパート
//! （`sheets/<unknown>.jsonl` / `schemas/<unknown>.json`）も参照破れである。design のエラー
//! 表にはこのケース専用の変種が無いため、**新しい変種を [`DocumentError`] に足さない**。
//! 型付き変種の無い構造的矛盾は [`DocumentError::InvalidContainer`] の `entry` に「失敗箇所の
//! ラベル + 理由」を載せるという既存規約（[`crate::parts::manifest`] /
//! [`crate::parts::rows_codec`] と同じ）に従い、`entry` にエントリ名と「そのシートが
//! `document.json` に無い」という理由を載せる。変種を増やさないのは、design のエラー表が
//! 閉じた 10 変種であり、参照先の種別ごとに変種を増やすと読み込み経路の分岐が際限なく
//! 増えるためである。
//!
//! ## 参照の目録は「出現」をそのまま並べる
//!
//! 参照の宣言は**出現 1 件につき 1 件**であり、同じ参照先を何度参照してもその数だけ並ぶ
//! （重複排除はしない）。実在判定に使う集合（同一シートの型定義 / 添付 / シート）は本モジュールが
//! 組み立て、**所属判定にしか使わない**（反復しない。報告順は参照の目録順である）。
//!
//! ## 識別子の解決規約（タスク 4.7。親の裁定）
//!
//! 型定義識別子の比較は、宣言側（[`IdDeclaration`] の型定義 id）と参照先
//! （[`TypeRefDeclaration::to`]）の**双方**を [`TypeDefId`](crate::ids::TypeDefId) の
//! テキスト解決（大小文字を問わない ULID。`ids` の規約）で**正準形（大文字 26 文字）へ
//! 写してから**行う。`$ref` は生テキストであり、この形式は小文字表記の ULID を受理する
//! （[`crate::parts::schema_codec`] も小文字表記の `id` を受理し、再符号化で正準形に揃える）
//! ため、生テキスト比較のままでは**受理する文書を誤って宙吊りと報告する**。識別子はどの
//! 経路でも同じ規約で解決する（エントリ名の完全一致規則はファイル名に固有の別規約である）。
//!
//! **正規化であってサニタイズではない**: [`TypeDefId`](crate::ids::TypeDefId) として解決
//! できないテキストは原文のまま扱い、「実在しない参照先」として報告する（新しい失敗種別を
//! 足さない）。**報告に載る値は原文**である: [`DocumentError::DanglingTypeRef`] の `to` には
//! `$ref` に書かれていたテキストをそのまま運ぶ（要件 1.7 の診断は原文が最も忠実）。
//!
//! 添付識別子は正規化しない: [`CellValue::Attachment`] の参照は [`crate::value`] の判定規約
//! （64 文字**小文字** hex のみを添付と判定。大文字 hex は `Text`）により常に正準形であり、
//! 大小文字の揺れが存在しない（要件 7.3）。シート識別子の結合鍵（[`TypeRefDeclaration::sheet`]）
//! も正準形で与えられる前提であり、正規化しない。
//!
//! # 報告順（決定性。要件 3.6）
//!
//! 違反が複数ある目録でも常に同じエラーが返るよう、報告順を次のように定める。最初に見つかった
//! 1 件だけを返す（design のエラー戦略: 読み込みの失敗は中止であり、部分的な結果を返さない）。
//!
//! 1. 識別子の一意性（要件 4.3）: 種別 → 識別子テキストの昇順。種別の順序は [`IdKind`] の
//!    宣言順（`Sheet` → `Row` → `TypeDef` → `Attachment`）であり、**`error.rs` が唯一の源**
//!    である。識別子は正準テキスト形（26 文字固定幅）なので、テキスト昇順は ULID の数値昇順
//!    と一致する（[`crate::ids`] の新型の `Ord`）。
//! 2. スキーマの存在（要件 4.4）: [`PartInventory::required_sheets`] の順、すなわち
//!    `document.json` のシート順（配列順そのものがデータ = 要件 1.1。ソートした順ではない）。
//! 3. 参照の実在性（要件 1.7 / 4.2 / 7.4）: 参照の種別ごとに **型定義参照 → 添付参照 →
//!    シート参照** の順（design エラー表の変種順 1.7 → 7.4 に一致させ、裁定で加わったシート
//!    参照を最後に置く）。種別内は**目録に加えた順**（[`Vec`]）であり、ソートしない
//!    （参照は文書中の出現そのものであり、出現箇所を目録の順で報告する 1 の流儀と同じ）。
//!
//! 一意性とスキーマ存在を先に見るのは、識別子の集合が決まらなければ参照先の実在を一意に
//! 判定できないためである。
//!
//! この順序は `BTreeMap` / `BTreeSet` / `Vec` が構造として保証する。本モジュールは
//! `HashMap` / `HashSet` を使わない（反復順が実行ごとに変わり決定性を壊す。
//! `crate::value` / `crate::json` と同じ禁止理由）。
//!
//! # 純粋関数であること
//!
//! - I/O を一切行わない。入力は借用で受け、目録を変更しない（`&PartInventory` のみを取る）。
//! - モデルに依存しない: 検証は**モデル構築の前**に完了する（design 読み込みフロー、
//!   要件 5.4）。したがって `crate::model` を参照せず、識別子をコードから復号しない。参照は
//!   呼び出し元が**テキスト**（[`TypeRefDeclaration`] / [`AttachmentRefDeclaration`] /
//!   [`SheetRefDeclaration`]）へ写して渡す（スキーマ側の参照元つき抽出は
//!   [`crate::model::SchemaPart::type_refs`]、セル値の走査は
//!   [`PartInventory::declare_attachment_refs`] が担う）。
//! - panic しない: 空の目録・空テキストの宣言・空テキストの参照を含め、どんな目録でも
//!   [`Result`] を返す（添字参照も `unwrap` も持たない）。
//! - `serde_json` の汎用値型・マップ型を経由しない（親モジュール [`crate::parts`] の規則）。
//!   セル値を走査するのは [`crate::value`] の [`CellValue`] /
//!   [`NestedValue`](crate::value::NestedValue) であり、モデル型（`Document` / `Sheet` /
//!   `Row`）は参照しない。
//!
//! # 対象外
//!
//! - 「実在するが未参照の添付」の一覧と削除（要件 7.6）: 本検証は「参照 → 実在」の向きだけを
//!   見て、添付の削除も警告もしない
//!   （[`crate::model::AttachmentRegistry::unreferenced_attachments`] の担当）。
//! - `document.json` に無いシートのスキーマ（孤立パート）の**列挙**: コンテナ層の許可リスト
//!   （要件 2.5）と `manifest.json` の索引が扱う。本検証が見るのは、呼び出し元が
//!   [`SheetRefDeclaration`] として渡したパートのエントリ名が指すシートだけである
//!   （[`PartInventory::provide_schema`] の集合は存在判定にしか使わない）。
//! - 目録の網羅性: 全パートを目録へ写したこと、`document.json` が存在することは呼び出し元
//!   （4.8）の責務である。本検証は与えられた目録の内部整合だけを見る。
//!
//! # エラー対応
//!
//! | 違反 | 返す変種と文脈 |
//! |------|----------------|
//! | 同一種別・同一テキストの識別子が 2 回以上宣言されている | [`DocumentError::DuplicateId`]（`kind` = 種別、`id` = 識別子、`occurrences` = **重複した全出現箇所**を目録の順で） |
//! | 要求されたシートにスキーマが無い | [`DocumentError::MissingSchema`]（`sheet` = そのシートの識別子） |
//! | 参照先のテキストが**そのシート**の型定義集合に無い | [`DocumentError::DanglingTypeRef`]（`from` = 参照元、`to` = 参照先の生テキスト。ULID でないテキストもそのまま載る） |
//! | 引用された添付識別子が宣言されていない | [`DocumentError::DanglingAttachmentRef`]（`from` = 参照元、`id` = 参照された識別子） |
//! | パートのエントリ名が `document.json` に無いシートを指す | [`DocumentError::InvalidContainer`]（`entry` = エントリ名 + 理由） |
//!
//! # 例
//!
//! 同じ行識別子が 2 箇所に現れる目録は、種別つきの重複として報告される（タスク 4.6）:
//!
//! ```
//! use document_format::parts::{IdDeclaration, PartInventory, StructuralValidator};
//! use document_format::{DocumentError, IdKind};
//!
//! let mut inventory = PartInventory::new();
//! inventory.declare(IdDeclaration::new(
//!     IdKind::Row,
//!     "01ARZ3NDEKTSV4RRFFQ69G5FAV",
//!     "sheets/A.jsonl line 1",
//! ));
//! inventory.declare(IdDeclaration::new(
//!     IdKind::Row,
//!     "01ARZ3NDEKTSV4RRFFQ69G5FAV",
//!     "sheets/B.jsonl line 4",
//! ));
//!
//! let error = StructuralValidator::validate(&inventory).expect_err("同一の行識別子が 2 箇所");
//! assert!(matches!(
//!     error,
//!     DocumentError::DuplicateId { kind: IdKind::Row, ref id, ref occurrences }
//!         if id == "01ARZ3NDEKTSV4RRFFQ69G5FAV" && occurrences.len() == 2
//! ));
//! ```
//!
//! 実在しない型定義を指す参照も同じ入口で報告される（タスク 4.7）:
//!
//! ```
//! use document_format::parts::{PartInventory, StructuralValidator, TypeRefDeclaration};
//! use document_format::DocumentError;
//!
//! let mut inventory = PartInventory::new();
//! inventory.declare_type_ref(TypeRefDeclaration::new(
//!     "schemas/A.json root",
//!     "01ARZ3NDEKTSV4RRFFQ69G5FAV",
//!     "01ARZ3NDEKTSV4RRFFQ69G5FAV",
//! ));
//!
//! let error = StructuralValidator::validate(&inventory).expect_err("型定義が実在しない");
//! assert!(matches!(
//!     error,
//!     DocumentError::DanglingTypeRef { ref from, ref to }
//!         if from == "schemas/A.json root" && to == "01ARZ3NDEKTSV4RRFFQ69G5FAV"
//! ));
//! ```

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{DocumentError, IdKind};
use crate::ids::TypeDefId;
use crate::value::{visit_attachment_references, CellValue};

/// 識別子 1 個の宣言（[`PartInventory`] の要素）。
///
/// 種別を文字列ではなく [`IdKind`] で持つのは、一意性を種別ごとに判定するためである
/// （種別を無視すると、別体系の識別子が同じテキストを持つだけで重複と誤判定される）。
/// 出現箇所は診断用のテキストであり、本モジュールはその綴りを解釈しない。
///
/// `sheet` は所属シートであり、型定義参照の**同一シート規則**（要件 1.7）が使う
/// （タスク 4.7 が additive に追加した。既存の意味・報告順は変えていない）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdDeclaration {
    /// 識別子の種別（`Sheet` / `Row` / `TypeDef` / `Attachment`）。
    pub kind: IdKind,
    /// 識別子のテキスト形（正準形。モジュール docs「識別子は正準テキスト形で与えられる」）。
    pub id: String,
    /// この宣言が現れた箇所を示すテキスト（`sheets/<ulid>.jsonl line 7` など）。
    pub location: String,
    /// この宣言が属するシートの識別子（所属が意味を持つ種別のみ `Some`）。
    pub sheet: Option<String>,
}

impl IdDeclaration {
    /// 種別・識別子・出現箇所から 1 件を組み立てる（所属シートは未設定）。
    pub fn new(kind: IdKind, id: impl Into<String>, location: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            location: location.into(),
            sheet: None,
        }
    }

    /// 所属シートを与える（ビルダー。[`IdDeclaration::new`] の意味は変えない）。
    ///
    /// **型定義の宣言は所属シートを持つこと**: 参照の実在判定は「参照元のシートの型定義
    /// 集合」を見るため、所属の無い型定義宣言（`None`）はどのシートの参照も満たさない。
    pub fn with_sheet(mut self, sheet: impl Into<String>) -> Self {
        self.sheet = Some(sheet.into());
        self
    }
}

/// 型定義参照 1 件（要件 1.7, 4.2。タスク 4.7）。
///
/// 参照元（`from`）・参照先（`to`）・参照が現れたシート（`sheet`）を持つ。`to` は `$ref` が
/// 持っていた**生テキスト**であり、ULID とは限らない（本クレートは参照構造だけを見て、
/// 識別子の意味論を解釈しない。design「スキーマペイロードの不透明性」）。したがって ULID で
/// ない `to` も単に「実在しない参照先」として報告される（パース失敗にはしない）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRefDeclaration {
    /// 参照元を示す診断用テキスト（例 `schemas/<ulid>.json root`）。
    pub from: String,
    /// 参照先の生テキスト（`$ref` の値）。
    pub to: String,
    /// この参照が現れたシートの識別子（同一シート規則の判定に使う）。
    pub sheet: String,
}

impl TypeRefDeclaration {
    /// 参照元・参照先・所属シートから 1 件を組み立てる。
    pub fn new(from: impl Into<String>, to: impl Into<String>, sheet: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            sheet: sheet.into(),
        }
    }
}

/// 添付参照 1 件（要件 7.4, 4.2。タスク 4.7）。
///
/// セル値の [`CellValue::Attachment`] 1 個に対応する。`id` は添付識別子のテキスト形であり、
/// 添付の宣言（`IdKind::Attachment`）と同じ正準形である（[`PartInventory::declare_attachment_refs`]
/// が [`AttachmentId`](crate::ids::AttachmentId) から作る）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentRefDeclaration {
    /// 参照元を示す診断用テキスト（例 `sheets/<ulid>.jsonl line 7`）。
    pub from: String,
    /// 実在を要求する添付識別子のテキスト形。
    pub id: String,
}

impl AttachmentRefDeclaration {
    /// 参照元と添付識別子から 1 件を組み立てる。
    pub fn new(from: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            id: id.into(),
        }
    }
}

/// シート参照 1 件（パートのエントリ名が指すシート。要件 4.2。タスク 4.7 の親の裁定）。
///
/// エントリ名（`sheets/<ulid>.jsonl` / `schemas/<ulid>.json`）はシート識別子を**参照**して
/// いる。`entry` はそのエントリ名そのもの（診断に載せる）、`sheet` はエントリ名が指すシート
/// 識別子のテキストである。本モジュールはエントリ名を解析しない（綴りは呼び出し元が復号層
/// [`crate::entry_name`] から受け取る）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetRefDeclaration {
    /// シートを参照しているエントリ名。
    pub entry: String,
    /// そのエントリ名が指すシートの識別子のテキスト形。
    pub sheet: String,
}

impl SheetRefDeclaration {
    /// エントリ名と、それが指すシート識別子から 1 件を組み立てる。
    pub fn new(entry: impl Into<String>, sheet: impl Into<String>) -> Self {
        Self {
            entry: entry.into(),
            sheet: sheet.into(),
        }
    }
}

/// 復号済みパート群から取り出した識別子と参照の目録（[`StructuralValidator`] の入力）。
///
/// 6 つのコレクションから成る（モジュール docs「入力の形」）:
///
/// | コレクション | 内容 | 順序 |
/// |--------------|------|------|
/// | [`PartInventory::declarations`] | 識別子の宣言 | 与えた順が出現箇所の順序 |
/// | [`PartInventory::required_sheets`] | ルートスキーマを要求するシート | 与えた順が文書順（`document.json` の配列順） |
/// | [`PartInventory::schema_sheets`] | スキーマパートを持つシート | 順序に意味は無い（存在判定のみ） |
/// | [`PartInventory::type_refs`] | 型定義参照（要件 1.7） | 与えた順が報告順 |
/// | [`PartInventory::attachment_refs`] | 添付参照（要件 7.4） | 与えた順が報告順 |
/// | [`PartInventory::sheet_refs`] | シート参照（要件 4.2） | 与えた順が報告順 |
///
/// `Clone` / `PartialEq` を持つのは、検証が目録を変更しないことをテストで直接確かめられる
/// ようにするためである（`PreservedFields` のような内部カーソルを持たない素のデータなので、
/// `PartialEq` の導出に落とし穴は無い）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartInventory {
    declarations: Vec<IdDeclaration>,
    required_sheets: Vec<String>,
    schema_sheets: BTreeSet<String>,
    type_refs: Vec<TypeRefDeclaration>,
    attachment_refs: Vec<AttachmentRefDeclaration>,
    sheet_refs: Vec<SheetRefDeclaration>,
}

impl PartInventory {
    /// 空の目録を組み立てる。
    pub fn new() -> Self {
        Self::default()
    }

    /// 識別子の宣言を 1 件加える。
    ///
    /// 同じ識別子の宣言が複数回現れれば、それがそのまま重複の出現箇所になる。
    pub fn declare(&mut self, declaration: IdDeclaration) {
        self.declarations.push(declaration);
    }

    /// ルートスキーマを要求するシートを 1 枚加える（`document.json` に列挙された全シート）。
    ///
    /// 与えた順が文書順であり、スキーマ欠落の報告順になる（モジュール docs「報告順」）。
    /// シート集合は参照の実在検証（要件 4.2）でも「実在するシート」の集合として使う。
    pub fn require_schema(&mut self, sheet: impl Into<String>) {
        self.required_sheets.push(sheet.into());
    }

    /// スキーマパートを持つシートを 1 枚記録する（`schemas/<ulid>.json` のエントリ）。
    ///
    /// 同じシートを 2 度記録しても 1 つとして扱う。2 つ以上のスキーマが併存する状況は
    /// コンテナ層（要件 2.6）と `manifest.json` の索引が既に拒否している。
    pub fn provide_schema(&mut self, sheet: impl Into<String>) {
        self.schema_sheets.insert(sheet.into());
    }

    /// 型定義参照を 1 件加える（要件 1.7）。
    pub fn declare_type_ref(&mut self, declaration: TypeRefDeclaration) {
        self.type_refs.push(declaration);
    }

    /// 添付参照を 1 件加える（要件 7.4）。
    pub fn declare_attachment_ref(&mut self, declaration: AttachmentRefDeclaration) {
        self.attachment_refs.push(declaration);
    }

    /// セル値 1 つから添付参照を再帰的に集め、`from` を参照元として目録に加える（要件 7.4）。
    ///
    /// 走査規則（たどるのは `Nested` の内側だけ、参照として数えるのは
    /// [`CellValue::Attachment`] だけ）の実装は [`visit_attachment_references`] が唯一の
    /// 場所である（[`crate::model::AttachmentRegistry::unreferenced_attachments`] と同じ規則。
    /// あちらは集合へ、こちらは出現列へ集める）。深さに制限は無く、同じセルの中の複数の参照は
    /// 出現順にそのまま並ぶ（重複も排除しない）。`from` はセルを指すテキスト（例
    /// `sheets/<ulid>.jsonl line 7`）であり、ネストの位置までは区別しない（要件 7.4 が要求
    /// するのは参照元と識別子である）。セル値の鍵（文字列）や `Text` / `Decimal` の内容は参照
    /// として数えない（要件 7.3）。
    pub fn declare_attachment_refs(&mut self, from: impl Into<String>, value: &CellValue) {
        let from = from.into();
        let references = &mut self.attachment_refs;
        visit_attachment_references(value, &mut |id| {
            references.push(AttachmentRefDeclaration {
                from: from.clone(),
                id: id.to_string(),
            });
        });
    }

    /// シート参照（パートのエントリ名が指すシート）を 1 件加える（要件 4.2）。
    pub fn declare_sheet_ref(&mut self, declaration: SheetRefDeclaration) {
        self.sheet_refs.push(declaration);
    }

    /// 宣言された識別子（与えた順）。
    pub fn declarations(&self) -> &[IdDeclaration] {
        &self.declarations
    }

    /// ルートスキーマを要求するシート（与えた順 = 文書順）。
    pub fn required_sheets(&self) -> &[String] {
        &self.required_sheets
    }

    /// スキーマパートを持つシート（識別子テキストの昇順。順序は報告に使わない）。
    pub fn schema_sheets(&self) -> &BTreeSet<String> {
        &self.schema_sheets
    }

    /// 型定義参照（与えた順 = 報告順）。
    pub fn type_refs(&self) -> &[TypeRefDeclaration] {
        &self.type_refs
    }

    /// 添付参照（与えた順 = 報告順）。
    pub fn attachment_refs(&self) -> &[AttachmentRefDeclaration] {
        &self.attachment_refs
    }

    /// シート参照（与えた順 = 報告順）。
    pub fn sheet_refs(&self) -> &[SheetRefDeclaration] {
        &self.sheet_refs
    }
}

/// 目録に対する構造検証（design「Components and Interfaces」の StructuralValidator。
/// Service 契約）。
///
/// 状態を持たない（フィールドの無い型で、関連関数だけを持つ。[`crate::parts::SchemaCodec`]
/// と同じ形）。関連関数は目録を借用するだけで、I/O もモデルへの依存も持たない
/// （モジュール docs「純粋関数であること」）。
#[derive(Debug, Clone, Copy)]
pub struct StructuralValidator;

impl StructuralValidator {
    /// 識別子の一意性（要件 4.3）、スキーマの存在（要件 4.4）、参照の実在性
    /// （要件 1.7 / 4.2 / 7.4）をこの順に検証する。
    ///
    /// 違反があれば、モジュール docs「報告順」で定めた順序で**最初の 1 件**を返す。
    pub fn validate(inventory: &PartInventory) -> Result<(), DocumentError> {
        Self::validate_unique_ids(inventory.declarations())?;
        Self::validate_schema_presence(inventory.required_sheets(), inventory.schema_sheets())?;
        Self::validate_references(inventory)
    }

    /// 識別子が種別ごとに一意であること（要件 1.4, 4.3）を検証する。
    ///
    /// 種別と識別子テキストの組を鍵にした `BTreeMap` で数えるため、反復順がそのまま
    /// 報告順（種別 → テキスト昇順）になる。出現箇所は目録に現れた順に集める。
    fn validate_unique_ids(declarations: &[IdDeclaration]) -> Result<(), DocumentError> {
        let mut occurrences_by_identity: BTreeMap<(IdKind, &str), Vec<&str>> = BTreeMap::new();
        for declaration in declarations {
            occurrences_by_identity
                .entry((declaration.kind, declaration.id.as_str()))
                .or_default()
                .push(declaration.location.as_str());
        }

        for ((kind, id), locations) in &occurrences_by_identity {
            if locations.len() > 1 {
                return Err(DocumentError::DuplicateId {
                    kind: *kind,
                    id: (*id).to_owned(),
                    occurrences: locations
                        .iter()
                        .map(|location| (*location).to_owned())
                        .collect(),
                });
            }
        }
        Ok(())
    }

    /// 要求された全シートにスキーマが存在すること（要件 4.4）を検証する。
    ///
    /// 走査順は要求順（= 文書順）であり、ソートしない。
    fn validate_schema_presence(
        required_sheets: &[String],
        schema_sheets: &BTreeSet<String>,
    ) -> Result<(), DocumentError> {
        for sheet in required_sheets {
            if !schema_sheets.contains(sheet) {
                return Err(DocumentError::MissingSchema {
                    sheet: sheet.clone(),
                });
            }
        }
        Ok(())
    }

    /// 参照先が実在すること（要件 1.7 / 4.2 / 7.4）を検証する。
    ///
    /// 判定に使う集合（同一シートの型定義 / 添付 / シート）は `BTreeSet` で組み立てる。
    /// 所属判定にしか使わず**反復しない**ため、報告順（参照の目録順）に影響しない。
    fn validate_references(inventory: &PartInventory) -> Result<(), DocumentError> {
        // 判定に使う集合は宣言の 1 回の走査で作る（種別ごとに走査し直さない）。
        let mut defined_type_defs: BTreeSet<(&str, String)> = BTreeSet::new();
        let mut known_attachments: BTreeSet<&str> = BTreeSet::new();
        for declaration in inventory.declarations() {
            match declaration.kind {
                IdKind::TypeDef => {
                    // 所属シートを持たない型定義宣言はどのシートの参照も満たさない
                    // （`IdDeclaration::with_sheet` の docs）。
                    if let Some(sheet) = declaration.sheet.as_deref() {
                        // 識別子は正準形で比較する（モジュール docs「識別子の解決規約」）。
                        defined_type_defs.insert((sheet, canonical_type_def_id(&declaration.id)));
                    }
                }
                IdKind::Attachment => {
                    known_attachments.insert(declaration.id.as_str());
                }
                _ => {}
            }
        }

        // 1. 型定義参照（要件 1.7）。「同一シートの型定義集合」に実在すること。参照先の
        //    生テキストは識別子として解決してから比較し、報告には原文を載せる。
        for reference in inventory.type_refs() {
            let to = canonical_type_def_id(&reference.to);
            if !defined_type_defs.contains(&(reference.sheet.as_str(), to)) {
                return Err(DocumentError::DanglingTypeRef {
                    from: reference.from.clone(),
                    to: reference.to.clone(),
                });
            }
        }

        // 2. 添付参照（要件 7.4）。宣言された添付に実在すること。
        for reference in inventory.attachment_refs() {
            if !known_attachments.contains(reference.id.as_str()) {
                return Err(DocumentError::DanglingAttachmentRef {
                    from: reference.from.clone(),
                    id: reference.id.clone(),
                });
            }
        }

        // 3. シート参照（要件 4.2。親の裁定）。`document.json` に列挙されたシートに実在する
        //    こと。専用の変種が design に無いため `InvalidContainer` の `entry` に載せる。
        let known_sheets: BTreeSet<&str> = inventory
            .required_sheets()
            .iter()
            .map(String::as_str)
            .collect();
        for reference in inventory.sheet_refs() {
            if !known_sheets.contains(reference.sheet.as_str()) {
                return Err(DocumentError::InvalidContainer {
                    entry: format!(
                        "{}: sheet `{}` is not listed in document.json",
                        reference.entry, reference.sheet
                    ),
                });
            }
        }
        Ok(())
    }
}

/// 型定義識別子のテキストを**正準形**（大文字 26 文字）へ写す（型定義参照の実在判定。
/// モジュール docs「識別子の解決規約」）。
///
/// [`TypeDefId`] として解決できたときだけ `Display` の正準形へ写し、解決できないテキストは
/// **原文のまま**返す（正規化であってサニタイズではない。原文のまま比較すれば「実在しない
/// 参照先」として報告され、パース失敗を新しい失敗種別にしない）。
fn canonical_type_def_id(text: &str) -> String {
    match text.parse::<TypeDefId>() {
        Ok(id) => id.to_string(),
        Err(_) => text.to_owned(),
    }
}

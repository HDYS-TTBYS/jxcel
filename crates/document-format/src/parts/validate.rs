//! 構造検証（`StructuralValidator`。タスク 4.6。要件 1.4, 4.3, 4.4。design「Components and
//! Interfaces」の StructuralValidator）。
//!
//! 復号済みパート群から取り出した**目録**（[`PartInventory`]）を受け取り、ドキュメント
//! 全体にわたる構造不変条件を検証する。タスク 4.6 が持つのは次の 2 つである:
//!
//! | 検証 | 不変条件 | 違反時の変種 |
//! |------|----------|--------------|
//! | 識別子の一意性 | シート / 行 / 型定義 / 添付の識別子は**種別ごとに**ドキュメント内で一意（要件 1.4, 4.3。design「Domain Model 不変条件」） | [`DocumentError::DuplicateId`] |
//! | スキーマの存在 | 文書の全シートがルートスキーマをちょうど 1 つ持つ（要件 4.4。design 同不変条件） | [`DocumentError::MissingSchema`] |
//!
//! 参照の実在性（要件 1.7 / 4.2 / 7.4）は同じ design コンポーネントの残りの責務であり、
//! **同じファイルへタスク 4.7 が追加する**（合成点は [`StructuralValidator::validate`]）。
//! 本モジュールは 4.6 の 2 検証だけを持ち、参照の検証を先取りしない。
//!
//! # 入力の形（タスク 4.8 が流し込む確定形）
//!
//! 検証器の入力は `DocumentParts`（タスク 4.8）ではなく**目録** [`PartInventory`] である。
//! 目録は復号済みパート群から取り出した識別子の宣言と、スキーマの要求/提供の 3 つの
//! コレクションから成る。呼び出し元は `DocumentParts` のエントリを復号しながら次のように
//! 写す（4.8 の `from_parts` がこの形で流し込む）:
//!
//! ```text
//! document.json       →  各シートについて declare(Sheet, sheet_id, "document.json sheets[i]")
//!                         かつ require_schema(sheet_id)（全シート。行データの有無を問わない）
//! schemas/<ulid>.json →  provide_schema(<ulid>) と、
//!                         各型定義について declare(TypeDef, type_id, "schemas/<ulid>.json types[i]")
//! sheets/<ulid>.jsonl →  各行について declare(Row, row_id, "sheets/<ulid>.jsonl line n")
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
//! 出現箇所テキストの綴りは本モジュールにとって**不透明**である（診断用の文字列として
//! そのまま [`DocumentError::DuplicateId`] の `occurrences` へ運ぶだけで、解析も再構成も
//! しない）。上の慣例は呼び出し元が綴りを発明しなくて済むための推奨であり、強制ではない。
//!
//! ## 目録に入れるのは「宣言」だけである
//!
//! シート識別子は `document.json` の `sheets` 配列の要素が**宣言**であり、
//! `schemas/<ulid>.json` / `sheets/<ulid>.jsonl` のエントリ名に現れる同じテキストは
//! **参照**である。同じく添付識別子はエントリ名 `attachments/<hex64>.bin` 自体が宣言である。
//! 目録へ参照を入れてはならない: エントリ名を宣言として数えると、妥当な文書が常に重複として
//! 報告されてしまう。参照先が実在するかの検証はタスク 4.7 の担当である（要件 4.2, 7.4）。
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
//! # 報告順（決定性。要件 3.6）
//!
//! 違反が複数ある目録でも常に同じエラーが返るよう、報告順を次のように定める。最初に見つかった
//! 1 件だけを返す（design のエラー戦略: 読み込みの失敗は中止であり、部分的な結果を返さない）。
//!
//! 1. 識別子の一意性（要件 4.3）: 種別 → 識別子テキストの昇順。種別の順序は [`IdKind`] の
//!    宣言順（`Sheet` → `Row` → `TypeDef` → `Attachment`）であり、**`error.rs` が唯一の源**
//!    である。識別子は正準テキスト形（26 文字固定幅）なので、テキスト昇順は ULID の数値昇順
//!    と一致する（[`crate::ids`] の新型の `Ord`）。一意性を先に見るのは、識別子が一意でないと
//!    参照先を一意に決められないためである。
//! 2. スキーマの存在（要件 4.4）: [`PartInventory::required_sheets`] の順、すなわち
//!    `document.json` のシート順（配列順そのものがデータ = 要件 1.1。ソートした順ではない）。
//!
//! この順序は `BTreeMap` / `BTreeSet` / `Vec` が構造として保証する。本モジュールは
//! `HashMap` / `HashSet` を使わない（反復順が実行ごとに変わり決定性を壊す。
//! `crate::value` / `crate::json` と同じ禁止理由）。
//!
//! # 純粋関数であること
//!
//! - I/O を一切行わない。入力は借用で受け、目録を変更しない（`&PartInventory` のみを取る）。
//! - モデルに依存しない: 検証は**モデル構築の前**に完了する（design 読み込みフロー、
//!   要件 5.4）。したがって `crate::model` を参照せず、識別子をコードから復号しない。
//! - panic しない: 空の目録・空テキストの宣言を含め、どんな目録でも [`Result`] を返す
//!   （添字参照も `unwrap` も持たない）。
//! - `serde_json` の汎用値型・マップ型を経由しない（親モジュール [`crate::parts`] の規則）。
//!
//! # 対象外
//!
//! - 参照の実在性（要件 1.7 / 4.2 / 7.4）: タスク 4.7 が同じファイルに追加する。
//! - `document.json` に無いシートのスキーマ（孤立パート）: コンテナ層の許可リスト
//!   （要件 2.5）と `manifest.json` の索引が扱う。
//! - 目録の網羅性: 全パートを目録へ写したこと、`document.json` が存在することは呼び出し元
//!   （4.8）の責務である。本検証は与えられた目録の内部整合だけを見る。
//!
//! # エラー対応
//!
//! | 違反 | 返す変種と文脈 |
//! |------|----------------|
//! | 同一種別・同一テキストの識別子が 2 回以上宣言されている | [`DocumentError::DuplicateId`]（`kind` = 種別、`id` = 識別子、`occurrences` = **重複した全出現箇所**を目録の順で） |
//! | 要求されたシートにスキーマが無い | [`DocumentError::MissingSchema`]（`sheet` = そのシートの識別子） |
//!
//! # 例
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

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{DocumentError, IdKind};

/// 識別子 1 個の宣言（[`PartInventory`] の要素）。
///
/// 種別を文字列ではなく [`IdKind`] で持つのは、一意性を種別ごとに判定するためである
/// （種別を無視すると、別体系の識別子が同じテキストを持つだけで重複と誤判定される）。
/// 出現箇所は診断用のテキストであり、本モジュールはその綴りを解釈しない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdDeclaration {
    /// 識別子の種別（`Sheet` / `Row` / `TypeDef` / `Attachment`）。
    pub kind: IdKind,
    /// 識別子のテキスト形（正準形。モジュール docs「識別子は正準テキスト形で与えられる」）。
    pub id: String,
    /// この宣言が現れた箇所を示すテキスト（`sheets/<ulid>.jsonl line 7` など）。
    pub location: String,
}

impl IdDeclaration {
    /// 種別・識別子・出現箇所から 1 件を組み立てる。
    pub fn new(kind: IdKind, id: impl Into<String>, location: impl Into<String>) -> Self {
        Self { kind, id: id.into(), location: location.into() }
    }
}

/// 復号済みパート群から取り出した識別子の目録（[`StructuralValidator`] の入力）。
///
/// 3 つのコレクションから成る（モジュール docs「入力の形」）:
///
/// | コレクション | 内容 | 順序 |
/// |--------------|------|------|
/// | [`PartInventory::declarations`] | 識別子の宣言 | 与えた順が出現箇所の順序 |
/// | [`PartInventory::required_sheets`] | ルートスキーマを要求するシート | 与えた順が文書順（`document.json` の配列順） |
/// | [`PartInventory::schema_sheets`] | スキーマパートを持つシート | 順序に意味は無い（存在判定のみ） |
///
/// `Clone` / `PartialEq` を持つのは、検証が目録を変更しないことをテストで直接確かめられる
/// ようにするためである（`PreservedFields` のような内部カーソルを持たない素のデータなので、
/// `PartialEq` の導出に落とし穴は無い）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartInventory {
    declarations: Vec<IdDeclaration>,
    required_sheets: Vec<String>,
    schema_sheets: BTreeSet<String>,
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
    /// 識別子の一意性（要件 4.3）とスキーマの存在（要件 4.4）を順に検証する。
    ///
    /// 違反があれば、モジュール docs「報告順」で定めた順序で**最初の 1 件**を返す。
    /// タスク 4.7 はこの合成点へ参照整合性の検証を加える。
    pub fn validate(inventory: &PartInventory) -> Result<(), DocumentError> {
        Self::validate_unique_ids(inventory.declarations())?;
        Self::validate_schema_presence(inventory.required_sheets(), inventory.schema_sheets())
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
                return Err(DocumentError::MissingSchema { sheet: sheet.clone() });
            }
        }
        Ok(())
    }
}

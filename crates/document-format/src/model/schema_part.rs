//! 不透明スキーマ・ペイロードとネスト型定義(タスク 2.2。要件 1.2, 1.3。未知フィールド
//! 保持の一般形への一本化はタスク 4.4)。
//!
//! # 不透明性(デザイン「スキーマ・ペイロードの不透明性」)
//!
//! [`SchemaPart`] の内容は本スペックにとって**不透明**である。本スペックが解釈するのは
//! 型定義の**識別子**と**参照構造**(`{"$ref": "<ターゲット>"}` のオブジェクト・マーカー)
//! だけであり、型の意味論には一切触れない。これにより `schema-engine` は本クレートを
//! 変更せずに型システムを進化させられる(design トレース可能性 1.2 / 1.3:
//! `SchemaPart::type_defs`)。
//!
//! 具体的には:
//!
//! * JSON ペイロードは [`RawJson`] として生のバイト列のまま保持し、
//!   `serde_json::Value` には一切保持しない(`value` モジュール docs の禁止理由参照)。
//!   バイト単位の一致が parse とアクセスの両方を生き延びる — 再シリアライズは行わない。
//!   不透明性の公認メカニズムは `serde_json::value::RawValue` である(JSON 値のソース
//!   テキストをそのまま捕捉する型であり、汎用値型ではない。`serde_json::Value` は
//!   引き続いて禁止)。
//! * 参照の抽出は serde のシリアライズ**イベント**の走査(`Visitor` ウォーク)で行い、
//!   何も意味解釈しない(テスト `reference_like_strings_are_not_references`、
//!   `semantics_blind_to_unknown_structures`)。
//!
//! # エンベロープ形状(本スペックが所有)
//!
//! `schemas/<sheet-ulid>.json` エントリの JSON テキストは次の形である:
//!
//! ```json
//! { "root": <opaque>, "types": [ { "id": "<ULID-26>", "definition": <opaque> } ] }
//! ```
//!
//! * `root` は**ちょうど 1 回**出現する(要件 1.2: 1 シートはルートスキーマを厳密に
//!   1 つ関連付ける)。欠落も重複も棄却する。JSON テキスト中の重複キーはデフォルトの
//!   last-wins ではなく、エンベロープ水準で明示的に検出して拒否する。
//! * `types` は 0 個以上の型定義のリスト(要件 1.3)。`types` キー自体の欠落は 0 個と
//!   解釈する(文書化済み決定)。
//! * エンベロープの最終的な符号化と決定的出力(バイト再構成)はタスク 4.4 の
//!   [`SchemaCodec`](crate::parts::schema_codec::SchemaCodec) が担う。本モジュールは
//!   parse と保持だけを所有する。
//!
//! # 未知フィールドの保持(前方互換。要件 6.2 / 6.3)
//!
//! 未知キーは [`PreservedFields`](crate::json::PreservedFields)(タスク 3.2 の一般形)が
//! **原文の位置ごと**保持する。タスク 2.2 はエンベロープのトップレベルに限定した独自の
//! 保持(`RawField` / `SchemaPart::unknown_fields`。本タスクで削除)を持っていたが、
//! タスク 4.4 で一般形へ一本化した(二重実装を残さない)。保持の範囲は**エンベロープの
//! 全階層**である:
//!
//! * トップレベル: `root` / `types` 以外のキー([`SchemaPart::preserved_fields`])。
//! * 型定義要素の内部: `id` / `definition` 以外のキー([`TypeDef::preserved_fields`])。
//!
//! 型定義要素の未知キーはタスク 2.2 では [`DocumentError::InvalidContainer`] として
//! **拒否**していた(前方互換はトップレベルのみという文書化済み決定)。しかし design の
//! `DeterministicJson` の責務は「未知フィールドは破棄せず保持し書き戻す」でスコープの
//! 限定が無く、拒否すると**将来の minor が型定義へ省略可能フィールドを足したときに
//! 読めなくなる**ため、タスク 4.4 で**保持へ変更**した(要素ごとに `PreservedFields` を
//! 1 つ持たせる)。意味は要件 6.2 / 6.3 に忠実な側であり、既知キーの重複拒否
//! (差し戻し位置の基準を守るため)は変わらない。
//!
//! 保持の値は原文のバイト列のままで、キーの表記(`\uXXXX` エスケープ)と値の外側の空白は
//! 保たれない(タスク 3.2 の規則。`PreservedFields` の docs)。書き戻しは
//! [`SchemaCodec`](crate::parts::schema_codec::SchemaCodec) が
//! [`PreservingObjectWriter`](crate::json::PreservingObjectWriter) で行う。
//!
//! # 解釈するもの
//!
//! * `id`: [`TypeDefId`](crate::TypeDefId) への解決(大文字小文字を問わない ULID テキスト。
//!   `ids` モジュール準拠)。型定義 id の重複はここでは検査しない — 重複検証は
//!   タスク 4.6 の担当である(読み込み時の報告を含む。文書化済み決定)。
//! * 参照構造: ルートまたは型定義ペイロード内の任意の深さで、JSON オブジェクトが
//!   キー `$ref` を持つとき、その値は文字列でなければならず、文字列の生テキストが
//!   ターゲットとして**未検証のまま**収集される(存在検証はタスク 4.7。要件 1.7)。
//!   参照に見える文字列は参照ではない(オブジェクト・キーのみが参照)。`$ref` の値が
//!   文字列でない場合は不正な参照構造であり、位置らしき情報を含む
//!   [`DocumentError::InvalidContainer`](crate::DocumentError::InvalidContainer)
//!   (コンテナ水準。文書化済み決定)。
//!
//! 走査順は `root`、次に型定義を保持順。1 ペイロード内はドキュメント順で、**重複を
//! 保持**して返す(呼び出し側が重複排除する。テストで文書化)。抽出した参照は 2 つの
//! ビューで取り出せる: 生テキストのみの [`SchemaPart::type_ref_targets`] と、参照元つきの
//! [`SchemaPart::type_refs`](要素は [`TypeRef`]。タスク 4.7。同一の走査が双方を埋める)。
//!
//! # 構築 API と Sheet 結線
//!
//! 構築は [`SchemaPart::parse`](エントリ・テキストからの復号)と
//! [`SchemaPart::empty`](空のルートスキーマでの初期化)のみである。部分的な変更や
//! プログラマティックなビルダーは必要になる後続タスクで追加する(文書化済み決定)。
//!
//! [`Sheet`](crate::model::Sheet) はこの型を `root_schema` としてちょうど 1 つ持ち
//! (要件 1.2 / design ER 図 `Sheet ||--|| SchemaPart : has_root`)、新規シートは
//! [`SchemaPart::empty`] で初期化される。差し替えは集約ルート経由の
//! [`Document::set_root_schema`](crate::model::Document::set_root_schema) のみで、
//! 常に置換である(0 個・2 個を作る口は無い)。

use std::fmt;
use std::str::FromStr;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

use crate::error::DocumentError;
use crate::ids::TypeDefId;
use crate::json::PreservedFields;

/// エンベロープ・キー: ルートスキーマ(要件 1.2)。符号化(タスク 4.4 の `SchemaCodec`)と
/// クレート内で共有するため `pub(crate)`(エンベロープ文法の単一の源)。
pub(crate) const KEY_ROOT: &str = "root";
/// エンベロープ・キー: ネスト型定義の配列(要件 1.3)。
pub(crate) const KEY_TYPES: &str = "types";
/// 型定義オブジェクトのキー: 識別子。
pub(crate) const KEY_ID: &str = "id";
/// 型定義オブジェクトのキー: 不透明な型定義本文。
pub(crate) const KEY_DEFINITION: &str = "definition";
/// 予約済み参照マーカー・キー(ペイロード内部専用。エンベロープ水準では無効)。
const REF_KEY: &str = "$ref";
/// `SchemaPart::parse` 失敗時の診断接頭辞(エントリ種別)。
const ENTRY_CONTEXT: &str = "schemas entry";
/// ルート・ペイロードの参照位置ラベル(参照元の診断と、走査失敗の診断接頭辞の双方を作る)。
const ROOT_LABEL: &str = "root";
/// [`SchemaPart::empty`] が持つ空のルート・ペイロード。
const EMPTY_ROOT: &str = "null";

/// parse 失敗をクレート共通エラーへ写す(`value` モジュールと同じ方針: design
/// エラー表に対応変種の無いコンテンツ解析失敗はコンテナ不正として扱う。理由は
/// 文字列のまま `entry` に残す)。`context` は失敗箇所を指す説明(例
/// `schemas entry: root`、`schemas entry: type <ULID>`)。
fn container_error(context: &dyn fmt::Display, reason: &dyn fmt::Display) -> DocumentError {
    DocumentError::InvalidContainer {
        entry: format!("{context}: {reason}"),
    }
}

/// 検証のみで保持する不透明 JSON ペイロード(タスク 2.2。design「不透明な保持」)。
///
/// 保存は常に**バイト完全一致**である: 構築時に受け取ったバイト列をそのまま保持し、
/// 再シリアライズは行わない。構文検証は serde のイベント走査(`IgnoredAny`)のみで、
/// `serde_json::Value` は作らない(不透明性の約束。モジュール docs 参照)。
///
/// 検証済み JSON バイト列は必ず有効な UTF-8 になる(JSON の文字列リテラルは UTF-8
/// 必須。文字列外の非 ASCII バイトは構文エラー)。したがって [`RawJson::as_str`] の
/// panic は構築不変条件により到達不能である。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawJson {
    bytes: Vec<u8>,
}

impl RawJson {
    /// テキストから構築し、JSON 構文だけを検証する(意味の解釈なし)。
    ///
    /// `entry` は失敗時の診断文脈(例 `schemas entry: root`)。バイト列は
    /// [`RawJson::as_bytes`] で受け取ったままの形復元される(前後の空白も含む)。
    /// 末尾のゴミも構文エラーとして棄却する(値 1 個で入力全体を消費することを要求)。
    pub fn from_str(text: &str, entry: &str) -> Result<Self, DocumentError> {
        Self::from_slice(text.as_bytes(), entry)
    }

    /// バイト列から構築し、JSON 構文だけを検証する([`RawJson::from_str`] のバイト版)。
    ///
    /// 無効 UTF-8 は構文エラーとして棄却する。`IgnoredAny` による構文走査は文字列
    /// **内容**の UTF-8 を検証しない(読み飛ばす)ため、ここで明示的に検証する:
    /// そうしないと [`RawJson::as_str`] の「常に有効 UTF-8」という不変条件が壊れる
    /// (JSON の文字列リテラルは UTF-8 必須 — 要件 2.4)。
    pub fn from_slice(bytes: &[u8], entry: &str) -> Result<Self, DocumentError> {
        let text = std::str::from_utf8(bytes).map_err(|e| container_error(&entry, &e))?;
        serde_json::from_str::<serde::de::IgnoredAny>(text)
            .map_err(|e| container_error(&entry, &e))?;
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    /// 保存済みバイト列のテキスト形。構築時に有効と検証されたバイト列のみが格納される
    /// ため、検証は不要で失敗しない。
    pub fn as_str(&self) -> &str {
        // 不変条件: 格納バイトは必ず構文検証済み JSON(= 有効 UTF-8)。
        // 両コンストラクタとエンベロープ経路(RawValue 捕捉)だけが格納する。
        std::str::from_utf8(&self.bytes)
            .expect("RawJson 不変条件: 構文検証済み JSON バイト列は有効 UTF-8")
    }

    /// 保存済みバイト列そのもの(verbatim)。
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// 内部コンストラクタ: **呼び出し側が構文検証済みと保証する**テキストからの構築。
    /// エンベロープ parse では `RawValue` として JSON パーサが構文検証済みのバイト列のみを渡す。
    fn captured(text: &str) -> Self {
        Self {
            bytes: text.as_bytes().to_vec(),
        }
    }
}

/// ネスト型定義 1 件: 識別子と不透明な定義ペイロード(要件 1.3)。
///
/// 定義ペイロードは解釈しない(意味論は `schema-engine`)。`id` の一意性はここでは
/// 検証しない(重複検証はタスク 4.6 の担当)。
///
/// `id` / `definition` 以外のキーは [`PreservedFields`] が原文の位置ごと保持する
/// (前方互換。要件 6.2 / 6.3。モジュール docs「未知フィールドの保持」)。`PartialEq` は
/// 提供しない: 保持している集合の比較には読み込みカーソルが混じるためである
/// ([`PreservedFields`] の docs)。したがって [`SchemaPart::type_defs`] の要素も直接 `==`
/// では比べられず、内容は [`TypeDef::id`] と [`TypeDef::definition`] を個別に比べる
/// ([`SchemaPart`] の docs 参照)。
#[derive(Debug, Clone)]
pub struct TypeDef {
    id: TypeDefId,
    definition: RawJson,
    /// 解釈しない要素内のフィールド(前方互換。要件 6.2 / 6.3)。
    preserved: PreservedFields,
}

impl TypeDef {
    /// 型定義識別子(エンベロープ・テキストから解決した ULID)。
    pub fn id(&self) -> TypeDefId {
        self.id
    }

    /// 不透明な定義ペイロード(parse 時のバイト列そのまま)。
    pub fn definition(&self) -> &RawJson {
        &self.definition
    }

    /// この型定義要素で保持した未知キー(原文の位置ごと。要件 6.2 / 6.3)。
    ///
    /// クレート内の符号化経路(タスク 4.4 の
    /// [`SchemaCodec`](crate::parts::schema_codec::SchemaCodec))が差し戻しに使う。
    pub(crate) fn preserved_fields(&self) -> &PreservedFields {
        &self.preserved
    }
}

/// 型定義参照 1 件(参照元ラベルと参照先の生テキスト。タスク 4.7。要件 1.7 / 4.2)。
///
/// [`SchemaPart::type_refs`] が返すビューであり、[`DocumentError::DanglingTypeRef`] の
/// `from` / `to` の両側を組み立てるための型である(どちらがどちらかが型から読める)。
///
/// * [`TypeRef::from`] は参照が現れた位置のラベルである: ルート・ペイロードなら `root`、
///   型定義の中なら `type <ULID-26>`。エントリ名(`schemas/<ulid>.json`)を前置して診断用の
///   参照元テキストにするのは呼び出し元の `parts` 層の責務である。
/// * [`TypeRef::to`] は `$ref` の値の**生テキスト**である。本クレートは参照構造だけを見る
///   ため、ULID とは限らない(存在検証は `parts` 層。design「スキーマペイロードの不透明性」)。
///
/// `PartialEq` は提供しない([`SchemaPart`] / [`TypeDef`] と同じ方針。タスク 2.2 / 4.1〜4.4)。
#[derive(Debug, Clone)]
pub struct TypeRef {
    from: String,
    to: String,
}

impl TypeRef {
    /// 参照元ラベル(`root` または `type <ULID-26>`)。
    pub fn from(&self) -> &str {
        &self.from
    }

    /// 参照先の生テキスト(`$ref` の値。ULID とは限らない)。
    pub fn to(&self) -> &str {
        &self.to
    }
}

/// 不透明スキーマ・ペイロード: ルートスキーマ 1 つとネスト型定義 N 件(要件 1.2, 1.3)。
///
/// 構造は保持のみで、ペイロード内部を解釈しない。抽出するのは型定義の識別子
/// ([`SchemaPart::type_def_ids`])と参照の出現
/// ([`SchemaPart::type_refs`] / [`SchemaPart::type_ref_targets`])だけである。構築は
/// [`SchemaPart::parse`] と [`SchemaPart::empty`] のみで、部分変更の API は持たない
/// (モジュール docs の「構築 API と Sheet 結線」参照)。
///
/// `PartialEq` は提供しない: 保持している未知フィールドの比較には読み込みカーソル
/// ([`PreservedFields`] の内部状態)が混じり、内容の等値を素直に表せないためである。
/// 2 つのスキーマが同じ内容かどうかは、[`SchemaPart::root`] のバイト列と各型定義の
/// [`TypeDef::id`] / [`TypeDef::definition`] を個別に比べるか、符号化したバイト列を
/// 比べて判定する(後者が本形式の契約: 同一内容は常に同一バイト列。要件 3.1)。
/// [`SchemaPart::type_defs`] を直接 `==` では比べられない — [`TypeDef`] も `PartialEq` を
/// 持たないためである。
#[derive(Debug, Clone)]
pub struct SchemaPart {
    root: RawJson,
    type_defs: Vec<TypeDef>,
    /// 解釈しないトップレベルのフィールド(前方互換。要件 6.2 / 6.3)。
    preserved: PreservedFields,
    /// 参照ターゲットの生テキスト(走査順。重複保持)。[`SchemaPart::type_ref_targets`] が
    /// そのまま返す、タスク 2.2 からの互換ビューである。
    ref_targets: Vec<String>,
    /// 参照元つきの出現列(タスク 4.7)。[`SchemaPart::type_refs`] がそのまま返す。
    /// `ref_targets` と同じ 1 回の走査が双方を埋める(走査器は 1 つ)。ターゲットの
    /// テキストを両ビューが別々に持つのは、互換ビューが `[String]`、こちらが [`TypeRef`] の
    /// 出現列という別々の型だからである(参照は希少で、解析時に 1 回だけ複製する)。
    refs: Vec<TypeRef>,
}

impl SchemaPart {
    /// ペイロード内部で参照マーカーとして予約されるオブジェクト・キー。
    pub const REF_KEY: &'static str = REF_KEY;

    /// `schemas/<sheet-ulid>.json` の JSON テキストをパースして保持する。
    ///
    /// 検証するのはエンベロープ構造(`root` がちょうど 1 つ、`types` の形、型定義
    /// オブジェクトの形)と参照構造(`$ref` の値が文字列であること)だけである。
    /// 不正はすべて [`DocumentError::InvalidContainer`] として報告する。
    /// `entry` には失敗箇所のラベル(エンベロープ、`root`、`type <ULID>`)と理由を残す。
    pub fn parse(text: &str) -> Result<Self, DocumentError> {
        // エンベロープ 1 個で入力全体を消費することを要求する(末尾のゴミを拒否)。
        let mut de = serde_json::Deserializer::from_str(text);
        let envelope =
            Envelope::deserialize(&mut de).map_err(|e| container_error(&ENTRY_CONTEXT, &e))?;
        de.end().map_err(|e| container_error(&ENTRY_CONTEXT, &e))?;

        // 参照抽出: ルートを先に、続いて型定義を保持順に走査する。抽出は構造の走査
        // のみで、型の意味論は一切解釈しない(失敗は該当パスのラベル付きで報告)。
        // 走査は 1 回だけで、参照元つきの出現列(タスク 4.7)とターゲット列(タスク 2.2
        // からの互換ビュー)を同時に埋める。
        let mut ref_targets = Vec::new();
        let mut refs = Vec::new();
        scan_payload(&envelope.root, &ROOT_LABEL, &mut refs, &mut ref_targets)?;
        for def in &envelope.types {
            let label = format!("type {}", def.id);
            scan_payload(&def.definition, &label, &mut refs, &mut ref_targets)?;
        }

        Ok(Self {
            root: envelope.root,
            type_defs: envelope.types,
            preserved: envelope.preserved,
            ref_targets,
            refs,
        })
    }

    /// 空のルートスキーマ(root ペイロード `null`、型定義 0 件、未知フィールド 0 件)を作る。
    ///
    /// 新規シートの初期化に使う(要件 1.2 の「ちょうど 1 つ」を型で保つ)。構文検証を
    /// 通したバイト列のみを保持するため、[`RawJson::from_str`] を経由して構築する。
    pub fn empty() -> Self {
        // `null` は構文検証済みの定数であり、この expect はパターンではなく不変条件の
        // 明文化である(失敗し得ない経路に panic を置かない設計は `value` モジュールと同様)。
        let root = RawJson::from_str(EMPTY_ROOT, ENTRY_CONTEXT)
            .expect("不変条件: 定数 `null` は常に妥当な JSON");
        Self {
            root,
            type_defs: Vec::new(),
            preserved: PreservedFields::new(),
            ref_targets: Vec::new(),
            refs: Vec::new(),
        }
    }

    /// ルートスキーマ(厳密に 1 つ。要件 1.2 の強制地点)。
    pub fn root(&self) -> &RawJson {
        &self.root
    }

    /// 保持されている型定義すべて(エンベロープ内の出現順)。
    pub fn type_defs(&self) -> &[TypeDef] {
        &self.type_defs
    }

    /// 型定義識別子の一覧(エンベロープ内の出現順。重複は排除しない)。
    pub fn type_def_ids(&self) -> Vec<TypeDefId> {
        self.type_defs.iter().map(TypeDef::id).collect()
    }

    /// ルートと型定義から構造的に抽出した参照ターゲットの生テキスト。
    ///
    /// ルートを先に、続いて型定義を保持順に走査し、各ペイロード内はドキュメント順、
    /// **重複はそのまま**返す(呼び出し側が必要に応じて重複排除する。ターゲットの
    /// ULID 妥当性・存在の有無は検証しない — 検証はタスク 4.7 の担当)。
    pub fn type_ref_targets(&self) -> &[String] {
        &self.ref_targets
    }

    /// 参照を**参照元つき**で列挙する(タスク 4.7。要件 1.7 / 4.2)。
    ///
    /// 各要素は [`TypeRef`] である: [`TypeRef::from`] は参照元ラベル(`root` または
    /// `type <ULID-26>`。エントリ名を前置して診断用の参照元テキストを組むのは呼び出し元の
    /// `parts` 層)、[`TypeRef::to`] は `$ref` の値の**生テキスト**(ULID とは限らない)である。
    /// 順序と重複の保持は [`SchemaPart::type_ref_targets`] と同一であり、同じ 1 回の走査が
    /// 双方を埋める(`type_ref_targets` はターゲットのみの互換ビュー)。
    ///
    /// 参照の実在検証(同一シートの型定義集合にあるか)は本クレートの `parts` 層
    /// ([`crate::parts::StructuralValidator`])の責務であり、ここでは区分もしない。
    pub fn type_refs(&self) -> &[TypeRef] {
        &self.refs
    }

    /// エンベロープのトップレベルで保持した未知キー(原文の位置ごと。要件 6.2 / 6.3)。
    ///
    /// クレート内の符号化経路(タスク 4.4 の
    /// [`SchemaCodec`](crate::parts::schema_codec::SchemaCodec))が差し戻しに使う。
    pub(crate) fn preserved_fields(&self) -> &PreservedFields {
        &self.preserved
    }
}

// ---------------------------------------------------------------------------
// parse 内部
// ---------------------------------------------------------------------------

/// parse 途中のエンベロープ構造(`root` はちょうど 1 つを visitor で強制する)。
struct Envelope {
    root: RawJson,
    types: Vec<TypeDef>,
    preserved: PreservedFields,
}

impl<'de> Deserialize<'de> for Envelope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(EnvelopeVisitor)
    }
}

/// エンベロープのトップレベルを 1 パスで読む visitor。
///
/// serde_json はマップの重複キーをデフォルトでは黙って last-wins にする。`root` /
/// `types` はこのスペックが解釈するキーなので、キー出現回数を visitor 側で数えて
/// **明示的に重複を拒否する**(要件 1.2 の「ちょうど 1 つ」)。未知キーは
/// [`PreservedFields`] が原文の位置ごと保持し(将来互換。要件 6.2 / 6.3)、値は
/// `RawValue` で構文検証済みバイトのみを受け取る。
struct EnvelopeVisitor;

impl<'de> Visitor<'de> for EnvelopeVisitor {
    type Value = Envelope;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a schema envelope object with exactly one `root` key")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Envelope, A::Error> {
        let mut root: Option<RawJson> = None;
        let mut types: Option<Vec<TypeDef>> = None;
        let mut preserved = PreservedFields::new();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                KEY_ROOT => {
                    if root.is_some() {
                        return Err(de::Error::custom("duplicate `root` key in schema envelope"));
                    }
                    root = Some(RawJson::captured(map.next_value::<Box<RawValue>>()?.get()));
                    // 書き出し側の `write_known` と同じ順序で既知フィールドの件数を
                    // 進める(未知フィールドの差し戻し位置の基準。タスク 3.2 の必須条件)。
                    preserved.record_known_field();
                }
                KEY_TYPES => {
                    if types.is_some() {
                        return Err(de::Error::custom(
                            "duplicate `types` key in schema envelope",
                        ));
                    }
                    types = Some(map.next_value::<Vec<TypeDef>>()?);
                    preserved.record_known_field();
                }
                _ => {
                    // 未知のトップレベル・キー: 値を参照として走査しない(走査対象は
                    // ルートと型定義のみ。文書化済み決定)。重複キーも保持する。
                    preserved.capture(&key, &mut map)?;
                }
            }
        }
        let Some(root) = root else {
            return Err(de::Error::custom(
                "schema envelope is missing the `root` key",
            ));
        };
        Ok(Envelope {
            root,
            types: types.unwrap_or_default(),
            preserved,
        })
    }
}

impl<'de> Deserialize<'de> for TypeDef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(TypeDefVisitor)
    }
}

/// 型定義オブジェクトの visitor。`id` と `definition` をちょうど 1 回ずつ要求し、未知キーは
/// [`PreservedFields`] が原文の位置ごと保持する(前方互換。要件 6.2 / 6.3。タスク 2.2 の
/// 「未知キーは拒否」からタスク 4.4 で保持へ変更した。モジュール docs
/// 「未知フィールドの保持」)。
struct TypeDefVisitor;

impl<'de> Visitor<'de> for TypeDefVisitor {
    type Value = TypeDef;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a type-definition object with an `id` and a `definition`")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<TypeDef, A::Error> {
        let mut id: Option<String> = None;
        let mut definition: Option<RawJson> = None;
        let mut preserved = PreservedFields::new();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                KEY_ID => {
                    if id.is_some() {
                        return Err(de::Error::custom("duplicate `id` key in type definition"));
                    }
                    id = Some(map.next_value::<String>()?);
                    // 書き出し側の `write_known` と同じ順序で件数を進める(必須条件)。
                    preserved.record_known_field();
                }
                KEY_DEFINITION => {
                    if definition.is_some() {
                        return Err(de::Error::custom(
                            "duplicate `definition` key in type definition",
                        ));
                    }
                    definition = Some(RawJson::captured(map.next_value::<Box<RawValue>>()?.get()));
                    preserved.record_known_field();
                }
                _ => preserved.capture(&key, &mut map)?,
            }
        }
        let Some(id_text) = id else {
            return Err(de::Error::custom("type definition is missing the `id` key"));
        };
        let Some(definition) = definition else {
            return Err(de::Error::custom(format_args!(
                "type definition `{id_text}` is missing the `definition` key"
            )));
        };
        // `id` は ULID-26 テキストとして解決する(型の解釈ではない: 識別子解決は本
        // スペックの所有。エラー入力は診断文言へ写す)。
        let id = TypeDefId::from_str(&id_text).map_err(de::Error::custom)?;
        Ok(TypeDef {
            id,
            definition,
            preserved,
        })
    }
}

/// 不透明ペイロードをイベント走査して `$ref` ターゲットを収集する visitor。
///
/// 意味解釈はしない: スカラーはすべて無視し、文字列は内容を見ない(参照に見える
/// 文字列は参照ではない)。`$ref` キーのみ値を調べ、文字列でなければ serde の型エラー
/// (`invalid type` + 位置情報)として走査全体を中止する。再帰は serde_json の再帰
/// 制限(既定 128)がスタック溢出を防ぐ。
struct RefScan<'a> {
    targets: &'a mut Vec<String>,
}

impl<'a, 'de> Visitor<'de> for RefScan<'a> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    /// JSON の文字列。**内容は一切見ない** — 参照に見える文字列は参照ではない
    /// (参照は `$ref` キーを持つオブジェクトのみ。テスト
    /// `reference_like_strings_are_not_references`)。`visit_string` /
    /// `visit_borrowed_str` は serde の既定でこのメソッドへ転送される。
    fn visit_str<E: de::Error>(self, _v: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_bool<E: de::Error>(self, _v: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E: de::Error>(self, _v: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E: de::Error>(self, _v: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E: de::Error>(self, _v: f64) -> Result<(), E> {
        Ok(())
    }

    /// JSON の `null`。
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let Self { targets } = self;
        // 各要素で &mut を再借用して再帰する(seeds は消費されるがループで再利用する)。
        while seq
            .next_element_seed(RefScan {
                targets: &mut *targets,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let Self { targets } = self;
        while let Some(key) = map.next_key::<String>()? {
            if key == REF_KEY {
                // 参照マーカー: 値は文字列でなければならない(RefTarget が検証し、
                // 非文字列は invalid_type 位置付きエラーで走査全体を中止させる)。
                targets.push(map.next_value_seed(RefTarget)?);
            } else {
                map.next_value_seed(RefScan {
                    targets: &mut *targets,
                })?;
            }
        }
        Ok(())
    }
}

/// `$ref` の値専用 visitor: 文字列のみ受け取る。他型は serde の既定で
/// `invalid type`(位置付きエラー)となる。
struct RefTarget;

impl<'de> Visitor<'de> for RefTarget {
    type Value = String;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string type-reference target")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
        Ok(v.to_owned())
    }
}

/// 入れ子の値走査を seed として渡すための実装([`MapAccess::next_value_seed`] /
/// [`SeqAccess::next_element_seed`] が要求する)。値の解釈は [`Visitor`] 側と同一。
impl<'de, 'a> DeserializeSeed<'de> for RefScan<'a> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

/// `$ref` 値の seed 実装。文字列のみを受け取り、他型は位置付きの `invalid type` に落ちる。
impl<'de> DeserializeSeed<'de> for RefTarget {
    type Value = String;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<String, D::Error> {
        deserializer.deserialize_str(self)
    }
}

/// 不透明ペイロード 1 本を走査し、参照元つきの出現列と互換ビュー(ターゲット列)を
/// **同じ 1 回の走査**で埋める(タスク 4.7。走査器を二重に持たない)。
///
/// `label` は参照元ラベルであり、同時に走査失敗の診断接頭辞(`schemas entry: <label>`)を
/// 作る。走査が失敗した場合、このペイロードの参照は 1 件も採用されない(呼び出し元は
/// 全体を中止するため、部分的な列は捨てられる)。
fn scan_payload(
    raw: &RawJson,
    label: &dyn fmt::Display,
    refs: &mut Vec<TypeRef>,
    targets: &mut Vec<String>,
) -> Result<(), DocumentError> {
    let start = targets.len();
    serde_json::Deserializer::from_str(raw.as_str())
        .deserialize_any(RefScan {
            targets: &mut *targets,
        })
        .map_err(|e| container_error(&format_args!("{}: {}", ENTRY_CONTEXT, label), &e))?;
    // 出現列はターゲット列の同じ範囲から作る(順序も重複もそのまま)。
    if targets.len() > start {
        let from = label.to_string();
        refs.extend(targets[start..].iter().map(|target| TypeRef {
            from: from.clone(),
            to: target.clone(),
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正準 ULID テキスト(26 文字。Crockford base32 大文字)。
    const ID_A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const ID_B: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA6";
    const ID_C: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA7";

    /// テキストから `TypeDefId` リストを作る(テストの読みやすさ用)。
    fn ids(texts: &[&str]) -> Vec<TypeDefId> {
        texts
            .iter()
            .map(|t| TypeDefId::from_str(t).unwrap())
            .collect()
    }

    /// 不正入力が必ず `InvalidContainer` になることを確認して `entry` を返す。
    fn invalid_container(text: &str) -> String {
        match SchemaPart::parse(text) {
            Err(DocumentError::InvalidContainer { entry }) => entry,
            other => panic!("InvalidContainer を期待: {other:?}"),
        }
    }

    // --- 不透明フェデリティ -------------------------------------------------

    #[test]
    fn raw_json_validates_syntax_and_preserves_bytes() {
        // バイト列は verbatim に保持する(前後の空白も含む — verbatim 約束)。
        let src = String::from(" [1, {\"a\": \"e🎉\"} ] ");
        let raw = RawJson::from_slice(src.as_bytes(), "schemas/t.json").unwrap();
        assert_eq!(src.as_bytes(), raw.as_bytes());
        assert_eq!(src.as_bytes(), raw.as_str().as_bytes());
        // from_str も同じバイト列を保持する。
        let from_text = RawJson::from_str(r#" {"x" : [ ] } "#, "schemas/t.json").unwrap();
        assert_eq!(r#" {"x" : [ ] } "#, from_text.as_str());
        // 構文検証のみ: 妥当な JSON 構文なら受理する(未知キーも大きな整数も不透明)。
        assert!(RawJson::from_str(r#"{"big": 99999999999999999999999}"#, "schemas/t.json").is_ok());
        // 構文不正は InvalidContainer(entry に理由が残る)。
        for bad in ["[1,", "nope", "{\"a\"}"] {
            let err = RawJson::from_str(bad, "schemas/t.json").unwrap_err();
            match err {
                DocumentError::InvalidContainer { entry } => {
                    assert!(entry.contains("schemas/t.json"))
                }
                other => panic!("InvalidContainer を期待: {other:?}"),
            }
        }
        // UTF-8 でないバイトは JSON 構文エラーとして棄却される。
        assert!(RawJson::from_slice(b"\"\xff\"", "schemas/t.json").is_err());
    }

    #[test]
    fn root_and_type_definitions_are_held_byte_identical() {
        // 内部の空白・Unicode・エスケープ・大きな整数まで parse → アクセスで
        // バイト同一(再シリアライズなら漂移する = 解釈していない証明)。
        let text = concat!(
            r#"{"root" :  { "cols" : [ {"name":"金額","kind":9999999999999999999999999} ,"e🎉\n\u00e9" ] ,"z": null },"#,
            r#""types":[{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","definition": { "x" : [ null, false ] } }]}"#,
        );
        let part = SchemaPart::parse(text).unwrap();
        assert_eq!(
            r#"{ "cols" : [ {"name":"金額","kind":9999999999999999999999999} ,"e🎉\n\u00e9" ] ,"z": null }"#,
            part.root().as_str()
        );
        assert_eq!(1, part.type_defs().len());
        assert_eq!(ids(&[ID_A])[0], part.type_defs()[0].id());
        assert_eq!(
            r#"{ "x" : [ null, false ] }"#,
            part.type_defs()[0].definition().as_str()
        );
    }

    // --- 要件 1.2: ルートはちょうど 1 つ -----------------------------------

    #[test]
    fn part_holds_exactly_one_root_schema() {
        // 要件 1.2: 1 シートはルートスキーマをちょうど 1 つ持つ。
        // SchemaPart は `root` フィールドをちょうど 1 つだけ持つ構造であり、
        // 0 個 / 2 個は parse で拒否される(root_key_* テスト参照)。
        // root は不透明 — 妥当な JSON 値なら null でも許される。
        let part = SchemaPart::parse(r#"{"root":null}"#).unwrap();
        assert_eq!("null", part.root().as_str());
        assert!(part.type_defs().is_empty());
        assert!(part.preserved_fields().is_empty());
    }

    #[test]
    fn empty_is_a_null_root_with_no_definitions() {
        // 新規シートの初期化に使う空スキーマ: root = null、型定義 0 件、
        // 未知フィールド 0 件、参照 0 件。構築は RawJson::from_str を経由するため
        // parse 経由の `{"root":null}` と同一の内容になる(SchemaPart は保持している
        // 未知フィールドの読み込みカーソルを等値比較へ持ち込まないため PartialEq を
        // 持たない。内容は各アクセサで比べる)。
        let part = SchemaPart::empty();
        assert_eq!("null", part.root().as_str());
        assert!(part.type_defs().is_empty());
        assert!(part.preserved_fields().is_empty());
        assert!(part.type_ref_targets().is_empty());
        let parsed = SchemaPart::parse(r#"{"root":null}"#).unwrap();
        assert_eq!(part.root().as_bytes(), parsed.root().as_bytes());
        assert_eq!(part.type_defs().len(), parsed.type_defs().len());
        assert_eq!(
            part.preserved_fields().len(),
            parsed.preserved_fields().len()
        );
    }

    #[test]
    fn root_key_is_required() {
        // 要件 1.2: 0 個は不正。
        let entry = invalid_container(r#"{"types": []}"#);
        assert!(entry.contains("root"), "診断に root を含むべき: {entry}");
    }

    #[test]
    fn duplicate_root_key_is_rejected() {
        // 要件 1.2: 2 個は不正。serde_json のデフォルト last-wins にせず、
        // エンベロープ水準で明示的に検出する。出現順どちらのパターンも拒否する。
        for text in [
            r#"{"root": 1, "root": 2, "types": []}"#,
            r#"{"root":1,"types":[],"root":2}"#,
        ] {
            let entry = invalid_container(text);
            assert!(entry.contains("root"), "診断に root を含むべき: {entry}");
        }
    }

    #[test]
    fn duplicate_types_key_is_rejected() {
        let entry = invalid_container(r#"{"root":{},"types":[],"types":[]}"#);
        assert!(entry.contains("types"), "診断に types を含むべき: {entry}");
    }

    // --- types の形 ---------------------------------------------------------

    #[test]
    fn types_with_wrong_shape_is_rejected() {
        // 配列でなければならない(`{}` は棄却)。
        invalid_container(r#"{"root":{},"types":{}}"#);
        // 要素は型定義オブジェクトでなければならない。
        invalid_container(r#"{"root":{},"types":[1]}"#);
    }

    #[test]
    fn missing_types_means_zero_definitions() {
        // 要件 1.3: 型定義は 0 個以上。`types` キー欠落は 0 個(文書化済み決定)。
        let part = SchemaPart::parse(r#"{"root":{"a":1}}"#).unwrap();
        assert!(part.type_defs().is_empty());
        assert!(part.type_def_ids().is_empty());
    }

    #[test]
    fn type_definitions_are_held_and_listed() {
        // 要件 1.3: N 個の型定義を保持し、識別子で一覧できる。
        let text = format!(
            "{{\"root\":{{\"v\":1}},\"types\":[\
             {{\"id\":\"{ID_A}\",\"definition\":{{\"a\":[1]}}}},\
             {{\"id\":\"{ID_B}\",\"definition\":\"scalar-ok\"}},\
             {{\"id\":\"{ID_C}\",\"definition\":[null]}}]}}"
        );
        let part = SchemaPart::parse(&text).unwrap();
        assert_eq!(3, part.type_defs().len());
        assert_eq!(ids(&[ID_A, ID_B, ID_C]), part.type_def_ids());
        assert_eq!(r#"{"a":[1]}"#, part.type_defs()[0].definition().as_str());
        assert_eq!(r#""scalar-ok""#, part.type_defs()[1].definition().as_str());
        assert_eq!("[null]", part.type_defs()[2].definition().as_str());
    }

    #[test]
    fn type_definition_shape_is_enforced() {
        // `id` / `definition` をちょうど 1 ずつ(重複は差し戻し位置の基準を壊すため拒否。
        // 未知キーは保持するのでここでは拒否しない。`unknown_keys_inside_type_definitions_
        // are_preserved` 参照)。id は ULID-26 テキスト。
        let cases = [
            r#"{"root":0,"types":[{"definition":{}}]}"#.to_owned(),
            format!(r#"{{"root":0,"types":[{{"id":"{ID_A}"}}]}}"#),
            format!(r#"{{"root":0,"types":[{{"id":"{ID_A}","id":"{ID_B}","definition":{{}}}}]}}"#),
            format!(r#"{{"root":0,"types":[{{"id":"{ID_A}","definition":1,"definition":2}}]}}"#),
            format!(r#"{{"root":0,"types":[{{"id":"not-a-ulid","definition":{{}}}}]}}"#),
        ];
        for text in &cases {
            invalid_container(text);
        }
    }

    #[test]
    fn unknown_keys_inside_type_definitions_are_preserved() {
        // 前方互換(要件 6.2 / 6.3): 型定義要素の未知キーはタスク 2.2 では
        // `InvalidContainer` で拒否していたが、タスク 4.4 で**保持**へ変更した
        // (将来の minor が型定義へ省略可能フィールドを足しても読めなくなるため)。
        // 位置は [`PreservedFields`](crate::json::PreservedFields) が原文のまま持ち、
        // 書き戻しは `SchemaCodec` が行う(バイト単位の往復は parts 層のテスト)。
        // 要素 1 件だけでは保持が壊れても検出できないため 2 件で分散させる。
        let text = format!(
            "{{\"root\":{{}},\"types\":[\
             {{\"past\":1,\"id\":\"{ID_A}\",\"mid\":2,\"definition\":{{}},\"tail\":3}},\
             {{\"id\":\"{ID_B}\",\"definition\":[null],\"only\":true}}]}}"
        );
        let part = SchemaPart::parse(&text).unwrap();
        assert_eq!(2, part.type_defs().len());
        assert_eq!(ids(&[ID_A, ID_B]), part.type_def_ids());

        let first: Vec<(&str, usize)> = part.type_defs()[0]
            .preserved_fields()
            .iter()
            .map(|field| (field.key(), field.preceding_known_fields()))
            .collect();
        let second: Vec<(&str, usize)> = part.type_defs()[1]
            .preserved_fields()
            .iter()
            .map(|field| (field.key(), field.preceding_known_fields()))
            .collect();
        assert_eq!(vec![("past", 0), ("mid", 1), ("tail", 2)], first);
        assert_eq!(vec![("only", 2)], second);
        // 値は原文のバイト列のまま。
        let tail = &part.type_defs()[0]
            .preserved_fields()
            .iter()
            .nth(2)
            .unwrap();
        assert_eq!(b"3", tail.value_bytes());
    }

    // --- 参照抽出 -----------------------------------------------------------

    #[test]
    fn references_are_extracted_in_document_order() {
        // 深さ 1、配列直下、3 階層以上、型定義内 — すべてドキュメント順で抽出する。
        // 走査順は root → 型定義(保持順)。
        let text = r#"{"root":{"flat":{"$ref":"R1"},"list":[{"$ref":"R2"},{"y":{"deep":{"$ref":"R3"}}}],"deep":{"a":{"b":{"c":{"$ref":"R4"}}}}},"types":[{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","definition":{"$ref":"R5"}}]}"#;
        let part = SchemaPart::parse(text).unwrap();
        let targets: Vec<&str> = part.type_ref_targets().iter().map(String::as_str).collect();
        assert_eq!(
            targets,
            ["R1", "R2", "R3", "R4", "R5"],
            "ドキュメント順(root 先、型定義後)で抽出されない"
        );
    }

    #[test]
    fn duplicate_targets_are_preserved_and_raw() {
        // 重複は排除しない(呼び出し側が重複排除する。モジュール docs の決定)。
        // ターゲットは生テキストのまま — ULID でなくても parse は通す(検証は 4.7)。
        let text = r#"{"root":{"a":{"$ref":"X"},"c":[{"$ref":"Y"},{"$ref":"X"}],"b":{"$ref":"not-a-ulid!!"}}}"#;
        let part = SchemaPart::parse(text).unwrap();
        let targets: Vec<&str> = part.type_ref_targets().iter().map(String::as_str).collect();
        assert_eq!(targets, ["X", "Y", "X", "not-a-ulid!!"]);
    }

    #[test]
    fn reference_like_strings_are_not_references() {
        // 意味論ブラインドの核心: 文字列値は(内部に `$ref` に見えるテキストが
        // あっても)参照ではない。キーが `$ref` と完全一致するオブジェクトのみ参照。
        let text = r#"{"root":{"note":"{\"$ref\": \"01ARZ3NDEKTSV4RRFFQ69G5FAV\"}","also":"$ref","$refx":"01ARZ3NDEKTSV4RRFFQ69G5FAV","$REF":"x","nested":{"Ref":{"$ref":"real"}}}}"#;
        let part = SchemaPart::parse(text).unwrap();
        let targets: Vec<&str> = part.type_ref_targets().iter().map(String::as_str).collect();
        assert_eq!(targets, ["real"]);
    }

    #[test]
    fn non_string_ref_target_is_container_error() {
        // `$ref` の値が非文字列は不正な参照構造 → InvalidContainer(文書化済み決定)。
        // エントリには場所らしき情報(ペイロードのラベル + 列番号)を含める。
        let entry = invalid_container(r#"{"root":{"a":{"$ref":123}}}"#);
        assert!(entry.contains("root"), "root ラベルを含むべき: {entry}");
        assert!(entry.contains("column"), "位置情報を含むべき: {entry}");
        // null も非文字列として棄却。
        invalid_container(r#"{"root":{"$ref":null}}"#);
        // 型定義内の不正参照はその型定義の識別子を診断に含む。
        let entry = invalid_container(&format!(
            r#"{{"root":{{}},"types":[{{"id":"{ID_A}","definition":{{"$ref":[1]}}}}]}}"#
        ));
        assert!(entry.contains(ID_A), "型定義 id を含むべき: {entry}");
    }

    #[test]
    fn semantics_blind_to_unknown_structures() {
        // 本クレートは型意味論を解釈しない:
        let part = SchemaPart::parse(
            r#"{"root":{"anything":{"weird":[1e30,-0,"x",{"unknown":{"deep":[[[null]]]}}]},"big":123456789012345678901234567890,"exp":1e30}}"#,
        )
        .unwrap();
        // 未知キー・未知構造・浮動小数・大きな整数を含んでも parse は成功し、
        // 構造的な参照は何も抽出しない(意味論ブラインド)。
        assert!(part.type_ref_targets().is_empty());
    }

    #[test]
    fn envelope_level_ref_key_is_not_scanned() {
        // 参照マーカーはペイロード内部専用。エンベロープの未知トップレベル・キーに
        // なった `$ref` は参照として扱わず、不透明フィールドとして保持する
        // (前方互換の決定。走査対象は root / 型定義のみ)。
        let part = SchemaPart::parse(r#"{"$ref":"01ARZ3NDEKTSV4RRFFQ69G5FAV","root":{}}"#).unwrap();
        assert!(part.type_ref_targets().is_empty());
        let preserved: Vec<&str> = part
            .preserved_fields()
            .iter()
            .map(|field| field.key())
            .collect();
        assert_eq!(vec!["$ref"], preserved);
        assert_eq!(
            br#""01ARZ3NDEKTSV4RRFFQ69G5FAV""#.as_slice(),
            part.preserved_fields().iter().next().unwrap().value_bytes()
        );
    }

    // --- 未知キーの保持 -----------------------------------------------------

    #[test]
    fn unknown_top_level_fields_are_preserved_verbatim() {
        // 前方互換(ルール 6.2 / 6.3): 未知キーは原文の順序とバイト列そのままで保持し、
        // 位置は「その未知キーより前に現れた既知フィールドの件数」で表す
        // (PreservedFields の方式。タスク 3.2)。未知キー同士の重複もそのまま保持する。
        // 最終的なバイト再構成はタスク 4.4 の `SchemaCodec` が担う(ここでは保持まで)。
        let text = r#"{"future":{"deep": [1, "é"]},"root":{},"beta":"b","beta":2}"#;
        let part = SchemaPart::parse(text).unwrap();
        let preserved: Vec<(&str, &[u8], usize)> = part
            .preserved_fields()
            .iter()
            .map(|field| {
                (
                    field.key(),
                    field.value_bytes(),
                    field.preceding_known_fields(),
                )
            })
            .collect();
        assert_eq!(
            vec![
                ("future", r#"{"deep": [1, "é"]}"#.as_bytes(), 0),
                ("beta", br#""b""#.as_slice(), 1),
                ("beta", b"2".as_slice(), 1),
            ],
            preserved
        );
    }

    #[test]
    fn deeply_nested_payload_is_rejected_without_overflow() {
        // 異常に深いネストは構築を拒否する(serde_json の再帰制限がスタック溢出を
        // 防ぐ — イベント走査抽出も同じガードを継承する)。
        let deep = format!("{}{}", "[".repeat(5_000), "]".repeat(5_000));
        let text = format!(r#"{{"root":{deep}}}"#);
        invalid_container(&text);
    }
}

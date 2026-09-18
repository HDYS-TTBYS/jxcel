//! マクロ向け型定義（`.d.ts`）の生成と、宣言表との乖離の検査（tasks.md 3.3。要件 4.6,
//! 10.1–10.3。design.md「Components and Interfaces」の `types.rs`）。
//!
//! マクロを書く人が補完を使えるように、**マクロから見えるホスト API と型**を 1 つの
//! `.d.ts`（[`GENERATED_PATH`]）として組み立てる。**手で書かない**（`structure.md`
//! 「マクロ向け型定義」）。直すのは生成元であり、生成物は追跡して生成器の出力とバイト比較する
//! （`src/ipc/bindings.ts` と同じ運用。検査は `tests/macro_host_dts_drift.rs`）。
//!
//! # 何から組み立てるか（2 つの源）
//!
//! | 部分 | 源 |
//! |---|---|
//! | ホスト API の関数の宣言（`declare namespace host { … }`） | [`HOST_APIS`]（宣言表。タスク 2.1） |
//! | 型の宣言（`type SheetInfo = …;`） | [`catalog`]（`ts-rs` の宣言と、下記の**決めた綴り**） |
//!
//! 能力を要する API は、その宣言のコメントに**能力名を書く**（要件 8.2 の提示と一致させる。
//! 補完で見える）。
//!
//! # 型の出どころ（この決定の記録。tasks.md 3.3 の「型の無い API を公開できない」）
//!
//! `ipc-contract.md` は「`ts-rs` の derive を付けてよいのは `crates/app-shell/src/ipc/` の
//! 下だけ」と定める（`.kiro/specs/schema-engine/research.md` の 33 行目が引く）。
//! **したがって上流（`document-format` / `schema-engine`）の型に `ts-rs` の導出は足せない**
//! — 上流に導出が無いのは漏れではなく意図である。一方でマクロ向けの型定義は
//! 「ドメインクレートから生成する。手書きしない」を要る（`structure.md`「マクロ向け型定義」）。
//! 両立点は**この 1 箇所**である:
//!
//! 1. **macro-runtime 自身の型は `ts-rs` を導出する** — `host/overlay.rs` の
//!    [`SheetInfo`] / [`ColumnTypeInfo`] / [`RowSpan`] / [`ReadRow`] / [`RowPage`] と
//!    `host/changes.rs` の [`CellWrite`]。macro-runtime は IPC 境界ではないため、
//!    `ipc-contract.md` の射程外である（本クレートの値は V8 の境界を越え、JSON を通らない）
//! 2. **上流の型は、マクロから見える綴りを本モジュールで決める**（[`FIXED_ALIASES`] と
//!    [`type_kind_name`](crate::host::value::type_kind_name)）。導出を写すのではなく、
//!    **境界の綴り**を決めている。
//!    `TypeKind` の合併型は `schema-engine` の種別カタログ（`TypeKind::ALL`）を走査して
//!    組み立て、**一覧を書き写さない**
//!
//! 宣言表（`surface/declaration.rs`）が要求する型名と、その出どころは次のとおりである
//! （[`catalog`] の `source` 欄が機械可読な同じ情報を持つ）:
//!
//! | 型名 | 出どころ |
//! |---|---|
//! | `SheetInfo` / `ColumnTypeInfo` / `RowSpan` / `ReadRow` / `RowPage` | `crates/macro-runtime/src/host/overlay.rs`（`ts-rs` の導出） |
//! | `CellWrite` | `crates/macro-runtime/src/host/changes.rs`（`ts-rs` の導出） |
//! | `SheetId` / `RowId` | `crates/document-format/src/ids.rs` の識別子。マクロから見える形は `string` |
//! | `CellValue` | `crates/macro-runtime/src/host/value.rs`（値の写像の doc が綴りを定める） |
//! | `ColumnIndex` | `crates/schema-engine/src/compile/plan.rs`。マクロから見える形は `number` |
//! | `TypeKind` | `crates/schema-engine/src/types/mod.rs` の `TypeKind::ALL` を走査した合併型 |
//!
//! # なぜセル値の数値が `number` なのか（`bigint` を出さない理由）
//!
//! `ipc-contract.md` は境界へ 64 ビット整数を出さないと定める。マクロの境界では、
//! **そもそも 64 ビット整数が現れない** — `CellValue::Int` は設計表が 2^53 の壁を越えないと
//! 定めており（`crates/document-format/src/value.rs` の `Int` の doc）、値の写像は
//! `Int` / `Float` を 1 種類の JS の数値へ畳む（`host/value.rs` の「写像が保たないもの」）。
//! したがって [`CELL_VALUE_UNION`] は `number` を使う。**`ts-rs` の既定をそのまま使うと
//! `i64` は `bigint` になり、実行時に届く値（`number`）と食い違う** — これも、上流の型に
//! 導出を付けずに本モジュールで綴りを決めている理由の 1 つである。
//!
//! # 生成物はモジュールではない
//!
//! `import` も `export` も書かない。書くとファイルがモジュールになり、
//! `declare namespace host` がグローバルでなくなる（マクロはグローバルの `host` を呼び、
//! 型の名前をそのまま書ける必要がある）。フロントエンドの型検査
//! （`tsconfig.json` の `include` は `src`）には入らない — 取り込むのは後続の
//! `macro-editor-lsp` である（design.md「Components and Interfaces」の `types.rs`）。
//!
//! # 層の鎖（design.md「File Structure Plan」）
//!
//! `error / source → surface → host → engine → types → api`。本モジュールは鎖の 5 番目で
//! あり、`surface` と `host` を参照する。`api`（`src/api.rs`）が本モジュールを参照する側で
//! あり、本モジュールは `api` を知らない。

use core::fmt;

use schema_engine::TypeKind;

use crate::host::changes::CellWrite;
use crate::host::overlay::{ColumnTypeInfo, ReadRow, RowPage, RowSpan, SheetInfo};
// 種別の綴りの唯一の源は `host` 層にある（層の鎖: `engine` も `types` も参照できる）。
// ここは再輸出である（生成器と検査が同じ綴りを読むための名前）。
pub use crate::host::value::type_kind_name;
use crate::surface::declaration::{ApiDecl, HOST_APIS, HOST_NAMESPACE};

/// 生成物の位置（**リポジトリルートからの相対**。生成器と検査が同じ 1 つを読む）。
pub const GENERATED_PATH: &str = "types/macro-host.d.ts";

/// 生成物の再生成のコマンド（生成物の冒頭と、生成できないときの報告に出す）。
pub const REGENERATE_COMMAND: &str = "cargo run -p macro-runtime --bin generate-macro-types";

/// マクロから見えるセル値の綴り（`host/value.rs` の「マクロ向けの型定義」節が定める）。
///
/// 素の JS の値（`null` / `boolean` / `number` / `string`）と、構造として渡す値
/// （配列・オブジェクト）の合併型である。**自己記述的な形**（`{"$t":"text","v":"…"}`）も
/// オブジェクトであるため、この合併型で覆われる（要件 4.2, 4.3）。
pub const CELL_VALUE_UNION: &str =
    "null | boolean | number | string | CellValue[] | { [key: string]: CellValue }";

/// マクロから見える型 1 件の宣言（[`catalog`] の要素）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostTypeDecl {
    /// TypeScript の型名（宣言表の `params` / `returns` に書かれる名前と同じ）。
    pub name: String,
    /// **宣言の出どころ**（人が読む位置。`ファイル シンボル` の形）。
    ///
    /// 生成できないとき（型の宣言が無いとき）に、どこを直すかを言えるようにするために持つ。
    pub source: &'static str,
    /// JSDoc（`ts-rs` が Rust の doc コメントから作る。無ければ `None`）。
    pub docs: Option<String>,
    /// 宣言の本文（`type <名前> = …;`。**`export` を付けない**）。
    pub declaration: String,
}

/// マクロから見える型のカタログ（[`HostTypeDecl`] の並び。**名前の辞書順**）。
///
/// 「宣言表が要求する型」の集合ではなく、**公開しうる型の集合**である。`.d.ts` へ出るのは
/// 宣言表から推移的に要求された分だけであり（[`generate`]）、使われない型は並ばない。
pub fn catalog() -> Vec<HostTypeDecl> {
    let cfg = ts_rs::Config::default();
    let mut types = vec![
        derived::<SheetInfo>(&cfg, "crates/macro-runtime/src/host/overlay.rs SheetInfo"),
        derived::<ColumnTypeInfo>(
            &cfg,
            "crates/macro-runtime/src/host/overlay.rs ColumnTypeInfo",
        ),
        derived::<RowSpan>(&cfg, "crates/macro-runtime/src/host/overlay.rs RowSpan"),
        derived::<ReadRow>(&cfg, "crates/macro-runtime/src/host/overlay.rs ReadRow"),
        derived::<RowPage>(&cfg, "crates/macro-runtime/src/host/overlay.rs RowPage"),
        derived::<CellWrite>(&cfg, "crates/macro-runtime/src/host/changes.rs CellWrite"),
        type_kind_declaration(),
    ];
    types.extend(
        FIXED_ALIASES
            .iter()
            .map(|alias| alias.declaration())
            .collect::<Vec<_>>(),
    );
    types.sort_by(|a, b| a.name.cmp(&b.name));
    types
}

/// `ts-rs` の導出から 1 件作る。
///
/// 境界を `ts_rs::TS` にしてあるのは、**導出を持たない型をカタログへ書けない**ようにする
/// ためである（綴りを手で書いた型が紛れ込む余地を型で塞ぐ）。
fn derived<T: ts_rs::TS + 'static>(cfg: &ts_rs::Config, source: &'static str) -> HostTypeDecl {
    HostTypeDecl {
        name: <T as ts_rs::TS>::ident(cfg),
        source,
        docs: <T as ts_rs::TS>::docs(),
        declaration: <T as ts_rs::TS>::decl(cfg),
    }
}

/// 上流の型に `ts-rs` の導出を付けない（`ipc-contract.md`）ため、**マクロから見える綴り**を
/// ここで決める 1 件。
struct FixedAlias {
    /// TypeScript の型名。
    name: &'static str,
    /// 綴り（`type <名前> = <これ>;`）。
    body: &'static str,
    /// JSDoc の本文（`/**` と `*/` は [`FixedAlias::declaration`] ではなく組み立て側が付ける）。
    docs: &'static str,
    /// 出どころ。
    source: &'static str,
}

impl FixedAlias {
    fn declaration(&self) -> HostTypeDecl {
        HostTypeDecl {
            name: self.name.to_owned(),
            source: self.source,
            docs: Some(format!("/**\n{}\n */", indent_doc(self.docs))),
            declaration: format!("type {} = {};", self.name, self.body),
        }
    }
}

/// 上流の型の**マクロから見える綴り**（module docs「型の出どころ」）。
const FIXED_ALIASES: &[FixedAlias] = &[
    FixedAlias {
        name: "CellValue",
        body: CELL_VALUE_UNION,
        docs: "セルの値（マクロから見える形）。\n\n素の JS の値と、構造として渡す値（配列・オブジェクト）の合併型である。\n整数は 2^53 の内側に限られるため `number` で正確に表せる。",
        source: "crates/macro-runtime/src/host/value.rs CellValue（値の写像の綴り）",
    },
    FixedAlias {
        name: "ColumnIndex",
        body: "number",
        docs: "列の添字（0 起点。`host.columns` が返す並びの位置）。",
        source: "crates/schema-engine/src/compile/plan.rs ColumnIndex",
    },
    FixedAlias {
        name: "RowId",
        body: "string",
        docs: "行の識別子。`host.readRange` が返す識別子を、そのまま書き込みへ渡せる。",
        source: "crates/document-format/src/ids.rs RowId",
    },
    FixedAlias {
        name: "SheetId",
        body: "string",
        docs: "シートの識別子。`host.sheets` が返す識別子を、読み書きの要求へ渡す。",
        source: "crates/document-format/src/ids.rs SheetId",
    },
];

/// 種別の綴りの合併型（`schema-engine` の種別カタログを**走査して**作る。一覧を書き写さない）。
fn type_kind_declaration() -> HostTypeDecl {
    let union = TypeKind::ALL
        .iter()
        .map(|kind| format!("\"{}\"", type_kind_name(*kind)))
        .collect::<Vec<_>>()
        .join(" | ");
    HostTypeDecl {
        name: "TypeKind".to_owned(),
        source: "crates/schema-engine/src/types/mod.rs TypeKind::ALL",
        docs: Some(
            "/**\n * 列に宣言された型の種別（`host.columns` が返す `kind`）。\n *\n * 綴りは `schema-engine` の種別カタログ（`TypeKind::ALL`）の変種名である。\n * マクロへ種別を渡す口は、この綴りを作る `macro_runtime::host::value::type_kind_name` を使う\n * （`.d.ts` と実行時の値が同じ綴りになる）。\n */"
                .to_owned(),
        ),
        declaration: format!("type TypeKind = {union};"),
    }
}

/// `.d.ts` を組み立てる（**決定的**。同じ入力から常に同じバイト列を返す）。
///
/// 宣言表の API が要求する型の閉包を `.d.ts` へ出し、**宣言の無い型が 1 つでもあれば失敗する**
/// （型の無い API を公開しない。要件 10.1）。
pub fn generate() -> Result<String, Divergence> {
    generate_with(HOST_APIS)
}

/// 任意の宣言表から `.d.ts` を組み立てる（[`generate`] の本体。検査が合成の表で試す）。
fn generate_with(apis: &[ApiDecl]) -> Result<String, Divergence> {
    let catalog = catalog();
    let required = required_types(&catalog, apis);
    let undeclared = undeclared_types(&catalog, &required);
    if !undeclared.is_empty() {
        return Err(Divergence {
            apis_missing: Vec::new(),
            apis_extra: Vec::new(),
            types_undeclared: undeclared,
        });
    }
    let mut out = header();
    out.push_str(&api_section(apis));
    out.push_str(&type_section(&catalog, &required));
    Ok(out)
}

/// 宣言表と生成物を突き合わせる（tasks.md 3.3 の受け入れ）。
pub fn check() -> Result<(), Divergence> {
    check_with(HOST_APIS, &generate()?)
}

/// 任意の宣言表と任意の `.d.ts` を突き合わせる（検査の本体）。
///
/// 見るものは 3 つで、**どれも名前つきで報告する**:
///
/// 1. 宣言表にあるのに `.d.ts` に現れない API（実装漏れ。要件 10.3）
/// 2. `.d.ts` に現れるのに宣言表に無い API（公開していない実装）
/// 3. `.d.ts` の中で名前だけが現れ、宣言が無い型（型の無い API。要件 10.1）
///
/// 3 は**推移的に**見る: API の引数・戻り値の型から始め、その型の宣言の本文が参照する型へ
/// 辿る（`RowPage` が `ReadRow` を参照する、のように、直接書かれていない型も漏らさない）。
fn check_with(apis: &[ApiDecl], text: &str) -> Result<(), Divergence> {
    let declared = declared_apis(text);
    let apis_missing = apis
        .iter()
        .map(|api| api.js_name())
        .filter(|name| !declared.iter().any(|seen| seen == name))
        .collect::<Vec<_>>();
    let apis_extra = declared
        .iter()
        .filter(|name| !apis.iter().any(|api| &api.js_name() == *name))
        .cloned()
        .collect::<Vec<_>>();

    // 型は**本文から**辿る（カタログではなく生成物そのものを疑う）。
    let bodies = declared_types(text);
    let mut required = Vec::new();
    for api in apis {
        for name in api_type_names(api) {
            add_required(&mut required, name, &api.js_name());
        }
    }
    let mut index = 0;
    while index < required.len() {
        let name = required[index].name.clone();
        let requirers = required[index].required_by.clone();
        index += 1;
        let Some((_, body)) = bodies.iter().find(|(declared, _)| *declared == name) else {
            continue;
        };
        for referenced in type_names(body) {
            for requirer in &requirers {
                add_required(&mut required, referenced.clone(), requirer);
            }
        }
    }
    let types_undeclared = required
        .into_iter()
        .filter(|entry| !bodies.iter().any(|(declared, _)| *declared == entry.name))
        .map(|entry| UndeclaredType {
            name: entry.name,
            required_by: entry.required_by,
        })
        .collect::<Vec<_>>();

    let divergence = Divergence {
        apis_missing,
        apis_extra,
        types_undeclared,
    };
    if divergence.is_empty() {
        Ok(())
    } else {
        Err(divergence)
    }
}

/// 宣言表と `.d.ts` の食い違い（**名前つき**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// 宣言表にあるのに `.d.ts` に現れない API（`host.readRange` の形）。
    pub apis_missing: Vec<String>,
    /// `.d.ts` に現れるのに宣言表に無い API（`host.readRange` の形）。
    pub apis_extra: Vec<String>,
    /// 名前だけが現れ、宣言が無い型（要件 10.1）。
    pub types_undeclared: Vec<UndeclaredType>,
}

impl Divergence {
    /// 食い違いが 1 つも無いか。
    fn is_empty(&self) -> bool {
        self.apis_missing.is_empty()
            && self.apis_extra.is_empty()
            && self.types_undeclared.is_empty()
    }
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "宣言表と .d.ts が食い違う")?;
        if !self.apis_missing.is_empty() {
            write!(
                f,
                " / 宣言にあるのに .d.ts に無い API: [{}]",
                self.apis_missing.join(", ")
            )?;
        }
        if !self.apis_extra.is_empty() {
            write!(
                f,
                " / .d.ts にあるのに宣言に無い API: [{}]",
                self.apis_extra.join(", ")
            )?;
        }
        if !self.types_undeclared.is_empty() {
            let named = self
                .types_undeclared
                .iter()
                .map(|entry| format!("{}（要求元: {}）", entry.name, entry.required_by.join(", ")))
                .collect::<Vec<_>>()
                .join(", ");
            write!(f, " / 宣言の無い型: [{named}]")?;
        }
        Ok(())
    }
}

impl std::error::Error for Divergence {}

/// 宣言の無い型（[`Divergence::types_undeclared`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndeclaredType {
    /// 宣言が無い型の名前。
    pub name: String,
    /// その型を要求している API（`host.readRange` の形。推移的に要求する元まで遡る）。
    pub required_by: Vec<String>,
}

/// 要求された型 1 件（宣言表から推移的に辿ったもの）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RequiredType {
    name: String,
    /// この型を要求している API（`host.readRange` の形）。
    required_by: Vec<String>,
}

/// 宣言表が要求する型名の閉包（推移的に参照される型を含む。名前の辞書順）。
fn required_types(catalog: &[HostTypeDecl], apis: &[ApiDecl]) -> Vec<RequiredType> {
    let mut required: Vec<RequiredType> = Vec::new();
    for api in apis {
        for name in api_type_names(api) {
            add_required(&mut required, name, &api.js_name());
        }
    }
    let mut index = 0;
    while index < required.len() {
        let name = required[index].name.clone();
        let requirers = required[index].required_by.clone();
        index += 1;
        let Some(entry) = catalog.iter().find(|entry| entry.name == name) else {
            continue;
        };
        for referenced in type_names(&entry.declaration) {
            for requirer in &requirers {
                add_required(&mut required, referenced.clone(), requirer);
            }
        }
    }
    required.sort_by(|a, b| a.name.cmp(&b.name));
    required
}

/// 1 つの API の引数と戻り値に現れる型名。
fn api_type_names(api: &ApiDecl) -> Vec<String> {
    let mut names = Vec::new();
    for param in api.params {
        names.extend(type_names(param.ty));
    }
    names.extend(type_names(api.returns));
    names
}

/// 要求された型を加える（同じ名前は 1 件に畳み、要求元を足す）。
fn add_required(required: &mut Vec<RequiredType>, name: String, requirer: &str) {
    match required.iter_mut().find(|entry| entry.name == name) {
        Some(entry) => {
            if !entry.required_by.iter().any(|seen| seen == requirer) {
                entry.required_by.push(requirer.to_owned());
                entry.required_by.sort();
            }
        }
        None => required.push(RequiredType {
            name,
            required_by: vec![requirer.to_owned()],
        }),
    }
}

/// カタログに宣言が無い要求（生成を止める理由。要件 10.1）。
fn undeclared_types(catalog: &[HostTypeDecl], required: &[RequiredType]) -> Vec<UndeclaredType> {
    required
        .iter()
        .filter(|entry| !catalog.iter().any(|decl| decl.name == entry.name))
        .map(|entry| UndeclaredType {
            name: entry.name.clone(),
            required_by: entry.required_by.clone(),
        })
        .collect()
}

/// 生成物の冒頭（生成物であること・再生成のコマンド・モジュールにしない理由）。
fn header() -> String {
    format!(
        "// このファイルは生成物である。**手で編集しない。**\n\
         // ホスト API の関数の宣言は crates/macro-runtime/src/surface/declaration.rs の HOST_APIS が、\n\
         // 型の宣言は crates/macro-runtime/src/{{types.rs,host/overlay.rs,host/changes.rs}} の定義から\n\
         // ts-rs が生成する。直すのは生成元である。\n\
         //\n\
         // 再生成（リポジトリルートで実行する）: {REGENERATE_COMMAND}\n\
         // 本ファイルは追跡対象である。ドリフト検査（crates/macro-runtime/tests/macro_host_dts_drift.rs）が\n\
         // 生成器の出力とバイト比較する。\n\
         //\n\
         // **本ファイルはモジュールではない**（`import` も `export` も書かない）。書くとファイルが\n\
         // モジュールになり、`declare namespace host` がグローバルでなくなるためである。\n\
         // マクロはグローバルの `host` の下の API を呼び、型の名前をそのまま書ける必要がある。\n\
         // フロントエンドの型検査（tsconfig.json の `include` は `src`）には入らない — 取り込むのは\n\
         // 後続の macro-editor-lsp である。\n"
    )
}

/// ホスト API の節（宣言表の並びをそのまま保つ）。
fn api_section(apis: &[ApiDecl]) -> String {
    let mut out = String::new();
    out.push_str("\n/**\n");
    out.push_str(" * マクロから見えるホスト API（要件 4.1–4.6）。\n");
    out.push_str(" *\n");
    out.push_str(
        " * マクロのソースでは `host.readRange(sheet, span)` のように呼ぶ。**能力を要する\n",
    );
    out.push_str(
        " * API は、ソースの先頭でその能力を宣言していなければ呼べない**（要件 8.1–8.4）。\n",
    );
    out.push_str(" * 宣言は `// @grant file.read, net` の形で書く。\n");
    out.push_str(" */\n");
    out.push_str(&format!("declare namespace {HOST_NAMESPACE} {{\n"));
    for api in apis {
        out.push_str("  /**\n");
        match api.required_capability() {
            Some(capability) => out.push_str(&format!(
                "   * `{}` — 能力: `{}` を宣言したマクロだけが呼べる（`// @grant {}`）。\n",
                api.js_name(),
                capability.as_str(),
                capability.as_str()
            )),
            None => out.push_str(&format!(
                "   * `{}` — 能力: 不要（宣言なしで呼べる）。\n",
                api.js_name()
            )),
        }
        out.push_str("   */\n");
        out.push_str(&format!(
            "  function {}({}): {};\n",
            api.name,
            signature_params(api),
            api.returns
        ));
    }
    out.push_str("}\n");
    out
}

/// 引数の並び（`sheet: SheetId, span: RowSpan`。宣言表の並びをそのまま保つ）。
fn signature_params(api: &ApiDecl) -> String {
    api.params
        .iter()
        .map(|param| format!("{}: {}", param.name, param.ty))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 型の節（**宣言表が要求する型の閉包だけ**を名前の辞書順で並べる）。
fn type_section(catalog: &[HostTypeDecl], required: &[RequiredType]) -> String {
    let mut out = String::new();
    out.push_str(
        "\n// ---------------------------------------------------------------------------\n",
    );
    out.push_str("// マクロから見える型（宣言表が要求する型の閉包。出どころは crates/macro-runtime/src/types.rs）\n");
    for entry in required {
        let Some(declaration) = catalog.iter().find(|decl| decl.name == entry.name) else {
            continue;
        };
        out.push('\n');
        if let Some(docs) = &declaration.docs {
            out.push_str(docs);
            out.push('\n');
        }
        out.push_str(&declaration.declaration);
        out.push('\n');
    }
    out
}

/// JSDoc の本文を ` * ` で字下げする。
fn indent_doc(docs: &str) -> String {
    docs.lines()
        .map(|line| {
            if line.is_empty() {
                " *".to_owned()
            } else {
                format!(" * {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// 生成物を読む（構造で見る。`structure.md`「逸脱の検出は文字列ではなく構造で行う」）
// ---------------------------------------------------------------------------

/// `.d.ts` に現れる API の名前（`host.<名前>` の形。**`host` の名前空間の中だけ**を見る）。
fn declared_apis(text: &str) -> Vec<String> {
    let Some(body) = namespace_body(text, HOST_NAMESPACE) else {
        return Vec::new();
    };
    let code = without_comments_and_strings(&body);
    let mut out = Vec::new();
    let bytes = code.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !is_ident_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_ident_continue(bytes[index]) {
            index += 1;
        }
        if &code[start..index] != "function" {
            continue;
        }
        let cursor = skip_spaces(bytes, index);
        let Some(name) = take_identifier(&code, cursor) else {
            continue;
        };
        out.push(format!("{HOST_NAMESPACE}.{name}"));
        index = cursor + name.len();
    }
    out
}

/// `.d.ts` に宣言されている型（`type <名前> = …;` の名前と本文）。
fn declared_types(text: &str) -> Vec<(String, String)> {
    let code = without_comments_and_strings(text);
    let bytes = code.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !is_ident_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_ident_continue(bytes[index]) {
            index += 1;
        }
        if &code[start..index] != "type" {
            continue;
        }
        // `type <名前> = …;` でなければ、その語は型名ではない（`type: string` のような欄名）。
        let cursor = skip_spaces(bytes, index);
        let Some(name) = take_identifier(&code, cursor) else {
            continue;
        };
        let after_name = skip_spaces(bytes, cursor + name.len());
        if after_name >= bytes.len() || bytes[after_name] != b'=' {
            continue;
        }
        let body_start = after_name + 1;
        let (body_end, next) = statement_end(bytes, body_start);
        out.push((name, code[body_start..body_end].trim().to_owned()));
        index = next;
    }
    out
}

/// `declare namespace <名前> { … }` の内側（無ければ `None`）。
fn namespace_body(text: &str, namespace: &str) -> Option<String> {
    let code = without_comments_and_strings(text);
    let bytes = code.as_bytes();
    let needle = format!("namespace {namespace}");
    let found = code.find(&needle)?;
    let after = found + needle.len();
    let open = skip_spaces(bytes, after);
    if open >= bytes.len() || bytes[open] != b'{' {
        return None;
    }
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        match bytes[index] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(code[open + 1..index].to_owned());
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// 型式に現れる**宣言を要する**名前（TS の組込とキーワード、欄名、メンバー参照を除く）。
fn type_names(expression: &str) -> Vec<String> {
    let code = without_comments_and_strings(expression);
    let bytes = code.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !is_ident_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_ident_continue(bytes[index]) {
            index += 1;
        }
        let name = &code[start..index];
        let member = start > 0 && bytes[start - 1] == b'.';
        let field = is_field_name(bytes, index);
        if !member
            && !field
            && !NOT_A_TYPE_NAME.contains(&name)
            && !out.iter().any(|seen| seen == name)
        {
            out.push(name.to_owned());
        }
    }
    out
}

/// 直後が `:`（または `?:`）なら、その名前はオブジェクト型の**欄名**である。
fn is_field_name(bytes: &[u8], from: usize) -> bool {
    let cursor = skip_spaces(bytes, from);
    if cursor < bytes.len() && bytes[cursor] == b':' {
        return true;
    }
    if cursor < bytes.len() && bytes[cursor] == b'?' {
        let after = skip_spaces(bytes, cursor + 1);
        return after < bytes.len() && bytes[after] == b':';
    }
    false
}

/// コメントと文字列リテラルを落とす（**構造を見る前の下ごしらえ**）。
///
/// JSDoc の中の日本語や、`"Int"` のような文字列リテラルの中身を型名と誤認すると、検査が
/// 説明文で誤検出する。文字列リテラルは `""` へ置き換え、引用符の状態を追う
/// （`//` を含む文字列をコメントと誤認しない）。
fn without_comments_and_strings(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '/' if chars.peek() == Some(&'/') => {
                while let Some(next) = chars.next() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                while let Some(next) = chars.next() {
                    if next == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                    if next == '\n' {
                        out.push('\n');
                    }
                }
            }
            '"' | '\'' | '`' => {
                out.push_str("\"\"");
                let quote = character;
                while let Some(next) = chars.next() {
                    if next == '\\' {
                        chars.next();
                        continue;
                    }
                    if next == quote {
                        break;
                    }
                }
            }
            _ => out.push(character),
        }
    }
    out
}

/// `=` の後ろから、**深さ 0 の `;`** までを 1 つの宣言の本文として取り出す。
fn statement_end(bytes: &[u8], from: usize) -> (usize, usize) {
    let mut depth = 0i32;
    let mut index = from;
    while index < bytes.len() {
        match bytes[index] {
            b'{' | b'[' | b'(' => depth += 1,
            b'}' | b']' | b')' => depth -= 1,
            b';' if depth == 0 => return (index, index + 1),
            _ => {}
        }
        index += 1;
    }
    (bytes.len(), bytes.len())
}

fn skip_spaces(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && (bytes[index] as char).is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn take_identifier(code: &str, from: usize) -> Option<String> {
    let bytes = code.as_bytes();
    if from >= bytes.len() || !is_ident_start(bytes[from]) {
        return None;
    }
    let mut index = from;
    while index < bytes.len() && is_ident_continue(bytes[index]) {
        index += 1;
    }
    Some(code[from..index].to_owned())
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

/// 型名として宣言を要さない名前（TS のキーワードと組込型）。
///
/// ここに挙げた名前は `.d.ts` に宣言が無くてもよい（`Array<SheetInfo>` の `Array` など）。
const NOT_A_TYPE_NAME: &[&str] = &[
    "Array",
    "Date",
    "Map",
    "Promise",
    "ReadonlyArray",
    "Record",
    "Set",
    "any",
    "as",
    "bigint",
    "boolean",
    "const",
    "declare",
    "export",
    "extends",
    "false",
    "function",
    "in",
    "interface",
    "keyof",
    "namespace",
    "never",
    "new",
    "null",
    "number",
    "object",
    "readonly",
    "string",
    "symbol",
    "true",
    "type",
    "typeof",
    "undefined",
    "unknown",
    "void",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::capability::Capability;
    use crate::surface::declaration::{ApiCapability, ParamDecl};

    /// 生成物（**モジュールの外の検査も同じものを使う**）。
    fn generated() -> String {
        generate().expect("宣言表の型はすべて宣言を持つ（`catalog` を参照）")
    }

    /// 宣言表が要求する型名を**名前で固定する**（洩れなく・推移的に）。
    ///
    /// 直接書かれた 8 名（`surface/declaration.rs` の doc が挙げる名前）に加え、
    /// `RowPage.rows` → `ReadRow`、`ColumnTypeInfo.kind` → `TypeKind`、
    /// `CellWrite.column` → `ColumnIndex` が推移的に要る。
    #[test]
    fn the_table_requires_exactly_these_type_names() {
        let names = required_types(&catalog(), HOST_APIS)
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "CellValue",
                "CellWrite",
                "ColumnIndex",
                "ColumnTypeInfo",
                "ReadRow",
                "RowId",
                "RowPage",
                "RowSpan",
                "SheetId",
                "SheetInfo",
                "TypeKind",
            ],
            "宣言表が要求する型の集合が変わった（宣言表か型の定義を見直すこと）"
        );
    }

    /// 宣言表の全 API が、**引数と戻り値の型まで**生成物に現れる（要件 4.6, 10.1, 10.3）。
    #[test]
    fn every_api_appears_with_the_declared_parameter_and_return_types() {
        let text = generated();
        for api in HOST_APIS {
            let expected = format!(
                "  function {}({}): {};\n",
                api.name,
                signature_params(api),
                api.returns
            );
            assert!(
                text.contains(&expected),
                "生成物に `{}` が無い",
                expected.trim_end()
            );
        }
        assert_eq!(
            HOST_APIS.len(),
            declared_apis(&text).len(),
            "生成物の API の本数が宣言表と一致しない"
        );
        assert!(
            check().is_ok(),
            "生成物は宣言表と一致するはず: {:?}",
            check()
        );
    }

    /// 能力を要する API は宣言のコメントに能力名を持ち、能力不要の API は持たない
    /// （要件 8.2 の提示と `.d.ts` を一致させる。tasks.md 3.3）。
    #[test]
    fn the_comment_of_each_api_names_its_capability() {
        let text = generated();
        for api in HOST_APIS {
            let expected = match api.required_capability() {
                Some(capability) => format!(
                    "`{}` — 能力: `{}` を宣言したマクロだけが呼べる",
                    api.js_name(),
                    capability.as_str()
                ),
                None => format!("`{}` — 能力: 不要", api.js_name()),
            };
            assert!(text.contains(&expected), "生成物に `{expected}` が無い");
        }
        // 能力の綴りが実行時の提示（`Capability::as_str`）と同じであること。
        assert!(text.contains(Capability::FileRead.as_str()));
        assert!(text.contains(Capability::FileWrite.as_str()));
        assert!(text.contains(Capability::Net.as_str()));
    }

    /// 乖離を**名前つきで**両方向へ検出する（tasks.md 3.3 の受け入れ）。
    #[test]
    fn a_missing_api_and_an_undeclared_api_are_reported_by_name() {
        let text = generated();

        // 宣言表にあるのに生成物に無い（`host.readRange` の宣言を改名する）。
        let renamed = text.replace("  function readRange(", "  function readRangeX(");
        let divergence = check_with(HOST_APIS, &renamed).expect_err("欠けを見落とした");
        assert_eq!(divergence.apis_missing, vec!["host.readRange".to_owned()]);
        assert_eq!(divergence.apis_extra, vec!["host.readRangeX".to_owned()]);
        assert!(divergence.types_undeclared.is_empty());

        // 生成物にあるのに宣言表に無い（表に無い API を 1 本足す）。
        let extra = text.replace(
            &format!("declare namespace {HOST_NAMESPACE} {{"),
            &format!(
                "declare namespace {HOST_NAMESPACE} {{\n  /** `host.deleteEverything` — 能力: 不要。 */\n  function deleteEverything(): void;"
            ),
        );
        let divergence = check_with(HOST_APIS, &extra).expect_err("余りを見落とした");
        assert_eq!(
            divergence.apis_extra,
            vec!["host.deleteEverything".to_owned()]
        );
        assert!(divergence.apis_missing.is_empty());

        // 名前空間の外の `function` は API ではない（`host` の内側だけを見る）。
        let outside = format!("{text}\nfunction helper(): void;\n");
        assert!(
            check_with(HOST_APIS, &outside).is_ok(),
            "名前空間の外を API と数えた"
        );
    }

    /// **型の無い API は公開できない**（要件 10.1。design.md の Risks「型の無い API を足すと
    /// 生成が失敗する」）。
    #[test]
    fn an_api_whose_type_has_no_declaration_fails_generation() {
        const PARAMS: &[ParamDecl] = &[ParamDecl {
            name: "sheet",
            ty: "NoSuchType",
        }];
        const SYNTHETIC: &[ApiDecl] = &[ApiDecl {
            name: "typo",
            params: PARAMS,
            returns: "void",
            capability: ApiCapability::NotRequired,
        }];

        let divergence = generate_with(SYNTHETIC).expect_err("宣言の無い型で生成できてしまった");
        assert_eq!(
            divergence.types_undeclared,
            vec![UndeclaredType {
                name: "NoSuchType".to_owned(),
                required_by: vec!["host.typo".to_owned()],
            }]
        );
        assert!(divergence.apis_missing.is_empty());
        assert!(divergence.apis_extra.is_empty());
        // 報告は名前を含む（人がそのまま直せる）。
        assert!(divergence.to_string().contains("NoSuchType"));
        assert!(divergence.to_string().contains("host.typo"));
    }

    /// 推移的に参照される型の宣言が欠けていても、**名前つきで**報告する
    /// （`RowPage` が参照する `ReadRow` を落とした場合）。
    #[test]
    fn a_type_referenced_by_another_declaration_is_reported_by_name() {
        let text = generated();
        let missing_read_row = text.replace("type ReadRow = {", "type ReadRowX = {");
        assert_ne!(text, missing_read_row, "`ReadRow` の宣言が見つからない");

        let divergence = check_with(HOST_APIS, &missing_read_row).expect_err("欠けを見落とした");
        assert_eq!(
            divergence.types_undeclared,
            vec![UndeclaredType {
                name: "ReadRow".to_owned(),
                required_by: vec!["host.readRange".to_owned()],
            }]
        );
        assert!(divergence.apis_missing.is_empty());
        assert!(divergence.apis_extra.is_empty());
    }

    /// 要求された型はすべてカタログにあり、**出どころ**を持つ（どこを直すか言える）。
    #[test]
    fn every_required_type_has_a_declaration_and_a_source() {
        let catalog = catalog();
        for required in required_types(&catalog, HOST_APIS) {
            let entry = catalog
                .iter()
                .find(|entry| entry.name == required.name)
                .unwrap_or_else(|| panic!("型 {} の宣言がカタログに無い", required.name));
            assert!(
                !entry.source.is_empty(),
                "型 {} の出どころが空",
                required.name
            );
            assert!(
                entry
                    .declaration
                    .starts_with(&format!("type {} = ", entry.name)),
                "型 {} の宣言が名前から始まらない: {}",
                entry.name,
                entry.declaration
            );
            assert!(
                !entry.declaration.contains("export"),
                "型 {} の宣言に `export` がある（モジュールになってしまう）",
                entry.name
            );
        }
    }

    /// 種別の合併型は `schema-engine` の種別カタログを走査して作る（**一覧を書き写さない**）。
    #[test]
    fn the_type_kind_union_is_scanned_from_the_kind_catalog() {
        let text = generated();
        let expected = TypeKind::ALL
            .iter()
            .map(|kind| format!("\"{}\"", type_kind_name(*kind)))
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            text.contains(&format!("type TypeKind = {expected};")),
            "種別の合併型が種別カタログから作られていない"
        );

        let mut names = TypeKind::ALL
            .iter()
            .map(|kind| type_kind_name(*kind))
            .collect::<Vec<_>>();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(count, names.len(), "2 つの種別が同じ綴りへ潰れている");
    }

    /// 種別の綴りは**変種名そのもの**である（`app-shell` の `TypeKindTag` と同じ綴りであり、
    /// `src-tauri` の検査が `format!("{kind:?}")` で両者を突き合わせる）。
    ///
    /// 綴りの `match` は手で書かれており（`host/value.rs` の `type_kind_name`）、
    /// **網羅性は rustc が守るが、綴りの正しさは守らない**。1 つでもずれればこの検査が落ちる
    /// — `TypeKind::ALL` を走査した合併型と、実行時の札が食い違う状態を作らない。
    #[test]
    fn the_kind_spelling_is_the_variant_name() {
        for kind in TypeKind::ALL {
            assert_eq!(
                format!("{kind:?}"),
                type_kind_name(kind),
                "種別の綴りが変種名と一致しない"
            );
        }
    }

    /// 生成物は**モジュールではない**（`import` / `export` を書かない）。
    ///
    /// 書くと `declare namespace host` がグローバルでなくなり、マクロから `host` が見えなく
    /// なる（`types.rs` の module docs）。
    #[test]
    fn the_generated_file_stays_a_global_script() {
        let text = generated();
        for line in text.lines() {
            assert!(
                !line.starts_with("export ") && !line.starts_with("import "),
                "`import` / `export` があると `host` がグローバルでなくなる: {line}"
            );
        }
        assert!(text.contains(&format!("declare namespace {HOST_NAMESPACE} {{")));
    }

    /// 生成物は決定的である（同じ入力から同じバイト列。ドリフト検査の前提）。
    #[test]
    fn generation_is_deterministic() {
        assert_eq!(generated(), generated());
    }

    /// 宣言表の `params` / `returns` の型名は、**カタログの名前か TS の組込**である
    /// （宣言表とカタログの綴りがずれていない）。
    #[test]
    fn the_table_uses_only_declared_names_or_builtins() {
        let catalog = catalog();
        for api in HOST_APIS {
            for name in api_type_names(api) {
                assert!(
                    catalog.iter().any(|entry| entry.name == name),
                    "宣言表の型名 `{name}` がカタログに無い"
                );
            }
        }
    }
}

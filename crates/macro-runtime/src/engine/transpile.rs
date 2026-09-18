//! TypeScript → JavaScript の変換とソースマップ（tasks.md 3.1。要件 3.1–3.5, 9.1）。
//!
//! `deno_core` は純粋な JS エンジンであり **TypeScript を素では実行できない**（`brief.md`）。
//! したがって実行の直前に 1 回だけ変換し、**型注釈を落とした JavaScript** と、生成位置から
//! 原位置（TypeScript の行・列）へ写す**ソースマップ**を作る。**結線するのはタスク 3.2**
//! （`engine/isolate.rs` の `ModuleLoader::load`）であり、本モジュールはその 1 歩だけを持つ。
//!
//! # 実測に基づく形（`deno_ast` 0.53.3 の `transpiling`）
//!
//! | 段 | 使う口 | 根拠 |
//! |----|--------|------|
//! | 解析 | [`deno_ast::parse_module`]（`ParseParams`） | 構文の誤りを**行・列つきの**診断として返す（要件 3.4） |
//! | 変換 | `ParsedSource::transpile`（`TranspileOptions` / `TranspileModuleOptions` / `EmitOptions`） | `MediaType::TypeScript` で型注釈を除去する（要件 3.1） |
//! | 写し | `EmitOptions { source_map: `[`SourceMapOption::Separate`]` }` → `SourceMap::to_data_url` | 変換結果へ `sourceMappingURL` を付け、`deno_core` の `SourceMapper` が原位置へ写す |
//!
//! [`SourceMapOption::Inline`] を使えば `deno_ast` が自分でインラインの `sourceMappingURL` を
//! 書くが、その場合**写しそのもの（JSON）は手に入らない**（`EmittedSourceText.source_map` が
//! `None` になる。`emit.rs` の実測）。タスク 3.2 が写しを実行基盤へ渡せるよう、ここでは
//! `Separate` で JSON を受け取り、**`deno_core` が復号に使うのと同じ `sourcemap` の実装**で
//! データ URL へ符号化して末尾に付ける。付ける形（元が改行で終わらなければ `\n` を足してから
//! `//# sourceMappingURL=` + データ URL）は `deno_ast` の `Inline` の分岐と同じである
//! （`emit.rs` の実測）。データ URL は `data:application/json;charset=utf-8;base64,` で始まり、
//! `deno_core` の `source_map.rs` の `decode_data_url` がこの綴りを受け付ける（`;charset=` は
//! 任意であり、付けても付けなくても読める）。**テストはこの 2 つの経路（JSON の欄と、コード
//! 末尾のデータ URL）の両方を `deno_core` と同じ手順で開いて、同じ原位置を指すことを固定する。**
//!
//! モジュール名は設計の例（`macro:在庫集計.ts`）に従い、`macro:` の scheme と種別から決まる
//! 拡張子を持つ（[`module_name`]）。ローダー（3.2）もこの関数を唯一の源として使う。
//! **名前は URL として正規化した形で返す** — `url` は非 ASCII を必ず `%XX` へ符号化するため、
//! 名前が `在庫集計` の TypeScript のモジュール名は
//! `macro:%E5%9C%A8%E5%BA%AB%E9%9B%86%E8%A8%88.ts` になる（実測。空白は符号化されずそのまま
//! 残る。設計の例の綴り `macro:在庫集計.ts` は**利用者が読む名前**であり、それは
//! [`MacroName`](crate::source::record::MacroName) が持つ）。
//! 正規化した 1 つの綴りを返すことで、V8 が報告するモジュール名・写しの `sources`・ローダーが
//! 引く名前が揃う。したがって `SourceMapper` は**ファイル名を書き換えずに行と列だけ**を写し
//! （`SourceMapApplication::LineAndColumn`）、3.2 は [`MacroFailure`] の [`Frame`] に
//! マクロ名を入れればよい。
//!
//! # 型の誤りでは止めない（要件 3.2）/ 構文の誤りは位置つきで返す（要件 3.4）
//!
//! `deno_ast` の変換は**型の検査をしない**（swc の TypeScript 変換は型注釈を構文的に落とす
//! だけである）。したがって `const n: number = "文字列"` や未定義の型名は**そのまま通る** —
//! 型の検査はエディタを所有する機能の役割であり（要件 3.2）、ここで止めてはならない。
//! 一方、**構文**が壊れていれば `parse_module` が診断を返すので、その**行と列**（および理由）を
//! [`MacroFailure`] の [`Frame`] に入れて返す（要件 3.4）。
//!
//! **種別が JavaScript のソースは変換しない**（要件 3.3）。ただし**解析は行う** — 構文の誤りを
//! 種別によらず同じ形（[`FailureKind::Transpile`] と位置）で返すためであり、**返すコードは
//! 入力そのもの（バイト単位で同一）**である。解析器は `deno_ast` が種別に割り当てる JavaScript
//! の構文であり、デコレータ・import attributes・auto accessors・explicit resource management を
//! 許す（`parsing.rs` の `get_syntax` の実測）ため、V8 が受ける現代の JavaScript をここで
//! 落とすことはない。判断を分けたくない場合の代償は「宣言だけの綴りをここで確かめる」ことで
//! あり、**ソースを書き換えないこと**は変わらない。
//!
//! # 何をここに置かないか
//!
//! - **取り込みの解決**（要件 3.5）: 変換は `import` の並びを**そのまま残す**（`var_decl_imports`
//!   は既定の `false`。型だけの取り込みは `imports_not_used_as_values: Remove` で消えるため、
//!   型のために書いた取り込みが実行時に名前を要求することはない）。どの名前が解決できるかを
//!   知っているのは `ModuleLoader` であり、**解決できない名前を挙げて失敗させるのはタスク 3.2
//!   のローダー**である。ここで先回りして落とすと、後で標準ライブラリ（`macro-stdlib`）が
//!   足す名前を二重に管理することになる
//! - **キャッシュ**: 変換は実行の直前に 1 回だけ行う（design.md「Transpiler」の Integration）。
//!   実行ごとに isolate を作り直す（design.md 決定 1）ため、変換結果を持ち越す相手がいない
//! - **trait**: 設計の Service Interface は `TranspilePort` を挙げるが、実装は 1 つしかなく、
//!   アダプタが差し込む縫い目（`HostPort`）でもないため trait にしていない。2 つ目の実装が
//!   現れた時点で起こす
//!
//! # 観測（tasks.md 3.1 の受け入れ。テストとして固定してある）
//!
//! | 観測 | テスト |
//! |------|--------|
//! | 型注釈・`as`・generics を落とした JavaScript を返し、それが JavaScript として解釈できる | `tests::型注釈とasとgenericsを落としたjavascriptを返す` |
//! | **型の誤りでは止まらない**（要件 3.2） | `tests::型の誤りでは止まらない` |
//! | TypeScript の構文の誤りは**行と列つき**の [`FailureKind::Transpile`] になる（要件 3.4） | `tests::構文の誤りは行と列つきで返る` |
//! | JavaScript のソースは**バイト単位でそのまま**返る（要件 3.3） | `tests::javascriptのソースはそのまま返る` |
//! | JavaScript の構文の誤りも行と列つきで返る（要件 3.4） | `tests::javascriptの構文の誤りも行と列つきで返る` |
//! | ソースマップが**生成位置を原位置へ写せる**（JSON とデータ URL の両方の経路で） | `tests::ソースマップは生成位置を原位置へ写せる` |
//! | モジュール名がマクロ名と種別から決まり、URL として往復する | `tests::モジュール名はマクロ名と種別から決まる` |

use std::sync::Arc;

use deno_ast::diagnostics::Diagnostic;
use deno_ast::{
    parse_module, EmitOptions, MediaType, ModuleSpecifier, ParseDiagnostic, ParseParams,
    SourceMapOption, TranspileModuleOptions, TranspileOptions,
};

use crate::engine::outcome::{FailureKind, Frame, MacroFailure};
use crate::source::record::{MacroKind, MacroRecord};

/// 変換結果へ付ける `sourceMappingURL` の前置き（`deno_ast` の `Inline` と同じ綴り）。
const SOURCE_MAPPING_PREFIX: &str = "//# sourceMappingURL=";

/// マクロのモジュールを表す scheme（design.md「Transpiler」の例 `macro:在庫集計.ts`）。
///
/// **この scheme の名前だけ**がマクロのモジュールである。ローダー（タスク 3.2）はこの scheme
/// の名前を解決し、それ以外は解決しない（design.md「Transpiler」の Integration）。
pub const MODULE_SCHEME: &str = "macro:";

/// 変換の結果（design.md「Transpiler」の Service Interface の `Transpiled`）。
///
/// [`Transpiled::code`] は**末尾に `sourceMappingURL` を持つ** JavaScript であり、そのまま
/// モジュールとしてコンパイルできる。[`Transpiled::source_map`] は同じ写しの JSON、すなわち
/// [`Transpiled::code`] の末尾のデータ URL が運ぶものと同じ写しであり、`deno_core` の
/// `SourceMapData`（`SourceMap::from_slice` が読む生のバイト列）として渡せる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transpiled {
    /// 変換後の JavaScript（末尾に `sourceMappingURL`）。
    pub code: String,
    /// 生成位置 → 原位置の写し（JSON）。**種別が JavaScript のときは `None`**（位置が動かない）。
    pub source_map: Option<String>,
    /// モジュールの名前（URL として正規化した綴り。例: 名前 `在庫集計` の TypeScript は
    /// `macro:%E5%9C%A8%E5%BA%AB%E9%9B%86%E8%A8%88.ts`。上のモジュールの説明を参照）。
    pub module_name: String,
}

/// TypeScript → JavaScript の変換器（状態を持たない）。
///
/// 実行の直前に 1 回だけ [`Transpiler::transpile`] を呼ぶ。タスク 3.2 のローダーが
/// `ModuleLoader::load` の中でこれを持つ。
#[derive(Debug, Clone, Copy, Default)]
pub struct Transpiler;

impl Transpiler {
    /// 変換器を作る。
    pub const fn new() -> Self {
        Self
    }

    /// 記録 1 件を実行できる JavaScript へ変換する（tasks.md 3.1 の受け入れ）。
    ///
    /// 種別が [`MacroKind::JavaScript`] のときは**解析だけを行い、ソースをバイト単位で
    /// そのまま返す**（要件 3.3。[`Transpiled::source_map`] は `None`）。種別が
    /// [`MacroKind::TypeScript`] のときは型注釈を落とし、写しを付ける（要件 3.1）。
    ///
    /// 失敗はすべて [`FailureKind::Transpile`] の [`MacroFailure`] になる。**記録は書き換えない**
    /// （保存されたままのテキストを保つ。要件 1.5）。
    pub fn transpile(&self, record: &MacroRecord) -> Result<Transpiled, MacroFailure> {
        let module_name = module_name(record);
        // 名前は利用者が決めるため、URL として読めない綴り（`macro://:` のように authority の
        // 解釈が壊れるもの）でも panic させない。読めなければ変換できない理由として返す。
        let specifier = ModuleSpecifier::parse(&module_name).map_err(|error| {
            MacroFailure::new(
                FailureKind::Transpile,
                format!("モジュール名として使えない {module_name}: {error}"),
                Vec::new(),
            )
        })?;

        let parsed = parse_module(ParseParams {
            specifier,
            text: Arc::from(record.source.as_str()),
            media_type: media_type(record.kind),
            // 変換に必要なのは構文木とコメントだけである（字句とスコープ解析は使わない）。
            capture_tokens: false,
            scope_analysis: false,
            maybe_syntax: None,
        })
        .map_err(|diagnostic| syntax_failure(record, &diagnostic))?;

        if record.kind == MacroKind::JavaScript {
            // **変換しない**（要件 3.3）。解析は構文を確かめるために済ませてあり、返すのは
            // 保存されたままのソースである。位置が動かないので写しも要らない。
            return Ok(Transpiled {
                code: record.source.clone(),
                source_map: None,
                module_name,
            });
        }

        let emitted = parsed
            .transpile(
                &transpile_options(),
                &TranspileModuleOptions::default(),
                &EmitOptions {
                    source_map: SourceMapOption::Separate,
                    source_map_file: Some(module_name.clone()),
                    inline_sources: true,
                    remove_comments: false,
                    ..EmitOptions::default()
                },
            )
            .map_err(|error| {
                MacroFailure::new(
                    FailureKind::Transpile,
                    format!("{module_name} を変換できない: {error}"),
                    Vec::new(),
                )
            })?
            .into_source();

        // `Separate` は必ず写しを返す。返らなければ変換の道具立てが壊れているので、黙って
        // 写し無しのコードを渡さない（位置が原位置を指さなくなる）— 理由を返して止める。
        let source_map = emitted.source_map.ok_or_else(|| {
            MacroFailure::new(
                FailureKind::Transpile,
                format!("{module_name} の写し（source map）が出力されない"),
                Vec::new(),
            )
        })?;
        let code = with_source_mapping_url(emitted.text, &source_map, &module_name)?;

        Ok(Transpiled {
            code,
            source_map: Some(source_map),
            module_name,
        })
    }
}

/// モジュールの名前（design.md「Transpiler」の例 `macro:在庫集計.ts`）。
///
/// **ローダー（タスク 3.2）と本モジュールが同じ名前を使うための唯一の源**である。拡張子を
/// 種別から決めるのは、名前を見ただけで種別が判別できるようにするためである。
///
/// 返すのは **URL として正規化した形**である（空白は `%20`、例: `macro:棚%20卸し.js`）。
/// こうすると、この名前・V8 が報告するモジュール名・写しの `sources` が同じ 1 つの綴りに
/// 揃い、`SourceMapper` がファイル名を書き換えずに**行と列だけ**を写す（3.2 がマクロ名を
/// [`Frame`] へ入れる邪魔をしない）。読み取れない綴りは正規化できないため生のまま返し、
/// [`Transpiler::transpile`] が**理由つきで**断る（黙って別の名前に化けさせない）。
pub fn module_name(record: &MacroRecord) -> String {
    let raw = format!(
        "{MODULE_SCHEME}{}.{}",
        record.name,
        match record.kind {
            MacroKind::TypeScript => "ts",
            MacroKind::JavaScript => "js",
        }
    );
    match ModuleSpecifier::parse(&raw) {
        Ok(specifier) => specifier.as_str().to_owned(),
        Err(_) => raw,
    }
}

/// 種別に対応する `deno_ast` のメディア種別。
fn media_type(kind: MacroKind) -> MediaType {
    match kind {
        MacroKind::TypeScript => MediaType::TypeScript,
        MacroKind::JavaScript => MediaType::JavaScript,
    }
}

/// 変換の選択（**型の検査をしない**のが要件 3.2 の要求である）。
///
/// 既定から動かすのは `jsx` だけである。既定は `Some(Classic)` であり `.ts` では使われない
/// ものの、マクロは JSX を書く場所ではないため**変換しない**ことを明示する（`None`）。
/// 他の既定は要件どおりである: `verbatim_module_syntax: false` ＝ 型の注釈をすべて落とす、
/// `imports_not_used_as_values: Remove` ＝ 型だけの取り込みを消す、`var_decl_imports: false`
/// ＝ `import` の並びはローダーが解決する形のまま残す、`decorators: None` ＝ デコレータは
/// そのまま残す（V8 が解釈する）。
fn transpile_options() -> TranspileOptions {
    TranspileOptions {
        jsx: None,
        ..TranspileOptions::default()
    }
}

/// 構文の誤りを、**位置つき**の失敗へ写す（要件 3.4）。
///
/// 位置は `deno_ast` の診断の表示位置をそのまま使う（1 起点。`Frame` の数え方と同じである）。
/// 関数名は無い（構文の誤りは呼び出しの並びを持たない）ため `None` にする。
fn syntax_failure(record: &MacroRecord, diagnostic: &ParseDiagnostic) -> MacroFailure {
    let position = diagnostic.display_position();
    MacroFailure::new(
        FailureKind::Transpile,
        format!("構文の誤り: {}", diagnostic.message()),
        vec![Frame::at(
            record.name.clone(),
            None,
            position.line_number as u32,
            position.column_number as u32,
        )],
    )
}

/// 変換結果の末尾へ**インラインの** `sourceMappingURL` を付ける（design.md「Transpiler」）。
///
/// `deno_core` の `SourceMapper` は、モジュールをコンパイルしたときに V8 が報告した
/// `sourceMappingURL` を覚え、例外の位置をそこから復号した写しで置き換える
/// （`source_map.rs` の `decode_source_map` の実測）。したがって写しは**外部ファイルではなく
/// データ URL** としてコードの中へ入れる — 実行基盤が写しを読みに行く経路を作らずに済み、
/// `Transpiled::source_map` を別に渡す必要もない（渡せるようにもしてある）。
///
/// 符号化には `deno_core` が復号に使うのと同じ `sourcemap` の実装（`SourceMap::to_data_url`）を
/// 使う。ここを通すことで、写しが**読める形であること**も同時に確かめられる。
fn with_source_mapping_url(
    text: String,
    source_map: &str,
    module_name: &str,
) -> Result<String, MacroFailure> {
    let decoded =
        deno_core::sourcemap::SourceMap::from_slice(source_map.as_bytes()).map_err(|error| {
            MacroFailure::new(
                FailureKind::Transpile,
                format!("{module_name} の写し（source map）を読めない: {error}"),
                Vec::new(),
            )
        })?;
    let data_url = decoded.to_data_url().map_err(|error| {
        MacroFailure::new(
            FailureKind::Transpile,
            format!("{module_name} の写し（source map）を符号化できない: {error}"),
            Vec::new(),
        )
    })?;

    let mut code = text;
    if !code.ends_with('\n') {
        code.push('\n');
    }
    code.push_str(SOURCE_MAPPING_PREFIX);
    code.push_str(&data_url);
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::source::record::MacroName;

    /// 写しの欄（3.2 がローダーから渡す経路）を、`deno_core` と同じ手順で開く。
    fn open_map_field(transpiled: &Transpiled) -> deno_core::sourcemap::SourceMap {
        let json = transpiled
            .source_map
            .as_ref()
            .expect("TypeScript の変換は写しの欄を持つ");
        deno_core::sourcemap::SourceMap::from_slice(json.as_bytes()).expect("写しが読める")
    }

    /// コード末尾のデータ URL（3.2 が V8 に読ませる経路）を、`deno_core` と同じ手順で開く。
    fn open_inline_map(transpiled: &Transpiled) -> deno_core::sourcemap::SourceMap {
        let data_url = transpiled
            .code
            .split_once(SOURCE_MAPPING_PREFIX)
            .map(|(_, url)| url)
            .expect("末尾に sourceMappingURL がある");
        match deno_core::sourcemap::decode_data_url(data_url).expect("データ URL が読める") {
            deno_core::sourcemap::DecodedMap::Regular(map) => map,
            _ => panic!("通常の写しとして読めない"),
        }
    }

    fn typescript(source: &str) -> MacroRecord {
        MacroRecord::new(MacroName::new("在庫集計"), MacroKind::TypeScript, source)
    }

    #[test]
    fn 型注釈とasとgenericsを落としたjavascriptを返す() {
        let record = typescript(
            "interface 行 { 数量: number }\n\
             import type { Sheet } from \"./t.ts\";\n\
             const 数量: number = 3;\n\
             const 名: string = \"棚\";\n\
             const 一覧 = [1, 2] as number[];\n\
             function 先頭<T>(items: T[]): T { return items[0]; }\n\
             const 表 = new Map<string, number>();\n\
             console.log(数量, 名, 一覧, 先頭([1]), 表);\n",
        );

        let transpiled = Transpiler::new().transpile(&record).expect("変換できる");

        // 型の注釈・`as`・generics が落ちている（要件 3.1）。
        assert!(!transpiled.code.contains(": number"), "{}", transpiled.code);
        assert!(!transpiled.code.contains(": string"), "{}", transpiled.code);
        assert!(
            !transpiled.code.contains(" as number[]"),
            "{}",
            transpiled.code
        );
        assert!(!transpiled.code.contains("<T>"), "{}", transpiled.code);
        assert!(
            !transpiled.code.contains("<string, number>"),
            "{}",
            transpiled.code
        );
        // **型だけの取り込みは残らない**（実行時に名前を解決しに行かない）。
        assert!(!transpiled.code.contains("./t.ts"), "{}", transpiled.code);
        assert!(
            !transpiled.code.contains("interface"),
            "{}",
            transpiled.code
        );
        // 値の部分は残る。
        assert!(
            transpiled.code.contains("const 数量 = 3"),
            "{}",
            transpiled.code
        );
        assert!(
            transpiled.code.contains("function 先頭(items)"),
            "{}",
            transpiled.code
        );

        // 変換結果が**JavaScript として解釈できる**（型の構文が残っていればここで落ちる）。
        deno_ast::parse_module(ParseParams {
            specifier: ModuleSpecifier::parse(&transpiled.module_name).expect("読める名前"),
            text: Arc::from(transpiled.code.as_str()),
            media_type: MediaType::JavaScript,
            capture_tokens: false,
            scope_analysis: false,
            maybe_syntax: None,
        })
        .expect("変換結果が JavaScript として解釈できる");

        // 末尾にインラインの写しが付いている（3.2 が V8 に読ませる形）。
        assert!(
            transpiled.code.contains(SOURCE_MAPPING_PREFIX),
            "sourceMappingURL が付いていない"
        );
        assert!(transpiled.source_map.is_some(), "写しの欄が空である");
    }

    #[test]
    fn 型の誤りでは止まらない() {
        // どれも**型として**誤っている（未定義の型名・代入の不一致・無い型への `as`）。
        // 型の検査はエディタの仕事であり、実行は止めない（要件 3.2）。
        let record = typescript(
            "const 数: number = \"文字列\";\n\
             const 無い型: 存在しない型 = 1;\n\
             const 移し替え = {} as これも無い型;\n\
             function 返り値(): 無い型 { return 1; }\n\
             console.log(数, 無い型, 移し替え, 返り値());\n",
        );

        let transpiled = Transpiler::new()
            .transpile(&record)
            .expect("型の誤りでは止まらない（要件 3.2）");

        // 値の綴りは書き換えない（落とすのは型の構文だけである）。
        assert!(
            transpiled.code.contains("const 数 = \"文字列\""),
            "{}",
            transpiled.code
        );
        assert!(
            transpiled.code.contains("function 返り値()"),
            "{}",
            transpiled.code
        );
    }

    #[test]
    fn 構文の誤りは行と列つきで返る() {
        // 1 行目は正しく、2 行目の `=` の右が無い（要件 3.4）。
        let record = typescript("const 棚 = 1;\nconst 数 = ;\n");

        let failure = Transpiler::new()
            .transpile(&record)
            .expect_err("構文の誤りは失敗になる");

        assert_eq!(failure.kind, FailureKind::Transpile);
        assert!(
            failure.message.starts_with("構文の誤り: "),
            "理由が構文の誤りでない: {}",
            failure.message
        );
        let frame = failure.innermost().expect("位置を持つ");
        assert_eq!(frame.macro_name, record.name);
        assert_eq!(
            (frame.line, frame.column),
            (2, 11),
            "原位置が行と列で指せていない"
        );
        assert_eq!(frame.function, None, "構文の誤りに呼び出しの段は無い");
    }

    #[test]
    fn javascriptのソースはそのまま返る() {
        let record = MacroRecord::new(
            MacroName::new("棚卸し"),
            MacroKind::JavaScript,
            "// そのまま実行される\nconst 棚 = 1;\nconsole.log(棚);\n",
        );

        let transpiled = Transpiler::new()
            .transpile(&record)
            .expect("変換しないので通る");

        assert_eq!(
            transpiled.code, record.source,
            "JavaScript のソースがバイト単位で変わった（要件 3.3）"
        );
        assert_eq!(
            transpiled.source_map, None,
            "位置が動かないので写しは要らない"
        );
        // モジュール名は URL の綴りである（非 ASCII は `%XX` になる。下の「モジュール名」の
        // テストと同じ実測）。
        assert_eq!(
            transpiled.module_name,
            "macro:%E6%A3%9A%E5%8D%B8%E3%81%97.js"
        );
    }

    #[test]
    fn javascriptの構文の誤りも行と列つきで返る() {
        // 種別によらず構文は同じ形で確かめる（要件 3.4）。
        let record = MacroRecord::new(
            MacroName::new("壊れた棚卸し"),
            MacroKind::JavaScript,
            "const 棚 = 1;\nfunction () {}\n",
        );

        let failure = Transpiler::new()
            .transpile(&record)
            .expect_err("構文の誤りは失敗になる");

        assert_eq!(failure.kind, FailureKind::Transpile);
        let frame = failure.innermost().expect("位置を持つ");
        assert_eq!(frame.line, 2, "誤りの行が指せていない");
        assert!(frame.column >= 1, "列が 1 起点でない");
    }

    #[test]
    fn ソースマップは生成位置を原位置へ写せる() {
        // 先頭に 4 行の interface を置く（**変換で消える**）。以降の文が前へずれるため、
        // 写しが無ければ「生成位置の行＝原位置の行」にならない — 写しが効いていることが見える。
        let record = typescript(
            "\
interface 行 {
  数量: number;
  名: string;
}
const 一つ目 = 1;
const 二つ目 = 2;
console.log(一つ目, 二つ目);
",
        );
        let transpiled = Transpiler::new().transpile(&record).expect("変換できる");

        // interface が消えて、生成位置の行が原位置より前になっている。
        assert!(
            !transpiled.code.contains("interface"),
            "{}",
            transpiled.code
        );
        let generated_line_of = |needle: &str| -> u32 {
            transpiled
                .code
                .lines()
                .position(|line| line.contains(needle))
                .unwrap_or_else(|| panic!("{needle} が生成位置に無い")) as u32
        };
        // 目印 → 原位置の行（1 起点）。
        let markers = [("一つ目", 5u32), ("二つ目", 6), ("console", 7)];
        assert!(
            markers
                .iter()
                .all(|(needle, line)| generated_line_of(needle) + 1 != *line),
            "写しが無くても通る位置関係になっている（テストが効かない）"
        );

        // **2 つの経路**（写しの欄と、コード末尾のデータ URL）で同じ原位置を指す。
        // `get_src_line() + 1` と `get_src_col() + 1` がそのまま `Frame::at` へ入れる値である
        // （3.2 は `deno_core` の `SourceMapper::apply_source_map` を通してこれを行う）。
        for map in [open_map_field(&transpiled), open_inline_map(&transpiled)] {
            for (needle, original_line) in markers {
                let generated_line = generated_line_of(needle);
                let line_text = transpiled
                    .code
                    .lines()
                    .nth(generated_line as usize)
                    .expect("生成位置の行");
                let generated_column = line_text.find(needle).unwrap_or(0) as u32;

                let token = map
                    .lookup_token(generated_line, generated_column)
                    .unwrap_or_else(|| panic!("{needle} の位置に写しが無い"));
                assert_eq!(
                    token.get_src_line() + 1,
                    original_line,
                    "{needle} の原位置の行が違う"
                );
                assert_eq!(
                    token.get_source(),
                    Some(transpiled.module_name.as_str()),
                    "{needle} の写しの名前がモジュール名と違う"
                );
            }

            // 原位置のソースも写しが抱えている（3.2 が例外の行の中身を出すのに使う）。
            assert_eq!(map.get_source_contents(0), Some(record.source.as_str()));
        }
    }

    #[test]
    fn モジュール名はマクロ名と種別から決まる() {
        let ts = typescript("const 棚 = 1;\n");
        // URL は非 ASCII を常に `%XX` へ符号化する（`url` の実測）。利用者が読む名前は
        // `MacroName`（`在庫集計`）であり、モジュールの同一性はこの URL の綴りで決まる。
        assert_eq!(
            module_name(&ts),
            "macro:%E5%9C%A8%E5%BA%AB%E9%9B%86%E8%A8%88.ts"
        );
        assert_eq!(
            Transpiler::new()
                .transpile(&ts)
                .expect("変換できる")
                .module_name,
            "macro:%E5%9C%A8%E5%BA%AB%E9%9B%86%E8%A8%88.ts"
        );

        // 非 ASCII は `%XX` になり、空白はそのまま残る（`url` の実測 — `path` の符号化は
        // ASCII 以外を必ず符号化し、空白は集合に入っていない）。ローダー（3.2）もこの関数を
        // 通すため、V8 が報告する名前・写しの `sources`・ローダーが引く名前が同じ 1 つに揃う。
        let js = MacroRecord::new(MacroName::new("棚 卸し"), MacroKind::JavaScript, "1;\n");
        assert_eq!(module_name(&js), "macro:%E6%A3%9A %E5%8D%B8%E3%81%97.js");

        // 名前は URL として往復する（往復で綴りが変わると、モジュールの同一性が崩れる）。
        for record in [&ts, &js] {
            let name = module_name(record);
            let specifier = ModuleSpecifier::parse(&name).expect("読める名前");
            assert_eq!(specifier.as_str(), name, "名前が往復で変わった");
        }
    }
}

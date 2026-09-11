//! フロントエンドとドメインコアをつなぐ単一の通信境界（要件 4.1、4.2）。
//!
//! 本モジュールは、境界を越えるすべての入力と出力の型を**ひとつの定義**から導けるようにする
//! 場所である。フロントエンド側とドメイン側はこの定義だけを参照し、呼び出し口を各機能が
//! 生やさない。`ts-rs` の derive を付けてよい唯一の場所でもあり、他のモジュールで境界の型に
//! derive してはならない（design.md「IpcContract」の不変条件）。`i64` / `u64` は境界へ直接
//! 出さず、識別子は文字列表現とする。
//!
//! 本モジュールは境界の型そのものと、エラー封筒を再輸出する。実体は tasks.md が追加する:
//! - 2.1: 境界を越える型とエラー封筒（本ファイルと `error`）
//! - 2.2: コマンド名の単一配列と TypeScript 生成（`command_names`）

use serde::{Deserialize, Serialize};

pub mod command_names;
pub mod error;

pub use error::IpcError;

/// 境界を越えるすべてのコマンドが返す封筒（要件 4.2、4.4）。
///
/// `status` を判別子とし、成功（`ok`）と失敗（`error`）を型で区別する判別可能な合併型として
/// TypeScript へ落ちる。利用側は `status` で網羅的に分岐できる（tasks.md 2.4 がこの性質の上に
/// 薄い呼び出しラッパを載せる）。本型を含め、境界の生成物に `any` を混入させない
/// （research.md 決定 1 が `tauri-specta` を却下した理由のひとつ）。
//
// `serde(tag = "status")` の内部タグ付け。生成される TypeScript は
// `{ "status": "ok", data: T, } | { "status": "error", error: E, }` である。失敗の原因は
// [`IpcError`] が運ぶ（要件 4.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "status")]
pub enum IpcResult<T, E> {
    /// 成功。ドメインの結果を `data` に載せる。
    #[serde(rename = "ok")]
    Ok { data: T },
    /// 失敗。原因を `error` に載せる。
    #[serde(rename = "error")]
    Err { error: E },
}

/// 境界を越えるウィンドウの識別子（要件 4.2、4.6）。
///
/// 境界を越える識別子は 64 ビット整数をそのまま公開せず、文字列表現とする。JavaScript の
/// `number` は IEEE 754 の倍精度であり、`i64` / `u64` の全域を正確に表せないためである。
/// TypeScript 側の型も `string` に固定する（design.md「IpcContract」の不変条件）。
//
// 丸めの具体例: `u64::MAX = 18446744073709551615` は JavaScript の数値では
// `18446744073709552000` になる。識別子を 64 ビット整数のまま境界へ出すと、型検査も実行時も
// 静かに値を取り違える。
//
// `ts(type = "string")` は、内側の型が将来 64 ビット整数へ変わっても生成物が `bigint` /
// `number` へ落ちないようにするための固定である。1 フィールドの新定型は serde でも内側の値
// そのものとして直列化されるため `serde(transparent)` は付けない（付けると ts-rs が解釈
// できず警告を出すうえ、挙動は変わらない）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[ts(type = "string")]
pub struct WindowLabel(String);

impl WindowLabel {
    /// ラベル文字列から識別子を作る。
    pub fn new(label: impl Into<String>) -> Self {
        Self(label.into())
    }

    /// 元のラベル文字列を返す。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// コマンド呼び出しの文脈（要件 4.2、4.6）。呼び出し元ウィンドウを呼び出し先が識別できる
/// ようにする。
//
// 呼び出し元の識別子は文字列の [`WindowLabel`] で運ぶため、64 ビット整数は境界に現れない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct WindowContext {
    /// 呼び出し元ウィンドウのラベル。
    pub window: WindowLabel,
}


#[cfg(test)]
mod tests {
    use super::error::IpcError;
    use super::*;
    use ts_rs::TS;

    /// 境界を越える型の TypeScript 宣言を文字列として得る。`export_to_string` はファイルを
    /// 書かないため、テストから副作用がない。
    fn generated<T: TS + 'static>() -> String {
        T::export_to_string(&ts_rs::Config::default())
            .expect("境界を越える型は TypeScript へ生成できなければならない")
    }

    /// 生成物からコメントを取り除く。ts-rs は Rust の `///` を JSDoc（`/** … */`）として
    /// 出力するため、説明文がそのまま型宣言の隣に並ぶ。
    ///
    /// **違反を判定してよいのは型の位置だけである。** 要件 4.2 / 4.4 が問題にしているのは
    /// 型の表現であり、research.md 決定 1 が `tauri-specta` を却下した理由（`e as any` /
    /// `payload: any` / `Event<any>`）も、フロントエンドの `@typescript-eslint/no-explicit-any`
    /// が見るのも型の位置である。説明文に `any` や `number` が現れても逸反ではないので、
    /// 走査の前にコメントを落とす。
    ///
    /// 文字列リテラルの中の `//` をコメントと誤認しないよう、引用符の状態を追う。
    fn strip_comments(ts: &str) -> String {
        let mut out = String::with_capacity(ts.len());
        let mut chars = ts.chars().peekable();
        let mut quote: Option<char> = None;
        while let Some(c) = chars.next() {
            if let Some(q) = quote {
                out.push(c);
                if c == '\\' {
                    if let Some(escaped) = chars.next() {
                        out.push(escaped);
                    }
                } else if c == q {
                    quote = None;
                }
                continue;
            }
            match c {
                '"' | '\'' | '`' => {
                    quote = Some(c);
                    out.push(c);
                }
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    let mut prev = '\0';
                    for c in chars.by_ref() {
                        if prev == '*' && c == '/' {
                            break;
                        }
                        prev = c;
                    }
                    // 前後の識別子が連結して見えないよう空白で置き換える。
                    out.push(' ');
                }
                '/' if chars.peek() == Some(&'/') => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                    out.push(' ');
                }
                _ => out.push(c),
            }
        }
        out
    }

    /// 型の位置に現れた `token` を単語境界で探す。識別子の一部（`NumberOfRows` の `number`、
    /// `anyCount` の `any`）は型ではないので拾わない。JSDoc の中身は [`strip_comments`] で
    /// 落としてから走査する。
    fn assert_no_type_token(ts: &str, token: &str) {
        let code = strip_comments(ts);
        let mut from = 0;
        while let Some(rel) = code[from..].find(token) {
            let at = from + rel;
            let before = code[..at].chars().next_back();
            let after = code[at + token.len()..].chars().next();
            let is_ident = |c: Option<char>| {
                c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
            };
            assert!(
                is_ident(before) || is_ident(after),
                "生成物の型の位置に {token} が現れた:\n{code}"
            );
            from = at + token.len();
        }
    }

    /// 生成物の型の位置に `any` が現れないことを検査する。
    fn assert_no_any(ts: &str) {
        assert_no_type_token(ts, "any");
    }

    /// 生成物の型の位置に 64 ビット整数由来の数値型が現れないことを検査する。
    fn assert_no_numeric_type(ts: &str) {
        assert_no_type_token(ts, "number");
        assert_no_type_token(ts, "bigint");
    }

    /// 数値が 1 つでも現れたら失敗する。境界の型が 64 ビット整数を露出していれば、
    /// JSON では `Number` として現れる。
    fn assert_no_json_number(value: &serde_json::Value, at: &str) {
        match value {
            serde_json::Value::Number(n) => panic!("{at} に数値が露出している: {n}"),
            serde_json::Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    assert_no_json_number(item, &format!("{at}[{i}]"));
                }
            }
            serde_json::Value::Object(fields) => {
                for (name, item) in fields {
                    assert_no_json_number(item, &format!("{at}.{name}"));
                }
            }
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::String(_) => {}
        }
    }

    #[test]
    fn generated_bindings_contain_no_any() {
        assert_no_any(&generated::<IpcResult<WindowContext, IpcError>>());
        assert_no_any(&generated::<IpcError>());
        assert_no_any(&generated::<WindowContext>());
        assert_no_any(&generated::<WindowLabel>());
        assert_no_numeric_type(&generated::<IpcResult<WindowContext, IpcError>>());
        assert_no_numeric_type(&generated::<IpcError>());
        assert_no_numeric_type(&generated::<WindowContext>());
        assert_no_numeric_type(&generated::<WindowLabel>());
    }

    /// 検出器そのものが本物の `any` を落とすことを固定する。これが無いと、判定が常に通っても
    /// 誰も気づけない。
    #[test]
    #[should_panic(expected = "any")]
    fn any_detector_rejects_a_real_any() {
        assert_no_any("export type Value = any;");
    }

    /// 数値型についても同じ対称性を固定する。
    #[test]
    #[should_panic(expected = "number")]
    fn numeric_detector_rejects_a_real_number() {
        assert_no_numeric_type("export type Id = number;");
    }

    #[test]
    #[should_panic(expected = "bigint")]
    fn numeric_detector_rejects_a_real_bigint() {
        assert_no_numeric_type("export type Id = bigint;");
    }

    /// `any` を部分文字列として含む識別子を誤検出しないことを固定する。
    #[test]
    fn detectors_ignore_identifiers_containing_the_tokens() {
        assert_no_any("export type Company = { many: string, anything: string, anyCount: string, }");
        assert_no_numeric_type("export type RowCount = { numberOfRows: string, bigintValue: string, }");
    }

    /// 説明文に現れただけの語は違反ではないことを固定する。ts-rs は `///` を JSDoc として
    /// 出力するため、これが無いと「文書を書くとテストが落ちる」状態に戻る。
    #[test]
    fn detectors_ignore_tokens_that_appear_only_in_comments() {
        let ts = concat!(
            "// This value must not be any arbitrary JavaScript number.\n",
            "/**\n",
            " * TypeScript の number へは落ちない。bigint でも、any でもない。\n",
            " */\n",
            "export type WindowLabel = string;\n",
        );
        assert_no_any(ts);
        assert_no_numeric_type(ts);
    }

    /// コメントを落とした後も型の位置は残ることを固定する（除去が過剰でないことの確認）。
    #[test]
    #[should_panic(expected = "number")]
    fn comment_stripping_keeps_type_positions() {
        assert_no_numeric_type("// number\nexport type Id = number;");
    }

    #[test]
    fn envelope_narrows_on_status() {
        let ts = generated::<IpcResult<WindowContext, IpcError>>();
        assert!(ts.contains("\"status\": \"ok\""), "{ts}");
        assert!(ts.contains("\"status\": \"error\""), "{ts}");
    }

    #[test]
    fn error_distinguishes_its_causes() {
        let ts = generated::<IpcError>();
        for kind in ["Settings", "Sidecar", "Window"] {
            assert!(ts.contains(&format!("\"kind\": \"{kind}\"")), "{ts}");
        }
    }

    #[test]
    fn identifier_is_a_string_that_keeps_the_exact_value() {
        let ts = generated::<WindowLabel>();
        assert!(ts.contains("type WindowLabel = string;"), "{ts}");
        // 型の位置に数値型が無いこと。説明文（JSDoc）の中身は対象外である。
        assert_no_numeric_type(&ts);
        assert_no_any(&ts);
        let ts = generated::<WindowContext>();
        assert!(ts.contains("window: WindowLabel"), "{ts}");
        // f64 では正確に表せない値。文字列表現ならそのまま往復する。
        let raw = "18446744073709551615";
        let value = serde_json::to_value(WindowLabel::new(raw)).unwrap();
        assert_eq!(value, serde_json::Value::String(raw.into()));
        let back: WindowLabel = serde_json::from_value(value).unwrap();
        assert_eq!(back.as_str(), raw);
    }

    #[test]
    fn boundary_types_expose_no_number() {
        let envelope: IpcResult<WindowContext, IpcError> = IpcResult::Ok {
            data: WindowContext { window: WindowLabel::new("doc-0") },
        };
        assert_no_json_number(&serde_json::to_value(&envelope).unwrap(), "envelope");

        let err: IpcResult<WindowContext, IpcError> = IpcResult::Err {
            error: IpcError::Sidecar { message: "起動できない".into() },
        };
        assert_no_json_number(&serde_json::to_value(&err).unwrap(), "error");
    }

    #[test]
    fn error_payload_is_not_a_bare_string() {
        let causes = [
            IpcError::Settings { message: "設定を読めない".into() },
            IpcError::Sidecar { message: "起動できない".into() },
            IpcError::Window { message: "生成できない".into() },
        ];
        let mut kinds = std::collections::BTreeSet::new();
        for cause in &causes {
            let value = serde_json::to_value(cause).unwrap();
            let object = value.as_object().expect("エラーは原因の情報を持つ対象である");
            assert!(kinds.insert(object["kind"].as_str().unwrap().to_owned()));
            // 詳細は `detail` の下の対象であり、裸の文字列ではない。
            let detail = object["detail"].as_object().expect("詳細は対象である");
            assert!(detail["message"].is_string(), "{value}");
        }
        assert_eq!(kinds.len(), causes.len());
    }

    #[test]
    fn envelope_round_trips_both_arms() {
        let ok: IpcResult<WindowContext, IpcError> = IpcResult::Ok {
            data: WindowContext { window: WindowLabel::new("doc-0") },
        };
        let value = serde_json::to_value(&ok).unwrap();
        assert_eq!(value["status"], serde_json::Value::String("ok".into()));
        assert!(value.get("data").is_some());
        assert_eq!(serde_json::from_value::<IpcResult<WindowContext, IpcError>>(value).unwrap(), ok);

        let err: IpcResult<WindowContext, IpcError> = IpcResult::Err {
            error: IpcError::Window { message: "生成できない".into() },
        };
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(value["status"], serde_json::Value::String("error".into()));
        assert!(value.get("error").is_some());
        assert_eq!(serde_json::from_value::<IpcResult<WindowContext, IpcError>>(value).unwrap(), err);
    }
}

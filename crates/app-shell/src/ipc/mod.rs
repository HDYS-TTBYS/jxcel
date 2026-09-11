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
//! - 7.1: 設定コマンドの入力と応答（[`SettingsGetRequest`] / [`SettingsSetRequest`] /
//!   [`SettingsResponse`] / [`SettingsValue`]）と、設定変更のイベント（[`SETTINGS_CHANGED_EVENT`] /
//!   [`SettingsChangedEvent`]）

use serde::{Deserialize, Serialize};

pub mod command_names;
pub mod error;

pub use command_names::COMMAND_NAMES;
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

/// 境界を越える設定値（タスク 7.1。要件 7.1、7.4）。
///
/// 設定ストアは鍵から生の JSON への写像を持つ（[`crate::settings`]）。その値を境界で型付け
/// するには、カタログの 5 鍵それぞれに固有の型を並べた列挙を持つか、JSON の形をそのまま写す
/// かのどちらかである。ここは後者を取り、**TypeScript では `unknown`** として出す
/// （`#[ts(type = "unknown")]`）。
///
/// **`serde_json::Value` をそのまま境界へ出さない理由**は 2 つある。ts-rs の
/// `serde-json-impl` feature を有効にすると生成物が `any` になり、「生成物に `any` を混ぜない」
/// という不変条件（tasks.md 2.1、research.md 決定 1）を破る。また `i64` / `u64` を露出させる
/// 経路を型で塞いでおく必要がある（識別子は文字列とする規則）。`unknown` は受け手に絞り込みを
/// 強制するため、値の形を呼び出し側が仮定しない。
///
/// 列挙の新定型は serde でも内側の値そのものとして直列化される（[`WindowLabel`] と同じ）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(type = "unknown")]
pub struct SettingsValue(serde_json::Value);

impl SettingsValue {
    /// 生の JSON から境界の値を作る。
    pub fn new(value: serde_json::Value) -> Self {
        Self(value)
    }

    /// 生の JSON を借用する。
    pub fn as_json(&self) -> &serde_json::Value {
        &self.0
    }
}

/// 設定値の読み取り要求（タスク 7.1。要件 7.1）。
///
/// 鍵は**名前の文字列**で運ぶ。`SettingsKey` は閉じた列挙であり、文字列から鍵を作る入口は
/// `SettingsKey::from_name` だけである（要件 7.7）。したがってカタログに無い名前は
/// 呼び出し先でエラー封筒の腕になり、**値の型は鍵ごとに固定しない**（値の型の閉性は
/// 7.7 の要求ではなく、要求は鍵空間の閉性である。tasks.md 4.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct SettingsGetRequest {
    /// 読み取る設定の鍵（カタログ名。例 `appearance.theme`）。
    pub key: String,
}

/// 設定値の書き込み要求（タスク 7.1。要件 7.1、7.4）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct SettingsSetRequest {
    /// 書き込む設定の鍵（カタログ名）。
    pub key: String,
    /// 書き込む値。ドキュメントの内容を指す鍵はカタログに存在しない（要件 7.7）。
    pub value: SettingsValue,
}

/// 設定コマンドの応答（タスク 7.1。要件 4.6、7.1）。
///
/// **呼び出し元ウィンドウの文脈を必ず含む。**呼び出し先は呼び出し元を識別でき（要件 4.6、
/// 2.1 の [`WindowContext`] を再定義せずそのまま使う）、フロントエンドも自分がどのウィンドウから
/// 呼んだかを応答から観測できる。`key` は正規化後のカタログ名、`value` は書き込み後（読み取りは
/// 現在）の値で、鍵が存在しなければ `None` である。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct SettingsResponse {
    /// 呼び出し元ウィンドウの文脈（要件 4.6）。
    pub context: WindowContext,
    /// 対象の鍵（カタログ名）。
    pub key: String,
    /// 現在（書き込みコマンドでは書き込み後）の値。存在しない鍵は `None`。
    pub value: Option<SettingsValue>,
}

/// 設定変更を通知する Tauri イベントの名前（タスク 7.1。要件 7.4）。
///
/// `invoke` の宛先を持たないためコマンド名の配列（[`COMMAND_NAMES`]）には現れない。代わりに
/// **生成物（`src/ipc/bindings.ts`）へ定数として出す**ことで、フロントエンドが文字列
/// リテラルを綴り間違える経路を塞ぐ（タスク 2.3 のドリフト検査がこの定数もバイト比較する）。
pub const SETTINGS_CHANGED_EVENT: &str = "settings_changed";

/// 設定変更の通知（タスク 7.1。要件 7.4）。
///
/// 設定ストアの通知（`crate::settings::SettingsChanged`）を境界の形へ写したものである。運ぶのは
/// **カタログにある閉じた鍵の名前と、その新しい値だけ**である。したがってドキュメントの内容
/// （セル値・スキーマ）を指す鍵はカタログに存在せず、この経路には載りえない（要件 7.7、8.4）。
/// 記録側へ値を渡す経路はこの型ではなく [`crate::diagnostics::Redacted`] を通る（タスク 7.1 は
/// 記録に**鍵だけ**を書き、値は決して書かない）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct SettingsChangedEvent {
    /// 変更された鍵（カタログ名）。
    pub key: String,
    /// 変更後の値。
    pub value: SettingsValue,
}

/// TypeScript の生成物を再生成する、唯一の文書化されたコマンド（タスク 2.2）。
///
/// 生成物のヘッダにもこの文字列を埋め込むため、定数として一箇所に持つ。実行ファイルは
/// `crates/app-shell/src/bin/generate-bindings.rs` であり、**リポジトリルートで**実行する。
pub const REGENERATE_BINDINGS_COMMAND: &str = "cargo run -p app-shell --bin generate-bindings";

/// 生成物のヘッダ。生成物であることと再生成の手段を明示する（tasks.md 2.2）。
fn generated_header() -> String {
    format!(
        "// このファイルは生成物である。**手で編集しない。**\n\
         // 型の宣言は crates/app-shell/src/ipc/ の定義から ts-rs が、コマンド名の定数は\n\
         // command_names.rs の COMMAND_NAMES が生成する。直すのは生成元である。\n\
         //\n\
         // 再生成（リポジトリルートで実行する）: {REGENERATE_BINDINGS_COMMAND}\n\
         // 本ファイルは追跡対象である。ドリフト検査（タスク 2.3）がバイト比較する。\n"
    )
}

/// コマンド名の定数を生成する。単一の源（[`command_names::COMMAND_NAMES`]）をそのまま写し、
/// 順序も変えない。
fn command_names_constant() -> String {
    let mut out = String::new();
    out.push('\n');
    out.push_str("/**\n");
    out.push_str(
        " * 境界を越えるコマンド名の一覧。`crates/app-shell/src/ipc/command_names.rs` の\n",
    );
    out.push_str(
        " * `COMMAND_NAMES` と同一の内容・同一の順序である。`src-tauri` のハンドラ登録と\n",
    );
    out.push_str(" * 本生成物が同じ配列を参照し、名前のドリフトを構造的に塞ぐ。\n");
    out.push_str(" */\n");
    out.push_str("export const COMMAND_NAMES = [\n");
    for name in command_names::COMMAND_NAMES {
        out.push_str("  \"");
        out.push_str(name);
        out.push_str("\",\n");
    }
    out.push_str("] as const;\n");
    out
}

/// イベント名の定数を生成する（タスク 7.1。要件 7.4）。
///
/// 設定変更の通知は `invoke` の宛先を持たないため [`command_names::COMMAND_NAMES`] には現れない
/// が、**フロントエンドが文字列リテラルを綴り間違えない**ように、名前を生成物へ定数として出す。
/// 生成物はタスク 2.3 のドリフト検査がバイト比較するので、名前の変更は生成のやり直しを強制する。
fn event_names_constant() -> String {
    let mut out = String::new();
    out.push('\n');
    out.push_str("/**\n");
    out.push_str(
        " * 境界を越えるイベント名の一覧。`crates/app-shell/src/ipc/mod.rs` の定義と同一で、\n",
    );
    out.push_str(" * フロントエンドはこの定数だけを参照する（文字列リテラルを書かない）。\n");
    out.push_str(" */\n");
    out.push_str("export const SETTINGS_CHANGED_EVENT = \"");
    out.push_str(SETTINGS_CHANGED_EVENT);
    out.push_str("\";\n");
    out
}

/// 型 `T` の TypeScript 宣言を、JSDoc と `export` を付けて組み立て、整列用の型名と対で返す。
///
/// `TS::decl` はジェネリックな型でも `type IpcResult<T, E> = ...` の形を返すため、
/// ジェネリックな宣言もそのまま単一ファイルへ置ける。
fn declared<T: ts_rs::TS + 'static>(cfg: &ts_rs::Config) -> (String, String) {
    let mut text = String::new();
    if let Some(docs) = <T as ts_rs::TS>::docs() {
        text.push_str(&docs);
    }
    text.push_str("export ");
    text.push_str(&<T as ts_rs::TS>::decl(cfg));
    text.push('\n');
    (<T as ts_rs::TS>::ident(cfg), text)
}

/// 封筒の具体形。`IpcResult<T, E>` の宣言はジェネリックな形（`IpcResult<T, E>`）で出るため、
/// ペイロード型を名指しする具体形を明示的に生成する（2.1 の申し送り、tasks.md 2.2）。
///
/// ペイロード型を増やしたタスクは、同じ要領で具体形を足す（タスク 7.1 が
/// [`concrete_settings_result`] を足した）。
fn concrete_window_context_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "WindowContextResult";
    let mut text = String::from(
        "// コマンド応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指ししない\n\
         // ため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<WindowContext, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 設定コマンドの応答の具体形（タスク 7.1）。 [`concrete_window_context_result`] と同じ理由で
/// 置く。ペイロード型は [`SettingsResponse`] である。
fn concrete_settings_result(cfg: &ts_rs::Config) -> (String, String) {
    const NAME: &str = "SettingsResult";
    let mut text = String::from(
        "// 設定コマンドの応答の具体形。ジェネリックな `IpcResult` の宣言はペイロード型を名指し\n\
         // しないため、境界が名指しできる具体形を明示的に置く。\n",
    );
    text.push_str(&format!(
        "export type {NAME} = {};\n",
        <IpcResult<SettingsResponse, IpcError> as ts_rs::TS>::name(cfg)
    ));
    (NAME.to_owned(), text)
}

/// 境界を越える型とコマンド名から、追跡対象の TypeScript（`src/ipc/bindings.ts`）を生成する
/// （tasks.md 2.2、design.md「IpcContract」の Service Interface）。
///
/// 同一の入力から常に同一のバイト列を返す。時刻・絶対パス・ホスト名・走査順に依存する内容を
/// 含めず、宣言は TypeScript の型名で整列するため、型の追加や削除でも順序が揺れない。
///
/// `Result` を返すのは design.md の署名に合わせるためである。現在の生成経路は
/// `TS::decl` / `TS::ident` / `TS::name` だけで完結して失敗しないが、`export_to_string` を
/// 要する型が境界へ加わったときに失敗を伝えられる形を保つ。
pub fn render_bindings() -> Result<String, ts_rs::ExportError> {
    let cfg = ts_rs::Config::default();

    let mut declarations = vec![
        declared::<WindowLabel>(&cfg),
        declared::<WindowContext>(&cfg),
        declared::<SettingsValue>(&cfg),
        declared::<SettingsGetRequest>(&cfg),
        declared::<SettingsSetRequest>(&cfg),
        declared::<SettingsResponse>(&cfg),
        declared::<SettingsChangedEvent>(&cfg),
        declared::<IpcError>(&cfg),
        declared::<IpcResult<WindowContext, IpcError>>(&cfg),
        concrete_window_context_result(&cfg),
        concrete_settings_result(&cfg),
    ];
    declarations.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = generated_header();
    out.push_str(&command_names_constant());
    out.push_str(&event_names_constant());
    out.push_str(
        "\n// ---------------------------------------------------------------------------\n\
         // 境界を越える型（crates/app-shell/src/ipc/ の定義から ts-rs が生成）\n\n",
    );
    for (_, declaration) in declarations {
        out.push_str(&declaration);
    }
    Ok(out)
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
            let is_ident =
                |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$');
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
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::String(_) => {
            }
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
        assert_no_any(
            "export type Company = { many: string, anything: string, anyCount: string, }",
        );
        assert_no_numeric_type(
            "export type RowCount = { numberOfRows: string, bigintValue: string, }",
        );
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
            data: WindowContext {
                window: WindowLabel::new("doc-0"),
            },
        };
        assert_no_json_number(&serde_json::to_value(&envelope).unwrap(), "envelope");

        let err: IpcResult<WindowContext, IpcError> = IpcResult::Err {
            error: IpcError::Sidecar {
                message: "起動できない".into(),
            },
        };
        assert_no_json_number(&serde_json::to_value(&err).unwrap(), "error");
    }

    #[test]
    fn error_payload_is_not_a_bare_string() {
        let causes = [
            IpcError::Settings {
                message: "設定を読めない".into(),
            },
            IpcError::Sidecar {
                message: "起動できない".into(),
            },
            IpcError::Window {
                message: "生成できない".into(),
            },
        ];
        let mut kinds = std::collections::BTreeSet::new();
        for cause in &causes {
            let value = serde_json::to_value(cause).unwrap();
            let object = value
                .as_object()
                .expect("エラーは原因の情報を持つ対象である");
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
            data: WindowContext {
                window: WindowLabel::new("doc-0"),
            },
        };
        let value = serde_json::to_value(&ok).unwrap();
        assert_eq!(value["status"], serde_json::Value::String("ok".into()));
        assert!(value.get("data").is_some());
        assert_eq!(
            serde_json::from_value::<IpcResult<WindowContext, IpcError>>(value).unwrap(),
            ok
        );

        let err: IpcResult<WindowContext, IpcError> = IpcResult::Err {
            error: IpcError::Window {
                message: "生成できない".into(),
            },
        };
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(value["status"], serde_json::Value::String("error".into()));
        assert!(value.get("error").is_some());
        assert_eq!(
            serde_json::from_value::<IpcResult<WindowContext, IpcError>>(value).unwrap(),
            err
        );
    }

    // ------------------------------------------------------------------
    // 2.2: コマンド名の単一配列と TypeScript 生成
    // ------------------------------------------------------------------

    /// 生成物の `COMMAND_NAMES` の配列リテラルから、要素の文字列を順に取り出す。
    ///
    /// 全文一致ではなく配列の中の文字列リテラルだけを取り出すため、整形（インデントや改行）が
    /// 変わっても「同一の内容・同一の順序」という性質だけを検査できる。名前は snake_case で
    /// 引用符を含まないため、素朴な走査で足りる。
    fn emitted_command_names(ts: &str) -> Vec<String> {
        let start = ts
            .find("export const COMMAND_NAMES = [")
            .expect("生成物にコマンド名の定数が必要である");
        let rest = &ts[start..];
        let open = rest.find('[').expect("コマンド名の配列が開かれていない");
        let close = rest.find(']').expect("コマンド名の配列が閉じられていない");
        assert!(open < close, "配列の開始が終了より後にある");
        let mut names = Vec::new();
        let mut chars = rest[open + 1..close].chars();
        while let Some(c) = chars.next() {
            if c != '"' {
                continue;
            }
            let mut name = String::new();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                name.push(c);
            }
            names.push(name);
        }
        names
    }

    #[test]
    fn bindings_are_byte_deterministic() {
        let first = render_bindings().expect("境界の型から TypeScript を生成できなければならない");
        let second = render_bindings().expect("境界の型から TypeScript を生成できなければならない");
        assert!(!first.is_empty(), "生成物が空である");
        assert_eq!(
            first, second,
            "同一の入力から常に同一のバイト列が出なければならない"
        );
    }

    #[test]
    fn bindings_mirror_command_names_in_order() {
        let ts = render_bindings().unwrap();
        let emitted = emitted_command_names(&ts);
        let expected = COMMAND_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            emitted, expected,
            "生成物のコマンド名は COMMAND_NAMES と同一の内容・同一の順序でなければならない"
        );
    }

    #[test]
    fn command_names_are_unique_and_well_formed() {
        assert!(!COMMAND_NAMES.is_empty(), "コマンド名の配列を空にしない");
        let mut seen = std::collections::BTreeSet::new();
        for name in COMMAND_NAMES {
            assert!(!name.is_empty(), "空のコマンド名を置かない");
            assert!(
                name.chars().next().is_some_and(|c| c.is_ascii_lowercase()),
                "コマンド名は英小文字で始める: {name}"
            );
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "コマンド名は snake_case とする: {name}"
            );
            assert!(seen.insert(*name), "コマンド名が重複している: {name}");
        }
    }

    /// 生成物に型の位置の `any` が無いこと。**整数の数値型も同様に検査する** — 境界の型を
    /// 数値で露出させないことが 2.1 の不変条件であり、生成物はその合算である。
    #[test]
    fn bindings_contain_no_any() {
        let ts = render_bindings().unwrap();
        assert_no_any(&ts);
        assert_no_numeric_type(&ts);
    }

    #[test]
    fn bindings_declare_every_boundary_type() {
        let ts = render_bindings().unwrap();
        for declaration in [
            "export type WindowLabel = string;",
            "export type IpcError =",
            "export type IpcResult<T, E> =",
            "export type WindowContext =",
            "export type SettingsValue = unknown;",
            "export type SettingsGetRequest =",
            "export type SettingsSetRequest =",
            "export type SettingsResponse =",
            "export type SettingsChangedEvent =",
        ] {
            assert!(
                ts.contains(declaration),
                "生成物に `{declaration}` が無い:\n{ts}"
            );
        }
    }

    /// 封筒の宣言はジェネリックな形で出るため、ペイロード型を名指しする具体形を別途出力して
    /// いることを固定する（2.1 の申し送り）。
    #[test]
    fn bindings_name_the_payload_types_of_the_envelope() {
        let ts = render_bindings().unwrap();
        assert!(
            ts.contains("export type WindowContextResult = IpcResult<WindowContext, IpcError>;"),
            "封筒の具体形がペイロード型を名指ししていない:\n{ts}"
        );
        assert!(
            ts.contains("export type SettingsResult = IpcResult<SettingsResponse, IpcError>;"),
            "設定コマンドの封筒の具体形がペイロード型を名指ししていない:\n{ts}"
        );
    }

    /// 設定の境界の型がタスク 7.1 の契約どおりであることを固定する。
    #[test]
    fn settings_boundary_types_round_trip_the_value() {
        let request = SettingsSetRequest {
            key: "appearance.theme".to_owned(),
            value: SettingsValue::new(serde_json::Value::String("dark".to_owned())),
        };
        let encoded = serde_json::to_value(&request).unwrap();
        // 新定型は内側の値そのものとして直列化される（`{"key":...,"value":"dark"}`）。
        assert_eq!(
            encoded["value"],
            serde_json::Value::String("dark".to_owned())
        );
        let back: SettingsSetRequest = serde_json::from_value(encoded).unwrap();
        assert_eq!(back, request);

        let response = SettingsResponse {
            context: WindowContext {
                window: WindowLabel::new("empty-1"),
            },
            key: "appearance.theme".to_owned(),
            value: Some(SettingsValue::new(serde_json::Value::Bool(true))),
        };
        let encoded = serde_json::to_value(&response).unwrap();
        // 応答は呼び出し元ウィンドウの識別子を文字列で運ぶ（要件 4.6）。
        assert_eq!(
            encoded["context"]["window"],
            serde_json::Value::String("empty-1".into())
        );
        assert_eq!(encoded["value"], serde_json::Value::Bool(true));

        let changed = SettingsChangedEvent {
            key: "window.geometry".to_owned(),
            value: SettingsValue::new(
                serde_json::json!({ "x": 1, "y": 2, "width": 3, "height": 4 }),
            ),
        };
        let encoded = serde_json::to_value(&changed).unwrap();
        assert_eq!(
            encoded["key"],
            serde_json::Value::String("window.geometry".into())
        );
        assert_eq!(encoded["value"]["width"], serde_json::json!(3));
    }

    /// イベント名の定数が生成物に出ることを固定する（フロントエンドが綴りを間違えないため）。
    #[test]
    fn bindings_expose_the_event_name() {
        let ts = render_bindings().unwrap();
        assert!(
            ts.contains(&format!(
                "export const SETTINGS_CHANGED_EVENT = \"{SETTINGS_CHANGED_EVENT}\";"
            )),
            "生成物にイベント名の定数が無い:\n{ts}"
        );
    }

    /// 設定変更の通知は閉じた鍵の名前だけを運ぶ（要件 7.7、8.4）。
    #[test]
    fn settings_change_event_carries_only_the_catalog_key_and_value() {
        let changed = SettingsChangedEvent {
            key: "diagnostics.level".to_owned(),
            value: SettingsValue::new(serde_json::Value::String("debug".to_owned())),
        };
        let encoded = serde_json::to_value(&changed).unwrap();
        let object = encoded.as_object().expect("通知は対象でなければならない");
        assert_eq!(
            object
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            ["key", "value"]
                .into_iter()
                .map(str::to_owned)
                .collect::<std::collections::BTreeSet<_>>(),
            "通知が鍵と値以外を運んでいる: {encoded}"
        );
    }

    #[test]
    fn bindings_header_names_the_regeneration_command() {
        let ts = render_bindings().unwrap();
        assert!(
            ts.contains(REGENERATE_BINDINGS_COMMAND),
            "生成物のヘッダに再生成コマンドが無い:\n{ts}"
        );
        assert!(
            ts.contains("手で編集しない"),
            "生成物であることの注意がヘッダに無い:\n{ts}"
        );
    }
}

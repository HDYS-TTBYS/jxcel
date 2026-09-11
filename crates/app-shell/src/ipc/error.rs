//! 境界を越えるエラーの型付き封筒（要件 4.2、4.4）。
//!
//! 成功と失敗を型で区別し、失敗は原因を区別できる列挙として扱う。文字列だけのエラーにしない
//! ことで、フロントエンドが `status` により網羅的に分岐でき、原因の取りこぼしが実行前に
//! 見つかる（design.md「IpcContract」の `IpcResult` / `IpcError`）。判別可能な合併型として
//! TypeScript へ落ちる形を崩さないこと。
//!
//! 本モジュールは `ts-rs` の derive を持つ。ただし置いてよいのは [`crate::ipc`] の内側だけで
//! あり、他のモジュールで境界の型に derive してはならない（design.md「IpcContract」の
//! 不変条件）。

use serde::{Deserialize, Serialize};

/// 失敗の原因を区別できる列挙（要件 4.4）。文字列だけのエラーにしない。
///
/// `kind` を判別子とし、原因ごとの詳細を `detail` に持つ判別可能な合併型として TypeScript へ
/// 落ちる。利用側は `kind` で網羅的に分岐でき、原因ごとに異なる扱いを型で強制できる
/// （tasks.md 2.4）。
//
// 直列化の形: `serde(tag = "kind", content = "detail")` の**隣接タグ付け**で、原因（`kind`）と
// その詳細（`detail`）を分けて運ぶ。生成される TypeScript は
// `{ "kind": "Settings", "detail": { message: string, } } | …` である。
//
// design.md の Service Interface はこの属性と構造体変種（`Settings { message: String }`）の
// 組み合わせを字面どおり示している。実測した限り、固定済みの serde 1.0.229 はこの組み合わせを
// 受理する: 直列化は `{"kind":"Settings","detail":{"message":"x"}}`、復元も成功し、ts-rs の
// 生成結果も上記の判別可能な合併型になった。したがって属性を字面から動かす必要はなく、
// 「原因と詳細を分ける」という設計意図をそのまま保っている。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error, ts_rs::TS)]
#[serde(tag = "kind", content = "detail")]
pub enum IpcError {
    /// 設定の読み書きに失敗した。
    #[error("設定の読み取りに失敗した")]
    Settings { message: String },
    /// 補助プロセスの起動・整合性・終了のいずれかに失敗した。
    #[error("補助プロセスを起動できない")]
    Sidecar { message: String },
    /// ウィンドウの生成・制御に失敗した。
    #[error("ウィンドウを生成できない")]
    Window { message: String },
    /// 診断情報の提示・書き出しに失敗した（タスク 9.5 の導線。要件 8.1、8.6）。
    ///
    /// **設定の失敗と混ぜない。** 詳細度の保存は設定の書き込みそのものなので
    /// [`IpcError::Settings`] が運ぶが、保存先の解決と書き出しは設定ストアを通らない
    /// （要件 8.1、8.6）ため、原因を区別できるようにここへ分ける（要件 4.4）。
    #[error("診断情報を扱えない")]
    Diagnostics { message: String },
}

//! ビルドスクリプト（Tauri のビルド時処理の結線点）。
//!
//! `tauri_build::build()` は次を担う（design.md「Components and Interfaces →
//! Infrastructure → BuildPipeline」）:
//!
//! - `tauri.conf.json` の検証（ウィンドウ設定・識別子・バンドル設定）
//! - capability スキーマの生成（`src-tauri/gen/schemas/`）
//! - ACL マニフェストの生成（`src-tauri/gen/schemas/acl-manifests.json`）
//! - `externalBin` の複製（タスク 1.7 が配置した補助プロセスの原本を扱う）
//!
//! 生成先 `src-tauri/gen/` は追跡しない（`.gitignore`）。capability の逸脱を検査する
//! タスク 7.3 の機械検査は、この生成物を入力にする。
//!
//! 本ファイルの結線はタスク 1.3 が行った。`tauri.conf.json` を追加するのと同じ変更で
//! 結線する必要がある（`tauri_build::build()` は同ファイルを必須とするため）。

fn main() {
    tauri_build::build()
}

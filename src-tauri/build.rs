//! ビルドスクリプト（Tauri のビルド時処理の結線点）。
//!
//! 本来ここで `tauri_build::build()` を呼ぶ。同関数はウィンドウ設定の検証・
//! capability スキーマの生成・ACL マニフェストの生成・`externalBin` の複製を行う
//! （design.md「Components and Interfaces → Infrastructure → BuildPipeline」および
//! 決定 3・決定 4）。
//!
//! `tauri_build::build()` は `tauri.conf.json` を必須とする。tauri-utils の
//! `config::parse::read_from` は `tauri.conf.json` / `tauri.conf.json5` / `Tauri.toml` の
//! いずれも見つからない場合に失敗し、`tauri_build::build()` はその失敗でビルドを止める。
//! 本タスク（1.1）はワークスペースの members を成立させる足場であり `tauri.conf.json` を
//! 持たない（同ファイルは task 1.3 の成果物）ため、ここでは呼び出さない。
//! タスク 1.3 が `tauri.conf.json` を追加するのと同じ変更で、この本体を
//! `tauri_build::build()` に置き換える。

fn main() {}

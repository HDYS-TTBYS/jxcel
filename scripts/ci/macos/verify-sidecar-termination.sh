#!/bin/bash
# macOS の終了保証の検証（tasks.md 10.7）。同じ検査器を macOS の分岐で走らせる
# （`/proc` が無いので `ps -axo pid=,comm=` の名前照合になる）。`externalBin` の原本は
# `tauri-build` が実行ファイルの隣（`target/release/sidecar-smoke`）へ複製している。

set -eu
if [ ! -x target/release/sidecar-smoke ]; then
  echo "NG: 8.1 の解決先の補助プロセスがありません（--features verification-triggers のビルドか確認）: target/release/sidecar-smoke" >&2
  exit 1
fi
sh scripts/check-sidecar-termination.sh target/release/jxcel 60

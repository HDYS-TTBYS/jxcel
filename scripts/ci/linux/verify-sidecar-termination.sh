#!/bin/bash
# Linux の終了保証の検証（tasks.md 10.7）。検査器は 4 つの条件（通常終了・強制終了・
# 孫あり・孫あり強制終了 → 次回起動の掃除）を 1 回の実行で検証する。非 0 はそのまま
# パイプラインの失敗になる。

set -eu
# 8.1 の解決規則（Linux は `usr/share/jxcel/sidecar-smoke`。AppImage は ${APPDIR}、
# それ以外は実行ファイルの 1 つ上の `share/jxcel`）に合わせ、**検証用の形から見た
# 解決先**へ staging した原本を複製する（deb / システムインストールと同じ形）。
# `tauri-build` は Linux では externalBin を複製しないため、この段が要る。
set -- sidecars/sidecar-smoke-*
staged=$1
if [ ! -f "$staged" ]; then
  echo "NG: 原本がありません（先に bash scripts/stage-sidecars.sh）: $staged" >&2
  exit 1
fi
mkdir -p target/share/jxcel
cp "$staged" target/share/jxcel/sidecar-smoke
chmod 755 target/share/jxcel/sidecar-smoke
xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-sidecar-termination.sh target/release/jxcel 60

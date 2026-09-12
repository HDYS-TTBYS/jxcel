#!/bin/bash
set -eu
if [ -z "${LOCALAPPDATA:-}" ]; then
  echo "NG: LOCALAPPDATA が設定されていません" >&2
  exit 2
fi
for setup in target/release/bundle/nsis/*-setup.exe; do
  echo "配布物 (NSIS インストーラ) のサイズ: ${setup}: $(wc -c < "$setup") バイト"
done
# 既定の installMode は currentUser なので導入先は %LOCALAPPDATA%\jxcel である
# （直前の段が /S で導入済み）。Git Bash の bash からは `$LOCALAPPDATA` がそのまま
# 見え、MSYS のランタイムは `\` と `/` のどちらの区切りも受け付ける。
bash scripts/check-sidecar-integrity.sh "${LOCALAPPDATA}/jxcel" sidecar-smoke.exe

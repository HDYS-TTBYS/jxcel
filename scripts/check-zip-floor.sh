#!/bin/sh
# check-zip-floor.sh — Cargo.lock に解決された `zip` バージョンの下限機械検査
#
# 根拠（.kiro/specs/document-format/design.md Security Considerations / Requirement 3.2）:
#   RUSTSEC-2025-0168（シンボリックリンク経由の展開先脱出）は zip 2.3.0 で修正済み。
#   本ワークスペースは 8.x を採用しているが、Cargo.lock が推移的に 2.3.0 未満へ
#   ダウングレードされていないことを CI で機械的に確認する。
#
# POSIX sh 互換: `bash scripts/check-zip-floor.sh` が Linux / macOS /
# Windows (Git Bash) のいずれでも動作すること（3 OS マトリクス共用）。
#
# 使い方: sh scripts/check-zip-floor.sh [Cargo.lock のパス]  (既定: Cargo.lock)
# 終了コード: 0 = 下限充足 / 1 = 下限割れ / 2 = lock ファイル異常・解釈失敗
set -eu

FLOOR="2.3.0"
LOCK="${1:-Cargo.lock}"

if [ ! -f "$LOCK" ]; then
  echo "check-zip-floor: lock file not found: $LOCK" >&2
  exit 2
fi

# Cargo.lock の [[package]] ブロックは name 行の直後に version 行が続く。
# 同名 ("zip") のブロックが複数存在する場合（複数バージョン解決）は全て比較対象にする。
versions=$(awk '
  /^name = "zip"$/ {
    if ((getline v) > 0) {
      sub(/^version *= *"/, "", v)
      sub(/".*$/, "", v)
      print v
    }
  }
' "$LOCK")

if [ -z "$versions" ]; then
  echo "check-zip-floor: no 'name = \"zip\"' package found in $LOCK" >&2
  exit 2
fi

# 複数解決された場合は最小値が下限を満たせばよい（floor は最小に対する制約）
lowest=$(printf '%s\n' "$versions" | sort -V | head -n 1)

# semver 比較: sort -V で (解決版, 下限) の最小値が下限そのものなら 解決版 >= 下限
smallest=$(printf '%s\n%s\n' "$lowest" "$FLOOR" | sort -V | head -n 1)
if [ "$smallest" != "$FLOOR" ]; then
  echo "check-zip-floor: VIOLATION resolved zip $lowest < required floor $FLOOR (RUSTSEC-2025-0168)" >&2
  echo "check-zip-floor: all resolved zip versions in $LOCK:" >&2
  printf '%s\n' "$versions" >&2
  exit 1
fi

echo "check-zip-floor: OK resolved zip $lowest >= $FLOOR"

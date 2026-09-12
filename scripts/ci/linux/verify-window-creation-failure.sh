#!/bin/bash
# 2.10（生成の失敗の提示と隔離）。検査器は**配布物を入力として拒む**（引き金が無い）ので、
# 段は配布物を渡して exit 2 を確かめる（負の対照）。10.7 の「検証用でない形は exit 2」と
# 同じ判断である。

set -eu
# 4.4 の解決（Linux はアプリケーションデータ領域の下の logs）。
record="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs/jxcel.log"
echo "診断記録: $record"
document="$RUNNER_TEMP/jxcel-2-10-document.txt"
# 7.7 は位置を読まないが、渡す位置は実在させる（検査側も実在を要求する）。
printf 'jxcel 2.10 の検証で開くドキュメント（内容は読まれない。tasks.md 7.7）\n' > "$document"
echo "検証: 検証用の形を fail-window:<ms> で起動し、失敗の提示と隔離を確かめる"
xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-window-failure.sh \
    target/release/jxcel jxcel "$record" "$document" 60 100 100
set -- target/release/bundle/appimage/*.AppImage
appimage=$1
echo "反証: 配布物を渡すと検査器が引き金の不在で exit 2 になることを確かめる"
rc=0
# **反証も仮想ディスプレイの下で走らせる。** 検査器は実行ファイルの引き金を読む前に
# 環境（`xwininfo` と `DISPLAY`）を確かめて exit 2 で落ちるので、ディスプレイ無しで
# 呼ぶと「引き金の不在」ではなく「DISPLAY 不在」で落ち、対照が成立しない
# （実測: 2026-09-12 の Linux のランナー）。
output=$(xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-window-failure.sh \
  "$appimage" jxcel "$record" "$document" 60 100 100 2>&1) || rc=$?
printf '%s\n' "$output" | tail -n 3
if [ "$rc" -eq 0 ]; then
  echo "NG: 引き金を持たない配布物で検査が成功してしまった（検査が引き金を本当に見ていない）" >&2
  exit 1
fi
if [ "$rc" -ne 2 ]; then
  echo "NG: 配布物の対照が exit 2（入力が使えない）ではありません: ${rc}" >&2
  exit 1
fi
if ! printf '%s\n' "$output" | grep -q "引き金 'fail-window' がありません"; then
  echo "NG: 配布物の対照が「引き金の不在」以外の理由で落ちた（検査器が動いていない可能性がある）" >&2
  exit 1
fi
echo "反証: 期待どおり exit 2 で落ちました（配布物には引き金が無い）"

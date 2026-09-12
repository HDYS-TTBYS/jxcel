#!/bin/bash
# Linux の一括転送の検証（tasks.md 10.8）。1 回目の検証は検証用の形で 2 つの大きさ
# （100 行と 100,000 行）を 1 回ずつ転送させ、2 回目は**配布物を渡して非 0 で落ちることを
# 要求する**（「本当に実行されたか」の錠前の実測。FUSE に依存しないよう展開実行で起動する）。

set -eu
# 4.4 の解決（Linux はアプリケーションデータ領域の下の logs。空の XDG_DATA_HOME は
# 未設定として $HOME へ落ちる — アプリ側の解決と同じ）。
record="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs/jxcel.log"
echo "診断記録: $record"
# 記録は**追記式**であり、検査器は失敗時に記録の末尾 40 行を出力へダンプする。したがって
# **出力全体を grep してはいけない** — 直前の実行（陽性）が書いた行がダンプに混ざり、
# 前の実行の要求行で「引き金を読んでいない」判定が壊れ、前の実行の掃除行で「起動した」
# 判定が偽に成立する（いずれも実際に起きた）。以下の判定は**各実行の直前に取った行数より
# 後ろの行だけ**を見る（Windows の段の `$beforeNeg` と同じ規律）。
record_lines() {
  if [ -f "$record" ]; then wc -l < "$record" | tr -d ' '; else echo 0; fi
}
# `<開始行数>` より後の行だけから `grep -E` の一致数を出す（記録が無ければ 0）。
count_after() {
  if [ ! -f "$record" ]; then echo 0; return 0; fi
  tail -n "+$(( $1 + 1 ))" "$record" 2>/dev/null | grep -c -E "$2" || true
}

set -- target/release/bundle/appimage/*.AppImage
appimage=$1
echo "検証 1/2: 検証用の形で 100 行と 100,000 行を 1 回ずつ転送し、呼び出し回数が一定であることを検証する"
before_pos=$(record_lines)
xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-bulk-transfer.sh target/release/jxcel 60 "$record" 100,100000
# 陽性の主張も**この実行が書いた行だけ**で確かめる（検査器の内部判定は起動直前の行数から
# 始まるが、段の側でも同じ範囲を要求し、「前の段の行で満たされた」経路を塞ぐ）。
if [ "$(count_after "$before_pos" '検証用の一括転送を要求した:')" -ne 1 ]; then
  echo "NG: 検証 1/2 で検証用の形が引き金を読んだ記録が 1 行ではありません（前の段の行では満たせない）" >&2
  exit 1
fi
if [ "$(count_after "$before_pos" '残留プロセスの掃除で')" -lt 1 ]; then
  echo "NG: 検証 1/2 で検証用の形が起動した形跡がありません（起動しなかった実行を成功と取り違えない）" >&2
  exit 1
fi
echo "検証 2/2: 反証 — 配布物（既定のビルド）は検証用の引き金を読まないので転送は起きない。検査は非 0 で落ちなければならない"
before_neg=$(record_lines)
rc=0
output=$(APPIMAGE_EXTRACT_AND_RUN=1 \
  xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-bulk-transfer.sh "$appimage" 20 "$record" 100,100000 2>&1) || rc=$?
printf '%s\n' "$output"
if [ "$rc" -eq 0 ]; then
  echo "NG: 配布物に対して検査が成功してしまった（転送が 1 件も起きていないのに通っている）" >&2
  exit 1
fi
if ! printf '%s\n' "$output" | grep -q '一括転送の結果が'; then
  echo "NG: 反証が「結果の行が現れない」以外の理由で落ちた（検査器が動いていない可能性がある）" >&2
  exit 1
fi
# **配布物が起動していたこと**まで要求する — 起動しなかった場合も「結果が現れない」に
# なるので、それだけでは錠前の実測にならない。**直前の行数より後**の掃除行だけを数える
# （出力のダンプに残る前の実行の行では満たせない）。
if [ "$(count_after "$before_neg" '残留プロセスの掃除で')" -lt 1 ]; then
  echo "NG: 反証で配布物が起動した形跡がありません（起動しなかった実行を錠前の実測と取り違えない）" >&2
  exit 1
fi
# **この実行が書いた行**に要求行が 1 行も無いこと（前の実行の要求行では判定しない）。
if [ "$(count_after "$before_neg" '検証用の一括転送を要求した:')" -ne 0 ]; then
  echo "NG: 配布物が検証用の引き金を読んでいます（既定のビルドに検証用の経路が入っている）" >&2
  exit 1
fi
echo "反証: 期待どおり非 0 で落ちました（配布物は起動したが、検証用の引き金を読まないため転送が起きない）"
# 展開実行（`APPIMAGE_EXTRACT_AND_RUN=1`）は AppImage のランタイムが一時ディレクトリへ
# 展開する。検査器はプロセスを子孫ごと終了するが、ランタイムの後始末は走らないので、
# この段が作った展開先をここで片付ける（後続の段へ持ち越さない）。
rm -rf /tmp/appimage_extracted_* 2>/dev/null || true

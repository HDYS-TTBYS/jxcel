#!/bin/bash
set -eu
# 4.4 の解決（Linux はアプリケーションデータ領域の下の logs。空の XDG_DATA_HOME は
# 未設定として $HOME へ落ちる — アプリ側の解決と同じ）。
record="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs/jxcel.log"
echo "診断記録: $record"
set -- target/release/bundle/appimage/*.AppImage
appimage=$1
# 展開先を変えた後（`cd` の後）でも同じファイルを指せるよう絶対パスにする。
appimage=$(CDPATH='' cd -- "$(dirname -- "$appimage")" && pwd)/$(basename -- "$appimage")
document="$RUNNER_TEMP/jxcel-10-5-document.txt"
# 7.7 は位置を読まないが、渡す位置は実在させる（検査側も実在を要求する）。
printf 'jxcel 10.5 の検証で開くドキュメント（内容は読まれない。tasks.md 7.7）\n' > "$document"

# 配布物の実行ファイルに検証専用の識別子が入っていないこと（5.4 の片付けの規約）。
# **AppImage の中身は squashfs（圧縮）である** — AppImage をそのまま `strings` にかけても
# 圧縮された中身は現れないので、**取り出してから**調べる（`--appimage-extract` は FUSE を
# 要さない。10.2 が補助プロセスの取り出しに使っているのと同じ機構）。
extract="$RUNNER_TEMP/jxcel-10-5-extract"
rm -rf "$extract"
mkdir -p "$extract"
( cd "$extract" && "$appimage" --appimage-extract 'usr/bin/jxcel' >/dev/null )
# **`strings -a … | grep -q` を使わない**（`grep -q` が早く読むのをやめると `strings` が
# EPIPE で非 0 終了し、`pipefail` の下では一致していても pipeline が失敗する。macOS の
# 段で実際に偽の失敗になった。`grep -c` は最後まで読む）。
if [ "$(strings -a "$extract/squashfs-root/usr/bin/jxcel" | grep -c 'JXCEL_VERIFICATION' || true)" -ne 0 ]; then
  echo "NG: 配布物の実行ファイルに検証専用の識別子が入っています" >&2
  strings -a "$extract/squashfs-root/usr/bin/jxcel" | grep 'JXCEL_VERIFICATION' >&2
  exit 1
fi
rm -rf "$extract"
if [ "$(strings -a target/release/jxcel | grep -c 'JXCEL_VERIFICATION_DENY_CLOSE' || true)" -eq 0 ]; then
  echo "NG: 検証用の形に JXCEL_VERIFICATION_DENY_CLOSE がありません（--features verification-triggers のビルドではない）" >&2
  exit 1
fi
echo "検証 (片付け): 配布物の実行ファイルに JXCEL_VERIFICATION は無く（strings -a で 0 件）、検証用の形は JXCEL_VERIFICATION_DENY_CLOSE を持つ"

# 単一インスタンスはプラットフォームの機構に依存する（Linux は D-Bus のセッションバス。
# design.md の既知のリスク）。ランナーの既定のバスを使い、`dbus-run-session` があるなら
# **専用のバス**を用意して検査する（どちらでも同じ検査が走る）。
check_multi_window() {
  if command -v dbus-run-session >/dev/null 2>&1; then
    xvfb-run -a --server-args="-screen 0 1280x1024x24" \
      dbus-run-session -- sh scripts/check-multi-window.sh \
        "$appimage" target/release/jxcel jxcel 60 100 100 "$record" "$document" doc-1
  else
    xvfb-run -a --server-args="-screen 0 1280x1024x24" \
      sh scripts/check-multi-window.sh \
        "$appimage" target/release/jxcel jxcel 60 100 100 "$record" "$document" doc-1
  fi
}

if ! check_multi_window; then
  echo "FUSE 経由で配布物を起動できなかったため、APPIMAGE_EXTRACT_AND_RUN=1 で再試行します" >&2
  export APPIMAGE_EXTRACT_AND_RUN=1
  check_multi_window
fi

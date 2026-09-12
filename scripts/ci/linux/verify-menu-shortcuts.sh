#!/bin/bash
# Linux の段（tasks.md 10.6）。検査本体は POSIX sh の
# `scripts/check-menu-shortcut.sh`（ローカルでも同じものを実行できる）である。
#
# AT-SPI の前提を出力に残す: アクセシビリティの木を読むには GTK のアクセシビリティの橋
# （`GTK_MODULES=gail:atk-bridge`。検査器が自分で設定する）、D-Bus のセッションバス
# （`dbus-run-session` があれば専用のものを使う。10.5 と同じ退避）、at-spi2-core
# （`at-spi-bus-launcher`）が要る。**無ければ検査は非 0 で落ちる**（ここでは人の読める形で
# 前提を出すだけである）。
#
# 検査は 3 つの主張を 1 回の実行で確かめ、そのあと**反証**を 1 回行う（検証用の形の代わりに
# 配布物を渡すと、7.5 の検証専用の項目が無いので非 0 で落ちなければならない。落ちなければ
# 検査が項目を本当に見ていないことになる）。

set -eu
# 4.4 の解決（Linux はアプリケーションデータ領域の下の logs。空の XDG_DATA_HOME は
# 未設定として $HOME へ落ちる — アプリ側の解決と同じ）。
record="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs/jxcel.log"
echo "診断記録: $record"
set -- target/release/bundle/appimage/*.AppImage
appimage=$1
# 展開先を変えた後（`cd` の後）でも同じファイルを指せるよう絶対パスにする。
appimage=$(CDPATH='' cd -- "$(dirname -- "$appimage")" && pwd)/$(basename -- "$appimage")

# **配布物の AppImage をそのまま起動する。** この段が確かめるのは**配布物のメニューの
# 内容**であり、同梱ライブラリの組み合わせではない（自己完結性そのものは 10.3 / 10.4 の
# 起動検証と `check-shipping-bundle.sh` が担う）。
#
# **a11y スタックを入れ替える処置（AppImage の展開＋ホストのライブラリ）は要らない。**
# 以前それを足したのは、検査器が `org.a11y.atspi.Action.GetActions` を呼んでいて、
# **22.04 の libatk-bridge 2.38 がそれで被検体を abort させる**ためだった
# （`scripts/check-menu-shortcut.sh` の「キーバインドは `GetKeyBinding` で読む」を参照。
# 同梱のものでもホストのものでも 2.38 なので、入れ替えでは直らない）。
# **この段は `GetActions` を呼ばない**（キーバインドは応答が `s` である `GetKeyBinding` で読む）
# ので、配布物をそのまま使える。
# FUSE が使えないランナーでは起動の置き場が `APPIMAGE_EXTRACT_AND_RUN=1` へ退避する。
shipping="$appimage"
document="$RUNNER_TEMP/jxcel-10-6-document.txt"
# 7.7 は位置を読まないが、渡す位置は実在させる（検査側も実在を要求する）。
printf 'jxcel 10.6 の検証で開くドキュメント（内容は読まれない。tasks.md 7.7）\n' > "$document"

# `at-spi-bus-launcher` はディストリビューションによって PATH 上に無く
# `/usr/libexec` に置かれる（実測: ubuntu-22.04 のランナー）。
at_spi_launcher=""
for candidate in at-spi-bus-launcher /usr/libexec/at-spi-bus-launcher \
                 /usr/lib/at-spi2-core/at-spi-bus-launcher; do
  if command -v "$candidate" >/dev/null 2>&1; then
    at_spi_launcher=$candidate
    break
  fi
done
if [ -n "$at_spi_launcher" ]; then
  echo "AT-SPI: at-spi-bus-launcher あり（${at_spi_launcher}。at-spi2-core が導入されている）"
else
  echo "AT-SPI: at-spi-bus-launcher が無い（at-spi2-core が無い。メニューの AT-SPI 読みは成立しない）" >&2
fi
echo "AT-SPI: busctl = $(command -v busctl || echo '(見つからない)') $(busctl --version 2>/dev/null | head -n 1)"

# <検証用の実行ファイル> を差し替えて検査を走らせる（反証で配布物を渡すため）。
run_check() {
  _verify=$1
  if command -v dbus-run-session >/dev/null 2>&1; then
    xvfb-run -a --server-args="-screen 0 1280x1024x24" \
      dbus-run-session -- env GTK_MODULES=gail:atk-bridge \
      sh scripts/check-menu-shortcut.sh \
        "$shipping" "$_verify" jxcel 60 100 100 "$record" "$document"
  else
    xvfb-run -a --server-args="-screen 0 1280x1024x24" \
      env GTK_MODULES=gail:atk-bridge \
      sh scripts/check-menu-shortcut.sh \
        "$shipping" "$_verify" jxcel 60 100 100 "$record" "$document"
  fi
}

echo "検証: 配布物のメニュー項目（AT-SPI）・選択の通知・フォーカス先への作用・解決済みの綴り"
run_check target/release/jxcel

# 反証: 検証用の形の代わりに配布物を渡す（7.5 の検証専用の項目が無い）。
echo "反証: 検証用の形の代わりに配布物を渡す（検証専用の項目が無いので非 0 で落ちるはず）"
rc=0
output=$(run_check "$shipping" 2>&1) || rc=$?
printf '%s\n' "$output" | tail -n 20
if [ "$rc" -eq 0 ]; then
  echo "NG: 検証専用の項目が無いのに検査が成功してしまった（検査が項目を本当に見ていない）" >&2
  exit 1
fi
if ! printf '%s\n' "$output" | grep -q "に項目 '検証: 対象ウィンドウを記録' がありません"; then
  echo "NG: 反証が「検証専用の項目の不在」以外の理由で落ちた（検査器が動いていない可能性がある）" >&2
  exit 1
fi
echo "反証: 期待どおり非 0 で落ちました（検証専用の項目が無いことを検査が見ている）"

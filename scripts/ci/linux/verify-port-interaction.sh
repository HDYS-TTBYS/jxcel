#!/bin/bash
# Linux の段（**tasks.md 7.2**）。描画層の移植口の実装（GlideAdapter）を**実物の上で駆動**し、
# 7.2 の 3 つの主張（10 万行の走査・選択の視覚的な区別・列幅と列の位置の操作）を観測する。
# 判定の本体は POSIX sh の `scripts/check-port-interaction.sh`（ローカルでも同じものを実行できる）。
#
# # この段は一時的である（重要）
#
# 恒久の 3 OS の観測は **9.2**（走査と編集の成立）と **9.3**（走査の劣化の検出と診断への記録）が
# 担う。**この段は 9.2 / 9.3 が入った時点で取り除く** — それまでは 7.2 の受け入れの裏付けとして
# 走らせる（`design.md`「計測が無い状態で予算ゲートだけ先に結線しない。結線は計測を入れるタスクが
# 行う」に対する 7.2 の側の結線であり、**恒久のゲートではない**）。
# 1.6 の `scripts/ci/linux/verify-render-traversal.sh` と同じ性質である。
#
# # Linux だけが観測の行を読める
#
# 観測の行は**アクセシビリティの木**（AT-SPI。`busctl` で読む）から取る。**AT-SPI を持つのは
# Linux のランナーだけである**（macOS の WKWebView と Windows の WebView2 は別の
# アクセシビリティ API を使う）。したがって Linux の段だけが `check-port-interaction.sh` を
# 呼び、macOS / Windows の段は**移植口の確認の画面がその OS で描画されること**だけを確かめる
# （1.6 が同じ制約を同じ形で扱っている。恒久の 3 OS の観測は 9.2 が担う）。
#
# # 前提（CI の Linux ランナー）
#
# `scripts/ci/linux/install-system-deps.sh` が `xvfb` / `x11-utils` / `at-spi2-core` /
# `dbus-x11` 相当を導入している（`busctl` は systemd、`python3` は既定）。
# **アクセシビリティの橋（`GTK_MODULES=gail:atk-bridge`）が要る** — 観測の行は
# アクセシビリティの木から読むためである（10.6 の段と同じ退避）。

set -eu
# 4.4 の解決（Linux はアプリケーションデータ領域の下の logs。空の XDG_DATA_HOME は
# 未設定として $HOME へ落ちる — アプリ側の解決と同じ）。
record="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs/jxcel.log"
echo "診断記録: $record"

# **検証用の形を起動する**（`target/release/jxcel`。10.4 の段が
# `--features verification-triggers` と `JXCEL_VERIFICATION_BUILD=1` で作ったもの）。
verify=target/release/jxcel
if [ ! -x "$verify" ]; then
  echo "NG: 検証用の形の実行ファイルがありません: $verify（先に検証用のビルドを走らせてください）" >&2
  exit 2
fi

# 検査器を走らせる（第 1 引数は検査の対象の実行ファイル）。**Xvfb と専用のセッションバス**を
# 使うのは 1.6 の段と同じ退避である（アクセシビリティの橋は D-Bus を要する）。
run_check() {
  _app=$1
  if command -v dbus-run-session >/dev/null 2>&1; then
    xvfb-run -a --server-args="-screen 0 1280x1024x24" \
      dbus-run-session -- env GTK_MODULES=gail:atk-bridge \
      sh scripts/check-port-interaction.sh "$_app" jxcel 60 "$record"
  else
    xvfb-run -a --server-args="-screen 0 1280x1024x24" \
      env GTK_MODULES=gail:atk-bridge \
      sh scripts/check-port-interaction.sh "$_app" jxcel 60 "$record"
  fi
}

echo "検証: 移植口の操作（10 万行の走査・選択の区別・列幅と列の位置の操作）"
run_check "$verify"

# 反証: **配布物は検証専用の初期画面を読まない**（9.7 の片付けの規約）ので、移植口の確認の画面は
# 現れず観測の行は読めない。検査器は非 0 で落ちなければならない（落ちなければ、検査器が観測の行を
# 本当に見ていないことになる）。**この段が空回りしていないことの錠前である**
# （`verification.md`「無いことを確かめる検査には負の対照を付ける」）。
echo "反証: 配布物（検証用の初期画面を読まない）で検査が非 0 で落ちること"
set -- target/release/bundle/appimage/*.AppImage
appimage=$1
export APPIMAGE_EXTRACT_AND_RUN=1
rc=0
output=$(run_check "$appimage" 2>&1) || rc=$?
printf '%s\n' "$output" | tail -n 20
if [ "$rc" -eq 0 ]; then
  echo "NG: 配布物に対して検査が成功してしまった（観測の行が無いのに通っている）" >&2
  exit 1
fi
if ! printf '%s\n' "$output" | grep -q '観測の行'; then
  echo "NG: 反証が「観測の行が読めない」以外の理由で落ちた（検査器が動いていない可能性がある）" >&2
  exit 1
fi
# **配布物が起動していたこと**まで要求する — 起動しなかった場合も「観測の行が読めない」に
# なるので、それだけでは錠前の実測にならない。
if ! printf '%s\n' "$output" | grep -q 'ウィンドウ:'; then
  echo "NG: 反証で配布物のウィンドウが観測されていません（起動しなかった実行を錠前の実測と取り違えない）" >&2
  exit 1
fi
echo "反証: 期待どおり非 0 で落ちました（配布物は起動したが、移植口の確認の画面を要求しないため観測の行が無い）"
# 展開実行（`APPIMAGE_EXTRACT_AND_RUN=1`）の後始末（10.8 の段と同じ規律）。
rm -rf /tmp/appimage_extracted_* 2>/dev/null || true

echo "注記: この段と検査器は一時的です（恒久の 3 OS の観測は 9.2 / 9.3 が担い、その時点で取り除きます）。"

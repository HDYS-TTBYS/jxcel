#!/bin/bash
# Linux の描画検証（tasks.md 10.4）。4 回起動する（配布物・smoke-table・smoke-editor・
# 反証）。検査本体は POSIX sh の `scripts/check-x11-render.sh`（ローカルでも同じものを
# 実行できる）。第 7 引数は**期待する「描画された画面」**である（要求した識別子ではない）。

set -eu
# 4.4 の解決（Linux はアプリケーションデータ領域の下の logs。空の XDG_DATA_HOME は
# 未設定として $HOME へ落ちる — アプリ側の解決と同じ）。
record="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs/jxcel.log"
echo "診断記録: $record"
set -- target/release/bundle/appimage/*.AppImage
appimage=$1
# 配布物は環境変数を読まない（9.7）ので初期画面は 9.6 の空ウィンドウの画面である。
# それでも `画面=empty-window` を要求する — **配布物でも画面の報告経路が働いている
# こと**の証明になる（要求は無視されるが、期待は成立する）。
echo "検証 1/4: 配布物（AppImage）を直接起動する。追加のインストール手順は無い（deb の導入も apt も使わない）"
if ! xvfb-run -a --server-args="-screen 0 1280x1024x24" \
     sh scripts/check-x11-render.sh "$appimage" jxcel 60 100 100 "$record" empty-window; then
  echo "FUSE 経由で起動できなかったため、APPIMAGE_EXTRACT_AND_RUN=1 で再試行します" >&2
  export APPIMAGE_EXTRACT_AND_RUN=1
  xvfb-run -a --server-args="-screen 0 1280x1024x24" \
    sh scripts/check-x11-render.sh "$appimage" jxcel 60 100 100 "$record" empty-window
fi
echo "検証 2/4: 表形式の最小画面 smoke-table（検証用の形。スモーク画面のコードとシェルの構造は配布物と共通）"
xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-x11-render.sh target/release/jxcel jxcel 60 100 100 "$record" smoke-table
echo "検証 3/4: 文字編集の最小画面 smoke-editor（検証用の形。スモーク画面のコードとシェルの構造は配布物と共通）"
xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-x11-render.sh target/release/jxcel jxcel 60 100 100 "$record" smoke-editor
# 反証: 埋め込み可能だが**登録簿に無い**識別子を期待として渡す。実際に描画されるのは
# 既定の空ウィンドウの画面（9.7 の契約で既定へ落ちる）なので、検査は
# 「描画された画面が期待と一致しない」で非 0 で落ちなければならない。**落ちなかった
# らこの段を失敗させる** — 検査が描画された画面を見ていないことになる。
echo "検証 4/4: 反証 — 登録簿に無い識別子 no-such-screen で検査が非 0 で落ちること"
rc=0
output=$(xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-x11-render.sh target/release/jxcel jxcel 60 100 100 "$record" no-such-screen 2>&1) || rc=$?
printf '%s\n' "$output"
if [ "$rc" -eq 0 ]; then
  echo "NG: 未登録の画面の識別子で検査が成功してしまった（描画された画面を証明していない）" >&2
  exit 1
fi
if ! printf '%s\n' "$output" |
  grep -q "描画された画面が期待と一致しません: 期待 screen=no-such-screen / 実際 screen=empty-window"; then
  echo "NG: 反証が「描画された画面の不一致」以外の理由で落ちた（検査器が動いていない可能性がある）" >&2
  exit 1
fi
echo "反証: 期待どおり非 0 で落ちました（実際に描画された画面は empty-window）"

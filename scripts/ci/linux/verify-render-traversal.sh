#!/bin/bash
# Linux の段（**tasks.md 1.6**）。10 万行 × 30 列の走査を、**回避の環境変数の有無それぞれ**で
# 実測し、フレーム時間の中央値を記録する。判定の本体は POSIX sh の
# `scripts/check-render-traversal.sh`（ローカルでも同じものを実行できる）。
#
# # この段は一時的である（重要）
#
# 1.6 は**判断点**であり、その要求は「実測値と採否の判断を `research.md` へ追記する」こと
# である。恒久の 3 OS の観測は **9.2**（走査と編集の成立）と **9.3**（走査の劣化の検出と
# 診断への記録。毎秒 60 回の維持を実画面で確かめる）が担う。**この段は 9.2 / 9.3 が入った
# 時点で取り除く** — それまで `research.md` の追記の裏付けとして走らせる。
# `design.md`「計測が無い状態で予算ゲートだけ先に結線しない。結線は計測を入れるタスクが
# 行う」に対する 1.6 の側の結線であり、**恒久のゲートではない**（だから予算の合否も
# ここでは判定せず、中央値を記録するに留める）。
#
# # Linux が拘束条件である
#
# 1.6 は「**Linux の結果を判断の拘束条件とする。**最も危険な環境であり、ここが通らなければ
# 他の 2 つを測る意味がない」と定める。危険の実体は WebKitGTK の DMA-BUF レンダラである
# （research.md「Linux / WebKitGTK 上の canvas」: WebKit bug 262607 は WONTFIX、
# tauri-apps/tauri#15936 が 2.52.3 で「DOM はあるが何も塗られない」を報告）。**したがって
# この段だけが数値を出し、macOS / Windows の段は起動の確認に留めて数値を 9.2 へ送る。**
#
# # 回避の環境変数を 4 条件で振る
#
# research.md の順序（`__NV_DISABLE_EXPLICIT_SYNC=1` → `WEBKIT_DISABLE_DMABUF_RENDERER=1` →
# `WEBKIT_DISABLE_COMPOSITING_MODE=1`）で**累積**して振る（1.6「の有無それぞれで測る」）。
# `crates/app-shell/src/render_fallback.rs` の `LINUX_POLICY`（採用）と `CANDIDATES` が
# この順序の出所である。
#
# # 前提（CI の Linux ランナー）
#
# `scripts/ci/linux/install-system-deps.sh` が `xvfb` / `x11-utils` / `at-spi2-core` /
# `dbus-x11` 相当を導入している（`busctl` は systemd、`python3` は既定）。
# **アクセシビリティの橋（`GTK_MODULES=gail:atk-bridge`）が要る** — 計測の行は
# アクセシビリティの木から読むためである。`dbus-run-session` があれば専用のセッションバスを
# 使う（10.6 の段と同じ退避）。

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

# 検査器を 1 回走らせる。第 1 引数は検査の対象の実行ファイル、第 2 引数は出力の識別に使う
# 深さの名前、残りは**回避の環境変数**（`NAME=VALUE`。呼び出し側が与える）である。
run_check() {
  _app=$1
  _depth=$2
  shift 2
  if command -v dbus-run-session >/dev/null 2>&1; then
    xvfb-run -a --server-args="-screen 0 1280x1024x24" \
      dbus-run-session -- env GTK_MODULES=gail:atk-bridge "$@" \
      sh scripts/check-render-traversal.sh "$_app" jxcel 60 "$record" "$_depth"
  else
    xvfb-run -a --server-args="-screen 0 1280x1024x24" \
      env GTK_MODULES=gail:atk-bridge "$@" \
      sh scripts/check-render-traversal.sh "$_app" jxcel 60 "$record" "$_depth"
  fi
}

# 4 条件（1.6「回避の環境変数の有無それぞれ」。research.md の順序で累積する）。
echo "検証 1/4: 回避の環境変数なし（素の状態。この数値が最も重要である）"
run_check "$verify" none

echo "検証 2/4: __NV_DISABLE_EXPLICIT_SYNC=1"
run_check "$verify" explicit-sync __NV_DISABLE_EXPLICIT_SYNC=1

echo "検証 3/4: __NV_DISABLE_EXPLICIT_SYNC=1 + WEBKIT_DISABLE_DMABUF_RENDERER=1（採用している回避策）"
run_check "$verify" dmabuf __NV_DISABLE_EXPLICIT_SYNC=1 WEBKIT_DISABLE_DMABUF_RENDERER=1

echo "検証 4/4: 3 つすべて（+ WEBKIT_DISABLE_COMPOSITING_MODE=1）"
run_check "$verify" all __NV_DISABLE_EXPLICIT_SYNC=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 \
  WEBKIT_DISABLE_COMPOSITING_MODE=1

# 反証: **配布物は検証専用の初期画面を読まない**（9.7 の片付けの規約）ので、走査の画面は
# 現れず計測の行は読めない。検査器は非 0 で落ちなければならない（落ちなければ、検査器が
# 計測の行を本当に見ていないことになる）。**この段の 4 条件が空回りしていないことの錠前で
# ある**（`verification.md`「無いことを確かめる検査には負の対照を付ける」）。
echo "反証: 配布物（検証用の初期画面を読まない）で検査が非 0 で落ちること"
set -- target/release/bundle/appimage/*.AppImage
appimage=$1
export APPIMAGE_EXTRACT_AND_RUN=1
rc=0
output=$(run_check "$appimage" negative 2>&1) || rc=$?
printf '%s\n' "$output" | tail -n 20
if [ "$rc" -eq 0 ]; then
  echo "NG: 配布物に対して検査が成功してしまった（走査の計測の行が無いのに通っている）" >&2
  exit 1
fi
if ! printf '%s\n' "$output" | grep -q '走査の計測の行'; then
  echo "NG: 反証が「計測の行が読めない」以外の理由で落ちた（検査器が動いていない可能性がある）" >&2
  exit 1
fi
# **配布物が起動していたこと**まで要求する — 起動しなかった場合も「計測の行が読めない」に
# なるので、それだけでは錠前の実測にならない。
if ! printf '%s\n' "$output" | grep -q 'ウィンドウ:'; then
  echo "NG: 反証で配布物のウィンドウが観測されていません（起動しなかった実行を錠前の実測と取り違えない）" >&2
  exit 1
fi
echo "反証: 期待どおり非 0 で落ちました（配布物は起動したが、走査の画面を要求しないため計測の行が無い）"
# 展開実行（`APPIMAGE_EXTRACT_AND_RUN=1`）の後始末（10.8 の段と同じ規律）。
rm -rf /tmp/appimage_extracted_* 2>/dev/null || true

#!/bin/sh
# X11 上で配布物を起動し、ウィンドウが実際に現れることを検査する（tasks.md 1.5）。
#
# 使い方: check-x11-window.sh <実行ファイル> <タイトル部分文字列> [タイムアウト秒] [最小幅] [最小高さ] [計測値ファイル]
#
#   - 実行ファイル            : AppImage / バンドル済みバイナリなど、起動できるパス
#   - タイトル部分文字列      : ウィンドウタイトルに含まれるべき文字列（`jxcel`）
#   - タイムアウト秒          : 既定 30
#   - 最小幅 / 最小高さ       : 既定 100。これ未満のウィンドウは検出と見なさない
#                               （GTK は 20x20 程度の補助ウィンドウも作るため）
#   - 計測値ファイル          : 省略可。与えられたときだけ、成功時に
#                               `<プラットフォーム>=<ミリ秒>` を 1 行だけ書く（上書き）。
#
# 起動時間の計測（tasks.md 10.3 / 要件 1.3, 6.8）: 計測区間は**アプリを起動した瞬間から
#   ウィンドウを観測した瞬間まで**である。始点は `GDK_BACKEND=x11 nohup "$app"` を
#   実行する直前の時刻、終点はタイトルが一致して最小寸法を満たすウィンドウを X サーバ上で
#   観測した時刻。仮想ディスプレイ（Xvfb）や `xvfb-run` の起動は区間に含めない — 要件が
#   要求するのは「配布物が起動されたとき」の時間であり、検証基盤の起動時間ではないため。
#   観測は 0.1 秒ごとなので、計測値は実際の表示より最大その粒度だけ大きく出る（保守側）。
#   書き出した値の判定は `scripts/check-startup-budget.sh` が行う。
#
#   ミリ秒の時計は `date +%s%N`（GNU coreutils / uutils）を要求する。得られなければ
#   exit 2（このスクリプトの前提は X11 の xwininfo であり、実行環境は Linux である）。
#   `%N` を展開しない BSD の date で秒精度の計測値を**黙って**書くことはしない。
#
# 検査は「プロセスが起動したこと」ではなく「ウィンドウが現れたこと」に対して行う。
# プロセスの生存だけでは空白のウィンドウを見逃すため、X サーバのウィンドウツリー
# （xwininfo -root -tree）を 0.1 秒ごとに走査し、タイトルが一致して最小寸法を満たす
# ウィンドウが現れるまで待つ。現れなければ非 0 で終了し、アプリの出力を残す。
#
# 前提:
#   - DISPLAY が設定されていること。CI（Linux ランナー）では xvfb-run が設定する。
#     ローカルでは `DISPLAY=:0` など、実画面の X サーバを指す。
#   - xwininfo（x11-utils）と xvfb が必要（CI）。POSIX sh のみで動く。
#
# ウィンドウは X11 を強制して作らせる（GDK_BACKEND=x11）。Wayland に行くと X の
# ウィンドウツリーに現れず、この検査が成立しないため。
#
# 注意: AppImage を展開実行（APPIMAGE_EXTRACT_AND_RUN=1）した場合、起動ラッパーが
# 本体より先に終了して本体が孤児になることがある。その場合の後始末は仮想ディスプレイの
# 終了に委ねる（X との接続が切れると GTK アプリは終了する）。通常の FUSE 実行では
# ラッパーが本体を exec するため、この検査が起動した pid の終了で後始末が完結する。
set -eu

# ミリ秒単位の単調でない壁時計（エポックからのミリ秒）。GNU coreutils / uutils の date は
# `%N` をナノ秒に展開するので 1000000 で割る。BSD（macOS）の date は `%N` を解釈せず
# リテラルを返すため、数字以外が混じれば失敗する（秒精度の値を黙って返さない）。
now_ms() {
  _now_ns=$(date +%s%N 2>/dev/null || true)
  case "$_now_ns" in
    ''|*[!0-9]*) return 1 ;;
    *) printf '%s\n' $((_now_ns / 1000000)) ;;
  esac
}

usage() {
  echo "usage: $0 <app-path> <window-title-substring> [timeout-seconds] [min-width] [min-height] [measurement-file]" >&2
  exit 2
}

[ "$#" -ge 2 ] || usage

app=$1
title=$2
timeout_secs=${3:-30}
min_w=${4:-100}
min_h=${5:-100}
measure_file=${6:-}

if [ ! -x "$app" ]; then
  echo "NG: 実行ファイルが無いか実行権限がありません: $app" >&2
  exit 2
fi

if ! command -v xwininfo >/dev/null 2>&1; then
  echo "NG: xwininfo が見つかりません（x11-utils を導入してください）" >&2
  exit 2
fi

if [ -z "${DISPLAY:-}" ]; then
  echo "NG: DISPLAY が設定されていません（仮想ディスプレイ上で実行してください）" >&2
  exit 2
fi

# ミリ秒の時計が無ければ計測できない。ウィンドウの存在検査は成立するが、計測値を
# 書けないまま通すより、前提の不成立として落とす（tasks.md 10.3 の予算判定が
# 「計測されていない値」で無言に通ることを許さない）。
if ! now_ms >/dev/null; then
  echo "NG: ミリ秒の時計が得られません（date +%s%N を展開する実装が必要です）" >&2
  exit 2
fi

# 計測値の行に付けるプラットフォーム名。このスクリプトは X11（xwininfo）を要求するため
# 実際に走るのは Linux であるが、他の X11 環境で誤った名前を書かないよう uname で決める。
case "$(uname -s 2>/dev/null || echo unknown)" in
  Linux) platform=linux ;;
  Darwin) platform=macos ;;
  MINGW*|MSYS*|CYGWIN*) platform=windows ;;
  *) platform=x11 ;;
esac

log=$(mktemp)
pid=""

# 起動したアプリは必ず片付ける。同じジョブの後続の段に残したままにしない。
# 子孫を先に終了する: AppImage を展開実行（APPIMAGE_EXTRACT_AND_RUN=1）した場合、
# ラッパーの子として本体が動くため、ラッパーだけを終了すると本体が残る。
# shellcheck disable=SC2329 # trap 経由の cleanup から呼ばれる（shellcheck は trap を追えない）
kill_tree() {
  _tree_pid=$1
  if command -v pgrep >/dev/null 2>&1; then
    for _tree_child in $(pgrep -P "$_tree_pid" 2>/dev/null || true); do
      kill_tree "$_tree_child"
    done
  fi
  kill "$_tree_pid" 2>/dev/null || true
}

# shellcheck disable=SC2329 # 下の trap から呼ばれる
cleanup() {
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    kill_tree "$pid"
    n=0
    while [ "$n" -lt 10 ] && kill -0 "$pid" 2>/dev/null; do
      sleep 0.5
      n=$((n + 1))
    done
    kill -9 "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  if [ -n "${log:-}" ] && [ -f "$log" ]; then
    rm -f "$log"
  fi
}
trap cleanup EXIT INT TERM

report_failure() {
  echo "NG: タイトルに '$title' を含む $min_w x $min_h 以上のウィンドウが $timeout_secs 秒以内に現れませんでした" >&2
  echo "--- xwininfo: タイトル一致行 ---" >&2
  xwininfo -root -tree 2>/dev/null | grep -F "\"$title\"" >&2 || echo "(一致なし)" >&2
  echo "--- アプリの出力（末尾）---" >&2
  if [ -f "$log" ]; then
    tail -n 40 "$log" >&2
  else
    echo "(出力なし)" >&2
  fi
  exit 1
}

# 計測区間の始点。ここから起動してウィンドウを観測するまでが要件 1.3 の時間である。
start_ms=$(now_ms)
GDK_BACKEND=x11 nohup "$app" >"$log" 2>&1 &
pid=$!

# 起動したプロセスの終了を観測しても、即座に失敗とはしない。AppImage の展開実行
# （APPIMAGE_EXTRACT_AND_RUN=1）では起動ラッパーが本体より先に終了することがあり、
# ラッパーの終了はアプリの失敗を意味しないためである。ウィンドウの出現を期限まで
# 待ち続け、現れなかった場合にだけ、終了を観測した事実を添えて失敗する。
died=0
deadline_ms=$((start_ms + timeout_secs * 1000))

# 検出粒度を 0.1 秒にする（起動時間をミリ秒で報告するため）。分数秒を受け付けない
# sleep では 1 秒へ退避する（計測値はその粒度だけ大きく出る＝保守側）。
if sleep 0.1 2>/dev/null; then
  poll_sleep=0.1
else
  poll_sleep=1
fi

while :; do
  lines=$(xwininfo -root -tree 2>/dev/null | grep -F "\"$title\"" || true)
  if [ -n "$lines" ]; then
    # 各行から "幅x高さ" を取り出し、最小寸法を満たすものが 1 つでもあれば成立とする。
    match=$(printf '%s\n' "$lines" |
      sed -n 's/.*[^0-9]\([0-9][0-9]*\)x\([0-9][0-9]*\)[+-].*/\1 \2/p' |
      awk -v mw="$min_w" -v mh="$min_h" '$1 >= mw && $2 >= mh { print $1 "x" $2; exit }')
    if [ -n "$match" ]; then
      # 計測区間の終点は「観測した瞬間」。0.1 秒ごとの観測なので実際の表示より
      # 最大その粒度だけ大きく出る（保守側）。
      elapsed_ms=$(( $(now_ms) - start_ms ))
      echo "OK: ウィンドウ '$title' $match が現れました（pid=${pid}, 起動から ${elapsed_ms} ms）"
      echo "起動時間: ${elapsed_ms} ms（起動から ウィンドウ表示まで）"
      # 計測値の書き出しは**成功した試行だけ**が行う（この分岐に入った時点で成功）。
      # したがって APPIMAGE_EXTRACT_AND_RUN=1 の再試行がある場合、権威があるのは
      # exit 0 になった試行の値であり、失敗した試行はファイルに触れない。
      if [ -n "$measure_file" ]; then
        printf '%s=%s\n' "$platform" "$elapsed_ms" > "$measure_file"
        echo "計測値: ${platform}=${elapsed_ms}（書き出し先 ${measure_file}）"
      fi
      printf '%s\n' "$lines"
      exit 0
    fi
  fi

  if [ "$died" = 0 ] && ! kill -0 "$pid" 2>/dev/null; then
    echo "注意: 起動したプロセス（pid=${pid}）が先に終了しました。ウィンドウの出現を待ち続けます" >&2
    died=1
  fi

  if [ "$(now_ms)" -ge "$deadline_ms" ]; then
    break
  fi
  sleep "$poll_sleep"
done

if [ "$died" = 1 ]; then
  echo "NG: 起動したプロセスがウィンドウを出す前に終了しました（pid=${pid}）" >&2
  echo "--- アプリの出力（末尾）---" >&2
  if [ -f "$log" ]; then
    tail -n 40 "$log" >&2
  else
    echo "(出力なし)" >&2
  fi
  exit 1
fi

report_failure

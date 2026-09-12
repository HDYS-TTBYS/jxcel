#!/bin/sh
# X11 上でアプリを起動し、**ウィンドウの出現**と**初回描画の成立**、そして指定があれば
# **描画された画面の識別**を検査する（tasks.md 10.4 / 要件 1.1, 1.2, 10.1, 10.4）。
#
# 使い方:
#   check-x11-render.sh <実行ファイル> <タイトル部分文字列> [タイムアウト秒] [最小幅] [最小高さ] \
#                       <記録ファイル> [期待する画面の識別子]
#
#   - 実行ファイル          : AppImage / 検証ビルドの実行ファイルなど、起動できるパス
#   - タイトル部分文字列    : ウィンドウタイトルに含まれるべき文字列（`jxcel`）
#   - タイムアウト秒        : 既定 60。ウィンドウの出現を待つ上限
#   - 最小幅 / 最小高さ     : 既定 100。これ未満のウィンドウは検出と見なさない
#   - 記録ファイル          : アプリの診断記録（4.4 の保存先の `jxcel.log`）。
#                             **起動の直前に削除する**（前回の起動の行で偽の成功をしない）
#   - 期待する画面の識別子  : 省略可。与えると `JXCEL_VERIFICATION_INITIAL_SCREEN` を設定して
#                             起動し（検証ビルドだけが読む。9.7）、記録の初回描画の行が
#                             `画面=<識別子>` を報告していることを**要求する**。
#
# 検査は 3 段である:
#
#   1. **ウィンドウが現れたこと** — 1.5 と同じ観測（`xwininfo -root -tree` を 0.1 秒ごとに
#      走査し、タイトルが一致して最小寸法を満たすウィンドウを待つ。解析は
#      `scripts/lib/x11-window.sh` の 1 実装）。プロセスの生存ではなくウィンドウの出現に
#      対して検査する。
#   2. **初回描画が成立したこと** — 記録に 8.2 の成立行（`初回描画が成立した: label=…`、または
#      ソフトウェアラスタライザ経由の `初回描画は成立したがソフトウェアラスタライザ経由である:
#      label=…`、または**期限を超えてから成立した** `期限超過のあとに描画の通知が届いた
#      （不成立の提示を取り下げる）: label=…`）が現れることを要求する。**3 つ目も `画面=` を
#      運ぶ**ので、どの画面が描画されたかの証拠としては同等である（遅いランナーでは期限を
#      わずかに超えることがある。判定は `NoPaint` のままで、不成立の提示だけが取り下げられる）。**これが「最初のフレームが描かれた」
#      ことの客観的な信号である**（描画の失敗を検出する API は基盤に存在せず、通知は描画
#      フレームの中から届く。8.2 の契約）。期限はウィンドウ生成から 3 秒なので、ウィンドウを
#      観測した後に十分な猶予（既定 20 秒）を取って待つ。期限を超えると 8.2 は
#      `初回描画が成立しなかった: …` を記録するため、失敗の理由は記録の末尾に出る（**その行で
#      即座に落とさず、猶予の間は上の成立行を待つ** — 期限を超えても描画は成立しうる）。
#      ソフトウェア経路も**描画は成立している**（遅いだけ）ので、成立として扱う。
#   3. **指定した画面が描画されたこと**（第 7 引数があるとき） — 上記の成立行が報告する
#      `画面=<識別子>` が**期待と一致する**ことを要求する。**この行が描画の証明である。**
#
# # なぜ「要求した識別子」の行を証明に使わないのか（レビューで棄却された旧い形）
#
# 旧い形は記録の `検証用の初期画面を指定した: screen=<識別子>`（`window/mod.rs` が**要求**を
# 記録するだけの行）を画面の証明にしていた。この行は「その識別子を要求した」ことしか述べず、
# **実際に描画された画面を述べない**。フロントエンドは要求が登録簿に無ければ**警告 1 行を
# 残して既定の初期画面へ落ちる**（9.7 の契約。`src/shell/verificationScreen.ts`）ため、
# 旧い形では `no-such-screen` のような未登録の識別子や、登録簿から消えた画面でも段が緑に
# なった。したがって証明は**通知が報告する実際の画面**（`画面=`）で行う。要求の行は
# 「起動の識別」として読むだけであり（あれば出す）、**証明からは外す**。
#
# # 4 段目の検査（反証）として
#
# `no-such-screen` のような「埋め込み可能だが登録簿に無い」識別子を期待として渡すと、実際には
# 既定の画面が描画されるため `画面=empty-window` が報告され、この検査は exit 1 で落ちる。
# **落ちるべき入力で落ちることを、段そのもののコマンドで確認できる**ようにしてある。
#
# 検査の対象は「配布物」と「検証ビルド」の両方である。**期待する画面の識別子は配布物にも
# 渡せる** — 配布物は環境変数を読まない（9.7 の片付けの規約）ので要求は無視されるが、初期画面は
# 9.6 の空ウィンドウの画面であり、`画面=empty-window` が報告されることを要求できる（配布物でも
# 画面の報告経路が働いていることの証明になる）。
#
# # 前提
#   - DISPLAY が設定されていること。CI（Linux ランナー）では `xvfb-run` が設定する。
#     ローカルでは `DISPLAY=:0` など、実画面の X サーバを指す。
#   - `xwininfo`（x11-utils）が必要。POSIX sh のみで動く（3 OS 共有ではなく Linux 専用の
#     検査器である — X11 を要求するため）。
#
# # 終了コード
#   0 = すべて成立 / 1 = 検査失敗（ウィンドウ・初回描画・**描画された画面**のいずれかが
#   成立しない）/ 2 = 入力が使えない（実行ファイル不在・DISPLAY 不在・xwininfo 不在・記録
#   ファイルの指定なし）
#
# 起動したアプリは EXIT / INT / TERM のトラップで必ず片付ける（同じジョブの後続の段に
# 残さない。後続の段は同じアプリをもう一度起動するため、残ると単一インスタンスの機構に
# 引き継がれてウィンドウが出ない）。片付けと観測の実装は `scripts/lib/x11-window.sh` が持つ
# （`check-x11-window.sh` と共有する。この検査器はその置き場を source するだけである）。
#
# SC1091 / SC2154: 置き場は**同じリポジトリのファイル**であり、`x11_log` / `x11_poll_sleep` /
# `x11_died` はそこで代入される。qlty は検査対象を一時ディレクトリへ写してから shellcheck に
# かけるため、shellcheck は置き場をたどれず「たどれない・未代入」と報告する（実際の実行では
# `$0` からの相対で解決する）。契約は置き場の doc に 1 つだけ書いてある。
# shellcheck disable=SC1091,SC2154
set -eu

_x11_lib_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
# 置き場は `$0` からの相対で解決する（上の SC1091 / SC2154 の注記を参照）。
. "$_x11_lib_dir/lib/x11-window.sh"

usage() {
  echo "usage: $0 <app-path> <window-title-substring> [timeout-seconds] [min-width] [min-height] <record-file> [expected-screen-id]" >&2
  exit 2
}

[ "$#" -ge 6 ] || usage

app=$1
title=$2
timeout_secs=${3:-60}
min_w=${4:-100}
min_h=${5:-100}
record=$6
expected_screen=${7:-}

# 初回描画と画面の報告を待つ猶予（秒）。8.2 の期限はウィンドウ生成から 3 秒であり、これを
# 大きく超える値だから、ここで現れない行は後からも現れない。
render_grace_secs=20

x11_require_app "$app"

if [ -z "$record" ]; then
  echo "NG: 記録ファイルのパスが指定されていません" >&2
  exit 2
fi

x11_require_environment

case "$timeout_secs" in
  ''|*[!0-9]*)
    echo "NG: タイムアウト秒が整数ではありません: $timeout_secs" >&2
    exit 2
    ;;
esac

report_failure() {
  echo "NG: $1" >&2
  echo "--- 診断記録（末尾）: $record ---" >&2
  if [ -f "$record" ]; then
    tail -n 40 "$record" >&2
  else
    echo "(記録ファイルがありません)" >&2
  fi
  x11_dump_tail "アプリの出力（末尾）" "$x11_log"
  exit 1
}

# 記録に求める行。**成立行は 3 つある**:
#   - 期限内に成立した 2 つ（8.2 の判定は三値であり、`Painted` と `SoftwareRaster` の
#     どちらも描画の成立である）。
#   - **期限を超えてから描画が成立した 1 つ**（`期限超過のあとに描画の通知が届いた…`）。
#     遅いランナーでは期限（3 秒）をわずかに超えることがある（実測: 2026-09-12 の macOS の
#     ランナーで `期限=3000 ms 経過=3142 ms`）。この行も `画面=` を運ぶ（`watchdog.rs`）ので、
#     **どの画面が描画されたかの証拠としては同等**である（判定は `NoPaint` のままであり、
#     不成立の提示だけが取り下げられる — 8.2 の契約）。
heartbeat_re='初回描画が成立した: label=|初回描画は成立したがソフトウェアラスタライザ経由である: label=|期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label='
requested_pattern="検証用の初期画面を指定した: screen=${expected_screen}"

x11_install_cleanup_trap

# **前回の起動の行で成功しないよう、起動の前に記録を消す。**記録機構の保存先のディレクトリは
# アプリの起動（5.2 の前提確認）が作るため、ここでは作らない（無ければ何も起きない）。
rm -f "$record"

start_secs=$(date +%s)
if [ -n "$expected_screen" ]; then
  # 検証ビルドだけが読む環境変数（9.7）。配布物では無視される。
  JXCEL_VERIFICATION_INITIAL_SCREEN=$expected_screen GDK_BACKEND=x11 nohup "$app" >"$x11_log" 2>&1 &
else
  GDK_BACKEND=x11 nohup "$app" >"$x11_log" 2>&1 &
fi
x11_pid=$!

x11_pick_poll_sleep

deadline=$((start_secs + timeout_secs))
x11_window_match=""
x11_window_lines=""

while [ "$(date +%s)" -lt "$deadline" ]; do
  x11_observe_window "$title" "$min_w" "$min_h"
  if [ -n "$x11_window_match" ]; then
    break
  fi

  # 起動したプロセスの終了は即座に失敗としない（展開実行ではラッパーが本体より先に終了する）。
  # 期限まで待ち、現れなければ終了を観測した事実を添えて失敗する。
  x11_note_if_process_died
  sleep "$x11_poll_sleep"
done

if [ -z "$x11_window_match" ]; then
  if [ "$x11_died" = 1 ]; then
    report_failure "起動したプロセスがウィンドウを出す前に終了しました（pid=${x11_pid}）"
  fi
  report_failure "タイトルに '$title' を含む ${min_w} x ${min_h} 以上のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
fi

echo "OK: ウィンドウ '$title' ${x11_window_match} が現れました（pid=${x11_pid}, 起動から $(( $(date +%s) - start_secs )) 秒）"
printf '%s\n' "$x11_window_lines"

# ウィンドウを観測した後、初回描画（と、指定があれば画面の報告）を待つ。
render_deadline=$(( $(date +%s) + render_grace_secs ))
heartbeat_line=""
actual_screen=""

while [ "$(date +%s)" -lt "$render_deadline" ]; do
  if [ -f "$record" ]; then
    heartbeat_line=$(grep -E "$heartbeat_re" "$record" | tail -n 1 || true)
    # **期限超過の行は即座に失敗としない。** 期限を超えた場合は「期限超過のあとに描画の
    # 通知が届いた」行（`heartbeat_re` の 3 つ目）が続けて出るので、それを待つ。猶予を使い
    # 切っても成立行が現れなければ、不成立の行を添えて失敗する（失敗の理由が記録にある）。
    if [ -z "$heartbeat_line" ]; then
      no_paint=$(grep -F '初回描画が成立しなかった' "$record" | tail -n 1 || true)
    fi
  fi
  if [ -n "$heartbeat_line" ]; then
    if [ -z "$expected_screen" ]; then
      break
    fi
    # 成立行が報告するのは**実際に描画されていた画面**である（通知はウィンドウごとに 1 回
    # なので、待っても変わらない。8.2 の契約）。一致しなければその場で落とす。
    actual_screen=$(printf '%s\n' "$heartbeat_line" |
      sed -n 's/.*画面=\([^ ]*\).*/\1/p')
    if [ "$actual_screen" = "$expected_screen" ]; then
      break
    fi
    report_failure "描画は成立しましたが、描画された画面が期待と一致しません: 期待 screen=${expected_screen} / 実際 screen=${actual_screen:-（報告なし）}（起動時の指定が登録簿に無いか、描画が既定の画面へ落ちています）"
  fi
  sleep "$x11_poll_sleep"
done

if [ -z "$heartbeat_line" ]; then
  if [ -n "${no_paint:-}" ]; then
    report_failure "8.2 が初回描画の不成立を記録し、${render_grace_secs} 秒以内に描画の通知が届きませんでした: ${no_paint}"
  fi
  report_failure "初回描画の成立行（'$heartbeat_re'）が ${render_grace_secs} 秒以内に現れませんでした（初回描画が成立していない）"
fi
echo "初回描画: ${heartbeat_line}"

if [ -n "$expected_screen" ]; then
  echo "描画された画面: ${actual_screen}（期待 ${expected_screen}）"
  # 要求の行は**証明ではなく起動の識別**として読む（あれば出す。配布物はこの行を出さない）。
  requested_line=$(grep -F "$requested_pattern" "$record" | tail -n 1 || true)
  if [ -n "$requested_line" ]; then
    echo "初期画面の指定: ${requested_line}"
  fi
fi

exit 0

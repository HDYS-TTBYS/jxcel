#!/bin/sh
# 異常終了（パニック）の記録が残ることを検査する（要件 8.2。tasks.md 5.5 が置いた検証専用の
# 引き金 `panic:<ms>` を駆動する）。
#
# 使い方:
#   check-crash-record.sh <実行ファイル> <タイトル部分文字列> <診断記録> <異常終了の記録> \
#                         <モード: panic|ignored> [タイムアウト秒] [最小幅] [最小高さ]
#
#   - 実行ファイル         : モード `panic` では `--features verification-triggers` の
#                            **検証用の形**、モード `ignored` では**配布物**（既定のビルド）
#   - タイトル部分文字列   : ウィンドウタイトルに含まれるべき文字列（`jxcel`）
#   - 診断記録             : アプリの診断記録（4.4 の保存先の `jxcel.log`）。**起動の前に消す**
#   - 異常終了の記録       : 5.5 が書く `jxcel-crash.log` の絶対パス（`lifecycle` の
#                            `crash_record_path` と同じ場所）。**起動の前に消す**
#   - モード               :
#       panic   — パニックを起こし、**記録が残ること**と、そのあとアプリが**通常に起動・描画・
#                 終了できること**を要求する（0 か 1 を返す）
#       ignored — パニックの引き金を**読まない形**（配布物）を渡し、**記録が残らないこと**と
#                 アプリが動き続けることを要求する（**負の対照**。0 か 1 を返す）
#   - タイムアウト秒       : 既定 60
#   - 最小幅 / 最小高さ    : 既定 100。GTK の 20x20 程度の補助ウィンドウを数えないため
#
# # モード `panic` が証明すること（要件 8.2）
#
# 1. **意図したパニックが起きたこと** — 引き金 `panic:<ms>` が**メインスレッド**でパニックを
#    起こし（他のスレッドではプロセスが終わらない）、プロセスが非 0 で終わる。
# 2. **異常終了の記録が残ったこと** — 診断の保存先の `jxcel-crash.log` が、5.5 の契約
#    （`===== 異常終了（パニック） =====` の見出し・スレッド・**プロセスの終了**・
#    メッセージ・位置）を満たす。**この行が要件 8.2 の主張そのものである。**
#    パニックのメッセージは `検証専用の意図的な異常終了`（引き金が起こしたもの）であり、
#    **別の異常終了を拾った記録で満たせない**。
# 3. **記録がパニックを握り潰していないこと** — 記録のあとに元のフックが呼ばれるので、
#    既定のパニック出力（`panicked at` とメッセージ）がアプリの出力に残る。記録が例外を
#    飲み込む回帰はここで落ちる。
# 4. **何も壊れていないこと** — 同じ実行ファイルを**もう一度**起動し、ウィンドウが出て
#    初回描画が成立し、**終了コード 0 で終わる**ことを要求する（設定・記録の実体が壊れて
#    いれば、次の起動がここで落ちる）。
#
# # モード `ignored` が証明すること（負の対照）
#
# 配布物（既定のビルド）は `JXCEL_VERIFICATION_EXIT_AFTER_MS` を**読まない**（5.4 / 8.2 の
# 片付けの規約。7.4 が終了メニューを配線した時点で非既定の feature の下へ移した）。
# `panic:<ms>` を渡しても:
#
#   - アプリは**終了しない**（パニックが起きていない）
#   - `jxcel-crash.log` は**現れない**（記録が残らない）
#   - それでもウィンドウは現れ、初回描画は成立する（検査が「起動しなかった」結果として
#     緑になっていない）
#
# **「終了しない」は観測したウィンドウの `_NET_WM_PID`（そのウィンドウを所有するプロセス）で
# 見る。** 配布物（AppImage）の起動では起動ラッパーが本体を切り離すため、起動した pid は
# 本体を指さない（`scripts/lib/x11-window.sh` の `x11_window_pid` の doc）。取れなければ
# **exit 2** で落とす（生存を観測できないまま緑にしない）。
#
# **この 3 つが揃ってはじめて、モード `panic` の記録が「引き金が起こしたパニック」の記録で
# あると言える** — モード `panic` が存在しない記録を読んでいたわけではないことの対照である。
#
# # 前提
#   - DISPLAY が設定されていること（CI の Linux ランナーでは `xvfb-run` が設定する）。
#   - `xwininfo`（x11-utils）、`ps` が必要。モード `ignored` では配布物を直接起動するので、
#     AppImage の FUSE が使えない環境では `APPIMAGE_EXTRACT_AND_RUN=1` を呼び出し側が設定する。
#
# # 終了コード
#   0 = モードの主張がすべて成立 / 1 = 検査失敗 / 2 = 入力が使えない（実行ファイル不在・
#   DISPLAY 不在・モードの綴り違い・引数の欠落）
#
# 起動したプロセスは EXIT / INT / TERM のトラップで必ず片付ける（モード `ignored` のアプリは
# 自分で終わらないため、片付けが要る）。片付けと観測の実装は `scripts/lib/x11-window.sh` が持つ。
#
# SC1091 / SC2154: 置き場は同じリポジトリのファイルであり、`x11_log` / `x11_poll_sleep` /
# `x11_exit_status` などはそこで代入される（qlty は検査対象を一時ディレクトリへ写してから
# **shellcheck** にかけるため（下の disable 行）、置き場をたどれず「たどれない・未代入」と
# 報告する。契約は置き場の doc に 1 つだけ書いてある。
# shellcheck disable=SC1091,SC2154
set -eu

_x11_lib_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
. "$_x11_lib_dir/lib/x11-window.sh"

usage() {
  echo "使い方: check-crash-record.sh <実行ファイル> <タイトル部分文字列> <診断記録> <異常終了の記録> <モード: panic|ignored> [タイムアウト秒] [最小幅] [最小高さ]" >&2
  exit 2
}

[ "$#" -ge 5 ] || usage

app=$1
title=$2
record=$3
crash_record=$4
mode=$5
timeout_secs=${6:-60}
min_w=${7:-100}
min_h=${8:-100}

# パニックを起こすまでの待ち（ミリ秒）。ウィンドウの出現と初回描画（8.2 の期限は 3 秒）を
# 観測した**あと**にパニックさせたいので、余裕を取る（遅い runner でも 6 秒あれば足りる）。
panic_ms=6000
# モード `panic` の 4. で使う、通常終了の待ち（ミリ秒）。
normal_exit_ms=4000

case "$timeout_secs" in
  ''|*[!0-9]*)
    echo "NG: タイムアウト秒が整数ではありません: $timeout_secs" >&2
    exit 2
    ;;
esac
case "$mode" in
  panic | ignored) ;;
  *)
    echo "NG: モードは panic か ignored です: $mode" >&2
    exit 2
    ;;
esac
if [ -z "$record" ] || [ -z "$crash_record" ]; then
  echo "NG: 診断記録と異常終了の記録の両方が要ります" >&2
  exit 2
fi

x11_require_app "$app"
x11_require_environment

report_failure() {
  echo "NG: $1" >&2
  echo "--- 診断記録（末尾）: $record ---" >&2
  if [ -f "$record" ]; then
    tail -n 40 "$record" >&2
  else
    echo "(記録ファイルがありません)" >&2
  fi
  echo "--- 異常終了の記録: $crash_record ---" >&2
  if [ -f "$crash_record" ]; then
    cat "$crash_record" >&2
  else
    echo "(記録ファイルがありません)" >&2
  fi
  x11_dump_tail "アプリの出力（末尾）" "$x11_log"
  exit 1
}

# 記録の <行番号> より後ろに現れた最後の一致行を出す（無ければ空）。**前の段の行で成功しない**。
record_find_from() {
  if [ ! -f "$record" ]; then
    return 0
  fi
  tail -n "+$(( $1 + 1 ))" "$record" 2>/dev/null | grep -E "$2" | tail -n 1 || true
}

# 記録の現在の行数。
record_count() {
  if [ ! -f "$record" ]; then
    echo 0
    return 0
  fi
  wc -l < "$record" | tr -d ' '
}

heartbeat_re='初回描画が成立した: label=|初回描画は成立したがソフトウェアラスタライザ経由である: label=|期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label='

# タイトルが一致するウィンドウを 1 回観測する（残っていると単一インスタンスの引き継ぎに
# 吸われるため、開始前に 0 件を要求する）。
x11_observe_window "$title" "$min_w" "$min_h"
if [ "$x11_window_count" -ne 0 ]; then
  echo "NG: タイトルに '$title' を含む ${min_w} x ${min_h} 以上のウィンドウが既に ${x11_window_count} 件あります（前の段のプロセスが残っています）" >&2
  printf '%s\n' "$x11_window_lines" >&2
  exit 2
fi

# **単一インスタンスの機構に吸われないよう、アプリのプロセスが既に無いことを先に確かめる。**
# ウィンドウの有無だけでは足りない（画面を閉じて常駐しているプロセスや、X サーバが消えた後に
# 残ったプロセスを検出できない）。実行ファイル名は題名と同じという前提で、**回収前の子
# （ゾンビ）は数えない**（`check-multi-window.sh` の常駐の数え上げと同じ判断）。
stale_instances=$(ps -eo stat=,comm= 2>/dev/null |
  awk -v name="$title" '$1 !~ /^Z/ && $2 == name { n += 1 } END { print n + 0 }')
if [ "${stale_instances:-0}" -gt 0 ]; then
  echo "NG: '$title' という名前のプロセスが ${stale_instances} 件あります（前の段のアプリが残っています。単一インスタンスの機構に引き継がれるとこの検査は成立しません）" >&2
  ps -eo pid=,stat=,comm= 2>/dev/null | awk -v name="$title" '$3 == name' >&2
  exit 2
fi

x11_install_cleanup_trap

# **前回の記録で成功しないよう、起動の前に消す。**（消したことを出力にも残す — 記録が
# 「前回の実行のもの」である可能性を検査の出力から排除する。）
rm -f "$record" "$crash_record"
echo "検査の前提: 診断記録（${record}）と異常終了の記録（${crash_record}）を起動の前に消しました"

# ウィンドウの出現を待つ（<整数> は待つ上限、結果は終了コード）。
wait_for_window() {
  _deadline=$(( $(date +%s) + $1 ))
  while [ "$(date +%s)" -lt "$_deadline" ]; do
    x11_observe_window "$title" "$min_w" "$min_h"
    if [ -n "$x11_window_match" ]; then
      return 0
    fi
    x11_note_if_process_died
    sleep "$x11_poll_sleep"
  done
  return 1
}

# 初回描画の成立行を待つ（<開始行> <秒>。見つかればその行を echo して 0）。
wait_for_heartbeat() {
  _from=$1
  _deadline=$(( $(date +%s) + $2 ))
  while [ "$(date +%s)" -lt "$_deadline" ]; do
    _line=$(record_find_from "$_from" "$heartbeat_re")
    if [ -n "$_line" ]; then
      printf '%s\n' "$_line"
      return 0
    fi
    sleep "$x11_poll_sleep"
  done
  return 1
}

if [ "$mode" = panic ]; then
  # -------------------------------------------------------------------
  # モード `panic`: 意図的なパニックを起こし、記録が残ることを確かめる
  # -------------------------------------------------------------------
  JXCEL_VERIFICATION_EXIT_AFTER_MS="panic:${panic_ms}" GDK_BACKEND=x11 \
    nohup "$app" >"$x11_log" 2>&1 &
  x11_pid=$!
  x11_pick_poll_sleep

  if ! wait_for_window "$timeout_secs"; then
    if [ "$x11_died" = 1 ]; then
      report_failure "起動したプロセスがウィンドウを出す前に終了しました（pid=${x11_pid}。パニックの前に落ちている）"
    fi
    report_failure "タイトルに '$title' を含む ${min_w} x ${min_h} 以上のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
  fi
  echo "OK: ウィンドウ '$title' ${x11_window_match} が現れました（パニックの前は通常に動作している）"

  if ! heartbeat_line=$(wait_for_heartbeat 0 20); then
    report_failure "初回描画の成立行（'$heartbeat_re'）が現れませんでした（パニックの前から描画が成立していない）"
  fi
  echo "OK: 初回描画が成立しました: ${heartbeat_line}"

  if ! x11_wait_for_exit "$x11_pid" "$timeout_secs"; then
    report_failure "パニックが起きませんでした（pid=${x11_pid} が ${timeout_secs} 秒以内に終了しません。引き金が働いていない）"
  fi
  if [ "$x11_exit_status" -eq 0 ]; then
    report_failure "パニックの後にプロセスが終了コード 0 で終わりました（異常終了していない）"
  fi
  echo "OK: 意図的なパニックでプロセスが非 0 の終了コードで終わりました: ${x11_exit_status}"

  # 既定のフックの出力（記録のあとに呼ばれる）が残っていること。**記録が握り潰す回帰**を
  # ここで捕まえる。
  if ! grep -qF 'panicked at' "$x11_log"; then
    report_failure "アプリの出力に既定のパニック出力（'panicked at'）がありません（記録がパニックを握り潰している）"
  fi
  if ! grep -qF '検証専用の意図的な異常終了' "$x11_log"; then
    report_failure "アプリの出力に意図的なパニックのメッセージがありません"
  fi
  echo "OK: 既定のパニック出力（'panicked at' とメッセージ）がアプリの出力にも残っています"

  # **異常終了の記録（要件 8.2 の主張そのもの）。**
  if [ ! -f "$crash_record" ]; then
    report_failure "異常終了の記録（${crash_record}）がありません（8.2 の記録が残っていない）"
  fi
  if ! grep -qF '===== 異常終了（パニック） =====' "$crash_record"; then
    report_failure "異常終了の記録に見出し（'===== 異常終了（パニック） ====='）がありません"
  fi
  if ! grep -qF 'スレッド: main' "$crash_record"; then
    report_failure "異常終了の記録がメインスレッドのパニックを述べていません（'スレッド: main' が無い）"
  fi
  if ! grep -qF 'プロセスの終了: このパニックにより異常終了する' "$crash_record"; then
    report_failure "異常終了の記録が「このパニックにより異常終了する」ことを述べていません"
  fi
  if ! grep -qF 'メッセージ: 検証専用の意図的な異常終了' "$crash_record"; then
    report_failure "異常終了の記録のメッセージが引き金のものではありません（別のパニックの記録で満たしていないか）"
  fi
  if ! grep -qF '位置: ' "$crash_record"; then
    report_failure "異常終了の記録に発生位置（'位置: '）がありません"
  fi
  echo "OK: 異常終了の記録が残りました（${crash_record}）:"
  sed 's/^/    /' "$crash_record"

  # **何も壊れていないこと**: もう一度起動して、描画まで成立し、通常終了（0）する。
  before_recovery=$(record_count)
  # 前回のウィンドウが消えるのを待つ（単一インスタンスの引き継ぎに吸われないため）。
  gone_deadline=$(( $(date +%s) + timeout_secs ))
  while [ "$(date +%s)" -lt "$gone_deadline" ]; do
    x11_observe_window "$title" "$min_w" "$min_h"
    if [ "$x11_window_count" -eq 0 ]; then
      break
    fi
    sleep "$x11_poll_sleep"
  done
  if [ "$x11_window_count" -ne 0 ]; then
    report_failure "パニックの後にウィンドウが消えませんでした（X のツリーに ${x11_window_count} 件残っています）"
  fi

  echo "回復の確認: 同じ実行ファイルをもう一度起動し、通常に起動・描画・終了することを確かめる"
  JXCEL_VERIFICATION_EXIT_AFTER_MS="${normal_exit_ms}" GDK_BACKEND=x11 \
    nohup "$app" >"$x11_log.recovery" 2>&1 &
  recovery_pid=$!
  x11_pid=$recovery_pid
  x11_died=0
  if ! wait_for_window "$timeout_secs"; then
    report_failure "パニックの後の起動でウィンドウが ${timeout_secs} 秒以内に現れませんでした（異常終了が次の起動を壊している）"
  fi
  if ! recovery_heartbeat=$(wait_for_heartbeat "$before_recovery" 20); then
    report_failure "パニックの後の起動で初回描画が成立しませんでした"
  fi
  if ! x11_wait_for_exit "$recovery_pid" "$timeout_secs"; then
    report_failure "パニックの後の起動が ${timeout_secs} 秒以内に終了しませんでした"
  fi
  if [ "$x11_exit_status" -ne 0 ]; then
    report_failure "パニックの後の起動が終了コード ${x11_exit_status} で終わりました（通常終了は 0）"
  fi
  echo "OK: パニックの後も通常に起動し、描画が成立し（${recovery_heartbeat}）、終了コード 0 で終わりました"

  echo "OK: 異常終了の記録が残り（${crash_record}）、記録がパニックを握り潰さず、そのあとの起動も壊れていない"
  exit 0
fi

# -------------------------------------------------------------------
# モード `ignored`: 引き金を読まない形（配布物）を渡す（負の対照）
# -------------------------------------------------------------------
# **このモードは配布物を起動する。**配布物（AppImage）の起動では起動ラッパーが本体を
# 切り離すため、`$!` を本体のハンドルに使えない（実測。`scripts/lib/x11-window.sh` の
# `x11_window_pid` の doc を参照）。**ウィンドウを所有するプロセスの pid を `_NET_WM_PID`
# から引き、それを「アプリが生きている」ことの根拠にする**（観測したウィンドウとプロセスを
# 結び付けられる唯一の手段である）。
if ! command -v xprop >/dev/null 2>&1; then
  echo "NG: xprop が見つかりません（ウィンドウを所有するプロセスの識別に使います）" >&2
  exit 2
fi

JXCEL_VERIFICATION_EXIT_AFTER_MS="panic:${panic_ms}" GDK_BACKEND=x11 \
  nohup "$app" >"$x11_log" 2>&1 &
x11_pid=$!
x11_pick_poll_sleep

if ! wait_for_window "$timeout_secs"; then
  if [ "$x11_died" = 1 ]; then
    report_failure "起動したプロセスがウィンドウを出す前に終了しました（pid=${x11_pid}）"
  fi
  report_failure "タイトルに '$title' を含む ${min_w} x ${min_h} 以上のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
fi
# **集合が安定するまで待つ**（消えかけのウィンドウを本体と取り違えない。
# `x11_settle_windows` の doc）。
if ! x11_settle_windows "$title" "$min_w" "$min_h" 10; then
  report_failure "タイトルに '$title' を含むウィンドウの集合が安定しませんでした（消えかけのウィンドウが残っています）"
fi
if [ "$x11_window_count" -ne 1 ]; then
  report_failure "タイトルに '$title' を含むウィンドウが ${x11_window_count} 枚あります（この検査は 1 枚を対象にする）"
fi
probe_id=$(printf '%s\n' "$x11_window_ids" | head -n 1)
if [ -z "$probe_id" ]; then
  report_failure "ウィンドウの識別子が得られませんでした"
fi
app_pid=$(x11_window_pid "$probe_id")
if [ -z "$app_pid" ]; then
  echo "NG: ウィンドウ ${probe_id} の _NET_WM_PID を読めません（アプリのプロセスの生存を観測できません）" >&2
  exit 2
fi
echo "配布物の本体: pid=${app_pid}（ウィンドウ ${probe_id} の _NET_WM_PID。起動した pid=${x11_pid} ではない）"

heartbeat_line=$(wait_for_heartbeat 0 20 || true)
if [ -z "$heartbeat_line" ]; then
  report_failure "初回描画の成立行（'$heartbeat_re'）が現れませんでした（起動していない検査を緑にしない）"
fi
echo "OK: 配布物は起動し、初回描画が成立しました: ${heartbeat_line}"

# パニックが起きるはずの時刻（${panic_ms} ms）を十分に超えて観測する。
observation_secs=$(( panic_ms / 1000 + 6 ))
echo "観測: ${observation_secs} 秒のあいだ、引き金が無視されること（終了しないこと）を確かめます"
observation_end=$(( $(date +%s) + observation_secs ))
while [ "$(date +%s)" -lt "$observation_end" ]; do
  if ! x11_process_alive "$app_pid"; then
    report_failure "配布物の本体（pid=${app_pid}）が引き金（panic:${panic_ms}）で終了しました（既定のビルドが環境変数を読んでいる。5.4 の片付けが壊れている）"
  fi
  if [ -f "$crash_record" ]; then
    report_failure "配布物が異常終了の記録（${crash_record}）を書きました（既定のビルドに検証用の経路が入っている）"
  fi
  x11_observe_window "$title" "$min_w" "$min_h"
  if [ "$x11_window_count" -eq 0 ]; then
    report_failure "配布物のウィンドウが観測の途中で消えました（終了した可能性がある）"
  fi
  sleep "$x11_poll_sleep"
done

echo "OK: 配布物は引き金（JXCEL_VERIFICATION_EXIT_AFTER_MS=panic:${panic_ms}）を無視しました（終了せず、異常終了の記録も残らない）"
echo "OK: 異常終了の記録が「引き金が起こしたパニック」の記録であることの負の対照が成立した"
exit 0

#!/bin/sh
# 単一インスタンス化と複数ウィンドウの経路を X11 上で検査する（tasks.md 10.5 / 要件 1.5, 2.3, 2.6）。
#
# 使い方:
#   check-multi-window.sh <配布物> <検証用の実行ファイル> <タイトル部分文字列> \
#                         [タイムアウト秒] [最小幅] [最小高さ] \
#                         <記録ファイル> <ドキュメント位置> <拒否ラベル>
#
#   - 配布物                : AppImage など、**(a)/(b) の実測対象である既定のビルド**の実行ファイル
#   - 検証用の実行ファイル  : `--features verification-triggers` の**検証用の形**
#                             （(c) の実測対象。`JXCEL_VERIFICATION_DENY_CLOSE` を読む唯一の形）
#   - タイトル部分文字列    : ウィンドウタイトルに含まれるべき文字列（`jxcel`）。
#                             **実行ファイル名でもある**（常駐インスタンスの数え上げに使う）
#   - タイムアウト秒        : 既定 60。ウィンドウの出現・2 つ目の終了・引き渡しを待つ上限
#   - 最小幅 / 最小高さ     : 既定 100。GTK の 20x20 程度の補助ウィンドウを数えないため
#   - 記録ファイル          : アプリの診断記録（4.4 の保存先の `jxcel.log`）。**ラベルの根拠**である
#   - ドキュメント位置      : 2 つ目の起動へ渡す引数（7.7 は位置を読まない。実在するだけでよい）
#   - 拒否ラベル            : 検証用の形をドキュメント付きで起動したときのウィンドウのラベル
#                             （`doc-1`）。`JXCEL_VERIFICATION_DENY_CLOSE` に渡し、
#                             記録に現れること**も**要求する（どちらかが食い違えば失敗する）
#
# # 検査する 3 つの経路（tasks.md 10.5）
#
#   (a) **アプリ起動中に配布物を再実行しても 2 つ目が常駐しない** — 2 つ目の起動へドキュメントを
#       渡す（単一インスタンスプラグインは引数をそのまま引き渡す。5.1）。2 つ目のプロセスが
#       期限内に**終了コード 0 で終わる**こと、常駐インスタンスが 1 つだけであること、
#       既存のウィンドウが残っていることを見る。**引数なしの 2 つ目の起動も測る** —
#       5.1 / 6.1 の `present_existing_or_create` は既にあるウィンドウを前面に出すだけで
#       **新しいウィンドウを作らない**（観測した事実を記録に残す）。
#   (b) **ドキュメントを開くとウィンドウが増え、既存のウィンドウが閉じない** — (a) と同じ起動で、
#       ウィンドウ数が 1 増えること、既存の識別子が残っていること、記録が新しいウィンドウを
#       `doc-<連番>` として記録していること（6.1 のラベル規約）を見る。
#   (c) **委譲先が拒否を返すとウィンドウが閉じない** — 検証用の形を
#       `JXCEL_VERIFICATION_DENY_CLOSE=<ラベル>` で起動し、そのウィンドウへ
#       `WM_DELETE_WINDOW` を送る（7.6 と同じ `XSendEvent`。`event_mask=0`）。ウィンドウと
#       プロセスが残り、記録に**拒否の往復**（`can_close_window` の判定）が現れることを見る。
#       続けて**対照**（拒否しない委譲先）で同じ操作を行い、ウィンドウが消えプロセスも終了する
#       ことを見る。
#
# # 証拠の形（何を証明し、何を証明しないか）
#
#   - ウィンドウの**数と識別子**は `xwininfo -root -tree` の観測（1.5 / 10.3 / 10.4 と同じ実装を
#     `scripts/lib/x11-window.sh` から使う）である。したがって「増えた 1 枚」が**どのラベル**の
#     ウィンドウかは X からは分からない。**ラベルは診断記録（6.1 の `ウィンドウを開いた:
#     label=…`）で示す** — これが「新しいウィンドウが `doc-*` である」ことの根拠である。
#   - 拒否の往復も**診断記録**が出す（`can_close_window` の 1 行。7.6）。X の観測だけでは
#     「拒否されたから閉じなかった」と「閉鎖要求が届かなかった」を区別できないので、
#     **両方を要求する**（ウィンドウが残っていること＋拒否の行が現れていること）。
#   - **配布物（既定のビルド）は環境変数を読まない**（9.7 の片付けの規約）ので、(a)/(b) は
#     配布物で測り、(c) は検証用の形で測る。段はその 2 つを別の引数で受け取る。
#
# # 記録の読み方
#
# 記録は**起動の前に消さない**（起動中のアプリは開いたファイルへ書き続けるため、消すと行が
# 失われる）。代わりに**各段の開始時の行数**を取り、それ以降の行だけを調べる（前の段や
# 前の CI 段の行で偽の成功をしない）。記録がローテーションする規模（8 MB）には達しない前提である。
#
# # 前提
#   - DISPLAY が設定されていること。CI（Linux ランナー）では `xvfb-run` が設定する。
#   - `xwininfo`（x11-utils）、`ps`、`pgrep`、`python3`（libX11 を ctypes で呼ぶ）が必要。
#   - **単一インスタンスはプラットフォームの機構に依存する**（Linux は D-Bus のセッションバス。
#     design.md の既知のリスク）。バスが無ければ 2 つ目は常駐しないという検査が成立しないので、
#     この段はその環境では失敗する（黙って通さない）。CI は段の外側で `dbus-run-session` を
#     使えるなら使う（`ci.yml` の 10.5 節を参照）。
#
# # 終了コード
#   0 = 3 つの経路すべてが期待どおり / 1 = 検査失敗 / 2 = 入力が使えない
#
# SC1091 / SC2154: 置き場は**同じリポジトリのファイル**であり、`x11_log` / `x11_poll_sleep` /
# `x11_window_ids` などはそこで代入される。qlty は検査対象を一時ディレクトリへ写してから shellcheck に
# かけるため、shellcheck は置き場をたどれず「たどれない・未代入」と報告する（実際の実行では
# `$0` からの相対で解決する）。契約は置き場の doc に 1 つだけ書いてある。
# shellcheck disable=SC1091,SC2154
set -eu

_x11_lib_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
. "$_x11_lib_dir/lib/x11-window.sh"

usage() {
  echo "使い方: check-multi-window.sh <配布物> <検証用の実行ファイル> <タイトル部分文字列> [タイムアウト秒] [最小幅] [最小高さ] <記録ファイル> <ドキュメント位置> <拒否ラベル>" >&2
  exit 2
}

[ "$#" -ge 9 ] || usage

app=$1
verify_app=$2
title=$3
timeout_secs=${4:-60}
min_w=${5:-100}
min_h=${6:-100}
record=$7
document=$8
deny_label=$9

case "$timeout_secs" in
  ''|*[!0-9]*)
    echo "NG: タイムアウト秒が整数ではありません: $timeout_secs" >&2
    exit 2
    ;;
esac

if [ -z "$record" ]; then
  echo "NG: 記録ファイルが指定されていません" >&2
  exit 2
fi
if [ ! -f "$document" ]; then
  echo "NG: ドキュメント位置が実在しません: $document（7.7 は読まないが、渡す位置は実在させる）" >&2
  exit 2
fi
if [ -z "$deny_label" ]; then
  echo "NG: 拒否ラベルが空です" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "NG: python3 が見つかりません（WM_DELETE_WINDOW の送信に使います）" >&2
  exit 2
fi

x11_require_app "$app"
x11_require_app "$verify_app"
x11_require_environment

# **検証用の引き金が親の環境から漏れないようにする。** 配布物は読まないが、検証用の形の対照
# （拒否しない側）へ `JXCEL_VERIFICATION_DENY_CLOSE` が漏れると、対照が成立しなくなる。
unset JXCEL_VERIFICATION_DENY_CLOSE 2>/dev/null || true
unset JXCEL_VERIFICATION_INITIAL_SCREEN 2>/dev/null || true
unset JXCEL_VERIFICATION_EXIT_AFTER_MS 2>/dev/null || true

x11_install_cleanup_trap
# 検出粒度（0.1 秒。分数秒を受け付けない sleep では 1 秒へ退避する）。
x11_pick_poll_sleep
work=$(mktemp -d)
launched_pids=""
cleaned=0
exit_status=0
deny_window_ids=""

# 起動したもの（この段の主役以外も含む）を必ず片付ける。**主役は `x11_pid`**（置き場のトラップが
# 木ごと終了し、出力の控えを消す）。2 つ目の起動のように自分で終わるものも、期限までに
# 終わらなければここで終了する。**TERM に応じないプロセスでこの関数が止まらないよう、猶予の後は
# SIGKILL する**（置き場の `x11_cleanup` と同じ判断）。
# shellcheck disable=SC2329 # 下の trap から呼ばれる
cleanup() {
  [ "$cleaned" = 1 ] && return 0
  cleaned=1
  for _p in $launched_pids; do
    if kill -0 "$_p" 2>/dev/null; then
      x11_kill_tree "$_p"
      _n=0
      while [ "$_n" -lt 10 ] && kill -0 "$_p" 2>/dev/null; do
        sleep 0.5
        _n=$((_n + 1))
      done
      kill -9 "$_p" 2>/dev/null || true
      wait "$_p" 2>/dev/null || true
    fi
  done
  x11_cleanup
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

report_failure() {
  echo "NG: $1" >&2
  x11_dump_tail "アプリの出力（末尾）" "${x11_log:-}"
  if [ -f "$record" ]; then
    echo "--- 診断記録（末尾）: $record ---" >&2
    tail -n 40 "$record" >&2
  else
    echo "--- 診断記録: $record（存在しません） ---" >&2
  fi
  exit 1
}

# 記録の現在の行数（無ければ 0）。各段の開始時に取り、それ以降の行だけを調べる。
record_lines() {
  if [ -f "$record" ]; then
    wc -l < "$record" | tr -d ' '
  else
    echo 0
  fi
}

# 記録の `<開始行数>` より後から正規表現に一致する最初の行を出す（無ければ空で終了 1）。
record_find() {
  _from=$1
  _pattern=$2
  [ -f "$record" ] || return 1
  tail -n "+$((_from + 1))" "$record" 2>/dev/null | grep -E "$_pattern" | head -n 1
}

# 記録の `<開始行数>` より後に正規表現が現れた回数。
record_count() {
  _from=$1
  _pattern=$2
  if [ ! -f "$record" ]; then
    echo 0
    return 0
  fi
  tail -n "+$((_from + 1))" "$record" 2>/dev/null | awk -v p="$_pattern" '$0 ~ p { n += 1 } END { print n + 0 }'
}

# 記録に一致行が現れるのを待つ（最大 <秒>）。現れれば行を出して終了 0。
wait_record() {
  _from=$1
  _pattern=$2
  _seconds=$3
  _deadline=$(( $(date +%s) + _seconds ))
  while :; do
    _line=$(record_find "$_from" "$_pattern" || true)
    if [ -n "$_line" ]; then
      printf '%s\n' "$_line"
      return 0
    fi
    if [ "$(date +%s)" -ge "$_deadline" ]; then
      return 1
    fi
    sleep "$x11_poll_sleep"
  done
}

# タイトルが一致して最小寸法を満たすウィンドウを観測する（置き場の実装）。
observe_windows() { x11_collect_windows "$title" "$min_w" "$min_h"; }

# ウィンドウ数が <期待する数> 以上になるのを待つ。期限を超えたら終了 1。
wait_for_windows() {
  _expected=$1
  _deadline=$(( $(date +%s) + $2 ))
  while :; do
    observe_windows
    if [ "$x11_window_count" -ge "$_expected" ]; then
      return 0
    fi
    if [ "$(date +%s)" -ge "$_deadline" ]; then
      return 1
    fi
    sleep "$x11_poll_sleep"
  done
}

# ウィンドウが 1 枚も無くなるのを待つ。期限を超えたら終了 1。
wait_for_no_windows() {
  _deadline=$(( $(date +%s) + $1 ))
  while :; do
    observe_windows
    if [ "$x11_window_count" -eq 0 ]; then
      return 0
    fi
    if [ "$(date +%s)" -ge "$_deadline" ]; then
      return 1
    fi
    sleep "$x11_poll_sleep"
  done
}

# 常駐しているアプリのインスタンス数。**ゾンビは数えない** — 終了したプロセスが名前を
# 保持しても常駐してはいない（回収前の子が名前に残る）。実行ファイル名は題名と同じ前提である
# （このアプリでは実行ファイル名もウィンドウタイトルも `jxcel`）。
count_instances() {
  ps -eo stat=,comm= 2>/dev/null |
    awk -v name="$title" '$1 !~ /^Z/ && $2 == name { n += 1 } END { print n + 0 }'
}

# プロセスが生きているか。**ゾンビは「生きていない」**（終了はしているが回収待ち）。
process_alive() {
  _state=$(sed -n 's/^[^)]*) \([A-Z]\).*/\1/p' "/proc/$1/stat" 2>/dev/null || true)
  [ -n "$_state" ] && [ "$_state" != "Z" ]
}

# プロセスの終了を待ち、**回収して**終了コードを `exit_status` に入れる（期限を超えたら終了 1）。
wait_for_exit() {
  _deadline=$(( $(date +%s) + $2 ))
  while process_alive "$1"; do
    if [ "$(date +%s)" -ge "$_deadline" ]; then
      return 1
    fi
    sleep "$x11_poll_sleep"
  done
  exit_status=0
  wait "$1" || exit_status=$?
  return 0
}

# 自分で終了しないプロセス木を止め、消えるまで待つ。**猶予（TERM）の後に SIGKILL する** —
# TERM に応じないプロセスでこの検査が止まらないようにするためである。
stop_instance() {
  x11_kill_tree "$1"
  _n=0
  while [ "$_n" -lt 20 ] && process_alive "$1"; do
    sleep 0.5
    _n=$((_n + 1))
  done
  if process_alive "$1"; then
    kill -9 "$1" 2>/dev/null || true
    _n=0
    while [ "$_n" -lt 10 ] && process_alive "$1"; do
      sleep 0.5
      _n=$((_n + 1))
    done
  fi
  wait "$1" 2>/dev/null || true
  if process_alive "$1"; then
    return 1
  fi
  return 0
}

# ウィンドウへ `WM_DELETE_WINDOW` を送る（7.6 と同じ `XSendEvent`。`event_mask=0` で
# **そのウィンドウを作ったクライアントへ**届く。ウィンドウマネージャのフレームではない）。
x11_send_delete() {
  python3 - "$@" <<'PY'
import ctypes
import sys
from ctypes import POINTER, Union, Structure, byref, c_char_p, c_int, c_long, c_ulong, c_void_p

CLIENT_MESSAGE = 33
WM_PROTOCOLS = b"WM_PROTOCOLS"
WM_DELETE_WINDOW = b"WM_DELETE_WINDOW"


class XClientMessageEvent(Structure):
    # X11 の XClientMessageEvent。data は 5 つの long の共用体として扱う。
    _fields_ = [
        ("type", c_int),
        ("serial", c_ulong),
        ("send_event", c_int),
        ("display", c_void_p),
        ("window", c_ulong),
        ("message_type", c_ulong),
        ("format", c_int),
        ("data", c_long * 5),
    ]


class XEvent(Union):
    # XEvent の大きさ（64 ビットでは 192 バイト = long 24 個）に合わせる。
    _fields_ = [("type", c_int), ("xclient", XClientMessageEvent), ("pad", c_long * 24)]


def main() -> int:
    windows = [int(argument, 16) for argument in sys.argv[1:]]
    if not windows:
        print("NG: 送信先のウィンドウがありません", file=sys.stderr)
        return 2
    x11 = ctypes.CDLL("libX11.so.6")
    x11.XOpenDisplay.restype = c_void_p
    x11.XOpenDisplay.argtypes = [c_char_p]
    x11.XInternAtom.restype = c_ulong
    x11.XInternAtom.argtypes = [c_void_p, c_char_p, c_int]
    x11.XSendEvent.restype = c_int
    x11.XSendEvent.argtypes = [c_void_p, c_ulong, c_int, c_long, POINTER(XEvent)]
    x11.XFlush.argtypes = [c_void_p]
    x11.XCloseDisplay.argtypes = [c_void_p]

    display = x11.XOpenDisplay(None)
    if not display:
        print("NG: DISPLAY を開けません", file=sys.stderr)
        return 2
    protocols = x11.XInternAtom(display, WM_PROTOCOLS, False)
    delete = x11.XInternAtom(display, WM_DELETE_WINDOW, False)
    for window in windows:
        event = XEvent()
        event.xclient.type = CLIENT_MESSAGE
        event.xclient.serial = 0
        event.xclient.send_event = 1
        event.xclient.display = display
        event.xclient.window = window
        event.xclient.message_type = protocols
        event.xclient.format = 32
        event.xclient.data[0] = delete
        event.xclient.data[1] = 0
        if not x11.XSendEvent(display, window, 0, 0, byref(event)):
            print(f"NG: 送信できませんでした: window=0x{window:x}", file=sys.stderr)
            return 1
    x11.XFlush(display)
    x11.XCloseDisplay(display)
    return 0


raise SystemExit(main())
PY
}

echo "診断記録: $record"
echo "配布物: $app"
echo "検証用の形: $verify_app"
echo "ドキュメント位置: $document"

# 検証の前に残存インスタンスが無いこと。あると単一インスタンスの検査が成立しない
# （2 つ目の起動がそちらへ引き継がれ、この段のインスタンスには届かない）。
initial_instances=$(count_instances)
if [ "$initial_instances" -ne 0 ]; then
  report_failure "検証の前に別のインスタンスが ${initial_instances} 件動いています（この段は単一インスタンスを測るので成立しません）"
fi

# ---------------------------------------------------------------------------
# (a)(b) 配布物: 起動中の再実行と、ドキュメントを開いたときの新しいウィンドウ
# ---------------------------------------------------------------------------

phase_ab=$(record_lines)

echo "検証 (a)(b): 1 つ目として配布物を起動する（引数なし → ドキュメントを関連付けないウィンドウ）"
GDK_BACKEND=x11 nohup "$app" >"$x11_log" 2>&1 &
x11_pid=$!
launched_pids="$launched_pids $x11_pid"

if ! wait_for_windows 1 "$timeout_secs"; then
  report_failure "1 つ目のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
fi

first_label=$(wait_record "$phase_ab" "ウィンドウを開いた: label=empty-1 " 10 || true)
if [ -z "$first_label" ]; then
  report_failure "1 つ目のウィンドウが label=empty-1（ドキュメントを関連付けないウィンドウ）として記録に現れません"
fi
observe_windows
first_count=$x11_window_count
first_ids=$x11_window_ids
echo "検証 (a): 1 つ目の起動: pid=${x11_pid} ウィンドウ数=${first_count} 識別子=$(printf '%s' "$first_ids" | tr '\n' ' ')"
echo "検証 (b): 1 つ目のウィンドウのラベル: $(printf '%s' "$first_label" | sed -n 's/.*label=\([^ ]*\).*/\1/p')"

echo "検証 (a)(b): 同じ配布物を再度実行する（ドキュメント位置を渡す → 単一インスタンスが引数を引き渡す）"
GDK_BACKEND=x11 nohup "$app" "$document" >"$work/second.log" 2>&1 &
second_pid=$!
launched_pids="$launched_pids $second_pid"

if ! wait_for_exit "$second_pid" "$timeout_secs"; then
  report_failure "2 つ目の起動（pid=${second_pid}）が ${timeout_secs} 秒以内に終了しません（2 つ目が常駐している。単一インスタンスが成立していない）"
fi
if [ "$exit_status" -ne 0 ]; then
  report_failure "2 つ目の起動が終了コード ${exit_status} で終わった（0 であるべき。単一インスタンスの引き継ぎの挙動）"
fi
echo "検証 (a): 2 つ目の起動は終了コード 0 で終わった（常駐しない）"

after_instances=$(count_instances)
if [ "$after_instances" -ne 1 ]; then
  report_failure "常駐しているインスタンスが ${after_instances} 件になった（1 件であるべき。2 つ目は常駐してはならない）"
fi
echo "検証 (a): 常駐インスタンス数=${after_instances}（2 つ目は常駐していない）"

if ! wait_for_windows "$((first_count + 1))" "$timeout_secs"; then
  report_failure "2 つ目の起動の後、ウィンドウ数が $((first_count + 1)) 以上になりませんでした（引き継いだ側がウィンドウを提示していない）"
fi
observe_windows
second_count=$x11_window_count

# **既存のウィンドウが閉じていないこと**（要件 2.3）。識別子の集合で見る。
for _id in $first_ids; do
  if ! printf '%s\n' "$x11_window_ids" | grep -q -x -e "$_id"; then
    report_failure "ドキュメントを開いたときに既存のウィンドウ ${_id} が消えた（既存のウィンドウを閉じてはならない）"
  fi
done

# 新しいウィンドウが**ドキュメント付き**（`doc-<連番>`）であること（6.1 のラベル規約）。
new_label_line=$(wait_record "$phase_ab" "ウィンドウを開いた: label=doc-" 10 || true)
if [ -z "$new_label_line" ]; then
  report_failure "新しいウィンドウが label=doc-*（ドキュメント付き）として記録に現れません"
fi
new_label=$(printf '%s' "$new_label_line" | sed -n 's/.*label=\([^ ]*\).*/\1/p')
handover_line=$(wait_record "$phase_ab" "二重起動を引き継ぎました.*ドキュメント要求 " 10 || true)
if [ -z "$handover_line" ]; then
  report_failure "引き継ぎの行（二重起動を引き継ぎました … ドキュメント要求 …）が記録に現れません"
fi
echo "検証 (b): ウィンドウ数=${second_count}（1 つ目=${first_count} から 1 増えた） 既存の識別子=$(printf '%s' "$first_ids" | tr '\n' ' ') は残っている"
echo "検証 (b): 新しいウィンドウのラベル=${new_label}（doc-* ＝ ドキュメント付き）"
echo "検証 (b): 記録（引き継ぎ）: ${handover_line}"

# **引数なしの 2 つ目の起動**: 5.1 / 6.1 は既にあるウィンドウを前面に出すだけで、新しい
# ウィンドウを作らない。ウィンドウが増えないことを数秒間確かめる。
echo "検証 (a): 引数なしで再度実行する（既にあるウィンドウを前面に出すだけで、新しいウィンドウを作らない）"
phase_arga=$(record_lines)
GDK_BACKEND=x11 nohup "$app" >"$work/third.log" 2>&1 &
third_pid=$!
launched_pids="$launched_pids $third_pid"

if ! wait_for_exit "$third_pid" "$timeout_secs"; then
  report_failure "引数なしの 2 つ目の起動（pid=${third_pid}）が ${timeout_secs} 秒以内に終了しません"
fi
if [ "$exit_status" -ne 0 ]; then
  report_failure "引数なしの 2 つ目の起動が終了コード ${exit_status} で終わった（0 であるべき）"
fi
arga_line=$(wait_record "$phase_arga" "二重起動を引き継ぎました.*ドキュメント要求なし" 10 || true)
if [ -z "$arga_line" ]; then
  report_failure "引数なしの引き継ぎの行（… ドキュメント要求なし）が記録に現れません"
fi
# 増えないことを数秒間見る（増えるならこの間に現れる）。
_settle_deadline=$(( $(date +%s) + 3 ))
while [ "$(date +%s)" -lt "$_settle_deadline" ]; do
  observe_windows
  if [ "$x11_window_count" -ne "$second_count" ]; then
    report_failure "引数なしの 2 つ目の起動でウィンドウ数が ${second_count} から ${x11_window_count} に変化した（新しいウィンドウを作ってはならない）"
  fi
  sleep "$x11_poll_sleep"
done
observe_windows
echo "検証 (a): 引数なしの 2 つ目の起動も終了コード 0 で終わり、ウィンドウ数=${x11_window_count} のまま変わらない"
echo "検証 (a): 記録（引数なしの引き継ぎ）: ${arga_line}"

# 配布物のインスタンスを片付けてから (c) へ移る（同じプロファイルを奪い合わないように）。
if ! stop_instance "$x11_pid"; then
  report_failure "1 つ目のインスタンス（pid=${x11_pid}）を終了できませんでした"
fi
x11_pid=""
if ! wait_for_no_windows "$timeout_secs"; then
  report_failure "配布物のウィンドウが ${timeout_secs} 秒以内に消えませんでした"
fi

# ---------------------------------------------------------------------------
# (c) 検証用の形: 拒否される委譲先では閉じず、拒否しない委譲先では閉じる（要件 2.6）
# ---------------------------------------------------------------------------

phase_deny=$(record_lines)

echo "検証 (c): 検証用の形を起動する（JXCEL_VERIFICATION_DENY_CLOSE=${deny_label} → ${deny_label} の終了を拒否する委譲先）"
JXCEL_VERIFICATION_DENY_CLOSE=$deny_label GDK_BACKEND=x11 nohup "$verify_app" "$document" >"$work/deny.log" 2>&1 &
x11_pid=$!
launched_pids="$launched_pids $x11_pid"

if ! wait_for_windows 1 "$timeout_secs"; then
  report_failure "検証用の形のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
fi
if [ -z "$(wait_record "$phase_deny" "ウィンドウを開いた: label=${deny_label} " 10 || true)" ]; then
  report_failure "検証用の形が ${deny_label} というラベルのウィンドウを開いていません（拒否の対象が存在しない）"
fi
# フロントエンドの購読（7.6）が載っていることを待つ。初回描画の成立行は通知が描画フレームの
# 中から出るので、これが現れていれば購読も張られている（`src/main.tsx` の順序）。
if [ -z "$(wait_record "$phase_deny" "初回描画が成立した: label=${deny_label} |初回描画は成立したがソフトウェアラスタライザ経由である: label=${deny_label} " 20 || true)" ]; then
  report_failure "初回描画の成立行が現れません（購読が張られたことを確認できない）"
fi

observe_windows
deny_window_ids=$x11_window_ids
echo "検証 (c): ウィンドウ数=${x11_window_count} 識別子=$(printf '%s' "$deny_window_ids" | tr '\n' ' ')"
echo "検証 (c): WM_DELETE_WINDOW を送る（拒否される委譲先）"
# 識別子は空白区切りで複数の引数に分ける（意図した分割）。
# shellcheck disable=SC2086
if ! x11_send_delete $deny_window_ids; then
  report_failure "WM_DELETE_WINDOW を送れませんでした"
fi

# 拒否の往復が記録に現れ、ウィンドウとプロセスが残ることを要求する。
deny_line=$(wait_record "$phase_deny" "can_close_window: 呼び出し元ウィンドウ = ${deny_label} / 判定 = 拒否" 10 || true)
if [ -z "$deny_line" ]; then
  report_failure "拒否の往復（can_close_window の判定 = 拒否）が記録に現れません"
fi
# 数秒間、ウィンドウとプロセスが残ることを見る（遅れて閉じる場合も捕まえる）。
_settle_deadline=$(( $(date +%s) + 3 ))
while [ "$(date +%s)" -lt "$_settle_deadline" ]; do
  observe_windows
  if [ "$x11_window_count" -lt 1 ]; then
    report_failure "拒否されたはずのウィンドウが閉じた（委譲先の拒否が効いていない）"
  fi
  for _id in $deny_window_ids; do
    if ! printf '%s\n' "$x11_window_ids" | grep -q -x -e "$_id"; then
      report_failure "拒否されたはずのウィンドウ ${_id} が消えた（別のウィンドウが残っているだけである）"
    fi
  done
  if ! process_alive "$x11_pid"; then
    report_failure "拒否された後にプロセス（pid=${x11_pid}）が終了した"
  fi
  sleep "$x11_poll_sleep"
done
deny_round_trips=$(record_count "$phase_deny" "can_close_window: 呼び出し元ウィンドウ = ${deny_label} ")
if [ "$deny_round_trips" -ne 1 ]; then
  report_failure "1 回の終了要求に対する拒否の往復が ${deny_round_trips} 回だった（1 回であるべき。7.6 の実測）"
fi
echo "検証 (c): 拒否 — ウィンドウは残り（$(printf '%s' "$deny_window_ids" | tr '\n' ' ')）、プロセス pid=${x11_pid} も生存"
echo "検証 (c): 記録（拒否の往復）: ${deny_line}"
echo "検証 (c): 拒否の往復の回数=${deny_round_trips}（1 回の終了要求につき 1 回）"

if ! stop_instance "$x11_pid"; then
  report_failure "拒否の検証の後始末でインスタンス（pid=${x11_pid}）を終了できませんでした"
fi
x11_pid=""
if ! wait_for_no_windows "$timeout_secs"; then
  report_failure "拒否の検証の後始末でウィンドウが消えませんでした"
fi

phase_allow=$(record_lines)

echo "検証 (c): 対照 — 検証用の形を起動する（JXCEL_VERIFICATION_DENY_CLOSE なし → 常に許可する委譲先）"
GDK_BACKEND=x11 nohup "$verify_app" "$document" >"$work/allow.log" 2>&1 &
x11_pid=$!
launched_pids="$launched_pids $x11_pid"

if ! wait_for_windows 1 "$timeout_secs"; then
  report_failure "対照のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
fi
if [ -z "$(wait_record "$phase_allow" "初回描画が成立した: label=${deny_label} |初回描画は成立したがソフトウェアラスタライザ経由である: label=${deny_label} " 20 || true)" ]; then
  report_failure "対照の初回描画の成立行が現れません（購読が張られたことを確認できない）"
fi
observe_windows
allow_window_ids=$x11_window_ids
echo "検証 (c): 対照のウィンドウ数=${x11_window_count} 識別子=$(printf '%s' "$allow_window_ids" | tr '\n' ' ')"
echo "検証 (c): WM_DELETE_WINDOW を送る（拒否しない委譲先）"
# 識別子は空白区切りで複数の引数に分ける（意図した分割）。
# shellcheck disable=SC2086
if ! x11_send_delete $allow_window_ids; then
  report_failure "WM_DELETE_WINDOW を送れませんでした（対照）"
fi
if ! wait_for_no_windows "$timeout_secs"; then
  report_failure "拒否しない委譲先で WM_DELETE_WINDOW を送ったが、ウィンドウが消えませんでした（許可が効いていない）"
fi
if ! wait_for_exit "$x11_pid" "$timeout_secs"; then
  report_failure "最後のウィンドウが閉じた後にプロセスが終了しませんでした（要件 2.8）"
fi
if [ "$exit_status" -ne 0 ]; then
  report_failure "対照のプロセスが終了コード ${exit_status} で終わった（0 であるべき）"
fi
allow_line=$(record_find "$phase_allow" "判定 = 許可" | head -n 1 || true)
echo "検証 (c): 対照 — ウィンドウは消え（$(printf '%s' "$allow_window_ids" | tr '\n' ' ')）、プロセスは終了コード 0 で終わった"
echo "検証 (c): 記録（許可の往復）: ${allow_line:-（記録なし）}"
x11_pid=""

# ---------------------------------------------------------------------------
# 後始末: 残存プロセスが無いことをこの段の出力で確かめる
# ---------------------------------------------------------------------------

for _p in $launched_pids; do
  if process_alive "$_p"; then
    if ! stop_instance "$_p"; then
      report_failure "残存プロセス（pid=${_p}）を終了できませんでした"
    fi
  fi
done
if ! wait_for_no_windows "$timeout_secs"; then
  report_failure "後始末の後にウィンドウが残っています"
fi
remaining=$(count_instances)
if [ "$remaining" -ne 0 ]; then
  report_failure "後始末の後に常駐しているインスタンスが ${remaining} 件あります"
fi
echo "検証 (後始末): 常駐インスタンス数=0（残存プロセスなし）"

echo "OK: 単一インスタンス化と複数ウィンドウの 3 経路（(a) 再実行が常駐せずウィンドウが増える / (b) 新しいウィンドウが doc-* で既存が閉じない / (c) 拒否では閉じず許可では閉じる）を検証した"

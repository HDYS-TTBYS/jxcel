#!/bin/sh
# 初回描画が期限までに成立しなかったときの**提示**を X11 上で検査する
# （要件 10.2。tasks.md 8.2 が置いた検証専用の引き金 `JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT` を
# 駆動する）。
#
# 使い方:
#   check-no-paint.sh <実行ファイル> <タイトル部分文字列> <診断記録> <モード: suppressed|ignored> \
#                     [タイムアウト秒] [最小幅] [最小高さ]
#
#   - 実行ファイル         : モード `suppressed` では `--features verification-triggers` の
#                            **検証用の形**、モード `ignored` では**配布物**（既定のビルド）
#   - タイトル部分文字列   : ウィンドウタイトルに含まれるべき文字列（`jxcel`）
#   - 診断記録             : アプリの診断記録（4.4 の保存先の `jxcel.log`）。**起動の前に消す**
#   - モード               :
#       suppressed — フロントエンドの通知を抑止し（検証用の形だけが読む）、**期限超過の経路**を
#                    起こして、判定・題名・全画面の注意書き・8.3 の印を要求する（0 か 1 を返す）
#       ignored    — 同じ環境変数を**配布物**へ渡し、抑止されないこと（通知が通り、期限超過の
#                    経路に入らないこと）を要求する（**負の対照**。0 か 1 を返す）
#   - タイムアウト秒       : 既定 60。ウィンドウの出現を待つ上限
#   - 最小幅 / 最小高さ    : 既定 100。GTK の 20x20 程度の補助ウィンドウを数えないため
#
# # モード `suppressed` が証明すること（要件 10.2）
#
# 1. **通知が抑止されたこと** — 記録に `[検証] 描画の通知を抑止した` が現れる（引き金が働いた）。
# 2. **期限超過の判定が記録されたこと** — 8.2 が `初回描画が成立しなかった: label=… 期限=3000 ms
#    経過=… 画面=(報告なし) 診断情報の保存先=…` を残す。**通知の抑止を外して同じ検査をすると
#    成立行が出るので、この行は「通知が届かなかった」ことの結果である。**
# 3. **ネイティブの題名が変わったこと** — X のウィンドウツリーに、**同じ識別子の**ウィンドウが
#    `描画が成立しませんでした`・対象のラベル・診断の保存先を含む題名で現れる（OS が描くので
#    WebView の描画が壊れていても利用者に届く経路である）。
# 4. **全画面の注意書きが提示されたこと** — ウィンドウの**画素**を読んで確かめる。注意書きは
#    `inset: 0` の要素で、背景は `#b00020`（rgb(176, 0, 32)）、本文は白である。したがって
#    「ウィンドウのほぼ全面が注意書きの赤で、白の文字の画素がある」ことを要求する。
#    **`eval` が成功したという記録は証拠にしない** — それは「差し込んだ」ことしか述べず、
#    「描かれた」ことを述べない（10.4 が「要求した識別子」を証拠にしなかったのと同じ理由）。
# 5. **8.3 の印が残ったこと** — 設定の実体（診断の保存先の親の `settings.json`）に
#    `"render.fallback": true` が現れる（次の起動が代替経路を試みる前提そのものである）。
#    加えて、**起動の前に印が立っていなかった場合**は、この実行で値が変わったことの記録
#    （`設定変更を全ウィンドウへ通知する（キー: render.fallback）`。設定ストアは同一値の
#    書き込みを通知しないので、この行は遷移の証拠である）も要求する。既に立っていた場合は
#    遷移が観測できないため値を要求するに留め、その事実を出力に残す。
# 6. **アプリが止まらないこと** — 判定の後もプロセスが生きており、そのあと**終了コード 0 で
#    終わる**（期限超過の経路はアプリを終わらせない。要件 10.2 の「提示して動作を続ける」）。
#
# # モード `ignored`（負の対照）が証明すること
#
# 配布物は `JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT` を読まない（8.2 の片付け。非既定の feature の
# 下にしか無い）。したがって:
#
#   - 記録に**成立行**が現れる（抑止されていない）
#   - 期限（3 秒）を過ぎても `初回描画が成立しなかった` は現れない
#   - 題名は変わらず、**注意書きの赤は画面の大半を占めない**（画素の判定が空回りしていないこと
#     の対照でもある）
#   - プロセスは動き続ける
#
# **この 4 つが揃ってはじめて、モード `suppressed` の赤は「抑止した結果」だと言える。**
#
# # 前提
#   - DISPLAY が設定されていること（CI の Linux ランナーでは `xvfb-run` が設定する）。
#   - `xwininfo`（x11-utils）、`python3`、`libX11`（`ctypes` で呼ぶ）、`strings`（binutils）が要る。
#     画素を読む道具（`xwd` / ImageMagick / スクリーンショット）はランナーに無いため、
#     **libX11 を `ctypes` で直接呼ぶ**（10.5 が `XSendEvent` に使っているのと同じ手段）。
#
# # 終了コード
#   0 = モードの主張がすべて成立 / 1 = 検査失敗 / 2 = 入力が使えない（実行ファイル不在・
#   DISPLAY 不在・モードの綴り違い・python3 / libX11 不在・引き金を持たない実行ファイル・
#   同題名のウィンドウが既にある）
#
# 起動したプロセスは EXIT / INT / TERM のトラップで必ず片付ける（モード `suppressed` では
# 終了コード 0 を見たあとに、モード `ignored` では検査の終わりに片付ける）。
#
# SC1091 / SC2154: 置き場は同じリポジトリのファイルであり、`x11_log` / `x11_poll_sleep` /
# `x11_window_ids` などはそこで代入される（qlty は検査対象を一時ディレクトリへ写してから
# **shellcheck** にかけるため（下の disable 行）、置き場をたどれず「たどれない・未代入」と
# 報告する。契約は置き場の doc に 1 つだけ書いてある。
# shellcheck disable=SC1091,SC2154
set -eu

_x11_lib_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
. "$_x11_lib_dir/lib/x11-window.sh"

usage() {
  echo "使い方: check-no-paint.sh <実行ファイル> <タイトル部分文字列> <診断記録> <モード: suppressed|ignored> [タイムアウト秒] [最小幅] [最小高さ]" >&2
  exit 2
}

[ "$#" -ge 4 ] || usage

app=$1
title=$2
record=$3
mode=$4
timeout_secs=${5:-60}
min_w=${6:-100}
min_h=${7:-100}

# モード `suppressed` で、判定とそのあとの観測を終えるまでの待ち（ミリ秒）。検査の間に
# アプリが自分で終わらないよう十分大きく取る（片付けはトラップが行う）。
suppress_exit_ms=20000

# 8.2 の期限（`app_shell::render::FIRST_PAINT_DEADLINE` は 3 秒）。題名と記録の要求に使う。
first_paint_deadline_ms=3000
# 抑止したまま待つ猶予（秒）。期限（3 秒）を確実に超える。
suppress_grace_secs=20

# 注意書きの題名に含まれる語（`watchdog::missing_paint_title`）。
notice_title_word='描画が成立しませんでした'

case "$timeout_secs" in
  ''|*[!0-9]*)
    echo "NG: タイムアウト秒が整数ではありません: $timeout_secs" >&2
    exit 2
    ;;
esac
case "$mode" in
  suppressed | ignored) ;;
  *)
    echo "NG: モードは suppressed か ignored です: $mode" >&2
    exit 2
    ;;
esac
if [ -z "$record" ]; then
  echo "NG: 診断記録が指定されていません" >&2
  exit 2
fi

x11_require_app "$app"
x11_require_environment
if ! command -v python3 >/dev/null 2>&1; then
  echo "NG: python3 が見つかりません（ウィンドウの画素の読み取りに使います）" >&2
  exit 2
fi
if ! python3 -c 'import ctypes.util, sys; sys.exit(0 if (ctypes.util.find_library("X11") or "libX11.so.6") else 1)' >/dev/null 2>&1; then
  echo "NG: libX11 を見つけられません（ctypes で画素を読みます）" >&2
  exit 2
fi

# 8.3 の印は設定の実体（4.1 の `settings.json`）に現れる。**この検査器は Linux 専用である**
# （X11 の観測と、GTK のウィンドウを要求する）。Linux の診断の保存先はアプリケーションデータ
# 領域の下の `logs` なので（4.4 の解決）、設定の実体は**その親**にある。macOS（ログは
# `$HOME/Library/Logs`、設定は `$HOME/Library/Application Support`）ではこの導出は成立しない
# ため、規約から外れていれば **exit 2** で落とす（**黙って別のファイルを見ない**）。
log_dir=$(dirname -- "$record")
case "$log_dir" in
  */logs)
    settings_file=$(dirname -- "$log_dir")/settings.json
    ;;
  *)
    echo "NG: 診断の保存先（$log_dir）が Linux の規約（…/{識別子}/logs）ではありません。この検査器は Linux 専用です" >&2
    exit 2
    ;;
esac

if [ "$mode" = suppressed ]; then
  if ! command -v strings >/dev/null 2>&1; then
    echo "NG: strings が見つかりません（実行ファイルが検証用の形かどうかの判定に使います）" >&2
    exit 2
  fi
  if ! strings -a "$app" | grep -q 'JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT'; then
    echo "NG: 実行ファイルに引き金 'JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT' がありません（--features verification-triggers の検証用の形を渡してください）" >&2
    exit 2
  fi
fi

report_failure() {
  echo "NG: $1" >&2
  echo "--- 診断記録（末尾）: $record ---" >&2
  if [ -f "$record" ]; then
    tail -n 40 "$record" >&2
  else
    echo "(記録ファイルがありません)" >&2
  fi
  echo "--- ウィンドウツリー（題名に '$title' を含む行） ---" >&2
  xwininfo -root -tree 2>/dev/null | grep -F "\"$title\"" >&2 || echo "(一致なし)" >&2
  x11_dump_tail "アプリの出力（末尾）" "$x11_log"
  exit 1
}

# 記録の <行番号> より後ろに現れた最後の一致行を出す（無ければ空）。
record_find_from() {
  if [ ! -f "$record" ]; then
    return 0
  fi
  tail -n "+$(( $1 + 1 ))" "$record" 2>/dev/null | grep -E "$2" | tail -n 1 || true
}

# ウィンドウの画素を数える。
#
#   x11_notice_pixels <ウィンドウ識別子> <格子の一辺>
#
# 標準出力へ `<注意書きの赤> <白> <その他> <合計>` を出す（空白区切り）。**アプリの自己申告を
# 使わない** — 描かれた画素そのものを読む。赤の判定は注意書きの背景 `#b00020`（rgb(176, 0, 32)）
# との近さ（±40 / ≤40 / ±30）で行う。**画面のアクセント色 `#c2185b`（rgb(194, 24, 91)）は
# この判定に入らない**（青成分の差が 30 を超える）ので、注意書きが無い画面を「赤い」と
# 誤認しない。格子は画面全体に均等に打つ（`inset: 0` の要素を覆っていることの確認である）。
x11_notice_pixels() {
  python3 - "$1" "$2" <<'PY'
import ctypes
import ctypes.util
import os
import sys


class Attr(ctypes.Structure):
    _fields_ = [
        ("x", ctypes.c_int), ("y", ctypes.c_int),
        ("width", ctypes.c_int), ("height", ctypes.c_int),
        ("border_width", ctypes.c_int), ("depth", ctypes.c_int),
        ("visual", ctypes.c_void_p), ("root", ctypes.c_ulong),
        ("class_", ctypes.c_int), ("bit_gravity", ctypes.c_int),
        ("win_gravity", ctypes.c_int), ("backing_store", ctypes.c_int),
        ("backing_planes", ctypes.c_ulong), ("backing_pixel", ctypes.c_ulong),
        ("save_under", ctypes.c_int), ("colormap", ctypes.c_ulong),
        ("map_installed", ctypes.c_int), ("map_state", ctypes.c_int),
        ("all_event_masks", ctypes.c_long), ("your_event_mask", ctypes.c_long),
        ("do_not_propagate_mask", ctypes.c_long), ("override_redirect", ctypes.c_int),
        ("screen", ctypes.c_void_p),
    ]


def main():
    display = os.environ.get("DISPLAY")
    if not display:
        print("DISPLAY がありません", file=sys.stderr)
        return 2
    library = ctypes.util.find_library("X11") or "libX11.so.6"
    x11 = ctypes.CDLL(library)
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XGetWindowAttributes.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(Attr)]
    x11.XGetImage.restype = ctypes.c_void_p
    x11.XGetImage.argtypes = [
        ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
        ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_int,
    ]
    x11.XGetPixel.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
    x11.XGetPixel.restype = ctypes.c_ulong
    x11.XDestroyImage.argtypes = [ctypes.c_void_p]

    window = int(sys.argv[1], 16)
    grid = int(sys.argv[2])
    dpy = x11.XOpenDisplay(display.encode())
    if not dpy:
        print("X サーバへ接続できません", file=sys.stderr)
        return 3
    attr = Attr()
    if not x11.XGetWindowAttributes(dpy, window, ctypes.byref(attr)):
        print(f"ウィンドウ {sys.argv[1]} の属性を読めません（既に無い）", file=sys.stderr)
        return 3
    if attr.width <= 0 or attr.height <= 0:
        print(f"ウィンドウ {sys.argv[1]} の寸法が不正です: {attr.width}x{attr.height}", file=sys.stderr)
        return 3
    image = x11.XGetImage(dpy, window, 0, 0, attr.width, attr.height, ~0, 2)
    if not image:
        print(f"ウィンドウ {sys.argv[1]} の画像を取得できません", file=sys.stderr)
        return 3
    notice = white = other = total = 0
    for iy in range(grid):
        for ix in range(grid):
            pixel = x11.XGetPixel(
                image,
                int((ix + 0.5) * attr.width / grid),
                int((iy + 0.5) * attr.height / grid),
            )
            r = (pixel >> 16) & 0xFF
            g = (pixel >> 8) & 0xFF
            b = pixel & 0xFF
            total += 1
            if abs(r - 176) <= 40 and g <= 40 and abs(b - 32) <= 30:
                notice += 1
            elif r >= 200 and g >= 200 and b >= 200:
                white += 1
            else:
                other += 1
    x11.XDestroyImage(image)
    print(f"{notice} {white} {other} {total}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
PY
}

# ウィンドウの出現を待つ（<秒>。成功なら 0）。
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

# タイトルが一致するウィンドウを 1 回観測する（開始前の確認に使う）。
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

# **前回の実行の行で成功しないよう、起動の前に消す。**
rm -f "$record"

heartbeat_re='初回描画が成立した: label=|初回描画は成立したがソフトウェアラスタライザ経由である: label='
no_paint_re='初回描画が成立しなかった: label='

if [ "$mode" = suppressed ]; then
  # -------------------------------------------------------------------
  # モード `suppressed`: 期限超過の経路を実測する
  # -------------------------------------------------------------------
  # 8.3 の印の状態を**起動の前に**読む（この実行で値が変わったかを見るため。5. で使う）。
  mark_before=0
  if [ -f "$settings_file" ] && tr -d ' \n' < "$settings_file" | grep -qF '"render.fallback":true'; then
    mark_before=1
  fi
  if [ "$mark_before" -eq 1 ]; then
    echo "検査の前提: 8.3 の印（render.fallback）は起動の前から立っています（遷移の記録は要求しません）"
  else
    echo "検査の前提: 8.3 の印（render.fallback）は起動の前に立っていません（この実行で立つことを要求します）"
  fi

  JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT=1 \
    JXCEL_VERIFICATION_EXIT_AFTER_MS="${suppress_exit_ms}" \
    GDK_BACKEND=x11 nohup "$app" >"$x11_log" 2>&1 &
  x11_pid=$!
  x11_pick_poll_sleep

  if ! wait_for_window "$timeout_secs"; then
    if [ "$x11_died" = 1 ]; then
      report_failure "起動したプロセスがウィンドウを出す前に終了しました（pid=${x11_pid}）"
    fi
    report_failure "タイトルに '$title' を含む ${min_w} x ${min_h} 以上のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
  fi
  # **集合が安定するまで待つ**（消えかけのウィンドウを自分のウィンドウと取り違えない。
  # `x11_settle_windows` の doc）。
  if ! x11_settle_windows "$title" "$min_w" "$min_h" 10; then
    report_failure "タイトルに '$title' を含むウィンドウの集合が安定しませんでした（消えかけのウィンドウが残っています）"
  fi
  if [ "$x11_window_count" -ne 1 ]; then
    report_failure "タイトルに '$title' を含むウィンドウが ${x11_window_count} 枚あります（この検査は 1 枚を対象にする）"
  fi
  first_ids=$x11_window_ids
  echo "OK: ウィンドウ '$title' ${x11_window_match} が現れました（識別子: $(printf '%s' "$first_ids" | tr '\n' ' ')）"

  # 1. 引き金が働いたこと。
  suppress_line=""
  no_paint_line=""
  deadline=$(( $(date +%s) + suppress_grace_secs ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    suppress_line=$(record_find_from 0 '\[検証\] 描画の通知を抑止した')
    no_paint_line=$(record_find_from 0 "$no_paint_re")
    if [ -n "$no_paint_line" ]; then
      break
    fi
    sleep "$x11_poll_sleep"
  done
  if [ -z "$suppress_line" ]; then
    report_failure "抑止の記録（'[検証] 描画の通知を抑止した'）が現れませんでした（引き金が働いていない）"
  fi
  if [ -z "$no_paint_line" ]; then
    recorded=$(record_find_from 0 "$heartbeat_re")
    report_failure "8.2 が期限超過を記録しませんでした（'${no_paint_re}' が現れない。成立行: ${recorded:-（なし）}）"
  fi
  # 2. 判定の中身（期限内に通知が届いていないこと）。
  if ! printf '%s\n' "$no_paint_line" | grep -qF "期限=${first_paint_deadline_ms} ms"; then
    report_failure "期限超過の記録の期限が ${first_paint_deadline_ms} ms ではありません: ${no_paint_line}"
  fi
  if ! printf '%s\n' "$no_paint_line" | grep -qF '画面=(報告なし)'; then
    report_failure "期限超過の記録が画面の報告を「無し」と述べていません（抑止が効いていない）: ${no_paint_line}"
  fi
  if grep -qE "$heartbeat_re" "$record"; then
    report_failure "抑止したのに成立行が記録されています（抑止が効いていない）"
  fi
  no_paint_label=$(printf '%s\n' "$no_paint_line" | sed -n 's/.*label=\([^ ]*\).*/\1/p')
  if [ -z "$no_paint_label" ]; then
    report_failure "期限超過の記録からラベルを読み取れませんでした: ${no_paint_line}"
  fi
  echo "OK: 通知が抑止され、8.2 が期限超過を記録しました: ${no_paint_line}"

  # 6. 判定がアプリを終わらせていないこと（この時点で観測する — 20 秒の終了より前）。
  if ! x11_process_alive "$x11_pid"; then
    report_failure "期限超過の判定の後にアプリのプロセスが終了しました（10.2 は提示して動作を続けることを要求する）"
  fi
  echo "OK: 判定の後もアプリのプロセス pid=${x11_pid} は生きています"

  # 3. ネイティブの題名が変わったこと（**同じ識別子のウィンドウ**であること）。
  #
  # **識別子で題名を読む** — `x11_collect_windows` の題名一致は先頭一致なので、題名の途中の
  # 語（`描画が成立しませんでした`）では引けない（初版がそれで失敗した）。識別子で読めば、
  # 題名が変わったのが**同じウィンドウ**であることも同時に言える。
  probe_id=$(printf '%s\n' "$first_ids" | head -n 1)
  if [ -z "$probe_id" ]; then
    report_failure "題名を読むウィンドウの識別子が得られませんでした"
  fi
  notice_title=""
  title_deadline=$(( $(date +%s) + suppress_grace_secs ))
  while [ "$(date +%s)" -lt "$title_deadline" ]; do
    notice_title=$(x11_window_title "$probe_id")
    case "$notice_title" in
      *"$notice_title_word"*) break ;;
    esac
    notice_title=""
    sleep "$x11_poll_sleep"
  done
  if [ -z "$notice_title" ]; then
    report_failure "ウィンドウ ${probe_id} の題名が '$notice_title_word' を含むようになりませんでした（ネイティブの題名で提示されていない）"
  fi
  case "$notice_title" in
    *"$no_paint_label"*) ;;
    *) report_failure "提示の題名が対象のウィンドウ（${no_paint_label}）を名指ししていません: ${notice_title}" ;;
  esac
  case "$notice_title" in
    *'診断情報: '*) ;;
    *) report_failure "提示の題名に診断情報の保存先がありません: ${notice_title}" ;;
  esac
  # 提示は**既存のウィンドウ**に対して行われる（新しいウィンドウを作らない）。
  x11_observe_window "$title" "$min_w" "$min_h"
  if [ "$x11_window_count" -ne 1 ] || [ "$x11_window_ids" != "$first_ids" ]; then
    report_failure "提示の後にウィンドウ集合が変わりました（期待: ${first_ids} の 1 枚。観測: ${x11_window_count} 枚 $(printf '%s' "$x11_window_ids" | tr '\n' ' ')）"
  fi
  echo "OK: ネイティブの題名が変わりました（同じウィンドウ ${probe_id}）: ${notice_title}"

  # 4. 全画面の注意書きが**描かれた**こと（画素を読む）。
  pixel_line=$(x11_notice_pixels "$probe_id" 64) ||
    report_failure "ウィンドウ ${probe_id} の画素を読めませんでした"
  notice_px=$(printf '%s\n' "$pixel_line" | awk '{ print $1 }')
  white_px=$(printf '%s\n' "$pixel_line" | awk '{ print $2 }')
  other_px=$(printf '%s\n' "$pixel_line" | awk '{ print $3 }')
  total_px=$(printf '%s\n' "$pixel_line" | awk '{ print $4 }')
  if [ -z "$total_px" ] || [ "$total_px" -eq 0 ]; then
    report_failure "画素の合計が 0 です（読み取りが成立していない）: ${pixel_line}"
  fi
  notice_percent=$((notice_px * 100 / total_px))
  echo "画素（格子 64x64、注意書きの赤・白・その他・合計）: ${notice_px} ${white_px} ${other_px} ${total_px} → 赤 ${notice_percent}%"
  if [ "$notice_percent" -lt 80 ]; then
    report_failure "注意書きの赤（#b00020）が画面の ${notice_percent}% しかありません（80% 以上を要求する。全画面の注意書きが描かれていない）"
  fi
  if [ "$white_px" -lt 1 ]; then
    report_failure "注意書きの本文（白）の画素が 1 つもありません（赤い面だけで本文が描かれていない）"
  fi
  echo "OK: 全画面の注意書きが描かれています（赤 ${notice_percent}%、本文の白 ${white_px} 画素）"

  # 5. 8.3 の印（設定の実体に現れる）。
  mark_deadline=$(( $(date +%s) + 10 ))
  mark_found=0
  while [ "$(date +%s)" -lt "$mark_deadline" ]; do
    if [ -f "$settings_file" ] &&
      tr -d ' \n' < "$settings_file" | grep -qF '"render.fallback":true'; then
      mark_found=1
      break
    fi
    sleep "$x11_poll_sleep"
  done
  if [ "$mark_found" -ne 1 ]; then
    report_failure "8.3 の印（${settings_file} の \"render.fallback\": true）が残っていません"
  fi
  if [ "$mark_before" -eq 0 ]; then
    # 設定ストアは**同一値の書き込みを通知しない**（4.3 の契約）。この行は「値が実際に
    # false から true へ変わった」ことの証拠であり、値の要求だけでは満たせない。
    if ! grep -qF '設定変更を全ウィンドウへ通知する（キー: render.fallback）' "$record"; then
      report_failure "8.3 の印がこの実行で変わったことが記録にありません（'設定変更を全ウィンドウへ通知する（キー: render.fallback）' が現れない）"
    fi
    echo "OK: 8.3 の印がこの実行で false から true へ変わりました（${settings_file}）"
  else
    echo "注意: 8.3 の印は起動の前から立っていたため、遷移の記録は要求しません（値そのものは要求済み）"
  fi

  # 6. アプリが自分で通常終了すること（終了コード 0）。
  if ! x11_wait_for_exit "$x11_pid" "$(( suppress_exit_ms / 1000 + 30 ))"; then
    report_failure "判定の後にアプリが終了しませんでした（pid=${x11_pid}）"
  fi
  if [ "$x11_exit_status" -ne 0 ]; then
    report_failure "判定の後にアプリが終了コード ${x11_exit_status} で終わりました（期限超過の経路は通常終了させる）"
  fi
  echo "OK: 判定の後もアプリは動作を続け、終了コード 0 で終わりました"

  echo "OK: 期限超過の判定が記録され、ネイティブの題名が変わり、全画面の注意書きが描かれ、8.3 の印が残った"
  exit 0
fi

# -------------------------------------------------------------------
# モード `ignored`: 配布物は抑止の引き金を読まない（負の対照）
# -------------------------------------------------------------------
# **このモードは配布物（AppImage）を起動する。**起動ラッパーが本体を切り離すため、起動した
# pid（`$!`）は本体を指さない（`scripts/lib/x11-window.sh` の `x11_window_pid` の doc）。
# **「アプリが生きている」ことと「題名・画素を読む対象」は、観測したウィンドウを所有する
# プロセス（`_NET_WM_PID`）で見る。** 取れなければ **exit 2** で落とす（生存を観測できない
# まま緑にしない）。
if ! command -v xprop >/dev/null 2>&1; then
  echo "NG: xprop が見つかりません（ウィンドウを所有するプロセスの識別に使います）" >&2
  exit 2
fi

JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT=1 GDK_BACKEND=x11 \
  nohup "$app" >"$x11_log" 2>&1 &
x11_pid=$!
x11_pick_poll_sleep

if ! wait_for_window "$timeout_secs"; then
  if [ "$x11_died" = 1 ]; then
    report_failure "起動したプロセスがウィンドウを出す前に終了しました（pid=${x11_pid}）"
  fi
  report_failure "タイトルに '$title' を含む ${min_w} x ${min_h} 以上のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
fi
# **集合が安定するまで待つ**（消えかけのウィンドウを自分のウィンドウと取り違えない。
# `x11_settle_windows` の doc）。
if ! x11_settle_windows "$title" "$min_w" "$min_h" 10; then
  report_failure "タイトルに '$title' を含むウィンドウの集合が安定しませんでした（消えかけのウィンドウが残っています）"
fi
if [ "$x11_window_count" -ne 1 ]; then
  report_failure "タイトルに '$title' を含むウィンドウが ${x11_window_count} 枚あります（この検査は 1 枚を対象にする）"
fi
probe_id=$(printf '%s\n' "$x11_window_ids" | head -n 1)
if [ -z "$probe_id" ]; then
  report_failure "題名と画素を読むウィンドウの識別子が得られませんでした"
fi
app_pid=$(x11_window_pid "$probe_id")
if [ -z "$app_pid" ]; then
  echo "NG: ウィンドウ ${probe_id} の _NET_WM_PID を読めません（アプリのプロセスの生存を観測できません）" >&2
  exit 2
fi
echo "配布物の本体: pid=${app_pid}（ウィンドウ ${probe_id} の _NET_WM_PID。起動した pid=${x11_pid} ではない）"
heartbeat_line=""
heartbeat_deadline=$(( $(date +%s) + suppress_grace_secs ))
while [ "$(date +%s)" -lt "$heartbeat_deadline" ]; do
  heartbeat_line=$(record_find_from 0 "$heartbeat_re")
  if [ -n "$heartbeat_line" ]; then
    break
  fi
  sleep "$x11_poll_sleep"
done
if [ -z "$heartbeat_line" ]; then
  report_failure "配布物の初回描画の成立行（'$heartbeat_re'）が現れませんでした（起動していない検査を緑にしない）"
fi
echo "OK: 配布物は抑止されずに初回描画が成立しました: ${heartbeat_line}"

# 期限（3 秒）を確実に超えて観測する。**配布物は終了の引き金も読まないので、片付けはトラップ。**
observation_secs=$(( first_paint_deadline_ms / 1000 + 6 ))
echo "観測: ${observation_secs} 秒のあいだ、期限超過の経路に入らないことを確かめます"
observation_end=$(( $(date +%s) + observation_secs ))
while [ "$(date +%s)" -lt "$observation_end" ]; do
  if ! x11_process_alive "$app_pid"; then
    report_failure "配布物の本体（pid=${app_pid}）が観測の途中で終了しました"
  fi
  if grep -qE "$no_paint_re" "$record"; then
    report_failure "配布物が期限超過（'$no_paint_re'）を記録しました（既定のビルドが抑止の引き金を読んでいる）"
  fi
  # 題名は**識別子で**読む（先頭一致では題名の途中の語を引けない。上の注記を参照）。
  current_title=$(x11_window_title "$probe_id")
  case "$current_title" in
    *"$notice_title_word"*)
      report_failure "配布物の題名が '$notice_title_word' を含むようになりました（既定のビルドに検証用の経路が入っている）: ${current_title}"
      ;;
  esac
  # **ウィンドウの識別子を取り直す**（ウィンドウマネージャが toplevel を作り直す環境があり、
  # 古い識別子は無効になる。実測: 作り直しの後に古い識別子で画素を読むと BadWindow になる）。
  x11_observe_window "$title" "$min_w" "$min_h"
  if [ "$x11_window_count" -ne 1 ]; then
    report_failure "配布物のウィンドウが観測の途中で ${x11_window_count} 枚になりました（終了したか、増えた）"
  fi
  probe_id=$(printf '%s\n' "$x11_window_ids" | head -n 1)
  sleep "$x11_poll_sleep"
done
echo "OK: 配布物の題名は変わっていません（$probe_id）"

pixel_line=$(x11_notice_pixels "$probe_id" 64) ||
  report_failure "ウィンドウ ${probe_id} の画素を読めませんでした"
notice_px=$(printf '%s\n' "$pixel_line" | awk '{ print $1 }')
total_px=$(printf '%s\n' "$pixel_line" | awk '{ print $4 }')
if [ -z "$notice_px" ] || [ -z "$total_px" ] || [ "$total_px" -eq 0 ]; then
  report_failure "画素の読み取りが成立していません: ${pixel_line}"
fi
notice_percent=$((notice_px * 100 / total_px))
echo "画素（格子 64x64、注意書きの赤の割合）: ${notice_percent}%（配布物は注意書きを提示しない）"
if [ "$notice_percent" -ge 80 ]; then
  report_failure "配布物の画面が注意書きの赤で覆われています（既定のビルドが期限超過の提示を行っている）"
fi

echo "OK: 配布物は引き金（JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT=1）を無視しました（成立行が出て、期限超過の経路に入らず、注意書きも提示されない）"
echo "OK: モード suppressed の提示が「抑止した結果」であることの負の対照が成立した"
exit 0

#!/bin/sh
# 補助プロセスの終了保証を実機で検証する（tasks.md 10.7 / 要件 5.6）。**Linux / macOS 用**。
#
# 使い方:
#   sh scripts/check-sidecar-termination.sh <検証用の形の実行ファイル> [<起動タイムアウト秒>]
#
# <検証用の形> は `--features verification-triggers` のビルド（例 `target/release/jxcel`）。
# 配布物（既定のビルド）は検証専用の環境変数を読まないので、この検査は成立しない。
# <起動タイムアウト秒> の既定は 60。
#
# 終了コード: 0 = 全条件で残存 0 / 1 = 検証失敗（残存あり・起動しない・期限内に終わらない） /
# 2 = 入力が使えない（引数・Linux での DISPLAY 不在・検証用の形でない・解決先の補助プロセスが無い）。
#
# # 前提（満たさなければ exit 2。**無言で通る経路を作らない**）
#
#   - **Linux だけ** `DISPLAY` があること（GUI アプリを起動するのに X が要る。CI は
#     `xvfb-run` の下で呼ぶ）。**macOS は AppKit であり X を使わないのでこの前提は課さない**
#     （`DISPLAY` は通常未設定である）。
#   - 検証用の形であること（`strings` で `JXCEL_VERIFICATION_EXIT_AFTER_MS` の存在を確かめる）。
#   - **8.1 の解決規則が指す補助プロセスが実行できること**:
#       Linux : `<アプリのディレクトリ>/../share/jxcel/sidecar-smoke`（deb / システム
#               インストールと同じ形。AppImage は `$APPDIR` を使うので別のパスになる。
#               この検査は検証用の形を起動するので、CI の段が staging した原本から複製する）
#       macOS : `<アプリのディレクトリ>/sidecar-smoke`（`externalBin` が実行ファイルの隣へ複製する）
#
# # 検証する条件（**すべてで残存 0 を要求する**）
#
#   (a) 通常終了 — `sidecar:<ms>` で起動し、通常終了（`RunEvent::Exit`）の後に残存 0。
#       機構は 5.6 の `shutdown_all` → 3.3 のプロセスグループ（`killpg`）/ Job Object。
#   (b) 強制終了 — `sidecar:<ms>` で起動し `SIGKILL`。**こちらが本番の経路である**（異常終了
#       では終了処理が走らない）。Unix では `RunEvent::Exit` が届かず `killpg` も走らないため、
#       実効的な機構は**補助プロセス自身の親監視**（1.6 / 3.5 の `--parent-pid`）だけである。
#       検出間隔は 100 ms（`crates/sidecar-smoke` の `POLL_INTERVAL`）。
#   (c) 孫あり・通常終了 — `sidecar-grandchild:<ms>` で孫を持つ補助プロセスを起動する。3.3 の
#       プロセスグループが**孫まで**届くこと（`setpgid` + `killpg`）を確かめる。
#   (d) 補助: 孫あり・強制終了 — 親監視の届かない孫が残ることを観測し、**次回起動の掃除**
#       （3.5 の `sweep_orphans`）だけがそれを除去することを確かめる。強制終了の後に孫を
#       終わらせる経路は掃除のほかに無いので、この段は掃除を load-bearing に確かめる。
#
# # 「本当に起動したか」の事前条件
#
# 残存 0 の判定の前に、**補助プロセスが実際に動いていること**を要求する。加えて、その
# コマンドラインが**この検査が起動したアプリの識別子**を `--parent-pid` に持つことまで要求する
# （監督が注入する。3.2 / 3.5）。これが無いと「一度も起動していない」実行が残存 0 で通る。
#
# # プロセスの照合（プラットフォームで分ける。**限界を正直に記す**）
#
#   - Linux : `/proc/<pid>/exe` を読み、**8.1 の解決先の絶対パスと一致する**ものだけを数える
#     （3.5 と同じく名前だけでなく実行ファイルで照合する）。
#   - macOS : `ps -axo pid=,comm=` を使う。macOS の `comm` はコマンド名（カーネルの `p_comm` は
#     16 文字で切詰め）であり、**起動時の絶対パスは得られない**（tasks.md 3.5 の申し送り。
#     8.1 が macOS に期待パスを与えない理由そのものである）。語幹 `sidecar-smoke` は 13 文字で
#     16 文字に収まるため、**名前の一致だけは成立する**。したがって macOS では名前で数え、
#     絶対パス照合が効かないことを診断にも記す。
#
# # 後始末
#
# EXIT トラップで、起動したアプリと**解決先に属する残存補助プロセス**を終了させる（後続の段へ
# 持ち越さない。10.5 の申し送りと同じ理由）。片付けはすべての判定より後に走るので、検証の
# 失敗（残存ありで exit 1）を覆い隠さない。
set -eu

usage() {
  echo "使い方: sh scripts/check-sidecar-termination.sh <検証用の形の実行ファイル> [<起動タイムアウト秒>]" >&2
}

# --- 引数と入力の前提 -------------------------------------------------------
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  usage
  exit 2
fi
app=$1
timeout_seconds=${2:-60}

case $timeout_seconds in
  '' | *[!0-9]*)
    echo "NG: 起動タイムアウト秒が整数ではありません: $timeout_seconds" >&2
    exit 2
    ;;
esac

if [ ! -x "$app" ]; then
  echo "NG: 実行できるアプリがありません: $app" >&2
  exit 2
fi

case $(uname -s) in
  Linux) os_mode=linux ;;
  Darwin) os_mode=macos ;;
  *)
    echo "NG: この検査器は Linux / macOS 用です: $(uname -s)" >&2
    exit 2
    ;;
esac

# **`DISPLAY` は Linux だけの前提である。** Linux では GUI アプリを起動するのに X が要る
# （CI は `xvfb-run` の下で呼ぶ）が、**macOS は AppKit であり X を使わない**（通常 `DISPLAY` は
# 未設定）。以前はこの検査を**プラットフォームの判定より前**に行っていたため、macOS の段が
# 必ず exit 2 で落ちていた（実測: 2026-09-12 の macOS のランナー。手前の段が落ちていた間は
# 露見していなかった）。
if [ "$os_mode" = linux ] && [ -z "${DISPLAY:-}" ]; then
  echo "NG: DISPLAY がありません（GUI アプリ。CI は xvfb-run の下で呼ぶ）" >&2
  exit 2
fi

# 配布物（既定のビルド）は環境変数の読み取りを持たない。**検証用の形でなければ落とす**
# （配布物を渡すと 1 条件も成立しないのに、補助プロセスが現れないことを「残存 0」と
# 取り違ねないための最初の錠前）。
if ! strings -a "$app" | grep -q 'JXCEL_VERIFICATION_EXIT_AFTER_MS'; then
  echo "NG: 検証用の形ではありません（--features verification-triggers のビルドではない）: $app" >&2
  exit 2
fi

app_dir=$(CDPATH='' cd -- "$(dirname -- "$app")" && pwd)
case $os_mode in
  linux) sidecar_path="$app_dir/../share/jxcel/sidecar-smoke" ;;
  macos) sidecar_path="$app_dir/sidecar-smoke" ;;
esac
sidecar_dir=$(dirname -- "$sidecar_path")
sidecar_name=$(basename -- "$sidecar_path")
sidecar_canonical=$(
  CDPATH='' cd -- "$sidecar_dir" 2>/dev/null && printf '%s/%s\n' "$(pwd)" "$sidecar_name"
) || sidecar_canonical=''
if [ -z "$sidecar_canonical" ] || [ ! -x "$sidecar_canonical" ]; then
  echo "NG: 8.1 の解決先に補助プロセスがありません: $sidecar_path" >&2
  echo "    Linux は staging した原本を target/share/jxcel/sidecar-smoke へ複製してください。" >&2
  exit 2
fi

# --- 作業領域 ---------------------------------------------------------------
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/jxcel-sidecar-termination.XXXXXX")
data_dir="$tmp_dir/data"
mkdir -p "$data_dir"

app_pid=''
app_log=''
app_exit_rc=''
sidecar_count=0
sidecar_pids=''
sidecar_idle_count=0
sidecar_matches_parent=0
expected_parent_pid=''

# --- 補助プロセスの列挙 -----------------------------------------------------
#
# `scan_sidecars` は一致したプロセスの数・識別子・`--idle`（孫）の数・`--parent-pid` の一致を
# 求める。引数に `print` を与えると一覧も出す（CI のログに数を残すため）。
scan_sidecars() {
  sidecar_count=0
  sidecar_pids=''
  sidecar_idle_count=0
  sidecar_matches_parent=0
  case $os_mode in
    linux)
      for link in /proc/[0-9]*/exe; do
        [ -e "$link" ] || continue
        pid=${link#/proc/}
        pid=${pid%/exe}
        target=$(readlink "$link" 2>/dev/null) || continue
        [ -n "$target" ] || continue
        # 期待パスは `..` を含む形（`<exe>/../share/jxcel`）なので、実行ファイル側と同じ
        # 正規化を当ててから比べる。
        resolved=$(readlink -f "$target" 2>/dev/null) || resolved=$target
        [ "$resolved" = "$sidecar_canonical" ] || continue
        detail=$(tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null) || detail=''
        record_sidecar "$pid" "exe=$target" "$detail"
      done
      ;;
    macos)
      ps -axo pid=,comm= >"$tmp_dir/ps.txt" 2>/dev/null || : >"$tmp_dir/ps.txt"
      while read -r pid name; do
        [ -n "${pid:-}" ] || continue
        # `comm` はコマンド名（16 文字で切詰め）。絶対パスは得られないので名前で照合する。
        case $name in
          sidecar-smoke | */sidecar-smoke) ;;
          *) continue ;;
        esac
        detail=$(ps -p "$pid" -o args= 2>/dev/null) || detail=''
        record_sidecar "$pid" "comm=$name" "$detail"
      done <"$tmp_dir/ps.txt"
      ;;
  esac
}

# 1 件を数え上げる（一覧の表示と、孫・親監視の照合を 1 箇所に集める）。
record_sidecar() {
  pid=$1
  where=$2
  detail=$3
  sidecar_count=$((sidecar_count + 1))
  sidecar_pids="$sidecar_pids $pid"
  case " $detail " in
    *" --idle "*) sidecar_idle_count=$((sidecar_idle_count + 1)) ;;
  esac
  if [ -n "$expected_parent_pid" ]; then
    case " $detail " in
      *" --parent-pid $expected_parent_pid "*) sidecar_matches_parent=1 ;;
    esac
  fi
  if [ "${scan_print:-}" = 1 ]; then
    printf '    pid=%s %s args=%s\n' "$pid" "$where" "$detail"
  fi
}

# 一覧と件数を出す（CI のログに数と `args` を残す。**数えることと出すことを同じ走査で行う**）。
show_sidecars() {
  scan_print=1
  scan_sidecars
  scan_print=0
  printf '  補助プロセスの数: %s（うち孫: %s）\n' "$sidecar_count" "$sidecar_idle_count"
}

# --- 待つ -------------------------------------------------------------------

# 補助プロセス（と孫）が現れ、かつ `--parent-pid` が**このアプリ**を指すまで待つ。
# 戻り値 0 = 現れた。1 = 期限内に現れなかった（呼び出し側が失敗にする）。
wait_for_start() {
  want=$1
  deadline=$(( $(date +%s) + timeout_seconds ))
  while [ "$(date +%s)" -le "$deadline" ]; do
    scan_sidecars
    if [ "$sidecar_count" -ge "$want" ] && [ "$sidecar_matches_parent" -eq 1 ]; then
      return 0
    fi
    sleep 0.2
  done
  return 1
}

# 残存が 0 になるまで待つ（最大 <秒>）。戻り値 0 = 0 になった。
wait_for_zero() {
  limit=$1
  deadline=$(( $(date +%s) + limit ))
  while [ "$(date +%s)" -le "$deadline" ]; do
    scan_sidecars
    [ "$sidecar_count" -eq 0 ] && return 0
    sleep 0.1
  done
  scan_sidecars
  return 1
}

# --- 起動と終了 -------------------------------------------------------------

# 検証用の形を起動する（`$1` = 引き金の値、`$2` = ログの名前）。
#
# 診断と設定は**この検査専用の XDG_DATA_HOME** に置く（利用者の設定を読み書きしない。
# macOS の解決は XDG を使わないので、そこでは $HOME の下に書かれる）。
launch_app() {
  trigger=$1
  label=$2
  app_log="$tmp_dir/app-$label.log"
  JXCEL_VERIFICATION_EXIT_AFTER_MS="$trigger" XDG_DATA_HOME="$data_dir" \
    "$app" >"$app_log" 2>&1 &
  app_pid=$!
  expected_parent_pid=$app_pid
}

# アプリの終了を待つ。`app_exit_rc` に終了コードを入れる。戻り値 1 = 期限内に終わらない。
wait_app_exit() {
  limit=$1
  deadline=$(( $(date +%s) + limit ))
  while [ "$(date +%s)" -le "$deadline" ]; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
      app_exit_rc=0
      wait "$app_pid" 2>/dev/null || app_exit_rc=$?
      app_pid=''
      expected_parent_pid=''
      return 0
    fi
    sleep 0.1
  done
  app_exit_rc=''
  return 1
}

# 強制終了（SIGKILL）。終了処理は走らない（本番の異常終了と同じ）。
force_kill_app() {
  if [ -z "$app_pid" ]; then
    return 0
  fi
  kill -9 "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true
  app_pid=''
  expected_parent_pid=''
}

# 期限内にアプリが 0 で終わることを要求する（通常終了の条件で使う）。
require_clean_exit() {
  phase=$1
  if ! wait_app_exit $((timeout_seconds + 10)); then
    echo "NG: ${phase} アプリが期限内に終了しませんでした" >&2
    show_sidecars >&2 || true
    tail -n 30 "$app_log" >&2 || true
    exit 1
  fi
  if [ "$app_exit_rc" -ne 0 ]; then
    echo "NG: ${phase} アプリの終了コードが ${app_exit_rc}（通常終了は 0 であるべき）" >&2
    tail -n 30 "$app_log" >&2 || true
    exit 1
  fi
  echo "${phase} アプリの終了コード: ${app_exit_rc}（通常終了）"
}

# 補助プロセスが現れることを要求する（**「起動していない」を残存 0 と取り違えない**）。
require_started() {
  phase=$1
  want=$2
  if wait_for_start "$want"; then
    return 0
  fi
  scan_sidecars
  if [ "$sidecar_count" -lt "$want" ]; then
    echo "NG: ${phase} 補助プロセスが ${timeout_seconds} 秒以内に現れませんでした（観測: 補助プロセス ${sidecar_count} 件。期待 ${want} 件以上）" >&2
  else
    echo "NG: ${phase} 補助プロセスは現れましたが、その --parent-pid がこのアプリ（${app_pid}）を指していません（別の実体を数えている）" >&2
  fi
  show_sidecars >&2 || true
  tail -n 30 "$app_log" >&2 || true
  exit 1
}

# 残存 0 を要求する。
require_zero() {
  phase=$1
  limit=$2
  if wait_for_zero "$limit"; then
    return 0
  fi
  echo "NG: ${phase} 補助プロセスが残っています（残存 $sidecar_count 件）" >&2
  show_sidecars >&2 || true
  exit 1
}

# --- 後始末 -----------------------------------------------------------------

# 解決先に属する残存補助プロセスを終了させる（掃除と違って PID を直接指定する。前回の実行の
# プロセスはこの実行のプロセスグループに入っていない）。
# shellcheck disable=SC2329 # cleanup（EXIT トラップ）から呼ばれる（shellcheck は trap を追えない）
kill_residual_sidecars() {
  scan_sidecars
  if [ "$sidecar_count" -eq 0 ]; then
    return 0
  fi
  for pid in $sidecar_pids; do
    kill -9 "$pid" 2>/dev/null || true
  done
  sleep 0.3
  scan_sidecars
  return 0
}

# shellcheck disable=SC2329 # 下の trap から呼ばれる
cleanup() {
  if [ -n "${app_pid:-}" ]; then
    kill -9 "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
    app_pid=''
  fi
  kill_residual_sidecars || true
  if [ -n "${tmp_dir:-}" ] && [ -d "$tmp_dir" ]; then
    rm -rf "$tmp_dir"
  fi
  :
}
trap cleanup 0 1 2 15

# --- (a) 通常終了 -----------------------------------------------------------
echo "== (a) 通常終了（sidecar:8000） =="
launch_app 'sidecar:8000' a
require_started '(a)' 1
echo "(a) 補助プロセスが動作していることを観測した（--parent-pid がこのアプリと一致）:"
show_sidecars
require_clean_exit '(a)'
require_zero '(a) 通常終了の後' 15
echo "(a) 通常終了の後の一覧（残存 0）:"
show_sidecars
echo "OK: (a) 通常終了の後に補助プロセスは残らない（5.6 の shutdown_all → 3.3 のグループ終了）"

# --- (b) 強制終了（本番の経路） ---------------------------------------------
echo
echo "== (b) 強制終了（SIGKILL。本番の経路） =="
launch_app 'sidecar:600000' b
require_started '(b)' 1
echo "(b) 強制終了の前の一覧:"
show_sidecars
killed_pid=$app_pid
echo "(b) アプリ（pid=${killed_pid}）へ SIGKILL を送る（終了処理は走らない）"
force_kill_app
ticks=0
while [ "$ticks" -lt 200 ]; do
  scan_sidecars
  [ "$sidecar_count" -eq 0 ] && break
  sleep 0.05
  ticks=$((ticks + 1))
done
if [ "$sidecar_count" -ne 0 ]; then
  echo "NG: (b) 強制終了の後に補助プロセスが残っています（残存 $sidecar_count 件）" >&2
  show_sidecars >&2 || true
  exit 1
fi
echo "(b) SIGKILL の後、残存 0 を観測するまでの走査: $((ticks + 1)) 回（走査の間は 50 ms）"
echo "(b) 強制終了の後の一覧（残存 0）:"
show_sidecars
echo "OK: (b) 強制終了の後に補助プロセスは残らない（Unix の実効的な機構は 1.6 / 3.5 の親監視。"
echo "        検出間隔は 100 ms。killpg は shutdown_all の内側でしか走らないため、この経路には届かない）"

# --- (c) 孫あり・通常終了 ---------------------------------------------------
echo
echo "== (c) 孫プロセスあり・通常終了（sidecar-grandchild:8000） =="
launch_app 'sidecar-grandchild:8000' c
require_started '(c)' 2
echo "(c) 補助プロセスと孫が動作していることを観測した（孫の args は --idle）:"
show_sidecars
require_clean_exit '(c)'
require_zero '(c) 通常終了の後' 15
echo "(c) 通常終了の後の一覧（残存 0）:"
show_sidecars
echo "OK: (c) 孫を持つ補助プロセスも通常終了で残らない（3.3 のプロセスグループが孫まで届く）"

# --- (d) 補助: 孫あり・強制終了 → 次回起動の掃除 ----------------------------
echo
echo "== (d) 補助: 孫あり・強制終了 → 次回起動の掃除（3.5） =="
launch_app 'sidecar-grandchild:600000' d
require_started '(d)' 2
echo "(d) 強制終了の前の一覧:"
show_sidecars
force_kill_app
ticks=0
while [ "$ticks" -lt 200 ]; do
  scan_sidecars
  [ "$sidecar_count" -le 1 ] && break
  sleep 0.05
  ticks=$((ticks + 1))
done
if [ "$sidecar_count" -ne 1 ] || [ "$sidecar_idle_count" -ne 1 ]; then
  echo "NG: (d) 強制終了の後に、親監視の届かない孫（--idle）だけが 1 つ残ることを観測できませんでした（観測: 補助プロセス $sidecar_count 件 / うち孫 $sidecar_idle_count 件）" >&2
  show_sidecars >&2 || true
  exit 1
fi
echo "(d) 強制終了の後の一覧（親監視の届く子は消え、届かない孫だけが残った）:"
show_sidecars
echo "(d) 次回起動で起動時の残留掃除（3.5）を走らせる（exit:2500。この起動以外に孫を終わらせる経路は無い）"
launch_app 'exit:2500' d2
require_clean_exit '(d)'
require_zero '(d) 次回起動の掃除の後' 15
echo "(d) 掃除の後の一覧（残存 0）:"
show_sidecars
if [ "$os_mode" = linux ]; then
  # Linux では診断記録の位置が XDG_DATA_HOME から決まる（4.4 / 5.2）ので、掃除が実際に
  # 終了させた件数の行も確かめられる。**件数の主張とプロセスの観測を両方出す**。
  sweep_log="$data_dir/com.jxcel.app/logs/jxcel.log"
  if [ ! -f "$sweep_log" ]; then
    echo "NG: (d) 診断記録が見つかりません（掃除の記録を確かめられない）: $sweep_log" >&2
    exit 1
  fi
  sweep_line=$(grep '残留プロセスの掃除で' "$sweep_log" | tail -n 1 || true)
  if ! printf '%s\n' "$sweep_line" | grep -Eq '残留プロセスの掃除で [1-9][0-9]* 件'; then
    echo "NG: (d) 掃除が孫を終了させた記録がありません: ${sweep_line:-（該当行なし）}" >&2
    exit 1
  fi
  echo "(d) 記録（5.1 の起動時の掃除）: $sweep_line"
else
  echo "(d) 限界: macOS の診断記録は \$HOME/Library/Logs の下にあり、この検査は記録を読まずに"
  echo "        プロセスの観測だけで掃除を確かめている（残存 0 は名前照合の掃除以外では達成できない）"
fi
echo "OK: (d) 親監視の届かない孫は 3.5 の起動時掃除だけが除去する（名前の一致だけで働く）"

echo
echo "OK: 通常終了・強制終了・孫ありの 3 条件すべてで、残存する補助プロセスは 0 件である"
exit 0

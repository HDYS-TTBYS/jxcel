#!/bin/sh
# ウィンドウの生成に失敗したときの**提示**と**隔離**を X11 上で検査する
# （要件 2.10。tasks.md 6.1 が置いた検証専用の引き金 `fail-window` を駆動する）。
#
# 使い方:
#   check-window-failure.sh <検証用の実行ファイル> <タイトル部分文字列> <診断記録> \
#                           <ドキュメント位置> [タイムアウト秒] [最小幅] [最小高さ]
#
#   - 検証用の実行ファイル : `--features verification-triggers` の**検証用の形**。
#                             `JXCEL_VERIFICATION_EXIT_AFTER_MS=fail-window:<ms>` を読む唯一の形
#   - タイトル部分文字列   : ウィンドウタイトルに含まれるべき文字列（`jxcel`）
#   - 診断記録             : アプリの診断記録（4.4 の保存先の `jxcel.log`）。**起動の前に消す**
#                             （前回の実行の行で成功しないため）
#   - ドキュメント位置     : 失敗の後に「他のウィンドウの動作が続く」ことを測るため、引き継ぎ
#                             （二重起動。1.5）へ渡す引数。**実在**していること（7.7 は読まない）
#   - タイムアウト秒       : 既定 60。ウィンドウの出現と引き継ぎを待つ上限
#   - 最小幅 / 最小高さ    : 既定 100。GTK の 20x20 程度の補助ウィンドウを数えないため
#
# # 何を証明するか（要件 2.10）
#
# 1. **失敗が起きたこと** — 引き金 `fail-window:<ms>` が、生きているウィンドウのラベルと
#    **衝突するラベル**で生成を試み、6.1 の**通常の失敗側の分岐**（登録の取り消しと報告）を
#    そのまま通す（検証専用の失敗経路を別に持たない）。
# 2. **失敗が提示されたこと** — 提示は 2 段である:
#    (a) 記録機構の行 `ウィンドウを生成できなかった: label=…`（診断の記録と標準出力へ出る）
#    (b) 診断の保存先の 1 件の記録 `jxcel-window-error.log`
#        （`lifecycle::persist_window_failure`。失敗したラベル・対象のドキュメント・理由を名指しし、
#        他のウィンドウに触れていないことを述べる）
#    **2 段目が提示の本体である** — 記録機構の行は設定された詳細度で落ちうるし、利用者が
#    記録の中の該当行を探し当てることを前提にしてしまう（要件 1.4 の起動失敗の提示と同じ形）。
#    したがって (b) の**内容**（ラベルと理由）を要求する。(a) だけでは足りない。
# 3. **既に開いている他のウィンドウの動作が中断していないこと** —
#    (a) 失敗を起こす前に在ったウィンドウが**同じ X の識別子のまま**残り、プロセスが生き続ける
#        （観測は数秒続ける。1 回の観測では「たまたま生きていた」を排除できない）
#    (b) そのあと**引き継ぎで開いた新しいウィンドウが描画まで成立する**
#        （`初回描画が成立した: label=…`）。**「プロセスが生きている」だけでは足りない** —
#        生成の失敗がウィンドウのレジストリ（6.1）や描画の監視（8.2）を壊していれば、次の生成が
#        失敗するか描画が成立しないので、その両方を要求する。
# 4. **失敗したウィンドウが現れていないこと** — 判定の終わりの X のウィンドウ数は 2（失敗を
#    起こす前の 1 枚 + 引き継ぎの 1 枚）であり、**失敗したラベル**が
#    `ウィンドウを開いた: label=…` として記録に現れない（レジストリに残骸が無いことの確認）。
#
# # 検証の限界（正直に記す）
#
#   - 提示は**利用者が見つけられる記録**までである。**可視のダイアログは無い**
#     （その理由は `lifecycle::persist_window_failure` の doc に書いてある — 3 OS で成立する
#     ネイティブ提示の複雑さと、macOS / Windows のランナーがアクセシビリティ許可を持たず
#     ダイアログのテキストを外部から読めないこと）。この検査器が証明するのは
#     「記録がその場所に、その内容で残る」ことまでである。
#   - 引き金は**非既定の feature** の下にある。したがって配布物ではこの失敗を起こせない。
#     配布物でも同じ提示の経路が走ること（`persist_window_failure` は feature で括られていない）
#     は、**検証用の形が同じコードを通ること**で示す — 配布物を渡すとこの検査器は exit 2 で
#     落ちる（下記）。
#
# # 入力の前提（負の対照）
#
# **配布物（既定のビルド）を渡してはならない。**引き金の名前 `fail-window` は
# `verification-triggers` の下にしか無い（5.4 の片付け）。実行ファイルにその名前が無ければ
# **exit 2**（入力が使えない）で落ちる。CI は配布物を渡して非 0 を確かめることで、
# **検査器が引き金を本当に見ている**こと（常に成功する検査器ではないこと）を反証できる。
#
# # 前提
#   - DISPLAY が設定されていること（CI の Linux ランナーでは `xvfb-run` が設定する）。
#   - `xwininfo`（x11-utils）、`strings`（binutils）、`ps` / `pgrep` が必要。
#
# # 終了コード
#   0 = すべて成立 / 1 = 検査失敗 / 2 = 入力が使えない（実行ファイル不在・DISPLAY 不在・
#   引き金を持たない実行ファイル・同題名のウィンドウが既にある・引数の欠落）
#
# 起動したプロセス（引き継ぎの 2 つ目を含む）は EXIT / INT / TERM のトラップで必ず片付ける。
# 片付けと観測の実装は `scripts/lib/x11-window.sh` が持つ。
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
  echo "使い方: check-window-failure.sh <検証用の実行ファイル> <タイトル部分文字列> <診断記録> <ドキュメント位置> [タイムアウト秒] [最小幅] [最小高さ]" >&2
  exit 2
}

[ "$#" -ge 4 ] || usage

app=$1
title=$2
record=$3
document=$4
timeout_secs=${5:-60}
min_w=${6:-100}
min_h=${7:-100}

# 引き金が終了させるまでの待ち（ミリ秒）。**検査の間は生き続けてほしい**ので、観測に要する
# 想定時間より十分大きくする。片付けはトラップが行うので、この値でアプリが残ることはない。
fail_window_ms=120000

# 提示の記録（6.1 の `lifecycle::WINDOW_FAILURE_FILE_NAME` と同じ名前でなければならない。
# 診断の保存先の直下に置かれる）。
window_error_record=$(dirname -- "$record")/jxcel-window-error.log

# 引き継ぎの 2 つ目（自分で終了するが、期限内に終わらなければ片付ける）。
second_pid=""

case "$timeout_secs" in
  ''|*[!0-9]*)
    echo "NG: タイムアウト秒が整数ではありません: $timeout_secs" >&2
    exit 2
    ;;
esac
if [ -z "$record" ]; then
  echo "NG: 診断記録が指定されていません" >&2
  exit 2
fi
if [ ! -f "$document" ]; then
  echo "NG: ドキュメント位置が実在しません: ${document}（7.7 は読まないが、渡す位置は実在させる）" >&2
  exit 2
fi

x11_require_app "$app"
x11_require_environment
if ! command -v strings >/dev/null 2>&1; then
  echo "NG: strings が見つかりません（実行ファイルが検証用の形かどうかの判定に使います）" >&2
  exit 2
fi
if ! strings -a "$app" | grep -q 'fail-window'; then
  echo "NG: 実行ファイルに引き金 'fail-window' がありません（--features verification-triggers の検証用の形を渡してください）" >&2
  exit 2
fi

# 起動したプロセス（引き継ぎの 2 つ目を含む）を必ず片付ける。**主役は `x11_pid`**
# （置き場のトラップが木ごと終了し、出力の控えを消す）。
# shellcheck disable=SC2329 # 下の trap から呼ばれる
cleanup() {
  if [ -n "$second_pid" ] && x11_process_alive "$second_pid"; then
    x11_kill_tree "$second_pid"
    kill -9 "$second_pid" 2>/dev/null || true
    wait "$second_pid" 2>/dev/null || true
  fi
  x11_cleanup
}

report_failure() {
  echo "NG: $1" >&2
  echo "--- 診断記録（末尾）: $record ---" >&2
  if [ -f "$record" ]; then
    tail -n 40 "$record" >&2
  else
    echo "(記録ファイルがありません)" >&2
  fi
  echo "--- 提示の記録: $window_error_record ---" >&2
  if [ -f "$window_error_record" ]; then
    cat "$window_error_record" >&2
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

# 記録の現在の行数（`wc -l` の前後の空白を落とす）。
record_count() {
  if [ ! -f "$record" ]; then
    echo 0
    return 0
  fi
  wc -l < "$record" | tr -d ' '
}

# 8.2 の成立行（`Painted` と `SoftwareRaster` のどちらも描画の成立である）。
heartbeat_re='初回描画が成立した: label=|初回描画は成立したがソフトウェアラスタライザ経由である: label=|期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label='

# 同題名・最小寸法以上のウィンドウが既に無いことを先に確かめる。残っていると、引き継ぎの
# 2 つ目がそのインスタンスへ吸われ、ラベルの前提（`empty-1` から始まる）が崩れる。
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
trap cleanup EXIT INT TERM

# **前回の実行の行と前回の提示の記録で成功しないよう、起動の前に消す。**
rm -f "$record" "$window_error_record"

# 検証用の引き金を親の環境から漏らさない（この検査器が設定する値だけを使う）。
JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT="" \
JXCEL_VERIFICATION_INITIAL_SCREEN="" \
JXCEL_VERIFICATION_EXIT_AFTER_MS="fail-window:${fail_window_ms}" \
GDK_BACKEND=x11 nohup "$app" >"$x11_log" 2>&1 &
x11_pid=$!

x11_pick_poll_sleep

# --- 1. 失敗を起こす前に在るウィンドウ（= 「他のウィンドウ」）を観測する ---------------
# **集合が安定するまで待つ** — 直前の検査のプロセスが片付いた直後は、消えかけのウィンドウが
# 1 回目の観測にだけ現れる（`x11_settle_windows` の doc。実測でこれを拾って失敗した）。
first_ids=""
first_size=""
deadline=$(( $(date +%s) + timeout_secs ))
while [ "$(date +%s)" -lt "$deadline" ]; do
  x11_observe_window "$title" "$min_w" "$min_h"
  if [ -n "$x11_window_match" ]; then
    if ! x11_settle_windows "$title" "$min_w" "$min_h" 10; then
      report_failure "タイトルに '$title' を含むウィンドウの集合が安定しませんでした（消えかけのウィンドウが残っています）"
    fi
    if [ "$x11_window_count" -ne 1 ]; then
      report_failure "タイトルに '$title' を含むウィンドウが ${x11_window_count} 枚あります（この検査は 1 枚から始める。別のアプリが残っています）"
    fi
    first_ids=$x11_window_ids
    first_size=$x11_window_match
    break
  fi
  x11_note_if_process_died
  sleep "$x11_poll_sleep"
done
if [ -z "$first_ids" ]; then
  if [ "$x11_died" = 1 ]; then
    report_failure "起動したプロセスがウィンドウを出す前に終了しました（pid=${x11_pid}）"
  fi
  report_failure "タイトルに '$title' を含む ${min_w} x ${min_h} 以上のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
fi
echo "OK: 他のウィンドウ '$title' ${first_size} が現れました（識別子: $(printf '%s' "$first_ids" | tr '\n' ' ')）"

# --- 2. そのウィンドウの初回描画が成立している（= 正常に動いていた）ことを確かめる -------
heartbeat_line=""
heartbeat_deadline=$(( $(date +%s) + 20 ))
while [ "$(date +%s)" -lt "$heartbeat_deadline" ]; do
  heartbeat_line=$(record_find_from 0 "$heartbeat_re")
  if [ -n "$heartbeat_line" ]; then
    break
  fi
  sleep "$x11_poll_sleep"
done
if [ -z "$heartbeat_line" ]; then
  report_failure "他のウィンドウの初回描画の成立行（'$heartbeat_re'）が現れませんでした（失敗を起こす前から描画が成立していない）"
fi
first_label=$(printf '%s\n' "$heartbeat_line" | sed -n 's/.*label=\([^ ]*\).*/\1/p')
if [ -z "$first_label" ]; then
  report_failure "初回描画の成立行からラベルを読み取れませんでした: ${heartbeat_line}"
fi
echo "OK: 他のウィンドウの初回描画が成立しました: ${heartbeat_line}"

# --- 3. 生成の失敗（引き金と、通常の失敗側の分岐の記録）を待つ -------------------------
trigger_line=""
failure_line=""
failure_deadline=$(( $(date +%s) + 20 ))
while [ "$(date +%s)" -lt "$failure_deadline" ]; do
  trigger_line=$(record_find_from 0 "\[検証\] ウィンドウ生成の失敗を起こす（衝突相手: label=${first_label}）")
  failure_line=$(record_find_from 0 "ウィンドウを生成できなかった: label=")
  if [ -n "$failure_line" ]; then
    break
  fi
  sleep "$x11_poll_sleep"
done
if [ -z "$trigger_line" ]; then
  report_failure "引き金の記録（'[検証] ウィンドウ生成の失敗を起こす（衝突相手: label=${first_label}）'）が現れませんでした（fail-window の引き金が働いていない）"
fi
if [ -z "$failure_line" ]; then
  report_failure "生成の失敗の記録（'ウィンドウを生成できなかった: label=…'）が現れませんでした"
fi
failed_label=$(printf '%s\n' "$failure_line" | sed -n 's/.*label=\([^ ]*\).*/\1/p')
if [ -z "$failed_label" ]; then
  report_failure "失敗の記録からラベルを読み取れませんでした: ${failure_line}"
fi
if [ "$failed_label" = "$first_label" ]; then
  report_failure "失敗したラベルが既存のウィンドウと同じです（衝突していない）: ${failure_line}"
fi
echo "OK: 生成の失敗が起きました: ${failure_line}"
echo "    引き金の記録: ${trigger_line}"

# --- 4. 提示（診断の保存先の 1 件の記録）を確かめる ------------------------------------
presentation_deadline=$(( $(date +%s) + 10 ))
while [ "$(date +%s)" -lt "$presentation_deadline" ]; do
  if [ -f "$window_error_record" ] &&
    grep -qF "対象のウィンドウ: ${failed_label}" "$window_error_record"; then
    break
  fi
  sleep "$x11_poll_sleep"
done
if [ ! -f "$window_error_record" ]; then
  report_failure "提示の記録（${window_error_record}）がありません（失敗が提示されていない）"
fi
if ! grep -qF 'ウィンドウを生成できませんでした。' "$window_error_record"; then
  report_failure "提示の記録に失敗の宣言（'ウィンドウを生成できませんでした。'）がありません"
fi
if ! grep -qF "対象のウィンドウ: ${failed_label}" "$window_error_record"; then
  report_failure "提示の記録が失敗したラベル（${failed_label}）を名指ししていません"
fi
if ! grep -qF '理由: ' "$window_error_record"; then
  report_failure "提示の記録に失敗の理由（'理由: '）がありません"
fi
if ! grep -qF '既に開いている他のウィンドウの動作は中断していません。' "$window_error_record"; then
  report_failure "提示の記録に「他のウィンドウの動作は中断していません」がありません"
fi
echo "OK: 失敗が提示されました（${window_error_record}）:"
sed 's/^/    /' "$window_error_record"

# --- 5. 失敗のあとも他のウィンドウが生き続けることを数秒観測する -------------------------
# **1 回の観測では足りない**（失敗の直後にたまたま生きていただけかもしれない）。識別子の集合が
# 変わらないこととプロセスの生存を、settle 秒のあいだ繰り返し確かめる。
settle_secs=3
settle_end=$(( $(date +%s) + settle_secs ))
observed=0
while [ "$(date +%s)" -lt "$settle_end" ]; do
  x11_observe_window "$title" "$min_w" "$min_h"
  if [ "$x11_window_ids" != "$first_ids" ]; then
    report_failure "失敗の後に X のウィンドウ集合が変わりました（期待: $(printf '%s' "$first_ids" | tr '\n' ' ') / 観測: $(printf '%s' "$x11_window_ids" | tr '\n' ' ')）"
  fi
  if ! x11_process_alive "$x11_pid"; then
    report_failure "失敗の後にアプリのプロセスが終了しました（pid=${x11_pid}。2.10 は失敗を隔離して動作を続けることを要求する）"
  fi
  observed=$((observed + 1))
  sleep "$x11_poll_sleep"
done
echo "OK: 失敗の後も他のウィンドウ（${first_label}）は同じ識別子のまま残り、プロセス pid=${x11_pid} も生存している（${observed} 回観測）"

# --- 6. 引き継ぎで新しいウィンドウが開き、描画まで成立すること ---------------------------
# 「生きている」だけでは足りない — 生成の失敗がレジストリや描画の監視を壊していれば、次の生成が
# 失敗するか描画が成立しない。**実際に次のウィンドウが開いて描けること**を要求する。
before_handover=$(record_count)
GDK_BACKEND=x11 nohup "$app" "$document" >"$x11_log.second" 2>&1 &
second_pid=$!

handover_line=$(record_find_from "$before_handover" "二重起動を引き継ぎました.*ドキュメント要求 ")
handover_deadline=$(( $(date +%s) + timeout_secs ))
new_label=""
new_heartbeat=""
while [ "$(date +%s)" -lt "$handover_deadline" ]; do
  if [ -z "$handover_line" ]; then
    handover_line=$(record_find_from "$before_handover" "二重起動を引き継ぎました.*ドキュメント要求 ")
  fi
  # 引き継ぎで開いたウィンドウのラベル（既存のラベル以外で最後に開かれたもの）。
  opened_labels=$(record_find_from "$before_handover" "ウィンドウを開いた: label=" |
    sed -n 's/.*label=\([^ ]*\).*/\1/p' || true)
  for _label in $opened_labels; do
    if [ "$_label" != "$first_label" ]; then
      new_label=$_label
    fi
  done
  if [ -n "$new_label" ]; then
    new_heartbeat=$(record_find_from "$before_handover" "${heartbeat_re}" |
      grep -F "label=${new_label} " | tail -n 1 || true)
    if [ -n "$new_heartbeat" ]; then
      break
    fi
  fi
  sleep "$x11_poll_sleep"
done
if [ -z "$handover_line" ]; then
  report_failure "引き継ぎの記録（'二重起動を引き継ぎました … ドキュメント要求 …'）が現れませんでした"
fi
if [ -z "$new_label" ]; then
  report_failure "引き継ぎで新しいウィンドウが開かれませんでした（失敗の後に生成が働いていない）"
fi
if [ -z "$new_heartbeat" ]; then
  report_failure "引き継ぎで開いたウィンドウ（label=${new_label}）の初回描画が成立しませんでした（描画の監視が壊れている）"
fi
echo "OK: 失敗の後でも引き継ぎで新しいウィンドウが開き、描画が成立しました: ${new_heartbeat}"

# --- 7. 失敗したウィンドウが現れていない（レジストリの残骸が無い） ------------------------
x11_observe_window "$title" "$min_w" "$min_h"
if [ "$x11_window_count" -ne 2 ]; then
  report_failure "判定の終わりのウィンドウ数が 2 ではありません（観測 ${x11_window_count} 件。失敗したウィンドウが現れたか、既存のウィンドウが消えています）"
fi
for _id in $first_ids; do
  if ! printf '%s\n' "$x11_window_ids" | grep -qF "$_id"; then
    report_failure "失敗の前に在ったウィンドウ（${_id}）が判定の終わりに残っていません"
  fi
done
if grep -F "ウィンドウを開いた: label=${failed_label} " "$record" >/dev/null 2>&1; then
  report_failure "失敗したはずのラベル（${failed_label}）が 'ウィンドウを開いた' として記録されています（過剰なウィンドウが現れた）"
fi
if grep -F '初回描画が成立しなかった' "$record" >/dev/null 2>&1; then
  report_failure "記録に描画不成立（'初回描画が成立しなかった'）が現れました（失敗が他のウィンドウの描画を壊している）"
fi
echo "OK: 失敗したウィンドウ（${failed_label}）は現れておらず、ウィンドウ数は 2（既存 ${first_label} + 引き継ぎ ${new_label}）"

echo "OK: ウィンドウの生成の失敗は提示され（記録 ${window_error_record}）、既に開いている他のウィンドウの動作を中断していない"
exit 0

#!/bin/sh
# 大きなペイロードの一括転送を実機で検証する（tasks.md 10.8 / 要件 4.5）。**Linux / macOS 用**。
#
# 使い方:
#   sh scripts/check-bulk-transfer.sh <検証用の形の実行ファイル> <タイムアウト秒> <記録ファイル> <行数の一覧>
#
#   - 検証用の形の実行ファイル: **`npx tauri build --no-bundle --features verification-triggers`**
#                               の生成物（例 `target/release/jxcel`）。**この形で作ること** —
#                               素の `cargo build … --features verification-triggers` は
#                               `tauri-build` が `dev` の cfg を選び（`dev = !custom-protocol`。
#                               このリポジトリは `custom-protocol` を持たない）、**フロントエンドを
#                               埋め込まない**ので、起動しても転送が起きず「結果が現れませんでした」
#                               という偽の失敗になる（`ci.yml` の配布物生成の注記と同じ理由）。
#                               **配布物（既定のビルド）は検証専用の環境変数を読まない**ので、
#                               この検査は成立しない（渡すと「一括転送が起きない」で exit 1）。
#   - タイムアウト秒          : 結果の行が現れるのを待つ上限。
#   - 記録ファイル            : アプリの診断記録（4.4 の保存先の `jxcel.log`）。**消さない** —
#                               起動中のアプリが開いたファイルへ書き続けるため、消すと行が失われる。
#                               代わりに起動直前の行数を取り、それ以降の行だけを調べる（前の段の
#                               行で偽の成功をしない）。
#   - 行数の一覧              : `,` 区切りの正の整数（2〜4 件。例 `100,100000`）。**少なくとも
#                               1 件は 100,000 以上**でなければならない（10 万行規模の受入）。
#
# # 検証する 2 つの主張（tasks.md 10.8 の完了状態）
#
#   (i) 10 万行規模のデータが **1 回の呼び出し**で受け渡せる。行数ごとに、Rust 側の `bulk_echo`
#       の記録行（呼び出しごとに 1 行。7.2）が**ちょうど 1 行**であり、フロントエンドが報告する
#       送信バイト数・受信バイト数・`byteIdentical` が一致することを要求する。
#  (ii) **呼び出し回数が行数に比例しない**。行数の一覧の**すべて**で呼び出し回数が同じ定数 1 で
#       あることを要求する（一覧は 2 件以上でなければならない — 1 件では定数であることを
#       示せない）。加えて、その行数の転送以外の `bulk_echo` 呼び出しが 1 件も無いことを要求する
#       （合計 = 行数の件数）。
#
# # 何をもって「1 回の呼び出し」とするか（2 つの独立した事実を突き合わせる）
#
#   - **Rust 側の記録**: `bulk_echo` は呼び出しごとに `受信バイト数 = <N>` を記録する（7.2）。
#     これが**呼び出しの実在**の根拠であり、その行数がそのまま**呼び出し回数**である。
#   - **フロントエンドの報告**: `src/shell/verificationBulk.ts` が自分で数えた呼び出し回数と、
#     送信・受信のバイト数、往復のバイト一致をイベントで送り、Rust（`verification-triggers` の
#     下にだけ存在するリスナ）が記録へ写す（`一括転送の結果: {"rows":…,"sentBytes":…,…}`）。
#   両者を突き合わせるので、**どちらか一方の自己申告だけでは通らない**。
#
# # 「本当に実行されたか」の錠前（空振りで緑にしない）
#
#   判定の前に、**要求の行**（`検証用の一括転送を要求した:`。検証用の形の Rust だけが出す）と、
#   **行数ごとの `bulk_echo` の行**と、**結果の行**の 3 つすべてを要求する。転送が 1 件も起きて
#   いなければ（配布物・環境変数の不正・フロントエンドの失敗）、要求の行か `bulk_echo` の行が
#   欠けるので**必ず非 0 で落ちる**。期待バイト数は `行数 × 1 行あたりのバイト数` としてこの
#   スクリプトが計算するので、`> 0` は構造的に保証される（加えて明示的に検査する）。
#
# # 1 行あたりのバイト数は対の契約である
#
#   `bulk_line_bytes` は **`src/shell/verificationBulk.ts` の `BULK_LINE_BYTES`** と
#   **`src-tauri/src/window/mod.rs` の `VERIFY_BULK_LINE_BYTES`** と同じ値（47）でなければ
#   ならない。3 箇所で独立に定義しているのは、検査器がフロントエンドの計算を信用せずに
#   期待バイト数を再計算するためである（**同じ値が 3 箇所で一致すること自体が検査である**）。
#
# # 終了コード
#   0 = 2 つの主張がすべて成立 / 1 = 検査失敗（転送が起きない・呼び出し回数が 1 でない・
#   往復が一致しない・余分な呼び出しがある・期限内に結果が出ない）/ 2 = 入力が使えない
#   （引数・実行ファイル・記録ファイルの指定・行数の一覧の形・DISPLAY 不在）
#
# SC1091 / SC2154: 置き場は**同じリポジトリのファイル**であり、`x11_kill_tree` はそこで定義
# される。qlty は検査対象を一時ディレクトリへ写してから shellcheck にかけるため、shellcheck は
# 置き場をたどれない（実際の実行では `$0` からの相対で解決する）。
# shellcheck disable=SC1091,SC2154
set -eu

bulk_line_bytes=47

_x11_lib_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
# 片付けは置き場の 1 実装を使う（**フォークしない**）— AppImage を展開実行するとラッパーの子と
# して本体が動くため、ラッパーだけを終了すると本体が残る。`x11_kill_tree` がまさにそのための
# 実装である（10.5 が発見した欠陥の修正を含む）。
. "$_x11_lib_dir/lib/x11-window.sh"

usage() {
  echo "使い方: sh scripts/check-bulk-transfer.sh <検証用の形の実行ファイル> <タイムアウト秒> <記録ファイル> <行数の一覧>" >&2
  exit 2
}

[ "$#" -eq 4 ] || usage

app=$1
timeout_seconds=$2
record=$3
rows_list=$4

case "$timeout_seconds" in
  '' | *[!0-9]*)
    echo "NG: タイムアウト秒が整数ではありません: $timeout_seconds" >&2
    exit 2
    ;;
esac
if [ "$timeout_seconds" -le 0 ]; then
  echo "NG: タイムアウト秒が 0 以下です: $timeout_seconds" >&2
  exit 2
fi

if [ -z "$record" ]; then
  echo "NG: 記録ファイルが指定されていません" >&2
  exit 2
fi

if [ ! -f "$app" ] || [ ! -x "$app" ]; then
  echo "NG: 実行ファイルがありません（実行権限も必要です）: $app" >&2
  exit 2
fi

case $(uname -s) in
  Linux)
    os_mode=linux
    ;;
  Darwin)
    os_mode=macos
    ;;
  *)
    echo "NG: この検査は Linux / macOS 用です（uname -s = $(uname -s)）" >&2
    exit 2
    ;;
esac

if [ "$os_mode" = linux ] && [ -z "${DISPLAY:-}" ]; then
  echo "NG: DISPLAY が設定されていません（GUI アプリを起動できません。CI では xvfb-run の下で呼ぶ）" >&2
  exit 2
fi

# --- 行数の一覧を解釈する ---------------------------------------------------
#
# `,` 区切りの正の整数を 2〜4 件。**1 件では「呼び出し回数が行数に比例しない」ことを示せない**
# ので受け付けない。少なくとも 1 件は 100,000 以上でなければならない（10 万行規模の受入）。
# 1 件あたりの上限は Rust 側（1,000,000 行）とそろえる。
max_rows=1000000
rows_count=0
rows_values=''
bytes_values=''
large_present=0

old_ifs=$IFS
IFS=,
# `*` のような値でファイル名へ展開しないよう、分割の間だけ glob を止める。
set -f
# shellcheck disable=SC2086 # `,` 区切りへ意図的に分割する
set -- $rows_list
set +f
IFS=$old_ifs

if [ "$#" -lt 2 ] || [ "$#" -gt 4 ]; then
  echo "NG: 行数の一覧は 2〜4 件の ',' 区切りでなければなりません: $rows_list" >&2
  exit 2
fi

for value in "$@"; do
  case "$value" in
    '' | *[!0-9]*)
      echo "NG: 行数が正の整数ではありません: $value" >&2
      exit 2
      ;;
  esac
  if [ "$value" -le 0 ] || [ "$value" -gt "$max_rows" ]; then
    echo "NG: 行数が範囲外です（1 以上 ${max_rows} 以下）: $value" >&2
    exit 2
  fi
  expected=$((value * bulk_line_bytes))
  rows_count=$((rows_count + 1))
  rows_values="${rows_values:+${rows_values},}${value}"
  bytes_values="${bytes_values:+${bytes_values},}${expected}"
  if [ "$value" -ge 100000 ]; then
    large_present=1
  fi
done

if [ "$large_present" -ne 1 ]; then
  echo "NG: 行数の一覧に 100,000 以上の行数がありません（10 万行規模の受入を検査できない）: $rows_list" >&2
  exit 2
fi

echo "検証: 行数=${rows_values} / 1 行あたり=${bulk_line_bytes} B / 期待バイト数=${bytes_values}"

# --- 作業領域と後始末 -------------------------------------------------------
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/jxcel-bulk-transfer.XXXXXX")
app_pid=''
app_log="$tmp_dir/app.log"

# アプリを必ず終了する（後続の段へ持ち越さない。10.5 / 10.7 の申し送りと同じ理由）。
# **子孫を先に終了する** — AppImage を展開実行（`APPIMAGE_EXTRACT_AND_RUN=1`）した場合、
# ラッパーの子として本体が動くため、ラッパーだけを終了すると本体が残る（置き場の
# `x11_kill_tree` の doc を参照）。
# shellcheck disable=SC2329 # 下の trap から呼ばれる
cleanup() {
  if [ -n "${app_pid:-}" ] && kill -0 "$app_pid" 2>/dev/null; then
    x11_kill_tree "$app_pid"
    ticks=0
    while [ "$ticks" -lt 20 ] && kill -0 "$app_pid" 2>/dev/null; do
      sleep 0.1
      ticks=$((ticks + 1))
    done
    if kill -0 "$app_pid" 2>/dev/null; then
      kill -9 "$app_pid" 2>/dev/null || true
    fi
    wait "$app_pid" 2>/dev/null || true
  fi
  app_pid=''
  if [ -n "${tmp_dir:-}" ] && [ -d "$tmp_dir" ]; then
    rm -rf "$tmp_dir"
  fi
  :
}
trap cleanup 0 1 2 15

# 記録の現在の行数（無ければ 0）。**起動の直前に取る。**
record_lines() {
  if [ -f "$record" ]; then
    wc -l < "$record" | tr -d ' '
  else
    echo 0
  fi
}

# 記録の `<開始行数>` より後から、`grep -E` のパターンに一致する行を全部出す。
record_after() {
  from=$1
  pattern=$2
  [ -f "$record" ] || return 0
  tail -n "+$((from + 1))" "$record" 2>/dev/null | grep -E "$pattern" || true
}

# 記録の `<開始行数>` より後から、`grep -E` のパターンに一致する行の数。
record_count_after() {
  from=$1
  pattern=$2
  if [ ! -f "$record" ]; then
    echo 0
    return 0
  fi
  tail -n "+$((from + 1))" "$record" 2>/dev/null |
    awk -v p="$pattern" '$0 ~ p { n += 1 } END { print n + 0 }'
}

report_failure() {
  echo "NG: $1" >&2
  echo "--- アプリの出力（末尾）: $app_log ---" >&2
  tail -n 30 "$app_log" 2>/dev/null >&2 || true
  if [ -f "$record" ]; then
    echo "--- 診断記録（末尾）: $record ---" >&2
    tail -n 40 "$record" >&2
  else
    echo "--- 診断記録: ${record}（存在しません） ---" >&2
  fi
  exit 1
}

# --- 起動 -------------------------------------------------------------------
baseline=$(record_lines)
echo "起動前の記録の行数: ${baseline}（これ以降の行だけを調べる）"
echo "起動: ${app}（JXCEL_VERIFICATION_BULK_ROWS=${rows_values}）"
if [ "$os_mode" = linux ]; then
  nohup env GDK_BACKEND=x11 JXCEL_VERIFICATION_BULK_ROWS="$rows_values" "$app" \
    >"$app_log" 2>&1 &
else
  nohup env JXCEL_VERIFICATION_BULK_ROWS="$rows_values" "$app" >"$app_log" 2>&1 &
fi
app_pid=$!

# --- 結果の行が出るまで待つ -------------------------------------------------
#
# 待つのは**行数ごとの結果の行**（フロントエンドの報告。Rust が記録へ写す）である。行数ごとに
# ちょうど 1 行なので、期待する行数は一覧の件数に等しい。8.2 の初回描画（転送はその後に回る）と
# 10 万行の往復を見込んで呼び出し側がタイムアウトを決める。
result_pattern='一括転送の結果:.*"invocations":'
deadline=$(( $(date +%s) + timeout_seconds ))
observed=0
while [ "$(date +%s)" -le "$deadline" ]; do
  observed=$(record_count_after "$baseline" "$result_pattern")
  if [ "$observed" -ge "$rows_count" ]; then
    break
  fi
  # 途中で落ちた場合は待ち続けない（結果は現れない）。
  if ! kill -0 "$app_pid" 2>/dev/null; then
    break
  fi
  sleep 0.2
done

if [ "$observed" -lt "$rows_count" ]; then
  report_failure "行数ごとの一括転送の結果が ${timeout_seconds} 秒以内に現れませんでした（観測: ${observed} 行。期待 ${rows_count} 行。転送が 1 件も起きていないか、失敗しています）"
fi

# --- 主張 (i)/(ii) の判定 ---------------------------------------------------
#
# 期待バイト数はこのスクリプトが計算し直した値である（フロントエンドの申告は信用しない）。
# 行数ごとに、Rust 側の `bulk_echo` の行がちょうど 1 行、フロントエンドの結果の行がちょうど 1 行、
# 両者が期待バイト数で一致することを要求する。加えて、合計の呼び出し回数が一覧の件数に等しい
# （**余分な呼び出しが 1 件も無い**）ことを要求する。

# 要求の行（検証用の形の Rust だけが出す）。**転送が本当に要求されたこと**の根拠である。
request_lines=$(record_count_after "$baseline" '検証用の一括転送を要求した:')
if [ "$request_lines" -ne 1 ]; then
  report_failure "要求の行（'検証用の一括転送を要求した:'）が 1 行ではありません（観測 ${request_lines} 行）。検証用の形で起動していないか、環境変数が解釈されていません"
fi
request_line=$(record_after "$baseline" '検証用の一括転送を要求した:' | head -n 1)
echo "要求の記録: $request_line"

total_calls=$(record_count_after "$baseline" 'jxcel::commands::bulk.*受信バイト数 = ')
if [ "$total_calls" -ne "$rows_count" ]; then
  report_failure "一括転送の呼び出しの合計が行数の件数と一致しません（観測 ${total_calls} 回。期待 ${rows_count} 回 = 行数ごとにちょうど 1 回）。行数に比例した呼び出し（行ごとの往復）が起きていないか、重複した呼び出しがあります"
fi

for value in "$@"; do
  expected=$((value * bulk_line_bytes))

  # 「空振りで緑にしない」錠前: 期待バイト数は正でなければならない（構造的に正だが明示する）。
  if [ "$expected" -le 0 ]; then
    report_failure "行数 ${value} の期待バイト数が 0 以下です（ペイロードが空である）"
  fi

  # Rust 側の `bulk_echo` の行。**呼び出しの実在**と**回数**の根拠である。
  calls=$(record_count_after "$baseline" "jxcel::commands::bulk.*受信バイト数 = ${expected}$")
  if [ "$calls" -ne 1 ]; then
    report_failure "行数 ${value}（期待 ${expected} B）の bulk_echo の記録行が 1 行ではありません（観測 ${calls} 行）。1 回の呼び出しで受け渡せていないか、同じ大きさの転送が重複しています"
  fi

  # フロントエンドの結果の行。**送信・受信バイト数・往復の一致・呼び出し回数**を含む。
  result=$(record_after "$baseline" "\"rows\":${value}[,}]" |
    grep -E "\"sentBytes\":${expected}[,}]" |
    grep -E "\"receivedBytes\":${expected}[,}]" |
    grep -F '"byteIdentical":true' |
    grep -E '"invocations":1([,}])' || true)
  result_lines=$(printf '%s\n' "$result" | grep -c '"rows":' || true)
  if [ "$result_lines" -ne 1 ]; then
    report_failure "行数 ${value}（期待 ${expected} B）の一括転送の結果が 1 行ではありません（観測 ${result_lines} 行）。送信バイト数・受信バイト数・バイト一致・呼び出し回数 1 のいずれかが期待と一致しません"
  fi
  echo "行数=${value} 呼び出し回数=1 期待バイト数=${expected} / $(printf '%s\n' "$result" | head -n 1)"
done

echo "OK: 行数の一覧（${rows_values}）のすべてで、1 回の呼び出しがちょうど 1 行であり、往復のバイト数が一致した"
echo "OK: 呼び出し回数は行数によらず定数 1 である（合計 ${total_calls} 回 = 行数の件数 ${rows_count} 件。行数に比例しない）"
exit 0

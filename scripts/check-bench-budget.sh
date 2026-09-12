#!/bin/sh
# check-bench-budget.sh — criterion の計測値に対する性能予算の機械検査
#
# 根拠（.kiro/specs/document-format/design.md「Performance Tests」・
# requirements 8.1 / 8.2・tasks.md 8.9 / .kiro/specs/schema-engine/design.md
# 「Performance」・requirements 10.1 / 10.3・tasks.md 9.2）:
#   10 万行 × 30 列のドキュメントを **開く 3 秒以内**、**保存 2 秒以内**で処理する。
#   加えて、10 万行 × 30 列のシートの**全件検証を 1 秒以内**に完了する（要件 10.1, 10.3。
#   開く 3 秒の内側で走るため、その予算を圧迫しない上限）。
#   `.github/workflows/bench.yml` の `cargo bench -p document-format -p schema-engine` が
#   残す criterion の計測値（`target/criterion/**/new/estimates.json`）を読み、平均（mean）の
#   点推定値が予算を超えていれば非 0 で終了し、CI を失敗させる。これにより予算超過を
#   機能追加と同時に検出する。
#
# 単位: criterion の `estimates.json` の値はナノ秒（ns）である（criterion の既定単位）。
#   予算は 3 秒 = 3_000_000_000 ns / 2 秒 = 2_000_000_000 ns / 1 秒 = 1_000_000_000 ns
#   （シートの全件検証）。
#
# 計測環境（要件 8.3）: 予算判定は CI の GitHub-hosted ランナー（ubuntu-latest /
#   macos-latest / windows-latest）で計測した release の値に対して行う。private
#   リポジトリのランナーは 2 vCPU（macOS は 3 コア M1）で要件 8.3 の「4 コア以上」より
#   弱いが、閾値は要件値のまま使う（弱い環境で通れば要件の環境でも通るとみなす
#   保守的な代理。bench.yml 冒頭の「計測環境」）。ローカルでも同じコマンドで同じ判定を
#   再現できる（`cargo bench -p document-format --bench large_document` と
#   `cargo bench -p schema-engine --bench large_sheet` の後に本スクリプト）。
#
# POSIX sh 互換: `bash scripts/check-bench-budget.sh` が Linux / macOS /
# Windows (Git Bash) のいずれでも動作すること（3 OS マトリクス共用）。
#
# 使い方: sh scripts/check-bench-budget.sh [criterion ディレクトリ] [開く予算 ns] [保存予算 ns] [検証予算 ns]
#   予算の引数は既定（要件値）を上書きする。CI は引数なしで呼ぶ
#   （予算判定の閾値を CI から差し替えない）。
# 終了コード: 0 = 全予算内 / 1 = 予算超過 / 2 = 計測値が無い・解釈できない
set -eu

OPEN_BUDGET="${2:-3000000000}"
SAVE_BUDGET="${3:-2000000000}"
VALIDATE_BUDGET="${4:-1000000000}"
CRITERION="${1:-target/criterion}"

# criterion の mean.point_estimate（ナノ秒, 浮動小数）を取り出す。
#
# estimates.json は 1 行のコンパクト JSON だが、整形されていても読めるよう全行を連結して
# から `"mean"` の直後の `"point_estimate"` を探す（greedy な正規表現で別の統計量の
# 点推定値を拾わないため、文字列の位置で切り出す）。数値リテラル（指数表記を含む）だけを
# 返す。
mean_point_estimate_ns() {
  awk '
    { line = line $0 }
    END {
      i = index(line, "\"mean\"")
      if (i == 0) exit 3
      rest = substr(line, i + 6)
      key = "\"point_estimate\":"
      j = index(rest, key)
      if (j == 0) exit 3
      rest = substr(rest, j + length(key))
      # 整形された JSON（`"point_estimate": 617…`）でも読めるよう、先頭の空白を落としてから
      # 数値リテラルを行頭で照合する（criterion の出力は現在 1 行のコンパクト JSON だが、
      # 出力形式に依存しない）。
      sub(/^[ \t]*/, "", rest)
      if (match(rest, /-?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?/) != 1) exit 3
      print substr(rest, 1, RLENGTH)
    }
  ' "$1"
}

# 1 つのベンチの計測値を予算と比較する。超過なら 1、計測値が読めなければ 2 を返す。
check_budget() {
  label="$1"
  relative="$2"
  budget_ns="$3"
  estimate="$CRITERION/$relative/new/estimates.json"

  if [ ! -f "$estimate" ]; then
    echo "check-bench-budget: 計測値がありません: $estimate" >&2
    return 2
  fi

  measured_ns=$(mean_point_estimate_ns "$estimate") || {
    echo "check-bench-budget: 計測値を解釈できません: $estimate" >&2
    return 2
  }

  seconds=$(awk -v ns="$measured_ns" 'BEGIN { printf "%.3f", ns / 1000000000 }')
  budget_seconds=$(awk -v ns="$budget_ns" 'BEGIN { printf "%.3f", ns / 1000000000 }')

  if awk -v m="$measured_ns" -v b="$budget_ns" 'BEGIN { exit (m > b) ? 1 : 0 }'; then
    echo "check-bench-budget: OK  $label ${seconds}s <= ${budget_seconds}s"
    return 0
  fi

  echo "check-bench-budget: VIOLATION  $label ${seconds}s > 予算 ${budget_seconds}s" >&2
  return 1
}

# 各チェックの終了コードを集約する。2（計測値が読めない）が最も重く、次に 1（予算超過）。
# すべてのベンチを必ず検査し、どれが問題かを 1 回の実行で報告する。
status=0

record_status() {
  rc="$1"
  case "$rc" in
    2) status=2 ;;
    1) [ "$status" -eq 2 ] || status=1 ;;
  esac
}

check_budget "open (10万行×30列)" "large_document/open_100k_rows_x_30_columns" "$OPEN_BUDGET" ||
  record_status $?
check_budget "save (10万行×30列)" "large_document/save_100k_rows_x_30_columns" "$SAVE_BUDGET" ||
  record_status $?
check_budget "validate (10万行×30列)" "large_sheet/validate_100k_rows_x_30_columns" "$VALIDATE_BUDGET" ||
  record_status $?

exit "$status"

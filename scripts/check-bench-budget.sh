#!/bin/sh
# check-bench-budget.sh — criterion の計測値に対する性能予算の機械検査
#
# 根拠（.kiro/specs/document-format/design.md「Performance Tests」・
# requirements 8.1 / 8.2・tasks.md 8.9 / .kiro/specs/schema-engine/design.md
# 「Performance」・requirements 10.1 / 10.3・tasks.md 9.2 /
# .kiro/specs/document-session/design.md「Benchmarks（予算のゲート）」・
# requirements 3.5 / 5.6 / 8.1・tasks.md 5.1）:
#   10 万行 × 30 列のドキュメントを **開く 3 秒以内**、**保存 2 秒以内**で処理する。
#   加えて、10 万行 × 30 列のシートの**全件検証を 1 秒以内**に完了する（要件 10.1, 10.3。
#   開く 3 秒の内側で走るため、その予算を圧迫しない上限）。さらに、10 万行 × 30 列の
#   **すべてのセルを置き換える一括の適用を 1 秒以内**に完了する（要件 3.5。セッションの
#   経路の予算であり、開く 3 秒の内側で走る）。
#   さらに、10 万行 × 30 列のシートへの**1 万行の貼り付けを 3 秒以内**に完了する
#   （.kiro/specs/data-grid/requirements.md 11.5・design.md「Performance & Scalability」・
#   tasks.md 9.1。判定の場は `benches/large_grid.rs` の `large_grid/paste_10k` ただ 1 つで
#   あり、data-grid の他の計測（窓の符号化・順序の再計算）は要件に絶対値が無いため
#   判定しない — 計測値は criterion のレポートに残る）。
#   さらに、マクロの一括処理を **10 万行 × 30 列の全行読み + 集計は 10 秒以内**、
#   **1 万行 × 30 列の書き換えは 5 秒以内**に完了する（.kiro/specs/macro-runtime/
#   requirements.md 11.1・11.2・design.md「Performance / Load」・tasks.md 5.3。判定の場は
#   `crates/macro-runtime/benches/bulk.rs` の 2 本であり、実行 1 回の全体（isolate の生成・
#   変換・実行・値の往復・変更の集約）を測る）。
#   `.github/workflows/bench.yml` の
#   `cargo bench -p document-format -p schema-engine -p document-session -p data-grid
#   -p macro-runtime` が
#   残す criterion の
#   計測値（`target/criterion/**/new/estimates.json`）を読み、平均（mean）の点推定値が予算を
#   超えていれば非 0 で終了し、CI を失敗させる。これにより予算超過を機能追加と同時に検出する。
#
# 単位: criterion の `estimates.json` の値はナノ秒（ns）である（criterion の既定単位）。
#   予算は 3 秒 = 3_000_000_000 ns / 2 秒 = 2_000_000_000 ns / 1 秒 = 1_000_000_000 ns
#   （シートの全件検証・一括の適用・1 万行の貼り付け）。
#
# 計測環境（要件 8.3）: 予算判定は CI の GitHub-hosted ランナー（ubuntu-latest /
#   macos-latest / windows-latest）で計測した release の値に対して行う。private
#   リポジトリのランナーは 2 vCPU（macOS は 3 コア M1）で要件 8.3 の「4 コア以上」より
#   弱いが、閾値は要件値のまま使う（弱い環境で通れば要件の環境でも通るとみなす
#   保守的な代理。bench.yml 冒頭の「計測環境」）。ローカルでも同じコマンドで同じ判定を
#   再現できる（`cargo bench -p document-format -p schema-engine -p document-session
#   -p data-grid` の後に本スクリプト）。
#
# POSIX sh 互換: `bash scripts/check-bench-budget.sh` が Linux / macOS /
# Windows (Git Bash) のいずれでも動作すること（3 OS マトリクス共用）。
#
# 使い方: sh scripts/check-bench-budget.sh [criterion ディレクトリ] [開く予算 ns] [保存予算 ns] [検証予算 ns] [一括の適用の予算 ns] [1 万行の貼り付けの予算 ns] [10 万行の読みの予算 ns] [1 万行の書き換えの予算 ns]
#   予算の引数は既定（要件値）を上書きする。CI は引数なしで呼ぶ
#   （予算判定の閾値を CI から差し替えない）。
# 終了コード: 0 = 全予算内 / 1 = 予算超過 / 2 = 計測値が無い・解釈できない
set -eu

OPEN_BUDGET="${2:-3000000000}"
SAVE_BUDGET="${3:-2000000000}"
VALIDATE_BUDGET="${4:-1000000000}"
# 一括の適用（全セルの置き換え。要件 3.5、tasks.md 5.1）の予算。既定は要件値の 1 秒。
BULK_BUDGET="${5:-1000000000}"
# 1 万行の貼り付け（要件 7.7, 11.5、tasks.md 9.1）の予算。既定は要件値の 3 秒。
PASTE_BUDGET="${6:-3000000000}"
# 10 万行 × 30 列の全行読み + 集計（macro-runtime の要件 11.1、tasks.md 5.3）の予算。
# 既定は要件値の 10 秒。
READ_BUDGET="${7:-10000000000}"
# 1 万行 × 30 列の書き換え（macro-runtime の要件 11.2、tasks.md 5.3）の予算。既定は要件値の
# 5 秒。
REWRITE_BUDGET="${8:-5000000000}"
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
check_budget "apply (10万行×30列)" "large_session/apply_bulk_edit_100k_rows_x_30_columns" "$BULK_BUDGET" ||
  record_status $?
# セッションの経路の読み込みと保存も判定する（要件 5.6, 8.1。形式の側の計測とは別の経路で
# あり、bench id を `large_session/` の下に持つ）。
check_budget "open+hold (10万行×30列)" "large_session/open_and_hold_100k_rows_x_30_columns" "$OPEN_BUDGET" ||
  record_status $?
check_budget "save (10万行×30列, セッション経路)" "large_session/save_100k_rows_x_30_columns" "$SAVE_BUDGET" ||
  record_status $?
# data-grid の 1 万行の貼り付け（要件 7.7, 11.5。tasks.md 9.1）。`benches/large_grid.rs` の
# `paste_10k` が残す計測値であり、**計測が無ければ 2（fail-closed）**である — data-grid の
# ベンチをワークフローの `cargo bench` から落とす変更は、この 2 で検出される。
check_budget "paste (1万行×30列)" "large_grid/paste_10k" "$PASTE_BUDGET" ||
  record_status $?
# macro-runtime の一括処理（要件 11.1, 11.2。tasks.md 5.3）。`benches/bulk.rs` の 2 本が
# 残す計測値であり、**計測が無ければ 2（fail-closed）**である — macro-runtime のベンチを
# ワークフローの `cargo bench` から落とす変更は、この 2 で検出される。
check_budget "read (10万行×30列, マクロの一括の読み+集計)" "bulk/read_100k_rows_x_30_columns" "$READ_BUDGET" ||
  record_status $?
check_budget "rewrite (1万行×30列, マクロの書き換え)" "bulk/rewrite_10k_rows" "$REWRITE_BUDGET" ||
  record_status $?

exit "$status"

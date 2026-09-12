#!/bin/sh
# check-startup-budget.sh — 起動から操作可能なウィンドウが表示されるまでの時間の予算判定
#
# 根拠（.kiro/specs/app-shell/design.md「BuildPipeline」のゲート一覧・
# requirements 1.3 / 6.8・tasks.md 10.3）:
#   配布物が起動されたとき、**操作可能なウィンドウが表示されるまで 2 秒以内**
#   （要件 1.3）。`.github/workflows/ci.yml` の各 OS の起動検証（tasks.md 1.5）が
#   計測した値を読み、要件値を超えていれば非 0 で終了し、パイプラインを失敗させる
#   （要件 6.8）。これにより起動時間の退行を配布物の生成と同時に検出する。
#
# 計測値の形式: 1 行 1 計測で `<プラットフォーム>=<ミリ秒>`。例: `linux=812`。
#   書き出し元は各 OS の起動検証の段であり、Linux は `scripts/check-x11-window.sh`、
#   macOS は Swift のポーリング、Windows は pwsh のポーリングである。1 つのランナーは
#   自分の OS の 1 行だけを書くため、この判定は**ランナーごとに 1 つの値**を対象に走る。
#   3 つの値は 3 OS マトリクスの各ジョブの出力（起動検証の段と本スクリプトの両方）に
#   現れる。形式が壊れた行・読み取れないファイルは exit 2 で落ちる（無言で通る経路は無い）。
#
# 判定の考え方（tasks.md 10.3「予算に対して最も余裕がないプラットフォームを基準に
#   判断する」）: 予算は 3 プラットフォーム共通の要件値 2 秒であり、**どの 1 つでも
#   超えたら失敗**する。余裕が最も薄いのは macOS（research.md の公式実測で 1.564 秒、
#   要件値まで 0.436 秒しかない）であり、実質的に最初に鳴るのは macOS の段である。
#   特定の 1 プラットフォームだけを見るのではなく全計測値を同じ予算で判定し、余裕
#   （残りミリ秒）も全件出力する。「余裕が最も薄いものが収まれば他も収まる」は
#   この全件判定の系として成り立つ。
#
# 計測環境（tasks.md 10.3 の申し送り・ユーザー決定 2026-09-11）: 予算判定は CI の
#   GitHub-hosted ランナー（ubuntu / windows は 2 vCPU、macOS は 3 コア M1）で計測した
#   値に対して行う。このリポジトリは private であり、要件 1.3 の「SSD を搭載した
#   4 コア以上の一般的なデスクトップ環境」を満たすランナーは無い。ランナーは要件環境
#   より**弱い**保守的な代理であり、ここで通れば要件の環境でも通るとみなす。
#   **閾値は要件値の 2 秒のまま判定し、ランナーが弱いことを理由に緩めない**
#   （document-format の `scripts/check-bench-budget.sh` と同じ方針）。
#   ローカルでも同じコマンドで同じ判定を再現できる。
#
# POSIX sh 互換: `bash scripts/check-startup-budget.sh` が Linux / macOS /
#   Windows (Git Bash) のいずれでも動作すること（3 OS マトリクス共用）。
#
# 使い方: sh scripts/check-startup-budget.sh [計測ファイル] [起動予算 ms]
#   予算の引数は既定（要件値）を上書きする。**CI は引数なしで呼ぶ**
#   （予算判定の閾値を CI から差し替えない）。
# 終了コード: 0 = 全計測値が予算内 / 1 = 予算超過 / 2 = 計測値が無い・解釈できない
set -eu

BUDGET_MS="${2:-2000}"
MEASUREMENTS="${1:-target/startup-measurements.txt}"

echo "check-startup-budget: 予算 ${BUDGET_MS} ms（要件 1.3 / 6.8 の要件値は 2000 ms: 起動から操作可能なウィンドウの表示まで 2 秒以内） 計測ファイル: ${MEASUREMENTS}"

if [ ! -f "$MEASUREMENTS" ]; then
  echo "check-startup-budget: 計測値がありません: ${MEASUREMENTS}" >&2
  exit 2
fi

# 各チェックの終了コードを集約する。2（計測値が解釈できない）が最も重く、次に 1（予算超過）。
# すべての計測値を検査し、どれが問題かを 1 回の実行で報告する。
status=0
count=0

record_status() {
  rc="$1"
  case "$rc" in
    2) status=2 ;;
    1) [ "$status" -eq 2 ] || status=1 ;;
  esac
}

# 1 つの計測値を予算と比較する。超過なら 1（超過量を添えて報告）。値と余裕は常に出力する。
check_budget() {
  label="$1"
  measured_ms="$2"
  count=$((count + 1))

  if [ "$measured_ms" -le "$BUDGET_MS" ]; then
    headroom=$((BUDGET_MS - measured_ms))
    echo "check-startup-budget: OK  ${label} ${measured_ms} ms <= 予算 ${BUDGET_MS} ms（残り ${headroom} ms）"
    return 0
  fi

  over=$((measured_ms - BUDGET_MS))
  echo "check-startup-budget: VIOLATION  ${label} ${measured_ms} ms > 予算 ${BUDGET_MS} ms（超過 ${over} ms）" >&2
  return 1
}

# 1 行ずつ `<プラットフォーム>=<ミリ秒>` を読む。空行は無視する。構造が壊れた行は
# 解釈できない計測値として exit 2 で落とす（判定を飛ばして通る経路を作らない）。
lineno=0
while IFS= read -r line || [ -n "$line" ]; do
  lineno=$((lineno + 1))
  # CR（Windows の書き出しや autocrlf のチェックアウト）と前後の空白を落とす。
  line=$(printf '%s' "$line" | tr -d ' \t\r')
  [ -n "$line" ] || continue

  case "$line" in
    *=*) : ;;
    *)
      echo "check-startup-budget: 計測値を解釈できません（${lineno} 行目: 区切り '=' がありません）: ${line}" >&2
      exit 2
      ;;
  esac

  label=${line%%=*}
  measured_ms=${line#*=}

  case "$label" in
    ''|*[!a-z0-9_-]*)
      echo "check-startup-budget: 計測値を解釈できません（${lineno} 行目: プラットフォーム名が不正です）: ${line}" >&2
      exit 2
      ;;
  esac

  case "$measured_ms" in
    ''|*[!0-9]*)
      echo "check-startup-budget: 計測値を解釈できません（${lineno} 行目: ミリ秒が非負の整数ではありません）: ${line}" >&2
      exit 2
      ;;
  esac

  check_budget "$label" "$measured_ms" || record_status $?
done < "$MEASUREMENTS"

if [ "$count" -eq 0 ]; then
  echo "check-startup-budget: 計測値が 1 件もありません: ${MEASUREMENTS}" >&2
  exit 2
fi

if [ "$status" -eq 0 ]; then
  echo "check-startup-budget: ${count} 件の計測値がすべて予算内です（予算 ${BUDGET_MS} ms / 要件 1.3, 6.8）"
fi

exit "$status"

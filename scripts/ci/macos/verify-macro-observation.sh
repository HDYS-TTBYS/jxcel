#!/bin/bash
# macOS の段（**tasks.md 5.2** / 要件 1.2, 1.3, 1.5, 2.1, 2.3, 2.5, 5.1, 5.5, 6.1, 6.4, 8.2, 8.3,
# 9.1, 9.2, 9.3）。
#
# 実起動のマクロの観測を **macOS のランナーで**閉じる。判定の本体は POSIX sh の
# `scripts/check-macro-observation.sh`（**3 OS のランナーで同じものを走らせる**）。
# **この段は Linux の段 `scripts/ci/linux/verify-macro-observation.sh` の鏡であり、恒久である**
# — `data-grid` の 9.2 の段（`scripts/ci/macos/verify-grid-observation.sh`）と同じ形に乗っている
# （2 つ目の流儀を作らない）。
#
# # **この段は macOS では走らせていない（この開発機に macOS の実行環境が無い）**
#
# 実測したのは **Linux の段だけ**である。ここで確かめられるのは `bash -n` による構文の確認と、
# 兄弟の段（9.2）との対比だけである。**実行（実物の起動と観測）は CI の macOS ランナーでのみ
# 確かめる**（`verification.md`「ローカルで閉じられないもの」）。同じ理由で Windows の段
# （`scripts/ci/windows/verify-macro-observation.ps1`）も走らせていない。
#
# 段が足すのは OS 固有の 3 点だけである（9.2 の macOS の段と同じ規律）:
#
#   1. **記録の位置**（4.4 の解決。macOS は `$HOME/Library/Logs/{識別子}`。`logs` 接尾辞は付かない）
#   2. **実行ファイルの在り処**（検証用の形は `target/release/jxcel`。反証に渡す配布物は `.app` の
#      中の実行ファイル `target/release/bundle/macos/jxcel.app/Contents/MacOS/jxcel` —
#      **インストーラを使わずそのまま起動する**）
#   3. **標本の生成**（マクロ入りの文書を 5.1 の生成器から `target/observation/` へ書く）
#
# # 何を確かめるか（検査器の doc が正本）
#
#   1. 実行の成功と変更の件数（要件 2.1, 5.1, 5.5）
#   2. 失敗の理由とフレーム（要件 9.1–9.3）
#   3. 能力の拒否（要件 8.3, 9.2）
#   4. 打ち切りと、その後の操作可能性（要件 6.1, 6.4）
#   5. 保存と開き直しの往復（要件 1.2, 1.3, 1.5）
#
# **判定は要件値で行う**（検査器が持つ。この段は判定を足さない）。**引き金
# （`JXCEL_VERIFICATION_*`）は検査器が設定する** — この段は環境変数を足さない。
#
# # 反証（この段が空回りしていないことの錠前）
#
# **配布物（`.app` の中の実行ファイル）を渡すと検査器は非 0 で落ちる。** 配布物は検証専用の
# 環境変数を読まないので観測の行が 1 行も現れない。ただし**それだけでは錠前の実測にならない** —
# 起動していない実行も同じ理由で落ちるためである。したがってこの段は、(1) 落ちた理由が
# 「観測の行の不在」であることと、(2) **配布物が実際に起動してウィンドウを開いたこと**
# （検査器が出す `ウィンドウ:` の行）の両方を要求する。
#
# # アプリの出力を記録と混ぜないこと（9.2 の macOS の段が発見した罠）
#
# macOS では記録の保存先が `$HOME/Library/Logs` の下にあり、**同じファイルへ標準出力を向けると
# 2 人の書き手が同じ offset を触りうる**。検査器は自分でアプリの出力を自分の作業領域のファイルへ
# 向けるので、この段は記録と混ざらない（段の側でアプリの出力を記録へ流す経路を作らない）。
#
# # 前提（CI の macOS ランナー）
#
# `xvfb-run` のような仮想ディスプレイの包みは要らない（OS が画面を持つ）。検証用の形は先に
# `--features verification-triggers` でビルドされ、配布物は直前の「Build platform bundle」が
# 作っている。検査器は起動したアプリを**回収まで行って**終了するので、次の起動が単一インスタンスの
# 機構に引き継がれることはない（段の側でプロセスを止める必要は無い）。
#
# # 終了コード
#   0 = 適合 / 1 = 逸脱（検査器が非 0） / 2 = 入力が使えない（実行ファイル・配布物・生成器の失敗）
set -euo pipefail

verify=target/release/jxcel
if [ ! -x "$verify" ]; then
  echo "NG: 検証用の形の実行ファイルがありません: $verify（先に --features verification-triggers のビルドを走らせてください）" >&2
  exit 2
fi

# 配布物（`.app` の中の実行ファイル）。**インストーラを使わずそのまま起動する**（反証に渡す）。
# 無ければ 2 で落ちる — 反証はこの段の錠前であり、無いまま緑にしない
# （`verification.md`「入力が無いときに 0 を返さない」）。
shipping=target/release/bundle/macos/jxcel.app/Contents/MacOS/jxcel
if [ ! -x "$shipping" ]; then
  echo "NG: 配布物（.app の実行ファイル）がありません: $shipping（直前の「Build platform bundle」が先に必要）" >&2
  exit 2
fi

# 記録の置き場（4.4 の解決。macOS は $HOME/Library/Logs/{識別子}。`logs` 接尾辞は付かない）。
record="$HOME/Library/Logs/com.jxcel.app/jxcel.log"
mkdir -p "$(dirname "$record")"
echo "診断記録: $record"

# **標本はリポジトリの中（`target/`）へ書く**（リポジトリの外のファイルシステムはアプリと共有され
# ない環境がある。Linux の段が実測した）。`target/` はリポジトリの中で、かつ配布物に入らない。
mkdir -p target/observation
sample="target/observation/macro-observation-$$.jxcel"
trap 'rm -f "$sample"' EXIT

echo "標本を作る: シート「在庫」（3 行 × 2 列）＋マクロ 5 件（5.1 の生成器）"
cargo run --release -q -p macro-runtime --example make-macro-document \
  --features verification-samples -- "$sample"

# 検査器を 1 回走らせる。第 1 引数は実行ファイル、第 2 引数は標本、第 3 引数は記録である。
run_check() {
  _app=$1
  shift 1
  sh scripts/check-macro-observation.sh "$_app" "$sample" "$record" "$@"
}

echo "観測: 5 つの筋書き（成功・失敗・拒否・打ち切りと復帰・保存と開き直し）"
run_check "$verify" --timeout=180

# 反証: **配布物は検証専用の環境変数を読まない**ので、観測の行は現れず検査器は非 0 で落ちる。
echo "反証: 配布物（検証専用の引き金を読まない）で検査が非 0 で落ちること"
rc=0
output=$(run_check "$shipping" --timeout=60 2>&1) || rc=$?
printf '%s\n' "$output" | tail -n 20
if [ "$rc" -eq 0 ]; then
  echo "NG: 配布物に対して検査が成功してしまった（観測の行が無いのに通っている。既定のビルドに検証専用の経路が入っている）" >&2
  exit 1
fi
if [ "$rc" -eq 2 ]; then
  echo "NG: 反証が「入力が使えない」で落ちた（検査器が走っていない）" >&2
  exit 1
fi
# **落ちた理由が「観測の行の不在」であること**（起動しなかった実行も事実の欠落で落ちる）。
if ! printf '%s\n' "$output" | grep -q '観測の行'; then
  echo "NG: 反証が「観測の行が読めない」以外の理由で落ちた（検査器が動いていない可能性がある）" >&2
  exit 1
fi
# **配布物が実際に起動してウィンドウを開いたこと**（検査器が出す起動の形跡の行）。
if ! printf '%s\n' "$output" | grep -q 'ウィンドウ:'; then
  echo "NG: 反証で配布物のウィンドウが観測されていません（起動しなかった実行を錠前の実測と取り違えない）" >&2
  exit 1
fi
echo "反証: 期待どおり非 0 で落ちました（配布物は起動したが、観測の行が現れない）"

echo "OK: 3 OS の観測の段（macOS）— 実行の成功と変更の件数・失敗の理由とフレーム・能力の拒否・打ち切りとその後の操作・保存と開き直しの往復が成立した"

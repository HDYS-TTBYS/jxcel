#!/bin/bash
# macOS の段（tasks.md 10.5）。ウィンドウの観測は 1.5 / 10.4 と同じ CoreGraphics の列挙で、
# 2 つ目の起動の終了は `Process` で見る。**(c) の閉鎖要求はアクセシビリティ API でしか
# 注入できない**（macOS にウィンドウマネージャの閉鎖プロトコルは無い）。GitHub の macOS
# ランナーはその許可を与えない（actions/runner-images#8214）ため、**許可が無い場合は
# (c) を注入せず、その事実を出力に残す** — (c) の拒否と許可の実測は Linux と Windows の
# 段が担う。許可がある環境（開発機など）では同じ分岐が閉じるボタンの押下で (c) を実測する。

set -euo pipefail
# 4.4 の解決（macOS は $HOME/Library/Logs/{識別子}。`logs` 接尾辞は付かない）。
record="$HOME/Library/Logs/com.jxcel.app/jxcel.log"
mkdir -p "$(dirname "$record")"
echo "診断記録: $record"
dist="target/release/bundle/macos/jxcel.app/Contents/MacOS/jxcel"
verify="target/release/jxcel"
if [ ! -x "$dist" ]; then
  echo "NG: 配布物の実行ファイルがありません: $dist" >&2
  exit 1
fi
if [ ! -x "$verify" ]; then
  echo "NG: 検証用の形の実行ファイルがありません: $verify" >&2
  exit 1
fi
# 配布物に検証専用の識別子が入っていないこと（5.4 の片付けの規約）と、検証用の形には
# 入っていること。(c) が実測するのは検証用の形である。
#
# **`strings -a … | grep -q` を使わない。** `grep -q` は一致した時点で読むのをやめる
# （＝書き手のパイプを閉じる）ので、`strings` が EPIPE で非 0 終了し、`set -o pipefail`
# の下では**一致していても pipeline が失敗する**（実測: 2026-09-12 の macOS のランナー。
# Xcode の `strings` が `error: … strings: failed to flush output` を出し、
# 下の「DENY_CLOSE が無い」が**偽の失敗**になった）。**`grep -c` は最後まで読む**。
dist_markers=$(strings -a "$dist" | grep -c 'JXCEL_VERIFICATION' || true)
if [ "$dist_markers" -ne 0 ]; then
  echo "NG: 配布物の実行ファイルに検証専用の識別子が入っています（$dist_markers 件）" >&2
  exit 1
fi
verify_markers=$(strings -a "$verify" | grep -c 'JXCEL_VERIFICATION_DENY_CLOSE' || true)
if [ "$verify_markers" -eq 0 ]; then
  echo "NG: 検証用の形に JXCEL_VERIFICATION_DENY_CLOSE がありません（--features verification-triggers のビルドではない）" >&2
  exit 1
fi
echo "検証 (片付け): 配布物の実行ファイルに JXCEL_VERIFICATION は無く（strings -a で 0 件）、検証用の形は JXCEL_VERIFICATION_DENY_CLOSE を持つ"
document="$RUNNER_TEMP/jxcel-10-5-document.txt"
printf 'jxcel 10.5 の検証で開くドキュメント（内容は読まれない。tasks.md 7.7）\n' > "$document"
output_prefix="$RUNNER_TEMP/jxcel-10-5-app"
# **スパイクは同じディレクトリの `.swift` ファイルである**（以前はこの場で heredoc を書いていた）。
spike="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/verify-single-instance.swift"
swift "$spike" "$dist" "$verify" "$record" "$output_prefix" "$document" "jxcel" 60
# この段が起動したプロセスが残っていないことを段の出力で確かめる（残ると後続の段が
# 単一インスタンスの機構に引き継がれ、ウィンドウが出ない偽の失敗になる）。
if pgrep -x jxcel >/dev/null 2>&1; then
  echo "NG: 後始末の後に jxcel のプロセスが残っています" >&2
  pgrep -l -x jxcel >&2
  exit 1
fi
echo "検証 (後始末): jxcel のプロセスは残っていない（pgrep -x jxcel で 0 件）"

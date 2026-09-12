#!/bin/bash
# 起動の検証（macOS）。**追加のランナー設定は不要**だが、Apple Silicon では
# コード署名が必須であるため、`tauri.conf.json` の
# `bundle.macOS.signingIdentity` に疑似 identity `-`（ad-hoc 署名）を設定してある
# （Tauri 公式ドキュメント「macOS Code Signing → Ad-Hoc Signing」）。これが無いと
# tauri-bundler は署名自体を省略し（app.rs の分岐）、配布物が起動できない可能性がある。
# ランナー自体はウィンドウサーバを持つ VM で、GUI アプリの起動とウィンドウの観測が
# できる（GitHub-hosted macOS ランナー上での GUI アプリ起動の実例がある）。
# 観測は CoreGraphics の `CGWindowListCopyWindowInfo` で行う。画面収録の許可を
# 要する情報（ウィンドウ名）は使わず、所有者 pid・レイヤー・寸法だけで判定する。
#
# 同じスパイクが**起動からウィンドウ表示までの時間**も計測する（tasks.md 10.3 /
# 要件 1.3, 6.8）。計測区間の始点を起動の瞬間に取るため、**アプリの起動も
# スパイク自身が行う**（シェルが先に起動してから `swift` の起動を待つと、
# 待った分だけ計測値が小さく出る）。値は `target/startup-measurements.txt` に
# `macos=<ミリ秒>` として書き、CI の出力にも出す。

set -euo pipefail
APP="target/release/bundle/macos/jxcel.app/Contents/MacOS/jxcel"
# **スパイクは同じディレクトリの `.swift` ファイルである**（以前はこの場で heredoc を書いていた。
# 実体を別ファイルにすると、Swift の編集・レビューがそのファイルだけで完結する）。
SPIKE="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/verify-bundle-window.swift"
LOG="$RUNNER_TEMP/jxcel-macos.log"
MEASURE="target/startup-measurements.txt"
if [ ! -x "$APP" ]; then
  echo "NG: 配布物の実行ファイルがありません: $APP" >&2
  exit 1
fi
rm -f "$MEASURE"
# 冷えたランナーの**初回起動だけ**が持つ一度きりのコスト（WebKit / AppKit の
# フレームワーク読み込み、フォントや各種キャッシュの初回構築、OS の初回 GUI 起動）を
# 計測区間へ混ぜない。要件 1.3 の根拠である Tauri の公式ベンチは**ウォーム実行**
# （3 回のウォームアップを捨てる。research.md「起動時間」）であり、ここでも 1 回
# 起動して捨ててから計測する。冷えた値も情報として出力する（判定には使わない）。
WARMUP="$RUNNER_TEMP/jxcel-macos-warmup-measure.txt"
rm -f "$WARMUP"
swift "$SPIKE" "$APP" "$LOG" "$WARMUP" >/dev/null 2>&1 || true
echo "参考: ウォームアップ起動（計測には使わない）: $(cat "$WARMUP" 2>/dev/null || echo '(計測なし)')"

status=0
swift "$SPIKE" "$APP" "$LOG" "$MEASURE" || status=$?
if [ "$status" -ne 0 ]; then
  echo "--- アプリの出力 ---" >&2
  cat "$LOG" >&2 || true
fi
exit "$status"

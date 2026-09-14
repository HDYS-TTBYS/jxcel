#!/bin/bash
# macOS の段（**tasks.md 1.6**）。走査の使い捨ての画面（`smoke-glide-probe`）が macOS で
# **実際に描画され、10 万行分の縦の広がりを持つ**ことを起動して確かめる。観測は 10.4 の段と
# 同じ経路を使う（CoreGraphics の列挙と診断記録。**この段のために新しい系統を作らない** —
# 1.6「既存の 3 OS の検証マトリクスに一時的な段を足し、新しい系統を作らない」）。
#
# # 数値をここでは出さない（1.6 の明文）
#
# 1.6 は「**Linux の結果を判断の拘束条件とする。**…Windows と macOS の数値は **9.2 の観測で
# 確かめる**」と定める。走査中のフレーム時間の中央値は、使い捨ての画面が
# **アクセシビリティの木**（AT-SPI）へ出す 1 行から読むが、**macOS には AT-SPI が無い**
# （AT-SPI は D-Bus 上の仕組みであり、macOS の WKWebView は別のアクセシビリティ API を使う）。
# したがって**この段は macOS で数値を出さない**。ここで確かめるのは次の 2 つである:
#
#   1. 走査の画面が WKWebView で**実際に描画される**こと（`画面=smoke-glide-probe` の成立行）。
#      これは Linux の DMA-BUF とは別の失敗の型（描画不成立）を macOS で塞ぐ。
#   2. 10 万行の走査が macOS で**起動できる**こと（画面が現れ、初回描画が成立する）。
#
# **数値の欠落を「成立」と読み替えない** — この段が緑でも、要件 11.1 の macOS の証拠には
# ならない。恒久の観測は 9.2 が担う（同じく `verification.md`「無いことを確かめる検査には
# 負の対照を付ける」の精神で、この段は**何を証明しないかを冒頭に書く**）。
#
# # 一時的な段である
#
# 9.2 が 3 OS の観測を恒久の段として載せた時点で、**この段は取り除く**。

set -euo pipefail
# 4.4 の解決（macOS は $HOME/Library/Logs/{識別子}。`logs` 接尾辞は付かない）。
record="$HOME/Library/Logs/com.jxcel.app/jxcel.log"
# **アプリの標準出力・標準誤差は診断記録とは別のファイルへ。**同じファイルへ書くと、アプリ
# （`TargetKind::Stdout`）と診断記録（`TargetKind::Folder`）が独立した 2 つの書き手として
# 同じファイルを offset 0 と EOF から触り、アプリ側が先頭から上書きして記録の行を壊しうる
# （偽の失敗。10.4 の段の冒頭に同じ注記がある）。
appout="$RUNNER_TEMP/jxcel-render-traversal-app.log"
mkdir -p "$(dirname "$record")"
echo "診断記録: $record"
echo "アプリの出力: ${appout}（記録とは別ファイル）"

# ウィンドウの観測と診断記録の読みは 10.4 の段のスパイクをそのまま使う（**同じファイルを
# 共有する** — 観測の仕方を 2 つに割らない）。
spike="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/../macos/verify-first-paint.swift"
if [ ! -f "$spike" ]; then
  echo "NG: 10.4 の段のスパイクが見つかりません: $spike" >&2
  exit 2
fi

echo "検証: 走査の使い捨ての画面 smoke-glide-probe が macOS で描画されること（数値は 9.2 が担う）"
swift "$spike" target/release/jxcel smoke-glide-probe "$record" "$appout" 60
echo "注記: この段は走査の画面が macOS で描画されることだけを確かめました。"
echo "注記: 走査中のフレーム時間の中央値の macOS の数値は、1.6 の明文により 9.2 の観測で確かめます。"

#!/bin/bash
# macOS の描画検証（tasks.md 10.4）。3 回起動する（配布物・smoke-table・smoke-editor）。
# ウィンドウの観測は 1.5 のスパイクと同じ CoreGraphics の列挙（所有者 pid・レイヤー・寸法のみ。
# 画面収録の許可を要するウィンドウ名は使わない）。描画と**描画された画面**は診断記録を読む。
#
# **アプリの標準出力・標準誤差は診断記録とは別のファイルへ書く。**同じファイルへ書くと、
# アプリ（`TargetKind::Stdout`）と診断記録（5.2 の `TargetKind::Folder`）が**独立した 2 つの
# 書き手**として同じファイルを offset 0 と EOF から触り、アプリ側が先頭から上書きして
# `初回描画が成立した` の行を壊しうる（偽の失敗）。Linux / Windows の段は最初から別ファイル
# である（`>"$log"` / `Start-Process` の既定）。

set -euo pipefail
# 4.4 の解決（macOS は $HOME/Library/Logs/{識別子}。`logs` 接尾辞は付かない）。
record="$HOME/Library/Logs/com.jxcel.app/jxcel.log"
appout="$RUNNER_TEMP/jxcel-app-output.log"
mkdir -p "$(dirname "$record")"
echo "診断記録: $record"
echo "アプリの出力: ${appout}（記録とは別ファイル）"
# **スパイクは同じディレクトリの `.swift` ファイルである**（以前はこの場で heredoc を書いていた）。
spike="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/verify-first-paint.swift"
# 3 回とも**期待する「描画された画面」**を渡す（配布物は要求を無視するが、初期画面は
# 9.6 の空ウィンドウの画面なので `画面=empty-window` が成立する）。
echo "検証 1/3: 配布物（.app）をそのまま起動する。追加のインストール手順は無い（インストーラを使わない）"
swift "$spike" "target/release/bundle/macos/jxcel.app/Contents/MacOS/jxcel" "empty-window" "$record" "$appout" 60
echo "検証 2/3: 表形式の最小画面 smoke-table（検証用の形。スモーク画面のコードとシェルの構造は配布物と共通）"
swift "$spike" target/release/jxcel smoke-table "$record" "$appout" 60
echo "検証 3/3: 文字編集の最小画面 smoke-editor（検証用の形。スモーク画面のコードとシェルの構造は配布物と共通）"
swift "$spike" target/release/jxcel smoke-editor "$record" "$appout" 60

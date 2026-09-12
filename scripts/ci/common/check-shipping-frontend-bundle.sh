#!/bin/bash
# 配布物のフロントエンド資産の衛生検査（tasks.md 5.4 / 8.2 の決定、要件 4.7 の精神）。
#
# **なぜ `dist/` を検査するのか（重要）**: Tauri は `compression` feature（既定で有効）で
# フロントエンド資産を **brotli 圧縮して**実行ファイルへ埋め込むため、実行ファイルに対する
# `strings -a` では**埋め込み資産の本文が見えない**（実行ファイルに残るのは内容ハッシュ付きの
# 資産のファイル名だけである）。直前の `npx tauri build --bundles` が `beforeBuildCommand`
# （`npm run build`）で書いた `dist/` は、その実行ファイルへ埋め込まれる入力そのものである。
# したがってここで `dist/` を検査する。**実行ファイルに対する `strings` の検査（10.5 の段）は
# Rust 側のためにそのまま残す。**
#
# 第 2 引数（`target/release/jxcel[.exe]`。Windows は接尾辞を補う）を渡すと、
# **資産のファイル名の一致**で「その実行ファイルが検査した dist を埋め込んでいる」ことも
# 確かめる（ファイル名は Vite が内容から決めるので、一致は埋め込み内容の同一性の根拠になる）。
#
# **10.4 の段より前に置く** — 10.4 は `npm run build` をやり直して `dist/` を検証用の形で
# 上書きするためである。ここで配布物の `dist/` を `target/shipping-frontend-dist` へ退避し、
# 10.4 の形の突き合わせ（`scripts/check-bundle-forms.sh`）が使う。

set -eu
bash scripts/check-shipping-bundle.sh dist target/release/jxcel
rm -rf target/shipping-frontend-dist
cp -r dist target/shipping-frontend-dist

#!/bin/bash
# 起動の検証（tasks.md 1.5 の完了状態）。プロセスの起動ではなく、**ウィンドウが
# 現れること**を確認する。GUI を持たないランナーでは仮想ディスプレイ（Xvfb）上で
# 起動する（design.md「BuildPipeline」の前提）。
# 検査本体は POSIX sh の `scripts/check-x11-window.sh`（ローカルでも同じものを実行できる）。
# 対象は生成した AppImage（`target/release/bundle/appimage/*.AppImage`）。
# FUSE が使えないランナーでは `APPIMAGE_EXTRACT_AND_RUN=1`（展開実行）へ退避する。
#
# 同じ段が**起動からウィンドウ表示までの時間**も計測する（tasks.md 10.3 /
# 要件 1.3, 6.8）。検査器は計測値を `target/startup-measurements.txt` に
# `linux=<ミリ秒>` として書き、CI の出力にも出す。仮想ディスプレイや xvfb-run の
# 起動は計測区間に含めない（要件が測るのは配布物の起動時間であり、検証基盤の
# 起動時間ではない）。再試行がある場合、**権威があるのは成功した試行の値**である
# （失敗した試行はファイルに触れない）。

# **Actions の既定の bash 段は `bash -e` で走る**（この段は `shell:` を指定していない）。
# 抽出前は台本側が与えていたので、同じ厳しさをここで持つ（`-u` と pipefail は元の段にも無いので足さない）。
set -e

set -- target/release/bundle/appimage/*.AppImage
appimage=$1
measure=target/startup-measurements.txt
warmup=target/startup-warmup.txt
rm -f "$measure" "$warmup"

# 冷えたランナーの**初回起動だけ**が持つ一度きりのコスト（WebKit の共有ライブラリの
# 読み込み・フォントキャッシュ・X / GUI の初回起動）を計測区間へ混ぜない。要件 1.3 の
# 根拠である Tauri の公式ベンチは**ウォーム実行**（3 回のウォームアップを捨てる。
# research.md「起動時間」）であり、ここでも 1 回起動して捨ててから計測する。冷えた値も
# 情報として出力する（判定には使わない）。**閾値は要件値の 2 秒のままである。**
run_check() {
  xvfb-run -a --server-args="-screen 0 1280x1024x24" \
    sh scripts/check-x11-window.sh "$appimage" jxcel 60 100 100 "$1"
}
if ! run_check "$warmup"; then
  echo "FUSE 経由で起動できなかったため、APPIMAGE_EXTRACT_AND_RUN=1 で再試行します" >&2
  export APPIMAGE_EXTRACT_AND_RUN=1
  run_check "$warmup"
fi
echo "参考: ウォームアップ起動（計測には使わない）: $(cat "$warmup" 2>/dev/null || echo '(計測なし)')"

if ! run_check "$measure"; then
  echo "FUSE 経由で起動できなかったため、APPIMAGE_EXTRACT_AND_RUN=1 で再試行します" >&2
  export APPIMAGE_EXTRACT_AND_RUN=1
  run_check "$measure"
fi

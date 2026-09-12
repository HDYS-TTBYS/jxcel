#!/bin/bash
set -eu
logs="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs"
record="$logs/jxcel.log"
crash="$logs/jxcel-crash.log"
echo "診断記録: $record"
set -- target/release/bundle/appimage/*.AppImage
appimage=$1
# 展開先を変えた後（`cd` の後）でも同じファイルを指せるよう絶対パスにする。
appimage=$(CDPATH='' cd -- "$(dirname -- "$appimage")" && pwd)/$(basename -- "$appimage")
echo "反証 1/2: 配布物を panic:<ms> で起動する（引き金は無視されなければならない）"
if ! xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-crash-record.sh \
    "$appimage" jxcel "$record" "$crash" ignored 60 100 100; then
  echo "FUSE 経由で配布物を起動できなかったため、APPIMAGE_EXTRACT_AND_RUN=1 で再試行します" >&2
  export APPIMAGE_EXTRACT_AND_RUN=1
  xvfb-run -a --server-args="-screen 0 1280x1024x24" \
    sh scripts/check-crash-record.sh \
      "$appimage" jxcel "$record" "$crash" ignored 60 100 100
fi
echo "反証 2/2: 配布物を JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT=1 で起動する（抑止されない）"
if ! xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-no-paint.sh "$appimage" jxcel "$record" ignored 60 100 100; then
  echo "FUSE 経由で配布物を起動できなかったため、APPIMAGE_EXTRACT_AND_RUN=1 で再試行します" >&2
  export APPIMAGE_EXTRACT_AND_RUN=1
  xvfb-run -a --server-args="-screen 0 1280x1024x24" \
    sh scripts/check-no-paint.sh "$appimage" jxcel "$record" ignored 60 100 100
fi

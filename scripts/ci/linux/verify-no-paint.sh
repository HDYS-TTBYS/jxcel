#!/bin/bash
# 10.2（描画不成立の提示）。抑止した検証用の形で期限超過の経路を起こし、記録・題名・
# **全画面の注意書き（画素）**・8.3 の印・通常終了を要求する。
# **この段は 8.3 の印（`render.fallback`）を立てる**ので、描画を測る段（10.4）より後に
# 置く（印は次の起動の描画経路を変える）。

set -eu
logs="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs"
record="$logs/jxcel.log"
echo "診断記録: $record"
echo "検証: 検証用の形を JXCEL_VERIFICATION_SUPPRESS_HEARTBEAT=1 で起動し、期限超過の提示を確かめる"
xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-no-paint.sh target/release/jxcel jxcel "$record" suppressed 60 100 100

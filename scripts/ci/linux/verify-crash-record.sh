#!/bin/bash
# 8.2（異常終了の記録）。検証用の形で意図的なパニックを起こし、記録の契約・記録が
# パニックを握り潰していないこと・そのあとの起動が壊れていないことを要求する。
# **配布物を同じ環境変数で起動する負の対照は、この節の最後の段**（配布物の対照）が行う。

set -eu
logs="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs"
record="$logs/jxcel.log"
crash="$logs/jxcel-crash.log"
echo "診断記録: $record"
echo "異常終了の記録: $crash"
echo "検証: 検証用の形を panic:<ms> で起動し、異常終了の記録とそのあとの起動を確かめる"
xvfb-run -a --server-args="-screen 0 1280x1024x24" \
  sh scripts/check-crash-record.sh \
    target/release/jxcel jxcel "$record" "$crash" panic 60 100 100

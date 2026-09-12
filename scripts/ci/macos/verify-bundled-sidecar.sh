#!/bin/bash
set -eu
for dmg in target/release/bundle/dmg/*.dmg; do
  echo "配布物 (dmg) のサイズ: ${dmg}: $(wc -c < "$dmg") バイト"
done
bash scripts/check-sidecar-integrity.sh \
  target/release/bundle/macos/jxcel.app \
  Contents/MacOS/sidecar-smoke

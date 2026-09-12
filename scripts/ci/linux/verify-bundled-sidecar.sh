#!/bin/bash
set -eu
# グロブが一致しなければリテラルのまま残り、検査が入力の不在として 2 で落ちる。
set -- target/release/bundle/appimage/*.AppImage
appimage=$1
set -- target/release/bundle/deb/*.deb
deb=$1
bash scripts/check-sidecar-integrity.sh "$appimage" usr/share/jxcel/sidecar-smoke
bash scripts/check-sidecar-integrity.sh "$deb" usr/share/jxcel/sidecar-smoke

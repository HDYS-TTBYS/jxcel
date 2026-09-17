#!/bin/bash
# Linux の WebKitGTK 系開発パッケージと仮想ディスプレイ。
# Tauri v2 は OS の WebView にリンクするため、ヘッダと .pc ファイルが無いと
# `webkit2gtk-sys` / `gtk-sys` / `soup3-sys` / `javascriptcore-rs-sys` の
# ビルドスクリプトが pkg-config の解決に失敗し、ワークスペース全体が落ちる。
#   - libwebkit2gtk-4.1-dev        : WebView 本体（Tauri v2 が使う 4.1 系）
#   - libgtk-3-dev                 : ウィンドウと GTK ウィジェット
#   - libsoup-3.0-dev              : WebKitGTK 4.1 系が要求する HTTP 実装
#   - libjavascriptcoregtk-4.1-dev : JS エンジン
#   - librsvg2-dev                 : アイコン（SVG）の読み込み
#   - libssl-dev                   : OpenSSL ヘッダ（openssl-sys の解決）
#   - libdbus-1-dev                : tauri の既定 feature `dbus` →
#     tauri-runtime-wry → tao/dbus → dbus → libdbus-sys が `dbus-1.pc` を
#     pkg-config に要求する。欠けると libdbus-sys のビルドスクリプトが panic し、
#     ワークスペース全体が落ちる（ローカルで実測）。Tauri のドキュメントの
#     パッケージ一覧には現れないが、既定 feature のビルドには必須である。
#   - pkg-config                   : 上記の解決機構そのもの
#   - patchelf                     : AppImage バンドル時の実行ファイル書き換え（タスク 1.5）
#   - xvfb                         : 仮想ディスプレイ。GUI の無いランナーで
#     配布物を起動しウィンドウの表示を確認するために使う（タスク 1.5。design.md
#     「BuildPipeline」が省略できない前提としている）
#   - x11-utils                    : `xwininfo`。ウィンドウツリーの走査に使う
#   - libfuse2                     : Tauri が生成する AppImage のランタイムが FUSE 2 を
#     dlopen する版に対応する。FUSE が使えない環境では
#     `APPIMAGE_EXTRACT_AND_RUN=1` で展開実行へ切り替えられる
# macOS / Windows は OS 標準の WebView を使うため追加パッケージを要さない。

# **Actions の既定の bash 段は `bash -e` で走る**（この段は `shell:` を指定していない）。
# 抽出前は台本側が与えていたので、同じ厳しさをここで持つ（`-u` は元の段にも無いので足さない）。
set -e

sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev \
  librsvg2-dev \
  libssl-dev \
  libdbus-1-dev \
  pkg-config \
  patchelf \
  xvfb \
  x11-utils \
  libfuse2 \
  at-spi2-core

# **`at-spi2-core` を明示する理由**: 10.6 のメニュー検査は AT-SPI で
# アクセシビリティの木を読む。そのためには `org.a11y.Bus`（`at-spi-bus-launcher`）
# が要るが、**それは GTK の推奨依存としては入らない**（実測: 素の
# `libgtk-3-dev` などの導入では `at-spi-bus-launcher` が無く、起動時に
# `org.a11y.Bus was not provided by any .service files` が出る）。

# **ランナーの xdg-desktop-portal を D-Bus の活性化対象から外す。**
# これはポータルを使わないという意味ではなく、**壊れたポータルが起動を 25 秒
# 止めること**を避けるための処置である。実測（2026-09-12、ubuntu-22.04 ランナー）:
# アプリの `builder.build()` の中で `StartServiceByName
# org.freedesktop.portal.Desktop` が送られ、GLib / GDBus の既定のタイムアウト
# 25 秒を待つ `poll(..., 25000)` がタイムアウトしてから起動が続く。段ごとの
# 計測で `builder.build()` が 25,077 ms、`DBUS_SESSION_BUS_ADDRESS` を
# 無効化すると 48 ms、**この .service を外すと 42 ms** だった（strace で確認）。
# ポータルは本アプリの動作に必要ない（要件 1.2 は追加のランタイムを要求しない。
# `scripts/` にポータルを要求する検査は無い）。セッションバス自体は残すので、
# AT-SPI を使う段（10.6 のメニュー検査）には影響しない。
# xdg-desktop-portal が入っていないランナーでは `rm -f` は空振りする。
sudo rm -f /usr/share/dbus-1/services/org.freedesktop.portal.Desktop.service

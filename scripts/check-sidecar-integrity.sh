#!/bin/sh
# 配布物に同梱された補助プロセスの実行ファイルを検証する（tasks.md 10.2、要件 6.4, 6.5, 6.6）。
#
# **これはバンドル処理が同梱した実行ファイルを書き換える既知の問題に対する恒久的な検出器である。**
# Linux の標準の同梱機構（`externalBin`）が `usr/bin` 配下の ELF を無条件に書き換える
# （tauri-apps/tauri#5189）ため、1.7 は走査対象外の `usr/share/jxcel/` への回避配置を決めた。
# その配置が破れた日に、静かにではなく明確に落ちることがこの検査器の役割である。
#
# 使い方:
#   check-sidecar-integrity.sh <配布物> <配布物内のパス> [<同梱前の原本>] [<起動タイムアウト秒>]
#
#   <配布物>           : 検証対象。次の 3 種のみを受け付ける。
#                        - `*.AppImage` : バンドル済みの実行ファイルを起動せずに取り出す
#                        - `*.deb`      : `dpkg-deb -x` で展開する（Linux のランナーのみ）
#                        - ディレクトリ : 既に展開された配布物根。macOS の `jxcel.app` と、
#                                          Windows の NSIS 導入先（`%LOCALAPPDATA%\jxcel`）が
#                                          これに当たる
#   <配布物内のパス>   : 配布物根から補助プロセスの実行ファイルへの相対パス。この basename から
#                        語幹（`sidecar-smoke`）を取り出す。
#                        例: `usr/share/jxcel/sidecar-smoke`（Linux の AppImage / deb）、
#                            `Contents/MacOS/sidecar-smoke`（macOS）、
#                            `sidecar-smoke.exe`（Windows の導入先）
#   <同梱前の原本>     : 既定は `<リポジトリ根>/sidecars/<語幹>-<ホストのターゲットトリプル>[.exe]`。
#                        これは `scripts/stage-sidecars.sh`（唯一の文書化された配置手順）が
#                        配置する名前であり、`crates/app-shell/build.rs` が 3.1 の期待ダイジェストを
#                        算出する相手でもある（実行時の防御と CI のカナリアが同じ値を共有する）。
#                        明示した場合は**名前の接尾辞がホストのターゲットトリプルと一致することを
#                        検査する**。`cargo build --release --target <T>` の成果物はネイティブ
#                        ビルドとバイトが異なるため、CI はネイティブで配置する（1.7 の決定）。
#   <起動タイムアウト秒>: 既定 10（起動行が現れるまでの待ち時間）。
#
# 何を検証するか:
#   1. 配布物から補助プロセスの実行ファイルを取り出す。
#   2. 同梱前の原本と**バイト単位で一致する**こと。判定は `cmp` による直接のバイト比較で行い、
#      両者の SHA-256 を併せて表示する。`cmp` を判定に使う理由は、ダイジェスト算出器の有無や
#      その出力形式に依存しない最も直接的な比較だからである。SHA-256 を表示する理由は、
#      CI のログに値が残って食い違いを診断でき、3.1 が実行時に照合する期待値と突き合わせられる
#      からである。**不一致の失敗メッセージには両方のパスと両方のダイジェストを含める。**
#   3. 取り出した実行ファイルが**起動できる**こと。補助プロセス（`crates/sidecar-smoke`、1.6）の
#      `--idle` は起動時に `sidecar-smoke ready pid=<識別子> mode=idle` の 1 行を出して待機する
#      （標準入力も親監視も持たない起動専用のモード）。この行が期限内に現れることをもって起動と
#      見なす。`--parent-pid` を使わないのは、POSIX sh から見た子の識別子が Windows では
#      Windows のプロセス識別子と一致せず、渡した識別子の解釈が環境依存になるためである。
#      起動したプロセスは終了時に必ず終了させる（Unix は `kill`、Windows は起動行が報告した
#      実プロセス識別子への `taskkill`）。
#   4. 配布物と取り出した実行ファイルのサイズを出力に記録する（要件 6.6）。
#
# 終了コード:
#   0 = 検証成功（バイト一致・起動・サイズの記録がすべて済んだ）
#   1 = 検証失敗（内容の不一致、配布物内に実行ファイルが無い、実行権限が無い、起動しない）
#   2 = 入力が使えない（配布物・原本・ホストのターゲットトリプルが解決できない、
#       ダイジェストを計算できない、配布物内のパスが不正、`dpkg-deb` が無い）
#
# 一時展開先は EXIT/INT/TERM で必ず消す。**配布物や原本が無いときに黙って通さない。**
set -eu

usage() {
  echo "使い方: $0 <配布物> <配布物内のパス> [<同梱前の原本>] [<起動タイムアウト秒>]" >&2
  echo "  <配布物> は *.AppImage / *.deb / 展開済みの配布物根（ディレクトリ）のいずれか" >&2
  exit 2
}

[ "$#" -ge 2 ] || usage

artifact=$1
inner=$2
original_arg=${3:-}
timeout_secs=${4:-10}

# --- 引数の構造検証（入力が使えない場合は 2） --------------------------------
case $timeout_secs in
  '' | *[!0-9]*)
    echo "NG: 起動タイムアウト秒が正の整数ではありません: $timeout_secs" >&2
    exit 2
    ;;
esac
if [ "$timeout_secs" -lt 1 ]; then
  echo "NG: 起動タイムアウト秒が正の整数ではありません: $timeout_secs" >&2
  exit 2
fi

case $inner in
  '')
    echo "NG: 配布物内のパスが空です" >&2
    exit 2
    ;;
  /*)
    echo "NG: 配布物内のパスは配布物根からの相対パスでなければなりません: $inner" >&2
    exit 2
    ;;
esac
case "/$inner/" in
  */../*)
    echo "NG: 配布物内のパスに '..' を含めることはできません: $inner" >&2
    exit 2
    ;;
esac

if [ ! -e "$artifact" ]; then
  echo "NG: 配布物がありません: $artifact" >&2
  echo "  先に配布物を生成してください（npx tauri build --bundles <OS 別の形式>）。" >&2
  exit 2
fi

if [ -d "$artifact" ]; then
  kind=dir
elif [ -f "$artifact" ]; then
  case $artifact in
    *.AppImage) kind=appimage ;;
    *.deb) kind=deb ;;
    *)
      echo "NG: 配布物の種別を判別できません（*.AppImage / *.deb / 展開済みのディレクトリのいずれかを渡してください）: $artifact" >&2
      exit 2
      ;;
  esac
else
  echo "NG: 配布物が通常のファイルでもディレクトリでもありません: $artifact" >&2
  exit 2
fi

# --- ホストのターゲットトリプルと、期待する原本の名前 ------------------------
# stage-sidecars.sh と同じ規則（ホストのトリプル + Windows のみ `.exe`）で原本を導く。
# 明示された原本の接尾辞が違えば、1.7 が禁じたクロス配置である。
rustc_vv=$(rustc -vV 2>/dev/null || true)
host_triple=$(printf '%s\n' "$rustc_vv" | sed -n 's/^host: //p')
if [ -z "$host_triple" ]; then
  echo "NG: ホストのターゲットトリプルを rustc から取得できません（rustc を PATH に用意してください）" >&2
  exit 2
fi

stem=${inner##*/}
case $stem in
  *.exe) stem=${stem%.exe} ;;
esac
case $host_triple in
  *windows*) exe_suffix=.exe ;;
  *) exe_suffix='' ;;
esac
expected_name="${stem}-${host_triple}${exe_suffix}"

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH='' cd -- "$script_dir/.." && pwd)
expected_original="sidecars/${expected_name}"

if [ -n "$original_arg" ]; then
  original=$original_arg
  if [ "${original##*/}" != "$expected_name" ]; then
    echo "NG: 同梱前の原本の名前がホストのターゲットトリプルと一致しません" >&2
    echo "  指定: $original" >&2
    echo "  期待: $expected_original（ホスト: ${host_triple}）" >&2
    echo "  ネイティブの成果物はクロス指定の成果物とバイトが異なる（1.7）。配置し直すこと:" >&2
    echo "    bash scripts/stage-sidecars.sh" >&2
    exit 2
  fi
else
  original="$repo_root/$expected_original"
fi

if [ ! -f "$original" ]; then
  echo "NG: 同梱前の原本がありません: $original" >&2
  echo "  期待する名前: $expected_name（ホスト: ${host_triple}）" >&2
  echo "  配置するには: bash scripts/stage-sidecars.sh" >&2
  exit 2
fi

# --- ダイジェストの算出手段 --------------------------------------------------
# SHA-256 を小文字 hex で返す。3 OS のランナーのいずれにも次のどれかがある
# （Linux / Git Bash は `sha256sum`、macOS は `shasum`、いずれも無ければ `openssl`）。
# どれも無い場合と、あっても実行できない場合は、呼び出し側が 2 で落とす（沈黙して通さない）。
digest_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$1" | sed 's/.*= //'
  else
    return 1
  fi
}

# ファイルは実バイト数、ディレクトリは割り当て済みのサイズ（1K ブロック）をバイトへ換算する。
size_of() {
  if [ -d "$1" ]; then
    du -sk "$1" | awk '{ print $1 * 1024 }'
  else
    wc -c < "$1" | tr -d ' '
  fi
}

# --- 後始末 ------------------------------------------------------------------
# 一時展開先と、起動した補助プロセスを残さない。EXIT だけでなく INT/TERM でも走らせる。
work=''
log=''
sh_child_pid=''
reported_pid=''

# shellcheck disable=SC2329 # 下の trap から呼ばれる（shellcheck は trap を追えない）
cleanup() {
  if [ -n "$reported_pid" ] && command -v taskkill >/dev/null 2>&1; then
    # Windows（Git Bash）: 起動行が報告した Windows の識別子で終了させる。
    taskkill //F //PID "$reported_pid" >/dev/null 2>&1 || true
  fi
  if [ -n "$sh_child_pid" ]; then
    kill "$sh_child_pid" 2>/dev/null || true
    kill -9 "$sh_child_pid" 2>/dev/null || true
  fi
  reported_pid=''
  sh_child_pid=''
  if [ -n "$log" ] && [ -f "$log" ]; then
    rm -f "$log" || true
  fi
  log=''
  if [ -n "$work" ] && [ -d "$work" ]; then
    rm -rf "$work" || true
  fi
  work=''
}
trap cleanup EXIT INT TERM

# --- 配布物からの取り出し ----------------------------------------------------
case $kind in
  appimage)
    if [ ! -x "$artifact" ]; then
      echo "NG: 配布物に実行権限がありません（--appimage-extract で展開するために必要です）: $artifact" >&2
      echo "  GitHub の成果物は実行権限を保持しない（1.5）。ダウンロードした AppImage を使う場合は chmod +x すること。" >&2
      exit 2
    fi
    # `--appimage-extract` はカレントディレクトリへ `squashfs-root/` を作るので、一時ディレクトリへ
    # 移ってから実行する（リポジトリの木を汚さない）。移る前に絶対パスへ直しておく。
    artifact_dir=$(CDPATH='' cd -- "$(dirname -- "$artifact")" && pwd)
    artifact_abs="$artifact_dir/${artifact##*/}"
    work=$(mktemp -d)
    if ! (cd "$work" && "$artifact_abs" --appimage-extract "$inner" >/dev/null 2>&1); then
      echo "NG: 配布物の展開に失敗しました: $artifact" >&2
      echo "  \`--appimage-extract ${inner}\` が非 0 で終了しました。" >&2
      exit 1
    fi
    sidecar="$work/squashfs-root/$inner"
    extraction="AppImage のランタイムによる --appimage-extract（FUSE を要さず、アプリを起動せずに取り出す）"
    ;;
  deb)
    if ! command -v dpkg-deb >/dev/null 2>&1; then
      echo "NG: dpkg-deb が見つかりません（.deb の展開に必要です）" >&2
      exit 2
    fi
    work=$(mktemp -d)
    if ! dpkg-deb -x "$artifact" "$work" >/dev/null 2>&1; then
      echo "NG: 配布物の展開に失敗しました: $artifact" >&2
      echo "  \`dpkg-deb -x\` が非 0 で終了しました。" >&2
      exit 1
    fi
    sidecar="$work/$inner"
    extraction="dpkg-deb -x"
    ;;
  dir)
    sidecar="$artifact/$inner"
    extraction="展開済みの配布物根をそのまま使う"
    ;;
esac

if [ ! -f "$sidecar" ]; then
  echo "NG: 配布物の中に補助プロセスの実行ファイルがありません" >&2
  echo "  配布物: $artifact" >&2
  echo "  配布物内のパス: $inner" >&2
  exit 1
fi

if [ ! -x "$sidecar" ]; then
  echo "NG: 配布物内の補助プロセスに実行権限がありません（配布物が権限を失っています）: $sidecar" >&2
  echo "  1.7 の完了状態は「実行権限を保った実行ファイルが存在する」である。" >&2
  exit 1
fi

# --- サイズの記録（要件 6.6） ------------------------------------------------
echo "配布物: $artifact"
echo "配布物のサイズ: $(size_of "$artifact") バイト"
echo "取り出し方法: $extraction"
echo "配布物内の補助プロセス: $inner"
echo "取り出した補助プロセスのサイズ: $(size_of "$sidecar") バイト"
echo "同梱前の原本: $original"
echo "同梱前の原本のサイズ: $(size_of "$original") バイト"
echo "ホストのターゲットトリプル: $host_triple"

# --- 同梱前とのバイト一致（要件 6.5） ----------------------------------------
# ダイジェストは失敗の診断に必須である（両方のパスと両方の値を出す）。算出できない場合は
# 比較に入る前に 2 で落とす。空の出力も失敗として扱う（`cut` や `sed` は前段が失敗しても
# 0 を返しうるため）。
missing_digest_tool='SHA-256 を計算するツールがありません（sha256sum / shasum / openssl のいずれかが必要です）'
if ! expected_digest=$(digest_of "$original") || [ -z "$expected_digest" ]; then
  echo "NG: 同梱前の原本の SHA-256 を計算できません: $original" >&2
  echo "  ${missing_digest_tool}" >&2
  exit 2
fi
if ! actual_digest=$(digest_of "$sidecar") || [ -z "$actual_digest" ]; then
  echo "NG: 配布物内の補助プロセスの SHA-256 を計算できません: $sidecar" >&2
  echo "  ${missing_digest_tool}" >&2
  exit 2
fi

if ! cmp -s "$sidecar" "$original"; then
  echo "NG: 配布物内の補助プロセスが同梱前の原本と一致しません（バンドル処理が実行ファイルを書き換えています）" >&2
  echo "  配布物内: $sidecar" >&2
  echo "    sha256: ${actual_digest}" >&2
  echo "    サイズ: $(size_of "$sidecar") バイト" >&2
  echo "  同梱前  : $original" >&2
  echo "    sha256: ${expected_digest}" >&2
  echo "    サイズ: $(size_of "$original") バイト" >&2
  echo "  修復はしない（配布物は読み取り専用でありうる）。配置側の回避策（1.7 の決定 4）を見直すこと。" >&2
  exit 1
fi

echo "OK: 同梱前の原本とバイト一致しました（cmp）"
echo "  sha256（配布物内）: ${actual_digest}"
echo "  sha256（同梱前）  : ${expected_digest}"

# --- 起動の検査（要件 6.4） --------------------------------------------------
# 補助プロセスの `--idle` は起動行を 1 行出して待機する（1.6）。その行が期限内に現れることを
# もって「起動できる」と見なす。期待する行は実装が実際に出す形式そのものである。
ready_pattern='^sidecar-smoke ready pid=[0-9][0-9]* mode=idle$'

log=$(mktemp)
"$sidecar" --idle >"$log" 2>&1 &
sh_child_pid=$!

started=0
elapsed=0
while [ "$elapsed" -lt "$timeout_secs" ]; do
  if grep -q "$ready_pattern" "$log" 2>/dev/null; then
    started=1
    break
  fi
  sleep 1
  elapsed=$((elapsed + 1))
done

# 起動行が報告した実プロセスの識別子（Windows の後始末に使う）。
reported_pid=$(sed -n 's/^sidecar-smoke ready pid=\([0-9][0-9]*\) .*$/\1/p' "$log" | head -n 1)

if [ "$started" -ne 1 ]; then
  echo "NG: 取り出した補助プロセスが ${timeout_secs} 秒以内に起動しませんでした: $sidecar" >&2
  echo "  期待する起動行: sidecar-smoke ready pid=<識別子> mode=idle" >&2
  if kill -0 "$sh_child_pid" 2>/dev/null; then
    echo "  起動したプロセスは生存しています（起動行を出していない）" >&2
  else
    echo "  起動したプロセスは起動行を出す前に終了しています（実行形式でない・権限が無い等）" >&2
  fi
  echo "--- 補助プロセスの出力 ---" >&2
  if [ -s "$log" ]; then
    cat "$log" >&2
  else
    echo "(出力なし)" >&2
  fi
  exit 1
fi

echo "OK: 起動しました: $(sed -n '1p' "$log")"
echo "OK: 配布物からの取り出しと起動の検証に成功しました: $artifact"
exit 0

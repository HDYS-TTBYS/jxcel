#!/bin/sh
# check-render-traversal.sh — 10 万行 × 30 列の走査中のフレーム時間の中央値と、塗りの成立の検査
#
# 根拠（**tasks.md 1.6** / 要件 11.1 / 12.1 / 12.2。`design.md`「Performance & Scalability」の
# 「走査中の描画更新 = 毎秒 60 回 = 要件 11.1 = 実画面の観測（3 OS）」）:
#
#   1.6 は**判断点**である。使い捨ての画面（`smoke-glide-probe`）で 10 万行 × 30 列を走査し、
#   **フレーム時間の中央値**を測る。**毎秒 60 回（＝中央値 16.67 ms 以下）に届かなければ、
#   7.2 の実装を移植口の背後で自前 canvas へ切り替える判断を記録する。**
#
# **この検査器は恒久の予算ゲートではない。** 恒久の観測は 9.2（3 OS の走査と編集）と 9.3
# （走査の劣化の検出と診断への記録）が担う。本検査器と、それを呼ぶ 3 OS の段は**一時的**で
#   あり、9.2 / 9.3 が入ったときに**この段ごと取り除く**（`design.md`「計測が無い状態で予算
#   ゲートだけ先に結線しない」に対する、1.6 の側の結線である）。
#
# # 何を読むのか（そして、なぜその経路なのか）
#
# 走査の計測の結果は**使い捨ての画面が DOM の `aria-label` に 1 行で出す**
# （`[検証] グリッドの走査: 状態=… 中央値ms=… フレーム数=… 到達行=… 塗り=… 画素=…
# 色数=…`）。この行を**アクセシビリティの木から読む**（AT-SPI。`busctl` で D-Bus を直接叩く。
# 既存の `scripts/check-menu-shortcut.sh` と同じ読み方であり、`verification.md` の
# 「UI の中身はアクセシビリティの木を読むのが最も強い」に従う）。
#
# **アプリの診断記録（`TargetKind::Folder`）は使えない** — フロントエンドから診断記録へ書く
# 経路が無い（実測: wry は console を stdout へ流さず、`tauri-plugin-log` に `TargetKind::Webview`
# は無く、`document.title` もネイティブの題名へ伝わらない）。**新しい記録の仕組みを作らない**
# という 9.3 の制約（`design.md` 9.3「新しい記録の仕組みを作らない」）とも整合する。
#
# # 判定（3 つの主張）
#
#   1. **状態=measured** であること。`unmeasurable` は「測れなかった」であり、**通してはならない**
#      （フレームの標本が 1 本も取れない・走査が末尾へ届かない・塗りが成立しない、のいずれかで
#      起きる。**測定不能を「速い」と読ませない**のが 1.6 の要件である）。
#   2. **塗りの読み戻しが成立していること**（`塗り=ok` かつ `画素=17,205,238,255`）。
#      これは「DOM はあるが何も塗られない」症状（tauri-apps/tauri#15936、WebKitGTK 2.52.x）を
#      捕まえるための**1.6 の明示の要求**である。あわせて**標本の面の canvas の色数が 2 以上**で
#      あること（1 は一様＝何も塗られていない）。**読み戻しだけでは足りない** — 貼っていない
#      8x8 の canvas が塗れても、グリッドが描かれていることの証拠にはならないためである。
#   3. **走査が 10 万行の末尾へ届いていること**（`到達行` が `行数 - 1`）。
#
# # 予算（毎秒 60 回）を段で判定しない理由
#
# 本検査器は**実測値を出力へ記録する**（`中央値ms=` をそのまま出す）。**予算の合否はここで
# 決めない** — 1.6 の要求は「中央値を記録し、採否の判断を `research.md` へ追記する」であり、
# 判定は判断点として人が（そして 9.2 / 9.3 が）行う。**段を恒久のゲートにしない**という
# 一時的な段の性質とも整合する。ただし**閾値は出力に添える**（`閾値ms=16.67`）ので、
# 記録を読む者は超過をその場で見分けられる。
#
# # 環境（回避の環境変数）を段が与える
#
# 1.6 は「回避の環境変数（明示同期の無効化 → DMA-BUF レンダラの無効化 → 合成の無効化）の
# **有無それぞれ**で測る」ことを要求する。**本検査器は環境変数を設定しない**（呼び出し側が
# `env` で与える）。したがって同じ検査器を 4 回（無し / 各段）呼べば、そのまま 4 条件の計測に
# なる。深さは `JXCEL_TRAVERSAL_DEPTH`（既定 `none`）で受け、出力の識別に使う。
#
# # 使い方
#
#   check-render-traversal.sh <実行ファイル> <タイトル部分文字列> [タイムアウト秒] <記録ファイル> [深さ]
#
#   - 実行ファイル        : 検証用の形（`target/release/jxcel`）。**この検査は検証専用の初期画面
#                           を要求する**ので、配布物を渡すと非 0 で落ちる（反証に使える）。
#   - タイトル部分文字列  : ウィンドウの題名に含まれるべき文字列（`jxcel`）
#   - タイムアウト秒      : 既定 60。ウィンドウの出現を待つ上限
#   - 記録ファイル        : 診断記録（初回描画の成立を読む。4.4 の保存先の `jxcel.log`）
#   - 深さ                : `none` / `explicit-sync` / `dmabuf` / `all`（出力の識別だけに使う）
#
# # 終了コード
#   0 = 3 つの主張がすべて成立 / 1 = 検査失敗（測定不能・塗りの不成立・走査の不到達、または
#   アクセシビリティの木から計測の行が読めない）/ 2 = 入力が使えない（実行ファイル不在・
#   DISPLAY 不在・`xwininfo` / `busctl` / `python3` 不在・記録ファイルの指定なし）
#
# # 3 OS での実行
#
# POSIX sh のみで動くが、**アクセシビリティの木を読むには AT-SPI の橋が要る**（Linux の
# `GTK_MODULES=gail:atk-bridge`、macOS は既定で有効、Windows は**AT-SPI そのものが無い**）。
# Linux の段はこの検査器をそのまま呼ぶ。macOS の段も同様（AT-SPI は D-Bus の仕組みであり、
# macOS の GTK ビルドでは `at-spi2-core` が無いため、**実際には macOS / Windows では計測の行を
# 読めない**）。**その場合この検査器は 1 で落ちる。** 1.6 は「Windows と macOS の数値は 9.2 の
# 観測で確かめる」と明記しているので、**両 OS の段はこの検査器を呼ばず、起動そのものだけを
# 確かめて数値を 9.2 へ送る**（段の冒頭にその旨を書いてある）。
set -eu

usage() {
  echo "usage: $0 <app-path> <window-title-substring> [timeout-seconds] <record-file> [depth]" >&2
  exit 2
}

[ "$#" -ge 4 ] || usage

app=$1
title=$2
timeout_secs=${3:-60}
record=$4
depth=${5:-none}

if [ ! -x "$app" ]; then
  echo "NG: 実行ファイルが無いか実行権限がありません: $app" >&2
  exit 2
fi
if [ -z "$record" ]; then
  echo "NG: 記録ファイルのパスが指定されていません" >&2
  exit 2
fi
if [ -z "${DISPLAY:-}" ]; then
  echo "NG: DISPLAY が設定されていません（仮想ディスプレイ上で実行してください）" >&2
  exit 2
fi
for tool in xwininfo busctl python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "NG: $tool が見つかりません（x11-utils / systemd / python3 を導入してください）" >&2
    exit 2
  fi
done

work=$(mktemp -d)
app_log="$work/app.log"
app_pid=""
cleanup() {
  if [ -n "$app_pid" ] && kill -0 "$app_pid" 2>/dev/null; then
    # **子孫を先に終了する**（WebKit の補助プロセスが残ると、後続の段が単一インスタンスの
    # 機構に引き継がれて偽の失敗になる。`scripts/lib/x11-window.sh` と同じ前提）。
    if command -v pgrep >/dev/null 2>&1; then
      for child in $(pgrep -P "$app_pid" 2>/dev/null || true); do
        kill "$child" 2>/dev/null || true
      done
    fi
    kill "$app_pid" 2>/dev/null || true
    _n=0
    while [ "$_n" -lt 10 ] && kill -0 "$app_pid" 2>/dev/null; do
      sleep 0.5
      _n=$((_n + 1))
    done
    kill -9 "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

echo "check-render-traversal: 深さ=${depth} 閾値ms=16.67（毎秒 60 回 = 要件 11.1）"
echo "check-render-traversal: 実行ファイル=${app}"
echo "check-render-traversal: 記録ファイル=${record}"

# **記録は追記式である。**起動の直前に取った行数より後ろだけを読む（前の実行の成立行で
# 偽の成功をしない。`scripts/check-x11-render.sh` と同じ規律）。
record_lines() {
  if [ -f "$record" ]; then wc -l < "$record" | tr -d ' '; else echo 0; fi
}
before=$(record_lines)

# 1.6 の初期画面（検証専用。配布物は環境変数を読まないので既定の画面のままになる）。
JXCEL_VERIFICATION_INITIAL_SCREEN=smoke-glide-probe
export JXCEL_VERIFICATION_INITIAL_SCREEN

# 起動。標準出力・標準誤差は記録とは**別のファイル**へ（同じファイルへ書くと、アプリ
# (`TargetKind::Stdout`) と診断記録 (`TargetKind::Folder`) が独立の書き手として同じファイルを
# 先頭と末尾から触り、記録の行を壊しうる。macOS の段の冒頭に同じ注記がある）。
"$app" >"$app_log" 2>&1 &
app_pid=$!

# 2. ウィンドウの出現（1.5 / 10.3 / 10.4 と同じ観測）。**最小寸法 100x100 を要求する** —
#    GTK は 10x10 程度の補助ウィンドウも同じ題名で作る（実測: `10x10+10+10` を先に拾った）。
deadline=$(( $(date +%s) + timeout_secs ))
window_lines=""
while [ "$(date +%s)" -lt "$deadline" ]; do
  if ! kill -0 "$app_pid" 2>/dev/null; then
    if [ -n "$window_lines" ]; then break; fi
    echo "NG: アプリがウィンドウを出す前に終了しました。出力の末尾:" >&2
    tail -n 20 "$app_log" >&2 || true
    exit 1
  fi
  window_lines=$(xwininfo -root -tree 2>/dev/null |
    grep -F "\"$title\"" |
    grep -F -v 'mutter-x11-frames' |
    grep -E '[^0-9]([0-9]{3,})x([0-9]{3,})[+-]' || true)
  if [ -n "$window_lines" ]; then break; fi
  sleep 0.1
done
if [ -z "$window_lines" ]; then
  echo "NG: ${timeout_secs} 秒以内にウィンドウが現れませんでした。出力の末尾:" >&2
  tail -n 20 "$app_log" >&2 || true
  exit 1
fi
echo "ウィンドウ: $(printf '%s\n' "$window_lines" | head -n 1)"

# 3. 初回描画の成立（8.2 の成立行。**描画が成立していなければ、走査の計測も成立しない**）。
#    期限はウィンドウ生成から 3 秒なので、これを大きく超える猶予で足りる。
painted=""
render_deadline=$(( $(date +%s) + 20 ))
while [ "$(date +%s)" -lt "$render_deadline" ]; do
  painted=$(tail -n "+$(( before + 1 ))" "$record" 2>/dev/null |
    grep -E '初回描画が成立した:|初回描画は成立したがソフトウェアラスタライザ経由である:|期限超過のあとに描画の通知が届いた' |
    head -n 1 || true)
  if [ -n "$painted" ]; then break; fi
  if ! kill -0 "$app_pid" 2>/dev/null; then break; fi
  sleep 0.2
done
if [ -z "$painted" ]; then
  echo "NG: 初回描画の成立行が 20 秒以内に現れませんでした（この実行の記録には描画の成立がありません）" >&2
  tail -n "+$(( before + 1 ))" "$record" 2>/dev/null | tail -n 20 >&2 || true
  exit 1
fi
echo "初回描画: $painted"

# 4. 走査の計測の行をアクセシビリティの木から読む（**この 1 行が計測の一次証拠である**）。
#    走査は 333 フレーム前後（10 万行 ÷ 1 フレーム 300 行）掛かるので、猶予は 60 秒取る。
#
#    読み方は `scripts/check-menu-shortcut.sh` の `atspi-tree` と同じである（`busctl --json=short`
#    で `org.a11y.atspi.Accessible` の名前をたどる薄い実装。`gir1.2-atspi` を要求しない）。
#    **巨大な部分木（`table` / `grid` / `document web`）は刈る** — 名前に用が無いためである。
atspi_read_name() {
  python3 - "$title" "グリッドの走査" <<'PY'
import json
import subprocess
import sys


def run(command):
    result = subprocess.run(command, capture_output=True, text=True, timeout=25)
    if result.returncode != 0:
        raise RuntimeError(" ".join(command) + "\n" + result.stderr.strip())
    return result.stdout


class Atspi:
    def __init__(self):
        out = run([
            "busctl", "--user", "call", "org.a11y.Bus", "/org/a11y/bus",
            "org.a11y.Bus", "GetAddress",
        ])
        if out.lstrip().startswith("{"):
            self.address = json.loads(out)["data"][0]
        else:
            self.address = out.strip().split('"')[1]

    def _json(self, *args):
        return json.loads(run(["busctl", f"--address={self.address}", "--json=short", *args]))

    def call(self, dest, path, interface, method, *args):
        data = self._json("call", dest, path, interface, method, *args)["data"]
        if isinstance(data, list) and len(data) == 1:
            return data[0]
        return data

    def property(self, dest, path, interface, name):
        return self._json("get-property", dest, path, interface, name)["data"]

    def children(self, dest, path):
        return [(name, child) for name, child in self.call(dest, path, ACCESSIBLE, "GetChildren")]

    def name(self, dest, path):
        return self.property(dest, path, ACCESSIBLE, "Name")


ACCESSIBLE = "org.a11y.atspi.Accessible"
REGISTRY = "org.a11y.atspi.Registry"
ROOT_PATH = "/org/a11y/atspi/accessible/root"
PRUNE_ROLES = {"scroll pane", "document web", "document frame", "table", "grid", "tree table"}
MAX_DEPTH = 18

app_name = sys.argv[1]
needle = sys.argv[2]

atspi = Atspi()
app = None
for name, path in atspi.children(REGISTRY, ROOT_PATH):
    try:
        if atspi.name(name, path) == app_name:
            app = (name, path)
    except RuntimeError:
        continue
if app is None:
    sys.exit(3)

found = []
stack = [(app[0], app[1], 0)]
while stack:
    dest, path, depth = stack.pop()
    if depth > MAX_DEPTH:
        continue
    role = atspi.call(dest, path, ACCESSIBLE, "GetRoleName")
    name = atspi.name(dest, path)
    if needle in name:
        found.append(name)
    if role in PRUNE_ROLES:
        continue
    for child_name, child_path in atspi.children(dest, path):
        stack.append((child_name, child_path, depth + 1))

for line in found:
    print(line)
PY
}

measure_line=""
measure_deadline=$(( $(date +%s) + 60 ))
while [ "$(date +%s)" -lt "$measure_deadline" ]; do
  # **「行が現れた」では足りない。** 計測の行は画面の初期状態から存在し、そのときは
  # `状態=未測定` である（`GlideProbe` の doc「計測がまだ終わっていない」）。**終わりの状態
  # （measured / unmeasurable）になるまで待つ** — ここで最初の 1 行を取ると、走査の前に
  # 読んでしまい「測定不能」と誤判定する（実際に起きた）。
  candidate=$(atspi_read_name 2>/dev/null || true)
  case "$candidate" in
    *"状態=measured"*|*"状態=unmeasurable"*)
      measure_line=$candidate
      break
      ;;
  esac
  if ! kill -0 "$app_pid" 2>/dev/null; then break; fi
  sleep 1
done
if [ -z "$measure_line" ]; then
  echo "NG: アクセシビリティの木から走査の計測の行（'[検証] グリッドの走査 …'）を読めませんでした" >&2
  echo "    （アクセシビリティの橋が読み込まれていないか、計測がまだ終わっていません。" >&2
  echo "     GTK_MODULES=gail:atk-bridge と、D-Bus のセッションバスを確かめてください）" >&2
  echo "     アプリの出力の末尾:" >&2
  tail -n 20 "$app_log" >&2 || true
  exit 1
fi
echo "計測: $measure_line"
echo "記録（深さ=${depth}）: $measure_line"

# 5. 3 つの主張の判定（**測定不能を通さない**）。
field() {
  printf '%s\n' "$measure_line" | sed -n "s/.* $1=\([^ ]*\).*/\1/p" | head -n 1
}
status=$(field "状態")
median=$(field "中央値ms")
frames=$(field "フレーム数")
reached=$(field "到達行")
rows=$(field "行数")
columns=$(field "列数")
paint=$(field "塗り")
pixel=$(field "画素")
colors=$(field "色数")
tick=$(field "時計刻みms")

if [ "$status" != "measured" ]; then
  echo "NG: 計測の状態が measured ではありません（${status}）。測定不能を通してはいけません" >&2
  echo "    理由: $(printf '%s\n' "$measure_line" | sed -n 's/.* 理由=//p')" >&2
  exit 1
fi
if [ "$paint" != "ok" ]; then
  echo "NG: 塗りの読み戻しが成立していません（塗り=${paint} 画素=${pixel}）— DOM はあるが何も塗られない症状の検査である" >&2
  exit 1
fi
if [ "$pixel" != "17,205,238,255" ]; then
  echo "NG: 読み戻した画素が期待と一致しません（期待 17,205,238,255 / 実際 ${pixel}）" >&2
  exit 1
fi
case "$colors" in
  ''|*[!0-9]*)
    echo "NG: canvas の色数を読めませんでした（色数=${colors}）" >&2
    exit 1
    ;;
esac
if [ "$colors" -lt 2 ]; then
  echo "NG: 標本の面の canvas が一様です（色数=${colors}。DOM はあるが何も塗られていない）" >&2
  exit 1
fi
if [ "$reached" != "$(( rows - 1 ))" ]; then
  echo "NG: 走査が末尾へ届いていません（到達行=${reached} / 期待 $(( rows - 1 ))）— 全件の走査ではない" >&2
  exit 1
fi

echo "OK: 走査の計測（深さ=${depth}）状態=measured 中央値ms=${median} フレーム数=${frames} 到達行=${reached}/${rows} 列=${columns} 塗り=ok 画素=${pixel} 色数=${colors} 時計刻みms=${tick}"
echo "OK: 閾値は 16.67 ms（毎秒 60 回）。中央値 ${median} ms の採否の判断は research.md が持つ（この段は判定しない）"

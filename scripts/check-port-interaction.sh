#!/bin/sh
# check-port-interaction.sh — 描画層の移植口の実装（GlideAdapter）を実物の上で駆動し、3 つの
# 主張を観測する検査器
#
# 根拠（**tasks.md 7.2** / 要件 1.2 / 1.3 / 2.4 / 7.1 / 7.2。`tech.md`「GUI・配布物・
# プラットフォーム差を含む主張は、実物を起動して観測した結果で裏付けること。単体テストは回帰の
# 網であって受入の証明ではない」）:
#
#   7.2 の受け入れは**見え方の主張**である —「10 万行のシートを走査でき、選択した範囲が視覚的に
#   区別でき、列幅と列の位置が操作できる」。これは使い捨ての画面（`smoke-port-probe`）が
#   **移植口の実装を実際に駆動して**観測する。駆動の内容（何を注ぎ、何を読むか）は
#   `src/features/smoke/portProbeAdapter.tsx` の冒頭にある。
#
# **この検査器は恒久の予算ゲートではない。** 恒久の 3 OS の観測は 9.2（走査と編集の成立）と
# 9.3（走査の劣化の検出）が担う。本検査器と、それを呼ぶ 3 OS の段は**一時的**であり、
# 9.2 / 9.3 が入ったときに**この段ごと取り除く**（1.6 の
# `scripts/check-render-traversal.sh` と同じ性質である）。
#
# # 何を読むのか（そして、なぜその経路なのか）
#
# 駆動の結果は**使い捨ての画面が DOM の `aria-label` に 1 行で出す**
# （`[検証] 移植口の操作: 状態=… 到達行=… 選択=… 列幅=… 列の移動=… 見出し=… 往復=…`）。
# この行を**アクセシビリティの木から読む**（AT-SPI。`busctl` で D-Bus を直接叩く。
# `scripts/check-menu-shortcut.sh` / `scripts/check-render-traversal.sh` と同じ読み方であり、
# `verification.md` の「UI の中身はアクセシビリティの木を読むのが最も強い」に従う）。
#
# **アプリの診断記録（`TargetKind::Folder`）は使えない** — フロントエンドから診断記録へ書く
# 経路が無い（実測: wry は console を stdout へ流さず、`tauri-plugin-log` に `TargetKind::Webview`
# は無く、`document.title` もネイティブの題名へ伝わらない。1.6 の `research.md`
# 「測れなかったこと」に同じ記録がある）。**新しい記録の仕組みを作らない**という 9.3 の制約とも
# 整合する。
#
# # 判定（3 つの主張と、その裏付け）
#
#   1. **状態=ok** であること。`failed` は「駆動の途中で例外が出た」であり、**通してはならない**
#      （理由は `理由=` に出る。**数を捏造しない**のが駆動側の規律である）。
#   2. **10 万行を走査できる**: `到達行` が `行数 - 1` であり、`scrollTop` が
#      `scrollHeight - clientHeight`（末尾）に一致し、`塗り=ok`（既知の色 `17,205,238,255` が
#      読み戻る）かつ `色数` が 2 以上であること（**「DOM はあるが何も塗られない」症状**を
#      捕まえる。1.6 が同じ検査を置いている）。
#   3. **選択した範囲が視覚的に区別できる**: 移植口が受け取った選択が `1:0-3:1` であり、
#      **同じ画素の色が選択の前後で変わっている**こと（`選択画素=変化`。変わらなければ選択は
#      見えていない）。加えてアクセシビリティの木で選択として印されたセルが 6 つ以上あること。
#   4. **列幅と列の位置が操作できる**: 移植口が受け取った列幅の変更（`列幅=添字:変更前→変更後`）の
#      増分が、**描かれた内容の幅の増分**と一致すること（= 変更が実際に描かれた幅に現れている）。
#      列の移動は `列の移動=2→0` であり、**描かれている見出しの並び**の先頭が
#      `列2,列0,列1` であること（= 並びが実際に変わっている）。
#   5. **クリップボードの配管**（要件 7.1、7.2）: `複製範囲` が選択の範囲であり、貼り付けの
#      `往復=一致`（送った文字列と移植口が受け取った文字列が 1 バイトも違わない）であること。
#      `クリップボード` は `一致` / `不一致` / `不可` のいずれかであり、**`不可` を `一致` と
#      読み替えない**（読み取りの権限が無い環境では「読めなかった」ままにする）。
#   6. **読み込み中は空白ではない**: `読み込み画素=n/m` の `n` が 0 でないこと（末尾は取得済みで
#      ない行であり、地色と違う画素があればそれは骨組みの棒である。**0 なら何も描かれていない**）。
#
# # 使い方
#
#   check-port-interaction.sh <実行ファイル> <タイトル部分文字列> [タイムアウト秒] <記録ファイル>
#
#   - 実行ファイル        : 検証用の形（`target/release/jxcel`）。**この検査は検証専用の初期画面
#                           を要求する**ので、配布物を渡すと非 0 で落ちる（反証に使える）。
#   - タイトル部分文字列  : ウィンドウの題名に含まれるべき文字列（`jxcel`）
#   - タイムアウト秒      : 既定 60。ウィンドウの出現を待つ上限
#   - 記録ファイル        : 診断記録（初回描画の成立を読む。4.4 の保存先の `jxcel.log`）
#
# # 終了コード
#   0 = 3 つの主張とクリップボードの配管がすべて成立 / 1 = 検査失敗（駆動の失敗・塗りの不成立・
#   走査の不到達・選択の画素が変わらない・幅や並びが変わらない・往復の不一致、または
#   アクセシビリティの木から観測の行が読めない）/ 2 = 入力が使えない（実行ファイル不在・
#   DISPLAY 不在・`xwininfo` / `busctl` / `python3` 不在・記録ファイルの指定なし）
#
# # 3 OS での実行
#
# POSIX sh のみで動くが、**アクセシビリティの木を読むには AT-SPI の橋が要る**（Linux の
# `GTK_MODULES=gail:atk-bridge`、macOS は既定で有効、Windows は**AT-SPI そのものが無い**）。
# したがって、この検査器をそのまま数値を出す段に使えるのは Linux だけである
# （1.6 の `check-render-traversal.sh` が同じ制約を冒頭に書いている）。
# **macOS / Windows の段はこの検査器を呼ばず、画面が描画されることだけを確かめる** —
# 移植口の操作の恒久の 3 OS の観測は 9.2 が担う。
set -eu

usage() {
  echo "使い方: check-port-interaction.sh <実行ファイル> <タイトル部分文字列> [タイムアウト秒] <記録ファイル>" >&2
  exit 2
}

[ "$#" -ge 3 ] || usage

app=$1
title=$2
if [ "$#" -ge 4 ]; then
  timeout_secs=$3
  record=$4
else
  timeout_secs=60
  record=$3
fi

if [ ! -x "$app" ]; then
  echo "check-port-interaction: 実行ファイルがありません: $app" >&2
  exit 2
fi
if [ -z "$record" ]; then
  echo "check-port-interaction: 記録ファイルの指定がありません（初回描画の成立を読むのに要る）" >&2
  exit 2
fi
if [ -z "${DISPLAY:-}" ]; then
  echo "check-port-interaction: DISPLAY がありません（ウィンドウを観測できません）" >&2
  exit 2
fi
for tool in xwininfo busctl python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "check-port-interaction: $tool が見つかりません" >&2
    exit 2
  fi
done

work=$(mktemp -d)
app_log="$work/app.log"
app_pid=""
cleanup() {
  if [ -n "$app_pid" ] && kill -0 "$app_pid" 2>/dev/null; then
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

echo "check-port-interaction: 実行ファイル=${app}"
echo "check-port-interaction: 記録ファイル=${record}"

# **記録は追記式である。**起動の直前に取った行数より後ろだけを読む（前の実行の成立行で
# 偽の成功をしない。`scripts/check-x11-render.sh` と同じ規律）。
record_lines() {
  if [ -f "$record" ]; then wc -l < "$record" | tr -d ' '; else echo 0; fi
}
before=$(record_lines)

# 7.2 の初期画面（検証専用。配布物は環境変数を読まないので既定の画面のままになる）。
JXCEL_VERIFICATION_INITIAL_SCREEN=smoke-port-probe
export JXCEL_VERIFICATION_INITIAL_SCREEN

# 起動。標準出力・標準誤差は記録とは**別のファイル**へ（同じファイルへ書くと、アプリ
# (`TargetKind::Stdout`) と診断記録 (`TargetKind::Folder`) が独立の書き手として同じファイルを
# 先頭と末尾から触り、記録の行を壊しうる）。
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

# 3. 初回描画の成立（8.2 の成立行。**描画が成立していなければ、移植口の駆動も成立しない**）。
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

# 4. 観測の行をアクセシビリティの木から読む（**この 1 行が一次証拠である**）。
#
#    読み方は `scripts/check-render-traversal.sh` の `atspi_read_name` と同じである
#    （`busctl --json=short` で `org.a11y.atspi.Accessible` の名前をたどる薄い実装。
#     `gir1.2-atspi` を要求しない）。**巨大な部分木（`table` / `grid` / `document web`）は刈る**
#    — 名前に用が無いためである。
atspi_read_name() {
  # 読み出しの針は**観測の行そのもの**である（画面の見出し「描画確認: 移植口の操作」も
  # 同じ語を含むので、状態の欄まで含めて針にする）。
  python3 - "$title" "移植口の操作: 状態=" <<'PY'
import json
import subprocess
import sys
from collections import deque


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
# **刈るのは巨大な部分木だけである。**頁の中身（`document web` / `scroll pane`）を刈っては
# ならない — 読む針（観測の行）は頁の中にある（実測: この刈り方で CI の Linux ランナーが
# 「観測の行を読めませんでした」で落ちた。頁の下の節は役割が空であり、刈ると針へ届かない）。
PRUNE_ROLES = {"table", "grid", "tree table"}
MAX_DEPTH = 18
# **1 回の歩きに上限を置く。**深く広く歩き続けると **WebKit のアクセシビリティが答えなくなる**
# （実測: 565 節まで見えた直後の巡回から 14 節へ落ち、以後 300 秒以上戻らなかった）。
# 針は頁の浅い位置にあるので、上限つきの幅優先（浅い節から見る）で足りる。
VISIT_BUDGET = 900

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
stack = deque([(app[0], app[1], 0)])
visits = 0
while stack:
    dest, path, depth = stack.popleft()
    if depth > MAX_DEPTH or visits >= VISIT_BUDGET:
        continue
    visits += 1
    try:
        role = atspi.call(dest, path, ACCESSIBLE, "GetRoleName")
        name = atspi.name(dest, path)
        children = atspi.children(dest, path)
    except (RuntimeError, ValueError, json.JSONDecodeError):
        # **過渡の失敗で歩みを止めない**（木は走行中に変わる。消えた経路を読むと
        # 「オブジェクトが存在しません」が返る）。
        continue
    if needle in name:
        found.append(name)
    if role in PRUNE_ROLES:
        continue
    for child_name, child_path in children:
        stack.append((child_name, child_path, depth + 1))

for line in found:
    print(line)
PY
}

# 駆動は選択・複製・貼り付け・列幅・列の移動・末尾への走査を順に行う。段ごとに描画と
# アクセシビリティの木の確定を待つ（木は 200 ms 遅れる）ので、猶予は 90 秒取る。
#
# **「行が現れた」では足りない。** 観測の行は画面の初期状態から存在し、そのときは `状態=未測定`
# である。**終わりの状態（ok / failed）になるまで待つ**（1.6 の検査器が `measured` /
# `unmeasurable` を待つのと同じ理由である）。
report=""
report_deadline=$(( $(date +%s) + 90 ))
while [ "$(date +%s)" -lt "$report_deadline" ]; do
  candidate=$(atspi_read_name 2>/dev/null || true)
  case "$candidate" in
    *"状態=ok"*|*"状態=failed"*)
      report=$candidate
      break
      ;;
  esac
  if ! kill -0 "$app_pid" 2>/dev/null; then break; fi
  sleep 1
done
if [ -z "$report" ]; then
  echo "NG: アクセシビリティの木から観測の行（'[検証] 移植口の操作 …'）を読めませんでした" >&2
  echo "    （アクセシビリティの橋が読み込まれていないか、駆動がまだ終わっていません。" >&2
  echo "     GTK_MODULES=gail:atk-bridge と、D-Bus のセッションバスを確かめてください）" >&2
  echo "     アプリの出力の末尾:" >&2
  tail -n 20 "$app_log" >&2 || true
  exit 1
fi
echo "観測: $report"

# 5. 主張の判定（**測定不能を通さない**）。
field() {
  printf '%s\n' "$report" | sed -n "s/.* $1=\([^ ]*\).*/\1/p" | head -n 1
}

status=$(field "状態")
rows=$(field "行数")
reached=$(field "到達行")
scroll_top=$(field "scrollTop")
scroll_height=$(field "scrollHeight")
client_height=$(field "clientHeight")
paint=$(field "塗り")
pixel=$(field "画素")
colors=$(field "色数")
selection=$(field "選択")
selected_cells=$(field "選択セル数")
selection_pixel=$(field "選択画素")
resize=$(field "列幅")
content_width=$(field "内容幅")
move=$(field "列の移動")
header_order=$(field "見出し")
copy_range=$(field "複製範囲")
copy_chars=$(field "複製文字数")
paste_anchor=$(field "錨")
round_trip=$(field "往復")
loading_pixels=$(field "読み込み画素")

if [ "$status" != "ok" ]; then
  echo "NG: 駆動の状態が ok ではありません（${status}）。失敗を通してはいけません" >&2
  echo "    理由: $(printf '%s\n' "$report" | sed -n 's/.* 理由=//p')" >&2
  exit 1
fi

# 主張 1: 10 万行を走査できる。
if [ "$reached" != "$(( rows - 1 ))" ]; then
  echo "NG: 走査が末尾へ届いていません（到達行=${reached} / 期待 $(( rows - 1 ))）" >&2
  exit 1
fi
case "$scroll_top" in
  ''|*[!0-9]*) echo "NG: scrollTop を読めませんでした（${scroll_top}）" >&2; exit 1 ;;
esac
case "$scroll_height" in
  ''|*[!0-9]*) echo "NG: scrollHeight を読めませんでした（${scroll_height}）" >&2; exit 1 ;;
esac
case "$client_height" in
  ''|*[!0-9]*) echo "NG: clientHeight を読めませんでした（${client_height}）" >&2; exit 1 ;;
esac
# 末尾であることの判定（`scrollTop` は `scrollHeight - clientHeight` に一致するはずである）。
expected_bottom=$(( scroll_height - client_height ))
if [ "$scroll_top" -ne "$expected_bottom" ]; then
  echo "NG: 表示位置が末尾ではありません（scrollTop=${scroll_top} / 期待 ${expected_bottom} = scrollHeight ${scroll_height} - clientHeight ${client_height}）" >&2
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
  ''|*[!0-9]*) echo "NG: canvas の色数を読めませんでした（色数=${colors}）" >&2; exit 1 ;;
esac
if [ "$colors" -lt 2 ]; then
  echo "NG: 標本の面の canvas が一様です（色数=${colors}。DOM はあるが何も塗られていない）" >&2
  exit 1
fi

# 主張 2: 選択した範囲が視覚的に区別できる。
if [ "$selection" != "1:0-3:1" ]; then
  echo "NG: 移植口が受け取った選択が期待と違います（選択=${selection} / 期待 1:0-3:1）" >&2
  exit 1
fi
if [ "$selection_pixel" != "変化" ]; then
  echo "NG: 選択の前後で同じ画素の色が変わっていません（選択画素=${selection_pixel}）— 選択が見えていない" >&2
  exit 1
fi
case "$selected_cells" in
  ''|*[!0-9]*) echo "NG: 選択として印されたセルの数を読めませんでした（${selected_cells}）" >&2; exit 1 ;;
esac
if [ "$selected_cells" -lt 6 ]; then
  echo "NG: 選択として印されたセルが 6 つ未満です（${selected_cells}。3 行 × 2 列を選んでいる）" >&2
  exit 1
fi

# 主張 3: 列幅と列の位置が操作できる。
case "$resize" in
  1:*→*) ;;
  *)
    echo "NG: 列幅の変更が移植口へ届いていません（列幅=${resize}）" >&2
    exit 1
    ;;
esac
resize_from=$(printf '%s\n' "$resize" | sed -n 's/^1:\([0-9]*\)→.*$/\1/p')
resize_to=$(printf '%s\n' "$resize" | sed -n 's/^1:[0-9]*→\([0-9]*\)$/\1/p')
if [ -z "$resize_from" ] || [ -z "$resize_to" ]; then
  echo "NG: 列幅の変更の前後を読めませんでした（列幅=${resize}）" >&2
  exit 1
fi
if [ "$(( resize_to - resize_from ))" -ne 120 ]; then
  echo "NG: 掴んで動かした量と幅の増分が一致しません（${resize_from}→${resize_to}。120 px 動かした）" >&2
  exit 1
fi
width_before=$(printf '%s\n' "$content_width" | sed -n 's/^\([0-9]*\)→.*$/\1/p')
width_after=$(printf '%s\n' "$content_width" | sed -n 's/^[0-9]*→\([0-9]*\)$/\1/p')
if [ -z "$width_before" ] || [ -z "$width_after" ]; then
  echo "NG: 内容の幅の前後を読めませんでした（内容幅=${content_width}）" >&2
  exit 1
fi
if [ "$(( width_after - width_before ))" -ne 120 ]; then
  echo "NG: 変更が描かれた幅に現れていません（内容幅=${content_width}。幅の増分は 120 px である）" >&2
  exit 1
fi
if [ "$move" != "2→0" ]; then
  echo "NG: 列の移動が移植口へ届いていません（列の移動=${move} / 期待 2→0）" >&2
  exit 1
fi
case "$header_order" in
  列2,列0,列1,*) ;;
  *)
    echo "NG: 描かれている見出しの並びが変わっていません（見出し=${header_order} / 期待 列2,列0,列1,…）" >&2
    exit 1
    ;;
esac

# クリップボードの配管（要件 7.1、7.2）。
if [ "$copy_range" != "1:0-3:1" ]; then
  echo "NG: 複製の範囲が選択と一致しません（複製範囲=${copy_range} / 期待 1:0-3:1）" >&2
  exit 1
fi
case "$copy_chars" in
  ''|*[!0-9]*) echo "NG: 複製した文字数を読めませんでした（${copy_chars}）" >&2; exit 1 ;;
esac
if [ "$copy_chars" -le 0 ]; then
  echo "NG: 複製した文字列が空です（複製文字数=${copy_chars}）" >&2
  exit 1
fi
if [ "$paste_anchor" != "1:0" ]; then
  echo "NG: 貼り付けの錨が選択の左上ではありません（錨=${paste_anchor} / 期待 1:0）" >&2
  exit 1
fi
if [ "$round_trip" != "一致" ]; then
  echo "NG: 貼り付けの往復が一致しません（往復=${round_trip}）— 移植口は文字列を解釈しないはずである" >&2
  exit 1
fi

# 読み込み中は空白ではない（骨組みの棒が引かれている）。
case "$loading_pixels" in
  ''|*[!0-9/*]*) echo "NG: 読み込み中の画素の数を読めませんでした（${loading_pixels}）" >&2; exit 1 ;;
esac
loading_hit=$(printf '%s\n' "$loading_pixels" | sed -n 's#^\([0-9]*\)/.*$#\1#p')
loading_all=$(printf '%s\n' "$loading_pixels" | sed -n 's#^[0-9]*/\([0-9]*\)$#\1#p')
if [ -z "$loading_hit" ] || [ -z "$loading_all" ]; then
  echo "NG: 読み込み中の画素の数を読めませんでした（${loading_pixels}）" >&2
  exit 1
fi
if [ "$loading_hit" -eq 0 ]; then
  echo "NG: 読み込み中の行に何も描かれていません（読み込み画素=${loading_pixels}。空白のセルは「値なし」と区別がつかない）" >&2
  exit 1
fi

echo "OK: 走査 到達行=${reached}/${rows} scrollTop=${scroll_top}（末尾）塗り=ok 画素=${pixel} 色数=${colors}"
echo "OK: 選択 範囲=${selection} 選択セル数=${selected_cells} 同じ画素の色が変化（${selection_pixel}）"
echo "OK: 列幅 ${resize}（内容幅 ${content_width}）列の移動 ${move}（見出し ${header_order}）"
echo "OK: クリップボード 複製範囲=${copy_range} 文字数=${copy_chars} 錨=${paste_anchor} 往復=${round_trip} クリップボード=$(field "クリップボード")"
echo "OK: 読み込み中の骨組み ${loading_pixels} 画素が地色と異なる（地色 $(field "地色画素")）"

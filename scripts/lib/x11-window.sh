#!/bin/sh
# X11 上でウィンドウを観測するための**共有部品**（tasks.md 1.5 / 10.3 / 10.4 / 10.5）。
#
# `scripts/check-x11-window.sh`（配布物の起動と起動時間の計測。10.3）と
# `scripts/check-x11-render.sh`（初回描画と画面の識別。10.4）、
# `scripts/check-multi-window.sh`（単一インスタンスと複数ウィンドウの経路。10.5）が source する。
# **ウィンドウの観測の仕方は 1 つでなければならない** — タイトル一致と最小寸法の判定、
# `xwininfo` の行の解析、起動したプロセス木の片付けは、どの段でも同じ前提（GTK の補助ウィンドウを
# 除く・AppImage の展開実行ではラッパーだけを終了しても本体が残る）に立つ。**この前提が
# 2 箇所に分裂すると、片方だけ直したときに検査の意味がずれる。**
#
# 共有しないもの: 計測（10.3 の区間。ミリ秒時計を要求する）、待つ期限の単位（秒 / ミリ秒）、
# 失敗時の診断の並べ方（ウィンドウ検査は xwininfo の一致行を、描画検査は診断記録を出す）、
# 起動の行そのもの（描画検査だけが検証用の環境変数を渡す）。**これらは各段の意味そのもの
# なので、この置き場へは寄せない。**
#
# source する側の契約:
#   - POSIX sh。`set -eu` の下で使う（この置き場はトップレベルで変数を参照しない）。
#   - 実行ファイルのパスとタイトルは呼び出し側が持つ。この置き場は X11 の観測だけを行う。
#   - 起動したアプリの pid は `x11_pid`、アプリの出力の控えは `x11_log` である
#     （呼び出し側はこの 2 つを自分で別名へ写さない — 片付けのトラップがここにある）。
#
# 終了コードの規約は呼び出し側の doc に従う（この置き場の関数は `exit 2`＝入力が使えない、
# を返しうる）。
#
# この置き場が定義する変数（`x11_pid` / `x11_log` / `x11_poll_sleep` / `x11_window_match` /
# `x11_window_lines` / `x11_window_ids` / `x11_window_count` / `x11_died`）は **source する側が
# 読む**契約である。単体で shellcheck にかけると「未使用」に見えるため、ファイル全体で抑止する
# （呼び出し側は `scripts/check-x11-window.sh` / `scripts/check-x11-render.sh` /
# `scripts/check-multi-window.sh`）。
# shellcheck disable=SC2034
set -eu

# 実行ファイルの存在と実行権限（呼び出し側の引数の前提）。
x11_require_app() {
  if [ ! -x "$1" ]; then
    echo "NG: 実行ファイルが無いか実行権限がありません: $1" >&2
    exit 2
  fi
}

# X11 の観測に必要な前提（`xwininfo` と `DISPLAY`）。
#
# **`DISPLAY` が無ければ落とす。** Wayland に行った GTK アプリは X のウィンドウツリーに
# 現れないので、この検査は成立しない（呼び出し側は `GDK_BACKEND=x11` も渡す）。
x11_require_environment() {
  if ! command -v xwininfo >/dev/null 2>&1; then
    echo "NG: xwininfo が見つかりません（x11-utils を導入してください）" >&2
    exit 2
  fi
  if [ -z "${DISPLAY:-}" ]; then
    echo "NG: DISPLAY が設定されていません（仮想ディスプレイ上で実行してください）" >&2
    exit 2
  fi
}

# 検出粒度。分数秒を受け付けない `sleep` では 1 秒へ退避する（計測値はその粒度だけ大きく
# 出る＝保守側。10.3）。結果は `x11_poll_sleep` に入る。
x11_pick_poll_sleep() {
  if sleep 0.1 2>/dev/null; then
    x11_poll_sleep=0.1
  else
    x11_poll_sleep=1
  fi
}

# 起動したアプリを必ず片付ける（EXIT / INT / TERM）。**子孫を先に終了する** — AppImage を
# 展開実行（`APPIMAGE_EXTRACT_AND_RUN=1`）した場合、ラッパーの子として本体が動くため、
# ラッパーだけを終了すると本体が残る。残ると同じジョブの後続の段が単一インスタンスの機構に
# 引き継がれ、ウィンドウが出ない（偽の失敗になる）。
#
# **再帰を使わない。**POSIX sh にローカル変数が無いため、再帰するとループ変数と対象の pid が
# 内側の呼び出しで上書きされ、**対象そのものへシグナルが届かない**（実測: WebKit の補助
# プロセスを持つアプリで、TERM が補助プロセスにだけ送られ、対象が生き残った）。子孫は幅優先で
# 集めてから、深い方から順に送る。
#
# shellcheck disable=SC2329 # trap 経由の cleanup から呼ばれる（shellcheck は trap を追えない）
x11_kill_tree() {
  _x11_tree_pids=$1
  if command -v pgrep >/dev/null 2>&1; then
    _x11_tree_front=$1
    while [ -n "$_x11_tree_front" ]; do
      _x11_tree_next=""
      for _x11_tree_p in $_x11_tree_front; do
        _x11_tree_children=$(pgrep -P "$_x11_tree_p" 2>/dev/null || true)
        if [ -n "$_x11_tree_children" ]; then
          _x11_tree_next="$_x11_tree_next $_x11_tree_children"
        fi
      done
      if [ -n "$_x11_tree_next" ]; then
        _x11_tree_pids="$_x11_tree_pids $_x11_tree_next"
      fi
      _x11_tree_front=$_x11_tree_next
    done
  fi
  # 深い方（子孫）から対象の順に送る。**蓄積は空白区切りなので、いったん 1 行 1 pid の形に
  # 直してから逆順に並べ替える**（蓄積した文字列をそのまま `printf '%s\n'` に渡すと 1 行の
  # ままになり、逆順が効かない）。
  _x11_tree_lines=""
  for _x11_tree_p in $_x11_tree_pids; do
    _x11_tree_lines="$_x11_tree_lines$_x11_tree_p
"
  done
  for _x11_tree_p in $(printf '%s\n' "$_x11_tree_lines" |
    awk '{ line[NR] = $0 } END { for (i = NR; i >= 1; i--) print line[i] }'); do
    kill "$_x11_tree_p" 2>/dev/null || true
  done
}

# shellcheck disable=SC2329 # 下の trap から呼ばれる
x11_cleanup() {
  if [ -n "${x11_pid:-}" ] && kill -0 "$x11_pid" 2>/dev/null; then
    x11_kill_tree "$x11_pid"
    _x11_n=0
    while [ "$_x11_n" -lt 10 ] && kill -0 "$x11_pid" 2>/dev/null; do
      sleep 0.5
      _x11_n=$((_x11_n + 1))
    done
    kill -9 "$x11_pid" 2>/dev/null || true
    wait "$x11_pid" 2>/dev/null || true
  fi
  if [ -n "${x11_log:-}" ] && [ -f "$x11_log" ]; then
    rm -f "$x11_log"
  fi
}

# アプリの出力の控えを用意し、片付けのトラップを張る。**起動の前に呼ぶ。**
x11_install_cleanup_trap() {
  x11_log=$(mktemp)
  x11_pid=""
  x11_died=0
  trap x11_cleanup EXIT INT TERM
}

# タイトルが一致して最小寸法を満たすウィンドウを**すべて**集める。
#
#   x11_collect_windows <タイトル部分文字列> <最小幅> <最小高さ>
#
# 結果は 4 つに入る:
#   - `x11_window_lines` : `xwininfo -root -tree` のタイトル一致行（フレームを除く。そのまま出す）
#   - `x11_window_ids`   : 一致したウィンドウの X の識別子（1 行に 1 つ。`0x…`）
#   - `x11_window_count` : その数（0 なら一致なし）
#   - `x11_window_match` : 最初の一致の寸法（`幅x高さ`）。空なら「一致なし」
#
# 0.1 秒ごとに呼び出し側が繰り返す（期限の単位は呼び出し側が決める）。
#
# **識別子を返すのは、複数のウィンドウを数えるだけでなく、同じウィンドウが残っていることを
# 特定するためである**（tasks.md 10.5 の「既存のウィンドウが閉じない」は識別子の集合で見る）。
x11_collect_windows() {
  _x11_title=$1
  _x11_min_w=$2
  _x11_min_h=$3
  # **再親付けを行うウィンドウマネージャは、アプリのウィンドウごとに同じ題名のフレーム窓を
  # 1 枚作る**（開発ホストの mutter は `WM_CLASS` が `mutter-x11-frames` の親窓を作る）。
  # フレームはアプリのウィンドウではないので数えない・閉じない。CI の Xvfb にはウィンドウ
  # マネージャが無いのでこの行は現れない（仮に現れても除外は無害である）。
  x11_window_lines=$(xwininfo -root -tree 2>/dev/null |
    grep -F "\"$_x11_title\"" |
    grep -F -v 'mutter-x11-frames' || true)
  x11_window_ids=$(printf '%s\n' "$x11_window_lines" |
    sed -n 's/^[[:space:]]*\(0x[0-9a-fA-F][0-9a-fA-F]*\).*[^0-9]\([0-9][0-9]*\)x\([0-9][0-9]*\)[+-].*/\1 \2 \3/p' |
    awk -v mw="$_x11_min_w" -v mh="$_x11_min_h" '$2 >= mw && $3 >= mh { print $1 }')
  x11_window_count=$(printf '%s\n' "$x11_window_ids" | awk 'NF { n += 1 } END { print n + 0 }')
  # 各行から "幅x高さ" を取り出し、最小寸法を満たすものが 1 つでもあれば成立とする
  # （GTK は 20x20 程度の補助ウィンドウも作るため、タイトル一致だけでは足りない）。
  x11_window_match=$(printf '%s\n' "$x11_window_lines" |
    sed -n 's/.*[^0-9]\([0-9][0-9]*\)x\([0-9][0-9]*\)[+-].*/\1x\2/p' |
    awk -v mw="$_x11_min_w" -v mh="$_x11_min_h" -F x '$1 >= mw && $2 >= mh { print; exit }')
}

# タイトルが一致して最小寸法を満たすウィンドウを 1 回だけ観測する。
#
# **実体は [`x11_collect_windows`] である**（複数ウィンドウを数える検査器と 1 枚だけを
# 見る検査器で、`xwininfo` の解析が 2 箇所に分かれないようにする）。
x11_observe_window() { x11_collect_windows "$@"; }

# 起動したプロセスの終了を観測しても即座に失敗とはしない（展開実行ではラッパーが本体より
# 先に終了する）。**注意を 1 回だけ出し、観測を続ける**（`x11_died`）。
x11_note_if_process_died() {
  if [ "${x11_died:-0}" = 0 ] && ! kill -0 "$x11_pid" 2>/dev/null; then
    echo "注意: 起動したプロセス（pid=${x11_pid}）が先に終了しました。ウィンドウの出現を待ち続けます" >&2
    x11_died=1
  fi
}

# ファイルの末尾を見出しつきで診断へ出す（失敗の理由を人の読める形で残す）。
x11_dump_tail() {
  echo "--- $1 ---" >&2
  if [ -f "$2" ]; then
    tail -n 40 "$2" >&2
  else
    echo "(出力なし)" >&2
  fi
}

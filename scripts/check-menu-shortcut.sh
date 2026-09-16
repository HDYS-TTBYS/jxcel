#!/bin/sh
# メニューとショートカットを X11 上で検査する（tasks.md 10.6 / 要件 3.2, 3.5, 3.6）。
#
# 使い方:
#   check-menu-shortcut.sh <配布物> <検証用の実行ファイル> <タイトル部分文字列> \
#                          [タイムアウト秒] [最小幅] [最小高さ] \
#                          <記録ファイル> <ドキュメント位置>
#
#   - 配布物                : AppImage など、**既定のビルド**（検証専用の項目が入らない形）
#   - 検証用の実行ファイル  : `--features verification-triggers` の**検証用の形**
#                             （7.5 の `verification.probe` と 10.6 の配置の記録を持つ唯一の形）
#   - タイトル部分文字列    : ウィンドウタイトルと **AT-SPI のアプリ名**（どちらも `jxcel`）
#   - タイムアウト秒        : 既定 60。ウィンドウの出現・AT-SPI の登録・2 つ目の起動を待つ上限
#   - 最小幅 / 最小高さ     : 既定 100。GTK の 20x20 程度の補助ウィンドウを数えないため
#   - 記録ファイル          : アプリの診断記録（4.4 の保存先の `jxcel.log`）。**通知の根拠**である
#   - ドキュメント位置      : 2 つ目の起動へ渡す引数（7.7 は位置を読まない。実在するだけでよい）
#
# # 検査する 3 つの主張
#
#   (1) **登録した項目がメニューに現れ、選択が登録元へ通知される** — 配布物を起動し、**AT-SPI** で
#       実アプリのアクセシビリティ木を読む（9.5 が実測した手段。`[menu bar] → [menu] ファイル /
#       診断 → [menu item] …`）。続けて `org.a11y.atspi.Action.DoAction` で 1 つの項目を
#       **実際に活性化**し、記録に現れる 2 行（`メニュー項目が選択された: 登録元=… 項目=…
#       対象ウィンドウ=…` と、登録元が送った `診断の導線の要求を送った: ウィンドウ = … /
#       導線 = …`）を要求する。**配布物に対して行う**ので、配布物のメニューそのものが読まれ、
#       駆動される。
#       あわせて**各項目のキーバインド**（基盤が表示に使う綴り。`<Primary>q` など）を読む。
#       **読むのは `GetKeyBinding` であり、`GetActions` ではない** — 理由は下の
#       「キーバインドは `GetKeyBinding` で読む」。
#       **読む項目の集合は下の `EXPECTED` が固定する**（`ファイル` の 4 件 — 「開く…」「終了」
#       「新規」「保存」— と `編集` の 3 件 — 8.7 の「複製」・**8.9 の「元に戻す」「やり直し」** —
#       と `診断` の 3 件。配布物・検証用の
#       形のそれぞれについて、`EXPECTED` の各項目が木に現れることを要求する）。「新規」「保存」はドキュメントのセッション
#       （タスク 3.6）が登録する項目であり、報告 `document-session.new` / `document-session.save`
#       として同じ名前空間の下に並ぶ。
#   (2) **フォーカスされているウィンドウにだけショートカットが作用する** — 検証用の形を起動し、
#       ドキュメント付きの 2 枚目を単一インスタンスの引き継ぎで開く。`XSetInputFocus` で
#       フォーカスを移し、`XSendEvent`（`event_mask=0`。7.5 の実測手段。**XTEST はこの
#       ツールキットに届かない**）で `Ctrl+Shift+J` を送り、7.5 の `verification.probe` が残す
#       `[検証] ショートカットが作用した対象ウィンドウ=…` が**フォーカス中のラベルと一致する**
#       ことを要求する。続けて**もう 1 枚へフォーカスを移して同じことを行い**、対象が追随する
#       こと（および 1 回の押下で作用するのは 1 枚だけであること）を見る。
#       **キーを実際に送って作用を要求するので、この段は「そのショートカットが基盤に登録され、
#       配信されること」の実測である**（表示の綴りそのものではない）。送るのは 7.5 の
#       `verification.probe` のショートカット（`Ctrl+Shift+J`）だけである — **8.9 の
#       `Ctrl+Z` / `Ctrl+Shift+Z` はこの段で送らない**: 作用を記録に残すのは検証用の項目だけで
#       あり、**グリッド画面を開いていないこの段では活性化しても観測できるものが無い**
#       （8.7 の起動観測が実測した。`design.md`「単体テストが観測しないもの（8.9）」）。
#   (3) **ショートカットの表示がプラットフォームの表記に従う** — 検証用の形が残す
#       `[検証] メニューを配置した: 配置=… 項目=…` の行を読み、**各部分メニューの位置・表示名・
#       基盤へ渡した綴り（正準形）・配置方式**を要求する。Linux / Windows の正準形は `ctrl+…`、
#       macOS は `super+…`（＝`Cmd`）であり、**同じ論理ショートカットがプラットフォームごとの
#       綴りへ解決されていること**がここに現れる。
#       **配置の記録は 12 項目すべてを要求する**（7.4 の「終了」・7.7 の「開く…」・9.5 の診断 3 件・
#       10.6 の検証専用 2 件・**8.7 の「複製」**・**8.9 の「元に戻す」「やり直し」**に、
#       **3.6 の「新規」「保存」**が加わった数である）。検証専用の
#       2 件は `--features verification-triggers` の形にだけ現れるので、配布物の起動（(1)）で
#       この行が現れないことも併せて要求する（下の「配布物の記録に検証専用の配置の行は現れない」）。
#
# # キーバインドは `GetKeyBinding` で読む（`GetActions` を呼んではならない）
#
# **`org.a11y.atspi.Action.GetActions` を呼んではならない。** 応答が `a(sss)` であるため、
# 基盤がその構造体の配列を組み立てる途中で**被検体のアプリが abort する**:
#
#   - CI が固定している**最も古い対象環境（ubuntu-22.04）の `libatk-bridge-2.0.so.0` は 2.38.0**
#     であり、その `impl_GetActions` は `a(sss)` の構造体へ**4 つ目の文字列**を書こうとする。
#     libdbus は型の不一致で `Array or variant type requires that type end_struct be written,
#     but string was written.` を出して **abort する** — つまり `GetActions` を 1 回呼ぶだけで
#     **被検体のアプリ自身が死ぬ**（呼び出し側からは「相手が返事の前に D-Bus から消えた」と
#     見える）。上流の修正（GNOME/at-spi2-core `dc0dc331`、2022-04-07）は 2.38 より後であり、
#     22.04 には入らない。**アプリのバグではなく、ランナーの a11y スタックの性質である**
#     （配布物が同梱する a11y スタックでも、ホストのものを読ませても 2.38 なので同じ）。
#   - **要るのはキーバインドの文字列 1 本だけ**である。同じ `org.a11y.atspi.Action` の
#     `GetKeyBinding(index) -> s` は**応答が文字列**なので構造体の組み立てを通らず、この abort を
#     踏まない。AT-SPI の仕様が「`GetActions` は各項目について `GetLocalizedName` /
#     `GetDescription` / `GetKeyBinding` を呼ぶのと等価」と定めている（`xml/Action.xml`）ので、
#     **読める内容は同じ**であり、この段が確かめる主張（項目ごとの表示の綴り）は変わらない。
#   - 項目ごとの有無は `Action` の `NActions` プロパティ（読取専用）で見る。**アクションを
#     持たない項目**（キーバインドを割り当てていない項目）はここで空を返し、(3) の「キーが無い」
#     対照が成立する。
#
# したがって表示の綴りの証拠は **(1) の基盤の報告（`<Primary>…`。GTK が表示に使う綴り）** と
# (3) の記録（アプリが基盤へ渡した解決済みの綴り `ctrl+…`）と (2) のキー送出（そのショートカットが
# 実際に作用する）の 3 つで取る。**ポップアップの字形そのものは見ていない**（7.5 がホストで
# 画素により実測済み。ランナーでは再現できない）。
#
# # 証拠が何を証明し、何を証明しないか
#
#   - (1) の「項目が現れる」は**実アプリのアクセシビリティ木**であり、GTK のメニューバーそのものを
#     読んでいる（1.5 / 10.4 がピクセル・ウィンドウで見ているのとは別の層）。**ただし
#     アクセシビリティの橋（atk-bridge）が読み込まれている必要がある** — 無ければ
#     AT-SPI のアプリ一覧に現れず、この検査は exit 1 で落ちる（黙って通らない）。
#   - (1) の通知は**アプリの記録**が出す。AT-SPI の活性化が届かなかった場合と、届いたが登録元が
#     動かなかった場合を区別できるよう、**AT-SPI の読み（項目の存在）と記録の 2 行を両方**要求する。
#   - (2) の対象ウィンドウは**アプリが自分で解決した値**（活性化時点のフォーカス。7.5 の
#     `activation_target`）である。「作用した」ことの直接の証拠はログの 1 行であり、
#     どのウィンドウのウィジェットが反応したかは見ていない。**それでも「フォーカスと一致する
#     こと」「フォーカスを移すと追随すること」は、この 1 行の比較で成立する。**
#   - (1) のキーバインドは**基盤（GTK / ATK）が報告する綴り**であり、アプリの自己申告ではない。
#     **アクセラレータを割り当てていない項目が空を報告すること**（対照）も同じ経路で確かめる。
#   - (3) の「表示」は**基盤へ渡した綴りまで**である。GTK が描く字形（`Ctrl+Shift+L`）そのものは
#     アプリの記録には現れない（7.5 がポップアップの画素で確かめた）。
#   - **配布物（既定のビルド）は検証専用の引き金を読まない**（9.7 の片付けの規約）ので、
#     (1) は配布物で、(2)(3) は検証用の形で測る。段はその 2 つを別の引数で受け取る。
#     配布物の記録に検証専用の行が 1 つも現れないことも (1) で確かめる。
#   - **AT-SPI の呼び出しが失敗したら、アプリの出力と記録を添えて落ちる**（`atspi` の wrapper）。
#     読み取りの失敗の原因（アプリが消えた・橋が落ちた・D-Bus が拒否した）は、被検体の出力に
#     しか現れないことがある — 実際、`GetActions` の abort（下の「キーバインドは
#     `GetKeyBinding` で読む」）はこの wrapper が無い間は見えなかった。
#
# # 記録の読み方
#
# 記録は**起動の前に消さない**（起動中のアプリは開いたファイルへ書き続けるため、消すと行が
# 失われる）。代わりに**各段の開始時の行数**を取り、それ以降の行だけを調べる（前の段や
# 前の CI 段の行で偽の成功をしない）。
#
# # 前提
#   - DISPLAY が設定されていること。CI（Linux ランナー）では `xvfb-run` が設定する。
#   - `xwininfo`（x11-utils）、`python3`（標準ライブラリのみ。libX11 を ctypes で呼ぶ）、
#     `busctl`（systemd。AT-SPI の D-Bus 呼び出しに使う。**`gir1.2-atspi` は要求しない**）。
#   - **アクセシビリティの橋**が必要である（`GTK_MODULES=gail:atk-bridge`。この検査は自分で
#     設定する）。at-spi2-core は GTK の依存（推奨）として入る。
#
# # 終了コード
#   0 = 3 つの主張すべてが期待どおり / 1 = 検査失敗 / 2 = 入力が使えない
#
# SC1091 / SC2154: 置き場は**同じリポジトリのファイル**であり、`x11_log` / `x11_poll_sleep` /
# `x11_window_ids` などはそこで代入される。qlty は検査対象を一時ディレクトリへ写してから静的解析に
# かけるため、置き場をたどれず「たどれない・未代入」と報告する
# （実際の実行では `$0` からの相対で解決する）。契約は置き場の doc に 1 つだけ書いてある。
# shellcheck disable=SC1091,SC2154
set -eu

_x11_lib_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
. "$_x11_lib_dir/lib/x11-window.sh"

usage() {
  echo "使い方: check-menu-shortcut.sh <配布物> <検証用の実行ファイル> <タイトル部分文字列> [タイムアウト秒] [最小幅] [最小高さ] <記録ファイル> <ドキュメント位置>" >&2
  exit 2
}

[ "$#" -ge 8 ] || usage

app=$1
verify_app=$2
title=$3
timeout_secs=${4:-60}
min_w=${5:-100}
min_h=${6:-100}
record=$7
document=$8

case "$timeout_secs" in
  ''|*[!0-9]*)
    echo "NG: タイムアウト秒が整数ではありません: $timeout_secs" >&2
    exit 2
    ;;
esac

if [ -z "$record" ]; then
  echo "NG: 記録ファイルが指定されていません" >&2
  exit 2
fi
if [ ! -f "$document" ]; then
  echo "NG: ドキュメント位置が実在しません: ${document}（7.7 は読まないが、渡す位置は実在させる）" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "NG: python3 が見つかりません（AT-SPI の読みと X11 の駆動に使います）" >&2
  exit 2
fi
if ! command -v busctl >/dev/null 2>&1; then
  echo "NG: busctl が見つかりません（AT-SPI の D-Bus 呼び出しに使います。systemd に同梱されます）" >&2
  exit 2
fi

x11_require_app "$app"
x11_require_app "$verify_app"
x11_require_environment

# **検証用の引き金が親の環境から漏れないようにする。** 配布物は読まないが、検証用の形が親の
# 環境変数（初期画面・明示終了など）に引きずられると、この検査の前提（起動時のウィンドウ）が
# 崩れる。
unset JXCEL_VERIFICATION_DENY_CLOSE 2>/dev/null || true
unset JXCEL_VERIFICATION_INITIAL_SCREEN 2>/dev/null || true
unset JXCEL_VERIFICATION_EXIT_AFTER_MS 2>/dev/null || true

# **アクセシビリティの橋を明示的に読み込む。** CI のランナーは Xsession を通らないので
# `GTK_MODULES` が設定されておらず、GTK は AT-SPI へ登録しない（ローカルの実画面セッションでは
# 設定済みである）。
GTK_MODULES=${GTK_MODULES:-gail:atk-bridge}
export GTK_MODULES
unset NO_AT_BRIDGE 2>/dev/null || true
export GDK_BACKEND=x11

x11_install_cleanup_trap
x11_pick_poll_sleep
work=$(mktemp -d)
launched_pids=""
cleaned=0
exit_status=0
# 記録に求める行の印。**`grep -E` に渡すので角括弧は退避する**（`[検証]` のままだと文字クラスに
# なって一致しない）。
probe_marker='\[検証\] ショートカットが作用した対象ウィンドウ='
placement_marker='\[検証\] メニューを配置した: '

# AT-SPI の見出し（アプリ名と待ち時間）。**X11 のウィンドウ出現より登録が遅れうる**ので、
# 読み手の側で待つ。
JXCEL_ATSPI_APP=$title
JXCEL_ATSPI_TIMEOUT=$timeout_secs
export JXCEL_ATSPI_APP JXCEL_ATSPI_TIMEOUT

# 起動したもの（この段の主役以外も含む）を必ず片付ける。**主役は `x11_pid`**（置き場のトラップが
# 木ごと終了し、出力の控えを消す）。2 つ目の起動のように自分で終わるものも、期限までに
# 終わらなければここで終了する。
# shellcheck disable=SC2329 # 下の trap から呼ばれる
cleanup() {
  [ "$cleaned" = 1 ] && return 0
  cleaned=1
  for _p in $launched_pids; do
    if kill -0 "$_p" 2>/dev/null; then
      x11_kill_tree "$_p"
      _n=0
      while [ "$_n" -lt 10 ] && kill -0 "$_p" 2>/dev/null; do
        sleep 0.5
        _n=$((_n + 1))
      done
      kill -9 "$_p" 2>/dev/null || true
      wait "$_p" 2>/dev/null || true
    fi
  done
  x11_cleanup
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

report_failure() {
  echo "NG: $1" >&2
  x11_dump_tail "アプリの出力（末尾）" "${x11_log:-}"
  if [ -f "$record" ]; then
    echo "--- 診断記録（末尾）: $record ---" >&2
    tail -n 40 "$record" >&2
  else
    echo "--- 診断記録: ${record}（存在しません） ---" >&2
  fi
  exit 1
}

# 記録の現在の行数（無ければ 0）。各段の開始時に取り、それ以降の行だけを調べる。
record_lines() {
  if [ -f "$record" ]; then
    wc -l < "$record" | tr -d ' '
  else
    echo 0
  fi
}

# 記録の `<開始行数>` より後から正規表現に一致する最初の行を出す（無ければ空で終了 1）。
record_find() {
  _from=$1
  _pattern=$2
  [ -f "$record" ] || return 1
  tail -n "+$((_from + 1))" "$record" 2>/dev/null | grep -E "$_pattern" | head -n 1
}

# 記録の `<開始行数>` より後から正規表現に一致する**最後の**行を出す（無ければ空で終了 1）。
# 配置の記録は登録のたびに伸びるので、最後の行が最も多くの項目を運ぶ。
record_last() {
  _from=$1
  _pattern=$2
  [ -f "$record" ] || return 1
  tail -n "+$((_from + 1))" "$record" 2>/dev/null | grep -E "$_pattern" | tail -n 1
}

# 記録の `<開始行数>` より後に正規表現が現れた回数。
#
# **`awk -v` を使わない。** `awk -v p="…"` は**代入の値のバックスラッシュを解釈してしまう**ため、
# 呼び出し側が渡す退避つきの角括弧（`probe_marker` の `\[検証\]`。`grep -E` に渡す前提）が
# 素の `[検証]`（＝文字クラス）になり、**一致件数が常に 0 になる**（実測: 2026-09-12 の Linux の
# ランナー。`awk: warning: escape sequence '\[' treated as plain '['` を出したうえで
# 「2 枚目にフォーカスしたときの作用が 1 回ではありません（0 回）」と偽の失敗になった）。
# `grep -E` は他の 2 つ（`record_find` / `record_last`）と同じ解釈であり、件数もそのまま数える。
record_count() {
  _from=$1
  _pattern=$2
  if [ ! -f "$record" ]; then
    echo 0
    return 0
  fi
  # 一致が無ければ `grep -c` は 0 を出して非 0 終了する（`set -e` の下でも落ちないように受ける）。
  tail -n "+$((_from + 1))" "$record" 2>/dev/null | grep -c -E "$_pattern" || true
}

# 記録に一致行が現れるのを待つ（最大 <秒>）。現れれば行を出して終了 0。
wait_record() {
  _from=$1
  _pattern=$2
  _seconds=$3
  _deadline=$(( $(date +%s) + _seconds ))
  while :; do
    _line=$(record_find "$_from" "$_pattern" || true)
    if [ -n "$_line" ]; then
      printf '%s\n' "$_line"
      return 0
    fi
    if [ "$(date +%s)" -ge "$_deadline" ]; then
      return 1
    fi
    sleep "$x11_poll_sleep"
  done
}

# タイトルが一致して最小寸法を満たすウィンドウを観測する（置き場の実装）。
observe_windows() { x11_collect_windows "$title" "$min_w" "$min_h"; }

# **ウィンドウの集合が落ち着くまで待つ。** このホストの mutter は起動直後にウィンドウを作り直す
# （既定寸法の toplevel が一瞬現れ、最大化されたものに置き換わる）。1 回の観測で得た識別子は
# **既に消えているもの**を含みうるので、1 秒間隔で 2 回続けて同じ集合が観測できたときに
# 「落ち着いた」と見なす。期限を超えたら終了 1。
stable_wait() {
  _deadline=$(( $(date +%s) + $1 ))
  _prev=""
  while :; do
    observe_windows
    if [ "$x11_window_count" -ge 1 ]; then
      if [ -n "$_prev" ] && [ "$x11_window_ids" = "$_prev" ]; then
        return 0
      fi
      _prev=$x11_window_ids
    fi
    if [ "$(date +%s)" -ge "$_deadline" ]; then
      return 1
    fi
    sleep 1
  done
}

# ウィンドウ数が <期待する数> 以上になるのを待つ。期限を超えたら終了 1。
wait_for_windows() {
  _expected=$1
  _deadline=$(( $(date +%s) + $2 ))
  while :; do
    observe_windows
    if [ "$x11_window_count" -ge "$_expected" ]; then
      return 0
    fi
    if [ "$(date +%s)" -ge "$_deadline" ]; then
      return 1
    fi
    sleep "$x11_poll_sleep"
  done
}

# ウィンドウが 1 枚も無くなるのを待つ。期限を超えたら終了 1。
wait_for_no_windows() {
  _deadline=$(( $(date +%s) + $1 ))
  while :; do
    observe_windows
    if [ "$x11_window_count" -eq 0 ]; then
      return 0
    fi
    if [ "$(date +%s)" -ge "$_deadline" ]; then
      return 1
    fi
    sleep "$x11_poll_sleep"
  done
}

# プロセスが生きているか。**ゾンビは「生きていない」**（終了はしているが回収待ち）。
process_alive() {
  _state=$(sed -n 's/^[^)]*) \([A-Z]\).*/\1/p' "/proc/$1/stat" 2>/dev/null || true)
  [ -n "$_state" ] && [ "$_state" != "Z" ]
}

# プロセスの終了を待ち、**回収して**終了コードを `exit_status` に入れる（期限を超えたら終了 1）。
wait_for_exit() {
  _deadline=$(( $(date +%s) + $2 ))
  while process_alive "$1"; do
    if [ "$(date +%s)" -ge "$_deadline" ]; then
      return 1
    fi
    sleep "$x11_poll_sleep"
  done
  exit_status=0
  wait "$1" || exit_status=$?
  return 0
}

# 起動中のアプリを止め、ウィンドウが消えるまで待つ。**次の段は同じアプリをもう一度起動するので、
# 残すと単一インスタンスの機構に引き継がれてウィンドウが出ない**（偽の失敗になる）。
stop_app() {
  if [ -n "${x11_pid:-}" ] && kill -0 "$x11_pid" 2>/dev/null; then
    x11_kill_tree "$x11_pid"
    _n=0
    while [ "$_n" -lt 20 ] && process_alive "$x11_pid"; do
      sleep 0.5
      _n=$((_n + 1))
    done
    if process_alive "$x11_pid"; then
      kill -9 "$x11_pid" 2>/dev/null || true
    fi
    wait "$x11_pid" 2>/dev/null || true
  fi
  x11_pid=""
  if ! wait_for_no_windows "$timeout_secs"; then
    report_failure "アプリを止めた後にウィンドウが ${timeout_secs} 秒以内に消えませんでした（次の段が同じアプリを起動できません）"
  fi
}

# メニューの観測・活性化と X11 の駆動（標準ライブラリの python3 だけを使う）。
#
# モード:
#   atspi-tree                 AT-SPI のメニュー木を出す（証拠。判定はしない）
#   atspi-verify-shipping      配布物の期待（出荷項目が現れ、検証専用の項目が現れないこと）
#   atspi-verify-verification  検証用の形の期待（検証専用の項目も現れること）
#   atspi-activate <表示名>    項目を 1 つ活性化する（`DoAction`）
#   x11-focus <id>             入力フォーカスをそのウィンドウへ移す（移した結果を出す）
#   x11-shortcut <id>          そのウィンドウへ `Ctrl+Shift+J` を送る
#
# **`gir1.2-atspi`（PyGObject の AT-SPI 束縛）を要求しない。** ランナーには無いため、
# アクセシビリティの木は `busctl` で D-Bus を直接叩いて読む（`--json=short` の出力を解析する）。
atspi_run() {
  python3 - "$@" <<'PY'
import ctypes
import json
import os
import subprocess
import sys
import time

ACCESSIBLE = "org.a11y.atspi.Accessible"
ACTION = "org.a11y.atspi.Action"
REGISTRY = "org.a11y.atspi.Registry"
ROOT_PATH = "/org/a11y/atspi/accessible/root"
# 読む深さと、辿らないロール。**メニューバーは frame の 2 階層下**にあり、webview の
# アクセシビリティ木（巨大になりうる）は `scroll pane` / `document web` の下にある。
MAX_DEPTH = 6
PRUNE_ROLES = {"scroll pane", "document web", "document frame", "table", "text"}

EXPECTED = {
    "shipping": {
        "ファイル": {
            "開く…": (("primary",), "o"),
            "終了": (("primary",), "q"),
            # ドキュメントのセッション（タスク 3.6。要件 5.1、7.1）。**配布物にも現れる**
            # （既定のビルドに入るのは「開く…」「終了」と同じ扱いである）。
            "新規": (("primary",), "n"),
            "保存": (("primary",), "s"),
        },
        # 8.7 が足した「編集 > 複製」（要件 7.8）。**配布物にも現れる**（打鍵と同じ入口へ着く）。
        # 8.9 が足した「編集 > 元に戻す / やり直し」（要件 9.9）。**キーボードの経路は
        # アクセラレータそのものである**（画面は打鍵を聴かない。非 macOS は `Ctrl+Z` /
        # `Ctrl+Shift+Z`、macOS は `Cmd+Z` / `Cmd+Shift+Z`）。
        "編集": {
            "複製": (("primary",), "c"),
            "元に戻す": (("primary",), "z"),
            "やり直し": (("primary", "shift"), "z"),
        },
        "診断": {
            "診断情報を書き出す…": (("primary", "shift"), "e"),
            "記録の保存場所を表示": (("primary", "shift"), "l"),
            "記録の詳細度…": (("primary", "shift"), "v"),
        },
    },
    "verification": {
        "ファイル": {
            "開く…": (("primary",), "o"),
            "終了": (("primary",), "q"),
            "検証: 対象ウィンドウを記録": (("primary", "shift"), "j"),
            "検証: ドキュメント付きのみ": ((), None),
            # 検証用の形でも配布物の項目はそのまま残る（検証専用の項目が足されるだけである）。
            "新規": (("primary",), "n"),
            "保存": (("primary",), "s"),
        },
        "編集": {
            "複製": (("primary",), "c"),
            "元に戻す": (("primary",), "z"),
            "やり直し": (("primary", "shift"), "z"),
        },
        "診断": {
            "診断情報を書き出す…": (("primary", "shift"), "e"),
            "記録の保存場所を表示": (("primary", "shift"), "l"),
            "記録の詳細度…": (("primary", "shift"), "v"),
        },
    },
}
# 配布物（既定のビルド）に現れてはならない検証専用の項目（7.4 / 7.5 の片付けの規約）。
ABSENT_IN_SHIPPING = ("検証: 対象ウィンドウを記録", "検証: ドキュメント付きのみ")
# 診断の部分メニューの項目数（9.5 の契約）。
DIAGNOSTICS_ITEMS = 3


def ng(message):
    print(f"NG: {message}", file=sys.stderr)
    raise SystemExit(1)


def run(command, timeout=20):
    result = subprocess.run(command, capture_output=True, text=True, timeout=timeout)
    if result.returncode != 0:
        raise RuntimeError(f"{' '.join(command)}\n{result.stderr.strip()}")
    return result.stdout


class Atspi:
    """`busctl` だけで読む AT-SPI（必要な範囲に限った薄い実装）。"""

    def __init__(self):
        out = run([
            "busctl", "--user", "call", "org.a11y.Bus", "/org/a11y/bus",
            "org.a11y.Bus", "GetAddress",
        ])
        if out.lstrip().startswith("{"):
            self.address = json.loads(out)["data"][0]
        else:
            self.address = out.strip().split('"')[1]
        self.calls = 0

    def _json(self, *args):
        self.calls += 1
        return json.loads(run(["busctl", f"--address={self.address}", "--json=short", *args]))

    def call(self, dest, path, interface, method, *args):
        data = self._json("call", dest, path, interface, method, *args)["data"]
        if isinstance(data, list) and len(data) == 1:
            return data[0]
        return data

    def property(self, dest, path, interface, name):
        # `get-property` の `data` は値そのもの（`call` と形が違う）。
        return self._json("get-property", dest, path, interface, name)["data"]

    def children(self, dest, path):
        return [(name, child) for name, child in self.call(dest, path, ACCESSIBLE, "GetChildren")]

    def role(self, dest, path):
        return self.call(dest, path, ACCESSIBLE, "GetRoleName")

    def name(self, dest, path):
        return self.property(dest, path, ACCESSIBLE, "Name")

    def keybinding(self, dest, path):
        """その項目のキーバインド（**基盤が表示に使う綴り**。`<Primary>q` など）。

        **`GetActions` を呼んではならない。** 応答が `a(sss)` であるため、基盤がその構造体の
        配列を組み立てる途中で**被検体のアプリが abort する**（実測: libdbus の
        `Array or variant type requires that type end_struct be written, but string was
        written.` のあと、呼び出し側には
        `Message recipient disconnected from message bus without replying` が返る）。

        ここで要るのは**キーバインドの文字列 1 本**だけなので、応答が `s` である
        `GetKeyBinding` で読む（構造体の組み立てを通らない）。AT-SPI の仕様が「`GetActions` は
        各項目について `GetLocalizedName` / `GetDescription` / `GetKeyBinding` を呼ぶのと
        等価」と定めている（`xml/Action.xml`）ので、**読める内容は同じ**である。
        """
        # アクションを持たない項目（キーバインドを割り当てていない項目）は空を返す。
        if not self.property(dest, path, ACTION, "NActions"):
            return ""
        binding = self.call(dest, path, ACTION, "GetKeyBinding", "i", "0")
        # 基盤は `ニーモニック;列;ショートカット` の形で返す（`xml/Action.xml` の
        # `GetKeyBinding`。GTK のメニュー項目では最後の欄が表示される綴りである）。
        return str(binding).split(";")[-1].strip()

    def do_action(self, dest, path, index=0):
        return self.call(dest, path, ACTION, "DoAction", "i", str(index))


def find_app(atspi, app_name, timeout):
    """AT-SPI に現れたアプリのルートを待つ（登録はウィンドウの生成より遅れうる）。"""
    deadline = time.monotonic() + timeout
    while True:
        for name, path in atspi.children(REGISTRY, ROOT_PATH):
            try:
                if atspi.name(name, path) == app_name:
                    return (name, path)
            except RuntimeError:
                continue
        if time.monotonic() >= deadline:
            return None
        time.sleep(0.5)


def walk(atspi, app):
    """`menu bar` / `menu` / `menu item` だけを集める（深さとロールで枝を刈る）。"""
    stack = [(app[0], app[1], 0)]
    nodes = []
    while stack:
        dest, path, depth = stack.pop()
        if depth > MAX_DEPTH:
            continue
        try:
            role = atspi.role(dest, path)
        except RuntimeError as error:
            ng(f"AT-SPI のロールを読めませんでした（{dest} {path}）: {error}")
        entry = {"role": role, "name": "", "keybinding": "", "depth": depth}
        if role in ("menu bar", "menu", "menu item"):
            try:
                entry["name"] = atspi.name(dest, path)
                # **キーバインドを読むのは `menu item` だけである**（`menu bar` と `menu` には
                # アクションが無く、`NActions` が 0 を返す）。
                if role == "menu item":
                    entry["keybinding"] = atspi.keybinding(dest, path)
            except RuntimeError as error:
                ng(f"AT-SPI の項目を読めませんでした（{dest} {path}）: {error}")
            nodes.append(entry)
        if role in PRUNE_ROLES:
            continue
        try:
            children = atspi.children(dest, path)
        except RuntimeError:
            continue
        for child_name, child_path in children:
            stack.append((child_name, child_path, depth + 1))
    return nodes


def menus(nodes):
    """`menu bar` の直下の `menu` ごとに、項目名 → キーバインド を集める。"""
    found = {}
    bar_depth = None
    for node in nodes:
        if node["role"] == "menu bar":
            bar_depth = node["depth"]
            break
    if bar_depth is None:
        return found
    current = None
    for node in nodes:
        if node["role"] == "menu" and node["depth"] == bar_depth + 1:
            current = node["name"]
            found.setdefault(current, {})
        elif node["role"] == "menu item" and current is not None and node["depth"] == bar_depth + 2:
            found[current][node["name"]] = node["keybinding"]
    return found


def normalize(binding):
    """GTK の報告を比較できる形にする（`<Primary>` を小文字の `primary` に、他も小文字に）。"""
    return binding.lower()


def check_binding(item, binding, modifiers, key):
    """**基盤が報告するキーバインド**が期待どおりかを確かめ、記録に出す 1 行を返す。"""
    text = normalize(binding)
    if key is None:
        # キーバインドを割り当てていない項目の対照。**空であること**を要求する。
        if text != "":
            ng(f"AT-SPI: {item} にキーバインドがあってはならないが {binding!r} が付いています")
        return f"{item}=（キーバインドなし）"
    if not text.endswith(key):
        ng(f"AT-SPI: {item} のキーバインド {binding!r} がキー {key!r} で終わっていません")
    for modifier in modifiers:
        # `<Primary>` は GTK の「主修飾キー」表記（Linux / Windows は Ctrl、macOS は Command）。
        # ランナーの GTK 版によって `<Control>` と綴られることがあるため両方を受ける。
        if modifier == "primary":
            if "primary" not in text and "control" not in text and "meta" not in text:
                ng(f"AT-SPI: {item} のキーバインド {binding!r} に主修飾キーがありません")
        elif modifier not in text:
            ng(f"AT-SPI: {item} のキーバインド {binding!r} に {modifier} がありません")
    return f"{item}={binding}"


def verify(mode, app_name, timeout):
    atspi = Atspi()
    app = find_app(atspi, app_name, timeout)
    if app is None:
        ng(f"AT-SPI にアプリ {app_name!r} が現れませんでした（{timeout} 秒。アクセシビリティの橋が読み込まれていない可能性があります）")
    nodes = walk(atspi, app)
    if not any(node["role"] == "menu bar" for node in nodes):
        ng(f"AT-SPI の木にメニューバーがありません（アプリ {app_name!r} の木を {atspi.calls} 回の呼び出しで読みました）")
    found = menus(nodes)
    expected = EXPECTED[mode]
    for menu, items in expected.items():
        if menu not in found:
            ng(f"AT-SPI: 部分メニュー {menu!r} がメニューバーにありません（あるのは {sorted(found)}）")
        for item, (modifiers, key) in items.items():
            if item not in found[menu]:
                ng(f"AT-SPI: {menu!r} に項目 {item!r} がありません（あるのは {sorted(found[menu])}）")
            print("OK: AT-SPI: " + check_binding(f"{menu} > {item}", found[menu][item], modifiers, key))
    if found.get("診断") is not None and len(found["診断"]) != DIAGNOSTICS_ITEMS:
        ng(f"AT-SPI: 診断の部分メニューの項目が {len(found['診断'])} 個です（{DIAGNOSTICS_ITEMS} 個であるべき）")
    if mode == "shipping":
        for label in ABSENT_IN_SHIPPING:
            for menu, items in found.items():
                if label in items:
                    ng(f"AT-SPI: 配布物の {menu!r} に検証専用の項目 {label!r} が現れています（既定のビルドに入ってはならない）")
        print(f"OK: AT-SPI: 配布物に検証専用の項目は現れない（{len(found)} 個の部分メニューを読んだ）")
    print(f"OK: AT-SPI: {app_name} の木を {atspi.calls} 回の呼び出しで読んだ")
    return 0


def tree(app_name, timeout):
    atspi = Atspi()
    app = find_app(atspi, app_name, timeout)
    if app is None:
        ng(f"AT-SPI にアプリ {app_name!r} が現れませんでした（{timeout} 秒）")
    for node in walk(atspi, app):
        indent = "  " * node["depth"]
        if node["role"] == "menu item":
            print(f"{indent}{node['role']} | {node['name']!r} | {node['keybinding']!r}")
        else:
            print(f"{indent}{node['role']} | {node['name']!r}")
    return 0


def activate(label, app_name, timeout):
    atspi = Atspi()
    app = find_app(atspi, app_name, timeout)
    if app is None:
        ng(f"AT-SPI にアプリ {app_name!r} が現れませんでした（{timeout} 秒）")
    stack = [(app[0], app[1], 0)]
    while stack:
        dest, path, depth = stack.pop()
        if depth > MAX_DEPTH:
            continue
        role = atspi.role(dest, path)
        if role == "menu item" and atspi.name(dest, path) == label:
            if not atspi.do_action(dest, path):
                ng(f"AT-SPI: {label!r} の活性化が拒否されました")
            print(f"OK: AT-SPI: {label!r} を活性化した（org.a11y.atspi.Action.DoAction）")
            return 0
        if role in PRUNE_ROLES:
            continue
        for child_name, child_path in atspi.children(dest, path):
            stack.append((child_name, child_path, depth + 1))
    ng(f"AT-SPI: 活性化する項目 {label!r} が木にありません")


# ---------------------------------------------------------------------------
# X11（フォーカスの移動とキーの送信）
# ---------------------------------------------------------------------------


class XKeyEvent(ctypes.Structure):
    _fields_ = [
        ("type", ctypes.c_int),
        ("serial", ctypes.c_ulong),
        ("send_event", ctypes.c_int),
        ("display", ctypes.c_void_p),
        ("window", ctypes.c_ulong),
        ("root", ctypes.c_ulong),
        ("subwindow", ctypes.c_ulong),
        ("time", ctypes.c_ulong),
        ("x", ctypes.c_int),
        ("y", ctypes.c_int),
        ("x_root", ctypes.c_int),
        ("y_root", ctypes.c_int),
        ("state", ctypes.c_uint),
        ("keycode", ctypes.c_uint),
        ("same_screen", ctypes.c_int),
    ]


class XEvent(ctypes.Union):
    # XEvent の大きさ（64 ビットでは 192 バイト = long 24 個）に合わせる。
    _fields_ = [("type", ctypes.c_int), ("xkey", XKeyEvent), ("pad", ctypes.c_long * 24)]


def x11(mode, *args):
    x11_lib = ctypes.CDLL("libX11.so.6")
    x11_lib.XOpenDisplay.restype = ctypes.c_void_p
    x11_lib.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11_lib.XStringToKeysym.restype = ctypes.c_ulong
    x11_lib.XStringToKeysym.argtypes = [ctypes.c_char_p]
    x11_lib.XKeysymToKeycode.restype = ctypes.c_ubyte
    x11_lib.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    x11_lib.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
    x11_lib.XGetInputFocus.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int)]
    x11_lib.XSendEvent.restype = ctypes.c_int
    x11_lib.XSendEvent.argtypes = [
        ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(XEvent),
    ]
    x11_lib.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
    x11_lib.XFlush.argtypes = [ctypes.c_void_p]
    x11_lib.XCloseDisplay.argtypes = [ctypes.c_void_p]

    # **X のエラーを既定のハンドラに任せない。** 既定はプロセスを即座に終了させるので、
    # 消えたウィンドウへ送った場合（`BadWindow`）に「なぜ落ちたか」の説明が残らない。
    # エラーを捕まえ、観測（`XGetInputFocus` の一致）と合わせて診断できるようにする。
    errors = []
    handler_type = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)

    def on_error(_display, event):
        errors.append(event)
        return 0

    handler = handler_type(on_error)
    x11_lib.XSetErrorHandler.restype = ctypes.c_void_p
    x11_lib.XSetErrorHandler.argtypes = [handler_type]
    x11_lib.XSetErrorHandler(handler)

    display = x11_lib.XOpenDisplay(None)
    if not display:
        ng("DISPLAY を開けません")
    try:
        if mode == "focus":
            window = int(args[0], 0)
            # RevertToParent(2) / CurrentTime(0)
            x11_lib.XSetInputFocus(display, window, 2, 0)
            x11_lib.XSync(display, 0)
            if errors:
                ng(f"XSetInputFocus が拒否されました: window=0x{window:x}（ウィンドウが既に無い）")
            focused = ctypes.c_ulong(0)
            revert = ctypes.c_int(0)
            x11_lib.XGetInputFocus(display, ctypes.byref(focused), ctypes.byref(revert))
            if focused.value != window:
                ng(f"XSetInputFocus の後にフォーカスが 0x{focused.value:x} です（期待 0x{window:x}）")
            print(f"OK: X11: 入力フォーカスを 0x{window:x} へ移した（XGetInputFocus で確認）")
            return 0
        if mode == "shortcut":
            window = int(args[0], 0)
            event = XEvent()
            event.xkey.type = 2  # KeyPress
            event.xkey.serial = 0
            event.xkey.send_event = 1
            event.xkey.display = display
            event.xkey.window = window
            event.xkey.root = 0
            event.xkey.subwindow = 0
            event.xkey.time = 0
            event.xkey.x = 0
            event.xkey.y = 0
            event.xkey.x_root = 0
            event.xkey.y_root = 0
            # ControlMask(1<<2) | ShiftMask(1<<0)。`event_mask=0` で**そのウィンドウを作った
            # クライアント**へ届く（7.5 の実測。XTEST はこのツールキットに届かない）。
            event.xkey.state = (1 << 2) | 1
            event.xkey.keycode = x11_lib.XKeysymToKeycode(
                display, x11_lib.XStringToKeysym(b"j"),
            )
            event.xkey.same_screen = 1
            if not x11_lib.XSendEvent(display, window, 0, 0, ctypes.byref(event)):
                ng(f"XSendEvent に失敗しました: window=0x{window:x}")
            release = XEvent()
            ctypes.memmove(ctypes.byref(release), ctypes.byref(event), ctypes.sizeof(XEvent))
            release.xkey.type = 3  # KeyRelease
            x11_lib.XSendEvent(display, window, 0, 0, ctypes.byref(release))
            x11_lib.XSync(display, 0)
            if errors:
                ng(f"XSendEvent が拒否されました: window=0x{window:x}（ウィンドウが既に無い）")
            print(f"OK: X11: 0x{window:x} へ Ctrl+Shift+J を送った（keycode={event.xkey.keycode}）")
            return 0
        ng(f"未知のモードです: {mode}")
    finally:
        x11_lib.XCloseDisplay(display)
    return 0


def main():
    if len(sys.argv) < 2:
        ng("モードが指定されていません")
    mode = sys.argv[1]
    app_name = os.environ.get("JXCEL_ATSPI_APP", "jxcel")
    timeout = float(os.environ.get("JXCEL_ATSPI_TIMEOUT", "30"))
    if mode == "atspi-tree":
        return tree(app_name, timeout)
    if mode in ("atspi-verify-shipping", "atspi-verify-verification"):
        return verify(mode[len("atspi-verify-"):], app_name, timeout)
    if mode == "atspi-activate":
        if len(sys.argv) < 3:
            ng("活性化する項目の表示名がありません")
        return activate(sys.argv[2], app_name, timeout)
    if mode in ("x11-focus", "x11-shortcut"):
        if len(sys.argv) < 3:
            ng("対象のウィンドウがありません")
        return x11(mode[len("x11-"):], *sys.argv[2:])
    ng(f"未知のモードです: {mode}")


if __name__ == "__main__":
    raise SystemExit(main())
PY
}

# AT-SPI の呼び出し。**失敗したら、アプリの出力と記録を添えて落ちる。** 読み取りの失敗の原因
# （アプリが消えた・アクセシビリティの橋が落ちた・D-Bus が呼び出しを拒否した）は被検体の出力に
# しか現れないことがある — 実際、22.04 の libatk-bridge の `GetActions` がアプリを abort させて
# いたことは、この wrapper（とその wrapper が呼ぶ `report_failure`）を足すまで見えなかった
# （ファイル冒頭の「キーバインドは `GetKeyBinding` で読む」を参照）。
atspi() {
  atspi_run "$@" || report_failure "AT-SPI の呼び出しが失敗した（${*}）"
}

echo "診断記録: $record"
echo "配布物: $app"
echo "検証用の形: $verify_app"
echo "ドキュメント位置: $document"

# **検証の前に、同じ題名のウィンドウが 1 枚も無いこと。** 残っていると (a) 単一インスタンスの
# 機構に引き継がれて新しいウィンドウが出ない、(b) 古い識別子へフォーカスやキーを送って
# しまう（偽の失敗）。1.5 / 10.3 / 10.4 / 10.5 と同じ前提をここでも明示する。
observe_windows
if [ "$x11_window_count" -ne 0 ]; then
  echo "残っているウィンドウ:" >&2
  printf '%s\n' "$x11_window_lines" >&2
  report_failure "検証の前に '${title}' のウィンドウが ${x11_window_count} 枚あります（前の実行の残り。単一インスタンスの機構に引き継がれます）"
fi
echo "前提: '${title}' のウィンドウは検証の前に 1 枚も無い"

# ---------------------------------------------------------------------------
# (1) 配布物: メニュー項目の出現・選択の通知（表示の綴りは (3) が担う）
# ---------------------------------------------------------------------------

phase_shipping=$(record_lines)

echo "検証 1/3: 配布物を起動し、AT-SPI でメニュー木を読む（項目の出現）"
nohup "$app" >"$x11_log" 2>&1 &
x11_pid=$!
launched_pids="$launched_pids $x11_pid"

if ! wait_for_windows 1 "$timeout_secs"; then
  report_failure "配布物のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
fi
if ! stable_wait "$timeout_secs"; then
  report_failure "配布物のウィンドウの集合が ${timeout_secs} 秒以内に落ち着きませんでした"
fi
if [ "$x11_window_count" -ne 1 ]; then
  printf '%s\n' "$x11_window_lines" >&2
  report_failure "配布物のウィンドウが ${x11_window_count} 枚あります（1 枚であるべき。前の実行の残りか、余分なウィンドウ）"
fi
observe_windows
shipping_id=$(printf '%s\n' "$x11_window_ids" | head -n 1)
echo "検証 1/3: 配布物のウィンドウ: $(printf '%s\n' "$x11_window_lines" | head -n 1)"

# **活性化の対象を決めるのは活性化の時点のフォーカスである**（7.5）。AT-SPI の活性化の前に
# フォーカスを確定させる（ウィンドウマネージャの有無に依存しない）。
atspi x11-focus "$shipping_id"

echo "AT-SPI（配布物）: メニュー木"
atspi atspi-tree
echo "AT-SPI（配布物）: 期待の検査"
atspi atspi-verify-shipping

# 記録に検証専用の行が現れないこと（9.7 の片付けの規約。既定のビルドは検証専用の記録を出さない）。
if [ "$(record_count "$phase_shipping" "$placement_marker")" -ne 0 ]; then
  report_failure "配布物の記録に検証専用の配置の行が現れています（既定のビルドに検証専用のコードが入っています）"
fi
echo "検証 1/3: 配布物の記録に検証専用の配置の行は現れない（0 件）"

echo "検証 1/3: 診断 > 記録の保存場所を表示 を AT-SPI で活性化する（選択が登録元へ通知されること）"
atspi atspi-activate "記録の保存場所を表示"

notified=$(wait_record "$phase_shipping" "メニュー項目が選択された: 登録元=app-shell 項目=app-shell.diagnostics-log-location 対象ウィンドウ=" 10 || true)
if [ -z "$notified" ]; then
  report_failure "活性化の通知の行（メニュー項目が選択された: 登録元=app-shell 項目=app-shell.diagnostics-log-location …）が記録に現れません"
fi
echo "検証 1/3: 記録（通知）: $notified"

requested=$(wait_record "$phase_shipping" "診断の導線の要求を送った: ウィンドウ = [[:alnum:]_-]* / 導線 = 記録の保存場所" 10 || true)
if [ -z "$requested" ]; then
  report_failure "登録元が要求を送った行（診断の導線の要求を送った: ウィンドウ = … / 導線 = 記録の保存場所）が記録に現れません"
fi
echo "検証 1/3: 記録（登録元の処理）: $requested"

target=$(printf '%s\n' "$notified" | sed -n 's/.*対象ウィンドウ=\(.*\)$/\1/p')
if [ "$target" != "$(printf '%s\n' "$requested" | sed -n 's/.*ウィンドウ = \([^ ]*\) \/.*/\1/p')" ]; then
  report_failure "通知の対象ウィンドウ（${target}）と登録元が送った先が一致しません"
fi
if [ "$target" = "(対象なし)" ] || [ -z "$target" ]; then
  report_failure "活性化の対象ウィンドウがありません（フォーカスがアプリのウィンドウに無い。XSetInputFocus が効いていない）"
fi
echo "検証 1/3: 通知の対象ウィンドウ = ${target}（フォーカス中のウィンドウ）"

stop_app

# ---------------------------------------------------------------------------
# (2) 検証用の形: フォーカスされているウィンドウにだけショートカットが作用すること
# ---------------------------------------------------------------------------

phase_shortcut=$(record_lines)

echo "検証 2/3: 検証用の形を起動する（1 枚目はドキュメントなし）"
nohup "$verify_app" >"$x11_log" 2>&1 &
x11_pid=$!
launched_pids="$launched_pids $x11_pid"

if ! wait_for_windows 1 "$timeout_secs"; then
  report_failure "検証用の形のウィンドウが ${timeout_secs} 秒以内に現れませんでした"
fi
if ! stable_wait "$timeout_secs"; then
  report_failure "検証用の形のウィンドウの集合が ${timeout_secs} 秒以内に落ち着きませんでした"
fi
if [ "$x11_window_count" -ne 1 ]; then
  printf '%s\n' "$x11_window_lines" >&2
  report_failure "検証用の形のウィンドウが ${x11_window_count} 枚あります（1 枚であるべき）"
fi
first_label_line=$(wait_record "$phase_shortcut" "ウィンドウを開いた: label=empty-1 " 10 || true)
if [ -z "$first_label_line" ]; then
  report_failure "1 枚目のウィンドウが label=empty-1 として記録に現れません"
fi
observe_windows
first_id=$(printf '%s\n' "$x11_window_ids" | head -n 1)
first_count=$x11_window_count
echo "検証 2/3: 1 枚目: ラベル=empty-1 識別子=0x$(printf '%s' "$first_id" | sed 's/^0x//') ウィンドウ数=${first_count}"

# **検証用の形のメニュー木も読む** — 7.5 の `verification.probe` が実際にメニューへ現れ、
# 割り当てた `Ctrl+Shift+J` を基盤が保持していること（これから駆動するショートカット）を
# 確かめてから駆動する。
echo "AT-SPI（検証用の形）: 期待の検査"
atspi atspi-verify-verification

# 1 枚目へフォーカスし、ショートカットを送る。**対象ウィンドウが empty-1 であること**を要求する。
# **印は送る前に取る**（作用の行は送った直後に書かれる）。
first_phase=$(record_lines)
atspi x11-focus "$first_id"
atspi x11-shortcut "$first_id"
empty_hit=$(wait_record "$first_phase" "${probe_marker}empty-1$" 10 || true)
if [ -z "$empty_hit" ]; then
  report_failure "1 枚目にフォーカスしたときのショートカットの対象が empty-1 として記録に現れません（ショートカットが作用していないか、対象がフォーカスと一致していません）"
fi
empty_target=$(printf '%s\n' "$empty_hit" | sed -n 's/.*対象ウィンドウ=\(.*\)$/\1/p')
if [ "$empty_target" != "empty-1" ]; then
  report_failure "1 枚目にフォーカスしたときの対象が empty-1 ではありません（${empty_target}）"
fi
echo "検証 2/3: 記録（1 枚目にフォーカス）: $empty_hit"

# ドキュメント付きの 2 枚目を単一インスタンスの引き継ぎで開く。
echo "検証 2/3: 同じ検証用の形をドキュメント位置つきで再度実行する（単一インスタンスが引き継ぐ）"
nohup "$verify_app" "$document" >"$work/second.log" 2>&1 &
second_pid=$!
launched_pids="$launched_pids $second_pid"

if ! wait_for_exit "$second_pid" "$timeout_secs"; then
  report_failure "2 つ目の起動（pid=${second_pid}）が ${timeout_secs} 秒以内に終了しません（2 つ目が常駐している）"
fi
if [ "$exit_status" -ne 0 ]; then
  report_failure "2 つ目の起動が終了コード ${exit_status} で終わった（0 であるべき）"
fi
if ! wait_for_windows "$((first_count + 1))" "$timeout_secs"; then
  report_failure "2 つ目の起動の後、ウィンドウ数が $((first_count + 1)) 以上になりませんでした"
fi
if ! stable_wait "$timeout_secs"; then
  report_failure "2 つ目の起動の後のウィンドウの集合が ${timeout_secs} 秒以内に落ち着きませんでした"
fi
if [ "$x11_window_count" -ne "$((first_count + 1))" ]; then
  printf '%s\n' "$x11_window_lines" >&2
  report_failure "2 つ目の起動の後のウィンドウが ${x11_window_count} 枚です（$((first_count + 1)) 枚であるべき）"
fi
doc_label_line=$(wait_record "$phase_shortcut" "ウィンドウを開いた: label=doc-" 10 || true)
if [ -z "$doc_label_line" ]; then
  report_failure "新しいウィンドウが label=doc-*（ドキュメント付き）として記録に現れません"
fi
doc_label=$(printf '%s' "$doc_label_line" | sed -n 's/.*label=\([^ ]*\).*/\1/p')
observe_windows
second_id=""
for _id in $x11_window_ids; do
  if [ "$_id" != "$first_id" ]; then
    second_id=$_id
  fi
done
if [ -z "$second_id" ]; then
  report_failure "2 枚目のウィンドウの識別子が増えていません（1 枚目と同じ: ${first_id}）"
fi
echo "検証 2/3: 2 枚目: ラベル=${doc_label} 識別子=0x$(printf '%s' "$second_id" | sed 's/^0x//')（1 枚目とは別の識別子）"

# 2 枚目へフォーカスし、同じショートカットを送る。**対象が 2 枚目へ移ること**を要求する。
second_phase=$(record_lines)
atspi x11-focus "$second_id"
atspi x11-shortcut "$second_id"
doc_hit=$(wait_record "$second_phase" "${probe_marker}${doc_label}$" 10 || true)
if [ -z "$doc_hit" ]; then
  report_failure "2 枚目にフォーカスしたときのショートカットの対象が ${doc_label} として記録に現れません（フォーカス先へ振り向いていない）"
fi
echo "検証 2/3: 記録（2 枚目にフォーカス）: $doc_hit"

# **作用したのはフォーカス中の 1 枚だけであること。** 送った回数と対象の内訳を数える。
if [ "$(record_count "$second_phase" "${probe_marker}${doc_label}$")" -ne 1 ]; then
  report_failure "2 枚目にフォーカスしたときの作用が 1 回ではありません（$(( $(record_count "$second_phase" "${probe_marker}${doc_label}$") )) 回）"
fi
if [ "$(record_count "$second_phase" "${probe_marker}${empty_target}$")" -ne 0 ]; then
  report_failure "2 枚目にフォーカスしたのに 1 枚目（${empty_target}）へ作用しました（フォーカスされていないウィンドウに作用している）"
fi
echo "検証 2/3: 2 枚目のショートカットは 2 枚目だけに作用した（empty-1 への作用は 0 件）"

# フォーカスを 1 枚目へ戻すと、対象も戻ること（追随すること）。
back_phase=$(record_lines)
atspi x11-focus "$first_id"
atspi x11-shortcut "$first_id"
back_hit=$(wait_record "$back_phase" "${probe_marker}empty-1$" 10 || true)
if [ -z "$back_hit" ]; then
  report_failure "フォーカスを 1 枚目へ戻したときのショートカットの対象が empty-1 として記録に現れません（フォーカスの移動に追随していない）"
fi
if [ "$(record_count "$back_phase" "${probe_marker}${doc_label}$")" -ne 0 ]; then
  report_failure "フォーカスを 1 枚目へ戻したのに ${doc_label} へ作用しました"
fi
echo "検証 2/3: 記録（フォーカスを 1 枚目へ戻した）: $back_hit"
echo "検証 2/3: ショートカットの対象はフォーカスに追随した（1 枚目 → ${doc_label} → 1 枚目）"

# ---------------------------------------------------------------------------
# (3) 検証用の形: 配置したメニューの記録（位置・表示名・基盤へ渡した綴り・配置方式）
# ---------------------------------------------------------------------------

echo "検証 3/3: 配置したメニューの記録を読む（解決済みの綴りと配置方式）"
placement=$(record_last "$phase_shortcut" "$placement_marker" || true)
if [ -z "$placement" ]; then
  report_failure "配置の記録（[検証] メニューを配置した: …）が記録に現れません（--features verification-triggers のビルドではない）"
fi
echo "検証 3/3: 記録（配置）: $placement"

for expected in \
  "配置=ウィンドウ単位" \
  "項目数=12" \
  "app-shell.open-document(ファイル > 開く…, ショートカット=ctrl+KeyO)" \
  "app-shell.quit(ファイル > 終了, ショートカット=ctrl+KeyQ)" \
  "verification.document-only(ファイル > 検証: ドキュメント付きのみ, ショートカット=(なし))" \
  "verification.probe(ファイル > 検証: 対象ウィンドウを記録, ショートカット=ctrl+shift+KeyJ)" \
  "document-session.new(ファイル > 新規, ショートカット=ctrl+KeyN)" \
  "data-grid.copy(編集 > 複製, ショートカット=ctrl+KeyC)" \
  "data-grid.undo(編集 > 元に戻す, ショートカット=ctrl+KeyZ)" \
  "data-grid.redo(編集 > やり直し, ショートカット=ctrl+shift+KeyZ)" \
  "document-session.save(ファイル > 保存, ショートカット=ctrl+KeyS)" \
  "app-shell.diagnostics-export(診断 > 診断情報を書き出す…, ショートカット=ctrl+shift+KeyE)" \
  "app-shell.diagnostics-log-location(診断 > 記録の保存場所を表示, ショートカット=ctrl+shift+KeyL)" \
  "app-shell.diagnostics-verbosity(診断 > 記録の詳細度…, ショートカット=ctrl+shift+KeyV)"
do
  case "$placement" in
    *"$expected"*) ;;
    *)
      report_failure "配置の記録に ${expected} がありません（メニューの位置・表示名・解決済みの綴りが期待と違う）"
      ;;
  esac
done
echo "検証 3/3: 配置の記録は期待どおり（位置・表示名・解決済みの綴り ctrl+… が 12 項目そろっている）"

stop_app

echo "OK: メニューとショートカットの検査が 3 つとも成立しました（配布物のメニュー項目の出現・選択の通知・フォーカス先への作用・解決済みの綴り）"
exit 0

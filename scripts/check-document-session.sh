#!/bin/sh
# ドキュメントのセッションの 1 回の走行を実機で観測する（tasks.md 5.3 / 要件 8.2、8.3）。
# **Linux / macOS / Windows（Git Bash）の 3 OS で同じものが走る**（POSIX sh）。
#
# 使い方:
#   sh scripts/check-document-session.sh <検証用の形> <タイムアウト秒> <記録ファイル> \
#                                        <雛形のドキュメント> [書き換える行数]
#
#   - 検証用の形            : `--features verification-triggers` の実行ファイル
#                             （`JXCEL_VERIFICATION_BUILD=1 npx tauri build --no-bundle
#                             --features verification-triggers` が作る `target/release/jxcel[.exe]`）。
#                             **配布物（既定のビルド）を渡すと必ず非 0 で落ちる** — それが 5.3 の
#                             負の対照である（下の「負の対照」）。
#   - タイムアウト秒        : 1 回の走行で記録がそろうのを待つ上限（段は 60 秒を渡す）
#   - 記録ファイル          : アプリの診断記録（4.4 の保存先の `jxcel.log`）。**消さない** —
#                             起動中のアプリが開いたファイルへ書き続けるため、消すと行が失われる。
#   - 雛形のドキュメント    : 起動引数に渡すドキュメント。**2 行以上と 1 列以上を持ち、起動時に
#                             内容が変えられてよいもの**（この検査は雛形をそのまま使い回さず、
#                             毎回新しい写しを作ってから走らせる）。既存の標本は
#                             `crates/document-format/tests/fixtures/golden/v1/anchored.jxcel`
#                             （40 行 × 3 列、2140 B）である
#   - 書き換える行数        : 引き金に渡す `<行数>`（既定 4）。**保存のバイト数が雛形と変わる値**
#                             でなければならない（5.2 の実測: 行数 1〜25・39・40 のいずれでも差分は
#                             −1〜+24 で、0 は無い。既定の 4 は 2147 B になる）
#
# # 何のために在るか（なぜ記録を読むのか）
#
# 要件 8.2 は「3 つの OS のそれぞれについて、ドキュメントを読み込み、変更を適用し、保存した結果を、
# **実際に起動して観測した結果**で確認できるようにする」ことを求める。判定の材料は 5.2 の引き金が
# 残す**記録行**である（`[検証] セッション: ` を行頭に固定した 6 行。`src-tauri/src/session/
# verification.rs` の「記録」の節が正本）。**画面もウィンドウツリーも読まない** —
# それらは OS ごとに別機構になり（X11 / AX / UI Automation）、3 OS で同じ判定を閉じられなくなる。
# 記録は 3 OS が同じ綴りで出すので、この検査は 1 つで足りる（`verification.md`「判定は
# ランナーごとに閉じる」）。
#
# # 検証する主張
#
#   (1) **起動の形跡が先にある。** 記録の区切り（走行の直前に取った行数）より後ろに
#       `ウィンドウを開いた: label=doc-` が現れること。**これを最初に確かめる** — 起動して
#       いない実行は「要求が満たされない」だけであり、空振りを緑にしないための錠前である
#       （`verification.md`「『本当に起動したか』を先に確かめてから結果を判定する」）。
#       起動の形跡として使えるのは 10.4 の描画の成立行ではなく**この行**である — 描画は
#       期限に間に合わないことがあり（ソフトウェアラスタライザ）、セッションの引き金は
#       **描画の成否に依存しない**（`RunEvent::Ready` から走る）。
#   (2) **引き金の 6 つの事実が、区切りの後ろに 1 つずつ現れる。**
#       引き金を読んだ（書き換える行数 = 渡した値）/ 読み込んだ行数 / 一括の適用を 1 回行った
#       （**版 = 2** と 行数 = 渡した値）/ 閉じてよいかの答え（保存の前）= 拒否 /
#       保存した（バイト数）/ 閉じてよいかの答え（保存の後）= 許可。
#       **版 = 2 を要求する**のは、要件 3.5（一括が 1 回の適用として運ばれる）を数で示す
#       ためである — 読み込みの完了で版は 1 になり、適用で 2 になる（`document-session` の
#       `session` の不変条件）。版が 3 以上なら一括が複数の適用に割れている。
#       セル数は `セル数 = 行数 × 列数`（比例）であることだけを要求する — 列数は記録に無く、
#       雛形の列数を検査器が知る術が無い（知っていても引き金の側の値を検査器が写すだけになる）。
#   (3) **失敗の記録が無いこと。** `引き金の走行を完了できなかった` が 0 行であること
#       （5.2 は「成功の記録を偽らない」ために、適用も保存も行わない入力をこの 1 行で表す）。
#   (4) **変更が保存へ届いたこと。** 記録のバイト数が**雛形と異なる**こと。あわせて、記録の
#       バイト数が**実際のファイルの大きさと一致する**こと（アプリの自己申告だけでは、保存の
#       完了と記録が食い違っていても通る）。**両方を要求する**理由は、片方だけでは
#       「保存したと言ったが届いていない」と「届いたが記録が古い」を区別できないためである。
#   (5) **決定性が壊れていないこと。** 同じ入力（雛形の**新しい写し**）の 2 回目の走行が、
#       1 回目と**同一バイト列**を書き出すこと。**2 回目は 1 回目の出力を入力にしない** —
#       1 回目の出力を入力にすると「同じ入力の 2 回目」ではなくなる（要件 3.1 / 3.2 の
#       決定性は、同じ入力から同じバイト列が出ることである）。1 回の走行につき保存は 1 回で
#       あるから、2 回目は**新しい写しからの 2 回目の走行**でしか作れない。
#
# # 記録の読み方（主張は区切りの後ろに限定する）
#
# 各走行の**直前**に記録の行数を取り、それ以降の行だけを調べる。**記録は追記式であり、
# 失敗のときに検査器が末尾を出力へダンプする**ので、出力全体を `grep` すると前の走行が書いた行で
# 「引き金を読んだ」が満たされうる（10.8 の段で実際に起きた）。判定はすべて
# `record_count_after` / `record_after`（開始行数より後ろだけ）を通す。
#
# **行頭の印は `\[検証\] セッション: ` である。**`grep -E` へ渡すので角括弧は退避する
# （`[検証]` のままだと `検` か `証` の 1 文字に一致してしまう。`check-menu-shortcut.sh` の
# `probe_marker` と同じ理由・同じ扱い）。
#
# # 負の対照（この検査自身が持つ）
#
# **配布物（既定のビルド）を渡すと、この検査は非 0 で落ちる。** 配布物は
# `JXCEL_VERIFICATION_SESSION` を読まないので「引き金を読んだ」の行が 1 行も現れず、(2) で落ちる。
# ただし**それだけでは錠前の実測にならない** — 起動していない実行も同じ理由で落ちるためである。
# したがって段は、(1) が成立していること（配布物が実際に起動してウィンドウを開いたこと）と、
# 落ちた理由が**引き金の行の不在**であることの両方を要求する（`scripts/ci/*/verify-document-session.*`
# を参照）。**配布物が起動して自分から落ちる検査**であって、「常に落ちる検査」ではない。
#
# # 片付け
#
# 起動したアプリと作業領域（雛形の写し）は**トラップで必ず片付ける**。残すと後続の段が
# 単一インスタンスの機構に引き継がれ、ウィンドウが出ない（偽の失敗になる。10.5 が実測した）。
# 木の走査は `scripts/lib/x11-window.sh` の `x11_kill_tree` を**そのまま使う**（AppImage の
# 展開実行ではラッパーの子として本体が動くため、ラッパーだけを終了すると本体が残る。置き場の
# doc を参照）。**写すと片方だけ直る**（`verification.md`「観測と後始末の共有部分は
# `scripts/lib/` に置く」）。
#
# ## Windows（Git Bash）での残る差（正直に記す）
#
#   - 置き場の `x11_kill_tree` は子孫の列挙に `pgrep` を使う。Git Bash には既定で `pgrep` が
#     無いため、Windows では**起動したプロセスそのもの**だけが終了する（WebView2 の補助
#     プロセスはアプリの死で道連れになるが、保証ではない）。段の側が
#     `Get-Process -Name jxcel` で残存を確かめて落とす（OS 固有の部分は段に置く）。
#   - 残留の確認（次の走行の前に同じ名前のプロセスが消えたことを確かめる。下の
#     `wait_for_no_app`）も `pgrep` を要する。無ければ**確認をスキップしたことを明示して**
#     続ける（黙って飛ばさない）。
#
# # ローカルで閉じられないもの
#
# macOS / Windows での実行はこの開発機では行えない（`verification.md`「ローカルで閉じられない
# もの」）。この検査は 3 OS で同じものが走るが、**走らせた事実は OS ごとに段が残す**。
#
# # 前提
#   - Linux では `DISPLAY` が要る（GUI の無いランナーでは `xvfb-run` の下で呼ぶ）。macOS /
#     Windows では要求しない。X11 の道具（`xwininfo` など）も**要求しない** — この検査は
#     ウィンドウツリーを読まない。
#   - `pgrep`（あれば残留の確認と木の走査に使う。無ければ上記の差が残る）。
#
# # 終了コード
#   0 = 2 回の走行で 5 つの主張がすべて成立 / 1 = 検査失敗（起動の形跡が無い・事実が欠ける・
#   版が 2 でない・保存のバイト数が雛形と同じ・2 回の出力が一致しない）/ 2 = 入力が使えない
#   （引数・実行ファイル・記録の指定・雛形の不在・行数の形・DISPLAY の不在・未対応の OS）
#
# SC1091 / SC2154: 置き場は**同じリポジトリのファイル**であり、`x11_kill_tree` /
# `x11_pick_poll_sleep` はそこで代入される。qlty は検査対象を一時ディレクトリへ写してから
# 静的解析にかけるため、置き場をたどれず「たどれない・未代入」と報告する（実際の実行では
# `$0` からの相対で解決する）。契約は置き場の doc に 1 つだけ書いてある。
# shellcheck disable=SC1091,SC2154
set -eu

# 引き金に渡す行数の既定。**保存のバイト数が雛形と変わる値**であること（5.2 の実測: 行数 4 は
# 2140 B → 2147 B）。
default_rows=4

# 行数の上限。引き金は読み込んだ行数を超える行数を拒否するが、雛形の行数を検査器は知らないので、
# 明らかに異常な値だけを弾く（入力の形の検査であって、受入の判定ではない）。
max_rows=1000000

_x11_lib_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
# 片付け（木の走査）と検出粒度は**置き場の 1 実装を使う**。この検査はウィンドウを観測しないので、
# 置き場の他の関数（`x11_collect_windows` など）は呼ばない — 置き場から読むのはこの 2 つだけである。
. "$_x11_lib_dir/lib/x11-window.sh"

usage() {
  echo "使い方: sh scripts/check-document-session.sh <検証用の形> <タイムアウト秒> <記録ファイル> <雛形のドキュメント> [書き換える行数]" >&2
  exit 2
}

if [ "$#" -lt 4 ] || [ "$#" -gt 5 ]; then
  usage
fi

app=$1
timeout_seconds=$2
record=$3
template=$4
rows=${5:-$default_rows}

case "$timeout_seconds" in
  '' | *[!0-9]*)
    echo "NG: タイムアウト秒が整数ではありません: $timeout_seconds" >&2
    exit 2
    ;;
esac
if [ "$timeout_seconds" -le 0 ]; then
  echo "NG: タイムアウト秒が 0 以下です: $timeout_seconds" >&2
  exit 2
fi

# **先頭の 0 を弾く。** 記録には引き金が解釈した値が `4` の形で出るので、`04` を受け付けると
# 文字列の比較が食い違い、正しい走行を失敗と報告する（値そのものは同じである）。
case "$rows" in
  '' | 0* | *[!0-9]*)
    echo "NG: 書き換える行数が正の整数ではありません（先頭の 0 も受け付けません）: $rows" >&2
    exit 2
    ;;
esac
if [ "$rows" -le 0 ] || [ "$rows" -gt "$max_rows" ]; then
  echo "NG: 書き換える行数が範囲外です（1 以上 ${max_rows} 以下）: $rows" >&2
  exit 2
fi

if [ -z "$record" ]; then
  echo "NG: 記録ファイルが指定されていません（アプリの診断記録の保存先を渡してください）" >&2
  exit 2
fi

if [ ! -f "$app" ] || [ ! -x "$app" ]; then
  echo "NG: 検証用の形がありません（実行権限も必要です）: $app" >&2
  echo "    --features verification-triggers のビルド（例 JXCEL_VERIFICATION_BUILD=1 npx tauri build --no-bundle --features verification-triggers）を先に行ってください" >&2
  exit 2
fi

if [ ! -f "$template" ]; then
  echo "NG: 雛形のドキュメントがありません: $template" >&2
  exit 2
fi

case "$(uname -s 2>/dev/null || echo unknown)" in
  Linux)
    os_mode=linux
    ;;
  Darwin)
    os_mode=macos
    ;;
  MINGW* | MSYS* | CYGWIN*)
    os_mode=windows
    ;;
  *)
    echo "NG: この検査は Linux / macOS / Windows（Git Bash）用です（uname -s = $(uname -s 2>/dev/null || echo unknown)）" >&2
    exit 2
    ;;
esac

# Linux では表示サーバーが要る（GUI アプリを起動できない）。CI は `xvfb-run` の下で走らせる。
# macOS / Windows は OS が画面を持っているので要求しない（この検査はそれを読まない）。
if [ "$os_mode" = linux ] && [ -z "${DISPLAY:-}" ]; then
  echo "NG: DISPLAY が設定されていません（GUI アプリを起動できません。CI では xvfb-run の下で呼ぶ）" >&2
  exit 2
fi

# **親の環境から検証専用の引き金が漏れないようにする**（10.6 の段と同じ規律）。とくに
# `JXCEL_VERIFICATION_EXIT_AFTER_MS` は走行の途中でアプリを終わらせ、記録を途中で断ち切る
# （＝偽の失敗）。**この検査は引き金を自分でアプリへ渡す**（下の起動）ので、親の値は消す。
unset JXCEL_VERIFICATION_DENY_CLOSE 2>/dev/null || true
unset JXCEL_VERIFICATION_INITIAL_SCREEN 2>/dev/null || true
unset JXCEL_VERIFICATION_EXIT_AFTER_MS 2>/dev/null || true
unset JXCEL_VERIFICATION_BULK_ROWS 2>/dev/null || true
# 自分の系統は**起動の行で渡す**（親の値に依らない）。
unset JXCEL_VERIFICATION_SESSION 2>/dev/null || true

x11_pick_poll_sleep
poll_sleep=$x11_poll_sleep

# 作業領域（雛形の写し）。**起動の前に作り、トラップで必ず片付ける。**
work=$(mktemp -d "${TMPDIR:-/tmp}/jxcel-document-session.XXXXXX")
app_pid=''
app_log=''
run1="$work/run1.jxcel"
run2="$work/run2.jxcel"

# 起動したアプリを終了し、**回収する**。`wait` まで行うので、戻った時点でプロセスは確実に
# 消えている（これが次の走行の前提である — 生きていると単一インスタンスの機構が新しい起動を
# 引き継ぎ、2 回目の走行は何も記録しない。10.5 / 10.6 が実測した偽の失敗である）。
stop_app() {
  if [ -n "${app_pid:-}" ] && kill -0 "$app_pid" 2>/dev/null; then
    x11_kill_tree "$app_pid"
    ticks=0
    while [ "$ticks" -lt 50 ] && kill -0 "$app_pid" 2>/dev/null; do
      sleep "$poll_sleep"
      ticks=$((ticks + 1))
    done
    if kill -0 "$app_pid" 2>/dev/null; then
      kill -9 "$app_pid" 2>/dev/null || true
    fi
    wait "$app_pid" 2>/dev/null || true
  fi
  app_pid=''
}

# 同じ名前のプロセスが残っていないことを確かめる（**次の走行の前の錠前**）。
#
# `pgrep` が無い環境（既定の Git Bash）では**確認をスキップしたことを明示して**続ける —
# 「確かめた」と偽らない。段の側が OS 固有の手段（Windows は `Get-Process`）で確かめる。
wait_for_no_app() {
  if ! command -v pgrep >/dev/null 2>&1; then
    echo "注意: pgrep が無いので残留プロセスの確認をスキップします（Windows の段は Get-Process で確かめます）"
    return 0
  fi
  ticks=0
  while [ "$ticks" -lt 50 ] && pgrep -x jxcel >/dev/null 2>&1; do
    sleep "$poll_sleep"
    ticks=$((ticks + 1))
  done
  if pgrep -x jxcel >/dev/null 2>&1; then
    echo "注意: 名前 jxcel のプロセスが残っています（単一インスタンスの機構が次の走行を引き継ぐと、2 回目は何も記録しません）" >&2
  fi
}

# shellcheck disable=SC2329 # 下の trap から呼ばれる（shellcheck は trap を追えない）
cleanup() {
  stop_app
  if [ -n "${work:-}" ] && [ -d "$work" ]; then
    rm -rf "$work"
  fi
  :
}
trap cleanup EXIT INT TERM

# 記録の現在の行数（無ければ 0）。**各走行の直前に取る。**
record_lines() {
  if [ -f "$record" ]; then
    wc -l < "$record" | tr -d ' '
  else
    echo 0
  fi
}

# 記録の `<開始行数>` より後から、`grep -E` のパターンに一致する行を全部出す。
#
# **行末の CR を落とす。** 値の取り出し（下の `sed 's/… = \([0-9]*\)$/\1/p'`）は行末に錨を
# 置くので、`\r\n` の記録では**常に空文字が取れて偽の失敗になる**。記録機構は `\n` で書く
# （10.8 の Windows の段も同じ前提で `$` を使っている）が、**綴りに依存しない**ようにしておく —
# 落とすのは行末の CR だけで、記録の中身には現れない文字である。
record_after() {
  from=$1
  pattern=$2
  [ -f "$record" ] || return 0
  tail -n "+$((from + 1))" "$record" 2>/dev/null | grep -E "$pattern" | tr -d '\r' || true
}

# 記録の `<開始行数>` より後から、`grep -E` のパターンに一致する行の数。
#
# **`awk -v` を使わない。** 代入の値のバックスラッシュを解釈してしまい、退避つきの角括弧
# （`\[検証\]`）が素の `[検証]`（＝文字クラス）になって一致件数が常に 0 になる
# （10.6 の検査器が実測した。`grep -c -E` は `record_after` と同じ解釈である）。
# **行末の CR も落とす**（`record_after` と同じ理由 — `$` に錨を置くパターンが空振りする）。
record_count_after() {
  from=$1
  pattern=$2
  if [ ! -f "$record" ]; then
    echo 0
    return 0
  fi
  tail -n "+$((from + 1))" "$record" 2>/dev/null | tr -d '\r' | grep -c -E "$pattern" || true
}

report_failure() {
  echo "NG: $1" >&2
  if [ -n "${app_log:-}" ] && [ -f "$app_log" ]; then
    echo "--- アプリの出力（末尾）: $app_log ---" >&2
    tail -n 30 "$app_log" >&2 || true
  fi
  if [ -f "$record" ]; then
    echo "--- 診断記録（末尾）: $record ---" >&2
    tail -n 40 "$record" >&2
  else
    echo "--- 診断記録: ${record}（存在しません。アプリは記録を書いていません） ---" >&2
  fi
  exit 1
}

# 走行 1 回分の記録を、**区切りの後ろだけ**から判定する。
#
#   judge_run <開始行数> <呼び名> <保存先になったファイル>
#
# 判定が順序を持つのが要点である: (1) 起動の形跡 → (3) 失敗の記録の不在 → (2) の 6 つの事実。
# 起動していない実行を「失敗の記録が無い」で通さないため、起動の形跡を最初に要求する。
judge_run() {
  judge_baseline=$1
  judge_label=$2
  judge_file=$3

  # (1) 起動の形跡。**`doc-` の窓に限る** — この検査はドキュメント付きで起動するので、
  #     ドキュメントの窓が開いたことが「起動の形跡」である（`window/mod.rs` の
  #     `ウィンドウを開いた: label=… ドキュメント=…`）。
  startup_count=$(record_count_after "$judge_baseline" 'ウィンドウを開いた: label=doc-')
  if [ "$startup_count" -lt 1 ]; then
    if [ ! -f "$record" ]; then
      report_failure "${judge_label}: 記録ファイルが作られていません（${record}）。起動できていないか、記録の保存先が違います（アプリは 4.4 の解決で保存先を決めます）"
    fi
    report_failure "${judge_label}: 起動の形跡（'ウィンドウを開いた: label=doc-'）が区切りの後ろに 1 行もありません。アプリが起動していないか、ウィンドウを開けていません（空振りの実行を緑にしないための錠前です）"
  fi
  startup_line=$(record_after "$judge_baseline" 'ウィンドウを開いた: label=doc-' | head -n 1)
  echo "  ${judge_label}: 起動の形跡: ${startup_line}"

  # (3) 失敗の記録。**先に見る** — 6 つの事実が欠けた理由（提示の不在・行数の超過・回転が恒等、
  #     など）はこの行に書かれている（5.2 の `TriggerFailure`）。
  failure_line=$(record_after "$judge_baseline" '引き金の走行を完了できなかった' | head -n 1)
  if [ -n "$failure_line" ]; then
    report_failure "${judge_label}: 引き金が走行を完了できませんでした（観測対象が要求を満たせない入力です。行数を減らすか、雛形の行数・列数・値を見直してください）: ${failure_line}"
  fi

  # (2a) 引き金を読んだ。**渡した行数と一致すること**まで要求する（別の行数で走った記録を
  #      この走行の証拠にしない）。
  read_count=$(record_count_after "$judge_baseline" "${marker}引き金を読んだ: 書き換える行数 = ")
  if [ "$read_count" -ne 1 ]; then
    report_failure "${judge_label}: '引き金を読んだ' の行が 1 行ではありません（観測 ${read_count} 行。期待 1 行）。配布物（既定のビルド）は引き金を読まないので、この理由で落ちるのは負の対照では正しい応答です。検証用の形（--features verification-triggers）で起動しているかを確かめてください（配布物が起動していたことは、上の起動の形跡で確かめています）"
  fi
  declared=$(record_after "$judge_baseline" "${marker}引き金を読んだ: 書き換える行数 = " |
    sed -n 's/.*書き換える行数 = \([0-9][0-9]*\)$/\1/p' | head -n 1)
  if [ "$declared" != "$rows" ]; then
    report_failure "${judge_label}: 引き金が読んだ行数が渡した値と違います（観測 ${declared} / 渡した値 ${rows}）"
  fi
  echo "  ${judge_label}: 引き金を読んだ（書き換える行数 = ${declared}）"

  # (2b) 読み込み。**2 行以上**であること（1 行では回転が恒等写像になり、変更が保存へ届かない。
  #      5.2 はその入力を引き金の側でも拒否する）と、渡した行数がその範囲に収まること。
  loaded_count=$(record_count_after "$judge_baseline" "${marker}読み込んだ行数 = ")
  if [ "$loaded_count" -ne 1 ]; then
    report_failure "${judge_label}: '読み込んだ行数' の行が 1 行ではありません（観測 ${loaded_count} 行。期待 1 行。ドキュメントを保持できていないか、起動引数に渡っていません）"
  fi
  loaded=$(record_after "$judge_baseline" "${marker}読み込んだ行数 = " |
    sed -n 's/.*読み込んだ行数 = \([0-9][0-9]*\)$/\1/p' | head -n 1)
  if [ -z "$loaded" ]; then
    report_failure "${judge_label}: '読み込んだ行数' の行から数値を読めませんでした"
  fi
  if [ "$loaded" -lt 2 ]; then
    report_failure "${judge_label}: 読み込んだ行数が 2 未満です（${loaded}）。雛形には 2 行以上が必要です"
  fi
  if [ "$rows" -gt "$loaded" ]; then
    report_failure "${judge_label}: 渡した行数（${rows}）が読み込んだ行数（${loaded}）を超えています"
  fi
  echo "  ${judge_label}: 読み込んだ行数 = ${loaded}"

  # (2c) 一括の適用。**版 = 2 と 行数 = 渡した値**を要求する（要件 3.5 の「1 回の一括が 1 回の
  #      適用として運ばれる」の数による証拠）。セル数は行数に比例することだけを見る。
  apply_pattern="${marker}一括の適用を 1 回行った: 版 = 2 / 行数 = ${rows} / セル数 = "
  apply_count=$(record_count_after "$judge_baseline" "$apply_pattern")
  if [ "$apply_count" -ne 1 ]; then
    observed_apply=$(record_after "$judge_baseline" "${marker}一括の適用を 1 回行った: " | head -n 1)
    report_failure "${judge_label}: '版 = 2 / 行数 = ${rows}' の適用の行が 1 行ではありません（観測 ${apply_count} 行。期待 1 行。観測した行: ${observed_apply:-（なし）}）。版が 2 でないときは、一括が 1 回の適用として運ばれていません（読み込みで 1、適用で 2 が正しい）"
  fi
  cells=$(record_after "$judge_baseline" "$apply_pattern" |
    sed -n 's/.*セル数 = \([0-9][0-9]*\)$/\1/p' | head -n 1)
  if [ -z "$cells" ] || [ "$cells" -lt "$rows" ]; then
    report_failure "${judge_label}: 適用したセル数（${cells:-読めない}）が行数（${rows}）に満たません（1 行につき 1 列以上が必要です）"
  fi
  if [ "$((cells % rows))" -ne 0 ]; then
    report_failure "${judge_label}: 適用したセル数（${cells}）が行数（${rows}）で割り切れません（セル数 = 行数 × 列数 でなければ、選択した行以外を書き換えています）"
  fi
  echo "  ${judge_label}: 一括の適用を 1 回行った（版 = 2 / 行数 = ${rows} / セル数 = ${cells}（列数 $((cells / rows))））"

  # (2d) 閉じてよいかの答え（保存の前）。**未保存が立っているので拒否**が正しい答えである。
  before_count=$(record_count_after "$judge_baseline" "${marker}閉じてよいかの答え（保存の前） = ")
  if [ "$before_count" -ne 1 ]; then
    report_failure "${judge_label}: '閉じてよいかの答え（保存の前）' の行が 1 行ではありません（観測 ${before_count} 行。期待 1 行）"
  fi
  before_answer=$(record_after "$judge_baseline" "${marker}閉じてよいかの答え（保存の前） = " |
    sed -n 's/.*閉じてよいかの答え（保存の前） = \(.*\)$/\1/p' | head -n 1)
  if [ "$before_answer" != "拒否" ]; then
    report_failure "${judge_label}: 保存の前の答えが '拒否' ではありません（観測 '${before_answer}'）。適用で未保存が立っていない疑いがあります（要件 4.1 / 6.1）"
  fi
  echo "  ${judge_label}: 閉じてよいかの答え（保存の前） = ${before_answer}"

  # (2e) 保存。**記録のバイト数と実際のファイルの大きさが一致**し、**雛形と異なる**こと。
  save_count=$(record_count_after "$judge_baseline" "${marker}保存した: バイト数 = ")
  if [ "$save_count" -ne 1 ]; then
    report_failure "${judge_label}: '保存した' の行が 1 行ではありません（観測 ${save_count} 行。期待 1 行）"
  fi
  saved_bytes=$(record_after "$judge_baseline" "${marker}保存した: バイト数 = " |
    sed -n 's/.*保存した: バイト数 = \([0-9][0-9]*\)$/\1/p' | head -n 1)
  if [ -z "$saved_bytes" ]; then
    report_failure "${judge_label}: '保存した' の行からバイト数を読めませんでした"
  fi
  if [ ! -f "$judge_file" ]; then
    report_failure "${judge_label}: 保存先（${judge_file}）がありません（保存が書き出しまで届いていません）"
  fi
  actual_bytes=$(wc -c < "$judge_file" | tr -d ' ')
  if [ "$saved_bytes" != "$actual_bytes" ]; then
    report_failure "${judge_label}: 記録のバイト数（${saved_bytes}）が実際のファイルの大きさ（${actual_bytes}）と一致しません（保存の完了と記録が食い違っています）"
  fi
  if [ "$saved_bytes" = "$template_bytes" ]; then
    report_failure "${judge_label}: 保存のバイト数（${saved_bytes}）が雛形と同じです（変更が保存へ届いていません）。雛形と行数の組を、バイト数が変わるものに替えてください（5.2 の実測: 行数 1〜25・39・40 では差分は −1〜+24 で、0 はありません）"
  fi
  echo "  ${judge_label}: 保存した（バイト数 = ${saved_bytes} / 雛形 = ${template_bytes} / 実際のファイル = ${actual_bytes}）"

  # (2f) 閉じてよいかの答え（保存の後）。**保存の成功で未保存が落ちるので許可**が正しい答えである
  #      （要件 5.5）。保存の前後で答えが変わったことが、この 2 行の対で示される。
  after_count=$(record_count_after "$judge_baseline" "${marker}閉じてよいかの答え（保存の後） = ")
  if [ "$after_count" -ne 1 ]; then
    report_failure "${judge_label}: '閉じてよいかの答え（保存の後）' の行が 1 行ではありません（観測 ${after_count} 行。期待 1 行）"
  fi
  after_answer=$(record_after "$judge_baseline" "${marker}閉じてよいかの答え（保存の後） = " |
    sed -n 's/.*閉じてよいかの答え（保存の後） = \(.*\)$/\1/p' | head -n 1)
  if [ "$after_answer" != "許可" ]; then
    report_failure "${judge_label}: 保存の後の答えが '許可' ではありません（観測 '${after_answer}'）。保存の成功で未保存が落ちているかを確かめてください（要件 5.5）"
  fi
  echo "  ${judge_label}: 閉じてよいかの答え（保存の後） = ${after_answer}"

  judge_saved_bytes=$saved_bytes
}

# --- 前提の出力と雛形の大きさ ------------------------------------------------
template_bytes=$(wc -c < "$template" | tr -d ' ')
echo "検証用の形: $app"
echo "診断記録: $record"
# **変数名の直後に全角文字を置かない。**macOS の bash 3.2 は UTF-8 のロケールで
# `$template` の直後に `（` を置くと、それを変数名の一部として読み、`set -u` の下で
# 「template（: unbound variable」になる（CI の macOS の実測。Linux では起きない）。
echo "雛形: ${template}（${template_bytes} B）"
echo "書き換える行数: ${rows} / 1 走行のタイムアウト: ${timeout_seconds} 秒 / OS: ${os_mode}"
echo "作業領域: $work"

# 記録の行頭の印。**`grep -E` に渡すので角括弧は退避する**（ファイル冒頭の「記録の読み方」）。
marker='\[検証\] セッション: '

# 走行 1 回分の本体。**雛形の新しい写し**を作り、その写しを起動引数に渡す。
#
#   run_once <写しの位置> <呼び名>
#
# 写しを作るのは、雛形をその場で書き換えないためである（2 回目の走行が 1 回目の出力を入力に
# しないことと同じ理由。下の「2 回の走行」）。
run_once() {
  run_document=$1
  run_label=$2
  cp "$template" "$run_document"

  baseline=$(record_lines)
  echo "  ${run_label}: 記録の開始行数 = ${baseline}（これ以降の行だけを調べる）"

  app_log="$work/${run_label}.output"
  # 引き金は**この 1 回の起動だけ**に渡す（親の環境には残さない）。
  # Linux だけ `GDK_BACKEND=x11` を渡す（10.8 の検査器と同じ理由 — 仮想ディスプレイ上で
  # ウィンドウを持たせる）。
  if [ "$os_mode" = linux ]; then
    nohup env GDK_BACKEND=x11 \
      JXCEL_VERIFICATION_SESSION="open,edit,${rows},save" \
      "$app" "$run_document" >"$app_log" 2>&1 &
  else
    nohup env JXCEL_VERIFICATION_SESSION="open,edit,${rows},save" \
      "$app" "$run_document" >"$app_log" 2>&1 &
  fi
  app_pid=$!

  # **最後の事実（保存の後の答え）か失敗の記録か期限まで待つ。** 途中で落ちた場合も期限まで
  # 待つ（`$!` が起動ラッパーを指す AppImage の展開実行では、ラッパーの終了がアプリの終了を
  # 意味しない。10.3 / 10.8 の実測）。期限で打ち切っても、**判定は judge_run が行う** —
  # 何が欠けているかはそちらが名指しする。
  deadline=$(( $(date +%s) + timeout_seconds ))
  waited=0
  while :; do
    if [ "$(record_count_after "$baseline" "${marker}閉じてよいかの答え（保存の後） = ")" -ge 1 ]; then
      break
    fi
    if [ "$(record_count_after "$baseline" '引き金の走行を完了できなかった')" -ge 1 ]; then
      break
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      waited=1
      break
    fi
    sleep "$poll_sleep"
  done
  if [ "$waited" -eq 1 ]; then
    echo "  ${run_label}: ${timeout_seconds} 秒以内に最後の事実が現れなかったので、記録の判定へ進みます"
  fi

  # 記録を読む前に止める（読み終えた区間がこれ以上伸びないようにする。**回収まで行う**ので、
  # 次の走行は単一インスタンスの機構に引き継がれない）。
  stop_app
  wait_for_no_app

  judge_run "$baseline" "$run_label" "$run_document"
  # 呼び出し側が読む値（POSIX sh に参照渡しが無いので、局所の 1 変数に写す）。
  run_saved_bytes=$judge_saved_bytes
}

# --- 2 回の走行（同じ入力の 2 回目） -----------------------------------------
#
# **1 回目と 2 回目は同じ雛形の写しから始める**（1 回目の出力を 2 回目の入力にすると、
# 「同じ入力の 2 回目の保存」ではなくなる）。2 回が同一バイト列であることが、決定性
# （要件 3.1 / 3.2）がセッションの経路を通っても壊れていないことの証拠である。
echo "走行 1/2: 引き金の走行（読み込み → 一括の適用 → 保存）"
run_once "$run1" run1
bytes_run1=$run_saved_bytes

echo "走行 2/2: 同じ雛形の新しい写しで、もう一度だけ走らせる（決定性）"
run_once "$run2" run2
bytes_run2=$run_saved_bytes

if ! cmp -s "$run1" "$run2"; then
  echo "NG: 同じ雛形の写しから始めた 2 回の保存が同一バイト列ではありません（決定性が壊れています）" >&2
  echo "  1 回目: ${bytes_run1} B / 2 回目: ${bytes_run2} B（保存先: ${run1} / ${run2}）" >&2
  cmp "$run1" "$run2" 2>&1 | head -n 3 >&2 || true
  exit 1
fi

echo "OK: 起動の形跡のあとに 6 つの事実がそろった（引き金を読んだ・読み込んだ行数・版 = 2 の適用・保存の前の拒否・保存した・保存の後の許可）"
echo "OK: 変更が保存へ届いた（記録のバイト数 ${bytes_run1} B ≠ 雛形 ${template_bytes} B、かつ実際のファイルの大きさと一致）"
echo "OK: 同じ雛形の写しからの 2 回目の保存が同一バイト列である（決定性。1 回目 ${bytes_run1} B / 2 回目 ${bytes_run2} B）"
echo "注意: この検査は記録を読む検査であり、ウィンドウツリーも画面も読んでいない（ウィンドウが開いたことはアプリの記録で確かめている）"
exit 0

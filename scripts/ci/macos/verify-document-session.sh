#!/bin/bash
# # macOS の段（tasks.md 5.3 / 要件 8.2、8.3）。判定の本体は POSIX sh の
# `scripts/check-document-session.sh`（**同じものが 3 OS で走る**）。**この段が足すのは OS 固有の
# 1 つだけである**: 記録の位置（macOS の規約 `$HOME/Library/Logs/{識別子}`。`logs` 接尾辞は
# 付かない）と、配布物の取り出し方（`.app` の中の実行ファイルをインストーラ無しで起動する）。
#
# # アプリの出力を記録と混ぜないこと（10.4 / 10.6 の macOS の段が発見した罠）
#
# macOS では記録の保存先が `$HOME/Library/Logs` の下にあり、**同じファイルへ標準出力を向けると
# 2 人の書き手が同じ offset を触りうる** — アプリ（`TargetKind::Stdout`）が先頭から上書きし、
# 記録機構（`TargetKind::Folder`）が末尾へ書く（10.4 が実測した偽の失敗）。**検査器は自分で
# アプリの出力を自分の作業領域のファイルへ向ける**（`>"$app_log" 2>&1`）ので、この段は記録と
# 混ざらない — 段の側で別ファイルを作る必要は無い（作っても誰も読まない）。失敗のときにだけ
# 末尾 30 行が診断として出る。
#
# # 何を確かめるか（Linux の段と同じ 3 つ）
#
#   1. **正例**: 検証用の形（`target/release/jxcel`）をドキュメント付きで起動し、引き金
#      （`JXCEL_VERIFICATION_SESSION=open,edit,4,save`）の 6 つの事実が記録されること。検査器は
#      起動の形跡を先に確かめ、区切りの後ろだけを数え、保存のバイト数が雛形と異なること、
#      2 回目の保存（**雛形の新しい写しからの 2 回目の走行**）が同一バイト列であることを要求する。
#   2. **負の対照**: 配布物（`.app` の中の実行ファイル。インストーラを使わずそのまま起動する）を
#      渡すと検査器が非 0 で落ちること。**落ちた理由が「引き金の行の不在」であること**と、
#      **配布物が起動してドキュメントの窓を開いたこと**まで要求する（起動しなかった実行も
#      事実の欠落で落ちるので、理由を見ないと錠前の実測にならない）。
#   3. **入力が使えない場合**: 存在しない実行ファイルを渡すと 2 で落ちること
#      （`verification.md`「入力が無いときに 0 を返さない」）。
#
# # 証拠が何を証明し、何を証明しないか
#
#   - 証明する: **このランナーの実物のアプリ**が、ドキュメントを読み込み、一括の変更を適用し、
#     保存し、そのバイト列が同じ入力の 2 回目で一致すること。判定はアプリ自身の記録と、
#     保存先のファイルの大きさ・バイト列で行う（画面もアクセシビリティの木も読まない — 3 OS で
#     同じ判定を閉じるため）。
#   - 証明しない: 画面の見え方・メニューからの保存・保存先の選択の提示（別の段 / 別のタスク）。
#   - **アクセシビリティの許可は要らない**（この段は AX を読まない）。10.6 の macOS の段が
#     「AX が無いのでクリックとキー入力を配送できない」と記しているのと同じ制約の中で、
#     **この段はその制約の影響を受けない** — 記録を読むだけだからである。
#
# # 位置
#
# 配布物を起動する段である（負の対照）。成果物の保存の直前（配布物を起動する段の並びの最後）に置く。
# `xvfb-run` のような仮想ディスプレイの包みは要らない（OS が画面を持ち、この段はそれを読まない）。
# 検査器は起動したアプリを**回収まで行って**終了するので、2 回目の走行が単一インスタンスの機構に
# 引き継がれることはない（段の側でプロセスを止める必要は無い）。

set -euo pipefail
# 4.4 の解決（macOS は $HOME/Library/Logs/{識別子}。`logs` 接尾辞は付かない）。
record="$HOME/Library/Logs/com.jxcel.app/jxcel.log"
mkdir -p "$(dirname "$record")"
echo "診断記録: $record"

# 5.2 の引き金が書き換える行数。**保存のバイト数が雛形と変わる値**でなければならない
# （5.2 の実測: 行数 4 は 2140 B → 2147 B）。
rows=4
template=crates/document-format/tests/fixtures/golden/v1/anchored.jxcel
if [ ! -f "$template" ]; then
  echo "NG: 雛形のドキュメントがありません: $template" >&2
  exit 2
fi

verify=target/release/jxcel
if [ ! -x "$verify" ]; then
  echo "NG: 検証用の形がありません: $verify（--features verification-triggers のビルドが先に必要。10.4 の段が作ります）" >&2
  exit 2
fi
# 配布物（`.app` の中の実行ファイル）。**インストーラを使わずそのまま起動する**（10.4 の段と同じ）。
shipping=target/release/bundle/macos/jxcel.app/Contents/MacOS/jxcel
if [ ! -x "$shipping" ]; then
  echo "NG: 配布物（.app の実行ファイル）がありません: $shipping（直前の「Build platform bundle」が先に必要）" >&2
  exit 2
fi

# 記録の行数。**各呼び出しの直前**に取り、それ以降の行だけを調べる（記録は追記式であり、
# 検査器は失敗のときに末尾を出力へダンプするので、出力全体を `grep` すると前の呼び出しの行で
# 満たされうる）。
record_lines() {
  if [ -f "$record" ]; then wc -l < "$record" | tr -d ' '; else echo 0; fi
}
count_after() {
  if [ ! -f "$record" ]; then echo 0; return 0; fi
  tail -n "+$(( $1 + 1 ))" "$record" 2>/dev/null | grep -c -E "$2" || true
}

echo "--- 1/3 正例: 検証用の形で引き金の走行を 2 回行う（読み込み → 一括の適用 → 保存。2 回目は決定性）"
before_pos=$(record_lines)
sh scripts/check-document-session.sh "$verify" 60 "$record" "$template" "$rows"
if [ "$(count_after "$before_pos" '引き金を読んだ: 書き換える行数 = '"$rows"'$')" -ne 2 ]; then
  echo "NG: 正例で「引き金を読んだ」の行が 2 行ではありません（2 回の走行で 2 行が期待。前の段の行では満たせない）" >&2
  exit 1
fi
if [ "$(count_after "$before_pos" '保存した: バイト数 = ')" -ne 2 ]; then
  echo "NG: 正例で「保存した」の行が 2 行ではありません（2 回の走行で 2 行が期待）" >&2
  exit 1
fi
echo "OK: 正例: 検証用の形で 2 回の走行が成立し、6 つの事実がそれぞれ 2 行そろった"

echo "--- 2/3 負の対照: 配布物（.app の実行ファイル）を渡すと検査器が非 0 で落ちること"
before_neg=$(record_lines)
rc=0
output=$(sh scripts/check-document-session.sh "$shipping" 45 "$record" "$template" "$rows" 2>&1) || rc=$?
printf '%s\n' "$output"
if [ "$rc" -eq 0 ]; then
  echo "NG: 配布物に対して検査器が成功してしまった（既定のビルドに検証専用の引き金が入っている）" >&2
  exit 1
fi
if [ "$rc" -eq 2 ]; then
  echo "NG: 負の対照が「入力が使えない」で落ちた（検査器が走っていない）" >&2
  exit 1
fi
# **落ちた理由が「引き金の行の不在」であること**（起動しなかった実行も事実の欠落で落ちる）。
if ! printf '%s\n' "$output" | grep -q "'引き金を読んだ' の行が 1 行ではありません"; then
  echo "NG: 負の対照が「引き金の行の不在」以外の理由で落ちた（検査器が錠前を見ていない可能性がある）" >&2
  exit 1
fi
# **配布物が実際に起動してドキュメントの窓を開いたこと**（検査器が出す起動の形跡の行）。
if ! printf '%s\n' "$output" | grep -q '起動の形跡: .*ウィンドウを開いた: label=doc-'; then
  echo "NG: 負の対照で配布物が起動した形跡がありません（起動しなかった実行を錠前の実測と取り違えない）" >&2
  exit 1
fi
if [ "$(count_after "$before_neg" '引き金を読んだ: 書き換える行数 = ')" -ne 0 ]; then
  echo "NG: 配布物が検証専用の引き金を読んでいます（既定のビルドに検証専用の経路が入っている）" >&2
  exit 1
fi
echo "OK: 反証: 配布物は起動してドキュメントの窓を開いたが、引き金を読まないため検査器が非 0 で落ちた"

echo "--- 3/3 入力が使えない場合: 存在しない実行ファイルなら 2 で落ちること"
rc=0
output=$(sh scripts/check-document-session.sh "$PWD/no-such-verification-build" 10 "$record" "$template" "$rows" 2>&1) || rc=$?
printf '%s\n' "$output"
if [ "$rc" -ne 2 ]; then
  echo "NG: 入力が使えない場合に 2 を返しませんでした（返した値: ${rc}。1 = 逸脱 / 0 = 適合はどちらも誤り）" >&2
  exit 1
fi
echo "OK: 入力が使えない場合（実行ファイルの不在）は 2 で落ちた"

# 検査器は起動したアプリを回収まで行って終了する（ゾンビは残らない）。ここでは**常駐が 0 件**で
# あることを確かめる（残ると後続の段が単一インスタンスの機構に引き継がれる）。
remaining=$(ps -Ao stat=,comm= | awk '$1 !~ /^Z/ && $2 == "jxcel" { n += 1 } END { print n + 0 }')
if [ "$remaining" -ne 0 ]; then
  echo "NG: 後始末のあとに jxcel のプロセスが残っています（${remaining} 件）" >&2
  exit 1
fi
echo "OK: 後始末: jxcel のプロセスは残っていない（ゾンビを除いて 0 件）"
echo "OK: macOS: 引き金の走行（正例 2 回・負の対照・入力の不在）を 3 つとも実測した（画面の見え方とメニューからの保存は別の段の担当）"

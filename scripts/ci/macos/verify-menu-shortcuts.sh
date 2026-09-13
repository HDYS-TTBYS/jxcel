#!/bin/bash
# macOS の段（tasks.md 10.6）。検証用の形を起動し、**アプリが組み立てて `set_menu` へ渡した
# アプリ全体のメニュー**を記録から読む（AX の許可が無いランナーでは、これが唯一の客観的な
# 観測である）。記録・アプリの出力は**別のファイル**にする（10.4 と同じ理由 — 同一ファイル
# だと 2 人の書き手が同じ offset を触りうる）。
#
# (2) フォーカスへの作用と (1) の通知の seam は、同じランナーで走る `menu.rs` のテストで
# 名指しに確かめ、その結果を出力に残す（**プラットフォームの配送ではない**ことを明記する）。

set -euo pipefail
# 4.4 の解決（macOS は $HOME/Library/Logs/{識別子}。`logs` 接尾辞は付かない）。
record="$HOME/Library/Logs/com.jxcel.app/jxcel.log"
output="$RUNNER_TEMP/jxcel-10-6-macos-output.log"
mkdir -p "$(dirname "$record")" "$(dirname "$output")"
echo "診断記録: $record"
echo "アプリの出力: ${output}（記録とは別ファイル）"
verify="target/release/jxcel"
if [ ! -x "$verify" ]; then
  echo "NG: 検証用の形の実行ファイルがありません: $verify" >&2
  exit 1
fi
# 記録は消さない（起動中のアプリが書き続ける）。**段の開始時の行数**を取り、それ以降だけを
# 調べる（前の段の行で偽の成功をしない）。
phase=0
if [ -f "$record" ]; then
  phase=$(wc -l < "$record" | tr -d ' ')
fi
: > "$output"
"$verify" >"$output" 2>&1 &
pid=$!
cleanup() {
  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

# 起動の直後に書かれる「配置の記録」を待つ（登録はウィンドウの生成より前である）。
line=""
deadline=$(( $(date +%s) + 60 ))
while [ "$(date +%s)" -lt "$deadline" ]; do
  line=$(tail -n "+$((phase + 1))" "$record" 2>/dev/null |
    grep -E '\[検証\] メニューを配置した: ' | tail -n 1 || true)
  [ -n "$line" ] && break
  if ! kill -0 "$pid" 2>/dev/null; then
    break
  fi
  sleep 1
done
if [ -z "$line" ]; then
  echo "NG: 配置の記録（[検証] メニューを配置した: …）が 60 秒以内に現れませんでした（--features verification-triggers のビルドではないか、起動できていない）" >&2
  tail -n 40 "$record" >&2 || true
  tail -n 40 "$output" >&2 || true
  exit 1
fi
echo "検証（配置の記録）: $line"
# **期待する位置は macOS の慣習に従う**（Linux / Windows の段とは違う）:
# 組み込みの「終了」と検証専用の項目は **`builtin_quit_path()` が macOS では
# アプリケーションメニュー（`jxcel`）を指す**ため、そこへ置かれる
# （`menu.rs` の `builtin_quit_path` と、それを固定するクレートのテスト
# `the_builtin_quit_item_is_placed_in_the_platform_conventional_submenu`）。
# ファイルメニューに入るのは「開く…」と**ドキュメントのセッションの「新規」「保存」**
# （タスク 3.6。`crate::menu::FILE_MENU_LABEL` を位置に使うので Linux / Windows と同じ位置である）
# であり、診断の 3 項目は 診断 に入る。**「新規」「保存」はアプリケーションメニューの項目ではない** —
# アプリケーションメニューへ畳み込まれるのは `builtin_quit_path()` を使う組み込みの「終了」と、
# 検証専用の 2 項目（`menu.rs` の検証用の登録が同じ位置を使う）だけである。
# **以前この一覧は Linux / Windows と同じ「ファイル > 終了」を要求していた** —
# macOS のアプリ全体のメニューでは成立せず、この段が初めて走った 2026-09-12 に露見した。
for expected in \
  "配置=アプリ全体" \
  "項目数=9" \
  "app-shell.open-document(ファイル > 開く…, ショートカット=super+KeyO)" \
  "app-shell.quit(jxcel > 終了, ショートカット=super+KeyQ)" \
  "verification.document-only(jxcel > 検証: ドキュメント付きのみ, ショートカット=(なし))" \
  "verification.probe(jxcel > 検証: 対象ウィンドウを記録, ショートカット=shift+super+KeyJ)" \
  "document-session.new(ファイル > 新規, ショートカット=super+KeyN)" \
  "document-session.save(ファイル > 保存, ショートカット=super+KeyS)" \
  "app-shell.diagnostics-export(診断 > 診断情報を書き出す…, ショートカット=shift+super+KeyE)" \
  "app-shell.diagnostics-log-location(診断 > 記録の保存場所を表示, ショートカット=shift+super+KeyL)" \
  "app-shell.diagnostics-verbosity(診断 > 記録の詳細度…, ショートカット=shift+super+KeyV)"
do
  case "$line" in
    *"$expected"*) ;;
    *)
      echo "NG: 配置の記録に ${expected} がありません（アプリ全体のメニューの位置・表示名・解決済みの綴りが期待と違う）" >&2
      exit 1
      ;;
  esac
done
echo "OK: macOS: アプリ全体のメニュー（配置=アプリ全体）に 9 項目が期待の位置・表示名で並び、ショートカットは super+…（Cmd）へ解決されている"

# 記録が「配置」だけを覆うことを明示する（配布物のメニューは AX 無しには読めない）。
echo "限界: macOS ではアクセシビリティ許可が無いため、ネイティブのメニューを外部から読めず、クリック・キー入力を配送できない（actions/runner-images#8214）"
echo "限界: したがって (2) フォーカス先への作用の実測は Linux と Windows の段が担い、この段は通知の seam と対象の規則をクレートのテストで確かめる"

# (2) フォーカスと (1) の通知の seam（`テストが 1 本だけの dispatch を通る`）。
echo "検証（macOS）: menu.rs のテスト（項目 → 登録元の通知・対象ウィンドウの解決）"
cargo test -p jxcel --bin jxcel menu:: 2>&1 | tee "$RUNNER_TEMP/jxcel-10-6-menu-tests.txt"
grep -q 'test result: ok' "$RUNNER_TEMP/jxcel-10-6-menu-tests.txt" ||
  { echo "NG: menu.rs のテストが成立しませんでした" >&2; exit 1; }
echo "OK: macOS: 通知の seam と対象ウィンドウの解決の 20 テストがこのランナーで成立した（プラットフォームの配送そのものは未検証）"

# 検証用の形が残っていないことを確かめる（後続の段に残さない）。**ゾンビは数えない**
# （終了したプロセスが名前を保持しても常駐してはいない。10.5 の Linux 検査と同じ判断）。
cleanup
if kill -0 "$pid" 2>/dev/null; then
  echo "NG: 後始末の後に jxcel（pid=${pid}）が残っています" >&2
  exit 1
fi
remaining=$(ps -Ao stat=,comm= | awk '$1 !~ /^Z/ && $2 == "jxcel" { n += 1 } END { print n + 0 }')
if [ "$remaining" -ne 0 ]; then
  echo "NG: 後始末の後に jxcel のプロセスが残っています（${remaining} 件）" >&2
  ps -Ao pid=,stat=,comm= | awk '$3 == "jxcel"' >&2
  exit 1
fi
echo "検証（後始末）: jxcel のプロセスは残っていない（ゾンビを除いて 0 件）"

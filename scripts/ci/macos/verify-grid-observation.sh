#!/bin/bash
# macOS の段（**tasks.md 9.2** / 要件 11.1, 11.2, 11.3, 12.1, 12.2, 12.3, 12.4）。
#
# 10 万行のシートを開き、末尾へ移動し、セルを編集し、取り消して戻すまでを**実際に起動して**
# 観測する。判定の本体は POSIX sh の `scripts/check-grid-observation.sh`（**3 OS のランナーで
# 同じものを走らせる**）。**この段は Linux の段 `scripts/ci/linux/verify-grid-observation.sh` の
# 鏡であり、恒久である** — 1.6 の一時的な段（`verify-render-traversal.sh`）は本段が入った時点で
# 取り除いた（`research.md` の実測は残る）。
#
# 段が足すのは OS 固有の 3 点だけである:
#
#   1. **記録の位置**（4.4 の解決。macOS は `$HOME/Library/Logs/{識別子}`。`logs` 接尾辞は付かない）
#   2. **実行ファイルの在り処**（検証用の形は `target/release/jxcel`。反証に渡す配布物は `.app` の
#      中の実行ファイル `target/release/bundle/macos/jxcel.app/Contents/MacOS/jxcel` —
#      **インストーラを使わずそのまま起動する**。`verify-bundle-window.sh` / `verify-document-session.sh`
#      と同じ解決）
#   3. **標本の生成**（10 万行 × 30 列を 1.4 の生成器から `target/observation/` へ書く）
#
# # 何を確かめるか（Linux の段と同じ。判定は検査器が持つ）
#
#   1. **通常の起動**: `--expect-paint=成立` で走査（要件 11.1 / 12.1）・編集の反映（11.3）・
#      取り消しの往復の成立。
#   2. **描画を成立させない条件の起動**: `--expect-paint=不成立` で、無内容の領域のまま留まらず
#      識別できる情報が提示され、`paint_failed` の記録が残ること（要件 12.2 / 12.3）。
#   3. **反証**: 配布物（`.app` の中の実行ファイル）を渡すと検査器が非 0 で落ちること。理由が
#      「観測の行の不在」であることと、**配布物が起動してウィンドウを開いたこと**（記録の行）
#      まで要求する（起動しなかった実行も事実の欠落で落ちるので、理由を見ないと錠前の実測に
#      ならない）。**この段が空回りしていないことの錠前である**（`verification.md`「無いことを
#      確かめる検査には負の対照を付ける」）。
#
# # アプリの出力を記録と混ぜないこと（10.4 / 10.6 の macOS の段が発見した罠）
#
# macOS では記録の保存先が `$HOME/Library/Logs` の下にあり、**同じファイルへ標準出力を向けると
# 2 人の書き手が同じ offset を触りうる** — アプリ（`TargetKind::Stdout`）が先頭から上書きし、
# 記録機構（`TargetKind::Folder`）が末尾へ書く（10.4 が実測した偽の失敗）。**検査器は自分で
# アプリの出力を自分の作業領域のファイルへ向ける**ので、この段は記録と混ざらない — 段の側で
# アプリの出力を記録へ流す経路を作らない（`verify-document-session.sh` と同じ解決。macOS の段が
# アプリの出力を `$RUNNER_TEMP` へ置くのは、段自身がアプリを起動する場合の規約である）。
#
# # 証拠が何を証明し、何を証明しないか
#
#   - 証明する: このランナーの**実物の検証用の形**を起動し、10 万行の走査と編集と取り消しが
#     成立すること、描画を成立させない条件で提示と記録が成ること、配布物では観測の行が読めない
#     こと。判定は検査器が要件値で行う（この段は判定を足さない）。
#   - **証明しない**: 観測の行の**読み口**（アクセシビリティの木か、記録か）が macOS で成立する
#     ことは検査器の側の責務であり、この段はそれを証明しない（段は検査器を呼ぶだけで、読み口を
#     足さない）。画面の見え方・レイアウト・描画の画素は見ない。配布物の署名と公証も見ない。
#   - **この段は macOS では走らせられない。** この開発機に macOS の実行環境が無いので、ここで
#     確かめられるのは `bash -n` による構文の確認と、既存の macOS の段との対比だけである。
#     実行（実物の起動と観測）は **CI の macOS ランナーでのみ**確かめる
#     （`verification.md`「ローカルで閉じられないもの」）。
#
# # 前提（CI の macOS ランナー）
#
# `xvfb-run` のような仮想ディスプレイの包みは要らない（OS が画面を持つ）。検証用の形は先に
# `--features verification-triggers` でビルドされ、配布物は直前の「Build platform bundle」が
# 作っている（1.5 の段）。検査器は起動したアプリを**回収まで行って**終了するので、2 回目の走行が
# 単一インスタンスの機構に引き継がれることはない（段の側でプロセスを止める必要は無い）。
set -euo pipefail

verify=target/release/jxcel
if [ ! -x "$verify" ]; then
  echo "NG: 検証用の形の実行ファイルがありません: $verify（先に --features verification-triggers のビルドを走らせてください）" >&2
  exit 2
fi

# 配布物（`.app` の中の実行ファイル）。**インストーラを使わずそのまま起動する**（反証に渡す）。
# 無ければ 2 で落ちる — 反証はこの段の錠前であり、無いまま緑にしない
# （`verification.md`「入力が無いときに 0 を返さない」）。
shipping=target/release/bundle/macos/jxcel.app/Contents/MacOS/jxcel
if [ ! -x "$shipping" ]; then
  echo "NG: 配布物（.app の実行ファイル）がありません: $shipping（直前の「Build platform bundle」が先に必要）" >&2
  exit 2
fi

# 記録の置き場（4.4 の解決。macOS は $HOME/Library/Logs/{識別子}。`logs` 接尾辞は付かない）。
record="$HOME/Library/Logs/com.jxcel.app/jxcel.log"
mkdir -p "$(dirname "$record")"
echo "診断記録: $record"

# **標本はリポジトリの中（`target/`）へ書く。** 生成器は `cargo` の側で走るので、**リポジトリの
# 外のファイルシステムはアプリと共有されない**環境がある（Linux の段が実測した）。`target/` は
# リポジトリの中で、かつ配布物に入らない（`.gitignore`）。
mkdir -p target/observation
sample="target/observation/grid-observation-$$.jxcel"
trap 'rm -f "$sample"' EXIT

echo "標本を作る: 10 万行 × 30 列（1.4 の生成器）"
cargo run --release -q -p data-grid --example make-large-sheet --features verification-samples -- \
  100000 30 "$sample"

# 検査器を 1 回走らせる。第 1 引数は実行ファイル、第 2 引数は標本、第 3 引数は記録、残りは
# 検査器の引数である。**引き金（`JXCEL_VERIFICATION_*`）は検査器が設定する** — この段は
# 環境変数を足さない（3 OS で同じ判定を閉じるため）。
run_check() {
  _app=$1
  shift 1
  sh scripts/check-grid-observation.sh "$_app" "$sample" "$record" "$@"
}

echo "観測 1/2: 通常の起動（走査・編集・取り消しの成立）"
run_check "$verify" --timeout=240 --expect-paint=成立

echo "観測 2/2: 描画を成立させない条件の起動（要件 12.2 / 12.3）"
run_check "$verify" --timeout=120 --expect-paint=不成立

# 反証: **配布物は検証専用の初期画面を読まない**（9.7 の片付けの規約）ので、観測の画面は
# 現れず観測の行は読めない。検査器は非 0 で落ちなければならない（落ちなければ、検査器が
# 観測の行を本当に見ていないことになる）。
echo "反証: 配布物（検証用の初期画面を読まない）で検査が非 0 で落ちること"
rc=0
output=$(run_check "$shipping" --timeout=60 --expect-paint=成立 2>&1) || rc=$?
printf '%s\n' "$output" | tail -n 20
if [ "$rc" -eq 0 ]; then
  echo "NG: 配布物に対して検査が成功してしまった（観測の行が無いのに通っている）" >&2
  exit 1
fi
if [ "$rc" -eq 2 ]; then
  echo "NG: 反証が「入力が使えない」で落ちた（検査器が走っていない）" >&2
  exit 1
fi
# **落ちた理由が「観測の行の不在」であること**（起動しなかった実行も事実の欠落で落ちる）。
if ! printf '%s\n' "$output" | grep -q '観測の行'; then
  echo "NG: 反証が「観測の行が読めない」以外の理由で落ちた（検査器が動いていない可能性がある）" >&2
  exit 1
fi
# **配布物が実際に起動してウィンドウを開いたこと**（検査器が出す起動の形跡の行）。
if ! printf '%s\n' "$output" | grep -q 'ウィンドウ:'; then
  echo "NG: 反証で配布物のウィンドウが観測されていません（起動しなかった実行を錠前の実測と取り違えない）" >&2
  exit 1
fi
echo "反証: 期待どおり非 0 で落ちました（配布物は起動したが、観測の画面を要求しないため観測の行が無い）"

echo "OK: 3 OS の観測の段（macOS）— 走査・編集・取り消しと、描画不成立の提示と記録が成立した"

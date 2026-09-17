#!/bin/bash
# Linux の段（**tasks.md 9.2** / 要件 11.1, 11.2, 11.3, 12.1, 12.2, 12.3, 12.4）。
#
# 10 万行のシートを開き、末尾へ移動し、セルを編集し、取り消して戻すまでを**実際に起動して**
# 観測する。判定の本体は POSIX sh の `scripts/check-grid-observation.sh`（ローカルでも同じものを
# 実行できる）。**この段は恒久である** — 1.6 の一時的な段（`verify-render-traversal.sh`）は
# 本段が入った時点で取り除いた（`research.md` の実測は残る）。
#
# # 何を観測するか（画面が書く 1 行 + 記録の行）
#
# 検証用の初期画面（`JXCEL_VERIFICATION_INITIAL_SCREEN=grid-observation`）は**製品の画面
# （`GridScreen`）そのもの**を描き、観測の結果を 1 行の `aria-label` に書く。検査器は
# アクセシビリティの木からそれを読み、**要件値で判定する**（1 秒 / 100 ミリ秒 / 16.67 ミリ秒・
# 末尾への到達・取り消しの成立）。
#
# **描画不成立の陽性の観測（要件 12.2 / 12.3）**は 2 回目の起動で行う —
# `JXCEL_VERIFICATION_GRID_PAINT_FAILURE=1` を付けると、観測の画面が**面の canvas を空の
# canvas と入れ替える**（WebKitGTK の「DOM はあるが何も塗られない」症状そのもの）。製品の
# 描画成立の検査（7.6）と告知・記録（9.3）が不成立を観測し、検査器は**告知**と
# **`diagnostics_record_render` の記録行**の双方を要求する。
#
# # 前提（CI の Linux ランナー）
#
# `scripts/ci/linux/install-system-deps.sh` が `xvfb` / `x11-utils` / `at-spi2-core` /
# `dbus-x11` 相当を導入している。**アクセシビリティの橋（`GTK_MODULES=gail:atk-bridge`）が
# 要る** — 観測の行はアクセシビリティの木から読むためである。`dbus-run-session` があれば
# 専用のセッションバスを使う（10.6 の段と同じ退避）。
#
# # 標本
#
# 10 万行 × 30 列の**実在する文書**を 1.4 の生成器から書き出す（`crates/data-grid` の例。
# 非既定の feature `verification-samples` を要求するので、既定のビルドではコンパイルすら
# されない）。**標本の生成器を写さない** — ベンチと同じ 1 つの源を使う。
set -euo pipefail

verify=target/release/jxcel
if [ ! -x "$verify" ]; then
  echo "NG: 検証用の形の実行ファイルがありません: $verify（先に検証用のビルドを走らせてください）" >&2
  exit 2
fi

# 記録の置き場（4.4 の解決。空の XDG_DATA_HOME は未設定として $HOME へ落ちる）。
record="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs/jxcel.log"
echo "診断記録: $record"

# **標本はリポジトリの中（`target/`）へ書く。** 生成器は `cargo` の側で走るので、**リポジトリの
# 外のファイルシステムはアプリと共有されない**（この機械の `cargo` は podman の中で走る shim で
# あり、バインドされるのはリポジトリだけである）。実測: `/tmp` へ書いた標本は生成器の報告どおりの
# 大きさで書けたのに、**アプリからは読めなかった**（`invalid container: … Could not find EOCD`
#  — アプリが見たのは別のファイルであり、`mktemp` が作った空のファイルだった）。`target/` は
# リポジトリの中で、かつ配布物に入らない（`.gitignore`）。
mkdir -p target/observation
sample="target/observation/grid-observation-$$.jxcel"
trap 'rm -f "$sample"' EXIT

echo "標本を作る: 10 万行 × 30 列（1.4 の生成器）"
cargo run --release -q -p data-grid --example make-large-sheet --features verification-samples -- \
  100000 30 "$sample"

# **仮想ディスプレイを自分で用意する**（兄弟の段と同じ形）。検査器は Linux で `DISPLAY` を
# 要求する — 要求しないと、表示サーバの無いランナーで**観測の前に** 2 で落ちる（CI の実測）。
# 表示サーバが既にある機械（開発機）では重ねない。
with_display() {
  # **表示サーバが既にあるなら重ねない**（開発機は実画面で走らせたい。Xvfb を重ねると
  # ソフトウェアラスタライザになり、所要時間の要件の前提（11.7）から外れてしまう）。
  if [ -n "${DISPLAY:-}" ]; then
    "$@"
  elif command -v xvfb-run >/dev/null 2>&1; then
    xvfb-run -a --server-args="-screen 0 1280x1024x24" "$@"
  else
    "$@"
  fi
}

# 検査器を 1 回走らせる。第 1 引数は実行ファイル、第 2 引数は標本、第 3 引数は記録、残りは
# 検査器の引数である。**`dbus-run-session` はアクセシビリティの木（貼り付けの項目の活性化）の
# ために要る**ので、仮想ディスプレイの内側で作る。
run_check() {
  _app=$1
  shift 1
  if command -v dbus-run-session >/dev/null 2>&1; then
    with_display dbus-run-session -- env GTK_MODULES=gail:atk-bridge \
      sh scripts/check-grid-observation.sh "$_app" "$sample" "$record" "$@"
  else
    with_display env GTK_MODULES=gail:atk-bridge \
      sh scripts/check-grid-observation.sh "$_app" "$sample" "$record" "$@"
  fi
}

echo "観測 1/2: 通常の起動（走査・編集・取り消しの成立）"
run_check "$verify" --timeout=360 --expect-paint=成立 \
  --expect-paste=成立 \
  --expect-items=nested_expansion,insert_row,sort_then_delete,violation_reason,reference_rows,sheet_switch_undo,replace_document,paste_through_menu

echo "観測 2/2: 描画を成立させない条件の起動（要件 12.2 / 12.3）"
run_check "$verify" --timeout=120 --expect-paint=不成立

# 反証: **配布物は検証専用の初期画面を読まない**（9.7 の片付けの規約）ので、観測の画面は
# 現れず観測の行は読めない。検査器は非 0 で落ちなければならない（落ちなければ、検査器が
# 観測の行を本当に見ていないことになる）。**この段が空回りしていないことの錠前である**
# （`verification.md`「無いことを確かめる検査には負の対照を付ける」）。
if compgen -G "target/release/bundle/appimage/*.AppImage" >/dev/null; then
  echo "反証: 配布物（検証用の初期画面を読まない）で検査が非 0 で落ちること"
  appimage=$(compgen -G "target/release/bundle/appimage/*.AppImage" | head -n 1)
  export APPIMAGE_EXTRACT_AND_RUN=1
  rc=0
  output=$(run_check "$appimage" --timeout=60 --expect-paint=成立 2>&1) || rc=$?
  printf '%s\n' "$output" | tail -n 20
  if [ "$rc" -eq 0 ]; then
    echo "NG: 配布物に対して検査が成功してしまった（観測の行が無いのに通っている）" >&2
    exit 1
  fi
  if ! printf '%s\n' "$output" | grep -q '観測の行'; then
    echo "NG: 反証が「観測の行が読めない」以外の理由で落ちた（検査器が動いていない可能性がある）" >&2
    exit 1
  fi
  if ! printf '%s\n' "$output" | grep -q 'ウィンドウ:'; then
    echo "NG: 反証で配布物のウィンドウが観測されていません（起動しなかった実行を錠前の実測と取り違えない）" >&2
    exit 1
  fi
  echo "反証: 期待どおり非 0 で落ちました（配布物は起動したが、観測の画面を要求しないため観測の行が無い）"
  rm -rf /tmp/appimage_extracted_* 2>/dev/null || true
else
  echo "反証: 配布物が無いので飛ばします（配布物のビルドは本段の前の段が作る）"
fi

echo "OK: 3 OS の観測の段（Linux）— 走査・編集・取り消しと、描画不成立の提示と記録が成立した"

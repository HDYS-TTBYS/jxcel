#!/bin/bash
# Linux の段（**tasks.md 5.2** / 要件 1.2, 1.3, 1.5, 2.1, 2.3, 2.5, 5.1, 5.5, 6.1, 6.4, 8.2, 8.3,
# 9.1, 9.2, 9.3）。
#
# 実起動のマクロの観測を **Linux のランナーで**閉じる。判定の本体は POSIX sh の
# `scripts/check-macro-observation.sh`（**3 OS のランナーで同じものを走らせる**。ローカルでも
# 同じものを実行できる）。**この段は恒久である** — `data-grid` の 9.2 の段
# （`scripts/ci/linux/verify-grid-observation.sh`）と同じ形であり、そちらが確立した「記録を
# 読み口にして要件値で判定する」形に乗っている（2 つ目の流儀を作らない）。
#
# # 何を観測するか（検査器の doc が正本）
#
#   1. **実行の成功と変更の件数** — `標本の記入` が 1 セル書く（要件 2.1, 5.1, 5.5）
#   2. **失敗の理由とフレーム** — `標本の失敗` が 3 行目で投げ、その原位置がフレームに出る
#      （要件 9.1–9.3）
#   3. **能力の拒否** — `標本の拒否` が宣言の無い `host.fileRead` を呼び、能力の名前つきで
#      拒まれる（要件 8.3, 9.2）
#   4. **打ち切りと、その後の操作可能性** — `標本の打ち切り,標本の記入` を**同じ起動で**順に
#      走らせ、30 秒で打ち切られたあとに続けて 1 セル書く（要件 6.1, 6.4）
#   5. **保存と開き直しの往復** — 標本の写しを `JXCEL_VERIFICATION_SESSION=open,edit,2,save` で
#      保存させ、開き直して `標本の往復` を走らせる（要件 1.2, 1.3, 1.5）
#
# **判定は要件値で行う**（一覧 5 件・変更 1 セル・打ち切りは 30 秒以上・フレームの原位置）。
# 記録のどの欄がどの要件の材料かは検査器の冒頭の表に 1 つだけ書いてある。
#
# # 前提（CI の Linux ランナー）
#
# `scripts/ci/linux/install-system-deps.sh` が `xvfb` などを導入している。**この段は
# アクセシビリティの橋（`GTK_MODULES=gail:atk-bridge`）も `dbus-run-session` も要らない** —
# 検査器は画面もウィンドウツリーも読まず、**診断記録だけを読む**（3 OS で同じ判定を閉じるため。
# 5.1 の申し送りと同じ理由である）。`python3` は観測の行（JSON）を読むために要る（ランナーに在る）。
#
# # 標本
#
# マクロ入りの文書を **5.1 の生成器**から `target/observation/` へ書き出す
# （`crates/macro-runtime/examples/make-macro-document.rs`。非既定の feature
# `verification-samples` を要求するので、既定のビルドではコンパイルすらされない）。
# **標本の生成器を写さない** — 5.1 が置いた 1 つの源を使う（検査器は標本の中身を作らない）。
#
# **標本はリポジトリの中（`target/`）へ書く。** 生成器は `cargo` の側で走るので、**リポジトリの
# 外のファイルシステムはアプリと共有されない**（この機械の `cargo` は podman の中で走る shim で
# あり、バインドされるのはリポジトリだけである。9.2 の段が `/tmp` で実測した）。`target/` は
# リポジトリの中で、かつ配布物に入らない（`.gitignore`）。
#
# # 反証（この段が空回りしていないことの錠前）
#
# **配布物（AppImage。既定のビルド）を渡すと検査器は非 0 で落ちる。** 配布物は検証専用の環境
# 変数を読まないので観測の行が 1 行も現れない。ただし**それだけでは錠前の実測にならない** —
# 起動していない実行も同じ理由で落ちるためである。したがってこの段は、(1) 落ちた理由が
# 「観測の行の不在」であることと、(2) **配布物が実際に起動してウィンドウを開いたこと**
# （検査器が出す `ウィンドウ:` の行）の両方を要求する（`verification.md`「無いことを確かめる
# 検査には負の対照を付ける」）。
#
# # macOS / Windows は走らせていない
#
# この開発機に macOS / Windows の実行環境は無い。**この段（Linux）は実測したが、macOS と
# Windows の段（`scripts/ci/macos/verify-macro-observation.sh` /
# `scripts/ci/windows/verify-macro-observation.ps1`）は走らせていない** — ここで確かめられるのは
# `bash -n` による構文の確認と、兄弟の段（9.2）との対比だけである。実行は **CI の macOS /
# Windows のランナーでのみ**確かめる（`verification.md`「ローカルで閉じられないもの」）。
#
# # 終了コード
#   0 = 適合 / 1 = 逸脱（検査器が非 0） / 2 = 入力が使えない（実行ファイル・生成器の失敗）
set -euo pipefail

verify=target/release/jxcel
if [ ! -x "$verify" ]; then
  echo "NG: 検証用の形の実行ファイルがありません: $verify（先に検証用のビルドを走らせてください）" >&2
  exit 2
fi

# 記録の置き場（4.4 の解決。空の XDG_DATA_HOME は未設定として $HOME へ落ちる）。
record="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs/jxcel.log"
echo "診断記録: $record"

mkdir -p target/observation
sample="target/observation/macro-observation-$$.jxcel"
trap 'rm -f "$sample"' EXIT

echo "標本を作る: シート「在庫」（3 行 × 2 列）＋マクロ 5 件（5.1 の生成器）"
cargo run --release -q -p macro-runtime --example make-macro-document \
  --features verification-samples -- "$sample"

# **仮想ディスプレイを自分で用意する**（兄弟の段と同じ形）。検査器は Linux で `DISPLAY` を
# 要求する — 要求しないと、表示サーバの無いランナーで**観測の前に** 2 で落ちる（CI の実測）。
# 表示サーバが既にある機械（開発機）では重ねない。
with_display() {
  if [ -n "${DISPLAY:-}" ]; then
    "$@"
  elif command -v xvfb-run >/dev/null 2>&1; then
    xvfb-run -a --server-args="-screen 0 1280x1024x24" "$@"
  else
    "$@"
  fi
}

# 検査器を 1 回走らせる。第 1 引数は実行ファイル、第 2 引数は標本、第 3 引数は記録である。
# **引き金（`JXCEL_VERIFICATION_*`）は検査器が設定する** — この段は環境変数を足さない
# （3 OS で同じ判定を閉じるため）。
run_check() {
  _app=$1
  shift 1
  with_display sh scripts/check-macro-observation.sh "$_app" "$sample" "$record" "$@"
}

echo "観測: 5 つの筋書き（成功・失敗・拒否・打ち切りと復帰・保存と開き直し）"
run_check "$verify" --timeout=180

# 反証: **配布物は検証専用の環境変数を読まない**ので、観測の行は現れず検査器は非 0 で落ちる。
if compgen -G "target/release/bundle/appimage/*.AppImage" >/dev/null; then
  echo "反証: 配布物（検証専用の引き金を読まない）で検査が非 0 で落ちること"
  appimage=$(compgen -G "target/release/bundle/appimage/*.AppImage" | head -n 1)
  export APPIMAGE_EXTRACT_AND_RUN=1
  rc=0
  output=$(run_check "$appimage" --timeout=60 2>&1) || rc=$?
  printf '%s\n' "$output" | tail -n 20
  if [ "$rc" -eq 0 ]; then
    echo "NG: 配布物に対して検査が成功してしまった（観測の行が無いのに通っている。既定のビルドに検証専用の経路が入っている）" >&2
    exit 1
  fi
  if [ "$rc" -eq 2 ]; then
    echo "NG: 反証が「入力が使えない」で落ちた（検査器が走っていない）" >&2
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
  echo "反証: 期待どおり非 0 で落ちました（配布物は起動したが、観測の行が現れない）"
  rm -rf /tmp/appimage_extracted_* 2>/dev/null || true
else
  echo "反証: 配布物が無いので飛ばします（配布物のビルドは本段の前の段が作る）"
fi

echo "OK: 3 OS の観測の段（Linux）— 実行の成功と変更の件数・失敗の理由とフレーム・能力の拒否・打ち切りとその後の操作・保存と開き直しの往復が成立した"

#!/bin/bash
# Linux の段（**tasks.md 5.2** / 要件 1.2, 1.3, 1.5, 2.1, 2.3, 2.5, 2.6, 5.1, 5.5, 6.1, 6.4, 8.2,
# 8.3, 9.1, 9.2, 9.3, 11.1, 11.2, 11.3）。
#
# 実起動のマクロの観測を **Linux のランナーで**閉じる。判定の本体は POSIX sh の
# `scripts/check-macro-observation.sh`（**3 OS のランナーで同じものを走らせる**。ローカルでも
# 同じものを実行できる）。**この段は恒久である** — `data-grid` の 9.2 の段
# （`scripts/ci/linux/verify-grid-observation.sh`）と同じ形であり、そちらが確立した「記録を
# 読み口にして要件値で判定する」形に乗っている（2 つ目の流儀を作らない）。
#
# # 何を観測するか（検査器の doc が正本）
#
#   1. **実行の成功と変更の件数** — `標本の記入` が 1 セル書き、3 行の出力を出す。製品の記録の
#      1 行が成否・変更の有無・**出力の行数**を運ぶ（要件 2.1, 2.6, 5.1, 5.5）
#   2. **失敗の理由とフレーム** — `標本の失敗` が 3 行目で投げ、その原位置がフレームに出る
#      （要件 9.1–9.3）
#   3. **能力の拒否** — `標本の拒否` が宣言の無い `host.fileRead` を呼び、能力の名前つきで
#      拒まれる（要件 8.3, 9.2）
#   4. **打ち切りと、その後の操作可能性** — `標本の打ち切り,標本の記入` を**同じ起動で**順に
#      走らせ、30 秒で打ち切られたあとに続けて 1 セル書く（要件 6.1, 6.4）
#   5. **保存と開き直しの往復** — 標本の写しを `JXCEL_VERIFICATION_SESSION=open,edit,2,save` で
#      保存させ、開き直して `標本の往復` を走らせる（要件 1.2, 1.3, 1.5）
#   6. **10 万行 × 30 列の全行読み + 集計** — **実起動の観測で予算（10 秒）を判定する**
#      （要件 11.1, 11.3）
#   7. **1 万行 × 30 列の書き換え** — 同じく**実起動の観測で予算（5 秒）を判定する**。あわせて
#      変更の件数（30 万セル）を要件値として読む（要件 11.2, 11.3）
#
# **判定は要件値で行う**（一覧 5 件・変更 1 セル・打ち切りは 30 秒以上・フレームの原位置・予算の
# 2 本）。記録のどの欄がどの要件の材料かは検査器の冒頭の表に 1 つだけ書いてある。
#
# # 前提（CI の Linux ランナー）
#
# `scripts/ci/linux/install-system-deps.sh` が `xvfb` などを導入している。**この段は
# アクセシビリティの橋（`GTK_MODULES=gail:atk-bridge`）も `dbus-run-session` も要らない** —
# 検査器は画面もウィンドウツリーも読まず、**診断記録だけを読む**（3 OS で同じ判定を閉じるため。
# 5.1 の申し送りと同じ理由である）。`python3` は観測の行（JSON）と製品の記録の行を読むために
# 要る（ランナーに在る）。**`strings`（binutils）は負の対照の錠前に要る**（下の「反証」）。
#
# # 標本
#
# マクロ入りの文書を **5.1 の生成器**から `target/observation/` へ書き出す
# （`crates/macro-runtime/examples/make-macro-document.rs`。非既定の feature
# `verification-samples` を要求するので、既定のビルドではコンパイルすらされない）。
# **標本の生成器を写さない** — 5.1 が置いた 1 つの源を使う（検査器は標本の中身を作らない）。
#
# **2 つの種別を書き出す**: 標準（シート「在庫」＋マクロ 5 件）と予算
# （`--kind=budget`。10 万行 × 30 列 ＋マクロ 2 件）。検査器は前者でシナリオ 1〜5、後者で
# シナリオ 6〜7 を走らせる（予算を**実起動の観測**で判定する。要件 11.3）。
#
# **標本はリポジトリの中（`target/`）へ書く。** 生成器は `cargo` の側で走るので、**リポジトリの
# 外のファイルシステムはアプリと共有されない**（この機械の `cargo` は podman の中で走る shim で
# あり、バインドされるのはリポジトリだけである。9.2 の段が `/tmp` で実測した）。`target/` は
# リポジトリの中で、かつ配布物に入らない（`.gitignore`）。
#
# # 反証（この段が空回りしていないことの錠前）
#
# **既定 feature のビルドを渡すと検査器は非 0 で落ちる。** そのビルドは検証専用の環境変数を
# 読まないので観測の行が 1 行も現れない。ただし**それだけでは錠前の実測にならない** — 起動して
# いない実行も同じ理由で落ちるためである。したがってこの段は、(1) 落ちた理由が
# 「観測の行の不在」であることと、(2) **そのビルドが実際に起動してウィンドウを開いたこと**
# （検査器が出す `ウィンドウ:` の行）の両方を要求する（`verification.md`「無いことを確かめる
# 検査には負の対照を付ける」）。
#
# **負の対照の実行ファイルはこの段が現ソースから作る**（`cargo build --release -p jxcel
# --features tauri/custom-protocol`。既定 feature）。**配布物の出来合いを渡してはならない** —
# 2026-09-18 の独立検証で、この段が使っていた AppImage が**マクロ機能そのものを持たない機能前の
# ビルド**（`macro_list` の文字列が 0 件）であり、「観測の行が現れない」が**機能の不在**でも
# 成立してしまっていた（錠前が空回りした。§5 の指摘）。したがって使う前に**3 つの錠前**を確かめ、
# 確かめられなければ明示的に失敗する:
#
#   a. **機能を含む** — `macro_list` の文字列がある（マクロのコマンドが実行ファイルに入って
#      いる。機能前のビルドでは 0 件になる）
#   b. **検証専用の識別子を含まない** — `JXCEL_VERIFICATION` が 0 件（既定 feature のビルドで
#      あること）
#   c. **フロントエンドの資産を埋め込んでいる** — `dist/assets/index-*.js` の名前が実行ファイルに
#      ある。**`tauri/custom-protocol` を付けない `cargo build` はフロントエンドを埋め込まない**
#      （Tauri の production 経路を選ぶ feature であり、`npx tauri build` が付けるものと
#      同じである）。埋め込まれていないと窓は開いてもフロントエンドが走らず、**錠前が
#      「引き金を読まないから観測の行が出ない」ではなく「フロントエンドが無いから出ない」で
#      成立する** — この段の実測（2026-09-18）で、`cargo build --release -p jxcel` だけの
#      ビルドは初回描画が成立せず（`画面=(報告なし)`）、観測の行が 1 行も出なかった。
#
# **`dist/` は触らない。** `npx tauri build` は `beforeBuildCommand` で dist を作り直すため、
# 後続の段が使う検証用の dist を壊しうる。この段は cargo を直接呼ぶ。
#
# **前の段が作った配布物を反証に使わない理由**: 配布物は「配布物が検証専用の経路を持たない」
# ことの検査（10.5）が別にあり、そちらは出来合いの配布物を**抽出して**調べる（AppImage の
# 中身は squashfs である）。この段が要るのは「**いまのソースの既定 feature のビルド**が観測の
# 行を出さないこと」であり、その実体をこの段が作るのが最も強い（古い配布物でも錠前が成立して
# しまう穴を閉じる）。
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
#   0 = 適合 / 1 = 逸脱（検査器が非 0・錠前が成立しない） / 2 = 入力が使えない（実行ファイル・
#   生成器の失敗・strings の不在）
set -euo pipefail

verify=target/release/jxcel
if [ ! -x "$verify" ]; then
  echo "NG: 検証用の形の実行ファイルがありません: $verify（先に検証用のビルドを走らせてください）" >&2
  exit 2
fi
if ! command -v strings >/dev/null 2>&1; then
  echo "NG: strings が見つかりません（負の対照の実行ファイルが機能を含み、検証専用の識別子を含まないことの錠前に使います）" >&2
  exit 2
fi

# 記録の置き場（4.4 の解決。空の XDG_DATA_HOME は未設定として $HOME へ落ちる）。
record="${XDG_DATA_HOME:-$HOME/.local/share}/com.jxcel.app/logs/jxcel.log"
echo "診断記録: $record"

mkdir -p target/observation
sample="target/observation/macro-observation-$$.jxcel"
budget="target/observation/macro-budget-$$.jxcel"
# 負の対照の実行ファイルと、検証用の形の退避（下の「反証」）。**走行の終わりに必ず片付ける。**
default_binary="target/observation/jxcel-default-$$"
verify_saved="target/observation/jxcel-verify-$$"
trap 'rm -f "$sample" "$budget" "$default_binary" "$verify_saved"' EXIT

echo "標本を作る（標準）: シート「在庫」（3 行 × 2 列）＋マクロ 5 件（5.1 の生成器）"
cargo run --release -q -p macro-runtime --example make-macro-document \
  --features verification-samples -- "$sample"

echo "標本を作る（予算）: 10 万行 × 30 列（1.4 の生成器）＋マクロ 2 件（5.1 の生成器 --kind=budget）"
cargo run --release -q -p macro-runtime --example make-macro-document \
  --features verification-samples -- --kind=budget "$budget"

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
# （3 OS で同じ判定を閉じるため）。予算の標本はどの走行でも渡す（検査器が必須にする）。
run_check() {
  _app=$1
  shift 1
  with_display sh scripts/check-macro-observation.sh "$_app" "$sample" "$record" \
    "--budget-document=$budget" "$@"
}

echo "観測: 7 つの筋書き（成功と出力の記録・失敗・拒否・打ち切りと復帰・保存と開き直し・読みの予算・書き換えの予算）"
run_check "$verify" --timeout=180

# ---------------------------------------------------------------------------
# 反証: **既定 feature の現ソースのビルド**は検証専用の環境変数を読まないので、観測の行は
# 現れず検査器は非 0 で落ちる。**使う前に、その実体が機能を含み検証専用の識別子を含まないことを
# 確かめる**（錠前が空回りしないように。上の「反証」）。
# ---------------------------------------------------------------------------
echo "反証の実行ファイルを作る: 既定 feature の現ソース（cargo build --release -p jxcel --features tauri/custom-protocol）"
# **検証用の形を退避してから**既定 feature をビルドする（`target/release/jxcel` を上書きする
# ためである）。後続の段は検証用の形を使うので、**同じパスへ戻す**。
#
# **`tauri/custom-protocol` を付ける理由**: これを付けない `cargo build` はフロントエンドの資産を
# 実行ファイルへ埋め込まない（Tauri の production 経路を選ぶ feature であり、`npx tauri build` が
# 付けるものと同じである）。埋め込まれないと窓は開いてもフロントエンドが 1 行も走らず、錠前が
# **「引き金を読まないから観測の行が出ない」ではなく「フロントエンドが無いから出ない」**で成立
# してしまう — §5 で見つかった穴と同じ形の穴である。**この段は下の錠前 (c) で埋め込みを確かめる。**
# `dist/` を触らない（`npx tauri build` は `beforeBuildCommand` で dist を作り直すため、後続の段が
# 使う検証用の dist を壊しうる。この段は cargo を直接呼ぶ）。
# **直前に `dist/` を既定の形で作り直す**: `tauri/custom-protocol` はフロントエンドの資産を
# `dist/` から実行ファイルへ埋め込むため、**この段より前の検証用のビルドが残した `dist/`**
# （`JXCEL_VERIFICATION_BUILD=1` で作られたもの）をそのまま使うと、既定 feature のビルドなのに
# **検証専用の識別子が埋め込まれる**（2026-09-18 の実測: 錠前 (b) が 8 件を検出して落ちた。
# 錠前が正しく効いた例である）。順序の罠なので、ここで既定の形へ戻す。
# **`JXCEL_VERIFICATION_BUILD` を外して呼ぶ**（呼び出し元に残っていても既定の形にするため）。
env -u JXCEL_VERIFICATION_BUILD npm run build
cp "$verify" "$verify_saved"
cargo build --release -p jxcel --features tauri/custom-protocol
cp "$verify" "$default_binary"
cp "$verify_saved" "$verify"

# 錠前 (b): 検証専用の識別子を含まない（既定 feature のビルドである）。
# **`grep -q` を使わない** — 一致した時点で読むのをやめ、`strings` が EPIPE で非 0 終了し、
# `pipefail` の下で偽の失敗になる（macOS の段が実測した。10.5 の段と同じ規律）。
verification_markers=$(strings -a "$default_binary" | grep -c 'JXCEL_VERIFICATION' || true)
if [ "$verification_markers" -ne 0 ]; then
  echo "NG: 反証に使う実行ファイルが検証専用の識別子を含んでいます（JXCEL_VERIFICATION = ${verification_markers} 件。既定 feature のビルドではありません）" >&2
  exit 1
fi
# 錠前 (a): 機能を含む（マクロのコマンドが入っている）。**無ければ錠前が成立しない** —
# 機能前のビルドを反証に使うと、「観測の行が現れない」が機能の不在でも成立してしまう。
macro_markers=$(strings -a "$default_binary" | grep -c 'macro_list' || true)
if [ "$macro_markers" -eq 0 ]; then
  echo "NG: 反証に使う実行ファイルにマクロの機能がありません（macro_list の文字列が 0 件。機能前のビルドを錠前に使わない）" >&2
  exit 1
fi
# 錠前 (c): **フロントエンドの資産が埋め込まれている**（いまの dist の入口の資産の名前が
# 実行ファイルにある）。無ければ窓は開いてもフロントエンドが走らず、錠前が「フロントエンドが
# 無いから観測の行が出ない」で成立してしまう。
entry_asset_path=$(ls dist/assets/index-*.js 2>/dev/null | head -n 1 || true)
if [ -z "$entry_asset_path" ]; then
  echo "NG: フロントエンドの入口の資産がありません: dist/assets/index-*.js（先に npm run build が要ります）" >&2
  exit 2
fi
entry_asset=$(basename "$entry_asset_path")
if [ "$(strings -a "$default_binary" | grep -c -- "$entry_asset" || true)" -eq 0 ]; then
  echo "NG: 反証に使う実行ファイルにフロントエンドの資産が埋め込まれていません（${entry_asset} が 0 件。tauri/custom-protocol を付けずにビルドしていませんか）" >&2
  exit 1
fi
echo "錠前: 反証の実行ファイルは機能を含み（macro_list = ${macro_markers} 件）、検証専用の識別子を含まず（JXCEL_VERIFICATION = 0 件）、フロントエンドの資産を埋め込んでいる（${entry_asset}）"

echo "反証: 既定 feature の現ソースのビルド（検証専用の引き金を読まない）で検査が非 0 で落ちること"
rc=0
output=$(run_check "$default_binary" --timeout=60 2>&1) || rc=$?
printf '%s\n' "$output" | tail -n 20
if [ "$rc" -eq 0 ]; then
  echo "NG: 既定 feature のビルドに対して検査が成功してしまった（観測の行が無いのに通っている。既定のビルドに検証専用の経路が入っている）" >&2
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
  echo "NG: 反証で実行ファイルのウィンドウが観測されていません（起動しなかった実行を錠前の実測と取り違えない）" >&2
  exit 1
fi
echo "反証: 期待どおり非 0 で落ちました（既定 feature のビルドは起動したが、観測の行が現れない）"

echo "OK: 3 OS の観測の段（Linux）— 実行の成功と出力の行数の記録・失敗の理由とフレーム・能力の拒否・打ち切りとその後の操作・保存と開き直しの往復・読みの予算（10 万行）・書き換えの予算（1 万行）が成立した"

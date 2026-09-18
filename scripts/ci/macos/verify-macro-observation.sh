#!/bin/bash
# macOS の段（**tasks.md 5.2** / 要件 1.2, 1.3, 1.5, 2.1, 2.5, 2.6, 5.1, 5.5, 6.1, 6.4, 8.2,
# （**2.3 は挙げない** — 「戻り値と出力を 1 つの面に提示する」は面の側（4.4 の vitest）が判定する。
#  本段は診断の記録を読み口にしており、戻り値と出力の本文を運ばない。2026-09-18 の再検証の指摘）
# 8.3, 9.1, 9.2, 9.3, 11.1, 11.2, 11.3）。
#
# 実起動のマクロの観測を **macOS のランナーで**閉じる。判定の本体は POSIX sh の
# `scripts/check-macro-observation.sh`（**3 OS のランナーで同じものを走らせる**）。
# **この段は Linux の段 `scripts/ci/linux/verify-macro-observation.sh` の鏡であり、恒久である**
# — `data-grid` の 9.2 の段（`scripts/ci/macos/verify-grid-observation.sh`）と同じ形に乗っている
# （2 つ目の流儀を作らない）。
#
# # **この段は macOS では走らせていない（この開発機に macOS の実行環境が無い）**
#
# 実測したのは **Linux の段だけ**である。ここで確かめられるのは `bash -n` による構文の確認と、
# 兄弟の段（9.2）との対比だけである。**実行（実物の起動と観測）は CI の macOS ランナーでのみ
# 確かめる**（`verification.md`「ローカルで閉じられないもの」）。同じ理由で Windows の段
# （`scripts/ci/windows/verify-macro-observation.ps1`）も走らせていない。
#
# 段が足すのは OS 固有の 2 点だけである（9.2 の macOS の段と同じ規律）:
#
#   1. **記録の位置**（4.4 の解決。macOS は `$HOME/Library/Logs/{識別子}`。`logs` 接尾辞は付かない）
#   2. **標本の生成**（標準と予算の 2 種を 5.1 の生成器から `target/observation/` へ書く）
#
# # 何を確かめるか（検査器の doc が正本）
#
#   1. 実行の成功と変更の件数、製品の記録の成否・出力の行数・変更の有無（要件 2.1, 2.6, 5.1, 5.5）
#   2. 失敗の理由とフレーム（要件 9.1–9.3）
#   3. 能力の拒否（要件 8.3, 9.2）
#   4. 打ち切りと、その後の操作可能性（要件 6.1, 6.4）
#   5. 保存と開き直しの往復（要件 1.2, 1.3, 1.5）
#   6. 10 万行 × 30 列の読み + 集計の予算（要件 11.1, 11.3）
#   7. 1 万行 × 30 列の書き換えの予算（要件 11.2, 11.3）
#
# **判定は要件値で行う**（検査器が持つ。この段は判定を足さない）。**引き金
# （`JXCEL_VERIFICATION_*`）は検査器が設定する** — この段は環境変数を足さない。
#
# # 反証（この段が空回りしていないことの錠前）
#
# **既定 feature のビルドを渡すと検査器は非 0 で落ちる。** そのビルドは検証専用の環境変数を
# 読まないので観測の行が 1 行も現れない。ただし**それだけでは錠前の実測にならない** — 起動して
# いない実行も同じ理由で落ちるためである。したがってこの段は、(1) 落ちた理由が
# 「観測の行の不在」であることと、(2) **そのビルドが実際に起動してウィンドウを開いたこと**
# （検査器が出す `ウィンドウ:` の行）の両方を要求する。
#
# **負の対照の実行ファイルはこの段が現ソースから作る**（`cargo build --release -p jxcel
# --features tauri/custom-protocol`。既定 feature ＋ Tauri の production 経路）。**配布物（`.app` の中の実行ファイル）の出来合いを渡してはならない** — 2026-09-18 の
# 独立検証で、Linux の段が使っていた配布物が**マクロ機能そのものを持たない機能前のビルド**
# であり、「観測の行が現れない」が機能の不在でも成立してしまっていた（錠前が空回りした。§5 の
# 指摘）。使う前に (a) 機能を含むこと（`macro_list` の文字列があること）と (b) 検証専用の
# 識別子を含まないこと（`JXCEL_VERIFICATION` が 0 件）と (c) **フロントエンドの資産を
# 埋め込んでいること**（`dist/assets/index-*.js` の名前が実行ファイルにあること）を確かめ、
# 確かめられなければ明示的に失敗する。(c) が要るのは、`tauri/custom-protocol` を付けない
# `cargo build` が**フロントエンドを埋め込まない**ためである（窓は開くがフロントエンドが
# 走らず、錠前が「フロントエンドが無いから観測の行が出ない」で成立してしまう。Linux の段が
# 2026-09-18 に実測した）。したがってビルドは `--features tauri/custom-protocol` を付けて行う。
#
# # アプリの出力を記録と混ぜないこと（9.2 の macOS の段が発見した罠）
#
# macOS では記録の保存先が `$HOME/Library/Logs` の下にあり、**同じファイルへ標準出力を向けると
# 2 人の書き手が同じ offset を触りうる**。検査器は自分でアプリの出力を自分の作業領域のファイルへ
# 向けるので、この段は記録と混ざらない（段の側でアプリの出力を記録へ流す経路を作らない）。
#
# # 前提（CI の macOS ランナー）
#
# `xvfb-run` のような仮想ディスプレイの包みは要らない（OS が画面を持つ）。**検証用の形は先に
# `--features verification-triggers` でビルドされている**（この段はビルドを消費するだけである
# — 負の対照のための既定 feature のビルドだけは自分で行う）。`strings` は Xcode のコマンド
# ラインツールに在る（10.5 の段が同じものを使っている）。検査器は起動したアプリを**回収まで
# 行って**終了するので、次の起動が単一インスタンスの機構に引き継がれることはない。
#
# # 終了コード
#   0 = 適合 / 1 = 逸脱（検査器が非 0・錠前が成立しない） / 2 = 入力が使えない（実行ファイル・
#   生成器の失敗・strings の不在）
set -euo pipefail

verify=target/release/jxcel
if [ ! -x "$verify" ]; then
  echo "NG: 検証用の形の実行ファイルがありません: $verify（先に --features verification-triggers のビルドを走らせてください）" >&2
  exit 2
fi
if ! command -v strings >/dev/null 2>&1; then
  echo "NG: strings が見つかりません（負の対照の実行ファイルが機能を含み、検証専用の識別子を含まないことの錠前に使います）" >&2
  exit 2
fi

# 記録の置き場（4.4 の解決。macOS は $HOME/Library/Logs/{識別子}。`logs` 接尾辞は付かない）。
record="$HOME/Library/Logs/com.jxcel.app/jxcel.log"
mkdir -p "$(dirname "$record")"
echo "診断記録: $record"

# **標本はリポジトリの中（`target/`）へ書く**（リポジトリの外のファイルシステムはアプリと共有され
# ない環境がある。Linux の段が実測した）。`target/` はリポジトリの中で、かつ配布物に入らない。
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

# 検査器を 1 回走らせる。第 1 引数は実行ファイル、第 2 引数は標本、第 3 引数は記録である。
# 予算の標本はどの走行でも渡す（検査器が必須にする）。
run_check() {
  _app=$1
  shift 1
  sh scripts/check-macro-observation.sh "$_app" "$sample" "$record" \
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
# してしまう（Linux の段が 2026-09-18 に実測した）。**この段は下の錠前 (c) で埋め込みを確かめる。**
cp "$verify" "$verify_saved"
# **直前に `dist/` を既定の形で作り直す**: `tauri/custom-protocol` はフロントエンドの資産を
# `dist/` から実行ファイルへ埋め込むため、**この段より前の検証用のビルドが残した `dist/`**
# （`JXCEL_VERIFICATION_BUILD=1` で作られたもの）をそのまま使うと、既定 feature のビルドなのに
# **検証専用の識別子が埋め込まれる**（2026-09-18 の実測: 錠前 (b) が 8 件を検出して落ちた。
# 錠前が正しく効いた例である）。順序の罠なので、ここで既定の形へ戻す。
# **`JXCEL_VERIFICATION_BUILD` を外して呼ぶ**（呼び出し元に残っていても既定の形にするため）。
env -u JXCEL_VERIFICATION_BUILD npm run build
cargo build --release -p jxcel --features tauri/custom-protocol
cp "$verify" "$default_binary"
cp "$verify_saved" "$verify"

# 錠前 (b): 検証専用の識別子を含まない（既定 feature のビルドである）。
# **`grep -q` を使わない** — 一致した時点で読むのをやめ、`strings` が EPIPE で非 0 終了し、
# `pipefail` の下で偽の失敗になる（`scripts/ci/macos/verify-single-instance.sh` が実測した）。
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
# 無いから観測の行が出ない」で成立してしまう（`tauri/custom-protocol` を付けない `cargo build`
# がその形である）。
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
# **落ちた理由が「観測の行の不在」であること**（起動しなかった実行も事実の欠落で落ちる）。
if ! printf '%s\n' "$output" | grep -q '観測の行'; then
  echo "NG: 反証が「観測の行が読めない」以外の理由で落ちた（検査器が動いていない可能性がある）" >&2
  exit 1
fi
# **実行ファイルが実際に起動してウィンドウを開いたこと**（検査器が出す起動の形跡の行）。
if ! printf '%s\n' "$output" | grep -q 'ウィンドウ:'; then
  echo "NG: 反証で実行ファイルのウィンドウが観測されていません（起動しなかった実行を錠前の実測と取り違えない）" >&2
  exit 1
fi
echo "反証: 期待どおり非 0 で落ちました（既定 feature のビルドは起動したが、観測の行が現れない）"

echo "OK: 3 OS の観測の段（macOS）— 実行の成功と出力の行数の記録・失敗の理由とフレーム・能力の拒否・打ち切りとその後の操作・保存と開き直しの往復・読みの予算（10 万行）・書き換えの予算（1 万行）が成立した"

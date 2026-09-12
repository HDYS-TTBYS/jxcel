#!/bin/sh
# check-forbidden-plugins.sh — ファイルシステム系・シェル系のプラグインが依存ツリーに無いことの検査
#                               （tasks.md 1.3 / 要件 4.7）
#
# 根拠（design.md「Security Considerations」・`src-tauri/Cargo.toml` の依存方針）:
#   要件 4.7 の制御は 4 段で成立している（`src-tauri/permissions/app.toml` のヘッダにも
#   同じ 4 段が書いてある）:
#     (1) **ファイルシステム系・シェル系のプラグインを依存に入れない**（本スクリプト）
#     (2) 自前コマンドの ACL を有効にする（`src-tauri/permissions/app.toml`）
#     (3) 未使用コマンドをビルド時に削る（`build.removeUnusedCommands`）
#     (4) 生成される capability の記述を機械検査する（`scripts/check-capabilities.sh`）
#   **(1) は「依存が無ければフロントエンドからの到達経路が存在しない」という最も強い制御で
#   ありながら、機械検査が無かった。** (2)〜(4) は「与えない」ことを検査するが、依存を 1 行
#   足せば `fs:*` / `shell:*` の権限を書ける状態になり、capability 検査はそれを検出する前に
#   「逸脱として落ちる」だけで、そもそも依存を入れてはならないという不変条件そのものは
#   守られない。本スクリプトは依存ツリー（推移的依存を含む）を走査する。
#
# 禁止するパッケージ（要件 4.7 が禁じる「任意のファイルを読み書きする経路」「任意のプロセスを
# 起動する経路」を開くプラグイン）:
#   - `tauri-plugin-fs`      : ファイルシステムへの直接アクセス
#   - `tauri-plugin-shell`   : 任意のプロセスの起動
#   - `tauri-plugin-store`   : ファイルへの設定の読み書き（本スペックは自前の設定ストアを使う）
#   - `tauri-plugin-dialog`  : `tauri-plugin-fs` を非 optional の通常依存として引き込む
#                              （タスク 1.3 が crates.io の依存データで確認。併せて Linux の
#                              ファイル選択は親ウィンドウを無視する実装であり、7.7 が不採用にした）
#
# **推移的依存まで見る理由**: 直接依存だけを禁止しても不十分である。上記のいずれかを
# 引き込むクレートを 1 行足した時点で到達経路が生まれる。したがって `cargo tree` の出力
# （依存ツリー全体）を検査する。`--all-features` も付ける（既定 feature の外に隠した依存を
# 素通りさせない。`scripts/check-core-deps.sh` と同じ規律）。
#
# 誤検出の回避: `cargo tree --format '{p}'` の**行頭のパッケージ名**だけを照合する。
#   - `tauri-plugin` そのもの（プラグインの基盤クレート。`tauri-plugin 2.6.3`）は行頭が
#     `tauri-plugin 2.6.3` であり、交替 `-fs|-shell|-store|-dialog` に一致しない。
#   - `tauri-plugin-log` / `tauri-plugin-single-instance` は許可される（前者は記録、
#     後者は二重起動の制御であり、ファイルシステムやプロセスの一般経路を開かない）。
#   - クレート自身の名前（`jxcel`）や絶対パスは行頭に現れない。
#
# POSIX sh 互換: `bash scripts/check-forbidden-plugins.sh` が Linux / macOS /
# Windows (Git Bash) のいずれでも動作すること（3 OS マトリクス共用）。依存は
# `[target.'cfg(...)'.dependencies]` で OS ごとに変わるため、3 OS すべてで走らせる意味がある。
#
# 使い方: sh scripts/check-forbidden-plugins.sh [パッケージ名]  (既定: jxcel)
#         アプリ（`src-tauri`、パッケージ名 `jxcel`）が対象である。ドメインコアクレートは
#         `scripts/check-core-deps.sh` が tauri 一族全体を禁止しているので、こちらでは扱わない。
# 終了コード:
#   0 = 禁止プラグインなし / 1 = 検出 / 2 = cargo 不在・依存解決の失敗
set -eu

PACKAGE="${1:-jxcel}"
EDGES="normal,build,dev"

if ! command -v cargo >/dev/null 2>&1; then
  echo "check-forbidden-plugins: cargo が見つかりません（依存ツリーの取得に必要です）" >&2
  echo "check-forbidden-plugins: Rust のツールチェーン（dtolnay/rust-toolchain）を導入してください" >&2
  exit 2
fi

# stderr はそのまま通すので、依存解決に失敗した理由がログに残る。
# `--color never` は行頭照合を色に依存させないための保険である。
if ! TREE=$(cargo tree -p "$PACKAGE" --all-features --edges "$EDGES" --color never --prefix none --format '{p}'); then
  echo "check-forbidden-plugins: 依存ツリーを取得できません: cargo tree -p ${PACKAGE} --all-features --edges ${EDGES}" >&2
  exit 2
fi

# cargo は同じ部分木を `(*)` 付きで再掲するので、行を畳んでから数える・照合する。
UNIQUE=$(printf '%s\n' "$TREE" | awk '!seen[$0]++')

# 成功時も件数を出す（空の走査が黙って通らないようにする）。
PACKAGE_COUNT=$(printf '%s\n' "$UNIQUE" | wc -l | tr -d ' ')

# 行頭（= `--format '{p}'` が先頭に出すパッケージ名）だけを照合する。
VIOLATIONS=$(printf '%s\n' "$UNIQUE" | grep -E '^tauri-plugin-(fs|shell|store|dialog)([-_ ]|$)' || true)

if [ -n "$VIOLATIONS" ]; then
  echo "check-forbidden-plugins: ファイルシステム系・シェル系のプラグインが ${PACKAGE} の依存ツリーに現れました（推移的依存も禁止。要件 4.7）:" >&2
  printf '%s\n' "$VIOLATIONS" | sed 's/^/  - /' >&2
  echo "check-forbidden-plugins: 取り込んだ経路の確認: cargo tree -p ${PACKAGE} --all-features --edges ${EDGES} --color never -i <上記のパッケージ名>" >&2
  echo "check-forbidden-plugins: ファイルシステムとプロセスの経路は、依存を入れないことが第一の制御である（src-tauri/Cargo.toml の依存方針を参照）" >&2
  exit 1
fi

echo "check-forbidden-plugins: OK ${PACKAGE} の依存ツリーに禁止プラグインはありません（走査 ${PACKAGE_COUNT} パッケージ）"

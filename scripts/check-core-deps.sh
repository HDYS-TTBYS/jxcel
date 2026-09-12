#!/bin/sh
# check-core-deps.sh — コアクレートが tauri に依存していないことの機械検査（tasks.md 10.1）
#
# 根拠（.kiro/steering/tech.md「Testing」・structure.md「Rust ドメインクレート」・
# ワークスペースと crates/app-shell/Cargo.toml の依存方針コメント）:
#   ドメインコアは GUI を起動せずにテストできなければならない。この前提は「行儀の良さ」ではなく
#   層の分離そのものであり、依存グラフで強制する。目視の確認では守られないため
#   （tech.md「不変条件は検査スクリプトにすること」）、本スクリプトが依存ツリーを走査する。
#
# 推移的依存まで見る理由:
#   直接依存だけを禁止しても不十分である。`tauri` を推移的に引き込むクレートを 1 行足した
#   時点で、コアは Tauri のビルドを要求し、GUI を持たないテストが成立しなくなる。
#   したがって `cargo tree` の出力（依存ツリー全体）を検査する。
#
# 走査する辺（--edges normal,build,dev）— 3 種すべてを禁止する:
#   - normal: 実行時に tauri を要求する。主たる違反である。
#   - build : ビルド時に tauri を要求する。Tauri のツールチェーン無しにコアをビルドできない。
#   - dev   : テストのビルド時に tauri が載る。「GUI を起動せずにテストできる」前提が崩れる。
#   structure.md の判定基準は「その振る舞いを確かめるのに画面が要るか」であり、
#   ビルド時・テスト時にしか使わない依存にも同じく当てはまる（3 種すべてを対象にする）。
#   `--all-features` も付ける。既定 feature の外に隠した依存を素通りさせないためである。
#
# 誤検出の回避: `cargo tree --format '{p}'` の**行頭のパッケージ名**だけを照合する。
#   - クレート自身の名前（`app-shell`）は一致しない。
#   - `not-tauri-x` のような部分文字列を含むだけの名前は、行頭が `not-tauri-x` なので一致しない。
#   - 絶対パス（例 `/home/user/tauri-work/jxcel/...`）は行頭に現れないので一致しない。
#   一致させてよいのは `tauri` そのもの、および `tauri-*` / `tauri_*`（tauri クレートの一族。
#   `tauri-utils` や `tauri-plugin-*` を名乗る以上、それは tauri への依存である）だけである。
#
# POSIX sh 互換: `bash scripts/check-core-deps.sh app-shell` が Linux / macOS /
# Windows (Git Bash) のいずれでも動作すること（3 OS マトリクス共用）。対象の依存は
# `[target.'cfg(...)'.dependencies]` で OS ごとに変わるため、3 OS すべてで走らせる意味がある。
#
# 使い方: sh scripts/check-core-deps.sh [パッケージ名]  (既定: app-shell)
#         `crates/*` のドメインクレートはすべて対象だが、10.1 がゲートするのはコアである
#         app-shell である。他のクレートを検査するときは名前を引数に渡す。
# 終了コード:
#   0 = tauri への依存なし
#   1 = tauri 系クレートが依存ツリーに現れる
#   2 = cargo 不在・依存解決の失敗（前提を満たす必要がある）
set -eu

PACKAGE="${1:-app-shell}"
EDGES="normal,build,dev"

if ! command -v cargo >/dev/null 2>&1; then
  echo "check-core-deps: cargo が見つかりません（依存ツリーの取得に必要です）" >&2
  echo "check-core-deps: Rust のツールチェーン（dtolnay/rust-toolchain）を導入してください" >&2
  exit 2
fi

# stderr はそのまま通すので、依存解決に失敗した理由がログに残る。
# `--color never` は行頭照合を色に依存させないための保険である（cargo は名前を着色しうる）。
if ! TREE=$(cargo tree -p "$PACKAGE" --all-features --edges "$EDGES" --color never --prefix none --format '{p}'); then
  echo "check-core-deps: 依存ツリーを取得できません: cargo tree -p ${PACKAGE} --all-features --edges ${EDGES}" >&2
  exit 2
fi

# cargo は同じ部分木を `(*)` 付きで再掲するので、行を畳んでから数える・照合する。
UNIQUE=$(printf '%s\n' "$TREE" | awk '!seen[$0]++')

# 成功時も件数を出す（空の走査が黙って通らないようにする。check-shared-assets.sh と同じ流儀）。
PACKAGE_COUNT=$(printf '%s\n' "$UNIQUE" | wc -l | tr -d ' ')

# 行頭（= `--format '{p}'` が先頭に出すパッケージ名）だけを照合する。
# `tauri` 単体（直後はバージョンの空白）と、`tauri-*` / `tauri_*` を逸脱とする。
VIOLATIONS=$(printf '%s\n' "$UNIQUE" | grep -E '^tauri([-_ ]|$)' || true)

if [ -n "$VIOLATIONS" ]; then
  echo "check-core-deps: tauri 系クレートが ${PACKAGE} の依存ツリーに現れました（推移的依存も禁止）:" >&2
  printf '%s\n' "$VIOLATIONS" | sed 's/^/  - /' >&2
  echo "check-core-deps: 取り込んだ経路の確認: cargo tree -p ${PACKAGE} --all-features --edges ${EDGES} --color never -i <上記のパッケージ名>" >&2
  echo "check-core-deps: コアは tauri に依存してはならない（structure.md「Rust ドメインクレート」。GUI を起動せずにテストできることが層の分離の前提である）" >&2
  exit 1
fi

echo "check-core-deps: OK ${PACKAGE} の依存ツリーに tauri はありません（走査 ${PACKAGE_COUNT} パッケージ）"

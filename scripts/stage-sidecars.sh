#!/bin/sh
# サイドカーの同梱原本を配置する（tasks.md 1.7）。**これが唯一の文書化された手順である。**
#
# 使い方:
#   bash scripts/stage-sidecars.sh                # ホストのツールチェーンのターゲットで配置
#   bash scripts/stage-sidecars.sh --target <T>   # 明示的なクロス配置（T はターゲットトリプル）
#   bash scripts/stage-sidecars.sh --help
#
# 注意（--target 指定時）: `cargo build --release --target <T>` の成果物は、指定なしの
# ネイティブビルドの成果物と**バイト単位では一致しない**（Cargo がターゲット別の出力
# ディレクトリとビルド指紋を使うため。ホストと同じトリプルを指定した場合でも異なる）。
# 整合性検査（3.1）は「ステージングされたファイル」と「配布物に入ったファイル」を照合するので、
# 同一の配布物を作る際は**常に同じ指定**で staging すること（CI は指定なしのネイティブで統一する）。
#
# 何をするか:
#   1. `crates/sidecar-smoke` を release でビルドする（リポジトリの `cargo` を使う）。
#   2. 生成された実行ファイルを `sidecars/sidecar-smoke-<ターゲットトリプル>[.exe]` として
#      複製する。**語幹 `sidecar-smoke` は `crates/app-shell` の `SidecarKind::Smoke::as_str()`
#      が返す値であり、タスク 1.2 の契約である**（変更しない。タスク 3.5 の孤児掃除が PID と
#      この実行ファイル名の両方で照合する）。トリプル接尾辞は `externalBin` と `tauri-build` の
#      命名規約そのもので、Tauri は設定に接尾辞なしの名前を書き、ツール側が接尾辞を付ける
#      （`tauri_utils::resources::external_binaries`。Windows のみ `.exe` が付く）。
#   3. 配置したパスを stdout に **1 行だけ**出力する（機械可読）。進行状況とエラーは stderr。
#
# なぜ明示的な成果物なのか:
#   ワークスペース内のクレート成果物を別クレートのビルド時処理が参照する順序は保証されない
#   （`cargo build --workspace` は依存関係の順に走るだけで、`src-tauri` のビルド時に
#   `sidecar-smoke` の release 成果物が存在することを Cargo は保証しない）。したがって暗黙の
#   ビルド連鎖に任せず、配布物生成（`npx tauri build`）の前提としてこの 1 手順を明示的に実行する。
#   CI（`.github/workflows/ci.yml`）は 3 OS すべてで、**`tauri build` より前**（Windows / macOS は
#   `tauri-build` がビルド時点で原本を要求するため `cargo build --workspace` より前）に実行する。
#
# 配置の規約（プラットフォームで分ける。research.md 決定 4）:
#   - Windows / macOS: `src-tauri/tauri.windows.conf.json` / `tauri.macos.conf.json` の
#     `bundle.externalBin` が `../sidecars/sidecar-smoke`（接尾辞なし）を指す。Tauri が
#     ターゲットトリプル付きの名前を解決し、配布物（Windows は実行ファイルの隣、macOS は
#     `Contents/MacOS/`）へ複製する。macOS では bundler が codesign する。
#   - Linux: `bundle.externalBin` は**使わない**。AppImage バンドラ（linuxdeploy）が `usr/bin`
#     配下の ELF を無条件に patchelf で書き換えるため、同梱した実行ファイルの内容が変わってしまう
#     （tauri-apps/tauri#5189。`NO_STRIP=1` は効かない）。代わりに
#     `src-tauri/tauri.linux.conf.json` の `bundle.linux.appimage.files` で
#     **`usr/share/jxcel/sidecar-smoke`** へ置く。`usr/share` は linuxdeploy の走査対象
#     （`usr/bin` は非再帰、`usr/lib` は再帰）の外であり、`appimage.files` は権限を保って複製する。
#     **Linux の実行時解決先は `usr/share/jxcel/sidecar-smoke` で確定である**（タスク 8.1 の入力）。
#     なお `appimage.files` は `externalBin` と違い**接尾辞を自動付加しない**ため、ソース側には
#     トリプル付きの実ファイル名を書く。Linux の対象は x86_64（`appimage` は amd64）なので確定名は
#     `sidecars/sidecar-smoke-x86_64-unknown-linux-gnu` である。他のアーキテクチャで Linux を
#     バンドルすると原本が見つからず**その場で失敗する**（下記の不在時の方針を参照）。
#
# 原本が未配置のときの扱い（不在時の方針。**タスク 3.1 が従う入力**）:
#   - 本スクリプト: ビルド出力が無ければ非 0 終了し、stderr に理由を出す。一時ファイルへ複製して
#     から `mv` で置換するため、失敗時に空・中途半端なファイルを残さず、既存の配置も壊さない。
#   - バンドル生成: Tauri は設定された原本が無ければその場で失敗する（バンドラの
#     `copy_file` / `copy_custom_files` が `does not exist` / `is not a file` を返す）。サイドカーを
#     欠いた配布物が黙ってできることはない。Windows / macOS では `externalBin` のため
#     `cargo build`（`tauri-build`）の時点で失敗する。
#   - 整合性検査（3.1）: 原本が無い状態のビルドでは**期待ダイジェストを埋め込まない**
#     （`crates/app-shell/build.rs` は panic しない。クローン直後の `cargo build` を壊さないため）。
#     実行時の `verify` は「原本が未配置で期待値が無い」ことを `Mismatch`（内容不一致）とも
#     `Unreadable`（実行時の読み取り不能）とも区別できる結果として返し、起動を中止して報告する。
#     **沈黙して通してはならない**（要件 5.3）。
set -eu

# --- 使い方 -----------------------------------------------------------------
usage() {
  cat >&2 <<'EOF'
使い方: scripts/stage-sidecars.sh [オプション]

  --target <T>   配置するターゲットトリプルを明示する（例: aarch64-unknown-linux-gnu）。
                 省略時は `rustc -vV` の host を使う。指定した場合は
                 `cargo build --release --target <T>` を行い、成果物を
                 `target/<T>/release/` から取る。
  -h, --help     この使い方を表示して終了する。

成果物のパスを stdout に 1 行で出力する。失敗時は非 0 終了し、理由を stderr に出す。
EOF
}

target=''
while [ $# -gt 0 ]; do
  case $1 in
    --target)
      if [ $# -lt 2 ]; then
        echo "エラー: --target にはターゲットトリプルが必要です。" >&2
        usage
        exit 2
      fi
      target=$2
      shift 2
      ;;
    --target=*)
      target=${1#--target=}
      if [ -z "$target" ]; then
        echo "エラー: --target= の値が空です。" >&2
        usage
        exit 2
      fi
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "エラー: 未知の引数です: $1" >&2
      usage
      exit 2
      ;;
  esac
done

# --- リポジトリの位置 -------------------------------------------------------
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)

# --- ターゲットトリプルの決定 -----------------------------------------------
explicit=''
if [ -z "$target" ]; then
  if ! rustc_info=$(rustc -vV 2>/dev/null); then
    echo "エラー: rustc を実行できません。--target でターゲットトリプルを明示してください。" >&2
    exit 1
  fi
  target=$(printf '%s\n' "$rustc_info" | sed -n 's/^host: //p')
  if [ -z "$target" ]; then
    echo "エラー: rustc -vV の出力から host を取り出せませんでした。" >&2
    exit 1
  fi
else
  explicit=1
fi

case $target in
  *windows*) exe_suffix='.exe' ;;
  *)         exe_suffix='' ;;
esac

# --- ビルドと配置 -----------------------------------------------------------
cd "$repo_root"

# 変数の直後に全角文字が続く展開は必ず `${...}` で区切る。macOS の bash は全角文字の
# 先頭バイトまで変数名として読み、`set -u` の下で「unbound variable」になる。
echo "sidecar-smoke を release でビルドします（ターゲット: ${target}）" >&2
if [ -n "$explicit" ]; then
  cargo build -p sidecar-smoke --release --target "$target"
  build_dir="$repo_root/target/$target/release"
else
  cargo build -p sidecar-smoke --release
  build_dir="$repo_root/target/release"
fi

src="$build_dir/sidecar-smoke$exe_suffix"
if [ ! -f "$src" ]; then
  echo "エラー: ビルド出力が見つかりません: $src" >&2
  echo "      （クロス配置では --target の値と rustup target の導入状況を確認してください）" >&2
  exit 1
fi
if [ ! -x "$src" ]; then
  echo "エラー: ビルド出力に実行権限がありません: $src" >&2
  exit 1
fi

dest_dir="$repo_root/sidecars"
mkdir -p "$dest_dir"
dest="$dest_dir/sidecar-smoke-$target$exe_suffix"

# 一時ファイルへ複製してから置換する。失敗しても空・中途半端なファイルを残さず、
# 直前の完全な配置を壊さない（不在時の方針）。再実行しても同じ結果になる。
tmp="$dest.tmp.$$"
cleanup() {
  if [ -n "$tmp" ] && [ -e "$tmp" ]; then
    rm -f "$tmp"
  fi
}
trap cleanup 0 1 2 15

cp "$src" "$tmp"
chmod 755 "$tmp"
mv -f "$tmp" "$dest"
tmp=''

relative=${dest#"$repo_root/"}
echo "配置しました: $relative" >&2
printf '%s\n' "$relative"

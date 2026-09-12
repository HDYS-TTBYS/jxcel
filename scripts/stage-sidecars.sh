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
#   3. 配置先のターゲットが macOS のときだけ、配置する実行ファイルを **bundler の再署名と
#      同じ条件で事前に ad-hoc 署名する**（下記「macOS の事前署名」）。
#   4. 配置したパスを stdout に **1 行だけ**出力する（機械可読）。進行状況とエラーは stderr。
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
#
# macOS の事前署名（重要。タスク 10.2。要件 6.5 と 3.1 の両方を成立させる）:
#   macOS の配布物では、bundler が同梱した補助プロセスの実行ファイルを**必ず再署名する**。
#   素のビルド成果物をそのまま配置すると、同梱物のバイトが配置物と一致しなくなり、
#   要件 6.5 のバイト比較と、3.1 の実行時の整合性検査（期待値は配置物から算出される）が
#   どちらも macOS で必ず失敗する（後者は `SidecarHost::ensure` が補助プロセスを起動できなく
#   なる機能上の破綻でもある）。**対策は「配置物を、bundler の再署名と同じ入力であらかじめ
#   署名しておく」こと**であり、そうすれば再署名は同じバイトを書き直すだけになる。
#
#   bundler の再署名の実体（tauri-cli 2.11.4 が使う tauri-bundler 2.9.4 / tauri-macos-sign 2.3.4。
#   版の対応は tauri-cli 2.11.4 の Cargo.toml: tauri-bundler = "2.9.4", tauri-macos-sign = "2.3.4",
#   tauri-utils = "2.9.3"）:
#     - 外部バイナリは `Contents/MacOS/<語幹>` へ、配置名から `-<ターゲット>` の接尾辞を外して
#       複製される（tauri-bundler 2.9.4 src/bundle/settings.rs:1160-1176 の `copy_binaries`。
#       1170 行が `.replace(&format!("-{}", self.target), "")`。複製は
#       src/utils/fs_utils.rs:81-96 の `fs::copy` でバイトを保つ）。
#     - そのパスが `SignTarget { is_an_executable: true }` として署名対象に入る
#       （src/bundle/macos/app.rs:99-105。呼び出しは 132 行の `sign(...)`）。
#     - 実行されるのは `codesign --force -s -`、および実行ファイルかつ hardened runtime 有効の
#       ときだけの `--options runtime`。src/bundle/macos/sign.rs:46-73 が
#       `target.is_an_executable && settings.macos().hardened_runtime` を渡し（66-72 行。
#       entitlements は 54-60 行）、tauri-macos-sign 2.3.4 src/keychain.rs:207-249 が
#       `--force -s <identity>` と `--options runtime` を組み立てる。
#       **引数の順序もこの 2 ファイルのとおりに写す**。
#     - `--keychain` は渡されない（identity が `-` のとき keychain.rs:42-47 が path を None にし、
#       keychain.rs:230-232 は path があるときだけ付ける）。`--entitlements` も渡されない
#       （sign.rs:54-59 が `settings.macos().entitlements` があるときだけ渡す。既定は None:
#       tauri-utils 2.9.3 src/config.rs:684。tauri-cli 2.11.4 src/interface/rust.rs:1493-1543 も
#       `bundle.macOS.entitlements` が無ければ None のままにする）。
#     - `hardenedRuntime` の既定は true（tauri-utils 2.9.3 src/config.rs:682）で、CLI は
#       `config.macos.hardened_runtime` をそのまま渡す（rust.rs:1644）。
#     - **`-i`（識別子）は渡されない。** したがって識別子は codesign が署名対象のファイル名から
#       導く（Apple Security の signer.cpp:158-175。`-i` が無いときだけ
#       `recommendedIdentifier()`（= パスのベース名。diskrep.cpp:286-319 の `canonicalIdentifier`
#       がディレクトリを落とし、singlediskrep.cpp:121-124 がパスをそのまま渡す）に、ad-hoc の
#       ときだけ `-<uniqueName>` を足す。uniqueName は Mach-O の LC_UUID を hex にしたもので
#       （machorep.cpp:236-255）、同じファイルなら再署名しても変わらない
#       （signer.cpp:1027-1046 のコメントが「reproducible for identical inputs, even upon
#       resigning」と明記している）。**識別子がファイル名に依存するため、配置名
#       `sidecar-smoke-<ターゲット>` のまま署名すると
#       `sidecar-smoke-aarch64-apple-darwin-<uuid>` になり、同梱名 `sidecar-smoke` から導かれる
#       `sidecar-smoke-<uuid>` と一致しない。**
#     - **`-i sidecar-smoke` を明示しても一致しない**: `-<uniqueName>` の付加は
#       signer.cpp:168 の「明示の識別子が無いとき」だけの分岐なので、`-i` を渡すと逆に
#       `<uuid>` の接尾辞が落ちて別物になる。
#   したがって本スクリプトは、**ベース名を同梱名 `sidecar-smoke` に合わせた一時コピー**を作り、
#   `-i` を付けずに bundler と同じ引数で署名してから配置する（識別子の導出を bundler と
#   同じ経路に乗せるのが要点である）。ad-hoc 署名の署名ブロブは空（signer.cpp:900-905 が
#   identity を持たないとき長さ 0 のブロブを返す）で、時刻も乱数も含まれない。ゆえに
#   同じ入力（ファイル内容・識別子・オプション・entitlements）なら署名はバイト単位で同一になり、
#   同梱時の再署名はバイト中立になる。これが 6.5 と 3.1 を同時に成立させる根拠である。
#   **この一致の実測は macOS のランナーでのみ可能**であり、`.github/workflows/ci.yml` の
#   「Verify bundled sidecar (macOS)」が確認の場である。
#
#   注意:
#     - 事前署名が要るのは**配置先のターゲットが macOS のとき**（ホスト OS ではない）。
#     - macOS のターゲットで `codesign` が PATH に無ければ**その場で非 0 終了する**。
#       沈黙して未署名のまま配置すると、同梱時の再署名でバイトが変わり要件 6.5 が必ず破れる
#       （黙って通すくらいなら落ちる）。macOS 以外で `--target *-apple-darwin` を選んだ
#       クロス配置もここで止まる。
#     - `APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD` / `APPLE_SIGNING_IDENTITY` が
#       設定されていれば**非 0 終了する**。bundler はそのとき `-` ではなくその identity で
#       再署名するため（sign.rs:19-44、rust.rs:1467-1475）、本スクリプトの ad-hoc の事前署名は
#       一致しなくなる。CI はこれらの環境変数を設定しない。
#     - 再実行は冪等である（ビルド成果物が同じなら、同じ入力に同じ署名が付く）。
#     - Linux / Windows の配置は**バイト単位で不変**である（この分岐に入らない）。
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

T が macOS（*-apple-darwin）のときは、配置する前に bundler の再署名と同じ条件で ad-hoc
署名する（同梱時の再署名をバイト中立にするため。ファイル先頭の「macOS の事前署名」を参照）。
その経路では codesign が PATH に必要で、無ければ非 0 終了する。

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
script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH='' cd -- "$script_dir/.." && pwd)

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

# macOS のターゲットかどうかは**配置先のターゲット**で判定する（ホスト OS ではない）。
# 事前署名が要るのは、macOS の同梱（`externalBin`）で bundler が再署名する場合である
# （ヘッダ「macOS の事前署名」を参照）。
apple_target=''
case $target in
  *apple-darwin*) apple_target=1 ;;
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
sign_dir=''
cleanup() {
  if [ -n "$sign_dir" ] && [ -d "$sign_dir" ]; then
    rm -rf "$sign_dir"
  fi
  if [ -n "$tmp" ] && [ -e "$tmp" ]; then
    rm -f "$tmp"
  fi
}
trap cleanup 0 1 2 15

cp "$src" "$tmp"
chmod 755 "$tmp"

# --- macOS: 同梱前の原本を事前署名する（ヘッダ「macOS の事前署名」） --------------
# bundler の再署名（`codesign --force -s - --options runtime`、`-i` なし、
# entitlements なし。tauri-bundler 2.9.4 src/bundle/macos/sign.rs:46-73 と
# tauri-macos-sign 2.3.4 src/keychain.rs:207-249）と**同じ入力**を作る。
# 識別子は codesign が署名対象の**ファイル名**から導く（Apple Security signer.cpp:158-175）ため、
# 署名対象のベース名を同梱時の名前 `sidecar-smoke` に合わせる。`-i` は使わない
# （使うと ad-hoc 特有の `-<uuid>` の接尾辞が落ち、bundler の導出値と一致しなくなる）。
# 一時ファイル `$tmp` を直接署名しないのは、そのベース名 `sidecar-smoke-<target>.tmp.<pid>` が
# 別の識別子を導いてしまうためである（署名対象のベース名が識別子を決める）。
if [ -n "$apple_target" ]; then
  if ! command -v codesign >/dev/null 2>&1; then
    echo "エラー: macOS 用の原本を配置するには codesign が必要ですが、PATH 上に見つかりません。" >&2
    echo "      codesign は macOS の Xcode Command Line Tools に含まれます（macOS のランナーには" >&2
    echo "      既定で入っています）。macOS 以外で --target *-apple-darwin を指定したクロス配置は" >&2
    echo "      ここでは実行できません。**未署名のまま配置すると、同梱時の再署名でバイトが変わり、" >&2
    echo "      要件 6.5 と 3.1 の整合性検査が必ず失敗する**ため、黙って続行しません。" >&2
    exit 1
  fi

  # 署名 identity が `-`（ad-hoc）でないと、事前署名と bundler の再署名が一致しない。
  # `APPLE_CERTIFICATE`/`APPLE_CERTIFICATE_PASSWORD` は `-` より優先して証明書を読み込み
  # （tauri-bundler 2.9.4 src/bundle/macos/sign.rs:19-44）、`APPLE_SIGNING_IDENTITY` は
  # `bundle.macOS.signingIdentity` より優先される（tauri-cli 2.11.4 src/interface/rust.rs:1467-1475）。
  # どちらも CI では設定していない。沈黙して食い違うより、ここで落とす。
  if [ -n "${APPLE_CERTIFICATE:-}${APPLE_CERTIFICATE_PASSWORD:-}${APPLE_SIGNING_IDENTITY:-}" ]; then
    echo "エラー: APPLE_CERTIFICATE / APPLE_CERTIFICATE_PASSWORD / APPLE_SIGNING_IDENTITY の" >&2
    echo "      いずれかが設定されています。bundler は設定ファイルの「-」（ad-hoc）ではなくそれを" >&2
    echo "      使って再署名するため、事前署名が一致しません（本スクリプトの事前署名は ad-hoc を" >&2
    echo "      前提としています）。3 OS の検証は ad-hoc 署名を前提にしています。" >&2
    exit 1
  fi

  # ベース名を同梱名に合わせるための一時ディレクトリ。EXIT/INT/TERM で必ず消す。
  # 同梱名は bundler と同じ規則で導く（`copy_binaries` が配置名から `-<target>` の接尾辞を
  # 外す。tauri-bundler 2.9.4 src/bundle/settings.rs:1165-1171。`.replace()` なので
  # `.exe` は外さない）。パターンを引用して、値がグロブとして解釈されないようにする。
  bundled_name=${dest##*/}
  target_suffix="-$target"
  bundled_name=${bundled_name%"$target_suffix"}
  sign_dir=$(mktemp -d "${TMPDIR:-/tmp}/jxcel-sidecar-sign.XXXXXX")
  sign_path="$sign_dir/$bundled_name"
  cp "$tmp" "$sign_path"

  # bundler と同じ引数（順序も同じ）。entitlements は `bundle.macOS.entitlements` が
  # 未設定なので渡さない（bundler も渡さない。既定は None: tauri-utils 2.9.3 src/config.rs:684）。
  if ! codesign --force -s - --options runtime "$sign_path"; then
    echo "エラー: 事前署名に失敗しました: $sign_path" >&2
    echo "      codesign の出力を確認してください。**未署名の原本は配置しません**（同梱時の" >&2
    echo "      再署名でバイトが変わり要件 6.5 が破れるため）。既存の配置は壊していません。" >&2
    exit 1
  fi

  cp "$sign_path" "$tmp"
  chmod 755 "$tmp"
  rm -rf "$sign_dir"
  sign_dir=''
  echo "macOS の事前署名を行いました（署名対象のベース名は同梱名 sidecar-smoke）: $dest" >&2
fi

mv -f "$tmp" "$dest"
tmp=''

relative=${dest#"$repo_root/"}
echo "配置しました: $relative" >&2
printf '%s\n' "$relative"

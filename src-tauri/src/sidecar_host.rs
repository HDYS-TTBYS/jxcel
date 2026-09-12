//! 補助プロセスのホスト — プラットフォーム別の実行ファイル解決、監督の束ね、出力の診断連携。
//!
//! 所有: `SidecarHost`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 5.1, 5.2, 5.3, 5.5, 5.9, 8.4, 8.7。
//!
//! 本モジュールが持つのは次の 4 つである。
//!
//! 1. **プラットフォーム別の実行ファイル解決**（[`resolve_executable`]）。規則は
//!    [`sidecar_path`] の 1 つの純粋関数にあり、このホスト（Linux）で 3 つすべてを試験できる。
//! 2. **監督の束ね**（[`SidecarHost`]）。アプリ全体で 1 実体の監督を所有し、**要求元が何であれ
//!    同じ登録簿を見る**ようにする。種類ごとに 1 つのプロセスという不変条件は監督の登録簿
//!    ロックが成立させており（tasks.md 3.2）、ここは同じ実体を配るだけである（要件 5.5）。
//! 3. **残留の掃除へ与える期待パス**（[`expected_executables`]。タスク 5.1 の seam）。
//! 4. **補助プロセスの出力を診断の記録先へ流す購読**（[`SidecarHost::watch_output`]。要件 5.9）。
//!
//! # 配置と解決（バンドラのソースで確かめた事実）
//!
//! 配置はプラットフォームで異なる（research.md 決定 4、tasks.md 1.7）。**実行時の名前と位置は
//! 推測せず、バンドラの実装から確かめた**:
//!
//! - **Windows / macOS** は標準の同梱機構（`bundle.externalBin`）を使う。設定には接尾辞なしの
//!   名前（`../sidecars/sidecar-smoke`）を書き、`tauri_utils::resources::external_binaries` が
//!   `-<ターゲットトリプル>[.exe]` を付けたパスを原本として解決する。配置する側
//!   （`tauri-build` の `copy_binaries` と tauri-bundler の `Settings::copy_binaries`）は
//!   ファイル名から `-<ターゲットトリプル>` を**外して**複製する。したがって配布物の中の名前は
//!   語幹そのもの（`sidecar-smoke` / Windows は `sidecar-smoke.exe`）である。
//! - **置き場所は実行ファイルと同じディレクトリ**である。macOS は `Contents/MacOS/`
//!   （`bundle/macos/app.rs` が `copy_binaries(&bin_dir)` を呼ぶ。`bin_dir` = `Contents/MacOS`）、
//!   Windows は NSIS の `$INSTDIR`（`binaries` の各項目を `File` で `$INSTDIR` へ置く）で、
//!   どちらも主実行ファイルの隣である。開発ビルドも同じで、`tauri-build` が主実行ファイルの
//!   隣（`target/<profile>/sidecar-smoke[.exe]`）へ複製する。
//! - **Linux は標準の同梱機構を使わない。** AppImage のバンドル処理（linuxdeploy）が `usr/bin`
//!   配下の ELF を無条件に `patchelf` で書き換えるため（tauri#5189）、`bundle.linux.appimage.files`
//!   と `bundle.linux.deb.files` で **`usr/share/jxcel/sidecar-smoke`** へ置く（走査対象は
//!   `usr/bin` 非再帰と `usr/lib` 再帰だけであり、`usr/share` はその外である）。複製は
//!   `fs_utils::copy_custom_files` が `copy_file(path, &data_dir.join(pkg_path))` で行うので
//!   **権限を保ったまま、宛先のパスがそのまま実行時の位置になる**。
//! - **AppImage でその根を見つける方法**: AppRun の AppImage フックが
//!   `export APPDIR="${APPDIR:-"$(dirname "$(realpath "$0")")"}"` で **`APPDIR` をマウント根に
//!   設定してから**アプリを起動する（1.7 が作った実配布物の
//!   `apprun-hooks/linuxdeploy-plugin-gtk.sh` で確認）。したがって AppImage では
//!   `$APPDIR/usr/share/jxcel/sidecar-smoke` が解決先である。
//! - **システムインストール（deb）** では `APPDIR` は無い。実行ファイルは `/usr/bin/jxcel` に
//!   置かれるので、**実行ファイルの 1 つ上がる `share/jxcel`**（`/usr/share/jxcel`）が同じ位置を
//!   指す。AppImage でも `/proc/self/exe` は `$APPDIR/usr/bin/jxcel` を指すため、この規則だけでも
//!   同じファイルに到達する（`APPDIR` は明示的な第 1 候補として先に見る）。
//!
//! 解決規則の要約（[`sidecar_path`]）:
//!
//! | プラットフォーム | 条件 | 解決先 |
//! |---|---|---|
//! | Windows / macOS | — | `<実行ファイルのディレクトリ>/sidecar-smoke[.exe]` |
//! | Linux | `APPDIR` が絶対パスで設定されている（AppImage） | `$APPDIR/usr/share/jxcel/sidecar-smoke` |
//! | Linux | それ以外（deb / システムインストール） | `<実行ファイルのディレクトリ>/../share/jxcel/sidecar-smoke` |
//!
//! # 展開・未配置・開発ビルドのときの振る舞い（**無言で誤ったパスに解決しない**）
//!
//! **通常経路では展開しない**（research.md 決定 4）。解決は常に上表の 1 箇所に定まり、
//! ファイルの有無で別の場所へ切り替わることはない。したがって次の失敗はすべて
//! **どのパスを見たかを含む区別可能なエラー**になる。
//!
//! - **未配置のビルド**: Windows / macOS は `tauri-build` がビルド時に落ちる（`externalBin` の
//!   原本が無い）。Linux はバンドル時に落ちる（`copy_custom_files` が「存在しない」を返す）。
//!   `cargo build` だけを行った Linux では落ちないため、実行時に
//!   [`SidecarHostError::Spawn`] → `SpawnError::NotFound { path }` として現れる。
//! - **開発ビルド（パッケージしていない実行）**: Windows / macOS は実行ファイルの隣（`cargo run`
//!   なら `target/<profile>/`）に原本が複製されているので**そのまま動く**。Linux は
//!   `<実行ファイル>/../share/jxcel/sidecar-smoke` を探すため、`target/debug/jxcel` から起動すると
//!   `target/share/jxcel/sidecar-smoke` を見に行き、無ければ `NotFound { path }` になる。
//!   **これは解決規則を開発用に曲げないことの代償であり、意図した振る舞いである** — 開発時に
//!   補助プロセスを起動したい場合は、配布物と同じ形（`usr/bin/` と `usr/share/jxcel/`）の
//!   ディレクトリを作ってそこから起動する（検証手順は tasks.md 8.1 の実測を参照）。
//! - **ダイジェストが埋め込まれていないビルド**（原本が未配置のままビルドした）: 監督が
//!   `SpawnError::IntegrityMismatch { source: IntegrityError::Unregistered }` を返す。整合性検査は
//!   **起動の前**に走る（tasks.md 3.1 / 3.2）ので、起動は試みられない。
//!
//! # 整合性検査が起動の前であることの経路
//!
//! [`SidecarHost::ensure`] → [`Supervisor::ensure`]（`app_shell::sidecar`）→ `spawn` の内側で
//! `preflight`（存在・実行権限）→ `integrity::verify`（同梱時ダイジェストとの照合）→ `Command::spawn`
//! の順に走る。本モジュールはこの順序に何も挟まない — 解決したパスをそのまま監督へ渡すだけで
//! ある。不一致（`Mismatch`）も読み取り不能（`Unreadable`）も期待値の不在（`Unregistered`）も
//! `IntegrityMismatch` の `source` として区別でき、修復経路は存在しない（design.md「Error
//! Handling」、research.md 決定 4）。
//!
//! # 出力を診断の記録先へ流す（要件 5.9）
//!
//! 監督は出来事を publish するだけで、記録へつなぐのは本モジュールの責務である（tasks.md 3.4 の
//! 申し送り）。[`SidecarHost::watch_output`] が購読を 1 つ張り、**購読者ごとの独立した受信路**を
//! 1 つの専用スレッドが読み続ける。したがって**他の購読者を止める経路は無い**（監督の配布は
//! 登録簿のロックを保持せず、送信路は無限容量で `send` はブロックしない。tasks.md 3.4）。
//!
//! 記録先は**記録機構（`tauri-plugin-log`）が唯一の宛先**である。本モジュールは `log` の面だけを
//! 使い、保存先のパスを自前で解決しない。保存先は 5.2 が [`diagnostics`] の方針値
//! （`DiagnosticsPolicy::log_dir()`）を唯一の源としてプラグインの `Folder` ターゲットへ渡しており、
//! 5.2 の起動時の確認（`confirm_effective_logging`）が「その名前のファイルが実際に現れた」ことを
//! 検証済みである。ここでパスを二重に持つと食い違いの余地が生まれるため、持たない。
//!
//! 記録の対象と水準（決定。妥当性は要件 5.9 と 8.7 の交点で決めた）:
//!
//! - `Output { stream: stdout }` → **Info**、`Output { stream: stderr }` → **Warn**。標準出力は
//!   子の通常の出力であり既定の詳細度（Info）で残る。標準エラーは子自身が警告・エラーの流れとして
//!   使う通例があり、詳細度を Warn 以上へ絞った利用者にも残る必要がある（要件 8.7 の絞り込みは
//!   利用者の意思であり、その下でも「子が異常を報告している」ことは落としたくない）。**どちらも
//!   本文に流れの名前を残す**ので、水準を上げたことが情報を失わせることはない。
//! - `Exited { status: Deliberate }` → **Info**。アプリ自身の終了処理であり異常ではない。
//! - `Exited { status: Unexpected }` → **Warn**。要件 5.7 が通知を求める「予期せぬ終了」であり、
//!   Info へ絞った利用者にも残らなければならない。
//! - `log` の対象名は **`jxcel::sidecar`** に固定する。記録機構の既定の書式は
//!   `[日時][対象名][水準] 本文` であり、モジュール名を既定に任せず固定することで、補助プロセスの
//!   記録だけを後から抜き出せる（モジュールを動かしても名前が変わらない）。
//!
//! **行は断片でありうる**（`MAX_LINE_BYTES` = 64 KiB を超える行は分割して届く。tasks.md 3.4）。
//! 本モジュールは断片をそのまま記録へ流し、連結や再構成を試みない — 断片が完成した 1 行か長い行の
//! 先頭かを [`SidecarEvent`] のフィールド集合からは区別できないためである（連結規則は読み取りの
//! 内部にしか無い）。同様に、**再起動をまたぐと新しい子の `Output` が前の子の `Exited` より先に
//! 届きうる**ので、出来事の時系列そのままに記録し、どの実体に属するかを推定しない。
//!
//! # 秘匿（要件 8.4）と、この経路の限界
//!
//! 流すのは**補助プロセスが自分で書いた行**であり、本モジュールもアプリもその内容を解釈しない。
//! 4.4 の `Redacted<T>` は「アプリが保持するドキュメントの値を記録経路へ渡せない」ことを型で
//! 保証する仕組みであり、外のプロセスが書いた文字列には適用できない（適用しても、値を伏せる
//! 場所が無い）。本モジュールはドキュメントのセル値もスキーマの内容も保持しないため、いま
//! 同梱している種類（[`SidecarKind::Smoke`]）の出力に要件 8.4 が禁じる内容は入りえない。
//! **種類を足すときの制約（下流スペックへの申し送り）**: 標準入出力がドキュメントの内容を
//! 運びうる種類（例: 本文を解析する言語サーバ）を [`SidecarKind`] へ足す場合、その出力を
//! **そのまま記録へ流してはならない**。そのときは (a) その種類の出力を記録から外す、(b) 内容を
//! 含まない要約（行数・種別・大きさ）だけを記録する、のいずれかを選び、判断をその種類を足す
//! タスクに負わせる。本モジュールは種類ごとの解釈を持たないので、ここで一律に落とすと
//! 5.9 が求める「出力を診断情報として取得できる」が成立しなくなる。

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;

use app_shell::sidecar::{
    SidecarEvent, SidecarExit, SidecarHandle, SidecarKind, SidecarSpec, SidecarStream,
    SidecarSupervisor, SpawnError, Supervisor,
};
use tauri_plugin_log::log;

/// AppImage の AppRun がマウント根を設定する環境変数の名前。
///
/// 値の設定はバンドルされたフック（`apprun-hooks/linuxdeploy-plugin-gtk.sh`）が行い、アプリは
/// その子として起動される。したがって「AppImage の中で動いている」ことの判定に使える。
const APPDIR_ENV: &str = "APPDIR";

/// AppImage（`$APPDIR`）からの同梱物の位置。`usr/share` は linuxdeploy の走査対象の外である
/// （`appimage.files` の宛先キーと同じ表現。tasks.md 1.7）。
const LINUX_APPDIR_SHARE_DIR: &str = "usr/share/jxcel";

/// システムインストール（`/usr/bin/jxcel`）から見た同梱物の位置。deb の宛先
/// （`deb.files` のキー `usr/share/jxcel/sidecar-smoke`）と同じ場所を指す。
const LINUX_BIN_RELATIVE_SHARE_DIR: &str = "../share/jxcel";

/// 補助プロセスの出来事を記録するときの `log` の対象名（モジュール doc「出力を診断の記録先へ
/// 流す」を参照）。
const LOG_TARGET: &str = "jxcel::sidecar";

// ---------------------------------------------------------------------------
// プラットフォーム別の解決（要件 5.1, 5.2）
// ---------------------------------------------------------------------------

/// 実行ファイルの配置規則が異なるプラットフォーム。
///
/// **規則を値として表す**のは、このホスト（Linux）でも 3 つすべての規則を決定的に試験できる
/// ようにするためである（[`sidecar_path`] の試験を参照）。実行時に使う値は
/// [`current_platform`] が 1 箇所で選ぶ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// 標準の同梱機構を使わない。`usr/share/jxcel/` に置く（AppImage / deb）。
    ///
    /// 実行時にこの変種を構築するのは Linux のビルドだけである（他の 2 つの変種と同じ理由で、
    /// それ以外のホストでは未構築として警告されないようにする）。
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Linux,
    /// `externalBin`。アプリバンドルの `Contents/MacOS/` に置く。
    ///
    /// **この変種を実行時に構築するのは macOS のビルドだけである。**それ以外のホストでは
    /// 規則の表と試験（[`sidecar_path`] の 3 プラットフォーム試験）だけが参照するため、
    /// 未構築として警告されないようにする（`orphan_sweep` が Windows 専用の関数へ付けて
    /// いるのと同じ扱い）。
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Macos,
    /// `externalBin`。インストール先（実行ファイルの隣）に置く。上の `Macos` と同じ理由で
    /// Windows 以外のホストでは実行時に構築されない。
    #[cfg_attr(not(windows), allow(dead_code))]
    Windows,
}

/// 実行中のホストの配置規則。本スペックの対象（Linux / macOS / Windows）以外では `None` を
/// 返し、呼び出し側が「配置規則が無い」として報告する（黙って別の規則を当てない）。
#[cfg(target_os = "linux")]
fn current_platform() -> Option<Platform> {
    Some(Platform::Linux)
}

#[cfg(target_os = "macos")]
fn current_platform() -> Option<Platform> {
    Some(Platform::Macos)
}

#[cfg(windows)]
fn current_platform() -> Option<Platform> {
    Some(Platform::Windows)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn current_platform() -> Option<Platform> {
    None
}

/// `APPDIR` の値のうち、配置の根として使えるものだけを返す（**純粋関数**）。
///
/// AppImage の AppRun は**絶対パス**を設定する（`export APPDIR="${APPDIR:-"$(dirname "$(realpath
/// "$0")")"}"`。1.7 が作った実配布物の `apprun-hooks/linuxdeploy-plugin-gtk.sh` で確認済み）。
/// **空の値と相対パスは無視する** — それらを配置の根にすると、`..` を含む相対パスが
/// `PathBuf::join` で連結され、そのパスが実行ファイルの隣の別の場所を指しうる（無言で誤った
/// パスになる）。無視した場合は通常の Linux の規則（実行ファイルからの相対）へ落ちる。
fn appdir_from_env(value: Option<&OsStr>) -> Option<PathBuf> {
    let value = value?;
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

/// 解決規則そのもの（**純粋関数**）。
///
/// `None` を返すのは、実行ファイルの属するディレクトリを特定できないときだけである
/// （通常の絶対パスでは起こらない）。**ファイルの有無では分岐しない** — 存在するかどうかは
/// 監督の事前検査（`SpawnError::NotFound`）が名前付きで報告する。
///
/// `exe_suffix` は `std::env::consts::EXE_SUFFIX` を渡す（Windows の同梱名だけが `.exe` を
/// 持つ）。**Linux の同梱名は接尾辞を持たない** — `appimage.files` / `deb.files` の宛先キーが
/// そのまま名前になるため、`exe_suffix` は Linux の腕では使わない。
fn sidecar_path(
    platform: Platform,
    executable: &Path,
    appdir: Option<&Path>,
    stem: &str,
    exe_suffix: &str,
) -> Option<PathBuf> {
    match platform {
        // 標準の同梱機構。バンドラが接尾辞を外して実行ファイルと同じディレクトリへ置く。
        Platform::Macos | Platform::Windows => {
            let directory = executable.parent()?;
            Some(directory.join(format!("{stem}{exe_suffix}")))
        }
        // AppImage。AppRun が設定したマウント根からの位置。
        Platform::Linux => match appdir {
            Some(appdir) => Some(appdir.join(LINUX_APPDIR_SHARE_DIR).join(stem)),
            // システムインストール。`/usr/bin/jxcel` の 1 つ上の `share/jxcel`。
            None => {
                let directory = executable.parent()?;
                Some(directory.join(LINUX_BIN_RELATIVE_SHARE_DIR).join(stem))
            }
        },
    }
}

/// 1 つの種類の実行ファイルを、実行中のホストに合わせて解決する（要件 5.1、5.2）。
///
/// # Errors
///
/// - [`ResolutionError::UnsupportedPlatform`] — 本スペックの対象外のターゲットで動いている
/// - [`ResolutionError::CurrentExecutable`] — 実行ファイルの位置を特定できない
///
/// **ファイルの不在はここでは扱わない。** 解決したパスをそのまま監督へ渡し、監督が
/// `SpawnError::NotFound { path }` として（パスを含めて）報告する。
pub fn resolve_executable(kind: SidecarKind) -> Result<PathBuf, ResolutionError> {
    let platform = current_platform().ok_or(ResolutionError::UnsupportedPlatform)?;
    let executable = executable_path()?;
    let appdir = appdir_from_env(std::env::var_os(APPDIR_ENV).as_deref());
    sidecar_path(
        platform,
        &executable,
        appdir.as_deref(),
        kind.as_str(),
        std::env::consts::EXE_SUFFIX,
    )
    .ok_or_else(|| ResolutionError::CurrentExecutable {
        message: format!(
            "実行ファイル {} の属するディレクトリを特定できない",
            executable.display()
        ),
    })
}

/// 残留の掃除（`Supervisor::with_expected_executables`）へ与える期待パス（タスク 5.1 の seam）。
///
/// 掃除は「PID と実行ファイル名の一致」に加えて、期待パスが与えられていればそのパスへの所属も
/// 要求する（`supervisor::sweep_orphans`）。**与えるかどうかは、そのパスが実際に照合しうるかで
/// 決める** — 照合しえない期待パスを与えると、掃除は例外を出さずに**何もしなくなり**、名前照合と
/// いう唯一働く錠前まで外れるためである（tasks.md 3.5 の申し送り）。
///
/// - **Windows**: 与える。`resolve_executable`（掃除側）は
///   `QueryFullProcessImageNameW` で起動時の**絶対パス**を返すため、インストール先が同じなら
///   一致する（インストール先は実行のたびに変わらない）。
/// - **Linux（システムインストール）**: 与える。`/proc/<pid>/exe` も `/usr/share/jxcel/...` を
///   返すため一致する（`same_executable` が正規化して比べるので、`/usr/bin/..` を含む表現でも
///   一致する）。
/// - **Linux（AppImage）**: **与えない。** マウント根は実行ごとに変わる（`/tmp/.mount_*`）ため、
///   **前回の実行が残した孤児のパスは今回の期待パスと一致しえない**（孤児は前回のマウントの
///   ままロックしているので、そのマウントは生きているが別のパスである）。与えれば macOS と
///   同じ無音の no-op を作る。
/// - **macOS**: **与えない。** 掃除側の `resolve_executable` は `ps -p <pid> -o comm=` を使い、
///   これは起動時のパスではなくコマンド名を返す（カーネルの `p_comm` は 16 文字で切詰められる。
///   tasks.md 3.5）。絶対パス比較は一致しえない。**名前照合は成立する** — 語幹 `sidecar-smoke` は
///   16 文字に収まるので `identify` が成立する。**この判断を反転させる条件**: 10.x が macOS の
///   実機で「孤児の `comm` が解決パスと同一の絶対パスを返す」ことを実測したとき（そのときは
///   [`sweep_can_match_expected_paths`] の macOS の腕を `true` へ変えるだけでよい）。
///   CI で実測できるまでは、無音の no-op を作らない側に倒す。
///
/// 与える場合の値は**[`resolve_executable`] そのもの**である（同じ 1 箇所から導くので、
/// 起動するパスと掃除が期待するパスが食い違いようがない）。実行ファイルの位置を特定できない
/// 場合は**空を返す**（名前照合だけの掃除へ落ちる。**掃除そのものは止めない**）。
pub fn expected_executables() -> Vec<PathBuf> {
    let Some(platform) = current_platform() else {
        return Vec::new();
    };
    let appdir = appdir_from_env(std::env::var_os(APPDIR_ENV).as_deref());
    if !sweep_can_match_expected_paths(platform, appdir.as_deref()) {
        return Vec::new();
    }
    SidecarKind::ALL
        .iter()
        .filter_map(|kind| resolve_executable(*kind).ok())
        .collect()
}

/// 期待パスを掃除へ与えてよいか（**純粋関数**）。doc は [`expected_executables`]。
///
/// **与えないと決めた場合でも掃除は無効にならない** — 期待パスは実行ファイル名の一致に
/// 加える 2 つ目の錠前であり、外しても名前の照合は残る（`supervisor::sweep_orphans`）。
/// 逆に、**照合しえない期待パスを与えると掃除は無音で何もしなくなる**（`same_executable` が
/// 不一致を返し、対象がすべて読み飛ばされる）。だから「照合しうるときだけ与える」のである。
fn sweep_can_match_expected_paths(platform: Platform, appdir: Option<&Path>) -> bool {
    match platform {
        // Windows: インストール先は実行のたびに変わらず、掃除側も絶対パスを返す。
        Platform::Windows => true,
        // Linux: システムインストール（deb）はパスが安定している。AppImage は実行ごとに
        // マウント根が変わるため一致しえない。
        Platform::Linux => appdir.is_none(),
        // macOS: `comm` は 16 文字に切詰められる。
        Platform::Macos => false,
    }
}

/// 実行中のプロセスの実行ファイル。取得できない場合は区別できるエラーとして返す
/// （**パニックしない** — 起動を止めるのは呼び出し側が判断する）。
fn executable_path() -> Result<PathBuf, ResolutionError> {
    std::env::current_exe().map_err(|error| ResolutionError::CurrentExecutable {
        message: error.to_string(),
    })
}

/// 実行ファイルの解決に失敗した理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    /// 本スペックの対象外のターゲットで動いている（配置規則が無い）。
    UnsupportedPlatform,
    /// 実行ファイルの位置、またはその属するディレクトリを特定できない。
    CurrentExecutable {
        /// 取得に失敗した理由、または特定できなかったパス。
        message: String,
    },
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolutionError::UnsupportedPlatform => write!(
                formatter,
                "このプラットフォーム向けの補助プロセスの配置規則が無い"
            ),
            ResolutionError::CurrentExecutable { message } => {
                write!(
                    formatter,
                    "補助プロセスの実行ファイルを解決できない: {message}"
                )
            }
        }
    }
}

impl std::error::Error for ResolutionError {}

// ---------------------------------------------------------------------------
// 監督の束ね（要件 5.3, 5.5）
// ---------------------------------------------------------------------------

/// 補助プロセスのホスト。**アプリ全体で 1 実体**を管理状態として置き、すべての要求元が
/// ここから [`SidecarHost::ensure`] を呼ぶ。
///
/// 種類ごとに 1 つのプロセスという不変条件は**監督が持つ** — `SidecarHost` は同じ監督を
/// （`Clone` の複製を通じても同じ登録簿を）配るだけであり、ここで登録簿を二重に持たない。
/// したがって「複数の要求元が同じ種類を要求しても起動は 1 回」であり（要件 5.5）、
/// 起動時・終了時の終了（[`crate::lifecycle`] の 5.6 の経路）も同じ登録簿を見る。
#[derive(Clone)]
pub struct SidecarHost {
    supervisor: Supervisor,
}

impl SidecarHost {
    /// 監督を束ねる。監督の組み立て（期待パス・猶予）は呼び出し側が行う
    /// （[`crate::lifecycle`] の `sidecar_host` が唯一の生成点）。
    pub fn new(supervisor: Supervisor) -> Self {
        Self { supervisor }
    }

    /// 束ねている監督。起動時の残留掃除と終了時の終了が同じ実体を使うために公開する。
    pub fn supervisor(&self) -> &Supervisor {
        &self.supervisor
    }

    /// 補助プロセスを要求する（要件 5.2、5.3、5.5）。**すべての要求元が通る唯一の入口である。**
    ///
    /// 解決した絶対パスを監督へ渡すだけであり、**整合性検査は監督の `spawn` の内側で起動の前に
    /// 走る**（モジュール doc「整合性検査が起動の前であることの経路」）。
    ///
    /// # Errors
    ///
    /// - [`SidecarHostError::Resolution`] — 実行ファイルの位置を解決できない
    /// - [`SidecarHostError::Spawn`] — 監督が起動を拒んだ（存在しない・実行権限が無い・
    ///   整合性検査に失敗した・起動そのものに失敗した）。原因は [`SpawnError`] の変種で区別できる
    ///   （要件 5.4）
    ///
    /// # 既定ビルドでの未使用について
    ///
    /// **この入口を使うのは、アプリ内で補助プロセスを必要とする機能である。**本スペックの
    /// 時点ではそんな機能は無く（同梱する実物がまだ無い。research.md 決定 9）、在るのは非既定の
    /// `verification-triggers` の引き金だけである。**配布物に検証専用のコードを入れるわけには
    /// いかない**（tasks.md 5.4 の片付け義務）ので、feature が無いビルドではこの入口に呼び出し元が
    /// 無い。したがって未使用の警告は**feature が無いときだけ**許す — feature 付きのビルドでは
    /// 引き金がこの入口を使わなくなれば警告が出る（結線が外れたことに気づける）。
    #[cfg_attr(not(feature = "verification-triggers"), allow(dead_code))]
    pub fn ensure(
        &self,
        kind: SidecarKind,
        args: Vec<String>,
    ) -> Result<SidecarHandle, SidecarHostError> {
        let executable = resolve_executable(kind)?;
        let spec = SidecarSpec {
            kind,
            executable,
            args,
        };
        self.supervisor
            .ensure(&spec)
            .map_err(|source| SidecarHostError::Spawn { kind, source })
    }

    /// 補助プロセスの出来事を診断の記録先へ流す購読を結線する（要件 5.9）。
    ///
    /// **1 回だけ呼ぶ**（呼ぶたびに購読が増え、同じ行が重複して記録される）。返るハンドルは
    /// 所有しなくてよい — スレッドはアプリの寿命と同じだけ生き、プロセスの終了で消える。
    ///
    /// 記録先は記録機構（`tauri-plugin-log`）であり、その登録（5.2）より後、**最初の
    /// `ensure` より前**に呼ぶ必要がある（購読は過去を遡らない）。`crate::lifecycle` の `run` は
    /// 手順 4.4（`Builder::build` の後、`app.run` の前）で呼ぶ — その時点で記録機構のロガーは
    /// 取り付け済みであり、補助プロセスを起動しうる経路（下流スペックの `setup`、`RunEvent::Ready`
    /// の検証の引き金）はまだ 1 つも走っていない。
    ///
    /// スレッドを作れなかった場合は `None` を返し、**警告を記録して起動は続ける**（要件 5.4 の
    /// 精神。補助プロセスの出力が残らないことは、アプリを起動できなくする理由にならない）。
    pub fn watch_output(&self) -> Option<JoinHandle<()>> {
        let receiver = self.supervisor.subscribe();
        match std::thread::Builder::new()
            .name("sidecar-output".to_owned())
            .spawn(move || drain_events(receiver))
        {
            Ok(handle) => Some(handle),
            Err(error) => {
                log::warn!(target: LOG_TARGET, "補助プロセスの出力を記録へ流す購読を結線できなかった: {error}");
                None
            }
        }
    }
}

/// 補助プロセスの要求が失敗した理由（要件 5.4 の「区別できる形で報告」）。
///
/// **既定ビルドでは未使用である**（この型を作る唯一の入口 [`SidecarHost::ensure`] に呼び出し元が
/// 無い）。理由はその doc の「既定ビルドでの未使用について」にある。
#[cfg_attr(not(feature = "verification-triggers"), allow(dead_code))]
#[derive(Debug)]
pub enum SidecarHostError {
    /// 実行ファイルの解決に失敗した（配置の前提が無い）。
    Resolution(ResolutionError),
    /// 監督が起動を拒んだ。原因は [`SpawnError`] が区別する。
    Spawn {
        /// 起動しようとした種類。
        kind: SidecarKind,
        /// 監督が返した理由。
        source: SpawnError,
    },
}

impl fmt::Display for SidecarHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SidecarHostError::Resolution(error) => write!(formatter, "{error}"),
            SidecarHostError::Spawn { kind, source } => {
                write!(formatter, "補助プロセス {kind} を起動できない: {source}")
            }
        }
    }
}

impl std::error::Error for SidecarHostError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SidecarHostError::Resolution(error) => Some(error),
            SidecarHostError::Spawn { source, .. } => Some(source),
        }
    }
}

impl From<ResolutionError> for SidecarHostError {
    fn from(error: ResolutionError) -> Self {
        SidecarHostError::Resolution(error)
    }
}

// ---------------------------------------------------------------------------
// 出力の診断連携（要件 5.9、5.7）
// ---------------------------------------------------------------------------

/// 購読が届ける出来事を記録へ流し続ける（専用スレッドの本体）。
///
/// 取り出すのは**自分専用の受信側だけ**であり、他の購読者の受信路と読み取りスレッドには
/// 触れない。監督が送信側を保持する限り `recv` はブロックして待つ（購読は過去を遡らない）。
fn drain_events(receiver: Receiver<SidecarEvent>) {
    while let Ok(event) = receiver.recv() {
        match event {
            SidecarEvent::Output { kind, stream, line } => record_output(kind, stream, &line),
            SidecarEvent::Exited { kind, status } => record_exit(kind, status),
        }
    }
}

/// 1 行（またはその断片）を記録する。水準の決定はモジュール doc にある。
fn record_output(kind: SidecarKind, stream: SidecarStream, line: &str) {
    let stream_name = match stream {
        SidecarStream::Stdout => "stdout",
        SidecarStream::Stderr => "stderr",
    };
    match stream {
        SidecarStream::Stdout => log::info!(
            target: LOG_TARGET,
            "補助プロセスの出力: kind={kind} stream={stream_name} {line}"
        ),
        SidecarStream::Stderr => log::warn!(
            target: LOG_TARGET,
            "補助プロセスの出力: kind={kind} stream={stream_name} {line}"
        ),
    }
}

/// 終了の出来事を記録する。**由来で水準を分ける**（意図的な終了は Info、予期せぬ終了は Warn）。
///
/// ここは**通知するだけで、アプリケーション本体を終了させない**（要件 5.7 の購読側の責務）。
fn record_exit(kind: SidecarKind, exit: SidecarExit) {
    let status = match exit.status() {
        Some(status) => status.to_string(),
        None => "終了状態を取得できなかった".to_owned(),
    };
    if exit.is_deliberate() {
        log::info!(
            target: LOG_TARGET,
            "補助プロセスが終了した（こちらからの終了）: kind={kind} status={status}"
        );
    } else {
        log::warn!(
            target: LOG_TARGET,
            "補助プロセスが予期せず終了した: kind={kind} status={status}"
        );
    }
}

// ---------------------------------------------------------------------------
// テスト（タスク 8.1）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{appdir_from_env, resolve_executable, sidecar_path, Platform, SidecarHost};
    use app_shell::sidecar::integrity::{verify_with, IntegrityError, BUILD_TARGET_TRIPLE};
    use app_shell::sidecar::{
        IntegrityVerifier, SidecarKind, SidecarSpec, SidecarSupervisor, SpawnError, Supervisor,
    };
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Barrier, LazyLock, Mutex, MutexGuard};
    use std::time::{Duration, Instant};
    use tauri_plugin_log::log;

    /// 補助プロセスを実際に起動する試験を直列化する。
    ///
    /// 2 つの試験が同じ配置のパスから起動するため、並行して走ると「種類ごとに 1 つのプロセス」を
    /// OS 上の数で確かめる試験（[`two_requesters_share_one_sidecar_process`]）が互いの子を数えて
    /// しまう。直列化は試験の前提を決定的にするためだけのものである。
    fn sidecar_tests() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // -----------------------------------------------------------------------
    // 解決規則（3 プラットフォームすべてをこのホストで確かめる）
    // -----------------------------------------------------------------------

    /// Linux は「AppImage なら `$APPDIR/usr/share/jxcel`」「そうでなければ実行ファイルの隣の
    /// `share/jxcel`」の 2 つの規則を持つ。
    #[test]
    fn the_linux_resolution_follows_the_appimage_marker_then_the_system_layout() {
        let executable = Path::new("/usr/bin/jxcel");

        // システムインストール（deb）。`/usr/bin/jxcel` → `/usr/share/jxcel/sidecar-smoke`。
        assert_eq!(
            sidecar_path(Platform::Linux, executable, None, "sidecar-smoke", ""),
            Some(PathBuf::from("/usr/bin/../share/jxcel/sidecar-smoke")),
        );

        // AppImage。AppRun が設定するマウント根からの位置。
        assert_eq!(
            sidecar_path(
                Platform::Linux,
                Path::new("/tmp/.mount_jxcel/usr/bin/jxcel"),
                Some(Path::new("/tmp/.mount_jxcel")),
                "sidecar-smoke",
                "",
            ),
            Some(PathBuf::from(
                "/tmp/.mount_jxcel/usr/share/jxcel/sidecar-smoke"
            )),
        );

        // Linux の同梱名は接尾辞を持たない（`appimage.files` / `deb.files` の宛先キーが名前）。
        assert_eq!(
            sidecar_path(Platform::Linux, executable, None, "sidecar-smoke", ".exe"),
            Some(PathBuf::from("/usr/bin/../share/jxcel/sidecar-smoke")),
        );
    }

    /// Windows / macOS は標準の同梱機構（`externalBin`）の規則で、**実行ファイルと同じ
    /// ディレクトリ**に置かれた語幹（Windows だけ `.exe`）を指す。
    #[test]
    fn the_bundled_resolution_places_the_sidecar_next_to_the_executable() {
        // macOS: `Contents/MacOS/` に接尾辞なしで置かれる（bundle/macos/app.rs）。
        assert_eq!(
            sidecar_path(
                Platform::Macos,
                Path::new("/Applications/jxcel.app/Contents/MacOS/jxcel"),
                None,
                "sidecar-smoke",
                "",
            ),
            Some(PathBuf::from(
                "/Applications/jxcel.app/Contents/MacOS/sidecar-smoke"
            )),
        );

        // Windows: 同じ規則に `.exe` の接尾辞を渡した場合（インストール先は `$INSTDIR` =
        // 実行ファイルの隣）。**パスの区切りは std の担当なので、ここでは規則と接尾辞だけを
        // 確かめる** — Windows の実パスで動かす確認は 10.x の CI が担う。
        assert_eq!(
            sidecar_path(
                Platform::Windows,
                Path::new("/opt/jxcel/jxcel.exe"),
                None,
                "sidecar-smoke",
                ".exe",
            ),
            Some(PathBuf::from("/opt/jxcel/sidecar-smoke.exe")),
        );
    }

    /// `APPDIR` の値のうち配置の根として使えるのは**絶対パスだけ**である（空・相対は無視して
    /// 通常の Linux の規則へ落ちる）。
    #[test]
    fn only_an_absolute_appdir_value_is_used_as_the_root() {
        use std::ffi::OsStr;

        // 絶対パスの例は**ホストの規則**に合わせる（`/tmp/.mount_jxcel` は Windows では
        // ドライブ前置が無いため絶対パスではない）。これを合わせないと、規則そのものではなく
        // `Path` のプラットフォーム差を試験してしまい、Windows で必ず落ちる。
        let absolute_root = if cfg!(windows) {
            PathBuf::from(r"C:\mount\jxcel")
        } else {
            PathBuf::from("/tmp/.mount_jxcel")
        };
        assert_eq!(
            appdir_from_env(Some(absolute_root.as_os_str())),
            Some(absolute_root),
        );
        assert_eq!(appdir_from_env(Some(OsStr::new(""))), None);
        assert_eq!(appdir_from_env(Some(OsStr::new("mount_jxcel"))), None);
        assert_eq!(appdir_from_env(Some(OsStr::new("./mount_jxcel"))), None);
        assert_eq!(appdir_from_env(None), None);
    }

    /// 期待パスは**照合しうるホストにだけ**与える（tasks.md 3.5 の申し送りと、AppImage の
    /// マウント根が実行ごとに変わること）。与える場合の値は解決そのものなので、値の組み立ては
    /// [`the_bundled_resolution_places_the_sidecar_next_to_the_executable`] が覆う。
    #[test]
    fn the_expected_paths_are_withheld_where_the_sweep_cannot_match_them() {
        // Windows: 与える（掃除側は起動時の絶対パスを返し、インストール先は安定している）。
        assert!(super::sweep_can_match_expected_paths(
            Platform::Windows,
            None
        ));

        // Linux（システムインストール = deb）: 与える（`/proc/<pid>/exe` が同じファイルを指す）。
        assert!(super::sweep_can_match_expected_paths(Platform::Linux, None));

        // Linux（AppImage）: 与えない（マウント根は実行ごとに変わる）。
        assert!(!super::sweep_can_match_expected_paths(
            Platform::Linux,
            Some(Path::new("/tmp/.mount_jxcel")),
        ));

        // macOS: 与えない（`ps comm` は 16 文字に切詰められ、絶対パス比較が成立しない）。
        assert!(!super::sweep_can_match_expected_paths(
            Platform::Macos,
            None
        ));
    }

    /// 与えると決めたホストでは、期待パスが**解決と同じ値**である（`resolve_executable` を
    /// 唯一の源にする）。与えないと決めたホストでは空である（**掃除は名前照合だけで働く**）。
    #[test]
    fn the_expected_paths_are_derived_from_the_same_resolution() {
        let Some(platform) = super::current_platform() else {
            return;
        };
        let appdir = super::appdir_from_env(std::env::var_os("APPDIR").as_deref());
        let expected = super::expected_executables();

        if super::sweep_can_match_expected_paths(platform, appdir.as_deref()) {
            assert_eq!(
                expected,
                vec![resolve_executable(SidecarKind::Smoke).expect("解決できる")],
            );
        } else {
            assert!(
                expected.is_empty(),
                "照合しえないホストでは与えない: {expected:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // 実プロセスを伴う検証（同梱原本が配置されている環境でのみ走る）
    // -----------------------------------------------------------------------

    /// 同梱原本のパス（tasks.md 1.7 の配置規約。全 3 OS で同じ組み立て）。
    fn staged_original() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("sidecars")
            .join(format!(
                "{}-{}{}",
                SidecarKind::Smoke.as_str(),
                BUILD_TARGET_TRIPLE,
                std::env::consts::EXE_SUFFIX,
            ))
    }

    /// 検証の前提を整える: **解決規則が指すパスへ原本を置き**、そのパスを返す。
    ///
    /// 戻り値が `None` のときは検証を飛ばす。飛ばす条件は 2 つあり、どちらも標準エラーへ理由を
    /// 出す（黙って成功させない）:
    ///
    /// - 原本が無い（クローン直後。`bash scripts/stage-sidecars.sh` を実行していない）
    /// - このプロセスの解決先が配置の規則と一致しない（AppImage の中で `cargo test` を走らせた
    ///   場合。`APPDIR` が設定されていると解決先はマウント根を指す）
    fn prepare_layout() -> Option<PathBuf> {
        let stem = SidecarKind::Smoke.as_str();
        let original = staged_original();
        if !original.is_file() {
            eprintln!(
                "前提が無いため飛ばす: 同梱原本 {} が配置されていない（bash scripts/stage-sidecars.sh）",
                original.display()
            );
            return None;
        }

        let executable = std::env::current_exe().ok()?;
        let platform = super::current_platform()?;
        let layout = sidecar_path(
            platform,
            &executable,
            None,
            stem,
            std::env::consts::EXE_SUFFIX,
        )?;
        if resolve_executable(SidecarKind::Smoke).ok().as_deref() != Some(layout.as_path()) {
            eprintln!(
                "前提が無いため飛ばす: この環境の解決先が配置の規則（{}）と一致しない",
                layout.display()
            );
            return None;
        }

        let parent = layout.parent()?;
        std::fs::create_dir_all(parent).ok()?;
        // 一時ファイル経由で置換する（`scripts/stage-sidecars.sh` と同じ手順。実行中のプロセスが
        // 中途半端なファイルを読まない）。
        let temporary = parent.join(format!(".{stem}.tmp.{}", std::process::id()));
        std::fs::copy(&original, &temporary).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755)).ok()?;
        }
        std::fs::rename(&temporary, &layout).ok()?;
        Some(layout)
    }

    /// 短い猶予を与えた本番の監督（整合性検査は [`app_shell::sidecar::integrity::verify`] のまま）。
    fn test_supervisor() -> Supervisor {
        Supervisor::new().with_grace(Duration::from_millis(300))
    }

    /// 指定した期待ダイジェストの監督で起動を試みる（`with_verifier` は tasks.md 3.2 の seam。
    /// **本番のコードはこれを呼ばない**）。
    fn supervisor_with_digest(digest: [u8; 32]) -> Supervisor {
        let digests: Vec<(SidecarKind, [u8; 32])> = vec![(SidecarKind::Smoke, digest)];
        let verify: Arc<IntegrityVerifier> =
            Arc::new(move |kind, path| verify_with(&digests, kind, path));
        Supervisor::with_verifier(verify).with_grace(Duration::from_millis(300))
    }

    /// **要件 5.5**: 2 つの独立した要求元が要求しても、起動は 1 つであり同じハンドルが返る。
    ///
    /// 要求元は `SidecarHost` の複製 2 つ（本番でウィンドウや機能がそれぞれ持つのと同じ形。
    /// 監督の登録簿は `Clone` を通じて共有される）。**重なるように要求する**ためバリアで
    /// 同時に放ち、その後 OS 上のプロセス数も数える。
    #[test]
    fn two_requesters_share_one_sidecar_process() {
        let _serial = sidecar_tests();
        let Some(layout) = prepare_layout() else {
            return;
        };

        let supervisor = test_supervisor();
        let first = SidecarHost::new(supervisor.clone());
        let second = SidecarHost::new(supervisor.clone());
        let barrier = std::sync::Arc::new(Barrier::new(2));

        let request = |host: SidecarHost, barrier: Arc<Barrier>| {
            std::thread::spawn(move || {
                barrier.wait();
                host.ensure(SidecarKind::Smoke, Vec::new()).map(|handle| {
                    // 生存していることまで確かめる（ハンドルが「使える」ことの最小の観測）。
                    let alive = matches!(handle.try_wait(), Ok(None));
                    (handle.kind(), handle.pid(), alive)
                })
            })
        };
        let first_thread = request(first, Arc::clone(&barrier));
        let second_thread = request(second, barrier);
        let first_result = first_thread
            .join()
            .expect("要求元のスレッドはパニックしない");
        let second_result = second_thread
            .join()
            .expect("要求元のスレッドはパニックしない");

        // 数えるのは `shutdown_all` の前である（終了させた後では 0 になる）。
        let running = running_processes(&layout);
        let shutdown = supervisor.shutdown_all();

        let (first_kind, first_pid, first_alive) =
            first_result.expect("1 つ目の要求元がハンドルを受け取る");
        let (second_kind, second_pid, second_alive) =
            second_result.expect("2 つ目の要求元がハンドルを受け取る");
        assert_eq!(first_kind, SidecarKind::Smoke);
        assert_eq!(second_kind, SidecarKind::Smoke);
        assert!(first_alive && second_alive, "どちらのハンドルも生存する");
        assert!(first_pid > 0);
        assert_eq!(
            first_pid, second_pid,
            "2 つの要求元が同じ 1 つのプロセスを共有する"
        );
        // OS 上の数を照合できるホスト（Linux）でのみ数える（[`running_processes`] の doc）。
        // **pid の一致はこの計測に依らない**ので、`None` のホストでも共有の主張は残る。
        if let Some(running) = running {
            assert_eq!(
                running,
                1,
                "OS 上に存在する補助プロセスは 1 つだけである（配置: {}）",
                layout.display()
            );
        }
        assert!(shutdown.is_ok(), "終了は成功する: {shutdown:?}");
    }

    /// **要件 5.3**: ダイジェストが一致しない実行ファイルは、**起動を試みる前に**拒否される。
    ///
    /// 本番の解決（[`resolve_executable`]）が指すパスを使い、期待ダイジェストだけを差し替えて
    /// 「不一致」を作る（tasks.md 3.2 の `with_verifier` seam）。起動が試みられていないことは、
    /// 期待ダイジェストを**正しい値**に戻せば起動できることと、拒否の理由が
    /// `IntegrityMismatch { source: IntegrityError::Mismatch }` であることで示す。
    #[test]
    fn a_mismatching_digest_is_rejected_before_any_start() {
        let _serial = sidecar_tests();
        let Some(layout) = prepare_layout() else {
            return;
        };

        let rejected = supervisor_with_digest([0xab; 32]).ensure(&SidecarSpec {
            kind: SidecarKind::Smoke,
            executable: layout.clone(),
            args: Vec::new(),
        });
        match rejected {
            Err(SpawnError::IntegrityMismatch { path, source }) => {
                assert_eq!(path, layout, "拒否の報告に解決したパスが載る");
                assert!(
                    matches!(source, IntegrityError::Mismatch { .. }),
                    "内容の不一致として区別できる（実際: {source}）"
                );
            }
            other => panic!("整合性検査の失敗として拒否される（実際: {other:?}）"),
        }

        // 起動そのものは試みられていない（拒否のあとに OS 上のプロセスは 0 のまま）。
        // **OS 上の数を照合できるホスト（Linux）に限る**（[`running_processes`] の doc）。
        if let Some(running) = running_processes(&layout) {
            assert_eq!(running, 0);
        }

        // 正しいダイジェスト（本番の検査）なら起動する — 拒否がファイルの不在や実行権限では
        // ないことを同じ前提で裏付ける。
        let supervisor = test_supervisor();
        let started = SidecarHost::new(supervisor.clone()).ensure(SidecarKind::Smoke, Vec::new());
        let pid = started.map(|handle| handle.pid());
        let shutdown = supervisor.shutdown_all();
        assert!(pid.is_ok(), "本番の検査では起動できる: {pid:?}");
        assert!(shutdown.is_ok());
    }

    /// **要件 5.9 / 5.7**: 購読が補助プロセスの出力と終了を記録へ流す。
    ///
    /// 記録機構（`tauri-plugin-log`）の代わりに `log` の面へ捕獲用の記録器を取り付ける。本番の
    /// 記録先は同じ `log` の面の先にあるため、**水準と対象名を含めて**ここで確認できる。
    #[test]
    fn the_subscription_records_the_output_and_the_exit() {
        let _serial = sidecar_tests();
        install_capture_logger();

        let Some(_layout) = prepare_layout() else {
            return;
        };

        let supervisor = test_supervisor();
        let host = SidecarHost::new(supervisor.clone());
        let subscriber = host.watch_output().expect("購読スレッドを起動できる");
        let handle = host
            .ensure(SidecarKind::Smoke, Vec::new())
            .expect("補助プロセスを起動できる");

        // 補助プロセスは起動時に `sidecar-smoke ready pid=…` を標準出力へ書く（毎行 flush）。
        // **このハンドルの識別子を含む行**を待つので、以前の行が残っていても条件が満たされた
        // ことにはならない。
        let ready = format!("sidecar-smoke ready pid={}", handle.pid());
        assert!(
            wait_for_record(
                |line| line.contains("[jxcel::sidecar][INFO]")
                    && line.contains("kind=sidecar-smoke stream=stdout")
                    && line.contains(&ready),
                Duration::from_secs(10),
            ),
            "補助プロセスの標準出力が Info で記録される（実際: {:?}）",
            captured(),
        );

        // 直接の子を終了させる。監督の監視スレッドが読み取りの終了を待ってから Exited を配るので、
        // 記録には最後の出力の後に「予期せぬ終了」が現れる（要件 5.7）。
        handle.kill().expect("直接の子を終了できる");
        assert!(
            wait_for_record(
                |line| line.contains("[jxcel::sidecar][WARN]")
                    && line.contains("補助プロセスが予期せず終了した")
                    && line.contains("kind=sidecar-smoke"),
                Duration::from_secs(10),
            ),
            "予期せぬ終了が Warn で記録される（実際: {:?}）",
            captured(),
        );

        let shutdown = supervisor.shutdown_all();
        assert!(shutdown.is_ok());
        drop(subscriber);
    }

    /// 解決が実際に指すパスから起動されたプロセスの数を数える（Linux のみ。`/proc` を直接読む）。
    ///
    /// **数を観測できないホストでは `None` を返す。** macOS の `ps comm` は 16 文字に切詰められ、
    /// Windows の Toolhelp32 はイメージ名しか与えないため、起動時の絶対パスとの照合が成立しない。
    /// 呼び出し側は `None` を「このホストでは OS 上の数を観測できない」として扱い、**数を根拠に
    /// する判定を省く**（黙って 0 や 1 を返してはならない — 以前は Linux 以外で常に 1 を返して
    /// おり、「拒否のあとにプロセスが 0」という判定が macOS / Windows で必ず落ちていた）。
    /// **この計測は Linux の補助に過ぎない** — 共有の主張は pid の一致が担う（3 OS の実行時確認は
    /// 10.x）。
    #[cfg(target_os = "linux")]
    fn running_processes(layout: &Path) -> Option<usize> {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Some(0);
        };
        Some(
            entries
                .flatten()
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.parse::<u32>().is_ok())
                })
                .filter(|entry| {
                    let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
                        return false;
                    };
                    let first = cmdline.split(|byte| *byte == 0).next().unwrap_or_default();
                    first == layout.as_os_str().as_encoded_bytes()
                })
                .count(),
        )
    }

    /// Linux 以外では `/proc` が無く、絶対パスとの照合が成立しない（上の doc を参照）。
    /// **数を返さない**（`None`）。
    #[cfg(not(target_os = "linux"))]
    fn running_processes(_layout: &Path) -> Option<usize> {
        None
    }

    // -----------------------------------------------------------------------
    // 記録の捕獲（`log` の面へ取り付ける最小の記録器）
    // -----------------------------------------------------------------------

    /// 捕獲した記録の行（`[対象名][水準] 本文`）。試験の間だけの控えである。
    static CAPTURED: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

    fn captured() -> Vec<String> {
        CAPTURED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// `log` の面へ取り付ける捕獲用の記録器。
    struct CaptureLogger;

    impl log::Log for CaptureLogger {
        fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
            true
        }

        fn log(&self, record: &log::Record<'_>) {
            let line = format!(
                "[{}][{}] {}",
                record.target(),
                record.level(),
                record.args()
            );
            CAPTURED
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(line);
        }

        fn flush(&self) {}
    }

    /// 捕獲用の記録器を取り付ける。**`log` の面に取り付けられる記録器は 1 プロセスに 1 つだけ**
    /// なので、2 度目以降の呼び出しは何もしない。
    fn install_capture_logger() {
        static INSTALLED: LazyLock<()> = LazyLock::new(|| {
            static LOGGER: CaptureLogger = CaptureLogger;
            log::set_logger(&LOGGER).expect("この試験ではまだ記録器が取り付いていない");
            log::set_max_level(log::LevelFilter::Trace);
        });
        LazyLock::force(&INSTALLED);
    }

    /// 記録に条件を満たす行が現れるまで待つ（現れなければ期限で偽を返す）。
    fn wait_for_record(predicate: impl Fn(&str) -> bool, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if captured().iter().any(|line| predicate(line)) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

//! アプリケーションのライフサイクル — 起動の順序、単一インスタンス、起動を継続できない
//! 前提不成立の報告を確定する。
//!
//! 所有: `AppLifecycle`（design.md「Components and Interfaces → Adapter Layer」）。
//! 要件: 1.2, 1.4, 1.5, 1.6, 2.8, 2.9, 8.1, 8.2, 8.5, 8.7, 10.3。
//!
//! タスク 5.1 が置いた実体は次の 3 つである:
//!
//! 1. **起動順序の固定**（[`run`]）。順序は 1 箇所の直線的な関数として書き、
//!    「回避策の予約点 → 単一インスタンスの登録 → 診断の初期化 → 構築 → 残留プロセスの掃除 →
//!    実行」を本体の並びそのもので表す。**掃除だけがタスク 5.1 の文言と位置が異なる** —
//!    構築の前に置くと、引き継ぎ側の 2 つ目のプロセスも掃除を実行して動作中のアプリの補助
//!    プロセスを終了させる（要件 5.5・5.6 を破る）。理由と実測は [`run`] の手順 5 にある。
//! 2. **単一インスタンス化**（要件 1.5）。プラグインは**最初に**登録する。二重起動は
//!    2 つ目のプロセスが引数を引き渡して自ら終了する形で成立し（プラグインの実装が
//!    `exit(0)` する。research.md「単一インスタンス化の実際の挙動」）、引き継いだ側が
//!    要求に対応するウィンドウを提示する。
//! 3. **起動を継続できない前提不成立の報告**（要件 1.4）。満たされなかった前提を名指しし、
//!    **無言で終了しない**（[`StartupError`] / [`report_startup_failure`]）。
//!
//! タスク 5.2 が加えたのは**記録機構の登録と保持方針の適用、および実効設定の起動時確認**である
//! （要件 8.1、8.5、8.7）。記録機構 `tauri-plugin-log` の既定は 40 KB / `KeepOne` であり、
//! 要件 8.5 の 50 MB と桁が違う。したがって方針値（`crates/app-shell/src/diagnostics.rs` の
//! [`diagnostics::MAX_LOG_FILE_BYTES`] / [`diagnostics::KEEP_SOME_ARCHIVED_FILES`]）で
//! **明示的に上書きする**。登録は [`register_logging`]、実効設定の組み立ては [`logging_config`]、
//! 起動時の確認は [`confirm_effective_logging`] が担う。手順の並びは [`run`] のとおりで、
//! 記録機構は単一インスタンス（手順 2）の後・構築（手順 4）の前に登録する。
//!
//! 本ファイルがまだ持たないもの（各タスクがここへ書き込む）:
//!
//! - タスク 5.4: 最後のウィンドウを閉じたときの終了と常駐慣習の扱い（要件 2.8, 2.9）。
//!   実行時のコールバック（[`run`] の手順 6）が結線点である。
//! - タスク 5.5: 異常終了の記録（要件 8.2）。
//! - タスク 8.3: 描画の代替経路の判定と適用（要件 10.3）。
//!   [`reserve_render_fallback_point`] の中身を埋める。
//! - タスク 6.1 / 9.6: 引き継いだ起動要求と、ウィンドウおよびドキュメントの対応付け。
//!   [`present_window_for_request`] が seam である。

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use app_shell::diagnostics::{self, DiagnosticsLevel};
use app_shell::settings::{self, FileSettingsStore, RecoveredFrom, SettingsStore};
use app_shell::sidecar::{SidecarSupervisor, Supervisor};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_log::log::{self, LevelFilter};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

/// 起動を継続できない前提の名前: アプリケーションデータ領域（設定の保存先）。
const PREREQUISITE_APP_DATA: &str = "アプリケーションデータ領域";

/// 起動を継続できない前提の名前: 診断情報（記録）の保存先。
const PREREQUISITE_DIAGNOSTICS: &str = "診断情報の保存先";

/// 起動を継続できない前提の名前: Tauri ランタイムとウィンドウの構築。
const PREREQUISITE_RUNTIME: &str = "Tauri ランタイム";

/// 起動失敗の記録を残すファイル名。実行ファイルの隣、書けなければ OS の一時ディレクトリに置く。
const STARTUP_FAILURE_FILE_NAME: &str = "jxcel-startup-error.log";

/// 記録機構が書く**記録中のファイル**の名前（拡張子を除く）。ローテーション済みのファイルは
/// `{これ}_{日時}.log` になる（`tauri-plugin-log` 2.9.1 の `RotatingFile`）。
///
/// **プラグインの既定（`package_info().name`）に任せず明示する。** 起動時の確認
/// （[`confirm_effective_logging`]）が「記録機構が実際にこの名前のファイルを書けた」ことを
/// 検証できるようにし、確認の対象を `tauri.conf.json` の `productName` に依存させないためである。
/// 4.4 の書き出し（`diagnostics::export`）は拡張子 `log` のファイルだけを連結するので、
/// この名前はその対象（`jxcel.log` / `jxcel_*.log`）に含まれる。
const LOG_FILE_STEM: &str = "jxcel";

// ---------------------------------------------------------------------------
// 起動の順序（要件 1.2, 1.4, 1.5）
// ---------------------------------------------------------------------------

/// 起動の唯一の入口。**手順の並びがこの関数の本体そのものである。**
///
/// 手順を入れ替えてはならない。それぞれの理由は各手順のコメントにある。構築（手順 4）の
/// 直前まで `builder` を値として持ち回るため、順序を変えるにはこの関数を書き換える必要がある。
/// 掃除（手順 5）だけはタスク 5.1 の文言と位置が異なる — 理由はその手順のコメントにある。
///
/// # Errors
///
/// 起動を継続できない前提が満たされないとき [`StartupError`] を返す。呼び出し元（`main`）は
/// [`report_startup_failure`] でメッセージを提示し、非 0 の終了コードで終える（要件 1.4）。
pub fn run() -> Result<(), StartupError> {
    // 手順 1: 描画の回避策を適用する場所の予約（中身は 8.3 が埋める。ここでは判定しない）。
    //   回避策の環境変数は GTK / WebKit のコードが動く前に設定しなければならない。GTK /
    //   WebKit のランタイムは手順 4 の `Builder::build` で生成される（tauri の `Runtime::new`）。
    //   したがって予約点は手順 4 より前でなければならない（決定 7、research.md 実測）。
    //   **無条件には適用しない**（要件 10.3。一部の環境の問題を全環境の性能低下と
    //   引き換えに直さない）。
    reserve_render_fallback_point();

    // 手順 2: 単一インスタンスの登録。**最初のプラグインとして登録する。**プラグインの
    //   初期化は登録順に行われるため、後続タスクが足すプラグイン（5.2 の記録機構など）より
    //   前に置かなければならない。2 つ目のプロセスは手順 4 の初期化中に引数を引き渡して
    //   自ら終了する（この関数は 2 つ目のプロセスでは戻らない）。
    let builder = register_single_instance(tauri::Builder::default());

    // 手順 3: 診断の初期化。設定ストアと診断の保存先をここで解決・準備し、記録機構へ渡す
    //   実効設定（4.4 の方針値 + 設定から読んだ詳細度）を組み立てる。前提が満たせない場合は
    //   `?` で抜け、`main` が満たされなかった前提を名指しして非 0 で終了する（要件 1.4。
    //   無言で終了しない）。**破損した設定はここで中止しない**（要件 7.5）。
    let startup = init_diagnostics()?;

    // 記録機構の実効設定（要件 8.1、8.5、8.7）。**4.4 の方針値をそのまま使い、プラグインの
    // 既定（40 KB / `KeepOne`）を明示的に上書きする。**詳細度は 5.1 が開いた設定ストアから
    // 読む（同じディレクトリのストアを 2 つ開かない。要件 7.3）。
    let logging = logging_config(startup.log_dir(), &**startup.settings());

    // 実効設定を構築の前に検める。ここで前提不成立を名指ししておくと、方針を適用できない
    // 原因が記録機構の初期化失敗（プラグインの setup が返す不透明な `tauri::Error` に
    // 埋もれる）ではなく「診断情報の保存先」であることが利用者に伝わる（要件 1.4）。
    confirm_logging_values(&logging)?;

    // 手順 3.5: 記録機構の登録。単一インスタンス（手順 2）の後・構築（手順 4）の前である。
    let builder = register_logging(builder, &logging);
    let builder = builder.manage(startup);

    // 手順 4: 構築。ここで GTK / WebKit のランタイムと、登録順に各プラグインが初期化される。
    //   **記録機構のロガーもここで取り付けられる**（`tauri-plugin-log` の `setup`）。そのため
    //   これより前の `log::…!` はどこにも残らない。
    //   **単一インスタンスの 2 つ目のプロセスはこの中で引数を引き渡して自ら終了する**
    //   （この関数は 2 つ目のプロセスでは戻らない）。
    let app = builder
        .build(tauri::generate_context!())
        .map_err(|error| StartupError::new(PREREQUISITE_RUNTIME, error.to_string()))?;

    // 手順 4.5: 記録機構の実効設定を起動時に確認する（要件 8.1、8.5）。起動行を記録し、
    //   記録中のファイルが方針の保存先に現れたことを確かめる。書けなければ診断の保存先の
    //   前提不成立として報告する（無言で劣化させない）。
    confirm_effective_logging(&logging)?;

    // 手順 5: 残留プロセスの掃除（要件 5.6、タスク 3.5）。前回の実行が終了処理を走らせられずに
    //   残した補助プロセスを終了させる。
    //
    //   **位置の意図的な逸脱（タスク 5.1 の文言は掃除を構築の前に置く）**: 構築の前に掃除すると、
    //   引き継ぎ側の 2 つ目のプロセスも掃除を実行してしまい、動作中のアプリが使っている補助
    //   プロセスを終了させる（実測で再現。要件 5.5・5.6 を破り、8.1 が実物の補助プロセスを
    //   持った時点で実害になる）。単一インスタンスの判定は手順 4 のプラグイン初期化で行われるため、
    //   **生存している側だけがこの位置に到達する。**ここは依然として「アプリが使えるように
    //   なる前」である — ウィンドウは手順 6 の `RunEvent::Ready` で初めて作られる。
    let swept = sweep_orphans_at_startup();
    // 記録機構は手順 3.5 で登録済みであり、この行は方針の保存先（要件 8.1）へ残る。
    log::info!("残留プロセスの掃除で {swept} 件を終了した");

    // 手順 6: 実行。終了条件（最後のウィンドウ・常駐慣習）は 5.4 がこのコールバックへ結線する。
    app.run(|_app, _event| {});

    Ok(())
}

/// 手順 1: 描画の回避策を適用するために確保した予約点（**未実装**）。
///
/// タスク 8.3 がここに「設定に残した印を読む → 必要なら回避策の環境変数を適用する →
/// 適用した事実を記録する」を埋める。**この段では判定処理を書かない**（タスク 5.1 の指示。
/// 判定と適用は 8.3 が所有する）。8.3 はこの関数の中身だけを置き換える — 呼び出し位置
/// （`run` の手順 1）は順序の制約そのものであるため動かさない。
///
/// 位置の根拠: 回避策の環境変数は GTK と WebKit のコードが動く前に設定しなければならず、
/// 検出した時点では既に手遅れである（design.md「RenderWatchdog」）。検出時は設定に印を残し、
/// **次回の起動で構築の前に読んで適用する**（要件 10.3）。
fn reserve_render_fallback_point() {
    // 判定と適用は 8.3 が埋める。ここは順序の予約だけを担う。
}

/// 手順 2: 単一インスタンスのプラグインを**最初のプラグインとして**登録する（要件 1.5）。
///
/// コールバック [`handover`] は、2 つ目のプロセスが自分の引数を引き渡して終了した後、
/// **既に動作している側**で呼ばれる。ここで登録した 1 つだけが最初であり、後続タスクは
/// これより後ろにプラグインを足すこと（5.2 の記録機構など）。
fn register_single_instance(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.plugin(tauri_plugin_single_instance::init(handover))
}

/// 手順 5: 前回の実行が残した補助プロセスを掃除する（要件 5.6、タスク 3.5）。
///
/// **構築の後（単一インスタンスの初期化が済んだ後）に呼ぶ。**タスク 5.1 の文言は掃除を構築の
/// 前に置くが、その位置では引き継ぎ側の 2 つ目のプロセスが「動作中のアプリの補助プロセス」を
/// 残留と誤認して終了させる（実測で再現。要件 5.5・5.6 を破る）。ここへ移すと、単一インスタンス
/// プラグインが手順 4 で 2 つ目のプロセスを終了させるため、**生存している側だけが掃除に到達する**。
///
/// 掃除は最善努力であり、失敗を報告しない（戻り値は終了させた数だけである。[`Supervisor`] の
/// 契約）。起動を止めてはならない（要件 5.4 の精神）。
fn sweep_orphans_at_startup() -> usize {
    let supervisor = Supervisor::new().with_expected_executables(expected_sidecar_executables());
    supervisor.sweep_orphans()
}

/// 残留の掃除が名前の一致に加えて要求する「このアプリの補助プロセスの実行ファイルの絶対パス」。
///
/// **今は空を返す。**この解決はタスク 8.1（`SidecarHost`）が所有する。8.1 がプラットフォーム別の
/// パス解決を実装した時点で、その結果がここへ流れる。空の間は実行ファイル名の一致だけで掃除する
/// （[`Supervisor`] の既定挙動）。
///
/// 空を選ぶ理由（配線を 8.1 に残す理由）: macOS では `ps -p <pid> -o comm=` が起動時パスではなく
/// コマンド名（カーネルの `p_comm` は 16 文字で切詰め）を返すため、**期待パスを設定すると
/// 絶対パス比較が一致せず掃除が無音で no-op になる**（tasks.md 3.5 の申し送り）。8.1 が期待パスを
/// 配線する前に macOS の実機（10.x の CI）で名前照合が成立することを確認しなければならない。
/// 今ここで推測のパスを入れると、この no-op を自分で作ることになる。
fn expected_sidecar_executables() -> Vec<PathBuf> {
    Vec::new()
}

// ---------------------------------------------------------------------------
// 診断の初期化と起動の前提（要件 1.4, 7.2, 7.5, 8.1）
// ---------------------------------------------------------------------------

/// 手順 3: 起動の前提を解決・準備し、後続タスクが使う状態を返す。
///
/// ここで準備するのは 2 つである:
///
/// - **設定ストア**（要件 7.2）。OS 標準のアプリケーションデータ領域の下に
///   [`settings::open`] で開く。ディレクトリを用意できないことは**起動の前提不成立**である
///   （design.md「Error Handling」の分類表の「起動時の前提不成立 = 設定ディレクトリを作成
///   できない」がこれに対応する）。**設定の内容を読めないことは前提不成立ではない**（要件 7.5）—
///   既定値で起動して事実を [`StartupState`] に載せ、記録に残す。壊れたファイルは削除しない。
/// - **診断の保存先**（要件 8.1）。[`diagnostics::log_dir`] は場所だけを返す。ディレクトリの
///   用意（作成と書き込み可能性の確認）は記録機構を登録する 5.2 の前提なので、ここで行う
///   （[`prepare_log_directory`]）。記録機構の `setup` に任せると、方針を適用できない原因が
///   プラグインが返す不透明な `tauri::Error` に埋もれる。ここで前提不成立として名指しする。
///
/// # Errors
///
/// 上記のいずれかを解決・準備できないとき [`StartupError`] を返す。
fn init_diagnostics() -> Result<StartupState, StartupError> {
    let settings_dir = settings::app_data_dir()
        .map_err(|error| StartupError::new(PREREQUISITE_APP_DATA, error.to_string()))?;
    let (settings, report) = settings::open(&settings_dir)
        .map_err(|error| StartupError::new(PREREQUISITE_APP_DATA, error.to_string()))?;

    // 破損・未知の版・読み取り不能は**起動を中止しない**（要件 7.5、タスク 4.2）。
    // 既定値で起動し、事実を残す。5.2 が記録機構を登録した後にここを再度記録できるよう、
    // `StartupState` にも載せる（[`StartupState::recovered_from`]）。
    let recovered = report.recovered_from().cloned();
    if let Some(fact) = &recovered {
        log::warn!("設定を読み取れなかったので既定値で起動する: {fact}");
    }

    let log_dir = diagnostics::log_dir()
        .map_err(|error| StartupError::new(PREREQUISITE_DIAGNOSTICS, error.to_string()))?;
    prepare_log_directory(&log_dir).map_err(|error| {
        // どのディレクトリを用意できなかったかを名指しする（原因が「先客の通常ファイル」でも
        // 「権限なし」でも、利用者が場所を特定できるようにする）。
        StartupError::new(
            PREREQUISITE_DIAGNOSTICS,
            format!("{}: {error}", log_dir.display()),
        )
    })?;

    Ok(StartupState { settings, log_dir, recovered })
}

/// 記録の保存先を用意する（要件 8.1、8.5。タスク 5.2）。
///
/// 4.4 の [`diagnostics::log_dir`] は場所を返すだけでディレクトリを作らないため、記録機構を
/// 登録する前にここで作る。**作成だけでなく書き込み可能性まで確かめる** — `create_dir_all` は
/// 既存の読み取り専用ディレクトリでも成功し、その場合記録機構は記録を 1 行も残せない。
///
/// 失敗は [`StartupError`]（前提「診断情報の保存先」）として報告し、**無言で劣化させない**
/// （要件 1.4）。確認用の一時ファイルは通常ファイルとして残さない（拡張子 `log` を付けず、
/// 4.4 の書き出し・プラグインのローテーションの対象から外したうえで削除する）。
fn prepare_log_directory(directory: &Path) -> std::io::Result<()> {
    fs::create_dir_all(directory)?;
    let probe = directory.join(format!(".jxcel-write-probe-{}", std::process::id()));
    fs::write(&probe, b"")?;
    fs::remove_file(&probe)
}

// ---------------------------------------------------------------------------
// 記録機構の登録と保持方針の適用（要件 8.1, 8.5, 8.7。タスク 5.2）
// ---------------------------------------------------------------------------

/// 5.2 が記録機構へ実際に渡した実効設定。
///
/// 起動時の確認（[`confirm_logging_values`] / [`confirm_effective_logging`]）が、4.4 の方針値が
/// そのまま渡ったことと、保持の不変条件（(アーカイブ数 + 記録中の 1) × 1 ファイル上限が合計
/// 上限以下）を照合するための値を持つ。組み立ては [`logging_config`] の 1 箇所だけである。
struct LoggingConfig {
    /// 4.4 が解決した保存先（方針と記録機構で唯一の値）。
    directory: PathBuf,
    /// 記録中のファイルの語幹（[`LOG_FILE_STEM`]）。
    file_stem: &'static str,
    /// 1 ファイル上限（[`diagnostics::MAX_LOG_FILE_BYTES`]）。
    max_file_bytes: u64,
    /// 保持するアーカイブ数（[`diagnostics::KEEP_SOME_ARCHIVED_FILES`]）。
    archived_files: usize,
    /// 保持されうる総ファイル数（アーカイブ + 記録中）。
    retained_files: u64,
    /// 保持されうる合計バイト数。
    retained_bytes: u64,
    /// 適用する詳細度（4.5 の設定値）。
    level: DiagnosticsLevel,
}

impl LoggingConfig {
    /// 記録機構が書く記録中のファイルの絶対パス（`{保存先}/{語幹}.log`）。
    fn active_file(&self) -> PathBuf {
        self.directory.join(format!("{}.log", self.file_stem))
    }
}

/// 4.4 の方針値と設定の詳細度から、記録機構へ渡す実効設定を組み立てる（要件 8.5、8.7）。
///
/// **保存先は 4.4 が解決した [`diagnostics::log_dir`] の結果をそのまま使う。** プラグインの
/// `TargetKind::LogDir` を使うと保存先の算出がプラグイン側にもう 1 つ生まれ、方針と食い違う
/// 余地が残る（両者が同じ規約であることは今一致しているだけで、構造的な保証ではない）。
/// 明示的な `TargetKind::Folder { path }` に 4.4 の結果を渡すことで、**両者が同じ値である
/// ことが構造的に保証される**（食い違わせるにはこの関数を書き換えるしかない）。
///
/// 詳細度は 5.1 が開いた設定ストアから読む（要件 7.3。同じディレクトリの実体を共有し、
/// 2 つ目のストアを開かない）。解釈できない値は [`DiagnosticsLevel::default`]（Info）に落ちる
/// （4.5 の契約）。
fn logging_config(log_dir: &Path, settings: &impl SettingsStore) -> LoggingConfig {
    LoggingConfig {
        directory: log_dir.to_path_buf(),
        file_stem: LOG_FILE_STEM,
        max_file_bytes: diagnostics::MAX_LOG_FILE_BYTES,
        archived_files: diagnostics::KEEP_SOME_ARCHIVED_FILES as usize,
        retained_files: diagnostics::RETAINED_LOG_FILES,
        retained_bytes: diagnostics::MAX_RETAINED_LOG_BYTES,
        level: DiagnosticsLevel::from_store(settings),
    }
}

/// 記録機構を登録する（要件 8.1、8.5）。
///
/// **プラグインの既定を明示的に上書きする。** `tauri-plugin-log` 2.9.1 の既定は 1 ファイル
/// 40 KB / `RotationStrategy::KeepOne` であり、要件 8.5 の合計 50 MB と桁が違う。上書きしないと
/// 記録は直近 40 KB しか残らない（research.md「ログと設定の永続化」）。
///
/// - `max_file_size`: [`diagnostics::MAX_LOG_FILE_BYTES`]（8 MB）
/// - `rotation_strategy`: [`RotationStrategy::KeepSome`]（
///   [`diagnostics::KEEP_SOME_ARCHIVED_FILES`] = 5）。**`KeepSome(n)` が保持するのはアーカイブ
///   n 個であり、記録中の現行ファイルを含まない**（プラグインの `RotatingFile::remove_old_files`
///   は現行ファイルを除外する）。起動時に `remove_old_files(n)`、ローテーション直前には
///   アーカイブする 1 個分の余地を空けるため `remove_old_files(n - 1)` を呼ぶ。したがって
///   保持される総ファイル数は [`diagnostics::RETAINED_LOG_FILES`] = n + 1 = 6 である
/// - `level`: 設定から読んだ詳細度を `log::LevelFilter` へ 1 対 1 で対応付けたもの
///   （[`to_level_filter`]）
///
/// 対象（`TargetKind`）は標準出力と方針の保存先の 2 つである。**フロントエンドへ転送する
/// `TargetKind::Webview` は含めない** — 記録をウェブビューへ届けるには各ウィンドウが
/// `attachConsole` で購読する必要があり、本スペックのどのタスクもそれを要求していない。含めれば
/// `log://log` イベントの購読とフロント側の実装が前提になり、capability を増やさずに済む現状を
/// 無理に広げることになる（`src-tauri/capabilities/default.json` は `core:default` のまま）。
/// フロントエンドの記録を必要とするタスクが、その時点で capability と併せて追加する。
fn register_logging(
    builder: tauri::Builder<tauri::Wry>,
    config: &LoggingConfig,
) -> tauri::Builder<tauri::Wry> {
    let logger = tauri_plugin_log::Builder::new()
        .targets([
            Target::new(TargetKind::Stdout),
            // 方針の保存先をそのまま渡す（`LogDir` を使わない理由は `logging_config` の doc）。
            Target::new(TargetKind::Folder {
                path: config.directory.clone(),
                file_name: Some(config.file_stem.to_owned()),
            }),
        ])
        .max_file_size(config.max_file_bytes as u128)
        .rotation_strategy(RotationStrategy::KeepSome(config.archived_files))
        .level(to_level_filter(config.level));
    builder.plugin(logger.build())
}

/// 実効設定が方針と一致し、かつ保存先をディレクトリとして使えることを構築の前に確かめる
/// （要件 8.1、8.5）。
///
/// 4.4 のコンパイル時検査（`MAX_RETAINED_LOG_BYTES <= MAX_TOTAL_LOG_BYTES`）が方針値どうしの
/// 不変条件の一次的な守りである。この関数は**実際に記録機構へ渡す値（[`LoggingConfig`]）が
/// その方針値と一致し、保持の不変条件を満たすこと**を起動時に再確認する。
///
/// # Errors
///
/// 値の不一致、不変条件の破れ、保存先がディレクトリでない（または解決できない）とき
/// [`StartupError`]（前提「診断情報の保存先」）。
fn confirm_logging_values(config: &LoggingConfig) -> Result<(), StartupError> {
    let archived = config.archived_files as u64;
    if config.max_file_bytes != diagnostics::MAX_LOG_FILE_BYTES
        || archived != u64::from(diagnostics::KEEP_SOME_ARCHIVED_FILES)
        || config.retained_files != archived + 1
        || config.retained_bytes != config.max_file_bytes * config.retained_files
        || config.retained_bytes > diagnostics::MAX_TOTAL_LOG_BYTES
    {
        return Err(StartupError::new(
            PREREQUISITE_DIAGNOSTICS,
            format!(
                "記録の保持方針が実効値と一致しない（1ファイル上限={} B、アーカイブ={}、保持総数={}、保持合計={} B。方針: 合計上限={} B / 保持総数={}）",
                config.max_file_bytes,
                archived,
                config.retained_files,
                config.retained_bytes,
                diagnostics::MAX_TOTAL_LOG_BYTES,
                diagnostics::RETAINED_LOG_FILES,
            ),
        ));
    }
    if !config.directory.is_dir() {
        return Err(StartupError::new(
            PREREQUISITE_DIAGNOSTICS,
            format!(
                "記録の保存先 {} をディレクトリとして使えない",
                config.directory.display()
            ),
        ));
    }
    Ok(())
}

/// 適用後の実効設定を起動時に確認し、記録する（要件 8.1、8.5、8.7。タスク 5.2 の完了状態）。
///
/// 構築（`Builder::build`）の後に呼ぶ。ロガーはそこで初めて取り付けられ、これより前の
/// `log::…!` はどこにも残らないためである。行うことは 2 つ:
///
/// 1. **実効設定の起動行を記録する** — 保存先、1 ファイル上限、アーカイブ世代数、保持総数、
///    保持合計、詳細度を 1 行にまとめる。利用者・保守担当はこれで適用後の値を確認できる。
///    加えて、詳細度が `Debug` 以上のときだけ現れる行を 1 つ置く（詳細度が実際にフィルタへ
///    効いていることを、設定を変えて起動するだけで観察できるようにする）。
/// 2. **記録中のファイルが方針の保存先に現れたことを確かめる** — プラグインは対象の `setup`
///    で記録中のファイルを開く（`RotatingFile::new` の `open_file`）ので、詳細度が `Off` でも
///    ファイルは作られる。現れなければ記録機構を適用できていない。
///
/// # Errors
///
/// 記録中のファイルが方針の保存先に現れない（または通常ファイルでない）とき
/// [`StartupError`]（前提「診断情報の保存先」）。**無言で劣化させない**（要件 1.4）。
fn confirm_effective_logging(config: &LoggingConfig) -> Result<(), StartupError> {
    log::info!(
        "診断の実効設定: 保存先={} / 1ファイル上限={} B / アーカイブ世代={} / 保持総数={} ファイル / 保持合計={} B / 詳細度={:?}",
        config.directory.display(),
        config.max_file_bytes,
        config.archived_files,
        config.retained_files,
        config.retained_bytes,
        config.level,
    );
    // 詳細度が Debug 以上のときだけ残る確認行。4.5 の詳細度が実際にフィルタへ効いていることを、
    // 設定を変えて起動するだけで観察できるようにする。
    log::debug!(
        "診断の詳細度フィルタを確認: この行は詳細度が Debug 以上のときだけ現れる（現在={:?}）",
        config.level
    );

    let active = config.active_file();
    match fs::metadata(&active) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(StartupError::new(
            PREREQUISITE_DIAGNOSTICS,
            format!("記録中のファイル {} が通常ファイルでない", active.display()),
        )),
        Err(error) => Err(StartupError::new(
            PREREQUISITE_DIAGNOSTICS,
            format!("記録機構が {} へ記録を書けなかった: {error}", active.display()),
        )),
    }
}

/// 4.4 の詳細度を `log::LevelFilter` へ 1 対 1 で対応付ける（要件 8.7）。
///
/// 4.4 は `log` クレートに依存しない（Tauri 非依存のコアを保つ）ため、この対応付けはアダプタ層
/// が持つ（`crates/app-shell/src/diagnostics.rs` の `DiagnosticsLevel` の doc）。
fn to_level_filter(level: DiagnosticsLevel) -> LevelFilter {
    match level {
        DiagnosticsLevel::Off => LevelFilter::Off,
        DiagnosticsLevel::Error => LevelFilter::Error,
        DiagnosticsLevel::Warn => LevelFilter::Warn,
        DiagnosticsLevel::Info => LevelFilter::Info,
        DiagnosticsLevel::Debug => LevelFilter::Debug,
        DiagnosticsLevel::Trace => LevelFilter::Trace,
    }
}

/// 手順 3 が準備した状態。後続タスクが `AppHandle::state` から読む。
///
/// 5.1 は準備と前提確認を行い、記録機構の登録（5.2）が保存先と設定ストアをここから消費する。
/// 設定のコマンド面（7.1）と異常終了の記録（5.5）も同じ実体を読む。
pub struct StartupState {
    /// 解決済みの設定ストア（要件 7.x）。
    settings: Arc<FileSettingsStore>,
    /// 解決済みの診断の保存先。5.2 が記録機構を登録するときに使う（要件 8.1）。
    log_dir: PathBuf,
    /// 既定値で起動した事実（あれば）。**起動を中止しない**（要件 7.5）。
    recovered: Option<RecoveredFrom>,
}

#[allow(dead_code)] // recovered_from は 5.5（異常終了の記録）などが消費するまでの seam。
impl StartupState {
    /// 解決済みの診断の保存先。
    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    /// 解決済みの設定ストア。
    pub fn settings(&self) -> &Arc<FileSettingsStore> {
        &self.settings
    }

    /// 既定値で起動した事実（設定を読めなかった場合のみ）。
    pub fn recovered_from(&self) -> Option<&RecoveredFrom> {
        self.recovered.as_ref()
    }
}

// ---------------------------------------------------------------------------
// 起動を継続できない前提の報告（要件 1.4）
// ---------------------------------------------------------------------------

/// 起動を継続できない前提不成立。**満たされなかった前提を名指しできる形で持つ。**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupError {
    /// 満たされなかった前提の名前（[`PREREQUISITE_APP_DATA`] など）。
    prerequisite: &'static str,
    /// 前提が満たせなかった理由の詳細。
    detail: String,
}

impl StartupError {
    fn new(prerequisite: &'static str, detail: impl Into<String>) -> Self {
        Self { prerequisite, detail: detail.into() }
    }

    /// 満たされなかった前提の名前。メッセージの先頭に出す。
    pub fn prerequisite(&self) -> &'static str {
        self.prerequisite
    }
}

impl fmt::Display for StartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "起動を継続できません（満たされなかった前提: {}）: {}",
            self.prerequisite, self.detail
        )
    }
}

impl std::error::Error for StartupError {}

/// 前提不成立のメッセージを提示し、**記録を残す**。**無言で終了しない**（要件 1.4）。
///
/// # 提示の経路と、その限界（正直に記す）
///
/// 二段構えである:
///
/// 1. **stderr**（と非 0 の終了コード）。端末からの起動（開発・CI）ではそのまま見える。
/// 2. **実行ファイルの隣（書けなければ OS の一時ディレクトリ）の `jxcel-startup-error.log`**。
///    stderr が見えない起動 — Windows の GUI サブシステム（release はコンソールを持たない）、
///    macOS の `.app` を Finder から起動、Linux のファイルマネージャや `.desktop` からの起動 —
///    でも、失敗した事実と満たされなかった前提が後から確認できる。
///
/// したがって「満たされなかった前提を特定できるメッセージを提示する」は、**可視のダイアログ
/// なしでも事実として残る**。可視のダイアログ提示は設計上まだ無く、これはその代用である:
/// 親ウィンドウを指定したネイティブ提示を所有する **タスク 7.7** が、起動時エラーのダイアログ
/// 提示も併せて結線すべきである（`tauri-plugin-dialog` は `tauri-plugin-fs` を非オプションの
/// 通常依存に持ち、タスク 1.3 の「fs プラグインを依存に入れない」制約と衝突するため、7.7 は
/// `rfd` を直接使う選択も視野にこの緊張を解くこと）。利用者向けの導線（メニュー等）は
/// **タスク 9.5** が担う。メッセージの生成（[`StartupError`]）と提示（この関数）を分けているのは、
/// 7.7 が提示だけを差し替えられるようにするためである。
pub fn report_startup_failure(error: &StartupError) {
    let record = format!(
        "jxcel: {error}\njxcel: 起動を中止しました（前提「{}」を満たせません）。\n",
        error.prerequisite()
    );
    eprint!("{record}");
    match persist_startup_failure(&record) {
        Some(path) => eprintln!("jxcel: この内容を {} に記録しました。", path.display()),
        None => eprintln!("jxcel: この内容をファイルに記録できませんでした。"),
    }
}

/// 起動失敗の記録を、利用者が見つけられる場所へ最善努力で残す。
///
/// 優先順は (1) 実行ファイルの隣（配布物の隣で最も見つけやすい）、(2) OS の一時ディレクトリ
/// （実行ファイルの場所が読み取り専用のときの退避。macOS の `.app` の中や Windows の
/// `Program Files` 配下が該当する）。どちらにも書けなければ `None` を返す（既に stderr には
/// 出ているため、失敗しても起動中止の報告自体は成立する）。
///
/// 記録は毎回置き換える（失敗の履歴を無限に積まないため）。内容は stderr に出したものと同一である。
fn persist_startup_failure(record: &str) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join(STARTUP_FAILURE_FILE_NAME));
        }
    }
    candidates.push(std::env::temp_dir().join(STARTUP_FAILURE_FILE_NAME));
    candidates.into_iter().find(|path| fs::write(path, record).is_ok())
}

// ---------------------------------------------------------------------------
// 単一インスタンスの引き継ぎ（要件 1.5）
// ---------------------------------------------------------------------------

/// 二重起動の引き継ぎコールバック。既に動作している側で呼ばれる。
///
/// **2 つ目のプロセスはここに来ない。**プラグインは 2 つ目のプロセスの初期化中に、この
/// コールバックを D-Bus / 名前付きミューテックス / Unix ソケット経由で呼び出させたうえで、
/// `app.cleanup_before_exit()` の後に `exit(0)` する（research.md「単一インスタンス化の実際の
/// 挙動」、`tauri-plugin-single-instance` の実装）。したがって「2 つ目が常駐しない」ことは
/// プラグインの挙動であり、こちらが終了させるのではない。
///
/// `argv` の与えられ方はプラットフォームで異なる。[`LaunchRequest::from_argv`] が吸収する。
fn handover(app: &AppHandle, argv: Vec<String>, cwd: String) {
    let request = LaunchRequest::from_argv(&argv);
    tauri_plugin_log::log::info!("二重起動を引き継ぎました（cwd={cwd}）: {}", request.describe());
    present_window_for_request(app, request);
}

/// 引き継いだ起動要求。
struct LaunchRequest {
    /// 要求されたドキュメントの位置（あれば）。
    document: Option<PathBuf>,
}

impl LaunchRequest {
    /// 引き継いだ argv から要求を読む。
    ///
    /// - Linux / macOS は OS が渡した argv がそのまま入る。
    /// - **Windows はプラグインが argv 全体を `|` で連結した 1 要素として渡す**
    ///   （research.md「単一インスタンス化の実際の挙動」）。`|` は Windows のファイル名に
    ///   使えない文字なので実害は小さいが、ここで分解しておく。`|` を含みうる Unix のパスを
    ///   壊さないよう、分解は `cfg(windows)` に限る。
    fn from_argv(argv: &[String]) -> Self {
        let mut args: Vec<String> = Vec::new();
        #[cfg(windows)]
        for element in argv {
            args.extend(element.split('|').map(str::to_owned));
        }
        #[cfg(not(windows))]
        args.extend(argv.iter().cloned());

        // 先頭は実行ファイル自身である。`-` で始まる引数は現時点で解釈しない
        // （どの引数がドキュメントを指すかの規則は 6.1 / 9.6 が所有する）。
        let document = args
            .into_iter()
            .skip(1)
            .find(|argument| !argument.is_empty() && !argument.starts_with('-'))
            .map(PathBuf::from);
        Self { document }
    }

    /// 記録に出すための短い説明。
    fn describe(&self) -> String {
        match &self.document {
            Some(path) => format!("ドキュメント要求 {}", path.display()),
            None => "ドキュメント要求なし".to_owned(),
        }
    }
}

/// 要求に対応するウィンドウを提示する（要件 1.5）。
///
/// **これは seam である。**どのウィンドウを、どのドキュメントに関連付けて提示するかは
/// タスク 6.1（ウィンドウのレジストリとラベル規約）と 9.6（ドキュメントを関連付けていない
/// ウィンドウの操作導線）が所有する。5.1 の時点で保証するのは「要求に対応するウィンドウが
/// 提示される」ことだけであり、ドキュメントとの対応付けは行わない（引き渡したパスは記録に
/// 残すだけで、開かない）。
///
/// - ドキュメント要求がある場合: **別のドキュメントを開く要求**なので、既存のウィンドウを
///   閉じずに新しいウィンドウを提示する（要件 2.3）。
/// - ドキュメント要求が無い場合: 既にあるウィンドウを前面に出す（新規作成しない）。
fn present_window_for_request(app: &AppHandle, request: LaunchRequest) {
    if request.document.is_some() {
        create_handover_window(app);
        return;
    }
    match app.webview_windows().values().next().cloned() {
        Some(window) => focus_window(&window),
        // 起動直後に 1 枚も無い場合の保険。通常は `tauri.conf.json` の宣言が先に開いている。
        None => create_handover_window(app),
    }
}

/// 引き継ぎで新しいウィンドウを 1 枚提示する。
///
/// **ウィンドウ生成は非同期で行う。**同期のコマンドやイベントハンドラの中で生成すると一部の
/// プラットフォームで停止する（design.md「WindowManager」）。このコールバックは Linux では
/// D-Bus の受信スレッド、macOS では非同期タスク、Windows ではウィンドウメッセージの処理中に
/// 呼ばれるため、生成は非同期ランタイムへ逃がす。
///
/// ラベルは暫定の `handover-<連番>` である。ラベル規約 `doc-<連番>` / `empty-<連番>` と
/// レジストリは 6.1 が所有するため、ここでは衝突を避ける最小の一意化だけを行う。6.1 が
/// レジストリを導入したら、この関数は `WindowManager` の生成経路への呼び出しに置き換わる。
fn create_handover_window(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let label = handover_label(&app);
        match WebviewWindowBuilder::new(&app, label.clone(), WebviewUrl::default())
            .title("jxcel")
            .build()
        {
            Ok(window) => focus_window(&window),
            Err(error) => {
                // ウィンドウを提示できないことは本体の継続を妨げない（要件 2.10 の精神）。
                tauri_plugin_log::log::error!("要求されたウィンドウを提示できない: {error}");
            }
        }
    });
}

/// 引き継ぎで作るウィンドウのラベル。既存のラベルと衝突しない最小の連番を選ぶ。
fn handover_label(app: &AppHandle) -> String {
    let mut ordinal = app.webview_windows().len() + 1;
    loop {
        let label = format!("handover-{ordinal}");
        if !app.webview_windows().contains_key(&label) {
            return label;
        }
        ordinal += 1;
    }
}

/// ウィンドウを復元して前面に出す。失敗は記録に残すだけで、引き継ぎの成功を妨げない。
fn focus_window(window: &tauri::WebviewWindow) {
    if let Err(error) = window.unminimize() {
        tauri_plugin_log::log::warn!("ウィンドウを復元できない: {error}");
    }
    if let Err(error) = window.set_focus() {
        tauri_plugin_log::log::warn!("ウィンドウを前面に出せない: {error}");
    }
}

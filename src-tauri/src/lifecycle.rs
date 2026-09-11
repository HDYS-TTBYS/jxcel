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
//! タスク 5.3 が加えたのは**通信内容保護方針による外部ネットワーク経路の遮断と、その実効値の
//! 起動時確認**である（要件 1.6、8.3）。方針そのものは `tauri.conf.json` の `app.security.csp`
//! にあり、`connect-src` を通信境界（IPC）の宛先だけに限定する。取り出しは [`csp_config`]、
//! 構築前の確認は [`confirm_csp_values`]、構築後の起動行と再確認は [`confirm_effective_csp`] が
//! 担う。**`connect-src` に IPC の宛先が欠けると、通信境界の呼び出しが警告 1 行だけを残して
//! 低速な文字列経路へ恒久的に降格する**（tauri#12835）ため、構築の前に落とす。
//! HTTP クライアントのプラグインは依存に入れない（`src-tauri/Cargo.toml`。要件 1.6 の
//! 「経路が構造的に存在しない」側の担保）。
//!
//! タスク 5.4 が加えたのは**最後のウィンドウを閉じたときの終了と、常駐慣習の扱い**である
//! （要件 2.8、2.9）。既定は 3 OS 共通で「最後のウィンドウが閉じたら終了する」であり、
//! Tauri の既定をそのまま使う（該当分岐は `cfg` で切られておらず macOS の常駐慣習には
//! 従わない。research.md「最後のウィンドウを閉じたときの挙動」）。常駐させるのは**常駐の
//! 慣習を持つプラットフォーム（macOS）**で、かつ**終了コードが指定されていない終了要求**
//! （`code: None` = 最後のウィンドウが閉じられた経路）だけである（[`vetoes_exit`]）。
//! **無条件に拒否しない** — 明示的な終了操作（[`request_exit`]）は [`ExitControl`] の掛け金を
//! 立ててから `app.exit(0)` を呼び、以後は拒否が起きない（tauri#13511 の限界については
//! [`vetoes_exit`] の doc）。Dock アイコンのクリック（`RunEvent::Reopen`）は `handle_reopen`
//! が扱う（macOS のみ。ウィンドウのレジストリ 6.1 が入るまでの seam）。
//!
//! 本ファイルがまだ持たないもの（各タスクがここへ書き込む）:
//!
//! - タスク 7.4 / 7.5: メニューの「終了」項目。[`request_exit`] を呼ぶこと。
//! - タスク 5.5: 異常終了の記録（要件 8.2）。
//! - タスク 8.3: 描画の代替経路の判定と適用（要件 10.3）。
//!   [`reserve_render_fallback_point`] の中身を埋める。
//! - タスク 6.1 / 9.6: 引き継いだ起動要求と、ウィンドウおよびドキュメントの対応付け。
//!   [`present_window_for_request`] が seam である。

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use app_shell::diagnostics::{self, DiagnosticsLevel};
use app_shell::settings::{self, FileSettingsStore, RecoveredFrom, SettingsStore};
use app_shell::sidecar::{SidecarSupervisor, Supervisor};
use tauri::utils::config::{Csp, CspDirectiveSources};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_log::log::{self, LevelFilter};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

/// 起動を継続できない前提の名前: アプリケーションデータ領域（設定の保存先）。
const PREREQUISITE_APP_DATA: &str = "アプリケーションデータ領域";

/// 起動を継続できない前提の名前: 診断情報（記録）の保存先。
const PREREQUISITE_DIAGNOSTICS: &str = "診断情報の保存先";

/// 起動を継続できない前提の名前: Tauri ランタイムとウィンドウの構築。
const PREREQUISITE_RUNTIME: &str = "Tauri ランタイム";

/// 起動を継続できない前提の名前: 外部ネットワーク経路の遮断（通信内容保護方針）。
///
/// 欠けた宛先を検査する側 (`confirm_csp_values`) が使う。**静かに壊れる種類の失敗**なので、
/// 満たされなかった前提として名指しして起動を中止する（要件 1.4、1.6、8.3）。
const PREREQUISITE_CSP: &str = "通信内容保護方針 (CSP)";

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

    // 手順 2.5: 通信内容保護方針の実効値を取り出し、外部への経路が塞がれていることを
    //   構築の前に確認する（要件 1.6、8.3。タスク 5.3）。値は `generate_context!` が
    //   `tauri.conf.json` から読み込んだ実効の `Config` から取る（定数の読み直しでも
    //   ファイルの再読込でもない）。`connect-src` に IPC の宛先が欠けると、通信境界の呼び出しが
    //   警告 1 行だけを残して低速な文字列経路へ恒久的に降格する（tauri#12835）ため、
    //   構築の前に落とす。Tauri は同じ「前提不成立」の経路を利用者に提示する（要件 1.4）。
    let context = tauri::generate_context!();
    let csp = csp_config(context.config());
    confirm_csp_values(&csp)?;

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
    // 明示的な終了の掛け金（5.4）。終了要求のコールバックがここから読む。
    let builder = builder.manage(ExitControl::default());
    let builder = builder.manage(startup);

    // 手順 4: 構築。ここで GTK / WebKit のランタイムと、登録順に各プラグインが初期化される。
    //   **記録機構のロガーもここで取り付けられる**（`tauri-plugin-log` の `setup`）。そのため
    //   これより前の `log::…!` はどこにも残らない。
    //   **単一インスタンスの 2 つ目のプロセスはこの中で引数を引き渡して自ら終了する**
    //   （この関数は 2 つ目のプロセスでは戻らない）。
    let app = builder
        .build(context)
        .map_err(|error| StartupError::new(PREREQUISITE_RUNTIME, error.to_string()))?;

    // 手順 4.5: 記録機構の実効設定を起動時に確認する（要件 8.1、8.5）。起動行を記録し、
    //   記録中のファイルが方針の保存先に現れたことを確かめる。書けなければ診断の保存先の
    //   前提不成立として報告する（無言で劣化させない）。
    confirm_effective_logging(&logging)?;

    // 手順 4.6: 実効の `connect-src` を起動行として記録し、検査を再度通す（要件 1.6、8.3。
    //   タスク 5.3）。ロガーは手順 4 で取り付けられたため、この行は方針の保存先へ残る。
    confirm_effective_csp(&csp)?;

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

    // 手順 6: 実行。終了条件（最後のウィンドウ・常駐慣習）は [`handle_run_event`] が扱う
    //   （タスク 5.4、要件 2.8・2.9）。既定では最後のウィンドウが閉じた時点でランタイムが
    //   `ExitRequested { code: None }` を発し、それを拒否しなければプロセスは終了する。
    //   常駐の慣習を持つプラットフォーム（macOS）でだけ、明示的な終了が要求されていない
    //   限りこれを拒否する（無条件には拒否しない）。
    app.run(handle_run_event);

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

// ---------------------------------------------------------------------------
// 外部ネットワーク経路の遮断（要件 1.6, 8.3。タスク 5.3）
// ---------------------------------------------------------------------------

/// 通信内容保護方針の `connect-src` が許可しなければならない唯一の宛先（通信境界）。
///
/// `ipc:` は Linux / macOS の IPC カスタムプロトコル、`http://ipc.localhost` は Windows の
/// IPC エンドポイントである（research.md「IPC の転送形式と大きなペイロード」）。3 OS が同一の
/// `tauri.conf.json` を共有するため、両方を常に許可する。
///
/// **欠けると静かに壊れる。** 方針を設定した状態で `connect-src` にこの宛先が無いと、Tauri の
/// `fetch` による IPC が拒否され、`console.warn` を 1 行残しただけで `postMessage` の文字列
/// 経路へ**恒久的に降格する**（tauri#12835）。呼び出し自体は成功し続けるため気づきにくく、
/// 大きなペイロード（要件 4.5）だけが目に見えず遅くなる。したがって起動時に検出して中止する。
const REQUIRED_CONNECT_SRC: [&str; 2] = ["ipc:", "http://ipc.localhost"];

/// 実効の通信内容保護方針のうち、起動時の確認（タスク 5.3）が対象にする部分。
///
/// **「実効値」であることの根拠**: 値は [`tauri::generate_context!`] が `tauri.conf.json`
/// （およびプラットフォーム別の `tauri.<os>.conf.json`）から読み込んでコンパイル時に埋め込んだ
/// [`tauri::Config`] から取り出す。実行時に `tauri.conf.json` を読み直すのでも、Rust の定数を
/// そのまま読むのでもなく、**Tauri が各ウィンドウの応答ヘッダに載せる値そのもの**
/// （`AppManager::csp` → `set_csp`）を検査する。Tauri は配信時に `script-src` / `style-src` へ
/// nonce を足すが `connect-src` には触れないため、`connect-src` は設定値と配信値が一致する。
///
/// **`tauri.conf.json` にコメントを書けない**ため、方針を構成する各指示子の理由はこの Rust 側の
/// doc に記す（下の [`csp_config`]）。
struct CspConfig {
    /// 実効の方針を 1 行にしたもの（起動行に出す）。
    policy: String,
    /// 実効の `connect-src` が許可する宛先（順序は保存しない）。
    connect_src: Vec<String>,
}

/// `tauri.conf.json` の実効設定から通信内容保護方針を取り出す（タスク 5.3）。
///
/// 設定する方針（`app.security.csp`）と、各指示子を置く理由は次のとおりである。**方針は
/// 「外部への経路を与えない」（要件 1.6）ことと「記録を外部へ送信しない」（要件 8.3）ことを
/// 実行時に強制する唯一の機構であり、指示子が 1 つ欠けるとその経路が開く。**
///
/// - `default-src 'self'` — 明示しない全種別（画像・フォント・メディア・フレーム・
///   `EventSource` など）の取得元を自前の資産だけに閉じる。**これが無いと、画像やフレームの
///   読み込みという形で任意の外部オリジンへの経路が残る。**
/// - `connect-src ipc: http://ipc.localhost` — `fetch` / `XMLHttpRequest` / `WebSocket` の
///   接続先を通信境界（IPC）だけに限定し、`default-src` の `'self'` を上書きする。
///   **これが欠けると IPC の `fetch` が CSP に拒否され、警告 1 行だけで低速な文字列経路へ
///   恒久的に降格する**（[`REQUIRED_CONNECT_SRC`]）。同時に、外部オリジンへの接続は
///   `ipc:` / `http://ipc.localhost` 以外すべて拒否される。
/// - `script-src 'self'` — 実行できるコードを自前の資産だけにする。リモートスクリプトの
///   読み込みは取得と実行の両方の経路になるため、ここを閉じる。Tauri は配信時に自前の
///   初期化スクリプトへ nonce を足すが、それは `default-src` の `'self'` を緩めない
///   （`script-src` を明示しても Tauri の初期化スクリプトは動作する）。
/// - `style-src 'self'` — 自前の資産のスタイルだけを許す。**初期画面はこれで描画されるため
///   `'unsafe-inline'` は不要である**（当初は必要と判断して付けていたが誤りだった。実測で
///   画素が一致した）。(1) `src/shell/Layout.tsx` の `style={{ … }}` は React が CSSOM
///   （`CSSStyleDeclaration`）経由で適用するため CSP の `style-src` の対象外であり、
///   (2) `src/index.html` のインライン `<style>` はビルド時に Tauri の `__TAURI_STYLE_NONCE__`
///   トークンが埋め込まれ、配信時に実 nonce へ置換されて `style-src` に載るためである
///   （`tauri-codegen` の `inject_nonce_token` → `AppManager::set_csp`）。したがって
///   `'unsafe-inline'` を外しても描画は変わらず、方針だけが厳しくなる。
/// - `object-src 'none'` — プラグイン・埋め込みオブジェクトの読み込みを全面禁止する。
///   `default-src 'self'` では自前オリジンからの埋め込みが残るため、経路を完全に塞ぐ。
/// - `base-uri 'none'` — `<base href>` による相対 URL の基準の付け替えを禁止する。
///   これが無いと、相対 URL が外部オリジンへ向け直される経路が残る。
/// - `form-action 'none'` — フォーム送信を全面禁止する。**`form-action` は `default-src` の
///   影響を受けない**（フォールバックが無い）ため、明示しなければフォーム送信という外部への
///   経路が残る。
///
/// 指示子を増やす場合は、それが外部への経路を開かないことを確かめ、[`REQUIRED_CONNECT_SRC`]
/// を緩めるなら [`confirm_csp_values`] の検査と本 doc を同時に更新すること。
fn csp_config(config: &tauri::Config) -> CspConfig {
    // 注意: dev ビルドでは Tauri が `dev_csp.or(csp)` を配信する（`AppManager::csp` は
    // `is_dev()` のとき `dev_csp` を優先する）。`app.security.devCsp` は現在未設定なので
    // `csp` がそのまま実効値であり、この取り出しは正確である。**将来 `devCsp` を設定するなら、
    // この関数は dev 側の値も検査するよう更新すること**（さもないと起動時の確認だけが
    // 実際に配信される方針を見落とす）。
    csp_config_from(config.app.security.csp.as_ref())
}

/// [`csp_config`] の純粋な部分。`Csp` から実効の `connect-src` を取り出す。
///
/// 方針の解釈は [`Csp`] の `From<Csp> for HashMap`（`;` と空白で分割する。tauri-utils の
/// 実装）に委ねる。**自前の文字列分割を書かない** — 配信時の解釈と食い違う余地を作らないため。
fn csp_config_from(csp: Option<&Csp>) -> CspConfig {
    let policy = csp.map(Csp::to_string).unwrap_or_default();
    let connect_src = csp
        .map(|csp| {
            let directives: HashMap<String, CspDirectiveSources> = csp.clone().into();
            directives
                .get("connect-src")
                .cloned()
                .map(Vec::<String>::from)
                .unwrap_or_default()
        })
        .unwrap_or_default();
    CspConfig { policy, connect_src }
}

/// 構築の前に、実効の方針が IPC の宛先だけを許可していることを確かめる（要件 1.6、8.3）。
///
/// **静かに壊れることの防止が目的である。** `connect-src` が欠ける、片方の宛先が抜ける、
/// 余分な宛先（外部オリジン・ワイルドカード）が混ざる、のいずれでも起動を中止する。
/// **「IPC の宛先が含まれる」だけの部分一致にしない** — 外部オリジンを併記した方針も
/// 要件 1.6 を破るため、集合の一致で判定する。
///
/// # Errors
///
/// 実効の `connect-src` が [`REQUIRED_CONNECT_SRC`] と一致しないとき [`StartupError`]
/// （前提「通信内容保護方針 (CSP)」）。呼び出し元（`main`）が 5.1 の前提不成立経路
/// （stderr + `jxcel-startup-error.log` + 非 0 終了）で提示する。
fn confirm_csp_values(config: &CspConfig) -> Result<(), StartupError> {
    if sorted(&config.connect_src) != sorted(&REQUIRED_CONNECT_SRC) {
        return Err(StartupError::new(
            PREREQUISITE_CSP,
            format!(
                "connect-src が通信境界の宛先だけを許可していない（実効値: {} / 必須: {}）。\
                 connect-src に IPC の宛先が欠けると、通信境界の呼び出しが警告 1 行だけを残して\
                 低速な文字列経路へ恒久的に降格する（tauri#12835）。tauri.conf.json の \
                 app.security.csp を直すこと。",
                show_sources(&config.connect_src),
                show_sources(&REQUIRED_CONNECT_SRC),
            ),
        ));
    }
    Ok(())
}

/// 構築の後に、実効の `connect-src` を起動行として記録し、**同じ検査を再度通す**
/// （要件 1.6、8.3。タスク 5.3 の完了状態）。
///
/// 構築の後に置くのは、ロガーが手順 4 で初めて取り付けられるためである（これより前の
/// `log::…!` はどこにも残らない）。保守担当はこの行で「どの宛先が実効で許可されているか」を
/// 起動記録から確認できる。
///
/// # Errors
///
/// [`confirm_csp_values`] と同じ。構築を挟んだ後でも値が変わっていないことを確かめる。
fn confirm_effective_csp(config: &CspConfig) -> Result<(), StartupError> {
    log::info!(
        "通信内容保護方針の実効 connect-src: {} / 方針全体: {}",
        show_sources(&config.connect_src),
        if config.policy.is_empty() { "(未設定)" } else { &config.policy },
    );
    confirm_csp_values(config)
}

/// 宛先の集合を比較・表示できる形に正規化する（複製してソートする）。
fn sorted<S: AsRef<str>>(sources: &[S]) -> Vec<String> {
    let mut normalized: Vec<String> = sources.iter().map(|s| s.as_ref().to_owned()).collect();
    normalized.sort();
    normalized
}

/// 宛先の並びを起動行・エラー文向けに 1 つの文字列にする。空なら「(なし)」と出す。
fn show_sources<S: AsRef<str>>(sources: &[S]) -> String {
    if sources.is_empty() {
        return "(なし)".to_owned();
    }
    sources.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(" ")
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
    present_existing_or_create(app);
}

/// 既にあるウィンドウを前面に出し、1 枚も無ければ 1 枚作る。
///
/// 引き継ぎ（要件 1.5）と Dock アイコンのクリック（要件 2.9、macOS）の両方が使う。**これは
/// seam である** — どのウィンドウをどう提示するかは 6.1 の `WindowManager` が所有し、
/// 6.1 がレジストリを導入したらこの関数はそちらの提示経路への呼び出しに置き換わる。
fn present_existing_or_create(app: &AppHandle) {
    match app.webview_windows().values().next().cloned() {
        Some(window) => focus_window(&window),
        // 起動直後や常駐中に 1 枚も無い場合の経路。通常の起動では `tauri.conf.json` の
        // 宣言が先に開いている。
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

// ---------------------------------------------------------------------------
// 最後のウィンドウと常駐慣習の扱い（要件 2.8, 2.9。タスク 5.4）
// ---------------------------------------------------------------------------

/// 検証専用の終了の引き金が読む環境変数の名前。
///
/// **通常の利用環境に存在しないことを狙った名前である。**値は整数のミリ秒で、設定されて
/// いるときだけ [`arm_verification_exit_trigger`] がその時間後に [`request_exit`] を呼ぶ。
const VERIFY_EXIT_ENV: &str = "JXCEL_VERIFICATION_EXIT_AFTER_MS";

/// プラットフォームが最後のウィンドウを閉じてもアプリを常駐させる慣習を持つか。
///
/// **3 OS 共通の既定は「最後のウィンドウが閉じたら終了する」であり、macOS の常駐慣習には
/// 既定では従わない**（該当分岐は `cfg` で切られていない。research.md「最後のウィンドウを
/// 閉じたときの挙動」）。したがって要件 2.9 は Tauri の既定任せでは成立せず、macOS のとき
/// だけ [`vetoes_exit`] が終了要求を拒否する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Residency {
    /// 最後のウィンドウを閉じてもプロセスを常駐させる慣習を持つ（macOS）。
    StayResident,
    /// 最後のウィンドウを閉じたらプロセスを終了する（Windows / Linux）。
    ExitOnLastWindowClosed,
}

impl Residency {
    /// 実行中のプラットフォームの慣習。`cfg!` はコンパイル時に評価されるため実行時の分岐は
    /// 残らない（macOS 以外は常に [`Residency::ExitOnLastWindowClosed`]）。
    const CURRENT: Self = if cfg!(target_os = "macos") {
        Self::StayResident
    } else {
        Self::ExitOnLastWindowClosed
    };
}

/// 終了要求を拒否して常駐すべきか。**純粋関数**であり、方針をテストで固定する
/// （`tests::the_residency_policy_follows_platform_conventions`）。
///
/// 拒否するのは次の 3 条件がすべて成り立つときだけである:
///
/// 1. プラットフォームが常駐の慣習を持つ（macOS）
/// 2. 終了コードが指定されていない（`code: None` = 最後のウィンドウが閉じられた経路）
/// 3. 明示的な終了がまだ要求されていない
///
/// **`code: Some(_)` を拒否してはならない。** [`request_exit`] は `app.exit(0)` を使うが、
/// `AppHandle::exit` は `RunEvent::ExitRequested { code: Some(0) }` を発生させる
/// （tauri 2.11.5 の `App::exit` → `RuntimeHandle::request_exit`）。ここで `Some` を拒否すると
/// **明示的な終了操作そのものが効かなくなる**。
///
/// **無条件の拒否をしてはならない理由（tauri#13511）。**「最後のウィンドウが閉じた」と
/// 「利用者が終了を選んだ」を区別する手段は `code` が `None` か `Some` かという推定のほかに
/// 無く、この問題は 2025-05 から未解決である。推定が将来崩れてもプロセスが通常手段で終了
/// できなくならないよう、明示的な終了は [`ExitControl`] の掛け金で拒否をすべて解除し、
/// [`request_exit`] から常に [`AppHandle::exit`] を呼べるようにする（条件 3）。
fn vetoes_exit(residency: Residency, code: Option<i32>, explicit_quit: bool) -> bool {
    matches!(residency, Residency::StayResident) && code.is_none() && !explicit_quit
}

/// 明示的な終了が要求されたことを保持する一方通行の掛け金。
///
/// [`request_exit`] だけが [`ExitControl::authorize_quit`] を呼ぶ。**いったん立てば戻らない**
/// — 終了は不可逆な操作であり、「常駐に戻る」経路は存在しないためである。
#[derive(Default)]
struct ExitControl {
    quit_requested: AtomicBool,
}

impl ExitControl {
    /// 明示的な終了を認可する。以後 [`vetoes_exit`] は常に `false` を返す。
    fn authorize_quit(&self) {
        self.quit_requested.store(true, Ordering::SeqCst);
    }

    /// 明示的な終了が要求済みか。
    fn quit_requested(&self) -> bool {
        self.quit_requested.load(Ordering::SeqCst)
    }
}

/// **明示的な終了操作の唯一の入口。**メニューの「終了」、終了コマンド、Dock の「終了」は
/// すべてこれを呼ぶこと（7.4 / 7.5 への申し送り）。行うことは 2 つである:
///
/// 1. [`ExitControl`] の掛け金を立てる。以後 [`vetoes_exit`] は macOS でも `false` を返し、
///    `code: None` の終了要求（最後のウィンドウが閉じられた経路）でさえ拒否しない。
/// 2. `app.exit(0)` を呼ぶ。これは `RunEvent::ExitRequested { code: Some(0) }` を発生させるが、
///    条件 1・2 により [`vetoes_exit`] は `false` を返し、拒否されない。実行時は
///    `ControlFlow::ExitWithCode(0)` となり、プロセスは終了コード 0 で終わる。
///
/// **この経路がプロセスを終了できなくなることはない。** `app.exit(0)` は
/// `RuntimeHandle::request_exit(0)` を呼んで `run` の制御フローを `Exit` にする
/// （`tauri-runtime-wry` 2.11.4 の `Message::RequestExit`）。終了を止められるのは
/// `RunEvent::ExitRequested` のコールバックが `ExitRequestApi::prevent_exit` を呼んだときだけ
/// であり、掛け金が立った後は [`handle_run_event`] がそれを呼ばない。掛け金は `SeqCst` の
/// 一方通行なので、立てる側と読む側のどちらが先でも「拒否しない」側に倒れる。
///
/// ウィンドウが 1 枚も無くても有効である（常駐中に呼ばれるのが通常の経路である）。
pub fn request_exit(app: &AppHandle) {
    app.state::<ExitControl>().authorize_quit();
    log::info!("明示的な終了操作を受け付けた。常駐の拒否を解除して終了する");
    app.exit(0);
}

/// `app.run` のコールバック。終了要求と Dock クリックを扱う（要件 2.8、2.9。タスク 5.4）。
///
/// それ以外のイベント（`WindowEvent::CloseRequested` を含む）は処理しない。ウィンドウを
/// 閉じてよいかの仲介は 7.6 が所有し、ここは「閉じられた後の帰結」だけを決める。
/// 起動完了時の [`RunEvent::Ready`] では検証専用の終了の引き金（[`arm_verification_exit_trigger`]）
/// だけを用意する（環境変数が無ければ何もしない）。
fn handle_run_event(app: &AppHandle, event: RunEvent) {
    match event {
        RunEvent::ExitRequested { code, api, .. } => {
            let explicit_quit = app.state::<ExitControl>().quit_requested();
            if vetoes_exit(Residency::CURRENT, code, explicit_quit) {
                log::info!(
                    "最後のウィンドウが閉じられたが常駐の慣習に従って終了しない（macOS）。\
                     終了するには明示的な終了操作（request_exit）を使う"
                );
                api.prevent_exit();
            }
        }
        // 起動の完了時に検証専用の引き金を用意する。環境変数が無ければ何もしない。
        RunEvent::Ready => arm_verification_exit_trigger(app),
        #[cfg(target_os = "macos")]
        RunEvent::Reopen {
            has_visible_windows, ..
        } => handle_reopen(app, has_visible_windows),
        _ => {}
    }
}

/// Dock アイコンのクリック（`RunEvent::Reopen`）に対応する（要件 2.9。**macOS のみ**）。
///
/// 常駐中は最後のウィンドウが閉じられた後もプロセスが生きているため、利用者が Dock の
/// アイコンをクリックしてウィンドウを求める経路が要る。見えているウィンドウが 1 枚も無ければ
/// 1 枚提示し直し、あれば何もしない（前面化は OS が行う）。
///
/// **ウィンドウのレジストリ（6.1）がまだ無い。**そのため「既存のウィンドウを前面に出す／
/// 無ければ 1 枚作る」だけを行い、どのウィンドウをどう提示するかは 6.1 の `WindowManager` に
/// 委ねる（[`present_existing_or_create`] が seam である）。
#[cfg(target_os = "macos")]
fn handle_reopen(app: &AppHandle, has_visible_windows: bool) {
    if has_visible_windows {
        return;
    }
    log::info!("Dock のクリックを受け付け、ウィンドウを提示し直す");
    present_existing_or_create(app);
}

/// 明示的な終了（[`request_exit`]）が確実にプロセスを終わらせることを実測するための、
/// **検証専用**の引き金。
///
/// メニュー項目（7.4 / 7.5）が作られる前は、[`request_exit`] を人手で呼ぶ経路が無い。環境変数
/// [`VERIFY_EXIT_ENV`] にミリ秒が設定されているときだけ、その時間だけ待ってから
/// [`request_exit`] を呼ぶ。**環境変数が無い通常の起動では関数の先頭で即座に戻るので何もしない**
/// （数値として解釈できない値のときも何もしない）。したがって配布物の既定の振る舞いを変えない。
///
/// 7.4 / 7.5 がメニュー項目を結線したら、この引き金は不要になる。残す場合もメニューの経路を
/// 置き換えてはならない（引き金は環境変数が設定された検証のときだけ働く）。
fn arm_verification_exit_trigger(app: &AppHandle) {
    let Ok(value) = std::env::var(VERIFY_EXIT_ENV) else {
        return;
    };
    let Ok(delay_ms) = value.parse::<u64>() else {
        log::warn!("{VERIFY_EXIT_ENV} を数値として解釈できないので無視する: {value:?}");
        return;
    };
    let app = app.clone();
    log::info!("検証専用の終了の引き金が有効である: {delay_ms} ms 後に明示的な終了を行う");
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        request_exit(&app);
    });
}

// ---------------------------------------------------------------------------
// テスト（タスク 5.3 / 5.4）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        confirm_csp_values, csp_config_from, vetoes_exit, CspConfig, ExitControl, Residency,
    };
    use tauri::utils::config::Csp;

    /// 方針文字列から実効設定を組み立てる（本番の `csp_config` が `Config` から取り出すのと
    /// 同じ経路を通す。方針の解釈をテスト側で二重実装しないための入口である）。
    fn policy(text: &str) -> CspConfig {
        csp_config_from(Some(&Csp::Policy(text.to_owned())))
    }

    #[test]
    fn connect_src_with_exactly_the_ipc_destinations_is_accepted() {
        let config = policy(
            "default-src 'self'; connect-src ipc: http://ipc.localhost; script-src 'self'",
        );
        assert_eq!(config.connect_src.len(), 2);
        assert!(confirm_csp_values(&config).is_ok());
    }

    #[test]
    fn a_missing_ipc_destination_is_rejected_and_named() {
        let error = confirm_csp_values(&policy("default-src 'self'; connect-src ipc:"))
            .expect_err("欠けた宛先は拒否しなければならない");
        assert!(
            error.to_string().contains("http://ipc.localhost"),
            "どの宛先が欠けているかを名指しする: {error}"
        );
    }

    #[test]
    fn an_extra_connect_destination_is_rejected() {
        assert!(
            confirm_csp_values(&policy(
                "connect-src ipc: http://ipc.localhost https://example.com"
            ))
            .is_err(),
            "IPC 以外の宛先を許可してはならない"
        );
    }

    #[test]
    fn a_wildcard_connect_destination_is_rejected() {
        assert!(
            confirm_csp_values(&policy("connect-src ipc: http://ipc.localhost *")).is_err(),
            "ワイルドカードを許可してはならない"
        );
    }

    #[test]
    fn an_absent_connect_src_is_rejected() {
        assert!(
            confirm_csp_values(&policy("default-src 'self'")).is_err(),
            "connect-src の記述漏れを検出しなければならない"
        );
    }

    // -----------------------------------------------------------------------
    // 常駐の方針（タスク 5.4、要件 2.8・2.9）
    // -----------------------------------------------------------------------

    #[test]
    fn the_residency_policy_follows_platform_conventions() {
        // Windows / Linux は最後のウィンドウが閉じたら終了する（要件 2.8）。**ここで拒否を
        // 返したらプロセスが常駐してしまい、既定が壊れる。**
        assert!(!vetoes_exit(Residency::ExitOnLastWindowClosed, None, false));
        assert!(!vetoes_exit(
            Residency::ExitOnLastWindowClosed,
            Some(0),
            false
        ));

        // macOS は `code: None`（最後のウィンドウが閉じられた経路）だけ常駐する（要件 2.9）。
        assert!(vetoes_exit(Residency::StayResident, None, false));
    }

    #[test]
    fn a_coded_exit_request_is_never_vetoed() {
        // `AppHandle::exit` は `code: Some(_)` を伴う終了要求を出す。これを拒否すると明示的な
        // 終了操作が効かなくなる（`request_exit` がプロセスを終わらせられなくなる）。
        assert!(!vetoes_exit(Residency::StayResident, Some(0), false));
        assert!(!vetoes_exit(Residency::StayResident, Some(1), false));
    }

    #[test]
    fn an_explicit_quit_releases_every_veto() {
        let control = ExitControl::default();
        assert!(!control.quit_requested(), "初期状態では掛け金は立っていない");
        control.authorize_quit();
        assert!(control.quit_requested());
        // 掛け金が立った後は、macOS の `code: None` でさえ拒否しない。**これが「通常手段で
        // 終了できなくならない」ことの構造的な担保である**（tauri#13511。
        // `request_exit` は掛け金を立ててから `app.exit(0)` を呼ぶ）。
        assert!(!vetoes_exit(Residency::StayResident, None, true));
    }
}

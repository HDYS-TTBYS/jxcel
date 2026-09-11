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
//! タスク 5.5 が加えたのは**異常終了の記録**である（要件 8.2）。パニックフックを診断の初期化
//! （手順 3）で設置し、パニックした事実・メッセージ・位置・スレッドを方針の保存先の
//! [`CRASH_RECORD_FILE_NAME`] へ**同期して置換書き込み**する（一時ファイル → `sync_all` →
//! `rename`。4.5 と同じ [`settings::atomic`]）。記録機構の非同期な書き込みやそのファイル
//! ハンドルに依存しないため、プロセスが直後に死んでも記録は完全な形で残る。ドキュメントの
//! 内容は記録しない（要件 8.4。扱うのはパニックのペイロードと発生位置だけである）。設置は
//! [`install_crash_recorder`]、記録の組み立ては [`CrashRecord`]、書式は [`format_crash_record`]
//! が担う。**異常終了の引き金は 5.4 の検証専用の環境変数に統合した**（[`VERIFY_EXIT_ENV`] の
//! 値が `panic:<ミリ秒>` のとき意図的にパニックする。片付けは 1 箇所のままである）。
//!
//! タスク 5.6 が加えたのは**アプリ終了時の補助プロセスの終了**である（要件 5.6）。監督
//! （[`Supervisor`]）をアプリの管理状態として 1 実体だけ所有し（[`sidecar_supervisor`]）、
//! 通常終了でプロセスが終わる直前に必ず届く唯一のイベント [`RunEvent::Exit`] で
//! `shutdown_all` を**同期で**呼ぶ（[`shutdown_sidecars`]）。**基盤側の終了時清掃には依存
//! しない** — この子は Rust が `std::process::Command` で起動しており、`tauri-plugin-shell` は
//! 依存にすら入っていない（同プラグインの終了時清掃は JS→IPC 経路で起動した子だけを対象と
//! する）。強制終了やパニックではイベントループを経由せず `RunEvent::Exit` が届かないため、
//! それらの経路は次の機構が覆う: **Unix は補助プロセス自身の親監視**（1.6 / 3.5 が
//! `--parent-pid` を注入し、親が消えたら子が自己終了する。`killpg` は `shutdown_all` の内側で
//! しか走らないため、SIGKILL の後の Unix には届かない）、**Windows は Job Object の
//! `KILL_ON_JOB_CLOSE`**（3.3。カーネルが強制するため親の異常終了後も有効な唯一の機構）。
//! 起動時の残留掃除（[`sweep_orphans_at_startup`]）が最後の backstop である。終了時に待つ猶予は
//! プラットフォームで分ける（[`SIDECAR_SHUTDOWN_GRACE`]）。
//!
//! タスク 6.1 が加えたのは**ウィンドウの生成とレジストリ**である（要件 2.1〜2.3、2.5、2.10）。
//! 生成は [`window::open`] の 1 経路に一本化し、**非同期でのみ**行う — 同期のコマンドや
//! イベントハンドラの中で `WebviewWindowBuilder::build()` を呼ぶと Windows でデッドロックする
//! （design.md「WindowManager」）。このファイルの 3 つの同期文脈、すなわち起動時の 1 枚
//! （[`open_startup_window`]。`RunEvent::Ready`）、引き継いだ起動要求
//! （[`present_window_for_request`]。単一インスタンスのコールバック）、Dock のクリック
//! （`handle_reopen`。`RunEvent::Reopen`）は、いずれも [`window::open`] を呼ぶだけで
//! ブロックしない。ウィンドウの識別子から状態（関連付けられたドキュメントとラベル）への写像は
//! [`window::WindowRegistry`] が保持する。登録は**生成の前**に行い（生成中に閉じられても登録が
//! 漏れないため）、生成の完了で段階が進み、破棄の通知（[`window::on_window_event`]）で
//! 取り除かれる。**生成中の登録は残骸ではない**ため、ドキュメント要求の無い引き継ぎは生成中の
//! ウィンドウがあれば二重に開かず、その生成が完了時に自ら前面に出る。ラベル規約は
//! `doc-<連番>` / `empty-<連番>` であり、5.1 が暫定で使っていた `handover-<連番>` はここで
//! 規約へ収束した。
//!
//! 本ファイルがまだ持たないもの（各タスクがここへ書き込む）:
//!
//! - タスク 7.4 / 7.5: メニューの「終了」項目。[`request_exit`] を呼ぶこと。
//! - タスク 8.3: 描画の代替経路の判定と適用（要件 10.3）。
//!   [`reserve_render_fallback_point`] の中身を埋める。
//! - タスク 9.6: ドキュメントを関連付けていないウィンドウの操作導線（新規作成・既存ファイルを
//!   開く）。ドキュメントの関連付けそのものは 6.1 が [`window::open`] で実装済みである。
//! - タスク 8.1: 補助プロセスの実行ファイルの絶対パスの解決（プラットフォーム別）と、出力の
//!   診断連携。[`sidecar_supervisor`] が監督の唯一の生成点であり、8.1 はそこへ解決済みの
//!   期待パスを流し込み、`AppHandle::state` から同じ実体を取って `ensure` する。

use std::backtrace::{Backtrace, BacktraceStatus};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use app_shell::diagnostics::{self, DiagnosticsLevel};
use app_shell::settings::{self, FileSettingsStore, RecoveredFrom, SettingsStore};
use app_shell::sidecar::integrity::BUILD_TARGET_TRIPLE;
use app_shell::sidecar::{SidecarKind, SidecarSpec, SidecarSupervisor, Supervisor};
use tauri::utils::config::{Csp, CspDirectiveSources};
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_log::log::{self, LevelFilter};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

use crate::commands;
use crate::ports::DocumentHostPort;
use crate::window::{self, WindowRegistry, WindowRequest};

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

/// 異常終了の記録を残すファイルの名前。方針の保存先（[`diagnostics::log_dir`]）の直下に置く。
///
/// **記録機構の記録中のファイル（`{`[`LOG_FILE_STEM`]`}.log`）とは別のファイルにする。**理由は
/// [`record_abnormal_termination`] の doc にある。名前は記録機構のローテーションの対象外で
/// ありながら（`RotatingFile::remove_old_files` が消すのは `{語幹}_{日時}.log` だけである）、
/// 4.4 の書き出し（`diagnostics::export`）の対象（拡張子 `log`）には含まれる。
const CRASH_RECORD_FILE_NAME: &str = "jxcel-crash.log";

/// 異常終了の記録 1 件の上限（バイト）。
///
/// 記録は**追記ではなく置換**なので（[`record_abnormal_termination`]）、このファイルは常に
/// この大きさ以下である。値は方針の余裕（`MAX_TOTAL_LOG_BYTES - MAX_RETAINED_LOG_BYTES` = 2 MB）
/// をほとんど食わない大きさに選ぶ（下のコンパイル時検査）。1 件のパニックのメッセージと
/// スタックトレースには 64 KiB で十分であり、これを超える分は文字境界で切り詰める。
const MAX_CRASH_RECORD_BYTES: usize = 64 * 1024;

/// 異常終了の記録ファイルを足しても保持合計が方針の合計上限を超えないことをコンパイル時に固定する
/// （要件 8.5。記録機構の保持上限 + このファイルの上限 ≦ 合計上限）。
const _: () = assert!(
    diagnostics::MAX_RETAINED_LOG_BYTES + MAX_CRASH_RECORD_BYTES as u64
        <= diagnostics::MAX_TOTAL_LOG_BYTES
);

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

    // 異常終了の記録先（要件 8.2。タスク 5.5）。フックの設置は `init_diagnostics`（手順 3）が
    // 済ませており、ここで組み立てるのは起動行に出すための同じ値である。**組み立ては
    // `crash_record_path` の 1 箇所だけ**なので、フックへ渡した値と起動行の値は食い違わない。
    let crash_record = crash_record_path(startup.log_dir());

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
    // ウィンドウのレジストリ（要件 2.1・2.5。タスク 6.1）。アプリ全体で 1 実体だけ持ち、
    // 生成（[`window::open`]）が登録し、破棄の通知（[`window::on_window_event`]）が取り除く。
    // ウィンドウ単位の状態管理機構は基盤側に無いため、ここで自前の写像を管理状態として置く。
    let builder = builder.manage(WindowRegistry::new());
    // ドキュメント所有者への委譲点（要件 2.1・2.6。タスク 6.2）。**常に許可し、パスを
    //   受け取っても何もしない既定実装**をアプリ全体で 1 実体だけ置く。終了拒否の仲介
    //   （7.6）とネイティブファイル選択（7.7）は `app.state::<DocumentHostPort>()` から
    //   この実体を取り、判定と引き渡しをこのポート経由で行う。**下流スペックは
    //   `DocumentHostPort::install`（自分の `setup` フック）か、この行の
    //   `DocumentHostPort::default()` を `DocumentHostPort::new(自分の実装)` へ置き換える
    //   ことで差し替える**（`ports.rs` のモジュール doc に接続点を記した）。
    let builder = builder.manage(DocumentHostPort::default());
    // 破棄の通知をレジストリへ流す。**全ウィンドウに効く**（`tauri.conf.json` の宣言の有無に
    // よらず、`WebviewWindowBuilder` で作ったウィンドウにもマネージャ経由で結線される）。
    let builder = builder.on_window_event(window::on_window_event);
    // 設定の共有実体（要件 7.3。タスク 7.1）。**`StartupState` が持つ `Arc` と同じ実体**を
    // コマンド面の管理状態としても置く。`Manager::manage<T>()` は型ごとに 1 実体なので、
    // ここで別の `open` を呼んではならない（同じディレクトリのストアが 2 つになる）。
    // `startup` は直後に管理状態へ移すため、その前に `Arc` を複製しておく。
    let settings_store = Arc::clone(startup.settings());
    let builder = builder.manage(startup);
    // 補助プロセスの監督（要件 5.6。タスク 5.6）。アプリ全体で 1 実体だけ所有し、起動時の
    // 残留掃除（手順 5）と終了時の終了（[`shutdown_sidecars`]）が同じ登録簿を見るようにする。
    // 8.1 の `SidecarHost` は `AppHandle::state` からこの実体を取って `ensure` する。
    let builder = builder.manage(sidecar_supervisor());

    // 設定の共有実体をコマンド面の管理状態として置く（要件 7.3。タスク 7.1）。上の
    // `settings_store` と同じ `Arc` である。設定変更の通知（要件 7.4）は構築の後にこの
    // `Arc` から 1 回 `subscribe()` する。
    let builder = builder.manage(Arc::clone(&settings_store));
    // コマンド面の根（要件 4.1、4.4、4.6。タスク 7.1）。登録の一覧は `commands` モジュールの
    // 1 箇所だけにあり、`generate_handler!` へも同じ一覧が渡る。**この根に業務ロジックは
    // 無い** — 各機能は自分のモジュールにコマンド関数を持ち、根は列挙だけを行う（共有の継ぎ目。
    // design.md「CommandSurface」）。
    let builder = builder.invoke_handler(commands::invoke_handler());

    // 手順 4: 構築。ここで GTK / WebKit のランタイムと、登録順に各プラグインが初期化される。
    //   **記録機構のロガーもここで取り付けられる**（`tauri-plugin-log` の `setup`）。そのため
    //   これより前の `log::…!` はどこにも残らない。
    //   **単一インスタンスの 2 つ目のプロセスはこの中で引数を引き渡して自ら終了する**
    //   （この関数は 2 つ目のプロセスでは戻らない）。
    let app = builder
        .build(context)
        .map_err(|error| StartupError::new(PREREQUISITE_RUNTIME, error.to_string()))?;

    // 手順 4.1: 設定変更の通知をフロントエンドへ届ける配線（要件 7.4。タスク 7.1）。
    //   共有実体（要件 7.3）へ 1 回だけ `subscribe()` し、専用スレッドが受信ループを回して
    //   変更のたびに全ウィンドウへ Tauri イベントを emit する。**構築の後に置く** — emit には
    //   `AppHandle` が要り、ウィンドウは `RunEvent::Ready`（手順 6）以降に現れる。購読前の変更は
    //   再生されない（4.3）ため、起動直後に設定を書き換える経路よりも前に購読を張っておく。
    commands::start_settings_notifications(app.handle(), &settings_store);

    // 手順 4.5: 記録機構の実効設定を起動時に確認する（要件 8.1、8.5）。起動行を記録し、
    //   記録中のファイルが方針の保存先に現れたことを確かめる。書けなければ診断の保存先の
    //   前提不成立として報告する（無言で劣化させない）。
    confirm_effective_logging(&logging)?;

    // 手順 4.6: 実効の `connect-src` を起動行として記録し、検査を再度通す（要件 1.6、8.3。
    //   タスク 5.3）。ロガーは手順 4 で取り付けられたため、この行は方針の保存先へ残る。
    confirm_effective_csp(&csp)?;

    // 手順 4.7: 異常終了の記録フックが設置済みであることを起動行に残す（要件 8.2。タスク 5.5）。
    //   フック自体は手順 3 で保存先を用意できた直後に設置する — アプリが利用者のコードを
    //   動かす前であり、かつ記録先が書き込み可能であることを確かめた後である。この行が
    //   ロガー取り付け後の最初の「設置済み」の記録であり、保守担当は記録先を確認できる。
    log::info!(
        "異常終了の記録フックを設置済み（記録先: {} / 1 件の上限: {} B）",
        crash_record.display(),
        MAX_CRASH_RECORD_BYTES,
    );

    // 手順 5: 残留プロセスの掃除（要件 5.6、タスク 3.5）。前回の実行が終了処理を走らせられずに
    //   残した補助プロセスを終了させる。
    //
    //   **位置の意図的な逸脱（タスク 5.1 の文言は掃除を構築の前に置く）**: 構築の前に掃除すると、
    //   引き継ぎ側の 2 つ目のプロセスも掃除を実行してしまい、動作中のアプリが使っている補助
    //   プロセスを終了させる（実測で再現。要件 5.5・5.6 を破り、8.1 が実物の補助プロセスを
    //   持った時点で実害になる）。単一インスタンスの判定は手順 4 のプラグイン初期化で行われるため、
    //   **生存している側だけがこの位置に到達する。**ここは依然として「アプリが使えるように
    //   なる前」である — ウィンドウは手順 6 の `RunEvent::Ready` で初めて作られる。
    let swept = sweep_orphans_at_startup(app.handle());
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
///
/// **アプリの管理状態にある監督（[`sidecar_supervisor`]）を使う。**ここで別の実体を作ると、
/// 8.1 が解決した期待パスが掃除側にだけ効き、終了時の終了（[`shutdown_sidecars`]）とは別の
/// 登録簿を見ることになる。
fn sweep_orphans_at_startup(app: &AppHandle) -> usize {
    app.state::<Supervisor>().sweep_orphans()
}

/// アプリ全体で 1 つの監督を作る。**タスク 8.1 が差し替える seam である。**
///
/// 8.1（`src-tauri/src/sidecar_host.rs`）は補助プロセスの実行ファイルをプラットフォーム別に
/// 解決し、その絶対パスを [`expected_sidecar_executables`] へ流す。Linux は
/// `usr/share/jxcel/sidecar-smoke`、Windows / macOS は実行ファイルの隣（`externalBin`）である
/// （tasks.md 1.7 の配置規約）。解決が入るまでは期待パスの集合が空なので、残留の掃除は実行
/// ファイル名の一致だけで働く（[`Supervisor::with_expected_executables`] の既定）。
///
/// **この 1 実体を起動時の掃除（[`sweep_orphans_at_startup`]）と終了時の終了
/// （[`shutdown_sidecars`]）が共有する。**2 つ作ると、8.1 が解決した期待パスが掃除側にだけ
/// 効いたり、終了時に別の登録簿を見て起動済みの子を取り逃したりする。
///
/// 終了の猶予は [`SIDECAR_SHUTDOWN_GRACE`] を明示的に与える（`shutdown_all` はこの値を
/// 使って猶予段の満了まで `RunEvent::Exit` のコールバックを待たせるため、プラットフォーム差が
/// そのまま終了時の凍結時間になる）。
fn sidecar_supervisor() -> Supervisor {
    Supervisor::new()
        .with_expected_executables(expected_sidecar_executables())
        .with_grace(SIDECAR_SHUTDOWN_GRACE)
}

/// 補助プロセスの終了に与える猶予。**プラットフォームで分ける**（タスク 5.6 が決めた。
/// 3.3 の申し送りが「短縮するなら穏当段が届かない環境では猶予を待たない判断が要る」として
/// 5.6 に委ねたもの）。
///
/// **Unix は 3.3 の既定（`DEFAULT_GRACE` = 3 秒）のまま。**穏やかな段（プロセスグループ宛の
/// `SIGTERM`）が実際に届き、子は通常ミリ秒で終了するため、上限まで待つのは「猶予信号を無視した」
/// 場合だけである（実測でも約 22 ms）。
///
/// **Windows は 300 ms に短縮する。**Windows の穏やかな段は
/// `GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT)` であり、コンソールを持たない GUI プロセス
/// （release は `windows_subsystem = "windows"`）の子には**実質 no-op** である。`terminate` は
/// `signal_graceful` の失敗を無視して猶予の満了まで待つため、既定の 3 秒を与えると
/// **終了のたびに約 3 秒の無反応な凍結**が起きる。しかも [`RunEvent::Exit`] のコールバックは
/// `cleanup_before_exit` より前に走り、ウィンドウを残したままイベントループを止めているので、
/// その凍結は利用者に見える。300 ms は次の妥協である:
///
/// - 失うもの: 穏やかな段に協力したはずの補助プロセスが、猶予の満了を待たずに強制段
///   （`TerminateJobObject`）で終了させられる。**補助プロセスは終了する**（要件 5.6 は満たす）が、
///   書きかけの出力を流す機会を失いうる。
/// - 得るもの: 終了時の凍結が知覚されにくい長さ（おおむね 100〜200 ms 未満が体感の閾値と
///   される）に収まる。コンソールから起動された場合は `CTRL_BREAK_EVENT` が届く余地も残る。
///
/// **この値は Windows ではコンパイル検証しかしていない**（この環境に Windows 実機が無い。
/// 5.1 の申し送りと同じ制約）。実機で計測したら更新すること。
const SIDECAR_SHUTDOWN_GRACE: std::time::Duration = if cfg!(windows) {
    std::time::Duration::from_millis(300)
} else {
    app_shell::sidecar::supervisor::DEFAULT_GRACE
};

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

    // 手順 3 のうちでも早い位置で異常終了のフックを設置する（要件 8.2。タスク 5.5）。設置点の
    // 根拠は 2 つある: (1) アプリが利用者のコード（ウィンドウの読み込み・コマンド・イベント）を
    // 動かす前である — それにはウィンドウを作る `RunEvent::Ready` より前で足り、手順 4 の
    // 構築より前でもある。(2) 記録先のディレクトリが存在し書き込み可能であることを
    // `prepare_log_directory` で確かめた後である（記録の失敗を「場所が無い」で作り込まない）。
    // 設置の後に起きたパニックだけが記録の対象である（それ以前のパニックは設定ストアの
    // 解決など起動の前提そのものの失敗であり、5.1 の前提不成立の経路が扱う）。
    install_crash_recorder(&log_dir);

    Ok(StartupState {
        settings,
        log_dir,
        recovered,
    })
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
            format!(
                "記録機構が {} へ記録を書けなかった: {error}",
                active.display()
            ),
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
// 異常終了の記録（要件 8.2。タスク 5.5）
// ---------------------------------------------------------------------------

/// 異常終了の記録 1 件。**方針の保存先のファイルへ書く唯一の内容であり、ドキュメントの
/// 内容を含まない**（要件 8.4）。
///
/// ここに持つのはパニックが運んできた値（`panic!` に渡されたメッセージ）と、パニック機構が
/// 付ける位置・スレッド・バックトレースだけである。ドキュメントのセル値やスキーマの内容を
/// 記録経路へ渡す唯一の入口は 4.4 の `Redacted` / `recorded_value` であり、この経路はそれを
/// 迂回しない — **そもそもこの構造体へドキュメントの内容を入れる呼び出し元が存在しない**
/// （入力は [`CrashRecord::from_panic`] の `&PanicHookInfo` だけである）。
struct CrashRecord {
    /// 記録の時刻（UNIX epoch 秒）。`SystemTime` が epoch より前を指す環境では 0。
    ///
    /// 人が読む日時への整形には依存を足す必要があるため、数値のまま残す。記録機構の行が
    /// 持つ現地時刻の書式と合わせて読む。
    epoch_seconds: u64,
    /// アプリケーションのバージョン（`Cargo.toml` の `version`）。
    version: &'static str,
    /// パニックしたスレッド（名前と `ThreadId`）。名前の無いスレッドは `(無名)`。
    thread: String,
    /// **このパニックでプロセスが異常終了するか**（要件 8.2 の「異常終了した事実」）。
    ///
    /// メインスレッドのパニックはプロセスを終わらせる（`panic = "unwind"` なら終了コード 101、
    /// `panic = "abort"` なら `SIGABRT`）。**メインスレッド以外のパニックはそのスレッドだけを
    /// 終わらせる**（Tauri の非同期ランタイムの作業スレッドのパニックなど）。どちらでも記録は
    /// 残すが、**異常終了した事実を主張するのは本当に終了するときだけ**である — 記録は次の
    /// パニックで置き換わるため、生き続けたパニックを「異常終了」と書くと嘘が残る。
    process_terminates: bool,
    /// パニックのメッセージ（`panic!` のペイロード）。改行を含みうる。
    message: String,
    /// パニックの発生位置（`file:line:column`）。パニック機構が位置を持たない場合は `(位置不明)`。
    location: String,
    /// スタックトレース。**`RUST_BACKTRACE` が要求したときだけ**入る
    /// （`Backtrace::capture` は要求が無ければ捕捉しない）。既定のフックも同じ条件で
    /// トレースを出すため、記録と stderr の内容が揃う。
    backtrace: Option<String>,
}

impl CrashRecord {
    /// 進行中のパニックから記録を作る。**入力は `&PanicHookInfo` とメインスレッドの
    /// [`ThreadId`] だけである**（[`install_crash_recorder`] が設置時に記録した値）。
    fn from_panic(info: &PanicHookInfo<'_>, main_thread: std::thread::ThreadId) -> Self {
        // `panic!("…")` のペイロードは `&str`、`panic!("{}", …)` は `String` である。文字列
        // 以外のペイロード（`panic_any` に構造体を渡した場合）は表示しない — 表示には
        // `Debug` が要り、そこにドキュメントの内容が混ざりうるためである（要件 8.4 の精神）。
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(文字列でないペイロード)".to_owned());
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "(位置不明)".to_owned());
        let current = std::thread::current();
        let thread = format!(
            "{} ({:?})",
            current.name().unwrap_or("(無名)"),
            current.id()
        );
        // メインスレッドのパニックはプロセスを終わらせる。`panic = "abort"` のビルドでは
        // どのスレッドのパニックでも中断する（`cfg!` はコンパイル時に畳まれる）。
        let process_terminates = current.id() == main_thread || cfg!(panic = "abort");
        let captured = Backtrace::capture();
        let backtrace =
            (captured.status() == BacktraceStatus::Captured).then(|| captured.to_string());
        Self {
            epoch_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or(0),
            version: env!("CARGO_PKG_VERSION"),
            thread,
            process_terminates,
            message,
            location,
            backtrace,
        }
    }
}

/// 記録を 1 件のテキストに整形する。**純粋関数**であり、書式をテストで固定する
/// （`tests::the_crash_record_pins_the_format`）。
///
/// 先頭行が「異常終了」という事実を述べ、続けて時刻・バージョン・スレッド・終了の帰結・
/// メッセージ・位置を 1 行ずつ置く。末尾は常に改行 1 つで終わる（読み手が完全な記録かどうかを
/// 判断できる）。
fn format_crash_record(record: &CrashRecord) -> String {
    let mut text = String::new();
    text.push_str("===== 異常終了（パニック） =====\n");
    text.push_str(&format!("時刻 (UNIX epoch 秒): {}\n", record.epoch_seconds));
    text.push_str(&format!("バージョン: {}\n", record.version));
    text.push_str(&format!("スレッド: {}\n", record.thread));
    text.push_str(&format!(
        "プロセスの終了: {}\n",
        if record.process_terminates {
            "このパニックにより異常終了する"
        } else {
            "このスレッドだけが終了する（メインスレッド以外のパニック）"
        }
    ));
    text.push_str(&format!("メッセージ: {}\n", record.message));
    text.push_str(&format!("位置: {}\n", record.location));
    if let Some(backtrace) = &record.backtrace {
        text.push_str("バックトレース:\n");
        text.push_str(backtrace.trim_end());
        text.push('\n');
    }
    text
}

/// 記録を書き込み先へ書く。**上限を超える分は UTF-8 の文字境界で切り詰め、切り詰めた事実を
/// 最終行に残す**（巨大なメッセージでも記録そのものを失わない）。
///
/// 書き込み先を引数に取るのは、書式と書き込みの両方をファイルなしでテストできるようにする
/// ためである（`tests::an_oversized_crash_record_is_clamped_at_a_character_boundary`）。
/// 書き終えたバイト数を返す。
fn write_crash_record(writer: &mut impl Write, record: &CrashRecord) -> io::Result<usize> {
    let rendered = format_crash_record(record);
    let bytes = if rendered.len() <= MAX_CRASH_RECORD_BYTES {
        rendered.into_bytes()
    } else {
        let marker =
            format!("\n（記録の上限 {MAX_CRASH_RECORD_BYTES} バイトを超えたため切り詰めた）\n");
        let body = clamp_to_char_boundary(&rendered, MAX_CRASH_RECORD_BYTES - marker.len());
        let mut clamped = Vec::with_capacity(MAX_CRASH_RECORD_BYTES);
        clamped.extend_from_slice(body.as_bytes());
        clamped.extend_from_slice(marker.as_bytes());
        clamped
    };
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(bytes.len())
}

/// `text` を `limit` バイト以下に切り詰める。**文字境界を割らない**ので結果は常に有効な UTF-8 である。
fn clamp_to_char_boundary(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// 方針の保存先から異常終了の記録ファイルの絶対パスを組み立てる。**唯一の組み立て箇所**である
/// （フックへ渡す値と起動行に出す値が食い違わないようにするため）。
fn crash_record_path(log_dir: &Path) -> PathBuf {
    log_dir.join(CRASH_RECORD_FILE_NAME)
}

/// 記録中にさらにパニックしたか（再入の防止）。
///
/// **これが無いと、記録の途中でパニックしたときにフックが再入して無限に続く。**パニックの
/// 処理中にフックがパニックすると、ランタイムはフックを呼び直してから実行時エラーで中断する
/// ため、記録経路に再入しないことが前提になる（[`record_once`] / `record_abnormal_termination`）。
static ABNORMAL_TERMINATION_RECORDING: AtomicBool = AtomicBool::new(false);

/// 再入を防いで `body` を**初回だけ**走らせる。既に記録中なら `None` を返して `body` を走らせない。
///
/// **記録経路がパニックで再入してもここで止まるので、無限に続くことはない。**`body` が
/// パニックした場合、フラグは立ったままにする（わざと下ろさない）— 記録経路が壊れている
/// 状況で再入を許すと、まさに無限再帰を作るためである。フラグを下ろすのは `body` が正常に
/// 戻ったときだけであり、プロセスはその直後に異常終了する。
///
/// 単体テスト `tests::the_recording_guard_prevents_reentry` が両方の腕を固定する。
fn record_once<R>(body: impl FnOnce() -> R) -> Option<R> {
    if ABNORMAL_TERMINATION_RECORDING.swap(true, Ordering::SeqCst) {
        return None;
    }
    let result = body();
    ABNORMAL_TERMINATION_RECORDING.store(false, Ordering::SeqCst);
    Some(result)
}

/// 異常終了の記録フックを設置する（要件 8.2）。**起動時に 1 回だけ呼ぶ。**
///
/// 行うことは 2 つである:
///
/// 1. `std::panic::take_hook()` で**それまで設置されていたフックを取り出す**（既定のフックでも、
///    別のライブラリが設置したものでも同じ扱いである）。
/// 2. 記録してから取り出したフックを呼ぶフックを設置する。**パニックを握り潰さない** —
///    フックはパニックの伝播そのものに介入できないため、記録の後に元のフックを呼べば、
///    プロセスは通常どおり（`panic = "unwind"` のビルドでは終了コード 101、`panic = "abort"`
///    のビルドでは `SIGABRT`）異常終了し、stderr への既定の出力もそのまま残る。
///
/// 設置済みのフックが 1 つも無い場合は `take_hook` が既定のフックを返すので、連鎖は常に成立する。
///
/// **メインスレッドで呼ぶこと。**この関数を呼んだスレッドを「メインスレッド」として記録し、
/// 「このパニックでプロセスが異常終了するか」の判断に使う（[`CrashRecord::process_terminates`]）。
/// 起動順序（`run`）から呼ぶ限りこの前提は満たされる。
fn install_crash_recorder(log_dir: &Path) {
    let path = crash_record_path(log_dir);
    let main_thread = std::thread::current().id();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record_abnormal_termination(&path, main_thread, info);
        // 記録の成否にかかわらず必ず元のフックを呼ぶ（記録側の失敗で既定の出力まで失わない）。
        previous(info);
    }));
}

/// 1 件の異常終了を記録する。**フックの中身であり、パニックを伝播させる責任は呼び出し元が
/// 負う**（この関数は記録だけを行い、パニックを握り潰さない）。
///
/// メインスレッドのパニック（および `panic = "abort"` のビルドでの任意のパニック）は
/// プロセスを異常終了させるので、記録はその事実を述べる。メインスレッド以外のパニックは
/// そのスレッドだけを終わらせるため、記録はそう述べる（[`CrashRecord::process_terminates`]）。
///
/// # 記録先と、その書き込みが保証される理由
///
/// **記録機構（`tauri-plugin-log`）を経由せず、方針の保存先の [`CRASH_RECORD_FILE_NAME`] へ
/// 直接書く。**理由は 4 つある:
///
/// 1. **フックが設置される時点（手順 3）にロガーはまだ存在しない。**ロガーが取り付けられるのは
///    手順 4 の構築であり、それより前の `log::…!` はどこにも残らない。起動中のパニック
///    （依存の初期化・プラグインの登録・ウィンドウの生成）も記録したいため、ロガーに依存
///    できない。
/// 2. **同期して永続化できる。** [`settings::atomic::replace_with`] が一時ファイル →
///    `sync_all` → `rename` を行うので、この関数が戻った時点で記録は完全な形でディスクに
///    ある。記録機構のファイル書き込みはプロセス内のバッファ（`RotatingFile::buffer`）と
///    プラグイン側のミューテックスに依存し、フックがそこで詰まる可能性を排除できない。
/// 3. **記録機構のファイルハンドルと競合しない。**同じファイルへ追記すると、プラグインが
///    数える現在の大きさ（`RotatingFile::current_size`）と実際の内容がずれ、ローテーションの
///    判断が狂う。別ファイルなら両者が互いを知らずに済む。
/// 4. **置換なので上限が構造的に守られる。**記録は 1 件だけを保持し（古い記録は消える =
///    要件 8.5 の「古いものから破棄」）、ファイルは [`MAX_CRASH_RECORD_BYTES`] 以下である。
///    ローテーション対象外のファイルを足しても、保持合計は方針の合計上限を超えない
///    （コンパイル時検査）。
///
/// # 再入の防止
///
/// [`record_once`] を通してから記録する。記録の途中でパニックしてフックが再入しても、
/// [`ABNORMAL_TERMINATION_RECORDING`] が立っているため**記録経路には入らない**（フックは
/// 続けて元のフックを呼ぶので、パニックの処理はそのまま進む）。**無限の再帰にはならない。**
///
/// # 失敗したとき
///
/// 記録を書けないこと（ディレクトリが消えた・読み取り専用になった等）は stderr に 1 行残して
/// 続行する。**ここでパニックしてはならない**（フックの中であり、記録の失敗をパニックに
/// 変えると元の異常終了の内容が失われる）。
fn record_abnormal_termination(
    path: &Path,
    main_thread: std::thread::ThreadId,
    info: &PanicHookInfo<'_>,
) {
    let _ = record_once(|| {
        let record = CrashRecord::from_panic(info, main_thread);
        let written = settings::atomic::replace_with(path, |file| {
            write_crash_record(file, &record).map(|_written| ())
        });
        if let Err(error) = written {
            let _ = writeln!(
                std::io::stderr(),
                "異常終了の記録を {} へ書けなかった: {error}",
                path.display()
            );
        }
    });
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
    CspConfig {
        policy,
        connect_src,
    }
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
        if config.policy.is_empty() {
            "(未設定)"
        } else {
            &config.policy
        },
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
    sources
        .iter()
        .map(|s| s.as_ref())
        .collect::<Vec<_>>()
        .join(" ")
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

#[allow(dead_code)] // recovered_from は設定のコマンド面（7.x）が消費するまでの seam。
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
        Self {
            prerequisite,
            detail: detail.into(),
        }
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
    candidates
        .into_iter()
        .find(|path| fs::write(path, record).is_ok())
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
    tauri_plugin_log::log::info!(
        "二重起動を引き継ぎました（cwd={cwd}）: {}",
        request.describe()
    );
    present_window_for_request(app, request);
}

/// 引き継いだ起動要求。**起動時（`RunEvent::Ready`）と引き継ぎ（単一インスタンスの
/// コールバック）で同じ解釈を使う**（タスク 6.1。解釈を 2 箇所に持たない）。
struct LaunchRequest {
    /// 要求されたドキュメントの位置（あれば）。
    document: Option<PathBuf>,
}

impl LaunchRequest {
    /// 起動要求の argv から要求を読む。
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

        // 先頭は実行ファイル自身である。`-` で始まる引数は現時点で解釈しない。**最初の
        // `-` で始まらない引数をドキュメントの位置とみなす**（要件 1.5 が引き渡すのは
        // 実行時の引数であり、どの引数がドキュメントを指すかはこれが唯一の規則である）。
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

/// 起動要求に対応するウィンドウを 1 枚開く（要件 2.2、タスク 6.1）。
///
/// `tauri.conf.json` は起動時のウィンドウを宣言しない（`app.windows` は空である）。生成経路は
/// 6.1 のレジストリが引き取ったため、起動時の 1 枚もこの経路から開く。要求の読み取りは引き継ぎと
/// 同じ [`LaunchRequest`] を使う（引数の解釈を 2 箇所に持たない）。
///
/// **これは同期文脈（`RunEvent::Ready`）だが、[`window::open`] は生成を非同期ランタイムへ
/// 逃がすのでデッドロックしない。**
fn open_startup_window(app: &AppHandle) {
    let argv: Vec<String> = std::env::args().collect();
    let request = LaunchRequest::from_argv(&argv);
    match request.document {
        // ドキュメントを指定した起動: そのドキュメントを関連付けたウィンドウを開く。
        Some(path) => window::open(app, WindowRequest::Document(path)),
        // ドキュメントを指定しない起動: ドキュメントを関連付けないウィンドウを開く（要件 2.2）。
        // 起動前に引き継ぎが既に 1 枚作っている場合は二重に開かない。
        None => {
            if app.state::<WindowRegistry>().is_empty() {
                window::open(app, WindowRequest::Empty);
            }
        }
    }
}

/// 要求に対応するウィンドウを提示する（要件 1.5、2.3）。
///
/// 引き継ぎのコールバック（同期文脈）から呼ばれる。**生成は [`window::open`] が非同期で行う。**
///
/// - ドキュメント要求がある場合: **別のドキュメントを開く要求**である。既存のウィンドウは
///   閉じずに、新しいウィンドウを `doc-<連番>` で開き、要求されたドキュメントを関連付ける
///   （要件 2.3）。ラベルの払い出しと関連付けは [`window::WindowRegistry`] が担う。
/// - ドキュメント要求が無い場合: 既にあるウィンドウを前面に出す（新規作成しない）。
fn present_window_for_request(app: &AppHandle, request: LaunchRequest) {
    match request.document {
        Some(path) => window::open(app, WindowRequest::Document(path)),
        None => present_existing_or_create(app),
    }
}

/// 既にあるウィンドウを前面に出し、1 枚も無ければドキュメント無しのウィンドウを 1 枚開く。
///
/// 引き継ぎ（要件 1.5）と Dock アイコンのクリック（要件 2.9、macOS）の両方が使う。**どの
/// ウィンドウを提示するかは 6.1 のレジストリが決める。**
///
/// **生成中の登録を「既にあるウィンドウ」として扱う。**登録は生成より先に済むため、登録がある
/// のにネイティブのウィンドウがまだ無いのは正常な中間状態である（残骸ではない）。ここで
/// 取り除いたり無視したりすると、生きているウィンドウを写像から落とし、冗長な 2 枚目を作る。
/// 生成中のウィンドウは生成タスクが完了時に自ら前面に出す（[`window::open`]）ので、この関数は
/// 二重に開かずに戻るだけでよい（**待たない** — この関数は同期のコールバックから呼ばれる）。
fn present_existing_or_create(app: &AppHandle) {
    let registry = app.state::<WindowRegistry>();
    if let Some(label) = registry.first_ready_label() {
        if let Some(window) = app.get_webview_window(label.as_str()) {
            window::focus(&window);
            return;
        }
    }
    // 生きているウィンドウは無いが、**生成中のウィンドウが要求を満たす**なら二重に開かない。
    if registry.is_creating() {
        return;
    }
    window::open(app, WindowRequest::Empty);
}

// ---------------------------------------------------------------------------
// 最後のウィンドウと常駐慣習の扱い（要件 2.8, 2.9。タスク 5.4）
// ---------------------------------------------------------------------------

/// 検証専用の引き金が読む環境変数の名前。
///
/// **通常の利用環境に存在しないことを狙った名前である。**値の書式は `[<動作>:]<ミリ秒>` で、
/// 設定されているときだけ [`arm_verification_exit_trigger`] がその時間後に指定された動作を行う。
///
/// - `<ミリ秒>` だけ（5.4 から続く書式。例 `1500`）: [`VerificationAction::Exit`]
/// - `exit:<ミリ秒>`: 同上（明示形）
/// - `panic:<ミリ秒>`（5.5 が足した形）: [`VerificationAction::Panic`] — **意図的なパニックで
///   プロセスを異常終了させ、異常終了の記録（要件 8.2）を実測するために使う**
/// - `sidecar:<ミリ秒>`（5.6 が足した形）: [`VerificationAction::Sidecar`] — **監督を直接呼んで
///   補助プロセスを 1 つ起動し、その ms 後に通常終了する**。終了時に補助プロセスが残らないこと
///   （要件 5.6）を実測するために使う
/// - `fail-window:<ミリ秒>`（6.1 が足した形）: [`VerificationAction::FailWindow`] — **ウィンドウの
///   生成を意図的に失敗させ、その ms 後に通常終了する**。失敗が報告され、既に開いている他の
///   ウィンドウが動作し続けること（要件 2.10）を実測するために使う
///
/// **動作の選択を別の環境変数に分けない。**分けると「検証専用の引き金」の片付けが 2 箇所に
/// なってしまう。7.4 / 7.5 が片付ける対象はこの 1 つ（+ [`arm_verification_exit_trigger`] と
/// [`VerificationAction`]）である。
const VERIFY_EXIT_ENV: &str = "JXCEL_VERIFICATION_EXIT_AFTER_MS";

/// 検証専用の引き金が起こす動作（[`VERIFY_EXIT_ENV`] の `<動作>` 部分）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerificationAction {
    /// 明示的な終了（5.4 の既存の動作。[`request_exit`] を呼ぶ）。
    Exit,
    /// 意図的なパニック（5.5 が足した動作）。**メインスレッドで**起こす。
    Panic,
    /// **監督を直接呼んで補助プロセスを 1 つ起動した状態を作る**（タスク 5.6 が足した動作）。
    /// 終了時に残らないことを実測するために使う。起動は直ちに行い、`<ミリ秒>` は他の動作と
    /// 同じく終了までの待ちである。配布物経由の起動経路（8.1 の `SidecarHost`）が揃うまでの
    /// 代用であり、その時点でこの動作は不要になる。
    Sidecar,
    /// **ウィンドウの生成を意図的に失敗させ、既存のウィンドウが動作し続けることを実測する**
    /// （タスク 6.1 が足した動作。要件 2.10）。失敗は直ちに起こし、`<ミリ秒>` は他の動作と
    /// 同じく終了までの待ちである。失敗の起こし方は [`window::force_creation_failure`] にある。
    FailWindow,
}

impl VerificationAction {
    /// 環境変数の値に書ける名前を解釈する。解釈できない名前は `None`（無視する）。
    fn parse(name: &str) -> Option<Self> {
        match name {
            "exit" => Some(Self::Exit),
            "panic" => Some(Self::Panic),
            "sidecar" => Some(Self::Sidecar),
            "fail-window" => Some(Self::FailWindow),
            _ => None,
        }
    }

    /// 起動行に出す名前。
    fn name(self) -> &'static str {
        match self {
            Self::Exit => "exit",
            Self::Panic => "panic",
            Self::Sidecar => "sidecar",
            Self::FailWindow => "fail-window",
        }
    }
}

/// [`VERIFY_EXIT_ENV`] の値を `(<動作>, <ミリ秒>)` に解釈する。**純粋関数**であり、
/// 書式をテストで固定する（`tests::the_verification_trigger_...`）。
///
/// **動作を書かない値は 5.4 と同じ「明示的な終了」を意味する**（後方互換）。解釈できない値は
/// `None` を返し、呼び出し元が無視する（**配布物の既定の振る舞いを変えない**）。
fn parse_verification_trigger(value: &str) -> Option<(VerificationAction, u64)> {
    let (action, delay) = match value.split_once(':') {
        Some((action, delay)) => (VerificationAction::parse(action.trim())?, delay),
        None => (VerificationAction::Exit, value),
    };
    Some((action, delay.trim().parse::<u64>().ok()?))
}

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
/// 起動完了時の [`RunEvent::Ready`] では起動要求に対応するウィンドウを 1 枚開き
/// （[`open_startup_window`]。タスク 6.1）、検証専用の終了の引き金
/// （[`arm_verification_exit_trigger`]）を用意する（環境変数が無ければ何もしない）。
///
/// **このコールバックは同期文脈である**（イベントループのメインスレッド）。したがって
/// ウィンドウの生成をここで直接行ってはならない — [`window::open`] が生成を非同期ランタイムへ
/// 逃がす（design.md「WindowManager」）。
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
        // 起動の完了時に、起動要求に対応するウィンドウを 1 枚開き（タスク 6.1）、検証専用の
        // 引き金を用意する。環境変数が無ければ引き金は何もしない。
        RunEvent::Ready => {
            open_startup_window(app);
            arm_verification_exit_trigger(app);
        }
        // 通常終了でプロセスが終わる直前の最後の同期点（要件 5.6。タスク 5.6）。
        RunEvent::Exit => shutdown_sidecars(app),
        #[cfg(target_os = "macos")]
        RunEvent::Reopen {
            has_visible_windows,
            ..
        } => handle_reopen(app, has_visible_windows),
        _ => {}
    }
}

/// 通常終了でプロセスが終わる直前に、監督経由で起動したすべての補助プロセスを同期的に終了させる
/// （要件 5.6。タスク 5.6）。
///
/// **使うイベントは [`RunEvent::Exit`] である。**tauri 2.11.5 の `App::make_run_event_loop_callback`
/// は `RuntimeRunEvent::Exit` でこのコールバックを呼んだ**後**に `cleanup_before_exit` を呼ぶため、
/// `RunEvent::Exit` は「通常終了でプロセスが終わる前の最後の同期点」である。`RunEvent::ExitRequested`
/// は拒否できる段であり、macOS の常駐で拒否された場合は終了しないため使わない。2 つの通常終了
/// 経路の双方が `ControlFlow::Exit` を経て `Event::LoopDestroyed` で `RunEvent::Exit` に到達する:
///
/// - [`request_exit`] → `app.exit(0)` → `ExitRequested { code: Some(0) }`（wry 2.11.4 の
///   `Message::RequestExit`）
/// - 最後のウィンドウを閉じる → `ExitRequested { code: None }`（wry 2.11.4 の
///   `TaoWindowEvent::Destroyed` でウィンドウが空になったとき）
///
/// **基盤側の終了時清掃には依存しない。**子は Rust の [`Supervisor`] が `std::process::Command`
/// で起動しており、`tauri-plugin-shell` は依存にすら入っていない（同プラグインの終了時清掃は
/// JS→IPC 経路で起動した子だけを対象とする）。tauri の `cleanup_before_exit` は資源表を空に
/// するだけで、Rust から起動した子を登録簿に持たない（この関数はそれより前に走る）。
///
/// **待ち時間はこのコールバックを止める。**`shutdown_all` は猶予段の満了まで待つため、与える
/// 猶予がそのまま終了時の凍結になる。したがって監督には [`SIDECAR_SHUTDOWN_GRACE`] を与える
/// （Unix は 3.3 の既定 3 秒のまま。Windows は穏やかな段が届かず待つ意味が無いため 300 ms）。
///
/// **このコードは通常終了でしか走らない。**強制終了（`SIGKILL` / `TerminateProcess`）やパニック
/// による異常終了はイベントループを経由せず `RunEvent::Exit` が届かない（research.md
/// 「サイドカーのライフサイクル」）。それらを覆うのは **Unix では補助プロセス自身の親監視**
/// （1.6 / 3.5 の `--parent-pid`。`killpg` はこの関数の内側でしか走らないため、SIGKILL の後の
/// Unix には届かない）と、**Windows では Job Object の `KILL_ON_JOB_CLOSE`**（3.3。カーネルが
/// 強制するため親の異常終了後も有効な唯一の機構）である。[`sweep_orphans_at_startup`] が最後の
/// backstop である。
fn shutdown_sidecars(app: &AppHandle) {
    let supervisor = app.state::<Supervisor>();
    // 起動中の種類を先に数える（core の `shutdown_all` は件数を返さないため、core の契約を
    // 広げずにここで数える）。`get` は死んでいる登録を除去するので、数え漏れない。
    //
    // **数は「この時点で観測した対象」であり、実際に終了させた件数ではない。**この後
    // `shutdown_all` が登録簿を空にするまでの間に 8.1 が `ensure` した子は、この数に現れない
    // （`shutdown_all` は登録簿のロックを保持して終わるので、取り逃すことはない）。記録の
    // 文言もその区別に合わせる。
    let observed: Vec<&'static str> = SidecarKind::ALL
        .iter()
        .filter(|kind| supervisor.get(**kind).is_some())
        .map(|kind| kind.as_str())
        .collect();
    if observed.is_empty() {
        log::info!("終了時点で起動している補助プロセスは観測されなかった");
        return;
    }
    match supervisor.shutdown_all() {
        Ok(()) => log::info!(
            "補助プロセスの終了を完了した（対象として観測: {} 件: {}）",
            observed.len(),
            observed.join(", "),
        ),
        Err(error) => log::error!(
            "補助プロセスの終了に失敗した（対象として観測: {} 件: {}）: {error}",
            observed.len(),
            observed.join(", "),
        ),
    }
}

/// Dock アイコンのクリック（`RunEvent::Reopen`）に対応する（要件 2.9。**macOS のみ**）。
///
/// 常駐中は最後のウィンドウが閉じられた後もプロセスが生きているため、利用者が Dock の
/// アイコンをクリックしてウィンドウを求める経路が要る。見えているウィンドウが 1 枚も無ければ
/// 1 枚提示し直し、あれば何もしない（前面化は OS が行う）。
///
/// **ウィンドウのレジストリ（6.1）が「既存のウィンドウを前面に出す／無ければ 1 枚作る」を
/// 所有する**（[`present_existing_or_create`]）。ここはそれを呼ぶだけである。**このハンドラは
/// 同期文脈（イベントループのメインスレッド）だが、生成は [`window::open`] が非同期で行う。**
#[cfg(target_os = "macos")]
fn handle_reopen(app: &AppHandle, has_visible_windows: bool) {
    if has_visible_windows {
        return;
    }
    log::info!("Dock のクリックを受け付け、ウィンドウを提示し直す");
    present_existing_or_create(app);
}

/// 明示的な終了（[`request_exit`]）、意図的なパニック、または補助プロセスの起動を、環境変数で
/// 実測するための**検証専用**の引き金（[`VERIFY_EXIT_ENV`]）。
///
/// メニュー項目（7.4 / 7.5）が作られる前は、[`request_exit`] を人手で呼ぶ経路が無い。環境変数
/// に `<ミリ秒>`（または `exit:<ミリ秒>`）が設定されているときだけ、その時間だけ待ってから
/// [`request_exit`] を呼ぶ。`panic:<ミリ秒>` のときは代わりに**意図的なパニック**を起こし、
/// 異常終了の記録（要件 8.2）を実測できるようにする（5.5 が足した形）。`sidecar:<ミリ秒>` の
/// ときは**直ちに監督を直接呼んで補助プロセスを 1 つ起動**し（[`start_verification_sidecar`]）、
/// その ms 後に通常終了する（5.6 が足した形。終了時に残らないことを実測する）。
/// `fail-window:<ミリ秒>` のときは**直ちにウィンドウの生成を失敗させ**
/// （[`window::force_creation_failure`]）、その ms 後に通常終了する（6.1 が足した形。失敗が
/// 隔離されることを実測する）。**環境変数が無い通常の起動では関数の先頭で即座に戻るので何も
/// しない**（解釈できない値のときも何もしない）。したがって配布物の既定の振る舞いを変えない。
///
/// 7.4 / 7.5 がメニュー項目を結線したら、この引き金は不要になる。残す場合もメニューの経路を
/// 置き換えてはならない（引き金は環境変数が設定された検証のときだけ働く）。
///
/// **名前は 5.4 のままにしてある**（tasks.md の 5.4 の申し送りが片付け対象としてこの名前を
/// 指しているため）。動作は 4 つを選べるが、仕組みは 1 つのままである。
fn arm_verification_exit_trigger(app: &AppHandle) {
    let Ok(value) = std::env::var(VERIFY_EXIT_ENV) else {
        return;
    };
    let Some((action, delay_ms)) = parse_verification_trigger(&value) else {
        log::warn!("{VERIFY_EXIT_ENV} を解釈できないので無視する: {value:?}");
        return;
    };
    let app = app.clone();
    log::info!(
        "検証専用の引き金が有効である: {delay_ms} ms 後に {} を行う",
        action.name(),
    );
    // 補助プロセスの起動と生成の失敗は**待たずに直ちに行う**（`<ミリ秒>` は終了までの待ちで
    // ある。アプリが生きている間に、補助プロセスが動いていること・他のウィンドウが生きている
    // ことを外部から観測できるようにする）。
    match action {
        VerificationAction::Sidecar => start_verification_sidecar(&app),
        VerificationAction::FailWindow => window::force_creation_failure(&app),
        _ => {}
    }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        match action {
            VerificationAction::Exit
            | VerificationAction::Sidecar
            | VerificationAction::FailWindow => request_exit(&app),
            // **パニックはメインスレッドで起こす。**ほかのスレッドで起こしたパニックはその
            // スレッドを終わらせるだけでプロセスは生き続けるため、「意図的に異常終了させる」を
            // 満たさない。`run_on_main_thread` はイベントループへ処理を渡すので、パニックは
            // `app.run` の中から外へ伝播し、プロセスは異常終了する（フックは伝播の直前に走る）。
            VerificationAction::Panic => {
                if let Err(error) = app.run_on_main_thread(|| {
                    panic!("検証専用の意図的な異常終了（{VERIFY_EXIT_ENV}=panic:<ミリ秒>）");
                }) {
                    log::error!("検証専用のパニックをメインスレッドへ渡せなかった: {error}");
                }
            }
        }
    });
}

/// 検証専用: 監督を直接呼んで補助プロセスを 1 つ起動する（タスク 5.6）。
///
/// **配布物経由の起動経路の代用である。**プラットフォーム別の実行ファイルの解決と、整合性検査を
/// 通した起動を束ねるのは 8.1（`sidecar_host.rs`）の責務であり、ここは 1.7 の同梱原本の置き場
/// （[`verification_sidecar_path`]）を直接指して「監督経由で起動した状態」を作る。これは
/// [`VERIFY_EXIT_ENV`] が設定された検証のときだけ通る経路であり、通常の起動では呼ばれない。
///
/// 起動の失敗は記録に残す（**検証の失敗を無言にしない**）。失敗してもアプリは通常終了の経路へ
/// 進むので、終了時の終了処理そのものは実測できる。
fn start_verification_sidecar(app: &AppHandle) {
    let executable = verification_sidecar_path();
    let spec = SidecarSpec {
        kind: SidecarKind::Smoke,
        executable: executable.clone(),
        args: Vec::new(),
    };
    // アプリの管理状態にある監督（[`sidecar_supervisor`]）へ登録する。終了時の終了
    // （[`shutdown_sidecars`]）が同じ登録簿を見るため、この子は通常終了で終了される。
    match app.state::<Supervisor>().ensure(&spec) {
        Ok(handle) => log::info!(
            "検証用の補助プロセスを起動した: kind={} pid={} executable={}",
            handle.kind().as_str(),
            handle.pid(),
            executable.display(),
        ),
        Err(error) => log::error!(
            "検証用の補助プロセスを起動できなかった（{}）: {error}",
            executable.display(),
        ),
    }
}

/// 検証専用の引き金が起動する補助プロセスの実行ファイル（1.7 の同梱原本の置き場）。
///
/// **8.1 のランタイム解決の代用である。**`CARGO_MANIFEST_DIR` はビルド時の `src-tauri/` を
/// 指し、その値が配布物にも埋め込まれる。8.1 がプラットフォーム別の解決を実装した時点で、
/// この関数は解決済みのパス（Windows / macOS = 実行ファイルの隣の `externalBin`、Linux =
/// `usr/share/jxcel/sidecar-smoke`）を返す経路に置き換わる。それまでは、このリポジトリの
/// 検証で 1.7 が配置した原本だけを指す（語幹は [`SidecarKind::as_str`]、接尾辞は
/// [`BUILD_TARGET_TRIPLE`] と Windows の `.exe`。tasks.md 1.7 の命名規約と同じ組み立て）。
fn verification_sidecar_path() -> PathBuf {
    let suffix = if BUILD_TARGET_TRIPLE.contains("windows") {
        ".exe"
    } else {
        ""
    };
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sidecars")
        .join(format!(
            "{}-{}{}",
            SidecarKind::Smoke.as_str(),
            BUILD_TARGET_TRIPLE,
            suffix,
        ))
}

// ---------------------------------------------------------------------------
// テスト（タスク 5.3 / 5.4 / 5.5 / 5.6）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        clamp_to_char_boundary, confirm_csp_values, csp_config_from, format_crash_record,
        parse_verification_trigger, record_once, vetoes_exit, write_crash_record, CrashRecord,
        CspConfig, ExitControl, Residency, VerificationAction, ABNORMAL_TERMINATION_RECORDING,
        MAX_CRASH_RECORD_BYTES,
    };
    use std::sync::atomic::Ordering;
    use tauri::utils::config::Csp;

    /// 方針文字列から実効設定を組み立てる（本番の `csp_config` が `Config` から取り出すのと
    /// 同じ経路を通す。方針の解釈をテスト側で二重実装しないための入口である）。
    fn policy(text: &str) -> CspConfig {
        csp_config_from(Some(&Csp::Policy(text.to_owned())))
    }

    #[test]
    fn connect_src_with_exactly_the_ipc_destinations_is_accepted() {
        let config =
            policy("default-src 'self'; connect-src ipc: http://ipc.localhost; script-src 'self'");
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
        assert!(
            !control.quit_requested(),
            "初期状態では掛け金は立っていない"
        );
        control.authorize_quit();
        assert!(control.quit_requested());
        // 掛け金が立った後は、macOS の `code: None` でさえ拒否しない。**これが「通常手段で
        // 終了できなくならない」ことの構造的な担保である**（tauri#13511。
        // `request_exit` は掛け金を立ててから `app.exit(0)` を呼ぶ）。
        assert!(!vetoes_exit(Residency::StayResident, None, true));
    }

    // -----------------------------------------------------------------------
    // 異常終了の記録（タスク 5.5、要件 8.2・8.4）
    // -----------------------------------------------------------------------

    /// 固定値の記録（時刻・バージョン・スレッド・メッセージ・位置が既知である）。
    fn sample_record() -> CrashRecord {
        CrashRecord {
            epoch_seconds: 1_700_000_000,
            version: "9.9.9",
            thread: "main (ThreadId(1))".to_owned(),
            process_terminates: true,
            message: "テスト用のパニック".to_owned(),
            location: "src/lib.rs:1:1".to_owned(),
            backtrace: None,
        }
    }

    #[test]
    fn the_crash_record_pins_the_format() {
        // 先頭行が「異常終了という事実」、続けて時刻・バージョン・スレッド・終了の帰結・
        // メッセージ・位置。末尾は改行 1 つで終わる（読み手が完全な記録かどうかを判断できる）。
        assert_eq!(
            format_crash_record(&sample_record()),
            "===== 異常終了（パニック） =====\n\
             時刻 (UNIX epoch 秒): 1700000000\n\
             バージョン: 9.9.9\n\
             スレッド: main (ThreadId(1))\n\
             プロセスの終了: このパニックにより異常終了する\n\
             メッセージ: テスト用のパニック\n\
             位置: src/lib.rs:1:1\n",
        );
    }

    #[test]
    fn a_worker_thread_panic_does_not_claim_the_process_ended() {
        // メインスレッド以外のパニックではプロセスは生き続ける。**「異常終了した」と書かない**
        // （記録は次のパニックで置き換わるので、嘘を残さない）。
        let record = CrashRecord {
            thread: "tokio-runtime-worker (ThreadId(7))".to_owned(),
            process_terminates: false,
            ..sample_record()
        };
        let rendered = format_crash_record(&record);
        assert!(
            rendered.contains("このスレッドだけが終了する"),
            "終了しない事実を述べる: {rendered}"
        );
        assert!(!rendered.contains("このパニックにより異常終了する"));
    }

    #[test]
    fn the_crash_record_includes_the_backtrace_when_captured() {
        let record = CrashRecord {
            backtrace: Some("   0: a\n   1: b\n\n".to_owned()),
            ..sample_record()
        };
        let rendered = format_crash_record(&record);
        assert!(
            rendered.ends_with("バックトレース:\n   0: a\n   1: b\n"),
            "余分な空行を残さず改行 1 つで終わる: {rendered:?}"
        );
        assert_eq!(rendered.matches("バックトレース").count(), 1);
        // トレースが無いときは見出しも出さない。
        assert!(!format_crash_record(&sample_record()).contains("バックトレース"));
    }

    #[test]
    fn the_crash_record_is_written_to_any_sink() {
        // 書き込み先を引数に取るので、ファイル無しで書式と書き込みを固定できる。
        let mut sink = Vec::new();
        let written = write_crash_record(&mut sink, &sample_record())
            .expect("メモリへの書き込みは失敗しない");
        assert_eq!(written, sink.len());
        assert_eq!(
            String::from_utf8(sink.clone()).expect("UTF-8"),
            format_crash_record(&sample_record())
        );
        assert!(sink.ends_with(b"\n"));
        assert!(!sink.ends_with(b"\n\n"), "改行を重ねない");
    }

    #[test]
    fn an_oversized_crash_record_is_clamped_at_a_character_boundary() {
        // 上限を大きく超えるメッセージ（マルチバイト文字）でも記録は残り、上限を超えない。
        let record = CrashRecord {
            message: "あ".repeat(MAX_CRASH_RECORD_BYTES),
            ..sample_record()
        };
        let mut sink = Vec::new();
        write_crash_record(&mut sink, &record).expect("メモリへの書き込みは失敗しない");
        assert!(sink.len() <= MAX_CRASH_RECORD_BYTES, "上限を超えない");
        let text = String::from_utf8(sink).expect("文字境界で切るので UTF-8 のまま");
        assert!(text.starts_with("===== 異常終了（パニック） ====="));
        assert!(
            text.contains("切り詰めた"),
            "切り詰めた事実を残す: {text:?}"
        );
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn clamping_never_splits_a_character() {
        // 3 バイト文字の途中で切ろうとすると 2 バイト目まで戻る（有効な UTF-8 を保つ）。
        assert_eq!(clamp_to_char_boundary("あい", 4), "あ");
        assert_eq!(clamp_to_char_boundary("あい", 3), "あ");
        assert_eq!(clamp_to_char_boundary("あい", 2), "");
        assert_eq!(clamp_to_char_boundary("abc", 2), "ab");
        assert_eq!(clamp_to_char_boundary("abc", 3), "abc");
        assert_eq!(clamp_to_char_boundary("abc", 99), "abc");
    }

    #[test]
    fn the_recording_guard_prevents_reentry() {
        // 初回は本体が走り、フラグは元に戻る（2 回目も走る）。
        assert_eq!(record_once(|| 1), Some(1));
        assert_eq!(record_once(|| 2), Some(2));
        // 記録中に再入すると本体を走らせない。**これが「記録経路のパニックで無限に続かない」
        // ことのテストである**（フラグは record_once の外からも立てられる形にしてある）。
        assert!(
            !ABNORMAL_TERMINATION_RECORDING.swap(true, Ordering::SeqCst),
            "事前条件: フラグは下りている"
        );
        let mut ran = false;
        assert_eq!(
            record_once(|| {
                ran = true;
            }),
            None
        );
        assert!(!ran, "再入では本体を走らせない");
        ABNORMAL_TERMINATION_RECORDING.store(false, Ordering::SeqCst);
    }

    // -----------------------------------------------------------------------
    // 検証専用の引き金（タスク 5.4 / 5.5）
    // -----------------------------------------------------------------------

    #[test]
    fn the_verification_trigger_keeps_the_plain_integer_meaning() {
        // 5.4 が文書化した書式（整数だけ）は「明示的な終了」のままである（後方互換）。
        assert_eq!(
            parse_verification_trigger("1500"),
            Some((VerificationAction::Exit, 1500))
        );
        assert_eq!(
            parse_verification_trigger("0"),
            Some((VerificationAction::Exit, 0))
        );
    }

    #[test]
    fn the_verification_trigger_can_select_an_intentional_panic() {
        // 5.5 が足した形。**同じ環境変数のまま**動作を選べる（片付けは 1 箇所）。
        assert_eq!(
            parse_verification_trigger("panic:1500"),
            Some((VerificationAction::Panic, 1500))
        );
        assert_eq!(
            parse_verification_trigger("exit:1500"),
            Some((VerificationAction::Exit, 1500))
        );
        assert_eq!(
            parse_verification_trigger(" panic : 1500 "),
            Some((VerificationAction::Panic, 1500))
        );
    }

    #[test]
    fn the_verification_trigger_can_start_a_sidecar() {
        // 5.6 が足した形。**同じ環境変数のまま**補助プロセスを起動した状態を作れる
        // （片付けは 1 箇所のまま）。
        assert_eq!(
            parse_verification_trigger("sidecar:1500"),
            Some((VerificationAction::Sidecar, 1500))
        );
        assert_eq!(
            parse_verification_trigger(" sidecar : 0 "),
            Some((VerificationAction::Sidecar, 0))
        );
    }

    #[test]
    fn the_verification_trigger_can_fail_a_window_creation() {
        // 6.1 が足した形。**同じ環境変数のまま**生成の失敗を起こせる（片付けは 1 箇所のまま。
        // 要件 2.10 の隔離を実測するために使う）。
        assert_eq!(
            parse_verification_trigger("fail-window:3000"),
            Some((VerificationAction::FailWindow, 3000))
        );
        assert_eq!(
            parse_verification_trigger(" fail-window : 0 "),
            Some((VerificationAction::FailWindow, 0))
        );
    }

    #[test]
    fn an_uninterpretable_verification_trigger_selects_nothing() {
        // 解釈できない値では**何もしない**（配布物の既定の振る舞いを変えない）。
        for value in [
            "",
            "abc",
            "panic",
            "panic:",
            "exit:",
            "sidecar",
            "sidecar:",
            "sidecar:x",
            "fail-window",
            "fail-window:",
            "fail-window:x",
            "crash:1500",
            "1500:panic",
            "-1",
            "1.5",
        ] {
            assert_eq!(
                parse_verification_trigger(value),
                None,
                "解釈できてはならない: {value:?}"
            );
        }
    }
}

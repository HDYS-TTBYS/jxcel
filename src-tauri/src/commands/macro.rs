//! マクロのコマンド面 — 一覧・保存・削除・実行の 4 つと、メニューの 1 項目
//! （`macro-runtime` スペックの tasks.md 4.3。要件 1.1–1.7, 2.1–2.6, 3.1–3.5, 6.1–6.5, 7.1,
//! 8.2, 9.1–9.4）。
//!
//! # 4 つの経路
//!
//! | コマンド | 何をするか | 文書を変えるか |
//! |---|---|---|
//! | [`macro_list`] | そのウィンドウの文書のマクロを要約へ写す（名前・種別・宣言・**解釈できなかった理由**） | 変えない |
//! | [`macro_store`] | 1 件を保存する（同じ名前は置き換え。要件 1.6） | **1 回の `edit`** |
//! | [`macro_delete`] | 名前で 1 件を取り除く（要件 1.7） | **1 回の `edit`**（相手が無ければ呼ばない） |
//! | [`macro_run`] | 1 件を実行し、**成功で変更があるときだけ**適用する | 実行の結果しだい |
//!
//! **呼び出し元ウィンドウは基盤が注入する [`WebviewWindow`] から取る**（ペイロードで受け取らない
//! ＝ 偽装できない。要件 4.6、`ipc-contract.md`）。したがって 4 つとも要求の型にウィンドウは
//! 現れない。
//!
//! # 層の鎖（このモジュールが両側を繋ぐ唯一の場所である）
//!
//! エンジン（`crates/macro-runtime`）は**文書へ触らない**（design.md 決定 2 / 3）。文書の
//! パート（`document-format` の `macros.json`）を読み書きするのは本モジュールであり、
//! 写像は 2 方向ある:
//!
//! - **上流 → エンジン**（[`to_domain`]）: 記録（名前・種別・ソース）をエンジンの値へ写す
//! - **エンジン → 上流**（[`to_upstream`]）: 1 件を上流の記録へ写す（**触った 1 件にだけ使う**）
//!
//! **触っていない記録を作り直さないのが要点である。** `document_format::MacroRecord` は
//! 解釈しないキー（前方互換のために保持しているもの。`document-format` の要件 6.2 / 6.3）を
//! 内部に持ち、`MacroRecord::new` はそれを引き継がない。したがって保存と削除は
//! **上流の並びを複製し、規則が触った 1 件だけを差し替える**（[`write_back`]）。
//! 1.6 の結合検査（`tests/macro_part_roundtrip.rs`）も「製品の経路ではこの写像をアダプタ
//! （群 4）が担う」と明記しており、そのアダプタが本モジュールである。
//!
//! # 保存と削除は `edit` の閉包 1 回で行う（`structure.md`「セッションの所有の規約」）
//!
//! 未保存の印と版の記録は `document-session` が**同じ臨界区間の内側で**行う。下流は印を立てない
//! — 立てる手段が無い（`change::edit` だけが可変の貸出口である）。したがって本モジュールは
//! 閉包の中で**規則の適用（エンジンの `store` / `delete`）と上流への書き戻し**だけを行い、
//! 「保存した」「消した」という事実はセッションの側が記録する。
//!
//! **一覧と削除の存在確認は読みの閉包の中で行う。** 削除は「取り除く相手がいるか」を先に
//! 確かめ、いなければ `edit` を呼ばない — 呼べばセッションが**未保存の印と版を進めてしまう**
//! （`change.rs` の規律「閉包が失敗を返しても印を立てる」）ためであり、存在しない名前の削除で
//! 文書が「未保存」になるのは誤りである。
//!
//! # 実行の流れ（design.md「System Flows → 実行の流れ」）
//!
//! 1. **記録を文書から引く**（要求は名前だけを運ぶ。ソースは文書の中のものである）
//! 2. **上限を設定から解決する**（要件 6.5。[`resolve_limits`]）
//! 3. **縫い目（[`DocumentHost`]）を作り、実行する**（`MacroActor::run`。同期である）
//! 4. `Ran` で**変更があるときだけ**適用する（[`apply_macro_changes`]。要件 5.1）。失敗と
//!    打ち切りは何も適用しない（要件 6.3, 7.3）
//! 5. 記録に 1 行残す（要件 2.6。[`describe_run`]）
//!
//! ## 実行モデル（**`spawn_blocking` の上でだけ呼ぶ**）
//!
//! [`MacroActor::run`](macro_runtime::MacroActor::run) は同期であり、内部で tokio の
//! `blocking_send` / `blocking_recv` を使う。**非同期の実行文脈から直接呼ぶと tokio が panic
//! する**ため、[`macro_run`] は本体を
//! [`spawn_blocking`](tauri::async_runtime::spawn_blocking) へ載せる（`dialog.rs` /
//! `session/commands.rs` と同じ形）。実行は上限まで 30 秒かかりうるので、イベントループと
//! ランタイムのワーカーを塞がないことは要件 2.2 の前提でもある。
//!
//! ## 実行の間、グリッドの保持のロックを握らない（要件 2.2）
//!
//! 適用先のシート・計画・**履歴**はグリッドの保持（[`crate::commands::grid`] の `SheetEntry`）が
//! 所有する（履歴はドキュメント単位であり、マクロの変更が画面の取り消し 1 回で戻るために
//! 同じ実体でなければならない。要件 7.1）。ロックを握るのは**適用の閉包の内側だけ**であり
//! （[`GridSessions::with_displayed`](crate::commands::grid::GridSessions::with_displayed)）、
//! 実行そのものはロックの外で終わっている — 実行の間ロックを握れば、表を描く
//! `grid_rows_window` が待たされ、要件 2.2 の「表の操作を止めない」が破れる。
//!
//! ## 適用の後、画面は文書の状態変化の通知で作り直される
//!
//! 適用は `data-grid` の `EditApply` を通るため、グリッドのセッション（表示の順序・違反の索引）
//! は**適用の直後には古い**（セッションの側は変更を知らない）。本モジュールは適用が文書を
//! 変えたときに `DOCUMENT_SESSION_CHANGED_EVENT` を対象ウィンドウへ 1 回送り、画面が
//! シートを開き直して作り直す（`session/commands.rs` の `emit_session_changed`。保存・新規と
//! 同じ 1 本の経路である）。**履歴は開き直しを越えて保たれる** — `grid_open_sheet` の
//! `answer_open` が「保持していたシートが差し替え後の文書にも在れば履歴を持ち出す」規則を
//! 持つためである（要件 7.1 は開き直しの後も成立する）。
//!
//! # 失敗の載せ方（design.md「Error Handling」の表）
//!
//! **マクロの失敗と打ち切りは封筒の成功腕に載る**（[`MacroRunResponse::outcome`] の 3 値）。
//! 「実行したが失敗した」は要求が成立した結果であり、[`IpcError`] の腕に載せると**面が
//! 戻り値と出力を失う**（要件 2.4 は失敗を同じ面に出すことを求める）。
//!
//! **封筒の失敗腕へ落ちるのは経路の失敗だけである**:
//!
//! | 状態 | 腕 |
//! |---|---|
//! | そのウィンドウに文書が無い（[`SessionError`]） | 失敗 |
//! | 要求された名前のマクロが文書に無い | 失敗 |
//! | 実行基盤が要求を受け取らない（実行中・停止済み） | 失敗 |
//! | グリッドが開かれておらず、**変更の適用先が決まらない** | 失敗 |
//! | 変更の適用が上流に拒まれた（存在しない行など。要件 5.4） | 失敗（**文書は変わらない**） |
//! | マクロが例外で終わった・打ち切られた | **成功**（3 値の `Failed` / `Aborted`） |
//! | ソースの解釈に失敗した（能力宣言・構文） | **成功**（3 値の `Failed`。要件 1.4, 3.4） |
//!
//! 理由の文言を組み立てるのは適応層の仕事である（`grid.rs` / `session/commands.rs` と同じ規律。
//! エンジンの誤り型は表示用の文言を持たない）。
//!
//! # 記録（要件 2.6。design.md「Monitoring」）
//!
//! 実行が**始まった**とき、1 回につき 1 行残す:
//!
//! ```text
//! macro_run: ウィンドウ = doc-1 / マクロ = 棚卸し / 種別 = typescript / 結果 = ran /
//!            打ち切り = (なし) / 変更 = 1（セル 1 / 追加 0 / 削除 0 / 複製 0）/ 所要 = 12 ms
//! ```
//!
//! **ソースと値は出さない**（要件 8.3 の通信内容保護と同じ規律）。出すのは名前・種別・結果・
//! 打ち切りの種類・変更の件数・所要だけである。**ウィンドウのラベルを足した**のは、実起動の
//! 観測（5.2）が複数のウィンドウを開くためであり、どのウィンドウの実行かを記録から読めなければ
//! ならないからである。
//!
//! 適用に失敗したときは、上の 1 行に加えて**適用の失敗**を `log::error!` で残す（実行は成立し、
//! 適用だけが拒まれたという別の事実である。1 行に混ぜると「マクロが失敗した」と読めてしまう）。
//!
//! # 起動の結線
//!
//! [`install`] は `lifecycle::run` が `menu::install` の後（`commands::grid_install` と同じ段）で
//! 呼ぶ。7.4 の登録口（`MenuRegistry`）へ `マクロ > 実行…` を 1 件足し、活性化を
//! [`MACRO_RUN_REQUESTED_EVENT`] として対象ウィンドウへ送るだけである（一覧の提示・選択・
//! 結果は面が担う。4.4）。
//!
//! **実行できるマクロが 1 つも無いときに項目を無効化しない**（要件 2.7 は面が担う）。
//! 有効・無効の述語はフォーカスが移るたびに評価されるが、その時点でドキュメントを読むと
//! イベントループのスレッドが文書のロックを待つ（実行中のマクロが 10 万行を書いていれば
//! 秒単位で待つ）— 要件 2.2 に反する。面は一覧を既に持っているので、導線を出すかどうかは
//! 面が決める（`session/menu.rs` が同じ理由で述語を与えていない）。

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use app_shell::ipc::{
    command_names, IpcError, IpcResult, MacroAbortKind, MacroCapabilityTag, MacroChangeCounts,
    MacroDeleteRequest, MacroDeleteResponse, MacroFailureReport, MacroFailureTag, MacroFrame,
    MacroKindTag, MacroListResponse, MacroOutputLevel, MacroOutputLine, MacroRunOutcome,
    MacroRunRequest, MacroRunResponse, MacroStoreRequest, MacroStoreResponse, MacroSummary,
    WindowContext, WindowLabel, MACRO_RUN_REQUESTED_EVENT,
};
use app_shell::settings::{FileSettingsStore, SettingsKey, SettingsStore};
use document_session::{DocumentSessions, DocumentSessionsApi, SessionError};
use macro_runtime::engine::outcome::{ChangeSummary, FailureKind, LimitKind, RunOutcome};
use macro_runtime::host::HostPort;
use macro_runtime::{
    Limits, MacroFailure, MacroKind, MacroName, MacroRecord, MacroRuntime, MacroRuntimeApi,
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_log::log;

use crate::macro_apply::{apply_macro_changes, MacroApplyError};
use crate::macro_host::DocumentHost;
use crate::menu::{MenuItemSpec, MenuPath, MenuRegistry, MenuSelection};

/// 上流（`document-format` の `macros.json` の 1 件）の型。
type UpstreamRecord = document_format::MacroRecord;

// ---------------------------------------------------------------------------
// エンジン（アプリ全体で 1 実体）
// ---------------------------------------------------------------------------

/// 管理状態の生成を直列化する（`commands/grid.rs` の `GRID_STATE_CREATION` と同じ理由）。
static MACRO_STATE_CREATION: Mutex<()> = Mutex::new(());

/// アプリ全体で 1 実体のエンジンを取る。**初回に作る**（`grid_state` と同じ形）。
///
/// # 起こせなかったときに panic しない理由
///
/// `AppHandle::state` は**管理状態が無ければ panic する**。実行基盤の生成は専用スレッドを
/// 1 つ起こす操作であり、資源の枯渇で失敗しうる — panic させると、その失敗が「コマンドが
/// 壊れている」形で現れ、理由が利用者にも記録にも届かない。したがって `Option` を返し、
/// 呼び出し元が**経路の失敗**として答える。
///
/// **isolate はここでは作らない**（最初の実行まで遅れる。`MacroActor::spawn` の doc）。
/// 起こすのは専用スレッド 1 つだけであり、一覧・保存・削除もこの口を通る（4 面とも
/// `MacroRuntimeApi` の実体を要する）。
fn macro_state(app: &AppHandle) -> Option<State<'_, MacroRuntime>> {
    if app.try_state::<MacroRuntime>().is_none() {
        let _guard = MACRO_STATE_CREATION
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if app.try_state::<MacroRuntime>().is_none() {
            match MacroRuntime::new() {
                Ok(runtime) => {
                    let _ = app.manage(runtime);
                    log::info!("マクロの実行基盤を管理状態として置いた");
                }
                Err(error) => {
                    log::error!(
                        "マクロの実行基盤を起こせなかった（専用スレッドを作れない）: {error}"
                    );
                }
            }
        }
    }
    app.try_state::<MacroRuntime>()
}

// ---------------------------------------------------------------------------
// 文書と境界の間の写像（このモジュールが唯一の場所である）
// ---------------------------------------------------------------------------

/// 上流の記録をエンジンの記録へ写す（名前・種別・ソースの 3 つ組。design.md 決定 4）。
///
/// **意味を足さない** — 上流は形だけを持ち、ソースの解釈はエンジンが行う（1.6）。
fn to_domain(record: &UpstreamRecord) -> MacroRecord {
    MacroRecord::new(
        MacroName::new(record.name()),
        domain_kind(record.kind()),
        record.source(),
    )
}

/// エンジンの記録を上流の記録へ写す。
///
/// **触った 1 件にだけ使う**（モジュール doc「層の鎖」）— `MacroRecord::new` は解釈しない
/// キーの保持を引き継がないため、触っていない記録に使うと保持が落ちる。
fn to_upstream(record: &MacroRecord) -> UpstreamRecord {
    UpstreamRecord::new(
        record.name.as_str(),
        upstream_kind(record.kind),
        record.source.clone(),
    )
}

/// 種別の写像（上流 → エンジン。どちらも 2 値である）。
const fn domain_kind(kind: document_format::MacroKind) -> MacroKind {
    match kind {
        document_format::MacroKind::TypeScript => MacroKind::TypeScript,
        document_format::MacroKind::JavaScript => MacroKind::JavaScript,
    }
}

/// 種別の写像（エンジン → 上流）。
const fn upstream_kind(kind: MacroKind) -> document_format::MacroKind {
    match kind {
        MacroKind::TypeScript => document_format::MacroKind::TypeScript,
        MacroKind::JavaScript => document_format::MacroKind::JavaScript,
    }
}

/// 種別の写像（エンジン → 境界の札）。綴りは文書の中の形と同じ小文字である。
const fn kind_to_boundary(kind: MacroKind) -> MacroKindTag {
    match kind {
        MacroKind::TypeScript => MacroKindTag::TypeScript,
        MacroKind::JavaScript => MacroKindTag::JavaScript,
    }
}

/// 種別の写像（境界の札 → エンジン）。
const fn kind_from_boundary(kind: MacroKindTag) -> MacroKind {
    match kind {
        MacroKindTag::TypeScript => MacroKind::TypeScript,
        MacroKindTag::JavaScript => MacroKind::JavaScript,
    }
}

/// 能力の写像（エンジン → 境界の札）。綴りは宣言に書く正準の綴りそのものである（要件 8.2）。
const fn capability_to_boundary(capability: macro_runtime::Capability) -> MacroCapabilityTag {
    match capability {
        macro_runtime::Capability::FileRead => MacroCapabilityTag::FileRead,
        macro_runtime::Capability::FileWrite => MacroCapabilityTag::FileWrite,
        macro_runtime::Capability::Net => MacroCapabilityTag::Net,
    }
}

/// 出力の種別の写像（エンジン → 境界）。
const fn output_level_to_boundary(level: macro_runtime::OutputLevel) -> MacroOutputLevel {
    match level {
        macro_runtime::OutputLevel::Log => MacroOutputLevel::Log,
        macro_runtime::OutputLevel::Info => MacroOutputLevel::Info,
        macro_runtime::OutputLevel::Warn => MacroOutputLevel::Warn,
        macro_runtime::OutputLevel::Error => MacroOutputLevel::Error,
        macro_runtime::OutputLevel::Debug => MacroOutputLevel::Debug,
    }
}

/// 失敗の写像（エンジン → 境界。要件 2.4, 9.1, 9.2, 9.3）。
///
/// ホスト API の拒否は**拒んだ API の名前**を運ぶ（要件 9.2）。フレームは内側から外側へ
/// 並んだまま写す（要件 9.3）。`function` の `None` は空文字へ落ちる（境界は 64 ビットでも
/// `Option` でもなく、文字列と 32 ビット整数だけを運ぶ規約に従う）。
fn failure_to_boundary(failure: &MacroFailure) -> MacroFailureReport {
    MacroFailureReport {
        kind: match &failure.kind {
            FailureKind::Source => MacroFailureTag::Source,
            FailureKind::Transpile => MacroFailureTag::Transpile,
            FailureKind::Execution => MacroFailureTag::Execution,
            FailureKind::HostRejected { api } => MacroFailureTag::HostRejected { api: api.clone() },
        },
        reason: failure.message.clone(),
        frames: failure
            .frames
            .iter()
            .map(|frame| MacroFrame {
                macro_name: frame.macro_name.as_str().to_owned(),
                function: frame.function.clone().unwrap_or_default(),
                line: frame.line,
                column: frame.column,
            })
            .collect(),
    }
}

/// 件数の写像（`usize` → `u32`。**飽和させる**）。
///
/// 境界は 64 ビット整数を運ばない（`ipc-contract.md`）。実行 1 回の変更が 40 億件を超えることは
/// 無い（超えれば「非常に多い」という提示の意味しか失われない）。
fn count_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// 実行の所要の写像（ミリ秒。**飽和させる**）。
fn millis_to_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// 要約の写像（エンジン → 境界。要件 1.3, 1.4, 8.2）。
///
/// **解釈できなかった 1 件も写す**（理由つきで一覧に残す。要件 1.4）。そのとき能力は空である
/// （`failure` の有無が両者を区別する）。
fn summary_to_boundary(summary: &macro_runtime::MacroSummary) -> MacroSummary {
    MacroSummary {
        name: summary.name.as_str().to_owned(),
        kind: kind_to_boundary(summary.kind),
        capabilities: summary
            .declaration
            .as_ref()
            .map(|declaration| {
                declaration
                    .iter()
                    .map(capability_to_boundary)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        failure: summary.failure.as_ref().map(failure_to_boundary),
    }
}

/// 実行の結果の写像（エンジン → 境界。要件 2.3, 2.4, 6.1, 6.2）。
fn outcome_to_boundary(outcome: &RunOutcome) -> MacroRunOutcome {
    match outcome {
        RunOutcome::Ran {
            value,
            output,
            changes,
            elapsed_ms,
        } => MacroRunOutcome::Ran {
            value: value.clone(),
            output: output
                .iter()
                .map(|line| MacroOutputLine {
                    level: output_level_to_boundary(line.level),
                    text: line.text.clone(),
                })
                .collect(),
            changes: MacroChangeCounts {
                set_cells: count_to_u32(changes.set_cells),
                inserted_rows: count_to_u32(changes.inserted_rows),
                removed_rows: count_to_u32(changes.removed_rows),
                duplicated_rows: count_to_u32(changes.duplicated_rows),
            },
            elapsed_ms: millis_to_u32(*elapsed_ms),
        },
        RunOutcome::Failed { failure } => MacroRunOutcome::Failed {
            failure: failure_to_boundary(failure),
        },
        RunOutcome::Aborted {
            limit,
            elapsed_ms,
            failure,
        } => MacroRunOutcome::Aborted {
            limit: match limit {
                LimitKind::Time => MacroAbortKind::Time,
                LimitKind::Memory => MacroAbortKind::Memory,
            },
            elapsed_ms: millis_to_u32(*elapsed_ms),
            failure: failure_to_boundary(failure),
        },
    }
}

/// **エンジンが適用した規則の効果を、上流の並びへ写す**（要件 1.6）。
///
/// 規則（同じ名前は置き換え・位置を保つ）の実体はエンジンの `store` であり、ここはその効果を
/// **名前で位置を引いて**反映する。エンジンの `upsert` が触るのは「同じ名前の最初の 1 件の
/// 置き換え」か「末尾への追加」のどちらかであり、[`to_upstream`] を当てるのはその 1 件だけに
/// 閉じる（触っていない記録の解釈しないキーの保持を落とさない）。
fn write_back(upstream: &mut Vec<UpstreamRecord>, domain: &[MacroRecord], name: &MacroName) {
    let Some(record) = domain.iter().find(|record| &record.name == name) else {
        // エンジンの `store` は必ず 1 件を残す（置き換えか追加）。到達しない。
        return;
    };
    match upstream
        .iter()
        .position(|existing| existing.name() == name.as_str())
    {
        Some(at) => upstream[at] = to_upstream(record),
        None => upstream.push(to_upstream(record)),
    }
}

/// 上流の並びから、エンジンの規則で 1 件を取り除く（要件 1.7）。
///
/// 名前は一意であるという前提の下では 1 件だけが落ちる。上流は一意性を検査しない
/// （`macros-parts.rs` のモジュール doc）ため、重複した文書では**先頭の 1 件**が落ちる —
/// エンジンの `remove` と同じ規則である。
fn remove_upstream(upstream: &mut Vec<UpstreamRecord>, name: &MacroName) {
    if let Some(at) = upstream
        .iter()
        .position(|existing| existing.name() == name.as_str())
    {
        upstream.remove(at);
    }
}

// ---------------------------------------------------------------------------
// 上限の解決（要件 6.5）
// ---------------------------------------------------------------------------

/// 設定から実行の上限を解決する（要件 6.5。design.md「Data Contracts & Integration」）。
///
/// **既定の源はエンジンである。** 鍵が無い（または読めない）ときは `Limits::DEFAULT_*` を使う —
/// 既定値を本モジュールにも書くと、エンジン側の既定が変わったときに黙って食い違う。
/// 読めない値（別の型・負数）も既定へ落とす: 設定は利用者が手で書けるファイルであり、
/// 壊れた 1 つの値でマクロが走らなくなるより、既定で走る方がよい。
///
/// **実行のたびに解決する。** 設定を変えた次の実行から新しい上限が効く（要件 6.5）ことが、
/// この関数を実行の直前に呼ぶことそのもので満たされる。値の妥当性の検査は行わない
/// （時間 0 は即時の打ち切り、メモリ 0 は V8 の既定というエンジンの解釈をそのまま通す。
/// 設定は利用者の明示の入力である）。
fn resolve_limits(settings: &impl SettingsStore) -> Limits {
    let time = settings
        .get::<u64>(&SettingsKey::MacroTimeLimitMs)
        .map(Duration::from_millis)
        .unwrap_or(Limits::DEFAULT_TIME);
    let memory = settings
        .get::<u64>(&SettingsKey::MacroMemoryLimitBytes)
        .unwrap_or(Limits::DEFAULT_MEMORY_BYTES);
    Limits::new(time, memory)
}

// ---------------------------------------------------------------------------
// 失敗の写像（文言を組み立てるのは適応層の仕事である）
// ---------------------------------------------------------------------------

/// 経路そのものが成立しないことを伝える失敗（封筒の失敗腕）。
fn path_failure(command: &str, label: &WindowLabel, reason: &str) -> IpcError {
    IpcError::Document {
        message: format!(
            "{command}: ウィンドウ {} の経路が成立しない: {reason}",
            label.as_str()
        ),
    }
}

/// セッションの側の失敗を伝える（文書を保持していない等）。
fn session_failure(command: &str, label: &WindowLabel, error: &SessionError) -> IpcError {
    path_failure(command, label, &error.to_string())
}

/// 変更の適用の失敗を伝える（要件 5.4。**ドキュメントは変わっていない**）。
///
/// 上流の文言をそのまま運ぶ場合を除き、利用者に関係する 5 つの理由を日本語で名指しする
/// （`grid.rs` の `describe_reason` と同じ規律 — 提示の文言は適応層が組み立てる）。
fn apply_failure(command: &str, label: &WindowLabel, error: &MacroApplyError) -> IpcError {
    let reason = match error {
        MacroApplyError::SheetUnusable { sheet } => {
            format!("シート {sheet} へは適用できない（宣言が読めないか、列が 1 本も無い）")
        }
        MacroApplyError::SheetMismatch { expected, found } => format!(
            "変更がシート {found} を指しているが、このウィンドウが開いているのは {expected} である"
        ),
        MacroApplyError::UnknownRow { sheet, row } => {
            format!("シート {sheet} に存在しない行 {row} を指した")
        }
        MacroApplyError::UnknownColumn {
            sheet,
            column,
            columns,
        } => format!(
            "シート {sheet} の範囲外の列 {} を指した（列数 {columns}）",
            column.index()
        ),
        MacroApplyError::RowTooWide {
            sheet,
            values,
            columns,
        } => format!("追加する行の値の数 {values} がシート {sheet} の列数 {columns} を越えている"),
        // 上流の拒否・写像の前提の崩れ・変更集合の不在・セッションの失敗は、上流の文言を運ぶ
        // （どれも「経路の前提が崩れた」であり、利用者が直せる種類の誤りではない）。
        MacroApplyError::Rejected { .. }
        | MacroApplyError::Inconsistent { .. }
        | MacroApplyError::Unread
        | MacroApplyError::Session(_) => error.to_string(),
    };
    path_failure(command, label, &reason)
}

// ---------------------------------------------------------------------------
// 記録（要件 2.6。design.md「Monitoring」）
// ---------------------------------------------------------------------------

/// 実行 1 回の記録の 1 行を組み立てる（要件 2.6）。
///
/// **ソースと値は出さない**（名前・種別・結果・打ち切りの種類・変更の件数・所要だけである）。
/// 純粋関数にしてあるのは、**記録の形そのものを検査できるようにする**ためである
/// （実起動では行数を数えることしかできない。`verification.md` の「呼び出しの形を数える」）。
///
/// `elapsed_ms` は**エンジンが所要を運ばない結果**（`Failed`）のときだけ使う値であり、
/// [`macro_run`] が測った実行の所要である。成功と打ち切りはエンジンの値（isolate の生成から
/// 破棄まで。要件 11.3）を使う — 打ち切りと競合しない測り方である（design.md
/// 「MacroRuntimeApi」の Risks）。
fn describe_run(
    label: &WindowLabel,
    name: &str,
    kind: MacroKind,
    outcome: &RunOutcome,
    elapsed_ms: u64,
) -> String {
    let (result, limit, changes, elapsed) = match outcome {
        RunOutcome::Ran {
            changes,
            elapsed_ms,
            ..
        } => ("ran", "(なし)", *changes, *elapsed_ms),
        RunOutcome::Failed { .. } => ("failed", "(なし)", ChangeSummary::default(), elapsed_ms),
        RunOutcome::Aborted {
            limit, elapsed_ms, ..
        } => (
            "aborted",
            limit.as_str(),
            ChangeSummary::default(),
            *elapsed_ms,
        ),
    };
    format!(
        "macro_run: ウィンドウ = {} / マクロ = {name} / 種別 = {} / 結果 = {result} / \
         打ち切り = {limit} / 変更 = {}（セル {} / 追加 {} / 削除 {} / 複製 {}）/ 所要 = {elapsed} ms",
        label.as_str(),
        kind.as_str(),
        changes.total(),
        changes.set_cells,
        changes.inserted_rows,
        changes.removed_rows,
        changes.duplicated_rows,
    )
}

// ---------------------------------------------------------------------------
// コマンド 4 本
// ---------------------------------------------------------------------------

/// 呼び出し元ウィンドウの文書が持つマクロの一覧を返す（要件 1.3, 1.4, 8.2）。
///
/// **実行しない。** 解釈（能力の宣言と、種別としての構文）だけを行い、解釈できなかった
/// マクロも一覧に残して理由を添える（要件 1.4 の「ドキュメントは開ける」）。
#[tauri::command(async)]
pub fn macro_list(app: AppHandle, window: WebviewWindow) -> IpcResult<MacroListResponse, IpcError> {
    let command = command_names::MACRO_LIST;
    let context = caller_context(&window);
    let documents = super::grid::documents_of(&app);

    let result = match macro_state(&app) {
        Some(runtime) => answer_list(&runtime, &documents, &context.window),
        None => IpcResult::Err {
            error: path_failure(command, &context.window, "実行基盤を起こせない"),
        },
    };
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / マクロ = {} 件（解釈できない = {} 件）",
            context.window.as_str(),
            data.macros.len(),
            data.macros
                .iter()
                .filter(|summary| summary.failure.is_some())
                .count(),
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

/// 呼び出し元ウィンドウの文書へマクロを 1 件保存する（要件 1.1, 1.5, 1.6）。
///
/// 同じ名前のマクロは**置き換え**である（要件 1.6）。保存は `edit` の閉包 1 回で行うため、
/// **未保存の印と版が立つ**（`structure.md`「セッションの所有の規約」）。
#[tauri::command(async)]
pub fn macro_store(
    app: AppHandle,
    window: WebviewWindow,
    request: MacroStoreRequest,
) -> IpcResult<MacroStoreResponse, IpcError> {
    let command = command_names::MACRO_STORE;
    let context = caller_context(&window);
    let documents = super::grid::documents_of(&app);

    let result = match macro_state(&app) {
        Some(runtime) => answer_store(&runtime, &documents, &context.window, &request),
        None => IpcResult::Err {
            error: path_failure(command, &context.window, "実行基盤を起こせない"),
        },
    };
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / マクロ = {} / 種別 = {:?} / 解釈 = {} / 一覧 = {} 件",
            context.window.as_str(),
            data.stored.name,
            data.stored.kind,
            if data.stored.failure.is_some() {
                "できない（理由つきで保存した）"
            } else {
                "できる"
            },
            data.macros.len(),
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

/// 呼び出し元ウィンドウの文書からマクロを 1 件削除する（要件 1.7）。
///
/// **取り除く相手が無ければ `edit` を呼ばない** — 呼べばセッションが未保存の印と版を進める
/// （`change.rs` の規律）ためであり、存在しない名前の削除で文書が「未保存」になるのは誤りで
/// ある。その場合の応答は**成功**であり、`removed: false` と削除前の一覧を返す。
#[tauri::command(async)]
pub fn macro_delete(
    app: AppHandle,
    window: WebviewWindow,
    request: MacroDeleteRequest,
) -> IpcResult<MacroDeleteResponse, IpcError> {
    let command = command_names::MACRO_DELETE;
    let context = caller_context(&window);
    let documents = super::grid::documents_of(&app);

    let result = match macro_state(&app) {
        Some(runtime) => answer_delete(&runtime, &documents, &context.window, &request),
        None => IpcResult::Err {
            error: path_failure(command, &context.window, "実行基盤を起こせない"),
        },
    };
    match &result {
        IpcResult::Ok { data } => log::info!(
            "{command}: 呼び出し元ウィンドウ = {} / マクロ = {} / 取り除いた = {} / 一覧 = {} 件",
            context.window.as_str(),
            request.name,
            data.removed,
            data.macros.len(),
        ),
        IpcResult::Err { error } => log::error!(
            "{command}: 呼び出し元ウィンドウ = {} / 失敗 = {error}",
            context.window.as_str(),
        ),
    }
    result
}

/// 呼び出し元ウィンドウの文書に対してマクロを 1 件実行する（要件 2.1–2.6, 6.5, 7.1）。
///
/// **実行は `spawn_blocking` の上で行う**（モジュール doc「実行モデル」）。適用が文書を
/// 変えたときだけ `DOCUMENT_SESSION_CHANGED_EVENT` を対象ウィンドウへ 1 回送る
/// （画面がシートを開き直し、表示と違反の索引を作り直す。保存・新規と同じ 1 本の経路）。
#[tauri::command]
pub async fn macro_run(
    app: AppHandle,
    window: WebviewWindow,
    request: MacroRunRequest,
) -> IpcResult<MacroRunResponse, IpcError> {
    let command = command_names::MACRO_RUN;
    let context = caller_context(&window);
    let label = context.window.clone();
    let working = app.clone();

    let joined = tauri::async_runtime::spawn_blocking(move || {
        let documents = super::grid::documents_of(&working);
        let grids = super::grid::grid_state(&working);
        // 設定の実体は `Arc` で共有されている（要件 7.3）。`State` の借用はこのスレッドの
        // 中で閉じるため、`Arc` を複製して持ち込む。
        let settings = Arc::clone(&working.state::<Arc<FileSettingsStore>>());
        match macro_state(&working) {
            Some(runtime) => answer_run(&runtime, &documents, &grids, &settings, &label, &request),
            None => IpcResult::Err {
                error: path_failure(command, &label, "実行基盤を起こせない"),
            },
        }
    })
    .await;

    let result = match joined {
        Ok(result) => result,
        Err(error) => {
            // 実行のスレッドが異常終了した（**面の失敗**。マクロの失敗ではない）。
            log::error!(
                "{command}: ウィンドウ = {} の実行の処理が異常終了した: {error}",
                context.window.as_str(),
            );
            return IpcResult::Err {
                error: IpcError::Document {
                    message: format!("実行の処理が異常終了した: {error}"),
                },
            };
        }
    };

    // **文書が変わったときだけ**通知する（変更の件数が 0 の実行でイベントを飛ばさない）。
    if let IpcResult::Ok { data } = &result {
        if matches!(&data.outcome, MacroRunOutcome::Ran { changes, .. } if !changes.is_empty()) {
            crate::session::commands::emit_session_changed(&window);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// 本体（Tauri の実体を要さない。テストはここを直接駆動する）
// ---------------------------------------------------------------------------

/// 一覧の本体（[`macro_list`] の中身）。
pub(crate) fn answer_list(
    runtime: &MacroRuntime,
    documents: &Arc<DocumentSessions>,
    label: &WindowLabel,
) -> IpcResult<MacroListResponse, IpcError> {
    let command = command_names::MACRO_LIST;
    let context = WindowContext {
        window: label.clone(),
    };
    match summaries_of(runtime, documents, label) {
        Ok(macros) => IpcResult::Ok {
            data: MacroListResponse { context, macros },
        },
        Err(error) => IpcResult::Err {
            error: path_failure(command, label, &error),
        },
    }
}

/// 保存の本体（[`macro_store`] の中身）。
pub(crate) fn answer_store(
    runtime: &MacroRuntime,
    documents: &Arc<DocumentSessions>,
    label: &WindowLabel,
    request: &MacroStoreRequest,
) -> IpcResult<MacroStoreResponse, IpcError> {
    let command = command_names::MACRO_STORE;
    let context = WindowContext {
        window: label.clone(),
    };
    let record = MacroRecord::new(
        MacroName::new(request.name.clone()),
        kind_from_boundary(request.kind),
        request.source.clone(),
    );

    // **閉包 1 回**: 規則（同じ名前は置き換え。要件 1.6）を適用して上流の並びへ書き戻す。
    // 未保存の印と版は `document-session` が同じ臨界区間の内側で記録する（下流は立てない）。
    // **上流の並びを丸ごと作り直さない**（触っていない記録の未知フィールドを落とさない）。
    let edited = documents.edit(label, &mut |document| {
        let mut upstream: Vec<UpstreamRecord> = document.macros().to_vec();
        let mut domain: Vec<MacroRecord> = upstream.iter().map(to_domain).collect();
        let summary = runtime.store(&mut domain, record.clone());
        write_back(&mut upstream, &domain, &record.name);
        document.set_macros(upstream);
        (summary, domain)
    });

    let (stored, after) = match edited {
        Ok(edited) => edited.value,
        Err(error) => {
            return IpcResult::Err {
                error: session_failure(command, label, &error),
            }
        }
    };
    // 保存後の一覧は**閉包の外で**要約する（変換を文書のロックの下で走らせない。1 件の
    // 解釈は `store` が既に閉包の内側で行っている）。
    IpcResult::Ok {
        data: MacroStoreResponse {
            context,
            stored: summary_to_boundary(&stored),
            macros: runtime
                .list(&after)
                .iter()
                .map(summary_to_boundary)
                .collect(),
        },
    }
}

/// 削除の本体（[`macro_delete`] の中身）。
pub(crate) fn answer_delete(
    runtime: &MacroRuntime,
    documents: &Arc<DocumentSessions>,
    label: &WindowLabel,
    request: &MacroDeleteRequest,
) -> IpcResult<MacroDeleteResponse, IpcError> {
    let command = command_names::MACRO_DELETE;
    let context = WindowContext {
        window: label.clone(),
    };
    let name = MacroName::new(request.name.clone());

    // 1. **取り除く相手がいるかを、エンジンの規則で確かめる**（読みの閉包の下）。
    //    相手が無ければ `edit` を呼ばない（モジュール doc「保存と削除は…」）。
    let present = documents.read(label, &mut |document| {
        let mut domain: Vec<MacroRecord> = document.macros().iter().map(to_domain).collect();
        runtime.delete(&mut domain, &name).is_some()
    });
    match present {
        Ok(false) => {
            return match summaries_of(runtime, documents, label) {
                Ok(macros) => IpcResult::Ok {
                    data: MacroDeleteResponse {
                        context,
                        removed: false,
                        macros,
                    },
                },
                Err(reason) => IpcResult::Err {
                    error: path_failure(command, label, &reason),
                },
            }
        }
        Ok(true) => {}
        Err(error) => {
            return IpcResult::Err {
                error: session_failure(command, label, &error),
            }
        }
    }

    // 2. **閉包 1 回**で取り除く（上流の並びも同じ 1 件だけを落とす）。
    let edited = documents.edit(label, &mut |document| {
        let mut upstream: Vec<UpstreamRecord> = document.macros().to_vec();
        let mut domain: Vec<MacroRecord> = upstream.iter().map(to_domain).collect();
        runtime.delete(&mut domain, &name);
        remove_upstream(&mut upstream, &name);
        document.set_macros(upstream);
        domain
    });
    let after = match edited {
        Ok(edited) => edited.value,
        Err(error) => {
            return IpcResult::Err {
                error: session_failure(command, label, &error),
            }
        }
    };
    IpcResult::Ok {
        data: MacroDeleteResponse {
            context,
            removed: true,
            macros: runtime
                .list(&after)
                .iter()
                .map(summary_to_boundary)
                .collect(),
        },
    }
}

/// 実行の本体（[`macro_run`] の中身）。**`spawn_blocking` の上で呼ぶ。**
pub(crate) fn answer_run(
    runtime: &MacroRuntime,
    documents: &Arc<DocumentSessions>,
    grids: &super::grid::GridSessions,
    settings: &FileSettingsStore,
    label: &WindowLabel,
    request: &MacroRunRequest,
) -> IpcResult<MacroRunResponse, IpcError> {
    let command = command_names::MACRO_RUN;
    let context = WindowContext {
        window: label.clone(),
    };

    // 1. 記録を**文書から**引く（要求は名前だけを運ぶ。実行するのは保存されているもの）。
    let record = documents
        .read(label, &mut |document| {
            document
                .macros()
                .iter()
                .find(|record| record.name() == request.name)
                .map(to_domain)
        })
        .map_err(|error| session_failure(command, label, &error));
    let record = match record {
        Ok(Some(record)) => record,
        Ok(None) => {
            return IpcResult::Err {
                error: path_failure(
                    command,
                    label,
                    &format!("名前 {} のマクロがこの文書に無い", request.name),
                ),
            }
        }
        Err(error) => return IpcResult::Err { error },
    };

    // 2. 上限を設定から解決する（要件 6.5。実行のたびに読むので変更が次の実行から効く）。
    let limits = resolve_limits(settings);

    // 3. 縫い目を作り、実行する。所要の起点はここである — `RunOutcome::Failed` は所要を
    //    運ばない（1.3 の型）ため、記録の 1 行に書く値をここで測る。
    let started = Instant::now();
    let host = Arc::new(DocumentHost::new(Arc::clone(documents), label.clone()));
    let run_request = macro_runtime::RunRequest::new(
        record.clone(),
        limits,
        macro_runtime::WindowLabel::from(label.as_str()),
    );
    let outcome = match runtime.run(run_request, Arc::clone(&host) as Arc<dyn HostPort>) {
        Ok(outcome) => outcome,
        Err(error) => {
            // **実行そのものが始まらなかった**（実行中の 2 つ目の要求・停止済み）。記録の
            // 1 行は実行が始まったときだけ残す（要件 2.6 は実行の結果についての要求である）。
            log::warn!(
                "{command}: ウィンドウ = {} の実行を受け取れなかった（マクロ = {}）: {error}",
                label.as_str(),
                request.name,
            );
            return IpcResult::Err {
                error: path_failure(command, label, &format!("実行を受け取れない（{error}）")),
            };
        }
    };
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    // 4. 記録の 1 行（要件 2.6）。**ソースと値は出さない。**
    log::info!(
        "{}",
        describe_run(label, &request.name, record.kind, &outcome, elapsed_ms)
    );

    // 5. `Ran` で変更があるときだけ適用する（失敗と打ち切りは何も適用しない。要件 6.3, 7.3）。
    if let RunOutcome::Ran { changes, .. } = &outcome {
        if changes.is_empty() {
            return IpcResult::Ok {
                data: MacroRunResponse {
                    context,
                    outcome: outcome_to_boundary(&outcome),
                },
            };
        }
        // 適用先のシート・計画・**履歴**はグリッドの保持が所有する（要件 7.1 の 1 対の履歴）。
        // **実行の間ロックを握らない**（実行は既に終わっている。グリッドの保持の doc を参照）。
        let applied = grids.with_displayed(label, |sheet, schema, history| {
            apply_macro_changes(documents, label, sheet, schema, &*host, history)
        });
        match applied {
            None => {
                return IpcResult::Err {
                    error: path_failure(
                        command,
                        label,
                        "グリッドがまだ開かれていない（変更の適用先のシートを決められない）",
                    ),
                }
            }
            Some(Err(error)) => {
                log::error!(
                    "{command}: ウィンドウ = {} の結果を適用できなかった（マクロ = {}）: {error}",
                    label.as_str(),
                    request.name,
                );
                return IpcResult::Err {
                    error: apply_failure(command, label, &error),
                };
            }
            Some(Ok(applied)) => {
                // 適用した件数と、エンジンが数えた件数は一致する（同じ変更集合を読む）。
                // 食い違えばどちらかの写像が壊れているので、黙って隠さない。
                if applied != *changes {
                    log::warn!(
                        "{command}: 適用した件数 {applied:?} がエンジンの件数 {changes:?} と食い違う（ウィンドウ = {}）",
                        label.as_str(),
                    );
                }
            }
        }
    }

    IpcResult::Ok {
        data: MacroRunResponse {
            context,
            outcome: outcome_to_boundary(&outcome),
        },
    }
}

/// そのウィンドウの文書のマクロを読み、要約へ写す（一覧・保存・削除の応答が共有する実体）。
///
/// 返るのは境界の要約であり、順序は**保存順**である（一覧の提示順。要件 1.3）。
fn summaries_of(
    runtime: &MacroRuntime,
    documents: &Arc<DocumentSessions>,
    label: &WindowLabel,
) -> Result<Vec<MacroSummary>, String> {
    let records = documents
        .read(label, &mut |document| document.macros().to_vec())
        .map_err(|error| error.to_string())?;
    let domain: Vec<MacroRecord> = records.iter().map(to_domain).collect();
    Ok(runtime
        .list(&domain)
        .iter()
        .map(summary_to_boundary)
        .collect())
}

// ---------------------------------------------------------------------------
// メニューからの引き金（7.4 の登録口を通す。要件 2.1）
// ---------------------------------------------------------------------------

/// 登録元の識別子（`AcceleratorOwner`）。**本スペックの項目はこの名前空間を使う。**
const OWNER: &str = "macro-runtime";

/// 実行の項目の識別子。**アプリ全体で一意でなければならない。**
const RUN_ITEM_ID: &str = "macro-runtime.run";

/// 実行の項目の表示名。design.md の「マクロを実行」をメニューの 1 行に収めた綴りである
/// （押すと一覧から選ばせるので、末尾は省略の記号にする）。
const RUN_LABEL: &str = "実行…";

/// 実行の項目を置く部分メニュー（`menu.rs` のトップレベルの並びに従う）。
///
/// 位置の名前をここで書き写さず `menu` モジュールの定数を参照する（並びと名前の食い違いを
/// 作らない。`grid.rs` の「複製」が `編集` を参照するのと同じ形）。
fn run_menu_path() -> MenuPath {
    MenuPath::new([crate::menu::MACRO_MENU_LABEL]).expect("位置は空でない")
}

/// 実行の項目の登録内容を組み立てる（登録口へ渡す値の組み立てだけを切り出す）。
///
/// `handler` を差し替えられる形にしてあるのは、**GUI 無しで登録の受理と内容を検査できる**
/// ようにするためである（`MenuRegistry::enroll` は画面を要しない。診断の導線と同じ形）。
fn run_item_spec(handler: impl Fn(&MenuSelection) + Send + Sync + 'static) -> MenuItemSpec {
    MenuItemSpec::new(OWNER, RUN_ITEM_ID, run_menu_path(), RUN_LABEL, handler)
}

/// 実行の要求を、**活性化の対象ウィンドウ**（7.5 の振り向け）へ通知する。
///
/// メニューの処理はイベントループのスレッドで走るため、ここでブロックしてはならない。送るのは
/// 1 つのイベント（[`MACRO_RUN_REQUESTED_EVENT`]）だけで、送り先は**選ばれた時点で対象に
/// なっているウィンドウ**である。対象が無い場合（どのウィンドウもフォーカスされていない）は
/// 何もしない — 送り先が無いのに全ウィンドウへ配ると、触っていないウィンドウの画面が勝手に
/// 変わる。
///
/// **荷（ペイロード）を運ばない。** 運ぶ値が 1 つも無い（一覧の提示と選択は面の仕事である）。
fn request_run(app: &AppHandle, selection: &MenuSelection) {
    let Some(label) = selection.window().cloned() else {
        log::warn!("マクロの実行: 対象ウィンドウが無いため画面へ送らない");
        return;
    };
    match app.emit_to(label.as_str(), MACRO_RUN_REQUESTED_EVENT, ()) {
        Ok(()) => log::info!(
            "マクロの実行の要求を送った: ウィンドウ = {}",
            label.as_str(),
        ),
        Err(error) => log::error!(
            "マクロの実行の要求を送れなかった（ウィンドウ = {}）: {error}",
            label.as_str(),
        ),
    }
}

/// 起動時に 1 回だけ実行の項目を 7.4 の登録口へ登録する。`lifecycle::run` がメニューの構築
/// （[`crate::menu::install`]）の後に呼ぶ。
///
/// 登録は `MenuRegistry::register` を通すので、**ショートカットの競合と項目の識別子の重複は
/// 登録時に検出され、登録元（このモジュール）へ報告される** — 片方を黙って無効化する経路は
/// 無い。**アクセラレータは付けない**（要件はキーボードの導線を求めていない。付ければ
/// 打鍵を 1 つ奪うだけで、得るものが無い）。
///
/// 登録に失敗しても起動は続ける（項目が引けないことより、アプリが立ち上がらないことの方が
/// 悪い。グリッド・診断の導線と同じ判断）。
pub fn install(app: &AppHandle) {
    let registry = app.state::<MenuRegistry>();
    let handling_app = app.clone();
    let spec = run_item_spec(move |selection| {
        request_run(&handling_app, selection);
    });
    if let Err(error) = registry.register(app, spec) {
        log::error!("マクロの実行のメニュー項目を登録できなかった（{RUN_ITEM_ID}）: {error}");
        return;
    }
    log::info!("マクロの実行の導線をメニューへ登録した（{RUN_ITEM_ID}）");
}

/// 呼び出し元ウィンドウを境界の文脈へ写す（`grid.rs` と同じ 5 行。モジュールごとに 1 つ持つ
/// 規約であり、`caller_context` は `session/commands.rs` / `shell_cmds.rs` /
/// `diagnostics_cmds.rs` にもある）。
fn caller_context(window: &WebviewWindow) -> WindowContext {
    WindowContext {
        window: WindowLabel::new(window.label()),
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    use app_shell::ipc::{GridHistoryDirection, GridHistoryRequest, GridOpenRequest};
    use app_shell::settings::{open as open_settings, SettingsStore};
    use document_format::{CellValue, Document, DocumentFormat, DocumentFormatApi, SchemaPart};
    use document_session::SessionState;
    use schema_engine::{schema_to_text, ColumnDecl, Constraints, DeclaredKind, Schema, TypeDecl};
    use tauri::ipc::{InvokeBody, InvokeResponseBody, IpcResponse};

    use super::*;
    use crate::commands::grid::{
        answer_history, answer_open, answer_rows_window, GridSessions, WINDOW_REQUEST_HEADER_LEN,
        WINDOW_REQUEST_VERSION,
    };
    use crate::session::watch::testing::AlwaysPresent;

    /// 一時ディレクトリ（`grid.rs` のテストと同じ規律。プロセスごとに一意）。
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("時計は 1970 以降である")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "jxcel-macro-commands-{tag}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("一時ディレクトリを作れる");
            Self { path }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// 標本の宣言（`品番` は text、`数量` は int）。
    fn declaration() -> SchemaPart {
        let schema = Schema {
            columns: vec![
                ColumnDecl {
                    name: "品番".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(schema_engine::TypeKind::Text),
                        constraints: Constraints::default(),
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
                ColumnDecl {
                    name: "数量".into(),
                    ty: TypeDecl::Kind {
                        kind: DeclaredKind::Known(schema_engine::TypeKind::Int),
                        constraints: Constraints::default(),
                    },
                    required: false,
                    unique: false,
                    default: None,
                    description: None,
                },
            ],
        };
        let root = schema_to_text(&schema).expect("宣言は正準出力できる");
        SchemaPart::parse(&format!(r#"{{"root":{root},"types":[]}}"#)).expect("宣言は解析できる")
    }

    /// 2 行の標本の文書を書き、そのシートの識別子（文字列）を返す。
    fn write_document(path: &std::path::Path) -> String {
        let mut document = Document::new();
        let sheet = document.add_sheet("台帳");
        document
            .set_sheet_columns(sheet, vec!["品番".to_owned(), "数量".to_owned()])
            .expect("標本のシートは実在する");
        document
            .set_root_schema(sheet, declaration())
            .expect("標本のシートは実在する");
        for (label, quantity) in [("A", 1), ("B", 2)] {
            let row = document.add_row(sheet).expect("標本のシートは実在する");
            document
                .set_row_values(
                    sheet,
                    row,
                    vec![CellValue::Text(label.to_owned()), CellValue::Int(quantity)],
                )
                .expect("標本の行は実在する");
        }
        DocumentFormat::new()
            .save(&document, path)
            .expect("標本を保存できる");
        sheet.to_string()
    }

    /// 標本を開いた状態（表 + 文書 + ラベル + 表示中のシート）を作る。
    ///
    /// **開く経路そのものを通す**（`grid_open_sheet` の本体）— 履歴はこの保持が所有するため、
    /// マクロの適用が履歴を借りるには「開かれた」状態が要る（要件 7.1）。
    fn opened(
        tag: &str,
    ) -> (
        Scratch,
        Arc<DocumentSessions>,
        GridSessions,
        WindowLabel,
        String,
    ) {
        let scratch = Scratch::new(tag);
        let path = scratch.path.join("台帳.jxcel");
        let sheet = write_document(&path);
        let sessions = Arc::new(DocumentSessions::new());
        let label = WindowLabel::new("doc-1");
        sessions
            .resolve(&label, Some(&path))
            .expect("標本を読み込める");
        let grids = GridSessions::new(Arc::new(AlwaysPresent));
        let opened = answer_open(
            &sessions,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet.clone(),
            },
        );
        assert!(matches!(opened, IpcResult::Ok { .. }), "標本は開ける");
        (scratch, sessions, grids, label, sheet)
    }

    /// 実行基盤（テストごとに 1 つ。専用スレッドが要る）。
    fn runtime() -> MacroRuntime {
        MacroRuntime::new().expect("実行基盤を起こせる")
    }

    /// **実行の記録を数える検査を直列化する。**
    ///
    /// 記録の受け皿はプロセスに 1 つであり（`log` の facade がそういうもの）、テストは同じ
    /// バイナリで並行に走る。したがって `macro_run:` の行数を**差分で数える検査**は、他の
    /// 実行の検査が同時に走ると成立しない。実行を伴う検査はこのロックを取る（4 本。ロックの
    /// 取り合いは 1 秒の打ち切りの検査があるため、待ち時間は実行の所要に閉じる）。
    static RUN_RECORD_LOCK: Mutex<()> = Mutex::new(());

    /// 実行を伴う検査の直列化を取る（毒されていても中身を使う）。
    fn serialize_run_test() -> std::sync::MutexGuard<'static, ()> {
        RUN_RECORD_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// 標本の設定ストア（一時ディレクトリの下）。
    fn settings(scratch: &Scratch) -> Arc<FileSettingsStore> {
        let directory = scratch.path.join("settings");
        std::fs::create_dir_all(&directory).expect("設定のディレクトリを作れる");
        open_settings(&directory).expect("設定ストアを開ける").0
    }

    /// 成功の腕からデータを取り出す。
    fn data<T>(result: IpcResult<T, IpcError>) -> T {
        match result {
            IpcResult::Ok { data } => data,
            IpcResult::Err { error } => panic!("成功を期待したが失敗した: {error:?}"),
        }
    }

    /// 失敗の腕から原因を取り出す。
    fn error<T>(result: IpcResult<T, IpcError>) -> IpcError {
        match result {
            IpcResult::Ok { .. } => panic!("失敗を期待したが成功した"),
            IpcResult::Err { error } => error,
        }
    }

    /// そのウィンドウの未保存の印と版（`SessionState::Open`）。
    fn session_state(documents: &DocumentSessions, label: &WindowLabel) -> (bool, u64) {
        match documents.state(label) {
            SessionState::Open {
                unsaved, revision, ..
            } => (unsaved, revision),
            other => panic!("文書を保持していない: {other:?}"),
        }
    }

    /// 保存と削除は**閉包 1 回**で行われ、**未保存の印と版が立つ**（要件 1.1, 1.6, 1.7）。
    ///
    /// 3 つの事実を 1 本で見る: 保存で文書が変わること（印と版）、同じ名前の保存が置き換えで
    /// あること（要件 1.6）、削除が 1 件だけを落とすこと（要件 1.7）。**相手が無い名前の
    /// 削除では `edit` を呼ばない**（版を動かさない）ことも同じ材料で確かめる。
    #[test]
    fn 保存と削除は文書を変え未保存の印を立てる() {
        let (_scratch, documents, _grids, label, _sheet) = opened("store");
        let runtime = runtime();
        let source = "// @grant file.read\nexport default 1;\n";

        let before = session_state(&documents, &label);
        let stored = data(answer_store(
            &runtime,
            &documents,
            &label,
            &MacroStoreRequest {
                name: "棚卸し".to_owned(),
                kind: MacroKindTag::TypeScript,
                source: source.to_owned(),
            },
        ));
        assert_eq!(stored.stored.name, "棚卸し");
        assert!(
            stored.stored.failure.is_none(),
            "解釈できるソースに理由が付いた: {:?}",
            stored.stored.failure
        );
        assert_eq!(
            stored.stored.capabilities,
            vec![MacroCapabilityTag::FileRead],
            "宣言が要約に載っていない"
        );
        assert_eq!(stored.macros.len(), 1);

        let after_store = session_state(&documents, &label);
        assert!(after_store.0, "保存が未保存の印を立てていない");
        assert!(
            after_store.1 > before.1,
            "保存が版を進めていない（{} → {}）",
            before.1,
            after_store.1
        );

        // **ソースは保存されたまま**（要件 1.5）。
        let kept = documents
            .read(&label, &mut |document| {
                document.macros()[0].source().to_owned()
            })
            .expect("文書を読める");
        assert_eq!(kept, source, "保存でソースが変わった");

        // 同じ名前の保存は置き換えであり、一覧は 1 件のまま（要件 1.6）。
        let replaced = data(answer_store(
            &runtime,
            &documents,
            &label,
            &MacroStoreRequest {
                name: "棚卸し".to_owned(),
                kind: MacroKindTag::TypeScript,
                source: "export default 2;\n".to_owned(),
            },
        ));
        assert_eq!(replaced.macros.len(), 1, "置き換えで件数が増えた");
        assert!(replaced.macros[0].capabilities.is_empty());
        assert_eq!(
            documents
                .read(&label, &mut |document| document.macros()[0]
                    .source()
                    .to_owned())
                .expect("文書を読める"),
            "export default 2;\n"
        );

        // 2 件目を足し、**無い名前の削除では版を動かさない**。
        data(answer_store(
            &runtime,
            &documents,
            &label,
            &MacroStoreRequest {
                name: "検算".to_owned(),
                kind: MacroKindTag::JavaScript,
                source: "export default 3;\n".to_owned(),
            },
        ));
        let before_missing = session_state(&documents, &label);
        let missing = data(answer_delete(
            &runtime,
            &documents,
            &label,
            &MacroDeleteRequest {
                name: "無い名前".to_owned(),
            },
        ));
        assert!(!missing.removed, "無い名前を取り除いたと報告した");
        assert_eq!(missing.macros.len(), 2, "一覧が削除前と変わった");
        assert_eq!(
            session_state(&documents, &label),
            before_missing,
            "無い名前の削除で版か印が動いた"
        );

        // ある名前の削除は 1 件だけを落とし、版を進める（要件 1.7）。
        let removed = data(answer_delete(
            &runtime,
            &documents,
            &label,
            &MacroDeleteRequest {
                name: "棚卸し".to_owned(),
            },
        ));
        assert!(removed.removed);
        assert_eq!(removed.macros.len(), 1);
        assert_eq!(removed.macros[0].name, "検算");
        let after_delete = session_state(&documents, &label);
        assert!(after_delete.1 > before_missing.1, "削除が版を進めていない");
    }

    /// 解釈できないマクロも一覧に**理由つきで**残る（要件 1.3, 1.4）。
    ///
    /// 一覧は保存の経路（`macro_store`）で作った 2 件をそのまま読み返す — 解釈できない
    /// ソースの保存が成立し、その 1 件が理由とともに一覧に残ることを 1 本で確かめる。
    #[test]
    fn 解釈できないマクロも一覧に理由つきで残る() {
        let (_scratch, documents, _grids, label, _sheet) = opened("list");
        let runtime = runtime();

        for (name, source) in [
            ("書ける", "export default 1;\n"),
            ("書きかけ", "const x = ;\n"),
            ("綴り違い", "// @grant file.reaad\nexport default 2;\n"),
        ] {
            let stored = data(answer_store(
                &runtime,
                &documents,
                &label,
                &MacroStoreRequest {
                    name: name.to_owned(),
                    kind: MacroKindTag::TypeScript,
                    source: source.to_owned(),
                },
            ));
            if name != "書ける" {
                assert!(
                    stored.stored.failure.is_some(),
                    "{name} の解釈できない理由が要約に載っていない"
                );
            }
        }

        let listed = data(answer_list(&runtime, &documents, &label));
        assert_eq!(
            listed
                .macros
                .iter()
                .map(|summary| summary.name.as_str())
                .collect::<Vec<_>>(),
            vec!["書ける", "書きかけ", "綴り違い"],
            "一覧の順序が保存順でない、または解釈できないマクロが落ちた"
        );
        assert!(listed.macros[0].failure.is_none());
        // 構文の誤りは変換の層として、行と列をもって現れる（要件 3.4）。
        let failure = listed.macros[1]
            .failure
            .as_ref()
            .expect("書きかけに理由が付く");
        assert_eq!(failure.kind, MacroFailureTag::Transpile);
        assert!(!failure.frames.is_empty(), "構文の誤りに位置が載っていない");
        // 宣言の綴りの誤りは名前つきで現れる（要件 8.3）。
        let failure = listed.macros[2]
            .failure
            .as_ref()
            .expect("綴り違いに理由が付く");
        assert_eq!(failure.kind, MacroFailureTag::Source);
        assert!(
            failure.reason.contains("file.reaad"),
            "拒んだ能力の名前が理由に出ていない: {}",
            failure.reason
        );
        assert!(listed.macros[2].capabilities.is_empty());
    }

    /// **設定の変更が次の実行から効く**（要件 6.5）。既定はエンジンの値である。
    #[test]
    fn 設定の上限が解決され変更が次の実行から効く() {
        let (scratch, _documents, _grids, _label, _sheet) = opened("limits");
        let settings = settings(&scratch);

        // 鍵が無い = 既定（30 秒 / 512 MB。エンジンの定数が唯一の源）。
        assert_eq!(resolve_limits(&*settings), Limits::default());
        assert_eq!(
            resolve_limits(&*settings),
            Limits::new(Duration::from_secs(30), 512 * 1024 * 1024)
        );

        settings
            .set(&SettingsKey::MacroTimeLimitMs, &1_500u64)
            .expect("時間の上限を書ける");
        settings
            .set(&SettingsKey::MacroMemoryLimitBytes, &(64u64 * 1024 * 1024))
            .expect("メモリの上限を書ける");
        assert_eq!(
            resolve_limits(&*settings),
            Limits::new(Duration::from_millis(1_500), 64 * 1024 * 1024),
            "設定の値が上限に反映されていない"
        );

        // 壊れた値（別の型）は既定へ落ちる（設定は手で書けるファイルである）。
        settings
            .set(&SettingsKey::MacroTimeLimitMs, &"しばらく")
            .expect("壊れた値も保存はできる");
        assert_eq!(
            resolve_limits(&*settings).time,
            Limits::DEFAULT_TIME,
            "読めない値で既定へ落ちていない"
        );
    }

    /// **実行 1 回につき記録が 1 行増え**、変更が文書へ適用され、**画面の取り消し 1 回で戻る**
    /// （要件 2.3, 2.5, 2.6, 5.1, 7.1）。
    ///
    /// マクロは本物のホスト（`DocumentHost`）を通して実文書のセルを書き換える。**記録は
    /// テストが差し込んだ `log` の受け皿で読む**（実起動ではファイルである。`log` の
    /// facade が唯一の読み口である）。
    #[test]
    fn 実行が記録を1行残し変更が取り消し1回で戻る() {
        let (scratch, documents, grids, label, sheet) = opened("run");
        let runtime = runtime();
        let settings = settings(&scratch);
        crate::test_log::install();
        let _serialized = serialize_run_test();

        let source = format!(
            r#"
const 行 = host.readRange("{sheet}", {{ from: 0, to: 0 }}).rows[0];
host.setCells("{sheet}", [{{ row: 行.id, column: 1, value: 20 }}]);
console.log("書き換えた");
export default 行.cells[1];
"#
        );
        data(answer_store(
            &runtime,
            &documents,
            &label,
            &MacroStoreRequest {
                name: "書き換える".to_owned(),
                kind: MacroKindTag::TypeScript,
                source: source.clone(),
            },
        ));

        let before_lines = record_lines().len();
        let before_revision = session_state(&documents, &label).1;
        let response = data(answer_run(
            &runtime,
            &documents,
            &grids,
            &settings,
            &label,
            &MacroRunRequest {
                name: "書き換える".to_owned(),
            },
        ));

        match &response.outcome {
            MacroRunOutcome::Ran {
                value,
                output,
                changes,
                ..
            } => {
                assert_eq!(value, "1", "戻り値が文書から読んだ値でない");
                assert_eq!(
                    output
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>(),
                    vec!["書き換えた"],
                    "console の出力が回収されていない"
                );
                assert_eq!(changes.set_cells, 1, "セルの書き込みの件数が違う");
                assert_eq!(changes.total(), 1);
            }
            other => panic!("成功を期待したが {other:?} を返した"),
        }

        // 記録が 1 行だけ増え、必要な欄を持つ（要件 2.6）。
        let lines = record_lines();
        assert_eq!(
            lines.len(),
            before_lines + 1,
            "実行 1 回で記録が 1 行増えていない: {lines:?}"
        );
        let line = lines.last().expect("記録の行がある");
        for field in [
            "ウィンドウ = doc-1",
            "マクロ = 書き換える",
            "種別 = typescript",
            "結果 = ran",
            "打ち切り = (なし)",
            "変更 = 1",
            "所要 = ",
        ] {
            assert!(line.contains(field), "記録の行に `{field}` が無い: {line}");
        }
        assert!(
            !line.contains("host.setCells"),
            "記録にソースが出ている（要件 8.6 の規律）: {line}"
        );

        // 文書へ適用され、版が進んでいる（要件 5.1）。
        assert!(
            session_state(&documents, &label).1 > before_revision,
            "適用が版を進めていない"
        );
        let written = documents
            .read(&label, &mut |document| {
                document.sheets()[0].rows()[0].values()[1].clone()
            })
            .expect("文書を読める");
        assert_eq!(written, CellValue::Int(20), "書き込みが文書へ届いていない");

        // **画面の取り消し 1 回で全部戻る**（要件 7.1）。履歴はグリッドの保持が所有しており、
        // マクロの適用が積んだ 1 対をそのままの入口から戻せる。
        let undone = data(answer_history(
            &documents,
            &grids,
            &label,
            &GridHistoryRequest {
                direction: GridHistoryDirection::Undo,
            },
        ));
        assert!(undone.outcome.is_some(), "取り消す操作が積まれていない");
        let restored = documents
            .read(&label, &mut |document| {
                document.sheets()[0].rows()[0].values()[1].clone()
            })
            .expect("文書を読める");
        assert_eq!(
            restored,
            CellValue::Int(1),
            "取り消し 1 回でマクロの変更が戻っていない"
        );
    }

    /// 失敗したマクロは**何も適用せず**、記録には `failed` が残る（要件 2.4, 6.3, 7.3, 9.1）。
    #[test]
    fn 失敗したマクロは文書を変えない() {
        let (scratch, documents, grids, label, _sheet) = opened("failed");
        let runtime = runtime();
        let settings = settings(&scratch);
        crate::test_log::install();
        let _serialized = serialize_run_test();

        data(answer_store(
            &runtime,
            &documents,
            &label,
            &MacroStoreRequest {
                name: "落ちる".to_owned(),
                kind: MacroKindTag::TypeScript,
                source: "const x: number = 1;\nthrow new Error(\"わざと\");\nexport default x;\n"
                    .to_owned(),
            },
        ));

        let before = documents
            .read(&label, &mut |document| document.sheets()[0].rows().len())
            .expect("文書を読める");
        let before_revision = session_state(&documents, &label).1;
        let response = data(answer_run(
            &runtime,
            &documents,
            &grids,
            &settings,
            &label,
            &MacroRunRequest {
                name: "落ちる".to_owned(),
            },
        ));
        match &response.outcome {
            MacroRunOutcome::Failed { failure } => {
                assert_eq!(failure.kind, MacroFailureTag::Execution);
                assert!(
                    failure.reason.contains("わざと"),
                    "例外の理由が出ていない: {}",
                    failure.reason
                );
                assert!(!failure.frames.is_empty(), "例外の位置が出ていない");
            }
            other => panic!("失敗を期待したが {other:?} を返した"),
        }
        assert_eq!(
            session_state(&documents, &label).1,
            before_revision,
            "失敗した実行が文書の版を動かした"
        );
        assert_eq!(
            documents
                .read(&label, &mut |document| document.sheets()[0].rows().len())
                .expect("文書を読める"),
            before
        );
        let line = record_lines().pop().expect("記録の行がある");
        assert!(
            line.contains("結果 = failed"),
            "記録が失敗を運んでいない: {line}"
        );
    }

    /// 名前が文書に無い実行は**経路の失敗**であり、記録を残さない（要件 2.1 の前提）。
    #[test]
    fn 文書に無い名前の実行は経路の失敗になる() {
        let (scratch, documents, grids, label, _sheet) = opened("missing");
        let runtime = runtime();
        let settings = settings(&scratch);
        crate::test_log::install();
        let _serialized = serialize_run_test();

        let before = record_lines().len();
        let failure = error(answer_run(
            &runtime,
            &documents,
            &grids,
            &settings,
            &label,
            &MacroRunRequest {
                name: "無い".to_owned(),
            },
        ));
        assert!(
            matches!(failure, IpcError::Document { .. }),
            "経路の失敗でない: {failure:?}"
        );
        assert_eq!(
            record_lines().len(),
            before,
            "実行が始まっていないのに記録が増えた"
        );
    }

    /// **設定の上限が実行へ渡る**（要件 6.1, 6.2, 6.5）。
    ///
    /// 終わらない繰り返しを持つマクロを、設定で下げた時間の上限（1 秒）で打ち切る —
    /// 打ち切りの種類が `Aborted { Time }` として境界に現れる。
    #[test]
    fn 設定の時間の上限が実行を打ち切る() {
        let (scratch, documents, grids, label, _sheet) = opened("abort");
        let runtime = runtime();
        let settings = settings(&scratch);
        crate::test_log::install();
        let _serialized = serialize_run_test();

        data(answer_store(
            &runtime,
            &documents,
            &label,
            &MacroStoreRequest {
                name: "終わらない".to_owned(),
                kind: MacroKindTag::JavaScript,
                source: "while (true) {}\n".to_owned(),
            },
        ));
        settings
            .set(&SettingsKey::MacroTimeLimitMs, &1_000u64)
            .expect("時間の上限を書ける");

        let started = Instant::now();
        let response = data(answer_run(
            &runtime,
            &documents,
            &grids,
            &settings,
            &label,
            &MacroRunRequest {
                name: "終わらない".to_owned(),
            },
        ));
        match &response.outcome {
            MacroRunOutcome::Aborted { limit, .. } => {
                assert_eq!(*limit, MacroAbortKind::Time, "打ち切りの種類が違う");
            }
            other => panic!("打ち切りを期待したが {other:?} を返した"),
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "既定の 30 秒まで待っている（設定の上限が効いていない）"
        );
    }

    /// 表の窓の要求の引数を組み立てる（`grid_rows_window` の本体へ渡す生バイト）。
    ///
    /// **配置は `grid.rs` のモジュール docs「引数の配置」が唯一の源である**（ここにあるのは
    /// 検査の材料であり、本番の符号化はフロントエンド（7.3）が持つ）。
    fn window_argument(sheet: &str, generation: u64, start: u64, count: u64) -> Vec<u8> {
        let mut argument = Vec::with_capacity(WINDOW_REQUEST_HEADER_LEN + sheet.len());
        argument.push(WINDOW_REQUEST_VERSION);
        argument.extend_from_slice(&generation.to_le_bytes());
        argument.extend_from_slice(&start.to_le_bytes());
        argument.extend_from_slice(&count.to_le_bytes());
        argument.extend_from_slice(&(sheet.len() as u64).to_le_bytes());
        argument.extend_from_slice(sheet.as_bytes());
        argument
    }

    /// **実行中でも表の操作は止まらない**（要件 2.2）。
    ///
    /// 長いマクロを実行している間に、表の窓（`grid_rows_window` の本体。スクロールと描画が
    /// 呼ぶ読み）と**文書の操作**（行の追加）が通ることを確かめる。通らなければ、実行の間ずっと
    /// **保持のロック**（`GridSessions` のウィンドウごとの保持）が握られていることになり、
    /// 要件 2.2 が破れている。
    ///
    /// # なぜこの検査が本モジュールにあるのか（探す人へ）
    ///
    /// 規律そのものは `commands::grid` の
    /// [`GridSessions::with_displayed`](crate::commands::grid::GridSessions::with_displayed) が
    /// 持つ（「実行の間は保持のロックを握らない」。あちらの doc に理由がある）が、**検査は
    /// 実行の本体（[`answer_run`]）を駆動しなければならない** — 実行の最中に窓が返ることの
    /// 観測は、実行と表の両方を呼ぶ側でしか作れない。
    ///
    /// 加えて、`answer_run` は実行 1 回につき `macro_run:` の記録を 1 行出す。同じ `mod tests`
    /// の 3 本（記録の行数を数える・最後の行を見る）は**グローバルの受け皿の行数を数える**ため、
    /// [`serialize_run_test`] のロックで直列化している。**そのロックを取れるのは本モジュールの
    /// テストだけである**（`RUN_RECORD_LOCK` は `mod tests` の内側にある）。規律の検査を
    /// `grid.rs` のテストへ置くと、実行の記録が兄弟の数える窓に落ちてまれに落ちる — 決定的で
    /// あることを優先し、実行の隣に置く。
    ///
    /// # 待ち合わせは観測に基づく（時間に依存しない）
    ///
    /// * 実行が始まったことは [`MacroRuntime::is_running`] で観測する（実行の要求が受け取られ
    ///   た時点で真になり、実行の終わりに偽へ戻る）。検査が要るのは「実行が進行中である」
    ///   ことだけである
    /// * マクロは**自分の期限まで**動き続ける（数秒。エンジンの打ち切りには掛からない長さで
    ///   ある）。**マクロの側の合図で止める形は取らない** — 実行の開始の観測は isolate の
    ///   生成より前に真になる（要求の受け取りが先である）ため、検査が「実行が始まった」と
    ///   見てから文書を変えても、マクロの最初の読みより前になることがある（観測した: 最初の
    ///   読みが編集の後に来て、マクロが編集を見ない）。検査は観測（進行中であること・窓が
    ///   進行中に返ること）だけに基づかせ、端の競合を持ち込まない
    ///
    /// 表明は 3 つである: (1) 実行が進行中であること、(2) 窓が**進行中のまま**短い時間で
    /// 返ること（保持のロックを握っていれば実行の終わりまで待たされる）、(3) 実行の最中に
    /// 文書の操作が通ること。
    #[test]
    fn 実行中でも表の窓は止まらない() {
        /// マクロが動き続ける長さ（ミリ秒）。**エンジンの打ち切り（既定 30 秒）より遥かに
        /// 短く、検査が窓を要求するまでの時間（ミリ秒未満）より遥かに長い**。表明がこの値に
        /// 依存しないことは上の doc のとおりである（値が変えるのは、規律が壊れているときに
        /// 検査が待たされる長さだけである）。
        const 動き続けるミリ秒: u64 = 2_000;

        /// 実行の最中の窓の要求に許す時間。窓は数行を読むだけであり、実測は数十マイクロ秒で
        /// ある（規律が壊れているときは実行の終わり＝上のミリ秒まで待たされる）。
        const 窓の許容: Duration = Duration::from_millis(1_000);

        let (scratch, documents, grids, label, sheet) = opened("lock");
        let runtime = runtime();
        let settings = settings(&scratch);
        crate::test_log::install();
        // 実行の記録を数える 3 本と直列化する（上記「なぜこの検査が本モジュールにあるのか」）。
        let _serialized = serialize_run_test();

        // 窓の要求は**いまの世代**を名乗らなければならない（古い世代には空の窓が返る）。世代の
        // 源は `GridSession::generation()` 1 つであり、開いた応答がその 10 進表現を運ぶ。
        let opened = data(answer_open(
            &documents,
            &grids,
            &label,
            &GridOpenRequest {
                sheet: sheet.clone(),
            },
        ));
        let generation: u64 = opened
            .generation
            .parse()
            .expect("世代は 10 進の文字列である");

        // 実行するマクロ: 標本のシートを 1 行読み、**数秒動き続けて**から結果を返す。
        // 戻り値は標本から読んだ内容そのものであり、標本が読めたことの表明を兼ねる。
        data(answer_store(
            &runtime,
            &documents,
            &label,
            &MacroStoreRequest {
                name: "長く動く".to_owned(),
                kind: MacroKindTag::TypeScript,
                source: format!(
                    "const sheet = (await host.sheets())[0];\n\
                     const page = await host.readRange(sheet.id, {{ from: 0, to: 0 }});\n\
                     const 期限 = Date.now() + {動き続けるミリ秒};\n\
                     while (Date.now() < 期限) {{}}\n\
                     export default `${{sheet.name}}/${{page.rows.length}}`;\n"
                ),
            },
        ));

        // 実行は別のスレッドで始める（本スレッドは表を操作し続ける側である）。
        thread::scope(|scope| {
            let running = scope.spawn(|| {
                answer_run(
                    &runtime,
                    &documents,
                    &grids,
                    &settings,
                    &label,
                    &MacroRunRequest {
                        name: "長く動く".to_owned(),
                    },
                )
            });

            // **実行が始まったことを観測する**（表を触るのはこの観測の後である）。
            let waited = Instant::now();
            while !runtime.is_running() {
                assert!(
                    waited.elapsed() < Duration::from_secs(10),
                    "実行が始まったことを観測できない"
                );
                thread::sleep(Duration::from_millis(1));
            }

            // (1)(2) 実行の最中に、表の窓を要求する（`grid_rows_window` の本体）。
            let requested = Instant::now();
            let response = answer_rows_window(
                &documents,
                &grids,
                &label,
                &InvokeBody::Raw(window_argument(&sheet, generation, 0, 3)),
            );
            let elapsed = requested.elapsed();
            assert!(
                runtime.is_running(),
                "表の窓が返った時点で実行が終わっている（実行の間、保持のロックを握っている）"
            );
            assert!(
                elapsed < 窓の許容,
                "実行の最中の表の窓に {elapsed:?} かかった（実行の間、保持のロックを握っている）"
            );
            let bytes = match response.body().expect("生バイトの応答は常に作れる") {
                InvokeResponseBody::Raw(bytes) => bytes,
                InvokeResponseBody::Json(text) => panic!("封筒が返った: {text}"),
            };
            assert!(!bytes.is_empty(), "実行の最中に表の窓が空で返った");

            // (3) 実行の最中に、文書の操作（行の追加）が通る（要件 2.2 の「操作を止めない」）。
            let before = documents
                .read(&label, &mut |document| document.sheets()[0].rows().len())
                .expect("文書を読める");
            let sheet_id = documents
                .read(&label, &mut |document| document.sheets()[0].id())
                .expect("文書を読める");
            documents
                .edit(&label, &mut |document| document.add_row(sheet_id))
                .expect("標本のシートに行を足せる");
            let after = documents
                .read(&label, &mut |document| document.sheets()[0].rows().len())
                .expect("文書を読める");
            assert_eq!(after, before + 1, "実行の最中に文書の操作が通っていない");

            match running.join().expect("実行のスレッドは完走する") {
                IpcResult::Ok { data } => match &data.outcome {
                    MacroRunOutcome::Ran { value, .. } => {
                        // 戻り値の提示は JSON である（要件 2.3）ため、文字列は引用符つきで返る。
                        assert_eq!(value, "\"台帳/1\"", "マクロが標本のシートを読めていない");
                    }
                    other => panic!("成功を期待したが {other:?} を返した"),
                },
                IpcResult::Err { error } => panic!("実行が失敗した: {error:?}"),
            }
        });
    }

    /// メニューの項目の登録内容（位置・表示名・識別子・アクセラレータ無し）。
    #[test]
    fn 実行の項目の内容が登録の契約どおりである() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let spec = run_item_spec(move |_selection| {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        let registry = MenuRegistry::new();
        assert!(registry.enroll(spec).is_ok(), "登録できる");
        let model = registry.model();
        // 位置はモデルを降りて確かめる（**トップレベルの部分メニューは「マクロ」**）。
        let submenu = model
            .top()
            .iter()
            .find(|submenu| submenu.label() == crate::menu::MACRO_MENU_LABEL)
            .expect("マクロの部分メニューが無い");
        let node = submenu
            .children()
            .iter()
            .find_map(|child| match child {
                crate::menu::MenuNode::Item(item) if item.item().as_str() == RUN_ITEM_ID => {
                    Some(item)
                }
                crate::menu::MenuNode::Item(_) | crate::menu::MenuNode::Submenu(_) => None,
            })
            .expect("実行の項目がマクロの部分メニューに無い");
        assert_eq!(node.label(), RUN_LABEL);
        assert_eq!(node.owner().as_str(), OWNER);
        assert!(node.accelerator().is_none(), "打鍵を奪っている");
    }

    /// 4 本のコマンド関数の形を**コンパイル時に固定する**（`grid.rs` と同じ形）。
    ///
    /// 見るのは「封筒（[`IpcResult`]）を返すこと」と「引数の型」である — 面（4.4）と権限
    /// （`src-tauri/permissions/app.toml`）がこの形に依存している。とくに [`macro_run`] が
    /// **非同期である**ことは実行モデルの契約である（`MacroActor::run` は同期であり、
    /// `spawn_blocking` の上でだけ呼べる。モジュール doc「実行モデル」）。
    #[test]
    fn コマンドの形が契約どおりである() {
        fn assert_run<F, Fut>(_: F)
        where
            F: FnOnce(AppHandle, WebviewWindow, MacroRunRequest) -> Fut + Send + 'static,
            Fut: Future<Output = IpcResult<MacroRunResponse, IpcError>>,
        {
        }

        let _: fn(AppHandle, WebviewWindow) -> IpcResult<MacroListResponse, IpcError> = macro_list;
        let _: fn(
            AppHandle,
            WebviewWindow,
            MacroStoreRequest,
        ) -> IpcResult<MacroStoreResponse, IpcError> = macro_store;
        let _: fn(
            AppHandle,
            WebviewWindow,
            MacroDeleteRequest,
        ) -> IpcResult<MacroDeleteResponse, IpcError> = macro_delete;
        assert_run(macro_run);
    }

    /// `log` の受け皿（**記録の読み口**）。
    ///
    /// 実起動では記録は `tauri-plugin-log` がファイルへ書く。単体テストにはその機構が無いので、
    /// テストが `log` の面へ受け皿を取り付け、**書かれた行をそのまま読む**（行数を数える検査と、
    /// 行の中身を見る検査の両方が、同じ 1 本の口を通る）。
    ///
    /// **受け皿は `crate::test_log` に 1 つだけ置く**（`log` の面に取り付けられる記録器は
    /// 1 プロセスに 1 つだけである。自前のものをもう 1 つ取り付けようとすると、先に取った方が
    /// 勝ち、後から取った方は空を読む — `sidecar_host` の検査がこれで落ちた）。
    fn record_lines() -> Vec<String> {
        crate::test_log::lines_containing("macro_run:")
    }
}

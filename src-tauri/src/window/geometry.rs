//! ウィンドウの位置とサイズの記憶と復元 — 直近に閉じられたウィンドウの位置とサイズを
//! 記憶し、次に開くウィンドウの初期値に使う。
//!
//! 所有: ウィンドウ形状の永続化（design.md「Components and Interfaces → WindowManager」）。
//! 要件: 2.7。
//!
//! # 保存する値と置き場所
//!
//! 保存するのは **[`StoredGeometry`] ただ 1 つ**であり、4.1 / 4.2 が用意した設定ストアの
//! 既存のキー `SettingsKey::WindowGeometry`（`"window.geometry"`）へ載せる。
//!
//! | フィールド | 型 | 意味 |
//! |---|---|---|
//! | `x` | 整数 | ウィンドウ枠の左上の X 座標（**物理**ピクセル） |
//! | `y` | 整数 | ウィンドウ枠の左上の Y 座標（**物理**ピクセル） |
//! | `width` | 整数 | クライアント領域の幅（**物理**ピクセル） |
//! | `height` | 整数 | クライアント領域の高さ（**物理**ピクセル） |
//!
//! JSON ではこの 4 つをフィールドに持つ**オブジェクト**になる（design.md「Logical Data
//! Model」が `window.geometry` の型を「オブジェクト」と定めている）。
//!
//! **ラベル規約（`doc-<連番>` / `empty-<連番>`）をキーにしない。** ラベルをキーにすると
//! 設定がウィンドウの数だけ無限に増えるうえ、次に開くウィンドウへ復元すべき値がどれか
//! 決まらない — research.md「ウィンドウ管理と複数ウィンドウ」が `tauri-plugin-window-state`
//! を見送った理由がこれである。本モジュールは**常に 1 つの値**だけを永続化する。
//!
//! 物理ピクセルを使うのは、Tauri の取得系（`outer_position` / `inner_size`）とモニタの
//! 境界（`available_monitors`）がどちらも物理ピクセルだからである。論理ピクセルへの変換は
//! 復元のときに 1 回だけ行う（[`to_initial`]）。**値が無い・壊れている・解釈できないときは
//! すべて既定値に落ちる**（4.2 の復旧規則。決して致命的にしない）。
//!
//! # いつ保存するか — 閉じた時点
//!
//! **保存はウィンドウを閉じた時点で行う**（design.md「WindowManager」）。終了イベント
//! （`RunEvent::Exit`）だけに頼ると、クラッシュ・強制終了・単一インスタンスへの引き継ぎなど
//! **通常の終了経路以外で失われる**。research.md が `tauri-plugin-window-state` を見送った
//! 理由もこれである（同プラグインのディスク書き込みは `RunEvent::Exit` のときだけ）。
//! 本モジュールは `WindowEvent::Destroyed` で設定ストアへ書き込む。`Destroyed` は最後の
//! ウィンドウを閉じたときにも、他のウィンドウを残したまま 1 枚だけ閉じたときにも届き、
//! 書き込みはプロセスが終わる前に同期して完了する。したがって**ウィンドウを閉じた後に
//! クラッシュしても値は残る**。
//!
//! # なぜ「閉じた後」ではなく「閉じる前」に測るのか
//!
//! `Destroyed` の時点でネイティブウィンドウは既に無いため、そこから `outer_position()` /
//! `inner_size()` を読むことはできない。そこで**ウィンドウが生きている間に観測した形状**を
//! `WindowEvent::Moved` / `WindowEvent::Resized` / `CloseRequested` で [`OBSERVED`] に
//! 溜め、`Destroyed` ではその写像を書くだけにする。
//!
//! この方式は終了拒否の仲介（7.6）とも噛み合う。7.6 は拒否が解除された後に `destroy()` で
//! 閉じるが、**`destroy()` は `CloseRequested` を再発火しない**（research.md「ウィンドウを
//! 閉じる操作の拒否」）。移動・拡大縮小の観測を `Destroyed` の直前まで持ち越すなら、
//! `CloseRequested` を経ない経路でも「そのウィンドウの最後の形状」が保存される。また、
//! `CloseRequested` の時点で書き込むと、**拒否されてそのウィンドウが閉じなかった場合まで
//! 記憶してしまう**ので、観測だけに留めて書き込みは `Destroyed` まで待つ。
//!
//! [`OBSERVED`] はラベルごとに持つ（複数のウィンドウが開いているとき、`Destroyed` の
//! ラベルから「直近に閉じられたのはどのウィンドウか」を特定する必要がある）。**これは
//! プロセス内の一時的な写像であり、永続化される値は変わらず 1 つである。**
//!
//! # どのウィンドウが復元するか
//!
//! **新しく開くすべてのウィンドウ**が、直近に閉じられたウィンドウの形状を初期値に使う
//! （要件 2.7「次に開くウィンドウの初期値」）。1 ウィンドウ 1 ドキュメントの規約では
//! ドキュメントを開くたびに新しいウィンドウが現れるため、最初の 1 枚だけに限ると規約と
//! 噛み合わない。
//!
//! # 画面外の位置
//!
//! モニタが取り外された後などに、保存された位置が今のどのモニタにも重ならないことがある。
//! そのまま使うと**見えないウィンドウ**が開くので、[`plan_restore`] は「矩形が少なくとも
//! [`MIN_VISIBLE_EXTENT`] 平方ピクセル、どこかのモニタに重なる」ことを要求し、満たさなければ
//! **位置だけを捨てて既定の位置に任せ、大きさだけ復元する**（[`RestorePlan::SizeOnly`]）。
//! モニタを列挙できない場合も位置を信用しない（同じ理由）。

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use app_shell::settings::{SettingsKey, SettingsStore};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, WebviewWindow, Window, WindowEvent};
use tauri_plugin_log::log;

use crate::lifecycle::StartupState;

// ---------------------------------------------------------------------------
// 保存する値（要件 2.7。design.md「Logical Data Model」の `window.geometry`）
// ---------------------------------------------------------------------------

/// 設定ストアに載る唯一の形状値。フィールドの意味はモジュール doc の表にある。
///
/// すべてのフィールドを必須にする（1 つでも欠けたら「その値は壊れている」と見なして
/// 既定値に落ちる）。**未知のフィールドは無視する** — 将来この値に情報を足しても、
/// 古い版が書いた値を新しい版が読めるようにするためである。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct StoredGeometry {
    /// ウィンドウ枠の左上の X 座標（物理ピクセル）。
    pub(super) x: i32,
    /// ウィンドウ枠の左上の Y 座標（物理ピクセル）。
    pub(super) y: i32,
    /// クライアント領域の幅（物理ピクセル）。
    pub(super) width: u32,
    /// クライアント領域の高さ（物理ピクセル）。
    pub(super) height: u32,
}

impl StoredGeometry {
    /// 画面外判定に使う矩形。幅と高さはクライアント領域のもので、枠の分だけ実際より小さい
    /// （＝重なりを少なく見積もる）が、判定を安全側に倒すだけなので許容する。
    fn rect(self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

// ---------------------------------------------------------------------------
// 復元の計画（純粋な部分。GUI 無しでテストできる）
// ---------------------------------------------------------------------------

/// 復元を拒否する大きさの下限（物理ピクセル）。`0` は明らかに壊れた値である。
const MIN_RESTORABLE_DIMENSION: u32 = 1;

/// 復元を拒否する大きさの上限（物理ピクセル）。
///
/// 8K のモニタ（7680×4320）に十分な余裕を与えつつ、`u32` の極端な値（壊れた値）を弾く。
const MAX_RESTORABLE_DIMENSION: u32 = 32_768;

/// 位置を復元するために要求する、ウィンドウとモニタの最小の重なり（物理ピクセル、両軸）。
///
/// 64 は「つかめる角が残る」最小限として選んだ。これを下回る重なりしか無い位置は実質的に
/// 見えないので、位置だけを捨てて既定の位置に任せる。
const MIN_VISIBLE_EXTENT: u64 = 64;

/// 画面の矩形（物理ピクセル）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rect {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: u32,
    pub(super) height: u32,
}

/// モニタ 1 台の境界と拡大率。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct MonitorArea {
    /// モニタの境界（物理ピクセル）。
    pub(super) rect: Rect,
    /// 論理ピクセルと物理ピクセルを相互に写す拡大率。
    pub(super) scale_factor: f64,
}

/// 保存値から導いた復元の計画。**純粋関数 [`plan_restore`] の出力である。**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RestorePlan {
    /// 位置と大きさの両方を保存値から与える。
    PositionAndSize(StoredGeometry),
    /// 位置は使えない（画面外・モニタ不明）。既定の位置に任せ、大きさだけ保存値から与える。
    SizeOnly {
        width: u32,
        height: u32,
    },
    /// 復元できる値が無い（未保存・壊れた値）。生成側の既定値のままにする。
    Default,
}

/// 生成に渡す初期値（論理ピクセル）。`None` の項目は生成側の既定値のままにする。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) struct InitialGeometry {
    position: Option<(f64, f64)>,
    size: Option<(f64, f64)>,
}

impl InitialGeometry {
    /// 復元する位置（論理ピクセル）。無ければ生成側の既定の位置に任せる。
    pub(super) fn position(self) -> Option<(f64, f64)> {
        self.position
    }

    /// 復元する大きさ（論理ピクセル）。無ければ生成側の既定の大きさを使う。
    pub(super) fn size(self) -> Option<(f64, f64)> {
        self.size
    }
}

/// 保存値とモニタの一覧から復元の計画を立てる。**純粋関数**（GUI も `AppHandle` も要らない）。
///
/// 規則は 3 つである:
///
/// 1. 保存値が無い → [`RestorePlan::Default`]
/// 2. 大きさが壊れている（[`MIN_RESTORABLE_DIMENSION`] 未満、または
///    [`MAX_RESTORABLE_DIMENSION`] 超）→ [`RestorePlan::Default`]
/// 3. 位置が今のどのモニタにも [`MIN_VISIBLE_EXTENT`] 平方ピクセル以上重ならない
///    （＝モニタが取り外された等）→ [`RestorePlan::SizeOnly`]。モニタの一覧が空の場合も
///    同じ（位置を検証できないものは信用しない）
pub(super) fn plan_restore(
    stored: Option<StoredGeometry>,
    monitors: &[MonitorArea],
) -> RestorePlan {
    let Some(geometry) = stored else {
        return RestorePlan::Default;
    };
    if !usable_dimension(geometry.width) || !usable_dimension(geometry.height) {
        return RestorePlan::Default;
    }
    let on_screen = monitors.iter().any(|monitor| {
        let (width, height) = overlap_extent(geometry.rect(), monitor.rect);
        width >= MIN_VISIBLE_EXTENT && height >= MIN_VISIBLE_EXTENT
    });
    if on_screen {
        RestorePlan::PositionAndSize(geometry)
    } else {
        RestorePlan::SizeOnly {
            width: geometry.width,
            height: geometry.height,
        }
    }
}

/// 復元計画を論理ピクセルの初期値へ写す。**純粋関数**。
///
/// `scale` は「その値を使うモニタ」の拡大率である（位置が使える場合は重なりが最大のモニタ、
/// 位置が使えない場合は主モニタ）。拡大率が壊れている（非有限・0 以下）場合は 1.0 と見なす。
fn to_initial(plan: RestorePlan, scale: f64) -> InitialGeometry {
    match plan {
        RestorePlan::Default => InitialGeometry::default(),
        RestorePlan::SizeOnly { width, height } => InitialGeometry {
            position: None,
            size: Some((logical(width, scale), logical(height, scale))),
        },
        RestorePlan::PositionAndSize(geometry) => InitialGeometry {
            position: Some((
                logical_signed(geometry.x, scale),
                logical_signed(geometry.y, scale),
            )),
            size: Some((logical(geometry.width, scale), logical(geometry.height, scale))),
        },
    }
}

/// 復元に使える大きさか。
fn usable_dimension(value: u32) -> bool {
    (MIN_RESTORABLE_DIMENSION..=MAX_RESTORABLE_DIMENSION).contains(&value)
}

/// 2 つの矩形の重なりの幅・高さ（重なりが無ければ 0）。**`i64` で計算する** — `i32` の
/// 位置に `u32` の大きさを足すため、`i64` に上げないと桁あふれしうる。
fn overlap_extent(a: Rect, b: Rect) -> (u64, u64) {
    let (ax0, ay0) = (i64::from(a.x), i64::from(a.y));
    let (ax1, ay1) = (ax0 + i64::from(a.width), ay0 + i64::from(a.height));
    let (bx0, by0) = (i64::from(b.x), i64::from(b.y));
    let (bx1, by1) = (bx0 + i64::from(b.width), by0 + i64::from(b.height));
    let width = (ax1.min(bx1) - ax0.max(bx0)).max(0) as u64;
    let height = (ay1.min(by1) - ay0.max(by0)).max(0) as u64;
    (width, height)
}

/// 壊れた拡大率を 1.0 に寄せる（`0` 除算と NaN を生成側へ持ち込まない）。
fn sane_scale(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

/// 物理ピクセルの大きさを論理ピクセルへ写す。
fn logical(physical: u32, scale_factor: f64) -> f64 {
    f64::from(physical) / sane_scale(scale_factor)
}

/// 物理ピクセルの座標を論理ピクセルへ写す（負の座標を保つ）。
fn logical_signed(physical: i32, scale_factor: f64) -> f64 {
    f64::from(physical) / sane_scale(scale_factor)
}

// ---------------------------------------------------------------------------
// 解像度・モニタ（アダプタ層の実測。ここだけが `AppHandle` を要する）
// ---------------------------------------------------------------------------

/// 保存値とモニタの一覧から、生成に渡す初期値を組み立てる。
///
/// **新しく開くすべてのウィンドウ**がこれを呼ぶ（どのウィンドウかを区別しない）。
///
/// 保存値の読み取り・モニタの列挙・計画の立案はどれも失敗しても致命的でない（既定値に落ちる）。
pub(super) fn initial_geometry<R: Runtime>(app: &AppHandle<R>) -> InitialGeometry {
    let store = app.state::<StartupState>().settings().clone();
    let monitors = monitor_areas(app);
    let plan = plan_restore(read_geometry(&*store), &monitors);
    // 位置が使えるときは「重なりが最大のモニタ」の拡大率、位置が使えないときは主モニタの
    // 拡大率を使う（その値が実際に現れる画面に合わせる）。
    let scale = match plan {
        RestorePlan::PositionAndSize(geometry) => best_monitor(&geometry, &monitors)
            .map(|monitor| monitor.scale_factor)
            .unwrap_or_else(|| fallback_scale(app, &monitors)),
        RestorePlan::SizeOnly { .. } => fallback_scale(app, &monitors),
        RestorePlan::Default => return InitialGeometry::default(),
    };
    to_initial(plan, scale)
}

/// 今あるモニタの一覧を画面矩形へ写す。**列挙できないときは空を返す** — 位置を検証できない
/// 状態で保存された位置を使うと、見えないウィンドウが開きうるためである（[`plan_restore`] が
/// 空の一覧を「位置は使えない」と解釈する）。
fn monitor_areas<R: Runtime>(app: &AppHandle<R>) -> Vec<MonitorArea> {
    match app.available_monitors() {
        Ok(monitors) => monitors
            .iter()
            .map(|monitor| MonitorArea {
                rect: Rect {
                    x: monitor.position().x,
                    y: monitor.position().y,
                    width: monitor.size().width,
                    height: monitor.size().height,
                },
                scale_factor: monitor.scale_factor(),
            })
            .collect(),
        Err(error) => {
            log::warn!("モニタを列挙できなかったので保存された位置を使わない: {error}");
            Vec::new()
        }
    }
}

/// 保存値の矩形と最も大きく重なるモニタ。無ければ `None`。
fn best_monitor<'a>(
    geometry: &StoredGeometry,
    monitors: &'a [MonitorArea],
) -> Option<&'a MonitorArea> {
    monitors.iter().max_by_key(|monitor| {
        let (width, height) = overlap_extent(geometry.rect(), monitor.rect);
        width.saturating_mul(height)
    })
}

/// 位置が使えないときに大きさの写像へ使う拡大率（主モニタ → 先頭のモニタ → 1.0）。
fn fallback_scale<R: Runtime>(app: &AppHandle<R>, monitors: &[MonitorArea]) -> f64 {
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| monitor.scale_factor())
        .or_else(|| monitors.first().map(|monitor| monitor.scale_factor))
        .unwrap_or(1.0)
}

// ---------------------------------------------------------------------------
// 観測（ウィンドウが生きている間に形状を溜める）
// ---------------------------------------------------------------------------

/// 1 枚のウィンドウについて最後に観測した形状。
///
/// `Moved` と `Resized` は別々に届くので、位置と大きさは独立に埋まる。両方が揃うまでは
/// 保存できない（[`Observation::geometry`] が `None` を返す）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Observation {
    position: Option<(i32, i32)>,
    size: Option<(u32, u32)>,
}

impl Observation {
    /// 両方が揃っていれば保存できる値になる。
    fn geometry(self) -> Option<StoredGeometry> {
        let (x, y) = self.position?;
        let (width, height) = self.size?;
        Some(StoredGeometry {
            x,
            y,
            width,
            height,
        })
    }
}

/// 生きているウィンドウごとの最後の観測（プロセス内の一時的な写像）。
///
/// **永続化されるのは [`StoredGeometry`] ただ 1 つである。**この写像は「`Destroyed` の
/// ラベルがどのウィンドウの形状を指すか」を特定するためだけに存在し、`Destroyed` のたびに
/// その項目を取り除く（ウィンドウが生きている間だけの記憶なので、漏れない）。
static OBSERVED: LazyLock<Mutex<HashMap<String, Observation>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// [`OBSERVED`] のロックを取る。毒されていても panic しない（破棄の通知はイベントループの
/// 中で走るため、ここで panic すると他のウィンドウを巻き込む）。
fn observed() -> MutexGuard<'static, HashMap<String, Observation>> {
    OBSERVED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// ウィンドウのイベントに応じて観測と保存を行う（`Builder::on_window_event` から呼ぶ）。
///
/// - `Moved` / `Resized`: そのウィンドウの最後の形状を更新する（ウィンドウは生きている）。
/// - `CloseRequested`: 閉じる直前の確定した形状を観測する。**ここでは保存しない** —
///   7.6 の拒否でこのウィンドウが閉じないことがあるためである。
/// - `Destroyed`: 保存する。ネイティブウィンドウは既に無いので観測済みの値を使う。
///
/// それ以外のイベントは何もしない（要件 2.7 に関係しない）。
pub(super) fn observe<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    match event {
        WindowEvent::Moved(position) => {
            remember(window.label(), Some((position.x, position.y)), None);
        }
        WindowEvent::Resized(size) => {
            remember(window.label(), None, Some((size.width, size.height)));
        }
        WindowEvent::CloseRequested { .. } => observe_live(window),
        WindowEvent::Destroyed => persist(window),
        _ => {}
    }
}

/// 生成が完了したウィンドウの形状を最初に観測する（`Moved` / `Resized` が届かないまま
/// 閉じられても、少なくとも生成時の大きさを引き継げるようにする）。
pub(super) fn remember_created<R: Runtime>(window: &WebviewWindow<R>) {
    observe_live(window);
}

/// 生きているウィンドウから形状を読んで観測へ足す。読めない項目は既存の観測を保つ。
fn observe_live(window: &impl GeometryRead) {
    let (position, size) = window.read_geometry();
    remember(window.geometry_label(), position, size);
}

/// 観測へ 1 項目を足す（`None` の項目は既存の値を保つ）。
fn remember(label: &str, position: Option<(i32, i32)>, size: Option<(u32, u32)>) {
    let mut observed = observed();
    let entry = observed.entry(label.to_owned()).or_default();
    if position.is_some() {
        entry.position = position;
    }
    if size.is_some() {
        entry.size = size;
    }
}

/// 観測済みの形状を設定ストアへ保存し、そのラベルの観測を取り除く。
///
/// **これが「閉じた時点で保存する」の実体である。**`Destroyed` はウィンドウが閉じられた
/// 後に届くので、ここで書けば終了イベント（`RunEvent::Exit`）を待たずに値が残る。
fn persist<R: Runtime>(window: &Window<R>) {
    let label = window.label().to_owned();
    // 破棄の通知ではネイティブウィンドウが既に無いので、読み取りは「観測済みの値」で
    // 代替する。読めた場合はそちらを優先する（読めたならそれが最終の形状である）。
    let (live_position, live_size) = window.read_geometry();
    let mut observation = observed().remove(&label).unwrap_or_default();
    if live_position.is_some() {
        observation.position = live_position;
    }
    if live_size.is_some() {
        observation.size = live_size;
    }
    let Some(geometry) = observation.geometry() else {
        // 移動も拡大縮小も観測していないウィンドウである。保存する値が無いので、前の値を
        // そのまま残す（それが「直近に閉じられたウィンドウ」の形状として正しい）。
        log::debug!("観測した形状が無いのでウィンドウの位置とサイズを保存しない: label={label}");
        return;
    };
    save(window.app_handle(), &geometry);
}

/// 形状を読めるウィンドウ（`Window` と `WebviewWindow` の両方から読むための小さな抽象）。
trait GeometryRead {
    /// このウィンドウのラベル。
    fn geometry_label(&self) -> &str;
    /// 枠の位置とクライアント領域の大きさ（どちらも物理ピクセル。読めない項目は `None`）。
    fn read_geometry(&self) -> (Option<(i32, i32)>, Option<(u32, u32)>);
}

impl<R: Runtime> GeometryRead for Window<R> {
    fn geometry_label(&self) -> &str {
        self.label()
    }

    fn read_geometry(&self) -> (Option<(i32, i32)>, Option<(u32, u32)>) {
        (
            self.outer_position().ok().map(|p| (p.x, p.y)),
            self.inner_size().ok().map(|s| (s.width, s.height)),
        )
    }
}

impl<R: Runtime> GeometryRead for WebviewWindow<R> {
    fn geometry_label(&self) -> &str {
        self.label()
    }

    fn read_geometry(&self) -> (Option<(i32, i32)>, Option<(u32, u32)>) {
        (
            self.outer_position().ok().map(|p| (p.x, p.y)),
            self.inner_size().ok().map(|s| (s.width, s.height)),
        )
    }
}

// ---------------------------------------------------------------------------
// 設定ストアとの写像（4.1 / 4.2）
// ---------------------------------------------------------------------------

/// 保存された形状を読む。未保存・壊れた値はどちらも `None`（既定値に落ちる）。
fn read_geometry(store: &impl SettingsStore) -> Option<StoredGeometry> {
    store.get(&SettingsKey::WindowGeometry)
}

/// 形状をストアへ書く。**設定ストアとの写像はこの 1 箇所だけ**である（`AppHandle` を要さない
/// ので、実体のストアを使う単体テストから直接呼べる）。
fn write_geometry(
    store: &impl SettingsStore,
    geometry: &StoredGeometry,
) -> Result<(), app_shell::settings::SettingsError> {
    store.set(&SettingsKey::WindowGeometry, geometry)
}

/// 形状を保存する。**失敗しても致命的にしない** — 記録に残して既定値で動き続ける
/// （4.2 の「理解できないものを壊さない」原則と同じ扱い）。
fn save<R: Runtime>(app: &AppHandle<R>, geometry: &StoredGeometry) {
    let store = app.state::<StartupState>().settings().clone();
    match write_geometry(&*store, geometry) {
        Ok(()) => log::info!(
            "ウィンドウの位置とサイズを保存した: x={} y={} width={} height={}",
            geometry.x,
            geometry.y,
            geometry.width,
            geometry.height,
        ),
        Err(error) => log::warn!("ウィンドウの位置とサイズを保存できなかった: {error}"),
    }
}

// ---------------------------------------------------------------------------
// テスト（タスク 6.3）
//
// GUI を必要としない純粋な部分と、実体の設定ストアを介した写像だけを固定する。生成そのもの
// （`Moved` / `Resized` / `Destroyed` の実際の到来順、WM による位置の確定）はホスト側の
// GUI 実行でしか検証できない。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        logical, logical_signed, plan_restore, read_geometry, to_initial, write_geometry,
        MonitorArea, Observation, Rect, RestorePlan, StoredGeometry, MAX_RESTORABLE_DIMENSION,
        MIN_VISIBLE_EXTENT,
    };
    use std::path::PathBuf;
    use std::sync::Arc;

    /// モニタ 1 台（原点、1920×1080、拡大率 1.0）の作業領域。
    fn monitor() -> MonitorArea {
        MonitorArea {
            rect: Rect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            scale_factor: 1.0,
        }
    }

    fn geometry(x: i32, y: i32, width: u32, height: u32) -> StoredGeometry {
        StoredGeometry {
            x,
            y,
            width,
            height,
        }
    }

    // -----------------------------------------------------------------------
    // 保存値の形と、設定ストアへの写像
    // -----------------------------------------------------------------------

    /// テストごとに独立した設定ディレクトリを用意する（共有実体の登録簿に載らないよう、
    /// テスト名を接頭辞にした一意なパスを使う）。
    fn store_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jxcel-geometry-{}-{prefix}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn open_store(dir: &std::path::Path) -> Arc<app_shell::settings::FileSettingsStore> {
        let (store, report) = app_shell::settings::open(dir).expect("設定ディレクトリを用意できる");
        assert!(report.recovered_from().is_none());
        store
    }

    #[test]
    fn a_saved_geometry_is_one_object_under_the_window_geometry_key() {
        let dir = store_dir("round-trip");
        let store = open_store(&dir);
        let saved = geometry(120, 64, 1024, 768);

        write_geometry(&*store, &saved).expect("保存できる");
        assert_eq!(
            read_geometry(&*store),
            Some(saved),
            "保存した値がそのまま読み戻せる"
        );

        // ファイルの実体が **1 つのキー** `window.geometry` の下の**オブジェクト**であること
        // （design.md「Logical Data Model」）。キーがラベルごとに増えていないことも見る。
        let text = std::fs::read_to_string(dir.join("settings.json")).expect("設定ファイルを読める");
        assert!(text.contains("\"window.geometry\""), "{text}");
        assert!(text.contains("\"x\""), "{text}");
        assert!(text.contains("\"y\""), "{text}");
        assert!(text.contains("\"width\""), "{text}");
        assert!(text.contains("\"height\""), "{text}");
        assert!(
            !text.contains("doc-") && !text.contains("empty-"),
            "ラベル規約に依存したキーを作らない: {text}"
        );

        // 未知のフィールドは無視する（将来この値に情報を足しても、古い版が書いた値を
        // 新しい版が読めるようにするため）。
        drop(store);
        std::fs::write(
            dir.join("settings.json"),
            "{\"window.geometry\":{\"x\":1,\"y\":2,\"width\":3,\"height\":4,\"future\":true}}",
        )
        .expect("設定ファイルを書ける");
        let reopened = open_store(&dir);
        assert_eq!(
            read_geometry(&*reopened),
            Some(geometry(1, 2, 3, 4)),
            "未知のフィールドがあっても読める",
        );
        drop(reopened);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_or_broken_geometry_reads_as_none() {
        // 未保存。
        let dir = store_dir("missing");
        let store = open_store(&dir);
        assert_eq!(read_geometry(&*store), None);
        let _ = std::fs::remove_dir_all(&dir);

        // 壊れた値（オブジェクトでない・フィールドが欠ける・型が違う・巨大すぎる）は
        // どれも「保存値が無い」と同じ扱いになる。
        for broken in [
            "\"not an object\"",
            "{\"x\":1,\"y\":2,\"width\":3}",
            "{\"x\":1,\"y\":2,\"width\":-3,\"height\":4}",
            "{\"x\":1,\"y\":2,\"width\":99999999999,\"height\":4}",
        ] {
            let dir = store_dir("broken");
            std::fs::create_dir_all(&dir).expect("設定ディレクトリを作れる");
            std::fs::write(
                dir.join("settings.json"),
                format!("{{\"window.geometry\":{broken}}}"),
            )
            .expect("壊れた設定ファイルを書ける");
            let store = open_store(&dir);
            assert_eq!(read_geometry(&*store), None, "{broken}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    // -----------------------------------------------------------------------
    // 復元の計画（画面外・壊れた値）
    // -----------------------------------------------------------------------

    #[test]
    fn no_saved_value_or_a_broken_size_falls_back_to_the_defaults() {
        assert_eq!(plan_restore(None, &[monitor()]), RestorePlan::Default);
        // 大きさ 0 と、上限を超える大きさはどちらも壊れた値である。
        for (width, height) in [(0, 768), (1024, 0), (MAX_RESTORABLE_DIMENSION + 1, 768)] {
            assert_eq!(
                plan_restore(Some(geometry(10, 10, width, height)), &[monitor()]),
                RestorePlan::Default,
                "{width}x{height}",
            );
        }
    }

    #[test]
    fn an_on_screen_geometry_restores_the_position_and_the_size() {
        let stored = geometry(200, 100, 1024, 768);
        assert_eq!(
            plan_restore(Some(stored), &[monitor()]),
            RestorePlan::PositionAndSize(stored),
        );

        // 負の座標でも、十分に画面へ重なっていれば位置として使う（左端に寄せたウィンドウ）。
        let leaning = geometry(-300, 10, 1024, 768);
        assert_eq!(
            plan_restore(Some(leaning), &[monitor()]),
            RestorePlan::PositionAndSize(leaning),
        );
    }

    #[test]
    fn a_position_on_a_disconnected_monitor_keeps_only_the_size() {
        // 保存時にはあった 2 台目のモニタ（x=1920 から）が取り外された状況。
        let stored = geometry(2500, 200, 800, 600);
        assert_eq!(
            plan_restore(Some(stored), &[monitor()]),
            RestorePlan::SizeOnly {
                width: 800,
                height: 600
            },
            "見えない位置へは開かない",
        );
    }

    #[test]
    fn a_barely_visible_position_is_rejected_and_an_empty_monitor_list_is_not_trusted() {
        // 画面へ 1 ピクセルしか重ならない位置は「見えない」に倒す。
        let sliver = geometry(1919, 200, 800, 600);
        assert_eq!(
            plan_restore(Some(sliver), &[monitor()]),
            RestorePlan::SizeOnly {
                width: 800,
                height: 600
            },
        );
        // ちょうど最小の重なりなら受け入れる。
        let threshold = geometry(1920 - MIN_VISIBLE_EXTENT as i32, 0, 800, 600);
        assert_eq!(
            plan_restore(Some(threshold), &[monitor()]),
            RestorePlan::PositionAndSize(threshold),
        );
        // モニタを列挙できない場合は位置を検証できないので使わない。
        assert_eq!(
            plan_restore(Some(geometry(0, 0, 800, 600)), &[]),
            RestorePlan::SizeOnly {
                width: 800,
                height: 600
            },
        );
    }

    #[test]
    fn the_two_axis_rule_requires_visible_extent_on_both_axes() {
        // 横には十分重なるが縦には 1 ピクセルしか重ならない位置も、見えない側に倒す。
        let stored = geometry(100, 1079, 800, 600);
        assert_eq!(
            plan_restore(Some(stored), &[monitor()]),
            RestorePlan::SizeOnly {
                width: 800,
                height: 600
            },
        );
    }

    // -----------------------------------------------------------------------
    // 論理ピクセルへの写像
    // -----------------------------------------------------------------------

    #[test]
    fn the_plan_maps_to_logical_pixels_using_the_monitor_scale() {
        // 拡大率 1.0 では物理 = 論理。
        let stored = geometry(200, 100, 1024, 768);
        let initial = to_initial(RestorePlan::PositionAndSize(stored), 1.0);
        assert_eq!(initial.position(), Some((200.0, 100.0)));
        assert_eq!(initial.size(), Some((1024.0, 768.0)));

        // 拡大率 2.0 では半分の論理値になる（そのモニタでは同じ物理形状になる）。
        let initial = to_initial(RestorePlan::PositionAndSize(stored), 2.0);
        assert_eq!(initial.position(), Some((100.0, 50.0)));
        assert_eq!(initial.size(), Some((512.0, 384.0)));

        // 位置を使わない計画は大きさだけを写す。
        let initial = to_initial(
            RestorePlan::SizeOnly {
                width: 1024,
                height: 768,
            },
            2.0,
        );
        assert_eq!(initial.position(), None);
        assert_eq!(initial.size(), Some((512.0, 384.0)));

        // 復元する値が無い計画は何も与えない。
        assert_eq!(to_initial(RestorePlan::Default, 2.0).position(), None);
        assert_eq!(to_initial(RestorePlan::Default, 2.0).size(), None);
    }

    #[test]
    fn a_broken_scale_factor_is_treated_as_one() {
        assert_eq!(logical(1024, 0.0), 1024.0);
        assert_eq!(logical(1024, f64::NAN), 1024.0);
        assert_eq!(logical_signed(-200, -1.0), -200.0);
    }

    #[test]
    fn an_observation_needs_both_a_position_and_a_size() {
        assert_eq!(Observation::default().geometry(), None);
        let position_only = Observation {
            position: Some((10, 20)),
            ..Observation::default()
        };
        assert_eq!(position_only.geometry(), None);
        let size_only = Observation {
            size: Some((800, 600)),
            ..Observation::default()
        };
        assert_eq!(size_only.geometry(), None);
        let complete = Observation {
            position: Some((10, 20)),
            size: Some((800, 600)),
        };
        assert_eq!(complete.geometry(), Some(geometry(10, 20, 800, 600)));
    }
}

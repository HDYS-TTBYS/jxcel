//! 描画の代替経路（回避策の環境変数）の選択と適用（要件 10.3。タスク 8.3）。
//!
//! 所有: `RenderWatchdog`（design.md「Components and Interfaces → Adapter Layer」）のうち
//! **8.3 が担う「適用」の部分**。判定（[`crate::render`]）が残した印を読み、必要なら
//! **GTK / WebKit のコードが動く前**に回避策の環境変数を設定する。アダプタ層
//! （`src-tauri/src/lifecycle.rs` の `reserve_render_fallback_point`）が起動の手順 1 で
//! このモジュールの [`apply_pending`] を呼ぶ。
//!
//! # なぜ「次回の起動」なのか
//!
//! 回避策の環境変数は**描画基盤の初期化前に設定しなければならない**。描画の不成立を検出
//! できるのは描画基盤が動いた後（期限超過の時点）なので、検出した時点では既に手遅れである
//! （design.md「RenderWatchdog」）。したがって検出時は判定（8.2）が設定に印を残し、**次の
//! 起動の手順 1** でこのモジュールがその印を読んで適用する。順序の根拠は research.md 決定 7
//! と、design.md「AppLifecycle」の「環境変数（描画の回避策）は GTK と WebKit のコードが
//! 動く前に設定する」である。
//!
//! # 無条件には適用しない（要件 10.3）
//!
//! **印が立っているときだけ適用する。** 回避策は一部の環境の問題を全環境の性能低下と
//! 引き換えに直すものであり、公式ドキュメント自身が無条件適用を戒めている
//! （research.md「Linux / WebKitGTK の描画」）。印は「直近に確定した判定が `NoPaint`
//! であった」ことだけを意味するので、正常に描画できた環境の起動では何も設定しない。
//!
//! # 特定の変数に賭けない（research.md 決定 7）
//!
//! 回避策の優先順位について**情報源が矛盾しており Tauri の裁定がない**。したがって設計は
//! 「検出 → 適用 → 記録」という枠組みだけを固定し、どの変数を適用するかは**このファイルの
//! 表 [`CANDIDATES`] と、採用する並び [`LINUX_POLICY`] の 2 つだけが決める**。機構
//! （[`apply`] / [`apply_pending`]）は変数名を一切知らず、表を書き換えるだけで別の回避策へ
//! 切り替えられる。**この分離が「賭けない」ことの実装上の意味である** — 特定の変数を
//! 機構へ直接書かない。
//!
//! 現在の採用は [`LINUX_POLICY`] の 1 件（`WEBKIT_DISABLE_DMABUF_RENDERER=1`）である。
//! これは「これが正解だ」という主張ではなく、**証拠の広さと代償の小ささで選んだ初期値**で
//! ある。候補と代償は [`CANDIDATES`] に並べてあり、切り替えは 1 箇所の編集で済む
//! （[`LINUX_POLICY`] の要素を差し替える。機構も記録の形式も変えなくてよい）。
//!
//! | 候補 | 代償 | 採用しなかった理由 |
//! |---|---|---|
//! | `WEBKIT_DISABLE_DMABUF_RENDERER=1` | 高速描画経路（ゼロコピーの DMABUF）を捨てる。GPU 合成とハードウェア加速は残る | — **採用**（白いウィンドウの報告で最も広く一致し、特定のベンダーに依存しない） |
//! | `WEBKIT_DISABLE_COMPOSITING_MODE=1` | ハードウェア支援を切る（性能低下がより大きい） | 代償がより重い。DMABUF 経路が原因でない場合の第 2 候補として [`CANDIDATES`] に残す |
//! | `__NV_DISABLE_EXPLICIT_SYNC=1` | 性能を落とさないという報告がある | NVIDIA の明示同期に固有の対処であり、印は GPU の種類を持たないため他社の GPU では無意味になる。**賭けにならない**ので採用しない（NVIDIA に限定できる証拠が得られたら 1 行の切り替えで昇格できる） |
//!
//! # プラットフォームの境界
//!
//! **これは Linux / WebKitGTK の問題である**（design.md、research.md）。Linux 以外では
//! [`current_policy`] が空を返し、**何も適用しない** — Windows は WebView2、macOS は
//! WKWebView を使い、この回避策は意味を持たない。印が立っていても Linux 以外では何も
//! 起きず、印は次の起動の判定（`Painted`）で下ろされる（[`crate::render`] の状態機械）。
//!
//! # 起動を止めない
//!
//! 設定ストアを開けない場合・印を読めない場合は、**何も適用せずに起動を続ける**
//! （[`FallbackOutcome::unreadable`]）。前提不成立の報告は `init_diagnostics`（診断の初期化）
//! が本来の 1 経路で行う。ここで失敗を返すと報告経路が 2 本になる。
//!
//! # 検証の入口
//!
//! 実プロセスの環境変数とプラットフォームを [`Environment`] と [`RenderPlatform`] として
//! 注入できる。**テストは実環境を触らずに、印 → 適用の判断をそのまま通せる**
//! （検証専用の並行実装は持たない）。production の [`apply_pending`] は、この注入点へ
//! 実プロセスの環境と実行中のプラットフォームを渡すだけである。
//!
//! # 適用の記録
//!
//! このモジュールは記録機構（診断）を知らない。適用した事実は [`FallbackOutcome::applied`]
//! として返し、**アダプタがロガーの取り付け後に起動行として記録する**（記録機構は手順 4 で
//! 初めて動くため、手順 1 の時点では記録できない。5.2 の起動行と同じ事情）。

use crate::render::render_fallback_pending;
use crate::settings::FileSettingsStore;

// ---------------------------------------------------------------------------
// プラットフォーム（要件 10.3）
// ---------------------------------------------------------------------------

/// 回避策の適用先を決めるプラットフォーム。
///
/// **引数として受けるのは、テストが 3 OS の境界を決定的に確かめられるようにするためである**
/// （このクレートのテストは Linux 上で走るが、Windows / macOS の判断も固定できなければ
/// ならない）。production は [`RenderPlatform::current`] を使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderPlatform {
    /// この回避策の対象。GTK / WebKitGTK を使う。
    Linux,
    /// `WKWebView` を使う。この回避策の対象外。
    MacOs,
    /// `WebView2` を使う。この回避策の対象外。
    Windows,
    /// 上記以外。この回避策の対象外。
    Other,
}

impl RenderPlatform {
    /// 実行中のプラットフォーム。**`cfg` で選ぶ唯一の場所である。**
    pub const fn current() -> Self {
        #[cfg(target_os = "linux")]
        return RenderPlatform::Linux;
        #[cfg(target_os = "macos")]
        return RenderPlatform::MacOs;
        #[cfg(windows)]
        return RenderPlatform::Windows;
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        return RenderPlatform::Other;
    }

    /// 記録に出す名前（診断の行に「どのプラットフォームで適用しなかったか」を残す）。
    pub const fn as_str(self) -> &'static str {
        match self {
            RenderPlatform::Linux => "linux",
            RenderPlatform::MacOs => "macos",
            RenderPlatform::Windows => "windows",
            RenderPlatform::Other => "other",
        }
    }
}

// ---------------------------------------------------------------------------
// 回避策の表（**選択はここだけが決める**）
// ---------------------------------------------------------------------------

/// 回避策 1 件。適用する環境変数とその値である。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderFallback {
    /// 環境変数の名前。
    pub variable: &'static str,
    /// 設定する値。
    pub value: &'static str,
}

/// 採用する回避策の並び（**このファイルの外に変数名を書かない**）。
///
/// 現在は 1 件だけである。**複数にする場合は「代償の小さいものから順に」並べる** — 適用は
/// 並び順に行われ、どれが効いたかを記録から読み取れる（[`FallbackApplication`]）。
pub const LINUX_POLICY: &[RenderFallback] = &[RenderFallback {
    variable: "WEBKIT_DISABLE_DMABUF_RENDERER",
    value: "1",
}];

/// 候補の 1 件（採用・不採用の両方を、代償と根拠つきで残す）。
///
/// **表に残すのは、切り替えを 1 箇所の編集で済ませるためである。** 証拠が変わったときに
/// [`LINUX_POLICY`] を書き換えれば、機構も記録の形式もそのままで新しい回避策へ移れる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FallbackCandidate {
    /// 適用する環境変数。
    pub fallback: RenderFallback,
    /// この回避策を適用したときに利用者が払う代償。
    pub cost: &'static str,
    /// 採用しているかどうか（不採用の理由は `rationale`）。
    pub adopted: bool,
    /// 採用・不採用の根拠。
    pub rationale: &'static str,
}

/// 候補の一覧（research.md「Linux / WebKitGTK の描画」が挙げた 3 つ）。
pub const CANDIDATES: &[FallbackCandidate] = &[
    FallbackCandidate {
        fallback: RenderFallback {
            variable: "WEBKIT_DISABLE_DMABUF_RENDERER",
            value: "1",
        },
        cost: "高速描画経路（ゼロコピーの DMABUF）を捨てる。GPU 合成とハードウェア加速は残る",
        adopted: true,
        rationale: "白いウィンドウの報告で最も広く一致し、特定のベンダーに依存しない。代償は\
                    最適化 1 つの喪失にとどまる",
    },
    FallbackCandidate {
        fallback: RenderFallback {
            variable: "WEBKIT_DISABLE_COMPOSITING_MODE",
            value: "1",
        },
        cost: "ハードウェア支援を切る（性能低下がより大きい）",
        adopted: false,
        rationale: "代償がより重い。DMABUF 経路が原因でない場合の第 2 候補",
    },
    FallbackCandidate {
        fallback: RenderFallback {
            variable: "__NV_DISABLE_EXPLICIT_SYNC",
            value: "1",
        },
        cost: "性能を落とさないという報告がある",
        adopted: false,
        rationale: "NVIDIA の明示同期に固有の対処である。印は GPU の種類を持たないため、\
                    他社の GPU では無意味な賭けになる",
    },
];

/// このプラットフォームで採用している回避策。
///
/// **Linux 以外は空である**（[`RenderPlatform`] の doc とモジュール doc「プラットフォームの
/// 境界」）。`const fn` にしてあるので、production の呼び出しでも表の走査は起こらない。
pub const fn policy(platform: RenderPlatform) -> &'static [RenderFallback] {
    match platform {
        RenderPlatform::Linux => LINUX_POLICY,
        RenderPlatform::MacOs | RenderPlatform::Windows | RenderPlatform::Other => &[],
    }
}

/// 実行中のプラットフォームで採用している回避策（[`policy`] の production 用の入口）。
pub fn current_policy() -> &'static [RenderFallback] {
    policy(RenderPlatform::current())
}

// ---------------------------------------------------------------------------
// 環境（テストが実環境を触らずに適用を観察するための注入点）
// ---------------------------------------------------------------------------

/// 環境変数の読み書き。**production の実装は [`ProcessEnvironment`] 1 つだけである。**
pub trait Environment: Send + Sync {
    /// 現在の値（未設定なら `None`）。
    fn get(&self, name: &str) -> Option<String>;

    /// 値を設定する。
    fn set(&self, name: &str, value: &str);
}

/// 実プロセスの環境変数。
///
/// **この実装を呼ぶのは起動の手順 1 だけである**（`reserve_render_fallback_point`）。その
/// 時点のプロセスは単一スレッドである — 単一インスタンスのプラグインも GTK もまだ動いて
/// おらず、期限を見張るスレッド（8.2）も構築の後にしか始まらない。したがって環境変数の
/// 設定が他のスレッドと競合する経路は無い。
pub struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn get(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn set(&self, name: &str, value: &str) {
        std::env::set_var(name, value);
    }
}

// ---------------------------------------------------------------------------
// 適用（要件 10.3）
// ---------------------------------------------------------------------------

/// 適用した 1 件の由来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackOrigin {
    /// 環境に無かったので設定した。
    Added,
    /// 環境にあったが値が違ったので置き換えた。**印の指示が優先する**（印は描画が成立して
    /// いないことの記録であり、既存の値はその不成立を防げなかった）。
    Replaced,
    /// 既に同じ値があったので何もしなかった（利用者が自分で設定している場合を含む）。
    AlreadySet,
}

/// 適用した 1 件（記録の材料であり、ヘッドレスの検証の観測点でもある）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackApplication {
    /// 適用した環境変数の名前。
    pub variable: &'static str,
    /// 適用した値。
    pub value: &'static str,
    /// 適用の由来（設定した・置き換えた・既にあった）。
    pub origin: FallbackOrigin,
}

/// 印に従って回避策を適用する（**純粋な部分。実プロセスの環境は [`env`] が担う**）。
///
/// 印が立っていなければ何もしない（要件 10.3 の「無条件には適用しない」）。適用したものを
/// 返すので、呼び出し側（アダプタ）が**どの代替経路を使ったかを記録できる**
/// （要件 10.3 の「代替経路を用いた事実を診断情報に記録する」）。
///
/// [`env`]: Environment
pub fn apply(
    pending: bool,
    platform: RenderPlatform,
    env: &dyn Environment,
) -> Vec<FallbackApplication> {
    if !pending {
        return Vec::new();
    }
    let policy = policy(platform);
    let mut applied = Vec::with_capacity(policy.len());
    for fallback in policy {
        let origin = match env.get(fallback.variable) {
            Some(current) if current == fallback.value => FallbackOrigin::AlreadySet,
            Some(_) => {
                env.set(fallback.variable, fallback.value);
                FallbackOrigin::Replaced
            }
            None => {
                env.set(fallback.variable, fallback.value);
                FallbackOrigin::Added
            }
        };
        applied.push(FallbackApplication {
            variable: fallback.variable,
            value: fallback.value,
            origin,
        });
    }
    applied
}

/// 起動の手順 1 の結論（**起動を止めない**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackOutcome {
    /// 印が立っていたか。`None` は**印を読めなかった**こと（設定ストアを開けなかった）を
    /// 表す。`Some(false)` は「読めたが立っていなかった」である。
    pub pending: Option<bool>,
    /// この起動で適用した回避策（印が無ければ空）。
    pub applied: Vec<FallbackApplication>,
}

impl FallbackOutcome {
    /// 印を読めなかった場合の結論（**何も適用していない**）。
    ///
    /// 起動は止めない。設定ストアを開けないという事実は `init_diagnostics`（手順 3）が
    /// 本来の報告経路で扱う。
    pub fn unreadable() -> Self {
        Self {
            pending: None,
            applied: Vec::new(),
        }
    }
}

/// 印を読んで適用する（**production の唯一の入口**）。現在のプラットフォームと、実プロセスの
/// 環境を使う。
///
/// **8.3 は印を書かない。** 書くのは判定の中核 [`crate::render::RenderWatchdog`] だけである
/// （印の状態機械は [`crate::render`] のモジュール doc を参照）。
pub fn apply_pending(settings: &FileSettingsStore) -> FallbackOutcome {
    apply_pending_with(settings, RenderPlatform::current(), &ProcessEnvironment)
}

/// プラットフォームと環境を差し替えられる [`apply_pending`]（**同じ経路である**）。
///
/// ヘッドレスのテストが実環境を汚さずに「印 → 適用」の判断を通すための入口であり、
/// 検証専用の並行実装ではない。production の [`apply_pending`] はこれをそのまま呼ぶ。
pub fn apply_pending_with(
    settings: &FileSettingsStore,
    platform: RenderPlatform,
    env: &dyn Environment,
) -> FallbackOutcome {
    let pending = render_fallback_pending(settings);
    FallbackOutcome {
        pending: Some(pending),
        applied: apply(pending, platform, env),
    }
}

// ---------------------------------------------------------------------------
// テスト（タスク 8.3 の完了状態: 実画面なしで印 → 適用を固定する）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{FallbackMark, SettingsFallbackMark};
    use crate::settings::{self, SettingsKey, SettingsStore};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    /// 記録するだけの環境（**実プロセスの環境を汚さない**。並行するテストと競合しない）。
    #[derive(Default)]
    struct RecordingEnvironment {
        values: Mutex<BTreeMap<String, String>>,
        sets: AtomicU64,
    }

    impl RecordingEnvironment {
        fn with(name: &str, value: &str) -> Self {
            let environment = Self::default();
            environment
                .values
                .lock()
                .expect("環境の写しを壊さない")
                .insert(name.to_owned(), value.to_owned());
            environment
        }

        fn get_recorded(&self, name: &str) -> Option<String> {
            self.values
                .lock()
                .expect("環境の写しを壊さない")
                .get(name)
                .cloned()
        }

        fn set_count(&self) -> u64 {
            self.sets.load(Ordering::SeqCst)
        }
    }

    impl Environment for RecordingEnvironment {
        fn get(&self, name: &str) -> Option<String> {
            self.get_recorded(name)
        }

        fn set(&self, name: &str, value: &str) {
            self.sets.fetch_add(1, Ordering::SeqCst);
            self.values
                .lock()
                .expect("環境の写しを壊さない")
                .insert(name.to_owned(), value.to_owned());
        }
    }

    /// 設定ストアを一時ディレクトリに開く（実在する鍵に本当に書けることを確かめるため）。
    fn settings_store() -> (std::path::PathBuf, std::sync::Arc<FileSettingsStore>) {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "jxcel-render-fallback-{}-{sequence}",
            std::process::id()
        ));
        let (store, _report) = settings::open(&directory).expect("設定ストアを開ける");
        (directory, store)
    }

    // ------------------------------------------------------------------
    // 採用している回避策（判断の単一の場所）
    // ------------------------------------------------------------------

    /// Linux には採用した回避策があり、Linux 以外には無い（要件 10.3 のプラットフォーム境界）。
    #[test]
    fn only_linux_has_a_fallback_policy() {
        assert!(!policy(RenderPlatform::Linux).is_empty());
        assert!(policy(RenderPlatform::MacOs).is_empty());
        assert!(policy(RenderPlatform::Windows).is_empty());
        assert!(policy(RenderPlatform::Other).is_empty());
    }

    /// 採用した並びは候補表から選ばれている（**表の外に変数名が生えない**）。
    #[test]
    fn the_adopted_policy_comes_from_the_candidate_table() {
        for fallback in LINUX_POLICY {
            assert!(
                CANDIDATES
                    .iter()
                    .any(|candidate| candidate.adopted && candidate.fallback == *fallback),
                "候補表に無い回避策を採用している: {fallback:?}"
            );
        }
        // 採用した候補には代償と根拠が書かれている（人が選び直せる状態に保つ）。
        for candidate in CANDIDATES.iter().filter(|candidate| candidate.adopted) {
            assert!(!candidate.cost.is_empty());
            assert!(!candidate.rationale.is_empty());
        }
    }

    // ------------------------------------------------------------------
    // 完了状態 (a): 印が立っていれば適用され、適用したものが観測できる
    // ------------------------------------------------------------------

    /// 印が立っていれば、採用した回避策が環境へ設定され、適用の一覧で観測できる。
    #[test]
    fn a_pending_mark_applies_the_documented_fallback() {
        let (directory, store) = settings_store();
        // 8.2 の書き手（`RenderWatchdog`）が実際に使う経路で印を立てる。
        SettingsFallbackMark::new(std::sync::Arc::clone(&store))
            .set(true)
            .expect("印を書ける");
        let environment = RecordingEnvironment::default();

        let outcome = apply_pending_with(&store, RenderPlatform::Linux, &environment);

        assert_eq!(outcome.pending, Some(true));
        assert_eq!(outcome.applied.len(), LINUX_POLICY.len());
        for fallback in LINUX_POLICY {
            assert_eq!(
                environment.get_recorded(fallback.variable).as_deref(),
                Some(fallback.value),
                "採用した回避策が環境へ設定されていない: {fallback:?}"
            );
        }
        assert_eq!(outcome.applied[0].origin, FallbackOrigin::Added);
        assert_eq!(outcome.applied[0].variable, LINUX_POLICY[0].variable);
        assert_eq!(outcome.applied[0].value, LINUX_POLICY[0].value);
        let _ = std::fs::remove_dir_all(&directory);
    }

    // ------------------------------------------------------------------
    // 完了状態 (b): 印が無ければ何も適用しない
    // ------------------------------------------------------------------

    /// 印が無ければ環境に一切触れない（**無条件適用の禁止**。要件 10.3）。
    #[test]
    fn a_clear_mark_applies_nothing() {
        let (directory, store) = settings_store();
        let environment = RecordingEnvironment::default();

        let outcome = apply_pending_with(&store, RenderPlatform::Linux, &environment);

        assert_eq!(outcome.pending, Some(false));
        assert!(outcome.applied.is_empty());
        assert_eq!(environment.set_count(), 0, "環境へ書き込んではならない");
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// 印が `false` と書かれていても何も適用しない（明示的な不成立の解除）。
    #[test]
    fn an_explicitly_cleared_mark_applies_nothing() {
        let (directory, store) = settings_store();
        store
            .set(&SettingsKey::RenderFallback, &false)
            .expect("印を書ける");
        let environment = RecordingEnvironment::default();

        let outcome = apply_pending_with(&store, RenderPlatform::Linux, &environment);

        assert_eq!(outcome.pending, Some(false));
        assert!(outcome.applied.is_empty());
        assert_eq!(environment.set_count(), 0);
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// 読めない印（真偽でない値）は「立っていない」として扱い、起動を止めない。
    #[test]
    fn an_unreadable_mark_is_treated_as_clear() {
        let (directory, store) = settings_store();
        store
            .set(&SettingsKey::RenderFallback, &"yes")
            .expect("印を書ける");
        let environment = RecordingEnvironment::default();

        let outcome = apply_pending_with(&store, RenderPlatform::Linux, &environment);

        assert_eq!(outcome.pending, Some(false));
        assert!(outcome.applied.is_empty());
        assert_eq!(environment.set_count(), 0);
        let _ = std::fs::remove_dir_all(&directory);
    }

    // ------------------------------------------------------------------
    // プラットフォームの境界（Linux / WebKitGTK 以外には適用しない）
    // ------------------------------------------------------------------

    /// 印が立っていても、Linux 以外では何も適用しない（Windows / macOS は別の WebView）。
    #[test]
    fn a_pending_mark_applies_nothing_outside_linux() {
        let (directory, store) = settings_store();
        SettingsFallbackMark::new(std::sync::Arc::clone(&store))
            .set(true)
            .expect("印を書ける");
        for platform in [
            RenderPlatform::MacOs,
            RenderPlatform::Windows,
            RenderPlatform::Other,
        ] {
            let environment = RecordingEnvironment::default();
            let outcome = apply_pending_with(&store, platform, &environment);
            assert_eq!(outcome.pending, Some(true));
            assert!(
                outcome.applied.is_empty(),
                "{platform:?} へ適用してはならない: {outcome:?}"
            );
            assert_eq!(environment.set_count(), 0);
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    // ------------------------------------------------------------------
    // 既存の値の扱い
    // ------------------------------------------------------------------

    /// 既に同じ値があれば設定し直さない（由来は `AlreadySet`）。
    #[test]
    fn an_already_set_value_is_not_written_again() {
        let (directory, store) = settings_store();
        SettingsFallbackMark::new(std::sync::Arc::clone(&store))
            .set(true)
            .expect("印を書ける");
        let fallback = LINUX_POLICY[0];
        let environment = RecordingEnvironment::with(fallback.variable, fallback.value);

        let outcome = apply_pending_with(&store, RenderPlatform::Linux, &environment);

        assert_eq!(outcome.applied[0].origin, FallbackOrigin::AlreadySet);
        assert_eq!(environment.set_count(), 0, "同じ値を書き直してはならない");
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// 違う値があれば印の指示で置き換える（由来は `Replaced`）。
    #[test]
    fn a_different_existing_value_is_replaced() {
        let (directory, store) = settings_store();
        SettingsFallbackMark::new(std::sync::Arc::clone(&store))
            .set(true)
            .expect("印を書ける");
        let fallback = LINUX_POLICY[0];
        let environment = RecordingEnvironment::with(fallback.variable, "0");

        let outcome = apply_pending_with(&store, RenderPlatform::Linux, &environment);

        assert_eq!(outcome.applied[0].origin, FallbackOrigin::Replaced);
        assert_eq!(
            environment.get_recorded(fallback.variable).as_deref(),
            Some(fallback.value)
        );
        assert_eq!(environment.set_count(), 1);
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// 印を読めないときの結論は `pending: None` で、環境に触れない（起動を止めない）。
    #[test]
    fn an_unreadable_outcome_carries_no_application() {
        let outcome = FallbackOutcome::unreadable();

        assert_eq!(outcome.pending, None);
        assert!(outcome.applied.is_empty());
    }

    /// 実プロセスの環境を読み書きできる（`ProcessEnvironment` は薄い写像である）。
    ///
    /// 採用した回避策の名前は使わない — **このテストが本物の名前を汚すと、同じプロセスで
    /// 走る他のテストの前提を壊しうる**（印 → 適用の判断は `RecordingEnvironment` が固定する）。
    #[test]
    fn the_process_environment_reads_and_writes_variables() {
        let name = format!("JXCEL_TEST_PROCESS_ENV_{}", std::process::id());
        let environment = ProcessEnvironment;
        assert_eq!(environment.get(&name), None);

        environment.set(&name, "probe");
        assert_eq!(environment.get(&name).as_deref(), Some("probe"));
        std::env::remove_var(&name);
        assert_eq!(environment.get(&name), None);
    }
}

//! テスト専用: `log` の面へ取り付ける記録の受け皿（**全テストが 1 つを共有する**）。
//!
//! # なぜ共有するのか
//!
//! `log` の面に取り付けられる記録器は **1 プロセスに 1 つだけ**であり、`set_logger` は
//! 2 度目に失敗する。テストは同じバイナリで並行に走るため、複数のテストがそれぞれ自分の
//! 受け皿を取り付けようとすると、**先に取った方が勝ち、後から取った方は記録を読めない**
//! （観測が空になり、原因の見えない失敗になる。実際に `macro-runtime` のタスク 4.3 が
//! 受け皿を足したときに `sidecar_host` の検査がこれで落ちた）。
//!
//! したがって受け皿はここ 1 つに置き、**行を読む側は [`lines`] で自分の接頭辞に絞る**。
//! 実起動の記録（`tauri-plugin-log` がファイルへ書くもの）と同じ内容が入る — 実起動の観測は
//! 5.2 の検査器がファイルを読み、単体テストはこの受け皿を読む（読む口が違うだけで、
//! **記録を出す経路は同じである**）。
//!
//! # 本番には存在しない
//!
//! `main.rs` が `#[cfg(test)]` の下でだけ宣言する。配布物には識別子すら残らない
//! （`structure.md`「検証専用のコードは出荷物に入れない」の Rust 側の手段と同じ）。

use std::sync::{LazyLock, Mutex};

use tauri_plugin_log::log;

/// 捕まえた記録の行（`[対象名][水準] 本文`）。試験の間だけの控えである。
static CAPTURED: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// 受け皿の取り付け（**プロセスで 1 回だけ**。`LazyLock` が初期化を 1 度に閉じる）。
static INSTALLED: LazyLock<()> = LazyLock::new(|| {
    log::set_logger(&LOGGER).expect("この試験ではまだ記録器が取り付いていない");
    log::set_max_level(log::LevelFilter::Trace);
});

/// 受け皿そのもの（`log::set_logger` は `'static` の参照を要求する）。
struct CaptureLogger;

static LOGGER: CaptureLogger = CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let line = format!("[{}][{}] {}", record.target(), record.level(), record.args());
        CAPTURED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(line);
    }

    fn flush(&self) {}
}

/// 受け皿を取り付ける（2 度目以降は何もしない）。
///
/// **この関数を呼ばないテストは記録を読めない**（本番と同じく、記録は誰かが取り付けた
/// 記録器へ流れる）。
pub(crate) fn install() {
    LazyLock::force(&INSTALLED);
}

/// 捕まえた行の全体（書かれた順）。
pub(crate) fn captured() -> Vec<String> {
    CAPTURED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// 捕まえた行のうち、`needle` を含むものだけ（書かれた順）。
///
/// **接頭辞ではなく「含む」で絞る**のは、`log` の受け皿が行の頭に
/// `[対象名][水準] ` を足すためである（記録の本文はそのうしろに始まる）。
pub(crate) fn lines_containing(needle: &str) -> Vec<String> {
    captured()
        .into_iter()
        .filter(|line| line.contains(needle))
        .collect()
}

/// 記録に条件を満たす行が現れるまで待つ（現れなければ期限で偽を返す）。
pub(crate) fn wait_for_record(
    predicate: impl Fn(&str) -> bool,
    timeout: std::time::Duration,
) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if captured().iter().any(|line| predicate(line)) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

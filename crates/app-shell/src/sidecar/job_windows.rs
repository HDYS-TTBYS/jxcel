//! Windows における Job Object と `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` による終了保証（要件 5.6）。
//!
//! 補助プロセスを Job Object に割り当て、ジョブのハンドルが閉じたときにカーネルが子を終了させる
//! よう設定する。これはアプリケーション側が異常終了して `RunEvent::Exit` が発火しない経路でも
//! 有効な唯一の機構である（design.md「SidecarSupervisor」、research.md 決定 5）。孫プロセスも
//! ジョブに属する限り同じ保証の下に入る。
//!
//! 本モジュールは `cfg(windows)` のときだけコンパイルされる。本タスク（1.2）では骨組みのみを
//! 置く。実体は tasks.md 3.3 が追加する。

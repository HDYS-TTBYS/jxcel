//! Unix におけるプロセスグループと `killpg` による終了保証（要件 5.6）。
//!
//! 補助プロセスを専用のプロセスグループで起動し、終了時はグループ宛に送ることで、補助プロセスが
//! さらに起動した孫プロセスまで届かせる。猶予を与えてから強制終了へ移る段階もここが持つ
//! （design.md「SidecarSupervisor」、research.md 決定 5）。`libc` は `cfg(unix)` の
//! ターゲット依存であり、他プラットフォームのビルドには現れない。
//!
//! # なぜ `setsid` ではなく `setpgid` か
//!
//! [`configure`] は子を `setpgid(0, 0)` で**新しいプロセスグループのリーダー**にする。`setsid` でも
//! 新しいプロセスグループはできるが、同時に新しいセッションへ移り制御端末を切り離す。補助プロセスは
//! 私たちが起動する通常の子プロセスであり、セッションを分ける必要はない。`setpgid` の方が対象が
//! 狭く、副作用が少ない。
//!
//! 重要なのは**いつ**グループを作るかである。`pre_exec` は `fork` 後 `exec` 前の子で走るため、
//! グループは子がどんなコードを実行するよりも前に確定する。子がこの後に起動する孫は必ずこの
//! グループを継承するので、「孫まで届く」が race なしに成立する（tasks.md 3.3 の完了状態）。
//!
//! # 猶予と強制
//!
//! 穏やかな終了は [`Group::signal_graceful`]（`SIGTERM`）、強制は [`Group::signal_force`]
//! （`SIGKILL`）である。どちらもグループ宛に送る。段階の制御（猶予時間の計測と強制への移行）は
//! [`super::supervisor`] の `terminate` が行い、ここは信号を送るだけである。

use std::io;
use std::process::{Child, Command, ExitStatus};

/// 子を新しいプロセスグループのリーダーにする。`spawn` の直前に呼ぶ。
pub(crate) fn configure(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    // SAFETY: `pre_exec` のクロージャは `fork` 後 `exec` 前の子で走る。ここで呼ぶのは
    // async-signal-safe な `setpgid(2)` だけであり、確保もロックも行わない。
    // `setpgid(0, 0)` は「自身の PID を自身の PGID にする」の意である。
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// 起動した補助プロセスのプロセスグループ。
///
/// PID と PGID が一致する（[`configure`] が `setpgid(0, 0)` を使うため）ので、保持するのは
/// グループリーダーの PID だけでよい。`killpg` の宛先になる。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Group {
    pgid: libc::pid_t,
}

impl Group {
    /// 子のプロセスグループを表す値を得る。`configure` 済みの子に対して呼ぶ。
    pub(crate) fn attach(child: &Child) -> io::Result<Self> {
        Ok(Group {
            pgid: child.id() as libc::pid_t,
        })
    }

    /// グループ宛に穏やかな終了（`SIGTERM`）を送る。既に対象が無い場合は `Ok(())`。
    pub(crate) fn signal_graceful(&self) -> io::Result<()> {
        self.signal(libc::SIGTERM)
    }

    /// グループ宛に強制終了（`SIGKILL`）を送る。既に対象が無い場合は `Ok(())`。
    pub(crate) fn signal_force(&self) -> io::Result<()> {
        self.signal(libc::SIGKILL)
    }

    /// グループにまだ生存しているプロセスがあるか。
    ///
    /// `killpg(pgid, 0)` は信号を送らずに存在確認だけを行う。直接の子が既に終了していても、
    /// グループに孫が残っていれば真を返す — これが猶予後に強制段へ移る判断材料になる。
    pub(crate) fn exists(&self) -> bool {
        if unsafe { libc::killpg(self.pgid, 0) } == 0 {
            return true;
        }
        // `EPERM` は「存在するが信号を送る権限が無い」を意味する。存在として扱う。
        io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    fn signal(&self, signal: libc::c_int) -> io::Result<()> {
        if unsafe { libc::killpg(self.pgid, signal) } == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            // 対象が既に存在しない。冪等な成功として扱う（2 回目の終了処理を許す）。
            Some(libc::ESRCH) => Ok(()),
            _ => Err(error),
        }
    }
}

/// 子の終了をブロックして待つ（Unix）。
///
/// `Child::try_wait` をループで叩く（ポーリングする）代わりに `waitpid(2)` で眠る。終了の検出が
/// 即時になり、通知（要件 5.7）が子の死後ただちに届く。待機中に CPU を消費しない。
///
/// **この待機は子を回収する。** 同じ子に対して `Child::try_wait` / `Child::wait` を併用しては
/// ならない（回収済みの子には `ECHILD` を返す）。監督側の生存判定は、この待機が記録する終了
/// 状態（`supervisor` の `ExitState`）だけを見る。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Waiter {
    /// 回収対象の子の識別子。
    pid: libc::pid_t,
}

impl Waiter {
    /// `configure` 済みの、起動直後の子に対する待機を作る。
    pub(crate) fn new(child: &Child) -> io::Result<Self> {
        Ok(Waiter {
            pid: child.id() as libc::pid_t,
        })
    }

    /// 子が終了するまでブロックし、終了状態を返す。
    pub(crate) fn wait(&self) -> io::Result<ExitStatus> {
        use std::os::unix::process::ExitStatusExt;

        loop {
            let mut status: libc::c_int = 0;
            let reaped = unsafe { libc::waitpid(self.pid, &mut status, 0) };
            if reaped == self.pid {
                return Ok(ExitStatus::from_raw(status));
            }
            let error = io::Error::last_os_error();
            // 信号による割り込みは再試行する（`waitpid` は `EINTR` を返しうる）。
            if error.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(error);
        }
    }
}

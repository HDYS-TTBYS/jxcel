//! Windows における Job Object と `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` による終了保証（要件 5.6）。
//!
//! 補助プロセスを Job Object に割り当て、ジョブのハンドルが閉じたときにカーネルが子を終了させる
//! よう設定する。これはアプリケーション側が異常終了して `RunEvent::Exit` が発火しない経路でも
//! 有効な唯一の機構である（design.md「SidecarSupervisor」、research.md 決定 5）。孫プロセスは
//! ジョブを継承するため、孫も同じ保証の下に入る。
//!
//! # 猶予と強制
//!
//! Windows に Unix の `SIGTERM` に相当する汎用の穏やかな終了通知は無い。ここではコンソール制御
//! イベント `CTRL_BREAK_EVENT` を子のプロセスグループ宛に送るのを穏やかな段とし、強制段は
//! [`TerminateJobObject`] とする。GUI アプリ（コンソールを持たない）では
//! `GenerateConsoleCtrlEvent` が失敗するが、その場合でも強制段がグループ全体を確実に終了させる。
//! 段階の制御は [`super::supervisor`] の `terminate` が行う。
//!
//! # 起動と割り当ての間に残る窓（正直に記載する）
//!
//! `CreateProcess` の直後に `AssignProcessToJobObject` で子をジョブへ割り当てる。この 2 つの間に
//! 子が孫を起動すると、その孫はジョブの外に残りうる（割り当ては孫の生成に間に合わない）。
//! `CREATE_SUSPENDED` で止めて割り当ててから再開する方法なら窓は閉じられるが、`std::process`
//! は子のスレッドハンドルを公開しないため再開できない。したがって**この窓は残る**。実際上は
//! `spawn` が戻った直後に割り当てを行うため、窓は数マイクロ秒に限られ、検証用の補助プロセスは
//! 起動直後に孫を作らない。加えて、この窓の外で生まれた孫はジョブを継承するので、
//! [`TerminateJobObject`] と `KILL_ON_JOB_CLOSE` が確実に届く。

use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Console::{GenerateConsoleCtrlEvent, CTRL_BREAK_EVENT};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;

/// 子を新しいプロセスグループで起動する。`spawn` の直前に呼ぶ。
///
/// `CREATE_NEW_PROCESS_GROUP` により、子の PID がそのままプロセスグループの識別子になり、
/// `CTRL_BREAK_EVENT` を子とその子孫（同じグループを継承する）へ届けられる。
pub(crate) fn configure(command: &mut Command) {
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

/// 起動した補助プロセスを収めた Job Object。
///
/// ハンドルを保持し続けることが終了保証の一部である。この値が最後に破棄されてハンドルが閉じた
/// 時点で、`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` によりカーネルがジョブ内の全プロセスを終了させる
/// （アプリが異常終了して `Drop` を通らない場合も、OS がハンドルを閉じるため同じ効果が得られる）。
pub(crate) struct Group {
    job: HANDLE,
    /// 子のプロセス識別子。`CREATE_NEW_PROCESS_GROUP` によりプロセスグループの識別子でもある。
    pid: u32,
}

// `HANDLE` は生ポインタだが、カーネルオブジェクトの識別子にすぎず、複数スレッドから参照しても
// 安全である（Job Object の API 自体がスレッドセーフ）。`Arc<Group>` を監督間で共有するために
// `Send` / `Sync` を与える。
unsafe impl Send for Group {}
unsafe impl Sync for Group {}

impl std::fmt::Debug for Group {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Group")
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

impl Group {
    /// 子を新しい Job Object に割り当てる。`configure` 済みの子に対して呼ぶ。
    pub(crate) fn attach(child: &Child) -> io::Result<Self> {
        // 1. ジョブを作る。
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }

        // 2. ハンドルが閉じたときにジョブ内の全プロセスを終了させる設定を与える。
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            let error = io::Error::last_os_error();
            unsafe { CloseHandle(job) };
            return Err(error);
        }

        // 3. 子を割り当てる。ここまでの間に子が孫を作ると、その孫はジョブの外に残りうる
        //    （モジュール冒頭の「起動と割り当ての間に残る窓」を参照）。
        let process = child.as_raw_handle() as HANDLE;
        if unsafe { AssignProcessToJobObject(job, process) } == 0 {
            let error = io::Error::last_os_error();
            unsafe { CloseHandle(job) };
            return Err(error);
        }

        Ok(Group {
            job,
            pid: child.id(),
        })
    }

    /// コンソール制御イベント `CTRL_BREAK_EVENT` を子のプロセスグループ宛に送る。
    ///
    /// コンソールを持たないアプリでは失敗する。呼び出し側（`terminate`）はこの失敗を無視し、
    /// 猶予の後に強制段へ移る。
    pub(crate) fn signal_graceful(&self) -> io::Result<()> {
        if unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, self.pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// ジョブ内の全プロセスを強制終了する。
    pub(crate) fn signal_force(&self) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.job, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// ジョブにまだ生存しているプロセスがあるか。直接の子が終了していても、孫が残っていれば真。
    pub(crate) fn exists(&self) -> bool {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        let queried = unsafe {
            QueryInformationJobObject(
                self.job,
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut core::ffi::c_void,
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };
        // 問い合わせに失敗した場合は「生存していない」に倒す。強制段は冪等であり、
        // 誤って強制を打っても副作用が無いためである。
        if queried == 0 {
            return false;
        }
        info.ActiveProcesses > 0
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        // ハンドルを閉じる。`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` により、閉じた時点でジョブ内の
        // プロセスはカーネルによって終了させられる（明示的な強制段を経なかった場合の backstop）。
        unsafe { CloseHandle(self.job) };
    }
}

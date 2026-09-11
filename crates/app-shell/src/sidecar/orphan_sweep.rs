//! 起動時の残留プロセス掃除（要件 5.6）。
//!
//! 前回の実行が終了処理を走らせられずに終わった場合に残った補助プロセスを探して終了させる。
//! 識別子（PID）と実行ファイル名の**両方**で照合し、PID の再利用による無関係なプロセスの
//! 誤終了を避ける（design.md「SidecarSupervisor」、research.md 決定 5）。実行ファイル名の照合は
//! [`SidecarKind`] の文字列表現を共有する。
//!
//! # このモジュールが持つもの
//!
//! プラットフォーム別の**列挙・同定・終了**だけである。どのプロセスを対象にするかの判断
//! （登録簿に載っている子を除く・期待するパスに属することを要求する・猶予を与えてから強制へ
//! 移る）は [`super::supervisor`] の `sweep_orphans` が持つ。分けている理由は、判断の規則を
//! OS ごとの実装の違いから独立に読めるようにするためである。
//!
//! # 列挙の機構（プラットフォーム別。ここでは実行できないものも正直に記す）
//!
//! - **Linux**: `/proc` を直接読む。外部プロセスを起動しない（tasks.md 3.5 の制約）。各
//!   `/proc/<pid>/exe` のシンボリックリンクが、そのプロセスの実行ファイルの絶対パスを与える。
//! - **macOS**: `/proc` は存在しない。`ps -axo pid=,comm=` を使う。`libc` の `sysctl`
//!   (`KERN_PROC`) + `proc_pidpath` の経路も選べるが、`kinfo_proc` のレイアウトを Rust の
//!   `unsafe` で写す必要があり、**この環境では実行して確かめられない**。`ps` は文書化された
//!   安定したインタフェースであり、同ファイルのテスト補助（`pgrep` / `ps`）と同じ前提に立つ。
//!   よって `ps` を選ぶ。**macOS の分岐はコンパイル検証のみで、実行検証はできていない。**
//! - **Windows**: Toolhelp32 のスナップショット (`CreateToolhelp32Snapshot` / `Process32FirstW` /
//!   `Process32NextW`) で識別子とイメージ名を列挙する。`windows-sys` の feature は
//!   `Win32_System_Diagnostics_ToolHelp` を足すだけで済む。
//!
//! # 列挙と終了の間の窓をどう閉じるか
//!
//! 列挙した時点の名前と、信号を送る直前の名前は一致するとは限らない（列挙した識別子が
//! 再利用されうる）。したがって [`resolve_executable`] が**その識別子の実行ファイルをその場で
//! 解決し直し**、[`supervisor`](super::supervisor) が名前を再照合してから信号を送る。この
//! 再解決があるため、このモジュールは「列挙結果」と「終了対象」を別々に扱う。
//!
//! # 終了の手段
//!
//! 掃除の対象は**前回の実行が起動したプロセス**であり、この実行のプロセスグループにも
//! Job Object にも入っていない。したがって `killpg` / `TerminateJobObject` は使えない
//! （`killpg` に至っては、前回の起動時に `setpgid` を通っていないプロセスでは無関係な
//! グループを巻き込みうる）。識別子を直接指定して終了させる:
//!
//! - Unix: [`terminate_graceful`] は `SIGTERM`、[`terminate_force`] は `SIGKILL`。
//! - Windows: Windows に Unix の `SIGTERM` に相当する汎用の穏やかな終了通知は無く、前回の
//!   実行が作ったプロセスグループのコンソールも当てにできない。`TerminateProcess` で直接
//!   終了させる（穏やかな段は無い。猶予段の意味が無いため、[`supervisor`](super::supervisor)
//!   は Windows では猶予を待たずに強制へ進む）。

use std::path::{Path, PathBuf};

use super::SidecarKind;

/// 走査で見つけた 1 プロセス。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunningProcess {
    /// プロセス識別子。
    pub(crate) pid: u32,
    /// 実行ファイルの名前、またはパス。列挙の実装が何を取得できるかで決まる（Linux は絶対パス、
    /// Windows の Toolhelp32 はイメージ名だけ、macOS の `ps comm` は起動時のパス）。
    pub(crate) executable_name: PathBuf,
}

/// 実行中のプロセスを列挙する。取得できない場合は空を返す（掃除は最善努力である）。
#[cfg(target_os = "linux")]
pub(crate) fn running_processes() -> Vec<RunningProcess> {
    let mut processes = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return processes;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        if pid == 0 {
            continue;
        }
        // `/proc/<pid>/exe` を読めない場合は対象から外す。ゾンビ（リンクが空）もここで外れる。
        if let Some(executable) = resolve_executable(pid) {
            processes.push(RunningProcess {
                pid,
                executable_name: executable,
            });
        }
    }
    processes
}

/// macOS には `/proc` が無いため `ps` を使う（モジュール冒頭の「列挙の機構」を参照）。
#[cfg(target_os = "macos")]
pub(crate) fn running_processes() -> Vec<RunningProcess> {
    let Some(stdout) = ps(&["-axo", "pid=,comm="]) else {
        return Vec::new();
    };
    let mut processes = Vec::new();
    for line in stdout.lines() {
        let line = line.trim_start();
        let Some((pid, name)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let Ok(pid) = pid.trim().parse::<u32>() else {
            continue;
        };
        let name = name.trim();
        if pid == 0 || name.is_empty() {
            continue;
        }
        processes.push(RunningProcess {
            pid,
            executable_name: PathBuf::from(name),
        });
    }
    processes
}

/// Windows は Toolhelp32 のスナップショットで列挙する。
#[cfg(windows)]
pub(crate) fn running_processes() -> Vec<RunningProcess> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }

    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut processes = Vec::new();
    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) };
    while has_entry != 0 {
        if entry.th32ProcessID != 0 {
            processes.push(RunningProcess {
                pid: entry.th32ProcessID,
                executable_name: PathBuf::from(wide_to_string(&entry.szExeFile)),
            });
        }
        has_entry = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    unsafe { CloseHandle(snapshot) };
    processes
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(crate) fn running_processes() -> Vec<RunningProcess> {
    // 対象は Linux / macOS / Windows である。それ以外では列挙の手段を持たないため、
    // 掃除は何もしない（起動を止めない。要件 5.4 の精神）。
    Vec::new()
}

/// 指定した識別子の実行ファイルを、**いまこの瞬間の状態で**解決する。
///
/// 列挙（[`running_processes`]）と信号の間で識別子が再利用されていないかを確かめるために使う。
/// 取得できない場合（既に存在しない・権限が無い）は `None`。
#[cfg(target_os = "linux")]
pub(crate) fn resolve_executable(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[cfg(target_os = "macos")]
pub(crate) fn resolve_executable(pid: u32) -> Option<PathBuf> {
    let stdout = ps(&["-p", &pid.to_string(), "-o", "comm="])?;
    let name = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    Some(PathBuf::from(name))
}

/// Windows は Toolhelp32 のスナップショットが与えるのはイメージ名だけなので、フルパスは
/// `QueryFullProcessImageNameW` で解決する。
#[cfg(windows)]
pub(crate) fn resolve_executable(pid: u32) -> Option<PathBuf> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }

    // パスの上限は Windows の長いパスでも収まる 32 KiB 文字（`MAX_PATH` に縛られない）。
    let mut buffer = vec![0u16; 32768];
    let mut length = buffer.len() as u32;
    let resolved =
        unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) };
    unsafe { CloseHandle(handle) };
    if resolved == 0 {
        return None;
    }
    Some(PathBuf::from(wide_to_string(&buffer[..length as usize])))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(crate) fn resolve_executable(_pid: u32) -> Option<PathBuf> {
    None
}

/// 指定した識別子のプロセスが、まだ実行中か。**ゾンビは実行中と見なさない。**
///
/// ゾンビを生存と扱うと、終了させたのに猶予時間を満杯まで待ってしまう。掃除が対象にするのは
/// 実行中のプロセスなので、判定の意味をそこに揃える。
#[cfg(target_os = "linux")]
pub(crate) fn is_alive(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    // `pid (comm) state ...` — comm は空白や括弧を含みうるので、最後の `)` の次を見る。
    let Some((_, rest)) = stat.rsplit_once(')') else {
        return false;
    };
    !matches!(
        rest.trim_start().chars().next(),
        None | Some('Z') | Some('X')
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn is_alive(pid: u32) -> bool {
    let Some(stdout) = ps(&["-p", &pid.to_string(), "-o", "stat="]) else {
        return false;
    };
    match stdout.trim().chars().next() {
        Some(state) => !matches!(state, 'Z' | 'X'),
        None => false,
    }
}

#[cfg(windows)]
pub(crate) fn is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let mut code: u32 = 0;
    let queried = unsafe { GetExitCodeProcess(handle, &mut code) };
    unsafe { CloseHandle(handle) };
    queried != 0 && code == STILL_ACTIVE as u32
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(crate) fn is_alive(_pid: u32) -> bool {
    false
}

/// 穏やかな終了（Unix は `SIGTERM`）を識別子へ直接送る。
///
/// 既に存在しない場合は失敗として返す（数え上げに含めないため）。対象がゾンビの場合は成功する
/// が、[`is_alive`] が偽を返すため猶予を待たずに済む。
#[cfg(unix)]
pub(crate) fn terminate_graceful(pid: u32) -> std::io::Result<()> {
    signal(pid, libc::SIGTERM)
}

/// 強制終了（Unix は `SIGKILL`）を識別子へ直接送る。
#[cfg(unix)]
pub(crate) fn terminate_force(pid: u32) -> std::io::Result<()> {
    signal(pid, libc::SIGKILL)
}

#[cfg(unix)]
fn signal(pid: u32, signal: libc::c_int) -> std::io::Result<()> {
    if unsafe { libc::kill(pid as libc::pid_t, signal) } == 0 {
        return Ok(());
    }
    Err(std::io::Error::last_os_error())
}

/// Windows は `TerminateProcess` で直接終了させる（モジュール冒頭の「終了の手段」を参照）。
#[cfg(windows)]
pub(crate) fn terminate_graceful(pid: u32) -> std::io::Result<()> {
    terminate(pid)
}

/// Windows は `TerminateProcess` で直接終了させる。
///
/// Windows には穏やかな段が無い（[`terminate_graceful`] も同じ `TerminateProcess` を呼ぶ）ため、
/// この関数は Unix でのみ呼ばれる。呼び出し側は猶予の後に強制段としてこれを呼ぶが、Windows では
/// 最初の呼び出しで既に終了している。
#[cfg(windows)]
#[allow(dead_code)]
pub(crate) fn terminate_force(pid: u32) -> std::io::Result<()> {
    terminate(pid)
}

#[cfg(windows)]
fn terminate(pid: u32) -> std::io::Result<()> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let terminated = unsafe { TerminateProcess(handle, 1) };
    let error = std::io::Error::last_os_error();
    unsafe { CloseHandle(handle) };
    if terminated == 0 {
        return Err(error);
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn terminate_graceful(_pid: u32) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn terminate_force(_pid: u32) -> std::io::Result<()> {
    Ok(())
}

/// 実行ファイルの名前から、このアプリが同梱する補助プロセスの種類を識別する。
///
/// **照合は語幹の一致までとする**。実行ファイル名は配置の形によって変わる — Linux の配布物は
/// `sidecar-smoke`、配置規約（tasks.md 1.7）の原本と Windows / macOS の `externalBin` は
/// `sidecar-smoke-<ターゲットトリプル>[.exe]` である。したがって「語幹と等しい」または
/// 「語幹 + `-` で始まる」を一致とする。[`SidecarKind::parse`] が厳密一致に限るのとは役割が
/// 違い、こちらは**名前から種類を推測する**ための入口である（曖昧さは、8.1 が与える
/// 期待パスとの照合が締める）。
pub(crate) fn identify(executable: &Path) -> Option<SidecarKind> {
    let name = executable.file_name()?.to_str()?;
    // Windows の同梱名は `.exe` を持つ。大文字小文字は区別しない。
    let stem = name
        .strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".EXE"))
        .unwrap_or(name);
    SidecarKind::ALL
        .iter()
        .copied()
        .find(|kind| is_named_as(stem, kind.as_str()))
}

/// 実行ファイル名がその種類の語幹を持つか。
fn is_named_as(name: &str, stem: &str) -> bool {
    name == stem
        || name
            .strip_prefix(stem)
            .is_some_and(|rest| rest.starts_with('-'))
}

/// Windows の UTF-16（NUL 終端）を `String` へ変換する。不正な並びは置換文字にする。
#[cfg(windows)]
fn wide_to_string(wide: &[u16]) -> String {
    let end = wide
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

/// `ps` を引数付きで実行し、成功したときだけ標準出力を返す（macOS 専用）。
#[cfg(target_os = "macos")]
fn ps(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("ps").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

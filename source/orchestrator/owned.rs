//! One no-window, kill-on-close process tree with a cancellable monotonic limit.
use crate::Result;
use bridgeforge_core::process::process_tree::ProcessTree;
use std::{
    process::{ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Instant,
};

pub(crate) struct OwnedProcess {
    tree: Arc<Mutex<ProcessTree>>,
    pid: u32,
    identity: Option<String>,
    _deadline: Option<DeadlineGuard>,
}
pub(crate) struct Pipes {
    pub input: ChildStdin,
    pub output: ChildStdout,
    pub errors: ChildStderr,
}
impl OwnedProcess {
    pub fn spawn(command: &mut Command, deadline: Option<Instant>) -> Result<Self> {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Err("time limit reached before spawn".into());
        }
        let tree = ProcessTree::spawn(command).map_err(|e| e.to_string())?;
        let pid = tree.child.id();
        #[cfg(windows)]
        let identity = {
            use std::os::windows::io::AsRawHandle;
            handle_identity(tree.child.as_raw_handle())?
        };
        #[cfg(not(windows))]
        let identity = None;
        let tree = Arc::new(Mutex::new(tree));
        let guard = deadline.map(|d| DeadlineGuard::start(&tree, d));
        Ok(Self {
            tree,
            pid,
            identity,
            _deadline: guard,
        })
    }
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn identity(&self) -> Option<&str> {
        self.identity.as_deref()
    }
    pub fn pipes(&self) -> Result<Pipes> {
        let mut tree = self
            .tree
            .lock()
            .map_err(|_| "owned process lock poisoned")?;
        Ok(Pipes {
            input: tree.child.stdin.take().ok_or("missing child stdin")?,
            output: tree.child.stdout.take().ok_or("missing child stdout")?,
            errors: tree.child.stderr.take().ok_or("missing child stderr")?,
        })
    }
    pub fn try_wait(&self) -> Result<Option<ExitStatus>> {
        self.tree
            .lock()
            .map_err(|_| "owned process lock poisoned")?
            .child
            .try_wait()
            .map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
fn handle_identity(handle: windows_sys::Win32::Foundation::HANDLE) -> Result<Option<String>> {
    use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetProcessTimes};
    let mut created = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    if unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) } == 0 {
        return Err(format!(
            "cannot query process birth identity: {}",
            std::io::Error::last_os_error()
        ));
    }
    if exited.dwLowDateTime != 0 || exited.dwHighDateTime != 0 {
        return Ok(None);
    }
    let birth = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    Ok(Some(format!("win-filetime-{birth:016x}")))
}

pub(crate) fn current_identity() -> Result<Option<String>> {
    #[cfg(windows)]
    {
        handle_identity(unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() })
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

/// A process snapshot exposes image names even for protected processes whose
/// full path or birth time cannot be opened. Query only; never terminate by PID.
pub(crate) fn image_name(pid: u32) -> Result<Option<String>> {
    #[cfg(windows)]
    {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            System::Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
        };
        let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(format!(
                "process snapshot unavailable: {}",
                std::io::Error::last_os_error()
            ));
        }
        let _owned = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut available = unsafe { Process32FirstW(handle, &mut entry) };
        while available != 0 {
            if entry.th32ProcessID == pid {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .ok_or("unterminated process image")?;
                let name =
                    String::from_utf16(&entry.szExeFile[..end]).map_err(|e| e.to_string())?;
                if name.is_empty() {
                    return Err("empty process image".into());
                }
                return Ok(Some(name));
            }
            available = unsafe { Process32NextW(handle, &mut entry) };
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(18) {
            return Err("incomplete process snapshot".into());
        }
        Ok(None)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err("legacy process image proof is only supported on Windows".into())
    }
}

/// Query only. A reused numeric PID is never treated as the original owner.
pub(crate) fn same_process(pid: u32, expected: &str) -> Result<bool> {
    #[cfg(windows)]
    {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            let error = std::io::Error::last_os_error();
            return if error.raw_os_error() == Some(87) {
                Ok(false)
            } else {
                Err(format!("cannot prove prior owner identity: {error}"))
            };
        }
        let _owned = unsafe { OwnedHandle::from_raw_handle(handle) };
        Ok(handle_identity(handle)?.as_deref() == Some(expected))
    }
    #[cfg(not(windows))]
    {
        let _ = (pid, expected);
        Err("process birth identity is not supported on this host".into())
    }
}

#[cfg(all(test, windows))]
mod identity_tests {
    use super::*;
    #[test]
    fn birth_identity_distinguishes_a_reused_numeric_pid() {
        let current = current_identity().unwrap().unwrap();
        assert!(same_process(std::process::id(), &current).unwrap());
        assert!(!same_process(std::process::id(), "win-filetime-0000000000000000").unwrap());
    }
}

// Weak ownership never keeps a finished process tree alive. Killing uses the
// original owned Job/process group, never a PID which might have been reused.
struct DeadlineGuard {
    signal: Arc<(Mutex<bool>, Condvar)>,
}
impl DeadlineGuard {
    fn start(tree: &Arc<Mutex<ProcessTree>>, deadline: Instant) -> Self {
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let wake = signal.clone();
        let owned = Arc::downgrade(tree);
        thread::spawn(move || {
            let (cancelled, changed) = &*wake;
            let lock = cancelled.lock().unwrap_or_else(|e| e.into_inner());
            let (cancelled, _) = changed
                .wait_timeout_while(
                    lock,
                    deadline.saturating_duration_since(Instant::now()),
                    |v| !*v,
                )
                .unwrap_or_else(|e| e.into_inner());
            if !*cancelled {
                drop(cancelled);
                if let Some(tree) = owned.upgrade() {
                    let _ = tree.lock().unwrap_or_else(|e| e.into_inner()).terminate();
                }
            }
        });
        Self { signal }
    }
}
impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        let (cancelled, changed) = &*self.signal;
        *cancelled.lock().unwrap_or_else(|e| e.into_inner()) = true;
        changed.notify_all();
    }
}

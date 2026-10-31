//! Launch the opaque snapshot worker without keeping its hook's pipes open.
use std::ffi::OsString;
use std::io;
use std::path::Path;

#[cfg(not(windows))]
pub fn spawn(executable: &Path, args: &[OsString]) -> io::Result<u32> {
    use std::process::{Command, Stdio};
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Ok(command.spawn()?.id())
}

#[cfg(windows)]
pub fn spawn(executable: &Path, args: &[OsString]) -> io::Result<u32> {
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, DETACHED_PROCESS, PROCESS_INFORMATION, STARTUPINFOW,
    };

    let mut application: Vec<u16> = executable.as_os_str().encode_wide().collect();
    if !executable.is_absolute() || application.contains(&0) || application.contains(&34) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "worker executable must be an absolute native path",
        ));
    }
    let mut command_line = vec![34];
    command_line.extend_from_slice(&application);
    command_line.push(34);
    for arg in args {
        command_line.push(32);
        append_argument(&mut command_line, arg)?;
    }
    command_line.push(0);
    if command_line.len() > 32767 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "worker command line is too long",
        ));
    }
    application.push(0);
    // No standard handles: this worker communicates only through durable state.
    // std::Command's null stdio still inherits unrelated inheritable pipe copies.
    // Do not break away from the parent's job or change its cancellation policy.
    let mut startup: STARTUPINFOW = unsafe { zeroed() };
    startup.cb = size_of::<STARTUPINFOW>() as u32;
    let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
    // SAFETY: terminated application, writable terminated command line and valid
    // Win32 structures live throughout this call. FALSE disables all inheritance;
    // null environment/cwd preserve the caller's values, with no shell involved.
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            DETACHED_PROCESS,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: success returned two newly owned handles. Closing them does not
    // terminate the worker; its own durable reservation tracks its lifecycle.
    let _process = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
    let _thread = unsafe { OwnedHandle::from_raw_handle(process.hThread) };
    Ok(process.dwProcessId)
}

#[cfg(windows)]
fn append_argument(line: &mut Vec<u16>, arg: &OsString) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    line.push(34);
    let mut slashes = 0;
    for unit in arg.encode_wide() {
        match unit {
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "worker argument contains NUL",
                ));
            }
            92 => {
                slashes += 1;
                continue;
            }
            34 => {
                line.extend(std::iter::repeat_n(92, slashes * 2 + 1));
                line.push(unit);
            }
            _ => {
                line.extend(std::iter::repeat_n(92, slashes));
                line.push(unit);
            }
        }
        slashes = 0;
    }
    line.extend(std::iter::repeat_n(92, slashes * 2));
    line.push(34);
    Ok(())
}

#[cfg(all(test, windows, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_memory_background.rs"]
mod tests;

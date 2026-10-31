//! Read-only Windows process context for cleanup preflight. This is a bounded
//! activity snapshot, not an inter-process exclusion protocol.
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    mem::{offset_of, size_of, zeroed},
    os::windows::{
        ffi::OsStringExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
};
use windows_sys::{
    Wdk::System::Threading::{
        NtQueryInformationProcess, ProcessBasicInformation, ProcessWow64Information,
    },
    Win32::{
        Foundation::{FILETIME, HANDLE, INVALID_HANDLE_VALUE, LocalFree},
        Security::{GetLengthSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser},
        System::{
            Diagnostics::{Debug::ReadProcessMemory, ToolHelp::*},
            Threading::*,
        },
        UI::Shell::CommandLineToArgvW,
    },
};

// Layout source, pinned and verified 2026-10-30:
// https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntrtl.h#L3985
// https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntwow64.h#L212
// Integer addresses are never dereferenced locally. Native PEB is the SDK type.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct Text<P> {
    length: u16,
    maximum: u16,
    buffer: P,
}
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct Directory<P> {
    path: Text<P>,
    handle: P,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Parameters<P> {
    maximum_length: u32,
    length: u32,
    flags: u32,
    debug_flags: u32,
    console_handle: P,
    console_flags: u32,
    standard_input: P,
    standard_output: P,
    standard_error: P,
    current_directory: Directory<P>,
    dll_path: Text<P>,
    image_path: Text<P>,
    command_line: Text<P>,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Peb32 {
    inherited_address_space: u8,
    read_image_file_exec_options: u8,
    being_debugged: u8,
    bit_field: u8,
    mutant: u32,
    image_base_address: u32,
    ldr: u32,
    process_parameters: u32,
}

fn error() -> String {
    std::io::Error::last_os_error().to_string()
}
fn open(pid: u32, access: u32) -> Result<OwnedHandle, String> {
    let raw = unsafe { OpenProcess(access, 0, pid) };
    if raw.is_null() {
        Err(error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
    }
}
fn read<T: Copy>(process: HANDLE, address: usize) -> Result<T, String> {
    if address == 0 || address.checked_add(size_of::<T>()).is_none() {
        return Err("invalid process address".into());
    }
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    let mut count = 0;
    if unsafe {
        ReadProcessMemory(
            process,
            address as _,
            value.as_mut_ptr().cast(),
            size_of::<T>(),
            &mut count,
        )
    } == 0
        || count != size_of::<T>()
    {
        return Err(error());
    }
    Ok(unsafe { value.assume_init() })
}
fn string(process: HANDLE, text: Text<usize>) -> Result<OsString, String> {
    if text.length > text.maximum || text.length % 2 != 0 || text.maximum % 2 != 0 {
        return Err("invalid process string length".into());
    }
    if text.length == 0 {
        return Ok(OsString::new());
    }
    if text.buffer == 0 {
        return Err("null process string".into());
    }
    let mut value = vec![0u16; usize::from(text.length) / 2];
    let mut count = 0;
    if unsafe {
        ReadProcessMemory(
            process,
            text.buffer as _,
            value.as_mut_ptr().cast(),
            usize::from(text.length),
            &mut count,
        )
    } == 0
        || count != usize::from(text.length)
    {
        return Err(error());
    }
    if value.contains(&0) {
        return Err("embedded null in process string".into());
    }
    Ok(OsString::from_wide(&value))
}
fn native(value: Text<u32>) -> Text<usize> {
    Text {
        length: value.length,
        maximum: value.maximum,
        buffer: value.buffer as usize,
    }
}

fn context(process: HANDLE) -> Result<(PathBuf, OsString), String> {
    if size_of::<usize>() != 8 {
        return Err("cleanup process-context inspection requires native 64-bit Windows".into());
    }
    let mut basic: PROCESS_BASIC_INFORMATION = unsafe { zeroed() };
    let mut wow = 0usize;
    if unsafe {
        NtQueryInformationProcess(
            process,
            ProcessBasicInformation,
            (&mut basic as *mut PROCESS_BASIC_INFORMATION).cast(),
            size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            std::ptr::null_mut(),
        )
    } < 0
        || unsafe {
            NtQueryInformationProcess(
                process,
                ProcessWow64Information,
                (&mut wow as *mut usize).cast(),
                size_of::<usize>() as u32,
                std::ptr::null_mut(),
            )
        } < 0
    {
        return Err("cannot query process parameter identity".into());
    }
    let descriptors = || -> Result<(usize, Text<usize>, Text<usize>), String> {
        if wow != 0 {
            let address = read::<u32>(
                process,
                wow.checked_add(offset_of!(Peb32, process_parameters))
                    .ok_or("process address overflow")?,
            )? as usize;
            let values = read::<Parameters<u32>>(process, address)?;
            if (values.length as usize) < size_of::<Parameters<u32>>()
                || values.length > values.maximum_length
            {
                return Err("unsupported WOW64 process parameters".into());
            }
            Ok((
                address,
                native(values.current_directory.path),
                native(values.command_line),
            ))
        } else {
            let address = read::<usize>(
                process,
                (basic.PebBaseAddress as usize)
                    .checked_add(offset_of!(PEB, ProcessParameters))
                    .ok_or("process address overflow")?,
            )?;
            let values = read::<Parameters<usize>>(process, address)?;
            if (values.length as usize) < size_of::<Parameters<usize>>()
                || values.length > values.maximum_length
            {
                return Err("unsupported native process parameters".into());
            }
            Ok((address, values.current_directory.path, values.command_line))
        }
    };
    let first = descriptors()?;
    let cwd = string(process, first.1)?;
    let command = string(process, first.2)?;
    let second = descriptors()?;
    if first != second || cwd != string(process, second.1)? || command != string(process, second.2)?
    {
        return Err("process context changed while inspected".into());
    }
    let cwd = PathBuf::from(cwd);
    if !cwd.is_absolute() {
        return Err("process working directory is not absolute".into());
    }
    Ok((cwd, command))
}

fn user_sid(process: HANDLE) -> Result<Vec<u8>, String> {
    let mut raw = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut raw) } == 0 {
        return Err(error());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut length = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut length,
        );
    }
    if length == 0 || length > 65536 {
        return Err("invalid process token length".into());
    }
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return Err(error());
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let size = unsafe { GetLengthSid(user.User.Sid) } as usize;
    if size == 0 || size > 256 {
        return Err("invalid process user SID".into());
    }
    Ok(unsafe { std::slice::from_raw_parts(user.User.Sid.cast::<u8>(), size) }.to_vec())
}
fn birth(process: HANDLE) -> Result<u64, String> {
    let mut created: FILETIME = unsafe { zeroed() };
    let mut exit = created;
    let mut kernel = created;
    let mut user = created;
    if unsafe { GetProcessTimes(process, &mut created, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(error());
    }
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}
fn exited(process: HANDLE) -> bool {
    let mut code = 259;
    unsafe { GetExitCodeProcess(process, &mut code) != 0 && code != 259 }
}
fn command_paths(command: &std::ffi::OsStr) -> Result<Vec<PathBuf>, String> {
    use std::os::windows::ffi::OsStrExt;
    let text = command.encode_wide().chain(Some(0)).collect::<Vec<_>>();
    let mut count = 0;
    let pointers = unsafe { CommandLineToArgvW(text.as_ptr(), &mut count) };
    if pointers.is_null() {
        return Err(error());
    }
    let mut result = Vec::new();
    for index in 0..count {
        let ptr = unsafe { *pointers.add(index as usize) };
        let mut length = 0;
        while unsafe { *ptr.add(length) } != 0 {
            length += 1;
        }
        let arg = OsString::from_wide(unsafe { std::slice::from_raw_parts(ptr, length) });
        let text = arg.to_string_lossy();
        let path = PathBuf::from(if text.starts_with('-') {
            text.split_once('=')
                .map_or(text.as_ref(), |(_, value)| value)
        } else {
            text.as_ref()
        });
        if path.is_absolute() {
            result.push(path);
        }
    }
    unsafe {
        LocalFree(pointers.cast());
    }
    Ok(result)
}

fn task_candidate(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let stem = name.strip_suffix(".exe").unwrap_or(&name);
    stem.starts_with("python")
        || stem.starts_with("bridgeforge")
        || matches!(
            stem,
            "node"
                | "nodejs"
                | "powershell"
                | "pwsh"
                | "cmd"
                | "bash"
                | "sh"
                | "wsl"
                | "cargo"
                | "rustc"
                | "rustup"
                | "dotnet"
                | "msbuild"
                | "ninja"
                | "cmake"
                | "make"
                | "cl"
                | "link"
                | "java"
                | "javaw"
                | "go"
                | "ruby"
                | "perl"
                | "rscript"
        )
}

fn reference_within(path: &Path, root: &Path) -> Result<bool, String> {
    use std::path::Component;
    // Collapse lexical parents before finding an existing ancestor. Both sides
    // then use the canonical volume prefix, including Windows verbatim paths.
    let mut probe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => (),
            Component::ParentDir => {
                if !probe.pop() {
                    return Err("process path escapes its volume".into());
                }
            }
            other => probe.push(other.as_os_str()),
        }
    }
    let mut suffix = Vec::new();
    let mut normalized = loop {
        match probe.canonicalize() {
            Ok(path) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::symlink_metadata(&probe) {
                    Ok(_) => {
                        if crate::persistence::is_link_or_reparse(&probe)
                            .map_err(|e| e.to_string())?
                        {
                            return Err(format!(
                                "unresolved linked process path: {}",
                                probe.display()
                            ));
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Err(error) => return Err(error.to_string()),
                }
                suffix.push(
                    probe
                        .file_name()
                        .ok_or("process path has no existing ancestor")?
                        .to_os_string(),
                );
                if !probe.pop() {
                    return Err("process path has no existing ancestor".into());
                }
            }
            Err(error) => {
                return Err(format!(
                    "cannot resolve process path {}: {error}",
                    probe.display()
                ));
            }
        }
    };
    for component in suffix.into_iter().rev() {
        normalized.push(component);
    }
    Ok(crate::artifact_paths::within(
        &normalized.to_string_lossy(),
        &root.to_string_lossy(),
    ))
}

pub(super) fn project_quiet(root: &Path) -> Result<(), String> {
    let records = snapshot()?;
    inspect_records(root, records)
}

fn snapshot() -> Result<BTreeMap<u32, (u32, String)>, String> {
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(error());
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut valid = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) };
    let mut records = BTreeMap::new();
    while valid != 0 {
        let name = String::from_utf16_lossy(
            &entry.szExeFile[..entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len())],
        );
        records.insert(entry.th32ProcessID, (entry.th32ParentProcessID, name));
        valid = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) };
    }
    let ending = std::io::Error::last_os_error();
    if ending.raw_os_error() != Some(18) {
        return Err(format!("incomplete process snapshot: {ending}"));
    }
    Ok(records)
}

fn inspect_records(root: &Path, records: BTreeMap<u32, (u32, String)>) -> Result<(), String> {
    let own = std::process::id();
    let own_handle = open(own, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let own_sid = user_sid(own_handle.as_raw_handle())?;
    let mut ancestor = BTreeSet::from([own]);
    let mut current = own;
    let mut child_birth = birth(own_handle.as_raw_handle())?;
    while let Some(parent) = records
        .get(&current)
        .map(|r| r.0)
        .filter(|p| *p != 0 && !ancestor.contains(p))
    {
        let Ok(handle) = open(parent, PROCESS_QUERY_LIMITED_INFORMATION) else {
            break;
        };
        let parent_birth = birth(handle.as_raw_handle())?;
        if parent_birth > child_birth {
            break;
        }
        ancestor.insert(parent);
        current = parent;
        child_birth = parent_birth;
    }
    for pid in records
        .keys()
        .copied()
        .filter(|p| *p > 4 && !ancestor.contains(p))
    {
        let candidate = task_candidate(&records[&pid].1);
        let limited = match open(pid, PROCESS_QUERY_LIMITED_INFORMATION) {
            Ok(handle) => handle,
            Err(reason) => {
                if !snapshot()?.contains_key(&pid) {
                    continue;
                }
                if candidate {
                    return Err(format!(
                        "cannot inspect active task process {pid}: {reason}"
                    ));
                }
                continue;
            }
        };
        if exited(limited.as_raw_handle()) {
            continue;
        }
        let sid = match user_sid(limited.as_raw_handle()) {
            Ok(sid) => sid,
            Err(reason) if candidate => {
                return Err(format!(
                    "cannot establish task process {pid} owner: {reason}"
                ));
            }
            Err(_) => continue,
        };
        if sid != own_sid {
            continue;
        }
        let observed = (|| {
            let handle = open(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)?;
            if birth(handle.as_raw_handle())? != birth(limited.as_raw_handle())? {
                return Err("process identity changed".into());
            }
            let (cwd, command) = context(handle.as_raw_handle())?;
            let mut paths = command_paths(&command)?;
            paths.push(cwd);
            Ok::<_, String>(paths)
        })();
        match observed {
            Ok(paths) => {
                for path in paths {
                    let related = match reference_within(&path, root) {
                        Ok(related) => related,
                        Err(reason) if candidate => {
                            return Err(format!(
                                "cannot establish task process {pid} path: {reason}"
                            ));
                        }
                        Err(_) => continue,
                    };
                    if related {
                        return Err(format!(
                            "active process {pid} references project path {}; finish it before cleanup",
                            path.display()
                        ));
                    }
                }
            }
            Err(reason) if candidate && !exited(limited.as_raw_handle()) => {
                return Err(format!(
                    "cannot establish activity of task process {pid}: {reason}"
                ));
            }
            Err(_) => (),
        }
    }
    Ok(())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_process_context.rs"]
mod tests;

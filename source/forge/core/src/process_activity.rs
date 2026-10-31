//! Read-only process checks before retiring legacy executable/cache paths.
use std::path::Path;

#[cfg(windows)]
#[path = "process_context.rs"]
mod context;

#[cfg(windows)]
pub fn project_quiet(root: &Path) -> Result<(), String> {
    context::project_quiet(root)
}

pub fn legacy_clients_quiet(root: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    legacy_clients_quiet_with_images(&root, images()?)
}

fn legacy_clients_quiet_with_images(
    root: &Path,
    images: Vec<(u32, std::path::PathBuf)>,
) -> Result<(), String> {
    let old_bin = root.join(".codex/bin");
    let old_images = root.join(".runtime/bridgeforge-codex/git-sync-images");
    for (pid, image) in images {
        let image = image.canonicalize().unwrap_or(image);
        let in_old = image.starts_with(&old_bin) || image.starts_with(&old_images);
        let managed = matches!(
            image.file_stem().and_then(|s| s.to_str()),
            Some("bridgeforge" | "bridgeforge-factory")
        );
        let current_layout = image.starts_with(root.join(crate::artifact_paths::FACTORY_BIN))
            || image.starts_with(root.join(crate::artifact_paths::PROJECT_BIN));
        if in_old || (pid != std::process::id() && managed && !current_layout) {
            return Err(format!(
                "active legacy-capable BridgeForge process {pid}: {}; finish it before migration",
                image.display()
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
pub fn images() -> Result<Vec<(u32, std::path::PathBuf)>, String> {
    use std::{
        mem::{size_of, zeroed},
        os::windows::{
            ffi::OsStringExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
    };
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
        },
    };
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut valid = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) };
    let mut results = Vec::new();
    while valid != 0 {
        let name = String::from_utf16_lossy(
            &entry.szExeFile[..entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len())],
        );
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID) };
        if !raw.is_null() {
            let process = unsafe { OwnedHandle::from_raw_handle(raw) };
            let mut path = vec![0u16; 32768];
            let mut length = path.len() as u32;
            if unsafe {
                QueryFullProcessImageNameW(
                    process.as_raw_handle(),
                    0,
                    path.as_mut_ptr(),
                    &mut length,
                )
            } != 0
            {
                results.push((
                    entry.th32ProcessID,
                    std::ffi::OsString::from_wide(&path[..length as usize]).into(),
                ));
            } else if name.to_ascii_lowercase().starts_with("bridgeforge") {
                return Err(format!(
                    "cannot inspect active BridgeForge process {}",
                    entry.th32ProcessID
                ));
            }
        } else if name.to_ascii_lowercase().starts_with("bridgeforge") {
            return Err(format!(
                "cannot inspect active BridgeForge process {}",
                entry.th32ProcessID
            ));
        }
        valid = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) };
    }
    let ending = std::io::Error::last_os_error();
    if ending.raw_os_error() != Some(18) {
        return Err(format!("incomplete process image snapshot: {ending}"));
    }
    Ok(results)
}

#[cfg(target_os = "linux")]
pub fn images() -> Result<Vec<(u32, std::path::PathBuf)>, String> {
    let mut results = Vec::new();
    for entry in std::fs::read_dir("/proc").map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if let Ok(image) = std::fs::read_link(entry.path().join("exe")) {
            results.push((pid, image));
        }
    }
    Ok(results)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn images() -> Result<Vec<(u32, std::path::PathBuf)>, String> {
    Err("process activity inspection is unavailable on this platform".into())
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_process_activity.rs"]
mod tests;

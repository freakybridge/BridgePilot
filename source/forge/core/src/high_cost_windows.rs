//! Bounded read-only access through Windows' system SQLite; no extra runtime.
use std::ffi::{CString, c_char, c_int, c_void};
use std::path::Path;
use std::time::{Duration, Instant};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> c_int;
}

type Open = unsafe extern "C" fn(*const c_char, *mut *mut c_void, c_int, *const c_char) -> c_int;
type Close = unsafe extern "C" fn(*mut c_void) -> c_int;
type Prepare = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    c_int,
    *mut *mut c_void,
    *mut *const c_char,
) -> c_int;
type Bind = unsafe extern "C" fn(
    *mut c_void,
    c_int,
    *const c_char,
    c_int,
    Option<unsafe extern "C" fn(*mut c_void)>,
) -> c_int;
type Step = unsafe extern "C" fn(*mut c_void) -> c_int;
type Text = unsafe extern "C" fn(*mut c_void, c_int) -> *const u8;
type Bytes = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type Timeout = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type Progress = unsafe extern "C" fn(
    *mut c_void,
    c_int,
    Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    *mut c_void,
);

struct Api {
    library: *mut c_void,
    open: Open,
    close: Close,
    prepare: Prepare,
    bind: Bind,
    step: Step,
    text: Text,
    bytes: Bytes,
    finalize: Close,
    timeout: Timeout,
    progress: Progress,
}
impl Drop for Api {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.library);
        }
    }
}
impl Api {
    fn load() -> Result<Self, String> {
        let name: Vec<u16> = "winsqlite3.dll\0".encode_utf16().collect();
        let library = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), 0x800) }; // SYSTEM32 only
        if library.is_null() {
            return Err("system SQLite unavailable".into());
        }
        macro_rules! load {
            ($name:literal, $kind:ty) => {{
                let name = CString::new($name).unwrap();
                let address = unsafe { GetProcAddress(library, name.as_ptr()) };
                if address.is_null() {
                    return Err("system SQLite API unavailable".into());
                }
                unsafe { std::mem::transmute::<*mut c_void, $kind>(address) }
            }};
        }
        let result = (|| {
            Ok(Self {
                library,
                open: load!("sqlite3_open_v2", Open),
                close: load!("sqlite3_close_v2", Close),
                prepare: load!("sqlite3_prepare_v2", Prepare),
                bind: load!("sqlite3_bind_text", Bind),
                step: load!("sqlite3_step", Step),
                text: load!("sqlite3_column_text", Text),
                bytes: load!("sqlite3_column_bytes", Bytes),
                finalize: load!("sqlite3_finalize", Close),
                timeout: load!("sqlite3_busy_timeout", Timeout),
                progress: load!("sqlite3_progress_handler", Progress),
            })
        })();
        if result.is_err() {
            unsafe {
                FreeLibrary(library);
            }
        }
        result
    }
}
struct Connection<'a>(&'a Api, *mut c_void);
impl Drop for Connection<'_> {
    fn drop(&mut self) {
        unsafe {
            if !self.1.is_null() {
                (self.0.progress)(self.1, 0, None, std::ptr::null_mut());
            }
            (self.0.close)(self.1);
        }
    }
}
struct Statement<'a>(&'a Api, *mut c_void);
impl Drop for Statement<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.0.finalize)(self.1);
        }
    }
}

unsafe extern "C" fn expired(data: *mut c_void) -> c_int {
    i32::from(unsafe { &*(data as *const Instant) }.elapsed() >= Duration::from_millis(200))
}

pub(super) fn rows(path: &Path, session: &str) -> Result<Vec<String>, String> {
    if !path.is_file() {
        return Err("Codex log database missing".into());
    }
    let api = Api::load()?;
    let filename = CString::new(path.to_str().ok_or("non-Unicode database path")?)
        .map_err(|_| "invalid path")?;
    let mut handle = std::ptr::null_mut();
    let status = unsafe { (api.open)(filename.as_ptr(), &mut handle, 1, std::ptr::null()) }; // READONLY
    let connection = Connection(&api, handle);
    if status != 0 {
        return Err("cannot open Codex logs read-only".into());
    }
    let mut started = Instant::now();
    unsafe {
        (api.timeout)(connection.1, 25);
        (api.progress)(
            connection.1,
            1000,
            Some(expired),
            (&mut started as *mut Instant).cast(),
        );
    }
    let sql = c"SELECT substr(feedback_log_body,1,131072) FROM (SELECT target,feedback_log_body,ts,ts_nanos,id FROM logs WHERE thread_id=?1 ORDER BY ts DESC,ts_nanos DESC,id DESC LIMIT 4096) WHERE target='codex_core::session::handlers' AND feedback_log_body LIKE '%op: TurnInput {%' ORDER BY ts DESC,ts_nanos DESC,id DESC LIMIT 64";
    // SQLITE_STATIC requires the bound text to outlive the prepared statement.
    let session = CString::new(session).map_err(|_| "invalid session")?;
    let mut statement = std::ptr::null_mut();
    let status = unsafe {
        (api.prepare)(
            connection.1,
            sql.as_ptr(),
            -1,
            &mut statement,
            std::ptr::null_mut(),
        )
    };
    let statement = Statement(&api, statement);
    if status != 0 {
        return Err("unsupported Codex log schema".into());
    }
    if unsafe { (api.bind)(statement.1, 1, session.as_ptr(), -1, None) } != 0 {
        return Err("log query binding failed".into());
    }
    let mut output = Vec::new();
    loop {
        if started.elapsed() >= Duration::from_millis(250) {
            return Err("log query deadline".into());
        }
        match unsafe { (api.step)(statement.1) } {
            100 => {
                let size = unsafe { (api.bytes)(statement.1, 0) };
                let text = unsafe { (api.text)(statement.1, 0) };
                if text.is_null() || !(0..=131072).contains(&size) {
                    return Err("invalid log row".into());
                }
                let bytes = unsafe { std::slice::from_raw_parts(text, size as usize) };
                output.push(
                    std::str::from_utf8(bytes)
                        .map_err(|_| "invalid log encoding")?
                        .into(),
                );
            }
            101 => break,
            _ => return Err("Codex log query interrupted or unavailable".into()),
        }
    }
    Ok(output)
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../scripts/tests/unit/core_high_cost_windows.rs"]
mod tests;

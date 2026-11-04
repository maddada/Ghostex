//! The Win32 calls the native Windows start makes directly instead of through PowerShell or CIM.

use std::ffi::{c_void, OsString};
use std::os::windows::ffi::OsStringExt;

const AF_INET: u32 = 2;
const TCP_TABLE_OWNER_PID_LISTENER: u32 = 3;
const NO_ERROR: u32 = 0;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
/// MIB_TCPROW_OWNER_PID: state, local address, local port, remote address, remote port, owning pid.
const TCP_ROW_WORDS: usize = 6;

#[link(name = "iphlpapi")]
extern "system" {
    fn GetExtendedTcpTable(
        table: *mut c_void,
        size: *mut u32,
        order: i32,
        address_family: u32,
        table_class: u32,
        reserved: u32,
    ) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit_handle: i32, pid: u32) -> *mut c_void;
    fn QueryFullProcessImageNameW(
        process: *mut c_void,
        flags: u32,
        name: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn ProcessIdToSessionId(pid: u32, session_id: *mut u32) -> i32;
    fn TerminateProcess(process: *mut c_void, exit_code: u32) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
}

const PROCESS_TERMINATE: u32 = 0x0001;
const SYNCHRONIZE: u32 = 0x0010_0000;
const WAIT_OBJECT_0: u32 = 0;

/// Ends `pid` and waits up to `wait_ms` for it to exit; `true` once it is gone.
pub fn terminate_process(pid: u32, wait_ms: u32) -> bool {
    let process = unsafe { OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid) };
    if process.is_null() {
        // ERROR_INVALID_PARAMETER: no process has this pid any more, so it has already ended. One gxserver listens on two ports and is found twice; ending it the second time must not fail the start.
        return std::io::Error::last_os_error().raw_os_error() == Some(87);
    }
    unsafe { TerminateProcess(process, 1) };
    let waited = unsafe { WaitForSingleObject(process, wait_ms) };
    unsafe { CloseHandle(process) };
    waited == WAIT_OBJECT_0
}

/// The (owning pid, port) of every IPv4 TCP listener bound to 127.0.0.1.
pub fn loopback_listeners() -> std::io::Result<Vec<(u32, u16)>> {
    let mut buffer: Vec<u32> = Vec::new();
    let mut size = 0u32;
    loop {
        let table = if buffer.is_empty() {
            std::ptr::null_mut()
        } else {
            buffer.as_mut_ptr().cast()
        };
        match unsafe {
            GetExtendedTcpTable(
                table,
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        } {
            NO_ERROR if !buffer.is_empty() => break,
            // The table can grow between the size query and the read, so ask again until it fits.
            NO_ERROR | ERROR_INSUFFICIENT_BUFFER => {
                buffer = vec![0; (size as usize).div_ceil(4).max(1)];
            }
            code => return Err(std::io::Error::from_raw_os_error(code as i32)),
        }
    }
    let entries = buffer[0] as usize;
    let rows = buffer[1..].chunks_exact(TCP_ROW_WORDS).take(entries);
    Ok(rows
        .filter(|row| row[1].to_ne_bytes() == [127, 0, 0, 1])
        .map(|row| (row[5], u16::from_be(row[2] as u16)))
        .collect())
}

/// The full Win32 path of a process's executable, or None when it has exited or this token may not query it.
pub fn process_image_path(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut name = vec![0u16; 32 * 1024];
    let mut length = name.len() as u32;
    let queried =
        unsafe { QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut length) } != 0;
    unsafe { CloseHandle(process) };
    queried.then(|| {
        OsString::from_wide(&name[..length as usize])
            .to_string_lossy()
            .into_owned()
    })
}

#[repr(C)]
struct Guid(u32, u16, u16, [u8; 8]);

/// A 24-byte OLE VARIANT holding VT_EMPTY, VT_I4 or VT_BSTR.
#[repr(C)]
struct Variant {
    vt: u16,
    reserved: [u16; 3],
    data: [u64; 2],
}

const VT_EMPTY: u16 = 0;
const VT_I4: u16 = 3;
const VT_BSTR: u16 = 8;

impl Variant {
    fn new(vt: u16, data: u64) -> Self {
        Self {
            vt,
            reserved: [0; 3],
            data: [data, 0],
        }
    }
}

const CLSID_SHELL_WINDOWS: Guid = Guid(
    0x9BA0_5972,
    0xF6A8,
    0x11CF,
    [0xA4, 0x42, 0x00, 0xA0, 0xC9, 0x0A, 0x8F, 0x39],
);
const IID_ISHELL_WINDOWS: Guid = Guid(
    0x85CB_6900,
    0x4D95,
    0x11CF,
    [0x96, 0x0C, 0x00, 0x80, 0xC7, 0xF4, 0xEE, 0x85],
);
const IID_ISERVICE_PROVIDER: Guid = Guid(
    0x6D51_40C1,
    0x7436,
    0x11CE,
    [0x80, 0x34, 0x00, 0xAA, 0x00, 0x60, 0x09, 0xFA],
);
const SID_STOP_LEVEL_BROWSER: Guid = Guid(
    0x4C96_BE40,
    0x915C,
    0x11CF,
    [0x99, 0xD3, 0x00, 0xAA, 0x00, 0x4A, 0xE8, 0x37],
);
const IID_ISHELL_BROWSER: Guid = Guid(0x0002_14E2, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IDISPATCH: Guid = Guid(0x0002_0400, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_ISHELL_FOLDER_VIEW_DUAL: Guid = Guid(
    0xE7A1_AF80,
    0x4D96,
    0x11CF,
    [0x96, 0x0C, 0x00, 0x80, 0xC7, 0xF4, 0xEE, 0x85],
);
const IID_ISHELL_DISPATCH2: Guid = Guid(
    0xA4C6_892C,
    0x3BA9,
    0x11D2,
    [0x9D, 0xEA, 0x00, 0xC0, 0x4F, 0xB1, 0x61, 0x62],
);

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
    fn CoUninitialize();
    fn CoCreateInstance(
        clsid: *const Guid,
        outer: *mut c_void,
        context: u32,
        iid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32;
}

#[link(name = "oleaut32")]
extern "system" {
    fn SysAllocString(text: *const u16) -> *mut u16;
    fn SysFreeString(text: *mut u16);
}

/// An owned COM interface pointer, released on drop.
struct Com(*mut c_void);

impl Com {
    /// The `index`th entry of the object's vtable, cast to the method's signature `F`.
    unsafe fn method<F: Copy>(&self, index: usize) -> F {
        let vtable = *(self.0 as *const *const *const c_void);
        std::mem::transmute_copy(&*vtable.add(index))
    }

    unsafe fn query(&self, iid: &Guid) -> std::io::Result<Com> {
        let query: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
            self.method(0);
        let mut out = std::ptr::null_mut();
        check(query(self.0, iid, &mut out), "QueryInterface")?;
        Ok(Com(out))
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            let release: unsafe extern "system" fn(*mut c_void) -> u32 = self.method(2);
            release(self.0);
        }
    }
}

fn check(hresult: i32, call: &str) -> std::io::Result<()> {
    if hresult < 0 {
        return Err(std::io::Error::other(format!(
            "{call} failed with HRESULT 0x{:08X}",
            hresult as u32
        )));
    }
    Ok(())
}

struct Bstr(*mut u16);

impl Bstr {
    fn new(text: &std::ffi::OsStr) -> Self {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = text.encode_wide().chain(Some(0)).collect();
        Self(unsafe { SysAllocString(wide.as_ptr()) })
    }
}

impl Drop for Bstr {
    fn drop(&mut self) {
        unsafe { SysFreeString(self.0) };
    }
}

/// Starts `file` the way the Start menu does: the running Explorer shell creates the process (IShellDispatch2::ShellExecute on the desktop's shell view), so it belongs to no job of this process's callers, inherits none of their handles, and gets the signed-in user's environment.
pub fn shell_execute_from_desktop(
    file: &std::path::Path,
    arguments: &str,
    directory: &std::path::Path,
) -> std::io::Result<()> {
    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const CLSCTX_ALL: u32 = 0x17;
    const CSIDL_DESKTOP: u64 = 0;
    const SWC_DESKTOP: i32 = 8;
    const SWFO_NEEDDISPATCH: i32 = 1;
    const SVGIO_BACKGROUND: u32 = 0;
    const SW_SHOWNORMAL: u64 = 1;
    unsafe {
        let initialized = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED) >= 0;
        let result = (|| {
            let mut out = std::ptr::null_mut();
            check(
                CoCreateInstance(
                    &CLSID_SHELL_WINDOWS,
                    std::ptr::null_mut(),
                    CLSCTX_ALL,
                    &IID_ISHELL_WINDOWS,
                    &mut out,
                ),
                "CoCreateInstance(ShellWindows)",
            )?;
            let shell_windows = Com(out);
            // IShellWindows::FindWindowSW
            let find_window: unsafe extern "system" fn(
                *mut c_void,
                *const Variant,
                *const Variant,
                i32,
                *mut i32,
                i32,
                *mut *mut c_void,
            ) -> i32 = shell_windows.method(15);
            let location = Variant::new(VT_I4, CSIDL_DESKTOP);
            let root = Variant::new(VT_EMPTY, 0);
            let mut hwnd = 0i32;
            let mut out = std::ptr::null_mut();
            let found = find_window(
                shell_windows.0,
                &location,
                &root,
                SWC_DESKTOP,
                &mut hwnd,
                SWFO_NEEDDISPATCH,
                &mut out,
            );
            check(found, "IShellWindows::FindWindowSW")?;
            if out.is_null() {
                return Err(std::io::Error::other(
                    "Explorer's desktop window was not found; is Explorer running as the shell?",
                ));
            }
            let desktop = Com(out);
            let services = desktop.query(&IID_ISERVICE_PROVIDER)?;
            // IServiceProvider::QueryService
            let query_service: unsafe extern "system" fn(
                *mut c_void,
                *const Guid,
                *const Guid,
                *mut *mut c_void,
            ) -> i32 = services.method(3);
            let mut out = std::ptr::null_mut();
            check(
                query_service(
                    services.0,
                    &SID_STOP_LEVEL_BROWSER,
                    &IID_ISHELL_BROWSER,
                    &mut out,
                ),
                "QueryService(IShellBrowser)",
            )?;
            let browser = Com(out);
            // IShellBrowser::QueryActiveShellView
            let active_view: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32 =
                browser.method(15);
            let mut out = std::ptr::null_mut();
            check(
                active_view(browser.0, &mut out),
                "IShellBrowser::QueryActiveShellView",
            )?;
            let view = Com(out);
            // IShellView::GetItemObject
            let item_object: unsafe extern "system" fn(
                *mut c_void,
                u32,
                *const Guid,
                *mut *mut c_void,
            ) -> i32 = view.method(15);
            let mut out = std::ptr::null_mut();
            check(
                item_object(view.0, SVGIO_BACKGROUND, &IID_IDISPATCH, &mut out),
                "IShellView::GetItemObject",
            )?;
            let background = Com(out);
            let folder_view = background.query(&IID_ISHELL_FOLDER_VIEW_DUAL)?;
            // IShellFolderViewDual::get_Application
            let application: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32 =
                folder_view.method(7);
            let mut out = std::ptr::null_mut();
            check(
                application(folder_view.0, &mut out),
                "IShellFolderViewDual::get_Application",
            )?;
            let shell = Com(out).query(&IID_ISHELL_DISPATCH2)?;
            // IShellDispatch2::ShellExecute
            let shell_execute: unsafe extern "system" fn(
                *mut c_void,
                *mut u16,
                Variant,
                Variant,
                Variant,
                Variant,
            ) -> i32 = shell.method(31);
            let file = Bstr::new(file.as_os_str());
            let arguments = Bstr::new(std::ffi::OsStr::new(arguments));
            let directory = Bstr::new(directory.as_os_str());
            check(
                shell_execute(
                    shell.0,
                    file.0,
                    Variant::new(VT_BSTR, arguments.0 as u64),
                    Variant::new(VT_BSTR, directory.0 as u64),
                    Variant::new(VT_EMPTY, 0),
                    Variant::new(VT_I4, SW_SHOWNORMAL),
                ),
                "IShellDispatch2::ShellExecute",
            )
        })();
        if initialized {
            CoUninitialize();
        }
        result
    }
}

/// The Windows session this process runs in (0 is the services session, which has no desktop).
pub fn current_session_id() -> std::io::Result<u32> {
    let mut session_id = 0u32;
    if unsafe { ProcessIdToSessionId(std::process::id(), &mut session_id) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(session_id)
}

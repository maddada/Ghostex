//! Runs one shell command as root on macOS after the standard administrator password dialog,
//! asked for by gxserver itself so the dialog names Ghostex.
//!
//! CDXC:HelperNames 2026-11-21 WHY: the administrator dialog names the process that asks. Turning
//! on Remote Login ran `/usr/bin/osascript -e 'do shell script … with administrator privileges'`,
//! so the dialog read "osascript wants to make changes" and people could not tell the request came
//! from Ghostex. gxserver now asks Authorization Services itself: the dialog names gxserver's
//! bundle, "Ghostex Server", and shows Ghostex's own sentence. The command then runs as root
//! through `AuthorizationExecuteWithPrivileges`, deprecated but still exported (macOS 27). It is
//! looked up at run time, and a macOS without it gets [`AdministratorRun::Unavailable`] so the
//! caller can fall back to osascript with the same sentence as its prompt. The command's shell
//! prints its own pid first (the tool runs in the pid of gxserver's child, so it is reaped by that
//! pid, never with a wait that could take another child) and a marker after the command succeeds;
//! the marker decides success. The same approach is in the desktop's Keep Awake installer
//! (apps/desktop/native/macos/GpuiLidSleepHelperClient.m).

use std::ffi::{c_char, c_void, CString};
use std::fs::File;
use std::io::Read;
use std::os::fd::FromRawFd;

/// How an administrator run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdministratorRun {
    /// The command ran as root and exited 0.
    Succeeded,
    /// The person closed the dialog.
    Cancelled,
    /// Authorization was refused, or the command ran and failed.
    Failed,
    /// This macOS can't run it this way; the caller falls back to osascript.
    Unavailable,
}

const DONE_MARKER: &str = "ghostex-administrator-step-done";

type OsStatus = i32;
type AuthorizationRef = *mut c_void;
type AuthorizationFlags = u32;

const ERR_AUTHORIZATION_SUCCESS: OsStatus = 0;
const ERR_AUTHORIZATION_CANCELED: OsStatus = -60006;
const FLAG_DEFAULTS: AuthorizationFlags = 0;
const FLAG_INTERACTION_ALLOWED: AuthorizationFlags = 1 << 0;
const FLAG_EXTEND_RIGHTS: AuthorizationFlags = 1 << 1;
const FLAG_DESTROY_RIGHTS: AuthorizationFlags = 1 << 3;
const FLAG_PRE_AUTHORIZE: AuthorizationFlags = 1 << 4;
const RIGHT_EXECUTE: &[u8] = b"system.privilege.admin\0";
const ENVIRONMENT_PROMPT: &[u8] = b"prompt\0";

#[repr(C)]
struct AuthorizationItem {
    name: *const c_char,
    value_length: usize,
    value: *mut c_void,
    flags: u32,
}

#[repr(C)]
struct AuthorizationItemSet {
    count: u32,
    items: *mut AuthorizationItem,
}

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn AuthorizationCreate(
        rights: *const AuthorizationItemSet,
        environment: *const AuthorizationItemSet,
        flags: AuthorizationFlags,
        authorization: *mut AuthorizationRef,
    ) -> OsStatus;
    fn AuthorizationCopyRights(
        authorization: AuthorizationRef,
        rights: *const AuthorizationItemSet,
        environment: *const AuthorizationItemSet,
        flags: AuthorizationFlags,
        authorized_rights: *mut *mut AuthorizationItemSet,
    ) -> OsStatus;
    fn AuthorizationFree(authorization: AuthorizationRef, flags: AuthorizationFlags) -> OsStatus;
}

type AuthorizationExecuteWithPrivileges = unsafe extern "C" fn(
    AuthorizationRef,
    *const c_char,
    AuthorizationFlags,
    *const *mut c_char,
    *mut *mut libc::FILE,
) -> OsStatus;

fn execute_with_privileges() -> Option<AuthorizationExecuteWithPrivileges> {
    let name = b"AuthorizationExecuteWithPrivileges\0";
    // SAFETY: dlsym with RTLD_DEFAULT and a NUL-terminated name; a non-null result is that
    // function, whose signature is declared in Security/Authorization.h.
    let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr().cast()) };
    (!symbol.is_null()).then(|| {
        // SAFETY: see above.
        unsafe { std::mem::transmute::<*mut c_void, AuthorizationExecuteWithPrivileges>(symbol) }
    })
}

/// Frees an authorization and destroys the rights it holds.
struct Authorization(AuthorizationRef);

impl Drop for Authorization {
    fn drop(&mut self) {
        // SAFETY: the reference came from AuthorizationCreate and is freed once.
        unsafe { AuthorizationFree(self.0, FLAG_DESTROY_RIGHTS) };
    }
}

/// Asks for an administrator's password with `prompt` (`interactive`), then runs `command` with
/// `/bin/sh -c` as root. Without `interactive` it only checks whether the right is already held,
/// which never shows a dialog (the tests use that).
pub(crate) fn run_as_administrator(
    command: &str,
    prompt: &str,
    interactive: bool,
) -> AdministratorRun {
    let Some(execute) = execute_with_privileges() else {
        return AdministratorRun::Unavailable;
    };
    let (Ok(prompt), Ok(wrapped)) = (CString::new(prompt), CString::new(wrapped_command(command)))
    else {
        return AdministratorRun::Failed;
    };
    let mut reference: AuthorizationRef = std::ptr::null_mut();
    // SAFETY: null rights and environment are allowed; `reference` receives the new authorization.
    let status = unsafe {
        AuthorizationCreate(
            std::ptr::null(),
            std::ptr::null(),
            FLAG_DEFAULTS,
            &mut reference,
        )
    };
    if status != ERR_AUTHORIZATION_SUCCESS || reference.is_null() {
        return AdministratorRun::Unavailable;
    }
    let authorization = Authorization(reference);

    let mut right = AuthorizationItem {
        name: RIGHT_EXECUTE.as_ptr().cast(),
        value_length: 0,
        value: std::ptr::null_mut(),
        flags: 0,
    };
    let rights = AuthorizationItemSet {
        count: 1,
        items: &mut right,
    };
    let mut prompt_item = AuthorizationItem {
        name: ENVIRONMENT_PROMPT.as_ptr().cast(),
        value_length: prompt.as_bytes().len(),
        value: prompt.as_ptr() as *mut c_void,
        flags: 0,
    };
    let environment = AuthorizationItemSet {
        count: 1,
        items: &mut prompt_item,
    };
    let flags = FLAG_EXTEND_RIGHTS
        | FLAG_PRE_AUTHORIZE
        | if interactive {
            FLAG_INTERACTION_ALLOWED
        } else {
            0
        };
    // SAFETY: every pointer refers to a value that outlives the call.
    let status = unsafe {
        AuthorizationCopyRights(
            authorization.0,
            &rights,
            &environment,
            flags,
            std::ptr::null_mut(),
        )
    };
    match status {
        ERR_AUTHORIZATION_SUCCESS => {}
        ERR_AUTHORIZATION_CANCELED => return AdministratorRun::Cancelled,
        _ => return AdministratorRun::Failed,
    }

    let tool = c"/bin/sh";
    let dash_c = c"-c";
    let arguments: [*mut c_char; 3] = [
        dash_c.as_ptr() as *mut c_char,
        wrapped.as_ptr() as *mut c_char,
        std::ptr::null_mut(),
    ];
    let mut pipe: *mut libc::FILE = std::ptr::null_mut();
    // SAFETY: the tool path and arguments are NUL-terminated and the argument list ends in null.
    let status = unsafe {
        execute(
            authorization.0,
            tool.as_ptr(),
            FLAG_DEFAULTS,
            arguments.as_ptr(),
            &mut pipe,
        )
    };
    drop(authorization);
    if status != ERR_AUTHORIZATION_SUCCESS || pipe.is_null() {
        if !pipe.is_null() {
            // SAFETY: a FILE the call returned, closed once.
            unsafe { libc::fclose(pipe) };
        }
        return AdministratorRun::Failed;
    }
    // SAFETY: `pipe` is a valid FILE; its descriptor is duplicated before the FILE is closed, and
    // the File owns the duplicate.
    let mut output = String::new();
    unsafe {
        let duplicate = libc::dup(libc::fileno(pipe));
        libc::fclose(pipe);
        if duplicate >= 0 {
            let _ = File::from_raw_fd(duplicate).read_to_string(&mut output);
        }
    }
    let (pid, done) = read_output(&output);
    if let Some(pid) = pid {
        let mut wait_status = 0;
        // SAFETY: waits for this process's own child, the pid its shell printed.
        while unsafe { libc::waitpid(pid, &mut wait_status, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
        {}
    }
    if done {
        AdministratorRun::Succeeded
    } else {
        AdministratorRun::Failed
    }
}

/// The command as the root shell runs it: its pid first, then the command with its output kept
/// out of the pipe, then the marker when it succeeded.
fn wrapped_command(command: &str) -> String {
    format!("echo $$; ( {command} ) </dev/null >/dev/null 2>&1 && echo {DONE_MARKER}")
}

/// The pid the root shell printed and whether it printed the marker.
fn read_output(output: &str) -> (Option<libc::pid_t>, bool) {
    let mut lines = output.lines();
    let pid = lines
        .next()
        .and_then(|line| line.trim().parse::<libc::pid_t>().ok())
        .filter(|pid| *pid > 0);
    let done = output.lines().any(|line| line.trim() == DONE_MARKER);
    (pid, done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn the_wrapped_command_prints_its_pid_and_the_marker_only_on_success() {
        for (command, succeeds) in [("true", true), ("echo noisy; exit 3", false)] {
            let child = Command::new("/bin/sh")
                .args(["-c", &wrapped_command(command)])
                .output()
                .expect("sh runs");
            let output = String::from_utf8_lossy(&child.stdout);
            let (pid, done) = read_output(&output);
            assert!(pid.is_some(), "{output}");
            assert_eq!(done, succeeds, "{output}");
            assert!(!output.contains("noisy"), "{output}");
        }
    }

    #[test]
    fn output_without_a_pid_or_marker_is_a_failure() {
        assert_eq!(read_output(""), (None, false));
        assert_eq!(read_output("abc\n"), (None, false));
        assert_eq!(read_output("42\n"), (Some(42), false));
        assert_eq!(
            read_output(&format!("42\n{DONE_MARKER}\n")),
            (Some(42), true)
        );
    }

    #[test]
    fn this_macos_still_exports_the_administrator_run() {
        assert!(execute_with_privileges().is_some());
    }

    /// Without interaction the right is only checked, so no dialog opens: a test process holds no
    /// administrator right, which is refused (or a CI runner already holding it runs `true`).
    #[test]
    fn a_check_without_interaction_never_opens_the_dialog() {
        let run = run_as_administrator("true", "Ghostex test", false);
        assert!(
            matches!(run, AdministratorRun::Failed | AdministratorRun::Succeeded),
            "{run:?}"
        );
    }
}

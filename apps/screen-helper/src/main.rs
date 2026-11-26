//! `ghostex-screen`: the remote screen helper gxserver starts while a viewer watches or controls
//! this computer (server/src/remote_screen/). It runs voxl, which captures the screen, encodes it
//! as H.264 and applies the viewer's mouse and keyboard, on a loopback port only gxserver dials.
//!
//! CDXC:RemoteScreen 2026-11-26 DECISION:
//! User (plan page, Q5110): build remote control on voxl, and "it's fine to use gpl code in ghostex as long as it's a external lib or linked from outside basically". voxl links x264 (GPL-2.0-or-later), so it lives in this separate program that gxserver launches, never compiled into gxserver or the app; deny.toml and THIRD_PARTY_NOTICES.md say so.
//! SEE-ALSO: server/src/remote_screen/helper.rs (starts it), deny.toml (the licence exception).

use std::io::Read;

/// Flags gxserver may not override: the helper answers on loopback only (gxserver is its one
/// client) and has no sound, microphone or file panel until the first version turns them on.
const FIXED_FLAGS: &[&str] = &[
    "--addr=127.0.0.1",
    "--title=Ghostex",
    "--audio-enabled=false",
    "--microphone-enabled=false",
];

fn main() {
    #[cfg(target_os = "macos")]
    become_own_privacy_identity();
    let passed: Vec<String> = std::env::args().skip(1).collect();
    if passed
        .iter()
        .any(|arg| arg == "--help" || arg == "--version")
    {
        voxl::run(passed);
        return;
    }
    // gxserver holds this process's stdin open; when gxserver ends, however it ends, the pipe
    // closes and the screen stops being shared.
    if std::env::var_os("GHOSTEX_SCREEN_EXIT_ON_STDIN_EOF").is_some() {
        std::thread::spawn(|| {
            let mut sink = [0u8; 64];
            let mut stdin = std::io::stdin();
            while matches!(stdin.read(&mut sink), Ok(count) if count > 0) {}
            std::process::exit(0);
        });
    }
    let mut args: Vec<String> = passed
        .into_iter()
        .filter(|arg| {
            !FIXED_FLAGS
                .iter()
                .any(|fixed| arg.split('=').next() == fixed.split('=').next())
        })
        .collect();
    args.extend(FIXED_FLAGS.iter().map(|flag| flag.to_string()));
    voxl::run(args);
}

/// CDXC:RemoteScreen 2026-11-27 WHY: macOS charges a child's Screen Recording and Accessibility
/// checks to its responsible process, so started by gxserver this helper would ask for (and use)
/// the grants of whatever app started gxserver, and System Settings would never list
/// ghostex-screen. Re-executing itself once with responsibility disclaimed makes it its own
/// privacy identity, the same pid with the same pipes (POSIX_SPAWN_SETEXEC replaces the image in
/// place), so gxserver's stdin pipe and child handle keep working. On failure it carries on as is.
/// SEE-ALSO: apps/desktop/src/app/helpers/os_cli/disclaimed_spawn.rs (the app's helpers do the same).
#[cfg(target_os = "macos")]
fn become_own_privacy_identity() {
    use std::{ffi::CString, os::unix::ffi::OsStringExt};

    const MARKER: &str = "GHOSTEX_SCREEN_DISCLAIMED";
    if std::env::var_os(MARKER).is_some() {
        return;
    }
    unsafe extern "C" {
        /// Private but stable since macOS 10.14 (libsystem `spawn_private.h`).
        fn responsibility_spawnattrs_setdisclaim(
            attrs: *mut libc::posix_spawnattr_t,
            disclaim: libc::c_int,
        ) -> libc::c_int;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Ok(program) = CString::new(exe.into_os_string().into_vec()) else {
        return;
    };
    let argv_owned: Vec<CString> = std::env::args_os()
        .filter_map(|arg| CString::new(arg.into_vec()).ok())
        .collect();
    let mut envp_owned: Vec<CString> = std::env::vars_os()
        .filter_map(|(key, value)| {
            let mut entry = key.into_vec();
            entry.push(b'=');
            entry.extend(value.into_vec());
            CString::new(entry).ok()
        })
        .collect();
    envp_owned.push(CString::new(format!("{MARKER}=1")).expect("no NUL"));
    let mut argv: Vec<*mut libc::c_char> =
        argv_owned.iter().map(|a| a.as_ptr().cast_mut()).collect();
    argv.push(std::ptr::null_mut());
    let mut envp: Vec<*mut libc::c_char> =
        envp_owned.iter().map(|e| e.as_ptr().cast_mut()).collect();
    envp.push(std::ptr::null_mut());
    // SAFETY: the attribute object is initialised before use; argv and envp are null-terminated
    // arrays of live C strings. With SETEXEC a successful call does not return.
    unsafe {
        let mut attr: libc::posix_spawnattr_t = std::mem::zeroed();
        if libc::posix_spawnattr_init(&mut attr) != 0 {
            return;
        }
        libc::posix_spawnattr_setflags(&mut attr, libc::POSIX_SPAWN_SETEXEC as _);
        responsibility_spawnattrs_setdisclaim(&mut attr, 1);
        let mut pid: libc::pid_t = 0;
        libc::posix_spawn(
            &mut pid,
            program.as_ptr(),
            std::ptr::null(),
            &attr,
            argv.as_ptr(),
            envp.as_ptr(),
        );
        libc::posix_spawnattr_destroy(&mut attr);
    }
}

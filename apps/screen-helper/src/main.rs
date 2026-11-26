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

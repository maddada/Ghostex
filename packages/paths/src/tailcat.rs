//! Which tailcat (Easy Connect's WireGuard relay helper) a Ghostex client may dial with.
//!
//! CDXC:RemotePairing 2026-11-02 WHY:
//! A tailcat v0.6.0 server (every Cloud Box, every computer that installed Easy Connect since
//! 2026-09-15, the phone's own bridge) adds a WireGuard pre-shared key to its address, and a v0.5.0
//! or older client answers its ping but never completes the handshake. The desktop's lookup used
//! the first tailcat it found, so a Mac with an old `~/go/bin/tailcat` could not reach a box at all
//! (found while proving Cloud Boxes on 2026-11-02). Clients therefore use only a helper at least
//! [`TAILCAT_MIN_VERSION`], preferring the one Ghostex installs; an older one elsewhere is left alone.
//! SEE-ALSO: apps/desktop/src/app/helpers/remote/easy_connect_forward.rs,
//! server/src/tailcat/binary.rs, server/src/tailcat/install.rs (`TAILCAT_MODULE`, the pinned version).

use std::path::PathBuf;

/// The oldest tailcat a client may dial with: the version Ghostex installs and the Linux package
/// and the phone's bridge ship.
pub const TAILCAT_MIN_VERSION: &str = "v0.6.0";

fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let text = version.trim().trim_start_matches('v');
    let core = text.split(['-', '+', ' ']).next()?;
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    Some((
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
    ))
}

/// Whether `tailcat version`'s answer is at least [`TAILCAT_MIN_VERSION`]. Unreadable answers are not.
pub fn tailcat_version_supported(version: &str) -> bool {
    match (parse(version), parse(TAILCAT_MIN_VERSION)) {
        (Some(found), Some(minimum)) => found >= minimum,
        _ => false,
    }
}

/// The tailcat a Ghostex package ships in the gxserver package's `bin/`, beside `gxserver` itself:
/// the Linux desktop packages (`/opt/ghostex/gxserver/bin/tailcat`, the AppImage's copy of it),
/// remote computers and Cloud Boxes. gxserver finds it beside its own executable; the desktop app
/// finds it beside the gxserver it runs.
pub fn bundled_tailcat_binary(gxserver_binary: &std::path::Path) -> Option<PathBuf> {
    let directory = gxserver_binary.parent()?;
    Some(directory.join(if cfg!(windows) {
        "tailcat.exe"
    } else {
        "tailcat"
    }))
}

/// Where Ghostex's own install (Settings › Remote › Easy Connect › Install) puts tailcat.
pub fn managed_tailcat_binary(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("tailcat/bin").join(if cfg!(windows) {
        "tailcat.exe"
    } else {
        "tailcat"
    })
}

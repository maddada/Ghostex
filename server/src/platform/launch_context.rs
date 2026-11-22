use std::{io, os::windows::io::AsRawHandle, ptr, sync::OnceLock};

use serde_json::{json, Value};
use windows_sys::Win32::{
    Security::{
        EqualSid, IsTokenRestricted, TokenGroups, TokenSessionId, WinNetworkSid,
        SID_AND_ATTRIBUTES, TOKEN_GROUPS,
    },
    System::Threading::{
        GetCurrentProcess, GetProcessMitigationPolicy, ProcessRedirectionTrustPolicy,
    },
};

use crate::protocol::LaunchContextReport;

use super::standard_user::{current_process_token, token_information, well_known_sid};

/// What a customer reads in gxserver's log when the server runs somewhere its agents are limited.
const LAUNCH_CONTEXT_WARNING: &str = "started from an SSH/background session; agents started here cannot use junction-based tools (codex, cua-driver). Ghostex replaces it with a server in your desktop session when the app opens.";

/// How this process was started, read from its own token and mitigation policy. A value is `None` when Windows would not answer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LaunchContext {
    pub(crate) session_id: Option<u32>,
    pub(crate) network_logon: Option<bool>,
    pub(crate) restricted_token: bool,
    pub(crate) redirection_trust_enforced: Option<bool>,
}

impl LaunchContext {
    /// Started outside any desktop: Windows' services session, or a network logon such as SSH.
    pub(crate) fn is_background(&self) -> bool {
        self.session_id == Some(0) || self.network_logon == Some(true)
    }

    /// Anything that limits the wmx sessions and agents this process starts.
    pub(crate) fn is_limited(&self) -> bool {
        self.is_background() || self.redirection_trust_enforced == Some(true)
    }

    pub(crate) fn report(&self) -> LaunchContextReport {
        LaunchContextReport {
            background: self.is_background(),
            limited: self.is_limited(),
            network_logon: self.network_logon,
            redirection_trust_enforced: self.redirection_trust_enforced,
            restricted_token: self.restricted_token,
            windows_session_id: self.session_id,
        }
    }
}

/// This process's launch context; a token never changes session or logon type, so it is read once.
pub(crate) fn current() -> LaunchContext {
    static CONTEXT: OnceLock<LaunchContext> = OnceLock::new();
    *CONTEXT.get_or_init(|| {
        let token = current_process_token().ok();
        LaunchContext {
            session_id: token
                .as_ref()
                .and_then(|token| token_session_id(token).ok()),
            network_logon: token
                .as_ref()
                .and_then(|token| token_has_network_logon(token).ok()),
            restricted_token: token
                .as_ref()
                .is_some_and(|token| unsafe { IsTokenRestricted(token.as_raw_handle()) != 0 }),
            redirection_trust_enforced: redirection_trust_enforced().ok(),
        }
    })
}

/// CDXC:PlatformSupport 2026-10-03 WHY: A gxserver started by a remote `ghostex` call over SSH runs in session 0 with a network-logon token, and on 2026-10-02 it also carried the RedirectionGuard mitigation (EnforceRedirectionTrust). Every wmx session, agent and build under it inherits that, so junction-based tools on PATH (Codex, cua-driver) cannot run and zig aborts the libghostty-vt build (see `zig_build_path_without_untrusted_mount_points` in apps/desktop/build.rs). Nothing showed how the server had been started, so startup logs one warning with these facts when any of them is off, and nothing otherwise. Since 2026-10-04 such a server only exists while nobody is signed in to the desktop or until the app replaces it (see `desktop_session.rs`), and health reports the same facts as `launchContext` so the app can tell.
pub(crate) fn launch_context_warning_details() -> Option<Value> {
    let context = current();
    context.is_limited().then(|| {
        json!({
            "warning": LAUNCH_CONTEXT_WARNING,
            "windowsSessionId": context.session_id,
            "networkLogon": context.network_logon,
            "restrictedToken": context.restricted_token,
            "redirectionTrustEnforced": context.redirection_trust_enforced,
        })
    })
}

fn token_session_id(token: &std::os::windows::io::OwnedHandle) -> io::Result<u32> {
    let buffer = token_information(token, TokenSessionId)?;
    Ok(buffer[0] as u32)
}

fn token_has_network_logon(token: &std::os::windows::io::OwnedHandle) -> io::Result<bool> {
    let groups = token_information(token, TokenGroups)?;
    let groups = unsafe {
        let header = groups.as_ptr().cast::<TOKEN_GROUPS>();
        std::slice::from_raw_parts(
            ptr::addr_of!((*header).Groups).cast::<SID_AND_ATTRIBUTES>(),
            (*header).GroupCount as usize,
        )
    };
    let network = well_known_sid(WinNetworkSid)?;
    Ok(groups
        .iter()
        .any(|group| unsafe { EqualSid(group.Sid, network.as_ptr().cast_mut().cast()) } != 0))
}

fn redirection_trust_enforced() -> io::Result<bool> {
    // PROCESS_MITIGATION_REDIRECTION_TRUST_POLICY: bit 0 is EnforceRedirectionTrust.
    let mut flags: u32 = 0;
    let queried = unsafe {
        GetProcessMitigationPolicy(
            GetCurrentProcess(),
            ProcessRedirectionTrustPolicy,
            ptr::from_mut(&mut flags).cast(),
            std::mem::size_of::<u32>(),
        )
    };
    if queried == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(flags & 1 != 0)
}

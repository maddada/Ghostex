//! One line in the remote log for every end of a computer's link and for its return.
//!
//! CDXC:RemoteMachines 2026-11-10 WHY:
//! A user reported "a lot of reconnects" between app and remote, and of 34 reconnects of one Mac to one PC in two days, 7 had no explanation anywhere: the watchdog, the liveness stream and the Easy Connect forwarder all dropped and reconnected a computer without writing a word, so the only record was the other computer's sshd log. Each drop now writes `gpui.remoteGxserver.linkEnded` with a reason (`tunnelExited`, `healthRefused`, `healthUnanswered`, `wakeUnconfirmed`, `liveStreamFailed`), and the connection that follows writes `gpui.remoteGxserver.linkRestored` with how long the computer was gone and the reason it had gone. They are warnings, so they are written whatever the Diagnostic logs switches say, into the file the connect lines already use (`gpui-remote-gxserver-install-debug.log`; `ghostex logs --file` reads it). Only the machine's id and bounded words go in, never a host, a user, a port or a token.
//! SEE-ALSO: easy_connect_forward.rs (`easyConnectHelperEnded`), server/src/tailcat/link_log.rs (the other computer's side of an Easy Connect link).

use std::collections::HashMap;
use std::time::Instant;

use crate::*;

/// When each computer's link came up and, for one that was dropped, when and why.
#[derive(Default)]
pub(crate) struct GpuiRemoteLinkLog {
    up_since: HashMap<String, Instant>,
    dropped: HashMap<String, (Instant, GpuiRemoteLinkEnd)>,
}

/// Why a computer's link was dropped, in the words the log uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiRemoteLinkEnd {
    /// The tunnel's SSH process exited.
    TunnelExited,
    /// Health checks through the tunnel were refused twice in a row.
    HealthRefused,
    /// Health checks through the tunnel went unanswered for a minute.
    HealthUnanswered,
    /// Two health checks in a row after this computer woke did not confirm the tunnel.
    WakeUnconfirmed,
    /// The liveness stream kept ending and the tunnel no longer answered.
    LiveStreamFailed,
}

impl GpuiRemoteLinkEnd {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::TunnelExited => "tunnelExited",
            Self::HealthRefused => "healthRefused",
            Self::HealthUnanswered => "healthUnanswered",
            Self::WakeUnconfirmed => "wakeUnconfirmed",
            Self::LiveStreamFailed => "liveStreamFailed",
        }
    }
}

/// Appends `gpui.remoteGxserver.<event>` to the remote log.
pub(crate) fn gpui_log_remote_link_event(event: &str, details: serde_json::Value) {
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        &format!("gpui.remoteGxserver.{event}"),
        details,
    );
}

impl GhostexGpuiApp {
    /// Records that `remote_machine_id`'s working link was dropped for `reason`, with how long it
    /// had been up.
    pub(crate) fn note_gpui_remote_link_ended(
        &mut self,
        remote_machine_id: &str,
        reason: GpuiRemoteLinkEnd,
    ) {
        let up_ms = self
            .remote_link_log
            .up_since
            .remove(remote_machine_id)
            .map(|since| since.elapsed().as_millis() as u64);
        gpui_log_remote_link_event(
            "linkEnded",
            serde_json::json!({
                "level": "warning",
                "machineId": remote_machine_id,
                "reason": reason.as_str(),
                "upMs": up_ms,
            }),
        );
        self.remote_link_log
            .dropped
            .insert(remote_machine_id.to_string(), (Instant::now(), reason));
    }

    /// Records that `remote_machine_id` is connected, and that it is back when this app run saw
    /// its link dropped.
    pub(crate) fn note_gpui_remote_link_connected(&mut self, remote_machine_id: &str) {
        self.remote_link_log
            .up_since
            .insert(remote_machine_id.to_string(), Instant::now());
        let Some((dropped_at, reason)) = self.remote_link_log.dropped.remove(remote_machine_id)
        else {
            return;
        };
        gpui_log_remote_link_event(
            "linkRestored",
            serde_json::json!({
                "machineId": remote_machine_id,
                "goneMs": dropped_at.elapsed().as_millis() as u64,
                "afterFailure": reason.as_str(),
            }),
        );
    }
}

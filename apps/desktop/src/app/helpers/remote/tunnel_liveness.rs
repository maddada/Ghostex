//! Whether a remote machine's gxserver tunnel is alive, slow, or gone.
//!
//! CDXC:RemoteMachines 2026-10-31 WHY:
//! GitHub #361: over a slow, lossy Tailscale path (about 1 s round trips, up to 20% loss) the
//! terminal kept flowing while the sidebar went grey and every chat read failed for an hour. The
//! tunnel's health probe gave the remote gxserver 1 s to answer and two misses in a row tore the
//! tunnel down, so a link that was only slow was reported as disconnected, every request in flight
//! through it was cut ("Could not read remote gxserver response."), and the reconnect that
//! followed waited 7 s for the same 1 s probe and failed too. Measured over a throttled link
//! (1 Mbps, 300 ms, 2% loss, two chats reading): the probe took longer than 1 s often enough that
//! the old rule dropped a working tunnel. Now a probe is given [`GPUI_REMOTE_GXSERVER_HEALTH_TIMEOUT`],
//! an answer to any request through the tunnel counts as a sign of life, and only a tunnel that
//! refuses (nothing listening, or the far side closing without an answer) twice in a row, or that
//! answers nothing for [`TUNNEL_UNANSWERED_PROBES_TO_DROP`] probes in a row, is dropped. A tunnel
//! whose SSH process exits is still dropped at once by the watchdog.
//! SEE-ALSO: apps/desktop/src/app/remote_conn/presentation_and_watchdog.rs (the watchdog),
//! websocket_presentation.rs (the event stream's liveness), tunnel_and_auth.rs (the connect wait),
//! ~/.local/share/agent-work/Ghostex/issues-1027/remote-slow-361/ (the measurements).

use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
    net::TcpStream,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use crate::app::helpers::*;
use crate::*;

/// Refusals in a row that drop a tunnel. A refusal is an answer, so it is never slowness.
pub(crate) const TUNNEL_REFUSED_PROBES_TO_DROP: u8 = 2;
/// Probes in a row with no answer at all (and no other request answered meanwhile) that drop a
/// tunnel: at the watchdog's 15 s interval, a minute or more of silence.
pub(crate) const TUNNEL_UNANSWERED_PROBES_TO_DROP: u8 = 4;

/// What one authenticated health probe got back.
#[derive(Clone, Debug)]
pub(crate) enum GpuiRemoteHealthProbe {
    Healthy(GpuiRemoteGxserverCapabilities),
    /// Nothing listens on the tunnel port, the connection was reset, or the far side answered
    /// without a healthy status (SSH closes a forwarded connection it cannot open on the remote).
    Refused,
    /// The probe was sent and nothing came back in time: a slow or stalled link.
    NoAnswer,
}

/// What the watchdog makes of a machine's tunnel after a probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiRemoteTunnelVerdict {
    Live,
    /// Not confirmed by this probe, not gone either: keep the tunnel and keep calling it connected.
    Slow,
    Dropped,
}

/// The probes in a row that did not confirm a tunnel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiRemoteTunnelLiveness {
    refused_in_a_row: u8,
    unanswered_in_a_row: u8,
}

impl GpuiRemoteTunnelLiveness {
    /// Records one probe. `answered_meanwhile` is whether any other request through this tunnel
    /// got its whole answer since the previous probe, which proves the tunnel as well as a probe.
    pub(crate) fn record(
        &mut self,
        probe: &GpuiRemoteHealthProbe,
        answered_meanwhile: bool,
    ) -> GpuiRemoteTunnelVerdict {
        match probe {
            GpuiRemoteHealthProbe::Healthy(_) => {
                *self = Self::default();
                GpuiRemoteTunnelVerdict::Live
            }
            _ if answered_meanwhile => {
                *self = Self::default();
                GpuiRemoteTunnelVerdict::Live
            }
            GpuiRemoteHealthProbe::Refused => {
                self.unanswered_in_a_row = 0;
                self.refused_in_a_row = self.refused_in_a_row.saturating_add(1);
                if self.refused_in_a_row >= TUNNEL_REFUSED_PROBES_TO_DROP {
                    GpuiRemoteTunnelVerdict::Dropped
                } else {
                    GpuiRemoteTunnelVerdict::Slow
                }
            }
            GpuiRemoteHealthProbe::NoAnswer => {
                self.refused_in_a_row = 0;
                self.unanswered_in_a_row = self.unanswered_in_a_row.saturating_add(1);
                if self.unanswered_in_a_row >= TUNNEL_UNANSWERED_PROBES_TO_DROP {
                    GpuiRemoteTunnelVerdict::Dropped
                } else {
                    GpuiRemoteTunnelVerdict::Slow
                }
            }
        }
    }
}

/// When a request through each tunnel port last got its whole answer.
static TUNNEL_LAST_ANSWER: LazyLock<Mutex<HashMap<u16, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Called by the remote request helper when a response was read to its end.
pub(crate) fn gpui_remote_tunnel_note_answer(local_port: u16) {
    if let Ok(mut answers) = TUNNEL_LAST_ANSWER.lock() {
        answers.insert(local_port, Instant::now());
    }
}

/// Whether a request through `local_port` got its whole answer within `window`.
pub(crate) fn gpui_remote_tunnel_answered_within(local_port: u16, window: Duration) -> bool {
    TUNNEL_LAST_ANSWER
        .lock()
        .ok()
        .and_then(|answers| answers.get(&local_port).copied())
        .is_some_and(|at| at.elapsed() <= window)
}

/// Authenticated remote-daemon probe that says how it failed. A healthy answer also carries that
/// daemon's advertised capability inventory.
pub(crate) fn gpui_remote_health_probe(
    local_port: u16,
    token: &str,
    timeout: Duration,
) -> GpuiRemoteHealthProbe {
    let address = format!("127.0.0.1:{local_port}");
    let Ok(mut stream) = TcpStream::connect(address.as_str()) else {
        return GpuiRemoteHealthProbe::Refused;
    };
    let timeout = timeout.max(Duration::from_millis(1));
    if stream.set_read_timeout(Some(timeout)).is_err()
        || stream.set_write_timeout(Some(timeout)).is_err()
    {
        return GpuiRemoteHealthProbe::Refused;
    }
    let request = format!(
        "GET /api/health/server HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\n\r\n",
    );
    if let Err(error) = stream.write_all(request.as_bytes()) {
        return gpui_remote_health_probe_io_failure(error.kind());
    }
    let mut response = String::new();
    if let Err(error) = stream.read_to_string(&mut response) {
        return gpui_remote_health_probe_io_failure(error.kind());
    }
    gpui_remote_health_probe_from_response(response.as_str())
}

fn gpui_remote_health_probe_io_failure(kind: ErrorKind) -> GpuiRemoteHealthProbe {
    if matches!(kind, ErrorKind::WouldBlock | ErrorKind::TimedOut) {
        GpuiRemoteHealthProbe::NoAnswer
    } else {
        GpuiRemoteHealthProbe::Refused
    }
}

pub(crate) fn gpui_remote_health_probe_from_response(response: &str) -> GpuiRemoteHealthProbe {
    let healthy = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .is_some_and(|status| (200..300).contains(&status));
    if healthy {
        GpuiRemoteHealthProbe::Healthy(gpui_remote_gxserver_capabilities_from_health_response(
            response,
        ))
    } else {
        GpuiRemoteHealthProbe::Refused
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn healthy() -> GpuiRemoteHealthProbe {
        GpuiRemoteHealthProbe::Healthy(GpuiRemoteGxserverCapabilities::default())
    }

    #[test]
    fn a_slow_tunnel_stays_connected_until_a_minute_of_silence() {
        let mut liveness = GpuiRemoteTunnelLiveness::default();
        for _ in 1..TUNNEL_UNANSWERED_PROBES_TO_DROP {
            assert_eq!(
                liveness.record(&GpuiRemoteHealthProbe::NoAnswer, false),
                GpuiRemoteTunnelVerdict::Slow
            );
        }
        assert_eq!(
            liveness.record(&GpuiRemoteHealthProbe::NoAnswer, false),
            GpuiRemoteTunnelVerdict::Dropped
        );
    }

    #[test]
    fn any_answer_through_the_tunnel_proves_it() {
        let mut liveness = GpuiRemoteTunnelLiveness::default();
        for _ in 0..10 {
            // The probe itself lost the race to a big chat read, which was answered.
            assert_eq!(
                liveness.record(&GpuiRemoteHealthProbe::NoAnswer, true),
                GpuiRemoteTunnelVerdict::Live
            );
        }
        liveness.record(&GpuiRemoteHealthProbe::NoAnswer, false);
        liveness.record(&GpuiRemoteHealthProbe::NoAnswer, false);
        assert_eq!(
            liveness.record(&healthy(), false),
            GpuiRemoteTunnelVerdict::Live
        );
        // The healthy answer started the count over.
        for _ in 1..TUNNEL_UNANSWERED_PROBES_TO_DROP {
            assert_eq!(
                liveness.record(&GpuiRemoteHealthProbe::NoAnswer, false),
                GpuiRemoteTunnelVerdict::Slow
            );
        }
    }

    #[test]
    fn a_refusing_tunnel_drops_quickly_and_is_not_hidden() {
        let mut liveness = GpuiRemoteTunnelLiveness::default();
        assert_eq!(
            liveness.record(&GpuiRemoteHealthProbe::Refused, false),
            GpuiRemoteTunnelVerdict::Slow
        );
        assert_eq!(
            liveness.record(&GpuiRemoteHealthProbe::Refused, false),
            GpuiRemoteTunnelVerdict::Dropped
        );
        // Silence and refusals do not add up to each other's count.
        let mut mixed = GpuiRemoteTunnelLiveness::default();
        mixed.record(&GpuiRemoteHealthProbe::Refused, false);
        assert_eq!(
            mixed.record(&GpuiRemoteHealthProbe::NoAnswer, false),
            GpuiRemoteTunnelVerdict::Slow
        );
        assert_eq!(
            mixed.record(&GpuiRemoteHealthProbe::Refused, false),
            GpuiRemoteTunnelVerdict::Slow
        );
    }

    #[test]
    fn answers_are_remembered_per_tunnel_port() {
        let port = 41_999;
        assert!(!gpui_remote_tunnel_answered_within(
            port,
            Duration::from_secs(60)
        ));
        gpui_remote_tunnel_note_answer(port);
        assert!(gpui_remote_tunnel_answered_within(
            port,
            Duration::from_secs(60)
        ));
        assert!(!gpui_remote_tunnel_answered_within(
            port + 1,
            Duration::from_secs(60)
        ));
    }

    /// A server that reads the request and then holds the answer for `delay`.
    fn slow_server(delay: Duration, answer: &'static str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                thread::sleep(delay);
                let _ = stream.write_all(answer.as_bytes());
            }
        });
        port
    }

    #[test]
    fn the_probe_tells_a_slow_answer_from_a_refusal() {
        let ok = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
        // An answer that takes 1.5 s, which the old 1 s probe called a dead tunnel.
        let port = slow_server(Duration::from_millis(1_500), ok);
        assert!(matches!(
            gpui_remote_health_probe(port, "t", GPUI_REMOTE_GXSERVER_HEALTH_TIMEOUT),
            GpuiRemoteHealthProbe::Healthy(_)
        ));
        let port = slow_server(Duration::from_secs(2), ok);
        assert!(matches!(
            gpui_remote_health_probe(port, "t", Duration::from_millis(300)),
            GpuiRemoteHealthProbe::NoAnswer
        ));
        // The far side closing without an answer is how SSH reports a remote it cannot reach.
        let port = slow_server(Duration::ZERO, "");
        assert!(matches!(
            gpui_remote_health_probe(port, "t", GPUI_REMOTE_GXSERVER_HEALTH_TIMEOUT),
            GpuiRemoteHealthProbe::Refused
        ));
        let port = slow_server(Duration::ZERO, "HTTP/1.1 401 Unauthorized\r\n\r\n");
        assert!(matches!(
            gpui_remote_health_probe(port, "t", GPUI_REMOTE_GXSERVER_HEALTH_TIMEOUT),
            GpuiRemoteHealthProbe::Refused
        ));
        // Nothing listening.
        let unused = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = unused.local_addr().unwrap().port();
        drop(unused);
        assert!(matches!(
            gpui_remote_health_probe(port, "t", GPUI_REMOTE_GXSERVER_HEALTH_TIMEOUT),
            GpuiRemoteHealthProbe::Refused
        ));
    }
}

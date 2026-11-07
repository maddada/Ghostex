//! How the online check reads the real network: the system's own route out, and whether an
//! agent's API host answers.

use std::{
    collections::HashMap,
    net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use super::watch::{NetworkChecker, RouteReading};

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// The checker the runtime uses.
pub struct SystemChecker;

impl NetworkChecker for SystemChecker {
    fn route(&self) -> RouteReading {
        route_reading()
    }

    fn reach(&self, target: &str) -> bool {
        reach_cached(target, Duration::ZERO)
    }
}

/// CDXC:OfflineHold 2026-11-07 WHY:
/// The system's view of the network is its routing decision: "connecting" a UDP socket to a public address sends no packet, and the kernel either picks a route and a source address or fails with "network unreachable". That is free, the same on macOS, Linux and Windows, and its source address changes when the network does (Wi-Fi to Ethernet, a VPN coming up), which triggers a probe at once. NWPathMonitor, NetworkManager and NLM would each need their own bindings in a gxserver that also builds for static musl Linux and Windows, for the same two facts. A route alone does not prove the internet works (a Wi-Fi with no uplink keeps its route while DNS fails, which is how the 2026-11-07 outage looked), so the host probe decides; the route only makes it look sooner, and decide on the first miss when there is no route at all.
pub fn route_reading() -> RouteReading {
    let source = |bind: &str, remote: &str| {
        UdpSocket::bind(bind)
            .and_then(|socket| {
                socket.connect(remote)?;
                socket.local_addr()
            })
            .ok()
            .map(|address| address.ip())
    };
    let v4 = source("0.0.0.0:0", "1.1.1.1:53");
    let v6 = source("[::]:0", "[2606:4700:4700::1111]:53");
    RouteReading {
        online: v4.is_some() || v6.is_some(),
        fingerprint: format!("{v4:?}|{v6:?}"),
    }
}

fn answers() -> &'static Mutex<HashMap<String, (Instant, bool)>> {
    static ANSWERS: OnceLock<Mutex<HashMap<String, (Instant, bool)>>> = OnceLock::new();
    ANSWERS.get_or_init(Default::default)
}

/// BLOCKING (a DNS lookup and a TCP connect, a few seconds at most). Whether `target`
/// (`host:port`) answers, reusing an answer younger than `ttl`. A TCP connect to the HTTPS port
/// is enough: any answer there means the API can be reached, and it spends no request.
pub fn reach_cached(target: &str, ttl: Duration) -> bool {
    if let Some((at, reachable)) = answers()
        .lock()
        .ok()
        .and_then(|answers| answers.get(target).copied())
    {
        if at.elapsed() < ttl {
            return reachable;
        }
    }
    let reachable = target.to_socket_addrs().is_ok_and(|addresses| {
        addresses
            .take(2)
            .any(|address: SocketAddr| TcpStream::connect_timeout(&address, PROBE_TIMEOUT).is_ok())
    });
    if let Ok(mut answers) = answers().lock() {
        answers.insert(target.to_string(), (Instant::now(), reachable));
    }
    reachable
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads this computer's real route and probes one API host (a DNS lookup and one TCP
    /// connect). Run by hand: `cargo test --lib live_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_probe_reads_this_computer() {
        let route = route_reading();
        let started = Instant::now();
        let reachable = reach_cached("api.anthropic.com:443", Duration::ZERO);
        println!(
            "route {route:?}, api.anthropic.com reachable {reachable} in {:?}",
            started.elapsed()
        );
        let started = Instant::now();
        let missing = reach_cached("no-such-host.invalid:443", Duration::ZERO);
        println!(
            "unknown host reachable {missing} in {:?}",
            started.elapsed()
        );
        assert!(!missing);
    }
}

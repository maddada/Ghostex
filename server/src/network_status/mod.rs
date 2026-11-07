//! Whether this computer can reach the internet and each agent's API, for everything Ghostex types
//! into agents on its own. One background watch (server/network_status_runtime.rs) reads the
//! system's route every 2 seconds and probes the API hosts of the agents in use every 30 seconds
//! (every 5 while one fails, and at once when the route changes); every reader here only looks at
//! its last answer, so no hot path ever waits on the network.
//!
//! CDXC:OfflineHold 2026-11-07 DECISION:
//! User, after the Mac lost its internet for about 1.5 hours and every agent sat on "API Error: Can't reach the API server (ENOTFOUND)" while Ghostex kept nudging one thread and reported it to its coordinator 9 times: "we need to please have a thing where auto pilot doesn't keep trying to continue threads when network is down". While offline, Autopilot sends no carry-ons and wakes no supervisor, the not-moving watches pause their clocks (no nudge, no report, no red dot) and start them again when the network is back, coordinator reports and the automatic restart for a held message wait, and messages a person or a coordinator sends on purpose still go out. The user hears it once: the Autopilot pill and `coordinator status` say "Offline: waiting for the network" while it lasts, and one line when it is back (the Autopilot log, and one report per coordinator whose threads were held). Not a ping of google.com: ICMP is often blocked and says nothing about the agent's API.
//! SEE-ALSO: coordinators/stall.rs, coordinators/idle_wait.rs, coordinators/remote_stall.rs (the held clocks), server/autopilot_runtime.rs and autopilot_judge.rs, server/coordinator_runtime.rs (held reports and the line when back), session_chat_agent_restart.rs (held restart), accounts/api_errors.rs (recovery's own wait for the network uses the same probe), packages/gx-chat-core/src/extras/autopilot.rs (the pill).

mod probe;
mod watch;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock, PoisonError, RwLock};
use std::time::Duration;

use serde_json::Value;

pub use probe::{reach_cached, SystemChecker};
pub use watch::{NetworkWatch, Transition};

/// The words every surface shows while the computer is offline.
pub const OFFLINE_LABEL: &str = "Offline: waiting for the network";
/// An agent asked about this recently keeps its API host probed.
const INTEREST_TTL_MS: i64 = 10 * 60_000;
/// Probed when no agent with a known API host asked: the most common agent's.
const DEFAULT_TARGET: &str = "api.anthropic.com:443";

/// The watch's last answer, as every reader sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub offline_since: Option<i64>,
    /// API targets (`host:port`) that stopped answering.
    pub down: BTreeSet<String>,
}

fn snapshot_cell() -> &'static RwLock<Snapshot> {
    static SNAPSHOT: OnceLock<RwLock<Snapshot>> = OnceLock::new();
    SNAPSHOT.get_or_init(Default::default)
}

#[cfg(test)]
thread_local! {
    static TEST_SNAPSHOT: std::cell::RefCell<Option<Snapshot>> = const { std::cell::RefCell::new(None) };
}

/// Makes this test thread read `snapshot` instead of the watch's, until the guard drops. Tests
/// simulate an outage this way, never by cutting the computer's network.
#[cfg(test)]
pub fn simulate(snapshot: Snapshot) -> SimulatedNetwork {
    TEST_SNAPSHOT.with(|cell| *cell.borrow_mut() = Some(snapshot));
    SimulatedNetwork
}

/// See [`simulate`].
#[cfg(test)]
pub fn simulate_offline(since: i64) -> SimulatedNetwork {
    simulate(Snapshot {
        offline_since: Some(since),
        down: BTreeSet::new(),
    })
}

#[cfg(test)]
pub struct SimulatedNetwork;

#[cfg(test)]
impl Drop for SimulatedNetwork {
    fn drop(&mut self) {
        TEST_SNAPSHOT.with(|cell| *cell.borrow_mut() = None);
    }
}

/// The watch's last answer.
pub fn snapshot() -> Snapshot {
    #[cfg(test)]
    if let Some(simulated) = TEST_SNAPSHOT.with(|cell| cell.borrow().clone()) {
        return simulated;
    }
    snapshot_cell()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The runtime publishes each tick's answer here.
pub fn publish(watch: &NetworkWatch) {
    let next = Snapshot {
        offline_since: watch.offline_since(),
        down: watch.down_targets(),
    };
    *snapshot_cell()
        .write()
        .unwrap_or_else(PoisonError::into_inner) = next;
}

/// Since when this computer has been offline (ms), while it is.
pub fn offline_since() -> Option<i64> {
    snapshot().offline_since
}

pub fn is_offline() -> bool {
    offline_since().is_some()
}

fn interest() -> &'static Mutex<HashMap<String, i64>> {
    static INTEREST: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    INTEREST.get_or_init(Default::default)
}

/// Whether Ghostex should hold what it would type into `agent`'s session on its own: the computer
/// is offline, or that agent's API host stopped answering. Never blocks; an agent asked about for
/// the first time is probed from the next tick on, and reads as reachable until then.
pub fn agent_offline(agent: Option<&str>) -> bool {
    let snapshot = snapshot();
    let target = agent.and_then(agent_target);
    if let Some(target) = &target {
        let now = chrono::Utc::now().timestamp_millis();
        interest()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(target.clone(), now);
    }
    snapshot.offline_since.is_some() || target.is_some_and(|target| snapshot.down.contains(&target))
}

/// [`agent_offline`] for a session row's agent.
pub fn session_offline(session: &Value) -> bool {
    agent_offline(crate::session_chat_follower::session_chat_agent_for_session(session).as_deref())
}

/// The targets the watch probes now: the API hosts of the agents asked about in the last ten
/// minutes, or the default one when none was.
pub fn wanted_targets(now: i64) -> BTreeSet<String> {
    let mut interest = interest().lock().unwrap_or_else(PoisonError::into_inner);
    interest.retain(|_, asked| now - *asked < INTEREST_TTL_MS);
    let mut targets: BTreeSet<String> = interest.keys().cloned().collect();
    if targets.is_empty() {
        targets.insert(proxy_target().unwrap_or_else(|| DEFAULT_TARGET.to_string()));
    }
    targets
}

/// BLOCKING (a DNS lookup and a TCP connect, a few seconds at most). Whether `agent`'s API host
/// answers right now, reusing an answer younger than `ttl` (the watch's included). For a caller
/// that must know before it spends a try (account recovery's quick retries).
pub fn agent_reachable_now(agent: &str, ttl: Duration) -> bool {
    agent_target(agent).is_none_or(|target| reach_cached(&target, ttl))
}

/// The `host:port` that tells whether `agent` can reach its API: the proxy gxserver's environment
/// names when there is one (a computer that only gets out through it cannot connect straight to
/// the API), else the agent's base URL override or its own API host. `None` for an agent whose
/// API host Ghostex does not know: only the computer being offline holds it.
pub fn agent_target(agent: &str) -> Option<String> {
    let (base_url_env, host) = match agent {
        "claude" => ("ANTHROPIC_BASE_URL", "api.anthropic.com"),
        "codex" => ("OPENAI_BASE_URL", "chatgpt.com"),
        "gemini" => (
            "GOOGLE_GEMINI_BASE_URL",
            "generativelanguage.googleapis.com",
        ),
        _ => return None,
    };
    if let Some(proxy) = proxy_target() {
        return Some(proxy);
    }
    Some(env_url_target(base_url_env).unwrap_or_else(|| format!("{host}:443")))
}

fn proxy_target() -> Option<String> {
    ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]
        .iter()
        .find_map(|name| env_url_target(name))
}

fn env_url_target(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    url_target(value.trim())
}

/// `host:port` of a URL (`https://proxy.corp:8080` → `proxy.corp:8080`); a bare `host:port` too.
fn url_target(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let with_scheme = if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    };
    let url = url::Url::parse(&with_scheme).ok()?;
    let host = url.host_str()?;
    let port = url.port_or_known_default()?;
    Some(if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    })
}

/// "1 hour 46 minutes", "3 minutes", "under a minute": how long an outage lasted.
pub fn outage_length(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    let plural = |n: i64, word: &str| {
        if n == 1 {
            format!("1 {word}")
        } else {
            format!("{n} {word}s")
        }
    };
    match (minutes / 60, minutes % 60) {
        (0, 0) => "under a minute".to_string(),
        (0, m) => plural(m, "minute"),
        (h, 0) => plural(h, "hour"),
        (h, m) => format!("{} {}", plural(h, "hour"), plural(m, "minute")),
    }
}

/// "21:02" in this computer's time.
pub fn local_clock(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxies_and_base_urls_become_targets() {
        assert_eq!(
            url_target("http://proxy.corp:8080").as_deref(),
            Some("proxy.corp:8080")
        );
        assert_eq!(
            url_target("https://gateway.example").as_deref(),
            Some("gateway.example:443")
        );
        assert_eq!(url_target("proxy:3128").as_deref(), Some("proxy:3128"));
        assert_eq!(url_target(""), None);
    }

    #[test]
    fn unknown_agents_follow_only_the_computer() {
        assert_eq!(agent_target("opencode"), None);
        let _offline = simulate_offline(1);
        assert!(agent_offline(Some("opencode")));
        assert!(agent_offline(None));
    }

    #[test]
    fn a_down_api_holds_only_its_agent() {
        let claude = agent_target("claude").expect("claude target");
        let _net = simulate(Snapshot {
            offline_since: None,
            down: [claude].into_iter().collect(),
        });
        assert!(agent_offline(Some("claude")));
        assert!(!agent_offline(Some("opencode")));
        assert!(!is_offline());
    }

    #[test]
    fn outage_lengths_read_plainly() {
        assert_eq!(outage_length(30_000), "under a minute");
        assert_eq!(outage_length(3 * 60_000), "3 minutes");
        assert_eq!(outage_length(60 * 60_000), "1 hour");
        assert_eq!(outage_length(106 * 60_000), "1 hour 46 minutes");
    }
}

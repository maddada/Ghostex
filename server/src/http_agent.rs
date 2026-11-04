//! The settings every outbound HTTP request of gxserver starts from (ureq 3), and the few reads
//! ureq 2 did differently.

use std::sync::OnceLock;
use std::time::Duration;

use serde::de::DeserializeOwned;
use ureq::config::ConfigBuilder;
use ureq::http::Response;
use ureq::typestate::AgentScope;
use ureq::{Body, Error, Timeout};

/// CDXC:Build 2026-10-26 WHY: gxserver moved from ureq 2 to ureq 3, whose defaults differ: ureq 3 sends requests to the proxy named in `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY` (ureq 2 here never read them, and a proxy would swallow the loopback calls to gxserver, OpenCode and Executor), follows 10 redirects instead of 5, and has no connect timeout instead of 30 seconds. Every agent starts from this builder so those stay as they were; the one caller that wants the environment's proxy (agent_cli/latest.rs) sets it back.
pub(crate) fn config() -> ConfigBuilder<AgentScope> {
    ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(5)
        .timeout_connect(Some(Duration::from_secs(30)))
}

/// The shared agent for one-off requests (what `ureq::get` and `ureq::post` were). An error
/// status comes back as `Error::StatusCode`.
pub(crate) fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| config().build().into())
}

/// The shared agent for callers that read an error answer's body or headers themselves: every
/// status comes back as a response.
pub(crate) fn answering_agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| config().http_status_as_error(false).build().into())
}

/// ureq 2's `into_json`: the whole body, with no size limit. ureq 3's `read_json` stops at 10 MB,
/// which an OpenCode session or a release catalog can pass.
pub(crate) fn read_json<T: DeserializeOwned>(response: Response<Body>) -> serde_json::Result<T> {
    serde_json::from_reader(response.into_body().into_reader())
}

/// A response header as text.
pub(crate) fn header<'a>(response: &'a Response<Body>, name: &str) -> Option<&'a str> {
    response.headers().get(name)?.to_str().ok()
}

/// The request never reached the server: the name did not resolve, nothing accepted the
/// connection, or the TLS handshake failed (ureq 2's `Dns`/`ConnectionFailed` kinds).
pub(crate) fn never_connected(error: &Error) -> bool {
    match error {
        Error::HostNotFound
        | Error::ConnectionFailed
        | Error::Tls(_)
        | Error::Rustls(_)
        | Error::Timeout(Timeout::Resolve | Timeout::Connect) => true,
        Error::Io(io) => matches!(
            io.kind(),
            std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::AddrNotAvailable
                | std::io::ErrorKind::NetworkUnreachable
                | std::io::ErrorKind::HostUnreachable
        ),
        _ => false,
    }
}

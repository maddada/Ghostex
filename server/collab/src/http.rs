//! The HTTP agents the component's outgoing requests use (ureq 3), with gxserver's defaults: no
//! proxy from the environment (a proxy would swallow the loopback calls to gxserver), at most 5
//! redirects, a 30 second connect timeout.

use std::sync::OnceLock;
use std::time::Duration;

fn config() -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
    ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(5)
        .timeout_connect(Some(Duration::from_secs(30)))
}

/// For callers that read an error answer themselves: every status comes back as a response.
pub fn answering_agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| config().http_status_as_error(false).build().into())
}

/// For gxserver's loopback address: never redirected, never proxied.
pub fn loopback_agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        config()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(120)))
            .build()
            .into()
    })
}

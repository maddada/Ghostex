use serde_json::Value;

use crate::domain::DomainStateError;

pub(crate) const ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS: u64 = 5_000;
pub const GXSERVER_ZMX_COMMAND_STDOUT_LIMIT_BYTES: usize = 512 * 1024;
pub const GXSERVER_ZMX_COMMAND_STDERR_LIMIT_BYTES: usize = 64 * 1024;
pub const GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES: usize = 256 * 1024;
pub const GXSERVER_ZMX_PROCESS_SNAPSHOT_STDOUT_LIMIT_BYTES: usize = 1024 * 1024;
pub const GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ZmxProcessIdentity {
    pub agent_id: Option<String>,
    pub agent_session_id: Option<String>,
    pub agent_session_path: Option<String>,
    pub(crate) process_id: Option<i64>,
    pub(crate) terminal_name: Option<String>,
}

#[derive(Clone)]
pub struct ZmxServerContext {
    pub auth_token_file: String,
    pub base_url: String,
}

pub struct ZmxEndpointOutput {
    pub result: Value,
    pub presentation_session: Option<(String, String)>,
    pub(crate) created_workspace_terminal: Option<CreatedWorkspaceTerminalIdentity>,
}

#[derive(Clone, Debug)]
pub(crate) struct CreatedWorkspaceTerminalIdentity {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) zmx_executable_path: String,
    pub(crate) zmx_name: String,
}

pub(crate) struct CreatedWorkspaceTerminalError {
    pub(crate) error: ZmxEndpointError,
    pub(crate) presentation_session: Option<(String, String)>,
}

#[derive(Debug)]
pub enum ZmxEndpointError {
    DependencyUnavailable(String),
    Domain(DomainStateError),
}

impl From<DomainStateError> for ZmxEndpointError {
    fn from(error: DomainStateError) -> Self {
        Self::Domain(error)
    }
}

impl From<ZmxEndpointError> for CreatedWorkspaceTerminalError {
    fn from(error: ZmxEndpointError) -> Self {
        Self {
            error,
            presentation_session: None,
        }
    }
}

impl From<DomainStateError> for CreatedWorkspaceTerminalError {
    fn from(error: DomainStateError) -> Self {
        ZmxEndpointError::from(error).into()
    }
}

pub(crate) type ZmxEndpointResult<T> = Result<T, ZmxEndpointError>;

#[derive(Clone, Debug, Default)]
pub(crate) struct ZmxCommandOptions {
    pub(crate) allow_stdout_truncation: bool,
    pub(crate) stderr_limit_bytes: Option<usize>,
    pub(crate) stdin: Option<String>,
    pub(crate) stdout_limit_bytes: Option<usize>,
    pub(crate) timeout_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct ZmxCommandResult {
    pub(crate) exit_code: i32,
    pub(crate) stderr: String,
    pub(crate) stdout: String,
    pub(crate) stdout_truncated: bool,
}

/*
CDXC:ZmxWakeProbeReuse 2026-09-01:
One `/api/wakeSession` used to spawn three separate `zmx list` probes for the
same session: the attach-metadata probe, `start_session_provider`'s immediate
re-probe of the state its caller had just read, and a third probe after the
launchd start loop had itself just watched the session appear. Provider state
stays authoritative — this only lets ONE request hand the next step the state it
observed milliseconds earlier, and only for the two outcomes it actually saw.
Anything that did not observe the session passes `Unobserved` and probes.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObservedProviderState {
    /// Nothing was observed in this request; probe the provider.
    Unobserved,
    /// This request just saw the session alive (a `zmx list` that listed it).
    Exists,
    /// This request's own probe returned `missing` milliseconds ago.
    Missing,
}

/// The result of a provider start, plus whether the start path itself saw the
/// session registered rather than merely accepting the launch command.
pub(crate) struct ZmxStartOutcome {
    pub(crate) observed_alive: bool,
    pub(crate) result: ZmxCommandResult,
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderProbe {
    pub(crate) error: Option<String>,
    pub(crate) lifecycle_state: String,
    pub(crate) probed_at: String,
    pub(crate) zmx_name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderKill {
    pub(crate) error: Option<String>,
    pub(crate) exit_code: i32,
    pub(crate) killed: bool,
    pub(crate) stderr: String,
    pub(crate) stdout: String,
    pub(crate) zmx_name: String,
}

pub(crate) struct LifecycleParams {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}

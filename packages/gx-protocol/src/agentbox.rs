//! How a client names and draws an agentbox run location, shared by the New Thread picker and the
//! sidebar launcher (gx-core) and the chat composer's Run on row (gx-chat-core), so one location
//! looks the same everywhere.
//!
//! The labels themselves come from gxserver's `/api/agentbox status` (a registered SSH host is its
//! alias); these helpers add what a client draws around them.

/// The run location that is not a box, named when neither gxserver nor the host said the
/// computer's own name (CDXC:RemoteMachines 2026-11-19 in
/// packages/gx-chat-core/src/extras/where_pill.rs: a computer goes by its name, never "This
/// computer").
pub const THIS_COMPUTER_LABEL: &str = "Local";

/// The hover text of a location: `Your server <alias> over SSH` for a registered remote Docker
/// host (`docker:<alias>`), nothing for the others (their label says it all).
pub fn agentbox_location_tooltip(provider: &str) -> Option<String> {
    provider
        .strip_prefix("docker:")
        .map(|alias| format!("Your server {alias} over SSH"))
}

/// The icon id of a location kind: a box on this computer (`local`), a server over SSH
/// (`remoteDocker`), or the cloud.
pub fn agentbox_location_icon(kind: &str) -> &'static str {
    match kind {
        "local" => "box",
        "remoteDocker" => "server",
        _ => "cloud",
    }
}

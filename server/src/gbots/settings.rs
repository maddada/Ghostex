//! The three settings GBots reads from the desktop's settings file: the extension switch, what the
//! Bots space shows, and the computer the bots run on.

use serde_json::Value;

use crate::paths::GxserverPaths;

/// The GBots extension is on (`gbotsEnabled`, off by default).
pub(crate) fn gbots_enabled(paths: &GxserverPaths) -> bool {
    crate::session_lifecycle::read_sidebar_settings(paths)
        .and_then(|settings| settings.get("gbotsEnabled").and_then(Value::as_bool))
        == Some(true)
}

/// The server id of the computer the bots run on; `None` for this computer.
pub(crate) fn gbots_computer(paths: &GxserverPaths) -> Option<String> {
    crate::session_lifecycle::read_sidebar_settings(paths).and_then(|settings| {
        settings
            .get("gbotsComputer")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

/// Whether this gxserver is the one that runs the bots: another computer's desktop made it the
/// GBots computer (`claimHome`), or GBots is on here and no other computer was chosen.
///
/// CDXC:GBots 2026-11-07 DECISION:
/// User (Q1507): "i want to do 1 pc for now, so u pick 1 pc and everything is ran there only and we have all the accounts and everything saved there. then if u want to switch pc u switch the whole thing to another pc (we can add migration to another pc later)". The GBots card picks one computer (this one by default); the desktop tells that computer's gxserver it is the GBots computer, which leaves a marker in its own GBots folder, so the choice holds whatever that computer's own desktop settings say. Choosing another computer releases the old one; the bots and their conversations stay on it until moving them exists.
pub(crate) fn runs_here(paths: &GxserverPaths, server_id: &str) -> bool {
    is_claimed_home()
        || (gbots_enabled(paths)
            && gbots_computer(paths)
                .is_none_or(|computer| computer.eq_ignore_ascii_case(server_id)))
}

fn home_marker() -> std::path::PathBuf {
    super::sidecar::state_dir().join("home.json")
}

/// Another computer's desktop made this the GBots computer.
pub(crate) fn is_claimed_home() -> bool {
    home_marker().is_file()
}

/// Makes this the GBots computer (`claimHome`), as `by` (the computer that asked) chose.
pub(crate) fn claim_home(by: &str) -> std::io::Result<()> {
    let path = home_marker();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::json!({ "claimedBy": by, "claimedAt": crate::domain::now_iso() }).to_string(),
    )
}

/// This is no longer the GBots computer (`releaseHome`); its bots and data stay here.
pub(crate) fn release_home() {
    let _ = std::fs::remove_file(home_marker());
}

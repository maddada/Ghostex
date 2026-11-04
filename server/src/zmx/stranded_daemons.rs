//! Session daemons whose gxserver is gone for good, ended at startup.
//!
//! CDXC:Resources 2026-11-05 WHY:
//! Resources listed 24 `zmx` processes under Orphaned / Detached (pids 47354, 47398, 47446, …).
//! Each was `zmx run S78-…`, `S7n-…` or `S6m-…`: session daemons of isolated test gxservers
//! (`GHOSTEX_HOME` under agent-work, their own port) that agents started for proofs on Oct 30
//! and 31 and then deleted with their homes. Every gxserver shares the user's zmx socket folder,
//! and a stopped gxserver leaves its daemons running on purpose (a sleeping session must survive
//! a restart), but only the owning server ends its unowned daemons
//! (`end_unowned_zmx_session_daemons` touches its own name prefix), so once a home was deleted
//! nothing could ever end them. The user: "look into this pls and fix".
//!
//! A daemon of another server id is ended only when its gxserver can never come back: the
//! `GHOSTEX_GXSERVER_AUTH_TOKEN_FILE` the daemon itself was started with names a gxserver state
//! folder that no longer exists, or one whose `identity.json` now names another server id (no row
//! of that server can own the name). A folder that exists with the same server id, a token path
//! of another shape, a daemon whose environment is unreadable, or a folder on a volume that is not
//! mounted keeps its daemon: that server may still restore the session.
//! SEE-ALSO: wire_cycle.rs (`end_unowned_zmx_session_daemons`), resources_snapshot.rs (Orphaned / Detached).

use std::path::{Component, Path};

use serde_json::json;

use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};

use super::process_identity::zmx_daemon_processes;
use super::wire_cycle::{read_process_snapshot, terminate_zmx_session_daemon_processes};

const AUTH_TOKEN_FILE_ENV: &str = "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE";

/// Ends every live session daemon of another gxserver whose state folder is gone for good. Runs beside the unowned-daemon pass at startup.
pub(crate) fn end_stranded_zmx_session_daemons(logger: &GxserverLogger, server_id: &str) {
    let Ok(ps_output) = read_process_snapshot() else {
        return;
    };
    let stranded: Vec<(String, &'static str)> = zmx_daemon_processes(&ps_output)
        .into_iter()
        .filter_map(|(pid, name)| {
            let owner = foreign_session_server_id(&name, server_id)?;
            let token_file = process_environment_value(pid, AUTH_TOKEN_FILE_ENV)?;
            let reason = stranded_owner_reason(owner, Path::new(&token_file))?;
            Some((name, reason))
        })
        .collect();
    if stranded.is_empty() {
        return;
    }
    let outcomes = terminate_zmx_session_daemon_processes(
        &stranded
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
    );
    for ((zmx_name, reason), outcome) in stranded.into_iter().zip(outcomes) {
        let _ = logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: match outcome {
                Ok(()) => "strandedSessionDaemonEnded",
                Err(_) => "strandedSessionDaemonEndFailed",
            }
            .to_string(),
            server_id: Some(server_id.to_string()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(match &outcome {
                Ok(()) => format!("A session terminal of another Ghostex server was still running after {reason}, so it was ended."),
                Err(error) => format!("A session terminal of another Ghostex server is still running after {reason} and could not be ended: {error}"),
            }),
            details: Some(json!({ "providerSessionId": zmx_name })),
        });
    }
}

/// The server id of a well-formed `<server>-<project>-<session>` zmx name when it is not this server's.
fn foreign_session_server_id<'a>(zmx_name: &'a str, server_id: &str) -> Option<&'a str> {
    let mut parts = zmx_name.split('-');
    let (owner, project, session) = (parts.next()?, parts.next()?, parts.next()?);
    (parts.next().is_none()
        && owner != server_id
        && crate::ids::is_gxserver_server_id(owner)
        && crate::ids::is_gxserver_project_id(project)
        && crate::ids::is_gxserver_session_id(session))
    .then_some(owner)
}

/// Why the gxserver behind `auth_token_file` can never restore a session of `owner`, or `None` when it might.
fn stranded_owner_reason(owner: &str, auth_token_file: &Path) -> Option<&'static str> {
    if !auth_token_file.is_absolute()
        || auth_token_file.file_name()? != "token"
        || auth_token_file.parent()?.file_name()? != "auth"
    {
        return None;
    }
    let root = auth_token_file.parent()?.parent()?;
    match std::fs::read_to_string(root.join("identity.json")) {
        Ok(text) => {
            let value: serde_json::Value = serde_json::from_str(&text).ok()?;
            let current = value.get("serverId")?.as_str()?;
            (crate::ids::is_gxserver_server_id(current) && current != owner)
                .then_some("its Ghostex state folder was reset")
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (!root.exists()
            && on_present_volume(root))
        .then_some("its Ghostex state folder was deleted"),
        Err(_) => None,
    }
}

/// False when the deepest folder of `path` that still exists is the root or a mount parent (`/Volumes`, `/media/<user>`, …): the folder may only be on a disk that is not mounted right now.
fn on_present_volume(path: &Path) -> bool {
    let Some(existing) = path.ancestors().find(|ancestor| ancestor.is_dir()) else {
        return false;
    };
    let names: Vec<_> = existing
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    !matches!(
        names.as_slice(),
        [] | ["Volumes"]
            | ["media"]
            | ["media", _]
            | ["mnt"]
            | ["run", "media"]
            | ["run", "media", _]
    )
}

#[cfg(target_os = "macos")]
fn process_environment_value(pid: i64, name: &str) -> Option<String> {
    super::process_table_macos::process_environment_value(pid, name)
}

#[cfg(not(target_os = "macos"))]
fn process_environment_value(pid: i64, name: &str) -> Option<String> {
    let environment = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    let prefix = format!("{name}=");
    environment
        .split(|byte| *byte == 0)
        .find_map(|entry| entry.strip_prefix(prefix.as_bytes()))
        .map(|value| String::from_utf8_lossy(value).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_root(home: &Path, server_id: Option<&str>) -> std::path::PathBuf {
        let root = home.join("state").join("gxserver");
        std::fs::create_dir_all(root.join("auth")).expect("root");
        if let Some(server_id) = server_id {
            std::fs::write(
                root.join("identity.json"),
                format!(
                    "{{\"createdAt\":\"2026-10-30T07:00:00.000Z\",\"serverId\":\"{server_id}\"}}\n"
                ),
            )
            .expect("identity");
        }
        root
    }

    #[test]
    fn only_well_formed_names_of_other_servers_are_candidates() {
        assert_eq!(
            foreign_session_server_id("S78-P4z2j-G8swm", "S90"),
            Some("S78")
        );
        assert_eq!(foreign_session_server_id("S90-P3lv0-G9jt7", "S90"), None);
        assert_eq!(foreign_session_server_id("S78-P4z2j-G8swm-x", "S90"), None);
        assert_eq!(foreign_session_server_id("S78-P4z2j", "S90"), None);
        assert_eq!(
            foreign_session_server_id("scratch-P4z2j-G8swm", "S90"),
            None
        );
        assert_eq!(foreign_session_server_id("S78-project-G8swm", "S90"), None);
    }

    #[test]
    fn a_deleted_state_folder_strands_its_daemons() {
        let parent = tempfile::tempdir().expect("tempdir");
        let home = parent.path().join("gx-home");
        let root = server_root(&home, Some("S78"));
        let token = root.join("auth").join("token");
        assert_eq!(
            stranded_owner_reason("S78", &token),
            None,
            "a live home keeps its daemons"
        );
        std::fs::remove_dir_all(&home).expect("delete home");
        assert_eq!(
            stranded_owner_reason("S78", &token),
            Some("its Ghostex state folder was deleted")
        );
    }

    #[test]
    fn a_state_folder_now_owned_by_another_server_strands_its_daemons() {
        let home = tempfile::tempdir().expect("tempdir");
        let root = server_root(home.path(), Some("S12"));
        assert_eq!(
            stranded_owner_reason("S78", &root.join("auth").join("token")),
            Some("its Ghostex state folder was reset")
        );
    }

    #[test]
    fn a_sleeping_servers_daemons_survive() {
        // The folder is there and still names the daemon's server: that server, running or not, may restore the session.
        let home = tempfile::tempdir().expect("tempdir");
        let root = server_root(home.path(), Some("S78"));
        assert_eq!(
            stranded_owner_reason("S78", &root.join("auth").join("token")),
            None
        );
    }

    #[test]
    fn unclear_ownership_keeps_the_daemon() {
        let home = tempfile::tempdir().expect("tempdir");
        let root = server_root(home.path(), None);
        let token = root.join("auth").join("token");
        assert_eq!(
            stranded_owner_reason("S78", &token),
            None,
            "no identity yet"
        );
        std::fs::write(root.join("identity.json"), "not json").expect("identity");
        assert_eq!(
            stranded_owner_reason("S78", &token),
            None,
            "unreadable identity"
        );
        assert_eq!(
            stranded_owner_reason("S78", Path::new("relative/state/gxserver/auth/token")),
            None
        );
        assert_eq!(
            stranded_owner_reason("S78", &home.path().join("gone").join("token")),
            None,
            "not an auth/token path"
        );
        assert_eq!(
            stranded_owner_reason(
                "S78",
                Path::new("/Volumes/ghostex-not-mounted-0d9e/gx-home/state/gxserver/auth/token")
            ),
            None,
            "a disk that is not mounted"
        );
    }

    #[test]
    fn mount_parents_are_not_present_volumes() {
        assert!(!on_present_volume(Path::new(
            "/Volumes/ghostex-not-mounted-0d9e/a/b"
        )));
        assert!(!on_present_volume(Path::new(
            "/ghostex-not-a-folder-0d9e/a"
        )));
        let home = tempfile::tempdir().expect("tempdir");
        assert!(on_present_volume(
            &home.path().join("deleted").join("state")
        ));
    }
}

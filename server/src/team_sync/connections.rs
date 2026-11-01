//! Each workspace's connection to its team's Convex project: the deployment URL and this member's
//! token, kept under `teamSync` in the private work-mode credentials file.
//!
//! CDXC:TeamSync 2026-10-09 WHY:
//! The member token is the only thing that proves who this Ghostex is to the team's Convex project,
//! so it lives next to the Linear keys in the private credentials file (0600 on macOS and Linux),
//! is never published, and no endpoint returns it.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::paths::GxserverPaths;
use crate::work_mode::{change_work_source_config, mutate_credentials, read_credentials};

const CREDENTIALS_KEY: &str = "teamSync";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TeamConnection {
    #[serde(skip)]
    pub(crate) workspace_id: String,
    /// `https://<name>.convex.cloud`, without a trailing slash.
    pub(crate) deployment_url: String,
    pub(crate) member_token: String,
    #[serde(default)]
    pub(crate) team_name: Option<String>,
    #[serde(default)]
    pub(crate) member_id: Option<String>,
    #[serde(default)]
    pub(crate) member_name: Option<String>,
    #[serde(default)]
    pub(crate) connected_at: Option<String>,
    /// "Create my Slack tickets with my own Linear key": this workspace's Linear key is kept in the
    /// team's Convex project for this member (crate::team_sync::linear_keys).
    #[serde(default)]
    pub(crate) own_linear_key: bool,
}

impl TeamConnection {
    /// What endpoints and the CLI may show: everything but the token.
    pub(crate) fn summary(&self) -> Value {
        serde_json::json!({
            "workspaceId": self.workspace_id,
            "deploymentUrl": self.deployment_url,
            "siteUrl": super::invite_link::site_url(&self.deployment_url),
            "teamName": self.team_name,
            "memberId": self.member_id,
            "memberName": self.member_name,
            "connectedAt": self.connected_at,
            "ownLinearKey": self.own_linear_key,
        })
    }
}

fn connections_map(credentials: &Map<String, Value>) -> Map<String, Value> {
    credentials
        .get(CREDENTIALS_KEY)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Every workspace's connection, ordered by workspace id.
pub(crate) fn read_team_connections(paths: &GxserverPaths) -> Vec<TeamConnection> {
    let mut connections: Vec<TeamConnection> = connections_map(&read_credentials(paths))
        .into_iter()
        .filter_map(|(workspace_id, value)| {
            let mut connection = serde_json::from_value::<TeamConnection>(value).ok()?;
            if connection.deployment_url.trim().is_empty()
                || connection.member_token.trim().is_empty()
            {
                return None;
            }
            connection.workspace_id = workspace_id;
            Some(connection)
        })
        .collect();
    connections.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
    connections
}

pub(crate) fn read_team_connection(
    paths: &GxserverPaths,
    workspace_id: &str,
) -> Option<TeamConnection> {
    read_team_connections(paths)
        .into_iter()
        .find(|connection| connection.workspace_id == workspace_id)
}

/// Saves (replacing) one workspace's connection. Joining a team is a change of the workspace's
/// ticket command setup too (a team workspace runs none), so it goes through the work-source
/// store's writer lock, which bumps the workspace's generation, before the credentials file.
pub(crate) fn store_team_connection(
    paths: &GxserverPaths,
    connection: &TeamConnection,
) -> std::io::Result<()> {
    change_work_source_config(&connection.workspace_id, || {
        mutate_credentials(paths, |credentials| {
            let mut connections = connections_map(credentials);
            connections.insert(
                connection.workspace_id.clone(),
                serde_json::to_value(connection).unwrap_or(Value::Null),
            );
            credentials.insert(CREDENTIALS_KEY.to_string(), Value::Object(connections));
        })
    })
}

/// Forgets one workspace's connection. Returns whether there was one.
pub(crate) fn remove_team_connection(
    paths: &GxserverPaths,
    workspace_id: &str,
) -> std::io::Result<bool> {
    // Nothing to remove: the file is left as it is.
    if !connections_map(&read_credentials(paths)).contains_key(workspace_id) {
        return Ok(false);
    }
    change_work_source_config(workspace_id, || {
        mutate_credentials(paths, |credentials| {
            let mut connections = connections_map(credentials);
            let removed = connections.remove(workspace_id).is_some();
            if removed {
                credentials.insert(CREDENTIALS_KEY.to_string(), Value::Object(connections));
            }
            removed
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every writer of the credentials file at once (the real team writer, the Linear key's and a
    /// ticket command's), 50 rounds each: none loses another's change.
    #[test]
    fn concurrent_credential_writers_keep_every_change() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        std::thread::scope(|scope| {
            for writer in 0..3 {
                let paths = &paths;
                scope.spawn(move || {
                    for round in 0..50 {
                        match writer {
                            0 => crate::work_mode::mutate_credentials(paths, |credentials| {
                                crate::work_mode::store_source_credentials(
                                    credentials,
                                    "ws-a",
                                    "fixture",
                                    &[(format!("f{round}"), Some(format!("v{round}")))],
                                )
                            })
                            .expect("write"),
                            1 => crate::work_mode::store_linear_api_key(
                                paths,
                                crate::work_mode::LinearKeyScope::Project(&format!("P{round}")),
                                Some(&format!("lin_api_{round}")),
                            )
                            .expect("write"),
                            _ => store_team_connection(
                                paths,
                                &TeamConnection {
                                    workspace_id: format!("ws-team-{round}"),
                                    deployment_url: "https://team.example".to_string(),
                                    member_token: format!("member-{round}"),
                                    team_name: None,
                                    member_id: None,
                                    member_name: None,
                                    connected_at: None,
                                    own_linear_key: false,
                                },
                            )
                            .expect("write"),
                        }
                    }
                });
            }
        });
        let credentials = read_credentials(&paths);
        for round in 0..50 {
            assert_eq!(
                credentials["workspaceSourceCredentials"]["ws-a"]["fixture"][format!("f{round}")],
                serde_json::json!(format!("v{round}"))
            );
            assert_eq!(
                credentials["projectLinearApiKeys"][format!("P{round}")],
                serde_json::json!(format!("lin_api_{round}"))
            );
            assert!(read_team_connection(&paths, &format!("ws-team-{round}")).is_some());
        }
    }

    #[test]
    fn removing_a_connection_that_isnt_there_writes_nothing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        assert!(!remove_team_connection(&paths, "ws-none").expect("nothing to remove"));
        assert!(!paths.auth_dir.join("work-mode-credentials.json").exists());
    }

    #[test]
    fn joining_or_leaving_a_team_bumps_the_workspace_generation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        let connection = TeamConnection {
            workspace_id: "ws-team-gen".to_string(),
            deployment_url: "https://team.example".to_string(),
            member_token: "member-token".to_string(),
            team_name: None,
            member_id: None,
            member_name: None,
            connected_at: None,
            own_linear_key: false,
        };
        let before = crate::work_mode::work_source_generation("ws-team-gen");
        store_team_connection(&paths, &connection).expect("joined");
        let joined = crate::work_mode::work_source_generation("ws-team-gen");
        assert_eq!(joined, before + 1);
        assert!(read_team_connection(&paths, "ws-team-gen").is_some());
        assert!(remove_team_connection(&paths, "ws-team-gen").expect("left"));
        assert_eq!(
            crate::work_mode::work_source_generation("ws-team-gen"),
            joined + 1
        );
    }
}

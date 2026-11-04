//! The command-line tools a workspace can sign in to on its own (GitHub's `gh` and Google
//! Workspace's `gws`): where each keeps the workspace's sign-in, whether it has one, and the
//! variables that point an agent's copy of the tool at it.
//!
//! CDXC:Connectors 2026-10-16 DECISION:
//! User (Q16, Q17): each workspace gets its own `gh` and `gws` sign-in, handed to the agents that start there and shown on the Connectors page with a Sign in button; `gws` is Google's Workspace CLI. A workspace without its own sign-in leaves the tool as the user set it up, so their usual sign-in applies.
//!
//! CDXC:Connectors 2026-10-16 WHY:
//! `gws` keeps its credentials encrypted with a key in the system keychain by default, under one name for every config folder, so a second folder's sign-in replaced the key and the first one's credentials no longer decrypted ("Could not decrypt", seen on this computer). Each workspace's `gws` keeps its key in its own folder (`GOOGLE_WORKSPACE_CLI_KEYRING_BACKEND=file`).

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

use super::super::store::file_stem;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliTool {
    Gh,
    Gws,
}

impl CliTool {
    pub(crate) const ALL: [CliTool; 2] = [CliTool::Gh, CliTool::Gws];

    pub(crate) fn parse(text: &str) -> Option<CliTool> {
        match text.trim().to_ascii_lowercase().as_str() {
            "gh" | "github" => Some(CliTool::Gh),
            "gws" | "google" | "google-workspace" => Some(CliTool::Gws),
            _ => None,
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            CliTool::Gh => "gh",
            CliTool::Gws => "gws",
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            CliTool::Gh => "GitHub CLI (gh)",
            CliTool::Gws => "Google Workspace CLI (gws)",
        }
    }
}

/// `<gxserver state>/connectors/cli/<workspace>/<tool>`: the tool's config folder for the
/// workspace.
pub(crate) fn config_dir(paths: &GxserverPaths, workspace: &str, tool: CliTool) -> PathBuf {
    paths
        .root_dir
        .join("connectors")
        .join("cli")
        .join(file_stem(workspace))
        .join(tool.id())
}

/// The tool on this computer, found the way a program started now would find it.
pub(crate) fn executable(tool: CliTool) -> Option<PathBuf> {
    crate::platform::live_path::find(tool.id(), &[])
}

/// The workspace's `gh` `hosts.yml`, when it has one.
fn gh_hosts(paths: &GxserverPaths, workspace: &str) -> Option<String> {
    fs::read_to_string(config_dir(paths, workspace, CliTool::Gh).join("hosts.yml")).ok()
}

/// The `github.com:` block's lines (indented lines under the host), as `gh` writes them.
fn github_block(hosts: &str) -> impl Iterator<Item = &str> {
    let mut in_github = false;
    hosts.lines().filter(move |line| {
        if !line.starts_with(' ') && !line.starts_with('\t') {
            in_github = line.trim_end() == "github.com:";
            return false;
        }
        in_github
    })
}

/// The GitHub account a `gh` config folder is signed in as (`user:` under `github.com:`).
fn gh_account(paths: &GxserverPaths, workspace: &str) -> Option<String> {
    github_account(&gh_hosts(paths, workspace)?)
}

fn github_account(hosts: &str) -> Option<String> {
    github_block(hosts)
        .find(|line| line.starts_with("    user:"))
        .and_then(|line| line.trim().strip_prefix("user:"))
        .map(str::trim)
        .filter(|user| !user.is_empty())
        .map(str::to_string)
}

/// Whether the folder's `hosts.yml` holds the token itself (`oauth_token:`), rather than leaving
/// it in the system keychain.
fn github_has_file_token(hosts: &str) -> bool {
    github_block(hosts).any(|line| {
        line.trim()
            .strip_prefix("oauth_token:")
            .is_some_and(|token| !token.trim().is_empty())
    })
}

/// CDXC:Connectors 2026-11-05 WHY:
/// A workspace `gh` folder whose token is in the system keychain (not its own `hosts.yml`) works only in shells that can read the keychain; elsewhere `gh` and `git push` fail ("could not read Username"), while the user's usual `gh` (token in its file) works. Ghostex signs in with `--insecure-storage` so the token stays in the folder, but a later `gh auth login` or refresh in that folder without it moves the token to the keychain. Such a folder is not counted as signed in, so agents keep the user's usual `gh`, and Settings and `ghostex connectors gh` say the sign-in needs redoing.
fn gh_needs_sign_in_again(paths: &GxserverPaths, workspace: &str) -> bool {
    gh_hosts(paths, workspace)
        .is_some_and(|hosts| github_account(&hosts).is_some() && !github_has_file_token(&hosts))
}

fn gws_signed_in(paths: &GxserverPaths, workspace: &str) -> bool {
    let dir = config_dir(paths, workspace, CliTool::Gws);
    dir.join("credentials.enc").is_file() || dir.join("credentials.json").is_file()
}

/// Whether the workspace has its own sign-in for the tool.
pub(crate) fn signed_in(paths: &GxserverPaths, workspace: &str, tool: CliTool) -> bool {
    match tool {
        CliTool::Gh => gh_hosts(paths, workspace).is_some_and(|hosts| {
            github_account(&hosts).is_some() && github_has_file_token(&hosts)
        }),
        CliTool::Gws => gws_signed_in(paths, workspace),
    }
}

/// The workspace's tools as Settings shows them.
pub(crate) fn status(paths: &GxserverPaths, workspace: &str) -> Value {
    let mut tools = serde_json::Map::new();
    for tool in CliTool::ALL {
        let account = match tool {
            CliTool::Gh => gh_account(paths, workspace),
            CliTool::Gws => None,
        };
        tools.insert(
            tool.id().to_string(),
            json!({
                "name": tool.name(),
                "installed": executable(tool).is_some(),
                "signedIn": signed_in(paths, workspace, tool),
                "account": account,
                "needsSignInAgain": tool == CliTool::Gh && gh_needs_sign_in_again(paths, workspace),
                "signingIn": super::signin::signing_in(paths, workspace, tool),
                "client": (tool == CliTool::Gws).then(|| super::client::client_status(paths, workspace)),
            }),
        );
    }
    Value::Object(tools)
}

/// The variables that point a tool at the workspace's config folder.
pub(crate) fn tool_env(
    paths: &GxserverPaths,
    workspace: &str,
    tool: CliTool,
) -> Vec<(String, String)> {
    let dir = config_dir(paths, workspace, tool)
        .to_string_lossy()
        .into_owned();
    match tool {
        CliTool::Gh => vec![("GH_CONFIG_DIR".to_string(), dir)],
        CliTool::Gws => vec![
            ("GOOGLE_WORKSPACE_CLI_CONFIG_DIR".to_string(), dir),
            (
                "GOOGLE_WORKSPACE_CLI_KEYRING_BACKEND".to_string(),
                "file".to_string(),
            ),
        ],
    }
}

/// The variables an agent in the workspace gets: one set per tool the workspace signed in to.
pub(crate) fn session_env(paths: &GxserverPaths, workspace: &str) -> Vec<(String, String)> {
    CliTool::ALL
        .into_iter()
        .filter(|tool| signed_in(paths, workspace, *tool))
        .flat_map(|tool| tool_env(paths, workspace, tool))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_with_hosts(hosts: &str) -> (tempfile::TempDir, GxserverPaths) {
        let home = tempfile::tempdir().expect("temp home");
        let paths = crate::paths::get_gxserver_paths(Some(home.path().to_path_buf()));
        let dir = config_dir(&paths, "work", CliTool::Gh);
        fs::create_dir_all(&dir).expect("gh folder");
        fs::write(dir.join("hosts.yml"), hosts).expect("hosts.yml");
        (home, paths)
    }

    const FILE_TOKEN: &str = "github.com:\n    users:\n        maddada:\n            oauth_token: gho_example\n    git_protocol: https\n    user: maddada\n    oauth_token: gho_example\n";
    const KEYCHAIN_ONLY: &str =
        "github.com:\n    users:\n        maddada: {}\n    git_protocol: https\n    user: maddada\n";

    #[test]
    fn a_gh_folder_with_its_own_token_is_signed_in_and_handed_to_agents() {
        let (_home, paths) = workspace_with_hosts(FILE_TOKEN);
        assert!(signed_in(&paths, "work", CliTool::Gh));
        assert!(!gh_needs_sign_in_again(&paths, "work"));
        let env = session_env(&paths, "work");
        assert!(env.iter().any(|(name, _)| name == "GH_CONFIG_DIR"));
    }

    #[test]
    fn a_gh_folder_whose_token_is_only_in_the_keychain_is_not_handed_to_agents() {
        let (_home, paths) = workspace_with_hosts(KEYCHAIN_ONLY);
        assert_eq!(gh_account(&paths, "work").as_deref(), Some("maddada"));
        assert!(!signed_in(&paths, "work", CliTool::Gh));
        assert!(gh_needs_sign_in_again(&paths, "work"));
        assert!(session_env(&paths, "work")
            .iter()
            .all(|(name, _)| name != "GH_CONFIG_DIR"));
        let status = status(&paths, "work");
        assert_eq!(status["gh"]["signedIn"], json!(false));
        assert_eq!(status["gh"]["needsSignInAgain"], json!(true));
        assert_eq!(status["gh"]["account"], json!("maddada"));
    }

    #[test]
    fn an_empty_token_or_a_folder_without_hosts_is_not_signed_in() {
        let (_home, paths) =
            workspace_with_hosts("github.com:\n    user: maddada\n    oauth_token:\n");
        assert!(!signed_in(&paths, "work", CliTool::Gh));
        let home = tempfile::tempdir().expect("temp home");
        let paths = crate::paths::get_gxserver_paths(Some(home.path().to_path_buf()));
        assert!(!signed_in(&paths, "work", CliTool::Gh));
        assert!(!gh_needs_sign_in_again(&paths, "work"));
    }
}

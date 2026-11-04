//! `POST /api/machineIntegrations`: the integrations of the machine this gxserver runs on, as
//! Settings > Integrations shows them for a picked machine: its platform, the `ghostex` command its
//! new terminals run, and every bundled agent skill with Install and Uninstall.
//!
//! CDXC:RemoteMachines 2026-10-26 DECISION:
//! User: "we need to show a dropdown at the top on agents page showing all the machines in settings and allowing us to update different remote machines. also same for integrations page (all should go through gxserver on that remote machine basically)". Agent CLIs and Tools already had gxserver APIs (`/api/agentCliMaintenance`, `/api/managedTools`); the skills and the CLI row were read and installed by the desktop app, so this endpoint gives every client the same answer from the machine's own gxserver. Installs run the code `ghostex <skill> install-skill` runs (`agent_skills::install_agent_skills` with the package source the CLI resolves); uninstall removes the canonical copy as the desktop's Uninstall does.
//! SEE-ALSO: apps/desktop/src/app/remote_conn/settings_rpc.rs (the desktop's route to a machine), apps/desktop/src/app/window/settings_modal/tabs/integrations/remote_machine.rs.
use crate::{
    agent_skills::{
        self, CANONICAL_AGENT_SKILLS_DIR, GHOSTEX_AGENT_SKILL_NAMES,
        MACOS_ONLY_GHOSTEX_AGENT_SKILL_NAMES, ORCHESTRATOR_ONLY_GHOSTEX_AGENT_SKILL_NAMES,
        SPACEO_MACOS_ONLY_MESSAGE,
    },
    agent_skills_remote,
    domain::DomainStateError,
    paths::GxserverPaths,
};
use serde_json::{json, Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) async fn dispatch(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    match params
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("status")
    {
        "status" => status(paths).await,
        "installSkill" => {
            let skill = skill_param(params)?;
            install_bundled_skill(paths, &skill)
                .await
                .map_err(DomainStateError::bad_request)?;
            status(paths).await
        }
        "uninstallSkill" => {
            let skill = skill_param(params)?;
            uninstall_skill(paths.agent_config_home_dir(), &skill)?;
            status(paths).await
        }
        _ => Err(DomainStateError::bad_request(
            "Unknown integrations action.",
        )),
    }
}

/// `std::env::consts::OS` in the words the Settings page uses.
fn platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

/// The bundled skills a person installs from Settings: every shipped one but the orchestrator
/// playbook (installed only for an orchestrator), each once.
fn listed_skill_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for name in GHOSTEX_AGENT_SKILL_NAMES {
        if !ORCHESTRATOR_ONLY_GHOSTEX_AGENT_SKILL_NAMES.contains(name) && !names.contains(name) {
            names.push(name);
        }
    }
    names
}

fn skill_unavailable_reason(skill: &str, platform: &str) -> Option<&'static str> {
    (platform != "macos" && MACOS_ONLY_GHOSTEX_AGENT_SKILL_NAMES.contains(&skill))
        .then_some(SPACEO_MACOS_ONLY_MESSAGE)
}

/// The Settings catalog's entry for a bundled skill (`name`, `description`, `tier`), which the
/// desktop reads itself and a client without the catalog (the phone) reads here.
fn catalog_skill(skill: &str) -> Option<Value> {
    use ghostex_settings_catalog::modules::{exports, module};
    exports(
        module::AGENT_SKILLS,
        ghostex_settings_catalog::Platform::current(),
    )
    .into_iter()
    .find(|(name, _)| *name == "VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS")?
    .1
    .to_value()
    .as_array()?
    .iter()
    .find(|entry| entry.get("skillName").and_then(Value::as_str) == Some(skill))
    .cloned()
}

/// The skills' state as the desktop reads its own (`~/.agents/skills/<name>/SKILL.md`).
fn skill_rows(home: &Path, platform: &str) -> Vec<Value> {
    let canonical = home.join(CANONICAL_AGENT_SKILLS_DIR);
    listed_skill_names()
        .into_iter()
        .map(|skill| {
            let entry = catalog_skill(skill);
            let text = |key: &str| {
                entry
                    .as_ref()
                    .and_then(|entry| entry.get(key)?.as_str().map(str::to_string))
            };
            json!({
                "skillName": skill,
                "name": text("name"),
                "description": text("description"),
                "installed": canonical.join(skill).join("SKILL.md").is_file(),
                "unavailableReason": skill_unavailable_reason(skill, platform),
            })
        })
        .collect()
}

async fn status(paths: &GxserverPaths) -> Result<Value, DomainStateError> {
    let home = paths.agent_config_home_dir().to_path_buf();
    let platform = platform();
    let skills = skill_rows(&home, platform);
    // What a new terminal on this machine runs as `ghostex` (the agent CLI probe's PATH).
    let cli_path =
        tokio::task::spawn_blocking(move || crate::agent_cli::process::resolve("ghostex", &home))
            .await
            .map_err(internal)?;
    Ok(json!({
        "platform": platform,
        "ghostexCli": { "installed": cli_path.is_some(), "path": cli_path },
        "skills": skills,
    }))
}

/// Installs one bundled skill the way `ghostex <skill> install-skill` does: the package source the
/// CLI resolves, GitHub main when it can be downloaded. Managed-tool installs use it for the skill a
/// tool brings (managed_tools/desktop_control.rs).
pub(crate) async fn install_bundled_skill(
    paths: &GxserverPaths,
    skill: &str,
) -> Result<(), String> {
    let owned = skill.to_string();
    let source = tokio::task::spawn_blocking(move || {
        crate::ghostex_cli::skills::resolve_ghostex_agent_skill_package_source(
            &owned,
            &[],
            &crate::ghostex_cli::args::Flags::default(),
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;
    install_skill_from(paths, skill, &source, true)
        .await
        .map(|_| ())
        .map_err(|error| error.message)
}

fn skill_param(params: &Map<String, Value>) -> Result<String, DomainStateError> {
    let skill = params
        .get("skillName")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if !listed_skill_names().contains(&skill) {
        return Err(DomainStateError::bad_request(
            "Unknown Ghostex agent skill.",
        ));
    }
    if let Some(reason) = skill_unavailable_reason(skill, platform()) {
        return Err(DomainStateError::bad_request(reason));
    }
    Ok(skill.to_string())
}

async fn install_skill_from(
    paths: &GxserverPaths,
    skill: &str,
    package_source: &str,
    download: bool,
) -> Result<Value, DomainStateError> {
    let mut params = Map::new();
    params.insert("packageSource".into(), json!(package_source));
    params.insert("skillNames".into(), json!([skill]));
    params.insert("remote".into(), json!(download));
    agent_skills::install_agent_skills(paths, &params).await
}

/// Removes the canonical copy (a link is removed, never what it points at) and the links an install
/// put in each agent's own skill folder to it; a real folder there is the user's and stays. An
/// absent skill is not an error.
fn uninstall_skill(home: &Path, skill: &str) -> Result<(), DomainStateError> {
    let path = home.join(CANONICAL_AGENT_SKILLS_DIR).join(skill);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(internal(error)),
    };
    let canonical = fs::canonicalize(&path).ok();
    let agent_links: Vec<PathBuf> = agent_skills_remote::installed_skill_roots(home)
        .into_iter()
        .map(|root| root.join(skill))
        .filter(|link| *link != path)
        .filter(|link| {
            fs::symlink_metadata(link).is_ok_and(|meta| meta.file_type().is_symlink())
                && canonical.is_some()
                && fs::canonicalize(link).ok() == canonical
        })
        .collect();
    // A link (on Windows also a junction) is removed as a link, never followed.
    if metadata.file_type().is_symlink() {
        ghostex_agent_sync::fsx::remove_link(&path)
    } else if metadata.is_dir() {
        fs::remove_dir_all(&path)
    } else {
        fs::remove_file(&path)
    }
    .map_err(internal)?;
    for link in agent_links {
        let _ = ghostex_agent_sync::fsx::remove_link(&link);
    }
    Ok(())
}

fn internal(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("The integrations request failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;

    fn write_skill(dir: &Path, name: &str) {
        fs::create_dir_all(dir).expect("skill dir");
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: test\n---\n"),
        )
        .expect("SKILL.md");
    }

    #[test]
    fn listed_skills_leave_out_the_orchestrator_playbook_and_repeat_none() {
        let names = listed_skill_names();
        assert!(!names.contains(&"ghostex-orchestrator"));
        assert!(names.contains(&"ghostex-cli"));
        let mut unique = names.clone();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
        assert_eq!(
            names
                .iter()
                .filter(|name| **name == "ghostex-manage-beads")
                .count(),
            1
        );
    }

    #[test]
    fn macos_only_skills_say_why_on_other_platforms() {
        let temp = tempfile::tempdir().expect("tempdir");
        let row = |platform: &str| {
            skill_rows(temp.path(), platform)
                .into_iter()
                .find(|row| row["skillName"] == "ghostex-spaceo")
                .expect("spaceo row")
        };
        assert_eq!(row("linux")["unavailableReason"], SPACEO_MACOS_ONLY_MESSAGE);
        assert_eq!(
            row("windows")["unavailableReason"],
            SPACEO_MACOS_ONLY_MESSAGE
        );
        assert!(row("macos")["unavailableReason"].is_null());
    }

    #[test]
    fn a_skill_is_installed_when_its_canonical_copy_has_a_skill_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_skill(
            &temp.path().join(".agents/skills/ghostex-cli"),
            "ghostex-cli",
        );
        // A copy only in an agent's own folder is not the canonical one the desktop reads.
        write_skill(
            &temp.path().join(".claude/skills/ghostex-help"),
            "ghostex-help",
        );
        let rows = skill_rows(temp.path(), "linux");
        let installed = |skill: &str| {
            rows.iter()
                .find(|row| row["skillName"] == skill)
                .map(|row| row["installed"] == true)
        };
        assert_eq!(installed("ghostex-cli"), Some(true));
        assert_eq!(installed("ghostex-help"), Some(false));
    }

    #[test]
    fn rows_carry_the_catalog_name_for_clients_without_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let rows = skill_rows(temp.path(), "linux");
        let cli = rows
            .iter()
            .find(|row| row["skillName"] == "ghostex-cli")
            .expect("cli row");
        assert_eq!(cli["name"], "Ghostex CLI");
        assert!(cli["description"]
            .as_str()
            .is_some_and(|text| !text.is_empty()));
    }

    #[test]
    fn unknown_skills_are_refused() {
        let params = json!({ "skillName": "../etc" });
        let error = skill_param(params.as_object().expect("params")).expect_err("refused");
        assert_eq!(error.code, "badRequest");
        let params = json!({ "skillName": "ghostex-orchestrator" });
        assert!(skill_param(params.as_object().expect("params")).is_err());
    }

    #[tokio::test]
    async fn install_then_uninstall_round_trips_through_the_status() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = tempfile::tempdir().expect("source");
        write_skill(&source.path().join("ghostex-help"), "ghostex-help");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let home = paths.agent_config_home_dir().to_path_buf();
        install_skill_from(
            &paths,
            "ghostex-help",
            &source.path().to_string_lossy(),
            false,
        )
        .await
        .expect("install");
        let installed = |home: &Path| {
            skill_rows(home, "linux")
                .into_iter()
                .find(|row| row["skillName"] == "ghostex-help")
                .map(|row| row["installed"] == true)
        };
        assert_eq!(installed(&home), Some(true));
        let claude_link = home.join(".claude/skills/ghostex-help");
        assert!(fs::symlink_metadata(&claude_link).is_ok_and(|meta| meta.file_type().is_symlink()));
        // A real folder of the same name in another agent's folder is the user's.
        write_skill(&home.join(".gemini/skills/ghostex-help"), "ghostex-help");
        uninstall_skill(&home, "ghostex-help").expect("uninstall");
        assert_eq!(installed(&home), Some(false));
        assert!(fs::symlink_metadata(&claude_link).is_err());
        assert!(home.join(".gemini/skills/ghostex-help/SKILL.md").is_file());
        // Uninstalling what is not there succeeds.
        uninstall_skill(&home, "ghostex-help").expect("uninstall again");
    }

    #[tokio::test]
    async fn status_names_the_platform_and_lists_every_skill() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let answer = dispatch(&paths, json!({ "action": "status" }).as_object().unwrap())
            .await
            .expect("status");
        assert_eq!(answer["platform"], platform());
        assert_eq!(
            answer["skills"].as_array().map(Vec::len),
            Some(listed_skill_names().len())
        );
        assert!(answer["ghostexCli"]["installed"].is_boolean());
    }
}

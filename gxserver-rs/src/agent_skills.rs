use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    path::{Component, Path, PathBuf},
};

use serde_json::{json, Map, Value};
use tokio::process::Command;

use crate::{domain::DomainStateError, paths::GxserverPaths};

pub const GHOSTEX_AGENT_SKILL_NAMES: &[&str] = &[
    "ghostex-browser-use",
    "ghostex-computer-use",
    "ghostex-agent-orchestration",
    "ghostex-generate-title",
    "ghostex-manage-beads",
];

pub const GHOSTEX_SKILLS_CLI_AGENT_IDS: &[&str] = &[
    "claude-code",
    "codex",
    "cursor",
    "gemini-cli",
    "opencode",
    "pi",
    "antigravity",
    "antigravity-cli",
    "amp",
    "kiro-cli",
    "droid",
    "github-copilot",
    "qoder",
    "codebuddy",
    "rovodev",
    "hermes-agent",
];

const AGENT_SKILL_INSTALL_ENV_OVERRIDES: &[&str] = &[
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_PROFILE",
    "CODEX_HOME",
    "CODEX_PROFILE",
];
const AGENT_SKILL_DISCOVERY_MAX_DEPTH: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSkillDiscoveryRoot {
    pub path: PathBuf,
    pub providers: Vec<String>,
    pub source_kind: &'static str,
}

#[derive(Clone, Debug)]
struct AgentSkillLocation {
    directory_path: PathBuf,
    providers: Vec<String>,
    root_path: PathBuf,
    skill_file_path: PathBuf,
    skill_name: String,
    source_kind: &'static str,
}

/*
CDXC:AgentSkills 2026-06-19-13:59:
Rust owns the same Ghostex skill setup surface as TypeScript: local authenticated RPCs and CLI commands discover Claude/Codex/shared skill folders, delegate installs to the external `skills` CLI, and never persist stdout, stderr, command text, package paths, or environment values in gxserver logs.
*/
pub fn read_agent_skill_status(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let skill_names = normalize_agent_skill_names(read_string_array(params, "skillNames")?)?;
    read_agent_skill_status_for_names(paths, params, skill_names)
}

pub async fn install_agent_skills(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let package_source =
        normalize_package_source(params.get("packageSource").and_then(Value::as_str))?;
    let skill_names = normalize_agent_skill_names(read_string_array(params, "skillNames")?)?;
    let install_command = build_gxserver_agent_skill_install_command(
        &package_source,
        &skill_names,
        read_string_array(params, "agentIds")?,
    );
    let output = run_agent_skill_install_command(paths, &package_source, &install_command).await?;
    let mut status = read_agent_skill_status_for_names(paths, params, skill_names)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    status.insert("installCommand".to_string(), json!(install_command));
    status.insert("packageSource".to_string(), json!(package_source));
    status.insert(
        "stderr".to_string(),
        json!(String::from_utf8_lossy(&output.stderr).to_string()),
    );
    status.insert(
        "stdout".to_string(),
        json!(String::from_utf8_lossy(&output.stdout).to_string()),
    );
    Ok(Value::Object(status))
}

pub fn build_gxserver_agent_skill_install_command(
    package_source: &str,
    skill_names: &[String],
    agent_ids: Option<Vec<String>>,
) -> Vec<String> {
    let agent_ids = normalize_agent_skill_agent_ids(agent_ids);
    let mut command = vec![
        "npx".to_string(),
        "--yes".to_string(),
        "skills".to_string(),
        "add".to_string(),
        package_source.to_string(),
        "--skill".to_string(),
    ];
    command.extend(skill_names.iter().cloned());
    command.push("--global".to_string());
    command.push("--agent".to_string());
    command.extend(agent_ids);
    command.push("--copy".to_string());
    command.push("-y".to_string());
    command
}

pub fn create_gxserver_agent_skill_install_environment(
    paths: &GxserverPaths,
    environment: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut install_environment = environment.clone();
    install_environment.insert("HOME".to_string(), path_string(&paths.home_dir));
    for key in AGENT_SKILL_INSTALL_ENV_OVERRIDES {
        install_environment.remove(*key);
    }
    install_environment
}

pub fn create_gxserver_agent_skill_discovery_sources(
    paths: &GxserverPaths,
    repository_paths: &[String],
) -> Vec<AgentSkillDiscoveryRoot> {
    let mut sources = vec![
        AgentSkillDiscoveryRoot {
            path: paths.home_dir.join(".codex").join("skills"),
            providers: vec!["codex".to_string()],
            source_kind: "global",
        },
        AgentSkillDiscoveryRoot {
            path: paths.home_dir.join(".agents").join("skills"),
            providers: vec!["agent-skills".to_string()],
            source_kind: "global",
        },
        AgentSkillDiscoveryRoot {
            path: paths.home_dir.join(".claude").join("skills"),
            providers: vec!["claude".to_string()],
            source_kind: "global",
        },
        AgentSkillDiscoveryRoot {
            path: paths.home_dir.join(".codex").join("plugins").join("cache"),
            providers: vec!["codex".to_string(), "agent-skills".to_string()],
            source_kind: "pluginCache",
        },
    ];
    for repository_path in repository_paths {
        let normalized_path = normalize_existing_absolute_path(repository_path);
        if normalized_path.as_os_str().is_empty() {
            continue;
        }
        sources.push(AgentSkillDiscoveryRoot {
            path: normalized_path.join(".agents").join("skills"),
            providers: vec!["agent-skills".to_string()],
            source_kind: "repository",
        });
        sources.push(AgentSkillDiscoveryRoot {
            path: normalized_path.join(".claude").join("skills"),
            providers: vec!["claude".to_string()],
            source_kind: "repository",
        });
    }
    unique_skill_discovery_roots(sources)
}

pub fn normalize_agent_skill_names(
    skill_names: Option<Vec<String>>,
) -> Result<Vec<String>, DomainStateError> {
    let source = skill_names.unwrap_or_else(|| {
        GHOSTEX_AGENT_SKILL_NAMES
            .iter()
            .map(|skill_name| (*skill_name).to_string())
            .collect()
    });
    let mut normalized = Vec::new();
    let valid: HashSet<&str> = GHOSTEX_AGENT_SKILL_NAMES.iter().copied().collect();
    let mut invalid = Vec::new();
    for skill_name in source {
        let trimmed = skill_name.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !valid.contains(trimmed) {
            invalid.push(trimmed.to_string());
            continue;
        }
        if !normalized.iter().any(|existing| existing == trimmed) {
            normalized.push(trimmed.to_string());
        }
    }
    if invalid.is_empty() {
        Ok(normalized)
    } else {
        Err(DomainStateError::bad_request(format!(
            "Unknown Ghostex agent skill: {}",
            invalid.join(", ")
        )))
    }
}

pub fn normalize_agent_skill_agent_ids(agent_ids: Option<Vec<String>>) -> Vec<String> {
    let source = match agent_ids {
        Some(values) if !values.is_empty() => values,
        _ => GHOSTEX_SKILLS_CLI_AGENT_IDS
            .iter()
            .map(|agent_id| (*agent_id).to_string())
            .collect(),
    };
    let mut normalized = Vec::new();
    for agent_id in source {
        for part in agent_id.split(',') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !normalized.iter().any(|existing| existing == trimmed) {
                normalized.push(trimmed.to_string());
            }
        }
    }
    normalized
}

fn read_agent_skill_status_for_names(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
    skill_names: Vec<String>,
) -> Result<Value, DomainStateError> {
    let repository_paths = read_string_array(params, "repositoryPaths")?.unwrap_or_default();
    let roots = create_gxserver_agent_skill_discovery_sources(paths, &repository_paths);
    let mut locations_by_skill: BTreeMap<String, Vec<Value>> = skill_names
        .iter()
        .map(|skill_name| (skill_name.clone(), Vec::new()))
        .collect();
    for root in &roots {
        for location in discover_skill_locations(root, &skill_names)? {
            if let Some(locations) = locations_by_skill.get_mut(&location.skill_name) {
                locations.push(location_to_value(&location));
            }
        }
    }
    let skills = skill_names
        .iter()
        .map(|skill_name| {
            let locations = locations_by_skill.remove(skill_name).unwrap_or_default();
            json!({
                "installed": !locations.is_empty(),
                "locations": locations,
                "skillName": skill_name,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "generatedAt": now_iso(),
        "homeDir": path_string(&paths.home_dir),
        "roots": roots.iter().map(root_to_value).collect::<Vec<_>>(),
        "skills": skills,
        "type": "agentSkillStatus",
    }))
}

async fn run_agent_skill_install_command(
    paths: &GxserverPaths,
    package_source: &str,
    install_command: &[String],
) -> Result<std::process::Output, DomainStateError> {
    let Some((executable, args)) = install_command.split_first() else {
        return Err(DomainStateError::bad_request(
            "Agent skill install command is empty.",
        ));
    };
    let mut command = Command::new(executable);
    command.args(args);
    if !is_uri_like(package_source) {
        if let Some(parent) = Path::new(package_source).parent() {
            command.current_dir(parent);
        }
    }
    let current_environment: BTreeMap<String, String> = env::vars().collect();
    let install_environment =
        create_gxserver_agent_skill_install_environment(paths, &current_environment);
    command.env_clear();
    command.envs(install_environment);
    let output = command.output().await.map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Agent skill install command could not start: {error}"),
    })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(DomainStateError {
            code: "internalError",
            message: format!(
                "Agent skill install command failed with exit code {}.",
                output
                    .status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
        })
    }
}

fn discover_skill_locations(
    root: &AgentSkillDiscoveryRoot,
    skill_names: &[String],
) -> Result<Vec<AgentSkillLocation>, DomainStateError> {
    if !is_existing_directory(&root.path) {
        return Ok(Vec::new());
    }
    let wanted: HashSet<&str> = skill_names.iter().map(String::as_str).collect();
    let mut matches = Vec::new();
    let mut seen_real_paths = HashSet::new();
    walk_skill_root(
        root,
        &root.path,
        AGENT_SKILL_DISCOVERY_MAX_DEPTH,
        &mut seen_real_paths,
        &mut |directory_path, skill_file_path| {
            let candidates = read_skill_name_candidates(directory_path, skill_file_path);
            let Some(skill_name) = candidates
                .iter()
                .find(|candidate| wanted.contains(candidate.as_str()))
                .cloned()
            else {
                return Ok(());
            };
            matches.push(AgentSkillLocation {
                directory_path: directory_path.to_path_buf(),
                providers: root.providers.clone(),
                root_path: root.path.clone(),
                skill_file_path: skill_file_path.to_path_buf(),
                skill_name,
                source_kind: root.source_kind,
            });
            Ok(())
        },
    )?;
    Ok(matches)
}

fn walk_skill_root<F>(
    root: &AgentSkillDiscoveryRoot,
    directory_path: &Path,
    remaining_depth: usize,
    seen_real_paths: &mut HashSet<PathBuf>,
    visit_skill: &mut F,
) -> Result<(), DomainStateError>
where
    F: FnMut(&Path, &Path) -> Result<(), DomainStateError>,
{
    let resolved_path = match fs::canonicalize(directory_path) {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    if !seen_real_paths.insert(resolved_path) {
        return Ok(());
    }
    let skill_file_path = directory_path.join("SKILL.md");
    if is_existing_file(&skill_file_path) {
        visit_skill(directory_path, &skill_file_path)?;
        return Ok(());
    }
    if remaining_depth == 0 {
        return Ok(());
    }
    let mut entries = match fs::read_dir(directory_path) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(_) => return Ok(()),
    };
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        if name == "node_modules" || name == ".git" {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() && !file_type.is_symlink() {
            continue;
        }
        if file_type.is_symlink() && root.source_kind == "repository" {
            continue;
        }
        walk_skill_root(
            root,
            &entry.path(),
            remaining_depth - 1,
            seen_real_paths,
            visit_skill,
        )?;
    }
    Ok(())
}

fn read_skill_name_candidates(directory_path: &Path, skill_file_path: &Path) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(name) = directory_path.file_name().and_then(|name| name.to_str()) {
        push_unique_candidate(&mut candidates, name.trim());
    }
    let markdown = fs::read_to_string(skill_file_path).unwrap_or_default();
    if let Some(name) = read_frontmatter_name(&markdown) {
        push_unique_candidate(&mut candidates, name.trim());
    }
    if let Some(name) = read_first_markdown_heading(&markdown) {
        push_unique_candidate(&mut candidates, name.trim());
    }
    candidates
}

fn read_frontmatter_name(markdown: &str) -> Option<String> {
    let mut lines = markdown.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        if line.trim() == "---" {
            return None;
        }
        let trimmed = line.trim();
        let Some(value) = trimmed.strip_prefix("name:") else {
            continue;
        };
        let value = value.trim();
        let name = if let Some(rest) = value.strip_prefix('"') {
            rest.split('"').next().unwrap_or_default()
        } else if let Some(rest) = value.strip_prefix('\'') {
            rest.split('\'').next().unwrap_or_default()
        } else {
            value
        };
        if !name.trim().is_empty() {
            return Some(name.trim().to_string());
        }
    }
    None
}

fn read_first_markdown_heading(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let rest = line.strip_prefix('#')?;
        let first = rest.chars().next()?;
        if !first.is_whitespace() {
            return None;
        }
        let heading = rest.trim();
        if heading.is_empty() {
            None
        } else {
            Some(heading.to_string())
        }
    })
}

fn push_unique_candidate(candidates: &mut Vec<String>, candidate: &str) {
    if candidate.is_empty() {
        return;
    }
    if !candidates.iter().any(|existing| existing == candidate) {
        candidates.push(candidate.to_string());
    }
}

fn normalize_package_source(package_source: Option<&str>) -> Result<String, DomainStateError> {
    let normalized = package_source.unwrap_or_default().trim();
    if normalized.is_empty() {
        return Err(DomainStateError::bad_request(
            "Agent skill installs require --source <skills-package-path>.",
        ));
    }
    if is_uri_like(normalized) {
        return Ok(normalized.to_string());
    }
    Ok(path_string(&normalize_absolute_path(PathBuf::from(
        normalized,
    ))))
}

fn normalize_existing_absolute_path(candidate: &str) -> PathBuf {
    let normalized = candidate.trim();
    if normalized.is_empty() {
        return PathBuf::new();
    }
    normalize_absolute_path(PathBuf::from(normalized))
}

fn normalize_absolute_path(path: PathBuf) -> PathBuf {
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    normalize_path_lexically(absolute)
}

fn normalize_path_lexically(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn unique_skill_discovery_roots(
    sources: Vec<AgentSkillDiscoveryRoot>,
) -> Vec<AgentSkillDiscoveryRoot> {
    let mut seen = HashSet::new();
    let mut roots = Vec::new();
    for mut source in sources {
        source.path = normalize_absolute_path(expand_home_prefix(source.path));
        let key = format!(
            "{}:{}:{}",
            source.source_kind,
            path_string(&source.path),
            source.providers.join(",")
        );
        if seen.insert(key) {
            roots.push(source);
        }
    }
    roots
}

fn expand_home_prefix(path: PathBuf) -> PathBuf {
    let path_text = path_string(&path);
    if path_text == "~" {
        return env::var_os("HOME").map(PathBuf::from).unwrap_or(path);
    }
    if let Some(rest) = path_text.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path
}

fn read_string_array(
    params: &Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>, DomainStateError> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    let Some(values) = value.as_array() else {
        return Err(DomainStateError::bad_request(format!(
            "{key} must be an array of strings."
        )));
    };
    let mut strings = Vec::new();
    for value in values {
        let Some(text) = value.as_str() else {
            return Err(DomainStateError::bad_request(format!(
                "{key} must be an array of strings."
            )));
        };
        strings.push(text.to_string());
    }
    Ok(Some(strings))
}

fn root_to_value(root: &AgentSkillDiscoveryRoot) -> Value {
    json!({
        "path": path_string(&root.path),
        "providers": root.providers.clone(),
        "sourceKind": root.source_kind,
    })
}

fn location_to_value(location: &AgentSkillLocation) -> Value {
    json!({
        "directoryPath": path_string(&location.directory_path),
        "providers": location.providers.clone(),
        "rootPath": path_string(&location.root_path),
        "skillFilePath": path_string(&location.skill_file_path),
        "sourceKind": location.source_kind,
    })
}

fn is_existing_directory(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

fn is_existing_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn is_uri_like(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    for character in chars {
        if character == ':' {
            return true;
        }
        if !character.is_ascii_alphanumeric()
            && character != '+'
            && character != '.'
            && character != '-'
        {
            return false;
        }
    }
    false
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;
    use std::io;

    #[test]
    fn install_command_delegates_to_skills_cli_for_default_agents() {
        let command = build_gxserver_agent_skill_install_command(
            "/Applications/Ghostex.app/Contents/Resources/CLI/skills",
            &["ghostex-browser-use".to_string()],
            None,
        );
        assert_eq!(
            &command[0..8],
            [
                "npx",
                "--yes",
                "skills",
                "add",
                "/Applications/Ghostex.app/Contents/Resources/CLI/skills",
                "--skill",
                "ghostex-browser-use",
                "--global",
            ]
        );
        assert!(command.iter().any(|item| item == "--copy"));
        assert!(command.iter().any(|item| item == "-y"));
        assert!(command.iter().any(|item| item == "claude-code"));
        assert!(command.iter().any(|item| item == "codex"));
        assert!(command.iter().any(|item| item == "cursor"));
        assert!(command.iter().any(|item| item == "gemini-cli"));
        assert!(command.iter().any(|item| item == "opencode"));
        assert!(command.iter().any(|item| item == "pi"));
    }

    #[test]
    fn install_environment_removes_profile_overrides() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let environment = BTreeMap::from([
            ("CLAUDE_CONFIG_DIR".to_string(), "/tmp/profile".to_string()),
            ("CLAUDE_PROFILE".to_string(), "personal".to_string()),
            ("CODEX_HOME".to_string(), "/tmp/codex-profile".to_string()),
            ("CODEX_PROFILE".to_string(), "personal".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ]);
        let install_environment =
            create_gxserver_agent_skill_install_environment(&paths, &environment);
        assert_eq!(
            install_environment.get("HOME"),
            Some(&path_string(temp.path()))
        );
        assert_eq!(
            install_environment.get("PATH"),
            Some(&"/usr/bin".to_string())
        );
        assert_eq!(install_environment.get("CLAUDE_CONFIG_DIR"), None);
        assert_eq!(install_environment.get("CLAUDE_PROFILE"), None);
        assert_eq!(install_environment.get("CODEX_HOME"), None);
        assert_eq!(install_environment.get("CODEX_PROFILE"), None);
    }

    #[test]
    fn agent_skill_status_checks_global_repository_and_plugin_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = tempfile::tempdir().expect("repo");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        write_skill(
            &temp
                .path()
                .join(".claude")
                .join("skills")
                .join("ghostex-browser-use"),
            "ghostex-browser-use",
        );
        write_skill(
            &temp
                .path()
                .join(".agents")
                .join("skills")
                .join("ghostex-computer-use"),
            "ghostex-computer-use",
        );
        write_skill(
            &temp
                .path()
                .join(".codex")
                .join("plugins")
                .join("cache")
                .join("plugin")
                .join("skills")
                .join("ghostex-generate-title"),
            "ghostex-generate-title",
        );
        write_skill(
            &repo
                .path()
                .join(".agents")
                .join("skills")
                .join("ghostex-agent-orchestration"),
            "ghostex-agent-orchestration",
        );
        let status = read_agent_skill_status(
            &paths,
            json!({ "repositoryPaths": [path_string(repo.path())] })
                .as_object()
                .expect("params"),
        )
        .expect("status");
        let skills = status
            .get("skills")
            .and_then(Value::as_array)
            .expect("skills");
        assert_eq!(skill_installed(skills, "ghostex-browser-use"), Some(true));
        assert_eq!(skill_installed(skills, "ghostex-computer-use"), Some(true));
        assert_eq!(
            skill_installed(skills, "ghostex-agent-orchestration"),
            Some(true)
        );
        assert_eq!(
            skill_installed(skills, "ghostex-generate-title"),
            Some(true)
        );
        assert_eq!(skill_installed(skills, "ghostex-manage-beads"), Some(false));
        let roots = status
            .get("roots")
            .and_then(Value::as_array)
            .expect("roots");
        assert!(roots.iter().any(|root| {
            root.get("path")
                == Some(&json!(path_string(
                    &temp.path().join(".codex").join("skills")
                )))
        }));
        assert!(roots.iter().any(|root| {
            root.get("path")
                == Some(&json!(path_string(
                    &repo.path().join(".agents").join("skills")
                )))
        }));
    }

    #[test]
    fn skill_name_candidates_match_basename_frontmatter_or_heading() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("different-name");
        fs::create_dir_all(&skill_dir).expect("mkdir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: ghostex-browser-use\n---\n# ghostex-computer-use\n",
        )
        .expect("write skill");
        let candidates = read_skill_name_candidates(&skill_dir, &skill_dir.join("SKILL.md"));
        assert!(candidates.iter().any(|name| name == "different-name"));
        assert!(candidates.iter().any(|name| name == "ghostex-browser-use"));
        assert!(candidates.iter().any(|name| name == "ghostex-computer-use"));
    }

    #[test]
    fn repository_roots_skip_symlinked_skill_directories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = tempfile::tempdir().expect("repo");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let target = temp.path().join("target-skill");
        write_skill(&target, "ghostex-browser-use");
        let link = repo
            .path()
            .join(".agents")
            .join("skills")
            .join("ghostex-browser-use");
        fs::create_dir_all(link.parent().expect("parent")).expect("mkdir");
        symlink_dir(&target, &link).expect("symlink");
        let status = read_agent_skill_status(
            &paths,
            json!({ "repositoryPaths": [path_string(repo.path())] })
                .as_object()
                .expect("params"),
        )
        .expect("status");
        let skills = status
            .get("skills")
            .and_then(Value::as_array)
            .expect("skills");
        assert_eq!(skill_installed(skills, "ghostex-browser-use"), Some(false));
    }

    fn write_skill(directory_path: &Path, skill_name: &str) {
        fs::create_dir_all(directory_path).expect("mkdir");
        fs::write(
            directory_path.join("SKILL.md"),
            format!("# {skill_name}\n\nUse this skill for tests.\n"),
        )
        .expect("write skill");
    }

    fn skill_installed(skills: &[Value], skill_name: &str) -> Option<bool> {
        skills
            .iter()
            .find(|skill| skill.get("skillName").and_then(Value::as_str) == Some(skill_name))
            .and_then(|skill| skill.get("installed"))
            .and_then(Value::as_bool)
    }

    #[cfg(unix)]
    fn symlink_dir(source: &Path, link: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(source, link)
    }

    #[cfg(windows)]
    fn symlink_dir(source: &Path, link: &Path) -> io::Result<()> {
        std::os::windows::fs::symlink_dir(source, link)
    }
}

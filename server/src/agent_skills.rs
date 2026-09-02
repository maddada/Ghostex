use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    path::{Component, Path, PathBuf},
};

use serde_json::{json, Map, Value};

use crate::{domain::DomainStateError, paths::GxserverPaths};

pub const GHOSTEX_AGENT_SKILL_NAMES: &[&str] = &[
    /*
    CDXC:CodexSessionMove 2026-06-26-13:47:
    Ghostex ships `$ghostex-move-codex-session` as an app-bundled skill, so gxserver must accept it in the same install/status allow-list as the existing first-launch skills.
    */
    "ghostex-browser-use",
    "ghostex-embedded-browser-use",
    "ghostex-computer-use",
    "ghostex-cli",
    "ghostex-manage-beads",
    "ghostex-fable-56-orchestration",
    "ghostex-auto-rename-session",
    "ghostex-move-codex-session",
    /*
    CDXC:BoardAssociateSession 2026-08-24:
    The Project Board beads skill teaches agents `ghostex board associate`, the
    only way a hand-started session gets onto its card, so it must be installable
    through the same allow-list as the other bundled skills.
    */
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

const AGENT_SKILL_DISCOVERY_MAX_DEPTH: usize = 8;
/*
CDXC:NativeAgentSkillInstall 2026-09-02:
Bundled skill installs used to shell out to `npx --yes skills add … --global
--copy`, which made first-launch setup depend on a Node runtime being on the
app's PATH (nvm/fnm installs are not) and on the npm registry being reachable,
and the desktop side killed the whole install after 120 seconds. Users saw this
as "Ghostex CLI install failed" because `$ghostex-cli` is the first skill the
first-launch flow installs. gxserver now copies the bundled skill folders itself
into exactly the global skill directories that `skills add --global --copy`
would have written, so an install needs nothing but the app bundle on disk.
Directory names below mirror the `skills` CLI agent table (universal agents
resolve to the canonical `~/.agents/skills`).
*/
const CANONICAL_AGENT_SKILLS_DIR: &str = ".agents/skills";
const AGENT_SKILL_COPY_EXCLUDED_NAMES: &[&str] = &[".git", "node_modules", ".DS_Store"];

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
    if skill_names.is_empty() {
        return Err(DomainStateError::bad_request(
            "Agent skill installs require at least one Ghostex agent skill.",
        ));
    }
    if is_uri_like(&package_source) {
        return Err(DomainStateError::bad_request(
            "Agent skill installs copy from a local skills directory; remote package sources are not supported.",
        ));
    }
    let agent_ids = normalize_agent_skill_agent_ids(read_string_array(params, "agentIds")?);
    let target_roots = agent_skill_install_target_roots(&paths.home_dir, &agent_ids)?;
    let mut installed_paths = Vec::new();
    let mut summary_lines = Vec::new();
    for skill_name in &skill_names {
        let source_dir = resolve_bundled_skill_source_dir(&package_source, skill_name)?;
        let mut installed_count = 0_usize;
        for target_root in &target_roots {
            let destination = target_root.join(skill_name);
            if copy_skill_directory_into_place(&source_dir, &destination)? {
                installed_paths.push(path_string(&destination));
                installed_count += 1;
            }
        }
        summary_lines.push(format!(
            "Installed {skill_name} into {installed_count} global skill folder(s)."
        ));
    }
    let install_command =
        describe_agent_skill_install_command(&package_source, &skill_names, &agent_ids);
    let mut status = read_agent_skill_status_for_names(paths, params, skill_names)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    status.insert("installCommand".to_string(), json!(install_command));
    status.insert("installedPaths".to_string(), json!(installed_paths));
    status.insert("packageSource".to_string(), json!(package_source));
    status.insert("stderr".to_string(), json!(""));
    status.insert(
        "stdout".to_string(),
        json!(format!("{}\n", summary_lines.join("\n"))),
    );
    Ok(Value::Object(status))
}

/// The equivalent CLI invocation, kept in the install result so clients that
/// display what ran keep working after the move away from `npx skills add`.
fn describe_agent_skill_install_command(
    package_source: &str,
    skill_names: &[String],
    agent_ids: &[String],
) -> Vec<String> {
    let mut command = vec![
        "gxserver".to_string(),
        "agent-skills".to_string(),
        "install".to_string(),
    ];
    command.extend(skill_names.iter().cloned());
    command.push("--source".to_string());
    command.push(package_source.to_string());
    command.push("--agent".to_string());
    command.extend(agent_ids.iter().cloned());
    command
}

/// Global skill directory for one `skills`-CLI agent id, relative to the
/// selected HOME. Universal agents share the canonical `~/.agents/skills`.
/// Profile overrides such as CLAUDE_CONFIG_DIR and CODEX_HOME are deliberately
/// ignored, matching the environment the old `npx` runner stripped before
/// spawning, so installs always land in the default global locations that the
/// status reader inspects.
pub fn agent_skill_global_dir(home_dir: &Path, agent_id: &str) -> Option<PathBuf> {
    let relative = match agent_id {
        "universal" | "agent-skills" | "amp" | "cursor" | "opencode" => {
            return Some(home_dir.join(CANONICAL_AGENT_SKILLS_DIR));
        }
        "claude-code" => ".claude/skills",
        "codex" => ".codex/skills",
        "gemini-cli" => ".gemini/skills",
        "pi" => ".pi/agent/skills",
        "antigravity" => ".gemini/antigravity/skills",
        "antigravity-cli" => ".gemini/antigravity-cli/skills",
        "kiro-cli" => ".kiro/skills",
        "droid" => ".factory/skills",
        "github-copilot" => ".copilot/skills",
        "qoder" => ".qoder/skills",
        "codebuddy" => ".codebuddy/skills",
        "rovodev" => ".rovodev/skills",
        "hermes-agent" => {
            return Some(
                env::var_os("HERMES_HOME")
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .unwrap_or_else(|| home_dir.join(".hermes"))
                    .join("skills"),
            );
        }
        _ => return None,
    };
    Some(home_dir.join(relative))
}

/// Every global skill root an install writes to: the canonical universal
/// directory first (it is what Ghostex's own status probe and Codex read), then
/// one root per requested agent, deduplicated in request order.
fn agent_skill_install_target_roots(
    home_dir: &Path,
    agent_ids: &[String],
) -> Result<Vec<PathBuf>, DomainStateError> {
    let mut roots = vec![home_dir.join(CANONICAL_AGENT_SKILLS_DIR)];
    let mut unknown = Vec::new();
    for agent_id in agent_ids {
        match agent_skill_global_dir(home_dir, agent_id) {
            Some(root) => {
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
            None => unknown.push(agent_id.clone()),
        }
    }
    if unknown.is_empty() {
        Ok(roots)
    } else {
        Err(DomainStateError::bad_request(format!(
            "Unknown agent skill target: {}. Supported agents: {}.",
            unknown.join(", "),
            GHOSTEX_SKILLS_CLI_AGENT_IDS.join(", ")
        )))
    }
}

/// The bundled skill folder for `skill_name` inside the package source, which
/// is either the skills root itself (`<source>/<skill>`) or a package root that
/// carries a `skills/` folder (`<source>/skills/<skill>`).
fn resolve_bundled_skill_source_dir(
    package_source: &str,
    skill_name: &str,
) -> Result<PathBuf, DomainStateError> {
    let package_root = Path::new(package_source);
    for candidate in [
        package_root.join(skill_name),
        package_root.join("skills").join(skill_name),
    ] {
        if is_existing_file(&candidate.join("SKILL.md")) {
            return Ok(candidate);
        }
    }
    Err(DomainStateError::bad_request(format!(
        "Bundled skill {skill_name} was not found under {package_source}. Reinstall Ghostex so its skills folder is present."
    )))
}

/// Replace `destination` with a fresh copy of `source_dir`. The copy is staged
/// next to the destination and renamed into place so a failure half-way never
/// leaves a truncated skill behind, and an existing symlink (a user's own
/// `skills add` symlink-mode install) is unlinked rather than followed so the
/// directory it points at is never deleted. Returns false when the source and
/// destination are the same folder, which must not be cleaned.
fn copy_skill_directory_into_place(
    source_dir: &Path,
    destination: &Path,
) -> Result<bool, DomainStateError> {
    let Some(parent) = destination.parent() else {
        return Err(DomainStateError {
            code: "internalError",
            message: format!(
                "Skill destination {} has no parent folder.",
                destination.display()
            ),
        });
    };
    let Some(destination_name) = destination.file_name().and_then(|name| name.to_str()) else {
        return Err(DomainStateError {
            code: "internalError",
            message: format!(
                "Skill destination {} has no folder name.",
                destination.display()
            ),
        });
    };
    if let (Ok(source_real), Ok(destination_real)) =
        (fs::canonicalize(source_dir), fs::canonicalize(destination))
    {
        if source_real == destination_real || source_real.starts_with(&destination_real) {
            return Ok(false);
        }
    }
    fs::create_dir_all(parent).map_err(|error| skill_install_io_error("create", parent, error))?;
    let staging = parent.join(format!(
        ".{destination_name}.ghostex-install-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let _ = remove_path_of_any_kind(&staging);
    if let Err(error) = copy_skill_tree(source_dir, &staging) {
        let _ = remove_path_of_any_kind(&staging);
        return Err(error);
    }
    if let Err(error) = remove_path_of_any_kind(destination) {
        let _ = remove_path_of_any_kind(&staging);
        return Err(skill_install_io_error("replace", destination, error));
    }
    if let Err(error) = fs::rename(&staging, destination) {
        let _ = remove_path_of_any_kind(&staging);
        return Err(skill_install_io_error("activate", destination, error));
    }
    Ok(true)
}

/// Recursive copy that dereferences symlinks (so a bundled skill that links to
/// a shared file ships the file), skips broken links instead of aborting, keeps
/// file permission bits, and leaves out VCS and dependency folders.
fn copy_skill_tree(source_dir: &Path, destination: &Path) -> Result<(), DomainStateError> {
    fs::create_dir_all(destination)
        .map_err(|error| skill_install_io_error("create", destination, error))?;
    let mut entries = fs::read_dir(source_dir)
        .map_err(|error| skill_install_io_error("read", source_dir, error))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        if AGENT_SKILL_COPY_EXCLUDED_NAMES
            .iter()
            .any(|excluded| name == *excluded)
        {
            continue;
        }
        let source_path = entry.path();
        let destination_path = destination.join(&name);
        // Follows symlinks; a broken link fails here and is skipped on purpose.
        let Ok(metadata) = fs::metadata(&source_path) else {
            continue;
        };
        if metadata.is_dir() {
            copy_skill_tree(&source_path, &destination_path)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        fs::copy(&source_path, &destination_path)
            .map_err(|error| skill_install_io_error("copy", &destination_path, error))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode() & 0o777;
            let _ = fs::set_permissions(&destination_path, fs::Permissions::from_mode(mode));
        }
    }
    Ok(())
}

fn remove_path_of_any_kind(path: &Path) -> std::io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn skill_install_io_error(action: &str, path: &Path, error: std::io::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!(
            "Agent skill install could not {action} {}: {error}",
            path.display()
        ),
    }
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
    Ok(path_string(&normalize_absolute_path(expand_home_prefix(
        PathBuf::from(normalized),
    ))))
}

fn normalize_existing_absolute_path(candidate: &str) -> PathBuf {
    let normalized = candidate.trim();
    if normalized.is_empty() {
        return PathBuf::new();
    }
    normalize_absolute_path(expand_home_prefix(PathBuf::from(normalized)))
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

/*
CDXC:SkillConsolidation 2026-08-24:
`ghostex-manage-automations`, `ghostex-agent-orchestration`, and
`ghostex-find-prev-session` were folded into the CLI help (`$ghostex-cli` is
the entry-point skill), so newer Ghostex must clean up copies that older
builds installed on user machines. The startup migration removes a retired
skill directory only when every file in it byte-matches a version Ghostex
actually shipped; a directory with extra files or edited content is user work
and is left alone.
*/
pub const RETIRED_GHOSTEX_AGENT_SKILL_NAMES: &[&str] = &[
    "ghostex-manage-automations",
    "ghostex-agent-orchestration",
    "ghostex-find-prev-session",
];

/// sha256 of every SKILL.md / agents/openai.yaml revision the retired skills
/// ever shipped with (collected from the repository history at removal time).
const RETIRED_GHOSTEX_AGENT_SKILL_SHIPPED_SHA256: &[&str] = &[
    // ghostex-manage-automations SKILL.md
    "0cb5fc530b28afa0ad5f43cfa47eed2b5bd92a0cb73165f021d909503299b78b",
    "e14f5eccba6cffbcf3d44313f12a23502ba7a409c3533823f34646b10d290f64",
    // ghostex-manage-automations agents/openai.yaml
    "31af0a385211af151897b2ddd75fc2773831b79ed02cf4c792ce759e566a0501",
    "4da015e2ea7b953e5dab5bed60bae1818bed8ce5d441c88b5d41f6515f2b3f63",
    // ghostex-agent-orchestration SKILL.md
    "044bcecf7c4d1ee120771efd8fc6309844ebf02c55f7fce314b84b141ebeb390",
    "8e2018b07cf1fe7c809ed7715312402e107389a6929a2526b7b7eb6932fd36d1",
    "da1dcd51ca2e6d958d326aa60f81db63077510d6d5a6e2293395024f730fb59a",
    // ghostex-agent-orchestration agents/openai.yaml
    "0a36e596d15d13b9ad06c8310c223a35def185d903efdae4681c6a7c5fb26daa",
    "a815a68a82e73e6b93b31248e1c641bed587a6bbf5bbd46e56fcc144641f49a0",
    // ghostex-find-prev-session SKILL.md
    "5ba3a354363d4fc86bf23a9f9807115d167697d159c11d4752260db45dfa9caf",
    "e2c8fae822dcfb1159c28d5f02015b26b777aa48469fcf1ff0f545b6d21a4b90",
    "e68fadf8f96ceabb339e2af36972cf92ecedb07d8b6c5cd9a4c0e4231349e765",
    // ghostex-find-prev-session agents/openai.yaml
    "20901c1230ebec7200694cdcdb2c2cdad7a0b500adc10be7123f318b23f3b73a",
    "fe50e56c5216d964637c993c27e7c539ee0bfdfb7f22b4cf5bcff0ad783eea61",
];

fn file_matches_shipped_retired_skill_content(path: &Path) -> bool {
    use sha2::{Digest, Sha256};
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let digest = format!("{:x}", Sha256::digest(&bytes));
    RETIRED_GHOSTEX_AGENT_SKILL_SHIPPED_SHA256.contains(&digest.as_str())
}

/// True when the directory holds exactly the files Ghostex shipped for a
/// retired skill (SKILL.md plus an optional agents/openai.yaml), all
/// byte-identical to a shipped revision.
fn retired_skill_dir_is_unmodified_shipped_copy(skill_dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(skill_dir) else {
        return false;
    };
    let mut saw_skill_md = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name == "SKILL.md" {
            if !file_matches_shipped_retired_skill_content(&entry.path()) {
                return false;
            }
            saw_skill_md = true;
        } else if name == "agents" {
            let Ok(agent_entries) = fs::read_dir(entry.path()) else {
                return false;
            };
            for agent_entry in agent_entries.flatten() {
                if agent_entry.file_name() != "openai.yaml"
                    || !file_matches_shipped_retired_skill_content(&agent_entry.path())
                {
                    return false;
                }
            }
        } else if name == ".DS_Store" {
            continue;
        } else {
            return false;
        }
    }
    saw_skill_md
}

/// Startup migration: delete unmodified shipped copies of the retired skills
/// from the global agent skill roots. Returns the removed directory paths.
pub fn remove_retired_ghostex_agent_skills(paths: &GxserverPaths) -> Vec<String> {
    let roots = [
        paths.home_dir.join(".agents").join("skills"),
        paths.home_dir.join(".codex").join("skills"),
        paths.home_dir.join(".claude").join("skills"),
    ];
    let mut removed = Vec::new();
    let mut seen_canonical: HashSet<PathBuf> = HashSet::new();
    for root in roots {
        let canonical_root = fs::canonicalize(&root).unwrap_or(root);
        if !seen_canonical.insert(canonical_root.clone()) {
            continue;
        }
        for skill_name in RETIRED_GHOSTEX_AGENT_SKILL_NAMES {
            let skill_dir = canonical_root.join(skill_name);
            let Ok(metadata) = fs::symlink_metadata(&skill_dir) else {
                continue;
            };
            let remove_result = if metadata.file_type().is_symlink() {
                // A symlinked skill points at user-managed content; remove
                // only the link itself.
                fs::remove_file(&skill_dir)
            } else if metadata.is_dir() && retired_skill_dir_is_unmodified_shipped_copy(&skill_dir)
            {
                fs::remove_dir_all(&skill_dir)
            } else {
                continue;
            };
            if remove_result.is_ok() {
                removed.push(path_string(&skill_dir));
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;
    use std::io;

    #[tokio::test]
    async fn install_agent_skills_rejects_explicit_empty_skill_names() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        for skill_names in [json!([]), json!([" ", "\n"])] {
            let params = json!({
                "packageSource": path_string(temp.path()),
                "skillNames": skill_names,
            });
            let error = install_agent_skills(&paths, params.as_object().expect("params"))
                .await
                .expect_err("empty skills should fail before command execution");
            assert_eq!(error.code, "badRequest");
            assert!(error.message.contains("at least one Ghostex agent skill"));
        }
    }

    #[test]
    fn package_and_repository_paths_expand_home_before_absolutizing() {
        let Some(home) = env::var_os("HOME") else {
            return;
        };
        let home = PathBuf::from(home);
        assert_eq!(
            normalize_package_source(Some("~/skills")).expect("package source"),
            path_string(&home.join("skills"))
        );
        assert_eq!(
            normalize_existing_absolute_path("~/repo"),
            home.join("repo")
        );
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
                .join(".agents")
                .join("skills")
                .join("ghostex-embedded-browser-use"),
            "ghostex-embedded-browser-use",
        );
        write_skill(
            &temp
                .path()
                .join(".codex")
                .join("plugins")
                .join("cache")
                .join("plugin")
                .join("skills")
                .join("ghostex-auto-rename-session"),
            "ghostex-auto-rename-session",
        );
        write_skill(
            &repo
                .path()
                .join(".agents")
                .join("skills")
                .join("ghostex-manage-beads"),
            "ghostex-manage-beads",
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
        assert_eq!(
            skill_installed(skills, "ghostex-embedded-browser-use"),
            Some(true)
        );
        assert_eq!(skill_installed(skills, "ghostex-computer-use"), Some(true));
        assert_eq!(skill_installed(skills, "ghostex-manage-beads"), Some(true));
        assert_eq!(
            skill_installed(skills, "ghostex-auto-rename-session"),
            Some(true)
        );
        assert_eq!(skill_installed(skills, "ghostex-cli"), Some(false));
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

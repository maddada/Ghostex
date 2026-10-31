use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use crate::agent_skills::GHOSTEX_AGENT_SKILL_NAMES;
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, AppState, RoutedResponse,
};
use crate::session_chat_agent_tasks::claude_config_directory;
use crate::session_chat_follower::session_chat_display_agent_for_session;
use crate::session_chat_skill_invocation::skill_invocation;
use crate::session_chat_skill_variants::collapse_session_chat_skill_variants;
use crate::storage::open_gxserver_database;
use axum::http::StatusCode;

const SESSION_CHAT_SKILL_DISCOVERY_MAX_DEPTH: usize = 8;
const GROK_SKILL_INSPECT_TIMEOUT: Duration = Duration::from_secs(10);
const GROK_SKILL_INSPECT_POLL_INTERVAL: Duration = Duration::from_millis(10);
const GROK_SKILL_INSPECT_READER_GRACE: Duration = Duration::from_secs(1);
const GROK_SKILL_INSPECT_OUTPUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy)]
struct SkillRootSpec {
    source_kind: &'static str,
}

#[derive(Debug)]
pub(crate) struct SessionChatSkill {
    pub(crate) directory_path: PathBuf,
    pub(crate) skill_file_path: PathBuf,
    pub(crate) name: String,
    pub(crate) source_kind: &'static str,
    /// The scanned skills root this copy was found under.
    pub(crate) root_path: PathBuf,
    /// Set only when the same name has several distinct SKILL.md contents.
    pub(crate) variant_label: Option<String>,
}

/*
CDXC:AgentSkills 2026-08-15:
Chat skill mentions are discovered by gxserver on the session's machine. Grok
Build owns a resolved catalog that includes bundled skills and collision-safe
invocation names, so its own `inspect` output is authoritative. Other agents
use the roots their own CLI loads (`provider_skill_roots`,
`project_skill_roots`). A project contributes its own roots without letting a
client submit arbitrary scan paths.
*/
pub fn read_session_chat_skills(
    paths: &GxserverPaths,
    agent_id: &str,
    project_path: Option<&Path>,
    claude_config_dir: Option<&Path>,
) -> Value {
    let agent_id = normalize_agent_id(agent_id);
    if agent_id == "grok" {
        return read_grok_session_chat_skills(paths, project_path);
    }
    let mut roots = Vec::<(PathBuf, SkillRootSpec)>::new();

    if let Some(project_path) = project_path.filter(|path| path.is_absolute()) {
        let (relatives, walk) = project_skill_roots(&agent_id);
        for folder in project_skill_folders(project_path, walk, &paths.home_dir) {
            for relative in relatives {
                roots.push((
                    join_relative(&folder, relative),
                    SkillRootSpec {
                        source_kind: "repository",
                    },
                ));
            }
        }
    }

    for relative in provider_skill_roots(&agent_id) {
        roots.push((
            join_relative(&paths.home_dir, relative),
            SkillRootSpec {
                source_kind: "global",
            },
        ));
    }
    if agent_id == "codex" {
        roots.push((
            paths.home_dir.join(".codex").join("plugins").join("cache"),
            SkillRootSpec {
                source_kind: "pluginCache",
            },
        ));
    }

    let mut seen_skill_paths = HashSet::new();
    let mut skills = Vec::new();
    for (root, spec) in roots {
        discover_root_skills(&root, spec, &mut seen_skill_paths, &mut skills);
    }
    let installed_plugin_roots = if agent_id == "claude" {
        let claude_home =
            claude_config_dir.map_or_else(|| paths.home_dir.join(".claude"), Path::to_path_buf);
        let plugins = enabled_claude_plugins(&claude_home, project_path);
        for (plugin, install) in &plugins {
            discover_claude_plugin_skills(plugin, install, &mut skills);
        }
        plugins.into_iter().map(|(_, install)| install).collect()
    } else {
        Vec::new()
    };
    let skills = collapse_session_chat_skill_variants(skills, &installed_plugin_roots);

    json!({
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "skills": skills.into_iter().map(|skill| skill_to_value(skill, &agent_id)).collect::<Vec<_>>(),
        "agentId": agent_id,
    })
}

fn read_grok_session_chat_skills(paths: &GxserverPaths, project_path: Option<&Path>) -> Value {
    let working_directory = project_path
        .filter(|path| path.is_absolute())
        .unwrap_or(&paths.home_dir);
    let installed_grok = paths
        .home_dir
        .join(".grok")
        .join("bin")
        .join(format!("grok{}", std::env::consts::EXE_SUFFIX));
    let grok_command = if installed_grok.is_file() {
        installed_grok
    } else {
        PathBuf::from("grok")
    };
    let inspected = run_grok_skill_inspect(&grok_command, working_directory);

    let mut seen_skill_paths = HashSet::new();
    let skills = inspected
        .as_ref()
        .and_then(|value| value.get("skills"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|skill| skill.get("userInvocable").and_then(Value::as_bool) != Some(false))
        .filter_map(|skill| {
            let skill_file_path = skill
                .get("source")
                .and_then(Value::as_object)
                .and_then(|source| source.get("path"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)?;
            let directory_path = skill_file_path.parent()?.to_path_buf();
            let identity =
                fs::canonicalize(&skill_file_path).unwrap_or_else(|_| skill_file_path.clone());
            if !seen_skill_paths.insert(identity) {
                return None;
            }
            let name = skill
                .get("invocableAs")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .or_else(|| {
                    skill
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                })?
                .to_string();
            let source_kind = match skill
                .get("source")
                .and_then(Value::as_object)
                .and_then(|source| source.get("type"))
                .and_then(Value::as_str)
            {
                Some("project") => "repository",
                Some("plugin") => "pluginCache",
                _ => "global",
            };
            let root_path = directory_path.parent()?.to_path_buf();
            Some(SessionChatSkill {
                directory_path,
                skill_file_path,
                name,
                source_kind,
                root_path,
                variant_label: None,
            })
        })
        .collect::<Vec<_>>();
    let skills = collapse_session_chat_skill_variants(skills, &[]);

    json!({
        "agentId": "grok",
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "skills": skills.into_iter().map(|skill| skill_to_value(skill, "grok")).collect::<Vec<_>>(),
    })
}

/*
Grok inspection runs on a blocking request worker, so the process must have its
own hard deadline. Drain stdout concurrently to avoid a full pipe deadlocking
the child, cap the captured JSON, and terminate plus reap the child on every
timeout or wait failure. On Unix the child owns a process group so helpers it
starts cannot keep the stdout pipe open after the deadline.
*/
fn run_grok_skill_inspect(grok_command: &Path, working_directory: &Path) -> Option<Value> {
    let mut command = crate::platform::process::background_command(grok_command);
    command
        .args(["inspect", "--json"])
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_grok_skill_inspect_process_group(&mut command);

    let mut child = command.spawn().ok()?;
    let Some(stdout) = child.stdout.take() else {
        terminate_grok_skill_inspect(&mut child);
        return None;
    };
    let (output_sender, output_receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let read_result = stdout
            .take((GROK_SKILL_INSPECT_OUTPUT_LIMIT_BYTES + 1) as u64)
            .read_to_end(&mut output);
        let output = read_result
            .ok()
            .filter(|_| output.len() <= GROK_SKILL_INSPECT_OUTPUT_LIMIT_BYTES)
            .map(|_| output);
        let _ = output_sender.send(output);
    });

    let deadline = Instant::now() + GROK_SKILL_INSPECT_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                terminate_grok_skill_inspect(&mut child);
                return None;
            }
            Ok(None) => std::thread::sleep(GROK_SKILL_INSPECT_POLL_INTERVAL),
            Err(_) => {
                terminate_grok_skill_inspect(&mut child);
                return None;
            }
        }
    };
    if !status.success() {
        terminate_grok_skill_inspect(&mut child);
        return None;
    }

    let output = match output_receiver.recv_timeout(GROK_SKILL_INSPECT_READER_GRACE) {
        Ok(Some(output)) => output,
        Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
            terminate_grok_skill_inspect(&mut child);
            return None;
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            terminate_grok_skill_inspect(&mut child);
            let _ = output_receiver.recv_timeout(GROK_SKILL_INSPECT_READER_GRACE);
            return None;
        }
    };
    serde_json::from_slice::<Value>(&output).ok()
}

fn configure_grok_skill_inspect_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(not(unix))]
    let _ = command;
}

fn terminate_grok_skill_inspect(child: &mut Child) {
    #[cfg(unix)]
    {
        let process_group_id = child.id() as libc::pid_t;
        if process_group_id > 0 {
            unsafe {
                libc::kill(-process_group_id, libc::SIGKILL);
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
}

fn normalize_agent_id(agent_id: &str) -> String {
    match agent_id.trim().to_ascii_lowercase().as_str() {
        "openclaude" | "claude-code" => "claude".to_string(),
        "cursor-cli" => "cursor".to_string(),
        "gemini-cli" => "gemini".to_string(),
        "github-copilot" => "copilot".to_string(),
        "factory-droid" => "droid".to_string(),
        "antigravity-cli" => "antigravity".to_string(),
        "amp-cli" => "amp".to_string(),
        "kiro-cli" => "kiro".to_string(),
        "rovo-dev" => "rovodev".to_string(),
        other => other.to_string(),
    }
}

/// The home folders each agent's own CLI loads skills from, nearest-first as the CLI ranks them.
///
/// CDXC:AgentSkills 2026-10-17 WHY:
/// The `$` picker lists what the session's agent can actually run, so every row mirrors the CLI's own loader (agent flow tests, 2026-10-17: Pi 1.3 loads all of its skills from `~/.agents/skills`, which the picker never read, so it showed "No skills available"). Sources: Pi `core/package-manager.js` and omp `src/discovery/`; Codex `ext/skills/src/host_roots.rs`; OpenCode `config/discovery.ts`; gemini-cli `config/storage.ts`; Claude Code, Cursor, Copilot, Droid, agy, Amp, CodeBuddy and Qoder from their shipped bundles and docs. Claude Code reads no `.agents/skills` folder (cswap account folders link their `skills` to `~/.claude/skills`), and no agent reads `~/agents/skills`.
/// SEE-ALSO: `project_skill_roots` below.
fn provider_skill_roots(agent_id: &str) -> &'static [&'static [&'static str]] {
    match agent_id {
        "claude" => &[&[".claude", "skills"]],
        "codex" => &[&[".codex", "skills"], &[".agents", "skills"]],
        // `skills-cursor` holds the skills Cursor ships and syncs itself (`create-hook`, `review`,
        // `statusline` …), which its own `/` list offers beside the user's.
        "cursor" => &[
            &[".cursor", "skills"],
            &[".cursor", "skills-cursor"],
            &[".agents", "skills"],
            &[".claude", "skills"],
            &[".codex", "skills"],
            &[".grok", "skills"],
        ],
        "opencode" => &[
            &[".config", "opencode", "skill"],
            &[".config", "opencode", "skills"],
            &[".claude", "skills"],
            &[".agents", "skills"],
        ],
        "pi" => &[&[".pi", "agent", "skills"], &[".agents", "skills"]],
        "omp" => &[
            &[".omp", "agent", "skills"],
            &[".agents", "skills"],
            &[".agent", "skills"],
        ],
        "gemini" => &[&[".gemini", "skills"], &[".agents", "skills"]],
        "copilot" => &[&[".copilot", "skills"], &[".agents", "skills"]],
        "droid" => &[
            &[".factory", "skills"],
            &[".agents", "skills"],
            &[".agent", "skills"],
        ],
        "antigravity" => &[
            &[".gemini", "config", "skills"],
            &[".gemini", "antigravity-cli", "skills"],
            &[".gemini", "antigravity", "skills"],
        ],
        "amp" => &[
            &[".config", "agents", "skills"],
            &[".config", "amp", "skills"],
            &[".claude", "skills"],
            &[".agents", "skills"],
        ],
        // The chat asks with its transcript id, `hermes`; the agent catalog's id is `hermes-agent`.
        "hermes" | "hermes-agent" => &[&[".hermes", "skills"]],
        "kiro" => &[&[".kiro", "skills"]],
        "codebuddy" => &[
            &[".codebuddy", "skills"],
            &[".codebuddy", "connectors", "skills"],
        ],
        "qoder" => &[&[".qoder", "skills"], &[".agents", "skills"]],
        "rovodev" => &[&[".rovodev", "skills"], &[".agents", "skills"]],
        _ => &[],
    }
}

/// How far above the session's folder an agent looks for project skills.
#[derive(Clone, Copy)]
enum ProjectSkillWalk {
    /// The session's folder only.
    Folder,
    /// Every folder from the session's up to the repository root (the first holding `.git`).
    RepositoryRoot,
    /// Every folder up to `/` (OpenCode and Amp).
    FilesystemRoot,
}

/// The project folders each agent's own CLI loads skills from, and how far up it looks.
fn project_skill_roots(agent_id: &str) -> (&'static [&'static [&'static str]], ProjectSkillWalk) {
    use ProjectSkillWalk::{FilesystemRoot, Folder, RepositoryRoot};
    match agent_id {
        "claude" => (&[&[".claude", "skills"]], RepositoryRoot),
        "codex" => (
            &[&[".agents", "skills"], &[".codex", "skills"]],
            RepositoryRoot,
        ),
        "cursor" => (
            &[
                &[".cursor", "skills"],
                &[".agents", "skills"],
                &[".claude", "skills"],
                &[".codex", "skills"],
                &[".grok", "skills"],
            ],
            Folder,
        ),
        "opencode" => (
            &[
                &[".opencode", "skill"],
                &[".opencode", "skills"],
                &[".claude", "skills"],
                &[".agents", "skills"],
            ],
            FilesystemRoot,
        ),
        "pi" => (
            &[&[".pi", "skills"], &[".agents", "skills"]],
            RepositoryRoot,
        ),
        "omp" => (
            &[
                &[".omp", "skills"],
                &[".agents", "skills"],
                &[".agent", "skills"],
                &[".claude", "skills"],
                &[".codex", "skills"],
                &[".github", "skills"],
            ],
            RepositoryRoot,
        ),
        "gemini" => (&[&[".gemini", "skills"], &[".agents", "skills"]], Folder),
        "copilot" => (
            &[
                &[".github", "skills"],
                &[".agents", "skills"],
                &[".claude", "skills"],
            ],
            RepositoryRoot,
        ),
        "droid" => (
            &[
                &[".factory", "skills"],
                &[".agents", "skills"],
                &[".agent", "skills"],
            ],
            RepositoryRoot,
        ),
        "antigravity" => (
            &[
                &[".agents", "skills"],
                &["_agents", "skills"],
                &[".agent", "skills"],
                &["_agent", "skills"],
            ],
            Folder,
        ),
        "amp" => (
            &[&[".agents", "skills"], &[".claude", "skills"]],
            FilesystemRoot,
        ),
        "kiro" => (&[&[".kiro", "skills"]], Folder),
        "codebuddy" => (&[&[".codebuddy", "skills"]], Folder),
        "qoder" => (&[&[".qoder", "skills"], &[".agents", "skills"]], Folder),
        "rovodev" => (&[&[".rovodev", "skills"], &[".agents", "skills"]], Folder),
        _ => (&[], Folder),
    }
}

/// The folders `walk` covers, nearest first. The home folder is left to the global roots, and a
/// repository walk outside any repository stops below it.
fn project_skill_folders(start: &Path, walk: ProjectSkillWalk, home_dir: &Path) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    for folder in start.ancestors() {
        if folder == home_dir {
            if matches!(walk, ProjectSkillWalk::FilesystemRoot) {
                continue;
            }
            break;
        }
        folders.push(folder.to_path_buf());
        match walk {
            ProjectSkillWalk::Folder => break,
            ProjectSkillWalk::RepositoryRoot if folder.join(".git").exists() => break,
            _ => {}
        }
    }
    folders
}

fn join_relative(root: &Path, parts: &[&str]) -> PathBuf {
    parts
        .iter()
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

fn discover_root_skills(
    root: &Path,
    spec: SkillRootSpec,
    seen_skill_paths: &mut HashSet<PathBuf>,
    skills: &mut Vec<SessionChatSkill>,
) {
    if !root.is_dir() {
        return;
    }
    let mut seen_directories = HashSet::new();
    walk_skill_directories(
        root,
        root,
        SESSION_CHAT_SKILL_DISCOVERY_MAX_DEPTH,
        spec,
        &mut seen_directories,
        seen_skill_paths,
        skills,
    );
}

fn walk_skill_directories(
    root: &Path,
    directory: &Path,
    remaining_depth: usize,
    spec: SkillRootSpec,
    seen_directories: &mut HashSet<PathBuf>,
    seen_skill_paths: &mut HashSet<PathBuf>,
    skills: &mut Vec<SessionChatSkill>,
) {
    let Ok(real_directory) = fs::canonicalize(directory) else {
        return;
    };
    if !seen_directories.insert(real_directory) {
        return;
    }

    let skill_file_path = directory.join("SKILL.md");
    if skill_file_path.is_file() {
        let identity =
            fs::canonicalize(&skill_file_path).unwrap_or_else(|_| skill_file_path.clone());
        if seen_skill_paths.insert(identity) {
            let name = directory
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or("Skill")
                .to_string();
            skills.push(SessionChatSkill {
                directory_path: directory.to_path_buf(),
                skill_file_path,
                name,
                source_kind: spec.source_kind,
                root_path: root.to_path_buf(),
                variant_label: None,
            });
        }
        return;
    }
    if remaining_depth == 0 {
        return;
    }

    let mut entries = match fs::read_dir(directory) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(_) => return,
    };
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_ignored_directory(&name) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() || file_type.is_symlink() {
            walk_skill_directories(
                root,
                &entry.path(),
                remaining_depth - 1,
                spec,
                seen_directories,
                seen_skill_paths,
                skills,
            );
        }
    }
}

fn is_ignored_directory(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "vendor"
            | "dist"
            | "build"
            | "out"
            | "target"
            | "coverage"
            | ".cache"
            | "__pycache__"
            | "tmp"
            | ".tmp"
    )
}

/// The plugins the session's Claude Code loads, `(plugin name, install folder)`: each entry of
/// `installed_plugins.json` that applies to the session's folder and that `enabledPlugins` turns
/// on (local settings over project settings over the user's). A plugin none of them names is off,
/// and its name is the one in its manifest, falling back to the `installed_plugins.json` key.
///
/// CDXC:AgentSkills 2026-10-29 WHY:
/// Claude Code loads a plugin's skills and commands only from the `installPath` of an enabled plugin and names them `/plugin:name`. Walking all of `plugins/cache` listed superseded versions, disabled plugins and leftover clone folders under bare names that two plugins can share, and never listed `commands/*.md`. Plugins are installed per config folder (`CLAUDE_CONFIG_DIR`), so the session's own folder is read. Checked against Claude Code 2.1.312's own list: a plugin missing from `enabledPlugins` stays unloaded even when its manifest says `defaultEnabled` (that key only matters when it is installed), and `/key-name:skill` for a plugin whose manifest names it differently reaches the model as plain text, so neither may be listed.
fn enabled_claude_plugins(
    claude_home: &Path,
    project_path: Option<&Path>,
) -> Vec<(String, PathBuf)> {
    let read = |path: PathBuf| {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    };
    let Some(installed) = read(claude_home.join("plugins").join("installed_plugins.json")) else {
        return Vec::new();
    };
    let mut settings = vec![read(claude_home.join("settings.json"))];
    if let Some(folder) = project_path {
        settings.push(read(folder.join(".claude").join("settings.json")));
        settings.push(read(folder.join(".claude").join("settings.local.json")));
    }
    installed["plugins"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, installs)| {
            // A project-scope install applies only inside its project.
            let install = installs.as_array()?.iter().find(|install| {
                install["projectPath"]
                    .as_str()
                    .is_none_or(|root| project_path.is_some_and(|folder| folder.starts_with(root)))
            })?;
            let install = PathBuf::from(install["installPath"].as_str()?);
            let enabled = settings
                .iter()
                .rev()
                .flatten()
                .find_map(|settings| settings["enabledPlugins"][key.as_str()].as_bool())
                .unwrap_or(false);
            if !enabled {
                return None;
            }
            let name = read(install.join(".claude-plugin").join("plugin.json"))
                .and_then(|manifest| manifest["name"].as_str().map(str::to_string))
                .filter(|name| !name.is_empty())
                .or_else(|| key.split('@').next().map(str::to_string))?;
            Some((name, install))
        })
        .collect()
}

/// A plugin's skills (`skills/<name>/SKILL.md`) and commands (`commands/<name>.md`) as Claude
/// Code's `/` menu lists them: `plugin:name`, a skill over a command of the same name, and nothing
/// marked `user-invocable: false`. Only the default folders are read, not extra ones a
/// plugin.json `skills` or `commands` key names.
fn discover_claude_plugin_skills(plugin: &str, install: &Path, skills: &mut Vec<SessionChatSkill>) {
    let mut names = HashSet::new();
    for (root, commands) in [
        (install.join("skills"), false),
        (install.join("commands"), true),
    ] {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        let mut entries = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            let (skill_file_path, name) = if commands {
                let is_markdown = path.extension().is_some_and(|extension| extension == "md");
                (path.clone(), path.file_stem().filter(|_| is_markdown))
            } else {
                (path.join("SKILL.md"), path.file_name())
            };
            let Some(name) = name.and_then(|name| name.to_str()) else {
                continue;
            };
            let name = format!("{plugin}:{name}");
            if !skill_file_path.is_file()
                || !names.insert(name.clone())
                || !user_invocable(&skill_file_path)
            {
                continue;
            }
            skills.push(SessionChatSkill {
                directory_path: path,
                skill_file_path,
                name,
                source_kind: "pluginCache",
                root_path: root.clone(),
                variant_label: None,
            });
        }
    }
}

/// False when the file's front matter sets `user-invocable: false`, which keeps a skill out of
/// Claude Code's `/` menu.
fn user_invocable(skill_file: &Path) -> bool {
    let Ok(text) = fs::read_to_string(skill_file) else {
        return false;
    };
    let mut lines = text.lines();
    lines.next().map(str::trim) != Some("---")
        || !lines
            .take_while(|line| line.trim() != "---")
            .filter_map(|line| line.strip_prefix("user-invocable:"))
            .any(|value| value.trim().trim_matches(['"', '\'']) == "false")
}

fn skill_to_value(skill: SessionChatSkill, agent_id: &str) -> Value {
    let mut value = json!({
        "directoryPath": skill.directory_path.to_string_lossy(),
        "name": skill.name,
        "skillFilePath": skill.skill_file_path.to_string_lossy(),
        "sourceKind": skill.source_kind,
    });
    if let Some(variant_label) = skill.variant_label {
        value["variantLabel"] = Value::String(variant_label);
    }
    if GHOSTEX_AGENT_SKILL_NAMES.contains(&skill.name.as_str()) {
        value["ghostex"] = Value::Bool(true);
        value["invocation"] = Value::String(skill_invocation(Some(agent_id), &skill.name));
        if let Some(description) = ghostex_skill_description(&skill.name) {
            value["description"] = Value::String(description);
        }
    }
    value
}

/// A bundled Ghostex skill's customer description, from the catalog Settings > Integrations shows.
///
/// CDXC:AgentSkills 2026-10-10 WHY:
/// The ⋯ menus' Skills submenu (chat and terminal, desktop and phone) lists the Ghostex skills the session's agent has installed, so the marker rides the per-session read the `$` picker already makes: the real install set (`GHOSTEX_AGENT_SKILL_NAMES`) and no list copied into a client. `invocation` is how the agent's own input runs the skill, which is what the terminal menus type. The SKILL.md description is written for agents ("Use this skill when…"), so the menu's detail line takes the Settings catalog's customer copy.
/// SEE-ALSO: packages/gx-chat-core/src/composer/ghostex_skills.rs, apps/desktop/src/app/render/terminal_agent_action_bar/skills_flyout.rs, apps/mobile/app/src/chat/native/composer/menus.ts.
fn ghostex_skill_description(skill_name: &str) -> Option<String> {
    use ghostex_settings_catalog::modules::{exports, module};
    let (_, skills) = exports(
        module::AGENT_SKILLS,
        ghostex_settings_catalog::Platform::current(),
    )
    .into_iter()
    .find(|(name, _)| *name == "VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS")?;
    skills
        .to_value()
        .as_array()?
        .iter()
        .find(|skill| skill.get("skillName").and_then(Value::as_str) == Some(skill_name))?
        .get("description")?
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::read_session_chat_skills;
    use crate::paths::get_gxserver_paths;

    fn add_skill(root: &Path, name: &str) {
        let directory = root.join(name);
        fs::create_dir_all(&directory).expect("skill directory");
        fs::write(directory.join("SKILL.md"), format!("# {name}\n")).expect("skill instructions");
    }

    #[test]
    fn filters_global_and_repository_skills_by_session_agent() {
        let home = tempdir().expect("home");
        let repository = tempdir().expect("repository");
        add_skill(&home.path().join(".agents/skills"), "shared-skill");
        add_skill(&home.path().join(".codex/skills"), "codex-skill");
        add_skill(&home.path().join(".claude/skills"), "claude-skill");
        add_skill(&repository.path().join(".agents/skills"), "project-shared");
        add_skill(&repository.path().join(".claude/skills"), "project-claude");
        let paths = get_gxserver_paths(Some(home.path().to_path_buf()));

        let codex = read_session_chat_skills(&paths, "codex", Some(repository.path()), None);
        let codex_names = codex["skills"]
            .as_array()
            .expect("codex skills")
            .iter()
            .filter_map(|skill| skill["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            codex_names,
            vec!["codex-skill", "project-shared", "shared-skill"]
        );

        let claude = read_session_chat_skills(&paths, "claude", Some(repository.path()), None);
        let claude_names = claude["skills"]
            .as_array()
            .expect("claude skills")
            .iter()
            .filter_map(|skill| skill["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(claude_names, vec!["claude-skill", "project-claude"]);
    }

    #[test]
    fn lists_enabled_claude_plugin_skills_and_commands_by_plugin_name() {
        let home = tempdir().expect("home");
        let config = home.path().join("profile");
        let cache = config.join("plugins/cache/market");
        let install = cache.join("tools/2.0.0");
        add_skill(&install.join("skills"), "alpha");
        add_skill(&cache.join("tools/1.0.0/skills"), "superseded");
        fs::create_dir_all(install.join("skills/hidden")).expect("hidden skill");
        fs::write(
            install.join("skills/hidden/SKILL.md"),
            "---\nname: hidden\nuser-invocable: false\n---\n",
        )
        .expect("hidden instructions");
        fs::create_dir_all(install.join("commands")).expect("commands");
        for command in ["alpha.md", "beta.md", "notes.txt"] {
            fs::write(install.join("commands").join(command), "# command\n").expect("command");
        }
        let disabled = cache.join("off/1.0.0");
        add_skill(&disabled.join("skills"), "gamma");
        fs::write(
            config.join("plugins/installed_plugins.json"),
            serde_json::json!({"version": 2, "plugins": {
                "tools@market": [{"scope": "user", "installPath": install}],
                "off@market": [{"scope": "user", "installPath": disabled}],
                "elsewhere@market": [
                    {"scope": "project", "projectPath": "/nowhere", "installPath": install}
                ],
            }})
            .to_string(),
        )
        .expect("installed plugins");
        fs::write(
            config.join("settings.json"),
            r#"{"enabledPlugins": {"tools@market": true, "off@market": false, "elsewhere@market": true}}"#,
        )
        .expect("settings");
        let paths = get_gxserver_paths(Some(home.path().to_path_buf()));

        let claude = read_session_chat_skills(&paths, "claude", None, Some(&config));
        let names = claude["skills"]
            .as_array()
            .expect("claude skills")
            .iter()
            .filter_map(|skill| skill["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["tools:alpha", "tools:beta"]);
    }

    /// One user-scope plugin per `(name, skills, manifest)` under `config`, listed for Claude with
    /// these user `enabledPlugins`.
    fn claude_plugin_rows(
        plugins: &[(&str, &[&str], Value)],
        enabled: Value,
        project: Option<&Path>,
    ) -> Vec<String> {
        let home = tempdir().expect("home");
        let config = home.path().join("profile");
        let mut installed = serde_json::Map::new();
        for (name, skills, manifest) in plugins {
            let install = config.join("plugins/cache/market").join(name).join("1.0.0");
            for skill in *skills {
                add_skill(&install.join("skills"), skill);
            }
            fs::create_dir_all(install.join(".claude-plugin")).expect("manifest folder");
            fs::write(
                install.join(".claude-plugin/plugin.json"),
                manifest.to_string(),
            )
            .expect("manifest");
            installed.insert(
                format!("{name}@market"),
                json!([{"scope": "user", "installPath": install}]),
            );
        }
        fs::write(
            config.join("plugins/installed_plugins.json"),
            json!({"version": 2, "plugins": installed}).to_string(),
        )
        .expect("installed plugins");
        fs::write(
            config.join("settings.json"),
            json!({"enabledPlugins": enabled}).to_string(),
        )
        .expect("settings");
        let paths = get_gxserver_paths(Some(home.path().to_path_buf()));
        read_session_chat_skills(&paths, "claude", project, Some(&config))["skills"]
            .as_array()
            .expect("claude skills")
            .iter()
            .filter_map(|skill| skill["name"].as_str().map(str::to_string))
            .collect()
    }

    #[test]
    fn a_claude_plugin_stays_off_unless_enabled_plugins_turns_it_on() {
        let project = tempdir().expect("project");
        fs::create_dir_all(project.path().join(".claude")).expect("project settings folder");
        fs::write(
            project.path().join(".claude/settings.json"),
            r#"{"enabledPlugins": {"project-on@market": true, "project-off@market": false}}"#,
        )
        .expect("project settings");
        fs::write(
            project.path().join(".claude/settings.local.json"),
            r#"{"enabledPlugins": {"local-off@market": false}}"#,
        )
        .expect("local settings");
        let rows = claude_plugin_rows(
            &[
                ("on", &["a"], json!({"name": "on"})),
                ("unnamed", &["b"], json!({"name": "unnamed"})),
                (
                    "default-on",
                    &["c"],
                    json!({"name": "default-on", "defaultEnabled": true}),
                ),
                ("versioned", &["d"], json!({"name": "versioned"})),
                ("project-on", &["e"], json!({"name": "project-on"})),
                ("project-off", &["f"], json!({"name": "project-off"})),
                ("local-off", &["g"], json!({"name": "local-off"})),
            ],
            json!({
                "on@market": true,
                "versioned@market": ["1.0.0"],
                "project-off@market": true,
                "local-off@market": true,
            }),
            Some(project.path()),
        );
        assert_eq!(rows, vec!["on:a", "project-on:e"]);
    }

    #[test]
    fn a_claude_plugin_row_uses_the_name_in_its_manifest() {
        let rows = claude_plugin_rows(
            &[
                ("listed-key", &["a"], json!({"name": "manifest-name"})),
                ("no-name", &["b"], json!({})),
            ],
            json!({"listed-key@market": true, "no-name@market": true}),
            None,
        );
        assert_eq!(rows, vec!["manifest-name:a", "no-name:b"]);
    }
}

pub(crate) async fn handle_read_session_chat_skills_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if project_id.is_empty() || session_id.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "readSessionChatSkills requires projectId and sessionId.".to_string(),
            },
        );
    }

    let resolved = (|| {
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = repository
            .get_session(project_id, session_id)?
            .ok_or_else(|| DomainStateError {
                code: "notFound",
                message: "The session no longer exists.".to_string(),
            })?;
        let project = repository
            .get_project(project_id)?
            .ok_or_else(|| DomainStateError {
                code: "notFound",
                message: "The project no longer exists.".to_string(),
            })?;
        // The CLI the session runs (OMP keeps its own folders though its transcript reads as Pi's).
        let agent_id = session_chat_display_agent_for_session(&session).unwrap_or_default();
        // The agent loads project skills from the folder it runs in (a worktree session's own checkout).
        let folder = |value: &Value, key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|path| Path::new(path).is_absolute())
                .map(PathBuf::from)
        };
        let project_path = folder(&session, "cwd").or_else(|| folder(&project, "path"));
        let claude_config_dir = read_runtime_text(&session, "agentSessionPath")
            .map(|path| claude_config_directory(Some(&path)));
        Ok::<_, DomainStateError>((agent_id, project_path, claude_config_dir))
    })();
    let (agent_id, project_path, claude_config_dir) = match resolved {
        Ok(resolved) => resolved,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let paths = state.paths.clone();
    let result = match tokio::task::spawn_blocking(move || {
        read_session_chat_skills(
            &paths,
            &agent_id,
            project_path.as_deref(),
            claude_config_dir.as_deref(),
        )
    })
    .await
    {
        Ok(result) => result,
        Err(_) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: "Session chat skills could not be read.".to_string(),
                },
            )
        }
    };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}

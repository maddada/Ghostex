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

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::session_chat_follower::session_chat_agent_for_session;
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
struct SessionChatSkill {
    directory_path: PathBuf,
    skill_file_path: PathBuf,
    name: String,
    source_kind: &'static str,
}

/*
CDXC:AgentSkills 2026-08-15:
Chat skill mentions are discovered by gxserver on the session's machine. Grok
Build owns a resolved catalog that includes bundled skills and collision-safe
invocation names, so its own `inspect` output is authoritative. Other agents
use their provider-owned roots plus the shared Claude/Codex roots. A project
contributes its own .agents/.claude roots without letting a client submit
arbitrary scan paths.
*/
pub fn read_session_chat_skills(
    paths: &GxserverPaths,
    agent_id: &str,
    project_path: Option<&Path>,
) -> Value {
    let agent_id = normalize_agent_id(agent_id);
    if agent_id == "grok" {
        return read_grok_session_chat_skills(paths, project_path);
    }
    let mut roots = Vec::<(PathBuf, SkillRootSpec)>::new();

    if let Some(project_path) = project_path.filter(|path| path.is_absolute()) {
        if agent_id == "claude" {
            roots.push((
                project_path.join(".claude").join("skills"),
                SkillRootSpec {
                    source_kind: "repository",
                },
            ));
        }
        if matches!(agent_id.as_str(), "claude" | "codex") {
            roots.push((
                project_path.join(".agents").join("skills"),
                SkillRootSpec {
                    source_kind: "repository",
                },
            ));
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
    if matches!(agent_id.as_str(), "claude" | "codex") {
        roots.push((
            paths.home_dir.join(".agents").join("skills"),
            SkillRootSpec {
                source_kind: "global",
            },
        ));
        roots.push((
            paths.home_dir.join("agents").join("skills"),
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
    if agent_id == "claude" {
        roots.push((
            paths.home_dir.join(".claude").join("plugins").join("cache"),
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
    skills.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.directory_path.cmp(&right.directory_path))
    });

    json!({
        "agentId": agent_id,
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "skills": skills.into_iter().map(skill_to_value).collect::<Vec<_>>(),
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
    let grok_command = installed_grok
        .is_file()
        .then_some(installed_grok)
        .unwrap_or_else(|| PathBuf::from("grok"));
    let inspected = run_grok_skill_inspect(&grok_command, working_directory);

    let mut seen_skill_paths = HashSet::new();
    let mut skills = inspected
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
            Some(SessionChatSkill {
                directory_path,
                skill_file_path,
                name,
                source_kind,
            })
        })
        .collect::<Vec<_>>();
    skills.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.directory_path.cmp(&right.directory_path))
    });

    json!({
        "agentId": "grok",
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "skills": skills.into_iter().map(skill_to_value).collect::<Vec<_>>(),
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
    let mut command = Command::new(grok_command);
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

fn provider_skill_roots(agent_id: &str) -> &'static [&'static [&'static str]] {
    match agent_id {
        "claude" => &[&[".claude", "skills"]],
        "codex" => &[&[".codex", "skills"]],
        "cursor" => &[&[".cursor", "skills"]],
        "opencode" => &[&[".config", "opencode", "skills"]],
        "pi" | "omp" => &[&[".pi", "agent", "skills"]],
        "gemini" => &[&[".gemini", "skills"]],
        "copilot" => &[&[".copilot", "skills"]],
        "droid" => &[&[".factory", "skills"]],
        "antigravity" => &[
            &[".gemini", "antigravity-cli", "skills"],
            &[".gemini", "antigravity", "skills"],
        ],
        "amp" => &[&[".config", "agents", "skills"]],
        "hermes-agent" => &[&[".hermes", "skills"]],
        "kiro" => &[&[".kiro", "skills"]],
        "codebuddy" => &[&[".codebuddy", "skills"]],
        "qoder" => &[&[".qoder", "skills"]],
        "rovodev" => &[&[".rovodev", "skills"]],
        _ => &[],
    }
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
        SESSION_CHAT_SKILL_DISCOVERY_MAX_DEPTH,
        spec,
        &mut seen_directories,
        seen_skill_paths,
        skills,
    );
}

fn walk_skill_directories(
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

fn skill_to_value(skill: SessionChatSkill) -> Value {
    json!({
        "directoryPath": skill.directory_path.to_string_lossy(),
        "name": skill.name,
        "skillFilePath": skill.skill_file_path.to_string_lossy(),
        "sourceKind": skill.source_kind,
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

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

        let codex = read_session_chat_skills(&paths, "codex", Some(repository.path()));
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

        let claude = read_session_chat_skills(&paths, "claude", Some(repository.path()));
        let claude_names = claude["skills"]
            .as_array()
            .expect("claude skills")
            .iter()
            .filter_map(|skill| skill["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            claude_names,
            vec![
                "claude-skill",
                "project-claude",
                "project-shared",
                "shared-skill",
            ]
        );
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
        let agent_id = session_chat_agent_for_session(&session).unwrap_or_default();
        let project_path = project
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        Ok::<_, DomainStateError>((agent_id, project_path))
    })();
    let (agent_id, project_path) = match resolved {
        Ok(resolved) => resolved,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let paths = state.paths.clone();
    let result = match tokio::task::spawn_blocking(move || {
        read_session_chat_skills(&paths, &agent_id, project_path.as_deref())
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

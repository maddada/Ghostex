//! Bundled Ghostex agent skills fetched from the public repository, so an
//! installed build picks up skill fixes without waiting for a new app release.

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde_json::{json, Value};
use sha1::{Digest, Sha1};

use crate::{
    agent_skills::{
        agent_skill_global_dir, copy_skill_directory_into_place, path_string,
        AGENT_SKILL_COPY_EXCLUDED_NAMES, CANONICAL_AGENT_SKILLS_DIR, GHOSTEX_AGENT_SKILL_NAMES,
        GHOSTEX_SKILLS_CLI_AGENT_IDS,
    },
    logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel},
    paths::GxserverPaths,
};

/*
CDXC:AgentSkills 2026-09-10 DECISION:
User: bundled skills must be updatable without pushing a new app version. gxserver downloads each bundled skill from the public repository's main branch itself, keeps the app-bundled copy as the offline source, and refreshes every installed bundled skill shortly after it starts.
This supersedes the 2026-09-02 bundle-only installs. That change dropped `npx skills add` because it needed Node on the app's PATH and a reachable npm registry; both reasons still hold, so the download is a plain HTTPS fetch with no Node involved.
WHY: the listing is one unauthenticated GitHub trees call for `main:skills` (60 an hour per address), and files come from raw.githubusercontent.com whose cache may lag main by a few minutes. Every file is checked against its git blob sha, and a mismatch skips that skill until the next attempt instead of installing a torn copy. Symlinked skill folders are user-managed pins and are never replaced. GHOSTEX_AGENT_SKILLS_REMOTE=off turns the network path off for source checkouts, CI, and air-gapped machines.
*/
pub const AGENT_SKILLS_REMOTE_REPOSITORY: &str = "maddada/Ghostex";
pub const AGENT_SKILLS_REMOTE_REF: &str = "main";
const AGENT_SKILLS_REMOTE_ROOT: &str = "skills";
const AGENT_SKILLS_REMOTE_OPT_OUT_ENV: &str = "GHOSTEX_AGENT_SKILLS_REMOTE";
const AGENT_SKILLS_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const AGENT_SKILLS_STARTUP_REFRESH_DELAY: Duration = Duration::from_secs(20);
const AGENT_SKILLS_USER_AGENT: &str = "ghostex-gxserver";
const AGENT_SKILLS_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// False in unit tests and when the operator opted out through the
/// environment; every remote path checks this first.
pub fn remote_skills_enabled() -> bool {
    if cfg!(test) {
        return false;
    }
    let value = std::env::var(AGENT_SKILLS_REMOTE_OPT_OUT_ENV).unwrap_or_default();
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "off" | "false" | "no"
    )
}

pub fn remote_skills_cache_root(paths: &GxserverPaths) -> PathBuf {
    paths.app_cache_dir.join("agent-skills").join("github")
}

#[derive(Clone, Debug)]
struct RemoteSkillFile {
    executable: bool,
    relative_path: String,
    sha: String,
    size: u64,
}

/// The `skills/` folder of the repository at the published ref: one file list
/// per bundled skill name, with the git blob sha every download is checked
/// against.
pub struct RemoteSkillsCatalog {
    files_by_skill: BTreeMap<String, Vec<RemoteSkillFile>>,
    tree_sha: String,
}

impl RemoteSkillsCatalog {
    pub fn fetch() -> Result<Self, String> {
        let url = format!(
            "https://api.github.com/repos/{AGENT_SKILLS_REMOTE_REPOSITORY}/git/trees/{AGENT_SKILLS_REMOTE_REF}:{AGENT_SKILLS_REMOTE_ROOT}?recursive=1"
        );
        let response = ureq::get(&url)
            .set("Accept", "application/vnd.github+json")
            .set("User-Agent", AGENT_SKILLS_USER_AGENT)
            .set("X-GitHub-Api-Version", "2022-11-28")
            .timeout(AGENT_SKILLS_REQUEST_TIMEOUT)
            .call()
            .map_err(|error| {
                format!(
                    "GitHub skill listing failed: {}",
                    summarize_ureq_error(error)
                )
            })?;
        let value: Value = response
            .into_json()
            .map_err(|error| format!("GitHub skill listing was not JSON: {error}"))?;
        if value
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err("GitHub skill listing was truncated.".to_string());
        }
        let tree_sha = value
            .get("sha")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut files_by_skill: BTreeMap<String, Vec<RemoteSkillFile>> = BTreeMap::new();
        for entry in value
            .get("tree")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if entry.get("type").and_then(Value::as_str) != Some("blob") {
                continue;
            }
            let (Some(path), Some(sha)) = (
                entry.get("path").and_then(Value::as_str),
                entry.get("sha").and_then(Value::as_str),
            ) else {
                continue;
            };
            let Some((skill_name, relative_path)) = path.split_once('/') else {
                continue;
            };
            if !GHOSTEX_AGENT_SKILL_NAMES.contains(&skill_name)
                || !is_safe_relative_path(relative_path)
            {
                continue;
            }
            files_by_skill
                .entry(skill_name.to_string())
                .or_default()
                .push(RemoteSkillFile {
                    executable: entry.get("mode").and_then(Value::as_str) == Some("100755"),
                    relative_path: relative_path.to_string(),
                    sha: sha.to_string(),
                    size: entry.get("size").and_then(Value::as_u64).unwrap_or(0),
                });
        }
        files_by_skill.retain(|_, files| files.iter().any(|file| file.relative_path == "SKILL.md"));
        Ok(Self {
            files_by_skill,
            tree_sha,
        })
    }

    pub fn tree_sha(&self) -> &str {
        &self.tree_sha
    }

    pub fn has_skill(&self, skill_name: &str) -> bool {
        self.files_by_skill.contains_key(skill_name)
    }

    /// True when the folder on disk holds exactly the published files, byte for
    /// byte, so a refresh has nothing to do.
    pub fn local_dir_matches(&self, skill_name: &str, dir: &Path) -> bool {
        let Some(files) = self.files_by_skill.get(skill_name) else {
            return false;
        };
        let mut local = BTreeMap::new();
        if collect_local_blob_shas(dir, dir, &mut local).is_err() {
            return false;
        }
        local.len() == files.len()
            && files.iter().all(|file| {
                local
                    .get(&file.relative_path)
                    .is_some_and(|sha| sha == &file.sha)
            })
    }

    /// Download the skill into `<cache_root>/<skill>` and return that folder.
    /// The files are staged beside the destination and renamed into place only
    /// after every one of them matched its published checksum. A cache folder
    /// that already matches is reused without any request.
    pub fn download_skill(&self, skill_name: &str, cache_root: &Path) -> Result<PathBuf, String> {
        let Some(files) = self.files_by_skill.get(skill_name) else {
            return Err(format!(
                "{skill_name} is not published under {AGENT_SKILLS_REMOTE_ROOT}/ on {AGENT_SKILLS_REMOTE_REF}."
            ));
        };
        let destination = cache_root.join(skill_name);
        if self.local_dir_matches(skill_name, &destination) {
            return Ok(destination);
        }
        fs::create_dir_all(cache_root)
            .map_err(|error| format!("Could not create {}: {error}", cache_root.display()))?;
        let staging = cache_root.join(format!(".{skill_name}.download-{}", std::process::id()));
        let _ = fs::remove_dir_all(&staging);
        let staged = files
            .iter()
            .try_for_each(|file| stage_remote_file(skill_name, file, &staging));
        if let Err(error) = staged {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        let _ = fs::remove_dir_all(&destination);
        fs::rename(&staging, &destination).map_err(|error| {
            let _ = fs::remove_dir_all(&staging);
            format!("Could not move {skill_name} into the skill cache: {error}")
        })?;
        Ok(destination)
    }
}

fn stage_remote_file(
    skill_name: &str,
    file: &RemoteSkillFile,
    staging: &Path,
) -> Result<(), String> {
    if file.size > AGENT_SKILLS_MAX_FILE_BYTES {
        return Err(format!(
            "{skill_name}/{} is larger than the {} MB skill file limit.",
            file.relative_path,
            AGENT_SKILLS_MAX_FILE_BYTES / (1024 * 1024)
        ));
    }
    let bytes = fetch_raw_skill_file(skill_name, &file.relative_path)?;
    if git_blob_sha1(&bytes) != file.sha {
        return Err(format!(
            "{skill_name}/{} did not match its published checksum (the download mirror may be behind main).",
            file.relative_path
        ));
    }
    let target = staging.join(&file.relative_path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    fs::write(&target, &bytes)
        .map_err(|error| format!("Could not write {}: {error}", target.display()))?;
    #[cfg(unix)]
    if file.executable {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&target, fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

fn fetch_raw_skill_file(skill_name: &str, relative_path: &str) -> Result<Vec<u8>, String> {
    let url = format!(
        "https://raw.githubusercontent.com/{AGENT_SKILLS_REMOTE_REPOSITORY}/{AGENT_SKILLS_REMOTE_REF}/{AGENT_SKILLS_REMOTE_ROOT}/{skill_name}/{relative_path}"
    );
    let response = ureq::get(&url)
        .set("User-Agent", AGENT_SKILLS_USER_AGENT)
        .timeout(AGENT_SKILLS_REQUEST_TIMEOUT)
        .call()
        .map_err(|error| {
            format!(
                "Download of {skill_name}/{relative_path} failed: {}",
                summarize_ureq_error(error)
            )
        })?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(AGENT_SKILLS_MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            format!("Download of {skill_name}/{relative_path} was cut short: {error}")
        })?;
    if bytes.len() as u64 > AGENT_SKILLS_MAX_FILE_BYTES {
        return Err(format!(
            "{skill_name}/{relative_path} is larger than the skill file limit."
        ));
    }
    Ok(bytes)
}

/// Keeps the response body out of the message: GitHub error pages are HTML and
/// a rate-limit reply carries nothing a user can act on beyond the status.
fn summarize_ureq_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(status, _) => format!("HTTP {status}"),
        ureq::Error::Transport(transport) => transport
            .message()
            .map(str::to_string)
            .unwrap_or_else(|| transport.kind().to_string()),
    }
}

/// Only plain path segments are accepted from the listing; anything else would
/// need URL escaping and could escape the staging folder.
fn is_safe_relative_path(relative_path: &str) -> bool {
    !relative_path.is_empty()
        && relative_path.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !AGENT_SKILL_COPY_EXCLUDED_NAMES.contains(&segment)
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
}

fn git_blob_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn collect_local_blob_shas(
    root: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, String>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if AGENT_SKILL_COPY_EXCLUDED_NAMES.contains(&name_str) {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            collect_local_blob_shas(root, &path, out)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| std::io::Error::other("skill file outside its folder"))?
                .components()
                .map(|component| component.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            out.insert(relative, git_blob_sha1(&fs::read(&path)?));
        }
    }
    Ok(())
}

/// Every global skill root a bundled skill may have been installed into: the
/// canonical universal folder plus each agent's own folder, deduplicated.
fn installed_skill_roots(home_dir: &Path) -> Vec<PathBuf> {
    let mut roots = vec![home_dir.join(CANONICAL_AGENT_SKILLS_DIR)];
    for agent_id in GHOSTEX_SKILLS_CLI_AGENT_IDS {
        if let Some(root) = agent_skill_global_dir(home_dir, agent_id) {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    }
    let mut seen_real = HashSet::new();
    roots
        .into_iter()
        .filter(|root| {
            let real = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
            seen_real.insert(real)
        })
        .collect()
}

/// Startup pass: a while after gxserver is up, replace every installed bundled
/// skill whose files differ from the published ones. Runs on its own thread so
/// a slow or absent network never delays startup, and stays silent when
/// offline or rate limited because nothing changed on disk.
pub fn spawn_startup_skill_refresh(paths: GxserverPaths, logger: Arc<GxserverLogger>) {
    if !remote_skills_enabled() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("agent-skills-refresh".to_string())
        .spawn(move || {
            std::thread::sleep(AGENT_SKILLS_STARTUP_REFRESH_DELAY);
            let Ok(result) = refresh_installed_bundled_skills(&paths) else {
                return;
            };
            let updated_count = result["updated"].as_array().map_or(0, Vec::len);
            let failed_count = result["failed"].as_array().map_or(0, Vec::len);
            if updated_count == 0 && failed_count == 0 {
                return;
            }
            let entry = GxserverLogInput {
                level: if failed_count == 0 {
                    LogLevel::Info
                } else {
                    LogLevel::Warn
                },
                event: "agentSkillsRefreshedFromGitHub".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: Some(result),
            };
            let _ = logger.log_routine(DiagnosticLogScenario::ServerLifecycle, entry);
        });
}

/// Replace every installed bundled skill whose files differ from the published
/// ones. Returns the refresh report, or the reason the listing could not be
/// fetched (offline, rate limited, downloads disabled).
pub fn refresh_installed_bundled_skills(paths: &GxserverPaths) -> Result<Value, String> {
    if !remote_skills_enabled() {
        return Err(format!(
            "GitHub skill downloads are disabled ({AGENT_SKILLS_REMOTE_OPT_OUT_ENV}=off)."
        ));
    }
    let catalog = RemoteSkillsCatalog::fetch()?;
    let cache_root = remote_skills_cache_root(paths);
    let roots = installed_skill_roots(&paths.home_dir);
    let mut refreshed_names = HashSet::new();
    let mut updated = Vec::new();
    let mut failed = Vec::new();
    for skill_name in GHOSTEX_AGENT_SKILL_NAMES {
        if !refreshed_names.insert(*skill_name) || !catalog.has_skill(skill_name) {
            continue;
        }
        let stale: Vec<PathBuf> = roots
            .iter()
            .map(|root| root.join(skill_name))
            .filter(|dir| {
                fs::symlink_metadata(dir)
                    .map(|metadata| !metadata.file_type().is_symlink() && metadata.is_dir())
                    .unwrap_or(false)
                    && !catalog.local_dir_matches(skill_name, dir)
            })
            .collect();
        if stale.is_empty() {
            continue;
        }
        let source_dir = match catalog.download_skill(skill_name, &cache_root) {
            Ok(dir) => dir,
            Err(error) => {
                failed.push(json!({ "error": error, "skill": skill_name }));
                continue;
            }
        };
        for destination in stale {
            match copy_skill_directory_into_place(&source_dir, &destination) {
                Ok(true) => updated.push(path_string(&destination)),
                Ok(false) => {}
                Err(error) => failed.push(json!({
                    "error": error.message,
                    "path": path_string(&destination),
                    "skill": skill_name,
                })),
            }
        }
    }
    Ok(json!({
        "failed": failed,
        "ref": AGENT_SKILLS_REMOTE_REF,
        "repository": AGENT_SKILLS_REMOTE_REPOSITORY,
        "treeSha": catalog.tree_sha(),
        "type": "agentSkillRefresh",
        "updated": updated,
    }))
}

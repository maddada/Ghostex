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
use sha2::{Digest, Sha256};

use crate::{
    agent_skills::{
        agent_reads_canonical_skills, agent_skill_global_dir, copy_skill_directory_into_place,
        link_skill_into_agent_root, path_string, AGENT_SKILL_COPY_EXCLUDED_NAMES,
        CANONICAL_AGENT_SKILLS_DIR, GHOSTEX_AGENT_SKILL_NAMES, GHOSTEX_SKILLS_CLI_AGENT_IDS,
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
/*
CDXC:AgentSkills 2026-11-08 DECISION:
GitHub #495 follow-up (Q1868, "pin skills"): a changed file on main must not reach installed apps unless it matches what Ghostex signed. The two options were pinning to the release's tag or signing with the component key; signing keeps skills updatable between releases, which the 2026-09-10 decision asks for, while a tag pin would freeze them until the next release and a moved tag would still be trusted. So the downloads are no longer checked against GitHub's own listing (git blob shas, which say only that the file is what main has now): gxserver reads one signed manifest, the `agent-skills` release's `ghostex-skills.json`, whose statement names a commit and every bundled skill file's SHA-256, size and mode, signed with Ghostex's Ed25519 component signing key (the `GHOSTEX_COMPONENT_SIGNING_KEY` Actions secret, `.github/workflows/publish-agent-skills.yml`). The signature is checked against the key built into gxserver, the files are fetched at that commit (an immutable URL), and each must match its SHA-256. A manifest older than the last one accepted is refused, so an old signed set can't be replayed. With no manifest, or one that doesn't verify, nothing is downloaded and the app's bundled copy stays the source.
SEE-ALSO: tooling/release-gpui/sign-agent-skills.mjs (the statement's other half), server/src/components/mod.rs (the key).
*/
pub const AGENT_SKILLS_REMOTE_REPOSITORY: &str = "maddada/Ghostex";
/// Where the signed skills come from, as reports name it.
pub const AGENT_SKILLS_REMOTE_REF: &str = "signed release agent-skills";
/// The release (tag) the signed manifest is published under, and its file name.
const AGENT_SKILLS_MANIFEST_TAG: &str = "agent-skills";
const AGENT_SKILLS_MANIFEST_ASSET: &str = "ghostex-skills.json";
/// First line of the signed statement; must equal `SKILLS_SIGNING_DOMAIN` in
/// tooling/release-gpui/sign-agent-skills.mjs.
const AGENT_SKILLS_SIGNING_DOMAIN: &str = "ghostex-skills-v1";
/// The newest manifest accepted so far (`issuedAtMs`), in the cache folder.
const AGENT_SKILLS_ACCEPTED_FILE: &str = "accepted-issued-at";
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
    /// Lowercase hex SHA-256 of the file, from the signed statement.
    sha256: String,
    size: u64,
}

/// The bundled skills Ghostex signed: one file list per bundled skill name, with the SHA-256
/// every download is checked against, at the commit the files are fetched from.
pub struct RemoteSkillsCatalog {
    files_by_skill: BTreeMap<String, Vec<RemoteSkillFile>>,
    commit: String,
}

impl RemoteSkillsCatalog {
    /// Reads and checks the signed manifest. `cache_root` keeps the newest one accepted, so an
    /// older signed manifest is refused.
    pub fn fetch(cache_root: &Path) -> Result<Self, String> {
        let url = debug_override("GHOSTEX_AGENT_SKILLS_MANIFEST_URL").unwrap_or_else(|| {
            format!(
                "https://github.com/{AGENT_SKILLS_REMOTE_REPOSITORY}/releases/download/{AGENT_SKILLS_MANIFEST_TAG}/{AGENT_SKILLS_MANIFEST_ASSET}"
            )
        });
        let response = crate::http_agent::agent()
            .get(&url)
            .header("User-Agent", AGENT_SKILLS_USER_AGENT)
            .config()
            .timeout_global(Some(AGENT_SKILLS_REQUEST_TIMEOUT))
            .build()
            .call()
            .map_err(|error| {
                format!(
                    "The signed skill list couldn't be read: {}",
                    summarize_ureq_error(error)
                )
            })?;
        let value: Value = crate::http_agent::read_json(response)
            .map_err(|error| format!("The signed skill list was not JSON: {error}"))?;
        let catalog = Self::from_signed(&value, signing_public_key().as_ref())?;
        catalog.accept_newer(cache_root, &value)?;
        Ok(catalog)
    }

    /// The catalog of a signed manifest `{ statement, signature }`, checked against `key`.
    fn from_signed(value: &Value, key: Option<&[u8; 32]>) -> Result<Self, String> {
        use base64::Engine as _;
        let statement = value
            .get("statement")
            .and_then(Value::as_str)
            .ok_or("The signed skill list has no statement.")?;
        let signature = value
            .get("signature")
            .and_then(Value::as_str)
            .and_then(|text| {
                base64::engine::general_purpose::STANDARD
                    .decode(text.trim())
                    .ok()
            })
            .ok_or("The signed skill list has no readable signature.")?;
        let key = key.ok_or("Ghostex's signing key is unreadable.")?;
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
            .verify(statement.as_bytes(), &signature)
            .map_err(|_| {
                "The skill list's signature doesn't match Ghostex's, so no skill was downloaded."
                    .to_string()
            })?;
        Self::parse_statement(statement)
    }

    /// Parses a statement whose signature was checked:
    /// `ghostex-skills-v1\n<commit>\n<issuedAtMs>\n<skill>/<path> <sha256> <size> <mode>…`.
    fn parse_statement(statement: &str) -> Result<Self, String> {
        let mut lines = statement.split('\n');
        if lines.next() != Some(AGENT_SKILLS_SIGNING_DOMAIN) {
            return Err("The signed skill list is in a format this Ghostex doesn't read.".into());
        }
        let commit = lines
            .next()
            .filter(|commit| {
                commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .ok_or("The signed skill list names no commit.")?
            .to_ascii_lowercase();
        lines
            .next()
            .and_then(|issued| issued.parse::<u64>().ok())
            .ok_or("The signed skill list has no date.")?;
        let mut files_by_skill: BTreeMap<String, Vec<RemoteSkillFile>> = BTreeMap::new();
        for line in lines {
            let mut fields = line.split(' ');
            let (Some(path), Some(sha256), Some(size), Some(mode), None) = (
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
            ) else {
                return Err("The signed skill list has a line it can't read.".into());
            };
            let Some((skill_name, relative_path)) = path.split_once('/') else {
                continue;
            };
            if !GHOSTEX_AGENT_SKILL_NAMES.contains(&skill_name)
                || !is_safe_relative_path(relative_path)
                || sha256.len() != 64
                || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                continue;
            }
            let size = size
                .parse::<u64>()
                .map_err(|_| "The signed skill list has a size it can't read.".to_string())?;
            files_by_skill
                .entry(skill_name.to_string())
                .or_default()
                .push(RemoteSkillFile {
                    executable: mode == "100755",
                    relative_path: relative_path.to_string(),
                    sha256: sha256.to_ascii_lowercase(),
                    size,
                });
        }
        files_by_skill.retain(|_, files| files.iter().any(|file| file.relative_path == "SKILL.md"));
        Ok(Self {
            files_by_skill,
            commit,
        })
    }

    /// Refuses a manifest older than the newest one accepted here, and remembers this one.
    fn accept_newer(&self, cache_root: &Path, value: &Value) -> Result<(), String> {
        let issued = value
            .get("statement")
            .and_then(Value::as_str)
            .and_then(|statement| statement.split('\n').nth(2))
            .and_then(|issued| issued.parse::<u64>().ok())
            .unwrap_or(0);
        let marker = cache_root.join(AGENT_SKILLS_ACCEPTED_FILE);
        let accepted = fs::read_to_string(&marker)
            .ok()
            .and_then(|text| text.trim().parse::<u64>().ok())
            .unwrap_or(0);
        if issued < accepted {
            return Err(
                "The signed skill list is older than one already used, so it was ignored.".into(),
            );
        }
        if issued > accepted {
            let _ = fs::create_dir_all(cache_root);
            let _ = fs::write(&marker, issued.to_string());
        }
        Ok(())
    }

    /// The commit the signed files are fetched from.
    pub fn commit(&self) -> &str {
        &self.commit
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
        if collect_local_sha256s(dir, dir, &mut local).is_err() {
            return false;
        }
        local.len() == files.len()
            && files.iter().all(|file| {
                local
                    .get(&file.relative_path)
                    .is_some_and(|sha| sha == &file.sha256)
            })
    }

    /// Download the skill into `<cache_root>/<skill>` and return that folder.
    /// The files are staged beside the destination and renamed into place only
    /// after every one of them matched the SHA-256 Ghostex signed. A cache folder
    /// that already matches is reused without any request.
    pub fn download_skill(&self, skill_name: &str, cache_root: &Path) -> Result<PathBuf, String> {
        let Some(files) = self.files_by_skill.get(skill_name) else {
            return Err(format!("{skill_name} is not in the signed skill list."));
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
            .try_for_each(|file| stage_remote_file(skill_name, file, &self.commit, &staging));
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
    commit: &str,
    staging: &Path,
) -> Result<(), String> {
    if file.size > AGENT_SKILLS_MAX_FILE_BYTES {
        return Err(format!(
            "{skill_name}/{} is larger than the {} MB skill file limit.",
            file.relative_path,
            AGENT_SKILLS_MAX_FILE_BYTES / (1024 * 1024)
        ));
    }
    let bytes = fetch_raw_skill_file(skill_name, &file.relative_path, commit)?;
    if sha256_hex(&bytes) != file.sha256 {
        return Err(format!(
            "{skill_name}/{} doesn't match what Ghostex signed, so the skill wasn't updated.",
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

fn fetch_raw_skill_file(
    skill_name: &str,
    relative_path: &str,
    commit: &str,
) -> Result<Vec<u8>, String> {
    let base = debug_override("GHOSTEX_AGENT_SKILLS_FILES_URL").unwrap_or_else(|| {
        format!("https://raw.githubusercontent.com/{AGENT_SKILLS_REMOTE_REPOSITORY}")
    });
    let url = format!(
        "{}/{commit}/{AGENT_SKILLS_REMOTE_ROOT}/{skill_name}/{relative_path}",
        base.trim_end_matches('/')
    );
    let response = crate::http_agent::agent()
        .get(&url)
        .header("User-Agent", AGENT_SKILLS_USER_AGENT)
        .config()
        .timeout_global(Some(AGENT_SKILLS_REQUEST_TIMEOUT))
        .build()
        .call()
        .map_err(|error| {
            format!(
                "Download of {skill_name}/{relative_path} failed: {}",
                summarize_ureq_error(error)
            )
        })?;
    let mut bytes = Vec::new();
    response
        .into_body()
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
        ureq::Error::StatusCode(status) => format!("HTTP {status}"),
        error => error.to_string(),
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

/// A URL a debug build may read from the environment instead (a local copy for testing); a
/// release build always uses GitHub. The signature and SHA-256 checks are the same either way.
fn debug_override(name: &str) -> Option<String> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The key the signed skill list is checked with: Ghostex's component signing key (a debug
/// build reads `GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY` for tests, as components do).
fn signing_public_key() -> Option<[u8; 32]> {
    crate::components::signing_public_key()
}

fn collect_local_sha256s(
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
            collect_local_sha256s(root, &path, out)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| std::io::Error::other("skill file outside its folder"))?
                .components()
                .map(|component| component.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            out.insert(relative, sha256_hex(&fs::read(&path)?));
        }
    }
    Ok(())
}

/// Every global skill root a bundled skill may have been installed into: the
/// canonical universal folder plus each agent's own folder, deduplicated.
pub(crate) fn installed_skill_roots(home_dir: &Path) -> Vec<PathBuf> {
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
            let updated_count = result["updated"].as_array().map_or(0, Vec::len)
                + result["linked"].as_array().map_or(0, Vec::len)
                + result["removed"].as_array().map_or(0, Vec::len);
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
/// ones, then turn the real copies older builds left in agents' own folders into
/// links to the `~/.agents/skills` copy, or remove them from the folders of
/// agents that read `~/.agents/skills` themselves. Returns the refresh report, or the reason
/// the listing could not be fetched (offline, rate limited, downloads disabled).
///
/// CDXC:AgentSkills 2026-10-14 WHY:
/// Builds before 2026-09-16 copied each bundled skill into every agent's folder, and this pass
/// then refreshed each copy in place, so one skill lived as up to fifteen separate folders that
/// drifted apart whenever one was edited. A bundled copy outside `~/.agents/skills` is now
/// replaced by a link, the shape fresh installs make, so editing the one copy reaches every agent.
pub fn refresh_installed_bundled_skills(paths: &GxserverPaths) -> Result<Value, String> {
    if !remote_skills_enabled() {
        return Err(format!(
            "GitHub skill downloads are disabled ({AGENT_SKILLS_REMOTE_OPT_OUT_ENV}=off)."
        ));
    }
    let cache_root = remote_skills_cache_root(paths);
    let catalog = RemoteSkillsCatalog::fetch(&cache_root)?;
    let home_dir = paths.agent_config_home_dir();
    let roots = installed_skill_roots(home_dir);
    let canonical_root = home_dir.join(CANONICAL_AGENT_SKILLS_DIR);
    let mut refreshed_names = HashSet::new();
    let mut updated = Vec::new();
    let mut linked = Vec::new();
    let mut removed = Vec::new();
    let mut failed = Vec::new();
    // Agents that load `~/.agents/skills` themselves need no copy and no link in their own folder.
    let native_reader_roots: HashSet<PathBuf> = GHOSTEX_SKILLS_CLI_AGENT_IDS
        .iter()
        .filter(|agent_id| agent_reads_canonical_skills(home_dir, agent_id))
        .filter_map(|agent_id| agent_skill_global_dir(home_dir, agent_id))
        .filter(|root| *root != canonical_root)
        .collect();
    for skill_name in GHOSTEX_AGENT_SKILL_NAMES {
        if !refreshed_names.insert(*skill_name) || !catalog.has_skill(skill_name) {
            continue;
        }
        let canonical_dir = canonical_root.join(skill_name);
        let is_real_dir = |dir: &Path| {
            fs::symlink_metadata(dir)
                .map(|metadata| !metadata.file_type().is_symlink() && metadata.is_dir())
                .unwrap_or(false)
        };
        let agent_copies: Vec<PathBuf> = roots
            .iter()
            .filter(|root| **root != canonical_root)
            .map(|root| root.join(skill_name))
            .filter(|dir| is_real_dir(dir))
            .collect();
        let canonical_missing = fs::symlink_metadata(&canonical_dir).is_err();
        let canonical_stale = (is_real_dir(&canonical_dir)
            && !catalog.local_dir_matches(skill_name, &canonical_dir))
            || (canonical_missing && !agent_copies.is_empty());
        if canonical_stale {
            let source_dir = match catalog.download_skill(skill_name, &cache_root) {
                Ok(dir) => dir,
                Err(error) => {
                    failed.push(json!({ "error": error, "skill": skill_name }));
                    continue;
                }
            };
            match copy_skill_directory_into_place(&source_dir, &canonical_dir) {
                Ok(true) => updated.push(path_string(&canonical_dir)),
                Ok(false) => {}
                Err(error) => {
                    failed.push(json!({
                        "error": error.message,
                        "path": path_string(&canonical_dir),
                        "skill": skill_name,
                    }));
                    continue;
                }
            }
        }
        if !canonical_dir.join("SKILL.md").is_file() {
            continue;
        }
        for destination in agent_copies {
            let reads_canonical = destination
                .parent()
                .is_some_and(|root| native_reader_roots.contains(root));
            if reads_canonical {
                match fs::remove_dir_all(&destination) {
                    Ok(()) => removed.push(path_string(&destination)),
                    Err(error) => failed.push(json!({
                        "error": error.to_string(),
                        "path": path_string(&destination),
                        "skill": skill_name,
                    })),
                }
                continue;
            }
            match link_skill_into_agent_root(&canonical_dir, &destination) {
                Ok(true) => linked.push(path_string(&destination)),
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
        "linked": linked,
        "ref": AGENT_SKILLS_REMOTE_REF,
        "removed": removed,
        "repository": AGENT_SKILLS_REMOTE_REPOSITORY,
        "commit": catalog.commit(),
        "type": "agentSkillRefresh",
        "updated": updated,
    }))
}

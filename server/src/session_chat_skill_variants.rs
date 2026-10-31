use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

use sha2::{Digest, Sha256};

use crate::session_chat_skills::SessionChatSkill;

const SKILL_DIGEST_CACHE_LIMIT: usize = 4096;
const CLAUDE_PLUGIN_ORPHANED_MARKER: &str = ".orphaned_at";

struct CachedSkillDigest {
    len: u64,
    modified: Option<SystemTime>,
    digest: [u8; 32],
}

static SKILL_DIGEST_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedSkillDigest>>> = OnceLock::new();

/// CDXC:AgentSkills 2026-09-19 DECISION:
/// User: "i think we 100% should show each skill only once in the list (check using sha or 256 that the skill file is different) if it's different show 1 of each variation of the skill".
/// Candidates are grouped by name and identified by the sha256 of their SKILL.md bytes; identical copies collapse to the copy the agent most likely loads, and each distinct variation keeps one row labelled with its source.
pub(crate) fn collapse_session_chat_skill_variants(
    skills: Vec<SessionChatSkill>,
    installed_plugin_roots: &[PathBuf],
) -> Vec<SessionChatSkill> {
    let mut groups = BTreeMap::<(String, String), Vec<SessionChatSkill>>::new();
    for skill in skills {
        groups
            .entry((skill.name.to_ascii_lowercase(), skill.name.clone()))
            .or_default()
            .push(skill);
    }

    let mut collapsed = Vec::new();
    for (_, mut candidates) in groups {
        if candidates.len() == 1 {
            collapsed.append(&mut candidates);
            continue;
        }
        candidates.sort_by_cached_key(|skill| skill_preference_key(skill, installed_plugin_roots));
        let mut seen_contents = HashSet::<Vec<u8>>::new();
        let mut variations = Vec::new();
        for skill in candidates {
            let identity = skill_file_digest(&skill.skill_file_path)
                .map(|digest| digest.to_vec())
                .unwrap_or_else(|| skill.skill_file_path.to_string_lossy().as_bytes().to_vec());
            if seen_contents.insert(identity) {
                variations.push(skill);
            }
        }
        if variations.len() > 1 {
            for skill in &mut variations {
                skill.variant_label = Some(skill_variant_label(skill, installed_plugin_roots));
            }
        }
        collapsed.append(&mut variations);
    }
    collapsed
}

/// CDXC:AgentSkills 2026-09-19 WHY:
/// The representative of identical copies is the one the agent itself would load: project over user over plugin cache, the installed plugin version over superseded ones (Claude marks those with `.orphaned_at`), a direct child of a skills root over a nested copy, then the newest SKILL.md.
fn skill_preference_key(
    skill: &SessionChatSkill,
    installed_plugin_roots: &[PathBuf],
) -> (u8, u8, usize, Reverse<Option<SystemTime>>, PathBuf) {
    let source_rank = match skill.source_kind {
        "repository" => 0,
        "global" => 1,
        _ => 2,
    };
    let plugin_rank = if skill.source_kind != "pluginCache"
        || is_installed_plugin_skill(skill, installed_plugin_roots)
    {
        0
    } else if plugin_version_directory(skill)
        .is_some_and(|directory| directory.join(CLAUDE_PLUGIN_ORPHANED_MARKER).exists())
    {
        2
    } else {
        1
    };
    let depth = skill
        .directory_path
        .strip_prefix(&skill.root_path)
        .map(|relative| relative.components().count())
        .unwrap_or(usize::MAX);
    let modified = fs::metadata(&skill.skill_file_path)
        .and_then(|metadata| metadata.modified())
        .ok();
    (
        source_rank,
        plugin_rank,
        depth,
        Reverse(modified),
        skill.directory_path.clone(),
    )
}

fn is_installed_plugin_skill(skill: &SessionChatSkill, installed_plugin_roots: &[PathBuf]) -> bool {
    installed_plugin_roots
        .iter()
        .any(|root| skill.directory_path.starts_with(root))
}

/// Plugin caches are laid out as `<marketplace>/<plugin>/<version>/…` under the cache root.
fn plugin_cache_parts(skill: &SessionChatSkill) -> Option<[String; 3]> {
    let relative = skill.directory_path.strip_prefix(&skill.root_path).ok()?;
    let mut parts = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned());
    Some([parts.next()?, parts.next()?, parts.next()?])
}

fn plugin_version_directory(skill: &SessionChatSkill) -> Option<PathBuf> {
    let [marketplace, plugin, version] = plugin_cache_parts(skill)?;
    Some(skill.root_path.join(marketplace).join(plugin).join(version))
}

fn skill_variant_label(skill: &SessionChatSkill, installed_plugin_roots: &[PathBuf]) -> String {
    let provider = || {
        skill
            .root_path
            .file_name()
            .filter(|name| *name == "skills")
            .and_then(|_| skill.root_path.parent())
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
    };
    match skill.source_kind {
        "repository" => match provider() {
            Some(provider) => format!("project {provider}"),
            None => "project".to_string(),
        },
        "pluginCache" => match plugin_cache_parts(skill) {
            Some([_, plugin, version])
                if is_installed_plugin_skill(skill, installed_plugin_roots) =>
            {
                format!("plugin {plugin} {version} (installed)")
            }
            Some([_, plugin, version]) => format!("plugin {plugin} {version}"),
            None => "plugin".to_string(),
        },
        _ => {
            let mut relative = skill
                .directory_path
                .strip_prefix(&skill.root_path)
                .ok()
                .into_iter()
                .flat_map(Path::components)
                .map(|component| component.as_os_str().to_string_lossy().into_owned());
            match (relative.next().as_deref(), relative.next()) {
                (Some("synced"), Some(bucket)) if relative.next().is_some() => {
                    format!("synced {}", bucket.chars().take(8).collect::<String>())
                }
                _ => match provider() {
                    Some(provider) => format!("user {provider}"),
                    None => "user".to_string(),
                },
            }
        }
    }
}

/// SKILL.md digests are cached by path and revalidated by size and mtime, so reopening the `$` picker rereads only files that changed.
fn skill_file_digest(path: &Path) -> Option<[u8; 32]> {
    let metadata = fs::metadata(path).ok()?;
    let len = metadata.len();
    let modified = metadata.modified().ok();
    let cache = SKILL_DIGEST_CACHE.get_or_init(Default::default);
    if let Some(entry) = cache.lock().ok()?.get(path) {
        if entry.len == len && entry.modified == modified {
            return Some(entry.digest);
        }
    }
    let digest: [u8; 32] = Sha256::digest(fs::read(path).ok()?).into();
    let mut cache = cache.lock().ok()?;
    if cache.len() >= SKILL_DIGEST_CACHE_LIMIT {
        cache.clear();
    }
    cache.insert(
        path.to_path_buf(),
        CachedSkillDigest {
            len,
            modified,
            digest,
        },
    );
    Some(digest)
}

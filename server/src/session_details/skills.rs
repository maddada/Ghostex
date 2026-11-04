//! Where each skill a Claude session lists comes from, so the panel opens that skill and not
//! another: Claude names a skill by its `name:` (which can differ from its folder), a plugin's
//! skills as `plugin:skill`, and lists skills built into Claude Code that have no file at all.
//!
//! CDXC:SessionDetails 2026-10-25 WHY:
//! User: "i still don't see the correct skill appear in the agents hub when i click on a skill in the sidebar". The panel sent only the name Claude lists and Agents Hub looked for a row of that name, but Agents Hub lists only the skill folders under the home folder, by folder name: a project skill (`<project>/.claude/skills`), a plugin skill and a built-in skill are never there, and a folder whose SKILL.md names it differently (`franky47-audit-github-actions` is `audit-github-actions`) matched nothing. Each skill now carries its SKILL.md and where it was found, and the panel opens a home skill in Agents Hub by that file, a project or plugin skill's SKILL.md itself, and nothing for a built-in one.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long one folder's index of skills is reused while the panel keeps reading.
const FRESH_FOR: Duration = Duration::from_secs(60);

/// Where one skill lives: `"user"` (a home skills folder, which Agents Hub lists), `"project"`
/// or `"plugin"` (a SKILL.md Agents Hub does not list), or `"builtin"` (no file).
#[derive(Clone)]
pub(super) struct SkillSource {
    pub scope: &'static str,
    pub path: Option<String>,
}

type Index = HashMap<String, SkillSource>;

fn cache() -> &'static Mutex<HashMap<PathBuf, (Instant, Index)>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (Instant, Index)>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// The `name:` of a SKILL.md's front matter.
fn front_matter_name(skill_file: &Path) -> Option<String> {
    let text = fs::read_to_string(skill_file).ok()?;
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    lines
        .take_while(|line| line.trim() != "---")
        .find_map(|line| line.strip_prefix("name:"))
        .map(|name| name.trim().trim_matches(['"', '\'']).to_string())
        .filter(|name| !name.is_empty())
}

/// Every skill in `root` (`<root>/<folder>/SKILL.md`), under the name Claude gives it and under
/// its folder name, with its real SKILL.md path.
fn index_root(root: &Path, scope: &'static str, prefix: &str, into: &mut Index) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let skill_file = entry.path().join("SKILL.md");
        if !skill_file.is_file() {
            continue;
        }
        let path = fs::canonicalize(&skill_file)
            .unwrap_or(skill_file.clone())
            .to_string_lossy()
            .into_owned();
        let source = SkillSource {
            scope,
            path: Some(path),
        };
        let folder = entry.file_name().to_string_lossy().into_owned();
        for name in front_matter_name(&skill_file)
            .into_iter()
            .chain(std::iter::once(folder))
        {
            into.entry(format!("{prefix}{name}"))
                .or_insert_with(|| source.clone());
        }
    }
}

/// The plugins Claude has installed: `(plugin name, install folder)`.
fn installed_plugins(claude_home: &Path) -> Vec<(String, PathBuf)> {
    let Ok(text) = fs::read_to_string(claude_home.join("plugins/installed_plugins.json")) else {
        return Vec::new();
    };
    let Ok(record) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    record["plugins"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, installs)| {
            let name = key.split('@').next()?.to_string();
            let path = installs
                .as_array()?
                .iter()
                .find_map(|install| install["installPath"].as_str().map(PathBuf::from))?;
            Some((name, path))
        })
        .collect()
}

fn build_index(home: &Path, folder: Option<&Path>) -> Index {
    let claude_home = home.join(".claude");
    let mut index = Index::new();
    // Claude reads the project's skills from the session's folder and the folders above it.
    if let Some(folder) = folder {
        for dir in folder.ancestors().take_while(|dir| *dir != home) {
            index_root(&dir.join(".claude/skills"), "project", "", &mut index);
        }
    }
    index_root(&claude_home.join("skills"), "user", "", &mut index);
    for (plugin, install) in installed_plugins(&claude_home) {
        index_root(
            &install.join("skills"),
            "plugin",
            &format!("{plugin}:"),
            &mut index,
        );
    }
    index
}

/// Where each of `names` comes from, for a session in `folder`. Blocking; cached a minute.
pub(super) fn skill_sources<'a>(
    home: &Path,
    folder: Option<&Path>,
    names: impl Iterator<Item = &'a str>,
) -> HashMap<String, SkillSource> {
    let key = folder.map_or_else(|| home.to_path_buf(), Path::to_path_buf);
    let cached = cache().lock().ok().and_then(|cache| {
        cache
            .get(&key)
            .filter(|(at, _)| at.elapsed() < FRESH_FOR)
            .map(|(_, index)| index.clone())
    });
    let index = cached.unwrap_or_else(|| {
        let index = build_index(home, folder);
        if let Ok(mut cache) = cache().lock() {
            if cache.len() > 32 {
                cache.clear();
            }
            cache.insert(key, (Instant::now(), index.clone()));
        }
        index
    });
    names
        .map(|name| {
            let source = index.get(name).cloned().unwrap_or(SkillSource {
                scope: "builtin",
                path: None,
            });
            (name.to_string(), source)
        })
        .collect()
}

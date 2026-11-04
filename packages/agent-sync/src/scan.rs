//! Scan HOME and describe, per agent, how far it is from the source of truth.
//! The report is metadata only: no file bodies, no hashes beyond the per-skill
//! identical-copy check.

use crate::catalog::{agent_catalog, source_root, AgentKind, AgentSpec, InstructionKind};
use crate::fsx;
use crate::pointer;
pub use crate::pointer::InstructionState;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    /// Everything this agent supports points at the source.
    Linked,
    /// At least one skill, instruction, hook, or lock item needs an operation.
    Attention,
    /// Its config folder does not exist on this computer.
    NotInstalled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillDirState {
    Missing,
    RealDir,
    /// The whole folder is a symlink to `~/.agents/skills`; converted to per-skill links.
    WholeFolderLink,
    /// A symlink to somewhere else; left alone and reported.
    OtherLink,
    NotADir,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillEntryState {
    /// A link to the source skill of the same name.
    Linked,
    /// A link to something that is not the source; kept and reported.
    LinkedElsewhere,
    /// A link whose target is gone.
    Dangling,
    /// A real folder with the same content as the source skill; replaced by a link.
    CopyIdentical,
    /// A real folder that differs from the source skill; kept and reported.
    CopyDrifted,
    /// A real folder with no counterpart in the source.
    OnlyHere,
    /// A source skill with nothing in this agent folder yet.
    Missing,
    /// Reachable through a whole-folder link; becomes a per-skill link after conversion.
    ViaWholeFolder,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub name: String,
    pub state: SkillEntryState,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    /// Set when the source skill is one Ghostex ships (`ghostex-*`).
    pub ghostex_bundled: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCounts {
    pub linked: usize,
    pub linked_elsewhere: usize,
    pub dangling: usize,
    pub copies_identical: usize,
    pub copies_drifted: usize,
    pub only_here: usize,
    pub missing: usize,
    pub via_whole_folder: usize,
    /// Links into `~/.agents/skills` inside the folder of an agent that reads that folder
    /// itself (a whole-folder link counts as one); they make it load every skill twice.
    pub redundant_links: usize,
    /// Leftover `<name>.pre-sync-<stamp>.bak` folders, which agents read as more skills.
    pub backups: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsReport {
    pub dir: String,
    pub dir_state: SkillDirState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir_link_target: Option<String>,
    pub entries: Vec<SkillEntry>,
    /// Display paths of the leftover backup folders counted in `counts.backups`.
    pub backups: Vec<String>,
    pub counts: SkillCounts,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionReport {
    pub path: String,
    pub kind: InstructionKind,
    pub state: InstructionState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HooksState {
    Linked,
    Missing,
    RealDir,
    OtherLink,
    /// `~/.agents/hooks` does not exist, so there is nothing to link; the agent's own folder stays as it is.
    NothingShared,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksReport {
    pub dir: String,
    pub state: HooksState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LockLinkState {
    Linked,
    Missing,
    RealFile,
    OtherLink,
    /// `~/.agents/.skill-lock.json` does not exist, so there is nothing to link.
    NothingShared,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockLinkReport {
    pub path: String,
    pub state: LockLinkState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReport {
    pub id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub kind: AgentKind,
    pub root: String,
    pub detected: bool,
    pub universal: bool,
    pub status: AgentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillsReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<InstructionReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<HooksReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock: Option<LockLinkReport>,
    #[serde(skip)]
    pub spec: Option<AgentSpec>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSkill {
    pub name: String,
    pub path: String,
    pub is_symlink: bool,
    /// A symlink in the source that cannot be resolved (including one pointing at itself).
    pub broken: bool,
    pub ghostex_bundled: bool,
    pub in_lock: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockInfo {
    pub path: String,
    pub exists: bool,
    pub entry_count: usize,
    pub stale_entries: Vec<String>,
    pub untracked_skills: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceGitState {
    None,
    NoCommits,
    Committed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub path: String,
    pub exists: bool,
    pub skills_dir: String,
    pub skills: Vec<SourceSkill>,
    pub main_md_exists: bool,
    pub md_files: Vec<String>,
    pub hooks_dir: String,
    pub hooks_dir_exists: bool,
    pub hook_script_count: usize,
    pub lock: LockInfo,
    pub git: SourceGitState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProblemKind {
    DanglingLinks,
    SourceBrokenLinks,
    StaleLockEntries,
    UntrackedSkills,
    CopiedFolders,
    WholeFolderLinks,
    MissingPointers,
    RedundantLinks,
    BackupFolders,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub kind: ProblemKind,
    pub count: usize,
    pub title: String,
    pub detail: String,
    pub items: Vec<String>,
    /// Informational rows have no fix.
    pub fixable: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummary {
    pub agents_detected: usize,
    pub agents_linked: usize,
    pub agents_attention: usize,
    pub agents_not_installed: usize,
    pub dangling_links: usize,
    pub stale_lock_entries: usize,
    pub copied_skill_folders: usize,
    pub whole_folder_links: usize,
    pub missing_pointers: usize,
    pub redundant_links: usize,
    pub backup_folders: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub generated_at: String,
    pub home: String,
    pub source: SourceInfo,
    pub agents: Vec<AgentReport>,
    pub problems: Vec<Problem>,
    pub summary: ReportSummary,
}

/// Skill folder names Ghostex ships and installs itself.
pub fn is_ghostex_bundled_skill(name: &str) -> bool {
    name.starts_with("ghostex-")
}

fn is_skill_dir(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

fn scan_source(home: &Path) -> SourceInfo {
    let root = source_root(home);
    let skills_dir = root.join("skills");
    let lock_path = root.join(".skill-lock.json");
    let lock_keys: Option<BTreeSet<String>> = fs::read_to_string(&lock_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| {
            value
                .get("skills")
                .and_then(|skills| skills.as_object().cloned())
        })
        .map(|skills| skills.keys().cloned().collect());
    let mut skills = Vec::new();
    for name in fsx::child_names(&skills_dir) {
        if name.starts_with('.') {
            continue;
        }
        let path = skills_dir.join(&name);
        let is_symlink = fsx::is_symlink(&path);
        let broken = is_symlink && fs::metadata(&path).is_err();
        if !broken && !is_skill_dir(&path) {
            continue;
        }
        skills.push(SourceSkill {
            in_lock: lock_keys
                .as_ref()
                .map(|keys| keys.contains(&name))
                .unwrap_or(false),
            ghostex_bundled: is_ghostex_bundled_skill(&name),
            name,
            path: fsx::display_path(&path, home),
            is_symlink,
            broken,
        });
    }
    let present: BTreeSet<&str> = skills
        .iter()
        .filter(|s| !s.broken)
        .map(|s| s.name.as_str())
        .collect();
    let stale_entries = lock_keys
        .as_ref()
        .map(|keys| {
            keys.iter()
                .filter(|key| !present.contains(key.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let untracked_skills = skills
        .iter()
        .filter(|s| !s.broken && !s.in_lock && lock_keys.is_some())
        .map(|s| s.name.clone())
        .collect();
    let md_files: Vec<String> = fsx::child_names(&root)
        .into_iter()
        .filter(|name| name.ends_with(".md") && root.join(name).is_file())
        .collect();
    let hooks_dir = root.join("hooks");
    let hook_script_count = fsx::child_names(&hooks_dir)
        .into_iter()
        .filter(|name| {
            let path = hooks_dir.join(name);
            path.is_file()
                && (name.ends_with(".sh")
                    || name.ends_with(".ts")
                    || name.ends_with(".js")
                    || name.ends_with(".py"))
        })
        .count();
    let git_dir = root.join(".git");
    let git = if !git_dir.exists() {
        SourceGitState::None
    } else if git_dir.join("packed-refs").exists()
        || fs::read_dir(git_dir.join("refs").join("heads"))
            .map(|entries| entries.filter_map(Result::ok).next().is_some())
            .unwrap_or(false)
    {
        SourceGitState::Committed
    } else {
        SourceGitState::NoCommits
    };
    SourceInfo {
        path: fsx::display_path(&root, home),
        exists: root.is_dir(),
        skills_dir: fsx::display_path(&skills_dir, home),
        main_md_exists: root.join("main.md").is_file(),
        md_files,
        hooks_dir: fsx::display_path(&hooks_dir, home),
        hooks_dir_exists: hooks_dir.is_dir(),
        hook_script_count,
        lock: LockInfo {
            path: fsx::display_path(&lock_path, home),
            exists: lock_keys.is_some(),
            entry_count: lock_keys.as_ref().map(BTreeSet::len).unwrap_or(0),
            stale_entries,
            untracked_skills,
        },
        git,
        skills,
    }
}

/// Agent Sync's own backup name, `<name>.pre-sync-<stamp>.bak`.
pub fn is_sync_backup_name(name: &str) -> bool {
    name.contains(".pre-sync-") && name.ends_with(".bak")
}

/// `scan_skill_entries` plus the two checks that need the agent's kind: links an
/// agent reading `~/.agents/skills` does not need, and leftover backup folders.
fn scan_skills(home: &Path, spec: &AgentSpec, source: &SourceInfo) -> Option<SkillsReport> {
    let mut report = scan_skill_entries(home, spec, source)?;
    let dir = spec.skills_dir.as_ref()?;
    if report.dir_state == SkillDirState::RealDir {
        report.backups = fsx::child_names(dir)
            .into_iter()
            .filter(|name| is_sync_backup_name(name))
            .map(|name| fsx::display_path(&dir.join(name), home))
            .collect();
    }
    report.counts.backups = report.backups.len();
    if spec.universal {
        report.counts.redundant_links = match report.dir_state {
            SkillDirState::WholeFolderLink => 1,
            _ => report.counts.linked,
        };
    }
    Some(report)
}

fn scan_skill_entries(home: &Path, spec: &AgentSpec, source: &SourceInfo) -> Option<SkillsReport> {
    let dir = spec.skills_dir.as_ref()?;
    let source_skills_dir = source_root(home).join("skills");
    let display_dir = fsx::display_path(dir, home);
    let metadata = fs::symlink_metadata(dir).ok();
    let Some(metadata) = metadata else {
        return Some(with_counts(SkillsReport {
            dir: display_dir,
            dir_state: SkillDirState::Missing,
            dir_link_target: None,
            entries: missing_entries(home, dir, source, spec.universal),
            backups: Vec::new(),
            counts: SkillCounts::default(),
        }));
    };
    let usable_source: Vec<&SourceSkill> = source.skills.iter().filter(|s| !s.broken).collect();
    if metadata.file_type().is_symlink() {
        let target = fsx::absolute_link_target(dir);
        let display_target = target.as_ref().map(|t| fsx::display_path(t, home));
        if fsx::same_entry(dir, &source_skills_dir) {
            let entries = usable_source
                .iter()
                .map(|skill| SkillEntry {
                    name: skill.name.clone(),
                    state: SkillEntryState::ViaWholeFolder,
                    path: format!("{display_dir}/{}", skill.name),
                    link_target: None,
                    ghostex_bundled: skill.ghostex_bundled,
                })
                .collect();
            return Some(with_counts(SkillsReport {
                dir: display_dir,
                dir_state: SkillDirState::WholeFolderLink,
                dir_link_target: display_target,
                entries,
                backups: Vec::new(),
                counts: SkillCounts::default(),
            }));
        }
        return Some(with_counts(SkillsReport {
            dir: display_dir,
            dir_state: SkillDirState::OtherLink,
            dir_link_target: display_target,
            entries: Vec::new(),
            backups: Vec::new(),
            counts: SkillCounts::default(),
        }));
    }
    if !metadata.is_dir() {
        return Some(with_counts(SkillsReport {
            dir: display_dir,
            dir_state: SkillDirState::NotADir,
            dir_link_target: None,
            entries: Vec::new(),
            backups: Vec::new(),
            counts: SkillCounts::default(),
        }));
    }
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for name in fsx::child_names(dir) {
        if name.starts_with('.') || name.ends_with(".bak") {
            continue;
        }
        let path = dir.join(&name);
        let display = fsx::display_path(&path, home);
        let source_match = usable_source.iter().find(|s| s.name == name);
        let bundled = is_ghostex_bundled_skill(&name);
        if fsx::is_symlink(&path) {
            let link_target = fsx::absolute_link_target(&path).map(|t| fsx::display_path(&t, home));
            let state = if fs::metadata(&path).is_err() {
                SkillEntryState::Dangling
            } else if fsx::same_entry(&path, &source_skills_dir.join(&name)) {
                SkillEntryState::Linked
            } else {
                SkillEntryState::LinkedElsewhere
            };
            seen.insert(name.clone());
            entries.push(SkillEntry {
                name,
                state,
                path: display,
                link_target,
                ghostex_bundled: bundled,
            });
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        seen.insert(name.clone());
        let state = match source_match {
            Some(skill) => {
                let source_path = source_skills_dir.join(&skill.name);
                if fsx::dir_content_hash(&path).is_some()
                    && fsx::dir_content_hash(&path) == fsx::dir_content_hash(&source_path)
                {
                    SkillEntryState::CopyIdentical
                } else {
                    SkillEntryState::CopyDrifted
                }
            }
            None => SkillEntryState::OnlyHere,
        };
        entries.push(SkillEntry {
            name,
            state,
            path: display,
            link_target: None,
            ghostex_bundled: bundled,
        });
    }
    if !spec.universal {
        for skill in &usable_source {
            if !seen.contains(&skill.name) {
                entries.push(SkillEntry {
                    name: skill.name.clone(),
                    state: SkillEntryState::Missing,
                    path: format!("{display_dir}/{}", skill.name),
                    link_target: None,
                    ghostex_bundled: skill.ghostex_bundled,
                });
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Some(with_counts(SkillsReport {
        dir: display_dir,
        dir_state: SkillDirState::RealDir,
        dir_link_target: None,
        entries,
        backups: Vec::new(),
        counts: SkillCounts::default(),
    }))
}

fn missing_entries(
    home: &Path,
    dir: &Path,
    source: &SourceInfo,
    universal: bool,
) -> Vec<SkillEntry> {
    if universal {
        return Vec::new();
    }
    let display_dir = fsx::display_path(dir, home);
    source
        .skills
        .iter()
        .filter(|s| !s.broken)
        .map(|skill| SkillEntry {
            name: skill.name.clone(),
            state: SkillEntryState::Missing,
            path: format!("{display_dir}/{}", skill.name),
            link_target: None,
            ghostex_bundled: skill.ghostex_bundled,
        })
        .collect()
}

fn with_counts(mut report: SkillsReport) -> SkillsReport {
    let mut counts = SkillCounts {
        backups: report.backups.len(),
        ..SkillCounts::default()
    };
    for entry in &report.entries {
        match entry.state {
            SkillEntryState::Linked => counts.linked += 1,
            SkillEntryState::LinkedElsewhere => counts.linked_elsewhere += 1,
            SkillEntryState::Dangling => counts.dangling += 1,
            SkillEntryState::CopyIdentical => counts.copies_identical += 1,
            SkillEntryState::CopyDrifted => counts.copies_drifted += 1,
            SkillEntryState::OnlyHere => counts.only_here += 1,
            SkillEntryState::Missing => counts.missing += 1,
            SkillEntryState::ViaWholeFolder => counts.via_whole_folder += 1,
        }
    }
    report.counts = counts;
    report
}

/// CDXC:AgentSync 2026-10-21 WHY:
/// A hooks folder or lock file only counts as "to fix" when `~/.agents` has one to link it to.
/// The scan used to report `Missing`/`RealDir` without that check while the plan skipped them, so
/// a computer without `~/.agents/hooks` showed "Hook scripts: 1 to fix" for Claude Code and Codex
/// that the Sync dialog could never fix (Windows report, 2026-10-21). `NothingShared` is that case,
/// and the plan, the agent's status and the Hub's counts all read it from here.
fn scan_hooks(home: &Path, spec: &AgentSpec, source: &SourceInfo) -> Option<HooksReport> {
    let dir = spec.hooks_dir.as_ref()?;
    let source_hooks = source_root(home).join("hooks");
    let nothing_shared = !source.hooks_dir_exists;
    let Ok(metadata) = fs::symlink_metadata(dir) else {
        return Some(HooksReport {
            dir: fsx::display_path(dir, home),
            state: if nothing_shared {
                HooksState::NothingShared
            } else {
                HooksState::Missing
            },
            link_target: None,
        });
    };
    let link_target = fsx::absolute_link_target(dir).map(|t| fsx::display_path(&t, home));
    let state = if metadata.file_type().is_symlink() {
        if fsx::same_entry(dir, &source_hooks) {
            HooksState::Linked
        } else {
            HooksState::OtherLink
        }
    } else if nothing_shared {
        HooksState::NothingShared
    } else {
        HooksState::RealDir
    };
    Some(HooksReport {
        dir: fsx::display_path(dir, home),
        state,
        link_target,
    })
}

/// On Windows without Developer Mode the lock file link is a copy (`fsx::link_file_with`): a copy
/// with the shared lock's exact bytes is Ghostex's, so it is not backed up and replaced on every
/// sync. A copy that fell behind the shared lock reads as the agent's own file again, which the
/// plan backs up and copies afresh. Elsewhere a real file is always the agent's own.
fn lock_copy_counts_as_link(path: &Path, source_lock: &Path) -> bool {
    cfg!(windows) && fsx::same_file_content(path, source_lock)
}

fn scan_lock(home: &Path, spec: &AgentSpec, source: &SourceInfo) -> Option<LockLinkReport> {
    let path = spec.lock_link.as_ref()?;
    let source_lock = source_root(home).join(".skill-lock.json");
    let nothing_shared = !source.lock.exists;
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Some(LockLinkReport {
            path: fsx::display_path(path, home),
            state: if nothing_shared {
                LockLinkState::NothingShared
            } else {
                LockLinkState::Missing
            },
        });
    };
    let state = if metadata.file_type().is_symlink() {
        if fsx::same_entry(path, &source_lock) {
            LockLinkState::Linked
        } else {
            LockLinkState::OtherLink
        }
    } else if nothing_shared {
        LockLinkState::NothingShared
    } else if lock_copy_counts_as_link(path, &source_lock) {
        LockLinkState::Linked
    } else {
        LockLinkState::RealFile
    };
    Some(LockLinkReport {
        path: fsx::display_path(path, home),
        state,
    })
}

/// An agent counts as installed when its root holds anything besides a
/// `skills` folder, or when that folder holds at least one entry that is not a
/// dangling link. A root that only carries dangling links is a leftover from an
/// earlier `skills add` run for an agent that was never used; its links are
/// still reported so Sync can remove them, but no per-skill links are added.
fn agent_detected(spec: &AgentSpec) -> bool {
    if !spec.root.is_dir() {
        return false;
    }
    let skills_name = spec
        .skills_dir
        .as_ref()
        .and_then(|dir| dir.strip_prefix(&spec.root).ok())
        .map(|rest| rest.to_path_buf());
    let has_other_content = fsx::child_names(&spec.root).into_iter().any(|name| {
        skills_name
            .as_ref()
            .map(|skills| {
                Path::new(&name)
                    != skills
                        .components()
                        .next()
                        .map(|c| Path::new(c.as_os_str()))
                        .unwrap_or(Path::new(""))
            })
            .unwrap_or(true)
    });
    if has_other_content {
        return true;
    }
    let Some(dir) = spec.skills_dir.as_ref() else {
        return true;
    };
    if fsx::is_symlink(dir) {
        return true;
    }
    fsx::child_names(dir)
        .into_iter()
        .any(|name| !fsx::is_dangling_link(&dir.join(name)))
}

fn scan_agent(home: &Path, spec: &AgentSpec, source: &SourceInfo) -> AgentReport {
    let detected = agent_detected(spec);
    let mut report = AgentReport {
        id: spec.id.clone(),
        display_name: spec.display_name.clone(),
        icon: spec.icon.clone(),
        kind: spec.kind,
        root: fsx::display_path(&spec.root, home),
        detected,
        universal: spec.universal,
        status: AgentStatus::NotInstalled,
        skills: None,
        instructions: None,
        hooks: None,
        lock: None,
        spec: Some(spec.clone()),
    };
    if !detected {
        // Leftover folders still get their dangling links listed so Sync can remove them.
        if let Some(dir) = spec.skills_dir.as_ref() {
            if dir.is_dir() && !fsx::is_symlink(dir) {
                let mut leftovers = scan_skills(home, spec, source);
                if let Some(skills) = leftovers.as_mut() {
                    skills
                        .entries
                        .retain(|entry| entry.state == SkillEntryState::Dangling);
                    if !skills.entries.is_empty() || !skills.backups.is_empty() {
                        report.skills = Some(with_counts(skills.clone()));
                    }
                }
            }
        }
        return report;
    }
    report.skills = scan_skills(home, spec, source);
    report.instructions = spec
        .instruction_file
        .as_ref()
        .map(|path| InstructionReport {
            path: fsx::display_path(path, home),
            kind: spec.instruction_kind,
            state: pointer::classify(path, spec.instruction_kind),
        });
    report.hooks = scan_hooks(home, spec, source);
    report.lock = scan_lock(home, spec, source);
    report.status = if agent_needs_attention(&report) {
        AgentStatus::Attention
    } else {
        AgentStatus::Linked
    };
    report
}

/// CDXC:AgentSync 2026-10-21 WHY:
/// An agent needs attention only when the plan has a change for it. States the plan only keeps
/// for review by hand (a skills folder or hooks link pointing elsewhere, a copy that drifted from
/// the source, an instruction file that is a link or not a file, nothing shared to link) used to
/// count too, which put the agent under "To fix" with a count the Sync dialog showed as "Nothing
/// to apply". Those states still show on the agent's cards.
fn agent_needs_attention(report: &AgentReport) -> bool {
    if let Some(skills) = &report.skills {
        if skills.dir_state == SkillDirState::WholeFolderLink {
            return true;
        }
        let c = &skills.counts;
        if c.dangling > 0
            || c.redundant_links > 0
            || c.backups > 0
            || c.copies_identical > 0
            || (!report.universal && c.missing > 0)
        {
            return true;
        }
    }
    if let Some(instructions) = &report.instructions {
        if matches!(
            instructions.state,
            InstructionState::Missing
                | InstructionState::LegacyPointer
                | InstructionState::OtherContent
        ) {
            return true;
        }
    }
    if let Some(hooks) = &report.hooks {
        if matches!(hooks.state, HooksState::Missing | HooksState::RealDir) {
            return true;
        }
    }
    if let Some(lock) = &report.lock {
        if matches!(lock.state, LockLinkState::Missing | LockLinkState::RealFile) {
            return true;
        }
    }
    false
}

fn build_problems(source: &SourceInfo, agents: &[AgentReport]) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut dangling = Vec::new();
    let mut copies = Vec::new();
    let mut whole = Vec::new();
    let mut pointers = Vec::new();
    let mut redundant = Vec::new();
    let mut backups = Vec::new();
    for agent in agents {
        if let Some(skills) = &agent.skills {
            backups.extend(skills.backups.iter().cloned());
            if agent.universal && skills.dir_state == SkillDirState::WholeFolderLink {
                redundant.push(skills.dir.clone());
            } else if agent.universal {
                redundant.extend(
                    skills
                        .entries
                        .iter()
                        .filter(|entry| entry.state == SkillEntryState::Linked)
                        .map(|entry| entry.path.clone()),
                );
            }
            for entry in &skills.entries {
                match entry.state {
                    SkillEntryState::Dangling => dangling.push(entry.path.clone()),
                    SkillEntryState::CopyIdentical | SkillEntryState::CopyDrifted => {
                        copies.push(entry.path.clone())
                    }
                    _ => {}
                }
            }
            if skills.dir_state == SkillDirState::WholeFolderLink && !agent.universal {
                whole.push(skills.dir.clone());
            }
        }
        if let Some(instructions) = &agent.instructions {
            if instructions.state != InstructionState::Pointer {
                pointers.push(instructions.path.clone());
            }
        }
    }
    let source_broken: Vec<String> = source
        .skills
        .iter()
        .filter(|s| s.broken)
        .map(|s| s.path.clone())
        .collect();
    if !dangling.is_empty() {
        problems.push(Problem {
            kind: ProblemKind::DanglingLinks,
            count: dangling.len(),
            title: format!(
                "{} dangling symlink{} point at skills that no longer exist",
                dangling.len(),
                plural(dangling.len())
            ),
            detail: "Removed by Sync. Nothing else references them.".to_string(),
            items: dangling,
            fixable: true,
        });
    }
    if !source_broken.is_empty() {
        problems.push(Problem {
            kind: ProblemKind::SourceBrokenLinks,
            count: source_broken.len(),
            title: format!("{} source skill{} is a broken link", source_broken.len(), plural(source_broken.len())),
            detail: "A symlink inside ~/.agents/skills that cannot be resolved, for example one pointing at itself. Sync removes the link; reinstall the skill afterwards.".to_string(),
            items: source_broken,
            fixable: true,
        });
    }
    if !whole.is_empty() {
        problems.push(Problem {
            kind: ProblemKind::WholeFolderLinks,
            count: whole.len(),
            title: format!("{} skills folder{} is a whole-folder link", whole.len(), plural(whole.len())),
            detail: "Converted to a real folder of per-skill links so agent-local files can coexist with the source.".to_string(),
            items: whole,
            fixable: true,
        });
    }
    if !copies.is_empty() {
        problems.push(Problem {
            kind: ProblemKind::CopiedFolders,
            count: copies.len(),
            title: format!("{} skill folder{} are copies, not links", copies.len(), plural(copies.len())),
            detail: "Identical copies are backed up and replaced by links. Copies that differ from the source are kept and listed on the agent.".to_string(),
            items: copies,
            fixable: true,
        });
    }
    if !redundant.is_empty() {
        problems.push(Problem {
            kind: ProblemKind::RedundantLinks,
            count: redundant.len(),
            title: format!(
                "{} link{} in the folders of agents that already read ~/.agents/skills",
                redundant.len(),
                plural(redundant.len())
            ),
            detail: "Those agents load each linked skill twice. Sync removes the links; the skills stay in ~/.agents/skills.".to_string(),
            items: redundant,
            fixable: true,
        });
    }
    if !backups.is_empty() {
        problems.push(Problem {
            kind: ProblemKind::BackupFolders,
            count: backups.len(),
            title: format!(
                "{} backup folder{} inside agent skills folders",
                backups.len(),
                plural(backups.len())
            ),
            detail: "Agents read a .pre-sync-*.bak folder as one more skill. Sync moves them to the Agent Sync backup folder; nothing is deleted.".to_string(),
            items: backups,
            fixable: true,
        });
    }
    if !pointers.is_empty() {
        problems.push(Problem {
            kind: ProblemKind::MissingPointers,
            count: pointers.len(),
            title: format!(
                "{} instruction file{} do not point at ~/.agents/main.md",
                pointers.len(),
                plural(pointers.len())
            ),
            detail:
                "Sync writes the one-line pointer. A file with other content is backed up first."
                    .to_string(),
            items: pointers,
            fixable: true,
        });
    }
    if !source.lock.stale_entries.is_empty() {
        problems.push(Problem {
            kind: ProblemKind::StaleLockEntries,
            count: source.lock.stale_entries.len(),
            title: format!("{} lock entr{} have no skill folder", source.lock.stale_entries.len(), if source.lock.stale_entries.len() == 1 { "y" } else { "ies" }),
            detail: "Left over in .skill-lock.json after skills were removed or renamed. Pruning is an opt-in step of the plan.".to_string(),
            items: source.lock.stale_entries.clone(),
            fixable: true,
        });
    }
    if !source.lock.untracked_skills.is_empty() {
        problems.push(Problem {
            kind: ProblemKind::UntrackedSkills,
            count: source.lock.untracked_skills.len(),
            title: format!("{} skills in the source have no lock entry", source.lock.untracked_skills.len()),
            detail: "Your own skills and the Ghostex bundled skills. The skills CLI skips them on update, which is fine. Nothing to fix.".to_string(),
            items: source.lock.untracked_skills.clone(),
            fixable: false,
        });
    }
    problems
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Scan `home` and describe every agent's distance from `~/.agents`.
pub fn scan(home: &Path) -> SyncReport {
    let source = scan_source(home);
    let agents: Vec<AgentReport> = agent_catalog(home)
        .iter()
        .map(|spec| scan_agent(home, spec, &source))
        .collect();
    let problems = build_problems(&source, &agents);
    let mut summary = ReportSummary::default();
    for agent in &agents {
        match agent.status {
            AgentStatus::Linked => {
                summary.agents_detected += 1;
                summary.agents_linked += 1;
            }
            AgentStatus::Attention => {
                summary.agents_detected += 1;
                summary.agents_attention += 1;
            }
            AgentStatus::NotInstalled => summary.agents_not_installed += 1,
        }
    }
    for problem in &problems {
        match problem.kind {
            ProblemKind::DanglingLinks => summary.dangling_links += problem.count,
            ProblemKind::SourceBrokenLinks => summary.dangling_links += problem.count,
            ProblemKind::StaleLockEntries => summary.stale_lock_entries = problem.count,
            ProblemKind::CopiedFolders => summary.copied_skill_folders = problem.count,
            ProblemKind::WholeFolderLinks => summary.whole_folder_links = problem.count,
            ProblemKind::MissingPointers => summary.missing_pointers = problem.count,
            ProblemKind::RedundantLinks => summary.redundant_links = problem.count,
            ProblemKind::BackupFolders => summary.backup_folders = problem.count,
            ProblemKind::UntrackedSkills => {}
        }
    }
    SyncReport {
        generated_at: crate::now_iso8601(),
        home: home.display().to_string(),
        source,
        agents,
        problems,
        summary,
    }
}

/// Resolve the HOME folder the way both hosts do: `HOME`, else `USERPROFILE`.
pub fn default_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

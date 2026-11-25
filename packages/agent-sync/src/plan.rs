//! Turn a report into an ordered list of filesystem operations, grouped so a
//! user can switch a whole group off before applying.
//!
//! CDXC:AgentSync 2026-09-16 WHY:
//! Five verbs only (link, write, backup, unlink, keep) plus mkdir and the opt-in lock drop.
//! There is no copy and no delete, so every step can be undone by hand. Dangling links are
//! removed without a backup.
//!
//! CDXC:AgentSync 2026-10-14 WHY:
//! Backups used to sit next to the original as `<name>.pre-sync-<stamp>.bak`. Inside a skills
//! folder every agent and the Agents Hub then read that backup as one more skill, so the same
//! skill showed up many times. Anything in the way is now moved to
//! `<Ghostex state>/agent-sync-backups/<stamp>/<path under HOME>` instead (`fsx::backup_path`).
//!
//! CDXC:AgentSync 2026-10-14 DECISION:
//! User: add a plan group, on by default, that removes only links pointing into `~/.agents/skills`
//! from the folders of agents that read that folder themselves (`RemoveRedundantLinks`), and move
//! leftover `<name>.pre-sync-*.bak` folders out of agents' skills folders into the backup folder
//! (`MoveBackups`): "Move, never delete, for other users."

use crate::catalog::{source_root, InstructionKind};
use crate::fsx;
use crate::pointer::{self, InstructionState};
use crate::scan::{
    AgentReport, HooksState, LockLinkState, MainMdSeedCandidate, SkillDirState, SkillEntryState,
    SyncReport,
};
use crate::SyncScope;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanVerb {
    Link,
    Write,
    Backup,
    Unlink,
    Keep,
    Mkdir,
    Drop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum PlanGroupKind {
    SeedMainMd,
    RemoveDangling,
    RemoveRedundantLinks,
    MoveBackups,
    ConvertWholeFolder,
    PerSkillLinks,
    PointerFiles,
    Hooks,
    PruneLock,
}

impl PlanGroupKind {
    pub const ALL: [PlanGroupKind; 9] = [
        PlanGroupKind::SeedMainMd,
        PlanGroupKind::RemoveDangling,
        PlanGroupKind::RemoveRedundantLinks,
        PlanGroupKind::MoveBackups,
        PlanGroupKind::ConvertWholeFolder,
        PlanGroupKind::PerSkillLinks,
        PlanGroupKind::PointerFiles,
        PlanGroupKind::Hooks,
        PlanGroupKind::PruneLock,
    ];

    pub fn parse(value: &str) -> Option<PlanGroupKind> {
        match value.trim() {
            "seedMainMd" | "seed-main-md" | "seed-main" | "seed" => Some(PlanGroupKind::SeedMainMd),
            "removeDangling" | "remove-dangling" | "dangling" => {
                Some(PlanGroupKind::RemoveDangling)
            }
            "removeRedundantLinks" | "remove-redundant-links" | "redundant-links" | "redundant" => {
                Some(PlanGroupKind::RemoveRedundantLinks)
            }
            "moveBackups" | "move-backups" | "backups" => Some(PlanGroupKind::MoveBackups),
            "convertWholeFolder" | "convert-whole-folder" | "whole-folder" => {
                Some(PlanGroupKind::ConvertWholeFolder)
            }
            "perSkillLinks" | "per-skill-links" | "skills" | "links" => {
                Some(PlanGroupKind::PerSkillLinks)
            }
            "pointerFiles" | "pointer-files" | "pointers" | "mds" => {
                Some(PlanGroupKind::PointerFiles)
            }
            "hooks" => Some(PlanGroupKind::Hooks),
            "pruneLock" | "prune-lock" | "lock" => Some(PlanGroupKind::PruneLock),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            PlanGroupKind::SeedMainMd => "seedMainMd",
            PlanGroupKind::RemoveDangling => "removeDangling",
            PlanGroupKind::RemoveRedundantLinks => "removeRedundantLinks",
            PlanGroupKind::MoveBackups => "moveBackups",
            PlanGroupKind::ConvertWholeFolder => "convertWholeFolder",
            PlanGroupKind::PerSkillLinks => "perSkillLinks",
            PlanGroupKind::PointerFiles => "pointerFiles",
            PlanGroupKind::Hooks => "hooks",
            PlanGroupKind::PruneLock => "pruneLock",
        }
    }

    fn title(self) -> &'static str {
        match self {
            PlanGroupKind::SeedMainMd => "Create ~/.agents/main.md",
            PlanGroupKind::RemoveDangling => "Remove dangling links",
            PlanGroupKind::RemoveRedundantLinks => {
                "Remove links agents that read ~/.agents/skills do not need"
            }
            PlanGroupKind::MoveBackups => "Move backup folders out of skills folders",
            PlanGroupKind::ConvertWholeFolder => "Convert whole-folder links to per-skill links",
            PlanGroupKind::PerSkillLinks => "Per-skill links",
            PlanGroupKind::PointerFiles => "Instruction pointer files",
            PlanGroupKind::Hooks => "Hook scripts and lock file links",
            PlanGroupKind::PruneLock => "Prune stale lock entries",
        }
    }

    fn enabled_by_default(self) -> bool {
        !matches!(self, PlanGroupKind::PruneLock)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanOp {
    pub verb: PlanVerb,
    /// Display path (`~/...`).
    pub path: String,
    /// Link target, backup destination, or lock entry name, depending on the verb.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Absolute path for the applier; never shown.
    #[serde(skip)]
    pub abs_path: PathBuf,
    #[serde(skip)]
    pub abs_target: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanGroup {
    pub kind: PlanGroupKind,
    pub title: String,
    pub enabled_by_default: bool,
    pub ops: Vec<PlanOp>,
    pub change_count: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub links: usize,
    pub writes: usize,
    pub backups: usize,
    pub unlinks: usize,
    pub keeps: usize,
    pub mkdirs: usize,
    pub drops: usize,
    /// Every non-keep op in groups that are enabled by default.
    pub default_changes: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    pub generated_at: String,
    pub scope: String,
    pub stamp: String,
    /// Display path of the folder this plan's backups go to (`~/.../<stamp>`).
    pub backup_dir: String,
    pub pointer_policy: PointerPolicy,
    /// True when `~/.agents/main.md` is missing or empty and no seed step is planned,
    /// so pointer file writes were skipped.
    pub main_md_blocked: bool,
    /// Richest existing non-pointer instruction file offered to seed `~/.agents/main.md`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_recommendation: Option<MainMdSeedCandidate>,
    /// All existing non-pointer instruction files that can seed `~/.agents/main.md`, richest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seed_candidates: Vec<MainMdSeedCandidate>,
    /// Clear warnings shown before anything is replaced or when `main.md` blocks pointer writes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub groups: Vec<PlanGroup>,
    pub summary: PlanSummary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PointerPolicy {
    /// Back the file up and write only the pointer (the user's default, DECISION 2B).
    Replace,
    /// Back the file up and prepend the pointer above the agent's own lines.
    Prepend,
    /// Leave existing instruction files alone; only create missing ones.
    LeaveAlone,
}

impl PointerPolicy {
    pub fn parse(value: &str) -> Option<PointerPolicy> {
        match value.trim().to_ascii_lowercase().as_str() {
            "replace" => Some(PointerPolicy::Replace),
            "prepend" => Some(PointerPolicy::Prepend),
            "leave-alone" | "leavealone" | "leave_alone" | "keep" => {
                Some(PointerPolicy::LeaveAlone)
            }
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PointerPolicy::Replace => "replace",
            PointerPolicy::Prepend => "prepend",
            PointerPolicy::LeaveAlone => "leave-alone",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlanOptions {
    pub scope: SyncScope,
    pub stamp: String,
    /// Backups go to `<backup_root>/<stamp>/...`.
    pub backup_root: PathBuf,
    pub pointer_policy: PointerPolicy,
    /// Optional agent id, alias (e.g. `claude`, `codex`), `richest`, or file path to seed
    /// `~/.agents/main.md` from when it is missing or empty.
    pub seed_main_from: Option<String>,
    pub link_hooks: bool,
    pub link_lock: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions {
            scope: SyncScope::All,
            stamp: crate::now_backup_stamp(),
            backup_root: fsx::default_backup_root(),
            pointer_policy: PointerPolicy::Replace,
            seed_main_from: None,
            link_hooks: true,
            link_lock: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedSeed {
    pub agent_id: Option<String>,
    pub label: String,
    pub abs_path: PathBuf,
    pub display_path: String,
    pub body: String,
    pub bytes: usize,
}

fn format_seed_candidates_hint(candidates: &[MainMdSeedCandidate]) -> String {
    if candidates.is_empty() {
        return "No existing non-pointer instruction files were found to seed from. Create ~/.agents/main.md by hand first."
            .to_string();
    }
    let list = candidates
        .iter()
        .map(|candidate| {
            format!(
                "{} ({}, {} bytes)",
                candidate.agent_id, candidate.path, candidate.bytes
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("Available seed sources: {list}.")
}

fn canonical_agent_alias(selector: &str) -> &str {
    match selector {
        "claude" => "claude-code",
        "copilot" => "github-copilot",
        "kiro" => "kiro-cli",
        "factory" | "factory-droid" => "droid",
        "qwen" => "qwen-code",
        "roo-code" => "roo",
        "kilo-code" => "kilo",
        other => other,
    }
}

/// Resolve a `--seed-main-from` selector (agent id, short alias like `claude`, `richest`, or a
/// file path) into the content that will be written to `~/.agents/main.md`.
/// Refuses to overwrite an existing non-empty `~/.agents/main.md` or seed from a pointer file.
pub fn resolve_seed_source(
    home: &Path,
    report: &SyncReport,
    selector: &str,
) -> Result<ResolvedSeed, String> {
    if report.source.main_md_ready() {
        return Err(
            "~/.agents/main.md already exists and has content; refusing to overwrite it. Remove --seed-main-from to sync pointer files."
                .to_string(),
        );
    }
    let main_md_path = source_root(home).join("main.md");
    if fsx::is_symlink(&main_md_path) && !main_md_path.is_file() {
        return Err(
            "~/.agents/main.md is a broken symlink; remove or fix it before seeding.".to_string(),
        );
    }
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "Seed source cannot be empty. {}",
            format_seed_candidates_hint(&report.source.main_md_seed_candidates)
        ));
    }
    let lower = trimmed.to_ascii_lowercase();
    if matches!(lower.as_str(), "richest" | "auto") {
        let Some(best) = report.source.main_md_seed_candidates.first() else {
            return Err(format_seed_candidates_hint(
                &report.source.main_md_seed_candidates,
            ));
        };
        return resolve_seed_source(home, report, &best.agent_id);
    }
    let aliased = canonical_agent_alias(&lower);
    let matched_agent = report.agents.iter().find(|agent| {
        agent.id.eq_ignore_ascii_case(trimmed)
            || agent.id.eq_ignore_ascii_case(aliased)
            || agent
                .icon
                .as_deref()
                .is_some_and(|icon| icon.eq_ignore_ascii_case(&lower))
            || agent.display_name.eq_ignore_ascii_case(trimmed)
    });
    if let Some(agent) = matched_agent {
        let Some(spec) = &agent.spec else {
            return Err(format!(
                "Agent `{}` has no catalog specification.",
                agent.id
            ));
        };
        let Some(path) = &spec.instruction_file else {
            return Err(format!(
                "{} does not have a global instruction file. {}",
                agent.display_name,
                format_seed_candidates_hint(&report.source.main_md_seed_candidates)
            ));
        };
        let display_path = fsx::display_path(path, home);
        let raw_state = pointer::classify(path, spec.instruction_kind);
        if raw_state != InstructionState::OtherContent {
            let why = match raw_state {
                InstructionState::Missing => "does not exist",
                InstructionState::Pointer
                | InstructionState::DanglingPointer
                | InstructionState::LegacyPointer => "is already a pointer to ~/.agents/main.md",
                InstructionState::Symlink => "is a symlink",
                InstructionState::NotAFile => "is not a regular file",
                InstructionState::OtherContent => unreachable!(),
            };
            return Err(format!(
                "Cannot seed ~/.agents/main.md from {} ({display_path}): file {why}. {}",
                agent.display_name,
                format_seed_candidates_hint(&report.source.main_md_seed_candidates)
            ));
        }
        let raw = std::fs::read_to_string(path)
            .map_err(|error| format!("Could not read {display_path}: {error}"))?;
        let body = pointer::seed_body(&raw, spec.instruction_kind);
        if body.trim().is_empty() {
            return Err(format!(
                "Cannot seed ~/.agents/main.md from {} ({display_path}): file is empty. {}",
                agent.display_name,
                format_seed_candidates_hint(&report.source.main_md_seed_candidates)
            ));
        }
        let bytes = body.len();
        return Ok(ResolvedSeed {
            agent_id: Some(agent.id.clone()),
            label: agent.display_name.clone(),
            abs_path: path.clone(),
            display_path,
            body,
            bytes,
        });
    }

    // Fall back to an explicit file path (supports `~/...` and OS-native absolute/relative paths).
    let candidate_path = abs_from_display(home, trimmed);
    if candidate_path.is_file() {
        let kind = if candidate_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("mdc"))
        {
            InstructionKind::Mdc
        } else {
            InstructionKind::Plain
        };
        let display_path = fsx::display_path(&candidate_path, home);
        let raw_state = pointer::classify(&candidate_path, kind);
        if raw_state != InstructionState::OtherContent {
            return Err(format!(
                "Cannot seed ~/.agents/main.md from {display_path}: file is already a pointer or not a regular instruction file."
            ));
        }
        let raw = std::fs::read_to_string(&candidate_path)
            .map_err(|error| format!("Could not read {display_path}: {error}"))?;
        let body = pointer::seed_body(&raw, kind);
        if body.trim().is_empty() {
            return Err(format!(
                "Cannot seed ~/.agents/main.md from {display_path}: file is empty."
            ));
        }
        let bytes = body.len();
        return Ok(ResolvedSeed {
            agent_id: None,
            label: display_path.clone(),
            abs_path: candidate_path,
            display_path,
            body,
            bytes,
        });
    }

    Err(format!(
        "Unknown seed source `{trimmed}`. {}",
        format_seed_candidates_hint(&report.source.main_md_seed_candidates)
    ))
}

struct Builder<'a> {
    home: &'a Path,
    stamp: &'a str,
    backup_root: &'a Path,
    groups: Vec<Vec<PlanOp>>,
}

impl<'a> Builder<'a> {
    fn push(&mut self, kind: PlanGroupKind, op: PlanOp) {
        self.groups[kind as usize].push(op);
    }

    fn op(&self, verb: PlanVerb, abs_path: PathBuf, agent_id: Option<&str>) -> PlanOp {
        PlanOp {
            verb,
            path: fsx::display_path(&abs_path, self.home),
            target: None,
            note: None,
            agent_id: agent_id.map(str::to_string),
            content: None,
            abs_path,
            abs_target: None,
        }
    }

    fn with_target(&self, mut op: PlanOp, abs_target: PathBuf) -> PlanOp {
        op.target = Some(fsx::display_path(&abs_target, self.home));
        op.abs_target = Some(abs_target);
        op
    }

    fn link_op(&self, link: PathBuf, target: &Path, agent_id: &str, relative: bool) -> PlanOp {
        let mut op = self.op(PlanVerb::Link, link.clone(), Some(agent_id));
        let written = if relative {
            fsx::link_target_for(&link, target)
        } else {
            target.to_path_buf()
        };
        op.target = Some(written.display().to_string());
        op.abs_target = Some(written);
        op
    }

    fn backup_op(&self, path: PathBuf, agent_id: Option<&str>, note: &str) -> PlanOp {
        let backup = fsx::backup_path(&path, self.home, self.backup_root, self.stamp);
        let mut op = self.with_target(self.op(PlanVerb::Backup, path, agent_id), backup);
        op.note = Some(note.to_string());
        op
    }

    fn keep_op(&self, path: PathBuf, agent_id: Option<&str>, note: &str) -> PlanOp {
        let mut op = self.op(PlanVerb::Keep, path, agent_id);
        op.note = Some(note.to_string());
        op
    }
}

fn abs_from_display(home: &Path, display: &str) -> PathBuf {
    match display
        .strip_prefix("~/")
        .or_else(|| display.strip_prefix("~\\"))
    {
        Some(rest) => rest
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .fold(home.to_path_buf(), |path, part| path.join(part)),
        None if display == "~" => home.to_path_buf(),
        None => PathBuf::from(display),
    }
}

fn plan_agent(
    b: &mut Builder<'_>,
    agent: &AgentReport,
    options: &PlanOptions,
    will_have_main_md: bool,
    main_md_empty: bool,
) {
    let Some(spec) = &agent.spec else {
        return;
    };
    let source = source_root(b.home);
    let source_skills = source.join("skills");
    let agent_id = agent.id.as_str();

    if let (Some(skills), Some(dir)) = (&agent.skills, spec.skills_dir.as_ref()) {
        for backup in &skills.backups {
            let path = abs_from_display(b.home, backup);
            let op = b.backup_op(path, Some(agent_id), "left over from an earlier sync");
            b.push(PlanGroupKind::MoveBackups, op);
        }
        match skills.dir_state {
            SkillDirState::WholeFolderLink if agent.universal => {
                let mut unlink = b.op(PlanVerb::Unlink, dir.clone(), Some(agent_id));
                unlink.note = Some(
                    "folder link to ~/.agents/skills; this agent reads that folder itself"
                        .to_string(),
                );
                b.push(PlanGroupKind::RemoveRedundantLinks, unlink);
            }
            SkillDirState::WholeFolderLink => {
                let mut unlink = b.op(PlanVerb::Unlink, dir.clone(), Some(agent_id));
                unlink.note = Some(
                    "folder symlink; replaced by a real folder of per-skill links".to_string(),
                );
                b.push(PlanGroupKind::ConvertWholeFolder, unlink);
                b.push(
                    PlanGroupKind::ConvertWholeFolder,
                    b.op(PlanVerb::Mkdir, dir.clone(), Some(agent_id)),
                );
                for entry in &skills.entries {
                    let op = b.link_op(
                        dir.join(&entry.name),
                        &source_skills.join(&entry.name),
                        agent_id,
                        true,
                    );
                    b.push(PlanGroupKind::ConvertWholeFolder, op);
                }
            }
            SkillDirState::OtherLink => {
                let op = b.keep_op(
                    dir.clone(),
                    Some(agent_id),
                    "skills folder is a link to somewhere else; review it by hand",
                );
                b.push(PlanGroupKind::PerSkillLinks, op);
            }
            SkillDirState::NotADir => {
                let op = b.keep_op(
                    dir.clone(),
                    Some(agent_id),
                    "not a folder; review it by hand",
                );
                b.push(PlanGroupKind::PerSkillLinks, op);
            }
            SkillDirState::Missing | SkillDirState::RealDir => {
                let mut need_mkdir = skills.dir_state == SkillDirState::Missing;
                for entry in &skills.entries {
                    let path = dir.join(&entry.name);
                    let source_path = source_skills.join(&entry.name);
                    match entry.state {
                        SkillEntryState::Dangling => {
                            let mut op = b.op(PlanVerb::Unlink, path, Some(agent_id));
                            op.note = entry
                                .link_target
                                .as_ref()
                                .map(|t| format!("target missing: {t}"));
                            b.push(PlanGroupKind::RemoveDangling, op);
                        }
                        SkillEntryState::Linked if agent.universal => {
                            let mut op = b.op(PlanVerb::Unlink, path, Some(agent_id));
                            op.note = Some("this agent reads ~/.agents/skills itself".to_string());
                            b.push(PlanGroupKind::RemoveRedundantLinks, op);
                        }
                        SkillEntryState::Linked => {
                            let op = b.keep_op(path, Some(agent_id), "already linked");
                            b.push(PlanGroupKind::PerSkillLinks, op);
                        }
                        SkillEntryState::LinkedElsewhere => {
                            let note = format!(
                                "linked to {}, not the source; kept",
                                entry.link_target.clone().unwrap_or_default()
                            );
                            let op = b.keep_op(path, Some(agent_id), &note);
                            b.push(PlanGroupKind::PerSkillLinks, op);
                        }
                        SkillEntryState::CopyIdentical => {
                            let backup = b.backup_op(
                                path.clone(),
                                Some(agent_id),
                                "identical to the source copy",
                            );
                            b.push(PlanGroupKind::PerSkillLinks, backup);
                            if !agent.universal {
                                let op = b.link_op(path, &source_path, agent_id, true);
                                b.push(PlanGroupKind::PerSkillLinks, op);
                            }
                        }
                        SkillEntryState::CopyDrifted => {
                            let op = b.keep_op(
                                path,
                                Some(agent_id),
                                "differs from the source; kept for review",
                            );
                            b.push(PlanGroupKind::PerSkillLinks, op);
                        }
                        SkillEntryState::OnlyHere => {
                            let op =
                                b.keep_op(path, Some(agent_id), "only here; not in the source");
                            b.push(PlanGroupKind::PerSkillLinks, op);
                        }
                        SkillEntryState::Missing => {
                            if need_mkdir {
                                need_mkdir = false;
                                b.push(
                                    PlanGroupKind::PerSkillLinks,
                                    b.op(PlanVerb::Mkdir, dir.clone(), Some(agent_id)),
                                );
                            }
                            let op = b.link_op(path, &source_path, agent_id, true);
                            b.push(PlanGroupKind::PerSkillLinks, op);
                        }
                        SkillEntryState::ViaWholeFolder => {}
                    }
                }
                if agent.universal && skills.dir_state == SkillDirState::RealDir {
                    let op = b.keep_op(
                        dir.clone(),
                        Some(agent_id),
                        "reads ~/.agents/skills directly; no links added",
                    );
                    b.push(PlanGroupKind::PerSkillLinks, op);
                }
            }
        }
    }

    if let (Some(instructions), Some(path)) = (&agent.instructions, spec.instruction_file.as_ref())
    {
        if !will_have_main_md {
            let reason = if main_md_empty {
                "~/.agents/main.md is empty"
            } else {
                "~/.agents/main.md does not exist"
            };
            let note = match instructions.state {
                InstructionState::DanglingPointer => {
                    format!("dangling pointer: {reason}; create main.md or restore from backup")
                }
                InstructionState::Symlink => "is a symlink; review it by hand".to_string(),
                InstructionState::NotAFile => "not a file; review it by hand".to_string(),
                _ => format!("skipped: {reason}; create main.md first"),
            };
            let op = b.keep_op(path.clone(), Some(agent_id), &note);
            b.push(PlanGroupKind::PointerFiles, op);
        } else {
            let content = pointer::pointer_content(spec.instruction_kind);
            let effective_state = if instructions.state == InstructionState::DanglingPointer {
                pointer::classify(path, spec.instruction_kind)
            } else {
                instructions.state
            };
            match effective_state {
                InstructionState::Pointer => {
                    let note = if instructions.state == InstructionState::DanglingPointer {
                        "pointer becomes valid once ~/.agents/main.md is created"
                    } else {
                        "already points to ~/.agents/main.md"
                    };
                    let op = b.keep_op(path.clone(), Some(agent_id), note);
                    b.push(PlanGroupKind::PointerFiles, op);
                }
                InstructionState::Missing => {
                    let mut op = b.op(PlanVerb::Write, path.clone(), Some(agent_id));
                    op.content = Some(content);
                    op.note = Some("pointer to ~/.agents/main.md".to_string());
                    b.push(PlanGroupKind::PointerFiles, op);
                }
                InstructionState::LegacyPointer | InstructionState::OtherContent => {
                    match options.pointer_policy {
                        PointerPolicy::Replace => {
                            let note = if effective_state == InstructionState::LegacyPointer {
                                "older pointer wording; will be replaced"
                            } else {
                                "has content of its own; will be replaced by pointer"
                            };
                            b.push(
                                PlanGroupKind::PointerFiles,
                                b.backup_op(path.clone(), Some(agent_id), note),
                            );
                            let mut op = b.op(PlanVerb::Write, path.clone(), Some(agent_id));
                            op.content = Some(content);
                            op.note = Some(
                                "replaces existing file with pointer to ~/.agents/main.md"
                                    .to_string(),
                            );
                            b.push(PlanGroupKind::PointerFiles, op);
                        }
                        PointerPolicy::Prepend => {
                            if effective_state == InstructionState::LegacyPointer {
                                b.push(
                                    PlanGroupKind::PointerFiles,
                                    b.backup_op(
                                        path.clone(),
                                        Some(agent_id),
                                        "older pointer wording; upgraded to current pointer",
                                    ),
                                );
                                let mut op = b.op(PlanVerb::Write, path.clone(), Some(agent_id));
                                op.content = Some(content);
                                op.note = Some("pointer to ~/.agents/main.md".to_string());
                                b.push(PlanGroupKind::PointerFiles, op);
                            } else {
                                let existing = std::fs::read_to_string(path).unwrap_or_default();
                                b.push(
                                    PlanGroupKind::PointerFiles,
                                    b.backup_op(
                                        path.clone(),
                                        Some(agent_id),
                                        "backed up before prepending pointer",
                                    ),
                                );
                                let mut op = b.op(PlanVerb::Write, path.clone(), Some(agent_id));
                                op.content = Some(pointer::prepend_pointer(
                                    &existing,
                                    spec.instruction_kind,
                                ));
                                op.note = Some(
                                    "pointer prepended above the existing content".to_string(),
                                );
                                b.push(PlanGroupKind::PointerFiles, op);
                            }
                        }
                        PointerPolicy::LeaveAlone => {
                            let op = b.keep_op(
                                path.clone(),
                                Some(agent_id),
                                "has other content; left alone by pointer policy",
                            );
                            b.push(PlanGroupKind::PointerFiles, op);
                        }
                    }
                }
                InstructionState::DanglingPointer => {
                    let op = b.keep_op(
                        path.clone(),
                        Some(agent_id),
                        "pointer becomes valid once ~/.agents/main.md is created",
                    );
                    b.push(PlanGroupKind::PointerFiles, op);
                }
                InstructionState::Symlink => {
                    let op = b.keep_op(
                        path.clone(),
                        Some(agent_id),
                        "is a symlink; review it by hand",
                    );
                    b.push(PlanGroupKind::PointerFiles, op);
                }
                InstructionState::NotAFile => {
                    let op = b.keep_op(
                        path.clone(),
                        Some(agent_id),
                        "not a file; review it by hand",
                    );
                    b.push(PlanGroupKind::PointerFiles, op);
                }
            }
        }
    }

    if options.link_hooks {
        if let (Some(hooks), Some(dir)) = (&agent.hooks, spec.hooks_dir.as_ref()) {
            let source_hooks = source.join("hooks");
            match hooks.state {
                HooksState::Linked => {
                    let op = b.keep_op(dir.clone(), Some(agent_id), "already linked");
                    b.push(PlanGroupKind::Hooks, op);
                }
                HooksState::Missing => {
                    let op = b.link_op(dir.clone(), &source_hooks, agent_id, false);
                    b.push(PlanGroupKind::Hooks, op);
                }
                HooksState::RealDir => {
                    b.push(
                        PlanGroupKind::Hooks,
                        b.backup_op(dir.clone(), Some(agent_id), "agent-local hooks folder"),
                    );
                    let op = b.link_op(dir.clone(), &source_hooks, agent_id, false);
                    b.push(PlanGroupKind::Hooks, op);
                }
                HooksState::NothingShared => {}
                HooksState::OtherLink => {
                    let op = b.keep_op(
                        dir.clone(),
                        Some(agent_id),
                        "linked somewhere else; review it by hand",
                    );
                    b.push(PlanGroupKind::Hooks, op);
                }
            }
        }
    }

    if options.link_lock {
        if let (Some(lock), Some(path)) = (&agent.lock, spec.lock_link.as_ref()) {
            let source_lock = source.join(".skill-lock.json");
            match lock.state {
                LockLinkState::Linked => {
                    let op = b.keep_op(path.clone(), Some(agent_id), "already linked");
                    b.push(PlanGroupKind::Hooks, op);
                }
                LockLinkState::Missing => {
                    let op = b.link_op(path.clone(), &source_lock, agent_id, false);
                    b.push(PlanGroupKind::Hooks, op);
                }
                LockLinkState::RealFile => {
                    b.push(
                        PlanGroupKind::Hooks,
                        b.backup_op(path.clone(), Some(agent_id), "agent-local lock file"),
                    );
                    let op = b.link_op(path.clone(), &source_lock, agent_id, false);
                    b.push(PlanGroupKind::Hooks, op);
                }
                LockLinkState::OtherLink => {
                    let op = b.keep_op(
                        path.clone(),
                        Some(agent_id),
                        "linked somewhere else; review it by hand",
                    );
                    b.push(PlanGroupKind::Hooks, op);
                }
                LockLinkState::NothingShared => {}
            }
        }
    }
}

/// Build the plan for `report`. `scope` limits agent work to one agent; the
/// source-level groups (broken source links, lock pruning) are included only
/// for the `all` scope.
///
/// CDXC:AgentSync 2026-11-24 WHY:
/// Never plan pointer writes while `~/.agents/main.md` is missing or empty unless the caller
/// explicitly seeds `main.md` as the first step (`SeedMainMd`), so agents never lose their
/// existing instructions to a dangling pointer (GitHub #735).
pub fn build_plan(home: &Path, report: &SyncReport, options: &PlanOptions) -> SyncPlan {
    let mut builder = Builder {
        home,
        stamp: &options.stamp,
        backup_root: &options.backup_root,
        groups: vec![Vec::new(); PlanGroupKind::ALL.len()],
    };
    let mut warnings = Vec::new();
    let main_md_ready = report.source.main_md_ready();
    let mut will_have_main_md = main_md_ready;
    let mut seed_group_title: Option<String> = None;

    if let Some(selector) = options.seed_main_from.as_deref() {
        match resolve_seed_source(home, report, selector) {
            Ok(seed) => {
                let main_md_path = source_root(home).join("main.md");
                let mut op = builder.with_target(
                    builder.op(PlanVerb::Write, main_md_path, seed.agent_id.as_deref()),
                    seed.abs_path,
                );
                op.content = Some(seed.body);
                op.note = Some(format!(
                    "seeded from {} ({}, {} bytes)",
                    seed.label, seed.display_path, seed.bytes
                ));
                seed_group_title = Some(format!("Create ~/.agents/main.md from {}", seed.label));
                builder.push(PlanGroupKind::SeedMainMd, op);
                will_have_main_md = true;
            }
            Err(error) => {
                warnings.push(error);
            }
        }
    }

    if options.scope == SyncScope::All {
        for skill in report.source.skills.iter().filter(|s| s.broken) {
            let mut op = builder.op(PlanVerb::Unlink, abs_from_display(home, &skill.path), None);
            op.note =
                Some("broken link inside the source; reinstall the skill afterwards".to_string());
            builder.push(PlanGroupKind::RemoveDangling, op);
        }
    }

    let mut any_in_scope_instructions = false;
    for agent in &report.agents {
        match &options.scope {
            SyncScope::All => {}
            SyncScope::Agent(id) if id == &agent.id => {}
            SyncScope::Agent(_) => continue,
        }
        if agent.detected {
            if agent.instructions.is_some() {
                any_in_scope_instructions = true;
            }
            plan_agent(
                &mut builder,
                agent,
                options,
                will_have_main_md,
                report.source.main_md_empty,
            );
        } else if let (Some(skills), Some(spec)) = (&agent.skills, &agent.spec) {
            // Leftover folders of agents that were never installed: only remove their dangling links.
            if let Some(dir) = spec.skills_dir.as_ref() {
                for entry in skills
                    .entries
                    .iter()
                    .filter(|e| e.state == SkillEntryState::Dangling)
                {
                    let mut op =
                        builder.op(PlanVerb::Unlink, dir.join(&entry.name), Some(&agent.id));
                    op.note = Some(
                        "leftover of an earlier install for an agent that is not set up here"
                            .to_string(),
                    );
                    builder.push(PlanGroupKind::RemoveDangling, op);
                }
            }
            for backup in &skills.backups {
                let path = abs_from_display(home, backup);
                let op = builder.backup_op(path, Some(&agent.id), "left over from an earlier sync");
                builder.push(PlanGroupKind::MoveBackups, op);
            }
        }
    }

    if options.scope == SyncScope::All && report.source.lock.exists {
        let lock_path = abs_from_display(home, &report.source.lock.path);
        for entry in &report.source.lock.stale_entries {
            let mut op = builder.op(PlanVerb::Drop, lock_path.clone(), None);
            op.target = Some(entry.clone());
            op.note = Some("no skill folder in ~/.agents/skills".to_string());
            builder.push(PlanGroupKind::PruneLock, op);
        }
    }

    let backup_dir = fsx::display_path(&options.backup_root.join(&options.stamp), home);
    let main_md_blocked = !will_have_main_md && any_in_scope_instructions;
    let seed_candidates = if main_md_ready {
        Vec::new()
    } else {
        report.source.main_md_seed_candidates.clone()
    };
    let seed_recommendation = seed_candidates.first().cloned();

    if main_md_blocked {
        let reason = if report.source.main_md_empty {
            "~/.agents/main.md is empty"
        } else {
            "~/.agents/main.md does not exist"
        };
        if let Some(best) = &seed_recommendation {
            warnings.push(format!(
                "{reason}. Pointer files will not be written until main.md has content. First step: create main.md from {} ({}, {} bytes).",
                best.display_name, best.path, best.bytes
            ));
        } else {
            warnings.push(format!(
                "{reason}. Pointer files will not be written until main.md has content. Create ~/.agents/main.md with your shared instructions (or restore originals from backup) first."
            ));
        }
    }

    let pointer_backups: Vec<String> = builder.groups[PlanGroupKind::PointerFiles as usize]
        .iter()
        .filter(|op| op.verb == PlanVerb::Backup)
        .map(|op| op.path.clone())
        .collect();
    if !pointer_backups.is_empty() {
        let listed = pointer_backups.join(", ");
        let noun = if pointer_backups.len() == 1 {
            "existing instruction file"
        } else {
            "existing instruction files"
        };
        match options.pointer_policy {
            PointerPolicy::Replace => {
                warnings.push(format!(
                    "Replacing {} {noun} with the one-line pointer to ~/.agents/main.md ({listed}). Originals are backed up to {backup_dir} first.",
                    pointer_backups.len()
                ));
            }
            PointerPolicy::Prepend => {
                warnings.push(format!(
                    "Modifying {} {noun} to prepend the pointer to ~/.agents/main.md ({listed}). Originals are backed up to {backup_dir} first.",
                    pointer_backups.len()
                ));
            }
            PointerPolicy::LeaveAlone => {}
        }
    }

    let other_replacements: Vec<String> = [PlanGroupKind::PerSkillLinks, PlanGroupKind::Hooks]
        .iter()
        .flat_map(|kind| builder.groups[*kind as usize].iter())
        .filter(|op| op.verb == PlanVerb::Backup)
        .map(|op| op.path.clone())
        .collect();
    if !other_replacements.is_empty() {
        let noun = if other_replacements.len() == 1 {
            "existing item"
        } else {
            "existing items"
        };
        warnings.push(format!(
            "Replacing {} {noun} with symlinks ({}). Originals are backed up to {backup_dir} first.",
            other_replacements.len(),
            other_replacements.join(", ")
        ));
    }

    let mut summary = PlanSummary::default();
    let groups: Vec<PlanGroup> = PlanGroupKind::ALL
        .iter()
        .zip(builder.groups)
        .map(|(kind, ops)| {
            let mut change_count = 0;
            for op in &ops {
                match op.verb {
                    PlanVerb::Link => summary.links += 1,
                    PlanVerb::Write => summary.writes += 1,
                    PlanVerb::Backup => summary.backups += 1,
                    PlanVerb::Unlink => summary.unlinks += 1,
                    PlanVerb::Keep => summary.keeps += 1,
                    PlanVerb::Mkdir => summary.mkdirs += 1,
                    PlanVerb::Drop => summary.drops += 1,
                }
                if op.verb != PlanVerb::Keep {
                    change_count += 1;
                }
            }
            if kind.enabled_by_default() {
                summary.default_changes += change_count;
            }
            let title = if *kind == PlanGroupKind::SeedMainMd {
                seed_group_title
                    .clone()
                    .unwrap_or_else(|| kind.title().to_string())
            } else {
                kind.title().to_string()
            };
            PlanGroup {
                kind: *kind,
                title,
                enabled_by_default: kind.enabled_by_default(),
                ops,
                change_count,
            }
        })
        .collect();

    SyncPlan {
        generated_at: crate::now_iso8601(),
        scope: options.scope.as_str(),
        stamp: options.stamp.clone(),
        backup_dir,
        pointer_policy: options.pointer_policy,
        main_md_blocked,
        seed_recommendation,
        seed_candidates,
        warnings,
        groups,
        summary,
    }
}

//! Agent Sync as the Hub's fifth tab reads it: the report, plan and apply result that
//! packages/agent-sync serializes (camelCase fields, camelCase enum strings), plus the display
//! rules packages/shared/agent-sync.ts (deleted 2026-10-01) and packages/core-ui/agents-hub-sync/problem-copy.ts (deleted 2026-10-01)
//! applied to them.
//!
//! CDXC:AgentSync 2026-09-16 SEE-ALSO:
//! packages/agent-sync/src/{scan,plan,apply}.rs are the source of truth for these shapes; the
//! desktop bridge (apps/desktop/src/app/helpers/agents_hub/sync.rs) and `ghostex agent-sync`
//! both emit them unchanged.
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncSkillEntry {
    #[serde(default)]
    pub(crate) ghostex_bundled: bool,
    #[serde(default)]
    pub(crate) link_target: Option<String>,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) state: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncSkillCounts {
    #[serde(default)]
    pub(crate) copies_drifted: usize,
    #[serde(default)]
    pub(crate) copies_identical: usize,
    #[serde(default)]
    pub(crate) dangling: usize,
    #[serde(default)]
    pub(crate) linked: usize,
    #[serde(default)]
    pub(crate) linked_elsewhere: usize,
    #[serde(default)]
    pub(crate) missing: usize,
    #[serde(default)]
    pub(crate) only_here: usize,
    #[serde(default)]
    pub(crate) via_whole_folder: usize,
    #[serde(default)]
    pub(crate) redundant_links: usize,
    #[serde(default)]
    pub(crate) backups: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncSkillsReport {
    #[serde(default)]
    pub(crate) counts: SyncSkillCounts,
    #[serde(default)]
    pub(crate) dir_state: String,
    #[serde(default)]
    pub(crate) entries: Vec<SyncSkillEntry>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncInstructionReport {
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) state: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncHooksReport {
    #[serde(default)]
    pub(crate) dir: String,
    #[serde(default)]
    pub(crate) state: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncLockLinkReport {
    #[serde(default)]
    pub(crate) state: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncAgentReport {
    #[serde(default)]
    pub(crate) detected: bool,
    #[serde(default)]
    pub(crate) display_name: String,
    #[serde(default)]
    pub(crate) hooks: Option<SyncHooksReport>,
    #[serde(default)]
    pub(crate) icon: Option<String>,
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) instructions: Option<SyncInstructionReport>,
    #[serde(default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) lock: Option<SyncLockLinkReport>,
    #[serde(default)]
    pub(crate) root: String,
    #[serde(default)]
    pub(crate) skills: Option<SyncSkillsReport>,
    #[serde(default)]
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) universal: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncSourceSkill {
    #[serde(default)]
    pub(crate) broken: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncSeedCandidate {
    #[serde(default)]
    pub(crate) agent_id: String,
    #[serde(default)]
    pub(crate) display_name: String,
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) bytes: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncSourceInfo {
    #[serde(default)]
    pub(crate) hook_script_count: usize,
    #[serde(default)]
    pub(crate) main_md_exists: bool,
    #[serde(default)]
    pub(crate) main_md_empty: bool,
    #[serde(default)]
    pub(crate) main_md_seed_candidates: Vec<SyncSeedCandidate>,
    #[serde(default)]
    pub(crate) md_files: Vec<String>,
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) skills: Vec<SyncSourceSkill>,
}

impl SyncSourceInfo {
    pub(crate) fn main_md_ready(&self) -> bool {
        self.main_md_exists && !self.main_md_empty
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncProblem {
    #[serde(default)]
    pub(crate) count: usize,
    #[serde(default)]
    pub(crate) detail: String,
    #[serde(default)]
    pub(crate) fixable: bool,
    #[serde(default)]
    pub(crate) items: Vec<String>,
    #[serde(default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) title: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncReportSummary {
    #[serde(default)]
    pub(crate) agents_attention: usize,
    #[serde(default)]
    pub(crate) agents_detected: usize,
    #[serde(default)]
    pub(crate) agents_linked: usize,
}

/// `agentSyncReport`, or its `errorMessage` when the scan could not run.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncReport {
    #[serde(default)]
    pub(crate) agents: Vec<SyncAgentReport>,
    #[serde(default)]
    pub(crate) generated_at: String,
    #[serde(default)]
    pub(crate) home: String,
    #[serde(default)]
    pub(crate) problems: Vec<SyncProblem>,
    #[serde(default)]
    pub(crate) source: SyncSourceInfo,
    #[serde(default)]
    pub(crate) summary: SyncReportSummary,
    #[serde(default)]
    pub(crate) error_message: Option<String>,
}

impl SyncReport {
    pub(crate) fn from_json(value: serde_json::Value) -> Option<Self> {
        serde_json::from_value(value).ok()
    }

    pub(crate) fn agent(&self, id: &str) -> Option<&SyncAgentReport> {
        self.agents.iter().find(|agent| agent.id == id)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncPlanOp {
    #[serde(default)]
    pub(crate) content: Option<String>,
    #[serde(default)]
    pub(crate) note: Option<String>,
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) target: Option<String>,
    #[serde(default)]
    pub(crate) verb: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncPlanGroup {
    #[serde(default)]
    pub(crate) change_count: usize,
    #[serde(default)]
    pub(crate) enabled_by_default: bool,
    #[serde(default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) ops: Vec<SyncPlanOp>,
    #[serde(default)]
    pub(crate) title: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncPlanSummary {
    #[serde(default)]
    pub(crate) backups: usize,
    #[serde(default)]
    pub(crate) links: usize,
    #[serde(default)]
    pub(crate) unlinks: usize,
    #[serde(default)]
    pub(crate) writes: usize,
}

/// `agentSyncPlan`, or its `errorMessage`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncPlan {
    #[serde(default)]
    pub(crate) groups: Vec<SyncPlanGroup>,
    #[serde(default)]
    pub(crate) scope: String,
    #[serde(default)]
    pub(crate) backup_dir: String,
    #[serde(default)]
    pub(crate) main_md_blocked: bool,
    #[serde(default)]
    pub(crate) seed_recommendation: Option<SyncSeedCandidate>,
    #[serde(default)]
    pub(crate) seed_candidates: Vec<SyncSeedCandidate>,
    #[serde(default)]
    pub(crate) warnings: Vec<String>,
    #[serde(default)]
    pub(crate) summary: SyncPlanSummary,
    #[serde(default)]
    pub(crate) error_message: Option<String>,
}

impl SyncPlan {
    /// CDXC:AgentSync 2026-10-24 DECISION: "only show stuff that needs fixing, don't show stuff
    /// already fixed with 'nothing to do'". A group with nothing to change is dropped here, where
    /// the plan becomes the sheet's state, so the drawing, "Copy as shell script", the Apply count,
    /// the header pills and the groups sent to Apply all see the same list.
    pub(crate) fn from_json(value: serde_json::Value) -> Option<Self> {
        let mut plan: Self = serde_json::from_value(value).ok()?;
        plan.groups
            .retain(|group| group.change_count > 0 || group.ops.iter().any(|op| op.verb != "keep"));
        // The header pills count the operations the sheet lists, so an empty plan shows zeros.
        let count = |verb: &str| {
            plan.groups
                .iter()
                .flat_map(|group| &group.ops)
                .filter(|op| op.verb == verb)
                .count()
        };
        plan.summary = SyncPlanSummary {
            backups: count("backup"),
            links: count("link"),
            unlinks: count("unlink"),
            writes: count("write"),
        };
        Some(plan)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncApplyFailure {
    #[serde(default)]
    pub(crate) error: String,
    #[serde(default)]
    pub(crate) op: SyncPlanOp,
}

/// `agentSyncApplyResult`, or its `errorMessage`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncApplyResult {
    #[serde(default)]
    pub(crate) done: Vec<SyncPlanOp>,
    #[serde(default)]
    pub(crate) failed: Vec<SyncApplyFailure>,
    #[serde(default)]
    pub(crate) scope: String,
    #[serde(default)]
    pub(crate) skipped_keeps: usize,
    #[serde(default)]
    pub(crate) error_message: Option<String>,
}

impl SyncApplyResult {
    pub(crate) fn from_json(value: serde_json::Value) -> Option<Self> {
        serde_json::from_value(value).ok()
    }
}

/// Skills, Instructions, Hook scripts: the three things Agent Sync shares, one tone each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncTone {
    Ok,
    Warn,
    Err,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncPart {
    Skills,
    Instructions,
    Hooks,
}

pub(crate) const SYNC_PARTS: [SyncPart; 3] =
    [SyncPart::Skills, SyncPart::Instructions, SyncPart::Hooks];

pub(crate) const PLAN_GROUP_KINDS: [&str; 9] = [
    "seedMainMd",
    "removeDangling",
    "removeRedundantLinks",
    "moveBackups",
    "convertWholeFolder",
    "perSkillLinks",
    "pointerFiles",
    "hooks",
    "pruneLock",
];

/// The plan groups a problem row's fix button enables.
pub(crate) fn problem_fix_groups(kind: &str) -> Vec<&'static str> {
    match kind {
        "danglingPointers" | "missingMainMd" => vec!["seedMainMd", "pointerFiles"],
        "danglingLinks" | "sourceBrokenLinks" => vec!["removeDangling"],
        "redundantLinks" => vec!["removeRedundantLinks"],
        "backupFolders" => vec!["moveBackups"],
        "wholeFolderLinks" => vec!["convertWholeFolder"],
        "copiedFolders" => vec!["perSkillLinks"],
        "missingPointers" => vec!["pointerFiles"],
        "staleLockEntries" => vec!["pruneLock"],
        _ => Vec::new(),
    }
}

pub(crate) fn default_plan_groups(plan: Option<&SyncPlan>) -> Vec<String> {
    match plan {
        None => PLAN_GROUP_KINDS
            .iter()
            .filter(|kind| **kind != "pruneLock")
            .map(|kind| kind.to_string())
            .collect(),
        Some(plan) => plan
            .groups
            .iter()
            .filter(|group| group.enabled_by_default)
            .map(|group| group.kind.clone())
            .collect(),
    }
}

pub(crate) fn plan_change_count(plan: &SyncPlan, enabled: &[String]) -> usize {
    plan.groups
        .iter()
        .filter(|group| enabled.contains(&group.kind))
        .map(|group| group.change_count)
        .sum()
}

pub(crate) fn skills_tone(agent: &SyncAgentReport) -> SyncTone {
    let Some(skills) = agent.skills.as_ref() else {
        return if agent.universal {
            SyncTone::Ok
        } else {
            SyncTone::Off
        };
    };
    if skills.counts.dangling > 0
        || skills.dir_state == "otherLink"
        || skills.dir_state == "notADir"
    {
        return SyncTone::Err;
    }
    if skills.dir_state == "wholeFolderLink"
        || skills.counts.redundant_links > 0
        || skills.counts.backups > 0
        || skills.counts.copies_identical > 0
        || skills.counts.copies_drifted > 0
        || (!agent.universal && skills.counts.missing > 0)
    {
        return SyncTone::Warn;
    }
    SyncTone::Ok
}

pub(crate) fn instructions_tone(agent: &SyncAgentReport) -> SyncTone {
    match agent
        .instructions
        .as_ref()
        .map(|value| value.state.as_str())
    {
        None => SyncTone::Off,
        Some("pointer") => SyncTone::Ok,
        Some("missing" | "legacyPointer" | "otherContent") => SyncTone::Warn,
        Some(_) => SyncTone::Err,
    }
}

pub(crate) fn hooks_tone(agent: &SyncAgentReport) -> SyncTone {
    match agent.hooks.as_ref().map(|value| value.state.as_str()) {
        None | Some("nothingShared") => SyncTone::Off,
        Some("linked") => SyncTone::Ok,
        Some("missing" | "realDir") => SyncTone::Warn,
        Some(_) => SyncTone::Err,
    }
}

/// The plan groups that fix one part of one agent; empty when that part has nothing to fix.
pub(crate) fn part_fix_groups(agent: &SyncAgentReport, part: SyncPart) -> Vec<&'static str> {
    match part {
        SyncPart::Skills => {
            let Some(skills) = agent.skills.as_ref() else {
                return Vec::new();
            };
            let mut groups = Vec::new();
            if skills.counts.dangling > 0 {
                groups.push("removeDangling");
            }
            if skills.counts.redundant_links > 0 {
                groups.push("removeRedundantLinks");
            }
            if skills.counts.backups > 0 {
                groups.push("moveBackups");
            }
            if skills.dir_state == "wholeFolderLink" && !agent.universal {
                groups.push("convertWholeFolder");
            }
            if agent.detected
                && (skills.counts.copies_identical > 0
                    || (!agent.universal && skills.counts.missing > 0))
            {
                groups.push("perSkillLinks");
            }
            groups
        }
        SyncPart::Instructions => {
            let state = agent
                .instructions
                .as_ref()
                .map(|value| value.state.as_str());
            if agent.detected
                && matches!(
                    state,
                    Some("missing" | "danglingPointer" | "legacyPointer" | "otherContent")
                )
            {
                vec!["pointerFiles"]
            } else {
                Vec::new()
            }
        }
        SyncPart::Hooks => {
            let hooks_open = agent
                .hooks
                .as_ref()
                .is_some_and(|hooks| matches!(hooks.state.as_str(), "missing" | "realDir"));
            let lock_open = agent
                .lock
                .as_ref()
                .is_some_and(|lock| matches!(lock.state.as_str(), "missing" | "realFile"));
            if agent.detected && (hooks_open || lock_open) {
                vec!["hooks"]
            } else {
                Vec::new()
            }
        }
    }
}

/// How many fixes an agent row advertises ("3 to fix"): the plan groups that have a change for it.
pub(crate) fn issue_count(agent: &SyncAgentReport) -> usize {
    SYNC_PARTS
        .iter()
        .map(|part| part_fix_groups(agent, *part).len())
        .sum()
}

/// Red when something is broken, amber when it works today but can go out of date.
pub(crate) fn worst_tone(agent: &SyncAgentReport) -> SyncTone {
    let tones = [
        skills_tone(agent),
        instructions_tone(agent),
        hooks_tone(agent),
    ];
    if tones.contains(&SyncTone::Err) {
        SyncTone::Err
    } else if tones.contains(&SyncTone::Warn) || agent.status == "attention" {
        SyncTone::Warn
    } else {
        SyncTone::Ok
    }
}

/// Which of the three shared things a problem belongs to; the shared folder's own problems have none.
pub(crate) fn problem_part(kind: &str) -> Option<SyncPart> {
    match kind {
        "danglingLinks" | "copiedFolders" | "wholeFolderLinks" | "redundantLinks"
        | "backupFolders" => Some(SyncPart::Skills),
        "missingPointers" | "danglingPointers" | "missingMainMd" => Some(SyncPart::Instructions),
        _ => None,
    }
}

pub(crate) fn skill_entry_label(state: &str) -> &'static str {
    match state {
        "copyDrifted" => "Copy, differs from the source",
        "copyIdentical" => "Copy, same content as the source",
        "dangling" => "Broken link",
        "linked" => "Linked",
        "linkedElsewhere" => "Linked to another folder",
        "missing" => "Not linked yet",
        "onlyHere" => "Only here",
        "viaWholeFolder" => "Available through the folder link",
        _ => "",
    }
}

pub(crate) fn skill_entry_tone(state: &str) -> SyncTone {
    match state {
        "linked" | "viaWholeFolder" => SyncTone::Ok,
        "dangling" => SyncTone::Err,
        "copyIdentical" | "copyDrifted" => SyncTone::Warn,
        _ => SyncTone::Off,
    }
}

/// The per-agent skill list's buckets, in display order.
pub(crate) const SKILL_BUCKETS: [(&str, &[&str], Option<&str>); 5] = [
    ("Broken", &["dangling"], None),
    (
        "Copies of a shared skill",
        &["copyIdentical", "copyDrifted"],
        None,
    ),
    (
        "Only here",
        &["onlyHere", "linkedElsewhere"],
        Some("not in the shared folder"),
    ),
    ("Linked", &["linked", "viaWholeFolder"], None),
    (
        "Missing here",
        &["missing"],
        Some("in the shared folder, not linked yet"),
    ),
];

pub(crate) fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// `toLocaleString()` for the counts the sheet prints: thousands separated by commas.
pub(crate) fn grouped(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A problem row: what is wrong in everyday words, why it matters, and the steps the fix takes.
///
/// CDXC:AgentSync 2026-09-22 DECISION:
/// The user asked for the Agent Sync page to be "less crowded" and "more organized and clear UX wise" because the first version was "very complicated". Problem rows therefore say what is wrong and why it matters in everyday words (no "drift", "dangling symlinks", "pointers", "prune"), and the paths and the exact fix only appear when a row is opened. The scanner's own titles stay as they are for `ghostex agent-sync status`.
pub(crate) struct ProblemCopy {
    pub(crate) steps: Vec<String>,
    pub(crate) sub: String,
    pub(crate) title: String,
    pub(crate) tone: SyncTone,
}

pub(crate) fn problem_copy(problem: &SyncProblem, report: &SyncReport) -> ProblemCopy {
    let count = problem.count;
    let one = count == 1;
    match problem.kind.as_str() {
        "danglingLinks" => ProblemCopy {
            steps: vec![
                "The dead links are removed.".to_string(),
                "The skills they pointed to are already gone, so nothing else changes.".to_string(),
            ],
            sub: "Left behind when skills were renamed or deleted. Safe to remove.".to_string(),
            title: format!(
                "{} to skills that no longer exist",
                plural(count, "link points", "links point")
            ),
            tone: SyncTone::Err,
        },
        "sourceBrokenLinks" => ProblemCopy {
            steps: vec![
                "The broken link is removed from your shared folder.".to_string(),
                "If you still want that skill, install it again afterwards.".to_string(),
            ],
            sub: "A link inside the shared folder that leads nowhere.".to_string(),
            title: format!(
                "{} in your shared folder {} broken",
                plural(count, "skill", "skills"),
                if one { "is" } else { "are" }
            ),
            tone: SyncTone::Err,
        },
        "copiedFolders" => {
            let edited: usize = report
                .agents
                .iter()
                .map(|agent| agent.skills.as_ref().map_or(0, |s| s.counts.copies_drifted))
                .sum();
            let second = if edited > 0 {
                let it = if edited == 1 { "it is" } else { "they are" };
                format!(
                    "{} edited: {it} kept as {it}.",
                    plural(edited, "copy was", "copies were")
                )
            } else {
                "A copy that was edited is never replaced.".to_string()
            };
            ProblemCopy {
                steps: vec![
                    "Copies that are identical to the original are backed up, then replaced with a link."
                        .to_string(),
                    second,
                    "Nothing is deleted. Backups go to Ghostex's own folder, outside every agent's skills folder."
                        .to_string(),
                ],
                sub: "Copies go out of date when you edit the original.".to_string(),
                title: format!(
                    "{} instead of {}",
                    plural(count, "skill is a copy", "skills are copies"),
                    if one { "a link" } else { "links" }
                ),
                tone: SyncTone::Warn,
            }
        }
        "redundantLinks" => ProblemCopy {
            steps: vec![
                "The links are removed from those agents' own skills folders.".to_string(),
                "The skills stay in ~/.agents/skills, which these agents read anyway.".to_string(),
            ],
            sub: "These agents read your shared folder themselves, so each linked skill loads twice."
                .to_string(),
            title: format!(
                "{} that these agents do not need",
                plural(count, "skill link", "skill links")
            ),
            tone: SyncTone::Warn,
        },
        "backupFolders" => ProblemCopy {
            steps: vec![
                "Each backup folder is moved to Ghostex's own backup folder, at the same path it had under your home folder."
                    .to_string(),
                "Nothing is deleted.".to_string(),
            ],
            sub: "Agents read a backup inside a skills folder as one more skill.".to_string(),
            title: format!(
                "{} inside agents' skills folders",
                plural(count, "old backup sits", "old backups sit")
            ),
            tone: SyncTone::Warn,
        },
        "wholeFolderLinks" => ProblemCopy {
            steps: vec![
                "The folder link is replaced with a real folder.".to_string(),
                "Every shared skill gets its own link inside it, so the agent sees the same skills as before."
                    .to_string(),
                "Skills that only this agent uses can then live next to them.".to_string(),
            ],
            sub: "Works today, but leaves no room for skills only that agent uses.".to_string(),
            title: format!(
                "{} the whole skills folder",
                plural(count, "agent links", "agents link")
            ),
            tone: SyncTone::Warn,
        },
        "danglingPointers" => {
            let reason = if report.source.main_md_empty {
                "empty"
            } else {
                "missing"
            };
            let first_step = match report.source.main_md_seed_candidates.first() {
                Some(best) => format!(
                    "Create ~/.agents/main.md from {} ({}) as the first step.",
                    best.display_name, best.path
                ),
                None => {
                    "Create ~/.agents/main.md with your shared instructions, or restore your original files from Ghostex's backup folder."
                        .to_string()
                }
            };
            ProblemCopy {
                steps: vec![
                    first_step,
                    "Sync will not write new instruction pointers until ~/.agents/main.md has content."
                        .to_string(),
                ],
                sub: format!(
                    "These agents are told to read ~/.agents/main.md, which is {reason}."
                ),
                title: format!(
                    "{} to a {reason} shared instructions file",
                    plural(count, "agent points", "agents point")
                ),
                tone: SyncTone::Err,
            }
        }
        "missingMainMd" => {
            let reason = if report.source.main_md_empty {
                "empty"
            } else {
                "missing"
            };
            let first_step = match report.source.main_md_seed_candidates.first() {
                Some(best) => format!(
                    "Create ~/.agents/main.md from {} ({}) first.",
                    best.display_name, best.path
                ),
                None => {
                    "Add your shared instructions to ~/.agents/main.md before linking agent instruction files."
                        .to_string()
                }
            };
            ProblemCopy {
                steps: vec![
                    first_step,
                    "Existing agent instruction files are never replaced while ~/.agents/main.md is missing or empty."
                        .to_string(),
                ],
                sub: "Agent Sync waits to write instruction pointers until ~/.agents/main.md has content."
                    .to_string(),
                title: format!("Shared instructions file (~/.agents/main.md) is {reason}"),
                tone: SyncTone::Warn,
            }
        }
        "missingPointers" => ProblemCopy {
            steps: vec![
                "Each agent gets a one-line instruction file that tells it to read ~/.agents/main.md."
                    .to_string(),
                "A file that already has other content is first moved to Ghostex's backup folder (or you can prepend or leave it alone in the plan)."
                    .to_string(),
            ],
            sub: "Their instruction file does not mention ~/.agents/main.md.".to_string(),
            title: format!(
                "{} not read your shared instructions",
                plural(count, "agent does", "agents do")
            ),
            tone: SyncTone::Warn,
        },
        _ => ProblemCopy {
            steps: Vec::new(),
            sub: problem.detail.clone(),
            title: problem.title.clone(),
            tone: SyncTone::Warn,
        },
    }
}

/// One agent a problem touches and how many of its things are affected.
pub(crate) struct ProblemAgentCount<'a> {
    pub(crate) agent: &'a SyncAgentReport,
    pub(crate) count: usize,
    pub(crate) unit: &'static str,
}

fn agent_problem_count(agent: &SyncAgentReport, kind: &str) -> usize {
    let skills = agent.skills.as_ref();
    match kind {
        "danglingLinks" => skills.map_or(0, |s| s.counts.dangling),
        "redundantLinks" => skills.map_or(0, |s| s.counts.redundant_links),
        "backupFolders" => skills.map_or(0, |s| s.counts.backups),
        "copiedFolders" => {
            skills.map_or(0, |s| s.counts.copies_identical + s.counts.copies_drifted)
        }
        "wholeFolderLinks" => usize::from(
            !agent.universal && skills.is_some_and(|s| s.dir_state == "wholeFolderLink"),
        ),
        "danglingPointers" => {
            let state = agent
                .instructions
                .as_ref()
                .map(|value| value.state.as_str());
            usize::from(agent.detected && state == Some("danglingPointer"))
        }
        "missingMainMd" => {
            let state = agent
                .instructions
                .as_ref()
                .map(|value| value.state.as_str());
            usize::from(
                agent.detected
                    && matches!(state, Some("missing" | "legacyPointer" | "otherContent")),
            )
        }
        "missingPointers" => {
            let state = agent
                .instructions
                .as_ref()
                .map(|value| value.state.as_str());
            usize::from(
                agent.detected
                    && matches!(state, Some("missing" | "legacyPointer" | "otherContent")),
            )
        }
        _ => 0,
    }
}

/// The agents a problem touches, largest first. Problems of the shared folder itself return none.
pub(crate) fn problem_agents<'a>(
    problem: &SyncProblem,
    report: &'a SyncReport,
) -> Vec<ProblemAgentCount<'a>> {
    let unit = match problem.kind.as_str() {
        "danglingLinks" | "redundantLinks" => "link",
        "copiedFolders" => "skill",
        "backupFolders" => "backup",
        _ => "",
    };
    let mut agents: Vec<ProblemAgentCount<'a>> = report
        .agents
        .iter()
        .map(|agent| ProblemAgentCount {
            agent,
            count: agent_problem_count(agent, &problem.kind),
            unit,
        })
        .filter(|entry| entry.count > 0)
        .collect();
    agents.sort_by(|a, b| b.count.cmp(&a.count));
    agents
}

fn shell_quote(path: &str) -> String {
    let unsafe_char =
        |ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'));
    if let Some(rest) = path.strip_prefix("~/") {
        if rest.chars().any(unsafe_char) {
            let mut escaped = String::new();
            for ch in rest.chars() {
                if matches!(ch, '"' | '$' | '`' | '\\') {
                    escaped.push('\\');
                }
                escaped.push(ch);
            }
            return format!("\"$HOME/{escaped}\"");
        }
        return format!("~/{rest}");
    }
    if path.chars().any(unsafe_char) {
        format!("'{}'", path.replace('\'', "'\\''"))
    } else {
        path.to_string()
    }
}

/// CDXC:AgentSync 2026-09-16 WHY:
/// The plan doubles as a shell script so the same operations can be read, kept, or run on
/// another computer; it emits only ln, mv, rm, mkdir and heredocs, never rm -r.
pub(crate) fn plan_to_shell_script(plan: &SyncPlan, enabled: &[String]) -> String {
    let mut lines = vec![
        "#!/usr/bin/env bash".to_string(),
        "# Ghostex Agent Sync plan".to_string(),
        format!("# scope: {}, backups go to {}", plan.scope, plan.backup_dir),
        "set -euo pipefail".to_string(),
        String::new(),
    ];
    let mut drops: Vec<String> = Vec::new();
    for group in &plan.groups {
        if !enabled.contains(&group.kind) {
            continue;
        }
        lines.push(format!("# {}", group.title));
        for op in &group.ops {
            match op.verb.as_str() {
                "unlink" => lines.push(format!("rm {}", shell_quote(&op.path))),
                "mkdir" => lines.push(format!("mkdir -p {}", shell_quote(&op.path))),
                "backup" => {
                    let fallback = format!("{}.bak", op.path);
                    let target = op.target.as_deref().unwrap_or(&fallback);
                    if let Some((parent, _)) = target.rsplit_once('/') {
                        lines.push(format!("mkdir -p {}", shell_quote(parent)));
                    }
                    lines.push(format!(
                        "mv {} {}",
                        shell_quote(&op.path),
                        shell_quote(target)
                    ));
                }
                "link" => lines.push(format!(
                    "ln -s {} {}",
                    shell_quote(op.target.as_deref().unwrap_or("")),
                    shell_quote(&op.path)
                )),
                "write" => {
                    lines.push(format!("mkdir -p \"$(dirname {})\"", shell_quote(&op.path)));
                    lines.push(format!("cat > {} <<'GHOSTEX_EOF'", shell_quote(&op.path)));
                    let content = op.content.as_deref().unwrap_or("");
                    lines.push(content.strip_suffix('\n').unwrap_or(content).to_string());
                    lines.push("GHOSTEX_EOF".to_string());
                }
                "drop" => {
                    if let Some(target) = op.target.as_ref() {
                        drops.push(target.clone());
                    }
                }
                _ => {}
            }
        }
        lines.push(String::new());
    }
    if !drops.is_empty() {
        lines.push("# Prune stale lock entries (needs jq)".to_string());
        lines.push(format!(
            "jq 'del(.skills[\"{}\"])' ~/.agents/.skill-lock.json > ~/.agents/.skill-lock.json.tmp && mv ~/.agents/.skill-lock.json.tmp ~/.agents/.skill-lock.json",
            drops.join("\"], .skills[\"")
        ));
        lines.push(String::new());
    }
    lines.join("\n")
}

/// Expands the report's `~/` display paths for native commands that need an absolute path.
pub(crate) fn expand_home_path(path: &str, home: &str) -> String {
    if path == "~" {
        return home.to_string();
    }
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => rest
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .fold(std::path::PathBuf::from(home), |acc, part| acc.join(part))
            .display()
            .to_string(),
        None => path.to_string(),
    }
}

//! Agent Sync: keep one source of truth in `~/.agents` (skills, instruction
//! markdown, hook scripts) and point every agent on the computer at it through
//! per-skill symlinks and one-line pointer files.
//!
//! CDXC:AgentSync 2026-09-16 DECISION:
//! User: "always per-skill like the CLI" (1B). Every agent skills folder gets one relative
//! symlink per source skill; whole-folder links are converted into real folders of links.
//! User: an entry file with other content is backed up and replaced by the pointer (2B).
//! User: `ghostex agent-sync` ships together with the Hub tab and shares this crate (3A).
//! User: pruning `.skill-lock.json` is an opt-in plan group, off by default (4A).
//! User: gxserver bundled skill installs link into agents instead of copying (5A).
//! SEE-ALSO: apps/desktop/src/app/helpers/agents_hub/sync.rs, server/src/ghostex_cli/agent_sync.rs,
//! packages/shared/agent-sync.ts (deleted 2026-10-01), packages/core-ui/agents-hub-sync/.

pub mod apply;
pub mod catalog;
pub mod fsx;
pub mod plan;
pub mod pointer;
pub mod scan;

pub use apply::{apply, ApplyFailure, ApplyResult};
pub use catalog::{agent_catalog, AgentKind, AgentSpec, InstructionKind, SOURCE_DIR_NAME};
pub use plan::{
    build_plan, resolve_seed_source, PlanGroup, PlanGroupKind, PlanOp, PlanOptions, PlanSummary,
    PlanVerb, PointerPolicy, ResolvedSeed, SyncPlan,
};
pub use scan::{
    scan, AgentReport, AgentStatus, HooksReport, HooksState, InstructionReport, InstructionState,
    LockLinkReport, MainMdSeedCandidate, Problem, ProblemKind, ReportSummary, SkillDirState,
    SkillEntry, SkillEntryState, SkillsReport, SourceInfo, SyncReport,
};

/// The scope of a plan or apply run: every detected agent, or one agent id
/// (`claude-code`, `codex:.playwright`, ...).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncScope {
    All,
    Agent(String),
}

impl SyncScope {
    pub fn parse(value: Option<&str>) -> SyncScope {
        match value.map(str::trim) {
            None | Some("") | Some("all") => SyncScope::All,
            Some(id) => SyncScope::Agent(id.to_string()),
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            SyncScope::All => "all".to_string(),
            SyncScope::Agent(id) => id.clone(),
        }
    }
}

/// ISO-8601 UTC timestamp with millisecond precision, e.g. `2026-09-16T12:34:56.789Z`.
/// Hand-rolled so the crate stays dependency-free for both host binaries.
pub fn now_iso8601() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60,
        duration.subsec_millis()
    )
}

/// Compact stamp for backup names: `20260916-1234`.
pub fn now_backup_stamp() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}",
        rem / 3600,
        (rem % 3600) / 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

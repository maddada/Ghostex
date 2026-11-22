//! CDXC:AgentHooks 2026-09-16 DECISION:
//! User: when Ghostex installs its Claude Code hooks, configure Claude Code so it stops deleting conversations after a month.
//! Claude Code prunes transcripts under `~/.claude/projects` whose last activity is older than `cleanupPeriodDays` (30 by default), and a pruned transcript can neither be reopened in chat nor resumed.
//! A value the user already set is kept as is, and uninstalling the hooks leaves the key in place because removing it would silently turn the deletion back on.
use serde_json::{json, Value};

/// One hundred years: Claude Code has no documented "never" value, so the retention window is pushed past any realistic lifetime of the machine.
pub(crate) const CLAUDE_TRANSCRIPT_RETENTION_DAYS: u64 = 36_500;

pub(crate) fn ensure_claude_transcript_retention(data: &mut Value) -> bool {
    let Some(object) = data.as_object_mut() else {
        return false;
    };
    if object.contains_key("cleanupPeriodDays") {
        return false;
    }
    object.insert(
        "cleanupPeriodDays".to_string(),
        json!(CLAUDE_TRANSCRIPT_RETENTION_DAYS),
    );
    true
}

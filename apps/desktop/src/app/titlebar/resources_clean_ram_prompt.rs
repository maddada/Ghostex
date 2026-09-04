//! The prompt the Resources panel's Clean RAM button copies to the clipboard.
//!
//! CDXC:Resources 2026-09-04 DECISION:
//! User: Clean RAM copies "a prompt to diagnose the ram use for the user",
//! the same investigation an agent ran by hand on 2026-09-04: break the
//! panel's total down by process, separate agents from the MCP helpers they
//! spawn, say which idle sessions Sleep Inactive would free, and rank the
//! fixes. The prompt carries the panel's own rows so the agent starts from
//! what the user saw, and points at `ghostex resources --json` for fresh
//! per-pid data.

use crate::app::model::*;
use crate::app::window::titlebar_panels::{
    format_gpui_resource_cpu_compact, format_gpui_resource_memory_compact,
};

const CLEAN_RAM_PROMPT_MAX_ROWS_PER_SECTION: usize = 40;
const CLEAN_RAM_PROMPT_MAX_LABEL_CHARS: usize = 60;

pub(crate) fn gpui_resources_clean_ram_prompt(snapshot: &GpuiNativeResourcesSnapshot) -> String {
    let mut prompt = String::new();
    prompt.push_str(&format!(
        "Ghostex is using {} of RAM on this computer (CPU {}). Please find out where that memory goes and help me bring it down.\n\n",
        format_gpui_resource_memory_compact(snapshot.total_memory_mb),
        format_gpui_resource_cpu_compact(snapshot.total_cpu),
    ));
    prompt.push_str("What the Ghostex Resources panel showed when I clicked Clean RAM:\n");
    prompt.push_str(&format!(
        "- Total across every process Ghostex started: RAM {}, CPU {}. Idle sessions that Sleep Inactive can put to sleep right now: {}.\n",
        format_gpui_resource_memory_compact(snapshot.total_memory_mb),
        format_gpui_resource_cpu_compact(snapshot.total_cpu),
        snapshot.inactive_terminal_sleep_count,
    ));
    for (label, rows) in [
        ("Dev servers".to_string(), &snapshot.server_rows),
        (
            format!("{} sessions", snapshot.project_label),
            &snapshot.session_rows,
        ),
        ("Code IDE".to_string(), &snapshot.code_rows),
        ("Browser".to_string(), &snapshot.browser_rows),
        ("Orphaned / detached".to_string(), &snapshot.orphan_rows),
    ] {
        if rows.is_empty() {
            continue;
        }
        let memory_mb = rows.iter().map(|row| row.memory_mb).sum::<f64>();
        prompt.push_str(&format!(
            "- {label}: {} over {} rows\n",
            format_gpui_resource_memory_compact(memory_mb),
            rows.len(),
        ));
        for row in rows.iter().take(CLEAN_RAM_PROMPT_MAX_ROWS_PER_SECTION) {
            prompt.push_str(&format!(
                "  - {} ({}): {}, {} processes{}\n",
                truncate_prompt_label(&row.label),
                row.detail,
                format_gpui_resource_memory_compact(row.memory_mb),
                row.pids.len(),
                if row.sleep_candidate { ", idle" } else { "" },
            ));
        }
        if rows.len() > CLEAN_RAM_PROMPT_MAX_ROWS_PER_SECTION {
            prompt.push_str(&format!(
                "  - and {} more rows\n",
                rows.len() - CLEAN_RAM_PROMPT_MAX_ROWS_PER_SECTION
            ));
        }
    }
    prompt.push_str(
        "\nPlease do this:\n\
1. Run `ghostex resources --json` for a fresh snapshot with one entry per pid (memoryMb is the process footprint, the same number Activity Monitor's Memory column shows on macOS). `ghostex resources` prints the readable version. Do not sleep, kill, or restart anything without asking me first.\n\
2. Group the total by what the processes are: the agents themselves (claude, codex, and similar), the MCP servers and wrappers each agent spawns (npm/npx wrappers, chrome-devtools-mcp and its telemetry watchdog, cua-driver), the Ghostex app and its CEF helper processes, gxserver, extension servers, and dev servers. Use `ps -axo pid,ppid,rss,command` to walk parent/child chains, and on macOS `top -l 1 -stats pid,command,mem,cmprs -o mem -n 40` to see how much is compressed rather than resident.\n\
3. Tell me which sessions are idle and how much RAM sleeping them would free. Sleep Inactive in the Resources panel does that on demand; Auto Sleep for agents in Settings does it automatically and is off by default.\n\
4. Point out per-session overhead I can cut in my agent's MCP configuration, for example `npx <package>@latest` wrappers that stay resident, telemetry side processes, or the same server started once per session, and give me the exact configuration change.\n\
5. Finish with a short ranked list: what to do, roughly how much RAM each step saves, and what it costs me (for example, a slept session has to resume before it can continue).\n",
    );
    prompt
}

fn truncate_prompt_label(label: &str) -> String {
    if label.chars().count() <= CLEAN_RAM_PROMPT_MAX_LABEL_CHARS {
        return label.to_string();
    }
    let mut out = label
        .chars()
        .take(CLEAN_RAM_PROMPT_MAX_LABEL_CHARS - 1)
        .collect::<String>();
    out.push('…');
    out
}

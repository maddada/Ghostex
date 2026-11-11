//! What a session's terminal runs in its foreground: only a shell (its prompt), or a command
//! started there. The agent-gone idle write reads it last, so an agent the process scan does not
//! recognise (started through a wrapper it has no name for) keeps its working state.

#[cfg(all(unix, not(target_os = "macos")))]
use std::collections::{HashMap, HashSet, VecDeque};

#[cfg(unix)]
use super::*;

/// Shells, by the name their command line starts with (a login shell's leading `-` dropped).
#[cfg(unix)]
const SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "mksh", "tcsh", "csh", "nu", "pwsh", "login",
];

#[cfg(unix)]
fn is_shell_command(command: &str) -> bool {
    let program = command.split_whitespace().next().unwrap_or_default();
    let name = program.rsplit('/').next().unwrap_or(program);
    SHELLS.contains(&name.trim_start_matches('-'))
}

/*
CDXC:SessionStatus 2026-11-11 FIX:
User (Q2274, on GitHub PR #551): "cover agents started through a wrapper Ghostex does not recognise". The idle write for a killed agent (session_chat_agent_gone_activity.rs) trusted the process scan, which only knows agents by their names (or a wrapper that names one), so an agent run through anything else, with a shell prompt left on screen, was set idle while it worked, until its next hook set it back. The terminal knows better: while a command runs it is the terminal's foreground process group, and at the prompt the shell is. So the write now also needs the foreground to be shells only; anything else in front (a wrapper, a script, an agent under another name) keeps the working state. A read that fails says nothing, and the write goes on as before.
*/
/// `Some(true)` when the session's terminal runs something other than a shell in its foreground,
/// `Some(false)` when only shells are in front (the prompt), `None` when it can't be read
/// (Windows, a session zmx does not list, a process table that does not answer).
pub(crate) fn zmx_session_runs_foreground_command(zmx_name: &str) -> Option<bool> {
    #[cfg(unix)]
    {
        let commands = foreground_commands(zmx_name)?;
        if commands.is_empty() {
            return None;
        }
        Some(commands.iter().any(|command| !is_shell_command(command)))
    }
    #[cfg(not(unix))]
    {
        let _ = zmx_name;
        None
    }
}

/// The command lines of the session's processes that are in their terminal's foreground group.
#[cfg(unix)]
fn foreground_commands(zmx_name: &str) -> Option<Vec<String>> {
    let zmx = require_zmx().ok()?;
    let result = run_zmx_probe_script(
        super::provider::build_zmx_process_snapshot_command(&zmx.executable_path),
        ZmxCommandOptions {
            stdout_limit_bytes: Some(GXSERVER_ZMX_PROCESS_SNAPSHOT_STDOUT_LIMIT_BYTES),
            ..ZmxCommandOptions::default()
        },
    )
    .ok()?;
    if result.exit_code != 0 {
        return None;
    }
    let names = [zmx_name.to_string()];
    #[cfg(target_os = "macos")]
    {
        let root = *super::provider::parse_zmx_root_pids(&result.stdout, &names).get(zmx_name)?;
        let rows = super::process_table_macos::process_tree_rows(&[root]);
        Some(
            rows.iter()
                .filter(|row| {
                    super::process_table_macos::process_group_and_foreground(row.pid())
                        .is_some_and(|(group, foreground)| group == foreground)
                })
                .map(|row| row.command().to_string())
                .collect(),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let (ps_output, zmx_list) =
            super::provider::parse_zmx_process_snapshot_sections(&result.stdout);
        let root = *super::provider::parse_zmx_root_pids(&zmx_list, &names).get(zmx_name)?;
        let rows = parse_process_rows(&ps_output);
        let mut children = HashMap::<i64, Vec<usize>>::new();
        for (index, row) in rows.iter().enumerate() {
            children.entry(row.ppid()).or_default().push(index);
        }
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([root]);
        let mut commands = Vec::new();
        while let Some(pid) = queue.pop_front() {
            if !seen.insert(pid) || seen.len() > 4096 {
                continue;
            }
            for &index in children.get(&pid).map(Vec::as_slice).unwrap_or_default() {
                let row = &rows[index];
                if linux_group_and_foreground(row.pid())
                    .is_some_and(|(group, foreground)| group == foreground)
                {
                    commands.push(row.command().to_string());
                }
                queue.push_back(row.pid());
            }
        }
        Some(commands)
    }
}

/// `/proc/<pid>/stat`'s process group and terminal foreground group (fields 5 and 8).
#[cfg(all(unix, not(target_os = "macos")))]
fn linux_group_and_foreground(pid: i64) -> Option<(i64, i64)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name in parentheses may hold spaces; the fields after it are plain.
    let rest = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After ")": state(3) ppid(4) pgrp(5) session(6) tty_nr(7) tpgid(8).
    let group = fields.get(2)?.parse::<i64>().ok()?;
    let foreground = fields.get(5)?.parse::<i64>().ok()?;
    (foreground > 0).then_some((group, foreground))
}

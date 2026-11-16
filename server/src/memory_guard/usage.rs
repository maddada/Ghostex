//! One session's memory: its agent and each command the agent started, and the guard's next step.

use std::collections::{HashMap, HashSet};

use super::processes::{memory_bytes, Member, ProcessRow};

/// A session smaller than this is never touched: the memory is going somewhere else.
const MIN_SESSION_BYTES: u64 = 2 << 30;
/// A command (or the agent itself) smaller than this is not worth stopping on its own.
const MIN_PART_BYTES: u64 = 512 << 20;

/// A debug build (the throwaway `cargo xtask test-server`) may lower a threshold through
/// `GHOSTEX_MEMORY_GUARD_TEST_<name>_MB`, so a test reaches every step with small processes.
/// Release builds always use the thresholds above.
fn threshold(name: &str, bytes: u64) -> u64 {
    #[cfg(debug_assertions)]
    if let Some(mb) = std::env::var(format!("GHOSTEX_MEMORY_GUARD_TEST_{name}_MB"))
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        return mb << 20;
    }
    let _ = name;
    bytes
}

/// The smallest session the guard acts on.
pub(crate) fn min_session_bytes() -> u64 {
    threshold("MIN_SESSION", MIN_SESSION_BYTES)
}

fn min_part_bytes() -> u64 {
    threshold("MIN_PART", MIN_PART_BYTES)
}

/// One command the agent started, with everything below it, or a process of the session that
/// left its tree (a detached dev server).
#[derive(Clone, Debug)]
pub(crate) struct ChildGroup {
    /// A short name for it ("vitest", "next", "node").
    pub(crate) label: String,
    /// The command line of its biggest process.
    pub(crate) command: String,
    pub(crate) bytes: u64,
    pub(crate) members: Vec<Member>,
}

#[derive(Clone, Debug)]
pub(crate) struct SessionUsage {
    /// Every process of the session together.
    pub(crate) total: u64,
    pub(crate) agent_pid: Option<u32>,
    /// The agent process alone.
    pub(crate) agent_bytes: u64,
    /// The agent's commands and the session's detached processes, biggest first.
    pub(crate) groups: Vec<ChildGroup>,
    /// Every process of the session, the daemon and the agent included.
    pub(crate) members: Vec<Member>,
}

/// What the guard does to the heaviest session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Stop that one command (an index into `groups`); the agent keeps running and is told.
    StopCommand(usize),
    /// The agent itself is the heavy one: restart it on the same conversation.
    RestartAgent,
    /// Stop the whole session; the user resumes it.
    StopSession,
}

impl SessionUsage {
    /// The smallest step that frees this session's memory. `escalate` is set once the guard
    /// already acted on this session and the computer is still critical; `restarted_recently`
    /// once its agent was restarted for memory in the last ten minutes, so a leaking agent is
    /// never restarted in a loop.
    pub(crate) fn next_step(&self, escalate: bool, restarted_recently: bool) -> Step {
        if escalate || self.agent_pid.is_none() {
            return Step::StopSession;
        }
        if let Some(group) = self.groups.first() {
            if group.bytes >= min_part_bytes() && group.bytes >= self.agent_bytes {
                return Step::StopCommand(0);
            }
        }
        if self.agent_bytes >= min_part_bytes() && !restarted_recently {
            return Step::RestartAgent;
        }
        Step::StopSession
    }
}

/// The executable's name in a command line: `vitest` for `node /x/.bin/vitest run`, `next` for
/// `node …/next dev`, else the program itself.
fn label_of(command: &str) -> String {
    let mut words = command.split_whitespace();
    let program = words
        .next()
        .map(|word| word.rsplit('/').next().unwrap_or(word))
        .unwrap_or("");
    let interpreted = matches!(
        program,
        "node" | "bun" | "deno" | "python" | "python3" | "ruby" | "java" | "sh" | "bash" | "zsh"
    );
    if interpreted {
        if let Some(script) = words.find(|word| !word.starts_with('-')) {
            let name = script.rsplit('/').next().unwrap_or(script);
            let name = name
                .trim_end_matches(".js")
                .trim_end_matches(".mjs")
                .trim_end_matches(".cjs")
                .trim_end_matches(".py");
            if !name.is_empty() && name != "cli" && name != "index" {
                return name.to_string();
            }
        }
    }
    if program.is_empty() {
        "a command".to_string()
    } else {
        program.to_string()
    }
}

fn is_daemon_of(command: &str, zmx_name: &str) -> bool {
    let mut words = command.split_whitespace();
    let Some(program) = words.next() else {
        return false;
    };
    let program = program.rsplit('/').next().unwrap_or(program);
    matches!(program, "zmx" | "zmx.exe")
        && words.next() == Some("run")
        && words.next() == Some(zmx_name)
}

/// Adds `root` and everything below it to `into`, once each.
fn descend<'a>(
    children: &HashMap<u32, Vec<&'a ProcessRow>>,
    root: &'a ProcessRow,
    into: &mut Vec<&'a ProcessRow>,
    seen: &mut HashSet<u32>,
) {
    let mut queue = vec![root];
    while let Some(row) = queue.pop() {
        if seen.insert(row.pid) {
            into.push(row);
            queue.extend(children.get(&row.pid).into_iter().flatten().copied());
        }
    }
}

/// `zmx_name`'s usage from one scan: the processes under its daemon, the processes tagged with
/// its name and everything below them. `None` when nothing of it is running.
pub(crate) fn session_usage(
    rows: &[ProcessRow],
    zmx_name: &str,
    agent_pid: Option<u32>,
) -> Option<SessionUsage> {
    let mut children: HashMap<u32, Vec<&ProcessRow>> = HashMap::new();
    for row in rows {
        children.entry(row.ppid).or_default().push(row);
    }
    let mut seen = HashSet::new();
    let mut tree = Vec::new();
    for root in rows
        .iter()
        .filter(|row| is_daemon_of(&row.command, zmx_name))
    {
        descend(&children, root, &mut tree, &mut seen);
    }
    // Detached processes that kept the session's tag, each with what runs below it.
    let mut detached = Vec::new();
    for row in rows
        .iter()
        .filter(|row| row.session_tag.as_deref() == Some(zmx_name))
    {
        if seen.contains(&row.pid) {
            continue;
        }
        let mut group = Vec::new();
        descend(&children, row, &mut group, &mut seen);
        detached.push(group);
    }
    if tree.is_empty() && detached.is_empty() {
        return None;
    }
    let sizes: HashMap<u32, u64> = tree
        .iter()
        .chain(detached.iter().flatten())
        .map(|row| (row.pid, memory_bytes(row.pid)))
        .collect();
    let group_of = |rows: Vec<&ProcessRow>| -> ChildGroup {
        let biggest = rows
            .iter()
            .max_by_key(|row| sizes.get(&row.pid).copied().unwrap_or(0))
            .copied();
        ChildGroup {
            label: biggest.map_or_else(|| "a command".to_string(), |row| label_of(&row.command)),
            command: biggest.map(|row| row.command.clone()).unwrap_or_default(),
            bytes: rows
                .iter()
                .map(|row| sizes.get(&row.pid).copied().unwrap_or(0))
                .sum(),
            members: rows
                .iter()
                .map(|row| Member {
                    pid: row.pid,
                    start: row.start,
                })
                .collect(),
        }
    };
    // An agent gxserver cannot name (a custom agent) is the program the session's shell runs:
    // the earliest started child of the daemon's shell.
    let agent = agent_pid
        .filter(|pid| tree.iter().any(|row| row.pid == *pid))
        .or_else(|| {
            let daemon = rows
                .iter()
                .find(|row| is_daemon_of(&row.command, zmx_name))?;
            let shell = children
                .get(&daemon.pid)?
                .iter()
                .min_by_key(|row| row.start)?;
            Some(
                children
                    .get(&shell.pid)?
                    .iter()
                    .min_by_key(|row| row.start)?
                    .pid,
            )
        });
    let mut groups = Vec::new();
    if let Some(agent) = agent {
        for child in children.get(&agent).into_iter().flatten() {
            let mut rows = Vec::new();
            descend(&children, child, &mut rows, &mut HashSet::new());
            groups.push(group_of(rows));
        }
    }
    groups.extend(detached.iter().cloned().map(group_of));
    groups.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    Some(SessionUsage {
        total: sizes.values().sum(),
        agent_pid: agent,
        agent_bytes: agent.and_then(|pid| sizes.get(&pid).copied()).unwrap_or(0),
        groups,
        members: tree
            .iter()
            .chain(detached.iter().flatten())
            .map(|row| Member {
                pid: row.pid,
                start: row.start,
            })
            .collect(),
    })
}

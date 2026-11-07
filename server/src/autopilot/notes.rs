//! The supervisor's notes: a folder under gxserver's state that every wake of the judgment agent
//! reads first, so a new agent (or the same one after a restart) picks up where the last left off.
//!
//! - `README.md`: how the supervisor works (written by Ghostex, the agent's playbook).
//! - `notes.md`: the running notes across wakes, which the agent keeps.
//! - `sessions/<projectId>-<sessionId>.md`: what it knows about one session, which it keeps.
//! - `wakes/<n>.md`: what Ghostex asks of one wake (the sessions and what it saw).
//!
//! CDXC:Autopilot 2026-11-07 DECISION:
//! User: "i want the watcher thread to write notes pls so if another one wakes up again it knows what's up and doesn't start from scratch": a running notes file across sweeps and one per session, each read first on every wake.

use std::path::{Path, PathBuf};

use crate::paths::GxserverPaths;

pub fn notes_dir(paths: &GxserverPaths) -> PathBuf {
    paths.app_state_dir.join("autopilot")
}

/// Makes the folder and its files when missing, and writes the playbook every time (it changes
/// with Ghostex, the notes do not).
pub fn ensure_notes(paths: &GxserverPaths) -> std::io::Result<PathBuf> {
    let dir = notes_dir(paths);
    std::fs::create_dir_all(dir.join("sessions"))?;
    std::fs::create_dir_all(dir.join("wakes"))?;
    std::fs::write(dir.join("README.md"), PLAYBOOK)?;
    let notes = dir.join("notes.md");
    if !notes.exists() {
        std::fs::write(
            &notes,
            "# Autopilot notes\n\nWhat earlier wakes learned and decided, newest at the bottom. Keep it short: one dated line per decision, and what to watch next.\n",
        )?;
    }
    Ok(dir)
}

/// The next wake's number: one past the newest wake file, so numbers keep counting across
/// restarts.
pub fn next_wake_number(dir: &Path) -> u64 {
    std::fs::read_dir(dir.join("wakes"))
        .map(|entries| {
            entries
                .filter_map(|entry| {
                    entry
                        .ok()?
                        .path()
                        .file_stem()?
                        .to_str()?
                        .parse::<u64>()
                        .ok()
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
        + 1
}

/// Writes wake `n`'s request and returns its path.
pub fn write_wake(dir: &Path, n: u64, body: &str) -> std::io::Result<PathBuf> {
    let path = dir.join("wakes").join(format!("{n}.md"));
    std::fs::write(&path, body)?;
    // Keep the last 50 wakes.
    if let Ok(entries) = std::fs::read_dir(dir.join("wakes")) {
        let mut numbers: Vec<u64> = entries
            .filter_map(|entry| {
                entry
                    .ok()?
                    .path()
                    .file_stem()?
                    .to_str()?
                    .parse::<u64>()
                    .ok()
            })
            .collect();
        numbers.sort_unstable();
        let excess = numbers.len().saturating_sub(50);
        for old in &numbers[..excess] {
            let _ = std::fs::remove_file(dir.join("wakes").join(format!("{old}.md")));
        }
    }
    Ok(path)
}

/// The supervisor's playbook, in its notes folder.
const PLAYBOOK: &str = r#"# Autopilot supervisor

You are Ghostex Autopilot's supervisor. The user turned Autopilot on so their agent sessions keep
going while they are away. Ghostex itself already heals what a rule can (carry on, interrupt,
reload, switch account, answer a question that has a recommended choice, approval cards within
the user's limits). It wakes you only for what needs judgment.

## Every wake

1. Read `notes.md` here, then `sessions/<projectId>-<sessionId>.md` for each session in the wake.
2. Read the wake file Ghostex named (`wakes/<n>.md`): each session, what Ghostex saw, and why it
   needs you.
3. For each session, look at what it was doing: `ghostex read-session-chat <projectId>:<sessionId>
   --last 6 --format text` (and `ghostex read-text <ref>` for its screen when that helps).
4. Decide, and act ONLY through Ghostex, once per session:
   - `ghostex autopilot decide <projectId> <sessionId> --answer "<option label or reply>"`: answer
     its open question. For several questions, repeat `--answer` once per question, in order.
   - `ghostex autopilot decide <projectId> <sessionId> --message "<text>"`: tell it how to carry on
     (it gets the message as the user's).
   - `ghostex autopilot decide <projectId> <sessionId> --leave "<one-line note for the user>"`:
     leave it for the user when only they can decide or act.
   Every decision is logged in Settings > Autopilot with your words.
5. Write down what you decided and why: one dated line in `notes.md`, and the session's own file
   (what it is doing, what you told it, what to check next time). Then end your turn.

## Rules

- Pick what the user would most likely pick: the agent's own recommendation, the safest choice that
  keeps the work moving, the smallest step that unblocks it. When the user said "do what you
  recommend", do that.
- Never sign in, pass 2FA, verify an identity or pay for anything; never tell an agent to. Leave
  those for the user with a note, and tell the agent to carry on with the rest of its task.
- Respect the hard limits the wake file lists as not allowed (push main, merge, delete outside the
  project, post publicly, and the rest): leave them for the user, and tell the agent to carry on
  with what it can do without them.
- Never approve permission prompts or approval cards yourself, and never type into another
  session's terminal: only `ghostex autopilot decide`.
- Don't change files in the user's projects yourself. You decide; the session's own agent works.
- Keep each wake short. If nothing needs doing, write one line in `notes.md` and end your turn.
"#;

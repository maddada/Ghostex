//! The words the board shows or hands to an agent: Beads failure messages, the board notice and
//! its fix prompts, the draft title of an untitled ticket, and the Start work prompt. Ported from
//! the deleted React board's project-board-shared.ts, board-state.ts and remote-migrate-gate.tsx
//! (apps/desktop/views/, in git history).

use serde_json::Value;

use super::model::BoardTicket;

/// `beadsErrorMessage`: the prose a failing `bd` wrote, without its advisory `warning:` lines and
/// without the JSON envelope printed after it. A remote-migration gate envelope is kept whole so
/// the notice can render it.
pub(crate) fn beads_error_message(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return "The Beads command failed.".to_string();
    }
    let mut lines = Vec::<&str>::new();
    let mut in_warning = false;
    for raw in trimmed.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.to_lowercase().starts_with("warning:") {
            in_warning = true;
            continue;
        }
        if in_warning && raw.starts_with(' ') {
            continue;
        }
        in_warning = false;
        lines.push(line);
    }
    if lines.is_empty() {
        return trimmed.to_string();
    }
    let envelope_start = lines.iter().position(|line| line.starts_with('{'));
    let prose = &lines[..envelope_start.unwrap_or(lines.len())];
    let envelope_text = envelope_start
        .map(|start| lines[start..].join("\n"))
        .unwrap_or_default();
    let envelope = serde_json::from_str::<Value>(&envelope_text)
        .ok()
        .filter(Value::is_object);
    if envelope
        .as_ref()
        .is_some_and(|envelope| envelope["remote_migrate_gate"].is_object())
    {
        return envelope_text;
    }
    if !prose.is_empty() {
        return prose.join(" ");
    }
    envelope
        .as_ref()
        .map(beads_envelope_error)
        .filter(|error| !error.is_empty())
        .unwrap_or_else(|| lines.join(" "))
}

fn beads_envelope_error(payload: &Value) -> String {
    let body = if payload["data"].is_object() {
        &payload["data"]
    } else {
        payload
    };
    if let Some(error) = body["failed"]
        .as_array()
        .and_then(|failed| failed.iter().find(|entry| entry.is_object()))
        .and_then(|entry| entry["error"].as_str())
        .filter(|error| !error.is_empty())
    {
        return error
            .strip_prefix("updating issue:")
            .map(str::trim_start)
            .unwrap_or(error)
            .to_string();
    }
    body["error"]
        .as_str()
        .or_else(|| payload["error"].as_str())
        .unwrap_or_default()
        .to_string()
}

/// `createProjectBoardDraftTitle`: a short deterministic title from the prompt's first line, shown
/// until the prompt agent's generated title replaces it.
pub(crate) fn draft_title(prompt: &str) -> String {
    const MAX: usize = 39;
    let mut without_fences = String::new();
    let mut in_fence = false;
    for line in prompt.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            without_fences.push_str(line);
            without_fences.push('\n');
        }
    }
    let first_line = without_fences
        .lines()
        .map(|line| {
            let line = line.trim_start();
            let line = line.trim_start_matches('#').trim_start();
            let line = line
                .strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .or_else(|| line.strip_prefix("+ "))
                .unwrap_or(line);
            let line = match line.find(['.', ')']) {
                Some(index)
                    if index > 0
                        && line[..index].chars().all(|ch| ch.is_ascii_digit())
                        && line[index + 1..].starts_with(' ') =>
                {
                    &line[index + 1..]
                }
                _ => line,
            };
            line.chars()
                .map(|ch| {
                    if matches!(ch, '`' | '*' | '_' | '~' | '>' | '#') {
                        ' '
                    } else {
                        ch
                    }
                })
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    // The first sentence, when the line opens with one of at least 8 characters.
    let sentence = first_line
        .char_indices()
        .find(|(_, ch)| matches!(ch, '.' | '!' | '?'))
        .filter(|(index, ch)| {
            first_line[..*index].chars().count() >= 8
                && first_line[index + ch.len_utf8()..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace)
        })
        .map(|(index, ch)| &first_line[..index + ch.len_utf8()])
        .unwrap_or(&first_line);
    let title = sentence
        .trim_end_matches(['.', '!', '?'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() {
        return "New ticket".to_string();
    }
    if title.chars().count() <= MAX {
        return title;
    }
    let head = title.chars().take(MAX).collect::<String>();
    let clipped = match head.rfind(char::is_whitespace) {
        Some(index) => head[..index].trim().to_string(),
        None => head.clone(),
    };
    let chosen = if clipped.chars().count() >= 12 {
        clipped
    } else {
        head
    };
    let chosen = chosen
        .trim_end_matches(['.', ',', ';', ':', '!', '?', '-'])
        .trim()
        .to_string();
    if chosen.is_empty() {
        "New ticket".to_string()
    } else {
        chosen
    }
}

/// What Start work sends to the agent: line for line the prompt `build_board_bead_work_prompt` in
/// server/src/board_start_work.rs sends for `ghostex board start-work`.
pub(crate) fn agent_work_prompt(ticket: &BoardTicket) -> String {
    let bead_id = &ticket.issue.id;
    let description = ticket.issue.description.trim();
    [
        format!(
            "Work on bead {bead_id} ({}): {}",
            ticket.display_id, ticket.issue.title
        ),
        String::new(),
        if description.is_empty() {
            "No prompt provided.".to_string()
        } else {
            description.to_string()
        },
        String::new(),
        "Put this session on the card before you start, so the board shows who is working it:"
            .to_string(),
        format!("- `gx board associate {bead_id}`"),
        "- Run it again after forking or restoring, so the card follows the session that is really working.".to_string(),
        String::new(),
        "After each turn where you made progress on this bead, add a bead comment summarizing what you did:".to_string(),
        format!("- `bd comment {bead_id} \"<summary>\"`"),
        "- Focus on user-facing requirements delivered and high-level technical approach.".to_string(),
        "- Do not list specific files or line numbers.".to_string(),
        "- End the comment with `Agent: <agent name>` and `Session: <saved agent CLI session id>` lines so the ticket view can show the agent after the user name and the resumable agent session id at the bottom.".to_string(),
        String::new(),
        "Status workflow for this project board:".to_string(),
        format!("- Park for later: `bd update {bead_id} --status backlog`"),
        format!("- When you start: `bd update {bead_id} --status in_progress`"),
        format!("- When implementation is ready for test: `bd update {bead_id} --status test`"),
        format!("- When ready for review: `bd update {bead_id} --status review`"),
        format!("- When done: `bd close {bead_id}`"),
    ]
    .join("\n")
}

/// `parseProjectBoardCommentText`: the body plus the optional `Agent:` footer name.
pub(crate) fn parse_comment(text: &str) -> (String, Option<String>) {
    let original = text.trim();
    let lines = original.lines().collect::<Vec<_>>();
    let mut cursor = lines.len() as isize - 1;
    let line_at = |cursor: isize| {
        if cursor >= 0 {
            lines.get(cursor as usize).map(|line| line.trim())
        } else {
            None
        }
    };
    let mut session = None;
    let mut agent = None;
    if let Some(value) = line_at(cursor).and_then(|line| line.strip_prefix("Session:")) {
        session = Some(value.trim().to_string()).filter(|value| !value.is_empty());
        cursor -= 1;
    }
    if let Some(value) = line_at(cursor).and_then(|line| line.strip_prefix("Agent:")) {
        agent = Some(value.trim().to_string()).filter(|value| !value.is_empty());
        cursor -= 1;
    }
    if session.is_none() && agent.is_none() {
        return (original.to_string(), None);
    }
    while line_at(cursor) == Some("") {
        cursor -= 1;
    }
    let has_separator = line_at(cursor) == Some("---");
    if has_separator {
        cursor -= 1;
    }
    if session.is_some() && agent.is_none() && !has_separator {
        return (original.to_string(), None);
    }
    while line_at(cursor) == Some("") {
        cursor -= 1;
    }
    let body = lines[..(cursor + 1).max(0) as usize]
        .join("\n")
        .trim()
        .to_string();
    (body, agent)
}

/// `formatShortDate`: "6 Aug".
pub(crate) fn format_short_date(value: Option<&str>) -> String {
    let Some(ms) = super::model::parse_time_ms(value) else {
        return String::new();
    };
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%-d %b")
                .to_string()
        })
        .unwrap_or_default()
}

pub(crate) const BEADS_INSTALL_GUIDE_URL: &str =
    "https://github.com/gastownhall/beads/blob/main/docs/INSTALLING.md";

/// The board-wide notice for a failed load or mutation (`ProjectBoardNotice`).
pub(crate) struct KanbanNotice {
    pub(crate) title: String,
    pub(crate) lines: Vec<String>,
    pub(crate) fix_prompt: String,
    pub(crate) link: Option<(String, String)>,
    /// The Beads CLI is missing, so the notice offers Install Beads.
    pub(crate) offers_install: bool,
}

pub(crate) fn board_notice(message: &str, project_path: &str) -> KanbanNotice {
    if let Some(notice) = remote_migrate_gate_notice(message, project_path) {
        return notice;
    }
    let lower = message.to_lowercase();
    let is_missing_project = [
        "not initialized",
        "no storage",
        "not a beads workspace",
        "not a beads project",
        "not a beads repository",
        "no beads database found",
        "run bd init",
        "run 'bd init",
        "run \"bd init",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let is_missing_beads = !is_missing_project
        && [
            "bd was not found",
            "beads cli",
            "executable",
            "command not found",
            "not found: bd",
            "bd: not found",
            "env: bd: no such file",
            "cannot find",
        ]
        .iter()
        .any(|needle| lower.contains(needle));
    let title = if is_missing_beads {
        "Beads CLI unavailable"
    } else if is_missing_project {
        "Initialize Beads for this project"
    } else {
        "Project board unavailable"
    };
    let lines = if is_missing_beads {
        vec![
            "The Project board runs the Beads CLI (bd) in the environment running this project: macOS, Linux, or the selected WSL distribution.".to_string(),
            "Ghostex can install it there for you: click Install Beads.".to_string(),
        ]
    } else if is_missing_project {
        vec!["This project does not have a Beads workspace yet. Copy a fix prompt for an agent to inspect and initialize it safely; the agent asks you before it creates any Beads files.".to_string()]
    } else {
        vec![
            message.to_string(),
            "Update Beads to the latest release, then retry. If this is a remote-backed migration, follow the coordinated migration instructions instead of migrating multiple clones independently.".to_string(),
        ]
    };
    /*
    CDXC:ProjectBoard 2026-09-30 DECISION:
    User: "add a message to confirm with the user before creating the beads files". An agent handed this prompt must ask the user and wait for a yes before running bd init or anything else that creates `.beads` files in a project that has none.
    */
    let fix_prompt = format!(
        "Fix this Beads Project Board issue.\n\nProject path: {project_path}\n\nNotice: {title}\n\nReported issue:\n{message}\n\nUse the machine-installed bd CLI in the environment that runs this project. Diagnose and correct the underlying problem safely. If the project has no Beads workspace yet, stop and ask the user to confirm they want Beads set up in this project before running bd init or anything else that creates .beads files, and wait for their answer. Preserve all existing and unpushed Beads data. If this is a remote-backed database or migration, follow the coordinated migration gate: designate exactly one clone to migrate and publish, and have every other clone adopt that result. Do not bypass the remote migration gate or migrate multiple clones independently. Verify the fix by running a normal read such as bd status from the project path, then report what changed."
    );
    KanbanNotice {
        title: title.to_string(),
        lines,
        fix_prompt,
        link: (is_missing_beads || !is_missing_project).then(|| {
            (
                "Beads install and update guide".to_string(),
                BEADS_INSTALL_GUIDE_URL.to_string(),
            )
        }),
        offers_install: is_missing_beads,
    }
}

fn migration_option_label(id: &str) -> String {
    match id {
        "migrate" => "Migrate this clone",
        "adopt" => "Adopt the remote",
        "adopt-fast-forward" => "Adopt the remote (lossless)",
        "reconcile-fork" => "Back up and adopt the canonical remote",
        other => other,
    }
    .to_string()
}

/// `RemoteMigrateGateNotice`: the operator decision Beads reports. The board never runs a
/// migration itself; it lists the options and hands an agent a fix prompt.
fn remote_migrate_gate_notice(message: &str, project_path: &str) -> Option<KanbanNotice> {
    let payload = serde_json::from_str::<Value>(message).ok()?;
    let gate = payload.get("remote_migrate_gate")?.as_object()?;
    let options = gate
        .get("options")?
        .as_array()?
        .iter()
        .filter_map(|option| {
            let id = option.get("id")?.as_str()?.to_string();
            let has_commands = option
                .get("commands")
                .and_then(Value::as_array)
                .is_some_and(|commands| commands.iter().any(Value::is_string));
            has_commands.then(|| {
                let text = |key: &str| {
                    option
                        .get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                (id, text("when"), text("risk"))
            })
        })
        .collect::<Vec<_>>();
    if options.is_empty() {
        return None;
    }
    let number = |key: &str| gate.get(key).and_then(Value::as_u64);
    let text = |key: &str| gate.get(key).and_then(Value::as_str).map(str::to_string);
    let decision = text("decision");
    let fallback_reason = text("fallback_reason");
    let (current, latest, pending) = (
        number("current_version"),
        number("latest_version"),
        number("pending").filter(|pending| *pending > 0),
    );
    let version_summary = match (current, latest) {
        (Some(current), Some(latest)) => format!(
            "Schema v{current} to v{latest}{}",
            pending
                .map(|pending| format!(" · {pending} pending"))
                .unwrap_or_default()
        ),
        _ => "A schema migration is pending.".to_string(),
    };
    let explanation = match decision.as_deref() {
        Some("adopt" | "adopt-ff") => {
            "The remote is already migrated. This clone must adopt that result; migrating it independently would fork the board schema."
        }
        Some("fork-skew") => {
            "This clone and its remote have already applied different migration content. Choose a canonical clone before replacing any local database."
        }
        _ => {
            "Ghostex could not prove which clone should migrate. Exactly one clone may migrate and publish; every other clone must adopt that result."
        }
    };
    let mut lines = vec![
        version_summary,
        "First install or update to the latest Beads release on every clone, then follow one coordinated migration path below.".to_string(),
        explanation.to_string(),
    ];
    match fallback_reason.as_deref() {
        Some("unreadable-remote-state") => lines.push("The cached remote schema state could not be read, so Ghostex cannot safely choose for you.".to_string()),
        Some("below-convergence-floor") => lines.push("This database predates Beads' merge-safe migration floor, so unattended migration is unsafe.".to_string()),
        _ => {}
    }
    for (id, when, risk) in &options {
        let mut line = migration_option_label(id);
        if !when.is_empty() {
            line.push_str(&format!(": use only when {when}."));
        }
        if !risk.is_empty() {
            line.push_str(&format!(" Risk: {risk}."));
        }
        lines.push(line);
    }
    let schema_summary = match (current, latest) {
        (Some(current), Some(latest)) => format!(
            "Current schema: v{current}\nTarget schema: v{latest}{}",
            pending
                .map(|pending| format!("\nPending migrations: {pending}"))
                .unwrap_or_default()
        ),
        _ => "Schema state: a migration is pending".to_string(),
    };
    let option_summary = options
        .iter()
        .map(|(id, when, risk)| {
            let mut lines = vec![format!("- {}", migration_option_label(id))];
            if !when.is_empty() {
                lines.push(format!("  Use only when: {when}"));
            }
            if !risk.is_empty() {
                lines.push(format!("  Risk: {risk}"));
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let fix_prompt = format!(
        "Fix this coordinated Beads Project Board migration issue.\n\nProject path: {project_path}\n\n{schema_summary}{}{}\n\nMigration choices reported by Beads:\n{option_summary}\n\nUse the machine-installed bd CLI in the environment that runs this project. Inspect the current database and remote state before changing anything. Preserve all existing and unpushed Beads data. Follow the remote migration gate exactly: designate one canonical clone to migrate and publish, and have every other clone adopt that result. Never migrate multiple clones independently or bypass the gate. Verify the repaired board with a normal read such as bd status from the project path, then report what changed.",
        decision
            .as_deref()
            .map(|decision| format!("\nMigration decision reported by Beads: {decision}"))
            .unwrap_or_default(),
        fallback_reason
            .as_deref()
            .map(|reason| format!("\nFallback reason: {reason}"))
            .unwrap_or_default(),
    );
    let docs = text("docs").map(|url| {
        url.replace(
            "/website/docs/getting-started/upgrading.md",
            "/docs/getting-started/upgrading.md",
        )
    });
    Some(KanbanNotice {
        title: "Project board migration needs coordination".to_string(),
        lines,
        fix_prompt,
        link: docs.map(|url| {
            (
                "Read the Beads multi-clone migration guide".to_string(),
                url,
            )
        }),
        offers_install: false,
    })
}

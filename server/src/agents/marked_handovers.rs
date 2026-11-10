//! Marked handovers: the style note that asks agents to mark the sections of their final message.

use std::path::Path;

use serde_json::Value;

use super::*;
use crate::paths::{get_gxserver_paths, GxserverPaths};

/// CDXC:MarkedHandovers 2026-10-17 DECISION:
/// User: "please allow launching an orchestrator without the reply style so the orchestrator answers normally. but put "Not recommended" and make it an advanced setting". Supersedes the 2026-10-11 "force the marked handovers for coordinators/orchestrator threads and their children": threads always get this note, orchestrators get it unless Orchestrators answer without markers (`orchestratorPlainReplies`, off by default) is on, and every other agent session Ghostex starts gets it only while the Marked handovers setting is on (off by default). The agent otherwise writes as it does today. The note goes into every such session's prompt, so it stays short and plain.
/// CDXC:MarkedHandovers 2026-10-11 WHY:
/// Wording tested in throwaway Claude sessions (Haiku and Sonnet, in this repo and in a plain folder): a plain list of markers was followed in a plain folder but ignored under this repo's long AGENTS.md (the models said they "missed" it). What made it stick: saying it applies on top of CLAUDE.md and AGENTS.md and overrides the no-emoji default, keeping the agent's own headings with the marker in front, naming the sections that always get ❓ and 🧪, and a check before sending. In this repo models still miss a section now and then.
/// CDXC:MarkedHandovers 2026-10-11 DECISION:
/// User: "let's call it 'Kicked off'": 🛫 Kicked off marks work started in this turn (threads an orchestrator just started), kept apart from 🔄 In progress, which is everything still running.
/// CDXC:MarkedHandovers 2026-10-11 DECISION:
/// User: "please never ever do this where the line is right next to the title, always should be bullet below it". A marker's title stands alone on its line and the section's content goes in bullets below it; the note's example shows that shape.
/// CDXC:MarkedHandovers 2026-10-14 DECISION:
/// User: "change our skill/instructions/memory so that we capitalize bullets". Every bullet and sub-bullet starts with a capital letter, even when it continues a lead-in sentence, so no lowercase starts and no list-ending semicolons or "and" chains; the note, the orchestrator playbook (coordinators/role.md) and the thread brief (coordinators/brief.rs) all say it.
/// CDXC:MarkedHandovers 2026-10-24 DECISION:
/// User: "this shouldn't have been \"waiting for you\" it should've been under Needs you ... I didn't know u needed me here". Anything that waits on the user (a check only they can do, a sign-in, a thread kept open until they try something) goes under ❓ Needs you with what to do, never under 🔄 In progress or 🧪 Untested; In progress is only work that carries on without them. The orchestrator playbook (skills/ghostex-orchestrator/SKILL.md) says the same.
/// CDXC:MarkedHandovers 2026-11-05 DECISION:
/// User (Q1232, Q1233): four more sections, 💡 Suggestions, ⏸️ Parked (so In progress lists only running work), 📦 Held for release and 📣 Comms (the user renamed "Posted" to "Comms"); "we should only send the ones that the user needs to see": a section appears only when it has something; and "move Suggestions, Needs you, Blocked and other ones like that that I must pay attention to at the bottom of the messages u send so i don't miss them". Status sections (✅ 🚀 📣 🛫 🔄 📦 🧪) come first and actionable ones (⚠️ 🛑 💡 ❓) last, with ❓ Needs you the very last. ⚠️ Read this sits with the actionable ones because it is something the user must take in before acting, not a report of what happened.
/// CDXC:MarkedHandovers 2026-11-05 DECISION:
/// User: "I REALLY don't care if u keep a thread open please don't share this." The ⏸️ Parked section is gone: whether a thread is kept open is the agent's bookkeeping, not news, so a handover never lists idle or kept-open threads. Older messages that used it still draw it.
/// SEE-ALSO: packages/settings-catalog/src/data/defaults.rs (`markedHandovers`), server/src/coordinators/role.rs (the orchestrator's role carries the note), server/src/coordinators/brief.rs (`thread_brief`), server/src/agents/activity.rs (ZCode's SessionStart context).
pub const MARKED_HANDOVERS_NOTE: &str = "Marked handovers: a format rule the user turned on in Ghostex. It applies in every project, on top of any CLAUDE.md or AGENTS.md, and allows these emoji even where you would otherwise avoid them.
In the final message of each turn (the one that hands back to the user), put the matching marker first, before any bold, in front of every section, heading or bold label that is one of these.
Status sections, at the top, in this order:
✅ Done: work you finished or pushed
🚀 Shipped: released or published to users
📣 Comms: replies and comments you sent outside (GitHub, Discord and the like)
🛫 Kicked off: work started this turn that carries on after it (threads or jobs you just started)
🔄 In progress: work running right now that needs nothing from the user (anything waiting on them goes under ❓ instead)
📦 Held for release: help text, patches or replies ready but waiting for a release
🧪 Untested: anything you did not run, test or verify for real, changed or not
Actionable sections, at the bottom, in this order, so they are the last thing the user reads:
⚠️ Read this: risks, caveats, anything important or changed
🛑 Blocked: stuck until something happens
💡 Suggestions: improvements you noticed, one short line each on why
❓ Needs you: a question, a decision, anything waiting on the user (a check or test only they can do, a sign-in, an approval, something kept open until they try it), saying exactly what to do; always last
Use a section only when it has something in it; never write an empty one.
Keep your own headings and wording and only add the marker in front (\"✅ **Done:**\", \"⚠️ **Caveats**\", \"❓ **Open questions**\"). The marker and title always stand alone on their line, and everything under them goes in bullets below, never on the title's line:
✅ **Done:**
- Fixed the login check.
Start every bullet and sub-bullet with a capital letter, even when it continues a lead-in sentence, so each bullet reads as its own sentence or phrase.
A section that asks the user anything always gets ❓, and one about what you did not run, test or verify always gets 🧪. Sections that only explain get no marker.
Before you send that final message, check that every matching section starts with its marker on a line of its own and that the actionable ones come last. Leave the markers out of your other messages.";

const SETTING_KEY: &str = "markedHandovers";
const ORCHESTRATOR_PLAIN_SETTING_KEY: &str = "orchestratorPlainReplies";

/// Whether the Marked handovers setting is on, read fresh from the shared settings file so a
/// change reaches the next launch or resume without a gxserver restart.
pub(crate) fn marked_handovers_enabled(paths: &GxserverPaths) -> bool {
    bool_setting(paths, SETTING_KEY)
}

/// Whether Coordinators answer without markers is on, read fresh like
/// [`marked_handovers_enabled`]. It only matters while Marked handovers is on (see
/// [`coordinator_marks_replies`]).
pub(crate) fn orchestrator_plain_replies(paths: &GxserverPaths) -> bool {
    bool_setting(paths, ORCHESTRATOR_PLAIN_SETTING_KEY)
}

/// Whether a coordinator writes its replies as marked sections: Marked handovers is on and
/// Coordinators answer without markers is off. Since 2026-11-11 a coordinator follows the Marked
/// handovers setting like every other session (CDXC:Coordinators 2026-11-11 DECISION in
/// coordinators/role.rs `coordinator_guide_for`), which supersedes "orchestrators get it unless
/// Orchestrators answer without markers is on" in the 2026-10-17 decision above; threads still
/// mark their reports whatever the settings say.
pub(crate) fn coordinator_marks_replies(paths: &GxserverPaths) -> bool {
    marked_handovers_enabled(paths) && !orchestrator_plain_replies(paths)
}

fn bool_setting(paths: &GxserverPaths, key: &str) -> bool {
    // Unit tests assert exact commands, which must not depend on the user's settings.
    if cfg!(test) {
        return false;
    }
    crate::shared_settings_file::read_shared_settings(paths)
        .and_then(|settings| settings.get(key).and_then(Value::as_bool))
        .unwrap_or(false)
}

/// CDXC:MarkedHandovers 2026-10-11 WHY:
/// For a session the setting covers, the note is added when the launch or resume command is resolved instead of being saved in the session's base command: the command is resolved again on every start, resume, fork and account switch, so turning the setting on or off reaches each session the next time it starts or resumes, while a running agent keeps the instructions it started with. Orchestrators made before 2026-10-17 carry it in their role (role.rs) unless they answer without markers; newer ones read it from `ghostex orchestrator guide`, which their `ghostex-orchestrator` skill sends them to (role.rs `coordinator_guide`), and threads have it saved in their base command at creation (`with_thread_marked_handovers`), so both keep it whatever the setting says. Claude keeps only the last `--append-system-prompt-file` (tested 2026-10-11 on Claude Code 2.1.296) and Codex reads one `developer_instructions`, so a command that already names its own is left alone rather than lose it, except a Codex orchestrator's guide pointer, which the note joins. ZCode has no such flag and gets the note as SessionStart hook context (activity.rs); Empryo ignores hook context and other agents have no channel Ghostex can use, so they do not get the note outside an Empryo orchestrator's profile.
/// CDXC:AgentGuide 2026-11-02 WHY:
/// The Ghostex note (agents/ghostex_note.rs) rides the same flag, joined with this note into one file or value, so neither replaces the other. Pi (`--append-system-prompt`, which takes a file) and Droid (`--append-system-prompt-file`) get the joined text through their own flags; OpenCode, Gemini, Cursor, Copilot, Amp and the rest have no flag Ghostex can use and get the note as the `ghostex-agent-guide` skill instead.
/// SEE-ALSO: server/src/agents/launch_plan.rs (`resolve_agent_launch_command` calls this), server/src/coordinators/role.rs (`coordinator_role_file` and `GUIDE_POINTER_COORDINATOR_INSTRUCTIONS`, the older orchestrators' role flags this combines with), server/src/agents/ghostex_note.rs.
pub(crate) fn with_launch_notes(agent_id: &str, icon: Option<&str>, command: &str) -> String {
    if command.trim().is_empty() {
        return command.to_string();
    }
    let family = default_agent_session_title_name(agent_id)
        .map(|_| agent_id.to_string())
        .or_else(|| icon.and_then(default_agent_icon_to_id).map(str::to_string))
        .or_else(|| infer_agent_id_from_command_executable(command));
    let Some(family) = family else {
        return command.to_string();
    };
    let paths = get_gxserver_paths(None);
    if family == "claude" {
        let role_file = crate::coordinators::coordinator_role_file(&paths);
        if command.contains(role_file.to_string_lossy().as_ref()) {
            // A Claude orchestrator reads its role file at every start and resume, so it is
            // rewritten here to follow the Marked handovers settings; its note is the role's.
            let _ = crate::coordinators::ensure_coordinator_role_file(&paths);
            return command.to_string();
        }
    }
    if family == "codex" {
        let pointer_arg = crate::coordinators::codex_developer_instructions_arg(
            crate::coordinators::GUIDE_POINTER_COORDINATOR_INSTRUCTIONS,
        );
        // An older Codex coordinator keeps only its guide pointer: the guide it points to ends
        // with the reply format (coordinators/role.rs `coordinator_guide_for`).
        if command.contains(&pointer_arg) {
            return command.to_string();
        }
    }
    let Some(text) = super::current_launch_note_text(&paths) else {
        return command.to_string();
    };
    with_note_text(&paths, &family, command, &text)
}

/// A thread's base command at creation, with the note saved in it, so the thread marks its
/// handovers on every start and resume whatever the setting says.
pub(crate) fn with_thread_marked_handovers(family: &str, command: &str) -> String {
    with_note_text(
        &get_gxserver_paths(None),
        family,
        command,
        MARKED_HANDOVERS_NOTE,
    )
}

/// `command` with `text` behind the system prompt flag of its family, unless it already names a
/// system prompt file or developer instructions (a note's own included).
fn with_note_text(paths: &GxserverPaths, family: &str, command: &str, text: &str) -> String {
    let file_flag = match family {
        "claude" if !command.contains("--append-system-prompt-file") => {
            "--append-system-prompt-file"
        }
        "droid" if !command.contains("--append-system-prompt") => "--append-system-prompt-file",
        // Pi reads a file named by `--append-system-prompt` (it takes text or a file).
        "pi" if !command.contains("--append-system-prompt") => "--append-system-prompt",
        "codex" if !command.contains("developer_instructions=") => {
            return format!("{} {}", command.trim_end(), codex_note_arg(text));
        }
        _ => return command.to_string(),
    };
    let path = note_file_path(paths, text);
    match write_note_file(&path, text) {
        Ok(()) => format!(
            "{} {file_flag} {}",
            command.trim_end(),
            quote_shell_arg(&path.to_string_lossy())
        ),
        Err(_) => command.to_string(),
    }
}

/// The Marked handovers note alone keeps its own file, which threads name in their saved commands;
/// any other text (the Ghostex note, alone or with it) has one file that every launch rewrites with
/// the current settings, which every session reads the next time it starts or resumes.
fn note_file_path(paths: &GxserverPaths, text: &str) -> std::path::PathBuf {
    if text == MARKED_HANDOVERS_NOTE {
        paths.root_dir.join("marked-handovers").join("note.md")
    } else {
        paths.root_dir.join("agent-note").join("note.md")
    }
}

/// `-c developer_instructions="…"` with the value as an ASCII-only TOML basic string: Codex parses
/// a `-c` value as TOML (and takes it as a raw string, quotes included, when that fails), and an
/// ASCII command types into every terminal and shell unchanged.
fn codex_note_arg(text: &str) -> String {
    let mut value = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => value.push_str("\\u0022"),
            '\\' => value.push_str("\\\\"),
            '\n' => value.push_str("\\n"),
            ' '..='~' => value.push(character),
            _ if (character as u32) <= 0xFFFF => {
                value.push_str(&format!("\\u{:04X}", character as u32))
            }
            _ => value.push_str(&format!("\\U{:08X}", character as u32)),
        }
    }
    value.push('"');
    format!(
        "-c {}",
        quote_shell_arg(&format!("developer_instructions={value}"))
    )
}

fn write_note_file(path: &Path, content: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).ok().as_deref() != Some(content) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_family_gets_the_note_through_its_own_flag() {
        let temp = tempfile::tempdir().unwrap();
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let text =
            crate::agents::launch_note_text(Some(crate::agents::GHOSTEX_NOTE), true).unwrap();
        let file = paths.root_dir.join("agent-note").join("note.md");
        let quoted = quote_shell_arg(&file.to_string_lossy());

        assert_eq!(
            with_note_text(&paths, "claude", "claude", &text),
            format!("claude --append-system-prompt-file {quoted}")
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        assert_eq!(
            with_note_text(&paths, "droid", "droid --resume x", &text),
            format!("droid --resume x --append-system-prompt-file {quoted}")
        );
        assert_eq!(
            with_note_text(&paths, "pi", "pi", &text),
            format!("pi --append-system-prompt {quoted}")
        );
        assert!(with_note_text(&paths, "codex", "codex", &text)
            .starts_with("codex -c 'developer_instructions=\"You are running inside Ghostex."));
        assert_eq!(
            with_note_text(&paths, "opencode", "opencode", &text),
            "opencode"
        );
    }

    #[test]
    fn a_command_with_its_own_system_prompt_keeps_it() {
        let temp = tempfile::tempdir().unwrap();
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        for (family, command) in [
            ("claude", "claude --append-system-prompt-file /x.md"),
            ("droid", "droid --append-system-prompt hi"),
            ("pi", "pi --append-system-prompt hi"),
            ("codex", "codex -c 'developer_instructions=\"hi\"'"),
        ] {
            assert_eq!(with_note_text(&paths, family, command, "note"), command);
        }
    }

    #[test]
    fn marked_handovers_alone_keeps_the_file_threads_name() {
        let temp = tempfile::tempdir().unwrap();
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        assert_eq!(
            note_file_path(&paths, MARKED_HANDOVERS_NOTE),
            paths.root_dir.join("marked-handovers").join("note.md")
        );
    }
}

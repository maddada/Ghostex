//! What loads into a bot: its persona, the shared preferences, `MEMORY.md` within its budget, the
//! index of its memory topics and its team. The same text is the generated `CLAUDE.md` and
//! `AGENTS.md` in the bot folder (read by the agent when a chat starts) and the answer of
//! `ghostex gbots brief` (run by the bot before its first reply and after any compaction, so a
//! long chat picks up memory that changed since it started).

use super::folder::{bot_folder, read_preferences, MEMORY_FILE};
use super::memory::{
    topic_index_lines, within_budget, MEMORY_MAX_BYTES, MEMORY_MAX_LINES, PREFERENCES_MAX_LINES,
};
use super::record::Bot;
use crate::paths::GxserverPaths;

const GENERATED_NOTE: &str = "<!-- Written by Ghostex from this GBot's settings and memory, and rewritten when they change: edits here are lost. Change the GBot's instructions in Ghostex (`ghostex gbots edit`); keep facts with `ghostex gbots memory add`. -->";

/// How a bot keeps its memory, in every brief (the `ghostex-gbots` skill says the same at more
/// length).
const KEEPING_MEMORY: &str = "## Keeping your memory\n\n- Save a fact worth keeping between chats: `ghostex gbots memory add \"<fact>\"` (add `--until YYYY-MM-DD` when it stops being true then). Keep each note short; when MEMORY.md fills up, Ghostex moves the oldest notes to memory/archive.md.\n- Longer detail goes in a topic: `ghostex gbots memory add --topic <name> \"<text>\"`; read one with `ghostex gbots memory topic <name>`.\n- Remove a note that is no longer true: `ghostex gbots memory forget \"<words of it>\"`.\n- Never edit MEMORY.md, the topics or this file with your own file tools: Ghostex keeps a journal of every change so the user can undo it, and keeps keys and passwords out of these files.\n- What you did each day is in memory/log/ (Ghostex writes it): `ghostex gbots memory log --days 3`.\n\n";

/// The text that loads into `bot`; `team` is every bot on this computer.
pub(crate) fn brief_text(paths: &GxserverPaths, bot: &Bot, team: &[Bot]) -> String {
    let folder = bot_folder(paths, &bot.bot_id);
    let mut out = String::new();
    let role = if bot.title.is_empty() {
        String::new()
    } else {
        format!(", {}", bot.title)
    };
    out.push_str(&format!("# You are {}{role}\n\n", bot.name));
    out.push_str(&format!(
        "You are a GBot: one of the user's own bots in Ghostex, led by Chief of Staff. You keep this name, these instructions and your memory in every chat you have; several of your chats may be open at once, all sharing this folder ({}). Your working files go in `files/`.\n\n",
        folder.display()
    ));
    if !bot.description.is_empty() {
        out.push_str(&format!("{}\n\n", bot.description));
    }
    out.push_str("## Your instructions\n\n");
    if bot.soul.trim().is_empty() {
        out.push_str("(None yet. The user or Chief of Staff sets them.)\n\n");
    } else {
        out.push_str(&format!("{}\n\n", bot.soul.trim()));
    }
    let preferences = read_preferences(paths);
    if !preferences.trim().is_empty() {
        let (kept, _) = within_budget(&preferences, PREFERENCES_MAX_LINES, MEMORY_MAX_BYTES);
        out.push_str(&format!(
            "## The user's preferences (shared by every GBot)\n\n{kept}\n\n"
        ));
    }
    out.push_str("## Your memory (MEMORY.md)\n\n");
    let memory = std::fs::read_to_string(folder.join(MEMORY_FILE)).unwrap_or_default();
    let (kept, cut) = within_budget(&memory, MEMORY_MAX_LINES, MEMORY_MAX_BYTES);
    if kept.is_empty() {
        out.push_str("(Empty.)\n\n");
    } else {
        out.push_str(&format!("{kept}\n\n"));
        if cut {
            out.push_str(&format!(
                "(MEMORY.md goes on past {MEMORY_MAX_LINES} lines or {} KB; read the file for the rest.)\n\n",
                MEMORY_MAX_BYTES / 1024
            ));
        }
    }
    let topics = topic_index_lines(&folder);
    if !topics.is_empty() {
        out.push_str(&format!(
            "## Memory topics\n\nRead a topic's file when the work touches it.\n\n{}\n\n",
            topics.join("\n")
        ));
    }
    out.push_str(KEEPING_MEMORY);
    out.push_str("## Your team\n\n- Chief of Staff: leads the GBots and may hand you work.\n");
    for other in team {
        let you = if other.bot_id == bot.bot_id {
            " (you)"
        } else {
            ""
        };
        let title = if other.title.is_empty() {
            String::new()
        } else {
            format!(": {}", other.title)
        };
        out.push_str(&format!("- {}{title}{you}\n", other.name));
    }
    out
}

/// The generated `CLAUDE.md` / `AGENTS.md`: the brief under a note that says where to edit.
pub(crate) fn instruction_file_text(paths: &GxserverPaths, bot: &Bot, team: &[Bot]) -> String {
    format!(
        "{GENERATED_NOTE}\n\n{}\nRun `ghostex gbots brief` before your first reply and again after any context compaction: it prints this with your latest memory.\n",
        brief_text(paths, bot, team)
    )
}

/// Rewrites `bot`'s `CLAUDE.md` and `AGENTS.md` from its record and memory files as they are now.
pub(crate) fn refresh_instruction_files(
    paths: &GxserverPaths,
    db: &rusqlite::Connection,
    bot: &Bot,
) {
    let team = super::record::list_bots(db).unwrap_or_default();
    let _ = super::folder::write_instruction_files(
        paths,
        bot,
        &instruction_file_text(paths, bot, &team),
    );
}

/// The same for every bot (a bot added, renamed or removed changes every bot's team list).
pub(crate) fn refresh_all_instruction_files(paths: &GxserverPaths, db: &rusqlite::Connection) {
    let team = super::record::list_bots(db).unwrap_or_default();
    for bot in &team {
        let _ = super::folder::write_instruction_files(
            paths,
            bot,
            &instruction_file_text(paths, bot, &team),
        );
    }
}

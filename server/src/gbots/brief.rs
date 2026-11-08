//! What loads into a bot: its persona, the shared preferences, `MEMORY.md` within its budget, the
//! index of its memory topics and its team. The same text is the generated `CLAUDE.md` and
//! `AGENTS.md` in the bot folder (read by the agent when a chat starts) and the answer of
//! `ghostex gbots brief` (run by the bot before its first reply and after any compaction, so a
//! long chat picks up memory that changed since it started).

use std::path::Path;

use super::folder::{bot_folder, read_preferences, MEMORY_DIR, MEMORY_FILE};
use super::record::Bot;
use crate::paths::GxserverPaths;

/// `MEMORY.md` loads up to this many lines and bytes; the rest stays in the file.
pub(crate) const MEMORY_MAX_LINES: usize = 200;
pub(crate) const MEMORY_MAX_BYTES: usize = 24 * 1024;
/// The topic index lists at most this many topics, in at most this many characters.
const TOPICS_MAX: usize = 40;
const TOPICS_MAX_CHARS: usize = 2_000;
/// Shared preferences load up to this many lines.
const PREFERENCES_MAX_LINES: usize = 100;

const GENERATED_NOTE: &str = "<!-- Written by Ghostex from this GBot's settings and memory, and rewritten when they change: edits here are lost. Change the GBot's instructions in Ghostex (`ghostex gbots edit`); keep facts in MEMORY.md. -->";

/// The first `max_lines` lines and `max_bytes` bytes of `text`, and whether it was cut.
fn within_budget(text: &str, max_lines: usize, max_bytes: usize) -> (String, bool) {
    let mut kept = String::new();
    let mut cut = false;
    for (index, line) in text.lines().enumerate() {
        if index >= max_lines || kept.len() + line.len() + 1 > max_bytes {
            cut = true;
            break;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    (kept.trim_end().to_string(), cut)
}

/// `memory/<topic>.md` files: the first `# ` heading is the title and the first plain line under
/// it the description. The archive and the daily log are not topics.
fn topic_index(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder.join(MEMORY_DIR)) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "md")
                && path.file_stem().is_some_and(|stem| stem != "archive")
        })
        .collect();
    files.sort();
    let mut lines = Vec::new();
    let mut used = 0;
    for path in files.into_iter().take(TOPICS_MAX) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut title = stem.clone();
        let mut description = String::new();
        let mut seen_title = false;
        for line in text.lines().map(str::trim) {
            if !seen_title {
                if let Some(heading) = line.strip_prefix("# ") {
                    title = heading.trim().to_string();
                    seen_title = true;
                }
                continue;
            }
            if !line.is_empty() && !line.starts_with('#') {
                description = line.to_string();
                break;
            }
        }
        let line = if description.is_empty() {
            format!("- {title} (memory/{stem}.md)")
        } else {
            format!("- {title}: {description} (memory/{stem}.md)")
        };
        if used + line.len() > TOPICS_MAX_CHARS {
            break;
        }
        used += line.len();
        lines.push(line);
    }
    lines
}

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
    let topics = topic_index(&folder);
    if !topics.is_empty() {
        out.push_str(&format!(
            "## Memory topics\n\nRead a topic's file when the work touches it.\n\n{}\n\n",
            topics.join("\n")
        ));
    }
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

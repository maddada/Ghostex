//! What loads into a bot: its persona, the shared preferences, `MEMORY.md` within its budget, the
//! index of its memory topics and its team. The same text is the generated `CLAUDE.md` and
//! `AGENTS.md` in the bot folder (read by the agent when a chat starts) and the answer of
//! `ghostex gbots brief` (run by the bot before its first reply and after any compaction, so a
//! long chat picks up memory that changed since it started).

use super::departments::{list_departments, Department};
use super::folder::{bot_folder, read_preferences, MEMORY_FILE};
use super::memory::{
    topic_index_lines, within_budget, MEMORY_MAX_BYTES, MEMORY_MAX_LINES, PREFERENCES_MAX_LINES,
};
use super::record::{list_bots, Bot};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// Other GBots a brief lists beyond the bot's own department.
const ROSTER_OTHERS_MAX: usize = 12;

/// Every bot and department on this computer: what a brief's team list is drawn from.
pub(crate) struct Team {
    pub bots: Vec<Bot>,
    pub departments: Vec<Department>,
}

impl Team {
    pub(crate) fn load(db: &rusqlite::Connection) -> Result<Self, DomainStateError> {
        Ok(Self {
            bots: list_bots(db)?,
            departments: list_departments(db)?,
        })
    }

    fn department(&self, id: Option<&str>) -> Option<&Department> {
        let id = id?;
        self.departments
            .iter()
            .find(|department| department.department_id == id)
    }

    fn name_of(&self, bot_id: &str) -> Option<&str> {
        self.bots
            .iter()
            .find(|bot| bot.bot_id == bot_id)
            .map(|bot| bot.name.as_str())
    }
}

const GENERATED_NOTE: &str = "<!-- Written by Ghostex from this GBot's settings and memory, and rewritten when they change: edits here are lost. Change the GBot's instructions in Ghostex (`ghostex gbots edit`); keep facts with `ghostex gbots memory add`. -->";

/// How a bot keeps its memory, in every brief (the `ghostex-gbots` skill says the same at more
/// length).
const KEEPING_MEMORY: &str = "## Keeping your memory\n\n- Save a fact worth keeping between chats: `ghostex gbots memory add \"<fact>\"` (add `--until YYYY-MM-DD` when it stops being true then). Keep each note short; when MEMORY.md fills up, Ghostex moves the oldest notes to memory/archive.md.\n- Longer detail goes in a topic: `ghostex gbots memory add --topic <name> \"<text>\"`; read one with `ghostex gbots memory topic <name>`.\n- Remove a note that is no longer true: `ghostex gbots memory forget \"<words of it>\"`.\n- Never edit MEMORY.md, the topics or this file with your own file tools: Ghostex keeps a journal of every change so the user can undo it, and keeps keys and passwords out of these files.\n- What you did each day is in memory/log/ (Ghostex writes it): `ghostex gbots memory log --days 3`.\n\n";

/// The text that loads into `bot`; `team` is every bot and department on this computer.
pub(crate) fn brief_text(paths: &GxserverPaths, bot: &Bot, team: &Team) -> String {
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
    out.push_str(&department_section(bot, team));
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
    out.push_str(&roster(bot, team));
    out
}

/// "Your department": its shared instructions, who leads it, and what that means for this bot.
///
/// CDXC:GBots 2026-11-09 DECISION:
/// User (plan piece 6): "department instructions reach each member's brief": every member reads its department's shared instructions under its own, and its brief says who leads it, so a lead knows whom it directs and a member knows whom it takes work from.
fn department_section(bot: &Bot, team: &Team) -> String {
    let Some(department) = team.department(bot.department_id.as_deref()) else {
        return String::new();
    };
    let mut out = format!("## Your department: {}\n\n", department.name);
    let lead = department
        .lead_bot_id
        .as_deref()
        .and_then(|lead| team.name_of(lead));
    let members: Vec<&Bot> = team
        .bots
        .iter()
        .filter(|other| other.department_id == bot.department_id && other.bot_id != bot.bot_id)
        .collect();
    match lead {
        _ if bot.is_lead => {
            out.push_str(&format!(
                "You lead {}. Hand its members work with `ghostex gbots ask <bot> --task \"<brief>\"`, read their reports, and change a member's instructions with `ghostex gbots edit <bot> --soul-file <file>` when the user or Chief of Staff asks for it. Other GBots you may only ask questions (`--consult`).\n\n",
                department.name
            ));
        }
        Some(lead) => out.push_str(&format!(
            "{lead} leads {}: your work comes from {lead} or Chief of Staff, and your reports go back to whoever handed you the work.\n\n",
            department.name
        )),
        None => out.push_str(&format!(
            "{} has no lead: Chief of Staff hands you work.\n\n",
            department.name
        )),
    }
    if !members.is_empty() {
        let names: Vec<&str> = members.iter().map(|other| other.name.as_str()).collect();
        out.push_str(&format!(
            "With you in {}: {}.\n\n",
            department.name,
            names.join(", ")
        ));
    }
    if !department.instructions.trim().is_empty() {
        out.push_str(&format!(
            "### {}'s shared instructions (every member follows them)\n\n{}\n\n",
            department.name,
            department.instructions.trim()
        ));
    }
    out
}

/// "Your team": Chief of Staff, the bot's own department first, then up to 12 other GBots, each
/// with its department and whether it leads.
fn roster(bot: &Bot, team: &Team) -> String {
    let mut out =
        String::from("## Your team\n\n- Chief of Staff: leads the GBots and may hand you work.\n");
    let line = |other: &Bot| {
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
        let place = match team.department(other.department_id.as_deref()) {
            Some(department) if other.is_lead => format!(" (leads {})", department.name),
            Some(department) => format!(" ({})", department.name),
            None => String::new(),
        };
        format!("- {}{title}{place}{you}\n", other.name)
    };
    let mut own: Vec<&Bot> = team
        .bots
        .iter()
        .filter(|other| {
            other.bot_id == bot.bot_id
                || (bot.department_id.is_some() && other.department_id == bot.department_id)
        })
        .collect();
    own.sort_by_key(|other| (!other.is_lead, other.name.to_lowercase()));
    for other in &own {
        out.push_str(&line(other));
    }
    let rest: Vec<&Bot> = team
        .bots
        .iter()
        .filter(|other| !own.iter().any(|mine| mine.bot_id == other.bot_id))
        .collect();
    for other in rest.iter().take(ROSTER_OTHERS_MAX) {
        out.push_str(&line(other));
    }
    if rest.len() > ROSTER_OTHERS_MAX {
        out.push_str(&format!(
            "- and {} more: `ghostex gbots list`\n",
            rest.len() - ROSTER_OTHERS_MAX
        ));
    }
    out
}

/// The generated `CLAUDE.md` / `AGENTS.md`: the brief under a note that says where to edit.
pub(crate) fn instruction_file_text(paths: &GxserverPaths, bot: &Bot, team: &Team) -> String {
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
    let Ok(team) = Team::load(db) else {
        return;
    };
    let _ = super::folder::write_instruction_files(
        paths,
        bot,
        &instruction_file_text(paths, bot, &team),
    );
}

/// The same for every bot (a bot added, renamed, removed or moved changes every bot's team list).
pub(crate) fn refresh_all_instruction_files(paths: &GxserverPaths, db: &rusqlite::Connection) {
    let Ok(team) = Team::load(db) else {
        return;
    };
    for bot in &team.bots {
        let _ = super::folder::write_instruction_files(
            paths,
            bot,
            &instruction_file_text(paths, bot, &team),
        );
    }
}

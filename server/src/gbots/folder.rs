//! The bot folder, `<app data>/gbots/bots/<botId>/`: the bot's project path and the working
//! folder of all its chats. It holds `SOUL.md` (a mirror of the record's standing instructions),
//! `MEMORY.md`, `memory/` (topic notes and the daily log), `files/` (its working files) and the
//! generated `CLAUDE.md` and `AGENTS.md` (brief.rs) the agents read when a chat starts.
//!
//! CDXC:GBots 2026-11-08 DECISION:
//! User (Q-C A): a bot's persona and memory reach it through generated CLAUDE.md / AGENTS.md in its folder plus `ghostex gbots brief` at start and after compaction, with no per-turn hook. The record is the truth for the instructions and `SOUL.md` only mirrors it: the file is rewritten only when the record's instructions change, so an edit made to the file (by hand or by the bot) never applies silently and shows as drift (`ghostex gbots show`) until the instructions are saved through Ghostex.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::record::Bot;
use crate::paths::GxserverPaths;

pub(crate) const SOUL_FILE: &str = "SOUL.md";
pub(crate) const MEMORY_FILE: &str = "MEMORY.md";
pub(crate) const MEMORY_DIR: &str = "memory";
pub(crate) const FILES_DIR: &str = "files";
pub(crate) const PREFERENCES_FILE: &str = "PREFERENCES.md";
/// The instruction files the agents read at start (Claude Code: CLAUDE.md; Codex and the others:
/// AGENTS.md), with the same text.
pub(crate) const INSTRUCTION_FILES: [&str; 2] = ["CLAUDE.md", "AGENTS.md"];

/// `<app data>/gbots/`: every bot folder, the shared preferences, and (later) the memory journal.
pub(crate) fn gbots_dir(paths: &GxserverPaths) -> PathBuf {
    paths.app_data_dir.join("gbots")
}

pub(crate) fn bot_folder(paths: &GxserverPaths, bot_id: &str) -> PathBuf {
    gbots_dir(paths).join("bots").join(bot_id)
}

/// Writes `text` to `path` through a temporary file, readable by the user only.
pub(crate) fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("ghostex-tmp");
    {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&temporary, path)
}

fn soul_text(bot: &Bot) -> String {
    format!("{}\n", bot.soul.trim_end())
}

/// Creates the folder and its starting files: `SOUL.md`, an empty `MEMORY.md`, `memory/` and
/// `files/`. Files already there are kept (a bot brought back keeps its memory).
pub(crate) fn create_folder(paths: &GxserverPaths, bot: &Bot) -> std::io::Result<PathBuf> {
    let folder = bot_folder(paths, &bot.bot_id);
    std::fs::create_dir_all(folder.join(MEMORY_DIR).join("log"))?;
    std::fs::create_dir_all(folder.join(FILES_DIR))?;
    write_atomic(&folder.join(SOUL_FILE), &soul_text(bot))?;
    let memory = folder.join(MEMORY_FILE);
    if !memory.exists() {
        write_atomic(
            &memory,
            &format!(
                "# {}'s memory\n\nWhat {} keeps between chats: one line per fact, newest last.\n",
                bot.name, bot.name
            ),
        )?;
    }
    Ok(folder)
}

/// Rewrites `SOUL.md` from the record (after its instructions were saved through Ghostex).
pub(crate) fn write_soul(paths: &GxserverPaths, bot: &Bot) -> std::io::Result<()> {
    write_atomic(
        &bot_folder(paths, &bot.bot_id).join(SOUL_FILE),
        &soul_text(bot),
    )
}

/// Whether `SOUL.md` no longer matches the record: changed or removed outside Ghostex.
pub(crate) fn soul_drift(paths: &GxserverPaths, bot: &Bot) -> Option<&'static str> {
    match std::fs::read_to_string(bot_folder(paths, &bot.bot_id).join(SOUL_FILE)) {
        Ok(text) if text.trim() == bot.soul.trim() => None,
        Ok(_) => Some("changed"),
        Err(_) => Some("missing"),
    }
}

/// The shared preferences every bot reads (written by the user and Chief of Staff); empty when
/// there are none yet.
pub(crate) fn read_preferences(paths: &GxserverPaths) -> String {
    std::fs::read_to_string(gbots_dir(paths).join(PREFERENCES_FILE)).unwrap_or_default()
}

/// Writes the generated `CLAUDE.md` and `AGENTS.md`.
pub(crate) fn write_instruction_files(
    paths: &GxserverPaths,
    bot: &Bot,
    text: &str,
) -> std::io::Result<()> {
    let folder = bot_folder(paths, &bot.bot_id);
    for name in INSTRUCTION_FILES {
        write_atomic(&folder.join(name), text)?;
    }
    Ok(())
}

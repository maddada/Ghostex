//! A GBot's memory: `MEMORY.md` (dated notes that load into every chat within 200 lines / 24 KB,
//! older ones moved to `memory/archive.md`), topic notes it reads when the work touches them
//! (`memory/<topic>.md`), a daily log gxserver writes of what it did (`memory/log/`), a journal of
//! every change with undo (outside the folder), and the shared preferences every GBot reads.
//!
//! - [`entries`]: the dated note format; [`budget`]: what loads and making room.
//! - [`topics`], [`log`], [`prefs`]: the topic notes, the daily log, the shared preferences.
//! - [`journal`], [`store`]: every write, locked per bot, journaled, then the instruction files
//!   rewritten; [`redact`]: secrets kept out of the files.
//! - [`ops`]: the `/api/gbots` memory ops; [`notify`]: open chats told to read their brief again.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q-C A): memory reaches a bot only through its generated CLAUDE.md / AGENTS.md and `ghostex gbots brief` (at the start of a chat and after compaction), never through a per-turn hook, so there is no automatic recall before each turn and no background pass that notices facts (both would need a hook or an extra model call per chat). A bot keeps its memory with `ghostex gbots memory …`; every write goes through gxserver so the budget, the journal and secret redaction always apply.

mod budget;
mod entries;
mod journal;
mod log;
mod notify;
mod ops;
mod prefs;
mod redact;
mod store;
mod topics;

pub(crate) use budget::{within_budget, MEMORY_MAX_BYTES, MEMORY_MAX_LINES};
pub(crate) use log::observe_activity;
pub(crate) use notify::{tell_every_bot, tell_open_chats};
pub(crate) use ops::{coordinator_remember, handles, run, MemoryCaller};
pub(crate) use prefs::PREFERENCES_MAX_LINES;
pub(crate) use topics::index_lines as topic_index_lines;
// Team files (gbots/team_file/) read and write starter notes the way memory does.
pub(crate) use journal::FileChange;
pub(crate) use redact::redact;
pub(crate) use store::{
    bot_lock as bot_memory_lock, commit as commit_memory, read_file as read_memory_file,
};
pub(crate) use topics::{list as topic_list, relative_path as topic_path, stem_of as topic_stem};

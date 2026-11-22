//! Per-agent history scanners (port of zehn's `src/scan.zig`).
//!
//! | Agent    | History location                                                      |
//! |----------|-----------------------------------------------------------------------|
//! | claude   | `~/.claude/history.jsonl`                                             |
//! | codex    | `~/.codex/history.jsonl` and `~/.codex/sessions/**/*.jsonl`           |
//! | pi       | `~/.pi/agent/sessions/*/*.jsonl`                                      |
//! | opencode | `~/.local/share/opencode/opencode.db` (SQLite)                        |
//! | cursor   | `~/.cursor/projects/*/agent-transcripts/*/*.jsonl`                    |
//! | grok     | `~/.grok/sessions/*/*/chat_history.jsonl` plus sibling `summary.json` |
//! | empryo   | `<repo>/.empryo/sessions/*/session.jsonl` for each repo in `~/.empryo/threads.db` |
//! | antigravity | `~/.gemini/antigravity-cli/history.jsonl` (titles from `conversation_summaries.db`) |
//! | hermes   | `~/.hermes/state.db` and `~/.hermes/profiles/*/state.db` (SQLite)     |
//! | copilot  | `~/.copilot/session-state/*/events.jsonl` plus sibling `workspace.yaml` |
//! | droid    | `~/.factory/sessions/*/*.jsonl` plus sibling `*.settings.json`        |
//! | omp      | `~/.omp/agent/sessions/*/*.jsonl` (Pi's session log)                  |
//! | zcode    | `~/.zcode/cli/db/db.sqlite` (SQLite)                                  |
//!
//! CDXC:PromptSearch 2026-08-20:
//! opencode history used to be read by shelling out to the `sqlite3` CLI, so a
//! machine without it silently lost opencode results. The Rust port links
//! SQLite directly, which removes that external dependency instead of papering
//! over it with a skip-and-warn path.

pub mod answers;
mod antigravity;
mod claude;
mod codex;
mod copilot;
mod cursor;
mod dates;
mod droid;
mod empryo;
mod files;
mod grok;
mod hermes;
mod json;
mod omp;
mod opencode;
mod pi;
mod prompt_text;
mod records;
mod scanner;
#[cfg(test)]
mod tests;
mod zcode;

pub use antigravity::*;
pub use copilot::*;
pub use dates::*;
pub use empryo::*;
use files::*;
pub use grok::*;
pub use json::*;
use prompt_text::*;
pub use records::*;
pub use scanner::*;

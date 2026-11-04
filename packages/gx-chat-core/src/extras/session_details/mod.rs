//! The Session details panel: what one agent session runs on and has loaded, beside the chat.
//!
//! The core owns everything a renderer would otherwise decide: when to read
//! (`/api/readSessionDetails`, only while the panel is on screen), the rows of every section with
//! their labels already worded, which rows open something, the folds, and the stop buttons. The
//! renderers draw `sessionDetails` and report the panel's visibility and clicks back.
//!
//! - `state.rs`: what the panel remembers between frames.
//! - `actions.rs`: the panel's clicks.
//! - `settle.rs`: the reads, their answers and the poll.
//! - `document.rs`: the `sessionDetails` document value.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User: a Claude Desktop style right panel "inside the chat view" with a button "top right" that shows it, docked when there is room and floating when not. From the mockups: docked as its own column with the transcript re-centered (Q1), separate cards (Q2), one button in the app header that opens the focused chat's panel (Q3) and shows the context ring with its % (Q4), and when the chat is too narrow a floating card that closes when the chat is clicked (Q5). Sections (Q9, Q14): Environment, Context, Tasks, Connectors, Skills, Memory files, Changes, Background tasks. The phone follows later (Q16).
//! SEE-ALSO: server/src/session_details/ (the read), apps/desktop/src/app/native_chat/session_details/ (the drawing), apps/desktop/src/app/render/workarea_header/ (the header button).

pub mod actions;
pub mod document;
pub mod settle;
pub mod state;

pub use state::SessionDetailsState;

/// The poll timer while the panel is on screen (`session/timers.rs` holds it while the chat is
/// hidden).
pub const SESSION_DETAILS_POLL: &str = "extras.sessionDetails";

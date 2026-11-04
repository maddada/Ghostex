//! Storage cleanup: the age limits gxserver applies to the files and rows Ghostex keeps, so its
//! data folders stop growing without bound. The daily sweep that runs them lives in
//! `server/src/server/storage_cleanup_sweep.rs`; the Codex mirror and visual page limits live
//! with their owners (`codex_rollout_archive.rs`, `visual_pages/store.rs`).

mod backups;
mod files;
mod images;
mod logs;
mod sessions;

pub(crate) use backups::prune_backups;
pub(crate) use files::{is_older, prune_tree, remove_file, Since, Tally, DAY};
pub(crate) use images::prune_images;
pub(crate) use logs::prune_logs;
pub(crate) use sessions::{expired_closed_sessions, CLOSED_SESSION_MAX_IDLE};

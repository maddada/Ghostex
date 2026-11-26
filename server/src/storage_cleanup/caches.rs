//! Derived caches and temporary files that Ghostex writes on its own and can safely regenerate
//! or discard past an age limit: `ghostex visual check` PNGs in `$TMPDIR/ghostex-visuals`,
//! derived agent transcript mirrors under `<gxserver state>`, and finished GPUI command-action
//! state files under `<state>/gpui-command-actions`.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use super::files::{is_older, prune_files_in, prune_tree, remove_file, Since, Tally, DAY};

/// Screenshots written by `ghostex visual check` into `$TMPDIR/ghostex-visuals` are throwaway
/// inspection images for agents.
const VISUAL_CHECK_MAX_AGE: Duration = Duration::from_secs(7 * DAY.as_secs());

/// Derived transcript mirrors rebuilt on demand from each agent's own store, matching
/// `codex-rollout-mirror`'s 30-day limit.
const CHAT_MIRROR_MAX_AGE: Duration = Duration::from_secs(30 * DAY.as_secs());

/// Transient command-action `.state` and `.command.tmp` files written by desktop command panes.
const COMMAND_ACTION_MAX_AGE: Duration = Duration::from_secs(7 * DAY.as_secs());

/// Only derived transcript mirrors that (1) rebuild unconditionally from the agent's own on-disk
/// store on open and (2) hold no Ghostex-only data.
///
/// Deliberately excluded:
/// - `antigravity-chat-mirror`: `agy` prunes `~/.gemini/antigravity-cli/brain/` after ~10-14 days,
///   so older mirrors can be the only plaintext copy left.
/// - `hermes-chat-mirror`: holds Ghostex-only `<id>.local.jsonl` / `local-*.jsonl` files.
/// - `empryo-chat-mirror`: holds Ghostex-only terminal-scraped `<id>.events.json` sidecars.
/// - `opencode-chat-mirror`: `session_chat_opencode::resolve_transcript` falls back to the mirror
///   when OpenCode's live HTTP service is stopped.
const DERIVED_CHAT_MIRROR_DIRS: &[&str] = &[
    "cursor-chat-mirror",
    "freebuff-chat-mirror",
    "zcode-chat-mirror",
    "dsh-session-mirror",
];

pub(crate) fn visual_checks_dir() -> PathBuf {
    std::env::temp_dir().join("ghostex-visuals")
}

/// Deletes `ghostex visual check` PNGs in `$TMPDIR/ghostex-visuals` older than
/// [`VISUAL_CHECK_MAX_AGE`].
pub(crate) fn prune_visual_checks(now: SystemTime) -> Tally {
    prune_files_in(&visual_checks_dir(), VISUAL_CHECK_MAX_AGE, now)
}

/// Deletes derived chat-mirror files older than [`CHAT_MIRROR_MAX_AGE`]. Never touches any
/// agent's own transcript store.
pub(crate) fn prune_chat_mirrors(gxserver_state_dir: &Path, now: SystemTime) -> Tally {
    let mut tally = Tally::default();
    for folder in DERIVED_CHAT_MIRROR_DIRS {
        tally.add(prune_tree(
            &gxserver_state_dir.join(folder),
            Since::Modified,
            CHAT_MIRROR_MAX_AGE,
            now,
        ));
    }
    tally
}

/// Deletes stale `command-session-*.state` and `command-session-*.command.tmp` files in
/// `<state>/gpui-command-actions` older than [`COMMAND_ACTION_MAX_AGE`].
pub(crate) fn prune_command_actions(app_state_dir: &Path, now: SystemTime) -> Tally {
    let dir = app_state_dir.join("gpui-command-actions");
    let mut tally = Tally::default();
    let Ok(entries) = fs::read_dir(&dir) else {
        return tally;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("command-session-")
            || !(name.ends_with(".state") || name.ends_with(".command.tmp"))
        {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_file() && is_older(&metadata, Since::Modified, COMMAND_ACTION_MAX_AGE, now) {
            remove_file(&path, &metadata, &mut tally);
        }
    }
    tally
}

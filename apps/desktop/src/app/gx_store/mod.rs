//! The Rust store (`ghostex-gx-core`) running inside the desktop app. Per-concern files:
//! `host.rs` owns the core, the client and the pump; `effects.rs` performs what the core asks
//! for; `local_focus.rs` makes selections local and draws the row highlight; `burst.rs` finishes a
//! selection once and releases deferred work when it settles; `focus_publish.rs` hands the store's
//! focus to the workspace and `focus_perform.rs` performs a session or group focus;
//! `session_walk.rs` walks the rendered sidebar rows for the previous and next session hotkeys;
//! `remote_clients.rs` runs one client per connected remote machine and owns the machine tabs,
//! and `remote_last_seen.rs` keeps the last-seen copy of every remote machine, reading one back to
//! seed a machine that has not connected in this run and writing it as the machine's rows move,
//! through `records_storage.rs`, the door to the `records` table an indexeddb-catalogued store
//! lives in rather than the `preferences` one every other door here uses;
//! `layout_persist.rs` writes the shell layout on a timer; `sidebar_list.rs` builds the sidebar
//! list (`_inputs` mirrors what it reads from outside the store, `sidebar_ui_storage.rs` reads the
//! hidden projects) and `sidebar_self_check.rs` rebuilds it from scratch every so often to catch a
//! cache that failed to invalidate, and writes the store's periodic counters;
//! `sidebar_menus.rs` builds the menus, hover buttons and header buttons the drawn list carries;
//! `sidebar_actions.rs` performs what a menu row, hover button or header button does, and
//! `sidebar_lifecycle.rs` the ones with a daemon round trip in the middle (sleep, wake, close
//! and fork), `sidebar_flags.rs` the four that are one call with different fields,
//! `sidebar_modals.rs` the two that only open a dialog,
//! `sidebar_accounts.rs` the two account pages (the agent launcher's and a row's Switch Account), `sidebar_open.rs` the family whose whole
//! answer is an app-modal-host message (the More menu's rows, a machine's Configure, the Space
//! editor, a project's Add Worktree and History), `sidebar_state_actions.rs` a row's Delayed Send,
//! the agent launcher's run and a machine tab's Hide Machine, and `sidebar_snooze.rs` the two that read
//! the clock and the local calendar, `sidebar_reload.rs` Full Reload and Split Right,
//! `sidebar_bulk.rs` the plural payloads and the renderer's
//! batch envelope, `sidebar_remote.rs` every per-session action of a row on a remote machine,
//! `sidebar_remote_focus.rs` the one remote payload that is not a call at all, a row click,
//! `sidebar_drag.rs` the session moves and what their order messages write, with
//! `sidebar_drop_queue.rs` holding a drop made before the document it edits was read, and
//! `workspace_groups.rs` the client-owned groups document those writes land in, with its stored
//! key, its debounced push and the guard that refuses the daemon's echo while one is outstanding;
//! `project_docs.rs` the PROJECT moves (reorder, into and out of a collection, Space membership)
//! and the two documents they write, on the generic `client_document.rs` host that owns the stored
//! key, the debounced push and the echo funnel for any client-owned document, with
//! `collection_menu.rs` holding the three Project Group menu items that write the same collections
//! document (Rename, a colour, Ungroup), `space_editor.rs` the New/Edit Space dialog's result,
//! `remote_project_docs.rs` the same two documents on a REMOTE machine, which are held rather than
//! owned and go back down that machine's own tunnel,
//! `space_switch.rs` the row a Space switch restores the focus to,
//! `added_project.rs` the Space and the order a newly added project joins, and
//! `diagnostics_project_docs.rs` their record lines;
//! `sidebar_ui_paths.rs` holds the three routes into the sidebar's own state that are NOT
//! sidebar commands (the per-Space session memory, the Space-editor delete, and the project slot
//! hotkey), and `sidebar_slot_jump.rs` the rest of that hotkey's jump, its focus and its reveal;
//! `sidebar_runtime_route.rs` sends a command the store did not perform itself straight to the
//! runtime, which is where the sidebar page used to forward it;
//! `runtime_facts.rs` ingests the runtime's one-way channel of the facts the list still borrows
//! from the old projection (the HUD, a project's git numbers, the two armed timers and a reveal
//! request) and compares it with the publish, with `diagnostics_runtime_facts.rs` writing its
//! periodic line, and `sidebar_clock.rs` owns the once-a-second tick the armed-timer labels and
//! the menu-host re-read ride;
//! `diagnostics.rs` writes the log lines.

mod activation_focus;
mod app_shot;
mod added_project;
mod attention;
mod burst;
mod client_document;
mod client_storage_init;
mod collection_menu;
mod create;
mod custom_tags_sync;
mod diagnostics;
mod diagnostics_open;
mod diagnostics_project_docs;
mod diagnostics_remote_last_seen;
mod diagnostics_runtime_facts;
mod effects;
mod focus_perform;
mod focus_publish;
pub(crate) mod git;
mod host;
mod hud;
mod indicators;
mod layout_persist;
mod presentation_ready;
mod primary_launcher;
mod local_delayed_sends;
mod local_focus;
mod notifications;
mod project_activation;
mod project_docs;
mod quick_access_data;
mod records_storage;
mod remote_clients;
mod remote_last_seen;
mod remote_last_seen_prune;
mod remote_project_docs;
mod remote_recent_projects;
mod renderer_commands;
mod rpc;
mod rpc_types;
mod runtime_facts;
mod session_walk;
mod sidebar_accounts;
mod sidebar_actions;
mod sidebar_bulk;
mod sidebar_command_run;
mod sidebar_clock;
mod sidebar_close_project;
mod sidebar_drag;
mod sidebar_drop_queue;
mod sidebar_flags;
mod sidebar_focus_route;
mod sidebar_lifecycle;
mod sidebar_list;
mod sidebar_list_inputs;
mod sidebar_menus;
mod sidebar_modals;
mod sidebar_more_menu;
mod sidebar_open;
mod sidebar_ready;
mod sidebar_reload;
mod sidebar_remote;
mod sidebar_remote_focus;
mod sidebar_runtime_route;
mod sidebar_scratch_compare;
mod sidebar_self_check;
mod sidebar_session_slot;
mod sidebar_slot_jump;
mod sidebar_snapshot;
mod sidebar_snooze;
mod sidebar_space_follow;
mod sidebar_state_actions;
mod sidebar_ui;
mod sidebar_ui_commands;
mod sidebar_ui_paths;
mod sidebar_ui_storage;
mod space_editor;
mod space_sleep;
mod space_switch;
pub(crate) mod terminal_lifecycle;
mod workspace_groups;

/// The client-storage doors, for the other Rust hosts in this app that own catalogued stores of
/// their own. The chat host (`src/app/gx_chat/storage.rs`) writes its own catalog rows through the
/// same connection pool rather than opening a second one: there is one client-storage database and
/// one busy timeout, and a second pool would deadlock against this one.
pub(crate) use records_storage::{
    RecordRead, RecordStore, read_record_raw, remove_record, scan_record_raw, write_record,
};
pub(crate) use sidebar_ui_storage::{
    read_preference_value, with_read_connection, with_write_connection, write_client_document_value,
};

pub(crate) use activation_focus::{menu_bar_session_focus_id, palette_session_focus_id};
pub(crate) use client_storage_init::initialize_client_storage_at_start;
pub(crate) use host::GxStoreHost;
pub(crate) use primary_launcher::read_primary_agent_launcher_id;
#[allow(unused_imports)] // the first callers arrive with the runtime port's family commits
pub(crate) use rpc::{gx_rpc, gx_rpc_with_timeout};
#[allow(unused_imports)]
pub(crate) use rpc_types::GxRpcError;
pub(crate) use workspace_groups::note_native_host_message_dropped;

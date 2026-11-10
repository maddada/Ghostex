//! The chat host itself: every retained chat, what each asks for, and what goes back to its view.
//!
//! This file does not know where it runs. `worker.rs` feeds it: on the desktop from one background
//! thread over a channel, in the GPUI web build from the page's own thread
//! (`apps/gpui-web/src/app/gx_chat/worker.rs`). Everything here is the same code in both.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use ghostex_gx_chat_client::{Endpoint, Inbound};
use ghostex_gx_chat_core::{ChatContext, Effect, Event, HostRequest};
use serde_json::Value;
use web_time::Instant;

use super::boot;
use super::diagnostics::{HostCounters, HostDiagnostics};
use super::draft_ops;
use super::effects::{self, Routed};
use super::events;
use super::frame;
use super::host_records;
use super::identity::ChatIdentity;
use super::locale;
use super::outbox::{self, Workers};
use super::platform;
use super::queries;
use super::read_aloud::{self, ReadAloudHost, ReadAloudInput};
use super::refusals;
use super::retained;
use super::saves;
use super::storage;
use super::store::ChatStore;
use super::transport::{self, Transport};

/// What a drained chat hands its view. The desktop's `native_chat` knows it as `ChatRuntimeOutput`,
/// the name the web build's own runtime uses for the same two cases.
pub(crate) enum ChatHostOutput {
    Drained(frame::DrainedFrame),
    /// The host failed in a way the view should draw instead of the chat. One thing raises it: a
    /// panic inside the Rust brain, which disables that ONE chat (see [`disable_chat`]). A storage
    /// refusal is counted rather than fatal.
    Error(String),
}

/// What a chat whose brain panicked draws instead of its transcript.
///
/// A whole sentence, and a constant one: it reaches the view's error banner
/// (`native_chat/render.rs` draws `self.error` above the transcript), and a panic payload can carry
/// anything the failing code was holding, which for this crate is the user's conversation.
///
/// CDXC:SessionChat 2026-09-25 WHY:
/// It names the one thing that works. "Reopen this chat" is what [`step`]'s `Attach` arm refuses
/// with this very message, and the Chat brain setting the 2026-09-22 sentence offered is deleted, so
/// a restart (a reload in the web build), which clears `disabled`, is the recovery left.
const HOST_PANIC_MESSAGE: &str = "Chat could not be shown. Restart Ghostex to open it again.";

/// What a chat draws when client storage answered none of its boot read.
///
/// It goes into the DOCUMENT rather than into the view's error banner, because it is the core's
/// `{status: 'error', error}` and the whole transcript is emptied with it. A sentence, and a
/// constant one: the reason a store refused is a path and a SQLite message, neither of which
/// belongs in a pane.
const BOOT_READ_FAILURE: &str =
    "Chat could not read its saved drafts and settings on this computer.";

/// How deep a storage answer may feed back into the core within one drive pass.
///
/// Each answer can ask for the next read, and a rule that asked for a record it had just written
/// would otherwise spin the thread. Twelve is well past the longest real chain (the boot read, then
/// the notice, the question drafts and the retired-question list).
const MAX_SETTLE_ROUNDS: usize = 12;

/// One attached view.
pub(super) struct Sink {
    pub(super) id: u64,
    pub(super) outputs: mpsc::Sender<ChatHostOutput>,
    pub(super) wake: Arc<dyn Fn() + Send + Sync>,
    /// The revision this view last drained, so a frame carries only what changed.
    pub(super) last_revision: u64,
    /// The view is off screen: it is sent the requests it must perform, but no document until it is
    /// shown again (`session_chat_warm_pool.rs`).
    pub(super) paused: bool,
}

/// Everything a runner hands the host.
pub(super) enum HostCommand {
    Attach {
        identity: ChatIdentity,
        /// A remote chat's gxserver, from its view's config.
        endpoint: Option<Endpoint>,
        sink: Sink,
    },
    Detach {
        key: String,
        sink: u64,
    },
    Call {
        key: String,
        /// The view that made the call, which becomes the one performing the chat's requests.
        sink: u64,
        method: &'static str,
        arguments: Vec<Value>,
    },
    Query {
        key: String,
        method: &'static str,
        arguments: Vec<Value>,
        reply: mpsc::Sender<Option<Value>>,
    },
    /// Pauses or resumes one view's document.
    Pause {
        key: String,
        sink: u64,
        paused: bool,
    },
    /// Where a machine's gxserver is.
    Endpoint {
        machine_id: String,
        endpoint: Endpoint,
    },
    /// What the chat socket read.
    Inbound(Inbound),
    /// Read aloud: what only the app knows, and the speaker's answers (`read_aloud.rs`).
    ReadAloud(ReadAloudInput),
    /// A session's computer changed (`computers.rs`); the turn's computer pass hands it on.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    Computers,
}

/// How many renderer requests one chat with no view attached may hold before the chat is released.
///
/// A chat is retained without a view for up to five minutes (`store.rs`), and the requests it
/// raises in that window are the view's to perform, so they wait for one. The bound is what keeps a
/// chat nobody reopens from growing a list nobody reads; 128 is well past what a retained chat's
/// timers produce in five minutes.
const MAX_HELD_REQUESTS: usize = 128;

/// One view attached to a chat, with its own record of what it was sent.
struct View {
    sink: Sink,
    sent: ghostex_gx_chat_core::SentFrame,
}

/// Every chat, and what the host counts about them.
#[derive(Default)]
pub(super) struct World {
    pub(super) store: ChatStore,
    /// The views of each chat, the ACTIVE one last.
    ///
    /// CDXC:AppWindows 2026-10-01 WHY:
    /// One chat can be on screen in several workspace windows at once (File > New Window), each its own view. A second view used to REPLACE the first, which then froze: it was sent nothing, and its resume was ignored because the host held the other sink. Every shown view is now sent the document, each against its own `SentFrame`, so none splices against rows it never had. Exactly one view, the last one attached, shown or used, performs what the chat asks of a view (its RPCs, saves, composer edits and opens), because those must happen once.
    views: std::collections::BTreeMap<String, Vec<View>>,
    wakes: std::collections::BTreeMap<String, Instant>,
    pub(super) counters: HostCounters,
    diagnostics: HostDiagnostics,
    /// The draft saves in flight, by request id, so the outbox row is cleared when gxserver has it.
    pub(super) draft_saves: std::collections::BTreeMap<u64, host_records::PendingDraft>,
    /// One retry worker per chat, which drains what a refused save left in the outbox.
    pub(super) draft_workers: Workers,
    /// When each chat's retry ladder is next due. Its own map, because `wakes` is the core's timer
    /// and `Effect::SetTimer` owns that one exclusively.
    pub(super) retry_wakes: std::collections::BTreeMap<String, Instant>,
    /// The `composer('park')` answer a handoff owes its `draftSubmitted` request.
    parked: std::collections::BTreeMap<String, draft_ops::ParkResult>,
    /// Delivery receipts already written, so a re-read of the same synced draft writes nothing.
    pub(super) delivered: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    /// What a chat with no attached view owes one, delivered when a view arrives.
    held_requests: std::collections::BTreeMap<String, Vec<HostRequest>>,
    /// The chat this thread is working on, so a panic knows which one to disable.
    driving: Option<String>,
    /// Chats whose brain panicked. They are not rebuilt: the same input would panic again.
    disabled: std::collections::BTreeSet<String>,
    /// The chat socket, and what the host knows about it.
    pub(super) transport: Transport,
    /// A context-detail preference one chat wrote, owed to every other chat once the step ends.
    broadcasts: Vec<(String, Event)>,
    /// When the timer pass last compared the chats' settings with Settings.
    settings_checked_at: Option<Instant>,
    /// The session list revision each chat was last handed (`session_directory.rs`).
    directory_sent: std::collections::HashMap<String, u64>,
    /// The computers revision each chat was last handed (`computers.rs`).
    computer_sent: std::collections::HashMap<String, u64>,
    /// Read aloud's app-wide facts and the one speaker (`read_aloud.rs`).
    pub(super) read_aloud: ReadAloudHost,
}

/// When the host must wake with no command: the core's timers and the draft retry ladder.
pub(super) fn next_wake(world: &World) -> Option<Instant> {
    world
        .wakes
        .values()
        .chain(world.retry_wakes.values())
        .copied()
        .min()
}

/// One command, or one pass of the due timers, then the retention prune and the counters.
pub(super) fn turn(world: &mut World, command: Option<HostCommand>) {
    // CDXC:SessionChat 2026-09-22 WHY:
    // ONE host carries every chat, so a panic in one chat's rules would otherwise end all of
    // them AND leave the runner holding a sender nobody reads: every later call would succeed
    // into a dead channel, every chat would freeze with no error drawn, and a paint's pure query
    // would wait out its timeout. The panic is caught per command instead: the one chat is
    // dropped, its view is told, and the host goes on. An unwind cannot be avoided by being
    // careful, because `packages/gx-chat-core` indexes and unwraps like any other crate.
    //
    // The retention pass is INSIDE the guard: `ChatStore::prune` measures an idle chat by
    // serializing its whole document, which is `packages/gx-chat-core`'s `Serialize` code, so a
    // panic there would have ended the host outright with none of the above applying.
    let pruned = std::panic::catch_unwind(AssertUnwindSafe(|| {
        step(world, command);
        directory_pass(world);
        computer_pass(world);
        flush_broadcasts(world);
        read_aloud::flush(world);
        world.store.prune()
    }));
    match pruned {
        Ok(pruned) => {
            for key in pruned {
                purge(world, &key);
            }
        }
        Err(_) => disable_chat(world),
    }
    world.counters.sessions_retained = world.store.len();
    world.counters.sessions_evicted = world.store.evicted;
    let counters = world.counters.clone();
    world.diagnostics.summary(&counters);
}

/// Drops the chat this thread was working on when its brain panicked.
///
/// The chat is REMOVED rather than reset: `World` is only assumed-unwind-safe because the one piece
/// of state a panicking core can have left half written is that chat's own entry, and dropping it
/// is what makes that true. Rebuilding it is not offered either, because the input that panicked
/// is on its way back in as soon as the view retries.
///
/// A panic that belongs to NO chat is still recorded. `driving` is `None` for the stretch of the
/// timer pass between its two loops and for a command naming a chat that is already gone, and a
/// silent unwind there was the one failure this guard could not be noticed by: nothing drawn,
/// nothing counted, and the thread carrying on as if the pass had run.
fn disable_chat(world: &mut World) {
    world.counters.panics += 1;
    if let Some(key) = world.driving.take() {
        world.store.remove(&key);
        purge(world, &key);
        world.disabled.insert(key.clone());
        for view in world.views.remove(&key).unwrap_or_default() {
            let posted = view
                .sink
                .outputs
                .send(ChatHostOutput::Error(HOST_PANIC_MESSAGE.to_string()))
                .is_ok();
            if posted {
                (view.sink.wake)();
            }
        }
        world.counters.chats_disabled += 1;
    }
    // Unconditional: "panic" in the event name is what `support_logs` reads as an important
    // diagnostic, and a chat whose brain died is one the user can see. Counts only, as ever: no
    // key, no session id, and never the payload, which is whatever the failing code was holding.
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::SessionChat,
        "gxChat.host.chatDisabledAfterPanic",
        serde_json::json!({
            "panics": world.counters.panics,
            "chatsDisabled": world.counters.chats_disabled,
        }),
    );
}

/// Forgets everything keyed by one chat, so a map does not outlive the chat it belongs to.
///
/// CDXC:Drafts 2026-09-22 WHY:
/// `draft_saves` is NOT purged here, unlike the six maps above it. Its entries are keyed by the
/// request id of a `setSessionChatDraft` the view is still holding, and `settle_draft_save` runs
/// before the store is even consulted, so the answer still arrives and still clears the outbox row
/// it queued. Dropping the entry with the chat meant a save that was in flight when the retention
/// prune took its chat (twelve chats open, this one the least recently touched) was never
/// acknowledged: the row stayed in the outbox and went out again on the next open, delivering a
/// revision gxserver already had. The map is bounded on its own at 256 in flight, which is where a
/// view that never answers is accounted for.
fn purge(world: &mut World, key: &str) {
    world.wakes.remove(key);
    world.retry_wakes.remove(key);
    world.draft_workers.remove(key);
    world.parked.remove(key);
    world.delivered.remove(key);
    world.held_requests.remove(key);
    world.directory_sent.remove(key);
    world.computer_sent.remove(key);
    read_aloud::forget(world, key);
    // The chat is gone from the store, so its identity is read back from its key.
    if let Some(identity) = ChatIdentity::from_retention_key(key) {
        world.transport.unfollow(key, &identity);
    }
}

/// One command, or one pass of the due timers.
///
/// `driving` names the chat this pass belongs to for as long as the pass runs, so a panic anywhere
/// inside it disables the right chat rather than none. The timer pass sets it per chat, because it
/// walks several.
fn step(world: &mut World, command: Option<HostCommand>) {
    world.driving = match &command {
        Some(HostCommand::Attach { identity, .. }) => Some(identity.retention_key()),
        Some(
            HostCommand::Detach { key, .. }
            | HostCommand::Call { key, .. }
            | HostCommand::Query { key, .. }
            | HostCommand::Pause { key, .. },
        ) => Some(key.clone()),
        Some(
            HostCommand::Endpoint { .. }
            | HostCommand::Inbound(_)
            | HostCommand::ReadAloud(_)
            | HostCommand::Computers,
        )
        | None => None,
    };
    match command {
        Some(HostCommand::Attach {
            identity,
            endpoint,
            sink,
        }) => {
            let key = identity.retention_key();
            if let (Some(machine_id), Some(endpoint)) = (identity.machine_id.as_deref(), endpoint) {
                world.transport.set_endpoint(machine_id, endpoint);
            }
            // A chat whose brain panicked is not rebuilt, and its view is told again rather than
            // being left drawing an empty pane.
            if world.disabled.contains(&key) {
                let posted = sink
                    .outputs
                    .send(ChatHostOutput::Error(HOST_PANIC_MESSAGE.to_string()))
                    .is_ok();
                if posted {
                    (sink.wake)();
                }
                return;
            }
            {
                let retained = world.store.entry(&identity);
                retained.listeners = 1;
                retained.touched_at = Instant::now();
            }
            // CDXC:SessionChat 2026-09-23 WHY:
            // A new view starts with no rows, and a RETAINED core remembers what the previous
            // view was sent: its next frame was a splice against rows the new view never had,
            // so a chat switched away from and back to drew an empty transcript. The new view
            // must be sent every channel whole, which is what each QuickJS view got by booting
            // its own brain: it starts with an empty `SentFrame` of its own (see `World::views`).
            let views = world.views.entry(key.clone()).or_default();
            views.retain(|view| view.sink.id != sink.id);
            views.push(View {
                sink,
                sent: Default::default(),
            });
            // `replayDraftSaves`: a save a previous run could not deliver is still in the
            // outbox, and opening its chat is what registers the writer that drains it.
            world.draft_workers.entry(key.clone()).or_default();
            world.retry_wakes.insert(key.clone(), Instant::now());
            let mut events: Vec<Event> = settings_moved(world, &key, &boot::chat_settings())
                .into_iter()
                .collect();
            events.extend(read_aloud::attach(world, &key));
            events.extend(visibility_moved(world, &key));
            drive(world, &key, events);
        }
        Some(HostCommand::Detach { key, sink }) => {
            let Some(views) = world.views.get_mut(&key) else {
                return;
            };
            views.retain(|view| view.sink.id != sink);
            if views.is_empty() {
                world.views.remove(&key);
                if let Some(retained) = world.store.get_mut(&key) {
                    retained.listeners = 0;
                    retained.touched_at = Instant::now();
                }
            }
            if let Some(event) = visibility_moved(world, &key) {
                drive(world, &key, vec![event]);
            }
        }
        Some(HostCommand::Call {
            key,
            sink,
            method,
            arguments,
        }) => {
            // The view the user is using performs what this call makes the chat ask for.
            activate_view(world, &key, sink);
            // A `resolve` is the answer to a request the core made, and the view echoes the
            // core's own id back, so there is no order matching to do. A retry write is the
            // one exception: it is the HOST's own request, numbered above every id the core
            // can allocate, and the core must never see its answer.
            if method == "resolve" && saves::settle_retry_write(world, &key, &arguments) {
                return;
            }
            let events = if method == "resolve" {
                refusals::note_rpc_refusal(&mut world.counters, &arguments);
                saves::settle_draft_save(world, &key, &arguments);
                events::resolved(&arguments).into_iter().collect()
            } else {
                events::events_for(method, &arguments)
            };
            if events.is_empty() {
                refusals::note_unrouted(&mut world.counters, method);
            }
            drive(world, &key, events);
        }
        Some(HostCommand::Query {
            key,
            method,
            arguments,
            reply,
        }) => {
            let answered = std::panic::catch_unwind(AssertUnwindSafe(|| {
                world.store.get(&key).and_then(|retained| {
                    let context = context(retained.core.state());
                    queries::answer(retained.core.state(), context, method, &arguments)
                })
            }));
            match answered {
                // A query changes nothing, so it does not drain; the view uses the answer at once.
                Ok(answer) => {
                    let _ = reply.send(answer);
                }
                Err(payload) => {
                    // The waiting view is answered FIRST, and only then does the unwind go on to
                    // the guard that disables the chat. `query_for_gesture` blocks the UI thread on
                    // this channel, so a panic that left it unanswered stalled the gesture for the
                    // whole timeout before the pane could draw anything at all, error included.
                    let _ = reply.send(None);
                    std::panic::resume_unwind(payload);
                }
            }
        }
        Some(HostCommand::Pause { key, sink, paused }) => {
            let Some(view) = world
                .views
                .get_mut(&key)
                .and_then(|views| views.iter_mut().find(|view| view.sink.id == sink))
            else {
                return;
            };
            let resumed = view.sink.paused && !paused;
            view.sink.paused = paused;
            if paused {
                // Hidden: a view still on screen in another window takes over the requests.
                if let Some(views) = world.views.get_mut(&key)
                    && views.last().is_some_and(|active| active.sink.id == sink)
                    && let Some(shown) = views.iter().rposition(|view| !view.sink.paused)
                {
                    let shown = views.remove(shown);
                    views.push(shown);
                }
            } else {
                activate_view(world, &key, sink);
            }
            match visibility_moved(world, &key) {
                // The chat's held clocks catch up in the same turn, and the drive drains it.
                Some(event) => drive(world, &key, vec![event]),
                // Shown again: everything the view missed, as one drain from the revision it last
                // had.
                None if resumed => publish(world, &key, Vec::new()),
                None => {}
            }
        }
        Some(HostCommand::Endpoint {
            machine_id,
            endpoint,
        }) => world.transport.set_endpoint(&machine_id, endpoint),
        Some(HostCommand::Inbound(inbound)) => transport::receive(world, inbound),
        Some(HostCommand::ReadAloud(input)) => read_aloud::receive(world, input),
        Some(HostCommand::Computers) => {}
        None => {
            settings_pass(world);
            let due: Vec<String> = world
                .retry_wakes
                .iter()
                .filter(|(_, at)| **at <= Instant::now())
                .map(|(key, _)| key.clone())
                .collect();
            for key in due {
                world.retry_wakes.remove(&key);
                world.driving = Some(key.clone());
                saves::drain_outbox(world, &key);
            }
            world.driving = None;
            let due: Vec<String> = world
                .wakes
                .iter()
                .filter(|(_, at)| **at <= Instant::now())
                .map(|(key, _)| key.clone())
                .collect();
            for key in due {
                world.wakes.remove(&key);
                drive(world, &key, vec![Event::Tick]);
            }
        }
    }
}

/// The chat settings again, for a RETAINED chat whose copy is older than Settings.
///
/// A QuickJS chat re-read them with every new view, because each view booted its own brain. A core
/// here outlives its views by up to five minutes and boots once, so without this a chat reopened
/// after "Hide account emails" was switched kept drawing with the value it booted with. A chat that
/// has not booted yet gets them from its boot read instead.
fn settings_moved(
    world: &World,
    key: &str,
    settings: &ghostex_gx_chat_core::ChatSettings,
) -> Option<Event> {
    let state = world.store.get(key)?.core.state();
    if !state.core.controller_started {
        return None;
    }
    (settings.hide_account_emails != state.core.hide_account_emails
        || settings.title != state.core.title
        || settings
            .catch_up
            .is_some_and(|catch_up| catch_up != state.core.catch_up_enabled)
        || settings
            .session_details
            .is_some_and(|enabled| enabled != state.core.session_details_enabled)
        || settings
            .send_on_pick
            .is_some_and(|enabled| enabled != state.core.send_on_pick_enabled)
        || settings
            .all_orchestrator_efforts
            .is_some_and(|enabled| enabled != state.core.all_orchestrator_efforts_enabled)
        || settings
            .tool_cards
            .is_some_and(|enabled| enabled != state.transcript_view.tool_cards)
        || settings
            .detailed_default
            .is_some_and(|detailed| detailed != state.transcript_view.detailed_default)
        || settings.marked_heading_strength.is_some_and(|strength| {
            ghostex_gx_chat_core::transcript::marked_sections::heading_strength(strength)
                != state.core.marked_heading_strength
        })
        || settings.chat_text_strength.is_some_and(|strength| {
            ghostex_gx_chat_core::transcript::text_strength::text_strength(strength)
                != state.core.chat_text_strength
        })
        || settings.chat_look.as_ref().is_some_and(|look| {
            !state.transcript_view.chat_look_from_host
                || state.transcript_view.chat_look.with_settings(look)
                    != state.transcript_view.chat_look
        })
        || settings
            .quiet_view
            .is_some_and(|quiet| Some(quiet) != state.transcript_view.quiet_view_settings))
    .then(|| Event::SettingsChanged(Box::new(settings.clone())))
}

/// Whether any of a chat's views is on screen, when its core was last told otherwise.
///
/// A view that is attached and not paused counts as shown: the warm pool pauses a view only once it
/// has been off screen for a while (`session_chat_warm_pool.rs`), so flipping between a few chats
/// keeps their clocks running.
fn visibility_moved(world: &World, key: &str) -> Option<Event> {
    let hidden = world.store.get(key)?.core.state().core.hidden;
    let shown = world
        .views
        .get(key)
        .is_some_and(|views| views.iter().any(|view| !view.sink.paused));
    (shown == hidden).then_some(Event::Visibility { shown })
}

/// How often the timer pass compares the chats' settings with Settings.
const SETTINGS_RECHECK: Duration = Duration::from_secs(2);

/// Settings changed while chats were open: each started chat is told.
///
/// `subscribeNativeChatSettings` pushed the change from the app runtime's store; that store is
/// not the chat's any more, so the host compares on its own timer pass instead, which a chat with
/// a view keeps running (its stall watchdog), at most once every two seconds.
fn settings_pass(world: &mut World) {
    if world
        .settings_checked_at
        .is_some_and(|at| at.elapsed() < SETTINGS_RECHECK)
    {
        return;
    }
    world.settings_checked_at = Some(Instant::now());
    read_aloud::settings_pass(world);
    // Read once for every chat: each read copies the whole settings object.
    let settings = boot::chat_settings();
    let keys: Vec<String> = world.views.keys().cloned().collect();
    for key in keys {
        if let Some(event) = settings_moved(world, &key, &settings) {
            drive(world, &key, vec![event]);
        }
    }
}

/// Hands each started chat the sidebar store's session list when it holds an older one.
///
/// Run after every command and timer pass rather than on a timer of its own: the store publishes
/// from the UI thread, and a chat with a view is driven often enough (its frames, its paint's
/// queries, its stall watchdog) to pick a rename up within a moment.
fn directory_pass(world: &mut World) {
    let (revision, sessions) = super::session_directory::current();
    if revision == 0 {
        return;
    }
    let stale: Vec<String> = world
        .store
        .keys()
        .into_iter()
        .filter(|key| world.directory_sent.get(key) != Some(&revision))
        .filter(|key| {
            world
                .store
                .get(key)
                .is_some_and(|retained| retained.core.state().core.controller_started)
        })
        .collect();
    for key in stale {
        world.directory_sent.insert(key.clone(), revision);
        world.driving = Some(key.clone());
        let event = Event::SessionDirectoryChanged {
            sessions: (*sessions).clone(),
        };
        drive(world, &key, vec![event]);
    }
    world.driving = None;
}

/// Hands every chat whose copy is older the computer its session runs on (`computers.rs`). A chat
/// on this computer is told so once, which is what keeps its "Couldn't reach Ghostex" words.
fn computer_pass(world: &mut World) {
    let (revision, computers) = super::computers::current();
    let stale: Vec<String> = world
        .store
        .keys()
        .into_iter()
        .filter(|key| world.computer_sent.get(key) != Some(&revision))
        .filter(|key| {
            world
                .store
                .get(key)
                .is_some_and(|retained| retained.core.state().core.controller_started)
        })
        .collect();
    for key in stale {
        world.computer_sent.insert(key.clone(), revision);
        let Some(identity) = ChatIdentity::from_retention_key(&key) else {
            continue;
        };
        let computer = super::computers::for_machine(&computers, identity.machine_id.as_deref());
        world.driving = Some(key.clone());
        drive(world, &key, vec![Event::ComputerChanged(Some(computer))]);
    }
    world.driving = None;
}

/// Hands every other retained chat the context-detail preference one chat just wrote.
fn flush_broadcasts(world: &mut World) {
    for (origin, event) in std::mem::take(&mut world.broadcasts) {
        for key in world.store.keys() {
            if key != origin {
                drive(world, &key, vec![event.clone()]);
            }
        }
    }
}

/// Applies a burst of events to one chat, performs what the host owns, and drains a frame.
pub(super) fn drive(world: &mut World, key: &str, events: Vec<Event>) {
    if world.store.get(key).is_none() {
        return;
    }
    world.driving = Some(key.to_string());
    let mut pending = events;
    // Whatever this chat owed a view it did not have, in the order it was raised. An empty list
    // for every chat with a view attached, which is every chat a gesture reaches.
    let mut requests: Vec<HostRequest> = world.held_requests.remove(key).unwrap_or_default();
    let mut rounds = 0usize;
    while !pending.is_empty() && rounds < MAX_SETTLE_ROUNDS {
        rounds += 1;
        let mut answers: Vec<Event> = Vec::new();
        for event in std::mem::take(&mut pending) {
            // The core's borrow ends before an effect is performed, because performing one reads
            // and writes the store's own counters.
            let (effects, session_key) = {
                let Some(retained) = world.store.get_mut(key) else {
                    world.driving = None;
                    return;
                };
                retained.touched_at = Instant::now();
                let session_key = retained.session_key.clone();
                let context = context(retained.core.state());
                let effects = retained.core.handle(event, context);
                (effects, session_key)
            };
            for effect in effects {
                *world
                    .counters
                    .effects
                    .entry(effect_name(&effect))
                    .or_insert(0) += 1;
                match effects::route(effect) {
                    // Performed by doing nothing, because the shipped brain does nothing either
                    // (`effects::SWALLOWED_HOST_ACTIONS`). Counted by name so a deliberate no-op is
                    // still visible in `gxChat.host.summary`; the name is a code constant.
                    Routed::Swallowed(name) => {
                        *world
                            .counters
                            .host_actions_swallowed
                            .entry(name)
                            .or_insert(0) += 1;
                    }
                    Routed::Renderer(request) => {
                        if request.kind
                            == ghostex_gx_chat_core::RequestKind::Other(
                                effects::UNROUTED.to_string(),
                            )
                        {
                            world.counters.effects_unrouted += 1;
                        }
                        saves::note_draft_save(world, &session_key, &request);
                        requests.push(adopt_park_answer(world, key, *request));
                    }
                    // A gesture the core asked to have replayed at itself, which joins this
                    // round's answers so it settles inside the same drive pass.
                    Routed::SelfAction(action) => answers.push(Event::Action(action)),
                    Routed::Host(effect) => {
                        perform(
                            world,
                            key,
                            &session_key,
                            effect,
                            &mut answers,
                            &mut requests,
                        );
                    }
                }
            }
        }
        pending.append(&mut answers);
    }
    // A chain deeper than the cap leaves events unapplied, which is a rule the core grew and this
    // host never expected. It is counted rather than logged, because the events themselves are the
    // user's conversation.
    if !pending.is_empty() {
        world.counters.settle_rounds_exhausted += 1;
    }
    saves::record_deliveries(world, key);
    publish(world, key, requests);
    world.driving = None;
}

/// A handoff's `draftSubmitted` carries what `composer('park')` minted, which is the host's.
///
/// `native-host.ts` spread the park answer into the request (`{...handoff, text, version}`), and
/// the view reads `nextVersion` out of it to start the composer's next draft. The core has no
/// random source and no view state, so both the handoff id and the next revision are made here and
/// merged on the way past.
fn adopt_park_answer(world: &mut World, key: &str, mut request: HostRequest) -> HostRequest {
    if request.kind != ghostex_gx_chat_core::RequestKind::DraftSubmitted {
        return request;
    }
    // Every send adopts a fresh revision, which is what `composer('submitted')` answers with.
    request
        .params
        .insert("nextVersion".into(), boot::next_draft_version());
    if request.method != "handoff" {
        return request;
    }
    if let Some(park) = world.parked.remove(key) {
        request
            .params
            .insert("handoffId".into(), Value::String(park.handoff_id));
        request
            .params
            .insert("content".into(), Value::String(park.content));
        request
            .params
            .insert("draftVersion".into(), park.draft_version);
    }
    request
}

/// Performs one host-side effect and queues the event that answers it.
fn perform(
    world: &mut World,
    key: &str,
    session_key: &str,
    effect: Effect,
    answers: &mut Vec<Event>,
    requests: &mut Vec<HostRequest>,
) {
    let now_ms = now_millis();
    match effect {
        Effect::ReadStorage { key: storage_key } => {
            let value = read_storage(world, &storage_key, now_ms);
            answers.push(Event::StorageLoaded {
                key: storage_key,
                value,
            });
        }
        // One host operation that reads several records and answers once. The COUNT of round trips
        // is part of the contract, not an optimisation, so the batch stays one answer.
        Effect::ReadStorageBatch { keys } => {
            let records = keys
                .into_iter()
                .map(|storage_key| {
                    let value = read_storage(world, &storage_key, now_ms);
                    ghostex_gx_chat_core::StorageRecord {
                        key: storage_key,
                        value,
                    }
                })
                .collect();
            answers.push(Event::StorageBatchLoaded { records });
        }
        Effect::WriteStorage {
            key: storage_key,
            value,
            ..
        } => {
            // `durable` asks for a flush before the answer. Every write here lands in one immediate
            // SQLite transaction with `synchronous=FULL`, so it is already durable when it returns.
            let error = match write_storage(world, key, session_key, &storage_key, &value, now_ms) {
                Ok(()) => {
                    if let Some(event) =
                        transport::context_preference_event(&storage_key.store, value.as_deref())
                    {
                        world.broadcasts.push((key.to_string(), event));
                    }
                    None
                }
                Err(reason) => {
                    world.counters.storage_refused += 1;
                    Some(reason.to_string())
                }
            };
            answers.push(Event::StorageWritten {
                key: storage_key,
                error,
            });
        }
        // One outcome for the whole batch, because the host operation it stands for is one call.
        // The writes go out in the order given and the first refusal names the failure.
        Effect::WriteStorageBatch { writes } => {
            let mut keys = Vec::with_capacity(writes.len());
            let mut error = None;
            for write in writes {
                if let Err(reason) =
                    write_storage(world, key, session_key, &write.key, &write.value, now_ms)
                {
                    world.counters.storage_refused += 1;
                    error.get_or_insert_with(|| reason.to_string());
                }
                keys.push(write.key);
            }
            answers.push(Event::StorageBatchWritten { keys, error });
        }
        // `composer('flush')` is `flushDraftSaves(sessionKey)`: every stored write this door makes
        // is already on disk when it returns, so what is left to wait for is the outbox, and the
        // send must not deliver while a revision gxserver has not acknowledged is still queued.
        Effect::FlushStorage { store } => {
            let drained = outbox::pending(session_key, now_ms).is_empty();
            if !drained
                && let Some(request) = saves::next_retry_write(world, key, session_key, now_ms)
            {
                requests.push(request);
            }
            answers.push(Event::StorageWritten {
                key: ghostex_gx_chat_core::StorageKey {
                    store,
                    suffix: String::new(),
                },
                // A queued revision is a delivery the daemon has not taken yet, not a storage
                // failure: the ladder keeps trying and the send goes on, which is what the
                // TypeScript's already-resolved `flushDraftSaves` did for an empty queue.
                error: None,
            });
        }
        Effect::ReadComposerBoot { .. } => {
            let mut errors = boot::BootReads::default();
            let mut read = boot::read(session_key, now_ms, &mut errors);
            world.counters.storage_refused += errors.errors as u64;
            // The cached record is this computer's lineup; a chat on another computer starts on
            // that computer's (or the bundled one until it pushes), never this one's.
            if let Some(identity) = world
                .store
                .get(key)
                .map(|retained| &retained.identity)
                .filter(|identity| identity.machine_id.is_some())
            {
                read.model_catalog = world
                    .transport
                    .catalog(identity)
                    .cloned()
                    .unwrap_or(Value::Null);
            }
            // Client storage answered nothing at all, which is `composer('read')` rejecting rather
            // than a profile with nothing saved. The core empties the transcript and draws
            // `{status: 'error'}`; without this arm it never starts a controller and therefore
            // never publishes again, so the pane stays blank with no explanation.
            if errors.all_refused() {
                answers.push(Event::ComposerBootFailed {
                    error: BOOT_READ_FAILURE.to_string(),
                });
                return;
            }
            // `start` in `native-host.ts` pushed `{kind: 'composerInit', method: 'restore', params:
            // result}` from the same answer. It is the ONLY path that gives the view its client id,
            // its draft id and its draft revision, so without it every save is refused with "Two
            // editors changed the same draft revision" and the composer never reports itself ready.
            requests.push(HostRequest {
                id: None,
                kind: ghostex_gx_chat_core::RequestKind::ComposerInit,
                method: "restore".to_string(),
                params: match serde_json::to_value(&read) {
                    Ok(Value::Object(params)) => params,
                    _ => serde_json::Map::new(),
                },
            });
            answers.push(Event::ComposerBootRead(Box::new(read)));
        }
        // The retained transcript cache, keyed by this chat's RETENTION key, which is what `key`
        // already is (`identity.rs`: `JSON.stringify([machineId, projectId, sessionId])`, the same
        // string `session::persistence::storage_key` builds).
        Effect::ReadRetainedSnapshot => {
            let value = match retained::read(key, now_ms) {
                Ok(value) => value,
                Err(_) => {
                    world.counters.storage_refused += 1;
                    None
                }
            };
            answers.push(Event::RetainedSnapshotLoaded { value });
        }
        // The only writer of the record now: the app runtime's retained store that wrote it beside
        // the core until 2026-09-25 is deleted with its socket. Nothing answers the effect; a failed
        // cache write is not a failure the chat reports, and the live stream stays authoritative.
        Effect::WriteRetainedSnapshot { value } => {
            if retained::write(key, value.as_deref(), now_ms).is_err() {
                world.counters.storage_refused += 1;
            }
        }
        Effect::Subscribe { limit, catalog } => {
            if let Some(identity) = world
                .store
                .get(key)
                .map(|retained| retained.identity.clone())
            {
                world.transport.follow(key, &identity, limit);
            }
            // `broker.ts` posted the catalog in effect with every `catalog` subscribe.
            if let Some(catalog) = catalog
                .then(|| {
                    let identity = &world.store.get(key)?.identity;
                    world.transport.catalog(identity).cloned()
                })
                .flatten()
            {
                answers.push(Event::ModelCatalogChanged { catalog });
            }
        }
        Effect::Unsubscribe => {
            if let Some(identity) = world
                .store
                .get(key)
                .map(|retained| retained.identity.clone())
            {
                world.transport.unfollow(key, &identity);
            }
        }
        Effect::Reconnect => {
            if let Some(identity) = world
                .store
                .get(key)
                .map(|retained| retained.identity.clone())
            {
                world.transport.refresh(&identity);
            }
        }
        Effect::SetTimer { delay_ms } => match delay_ms {
            Some(delay) => {
                world.wakes.insert(
                    key.to_string(),
                    Instant::now() + Duration::from_millis(delay.max(1)),
                );
            }
            None => {
                world.wakes.remove(key);
            }
        },
        effect @ (Effect::Speak { .. }
        | Effect::StopSpeaking { .. }
        | Effect::SetReadAloudAuto { .. }) => read_aloud::perform(world, key, effect),
        _ => {}
    }
}

/// One stored read, or the one store the core names that is a QUERY rather than a record.
///
/// CDXC:SavedPrompts 2026-09-22 WHY:
/// `composerHistory` has no row anywhere: it is `composer('history')`, which
/// `native-composer.ts` answered with `listSentSessionChatMessages().map(m => m.content)
/// .reverse()`, a scan of the whole `sentHistory` store across every session rather than a read of
/// one key. The suffix it arrives with is the session it was asked from and is deliberately not
/// used, because Up-arrow recall reaches what every composer on this computer sent. Left to the
/// catalog the read answers `unregistered`, which the core reads as an empty ring: Up and Down
/// would do nothing and `historyActive` would be false for ever.
///
/// The array is OLDEST FIRST, which is what `.reverse()` of a newest-first list produces and what
/// `recall_previous` walks backwards from.
fn read_storage(
    world: &mut World,
    storage_key: &ghostex_gx_chat_core::StorageKey,
    now_ms: i64,
) -> Option<String> {
    if storage_key.store == ghostex_gx_chat_core::composer::storage::COMPOSER_HISTORY_STORE {
        let entries = host_records::sent_history_contents(now_ms);
        return serde_json::to_string(&entries).ok();
    }
    match storage::read(storage_key, now_ms) {
        Ok(value) => value,
        Err(_) => {
            world.counters.storage_refused += 1;
            None
        }
    }
}

/// One stored write, or one of the three draft operations the core names as a store.
///
/// `composer('submitted')`, `composer('park')` and `composer('receive')` are conditional on what is
/// already on disk and touch records the core does not own, so they are performed here rather than
/// written through (`draft_ops.rs`). Anything else is an ordinary catalogued row.
fn write_storage(
    world: &mut World,
    key: &str,
    session_key: &str,
    storage_key: &ghostex_gx_chat_core::StorageKey,
    value: &Option<String>,
    now_ms: i64,
) -> Result<(), &'static str> {
    use ghostex_gx_chat_core::composer::storage::{
        DRAFT_PARK_STORE, DRAFT_RECEIVE_STORE, DRAFT_SUBMITTED_STORE,
    };
    let operation = matches!(
        storage_key.store.as_str(),
        DRAFT_SUBMITTED_STORE | DRAFT_PARK_STORE | DRAFT_RECEIVE_STORE
    );
    if !operation {
        return storage::write(storage_key, value.as_deref(), now_ms);
    }
    let payload: Value = value
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(Value::Null);
    match storage_key.store.as_str() {
        DRAFT_SUBMITTED_STORE => {
            let mut refusals = 0u64;
            let settled = draft_ops::submitted(session_key, &payload, now_ms, &mut refusals);
            world.counters.storage_refused += refusals;
            settled
        }
        DRAFT_PARK_STORE => {
            let park = draft_ops::park(session_key, &payload, now_ms)?;
            world.parked.insert(key.to_string(), park);
            Ok(())
        }
        _ => draft_ops::receive(session_key, &payload, now_ms),
    }
}

/// Drains a frame and posts it, when it carries anything.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// A chat with no attached view HOLDS its requests rather than dropping them. The transport is the
/// view's (`effects.rs`), and a retained chat still ticks, so the requests a timer raises have
/// nowhere to go for as long as five minutes. Dropped, they took the core's own bookkeeping with
/// them: a `SendRpc` it never gets an answer to leaves its lane marked in flight for ever, which is
/// how the model-selection outbox and the draft retry worker each wedged on the first tick after a
/// chat was closed. The document itself was never at risk, because a drain with no sink does not
/// advance the revision and a new view reads the whole snapshot.
pub(super) fn publish(world: &mut World, key: &str, requests: Vec<HostRequest>) {
    if world.views.get(key).is_none_or(|views| views.is_empty()) {
        hold(world, key, requests);
        return;
    }
    let (Some(views), Some(retained)) = (world.views.get_mut(key), world.store.get_mut(key)) else {
        return;
    };
    let active = views.len() - 1;
    let mut requests = Some(requests);
    for (index, view) in views.iter_mut().enumerate() {
        // Only the active view performs the chat's requests (see `World::views`).
        let view_requests = if index == active {
            requests.take().unwrap_or_default()
        } else {
            Vec::new()
        };
        // A paused view still performs what the chat asks of it (its reads, its saves), so its
        // lanes settle; the document waits, undrained, for the view to be shown.
        if view.sink.paused {
            if !view_requests.is_empty() {
                let envelope = frame::envelope(
                    ghostex_gx_chat_core::Frame {
                        revision: view.sink.last_revision,
                        ..Default::default()
                    },
                    view_requests,
                );
                if view
                    .sink
                    .outputs
                    .send(ChatHostOutput::Drained(envelope))
                    .is_ok()
                {
                    (view.sink.wake)();
                }
            }
            continue;
        }
        // `frame_at` (here `frame_for`, its per-view twin), not `frame`: `take` in
        // `native-host.ts` read `Date.now()` itself, and measuring `nextWakeMs` against the clock
        // of the last event handled is one turn stale, so the host would arm its timer that much
        // late (`docs/2026-09-21/rust-chat/PROGRESS.md`, Integration 2 item 8).
        let drained =
            retained
                .core
                .frame_for(&mut view.sent, view.sink.last_revision, now_millis() as f64);
        let revision = drained.revision;
        let envelope = frame::envelope(drained, view_requests);
        let carries = frame::envelope_carries_change(&envelope);
        view.sink.last_revision = revision;
        if !carries {
            continue;
        }
        let posted = view
            .sink
            .outputs
            .send(ChatHostOutput::Drained(envelope))
            .is_ok();
        world.counters.frames_published += 1;
        if posted {
            (view.sink.wake)();
        }
    }
}

/// Makes `sink` the chat's active view: the one that performs its requests.
fn activate_view(world: &mut World, key: &str, sink: u64) {
    if let Some(views) = world.views.get_mut(key)
        && let Some(index) = views.iter().position(|view| view.sink.id == sink)
    {
        let view = views.remove(index);
        views.push(view);
    }
}

/// Keeps what a chat with no attached view owes one, and RELEASES the chat when it owes too much.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// At the cap the whole chat goes, rather than the oldest requests going one by one. Dropping a
/// single request is exactly the failure the held lane exists to prevent: a `SendRpc` the core never
/// gets an answer to leaves its lane marked in flight for ever, so a chat trimmed at the cap would
/// come back with the model-selection outbox or the draft retry worker permanently wedged, and
/// nothing would ever say so. Releasing the chat is the five-minute retention prune arriving early,
/// and it costs nothing that is not already on disk or on the wire: the draft, its outbox row and
/// its recovery checkpoints are the host's records, and the transcript refolds from the snapshot the
/// next attach subscribes for. The next view therefore reads a chat with no half-settled lanes
/// instead of one that looks alive and is not.
fn hold(world: &mut World, key: &str, requests: Vec<HostRequest>) {
    if requests.is_empty() {
        return;
    }
    let held = world.held_requests.entry(key.to_string()).or_default();
    held.extend(requests);
    if held.len() <= MAX_HELD_REQUESTS {
        return;
    }
    let forgotten = held.len() as u64;
    world.store.remove(key);
    purge(world, key);
    world.counters.requests_dropped += forgotten;
    world.counters.chats_released += 1;
}

/// The clock, the timezone, the random draws and the formatted stamps, once per turn.
///
/// The core reads none of them: `packages/gx-chat-core` builds for wasm and must cross UniFFI, so
/// every one of them is an input. The draws are taken from the OS random source a v4 UUID uses
/// rather than a seeded generator, because the one rule that reads them is the working strip's
/// stint word and a repeated seed would freeze it on one word. The ids are raw entropy:
/// `ChatContext::random_id(slot)` forces the version and variant bits when it prints one back as a
/// canonical UUID, so the host must not pre-format them.
///
/// It is built with the `with_*` setters rather than an exhaustive struct literal, because an
/// input the core adds then costs this host nothing and falls back to what it did before.
fn context(state: &ghostex_gx_chat_core::ChatState) -> ChatContext {
    let bytes = platform::random_bytes();
    let draw = |at: usize| -> f64 {
        let mut value = 0u64;
        for byte in bytes[at..at + 7].iter() {
            value = (value << 8) | u64::from(*byte);
        }
        value as f64 / (1u64 << 56) as f64
    };
    let utc_offset_minutes = platform::utc_offset_minutes();
    ChatContext::at(now_millis() as f64)
        .with_utc_offset_minutes(utc_offset_minutes)
        .with_random_units([draw(0), draw(8)])
        .with_random_ids([
            u128::from_be_bytes(bytes),
            u128::from_be_bytes(platform::random_bytes()),
        ])
        .with_formatted_times(locale::formatted_times(state, utc_offset_minutes))
}

pub(super) fn now_millis() -> i64 {
    platform::now_millis()
}

/// The variant name of an effect, for the counters. Never its payload.
fn effect_name(effect: &Effect) -> &'static str {
    match effect {
        Effect::SendRpc { .. } => "sendRpc",
        Effect::Subscribe { .. } => "subscribe",
        Effect::Unsubscribe => "unsubscribe",
        Effect::Reconnect => "reconnect",
        Effect::ReadStorage { .. } => "readStorage",
        Effect::ReadStorageBatch { .. } => "readStorageBatch",
        Effect::ReadComposerBoot { .. } => "readComposerBoot",
        Effect::ReadRetainedSnapshot => "readRetainedSnapshot",
        Effect::WriteRetainedSnapshot { .. } => "writeRetainedSnapshot",
        Effect::UpdatePresentation { .. } => "updatePresentation",
        Effect::WriteStorage { .. } => "writeStorage",
        Effect::WriteStorageBatch { .. } => "writeStorageBatch",
        Effect::FlushStorage { .. } => "flushStorage",
        Effect::SetTimer { .. } => "setTimer",
        Effect::SetComposerText { .. } => "setComposerText",
        Effect::ClearComposerIfUnchanged { .. } => "clearComposerIfUnchanged",
        Effect::Open(_) => "open",
        Effect::Copy { .. } => "copy",
        Effect::Toast { .. } => "toast",
        Effect::RestoreReturnedPrompt { .. } => "restoreReturnedPrompt",
        Effect::MarkdownSaved { .. } => "markdownSaved",
        Effect::HostAction { .. } => "hostAction",
        Effect::Speak { .. } => "speak",
        Effect::StopSpeaking { .. } => "stopSpeaking",
        Effect::SetReadAloudAuto { .. } => "setReadAloudAuto",
        _ => "unrouted",
    }
}

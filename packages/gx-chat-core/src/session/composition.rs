//! The composed list: the authoritative transcript plus everything that has no record yet.
//!
//! Ported from the `assembled` / `surfaced` / `boundaried` / `messages` chain of
//! `packages/shared/session-chat-controller/controller.ts`. Order matters and is the contract:
//! transcript, then terminal statuses, then app commands, then markers, then the streaming
//! bubble, then the pending tool row, then the pending echoes.

use crate::state::ChatRow;
use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource};

use crate::session::app_commands::{
    app_commands_as_messages, reconcile_local_command_output,
    retire_markers_covered_by_local_commands,
};
use crate::session::assembler::assembled_messages;
use crate::session::markers::{
    apply_marker_boundaries_in, markers_as_messages, retire_interrupt_markers,
};
use crate::session::pending::{pending_sends_as_messages, visible_pending_sends};
use crate::session::startup_sends::pending_with_startup_sends;
use crate::session::streaming::{
    derive_streaming_text, streaming_message, without_trailing_section_titles,
};
use crate::session::terminal::{
    terminal_stream_is_tool, terminal_stream_retired, unreconciled_terminal_statuses,
    visible_terminal_tool,
};
use crate::session::text::{collapse_whitespace, is_js_space, parse_command_envelope};
use crate::state::ChatState;

/// The id the drawn copy of a gate-held echo carries in its `startupDelivery` (`held_echo`).
pub const HELD_BY_GATE_PREFIX: &str = "gate-held-";
use crate::transcript::noise::is_compaction_record;

/// `countSessionChatCompactionRecords`: how many compactions the authoritative transcript records.
///
/// The optimistic "Ran /compact" marker retires against this count: once the agent has said the
/// compaction happened, a client-side "we sent it" row would sit BELOW the result it announced.
pub fn compaction_records(messages: &[crate::state::ChatRow]) -> usize {
    messages
        .iter()
        .filter(|message| is_compaction_record(message))
        .count()
}

fn text_of(message: &ChatMessage, separator: &str) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(separator)
}

/// The text two rows must share to be the same user turn, from `use-session-chat/state.ts`.
fn normalized_text(message: &ChatMessage) -> String {
    collapse_whitespace(&text_of(message, "\n\n"))
        .trim_matches(is_js_space)
        .to_string()
}

/// `surfaceSkillInvocationUserTurns`: every slash command recorded as a user turn becomes the
/// user's own bubble.
///
/// Claude-family harnesses record a slash input's user turn as a `<command-name>` envelope, and
/// gxserver replays the commands it archived for every other agent in the same shape
/// (`local-command:` rows, and the live `app-command-local:` acknowledgement before them).
///
/// CDXC:SessionChat 2026-09-27 WHY:
/// A slash command sent from chat reaches the transcript three ways: Claude's own envelope while
/// the chat is live, gxserver's archived row (`local-command:`) once a read has merged the archive
/// over it, and the live acknowledgement (`app-command-local:`) before either. Every shape follows
/// one rule, so a command never changes look between live and reload.
///
/// CDXC:SessionChat 2026-11-02 DECISION:
/// User, about the grey "Slash command · /rename …" and "Local command output · …" rows: "i dont like how these look, they dont look like the other parts of the chat transcript". They picked "D. Your bubble, then a note" and "Yes, every agent": every slash command, built-in or skill, typed in the chat or in the terminal, for every agent, is the user's own bubble with the command in monospace (`slash_command_markdown` in `transcript/local_command.rs`), and its printed output is a status note under it (`suppressed_turn_presentation` in `transcript/noise.rs`). Supersedes the 2026-09-27 rule that kept the agent's built-in commands as `Slash command` rows. The model, effort and Fast mode controls' commands stay hidden: their result row is the one record.
pub fn surface_skill_invocation_user_turns(messages: &[ChatRow]) -> Vec<ChatRow> {
    surface_skill_invocation_user_turns_in(messages.to_vec())
}

/// [`surface_skill_invocation_user_turns`] on a list the caller owns, rewritten in place rather
/// than copied twice.
pub fn surface_skill_invocation_user_turns_in(mut messages: Vec<ChatRow>) -> Vec<ChatRow> {
    // A prompt a hook refused is the user's bubble too, with its card under it (hook_blocked.rs).
    if crate::transcript::hook_blocked::has_hook_blocked_prompt(&messages) {
        messages = crate::transcript::hook_blocked::split_hook_blocked_prompts(messages);
    }
    for message in &mut messages {
        let plain_text = message
            .blocks
            .iter()
            .all(|block| matches!(block, ChatBlock::Text { .. }));
        if !matches!(message.role, ChatRole::User) || !plain_text {
            continue;
        }
        let Some(envelope) = parse_command_envelope(&text_of(message, "\n")) else {
            continue;
        };
        let catalog_name = envelope.name.trim_start_matches('/').to_lowercase();
        // CDXC:SessionChat 2026-09-26 WHY:
        // The model, effort and Fast mode controls type `/model`, `/effort` and `/fast`; their
        // result row is the one user-facing record, so the command row stays hidden
        // (`classify_suppressed_turn`).
        if matches!(catalog_name.as_str(), "model" | "effort" | "fast") {
            continue;
        }
        // The harness canonicalizes a plugin skill to `/plugin:name`, but the user typed the
        // SHORT name. gxserver's rows carry what the user typed, so they keep it whole.
        let typed_by_user = message.id.starts_with("local-command:")
            || message.id.starts_with("app-command-local:");
        let token = if typed_by_user {
            envelope.name.clone()
        } else {
            format!("/{}", short_command_name(&envelope.name))
        };
        std::sync::Arc::make_mut(message).blocks = vec![ChatBlock::Text {
            text: if envelope.args.is_empty() {
                token
            } else {
                format!("{token} {}", envelope.args)
            },
        }];
    }
    messages
}

/// `/plugin:name` as the user typed it: `name`.
pub fn short_command_name(name: &str) -> &str {
    name.trim_start_matches('/')
        .rsplit(':')
        .next()
        .unwrap_or_default()
}

/// The list a pending echo is reconciled against.
///
/// CDXC:SessionChat 2026-09-15 WHY:
/// Startup sends hydrate as pending bubbles even for `/usage`. Its command acknowledgment can
/// arrive before any transcript exists, and it never needs an assistant reply, so pending
/// reconciliation must include the server's live local-command records.
pub fn pending_transcript(state: &ChatState, boundaried: &[ChatRow]) -> Vec<ChatRow> {
    let local: Vec<serde_json::Value> = state
        .session
        .app_commands
        .iter()
        .filter(|entry| {
            entry
                .get("localCommand")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        })
        .cloned()
        .collect();
    let mut rows = boundaried.to_vec();
    rows.extend(app_commands_as_messages(&local, boundaried));
    rows
}

/// What `boundaried` depends on: the transport list, the markers and the agent.
///
/// Keyed on the list's own revision rather than the composition identity, which also moves for
/// every streamed update, app-command field and pending echo: each of those rebuilt and re-sorted
/// the whole transcript although nothing it is made from had changed.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundariedKey {
    pub list_revision: u64,
    pub markers: Vec<crate::state::CommandMarker>,
    pub agent: Option<String>,
}

impl BoundariedKey {
    fn of(state: &ChatState) -> Self {
        Self {
            list_revision: state.messages.list_revision,
            markers: state.pending.markers.clone(),
            agent: state.session.agent.clone(),
        }
    }
}

/// The dependencies of the `messages` memo in `controller.ts`, by identity where the TypeScript
/// compares identities and by value where its setters bail out on an equal value.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposeKey {
    pub identity: u64,
    pub working: bool,
    pub async_questions_since: Option<i64>,
    pub preview_text: Option<String>,
    pub sends: Vec<crate::state::PendingSend>,
    pub markers: Vec<crate::state::CommandMarker>,
    pub statuses: Vec<ChatRow>,
    pub stream: Option<crate::state::TerminalStream>,
    pub visible_tool: Option<ChatMessage>,
    pub queue: Option<Vec<serde_json::Value>>,
    pub app_commands: Vec<serde_json::Value>,
    pub shell: Option<serde_json::Value>,
    /// The echo whose send has not gone yet and what it says (`composer::delivery::echo_note`).
    pub held: Option<(String, ghostex_gx_protocol::StartupDelivery)>,
    /// The rows of the messages that were not sent (`composer::delivery::unsent_rows`).
    pub unsent: Vec<crate::state::PendingSend>,
}

/// Rebuilds `state.messages.boundaried` when the transcript or the markers changed, and keeps it
/// otherwise: the `assembled` / `surfaced` / `boundaried` memos of `controller.ts`.
///
/// Rows only pushed at the list's end since the last refresh, under the same markers and agent,
/// extend it (`AssembledTail::extend`); `boundaried_extended` then says what it extended, for the
/// composition's head.
pub fn refresh_boundaried(state: &mut ChatState) {
    let key = BoundariedKey::of(state);
    if state.messages.boundaried_key.as_ref() == Some(&key) {
        return;
    }
    let messages = &mut state.messages;
    let previous = messages
        .boundaried_key
        .take()
        .filter(|previous| previous.markers == key.markers && previous.agent == key.agent);
    let added = match (
        previous.as_ref(),
        messages.pushed_since,
        &mut messages.assembled_tail,
    ) {
        (Some(_), Some(pushed), Some(tail)) => {
            tail.extend(&messages.list, key.list_revision, pushed)
        }
        _ => None,
    };
    match (previous, added) {
        (Some(previous), Some(added)) => {
            let from = messages.boundaried.len();
            messages.boundaried.extend(apply_marker_boundaries_in(
                surface_skill_invocation_user_turns_in(added),
                &state.pending.markers,
            ));
            messages.boundaried_extended = Some((previous, from));
        }
        _ => {
            let (assembled, tail) =
                crate::session::assembler::assembled_with_tail(&messages.list, key.list_revision);
            messages.assembled_tail = tail;
            messages.boundaried = apply_marker_boundaries_in(
                surface_skill_invocation_user_turns_in(assembled),
                &state.pending.markers,
            );
            messages.boundaried_extended = None;
        }
    }
    state.messages.boundaried_key = Some(key);
}

/// Rebuilds `state.messages.composed` when one of the memo's dependencies changed, and keeps it
/// otherwise. Bumps `compose_generation` on every rebuild, which is the new array the TypeScript
/// memo hands back.
pub fn compose_cached(state: &mut ChatState, preview_text: Option<&str>, working: bool) {
    refresh_boundaried(state);
    let key = ComposeKey {
        identity: state.messages.composition_identity,
        working,
        async_questions_since: state.session.async_questions_since,
        preview_text: preview_text.map(str::to_string),
        sends: state.pending.sends.clone(),
        markers: state.pending.markers.clone(),
        statuses: state.pending.terminal_status_messages.clone(),
        stream: state.pending.terminal_stream.clone(),
        visible_tool: visible_terminal_tool(state, working).cloned(),
        queue: (*state.session.queue_prompts).clone(),
        app_commands: state.session.app_commands.clone(),
        shell: state.pending.terminal_shell.clone(),
        held: crate::composer::delivery::echo_note(state),
        unsent: crate::composer::delivery::unsent_rows(state),
    };
    if state.messages.compose_key.as_ref() == Some(&key) {
        return;
    }
    let previous = std::mem::take(&mut state.messages.composed);
    let (composed, part) = compose_after(state, preview_text, working, previous);
    state.messages.composed = composed;
    state.messages.transcript_part = Some(part);
    state.messages.compose_key = Some(key);
    state.messages.compose_generation = state.messages.compose_generation.wrapping_add(1);
}

/// The transcript after assembly, skill surfacing and the `/clear` boundary.
///
/// This is `boundaried` in the TypeScript, and it is what every later step measures against.
/// [`refresh_boundaried`] keeps the answer on the state; this is the rebuild.
pub fn boundaried_transcript(state: &ChatState) -> Vec<ChatRow> {
    let surfaced = surface_skill_invocation_user_turns_in(assembled_messages(&state.messages.list));
    apply_marker_boundaries_in(surfaced, &state.pending.markers)
}

/// What the composed list's head, the transcript with the archived local commands reconciled into
/// it, is built from.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptPartKey {
    pub boundaried: BoundariedKey,
    pub app_commands: Vec<serde_json::Value>,
    pub pending: Vec<ChatRow>,
    pub async_questions_since: Option<i64>,
}

/// The composed list's head as the last composition built it: what from, how many rows, and a
/// revision that moves whenever the head was built again rather than kept.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptPart {
    pub key: TranscriptPartKey,
    pub len: usize,
    pub revision: u64,
}

/// The whole composed list the renderer sees.
///
/// Everything with no authoritative record yet rides in the tail, in the order `controller.ts`
/// builds it; the head is the transcript with the archived local commands reconciled into it.
pub fn compose(state: &ChatState, preview_text: Option<&str>, working: bool) -> Vec<ChatRow> {
    compose_after(state, preview_text, working, Vec::new()).0
}

/// [`compose`], given the list the last composition built: when the head is built from the same
/// transcript, app commands, echoes and retirement stamp, that list's head is kept rather than
/// built again, and only the tail is.
///
/// CDXC:SessionChat 2026-10-23 WHY: Every streamed delta recomposes the list (the stream is a
/// dependency), and building the head copied and reconciled the whole transcript each time, a few
/// milliseconds per event on a long chat for rows that had not changed.
fn compose_after(
    state: &ChatState,
    preview_text: Option<&str>,
    working: bool,
    previous: Vec<ChatRow>,
) -> (Vec<ChatRow>, TranscriptPart) {
    let rebuilt;
    let boundaried_current =
        state.messages.boundaried_key.as_ref() == Some(&BoundariedKey::of(state));
    let boundaried: &[ChatRow] = if boundaried_current {
        &state.messages.boundaried
    } else {
        rebuilt = boundaried_transcript(state);
        &rebuilt
    };
    let queue = (*state.session.queue_prompts).clone().unwrap_or_default();
    let mut startup_pending =
        pending_with_startup_sends(&state.pending.sends, &queue, state.session.agent.as_deref());
    // Messages this computer holds while the session's computer is offline.
    startup_pending.extend(crate::session::computer::held_as_pending(state));
    // Messages that were not sent, above the sends still on their way: they are older.
    startup_pending.splice(0..0, crate::composer::delivery::unsent_rows(state));
    // The list an echo is matched against copies the transcript; with nothing pending there is
    // nothing to match.
    let visible = if startup_pending.is_empty() {
        Vec::new()
    } else {
        visible_pending_sends(&startup_pending, &pending_transcript(state, boundaried))
    };
    let mut pending_messages = crate::transcript::local_command::with_live_shell_output(
        pending_sends_as_messages(&visible),
        state.pending.terminal_shell.as_ref(),
        boundaried,
    );
    // A message that has not gone yet says so under its bubble: held by the send gate with why
    // (CDXC:SessionChat 2026-11-11 on `hold_words` in composer/policy.rs), waiting to go again
    // after a failure time can fix, or still out (composer/delivery.rs). Only the drawn row
    // carries it, and it keeps its own `queued` mark: the echo is no queue row, so nothing that
    // settles queue rows reads it.
    if let Some((pending_id, delivery)) = crate::composer::delivery::echo_note(state) {
        let id = format!(
            "{}{pending_id}",
            crate::session::constants::PENDING_ID_PREFIX
        );
        if let Some(row) = pending_messages
            .iter_mut()
            .find(|row| row.id == id && row.startup_delivery.is_none())
        {
            std::sync::Arc::make_mut(row).startup_delivery = Some(delivery);
        }
    }
    let key = TranscriptPartKey {
        boundaried: BoundariedKey::of(state),
        app_commands: state.session.app_commands.clone(),
        pending: pending_messages.clone(),
        async_questions_since: state.session.async_questions_since,
    };
    let kept = state
        .messages
        .transcript_part
        .as_ref()
        .filter(|part| part.key == key && previous.len() >= part.len);
    // The head the last composition built, when `boundaried` has since only grown at its end
    // (`refresh_boundaried`) and nothing else the head is built from moved: the head is the same
    // rows with the new ones, reconciled, filtered and retired the same way, after them. A new row
    // that is an archived local command (or its output) can move what the reconciliation inserts
    // before it, so it builds the head again.
    let extended = kept.is_none().then_some(()).and_then(|()| {
        let part = state.messages.transcript_part.as_ref()?;
        let (extended, from) = state.messages.boundaried_extended.as_ref()?;
        let fits = boundaried_current
            && previous.len() >= part.len
            && part.key.boundaried == *extended
            && part.key.app_commands == key.app_commands
            && part.key.pending == key.pending
            && part.key.async_questions_since == key.async_questions_since
            && *from <= boundaried.len()
            && !boundaried[*from..]
                .iter()
                .any(|message| message.id.starts_with("local-command:"));
        fits.then_some((part, *from))
    });
    let (mut composed, part) = match (kept, extended) {
        (Some(part), _) => {
            let mut composed = previous;
            composed.truncate(part.len);
            (
                composed,
                TranscriptPart {
                    key,
                    len: part.len,
                    revision: part.revision,
                },
            )
        }
        (None, Some((part, from))) => {
            let mut composed = previous;
            composed.truncate(part.len);
            let mut added = crate::transcript::local_command::without_shell_echo_queue_rows(
                boundaried[from..].to_vec(),
                &pending_messages,
            );
            retire_async_questions(state, &mut added);
            composed.extend(added);
            let len = composed.len();
            // The rows before `part.len` are the ones the head held, so the revision stays: the
            // projection's settled prefix keeps its place in them (`settled_build::build_on`).
            (
                composed,
                TranscriptPart {
                    key,
                    len,
                    revision: part.revision,
                },
            )
        }
        (None, None) => {
            let mut transcript = crate::transcript::local_command::without_shell_echo_queue_rows(
                reconcile_local_command_output(boundaried, &state.session.app_commands),
                &pending_messages,
            );
            retire_async_questions(state, &mut transcript);
            let len = transcript.len();
            let revision = state
                .messages
                .transcript_part
                .as_ref()
                .map_or(0, |part| part.revision.wrapping_add(1));
            (transcript, TranscriptPart { key, len, revision })
        }
    };
    let transcript = &composed[..];

    // A marker whose text an authoritative user row already carries is the same fact twice. The
    // texts are normalized only when a user marker needs them: doing it for every user row on
    // every composition made each new message on a long chat pay for the whole history.
    let authoritative_text = std::cell::OnceCell::new();
    // Counting the compactions classifies every transcript row, which only a marker reads.
    let marker_messages: Vec<ChatRow> = if state.pending.markers.is_empty() {
        Vec::new()
    } else {
        markers_as_messages(
            &retire_interrupt_markers(&state.pending.markers, boundaried),
            compaction_records(&state.messages.list),
        )
    }
    .into_iter()
    .filter(|message| {
        !matches!(message.role, ChatRole::User)
            || !authoritative_text
                .get_or_init(|| {
                    boundaried
                        .iter()
                        .filter(|message| matches!(message.source, ChatSource::Transcript))
                        .map(|message| normalized_text(message))
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<String>>()
                })
                .contains(&normalized_text(message))
    })
    .collect();
    let marker_messages = retire_markers_covered_by_local_commands(
        &marker_messages,
        &state.session.app_commands,
        transcript,
        &state.pending.markers,
    );

    // Order, from `controller.ts`: the transient terminal statuses, then the app-command rows, then
    // the markers, then the streaming bubble, then the pending echoes.
    let mut tail: Vec<ChatRow> =
        unreconciled_terminal_statuses(&state.pending.terminal_status_messages, boundaried);
    tail.extend(surface_skill_invocation_user_turns(
        &app_commands_as_messages(&state.session.app_commands, transcript),
    ));
    tail.extend(marker_messages);

    let visible_tool = visible_terminal_tool(state, working);
    // The terminal's live message wins over the hook preview: it is the same bubble, read from the
    // screen the agent is painting instead of a status line.
    let terminal_stream_text = state.pending.terminal_stream.as_ref().filter(|stream| {
        (stream.live || working)
            && !visible_tool.is_some_and(|tool| terminal_stream_is_tool(stream, tool))
            && !terminal_stream_retired(stream, boundaried)
    });
    // `derive_streaming_text` reads only the last row of the transcript plus the pending echoes.
    let with_pending: Vec<ChatRow> = pending_messages
        .last()
        .or(boundaried.last())
        .cloned()
        .into_iter()
        .collect();
    // Titles at the end wait for the text under them while the message is still being painted;
    // a stream that is no longer the newest thing on screen (a tool call, the turn's end) shows all.
    let streaming_text = match terminal_stream_text {
        Some(stream) if stream.live => {
            Some(without_trailing_section_titles(&stream.text).to_string())
        }
        Some(stream) => Some(stream.text.clone()),
        None => derive_streaming_text(&with_pending, preview_text, working)
            .map(|text| without_trailing_section_titles(&text).to_string()),
    };
    if let Some(text) = streaming_text.filter(|text| !text.is_empty()) {
        tail.push(std::sync::Arc::new(streaming_message(&text)));
    }
    // CDXC:SessionChat 2026-10-18 DECISION: User (Q253, "card"): the pending tool card and the working row become one card (`extras::working_card`, since 2026-10-18 in its own row right above the composer). The pending tool no longer rides in the tail as a row of its own; the card's label shows it. Supersedes the 2026-10-17 open "Running now" section, gone with the card's opening ("let's make the pill itself not expandable at all").
    tail.extend(pending_messages);

    retire_async_questions(state, &mut tail);
    composed.extend(tail);
    (composed, part)
}

/// An async question older than the retirement stamp is no longer offered.
fn retire_async_questions(state: &ChatState, messages: &mut [ChatRow]) {
    if let Some(since) = state.session.async_questions_since {
        for message in messages {
            if message.async_questions.is_some()
                && message.timestamp.is_some_and(|stamp| stamp < since)
            {
                std::sync::Arc::make_mut(message).async_questions = None;
            }
        }
    }
}

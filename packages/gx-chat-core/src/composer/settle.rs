//! Family d's uniform settle hook: the boot read, the two catalog reads, and the send gate the
//! other families read.
//!
//! `computeSessionChatSkills` and `computeSessionChatFiles`
//! (`packages/shared/session-chat-controller/skills.ts` and `files.ts`) are `useEffect`
//! bookkeeping: the skills list is read once per agent and re-read when the agent changes, the
//! file list once per chat. The core has no render pass, so the same decision runs here, once per
//! event.

use serde_json::Value;

use crate::effect::Effect;
use crate::event::{ComposerBootRead, Event};
use crate::state::{ChatContext, ChatState};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// `composerChrome.refresh()`: one `Promise.all` over the stash list and the session note.
///
/// Each read has its own `.catch(() => null)` (`native-composer-chrome.ts:49`), so a refusal is
/// half an answer rather than a throw: the other half still lands, the badge and the dot keep what
/// they had, and nothing reaches the action's outer `catch`. Both halves had `request_id: 0` and
/// no settle at all until 2026-09-22, so the stash badge and the note dot never updated and a
/// refused stash-list read raised the composer's error bar where the TypeScript raises nothing.
fn settle_chrome_refresh(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    if !state.composer.chrome.awaits(request_id) {
        return None;
    }
    state.core.claim_refusal();
    let (prompts, note) = match outcome {
        RpcOutcome::Ok { result } => (
            result
                .get("prompts")
                .and_then(|prompts| serde_json::from_value(prompts.clone()).ok()),
            result
                .get("note")
                .and_then(Value::as_str)
                .map(str::to_string),
        ),
        RpcOutcome::Err { .. } => (None, None),
    };
    state
        .composer
        .chrome
        .settle_refresh(request_id, prompts, note);
    Some(Vec::new())
}

/// `toggleNote`'s read: the body fills the sheet, and the `finally` stops it spinning either way.
fn settle_note_read(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    if state.composer.note.read_request != Some(request_id) {
        return None;
    }
    state.composer.note.read_request = None;
    if let RpcOutcome::Ok { result } = outcome {
        let note = result.get("note").and_then(Value::as_str).unwrap_or("");
        state.composer.note.saved = note.trim().to_string();
        if !state.composer.note.edited {
            state.composer.note.value = note.to_string();
        }
    }
    state.composer.note.loading = false;
    Some(Vec::new())
}

/// `pushDraft`'s own continuation, shared by `saveDraft` and the send chain's push phase.
///
/// `if (result?.draft …) setSyncedDraft((current) => mergeSessionChatDraftState(current,
/// result.draft))`. Without it a blur save left `draft.synced` holding whatever the last frame
/// carried, so the composer's own revision and the delivery receipts were one write behind.
fn settle_draft_push(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let at = state
        .composer
        .draft_pushes
        .iter()
        .position(|pending| *pending == request_id)?;
    state.composer.draft_pushes.remove(at);
    if let RpcOutcome::Ok { result } = outcome {
        if let Some(draft) = result.get("draft") {
            state.session.synced_draft = Some(crate::session::fold::merge_draft_state(
                state.session.synced_draft.as_ref(),
                draft,
            ));
            state.session.synced_draft_revision += 1;
        }
    }
    Some(Vec::new())
}

/// `attachPaths`'s `try`/`finally`: the imported references go to the composer, and the read is
/// counted back down whether it succeeded or refused.
///
/// The refusal itself lands on `operationError` through the dispatcher, which is what the
/// TypeScript's `catch` around the whole arm does.
fn settle_attachment_import(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let at = state
        .composer
        .attachment_imports
        .iter()
        .position(|pending| *pending == request_id)?;
    state.composer.attachment_imports.remove(at);
    state.composer.pending_attachments = state.composer.pending_attachments.saturating_sub(1);
    let RpcOutcome::Ok { result } = outcome else {
        return Some(Vec::new());
    };
    Some(vec![Effect::HostAction {
        action: "attachmentReferences".to_string(),
        params: Box::new(serde_json::json!({ "paths": result.clone() })),
    }])
}

/// `composer('claimReturned')`: the prompt reaches the composer only the first time its id is seen.
///
/// The applied-id list is one record for the whole app, so the claim is a read, a membership test
/// and a write-back of the bounded list.
fn settle_returned_claim(
    state: &mut ChatState,
    key: &crate::event::StorageKey,
    value: Option<&str>,
) -> Vec<Effect> {
    if key.store != crate::composer::storage::RETURNED_PROMPTS_STORE {
        return Vec::new();
    }
    let Some((id, text)) = state.composer.claiming_returned.take() else {
        return Vec::new();
    };
    let applied = crate::composer::storage::decode_applied_returned_ids(value);
    if applied.contains(&id) {
        return Vec::new();
    }
    vec![
        Effect::WriteStorage {
            key: crate::composer::storage::returned_prompts_key(),
            value: Some(crate::composer::storage::encode_applied_returned_ids(
                &applied, &id,
            )),
            durable: true,
        },
        Effect::RestoreReturnedPrompt { text },
    ]
}

/// The controller's own `useEffect(() => { if (chat.returnedPrompt) void action({ type:
/// 'restoreReturned', returned: chat.returnedPrompt }) }, [chat.returnedPrompt?.id])`.
///
/// The brain dispatches the action to itself, so no renderer ever sends it; without this the core
/// never claimed a returned prompt, and the live brain's `claimReturned` round trip answered
/// nothing the core had asked for.
pub fn restore_returned_effect(
    state: &mut ChatState,
    _event: &Event,
    context: &ChatContext,
) -> Vec<Effect> {
    if !state.core.controller_started {
        return Vec::new();
    }
    let returned = state.session.returned_prompt.clone();
    let id = returned
        .as_ref()
        .and_then(|prompt| prompt.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    if id == state.composer.returned_effect_id {
        return Vec::new();
    }
    state.composer.returned_effect_id = id;
    let Some(returned) = returned.filter(|prompt| !prompt.is_null()) else {
        return Vec::new();
    };
    let mut action = crate::action::UserAction::new(crate::action::ActionKind::RestoreReturned);
    action.params.insert("returned".to_string(), returned);
    crate::dispatch::actions::dispatch(state, &action, context)
}

/// `composerHistory = { entries: await composer('history'), index: null }`, then the Up the arm was
/// suspended on.
///
/// The read only ever happens on an Up with no entry showing, so the recall that resumes is always
/// backwards; a Down never reaches here because `recallNextSessionChatDraft` answers `null` with a
/// null index.
fn settle_history_read(
    state: &mut ChatState,
    key: &crate::event::StorageKey,
    value: Option<&str>,
) -> Vec<Effect> {
    if key.store != crate::composer::storage::COMPOSER_HISTORY_STORE
        || !state.composer.history.loading
    {
        return Vec::new();
    }
    state
        .composer
        .history
        .adopt(crate::composer::storage::decode_composer_history(value));
    crate::composer::actions::apply_recall(state, true)
}

/// Settles family d's carried state for this event.
pub fn settle(state: &mut ChatState, event: &Event, context: &ChatContext) -> Vec<Effect> {
    let mut effects = Vec::new();
    match event {
        Event::ComposerBootRead(read) => adopt_boot_read(state, read),
        Event::RpcSettled {
            request_id,
            outcome,
        } => {
            // The send path walks its phases one answer at a time; each entry point matches on the
            // id or the key it recorded and answers `None` for anything that is not its own.
            let claimed =
                crate::composer::send::settle_request(state, context, *request_id, outcome)
                    .or_else(|| {
                        crate::composer::send::settle_key_send(state, context, *request_id, outcome)
                    })
                    .or_else(|| {
                        crate::composer::send::settle_handoff_acknowledgement(state, *request_id)
                    })
                    .or_else(|| {
                        crate::composer::send::settle_queue_mutation(state, *request_id, outcome)
                    });
            let claimed = claimed
                .or_else(|| settle_chrome_refresh(state, *request_id, outcome))
                .or_else(|| settle_note_read(state, *request_id, outcome))
                .or_else(|| settle_attachment_import(state, *request_id, outcome))
                .or_else(|| settle_draft_push(state, *request_id, outcome));
            match claimed {
                Some(round) => effects.extend(round),
                None => settle_catalog(state, *request_id, outcome.as_ref()),
            }
            effects.extend(crate::composer::queue_edit::advance(
                state,
                *request_id,
                outcome,
            ));
        }
        Event::StorageLoaded { key, value } => {
            effects.extend(settle_returned_claim(state, key, value.as_deref()));
            effects.extend(settle_history_read(state, key, value.as_deref()));
        }
        Event::StorageWritten { key, error } => {
            if let Some(round) =
                crate::composer::send::settle_storage(state, context, key, error.as_deref())
            {
                effects.extend(round);
            }
        }
        _ => {}
    }
    effects.extend(request_catalogs(state));
    effects.extend(crate::composer::draft_sync::settle(state));
    // The two latches `NativeComposerChrome.projection` sets on the way to its answer. They live
    // here because `document()` holds `&ChatState` and could only set them on a clone.
    let note_open = state.composer.note.open;
    let agent_session_id = state.session.agent_session_id.clone();
    state
        .composer
        .chrome
        .adopt(note_open, agent_session_id.as_deref());
    // `rewindEnabled` is `sendBlockedReason(state) === null`, which is family d's rule read by
    // family b. `document::assemble` runs b before d, so the answer is cached here rather than
    // read out of a half-built document.
    state.composer.send_blocked_reason = crate::composer::document::send_blocked(state, context);
    effects
}

/// The half of `composer('read')` that is family d's: the client id's draft entry and the two
/// transcript modes it hands to family b.
fn adopt_boot_read(state: &mut ChatState, read: &ComposerBootRead) {
    state.composer.boot_read = true;
    state.composer.stored_draft = serde_json::from_value(read.entry.clone()).ok();
    state.composer.version = read
        .entry
        .get("version")
        .and_then(|version| serde_json::from_value(version.clone()).ok());
    if state.composer.text.is_empty() {
        // A parked or submitted entry opens the composer empty, which is what the host's
        // `composerInit` arm does with the same record.
        let parked = read.entry.get("parked") == Some(&Value::Bool(true));
        let submitted = read.entry.get("submitted") == Some(&Value::Bool(true));
        if !parked && !submitted {
            state.composer.text = read
                .entry
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
        }
    }
    // The stored value is family d's to read and family b's to own
    // (`docs/2026-09-21/rust-chat/FAMILIES.md`, "Two assignments the seam map left ambiguous").
    state.transcript_view.summary_mode = read.summary_mode;
    state.transcript_view.verbose_override = read.verbose_override.as_bool();
}

/// `requestSkills()`, which is `load()` in `packages/shared/session-chat-controller/skills.ts:36`.
///
/// CDXC:AgentSkills 2026-09-22 WHY:
/// The guard is `loading || loaded`, and `loaded` is set only after a read SUCCEEDS
/// (`skills.ts:42`, the CDXC at `skills.ts:13`), which is what makes a failed read retryable from
/// the picker and a successful one not. `loaded` is `skills.is_some()` here, because a refusal
/// publishes `{loading: false, error}` and leaves the list unset. Starting a read clears both the
/// list and the error, the way `publish({loading: true})` replaces the whole record.
pub fn load_skills(state: &mut ChatState) -> Vec<Effect> {
    let sources = &mut state.composer.sources;
    // `loading` is the closure's own latch, NOT the published `skillsLoading`: the published one
    // starts true (`skills.ts:62`) and a guard on it would refuse the very first read.
    if sources.skills_request.is_some() || sources.skills.is_some() {
        return Vec::new();
    }
    sources.skills = None;
    sources.skills_error = None;
    sources.skills_loading = true;
    let request_id = state.core.allocate_request_id();
    state.composer.sources.skills_request = Some(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadSessionChatSkills,
        params: Box::new(Value::Object(Default::default())),
    }]
}

/// `computeSessionChatSkills` and `computeSessionChatFiles`: one read each, the first re-asked
/// when the agent the session runs under changes, plus the two edges
/// `NativeComposerSuggestions.projection` measures on every publish (`native-suggestions.ts:107`,
/// `:109`).
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// `skill_active` and `file_active` had NO WRITER, so neither edge could ever fire: the `@` list
/// never asked for the project files and opened on an empty catalog forever, and a `$` list whose
/// read had failed could not be reopened into a retry. They are the popup's own memory of the last
/// frame, so they are measured here, in the settle that stands in for `publish`.
fn request_catalogs(state: &mut ChatState) -> Vec<Effect> {
    let mut effects = Vec::new();
    if !state.composer.boot_read {
        return effects;
    }
    let agent = state.session.session_agent_id.clone();
    let sources = &mut state.composer.sources;
    if sources.skills_agent != agent || !sources.skills_asked {
        // The `useEffect` re-ran, so its `loaded` and `loading` latches are fresh closures again
        // and the read in flight is `active = false`: its answer publishes nothing.
        sources.skills_asked = true;
        sources.skills_agent.clone_from(&agent);
        sources.skills = None;
        sources.skills_request = None;
        // `current` is undefined for the new (transport, agentId), so the published value falls
        // back to `Boolean(transport.readSkills)` until `load()` publishes its own `{loading:true}`.
        sources.skills_loading = true;
        effects.extend(load_skills(state));
    }
    let (skill_active, file_active) = crate::composer::suggestions::picker_edges(
        &state.composer.suggestions.text,
        state.composer.suggestions.caret,
        state.session.agent.as_deref(),
        state.composer.suggestions.dismissed,
    );
    // `if (matches.skillPickerActive && !this.skillActive) sources.requestSkills();`, then the
    // latch, in that order.
    if skill_active && !state.composer.suggestions.skill_active {
        effects.extend(load_skills(state));
    }
    state.composer.suggestions.skill_active = skill_active;
    state.composer.suggestions.file_active = file_active;
    // `computeSessionChatFiles` reads nothing on mount: it hands back a `requestFiles` callback
    // the `@` list calls the first time it opens, and `filesLoading` is false until then. The
    // callback's own `requested` latch is `files_asked`; the caller's guard is `files === undefined`.
    if file_active && state.composer.sources.files.is_none() && !state.composer.sources.files_asked
    {
        state.composer.sources.files_asked = true;
        state.composer.sources.files_loading = true;
        let request_id = state.core.allocate_request_id();
        state.composer.sources.files_request = Some(request_id);
        effects.push(Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::ReadSessionChatFiles,
            params: Box::new(Value::Object(Default::default())),
        });
    }
    effects
}

/// The two catalog answers, which are the only reads family d issues on its own.
fn settle_catalog(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) {
    let sources = &mut state.composer.sources;
    if sources.skills_request == Some(request_id) {
        sources.skills_request = None;
        sources.skills_loading = false;
        match outcome {
            RpcOutcome::Ok { result } => {
                sources.skills = Some(
                    result
                        .get("skills")
                        .and_then(|skills| serde_json::from_value(skills.clone()).ok())
                        .unwrap_or_default(),
                );
                sources.skills_error = None;
            }
            RpcOutcome::Err { .. } => {
                // `publish({ loading: false, error: 'Could not load skills.' })`: one sentence the
                // picker can show, never the daemon's own message (`skills.ts:45`).
                sources.skills = None;
                sources.skills_error = Some(SKILLS_ERROR.to_string());
            }
        }
        return;
    }
    if sources.files_request == Some(request_id) {
        sources.files_request = None;
        sources.files_loading = false;
        // A refused listing answers an EMPTY list, not "not read yet" (`files.ts:27`): the `@`
        // popup then draws "Listing project files…" without a spinner rather than staying open on
        // a list that will never arrive.
        sources.files = Some(match outcome {
            RpcOutcome::Ok { result } => result
                .get("files")
                .and_then(|files| serde_json::from_value(files.clone()).ok())
                .unwrap_or_default(),
            RpcOutcome::Err { .. } => Vec::new(),
        });
    }
}

/// The one sentence a failed skills read shows, whatever the daemon said
/// (`packages/shared/session-chat-controller/skills.ts:45`).
const SKILLS_ERROR: &str = "Could not load skills.";

//! Family b's user actions: the row details, the transcript modes, the rewind sheet, saved
//! prompts, deferred work and the links and images a row opens.
//!
//! Ported from the family b arms of `action` in
//! `packages/shared/session-chat-controller/native-host.ts`, plus `native-message-actions.ts`.

use serde_json::{json, Value};

use crate::action::{ActionKind, UserAction};
use crate::effect::{Effect, OpenTarget};
use crate::state::{ChatContext, ChatState, OpenRow, RewindRequest};
use crate::transcript::deferred_work::{
    DeferredWalk, WalkStep, DEFERRED_WORK_PAGE_LIMIT, WORK_HISTORY_UNREADABLE,
};
use crate::transcript::file_position::FilePosition;
use crate::transcript::jsstr::js_trim;
use crate::transcript::links::{classify_link_href, file_position_from_href, LinkTarget};
use crate::transcript::rows;
use crate::transcript::view_level;
use crate::wire::{ChatRpcMethod, RpcOutcome};

fn param<'a>(action: &'a UserAction, key: &str) -> Option<&'a Value> {
    action.params.get(key)
}

fn text(action: &UserAction, key: &str) -> String {
    param(action, key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The row the document draws for a section that is still being read, or that refused.
fn work_row(error: Option<String>) -> crate::document::DeferredWorkRow {
    crate::document::DeferredWorkRow {
        loading: error.is_none(),
        error: match error {
            Some(message) => ghostex_gx_protocol::Tri::Value(message),
            None => ghostex_gx_protocol::Tri::Absent,
        },
    }
}

/// One page of a `loadWork` walk.
fn read_work_page(state: &mut ChatState, walk: DeferredWalk) -> Vec<Effect> {
    let request_id = state.core.allocate_request_id();
    let cursor = walk.cursor;
    state
        .transcript_view
        .deferred_requests
        .insert(request_id, walk);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadSessionChat,
        // `readHistory({beforeOffset, limit: 200, detail: true})`, which `native-host.ts` spread
        // into the call and spelled `historyMode: 'detail'`.
        params: Box::new(json!({
            "beforeOffset": cursor,
            "limit": DEFERRED_WORK_PAGE_LIMIT,
            "detail": true,
            "historyMode": "detail",
        })),
    }]
}

/// `case 'loadWork'`: a completed turn's collapsed work is read back out of history.
///
/// The failure belongs to the row that asked for it, not to the composer's error bar, so it never
/// reaches the outer handler. A section the cache already holds resolves on this turn, with no
/// round trip at all, which is what the TypeScript's already-settled promise did.
fn load_work(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let turn_id = text(action, "id");
    state
        .transcript_view
        .deferred_work
        .insert(turn_id.clone(), work_row(None));
    state.transcript_view.detail_revision += 1;
    // `publish(controller.current())` before the await: the row spins on this turn.
    state.core.request_publish();
    let Some(walk) = DeferredWalk::begin(turn_id.clone(), param(action, "work")) else {
        state
            .transcript_view
            .deferred_work
            .insert(turn_id, work_row(Some(WORK_HISTORY_UNREADABLE.to_string())));
        state.transcript_view.detail_revision += 1;
        return Vec::new();
    };
    if let Some(messages) = state.transcript_view.deferred_cache.take_fresh(&walk.key) {
        state
            .transcript_view
            .deferred
            .insert(turn_id.clone(), messages);
        state.transcript_view.deferred_work.remove(&turn_id);
        state.transcript_view.detail_revision += 1;
        return Vec::new();
    }
    read_work_page(state, walk)
}

/// The open rows the renderer reports, in the order it reports them.
fn open_rows(action: &UserAction) -> Vec<OpenRow> {
    let Some(Value::Array(rows)) = param(action, "open") else {
        return Vec::new();
    };
    rows.iter()
        .map(|row| OpenRow {
            key: row
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            kind: row
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            message_id: row
                .get("messageId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            index: row.get("index").and_then(Value::as_u64).unwrap_or_default() as usize,
        })
        .collect()
}

fn position_effect(href: &str, path: String) -> Effect {
    let position: Option<FilePosition> = file_position_from_href(href);
    Effect::Open(OpenTarget::File {
        path,
        line: position.map(|position| position.line as u32),
        column: position
            .and_then(|position| position.column)
            .map(|column| column as u32),
    })
}

/// Handles one action family b owns.
/// Makes `level` the chat's one view level (transcript/view_level.rs has the user's words). Summary
/// writes only its own record, so the chat's Normal or Detailed record stays as saved under it;
/// Normal and Detailed pin both level records (`simple`, and `verbose` off, Verbose being retired)
/// and turn Summary off.
fn set_view_level(state: &mut ChatState, level: &str) -> Vec<Effect> {
    let session_key = state.identity.session_key.clone();
    let summary = |state: &mut ChatState, on: bool| {
        state.transcript_view.pending_summary_mode = Some(on);
        Effect::WriteStorage {
            key: crate::composer::storage::summary_key(&session_key),
            value: Some(crate::composer::storage::encode_summary(on).to_string()),
            durable: false,
        }
    };
    if level == view_level::SUMMARY {
        return if state.transcript_view.summary_mode {
            Vec::new()
        } else {
            vec![summary(state, true)]
        };
    }
    let simple = level == view_level::NORMAL;
    state.transcript_view.pending_simple_override = Some(Some(simple));
    state.transcript_view.pending_verbose_override = Some(Some(false));
    let mut effects = vec![
        Effect::WriteStorage {
            key: crate::composer::storage::simple_key(&session_key),
            value: Some(crate::composer::storage::encode_verbose(simple).to_string()),
            durable: false,
        },
        Effect::WriteStorage {
            key: crate::composer::storage::verbose_key(&session_key),
            value: Some(crate::composer::storage::encode_verbose(false).to_string()),
            durable: false,
        },
    ];
    if state.transcript_view.summary_mode {
        effects.push(summary(state, false));
    }
    effects
}

pub fn handle(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    match action.kind {
        ActionKind::RowDetails => {
            state.transcript_view.open_rows = open_rows(action);
            state.transcript_view.row_details = rows::row_details(state, context);
            Vec::new()
        }
        // Each write lands before its mode changes (`crate::transcript::settle` on
        // `Event::StorageWritten`), so a refused write leaves the transcript as it was.
        // Turning Summary off goes to Normal (transcript/view_level.rs has the user's words).
        ActionKind::ToggleSummary => {
            let next = if state.transcript_view.summary_mode {
                view_level::NORMAL
            } else {
                view_level::SUMMARY
            };
            set_view_level(state, next)
        }
        // The retired Verbose switch, from an older renderer: on is Detailed, off Normal.
        ActionKind::SetVerbose => {
            let enabled = param(action, "enabled").and_then(Value::as_bool) == Some(true);
            set_view_level(
                state,
                if enabled {
                    view_level::DETAILED
                } else {
                    view_level::NORMAL
                },
            )
        }
        ActionKind::SetDetailLevel => {
            let level = view_level::parse(&text(action, "level"));
            set_view_level(state, level)
        }
        ActionKind::LoadWork => load_work(state, action),
        ActionKind::RewindOpen => {
            state.transcript_view.rewind = Some(RewindRequest {
                message_id: text(action, "messageId"),
                prompt: text(action, "prompt"),
                agent: state.session.agent.clone().unwrap_or_default(),
                ..RewindRequest::default()
            });
            Vec::new()
        }
        ActionKind::RewindCancel => {
            let busy = state
                .transcript_view
                .rewind
                .as_ref()
                .is_some_and(|request| request.busy);
            if !busy {
                state.transcript_view.rewind = None;
            }
            Vec::new()
        }
        ActionKind::RewindSubmit => {
            let Some(request) = state.transcript_view.rewind.as_mut() else {
                return Vec::new();
            };
            if request.busy || request.completed {
                return Vec::new();
            }
            request.busy = true;
            request.error = None;
            let message_id = request.message_id.clone();
            let request_id = state.core.allocate_request_id();
            if let Some(request) = state.transcript_view.rewind.as_mut() {
                request.request_id = Some(request_id);
            }
            vec![Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::RewindSessionChat,
                params: Box::new(json!({ "messageId": message_id })),
            }]
        }
        ActionKind::SavePrompt => {
            let message_id = text(action, "messageId");
            let prompt = text(action, "prompt");
            let saving = state
                .transcript_view
                .saved_prompts
                .get(&message_id)
                .map(String::as_str)
                == Some("saving");
            if saving || js_trim(&prompt).is_empty() {
                return Vec::new();
            }
            state
                .transcript_view
                .saved_prompts
                .insert(message_id.clone(), "saving".to_string());
            let request_id = state.core.allocate_request_id();
            state
                .transcript_view
                .save_prompt_requests
                .insert(request_id, message_id);
            vec![Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::SaveStashedPrompt,
                params: Box::new(json!({ "content": prompt })),
            }]
        }
        ActionKind::OpenMarkdownLink => {
            let href = text(action, "href");
            if let Some(tag) = crate::questions::open_questions::link_tag(&href) {
                // On its own question it never scrolls (`jump_to_tag`); the clients open the Reply
                // popover for it.
                if href.ends_with(crate::transcript::open_questions::HERE_SUFFIX) {
                    return Vec::new();
                }
                return crate::questions::open_questions::jump_to_tag(state, context, tag);
            }
            if let Some(target) = ghostex_gx_protocol::parse_global_session_ref(href.trim()) {
                return open_mentioned_session(state, target);
            }
            if let Some(item) = crate::extras::coordinator_board::board_item(&href) {
                return crate::extras::coordinator_board::show_item(state, item);
            }
            match classify_link_href(&href) {
                LinkTarget::File(path) => vec![position_effect(&href, path)],
                // CDXC:SessionChat 2026-10-26 WHY: Shift+click on a chat link opens it in the system browser (the 2026-08-03 SessionChat comment in the desktop's session_chat/host_actions.rs). The TypeScript brain passed the click's `external` on to the host; the Rust port dropped it, so Shift+click opened the app's Browser like a plain click.
                LinkTarget::Url(url) => vec![Effect::Open(OpenTarget::Url {
                    url,
                    external: param(action, "external").and_then(Value::as_bool) == Some(true),
                })],
                LinkTarget::Inert => Vec::new(),
            }
        }
        // A file change card's Review all: the app shows the changes where it reviews Git work.
        ActionKind::ReviewFileChanges => vec![Effect::HostAction {
            action: "reviewChanges".to_string(),
            params: Box::new(json!({
                "paths": param(action, "paths").cloned().unwrap_or_else(|| json!([])),
            })),
        }],
        // A session name's pill on an agent message card: the other session, focused like a fork branch.
        ActionKind::OpenAgentSession => {
            let (project_id, session_id) = (text(action, "projectId"), text(action, "sessionId"));
            if project_id.is_empty() || session_id.is_empty() {
                return Vec::new();
            }
            vec![Effect::HostAction {
                action: "openAgentSession".to_string(),
                params: Box::new(json!({ "projectId": project_id, "sessionId": session_id })),
            }]
        }
        // A command in a reply goes to a terminal of the Commands panel, which only the host can
        // reach (`run_command.rs`): Run types it and presses Enter, Paste only types it.
        ActionKind::RunInTerminal | ActionKind::PasteInTerminal => {
            let command = text(action, "command");
            // Paste only types the command and leaves Enter to the user, so it carries one line.
            if command.trim().is_empty()
                || (action.kind == ActionKind::PasteInTerminal
                    && command.chars().any(char::is_control))
            {
                return Vec::new();
            }
            let name = if action.kind == ActionKind::RunInTerminal {
                "runInTerminal"
            } else {
                "pasteInTerminal"
            };
            vec![Effect::HostAction {
                action: name.to_string(),
                params: Box::new(json!({ "command": command })),
            }]
        }
        ActionKind::LoadPageThumbnail => crate::transcript::page_thumbnails::load(state, action),
        ActionKind::LoadImage => {
            /*
            The picture behind an "[Image #N](path)" reference lives on the session's machine, so the
            native chat cannot open it directly: the bytes come back over the chat's transport, and
            a preview backend answers the same call. A file that has gone reports back as unreadable
            rather than raising the composer's error bar.
            */
            let request_id = state.core.allocate_request_id();
            let path = text(action, "path");
            state
                .transcript_view
                .image_requests
                .insert(request_id, path.clone());
            vec![Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::ReadSessionChatImage,
                params: Box::new(json!({ "path": path })),
            }]
        }
        _ => Vec::new(),
    }
}

/// The answers to the four calls family b's row actions make.
///
/// `NativeChatMessageActions.submit` has three outcomes (`native-message-actions.ts`): a rewind
/// that needs synchronization keeps the sheet up with its warning, a plain success puts the prompt
/// back in the composer, and a `messageNotFound` refusal closes the sheet and restores the prompt
/// rather than showing an error. Save prompt has two, and `loadWork` reports its failure on the
/// row that asked rather than on the composer's error bar.
pub fn settle_rpc(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) -> Vec<Effect> {
    let (result, failure) = match outcome {
        RpcOutcome::Ok { result } => (Some(result.clone()), None),
        RpcOutcome::Err { code, message, .. } => (None, Some((code.clone(), message.clone()))),
    };
    if let Some(message_id) = state
        .transcript_view
        .save_prompt_requests
        .remove(&request_id)
    {
        let status = if failure.is_none() { "saved" } else { "error" };
        state
            .transcript_view
            .saved_prompts
            .insert(message_id, status.to_string());
        // `NativeMessageActions.save` has its own `catch` (`native-message-actions.ts:129`), so
        // the row turns red and the action's outer `catch` never sees the refusal.
        state.core.claim_refusal();
        return Vec::new();
    }
    if let Some(mut walk) = state.transcript_view.deferred_requests.remove(&request_id) {
        // `readWork`'s own `catch` writes the row's text; nothing reaches the composer's bar.
        state.core.claim_refusal();
        let turn_id = walk.turn_id.clone();
        let step = match result {
            Some(page) => walk.advance(&page),
            None => {
                // The rpc itself rejected, which the arm's own `catch` turns into the row's text.
                let message = failure
                    .clone()
                    .map(|(_, message)| message)
                    .filter(|message| !message.is_empty())
                    .unwrap_or_else(|| WORK_HISTORY_UNREADABLE.to_string());
                WalkStep::Failed { message }
            }
        };
        match step {
            // The walk continues on the page before this one; the row keeps spinning and nothing
            // publishes, because the arm has not returned yet.
            WalkStep::ReadPage { .. } => return read_work_page(state, walk),
            WalkStep::Done { messages } => {
                state
                    .transcript_view
                    .deferred_cache
                    .store(&walk.key, &messages);
                state
                    .transcript_view
                    .deferred
                    .insert(turn_id.clone(), messages);
                state.transcript_view.deferred_work.remove(&turn_id);
            }
            WalkStep::Failed { message } => {
                state
                    .transcript_view
                    .deferred_work
                    .insert(turn_id, work_row(Some(message)));
            }
        }
        state.transcript_view.detail_revision += 1;
        return Vec::new();
    }
    if let Some(path) = state.transcript_view.image_requests.remove(&request_id) {
        // `loadImage`'s arm catches for itself and pushes `chatImage/failed`
        // (`native-host.ts:1509`), so a picture that has gone does not raise the error bar.
        state.core.claim_refusal();
        if let Some(bytes) = result
            .as_ref()
            .and_then(|image| image.get("base64Data"))
            .and_then(Value::as_str)
        {
            crate::transcript::read_images::note_picture_bytes(state, &path, bytes);
        }
        return vec![Effect::HostAction {
            action: "chatImage".to_string(),
            params: Box::new(match &result {
                Some(image) => json!({ "path": path, "image": image }),
                None => {
                    let (code, message) = failure.unwrap_or_default();
                    json!({
                        "path": path,
                        "reason": crate::transcript::images::image_read_failure_reason(code.as_deref().unwrap_or_default(), &message),
                        "error": message,
                    })
                }
            }),
        }];
    }
    let Some(request) = state.transcript_view.rewind.as_mut() else {
        return Vec::new();
    };
    if request.request_id != Some(request_id) {
        return Vec::new();
    }
    request.request_id = None;
    request.busy = false;
    // `NativeMessageActions.submit` catches for itself: the sentence goes on the rewind dialog
    // (`native-message-actions.ts:105`), not on the composer's error bar.
    state.core.claim_refusal();
    let prompt = request.prompt.clone();
    match result {
        Some(result) => {
            let warning = result
                .get("warning")
                .and_then(Value::as_str)
                .filter(|warning| !warning.is_empty())
                .map(str::to_string);
            // CDXC:SessionChat 2026-09-11 DECISION:
            // User approved Retry synchronization after Codex confirms rewind; the dialog and the
            // draft are preserved while the server reconnects to that branch.
            if result.get("synchronizationPending") == Some(&Value::Bool(true)) {
                request.synchronization_pending = true;
                request.error = Some(
                    warning.unwrap_or_else(|| "The rewind needs synchronization.".to_string()),
                );
                return Vec::new();
            }
            match warning {
                Some(warning) => {
                    request.error = Some(warning);
                    request.completed = true;
                }
                None => state.transcript_view.rewind = None,
            }
            vec![restore_prompt(prompt)]
        }
        None => {
            let (code, message) = failure.unwrap_or_default();
            // CDXC:SessionChat 2026-09-16 DECISION:
            // User: if Rewind finds that the message was never accepted, put its text back in the
            // composer instead of showing an error.
            if code.as_deref() == Some("messageNotFound") {
                state.transcript_view.rewind = None;
                return vec![restore_prompt(prompt)];
            }
            request.error = Some(if message.is_empty() {
                "The conversation could not be rewound.".to_string()
            } else {
                message
            });
            Vec::new()
        }
    }
}

/// Hands the rewound prompt back to the composer, so the reader edits it instead of retyping it.
fn restore_prompt(prompt: String) -> Effect {
    let caret = prompt.encode_utf16().count();
    Effect::SetComposerText {
        content: prompt,
        caret: Some(caret),
        from_history: false,
    }
}

/// A click on an agent pill (`session_mentions.rs`): the session it names, switched to as its
/// sidebar row would be. A session the host's loaded list no longer has does nothing; before the
/// list has loaded the click is passed on with the ref's own ids.
///
/// A session on another computer than the chat's (a `+` pick from a connected computer) carries
/// its gxserver as `serverId`, which the hosts open it on, as a coordinator's worker elsewhere is
/// (`openCoordinatorThread`); without it the host would look for those ids on the chat's computer.
fn open_mentioned_session(
    state: &ChatState,
    target: ghostex_gx_protocol::GlobalSessionRef<'_>,
) -> Vec<Effect> {
    let sessions = &state.transcript_view.session_directory;
    let found = sessions.iter().find(|session| {
        session.project_id == target.project_id && session.session_id == target.session_id
    });
    let (project_id, session_id) = match found {
        Some(session) => (session.project_id.clone(), session.session_id.clone()),
        None if sessions.is_empty() => {
            (target.project_id.to_string(), target.session_id.to_string())
        }
        None => return Vec::new(),
    };
    let own_server = crate::transcript::session_mentions::chat_server(
        sessions,
        &state.identity.project_id,
        &state.identity.session_id,
    );
    let mut params = json!({ "projectId": project_id, "sessionId": session_id });
    if !own_server.is_empty() && own_server != target.server_id {
        params["serverId"] = target.server_id.into();
    }
    vec![Effect::HostAction {
        action: "openAgentSession".to_string(),
        params: Box::new(params),
    }]
}

#[cfg(test)]
mod tests {
    use super::handle;
    use crate::action::{ActionKind, UserAction};
    use crate::state::{ChatContext, ChatState};
    use serde_json::json;

    fn act(kind: ActionKind, level: Option<&str>) -> UserAction {
        let mut action = UserAction::new(kind);
        if let Some(level) = level {
            action.params.insert("level".to_string(), json!(level));
        }
        action
    }

    /// A pill for a session on another computer opens it there (`serverId`); one on the chat's
    /// own computer opens as before.
    #[test]
    fn a_mention_of_another_computers_session_names_its_server() {
        use crate::event::KnownSession;
        let session = |global_ref: &str| {
            let parts: Vec<&str> = global_ref.split(':').collect();
            KnownSession {
                global_ref: global_ref.to_string(),
                project_id: parts[1].to_string(),
                session_id: parts[2].to_string(),
                ..KnownSession::default()
            }
        };
        let mut state = ChatState::default();
        state.identity.project_id = "P3lv0".to_string();
        state.identity.session_id = "Gme".to_string();
        state.transcript_view.session_directory =
            vec![session("S90:P3lv0:G1"), session("S8g:P5zq6:G4snt")];
        let open = |state: &mut ChatState, href: &str| {
            let mut action = UserAction::new(ActionKind::OpenMarkdownLink);
            action.params.insert("href".to_string(), json!(href));
            match handle(state, &action, &ChatContext::default()).pop() {
                Some(crate::effect::Effect::HostAction { action, params }) => {
                    assert_eq!(action, "openAgentSession");
                    *params
                }
                other => panic!("no host action: {other:?}"),
            }
        };
        assert_eq!(
            open(&mut state, "S8g:P5zq6:G4snt"),
            json!({ "projectId": "P5zq6", "sessionId": "G4snt", "serverId": "S8g" })
        );
        assert_eq!(
            open(&mut state, "S90:P3lv0:G1"),
            json!({ "projectId": "P3lv0", "sessionId": "G1" })
        );
    }

    /// Picking Normal or Detailed turns Summary off; picking Summary leaves the level records alone.
    #[test]
    fn view_levels_are_one_choice() {
        let context = ChatContext::default();
        let mut state = ChatState::default();
        state.transcript_view.summary_mode = true;
        handle(
            &mut state,
            &act(ActionKind::SetDetailLevel, Some("detailed")),
            &context,
        );
        assert_eq!(state.transcript_view.pending_summary_mode, Some(false));
        assert_eq!(
            state.transcript_view.pending_simple_override,
            Some(Some(false))
        );
        assert_eq!(
            state.transcript_view.pending_verbose_override,
            Some(Some(false))
        );

        let mut state = ChatState::default();
        state.transcript_view.simple_override = Some(false);
        let effects = handle(
            &mut state,
            &act(ActionKind::SetDetailLevel, Some("summary")),
            &context,
        );
        assert_eq!(effects.len(), 1, "only the summary record is written");
        assert_eq!(state.transcript_view.pending_summary_mode, Some(true));
        assert_eq!(state.transcript_view.pending_simple_override, None);
    }

    /// Turning Summary off always lands on Normal, whatever level was saved under it.
    #[test]
    fn summary_off_goes_to_normal() {
        let context = ChatContext::default();
        let mut state = ChatState::default();
        state.transcript_view.summary_mode = true;
        state.transcript_view.simple_override = Some(false);
        handle(&mut state, &act(ActionKind::ToggleSummary, None), &context);
        assert_eq!(state.transcript_view.pending_summary_mode, Some(false));
        assert_eq!(
            state.transcript_view.pending_simple_override,
            Some(Some(true))
        );
        assert_eq!(
            state.transcript_view.pending_verbose_override,
            Some(Some(false))
        );
    }
}

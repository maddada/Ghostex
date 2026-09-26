//! Projecting one message, and the item list the renderer walks.
//!
//! Ported from `packages/shared/session-chat-controller/native-presentation.ts`.

use std::collections::BTreeMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::{Map, Value};

use crate::document::TranscriptItem;
use crate::state::{
    ChatContext, ChatState, ProjectedMessage, TranscriptViewState, EAGER_TAIL_ITEMS,
};
use crate::transcript::agent_message::{
    agent_display_name, parse_agent_message, parse_inter_agent_message,
};
use crate::transcript::file_changes::{split_file_changes, FileChange};
use crate::transcript::foreign::{
    is_pending_message_id, is_terminal_tool_message, is_working, terminal_tool_activity,
};
use crate::transcript::images::{image_source, ImageRef};
use crate::transcript::jsstr::js_trim;
use crate::transcript::markdown_links::markdown_references;
use crate::transcript::message_text::{
    message_action_content, normalize_user_message_markdown, split_reasoning_headline,
    user_turn_copy_markdown,
};
use crate::transcript::message_time::message_time;
use crate::transcript::native_markdown::native_markdown;
use crate::transcript::noise::suppressed_turn_presentation;
use crate::transcript::prose::prose_markdown;
use crate::transcript::question_exchange::answered_question_exchange;
use crate::transcript::simple::{simple_edit_label, tool_count_label};
use crate::transcript::system_cards::classify_system_card;
use crate::transcript::tool_fold::{pair_tool_blocks, split_blocks, ToolPair};
use crate::transcript::tool_rows::tool_run_shows_all_rows;
use crate::transcript::transcript::{
    completed_chat_work, project_chat_transcript, TranscriptProjection,
};
use crate::transcript::transcript_rows::{file_rows, tool_detail, tool_fold, tool_rows};
use crate::transcript::turns::{partition_completed_work, worked_duration_label, RenderItem};

/// A message's file changes and tool calls, in the order their rows are numbered.
pub fn message_tool_rows(message: &ChatMessage) -> (Vec<FileChange<'_>>, Vec<ToolPair<'_>>) {
    let (_, tools) = split_blocks(&message.blocks);
    let (remaining, changes) = split_file_changes(tools);
    let pairs = pair_tool_blocks(remaining);
    (changes, pairs)
}

fn image_blocks(blocks: &[&ChatBlock]) -> Vec<ImageRef> {
    blocks
        .iter()
        .copied()
        .filter_map(|block| match block {
            ChatBlock::ImageRef { path, url, alt } => Some(ImageRef {
                path: path.clone(),
                url: url.clone(),
                alt: alt.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// CDXC:SessionChat 2026-09-02:
/// A rewind target is a prompt the agent has actually taken: the same "genuine user prompt" test the
/// transcript already uses for its turn boundaries (a suppressed harness turn is not one, a `queued`
/// row is still held by the agent's queue) plus the optimistic local echo, which has no transcript
/// row for the daemon to rewind to yet.
pub fn message_can_rewind(message: &ChatMessage, copy_text: &str, suppressed: &Value) -> bool {
    message.role == ChatRole::User
        && suppressed.is_null()
        && !copy_text.is_empty()
        && !message.queued
        && message.startup_delivery.is_none()
        && !is_pending_message_id(&message.id)
}

/// Agents whose own rewind flow Ghostex drives; anything else never offers the action.
pub fn agent_supports_rewind(agent: Option<&str>) -> bool {
    matches!(agent, Some("claude") | Some("codex") | Some("opencode"))
}

/// The message's own wire keys, which the projection spreads before it adds its own.
fn message_keys(message: &ChatMessage) -> Map<String, Value> {
    match serde_json::to_value(message) {
        Ok(Value::Object(entries)) => entries,
        _ => Map::new(),
    }
}

/// One projected message: the message plus everything the renderer needs to draw it without
/// re-parsing markdown.
pub fn project_message(
    message: &ChatMessage,
    agent_path: &str,
    working_directory: Option<&str>,
    context: &ChatContext,
) -> Value {
    let (prose, tools) = split_blocks(&message.blocks);
    let images = image_blocks(&prose);
    let body = js_trim(&prose_markdown(&message.blocks)).to_string();
    let is_user = message.role == ChatRole::User;
    let displayed_body = if is_user {
        normalize_user_message_markdown(&body)
    } else {
        body.clone()
    };
    let agent_message = parse_agent_message(&body);
    let inter_agent_message = is_user.then(|| parse_inter_agent_message(&body)).flatten();
    let (remaining, changes) = split_file_changes(tools);
    let tool_pairs = pair_tool_blocks(remaining);
    let copy_text = if is_user {
        let blocks: Vec<&ChatBlock> = prose
            .iter()
            .copied()
            .filter(|block| matches!(block, ChatBlock::ImageRef { .. }))
            .collect();
        user_turn_copy_markdown(&displayed_body, &blocks)
    } else {
        body.clone()
    };
    // Code-block headers, GitHub alerts, and typed file paths, marked for the native renderer.
    let native_body = native_markdown(&displayed_body, is_user);
    let suppressed = suppressed_turn_presentation(message);
    let rows = tool_rows(&tool_pairs, agent_path);
    let system_card = classify_system_card(message, &displayed_body);
    let reasoning = split_reasoning_headline(&body);
    let questions: Vec<Value> = {
        let (_, all_tools) = split_blocks(&message.blocks);
        pair_tool_blocks(all_tools)
            .iter()
            .filter_map(answered_question_exchange)
            .collect()
    };
    let mut changed_paths: Vec<&str> = Vec::new();
    for change in &changes {
        if !changed_paths.contains(&change.path.as_str()) {
            changed_paths.push(&change.path);
        }
    }

    let mut projected = message_keys(message);
    projected.insert("text".to_string(), native_body.clone().into());
    projected.insert("copyText".to_string(), copy_text.clone().into());
    projected.insert(
        "canRewind".to_string(),
        (message_can_rewind(message, &copy_text, &suppressed)
            && !inter_agent_message
                .as_ref()
                .is_some_and(|inter| inter.cross_session))
        .into(),
    );
    projected.insert("actionContent".to_string(), message_action_content(&body));
    projected.insert("time".to_string(), message_time(message.timestamp, context));
    projected.insert(
        "markdownReferences".to_string(),
        Value::Array(markdown_references(&native_body)),
    );
    projected.insert(
        "reasoning".to_string(),
        serde_json::json!({ "headline": reasoning.headline, "body": reasoning.body }),
    );
    projected.insert(
        "agentMessage".to_string(),
        match &agent_message {
            Some(agent_message) => serde_json::json!({
                "sender": agent_message.sender,
                "body": agent_message.body,
                "name": agent_display_name(&agent_message.sender),
            }),
            None => Value::Null,
        },
    );
    projected.insert(
        "interAgentMessage".to_string(),
        match &inter_agent_message {
            Some(inter) => serde_json::json!({
                "agentName": inter.agent_name,
                "sessionTitle": inter.session_title,
                "sessionId": inter.session_id,
                "agentId": inter.agent_id,
                "agentSessionId": inter.agent_session_id,
                "replyTo": inter.reply_to,
                "body": inter.body,
            }),
            None => Value::Null,
        },
    );
    projected.insert("questions".to_string(), Value::Array(questions));
    projected.insert(
        "images".to_string(),
        Value::Array(images.iter().map(image_source).collect()),
    );
    projected.insert("suppressed".to_string(), suppressed);
    /* The expanded subagent-message card renders its body as Markdown, so it needs the same marks
    and reference links the turn's own body gets; the collapsed clamp keeps the raw text React
    clamps. */
    projected.insert(
        "systemCard".to_string(),
        match system_card {
            Value::Object(mut card)
                if card.get("kind") == Some(&Value::String("agent-message".into())) =>
            {
                let body = card
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                card.insert("markdown".to_string(), native_markdown(&body, false).into());
                Value::Object(card)
            }
            other => other,
        },
    );
    projected.insert(
        "files".to_string(),
        Value::Array(file_rows(&changes, &message.id, working_directory)),
    );
    projected.insert(
        "simpleFileLabel".to_string(),
        simple_edit_label(changed_paths.len()).into(),
    );
    /* React counts only the work rows for this label: an answered question is conversation, so its
    card sits outside the group and is not one of the "N tool calls". */
    let call_rows = rows
        .iter()
        .filter(|row| {
            row.get("hasCall") == Some(&Value::Bool(true))
                && row.get("exchange") != Some(&Value::Bool(true))
        })
        .count();
    projected.insert(
        "simpleToolLabel".to_string(),
        tool_count_label(call_rows).into(),
    );
    projected.insert("tools".to_string(), Value::Array(rows));
    projected.insert("toolFold".to_string(), tool_fold(&tool_pairs));
    projected.insert(
        "toolsShowAllRows".to_string(),
        tool_run_shows_all_rows(!body.is_empty()).into(),
    );
    projected.insert(
        "terminalTool".to_string(),
        if is_terminal_tool_message(message) {
            terminal_tool_activity(message)
        } else {
            Value::Null
        },
    );
    Value::Object(projected)
}

/// CDXC:SessionChat 2026-09-18 WHY:
/// Projecting a message parses its markdown twice (bare file paths, then references), and a
/// 139-message transcript took about 800ms of QuickJS before its first item existed. The transcript
/// follows its tail, so only the newest items are projected before the first publish; older ones
/// ship as plain-text placeholders and are backfilled in batches on the runtime's timer, newest
/// first, each batch republishing.
fn placeholder(message: &ChatMessage) -> Value {
    let text = js_trim(
        &message
            .blocks
            .iter()
            .filter_map(|block| match block {
                ChatBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .to_string();
    let mut projected = message_keys(message);
    projected.insert("text".to_string(), text.clone().into());
    projected.insert("copyText".to_string(), text.into());
    projected.insert("pending".to_string(), true.into());
    Value::Object(projected)
}

/// The projection pass, with the placeholder queue it filled.
pub struct Projection {
    pub items: Vec<TranscriptItem>,
    pub final_ids: Vec<String>,
    /// Ids that shipped as placeholders and want a backfill batch.
    pub backfill: Vec<ChatMessage>,
    /// The minimap rail for this pass.
    ///
    /// `NativeChatPresentation.update` builds the rail inside the same result object as the items
    /// (native-presentation.ts:326), off the SUMMARY turns whichever mode the transcript is in,
    /// pointing at the row index of the list it just built. Family f owns the rail's rules; this
    /// pass is where the TypeScript calls them, so it is where the core calls them too.
    pub minimap: Vec<crate::extras::minimap_rail::MinimapMarkerRow>,
}

/// The inputs of ONE `NativeChatPresentation` instance.
///
/// Two exist: the session's own, whose scope is [`scope`] over [`ChatState`], and the subagent
/// viewer's, whose scope is the page it read (`native-subagent.ts:175` builds its own projector and
/// calls `update(page.messages, this.working, false, NO_DEFERRED_WORK, 0)`). Both run the same
/// rules over different messages, a different `agentPath` and a cache of their own, which is why
/// the pass is parameterised rather than reading `ChatState` directly.
pub struct ProjectionScope<'a> {
    pub messages: &'a [ChatMessage],
    pub working: bool,
    pub summary: bool,
    /// The rows a `loadWork` read brought back, keyed by the turn's user-message id. A child
    /// transcript has none of its own.
    pub deferred: &'a BTreeMap<String, Vec<ChatMessage>>,
    /// `/root` for the session, the child's own path inside the subagent viewer.
    pub agent_path: &'a str,
    /// Shortens the paths on file-change cards. Module level in the TypeScript, so both instances
    /// read the same one.
    pub working_directory: Option<&'a str>,
}

/// The session's own scope, off the composed list family a builds.
pub fn scope<'a>(state: &'a ChatState, view: &'a TranscriptViewState) -> ProjectionScope<'a> {
    ProjectionScope {
        messages: &state.messages.composed,
        working: is_working(state),
        summary: view.summary_mode,
        deferred: &view.deferred,
        agent_path: &view.agent_path,
        working_directory: view.working_directory.as_deref(),
    }
}

/// The per-message cache one presentation instance owns (`modelsById`), keyed by message id.
pub type ProjectionCache = BTreeMap<String, ProjectedMessage>;

/// `NativeChatPresentation.message`: the cached projection when the source is data-identical,
/// otherwise a fresh one, which is cached in its place.
pub fn project_cached(
    cache: &mut ProjectionCache,
    scope: &ProjectionScope<'_>,
    context: &ChatContext,
    message: &ChatMessage,
) -> Value {
    if let Some(entry) = cache.get(&message.id) {
        if entry.source == *message {
            return entry.model.clone();
        }
    }
    let model = project_message(message, scope.agent_path, scope.working_directory, context);
    cache.insert(
        message.id.clone(),
        ProjectedMessage {
            source: message.clone(),
            model: model.clone(),
        },
    );
    model
}

/// `pruneModels`: once the cache is well past the transcript's size, drop every entry whose id is
/// no longer in the transcript or its deferred rows.
fn prune_cache(cache: &mut ProjectionCache, scope: &ProjectionScope<'_>) {
    if cache.len() <= scope.messages.len() * 2 + 64 {
        return;
    }
    let mut live: std::collections::BTreeSet<&str> = scope
        .messages
        .iter()
        .map(|message| message.id.as_str())
        .collect();
    for rows in scope.deferred.values() {
        live.extend(rows.iter().map(|row| row.id.as_str()));
    }
    let stale: Vec<String> = cache
        .keys()
        .filter(|id| !live.contains(id.as_str()))
        .cloned()
        .collect();
    for id in stale {
        cache.remove(&id);
    }
}

struct Builder<'a> {
    scope: &'a ProjectionScope<'a>,
    context: &'a ChatContext,
    cache: &'a mut ProjectionCache,
    eager_from: usize,
    backfill: Vec<ChatMessage>,
}

impl Builder<'_> {
    /// `projected`: the cached model when the cache holds this exact message.
    fn projected(&self, message: &ChatMessage) -> Option<Value> {
        self.cache
            .get(&message.id)
            .filter(|entry| entry.source == *message)
            .map(|entry| entry.model.clone())
    }

    /// The full projection when it is cheap or the row is near the tail; otherwise a stable
    /// plain-text stand-in queued for backfill.
    fn message_or_placeholder(&mut self, message: &ChatMessage, eager: bool) -> Value {
        if let Some(model) = self.projected(message) {
            return model;
        }
        if eager {
            return project_cached(self.cache, self.scope, self.context, message);
        }
        self.backfill.push(message.clone());
        placeholder(message)
    }

    fn eager(&self, index: usize) -> bool {
        index >= self.eager_from
    }
}

fn message_id(item: &TranscriptItem) -> String {
    match item {
        TranscriptItem::Message { message } => message
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        TranscriptItem::Summary { id, .. } | TranscriptItem::CompletedWork { id, .. } => id.clone(),
        TranscriptItem::Unknown(_) => String::new(),
    }
}

/// The whole transcript list for the state as it stands, against a copy of its cache.
///
/// For the pure readers (`rows`, `document`) on the turns before the first refresh; the refresh
/// itself builds against the state's own cache so what it projects is kept.
pub fn build(state: &ChatState, context: &ChatContext) -> Projection {
    let mut cache = state.transcript_view.projected.clone();
    build_scope(&scope(state, &state.transcript_view), context, &mut cache)
}

/// The whole transcript list for one presentation instance's scope, projecting through (and
/// into) that instance's cache.
pub fn build_scope(
    scope: &ProjectionScope,
    context: &ChatContext,
    cache: &mut ProjectionCache,
) -> Projection {
    let projection: TranscriptProjection =
        project_chat_transcript(scope.messages, scope.working, &[]);
    let summary = scope.summary;
    let length = if summary {
        projection.summary_turns.len()
    } else {
        projection.items.len()
    };
    let mut builder = Builder {
        scope,
        context,
        cache,
        eager_from: length.saturating_sub(EAGER_TAIL_ITEMS),
        backfill: Vec::new(),
    };

    let items: Vec<TranscriptItem> = if summary {
        projection
            .summary_turns
            .iter()
            .enumerate()
            .map(|(index, turn)| {
                let eager = builder.eager(index);
                TranscriptItem::Summary {
                    id: turn.user.id.clone(),
                    user: builder.message_or_placeholder(&turn.user, eager),
                    final_message: turn
                        .final_message
                        .as_ref()
                        .map(|message| builder.message_or_placeholder(message, eager)),
                    active: turn.active,
                    work: turn
                        .active_work
                        .iter()
                        .map(|message| builder.message_or_placeholder(message, eager))
                        .collect(),
                }
            })
            .collect()
    } else {
        projection
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let eager = builder.eager(index);
                let turn = match item {
                    RenderItem::Message(message) => {
                        return TranscriptItem::Message {
                            message: builder.message_or_placeholder(message, eager),
                        }
                    }
                    RenderItem::CompletedWork(turn) => turn,
                };
                let work =
                    completed_chat_work(turn, scope.deferred.get(&turn.user.id).map(Vec::as_slice));
                let (visible_artifacts, collapsed_work) = partition_completed_work(&work);
                /*
                A finished turn's writes leave their rows and collect under one "N files changed"
                fold. The rows come back through the per-message projection so an unchanged turn
                keeps the same cards and ships nothing.
                */
                let mut files: Vec<Value> = Vec::new();
                let sources: Vec<&ChatMessage> =
                    work.iter().chain(turn.final_message.iter()).collect();
                for message in sources {
                    if let Value::Array(rows) = builder
                        .message_or_placeholder(message, eager)
                        .get("files")
                        .cloned()
                        .unwrap_or(Value::Null)
                    {
                        files.extend(rows);
                    }
                }
                let mut changed: Vec<String> = Vec::new();
                for file in &files {
                    if let Some(path) = file.get("path").and_then(Value::as_str) {
                        if !changed.iter().any(|seen| seen == path) {
                            changed.push(path.to_string());
                        }
                    }
                }
                if let Some(deferred) = &turn.user.deferred_work {
                    for path in &deferred.file_paths {
                        if !changed.iter().any(|seen| seen == path) {
                            changed.push(path.clone());
                        }
                    }
                }
                let changed_files = changed.len();
                let deferred_value = turn
                    .user
                    .deferred_work
                    .as_ref()
                    .and_then(|deferred| serde_json::to_value(deferred).ok());
                TranscriptItem::CompletedWork {
                    id: turn.user.id.clone(),
                    label: worked_duration_label(
                        turn.user.timestamp,
                        turn.final_message
                            .as_ref()
                            .and_then(|message| message.timestamp)
                            .or_else(|| {
                                turn.user
                                    .deferred_work
                                    .as_ref()
                                    .and_then(|deferred| deferred.completed_at)
                            }),
                    ),
                    files,
                    files_label: format!(
                        "{changed_files} {} changed",
                        if changed_files == 1 { "file" } else { "files" }
                    ),
                    simple_files_label: simple_edit_label(changed_files),
                    expandable: !collapsed_work.is_empty() || turn.user.deferred_work.is_some(),
                    deferred: deferred_value,
                    work: collapsed_work
                        .iter()
                        .map(|message| builder.message_or_placeholder(message, eager))
                        .collect(),
                    /* The turn's answered question cards, hoisted out of the fold. */
                    questions: work
                        .iter()
                        .flat_map(|row| {
                            match builder
                                .message_or_placeholder(row, eager)
                                .get("questions")
                                .cloned()
                            {
                                Some(Value::Array(rows)) => rows,
                                _ => Vec::new(),
                            }
                        })
                        .collect(),
                    artifacts: visible_artifacts
                        .iter()
                        .map(|message| builder.message_or_placeholder(message, eager))
                        .collect(),
                    final_message: turn
                        .final_message
                        .as_ref()
                        .map(|message| builder.message_or_placeholder(message, eager)),
                }
            })
            .collect()
    };

    // `itemIndex`: the first row that draws a given id, which is what a dash jumps to.
    let mut item_index: Vec<(String, usize)> = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let id = message_id(item);
        if !item_index.iter().any(|(known, _)| *known == id) {
            item_index.push((id, index));
        }
    }
    let turns: Vec<_> = projection
        .summary_turns
        .iter()
        .map(|turn| (&turn.user, turn.final_message.as_ref()))
        .collect();
    let minimap = crate::extras::minimap::project_minimap_turns(&turns, &item_index);
    let backfill = builder.backfill;
    prune_cache(cache, scope);
    Projection {
        items,
        final_ids: projection.final_ids,
        backfill,
        minimap,
    }
}

/// What an open row shows, read from the message the row was projected from: a tool's arguments and
/// result, or a file card's diff.
pub fn row_detail(
    state: &ChatState,
    context: &ChatContext,
    kind: &str,
    message_id: &str,
    index: usize,
) -> Option<Value> {
    let _ = context;
    row_detail_scope(&state.transcript_view.projected, kind, message_id, index)
}

/// The same detail, read against one presentation instance's own cache.
///
/// `rowDetail` reads `this.modelsById.get(messageId)?.source`: the message as it was PROJECTED,
/// which for a folded tool-only message is the anchor's merged blocks, and nothing at all for a row
/// still drawn as a placeholder. `native-host.ts:424` is `presentation.rowDetail(...) ??
/// subagentViewer.rowDetail(...)`: the open row belongs to whichever of the two transcripts
/// projected a message with that id, and only the viewer's own cache can answer for a child's row.
pub fn row_detail_scope(
    cache: &ProjectionCache,
    kind: &str,
    message_id: &str,
    index: usize,
) -> Option<Value> {
    let source = &cache.get(message_id)?.source;
    let (files, tools) = message_tool_rows(source);
    if kind == "file" {
        let change = files.get(index)?;
        return Some(serde_json::json!({ "lines": change.lines }));
    }
    tools.get(index).map(tool_detail)
}

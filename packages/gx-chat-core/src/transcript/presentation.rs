//! Projecting one message, and the item list the renderer walks.
//!
//! Ported from `packages/shared/session-chat-controller/native-presentation.ts`.

use std::collections::BTreeMap;
use std::sync::Arc;

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::{Map, Value};

use crate::document::{TranscriptItem, WorkTally};
use crate::state::{
    ChatContext, ChatState, ProjectedMessage, TranscriptViewState, EAGER_TAIL_ITEMS,
};
use crate::transcript::agent_message::{
    agent_display_name, parse_agent_message, parse_inter_agent_message,
};
use crate::transcript::background_rows::background_task_source;
use crate::transcript::file_changes::{split_file_changes, FileChange};
use crate::transcript::foreign::{
    is_pending_message_id, is_terminal_tool_message, is_working, terminal_tool_activity,
};
use crate::transcript::images::{image_source, ImageRef};
use crate::transcript::jsstr::js_trim;
use crate::transcript::line_breaks::{agent_line_breaks, user_line_breaks, AgentLineBreaks};
use crate::transcript::long_prompt::collapsed_prompt;
use crate::transcript::markdown_links::markdown_references;
use crate::transcript::marked_sections::mark_sections;
use crate::transcript::message_text::{
    message_action_content, normalize_user_message_markdown, split_reasoning_headline,
    user_turn_copy_markdown,
};
use crate::transcript::message_time::message_time;
use crate::transcript::native_markdown::{
    native_markdown, native_markdown_with_mentions, without_question_forms,
};
use crate::transcript::noise::suppressed_turn_presentation;
use crate::transcript::open_questions::{
    askers, link_question_tags, linkable_tags, linked_words, tag_words, Asker, QuestionTag,
};
use crate::transcript::page_answers::{
    page_answers_in, project_page_answers, split_page_answers, PageAnswers,
};
use crate::transcript::prose::prose_markdown;
use crate::transcript::question_exchange::answered_question_exchange;
use crate::transcript::question_forms::{question_forms, QuestionForm};
use crate::transcript::sent_message::{project_sent_messages, resolve_recipients};
use crate::transcript::simple::{edit_label, tool_count_label};
use crate::transcript::system_cards::classify_system_card;
use crate::transcript::tool_fold::{pair_tool_blocks, split_blocks, ToolPair};
use crate::transcript::tool_rows::{
    list_tools_in_work_fold, mark_work_fold, tool_run_shows_all_rows, unmark_work_fold,
};
use crate::transcript::transcript::{
    completed_chat_work, project_chat_transcript, TranscriptProjection,
};
use crate::transcript::transcript_rows::{file_rows, open_tool_detail, tool_fold, tool_rows};
use crate::transcript::turns::{
    is_omp_wake_kind, partition_completed_work, worked_duration_label, RenderItem,
};
use crate::transcript::work_summary::{counted_work_stats, tool_call_count, tool_spans};

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
#[allow(clippy::too_many_arguments)]
pub fn project_message(
    message: &ChatMessage,
    forms: &std::cell::OnceCell<Vec<QuestionForm>>,
    messages: &[ChatMessage],
    agent_path: &str,
    working_directory: Option<&str>,
    line_breaks: AgentLineBreaks,
    context: &ChatContext,
    spans: &std::collections::HashMap<String, (i64, i64)>,
    question_tags: &[QuestionTag],
    tool_cards: bool,
    working: bool,
) -> Value {
    let durations: std::collections::HashMap<String, i64> = spans
        .iter()
        .filter(|(_, (start, end))| end > start)
        .map(|(id, (start, end))| (id.clone(), end - start))
        .collect();
    let durations = &durations;
    let (prose, tools) = split_blocks(&message.blocks);
    let images = image_blocks(&prose);
    let body = js_trim(&prose_markdown(&message.blocks)).to_string();
    let is_user = message.role == ChatRole::User;
    // Answers a page or question form sent: the card stands for the envelope, and the bubble below
    // it shows only what the user wrote besides (`page_answers.rs`).
    let page_answers = is_user.then(|| split_page_answers(&body)).flatten();
    let user_words = match &page_answers {
        Some((_, rest)) if !rest.is_empty() => rest.as_str(),
        _ => body.as_str(),
    };
    // A side chat's first message: the bubble shows the question, not the note to its agent.
    let user_words = crate::composer::side_chat::without_side_chat_note(user_words);
    let user_markdown = is_user.then(|| normalize_user_message_markdown(user_words));
    // Review comment blocks show as the pills they were sent from (`review_comments.rs`); copying
    // the message still gives the full blocks.
    let displayed_body = if let Some(markdown) = &user_markdown {
        crate::composer::review_comments::collapse_review_comments(markdown)
    } else if message.role == ChatRole::Assistant {
        // Q-tags become links to the question that asked them (`open_questions.rs`).
        link_question_tags(&agent_line_breaks(&body, line_breaks), question_tags)
    } else if message.role == ChatRole::Reasoning {
        agent_line_breaks(&body, line_breaks)
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
        user_turn_copy_markdown(user_markdown.as_deref().unwrap_or(&displayed_body), &blocks)
    } else {
        body.clone()
    };
    // Code-block headers, GitHub alerts, and typed file paths, marked for the native renderer.
    // A ```questions block somebody typed stays code: only the agent's own reply asks the reader.
    let mentions = message.mention_paths.as_deref().unwrap_or_default();
    let native_body = if is_user {
        without_question_forms(native_markdown_with_mentions(
            &user_line_breaks(&displayed_body),
            true,
            mentions,
        ))
    } else {
        native_markdown(&displayed_body, false)
    };
    // The reply's marked sections, tagged after its references are read so the marks carry none.
    let marked_body = if message.role == ChatRole::Assistant {
        mark_sections(native_body.clone())
    } else {
        native_body.clone()
    };
    // A long prompt's first lines, which its bubble shows until it is expanded.
    let collapsed_body = is_user
        .then(|| collapsed_prompt(&displayed_body))
        .flatten()
        .map(|preview| {
            without_question_forms(native_markdown_with_mentions(
                &user_line_breaks(&preview),
                true,
                mentions,
            ))
        });
    let suppressed = suppressed_turn_presentation(message);
    let rows = tool_rows(&tool_pairs, agent_path, working_directory, durations);
    let system_card = classify_system_card(message, &displayed_body);
    let reasoning = split_reasoning_headline(&body);
    let questions: Vec<Value> = {
        let (_, all_tools) = split_blocks(&message.blocks);
        pair_tool_blocks(all_tools)
            .iter()
            .filter_map(answered_question_exchange)
            .collect()
    };
    let (changed_paths, failed_paths) = crate::transcript::simple::edited_and_failed_paths(
        changes
            .iter()
            .map(|change| (change.path.as_str(), change.result_is_error())),
    );

    let mut projected = message_keys(message);
    projected.insert("text".to_string(), marked_body.into());
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
        "collapsedReferences".to_string(),
        Value::Array(
            collapsed_body
                .as_deref()
                .map(markdown_references)
                .unwrap_or_default(),
        ),
    );
    projected.insert(
        "collapsedText".to_string(),
        collapsed_body.map_or(Value::Null, Value::String),
    );
    projected.insert(
        "reasoning".to_string(),
        serde_json::json!({
            "headline": reasoning.headline,
            "body": agent_line_breaks(&reasoning.body, line_breaks),
        }),
    );
    if message.role == ChatRole::Reasoning {
        projected.insert(
            "thinkingRow".to_string(),
            crate::transcript::thinking_card::thinking_row(&body),
        );
    }
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
                "subagent": inter.subagent,
                "body": agent_line_breaks(&inter.body, line_breaks),
            }),
            None => Value::Null,
        },
    );
    projected.insert(
        "pageAnswers".to_string(),
        page_answers
            .as_ref()
            .map_or(Value::Null, |(answers, rest)| {
                // An in-chat form's answers show its choices' labels (`project_page_answers`).
                let forms = forms.get_or_init(|| question_forms(messages));
                let form = forms.iter().find(|form| form.id == answers.page_id);
                let mut card = project_page_answers(answers, form);
                card["withText"] = Value::Bool(!rest.is_empty());
                card
            }),
    );
    projected.insert("questions".to_string(), Value::Array(questions));
    projected.insert(
        "sentMessages".to_string(),
        Value::Array(project_sent_messages(
            message,
            &tool_pairs,
            line_breaks,
            spans,
        )),
    );
    // The Workflow calls this message made, by call id: the document's `workflowCards` holds each
    // run's card, drawn after the message's tool rows (`workflow_card.rs`).
    projected.insert(
        "workflows".to_string(),
        Value::Array(
            tool_pairs
                .iter()
                .filter_map(crate::transcript::workflow_card::workflow_call_id)
                .map(Value::String)
                .collect(),
        ),
    );
    // The "GBot added" cards this message's commands printed (`gbot_cards.rs`), drawn after its
    // tool rows; only present when there are some.
    let gbot_cards: Vec<Value> = tool_pairs
        .iter()
        .flat_map(crate::transcript::gbot_cards::gbot_cards)
        .collect();
    if !gbot_cards.is_empty() {
        projected.insert("gbotCards".to_string(), Value::Array(gbot_cards));
    }
    projected.insert(
        "images".to_string(),
        Value::Array(images.iter().map(image_source).collect()),
    );
    // A slash command is the user's bubble with its text in monospace (`is_slash_command_line`).
    projected.insert(
        "slashCommand".to_string(),
        (is_user
            && suppressed.is_null()
            && crate::transcript::local_command::is_slash_command_line(&copy_text))
        .into(),
    );
    projected.insert("suppressed".to_string(), suppressed);
    projected.insert(
        "apiError".to_string(),
        crate::transcript::api_errors::api_error_presentation(message),
    );
    projected.insert(
        "sideQuestion".to_string(),
        crate::transcript::side_question::side_question_presentation(message),
    );
    /* The expanded subagent-message card renders its body as Markdown, so it needs the same marks
    and reference links the turn's own body gets; the collapsed clamp keeps the raw text React
    clamps. A card the classifier already built (an OMP agent's message) keeps its own sentence and
    logo and gets the row's time; an advisor card's notes get the same marks. */
    let time_label = Value::from(
        projected
            .get("time")
            .and_then(|time| time.get("label"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
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
                card.insert(
                    "markdown".to_string(),
                    native_markdown(&agent_line_breaks(&body, line_breaks), false).into(),
                );
                if let Some(shown) = card.get_mut("card").filter(|shown| shown.is_object()) {
                    shown["time"] = time_label;
                } else {
                    let name = card
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    card.insert(
                        "card".to_string(),
                        crate::transcript::agent_message_card::subagent_card(&name),
                    );
                }
                Value::Object(card)
            }
            Value::Object(mut card)
                if card.get("kind") == Some(&Value::String("advisor".into())) =>
            {
                card.insert("time".to_string(), time_label);
                if let Some(notes) = card.get_mut("notes").and_then(Value::as_array_mut) {
                    for note in notes {
                        let marked =
                            native_markdown(note["text"].as_str().unwrap_or_default(), false);
                        note["references"] = Value::Array(markdown_references(&marked));
                        note["markdown"] = marked.into();
                    }
                }
                Value::Object(card)
            }
            other => other,
        },
    );
    let files = file_rows(&changes, &message.id, working_directory);
    // Settings > Chat > Tool call cards: the message's calls and edits as one card
    // (`tool_card.rs`); off, the rows draw as before.
    if tool_cards {
        let (_, tool_blocks) = split_blocks(&message.blocks);
        if let Some(card) = crate::transcript::tool_card::tool_card(
            &tool_blocks,
            &rows,
            &files,
            spans,
            working_directory,
            working,
        ) {
            projected.insert("toolCard".to_string(), card);
        }
    }
    projected.insert(
        "fileCard".to_string(),
        crate::transcript::file_change_rows::file_change_card(&files),
    );
    projected.insert("files".to_string(), Value::Array(files));
    projected.insert(
        "simpleFileLabel".to_string(),
        edit_label(changed_paths.len(), failed_paths.len()).into(),
    );
    /* React counts only the work rows for this label: an answered question is conversation, so its
    card sits outside the group and is not one of the "N tool calls". */
    let call_rows = rows
        .iter()
        .filter(|row| {
            row.get("hasCall") == Some(&Value::Bool(true))
                && row.get("exchange") != Some(&Value::Bool(true))
                && row.get("sentMessage") != Some(&Value::Bool(true))
        })
        .count();
    projected.insert(
        "simpleToolLabel".to_string(),
        tool_count_label(call_rows).into(),
    );
    // The pictures the message's calls opened from disk, which its turn's fold keeps showing
    // (`read_images.rs`).
    projected.insert(
        "readImages".to_string(),
        Value::Array(crate::transcript::read_images::message_read_images(&rows)),
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
    // A `!` command the user ran: drawn as its tool card where the user's message sits.
    projected.insert(
        "shellCommand".to_string(),
        crate::transcript::local_command::is_shell_command_message(message).into(),
    );
    // What that card draws: its header, status and output.
    projected.insert(
        "shellCard".to_string(),
        crate::transcript::local_command::shell_card(message),
    );
    let mut projected = Value::Object(projected);
    crate::transcript::agent_message_card::dress_received(&mut projected);
    projected
}

/// CDXC:SessionChat 2026-09-18 WHY:
/// Projecting a message parses its markdown twice (bare file paths, then references), and a
/// 139-message transcript took about 800ms of QuickJS before its first item existed. The transcript
/// follows its tail, so only the newest items are projected before the first publish; older ones
/// ship as plain-text placeholders and are backfilled in batches on the runtime's timer, newest
/// first, each batch republishing.
fn placeholder(message: &ChatMessage) -> Value {
    let message = &*background_task_source(message);
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
    /// The published pages whose answers arrived in this transcript (`page_answers.rs`).
    pub answered_pages: Vec<String>,
    /// The question forms the agent's replies hold (`question_forms.rs`).
    pub question_forms: Vec<QuestionForm>,
    /// The answers that arrived, for pages and forms alike.
    pub page_answers: Vec<PageAnswers>,
    /// The forms and question pages, oldest first (`open_questions.rs`).
    pub askers: Vec<Asker>,
    /// Ids that shipped as placeholders and want a backfill batch.
    pub backfill: Vec<ChatMessage>,
    /// The minimap rail for this pass.
    ///
    /// `NativeChatPresentation.update` builds the rail inside the same result object as the items
    /// (native-presentation.ts:326), off the SUMMARY turns whichever mode the transcript is in,
    /// pointing at the row index of the list it just built. Family f owns the rail's rules; this
    /// pass is where the TypeScript calls them, so it is where the core calls them too.
    pub minimap: Vec<crate::extras::minimap_rail::MinimapMarkerRow>,
    /// The newest turn a user row opened, its id and its "Worked for 2m 13s", once it has finished
    /// and nothing was sent after it: the working card's line above the composer
    /// (`extras::working_card`, `latest_worked_turn`).
    pub worked_turn: Option<(String, String)>,
}

/// When a finished turn ended, as its "Worked for" label times it. `loaded` is the turn's part of
/// the rows a deferred read brought back (`section_part`), if they are here.
fn turn_end(
    turn: &crate::transcript::turns::CompletedWorkTurn,
    loaded: Option<&[ChatMessage]>,
) -> Option<i64> {
    turn.final_message
        .as_ref()
        .and_then(|message| message.timestamp)
        .or_else(|| {
            turn.user
                .deferred_work
                .as_ref()
                .and_then(|deferred| deferred.completed_at)
        })
        // An interrupted turn, or a wake-up the agent answered only with work, has no final
        // reply; it ran until its last row, the loaded ones included.
        .or_else(|| {
            let kept = turn.work.iter().rev().find_map(|row| row.timestamp);
            let loaded = loaded.and_then(|rows| rows.iter().filter_map(|row| row.timestamp).max());
            kept.max(loaded)
        })
}

/// "Worked for 2m 5s" for a finished turn, the label on its fold; `later` are the render items
/// after it. A turn a user row opened runs on through the turns of the OMP wake-ups after it
/// (`turns::opens_omp_turn`) to the newest of their replies; a wake-up's own turn is timed alone.
///
/// CDXC:SessionChat 2026-11-03 WHY: an OMP background job's result, IRC message or advisor note that wakes the agent after its reply opens a turn of its own, so the reply before it stays in view. Timed alone, the newest such turn took the working card's "Worked for" line and the "Previous turn" fold from the prompt, and the card read the wake-up's few seconds instead of the run. The prompt's turn keeps both and is timed from the prompt to the agent's newest reply, which is the time the chat showed when OMP rows opened no turn; a wake-up answered without a reply adds nothing, as it did not then either. A Claude background task's end arrives as a user row and keeps its own turn's line, as before.
fn turn_worked_label(
    turn: &crate::transcript::turns::CompletedWorkTurn,
    loaded: Option<&[ChatMessage]>,
    later: &[RenderItem],
) -> String {
    let mut end = turn_end(turn, loaded);
    if !is_omp_wake_kind(&turn.user) {
        for item in later {
            match item {
                RenderItem::Message(message) if message.role == ChatRole::User => break,
                RenderItem::Message(_) => {}
                RenderItem::CompletedWork(wake) if is_omp_wake_kind(&wake.user) => {
                    end = end.max(
                        wake.final_message
                            .as_ref()
                            .and_then(|reply| reply.timestamp),
                    );
                }
                RenderItem::CompletedWork(_) => break,
            }
        }
    }
    worked_duration_label(turn.user.timestamp, end)
}

/// The times a Reduced line reads off `turn` (`grouped_work.rs`): when it started, when it ended,
/// and how far it carries a line an earlier turn opened, which for an OMP wake-up is its reply
/// alone, as [`turn_worked_label`] runs a prompt's time on through them.
fn turn_times(
    turn: &crate::transcript::turns::CompletedWorkTurn,
    loaded: Option<&[ChatMessage]>,
) -> (Option<i64>, Option<i64>, Option<i64>) {
    let end = turn_end(turn, loaded);
    let extends_to = if is_omp_wake_kind(&turn.user) {
        turn.final_message
            .as_ref()
            .and_then(|reply| reply.timestamp)
    } else {
        end
    };
    (turn.user.timestamp, end, extends_to)
}

/// The Worked line of the newest turn a user row opened, when it has finished, timed, with no
/// prompt after it. The turns of the OMP wake-ups after it continue its run (`turn_worked_label`).
pub(crate) fn latest_worked_turn(items: &[RenderItem], working: bool) -> Option<(String, String)> {
    if working {
        return None;
    }
    for (at, item) in items.iter().enumerate().rev() {
        match item {
            RenderItem::Message(message) if message.role == ChatRole::User => return None,
            RenderItem::Message(_) => continue,
            RenderItem::CompletedWork(turn) if is_omp_wake_kind(&turn.user) => continue,
            RenderItem::CompletedWork(turn) => {
                let label = turn_worked_label(turn, None, &items[at + 1..]);
                // A turn with no length to show says only "Worked", which would be no news.
                return (label != "Worked").then(|| (turn.user.id.clone(), label));
            }
        }
    }
    None
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
    pub line_breaks: AgentLineBreaks,
    /// The side question whose card is open above the composer; its row waits for Close.
    pub live_side_question: Option<String>,
    /// The tool approval the agent is blocked on, whose call the list draws open.
    pub approval_wait: Option<crate::transcript::approval_wait::ApprovalWait>,
    /// The Subagents roster, which names the background agents whose messages the session gets.
    pub agent_fleet: Option<&'a Value>,
    /// The host's session list, which names the other session on a message between agents.
    pub session_directory: &'a [crate::event::KnownSession],
    /// The forms and question pages these messages hold, before their answers are read
    /// (`open_questions.rs`), and the tags a reply's words link to.
    pub askers: Vec<Asker>,
    pub question_tags: Vec<QuestionTag>,
    /// How the messages this session sent other agents and left waiting ended, by send id.
    pub agent_message_deliveries: Option<&'a Value>,
    /// The ids this instance's backfill batches already projected
    /// ([`TranscriptViewState::backfilled`]).
    pub backfilled: &'a std::collections::BTreeSet<String>,
    /// `question_forms(messages)`, read once per pass the first time an answers card needs it
    /// (or handed in already read, `transcript/settled.rs`).
    pub question_forms: std::cell::OnceCell<Vec<QuestionForm>>,
    /// Settings > Chat > Tool call cards (`chatToolCallCards`): each message's calls project as one
    /// card (`tool_card.rs`).
    pub tool_cards: bool,
    /// Agent work is Reduced or Quiet (`transcript/chat_look.rs`): the finished turns for each message of
    /// the user's fold into one `grouped-work` row (`grouped_work.rs`).
    pub grouped_work: bool,
    /// Agent work is Quiet (`transcript/chat_look.rs`): the grouped rows move every reply's calls
    /// into their work.
    pub hidden_work: bool,
    /// [`crate::session::working::hides_running_response`]: the running response draws only what
    /// needs the user (`grouped_work::hide_work`).
    pub hides_running: bool,
    /// [`crate::session::working::quiet_finished_before`].
    pub quiet_finished_before: Option<i64>,
    /// [`TranscriptViewState::picture_digests`]: which pictures read from disk the user already
    /// showed (`read_images.rs`).
    pub picture_digests: &'a std::collections::HashMap<String, u64>,
    /// [`ProjectionScope::seen_pictures`], read once per pass.
    pub seen_pictures: std::cell::OnceCell<crate::transcript::read_images::SeenPictures>,
}

impl<'a> ProjectionScope<'a> {
    /// The pictures the user's own messages show, which a closed fold's strip leaves out
    /// (`read_images.rs`), read off every user message once per pass.
    ///
    /// CDXC:SessionChat 2026-11-06 WHY: User: "bro why is the app laggy now??". `build_items` read
    /// them for each chunk it built, and settling the prefix builds one chunk per turn, so a
    /// 14,828-row coordinator chat scanned every user message for picture paths once per turn: 70%
    /// of a 25-second settle on the chat host thread, which every chat and every paste waits on.
    /// Every chunk of a pass reads the same messages and digests, so one read serves them all.
    pub fn seen_pictures(&self) -> &crate::transcript::read_images::SeenPictures {
        self.seen_pictures.get_or_init(|| {
            crate::transcript::read_images::SeenPictures::new(self.messages, self.picture_digests)
        })
    }
}

/// The session's line-break rule, which the subagent viewer shares: a child runs the same agent.
pub fn line_breaks(state: &ChatState) -> AgentLineBreaks {
    AgentLineBreaks::for_agent(
        state.session.agent.as_deref(),
        state.session.session_agent_id.as_deref(),
    )
}

/// The session's own scope, off the composed list family a builds.
pub fn scope<'a>(state: &'a ChatState, view: &'a TranscriptViewState) -> ProjectionScope<'a> {
    // The answers are read here too, so each tag's link carries whether its question is answered
    // or skipped; answers still sending or queued count already.
    let mut sent = page_answers_in(&state.messages.composed);
    for pending in crate::questions::open_questions::answers_on_the_way(state) {
        if !sent.iter().any(|known| known.page_id == pending.page_id) {
            sent.push(pending);
        }
    }
    let answered: Vec<String> = sent.iter().map(|answers| answers.page_id.clone()).collect();
    let mut found = askers(&state.messages.composed, &answered);
    crate::transcript::open_questions::mark_skipped(&mut found, &sent);
    crate::questions::resource_index::settle_loaded_by(
        &mut found,
        state.session.resource_index.view(),
    );
    crate::transcript::open_questions::share_answers(&mut found);
    crate::transcript::open_questions::mark_inherited(
        &mut found,
        &state.messages.composed,
        state.session.forked_at,
    );
    ProjectionScope {
        messages: &state.messages.composed,
        working: is_working(state),
        summary: view.summary_mode,
        deferred: &view.deferred,
        agent_path: &view.agent_path,
        working_directory: view.working_directory.as_deref(),
        line_breaks: line_breaks(state),
        live_side_question: crate::transcript::side_question::live_side_question(state),
        approval_wait: crate::transcript::approval_wait::approval_wait(state),
        agent_fleet: state.session.agent_fleet.as_ref(),
        session_directory: &view.session_directory,
        question_tags: linkable_tags(&found),
        askers: found,
        agent_message_deliveries: state.session.agent_message_deliveries.as_ref(),
        backfilled: &view.backfilled,
        question_forms: std::cell::OnceCell::new(),
        tool_cards: view.effective_tool_cards(),
        grouped_work: view.grouped_work(),
        hidden_work: view.hidden_work(),
        hides_running: crate::session::working::hides_running_response(state),
        quiet_finished_before: crate::session::working::quiet_finished_before(state),
        picture_digests: &view.picture_digests,
        seen_pictures: std::cell::OnceCell::new(),
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
) -> Arc<Value> {
    Arc::clone(&cached_entry(cache, scope, context, message).model)
}

/// [`project_cached`]'s cache entry.
fn cached_entry<'c>(
    cache: &'c mut ProjectionCache,
    scope: &ProjectionScope<'_>,
    context: &ChatContext,
    message: &ChatMessage,
) -> &'c mut ProjectedMessage {
    let stale = cache
        .get(&message.id)
        .is_none_or(|entry| !fresh(entry, message, scope));
    if stale {
        cache.insert(message.id.clone(), project_entry(scope, context, message));
    }
    cache
        .get_mut(&message.id)
        .expect("an entry just checked or cached")
}

/// How a row draws a model.
#[derive(Clone, Copy, PartialEq)]
enum Draw {
    /// As projected.
    Row,
    /// Inside a turn's work fold, with the fold's marks (`list_tools_in_work_fold`).
    InWorkFold,
    /// Not drawn, only read for keys the work-fold marks never touch (`files`, `questions`,
    /// `sentMessages`, `workflows`): either form does.
    Read,
}

/// The cached model in the form `draw` asks for. A row turns the entry to its own form, so the rows
/// that draw it share the cached model until one changes it (`ProjectedMessage::in_work_fold`).
fn drawn_from(entry: &mut ProjectedMessage, draw: Draw) -> Arc<Value> {
    match draw {
        Draw::Row => {
            if let Some(marks) = entry.in_work_fold.take() {
                unmark_work_fold(Arc::make_mut(&mut entry.model), marks);
            }
        }
        Draw::InWorkFold => {
            if entry.in_work_fold.is_none() {
                entry.in_work_fold = Some(mark_work_fold(Arc::make_mut(&mut entry.model)));
            }
        }
        Draw::Read => {}
    }
    Arc::clone(&entry.model)
}

/// A model no cache entry holds (a placeholder, a row projected off the cache) in the form `draw`
/// asks for.
fn drawn(mut model: Arc<Value>, draw: Draw) -> Arc<Value> {
    if draw == Draw::InWorkFold {
        list_tools_in_work_fold(Arc::make_mut(&mut model));
    }
    model
}

/// One message's full projection with the facts its cache entry is judged by.
fn project_entry(
    scope: &ProjectionScope<'_>,
    context: &ChatContext,
    message: &ChatMessage,
) -> ProjectedMessage {
    // A background task's end is projected (and its details read) as the tool calls it reports;
    // an OMP job's call ran as long as OMP recorded, which no row's time says.
    let task_spans = crate::transcript::background_rows::background_task_spans(message);
    let arrived = message;
    let source = background_task_source(arrived);
    let message = &*source;
    // How long each call ran comes from the rows as they arrived, before the fold merged them.
    let mut spans = tool_spans(
        message,
        std::iter::once(scope.messages).chain(scope.deferred.values().map(Vec::as_slice)),
    );
    spans.extend(task_spans);
    let model = project_message(
        message,
        &scope.question_forms,
        scope.messages,
        scope.agent_path,
        scope.working_directory,
        scope.line_breaks,
        context,
        &spans,
        &scope.question_tags,
        scope.tool_cards,
        scope.working,
    );
    let words = if message.role == ChatRole::Assistant {
        tag_words(&prose_markdown(&message.blocks))
    } else {
        Vec::new()
    };
    ProjectedMessage {
        arrived: matches!(source, std::borrow::Cow::Owned(_)).then(|| arrived.clone()),
        unanswered: crate::transcript::tool_card::has_unanswered_call(&message.blocks),
        source: source.into_owned(),
        model: Arc::new(model),
        in_work_fold: None,
        line_breaks: scope.line_breaks,
        linked_tags: linked_words(&words, &scope.question_tags),
        tag_words: words,
    }
}

/// The cached model still stands: the same message, projected under the same line-break rule, and
/// the same of its words are question tags (a new question links words an older reply already had).
///
/// CDXC:SessionChat 2026-10-30 WHY: the message is compared as it arrived, not as the tool calls a background row is turned into. Turning an unchanged OMP job row again on every check copied and split each job's whole result just to find nothing had changed.
fn fresh(entry: &ProjectedMessage, message: &ChatMessage, scope: &ProjectionScope<'_>) -> bool {
    entry.arrived.as_ref().unwrap_or(&entry.source) == message
        && entry.line_breaks == scope.line_breaks
        && linked_words(&entry.tag_words, &scope.question_tags) == entry.linked_tags
        && crate::transcript::tool_card::card_fresh(&entry.model, entry.unanswered, scope.working)
}

/// `pruneModels`: once the cache is well past the transcript's size, drop every entry whose id is
/// no longer in the transcript or its deferred rows.
///
/// CDXC:SessionChat 2026-10-23 WHY: An entry the pass just drew from (`used`) is kept whatever its
/// id. A row the fold derives (a Codex `!` command's `:command` and `:output` halves, a side
/// question) has an id no transcript message carries, so once loaded work rows held the cache past
/// the bound, every pass dropped those rows again; with more than one batch of them outside the
/// eager tail, each batch re-queued what the previous prune had dropped and the zero-delay backfill
/// re-ran the whole projection for as long as the chat was retained.
pub(crate) fn prune_cache(
    cache: &mut ProjectionCache,
    scope: &ProjectionScope<'_>,
    used: &std::collections::HashSet<String>,
) {
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
    live.extend(used.iter().map(String::as_str));
    let stale: Vec<String> = cache
        .keys()
        .filter(|id| !live.contains(id.as_str()))
        .cloned()
        .collect();
    for id in stale {
        cache.remove(&id);
    }
}

pub(crate) struct Builder<'a> {
    pub(crate) scope: &'a ProjectionScope<'a>,
    context: &'a ChatContext,
    cache: &'a mut ProjectionCache,
    eager_from: usize,
    pub(crate) backfill: Vec<ChatMessage>,
    /// Every id this pass has drawn a row for, so a second, different message with the same id
    /// never evicts the first one's cache entry.
    pub(crate) used: std::collections::HashSet<String>,
    /// The ids the settled turns drew (`transcript/settled.rs`), which count as drawn already,
    /// except those in `own_used`: the ones these same rows drew when they were built before.
    prior_used: Option<&'a std::collections::HashSet<String>>,
    own_used: Option<&'a std::collections::HashSet<String>>,
    /// Where each queued id sits in `backfill`, so a message drawn several times (a completed
    /// turn's row feeds its files, questions, sent messages, workflows and its own row) is queued
    /// once.
    queued: std::collections::HashMap<String, usize>,
    /// The row number being built, and the last one that drew a placeholder.
    current: usize,
    pub(crate) last_queued: Option<usize>,
    /// Some row was projected outside the cache, which every pass does again.
    pub(crate) off_cache: bool,
}

impl<'a> Builder<'a> {
    pub(crate) fn new(
        scope: &'a ProjectionScope<'a>,
        context: &'a ChatContext,
        cache: &'a mut ProjectionCache,
        eager_from: usize,
        prior_used: Option<&'a std::collections::HashSet<String>>,
        own_used: Option<&'a std::collections::HashSet<String>>,
    ) -> Self {
        Self {
            scope,
            context,
            cache,
            eager_from,
            backfill: Vec::new(),
            used: std::collections::HashSet::new(),
            prior_used,
            own_used,
            queued: std::collections::HashMap::new(),
            current: 0,
            last_queued: None,
            off_cache: false,
        }
    }

    /// `projected`: the cached model when the cache holds this exact message.
    fn projected(&mut self, message: &ChatMessage, draw: Draw) -> Option<Arc<Value>> {
        let scope = self.scope;
        self.cache
            .get_mut(&message.id)
            .filter(|entry| fresh(entry, message, scope))
            .map(|entry| drawn_from(entry, draw))
    }

    /// The full projection when it is cheap or the row is near the tail; otherwise a stable
    /// plain-text stand-in queued for backfill.
    ///
    /// A row the cache cannot hold is projected on the spot rather than queued: a second message
    /// with an id this pass already drew (the cache keeps one entry per id, so the two would evict
    /// each other on every batch), or a row a batch of this run already projected and that is
    /// still out of date. Queuing either kept the zero-delay backfill re-running the whole
    /// projection for ever.
    fn message_or_placeholder(
        &mut self,
        message: &ChatMessage,
        eager: bool,
        draw: Draw,
    ) -> Arc<Value> {
        if let Some(model) = self.projected(message, draw) {
            self.used.insert(message.id.clone());
            return model;
        }
        if let Some(&at) = self.queued.get(&message.id) {
            if self.backfill[at] == *message {
                return drawn(Arc::new(placeholder(message)), draw);
            }
        }
        let drawn_before = self
            .prior_used
            .is_some_and(|used| used.contains(&message.id))
            && !self.own_used.is_some_and(|own| own.contains(&message.id));
        let first = self.used.insert(message.id.clone()) && !drawn_before;
        if !first || self.scope.backfilled.contains(&message.id) {
            self.off_cache = true;
            return drawn(project_entry(self.scope, self.context, message).model, draw);
        }
        if eager || self.holds_waiting_call(message) {
            let entry = cached_entry(self.cache, self.scope, self.context, message);
            return drawn_from(entry, draw);
        }
        self.queued.insert(message.id.clone(), self.backfill.len());
        self.backfill.push(message.clone());
        self.last_queued = Some(self.current);
        drawn(Arc::new(placeholder(message)), draw)
    }

    fn eager(&self, index: usize) -> bool {
        index >= self.eager_from
    }

    /// Agent work Reduced or Quiet: the row holding the call an approval names, which is drawn whole
    /// however far up the list it sits (`chat_look::needs_the_user`).
    fn holds_waiting_call(&self, message: &ChatMessage) -> bool {
        if !self.scope.grouped_work || self.scope.summary {
            return false;
        }
        let Some(id) = self
            .scope
            .approval_wait
            .as_ref()
            .and_then(|wait| wait.tool_use_id.as_deref())
        else {
            return false;
        };
        message.blocks.iter().any(
            |block| matches!(block, ChatBlock::ToolCall { call_id: Some(call), .. } if call == id),
        )
    }
}

pub(crate) fn message_id(item: &TranscriptItem) -> String {
    match item {
        TranscriptItem::Message { message } => message
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        TranscriptItem::Summary { id, .. }
        | TranscriptItem::CompletedWork { id, .. }
        | TranscriptItem::GroupedWork { id, .. } => id.clone(),
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
    let projection: TranscriptProjection = project_chat_transcript(
        scope.messages,
        scope.working,
        &[],
        scope.live_side_question.as_deref(),
        scope.grouped_work && !scope.summary,
    );
    let summary = scope.summary;
    let worked_turn = latest_worked_turn(&projection.items, scope.working);
    let running_thinking =
        crate::transcript::thinking_card::RunningThinking::new(scope.messages, scope.working);
    let length = if summary {
        projection.summary_turns.len()
    } else {
        projection.items.len()
    };
    let mut builder = Builder::new(
        scope,
        context,
        cache,
        length.saturating_sub(EAGER_TAIL_ITEMS),
        None,
        None,
    );
    // CDXC:SessionChat 2026-09-26 DECISION: User: when summary mode is enabled, don't collapse the last agent reply. The newest turn with a reply keeps it open (it stays open while the next prompt is working), and the reader can still fold it.
    let latest_reply = projection
        .summary_turns
        .iter()
        .rposition(|turn| turn.final_message.is_some());
    let mut items = build_items(
        &mut builder,
        &projection,
        &running_thinking,
        0,
        latest_reply,
    );

    drop_hidden_rows(&mut items, &projection.hidden_rows);
    resolve_recipients(
        &mut items,
        scope.agent_fleet,
        scope.session_directory,
        scope.agent_message_deliveries,
    );

    let Builder { backfill, used, .. } = builder;
    crate::transcript::approval_wait::open_waiting_call(
        &mut items,
        cache,
        scope.approval_wait.as_ref(),
    );
    hide_running_work(&mut items, scope, &projection);

    // `itemIndex`: the first row that draws a given id, which is what a dash jumps to.
    let item_index = item_index(&items, 0);
    let turns: Vec<_> = projection
        .summary_turns
        .iter()
        .map(|turn| (&turn.user, turn.final_message.as_ref()))
        .collect();
    let minimap = crate::extras::minimap::project_minimap_turns(&turns, &item_index);
    prune_cache(cache, scope, &used);
    let page_answers = page_answers_in(scope.messages);
    let answered_pages: Vec<String> = page_answers
        .iter()
        .map(|answers| answers.page_id.clone())
        .collect();
    let askers = scope
        .askers
        .iter()
        .cloned()
        .map(|mut asker| {
            asker.answered = answered_pages.contains(&asker.id) || asker.answered_in_words();
            asker
        })
        .collect();
    Projection {
        items,
        final_ids: projection.final_ids,
        answered_pages,
        question_forms: question_forms(scope.messages),
        page_answers,
        askers,
        backfill,
        minimap,
        worked_turn,
    }
}

/// `itemIndex` for `items`, which start at row `first` of the list: the first row that draws each
/// id.
pub(crate) fn item_index(items: &[TranscriptItem], first: usize) -> Vec<(String, usize)> {
    let mut item_index: Vec<(String, usize)> = Vec::with_capacity(items.len());
    let mut indexed: std::collections::HashSet<String> =
        std::collections::HashSet::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let id = message_id(item);
        if indexed.insert(id.clone()) {
            item_index.push((id, first + index));
        }
    }
    item_index
}

/// The rows for `projection`'s render items (or its summary turns), the first of which is row
/// `first` of the whole list; `latest_reply` is the whole list's newest summary turn with a reply.
pub(crate) fn build_items(
    builder: &mut Builder,
    projection: &TranscriptProjection,
    running_thinking: &crate::transcript::thinking_card::RunningThinking,
    first: usize,
    latest_reply: Option<usize>,
) -> Vec<TranscriptItem> {
    let mut items = turn_items(builder, projection, running_thinking, first, latest_reply);
    crate::transcript::read_images::bubble_read_images(&mut items, builder.scope.seen_pictures());
    // Where the work is grouped, a finished turn's messages and answers fold into its row
    // (`folded_cards.rs`); Quiet draws no row and Summary no turn rows, so they keep their cards.
    if (builder.scope.tool_cards || builder.scope.grouped_work)
        && !builder.scope.hidden_work
        && !builder.scope.summary
    {
        items = crate::transcript::folded_cards::fold_cards(items);
    }
    // A settled chunk starts at a prompt the user sent (`settled::opens_turn`), whose row comes
    // before its turn's fold, and a running turn's rows fold nothing, so no run of finished turns
    // spans two chunks: grouping each chunk's rows alone groups the whole list.
    if builder.scope.grouped_work && !builder.scope.summary {
        crate::transcript::grouped_work::calls_as_lines(&mut items);
        let hidden = builder.scope.hidden_work;
        items = crate::transcript::grouped_work::group_work(
            items,
            hidden.then_some(projection.final_ids.as_slice()),
        );
    }
    items
}

/// Agent work Quiet, while the agent works: the running response draws only what needs the user
/// (`grouped_work::hide_work`). Runs once the waiting call is marked, never inside `build_items`
/// (`chat_look::needs_the_user`).
pub(crate) fn hide_running_work(
    items: &mut Vec<TranscriptItem>,
    scope: &ProjectionScope,
    projection: &TranscriptProjection,
) {
    let live = hidden_live(scope, projection);
    if live.is_empty() {
        return;
    }
    if scope.hides_running {
        crate::transcript::grouped_work::hide_work(items, &live, scope.quiet_finished_before);
    } else {
        crate::transcript::grouped_work::hide_live_calls(items, &live);
    }
}

/// The rows [`hide_running_work`] judges: the running response's, while Quiet hides it (all of
/// it, or only its calls when a working signal doubts the agent is mid-response).
fn hidden_live<'p>(
    scope: &ProjectionScope,
    projection: &'p TranscriptProjection,
) -> std::collections::HashSet<&'p str> {
    if scope.hidden_work && scope.working {
        projection.live_ids.iter().map(String::as_str).collect()
    } else {
        Default::default()
    }
}

/// The rows for `projection`: its summary turns in the Summary view, else its render items.
///
/// CDXC:SessionChat 2026-11-09 WHY: the Summary view's running turn opens its "Active work" and its newest row's calls (the 2026-10-13 decision on `summary_mode_turns`), but under Agent work Reduced and Quiet the work stays shut there too, and Quiet leaves the running rows out as it does outside the Summary view (`grouped_work::hide_work`): Sebastian filed the Summary view ignoring them as a bug, and Quiet's own decision is that only the status line shows while the agent runs (`chat_look.rs`). Default keeps the 2026-10-13 behaviour.
fn turn_items(
    builder: &mut Builder,
    projection: &TranscriptProjection,
    running_thinking: &crate::transcript::thinking_card::RunningThinking,
    first: usize,
    latest_reply: Option<usize>,
) -> Vec<TranscriptItem> {
    let scope = builder.scope;
    let live_card = scope
        .tool_cards
        .then(|| crate::transcript::tool_card::live_card_id(scope.messages, scope.working))
        .flatten();
    if scope.summary {
        // Agent work Quiet reads the running turn's rows themselves (`grouped_work::hide_work`).
        let hidden_live = hidden_live(scope, projection);
        projection
            .summary_turns
            .iter()
            .enumerate()
            .map(|(index, turn)| {
                let index = first + index;
                builder.current = index;
                let eager = builder.eager(index);
                // Agent work Reduced and Quiet: the running turn's work starts shut, as their
                // lines do (`grouped_work::hide_work`).
                let opens_running = !scope.grouped_work;
                TranscriptItem::Summary {
                    id: turn.user.id.clone(),
                    user: builder.message_or_placeholder(&turn.user, eager, Draw::Row),
                    final_message: turn
                        .final_message
                        .as_ref()
                        .map(|message| builder.message_or_placeholder(message, eager, Draw::Row)),
                    earlier_replies: turn
                        .earlier_replies
                        .iter()
                        .map(|message| builder.message_or_placeholder(message, eager, Draw::Row))
                        .collect(),
                    active: turn.active,
                    work_open: opens_running && turn.active && turn.final_message.is_none(),
                    // The running row's tool calls start open, so the call that is running shows.
                    work: turn
                        .active_work
                        .iter()
                        .map(|message| {
                            let eager = eager || hidden_live.contains(message.id.as_str());
                            let mut row =
                                builder.message_or_placeholder(message, eager, Draw::InWorkFold);
                            running_thinking.mark(message, &mut row);
                            if opens_running
                                && turn.running_row.as_deref() == Some(message.id.as_str())
                                && row.is_object()
                            {
                                if let Value::Object(fields) = Arc::make_mut(&mut row) {
                                    fields.insert("toolsOpen".to_string(), Value::Bool(true));
                                }
                            }
                            row
                        })
                        .collect(),
                    outcome: turn
                        .outcome
                        .iter()
                        .map(|message| builder.message_or_placeholder(message, eager, Draw::Row))
                        .collect(),
                    latest_reply: latest_reply == Some(index),
                    cards: turn
                        .cards
                        .iter()
                        .map(|message| builder.message_or_placeholder(message, eager, Draw::Row))
                        .collect(),
                    read_images: Vec::new(),
                }
            })
            .collect()
    } else {
        // Agent work Quiet hides the running response's rows by what they hold, so it reads the
        // rows themselves: a stand-in would show their words and could hold a form.
        let hidden_live = hidden_live(scope, projection);
        projection
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                builder.current = first + index;
                let eager = builder.eager(first + index);
                let turn = match item {
                    RenderItem::Message(message) => {
                        let eager = eager || hidden_live.contains(message.id.as_str());
                        let mut row = builder.message_or_placeholder(message, eager, Draw::Row);
                        running_thinking.mark(message, &mut row);
                        crate::transcript::tool_card::mark_live_card(
                            live_card.as_deref(),
                            message,
                            &mut row,
                        );
                        return TranscriptItem::Message { message: row };
                    }
                    RenderItem::CompletedWork(turn) => turn,
                };
                // The section, read through this fold or the prompt's that owns it.
                let section = scope.deferred.get(&turn.user.id).or_else(|| {
                    turn.section
                        .as_ref()
                        .and_then(|section| scope.deferred.get(&section.start_id))
                });
                let work = completed_chat_work(turn, section.map(Vec::as_slice));
                let (visible_artifacts, collapsed_work) = partition_completed_work(&work, scope.grouped_work);
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
                        .message_or_placeholder(message, eager, Draw::Read)
                        .get("files")
                        .cloned()
                        .unwrap_or(Value::Null)
                    {
                        files.extend(rows);
                    }
                }
                let (edited, failed) = crate::transcript::simple::edited_and_failed_paths(
                    files.iter().filter_map(|file| {
                        let path = file.get("path").and_then(Value::as_str)?;
                        Some((path, file.get("failed") == Some(&Value::Bool(true))))
                    }),
                );
                let mut changed: Vec<String> = edited.iter().map(|path| path.to_string()).collect();
                let failed_files = failed.len();
                // The range's file paths stand in for rows still on the server. A range the
                // wake-ups after the prompt share holds their files too, so the prompt counts only
                // the files of its own rows.
                if let Some(deferred) = turn
                    .user
                    .deferred_work
                    .as_ref()
                    .filter(|_| turn.next_opener.is_none())
                {
                    for path in &deferred.file_paths {
                        if !changed.iter().any(|seen| seen == path) {
                            changed.push(path.clone());
                        }
                    }
                }
                let changed_files = changed.len();
                let tool_calls = if turn.user.deferred_work.is_some() && work.is_empty() {
                    // The rows are still on the server: only the files are known until it opens.
                    0
                } else {
                    let rows: Vec<&ChatMessage> =
                        work.iter().chain(turn.final_message.iter()).collect();
                    tool_call_count(&rows)
                };
                let stats = counted_work_stats(tool_calls, changed_files);
                let loaded =
                    section.map(|rows| crate::transcript::transcript::section_part(turn, rows));
                let (started_ms, ended_ms, extends_to_ms) = turn_times(turn, loaded);
                let deferred_value = turn
                    .section
                    .as_ref()
                    .and_then(|deferred| serde_json::to_value(deferred).ok());
                TranscriptItem::CompletedWork {
                    id: turn.user.id.clone(),
                    label: turn_worked_label(turn, loaded, &projection.items[index + 1..]),
                    stats,
                    files_card: crate::transcript::file_change_rows::file_change_card(&files),
                    files,
                    files_label: format!(
                        "{changed_files} {} changed",
                        if changed_files == 1 { "file" } else { "files" }
                    ),
                    simple_files_label: edit_label(changed_files, failed_files),
                    expandable: !collapsed_work.is_empty() || turn.section.is_some(),
                    deferred: deferred_value,
                    work: collapsed_work
                        .iter()
                        .map(|message| {
                            builder.message_or_placeholder(message, eager, Draw::InWorkFold)
                        })
                        .collect(),
                    /* The turn's answered question cards, hoisted out of the fold. */
                    questions: work
                        .iter()
                        .flat_map(|row| {
                            match builder
                                .message_or_placeholder(row, eager, Draw::Read)
                                .get("questions")
                                .cloned()
                            {
                                Some(Value::Array(rows)) => rows,
                                _ => Vec::new(),
                            }
                        })
                        .collect(),
                    /* Only the folded rows' cards: an artifact row draws its own. */
                    sent_messages: collapsed_work
                        .iter()
                        .flat_map(|row| {
                            match builder
                                .message_or_placeholder(row, eager, Draw::Read)
                                .get("sentMessages")
                                .cloned()
                            {
                                Some(Value::Array(cards)) => cards,
                                _ => Vec::new(),
                            }
                        })
                        .collect(),
                    /* The folded rows' "GBot added" cards, hoisted like the sent messages. */
                    gbot_cards: collapsed_work
                        .iter()
                        .flat_map(|row| {
                            match builder
                                .message_or_placeholder(row, eager, Draw::Read)
                                .get("gbotCards")
                                .cloned()
                            {
                                Some(Value::Array(cards)) => cards,
                                _ => Vec::new(),
                            }
                        })
                        .collect(),
                    /* The folded rows' workflow cards, hoisted like the sent messages. */
                    workflows: collapsed_work
                        .iter()
                        .flat_map(|row| {
                            match builder
                                .message_or_placeholder(row, eager, Draw::Read)
                                .get("workflows")
                                .cloned()
                            {
                                Some(Value::Array(ids)) => ids,
                                _ => Vec::new(),
                            }
                        })
                        .collect(),
                    artifacts: visible_artifacts
                        .iter()
                        .map(|message| builder.message_or_placeholder(message, eager, Draw::Row))
                        .collect(),
                    final_message: turn
                        .final_message
                        .as_ref()
                        .map(|message| builder.message_or_placeholder(message, eager, Draw::Row)),
                    read_images: Vec::new(),
                    cards_in_fold: false,
                    tally: Box::new(WorkTally {
                        started_ms,
                        ended_ms,
                        extends_to_ms,
                        tool_calls,
                        changed,
                        ..WorkTally::default()
                    }),
                }
            })
            .collect()
    }
}

/// Takes the rows that keep their place in the turns but draw nothing out of the list.
pub(crate) fn drop_hidden_rows(items: &mut Vec<TranscriptItem>, hidden: &[String]) {
    if hidden.is_empty() {
        return;
    }
    let shown = |row: &Arc<Value>| {
        !row.get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| hidden.iter().any(|hidden| hidden == id))
    };
    items.retain(|item| match item {
        TranscriptItem::Message { message } => shown(message),
        _ => true,
    });
    for item in items.iter_mut() {
        match item {
            TranscriptItem::Summary {
                work,
                outcome,
                earlier_replies,
                cards,
                ..
            } => {
                work.retain(shown);
                outcome.retain(shown);
                earlier_replies.retain(shown);
                cards.retain(shown);
            }
            TranscriptItem::CompletedWork {
                work, artifacts, ..
            } => {
                work.retain(shown);
                artifacts.retain(shown);
            }
            TranscriptItem::GroupedWork { work, turns, .. } => {
                work.retain(shown);
                for turn in turns {
                    turn.artifacts.retain(shown);
                }
            }
            _ => {}
        }
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
        return Some(serde_json::json!({
            "lines": change.lines,
            // How many of `lines` a row shows until "Show all" is pressed.
            "preview": crate::transcript::file_change_rows::file_change_preview_end(&change.lines),
        }));
    }
    tools.get(index).map(open_tool_detail)
}

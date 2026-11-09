//! Files and links: the composer's button and the card that lists every attachment, file and link
//! the session mentioned, so the user can go through them quickly.
//!
//! The button asks gxserver (`/api/readSessionFilesLinks`) for the list, which it gathers with the
//! transcript's own detection (`transcript/files_links.rs`) from the session's whole transcript,
//! and for a coordinator from each of its threads too. A menu floating above the composer (a sheet on
//! the phone) shows it in
//! three sections (Attachments, Files, Links) with a count each, grouped by thread when the list
//! spans several sessions, with a filter box once it is long. A row opens its item (a picture in
//! the image viewer, a file the way a transcript pill opens it, a link in the browser); its jump
//! buttons scroll the transcript to the message that first or last mentioned it, or open the
//! thread that did.
//!
//! CDXC:FilesAndLinks 2026-10-15 DECISION:
//! User (2026-10-15): "move files & links inside the 3 dots pls and make it open a pop up menu not push the chat transcript please", and a hotkey for it (`toggleChatFilesLinks`). This supersedes the toolbar button and the card above the composer of 2026-10-12: the entry is a More actions row, and `open` now drives a floating menu (apps/desktop/src/app/native_chat/actions.rs and files_links.rs; the phone's sheet). The rest stands. User (2026-10-12): "i want to add a feature to see all the attachements and links that were mentioned in a session (and for coordinator threads should be that session and it's own sub sessions) should be a button, when i click it then it should show me a list that shows these files locations and links so i can go through them quick". The list shows Attachments, Files and Links, each item once with its first and last mention, and a coordinator's list covers its own chat and every thread, done ones included, grouped by thread title.
//! CDXC:FilesAndLinks 2026-10-12 WHY:
//! A row's jump finds the message among the rows the transcript has loaded, and a message folded into a finished turn lands on that turn. One older than the loaded history (a coordinator's First is often days back) pages the history in the way "Load earlier" does, one page per settle and at most `MAX_JUMP_PAGES`, then scrolls; a note says so only when it still cannot be reached.
//! CDXC:FilesAndLinks 2026-10-12 WHY:
//! A coordinator's list runs to thousands of rows (235 threads held 6,506 items), and a plain 40-row cap per section was filled by the coordinator's own chat so no thread showed. The first view gives each session's group its 5 newest rows, and "Show more" then adds 100 rows a click, so the threads are visible at once and the card never draws thousands of rows.
//! SEE-ALSO: server/src/session_files_links.rs (the list), apps/desktop/src/app/native_chat/files_links.rs and apps/mobile/app/src/chat/native/cards/FilesLinksCard.tsx (drawing).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::document::TranscriptItem;
use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::transcript::files_links::{FilesLinksItem, FilesLinksMention, FilesLinksSection};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// Rows a section shows before its "Show more".
const SECTION_ROWS: usize = 40;
/// Rows each "Show more" adds, so a coordinator's thousands of files never draw at once.
const PAGE_ROWS: usize = 100;
/// Rows each session's group shows at first when the list spans several sessions, so the threads
/// are listed under their titles instead of the coordinator's own chat filling the section.
const GROUP_ROWS: usize = 5;
/// History pages a jump reads at most before it gives up on reaching its message.
const MAX_JUMP_PAGES: u32 = 60;
/// The filter box shows once the list holds this many items.
const FILTER_FROM: usize = 10;

/// One session's part of the list, as gxserver sent it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesLinksSession {
    pub project_id: String,
    pub session_id: String,
    #[serde(default)]
    pub title: String,
    /// The session the chat shows, whose rows jump in place.
    #[serde(default)]
    pub own: bool,
    #[serde(default)]
    pub items: Vec<FilesLinksItem>,
}

/// Where the transcript should scroll, once per click.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesLinksJump {
    pub item: usize,
    pub revision: u64,
}

/// What the card remembers between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FilesLinksState {
    pub open: bool,
    /// The `readSessionFilesLinks` call in flight; an answer to any other id is dropped.
    pub request: Option<u64>,
    pub sessions: Option<Vec<FilesLinksSession>>,
    pub error: Option<String>,
    pub query: String,
    /// The "Show more" pages each section has opened.
    pub pages: Vec<(FilesLinksSection, usize)>,
    pub jump: Option<FilesLinksJump>,
    /// A jump still paging the history in.
    pub pending_jump: Option<PendingJump>,
}

/// A jump whose message is older than the loaded rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingJump {
    pub message_id: String,
    pub timestamp: Option<i64>,
    /// History pages read for it so far.
    pub pages: u32,
}

/// One row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesLinksRow {
    pub key: String,
    pub label: String,
    pub detail: String,
    /// `photo`, `file`, `folder` or `link`.
    pub icon: String,
    /// "Attached", "New", "Edited" or "Deleted".
    pub tag: Option<String>,
    /// "3 mentions · first 10:02 · last 14:30".
    pub meta: String,
    /// The action a click runs; a picture opens `image` in the viewer instead.
    pub open: Value,
    pub image: Option<Value>,
    /// "First" and "Last", or one "Show", or "Open thread": `{label, tooltip, command}` each.
    pub jumps: Vec<Value>,
    /// The file's or folder's path, for Copy Path and the folder row; absent for a web link.
    /// CDXC:SessionChat 2026-10-17 SEE-ALSO: `file_actions.rs` in apps/desktop/src/app/native_chat/ holds the decision; the phone's `FilesLinksCard.tsx` reads the same field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// One session's rows inside a section.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesLinksGroup {
    pub key: String,
    /// The thread's title; empty when the list covers one session.
    pub title: String,
    pub rows: Vec<FilesLinksRow>,
}

/// One section.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesLinksSectionView {
    /// `attachments`, `files` or `links`.
    pub id: String,
    pub label: String,
    pub count: usize,
    pub groups: Vec<FilesLinksGroup>,
    /// "Show all 120" while capped, "Show fewer" while shown whole, else empty.
    pub more_label: String,
}

/// The document's `filesLinks`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesLinks {
    /// The session has a conversation to list.
    pub enabled: bool,
    pub open: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub query: String,
    /// The list is long enough for the filter box.
    pub filter: bool,
    pub sections: Vec<FilesLinksSectionView>,
    /// "4 threads · 212 items"; empty while there is no list.
    pub caption: String,
    /// The quiet line when nothing is listed.
    pub empty_label: String,
    pub jump: Option<FilesLinksJump>,
}

/// The button: closes an open card, or opens it and reads the list.
pub fn toggle(state: &mut ChatState) -> Vec<Effect> {
    if state.composer.files_links.open {
        return close(state);
    }
    if !available(state) {
        return Vec::new();
    }
    run(state)
}

/// Refresh: reads the list again with the card open.
pub fn run(state: &mut ChatState) -> Vec<Effect> {
    if !available(state) {
        return Vec::new();
    }
    let files_links = &mut state.composer.files_links;
    files_links.open = true;
    files_links.error = None;
    state.core.request_publish();
    let request_id = state.core.allocate_request_id();
    state.composer.files_links.request = Some(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadSessionFilesLinks,
        params: Box::new(json!({})),
    }]
}

/// Close: the card goes, and an answer still on its way is dropped.
pub fn close(state: &mut ChatState) -> Vec<Effect> {
    let files_links = &mut state.composer.files_links;
    files_links.open = false;
    files_links.request = None;
    files_links.query.clear();
    files_links.pages.clear();
    files_links.jump = None;
    files_links.pending_jump = None;
    Vec::new()
}

pub fn set_query(state: &mut ChatState, query: &str) -> Vec<Effect> {
    state.composer.files_links.query = query.to_string();
    state.composer.files_links.pages.clear();
    Vec::new()
}

/// A section's "Show more" adds a page of rows; once every row shows, "Show fewer" goes back to the
/// first view.
pub fn toggle_section(state: &mut ChatState, section: &str) -> Vec<Effect> {
    let Some(section) = section_of(section) else {
        return Vec::new();
    };
    let files_links = &mut state.composer.files_links;
    let needle = files_links.query.trim().to_lowercase();
    let several = files_links
        .sessions
        .as_ref()
        .is_some_and(|sessions| sessions.len() > 1);
    let count = files_links
        .sessions
        .iter()
        .flatten()
        .map(|session| {
            session
                .items
                .iter()
                .filter(|item| item.section == section && matches(item, session, &needle, several))
                .count()
        })
        .sum();
    let pages = page_of(&files_links.pages, section);
    files_links.pages.retain(|(known, _)| *known != section);
    // The first click always opens a page: the first view may hide rows per thread even when the
    // section is short.
    if pages == 0 || budget(pages) < count {
        files_links.pages.push((section, pages + 1));
    }
    Vec::new()
}

fn page_of(pages: &[(FilesLinksSection, usize)], section: FilesLinksSection) -> usize {
    pages
        .iter()
        .find(|(known, _)| *known == section)
        .map_or(0, |(_, pages)| *pages)
}

/// The rows a section shows after `pages` "Show more" clicks.
fn budget(pages: usize) -> usize {
    SECTION_ROWS + pages * PAGE_ROWS
}

/// The filter: the item's name or path, or (across several sessions) its thread's title.
fn matches(
    item: &FilesLinksItem,
    session: &FilesLinksSession,
    needle: &str,
    several: bool,
) -> bool {
    needle.is_empty()
        || item.label.to_lowercase().contains(needle)
        || item.detail.to_lowercase().contains(needle)
        || (several && session.title.to_lowercase().contains(needle))
}

/// A row's jump: scrolls the transcript to the message, or to the turn that holds it.
pub fn jump(
    state: &mut ChatState,
    context: &ChatContext,
    message_id: &str,
    timestamp: Option<i64>,
) -> Vec<Effect> {
    state.composer.files_links.pending_jump = Some(PendingJump {
        message_id: message_id.to_string(),
        timestamp,
        pages: 0,
    });
    advance_jump(state, context)
}

/// Pages the history in until the jump's message is among the loaded rows, then scrolls to it.
/// Runs on the click and again on every settle while a page read is on its way.
pub fn advance_jump(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let Some(pending) = state.composer.files_links.pending_jump.clone() else {
        return Vec::new();
    };
    if state.messages.loading_earlier || state.messages.load_earlier_request.is_some() {
        return Vec::new();
    }
    let rows = crate::transcript::rows(state, context);
    let found = row_of(&rows, &pending.message_id, pending.timestamp);
    if found.is_none() && state.messages.has_more && pending.pages < MAX_JUMP_PAGES {
        let effects = crate::session::actions::load_earlier(state, context);
        if !effects.is_empty() {
            if let Some(pending) = state.composer.files_links.pending_jump.as_mut() {
                pending.pages += 1;
            }
            return effects;
        }
    }
    state.composer.files_links.pending_jump = None;
    if rows.is_empty() {
        return Vec::new();
    }
    let revision = state
        .composer
        .files_links
        .jump
        .map_or(1, |jump| jump.revision + 1);
    state.composer.files_links.jump = Some(FilesLinksJump {
        item: found.unwrap_or(0),
        revision,
    });
    state.core.request_publish();
    if found.is_none() {
        return vec![Effect::Toast {
            level: "info".to_string(),
            message: "Could not reach that message in the loaded history.".to_string(),
        }];
    }
    Vec::new()
}

/// The row that draws the message, else the last row that started before it.
pub(crate) fn row_of(
    rows: &[TranscriptItem],
    message_id: &str,
    timestamp: Option<i64>,
) -> Option<usize> {
    let id_of = |message: &Value| {
        message
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let messages_of = |item: &TranscriptItem| -> Vec<crate::document::ProjectedMessage> {
        match item {
            TranscriptItem::Message { message } => vec![message.clone()],
            TranscriptItem::Summary {
                user,
                final_message,
                earlier_replies,
                work,
                outcome,
                cards,
                ..
            } => std::iter::once(user)
                .chain(earlier_replies)
                .chain(work)
                .chain(outcome)
                .chain(cards)
                .chain(final_message)
                .cloned()
                .collect(),
            TranscriptItem::CompletedWork {
                work,
                artifacts,
                final_message,
                ..
            } => work
                .iter()
                .chain(artifacts)
                .chain(final_message)
                .cloned()
                .collect(),
            TranscriptItem::GroupedWork { work, turns, .. } => work
                .iter()
                .chain(
                    turns
                        .iter()
                        .flat_map(|turn| turn.artifacts.iter().chain(&turn.final_message)),
                )
                .cloned()
                .collect(),
            TranscriptItem::Unknown(_) => Vec::new(),
        }
    };
    let mut before = None;
    for (index, item) in rows.iter().enumerate() {
        let item_id = match item {
            TranscriptItem::Summary { id, .. } | TranscriptItem::CompletedWork { id, .. } => {
                Some(id.clone())
            }
            _ => None,
        };
        // A Reduced line stands for each turn it merged.
        let merges_turn = matches!(item, TranscriptItem::GroupedWork { turns, .. }
            if turns.iter().any(|turn| turn.id == message_id));
        let messages = messages_of(item);
        if item_id.as_deref() == Some(message_id)
            || merges_turn
            || messages
                .iter()
                .any(|message| id_of(message).as_deref() == Some(message_id))
        {
            return Some(index);
        }
        let started = messages
            .iter()
            .filter_map(|message| message.get("timestamp").and_then(Value::as_i64))
            .min();
        if let (Some(started), Some(timestamp)) = (started, timestamp) {
            if started <= timestamp {
                before = Some(index);
            }
        }
    }
    before
}

/// The session has a conversation to read.
fn available(state: &ChatState) -> bool {
    state.session.agent_session_id.is_some()
}

fn section_of(id: &str) -> Option<FilesLinksSection> {
    match id {
        "attachments" => Some(FilesLinksSection::Attachments),
        "files" => Some(FilesLinksSection::Files),
        "links" => Some(FilesLinksSection::Links),
        _ => None,
    }
}

/// The list's answer: it fills the card, and a refusal is the card's, never the composer's error
/// line.
pub fn settle(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) -> Option<Vec<Effect>> {
    if state.composer.files_links.request != Some(request_id) {
        return None;
    }
    state.core.claim_refusal();
    let files_links = &mut state.composer.files_links;
    files_links.request = None;
    match outcome {
        RpcOutcome::Ok { result } => {
            let sessions = result
                .get("sessions")
                .cloned()
                .and_then(|sessions| {
                    serde_json::from_value::<Vec<FilesLinksSession>>(sessions).ok()
                })
                .unwrap_or_default();
            files_links.sessions = Some(sessions);
        }
        // An older gxserver has no `readSessionFilesLinks` and refuses it without a sentence.
        RpcOutcome::Err { message, .. } if !message.chars().any(char::is_alphabetic) => {
            files_links.error = Some(
                "Ghostex on this computer is too old for Files and links. Update it and try again."
                    .to_string(),
            );
        }
        RpcOutcome::Err { message, .. } => files_links.error = Some(message.clone()),
    }
    state.core.request_publish();
    Some(Vec::new())
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn time_label(mention: &FilesLinksMention, context: &ChatContext) -> Option<String> {
    crate::transcript::message_time::message_time(mention.timestamp, context)
        .get("label")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn mention_jump(
    session: &FilesLinksSession,
    mention: &FilesLinksMention,
    label: &str,
    tooltip: &str,
) -> Value {
    json!({
        "label": label,
        "tooltip": tooltip,
        "command": {
            "type": "filesLinksJump",
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "messageId": mention.message_id,
            "timestamp": mention.timestamp,
        },
    })
}

fn row(item: &FilesLinksItem, session: &FilesLinksSession, context: &ChatContext) -> FilesLinksRow {
    let first = time_label(&item.first, context);
    let last = time_label(&item.last, context);
    let meta = match (item.mentions, first, last) {
        (1, Some(at), _) => format!("Mentioned {at}"),
        (1, None, _) => "Mentioned once".to_string(),
        (count, Some(first), Some(last)) if first != last => {
            format!("{count} mentions · first {first} · last {last}")
        }
        (count, _, Some(last)) => format!("{count} mentions · {last}"),
        (count, _, None) => format!("{count} mentions"),
    };
    let jumps = if !session.own {
        vec![json!({
            "label": "Open thread",
            "tooltip": format!("Open {} to read where it was mentioned", session.title),
            "command": {
                "type": "openCoordinatorThread",
                "projectId": session.project_id,
                "sessionId": session.session_id,
            },
        })]
    } else if item.mentions > 1 && item.first.message_id != item.last.message_id {
        vec![
            mention_jump(
                session,
                &item.first,
                "First",
                "Scroll to the message that first mentioned it",
            ),
            mention_jump(
                session,
                &item.last,
                "Last",
                "Scroll to the message that last mentioned it",
            ),
        ]
    } else {
        vec![mention_jump(
            session,
            &item.last,
            "Show",
            "Scroll to the message that mentioned it",
        )]
    };
    let href = item
        .url
        .clone()
        .or_else(|| item.path.clone())
        .unwrap_or_default();
    FilesLinksRow {
        key: format!("{}:{}:{}", session.project_id, session.session_id, item.key),
        label: item.label.clone(),
        detail: item.detail.clone(),
        icon: match item.kind.as_str() {
            "image" => "photo",
            "folder" => "folder",
            "url" => "link",
            _ => "file",
        }
        .to_string(),
        tag: match item.change.as_deref() {
            Some("new") => Some("New".to_string()),
            Some("edited") => Some("Edited".to_string()),
            Some("deleted") => Some("Deleted".to_string()),
            _ if item.attached => Some("Attached".to_string()),
            _ => None,
        },
        meta,
        open: json!({"type": "openMarkdownLink", "href": href}),
        image: item.image.clone(),
        jumps,
        path: item
            .url
            .is_none()
            .then(|| item.path.clone())
            .flatten()
            .filter(|path| !path.is_empty()),
    }
}

/// The document's `filesLinks`.
pub fn document(state: &ChatState, context: &ChatContext) -> FilesLinks {
    let files_links = &state.composer.files_links;
    let enabled = available(state);
    let open = enabled && files_links.open;
    let mut document = FilesLinks {
        enabled,
        open,
        loading: open && files_links.request.is_some(),
        error: files_links.error.clone().filter(|_| open),
        query: files_links.query.clone(),
        jump: files_links.jump.filter(|_| open),
        ..FilesLinks::default()
    };
    let Some(sessions) = files_links.sessions.as_ref().filter(|_| open) else {
        return document;
    };
    let total: usize = sessions.iter().map(|session| session.items.len()).sum();
    let several = sessions.len() > 1;
    let needle = files_links.query.trim().to_lowercase();
    let filtering = !needle.is_empty();
    for (section, id, label) in [
        (FilesLinksSection::Attachments, "attachments", "Attachments"),
        (FilesLinksSection::Files, "files", "Files"),
        (FilesLinksSection::Links, "links", "Links"),
    ] {
        let pages = page_of(&files_links.pages, section);
        let limit = budget(pages);
        // The first view of a list across several sessions gives each its few newest rows.
        let group_cap = if several && !filtering && pages == 0 {
            GROUP_ROWS
        } else {
            usize::MAX
        };
        let mut count = 0usize;
        let mut shown = 0usize;
        let mut groups = Vec::new();
        for session in sessions {
            let mut rows = Vec::new();
            for item in session
                .items
                .iter()
                .filter(|item| item.section == section && matches(item, session, &needle, several))
            {
                count += 1;
                if shown < limit && rows.len() < group_cap {
                    shown += 1;
                    rows.push(row(item, session, context));
                }
            }
            if !rows.is_empty() {
                groups.push(FilesLinksGroup {
                    key: format!("{}:{}", session.project_id, session.session_id),
                    title: if !several {
                        String::new()
                    } else if session.own {
                        format!("{} (this chat)", session.title)
                    } else {
                        session.title.clone()
                    },
                    rows,
                });
            }
        }
        if count == 0 {
            continue;
        }
        let more_label = if shown < count {
            format!("Show more ({shown} of {count} shown)")
        } else if pages > 0 {
            "Show fewer".to_string()
        } else {
            String::new()
        };
        document.sections.push(FilesLinksSectionView {
            id: id.to_string(),
            label: label.to_string(),
            count,
            groups,
            more_label,
        });
    }
    document.filter = total >= FILTER_FROM;
    document.caption = if several {
        format!(
            "{} · {}",
            plural(sessions.len(), "session", "sessions"),
            plural(total, "item", "items")
        )
    } else {
        plural(total, "item", "items")
    };
    if document.sections.is_empty() {
        document.empty_label = if filtering {
            format!(
                "Nothing matches \u{201c}{}\u{201d}.",
                files_links.query.trim()
            )
        } else if several {
            "No attachments, files or links mentioned in this chat or its threads yet.".to_string()
        } else {
            "No attachments, files or links mentioned in this chat yet.".to_string()
        };
    }
    document
}

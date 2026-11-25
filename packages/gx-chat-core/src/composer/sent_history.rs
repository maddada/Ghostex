//! History: the pill above the chat box that lists every message the user sent in this chat as
//! cards, with a search field and filters, each card jumping to its message or copying, reusing or
//! referencing it.
//!
//! gxserver reads the list from the session's whole transcript (`/api/readSessionChatSent`,
//! server/src/session_chat_sent.rs), fifty cards a page from the newest back, with the search and
//! the filter applied there, so a chat of hundreds of messages is never read whole into the chat
//! and a message older than the loaded rows is listed too. Which rows count and what a card holds
//! are decided here ([`card`], [`matches`]) for gxserver and every client alike.
//!
//! CDXC:SessionChat 2026-11-26 DECISION:
//! User: "i need a "history" pill that just shows a history icon above the chat. when i click on it
//! i wanna see all my prev sent messages in a chat in cards. pls put this on the left most of the
//! pills but before the scroll to bottom pill". From the mockup page the user picked: the pill
//! right of the scroll to bottom arrow, so the passing arrow never moves it (Q4980 B); a popover
//! above the chat box like the other pill menus (Q4981 A); a card shows the message's text, its
//! pictures and files and its time, not its M number (Q4982); a card offers Jump to it, Copy, Put
//! back in the chat box, Copy reference, Reference in chat, Fork from here and Rewind to here
//! (Q4983); oldest first, opening at the newest (Q4984); a search field plus filters (Q4985); the
//! same pill on the phone (Q4986). Defaults that stood: the pill shows once the chat has a sent
//! message; messages from before message numbers (Nov 20) are listed without a number; the list
//! pages fifty at a time from gxserver.
//! SEE-ALSO: server/src/session_chat_sent.rs (the read), apps/desktop/src/app/native_chat/sent_history.rs
//! and apps/mobile/app/src/chat/native/cards/SentHistorySheet.tsx (drawing).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ghostex_gx_protocol::{ChatBlock, ChatMessage};

use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::transcript::message_numbers::{
    copied_message_link, is_own_prompt, link_labels, message_link_named, shown_text, MessageRef,
};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// Cards one read brings: the first view, and each older page as the list scrolls to its top.
pub const PAGE_CARDS: usize = 50;
/// The most characters of a message a card carries for its preview (the card cuts it to three
/// lines); Copy and Put back take the whole text.
const PREVIEW_CHARS: usize = 600;
/// History pages a jump reads at most before it gives up on reaching its message.
const MAX_JUMP_PAGES: u32 = 60;

/// The filters beside the search field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SentFilter {
    #[default]
    All,
    /// Messages that carried a picture or a file.
    Attachments,
    /// Messages of words alone.
    Text,
}

impl SentFilter {
    pub fn parse(value: &str) -> Self {
        match value {
            "attachments" => Self::Attachments,
            "text" => Self::Text,
            _ => Self::All,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Attachments => "attachments",
            Self::Text => "text",
        }
    }
}

/// One picture a sent message carried, as its transcript block names it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentImage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
}

/// One sent message, as gxserver lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentCard {
    /// The transcript row's id, which Jump, Rewind and Fork name.
    pub id: String,
    /// Epoch ms, when the transcript recorded it.
    #[serde(default)]
    pub timestamp: Option<i64>,
    /// What the user wrote, as the chat shows it (`shown_text`).
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub images: Vec<SentImage>,
    /// The files it mentioned: those gxserver found on disk (`mentionPaths`), then the file pills
    /// in its words (an `@` mention is `[README.md #1](README.md)`), each once.
    #[serde(default)]
    pub files: Vec<String>,
    /// Its number (12 for M12); absent on messages sent before message numbers existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    /// `G9jt7.M12`, present exactly when `number` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_ref: Option<String>,
}

/// The card for `message` when it is one the user sent: the rows that get a message number
/// (`is_own_prompt`), whether or not they have one.
pub fn card(message: &ChatMessage) -> Option<SentCard> {
    if !is_own_prompt(message) {
        return None;
    }
    let images = message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::ImageRef { path, url, alt } => Some(SentImage {
                path: path.clone(),
                url: url.clone(),
                alt: alt.clone(),
            }),
            _ => None,
        })
        .collect::<Vec<SentImage>>();
    let text = shown_text(message).trim().to_string();
    let mut files = message.mention_paths.clone().unwrap_or_default();
    // A picture's own marker links its file too; the picture already shows as its thumbnail.
    for reference in crate::transcript::markdown_links::markdown_references(&text) {
        let Some(path) = reference["path"].as_str() else {
            continue;
        };
        let picture = images
            .iter()
            .any(|image| image.path.as_deref() == Some(path));
        if !picture && !files.iter().any(|file| file == path) {
            files.push(path.to_string());
        }
    }
    Some(SentCard {
        id: message.id.clone(),
        timestamp: message.timestamp,
        text,
        images,
        files,
        number: message.message_number,
        message_ref: message.message_ref.clone(),
    })
}

/// Whether `card` passes the search and the filter. The search reads like the `#M` picker's:
/// digits (with or without the M) find the message numbers they start, words find the text.
pub fn matches(card: &SentCard, query: &str, filter: SentFilter) -> bool {
    let attached = !card.images.is_empty() || !card.files.is_empty();
    let kept = match filter {
        SentFilter::All => true,
        SentFilter::Attachments => attached,
        SentFilter::Text => !attached,
    };
    if !kept {
        return false;
    }
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let digits = query.strip_prefix(['M', 'm']).unwrap_or(query);
    if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        if card
            .number
            .is_some_and(|number| number.to_string().starts_with(digits))
        {
            return true;
        }
    }
    let text = card.text.to_lowercase();
    let files = card.files.join("\n").to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| text.contains(word) || files.contains(word))
}

/// What the list remembers between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SentHistoryState {
    pub open: bool,
    /// The read in flight; an answer to any other id is dropped.
    pub request: Option<u64>,
    /// The read in flight adds an older page above the cards rather than replacing them.
    pub appending: bool,
    /// Oldest first.
    pub cards: Vec<SentCard>,
    /// Where the next older page ends; `None` once the list reached the chat's first message.
    pub before: Option<u64>,
    pub has_more: bool,
    /// A read has answered since the list opened or its search changed.
    pub loaded: bool,
    pub error: Option<String>,
    pub query: String,
    pub filter: SentFilter,
    pub jump: Option<SentJump>,
    pub pending_jump: Option<PendingSentJump>,
}

/// Where the transcript should scroll, once per click.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentJump {
    pub item: usize,
    pub revision: u64,
}

/// A jump whose message is older than the loaded rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingSentJump {
    pub message_id: String,
    pub timestamp: Option<i64>,
    pub pages: u32,
}

/// A filter beside the search field.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentFilterView {
    pub id: String,
    pub label: String,
    pub active: bool,
}

/// A file a card names.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentFile {
    pub label: String,
    pub path: String,
}

/// One card as the renderers draw it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentItem {
    pub id: String,
    pub timestamp: Option<i64>,
    /// The message's words with its links read as their labels, cut long; the card shows three
    /// lines of it.
    pub preview: String,
    /// `{label, title}` (`message_time`).
    pub time: Value,
    /// Each picture's image source (`transcript::images::image_source`).
    pub images: Vec<Value>,
    pub files: Vec<SentFile>,
    /// Copy's text: the whole message.
    pub copy_text: String,
    /// Put back's text: the message with its pictures as `[Image #N](path)`, as Up-arrow recall
    /// puts a sent message back (`composer/history.rs`).
    pub draft: String,
    /// Copy reference's link, on a numbered message (`message_numbers.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_link: Option<String>,
    /// Reference in chat's pill, on a numbered message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_pill: Option<String>,
}

/// The document's `sentHistory`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentHistory {
    /// The chat has a message the user sent, so the pill shows.
    pub enabled: bool,
    pub open: bool,
    /// The first read (or one after the search changed) is on its way.
    pub loading: bool,
    /// An older page is on its way.
    pub loading_more: bool,
    pub error: Option<String>,
    pub query: String,
    pub filters: Vec<SentFilterView>,
    /// Oldest first; the list opens at the newest, at its bottom.
    pub items: Vec<SentItem>,
    /// Older messages are left to read, asked for as the list reaches its top.
    pub has_more: bool,
    /// "50 sent", "120+ sent", "3 found".
    pub caption: String,
    /// The quiet line when nothing is listed.
    pub empty_label: String,
    pub jump: Option<SentJump>,
}

/// The pill: closes an open list, or opens it and reads the newest page.
pub fn toggle(state: &mut ChatState) -> Vec<Effect> {
    if state.composer.sent_history.open {
        return close(state);
    }
    let history = &mut state.composer.sent_history;
    history.open = true;
    history.query.clear();
    history.filter = SentFilter::All;
    read(state, false)
}

/// The list went away; an answer still on its way is dropped, a jump it started goes on.
pub fn close(state: &mut ChatState) -> Vec<Effect> {
    let history = &mut state.composer.sent_history;
    history.open = false;
    history.request = None;
    history.appending = false;
    history.cards.clear();
    history.before = None;
    history.has_more = false;
    history.loaded = false;
    history.error = None;
    history.query.clear();
    history.filter = SentFilter::All;
    state.core.request_publish();
    Vec::new()
}

/// The search field changed: the list reads again from the newest.
pub fn set_query(state: &mut ChatState, query: &str) -> Vec<Effect> {
    if !state.composer.sent_history.open || state.composer.sent_history.query == query {
        return Vec::new();
    }
    state.composer.sent_history.query = query.to_string();
    read(state, false)
}

/// A filter was picked: the list reads again from the newest.
pub fn set_filter(state: &mut ChatState, filter: &str) -> Vec<Effect> {
    let filter = SentFilter::parse(filter);
    if !state.composer.sent_history.open || state.composer.sent_history.filter == filter {
        return Vec::new();
    }
    state.composer.sent_history.filter = filter;
    read(state, false)
}

/// The list reached its top: the next older page.
pub fn more(state: &mut ChatState) -> Vec<Effect> {
    let history = &state.composer.sent_history;
    if !history.open || history.request.is_some() || !history.has_more || history.before.is_none()
    {
        return Vec::new();
    }
    read(state, true)
}

fn read(state: &mut ChatState, appending: bool) -> Vec<Effect> {
    let request_id = state.core.allocate_request_id();
    let history = &mut state.composer.sent_history;
    history.request = Some(request_id);
    history.appending = appending;
    history.error = None;
    if !appending {
        history.loaded = false;
    }
    let mut params = json!({
        "query": history.query.trim(),
        "filter": history.filter.wire(),
        "limit": PAGE_CARDS,
    });
    if appending {
        if let Some(before) = history.before {
            params["before"] = json!(before);
        }
    }
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadSessionChatSent,
        params: Box::new(params),
    }]
}

/// The read's answer: a first page replaces the cards, an older one goes above them. A refusal is
/// the list's, never the composer's error line.
pub fn settle(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) -> Option<Vec<Effect>> {
    if state.composer.sent_history.request != Some(request_id) {
        return None;
    }
    state.core.claim_refusal();
    let history = &mut state.composer.sent_history;
    history.request = None;
    let appending = std::mem::take(&mut history.appending);
    match outcome {
        RpcOutcome::Ok { result } => {
            let cards: Vec<SentCard> = result
                .get("messages")
                .cloned()
                .and_then(|messages| serde_json::from_value(messages).ok())
                .unwrap_or_default();
            if appending {
                let known: std::collections::HashSet<String> =
                    history.cards.iter().map(|card| card.id.clone()).collect();
                let mut older: Vec<SentCard> = cards
                    .into_iter()
                    .filter(|card| !known.contains(&card.id))
                    .collect();
                older.append(&mut history.cards);
                history.cards = older;
            } else {
                history.cards = cards;
            }
            history.has_more = result.get("hasMore").and_then(Value::as_bool) == Some(true);
            history.before = result.get("before").and_then(Value::as_u64);
            history.loaded = true;
        }
        // An older gxserver has no `readSessionChatSent` and refuses it without a sentence.
        RpcOutcome::Err { message, .. } if !message.chars().any(char::is_alphabetic) => {
            history.error = Some(
                "Ghostex on this computer is too old for History. Update it and try again."
                    .to_string(),
            );
            history.loaded = true;
        }
        RpcOutcome::Err { message, .. } => {
            history.error = Some(message.clone());
            history.loaded = true;
        }
    }
    state.core.request_publish();
    Some(Vec::new())
}

/// A card's Jump: scrolls the transcript to the message, paging older history in first when it is
/// not loaded yet.
pub fn jump(
    state: &mut ChatState,
    context: &ChatContext,
    message_id: &str,
    timestamp: Option<i64>,
) -> Vec<Effect> {
    state.composer.sent_history.pending_jump = Some(PendingSentJump {
        message_id: message_id.to_string(),
        timestamp,
        pages: 0,
    });
    advance_jump(state, context)
}

/// Runs on the click and again on every settle while a page read is on its way.
pub fn advance_jump(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let Some(pending) = state.composer.sent_history.pending_jump.clone() else {
        return Vec::new();
    };
    if state.messages.loading_earlier || state.messages.load_earlier_request.is_some() {
        return Vec::new();
    }
    let rows = crate::transcript::rows(state, context);
    let found =
        crate::composer::files_links::row_of(&rows, &pending.message_id, pending.timestamp);
    // A row found only by time is the turn before the message: keep paging until the message
    // itself is loaded.
    let exact = found.is_some()
        && crate::composer::files_links::row_of(&rows, &pending.message_id, None).is_some();
    if !exact && state.messages.has_more && pending.pages < MAX_JUMP_PAGES {
        let effects = crate::session::actions::load_earlier(state, context);
        if !effects.is_empty() {
            if let Some(pending) = state.composer.sent_history.pending_jump.as_mut() {
                pending.pages += 1;
            }
            return effects;
        }
    }
    state.composer.sent_history.pending_jump = None;
    if rows.is_empty() {
        return Vec::new();
    }
    let revision = state
        .composer
        .sent_history
        .jump
        .map_or(1, |jump| jump.revision + 1);
    state.composer.sent_history.jump = Some(SentJump {
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

/// Whether the chat has a message the user sent among its loaded rows.
fn enabled(state: &ChatState) -> bool {
    state.session.agent_session_id.is_some()
        && crate::state::messages_of(&state.messages.list).any(is_own_prompt)
}

/// The preview: the first lines of the message with its links read as their labels.
fn preview(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if out.chars().count() >= PREVIEW_CHARS {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&link_labels(line));
    }
    let characters: Vec<char> = out.trim().chars().collect();
    if characters.len() <= PREVIEW_CHARS {
        return characters.into_iter().collect();
    }
    let mut cut: String = characters[..PREVIEW_CHARS - 1].iter().collect();
    cut.push('\u{2026}');
    cut
}

/// Put back's text: the words, then the pictures as the composer writes them.
fn draft(card: &SentCard) -> String {
    let mut content = card.text.clone();
    let pictures: Vec<String> = card
        .images
        .iter()
        .filter_map(|image| {
            image
                .path
                .as_deref()
                .or(image.url.as_deref())
                .filter(|target| !target.is_empty())
        })
        .enumerate()
        .map(|(index, target)| format!("[Image #{}]({target})", index + 1))
        .collect();
    if !pictures.is_empty() {
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&pictures.join(" "));
    }
    content
}

fn item(card: &SentCard, context: &ChatContext) -> SentItem {
    let images = card
        .images
        .iter()
        .map(|image| {
            crate::transcript::images::image_source(&crate::transcript::images::ImageRef {
                path: image.path.clone(),
                url: image.url.clone(),
                alt: image.alt.clone(),
            })
        })
        .collect();
    let files = card
        .files
        .iter()
        .map(|path| SentFile {
            label: crate::transcript::jsstr::last_path_segment(path).to_string(),
            path: path.clone(),
        })
        .collect();
    let (reference_link, reference_pill) = match (card.number, card.message_ref.as_deref()) {
        (Some(number), Some(full)) => (
            copied_message_link(full, &card.text).or_else(|| Some(full.to_string())),
            Some(message_link_named(
                &MessageRef {
                    session_id: None,
                    number,
                },
                &card.text,
            )),
        ),
        _ => (None, None),
    };
    SentItem {
        id: card.id.clone(),
        timestamp: card.timestamp,
        preview: preview(&card.text),
        time: crate::transcript::message_time::message_time(card.timestamp, context),
        images,
        files,
        copy_text: card.text.clone(),
        draft: draft(card),
        reference_link,
        reference_pill,
    }
}

/// The document's `sentHistory`.
pub fn document(state: &ChatState, context: &ChatContext) -> SentHistory {
    let history = &state.composer.sent_history;
    let enabled = enabled(state);
    let open = history.open;
    let searching = !history.query.trim().is_empty() || history.filter != SentFilter::All;
    let filters = [
        (SentFilter::All, "All"),
        (SentFilter::Attachments, "With attachments"),
        (SentFilter::Text, "Text only"),
    ]
    .into_iter()
    .map(|(filter, label)| SentFilterView {
        id: filter.wire().to_string(),
        label: label.to_string(),
        active: history.filter == filter,
    })
    .collect();
    let count = history.cards.len();
    let caption = if !open || !history.loaded || history.error.is_some() {
        String::new()
    } else if searching {
        format!(
            "{count}{} found",
            if history.has_more { "+" } else { "" }
        )
    } else {
        format!("{count}{} sent", if history.has_more { "+" } else { "" })
    };
    let empty_label = if searching {
        "No sent message matches.".to_string()
    } else {
        "No sent messages in this chat yet.".to_string()
    };
    SentHistory {
        enabled,
        open,
        loading: open && history.request.is_some() && !history.appending,
        loading_more: open && history.request.is_some() && history.appending,
        error: history.error.clone().filter(|_| open),
        query: history.query.clone(),
        filters,
        items: if open {
            history.cards.iter().map(|card| item(card, context)).collect()
        } else {
            Vec::new()
        },
        has_more: open && history.has_more,
        caption,
        empty_label,
        jump: history.jump,
    }
}


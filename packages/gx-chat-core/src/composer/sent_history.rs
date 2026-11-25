//! History: the pill above the chat box that lists every message the user sent in this chat as
//! cards, with a search field and filters, each card jumping to its message or copying, reusing or
//! referencing it.
//!
//! gxserver reads the list from the session's whole transcript (`/api/readSessionChatSent`,
//! server/src/session_chat_sent.rs), a page at a time from the newest back, with the search and
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
//! pages fifty at a time from gxserver (now 20 first, then 40 a page: the WHY below).
//! SEE-ALSO: server/src/session_chat_sent.rs (the read), apps/desktop/src/app/native_chat/sent_history.rs
//! and apps/mobile/app/src/chat/native/cards/SentHistorySheet.tsx (drawing).
//!
//! CDXC:SessionChat 2026-11-26 WHY:
//! User: "when i click on chat history to open it it loads slow man pls make it load faster (load
//! only first 20 messages for ex. load more above when i scroll to them. do other optimisations like
//! that pls make it load fast. Cache it also so clicking again isn't slow". Measured on a copy of the
//! 2 GB coordinator transcript (web build, test gxserver): every open waited 600 to 850 ms for the
//! read, because a chat that is being written to changes its file, so gxserver's kept pages never
//! answered it, and a close threw the list away. Now the first view reads 20 cards, older pages 40
//! as the list nears its top, and the list of the last few chats is kept ([`CACHED_CHATS`]): a
//! History opened again shows it at once and reads only the newest page in the background,
//! fitting it on the kept cards ([`fit_newest`]), which also drops what a rewind took away.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ghostex_gx_protocol::{ChatBlock, ChatMessage};

use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::transcript::message_numbers::{
    copied_message_link, is_own_prompt, link_labels, message_link_named, shown_text, MessageRef,
};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// Cards the first view reads, and the background read of a kept list.
pub const FIRST_PAGE_CARDS: usize = 20;
/// Cards each older page brings as the list nears its top; gxserver's default for a read that
/// names no limit.
pub const PAGE_CARDS: usize = 40;
/// Chats whose list is kept after History closes, the most recently opened first to stay.
const CACHED_CHATS: usize = 4;
/// A kept list longer than this (cards, or bytes of their words) is not kept, so memory stays
/// small however far a list was scrolled.
const CACHED_CARDS: usize = 300;
const CACHED_BYTES: usize = 2 * 1024 * 1024;
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
    /// The read in flight is the newest page of a kept list, fitted on its cards when it answers
    /// ([`fit_newest`]); the kept cards show meanwhile, with no loading line.
    pub refreshing: bool,
    /// Bumped whenever `cards` change, so the drawn cards are built once per change ([`ItemMemo`]).
    pub revision: u64,
    pub memo: ItemMemo,
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

/// The cards as drawn, kept for the `cards` revision and the day they were built on, so a publish
/// while the list is open (a reply streaming in) does not build every card again.
#[derive(Debug, Default)]
pub struct ItemMemo(std::sync::Mutex<Option<(u64, i64, Vec<SentItem>)>>);

impl Clone for ItemMemo {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for ItemMemo {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// A chat's list as History last showed it, unsearched and unfiltered.
struct CachedList {
    key: String,
    cards: Vec<SentCard>,
    before: Option<u64>,
    has_more: bool,
}

/// The kept lists, the most recently used last.
static CACHE: std::sync::Mutex<Vec<CachedList>> = std::sync::Mutex::new(Vec::new());

/// The chat and the transcript it reads: a new conversation in the same session is another list.
fn cache_key(state: &ChatState) -> Option<String> {
    let agent_session = state.session.agent_session_id.as_deref()?;
    Some(format!("{}|{agent_session}", state.identity.session_key))
}

/// Keeps the list History shows when it is the whole one (no search, no filter, read without an
/// error), or forgets the chat's kept list when it grew too long to keep.
fn remember(state: &ChatState) {
    let history = &state.composer.sent_history;
    if !history.loaded
        || history.error.is_some()
        || !history.query.trim().is_empty()
        || history.filter != SentFilter::All
    {
        return;
    }
    let Some(key) = cache_key(state) else {
        return;
    };
    let bytes: usize = history.cards.iter().map(|card| card.text.len()).sum();
    let Ok(mut cache) = CACHE.lock() else {
        return;
    };
    cache.retain(|list| list.key != key);
    if history.cards.len() > CACHED_CARDS || bytes > CACHED_BYTES {
        return;
    }
    cache.push(CachedList {
        key,
        cards: history.cards.clone(),
        before: history.before,
        has_more: history.has_more,
    });
    let over = cache.len().saturating_sub(CACHED_CHATS);
    cache.drain(..over);
}

/// The chat's kept list, which becomes the most recently used.
fn recall(state: &ChatState) -> Option<(Vec<SentCard>, Option<u64>, bool)> {
    let key = cache_key(state)?;
    let mut cache = CACHE.lock().ok()?;
    let at = cache.iter().position(|list| list.key == key)?;
    let list = cache.remove(at);
    let kept = (list.cards.clone(), list.before, list.has_more);
    cache.push(list);
    Some(kept)
}

/// Fits the newest page on a kept list: the kept cards up to the page's first one, then the page,
/// which brings the messages sent since and leaves out any a rewind took. `None` when the page
/// does not meet the kept cards (more was sent since than a page holds, a rewind went further
/// back, another transcript) or holds the whole chat: the page is the list then.
fn fit_newest(kept: &[SentCard], page: &[SentCard], page_has_more: bool) -> Option<Vec<SentCard>> {
    let first = page.first()?;
    if !page_has_more {
        return None;
    }
    let at = kept.iter().position(|card| card.id == first.id)?;
    let mut cards = kept[..at].to_vec();
    cards.extend_from_slice(page);
    Some(cards)
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
    show_all(state)
}

/// The whole list: the kept one at once, with the newest page read behind it, or a first read.
fn show_all(state: &mut ChatState) -> Vec<Effect> {
    let Some((cards, before, has_more)) = recall(state) else {
        return read(state, Read::First);
    };
    let history = &mut state.composer.sent_history;
    history.cards = cards;
    history.before = before;
    history.has_more = has_more;
    history.loaded = true;
    history.revision += 1;
    read(state, Read::Refresh)
}

/// The list went away, kept for the next open; an answer still on its way is dropped, a jump it
/// started goes on.
pub fn close(state: &mut ChatState) -> Vec<Effect> {
    remember(state);
    let history = &mut state.composer.sent_history;
    history.open = false;
    history.request = None;
    history.appending = false;
    history.refreshing = false;
    history.cards.clear();
    history.revision += 1;
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
    remember(state);
    state.composer.sent_history.query = query.to_string();
    reread(state)
}

/// The search or the filter changed: the whole list comes back from the kept one, anything else
/// reads again from the newest.
fn reread(state: &mut ChatState) -> Vec<Effect> {
    let history = &state.composer.sent_history;
    if history.query.trim().is_empty() && history.filter == SentFilter::All {
        return show_all(state);
    }
    read(state, Read::First)
}

/// A filter was picked: the list reads again from the newest.
pub fn set_filter(state: &mut ChatState, filter: &str) -> Vec<Effect> {
    let filter = SentFilter::parse(filter);
    if !state.composer.sent_history.open || state.composer.sent_history.filter == filter {
        return Vec::new();
    }
    remember(state);
    state.composer.sent_history.filter = filter;
    reread(state)
}

/// The list reached its top: the next older page.
pub fn more(state: &mut ChatState) -> Vec<Effect> {
    let history = &state.composer.sent_history;
    if !history.open || history.request.is_some() || !history.has_more || history.before.is_none()
    {
        return Vec::new();
    }
    read(state, Read::Older)
}

/// What a read is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Read {
    /// The newest cards, replacing the list.
    First,
    /// The page before the oldest card.
    Older,
    /// The newest cards, fitted on the kept list shown meanwhile.
    Refresh,
}

fn read(state: &mut ChatState, kind: Read) -> Vec<Effect> {
    let request_id = state.core.allocate_request_id();
    let history = &mut state.composer.sent_history;
    let appending = kind == Read::Older;
    history.request = Some(request_id);
    history.appending = appending;
    history.refreshing = kind == Read::Refresh;
    history.error = None;
    if kind == Read::First {
        history.loaded = false;
    }
    let mut params = json!({
        "query": history.query.trim(),
        "filter": history.filter.wire(),
        "limit": if appending { PAGE_CARDS } else { FIRST_PAGE_CARDS },
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
    let refreshing = std::mem::take(&mut history.refreshing);
    history.revision += 1;
    match outcome {
        RpcOutcome::Ok { result } => {
            let cards: Vec<SentCard> = result
                .get("messages")
                .cloned()
                .and_then(|messages| serde_json::from_value(messages).ok())
                .unwrap_or_default();
            let has_more = result.get("hasMore").and_then(Value::as_bool) == Some(true);
            let before = result.get("before").and_then(Value::as_u64);
            if refreshing {
                // A kept list keeps where its older pages go on from; a page that replaces it
                // brings its own.
                match fit_newest(&history.cards, &cards, has_more) {
                    Some(fitted) => history.cards = fitted,
                    None => {
                        history.cards = cards;
                        history.has_more = has_more;
                        history.before = before;
                    }
                }
                history.loaded = true;
                state.core.request_publish();
                remember(state);
                return Some(Vec::new());
            }
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
            history.has_more = has_more;
            history.before = before;
            history.loaded = true;
            state.core.request_publish();
            remember(state);
            return Some(Vec::new());
        }
        // A kept list stays as it is when its background read fails.
        RpcOutcome::Err { .. } if refreshing => {}
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

/// The cards as drawn, built again only when the cards or the day changed (`message_time` reads
/// the day).
fn items(history: &SentHistoryState, context: &ChatContext) -> Vec<SentItem> {
    let day = (context.now_millis() + i64::from(context.utc_offset_minutes) * 60_000)
        .div_euclid(86_400_000);
    if let Ok(memo) = history.memo.0.lock() {
        if let Some((revision, built_on, items)) = memo.as_ref() {
            if *revision == history.revision && *built_on == day {
                return items.clone();
            }
        }
    }
    let built: Vec<SentItem> = history.cards.iter().map(|card| item(card, context)).collect();
    if let Ok(mut memo) = history.memo.0.lock() {
        *memo = Some((history.revision, day, built.clone()));
    }
    built
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
        loading: open && history.request.is_some() && !history.appending && !history.refreshing,
        loading_more: open && history.request.is_some() && history.appending,
        error: history.error.clone().filter(|_| open),
        query: history.query.clone(),
        filters,
        items: if open { items(history, context) } else { Vec::new() },
        has_more: open && history.has_more,
        caption,
        empty_label,
        jump: history.jump,
    }
}


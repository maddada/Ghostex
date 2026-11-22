//! "For later": a question form or question page the reader put off. It stays answerable, but it
//! no longer counts as waiting: no pink dot on the session row, no count or ping on the ? pill, no
//! notification. gxserver keeps the list (`laterQuestionIds`, server/src/session_chat_open_questions.rs)
//! so every client agrees and it survives restarts; this side shows a change at once and keeps it
//! shown until the wire agrees.
//!
//! CDXC:SessionStatus 2026-10-27 DECISION:
//! User: "i wish i could just mark an inline html as "For later" so it isn't bothering me with pink dot or saying "unanswered" but then i can go back to it any time to answer it". A page card and a question form offer "For later" while they wait, and "Back to unanswered" once put off. A For-later question leaves the session's question count (the sidebar's pink dot, `answer requested (n)`), the ? pill's count, ping and the turn-end notification, and is listed under For later in the ? pill's list. Answering it moves it to Answered; gxserver then forgets the mark.
//! SEE-ALSO: server/src/session_chat_open_questions.rs (the stored list and the count), server/src/session_chat_send/answer_http.rs (`questionForLater`), questions/open_questions.rs (the pill and its list), apps/desktop/src/app/native_chat/page_card.rs and question_form.rs, apps/mobile/app/src/chat/native/transcript/PageCard.tsx and QuestionForm.tsx.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The marks set here that the wire has not confirmed yet, by form or page id: whether it is For
/// later, and the call that set it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaterState {
    pub pending: BTreeMap<String, (bool, u64)>,
    /// The removals set here that the wire has not confirmed yet, by id, and the call that set them.
    pub removing: BTreeMap<String, u64>,
    /// The stars set or taken off here that the wire has not confirmed yet, by page id: whether it
    /// is starred, and the call that set it (`star`).
    pub starring: BTreeMap<String, (bool, u64)>,
    /// The pages opened here that the wire has not confirmed yet, by page id, and the call that
    /// set them (`seen_on_open`).
    pub opening: BTreeMap<String, u64>,
}

/// The forms and pages put off For later: gxserver's list with this client's changes on top.
pub fn later_ids(state: &ChatState) -> BTreeSet<String> {
    later_ids_at(state, 0)
}

/// [`later_ids`] read at `now_ms` (0 when the caller has no clock): any timed snooze whose end
/// (`later_question_until`) has passed is no longer put off, so a snoozed form or page returns to
/// Needs you as soon as its snooze ends even before a new server frame arrives.
pub fn later_ids_at(state: &ChatState, now_ms: i64) -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = state
        .session
        .later_question_ids
        .iter()
        .filter(|id| {
            now_ms <= 0
                || state
                    .session
                    .later_question_until
                    .get(*id)
                    .is_none_or(|until| *until > now_ms)
        })
        .cloned()
        .collect();
    for (id, (later, _)) in &state.questions.later.pending {
        if *later {
            ids.insert(id.clone());
        } else {
            ids.remove(id);
        }
    }
    ids
}

/// The forms and pages the reader removed without answering: gxserver's list with this client's
/// removals on top. They no longer wait, count or notify, and the list shows them under Earlier,
/// dimmed, as Canceled.
///
/// CDXC:SessionChat 2026-10-30 DECISION:
/// User, of the Chat Resources list's For later rows: "when i hover on it then we should show the restore button and a new x button to remove a question without answering it". The X removes a form or page for good: it leaves the unanswered and For later counts, the ? pill's stepping and the header's "N for later", moves to Earlier, is kept across restarts in gxserver beside the For later list, and sends nothing to the agent. Then (2026-10-30, Q797): "Canceled" is the word, on the row in the list and on the question's card in the chat in place of For later or Waiting; the form still opens to be read but takes no answer (no picks, notes, Skip all or Send), and the phone's rows and form card do the same. Supersedes the row being marked like a withdrawn question.
pub fn removed_ids(state: &ChatState) -> BTreeSet<String> {
    state
        .session
        .removed_question_ids
        .iter()
        .chain(state.questions.later.removing.keys())
        .cloned()
        .collect()
}

/// Every page the chat knows of (its loaded rows and its stored index): id, the file it was
/// published from, and when.
fn known_pages(state: &ChatState) -> Vec<(String, Option<String>, Option<i64>)> {
    use crate::transcript::open_questions::AskerKind;
    let mut pages: Vec<(String, Option<String>, Option<i64>)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut add = |id: &str, source: Option<&String>, at: Option<i64>| {
        if seen.insert(id.to_string()) {
            pages.push((id.to_string(), source.cloned(), at));
        }
    };
    for asker in state.transcript_view.askers.iter() {
        if asker.kind == AskerKind::Page {
            add(&asker.id, asker.source.as_ref(), asker.at);
        }
    }
    for item in state.questions.content.items.iter() {
        if item.kind == crate::transcript::question_forms::ContentKind::Page {
            add(&item.id, item.source.as_ref(), item.at);
        }
    }
    let view = state.session.resource_index.view();
    for entry in view.records().iter() {
        if entry.kind == "page" {
            add(&entry.id, entry.source.as_ref(), entry.at_ms);
        }
    }
    pages
}

/// The pages shown starred: gxserver's list with this client's changes on top, each star moved
/// to the newest version of its page's file (a newer `ghostex show` of the same file, whose old
/// card reads Replaced). Taking a star off takes it off every version of that file.
///
/// CDXC:SessionChat 2026-11-16 DECISION:
/// BANOZZ asked on Discord ("star resources", 2026-11-13) to star pages in a chat and find them in
/// Resources; the user promised it and picked on the star-resources page (Q3540 to Q3549): pages
/// only; a Starred entry in the Resources filter (no group of its own; starred rows stay in
/// their groups), newest first; on the desktop a star in the page card's window bar, a row's hover
/// button and Star page in the page window's ⋯ menu; on the phone Star in the ⋯ menus of the
/// page card and the page sheet, and a star on starred rows; stars per session, which agents
/// cannot read; and when a newer version of the same file replaces a starred page, the star
/// follows it.
/// SEE-ALSO: server/src/session_chat_open_questions.rs (`set_starred`, the stored list),
/// questions/open_questions.rs (the Starred filter), apps/desktop/src/app/native_chat/page_card.rs,
/// apps/desktop/src/app/window/visual_page_modal/more_menu.rs,
/// apps/mobile/app/src/chat/native/cards/CardMoreMenu.tsx.
pub fn starred_page_ids(state: &ChatState) -> BTreeSet<String> {
    let starring = &state.questions.later.starring;
    if state.session.starred_page_ids.is_empty() && starring.values().all(|(on, _)| !on) {
        return BTreeSet::new();
    }
    let pages = known_pages(state);
    let source_of = |id: &str| -> Option<&String> {
        pages
            .iter()
            .find(|(page, _, _)| page == id)
            .and_then(|(_, source, _)| source.as_ref())
    };
    // The files a star was taken off here: every version of them is off.
    let cleared: BTreeSet<&String> = starring
        .iter()
        .filter(|(_, (on, _))| !on)
        .filter_map(|(id, _)| source_of(id))
        .collect();
    let mut ids: BTreeSet<String> = state.session.starred_page_ids.iter().cloned().collect();
    for (id, (on, _)) in starring {
        if *on {
            ids.insert(id.clone());
        } else {
            ids.remove(id);
        }
    }
    let mut shown = BTreeSet::new();
    let mut files: BTreeSet<&String> = BTreeSet::new();
    for id in &ids {
        match source_of(id) {
            Some(source) if cleared.contains(source) => {}
            Some(source) => {
                files.insert(source);
            }
            None => {
                shown.insert(id.clone());
            }
        }
    }
    // Each starred file's newest version (the last one listed wins a tie, as the transcript's
    // order puts the newer one later).
    for file in files {
        let newest = pages
            .iter()
            .filter(|(_, source, _)| source.as_ref() == Some(file))
            .fold(
                None::<&(String, Option<String>, Option<i64>)>,
                |best, page| match best {
                    Some(best) if best.2 > page.2 => Some(best),
                    _ => Some(page),
                },
            );
        if let Some((id, _, _)) = newest {
            shown.insert(id.clone());
        }
    }
    shown
}

/// `pageStar {id, starred}`: stars a page in this chat, or takes its star off (every version of
/// its file). Only a page: a form or chart is not starred (Q3540).
pub fn star(state: &mut ChatState, id: &str, starred: bool) -> Vec<Effect> {
    let id = id.trim();
    if id.is_empty() || starred_page_ids(state).contains(id) == starred {
        return Vec::new();
    }
    if !known_pages(state).iter().any(|(page, _, _)| page == id) {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state
        .questions
        .later
        .starring
        .insert(id.to_string(), (starred, request_id));
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(json!({
            "kind": "pageStar",
            "questionId": id,
            "starred": starred,
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
        })),
    }]
}

/// `questionRemove {id}`: removes a form or page without answering it.
pub fn remove(state: &mut ChatState, id: &str) -> Vec<Effect> {
    remove_naming(state, id, None)
}

/// [`remove`], naming the question for the agent when it is the user's Dismiss (`dismissed`, as
/// the agent knows it): gxserver tells the agent with the next message it gets anyway.
fn remove_naming(state: &mut ChatState, id: &str, dismissed: Option<String>) -> Vec<Effect> {
    let id = id.trim();
    if id.is_empty() || removed_ids(state).contains(id) {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state
        .questions
        .later
        .removing
        .insert(id.to_string(), request_id);
    state.core.request_publish();
    let mut params = json!({
        "kind": "questionRemove",
        "questionId": id,
        "projectId": state.identity.project_id,
        "sessionId": state.identity.session_id,
    });
    if let Some(name) = dismissed.filter(|name| !name.trim().is_empty()) {
        params["dismissed"] = json!(name);
    }
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(params),
    }]
}

/// `questionForLater {id, later}`: puts a form or page off For later, or back to unanswered.
pub fn set(state: &mut ChatState, id: &str, later: bool) -> Vec<Effect> {
    let id = id.trim();
    if id.is_empty() || later_ids(state).contains(id) == later {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state
        .questions
        .later
        .pending
        .insert(id.to_string(), (later, request_id));
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(json!({
            "kind": "questionForLater",
            "questionId": id,
            "later": later,
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
        })),
    }]
}

/// `snoozeQuestion {id, hours}`: puts a form or page off For later for `hours` (1, 6 or 24); gxserver
/// brings it back by itself at the end (server/src/session_chat_open_questions.rs `LATER_UNTIL_KEY`).
/// A snooze again on one already put off moves its end.
///
/// CDXC:SessionChat 2026-11-10 DECISION:
/// User: "for any ask i should be able to click 'Snooze' and snooze it for 1h 6h 24h or cancel it".
/// Snooze is For later with an end, not a second kind of putting off; Cancel is the removal
/// (`remove`) that also tells the agent (`cancel`; since 2026-11-17 only with its next message).
pub fn snooze(state: &mut ChatState, id: &str, hours: u64) -> Vec<Effect> {
    let id = id.trim();
    if id.is_empty() {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state
        .questions
        .later
        .pending
        .insert(id.to_string(), (true, request_id));
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(json!({
            "kind": "questionForLater",
            "questionId": id,
            "later": true,
            "hours": hours.clamp(1, 24 * 14),
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
        })),
    }]
}

/// `cancelQuestion {id}`: the dock's Dismiss (Mark as read on a page that only waits to be seen).
/// Removes a form or page for good (`remove`) and sends the agent nothing: gxserver keeps its name
/// ("\"Release plan\" (Q12 to Q14)") and tells the agent with the next message it gets anyway, in
/// that message's hidden `<ghostex-system>` note, never as a bubble of the user's.
///
/// CDXC:SessionChat 2026-11-17 DECISION:
/// User, of "I dismissed your questions there: no need to ask them again." drawn as their own
/// bubble, twice for two dismissals: Dismiss is silent (Q3800 A, Q3801), on the phone too (Q3802),
/// and a row with no questions reads Mark as read. Supersedes the 2026-11-10 rule above that
/// Cancel also tells the agent. The full decision is in server/src/session_chat_dismissed.rs.
pub fn cancel(state: &mut ChatState, id: &str) -> Vec<Effect> {
    let asker = state
        .transcript_view
        .askers
        .iter()
        .find(|asker| asker.id == id);
    let (tags, title, seen_only) = asker
        .map(|asker| (asker.waiting_tags(), asker.title.clone(), asker.seen_only))
        .or_else(|| {
            state
                .session
                .resource_index
                .view()
                .records()
                .iter()
                .find(|entry| entry.id == id)
                .map(|entry| {
                    let seen_only =
                        entry.kind == "page" && (entry.awaits_seen || entry.tags.is_empty());
                    (entry.tags.clone(), entry.title.clone(), seen_only)
                })
        })
        .unwrap_or_default();
    let title = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('"', "'");
    let tags = if tags.is_empty() {
        String::new()
    } else {
        crate::transcript::open_questions::tag_range(&tags)
    };
    let name = match (title.is_empty(), tags.is_empty()) {
        (false, false) => format!("\"{title}\" ({tags})"),
        (false, true) => format!("\"{title}\""),
        (true, false) => tags,
        (true, true) => "an untitled form or page".to_string(),
    };
    let name = if seen_only {
        format!("{name} (marked read)")
    } else {
        name
    };
    remove_naming(state, id, Some(name))
}

/// `markQuestionAnswered {id}`: a dock row's ⋯ Mark answered on a form, page or one-click question
/// the reader already answered in their own words in the chat box. It leaves the dock as `remove`
/// takes it off, and the agent is told it was answered there, by its title with the tags it still
/// asks after it, as a Reply names it ("Settings layout (Q12): …").
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User, on issue #627 (an ask answered in the normal chat box stays under Needs you until he
/// clicks a wrong answer to clear it): add "Mark answered" next to Dismiss in each question row's
/// ⋯ menu, on the desktop dock and the phone, which closes the ask by hand.
pub fn mark_answered(state: &mut ChatState, id: &str) -> Vec<Effect> {
    let (tags, title) = state
        .transcript_view
        .askers
        .iter()
        .find(|asker| asker.id == id)
        .map(|asker| (asker.waiting_tags(), asker.title.clone()))
        .or_else(|| {
            state
                .session
                .resource_index
                .view()
                .records()
                .iter()
                .find(|entry| entry.id == id)
                .map(|entry| (entry.tags.clone(), entry.title.clone()))
        })
        .unwrap_or_default();
    let mut effects = remove(state, id);
    if effects.is_empty() {
        return effects;
    }
    let title = title.replace(['[', ']'], "");
    let name = match (title.trim().is_empty(), tags.is_empty()) {
        (false, false) => format!(
            "{} ({})",
            title.trim(),
            crate::transcript::open_questions::tag_range(&tags)
        ),
        (false, true) => title.trim().to_string(),
        (true, false) => crate::transcript::open_questions::tag_range(&tags),
        (true, true) => "Your questions there".to_string(),
    };
    let request_id = state.core.allocate_request_id();
    effects.push(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SendSessionChatMessage,
        params: Box::new(json!({
            "text": format!("{name}: I answered this in the chat."),
            "sendRequestId": format!("question-answered-{}-{id}", state.identity.session_id),
        })),
    });
    effects
}

/// `markPagesSeen {ids}`: the dock's Mark all read. Each page that waits only to be looked at
/// ("not seen yet") is removed as `remove` does, so it leaves the dock, the counts and the
/// sidebar's dot on every client and gxserver keeps it across restarts. Nothing reaches the agent:
/// a page's own Mark as read tells it, but one message per page would bury it.
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User picked "reply and build" on issue #626 (a dock with 16 page cards "not seen yet", most of
/// them already read): the dock gets Mark all read, on the desktop and the phone. A page cleared
/// this way reads Seen on its card, as one marked seen on the page does.
///
/// Only a page still waiting to be seen is cleared: one that asks questions keeps waiting for its
/// answers.
pub fn mark_seen(state: &mut ChatState, ids: &[&str]) -> Vec<Effect> {
    let unseen: Vec<String> = crate::questions::open_questions::all_waiting(state)
        .into_iter()
        .filter(|asker| asker.seen_only)
        .filter(|asker| {
            ids.iter()
                .any(|id| asker.id.eq_ignore_ascii_case(id.trim()))
        })
        .map(|asker| asker.id)
        .collect();
    unseen.iter().flat_map(|id| remove(state, id)).collect()
}

/// The pages that wait only to be looked at which the reader opened: gxserver's list with this
/// client's opens on top. Their dock rows and Work panel cards stay, but they no longer count as
/// unseen (the Work badge, the dock's header, gxserver's count behind the bell and the sidebar).
pub fn opened_page_ids(state: &ChatState) -> BTreeSet<String> {
    state
        .session
        .opened_page_ids
        .iter()
        .chain(state.questions.later.opening.keys())
        .map(|id| id.to_ascii_lowercase())
        .collect()
}

/// Whether a page that waits only to be looked at was opened ([`opened_page_ids`]).
pub fn page_opened(state: &ChatState, id: &str) -> bool {
    opened_page_ids(state).contains(&id.to_ascii_lowercase())
}

/// Opening a page that waits only to be looked at marks it opened (`pageOpened`, kept by gxserver
/// for every client): the dock, the Work panel, a Q-tag link, a chat link to it or its card, on the
/// desktop or the phone. It keeps its dock row and card until the reader dismisses it or marks it
/// seen (the page's own Mark as read, the dock's Mark all read), but leaves every unseen count.
/// Nothing reaches the agent, and a call that fails is dropped without a word: the reader only
/// opened a page. `target` is the page's id or any address that names it.
///
/// CDXC:Coordinators 2026-11-19 DECISION:
/// User, of a dock row "ClickHouse demo notes: what we record and ask (not seen yet)": "i see
/// "(not seen)" here but i actually opened and went thro this, just want to keep it here. we
/// shouldn't show "(not seen)" like this". Only the page's own Mark as read (at its very end)
/// counted, so a page read but not pressed at its end kept counting as not seen on the bell and
/// badges. Opening it now counts as seen for the counts, and the row stays where he kept it.
pub fn seen_on_open(state: &mut ChatState, target: &str) -> Vec<Effect> {
    let target = target.trim();
    if target.is_empty() {
        return Vec::new();
    }
    let lower = target.to_ascii_lowercase();
    let opened = opened_page_ids(state);
    let ids: Vec<String> = crate::questions::open_questions::all_waiting(state)
        .into_iter()
        .filter(|asker| asker.seen_only && !asker.id.is_empty())
        .filter(|asker| lower.contains(&asker.id.to_ascii_lowercase()))
        .filter(|asker| !opened.contains(&asker.id.to_ascii_lowercase()))
        .map(|asker| asker.id)
        .collect();
    let mut effects = Vec::new();
    for id in ids {
        let request_id = state.core.allocate_request_id();
        state.questions.later.opening.insert(id.clone(), request_id);
        effects.push(Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::AnswerSessionChatPrompt,
            params: Box::new(json!({
                "kind": "pageOpened",
                "questionId": id,
                "projectId": state.identity.project_id,
                "sessionId": state.identity.session_id,
            })),
        });
    }
    if !effects.is_empty() {
        state.core.request_publish();
    }
    effects
}

/// `questionsReply {id, text}`: the reader's own words on a whole form or page, from its dock
/// row's Reply, sent to the agent as a message without touching the chat box. `text` comes already
/// after its title and the questions it still asks ("Triage (Q12 to Q14): words", `extras::work_cards`), as a work
/// item ask's Reply comes as "W7 Q12: words". The form or page stays open: the words answer none
/// of its questions by themselves.
pub fn reply(state: &mut ChatState, id: &str, text: &str) -> Vec<Effect> {
    let text = text.trim();
    if id.is_empty() || text.is_empty() {
        return Vec::new();
    }
    // The same words on the same form or page are one send on every client and run, so a second
    // press is dropped by the send ledger (FNV-1a, as `coordinator_board`'s answers are).
    let hash = format!("{id}\n{text}")
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    let session: String = state.identity.session_id.chars().take(40).collect();
    let request_id = state.core.allocate_request_id();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SendSessionChatMessage,
        params: Box::new(json!({
            "text": text,
            "sendRequestId": format!("questions-reply-{session}-{hash:016x}"),
        })),
    }]
}

/// The call's answer, or `None` when it is not one of these. A refusal drops the change and says
/// why; a success keeps it shown until the wire carries it (`settle`).
pub fn rpc_settled(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let marked = state
        .questions
        .later
        .pending
        .iter()
        .find(|(_, (_, request))| *request == request_id)
        .map(|(id, _)| id.clone());
    let removed = state
        .questions
        .later
        .removing
        .iter()
        .find(|(_, request)| **request == request_id)
        .map(|(id, _)| id.clone());
    let star = state
        .questions
        .later
        .starring
        .iter()
        .find(|(_, (_, request))| *request == request_id)
        .map(|(id, _)| id.clone());
    // A page's open is silent either way (`seen_on_open`): a refusal only drops it, so the page
    // counts as unseen again.
    let opened = state
        .questions
        .later
        .opening
        .iter()
        .find(|(_, request)| **request == request_id)
        .map(|(id, _)| id.clone());
    if let Some(id) = opened {
        if matches!(outcome, RpcOutcome::Err { .. }) {
            state.questions.later.opening.remove(&id);
            // Claimed, so the chat box shows no error card for it either (an older gxserver
            // refuses the kind).
            state.core.refusal_claimed = true;
            state.core.request_publish();
        }
        return Some(Vec::new());
    }
    if marked.is_none() && removed.is_none() && star.is_none() {
        return None;
    }
    let RpcOutcome::Err { message, .. } = outcome else {
        return Some(Vec::new());
    };
    let what = if let Some(id) = star {
        state.questions.later.starring.remove(&id);
        "change the page's star"
    } else if let Some(id) = marked {
        state.questions.later.pending.remove(&id);
        "change For later"
    } else {
        if let Some(id) = removed {
            state.questions.later.removing.remove(&id);
        }
        "remove the question"
    };
    state.core.request_publish();
    Some(vec![Effect::Toast {
        level: "error".to_string(),
        message: format!("Couldn't {what}: {message}"),
    }])
}

/// Forgets the stars the wire now carries: a star once its page is on gxserver's list, a star
/// taken off once neither the page nor another version of its file is.
fn settle_stars(state: &mut ChatState) {
    let starred = &state.session.starred_page_ids;
    let pages = if state.questions.later.starring.values().any(|(on, _)| !on) {
        known_pages(state)
    } else {
        Vec::new()
    };
    let source_of = |id: &str| -> Option<&String> {
        pages
            .iter()
            .find(|(page, _, _)| page == id)
            .and_then(|(_, source, _)| source.as_ref())
    };
    let settled: Vec<String> = state
        .questions
        .later
        .starring
        .iter()
        .filter(|(id, (on, _))| {
            if *on {
                return starred.contains(id);
            }
            let file = source_of(id);
            !starred
                .iter()
                .any(|known| known == *id || (file.is_some() && source_of(known) == file))
        })
        .map(|(id, _)| id.clone())
        .collect();
    if settled.is_empty() {
        return;
    }
    for id in settled {
        state.questions.later.starring.remove(&id);
    }
}

/// Forgets the changes the wire now carries.
pub fn settle(state: &mut ChatState) {
    if !state.questions.later.starring.is_empty() {
        settle_stars(state);
    }
    if !state.questions.later.opening.is_empty() {
        let wire: BTreeSet<String> = state
            .session
            .opened_page_ids
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .collect();
        if state
            .questions
            .later
            .opening
            .keys()
            .any(|id| wire.contains(&id.to_ascii_lowercase()))
        {
            state
                .questions
                .later
                .opening
                .retain(|id, _| !wire.contains(&id.to_ascii_lowercase()));
        }
    }
    let wire = &state.session.later_question_ids;
    let removed = &state.session.removed_question_ids;
    // Runs on every event: the state is only borrowed mutably when a mark actually settles, so its
    // stamp (`state::Stamped`) stays put otherwise.
    let later = &state.questions.later;
    if !later
        .pending
        .iter()
        .any(|(id, (later, _))| wire.contains(id) == *later)
        && !later.removing.keys().any(|id| removed.contains(id))
    {
        return;
    }
    let later = &mut state.questions.later;
    later
        .pending
        .retain(|id, (later, _)| wire.contains(id) != *later);
    later.removing.retain(|id, _| !removed.contains(id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_shows_at_once_and_holds_until_the_wire_agrees() {
        let mut state = ChatState::default();
        let effects = set(&mut state, "page-a", true);
        assert_eq!(effects.len(), 1, "one call to gxserver");
        assert!(later_ids(&state).contains("page-a"));
        // A frame from before the write still says nothing: the mark stays.
        settle(&mut state);
        assert!(later_ids(&state).contains("page-a"));
        *state.session.later_question_ids = vec!["page-a".to_string()];
        settle(&mut state);
        assert!(state.questions.later.pending.is_empty());
        assert!(later_ids(&state).contains("page-a"));
        // Back to unanswered.
        set(&mut state, "page-a", false);
        assert!(!later_ids(&state).contains("page-a"));
        assert!(
            set(&mut state, "page-a", false).is_empty(),
            "already unanswered"
        );
    }

    #[test]
    fn a_refused_mark_is_dropped() {
        let mut state = ChatState::default();
        set(&mut state, "form-a", true);
        let request = state.questions.later.pending["form-a"].1;
        let effects = rpc_settled(
            &mut state,
            request,
            &RpcOutcome::Err {
                code: None,
                message: "no".to_string(),
                endpoint: None,
            },
        );
        assert!(matches!(effects.as_deref(), Some([Effect::Toast { .. }])));
        assert!(!later_ids(&state).contains("form-a"));
    }

    #[test]
    fn a_removal_shows_at_once_holds_until_the_wire_agrees_and_a_refusal_drops_it() {
        let mut state = ChatState::default();
        let effects = remove(&mut state, "page-a");
        assert_eq!(effects.len(), 1);
        assert!(removed_ids(&state).contains("page-a"));
        settle(&mut state);
        assert!(
            removed_ids(&state).contains("page-a"),
            "a frame from before the write"
        );
        *state.session.removed_question_ids = vec!["page-a".to_string()];
        settle(&mut state);
        assert!(state.questions.later.removing.is_empty());
        assert!(
            removed_ids(&state).contains("page-a"),
            "kept from the wire's list"
        );

        remove(&mut state, "form-a");
        let request = state.questions.later.removing["form-a"];
        let effects = rpc_settled(
            &mut state,
            request,
            &RpcOutcome::Err {
                code: None,
                message: "no".to_string(),
                endpoint: None,
            },
        );
        assert!(matches!(effects.as_deref(), Some([Effect::Toast { .. }])));
        assert!(!removed_ids(&state).contains("form-a"));
    }

    /// Opening a page that waits only to be looked at marks it opened, by its id or any address
    /// naming it, and keeps it listed; a page that asks questions, or one already opened, is left
    /// alone.
    #[test]
    fn opening_a_page_that_waits_to_be_seen_keeps_it_listed_but_counts_it_seen() {
        use crate::transcript::open_questions::{Asker, AskerKind};
        let page = |id: &str, seen_only: bool, tags: Vec<String>| Asker {
            id: id.to_string(),
            kind: AskerKind::Page,
            message_id: format!("m-{id}"),
            tags,
            answered: false,
            obsolete: false,
            withdrawn: Vec::new(),
            keys: Vec::new(),
            skipped: Vec::new(),
            superseded: Vec::new(),
            inherited: false,
            seen_only,
            source: None,
            replaced: false,
            prints: Vec::new(),
            rounds: Vec::new(),
            said: Vec::new(),
            title: id.to_string(),
            at: Some(1),
        };
        let unseen = "0123456789abcdef0123456789abcdef";
        let asks = "fedcba9876543210fedcba9876543210";
        let mut state = ChatState::default();
        *state.transcript_view.askers = vec![
            page(unseen, true, Vec::new()),
            page(asks, false, vec!["Q1".to_string()]),
        ];

        assert!(
            seen_on_open(&mut state, &format!("http://127.0.0.1:1/visual/{asks}")).is_empty(),
            "a page that asks questions keeps waiting for its answers"
        );
        assert!(!page_opened(&state, asks));

        let effects = seen_on_open(
            &mut state,
            &format!(
                "http://127.0.0.1:1/visual/{}?gx-readonly=1",
                unseen.to_uppercase()
            ),
        );
        let [Effect::SendRpc {
            request_id, params, ..
        }] = effects.as_slice()
        else {
            panic!("one call to gxserver: {effects:?}");
        };
        assert_eq!(params["kind"], "pageOpened");
        assert!(page_opened(&state, unseen));
        assert!(
            !removed_ids(&state).contains(unseen),
            "it keeps its dock row: opened is not removed"
        );
        assert!(
            crate::questions::open_questions::waiting(&state)
                .iter()
                .any(|asker| asker.id == unseen),
            "still listed until dismissed or marked seen"
        );
        assert!(
            seen_on_open(&mut state, unseen).is_empty(),
            "already opened"
        );

        // A refusal is dropped without a toast: the reader only opened a page.
        let effects = rpc_settled(
            &mut state,
            *request_id,
            &RpcOutcome::Err {
                code: None,
                message: "no".to_string(),
                endpoint: None,
            },
        );
        assert_eq!(effects, Some(Vec::new()));
        assert!(state.core.refusal_claimed, "no error card in the chat box");
        assert!(!page_opened(&state, unseen));

        // Once the wire carries it, the local mark is settled into gxserver's list.
        seen_on_open(&mut state, unseen);
        state.session.opened_page_ids = vec![unseen.to_string()];
        settle(&mut state);
        assert!(state.questions.later.opening.is_empty());
        assert!(page_opened(&state, unseen));
    }
}

//! The "Previous question" pill and the jumps to a question: which forms and question pages are
//! still waiting on the reader, which one each click of the pill goes to, and the transcript row a
//! Q-tag link lands on. What counts as asked, answered and obsolete is
//! `transcript/open_questions.rs`; the renderers scroll to `jump.item` once per `jump.revision` and
//! highlight `jump.target` briefly.
//!
//! CDXC:SessionChat 2026-10-14 DECISION:
//! User (Q81 = C): a "Previous question ↑" pill next to Scroll to bottom while the chat has unanswered questions; each click scrolls to the next older unanswered one, wrapping, and highlights it briefly. A form whose answers are queued or sending is no longer waiting on the reader. Then: "I don't like the emoji in the Previous question thing. Remove that icon and remove the number." Supersedes the same day's "❓ 2 · Previous question ↑" pill. Q-tag links only appear where the agent names a question and do nothing until clicked.
//!
//! CDXC:SessionChat 2026-10-24 DECISION:
//! User: "please remove the setting that alows disabling/enabling previous questions button". The pill always shows while a question is unanswered; the `chatPreviousQuestion` setting (off by default since 2026-10-14, for testing) is gone, and a value saved in an old settings file is ignored. Supersedes the 2026-10-14 off-by-default switch.

//!
//! CDXC:SessionChat 2026-10-27 DECISION:
//! User: "i want hovering over the ? mark "Previous questions" pill in the chat view to show me a list of all the questions. should look like this threads one in terms of having sections and i really care about the "already answered ones" so i can click to jump to those previously answered ones. make sure we sort by date". The pill carries a list of every question form and question page in the chat (`sections`), in the Links card's folding sections: Unanswered, For later, Answered and Withdrawn, each with its count, rows newest first, each row the page's title (or the form's first question), its tags and its time; a row jumps to its card and flashes it. Its count, attention dot and ping are only for unanswered ones (`count`). On the desktop hovering opens the list; on the phone a tap opens it.
//!
//! CDXC:SessionChat 2026-10-28 DECISION:
//! User: "please make the menu that appears when clicking on question not appear on hover, instead should appear when clicking on a new button we add there. call this button resources so that i can just click on the ? mark pill to jump between questions or click on this new button to see the list that we show. show the new resources button between threads and ? pills in the bar and give resources a different icon". The list now opens from a Resources pill (`chatPills`, extras/chat_pills.rs, right after the ? pill and before Threads) while the list has any row (`total`); a second click or Escape closes it. The ? pill never opens a list: a click only steps through the questions (`previous`). Supersedes the 2026-10-27 hover and the click that opened the list when nothing waited.
//!
//! CDXC:SessionChat 2026-10-28 DECISION:
//! User: "The question mark button here keeps jumping me between questions/inline html i already answered pls fix", then "let's not show the ? pill here unless there's an unanswered question (that i didn't set to 'for later')". The ? pill shows only while at least one question form or page is unanswered and not put off For later (`count`); answered, withdrawn and For later ones no longer keep it on screen, the Resources pill lists them and is where For later ones are reached. A click steps only through those (`previous`), never to an answered or For later one, and its dot, ping and ?/count swap follow the same count. Supersedes the 2026-10-27 pill that stayed while the chat had any question, the same day's click that opened the list when none waited, and the 2026-10-28 stepping into For later and Answered ones when none was unanswered.
//!
//! CDXC:SessionChat 2026-10-27 DECISION:
//! User, of the list: "we're not indicating HTML pages from normal questions here", "also please show "All" "Html" "Q&A" "Charts" as 4 selections so i can jump to previous generated content", "also i wanna see even Htmls that don't have any questions in them so i can jump back to them". Every row says what it is (`kind`: an HTML page, a Q&A form or a chart), and four filters sit over the list (`filters`, the reader's pick `filter`): All, Html (pages, with or without questions), Q&A (```questions forms) and Charts (```visual drawings: charts, stats, tables). Forms and question pages keep the Unanswered, For later, Answered and Withdrawn sections; pages that ask nothing are listed under Pages and charts under Charts, newest first. The pill's count, dot and ping stay about unanswered questions only.
//!
//! CDXC:SessionChat 2026-11-06 DECISION:
//! User: "i hate saying 'inline html' and 'inline q' and inline graphs. can we have different name for those?", then "Let's say ghostex page / gx page, ghostex form / gx form, ghostex visuals / gx visuals", everywhere. The filters read All, Pages, Forms and Visuals (ids `html`, `qa`, `charts` unchanged), and an untitled row is a "Ghostex page", "Ghostex form" or "Ghostex visual". Supersedes the 2026-10-27 labels Html, Q&A and Charts.

// A title shows a link's words, not its Markdown (`questions/form_links.rs`).
use crate::questions::form_links::plain_words;
use serde::{Deserialize, Serialize};

use crate::effect::Effect;
use crate::questions::resource_index::ResourceEntry;
use crate::state::{ChatContext, ChatState};
use crate::transcript::open_questions::{tag_target, Asker, AskerKind};
use crate::transcript::page_answers::PageAnswers;

/// The document's `openQuestions`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenQuestions {
    /// How many forms and question pages are unanswered and not put off For later; the ? pill shows
    /// while this is above zero.
    pub count: u32,
    /// The pill's text: "Previous question ↑".
    pub text: String,
    /// What a screen reader says for it.
    pub accessibility_label: String,
    /// The newest unanswered form's or page's id, or `null` when none waits. The renderers ping the
    /// pill when this changes without `count` dropping: a new question arrived.
    pub newest: Option<String>,
    /// The last jump, or `null` before the first.
    pub jump: Option<OpenQuestionJump>,
    /// Every form, page and chart in the list; the Resources pill shows while this is above zero.
    #[serde(default)]
    pub total: u32,
    /// The list's header detail: "2 unanswered · 1 for later".
    #[serde(default)]
    pub summary: String,
    /// The list's sections in order for the picked filter, each only when it has rows.
    #[serde(default)]
    pub sections: Vec<QuestionListSection>,
    /// The filters over the list, All, Html, Q&A and Charts, each with its count.
    #[serde(default)]
    pub filters: Vec<QuestionListFilter>,
    /// The picked filter's id (`all`, `html`, `qa`, `charts`); `questionListFilter` picks one.
    #[serde(default)]
    pub filter: String,
    /// How many rows `sections` leaves out (`page_sections`); the list ends in a "Show more" row
    /// while this is above zero, and `questionListMore` adds a page.
    #[serde(default)]
    pub remaining: u32,
}

/// One of the list's filters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionListFilter {
    pub id: String,
    pub label: String,
    pub count: u32,
}

/// One folding section of the pill's list.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionListSection {
    /// `unanswered`, `later`, `answered`, `withdrawn`, `canceled`, `pages` or `charts`.
    pub id: String,
    pub title: String,
    pub count: u32,
    /// Newest first.
    pub rows: Vec<QuestionListRow>,
    /// How many of this group's rows the page cut (`page_sections`): its own "Show more" sits under
    /// its rows while this is above zero, and a folded group shows none.
    #[serde(default)]
    pub remaining: u32,
}

/// One form, page or chart in the list; a click sends `jumpToQuestion {id}`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionListRow {
    /// The form's id, the page's id, or the chart's (`questions/content.rs` `chart_id`).
    pub id: String,
    /// `form` (Q&A), `page` (HTML) or `chart`.
    pub kind: String,
    pub title: String,
    /// Where it stands: `waiting` (unanswered), `later` (put off For later, still unanswered),
    /// `answered`, `withdrawn`, `canceled` (removed by the reader), or `other` for a page or chart that asks nothing. The renderers
    /// tell rows apart by it (the icon's colour, the For later marker and button).
    #[serde(default)]
    pub status: String,
    /// The Q-tags it asks.
    pub tags: Vec<String>,
    /// When it was asked, as a message's time reads ("14:03", "yesterday at 09:12"); empty when
    /// the transcript does not say.
    pub time: String,
    /// How long ago (`message_time::age_label`: "now", "4m", "yesterday", "3d") as of this
    /// document; empty when the transcript gives no time. The renderers recompute it from `at_ms`.
    #[serde(default)]
    pub age: String,
    /// The epoch milliseconds it was asked at, when the transcript says.
    #[serde(default)]
    pub at_ms: Option<i64>,
    /// The full date, for a tooltip.
    pub time_title: String,
    /// The thread that posted this page, on a row of a coordinator's Worker pages group.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub thread: String,
}

/// One jump to a question: scroll to `item` once per `revision`, then highlight `target` (the
/// form's id or the page's id).
///
/// CDXC:SessionChat 2026-10-20 WHY:
/// Where a jump lands, on every renderer: a reply that fits on screen is shown whole; a taller one has its end at the bottom of the transcript, unless that leaves the question above the top, and then the question's top sits `questionJumpMargin` (`scroll-bottom.json`) below the transcript's top. The question is the form question asking `question` when the form shows it, else the whole form or page card. The renderers measure the card inside the row after the row is drawn, so long unmeasured rows above it do not matter. Supersedes always aligning a tall reply's end, which left a form near the top of a long reply above the screen (user: "because im being scrolled to the bottom of the message and its sometimes loing").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenQuestionJump {
    pub revision: u64,
    pub item: usize,
    pub target: String,
    /// The tag a Q-tag link named, so the jump lands on that question inside the form; `null` for
    /// the pill's jumps, which land on the card.
    pub question: Option<String>,
}

/// What the pill remembers between clicks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenQuestionsState {
    /// The question the last jump went to, which the next click steps back from.
    pub last: Option<String>,
    pub jump: Option<OpenQuestionJump>,
    /// The list's picked filter; empty is All.
    pub filter: String,
    /// How many pages of the long sections the list shows; 0 is the first page.
    pub pages: u32,
}

const FILTERS: [(&str, &str); 4] = [
    ("all", "All"),
    ("html", "Pages"),
    ("qa", "Forms"),
    ("charts", "Visuals"),
];

/// Whether the filter `filter` shows a row of kind `kind` (`form`, `page`, `chart`).
fn filter_shows(filter: &str, kind: &str) -> bool {
    match filter {
        "html" => kind == "page",
        "qa" => kind == "form",
        "charts" => kind == "chart",
        _ => true,
    }
}

/// How many rows of the long group the list shows before "Show all" (user, 2026-10-29: "show 12 here
/// when first opened please"; 2026-11-10: the newest 30 per group, then one quiet "Show all N" row that
/// loads the rest, after a chat showed 867 resources).
pub const LIST_PAGE: usize = 30;

/// `pages` once "Show all" was asked: more than any list has rows, so nothing is left out.
const ALL_PAGES: u32 = 1_000_000;

/// The group whose rows are always all listed: what waits on the reader (waiting and For later),
/// which is few, and which the cards and the phone's flags read whole.
fn always_whole(section: &str) -> bool {
    section == "unanswered"
}

/// The list paged: Unanswered and For later whole, then each other group's rows up to `pages` pages
/// of `LIST_PAGE` (page 0 is the first), the rest left out. A section keeps
/// its `count` (the full total) even when its rows are cut. Returns the sections and how many rows
/// were left out.
///
/// CDXC:SessionChat 2026-10-28 DECISION:
/// User, of a chat with 356 resources: "make this not show all those resources like this, make it show just 1 scroll worth then i need to click show more to load 1 more page of them then scrolling to the bottom loads more (to make that thing lighter)". The panel builds one page of rows, then a Show more row adds a page, and after that click scrolling to the bottom adds the next by itself (`questionListMore`); changing the filter or closing the panel starts again from one page (`questionListReset`). Counts stay the full totals.
pub fn page_sections(
    sections: Vec<QuestionListSection>,
    pages: u32,
) -> (Vec<QuestionListSection>, u32) {
    // Each paged group has its own budget, so a coordinator's Worker pages are on the first page
    // beside its Earlier list and not behind all of it.
    let budget = (pages as usize + 1).saturating_mul(LIST_PAGE);
    let mut left_out = 0usize;
    let paged = sections
        .into_iter()
        .map(|mut section| {
            if !always_whole(&section.id) {
                let keep = section.rows.len().min(budget);
                left_out += section.rows.len() - keep;
                section.remaining = (section.rows.len() - keep) as u32;
                section.rows.truncate(keep);
            }
            section
        })
        .collect();
    (paged, left_out as u32)
}

/// `questionListMore`: "Show all", the list shows every row.
pub fn more(state: &mut ChatState) {
    state.questions.open_questions.pages = ALL_PAGES;
    state.core.request_publish();
}

/// `questionListReset`: back to the first page (the panel closed).
pub fn reset_pages(state: &mut ChatState) {
    if state.questions.open_questions.pages != 0 {
        state.questions.open_questions.pages = 0;
        state.core.request_publish();
    }
}

/// `questionListFilter {filter}`: the list shows what the filter picks, from its first page.
pub fn pick_filter(state: &mut ChatState, filter: &str) {
    let filter = if FILTERS.iter().any(|(id, _)| *id == filter) && filter != "all" {
        filter.to_string()
    } else {
        String::new()
    };
    if state.questions.open_questions.filter != filter {
        state.questions.open_questions.filter = filter;
        state.questions.open_questions.pages = 0;
        state.core.request_publish();
    }
}

/// The forms and question pages still waiting on the reader, oldest first, less those put off For
/// later (`questions/later.rs`).
pub fn unanswered(state: &ChatState) -> Vec<&Asker> {
    let later = crate::questions::later::later_ids(state);
    waiting(state)
        .into_iter()
        .filter(|asker| !later.contains(&asker.id))
        .collect()
}

/// The forms and question pages no answer, withdrawal or later question has settled yet, For later
/// ones included and removed ones left out, oldest first.
pub fn waiting(state: &ChatState) -> Vec<&Asker> {
    let removed = crate::questions::later::removed_ids(state);
    // A question a work item asks waits on its card, not here (`extras::work_cards::card_tags`).
    let cards = crate::extras::work_cards::card_tags(state);
    let (settled_forms, settled_pages) = settled_by_index(state);
    state
        .transcript_view
        .askers
        .iter()
        .filter(|asker| !asker.answered && !asker.obsolete && !asker.inherited)
        .filter(|asker| !crate::extras::work_cards::covered_by_cards(&asker.tags, &cards))
        .filter(|asker| match asker.kind {
            AskerKind::Form => !settled_forms.contains_key(asker.id.as_str()),
            AskerKind::Page => !settled_pages.contains_key(asker.id.as_str()),
        })
        .filter(|asker| !removed.contains(&asker.id))
        .filter(|asker| {
            asker.kind != AskerKind::Form
                || state
                    .questions
                    .forms
                    .get(&asker.id)
                    .is_none_or(|form| form.send.is_none() && form.sent.is_none())
        })
        .collect()
}

/// The forms and question pages the session's resource index records as answered or withdrawn
/// (`resource_index::settled_by_index`), each kind by id.
///
/// CDXC:SessionChat 2026-11-02 WHY:
/// The ? pill counted questions answered hours ago (8, then 2, with every question answered) and its click went nowhere. The loaded rows can hold a question without its answer: a jump read's window ends at the question's turn, and a reload forgets the form's own send state. A question the loaded rows show unanswered is settled when the index, which reads the whole conversation, records it answered or withdrawn, so the pill's count, the Resources row and the form card agree; the loaded rows still win when they say answered.
fn settled_by_index(
    state: &ChatState,
) -> (
    &std::collections::HashMap<String, &'static str>,
    &std::collections::HashMap<String, &'static str>,
) {
    let view = state.session.resource_index.view();
    (view.settled_forms(), view.settled_pages())
}

/// The forms and pages put off For later that still wait: their cards say For later
/// (the document's `laterQuestions`).
pub fn later_waiting(state: &ChatState) -> Vec<String> {
    let later = crate::questions::later::later_ids(state);
    waiting(state)
        .into_iter()
        .filter(|asker| later.contains(&asker.id))
        .map(|asker| asker.id.clone())
        .collect()
}

/// Where an asker stands (`QuestionListRow::status`), or `None` for one this chat does not list (an
/// unanswered question inherited from the session this one was forked from).
fn status_of(
    asker: &Asker,
    waiting: &[&Asker],
    later: &std::collections::BTreeSet<String>,
    removed: &std::collections::BTreeSet<String>,
    on_the_way: &[String],
    withdrawn_by_index: bool,
) -> Option<&'static str> {
    if waiting.iter().any(|known| known.id == asker.id) {
        return Some(if later.contains(&asker.id) {
            "later"
        } else {
            "waiting"
        });
    }
    if asker.answered || on_the_way.contains(&asker.id) {
        return Some("answered");
    }
    if asker.inherited {
        return None;
    }
    // Neither waiting nor answered: canceled (removed by the reader), withdrawn by the agent, asked again
    // later, or a form whose answers are on their way (answered, as the pill counts it).
    if removed.contains(&asker.id) {
        Some("canceled")
    } else if asker.obsolete || withdrawn_by_index {
        Some("withdrawn")
    } else {
        Some("answered")
    }
}

/// The session's resource index (gxserver's, `resource_index.rs`) as records.
pub(crate) fn indexed(state: &ChatState) -> Vec<ResourceEntry> {
    indexed_entries(state).as_ref().clone()
}

/// The session's resource index as records, read once per index rather than once per use.
///
/// CDXC:SessionChat 2026-11-04 WHY:
/// The ? pill and the Resources list are projected again on every chat event (each streamed token), and each projection read the whole index from JSON two or three times: on a coordinator chat with hundreds of pages and questions that was most of the chat worker's CPU while a visible chat streamed (issues-1027/macos-perf). The records are read when the index changes (`ResourceIndex`) and shared until it changes again, so what the list shows is exactly what reading it again would give.
fn indexed_entries(state: &ChatState) -> std::sync::Arc<Vec<ResourceEntry>> {
    std::sync::Arc::clone(state.session.resource_index.view().records())
}

/// The index records the loaded rows do not hold: older than what the chat has loaded, or from a
/// thread. A loaded row is always the fresher reading of its id.
fn index_only<'a>(state: &ChatState, entries: &'a [ResourceEntry]) -> Vec<&'a ResourceEntry> {
    let mut known: std::collections::HashSet<&str> = state
        .transcript_view
        .askers
        .iter()
        .map(|asker| asker.id.as_str())
        .collect();
    known.extend(
        state
            .questions
            .content
            .items
            .iter()
            .map(|item| item.id.as_str()),
    );
    entries
        .iter()
        .filter(|entry| !known.contains(entry.id.as_str()))
        .collect()
}

/// The status a record reads as here: the reader's For later and Remove marks are this session's
/// (`laterQuestionIds`, `removedQuestionIds`) and apply to the records too.
///
/// CDXC:SessionChat 2026-11-02 WHY:
/// A waiting record with no `offset` reads withdrawn. gxserver records the offset of everything its transcript read holds, so a record without one is from a transcript it no longer reads: nothing there can answer it, "Go to chat" and the Resources modal cannot read it back, and the ? pill counted it while its click only said "Ghostex has not recorded where that sits in the chat yet".
fn indexed_status(
    entry: &ResourceEntry,
    later: &std::collections::BTreeSet<String>,
    removed: &std::collections::BTreeSet<String>,
) -> String {
    match entry.status.as_str() {
        "waiting" if removed.contains(&entry.id) => "canceled",
        "waiting" if entry.offset.is_none() && entry.thread.is_none() => "withdrawn",
        "waiting" if later.contains(&entry.id) => "later",
        "withdrawn" if removed.contains(&entry.id) => "canceled",
        "" => "other",
        other => other,
    }
    .to_string()
}

/// How many forms and question pages the index says wait on the reader that the loaded rows do not
/// hold, not counting the ones put off For later or removed.
fn indexed_waiting<'a>(state: &ChatState, entries: &'a [ResourceEntry]) -> Vec<&'a ResourceEntry> {
    let later = crate::questions::later::later_ids(state);
    let removed = crate::questions::later::removed_ids(state);
    let on_the_way: Vec<String> = answers_on_the_way(state)
        .into_iter()
        .map(|answers| answers.page_id)
        .collect();
    index_only(state, entries)
        .into_iter()
        .filter(|entry| entry.thread.is_none())
        .filter(|entry| indexed_status(entry, &later, &removed) == "waiting")
        .filter(|entry| !on_the_way.contains(&entry.id))
        .collect()
}

fn indexed_row(
    entry: &ResourceEntry,
    status: String,
    context: &ChatContext,
    labels: bool,
) -> QuestionListRow {
    let (time, age, time_title) = time_fields(entry.at_ms, context, labels);
    let fallback = match entry.kind.as_str() {
        "form" => "Ghostex form",
        "chart" => "Ghostex visual",
        _ => "Ghostex page",
    };
    QuestionListRow {
        id: entry.id.clone(),
        kind: entry.kind.clone(),
        title: if entry.title.is_empty() {
            fallback.to_string()
        } else {
            plain_words(&entry.title)
        },
        status,
        tags: entry.tags.clone(),
        time,
        age,
        at_ms: entry.at_ms,
        time_title,
        thread: entry
            .thread
            .as_ref()
            .map(|thread| thread.title.clone())
            .unwrap_or_default(),
    }
}

/// The list's two groups: what waits on the reader (Unanswered, then those put off For later), and
/// everything else.
///
/// CDXC:SessionChat 2026-10-29 DECISION:
/// User, of the Chat Resources list: "show 12 here when first opened please. and don't split pages from answered from withdrawn, just different icon or icon color is enough. Just split unanswered from the others". The list has two groups only: Unanswered (the waiting questions, then the For later ones, which still need an answer and carry their own marker and button) and "Earlier", every answered form, page, chart and withdrawn question in one list, newest first, told apart by the row's icon and `status` (a withdrawn row's icon is dimmed). "Earlier" has no header while nothing is unanswered. The first page is 12 rows (`LIST_PAGE`). Supersedes the 2026-10-27 sections (Unanswered, For later, Answered, Withdrawn, Pages, Charts).
const GROUPS: [(&str, &str); 3] = [
    ("unanswered", "Unanswered"),
    ("earlier", "Earlier"),
    ("threads", "Worker pages"),
];

/// The pill's list: every form, page and chart in its group, all kinds. Earlier is newest first;
/// Unanswered is oldest first, so the waiting questions read in the order they were asked.
///
/// CDXC:SessionChat 2026-10-30 DECISION:
/// User, of the Chat Resources panel's Unanswered group (Q194, Q192, Q191, Q190 from the top): it lists oldest first (Q190, Q191, Q192, Q194), "so the questions read in the order they were asked". Only Unanswered flips (waiting ones, then the For later ones, each oldest first); Earlier stays newest first. The ? pill's click order (`previous`) and its `newest` ping id do not read this list.
pub fn list(state: &ChatState, context: &ChatContext) -> Vec<QuestionListSection> {
    list_rows(state, context, true)
}

/// `list`, with each row's time fields only when `labels` asks for them.
fn list_rows(state: &ChatState, context: &ChatContext, labels: bool) -> Vec<QuestionListSection> {
    let waiting = waiting(state);
    let later = crate::questions::later::later_ids(state);
    let removed = crate::questions::later::removed_ids(state);
    let on_the_way: Vec<String> = answers_on_the_way(state)
        .into_iter()
        .map(|answers| answers.page_id)
        .collect();
    let (settled_forms, settled_pages) = settled_by_index(state);
    let mut placed: Vec<(Option<i64>, usize, QuestionListRow)> = state
        .transcript_view
        .askers
        .iter()
        .enumerate()
        .filter_map(|(index, asker)| {
            let settled = match asker.kind {
                AskerKind::Form => &settled_forms,
                AskerKind::Page => &settled_pages,
            };
            let withdrawn = settled.get(asker.id.as_str()) == Some(&"withdrawn");
            let status = status_of(asker, &waiting, &later, &removed, &on_the_way, withdrawn)?;
            let mut row = list_row(asker, context, labels);
            row.status = status.to_string();
            Some((asker.at, index, row))
        })
        .collect();
    let askers = state.transcript_view.askers.len();
    for (index, item) in state.questions.content.items.iter().enumerate() {
        let mut row = content_row(item, context, labels);
        row.status = "other".to_string();
        placed.push((item.at, askers + index, row));
    }
    // What the whole conversation held beyond the loaded rows (the session's resource index), and
    // the pages the coordinator's threads posted.
    let mut thread_pages: Vec<QuestionListRow> = Vec::new();
    let entries = indexed_entries(state);
    for entry in index_only(state, &entries) {
        let mut status = indexed_status(entry, &later, &removed);
        // Answers sent from the modal wait in the queue: the form reads answered at once.
        if status == "waiting" && on_the_way.contains(&entry.id) {
            status = "answered".to_string();
        }
        let row = indexed_row(entry, status, context, labels);
        if entry.thread.is_some() {
            thread_pages.push(row);
        } else {
            placed.push((entry.at_ms, 0, row));
        }
    }
    // Newest first; a row the transcript gave no time sorts by its place.
    placed.sort_by(|(a_at, a_index, _), (b_at, b_index, _)| {
        b_at.cmp(a_at).then(b_index.cmp(a_index))
    });
    thread_pages.sort_by(|a, b| b.at_ms.cmp(&a.at_ms));
    // Waiting first, then the put-off ones, each oldest first (`placed` is newest first).
    let (mut waiting_rows, mut later_rows, mut earlier) = (Vec::new(), Vec::new(), Vec::new());
    for (_, _, row) in placed {
        match row.status.as_str() {
            "waiting" => waiting_rows.push(row),
            "later" => later_rows.push(row),
            _ => earlier.push(row),
        }
    }
    waiting_rows.reverse();
    later_rows.reverse();
    let mut unanswered = waiting_rows;
    unanswered.append(&mut later_rows);
    GROUPS
        .iter()
        .zip([unanswered, earlier, thread_pages])
        .filter_map(|((id, title), rows)| {
            (!rows.is_empty()).then(|| QuestionListSection {
                id: id.to_string(),
                title: title.to_string(),
                count: rows.len() as u32,
                rows,
                remaining: 0,
            })
        })
        .collect()
}

fn age_of(at: Option<i64>, context: &ChatContext) -> String {
    at.map_or_else(String::new, |at| {
        crate::transcript::message_time::age_label(context.now_millis(), at)
    })
}

/// A row's `time`, `age` and `time_title`, all read from when it was asked; empty without `labels`.
fn time_fields(at: Option<i64>, context: &ChatContext, labels: bool) -> (String, String, String) {
    if !labels {
        return Default::default();
    }
    let time = crate::transcript::message_time::message_time(at, context);
    (
        time["label"].as_str().unwrap_or_default().to_string(),
        age_of(at, context),
        time["title"].as_str().unwrap_or_default().to_string(),
    )
}

/// Fills the time fields of a row listed without them (`list_rows`), exactly as listing it with
/// them would have.
fn label_row(row: &mut QuestionListRow, context: &ChatContext) {
    (row.time, row.age, row.time_title) = time_fields(row.at_ms, context, true);
}

fn list_row(asker: &Asker, context: &ChatContext, labels: bool) -> QuestionListRow {
    let (time, age, time_title) = time_fields(asker.at, context, labels);
    let title = if asker.title.is_empty() {
        match asker.kind {
            AskerKind::Form => "Ghostex form".to_string(),
            AskerKind::Page => "Ghostex page".to_string(),
        }
    } else {
        plain_words(&asker.title)
    };
    QuestionListRow {
        id: asker.id.clone(),
        kind: match asker.kind {
            AskerKind::Form => "form",
            AskerKind::Page => "page",
        }
        .to_string(),
        title,
        status: String::new(),
        tags: asker.tags.clone(),
        time,
        age,
        at_ms: asker.at,
        time_title,
        thread: String::new(),
    }
}

fn content_row(
    item: &crate::questions::content::ContentItem,
    context: &ChatContext,
    labels: bool,
) -> QuestionListRow {
    use crate::transcript::question_forms::ContentKind;
    let (time, age, time_title) = time_fields(item.at, context, labels);
    let (kind, fallback) = match item.kind {
        ContentKind::Chart => ("chart", "Ghostex visual"),
        _ => ("page", "Ghostex page"),
    };
    QuestionListRow {
        id: item.id.clone(),
        kind: kind.to_string(),
        title: if item.title.is_empty() {
            fallback.to_string()
        } else {
            plain_words(&item.title)
        },
        status: String::new(),
        tags: Vec::new(),
        time,
        age,
        at_ms: item.at,
        time_title,
        thread: String::new(),
    }
}

/// The forms and pages whose answers are on their way but not in the transcript yet: a form whose
/// answers are sending or queued (as `unanswered` reads it), and a page or form whose answers wait
/// in the prompt queue. Their Q-tags read answered already, as the pill already skips them.
pub fn answers_on_the_way(state: &ChatState) -> Vec<PageAnswers> {
    let mut found: Vec<PageAnswers> = state
        .questions
        .forms
        .iter()
        .filter_map(|(id, form)| {
            let answers = form
                .send
                .as_ref()
                .map(|send| &send.answers)
                .or(form.sent.as_ref())?;
            Some(PageAnswers {
                page_id: id.clone(),
                title: String::new(),
                answers: answers.clone(),
            })
        })
        .collect();
    for entry in state.session.queue_prompts.iter().flatten() {
        let text = entry["prompt"]["text"]
            .as_str()
            .or_else(|| entry["text"].as_str())
            .unwrap_or_default();
        if !text.contains("<ghostex-page-answers") {
            continue;
        }
        if let Some((answers, _)) = crate::transcript::page_answers::split_page_answers(text) {
            if !found.iter().any(|known| known.page_id == answers.page_id) {
                found.push(answers);
            }
        }
    }
    found
}

/// Everything [`list_rows`] reads, kept beside the list it built (`document::AssembleCache`).
///
/// CDXC:SessionChat 2026-11-09 WHY:
/// User: "ghostex app gets slower with time until it becomes super slow". Every event of the Main Coordinator chat listed its ~1,300 forms, pages and charts again for the ? pill and the Resources panel, a fifth of an idle event, though the list only moves with the questions themselves. The rows without their time fields read no clock, so they are kept while what they are built from is equal; the page shown is cut from them and given its time fields on every event, as before.
#[derive(Clone, Debug, PartialEq)]
pub struct ListInputs {
    askers: Vec<Asker>,
    questions: crate::questions::forms::QuestionsInputs,
    content: Vec<crate::questions::content::ContentItem>,
    queue_prompts: Option<Vec<serde_json::Value>>,
    resource_index: crate::questions::resource_index::ResourceIndex,
    later_question_ids: Vec<String>,
    removed_question_ids: Vec<String>,
}

impl ListInputs {
    fn of(state: &ChatState) -> Self {
        Self {
            askers: state.transcript_view.askers.clone(),
            questions: crate::questions::forms::QuestionsInputs::of(&state.questions),
            content: state.questions.content.items.clone(),
            queue_prompts: state.session.queue_prompts.clone(),
            resource_index: state.session.resource_index.clone(),
            later_question_ids: state.session.later_question_ids.clone(),
            removed_question_ids: state.session.removed_question_ids.clone(),
        }
    }

    fn matches(&self, state: &ChatState) -> bool {
        self.askers == state.transcript_view.askers
            && self.questions.matches(&state.questions)
            && self.content == state.questions.content.items
            && self.queue_prompts == state.session.queue_prompts
            && self.resource_index == state.session.resource_index
            && self.later_question_ids == state.session.later_question_ids
            && self.removed_question_ids == state.session.removed_question_ids
    }
}

/// A kept unlabelled list and the inputs it was built from.
pub type KeptList = (ListInputs, std::sync::Arc<Vec<QuestionListSection>>);

/// The document's `openQuestions`.
pub fn project(state: &ChatState, context: &ChatContext) -> OpenQuestions {
    let asks = crate::extras::work_cards::open_asks(state, context);
    project_with(state, context, &mut None, &asks)
}

/// [`project`], listing the rows again only when what they are built from changed.
///
/// A coordinator's open work item asks (`extras::work_cards`) count as unanswered questions too:
/// the pill shows while one waits, and its newest one is `newest` when it is newer than every form
/// and page (the user's 2026-11-10 decision there).
pub fn project_with(
    state: &ChatState,
    context: &ChatContext,
    kept: &mut Option<KeptList>,
    asks: &[crate::extras::work_cards::OpenAsk],
) -> OpenQuestions {
    let open = unanswered(state);
    // The loaded rows' unanswered questions, plus the ones older than what the chat has loaded,
    // plus the coordinator's open asks.
    let count =
        (open.len() + indexed_waiting(state, &indexed_entries(state)).len() + asks.len()) as u32;
    let filter = state.questions.open_questions.filter.as_str();
    /*
    CDXC:SessionChat 2026-11-04 WHY:
    This runs on every chat event (each streamed token) to learn whether the document changed, and it listed every form, page and chart with its time labels and copied each row twice to keep the first page: on a coordinator chat with hundreds of them that was the chat worker's main cost while a visible chat streamed (issues-1027/macos-perf). Rows are listed without their time fields, moved rather than copied, and only the rows on the page get their time fields (`label_row`), which reads them exactly as listing them would.
    */
    let all = match kept.as_ref().filter(|(inputs, _)| inputs.matches(state)) {
        Some((_, all)) => std::sync::Arc::clone(all),
        None => {
            let all = std::sync::Arc::new(list_rows(state, context, false));
            *kept = Some((ListInputs::of(state), std::sync::Arc::clone(&all)));
            all
        }
    };
    let total = all.iter().map(|section| section.count).sum();
    let later = all
        .iter()
        .flat_map(|section| &section.rows)
        .filter(|row| row.status == "later")
        .count();
    let summary = [
        Some(format!("{count} unanswered")),
        (later > 0).then(|| format!("{later} for later")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" \u{b7} ");
    let filters = FILTERS
        .iter()
        .map(|(id, label)| QuestionListFilter {
            id: id.to_string(),
            label: label.to_string(),
            count: all
                .iter()
                .flat_map(|section| &section.rows)
                .filter(|row| filter_shows(id, &row.kind))
                .count() as u32,
        })
        .collect();
    // `page_sections` over the filtered sections, copying only the rows the page keeps.
    let budget = (state.questions.open_questions.pages as usize + 1) * LIST_PAGE;
    let mut remaining = 0u32;
    let mut sections: Vec<QuestionListSection> = all
        .iter()
        .filter_map(|section| {
            let shown = || {
                section
                    .rows
                    .iter()
                    .filter(|row| filter_shows(filter, &row.kind))
            };
            let count = shown().count();
            if count == 0 {
                return None;
            }
            let keep = if always_whole(&section.id) {
                count
            } else {
                count.min(budget)
            };
            remaining += (count - keep) as u32;
            Some(QuestionListSection {
                id: section.id.clone(),
                title: section.title.clone(),
                count: count as u32,
                rows: shown().take(keep).cloned().collect(),
                remaining: (count - keep) as u32,
            })
        })
        .collect();
    // "Earlier" has a header only beside another group.
    for row in sections
        .iter_mut()
        .flat_map(|section| section.rows.iter_mut())
    {
        label_row(row, context);
    }
    if sections.len() == 1 {
        for section in &mut sections {
            section.title.clear();
        }
    }
    OpenQuestions {
        count,
        text: "Previous question \u{2191}".to_string(),
        accessibility_label: match count {
            0 => "No unanswered questions".to_string(),
            1 => "1 unanswered question: go to it".to_string(),
            _ => format!("{count} unanswered questions: go to the previous one"),
        },
        newest: {
            let form = open.last();
            let ask = asks.last();
            match (form, ask) {
                (Some(form), Some(ask)) if form.at.is_some_and(|at| at > ask.at) => {
                    Some(form.id.clone())
                }
                (_, Some(ask)) => Some(ask.key.clone()),
                (form, None) => form.map(|asker| asker.id.clone()),
            }
        },
        jump: state.questions.open_questions.jump.clone(),
        total,
        summary,
        filters,
        filter: if filter.is_empty() {
            "all".to_string()
        } else {
            filter.to_string()
        },
        sections,
        remaining,
    }
}

/// `previousQuestion`: the pill's click. It goes to the unanswered question before the one it went
/// to last (wrapping to the newest after the oldest), or to the newest when it has not gone
/// anywhere yet or the last one is no longer waiting. For later ones are left out: the pill is not
/// on screen without an unanswered question, and Resources reaches the rest.
///
/// CDXC:SessionChat 2026-10-14 WHY:
/// Starting again from the newest whenever the reader sat at the bottom was tried and dropped: the newest question usually sits at the bottom, so the second click landed on it again instead of stepping back.
pub fn previous(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let open: Vec<Asker> = unanswered(state).into_iter().cloned().collect();
    // A coordinator's open work item asks are stops too, in time order among the forms and pages:
    // each is shown in the Work panel, the chat staying where it is.
    let asks = crate::extras::work_cards::open_asks(state, context);
    // Every stop, oldest first: (time, the form, page or ask, whether it is an ask).
    let mut stops: Vec<(i64, String, bool)> = open
        .iter()
        .map(|asker| (asker.at.unwrap_or(i64::MIN), asker.id.clone(), false))
        .chain(asks.iter().map(|ask| (ask.at, ask.key.clone(), true)))
        .collect();
    if stops.is_empty() {
        // The only questions waiting are older than the loaded rows (the pill counts them): the
        // newest of them opens in the popup, read back from the transcript.
        let entries = indexed_entries(state);
        let newest = indexed_waiting(state, &entries)
            .into_iter()
            .max_by_key(|entry| entry.at_ms.unwrap_or(i64::MIN))
            .map(|entry| entry.id.clone());
        return match newest {
            Some(id) => show(state, context, &id),
            None => Vec::new(),
        };
    }
    stops.sort_by_key(|stop| stop.0);
    let last = state.questions.open_questions.last.as_deref();
    let at = stops.iter().position(|stop| Some(stop.1.as_str()) == last);
    let (_, target, is_ask) = stops[step(stops.len(), at)].clone();
    state.questions.open_questions.last = Some(target.clone());
    if is_ask {
        return crate::extras::coordinator_board::focus_row(state, &target);
    }
    show(state, context, &target)
}

/// Shows a form, chart or question page without moving the chat: a form or chart opens in the
/// floating popup (`resource_modal.rs`), read back from the transcript when it is older than the
/// loaded rows; a page opens as itself, where the reader's Open chat pages in setting says.
///
/// CDXC:SessionChat 2026-11-10 DECISION:
/// User: "if i click on anything that needs me to see the question card don't scroll me up to it.
/// no more scrolling up to stuff please show it to me in a popup instead please (we already
/// implemented this). if it's an inline html just open that inline html. i shouldn't be scrolled up
/// to different places in the chat anymore please. also same when i click on a inline html from the
/// resources pill in the composer". The ? pill and its stepping, a Q-tag link, a row of the
/// Resources list, the Work panel's and the dock's questions and the In progress list all come
/// here; nothing scrolls the transcript to a question any more. Supersedes the jumps of the
/// 2026-10-14 and 2026-11-05 decisions in this file.
pub fn show(state: &mut ChatState, context: &ChatContext, id: &str) -> Vec<Effect> {
    let _ = context;
    if let Some(url) = page_url(state, id) {
        return vec![Effect::Open(crate::effect::OpenTarget::Url {
            url,
            external: false,
        })];
    }
    let effects = crate::questions::resource_modal::open(state, id);
    if state
        .questions
        .resource_modal
        .as_ref()
        .is_some_and(|modal| modal.id == id)
    {
        return effects;
    }
    vec![Effect::Toast {
        level: "info".to_string(),
        message: "That question isn't in this chat any more.".to_string(),
    }]
}

/// A question page's address, by its id: from the reply that holds it when it is loaded, else
/// from the session's resource index (a thread's page read-only).
fn page_url(state: &ChatState, id: &str) -> Option<String> {
    let loaded = state
        .transcript_view
        .askers
        .iter()
        .find(|asker| {
            asker.id == id
                && matches!(
                    asker.kind,
                    crate::transcript::open_questions::AskerKind::Page
                )
        })
        .map(|asker| asker.message_id.clone())
        .or_else(|| {
            state
                .questions
                .content
                .items
                .iter()
                .find(|item| {
                    item.id == id
                        && item.kind == crate::transcript::question_forms::ContentKind::Page
                })
                .map(|item| item.message_id.clone())
        });
    if let Some(message_id) = loaded {
        let message = state
            .messages
            .composed
            .iter()
            .find(|message| message.id == message_id)?;
        let markdown = crate::transcript::prose::prose_markdown(&message.blocks);
        return crate::transcript::question_forms::content_fences(&markdown)
            .into_iter()
            .filter(|fence| fence.kind == crate::transcript::question_forms::ContentKind::Page)
            .filter_map(|fence| ghostex_gx_visual::page_reference(&fence.body))
            .find(|page| page.page_id().map_or(page.url.as_str(), |page_id| page_id) == id)
            .map(|page| page.url.clone());
    }
    let entry = indexed(state).into_iter().find(|entry| entry.id == id)?;
    if entry.kind != "page" {
        return None;
    }
    let url = entry.url?;
    Some(if entry.thread.is_some() {
        read_only_url(&url)
    } else {
        url
    })
}

/// Which of `count` unanswered questions (oldest first) the pill goes to, given where the last
/// jump went (`at`).
fn step(count: usize, at: Option<usize>) -> usize {
    match at {
        Some(index) => index.checked_sub(1).unwrap_or(count - 1),
        None => count - 1,
    }
}

/// `jumpToQuestion {id}`: a row of the pill's list (a form or page, answered or not, or a chart),
/// shown where it is without moving the chat (`show`).
pub fn jump_to_id(state: &mut ChatState, context: &ChatContext, id: &str) -> Vec<Effect> {
    state.questions.open_questions.last = Some(id.to_string());
    show(state, context, id)
}

/// `openResource {id}`: a row of the list clicked. A form or chart opens in the floating modal and
/// the chat stays where it is (`resource_modal.rs`); a page opens as it always did (an older one,
/// or a thread's, by its address; a loaded one by scrolling to its card).
pub fn open_resource(state: &mut ChatState, context: &ChatContext, id: &str) -> Vec<Effect> {
    show(state, context, id)
}


/// The query that opens a published page read-only (server/src/visual_pages/serve.rs): its
/// answers are not sent, because they would go to the thread that posted it and not to the
/// coordinator the reader is talking to.
pub const READ_ONLY_QUERY: &str = "gx-readonly=1";

fn read_only_url(url: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}{READ_ONLY_QUERY}")
}

/// A Q-tag link's click (`#gx-question=Q80`, `transcript/open_questions.rs`): the latest form or
/// page that asked the tag, answered or not.
///
/// CDXC:SessionChat 2026-11-05 DECISION:
/// User: "clicking Q1289 even tho the question is right there infront of me takes me up to the first time this q was asked ... shouldn't jump me up there". A tag pill on the question it names (`;here`, `HERE_SUFFIX`) never scrolls: the desktop and the phone open the Reply popover for that question instead, so the reader can answer in their own words. A tag pill elsewhere (a later message, the composer) goes to the newest reply that asks the tag: a one-click question asked again in the same words is one form, whose asker keeps the reply that first asked it, so the newest reply holding it is looked up here.
pub fn jump_to_tag(state: &mut ChatState, context: &ChatContext, tag: &str) -> Vec<Effect> {
    // A tag a work item asks is shown in the Work panel (`show`'s decision: nothing scrolls).
    if crate::extras::work_cards::card_tags(state).contains(&tag.to_uppercase()) {
        if let Some(key) = crate::extras::work_cards::ask_key_of_tag(state, tag) {
            return crate::extras::coordinator_board::focus_row(state, &key);
        }
    }
    if let Some((_, form_id)) = newest_one_click_ask(state, tag) {
        return show(state, context, &form_id);
    }
    let Some(asker) = tag_target(&state.transcript_view.askers, tag).cloned() else {
        return vec![Effect::Toast {
            level: "info".to_string(),
            message: format!("{tag} isn't asked in the loaded part of this chat."),
        }];
    };
    show(state, context, &asker.id)
}

/// The newest reply asking `tag` as a one-click question, with that question's form id, when it is
/// newer than the reply of the tag's latest asker.
fn newest_one_click_ask(state: &ChatState, tag: &str) -> Option<(String, String)> {
    use crate::transcript::inline_answers::{inline_questions, ANSWER_SCHEME};
    let asker = tag_target(&state.transcript_view.askers, tag);
    for message in state.messages.composed.iter().rev() {
        if asker.is_some_and(|asker| asker.message_id == message.id) {
            return None;
        }
        if message.role != ghostex_gx_protocol::ChatRole::Assistant {
            continue;
        }
        let prose = crate::transcript::prose::prose_markdown(&message.blocks);
        if !prose.contains(ANSWER_SCHEME) {
            continue;
        }
        if let Some(question) = inline_questions(&prose)
            .into_iter()
            .find(|question| question.tag.eq_ignore_ascii_case(tag))
        {
            return Some((message.id.clone(), question.form().id));
        }
    }
    None
}

#[cfg(test)]
pub(crate) fn newest_one_click_ask_for_test(
    state: &ChatState,
    tag: &str,
) -> Option<(String, String)> {
    newest_one_click_ask(state, tag)
}


/// Scrolls to the row holding `message_id` and flashes `target` there; false when no row holds it.
pub(crate) fn jump_to_row(
    state: &mut ChatState,
    context: &ChatContext,
    message_id: &str,
    target: &str,
    question: Option<String>,
) -> bool {
    let rows = crate::transcript::rows(state, context);
    let Some(item) = crate::composer::files_links::row_of(&rows, message_id, None) else {
        return false;
    };
    let revision = state
        .questions
        .open_questions
        .jump
        .as_ref()
        .map_or(1, |jump| jump.revision + 1);
    let open_questions = &mut state.questions.open_questions;
    open_questions.last = Some(target.to_string());
    open_questions.jump = Some(OpenQuestionJump {
        revision,
        item,
        target: target.to_string(),
        question,
    });
    state.core.request_publish();
    true
}

/// The tag a Markdown link names, when it is a Q-tag link (`#gx-question=Q80;open`).
pub fn link_tag(href: &str) -> Option<&str> {
    href.trim()
        .strip_prefix(crate::transcript::open_questions::QUESTION_LINK_PREFIX)
        .map(|rest| rest.split(';').next().unwrap_or_default().trim())
        .filter(|tag| !tag.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pill_steps_back_from_the_newest_and_wraps() {
        assert_eq!(step(3, None), 2, "the first click goes to the newest");
        assert_eq!(step(3, Some(2)), 1);
        assert_eq!(step(3, Some(1)), 0);
        assert_eq!(
            step(3, Some(0)),
            2,
            "after the oldest it wraps to the newest"
        );
    }

    fn asker(id: &str, kind: AskerKind, at: Option<i64>) -> Asker {
        Asker {
            id: id.to_string(),
            kind,
            message_id: format!("m-{id}"),
            tags: vec![format!("Q-{id}")],
            answered: false,
            obsolete: false,
            withdrawn: Vec::new(),
            keys: Vec::new(),
            skipped: Vec::new(),
            superseded: Vec::new(),
            inherited: false,
            said: Vec::new(),
            title: format!("About {id}"),
            at,
        }
    }

    fn section<'a>(pill: &'a OpenQuestions, id: &str) -> Vec<&'a str> {
        pill.sections
            .iter()
            .find(|section| section.id == id)
            .map(|section| section.rows.iter().map(|row| row.id.as_str()).collect())
            .unwrap_or_default()
    }

    /// The rows of `pill` with this `status`, across both groups, in the list's order.
    fn with_status<'a>(pill: &'a OpenQuestions, status: &str) -> Vec<&'a str> {
        pill.sections
            .iter()
            .flat_map(|section| &section.rows)
            .filter(|row| row.status == status)
            .map(|row| row.id.as_str())
            .collect()
    }

    fn pool_ids(state: &ChatState) -> Vec<String> {
        unanswered(state)
            .iter()
            .map(|asker| asker.id.clone())
            .collect()
    }

    /// A chat whose transcript holds one assistant message per asker, so a click can find the row.
    fn chat_asking(askers: Vec<Asker>) -> ChatState {
        let mut state = ChatState::default();
        for asker in &askers {
            state.messages.composed.push(
                serde_json::from_value(serde_json::json!({
                    "id": asker.message_id,
                    "role": "assistant",
                    "blocks": [{"type": "text", "text": "A question."}],
                    "timestamp": 1_700_000_000_000_i64,
                    "source": "claude",
                }))
                .unwrap(),
            );
        }
        state.transcript_view.askers = askers;
        state
    }

    fn answered(id: &str, kind: AskerKind, at: i64) -> Asker {
        Asker {
            answered: true,
            ..asker(id, kind, Some(at))
        }
    }

    /// The question the last click went to.
    fn clicked(state: &mut ChatState) -> Option<String> {
        previous(state, &ChatContext::default());
        state.questions.open_questions.last.clone()
    }

    #[test]
    fn the_pill_steps_through_unanswered_questions_only() {
        let mut state = ChatState::default();
        let mut withdrawn = asker("gone", AskerKind::Page, Some(1_500));
        withdrawn.obsolete = true;
        state
            .transcript_view
            .askers
            .push(answered("done", AskerKind::Form, 1_000));
        state.transcript_view.askers.push(withdrawn);
        state
            .transcript_view
            .askers
            .push(asker("open", AskerKind::Form, Some(2_000)));
        assert_eq!(pool_ids(&state), ["open"]);
        assert_eq!(step(2, None), 1);
        assert_eq!(step(2, Some(1)), 0);
        assert_eq!(step(2, Some(0)), 1, "it wraps");
    }

    #[test]
    fn a_chat_with_only_answered_questions_has_no_pill() {
        let mut state = chat_asking(vec![
            answered("a", AskerKind::Form, 1_000),
            answered("b", AskerKind::Page, 2_000),
        ]);
        state.transcript_view.askers.push({
            let mut gone = asker("gone", AskerKind::Page, Some(3_000));
            gone.obsolete = true;
            gone
        });
        let pill = project(&state, &ChatContext::default());
        assert_eq!(
            pill.count, 0,
            "nothing waits, so the renderers hide the pill"
        );
        assert_eq!(pill.total, 3, "Resources still lists them all");
        assert_eq!(clicked(&mut state), None);
        assert!(state.questions.open_questions.jump.is_none());
    }

    #[test]
    fn the_index_settles_loaded_questions_whose_answer_is_outside_the_window() {
        let mut state = chat_asking(vec![
            asker("form", AskerKind::Form, Some(1_000)),
            asker("page", AskerKind::Page, Some(2_000)),
            asker("asked-again", AskerKind::Form, Some(3_000)),
        ]);
        state.session.resource_index = vec![
            indexed("form", "form", "answered", 1_000),
            indexed("page", "page", "answered", 2_000),
            indexed("asked-again", "form", "withdrawn", 3_000),
        ]
        .into();
        // No form send state survives a reload, and no answer or withdrawal is in this window.
        assert!(state.questions.forms.is_empty());
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.count, 0);
        assert_eq!(pill.total, 3);
        assert!(section(&pill, "unanswered").is_empty());
        assert_eq!(with_status(&pill, "answered"), vec!["page", "form"]);
        assert_eq!(with_status(&pill, "withdrawn"), vec!["asked-again"]);
        assert!(previous(&mut state, &ChatContext::default()).is_empty());
        assert!(state.questions.open_questions.jump.is_none());
    }

    #[test]
    fn a_counted_question_without_a_rendered_row_uses_its_indexed_offset() {
        let mut state = ChatState::default();
        state
            .transcript_view
            .askers
            .push(asker("old", AskerKind::Form, Some(1_000)));
        state.session.resource_index = vec![indexed("old", "form", "waiting", 1_000)].into();
        assert_eq!(project(&state, &ChatContext::default()).count, 1);
        let effects = previous(&mut state, &ChatContext::default());
        assert!(!effects.is_empty(), "the click reads the form back for the popup");
        assert!(state
            .questions
            .resource_modal
            .as_ref()
            .is_some_and(|modal| modal.id == "old" && modal.loading));
        assert!(state.questions.resource_go.is_none(), "nothing scrolls");
    }

    #[test]
    fn a_chat_where_only_for_later_questions_wait_has_no_pill() {
        let mut state = chat_asking(vec![
            answered("a", AskerKind::Form, 1_000),
            asker("later", AskerKind::Form, Some(2_000)),
        ]);
        crate::questions::later::set(&mut state, "later", true);
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.count, 0);
        assert_eq!(pill.total, 2);
        assert_eq!(
            clicked(&mut state),
            None,
            "For later ones are reached from Resources"
        );
    }

    #[test]
    fn a_click_goes_only_to_the_unanswered_question_among_answered_and_for_later() {
        let mut state = chat_asking(vec![
            answered("a", AskerKind::Form, 1_000),
            asker("later", AskerKind::Form, Some(1_500)),
            asker("open", AskerKind::Form, Some(2_000)),
            answered("b", AskerKind::Page, 3_000),
        ]);
        crate::questions::later::set(&mut state, "later", true);
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.count, 1);
        assert_eq!(clicked(&mut state).as_deref(), Some("open"));
        assert_eq!(
            clicked(&mut state).as_deref(),
            Some("open"),
            "the next click lands there again"
        );
        assert!(state.questions.open_questions.jump.is_none(), "nothing scrolls");
    }

    #[test]
    fn the_pill_shows_while_a_question_is_unanswered() {
        let mut state = ChatState::default();
        state
            .transcript_view
            .askers
            .push(asker("form", AskerKind::Form, None));
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.count, 1);
        assert_eq!(pill.total, 1);
        assert_eq!(pill.text, "Previous question \u{2191}");
    }

    #[test]
    fn the_for_later_button_marks_a_row_and_unmarks_it() {
        let mut state = ChatState::default();
        state
            .transcript_view
            .askers
            .push(asker("a", AskerKind::Form, Some(1_000)));
        let list = |state: &ChatState| project(state, &ChatContext::default());
        assert_eq!(section(&list(&state), "unanswered"), vec!["a"]);
        // The row's button sends `questionForLater {id, later: true}`: it shows at once.
        crate::questions::later::set(&mut state, "a", true);
        let pill = list(&state);
        // It stays in the Unanswered group, now marked For later.
        assert_eq!(section(&pill, "unanswered"), vec!["a"]);
        assert_eq!(with_status(&pill, "later"), vec!["a"]);
        assert!(with_status(&pill, "waiting").is_empty());
        assert_eq!(pill.count, 0, "a put-off question leaves the count");
        // "Back to open": `later: false`.
        crate::questions::later::set(&mut state, "a", false);
        let pill = list(&state);
        assert_eq!(section(&pill, "unanswered"), vec!["a"]);
        assert_eq!(with_status(&pill, "waiting"), vec!["a"]);
        assert!(with_status(&pill, "later").is_empty());
    }

    #[test]
    fn a_row_carries_how_long_ago_it_was_asked() {
        let mut state = ChatState::default();
        state
            .transcript_view
            .askers
            .push(asker("a", AskerKind::Form, Some(1_000)));
        state
            .transcript_view
            .askers
            .push(asker("b", AskerKind::Form, None));
        let pill = project(&state, &ChatContext::default());
        let row = |id: &str| {
            pill.sections
                .iter()
                .flat_map(|section| &section.rows)
                .find(|row| row.id == id)
                .unwrap()
        };
        assert_eq!(row("a").at_ms, Some(1_000));
        assert!(!row("a").age.is_empty());
        assert_eq!((row("b").at_ms, row("b").age.as_str()), (None, ""));
    }

    #[test]
    fn for_later_leaves_the_count_but_stays_in_the_list() {
        let mut state = ChatState::default();
        state
            .transcript_view
            .askers
            .push(asker("a", AskerKind::Page, Some(1_000)));
        state
            .transcript_view
            .askers
            .push(asker("b", AskerKind::Form, Some(2_000)));
        state.session.later_question_ids = vec!["a".to_string()];
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.count, 1, "only b waits");
        assert_eq!(pill.newest.as_deref(), Some("b"));
        assert_eq!(pill.summary, "1 unanswered \u{b7} 1 for later");
        // Waiting first, then the put-off ones, in one Unanswered group.
        assert_eq!(section(&pill, "unanswered"), vec!["b", "a"]);
        assert_eq!(with_status(&pill, "later"), vec!["a"]);
        assert_eq!(later_waiting(&state), vec!["a".to_string()]);
        // Everything put off: no count, no ping, but the list stays reachable.
        state.session.later_question_ids.push("b".to_string());
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.count, 0);
        assert_eq!(pill.newest, None);
        assert_eq!(pill.total, 2);
        // Stepping skips For later questions.
        assert!(previous(&mut state, &ChatContext::default()).is_empty());
        assert!(state.questions.open_questions.jump.is_none());
    }

    #[test]
    fn answering_a_for_later_question_moves_it_to_the_earlier_group() {
        let mut state = ChatState::default();
        let mut answered = asker("a", AskerKind::Page, Some(1_000));
        answered.answered = true;
        state.transcript_view.askers.push(answered);
        state.session.later_question_ids = vec!["a".to_string()];
        let pill = project(&state, &ChatContext::default());
        assert_eq!(section(&pill, "earlier"), vec!["a"]);
        assert_eq!(with_status(&pill, "answered"), vec!["a"]);
        assert!(section(&pill, "unanswered").is_empty());
        assert!(
            later_waiting(&state).is_empty(),
            "its card no longer says For later"
        );
    }

    /// The Unanswered group reads in the order the questions were asked: waiting ones oldest
    /// first, then the For later ones oldest first; the ? pill's own stepping is unchanged.
    #[test]
    fn unanswered_group_lists_oldest_first() {
        let mut state = chat_asking(
            [
                ("q190", 1_000),
                ("q191", 2_000),
                ("q192", 3_000),
                ("q193", 4_000),
                ("q194", 5_000),
            ]
            .map(|(id, at)| asker(id, AskerKind::Form, Some(at)))
            .to_vec(),
        );
        state.session.later_question_ids = vec!["q193".to_string(), "q191".to_string()];
        let pill = project(&state, &ChatContext::default());
        assert_eq!(
            section(&pill, "unanswered"),
            vec!["q190", "q192", "q194", "q191", "q193"]
        );
        assert_eq!(pill.newest.as_deref(), Some("q194"));
        // The click still starts at the newest waiting question and steps back.
        assert_eq!(clicked(&mut state).as_deref(), Some("q194"));
        assert_eq!(clicked(&mut state).as_deref(), Some("q192"));
    }

    /// The X on a For later row: the question leaves Unanswered, the counts and the ? pill's
    /// stepping at once, lands in Earlier marked canceled, and stays there once the wire carries it.
    #[test]
    fn a_removed_question_leaves_the_counts_and_lands_in_earlier_as_canceled() {
        let mut state = chat_asking(vec![
            asker("a", AskerKind::Form, Some(1_000)),
            asker("b", AskerKind::Page, Some(2_000)),
            asker("c", AskerKind::Form, Some(3_000)),
        ]);
        state.session.later_question_ids = vec!["a".to_string(), "b".to_string()];
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.summary, "1 unanswered \u{b7} 2 for later");
        assert_eq!(section(&pill, "unanswered"), vec!["c", "a", "b"]);

        let effects = crate::questions::later::remove(&mut state, "a");
        assert_eq!(effects.len(), 1, "one call to gxserver");
        assert!(
            crate::questions::later::remove(&mut state, "a").is_empty(),
            "already removed"
        );
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.summary, "1 unanswered \u{b7} 1 for later");
        assert_eq!(pill.count, 1);
        assert_eq!(section(&pill, "unanswered"), vec!["c", "b"]);
        assert_eq!(section(&pill, "earlier"), vec!["a"]);
        assert_eq!(with_status(&pill, "canceled"), vec!["a"]);
        assert_eq!(pill.total, 3, "the list still holds it");
        assert!(later_waiting(&state).iter().all(|id| id != "a"));

        // The wire agrees: the pending removal settles and the row stays under Earlier.
        state.session.removed_question_ids = vec!["a".to_string()];
        crate::questions::later::settle(&mut state);
        assert!(state.questions.later.removing.is_empty());
        let pill = project(&state, &ChatContext::default());
        assert_eq!(with_status(&pill, "canceled"), vec!["a"]);

        // Removing the waiting one too leaves no pill count, and the click has nowhere to go.
        crate::questions::later::remove(&mut state, "c");
        crate::questions::later::set(&mut state, "b", true);
        let pill = project(&state, &ChatContext::default());
        assert_eq!((pill.count, pill.newest), (0, None));
        assert_eq!(pill.summary, "0 unanswered \u{b7} 1 for later");
    }

    /// The ? pill never steps to a removed question, even one that was the newest.
    #[test]
    fn the_pill_click_skips_a_removed_question() {
        let mut state = chat_asking(vec![
            asker("old", AskerKind::Form, Some(1_000)),
            asker("new", AskerKind::Form, Some(2_000)),
        ]);
        state.session.removed_question_ids = vec!["new".to_string()];
        assert_eq!(pool_ids(&state), ["old"]);
        assert_eq!(clicked(&mut state).as_deref(), Some("old"));
        assert_eq!(
            project(&state, &ChatContext::default()).newest.as_deref(),
            Some("old")
        );
    }

    /// A removed question that is answered afterwards is simply answered.
    #[test]
    fn an_answer_after_a_removal_still_reads_as_answered() {
        let mut state = chat_asking(vec![answered("a", AskerKind::Form, 1_000)]);
        state.session.removed_question_ids = vec!["a".to_string()];
        let pill = project(&state, &ChatContext::default());
        assert_eq!(with_status(&pill, "answered"), vec!["a"]);
    }

    #[test]
    fn two_groups_unanswered_then_everything_else_newest_first() {
        let mut state = ChatState::default();
        let mut old_answer = asker("old-answer", AskerKind::Form, Some(1_000));
        old_answer.answered = true;
        let mut new_answer = asker("new-answer", AskerKind::Page, Some(5_000));
        new_answer.answered = true;
        let mut withdrawn = asker("withdrawn", AskerKind::Form, Some(2_000));
        withdrawn.obsolete = true;
        let mut inherited = asker("inherited", AskerKind::Form, Some(500));
        inherited.inherited = true;
        let mut undated_answer = asker("undated", AskerKind::Form, None);
        undated_answer.answered = true;
        for asker in [
            inherited,
            old_answer,
            withdrawn,
            asker("open-old", AskerKind::Page, Some(3_000)),
            new_answer,
            asker("open-new", AskerKind::Form, Some(6_000)),
            undated_answer,
        ] {
            state.transcript_view.askers.push(asker);
        }
        let pill = project(&state, &ChatContext::default());
        let order: Vec<&str> = pill
            .sections
            .iter()
            .map(|section| section.id.as_str())
            .collect();
        assert_eq!(order, vec!["unanswered", "earlier"], "two groups only");
        assert_eq!(
            section(&pill, "unanswered"),
            vec!["open-old", "open-new"],
            "Unanswered reads oldest first"
        );
        assert_eq!(
            section(&pill, "earlier"),
            vec!["new-answer", "withdrawn", "old-answer", "undated"],
            "answered and withdrawn in one list, newest first; one without a time goes last"
        );
        assert_eq!(with_status(&pill, "withdrawn"), vec!["withdrawn"]);
        assert_eq!(
            with_status(&pill, "answered"),
            vec!["new-answer", "old-answer", "undated"]
        );
        assert_eq!(
            pill.total, 6,
            "an unanswered question from before a fork is the parent's"
        );
        let earlier = &pill.sections[1];
        assert_eq!((earlier.title.as_str(), earlier.count), ("Earlier", 4));
        let row = &earlier.rows[0];
        assert_eq!(row.kind, "page");
        assert_eq!(row.status, "answered");
        assert_eq!(row.title, "About new-answer");
        assert_eq!(row.tags, vec!["Q-new-answer".to_string()]);
        assert_eq!(row.time, "12:00 AM");
        // Nothing unanswered: the one group has no header.
        let mut settled = ChatState::default();
        let mut done = asker("done", AskerKind::Form, Some(1_000));
        done.answered = true;
        settled.transcript_view.askers.push(done);
        let pill = project(&settled, &ChatContext::default());
        assert_eq!(pill.sections.len(), 1);
        assert_eq!(
            (
                pill.sections[0].id.as_str(),
                pill.sections[0].title.as_str()
            ),
            ("earlier", "")
        );
    }

    #[test]
    fn filters_pick_pages_forms_or_charts_and_count_them() {
        use crate::questions::content::ContentItem;
        use crate::transcript::question_forms::ContentKind;
        let mut state = ChatState::default();
        let mut answered_form = asker("form", AskerKind::Form, Some(2_000));
        answered_form.answered = true;
        state.transcript_view.askers.push(answered_form);
        state
            .transcript_view
            .askers
            .push(asker("question-page", AskerKind::Page, Some(3_000)));
        for (id, kind, at) in [
            ("plain-page", ContentKind::Page, 4_000),
            ("chart-1", ContentKind::Chart, 1_000),
        ] {
            state.questions.content.items.push(ContentItem {
                id: id.to_string(),
                kind,
                message_id: format!("m-{id}"),
                title: String::new(),
                at: Some(at),
            });
        }
        let pill = project(&state, &ChatContext::default());
        let counts: Vec<(&str, u32)> = pill
            .filters
            .iter()
            .map(|f| (f.label.as_str(), f.count))
            .collect();
        assert_eq!(
            counts,
            vec![("All", 4), ("Pages", 2), ("Forms", 1), ("Visuals", 1)]
        );
        assert_eq!(pill.filter, "all");
        assert_eq!(
            pill.count, 1,
            "only the question page waits; pages and charts never count"
        );
        let order: Vec<&str> = pill.sections.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(order, vec!["unanswered", "earlier"]);
        // Forms, pages and charts share the Earlier list, newest first, told apart by kind.
        let earlier: Vec<(&str, &str, &str)> = pill.sections[1]
            .rows
            .iter()
            .map(|row| (row.id.as_str(), row.kind.as_str(), row.status.as_str()))
            .collect();
        assert_eq!(
            earlier,
            vec![
                ("plain-page", "page", "other"),
                ("form", "form", "answered"),
                ("chart-1", "chart", "other"),
            ]
        );
        assert_eq!(
            pill.sections[1].rows[0].title, "Ghostex page",
            "a page without a title still reads as one"
        );

        pick_filter(&mut state, "html");
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.filter, "html");
        let html: Vec<&str> = pill
            .sections
            .iter()
            .flat_map(|s| s.rows.iter().map(|r| r.id.as_str()))
            .collect();
        assert_eq!(
            html,
            vec!["question-page", "plain-page"],
            "pages with and without questions"
        );
        assert_eq!(pill.total, 4, "the pill stays while anything is listed");

        pick_filter(&mut state, "qa");
        let pill = project(&state, &ChatContext::default());
        assert_eq!(section(&pill, "earlier"), vec!["form"]);
        assert_eq!(pill.sections.len(), 1);

        pick_filter(&mut state, "bogus");
        assert_eq!(
            state.questions.open_questions.filter, "",
            "an unknown filter is All"
        );
    }

    #[test]
    fn a_question_link_names_its_tag() {
        assert_eq!(link_tag("#gx-question=Q80"), Some("Q80"));
        assert_eq!(link_tag("#gx-question=Q80;answered"), Some("Q80"));
        assert_eq!(link_tag("https://example.com"), None);
    }

    fn sized(id: &str, count: usize) -> QuestionListSection {
        QuestionListSection {
            id: id.to_string(),
            title: id.to_string(),
            count: count as u32,
            rows: (0..count)
                .map(|index| QuestionListRow {
                    id: format!("r{index}"),
                    ..QuestionListRow::default()
                })
                .collect(),
            remaining: 0,
        }
    }

    #[test]
    fn the_list_shows_twelve_rows_then_a_page_per_click() {
        assert_eq!(LIST_PAGE, 30, "user, 2026-11-10: the newest 30 per group");
        let long = || vec![sized("unanswered", 3), sized("earlier", 100)];
        let (first, left) = page_sections(long(), 0);
        // Unanswered whole, the Earlier list fills the first page, counts stay full.
        let shown: Vec<(usize, u32)> = first.iter().map(|s| (s.rows.len(), s.count)).collect();
        assert_eq!(shown, vec![(3, 3), (30, 100)]);
        assert_eq!(left, 70);
        let (second, left) = page_sections(long(), 1);
        assert_eq!((second[1].rows.len(), left), (60, 40));
        let (third, left) = page_sections(long(), 2);
        assert_eq!((third[1].rows.len(), left), (90, 10));
        // Enough pages shows everything and leaves nothing out.
        let (all, left) = page_sections(long(), 10);
        assert_eq!((all[1].rows.len(), left), (100, 0));
    }

    #[test]
    fn many_unanswered_are_never_cut() {
        let (paged, left) = page_sections(
            vec![sized("unanswered", 3 * LIST_PAGE), sized("earlier", 5)],
            0,
        );
        assert_eq!(paged[0].rows.len(), 3 * LIST_PAGE);
        assert_eq!(
            paged[1].rows.len(),
            5,
            "the Earlier rows still get their page"
        );
        assert_eq!(left, 0);
    }

    #[test]
    fn more_reset_and_the_filter_move_the_page() {
        let mut state = ChatState::default();
        for index in 0..80 {
            let mut done = asker(&format!("a{index}"), AskerKind::Form, Some(1_000 + index));
            done.answered = true;
            state.transcript_view.askers.push(done);
        }
        let list = |state: &ChatState| project(state, &ChatContext::default());
        let pill = list(&state);
        assert_eq!(section(&pill, "earlier").len(), LIST_PAGE);
        assert_eq!(pill.remaining as usize, 80 - LIST_PAGE);
        assert_eq!(pill.total, 80, "the totals stay full");
        assert_eq!(pill.sections[0].count, 80);
        // "Show all" loads the rest at once.
        more(&mut state);
        let pill = list(&state);
        assert_eq!((section(&pill, "earlier").len(), pill.remaining), (80, 0));
        // Reopening starts again from one page; so does a new filter.
        reset_pages(&mut state);
        assert_eq!(section(&list(&state), "earlier").len(), LIST_PAGE);
        more(&mut state);
        pick_filter(&mut state, "qa");
        assert_eq!(state.questions.open_questions.pages, 0);
        assert_eq!(section(&list(&state), "earlier").len(), LIST_PAGE);
    }

    /// An index record as gxserver writes it for a reply its transcript read holds: with the
    /// reply's place (`offset`).
    fn indexed(id: &str, kind: &str, status: &str, at: i64) -> serde_json::Value {
        serde_json::json!({"id": id, "kind": kind, "title": format!("About {id}"), "status": status, "atMs": at,
            "offset": at.max(0) * 10,
            "url": if kind == "page" { Some(format!("http://127.0.0.1:1/visual/{id}")) } else { None }})
    }

    /// The conversation held more than the chat has loaded: the pill's count, the list, the totals
    /// and the filters cover the index, and a loaded row still wins for its own id.
    #[test]
    fn the_index_adds_what_the_loaded_rows_do_not_hold() {
        let mut state = ChatState::default();
        let mut loaded = asker("loaded", AskerKind::Form, Some(9_000));
        loaded.answered = true;
        state.transcript_view.askers.push(loaded);
        state.session.resource_index = vec![
            indexed("old-open", "form", "waiting", 1_000),
            indexed("old-done", "form", "answered", 2_000),
            indexed("old-page", "page", "other", 3_000),
            // The loaded row for this id says answered; the index record is stale.
            indexed("loaded", "form", "waiting", 9_000),
        ]
        .into();
        let pill = project(&state, &ChatContext::default());
        assert_eq!(
            pill.total, 4,
            "the loaded row once, plus the three older ones"
        );
        assert_eq!(pill.count, 1, "the older waiting question counts");
        assert_eq!(pill.summary, "1 unanswered");
        assert_eq!(section(&pill, "unanswered"), vec!["old-open"]);
        assert_eq!(
            section(&pill, "earlier"),
            vec!["loaded", "old-page", "old-done"],
            "newest first across loaded and indexed rows"
        );
        assert_eq!(with_status(&pill, "answered"), vec!["loaded", "old-done"]);
        let html = pill.filters.iter().find(|f| f.id == "html").unwrap();
        assert_eq!(html.count, 1);
        // The reader's marks apply to indexed rows too.
        state.session.later_question_ids = vec!["old-open".to_string()];
        let pill = project(&state, &ChatContext::default());
        assert_eq!(
            (pill.count, pill.summary.as_str()),
            (0, "0 unanswered · 1 for later")
        );
        assert_eq!(with_status(&pill, "later"), vec!["old-open"]);
        state.session.removed_question_ids = vec!["old-open".to_string()];
        assert_eq!(
            with_status(&project(&state, &ChatContext::default()), "canceled"),
            vec!["old-open"]
        );
    }

    /// The count survives a compaction and a partly loaded chat: nothing loaded, all in the index.
    #[test]
    fn a_chat_with_nothing_loaded_still_counts_the_whole_conversation() {
        let mut state = ChatState::default();
        state.session.resource_index = (0..300)
            .map(|n| {
                indexed(
                    &format!("r{n}"),
                    if n % 3 == 0 { "page" } else { "form" },
                    "answered",
                    n,
                )
            })
            .collect();
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.total, 300);
        assert_eq!(pill.filters[0].count, 300);
        assert_eq!(
            section(&pill, "earlier").len(),
            LIST_PAGE,
            "still one page of rows"
        );
        assert_eq!(pill.remaining as usize, 300 - LIST_PAGE);
    }

    /// A coordinator lists the pages its threads posted in their own group, counted in the total.
    #[test]
    fn thread_pages_get_their_own_group_with_the_threads_name() {
        let mut state = ChatState::default();
        let mut done = asker("mine", AskerKind::Form, Some(5_000));
        done.answered = true;
        state.transcript_view.askers.push(done);
        let mut page = indexed("tp", "page", "other", 7_000);
        page["thread"] =
            serde_json::json!({"title": "Fix the sidebar", "projectId": "p", "sessionId": "s"});
        state.session.resource_index = vec![page].into();
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.total, 2);
        let ids: Vec<&str> = pill.sections.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["earlier", "threads"]);
        assert_eq!(pill.sections[1].title, "Worker pages");
        assert_eq!(pill.sections[1].rows[0].thread, "Fix the sidebar");
        assert_eq!(pill.count, 0, "a thread's page never counts as waiting");
    }

    /// Opening an indexed row: a page opens (a thread's read-only), a form older than the loaded
    /// rows is read in by its place, and one with no recorded place says so.
    #[test]
    fn opening_an_indexed_row_opens_the_page_or_says_why_not() {
        let mut state = ChatState::default();
        let mut thread_page = indexed("tp", "page", "other", 1);
        thread_page["thread"] =
            serde_json::json!({"title": "T", "projectId": "p", "sessionId": "s"});
        state.session.resource_index = vec![
            indexed("own-page", "page", "other", 2),
            thread_page,
            indexed("old-form", "form", "waiting", 3),
            no_place("placeless", "waiting", 4),
        ]
        .into();
        let context = ChatContext::default();
        let open = |state: &mut ChatState, id: &str| jump_to_id(state, &context, id);
        match &open(&mut state, "own-page")[..] {
            [Effect::Open(crate::effect::OpenTarget::Url { url, .. })] => {
                assert_eq!(url, "http://127.0.0.1:1/visual/own-page");
            }
            other => panic!("{other:?}"),
        }
        match &open(&mut state, "tp")[..] {
            [Effect::Open(crate::effect::OpenTarget::Url { url, .. })] => {
                assert_eq!(
                    url,
                    &format!("http://127.0.0.1:1/visual/tp?{READ_ONLY_QUERY}")
                );
            }
            other => panic!("{other:?}"),
        }
        // One the index cannot place opens in the popup, which says why it cannot be read.
        open(&mut state, "placeless");
        assert!(state
            .questions
            .resource_modal
            .as_ref()
            .is_some_and(|modal| modal.id == "placeless"));
        crate::questions::resource_modal::close(&mut state);
        // The ? pill with only an older question waiting opens it in the popup.
        assert_eq!(
            project(&state, &context).count,
            1,
            "the placeless one is not counted"
        );
        assert!(!previous(&mut state, &context).is_empty());
        let modal = state.questions.resource_modal.as_ref().unwrap();
        assert_eq!((modal.id.as_str(), modal.loading), ("old-form", true));
        assert!(state.questions.resource_go.is_none(), "nothing scrolls");
    }

    /// A record of a reply the transcript gxserver reads no longer holds: it has no place.
    fn no_place(id: &str, status: &str, at: i64) -> serde_json::Value {
        let mut entry = indexed(id, "form", status, at);
        entry.as_object_mut().unwrap().remove("offset");
        entry
    }

    /// A waiting record nothing can reach any more reads withdrawn: the ? pill neither counts it
    /// nor steps to it, and the list shows it under Earlier.
    #[test]
    fn a_waiting_record_with_no_place_is_not_counted() {
        let mut state = ChatState::default();
        state.session.resource_index = vec![no_place("gone", "waiting", 5)].into();
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.count, 0);
        assert_eq!(with_status(&pill, "withdrawn"), vec!["gone"]);
        assert!(previous(&mut state, &ChatContext::default()).is_empty());
        assert!(state.questions.resource_go.is_none());
    }

    #[test]
    fn thread_pages_are_on_the_first_page_beside_a_long_earlier_list() {
        let (paged, left) = page_sections(
            vec![
                sized("unanswered", 2),
                sized("earlier", 200),
                sized("threads", 30),
            ],
            0,
        );
        let shown: Vec<usize> = paged.iter().map(|s| s.rows.len()).collect();
        assert_eq!(
            shown,
            [2, LIST_PAGE, LIST_PAGE],
            "each paged group has its own page"
        );
        assert_eq!(left as usize, (200 - LIST_PAGE) + (30 - LIST_PAGE));
    }

    /// Each group's Show more belongs to its own cut rows: a long Earlier next to a short Worker
    /// pages group says 401 on Earlier and nothing on the other (the screenshot where Earlier was
    /// folded showed Earlier's 401 under Worker pages).
    #[test]
    fn each_group_counts_its_own_cut_rows() {
        let (paged, left) = page_sections(
            vec![
                sized("unanswered", 2),
                sized("earlier", 413),
                sized("threads", 4),
            ],
            0,
        );
        let remaining: Vec<(&str, u32)> =
            paged.iter().map(|s| (s.id.as_str(), s.remaining)).collect();
        assert_eq!(
            remaining,
            [
                ("unanswered", 0),
                ("earlier", 413 - LIST_PAGE as u32),
                ("threads", 0)
            ]
        );
        assert_eq!(
            left,
            413 - LIST_PAGE as u32,
            "the list-wide total is their sum"
        );
        let (later, _) = page_sections(vec![sized("earlier", 100)], 1);
        assert_eq!(later[0].remaining, 100 - 2 * LIST_PAGE as u32);
    }
}

#[cfg(test)]
mod paged_labels_tests {
    use super::*;
    use crate::transcript::open_questions::{Asker, AskerKind};

    /// What `project` showed before it listed rows without their time fields: every row listed with
    /// them, then filtered and paged.
    fn sections_listed_whole(
        state: &ChatState,
        context: &ChatContext,
    ) -> (Vec<QuestionListSection>, u32) {
        let filter = state.questions.open_questions.filter.as_str();
        let mut sections = Vec::new();
        for mut section in list(state, context) {
            section.rows.retain(|row| filter_shows(filter, &row.kind));
            if !section.rows.is_empty() {
                section.count = section.rows.len() as u32;
                sections.push(section);
            }
        }
        let (mut sections, remaining) =
            page_sections(sections, state.questions.open_questions.pages);
        if sections.len() == 1 {
            sections[0].title.clear();
        }
        (sections, remaining)
    }

    #[test]
    fn an_index_that_changes_in_place_is_read_again() {
        let entry = |status: &str| serde_json::json!({"id": "r1", "kind": "form", "title": "One", "atMs": 5, "status": status, "offset": 1});
        let mut state = ChatState::default();
        state.session.resource_index = vec![entry("answered")].into();
        assert_eq!(project(&state, &ChatContext::default()).count, 0);
        state.session.resource_index = vec![entry("waiting")].into();
        assert_eq!(project(&state, &ChatContext::default()).count, 1);
        state.session.resource_index = vec![entry("answered")].into();
        assert_eq!(project(&state, &ChatContext::default()).count, 0);
        state.session.resource_index.clear();
        assert_eq!(project(&state, &ChatContext::default()).total, 0);
    }

    #[test]
    fn the_paged_list_reads_exactly_as_listing_every_row_with_its_time_would() {
        let now = 1_762_300_000_000_i64;
        let kinds = ["form", "page", "chart"];
        let mut state = ChatState::default();
        state.session.resource_index = (0..120)
            .map(|n| {
                serde_json::json!({
                    "id": format!("r{n}"),
                    "kind": kinds[n as usize % 3],
                    "title": format!("Resource **{n}**"),
                    "atMs": now - n * 3_600_000,
                    "status": if n % 7 == 0 { "waiting" } else { "answered" },
                    "offset": n * 10,
                })
            })
            .collect();
        for n in 0..6 {
            state.transcript_view.askers.push(Asker {
                id: format!("a{n}"),
                kind: if n % 2 == 0 {
                    AskerKind::Form
                } else {
                    AskerKind::Page
                },
                message_id: format!("m-a{n}"),
                tags: vec![format!("Q-a{n}")],
                answered: n % 3 == 0,
                obsolete: false,
                withdrawn: Vec::new(),
                keys: Vec::new(),
                skipped: Vec::new(),
                superseded: Vec::new(),
                inherited: false,
                said: Vec::new(),
                title: format!("Asked {n}"),
                at: Some(now - n * 60_000),
            });
        }
        state.session.later_question_ids = vec!["a1".to_string(), "r14".to_string()];
        let context = ChatContext {
            now_ms: now as f64,
            utc_offset_minutes: 240,
            ..Default::default()
        };
        for (filter, pages) in [("", 0), ("", 2), ("page", 0), ("form", 1), ("chart", 0)] {
            state.questions.open_questions.filter = filter.to_string();
            state.questions.open_questions.pages = pages;
            let projected = project(&state, &context);
            let (sections, remaining) = sections_listed_whole(&state, &context);
            assert_eq!(
                projected.sections, sections,
                "filter {filter:?}, pages {pages}"
            );
            assert_eq!(projected.remaining, remaining);
            assert!(projected
                .sections
                .iter()
                .flat_map(|section| &section.rows)
                .all(|row| !row.time.is_empty() && !row.time_title.is_empty()));
        }
    }
}

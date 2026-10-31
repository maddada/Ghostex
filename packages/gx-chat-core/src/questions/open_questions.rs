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

use serde::{Deserialize, Serialize};

use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::transcript::open_questions::{tag_target, Asker, AskerKind};
use crate::questions::resource_index::ResourceEntry;
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
    /// The thread that posted this page, on a row of a coordinator's Thread pages group.
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
    ("html", "Html"),
    ("qa", "Q&A"),
    ("charts", "Charts"),
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

/// How many rows of the long group one page of the list shows (user, 2026-10-29: "show 12 here when
/// first opened please"; the desktop panel is 380 px of 26 px rows, the phone's 520 pt of 40 pt).
pub const LIST_PAGE: usize = 12;

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
    // Each paged group has its own budget, so a coordinator's Thread pages are on the first page
    // beside its Earlier list and not behind all of it.
    let budget = (pages as usize + 1) * LIST_PAGE;
    let mut left_out = 0usize;
    let paged = sections
        .into_iter()
        .map(|mut section| {
            if !always_whole(&section.id) {
                let keep = section.rows.len().min(budget);
                left_out += section.rows.len() - keep;
                section.rows.truncate(keep);
            }
            section
        })
        .collect();
    (paged, left_out as u32)
}

/// `questionListMore`: the list shows one more page.
pub fn more(state: &mut ChatState) {
    state.questions.open_questions.pages += 1;
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
    state
        .transcript_view
        .askers
        .iter()
        .filter(|asker| !asker.answered && !asker.obsolete && !asker.inherited)
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
    } else if asker.obsolete {
        Some("withdrawn")
    } else {
        Some("answered")
    }
}

/// The session's resource index (gxserver's, `resource_index.rs`) as records.
pub(crate) fn indexed(state: &ChatState) -> Vec<ResourceEntry> {
    state
        .session
        .resource_index
        .iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

/// The index records the loaded rows do not hold: older than what the chat has loaded, or from a
/// thread. A loaded row is always the fresher reading of its id.
fn index_only(state: &ChatState) -> Vec<ResourceEntry> {
    let mut known: std::collections::HashSet<&str> = state
        .transcript_view
        .askers
        .iter()
        .map(|asker| asker.id.as_str())
        .collect();
    known.extend(state.questions.content.items.iter().map(|item| item.id.as_str()));
    indexed(state)
        .into_iter()
        .filter(|entry| !known.contains(entry.id.as_str()))
        .collect()
}

/// The status a record reads as here: the reader's For later and Remove marks are this session's
/// (`laterQuestionIds`, `removedQuestionIds`) and apply to the records too.
fn indexed_status(
    entry: &ResourceEntry,
    later: &std::collections::BTreeSet<String>,
    removed: &std::collections::BTreeSet<String>,
) -> String {
    match entry.status.as_str() {
        "waiting" if removed.contains(&entry.id) => "canceled",
        "waiting" if later.contains(&entry.id) => "later",
        "withdrawn" if removed.contains(&entry.id) => "canceled",
        "" => "other",
        other => other,
    }
    .to_string()
}

/// How many forms and question pages the index says wait on the reader that the loaded rows do not
/// hold, not counting the ones put off For later or removed.
fn indexed_waiting(state: &ChatState) -> u32 {
    let later = crate::questions::later::later_ids(state);
    let removed = crate::questions::later::removed_ids(state);
    let on_the_way: Vec<String> = answers_on_the_way(state)
        .into_iter()
        .map(|answers| answers.page_id)
        .collect();
    index_only(state)
        .iter()
        .filter(|entry| entry.thread.is_none())
        .filter(|entry| indexed_status(entry, &later, &removed) == "waiting")
        .filter(|entry| !on_the_way.contains(&entry.id))
        .count() as u32
}

fn indexed_row(
    entry: &ResourceEntry,
    status: String,
    context: &ChatContext,
) -> QuestionListRow {
    let time = crate::transcript::message_time::message_time(entry.at_ms, context);
    let fallback = match entry.kind.as_str() {
        "form" => "Questions",
        "chart" => "Chart",
        _ => "Page",
    };
    QuestionListRow {
        id: entry.id.clone(),
        kind: entry.kind.clone(),
        title: if entry.title.is_empty() {
            fallback.to_string()
        } else {
            entry.title.clone()
        },
        status,
        tags: entry.tags.clone(),
        time: time["label"].as_str().unwrap_or_default().to_string(),
        age: age_of(entry.at_ms, context),
        at_ms: entry.at_ms,
        time_title: time["title"].as_str().unwrap_or_default().to_string(),
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
    ("threads", "Thread pages"),
];

/// The pill's list: every form, page and chart in its group, all kinds. Earlier is newest first;
/// Unanswered is oldest first, so the waiting questions read in the order they were asked.
///
/// CDXC:SessionChat 2026-10-30 DECISION:
/// User, of the Chat Resources panel's Unanswered group (Q194, Q192, Q191, Q190 from the top): it lists oldest first (Q190, Q191, Q192, Q194), "so the questions read in the order they were asked". Only Unanswered flips (waiting ones, then the For later ones, each oldest first); Earlier stays newest first. The ? pill's click order (`previous`) and its `newest` ping id do not read this list.
pub fn list(state: &ChatState, context: &ChatContext) -> Vec<QuestionListSection> {
    let waiting = waiting(state);
    let later = crate::questions::later::later_ids(state);
    let removed = crate::questions::later::removed_ids(state);
    let on_the_way: Vec<String> = answers_on_the_way(state)
        .into_iter()
        .map(|answers| answers.page_id)
        .collect();
    let mut placed: Vec<(Option<i64>, usize, QuestionListRow)> = state
        .transcript_view
        .askers
        .iter()
        .enumerate()
        .filter_map(|(index, asker)| {
            let status = status_of(asker, &waiting, &later, &removed, &on_the_way)?;
            let mut row = list_row(asker, context);
            row.status = status.to_string();
            Some((asker.at, index, row))
        })
        .collect();
    let askers = state.transcript_view.askers.len();
    for (index, item) in state.questions.content.items.iter().enumerate() {
        let mut row = content_row(item, context);
        row.status = "other".to_string();
        placed.push((item.at, askers + index, row));
    }
    // What the whole conversation held beyond the loaded rows (the session's resource index), and
    // the pages the coordinator's threads posted.
    let mut thread_pages: Vec<QuestionListRow> = Vec::new();
    for entry in index_only(state) {
        let mut status = indexed_status(&entry, &later, &removed);
        // Answers sent from the modal wait in the queue: the form reads answered at once.
        if status == "waiting" && on_the_way.contains(&entry.id) {
            status = "answered".to_string();
        }
        let row = indexed_row(&entry, status, context);
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
    let rows: Vec<QuestionListRow> = placed.into_iter().map(|(_, _, row)| row).collect();
    // Waiting first, then the put-off ones, each oldest first (`rows` is newest first).
    let unanswered: Vec<QuestionListRow> = ["waiting", "later"]
        .iter()
        .flat_map(|status| rows.iter().rev().filter(move |row| row.status == *status))
        .cloned()
        .collect();
    let earlier: Vec<QuestionListRow> = rows
        .iter()
        .filter(|row| row.status != "waiting" && row.status != "later")
        .cloned()
        .collect();
    GROUPS
        .iter()
        .zip([unanswered, earlier, thread_pages])
        .filter_map(|((id, title), rows)| {
            (!rows.is_empty()).then(|| QuestionListSection {
                id: id.to_string(),
                title: title.to_string(),
                count: rows.len() as u32,
                rows,
            })
        })
        .collect()
}

fn age_of(at: Option<i64>, context: &ChatContext) -> String {
    at.map_or_else(String::new, |at| {
        crate::transcript::message_time::age_label(context.now_millis(), at)
    })
}

fn list_row(asker: &Asker, context: &ChatContext) -> QuestionListRow {
    let time = crate::transcript::message_time::message_time(asker.at, context);
    let title = if asker.title.is_empty() {
        match asker.kind {
            AskerKind::Form => "Questions".to_string(),
            AskerKind::Page => "Page".to_string(),
        }
    } else {
        asker.title.clone()
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
        time: time["label"].as_str().unwrap_or_default().to_string(),
        age: age_of(asker.at, context),
        at_ms: asker.at,
        time_title: time["title"].as_str().unwrap_or_default().to_string(),
        thread: String::new(),
    }
}

fn content_row(
    item: &crate::questions::content::ContentItem,
    context: &ChatContext,
) -> QuestionListRow {
    use crate::transcript::question_forms::ContentKind;
    let time = crate::transcript::message_time::message_time(item.at, context);
    let (kind, fallback) = match item.kind {
        ContentKind::Chart => ("chart", "Chart"),
        _ => ("page", "Page"),
    };
    QuestionListRow {
        id: item.id.clone(),
        kind: kind.to_string(),
        title: if item.title.is_empty() {
            fallback.to_string()
        } else {
            item.title.clone()
        },
        status: String::new(),
        tags: Vec::new(),
        time: time["label"].as_str().unwrap_or_default().to_string(),
        age: age_of(item.at, context),
        at_ms: item.at,
        time_title: time["title"].as_str().unwrap_or_default().to_string(),
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

/// The document's `openQuestions`.
pub fn project(state: &ChatState, context: &ChatContext) -> OpenQuestions {
    let open = unanswered(state);
    // The loaded rows' unanswered questions, plus the ones older than what the chat has loaded.
    let count = open.len() as u32 + indexed_waiting(state);
    let filter = state.questions.open_questions.filter.as_str();
    let all = list(state, context);
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
    let (sections, remaining) = page_sections(
        all.iter()
            .filter_map(|section| {
                let rows: Vec<QuestionListRow> = section
                    .rows
                    .iter()
                    .filter(|row| filter_shows(filter, &row.kind))
                    .cloned()
                    .collect();
                (!rows.is_empty()).then(|| QuestionListSection {
                    count: rows.len() as u32,
                    rows,
                    ..section.clone()
                })
            })
            .collect(),
        state.questions.open_questions.pages,
    );
    // "Earlier" has a header only beside another group.
    let mut sections = sections;
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
        newest: open.last().map(|asker| asker.id.clone()),
        jump: state.questions.open_questions.jump.clone(),
        total,
        summary,
        filters: FILTERS
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
            .collect(),
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
    if open.is_empty() {
        // The only questions waiting are older than the loaded rows (the pill counts them):
        // there is no row to scroll to, so say where they are.
        // The only questions waiting are older than the loaded rows: "Go to chat" brings the
        // newest of them in and scrolls to it, as the Resources modal's button does.
        let newest = indexed(state)
            .into_iter()
            .filter(|entry| entry.thread.is_none() && entry.status == "waiting")
            .filter(|entry| {
                !crate::questions::later::later_ids(state).contains(&entry.id)
                    && !crate::questions::later::removed_ids(state).contains(&entry.id)
            })
            .max_by_key(|entry| entry.at_ms.unwrap_or(i64::MIN));
        return match newest {
            Some(entry) => crate::questions::resource_modal::go_to(state, context, &entry.id),
            None => Vec::new(),
        };
    }
    let last = state.questions.open_questions.last.as_deref();
    let at = open
        .iter()
        .position(|asker| Some(asker.id.as_str()) == last);
    jump_to(state, context, &open[step(open.len(), at)], None)
}

/// Which of `count` unanswered questions (oldest first) the pill goes to, given where the last
/// jump went (`at`).
fn step(count: usize, at: Option<usize>) -> usize {
    match at {
        Some(index) => index.checked_sub(1).unwrap_or(count - 1),
        None => count - 1,
    }
}

/// `jumpToQuestion {id}`: a row of the pill's list (a form or page, answered or not, or a chart).
pub fn jump_to_id(state: &mut ChatState, context: &ChatContext, id: &str) -> Vec<Effect> {
    if let Some(asker) = state
        .transcript_view
        .askers
        .iter()
        .find(|asker| asker.id == id)
        .cloned()
    {
        return jump_to(state, context, &asker, None);
    }
    let Some(item) = state
        .questions
        .content
        .items
        .iter()
        .rev()
        .find(|item| item.id == id)
        .cloned()
    else {
        return open_indexed(state, context, id);
    };
    jump_to_row(state, context, &item.message_id, &item.id, None)
}

/// `openResource {id}`: a row of the list clicked. A form or chart opens in the floating modal and
/// the chat stays where it is (`resource_modal.rs`); a page opens as it always did (an older one,
/// or a thread's, by its address; a loaded one by scrolling to its card).
pub fn open_resource(state: &mut ChatState, context: &ChatContext, id: &str) -> Vec<Effect> {
    let effects = crate::questions::resource_modal::open(state, id);
    if state.questions.resource_modal.as_ref().is_some_and(|modal| modal.id == id) {
        return effects;
    }
    jump_to_id(state, context, id)
}

/// A jump to an item only the resource index holds: a page opens where the reader's Open chat
/// pages in setting says (a thread's page read-only, so nothing the reader types in it is sent);
/// a form or chart older than the loaded rows is brought in and scrolled to (`resource_modal::go_to`).
fn open_indexed(state: &mut ChatState, context: &ChatContext, id: &str) -> Vec<Effect> {
    let Some(entry) = indexed(state).into_iter().find(|entry| entry.id == id) else {
        return Vec::new();
    };
    match (&entry.url, entry.kind.as_str()) {
        (Some(url), "page") => {
            let url = if entry.thread.is_some() {
                read_only_url(url)
            } else {
                url.clone()
            };
            vec![Effect::Open(crate::effect::OpenTarget::Url {
                url,
                external: false,
            })]
        }
        _ => crate::questions::resource_modal::go_to(state, context, id),
    }
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
pub fn jump_to_tag(state: &mut ChatState, context: &ChatContext, tag: &str) -> Vec<Effect> {
    let Some(asker) = tag_target(&state.transcript_view.askers, tag).cloned() else {
        return vec![Effect::Toast {
            level: "info".to_string(),
            message: format!("{tag} isn't asked in the loaded part of this chat."),
        }];
    };
    // The form's own spelling of the tag, which its drawn question carries.
    let question = asker
        .tags
        .iter()
        .find(|known| known.eq_ignore_ascii_case(tag))
        .cloned();
    jump_to(state, context, &asker, question)
}

fn jump_to(
    state: &mut ChatState,
    context: &ChatContext,
    asker: &Asker,
    question: Option<String>,
) -> Vec<Effect> {
    jump_to_row(state, context, &asker.message_id, &asker.id, question)
}

/// Scrolls to the row holding `message_id` and flashes `target` there.
pub(crate) fn jump_to_row(
    state: &mut ChatState,
    context: &ChatContext,
    message_id: &str,
    target: &str,
    question: Option<String>,
) -> Vec<Effect> {
    let rows = crate::transcript::rows(state, context);
    let Some(item) = crate::composer::files_links::row_of(&rows, message_id, None) else {
        return Vec::new();
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
    Vec::new()
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
        let jump = state.questions.open_questions.jump.as_ref().unwrap();
        assert_eq!((jump.target.as_str(), jump.revision), ("open", 2));
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
            vec![("All", 4), ("Html", 2), ("Q&A", 1), ("Charts", 1)]
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
            pill.sections[1].rows[0].title, "Page",
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
        }
    }

    #[test]
    fn the_list_shows_twelve_rows_then_a_page_per_click() {
        assert_eq!(LIST_PAGE, 12, "user, 2026-10-29: show 12 when first opened");
        let long = || vec![sized("unanswered", 3), sized("earlier", 40)];
        let (first, left) = page_sections(long(), 0);
        // Unanswered whole, the Earlier list fills the first page, counts stay full.
        let shown: Vec<(usize, u32)> = first.iter().map(|s| (s.rows.len(), s.count)).collect();
        assert_eq!(shown, vec![(3, 3), (12, 40)]);
        assert_eq!(left, 28);
        // Each click adds one more page of 12.
        let (second, left) = page_sections(long(), 1);
        assert_eq!((second[1].rows.len(), left), (24, 16));
        let (third, left) = page_sections(long(), 2);
        assert_eq!((third[1].rows.len(), left), (36, 4));
        // Enough pages shows everything and leaves nothing out.
        let (all, left) = page_sections(long(), 10);
        assert_eq!((all[1].rows.len(), left), (40, 0));
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
        for index in 0..40 {
            let mut done = asker(&format!("a{index}"), AskerKind::Form, Some(1_000 + index));
            done.answered = true;
            state.transcript_view.askers.push(done);
        }
        let list = |state: &ChatState| project(state, &ChatContext::default());
        let pill = list(&state);
        assert_eq!(section(&pill, "earlier").len(), LIST_PAGE);
        assert_eq!(pill.remaining as usize, 40 - LIST_PAGE);
        assert_eq!(pill.total, 40, "the totals stay full");
        assert_eq!(pill.sections[0].count, 40);
        more(&mut state);
        assert_eq!(section(&list(&state), "earlier").len(), 2 * LIST_PAGE);
        more(&mut state);
        more(&mut state);
        let pill = list(&state);
        assert_eq!((section(&pill, "earlier").len(), pill.remaining), (40, 0));
        // Reopening starts again from one page; so does a new filter.
        reset_pages(&mut state);
        assert_eq!(section(&list(&state), "earlier").len(), LIST_PAGE);
        more(&mut state);
        pick_filter(&mut state, "qa");
        assert_eq!(state.questions.open_questions.pages, 0);
        assert_eq!(section(&list(&state), "earlier").len(), LIST_PAGE);
    }

    fn indexed(id: &str, kind: &str, status: &str, at: i64) -> serde_json::Value {
        serde_json::json!({"id": id, "kind": kind, "title": format!("About {id}"), "status": status, "atMs": at,
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
        ];
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.total, 4, "the loaded row once, plus the three older ones");
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
        assert_eq!((pill.count, pill.summary.as_str()), (0, "0 unanswered · 1 for later"));
        assert_eq!(with_status(&pill, "later"), vec!["old-open"]);
        state.session.removed_question_ids = vec!["old-open".to_string()];
        assert_eq!(with_status(&project(&state, &ChatContext::default()), "canceled"), vec!["old-open"]);
    }

    /// The count survives a compaction and a partly loaded chat: nothing loaded, all in the index.
    #[test]
    fn a_chat_with_nothing_loaded_still_counts_the_whole_conversation() {
        let mut state = ChatState::default();
        state.session.resource_index = (0..300)
            .map(|n| indexed(&format!("r{n}"), if n % 3 == 0 { "page" } else { "form" }, "answered", n))
            .collect();
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.total, 300);
        assert_eq!(pill.filters[0].count, 300);
        assert_eq!(section(&pill, "earlier").len(), LIST_PAGE, "still one page of rows");
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
        page["thread"] = serde_json::json!({"title": "Fix the sidebar", "projectId": "p", "sessionId": "s"});
        state.session.resource_index = vec![page];
        let pill = project(&state, &ChatContext::default());
        assert_eq!(pill.total, 2);
        let ids: Vec<&str> = pill.sections.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["earlier", "threads"]);
        assert_eq!(pill.sections[1].title, "Thread pages");
        assert_eq!(pill.sections[1].rows[0].thread, "Fix the sidebar");
        assert_eq!(pill.count, 0, "a thread's page never counts as waiting");
    }

    /// Opening an indexed row: a page opens (a thread's read-only), anything older than the loaded
    /// rows says so.
    #[test]
    fn opening_an_indexed_row_opens_the_page_or_says_why_not() {
        let mut state = ChatState::default();
        let mut thread_page = indexed("tp", "page", "other", 1);
        thread_page["thread"] = serde_json::json!({"title": "T", "projectId": "p", "sessionId": "s"});
        state.session.resource_index = vec![
            indexed("own-page", "page", "other", 2),
            thread_page,
            indexed("old-form", "form", "waiting", 3),
        ];
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
                assert_eq!(url, &format!("http://127.0.0.1:1/visual/tp?{READ_ONLY_QUERY}"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(&open(&mut state, "old-form")[..], [Effect::Toast { .. }]));
        // The ? pill with only an older question waiting says where it is.
        assert!(matches!(&previous(&mut state, &context)[..], [Effect::Toast { .. }]));
    }

    #[test]
    fn thread_pages_are_on_the_first_page_beside_a_long_earlier_list() {
        let (paged, left) = page_sections(
            vec![sized("unanswered", 2), sized("earlier", 200), sized("threads", 30)],
            0,
        );
        let shown: Vec<usize> = paged.iter().map(|s| s.rows.len()).collect();
        assert_eq!(shown, [2, LIST_PAGE, LIST_PAGE], "each paged group has its own page");
        assert_eq!(left as usize, (200 - LIST_PAGE) + (30 - LIST_PAGE));
    }
}

//! Pure-text guards on questions a coordinator asks the user (`item ask` and `ghostex visual check`
//! for ```questions forms).
//!
//! CDXC:Coordinators 2026-11-24 DECISION:
//! User (coord-asks-page, Q4823 "playbook and guard" + Q4824 five rules + detail note): coordinators
//! were sending questions nobody could understand on their own: pointer asks pointing at a Live page
//! or form already asking the user, internal numbers in titles (`W12`, `Q4816`, thread refs, commit
//! hashes), and missing or one-line descriptions too thin to decide from. `item ask` and
//! `ghostex visual check` on ```questions forms now refuse those with a message saying how to fix
//! them. Keeps the 2026-11-22 bare `W<n>` guard and widens it to every internal reference class.
//! SEE-ALSO: server/src/coordinators/work.rs (`handle_work_request` "ask" arm),
//! server/src/ghostex_cli/visual.rs (`report_question_form`),
//! server/src/coordinators/playbook.md ("Asking the user").

use super::user_handoff::unquoted;
use super::work::WorkItem;

/// Minimum length (in trimmed, whitespace-collapsed characters) for a question's description so the
/// user has enough detail to decide without opening the chat or hunting for context.
pub const MIN_DETAIL_CHARS: usize = 120;

/// Exact normalized-word phrases (outside quotes/backticks) that point the user at a Live page or
/// Ghostex form instead of asking directly.
const DIRECT_POINTER_PHRASES: &[&str] = &[
    "on the live page",
    "in the live page",
    "see the live page",
    "open the live page",
    "check the live page",
    "answer the live page",
    "review the live page",
    "use the live page",
    "on the page",
    "in the page",
    "see the page",
    "answer the page",
    "on the ghostex form",
    "in the ghostex form",
    "see the ghostex form",
    "open the ghostex form",
    "answer the ghostex form",
    "fill out the ghostex form",
    "fill in the ghostex form",
    "on the form",
    "in the form",
    "see the form",
    "open the form",
    "answer the form",
    "check the form",
    "fill out the form",
    "fill in the form",
    "on the question form",
    "in the question form",
    "on the questions form",
    "in the questions form",
    "see the question form",
    "see the questions form",
    "the live page above",
    "the page above",
    "the form above",
    "the live page in chat",
    "the page in chat",
    "the form in chat",
];

/// Verbs that direct the user to answer somewhere else, followed within a few words by "page" or
/// "form" (e.g. "pick on the welcome screen Live page", "answer on the triage page").
const POINTER_VERB_LEADS: &[&[&str]] = &[
    &["pick", "on", "the"],
    &["pick", "in", "the"],
    &["pick", "from", "the"],
    &["answer", "on", "the"],
    &["answer", "in", "the"],
    &["answer", "the"],
    &["choose", "on", "the"],
    &["choose", "in", "the"],
    &["vote", "on", "the"],
    &["respond", "on", "the"],
    &["respond", "in", "the"],
    &["select", "on", "the"],
];

/// Preposition/verb leads that point at an explicit Live page or Ghostex/questions form with a few
/// words in between (e.g. "on the New session Live page", "see the settings Live page").
const EXPLICIT_SURFACE_LEADS: &[&[&str]] = &[
    &["on", "the"],
    &["in", "the"],
    &["see", "the"],
    &["open", "the"],
    &["check", "the"],
    &["review", "the"],
    &["use", "the"],
];

const ANY_SURFACE_TARGETS: &[&[&str]] = &[
    &["live", "page"],
    &["ghostex", "form"],
    &["questions", "form"],
    &["question", "form"],
    &["page"],
    &["form"],
];

const EXPLICIT_SURFACE_TARGETS: &[&[&str]] = &[
    &["live", "page"],
    &["ghostex", "form"],
    &["questions", "form"],
    &["question", "form"],
];

/// Maximum words between a pointer lead and its surface target ("pick on the [welcome screen] live
/// page").
const MAX_POINTER_GAP_WORDS: usize = 6;

/// Lowercase words from `text` outside quotes and backticks, punctuation treated as spaces.
fn normalized_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in unquoted(text).chars() {
        let ch = if ch == '\u{2019}' { '\'' } else { ch };
        if ch.is_alphanumeric() || ch == '\'' {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn slice_matches(words: &[String], at: usize, pattern: &[&str]) -> bool {
    words.len() >= at + pattern.len()
        && pattern
            .iter()
            .enumerate()
            .all(|(offset, expected)| words[at + offset] == *expected)
}

fn find_lead_target(words: &[String], leads: &[&[&str]], targets: &[&[&str]]) -> Option<String> {
    for start in 0..words.len() {
        for lead in leads {
            if !slice_matches(words, start, lead) {
                continue;
            }
            let after_lead = start + lead.len();
            for gap in 0..=MAX_POINTER_GAP_WORDS {
                let target_start = after_lead + gap;
                if target_start >= words.len() {
                    break;
                }
                for target in targets {
                    if slice_matches(words, target_start, target) {
                        let end = target_start + target.len();
                        return Some(words[start..end].join(" "));
                    }
                }
            }
        }
    }
    None
}

/// The phrase in `text` that points the user at a Live page or form, if any.
fn pointer_phrase(text: &str) -> Option<String> {
    let words = normalized_words(text);
    if words.is_empty() {
        return None;
    }
    let joined = format!(" {} ", words.join(" "));
    if let Some(phrase) = DIRECT_POINTER_PHRASES
        .iter()
        .find(|phrase| joined.contains(&format!(" {phrase} ")))
    {
        return Some((*phrase).to_string());
    }
    if let Some(matched) = find_lead_target(&words, POINTER_VERB_LEADS, ANY_SURFACE_TARGETS) {
        return Some(matched);
    }
    find_lead_target(&words, EXPLICIT_SURFACE_LEADS, EXPLICIT_SURFACE_TARGETS)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InternalRef {
    token: String,
    /// Matching work item's title when `token` is a known `W<n>`.
    item_title: Option<String>,
}

fn left_boundary(chars: &[char], i: usize) -> bool {
    i == 0 || {
        let prev = chars[i - 1];
        !prev.is_alphanumeric() && !matches!(prev, '-' | '_' | '/' | '.' | '@' | '#')
    }
}

fn right_boundary(chars: &[char], end: usize) -> bool {
    if end >= chars.len() {
        return true;
    }
    let next = chars[end];
    let domain_dot = next == '.'
        && chars
            .get(end + 1)
            .is_some_and(|after| after.is_alphanumeric());
    !next.is_alphanumeric() && !matches!(next, '-' | '_' | '/') && !domain_dot
}

fn known_thread_id(items: &[WorkItem], token: &str) -> bool {
    items
        .iter()
        .any(|item| item.threads.iter().any(|thread| thread.session_id == token))
}

/// Internal references in `title` (outside quotes and backticks): `W<n>`, `Q<n>`, `S<n>:`,
/// `G[0-9a-z]{4}` thread refs, and commit-like hex IDs of 7+ chars.
fn internal_refs_in_title(title: &str, items: &[WorkItem]) -> Vec<InternalRef> {
    let cleaned = unquoted(title);
    let chars: Vec<char> = cleaned.chars().collect();
    let mut found: Vec<InternalRef> = Vec::new();
    let mut push_unique = |token: String, item_title: Option<String>| {
        if !found
            .iter()
            .any(|seen| seen.token.eq_ignore_ascii_case(&token))
        {
            found.push(InternalRef { token, item_title });
        }
    };
    let mut i = 0;
    while i < chars.len() {
        if !left_boundary(&chars, i) {
            i += 1;
            continue;
        }
        let ch = chars[i];
        // 1. Global/server refs starting with `S<digits>:` (e.g. `S90:` or `S90:P3lv0:G9jt7`).
        if ch == 'S' {
            let mut end = i + 1;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            if end > i + 1 && chars.get(end) == Some(&':') {
                let mut ref_end = end + 1;
                while ref_end < chars.len()
                    && (chars[ref_end].is_ascii_alphanumeric() || chars[ref_end] == ':')
                {
                    ref_end += 1;
                }
                let token: String = chars[i..ref_end].iter().collect();
                push_unique(token, None);
                i = ref_end;
                continue;
            }
        }
        // 2. Work-item IDs (`W12`, `w34`).
        if matches!(ch, 'W' | 'w') {
            let mut end = i + 1;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            if end > i + 1 && chars[i + 1] != '0' && right_boundary(&chars, end) {
                let digits: String = chars[i + 1..end].iter().collect();
                let item_title = digits.parse::<i64>().ok().and_then(|number| {
                    items
                        .iter()
                        .find(|item| item.number == number)
                        .map(|item| item.title.trim().to_string())
                        .filter(|title| !title.is_empty())
                });
                let token = format!("W{digits}");
                push_unique(token, item_title);
                i = end;
                continue;
            }
        }
        // 3. Question tags (`Q1`, `Q4816`, `q12`).
        if matches!(ch, 'Q' | 'q') {
            let mut end = i + 1;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            if end > i + 1 && chars[i + 1] != '0' && right_boundary(&chars, end) {
                let digits: String = chars[i + 1..end].iter().collect();
                push_unique(format!("Q{digits}"), None);
                i = end;
                continue;
            }
        }
        // 4. Thread/session refs (`G[0-9a-z]{4}`): uppercase `G` plus 4 base36 chars, requiring at
        //    least one digit (or matching a known attached thread ID) so capitalized 5-letter English
        //    words like "Group", "Guide", "Ghost" or "Great" never false-positive.
        if ch == 'G' && i + 5 <= chars.len() {
            let tail = &chars[i + 1..i + 5];
            let all_base36 = tail
                .iter()
                .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase());
            if all_base36 && right_boundary(&chars, i + 5) {
                let token: String = chars[i..i + 5].iter().collect();
                let has_digit = tail.iter().any(|c| c.is_ascii_digit());
                if has_digit || known_thread_id(items, &token) {
                    push_unique(token, None);
                    i += 5;
                    continue;
                }
            }
        }
        // 5. Commit-like hex IDs of 7+ chars: all lowercase (or all uppercase) hex with at least one
        //    digit and at least one hex letter a-f so plain numbers and words like "defaced" do not
        //    false-positive.
        if ch.is_ascii_hexdigit() {
            let mut end = i;
            while end < chars.len() && chars[end].is_ascii_hexdigit() {
                end += 1;
            }
            let len = end - i;
            if len >= 7 && right_boundary(&chars, end) {
                let slice = &chars[i..end];
                let has_digit = slice.iter().any(|c| c.is_ascii_digit());
                let has_lower_hex = slice.iter().any(|c| matches!(c, 'a'..='f'));
                let has_upper_hex = slice.iter().any(|c| matches!(c, 'A'..='F'));
                let consistent_case = !(has_lower_hex && has_upper_hex);
                if has_digit && (has_lower_hex || has_upper_hex) && consistent_case {
                    let token: String = slice.iter().collect();
                    push_unique(token, None);
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    found
}

fn collapsed_char_count(text: &str) -> usize {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .count()
}

fn check_question(
    title: &str,
    detail: &str,
    items: &[WorkItem],
    detail_field: &str,
) -> Option<String> {
    let mut problems = Vec::new();
    if let Some(phrase) = pointer_phrase(title).or_else(|| pointer_phrase(detail)) {
        problems.push(format!(
            "Do not ask a question that points at a Live page or form (matched \"{phrase}\"). A Live page or form asks its own questions; remove this ask and let the page or form collect the answer directly."
        ));
    }
    let refs = internal_refs_in_title(title, items);
    if !refs.is_empty() {
        let tokens: Vec<&str> = refs.iter().map(|r| r.token.as_str()).collect();
        let named: Vec<String> = refs
            .iter()
            .filter_map(|r| {
                r.item_title
                    .as_deref()
                    .map(|item_title| format!("{} (\"{}\")", r.token, item_title))
            })
            .collect();
        let mut message = format!(
            "The title contains internal references ({}) that mean nothing to someone who has been away for a day. Remove work-item IDs (W12), question tags (Q4816), thread or session refs (G9jt7, S90:) and commit hashes from the title, and name the subject in plain words instead.",
            tokens.join(", ")
        );
        if !named.is_empty() {
            message.push_str(&format!(" Matching work items: {}.", named.join(", ")));
        }
        problems.push(message);
    }
    let detail_len = collapsed_char_count(detail);
    if detail_len == 0 {
        problems.push(format!(
            "Missing description: pass {detail_field} with at least {MIN_DETAIL_CHARS} characters so the user can decide without opening the chat or reading anything else. Include what it is in plain words, what each answer does, what you recommend and why, any risks, and links."
        ));
    } else if detail_len < MIN_DETAIL_CHARS {
        problems.push(format!(
            "Description is too short ({detail_len} characters; minimum is {MIN_DETAIL_CHARS}) for the user to decide without extra context. Expand {detail_field} to cover what it is in plain words, what each answer does, what you recommend and why, any risks, and links."
        ));
    }
    match problems.len() {
        0 => None,
        1 => problems.into_iter().next(),
        _ => Some(format!(
            "This question cannot be asked yet:\n- {}",
            problems.join("\n- ")
        )),
    }
}

/// Why `item ask` refuses `title` and `detail`, or `None` when the question may stand.
pub fn ask_refusal(title: &str, detail: &str, items: &[WorkItem]) -> Option<String> {
    check_question(title, detail, items, "--detail")
}

/// Most answers an `item ask` may offer: its dropdown above the chat box holds a short pick, not a
/// list to compare.
pub const MAX_ASK_ANSWERS: usize = 4;

/// Longest answer label (in trimmed, whitespace-collapsed characters) an `item ask` may offer; a
/// longer one is cut off with "..." in the dropdown.
pub const MAX_ASK_ANSWER_CHARS: usize = 40;

// CDXC:Coordinators 2026-11-26 DECISION:
// User (lag-fixes ask): an `item ask --multiple` with six answers, each a long sentence ("Chat
// engine: stop building each chat twice per update and ..."), showed every one cut off with "..." in
// the Work panel dropdown; it should have been a Live page. `item ask` now refuses more than 4 answers
// or any answer over 40 characters, pointing at `--detail` for short labels and a Live page for
// choices that need a sentence each.
/// Why `item ask` refuses its answer list, or `None` when the answers fit its dropdown.
pub fn answers_refusal(answers: &[String]) -> Option<String> {
    // A trailing `*` marks a recommended answer as stored (`mark_answer_list`); it is not shown.
    let answers: Vec<&str> = answers
        .iter()
        .map(|a| a.trim().trim_end_matches('*').trim())
        .filter(|a| !a.is_empty())
        .collect();
    let mut problems = Vec::new();
    if answers.len() > MAX_ASK_ANSWERS {
        problems.push(format!(
            "Too many answers ({}; most is {MAX_ASK_ANSWERS}). `item ask` is for a short pick. When there are more choices, or the user needs to compare them, put them on a Live page instead: a short card per choice with its explanation, and the question (pick any) under the cards.",
            answers.len()
        ));
    }
    let long: Vec<String> = answers
        .iter()
        .filter(|a| collapsed_char_count(a) > MAX_ASK_ANSWER_CHARS)
        .map(|a| format!("\"{a}\" ({} characters)", collapsed_char_count(a)))
        .collect();
    if !long.is_empty() {
        problems.push(format!(
            "Answers are too long for the dropdown, which cuts them off with \"...\" (most is {MAX_ASK_ANSWER_CHARS} characters): {}. Make each answer a few-word label like \"Fix as suggested\" and explain each one in --detail. When each choice needs a sentence to explain, put them on a Live page instead: a short card per choice with its explanation, and the question under the cards.",
            long.join(", ")
        ));
    }
    match problems.len() {
        0 => None,
        1 => problems.into_iter().next(),
        _ => Some(format!(
            "This question cannot be asked yet:\n- {}",
            problems.join("\n- ")
        )),
    }
}

/// Why `ghostex visual check` refuses a ```questions block question, or `None` when it may stand.
pub fn form_question_refusal(title: &str, detail: &str) -> Option<String> {
    check_question(title, detail, &[], "\"detail\"")
}

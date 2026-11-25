//! `ghostex visual check`'s warning about a pick-one question that looks like several questions in
//! one: a long list of choices, or choices that share their first part before a ";" (two decisions
//! crossed). It warns and never refuses, for forms (visual.rs) and pages (visual_page_check.rs).
//!
//! CDXC:SessionChat 2026-11-25 DECISION:
//! User, after a page asked 10 menu looks as one pick-one question and their answer was "A (but bit darker bg in darkmode pls) + D + G" typed under Other: questions "that i can make multiple choices in as single choice" and "everything together in long list" are bad. On the page that followed (Q4911, Q4912) they picked "one question per dimension", "pick any unless choices truly exclude", and a `visual check` warning, but not a fixed cap on choices: so the warning names the sign it saw and says how to split, and promises no number. The rules themselves are in server/src/coordinators/playbook.md ("Choices") and server/src/ghostex_cli/visual_guide.md.

/// A pick-one list this long is warned about: the ⋯ menu page (Q4904) had 10 and the sidebar row
/// page (Q4893) 8, while one-dimension lists of 6 (where a dot goes) read fine.
const LONG_LIST: usize = 7;

/// How many choices must share their part before ";" to read as two decisions crossed.
const SHARED_PREFIX: usize = 3;

/// One choice question as the check sees it.
struct AskedChoices<'a> {
    /// Its tag, else "Question N".
    label: String,
    title: &'a str,
    multiple: bool,
    choices: Vec<&'a str>,
}

/// One warning line per pick-one question that looks like it mixes kinds of choice.
fn mixed_choice_warnings(questions: &[AskedChoices<'_>]) -> Vec<String> {
    questions
        .iter()
        .filter(|question| !question.multiple)
        .filter_map(warning)
        .collect()
}

fn warning(question: &AskedChoices<'_>) -> Option<String> {
    let mut signs = Vec::new();
    if question.choices.len() >= LONG_LIST {
        signs.push(format!(
            "it has {} pick-one choices, and a list that long usually mixes kinds of choice (a look, a layout, extras)",
            question.choices.len()
        ));
    }
    if let Some((prefix, count)) = shared_prefix(&question.choices) {
        signs.push(format!(
            "{count} of its choices start \"{prefix};\", so two decisions are crossed in one list (the part before \";\" and the part after it)"
        ));
    }
    if signs.is_empty() {
        return None;
    }
    Some(format!(
        "{} (\"{}\"): {}. Ask one question per kind of choice, and add `multiple` wherever the choices can combine, so a mix is several picks instead of Other and a note.",
        question.label,
        question.title,
        signs.join("; and ")
    ))
}

/// The warnings for a ```questions block.
pub(crate) fn form_warnings(form: &ghostex_gx_visual::questions::QuestionForm) -> Vec<String> {
    let asked: Vec<AskedChoices<'_>> = form
        .questions
        .iter()
        .enumerate()
        .filter(|(_, question)| !question.text)
        .map(|(index, question)| AskedChoices {
            label: label(&question.tag, index),
            title: &question.title,
            multiple: question.multiple,
            choices: question
                .choices
                .iter()
                .map(|choice| choice.label.as_str())
                .collect(),
        })
        .collect();
    mixed_choice_warnings(&asked)
}

/// The warnings for a page file's own questions (its `<gx-choice>` lines, not rows picked on the
/// page), or none when the file can't be read (the check itself says so).
pub(crate) fn page_warnings(path: &std::path::Path) -> Vec<String> {
    let Ok(html) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let questions = crate::visual_pages::written_questions(&html);
    let asked: Vec<AskedChoices<'_>> = questions
        .iter()
        .enumerate()
        .filter(|(_, question)| !question.text)
        .map(|(index, question)| AskedChoices {
            label: label(&question.tag, index),
            title: &question.title,
            multiple: question.multiple,
            choices: question
                .choices
                .iter()
                .map(|choice| choice.label.as_str())
                .collect(),
        })
        .collect();
    mixed_choice_warnings(&asked)
}

/// How a warning names a question: its tag, else its place.
fn label(tag: &str, index: usize) -> String {
    if tag.is_empty() {
        format!("Question {}", index + 1)
    } else {
        tag.to_string()
    }
}

/// The part before ";" most choices share, when at least [`SHARED_PREFIX`] of them do.
fn shared_prefix<'a>(choices: &[&'a str]) -> Option<(&'a str, usize)> {
    let prefixes: Vec<&str> = choices
        .iter()
        .filter_map(|choice| without_letter(choice).split_once(';'))
        .map(|(before, _)| before.trim())
        .filter(|before| !before.is_empty())
        .collect();
    prefixes
        .iter()
        .map(|prefix| {
            let count = prefixes
                .iter()
                .filter(|other| other.eq_ignore_ascii_case(prefix))
                .count();
            (*prefix, count)
        })
        .filter(|(_, count)| *count >= SHARED_PREFIX)
        .max_by_key(|(_, count)| *count)
}

/// The choice without a leading letter ("A. ", "B) ") the author may have written.
fn without_letter(choice: &str) -> &str {
    let trimmed = choice.trim_start();
    let mut chars = trimmed.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some(letter), Some('.' | ')'), Some(' ')) if letter.is_ascii_alphabetic() => {
            trimmed[3..].trim_start()
        }
        _ => trimmed,
    }
}

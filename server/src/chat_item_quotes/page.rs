//! The quote of a Live page a message points at (`[…](page:<id>)`): its title, when it was sent,
//! the file it was published from, and the questions on it still open.

use std::path::Path;

use ghostex_gx_chat_core::transcript::item_references::{cut, QuotedItem, ITEM_QUOTE_MAX_LINES};

use super::quote::{local_time, QUOTED_CHARS};

/// The most characters of one open question's words.
const QUESTION_CHARS: usize = 120;

/// `Live page "Settings redesign" (sent 2026-11-25 13:40)`, then `File: …` and an `Open: Q1 …`
/// line for each question not answered yet (or `All answered`).
pub(super) fn quote(state_dir: &Path, id: &str) -> QuotedItem {
    let Some(page) = crate::visual_pages::page_overview(state_dir, id) else {
        return QuotedItem {
            heading: format!("Live page {id}"),
            lines: Some(vec![
                "Ghostex has no Live page with this id (pages are deleted 90 days after they are sent)."
                    .to_string(),
            ]),
        };
    };
    let heading = match page.created_at.as_deref().and_then(local_time) {
        Some(sent) => format!("Live page \"{}\" (sent {sent})", cut(&page.title, 80)),
        None => format!("Live page \"{}\"", cut(&page.title, 80)),
    };
    let mut lines: Vec<String> = Vec::new();
    if let Some(path) = &page.source_path {
        lines.push(format!("File: {path}"));
    }
    if page.questions.is_empty() {
        lines.push("It asks no questions.".to_string());
    } else if let Some(at) = &page.answered_at {
        lines.push(match local_time(at) {
            Some(at) => format!("All answered (the answers were sent {at})"),
            None => "All answered".to_string(),
        });
    } else {
        let open: Vec<String> = page
            .questions
            .iter()
            .map(|question| {
                let words = cut(question.title.trim(), QUESTION_CHARS);
                match question.tag.trim() {
                    "" => format!("Open: {words}"),
                    tag => format!("Open: {tag} {words}"),
                }
            })
            .collect();
        // Room for the open questions within the quote's lines and characters, a line kept for
        // naming how many more there are.
        let room = ITEM_QUOTE_MAX_LINES.saturating_sub(lines.len() + 1);
        let mut used = 0;
        let mut shown = 0;
        for line in &open {
            if shown >= room || used + line.chars().count() > QUOTED_CHARS {
                break;
            }
            used += line.chars().count();
            lines.push(line.clone());
            shown += 1;
        }
        if shown < open.len() {
            lines.push(format!("And {} more open.", open.len() - shown));
        }
        if let Some((answered, total)) = crate::visual_pages::page_question_counts(state_dir, id)
            .filter(|(answered, _)| *answered > 0)
        {
            if lines.len() < ITEM_QUOTE_MAX_LINES {
                lines.push(format!(
                    "The reader has picked answers for {answered} of {total} on the page, not sent yet."
                ));
            }
        }
    }
    QuotedItem {
        heading,
        lines: Some(lines),
    }
}

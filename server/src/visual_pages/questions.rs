//! The questions a published page asks, read from its `<gx-questions>` markup the way the page's
//! question kit (`kit.rs`) reads it in the browser: each `<gx-question>`'s answer key (its `name`,
//! else its title, numbered when repeated), tag, kind and choices (each choice's `value`, else its
//! label without its `<small>` detail). gxserver's voice agent lists and answers them from here
//! (server/src/voice_agent/question_forms.rs), and the answers go through the page's own route.
//!
//! CDXC:SessionChat 2026-10-20 SEE-ALSO:
//! `question()` and `choice()` in kit.rs's script decide the same keys and values in the browser; change them together.

use std::path::Path;

use super::store::{read_page, read_record};

/// One choice of a page question.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageChoice {
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) recommended: bool,
}

/// One `<gx-question>` of a page.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageQuestion {
    /// What the answers are keyed by.
    pub(crate) key: String,
    pub(crate) tag: String,
    pub(crate) title: String,
    pub(crate) multiple: bool,
    pub(crate) text: bool,
    pub(crate) choices: Vec<PageChoice>,
}

/// A page's title and questions, when it is a page gxserver published.
pub(crate) fn page_questions(
    gxserver_state_dir: &Path,
    id: &str,
) -> Option<(String, Vec<PageQuestion>)> {
    let record = read_record(gxserver_state_dir, id)?;
    let html = String::from_utf8(read_page(gxserver_state_dir, id)?).ok()?;
    let title = record
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Live page")
        .to_string();
    Some((title, parse_questions(&html)))
}

/// The question tags a page's `<gx-questions>` markup asks (`Q12`), uppercased, in order.
pub(crate) fn page_tags(html: &str) -> Vec<String> {
    let mut tags: Vec<String> = parse_questions(html)
        .into_iter()
        .map(|question| question.tag.trim().to_ascii_uppercase())
        .filter(|tag| {
            tag.strip_prefix('Q').is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
            })
        })
        .collect();
    tags.dedup();
    tags
}

/// Whether a question on the page still has no answer in the reader's draft: its kit writes every
/// question's key into the draft (`collect()` in kit.rs), as `"skipped"` while a question has no
/// pick and no text, so a key that is missing or skipped is a question left. False once the page
/// was sent, and for a page with no questions unless it waits to be marked seen. The desktop's page window reads it for the dot on a
/// hidden page's tab in the chat header (apps/desktop/src/app/visual_page_hidden.rs).
pub(crate) fn page_questions_left(gxserver_state_dir: &Path, id: &str) -> bool {
    if page_answered(gxserver_state_dir, id) {
        return false;
    }
    // A page that asks nothing but waits to be marked seen (publish.rs) is left until it is.
    if read_record(gxserver_state_dir, id).and_then(|record| record.get("awaitsSeen")?.as_bool())
        == Some(true)
    {
        return true;
    }
    let Some((_, questions)) = page_questions(gxserver_state_dir, id) else {
        return false;
    };
    let draft = read_record(gxserver_state_dir, id)
        .and_then(|record| record.get("draft")?.get("kit").cloned())
        .unwrap_or(serde_json::Value::Null);
    questions
        .iter()
        .any(|question| match draft.get(&question.key) {
            None => true,
            Some(serde_json::Value::String(value)) => value == "skipped",
            Some(_) => false,
        })
}

/// How many of the page's questions the reader's draft answers, and how many it asks:
/// `(answered, total)`, a question counting as answered once its key in the draft has a pick or
/// text (anything but missing or `"skipped"`, as `page_questions_left` reads it). `None` for a page
/// with no questions or one gxserver does not have. A draft answered on the page but not sent
/// yet counts; once the page is sent every question counts as answered.
///
/// CDXC:Coordinators 2026-11-11 DECISION:
/// User, of a page row in the dock: "also show (10/16 answered) or (14 questions) next to the
/// page name to know how many i need to answer" (gx-chat-core work_cards.rs `page_count`).
pub(crate) fn page_question_counts(gxserver_state_dir: &Path, id: &str) -> Option<(usize, usize)> {
    let (_, questions) = page_questions(gxserver_state_dir, id)?;
    if questions.is_empty() {
        return None;
    }
    if page_answered(gxserver_state_dir, id) {
        return Some((questions.len(), questions.len()));
    }
    let draft = read_record(gxserver_state_dir, id)
        .and_then(|record| record.get("draft")?.get("kit").cloned())
        .unwrap_or(serde_json::Value::Null);
    let answered = questions
        .iter()
        .filter(|question| match draft.get(&question.key) {
            None => false,
            Some(serde_json::Value::String(value)) => value != "skipped",
            Some(_) => true,
        })
        .count();
    Some((answered, questions.len()))
}

/// Whether the page's answers were already sent (they may still wait in the session's queue).
pub(crate) fn page_answered(gxserver_state_dir: &Path, id: &str) -> bool {
    read_record(gxserver_state_dir, id)
        .is_some_and(|record| record.get("answeredAt").is_some_and(|at| !at.is_null()))
}

/// What the quote of a pointed-at Live page and `ghostex question show` say of a page: its title,
/// when it was published, the file it came from, its questions and the answers sent from it.
pub(crate) struct PageOverview {
    pub(crate) title: String,
    /// RFC 3339, from the page's record.
    pub(crate) created_at: Option<String>,
    pub(crate) source_path: Option<String>,
    pub(crate) answered_at: Option<String>,
    pub(crate) questions: Vec<PageQuestion>,
    /// The answers sent from the page, by question key (`Null` until sent).
    pub(crate) answers: serde_json::Value,
}

/// The [`PageOverview`] of a page gxserver published; `None` for one it does not have.
pub(crate) fn page_overview(gxserver_state_dir: &Path, id: &str) -> Option<PageOverview> {
    let record = read_record(gxserver_state_dir, id)?;
    let text = |key: &str| {
        record
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let (title, questions) = page_questions(gxserver_state_dir, id).unwrap_or_else(|| {
        (
            text("title").unwrap_or_else(|| "Live page".into()),
            Vec::new(),
        )
    });
    Some(PageOverview {
        title,
        created_at: text("createdAt"),
        source_path: text("sourcePath"),
        answered_at: text("answeredAt"),
        questions,
        answers: record
            .get("answers")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    })
}

fn parse_questions(html: &str) -> Vec<PageQuestion> {
    let mut questions = written_questions(html);
    add_row_choices(&mut questions, &page_rows(html));
    questions
}

/// The page's questions with only the `<gx-choice>` lines the author wrote, no rows added: what
/// `ghostex visual check` judges a question's own list by (ghostex_cli/visual_choice_mix.rs).
pub(crate) fn written_questions(html: &str) -> Vec<PageQuestion> {
    let mut questions = Vec::new();
    let mut used: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(at) = find_tag(rest, "gx-question") {
        let after = &rest[at + "<gx-question".len()..];
        let head_end = after.find('>').unwrap_or(after.len());
        let head = &after[..head_end];
        let body_start = (head_end + 1).min(after.len());
        let body_end = after[body_start..]
            .find("</gx-question>")
            .map_or(after.len(), |end| body_start + end);
        let body = &after[body_start..body_end];
        rest = &after[body_end..];
        let title = decode(attribute(head, "title").unwrap_or_default().trim());
        let mut key = decode(attribute(head, "name").unwrap_or_default().trim());
        if key.is_empty() {
            key = if title.is_empty() {
                format!("Question {}", questions.len() + 1)
            } else {
                title.clone()
            };
        }
        let base = key.clone();
        let mut copy = 2;
        while used.contains(&key) {
            key = format!("{base} ({copy})");
            copy += 1;
        }
        used.push(key.clone());
        let text = attribute(head, "type").is_some_and(|kind| kind.eq_ignore_ascii_case("text"));
        questions.push(PageQuestion {
            key,
            tag: decode(attribute(head, "tag").unwrap_or_default().trim()),
            title,
            multiple: has_attribute(head, "multiple"),
            text,
            choices: if text {
                Vec::new()
            } else {
                parse_choices(body)
            },
        });
    }
    questions
}

/// One pickable row of a page (`<gx-pick value>` or an element with `data-pick`), as the kit's
/// `rowNodes` / `rowAsks` / `rowValue` / `rowLabel` read it.
#[derive(Debug)]
struct PageRow {
    asked: String,
    value: String,
    label: String,
    recommended: bool,
}

/// Each row becomes a choice of the question it names (by key, title or tag) unless the question
/// lists it already, and the question's choices go in the rows' order with the others after them,
/// as the kit's `addRowChoices` and `inVariantOrder` do in the browser.
fn add_row_choices(questions: &mut [PageQuestion], rows: &[PageRow]) {
    for question in questions.iter_mut().filter(|question| !question.text) {
        let mine: Vec<&PageRow> = rows
            .iter()
            .filter(|row| {
                row.asked == question.key
                    || row.asked == question.title
                    || (!question.tag.is_empty() && row.asked == question.tag)
            })
            .collect();
        if mine.is_empty() {
            continue;
        }
        let mut ordered = Vec::with_capacity(question.choices.len() + mine.len());
        for row in &mine {
            let written = question.choices.iter().position(|choice| {
                choice.value == row.value || choice.label == row.label || choice.label == row.value
            });
            ordered.push(match written {
                Some(index) => question.choices.remove(index),
                None => PageChoice {
                    label: row.label.clone(),
                    value: row.value.clone(),
                    recommended: row.recommended,
                },
            });
        }
        ordered.append(&mut question.choices);
        question.choices = ordered;
    }
}

/// The page's rows, read with a small tag scanner that follows which `data-question` each row sits
/// under; rows inside a `<gx-question>`, inside another row, or in raw-text elements are skipped.
fn page_rows(html: &str) -> Vec<PageRow> {
    const VOID: &[&str] = &[
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source",
        "track", "wbr",
    ];
    const RAW: &[&str] = &["script", "style", "textarea", "pre", "template", "title"];
    let lower = html.to_ascii_lowercase();
    // Open elements: tag name, the data-question they carry, and whether they are a row.
    let mut open: Vec<(String, Option<String>, bool)> = Vec::new();
    let mut rows = Vec::new();
    let mut at = 0;
    while let Some(lt) = html[at..].find('<').map(|found| at + found) {
        if lower[lt..].starts_with("<!--") {
            at = lower[lt..]
                .find("-->")
                .map_or(html.len(), |end| lt + end + 3);
            continue;
        }
        let Some(gt) = html[lt..].find('>').map(|found| lt + found) else {
            break;
        };
        let tag = &html[lt + 1..gt];
        at = gt + 1;
        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim().to_ascii_lowercase();
            if let Some(index) = open.iter().rposition(|(open_name, ..)| *open_name == name) {
                open.truncate(index);
            }
            continue;
        }
        let name: String = tag
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '-')
            .collect::<String>()
            .to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if RAW.contains(&name.as_str()) {
            at = lower[at..]
                .find(&format!("</{name}"))
                .map_or(html.len(), |end| at + end);
            continue;
        }
        let head = &tag[name.len()..];
        let is_row = name == "gx-pick" || has_attribute(head, "data-pick");
        let inside = open
            .iter()
            .any(|(open_name, _, row)| *row || open_name == "gx-question");
        if is_row && !inside {
            let asked = attribute(head, "question")
                .or_else(|| attribute(head, "data-question"))
                .or_else(|| open.iter().rev().find_map(|(_, asked, _)| asked.clone()))
                .map(|asked| decode(asked.trim()))
                .unwrap_or_default();
            let label = row_label(&html[at..]);
            let value = decode(
                if name == "gx-pick" {
                    attribute(head, "value")
                } else {
                    attribute(head, "data-pick")
                }
                .unwrap_or_default()
                .trim(),
            );
            if !asked.is_empty() {
                rows.push(PageRow {
                    asked,
                    value: if value.is_empty() {
                        label.clone()
                    } else {
                        value
                    },
                    label,
                    recommended: has_attribute(head, "recommended")
                        || has_attribute(head, "data-recommended"),
                });
            }
        }
        if !VOID.contains(&name.as_str()) && !tag.trim_end().ends_with('/') {
            open.push((name, attribute(head, "data-question"), is_row));
        }
    }
    rows
}

/// A row's label: its first heading when the row starts with one, else its text (80 characters).
fn row_label(after_start: &str) -> String {
    let trimmed = after_start.trim_start();
    let heading = trimmed.len() > 3
        && trimmed.as_bytes()[0] == b'<'
        && matches!(trimmed.as_bytes()[1], b'h' | b'H')
        && (b'1'..=b'6').contains(&trimmed.as_bytes()[2]);
    if heading {
        let level = &trimmed[1..3].to_ascii_lowercase();
        if let Some(end) = trimmed.to_ascii_lowercase().find(&format!("</{level}")) {
            let inner = trimmed[..end]
                .find('>')
                .map_or("", |gt| &trimmed[gt + 1..end]);
            return label_text(inner);
        }
    }
    let text = label_text(&after_start[..after_start.len().min(4000)]);
    if text.chars().count() > 80 {
        text.chars().take(79).collect::<String>() + "\u{2026}"
    } else {
        text
    }
}

fn parse_choices(body: &str) -> Vec<PageChoice> {
    let mut choices = Vec::new();
    let mut rest = body;
    while let Some(at) = find_tag(rest, "gx-choice") {
        let after = &rest[at + "<gx-choice".len()..];
        let head_end = after.find('>').unwrap_or(after.len());
        let head = &after[..head_end];
        let inner_start = (head_end + 1).min(after.len());
        let inner_end = after[inner_start..]
            .find("</gx-choice>")
            .map_or(after.len(), |end| inner_start + end);
        let label = label_text(&after[inner_start..inner_end]);
        rest = &after[inner_end..];
        let value = decode(attribute(head, "value").unwrap_or_default().trim());
        choices.push(PageChoice {
            value: if value.is_empty() {
                label.clone()
            } else {
                value
            },
            label,
            recommended: has_attribute(head, "recommended"),
        });
    }
    choices
}

/// Where `<name` starts as a whole tag name.
fn find_tag(text: &str, name: &str) -> Option<usize> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(found) = text[from..].find(&open) {
        let at = from + found;
        let next = text[at + open.len()..].chars().next();
        if next.is_some_and(|character| {
            character.is_whitespace() || character == '>' || character == '/'
        }) {
            return Some(at);
        }
        from = at + open.len();
    }
    None
}

/// The choice's text the kit shows as its label: its content without `<small>` details or tags,
/// whitespace collapsed.
fn label_text(inner: &str) -> String {
    let mut text = String::new();
    let mut rest = inner;
    while let Some(at) = rest.find("<small") {
        text.push_str(&rest[..at]);
        rest = rest[at..]
            .find("</small>")
            .map_or("", |end| &rest[at + end + "</small>".len()..]);
    }
    text.push_str(rest);
    let mut plain = String::new();
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => plain.push(character),
            _ => {}
        }
    }
    decode(&plain.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn decode(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

/// An attribute's value in a start tag's text (`tag="Q1"` or `tag='Q1'`).
fn attribute(head: &str, name: &str) -> Option<String> {
    let mut search = head;
    while let Some(at) = search.find(name) {
        let before = search[..at].chars().next_back();
        let after = search[at + name.len()..].trim_start();
        search = &search[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value
            .chars()
            .next()
            .filter(|quote| *quote == '"' || *quote == '\'')?;
        let value = &value[1..];
        return Some(value[..value.find(quote)?].to_string());
    }
    None
}

/// Whether a start tag carries a bare or valued attribute.
fn has_attribute(head: &str, name: &str) -> bool {
    head.split(|character: char| character.is_whitespace())
        .any(|word| word == name || word.starts_with(&format!("{name}=")))
}

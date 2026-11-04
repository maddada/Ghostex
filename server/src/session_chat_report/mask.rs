//! What Copy Report hides: everything in the report that names the user, their machine or their
//! work, so a report can be pasted into a public GitHub issue as it is.
//!
//! CDXC:SessionChat 2026-10-29 DECISION:
//! User: "pls remove any user related stuff in Copy Report, i think the session id/name shouldn't be included for example". The report no longer carries the session's Global Ref, its name or its terminal name (which contains the ref), and every other line it copies (the input-box reason, the terminal notice, the queue's failure reasons, the send log's messages) is masked: Ghostex session refs and ids, terminal names, UUIDs, emails, URLs, folder paths (home, project and any other absolute path), and the session's, project's, account's and user's names become `<session ref>`, `<id>`, `<terminal>`, `<uuid>`, `<email>`, `<url>`, `<path>`, `<session name>`, `<project>`, `<account>` and `<user>`. What helps debug and names nobody stays: the Ghostex version, OS and architecture, the agent kind, the state, the terminal and client sizes, the verdict, the queue and the problem codes and times. Asked whether the terminal screen should stay (it holds conversation text), the user answered "Keep the screen, with IDs, paths and emails masked", so the screen keeps its text through the same mask (`report_screen`).
//! SEE-ALSO: ../session_chat_report.rs (builds the report and applies this mask), packages/gx-chat-core/src/extras/report.rs (copies it).

/// The session's own values the mask hides wherever they appear, with the label each becomes.
#[derive(Default)]
pub(crate) struct ReportMask {
    /// Longest first, so a project path is hidden before the folder name inside it.
    literals: Vec<(String, &'static str)>,
}

impl ReportMask {
    /// `values` are `(text, label)`; empty ones and one-letter ones are dropped.
    pub(crate) fn new(values: impl IntoIterator<Item = (String, &'static str)>) -> Self {
        let mut literals: Vec<(String, &'static str)> = values
            .into_iter()
            .map(|(text, label)| (text.trim().to_string(), label))
            .filter(|(text, _)| text.chars().count() >= 2)
            .collect();
        literals.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        literals.dedup_by(|a, b| a.0 == b.0);
        Self { literals }
    }

    /// `text` with the session's own values and every ref, id, UUID, email, URL and path masked.
    pub(crate) fn apply(&self, text: &str) -> String {
        let mut masked = text.to_string();
        for (literal, label) in &self.literals {
            masked = replace_bounded(&masked, literal, label);
        }
        mask_tokens(&masked)
    }
}

/// The terminal screen as the report prints it: the screen's own text, masked.
///
/// The one place to change if the screen's treatment changes (drop it, or keep it whole).
pub(crate) fn report_screen(mask: &ReportMask, screen: &str) -> String {
    screen
        .lines()
        .map(|line| mask.apply(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replaces `literal` where it stands on its own, not inside a longer word (`madda` is hidden in
/// `/Users/madda` and `madda's`, not in `maddada`).
fn replace_bounded(text: &str, literal: &str, label: &str) -> String {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(literal) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + literal.len()..].chars().next();
        let starts_word = literal.chars().next().is_some_and(is_word);
        let ends_word = literal.chars().next_back().is_some_and(is_word);
        let bounded = !(starts_word && before.is_some_and(is_word))
            && !(ends_word && after.is_some_and(is_word));
        out.push_str(&rest[..at]);
        if bounded {
            out.push_str(label);
        } else {
            out.push_str(literal);
        }
        rest = &rest[at + literal.len()..];
    }
    out.push_str(rest);
    out
}

/// Masks each whitespace-separated token on its own, keeping the whitespace.
fn mask_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut token = String::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            out.push_str(&mask_token(&token));
            token.clear();
            out.push(ch);
        } else {
            token.push(ch);
        }
    }
    out.push_str(&mask_token(&token));
    out
}

/// One token: the punctuation around it kept, what it is masked whole (a URL, an email, a path,
/// a ref), else every UUID and Ghostex id inside it.
fn mask_token(token: &str) -> String {
    let core_start = token
        .find(|ch: char| !matches!(ch, '(' | '[' | '{' | '"' | '\'' | '`'))
        .unwrap_or(token.len());
    let core_end = token
        .rfind(|ch: char| {
            !matches!(
                ch,
                ')' | ']' | '}' | '"' | '\'' | '`' | ',' | '.' | ';' | ':' | '!' | '?'
            )
        })
        .map_or(core_start, |at| {
            at + token[at..].chars().next().map_or(1, char::len_utf8)
        })
        .max(core_start);
    let (before, core, after) = (
        &token[..core_start],
        &token[core_start..core_end],
        &token[core_end..],
    );
    if core.is_empty() || (core.starts_with('<') && core.ends_with('>')) {
        return token.to_string();
    }
    let whole = if core.contains("://") {
        Some("<url>")
    } else if is_email(core) {
        Some("<email>")
    } else if is_path(core) {
        Some("<path>")
    } else if is_ref(core, ':') {
        Some("<session ref>")
    } else if is_ref(core, '-') {
        Some("<terminal>")
    } else if is_ghostex_id(core) {
        Some("<id>")
    } else {
        None
    };
    match whole {
        Some(label) => format!("{before}{label}{after}"),
        None => format!("{before}{}{after}", mask_uuids(core)),
    }
}

fn is_email(core: &str) -> bool {
    let Some((local, domain)) = core.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '.' | '-'))
}

/// A folder path: `~/…`, `…/name` (how Claude abbreviates one), a Windows drive path, or an
/// absolute path of two or more parts (`/effort` alone is a command, not a path).
fn is_path(core: &str) -> bool {
    if core.starts_with("~/") || core.starts_with("\u{2026}/") || core.starts_with(".../") {
        return true;
    }
    let bytes = core.as_bytes();
    if bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\' {
        return true;
    }
    core.starts_with('/')
        && core[1..]
            .chars()
            .next()
            .is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        && core[1..].trim_end_matches('/').contains('/')
}

/// `S90:P3lv0:G7hgi` (with `:`) or `S90-P3lv0-G7hgi` (a terminal name, with `-`).
fn is_ref(core: &str, separator: char) -> bool {
    let parts: Vec<&str> = core.split(separator).collect();
    parts.len() == 3
        && parts
            .iter()
            .zip(['S', 'P', 'G'])
            .all(|(part, letter)| id_shaped(part, letter, 2))
}

/// One Ghostex id on its own: `G1pif`, `P3lv0`, `S90`.
fn is_ghostex_id(core: &str) -> bool {
    ['S', 'P', 'G']
        .into_iter()
        .any(|letter| id_shaped(core, letter, 3))
}

/// `letter`, a digit, then lowercase letters and digits, at least `min_len` characters.
fn id_shaped(part: &str, letter: char, min_len: usize) -> bool {
    let mut chars = part.chars();
    part.len() >= min_len
        && chars.next() == Some(letter)
        && chars.next().is_some_and(|ch| ch.is_ascii_digit())
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

/// Every `8-4-4-4-12` hex UUID inside `text` as `<uuid>`.
fn mask_uuids(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if index + 36 <= chars.len() && is_uuid(&chars[index..index + 36]) {
            out.push_str("<uuid>");
            index += 36;
        } else {
            out.push(chars[index]);
            index += 1;
        }
    }
    out
}

fn is_uuid(window: &[char]) -> bool {
    window.iter().enumerate().all(|(at, ch)| match at {
        8 | 13 | 18 | 23 => *ch == '-',
        _ => ch.is_ascii_hexdigit(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask() -> ReportMask {
        ReportMask::new([
            ("S90:P3lv0:G1pif".to_string(), "<session ref>"),
            ("Main Coordinator".to_string(), "<session name>"),
            ("Ghostex".to_string(), "<project>"),
            ("/Users/madda/dev/_active/Ghostex".to_string(), "<path>"),
            ("/Users/madda".to_string(), "<path>"),
            ("madda".to_string(), "<user>"),
            ("ST".to_string(), "<account>"),
        ])
    }

    #[test]
    fn identifying_text_is_masked_and_the_rest_kept() {
        let masked = mask().apply(
            "Session S90:P3lv0:G1pif \"Main Coordinator\" in /Users/madda/dev/_active/Ghostex, \
             terminal S90-P3lv0-G1pif, thread G2z0a, conversation \
             ff8d2575-b069-47b5-b375-69750cc8353f, mail me@example.com, see \
             https://github.com/maddada/Ghostex/pull/1, ~/notes/a.md and C:\\Users\\madda\\x \
             (madda on ST). Composer not ready: 3 tries, /effort, 5h 7% \u{b7} 7d 58%, maddada.",
        );
        for gone in [
            "S90",
            "P3lv0",
            "G1pif",
            "G2z0a",
            "Main Coordinator",
            "Ghostex",
            "/Users",
            "ff8d2575",
            "me@example.com",
            "github.com",
            "notes",
            "madda ",
            " ST)",
        ] {
            assert!(!masked.contains(gone), "{gone} survived: {masked}");
        }
        for kept in [
            "Composer not ready: 3 tries",
            "/effort",
            "5h 7% \u{b7} 7d 58%",
            "maddada.",
            "<session ref>",
            "\"<session name>\"",
            "<terminal>",
            "<uuid>",
            "<email>",
            "<url>",
            "<path>",
            "(<user> on <account>).",
        ] {
            assert!(masked.contains(kept), "{kept} missing: {masked}");
        }
    }

    #[test]
    fn the_screen_keeps_its_text_with_ids_paths_and_emails_masked() {
        let screen = [
            "  \u{25fb} When 10.39.1 ships: tell G2z0a the mise fix is out (for Sam)",
            "\u{2500}\u{2500}\u{2500}\u{2500} Main Coordinator \u{2500}",
            "\u{276f} ",
            "  ff8d2575-b069-47b5-b375-69750cc8353f | \u{2026}/Ghostex",
            "  Opus 5.5 | xhigh | Ctx 66% | $913.27",
        ]
        .join("\n");
        assert_eq!(
            report_screen(&mask(), &screen),
            [
                "  \u{25fb} When 10.39.1 ships: tell <id> the mise fix is out (for Sam)",
                "\u{2500}\u{2500}\u{2500}\u{2500} <session name> \u{2500}",
                "\u{276f} ",
                "  <uuid> | <path>",
                "  Opus 5.5 | xhigh | Ctx 66% | $913.27",
            ]
            .join("\n")
        );
    }
}

//! The working card's label in runs: any file path in it shortened to its file name (its folders
//! dimmed and cut to the last one).
//!
//! CDXC:SessionChat 2026-10-28 DECISION:
//! User, of the two-row working bar: "i think there's not enough space between the 2 rows there but think we could do something better also", then picked option C of the mockup (Q724 "divider"): the top row is the working icon and the action; a hairline divider in the card's border colour runs across the bar between the rows; the bottom row is the pills, left-aligned, a touch smaller (24px tall, 12px text) with even padding above and below. A file path in the action keeps its file name and dims and shortens its folders ("Reading …/Screenshots/2026-10-28_Ghostex_20-09-21@2x.png"), so the part that matters is not the part cut off. User (2026-10-29), of the action's elapsed time that option C put at the top row's right end ("3s"): "dont show the time here, no need". The top row has no time; this supersedes that part of option C.
//!
//! CDXC:SessionChat 2026-10-29 SEE-ALSO:
//! The two-row bar is gone (the pills moved into the chat box, extras/chat_pills.rs); the bar is one row, and its label keeps the shortened path.
//!
//! CDXC:SessionChat 2026-10-28 SEE-ALSO:
//! extras/working_card.rs publishes `labelParts`; apps/desktop/src/app/native_chat/working_card.rs (desktop and web) and apps/mobile/app/src/chat/native/cards/WorkingCard.tsx (phone) draw them.

use serde::{Deserialize, Serialize};

/// One run of the label: `dim` for a shortened path's folders.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingCardLabelPart {
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dim: bool,
}

/// The label in runs, each file path in it shortened: its file name stays whole and its folders
/// become one dimmed run, cut to the last folder after "…/" when there are more
/// (`~/Documents/Screenshots/a.png` → "…/Screenshots/" dimmed, then "a.png"). A web address is not a
/// path. A label without a path is one run.
pub fn label_parts(label: &str) -> Vec<WorkingCardLabelPart> {
    let mut parts: Vec<WorkingCardLabelPart> = Vec::new();
    let mut push = |text: &str, dim: bool| {
        if text.is_empty() {
            return;
        }
        match parts.last_mut() {
            Some(last) if last.dim == dim => last.text.push_str(text),
            _ => parts.push(WorkingCardLabelPart {
                text: text.to_string(),
                dim,
            }),
        }
    };
    let mut rest = label;
    while !rest.is_empty() {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, after) = rest.split_at(end);
        let space = after.len() - after.trim_start().len();
        match shorten_path(word) {
            Some((before, folders, file, trailing)) => {
                push(before, false);
                push(&folders, true);
                push(file, false);
                push(trailing, false);
            }
            None => push(word, false),
        }
        push(&after[..space], false);
        rest = &after[space..];
    }
    parts
}

/// A word holding a path, split as (what leads it, its folders shortened, its file name, what
/// trails it): `Read(~/a/b/c.rs)` → ("Read(", "…/b/", "c.rs", ")").
fn shorten_path(word: &str) -> Option<(&str, String, &str, &str)> {
    if word.contains("://") {
        return None;
    }
    let is_edge = |c: char| {
        matches!(
            c,
            '(' | ')' | '"' | '\'' | '`' | ',' | ';' | ':' | '[' | ']'
        )
    };
    // The path starts after the last bracket or quote before its first slash ("Read(~/a.rs").
    let first_slash = word.find('/')?;
    let start = word[..first_slash]
        .char_indices()
        .filter(|(_, c)| is_edge(*c))
        .last()
        .map_or(0, |(index, c)| index + c.len_utf8());
    let end = word
        .char_indices()
        .rev()
        .find(|(_, c)| !is_edge(*c) && *c != '.')
        .map(|(index, c)| index + c.len_utf8())?;
    if end <= start {
        return None;
    }
    let path = &word[start..end];
    let slash = path.rfind('/')?;
    let (folders, file) = (&path[..=slash], &path[slash + 1..]);
    // A folder alone ("src/") or a bare slash is not a file path, and prose ("either/or") is not
    // one either: a path names a file with an extension or starts at a root.
    let rooted = ["/", "~/", "./", "../"]
        .iter()
        .any(|root| path.starts_with(root));
    if file.is_empty() || folders == "/" || !(rooted || file.contains('.')) {
        return None;
    }
    // The folders that name something: a root ("~", ".", "..") is not one.
    let names: Vec<&str> = folders
        .split('/')
        .filter(|name| !matches!(*name, "" | "~" | "." | ".."))
        .collect();
    let shortened = match names.as_slice() {
        // "src/main.rs", "~/notes.md", "~/bin/deploy": short enough as they are.
        [] | [_] => folders.to_string(),
        [.., last] => format!("…/{last}/"),
    };
    Some((&word[..start], shortened, file, &word[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(label: &str) -> Vec<(String, bool)> {
        label_parts(label)
            .into_iter()
            .map(|part| (part.text, part.dim))
            .collect()
    }

    #[test]
    fn a_long_path_keeps_its_file_name_and_dims_its_last_folder() {
        assert_eq!(
            runs("Reading ~/Documents/Screenshots/2026-10-28_Ghostex_20-09-21@2x.png"),
            [
                ("Reading ".to_string(), false),
                ("…/Screenshots/".to_string(), true),
                ("2026-10-28_Ghostex_20-09-21@2x.png".to_string(), false),
            ]
        );
        assert_eq!(
            runs("Read(/Users/me/dev/app/src/main.rs) twice"),
            [
                ("Read(".to_string(), false),
                ("…/src/".to_string(), true),
                ("main.rs) twice".to_string(), false),
            ]
        );
    }

    #[test]
    fn a_short_path_keeps_its_one_folder_dimmed() {
        assert_eq!(
            runs("Editing src/lib.rs."),
            [
                ("Editing ".to_string(), false),
                ("src/".to_string(), true),
                ("lib.rs.".to_string(), false),
            ]
        );
    }

    #[test]
    fn text_without_a_path_is_one_run() {
        assert_eq!(
            runs("Running cargo test"),
            [("Running cargo test".to_string(), false)]
        );
        assert_eq!(
            runs("Fetching https://example.com/a/b.html"),
            [("Fetching https://example.com/a/b.html".to_string(), false)]
        );
        assert_eq!(runs("Listing src/"), [("Listing src/".to_string(), false)]);
        assert_eq!(runs("either/or"), [("either/or".to_string(), false)]);
        assert_eq!(
            runs("Running ~/bin/deploy"),
            [
                ("Running ".to_string(), false),
                ("~/bin/".to_string(), true),
                ("deploy".to_string(), false),
            ]
        );
        assert!(runs("").is_empty());
    }

    /// The top row shows no time (the user's 2026-10-29 words in this file's decision).
    #[test]
    fn the_card_carries_no_time() {
        let card = serde_json::to_value(crate::document::WorkingCard::default()).unwrap();
        assert!(card.get("elapsed").is_none(), "{card}");
    }
}

//! The composer's `$skill` and `@path` triggers, and the two lists they filter.
//!
//! Port of `packages/core-ui/chat/session-chat-composer-trigger.ts`.
//!
//! Detection is caret-relative, not draft-relative: the token under the caret is the one being
//! typed, so the pickers open when a mention is written anywhere in the draft instead of only when
//! the draft ends with the token. The token boundary is whitespace only, mirroring how the agent
//! CLIs parse these mentions: a token that starts with `$` or `@` right after whitespace (or at the
//! start of the draft) is a mention, anything else ("cost$5", "user@host") is ordinary text.

use serde::{Deserialize, Serialize};

use crate::composer::references::is_js_whitespace;
use crate::composer::text::Utf16Text;

/// Which picker a trigger opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TriggerKind {
    Path,
    Skill,
}

/// The mention under the caret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerTrigger {
    pub kind: TriggerKind,
    /// Token text after the `$` or `@` sigil.
    pub query: String,
    /// Draft offset of the sigil, in UTF-16 code units.
    pub start: usize,
    /// Draft offset just past the token (the caret), in UTF-16 code units.
    pub end: usize,
}

/// One skill the `$` picker can offer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub directory_path: String,
    pub skill_file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_label: Option<String>,
    /// One of Ghostex's bundled skills (gxserver marks them).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ghostex: bool,
    /// The customer description of a Ghostex skill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// How the agent's own input runs a Ghostex skill (`/name` or `$name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation: Option<String>,
}

/// The token under the caret, or `None` when it is not a mention.
pub fn detect_composer_trigger(text: &str, caret: Option<usize>) -> Option<ComposerTrigger> {
    let indexed = Utf16Text::new(text);
    // `clampCaret` floors a fractional caret and treats a non-finite one as the end of the draft.
    let cursor = match caret {
        Some(caret) => indexed.index_of_offset(caret.min(indexed.utf16_len())),
        None => indexed.len(),
    };
    let mut index = cursor as isize - 1;
    while index >= 0 && !indexed.at(index as usize).is_some_and(is_js_whitespace) {
        index -= 1;
    }
    let start = (index + 1) as usize;
    let token = indexed.slice(start, cursor);
    let kind = if token.starts_with('$') {
        TriggerKind::Skill
    } else if token.starts_with('@') {
        TriggerKind::Path
    } else {
        return None;
    };
    Some(ComposerTrigger {
        kind,
        query: token.chars().skip(1).collect(),
        start: indexed.offset(start),
        end: indexed.offset(cursor),
    })
}

/// Skills whose name contains the query, in catalog order.
pub fn filter_skills<'a>(skills: &'a [Skill], query: &str) -> Vec<&'a Skill> {
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        return skills.iter().collect();
    }
    skills
        .iter()
        .filter(|skill| skill.name.to_lowercase().contains(&normalized))
        .collect()
}

/// `~/` for a path under the user's home, the way the `$` row shows it.
pub fn display_skill_directory_path(path: &str) -> String {
    // `/^\/Users\/[^/]+\//`.
    let Some(rest) = path.strip_prefix("/Users/") else {
        return path.to_string();
    };
    match rest.find('/') {
        Some(index) if index > 0 => format!("~/{}", &rest[index + 1..]),
        _ => path.to_string(),
    }
}

/// Secondary text of a `$` row; a same-name variation leads with its source so truncation keeps
/// rows distinguishable.
pub fn skill_detail(skill: &Skill) -> String {
    let path = display_skill_directory_path(&skill.directory_path);
    match skill.variant_label.as_deref() {
        Some(label) if !label.is_empty() => format!("{label} \u{b7} {path}"),
        _ => path,
    }
}

/// Markdown-linked skill mention: the label carries the invocation the agent reads (`$name`, or
/// `/name` for Claude Code), and the destination is the skill's `SKILL.md`, which is what a reader
/// clicking the mention wants open.
pub fn linked_skill_mention(skill: &Skill, agent: Option<&str>) -> String {
    file_reference(
        &skill.skill_file_path,
        &crate::composer::skill_invocation::skill_invocation(&skill.name, agent),
    )
}

/// The `@` picker completes to a named file link using the draft's next reference number.
pub fn file_mention(path: &str, index: u32) -> String {
    file_reference(path, &format!("{} #{index}", file_basename(path)))
}

/// CDXC:SessionChat 2026-09-06 DECISION:
/// User: Ghostex-generated file references use descriptive Markdown links instead of @filepath,
/// with a purpose and reference number or a session-title Handoff label.
pub fn file_reference(path: &str, label: &str) -> String {
    let collapsed = label.split_whitespace().collect::<Vec<_>>().join(" ");
    let escaped_label: String = collapsed
        .trim()
        .chars()
        .flat_map(|character| {
            if matches!(character, '\\' | '[' | ']') {
                vec!['\\', character]
            } else {
                vec![character]
            }
        })
        .collect();
    let destination = if path
        .chars()
        .any(|character| is_js_whitespace(character) || character == '<' || character == '>')
    {
        let inner: String = path
            .chars()
            .flat_map(|character| {
                if matches!(character, '\\' | '<' | '>') {
                    vec!['\\', character]
                } else {
                    vec![character]
                }
            })
            .collect();
        format!("<{inner}>")
    } else {
        path.chars()
            .flat_map(|character| {
                if matches!(character, '\\' | '(' | ')') {
                    vec!['\\', character]
                } else {
                    vec![character]
                }
            })
            .collect()
    };
    format!("[{escaped_label}]({destination})")
}

/// The last path segment, whichever separator the platform used.
pub fn file_basename(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// Everything before the last separator, or the empty string.
pub fn file_directory(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(index) => &path[..index],
        None => "",
    }
}

const FILE_MATCH_LIMIT: usize = 60;

/// Ranks project-relative paths for the `@` picker: basename prefix first, then basename
/// substring, then anywhere in the path. Ties break on the shortest path so top-level files
/// outrank deeply nested namesakes.
pub fn filter_files<'a>(files: &'a [String], query: &str) -> Vec<&'a str> {
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        return files
            .iter()
            .take(FILE_MATCH_LIMIT)
            .map(String::as_str)
            .collect();
    }
    let mut scored: Vec<(u8, &str)> = Vec::new();
    for path in files {
        let lowered = path.to_lowercase();
        let basename = file_basename(&lowered).to_string();
        let score = if basename.starts_with(&normalized) {
            0
        } else if basename.contains(&normalized) {
            1
        } else if lowered.contains(&normalized) {
            2
        } else {
            3
        };
        if score < 3 {
            scored.push((score, path.as_str()));
        }
    }
    // `Array.prototype.sort` is stable, and the comparator falls through to `localeCompare`, so
    // the tie-break below reproduces its order without a locale table for ASCII paths.
    scored.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| {
                crate::composer::text::utf16_len(left.1)
                    .cmp(&crate::composer::text::utf16_len(right.1))
            })
            .then_with(|| left.1.cmp(right.1))
    });
    scored
        .into_iter()
        .take(FILE_MATCH_LIMIT)
        .map(|(_, path)| path)
        .collect()
}

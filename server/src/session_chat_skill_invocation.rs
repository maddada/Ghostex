//! Claude Code runs a skill from its slash command, so a composer skill pill reaches Claude's
//! terminal as the bare `/name`.
//!
//! CDXC:AgentSkills 2026-09-27 WHY:
//! The composer keeps a skill as a `[/name](…/SKILL.md)` Markdown link, because the link is what draws the pill, and the saved draft revision a send is checked against holds that same link. So the draft is checked as written and the text is converted here, on its way to the terminal. A Claude agent handed the link, or the `$name` spelling Codex uses, answered that the skill only loads from the slash command itself. Codex and every other agent get the text unchanged.
//! SEE-ALSO: `packages/gx-chat-core/src/composer/skill_invocation.rs` spells the pill and the optimistic echo; both sides must convert exactly the same links, or the echo stops matching the turn Claude records.

use std::borrow::Cow;

/// Whether the agent invokes a skill as a slash command.
fn invokes_skills_with_slash(agent: Option<&str>) -> bool {
    matches!(
        crate::agents::identity::normalize_agent_id(agent).as_deref(),
        Some("claude" | "openclaude")
    )
}

/// How the agent's own input runs a skill: `/name` for Claude, `$name` for every other agent.
pub(crate) fn skill_invocation(agent: Option<&str>, name: &str) -> String {
    let sigil = if invokes_skills_with_slash(agent) {
        '/'
    } else {
        '$'
    };
    format!("{sigil}{name}")
}

/// The text the agent's terminal receives for a chat message: for Claude, every
/// `[$name](…/SKILL.md)` or `[/name](…/SKILL.md)` link becomes `/name`.
pub(crate) fn agent_skill_text<'a>(agent: Option<&str>, text: &'a str) -> Cow<'a, str> {
    if !invokes_skills_with_slash(agent) || !text.contains("](") {
        return Cow::Borrowed(text);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let mut changed = false;
    while index < chars.len() {
        let is_link_start = chars[index] == '[' && (index == 0 || chars[index - 1] != '!');
        if let Some((name, end)) = is_link_start
            .then(|| skill_link_at(&chars, index))
            .flatten()
        {
            out.push('/');
            out.push_str(&name);
            index = end;
            changed = true;
            continue;
        }
        out.push(chars[index]);
        index += 1;
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

/// The skill name of a `[$name](…/SKILL.md)` or `[/name](…/SKILL.md)` link opening at `start`, and
/// the index just past its closing `)`.
fn skill_link_at(chars: &[char], start: usize) -> Option<(String, usize)> {
    if !matches!(chars.get(start + 1), Some('$' | '/')) {
        return None;
    }
    let mut index = start + 2;
    let mut name = String::new();
    while let Some(&character) = chars.get(index) {
        if !(character.is_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')) {
            break;
        }
        name.push(character);
        index += 1;
    }
    if name.is_empty() || chars.get(index) != Some(&']') || chars.get(index + 1) != Some(&'(') {
        return None;
    }
    let (destination, end) = link_destination(chars, index + 2)?;
    is_skill_file(&destination).then_some((name, end))
}

/// The raw `(...)` half of a Markdown link starting after its `(`, in the angle-bracketed or the
/// nested-paren form the composer's reference scanner reads.
fn link_destination(chars: &[char], start: usize) -> Option<(String, usize)> {
    let escaped_at = |index: usize| {
        chars[index] == '\\'
            && chars
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_punctuation())
    };
    if chars.get(start) == Some(&'<') {
        let mut index = start + 1;
        while let Some(&character) = chars.get(index) {
            if character == '\n' || character == '\r' {
                return None;
            }
            if escaped_at(index) {
                index += 2;
                continue;
            }
            if character == '>' && chars.get(index + 1) == Some(&')') {
                return Some((chars[start + 1..index].iter().collect(), index + 2));
            }
            index += 1;
        }
        return None;
    }
    let mut depth = 1;
    let mut index = start;
    while let Some(&character) = chars.get(index) {
        match character {
            '\n' | '\r' => return None,
            _ if escaped_at(index) => {
                index += 2;
                continue;
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((chars[start..index].iter().collect(), index + 1));
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// A destination naming a `SKILL.md` file, whichever separator the platform used.
fn is_skill_file(destination: &str) -> bool {
    let lowered = destination.to_ascii_lowercase();
    lowered == "skill.md" || lowered.ends_with("/skill.md") || lowered.ends_with("\\skill.md")
}

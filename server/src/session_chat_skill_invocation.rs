//! Each agent runs a skill in its own spelling, so a composer skill pill reaches the agent's
//! terminal spelled its way (Claude's bare `/name`, Codex's `[$name](…/SKILL.md)` link, Pi's
//! `/skill:name`, OpenCode's `@name`).
//!
//! CDXC:AgentSkills 2026-09-27 WHY:
//! The composer keeps a skill as a `[/name](…/SKILL.md)` Markdown link, because the link is what draws the pill, and the saved draft revision a send is checked against holds that same link. So the draft is checked as written and the text is converted here, on its way to the terminal. A Claude agent handed the link, or the `$name` spelling Codex uses, answered that the skill only loads from the slash command itself. Since 2026-11-08 every agent gets its own spelling from the core's table (CDXC:AgentSkills 2026-11-08 on `SkillSpelling`).
//! SEE-ALSO: `packages/gx-chat-core/src/composer/skill_invocation.rs` spells the pill and the optimistic echo; both sides must convert exactly the same links, or the echo stops matching the turn Claude records.

use std::borrow::Cow;

/// The agent's id as the core's spelling table reads it.
fn agent_id(agent: Option<&str>) -> Option<String> {
    crate::agents::identity::normalize_agent_id(agent)
}

/// How the agent's own input runs a skill: `/name` (Claude, Copilot, Cursor, Gemini), `$name`
/// (Codex and every agent not listed), `/skill:name` (Pi) or `@name` (OpenCode). The table is the
/// core's, so the two sides cannot spell a skill differently.
pub(crate) fn skill_invocation(agent: Option<&str>, name: &str) -> String {
    ghostex_gx_chat_core::composer::skill_invocation::skill_invocation(
        name,
        agent_id(agent).as_deref(),
    )
}

/// The text the agent's terminal receives for a chat message: every `[$name](…/SKILL.md)` or
/// `[/name](…/SKILL.md)` link becomes the agent's own spelling (the core's
/// `agent_skill_reference`): the bare `/name`, `/skill:name` or `@name`, or for Codex the link
/// with a `$` label.
pub(crate) fn agent_skill_text<'a>(agent: Option<&str>, text: &'a str) -> Cow<'a, str> {
    if !text.contains("](") {
        return Cow::Borrowed(text);
    }
    let agent = agent_id(agent);
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
            let link: String = chars[index..end].iter().collect();
            let spelled = ghostex_gx_chat_core::composer::skill_invocation::agent_skill_reference(
                &link,
                &name,
                agent.as_deref(),
            );
            changed |= spelled != link;
            out.push_str(&spelled);
            index = end;
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
    is_skill_file(&name, &destination).then_some((name, end))
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

/// A destination naming a `SKILL.md` file, or a Claude Code command: an `.md` file in a
/// `commands` folder, or any `.md` file the pill names as a plugin command (`plugin:name`), since a
/// plugin's commands can sit in any folder its plugin.json names. Either separator counts.
///
/// CDXC:AgentSkills 2026-11-08 WHY:
/// A plugin command picked from `commands/git/push.md` or a manifest path such as `extras/deploy.md` reached Claude as the Markdown link instead of `/plugin:name`. The name must look like a plugin command (segments of `[A-Za-z0-9_-]`, the last not all digits), so a file link labelled `/notes.md:12` stays a link.
/// SEE-ALSO: `is_skill_file` in `packages/gx-chat-core/src/composer/reference_pills.rs` must accept exactly the same links.
fn is_skill_file(name: &str, destination: &str) -> bool {
    let lowered = destination.to_ascii_lowercase();
    lowered == "skill.md"
        || lowered.ends_with("/skill.md")
        || lowered.ends_with("\\skill.md")
        || (lowered.ends_with(".md")
            && (lowered.rsplit(['/', '\\']).nth(1) == Some("commands")
                || is_plugin_command_name(name)))
}

/// `plugin:name` or `plugin:folder:name`, as Claude Code names a plugin's commands.
fn is_plugin_command_name(name: &str) -> bool {
    let segments = name.split(':').collect::<Vec<_>>();
    segments.len() >= 2
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
        })
        && !segments
            .last()
            .is_some_and(|last| last.chars().all(|character| character.is_ascii_digit()))
}

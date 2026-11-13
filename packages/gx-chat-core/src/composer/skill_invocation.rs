//! How each agent spells a skill invocation, and the text the agent receives for a skill pill.
//!
//! CDXC:AgentSkills 2026-09-27 DECISION:
//! User: "let's invoke skills in claude code like /skill-name" and "for codex the actual way to invoke skills is just $". A Claude agent handed `[$cua-driver](…/SKILL.md)` answered that the skill only loads from the slash command itself, "without the link or `$`", so Claude receives the bare `/name`; Codex keeps the `[$name](…/SKILL.md)` link its own composer writes. Since 2026-11-08 (the decision on [`SkillSpelling`]) the composer's pill reads `/name` for every agent and the per-agent spelling is applied only to what the agent receives.
//! SEE-ALSO: `server/src/session_chat_skill_invocation.rs` types the same text into the agent's terminal; both sides must convert exactly the same links, or the optimistic echo stops matching the turn the agent records.

use std::borrow::Cow;

use crate::composer::reference_pills::{composer_references, ReferenceKind};
use crate::composer::text::Utf16Text;

/// How an agent's own input runs a skill.
///
/// CDXC:AgentSkills 2026-11-08 DECISION:
/// User (Q2012 on the reference syntax page): one Ghostex syntax, `/name`, in the chat box for every agent, and each agent receives its own spelling. Checked 2026-11-08 against each CLI: Claude Code, Copilot, Cursor and Gemini CLI run `/name`; Codex runs `$name` and rejects an unknown `/name` in its own box; Pi runs `/skill:name`; OpenCode loads a skill from `@name`. Every agent not named here keeps the `[$name](…/SKILL.md)` link it had before.
/// SEE-ALSO: `server/src/session_chat_skill_invocation.rs` converts the same links on their way to the terminal with this table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillSpelling {
    /// `/name`: Claude Code, OpenClaude, Copilot, Cursor, Gemini CLI.
    Slash,
    /// `/skill:name`: Pi.
    PiSkill,
    /// `@name`: OpenCode.
    At,
    /// The `[$name](…/SKILL.md)` link: Codex, and every agent not listed.
    Dollar,
}

/// The agent's skill spelling, for a raw or normalized agent id.
pub fn skill_spelling(agent: Option<&str>) -> SkillSpelling {
    let agent = agent.map(|agent| agent.trim().to_ascii_lowercase());
    match agent.as_deref() {
        Some(
            "claude" | "claude-code" | "openclaude" | "copilot" | "github-copilot" | "cursor"
            | "cursor-cli" | "cursor-agent" | "gemini" | "gemini-cli",
        ) => SkillSpelling::Slash,
        Some("pi") => SkillSpelling::PiSkill,
        Some("opencode") => SkillSpelling::At,
        _ => SkillSpelling::Dollar,
    }
}

/// `/name`, `/skill:name`, `@name` or `$name`: the way the agent's own input runs the skill.
pub fn skill_invocation(name: &str, agent: Option<&str>) -> String {
    match skill_spelling(agent) {
        SkillSpelling::Slash => format!("/{name}"),
        SkillSpelling::PiSkill => format!("/skill:{name}"),
        SkillSpelling::At => format!("@{name}"),
        SkillSpelling::Dollar => format!("${name}"),
    }
}

/// What one skill link in a sent message becomes for the agent: its bare spelling, or for a
/// `Dollar` agent the link itself with a `$` label. `link` is the whole `[…](…)` text and
/// `name` the skill name inside its label.
pub fn agent_skill_reference(link: &str, name: &str, agent: Option<&str>) -> String {
    match skill_spelling(agent) {
        SkillSpelling::Dollar => match link.strip_prefix("[/") {
            Some(rest) => format!("[${rest}"),
            None => link.to_string(),
        },
        _ => skill_invocation(name, agent),
    }
}

/// The bundled skill every coordinator works by: the one place its name is written for code
/// (gxserver's `COORDINATOR_SKILL_NAME` is this). Named `ghostex-orchestrator` until 2026-11-11.
pub const COORDINATOR_SKILL_NAME: &str = "ghostex-coordinator";

/// What a role's first message starts with, before the skill in the agent's spelling.
pub const SKILL_LINE_PREFIX: &str = "Read and adhere to this skill: ";

/// The line every new orchestrator's first message starts with, in the agent's own skill
/// spelling ([`skill_invocation`]): "Read and adhere to this skill: /ghostex-coordinator" for
/// Claude Code, `$ghostex-coordinator` for Codex. gxserver sends it, and the New Orchestrator
/// dialog shows it as the locked first line.
/// SEE-ALSO: server/src/coordinators/role.rs (`orchestrator_first_message`, the CDXC:Coordinators 2026-10-17 decision).
///
/// CDXC:Coordinators 2026-11-13 WHY:
/// Any agent may run a coordinator since the user's Q2980 decision, but only the agents in `COORDINATOR_AGENT_FAMILIES` (gx-protocol) load skills from a folder Ghostex installs into. Every other agent (and one Ghostex cannot name) gets [`COORDINATOR_GUIDE_LINE`], which works for any agent that can run a shell command; it keeps the same opening so a queued first message is still found by it (role.rs `coordinator_role_reminder`).
pub fn coordinator_skill_line(agent: Option<&str>) -> String {
    let known = agent.map(str::trim).is_some_and(|agent| {
        ghostex_gx_protocol::coordinator::COORDINATOR_AGENT_FAMILIES.contains(&agent)
    });
    if !known {
        return COORDINATOR_GUIDE_LINE.to_string();
    }
    format!(
        "{SKILL_LINE_PREFIX}{}",
        skill_invocation(COORDINATOR_SKILL_NAME, agent)
    )
}

/// The first line of a coordinator whose agent loads no skills Ghostex installs: the playbook
/// straight from the CLI.
pub const COORDINATOR_GUIDE_LINE: &str = "Read and adhere to this skill: run `ghostex coordinator guide` and follow what it prints for the whole session.";

/// The name in a `$name` or `/name` pill label, when it is one gxserver's scanner also accepts,
/// so the two sides convert the same links.
fn skill_name(label: &str) -> Option<&str> {
    label.strip_prefix(['$', '/']).filter(|name| {
        !name.is_empty()
            && name.chars().all(|character| {
                character.is_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
            })
    })
}

/// The text the agent receives for a composer draft: every skill pill becomes the agent's own
/// spelling ([`agent_skill_reference`]).
///
/// gxserver does the conversion on its way to the terminal; the core runs it only to know what the
/// agent will record, for the send's classification and its optimistic echo.
pub fn agent_skill_text<'a>(text: &'a str, agent: Option<&str>) -> Cow<'a, str> {
    let references = composer_references(text);
    let skills: Vec<_> = references
        .iter()
        .filter(|reference| reference.kind == ReferenceKind::Skill)
        .filter_map(|reference| Some((reference, skill_name(&reference.label)?)))
        .collect();
    if skills.is_empty() {
        return Cow::Borrowed(text);
    }
    let indexed = Utf16Text::new(text);
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (reference, name) in skills {
        let start = indexed.index_of_offset(reference.start);
        let end = indexed.index_of_offset(reference.end);
        out.push_str(&indexed.slice(cursor, start));
        out.push_str(&agent_skill_reference(
            &indexed.slice(start, end),
            name,
            agent,
        ));
        cursor = end;
    }
    out.push_str(&indexed.slice(cursor, indexed.len()));
    if out == text {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(out)
    }
}

/// The message with every skill typed by hand (`/name` or `$name`, not picked from the list)
/// turned into the pill a pick writes, or `None` when there is none.
///
/// A token counts when it starts the message or follows whitespace, names a skill or custom command
/// of the catalog exactly, and ends at whitespace, the end, or punctuation. Code spans and fences
/// stay as typed, and so does a message that opens with one of the agent's built-in commands, which
/// that agent runs ahead of a skill of the same name.
///
/// CDXC:AgentSkills 2026-11-08 DECISION:
/// User (Q2016 on the reference syntax page): a hand-typed `/name` or `$name` that matches a listed skill turns into the pill on send, so the agent receives its own spelling whichever key was used (issue #513: a typed `$name` reached Claude as plain text).
pub fn pill_typed_skills(
    text: &str,
    skills: &[crate::composer::trigger::Skill],
    agent: Option<&str>,
) -> Option<String> {
    if skills.is_empty() || !text.contains(['/', '$']) {
        return None;
    }
    let commands = crate::composer::slash_commands::slash_commands_for_agent(agent);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let mut changed = false;
    let mut in_code = false;
    let mut fence = false;
    while index < chars.len() {
        let character = chars[index];
        if character == '`' {
            let run = chars[index..].iter().take_while(|c| **c == '`').count();
            if run >= 3 {
                fence = !fence;
            } else if !fence {
                in_code = !in_code;
            }
            out.extend(&chars[index..index + run]);
            index += run;
            continue;
        }
        let starts_token = index == 0 || chars[index - 1].is_whitespace();
        if !in_code && !fence && starts_token && matches!(character, '/' | '$') {
            if let Some((skill, end)) = typed_skill_at(&chars, index, skills) {
                let leading_command = index == 0
                    && character == '/'
                    && commands.iter().any(|command| command.name == skill.name);
                if !leading_command {
                    out.push_str(&crate::composer::trigger::linked_skill_mention(skill));
                    index = end;
                    changed = true;
                    continue;
                }
            }
        }
        out.push(character);
        index += 1;
    }
    changed.then_some(out)
}

/// The catalog entry a `/name` or `$name` token at `start` names, and the index past its name.
fn typed_skill_at<'a>(
    chars: &[char],
    start: usize,
    skills: &'a [crate::composer::trigger::Skill],
) -> Option<(&'a crate::composer::trigger::Skill, usize)> {
    let mut end = start + 1;
    while chars
        .get(end)
        .is_some_and(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        end += 1;
    }
    // A sentence's own `.` or `:` after the name is not part of it.
    loop {
        if end <= start + 1 {
            return None;
        }
        let name: String = chars[start + 1..end].iter().collect();
        let boundary = chars.get(end).is_none_or(|c| {
            c.is_whitespace() || matches!(c, ',' | ';' | '!' | '?' | ')' | '.' | ':')
        });
        if boundary {
            if let Some(skill) = skills.iter().find(|skill| skill.name == name) {
                return Some((skill, end));
            }
        }
        if !matches!(chars[end - 1], '.' | ':') {
            return None;
        }
        end -= 1;
    }
}

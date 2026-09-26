//! The text rules an optimistic echo is matched with: normalization, the slash-command envelope,
//! and the two shapes that legitimately prefix a recorded turn.
//!
//! Ported from `packages/core-ui/chat/session-chat-pending.ts` and
//! `session-chat-command-envelope.ts`. The TypeScript used regular expressions; this crate carries
//! no regex dependency on purpose, so each pattern is spelled out below with the JavaScript it
//! reproduces quoted beside it. Behaviour, not shape, is what must match.

/// `/^\[Image #\d+\]\s*/`: the marker the agent records in front of an attached image prompt.
///
/// Stripped so an optimistic echo matches its transcript twin.
pub fn strip_image_prompt_marker(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("[Image #") else {
        return text;
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return text;
    }
    let Some(rest) = rest[digits..].strip_prefix(']') else {
        return text;
    };
    rest.trim_start_matches(is_js_space)
}

/// `/[\u0000-\u0008\u000b\u000c\u000e-\u001a\u001c-\u001f\u007f]/g`.
///
/// Kill-key bytes a TUI occasionally swallows into the submitted prompt as literal text: the send
/// path's Ctrl-U and Ctrl-K clear burst can coalesce into the paste frame's stdin chunk. Never
/// typeable, so both sides drop all C0 controls except tab, newline, carriage return (real text)
/// and escape (a bare strip would leave dangling ANSI fragments).
fn is_leaked_control(value: char) -> bool {
    matches!(value, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1a}' | '\u{1c}'..='\u{1f}' | '\u{7f}')
}

/// `\s` as JavaScript defines it, which is what every `trim` and `\s+` fold below means.
pub fn is_js_space(value: char) -> bool {
    value.is_whitespace() || matches!(value, '\u{feff}')
}

/// Collapses every run of whitespace to one space and trims, the `.trim().replace(/\s+/g, ' ')`
/// tail of `normalizeSessionChatPendingText`.
pub fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for value in text.chars() {
        if is_js_space(value) {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(value);
    }
    out
}

/// `/\[(\$(?:[^\]\\\n]|\\.)+)\]\((?:<(?:[^>\\]|\\.)*>|(?:[^)\s\\]|\\.)*)\)/g` replaced by `$1`.
///
/// A skill mention is typed as `[$name](path)`, but the harness owns the destination: Codex
/// resolves symlinked skill roots, so the echo and its transcript twin can disagree on the path
/// while meaning the same mention. Only the `$name` label is identity.
fn replace_skill_mention_links(text: &str) -> String {
    let bytes: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != '[' {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        match skill_mention_at(&bytes, index) {
            Some((label, end)) => {
                out.push_str(&label);
                index = end;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    out
}

/// Matches one `[$name](destination)` starting at `start`, returning the label and the index just
/// past the closing parenthesis.
fn skill_mention_at(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut index = start + 1;
    if chars.get(index) != Some(&'$') {
        return None;
    }
    let label_start = index;
    index += 1;
    let mut label_len = 1;
    loop {
        match chars.get(index) {
            Some('\\') => {
                // `\\.`: an escaped character, including a `]` that does not close the label.
                chars.get(index + 1)?;
                index += 2;
                label_len += 2;
            }
            Some(']') => break,
            Some('\n') | None => return None,
            Some(_) => {
                index += 1;
                label_len += 1;
            }
        }
    }
    if label_len < 2 {
        return None;
    }
    let label: String = chars[label_start..label_start + label_len].iter().collect();
    index += 1;
    if chars.get(index) != Some(&'(') {
        return None;
    }
    index += 1;
    if chars.get(index) == Some(&'<') {
        index += 1;
        loop {
            match chars.get(index) {
                Some('\\') => {
                    chars.get(index + 1)?;
                    index += 2;
                }
                Some('>') => {
                    index += 1;
                    break;
                }
                None => return None,
                Some(_) => index += 1,
            }
        }
    } else {
        loop {
            match chars.get(index) {
                Some('\\') => {
                    chars.get(index + 1)?;
                    index += 2;
                }
                Some(')') | None => break,
                Some(value) if is_js_space(*value) => break,
                Some(_) => index += 1,
            }
        }
    }
    if chars.get(index) != Some(&')') {
        return None;
    }
    Some((label, index + 1))
}

/// `/^Skill: (.+)$/` per line.
///
/// Daemons that predate the chip drop decode Codex's skill content chip into a "Skill: name" text
/// block the composer never typed. Drop such a line when the text already carries its `$name`
/// mention, so the echo still matches across that version skew.
fn strip_skill_chip_lines(text: &str) -> String {
    if !text.contains("Skill: ") {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let kept: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            let Some(name) = line
                .trim_matches(is_js_space)
                .strip_prefix("Skill: ")
                .filter(|name| !name.is_empty())
            else {
                return true;
            };
            let rest = lines
                .iter()
                .enumerate()
                .filter(|(other, _)| other != index)
                .map(|(_, line)| *line)
                .collect::<Vec<_>>()
                .join("\n");
            !rest.contains(&format!("${name}"))
        })
        .map(|(_, line)| *line)
        .collect();
    if kept.len() == lines.len() {
        text.to_string()
    } else {
        kept.join("\n")
    }
}

/// `normalizeSessionChatPendingText`: what two texts must agree on for an echo to be the same
/// prompt as a recorded turn.
pub fn normalize_pending_text(text: &str) -> String {
    let stripped: String = text
        .chars()
        .filter(|value| !is_leaked_control(*value))
        .collect();
    let linked = replace_skill_mention_links(&stripped);
    let unmarked = strip_image_prompt_marker(&linked).to_string();
    collapse_whitespace(&strip_skill_chip_lines(&unmarked))
}

/// `/^(?:(?:~|\.{1,2})?\/(?:[^\s\\]|\\.)*\s?)+$/`.
///
/// A file dragged onto the terminal pane types its shell-escaped path into the agent's input line.
/// A composer send that follows coalesces with that staged text into ONE recorded turn, while the
/// echo only carries the composer text. One or more such path tokens is the recognizable shape.
pub fn is_staged_path_input(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    let mut tokens = 0;
    while index < chars.len() {
        if chars[index] == '~' {
            index += 1;
        } else if chars[index] == '.' {
            index += 1;
            if chars.get(index) == Some(&'.') {
                index += 1;
            }
        }
        if chars.get(index) != Some(&'/') {
            return false;
        }
        index += 1;
        loop {
            match chars.get(index) {
                Some('\\') => {
                    if chars.get(index + 1).is_none() {
                        return false;
                    }
                    index += 2;
                }
                Some(value) if is_js_space(*value) => break,
                None => break,
                Some(_) => index += 1,
            }
        }
        if chars.get(index).is_some_and(|value| is_js_space(*value)) {
            index += 1;
        }
        tokens += 1;
    }
    tokens > 0
}

/// One `<command-name>`/`<command-args>` envelope, the shape a Claude-family harness records a
/// slash input's user turn as.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandEnvelope {
    pub name: String,
    pub args: String,
}

/// The attribute gxserver marks a replayed envelope with: same tags, contents escaped.
const ESCAPED_MARKUP_ATTRIBUTE: &str = "data-ghostex-escaped";

/// `parseSessionChatCommandEnvelope`.
pub fn parse_command_envelope(text: &str) -> Option<CommandEnvelope> {
    let trimmed = text.trim_start_matches(is_js_space);
    if !trimmed.to_lowercase().starts_with("<command-") {
        // Ordinary prompts, XML pastes.
        return None;
    }
    let (name_attributes, name) = tagged_section(trimmed, "command-name")?;
    let name = name.trim_matches(is_js_space);
    if name.is_empty() {
        return None;
    }
    let (args_attributes, args) = tagged_section(trimmed, "command-args")
        .map(|(attributes, value)| (attributes, value.trim_matches(is_js_space).to_string()))
        .unwrap_or_default();
    Some(CommandEnvelope {
        name: decode_markup(name, &name_attributes),
        args: decode_markup(&args, &args_attributes),
    })
}

/// `/<tag(\s[^>]*)?>([\s\S]*?)<\/tag>/`: the attribute list and the first, shortest body.
fn tagged_section(text: &str, tag: &str) -> Option<(String, String)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let at = text.find(&open)?;
    let after_name = at + open.len();
    let rest = &text[after_name..];
    let attributes_end = rest.find('>')?;
    let attributes = &rest[..attributes_end];
    if !attributes.is_empty() && !attributes.starts_with(is_js_space) {
        return None;
    }
    let body_start = after_name + attributes_end + 1;
    let body_end = text[body_start..].find(&close)? + body_start;
    Some((
        attributes.to_string(),
        text[body_start..body_end].to_string(),
    ))
}

/// `decodeSessionChatEscapedMarkup`, applied only when the tag says the body was escaped.
fn decode_markup(value: &str, attributes: &str) -> String {
    if !attributes.contains(ESCAPED_MARKUP_ATTRIBUTE) {
        return value.to_string();
    }
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// The first whitespace-delimited token of a command, lowercased. `commandMarkerName`.
pub fn command_marker_name(command: &str) -> String {
    command
        .trim_matches(is_js_space)
        .to_lowercase()
        .split(is_js_space)
        .next()
        .unwrap_or_default()
        .to_string()
}

/// `isSessionChatClearCommand`.
pub fn is_clear_command(command: &str) -> bool {
    command_marker_name(command) == "/clear"
}

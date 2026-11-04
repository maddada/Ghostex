//! Marked sections: the parts of an agent's reply that start with a Marked handovers marker
//! (❓ Needs you, ✅ Done, ⚠️ Read this, 🛫 Kicked off, 🔄 In progress, 🧪 Untested, 🚀 Shipped, 🛑 Blocked,
//! 📣 Comms, ⏸️ Parked, 📦 Held for release, 💡 Suggestions),
//! tagged with their kind so every chat draws them in that kind's colour.
//!
//! The pass runs over the native Markdown (`native_markdown.rs`) and wraps each section in marks,
//! the way an alert or a table is wrapped: `\u{e000}section:{…}` before a section's lines and
//! `\u{e000}/section` after them. The JSON names the kind, the emoji, the kind's own name, the bold
//! or `Label:` text the line started with (`label`, the kind's name when the line has text but no
//! label of its own, and `labelMarkdown`, the same text safe to draw as one Markdown paragraph: the
//! label of a marked heading is the whole heading, links and code spans included, and wraps),
//! whether that label only repeats the kind's name (`labelRepeats`) and the first line without its
//! emoji (`plain`, for the chat's emoji switch). The lines in between are the section as the agent
//! wrote it, except that its first line ends with the label: whatever followed the label on that
//! line becomes the section's first paragraph, under it. So a renderer that keeps the emoji draws
//! the lines as they are and one that replaces it draws its own first line from the JSON and the
//! lines after the first, and no style ever puts text beside the label.
//!
//! CDXC:MarkedHandovers 2026-10-11 DECISION:
//! User: the chat colours the sections an agent marks, in one of three styles picked in Settings > Chat under Marked handovers ("Marked sections style": a thin left rule, a coloured badge in place of the emoji, or coloured labels, the default). A bold label line and the bullets under it are one section. Colours are the mockup's palette (docs/2026-10-11/chat-section-markers/index.html), In progress #00A6ED; ❓ Needs you is orange #fb923c (#c2410c on a light chat; 2026-10-12, user: "these both are same color, please change Needs you color", when it was blue #60a5fa next to In progress); 🛫 Kicked off (user: "let's call it 'Kicked off'", work started this turn) is pink. It applies to every agent's replies that carry markers (orchestrators and their threads always mark their handovers; Marked handovers extends that to every session); copying a message still gives the agent's own text (`copyText` is the unmarked body). The label stands alone on its line (2026-10-13, user: "we agreed that we shouldn't show the text to the right of the heading in the responses when using colorful handover"): text an agent writes after the label on the same line is drawn under it as the section's first paragraph, in every style, whatever the agent wrote. Supersedes the same day's summary strip of chips at the top of the message counting what needs the reader (user: "let's actually just remove the summary at the top i don't think it's useful at all").
//! SEE-ALSO: apps/desktop/src/app/native_chat/marked_sections.rs (GPUI drawing and the palette), apps/mobile/app/src/chat/native/transcript/markdown/MarkedSections.tsx (phone drawing), server/src/agents/marked_handovers.rs (the note that asks agents for these markers; the emoji and names must match it).

use serde_json::json;

use crate::transcript::jsstr::{js_trim, js_trim_end, js_trim_start, split_lf};
use crate::transcript::native_markdown::{fence_close, fence_open, NATIVE_MARK};

pub const NATIVE_SECTION_OPEN: &str = "\u{e000}section:";
pub const NATIVE_SECTION_CLOSE: &str = "\u{e000}/section";

/// One marker: the kind id the renderers colour by, the emoji agents write, and the kind's name.
struct Marker {
    kind: &'static str,
    emoji: &'static str,
    name: &'static str,
}

const MARKERS: [Marker; 12] = [
    Marker {
        kind: "need",
        emoji: "\u{2753}",
        name: "Needs you",
    },
    Marker {
        kind: "read",
        emoji: "\u{26a0}\u{fe0f}",
        name: "Read this",
    },
    Marker {
        kind: "blocked",
        emoji: "\u{1f6d1}",
        name: "Blocked",
    },
    Marker {
        kind: "untested",
        emoji: "\u{1f9ea}",
        name: "Untested",
    },
    Marker {
        kind: "done",
        emoji: "\u{2705}",
        name: "Done",
    },
    Marker {
        kind: "kicked",
        emoji: "\u{1f6eb}",
        name: "Kicked off",
    },
    Marker {
        kind: "progress",
        emoji: "\u{1f504}",
        name: "In progress",
    },
    Marker {
        kind: "shipped",
        emoji: "\u{1f680}",
        name: "Shipped",
    },    Marker {
        kind: "comms",
        emoji: "\u{1f4e3}",
        name: "Comms",
    },
    Marker {
        kind: "parked",
        emoji: "\u{23f8}\u{fe0f}",
        name: "Parked",
    },
    Marker {
        kind: "held",
        emoji: "\u{1f4e6}",
        name: "Held for release",
    },
    Marker {
        kind: "suggestions",
        emoji: "\u{1f4a1}",
        name: "Suggestions",
    },
];

/// The marker `text` starts with, and what follows it (a variation selector after the emoji is
/// optional: agents write ⚠ and ⚠️ alike).
fn leading_marker(text: &str) -> Option<(&'static Marker, &str)> {
    MARKERS.iter().find_map(|marker| {
        let bare = marker.emoji.trim_end_matches('\u{fe0f}');
        let rest = text.strip_prefix(bare)?;
        Some((marker, rest.strip_prefix('\u{fe0f}').unwrap_or(rest)))
    })
}

fn has_any_marker(markdown: &str) -> bool {
    MARKERS
        .iter()
        .any(|marker| markdown.contains(marker.emoji.trim_end_matches('\u{fe0f}')))
}

/// The first line of a section, taken apart.
struct Opening<'a> {
    marker: &'static Marker,
    label: String,
    /// The first line up to the end of its label: the line the section opens with.
    head: String,
    /// The rest of the first line, which the section draws as its first paragraph under the label.
    lead: Option<&'a str>,
    /// The head with the marker emoji taken out (`**✅ Done:** x` → `**Done:**`).
    plain: String,
}

/// `^ {0,3}#{1,6}[ \t]+`, the heading's text when the line is one.
fn heading_text(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches(' ');
    if line.len() - rest.len() > 3 {
        return None;
    }
    let hashes = rest.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let text = &rest[hashes..];
    (text.starts_with([' ', '\t'])).then(|| js_trim_start(text))
}

/// A plain `Label:` at the start of `text`: a few words with no Markdown in them before a colon.
fn plain_label(text: &str) -> Option<(&str, &str)> {
    let colon = text.find(':')?;
    let label = js_trim(&text[..colon]);
    let words = label.split_whitespace().count();
    let plain = !label.contains(['*', '`', '[', ']', '_', '(', ')', '<', '>', '!', '?', '.']);
    (plain && (1..=4).contains(&words) && label.chars().count() <= 24)
        .then(|| (label, &text[colon + 1..]))
}

/// `label` as a Markdown paragraph that reads as the label: a leading `2.`, `-`, `#` or `>` would
/// open a list, a heading or a quote on its own, so its first punctuation is escaped.
fn label_markdown(label: &str) -> String {
    let digits = label.bytes().take_while(u8::is_ascii_digit).count();
    let after = &label[digits..];
    let ordered = (1..=9).contains(&digits)
        && after.starts_with(['.', ')'])
        && after[1..].starts_with([' ', '\t']);
    if ordered {
        return format!("{}\\{}", &label[..digits], after);
    }
    let hashes = label.bytes().take_while(|byte| *byte == b'#').count();
    let heading = (1..=6).contains(&hashes) && label[hashes..].starts_with([' ', '\t']);
    let bullet = label.starts_with(['-', '+', '*']) && label[1..].starts_with([' ', '\t']);
    if heading || bullet || label.starts_with('>') {
        return format!("\\{label}");
    }
    label.to_string()
}

/// One trailing hard break (an odd run of backslashes), which a line loses when it ends a block.
fn without_hard_break(line: &str) -> &str {
    let trimmed = line.trim_end_matches('\\');
    if (line.len() - trimmed.len()) % 2 == 1 {
        &line[..line.len() - 1]
    } else {
        line
    }
}

/// The line, read as the first line of a marked section.
///
/// A section starts on a top-level line or heading that begins with a marker, either bare
/// (`✅ **Done:** …`, `✅ Done: …`, `✅ fixed it`) or inside the bold label (`**✅ Done:** …`). A
/// list item that starts with a marker stays part of its list.
fn opening(line: &str) -> Option<Opening<'_>> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 {
        return None;
    }
    let heading = heading_text(line);
    let text = heading.unwrap_or(&line[indent..]);
    let bold_outside = text.strip_prefix("**").and_then(leading_marker);
    let (marker, after, in_bold) = match bold_outside {
        Some((marker, after)) => (marker, after, true),
        None => {
            let (marker, after) = leading_marker(text)?;
            (marker, after, false)
        }
    };
    let after = js_trim_start(after);
    let (label, lead) = if in_bold {
        match after.find("**") {
            Some(close) => (js_trim(&after[..close]).to_string(), &after[close + 2..]),
            None => (String::new(), after),
        }
    } else if let Some(bold) = after.strip_prefix("**") {
        match bold.find("**") {
            Some(close) if close > 0 => (js_trim(&bold[..close]).to_string(), &bold[close + 2..]),
            _ => (String::new(), after),
        }
    } else if let Some((label, lead)) = plain_label(after) {
        (format!("{label}:"), lead)
    } else if heading.is_some() {
        (js_trim(after).to_string(), "")
    } else {
        (String::new(), after)
    };
    // `**Done**: …` keeps its colon with the label.
    let (label, lead) = match js_trim_start(lead).strip_prefix(':') {
        Some(rest) if !label.is_empty() && !label.ends_with(':') => (format!("{label}:"), rest),
        _ => (label, lead),
    };
    // A title line that only names its kind (`🧪 Untested`, `🔄 in progress:`) is the label itself,
    // so no renderer draws the name twice ("Untested: Untested").
    let (label, lead) = match title_only(lead, marker) {
        Some(title) if label.is_empty() => (title, ""),
        Some(_) => (label, ""),
        None if label.is_empty() => match leading_name(lead, marker) {
            // `🧪 Untested on Windows`: the name is the label and the rest stays text.
            Some((name, rest)) => (name.to_string(), rest),
            None => (label, lead),
        },
        None => (label, lead),
    };
    // The rest of the line goes under the label, never beside it: it is a suffix of `line` (or
    // empty), so the head is the line before it.
    let rest = js_trim_start(lead);
    let has_lead =
        !js_trim(without_hard_break(js_trim_end(rest))).is_empty() && line.ends_with(rest);
    let (label, head, lead) = if has_lead {
        let head = js_trim_end(&line[..line.len() - rest.len()]);
        if label.is_empty() {
            // `✅ fixed it`: the kind's name stands in as the label above the text.
            let name = format!("{}:", marker.name);
            (name.clone(), format!("{head} {name}"), Some(rest))
        } else {
            (label, head.to_string(), Some(rest))
        }
    } else {
        (
            label,
            without_hard_break(js_trim_end(line)).to_string(),
            None,
        )
    };
    Some(Opening {
        marker,
        label,
        plain: without_marker(&head, marker),
        head,
        lead,
    })
}

/// The rest of a title line, as its label, when it is only the marker's own name: any case, bold
/// or not, with or without a trailing colon.
fn title_only(lead: &str, marker: &Marker) -> Option<String> {
    let text = js_trim(without_hard_break(js_trim_end(lead)));
    let unbolded = text
        .strip_prefix("**")
        .and_then(|inner| inner.strip_suffix("**"))
        .unwrap_or(text);
    let unbolded = js_trim(unbolded);
    let name = js_trim(unbolded.trim_end_matches(':'));
    (!name.is_empty() && name.eq_ignore_ascii_case(marker.name)).then(|| unbolded.to_string())
}

/// The marker's own name at the start of `lead` (any case) when a space follows it, and the text
/// after it.
fn leading_name<'a>(lead: &'a str, marker: &Marker) -> Option<(&'a str, &'a str)> {
    let text = js_trim_start(lead);
    let head = text.get(..marker.name.len())?;
    let rest = &text[marker.name.len()..];
    (head.eq_ignore_ascii_case(marker.name) && rest.starts_with([' ', '\t']))
        .then(|| (head, js_trim_start(rest)))
}

/// `line` with its marker emoji (and the space after it) taken out, as the line reads without it.
fn without_marker(line: &str, marker: &Marker) -> String {
    let bare = marker.emoji.trim_end_matches('\u{fe0f}');
    let Some(at) = line.find(bare) else {
        return line.to_string();
    };
    let rest = &line[at + bare.len()..];
    let rest = rest.strip_prefix('\u{fe0f}').unwrap_or(rest);
    let plain = format!("{}{}", &line[..at], js_trim_start(rest));
    without_hard_break(js_trim_end(&plain)).to_string()
}

/// `^( *)([-*+]|\d{1,9}[.)])( +|$)`: the indent of a list item line.
fn list_item_indent(line: &str) -> Option<usize> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    let rest = &line[indent..];
    let marker_len = if rest.starts_with(['-', '*', '+']) {
        1
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=9).contains(&digits) || !rest[digits..].starts_with(['.', ')']) {
            return None;
        }
        digits + 1
    };
    let after = &rest[marker_len..];
    (after.is_empty() || after.starts_with(' ')).then_some(indent)
}

/// A line that opens a block the core marked (an alert, a table, a drawn fence), with the line
/// that closes it.
fn mark_region(line: &str) -> Option<String> {
    let name = line.strip_prefix(NATIVE_MARK)?;
    if name.starts_with('/') || name.starts_with("image:") {
        return None;
    }
    let name = name.split(':').next().unwrap_or(name);
    Some(format!("{NATIVE_MARK}/{name}"))
}

/// The index past the block opened at `index` (a marked region or a fence), when it opens one.
fn skip_block(lines: &[&str], index: usize) -> Option<usize> {
    let line = lines[index];
    if let Some(close) = mark_region(line) {
        let open = &close[NATIVE_MARK.len_utf8() + 1..];
        let mut depth = 1usize;
        let mut end = index + 1;
        while end < lines.len() {
            let candidate = lines[end];
            if candidate == close {
                depth -= 1;
                if depth == 0 {
                    return Some(end + 1);
                }
            } else if candidate
                .strip_prefix(NATIVE_MARK)
                .is_some_and(|name| name.split(':').next() == Some(open))
            {
                depth += 1;
            }
            end += 1;
        }
        return Some(lines.len());
    }
    let (character, run, _) = fence_open(line)?;
    let close = lines[index + 1..]
        .iter()
        .position(|line| fence_close(line).is_some_and(|(c, r)| c == character && r >= run))
        .map_or(lines.len(), |offset| index + 2 + offset);
    Some(close)
}

/// Where the section that starts at `start` ends (exclusive): its paragraph, and the list or
/// indented lines that follow it, across blank lines.
fn section_end(lines: &[&str], start: usize) -> usize {
    let mut index = start + 1;
    while index < lines.len() {
        let line = lines[index];
        if js_trim(line).is_empty() {
            let next = lines[index..]
                .iter()
                .position(|line| !js_trim(line).is_empty())
                .map(|offset| index + offset);
            let Some(next) = next else {
                return index;
            };
            let continues =
                list_item_indent(lines[next]).is_some() || lines[next].starts_with("  ");
            if !continues || opening(lines[next]).is_some() {
                return index;
            }
            index = next;
            continue;
        }
        if opening(line).is_some() || heading_text(line).is_some() || mark_region(line).is_some() {
            return index;
        }
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        if fence_open(line).is_some() {
            if indent < 2 {
                return index;
            }
            index = skip_block(lines, index).unwrap_or(index + 1);
            continue;
        }
        index += 1;
    }
    lines.len()
}

/// The reply with its marked sections wrapped. A reply without a marker is returned as it is.
pub fn mark_sections(markdown: String) -> String {
    if !has_any_marker(&markdown) {
        return markdown;
    }
    let lines = split_lf(&markdown);
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 8);
    let mut marked = false;
    let mut index = 0;
    while index < lines.len() {
        if let Some(end) = skip_block(&lines, index) {
            out.extend(lines[index..end].iter().map(|line| (*line).to_string()));
            index = end;
            continue;
        }
        let Some(opening) = opening(lines[index]) else {
            out.push(lines[index].to_string());
            index += 1;
            continue;
        };
        let end = section_end(&lines, index);
        let body = &lines[index..end];
        // The line before ends its paragraph here, so its hard break would show as a backslash.
        if let Some(last) = out.last_mut() {
            *last = without_hard_break(last).to_string();
        }
        let label_name = js_trim(opening.label.trim_end_matches(':'));
        out.push(format!(
            "{NATIVE_SECTION_OPEN}{}",
            json!({
                "kind": opening.marker.kind,
                "emoji": opening.marker.emoji,
                "name": opening.marker.name,
                "label": opening.label,
                "labelMarkdown": label_markdown(&opening.label),
                "labelRepeats": label_name.eq_ignore_ascii_case(opening.marker.name),
                "plain": opening.plain,
            })
        ));
        let mut body: Vec<String> = body.iter().map(|line| (*line).to_string()).collect();
        body[0] = opening.head;
        if let Some(lead) = opening.lead {
            body.splice(1..1, [String::new(), label_markdown(lead)]);
        }
        while body.len() > 1 && js_trim(body.last().map(String::as_str).unwrap_or("")).is_empty() {
            body.pop();
        }
        if let Some(last) = body.last_mut() {
            *last = without_hard_break(last).to_string();
        }
        out.extend(body);
        out.push(NATIVE_SECTION_CLOSE.to_string());
        marked = true;
        index = end;
    }
    if !marked {
        return markdown;
    }
    out.join("\n")
}

/// The Handover heading color strength (percent) when nothing is saved; it matches the catalog default of `markedHandoversHeadingStrength`.
pub const DEFAULT_HEADING_STRENGTH: u8 = 60;

/// The Handover heading color strength the renderers draw: the setting clamped to 0 to 100 and
/// snapped to its 10% steps, `None` at the default, 60%.
///
/// CDXC:MarkedHandovers 2026-10-20 DECISION:
/// User: "i want an advanced setting to be able to set how strong the colors of these headings in the handover are" and "make current colors the max". Handover heading color strength (`markedHandoversHeadingStrength`, Settings > Chat, advanced) runs 0% to 100% in 10% steps: 100% draws exactly the palette below, lower blends each kind's colour (the label, the badge and the rule) toward the reply's normal text colour, and 0% leaves the label plain bold text. Nothing draws more vivid than the palette. The value rides the chat settings and the chat read, so the desktop, the web build and the phone draw the same strength.
/// CDXC:MarkedHandovers 2026-10-22 DECISION:
/// User: "make handover color strength 60 by default". The default is 60%, not full colour (it supersedes the 100% default above), so `None` now means 60% and 100% is `Some(100)`. Saved values are unchanged.
/// SEE-ALSO: apps/desktop/src/app/native_chat/marked_look.rs (`blend_strength`), apps/mobile/app/src/chat/native/transcript/markdown/MarkedSections.tsx (`blendStrength`), server/src/session_chat_read.rs (`markedHeadingStrength` on every read).
pub fn heading_strength(percent: f64) -> Option<u8> {
    if !percent.is_finite() {
        return None;
    }
    let snapped = ((percent.clamp(0.0, 100.0) / 10.0).round() * 10.0) as u8;
    (snapped != DEFAULT_HEADING_STRENGTH).then_some(snapped)
}

/// The reply's text without the section marks, for search: what a reader sees.
pub fn without_section_marks(text: &str) -> String {
    if !text.contains(NATIVE_SECTION_OPEN) {
        return text.to_string();
    }
    split_lf(text)
        .into_iter()
        .filter(|line| !line.starts_with(NATIVE_SECTION_OPEN) && *line != NATIVE_SECTION_CLOSE)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newer_markers_open_their_own_sections() {
        let marked = mark_sections(
            "📣 **Comms:**\n- Replied on GitHub.\n\n⏸ **Parked:**\n- Docs thread.\n\n📦 **Held for release:**\n- Help text.\n\n💡 **Suggestions:**\n- Add a flag."
                .to_string(),
        );
        for kind in ["comms", "parked", "held", "suggestions"] {
            assert!(
                marked.contains(&format!("\"kind\":\"{kind}\"")),
                "{kind} missing in {marked}"
            );
        }
        assert_eq!(marked.matches(NATIVE_SECTION_CLOSE).count(), 4);
    }
}

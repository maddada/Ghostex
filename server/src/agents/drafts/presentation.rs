/// Sidebar rows are one line; anything past this is noise the tooltip already
/// carries. Measured in CHARACTERS, not bytes, so a prompt written in any
/// script clamps to the same visual length.
const DRAFT_DISPLAY_TITLE_MAX_CHARS: usize = 64;

/*
The sidebar row's title while a draft waits: the first non-blank line of the
text the user has typed, so a draft reads as what it is about instead of as
"Claude Session". Projection-level only — the durable `title` stays the agent
default, which is what a promoted draft goes back to showing.
*/
pub(crate) fn draft_display_title(content: &str) -> Option<String> {
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    if normalized.chars().count() <= DRAFT_DISPLAY_TITLE_MAX_CHARS {
        return Some(normalized);
    }
    let mut clamped = normalized
        .chars()
        .take(DRAFT_DISPLAY_TITLE_MAX_CHARS - 1)
        .collect::<String>()
        .trim_end()
        .to_string();
    clamped.push('\u{2026}');
    Some(clamped)
}

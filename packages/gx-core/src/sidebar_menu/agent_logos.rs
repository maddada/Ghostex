//! The coloured agent logos a sidebar row, a launcher item and a header button draw.
//!
//! CDXC:Icons 2026-09-20 WHY:
//! The artwork and the brand colours are `packages/core-ui/assets/*.svg` and `AGENT_LOGO_COLORS`
//! in `packages/core-ui/agent-logos.ts` (deleted 2026-10-01). The same SVG files are embedded here rather than copied,
//! so there is one set of assets with two readers, and the transform below is
//! `svgTextToColorizedDataUrl` step for step: a data URL that differs from the TypeScript's would
//! decode to different artwork on the row. `examples/sidebar_menu_parity.rs` diffs all
//! twenty-four against the TypeScript output byte for byte.

use crate::keys::encode_uri_component;

/// `(icon key, brand colour, raw SVG)` in `COLORED_AGENT_LOGOS` order. A `static`, so the artwork
/// is in the binary once: the desktop's `agent-icons/<key>.svg` assets read it through
/// [`agent_logo_svg`] instead of embedding the same files again.
static LOGOS: &[(&str, &str, &str)] = &[
    (
        "amp-cli",
        "#ffffff",
        include_str!("../../../core-ui/assets/amp-cli.svg"),
    ),
    (
        "antigravity-cli",
        "#749bff",
        include_str!("../../../core-ui/assets/antigravity-cli.svg"),
    ),
    (
        "browser",
        "#82b7ff",
        include_str!("../../../core-ui/assets/browser.svg"),
    ),
    (
        "claude",
        "#d97757",
        include_str!("../../../core-ui/assets/claude.svg"),
    ),
    // CDXC:ClaudeCloud 2026-10-29 DECISION:
    // User: "please change the icon for claude cloud to an orange cloud icon instead (same color as claude #D67556". The Claude (cloud) agent draws a filled cloud in #d67556 wherever its logo shows (picker, Select Agent menu, sidebar rows, Quick Access, Settings); the plain Claude agent keeps its logo.
    (
        "claude-cloud",
        "#d67556",
        include_str!("../../../core-ui/assets/claude-cloud.svg"),
    ),
    (
        "codebuddy",
        "#72d6ff",
        include_str!("../../../core-ui/assets/codebuddy.svg"),
    ),
    (
        "command-code",
        "#22d3ee",
        include_str!("../../../core-ui/assets/command-code.svg"),
    ),
    (
        "cursor-cli",
        "#edecec",
        include_str!("../../../core-ui/assets/cursor-cli.svg"),
    ),
    (
        "codex",
        "#ffffff",
        include_str!("../../../core-ui/assets/codex.svg"),
    ),
    (
        "copilot",
        "#ffffff",
        include_str!("../../../core-ui/assets/copilot.svg"),
    ),
    (
        "devin",
        "#3ea6ff",
        include_str!("../../../core-ui/assets/devin.svg"),
    ),
    (
        "empryo",
        "#1fa31d",
        include_str!("../../../core-ui/assets/empryo.svg"),
    ),
    (
        "factory-droid",
        "#ff7a1a",
        include_str!("../../../core-ui/assets/factory-droid.svg"),
    ),
    (
        "gemini",
        "#8b9aff",
        include_str!("../../../core-ui/assets/gemini.svg"),
    ),
    (
        "grok-build",
        "#ffffff",
        include_str!("../../../core-ui/assets/grok-build.svg"),
    ),
    (
        "hermes-agent",
        "#f3c46b",
        include_str!("../../../core-ui/assets/hermes-agent.svg"),
    ),
    (
        "mastra",
        "#ffffff",
        include_str!("../../../core-ui/assets/mastra.svg"),
    ),
    (
        "kimi",
        "#7b6cf6",
        include_str!("../../../core-ui/assets/kimi.svg"),
    ),
    (
        "kiro",
        "#a6e3ff",
        include_str!("../../../core-ui/assets/kiro.svg"),
    ),
    (
        "omp",
        "#a663ed",
        include_str!("../../../core-ui/assets/omp.svg"),
    ),
    (
        "openclaude",
        "#f0a68a",
        include_str!("../../../core-ui/assets/openclaude.svg"),
    ),
    (
        "opencode",
        "#6d96c0",
        include_str!("../../../core-ui/assets/opencode.svg"),
    ),
    (
        "zcode",
        "#ffffff",
        include_str!("../../../core-ui/assets/zcode.svg"),
    ),
    (
        "freebuff",
        "#ffffff",
        include_str!("../../../core-ui/assets/freebuff.svg"),
    ),
    (
        "dsh",
        "#4d6bfe",
        include_str!("../../../core-ui/assets/dsh.svg"),
    ),
    (
        "pi",
        "#c8ff62",
        include_str!("../../../core-ui/assets/pi.svg"),
    ),
    (
        "qoder",
        "#a991ff",
        include_str!("../../../core-ui/assets/qoder.svg"),
    ),
    (
        "rovo-dev",
        "#4fc3a1",
        include_str!("../../../core-ui/assets/rovo-dev.svg"),
    ),
];

/// Every icon key that has artwork, in catalog order. For the parity harness.
pub fn agent_logo_icons() -> Vec<&'static str> {
    LOGOS.iter().map(|(icon, _, _)| *icon).collect()
}

/// The raw artwork of an icon key (`packages/core-ui/assets/<key>.svg`), or `None` for an icon
/// key with no artwork.
pub fn agent_logo_svg(icon: &str) -> Option<&'static str> {
    LOGOS
        .iter()
        .find(|(key, _, _)| *key == icon)
        .map(|(_, _, svg)| *svg)
}

/// `COLORED_AGENT_LOGOS[icon]`: the brand-coloured artwork as a data URL, or `None` for an icon
/// key with no artwork.
///
/// The URLs are built once. A host draws them for every row of every project on every
/// install, and percent-encoding a kilobyte of SVG that many times is work with one answer.
pub fn colored_agent_logo(icon: &str) -> Option<&'static str> {
    static BUILT: std::sync::OnceLock<Vec<(&'static str, String)>> = std::sync::OnceLock::new();
    BUILT
        .get_or_init(|| {
            LOGOS
                .iter()
                .map(|(key, color, svg)| (*key, colorized_data_url(svg, color)))
                .collect()
        })
        .iter()
        .find(|(key, _)| *key == icon)
        .map(|(_, url)| url.as_str())
}

/// `svgTextToColorizedDataUrl`.
fn colorized_data_url(svg_text: &str, color: &str) -> String {
    let with_root = with_root_color(svg_text, color);
    let colorized = replace_black_fills(&with_root.replace("currentColor", color), color);
    format!("data:image/svg+xml,{}", encode_uri_component(&colorized))
}

/// `replace(/<svg\b([^>]*)>/i, ...)`: give the root element the brand colour unless it already
/// carries one. Only the first match is replaced, as `String.replace` with a non-global regex does.
fn with_root_color(svg_text: &str, color: &str) -> String {
    let Some(start) = find_root_svg_tag(svg_text) else {
        return svg_text.to_string();
    };
    let after_name = start + "<svg".len();
    let Some(offset) = svg_text[after_name..].find('>') else {
        return svg_text.to_string();
    };
    let end = after_name + offset;
    let attributes = &svg_text[after_name..end];
    let mut replacement = format!("<svg{attributes}");
    if !has_attribute(attributes, "color") {
        replacement.push_str(&format!(" color=\"{color}\""));
    }
    if !has_attribute(attributes, "fill") {
        replacement.push_str(&format!(" fill=\"{color}\""));
    }
    replacement.push('>');
    format!(
        "{}{}{}",
        &svg_text[..start],
        replacement,
        &svg_text[end + 1..]
    )
}

/// The first `<svg` whose name ends at a non-word character, case-insensitively.
fn find_root_svg_tag(svg_text: &str) -> Option<usize> {
    let lowered = svg_text.to_lowercase();
    let mut from = 0usize;
    while let Some(offset) = lowered[from..].find("<svg") {
        let start = from + offset;
        let next = lowered[start + "<svg".len()..].chars().next();
        // `\b` after `svg`: the next character must not be a word character.
        if next.is_none_or(|character| !character.is_alphanumeric() && character != '_') {
            return Some(start);
        }
        from = start + "<svg".len();
    }
    None
}

/// `/\s<name>=/i` over the root tag's attributes.
fn has_attribute(attributes: &str, name: &str) -> bool {
    let lowered = attributes.to_lowercase();
    let needle = format!("{name}=");
    let mut from = 0usize;
    while let Some(offset) = lowered[from..].find(&needle) {
        let at = from + offset;
        if at > 0
            && lowered[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
        {
            return true;
        }
        from = at + needle.len();
    }
    false
}

/// The four black-fill rewrites: `fill="#000"`, `fill:#000`, and the two `rgb()` spellings of the
/// same colour, in the order the TypeScript applies them. None of the twenty-four logos carries
/// one today, which the parity harness confirms by diffing the whole data URL; this exists so a
/// logo that arrives with one keeps rendering the same on both sides.
fn replace_black_fills(svg_text: &str, color: &str) -> String {
    let mut output = String::with_capacity(svg_text.len());
    let bytes = svg_text.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        if !svg_text.is_char_boundary(at) || !starts_with_ignore_case(&svg_text[at..], "fill") {
            let width = next_char_width(svg_text, at);
            output.push_str(&svg_text[at..at + width]);
            at += width;
            continue;
        }
        let after = at + "fill".len();
        if let Some((replacement, end)) = black_fill_attribute(svg_text, after, color)
            .or_else(|| black_fill_style(svg_text, after, color))
        {
            output.push_str(&replacement);
            at = end;
            continue;
        }
        output.push_str(&svg_text[at..after]);
        at = after;
    }
    output
}

/// `fill=(["'])<black>\1` starting at the byte after `fill`.
fn black_fill_attribute(svg_text: &str, after: usize, color: &str) -> Option<(String, usize)> {
    let rest = &svg_text[after..];
    let quote = rest.strip_prefix('=')?.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value_at = after + '='.len_utf8() + quote.len_utf8();
    let value = &svg_text[value_at..];
    let consumed = ["#000", "#000000"]
        .iter()
        .find(|black| starts_with_ignore_case(value, black))
        .map(|black| black.len())
        .or_else(|| black_rgb_length(value))?;
    if !value[consumed..].starts_with(quote) {
        return None;
    }
    Some((
        format!("fill=\"{color}\""),
        value_at + consumed + quote.len_utf8(),
    ))
}

/// `fill:\s*<black>` starting at the byte after `fill`, with a word boundary after a hex value.
fn black_fill_style(svg_text: &str, after: usize, color: &str) -> Option<(String, usize)> {
    let rest = svg_text[after..].strip_prefix(':')?;
    let spaces = rest.len()
        - rest
            .trim_start_matches([' ', '\t', '\n', '\r', '\u{0c}'])
            .len();
    let value_at = after + ':'.len_utf8() + spaces;
    let value = &svg_text[value_at..];
    // The longer literal first, so `#000000` is not read as `#000` followed by a digit.
    for black in ["#000000", "#000"] {
        if starts_with_ignore_case(value, black)
            && value[black.len()..]
                .chars()
                .next()
                .is_none_or(|character| !character.is_alphanumeric() && character != '_')
        {
            return Some((format!("fill:{color}"), value_at + black.len()));
        }
    }
    black_rgb_length(value).map(|consumed| (format!("fill:{color}"), value_at + consumed))
}

/// The byte length of a leading `rgb( 0 , 0 , 0 )` or `rgb(16,24,32)`, whitespace allowed.
fn black_rgb_length(value: &str) -> Option<usize> {
    if !starts_with_ignore_case(value, "rgb(") {
        return None;
    }
    let close = value.find(')')?;
    let normalized: String = value[..=close]
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    (normalized.eq_ignore_ascii_case("rgb(0,0,0)")
        || normalized.eq_ignore_ascii_case("rgb(16,24,32)"))
    .then_some(close + 1)
}

fn next_char_width(value: &str, at: usize) -> usize {
    value[at..].chars().next().map_or(1, char::len_utf8)
}

fn starts_with_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack.len() >= needle.len()
        && haystack.is_char_boundary(needle.len())
        && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

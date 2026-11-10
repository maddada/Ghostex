//! How a marked section looks: the palette, the three styles, and the sample
//! reply the Settings preview draws in each.
//!
//! The file depends only on gpui and the chat core, so Settings includes it as its own module
//! (`settings_modal/tabs/general/marked_sections_preview.rs`): the Settings page also builds into
//! the standalone modal demo, which has no chat renderer to call into.
//!
//! CDXC:MarkedHandovers 2026-10-11 DECISION:
//! User picked three styles from the mockups (docs/2026-10-11/chat-section-markers/index.html): A, a thin left rule in the kind's colour with the emoji kept; C, a coloured badge naming the kind in place of the emoji, dropping a bold label that only repeats it; D, the emoji kept and the label coloured, the default. The chat colours marked sections whenever a reply has them: orchestrators and their threads mark their handovers even while Marked handovers is off (that switch extends marking to every other session). Supersedes the same day's summary strip at the top of the message (user: "let's actually just remove the summary at the top i don't think it's useful at all").
//!
//! CDXC:MarkedHandovers 2026-10-11 DECISION:
//! User: "add a toggle for emojis in the colorful handover please. and set it to off by default. and if enabled add emojis to all 3." With Show marker emojis (`markedHandoversEmoji`) off, no style shows the marker emoji and the colour and label carry the kind (a section with no label of its own shows the kind's name as its coloured label); on, all three show it, the badge style before its badge. Copying still gives the agent's own text.
//! SEE-ALSO: apps/desktop/src/app/native_chat/marked_sections.rs (the transcript's use of it), apps/mobile/app/src/chat/native/transcript/markdown/MarkedSections.tsx (the phone draws the same, with the same palette).
use gpui::{
    AnyElement, FontWeight, Hsla, IntoElement, ParentElement as _, Styled as _, div, px, rgb,
};
use serde::Deserialize;

pub(crate) const SECTION_OPEN: &str = "\u{E000}section:";
pub(crate) const SECTION_CLOSE: &str = "\u{E000}/section";

/// CDXC:MarkedHandovers 2026-11-06 DECISION:
/// User: "please make in progress color in handover headings #C68A06 like Moving is in the board". The "In progress" heading takes the board's Moving amber in both appearances (the board uses this one colour on dark and light, so there is no darker light shade); it replaces the earlier #00A6ED / #0077B6 pick of 2026-10-11.
const IN_PROGRESS_DARK: u32 = 0xc68a06;
const IN_PROGRESS_LIGHT: u32 = 0xc68a06;

/// CDXC:MarkedHandovers 2026-11-08 DECISION:
/// User: "please make Needs you pinkish color in the handover section headings, and make blocked red". "Needs you" takes the sidebar's pink question dot (`QUESTION_COLOR` #f472b6 in native_sidebar/status.rs) and "Blocked" its red dot (`FAILED_COLOR` #ef4444), on the dark chat; the light chat keeps a darker shade of each so the text stays readable on white (pink #be185d, red #dc2626). It replaces the orange #fb923c / #c2410c "Needs you" of 2026-10-12. "Kicked off" had the same pink and moved to sky blue the same day (below).
pub(super) const NEED_DARK: u32 = 0xf472b6;
pub(super) const NEED_LIGHT: u32 = 0xbe185d;
const BLOCKED_DARK: u32 = 0xef4444;
const BLOCKED_LIGHT: u32 = 0xdc2626;

/// CDXC:MarkedHandovers 2026-11-08 DECISION:
/// User: "give 'Kicked off' its own hue so it no longer matches Needs you's pink", one no other heading uses and clear of Read this's amber, Suggestions' purple and Shipped's indigo (for example a sky blue). "Kicked off" is sky blue (#38bdf8 dark, #0369a1 light): In progress is amber now, so no heading is blue, and it sits about 26 degrees from Untested's teal and 40 from Shipped's indigo.
const KICKED_DARK: u32 = 0x38bdf8;
const KICKED_LIGHT: u32 = 0x0369a1;

/// How the chat draws marked sections (`markedHandoversStyle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkerStyle {
    /// A: a thin rule in the kind's colour down the section's left edge.
    Rule,
    /// C: a pill naming the kind in place of the emoji.
    Badge,
    /// D: the emoji kept and the label coloured.
    Labels,
}

impl MarkerStyle {
    /// The style the settings ask for. It applies whether or not Marked handovers is on, because
    /// orchestrators and their threads mark their handovers either way.
    pub(crate) fn from_settings(settings: &serde_json::Map<String, serde_json::Value>) -> Self {
        Self::from_value(
            settings
                .get("markedHandoversStyle")
                .and_then(serde_json::Value::as_str),
        )
    }

    pub(crate) fn from_value(value: Option<&str>) -> Self {
        match value {
            Some("rule") => Self::Rule,
            Some("badge") => Self::Badge,
            _ => Self::Labels,
        }
    }

    /// Whether the style draws the section's first line itself instead of the line as written:
    /// the rule keeps the agent's line unless the emoji has to come out of it.
    pub(crate) fn draws_first_line(self, emoji: bool) -> bool {
        !(self == Self::Rule && emoji)
    }

    /// The first line a style that draws its own first line draws as Markdown: the rule draws the
    /// agent's line without the emoji; the others draw only their opening, so `None`. Nothing ever
    /// follows the label on its line: the core moves that text under it.
    pub(crate) fn first_line_text(self, meta: &SectionMeta) -> Option<String> {
        (self == Self::Rule && !meta.plain.is_empty()).then(|| meta.plain.clone())
    }

    /// The colour the label is drawn in when the style draws one: the kind's colour in coloured
    /// labels, the chat's foreground beside a badge. `None` when the style draws no label (the
    /// rule, a badge that only repeats its own name, a section with no label of its own).
    pub(crate) fn label_color(self, meta: &SectionMeta, look: &MarkedLook) -> Option<Hsla> {
        if meta.label.is_empty() {
            return None;
        }
        match self {
            Self::Rule => None,
            Self::Badge => (!meta.label_repeats).then_some(look.foreground),
            Self::Labels => Some(look.accent(&meta.kind)),
        }
    }
}

/// Whether marked sections show the marker emoji (`markedHandoversEmoji`, off by default).
pub(crate) fn emoji_from_settings(settings: &serde_json::Map<String, serde_json::Value>) -> bool {
    settings
        .get("markedHandoversEmoji")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// A section as the core described it (`packages/gx-chat-core/src/transcript/marked_sections.rs`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SectionMeta {
    pub(crate) kind: String,
    pub(crate) emoji: String,
    pub(crate) name: String,
    pub(crate) label: String,
    /// The label as one Markdown paragraph (a heading's label keeps its links and code spans).
    pub(crate) label_markdown: String,
    pub(crate) label_repeats: bool,
    /// The whole first line without its emoji.
    pub(crate) plain: String,
}

/// The chat tones a section is drawn against.
#[derive(Clone)]
pub(crate) struct MarkedLook {
    pub(crate) light: bool,
    pub(crate) scale: f32,
    pub(crate) foreground: Hsla,
    pub(crate) prose: Hsla,
    pub(crate) border: Hsla,
    /// Show the marker emoji (`markedHandoversEmoji`).
    pub(crate) emoji: bool,
    /// Handover heading color strength, 0 to 1: how far the kind's colour is drawn from the
    /// prose colour toward the palette (the core's `markedHeadingStrength`).
    pub(crate) strength: f32,
    /// The transcript's prose line height, unscaled.
    pub(crate) line_height: f32,
    /// The gap between Markdown blocks, scaled.
    pub(crate) block_gap: f32,
}

impl MarkedLook {
    /// The chat's default tones, for a preview drawn outside a chat.
    pub(crate) fn chat_default(light: bool, emoji: bool, strength: f32) -> Self {
        let color = |dark: u32, light_color: u32| -> Hsla {
            rgb(if light { light_color } else { dark }).into()
        };
        Self {
            light,
            scale: 1.0,
            foreground: color(0xfcfcfc, 0x27272a),
            prose: color(0xb4b8c0, 0x4d4d50),
            border: color(0x1c1c1c, 0xe4e4e7),
            emoji,
            strength,
            line_height: 22.75,
            block_gap: 12.0,
        }
    }
}

impl MarkedLook {
    /// A kind's colour at this look's heading strength.
    pub(crate) fn accent(&self, kind: &str) -> Hsla {
        blend_strength(kind_color(kind, self.light), self.prose, self.strength)
    }
}

/// `palette` drawn at `strength` (0 to 1): blended in sRGB toward `text`, so 1 is the palette
/// itself and 0 the plain text colour (CDXC:MarkedHandovers 2026-10-20 in
/// packages/gx-chat-core/src/transcript/marked_sections.rs).
pub(crate) fn blend_strength(palette: Hsla, text: Hsla, strength: f32) -> Hsla {
    let strength = strength.clamp(0.0, 1.0);
    if strength >= 1.0 {
        return palette;
    }
    let (from, to) = (gpui::Rgba::from(text), gpui::Rgba::from(palette));
    let mix = |a: f32, b: f32| a + (b - a) * strength;
    gpui::Rgba {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
    .into()
}

/// A kind's colour in the chat's light or dark appearance (the mockup's palette).
pub(crate) fn kind_color(kind: &str, light: bool) -> Hsla {
    let (dark, light_color) = match kind {
        "need" => (NEED_DARK, NEED_LIGHT),
        "done" => (0x4ade80, 0x15803d),
        "read" => (0xfbbf24, 0xb45309),
        "kicked" => (KICKED_DARK, KICKED_LIGHT),
        "progress" => (IN_PROGRESS_DARK, IN_PROGRESS_LIGHT),
        "untested" => (0x2dd4bf, 0x0f766e),
        "shipped" => (0x818cf8, 0x4f46e5),
        "blocked" => (BLOCKED_DARK, BLOCKED_LIGHT),
        "comms" => (0xa3e635, 0x4d7c0f),
        "parked" => (0x94a3b8, 0x475569),
        "held" => (0xd6a77a, 0x92400e),
        "suggestions" => (0xc084fc, 0x7e22ce),
        _ => (0x9e9e9e, 0x71717b),
    };
    rgb(if light { light_color } else { dark }).into()
}

/// The faint fill and the edge a chip or badge takes from its kind's colour.
fn tint(look: &MarkedLook) -> (f32, f32) {
    if look.light {
        (0.07, 0.28)
    } else {
        (0.11, 0.30)
    }
}

/// What the first line opens with in styles C and D: the emoji when shown, the badge (C), then the
/// label (coloured in D, plain bold in C unless it only repeats the badge). Without the emoji and
/// without a label of its own, D shows the kind's name as the label so the line keeps its colour.
///
/// The label may be long (a marked heading's label is the whole heading), so it takes the rest of
/// the row and wraps. `label_text` is the label drawn as Markdown by the caller (links and code
/// spans work); without it the label is plain text.
fn first_line_opening(
    style: MarkerStyle,
    meta: &SectionMeta,
    look: &MarkedLook,
    label_text: Option<AnyElement>,
) -> Vec<AnyElement> {
    let s = look.scale;
    let color = look.accent(&meta.kind);
    let line = px(look.line_height * s);
    let mut label_text = label_text;
    let mut label = |color: Hsla| {
        let cell = div().min_w_0().flex_1();
        match label_text.take() {
            Some(text) => cell.child(text).into_any_element(),
            None => cell
                .line_height(line)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(meta.label.clone())
                .into_any_element(),
        }
    };
    let mut parts: Vec<AnyElement> = Vec::new();
    if look.emoji {
        parts.push(
            div()
                .flex_shrink_0()
                .line_height(line)
                .child(meta.emoji.clone())
                .into_any_element(),
        );
    }
    if style == MarkerStyle::Badge {
        let (fill, edge) = tint(look);
        parts.push(
            div()
                .flex_shrink_0()
                .h(line)
                .flex()
                .items_center()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0 * s))
                        .pl(px(7.0 * s))
                        .pr(px(8.0 * s))
                        .py(px(3.0 * s))
                        .rounded_full()
                        .border_1()
                        .border_color(color.opacity(edge))
                        .bg(color.opacity(fill + 0.05))
                        .text_color(color)
                        .text_size(px(11.0 * s))
                        .line_height(px(13.0 * s))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(div().size(px(6.0 * s)).rounded_full().bg(color))
                        .child(meta.name.to_uppercase()),
                )
                .into_any_element(),
        );
        if !meta.label.is_empty() && !meta.label_repeats {
            parts.push(label(look.foreground));
        }
    } else if !meta.label.is_empty() {
        parts.push(label(color));
    } else if !look.emoji {
        parts.push(
            div()
                .flex_shrink_0()
                .line_height(line)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(format!("{}:", meta.name))
                .into_any_element(),
        );
    }
    parts
}

/// One section in `style`. `label` is the section's label drawn as Markdown in
/// `MarkerStyle::label_color` (none: it is drawn as plain text); `first` is `first_line_text` drawn
/// as Markdown (the rule's own first line); `body` is the section's lines drawn as Markdown: all of
/// them when the style does not draw its own first line, the ones after the first when it does.
pub(crate) fn section(
    style: MarkerStyle,
    meta: &SectionMeta,
    label: Option<AnyElement>,
    first: Option<AnyElement>,
    body: Vec<AnyElement>,
    look: &MarkedLook,
) -> AnyElement {
    let s = look.scale;
    let column = div()
        .relative()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .gap(px(look.block_gap));
    if style == MarkerStyle::Rule {
        return column
            .border_l(px(3.0 * s))
            .border_color(look.accent(&meta.kind))
            .pl(px(12.0 * s))
            .children(first)
            .children(body)
            .into_any_element();
    }
    let header = div()
        .flex()
        .items_start()
        .w_full()
        .min_w_0()
        .gap(px(6.0 * s))
        .text_size(px(14.0 * s))
        .text_color(look.prose)
        .children(first_line_opening(style, meta, look, label));
    column.child(header).children(body).into_any_element()
}

/// The reply the Settings preview draws in every style: the mockup's orchestrator handover.
const PREVIEW_REPLY: &str = "Here's where the three threads stand after this round.\n\n✅ **Done:**\n- The questions form draws on desktop and the web build; picks survive a reload.\n- Settings > Skills: the trash button comes first.\n\n🛫 **Kicked off:** a thread for the Windows installer.\n\n🔄 **In progress:**\n- Phone chat: the note box still loses focus when the keyboard opens.\n- Windows installer: started this turn.\n\n⚠️ **Read this:** a questions block with more than 20 questions is now refused with an error.\n\n❓ **Needs you:**\n- **Q1** Should a sent form collapse to one line? (a) collapse (b) stay open\n- **Q2** OK to ship the phone part first? (a) yes (b) wait\n\n🧪 Untested: answering from the floating page on Windows.";

/// The preview reply's marked text split at its marks: prose and sections.
enum PreviewPart {
    Prose(String),
    Section(SectionMeta, Vec<String>),
}

fn preview_parts(lines: &[&str]) -> Vec<PreviewPart> {
    let mut parts = Vec::new();
    let mut prose: Vec<&str> = Vec::new();
    let flush = |prose: &mut Vec<&str>, parts: &mut Vec<PreviewPart>| {
        if prose.iter().any(|line| !line.trim().is_empty()) {
            parts.push(PreviewPart::Prose(prose.join("\n")));
        }
        prose.clear();
    };
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if let Some(meta) = line.strip_prefix(SECTION_OPEN) {
            flush(&mut prose, &mut parts);
            let end = lines[index + 1..]
                .iter()
                .position(|line| *line == SECTION_CLOSE)
                .map_or(lines.len(), |offset| index + 1 + offset);
            parts.push(PreviewPart::Section(
                serde_json::from_str(meta).unwrap_or_default(),
                lines[index + 1..end]
                    .iter()
                    .map(|line| (*line).to_string())
                    .collect(),
            ));
            index = end;
        } else {
            prose.push(line);
        }
        index += 1;
    }
    flush(&mut prose, &mut parts);
    parts
}

/// The sample reply drawn in `style`, as the chat draws it; `text` draws a piece of Markdown.
pub(crate) fn preview_reply(
    style: MarkerStyle,
    look: &MarkedLook,
    text: impl Fn(String, String) -> AnyElement,
) -> AnyElement {
    use ghostex_gx_chat_core::transcript::{line_breaks, marked_sections, native_markdown};
    let body = line_breaks::agent_line_breaks(PREVIEW_REPLY, line_breaks::AgentLineBreaks::Every);
    let marked = marked_sections::mark_sections(native_markdown::native_markdown(&body, false));
    let lines: Vec<&str> = marked.lines().collect();
    let id = format!("marked-preview-{style:?}");
    let children = preview_parts(&lines)
        .into_iter()
        .enumerate()
        .map(|(index, part)| {
            let key = format!("{id}:{index}");
            match part {
                PreviewPart::Prose(content) => text(key, content),
                PreviewPart::Section(meta, lines) => {
                    let (first, body) = if style.draws_first_line(look.emoji) {
                        let first = style
                            .first_line_text(&meta)
                            .map(|first| text(format!("{key}:first"), first));
                        (first, lines.get(1..).unwrap_or_default().join("\n"))
                    } else {
                        (None, lines.join("\n"))
                    };
                    let body = (!body.trim().is_empty())
                        .then(|| text(format!("{key}:body"), body))
                        .into_iter()
                        .collect();
                    section(style, &meta, None, first, body, look)
                }
            }
        })
        .collect::<Vec<_>>();
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .gap(px(look.block_gap))
        .text_size(px(14.0 * look.scale))
        .text_color(look.prose)
        .children(children)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::kind_color;
    use gpui::rgb;

    /// CDXC:MarkedHandovers 2026-11-06: "In progress" is the board's Moving amber (#C68A06) in the
    /// dark and the light chat alike.
    #[test]
    fn in_progress_is_the_boards_moving_amber_in_both_appearances() {
        let amber = rgb(0xc68a06).into();
        assert_eq!(kind_color("progress", false), amber);
        assert_eq!(kind_color("progress", true), amber);
    }
}
